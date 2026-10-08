//! Player input. Input never changes the battle directly; it only sets ORDERS
//! (a unit's destination or fire target, through `Battle::order_move` / `Battle::order_fire`)
//! or the battle speed. The next model tick does the rest.
//!
//! Controls:
//! - Left click:  select one of YOUR (blue) units.
//! - Right click: on an enemy (red) unit: the selected unit fires at it (if it has muskets; it
//!   walks into range first). Anywhere else: move the selected unit there.
//! - F:           toggle "fire at will" for the selected unit.
//! - Space:       pause / unpause.
//! - Tab:         cycle speed in the original order 0 → 0.4 → 1 → 2 → 4 → 0 (CONFIRMED).
//! - R:           restart with the next RNG seed.
//! - Arrow keys pan the camera, Q / E turn it, the mouse wheel zooms (see `view::move_camera`).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_sim::battle::speed::BattleSpeed;

use super::{BattlePhase, BattleSim, setup};
use crate::data::GameData;

/// Max distance (metres) between a click and a unit's centre for the click to select it.
const SELECT_RADIUS_M: f32 = 25.0;

/// Keyboard: speed, pause and restart.
pub fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut sim: ResMut<BattleSim>,
    mut time: ResMut<Time<Virtual>>,
    data: Res<GameData>,
    setup_data: Res<setup::SetupData>,
) {
    // Harness runs ignore the live keyboard (repeatable captures, `view::camera_harness_run`).
    if super::view::camera_harness_run() {
        apply_speed(sim.speed, &mut time);
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        let seed = sim.seed.wrapping_add(1);
        *sim = setup::build_battle(&data.db, &setup_data, seed);
    }
    if keys.just_pressed(KeyCode::Space) {
        if sim.speed == BattleSpeed::Paused {
            sim.speed = sim.speed_before_pause;
        } else {
            sim.speed_before_pause = sim.speed;
            sim.speed = BattleSpeed::Paused;
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        sim.speed = sim.speed.cycle();
    }
    if keys.just_pressed(KeyCode::KeyF)
        && let Some(id) = sim.selected
        && let Some(i) = sim.battle.unit_index(id)
    {
        let unit = &mut sim.battle.units[i];
        unit.fire_at_will = !unit.fire_at_will;
        let on = unit.fire_at_will;
        sim.script_events.push(super::ScriptEvent::Command { name: "Fire At Will", unit: Some(id), bool1: on });
    }
    apply_speed(sim.speed, &mut time);
}

/// Scales Bevy's virtual clock. `FixedUpdate` (= model ticks) follows the virtual clock, so
/// ×2 simply means twice as many 0.1 s ticks per real second.
fn apply_speed(speed: BattleSpeed, time: &mut Time<Virtual>) {
    if speed == BattleSpeed::Paused {
        time.pause();
    } else {
        time.unpause();
        time.set_relative_speed(speed.multiplier());
    }
}

/// Mouse: select with the left button, order a move with the right button.
pub fn mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut sim: ResMut<BattleSim>,
    pointer: Option<Res<super::hud::HudPointer>>,
) {
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if (!left && !right) || pointer.is_some_and(|p| p.over) || super::view::camera_harness_run() {
        return;
    }
    let Some(cursor) = window.cursor_position() else { return };
    let (camera, camera_transform) = *camera;
    // Cursor ray → the terrain (or the plane y = 0 without a map) → battlefield metres
    // (see `view::world_of`).
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else { return };
    let Some(hit) = crate::terrain::pick_ground(ray) else { return };
    let point_m = Vec2::new(hit.x, -hit.z);

    if left {
        sim.selected = unit_near(&sim, point_m, |s, u| s.info_of(u.id).is_some_and(|i| i.controllable));
    }
    // Deployment: a right click inside the player's deployment area puts the selected unit there
    // at once, keeping its facing (PROVISIONAL: the original drags out a formation; the exact
    // placement rules are UNKNOWN). Clicks outside the area are refused.
    if right && sim.phase == BattlePhase::Deployment {
        if let Some(i) = sim.selected.and_then(|id| sim.battle.unit_index(id))
            && sim.deployment_area.as_ref().is_none_or(|a| setup::area_contains(a, (point_m.x, point_m.y)))
        {
            let u = &mut sim.battle.units[i];
            u.position = (point_m.x, point_m.y);
            u.destination = None;
        }
        return;
    }
    if right && let Some(id) = sim.selected {
        // Right click on an enemy: fire order (refused by the model if the unit can't shoot,
        // in which case it is treated as a move to that spot).
        let enemy = unit_near(&sim, point_m, |s, u| s.battle.unit_index(id).is_some_and(|i| s.battle.units[i].side != u.side));
        let fired = enemy.is_some_and(|enemy| sim.battle.order_fire(id, enemy));
        // The scripts hear these as the commands "Attack Unit" (about the target) and "Move".
        if fired {
            sim.script_events.push(super::ScriptEvent::Command { name: "Attack Unit", unit: enemy, bool1: false });
        } else {
            sim.battle.order_move(id, (point_m.x, point_m.y));
            sim.script_events.push(super::ScriptEvent::Command { name: "Move", unit: Some(id), bool1: false });
        }
    }
}

/// The id of the unit (with men left, accepted by `accept`) closest to `point_m`, within `SELECT_RADIUS_M`.
fn unit_near(sim: &BattleSim, point_m: Vec2, accept: impl Fn(&BattleSim, &ntw_sim::battle::model::LandUnit) -> bool) -> Option<u32> {
    sim.battle
        .units
        .iter()
        .filter(|u| u.men > 0 && accept(sim, u))
        .map(|u| (u.id, Vec2::new(u.position.0, u.position.1).distance(point_m)))
        .filter(|(_, d)| *d <= SELECT_RADIUS_M)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

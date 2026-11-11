//! Player input on the battlefield. Input never changes the battle directly; it only sets ORDERS
//! (a unit's destination or fire target, through `Battle::order_move` / `Battle::order_fire`)
//! or the battle speed. The next model tick does the rest.
//!
//! The keyboard is the original's: the battle bindings of `text/default_keys.xml` (P pause,
//! CTRL+T the speed, T fire at will, R run, W/S/A/D the camera, ESC the menu, ...), polled by the
//! battle HUD (`hud::keys`, BATTLE_FIDELITY.md §59.1). This module has the mouse and the speed:
//! - Left click:  select one of YOUR (blue) units.
//! - Right click: on an enemy (red) unit: the selected unit fires at it (if it has muskets; it
//!   walks into range first). Anywhere else: move the selected unit there, walking.
//! - Double right click: the selected unit's current move runs ([`is_double_click`]).
//! - F8, only with `NAPOLEON_BATTLE_RESTART_KEY` set (a test key, not the original's): restart
//!   with the next RNG seed.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_sim::battle::orders::MoveSpeed;
use ntw_sim::battle::speed::BattleSpeed;

use super::{BattlePhase, BattleSim, setup};
use crate::data::GameData;

/// Max distance (metres) between a click and a unit's centre for the click to select it.
const SELECT_RADIUS_M: f32 = 25.0;

/// The test restart: F8, only when this environment variable is set.
fn restart_key_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("NAPOLEON_BATTLE_RESTART_KEY").is_some())
}

/// Every frame: the battle speed onto the virtual clock, and the test restart (F8 with
/// `NAPOLEON_BATTLE_RESTART_KEY` set). The original's speed keys (P, CTRL+T) are the HUD's
/// shortcuts (`hud::keys`).
pub fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    mut sim: ResMut<BattleSim>,
    mut time: ResMut<Time<Virtual>>,
    data: Res<GameData>,
    setup_data: Res<setup::SetupData>,
) {
    // Harness runs ignore the live keyboard (repeatable captures, `view::camera_harness_run`).
    if !super::view::camera_harness_run() && restart_key_enabled() && keys.just_pressed(KeyCode::F8) {
        let seed = sim.seed.wrapping_add(1);
        *sim = setup::build_battle(&data.db, &setup_data, seed);
    }
    apply_speed(sim.speed(), &mut time);
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

/// The longest time in seconds between the two right-clicks of a double click, and the farthest
/// the cursor may move between them in window pixels. PROVISIONAL: the Windows defaults (500 ms,
/// `SM_CXDOUBLECLK` 4 px); the original's double right-click order
/// (`BCQ_MULTIPLE_SELECTION_ORDER_DBL_CLICK`) is issued from its order-mode object's double-click
/// slot (`0x005DD720`, vtable `0x0132FDD0` slot 13), and where the double click is told from two
/// clicks was not reached (BATTLE_FIDELITY.md §59).
const DOUBLE_CLICK_S: f64 = 0.5;
const DOUBLE_CLICK_PX: f32 = 4.0;

/// The last right-click that gave an order: when (real time, seconds), where (window pixels) and
/// to which selected unit.
#[derive(Clone, Copy)]
pub struct OrderClick {
    at: f64,
    pos: Vec2,
    unit: u32,
}

/// A right-click at `now` (seconds) and `pos` (window pixels), with `unit` selected, is the second
/// of a double click after the order click `prev`. A selection changed in between (a left click,
/// a unit card or a selection key) breaks it: the second click must order the unit the first one
/// did (PROVISIONAL, as the window above).
fn is_double_click(prev: Option<OrderClick>, now: f64, pos: Vec2, unit: u32) -> bool {
    prev.is_some_and(|c| c.unit == unit && now - c.at <= DOUBLE_CLICK_S && c.pos.distance(pos) <= DOUBLE_CLICK_PX)
}

/// Mouse: select with the left button, order a move with the right button, run with a double
/// right-click. `last` is the last right-click that gave an order, cleared by a left click.
pub fn mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut sim: ResMut<BattleSim>,
    pointer: Option<Res<super::hud::HudPointer>>,
    time: Res<Time<Real>>,
    mut last: Local<Option<OrderClick>>,
) {
    let left = buttons.just_pressed(MouseButton::Left);
    let right = buttons.just_pressed(MouseButton::Right);
    if left {
        // A left click between two right-clicks breaks the double click (the Windows rule the
        // PROVISIONAL window follows: a double click is two clicks of the same button).
        *last = None;
    }
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
            u.place_at((point_m.x, point_m.y), u.facing);
            u.set_destination(None);
        }
        return;
    }
    if right && let Some(id) = sim.selected {
        // A double right-click (CONFIRMED structure, `0x006D08F0` → per unit `0x0055DBA0`): the
        // unit's current move runs, if it can run; it gives no new move and raises no speed event
        // to the scripts. The same rule as the HUD's Run (`Battle::order_move_speed`).
        let now = time.elapsed_secs_f64();
        if is_double_click(last.take(), now, cursor, id) {
            sim.battle.order_move_speed(id, MoveSpeed::Run);
            return;
        }
        *last = Some(OrderClick { at: now, pos: cursor, unit: id });
        // Right click on an enemy: fire order (refused by the model if the unit can't shoot,
        // in which case it is treated as a move to that spot).
        let enemy = unit_near(&sim, point_m, |s, u| s.battle.unit_index(id).is_some_and(|i| s.battle.units[i].side != u.side));
        let fired = enemy.is_some_and(|enemy| sim.battle.order_fire(id, enemy));
        // The scripts hear these as the commands "Attack Unit" (about the target) and "Move".
        // A right-click order walks: the original builds every order of this click with run = 0
        // (`0x005DEAB0`, CONFIRMED for the move order; the attack builders get the same 0 in the
        // same argument, INFERRED). The Run button or a double click then edits the move.
        if fired {
            sim.battle.order_move_speed(id, MoveSpeed::Walk);
            sim.script_events.push(super::ScriptEvent::Command { name: "Attack Unit", unit: enemy, bool1: false });
        } else {
            sim.battle.order_move_at(id, (point_m.x, point_m.y), MoveSpeed::Walk);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A second right-click soon after and near the first is a double click; a late one, a far
    /// one or a first one is not.
    #[test]
    fn a_double_click_is_two_clicks_close_in_time_and_place() {
        let first = Some(OrderClick { at: 10.0, pos: Vec2::new(100.0, 100.0), unit: 3 });
        assert!(is_double_click(first, 10.3, Vec2::new(102.0, 101.0), 3));
        assert!(is_double_click(first, 10.5, Vec2::new(100.0, 104.0), 3));
        assert!(!is_double_click(first, 10.6, Vec2::new(100.0, 100.0), 3), "too late");
        assert!(!is_double_click(first, 10.1, Vec2::new(105.0, 100.0), 3), "too far");
        assert!(!is_double_click(None, 10.1, Vec2::new(100.0, 100.0), 3), "no first click");
    }

    /// A selection changed between the two right-clicks (a unit card or a selection key, which
    /// do not pass through the left-click reset) breaks the double click.
    #[test]
    fn a_selection_change_breaks_the_double_click() {
        let first = Some(OrderClick { at: 10.0, pos: Vec2::new(100.0, 100.0), unit: 3 });
        assert!(!is_double_click(first, 10.2, Vec2::new(100.0, 100.0), 4));
    }
}

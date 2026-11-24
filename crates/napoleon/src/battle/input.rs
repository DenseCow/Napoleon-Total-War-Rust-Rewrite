//! Player input on the battlefield. Input never changes the battle directly; it only sets ORDERS
//! (through the model's order functions: `Battle::order_move_at`, `order_attack`, ...) or the
//! battle speed. The next model tick does the rest.
//!
//! The keyboard is the original's: the battle bindings of `text/default_keys.xml` (P pause,
//! CTRL+T the speed, T fire at will, R run, W/S/A/D the camera, ESC the menu, ...), polled by the
//! battle HUD (`hud::keys`, BATTLE_FIDELITY.md §59.1). This module has the mouse and the speed.
//! The original's own help texts describe its controls (CONFIRMED from the shipped text):
//! - Left click on one of your units (or its card) selects it; CTRL + click selects several
//!   (advice `advice_levels_onscreen_text_1152775702`). [`SelectMode`] has the rules.
//! - Left drag: a selection box around units ("left-click and drag with your mouse around the
//!   ships", `battle_script_strings_string_CreativeAssembly.Naval_Tut_30`); the box is drawn by
//!   root.lua's `ShowSelectionBox` / `HideSelectionBox` (`0x005C5BB0`, `0x005D7220`).
//! - Right click on an enemy: every selected unit attacks it, in melee or with missiles by its
//!   melee mode (`0x005DEAB0` → `BCQ_MULTIPLE_SELECTION_ORDER_ATTACK_UNIT`, `Battle::order_attack`).
//! - Right click on the ground: the selection moves there, walking.
//! - Right drag: "determine the width, depth and orientation that your troops will form up in"
//!   (`battle_script_strings_string_CreativeAssembly.BT_Rich_right_click_drag`); see
//!   [`formation_line`] for what is ported.
//! - Double right click: the selection's current move runs ([`is_double_click`]).
//! - F8, only with `NAPOLEON_BATTLE_RESTART_KEY` set (a test key, not the original's): restart
//!   with the next RNG seed.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_sim::battle::orders::MoveSpeed;
use ntw_sim::battle::speed::BattleSpeed;

use super::sim::SelectMode;
use super::{BattlePhase, BattleSim, setup};
use crate::data::GameData;

/// Max distance (metres) between a click and a unit's centre for the click to select it.
const SELECT_RADIUS_M: f32 = 25.0;

/// How far (window pixels) the cursor must move with a button held for the press to be a drag
/// (a selection box or a dragged-out formation) rather than a click. PROVISIONAL: the original's
/// threshold is not traced; the Windows drag rectangle (`SM_CXDRAG`) is 4 px.
const DRAG_PX: f32 = 4.0;

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
/// to which selection (its first unit).
#[derive(Clone, Copy)]
pub(super) struct OrderClick {
    at: f64,
    pos: Vec2,
    unit: u32,
}

/// A right-click at `now` (seconds) and `pos` (window pixels), with `unit` first in the selection,
/// is the second of a double click after the order click `prev`. A selection changed in between
/// (a left click, a unit card or a selection key) breaks it: the second click must order the units
/// the first one did (PROVISIONAL, as the window above).
fn is_double_click(prev: Option<OrderClick>, now: f64, pos: Vec2, unit: u32) -> bool {
    prev.is_some_and(|c| c.unit == unit && now - c.at <= DOUBLE_CLICK_S && c.pos.distance(pos) <= DOUBLE_CLICK_PX)
}

/// A mouse button held down on the battlefield: where it went down (window pixels and, for the
/// right button, the ground point in battlefield metres) and whether CTRL was held then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Press {
    pub start: Vec2,
    pub ground: Option<Vec2>,
    pub ctrl: bool,
}

/// The presses in progress, read by the HUD to show the selection box ([`MouseDrag::box_corners`]).
#[derive(Resource, Default, Debug)]
pub struct MouseDrag {
    pub left: Option<Press>,
    pub right: Option<Press>,
    /// The cursor this frame (window pixels).
    pub cursor: Option<Vec2>,
}

impl MouseDrag {
    /// The selection box's two corners (window pixels) while the left button is dragged.
    pub fn box_corners(&self) -> Option<(Vec2, Vec2)> {
        let (p, c) = (self.left?, self.cursor?);
        (p.start.distance(c) > DRAG_PX).then_some((p.start, c))
    }
}

/// Leaving a battle: no press carries over into the next one.
pub fn reset_drag(mut drag: ResMut<MouseDrag>) {
    *drag = MouseDrag::default();
}

/// Mouse: select with the left button (click or box), order with the right button (attack, move,
/// dragged-out line), run with a double right-click. `last` is the last right-click that gave an
/// order, cleared by any left click.
#[allow(clippy::too_many_arguments)]
pub fn mouse(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut sim: ResMut<BattleSim>,
    pointer: Option<Res<super::hud::HudPointer>>,
    time: Res<Time<Real>>,
    mut drag: ResMut<MouseDrag>,
    mut last: Local<Option<OrderClick>>,
) {
    if super::view::camera_harness_run() {
        return;
    }
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let over_hud = pointer.is_some_and(|p| p.over);
    let cursor = window.cursor_position();
    drag.cursor = cursor;
    let (camera, camera_transform) = *camera;
    let ground_at = |p: Vec2| -> Option<Vec2> {
        let ray = camera.viewport_to_world(camera_transform, p).ok()?;
        let hit = crate::terrain::pick_ground(ray)?;
        Some(Vec2::new(hit.x, -hit.z))
    };

    // A press whose release was missed (the button let go while the battle was not updating, or
    // outside the window) is dropped.
    if !buttons.pressed(MouseButton::Left) && !buttons.just_released(MouseButton::Left) {
        drag.left = None;
    }
    if !buttons.pressed(MouseButton::Right) && !buttons.just_released(MouseButton::Right) {
        drag.right = None;
    }
    // Presses start only on the battlefield, not on the HUD; a release over the HUD cancels them.
    if let Some(c) = cursor.filter(|_| !over_hud) {
        if buttons.just_pressed(MouseButton::Left) {
            drag.left = Some(Press { start: c, ground: None, ctrl });
        }
        if buttons.just_pressed(MouseButton::Right) {
            drag.right = Some(Press { start: c, ground: ground_at(c), ctrl });
        }
    }

    if buttons.just_released(MouseButton::Left)
        && let Some(press) = drag.left.take()
    {
        // A left click between two right-clicks breaks the double click (the Windows rule the
        // PROVISIONAL window follows: a double click is two clicks of the same button).
        *last = None;
        let end = cursor.unwrap_or(press.start);
        if over_hud {
            // Released over the HUD: the HUD's own click, not the battlefield's.
        } else if press.start.distance(end) > DRAG_PX {
            let ids = units_in_box(&sim, press.start, end, |p| camera.world_to_viewport(camera_transform, p).ok());
            sim.select(&ids, if press.ctrl { SelectMode::Add } else { SelectMode::Replace });
        } else if let Some(point) = ground_at(end) {
            let hit = unit_near(&sim, point, |s, u| s.info_of(u.id).is_some_and(|i| i.controllable));
            match (hit, press.ctrl) {
                (Some(id), true) => sim.select(&[id], SelectMode::Toggle),
                (Some(id), false) => sim.select(&[id], SelectMode::Replace),
                (None, true) => {}
                (None, false) => sim.select(&[], SelectMode::Replace),
            }
        }
    }

    if buttons.just_released(MouseButton::Right)
        && let Some(press) = drag.right.take()
        && !over_hud
    {
        let end = cursor.unwrap_or(press.start);
        let Some(end_ground) = ground_at(end) else { return };
        let ids = sim.selection().to_vec();
        let Some(&first) = ids.first() else { return };
        if sim.phase() == BattlePhase::Deployment {
            deploy(&mut sim, &ids, end_ground);
            return;
        }
        let now = time.elapsed_secs_f64();
        let dragged = press.start.distance(end) > DRAG_PX;
        if !dragged && is_double_click(last.take(), now, end, first) {
            // A double right-click (CONFIRMED structure, `0x006D08F0` → per unit `0x0055DBA0`): the
            // current moves run, if they can; no new move and no speed event to the scripts. The
            // same rule as the HUD's Run (`Battle::order_move_speed`).
            for &id in &ids {
                sim.battle.order_move_speed(id, MoveSpeed::Run);
            }
            return;
        }
        if let (true, Some(start_ground)) = (dragged, press.ground) {
            // A drag is no first click of a double click.
            order_formation(&mut sim, &ids, start_ground, end_ground, camera_transform.forward().as_vec3());
            return;
        }
        *last = Some(OrderClick { at: now, pos: end, unit: first });
        // A right click on an enemy: the attack order on it (`0x005DEAB0`; the scripts hear the
        // command "Attack Unit" about the target, once per order).
        let enemy = unit_near(&sim, end_ground, |s, u| s.battle.unit_index(first).is_some_and(|i| s.battle.units[i].side != u.side));
        if let Some(target) = enemy {
            // A right-click order walks: the original builds every order of this click with run = 0
            // (`0x005DEAB0`, CONFIRMED for the move order; the attack builders get the same 0 in the
            // same argument, INFERRED). The Run button or a double click then edits the move.
            let mut any = false;
            for &id in &ids {
                if sim.battle.order_attack(id, target) {
                    sim.battle.order_move_speed(id, MoveSpeed::Walk);
                    any = true;
                }
            }
            if any {
                sim.script_events.push(super::ScriptEvent::Command { name: "Attack Unit", unit: Some(target), bool1: false });
            }
            return;
        }
        order_group_move(&mut sim, &ids, end_ground);
    }
}

/// Deployment: a right click inside the player's deployment area puts the selection there at
/// once, keeping each unit's place relative to the others and its facing (PROVISIONAL: the
/// original drags out a formation; the exact placement rules are UNKNOWN). A unit whose place
/// would be outside the area stays where it is.
fn deploy(sim: &mut BattleSim, ids: &[u32], point: Vec2) {
    for (id, dest) in group_destinations(sim, ids, point) {
        if sim.deployment_area.as_ref().is_some_and(|a| !setup::area_contains(a, (dest.x, dest.y))) {
            continue;
        }
        if let Some(i) = sim.battle.unit_index(id) {
            let u = &mut sim.battle.units[i];
            u.place_at((dest.x, dest.y), u.facing);
            u.set_destination(None);
        }
    }
}

/// A right click on the ground: the selection walks there, each unit keeping its place relative
/// to the others (INFERRED: the original's click without a drag builds the order from the
/// selection's current formation, `0x005C4030` with its byte argument set, which copies the
/// stored shape instead of laying out a new line; PROVISIONAL: the shape is moved, not turned).
fn order_group_move(sim: &mut BattleSim, ids: &[u32], point: Vec2) {
    for (id, dest) in group_destinations(sim, ids, point) {
        // The scripts hear these as the command "Move" (a right-click order walks, see above).
        if sim.battle.order_move_at(id, (dest.x, dest.y), MoveSpeed::Walk) {
            sim.script_events.push(super::ScriptEvent::Command { name: "Move", unit: Some(id), bool1: false });
        }
    }
}

/// Each selected unit's destination for a group move to `point`: the group's centre goes to the
/// point and every unit keeps its offset from that centre. Units gone from the field are left out.
fn group_destinations(sim: &BattleSim, ids: &[u32], point: Vec2) -> Vec<(u32, Vec2)> {
    let positions: Vec<(u32, Vec2)> = ids
        .iter()
        .filter_map(|&id| sim.battle.unit_index(id).map(|i| (id, &sim.battle.units[i])))
        .filter(|(_, u)| super::sim::on_field(u))
        .map(|(id, u)| (id, Vec2::from(u.position)))
        .collect();
    if positions.is_empty() {
        return Vec::new();
    }
    let centre = positions.iter().map(|(_, p)| *p).sum::<Vec2>() / positions.len() as f32;
    positions.into_iter().map(|(id, p)| (id, point + (p - centre))).collect()
}

/// A right drag from `start` to `end` (battlefield metres): the selection forms up along that line
/// ([`formation_line`]) and each unit walks to its place and turns to face the line's front.
fn order_formation(sim: &mut BattleSim, ids: &[u32], start: Vec2, end: Vec2, camera_forward: Vec3) {
    let widths: Vec<(u32, f32)> = ids
        .iter()
        .filter(|&&id| sim.battle.unit_index(id).is_some_and(|i| super::sim::on_field(&sim.battle.units[i])))
        .filter_map(|&id| sim.info_of(id).map(|i| (id, i.size_m.x.max(1.0))))
        .collect();
    // The battlefield's y axis is the world's −z.
    let away = Vec2::new(camera_forward.x, -camera_forward.z);
    for (id, place, facing) in formation_line(&widths, start, end, away) {
        if sim.battle.order_move_formed(id, (place.x, place.y), facing, MoveSpeed::Walk) {
            sim.script_events.push(super::ScriptEvent::Command { name: "Move", unit: Some(id), bool1: false });
        }
    }
}

/// The gap in metres between two units of a dragged-out line when the drag is shorter than the
/// units' fronts. PROVISIONAL (the original's spacing rules are in its formation code, `0x005C4030`
/// → `0x007FE9C0` / `0x007FECA0`, with the selection's minimum and maximum width from `0x006D9280`
/// / `0x006D8E90`; not ported).
const LINE_GAP_M: f32 = 2.0;

/// The places of a dragged-out line: the units side by side from `start` towards `end` in
/// selection order, each `(id, front width)`, spread evenly along the drag (at least
/// [`LINE_GAP_M`] apart), all facing the side of the line that `away` (the camera's view
/// direction on the ground) points to. Returns `(id, centre, facing)` per unit, the facing in the
/// model's radians. PROVISIONAL: the original's formation code also sets the units' width and depth
/// from the drag (`0x005C44C0` clamps the drag length between the selection's minimum and maximum
/// width); ours keeps each unit's own width and depth.
pub fn formation_line(units: &[(u32, f32)], start: Vec2, end: Vec2, away: Vec2) -> Vec<(u32, Vec2, f32)> {
    if units.is_empty() {
        return Vec::new();
    }
    let along = (end - start).try_normalize().unwrap_or(Vec2::X);
    // The front: the line's normal on the camera's far side.
    let mut front = along.perp();
    if front.dot(away) < 0.0 {
        front = -front;
    }
    let facing = front.y.atan2(front.x);
    let fronts: f32 = units.iter().map(|(_, w)| w).sum();
    let gaps = (units.len() - 1) as f32;
    let gap = if gaps > 0.0 { ((start.distance(end) - fronts) / gaps).max(LINE_GAP_M) } else { 0.0 };
    let mut at = 0.0;
    units
        .iter()
        .map(|&(id, w)| {
            let centre = start + along * (at + w * 0.5);
            at += w + gap;
            (id, centre, facing)
        })
        .collect()
}

/// The controllable units whose centre the camera shows inside the box from `a` to `b` (window
/// pixels). PROVISIONAL: the original's box test (`0x005D7220` → `0x005E0750`) is not traced; the
/// unit's centre on the ground stands in for its men.
fn units_in_box(sim: &BattleSim, a: Vec2, b: Vec2, to_screen: impl Fn(Vec3) -> Option<Vec2>) -> Vec<u32> {
    let (lo, hi) = (a.min(b), a.max(b));
    sim.battle
        .units
        .iter()
        .filter(|u| super::sim::on_field(u) && sim.info_of(u.id).is_some_and(|i| i.controllable))
        .filter(|u| {
            to_screen(super::view::world_of(Vec2::new(u.position.0, u.position.1)))
                .is_some_and(|p| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y)
        })
        .map(|u| u.id)
        .collect()
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

    /// A selection changed between the two right-clicks breaks the double click: also one made by
    /// a key or a script (`BattleUiRequest::Select`), which does not pass through the left-click
    /// reset in `mouse` (a unit card clicked with the mouse is a left press, so it does).
    #[test]
    fn a_selection_change_breaks_the_double_click() {
        let first = Some(OrderClick { at: 10.0, pos: Vec2::new(100.0, 100.0), unit: 3 });
        assert!(!is_double_click(first, 10.2, Vec2::new(100.0, 100.0), 4));
    }

    /// A drag longer than the fronts spreads the units evenly from its start; all face the far side.
    #[test]
    fn a_dragged_line_spreads_the_units_along_the_drag() {
        let units = [(1, 20.0), (2, 20.0), (3, 20.0)];
        let line = formation_line(&units, Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(0.0, 1.0));
        let centres: Vec<Vec2> = line.iter().map(|l| l.1).collect();
        assert_eq!(centres, [Vec2::new(10.0, 0.0), Vec2::new(50.0, 0.0), Vec2::new(90.0, 0.0)]);
        assert!(line.iter().all(|l| (l.2 - std::f32::consts::FRAC_PI_2).abs() < 1e-6), "face +y, away from the camera");
        let towards = formation_line(&units, Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(0.0, -1.0));
        assert!((towards[0].2 + std::f32::consts::FRAC_PI_2).abs() < 1e-6, "the camera's far side is −y");
    }

    /// A drag shorter than the fronts keeps the units side by side, the minimum gap apart.
    #[test]
    fn a_short_drag_keeps_the_units_side_by_side() {
        let line = formation_line(&[(1, 10.0), (2, 30.0)], Vec2::ZERO, Vec2::new(0.0, 5.0), Vec2::X);
        assert_eq!(line[0].1, Vec2::new(0.0, 5.0));
        assert_eq!(line[1].1, Vec2::new(0.0, 10.0 + LINE_GAP_M + 15.0));
        assert!((line[0].2 - 0.0).abs() < 1e-6, "faces +x");
        assert_eq!(formation_line(&[], Vec2::ZERO, Vec2::X, Vec2::Y), []);
    }
}

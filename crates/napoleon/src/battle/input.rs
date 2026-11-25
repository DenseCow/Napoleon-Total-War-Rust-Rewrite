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
    /// The unit under the mouse this frame (any side, on the battlefield, not over the HUD): its
    /// men show the mouse-over marker (`selection_markers`).
    pub hovered: Option<u32>,
    /// While the right button drags out a line in the battle: the places the release would order
    /// ([`formation_plan`]), shown by [`order_preview`]; empty otherwise.
    pub preview: Vec<LinePlace>,
    /// What [`Self::preview`] was laid out for: the drag's ground line, the camera's view
    /// direction and the model tick, and the selection then.
    preview_for: Option<(Vec2, Vec2, Vec3, u32)>,
    preview_ids: Vec<u32>,
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
    // The unit under the mouse (the original marks the hovered soldier's unit, `0x00789BD0`;
    // ours picks as a click does, PROVISIONAL).
    drag.hovered = cursor
        .filter(|_| !over_hud)
        .and_then(ground_at)
        .and_then(|p| unit_near(&sim, p, |_, u| super::sim::on_field(u)));

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
    // The line the right drag would order, while it is held (not in deployment, which places).
    // Laid out again only when the drag's line, the view, the model (a tick: the units moved or
    // lost men) or the selection changed.
    let line = if buttons.pressed(MouseButton::Right)
        && sim.phase() != BattlePhase::Deployment
        && !over_hud
        && let (Some(press), Some(c)) = (drag.right, cursor)
        && press.start.distance(c) > DRAG_PX
        && let (Some(start), Some(end)) = (press.ground, ground_at(c))
    {
        Some((start, end, camera_transform.forward().as_vec3(), sim.battle.tick))
    } else {
        None
    };
    if line != drag.preview_for || drag.preview_ids != sim.selection() {
        drag.preview_for = line;
        drag.preview_ids.clear();
        drag.preview_ids.extend_from_slice(sim.selection());
        drag.preview.clear();
        if let Some((start, end, forward, _)) = line {
            let plan = formation_plan(&sim, sim.selection(), start, end, forward);
            drag.preview.extend(plan);
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

/// A unit's place in a dragged-out line: `(id, centre, facing, new width)` ([`formation_line`]).
pub type LinePlace = (u32, Vec2, f32, Option<f32>);

/// A right drag from `start` to `end` (battlefield metres): the selection forms up along that line
/// ([`formation_line`]) and each unit walks to its place and turns to face the line's front. One
/// unit alone takes the drag's length as its new width (clamped to its limits,
/// [`ntw_sim::battle::model::LandUnit::drag_width_limits`]); several keep their own widths.
fn order_formation(sim: &mut BattleSim, ids: &[u32], start: Vec2, end: Vec2, camera_forward: Vec3) {
    for (id, place, facing, width) in formation_plan(sim, ids, start, end, camera_forward) {
        if sim.battle.order_move_formed(id, (place.x, place.y), facing, MoveSpeed::Walk, width) {
            sim.script_events.push(super::ScriptEvent::Command { name: "Move", unit: Some(id), bool1: false });
        }
    }
}

/// The places [`order_formation`] orders the selection to, and the drag's preview shows
/// ([`order_preview`]).
fn formation_plan(sim: &BattleSim, ids: &[u32], start: Vec2, end: Vec2, camera_forward: Vec3) -> Vec<LinePlace> {
    let units: Vec<LineUnit> = ids
        .iter()
        .filter_map(|&id| sim.battle.unit_index(id).map(|i| (id, &sim.battle.units[i])))
        .filter(|(id, u)| super::sim::on_field(u) && sim.info_of(*id).is_some())
        // The width each forms to on arrival (its line, `LandUnit::formed_width`). Only a unit
        // selected alone may be dragged to a new width (`0x00685BD0` branches on the selection).
        .map(|(id, u)| LineUnit {
            id,
            width: u.formed_width().max(1.0),
            position: Vec2::from(u.position),
            limits: if ids.len() == 1 { u.drag_width_limits() } else { None },
        })
        .collect();
    // The battlefield's y axis is the world's −z.
    let away = Vec2::new(camera_forward.x, -camera_forward.z);
    // The span a unit forms to for a requested width (its whole files at its spacing).
    let battle = &sim.battle;
    let span = |id: u32, w: f32| {
        battle.unit_index(id).map_or(w, |i| {
            let u = &battle.units[i];
            u.span_of_files(u.files_for_width(w))
        })
    };
    formation_line(&units, start, end, away, &span)
}

/// While the right button drags out a line: a marker on the ground at every man's place in it
/// (the `entity_selection_marker` square, `selection_markers`). PROVISIONAL (ours): the
/// original shows the dragged line with its proxy models (`infantry_proxy`, `cavalry_proxy`,
/// `artillery_proxy`, `facing_arrow_proxy`, loaded by `0x0059D010`), whose placement is not traced
/// yet; ours shows the line each man is ordered to, from the same layout the order uses
/// ([`formation_plan`], `LandUnit::line_at_width`).
pub fn order_preview(sim: Res<BattleSim>, drag: Res<MouseDrag>, quads: Option<ResMut<super::selection_markers::MarkerQuads>>) {
    let Some(mut quads) = quads else { return };
    for &(id, centre, facing, width) in &drag.preview {
        let Some(u) = sim.battle.unit_index(id).map(|i| &sim.battle.units[i]) else { continue };
        for p in line_places(u, centre, facing, width) {
            quads.push(super::selection_markers::Marker::Selected, super::view::world_of(p), facing);
        }
    }
}

/// Where each man of `u` stands (battlefield metres) once formed at `centre` facing `facing`, at
/// the dragged `width` (`None`: his own formation).
fn line_places(u: &ntw_sim::battle::model::LandUnit, centre: Vec2, facing: f32, width: Option<f32>) -> impl Iterator<Item = Vec2> + '_ {
    let line = width.map_or_else(|| u.formation(), |w| u.line_at_width(w));
    (0..u.men as usize).map(move |i| {
        let (x, y) = ntw_sim::battle::march::unit_frame_offset(facing, line.place(i));
        centre + Vec2::new(x, y)
    })
}

/// The metres between two units of a dragged-out line: the minimum width of several units adds
/// 5 m per unit (`0x006879A0`, `0x40A00000`, CONFIRMED static), and the layout `0x00685BD0` adds its
/// gap argument after each unit; that the two are the same 5 m is INFERRED.
const LINE_GAP_M: f32 = 5.0;

/// A unit of a dragged-out line: its front width now, where it stands, and the widths it may be
/// dragged out to (`None`: it keeps its width).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineUnit {
    pub id: u32,
    pub width: f32,
    pub position: Vec2,
    pub limits: Option<(f32, f32)>,
}

/// The places of a dragged-out line (the release's layout, `0x00685BD0`, CONFIRMED by debugger,
/// BATTLE_FIDELITY.md §59.6), all facing the side of the line that `away` (the camera's view
/// direction on the ground) points to. Returns `(id, centre, facing, new width)` per unit, the
/// facing in the model's radians.
/// - One unit: the drag's length clamped between its limits (`0x005C4030`: a drag shorter or
///   longer is laid at that length along the drag) is its new width (`0x0068E610`), centred on
///   the line from `start`.
/// - Several: sorted by where they stand along the drag (`0x00682BA0`) and laid side by side
///   from `start`, each keeping its width, [`LINE_GAP_M`] apart (`0x0068E5C0`).
pub fn formation_line(units: &[LineUnit], start: Vec2, end: Vec2, away: Vec2, span: &dyn Fn(u32, f32) -> f32) -> Vec<LinePlace> {
    let along = (end - start).try_normalize().unwrap_or(Vec2::X);
    // The front: the line's normal on the camera's far side.
    let mut front = along.perp();
    if front.dot(away) < 0.0 {
        front = -front;
    }
    let facing = front.y.atan2(front.x);
    if let [u] = units {
        let width = u.limits.map(|(lo, hi)| start.distance(end).clamp(lo, hi));
        // Centred on the span the unit forms to (`span`: its whole files), from the drag's start.
        let centre = start + along * (width.map_or(u.width, |w| span(u.id, w)) * 0.5);
        return vec![(u.id, centre, facing, width)];
    }
    let mut sorted: Vec<&LineUnit> = units.iter().collect();
    sorted.sort_by(|a, b| (a.position - start).dot(along).total_cmp(&(b.position - start).dot(along)));
    let mut at = 0.0;
    sorted
        .into_iter()
        .map(|u| {
            let centre = start + along * (at + u.width * 0.5);
            at += u.width + LINE_GAP_M;
            (u.id, centre, facing, None)
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

    fn unit(id: u32, width: f32, x: f32, limits: Option<(f32, f32)>) -> LineUnit {
        LineUnit { id, width, position: Vec2::new(x, 0.0), limits }
    }

    /// A unit that forms to exactly the width asked.
    fn exact(_: u32, w: f32) -> f32 {
        w
    }

    /// Several units: side by side from the drag's start in the order they stand along it, each
    /// keeping its width, [`LINE_GAP_M`] apart; all face the camera's far side.
    #[test]
    fn a_dragged_line_lays_the_units_side_by_side_in_their_order_along_it() {
        let units = [unit(1, 20.0, 80.0, Some((5.0, 60.0))), unit(2, 10.0, 0.0, None), unit(3, 20.0, 40.0, None)];
        let line = formation_line(&units, Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(0.0, 1.0), &exact);
        let placed: Vec<(u32, Vec2, Option<f32>)> = line.iter().map(|l| (l.0, l.1, l.3)).collect();
        assert_eq!(
            placed,
            [
                (2, Vec2::new(5.0, 0.0), None),
                (3, Vec2::new(10.0 + LINE_GAP_M + 10.0, 0.0), None),
                (1, Vec2::new(10.0 + LINE_GAP_M + 20.0 + LINE_GAP_M + 10.0, 0.0), None),
            ],
            "sorted along the drag, widths kept"
        );
        assert!(line.iter().all(|l| (l.2 - std::f32::consts::FRAC_PI_2).abs() < 1e-6), "face +y, away from the camera");
        let towards = formation_line(&units, Vec2::ZERO, Vec2::new(100.0, 0.0), Vec2::new(0.0, -1.0), &exact);
        assert!((towards[0].2 + std::f32::consts::FRAC_PI_2).abs() < 1e-6, "the camera's far side is −y");
        assert_eq!(formation_line(&[], Vec2::ZERO, Vec2::X, Vec2::Y, &exact), []);
    }

    /// One unit: the drag's length, clamped to its limits, is its new width, centred on the line
    /// from the drag's start; a unit without limits (artillery) keeps its width.
    #[test]
    fn one_dragged_unit_takes_the_drags_length_as_its_width() {
        let one = |len: f32, limits| formation_line(&[unit(7, 20.0, 0.0, limits)], Vec2::ZERO, Vec2::new(0.0, len), Vec2::X, &exact)[0];
        assert_eq!((one(30.0, Some((8.0, 60.0))).1, one(30.0, Some((8.0, 60.0))).3), (Vec2::new(0.0, 15.0), Some(30.0)));
        assert_eq!(one(3.0, Some((8.0, 60.0))).3, Some(8.0), "a short drag: the narrowest");
        assert_eq!(one(90.0, Some((8.0, 60.0))).3, Some(60.0), "a long drag: the widest");
        assert_eq!((one(90.0, None).1, one(90.0, None).3), (Vec2::new(0.0, 10.0), None), "artillery keeps its width");
        assert!((one(30.0, None).2 - 0.0).abs() < 1e-6, "faces +x");
    }

    /// The user's report (2026-11-25): no preview while dragging. The preview puts every man on
    /// the dragged line: as wide as the span the order forms to, centred on its place, with the
    /// front rank on the front side.
    #[test]
    fn the_drag_preview_shows_every_man_on_the_dragged_line() {
        let mut u = ntw_sim::battle::model::LandUnit::new(0, 0, 120, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_file_spacing, u.man_radius) = (40, 40.0, 1.0, 0.25);
        // Facing +x: the line runs along y.
        let (centre, facing) = (Vec2::new(50.0, 20.0), 0.0);
        let men: Vec<Vec2> = line_places(&u, centre, facing, Some(30.0)).collect();
        assert_eq!(men.len(), 120);
        let (lo, hi) = men.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        let span = u.span_of_files(u.files_for_width(30.0));
        assert!((hi - lo + 2.0 * u.man_radius - span).abs() < 1e-3, "{lo}..{hi} vs span {span}");
        assert!(((hi + lo) * 0.5 - centre.y).abs() < 0.6, "centred: {lo}..{hi}");
        // The front rank stands farthest along the facing.
        let front = men[..30].iter().map(|p| p.x).fold(f32::MIN, f32::max);
        assert!(men[30..].iter().all(|p| p.x < front), "the first rank is the front");
    }
}

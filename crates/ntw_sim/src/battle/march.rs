//! Each man's own timed move in his unit's march (UNITS_TERRAIN_FIDELITY.md §1.10 "Each man's
//! timed move").
//!
//! How the exe moves the men of a marching unit (CONFIRMED, `0x00584E10`, the unit move order's
//! issue, run when the order is made `0x0051A8FD` and re-run about every 11 ticks by `0x00581B81`
//! ([`timed_move_reissue_due`]); these are its only two callers):
//! - every tick the order's path cursor is set to the reference soldier's place on the path
//!   (`0x00531A70` → `0x007F05F0`: the nearest point of the path, never back on its segment);
//! - at each issue the cursor is copied and carried `D` metres further along the path (`0x007F0790`),
//!   with `D = v · t` from `0x0054C650` (v the gait speed `0x00565030`, t the planned time,
//!   [`TIMED_MOVE_PLANNED_SECONDS`]), stopping at the path's end for our orders ([`issue_point`]);
//!   the formation is laid out at that point (the layout object at engine `+0x628`, vfunc `+0x24`),
//!   which gives every man his own place;
//! - the issued men get the same timed-move order: his place there, the speed v and the time t
//!   (descriptor words 0-4, 8, 9 to `0x006533E0`). There is no random spread and no start delay:
//!   no generator is reachable from the issue (to depth 5). So each man's order speed is d / (t − e)
//!   ([`TimedMove::speed_for`]) with d his own distance to his own place, and his speed is that times
//!   his own multiplier `+0x1A4` (the ground under him, the slope ahead of him, fatigue) with his own
//!   acceleration step ([`step_speed`]). A man on slower ground falls behind, his d grows, and the
//!   next issue (d = D + his lag) speeds him up: the men's speeds, and so their gait clips, differ
//!   man by man and change at different moments, not all at once at the unit's re-plan;
//! - which men are issued ([`reissued`]): every man at the first issue (argument 1); at a re-issue
//!   (argument 0) a man whose timed move still runs, and a finished man only while the issue point is
//!   short of the path's end and `0x00658C20` holds for him;
//! - the order lasts until every man's move has finished: its done flag `+0xC` is `0x00531B70`, true
//!   when `0x0063E290` holds for every soldier (no action order, or his order's `+0x10` = 1, set by
//!   the update `0x008056D0` once his locomotive is done). Nothing is issued at the reference
//!   soldier's stop: the men keep the order in force, whose point is already their place at the
//!   path's end (the issue point reaches the end at least D − 1.1 v before the stop), and the
//!   re-issues go on for the men still walking.
//!
//! Ours: the unit's own move ([`LandUnit::timed_move`]) is the reference soldier's, and each man
//! keeps his offset from his slot in the formation ([`ManMarch::lag`] behind it, [`ManMarch::side`]
//! to its right); his place at the issue point is the issue point's distance ahead of his slot now,
//! so his d at an issue is his distance to that, and he walks straight to it. Once the unit's move
//! ends the order goes on ([`MenMarch::plan`]), re-issued on the same schedule with the issue point
//! where the unit stands (D = 0, his d is his distance to his slot), until every man has finished:
//! reached his slot, or braked to a stop for it ([`brakes_for`]) as the exe's stop state does.
//!
//! The slots (CONFIRMED static, [`LandUnit::relay`]): each lay-out leaves the men where they stand
//! and gives every man a slot by [`assign_slots`] (`0x005373F0`), the nearest by the cost
//! [`slot_cost`]: in full at a re-slot or an order behind the front, else only the one worst-placed
//! man may swap. A standing unit reforms in its idle state: the reform state's entry, when the
//! gate holds ([`LandUnit::idle_gate`], `0x00573770`: a new idle task, or men not found in place
//! within the last 20 ticks), sends each man farther than 0.25 m from his slot there in a plain
//! move, a run beyond 5 m ([`LandUnit::send_men_to_slots`], the reform state `0x005736B0`), and its
//! end stamps the men in place; the idle update's check every 10 ticks ([`LandUnit::in_place_check`])
//! keeps the stamp while they stand within the drill tolerance and clears it once they do not.
//!
//! PROVISIONAL (ours, until traced):
//! - the reference soldier: ours is the formation's centre; the exe's is the man the layout names
//!   (order `+0x30`, raised to `(unit +0x510) +0x27C` by `0x00572B00`), and the layout vfunc is not
//!   traced;
//! - the places: the grid is the exe's (files the front rank capped at the men, ranks
//!   `ceil(men / files)`, CONFIRMED, [`LandUnit::formation_grid`]), the place within a rank, the
//!   rank pitch and the anchor are ours ([`formation_place`]); a man's lag is along the unit's
//!   facing only (ours moves straight, so the places do not wheel);
//! - a move that follows an enemy (the advance, the fire-target approach) is a new order, a first
//!   issue to every man, whenever the enemy's position changed (`0x0054D9F0` re-reads it each tick
//!   and builds a new move order, `0x0051A940` → `0x0051A530`, on a change, CONFIRMED); the value it
//!   compares (an int at its `+0x38` against the target's vfunc `+4`) is not traced, so how far the
//!   enemy must move is ours (any change);
//! - after a braked stop short of the destination (at most the 0.5 m minimum braking distance),
//!   ours lays the men's places where the unit stands; the exe's stay at the path's end;
//! - the cursor's extra step of v at an issue when the reference soldier is within D / 4 of his place
//!   or `0x00658C20` holds (`0x005856F2`): ours carries the issue point D ahead of the unit, so the
//!   reference soldier is always D from his place at an issue (the first test never holds);
//! - the re-issue gate's other parts: a man whose move still runs is skipped when he started an
//!   action this tick of a kind table `0x00811890` maps to true (1, 2, 13, 14, 17..20; their
//!   meanings are not traced): ours has no per-man actions. `0x00658C20` holds when his slot's order
//!   `+0x1C` is 1, which `0x008056D0` sets from his locomotive's result (`+0x18` 1 → 1, 0 → 0, at
//!   the move's end); what the results mean is not traced, and ours takes a finished man's as 1. It
//!   matters only while the issue point is short of the path's end, where no man of ours finishes;
//! - an issued man gets the timed move (`0x006533E0`) only when `0x005516D0` on the cursor copy is
//!   false and his slot allows it; otherwise the order's alternative (`0x0057AC10`, soldier vfunc
//!   `+0xB8`) or `0x006527B0` (via `0x0081C080`): neither traced, ours always gives the timed move;
//! - a man ahead of his slot when the move ends (on faster ground or a steeper downhill than the
//!   reference soldier's) walks back to it facing the unit's facing; the exe's soldier turns to
//!   walk there, a turn ours does not draw;
//! - a march in which no man moved over a tick after the unit's move ended (a man on impassable
//!   ground) ends there; the exe's order would wait for him;
//! - the reform's gate: ours treats every standing unit as idle (the exe's firing and fighting
//!   units run their own task states, and the fire-at-target group's reform entry `0x005732B0` is
//!   not traced) and has neither veto of `0x00573770` (no unit controllers, vfunc `+0xB4`; the
//!   reform's moves are issued at once, `0x0057ABD0`); the in-place check counts every living man
//!   (the exe's only those whose soldier `+0x1A8` is clear) and runs on the unit's id's ticks
//!   ([`LandUnit::serial`]);
//! - the slots of a lay-out are laid at the unit's position (`0x00576B30` lays them at the order's
//!   path cursor, a point the reference soldier's place sets), and their assignment has no special
//!   soldiers (`0x005373F0` first pairs, in fixed slots, the soldiers whose vfunc `+0xF4` object
//!   has `+0xA8` set, and a soldier the layout names; neither is traced).

use super::TICK_SECONDS;
use super::abilities::dir16;
use super::model::{Battle, LandUnit, TimedMove, brakes_for, slope_multiplier, step_speed};
#[cfg(doc)]
use super::model::{TIMED_MOVE_PLANNED_SECONDS, issue_point, timed_move_reissue_due};

/// One man's state in his unit's march.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ManMarch {
    /// Metres he is behind his place in the formation, along the unit's facing (negative: ahead).
    pub lag: f32,
    /// Metres he is to the right of his place, across the unit's facing (negative: to the left):
    /// a re-lay leaves each man where he stands and gives him a slot ([`assign_slots`]).
    pub side: f32,
    /// Which of the unit's men he is (0 .. the unit's starting men, his index when the march was
    /// sized): the entries are kept in slot order, so a re-slot moves him to another index, and the
    /// view draws the figure numbered `figure` from the entry it finds him in.
    pub figure: u32,
    /// The kind of the move he was last issued ([`MoveKind`]): his unit's at a timed-move issue,
    /// his own at a walk to his slot ([`LandUnit::send_men_to_slots`]).
    pub kind: MoveKind,
    /// While he walks to his slot in a plain move ([`LandUnit::send_men_to_slots`]): its speed.
    pub walk: Option<f32>,
    /// His speed over the last tick (m/s): what he covered, as the unit's [`LandUnit::speed`].
    pub speed: f32,
    /// Metres left to his timed-move point (the soldier's distance to his order's `+0x3F4`).
    pub distance: f32,
    /// His timed move has finished (his order's `+0x10` = 1): he reached his place at the path's
    /// end or braked to a stop for it, and stands until he is issued again ([`reissued`]).
    pub finished: bool,
    /// His kept order speed (`+0x440`), for an order at the path's end ([`TimedMove::speed_for`]).
    pub kept: Option<f32>,
}

/// The kind of the move the men were issued (the soldier's `+0x220`: 0 walk, 1 and 3 run, 2
/// charge). CONFIRMED: `StartSoldierTimedMoveOrder` `0x00816900` writes it from the order
/// (`+0x38`, through `0x00662E50`), and while his locomotive moves `0x006543D0` sets his entity
/// state `+0x1B8` from it (8 walk, 0xB run, 0xD charge for the plain stances), whatever his unit
/// does: the state the display picks his locomotion graph by (`0x007A7190`). So a man still
/// walking to his place after his unit stopped moves in the kind of the move he was issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveKind {
    /// A walk (0; state 8).
    #[default]
    Walk,
    /// A run (1 and 3; state 0xB).
    Run,
    /// A charge (2; state 0xD).
    Charge,
}

/// The men's march of one unit: each man's state (in slot order: man `k` holds slot `k` of the
/// laid formation, and each keeps his own figure, [`ManMarch::figure`], which the view draws him
/// as; sized to the unit's starting men) and the timed-move order they share (issued in the same
/// tick).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MenMarch {
    /// Each man's state.
    pub men: Vec<ManMarch>,
    /// The order the men follow: the unit's own while it moves, then the same order, re-issued with
    /// its point where the unit stopped, until every man has finished; `None` when every man stands.
    /// Its `kept` and `target` stay `None`: each man keeps his own kept speed, and the target is
    /// the unit's ([`LandUnit::timed_move`]).
    pub plan: Option<TimedMove>,
    /// The kind of the move the men were last issued while the unit moved.
    pub kind: MoveKind,
    /// The formation the men hold their slots on ([`LandUnit::formation`]): laid out over the living
    /// men at a new move order (not at its re-issues), every tick of a move short of its path's
    /// end, at a re-slot and at the idle reform's entry
    /// ([`LandUnit::reslot_tick`]). `None`
    /// before the unit's first tick (the fresh layout then: the exe's unit is built with a re-slot
    /// pending, `0x00518DC0` sets `+0x278C` = 1 with `+0x2788` = 0).
    pub laid: Option<Formation>,
    /// The living men at the unit's last tick: fewer now is a loss, which restarts the re-slot
    /// countdown.
    pub last_men: u32,
    /// Whether a re-slot is pending (unit `+0x278C`).
    pub reslot: bool,
    /// Ticks until the pending re-slot runs (unit `+0x2788`, [`RESLOT_TICKS`] at each loss); the
    /// re-slot runs, and clears `reslot`, at the first tick it finds this at 0.
    pub reslot_in: u32,
    /// The box measured from the men where they stand (the squad summary at squad `+0x62C`,
    /// [`SquadBox`]), refreshed at the start of alternate ticks the unit has men
    /// ([`LandUnit::refresh_summary`]), and when the unit joins the battle or is placed
    /// ([`LandUnit::remeasure`]); `None` before its first measurement.
    pub measured: Option<SquadBox>,
    /// Whether men walk to their slots in plain moves ([`ManMarch::walk`]) while the unit stands:
    /// the reform state is on the unit's task stack (pushed by `0x005735A0`, popped when every
    /// man's move has ended).
    pub reforming: bool,
    /// The squad's in-place stamp (`+0x2784`, CONFIRMED static): the tick the men were last found
    /// in place, by the idle in-place check ([`LandUnit::in_place_check`], `0x00557C80`) or at the
    /// reform state's end (`0x00581CA0` → `0x00557D80`); 0 when the check found them out of place,
    /// and when the unit is built (`0x00518DC0`).
    pub in_place_at: u32,
    /// Whether the idle task has entered the reform state (the task's `+0x2C`, set by
    /// `0x005735A0`): a new idle task (`0x0051A3A0`, `+0x2C` = 0) is pushed when the unit's task
    /// stack empties, so ours clears it whenever the unit is under a move order.
    pub idle_reformed: bool,
    /// Whether this tick's layout step ([`LandUnit::reslot_tick`]) assigned the men their slots,
    /// and in full: `Some(full)` when it did, `None` when not (also for the reform state's entry
    /// without a re-slot, which keeps each man's slot). The move order's issue in the same tick
    /// lays out only when it did not, so the men are assigned once per tick (`0x00581AE0` →
    /// `0x00576B30`), or in full for an order behind the front when that one was partial
    /// ([`Battle::march_men`]). Set by every [`LandUnit::reslot_tick`].
    pub relaid: Option<bool>,
    /// The slot assignment's work lists, kept so a re-lay allocates nothing.
    pub scratch: SlotScratch,
    /// The slot assignments run so far ([`LandUnit::relay`]), for the tests.
    #[cfg(test)]
    pub assignments: u32,
}

/// The slot assignment's work lists ([`assign_slots`]): no state, so two are always equal and a
/// clone starts empty.
#[derive(Default)]
pub struct SlotScratch {
    costs: Vec<f32>,
    eligible: Vec<u32>,
}

impl Clone for SlotScratch {
    fn clone(&self) -> Self {
        SlotScratch::default()
    }
}

impl PartialEq for SlotScratch {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for SlotScratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SlotScratch")
    }
}

/// The squad summary's box (squad `+0x62C`), measured from the men where they stand: its centre
/// (summary `+0x18`, stored again at `+0xB0`), its heading (`+0x20`), width (`+0x24`, squad
/// `+0x650`), depth (`+0x28`, squad `+0x654`) and radius (`+0x44`, squad `+0x670`). CONFIRMED:
/// written by `0x0053D1A0` and `0x0053C3D0` ([`measure_squad_box`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SquadBox {
    /// The box's centre.
    pub centre: (f32, f32),
    /// The 16-bit heading the men face ([`super::abilities::angle16`]).
    pub heading: u16,
    /// Width in metres, across the heading.
    pub width: f32,
    /// Depth in metres, along the heading.
    pub depth: f32,
    /// The distance from the centre to the farthest man (no soldier radius added).
    pub radius: f32,
}

impl SquadBox {
    /// The unit direction of the heading (summary `+0x34`).
    pub fn front(&self) -> (f32, f32) {
        dir16(self.heading)
    }

    /// The middle of the box's front edge (summary `+0x9C`, `0x0053C3D0`: the centre plus the front
    /// times half the depth).
    pub fn front_centre(&self) -> (f32, f32) {
        let f = self.front();
        (self.centre.0 + f.0 * self.depth * 0.5, self.centre.1 + f.1 * self.depth * 0.5)
    }
}

/// One man as the squad summary reads him (the `0x1C`-byte record `0x00519150`): his position
/// (`+4`, `+8`), heading (`+0xC`) and radius (`+0x10`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryMan {
    /// Position (x, z plane).
    pub pos: (f32, f32),
    /// The 16-bit heading he faces.
    pub heading: u16,
    /// His radius (soldier `+0x620`).
    pub radius: f32,
}

/// The 16-bit angle of a unit direction as `0x0053D1A0` rounds it (`fpatan × 10430.378`, the x87
/// round to nearest even).
fn heading_of(dir: (f32, f32)) -> u16 {
    (dir.0.atan2(dir.1) * 10430.378).round_ties_even() as i32 as u16
}

/// The wrapped distance between two 16-bit headings, 0..=0x8000.
fn heading_gap(a: u16, b: u16) -> u32 {
    u32::from((a.wrapping_sub(b) as i16).unsigned_abs())
}

/// The squad summary's box measured from `men` (CONFIRMED static, `0x0053D1A0` then the radius of
/// `0x0053C3D0`); `previous` is the box's last heading. `None` without men (the exe then keeps the
/// summary as it was).
/// - One man: the box is his position, his heading, his radius as width and depth, radius 0.
/// - Otherwise, with `c` the men's mean position (`0x0053A390`): each man's distance `d` from `c`,
///   their mean `μ` and standard deviation `σ`, and the largest soldier radius `r`. The main axis
///   `A` of the men's spread is the eigenvector `(−Sxy, Sxx − λ)` of their covariance sums for the
///   larger eigenvalue `λ`, normalised; it falls back to +x when its length is at most 1e-4 or both
///   its components are at most 0.001. `B` is `A` turned a quarter (`(−A.y, A.x)`).
/// - The men with `0 < d < μ + 2σ` span each axis (from 0 both ways, so the span holds `c`); each
///   extent is its span plus `2r`, and the centre is `c` moved to the middle of both spans.
/// - The heading: each man votes for the nearest of `[A, −A, B, −B]` as 16-bit angles (a later one
///   only when strictly nearer); the most votes win, the first on a tie; with fewer than
///   `round(0.65 n)` votes, the one nearest `previous` wins instead (the first on a tie). Facing
///   along `±A` the width is the `B` extent and the depth the `A` extent; along `±B` the reverse.
/// - The radius: the farthest man from the box's centre.
pub fn measure_squad_box<I>(men: I, previous: u16) -> Option<SquadBox>
where
    I: Iterator<Item = SummaryMan> + Clone,
{
    let n = men.clone().count();
    if n == 0 {
        return None;
    }
    if n == 1 {
        let m = men.clone().next()?;
        return Some(SquadBox { centre: m.pos, heading: m.heading, width: m.radius, depth: m.radius, radius: 0.0 });
    }
    let inv = 1.0 / n as f32;
    let sum = men.clone().fold((0.0f32, 0.0f32), |s, m| (s.0 + m.pos.0, s.1 + m.pos.1));
    let c = (sum.0 * inv, sum.1 * inv);
    let dist = |m: &SummaryMan| ((m.pos.0 - c.0).powi(2) + (m.pos.1 - c.1).powi(2)).sqrt();
    let r = men.clone().fold(0.0f32, |r, m| if r <= m.radius { m.radius } else { r });
    let mean = men.clone().map(|m| dist(&m)).sum::<f32>() * inv;
    let sigma = (men.clone().map(|m| (dist(&m) - mean).powi(2)).sum::<f32>() * inv).sqrt();
    let (mut sxx, mut sxy, mut syy) = (0.0f32, 0.0f32, 0.0f32);
    for m in men.clone() {
        let q = (m.pos.0 - c.0, m.pos.1 - c.1);
        sxx += q.0 * q.0;
        sxy += q.0 * q.1;
        syy += q.1 * q.1;
    }
    let v = (-sxy, sxx - ((4.0 * sxy * sxy + (sxx - syy).powi(2)).sqrt() + syy + sxx) * 0.5);
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    let a = if len <= 1e-4 {
        (1.0, 0.0)
    } else {
        let k = 1.0 / len;
        let a = (k * v.0, k * v.1);
        if a.0.abs() <= 0.001 && a.1.abs() <= 0.001 { (1.0, 0.0) } else { a }
    };
    let b = (-a.1, a.0);
    let limit = sigma + sigma + mean;
    let (mut a_min, mut a_max, mut b_min, mut b_max) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for m in men.clone() {
        let d = dist(&m);
        if 0.0 < d && d < limit {
            let q = (m.pos.0 - c.0, m.pos.1 - c.1);
            let (pa, pb) = (a.0 * q.0 + a.1 * q.1, b.0 * q.0 + b.1 * q.1);
            (a_min, a_max, b_min, b_max) = (a_min.min(pa), a_max.max(pa), b_min.min(pb), b_max.max(pb));
        }
    }
    let (ext_a, ext_b) = (r * 2.0 + (a_max - a_min), (b_max - b_min) + r * 2.0);
    let (mid_a, mid_b) = ((a_max - a_min) * 0.5 + a_min, (b_max - b_min) * 0.5 + b_min);
    let centre = (b.0 * mid_b + a.0 * mid_a + c.0, b.1 * mid_b + a.1 * mid_a + c.1);
    let (ha, hb) = (heading_of(a), heading_of(b));
    let candidates = [ha, ha.wrapping_add(0x8000), hb, hb.wrapping_add(0x8000)];
    let nearest = |h: u16| (1..4).fold(0, |best, k| if heading_gap(h, candidates[k]) < heading_gap(h, candidates[best]) { k } else { best });
    let mut votes = [0u32; 4];
    for m in men.clone() {
        votes[nearest(m.heading)] += 1;
    }
    let mut won = (1..4).fold(0, |won, k| if votes[k] > votes[won] { k } else { won });
    if f64::from(votes[won]) < (f64::from(n as f32) * 0.65).round_ties_even() {
        won = nearest(previous);
    }
    let (width, depth) = if won < 2 { (ext_b, ext_a) } else { (ext_a, ext_b) };
    let far = men.fold(0.0f32, |f, m| f.max((m.pos.0 - centre.0).powi(2) + (m.pos.1 - centre.1).powi(2)));
    Some(SquadBox { centre, heading: candidates[won], width, depth, radius: far.sqrt() })
}

/// Ticks from a unit's last loss to its men's re-slot (CONFIRMED static): the squad's remove-soldier
/// vfunc `+4` `0x005755D0` sets the pending flag `+0x278C` and, when the soldier's byte `+0xEC` is
/// clear and the formation shape's vfunc `+0x30` returns true, the countdown `+0x2788` = 20;
/// `SQUAD::update` `0x00580A30` counts it down once per tick after the state updates, and the order
/// updates re-slot when `+0x278C` is set with `+0x2788` at 0. Both conditions hold at every death:
/// a death runs the soldier's vfunc `+0x104` `0x00653A10`, which clears `+0xEC` before it removes
/// him from his unit (`0x00563040` → `0x00574E90` → `0x005756D0` → `0x00575560` → the squad's
/// `+4`); `+0xEC` is set only by the soldier's reset `0x00660AB0` (vfunc `+0xF8`). The shape is the
/// line (kind 0 of the factory `0x00531680`, vtable `0x01333AA8`), whose `+0x30` (`0x00447880`)
/// returns true, as do kinds 3 to 6; kinds 1 and 2 return false (`0x00462CB0`): a loss in such a
/// shape makes the re-slot pending without restarting the countdown ([`LandUnit::shape_kind`]).
pub const RESLOT_TICKS: u32 = 20;

/// Ticks between an idle unit's in-place checks (`[0x014A35D0]`, 10, set by `0x00407740`): the
/// idle update `0x005577C0` runs the check in the ticks whose remainder by this is the unit's
/// ([`LandUnit::in_place_check`]). CONFIRMED static.
pub const IN_PLACE_CHECK_TICKS: u32 = 10;

/// Ticks an in-place stamp holds the idle unit out of the reform state (`0x00557BA0`: the stamp
/// is recent while it is not 0 and the tick is below it plus 20). CONFIRMED static.
pub const IN_PLACE_HOLD_TICKS: u32 = 20;

/// The in-place check's tolerance added to the drill tolerance ([`LandUnit::drill_tolerance`])
/// for a mounted unit ([`super::strength::mounted`]) and for one on foot (`0x00557BC0`: 2.5 m and
/// 1 m). CONFIRMED static.
pub const IN_PLACE_MOUNTED_M: f32 = 2.5;
/// See [`IN_PLACE_MOUNTED_M`].
pub const IN_PLACE_FOOT_M: f32 = 1.0;

/// The drill tolerance while the `light_infantry_behaviour` ability is active (`0x0057E1F0`
/// returns 1.5 then, else its training level's `entity_training_levels` float). CONFIRMED static.
pub const LIGHT_INFANTRY_TOLERANCE_M: f32 = 1.5;

/// The formation shape kind each special ability gives while active, in the order the exe tries
/// them (`0x005498D0`, CONFIRMED static): the first active one wins, kind 0 (the line) without
/// any. (Ability enum value, shape kind.) The shape factory `0x00531680` builds the kind's shape
/// (via `0x00531630`); `0x00579780` and the ability switch `0x00578860` (through `0x005315A0`)
/// re-create it. PROVISIONAL: every kind is laid out as the line. Traced so far (notes, "The
/// shape parameters"): kind 6 (light infantry) is the line with file spacing 3.5 and a half-pitch
/// wider front (`0x00642BA0`) whose ranks stagger by half a pitch (`0x0065A4F0`); kind 3 is the
/// line with spacings 1.0; the layouts of kinds 1, 2, 4 and 5 are not traced.
pub const SHAPE_KINDS: [(u8, u8); 6] = [(6, 3), (0, 2), (1, 4), (2, 5), (3, 1), (4, 6)];

/// The largest cost a pair may have to take part in a partial assignment (`0x005373F0` passes
/// 16.0 to `0x00502770` when not re-slotting, `f32::MAX` when it does; CONFIRMED static): a man
/// about 4 m or more from his slot keeps it.
pub const PARTIAL_ASSIGN_MAX_COST: f32 = 16.0;

/// The squared distance from his slot beyond which the reform sends a man there (`0x005736B0`
/// passes 0.0625 to `0x0055F160`, which moves him when he is strictly farther; CONFIRMED static).
pub const REFORM_MOVE_SQ: f32 = 0.0625;

/// The squared distance from his slot beyond which a man moved to it runs (the soldier's update
/// `0x00658C50` compares his squared distance to the target `0x00653560` stored with the float
/// 25.0 at `0x01325D54`, at `0x00658D07`: the run speed, record `+0x20`, and a run when strictly
/// farther; else the walk speed `+0x1C` and a walk; CONFIRMED static).
pub const REFORM_RUN_SQ: f32 = 25.0;

/// What it costs to give the man at `man` the slot at `slot` (`0x005281D0`, CONFIRMED static),
/// both in his unit's frame (to the right, back): the squared distance `c` between them, plus,
/// when `c > 0`, `(1 − min(f · d̂, 1)) / 2` with `d̂` the unit direction from him to the slot and `f`
/// the way he faces (`dir16` of his heading `+0x64`): a slot behind him costs up to 1 more than
/// one as far ahead. PROVISIONAL (ours): every man faces his unit's facing, straight ahead in its
/// frame; the men's own headings (soldier `+0x64`) are not modelled, and how the exe's differ from
/// the unit's while men walk or turn is not traced.
pub fn slot_cost(man: (f32, f32), slot: (f32, f32)) -> f32 {
    let d = (slot.0 - man.0, slot.1 - man.1);
    let c = d.0 * d.0 + d.1 * d.1;
    if c > 0.0 {
        // Ahead is −back in the unit's frame.
        let ahead = (-d.1 * (1.0 / c.sqrt())).min(1.0);
        (1.0 - ahead) * 0.5 + c
    } else {
        c
    }
}

/// Gives each man a slot (CONFIRMED static, `0x005373F0` → `0x00502770`): `men[k]` starts in slot
/// `k` (the soldier vector in order, each to the next slot) and the pairs are improved by swapping
/// two men's slots, costed by [`slot_cost`] of a man's position `pos` and a slot's `slot(k)`; on
/// return `men` is in slot order (`0x00555CA0` rebuilds the soldier vector from the pairs).
/// - Every round takes the pair of largest cost among those still eligible (all at first) whose
///   cost is below the limit (the first on a tie); none left ends the assignment.
/// - `full` (a re-slot, limit `f32::MAX`, no round limit): the first other eligible pair, in list
///   order, both below the limit, whose two swapped costs sum strictly below their present sum is
///   swapped with it; without one the pair leaves the eligible list (swapped with the list's last).
/// - Otherwise (limit [`PARTIAL_ASSIGN_MAX_COST`], one round): the other eligible pair below the
///   limit whose swap lowers the sum most (strictly below 0, the first on a tie) is swapped.
///
/// Ours has no special soldiers (`0x005373F0` pairs the soldiers whose vfunc `+0xF4` object has
/// `+0xA8` set first, in fixed slots, and a named soldier next: the layouts run here name none).
pub fn assign_slots<T>(men: &mut [T], pos: impl Fn(&T) -> (f32, f32), slot: impl Fn(usize) -> (f32, f32), full: bool, scratch: &mut SlotScratch) {
    let SlotScratch { costs, eligible } = scratch;
    costs.clear();
    costs.extend(men.iter().enumerate().map(|(k, m)| slot_cost(pos(m), slot(k))));
    eligible.clear();
    eligible.extend(0..men.len() as u32);
    let limit = if full { f32::MAX } else { PARTIAL_ASSIGN_MAX_COST };
    let mut rounds = 0u32;
    while full || rounds < 1 {
        rounds += 1;
        let mut worst: Option<usize> = None;
        for (e, &k) in eligible.iter().enumerate() {
            let c = costs[k as usize];
            if c < limit && worst.is_none_or(|w| costs[eligible[w] as usize] < c) {
                worst = Some(e);
            }
        }
        let Some(w) = worst else { break };
        let a = eligible[w] as usize;
        let mut swap: Option<(usize, f32, f32)> = None;
        let mut best = 0.0f32;
        for (e, &k) in eligible.iter().enumerate() {
            let b = k as usize;
            if e == w || costs[b] >= limit {
                continue;
            }
            // Man b in slot a, man a in slot b.
            let (ca, cb) = (slot_cost(pos(&men[b]), slot(a)), slot_cost(pos(&men[a]), slot(b)));
            if full {
                if cb + ca < costs[b] + costs[a] {
                    swap = Some((b, ca, cb));
                    break;
                }
            } else {
                let delta = (cb + ca) - (costs[b] + costs[a]);
                if delta < best {
                    (best, swap) = (delta, Some((b, ca, cb)));
                }
            }
        }
        match swap {
            Some((b, ca, cb)) => {
                men.swap(a, b);
                (costs[a], costs[b]) = (ca, cb);
            }
            None => {
                eligible.swap_remove(w);
            }
        }
    }
}

impl MenMarch {
    /// Whether any man is still moving or has a timed move left to walk.
    pub fn active(&self) -> bool {
        self.plan.is_some() || self.reforming
    }

    /// Sizes the men to `count`, numbering each new one's figure by his index.
    pub fn ensure_men(&mut self, count: usize) {
        if self.men.len() != count {
            let from = self.men.len();
            self.men.resize(count, ManMarch::default());
            for (i, m) in self.men.iter_mut().enumerate().skip(from) {
                m.figure = i as u32;
            }
        }
    }
}

/// Whether a man gets `order` at its issue (tick 0): the per-soldier gate of `0x00584E10`
/// (CONFIRMED). At the move's first issue every man does; at a re-issue a man whose move still runs
/// does (`0x0063E290` false), and a `finished` one only while the issue point is short of the path's
/// end (order `+0x3C` clear) and `0x00658C20` holds (ours: always, see the module note).
pub fn reissued(order: &TimedMove, finished: bool) -> bool {
    order.first || !finished || !order.at_end
}

/// Man `i`'s place in a block of `files` × `ranks`, `width` × `depth` metres, in the unit's own
/// frame: (to his right, back from the front), from the block's centre. Man `i` stands in file
/// `i % files` of rank `i / files`, rank 0 in front: the exe's line shape fills one rank of `files`
/// slots after another (`0x00619E60` steps the slot counter through a rank, CONFIRMED).
/// PROVISIONAL (ours, the drawn block; see the module note): the first rank's placement
/// `0x00619EB0` puts the men on alternate sides by slot parity (a rank filled from its anchor
/// outward) and was not decoded, so the place within a rank (and a partial last rank's, ours from
/// the left), the rank pitch and the anchor are ours. The view draws the men here and the model
/// walks them from here, both through [`Formation::place`].
pub fn formation_place(i: usize, files: usize, ranks: usize, width: f32, depth: f32) -> (f32, f32) {
    let (files, ranks) = (files.max(1), ranks.max(1));
    let (dx, dz) = (width / files as f32, depth / ranks as f32);
    let (file, rank) = (i % files, i / files);
    ((file as f32 - (files - 1) as f32 * 0.5) * dx, (rank as f32 - (ranks - 1) as f32 * 0.5) * dz)
}

/// The model-plane offset (x, y) of a point `(right, back)` metres in the frame of a unit facing
/// `facing` (radians, 0 = +x).
pub fn unit_frame_offset(facing: f32, (right, back): (f32, f32)) -> (f32, f32) {
    let (s, c) = facing.sin_cos();
    (right * s - back * c, -right * c - back * s)
}

/// A unit's formation laid out once: its grid and size ([`LandUnit::formation`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Formation {
    /// Men in the front rank.
    pub files: usize,
    /// Ranks the living men fill ([`LandUnit::formation_grid`]).
    pub ranks: usize,
    /// Width in metres.
    pub width: f32,
    /// Depth in metres.
    pub depth: f32,
}

impl Formation {
    /// Man `i`'s place in the unit's frame ([`formation_place`]).
    pub fn place(&self, i: usize) -> (f32, f32) {
        formation_place(i, self.files, self.ranks, self.width, self.depth)
    }
}

impl LandUnit {
    /// The formation's files and ranks over its living men: the files are the front rank
    /// ([`LandUnit::files`], the men who fire with the default drill, capped at the living men as
    /// the callers of `0x00642C90` cap them), the ranks as many as the living men fill
    /// (`ceil(men / files)`: `0x00581200` lays the unit's soldier count `+0x18` out over the slots,
    /// slot `i` in rank `i / files`, `0x00619E60`, CONFIRMED). The one grid the men's places come
    /// from: the laid one ([`LandUnit::formation`]).
    pub fn formation_grid(&self) -> (usize, usize) {
        let f = self.formation();
        (f.files, f.ranks)
    }

    /// The formation laid out fresh over the living men: the grid of [`LandUnit::formation_grid`]
    /// over [`LandUnit::fresh_width`], the ranks at [`LandUnit::rank_spacing`] (the shape's layout
    /// vfunc `+0x24` run with the unit's soldier count `+0x18`).
    fn fresh_formation(&self) -> Formation {
        let files = (self.files() as usize).max(1);
        let ranks = (self.men as usize).div_ceil(files).max(1);
        Formation { files, ranks, width: self.fresh_width(), depth: ranks as f32 * self.rank_spacing() }
    }

    /// The formation the men hold their slots on: the one layout the model and the view share
    /// (the unit's size is measured from the men standing on it, [`LandUnit::squad_box`]). It is laid out
    /// ([`LandUnit::fresh_formation`]) at a new move order (`BuildUnitMoveOrder` `0x0051A530` for
    /// one behind the front, else its first update when short of the path's end; a re-issue,
    /// `0x00584E10` with 0, assigns nothing, see [`Battle::march_men`]), at every tick of a move
    /// whose issue point is short of the path's end, and at a re-slot after losses
    /// ([`LandUnit::reslot_tick`]); otherwise the men
    /// keep their slots, and a loss pulls the men behind forward along the slot order (`0x005755D0`
    /// compacts the soldier vector along the shape's vfunc `+0x34` chain; ours: man `i` holds slot
    /// `i`, so the living men fill the first slots). CONFIRMED static. The fire order's own re-lay
    /// (`0x00581200`: when its counter `+0x20` exceeds men × 0.5 it zeroes the counter, lays the
    /// slots out again over the living men and sends each man farther than `0x0057E1F0()` + 1.5 m
    /// from his slot back to it) is not needed here: the counter (`0x00539910`) counts, per tick,
    /// the front-rank men whose present move ended with result 0 (`0x008056D0`; a move ends with 1
    /// when the entity reached motion state 7, `0x00818834` / `0x0081936B`, so 0 is a move that
    /// ended short), and ours has no collision pushes and no move that ends short of its slot
    /// (`march_men`), so the counter never grows and that re-lay never runs. Before the first
    /// tick (`laid` `None`) the fresh layout: the exe's unit is built with a re-slot pending (`0x00518DC0`).
    pub fn formation(&self) -> Formation {
        self.march.laid.unwrap_or_else(|| self.fresh_formation())
    }

    /// One unit tick of the men's layout (CONFIRMED static, see [`LandUnit::formation`]):
    /// - a loss since the last tick makes a re-slot pending and sets its countdown to
    ///   [`RESLOT_TICKS`] (`0x005755D0`, at each death);
    /// - the move order's update (`UpdateUnitMoveOrderReissue` `0x00581AE0` → `LayOutUnitMoveOrderSlots` `0x00576B30`) lays
    ///   the slots out over the living men (`0x0057AFC0`) at every tick while its issue point is
    ///   short of the path's end (order `+0x3C` clear, ours [`TimedMove::at_end`] of the unit's own
    ///   order), its first update included (order `+0x7C`; ours: the new order's re-lay in
    ///   `march_men`, as is `BuildUnitMoveOrder`'s for an order behind the front); at the end it
    ///   lays out only for a due re-slot. The every-tick layout is skipped while
    ///   `0x00532360` reports a man on a building structure (squad `+0x734`); ours always lays out,
    ///   as a unit leaves its building when it moves (`garrison`);
    /// - a pending re-slot whose countdown is out lays the formation out again over the living men
    ///   (`0x0057AFC0` with its re-slot argument set → `0x00555CA0`: the layout, the nearest-slot
    ///   assignment and the soldier vector rebuilt in slot order, then `+0x278C` cleared): only a
    ///   due re-slot clears the pending flag, so a moving unit re-lays again when it falls due;
    /// - then the countdown counts down (`0x00580A30`, after the state updates, so a death in tick
    ///   T re-slots in tick T + 20).
    ///
    /// A loss in a shape of kind 1 or 2 ([`LandUnit::shape_kind`]) makes the re-slot pending
    /// without restarting the countdown (the shape's vfunc `+0x30` `0x00462CB0` returns false), so
    /// it falls due at the next tick the countdown is out.
    ///
    /// Each lay-out leaves the men where they stand and gives each a slot ([`LandUnit::relay`]):
    /// the full assignment at a re-slot (and at the first lay-out, the unit's pending one), the
    /// partial one at the every-tick lay-out.
    ///
    /// A standing unit (no move order, every man's move ended) is idle: its pending re-slot waits
    /// for the reform state, which the idle state enters when its gate holds
    /// ([`LandUnit::idle_gate`], `0x00573770`, checked before the idle update in the state
    /// machine's pass `0x010FBBD0`). The entry (`0x005735A0` → `0x005736B0`) lays the slots out
    /// afresh, re-slots in full when a re-slot is due (its byte `+0x35`: pending with the countdown
    /// out), else keeps each man's slot, and sends every man off his slot to it
    /// ([`LandUnit::send_men_to_slots`]); the state ends when every man's move has ended, stamping
    /// the men in place (`0x00581CA0` → `0x00557D80`). Without the gate the idle update runs the
    /// in-place check on the unit's ticks ([`LandUnit::in_place_check`]). CONFIRMED static.
    ///
    /// The exe checks the pending re-slot in each order update found (the fire order `0x00581200`,
    /// the move order `0x00581AE0`, `0x0066E9B0`) and in the unit states through the this-adjust
    /// thunks (CONFIRMED static): `0x004F9DF0` → `0x00583150`, the attack state's update;
    /// `0x004F7C60` → `0x005732B0`, the reform state's entry for the fire-at-target task group;
    /// `0x004F8790` → `0x005735A0`, the reform state's entry for the idle group (the idle state
    /// `0x01450BDC`, reload, reform, ready), which sets the task's `+0x2C` to 1. PROVISIONAL
    /// (ours): a standing unit is idle, whatever it does; the exe's firing and fighting units run
    /// their own task states (the fire-at-target group's reform entry `0x005732B0` and its gate are
    /// not traced).
    pub(super) fn reslot_tick(&mut self, tick: u32) {
        self.march.ensure_men(self.max_men as usize);
        if self.men < self.march.last_men {
            self.march.reslot = true;
            if !matches!(self.shape_kind(), 1 | 2) {
                self.march.reslot_in = RESLOT_TICKS;
            }
        }
        self.march.last_men = self.men;
        let due = self.march.reslot && self.march.reslot_in == 0;
        let first = self.march.laid.is_none();
        let standing = self.timed_move.is_none() && self.march.plan.is_none();
        if standing {
            // The idle state: the reform state's entry when the gate holds (the unit's first tick
            // always: a new idle task), else the idle update.
            let reform = !self.march.reforming && (first || self.idle_gate(tick));
            // Only a re-slot assigns the men (`lay_out(Some(true))`); the entry's lay-out without
            // one keeps each man's slot (`0x005736B0`), so it does not stand for the move order's
            // assignment in the same tick.
            self.march.relaid = (reform && (first || due)).then_some(true);
            if reform {
                self.lay_out((first || due).then_some(true));
                self.march.reslot &= !due;
                self.march.idle_reformed = true;
                self.send_men_to_slots();
                if !self.march.reforming {
                    // Every man is at his slot: the state ends, stamping the men in place.
                    self.march.in_place_at = tick;
                }
            } else if !self.march.reforming && tick % IN_PLACE_CHECK_TICKS == self.serial() % IN_PLACE_CHECK_TICKS {
                self.in_place_check(tick);
            }
        } else {
            self.march.idle_reformed = false;
            let short_of_end = self.timed_move.is_some_and(|t| !t.at_end);
            self.march.relaid = (first || due || short_of_end).then_some(first || due);
            if let Some(full) = self.march.relaid {
                self.relay(full);
            }
            self.march.reslot &= !due;
        }
        self.march.reslot_in = self.march.reslot_in.saturating_sub(1);
    }

    /// The unit's serial, the stagger of its alternate-tick summary refresh
    /// ([`LandUnit::refresh_summary`]) and of its idle in-place check ([`IN_PLACE_CHECK_TICKS`]).
    /// PROVISIONAL (ours): the exe's is the unit body's serial (unit `+4`, read by `0x00518E3C`
    /// through `0x0057E910` and by the idle update through the battle unit's vfunc `+0x10`),
    /// numbered from the battle model's counter (`+0xBC`, incremented at `0x0051B816`; 0 when the
    /// model is built, `0x00506D92`, 1 after `0x00506700`) which other registered battle objects
    /// may also draw from; ours is the unit's id. Whether the two agree, and the exe tick's phase
    /// with ours, need a debugger sitting.
    pub fn serial(&self) -> u32 {
        self.id
    }

    /// The idle state's gate into the reform state (`CheckIdleUnitCanEnterReform` `0x00573770`,
    /// CONFIRMED static): not while the unit's top controller vetoes it (the controller stack
    /// `+0xADC`, its vfunc `+0xB4`) or a man holds a stored plain move not yet issued
    /// (`0x0057ABD0`, soldier `+0x619` clear); else always for a new idle task (`+0x2C` clear,
    /// [`MenMarch::idle_reformed`]), and otherwise unless the men were found in place within the
    /// last [`IN_PLACE_HOLD_TICKS`] (`0x00557BA0`, [`MenMarch::in_place_at`]). PROVISIONAL (ours):
    /// ours has no unit controllers (what the building and other controllers' vfunc `+0xB4`
    /// returns is not traced) and issues the reform's moves at once, so neither veto holds.
    pub fn idle_gate(&self, tick: u32) -> bool {
        let stamp = self.march.in_place_at;
        let in_place = stamp != 0 && stamp.wrapping_add(IN_PLACE_HOLD_TICKS) > tick;
        !self.march.idle_reformed || !in_place
    }

    /// The drill tolerance (`0x0057E1F0`, CONFIRMED static): [`LIGHT_INFANTRY_TOLERANCE_M`] while
    /// the `light_infantry_behaviour` ability is active, else the training level's
    /// ([`LandUnit::training_tolerance`]).
    pub fn drill_tolerance(&self) -> f32 {
        if self.active_abilities & (1 << super::abilities::ability_ids::LIGHT_INFANTRY_BEHAVIOUR) != 0 {
            LIGHT_INFANTRY_TOLERANCE_M
        } else {
            self.training_tolerance
        }
    }

    /// The idle in-place check (`0x00557BC0` → `0x00557C20` → `0x00557C80`, CONFIRMED static): the
    /// formation laid out afresh over the living men, the mean distance of each man from his slot
    /// in it (man `k` from slot `k`); when it is at least the drill tolerance plus
    /// [`IN_PLACE_MOUNTED_M`] (mounted) or [`IN_PLACE_FOOT_M`], the stamp is cleared (the gate then
    /// holds), else the men are stamped in place at `tick` (also when no man is counted).
    /// PROVISIONAL (ours): the exe counts only the men whose soldier `+0x1A8` is clear (not traced),
    /// ours every living man.
    pub fn in_place_check(&mut self, tick: u32) {
        let mounted = if super::strength::mounted(self) { IN_PLACE_MOUNTED_M } else { IN_PLACE_FOOT_M };
        let tolerance = self.drill_tolerance() + mounted;
        let (old, fresh) = (self.formation(), self.fresh_formation());
        let alive = (self.men as usize).min(self.march.men.len());
        let sum: f32 = self.march.men[..alive]
            .iter()
            .enumerate()
            .map(|(k, m)| {
                let (p, q) = (old.place(k), fresh.place(k));
                (p.0 + m.side - q.0).hypot(p.1 + m.lag - q.1)
            })
            .sum();
        let out_of_place = alive > 0 && tolerance <= sum / alive as f32;
        self.march.in_place_at = if out_of_place { 0 } else { tick };
    }

    /// The squad summary's refresh at the start of a unit tick (`SQUAD::update` `0x00580A30` runs
    /// `UpdateSquadSummary` `0x0053C180` at `0x00580A7C` when the squad has men, before the state
    /// updates; CONFIRMED static): in battle (`0x0055AD60` false: the battle state `+0xB0` → `+0x14`
    /// past 1) only on the ticks whose parity is the summary stamp's (`+8`), so the box is measured
    /// on alternate ticks ([`LandUnit::measure_box`]). Ours runs only in battle (deployment
    /// re-measures at each placement, [`LandUnit::place_at`], as the exe's every-tick refresh
    /// there does). The stamp is the unit body's serial ([`LandUnit::serial`], PROVISIONAL).
    pub(super) fn refresh_summary(&mut self, tick: u32) {
        if self.men > 0 && (tick ^ self.serial()) & 1 == 0 {
            self.march.measured = self.measure_box();
        }
    }

    /// Lays the formation out again over the living men ([`LandUnit::fresh_formation`]) and gives
    /// each man a slot (`0x00555CA0`: the shape's layout, then [`assign_slots`], `full` at a
    /// re-slot): every man stays where he stands, his offset from his new slot ([`ManMarch::lag`],
    /// [`ManMarch::side`]) what is left of his way to it. CONFIRMED static. The slots are laid at
    /// the unit's position on its facing: `LayOutUnitMoveOrderSlots` `0x00576B30` lays them at the
    /// order's path cursor (`0x007F2D70` → `0x0080AEA0`, a point on a path segment), ours the unit's
    /// reference point (see the module note).
    pub(super) fn relay(&mut self, full: bool) {
        self.lay_out(Some(full));
    }

    /// Lays the formation out again over the living men as [`LandUnit::relay`] does, giving the men
    /// their slots by [`assign_slots`] (`Some(full)`) or keeping each man's slot (`None`: the
    /// reform state's lay-out without a re-slot, `0x005736B0`, which sends man `k` to slot `k`).
    fn lay_out(&mut self, assign: Option<bool>) {
        let old = self.formation();
        let laid = self.fresh_formation();
        let alive = (self.men as usize).min(self.march.men.len());
        let MenMarch { men, scratch, .. } = &mut self.march;
        // Where each man stands in the unit's frame, then his offset from his new slot.
        for (k, m) in men[..alive].iter_mut().enumerate() {
            let p = old.place(k);
            (m.side, m.lag) = (p.0 + m.side, p.1 + m.lag);
        }
        if let Some(full) = assign {
            assign_slots(&mut men[..alive], |m| (m.side, m.lag), |k| laid.place(k), full, scratch);
        }
        for (k, m) in men[..alive].iter_mut().enumerate() {
            let p = laid.place(k);
            (m.side, m.lag) = (m.side - p.0, m.lag - p.1);
        }
        self.march.laid = Some(laid);
        #[cfg(test)]
        {
            self.march.assignments += u32::from(assign.is_some());
        }
    }

    /// Sends each living man farther than 0.25 m from his slot to it in a plain move (the reform
    /// state's `0x005736B0` → `0x0055F160` with [`REFORM_MOVE_SQ`], → `0x00653560`, CONFIRMED
    /// static): in battle `0x00653560` stores the target at soldier `+0x5FC` and the soldier's
    /// update (`0x00658C50`) issues the move once (`0x007E2040`), at the run speed and as a run
    /// beyond 5 m ([`REFORM_RUN_SQ`]), else at the walk speed as a walk. The heading test (a man
    /// turned more than 910 / 65536 of a turn from his slot's heading also moves) never holds for
    /// ours: every man faces his unit's facing (see [`slot_cost`]).
    pub fn send_men_to_slots(&mut self) {
        let alive = (self.men as usize).min(self.march.men.len());
        let (walk, run) = (self.walk_speed, self.run_speed);
        for m in &mut self.march.men[..alive] {
            let d2 = m.side * m.side + m.lag * m.lag;
            if d2 > REFORM_MOVE_SQ {
                (m.walk, m.kind) = if d2 > REFORM_RUN_SQ { (Some(run), MoveKind::Run) } else { (Some(walk), MoveKind::Walk) };
                (m.finished, m.kept) = (false, None);
                self.march.reforming = true;
            }
        }
    }

    /// Man `i`'s place in the unit's frame ([`Formation::place`]).
    pub fn man_place(&self, i: usize) -> (f32, f32) {
        self.formation().place(i)
    }

    /// The formation shape kind (`0x005498D0`, [`SHAPE_KINDS`]): that of the first of the unit's
    /// active special abilities in the exe's order, 0 (the line) without any. PROVISIONAL (ours):
    /// every kind is laid out as the line (the other shapes' layouts are not traced), and an
    /// ability stays active once performed.
    pub fn shape_kind(&self) -> u8 {
        SHAPE_KINDS.iter().find(|(ability, _)| self.active_abilities & (1u32 << ability) != 0).map_or(0, |&(_, kind)| kind)
    }

    /// The living men where they stand: each man at his slot in the laid formation, his offset
    /// from it ([`ManMarch::side`], [`ManMarch::lag`]), facing the unit's facing, with the man
    /// radius [`LandUnit::man_radius`].
    pub fn men_standing(&self) -> impl Iterator<Item = SummaryMan> + Clone + '_ {
        let formation = self.formation();
        let (facing, pos, radius) = (self.facing, self.position, self.man_radius);
        let heading = heading_of((facing.cos(), facing.sin()));
        let lags = &self.march.men;
        (0..self.men as usize).map(move |i| {
            let (right, back) = formation.place(i);
            let (side, lag) = lags.get(i).map_or((0.0, 0.0), |m| (m.side, m.lag));
            let (right, back) = (right + side, back + lag);
            let (ox, oy) = unit_frame_offset(facing, (right, back));
            SummaryMan { pos: (pos.0 + ox, pos.1 + oy), heading, radius }
        })
    }

    /// The box measured now from the living men where they stand ([`measure_squad_box`] over
    /// [`LandUnit::men_standing`]). `None` without men. The heading vote falls back to the last
    /// box's heading, 0 before the first one (the summary is built with its heading from the zero
    /// word `0x014A3498`, set by `0x00407A10`; CONFIRMED static).
    pub fn measure_box(&self) -> Option<SquadBox> {
        let previous = self.march.measured.map_or(0, |b| b.heading);
        measure_squad_box(self.men_standing(), previous)
    }

    /// Measures the box now ([`LandUnit::measure_box`]) and keeps it as the unit's
    /// ([`UnitMarch::measured`]); a unit without men keeps its last one. Run when the unit joins the
    /// battle ([`super::model::Battle::add_unit`]) and when it is put somewhere at once
    /// ([`LandUnit::place_at`]), so a unit that has not ticked since has its box where it stands.
    pub fn remeasure(&mut self) {
        if let Some(b) = self.measure_box() {
            self.march.measured = Some(b);
        }
    }

    /// Puts the unit at `position` facing `facing` at once (a deployment placement, a building
    /// entered, a reinforcement's entry) and measures its box there ([`LandUnit::remeasure`]).
    /// In deployment the exe refreshes the summary every tick (`0x0053C180` while `0x0055AD60`
    /// holds), so the box measured here is the one the next tick would measure; placements also
    /// force a refresh (`0x0054FDD0`, called from the best-heading placement `0x0056DEB0` and the
    /// group merge `0x0057B0E0`, CONFIRMED static). PROVISIONAL (ours): whether the building and
    /// reinforcement entries refresh at once is not traced.
    pub fn place_at(&mut self, position: (f32, f32), facing: f32) {
        (self.position, self.facing) = (position, facing);
        self.remeasure();
    }

    /// The squad summary's box ([`SquadBox`]): the one measured at the start of this tick (or when
    /// the unit joined the battle or was last placed, [`LandUnit::remeasure`]); a unit without men
    /// keeps its last one (a box of size 0 at its position if it never had men). The unit's width,
    /// depth and radius ([`LandUnit::width`], [`LandUnit::depth`], [`LandUnit::radius`]) are this
    /// box's.
    pub fn squad_box(&self) -> SquadBox {
        self.march.measured.or_else(|| self.measure_box()).unwrap_or(SquadBox {
            centre: self.position,
            heading: heading_of((self.facing.cos(), self.facing.sin())),
            width: 0.0,
            depth: 0.0,
            radius: 0.0,
        })
    }

    /// Man `i`'s rank (0 = the front rank), as [`LandUnit::man_place`] lays him out.
    pub fn man_rank(&self, i: usize) -> usize {
        i / self.formation_grid().0
    }
}

/// What the unit's own move did this tick, for its men ([`Battle::march_men`]).
#[derive(Debug, Clone, Copy)]
pub(super) struct UnitStep {
    /// The unit's position before the step.
    pub pos: (f32, f32),
    /// The unit's facing after the step (the men's places are laid out on it).
    pub facing: f32,
    /// The unit's direction of travel (unit vector) and metres left to its goal before the step.
    pub dir: (f32, f32),
    pub len: f32,
    /// What the unit covered (m).
    pub covered: f32,
    /// Whether the goal is an ordered destination (the men brake for it).
    pub is_order: bool,
    /// The fatigue speed multiplier (`FatigueEffects::speed`).
    pub fatigue: f32,
}

impl Battle {
    /// Moves unit `idx`'s men for this tick (see the module note). `step` is the unit's own move,
    /// `None` when it did not move (stopped, or never moving): the men then walk what is left of
    /// their way to their slots. `plan` is the order the unit followed this tick before its advance
    /// (`None` without a move), so the men use the same speed, planned time and time since the issue.
    pub(super) fn march_men(&mut self, idx: usize, step: Option<UnitStep>, plan: Option<TimedMove>) {
        // A standing unit whose men stand too (every tick of most units): nothing to do.
        let m = &self.units[idx].march;
        if (step.is_none() || plan.is_none()) && m.plan.is_none() && !m.reforming {
            return;
        }
        let ground = std::sync::Arc::clone(&self.ground);
        let tick = self.tick;
        let u = &mut self.units[idx];
        u.march.ensure_men(u.max_men as usize);
        let (class, run_speed, accel, decel) = (u.movement_class, u.run_speed, u.acceleration, u.deceleration);
        // The order: the unit's while it moves; after it, the same order, re-issued on the same
        // schedule with the issue point where the unit stands (no path left: D = 0, at the end).
        // The men's copy keeps no kept speed (each man keeps his own) and no target (the unit's).
        // `None` while only the men walking to their slots move (`ManMarch::walk`).
        let order = match (plan, step) {
            (Some(p), Some(_)) => Some(TimedMove { kept: None, target: None, ..p }),
            _ => u.march.plan.map(|p| TimedMove::next(Some(p), p.speed, 0.0, None, tick)),
        };
        let issued = order.is_some_and(|o| o.ticks == 0);
        if issued
            && let (Some(o), Some(s)) = (order, step)
        {
            // The kind of the move the men are issued (`+0x220`, see `MoveKind`): a charge, a run
            // (the run option, or a rout, which runs), else a walk.
            u.march.kind = if u.charging {
                MoveKind::Charge
            } else if u.running || u.morale.is_routing_or_shattered() {
                MoveKind::Run
            } else {
                MoveKind::Walk
            };
            // A new order to a goal behind the men's front re-slots them (CONFIRMED static): the
            // path request's `+0x8C` is set (`0x008102F0` at `0x00810D3F`) when the goal minus the
            // summary's front centre (`+0x9C`, `SquadBox::front_centre`) points against its front
            // (`+0x34`), and `BuildUnitMoveOrder` `0x0051A530` then lays out with the re-slot
            // argument (`0x00576B30` with 1), which clears the pending re-slot. PROVISIONAL: the
            // test runs only when the request's `+0x28` sub-object's first word is 0, which ours
            // takes for every order (its meaning is not traced).
            let behind = o.first
                && u.march.measured.is_some_and(|b| {
                    let (front, at) = (b.front(), b.front_centre());
                    let goal = (s.pos.0 + s.dir.0 * s.len, s.pos.1 + s.dir.1 * s.len);
                    (goal.0 - at.0) * front.0 + (goal.1 - at.1) * front.1 < 0.0
                });
            if behind {
                u.march.reslot = false;
            }
            // Only a new order gives the men slots again (`LandUnit::relay`, each man to his
            // nearest), CONFIRMED static: `BuildUnitMoveOrder` lays out in full for an order behind
            // the front (`0x0051A8E3` → `0x00576B30` with 1), and the order's first update lays out
            // (`0x00576B30`, `+0x7C` clear) when its issue point is short of the path's end
            // (`0x00581AE0` skips `0x00576B30` while `+0x3C` is set, unless a re-slot is due, which
            // `LandUnit::reslot_tick` runs). A re-issue (`0x00584E10` with 0) assigns nothing: its
            // shape vfunc `+0x24` only computes the slot points the men are issued to. The one
            // assignment that can run inside it (`0x0057AC10`) needs `0x005516D0` to hold, which it
            // never does for an ordinary move (order `+0x20` is 0 in every `0x0051A940` descriptor);
            // where it does run, it runs once per order (`+0x44`), at the first issue where a man
            // passes the check, which may be a re-issue (not modelled, BACKLOG). INFERRED: when this tick's layout step already laid out
            // (`LandUnit::reslot_tick`: the previous order short of its end), the issue adds none
            // unless it is behind the front and that one was partial; the order of a new order's
            // build and the squad's layout step in one tick is not traced.
            if o.first && (behind || !o.at_end) && u.march.relaid.is_none_or(|full| behind && !full) {
                u.relay(behind);
            }
        }
        let s = step.unwrap_or(UnitStep {
            pos: u.position,
            facing: u.facing,
            dir: (u.facing.cos(), u.facing.sin()),
            len: 0.0,
            covered: 0.0,
            is_order: true,
            fatigue: u.fatigue_effect().speed,
        });
        let alive = (u.men as usize).min(u.march.men.len());
        let (mut moving, mut all_finished) = (false, true);
        let formation = u.formation();
        let kind = u.march.kind;
        for (i, m) in u.march.men[..alive].iter_mut().enumerate() {
            if let Some(o) = order.filter(|_| issued)
                && reissued(&o, m.finished)
            {
                // His place at the issue point: its distance ahead of his slot now, from where he
                // stands.
                m.distance = m.side.hypot(o.distance + m.lag);
                (m.kept, m.finished, m.walk, m.kind) = (None, false, None, kind);
            }
            // His speed before his multiplier: a walk to his slot's own, else the order's.
            let base = match (m.walk, order) {
                _ if m.finished => None,
                (Some(v), _) => Some(v),
                (None, Some(o)) => Some(o.speed_for(run_speed, m.distance, &mut m.kept)),
                (None, None) => None,
            };
            let covered = match base {
                None => {
                    m.finished = true;
                    0.0
                }
                Some(base) => {
                    // His way to his slot at the goal, in the unit's frame (to his right, back).
                    let (across, back) = (m.side, s.len + m.lag);
                    let left = across.hypot(back);
                    let way = if left > 0.0 { unit_frame_offset(s.facing, (-across / left, -back / left)) } else { s.dir };
                    // His own multiplier: the ground under him and the slope ahead of him on his
                    // way (the unit's rules, `slope_multiplier`), with the unit's fatigue.
                    let place = formation.place(i);
                    let (ox, oy) = unit_frame_offset(s.facing, (place.0 + m.side, place.1 + m.lag));
                    let at = (s.pos.0 + ox, s.pos.1 + oy);
                    let modifier = ground.speed_modifier(at.0, at.1, class);
                    let probe = order.map_or(base, |o| o.speed);
                    let planned = (probe * s.fatigue * modifier * TICK_SECONDS).max(0.01);
                    let gradient = (ground.height(at.0 + way.0 * planned, at.1 + way.1 * planned) - ground.height(at.0, at.1)) / planned;
                    let multiplier = s.fatigue * modifier * slope_multiplier(gradient);
                    // He brakes for his slot as the unit does for its goal.
                    let braking = s.is_order && brakes_for(left, m.speed, decel);
                    let wanted = if braking { 0.0 } else { (base * multiplier).max(0.0) };
                    let next = step_speed(m.speed, wanted, accel * multiplier, decel) * TICK_SECONDS;
                    let covered = next.min(left);
                    // His move finishes at his slot (at the path's end for a timed move), or braked
                    // to a stop for it.
                    let at_end = m.walk.is_some() || order.is_some_and(|o| o.at_end);
                    m.finished = at_end && (left <= next || (braking && next <= 0.0));
                    if left > 0.0 {
                        m.side -= across * covered / left;
                        m.lag -= back * covered / left;
                    }
                    if m.finished {
                        m.walk = None;
                    }
                    covered
                }
            };
            m.speed = covered / TICK_SECONDS;
            m.distance -= covered;
            m.lag += s.covered;
            moving |= covered > 0.0;
            all_finished &= m.finished;
        }
        let going = moving && !all_finished;
        u.march.plan = order.and_then(|mut o| {
            o.advance(0.0);
            (step.is_some() || going).then_some(o)
        });
        // The reform state ends when every man's move has ended, stamping the men in place
        // (`0x00581CA0` → `0x00557D80`, CONFIRMED static). The order's end leaves the unit idle: its
        // next tick's idle state enters the reform state (`LandUnit::reslot_tick`).
        if u.march.reforming && order.is_none() && !going {
            u.march.in_place_at = tick;
        }
        u.march.reforming = order.is_none() && going;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::fatigue::KvFatigue;
    use crate::battle::ground::{BattleGround, GridSpec, GroundTypeGrid, MovementClass, speed_table};
    use crate::battle::morale::KvMorale;

    /// 40 men in 2 ranks of 20, 20 × 2 m, walking at 2 m/s from the origin towards +x on `ground`.
    fn marching(ground: BattleGround, dest: (f32, f32)) -> Battle {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.ground = std::sync::Arc::new(ground);
        let mut u = LandUnit::new(0, 0, 40, (0.0, 0.0));
        (u.walk_speed, u.run_speed, u.acceleration, u.deceleration) = (2.0, 4.0, 3.5, 10.0);
        (u.formation_files, u.formation_width, u.formation_rank_spacing, u.facing) = (20, 20.0, 1.0, 0.0);
        u.movement_class = MovementClass::Infantry;
        u.destination = Some(dest);
        b.add_unit(u);
        b
    }

    /// The +y half of a 400 m map is mud (infantry ×0.8), the −y half grassland: a unit walking
    /// along y = 0 towards +x has its left files (right < 0, at y > 0) in the mud.
    fn half_mud() -> BattleGround {
        BattleGround {
            types: Some(GroundTypeGrid { spec: GridSpec { cols: 1, rows: 2, width: 400.0, height: 400.0 }, cells: vec![4, 3] }),
            heights: None,
            speed_modifiers: speed_table([("mud", [0.6, 0.65, 0.8, 0.8])]),
        }
    }

    #[test]
    fn places_and_frame_match_the_drawn_block() {
        assert_eq!(formation_place(0, 20, 2, 20.0, 2.0), (-9.5, -0.5));
        assert_eq!(formation_place(39, 20, 2, 20.0, 2.0), (9.5, 0.5));
        // Facing +x the right hand is −y and the back −x; facing +y the right hand is +x.
        let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        assert!(near(unit_frame_offset(0.0, (1.0, 0.0)), (0.0, -1.0)));
        assert!(near(unit_frame_offset(0.0, (0.0, 1.0)), (-1.0, 0.0)));
        assert!(near(unit_frame_offset(std::f32::consts::FRAC_PI_2, (1.0, 0.0)), (1.0, 0.0)));
    }

    /// One layout: the front rank the view draws (`man_rank` 0) is the unit's files, the men who
    /// fire, whatever the ranks; the places follow the same grid.
    #[test]
    fn the_front_rank_is_the_units_files() {
        let mut u = LandUnit::new(0, 0, 45, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_rank_spacing) = (15, 15.0, 1.0);
        assert_eq!(u.formation_grid(), (15, 3));
        for i in 0..45 {
            assert_eq!(u.man_rank(i) == 0, i < u.files() as usize, "man {i}");
            assert_eq!(u.man_place(i).1, (u.man_rank(i) as f32 - 1.0) * 1.0, "man {i} on his rank's line");
        }
    }

    /// A formation of unknown size (width 0) still spreads its men (the `width` / `depth` fallbacks),
    /// not all on one spot.
    #[test]
    fn a_formation_of_unknown_size_spreads_its_men() {
        let u = LandUnit::new(0, 0, 40, (0.0, 0.0));
        assert_eq!((u.formation_width, u.formation_rank_spacing), (0.0, 0.0));
        let places: Vec<_> = (0..40).map(|i| u.man_place(i)).collect();
        for (i, a) in places.iter().enumerate() {
            for b in &places[i + 1..] {
                assert!((a.0 - b.0).abs() + (a.1 - b.1).abs() > 0.1, "{a:?} vs {b:?}");
            }
        }
    }

    /// The re-issue gate of `0x00584E10`: every man at the first issue; at a re-issue a finished
    /// man only while the issue point is short of the path's end.
    #[test]
    fn the_reissue_gate_skips_finished_men_at_the_end() {
        let first = TimedMove::plan(2.0, 3.0, None, 0);
        assert!(first.at_end && reissued(&first, true));
        let at_end = TimedMove::next(Some(first), 2.0, 3.0, None, 11);
        assert!(!at_end.first && at_end.at_end);
        assert!(reissued(&at_end, false) && !reissued(&at_end, true));
        let short = TimedMove::next(Some(first), 2.0, 100.0, None, 11);
        assert!(!short.at_end && reissued(&short, true));
    }

    /// The state hash covers every man's march state and the men's order, so a desync in them shows.
    #[test]
    fn the_state_hash_covers_the_march() {
        let mut b = marching(half_mud(), (1000.0, 0.0));
        for _ in 0..30 {
            b.step();
        }
        let base = b.state_hash();
        let changed = |f: &dyn Fn(&mut Battle)| {
            let mut c = b.clone();
            f(&mut c);
            c.state_hash() != base
        };
        assert!(changed(&|c| c.units[0].march.men[3].distance += 0.25));
        assert!(changed(&|c| c.units[0].march.men[3].finished = true));
        assert!(changed(&|c| c.units[0].march.men[3].kept = Some(2.5)));
        assert!(changed(&|c| c.units[0].march.plan = None));
        assert!(changed(&|c| c.units[0].march.plan.as_mut().unwrap().at_end ^= true));
        assert!(changed(&|c| c.units[0].march.kind = MoveKind::Charge));
        assert!(changed(&|c| c.units[0].march.laid = None));
        assert!(changed(&|c| c.units[0].march.laid.as_mut().unwrap().ranks += 1));
        assert!(changed(&|c| c.units[0].march.reslot = true));
        assert!(changed(&|c| c.units[0].march.reslot_in += 1));
        assert!(changed(&|c| c.units[0].march.last_men += 1));
        assert!(changed(&|c| c.units[0].march.measured = None));
        assert!(changed(&|c| c.units[0].march.measured.as_mut().unwrap().radius += 0.25));
        assert!(changed(&|c| c.units[0].march.measured.as_mut().unwrap().heading ^= 1));
        assert!(changed(&|c| c.units[0].timed_move.as_mut().unwrap().distance += 0.25));
        assert!(changed(&|c| c.units[0].timed_move.as_mut().unwrap().target = Some((5.0, 5.0))));
    }

    /// One grid: with the starting men and no width (the files fall back to the frontage), the
    /// depth is the grid's ranks times the rank spacing, and a front rank wider than the men is
    /// capped at the men (the callers of `0x00642C90`: `0x0061CAB0`, `0x00646B00`, `0x00646C70`,
    /// `0x00646D40`, `0x0065A410`, `0x0065A6D0`), so
    /// the block stays centred on the unit and only as wide as its men.
    #[test]
    fn the_depth_and_the_places_come_from_one_grid() {
        let mut u = LandUnit::new(0, 0, 30, (0.0, 0.0));
        u.formation_rank_spacing = 1.5;
        let f = u.formation();
        assert_eq!(f.ranks, 30usize.div_ceil(f.files), "{f:?}");
        assert!((f.depth - f.ranks as f32 * 1.5).abs() < 1e-5, "{f:?}");
        // The back rank's men stand on the block's back line, the front rank's on its front line.
        let back = u.man_place(29).1;
        assert!((back - (f.depth / 2.0 - 0.75)).abs() < 1e-4 && (u.man_place(0).1 + back).abs() < 1e-4, "{f:?}");
        // 12 men with 40 files: one rank of 12, centred, and the block only 12 files wide (the
        // men keep their 1 m file pitch, not spread over the 40 m).
        let mut few = LandUnit::new(0, 0, 12, (0.0, 0.0));
        (few.formation_files, few.formation_width) = (40, 40.0);
        assert_eq!(few.formation_grid(), (12, 1));
        assert!((few.width() - 11.0).abs() < 1e-4, "measured: the 12 centres 11 m apart, radius 0: {}", few.width());
        assert!((few.formation().width - 12.0).abs() < 1e-4, "{:?}", few.formation());
        let xs: f32 = (0..12).map(|i| few.man_place(i).0).sum();
        assert!(xs.abs() < 1e-3, "centred: {xs}");
        let spread = (0..12).map(|i| few.man_place(i).0.abs()).fold(0.0f32, f32::max);
        assert!(spread <= 6.0, "within the 12 m block: {spread}");
    }

    /// After losses the files, the width and the grid all follow the living men (unit `+0x18`, the
    /// count the fire step `0x005694D0` caps the files at and `0x00581200` lays out), with and
    /// without a known formation width, so the width and the files never disagree.
    #[test]
    fn the_files_the_width_and_the_grid_follow_the_living_men() {
        let mut u = LandUnit::new(0, 0, 120, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_rank_spacing) = (40, 40.0, 1.0);
        assert_eq!((u.files(), u.formation_grid()), (40, (40, 3)));
        // 70 left: still 40 files, but two ranks, so the block is two ranks deep.
        u.men = 70;
        assert_eq!((u.files(), u.formation_grid()), (40, (40, 2)));
        assert!((u.formation().width - 40.0).abs() < 1e-4 && (u.formation().depth - 2.0).abs() < 1e-4);
        // 25 left: fewer men than files, so one rank of 25, 25 m wide.
        u.men = 25;
        assert_eq!((u.files(), u.formation_grid()), (25, (25, 1)));
        assert!((u.formation().width - 25.0).abs() < 1e-4, "width {}", u.formation().width);
        // No formation size: the fallback files and the fallback width read the same living men.
        let mut v = LandUnit::new(1, 0, 120, (0.0, 0.0));
        v.men = 60;
        assert_eq!(v.files(), 20);
        assert_eq!(v.formation_grid(), (20, 3));
        assert!((v.formation().width - v.files() as f32 * crate::battle::abilities::FILE_SPACING_M).abs() < 1e-4);
    }

    /// A unit at its move's end keeps its slots between re-lays (CONFIRMED, [`LandUnit::formation`]):
    /// a loss leaves the laid grid, width and depth (the living men fill its first slots) and
    /// re-slots the men 20 ticks after the last loss (`0x005755D0`, `0x00580A30`); every death
    /// restarts the wait ([`RESLOT_TICKS`]).
    #[test]
    fn the_men_keep_their_slots_until_the_reslot() {
        let mut u = LandUnit::new(0, 0, 120, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_rank_spacing) = (40, 40.0, 1.0);
        u.timed_move = Some(TimedMove { at_end: true, ..TimedMove::plan(2.0, 5.0, None, 0) });
        u.reslot_tick(0);
        assert_eq!(u.formation_grid(), (40, 3));
        // 25 left in tick T: the laid grid, width and depth stay (fresh it would be one rank of 25).
        u.men = 25;
        u.reslot_tick(0);
        assert_eq!((u.formation_grid(), u.formation().width, u.formation().depth), ((40, 3), 40.0, 3.0), "width and depth: the laid block");
        for _ in 0..19 {
            u.reslot_tick(0);
        }
        assert_eq!(u.formation_grid(), (40, 3), "tick T + 19: still laid");
        u.reslot_tick(0);
        assert_eq!((u.formation_grid(), u.formation().width, u.formation().depth), ((25, 1), 25.0, 1.0), "tick T + 20");
        assert!(!u.march.reslot && u.march.reslot_in == 0);
        // A second loss within the wait restarts it from that loss.
        u.men = 24;
        u.reslot_tick(0);
        for _ in 0..10 {
            u.reslot_tick(0);
        }
        u.men = 23;
        u.reslot_tick(0);
        for _ in 0..19 {
            u.reslot_tick(0);
        }
        assert_eq!(u.formation_grid(), (25, 1), "20 ticks after the first loss: restarted");
        u.reslot_tick(0);
        assert_eq!(u.formation_grid(), (23, 1), "20 ticks after the second");
    }

    /// The move order lays the slots out over the living men every tick while its issue point is
    /// short of the path's end (`0x00581AE0` → `0x00576B30`, order `+0x3C` clear), so a loss
    /// re-lays at once; at the end a loss waits for the re-slot (20 ticks); and only a due re-slot
    /// clears the pending one, also while the every-tick layout runs.
    #[test]
    fn a_move_short_of_its_end_re_lays_every_tick() {
        let mut u = LandUnit::new(0, 0, 120, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_rank_spacing) = (40, 40.0, 1.0);
        u.timed_move = Some(TimedMove::plan(2.0, 1000.0, None, 0));
        assert!(!u.timed_move.is_some_and(|t| t.at_end));
        u.reslot_tick(0);
        u.men = 25;
        u.reslot_tick(0);
        assert_eq!((u.formation_grid(), u.formation().width, u.formation().depth), ((25, 1), 25.0, 1.0), "re-laid at once");
        assert!(u.march.reslot && u.march.reslot_in == RESLOT_TICKS - 1, "the re-slot still pending");
        for _ in 0..19 {
            u.reslot_tick(0);
            assert!(u.march.reslot, "pending until due");
        }
        u.reslot_tick(0);
        assert!(!u.march.reslot, "cleared when due (tick T + 20)");
        // At the path's end a loss keeps the laid grid until the re-slot.
        if let Some(t) = u.timed_move.as_mut() {
            t.at_end = true;
        }
        u.men = 20;
        u.reslot_tick(0);
        assert_eq!((u.formation_grid(), u.formation().width), ((25, 1), 25.0), "tick T: laid");
        for _ in 0..19 {
            u.reslot_tick(0);
        }
        assert_eq!(u.formation_grid(), (25, 1), "tick T + 19: laid");
        u.reslot_tick(0);
        assert_eq!((u.formation_grid(), u.formation().width), ((20, 1), 20.0), "tick T + 20: re-slotted");
    }

    /// A unit's first tick lays the slots out (the exe's unit starts with a re-slot pending,
    /// `0x00518DC0`), and in a battle a loss while the unit moves short of its end re-lays the
    /// slots in the next tick, the re-slot still pending.
    #[test]
    fn a_moving_unit_re_lays_after_a_loss_at_once() {
        let mut b = marching(half_mud(), (1000.0, 0.0));
        assert!(b.units[0].march.laid.is_none());
        b.step();
        assert_eq!(b.units[0].march.laid.map(|f| (f.files, f.ranks)), Some((20, 2)));
        b.units[0].men = 15;
        b.step();
        let u = &b.units[0];
        assert!(u.timed_move.is_some_and(|t| !t.at_end), "{:?}", u.timed_move);
        assert!(u.march.reslot && u.march.reslot_in == RESLOT_TICKS - 1 && u.formation_grid() == (15, 1), "{:?}", u.march);
    }

    /// A move that follows an enemy is a new order, issued to every man, when the enemy moved
    /// (`0x0054D9F0` builds a new move order, a first issue of `0x00584E10`); an unchanged target
    /// re-plans on the schedule.
    #[test]
    fn a_moved_target_is_a_first_issue() {
        let first = TimedMove::plan(2.0, 3.0, Some((10.0, 0.0)), 0);
        let later = TimedMove { ticks: 4, first: false, ..first };
        let same = TimedMove::next(Some(later), 2.0, 3.0, Some((10.0, 0.0)), 4);
        assert_eq!(same.ticks, 4, "the same order runs on");
        let moved = TimedMove::next(Some(later), 2.0, 3.0, Some((11.0, 0.0)), 4);
        assert!(moved.first && moved.ticks == 0 && reissued(&moved, true), "{moved:?}");
    }

    /// Men who finished under an order at its end stay finished at a re-issue with the same target
    /// (the exe's gate), and are issued again when the enemy they advance on moved, so none of them
    /// stays frozen while the unit follows it.
    #[test]
    fn finished_men_follow_an_enemy_that_moves_off() {
        let mut b = marching(BattleGround::default(), (1000.0, 0.0));
        b.units[0].destination = None;
        let target = Some((10.0, 0.0));
        let running = TimedMove { ticks: 5, first: false, ..TimedMove::plan(2.0, 3.0, target, 0) };
        assert!(running.at_end);
        let finish_all = |b: &mut Battle| {
            for m in b.units[0].march.men.iter_mut() {
                (m.finished, m.lag, m.speed) = (true, 0.0, 0.0);
            }
            b.units[0].march.men.resize(40, ManMarch { finished: true, ..ManMarch::default() });
            b.units[0].march.plan = Some(running);
        };
        let step = UnitStep { pos: (0.0, 0.0), facing: 0.0, dir: (1.0, 0.0), len: 3.0, covered: 0.2, is_order: false, fatigue: 1.0 };
        finish_all(&mut b);
        let same = TimedMove::next(Some(running), 2.0, 3.0, target, 5);
        b.march_men(0, Some(step), Some(same));
        assert!(b.units[0].march.men.iter().all(|m| m.finished && m.speed == 0.0), "the same target: no re-issue");
        finish_all(&mut b);
        let moved = TimedMove::next(Some(running), 2.0, 3.0, Some((10.5, 0.0)), 5);
        b.march_men(0, Some(step), Some(moved));
        assert!(b.units[0].march.men.iter().all(|m| !m.finished && m.speed > 0.0), "{:?}", b.units[0].march.men[0]);
    }

    #[test]
    fn on_even_ground_every_man_keeps_his_place_and_the_unit_speed() {
        let mut b = marching(BattleGround::default(), (1000.0, 0.0));
        for _ in 0..100 {
            b.step();
            let u = &b.units[0];
            assert_eq!(u.march.men.len(), 40);
            for m in &u.march.men {
                assert!(m.lag.abs() < 1e-4 && (m.speed - u.speed).abs() < 1e-4, "{m:?} vs {}", u.speed);
            }
        }
    }

    /// The exe re-issues each man's timed move to his own place D ahead (`0x00584E10`): a man on
    /// slower ground falls behind, and each re-issue's d = D + his lag speeds him up, so his lag
    /// levels off instead of growing 0.2 × 2 m/s every second.
    #[test]
    fn a_man_in_the_mud_falls_behind_and_is_caught_up_by_each_reissue() {
        let mut b = marching(half_mud(), (1000.0, 0.0));
        let lag = |b: &Battle, i: usize| b.units[0].march.men[i].lag;
        for _ in 0..200 {
            b.step();
        }
        let (mud, grass) = (lag(&b, 0), lag(&b, 39));
        assert!(grass.abs() < 1e-3, "on the unit's ground: {grass}");
        assert!(mud > 0.5, "behind in the mud: {mud}");
        // His speeds differ from the unit's (and so do his clips), man by man.
        assert!((b.units[0].march.men[0].speed - b.units[0].speed).abs() > 1e-3);
        for _ in 0..200 {
            b.step();
        }
        let later = lag(&b, 0);
        assert!((later - mud).abs() < 0.2 && later < 4.0, "levelled off: {mud} then {later}");
    }

    /// Nothing is issued at the unit's stop (`0x00584E10` runs only at `0x0051A8FD` and on the
    /// re-issue schedule): each man's point is already his place at the path's end, so his distance
    /// left is his lag plus what the unit has left (d = len + lag holds through the stop), not
    /// D + his lag.
    #[test]
    fn at_the_stop_each_mans_distance_is_his_lag() {
        let mut b = marching(half_mud(), (40.0, 0.0));
        for _ in 0..400 {
            b.step();
            let u = &b.units[0];
            // The unit's distance left to its destination (short of it by at most the 0.5 m
            // minimum braking distance once stopped).
            let len = 40.0 - u.position.0;
            if u.destination.is_none() {
                for m in u.march.men.iter().filter(|m| !m.finished) {
                    assert!((m.distance - (len + m.lag)).abs() < 0.05, "{m:?} at {len}");
                }
                return;
            }
            for m in u.march.men.iter().filter(|m| u.march.plan.is_some_and(|p| p.at_end) && !m.finished) {
                assert!((m.distance - (len + m.lag)).abs() < 0.05, "{m:?} at {len}");
            }
        }
        panic!("the unit never arrived");
    }

    /// The issue point stops at the path's end (`0x007F0790` with the clamp flag, `issue_point`),
    /// and the order there keeps its speed: a unit arriving keeps walking at its gait speed instead
    /// of slowing as d / t to the end.
    #[test]
    fn the_issue_point_stops_at_the_end_and_keeps_the_speed() {
        assert_eq!(crate::battle::model::issue_point(10.0, 25.0), (10.0, false));
        assert_eq!(crate::battle::model::issue_point(10.0, 4.0), (4.0, true));
        assert_eq!(crate::battle::model::issue_point(10.0, -1.0), (0.0, true));
        let mut b = marching(BattleGround::default(), (12.0, 0.0));
        let (mut speeds, mut at_end) = (Vec::new(), None);
        while b.units[0].destination.is_some() && speeds.len() < 200 {
            b.step();
            speeds.push(b.units[0].speed);
            if at_end.is_none() && b.units[0].timed_move.is_some_and(|t| t.at_end) {
                at_end = Some(speeds.len());
            }
        }
        // From the re-issue whose point is the end (the third issue, step 23: len < D = 10 m) to the
        // braking distance (≥ 0.5 m), the unit walks at its kept speed, never slowing as d / t
        // (about 8 / 5 there).
        let at_end = at_end.expect("the issue point reached the end");
        assert!(at_end <= 2 * 11 + 1, "at the end by the third issue: {at_end}");
        let cruise = &speeds[at_end - 1..speeds.len() - 6];
        assert!(cruise.iter().all(|&v| v >= 2.0 - 1e-3 && (v - cruise[0]).abs() < 1e-3), "{speeds:?}");
    }

    /// Once the unit stops, a man still behind walks the rest of his lag and brakes for his place
    /// as the unit does; then every man has finished and the march ends.
    #[test]
    fn behind_men_walk_to_their_places_after_the_unit_stops() {
        let mut b = marching(half_mud(), (40.0, 0.0));
        let mut at_stop = None;
        for _ in 0..400 {
            b.step();
            if at_stop.is_none() && b.units[0].destination.is_none() {
                at_stop = Some(b.units[0].march.men[0].lag);
            }
        }
        let at_stop = at_stop.expect("the unit arrived");
        let u = &b.units[0];
        assert!(at_stop > 0.5, "behind at the stop: {at_stop}");
        assert!(u.march.men[0].lag < at_stop && u.march.men[0].lag <= 0.5 + 0.2, "walked to his place (short by the 0.5 m minimum braking and a step): {:?}", u.march.men[0]);
        assert!(u.march.plan.is_none() && u.march.men.iter().all(|m| m.speed == 0.0 && m.finished), "every man stands");
    }

    /// Men in a line along y, one metre apart, all facing `heading`.
    fn line(n: usize, heading: u16, radius: f32) -> impl Iterator<Item = SummaryMan> + Clone {
        (0..n).map(move |i| SummaryMan { pos: (0.0, i as f32), heading, radius })
    }

    /// `0x0053D1A0`: a line of men along y facing +x. The men's spread lies along the line, they
    /// face across it, so the width is the line's span plus two man radii and the depth two radii;
    /// the centre is the line's middle, the radius (`0x0053C3D0`) the farthest man from it, and the
    /// front centre half the depth ahead.
    #[test]
    fn the_box_of_a_line_is_measured_from_the_men() {
        let east = heading_of((1.0, 0.0));
        assert_eq!(east, 0x4000);
        let b = measure_squad_box(line(10, east, 0.35), 0).unwrap();
        assert_eq!(b.heading, east);
        assert!((b.width - 9.7).abs() < 1e-4 && (b.depth - 0.7).abs() < 1e-4, "{b:?}");
        assert!(b.centre.0.abs() < 1e-4 && (b.centre.1 - 4.5).abs() < 1e-4, "{b:?}");
        assert!((b.radius - 4.5).abs() < 1e-4, "{b:?}");
        let f = b.front_centre();
        assert!((f.0 - 0.35).abs() < 1e-4 && (f.1 - 4.5).abs() < 1e-4, "{f:?}");
        assert_eq!(measure_squad_box(line(0, east, 0.35), 0), None);
    }

    /// A man at least `μ + 2σ` from the men's mean is left out of the box's spans but not out of the
    /// radius (the farthest man).
    #[test]
    fn a_straggler_is_left_out_of_the_box_but_not_the_radius() {
        let east = heading_of((1.0, 0.0));
        let men = line(20, east, 0.0).chain(std::iter::once(SummaryMan { pos: (0.0, 200.0), heading: east, radius: 0.0 }));
        let b = measure_squad_box(men, east).unwrap();
        assert!((b.width - 19.0).abs() < 1e-3 && (b.centre.1 - 9.5).abs() < 1e-3, "{b:?}");
        assert!((b.radius - 190.5).abs() < 1e-3, "{b:?}");
    }

    /// One man: his position, heading and radius as width and depth, and radius 0.
    #[test]
    fn one_man_is_his_own_box() {
        let m = SummaryMan { pos: (3.0, 4.0), heading: 0x1234, radius: 0.35 };
        let b = measure_squad_box(std::iter::once(m), 0).unwrap();
        assert_eq!(b, SquadBox { centre: (3.0, 4.0), heading: 0x1234, width: 0.35, depth: 0.35, radius: 0.0 });
    }

    /// The heading vote: the most votes win when they reach `round(0.65 n)` (6 of 10, the x87
    /// rounding of 6.5); fewer, and the candidate nearest the last heading wins, which swaps the
    /// width and the depth when it lies along the line.
    #[test]
    fn the_heading_vote_falls_back_to_the_last_heading() {
        let (east, west) = (heading_of((1.0, 0.0)), heading_of((-1.0, 0.0)));
        assert_eq!(west, 0xC000);
        let split = |east_men: usize| line(10, 0, 0.0).enumerate().map(move |(i, m)| SummaryMan { heading: if i < east_men { east } else { west }, ..m });
        // 6 of 10: east wins, whatever the last heading.
        assert_eq!(measure_squad_box(split(6), 0).unwrap().heading, east);
        // 5 of 10: the nearest to the last heading.
        let near = |b: SquadBox, w: f32, d: f32| (b.width - w).abs() < 1e-4 && (b.depth - d).abs() < 1e-4;
        let b = measure_squad_box(split(5), west).unwrap();
        assert!(b.heading == west && near(b, 9.0, 0.0), "{b:?}");
        let b = measure_squad_box(split(5), 0x0100).unwrap();
        assert!(b.heading == 0 && near(b, 0.0, 9.0), "along the line: the width and depth swap: {b:?}");
    }

    /// A loss in a shape of kind 1 or 2 (vfunc `+0x30` false, `0x00462CB0`) makes the re-slot
    /// pending without restarting its countdown; the line's restarts it.
    #[test]
    fn a_loss_in_shape_kind_1_or_2_keeps_the_countdown() {
        let mut u = LandUnit::new(0, 0, 120, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_rank_spacing) = (40, 40.0, 1.0);
        u.timed_move = Some(TimedMove { at_end: true, ..TimedMove::plan(2.0, 5.0, None, 0) });
        u.reslot_tick(0);
        u.men = 110;
        u.reslot_tick(0);
        assert_eq!(u.march.reslot_in, RESLOT_TICKS - 1);
        for _ in 0..5 {
            u.reslot_tick(0);
        }
        // Ability 3 (enum value) gives shape kind 1: the next loss leaves the countdown running.
        u.active_abilities = 1 << 3;
        assert_eq!(u.shape_kind(), 1);
        u.men = 100;
        u.reslot_tick(0);
        assert!(u.march.reslot && u.march.reslot_in == RESLOT_TICKS - 7, "{:?}", u.march);
        // A kind-1 loss with the countdown out re-slots in the same tick.
        for _ in 0..RESLOT_TICKS {
            u.reslot_tick(0);
        }
        assert!(!u.march.reslot && u.march.reslot_in == 0);
        u.men = 70;
        u.reslot_tick(0);
        assert!(!u.march.reslot && u.formation_grid() == (40, 2), "{:?}", u.march);
        // Without an ability the line (kind 0); the exe's order of the abilities picks the kind.
        u.active_abilities = (1 << 4) | (1 << 6);
        assert_eq!(u.shape_kind(), 3);
        u.active_abilities = 0;
        assert_eq!(u.shape_kind(), 0);
    }

    /// A new order to a goal behind the men's front (`0x008102F0` sets the path request's `+0x8C`)
    /// clears the pending re-slot (`0x0051A530` lays out with the re-slot argument); a goal ahead
    /// leaves it pending.
    #[test]
    fn an_order_behind_the_front_clears_the_pending_reslot() {
        for (dir, cleared) in [((-1.0, 0.0), true), ((1.0, 0.0), false)] {
            let mut b = marching(BattleGround::default(), (1000.0, 0.0));
            b.units[0].reslot_tick(0);
            (b.units[0].march.reslot, b.units[0].march.reslot_in) = (true, 10);
            let step = UnitStep { pos: (0.0, 0.0), facing: 0.0, dir, len: 50.0, covered: 0.2, is_order: true, fatigue: 1.0 };
            b.march_men(0, Some(step), Some(TimedMove::plan(2.0, 50.0, None, 0)));
            assert_eq!(b.units[0].march.reslot, !cleared, "goal along {dir:?}");
        }
    }

    /// The order's update lays the slots out once per tick (`0x00581AE0` → `0x00576B30`): a
    /// re-issue short of the path's end, which the tick's layout step already laid out, adds no
    /// second assignment round.
    #[test]
    fn a_reissue_tick_assigns_the_men_once() {
        let mut b = marching(BattleGround::default(), (1000.0, 0.0));
        let mut reissues = 0;
        for _ in 0..60 {
            let before = b.units[0].march.assignments;
            b.step();
            let t = b.units[0].timed_move.expect("still moving");
            assert!(!t.at_end);
            reissues += usize::from(!t.first && t.ticks == 1);
            assert_eq!(b.units[0].march.assignments - before, 1, "one round in tick {}", b.tick);
        }
        assert!(reissues >= 3, "{reissues} re-issues");
    }

    /// A re-issue assigns no slots (`0x00584E10` with 0 only computes the slot points; its one
    /// assignment, `0x0057AC10`, runs once per order): at the path's end, where the order's update
    /// lays out only for a due re-slot (`0x00581AE0`, `+0x3C` set), the men keep their slots
    /// through every re-issue while the unit still moves.
    #[test]
    fn an_at_end_reissue_does_not_reassign() {
        // 9 m to go at 2 m/s: the 10 m issue point is the path's end from the start.
        let mut b = marching(BattleGround::default(), (9.0, 0.0));
        b.step();
        let laid = b.units[0].march.assignments;
        let mut reissues = 0;
        while let Some(t) = b.units[0].timed_move {
            assert!(t.at_end);
            b.step();
            reissues += usize::from(b.units[0].timed_move.is_some_and(|t| !t.first && t.ticks == 1));
            assert_eq!(b.units[0].march.assignments, laid, "no round in tick {}", b.tick);
            assert!(b.tick < 200, "the unit arrives");
        }
        assert!(reissues >= 2, "{reissues} re-issues");
    }

    /// The room to evade (`0x0057A6F0`): the strip starts at the box's front centre (`+0x9C`), not
    /// the unit's position, and is at least the squad's radius long (`+0x670`), so a unit whose
    /// walk in 3 s would fit is still held back by its own size.
    #[test]
    fn the_evade_strip_runs_the_squad_radius_from_the_front_centre() {
        let mut b = marching(BattleGround::default(), (1000.0, 0.0));
        b.units[0].walk_speed = 1.0;
        b.units[0].reslot_tick(0);
        let r = b.units[0].radius();
        assert!(r > 9.0 && r < 10.0, "radius {r}");
        let front = b.units[0].squad_box().front_centre().0;
        assert!(front - b.units[0].position.0 > 0.2, "the front centre is ahead of the position: {front}");
        let east = heading_of((1.0, 0.0));
        b.playable_area = Some([-20.0, -20.0, 6.0, 20.0]);
        assert!(!b.evade_room(0, east), "3 m of walk fits, the {r} m radius does not");
        b.playable_area = Some([-20.0, -20.0, front + r - 0.1, 20.0]);
        assert!(!b.evade_room(0, east), "the strip starts at the front centre");
        b.playable_area = Some([-20.0, -20.0, front + r + 0.1, 20.0]);
        assert!(b.evade_room(0, east));
    }

    /// `0x005281D0`: the squared distance, plus up to 1 for a slot behind the man (he faces the
    /// unit's facing, ahead is −back).
    #[test]
    fn a_slot_behind_costs_more_than_one_as_far_ahead() {
        assert_eq!(slot_cost((0.0, 0.0), (0.0, 0.0)), 0.0);
        assert_eq!(slot_cost((0.0, 0.0), (0.0, -1.0)), 1.0, "ahead");
        assert_eq!(slot_cost((0.0, 0.0), (1.0, 0.0)), 1.5, "to his right");
        assert_eq!(slot_cost((0.0, 0.0), (0.0, 1.0)), 2.0, "behind");
        assert_eq!(slot_cost((0.0, 0.0), (0.0, -3.0)), 9.0);
    }

    /// `0x00502770`: the full assignment swaps until no pair improves; the partial one swaps the
    /// worst pair once, with the partner that lowers the sum most (the first on a tie), and only
    /// pairs costing under 16.
    #[test]
    fn the_full_assignment_untangles_and_the_partial_one_swaps_once() {
        let slots = |k: usize| (k as f32, 0.0);
        let mut scratch = SlotScratch::default();
        // Three men in a cycle: each one slot off.
        let cycle = [(1.0f32, 0.0f32), (2.0, 0.0), (0.0, 0.0)];
        let mut men = cycle;
        assign_slots(&mut men, |m| *m, slots, false, &mut scratch);
        assert_eq!(men, [(0.0, 0.0), (2.0, 0.0), (1.0, 0.0)], "the worst (man 2) swapped with man 0 (a tie with man 1)");
        let mut men = cycle;
        assign_slots(&mut men, |m| *m, slots, true, &mut scratch);
        assert_eq!(men, [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)], "every man at his own slot");
        // Two men 10 m off: the partial assignment leaves them (cost 100.5 ≥ 16), the full swaps.
        let slots = |k: usize| (k as f32 * 10.0, 0.0);
        let mut men = [(10.0f32, 0.0f32), (0.0, 0.0)];
        assign_slots(&mut men, |m| *m, slots, false, &mut scratch);
        assert_eq!(men, [(10.0, 0.0), (0.0, 0.0)]);
        assign_slots(&mut men, |m| *m, slots, true, &mut scratch);
        assert_eq!(men, [(0.0, 0.0), (10.0, 0.0)]);
    }

    /// A re-slot leaves every man where he stands: only his slot, and his offset from it, change.
    #[test]
    fn a_reslot_keeps_every_man_where_he_stands() {
        let mut u = LandUnit::new(0, 0, 120, (0.0, 0.0));
        (u.formation_files, u.formation_width, u.formation_rank_spacing) = (40, 40.0, 1.0);
        u.reslot_tick(0);
        u.men = 25;
        let at = |u: &LandUnit| {
            let mut by_figure: Vec<_> = u.men_standing().zip(&u.march.men).map(|(s, m)| (m.figure, s.pos)).collect();
            by_figure.sort_by_key(|p| p.0);
            by_figure
        };
        let before = at(&u);
        u.relay(true);
        let after = at(&u);
        assert_eq!(before.len(), after.len());
        for (b, a) in before.iter().zip(&after) {
            assert!(b.0 == a.0 && (b.1.0 - a.1.0).abs() < 1e-4 && (b.1.1 - a.1.1).abs() < 1e-4, "{b:?} -> {a:?}");
        }
        assert_eq!(u.formation_grid(), (25, 1));
        assert!(u.march.men[..25].iter().any(|m| m.side != 0.0), "the men of the old second rank are off their new slots");
    }

    /// `0x005736B0` → `0x0055F160` → `0x00653560`: a man more than 0.25 m from his slot is sent
    /// there, walking up to 5 m and running beyond.
    #[test]
    fn the_reform_walks_near_men_and_runs_far_ones() {
        let mut u = LandUnit::new(0, 0, 3, (0.0, 0.0));
        (u.walk_speed, u.run_speed) = (2.0, 4.0);
        u.march.ensure_men(3);
        (u.march.men[0].lag, u.march.men[1].lag, u.march.men[2].side) = (0.25, 3.0, -6.0);
        u.send_men_to_slots();
        let m = &u.march.men;
        assert_eq!((m[0].walk, m[1].walk, m[1].kind, m[2].walk, m[2].kind), (None, Some(2.0), MoveKind::Walk, Some(4.0), MoveKind::Run));
        assert!(u.march.reforming && u.march.active());
    }

    /// A standing unit's due re-slot waits for the reform state: while the men stand in place
    /// (within the drill tolerance plus 1 m on foot) the idle check stamps them and the gate stays
    /// shut; once their mean offset reaches it the stamp is cleared, the gate opens, the reform
    /// re-slots in full and sends the men to their slots, and they walk there and stand.
    #[test]
    fn a_due_reslot_walks_a_standing_units_men_to_their_slots() {
        let mut b = marching(BattleGround::default(), (0.0, 0.0));
        b.units[0].destination = None;
        b.units[0].formation_files = 40;
        // Tick 0 stamps 0, which reads as no stamp (`0x00557BA0`): tick 1 reforms again.
        b.step();
        b.step();
        assert!(b.units[0].march.idle_reformed && b.units[0].march.in_place_at == b.tick - 1);
        // One man 3 m off: the mean (3 / 40 m) is short of the 1 m tolerance, so no reform.
        b.units[0].march.men[5].lag = 3.0;
        b.units[0].march.reslot = true;
        for _ in 0..30 {
            b.step();
            assert!(!b.units[0].march.reforming && b.units[0].march.reslot, "the gate stays shut");
        }
        // Every man 3 m off: the next in-place check clears the stamp, the following tick reforms.
        for m in &mut b.units[0].march.men {
            m.lag = 3.0;
        }
        let mut reformed = None;
        for _ in 0..12 {
            b.step();
            if b.units[0].march.reforming {
                reformed = Some(b.units[0].march.relaid);
                break;
            }
        }
        assert_eq!(reformed, Some(Some(true)), "a full re-slot at the reform's entry");
        let m = b.units[0].march.men[5];
        assert!(!b.units[0].march.reslot && m.walk == Some(2.0) && m.kind == MoveKind::Walk && m.lag < 3.0, "{m:?}");
        for _ in 0..60 {
            b.step();
        }
        let u = &b.units[0];
        assert!(!u.march.reforming && u.march.men.iter().all(|m| m.walk.is_none() && m.speed == 0.0), "every man stands");
        assert!(u.march.men[5].lag <= 0.5 + 0.2 && u.march.men[5].lag > 0.0, "at his slot, short by the 0.5 m minimum braking and a step: {:?}", u.march.men[5]);
    }

    /// `0x00573770` / `0x00557BA0`: a new idle task always reforms; afterwards the gate is shut for
    /// 20 ticks after the men were stamped in place, and open once the stamp is older or cleared.
    #[test]
    fn the_idle_gate_holds_for_a_new_task_or_an_old_stamp() {
        let mut u = LandUnit::new(0, 0, 10, (0.0, 0.0));
        assert!(u.idle_gate(100), "a new idle task");
        u.march.idle_reformed = true;
        assert!(u.idle_gate(100), "no stamp");
        u.march.in_place_at = 90;
        assert!(!u.idle_gate(100) && !u.idle_gate(109));
        assert!(u.idle_gate(110), "the stamp is 20 ticks old");
        u.march.in_place_at = 0;
        assert!(u.idle_gate(100), "cleared");
    }

    /// The order's end: the reform state comes in the next idle tick (a new idle task), and ends
    /// with the men stamped in place, so the unit does not reform again while they stand.
    #[test]
    fn a_unit_reforms_once_after_its_orders_end() {
        let mut b = marching(BattleGround::default(), (8.0, 0.0));
        // The unit's first tick lays it out (its pending first lay-out) before the order starts.
        b.step();
        let mut reforms = 0;
        let mut was_idle = b.units[0].march.idle_reformed;
        for _ in 0..200 {
            b.step();
            let u = &b.units[0];
            // The reform state's entry: a new idle task (`+0x2C` clear) sets it.
            if u.march.idle_reformed && !was_idle {
                reforms += 1;
                // No re-slot due: the entry assigns nothing, so a move order issued in the same
                // tick still lays out.
                assert_eq!(u.march.relaid, None);
            }
            was_idle = u.march.idle_reformed;
        }
        let u = &b.units[0];
        assert!(u.march.idle_reformed && !u.march.reforming && u.march.in_place_at != 0, "{:?}", u.march.in_place_at);
        assert_eq!(reforms, 1, "one reform after the order's end");
    }

    /// `0x00557C80`: the stamp is cleared when the men's mean offset from the fresh lay-out
    /// reaches the drill tolerance plus 1 m (on foot) or 2.5 m (mounted), else set to the tick.
    #[test]
    fn the_in_place_check_measures_the_mean_offset() {
        let mut u = LandUnit::new(0, 0, 4, (0.0, 0.0));
        u.reslot_tick(1);
        u.training_tolerance = 0.2;
        for m in &mut u.march.men {
            m.lag = 1.1;
        }
        u.in_place_check(50);
        assert_eq!(u.march.in_place_at, 50, "1.1 m < 1.2 m");
        u.march.men[0].lag = 1.6;
        u.in_place_check(60);
        assert_eq!(u.march.in_place_at, 0, "a mean of 1.225 m reaches it");
        u.is_cavalry = true;
        u.in_place_check(70);
        assert_eq!(u.march.in_place_at, 70, "mounted: 2.7 m");
        u.men = 0;
        u.in_place_check(80);
        assert_eq!(u.march.in_place_at, 80, "no man counted");
    }

    /// `0x0057E1F0`: the light-infantry behaviour gives 1.5 m, else the training level's value.
    #[test]
    fn the_drill_tolerance_follows_the_training_level() {
        let mut u = LandUnit::new(0, 0, 4, (0.0, 0.0));
        u.training_tolerance = 0.13;
        assert_eq!(u.drill_tolerance(), 0.13);
        u.active_abilities = 1 << crate::battle::abilities::ability_ids::LIGHT_INFANTRY_BEHAVIOUR;
        assert_eq!(u.drill_tolerance(), LIGHT_INFANTRY_TOLERANCE_M);
    }

    /// `0x0053C180` in battle: the box is measured only on the ticks whose parity is the unit's.
    #[test]
    fn the_box_is_measured_on_alternate_ticks() {
        let mut u = LandUnit::new(0, 0, 10, (0.0, 0.0));
        u.refresh_summary(1);
        assert!(u.march.measured.is_none());
        u.refresh_summary(2);
        assert!(u.march.measured.is_some());
        let mut v = LandUnit::new(1, 0, 10, (0.0, 0.0));
        v.refresh_summary(2);
        assert!(v.march.measured.is_none());
        v.refresh_summary(3);
        assert!(v.march.measured.is_some());
    }

    /// The cost of the men's march (release: `cargo test -p ntw_sim --release -- --ignored
    /// march_cost --nocapture`): `march_men` and the squad box for 20 marching units of 160 men on
    /// a 64 × 64 height grid and a ground-type grid, per tick.
    #[test]
    #[ignore]
    fn march_cost() {
        use crate::battle::ground::HeightGrid;
        let n = 64u32;
        let heights = HeightGrid {
            spec: GridSpec { cols: n, rows: n, width: 2000.0, height: 2000.0 },
            heights: (0..n * n).map(|i| ((i % n) as f32 * 0.37).sin() * 3.0 + ((i / n) as f32 * 0.21).cos() * 2.0).collect(),
        };
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.ground = std::sync::Arc::new(BattleGround { heights: Some(heights), ..half_mud() });
        for k in 0..20 {
            let mut u = LandUnit::new(0, 0, 160, (-150.0, -190.0 + k as f32 * 19.0));
            (u.walk_speed, u.run_speed, u.acceleration, u.deceleration) = (2.0, 4.0, 3.5, 10.0);
            (u.formation_files, u.formation_width, u.formation_rank_spacing) = (40, 40.0, 1.0);
            u.movement_class = MovementClass::Infantry;
            b.add_unit(u);
        }
        let ticks = 1000;
        let mut plans: Vec<Option<TimedMove>> = vec![None; 20];
        let t = std::time::Instant::now();
        for tick in 0..ticks {
            for (idx, plan) in plans.iter_mut().enumerate() {
                let p = TimedMove::next(*plan, 2.0, 1000.0, None, tick);
                let (pos, facing) = (b.units[idx].position, b.units[idx].facing);
                let step = UnitStep { pos, facing, dir: (1.0, 0.0), len: 1000.0, covered: 0.2, is_order: true, fatigue: 1.0 };
                b.march_men(idx, Some(step), Some(p));
                let mut p = p;
                p.advance(0.2);
                *plan = Some(p);
            }
        }
        println!("march_men: {:.3} ms per tick for 3,200 men", t.elapsed().as_secs_f64() * 1e3 / ticks as f64);
        // The squad box measured at the start of each unit tick (`reslot_tick`).
        let t = std::time::Instant::now();
        let mut width = 0.0;
        for _ in 0..ticks {
            for u in &b.units {
                width += u.measure_box().map_or(0.0, |m| m.width);
            }
        }
        println!("measure_box: {:.3} ms per tick for 3,200 men ({width})", t.elapsed().as_secs_f64() * 1e3 / ticks as f64);
    }
}

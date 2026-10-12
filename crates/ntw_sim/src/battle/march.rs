//! Each man's own timed move in his unit's march (UNITS_TERRAIN_FIDELITY.md §1.10 "Each man's
//! timed move").
//!
//! How the exe moves the men of a marching unit (CONFIRMED, `0x00584E10`, the unit move order's
//! issue, run when the order is made `0x0051A8FD` and re-run every [`TIMED_MOVE_REPLAN_TICKS`] by
//! `0x00581B81`; these are its only two callers):
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
//! keeps his lag behind his place in the formation ([`ManMarch`]); his place at the issue point is
//! the issue point's distance ahead of his place now, so his d at an issue is that plus his lag. Once
//! the unit's move ends the order goes on ([`MenMarch::plan`]), re-issued on the same schedule with
//! the issue point where the unit stands (D = 0, his d is his lag), until every man has finished:
//! reached his place, or braked to a stop for it ([`brakes_for`]) as the exe's stop state does.
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
//! - a man ahead of his place when the move ends (on faster ground or a steeper downhill than the
//!   reference soldier's) stands there; the exe's soldier walks back to his place, with a turn ours
//!   does not draw;
//! - a march in which no man moved over a tick after the unit's move ended (a man on impassable
//!   ground) ends there; the exe's order would wait for him.

use super::TICK_SECONDS;
use super::model::{Battle, LandUnit, TimedMove, brakes_for, slope_multiplier, step_speed};
#[cfg(doc)]
use super::model::{TIMED_MOVE_PLANNED_SECONDS, TIMED_MOVE_REPLAN_TICKS, issue_point};

/// One man's state in his unit's march.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ManMarch {
    /// Metres he is behind his place in the formation, along the unit's facing (negative: ahead).
    pub lag: f32,
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

/// The men's march of one unit: each man's state (by man index, as the view draws them, sized to
/// the unit's starting men) and the timed-move order they share (issued in the same tick).
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
}

impl MenMarch {
    /// Whether any man is still moving or has a timed move left to walk.
    pub fn active(&self) -> bool {
        self.plan.is_some()
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
    /// Ranks the starting men fill.
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
    /// The formation's files and ranks over its starting men: the files are the front rank
    /// ([`LandUnit::files`], the men who fire with the default drill, capped at the men as the
    /// callers of `0x00642C90` cap them, `0x0061CAB0` / `0x00646B00` / `0x0065A410`), the ranks as
    /// many as the starting men fill (`ceil(men / files)`, `0x00619E60`, CONFIRMED). The one grid
    /// the men's places, the formation's width ([`LandUnit::width`]) and its depth
    /// ([`LandUnit::depth`]) come from.
    pub fn formation_grid(&self) -> (usize, usize) {
        let files = (self.files() as usize).max(1);
        (files, (self.max_men as usize).div_ceil(files).max(1))
    }

    /// The formation: [`LandUnit::formation_grid`] over [`LandUnit::width`] × [`LandUnit::depth`].
    /// The one layout the model and the view share.
    pub fn formation(&self) -> Formation {
        let (files, ranks) = self.formation_grid();
        Formation { files, ranks, width: self.width(), depth: self.depth() }
    }

    /// Man `i`'s place in the unit's frame ([`Formation::place`]).
    pub fn man_place(&self, i: usize) -> (f32, f32) {
        self.formation().place(i)
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
    /// their lags. `plan` is the order the unit followed this tick before its advance (`None`
    /// without a move), so the men use the same speed, planned time and time since the issue.
    pub(super) fn march_men(&mut self, idx: usize, step: Option<UnitStep>, plan: Option<TimedMove>) {
        // A standing unit whose men stand too (every tick of most units): nothing to do.
        if (step.is_none() || plan.is_none()) && self.units[idx].march.plan.is_none() {
            return;
        }
        let ground = std::sync::Arc::clone(&self.ground);
        let u = &mut self.units[idx];
        let count = u.max_men as usize;
        if u.march.men.len() != count {
            u.march.men.resize(count, ManMarch::default());
        }
        let (class, run_speed, accel, decel) = (u.movement_class, u.run_speed, u.acceleration, u.deceleration);
        // The order: the unit's while it moves; after it, the same order, re-issued on the same
        // schedule with the issue point where the unit stands (no path left: D = 0, at the end).
        // The men's copy keeps no kept speed (each man keeps his own) and no target (the unit's).
        let order = match (plan, step) {
            (Some(p), Some(_)) => TimedMove { kept: None, target: None, ..p },
            _ => match u.march.plan {
                Some(p) => TimedMove::next(Some(p), p.speed, 0.0, None),
                None => return,
            },
        };
        let issued = order.ticks == 0;
        if issued && step.is_some() {
            // The kind of the move the men are issued (`+0x220`, see `MoveKind`): a charge, a run
            // (the run option, or a rout, which runs), else a walk.
            u.march.kind = if u.charging {
                MoveKind::Charge
            } else if u.running || u.morale.is_routing_or_shattered() {
                MoveKind::Run
            } else {
                MoveKind::Walk
            };
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
        let alive = (u.men as usize).min(count);
        let (mut moving, mut all_finished) = (false, true);
        let formation = u.formation();
        for (i, m) in u.march.men[..alive].iter_mut().enumerate() {
            if issued && reissued(&order, m.finished) {
                // His place at the issue point: its distance ahead of his place now, plus his lag.
                m.distance = (order.distance + m.lag).max(0.0);
                m.kept = None;
                m.finished = false;
            }
            let covered = if m.finished {
                0.0
            } else {
                let order_speed = order.speed_for(run_speed, m.distance, &mut m.kept);
                // His own multiplier: the ground under him and the slope ahead of him (the unit's
                // rules, `slope_multiplier`), with the unit's fatigue.
                let place = formation.place(i);
                let (ox, oy) = unit_frame_offset(s.facing, (place.0, place.1 + m.lag));
                let at = (s.pos.0 + ox, s.pos.1 + oy);
                let modifier = ground.speed_modifier(at.0, at.1, class);
                let planned = (order.speed * s.fatigue * modifier * TICK_SECONDS).max(0.01);
                let gradient = (ground.height(at.0 + s.dir.0 * planned, at.1 + s.dir.1 * planned) - ground.height(at.0, at.1)) / planned;
                let multiplier = s.fatigue * modifier * slope_multiplier(gradient);
                // What he has left to his place at the goal; he brakes for it as the unit does.
                let left = s.len + m.lag;
                let braking = s.is_order && brakes_for(left, m.speed, decel);
                let wanted = if braking { 0.0 } else { (order_speed * multiplier).max(0.0) };
                let next = step_speed(m.speed, wanted, accel * multiplier, decel) * TICK_SECONDS;
                // Short of his place at the goal when he reaches it (as the unit covers `len.min(step)`).
                let covered = next.min(left.max(0.0));
                // His move finishes at his place at the path's end, or braked to a stop for it.
                m.finished = order.at_end && (left <= next || (braking && next <= 0.0));
                covered
            };
            m.speed = covered / TICK_SECONDS;
            m.distance -= covered;
            m.lag += s.covered - covered;
            moving |= covered > 0.0;
            all_finished &= m.finished;
        }
        let mut order = order;
        order.advance(0.0);
        u.march.plan = if step.is_some() || (moving && !all_finished) { Some(order) } else { None };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::fatigue::KvFatigue;
    use crate::battle::ground::{BattleGround, GridSpec, GroundTypeGrid, MovementClass, speed_table};
    use crate::battle::model::TIMED_MOVE_REPLAN_TICKS;
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
        let first = TimedMove::plan(2.0, 3.0, None);
        assert!(first.at_end && reissued(&first, true));
        let at_end = TimedMove::next(Some(TimedMove { ticks: TIMED_MOVE_REPLAN_TICKS, ..first }), 2.0, 3.0, None);
        assert!(!at_end.first && at_end.at_end);
        assert!(reissued(&at_end, false) && !reissued(&at_end, true));
        let short = TimedMove::next(Some(TimedMove { ticks: TIMED_MOVE_REPLAN_TICKS, ..first }), 2.0, 100.0, None);
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
        assert!(changed(&|c| c.units[0].timed_move.as_mut().unwrap().distance += 0.25));
        assert!(changed(&|c| c.units[0].timed_move.as_mut().unwrap().target = Some((5.0, 5.0))));
    }

    /// One grid: with the starting men and no width (the files fall back to the frontage), the
    /// depth is the grid's ranks times the rank spacing, and a front rank wider than the men is
    /// capped at the men (the callers of `0x00642C90`: `0x0061CAB0`, `0x00646B00`, `0x0065A410`), so
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
        assert!((few.width() - 12.0).abs() < 1e-4, "width {}", few.width());
        assert!((few.formation().width - 12.0).abs() < 1e-4, "{:?}", few.formation());
        let xs: f32 = (0..12).map(|i| few.man_place(i).0).sum();
        assert!(xs.abs() < 1e-3, "centred: {xs}");
        let spread = (0..12).map(|i| few.man_place(i).0.abs()).fold(0.0f32, f32::max);
        assert!(spread <= 6.0, "within the 12 m block: {spread}");
    }

    /// A move that follows an enemy is a new order, issued to every man, when the enemy moved
    /// (`0x0054D9F0` builds a new move order, a first issue of `0x00584E10`); an unchanged target
    /// re-plans on the schedule.
    #[test]
    fn a_moved_target_is_a_first_issue() {
        let first = TimedMove::plan(2.0, 3.0, Some((10.0, 0.0)));
        let later = TimedMove { ticks: 4, first: false, ..first };
        let same = TimedMove::next(Some(later), 2.0, 3.0, Some((10.0, 0.0)));
        assert_eq!(same.ticks, 4, "the same order runs on");
        let moved = TimedMove::next(Some(later), 2.0, 3.0, Some((11.0, 0.0)));
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
        let running = TimedMove { ticks: 5, first: false, ..TimedMove::plan(2.0, 3.0, target) };
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
        let same = TimedMove::next(Some(running), 2.0, 3.0, target);
        b.march_men(0, Some(step), Some(same));
        assert!(b.units[0].march.men.iter().all(|m| m.finished && m.speed == 0.0), "the same target: no re-issue");
        finish_all(&mut b);
        let moved = TimedMove::next(Some(running), 2.0, 3.0, Some((10.5, 0.0)));
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
        assert!(at_end <= 2 * TIMED_MOVE_REPLAN_TICKS as usize + 1, "at the end by the third issue: {at_end}");
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

    /// The cost of the men's march (release: `cargo test -p ntw_sim --release -- --ignored
    /// march_cost --nocapture`): `march_men` for 20 marching units of 160 men on a 64 × 64 height
    /// grid and a ground-type grid, per tick.
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
        for _ in 0..ticks {
            for (idx, plan) in plans.iter_mut().enumerate() {
                let p = TimedMove::next(*plan, 2.0, 1000.0, None);
                let (pos, facing) = (b.units[idx].position, b.units[idx].facing);
                let step = UnitStep { pos, facing, dir: (1.0, 0.0), len: 1000.0, covered: 0.2, is_order: true, fatigue: 1.0 };
                b.march_men(idx, Some(step), Some(p));
                let mut p = p;
                p.advance(0.2);
                *plan = Some(p);
            }
        }
        println!("march_men: {:.3} ms per tick for 3,200 men", t.elapsed().as_secs_f64() * 1e3 / ticks as f64);
    }
}

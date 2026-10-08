//! The unit-controller orders of the battle scripts that change a unit's mode rather than move it:
//! skirmish mode, deployable defences, special abilities, shot types, the script morale modes and
//! `defend_building` (BATTLE_FIDELITY.md §26).
//!
//! Every one of them is, in the original, a battle command (`BCQ_*`) queued by the Lua binding and
//! applied to each unit of the selection:
//!
//! | script call | binding | command (handler) | unit-level function |
//! |---|---|---|---|
//! | `skirmish(b)` | `0x00613B10` | `..._CHANGE_SKIRMISH` (`0x005C0770`) | `0x005602A0`: unit `+0xD9C` = b |
//! | `select_deployable_object(s)` | `0x00613D50` | `..._DEPLOYABLE_ITEM_SELECTION` (`0x005C0120`) | `0x005773A0`: unit `+0xDC0` = ability |
//! | `perform_special_ability(s)` | `0x00613D30` → `0x00645970` | `..._CHANGE_SPECIAL_ABILITY` (`0x005C0820`) | `0x006A74E0` → `0x005612D0` |
//! | `morale_behavior_*()` | `0x00612F00` / `0x00612EC0` / `0x00612F40` | `BCQ_UNIT_MORALE_CHANGE` (`0x005C2860`) | `0x00555970` |
//! | `change_shot_type(s)` | `0x00614090` → `0x00645170` | (shot type enum `0x00F59030`) | not followed |
//! | `defend_building(b, run)` | `0x00613460` → `0x00645350` | order `0x005990D0` | not followed |
//!
//! CONFIRMED: the bindings, the commands, the fields and the skirmish check below. The model is
//! unit-level (no soldiers, no formation shapes), so the formation geometry the checks use is a
//! PROVISIONAL stand-in ([`frontage_m`], [`depth_m`]).

use std::f32::consts::TAU;

use super::attributes::{ability, shot_type_value};
use super::model::{Battle, dist2};
use super::morale::{self, ScriptMorale};

/// Enemies within this distance are looked at by the skirmish check and weigh on the evade
/// direction (`0x0054C9D0` / `0x0054C980`: 150.0, CONFIRMED).
pub const SKIRMISH_SCAN_RANGE_M: f32 = 150.0;
/// How far ahead an enemy's movement is projected: its speed × 8 s, or × 13 s while the unit's
/// last result was "evade" (CONFIRMED constants of `0x0054C9D0`).
pub const SKIRMISH_LOOKAHEAD_S: f32 = 8.0;
/// See [`SKIRMISH_LOOKAHEAD_S`].
pub const SKIRMISH_LOOKAHEAD_EVADING_S: f32 = 13.0;
/// The unit's own look-ahead is its missile range, at most 50 m (CONFIRMED).
pub const SKIRMISH_RANGE_CAP_M: f32 = 50.0;
/// Enemies closer than this weigh 1.5 times as much on the evade direction (CONFIRMED,
/// `0x00533BA0`).
pub const SKIRMISH_NEAR_M: f32 = 30.0;
/// Number of direction sectors of the threat histogram (CONFIRMED, 32 × 2048 = 65536).
pub const SKIRMISH_SECTORS: usize = 32;
/// An evading unit keeps its direction while the new one is within this 16-bit angle of it
/// (`0x005859D0`: 0xCCD ≈ 18°, CONFIRMED).
pub const SKIRMISH_KEEP_ANGLE: u16 = 0x0CCD;
/// The evade destination is this far along the evade direction (`0x005859D0`: 5000.0,
/// CONFIRMED), then kept inside the playable area (INFERRED from the path call `0x007FCFF0`).
pub const SKIRMISH_EVADE_DISTANCE_M: f32 = 5000.0;
/// The room-to-evade test looks this many seconds of the unit's speed ahead, at least the
/// formation depth (`0x0057A6F0`: 3.0, CONFIRMED).
pub const SKIRMISH_EDGE_LOOKAHEAD_S: f32 = 3.0;
/// PROVISIONAL stand-in for the formation shape (`+0x650` width / `+0x670` depth of the unit's
/// formation object): the model has no soldiers, so a unit is `men / 3` files 1 m apart, 3 ranks
/// deep.
pub const FORMATION_RANKS: f32 = 3.0;
/// See [`FORMATION_RANKS`] (PROVISIONAL).
pub const FILE_SPACING_M: f32 = 1.0;

/// Ability enum values used here (`0x0131C3A0`, CONFIRMED; see [`super::attributes::ABILITY_NAMES`]).
pub mod ability_ids {
    /// `fougasse_basic`.
    pub const FOUGASSE_BASIC: u8 = 9;
    /// `fougasse_improved`.
    pub const FOUGASSE_IMPROVED: u8 = 10;
    /// `wooden_stakes`.
    pub const WOODEN_STAKES: u8 = 11;
    /// `chevaux_de_frise`.
    pub const CHEVAUX_DE_FRISE: u8 = 12;
    /// `earthworks`.
    pub const EARTHWORKS: u8 = 13;
    /// `gabionade`.
    pub const GABIONADE: u8 = 14;
    /// `unlimber`.
    pub const UNLIMBER: u8 = 16;
}

/// The skirmish check's memory on the unit's behaviour object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkirmishEval {
    /// `+0x24`: the last result of the check `0x0054C9D0`: 0 nothing, 1 evade, 2 threatened but no
    /// room to evade, or the unit's own path runs into an enemy that is not its target.
    pub result: u8,
    /// `+0x28`: the 16-bit evade direction in use.
    pub angle: u16,
    /// The unit is in the evading state (its destination is the evade point).
    pub evading: bool,
}

/// One piece of a deployable defence (each piece is its own battle object in the original:
/// chevaux de frise `0x0059A670`, earthworks `0x0059B120`, gabions `0x0059D4A0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Defence {
    /// Ability enum value of the defence (9..=14).
    pub kind: u8,
    /// Side of the unit that built it.
    pub side: u8,
    /// Id of the unit that built it.
    pub unit: u32,
    /// Centre of the piece.
    pub position: (f32, f32),
    /// Facing in radians (the unit's facing; the piece's depth runs along it).
    pub facing: f32,
    /// Width across the facing, in metres.
    pub width: f32,
    /// Depth along the facing, in metres.
    pub depth: f32,
}

impl Defence {
    /// Distance from `p` to the piece (0 inside).
    pub fn distance(&self, p: (f32, f32)) -> f32 {
        let f = (self.facing.cos(), self.facing.sin());
        let v = (p.0 - self.position.0, p.1 - self.position.1);
        let along = (v.0 * f.0 + v.1 * f.1).abs();
        let across = (v.0 * f.1 - v.1 * f.0).abs();
        let dx = (along - self.depth * 0.5).max(0.0);
        let dy = (across - self.width * 0.5).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }

    /// True if the segment `a` → `b` enters the piece (sampled every 0.25 m).
    pub fn blocks(&self, a: (f32, f32), b: (f32, f32)) -> bool {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let n = (len / 0.25).ceil().max(1.0) as usize;
        (1..=n).any(|k| {
            let t = k as f32 / n as f32;
            self.distance((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)) <= 0.0
        })
    }

    /// True for the low obstacles that stop a charge (chevaux de frise, stakes).
    pub fn stops_charges(&self) -> bool {
        matches!(self.kind, ability_ids::CHEVAUX_DE_FRISE | ability_ids::WOODEN_STAKES)
    }

    /// True for the defences that give cover from missiles (earthworks, gabions).
    pub fn gives_cover(&self) -> bool {
        matches!(self.kind, ability_ids::EARTHWORKS | ability_ids::GABIONADE)
    }
}

/// Chevaux de frise (`0x005F14A0`, CONFIRMED): `n = clamp(floor(width × 0.14492753), 1, 4)` pieces
/// 6.9 m wide, side by side across the formation's front, 2 m in front of its centre.
pub const CHEVAUX_PIECE_M: f32 = 6.9;
/// Chevaux de frise: distance of the pieces in front of the formation centre (CONFIRMED).
pub const CHEVAUX_AHEAD_M: f32 = 2.0;
/// Earthworks (`0x005F1710`, CONFIRMED): `n = clamp(floor(width × 0.21739131), 1, 10)` pieces 4.6 m
/// apart, 3.4 m in front of the formation centre; each piece 5.35 m deep and 4.6 m wide
/// (`0x0059B120` corners at ±2.675 / ±2.3).
pub const EARTHWORK_PIECE_M: f32 = 4.6;
/// Earthworks: see [`EARTHWORK_PIECE_M`].
pub const EARTHWORK_AHEAD_M: f32 = 3.4;
/// Earthworks: depth of a piece (CONFIRMED).
pub const EARTHWORK_DEPTH_M: f32 = 5.35;
/// PROVISIONAL: how far behind an earthwork or gabion piece a unit's centre may stand and still be
/// in cover (about a formation's depth).
pub const COVER_BEHIND_M: f32 = 10.0;
/// PROVISIONAL depth of a chevaux de frise piece and of stakes (the object's size arguments
/// `0.5, 1.0, 4.0` to `0x0059BB70` are not decoded).
pub const LOW_OBSTACLE_DEPTH_M: f32 = 2.0;

/// `floor(width × k)` clamped to `1..=max`, as `0x005EA900` / `0x005EA960` round (CONFIRMED).
fn piece_count(width: f32, k: f32, max: u32) -> u32 {
    ((width * k).floor().max(0.0) as u32).clamp(1, max)
}

/// The pieces a unit builds for defence `kind`, at its current position and facing.
pub fn defence_pieces(u: &super::model::LandUnit, kind: u8) -> Vec<Defence> {
    let f = (u.facing.cos(), u.facing.sin());
    // Across the front: the original's (z, −x) of the facing (x, z).
    let l = (f.1, -f.0);
    let piece = |ahead: f32, across: f32, width: f32, depth: f32| Defence {
        kind,
        side: u.side,
        unit: u.id,
        position: (u.position.0 + f.0 * ahead + l.0 * across, u.position.1 + f.1 * ahead + l.1 * across),
        facing: u.facing,
        width,
        depth,
    };
    match kind {
        ability_ids::CHEVAUX_DE_FRISE => {
            let n = piece_count(u.width(), 0.144_927_53, 4);
            let half = n as f32 * CHEVAUX_PIECE_M * 0.5;
            (0..n)
                .map(|i| piece(CHEVAUX_AHEAD_M, -half + CHEVAUX_PIECE_M * 0.5 + CHEVAUX_PIECE_M * i as f32, CHEVAUX_PIECE_M, LOW_OBSTACLE_DEPTH_M))
                .collect()
        }
        ability_ids::EARTHWORKS => {
            let n = piece_count(u.width(), 0.217_391_31, 10);
            let half = (n - 1) as f32 * EARTHWORK_PIECE_M * 0.5;
            (0..n)
                .map(|i| piece(EARTHWORK_AHEAD_M, -half + EARTHWORK_PIECE_M * i as f32, EARTHWORK_PIECE_M, EARTHWORK_DEPTH_M))
                .collect()
        }
        // PROVISIONAL: the gabions surround each gun (`0x005F19C0` places one per artillery piece;
        // the model has no gun positions): one earthwork-sized piece per gun across the front.
        ability_ids::GABIONADE => {
            let n = u.missile.map_or(1, |w| w.guns.max(1));
            let half = (n - 1) as f32 * EARTHWORK_PIECE_M * 0.5;
            (0..n)
                .map(|i| piece(EARTHWORK_AHEAD_M, -half + EARTHWORK_PIECE_M * i as f32, EARTHWORK_PIECE_M, EARTHWORK_DEPTH_M))
                .collect()
        }
        // PROVISIONAL: stakes and fougasse as one strip across the front (not decoded).
        _ => vec![piece(CHEVAUX_AHEAD_M, 0.0, u.width(), LOW_OBSTACLE_DEPTH_M)],
    }
}

impl super::model::LandUnit {
    /// The formation's width in metres: [`super::model::LandUnit::formation_width`] when known,
    /// else the PROVISIONAL [`frontage_m`].
    pub fn width(&self) -> f32 {
        if self.formation_width > 0.0 { self.formation_width } else { frontage_m(self.men) }
    }

    /// The formation's depth in metres (else the PROVISIONAL [`depth_m`]).
    pub fn depth(&self) -> f32 {
        if self.formation_depth > 0.0 { self.formation_depth } else { depth_m() }
    }
}

/// PROVISIONAL formation width of a unit of `men` (see [`FORMATION_RANKS`]).
pub fn frontage_m(men: u32) -> f32 {
    (men as f32 / FORMATION_RANKS).max(1.0) * FILE_SPACING_M
}

/// PROVISIONAL formation depth (see [`FORMATION_RANKS`]).
pub fn depth_m() -> f32 {
    FORMATION_RANKS * FILE_SPACING_M
}

/// The 16-bit angle of a direction `(x, y)` (y = the map's z): `atan2(x, y) × 65536 / 2π`, as
/// `0x006995D0` computes it (CONFIRMED: `fpatan(x, z) × 10430.378`).
pub fn angle16(dir: (f32, f32)) -> u16 {
    ((dir.0.atan2(dir.1) * (65536.0 / TAU)).round() as i32 & 0xFFFF) as u16
}

/// The unit direction of a 16-bit angle: `(sin, cos)` (the engine's table at `0x0176CFF8`).
pub fn dir16(a: u16) -> (f32, f32) {
    let t = a as f32 * (TAU / 65536.0);
    (t.sin(), t.cos())
}

/// The sector of a direction, `0x00586B40` (CONFIRMED): `atan2(x, y)` wrapped to `[0, 2π)`, times
/// 32/2π, rounded down, at most 31.
pub fn sector(dir: (f32, f32)) -> usize {
    let mut a = dir.0.atan2(dir.1);
    if a < 0.0 {
        a += TAU;
    }
    ((a * (SKIRMISH_SECTORS as f32 / TAU)).floor() as usize).min(SKIRMISH_SECTORS - 1)
}

/// An oriented rectangle from `start` along the unit vector `dir` (`0x006995D0`: centre
/// `start + dir × length/2`, the given full width and length).
#[derive(Debug, Clone, Copy)]
struct Strip {
    start: (f32, f32),
    dir: (f32, f32),
    length: f32,
    width: f32,
}

impl Strip {
    /// Distance from `p` to the rectangle (0 inside).
    fn distance(&self, p: (f32, f32)) -> f32 {
        let v = (p.0 - self.start.0, p.1 - self.start.1);
        let along = v.0 * self.dir.0 + v.1 * self.dir.1;
        let across = (self.dir.0 * v.1 - self.dir.1 * v.0).abs();
        let dx = (-along).max(along - self.length).max(0.0);
        let dy = (across - self.width * 0.5).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }

    /// The four corners.
    fn corners(&self) -> [(f32, f32); 4] {
        let side = (-self.dir.1 * self.width * 0.5, self.dir.0 * self.width * 0.5);
        let end = (self.start.0 + self.dir.0 * self.length, self.start.1 + self.dir.1 * self.length);
        [
            (self.start.0 + side.0, self.start.1 + side.1),
            (self.start.0 - side.0, self.start.1 - side.1),
            (end.0 + side.0, end.1 + side.1),
            (end.0 - side.0, end.1 - side.1),
        ]
    }
}

/// The smoothing kernel of the threat histogram (`0x00514BE0`, CONFIRMED): `k[0] = 0`,
/// `k[n] = 1 / 2^min(n, 32 - n)`.
fn kernel(n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    1.0 / (1u64 << n.min(SKIRMISH_SECTORS - n)) as f32
}

impl Battle {
    /// The direction unit `idx` is moving in, and its speed, if it is moving (`0x0057EEE0`: the
    /// unit's movement target minus its position, `None` within 0.01 m). The target is the
    /// model's movement goal (PROVISIONAL: the model's goal choice, see `movement_goal`).
    fn moving_dir(&self, idx: usize) -> Option<((f32, f32), f32)> {
        let (goal, speed, _) = self.movement_goal(idx)?;
        let p = self.units[idx].position;
        let d = (goal.0 - p.0, goal.1 - p.1);
        let len2 = d.0 * d.0 + d.1 * d.1;
        if len2 <= 0.0001 {
            return None;
        }
        let len = len2.sqrt();
        Some(((d.0 / len, d.1 / len), speed))
    }

    /// The gate `0x0053F9E0`: skirmish mode on and no attack order (INFERRED: the order whose
    /// vfunc `+0x2C` answers true is the melee attack; here a charge).
    fn skirmish_gate(&self, idx: usize) -> bool {
        let u = &self.units[idx];
        u.skirmish && !u.charging
    }

    /// Enemies of unit `idx` that the checks look at: active, with men, not routing, within
    /// [`SKIRMISH_SCAN_RANGE_M`], nearest first (PROVISIONAL order: the original walks a spatial
    /// query `0x00701310`).
    fn skirmish_enemies(&self, idx: usize) -> Vec<usize> {
        let me = &self.units[idx];
        let r2 = SKIRMISH_SCAN_RANGE_M * SKIRMISH_SCAN_RANGE_M;
        let mut v: Vec<(f32, usize)> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, e)| e.side != me.side && e.active && e.men > 0 && !e.morale.is_routing_or_shattered())
            .map(|(j, e)| (dist2(me.position, e.position), j))
            .filter(|(d, _)| *d <= r2)
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        v.into_iter().map(|(_, j)| j).collect()
    }

    /// The skirmish check `0x0054C9D0` (CONFIRMED structure and constants; unit-level geometry):
    /// 1. An enemy moving within 150 m projects a strip along its movement, its width wide and its
    ///    speed × 8 s long (13 s if the last result was 1). If the strip reaches the unit and no
    ///    friendly non-skirmishing unit that is nearer that enemy stands on the strip's centre
    ///    line, the result is 1 when the unit has room to evade ([`Battle::evade_room`]), else 2.
    /// 2. Otherwise, if the unit itself is moving, a strip along its own movement (its width, its
    ///    missile range capped at 50 m long) that reaches an enemy other than its fire target
    ///    gives 2.
    /// 3. Otherwise 0.
    pub fn skirmish_check(&self, idx: usize) -> u8 {
        if !self.skirmish_gate(idx) {
            return 0;
        }
        let me = &self.units[idx];
        let look = if me.skirmish_eval.result == 1 { SKIRMISH_LOOKAHEAD_EVADING_S } else { SKIRMISH_LOOKAHEAD_S };
        let range = me.missile_range(self.kv_rules.fire_on_walls_range_modifier).min(SKIRMISH_RANGE_CAP_M);
        let own = self.moving_dir(idx);
        let target = me.fire_target;
        let my_half = me.width() * 0.5;
        for j in self.skirmish_enemies(idx) {
            let e = &self.units[j];
            if let Some((edir, espeed)) = self.moving_dir(j) {
                let len = espeed * look;
                let strip = Strip { start: e.position, dir: edir, length: len, width: e.width() };
                if strip.distance(me.position) <= my_half {
                    let line = Strip { width: 0.0, ..strip };
                    let d_me = dist2(me.position, e.position);
                    let blocked = self.units.iter().enumerate().any(|(k, f)| {
                        k != idx
                            && f.side == me.side
                            && f.active
                            && f.men > 0
                            && !f.morale.is_routing_or_shattered()
                            && !f.skirmish
                            && dist2(f.position, me.position) <= SKIRMISH_SCAN_RANGE_M * SKIRMISH_SCAN_RANGE_M
                            && dist2(f.position, e.position) <= d_me
                            && line.distance(f.position) <= f.width() * 0.5
                    });
                    if !blocked {
                        let a = self.evade_angle(idx);
                        return if self.evade_room(idx, a) { 1 } else { 2 };
                    }
                }
            }
            if let Some((dir, _)) = own
                && target != Some(e.id)
            {
                let strip = Strip { start: me.position, dir, length: range, width: me.width() };
                if strip.distance(e.position) <= e.width() * 0.5 {
                    return 2;
                }
            }
        }
        0
    }

    /// The evade direction (`0x0054C980` → `0x00533BA0` → `0x0054C010`, CONFIRMED): each enemy
    /// within 150 m adds `1 − d/150` (× 1.5 under 30 m, × 1.5 if it has more than twice the
    /// unit's men; INFERRED: `+0x2CC` / `+0x18` are the men counts) to the sectors its formation
    /// covers; the histogram is smoothed with [`kernel`] (`0x005341F0`) and the emptiest sector
    /// (lowest index on ties) gives the angle `sector × 2048 + 1024`.
    pub fn evade_angle(&self, idx: usize) -> u16 {
        let me = &self.units[idx];
        let p = me.position;
        let mut hist = [0f32; SKIRMISH_SECTORS];
        for e in self.units.iter().filter(|e| e.side != me.side && e.active && e.men > 0) {
            let v = (e.position.0 - p.0, e.position.1 - p.1);
            let d2 = v.0 * v.0 + v.1 * v.1;
            if d2 > SKIRMISH_SCAN_RANGE_M * SKIRMISH_SCAN_RANGE_M || d2 <= 0.0 {
                continue;
            }
            let d = d2.sqrt();
            let los = (v.0 / d, v.1 / d);
            // The enemy formation as a line of its width across its facing (PROVISIONAL shape).
            let half = e.width() * 0.5;
            let across = (-e.facing.sin() * half, e.facing.cos() * half);
            let ends = if e.men == 1 {
                vec![e.position]
            } else {
                vec![
                    (e.position.0 + across.0, e.position.1 + across.1),
                    (e.position.0 - across.0, e.position.1 - across.1),
                ]
            };
            let unit_dir = |c: (f32, f32)| {
                let w = (c.0 - p.0, c.1 - p.1);
                let l = (w.0 * w.0 + w.1 * w.1).sqrt().max(1e-6);
                (w.0 / l, w.1 / l)
            };
            let (mut low, mut high) = ((0.0, 0.0), (0.0, 0.0));
            if ends.len() == 1 {
                low = unit_dir(ends[0]);
                high = low;
            } else {
                let (mut most_neg, mut least_pos) = (f32::MAX, f32::MAX);
                for &c in &ends {
                    let cd = unit_dir(c);
                    let cross = los.1 * cd.0 - los.0 * cd.1;
                    if cross <= 0.0 {
                        if cross < most_neg {
                            most_neg = cross;
                            low = cd;
                        }
                    } else if cross < least_pos {
                        least_pos = cross;
                        high = cd;
                    }
                }
            }
            let mut w = 1.0 - d / SKIRMISH_SCAN_RANGE_M;
            if d < SKIRMISH_NEAR_M {
                w *= 1.5;
            }
            if e.men > 2 * me.men {
                w *= 1.5;
            }
            let s0 = sector(low);
            let s1 = sector(high);
            let count = s1 + 1 + if s1 < s0 { SKIRMISH_SECTORS } else { 0 } - s0;
            for k in 0..count {
                hist[(s0 + k) % SKIRMISH_SECTORS] += w;
            }
        }
        let smooth: Vec<f32> = (0..SKIRMISH_SECTORS)
            .map(|i| (1..SKIRMISH_SECTORS).map(|n| hist[(i + n) % SKIRMISH_SECTORS] * kernel(n)).sum())
            .collect();
        for (h, s) in hist.iter_mut().zip(&smooth) {
            *h += s;
        }
        let mut best = 0;
        for i in 1..SKIRMISH_SECTORS {
            if hist[i] < hist[best] {
                best = i;
            }
        }
        (best * 2048 + 1024) as u16
    }

    /// The room-to-evade test `0x0057A6F0` (CONFIRMED): a strip along the evade direction, the
    /// unit's width wide and `max(depth, speed × 3 s)` long, must lie inside the playable area
    /// (all four corners). Always true without a playable area.
    pub fn evade_room(&self, idx: usize, angle: u16) -> bool {
        let Some([x0, y0, x1, y1]) = self.playable_area else { return true };
        let u = &self.units[idx];
        let speed = u.move_speed();
        let strip = Strip {
            start: u.position,
            dir: dir16(angle),
            length: u.depth().max(speed * SKIRMISH_EDGE_LOOKAHEAD_S),
            width: u.width(),
        };
        strip.corners().iter().all(|c| x0 <= c.0 && y0 <= c.1 && c.0 < x1 && c.1 < y1)
    }

    /// The skirmish behaviour of unit `idx` for this tick: the check every fifth tick
    /// (`0x00585DC0`: `tick % 5 == id % 5`, CONFIRMED stagger; a result is dropped at once when
    /// the gate closes), then the evading state `0x005859D0`: while the result is 1 the unit runs
    /// [`SKIRMISH_EVADE_DISTANCE_M`] along the evade direction, keeping its old direction while
    /// the new one is within [`SKIRMISH_KEEP_ANGLE`]; when it is not 1 any more the unit stops.
    /// INFERRED: a unit enters the evading state only when it has no move order of its own (the
    /// state is entered from the idle state; the transition was not read).
    pub(super) fn skirmish_step(&mut self, idx: usize, tick: u32) {
        let u = &self.units[idx];
        if !u.active || u.men == 0 || u.morale.is_routing_or_shattered() {
            if u.skirmish_eval.evading {
                let u = &mut self.units[idx];
                u.skirmish_eval.evading = false;
                u.destination = None;
                u.running = false;
            }
            return;
        }
        if u.id % 5 == tick % 5 {
            let r = self.skirmish_check(idx);
            self.units[idx].skirmish_eval.result = r;
        } else if u.skirmish_eval.result != 0 && !self.skirmish_gate(idx) {
            self.units[idx].skirmish_eval.result = 0;
        }
        let in_melee = self.melee_contact(idx).is_some();
        let u = &self.units[idx];
        let may_evade = u.skirmish_eval.evading || u.destination.is_none();
        if u.skirmish_eval.result == 1 && may_evade && !in_melee {
            let a = self.evade_angle(idx);
            let u = &self.units[idx];
            if u.skirmish_eval.evading && u.destination.is_some() {
                let diff = (a.wrapping_sub(u.skirmish_eval.angle) as i16 as i32).abs();
                if diff <= SKIRMISH_KEEP_ANGLE as i32 {
                    return;
                }
            }
            let d = dir16(a);
            let mut dest = (u.position.0 + d.0 * SKIRMISH_EVADE_DISTANCE_M, u.position.1 + d.1 * SKIRMISH_EVADE_DISTANCE_M);
            if let Some([x0, y0, x1, y1]) = self.playable_area {
                // Cut the line at the area's edge (INFERRED from the path call 0x007FCFF0).
                let p = u.position;
                let mut t = 1.0f32;
                for (pc, dc, lo, hi) in [(p.0, dest.0 - p.0, x0, x1), (p.1, dest.1 - p.1, y0, y1)] {
                    if dc > 0.0 {
                        t = t.min(((hi - pc) / dc).max(0.0));
                    } else if dc < 0.0 {
                        t = t.min(((lo - pc) / dc).max(0.0));
                    }
                }
                dest = (p.0 + (dest.0 - p.0) * t, p.1 + (dest.1 - p.1) * t);
            }
            let u = &mut self.units[idx];
            u.skirmish_eval.angle = a;
            u.skirmish_eval.evading = true;
            u.destination = Some(dest);
            u.running = true;
        } else if u.skirmish_eval.evading {
            let u = &mut self.units[idx];
            u.skirmish_eval.evading = false;
            u.destination = None;
            u.running = false;
        }
    }

    /// The skirmish default `0x005357B0` (CONFIRMED; run by the unit constructor `0x0051B7D0` and when
    /// abilities change): `+0xD9C` = the unit may skirmish (stats column 53, not dismounted,
    /// `0x0053F9C0`) && `0x0055C230` (no gabionade selected, category artillery (type record
    /// `+0x1C` == 1), can unlimber and is not unlimbered) && class `artillery_horse` (type record
    /// `+0x20` == 2; the enums are in `ntw_ai::battle::classes`). No shipped unit has column 53 and
    /// is horse artillery, so every unit starts with skirmish off.
    pub fn skirmish_default(&self, idx: usize) -> bool {
        let u = &self.units[idx];
        let dismounted = u.active_abilities & (1 << 15) != 0;
        let unlimbered = u.active_abilities & (1 << ability_ids::UNLIMBER) != 0;
        u.attributes.skirmisher
            && !dismounted
            && u.deployable != Some(ability_ids::GABIONADE)
            && u.unit_category == "artillery"
            && u.capabilities.has_ability(ability_ids::UNLIMBER)
            && !unlimbered
            && u.unit_class == "artillery_horse"
    }

    /// `skirmish(on)`: the unit's skirmish mode (`0x005602A0`: unit `+0xD9C`, CONFIRMED).
    /// Returns false if `id` is unknown.
    pub fn order_skirmish(&mut self, id: u32, on: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        self.units[i].skirmish = on;
        true
    }

    /// `select_deployable_object(name)`: the deployable defence the unit will build
    /// (`0x005773A0`: unit `+0xDC0` = the ability, CONFIRMED). The binding refuses an ability the
    /// unit does not have ("unit does not support this special ability", CONFIRMED message;
    /// INFERRED test: the card's ability list). When deployment ends, [`Battle::end_deployment`]
    /// builds it. Returns the error text for an unknown unit or ability.
    pub fn order_select_deployable(&mut self, id: u32, ability_value: u8) -> Result<(), String> {
        let i = self.unit_index(id).ok_or("unknown unit")?;
        if !self.units[i].capabilities.has_ability(ability_value) {
            return Err("unit does not support this special ability".into());
        }
        self.units[i].deployable = Some(ability_value);
        Ok(())
    }

    /// End of deployment, `0x00551BA0` (CONFIRMED): for each unit and each deployable it has
    /// (fougasse 9/10, chevaux de frise 12, earthworks 13, gabionade 14), the selected one is kept
    /// and the others are removed. The model has no defences placed during deployment, so the
    /// selected one is built here, at the unit's final position ([`defence_pieces`]).
    pub fn end_deployment(&mut self) {
        let kinds = [
            ability_ids::FOUGASSE_BASIC,
            ability_ids::FOUGASSE_IMPROVED,
            ability_ids::CHEVAUX_DE_FRISE,
            ability_ids::EARTHWORKS,
            ability_ids::GABIONADE,
        ];
        for idx in 0..self.units.len() {
            let u = &self.units[idx];
            if let Some(k) = u.deployable
                && kinds.contains(&k)
                && u.capabilities.has_ability(k)
                && u.active
            {
                self.build_defence(idx, k);
            }
        }
    }

    fn build_defence(&mut self, idx: usize, kind: u8) {
        let u = &self.units[idx];
        if self.defences.iter().any(|x| x.unit == u.id && x.kind == kind) {
            return;
        }
        let pieces = defence_pieces(u, kind);
        self.defences.extend(pieces);
    }

    /// The first enemy defence piece that the move of unit `idx` from `from` to `to` runs into
    /// (PROVISIONAL rule: the pieces are solid objects in the original, `0x0059BB70`; in the
    /// unit-level model a unit does not walk into an enemy piece). The unit's own side passes.
    pub(super) fn defence_in_the_way(&self, idx: usize, from: (f32, f32), to: (f32, f32)) -> Option<Defence> {
        let side = self.units[idx].side;
        self.defences.iter().find(|d| d.side != side && d.distance(from) > 0.0 && d.blocks(from, to)).copied()
    }

    /// Contact with an enemy defence piece, from the soldiers' collision handlers (CONFIRMED):
    /// - chevaux de frise `0x006E94E0`: a soldier of another alliance, alive (byte `+0xEC`), for which
    ///   the soldier vfunc `+0xF0` answers true is killed: `0x007FC120` (the invincibility check
    ///   `0x00679B20`) then the death dispatch `0x0080A4E0` with impulse 0.75;
    /// - stakes `0x006E9160`: an enemy soldier for which `+0xF0` or `+0xE8` answers true and whose
    ///   heading is within 0x238E (≈ 50°) of head-on to the stakes is killed the same way, impulse 1.0.
    ///
    /// The dispatch `0x0080A4E0` calls the soldier's `+0x104` and, when `+0xF0` answers true, the same
    /// on the entity `+0x90` returns: so `+0xF0` is "is a rider" and `+0x90` his mount (INFERRED),
    /// and `+0xE8` "is a mount" (INFERRED). The kill does not go through the hit points (`+0x624`,
    /// `0x00816F70`) nor the armour: contact kills the cavalryman. The 0.75 / 1.0 is the impulse
    /// of the death, not a chance (INFERRED from the projectile path, where the same dispatch follows
    /// the hit-point kill `0x007F1860`).
    ///
    /// Unit-level (PROVISIONAL): every man of a cavalry unit's front that touches the piece
    /// (`min(unit width, piece width) / 1 m`) dies, once per contact. Infantry is not hurt.
    pub(super) fn defence_contact(&mut self, idx: usize, d: &Defence) {
        let u = &self.units[idx];
        if u.defence_contact {
            return;
        }
        let rider = super::strength::mounted(u);
        let kills = match d.kind {
            ability_ids::CHEVAUX_DE_FRISE => rider,
            ability_ids::WOODEN_STAKES => {
                // Head-on: the unit's heading is opposite the stakes' facing within 0x238E.
                let a = angle16((u.facing.cos(), u.facing.sin()));
                let s = angle16((d.facing.cos(), d.facing.sin()));
                let diff = (a.wrapping_add(0x8000).wrapping_sub(s) as i16 as i32).abs();
                rider && diff < 0x238E
            }
            _ => false,
        };
        let touching = ((u.width().min(d.width) / FILE_SPACING_M).floor() as u32).min(u.men);
        let u = &mut self.units[idx];
        u.defence_contact = true;
        if kills && !u.invincible {
            u.men -= touching;
            u.recent_losses += touching;
        }
    }

    /// True if unit `target` is in cover from unit `shooter`: it stands behind one of its side's
    /// earthwork or gabion pieces (within the piece's width, at most [`COVER_BEHIND_M`] behind it)
    /// and the shooter is in front of that piece. INFERRED trigger for the CONFIRMED `in cover`
    /// term of the chance to hit (−0.2, `0x00DAB9D0`); the original's cover is the projectile
    /// meeting the 1.48 m high earthwork object.
    pub fn in_cover(&self, target: usize, shooter: usize) -> bool {
        let t = &self.units[target];
        // A unit in a building is in cover from every side (PROVISIONAL, `garrison`).
        if t.garrison.is_some() {
            return true;
        }
        let s = self.units[shooter].position;
        self.defences.iter().filter(|d| d.side == t.side && d.gives_cover()).any(|d| {
            let f = (d.facing.cos(), d.facing.sin());
            let rel = |p: (f32, f32)| {
                let v = (p.0 - d.position.0, p.1 - d.position.1);
                (v.0 * f.0 + v.1 * f.1, (v.0 * f.1 - v.1 * f.0).abs())
            };
            let (t_along, t_across) = rel(t.position);
            let (s_along, _) = rel(s);
            t_across <= d.width * 0.5 && t_along < -d.depth * 0.5 && t_along >= -d.depth * 0.5 - COVER_BEHIND_M && s_along > d.depth * 0.5
        })
    }

    /// `perform_special_ability(name)` (`0x00645970` → `0x006A74E0` with mode 0, CONFIRMED): an
    /// ability the unit does not have is an error ("unit does not support this special
    /// ability"); one that is already active, or that the unit cannot perform now (INFERRED for
    /// `0x0053EA60`: the unit is active, not routing and has men), does nothing. Otherwise it is
    /// performed (`0x005612D0`). Effects in the model: `square_formation` sets the melee square
    /// flag; `wooden_stakes` builds stakes in front of the unit (PROVISIONAL: at once, the original
    /// plants them over time); the others (`unlimber`: the model's guns are always ready; formation
    /// and light infantry behaviours) are recorded only. Returns whether the ability was performed.
    pub fn order_special_ability(&mut self, id: u32, ability_value: u8) -> Result<bool, String> {
        let i = self.unit_index(id).ok_or("unknown unit")?;
        let u = &self.units[i];
        if !u.capabilities.has_ability(ability_value) {
            return Err("unit does not support this special ability".into());
        }
        let bit = 1u32 << ability_value;
        if u.active_abilities & bit != 0 || !u.active || u.men == 0 || u.morale.is_routing_or_shattered() {
            return Ok(false);
        }
        self.units[i].active_abilities |= bit;
        match ability_value {
            ability::SQUARE_FORMATION => self.units[i].in_square = true,
            ability_ids::WOODEN_STAKES => self.build_defence(i, ability_ids::WOODEN_STAKES),
            _ => {}
        }
        Ok(true)
    }

    /// `change_shot_type(name)`: loads another shot (the shot type enum `0x00F59030`, CONFIRMED
    /// names). The unit must be able to fire it ("error: unit '%S' does not support this shot
    /// type", CONFIRMED message; INFERRED test: one of its gun's projectiles, `shot_options`). The reload in progress is kept (PROVISIONAL). Returns whether the shot
    /// changed; an unknown name or a shot the unit cannot fire is an error.
    pub fn order_change_shot_type(&mut self, id: u32, name: &str) -> Result<bool, String> {
        let i = self.unit_index(id).ok_or("unknown unit")?;
        let shot = shot_type_value(name).ok_or_else(|| format!("unknown shot type '{name}'"))?;
        let u = &mut self.units[i];
        if u.shot_type == Some(shot) {
            return Ok(false);
        }
        let k = u
            .shot_options
            .iter()
            .position(|(s, _)| *s == shot)
            .ok_or_else(|| format!("unit cannot fire '{name}'"))?;
        let (_, weapon) = u.shot_options.remove(k);
        if let (Some(old), Some(cur)) = (u.shot_type, u.missile) {
            u.shot_options.push((old, cur));
        }
        u.missile = Some(weapon);
        u.shot_type = Some(shot);
        Ok(true)
    }

    /// `set_invincible(b)`: the unit cannot be hit ([`super::model::LandUnit::invincible`]).
    /// Returns false if `id` is unknown.
    pub fn order_invincible(&mut self, id: u32, on: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        self.units[i].invincible = on;
        true
    }

    /// `morale_behavior_fearless` / `_default` / `_rout`, see [`morale::set_script_morale`].
    /// Returns false if `id` is unknown.
    pub fn order_script_morale(&mut self, id: u32, mode: ScriptMorale) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        morale::set_script_morale(&mut self.units[i].morale, mode);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::fatigue::KvFatigue;
    use crate::battle::model::LandUnit;
    use crate::battle::morale::{KvMorale, MoraleBehaviour, MoraleState};

    fn battle() -> Battle {
        Battle::new(1, KvMorale::default(), KvFatigue::default())
    }

    fn unit(id: u32, side: u8, pos: (f32, f32)) -> LandUnit {
        let mut u = LandUnit::new(id, side, 60, pos);
        u.walk_speed = 1.5;
        u.run_speed = 4.0;
        u.hold_position = true;
        u
    }

    #[test]
    fn angles_and_sectors() {
        assert_eq!(angle16((0.0, 1.0)), 0);
        assert_eq!(angle16((1.0, 0.0)), 0x4000);
        let d = dir16(0x4000);
        assert!((d.0 - 1.0).abs() < 1e-6 && d.1.abs() < 1e-6);
        assert_eq!(sector((0.0, 1.0)), 0);
        assert_eq!(sector((1.0, 0.0)), 8);
        assert_eq!(sector((0.0, -1.0)), 16);
        assert_eq!(sector((-1.0, 0.0)), 24);
        assert_eq!(sector((-0.001, 1.0)), 31);
        assert_eq!(kernel(0), 0.0);
        assert_eq!((kernel(1), kernel(2), kernel(31), kernel(16)), (0.5, 0.25, 0.5, 1.0 / 65536.0));
    }

    #[test]
    fn evade_angle_points_away_from_a_single_enemy() {
        let mut b = battle();
        b.add_unit(unit(1, 0, (0.0, 0.0)));
        b.add_unit(unit(2, 1, (0.0, 50.0))); // straight ahead along +y (sector 0)
        let a = b.evade_angle(0);
        // The emptiest sector is the one opposite the enemy (sector 16 → 16×2048+1024).
        assert_eq!(a, 16 * 2048 + 1024);
    }

    #[test]
    fn skirmishers_evade_an_approaching_enemy_and_stop_when_it_halts() {
        let mut b = battle();
        let mut s = unit(5, 0, (0.0, 0.0));
        s.skirmish = true;
        b.add_unit(s);
        let mut e = unit(1, 1, (0.0, 25.0));
        e.destination = Some((0.0, -100.0)); // running straight at the skirmishers
        e.running = true;
        b.add_unit(e);
        // Tick 0: unit id 5 checks on ticks 0, 5, 10, ...
        b.step();
        let s = &b.units[1];
        assert_eq!(s.skirmish_eval.result, 1);
        assert!(s.skirmish_eval.evading && s.running);
        let d = s.destination.expect("evading");
        assert!(d.1 < -1000.0, "runs away from the enemy: {d:?}");
        // The enemy halts: the next check finds nothing and the unit stops.
        b.units[0].destination = None;
        for _ in 0..5 {
            b.step();
        }
        let s = &b.units[1];
        assert_eq!(s.skirmish_eval.result, 0);
        assert!(!s.skirmish_eval.evading && s.destination.is_none());
    }

    #[test]
    fn a_line_unit_in_between_blocks_the_threat() {
        let mut b = battle();
        let mut s = unit(5, 0, (0.0, 0.0));
        s.skirmish = true;
        b.add_unit(s);
        b.add_unit(unit(6, 0, (0.0, 20.0))); // line infantry between them, not skirmishing
        let mut e = unit(1, 1, (0.0, 30.0));
        e.destination = Some((0.0, -100.0));
        e.running = true;
        b.add_unit(e);
        assert_eq!(b.skirmish_check(1), 0);
        b.units[2].skirmish = true; // a skirmisher does not block
        assert_eq!(b.skirmish_check(1), 1);
    }

    #[test]
    fn no_room_at_the_map_edge_gives_two() {
        let mut b = battle();
        b.playable_area = Some([-10.0, -10.0, 1000.0, 1000.0]);
        let mut s = unit(5, 0, (0.0, 0.0));
        s.skirmish = true;
        b.add_unit(s);
        let mut e = unit(1, 1, (0.0, 25.0));
        e.destination = Some((0.0, -100.0));
        e.running = true;
        b.add_unit(e);
        // Everything behind the unit is off the map.
        assert_eq!(b.skirmish_check(1), 2);
        b.units[1].skirmish = false;
        assert_eq!(b.skirmish_check(1), 0);
    }

    #[test]
    fn script_morale_modes() {
        let mut b = battle();
        b.add_unit(unit(1, 0, (0.0, 0.0)));
        assert!(b.order_script_morale(1, ScriptMorale::Fearless));
        let m = &b.units[0].morale;
        assert_eq!((m.state, m.behaviour, m.skip_flags), (MoraleState::Eager, MoraleBehaviour::Normal, [true, false, false]));
        b.order_script_morale(1, ScriptMorale::Rout);
        let m = &b.units[0].morale;
        assert_eq!((m.state, m.behaviour, m.skip_flags), (MoraleState::Shattered, MoraleBehaviour::ScriptRout, [true, true, false]));
        assert!(m.is_routing_or_shattered());
        b.order_script_morale(1, ScriptMorale::Default);
        assert_eq!(b.units[0].morale.skip_flags, [false, false, false]);
    }

    #[test]
    fn deployables_and_abilities_need_the_ability() {
        use crate::battle::attributes::UnitCapabilities;
        let mut b = battle();
        let mut u = unit(1, 0, (0.0, 0.0));
        u.capabilities = UnitCapabilities::from_names(["earthworks", "square_formation", "wooden_stakes"], []);
        b.add_unit(u);
        assert!(b.order_select_deployable(1, ability_ids::GABIONADE).is_err());
        b.order_select_deployable(1, ability_ids::EARTHWORKS).unwrap();
        b.end_deployment();
        // 60 men → 20 m wide → floor(20 × 0.217) = 4 earthwork pieces.
        assert_eq!(b.defences.len(), 4);
        assert_eq!(b.defences[0].kind, ability_ids::EARTHWORKS);
        assert!(b.order_special_ability(1, ability_ids::UNLIMBER).is_err());
        assert_eq!(b.order_special_ability(1, ability::SQUARE_FORMATION), Ok(true));
        assert!(b.units[0].in_square);
        assert_eq!(b.order_special_ability(1, ability::SQUARE_FORMATION), Ok(false));
        assert_eq!(b.order_special_ability(1, ability_ids::WOODEN_STAKES), Ok(true));
        assert_eq!(b.defences.len(), 5);
    }

    #[test]
    fn defence_geometry_follows_the_exe() {
        // A unit facing +y (the engine's z) at the origin, 30 m wide.
        let mut u = unit(1, 0, (0.0, 0.0));
        u.facing = std::f32::consts::FRAC_PI_2;
        u.formation_width = 30.0;
        // Chevaux: floor(30 × 0.1449) = 4 pieces of 6.9 m, 2 m ahead, centred.
        let c = defence_pieces(&u, ability_ids::CHEVAUX_DE_FRISE);
        assert_eq!(c.len(), 4);
        let xs: Vec<f32> = c.iter().map(|p| p.position.0).collect();
        for (x, want) in xs.iter().zip([-10.35, -3.45, 3.45, 10.35]) {
            assert!((x - want).abs() < 1e-4, "{xs:?}");
        }
        assert!(c.iter().all(|p| (p.position.1 - 2.0).abs() < 1e-4));
        // Earthworks: floor(30 × 0.2174) = 6 pieces 4.6 m apart, 3.4 m ahead.
        let e = defence_pieces(&u, ability_ids::EARTHWORKS);
        assert_eq!(e.len(), 6);
        assert!((e[0].position.0 + 11.5).abs() < 1e-4 && (e[5].position.0 - 11.5).abs() < 1e-4);
        assert!(e.iter().all(|p| (p.position.1 - 3.4).abs() < 1e-4 && p.depth == EARTHWORK_DEPTH_M));
        // Width caps: 4 chevaux, 10 earthworks; at least one piece.
        u.formation_width = 200.0;
        assert_eq!((defence_pieces(&u, ability_ids::CHEVAUX_DE_FRISE).len(), defence_pieces(&u, ability_ids::EARTHWORKS).len()), (4, 10));
        u.formation_width = 2.0;
        assert_eq!(defence_pieces(&u, ability_ids::CHEVAUX_DE_FRISE).len(), 1);
    }

    #[test]
    fn defences_stop_enemies_and_give_cover() {
        use crate::battle::attributes::UnitCapabilities;
        let mut b = battle();
        let mut d = unit(1, 0, (0.0, 0.0));
        d.facing = std::f32::consts::FRAC_PI_2;
        d.formation_width = 30.0;
        d.capabilities = UnitCapabilities::from_names(["chevaux_de_frise", "earthworks"], []);
        b.add_unit(d);
        let mut c = unit(2, 1, (0.0, 30.0));
        c.destination = Some((0.0, 0.0));
        c.charging = true;
        c.unit_category = "cavalry".into();
        c.running = true;
        c.hold_position = false;
        b.add_unit(c);
        b.order_select_deployable(1, ability_ids::CHEVAUX_DE_FRISE).unwrap();
        b.end_deployment();
        for _ in 0..100 {
            b.step();
        }
        let c = &b.units[1];
        assert!(c.position.1 > 3.0, "the charge stops in front of the chevaux: {:?}", c.position);
        assert!(!c.charging);
        // The riders touching the 6.9 m piece die (20 m front → 6 men); melee with the defenders
        // behind the chevaux may take more.
        assert!(c.men <= 54, "contact kills the touching riders: {} left", c.men);
        // Cover: behind its earthworks, from a shooter in front.
        let mut b2 = battle();
        let mut d = unit(1, 0, (0.0, 0.0));
        d.facing = std::f32::consts::FRAC_PI_2;
        d.formation_width = 30.0;
        d.capabilities = UnitCapabilities::from_names(["earthworks"], []);
        b2.add_unit(d);
        b2.add_unit(unit(2, 1, (0.0, 100.0)));
        b2.add_unit(unit(3, 1, (0.0, -100.0)));
        assert!(!b2.in_cover(0, 1));
        b2.order_select_deployable(1, ability_ids::EARTHWORKS).unwrap();
        b2.end_deployment();
        assert!(b2.in_cover(0, 1), "shot from the front");
        assert!(!b2.in_cover(0, 2), "shot from behind");
    }

    #[test]
    fn skirmish_default_needs_limbered_horse_artillery() {
        use crate::battle::attributes::UnitCapabilities;
        let mut b = battle();
        let mut u = unit(1, 0, (0.0, 0.0));
        u.attributes.skirmisher = true;
        u.unit_category = "infantry".into();
        u.unit_class = "infantry_light".into();
        b.add_unit(u);
        assert!(!b.skirmish_default(0), "light infantry: off");
        let u = &mut b.units[0];
        u.unit_category = "artillery".into();
        u.unit_class = "artillery_horse".into();
        u.capabilities = UnitCapabilities::from_names(["unlimber"], []);
        assert!(b.skirmish_default(0));
        b.units[0].active_abilities |= 1 << ability_ids::UNLIMBER;
        assert!(!b.skirmish_default(0), "unlimbered: off");
    }

    #[test]
    fn defence_contact_kills_riders_only() {
        let mut b = battle();
        let mut inf = unit(1, 1, (0.0, 10.0));
        inf.facing = -std::f32::consts::FRAC_PI_2;
        b.add_unit(inf);
        let mut cav = unit(2, 1, (0.0, 10.0));
        cav.facing = -std::f32::consts::FRAC_PI_2;
        cav.unit_category = "cavalry".into();
        b.add_unit(cav);
        // Stakes facing +y (towards the enemies coming down -y): head-on for both units.
        let stakes = Defence { kind: ability_ids::WOODEN_STAKES, side: 0, unit: 9, position: (0.0, 5.0), facing: std::f32::consts::FRAC_PI_2, width: 10.0, depth: 2.0 };
        b.defence_contact(0, &stakes);
        b.defence_contact(1, &stakes);
        assert_eq!(b.units[0].men, 60, "infantry is not hurt");
        assert_eq!(b.units[1].men, 50, "10 riders touch the 10 m stakes");
        b.defence_contact(1, &stakes);
        assert_eq!(b.units[1].men, 50, "once per contact");
        // From the side (90° off head-on) stakes do nothing.
        b.units[1].defence_contact = false;
        b.units[1].facing = 0.0;
        b.defence_contact(1, &stakes);
        assert_eq!(b.units[1].men, 50);
    }

    #[test]
    fn change_shot_type_swaps_the_weapon() {
        use crate::battle::shooting::MissileWeapon;
        let w = |range| MissileWeapon {
            range,
            accuracy: 0.0,
            reload_skill: 0,
            reload_time_s: 10,
            damage: 1.0,
            projectiles_per_shot: 1,
            is_artillery: true,
            guns: 2,
            ballistics: Default::default(),
        };
        let mut b = battle();
        let mut u = unit(1, 0, (0.0, 0.0));
        u.missile = Some(w(500));
        u.shot_type = Some(0);
        u.shot_options = vec![(3, w(150))];
        b.add_unit(u);
        assert!(b.order_change_shot_type(1, "grape").is_err());
        assert!(b.order_change_shot_type(1, "nonsense").is_err());
        assert_eq!(b.order_change_shot_type(1, "canister"), Ok(true));
        assert_eq!(b.units[0].missile.unwrap().range, 150);
        assert_eq!(b.order_change_shot_type(1, "round_shot"), Ok(true));
        assert_eq!(b.units[0].missile.unwrap().range, 500);
    }
}

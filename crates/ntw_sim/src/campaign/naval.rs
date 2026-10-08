//! The campaign's stat-based autoresolver for naval battles (CAMPAIGN_FIDELITY.md §Naval autoresolve).
//!
//! The pipeline is the land resolver's ([`super::autoresolve`]): setup → one winner roll → losses per side,
//! with the ship unit class's own pieces (CONFIRMED from the exe):
//! - **pair engagement** `0x00759B20` on crews and hulls (64 fuzzed samples, `0x0078EE10`), kill rates
//!   `0x0078D6C0` (`Knav` 0.2, `Mnav` 2), rout points `0x0078FFA0`;
//! - **statistics**: crew casualties as on land, plus the hull fractions (the 0x90 block's naval floats);
//! - **loss rate** `0x0078F6A0` (from the hull statistics, the naval victory percents);
//! - **damage** `0x0074FA70`: the damage pairs grow, the ship sinks past `autoresolve_ship_damage_required_for_sink`,
//!   otherwise the three crews lose their share; a wipeout sinks every losing ship (`0x007DD800`).
//!
//! Inputs, all CONFIRMED: the ship's saved state (`NAVAL_UNIT/SHIP_DAMAGE_INFO` = the card's +0x94..+0xC8, see
//! [`ShipState`]); the part hit points and sink weights (18 triples of `unit_stats_naval`, summed); the potential
//! `(#20 + #21) × (guns >> 1) × 3` (× 1.5 for bomb ketches and rocket ships), the morale #1 against the highest #1
//! of all types, and the range classes (line of battle 3, frigate and galley 2, others 1). The battle record
//! behind them was read with the debugger in a vanilla session (CAMPAIGN_FIDELITY.md §Naval autoresolve).
//! Not modelled: ship captures (`0x007CD570`, through the captives step `0x0074F710` / `0x0075C090`).

use crate::rng::{CaRng, INV_65535};

use super::autoresolve::{
    ai_loss_factor, advantage, combine_stats, finalize_probabilities, roll_winner, sample_stats, ArUnit, ArVars, PairResult,
    PairSample, Stats, VictoryType,
};

/// `unit_combat_query_base_kill_rate_naval_tweak` (CONFIRMED default 0.2).
pub const K_NAVAL: f32 = 0.2;
/// `unit_combat_query_naval_outnumbering_multiplier_tweak` (CONFIRMED default 2).
pub const M_NAVAL: f32 = 2.0;
const ROUT: f32 = 0.4;
const SHAKEN: f32 = 0.9;
const FUZZ: f32 = 0.2;

/// A ship's campaign damage state: `NAVAL_UNIT/SHIP_DAMAGE_INFO` v2 (CONFIRMED layout and uses: the card's
/// +0x94..+0xA4 damage fractions, +0xA8..+0xB0 crews, +0xB4..+0xBC full crews, +0xC0 guns, +0xC4 sunk,
/// +0xC8 full guns; the sinking `0x0074FA70` sets every damage to 1, the crews and guns to 0 and the flag).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShipState {
    /// Five damage fractions 0..1; the resolver uses the pairs (0, 2) and (1, 3) (+0xA4 only on sinking).
    pub damage: [f32; 5],
    /// The three crews (the `unit_stats_naval` #17..#19 of a new ship; their sum is the unit's men).
    pub crews: [i32; 3],
    /// The full crews.
    pub max_crews: [i32; 3],
    /// Guns.
    pub guns: u32,
    /// Sunk.
    pub sunk: bool,
    /// Full guns.
    pub max_guns: u32,
}

impl ShipState {
    /// The crew total.
    pub fn crew(&self) -> u32 {
        self.crews.iter().map(|&c| c.max(0) as u32).sum()
    }
}

/// What the naval rules know about a ship type (`unit_stats_naval`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ShipRules {
    /// #17..#19: the three crews of a new ship.
    pub crews: [i32; 3],
    /// Σ of the 18 parts' hit points (the resolver's hull, ship unit +0xE0).
    pub hull: f32,
    /// Mean of the parts' sink weights (ship unit +0xD8): the ship sinks at (1 − mean) × hull damage.
    pub sink_weight: f32,
    /// Morale: `unit_stats_naval` #1 (the battle record's +0x1C8; CONFIRMED with the debugger: 12 for a British 74,
    /// 10 for a French or Spanish 74, 14 for the British heavy first rate). The rout points use it against the
    /// highest #1 of all ship types ([`NavalVars::max_morale`]).
    pub morale: f32,
    /// `unit_stats_naval` #20 and #21: the two values the potential sums (record +0x1D8 / +0x1DC, CONFIRMED).
    pub fire: [i32; 2],
    /// The type's gun count (its model's +0x7C; equal to every ship's saved full guns, `SHIP_DAMAGE_INFO` #13,
    /// CONFIRMED). Not a DB column: filled from the ships of the loaded file (0 = unknown, then the ship's own
    /// saved full guns count).
    pub guns: u32,
    /// The range class (unit +0x9C, table `0x0070D370` by `units` category, CONFIRMED): line of battle 3,
    /// frigate and galley 2, every other ship 1.
    pub range: i32,
    /// `naval_bomb_ketch` and `naval_rocket_ship` classes: the potential × 1.5 (`0x00758950`, classes 0x18 / 0x26).
    pub bombard: bool,
    /// `naval_merchant` category (11): a winning merchant ship's loss rate × 0.95 (`0x0078F6A0`, CONFIRMED).
    pub merchant: bool,
}

/// One ship in a naval autoresolve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavalUnit {
    /// Its saved state (updated by [`resolve`]).
    pub state: ShipState,
    /// Its type's rules.
    pub rules: ShipRules,
    /// Owned by a human (difficulty factors).
    pub human: bool,
    /// Its force's index on its side (the captives step shares the captures out per winning force).
    pub force: usize,
}

impl NavalUnit {
    /// The potential `0x00758950` (CONFIRMED): `(#20 + #21) × (guns >> 1) × 3` in integers, × 1.5 for bomb
    /// ketches and rocket ships. The gun count is the type's, not the ship's current guns.
    pub fn potential(&self) -> f32 {
        let guns = if self.rules.guns > 0 { self.rules.guns } else { self.state.max_guns };
        let p = (i64::from(self.rules.fire[0]) + i64::from(self.rules.fire[1])) * i64::from(guns >> 1) * 3;
        if self.rules.bombard { p as f32 * 1.5 } else { p as f32 }
    }
    fn hull_damage(&self) -> f32 {
        let d = &self.state.damage;
        (d[0] + d[1] + d[2] + d[3]) * 0.25 * self.rules.hull
    }
}

/// The variables of the naval resolver: the land set plus the naval ones (campaign variables 61..68).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavalVars {
    /// The land set (gaussian, massacre, wipeout, difficulty, ...).
    pub land: ArVars,
    /// \[61..63] `autoresolve_minor/normal/major_naval_victory_win_percent`.
    pub win: [f32; 3],
    /// \[64..66] `..._lose_percent`.
    pub lose: [f32; 3],
    /// \[67] `autoresolve_ship_damage_fuzziness`.
    pub damage_fuzz: f32,
    /// \[68] `autoresolve_ship_damage_required_for_sink`.
    pub sink: f32,
    /// The highest morale (#1) of every ship type in the DB (the setup `0x0070D370` +0x74, CONFIRMED: 14 in the
    /// shipped data, the battle's +0xDC).
    pub max_morale: f32,
    /// \[55] `autoresolve_base_best_ship_kills_to_capture`: the share of the sunk losing ships that can be captured.
    pub best_kills: f32,
    /// \[56] `autoresolve_chance_of_not_capturing_ship`.
    pub not_capturing: f32,
    /// \[72] `autoresolve_ship_damage_capture_multiplier`.
    pub capture_mult: f32,
}

impl NavalVars {
    /// Reads the variables from the rules.
    pub fn from_rules(r: &super::rules::CampaignRules) -> Self {
        NavalVars {
            land: ArVars::from_rules(r),
            win: [
                r.var("autoresolve_minor_naval_victory_win_percent", 0.0),
                r.var("autoresolve_normal_naval_victory_win_percent", 0.0),
                r.var("autoresolve_major_naval_victory_win_percent", 0.0),
            ],
            lose: [
                r.var("autoresolve_minor_naval_victory_lose_percent", 0.0),
                r.var("autoresolve_normal_naval_victory_lose_percent", 0.0),
                r.var("autoresolve_major_naval_victory_lose_percent", 0.0),
            ],
            damage_fuzz: r.var("autoresolve_ship_damage_fuzziness", 0.0),
            sink: r.var("autoresolve_ship_damage_required_for_sink", 0.875),
            max_morale: r.ships.values().map(|s| s.morale).fold(0.0f32, f32::max),
            best_kills: r.var("autoresolve_base_best_ship_kills_to_capture", 0.0),
            not_capturing: r.var("autoresolve_chance_of_not_capturing_ship", 1.0),
            capture_mult: r.var("autoresolve_ship_damage_capture_multiplier", 0.0),
        }
    }
}

/// One naval sample: the result, crew casualty fractions (both over A's crew, as the exe does) and hull
/// damage fractions (1 past the sink point).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavalSample {
    /// Who won.
    pub result: PairResult,
    /// A's crew lost / A's crew.
    pub crew_a: f32,
    /// B's crew lost / A's crew (sic).
    pub crew_b: f32,
    /// A's hull fraction.
    pub hull_a: f32,
    /// B's hull fraction.
    pub hull_b: f32,
}

/// The pair's fixed inputs (the ship pair object `0x007201A0`).
#[derive(Debug, Clone, Copy)]
pub struct NavalPair {
    /// Crews (after the units' previous casualty means; 0 at the start).
    pub crew_a: f32,
    /// See `crew_a`.
    pub crew_b: f32,
    /// Hulls (Σ part hit points).
    pub hull_a: f32,
    /// See `hull_a`.
    pub hull_b: f32,
    /// Hull damage so far.
    pub dmg_a: f32,
    /// See `dmg_a`.
    pub dmg_b: f32,
    /// Mean sink weights.
    pub sink_a: f32,
    /// See `sink_a`.
    pub sink_b: f32,
}

/// The engagement `0x00759B20` (CONFIRMED, decompiled): `ka` / `kb` the rates, `rout_*` the rout points,
/// `level` the range level (> 0: A fires first).
pub fn engagement(p: &NavalPair, ka: f32, rout_a: f32, kb: f32, rout_b: f32, level: i32) -> NavalSample {
    let limit_a = p.crew_a - rout_a;
    let limit_b = p.crew_b - rout_b;
    let sink_b = (1.0 - p.sink_b) * p.hull_b;
    let sink_a = (1.0 - p.sink_a) * p.hull_a;
    let mut db = p.dmg_b;
    if level > 0 {
        db += ((p.hull_a - p.dmg_a) / p.hull_a) * ka * level as f32;
    }
    let mut db = db.clamp(0.0, p.hull_b);
    let mut lost_b = 1.0 / p.hull_b * db * p.crew_b;
    let mut da = p.dmg_a;
    if level < 0 {
        da -= (p.hull_b - db) * (1.0 / p.hull_b) * kb * level as f32;
    }
    let mut da = da.clamp(0.0, p.hull_a);
    let mut lost_a = 1.0 / p.hull_a * da * p.crew_a;
    let result = if limit_a <= lost_a || sink_a <= da {
        PairResult::BWins
    } else if limit_b <= lost_b || sink_b <= db {
        PairResult::AWins
    } else {
        loop {
            let da_old = da;
            da += (p.hull_b - db) * (1.0 / p.hull_b) * kb;
            lost_a = 1.0 / p.hull_a * da * p.crew_a;
            db += (p.hull_a - da_old) * (1.0 / p.hull_a) * ka;
            lost_b = 1.0 / p.hull_b * db * p.crew_b;
            if (limit_a * SHAKEN < lost_a && limit_b <= lost_b)
                || (sink_a * SHAKEN < da && sink_b <= db)
                || (limit_a <= lost_a && limit_b * SHAKEN < lost_b)
                || (sink_a <= da && sink_b * SHAKEN < db)
            {
                break PairResult::Draw;
            }
            if limit_a <= lost_a || sink_a <= da {
                break PairResult::BWins;
            }
            if !(lost_b < limit_b && db < sink_b) {
                break PairResult::AWins;
            }
        }
    };
    let (crew_a, crew_b) = if p.crew_a == 0.0 { (0.0, 0.0) } else { (lost_a / p.crew_a, lost_b / p.crew_a) };
    let hull_a = if da < sink_a { da / p.hull_a } else { 1.0 };
    let hull_b = if db < sink_b { db / p.hull_b } else { 1.0 };
    NavalSample { result, crew_a, crew_b, hull_a, hull_b }
}

/// The statistics of one pair or unit: crews (`.0`, as land) and hulls (`.1`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NavalStats {
    /// Probabilities and crew casualties.
    pub crew: Stats,
    /// The same probabilities, hull fractions as the casualties.
    pub hull: Stats,
}

impl NavalStats {
    fn flipped(self) -> Self {
        NavalStats { crew: self.crew.flipped(), hull: self.hull.flipped() }
    }
}

fn combine(list: &[NavalStats]) -> NavalStats {
    NavalStats {
        crew: combine_stats(&list.iter().map(|s| s.crew).collect::<Vec<_>>()),
        hull: combine_stats(&list.iter().map(|s| s.hull).collect::<Vec<_>>()),
    }
}

/// The pair query `0x0078F5A0` → `0x007201A0` → `0x0078EE10`: 64 fuzzed engagements.
fn pair_stats(a: &NavalUnit, b: &NavalUnit, max_morale: f32) -> NavalStats {
    let p = NavalPair {
        crew_a: a.state.crew() as f32,
        crew_b: b.state.crew() as f32,
        hull_a: a.rules.hull.max(1.0),
        hull_b: b.rules.hull.max(1.0),
        dmg_a: a.hull_damage(),
        dmg_b: b.hull_damage(),
        sink_a: a.rules.sink_weight,
        sink_b: b.rules.sink_weight,
    };
    // The range level `0x0078F5A0`: A's range class − B's, ∓ 1 by the advantage, which is 0 at query time
    // (CONFIRMED: 188 of 188 logged queries had level = class difference).
    let level = a.rules.range - b.rules.range;
    // 0x0078D6C0 with the advantage 0 at query time.
    let pa = if a.potential() == 0.0 { 1.0 } else { a.potential() };
    let pb = if b.potential() == 0.0 { 1.0 } else { b.potential() };
    let ka = K_NAVAL * (pa / pb);
    let kb = K_NAVAL * (pb / pa);
    // 0x0078FFA0.
    let rout = |u: &NavalUnit, crew: f32| {
        let m = if max_morale > 0.0 { u.rules.morale / max_morale } else { 0.0 };
        ROUT * crew * (1.0 - m)
    };
    let (ra, rb) = (rout(a, p.crew_a), rout(b, p.crew_b));
    let lo = 1.0 - FUZZ;
    let hi = FUZZ + 1.0;
    let both = |x: f32| [lo * x, hi * x];
    let mut samples = Vec::with_capacity(64);
    // The six inputs are (kA, kA, routA, kB, kB, routB) as on land; the engagement reads the second rate of each
    // side (p3, p6), so the first ones only double the samples.
    for _p2 in both(ka) {
        for p3 in both(ka) {
            for p4 in both(ra) {
                for _p5 in both(kb) {
                    for p6 in both(kb) {
                        for p7 in both(rb) {
                            samples.push(engagement(&p, p3, p4, p6, p7, level));
                        }
                    }
                }
            }
        }
    }
    let crew: Vec<PairSample> = samples.iter().map(|s| PairSample { result: s.result, cas_a: s.crew_a, cas_b: s.crew_b }).collect();
    let hull: Vec<PairSample> = samples.iter().map(|s| PairSample { result: s.result, cas_a: s.hull_a, cas_b: s.hull_b }).collect();
    NavalStats { crew: sample_stats(&crew), hull: sample_stats(&hull) }
}

/// The result of [`resolve`].
#[derive(Debug, Clone, PartialEq)]
pub struct NavalOutcome {
    /// True if side A won.
    pub a_won: bool,
    /// The victory type.
    pub victory: VictoryType,
    /// A wins, B wins, draw.
    pub probabilities: [f32; 3],
    /// Side potentials.
    pub potentials: (f32, f32),
    /// Captured ships: (index on the losing side, the captor's force index on the winning side). Their states are
    /// already the captured ones ([`capture_ship`]).
    pub captured: Vec<(usize, usize)>,
}

fn roll(rng: &mut CaRng) -> f32 {
    rng.next16() as f32 * INV_65535
}

struct Ctx<'a> {
    is_a: bool,
    won: bool,
    victory: VictoryType,
    battle: &'a Stats,
    v: &'a NavalVars,
}

/// `0x007C7430` (as the land resolver's).
fn adjust_by_probabilities(c: &Ctx, rate: f32) -> f32 {
    let p = c.battle;
    let (own, diff) = if c.is_a { (p.p_a, p.p_a - p.p_b) } else { (p.p_b, p.p_b - p.p_a) };
    let f = own + p.p_draw * 0.5;
    let mut r = rate;
    if !c.won {
        if diff > 0.0 {
            r -= diff * f;
        }
    } else if diff < 0.0 {
        r += (1.0 - f) * diff.abs();
    }
    r.clamp(0.0, 1.0)
}

/// `0x0078F6A0`: a ship's damage rate from its hull statistics (own point of view). Which outcome mean each
/// branch reads is INFERRED (the decompile shows one offset for both: own win for a winner, other win for a loser).
fn loss_rate(c: &Ctx, st: &NavalStats, rng: &mut CaRng) -> f32 {
    let s = &st.hull.a;
    let p = &st.hull;
    let fuzzed = |m: f32, sd: f32, rng: &mut CaRng| (roll(rng) * sd - sd * 0.5) + m;
    let r = if !c.won {
        if 1.0 <= p.p_draw + p.p_a {
            if p.p_draw > 0.0 && s.draw > 0.0 { fuzzed(s.draw, s.sd_draw, rng) } else { fuzzed(s.mean, s.sd, rng) }
        } else if s.other_win > 0.0 {
            fuzzed(s.other_win, s.sd_other_win, rng)
        } else {
            fuzzed(s.mean, s.sd, rng)
        }
    } else if p.p_a <= 0.0 {
        if p.p_draw > 0.0 && s.draw > 0.0 { fuzzed(s.draw, s.sd_draw, rng) } else { fuzzed(s.mean, s.sd, rng) }
    } else if s.own_win > 0.0 {
        fuzzed(s.own_win, s.sd_own_win, rng)
    } else {
        fuzzed(s.mean, s.sd, rng)
    };
    let r = adjust_by_probabilities(c, r);
    let k = match c.victory {
        VictoryType::Close => Some(0),
        VictoryType::Decisive => Some(1),
        VictoryType::Crushing => Some(2),
        VictoryType::Undecided => None,
    };
    let r = match k {
        Some(k) if c.won => r - c.v.win[k] * r,
        Some(k) => (1.0 - r) * c.v.lose[k] + r,
        None => r,
    };
    r.clamp(0.0, 1.0)
}

/// `0x0074FA70`: applies a damage rate to a ship (`flees` = the exe's last flag, never set here).
fn apply_damage(c: &Ctx, u: &mut NavalUnit, st: &NavalStats, rng: &mut CaRng) {
    let mut r = loss_rate(c, st, rng);
    if c.won && u.rules.merchant {
        r *= 0.95;
    }
    let fz = c.v.damage_fuzz;
    if fz != 0.0 {
        r = (roll(rng) * fz - fz * 0.5 + r).clamp(0.0, 1.0);
    }
    let d = &mut u.state.damage;
    let part = |rng: &mut CaRng| rng.next16() as f32 * 3.814_755_5e-6;
    let (hi, lo) = if 0.5 <= roll(rng) {
        d[2] += (1.0 - d[2]) * r;
        d[0] += part(rng) * r * (1.0 - d[0]);
        d[3] += (1.0 - d[3]) * r;
        d[1] += part(rng) * r * (1.0 - d[1]);
        (d[1], d[3])
    } else {
        d[0] += (1.0 - d[0]) * r;
        d[2] += part(rng) * r * (1.0 - d[2]);
        d[1] += (1.0 - d[1]) * r;
        d[3] += part(rng) * r * (1.0 - d[3]);
        (d[1], d[3])
    };
    if hi.max(lo) < c.v.sink {
        r *= roll(rng) * 0.35 + 0.6;
    } else {
        sink(u);
        return;
    }
    let factor = ai_loss_factor(&ArUnit { data: Default::default(), men: 0, has_general: false, general_rank: 0, human: u.human }, &c.v.land);
    let crews = &mut u.state.crews;
    for i in [1usize, 2, 0] {
        let mut lost = (crews[i] as f32 * r) as i32;
        if factor != 0.0 {
            lost = (lost as f32 * factor).round_ties_even() as i32;
        }
        crews[i] -= lost.min(crews[i]);
    }
}

/// The ship sinks (`0x0074FA70` past the sink point): every damage 1, crews and guns 0, the flag.
fn sink(u: &mut NavalUnit) {
    u.state.damage = [1.0; 5];
    u.state.crews = [0; 3];
    u.state.guns = 0;
    u.state.sunk = true;
}

/// `0x007DD800`: a wiped-out side's ship: the damage pairs rise to 0.81..0.90 of what is left plus a random
/// share of the rest, crews 0, the flag set.
fn wipe(u: &mut NavalUnit, rng: &mut CaRng) {
    let a = roll(rng);
    let d = &mut u.state.damage;
    let big = |rng: &mut CaRng| roll(rng) * 0.089_999_996 + 0.81;
    let small = |rng: &mut CaRng| rng.next16() as f32 * 3.433_279_7e-6;
    if a < 0.5 {
        let x = big(rng);
        d[0] += (1.0 - d[0]) * x;
        d[2] += (1.0 - d[2]) * small(rng);
        let y = big(rng);
        d[1] += (1.0 - d[1]) * y;
        d[3] += (1.0 - d[3]) * small(rng);
    } else {
        let x = big(rng);
        d[2] += (1.0 - d[2]) * x;
        d[0] += (1.0 - d[0]) * small(rng);
        let y = big(rng);
        d[3] += (1.0 - d[3]) * y;
        d[1] += (1.0 - d[1]) * small(rng);
    }
    u.state.crews = [0; 3];
    u.state.sunk = true;
}

/// Resolves a naval battle between `a` (attacker) and `b`; the ships' states are updated in place.
pub fn resolve(a: &mut [NavalUnit], b: &mut [NavalUnit], v: &NavalVars, rng: &mut CaRng) -> NavalOutcome {
    let sa: f32 = a.iter().map(NavalUnit::potential).sum();
    let sb: f32 = b.iter().map(NavalUnit::potential).sum();
    if sa + sb <= 0.0 || a.is_empty() || b.is_empty() {
        return NavalOutcome { a_won: b.is_empty(), victory: VictoryType::Undecided, probabilities: [0.0, 1.0, 0.0], potentials: (sa, sb), captured: Vec::new() };
    }
    // The highest morale of all ship types; without rules (tests), the battle's own.
    let max_morale = if v.max_morale > 0.0 { v.max_morale } else { a.iter().chain(b.iter()).map(|u| u.rules.morale).fold(0.0f32, f32::max) };
    let pairs: Vec<Vec<NavalStats>> = a.iter().map(|ua| b.iter().map(|ub| pair_stats(ua, ub, max_morale)).collect()).collect();
    let units_a: Vec<NavalStats> = pairs.iter().map(|row| combine(row)).collect();
    let units_b: Vec<NavalStats> = (0..b.len()).map(|j| combine(&pairs.iter().map(|row| row[j]).collect::<Vec<_>>()).flipped()).collect();
    let all: Vec<NavalStats> = pairs.iter().flatten().copied().collect();
    let mut battle = combine(&all);
    finalize_probabilities(&mut battle.crew, sa, sb, &v.land, 0, 0);
    let (a_won, victory) = roll_winner(&battle.crew, &v.land, rng);
    let adv = advantage(sa, sb);
    // The crews each ship had going in (the card's +0x70..+0x78, which a capture restores before scaling).
    let before_a: Vec<[i32; 3]> = a.iter().map(|u| u.state.crews).collect();
    let before_b: Vec<[i32; 3]> = b.iter().map(|u| u.state.crews).collect();
    let t = v.land.wipeout_threshold;
    for (is_a, units, stats) in [(true, &mut *a, &units_a), (false, &mut *b, &units_b)] {
        let won = a_won == is_a;
        let c = Ctx { is_a, won, victory, battle: &battle.crew, v };
        let wiped = !won && if is_a { adv > t } else { adv < -t };
        for (u, st) in units.iter_mut().zip(stats) {
            if wiped {
                wipe(u, rng);
            } else {
                apply_damage(&c, u, st, rng);
            }
        }
    }
    // The captives step `0x0074F710` (after the damage, on the losing side).
    let captured = if a_won { captives(&*a, b, &units_b, &before_b, v, rng) } else { captives(&*b, a, &units_a, &before_a, v, rng) };
    NavalOutcome { a_won, victory, probabilities: [battle.crew.p_a, battle.crew.p_b, battle.crew.p_draw], potentials: (sa, sb), captured }
}

/// The captives step `0x0074F710` → `0x00792CD0` / `0x00793D60` / `0x0075C090` (CONFIRMED unless tagged): which
/// sunk ships of the losing side the winners take.
/// - Candidates: the losing ships whose sunk flag is set, ordered by their own win probability (the first float
///   of their statistics block, a stable ascending merge sort); the first `round((1 − best_kills) × n)` drop out,
///   so the best `best_kills` share stays (all of it when there is one).
/// - Shares: each winning force gets `round(w / Σw × candidates)`, `w` its ships' crew total; the force with the
///   single highest ship value (+0xE8 of its setup records, UNKNOWN: here the most guns, PROVISIONAL) counts
///   twice. With one winning force (the usual case) it takes every candidate.
/// - Draws, force by force: a random candidate (`rand % left`), then a roll; above `not_capturing` the ship is
///   captured ([`capture_ship`]), else it stays sunk; either way it leaves the list.
fn captives(winners: &[NavalUnit], losers: &mut [NavalUnit], stats: &[NavalStats], before: &[[i32; 3]], v: &NavalVars, rng: &mut CaRng) -> Vec<(usize, usize)> {
    let mut cand: Vec<usize> = (0..losers.len()).filter(|&i| losers[i].state.sunk).collect();
    cand.sort_by(|&x, &y| stats[x].crew.p_a.total_cmp(&stats[y].crew.p_a));
    let n = cand.len();
    if n != 1 {
        let skip = (((1.0 - v.best_kills) * n as f32).round_ties_even() as usize).min(n);
        cand.drain(..skip);
    }
    if cand.is_empty() {
        return Vec::new();
    }
    // Per winning force: crew total and best ship.
    let forces = winners.iter().map(|u| u.force + 1).max().unwrap_or(0);
    let mut weight = vec![0i64; forces];
    let mut best = vec![0u32; forces];
    for u in winners {
        weight[u.force] += i64::from(u.state.crew());
        best[u.force] = best[u.force].max(if u.rules.guns > 0 { u.rules.guns } else { u.state.max_guns });
    }
    let top = best.iter().copied().max().unwrap_or(0);
    let mut total: i64 = weight.iter().sum();
    if best.iter().filter(|&&b| b == top).count() == 1 {
        let i = best.iter().position(|&b| b == top).unwrap_or(0);
        total += weight[i];
        weight[i] *= 2;
    }
    if total == 0 {
        return Vec::new();
    }
    let count: Vec<usize> = weight.iter().map(|&w| ((w as f32) * (1.0 / total as f32) * cand.len() as f32).round_ties_even().max(0.0) as usize).collect();
    let mut out = Vec::new();
    'forces: for (f, &k) in count.iter().enumerate() {
        for _ in 0..k {
            if cand.is_empty() {
                break 'forces;
            }
            let pick = (rng.next16() as usize) % cand.len();
            let r = roll(rng);
            if v.not_capturing < r {
                let i = cand[pick];
                capture_ship(&mut losers[i], before[i], v, rng);
                out.push((i, f));
            }
            cand.remove(pick);
        }
    }
    out
}

/// A ship changes sides (`0x007CD570`, ship class slot 5, CONFIRMED): its damage fractions are multiplied by
/// `clamp(capture_mult − 0.2 + rand × 0.4, 0.25, 0.6)`; its crews become the crews it went in with (INFERRED: the card's +0x70..+0x78) ×
/// (0.3 + rand × 0.3), truncated; the sunk flag is cleared.
pub fn capture_ship(u: &mut NavalUnit, before: [i32; 3], v: &NavalVars, rng: &mut CaRng) {
    let f = (rng.next16() as f32 * (0.4 / 65535.0) + (v.capture_mult - 0.2)).clamp(0.25, 0.6);
    for d in &mut u.state.damage {
        *d *= f;
    }
    let c = rng.next16() as f32 * (0.3 / 65535.0) + 0.3;
    u.state.crews = before.map(|x| (x as f32 * c) as i32);
    u.state.sunk = false;
}

//! The campaign's stat-based autoresolver (`EmpireAutoresolverStatBased`), land battles.
//!
//! Port of the original's pipeline (CAMPAIGN_FIDELITY.md §Autoresolve has the addresses and tags):
//! 1. **Setup** (`0x0078D9E0`): every unit's potential (`0x00790030`), the side sums `SA` / `SB`,
//!    then a fuzzy engagement for every (A unit, B unit) pair (`0x0078F460` → `0x0078E8A0` →
//!    64 × `0x00759860`), aggregated per pair (`0x007338D0`), per unit (`0x00734EF0`, B units
//!    flipped by `0x00796620`) and for the battle. The battle's outcome probabilities are weighted
//!    by the side potentials (`0x007DD740`), blended with a potential-only prediction (`0x007930A0`
//!    / `0x0075DBA0`), adjusted by the commanders' star ratings and normalised.
//! 2. **Winner** (`0x007690C0`): one gaussian roll decides the winner and the victory type.
//! 3. **Losses** (`0x0074F450` → `0x0074FF40` per side): the advantage `adv`, the wipeout rule,
//!    then every unit's loss rate (`0x0078FA30`) and its application (`0x0074F840`), then the
//!    reshuffle of losses between the two halves of the side.
//!
//! The kill rates of a pair are `ntw_sim::battle::autoresolve::kill_rates` (`0x0078D200`,
//! CONFIRMED). The engagement loop is [`engagement`], a port of `0x00759860` (the battle crate's
//! `engage` is a simplified version that differs; reported to the manager).
//!
//! Side A is the attacker, side B the defenders (INFERRED). All random numbers come from one
//! MS LCG ([`CaRng`]); the original uses the resolver's own LCG (seeded from the campaign's,
//! `0x0070D370`, seed rule not read: PROVISIONAL).

use crate::battle::autoresolve::{kill_rates, AutoresolveQuery, AutoresolveTweaks};
use crate::rng::{CaRng, INV_65535};

use super::rules::{CampaignRules, UnitAutoresolve};

/// One unit in an autoresolved battle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArUnit {
    /// The unit's autoresolve data.
    pub data: UnitAutoresolve,
    /// Current men (the card's `+0x54`).
    pub men: u32,
    /// The unit carries its army's general (card `+0xC0` in {1, 2, 4, 5}).
    pub has_general: bool,
    /// The general's star rating (unit `+0xAC`; 0 without a general).
    pub general_rank: u32,
    /// The unit belongs to a human player (unit `+0xB8`).
    pub human: bool,
}

/// The `campaign_variables` the resolver reads (through a pointer to the 110-float array;
/// index in brackets, CONFIRMED offsets).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArVars {
    /// \[45] `autoresolve_stat_massacre_chance`.
    pub massacre_chance: f32,
    /// \[46] `autoresolve_unit_losses_fuzziness`.
    pub losses_fuzziness: f32,
    /// \[47] `autoresolve_gaussian_boundary`.
    pub gaussian_boundary: f32,
    /// \[48] `autoresolve_gaussian_standard_deviation`.
    pub gaussian_sd: f32,
    /// \[52] `autoresolve_min_combat_potential_only_win_chance`.
    pub potential_only_weight: f32,
    /// \[53] `autoresolve_minimum_win_chance_to_win`.
    pub minimum_win_chance: f32,
    /// \[54] `autoresolve_commander_star_rating_impact`.
    pub star_rating_impact: f32,
    /// \[59] `autoresolve_major_land_victory_percent`.
    pub major_victory: f32,
    /// \[60] `autoresolve_minor_land_victory_percent`.
    pub minor_victory: f32,
    /// \[70] `autoresolve_minimum_casualties_on_win`.
    pub min_casualties_win: f32,
    /// \[71] `autoresolve_minimum_casualties_on_lose`.
    pub min_casualties_lose: f32,
    /// \[80] `autoresolve_advantage_over_enemy_wipeout_threshold`.
    pub wipeout_threshold: f32,
    /// \[82] `losing_unit_minimum_strength` (the pursuit roll, `0x0074F170`).
    pub losing_unit_minimum_strength: f32,
    /// \[74] `autoresolve_easy_difficulty_human_advantage`.
    pub easy_human_advantage: f32,
    /// \[75] `autoresolve_hard_difficulty_AI_advantage`.
    pub hard_ai_advantage: f32,
    /// \[76] `autoresolve_very_hard_difficulty_AI_advantage`.
    pub very_hard_ai_advantage: f32,
    /// \[77] `autoresolve_easy_campaign_AI_percent_reduction`.
    pub easy_ai_losses: f32,
    /// \[78] `autoresolve_hard_campaign_AI_percent_increase`.
    pub hard_ai_losses: f32,
    /// \[79] `autoresolve_very_hard_campaign_AI_percent_increase`.
    pub very_hard_ai_losses: f32,
    /// The battle's difficulty (unit `+0xB0` for the potential, `+0xB4` for the losses): 1 easy,
    /// 0 normal, −1 hard, −2 very hard. The caller sets it from the human player's campaign
    /// difficulty (`FactionDetails::difficulty`; both fields INFERRED to hold it), 0 without one.
    pub difficulty: i32,
}

impl ArVars {
    /// Reads the variables from the rules (0 when a row is missing).
    pub fn from_rules(r: &CampaignRules) -> Self {
        ArVars {
            massacre_chance: r.var("autoresolve_stat_massacre_chance", 0.0),
            losses_fuzziness: r.var("autoresolve_unit_losses_fuzziness", 0.0),
            gaussian_boundary: r.var("autoresolve_gaussian_boundary", 0.0),
            gaussian_sd: r.var("autoresolve_gaussian_standard_deviation", 0.0),
            potential_only_weight: r.var("autoresolve_min_combat_potential_only_win_chance", 0.0),
            minimum_win_chance: r.var("autoresolve_minimum_win_chance_to_win", 0.0),
            star_rating_impact: r.var("autoresolve_commander_star_rating_impact", 0.0),
            major_victory: r.var("autoresolve_major_land_victory_percent", 0.0),
            minor_victory: r.var("autoresolve_minor_land_victory_percent", 0.0),
            min_casualties_win: r.var("autoresolve_minimum_casualties_on_win", 0.0),
            min_casualties_lose: r.var("autoresolve_minimum_casualties_on_lose", 0.0),
            wipeout_threshold: r.var("autoresolve_advantage_over_enemy_wipeout_threshold", 0.0),
            losing_unit_minimum_strength: r.var("losing_unit_minimum_strength", 0.0),
            easy_human_advantage: r.var("autoresolve_easy_difficulty_human_advantage", 0.0),
            hard_ai_advantage: r.var("autoresolve_hard_difficulty_AI_advantage", 0.0),
            very_hard_ai_advantage: r.var("autoresolve_very_hard_difficulty_AI_advantage", 0.0),
            easy_ai_losses: r.var("autoresolve_easy_campaign_AI_percent_reduction", 0.0),
            hard_ai_losses: r.var("autoresolve_hard_campaign_AI_percent_increase", 0.0),
            very_hard_ai_losses: r.var("autoresolve_very_hard_campaign_AI_percent_increase", 0.0),
            difficulty: 0,
        }
    }
}

/// The victory type (resolver `+0x28`, set by `0x007690C0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VictoryType {
    /// 0: the massacre branch; loss rates use `autoresolve_major_land_victory_percent`.
    Crushing,
    /// 1: a clear win.
    Decisive,
    /// 2: a close win; loss rates use `autoresolve_minor_land_victory_percent`.
    Close,
    /// 3: the roll fell past every bucket (no type set).
    Undecided,
}

/// The result of [`resolve`].
#[derive(Debug, Clone, PartialEq)]
pub struct ArOutcome {
    /// True if side A (the attacker) won.
    pub a_won: bool,
    /// The victory type.
    pub victory: VictoryType,
    /// Outcome probabilities after setup: A wins, B wins, draw.
    pub probabilities: [f32; 3],
    /// The advantage `adv` (`0x0078FE60(1)`; > 0 means B is the stronger side).
    pub advantage: f32,
    /// Side potentials `SA`, `SB`.
    pub potentials: (f32, f32),
    /// Men lost per A unit (same order as the input).
    pub losses_a: Vec<u32>,
    /// Men lost per B unit.
    pub losses_b: Vec<u32>,
}

// ---------------------------------------------------------------------------------------------
// Engagement and statistics.

/// Shaken point of the engagement (`autoresolve` tweaker, CONFIRMED 0.9).
const SHAKEN: f32 = 0.9;
/// Per-step land kill multiplier (CONFIRMED 0.1).
const LAND_KILL_MULT: f32 = 0.1;
/// Fuzz of the 64-sample pair query (CONFIRMED 0.2).
const PAIR_FUZZ: f32 = 0.2;
/// Base rout point (CONFIRMED 0.4).
const ROUT: f32 = 0.4;

/// Outcome code of [`engagement`] (`0x00759860`, CONFIRMED codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PairResult {
    /// 0: B reached its rout point (A wins).
    AWins,
    /// 1: both broke (draw).
    Draw,
    /// 2: A reached its rout point first (B wins).
    BWins,
}

/// One engagement: the result and the casualty fractions of A and B.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairSample {
    /// Who won.
    pub result: PairResult,
    /// Fraction of A's men lost.
    pub cas_a: f32,
    /// Fraction of B's men lost.
    pub cas_b: f32,
}

/// The engagement loop `0x00759860` (CONFIRMED, decompiled):
/// - inputs: kill rates `k*_mel` / `k*_mis`, rout points `rout*` (men left when the unit breaks),
///   the men, and the range level (> 0: A fires first, < 0: B fires first);
/// - a pre-phase where the side with the longer range kills `men × max(k_mel, k_mis) × |level|`;
/// - the missile rates are used unless either side's missile rate is not above its melee rate,
///   then both use melee;
/// - each step, both sides lose `remaining_enemy × 0.1 × enemy_rate` (from the remaining men at
///   the start of the step), at least 0.05, at most the side's men;
/// - it stops when B breaks (A wins), when A breaks (B wins, unless B is past 0.9 of its rout
///   point: draw), or when A is past 0.9 of its rout point as B breaks (draw).
#[allow(clippy::too_many_arguments)]
pub fn engagement(
    ka_mel: f32,
    ka_mis: f32,
    rout_a: f32,
    kb_mel: f32,
    kb_mis: f32,
    rout_b: f32,
    men_a: u32,
    men_b: u32,
    level: i32,
) -> PairSample {
    let ma = men_a as f32;
    let mb = men_b as f32;
    let limit_a = ma - rout_a;
    let limit_b = mb - rout_b;
    let mut dead_b = 0.0f32;
    if level > 0 && mb > 0.0 {
        dead_b = ma * ka_mis.max(ka_mel) * level as f32;
    }
    let mut cas_b = dead_b.min(mb);
    let mut dead_a = 0.0f32;
    if level < 0 && ma > 0.0 {
        dead_a = (mb - cas_b) * kb_mis.max(kb_mel) * (-level) as f32;
    }
    let mut cas_a = dead_a.min(ma);
    let result = if limit_a <= cas_a {
        PairResult::BWins
    } else if cas_b < limit_b {
        let (mut rate_a, mut rate_b) = (ka_mis, kb_mis);
        if ka_mis <= ka_mel || kb_mis <= kb_mel {
            rate_a = ka_mel;
            rate_b = kb_mel;
        }
        loop {
            let rem_b = mb - cas_b;
            let rem_a = ma - cas_a;
            let da = (rem_b * LAND_KILL_MULT * rate_b).min(rem_a);
            let db = (rem_a * LAND_KILL_MULT * rate_a).min(rem_b);
            let da = if da < 0.05 { 0.05 } else { da.min(ma) };
            let db = if db < 0.05 { 0.05 } else { db.min(mb) };
            cas_a += da;
            cas_b += db;
            if limit_a * SHAKEN < cas_a && limit_b <= cas_b {
                break PairResult::Draw;
            }
            if limit_a <= cas_a {
                break if limit_b * SHAKEN < cas_b { PairResult::Draw } else { PairResult::BWins };
            }
            if cas_b >= limit_b {
                break PairResult::AWins;
            }
        }
    } else {
        PairResult::AWins
    };
    let frac = |cas: f32, men: f32, n: u32| if n == 0 { 0.0 } else { cas.clamp(0.0, men) / men };
    PairSample { result, cas_a: frac(cas_a, ma, men_a), cas_b: frac(cas_b, mb, men_b) }
}

/// One side's casualty statistics (16 floats of the 0x90 block; only the first 8 are used for
/// land battles, the other 8 are the naval damage fields).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SideStats {
    /// Mean casualty fraction when this side wins.
    pub own_win: f32,
    /// Mean casualty fraction when the other side wins.
    pub other_win: f32,
    /// Mean casualty fraction in a draw.
    pub draw: f32,
    /// Standard deviation of `own_win`.
    pub sd_own_win: f32,
    /// Standard deviation of `other_win`.
    pub sd_other_win: f32,
    /// Standard deviation of `draw`.
    pub sd_draw: f32,
    /// Mean casualty fraction over all samples.
    pub mean: f32,
    /// Its standard deviation.
    pub sd: f32,
}

/// The 0x90-byte statistics block (`0x007338D0` / `0x00734EF0` layout, CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    /// \[0] probability that A wins.
    pub p_a: f32,
    /// \[1] probability that B wins.
    pub p_b: f32,
    /// \[2] probability of a draw.
    pub p_draw: f32,
    /// \[3..=0x12] A's casualties.
    pub a: SideStats,
    /// \[0x13..=0x22] B's casualties.
    pub b: SideStats,
}

impl Stats {
    /// `0x00796620`: swaps the two sides (B units see themselves as "A").
    pub fn flipped(self) -> Self {
        Stats { p_a: self.p_b, p_b: self.p_a, p_draw: self.p_draw, a: self.b, b: self.a }
    }
}

/// Mean and population standard deviation (`0x010E1D60` / `0x010E1E30`, CONFIRMED population;
/// the original sums in 8 lanes, we sum in order: float rounding may differ in the last bits).
fn mean_sd(v: &[f32]) -> (f32, f32) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    let n = v.len() as f32;
    let mean = v.iter().sum::<f32>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / n;
    (mean, var.sqrt())
}

/// `0x007338D0`: statistics of a pair's engagement samples.
pub fn sample_stats(samples: &[PairSample]) -> Stats {
    let n = samples.len() as f32;
    if samples.is_empty() {
        return Stats::default();
    }
    let pick = |r: PairResult, f: fn(&PairSample) -> f32| -> Vec<f32> {
        samples.iter().filter(|s| s.result == r).map(f).collect()
    };
    let count = |r: PairResult| samples.iter().filter(|s| s.result == r).count() as f32;
    let side = |own: PairResult, other: PairResult, f: fn(&PairSample) -> f32| {
        let (own_win, sd_own_win) = mean_sd(&pick(own, f));
        let (other_win, sd_other_win) = mean_sd(&pick(other, f));
        let (draw, sd_draw) = mean_sd(&pick(PairResult::Draw, f));
        let (mean, sd) = mean_sd(&samples.iter().map(f).collect::<Vec<_>>());
        SideStats { own_win, other_win, draw, sd_own_win, sd_other_win, sd_draw, mean, sd }
    };
    Stats {
        p_a: count(PairResult::AWins) / n,
        p_b: count(PairResult::BWins) / n,
        p_draw: count(PairResult::Draw) / n,
        a: side(PairResult::AWins, PairResult::BWins, |s| s.cas_a),
        b: side(PairResult::BWins, PairResult::AWins, |s| s.cas_b),
    }
}

/// `0x00734EF0`: statistics over several pair statistics. Probabilities and overall means are
/// averaged over all entries; the per-outcome means only over the entries where that outcome
/// has a non-zero probability; the deviations are the spread of the entries' means (0 for a
/// single entry).
pub fn combine_stats(list: &[Stats]) -> Stats {
    let n = list.len() as f32;
    if list.is_empty() {
        return Stats::default();
    }
    let mut out = Stats {
        p_a: list.iter().map(|s| s.p_a).filter(|p| *p != 0.0).sum::<f32>() / n,
        p_b: list.iter().map(|s| s.p_b).filter(|p| *p != 0.0).sum::<f32>() / n,
        p_draw: list.iter().map(|s| s.p_draw).filter(|p| *p != 0.0).sum::<f32>() / n,
        ..Default::default()
    };
    (out.a.mean, out.a.sd) = mean_sd(&list.iter().map(|s| s.a.mean).collect::<Vec<_>>());
    (out.b.mean, out.b.sd) = mean_sd(&list.iter().map(|s| s.b.mean).collect::<Vec<_>>());
    let a_won: Vec<&Stats> = list.iter().filter(|s| s.p_a != 0.0).collect();
    let b_won: Vec<&Stats> = list.iter().filter(|s| s.p_b != 0.0).collect();
    let drawn: Vec<&Stats> = list.iter().filter(|s| s.p_draw != 0.0).collect();
    let ms = |v: &[&Stats], f: fn(&Stats) -> f32| mean_sd(&v.iter().map(|s| f(s)).collect::<Vec<_>>());
    (out.a.own_win, out.a.sd_own_win) = ms(&a_won, |s| s.a.own_win);
    (out.b.other_win, out.b.sd_other_win) = ms(&a_won, |s| s.b.other_win);
    (out.a.other_win, out.a.sd_other_win) = ms(&b_won, |s| s.a.other_win);
    (out.b.own_win, out.b.sd_own_win) = ms(&b_won, |s| s.b.own_win);
    (out.a.draw, out.a.sd_draw) = ms(&drawn, |s| s.a.draw);
    (out.b.draw, out.b.sd_draw) = ms(&drawn, |s| s.b.draw);
    out
}

// ---------------------------------------------------------------------------------------------
// Setup.

/// The unit's potential (`0x00790030`, land, CONFIRMED): melee + missile record potential for its
/// men, times `1 + handicap` and rounded when the battle's difficulty gives one: a human unit on
/// easy gets [74]; an AI unit on hard [75], on very hard [76].
/// The ×0.3 / ×0.6 for attacking cavalry in some battle types (`+0xBC` in {3, 5, 6, 8}) is not
/// applied (field battles only; the battle-type codes are UNKNOWN).
fn potential(u: &ArUnit, v: &ArVars) -> f32 {
    let p = u.data.melee(u.men) + u.data.missile(u.men);
    let h = match (u.human, v.difficulty) {
        (true, 1) => Some(v.easy_human_advantage),
        (false, -1) => Some(v.hard_ai_advantage),
        (false, -2) => Some(v.very_hard_ai_advantage),
        _ => None,
    };
    match h {
        Some(h) if h + 1.0 != 0.0 => ((h + 1.0) * p).round_ties_even(),
        _ => p,
    }
}

/// `0x00793010` (CONFIRMED): the AI's loss factor by difficulty: easy `1 + [77]`, hard `1 − [78]`,
/// very hard `1 − [79]`; 0 (none) for human units and at normal difficulty.
pub(crate) fn ai_loss_factor(u: &ArUnit, v: &ArVars) -> f32 {
    if u.human {
        return 0.0;
    }
    match v.difficulty {
        1 => v.easy_ai_losses + 1.0,
        -1 => 1.0 - v.hard_ai_losses,
        -2 => 1.0 - v.very_hard_ai_losses,
        _ => 0.0,
    }
}

/// Shock units for the pursuit roll: cavalry, elephants, camels (CONFIRMED codes 0, 4, 5).
fn shock(cat: u8) -> bool {
    matches!(cat, 0 | 4 | 5)
}

/// `0x00758830` (CONFIRMED): the missile modifier `r2` of every pair query, from the share of
/// units that do not pass `0x007CB8A0` (artillery; INFERRED: the class code 0x15 that also passes
/// is not mapped and ignored).
fn missile_modifier(n_a: usize, art_a: usize, n_b: usize, art_b: usize) -> f32 {
    let share = |n: usize, art: usize| if n == 0 { 0.0 } else { (n - art) as f32 / n as f32 };
    let fa = share(n_a, art_a);
    let fb = share(n_b, art_b);
    if fa == 0.0 && fb != 0.0 {
        return -fb;
    }
    if fa < 0.5 && fa + 0.2 < fb {
        return fa - fb;
    }
    if fb == 0.0 && fa != 0.0 {
        return fa;
    }
    if fb < 0.5 && fb + 0.2 < fa {
        return fa - fb;
    }
    0.0
}

/// The range class (unit `+0x9C`): artillery 2, other land units 1 (INFERRED).
fn range_class(u: &ArUnit) -> i32 {
    if u.data.category == 1 { 2 } else { 1 }
}

/// The pair query `0x0078F460` → `0x0071A1F0` → `0x0078E8A0` with fuzz on: 64 engagements with
/// every input scaled by `1 ∓ 0.2`, aggregated.
fn pair_stats(a: &ArUnit, b: &ArUnit, r2: f32, max_morale: f32) -> Stats {
    // The advantage at query time is 0 (the resolver computes it after setup; INFERRED).
    let adv = 0.0f32;
    let mut level = range_class(a) - range_class(b);
    if adv >= 0.5 {
        level -= 1;
    } else if adv <= -0.5 {
        level += 1;
    }
    // (`B +0xE4` set and B infantry → level − 1: the flag is UNKNOWN, taken as clear.)
    let q = AutoresolveQuery {
        start_men_a: a.men as f32,
        start_men_b: b.men as f32,
        men_a: a.men as f32,
        men_b: b.men as f32,
        r: adv,
        r2,
        melee_a: a.data.melee(a.men),
        melee_b: b.data.melee(b.men),
        missile_a: a.data.missile(a.men),
        missile_b: b.data.missile(b.men),
    };
    let k = kill_rates(&q, &AutoresolveTweaks::default());
    let rout = |u: &ArUnit| {
        let m = if max_morale > 0.0 { u.data.morale / max_morale } else { 0.0 };
        ROUT * (1.0 - m) * u.men as f32
    };
    let lo = 1.0 - PAIR_FUZZ;
    let hi = PAIR_FUZZ + 1.0;
    let both = |x: f32| [lo * x, hi * x];
    let mut samples = Vec::with_capacity(64);
    for p2 in both(k.melee_a) {
        for p3 in both(k.missile_a) {
            for p4 in both(rout(a)) {
                for p5 in both(k.melee_b) {
                    for p6 in both(k.missile_b) {
                        for p7 in both(rout(b)) {
                            samples.push(engagement(p2, p3, p4, p5, p6, p7, a.men, b.men, level));
                        }
                    }
                }
            }
        }
    }
    sample_stats(&samples)
}

/// Everything the loss stage reads from the setup.
struct Setup {
    /// Side potentials.
    sa: f32,
    sb: f32,
    /// Potentials of the shock units (resolver `+0x34` / `+0x38`).
    shock_a: f32,
    shock_b: f32,
    /// Per-unit statistics, each from the unit's own point of view.
    units_a: Vec<Stats>,
    units_b: Vec<Stats>,
    /// Battle statistics (resolver `+0x40`) with the final probabilities.
    battle: Stats,
    /// Resolver `+0x3C`: the prediction replaced the simulated probabilities.
    prediction_used: bool,
}

fn setup(a: &[ArUnit], b: &[ArUnit], v: &ArVars) -> Setup {
    let pot_a: Vec<f32> = a.iter().map(|u| potential(u, v)).collect();
    let pot_b: Vec<f32> = b.iter().map(|u| potential(u, v)).collect();
    let sa: f32 = pot_a.iter().sum();
    let sb: f32 = pot_b.iter().sum();
    let shock_a: f32 = a.iter().zip(&pot_a).filter(|(u, _)| shock(u.data.category)).map(|(_, p)| p).sum();
    let shock_b: f32 = b.iter().zip(&pot_b).filter(|(u, _)| shock(u.data.category)).map(|(_, p)| p).sum();
    let art = |s: &[ArUnit]| s.iter().filter(|u| u.data.category == 1).count();
    let r2 = missile_modifier(a.len(), art(a), b.len(), art(b));
    let max_morale = a.iter().chain(b).map(|u| u.data.morale).fold(0.0f32, f32::max);

    let pairs: Vec<Vec<Stats>> = a.iter().map(|ua| b.iter().map(|ub| pair_stats(ua, ub, r2, max_morale)).collect()).collect();
    let units_a: Vec<Stats> = pairs.iter().map(|row| combine_stats(row)).collect();
    let units_b: Vec<Stats> =
        (0..b.len()).map(|j| combine_stats(&pairs.iter().map(|row| row[j]).collect::<Vec<_>>()).flipped()).collect();
    let all: Vec<Stats> = pairs.iter().flatten().copied().collect();
    let mut battle = combine_stats(&all);
    let rank_a = a.iter().map(|u| u.general_rank).max().unwrap_or(0);
    let rank_b = b.iter().map(|u| u.general_rank).max().unwrap_or(0);

    let prediction_used = finalize_probabilities(&mut battle, sa, sb, v, rank_a, rank_b);
    Setup { sa, sb, shock_a, shock_b, units_a, units_b, battle, prediction_used }
}

/// The battle probabilities after the pair statistics (`0x007DD740` weighting, `0x007930A0` prediction,
/// `0x0075DBA0` blend, star ratings; CONFIRMED): shared by the land and naval resolvers. Returns whether the
/// prediction replaced the simulated probabilities (resolver `+0x3C`).
pub(crate) fn finalize_probabilities(battle: &mut Stats, sa: f32, sb: f32, v: &ArVars, rank_a: u32, rank_b: u32) -> bool {
    // 0x007DD740: weight the simulated probabilities by the side potentials.
    let wa = battle.p_a * sa;
    let wb = battle.p_b * sb;
    let k = 1.0 / ((sb + sa) * 0.5 * battle.p_draw + wb + wa);
    battle.p_b = k * wb;
    battle.p_a = k * wa;
    battle.p_draw = (1.0 - battle.p_a) - battle.p_b;

    // 0x007930A0: the potential-only prediction.
    let x = sa / (sb + sa);
    let qd = if !(0.25..=0.75).contains(&x) {
        0.05
    } else if !(0.4..=0.6).contains(&x) {
        0.2
    } else {
        0.35
    };
    let qn = 1.0 / ((1.0 - x) + x + qd);
    let (qa, qb, qd) = (x * qn, (1.0 - x) * qn, qd * qn);

    // 0x0075DBA0: blend, or take the prediction if the simulation is too far from it.
    let mut prediction_used = false;
    if ((battle.p_a + battle.p_draw * 0.5) - (qa + qd * 0.5)).abs() <= 0.5
        && ((battle.p_b + battle.p_draw * 0.5) - (qb + qd * 0.5)).abs() <= 0.5
    {
        // Resolver `+0x18` is 0 here (INFERRED: never set for a campaign battle).
        let extra = 0.0f32;
        let w = v.potential_only_weight.clamp(0.0, 1.0).max(extra.abs().min(1.0));
        let keep = 1.0 - w;
        battle.p_a = qa * w + battle.p_a * keep;
        battle.p_b = qb * w + battle.p_b * keep;
        battle.p_draw = qd * w + battle.p_draw * keep;
    } else {
        battle.p_a = qa;
        battle.p_b = qb;
        battle.p_draw = qd;
        prediction_used = true;
    }

    // Commander star ratings.
    let diff = rank_a as i64 - rank_b as i64;
    if diff > 0 {
        battle.p_a *= diff as f32 * v.star_rating_impact + 1.0;
    } else if diff < 0 {
        battle.p_b *= (-diff) as f32 * v.star_rating_impact + 1.0;
    }
    let n = 1.0 / (battle.p_b + battle.p_a + battle.p_draw);
    battle.p_a *= n;
    battle.p_b *= n;
    battle.p_draw *= n;

    prediction_used
}

// ---------------------------------------------------------------------------------------------
// Winner.

/// `0x010E19B0`: a gaussian from a local LCG seeded with `seed` (Marsaglia polar method), with
/// the original's bit-trick square root, times `sd`.
fn gaussian(seed: u32, sd: f32) -> f32 {
    let mut rng = CaRng::new(seed);
    let (y, s) = loop {
        let x = rng.next16() as f32 * INV_65535;
        let y = rng.next16() as f32 * INV_65535;
        let x = (x + x) - 1.0;
        let y = (y + y) - 1.0;
        let s = y * y + x * x;
        if s < 1.0 {
            break (y, s);
        }
    };
    let t = s.ln() * -2.0 / s;
    let root = f32::from_bits((((t.to_bits() as i32).wrapping_sub(0x3f80_0000) >> 1).wrapping_add(0x3f80_0000)) as u32);
    root * y * sd
}

/// `0x007690C0` (CONFIRMED): returns (A won, victory type).
pub(crate) fn roll_winner(p: &Stats, v: &ArVars, rng: &mut CaRng) -> (bool, VictoryType) {
    let bound = v.gaussian_boundary;
    let g = gaussian(rng.next16(), v.gaussian_sd).clamp(-bound, bound);
    let roll = ((bound + g) * (0.5 / bound)).clamp(0.01, 0.99);
    let (pa, pb, pd) = (p.p_a, p.p_b, p.p_draw);
    let massacre = v.massacre_chance;
    let (chance_a, chance_b) = if pd == 1.0 {
        (1.0, 1.0)
    } else if pa + pb == 0.0 {
        (0.0, 0.0)
    } else {
        (pd * 0.5 + pa, pd * 0.5 + pb)
    };
    // The minimum win chance forces the winner (the type is still rolled).
    let forced = if chance_a < v.minimum_win_chance {
        Some(false)
    } else if chance_b < v.minimum_win_chance {
        Some(true)
    } else {
        None
    };
    let pick = |a: bool| forced.unwrap_or(a);
    if pa * massacre > roll {
        return (pick(true), VictoryType::Crushing);
    }
    if roll < pa {
        return (pick(true), VictoryType::Decisive);
    }
    let mut c = pd * chance_a + pa;
    if roll < c {
        return (pick(true), VictoryType::Close);
    }
    c += pd * chance_b;
    if roll < c {
        return (pick(false), VictoryType::Close);
    }
    c += pb * (1.0 - massacre);
    if roll < c {
        return (pick(false), VictoryType::Decisive);
    }
    if c + pb * massacre <= roll {
        // Past every bucket: no winner is set by the roll (the resolver keeps its initial 0,
        // side A, INFERRED from the constructor) unless one was forced.
        return (forced.unwrap_or(true), VictoryType::Undecided);
    }
    (pick(false), VictoryType::Crushing)
}

// ---------------------------------------------------------------------------------------------
// Losses.

/// `0x0078FE60(1.0)` (CONFIRMED): `SA/SB > 1 → SB/SA − 1`, else `1 − SA/SB`; 0 if equal or zero.
pub fn advantage(sa: f32, sb: f32) -> f32 {
    if sa == sb || sa == 0.0 || sb == 0.0 {
        return 0.0;
    }
    if 1.0 < sa / sb { sb / sa - 1.0 } else { 1.0 - sa / sb }
}

/// Context of one side's loss stage.
struct LossCtx<'a> {
    is_a: bool,
    won: bool,
    adv: f32,
    victory: VictoryType,
    setup: &'a Setup,
    v: &'a ArVars,
}

fn roll(rng: &mut CaRng) -> f32 {
    rng.next16() as f32 * INV_65535
}

/// `0x007C7430`: moves a fallback rate towards the battle's outcome probabilities.
fn adjust_by_probabilities(c: &LossCtx, rate: f32) -> f32 {
    let p = &c.setup.battle;
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

/// `0x007C71B0`: when the prediction replaced the simulation, moves the rate by the gap between
/// the two sides' simulated casualties, floored at the minimum casualties.
fn adjust_by_casualty_gap(c: &LossCtx, rate: f32) -> f32 {
    let b = &c.setup.battle;
    let d = if c.is_a { b.a.mean - b.b.mean } else { b.b.mean - b.a.mean };
    let mut r = rate;
    if d > 0.35 {
        r -= d.abs() * 0.4;
        if !c.won {
            r -= d.abs() * 0.2;
        }
    } else if d < -0.35 {
        r += d.abs() * 0.4;
        if c.won {
            r += d.abs() * 0.2;
        }
    }
    let floor = if c.won { c.v.min_casualties_win } else { c.v.min_casualties_lose };
    if r < floor { floor } else { r.min(1.0) }
}

/// `0x007C7300`: the advantage adjustment.
fn adjust_by_advantage(c: &LossCtx, rate: f32) -> f32 {
    let adv = c.adv;
    // "Own side is much stronger" in the resolver's sign convention (adv > 0: B stronger).
    let own_much_stronger = |t: f32| if c.is_a { adv < -t } else { adv > t };
    let enemy_much_stronger = |t: f32| if c.is_a { adv > t } else { adv < -t };
    let mut r = rate;
    if !c.won {
        if own_much_stronger(0.45) {
            r *= 1.0 - adv.abs() * 0.2;
        }
    } else if enemy_much_stronger(0.35) {
        r *= (1.0 - adv.abs() * 0.8) + 1.0;
    } else if own_much_stronger(0.45) {
        r *= 1.0 - adv.abs();
    }
    r.clamp(0.0, 1.0)
}

/// `0x0074F170`: a losing unit may be caught by the enemy's shock units: its rate becomes
/// `1 − v + rand × v` with `v = losing_unit_minimum_strength`.
fn pursuit(c: &LossCtx, u: &ArUnit, rate: f32, rng: &mut CaRng) -> f32 {
    let s = c.setup;
    let (enemy_shock, own_total) = if c.is_a { (s.shock_b, s.sa) } else { (s.shock_a, s.sb) };
    if !(0.5 < rate || 0.0 < enemy_shock) {
        return rate;
    }
    let lead = if c.is_a { c.adv.max(0.0) } else { (-c.adv).max(0.0) };
    let ratio = if own_total == 0.0 { 1.0 } else { enemy_shock / own_total };
    let mut chance = (1.0 - lead) * ratio;
    if shock(u.data.category) {
        chance *= 0.5;
    }
    if roll(rng) < chance + lead {
        let v = c.v.losing_unit_minimum_strength;
        return (1.0 - v) + roll(rng) * v;
    }
    rate
}

/// `0x0078FA30`: a unit's loss rate.
fn loss_rate(c: &LossCtx, u: &ArUnit, st: &Stats, modifier: f32, rng: &mut CaRng) -> f32 {
    let s = &st.a; // own point of view
    let draw_ok = st.p_draw > 0.0 && s.draw > 0.0 && s.draw < 1.0;
    let fuzzed = |m: f32, sd: f32, rng: &mut CaRng| (roll(rng) * sd - sd * 0.5) + m;
    let fallback = |rng: &mut CaRng| adjust_by_probabilities(c, fuzzed(s.mean, s.sd, rng));
    let v = c.v;
    let mut r;
    if !c.won {
        r = if 1.0 <= st.p_draw + st.p_a {
            if draw_ok { fuzzed(s.draw, s.sd_draw, rng) } else { fallback(rng) }
        } else if s.other_win <= 0.0 || 1.0 <= s.other_win {
            fallback(rng)
        } else {
            fuzzed(s.other_win, s.sd_other_win, rng)
        };
        if c.setup.prediction_used {
            r = adjust_by_casualty_gap(c, r);
        }
        r = adjust_by_advantage(c, r);
        if r < v.min_casualties_lose {
            r = v.min_casualties_lose;
        }
        match c.victory {
            VictoryType::Crushing => r += (1.0 - r) * v.major_victory,
            VictoryType::Close => r -= v.minor_victory * r,
            _ => {}
        }
        r = pursuit(c, u, r, rng);
    } else {
        r = if st.p_a <= 0.0 {
            if draw_ok { fuzzed(s.draw, s.sd_draw, rng) } else { fallback(rng) }
        } else if s.own_win <= 0.0 || 1.0 <= s.own_win {
            fallback(rng)
        } else {
            fuzzed(s.own_win, s.sd_own_win, rng)
        };
        // 0x00793970: the winner's minimum, doubled unless the own side is clearly stronger.
        let clearly_stronger = if c.is_a { c.adv <= -0.2 } else { c.adv >= 0.2 };
        let min = v.min_casualties_win * modifier * if clearly_stronger { 1.0 } else { 2.0 };
        r = if min <= r { adjust_by_advantage(c, r) } else { min };
        if c.setup.prediction_used {
            r = adjust_by_casualty_gap(c, r);
        }
        match c.victory {
            VictoryType::Crushing => r -= (1.0 - r) * v.major_victory,
            VictoryType::Close => r += v.minor_victory * r,
            _ => {}
        }
    }
    r.clamp(0.0, 1.0)
}

/// `0x0074F840`: applies the rate to the unit; returns the men lost.
fn apply_rate(c: &LossCtx, u: &ArUnit, st: &Stats, modifier: f32, rng: &mut CaRng) -> u32 {
    let mut rate = loss_rate(c, u, st, modifier, rng);
    if !c.won {
        let own_much_stronger = if c.is_a { c.adv < -0.6 } else { c.adv > 0.6 };
        if own_much_stronger && 0.55 <= rate {
            rate = 0.55;
        }
    }
    let f = c.v.losses_fuzziness;
    if f != 0.0 {
        let f = f.clamp(0.0, 0.99);
        rate = (rate + (roll(rng) * f - f * 0.5)).clamp(0.0, 1.0);
    }
    if c.won && u.has_general {
        rate *= 0.35;
    }
    let mut lost = (u.men as f32 * rate).round_ties_even() as u32;
    if c.won && u.men <= lost {
        lost >>= 1;
    }
    let factor = ai_loss_factor(u, c.v);
    if factor != 0.0 {
        lost = (lost as f32 * factor).round_ties_even() as u32;
    }
    lost.min(u.men)
}

/// Sort key of a unit category (`0x00793B00`, CONFIRMED); units are sorted by it, descending
/// and stable (`0x00708630`).
fn sort_key(cat: u8) -> u8 {
    match cat {
        0 => 2,
        1 => 4,
        2 => 0,
        3 => 1,
        4 => 5,
        5 => 3,
        _ => 6,
    }
}

/// `0x0074FF40`: one side's losses. `men` is updated; returns the men lost per unit.
fn side_losses(c: &LossCtx, units: &mut [ArUnit], stats: &[Stats], rng: &mut CaRng) -> Vec<u32> {
    let mut lost = vec![0u32; units.len()];
    let t = c.v.wipeout_threshold;
    let wiped = !c.won && if c.is_a { c.adv > t } else { c.adv < -t };
    if wiped {
        for (u, l) in units.iter_mut().zip(&mut lost) {
            *l = u.men;
            u.men = 0;
        }
        return lost;
    }
    // (Resolver `+0x3D`, a winner without losses, is never set for land battles: INFERRED.)
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sort_key(units[i].data.category)));
    let n = units.len();
    for (k, &i) in order.iter().enumerate() {
        let applies = if c.is_a { c.adv < 0.0 } else { c.adv > 0.0 && c.won };
        let modifier = if applies {
            let pos = 1.0 - (n - k) as f32 / n as f32;
            (pos * (1.0 - 0.8) + 0.8) * (1.0 - c.adv.abs() * 0.5)
        } else {
            1.0
        };
        let l = apply_rate(c, &units[i], &stats[i], modifier, rng);
        units[i].men -= l;
        lost[i] = l;
    }
    // The reshuffle (land battles; resolver `+0x14` clear, INFERRED "not naval"): units of the
    // first half may get half their losses back, the same number of units of the second half
    // lose half their losses again.
    if n > 1 {
        let half = n / 2;
        let mut restored = 0usize;
        let mut rest = Vec::new();
        for (k, &i) in order.iter().enumerate() {
            if k < half {
                let r = roll(rng);
                if r < 0.5 || (restored == 0 && k == half - 1) {
                    let back = lost[i] / 2;
                    units[i].men += back;
                    lost[i] -= back;
                    restored += 1;
                }
            } else {
                rest.push(i);
            }
        }
        let mut idx = 0usize;
        while restored > 0 {
            if rest.is_empty() {
                break;
            }
            if roll(rng) < 0.5 {
                let i = rest[idx];
                let extra = (lost[i] / 2).min(units[i].men);
                units[i].men -= extra;
                lost[i] += extra;
                rest.remove(idx);
                restored -= 1;
                idx = 0;
            }
            idx += 1;
            if idx >= rest.len() {
                idx = 0;
            }
        }
    }
    lost
}

/// Resolves a land battle between `a` (attacker) and `b` (defenders).
pub fn resolve(a: &[ArUnit], b: &[ArUnit], v: &ArVars, rng: &mut CaRng) -> ArOutcome {
    let s = setup(a, b, v);
    let total = s.sa + s.sb;
    if total.is_nan() || total <= 0.0 {
        // No potential on either side (ships, PROVISIONAL: naval autoresolve is not ported): the
        // defenders hold, nobody is hurt.
        return ArOutcome {
            a_won: false,
            victory: VictoryType::Undecided,
            probabilities: [0.0, 1.0, 0.0],
            advantage: 0.0,
            potentials: (s.sa, s.sb),
            losses_a: vec![0; a.len()],
            losses_b: vec![0; b.len()],
        };
    }
    let (a_won, victory) = roll_winner(&s.battle, v, rng);
    let adv = advantage(s.sa, s.sb);
    let mut ua = a.to_vec();
    let mut ub = b.to_vec();
    let ca = LossCtx { is_a: true, won: a_won, adv, victory, setup: &s, v };
    let losses_a = side_losses(&ca, &mut ua, &s.units_a, rng);
    let cb = LossCtx { is_a: false, won: !a_won, adv, victory, setup: &s, v };
    let losses_b = side_losses(&cb, &mut ub, &s.units_b, rng);
    ArOutcome {
        a_won,
        victory,
        probabilities: [s.battle.p_a, s.battle.p_b, s.battle.p_draw],
        advantage: adv,
        potentials: (s.sa, s.sb),
        losses_a,
        losses_b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inf(men: u32) -> ArUnit {
        ArUnit { data: super::super::rules::TEST_AUTORESOLVE, men, has_general: false, general_rank: 0, human: false }
    }

    fn vars() -> ArVars {
        // MADE-UP round values (not game data).
        ArVars {
            massacre_chance: 0.1,
            losses_fuzziness: 0.1,
            gaussian_boundary: 3.0,
            gaussian_sd: 1.0,
            potential_only_weight: 0.3,
            minimum_win_chance: 0.05,
            star_rating_impact: 0.1,
            major_victory: 0.2,
            minor_victory: 0.2,
            min_casualties_win: 0.02,
            min_casualties_lose: 0.1,
            wipeout_threshold: 0.6,
            losing_unit_minimum_strength: 0.5,
            easy_human_advantage: 0.1,
            hard_ai_advantage: 0.08,
            very_hard_ai_advantage: 0.2,
            easy_ai_losses: 0.05,
            hard_ai_losses: 0.2,
            very_hard_ai_losses: 0.35,
            difficulty: 0,
        }
    }

    #[test]
    fn engagement_follows_the_decompiled_loop() {
        // Equal sides, melee only: both lose 0.1 × 0.2 × remaining per step; A breaks first
        // only through the shaken rule, so this is a draw.
        let s = engagement(0.2, 0.0, 0.0, 0.2, 0.0, 0.0, 100, 100, 0);
        assert_eq!(s.result, PairResult::Draw);
        assert!(s.cas_a > 0.9 && s.cas_b > 0.9);
        // A stronger: B breaks.
        let s = engagement(0.4, 0.0, 0.0, 0.1, 0.0, 40.0, 100, 100, 0);
        assert_eq!(s.result, PairResult::AWins);
        assert!(s.cas_b >= 0.6 && s.cas_a < s.cas_b);
        // A out-ranges B (level 1): B loses min(menB, menA × max k) before the loop.
        let s = engagement(0.1, 1.0, 0.0, 0.1, 0.0, 0.0, 100, 100, 1);
        assert_eq!(s.result, PairResult::AWins);
        assert_eq!(s.cas_b, 1.0);
        assert_eq!(s.cas_a, 0.0);
        // A already broken by B's pre-phase.
        let s = engagement(0.1, 0.0, 50.0, 0.1, 0.6, 0.0, 100, 100, -1);
        assert_eq!(s.result, PairResult::BWins);
        assert_eq!(s.cas_a, 0.6);
    }

    #[test]
    fn statistics_layout() {
        let samples = [
            PairSample { result: PairResult::AWins, cas_a: 0.2, cas_b: 0.6 },
            PairSample { result: PairResult::AWins, cas_a: 0.4, cas_b: 0.8 },
            PairSample { result: PairResult::BWins, cas_a: 0.7, cas_b: 0.3 },
            PairSample { result: PairResult::Draw, cas_a: 0.5, cas_b: 0.5 },
        ];
        let s = sample_stats(&samples);
        assert_eq!((s.p_a, s.p_b, s.p_draw), (0.5, 0.25, 0.25));
        assert!((s.a.own_win - 0.3).abs() < 1e-6 && (s.a.sd_own_win - 0.1).abs() < 1e-6);
        assert!((s.b.other_win - 0.7).abs() < 1e-6);
        assert_eq!((s.a.other_win, s.b.own_win, s.a.draw), (0.7, 0.3, 0.5));
        assert!((s.a.mean - 0.45).abs() < 1e-6);
        let f = s.flipped();
        assert_eq!((f.p_a, f.p_b, f.a, f.b), (s.p_b, s.p_a, s.b, s.a));
        // Combining: per-outcome means only over entries with that outcome.
        let only_b = Stats { p_b: 1.0, a: SideStats { other_win: 0.9, mean: 0.9, ..Default::default() }, ..Default::default() };
        let c = combine_stats(&[s, only_b]);
        assert!((c.p_a - 0.25).abs() < 1e-6 && (c.p_b - 0.625).abs() < 1e-6);
        assert!((c.a.own_win - 0.3).abs() < 1e-6 && c.a.sd_own_win == 0.0);
        assert!((c.a.other_win - 0.8).abs() < 1e-6 && (c.a.sd_other_win - 0.1).abs() < 1e-6);
    }

    #[test]
    fn advantage_and_missile_modifier() {
        assert_eq!(advantage(100.0, 100.0), 0.0);
        assert!((advantage(200.0, 100.0) + 0.5).abs() < 1e-6);
        assert!((advantage(100.0, 400.0) - 0.75).abs() < 1e-6);
        assert_eq!(missile_modifier(4, 0, 4, 0), 0.0);
        assert_eq!(missile_modifier(2, 2, 4, 0), -1.0);
        assert!((missile_modifier(4, 3, 4, 0) + 0.75).abs() < 1e-6);
    }

    #[test]
    fn gaussian_matches_the_bit_trick() {
        // The bit-trick root of 4.0 is exactly 2.0; of 2.0 it is 1.5 (not 1.414...).
        let r = |t: f32| f32::from_bits(((((t.to_bits() as i32) - 0x3f80_0000) >> 1) + 0x3f80_0000) as u32);
        assert_eq!(r(4.0), 2.0);
        assert_eq!(r(2.0), 1.5);
        let g = gaussian(12345, 1.0);
        assert!(g.is_finite() && g.abs() < 6.0);
        assert_eq!(g, gaussian(12345, 1.0));
    }

    #[test]
    fn a_much_stronger_army_wins_and_wipes_out_the_other() {
        let a = vec![inf(160); 4];
        let b = vec![inf(40)];
        let mut rng = CaRng::new(7);
        let o = resolve(&a, &b, &vars(), &mut rng);
        assert!(o.a_won);
        assert!(o.advantage < -0.6);
        assert_eq!(o.losses_b, vec![40]);
        assert!(o.losses_a.iter().sum::<u32>() < 160);
    }

    #[test]
    fn even_battles_are_decided_by_the_roll_and_are_deterministic() {
        let a = vec![inf(100), inf(100)];
        let b = vec![inf(100), inf(100)];
        let mut wins = 0;
        for seed in 0..40 {
            let mut rng = CaRng::new(seed);
            let o = resolve(&a, &b, &vars(), &mut rng);
            let mut rng2 = CaRng::new(seed);
            assert_eq!(o, resolve(&a, &b, &vars(), &mut rng2));
            assert!((o.probabilities.iter().sum::<f32>() - 1.0).abs() < 1e-5);
            wins += o.a_won as u32;
            assert!(o.losses_a.iter().chain(&o.losses_b).all(|&l| l <= 100));
        }
        assert!(wins > 5 && wins < 35, "{wins}");
    }
}

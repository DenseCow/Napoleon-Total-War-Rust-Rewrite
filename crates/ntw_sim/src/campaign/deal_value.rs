//! The AI recipient's value of a proposed deal (`analysis/ai/AI_RESEARCH.md` §4 "Deal
//! evaluation"): the parts of the original's evaluator that are traced. The evaluator itself
//! (`EvaluateProposedNegotiationDealAI 0x00AA5ED0`, run by `CCQ_DIPLOMACY_PROPOSE_DEAL` → `ProcessNegotiationDealProposal 0x00C49BE0` when the recipient is not
//! human), for the records the model holds. Ported here: the technology value, the deal inflation,
//! the evaluation sum and its accept tests, the goal weights of region and technology items (the
//! research need, the technology spread) and how a record sums them, the region value
//! (`0x00C131C0` → `0x00A364B0` → `0x00AA1E90`, then `0x00C4D140`), the join-war goal weights and
//! value (`0x00CC13D0` / `0x00CC0280`, `0x00C12D40`, the war strength `0x00AB5400`), the peace goal
//! weight and value (`0x00CC13D0` case 9 and the peace attitude `0x00D63260`, `0x00C130E0` →
//! `0x00A351E0`), and the
//! answer to a deal of regions, technologies, payments, peace and join-war items
//! ([`CampaignModel::ai_accepts_deal`]).
//!
//! A deal item's value is a triple of integers (`0x00518210`): `gain`, what the recipient gets
//! (slot 0), and `cost` / `given` (slots 1 and 2), two measures of what it gives.

use super::ids::{FactionId, RegionId};
use super::negotiation::{DealItems, Negotiation};
use super::research::state;
use super::world::CampaignModel;

/// One item's (or a whole deal's) value to the AI recipient (`0x00518210`'s three slots).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DealValue {
    /// Slot 0: what the recipient gains.
    pub gain: u32,
    /// Slot 1: what it gives (first measure).
    pub cost: u32,
    /// Slot 2: what it gives (second measure; the accept tests read both).
    pub given: u32,
}

impl std::ops::Add for DealValue {
    type Output = DealValue;

    /// Slot-wise sum (`0x00C0DF90`).
    fn add(self, o: DealValue) -> DealValue {
        DealValue { gain: self.gain.wrapping_add(o.gain), cost: self.cost.wrapping_add(o.cost), given: self.given.wrapping_add(o.given) }
    }
}

impl DealValue {
    /// Every slot times the world's deal inflation factor (`0x00C42FB0`). The factor is a float
    /// (1..3, [`deal_inflation`]) but is truncated to an integer before it multiplies (CVTTSS2SI at
    /// `0x00C42FB2`), so 1.0..2.99 count as 1 or 2. CONFIRMED (disassembly); kept 1:1.
    pub fn inflated(self, factor: f32) -> DealValue {
        let k = super::commands::cvttss2si(factor) as u32;
        DealValue { gain: self.gain.wrapping_mul(k), cost: self.cost.wrapping_mul(k), given: self.given.wrapping_mul(k) }
    }
}

/// `0x00C4D140`, the last step of the regions record's value (`0x00C131C0`): slot 2 becomes
/// `trunc(f32(slot 2) × 1.5^(max(m, 1) − 1))` (`powf`, constant `0x0131A7B8` = 1.5; slot 2 read
/// as unsigned), slots 0 and 1 unchanged. `m` is 0 unless the proposer is human and the deal has
/// no peace item; then it is the deal's demanded regions plus the regions the proposer has received
/// in earlier deals ([`super::World::deal_regions_received`]), so a human's demands grow dearer.
/// CONFIRMED (disassembly). The step runs for every deal, so slot 2 always passes through an f32.
pub fn human_demand_scaled(v: DealValue, m: u32) -> DealValue {
    let factor = 1.5f32.powf((m.max(1) - 1) as f32);
    DealValue { given: super::commands::cvttss2si(v.given as f32 * factor) as u32, ..v }
}

/// `0x00A36B20`: one technology of a deal. The base is `500 + trunc(10 × cost^1.1)` (`cost` = the
/// research points the technology needs, record `+0x1C`; computed as `500 − trunc(cost^1.1 ×
/// −10)` with `powf`), doubled when exactly one faction has researched it (`holders == 1`), then
/// divided (unsigned) by `(traded + 1)²`, `traded` being the proposer's traded count of it
/// (technology entry `+0x28`), unless the technology is demanded from the AI by a human proposer.
/// An offered technology (the proposer gives it) is all gain; a demanded one is all cost, in both
/// cost slots. CONFIRMED (disassembly `0x00A36B20`; constants `0x01325CF8` = 1.1,
/// `0x0133BCB4` = −10.0; `0x01285310` = `powf`).
pub fn technology_value(cost: i32, holders: usize, traded: u32, offered: bool, proposer_human: bool) -> DealValue {
    let base = 500u32.wrapping_sub(super::commands::cvttss2si((cost as f32).powf(1.1) * -10.0) as u32);
    let base = if holders == 1 { base.wrapping_mul(2) } else { base };
    let divisor = if offered || !proposer_human { traded.wrapping_add(1).wrapping_mul(traded.wrapping_add(1)).max(1) } else { 1 };
    let v = base / divisor;
    if offered { DealValue { gain: v, cost: 0, given: 0 } } else { DealValue { gain: 0, cost: v, given: v } }
}

/// The world's deal inflation factor `clamp(net / max(1, first), 1, 3)` in floats (`0x008A9920`),
/// where `net` is the sum over the factions of last turn's income minus expenses
/// ([`CampaignModel::world_net_income`]) and `first` the `net` of the first round end that ran.
/// Both are converted as unsigned (the `0x01318130` fix-up table) before the division, so a
/// negative net counts as about 4·10⁹. CONFIRMED (disassembly). Kept 1:1: the net is produced by a
/// signed subtraction (`0x0096D2E0`), but every reader of it and of `first` (this function, the
/// loader `0x00873B60`, the writer `0x008EBAB0` with ESF type u32) treats it as unsigned, so
/// nothing in the exe shows the unsigned reading to be unintended (AI_RESEARCH.md §4 "Inflation in
/// the save").
pub fn deal_inflation(net: i32, first: u32) -> f32 {
    let r = (net as u32) as f32 / first.max(1) as f32;
    // 1 below 1 (or NaN), 3 above 3: the exe's compare order.
    if (1.0..=3.0).contains(&r) { r } else if r > 3.0 { 3.0 } else { 1.0 }
}

/// The campaign's deal inflation state (`CAMPAIGN_MODEL` #21 / #22 = campaign `+0x1010` /
/// `+0x1014`; loader `0x00873B60`, writer `0x008EBAB0`; CONFIRMED). The factor multiplies deal
/// values ([`DealValue::inflated`]).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DealInflation {
    /// `+0x1010`: the net of the first round end that ran (0 until then; set once while 0).
    pub first: u32,
    /// `+0x1014`: the factor, 1.0 until the first round end (the constructors `0x00872550` /
    /// `0x008742C0` write 1.0).
    pub factor: f32,
}

impl Default for DealInflation {
    fn default() -> Self {
        DealInflation { first: 0, factor: 1.0 }
    }
}

impl DealInflation {
    /// The round-end update (`0x008A9920`, after the calendar moves on): `first` takes `net` while
    /// it is 0, then the factor is [`deal_inflation`]. CONFIRMED.
    pub fn round_end(&mut self, net: i32) {
        if self.first == 0 {
            self.first = net as u32;
        }
        self.factor = deal_inflation(net, self.first);
    }
}

/// `0x00D018B0`: the goal-type weight of an action record's index (the negotiation's record list,
/// `0x00BF5A60`: 0 trade, 1 access, 2 access cancel, 3 alliance, 4 regions, 5 technology, 6 state
/// gift, 7 payments, 8 protector, 9 peace, 10 war, 11 join war, 12 break trade, 13 break alliance).
/// CONFIRMED.
pub fn goal_type_weight(record: usize) -> f32 {
    match record {
        0 | 2 | 5 | 12 | 13 => 500.0,
        1 => 250.0,
        3 | 8 | 11 => 1000.0,
        4 | 9 => 2500.0,
        _ => 1.0,
    }
}

/// The evaluation the AI builds from the deal's records (`0x00AA5ED0`'s local at `+0x70`): the
/// value triple summed over the items, a bonus from the items' goal weights, an eagerness and an
/// item count.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DealEvaluation {
    /// `+0x00`: the items' values summed.
    pub value: DealValue,
    /// `+0x0C`: Σ goal weight × [`goal_type_weight`].
    pub bonus: f32,
    /// `+0x10`: Σ of each item's slot 0 (weight ≥ 0) or slot 2 (weight < 0) × its scaled weight.
    pub eagerness: f32,
    /// `+0x14`: the items added.
    pub items: i32,
}

impl DealEvaluation {
    /// `0x00CB21C0`: one item (a goal of record index `record`, value `v`, goal weight `weight`)
    /// with scale `p` (the evaluator passes 1.0). CONFIRMED.
    pub fn add(&mut self, record: usize, v: DealValue, weight: f32, p: f32) {
        self.value = self.value + v;
        self.bonus += weight * goal_type_weight(record);
        let e = if weight < 0.0 { v.given as f32 * ((weight + 1.0) - p) } else { v.gain as f32 * ((weight + p) - 1.0) };
        self.items += 1;
        self.eagerness += e * p;
    }

    /// `0x00CE5750`: neither cost slot exceeds 1.05 × (bonus + gain). CONFIRMED (float maths,
    /// constant `0x0137C258`).
    pub fn fair(&self) -> bool {
        let limit = (self.bonus + self.value.gain as f32) * 1.05;
        self.value.given as f32 <= limit && self.value.cost as f32 <= limit
    }

    /// `0x00CEFA40`: twice each cost slot is within bonus + gain (a clearly good deal). CONFIRMED.
    pub fn good(&self) -> bool {
        let limit = self.bonus + self.value.gain as f32;
        self.value.given.wrapping_mul(2) as f32 <= limit && self.value.cost.wrapping_mul(2) as f32 <= limit
    }
}

/// `0x00D63790` (the AI attitude object's virtual `+0x2B4`): the state of a peace attitude `v`
/// ([`CampaignModel::peace_attitude`]): 0 at most −0.75, else 1 at most −0.25, else 4 at least
/// 0.75, else 3 at least 0.25, else 2. CONFIRMED (decompile).
pub fn peace_attitude_state(v: f32) -> u8 {
    if v <= -0.75 {
        0
    } else if v <= -0.25 {
        1
    } else if v >= 0.75 {
        4
    } else if v >= 0.25 {
        3
    } else {
        2
    }
}

/// The peace value's base pair by the AI's peace attitude state ([`peace_attitude_state`];
/// `0x00A351E0`): what the AI asks for peace (first) and the scale of what it gives (second).
/// States 0 and 1 (2500, 6000), 2 (5000, 3000), 3 and 4 (10000, 1500). CONFIRMED (disassembly).
pub fn peace_base_pair(state: u8) -> (u32, u32) {
    match state {
        0 | 1 => (2500, 6000),
        2 => (5000, 3000),
        _ => (10000, 1500),
    }
}

/// `0x00A75B60`: a human faction's strength factor in the peace value by its difficulty (faction
/// `+0x6E4`): 0 → 1.2, 1 → 1.3, −1 → 1.1, any other 1.0 (an AI's strength is not scaled).
/// CONFIRMED (decompile).
pub fn human_strength_factor(difficulty: i32) -> f32 {
    match difficulty {
        0 => 1.2,
        1 => 1.3,
        -1 => 1.1,
        _ => 1.0,
    }
}

/// `0x00A75BC0`: a human faction's rank boost in the peace value by its difficulty: 0 → 0.67,
/// 1 → 1.0, −1 → 0.33, any other 0 (an AI gets none). CONFIRMED (decompile).
pub fn human_rank_boost(difficulty: i32) -> f32 {
    match difficulty {
        0 => 0.67,
        1 => 1.0,
        -1 => 0.33,
        _ => 0.0,
    }
}

/// The peace value's clamp of a strength ratio to [0.333 (`0x3EAA7EFA`), 3.0] (`0x00A351E0`).
/// CONFIRMED (constants).
fn peace_ratio_clamped(x: f32) -> f32 {
    let lo = f32::from_bits(0x3EAA_7EFA);
    if x < lo {
        lo
    } else if x > 3.0 {
        3.0
    } else {
        x
    }
}

/// The peace value's rank term (`0x00A351E0`): log10 of the clamped ratio
/// ([`peace_ratio_clamped`]) of the two sides' power ranks, `ai` and `p` = 5 − power category
/// ([`super::negotiation::Rankings::power`]) and `boost` the proposer's [`human_rank_boost`]:
/// with `b` = `p` + `boost`, (`b` − `ai`) × 0.4 + 1 when `ai` < `p`, else 1 / ((`ai` − `b`) × 0.4
/// + 1). CONFIRMED (disassembly).
pub fn peace_rank_log(ai: i32, p: i32, boost: f32) -> f32 {
    let b = p as f32 + boost;
    let r = if ai < p { (b - ai as f32) * 0.4 + 1.0 } else { 1.0 / ((ai as f32 - b) * 0.4 + 1.0) };
    peace_ratio_clamped(r).log10()
}

/// The peace value's weighted mean of one side's region log ratios (`0x00A351E0`): sorted largest
/// first (`0x00A2BB10`, an introsort with a greater-than comparator), the `i`-th (from 0) of `n`
/// weighs `n − i + 2`; each weighted term is divided by the integer total weight `n(n+1)/2 + 2n`
/// before it is added (the loops at `0x00A35712` and `0x00A35A40`: MULSS, DIVSS, then ADDSS), in
/// f32 from 0; 0 with no region. CONFIRMED (disassembly).
pub fn peace_weighted_mean(mut v: Vec<f32>) -> f32 {
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    v.sort_by(|a, b| b.total_cmp(a));
    let total = (n * (n + 1) / 2 + 2 * n) as f32;
    v.iter().enumerate().fold(0.0f32, |s, (i, x)| s + (n - i + 2) as f32 * x / total)
}

/// The peace value's war-length factor of `t` turns at war (`0x00A351E0`): (t + 1) × 2/3 below 5
/// turns, else 4 × log2(t / 5) (`logf` `0x012710FD` over ln 2, `0x014589CC`). CONFIRMED
/// (disassembly, `0x00A35C67`–`0x00A35CA7`: COMISS t against 5.0 `0x01325D24`, JC to
/// (t + 1.0) × 0.6666667 `0x01331B2C`, else t × 0.2 `0x01325CA8`, `logf`, ÷ ln 2, × 4.0
/// `0x01325D20`). The two pieces do not meet: the factor drops from 3.33 at 4 turns to 0 at 5 and
/// is back above 3.33 only at 9. Faithful, not an ORIGINAL BUG (CONFIRMED, peacefactor
/// 2026-10-22): `t` is the relation's turns at war (`+0x7E4`, read by `0x00B77DC0`, +1 at each
/// round end `0x00B29649`), read only here and by the attitude `0x00D63260`; no table, no wrong
/// compare or clamp joins the pieces (`t` ≤ 5 would still drop 4 to 1.05), and 5 turns starts the
/// same function's long-war terms, which also start from 0 there (the turns since a battle count
/// only from 5 turns, the extra max(tsb + t − 5, 0) × k), so the shape reads as designed.
pub fn peace_turn_factor(turns: u32) -> f32 {
    let t = turns as f32;
    if t < 5.0 { (t + 1.0) * (2.0 / 3.0) } else { 4.0 * ((t * 0.2).ln() / f32::from_bits(0x3F31_7218)) }
}

/// The peace value's momentum factor `m` (`0x00A351E0`) of the AI's war momentum towards the
/// proposer: 1 + momentum × 0.0057692304 (`0x3BBD0BD0`, a human proposer) or × 0.009374999
/// (`0x3C199999`, an AI), at least −0.75, when it is negative; else 1 + momentum × 0.075, at most
/// 0.75. CONFIRMED (disassembly).
pub fn peace_momentum_factor(momentum: i32, proposer_human: bool) -> f32 {
    let m = momentum as f32;
    let x = if momentum < 0 {
        let scale = f32::from_bits(if proposer_human { 0x3BBD_0BD0 } else { 0x3C19_9999 });
        (m * scale).max(-0.75)
    } else {
        (m * 0.075).min(0.75)
    };
    1.0 + x
}

/// What the peace value reads ([`peace_value`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeaceValueInputs {
    /// [`peace_base_pair`] of the AI's peace attitude state.
    pub base: (u32, u32),
    /// [`peace_rank_log`].
    pub rank_log: f32,
    /// [`peace_weighted_mean`] over the AI's regions (its enemies' strength over its own side's).
    pub own_mean: f32,
    /// [`peace_weighted_mean`] over the proposer's regions (the AI's strength over the proposer's side).
    pub their_mean: f32,
    /// The AI's record towards the proposer: turns at war, war momentum, turns since a battle.
    pub war_turns: u32,
    /// See `war_turns`.
    pub momentum: i32,
    /// See `war_turns`.
    pub turns_since_battle: u32,
    /// The proposer is human, and its difficulty (faction `+0x6E4`).
    pub proposer_human: bool,
    /// See `proposer_human`.
    pub proposer_difficulty: i32,
}

/// `0x00A351E0` after its loops: the peace record's value from its inputs (before the deal
/// inflation). CONFIRMED (disassembly):
/// - `A` = trunc(base.0 × clamp(10^(own_mean × 2/3 + rank_log / 3))), `c` = clamp(10^((2 ×
///   their_mean − rank_log) / 3)), `B` = trunc(c × 10000), `X` = trunc(base.1 × c) (truncating
///   float-to-unsigned `0x0126E2B0`; clamp [`peace_ratio_clamped`]);
/// - with `m` ([`peace_momentum_factor`]) and `tf` ([`peace_turn_factor`]): `A'` = (2 − m) × A,
///   `B'` = B × m (CVTTSS2SI);
/// - `v` = `A'`; when `A'` < `B'` (unsigned) `v` = `A'` + round(max(tsb + turns − off, 0) × k),
///   at most `B'`, with `tsb` the turns since a battle (0 below 5 turns at war) and (k, off) by a
///   human proposer's difficulty (jump table `0x00A35F00`): −2 (10, 5), −1 (30, 5), 1 (60, 0),
///   else (and an AI) (50, 5); the round is FISTP's, to even;
/// - the value (trunc(v × tf), trunc(B' × tf), trunc(max(tf × 0.85, 1) × X × m)).
pub fn peace_value(i: &PeaceValueInputs) -> DealValue {
    use super::commands::cvttss2si;
    // `0x0126E2B0`: truncate through a 64-bit integer, keep the low 32 bits.
    let ftoui = |v: f32| v as i64 as u32;
    let pow1 = peace_ratio_clamped(10f32.powf(i.own_mean * (2.0 / 3.0) + i.rank_log * (1.0 / 3.0)));
    let a = ftoui(i.base.0 as f32 * pow1);
    let c = peace_ratio_clamped(10f32.powf((2.0 * i.their_mean - i.rank_log) * (1.0 / 3.0)));
    let (b, x) = (ftoui(c * 10000.0), ftoui(i.base.1 as f32 * c));
    let m = peace_momentum_factor(i.momentum, i.proposer_human);
    let tf = peace_turn_factor(i.war_turns);
    let a2 = cvttss2si((2.0 - m) * a as f32) as u32;
    let b2 = cvttss2si(b as f32 * m) as u32;
    let (k, off) = match (i.proposer_human, i.proposer_difficulty) {
        (true, -2) => (10.0f32, 5.0f32),
        (true, -1) => (30.0, 5.0),
        (true, 1) => (60.0, 0.0),
        _ => (50.0, 5.0),
    };
    let tsb = if i.war_turns < 5 { 0 } else { i.turns_since_battle };
    let mut v = a2;
    if a2 < b2 {
        let extra = ((tsb as f32 + i.war_turns as f32 - off).max(0.0) * k).round_ties_even() as i32;
        v = a2.wrapping_add(extra as u32).min(b2);
    }
    DealValue {
        gain: cvttss2si(v as f32 * tf) as u32,
        cost: cvttss2si(b2 as f32 * tf) as u32,
        given: cvttss2si((tf * 0.85).max(1.0) * x as f32 * m) as u32,
    }
}

/// The AI's strength of each faction in each region, as its region beliefs hold it (`0x00D7B9E0`,
/// the map entry's `+8` of the region belief `0x00D96360` builds: the land strength of each
/// faction's mobiles with units in the region, plus the garrison's for the owner), taken once per
/// peace evaluation.
///
/// PROVISIONAL: a mobile's strength is its mobile belief's `+0xFC` (`0x00D96DE0`: 2000 × its raw
/// strength (`+0x104` of another belief) over a per-faction land norm (`0x00D8E600` slot 0xC, or
/// a unit-record fallback), not traced further); ours is the force's value
/// ([`CampaignModel::force_power`]), the commander's region its area ([`super::economy::in_region`]),
/// and the garrison's strength (`0x00C3C5F0` → `0x0080DC60`, not traced) the garrison force's
/// value. Every use is a ratio of two of them, so a common scale cancels.
struct RegionStrengths(std::collections::BTreeMap<(RegionId, FactionId), i32>);

impl RegionStrengths {
    fn of(m: &CampaignModel) -> Self {
        let w = &m.world;
        // Unknown unit keys count 0 here; the rankings report them (`FactionRankings::unknown_units`).
        let mut unknown = std::collections::BTreeSet::new();
        let mut map = std::collections::BTreeMap::new();
        let garrisons: std::collections::BTreeSet<_> = w.regions.values().filter_map(|r| r.garrison).collect();
        for (id, f) in &w.forces {
            if f.is_navy || f.units.is_empty() || garrisons.contains(id) {
                continue;
            }
            let Some(c) = f.commander.and_then(|c| w.characters.get(&c)) else { continue };
            let Some(r) = super::economy::region_of(m, c) else { continue };
            let e: &mut i32 = map.entry((r.id, f.faction)).or_default();
            *e = e.wrapping_add(m.force_power(f, &mut unknown));
        }
        for r in w.regions.values() {
            if let Some(g) = r.garrison.and_then(|g| w.forces.get(&g)) {
                let e: &mut i32 = map.entry((r.id, r.owner)).or_default();
                *e = e.wrapping_add(m.force_power(g, &mut unknown));
            }
        }
        RegionStrengths(map)
    }

    /// `faction`'s strength in `region` (0 with none).
    fn get(&self, region: RegionId, faction: FactionId) -> i32 {
        self.0.get(&(region, faction)).copied().unwrap_or(0)
    }
}

/// A faction's region counts, taken once per deal evaluation ([`CampaignModel::region_deal_worth`]).
struct RegionCensus {
    /// Regions it owns.
    held: usize,
    /// Of those, the ones with building slots.
    with_slots: usize,
}

/// The weight of a region goal in either of the AI's goal lists (`0x00CCB810` and `0x00CCB210`
/// both write −1.0, constant `0x01318068`): each region of a deal that is in the giver's goal list
/// counts −1 (× [`goal_type_weight`] 2500 in the bonus). CONFIRMED.
pub const REGION_GOAL_WEIGHT: f32 = -1.0;

/// `0x00D63120`: how widespread a technology is, from −1 (nobody has it) to 1. `holders` = the
/// CAI world's factions that have it researched, `entries` = all the CAI world's factions; with
/// `t = max(1, entries / 8)` it is `2 × min(holders, t) / t − 1` in floats. CONFIRMED.
pub fn technology_spread(holders: usize, entries: usize) -> f32 {
    let t = ((entries as u32) >> 3).max(1);
    let n = (holders as u32).min(t);
    let r = n as f32 / t as f32;
    (r + r) - 1.0
}

/// The weight of a technology the AI would **give** (its list-2 goal, `0x00CCB210`): the spread
/// ([`technology_spread`]) below −1 is −1, else at most −0.5. CONFIRMED.
pub fn given_technology_weight(spread: f32) -> f32 {
    if spread < -1.0 { -1.0 } else { spread.min(-0.5) }
}

/// The weight of a technology the AI would **receive** (its list-1 goal, `0x00CCB810`): its
/// research need ([`research_need_values`] for the technology's category) below −1 is −1, else at
/// most 0.5 (constant `0x01318038`). CONFIRMED.
pub fn received_technology_weight(need: f32) -> f32 {
    if need < -1.0 { -1.0 } else { need.min(0.5) }
}

/// One research need score (`0x00ABE100`, `0x00A64460`, `0x00A8D6E0`): `2^v` truncated, 256 when
/// `v` > 8 (`powf(2, v)`, constants `0x01325D14` = 2, `0x01325D88` = 256). CONFIRMED.
pub fn need_power(v: u32) -> u32 {
    if v > 8 { 256 } else { 2f32.powf(v as f32) as i32 as u32 }
}

/// `0x00ABE100`: the military need from the number of factions at war with the faction (the
/// relation belief's list `+0x40`, stance 0): `2^min(enemies, 8)`. CONFIRMED.
pub fn need_from_enemies(enemies: u32) -> u32 {
    need_power(enemies.min(8))
}

/// `0x00A98460`: the need from allies (list `+0x2C`, stance 2) minus enemies: 1 when negative, 256
/// above 8, else `2^d`. CONFIRMED.
pub fn need_from_allies(allies: u32, enemies: u32) -> u32 {
    let d = allies as i32 - enemies as i32;
    if d < 0 { 1 } else { need_power(d as u32) }
}

/// `0x00ABB340`: the research need belief's value per technology category (`0x00C5B950`: 0
/// "admin", 1 "economy", 2 "military"; `0x004CDC50` = 3 categories) from its five need scores:
/// `military` (`0x00ABE100`), `allies` (`0x00A98460`), `economy` (`0x00A9B810`), `p1`
/// (`0x00A64460`) and `p2` (`0x00A8D6E0`). `need[0] = need[1] = allies + economy`,
/// `need[2] = military + max(p1, p2)`, `h = (Σ need) >> 1`, value = `min(need, h) / h` (signed
/// compare, `1/h` first, as the exe). CONFIRMED. The inputs: [`CampaignModel::research_need`].
pub fn research_need_values(military: u32, allies: u32, economy: u32, p1: u32, p2: u32) -> [f32; 3] {
    let ae = allies.wrapping_add(economy);
    let need = [ae, ae, military.wrapping_add(p1.max(p2))];
    let h = need[1].wrapping_add(need[2]).wrapping_add(need[0]) >> 1;
    let inv = 1.0 / (h as i32 as f32);
    need.map(|x| (x as i32).min(h as i32) as f32 * inv)
}

/// The goal weight a record of the deal adds (`0x00A62400`, records 4 and 5): the weights of the
/// AI's list-1 goals whose item is one the proposer **offers**, plus those of its list-2 goals
/// whose item is one the proposer **demands** (a goal holds one item; a list holds an item once).
/// `received(item)` / `given(item)` give the item's goal weight when it is in list 1 / list 2.
/// CONFIRMED (disassembly `0x00A62766..0x00A62903`).
pub fn record_goal_weight<T>(items: &DealItems<T>, received: impl Fn(&T) -> Option<f32>, given: impl Fn(&T) -> Option<f32>) -> f32 {
    let offered = items.offered.iter().filter_map(received);
    let demanded = items.demanded.iter().filter_map(given);
    offered.chain(demanded).fold(0.0f32, |w, x| w + x)
}

/// The weight of a join-war goal in the AI's list 1 (a war of the AI's that it asks the proposer to
/// join, `0x00CCB810` type 11: −1.0, constant `0x01318068`). CONFIRMED.
pub const ASKED_JOIN_WAR_WEIGHT: f32 = -1.0;

/// The weight of a join-war goal in the AI's list 2 (a war of the proposer's the AI would join,
/// `0x00CCB210` case 11) from its attitude `w` ([`CampaignModel::join_war_attitude`]) and `cap`
/// (what the proposer is to the AI: −0.1 its patron, [`CampaignModel::believed_patron`], −0.25 an
/// ally, [`CampaignModel::believes_allied`], else −0.5): below −1 it is −1, else at most `cap`
/// (disassembly `0x00CCB5C1..0x00CCB62E`). CONFIRMED.
pub fn given_join_war_weight(w: f32, cap: f32) -> f32 {
    if w < -1.0 { -1.0 } else { w.min(cap) }
}

/// The join-war attitude's offset for the AI's stance-belief state towards the proposer
/// (`0x00D62DA0`, states 0..4 of `0x00ABCB80`): 0 → +0.5, 1 → +0.25, 3 → −0.25, 4 → −0.5, any
/// other 0 (constants `0x01318038` = 0.5, `0x01325CAC` = 0.25, `0x01331C38`, `0x01331C40`). The
/// target's offset is its negation. CONFIRMED.
pub fn join_war_state_offset(state: u8) -> f32 {
    match state {
        0 => 0.5,
        1 => 0.25,
        3 => -0.25,
        4 => -0.5,
        _ => 0.0,
    }
}

/// The join-war value's attitude factor at attitude +100 (`0x00A8D250`'s 4th argument, pushed as
/// `0x3F000000` at `0x00A34EED` and `0x00A34F20` in `0x00A34E80`). CONFIRMED.
pub const JOIN_WAR_FACTOR_AT_BEST: f32 = 0.5;
/// The join-war value's attitude factor at attitude −100 (`0x00A8D250`'s 5th argument, pushed as
/// `0x40000000` at `0x00A34EE5` and `0x00A34F18`). CONFIRMED.
pub const JOIN_WAR_FACTOR_AT_WORST: f32 = 2.0;

/// `0x00A8D250(value, a, b, at_best, at_worst)`: `value` scaled by a factor from `a`'s attitude
/// total `attitude` towards `b` (clamped to ±100), linear from 1 at 0 to `at_best` at +100
/// (`(at_best − 1) × att × 0.01 + 1`) and to `at_worst` at −100 (`(1 − at_worst) × (att + 100) ×
/// 0.01 + at_worst` below 0; constants `0x01318048` = 1.0, `0x01318034` = 0.01). CONFIRMED
/// (disassembly).
///
/// ORIGINAL BUG: the exe truncates the factor to an integer before it multiplies (`CVTTSS2SI`,
/// then `IMUL` with `value`, `0x00A8D2AF` / `0x00A8D2E2`), so every factor in [0, 1) gives 0 and
/// every one in [1, 2) gives 1: with the endpoints all its callers pass (0.5 / 2.0, in either
/// order), any attitude above 0 zeroes the value (or every one below 0, with the endpoints swapped)
/// and only ±100 reaches the other endpoint, where the sibling `0x00A34CD0`'s trade branch
/// multiplies first and keeps the fraction (`MULSS`, then `CVTTSS2SI`, `0x00A34DCD`). Ours
/// multiplies first: `trunc(k × value)`, wrapping as the exe's `IMUL` does.
pub fn attitude_scaled(value: u32, attitude: i32, at_best: f32, at_worst: f32) -> u32 {
    let att = attitude.clamp(-100, 100) as f32;
    let k = if att < 0.0 { (1.0 - at_worst) * (att + 100.0) * 0.01 + at_worst } else { (at_best - 1.0) * att * 0.01 + 1.0 };
    // k ≥ 0 for endpoints ≥ 0; the product fits a u64 (k ≤ 2^32 for any sane endpoint), and the
    // u32 cast wraps as the exe's 32-bit IMUL does for an integral factor.
    (f64::from(k) * f64::from(value)).trunc() as u64 as u32
}

/// `0x00A9B810`: the economic need from the faction's economics history (oldest first). With
/// `count` = min(records, 5): 16 when `count` ≤ 2; else over the newest `count − 2` records
/// `I` = Σ categories 0..12 (`0x00BC7860`) and `X` = Σ categories 13..24 (`0x00BC7840`),
/// `s = trunc(f32(2I) × 0.125)`, and `k` starts at 8 and drops by one for each step of `s` until
/// the steps reach `2I − X` (at most 8 steps); returns `2^k`. Integer sums wrap. CONFIRMED
/// (disassembly, constant `0x01325C9C` = 0.125).
pub fn need_from_economy(history: &[super::world::EconomyRecord]) -> u32 {
    let count = history.len().min(5);
    if count <= 2 {
        return 16;
    }
    let (mut income, mut expenses) = (0i32, 0i32);
    for r in history.iter().rev().take(count - 2) {
        income = income.wrapping_add(super::world::economy_sum(r, 0..13));
        expenses = expenses.wrapping_add(super::world::economy_sum(r, 13..25));
    }
    let twice = income.wrapping_add(income);
    let step = (twice as f32 * 0.125) as i32;
    let target = income.wrapping_sub(expenses).wrapping_add(income);
    let (mut k, mut reached) = (8i32, 0i32);
    while target > reached {
        k -= 1;
        reached = reached.wrapping_add(step);
        if k <= 0 {
            break;
        }
    }
    need_power(k.max(0) as u32)
}

/// `0x00C5B950`: a technology's research-need category from its key's prefix: "admin" 0,
/// "economy" 1, "military" 2 (CONFIRMED; every shipped key has one of them, e.g.
/// `admin1_public_schooling`, `economy2_joint_stock_company`, `military5_rockets`). Any other key
/// is `None`: ORIGINAL BUG: the exe returns 3 and the need belief (`0x00D63230` → `0x00A75CC0`)
/// reads `value[3]` past its 3-entry array (allocated with `0x004CDC50` = 3, `0x00A3AFE0`);
/// ours gives such a technology no need (weight 0).
pub fn technology_category(key: &str) -> Option<usize> {
    ["admin", "economy", "military"].iter().position(|p| key.starts_with(p))
}

/// One force of an attacker as [`CampaignModel::war_strength`] reads it (a CAI mobile with units).
struct WarMobile {
    /// Its value ([`CampaignModel::force_power`]).
    value: u32,
    /// A navy (halved, never near).
    navy: bool,
    /// A land force's area: the region its commander stands in, if any.
    area: Option<RegionId>,
}

impl CampaignModel {
    /// `SumFactionsLastTurnNetIncome` (`0x0096D2E0` on the world, `this` = campaign `+0xF5C`): over
    /// the world's faction list (`+0x2C` / `+0x30` = `FACTION_ARRAY`, dead factions included; the
    /// rebel faction is `+0x1C`, loaded from `REBEL_FACTION` at `0x0090C1E6`, and not in it), the
    /// wrapping sum of each faction's last income (categories 5..11) minus its last expenses
    /// (18..24). CONFIRMED.
    pub fn world_net_income(&self) -> i32 {
        self.world
            .factions
            .keys()
            .filter(|&&f| !self.is_rebel_faction(f))
            .fold(0i32, |s, &f| s.wrapping_sub(self.world.last_expenses(f)).wrapping_add(self.world.last_income(f)))
    }

    /// The research need belief of `faction` (`0x00ABB340`, [`research_need_values`]) from the
    /// model's state. The belief refreshes when it is read while dirty (`0x00A74AD0` →
    /// `0x00CCCF70`), so it is computed here when needed. Inputs:
    /// - enemies / allies: the lengths of `faction`'s CAI relation belief's enemy and ally lists
    ///   (`0x00ABE100` and `0x00A98460` read `0x00A64990` → `+0x40` / `0x00A63AB0` → `+0x2C`, count
    ///   `+0xC`; [`super::director::believed_enemy_count`], [`super::director::believed_ally_count`]),
    ///   so the rebels count as an enemy; CONFIRMED;
    /// - economy: [`need_from_economy`] of its history; CONFIRMED;
    /// - `p1` (`0x00A64460`): `(forts in its regions >> 1) + (its land forces with units >> 2)`
    ///   (CAI region `+0x15C` = the count of `CAI_REGION` #6, the forts `0x00C140C0` adds to the
    ///   region at the fort's position; a mobile's `+0x168` = land (`0x00C15370` ← force virtual
    ///   `+0x38`), `+0x160` = its unit count; CONFIRMED);
    /// - `p2` (`0x00A8D6E0`): `(port slots in its regions >> 1) + (its navies with units >> 2)`
    ///   (CAI region #3 entries whose object has a port `+0x1E4`, INFERRED: the region's port
    ///   slots, the model's [`super::world::RegionSlot::port`]).
    pub fn research_need(&self, faction: FactionId) -> [f32; 3] {
        let (enemies, allies) = (super::director::believed_enemy_count(self, faction), super::director::believed_ally_count(self, faction));
        let history = self.world.economy_history.get(&faction).map_or(&[][..], |h| &h[..]);
        let own = |r: &RegionId| self.world.regions.get(r).is_some_and(|x| x.owner == faction);
        let forts = self.world.forts.values().filter(|f| own(&f.region)).count() as u32;
        let ports = self.world.regions.values().filter(|r| r.owner == faction).map(|r| r.slots.iter().filter(|s| s.port).count() as u32).sum::<u32>();
        let forces = |navy: bool| self.world.forces.values().filter(|f| f.faction == faction && f.is_navy == navy && !f.units.is_empty()).count() as u32;
        let p1 = need_power((forts >> 1) + (forces(false) >> 2));
        let p2 = need_power((ports >> 1) + (forces(true) >> 2));
        research_need_values(need_from_enemies(enemies), need_from_allies(allies, enemies), need_from_economy(history), p1, p2)
    }

    /// The list-1 weight of `tech` (an AI with research need `need` ([`CampaignModel::research_need`]) receives
    /// it): [`received_technology_weight`] of the need for the technology's category ([`technology_category`]).
    pub fn received_technology_goal_weight(tech: &str, need: &[f32; 3]) -> f32 {
        technology_category(tech).map_or(0.0, |c| received_technology_weight(need[c]))
    }

    /// `0x00CCB150(a, b, index)`: a deal goal of action record `index` may be built between `a` and
    /// `b`: `a`'s `diplomacy_options` towards `b` is not 2 or 3 and `b`'s towards `a` not 1 or 3.
    /// CONFIRMED (disassembly).
    pub fn deal_goal_allowed(&self, a: FactionId, b: FactionId, index: usize) -> bool {
        self.may_propose(a, b, index) && self.may_accept(b, a, index)
    }

    /// The AI recipient's answer to the open negotiation's deal (the evaluator `0x00AA5ED0` for
    /// the records the model holds: 4 regions, 5 technology, 7 payment, 9 peace, then 11 join war): `None` when there is nothing to
    /// answer (no negotiation, a human recipient, no item), else whether it accepts. For each
    /// record with items, in record order, CONFIRMED:
    /// - decline when the recipient's `diplomacy_options` towards the proposer for the record is 1
    ///   or 3 (`0x00B27FE0`);
    /// - the record's goal weight ([`record_goal_weight`]):
    ///   - regions: one the proposer offers counts when it is one of the AI's list-1 goals (a
    ///     region of [`Self::tradeable_regions`] of the proposer), one it demands when it is a
    ///     list-2 goal (of the AI's tradeable regions), each [`REGION_GOAL_WEIGHT`], both only
    ///     when [`Self::deal_goal_allowed`];
    ///   - technologies: one the proposer offers counts when it is one of the AI's list-1 goals
    ///     ([`Self::is_technology_goal`] from the proposer to the AI and
    ///     [`Self::deal_goal_allowed`]) with [`Self::received_technology_goal_weight`]; one it
    ///     demands when it is a list-2 goal (from the AI to the proposer) with
    ///     [`Self::given_technology_goal_weight`];
    ///   - payment: 0 (`0x00A62400` builds its goal with weight 0);
    ///   - peace: [`Self::peace_goal_weight`];
    ///   - join war: [`Self::join_war_record_weight`];
    /// - [`DealEvaluation::add`] of the record's value ([`Self::region_deal_value`] /
    ///   [`Self::technology_deal_value`] / [`Self::peace_deal_value`] / [`Self::join_war_deal_value`] at the campaign's inflation
    ///   factor, [`Self::payment_deal_value`] as it is), scale 1.0.
    ///
    /// Then accept when `fair` and the diplomatic budget is at least the payment (slot 2 of the
    /// payment record's value, 0 without one), or `good` and the payment is at most the recipient's
    /// treasury (signed).
    ///
    /// PROVISIONAL: the budget (`0x00AAF570`, the finance pot `+0x140` = diplomatic spending bias ×
    /// treasury at its refresh, never negative until the AI's own paid diplomatic intentions (kind
    /// 8, `0x00CC53D0`) spend from it, which our AI does not have) is not ported: the `fair` test
    /// takes it as covering any payment; a deal the AI would not accept is declined where the exe
    /// may first make a counter-offer (`0x00CC58C0`, fewer than 10 per negotiation); the records the
    /// model does not hold (trade, alliance, ...) are not evaluated; the peace record reads the
    /// PROVISIONAL leaves of [`Self::peace_attitude_with`] and [`RegionStrengths`]; the join-war goal weight
    /// reads a PROVISIONAL stance-belief state ([`Self::join_war_attitude`]) and its value a
    /// PROVISIONAL nearness ([`Self::war_strength`]).
    pub fn ai_accepts_deal(&self) -> Option<bool> {
        use super::negotiation::NegotiationAction;
        let n = self.negotiations.current.as_ref()?;
        let (ai, proposer) = (n.recipient, n.proposer);
        let has_regions = !(n.regions.offered.is_empty() && n.regions.demanded.is_empty());
        let has_techs = !(n.technologies.offered.is_empty() && n.technologies.demanded.is_empty());
        let has_payment = n.payment.has_items();
        let has_join_war = !n.join_war.is_empty();
        if self.is_human(ai) || !(has_regions || has_techs || has_payment || n.peace || has_join_war) {
            return None;
        }
        let declines = |index: usize| !self.may_accept(ai, proposer, index);
        let inflation = self.deal_inflation.factor;
        let mut e = DealEvaluation::default();
        // One record's step (the same for both): decline, else its goal weight when the goals are allowed
        // and its value, added to the evaluation; false is a decline.
        let mut step = |action: NegotiationAction, weight: &dyn Fn(bool) -> f32, value: &dyn Fn() -> DealValue| {
            let index = action.option();
            if declines(index) {
                return false;
            }
            let weight = weight(self.deal_goal_allowed(ai, proposer, index));
            e.add(index, value(), weight, 1.0);
            true
        };
        if has_regions {
            let weight = |allowed: bool| {
                // One lookup of each side's tradeable regions for the whole record.
                let (theirs, ours): (Vec<RegionId>, Vec<RegionId>) = (self.tradeable_regions(proposer).collect(), self.tradeable_regions(ai).collect());
                let goal = |side: &[RegionId], r: &RegionId| (allowed && side.contains(r)).then_some(REGION_GOAL_WEIGHT);
                record_goal_weight(&n.regions, |r| goal(&theirs, r), |r| goal(&ours, r))
            };
            if !step(NegotiationAction::Regions, &weight, &|| self.region_deal_value(n, inflation)) {
                return Some(false);
            }
        }
        if has_techs {
            let weight = |allowed: bool| {
                // The AI's research need does not change between items; only needed when allowed.
                let need = allowed.then(|| self.research_need(ai));
                record_goal_weight(
                    &n.technologies,
                    |t| need.as_ref().filter(|_| self.is_technology_goal(t, proposer, ai)).map(|need| Self::received_technology_goal_weight(t, need)),
                    |t| (allowed && self.is_technology_goal(t, ai, proposer)).then(|| self.given_technology_goal_weight(t)),
                )
            };
            if !step(NegotiationAction::Technology, &weight, &|| self.technology_deal_value(n, inflation)) {
                return Some(false);
            }
        }
        // Record 7: weight 0 (0x00A62400); its value's slot 2 is the payment the accept tests read.
        let payment = if has_payment {
            let value = self.payment_deal_value(n);
            if !step(NegotiationAction::Payments, &|_| 0.0, &|| value) {
                return Some(false);
            }
            value.given as i32
        } else {
            0
        };
        // Record 9, peace: its goal weight, then its value; the region strengths read once for both.
        if n.peace {
            let st = RegionStrengths::of(self);
            let weight = |allowed: bool| self.peace_goal_weight(ai, proposer, allowed, &st);
            if !step(NegotiationAction::Peace, &weight, &|| self.peace_deal_value(ai, proposer, &st).inflated(inflation)) {
                return Some(false);
            }
        }
        // Record 11, join war: its goal weight ([`Self::join_war_record_weight`]), then its value.
        if has_join_war && !step(NegotiationAction::RequestJoinWar, &|allowed| self.join_war_record_weight(n, allowed), &|| self.join_war_deal_value(n, inflation)) {
            return Some(false);
        }
        let budget_ok = true;
        let treasury = self.world.factions.get(&ai).map_or(0, |f| f.treasury);
        Some((e.fair() && budget_ok) || (e.good() && payment <= treasury))
    }

    /// The payment record of `n` valued for its AI recipient (virtual `+0x38` = `0x00C12EE0`; not
    /// inflated), with `a` the record's amount ([`super::negotiation::DealPayment`]'s sign) and `t`
    /// its turns (compared signed, as the exe's int). CONFIRMED (disassembly):
    /// - `t` ≤ 1: (max(−a, 0), max(a, 0), max(a, 0)): the AI gains an offered amount and gives a
    ///   demanded one;
    /// - else the payer `F` (the recipient for a demand, `a` ≥ 0; the proposer for an offer) can pay at
    ///   most `cap` = f32(treasury(F) + max(t × (last income − last expenses of F), 0)) × 0.8
    ///   (`0x00BCAFE0`, `0x00BBE970`, `0x00BBE910`; wrapping int sums; constant `0x0131A7B4`), and the
    ///   payments are worth `k` = (0.9^t − 1) × −10.0 times one (`powf`, constants `0x01325CEC`,
    ///   `0x01318048`, `0x0133BCB4`); a demand is (0, trunc(min(f32(f64(k) × f64(a)), cap)), a × t),
    ///   an offer (trunc(min(k × f32(−a), cap)), 0, 0). `trunc` is CVTTSS2SI.
    pub fn payment_deal_value(&self, n: &Negotiation) -> DealValue {
        let (a, t) = (n.payment.amount, n.payment.turns as i32);
        if t <= 1 {
            let (gain, cost) = (a.wrapping_neg().max(0) as u32, a.max(0) as u32);
            return DealValue { gain, cost, given: cost };
        }
        let payer = if a >= 0 { n.recipient } else { n.proposer };
        let w = &self.world;
        let future = t.wrapping_mul(w.last_income(payer).wrapping_sub(w.last_expenses(payer))).max(0);
        let treasury = w.factions.get(&payer).map_or(0, |f| f.treasury);
        let cap = treasury.wrapping_add(future) as f32 * 0.8;
        let k = (0.9f32.powf(t as f32) - 1.0) * -10.0;
        // MINSS: the value unless it is not below the cap.
        let min = |v: f32| if v < cap { v } else { cap };
        if a >= 0 {
            let v = super::commands::cvttss2si(min((f64::from(k) * f64::from(a)) as f32));
            DealValue { gain: 0, cost: v as u32, given: a.wrapping_mul(t) as u32 }
        } else {
            let v = super::commands::cvttss2si(min(k * a.wrapping_neg() as f32));
            DealValue { gain: v as u32, cost: 0, given: 0 }
        }
    }

    /// The factions that have `tech` researched (state 0).
    fn technology_holders(&self, tech: &str) -> usize {
        self.world.factions.keys().filter(|&&f| self.tech_state(f, tech) == Some(state::RESEARCHED)).count()
    }

    /// The technology items of `n` valued for its AI recipient (the technology record's value,
    /// virtual `+0x38` = `0x00C13360`): each offered technology, then each demanded one
    /// ([`technology_value`]), summed and then [`DealValue::inflated`] by `inflation`. The holders
    /// are the factions whose state for it is researched, over every faction of the campaign
    /// (INFERRED: the exe walks the list at campaign `+0x110` and skips an entry whose `+0x194` is
    /// null, not traced further).
    pub fn technology_deal_value(&self, n: &Negotiation, inflation: f32) -> DealValue {
        let proposer_human = self.is_human(n.proposer);
        let value = |tech: &String, offered: bool| {
            let cost = self.rules.technologies.get(tech).map_or(0, |t| t.cost);
            let holders = self.technology_holders(tech);
            let traded = self.world.faction_details.get(&n.proposer).and_then(|d| d.research.get(tech)).map_or(0, |t| t.traded);
            technology_value(cost, holders, traded, offered, proposer_human)
        };
        let offered = n.technologies.offered.iter().map(|t| value(t, true));
        let demanded = n.technologies.demanded.iter().map(|t| value(t, false));
        offered.chain(demanded).fold(DealValue::default(), |s, v| s + v).inflated(inflation)
    }

    /// The region items of `n` valued for its AI recipient (the regions record's value, virtual
    /// `+0x38` = `0x00C131C0`; CONFIRMED steps):
    /// - `0x00A364B0`: each region the proposer offers is worth [`Self::region_deal_worth`] to the
    ///   AI, added to slot 2 when either of the exe's two tests holds (`0x00C3E0C0`: campaign
    ///   region `+0x22C`, UNKNOWN meaning; or the CAI region's strategy `+0x34` virtual `+0x94`,
    ///   INFERRED a siege), else to slot 0 when the AI's attitude to the region
    ///   ([`super::economy::hypothetical_order_attitude`], `0x00A79050` with the AI's state religion
    ///   and faction, CONFIRMED: the call at `0x00A365BC`) is at least 0, or at least −5 with the region
    ///   bordering one of the AI's ([`super::World::region_neighbours`]), else nowhere; each region the proposer demands adds
    ///   its worth to the proposer to slot 1 (unless either test holds) and its worth to the AI to
    ///   slot 2;
    /// - the sum times the inflation factor ([`DealValue::inflated`]);
    /// - [`human_demand_scaled`] with `m` = the demanded regions plus
    ///   [`super::World::deal_regions_received`] when the proposer is human and the deal holds no
    ///   peace item (0 otherwise).
    ///
    /// PROVISIONAL: the two tests read false (sieges are not in the model; `+0x22C` is UNKNOWN).
    pub fn region_deal_value(&self, n: &Negotiation, inflation: f32) -> DealValue {
        let (ai, proposer) = (n.recipient, n.proposer);
        // The regions bordering one of the AI's, built once and only if a test needs it.
        let ai_border = std::cell::OnceCell::new();
        let borders_ai = |r: RegionId| {
            ai_border
                .get_or_init(|| {
                    let owned = self.world.regions.values().filter(|x| x.owner == ai);
                    owned.filter_map(|x| self.world.region_neighbours.get(&x.id)).flatten().copied().collect::<std::collections::BTreeSet<RegionId>>()
                })
                .contains(&r)
        };
        let (ai_census, proposer_census) = (self.region_census(ai), self.region_census(proposer));
        let mut v = DealValue::default();
        for &r in &n.regions.offered {
            // A deal only holds existing regions (`propose_regions` refuses an unknown id).
            let Some(reg) = self.world.regions.get(&r) else { continue };
            let a = super::economy::hypothetical_order_attitude(self, reg, ai);
            if a >= 0 || (a >= -5 && borders_ai(r)) {
                v.gain = v.gain.wrapping_add(self.region_deal_worth_with(r, ai, &ai_census));
            }
        }
        for &r in &n.regions.demanded {
            v.cost = v.cost.wrapping_add(self.region_deal_worth_with(r, proposer, &proposer_census));
            v.given = v.given.wrapping_add(self.region_deal_worth_with(r, ai, &ai_census));
        }
        let m = if self.is_human(proposer) && !n.peace {
            (n.regions.demanded.len() as u32).wrapping_add(self.world.deal_regions_received.get(&proposer).copied().unwrap_or(0))
        } else {
            0
        };
        human_demand_scaled(v.inflated(inflation), m)
    }

    /// `0x00AA1E90`: region `r`'s worth to `faction` in a deal ([`region_value::faction_value`]),
    /// from its base ([`region_value::stored_or_formula`]). Read from the model, CONFIRMED: `n`
    /// counts `r`'s neighbours its owner holds; the theatre doubling counts `faction`'s regions
    /// over the whole map (vanilla campaigns have one theatre holding every region); the ×1.5
    /// reads the regions' building slots.
    /// PROVISIONAL: the own branch is `r`'s owner being `faction` and `r` belonging to a region
    /// group (`+0x12C`); groups are not in the model, so every region is taken to have one
    /// (INFERRED from the group analysis grouping each faction's regions, not traced); the region
    /// group's change state is NEW (the group analysis, belief 0x52, is not ported), so the
    /// personality multipliers are not reached and read their shipped defaults, not the AI
    /// personality's; the faction's CAI region list `+0x1C0` (`0x00C45CF0`, ×2) is not in the
    /// model and reads false; a map with several theatres is counted as one.
    pub fn region_deal_worth(&self, r: RegionId, faction: FactionId) -> u32 {
        self.region_deal_worth_with(r, faction, &self.region_census(faction))
    }

    /// How many regions `faction` holds and how many of them have building slots: the two counts every
    /// [`Self::region_deal_worth_with`] of one evaluation shares.
    fn region_census(&self, faction: FactionId) -> RegionCensus {
        let owned = || self.world.regions.values().filter(|x| x.owner == faction);
        RegionCensus { held: owned().count(), with_slots: owned().filter(|x| !x.slots.is_empty()).count() }
    }

    /// [`Self::region_deal_worth`] with `faction`'s [`RegionCensus`] already taken.
    fn region_deal_worth_with(&self, r: RegionId, faction: FactionId, census: &RegionCensus) -> u32 {
        use super::region_value::{faction_value, stored_or_formula, FactionRegion, GroupChange, Multipliers};
        // A deal only holds existing regions (`propose_regions` refuses an unknown id).
        let Some(reg) = self.world.regions.get(&r) else { return 0 };
        // PROVISIONAL: every region is taken to belong to a region group (`+0x12C` non-null).
        let grouped = true;
        let own_branch = reg.owner == faction && grouped;
        let held = census.held;
        let owner_neighbours = self
            .world
            .region_neighbours
            .get(&r)
            .map_or(0, |ns| ns.iter().filter(|x| self.world.regions.get(x).is_some_and(|x| x.owner == reg.owner)).count()) as i32;
        let fr = FactionRegion {
            own_branch,
            change: GroupChange::New,
            owner_neighbours,
            theatre_double: if own_branch { held == 1 } else { held == 0 },
            key_region: false,
            // No other region of `faction` has slots: its slotted regions are `r` alone (when `r` is its own).
            last_with_slots: !reg.slots.is_empty() && census.with_slots == usize::from(reg.owner == faction),
        };
        let base = stored_or_formula(self.world.region_base_values.get(&r).copied(), reg.gdp);
        faction_value(base, &fr, &Multipliers::from_tunables(|_, d| d)) as u32
    }

    /// Whether `tech` is a technology goal from `giver` to `receiver` (`0x008F4F10(this = giver,
    /// out, receiver)`, used by both goal lists): the giver has it researched (state 0) and the
    /// receiver has it in state 1, 2 or 3 (`0x008F3AC0`). CONFIRMED.
    pub fn is_technology_goal(&self, tech: &str, giver: FactionId, receiver: FactionId) -> bool {
        self.tech_state(giver, tech) == Some(state::RESEARCHED) && matches!(self.tech_state(receiver, tech), Some(1..=3))
    }

    /// The list-2 weight of `tech` (the AI gives it): [`given_technology_weight`] of
    /// [`technology_spread`]. The CAI world's faction list is taken to be every faction of the
    /// campaign (INFERRED: `0x00D63120` walks CAI world `+0x120`, whose builder is not traced).
    pub fn given_technology_goal_weight(&self, tech: &str) -> f32 {
        given_technology_weight(technology_spread(self.technology_holders(tech), self.world.factions.len()))
    }

    /// `0x008FA020(this = a, out, b)`: the join-war goal targets between `a` and `b`, in `a`'s record
    /// order ([`super::World::record_targets_in_order`]): each `t` that `a`'s record stance is war
    /// towards (`+0x790 → +0x10` = 0, `0x008FA06C..76`), not `b` (`0x00898B80`), and that `b`'s
    /// record towards (`0x00B64C50`) is not war (`+0x790 → +0x10` ≠ 0, `0x008FA0BE..C8`): a war `b`
    /// could still join. CONFIRMED (disassembly). Both tests read the records' stances
    /// ([`super::World::stance`]), not [`Self::at_war`]'s rebel rule.
    ///
    /// ORIGINAL BUG: `0x00B64C50` never returns null; when `b` holds no record towards `t` it falls
    /// back to `b`'s first record (`0x00B64C88`), so the exe tests an unrelated faction's stance.
    /// Ours reads the missing record as not at war (the stance default), as `region_open_to` does
    /// for the same lookup. Unreachable with shipped data: the rebels hold no records and are in no
    /// one's (so never a `t`), and every other pair has one (`eur_napoleon` startpos, measured
    /// 2026-10-19).
    pub fn join_war_targets(&self, a: FactionId, b: FactionId) -> Vec<FactionId> {
        use super::world::Stance;
        let w = &self.world;
        w.record_targets_in_order(a).into_iter().filter(|&t| t != b && w.stance(a, t) == Stance::War && w.stance(b, t) != Stance::War).collect()
    }

    /// `0x00C1A6A0(ai, p)`: allied ([`Self::allied_with`], `0x008CE390`) with a war joinable either
    /// way ([`Self::joinable_war`]). CONFIRMED (disassembly).
    fn join_war_pair_allowed(&self, ai: FactionId, p: FactionId) -> bool {
        self.allied_with(ai, p) && (self.joinable_war(ai, p) || self.joinable_war(p, ai))
    }

    /// Whether `g` is on `f`'s CAI relation belief's ally list ([`super::director::believes_allied`]).
    pub fn believes_allied(&self, f: FactionId, g: FactionId) -> bool {
        super::director::believes_allied(self, f, g)
    }

    /// `f`'s patron as its CAI relation belief holds it (`+0x100` info `+0x90`, `0x00A9BDC0`): the
    /// faction whose stance towards `f` is patron (`IsStancePatron` `0x00B5DCB0`), from the same walk
    /// as the ally list (in the game, no rebels). CONFIRMED.
    pub fn believed_patron(&self, f: FactionId) -> Option<FactionId> {
        self.patron(f).filter(|&g| !self.is_rebel_faction(f) && !self.is_rebel_faction(g) && self.in_the_game(g))
    }

    /// The AI's attitude to joining `p`'s war on `t` (its attitude object's virtual `+0x2C4` =
    /// `0x00D62DA0`, on all three of the AI personality vtables `0x0138EBB0` / `0x0138EEAC` /
    /// `0x0138F1B8`; CONFIRMED): [`join_war_state_offset`] of the AI's stance-belief state towards
    /// `p` minus that towards `t`, plus `rel(ai, p) − rel(ai, t)` ([`super::director::relation_belief_value`],
    /// entry `+8`) when the AI's relation belief has entries for both (`0x00A89E00`: the CAI world's
    /// factions, [`super::director::in_cai_world`]).
    ///
    /// PROVISIONAL: the stance-belief state (`0x00A74D60` → `0x00ABCB80`, 0..4) is not ported (it
    /// reads another belief, `0x00A73390`, the multi relation `0x00A7C270` and more), so both states
    /// read 2 (offset 0).
    pub fn join_war_attitude(&self, ai: FactionId, p: FactionId, t: FactionId) -> f32 {
        // PROVISIONAL: state 2 for both until 0x00ABCB80 is ported.
        let (state_p, state_t) = (2u8, 2u8);
        let offsets = join_war_state_offset(state_p) - join_war_state_offset(state_t);
        let has_entry = |x: FactionId| x != ai && super::director::in_cai_world(self, x);
        if has_entry(p) && has_entry(t) {
            let rel = |x: FactionId| super::director::relation_belief_value(self, ai, x);
            (rel(p) - rel(t)) + offsets
        } else {
            offsets
        }
    }

    /// The cap of the AI's list-2 join-war weights by what `p` is to it (`0x00CCB210` case 11,
    /// CONFIRMED): −0.1 when `p` is the AI's patron ([`Self::believed_patron`]), −0.25 when an ally
    /// ([`Self::believes_allied`]), else −0.5.
    fn given_join_war_cap(&self, ai: FactionId, p: FactionId) -> f32 {
        if self.believed_patron(ai) == Some(p) {
            -0.1
        } else if self.believes_allied(ai, p) {
            -0.25
        } else {
            -0.5
        }
    }

    /// The list-2 weight of `p`'s war on `t` (the AI would join it; `0x00CCB210` case 11,
    /// CONFIRMED): −1 when `t` has no CAI faction (`0x00C3C4E0`, [`super::director::in_cai_world`]),
    /// else [`given_join_war_weight`] of [`Self::join_war_attitude`] capped by what `p` is to the
    /// AI (`cap`, [`Self::given_join_war_cap`], computed once for a whole record). (The exe also
    /// gives −1 to a goal whose item list does not hold exactly one faction; a list-2 goal is built
    /// with one.)
    fn given_join_war_goal_weight(&self, ai: FactionId, p: FactionId, t: FactionId, cap: f32) -> f32 {
        if !super::director::in_cai_world(self, t) {
            return -1.0;
        }
        given_join_war_weight(self.join_war_attitude(ai, p, t), cap)
    }

    /// `0x00AB5400(attacker, target)`: the attacker's war strength against the target, over its
    /// mobiles (its forces, CAI faction `+0x14C`) that have units (`+0x160`): each force's value
    /// ([`Self::force_power`]) counts in full when it is land (`+0x168`) and near the target, a tenth
    /// otherwise, a navy's halved first (`>> 1`): `near + (total − near) / 10` in unsigned integers,
    /// `total` = land values + navy values >> 1. CONFIRMED (decompile).
    ///
    /// A land force is near when its area (mobile `+0x10`, the region its commander stands in,
    /// [`super::economy::in_region`]) is owned by the target (area virtual `+0xC`, CONFIRMED) or one
    /// of the area's boundary areas (`0x00C3DE80` over `+0x170`/`+0x174`, `GetBoundaryOtherRegion`
    /// `0x00C1BCE0`) answers its virtual `+8` for the target. PROVISIONAL: that virtual is taken as
    /// "owned by the target" (INFERRED: the wrapper `0x01382678`'s `+8` (`0x00C3FD10`) and `+0xC`
    /// (`0x00C45CF0`) both forward to the same inner object, and with none `+8(x)` is `x` = null
    /// where `+0xC` is null), the boundary areas are the region's land neighbours
    /// ([`super::World::region_neighbours`]), and `0x00C53FB0` (area `+0x190 → +0xC` = 1, meaning
    /// UNKNOWN), which takes a force out of the near count, reads false.
    pub fn war_strength(&self, attacker: FactionId, target: FactionId) -> u32 {
        self.war_strength_of(&self.war_mobiles(attacker), target)
    }

    /// The attacker's mobiles for [`Self::war_strength`], read once per evaluation: each force with
    /// units, its value ([`Self::force_power`]), whether it is a navy and its area (the region its
    /// commander stands in; `None` without one, never near).
    fn war_mobiles(&self, attacker: FactionId) -> Vec<WarMobile> {
        // Unknown unit keys count 0 here; the rankings report them (`FactionRankings::unknown_units`).
        let mut unknown = std::collections::BTreeSet::new();
        let area = |f: &super::MilitaryForce| {
            let c = f.commander.and_then(|c| self.world.characters.get(&c))?;
            super::economy::region_of(self, c).map(|r| r.id)
        };
        let forces = self.world.forces.values().filter(|f| f.faction == attacker && !f.units.is_empty());
        forces.map(|f| WarMobile { value: self.force_power(f, &mut unknown) as u32, navy: f.is_navy, area: if f.is_navy { None } else { area(f) } }).collect()
    }

    /// [`Self::war_strength`] over mobiles already read ([`Self::war_mobiles`]).
    fn war_strength_of(&self, mobiles: &[WarMobile], target: FactionId) -> u32 {
        let owned_by_target = |r: &RegionId| self.world.regions.get(r).is_some_and(|x| x.owner == target);
        let near = |area: RegionId| owned_by_target(&area) || self.world.region_neighbours.get(&area).is_some_and(|ns| ns.iter().any(owned_by_target));
        let (mut near_sum, mut total) = (0u32, 0u32);
        for m in mobiles {
            if m.navy {
                total = total.wrapping_add(m.value >> 1);
            } else {
                total = total.wrapping_add(m.value);
                if m.area.is_some_and(near) {
                    near_sum = near_sum.wrapping_add(m.value);
                }
            }
        }
        near_sum.wrapping_add(total.wrapping_sub(near_sum) / 10)
    }

    /// The join-war record's goal weight for `n`'s AI recipient ([`record_goal_weight`]), with
    /// `allowed` the record's [`Self::deal_goal_allowed`]: 0 unless that and `0x00C1A6A0` (the AI
    /// and the proposer allied, `0x008CE390`, with a war joinable either way, `0x00C1E590`), the gate
    /// of both goal lists; else each faction the proposer offers to fight counts
    /// [`ASKED_JOIN_WAR_WEIGHT`] when it is one of the AI's list-1 goals (`0x00CC13D0`:
    /// [`Self::join_war_targets`] from the AI to the proposer), each one the AI is asked to fight
    /// its list-2 weight when it is a list-2 goal (`0x00CC0280`: from the proposer to the AI): −1
    /// for a target with no CAI faction, else [`given_join_war_weight`] of
    /// [`Self::join_war_attitude`]. CONFIRMED.
    pub fn join_war_record_weight(&self, n: &Negotiation, allowed: bool) -> f32 {
        let (ai, proposer) = (n.recipient, n.proposer);
        if !(allowed && self.join_war_pair_allowed(ai, proposer)) {
            return 0.0;
        }
        let (asked, given) = (self.join_war_targets(ai, proposer), self.join_war_targets(proposer, ai));
        let cap = self.given_join_war_cap(ai, proposer);
        record_goal_weight(
            &n.join_war,
            |t| asked.contains(t).then_some(ASKED_JOIN_WAR_WEIGHT),
            |t| given.contains(t).then(|| self.given_join_war_goal_weight(ai, proposer, *t, cap)),
        )
    }

    /// The join-war record of `n` valued for its AI recipient (virtual `+0x38` = `0x00C12D40`,
    /// each faction through `0x00A75330` → `0x00A34E80`; CONFIRMED): with `att` the AI's attitude
    /// total towards the proposer and the factor's endpoints [`JOIN_WAR_FACTOR_AT_BEST`] /
    /// [`JOIN_WAR_FACTOR_AT_WORST`] ([`attitude_scaled`]), each faction `f` the proposer offers to
    /// fight is worth (2 × [`attitude_scaled`]([`Self::war_strength`](proposer, f)), 0, 0), each one
    /// the AI is asked to fight (0, [`attitude_scaled`](war_strength(ai, f)), war_strength(f, ai) +
    /// 1000); summed (wrapping) and then [`DealValue::inflated`] by `inflation`.
    pub fn join_war_deal_value(&self, n: &Negotiation, inflation: f32) -> DealValue {
        let (ai, proposer) = (n.recipient, n.proposer);
        let att = self.world.relationships.get(&(ai, proposer)).map_or(0, super::details::Relationship::attitude_total);
        let scaled = |v: u32| attitude_scaled(v, att, JOIN_WAR_FACTOR_AT_BEST, JOIN_WAR_FACTOR_AT_WORST);
        // Each side's mobiles once for the record, not once per item, and only for a side with items.
        let mobiles = |f: FactionId, items: &[FactionId]| if items.is_empty() { Vec::new() } else { self.war_mobiles(f) };
        let (theirs, ours) = (mobiles(proposer, &n.join_war.offered), mobiles(ai, &n.join_war.demanded));
        let offered = n.join_war.offered.iter().map(|&f| DealValue { gain: scaled(self.war_strength_of(&theirs, f)).wrapping_mul(2), cost: 0, given: 0 });
        let demanded = n.join_war.demanded.iter().map(|&f| DealValue {
            gain: 0,
            cost: scaled(self.war_strength_of(&ours, f)),
            given: self.war_strength(f, ai).wrapping_add(1000),
        });
        offered.chain(demanded).fold(DealValue::default(), |s, v| s + v).inflated(inflation)
    }

    /// A faction's difficulty (faction `+0x6E4`, `0x00C288E0`; a human's −2..1, an AI's the player's negated).
    fn faction_difficulty(&self, f: FactionId) -> i32 {
        self.world.faction_details.get(&f).map_or(0, |d| d.difficulty)
    }

    /// The AI's capital when it holds exactly one region (`GetCaiFactionRegionCount` = 1 and the
    /// region `0x00C1C200` gives), and the capital's land neighbours (its boundaries' other
    /// regions, as [`Self::war_strength`] reads them).
    fn lone_capital(&self, ai: FactionId) -> Option<(RegionId, &[RegionId])> {
        let w = &self.world;
        if w.regions.values().filter(|r| r.owner == ai).count() != 1 {
            return None;
        }
        let cap = w.capital(ai).filter(|c| w.regions.get(c).is_some_and(|r| r.owner == ai))?;
        Some((cap, w.region_neighbours.get(&cap).map_or(&[][..], Vec::as_slice)))
    }

    /// `0x00D593F0` (attitude virtual `+0x29C`) at war: the AI's eagerness to make peace with `p`
    /// for its capital's sake. CONFIRMED (decompile): −1 when the AI's patron
    /// ([`Self::believed_patron`]) is someone other than `p`; else, the AI holding one region, its
    /// capital ([`Self::lone_capital`]), with `s` the AI's strength there and `t` `p`'s there plus
    /// in each neighbour ([`RegionStrengths`]) and `q` = t / max(s, 1) (integers): when the capital
    /// borders a region of `p`, and `s` < t / 2: q 2 → 0.25, 3 → 0.5, 4 → 0.75, more → 0.9; when it
    /// does not, and `s` < t / 4: q 4 → 0.25, 5 → 0.5, 6 → 0.75, more → 0.9; −1 otherwise.
    ///
    /// The exe's branch for an AI not at war with `p` (an ally whose capital borders `p`) is not
    /// ported: the one caller, [`Self::peace_attitude`], reads it only at war.
    fn protector_attitude(&self, ai: FactionId, p: FactionId, st: &RegionStrengths) -> f32 {
        if self.believed_patron(ai).is_some_and(|g| g != p) {
            return -1.0;
        }
        let Some((cap, neighbours)) = self.lone_capital(ai) else { return -1.0 };
        let s = st.get(cap, ai);
        let t = neighbours.iter().fold(st.get(cap, p), |t, &nb| t.wrapping_add(st.get(nb, p)));
        let q = t / s.max(1);
        let borders = neighbours.iter().any(|nb| self.world.regions.get(nb).is_some_and(|r| r.owner == p));
        if borders && s < t / 2 {
            match q {
                0 | 1 => -1.0,
                2 => 0.25,
                3 => 0.5,
                4 => 0.75,
                _ => 0.9,
            }
        } else if !borders && s < t / 4 {
            match q {
                0..=3 => -1.0,
                4 => 0.25,
                5 => 0.5,
                6 => 0.75,
                _ => 0.9,
            }
        } else {
            -1.0
        }
    }

    /// `0x00D63810` (attitude virtual `+0x2D4`): `p` threatens the AI's lone capital
    /// ([`Self::lone_capital`]). CONFIRMED (decompile): with `s` the AI's strength there and `t`
    /// `p`'s ([`RegionStrengths`]): false when `t` is 0; true when `t` > 3s (or > 2s with the
    /// capital's strategy test `+0x34 → +0x94`); else `t` plus, over the capital's neighbours,
    /// max(`p`'s strength − the AI's, 0), against 4s.
    ///
    /// PROVISIONAL: the strategy test (INFERRED a siege, as [`Self::region_deal_value`]'s) reads false.
    fn peace_threatened_by(&self, ai: FactionId, p: FactionId, st: &RegionStrengths) -> bool {
        let Some((cap, neighbours)) = self.lone_capital(ai) else { return false };
        let (s, mut t) = (st.get(cap, ai), st.get(cap, p));
        if t == 0 {
            return false;
        }
        let siege = false;
        if (siege && t > s.wrapping_mul(2)) || t > s.wrapping_mul(3) {
            return true;
        }
        for &nb in neighbours {
            t = t.wrapping_add(st.get(nb, p).wrapping_sub(st.get(nb, ai)).max(0));
        }
        t > s.wrapping_mul(4)
    }

    /// The AI's peace attitude towards `p` (`0x00D63260`, attitude virtual `+0x2B0`), from −1 (no
    /// peace) to 1 (eager); `valuing` is the call's flag (the peace value's read, 1; the goal's, 0).
    /// [`Self::peace_attitude_with`] with the strengths read for it.
    pub fn peace_attitude(&self, ai: FactionId, p: FactionId, valuing: bool) -> f32 {
        self.peace_attitude_with(ai, p, valuing, &RegionStrengths::of(self))
    }

    /// [`Self::peace_attitude`] with the region strengths already read. CONFIRMED steps (decompile),
    /// with `rebels` = either side the rebels and the AI's record towards `p` (war momentum, turns
    /// at war, turns since a battle; read only without `rebels`):
    /// 1. −1 when the campaign has [`super::features::CampaignFeatures::ai_peace_refusal_attitude`],
    ///    `p` is human and the AI's attitude total towards it is at most that;
    /// 2. 0 when `p` is not on the AI's enemy list ([`super::director::believes_at_war`]);
    /// 3. `f` = [`Self::protector_attitude`] when the AI has no protectorates
    ///    ([`super::director::believed_protectorates`]), else −1;
    /// 4. `f` = 1 when (A: not `valuing`, not `rebels`, more than 1 turn at war and momentum > 0 or
    ///    more than 5 turns since a battle; or B: not `rebels`, no protectorates and `f` > 0) and
    ///    none of the AI's protectorates nor its allies ([`super::director::believed_allies`]) is at
    ///    war with `p`;
    /// 5. else `f` = 1 when not `rebels`, more than 1 turn at war and momentum below −10; else
    ///    max(`f`, −0.8) when `rebels` or `p` does not threaten the AI ([`Self::peace_threatened_by`]);
    ///    else 1;
    /// 6. without `rebels`, with a human other than `p` on the AI's enemy list (`0x00A64A60`): at
    ///    least 0 for an AI of difficulty 0 and 0.5 for one above 0 (`f` at most that gives it).
    ///
    /// PROVISIONAL: the leaves the model lacks read as absent: A also needs the AI's war-event
    /// belief (`0x00D56FD0`), the AI's target faction (CAI faction `+0x1B0`) and `0x00C3E4A0` to be
    /// clear, and those two raise the step-5 threshold to −15 / −20; an ally counts in step 4 only
    /// with an alliance attitude above 0 (`0x00D61D80`, attitude virtual `+0x2AC`, not ported), taken
    /// as every ally; [`RegionStrengths`].
    fn peace_attitude_with(&self, ai: FactionId, p: FactionId, valuing: bool, st: &RegionStrengths) -> f32 {
        use super::director::{believed_allies, believed_enemies, believed_protectorates, believes_at_war};
        let rel = self.world.relationships.get(&(ai, p));
        if let Some(min) = self.rules.features.ai_peace_refusal_attitude
            && self.is_human(p)
            && rel.map_or(0, super::details::Relationship::attitude_total) <= min
        {
            return -1.0;
        }
        if !believes_at_war(self, ai, p) {
            return 0.0;
        }
        // PROVISIONAL: the war-event belief, the target faction and 0x00C3E4A0 read clear.
        let (recent, target, c3e4a0) = (false, false, false);
        let protectorates = believed_protectorates(self, ai);
        let mut f = if protectorates.is_empty() { self.protector_attitude(ai, p, st) } else { -1.0 };
        let rebels = self.is_rebel_faction(ai) || self.is_rebel_faction(p);
        let (momentum, long, since_battle) = match rel {
            Some(r) if !rebels => (r.war_momentum, r.war_turns > 1, r.turns_since_battle),
            _ => (0, false, 0),
        };
        let a = !valuing && !c3e4a0 && !rebels && !recent && !target && long && (momentum > 0 || since_battle > 5);
        let b = !rebels && protectorates.is_empty() && f > 0.0;
        // PROVISIONAL: every ally is taken to have an alliance attitude above 0.
        let free = (a || b) && !protectorates.iter().any(|&g| believes_at_war(self, g, p)) && !believed_allies(self, ai).into_iter().any(|x| believes_at_war(self, x, p));
        if free {
            f = 1.0;
        } else {
            let threshold = if target {
                -20
            } else if recent {
                -15
            } else {
                -10
            };
            if !rebels && long && momentum < threshold {
                f = 1.0;
            } else if rebels || !self.peace_threatened_by(ai, p, st) {
                f = f.max(-0.8);
            } else {
                f = 1.0;
            }
        }
        if !rebels && believed_enemies(self, ai).into_iter().any(|g| g != p && self.is_human(g)) {
            match self.faction_difficulty(ai) {
                0 if f <= 0.0 => return 0.0,
                d if d > 0 && f <= 0.5 => return 0.5,
                _ => {}
            }
        }
        f
    }

    /// The peace record's goal weight for the AI (`0x00CC13D0` case 9, CONFIRMED): with the
    /// record's goals allowed ([`Self::deal_goal_allowed`], `allowed`), the two at war
    /// ([`Self::at_war`]) and the AI's peace option towards `p` 0 or 1 (`0x00C1A730`), its peace
    /// attitude ([`Self::peace_attitude`], flag 0) clamped to [−1, 0.9]; else 0 (no goal).
    ///
    /// PROVISIONAL: the AI's peace intentions (`0x00CCC7C0`, script-gated) build no goal here.
    fn peace_goal_weight(&self, ai: FactionId, p: FactionId, allowed: bool, st: &RegionStrengths) -> f32 {
        use super::negotiation::NegotiationAction;
        if allowed && self.at_war(ai, p) && self.diplomacy_option(ai, p, NegotiationAction::Peace.option()) <= 1 {
            self.peace_attitude_with(ai, p, false, st).clamp(-1.0, 0.9)
        } else {
            0.0
        }
    }

    /// The peace record valued for its AI recipient `ai` and proposer `p` (virtual `+0x38` =
    /// `0x00C130E0` → `0x00A75390` → `0x00A351E0`; not inflated). CONFIRMED (disassembly):
    /// [`peace_value`] of:
    /// - `base`: [`peace_base_pair`] of [`peace_attitude_state`] of the AI's peace attitude (flag 1);
    /// - `rank_log`: [`peace_rank_log`] of 5 − each side's power category
    ///   ([`Self::faction_rankings`]; 0 for an unranked side), with `p`'s [`human_rank_boost`];
    /// - `own_mean`: [`peace_weighted_mean`] over the AI's regions of log10 of the clamped ratio of
    ///   its enemies' strength ([`super::director::believed_enemies`], each `e`'s strength × k ×
    ///   [`human_strength_factor`] for a human, summed through an integer) over max(its own, 1): the
    ///   AI's strength × k plus each ally's ([`super::director::believed_allies`]) × k halved
    ///   (`>> 1`), k = 2 in the capital, else 1;
    /// - `their_mean`: the same over `p`'s regions of the AI's strength × k over `p`'s side (`p`'s ×
    ///   k × its factor, truncated, plus each of `p`'s allies' × k halved);
    /// - the AI's record towards `p`, and `p`'s humanity and difficulty.
    ///
    /// The strengths are [`RegionStrengths`] (PROVISIONAL).
    fn peace_deal_value(&self, ai: FactionId, p: FactionId, st: &RegionStrengths) -> DealValue {
        use super::commands::cvttss2si;
        use super::director::{believed_allies, believed_enemies};
        let base = peace_base_pair(peace_attitude_state(self.peace_attitude_with(ai, p, true, st)));
        let ranks = self.faction_rankings().ranks;
        let rank = |f: FactionId| ranks.get(&f).map_or(0, |r| 5 - i32::from(r.power));
        let p_human = self.is_human(p);
        let boost = if p_human { human_rank_boost(self.faction_difficulty(p)) } else { 0.0 };
        let rank_log = peace_rank_log(rank(ai), rank(p), boost);
        let scaled = |v: i32, f: FactionId| if self.is_human(f) { v as f32 * human_strength_factor(self.faction_difficulty(f)) } else { v as f32 };
        let halved = |allies: &[FactionId], r: RegionId, k: i32| allies.iter().fold(0u32, |s, &x| s.wrapping_add(st.get(r, x).wrapping_mul(k) as u32 >> 1));
        let ratio_log = |num: f32, den: u32| peace_ratio_clamped(num / (den as f32).max(1.0)).log10();
        let regions_of = |f: FactionId| {
            let cap = self.world.capital(f);
            self.world.regions.values().filter(move |r| r.owner == f).map(move |r| (r.id, if Some(r.id) == cap { 2 } else { 1 }))
        };
        let (enemies, ai_allies, p_allies) = (believed_enemies(self, ai), believed_allies(self, ai), believed_allies(self, p));
        let own_mean = peace_weighted_mean(
            regions_of(ai)
                .map(|(r, k)| {
                    let own = (st.get(r, ai).wrapping_mul(k) as u32).wrapping_add(halved(&ai_allies, r, k));
                    let theirs = enemies.iter().fold(0i32, |acc, &e| cvttss2si(acc as f32 + scaled(st.get(r, e).wrapping_mul(k), e)));
                    ratio_log(theirs as f32, own)
                })
                .collect(),
        );
        let their_mean = peace_weighted_mean(
            regions_of(p)
                .map(|(r, k)| {
                    let side = (cvttss2si(scaled(st.get(r, p).wrapping_mul(k), p)) as u32).wrapping_add(halved(&p_allies, r, k));
                    ratio_log(st.get(r, ai).wrapping_mul(k) as f32, side)
                })
                .collect(),
        );
        let rel = self.world.relationships.get(&(ai, p));
        peace_value(&PeaceValueInputs {
            base,
            rank_log,
            own_mean,
            their_mean,
            war_turns: rel.map_or(0, |r| r.war_turns),
            momentum: rel.map_or(0, |r| r.war_momentum),
            turns_since_battle: rel.map_or(0, |r| r.turns_since_battle),
            proposer_human: p_human,
            proposer_difficulty: self.faction_difficulty(p),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn float_to_int_casts_give_the_integer_indefinite_out_of_range() {
        // cvttss2si: 0x80000000 for a value past the int range or NaN, not a saturated value.
        assert_eq!(super::super::commands::cvttss2si(-2.9), -2);
        assert_eq!(super::super::commands::cvttss2si(f32::NAN), i32::MIN);
        assert_eq!(super::super::commands::cvttss2si(3.0e9), i32::MIN);
        let v = DealValue { gain: 1, cost: 2, given: 3_000_000_000 };
        assert_eq!(human_demand_scaled(v, 1), DealValue { given: 1u32 << 31, ..v }, "3e9 does not fit an int");
        assert_eq!(human_demand_scaled(DealValue { given: 100, ..v }, 3).given, 225);
        // NaN truncates to 0x80000000 (not a saturated 0 or i32::MAX): an odd slot keeps only that bit.
        assert_eq!(DealValue { gain: 1, cost: 2, given: 3 }.inflated(f32::NAN), DealValue { gain: 0x8000_0000, cost: 0, given: 0x8000_0000 });
    }

    #[test]
    fn the_peace_value_pieces_follow_the_traced_formula() {
        let s: Vec<u8> = [-1.0, -0.75, -0.5, -0.25, 0.0, 0.2, 0.25, 0.74, 0.75, 1.0].into_iter().map(peace_attitude_state).collect();
        assert_eq!(s, [0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
        assert_eq!([0, 1, 2, 3, 4].map(peace_base_pair), [(2500, 6000), (2500, 6000), (5000, 3000), (10000, 1500), (10000, 1500)]);
        assert_eq!([-2, -1, 0, 1, 7].map(human_strength_factor), [1.0, 1.1, 1.2, 1.3, 1.0]);
        assert_eq!([-2, -1, 0, 1, 7].map(human_rank_boost), [0.0, 0.33, 0.67, 1.0, 0.0]);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        // Ranks: 1 below 3 → 1.8; 3 over 1 + 1 → 1 / 1.4; 0 below 5 → 3 (the cap).
        assert!(close(peace_rank_log(1, 3, 0.0), 1.8f32.log10()));
        assert!(close(peace_rank_log(3, 1, 1.0), (1.0f32 / 1.4).log10()));
        assert!(close(peace_rank_log(0, 5, 0.0), 3.0f32.log10()));
        assert_eq!(peace_ratio_clamped(0.1), f32::from_bits(0x3EAA_7EFA));
        // Largest first: weights 4, 3 over 7; one region weighs 3 over 3.
        assert!(close(peace_weighted_mean(vec![1.0, 3.0]), 15.0 / 7.0));
        assert!(close(peace_weighted_mean(vec![0.5]), 0.5));
        assert_eq!(peace_weighted_mean(Vec::new()), 0.0);
        // Each term is divided before it is added (0x00A35712): 5/12 × 0.3 + 4/12 × 0.2 + 3/12 × 0.1
        // gives 0x3E5DDDDE in f32, where adding first and dividing once gives 0x3E5DDDDD.
        assert_eq!(peace_weighted_mean(vec![0.1, 0.2, 0.3]).to_bits(), 0x3E5D_DDDE);
        // The war-length factor jumps from 3.33 at 4 turns to 0 at 5 (faithful, 0x00A35C67), then
        // 4 × log2(t / 5), back above 3.33 only at 9.
        assert!(close(peace_turn_factor(0), 2.0 / 3.0) && close(peace_turn_factor(4), 10.0 / 3.0));
        assert_eq!(peace_turn_factor(5), 0.0);
        assert!(peace_turn_factor(8) < peace_turn_factor(4) && peace_turn_factor(9) > peace_turn_factor(4));
        assert!(close(peace_turn_factor(10), 4.0) && close(peace_turn_factor(20), 8.0));
        assert!(close(peace_momentum_factor(2, true), 1.15) && close(peace_momentum_factor(20, false), 1.75));
        assert!(close(peace_momentum_factor(-200, true), 0.25) && close(peace_momentum_factor(-10, false), 0.90625));
    }

    #[test]
    fn the_peace_value_follows_the_traced_formula() {
        let base = PeaceValueInputs {
            base: (5000, 3000),
            rank_log: 0.0,
            own_mean: 0.0,
            their_mean: 0.0,
            war_turns: 0,
            momentum: 0,
            turns_since_battle: 0,
            proposer_human: false,
            proposer_difficulty: 0,
        };
        // Even sides, no war yet: A = 5000, B = 10000, X = 3000, tf = 2/3; no turns past the offset.
        assert_eq!(peace_value(&base), DealValue { gain: 3333, cost: 6666, given: 3000 });
        // 10 turns (tf = 4), 3 since a battle, momentum 2 (m = 1.15), a human proposer of difficulty
        // 1 (k 60, no offset): A' = 0.85 × 5000, B' = 11500, v = 4250 + 13 × 60.
        let i = PeaceValueInputs { war_turns: 10, turns_since_battle: 3, momentum: 2, proposer_human: true, proposer_difficulty: 1, ..base };
        assert_eq!(peace_value(&i), DealValue { gain: 5030 * 4, cost: 46000, given: 11730 });
        // Long enough and v reaches B': the AI asks what it gives up.
        let v = peace_value(&PeaceValueInputs { base: (2500, 6000), war_turns: 100, turns_since_battle: 200, ..base });
        assert_eq!(v.gain, v.cost);
        // A strong AI (own mean high) asks more than it is worth when it is not ahead.
        let v = peace_value(&PeaceValueInputs { own_mean: 0.477, ..base });
        assert!(v.gain >= v.cost);
    }

    #[test]
    fn technology_value_follows_the_traced_formula() {
        // cost 1000: 1000^1.1 = 1995.26..., × 10 = 19952 (truncated), + 500 = 20452.
        let v = technology_value(1000, 3, 0, true, true);
        assert_eq!(v, DealValue { gain: 20452, cost: 0, given: 0 });
        // Only one faction has it: doubled. Traded once by the proposer: ÷ 4.
        assert_eq!(technology_value(1000, 1, 1, true, true).gain, 20452 * 2 / 4);
        // Demanded by a human proposer: no division; both cost slots.
        assert_eq!(technology_value(1000, 3, 1, false, true), DealValue { gain: 0, cost: 20452, given: 20452 });
        // Demanded by an AI proposer: divided.
        assert_eq!(technology_value(1000, 3, 1, false, false).cost, 20452 / 4);
    }

    #[test]
    fn a_fair_trade_passes_and_a_lopsided_demand_fails() {
        // Technology for technology, both worth 20452, no goal weight: cost ≤ 1.05 × gain.
        let mut e = DealEvaluation::default();
        e.add(5, technology_value(1000, 3, 0, true, true), 0.0, 1.0);
        e.add(5, technology_value(1000, 3, 0, false, true), 0.0, 1.0);
        assert!(e.fair());
        assert!(!e.good(), "an even trade is not twice as good");
        // A cheap technology (cost 100: 500 + trunc(10 × 158.48) = 2084) for a dear one.
        let mut e = DealEvaluation::default();
        e.add(5, technology_value(100, 3, 0, true, true), 0.0, 1.0);
        e.add(5, technology_value(1000, 3, 0, false, true), 0.0, 1.0);
        assert_eq!(e.value, DealValue { gain: 2084, cost: 20452, given: 20452 });
        assert!(!e.fair());
        // A goal weight of 1 on the technology record adds 500 to the bonus.
        let mut e = DealEvaluation::default();
        e.add(5, DealValue { gain: 1000, cost: 1500, given: 1500 }, 1.0, 1.0);
        assert_eq!(e.bonus, 500.0);
        assert!(e.fair() && !e.good());
        assert_eq!(e.eagerness, 1000.0);
    }

    #[test]
    fn technology_goal_weights_follow_the_traced_clamps() {
        // 40 factions: t = 5. Nobody has it: −1; two of five: 2 × 0.4 − 1; five or more: 1.
        assert_eq!(technology_spread(0, 40), -1.0);
        assert!((technology_spread(2, 40) - (-0.2)).abs() < 1e-6);
        assert_eq!(technology_spread(9, 40), 1.0);
        // Fewer than 8 factions: t = 1.
        assert_eq!(technology_spread(1, 3), 1.0);
        // The AI gives: at most −0.5; it receives: at most 0.5; both at least −1.
        assert_eq!(given_technology_weight(1.0), -0.5);
        assert_eq!(given_technology_weight(-0.8), -0.8);
        assert_eq!(given_technology_weight(-3.0), -1.0);
        assert_eq!(received_technology_weight(0.9), 0.5);
        assert_eq!(received_technology_weight(0.25), 0.25);
        assert_eq!(received_technology_weight(-2.0), -1.0);
    }

    #[test]
    fn research_need_follows_the_traced_formula() {
        assert_eq!((need_from_enemies(0), need_from_enemies(3), need_from_enemies(12)), (1, 8, 256));
        assert_eq!((need_from_allies(2, 5), need_from_allies(5, 2), need_from_allies(12, 0)), (1, 8, 256));
        assert_eq!((need_power(8), need_power(9)), (256, 256));
        // need = [4 + 16, 4 + 16, 8 + max(2, 4)] = [20, 20, 12], h = 52 >> 1 = 26.
        let v = research_need_values(8, 4, 16, 2, 4);
        assert_eq!(v, [20.0 * (1.0 / 26.0), 20.0 * (1.0 / 26.0), 12.0 * (1.0 / 26.0)]);
        // A need above h counts as h: need = [2, 2, 256 + 256], h = 516 >> 1 = 258.
        let v = research_need_values(256, 1, 1, 256, 0);
        assert_eq!(v, [2.0 * (1.0 / 258.0), 2.0 * (1.0 / 258.0), 258.0 * (1.0 / 258.0)]);
    }

    #[test]
    fn a_record_sums_the_goal_weights_of_its_items() {
        let items = DealItems { demanded: vec!['c', 'd'], offered: vec!['a', 'b'] };
        // 'a' is in list 1 (the AI wants it), 'c' in list 2 (the AI would give it).
        let received = |t: &char| (*t == 'a').then_some(0.25);
        let given = |t: &char| (*t == 'c').then_some(REGION_GOAL_WEIGHT);
        assert_eq!(record_goal_weight(&items, received, given), -0.75);
        // Swapped sides do not count: list 1 is only matched against what the proposer offers.
        let swapped = DealItems { demanded: vec!['a'], offered: vec!['c'] };
        assert_eq!(record_goal_weight(&swapped, received, given), 0.0);
    }

    #[test]
    fn economic_need_follows_the_traced_loop() {
        let rec = |income: i32, expenses: i32| {
            let mut r = [0i32; 25];
            r[5] = income;
            r[19] = expenses;
            r
        };
        // Two records or fewer: 16.
        assert_eq!(need_from_economy(&[rec(100, 0), rec(100, 0)]), 16);
        // Five records (the newest three count): I = 300, X = 0, s = trunc(600 × 0.125) = 75,
        // target 2I − X = 600: eight steps reach 600, k = 0 → 1.
        let h = vec![rec(9999, 0), rec(9999, 0), rec(100, 0), rec(100, 0), rec(100, 0)];
        assert_eq!(need_from_economy(&h), 1);
        // Expenses equal to income: target = I = 300, four steps of 75 → k = 4 → 16.
        let h = vec![rec(0, 0), rec(0, 0), rec(100, 100), rec(100, 100), rec(100, 100)];
        assert_eq!(need_from_economy(&h), 16);
        // Spending above twice the income: target ≤ 0, no step, k = 8 → 256.
        let h = vec![rec(0, 0), rec(0, 0), rec(100, 300), rec(100, 300), rec(100, 300)];
        assert_eq!(need_from_economy(&h), 256);
        // Only the newest five records count (a longer history).
        let mut long = vec![rec(0, 10_000); 5];
        long.extend([rec(0, 0), rec(0, 0), rec(100, 100), rec(100, 100), rec(100, 100)]);
        assert_eq!(need_from_economy(&long), 16);
    }

    #[test]
    fn technology_category_is_the_key_prefix() {
        assert_eq!(technology_category("admin1_public_schooling"), Some(0));
        assert_eq!(technology_category("economy2_joint_stock_company"), Some(1));
        assert_eq!(technology_category("military5_rockets"), Some(2));
        assert_eq!(technology_category("mod_navy_tech"), None);
    }

    #[test]
    fn inflation_takes_the_first_net_once() {
        let mut i = DealInflation::default();
        assert_eq!((i.first, i.factor), (0, 1.0));
        i.round_end(1000);
        assert_eq!((i.first, i.factor), (1000, 1.0));
        i.round_end(2500);
        assert_eq!((i.first, i.factor), (1000, 2.5));
        // A negative net is read as unsigned (kept 1:1): about 4·10⁹ / 1000 → 3.
        i.round_end(-5);
        assert_eq!(i.factor, 3.0);
        // A negative first net (the Spain start) leaves the factor at 1 for any later net.
        let mut s = DealInflation::default();
        s.round_end(-18880);
        assert_eq!(s.first, 4_294_948_416);
        s.round_end(50_000);
        assert_eq!(s.factor, 1.0);
        s.round_end(-10_000);
        assert!(s.factor >= 1.0 && s.factor < 1.001);
    }

    #[test]
    fn inflation_is_clamped_and_truncated_when_applied() {
        assert_eq!(deal_inflation(5000, 0), 3.0);
        assert_eq!(deal_inflation(500, 1000), 1.0);
        assert_eq!(deal_inflation(1800, 1000), 1.8);
        let v = DealValue { gain: 10, cost: 20, given: 30 };
        assert_eq!(v.inflated(1.8), v, "1.8 counts as 1");
        assert_eq!(v.inflated(2.5), DealValue { gain: 20, cost: 40, given: 60 });
    }

    #[test]
    fn the_attitude_factor_keeps_its_fraction() {
        // ORIGINAL BUG 0x00A8D250: the exe truncated the factor first (2 at −100, 1 from −99 to 0,
        // 0 above 0). Ours: 1 at 0, linear to 0.5 at +100 and 2.0 at −100 (clamped to ±100),
        // truncated after the multiply.
        let scaled = |v: u32, att: i32| attitude_scaled(v, att, JOIN_WAR_FACTOR_AT_BEST, JOIN_WAR_FACTOR_AT_WORST);
        for (att, v) in [(-500, 2000), (-100, 2000), (-50, 1500), (0, 1000), (50, 750), (100, 500), (500, 500)] {
            assert_eq!(scaled(1000, att), v, "attitude {att}");
        }
        assert_eq!(scaled(3, 50), 2, "2.25 truncates");
        assert_eq!(scaled(100, 1), 99, "99.5 truncates");
        assert_eq!(scaled(50, -1), 50, "50.5 truncates");
        assert_eq!(scaled(u32::MAX, -100), u32::MAX.wrapping_mul(2), "wraps as the exe's 32-bit multiply");
        // The endpoints are arguments: swapped, a negative attitude scales down.
        assert_eq!(attitude_scaled(1000, -50, 2.0, 0.5), 750);
        assert_eq!(attitude_scaled(1000, 50, 2.0, 0.5), 1500);
    }

    #[test]
    fn the_join_war_weights_and_offsets_follow_the_constants() {
        assert_eq!(ASKED_JOIN_WAR_WEIGHT, -1.0);
        assert_eq!([0, 1, 2, 3, 4, 5].map(join_war_state_offset), [0.5, 0.25, 0.0, -0.25, -0.5, 0.0]);
        // Below −1: −1; else at most the cap.
        assert_eq!(given_join_war_weight(-1.5, -0.1), -1.0);
        assert_eq!(given_join_war_weight(-1.0, -0.1), -1.0);
        assert_eq!(given_join_war_weight(-0.7, -0.25), -0.7);
        assert_eq!(given_join_war_weight(0.8, -0.5), -0.5);
        assert_eq!(given_join_war_weight(-0.05, -0.1), -0.1);
    }
}

//! The AI recipient's value of a proposed deal (`analysis/ai/AI_RESEARCH.md` §4 "Deal
//! evaluation"): the parts of the original's evaluator that are traced. The evaluator itself
//! (`0x00AA5ED0`, run by `CCQ_DIPLOMACY_PROPOSE_DEAL` → `0x00C49BE0` when the recipient is not
//! human), for the records the model holds. Ported here: the technology value, the deal inflation,
//! the evaluation sum and its accept tests, the goal weights of region and technology items (the
//! research need, the technology spread) and how a record sums them, the region value
//! (`0x00C131C0` → `0x00A364B0` → `0x00AA1E90`, then `0x00C4D140`), and the answer to a deal of
//! regions and technologies ([`CampaignModel::ai_accepts_deal`]).
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
    /// - enemies / allies: the other factions in the game (faction `+0x824` clear,
    ///   [`Self::in_the_game`]), neither of them the rebels, whose stance towards `faction` is war
    ///   (stance 0) / allied (stance 2) (`0x00ABAFB0` lists `+0x40` / `+0x2C`); CONFIRMED;
    /// - economy: [`need_from_economy`] of its history; CONFIRMED;
    /// - `p1` (`0x00A64460`): `(forts in its regions >> 1) + (its land forces with units >> 2)`
    ///   (CAI region `+0x15C` = the count of `CAI_REGION` #6, the forts `0x00C140C0` adds to the
    ///   region at the fort's position; a mobile's `+0x168` = land (`0x00C15370` ← force virtual
    ///   `+0x38`), `+0x160` = its unit count; CONFIRMED);
    /// - `p2` (`0x00A8D6E0`): `(port slots in its regions >> 1) + (its navies with units >> 2)`
    ///   (CAI region #3 entries whose object has a port `+0x1E4`, INFERRED: the region's port
    ///   slots, the model's [`super::world::RegionSlot::port`]).
    pub fn research_need(&self, faction: FactionId) -> [f32; 3] {
        let (mut enemies, mut allies) = (0u32, 0u32);
        if !self.is_rebel_faction(faction) {
            for &other in self.world.factions.keys() {
                if other == faction || self.is_rebel_faction(other) || !self.in_the_game(other) {
                    continue;
                }
                match self.world.stance(other, faction) {
                    super::world::Stance::War => enemies += 1,
                    super::world::Stance::Allied => allies += 1,
                    _ => {}
                }
            }
        }
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
    /// the records the model holds: 4 regions, then 5 technology): `None` when there is nothing to
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
    /// - [`DealEvaluation::add`] of the record's value ([`Self::region_deal_value`] /
    ///   [`Self::technology_deal_value`] at the campaign's inflation factor), scale 1.0.
    ///
    /// Then accept when `fair` and the diplomatic budget is at least the payment (0 here), or
    /// `good` and the payment is at most the recipient's treasury.
    ///
    /// PROVISIONAL: the budget (`0x00AAF570`, the finance pot `+0x140` = diplomatic spending bias ×
    /// treasury at its refresh, never negative until the AI's own paid diplomatic intentions (kind
    /// 8, `0x00CC53D0`) spend from it, which our AI does not have) is taken as at least 0; a deal
    /// the AI would not accept is declined where the exe may first make a counter-offer
    /// (`0x00CC58C0`, fewer than 10 per negotiation); the records the model does not hold (trade,
    /// payments, ...) are not evaluated.
    pub fn ai_accepts_deal(&self) -> Option<bool> {
        use super::negotiation::NegotiationAction;
        let n = self.negotiations.current.as_ref()?;
        let (ai, proposer) = (n.recipient, n.proposer);
        let has_regions = !(n.regions.offered.is_empty() && n.regions.demanded.is_empty());
        let has_techs = !(n.technologies.offered.is_empty() && n.technologies.demanded.is_empty());
        if self.is_human(ai) || !(has_regions || has_techs) {
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
                // The AI's research need does not change between items.
                let need = self.research_need(ai);
                record_goal_weight(
                    &n.technologies,
                    |t| (allowed && self.is_technology_goal(t, proposer, ai)).then(|| Self::received_technology_goal_weight(t, &need)),
                    |t| (allowed && self.is_technology_goal(t, ai, proposer)).then(|| self.given_technology_goal_weight(t)),
                )
            };
            if !step(NegotiationAction::Technology, &weight, &|| self.technology_deal_value(n, inflation)) {
                return Some(false);
            }
        }
        let payment = 0;
        let budget_ok = true;
        let treasury = self.world.factions.get(&ai).map_or(0, |f| f.treasury);
        Some((e.fair() && budget_ok) || (e.good() && payment <= treasury))
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
    ///   INFERRED a siege), else to slot 0 when the AI's attitude to the region (`0x00A79050`) is
    ///   at least 0, or at least −5 with the region bordering one of the AI's
    ///   ([`super::World::region_neighbours`]), else nowhere; each region the proposer demands adds
    ///   its worth to the proposer to slot 1 (unless either test holds) and its worth to the AI to
    ///   slot 2;
    /// - the sum times the inflation factor ([`DealValue::inflated`]);
    /// - [`human_demand_scaled`] with `m` = the demanded regions plus
    ///   [`super::World::deal_regions_received`] when the proposer is human (0 otherwise).
    ///
    /// PROVISIONAL: the two tests read false (sieges are not in the model; `+0x22C` is UNKNOWN);
    /// the attitude (`0x00A79050`: the lower of two population-class results from `0x008BDB90`,
    /// minus 6, or minus `floor((1 − x) × 6)` when the region is in the AI's list `+0x50/+0x54`) is
    /// not ported and reads 0, so every offered region counts as gain; the deal's peace record is
    /// not in the model, so `m` treats it as empty.
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
        // PROVISIONAL: the attitude `0x00A79050` is not ported.
        let attitude = |_: RegionId| 0;
        let mut v = DealValue::default();
        for &r in &n.regions.offered {
            let a = attitude(r);
            if a >= 0 || (a >= -5 && borders_ai(r)) {
                v.gain = v.gain.wrapping_add(self.region_deal_worth_with(r, ai, &ai_census));
            }
        }
        for &r in &n.regions.demanded {
            v.cost = v.cost.wrapping_add(self.region_deal_worth_with(r, proposer, &proposer_census));
            v.given = v.given.wrapping_add(self.region_deal_worth_with(r, ai, &ai_census));
        }
        let m = if self.is_human(proposer) {
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
}

//! Region population: the seven growth factors, the round-end growth and the one-round projection the
//! region details panel shows (CAMPAIGN_FIDELITY.md §Population; every rule CONFIRMED from the exe unless
//! tagged).
//!
//! The exe keeps a population object at region +0x28 (saved as `POPULATION`); its `REGION_FACTORS` part
//! (region +0x4C, saved as `POPULATION/REGION_FACTORS`) holds the live population (+0x30, #2), the
//! capacity (+0x34, #3) and its base (+0x38, #4), the seven growth factors in percent (+0x04..+0x1C, #0),
//! their total (+0x3C, #5), the trend of the last growth (+0x40, #6: 1 up, 2 unchanged, 3 down), the
//! overcrowded flag (+0x44, #7), the last round's migrants (+0x58, #8), the classes and the religion
//! breakdown.
//!
//! - **Factors** (`0x00AA9C10`, run by the round start `0x008F1F60` → `0x00AAE710` and by the round end's
//!   first pass `0x008E1030` → `0x00AB3E70` for every region before any faction's economy; also on a copy
//!   for the panel): see [`growth_factors`]. A rebel-owned region (`0x008CEEF0`) has all factors 0.
//! - **Growth** (`0x00AB4070`, from the round-end economy `0x008BC650` → `0x00AB42F0` → `0x00AB3FF0`):
//!   see [`grow`]; the religion conversion follows on the grown population, then the town wealth.
//! - **Projection** (`0x00A727D0`, the region info table and the region details panel): see
//!   [`project`].
//!
//! Migration (`0x00A44DB0`): each region of a faction offers a share of its population to the others,
//! weighted by `0x00A89770`, which is 0 for a region in its owner's home theatre (`0x00A8B5C0`), and with
//! no weight anywhere nobody moves. Every region of the model counts as in its owner's home theatre (the
//! model has no theatres; every shipped map is one theatre, see `economy::tax_bonuses`), so migration is
//! always 0 here: the migrant counts, factor 5 and the faction table are not computed (BACKLOG §0-E,
//! with theatres).

use super::effects::EffectSet;
use super::ids::RegionId;
use super::mod_state::ModWrites;
use super::world::{CampaignModel, Region};

/// The growth factors' keys in their slots (`0x00A88F20`: the table at `0x01458B04`; rows of
/// `public_order_factors`, which give each its picture and tooltip).
pub const FACTOR_KEYS: [&str; 7] = [
    "population_growth_base",
    "population_growth_buildings",
    "population_growth_taxes",
    "population_growth_military",
    "population_growth_food_shortages",
    "population_growth_migration",
    "population_growth_ports",
];

/// The original's population cap (`0x00AB42C1`: 200,000,000, compared unsigned).
pub const POPULATION_CAP: u32 = 200_000_000;

/// The share of capacity above which a region is overcrowded (`0x01325CEC`).
const OVERCROWDED: f32 = 0.9;

/// A region's population state (`REGION_FACTORS`, region +0x4C; the population itself is
/// [`Region::population`]).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PopulationState {
    /// #0: the growth factors in percent, in the slots of [`FACTOR_KEYS`], each a multiple of 0.01.
    pub factors: [f32; 7],
    /// #3: the capacity (+0x34).
    pub capacity: u32,
    /// #4: the capacity before effects (+0x38).
    pub base_capacity: u32,
    /// #5: the growth in percent per round, the factors' sum (+0x3C).
    pub growth: f32,
    /// #6: the last growth's direction (+0x40): 1 up, 2 unchanged, 3 down (a new state holds 2).
    pub trend: u32,
    /// #7: the population is above 90% of the capacity (+0x44).
    pub overcrowded: bool,
    /// #8: the last round's net migrants (+0x58).
    pub migrants: i32,
}

/// What the population factors read besides the region (`0x00AA9C10`'s arguments).
pub struct FactorInputs<'a> {
    /// The region's effect set (`0x00A67530`, or the predicted set `0x00A9B930` for the panel).
    pub set: &'a EffectSet,
    /// The units of hostile armies standing in the region ([`hostile_units`]).
    pub hostile_units: u32,
}

/// `0x00AA6830`: a percentage rounded to hundredths, [`as_hundredths`] then × 0.01 (`0x00A88EF0`, the
/// product rounded to single precision when stored).
pub fn hundredths(x: f32) -> f32 {
    (f64::from(0.01f32) * f64::from(as_hundredths(x))) as f32
}

/// `0x00A98AD0`: a percentage as whole hundredths (what the factor pips carry): x / 0.01 stored as a
/// float, then rounded half to even (FISTP).
pub fn as_hundredths(x: f32) -> i32 {
    ((f64::from(x) / f64::from(0.01f32)) as f32).round_ties_even() as i32
}

/// The units of hostile armies standing in the region (`0x00AAAFD0`): characters in the region's area
/// (`0x00A87A00` → `0x00BA6840`) commanding a force, of another faction than the owner that is the rebels
/// or at war with it (`0x008CE9B0`); each adds its force's unit count. 0 for a rebel-owned region.
pub fn hostile_units(model: &CampaignModel, reg: &Region) -> u32 {
    if model.is_rebel_faction(reg.owner) {
        return 0;
    }
    model
        .world
        .forces
        .values()
        .filter(|f| !f.is_navy && f.faction != reg.owner && model.at_war(reg.owner, f.faction))
        .filter(|f| f.commander.and_then(|c| model.world.characters.get(&c)).is_some_and(|c| super::economy::in_region(model, reg, c)))
        .map(|f| f.units.len() as u32)
        .sum()
}

/// The growth factors and capacity of a region (`0x00AA9C10`; `pop` is the population they are for):
/// - capacity = round(base × (1 + `maxpop_modifier`% + `pop_maxpop_modifier_tech_mod`%)); overcrowded
///   when pop / capacity > 0.9;
/// - 0 base: `baseline_pop_growth`; 1 buildings: `pop_growth_tech` + `pop_growth_farm`; 6 ports:
///   `pop_growth_port_fishing`; 3 military: −0.05 × the hostile units;
/// - 2 taxes: `pop_growth_tax_modifier` + `pop_growth_tax_modifier_multiplier` × (0 + 1 + 6);
/// - 4 food shortages, when overcrowded with s = 0 + 1 + 6 and r = pop / capacity: (1 − r) × 20 − s at
///   or above the capacity, else (0.9 − r) × 10 × s;
/// - 5 migration: 0 (set only on the panel's copy, see [`project`]);
///
/// each rounded to hundredths ([`hundredths`]); the growth is their sum. A rebel-owned region has every
/// factor 0 and keeps its capacity and growth.
pub fn growth_factors(model: &CampaignModel, reg: &Region, pop: u32, prev: &PopulationState, input: &FactorInputs<'_>) -> PopulationState {
    if model.is_rebel_faction(reg.owner) {
        return PopulationState { factors: [0.0; 7], overcrowded: false, ..prev.clone() };
    }
    let set = input.set;
    let fx = |b: &str| set.get(b);
    let add = |a: f32, b: f32| (f64::from(a) + f64::from(b)) as f32;
    // Single precision throughout (SSE), the tech modifier's product stored as a float first.
    let tech = (f64::from(fx("pop_maxpop_modifier_tech_mod")) * f64::from(0.01f32)) as f32;
    let scale = fx("maxpop_modifier") * 0.01 + tech + 1.0;
    let capacity = (prev.base_capacity as f32 * scale).round_ties_even() as u32;
    let ratio = pop as f32 / capacity as f32;
    let overcrowded = ratio > OVERCROWDED;
    let mut f = [0.0f32; 7];
    f[0] = hundredths(model.rules.var("baseline_pop_growth", 0.0));
    f[3] = hundredths(input.hostile_units as f32 * -0.05);
    f[1] = hundredths(add(fx("pop_growth_tech"), fx("pop_growth_farm")));
    f[6] = hundredths(fx("pop_growth_port_fishing"));
    // `0x00A6ABC0`: buildings + base + ports, summed in the x87 registers and stored as a float.
    let s = (f64::from(f[1]) + f64::from(f[0]) + f64::from(f[6])) as f32;
    let multiplied = (f64::from(fx("pop_growth_tax_modifier_multiplier")) * f64::from(s)) as f32;
    f[2] = hundredths(add(fx("pop_growth_tax_modifier"), multiplied));
    if overcrowded {
        f[4] = hundredths(if ratio >= 1.0 { (1.0 - ratio) * 20.0 - s } else { (OVERCROWDED - ratio) * 10.0 * s });
    }
    PopulationState { factors: f, capacity, overcrowded, growth: total(&f), ..prev.clone() }
}

/// `0x00A6B8C0`: the factors' sum (base + buildings + ports, then food, taxes, military, migration).
pub fn total(f: &[f32; 7]) -> f32 {
    (f64::from(f[1]) + f64::from(f[0]) + f64::from(f[6]) + f64::from(f[4]) + f64::from(f[2]) + f64::from(f[3]) + f64::from(f[5])) as f32
}

/// The population after one round of growth (`0x00AB4070`): pop + migrants + round(pop × growth% ),
/// raised to `minimum_population` and capped at [`POPULATION_CAP`], with the trend it gives (1 up,
/// 2 unchanged, 3 down; compared before the cap). A rebel-owned region does not grow (growth 0, trend
/// kept). The original's rule of the seam `population.grow` ([`super::seams`]): the model and the panel
/// call the rule in use, never this directly.
pub fn grow(model: &CampaignModel, reg: &Region, pop: u32, state: &PopulationState) -> (u32, PopulationState) {
    let mut next = state.clone();
    next.migrants = 0;
    if model.is_rebel_faction(reg.owner) {
        next.growth = 0.0;
        return (pop, next);
    }
    next.growth = total(&state.factors);
    // `pop × growth × 0.01` in single precision, the population read unsigned (`0x00AB4227`: CVTDQ2PD with the
    // 2^32 fix-up), then FISTP (round half to even; the integer indefinite out of range).
    let change = super::commands::fistp((pop as f32 * next.growth) * 0.01f32);
    let mut new = (pop as i32).wrapping_add(next.migrants).wrapping_add(change);
    let minimum = model.rules.var("minimum_population", 0.0);
    if minimum > new as f32 {
        new = minimum as i32;
    }
    let delta = new.wrapping_sub(pop as i32);
    next.trend = if delta == 0 { 2 } else if delta < 0 { 3 } else { 1 };
    (((new as u32).min(POPULATION_CAP)), next)
}

/// What recruiting costs a region's population (CONFIRMED, CAMPAIGN_FIDELITY.md §Recruitment cost and money):
/// campaign variable 37 `recruitment_population_cost` and variable 36 `minimum_population_after_recruitment`,
/// each read as the exe reads a campaign variable (`0x008B25F0`: FISTP of the f32 value). Every rule works on
/// the live population ([`Region::population`], `REGION_FACTORS` #2, region +0x7C) in signed 32-bit arithmetic,
/// as the exe does. The exe keeps that field as 32 bits that only these rules read signed: the growth
/// (`0x00AB4227`) and the factors (`0x00AA9D44`, `0x00AA9F91`) read it unsigned, and the cap compares it unsigned
/// (`0x00AB42CB`). So a result below 0 (only with a mod's negative variables) is stored as the same bits in
/// the `u32`, and every reader sees what the exe's readers see. Both variables are 0 in the shipped
/// `campaign_variables`, so with vanilla data a recruit is never refused for its population and takes and gives
/// back nothing; a mod may set them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecruitmentPopulation {
    /// Var 37: what one queued recruit takes from the region's population.
    pub cost: i32,
    /// Var 36: the population a recruit must leave behind.
    pub minimum: i32,
}

impl RecruitmentPopulation {
    /// The two variables of `rules` (0 when the table lacks the row).
    pub fn of(rules: &super::rules::CampaignRules) -> Self {
        let var = |key| super::commands::fistp(rules.var(key, 0.0));
        Self { cost: var("recruitment_population_cost"), minimum: var("minimum_population_after_recruitment") }
    }

    /// The gate (`HasRecruitmentPopulationAvailable` `0x00A89550`, from the entry flags `0x00B69BA0`, which
    /// set flag 4 when it fails): the population is at least cost + minimum (signed). It does not depend on
    /// the unit: every entry of the region's land and naval lists gets the same answer.
    pub fn available(self, pop: u32) -> bool {
        self.cost.wrapping_add(self.minimum) <= pop as i32
    }

    /// The population after queueing one recruit (`ChargeRecruitablePopulation` `0x00AAF190`, called by the
    /// queue command `0x00B58DD0` right after the money): less the cost when [`Self::available`], else set to
    /// the minimum. The command refuses an entry that fails the gate first, so it always takes the first branch.
    pub fn charged(self, pop: u32) -> u32 {
        if self.available(pop) { (pop as i32).wrapping_sub(self.cost) as u32 } else { self.minimum as u32 }
    }

    /// The population after `items` queued recruits leave the queue untrained (`CreditRecruitablePopulation`
    /// `0x00A61AA0`, called once per item by `CancelRecruitmentItem` `0x00B1A820`: the cancel command, the
    /// turn start's removal of items the region can no longer recruit, and the queues a capture empties,
    /// with or without the money back): the cost back per item, whatever the charge took. A trained item gives
    /// nothing back (the queue step destroys it without the cancel path), nor does disbanding the unit.
    pub fn credited(self, pop: u32, items: usize) -> u32 {
        (0..items).fold(pop as i32, |p, _| p.wrapping_add(self.cost)) as u32
    }
}

/// `0x00AA4860`: shares below 0.0005 become 0 and the rest are scaled to sum to 1 (left as they are
/// when they already do); with nothing left the first religion takes everything. ORIGINAL BUG: the
/// exe writes that first share even when the breakdown is empty (a write through a null list, a
/// crash for a region without religions); an empty breakdown stays empty here.
pub fn normalise_religions(religions: &mut [(String, f32)]) {
    let mut sum = 0.0f32;
    for (_, share) in religions.iter_mut() {
        if *share >= 0.0005 {
            sum += *share;
        } else {
            *share = 0.0;
        }
    }
    if sum == 1.0 {
        return;
    }
    if sum != 0.0 {
        for (_, share) in religions.iter_mut() {
            *share *= 1.0 / sum;
        }
    } else if let Some((_, first)) = religions.first_mut() {
        *first = 1.0;
    }
}

/// The predicted effect set of a region (`0x00A9B930`): the region's set, then (for an owner that is
/// not the rebels) the owner's technologies being researched (`0x008AFCF0` over the technology tree:
/// the nodes whose researching school, node +0xC, is set by `0x008EEC90` and cleared on completion by
/// `0x008EED20`), the faction-wide change of every building under construction in the owner's regions
/// (`0x008BF110` → `0x00A798F0`: the new level's set added, the slot's present building's removed) and
/// the local change of this region's own constructions (`0x00A67490`).
pub fn predicted_effect_set(model: &CampaignModel, reg: &Region) -> EffectSet {
    let mut set = super::economy::region_effect_set(model, reg);
    if model.is_rebel_faction(reg.owner) {
        return set;
    }
    set.merge(&predicted_faction_change(model, reg.owner));
    construction_change(&mut set, reg, model.rules.effects.building_local());
    set
}

/// The faction part of [`predicted_effect_set`]'s change: `faction`'s technologies under research
/// (`0x008AFCF0`) and the faction-wide change of the buildings under construction in its regions
/// (`0x008BF110`). Empty for the rebels (`0x008CEEF0`).
pub fn predicted_faction_change(model: &CampaignModel, faction: super::ids::FactionId) -> EffectSet {
    let mut set = EffectSet::default();
    if model.is_rebel_faction(faction) {
        return set;
    }
    let rules = &model.rules.effects;
    if let Some(d) = model.world.faction_details.get(&faction) {
        for (tech, t) in &d.research {
            if t.researcher != 0
                && let Some(s) = rules.technology().get(tech)
            {
                set.merge(s);
            }
        }
    }
    for r in model.world.regions.values().filter(|r| r.owner == faction) {
        construction_change(&mut set, r, rules.building_factionwide());
    }
    set
}

/// Every construction of `r` ([`Region::construction_changes`]): its level's set added and the
/// replaced building's taken away (`0x00A67490` / `0x00A798F0`), from `sets` (local or faction-wide).
fn construction_change(set: &mut EffectSet, r: &Region, sets: &std::collections::BTreeMap<String, EffectSet>) {
    for (_, level, old) in r.construction_changes() {
        if let Some(new) = sets.get(level) {
            set.merge(new);
        }
        if let Some(old) = old.and_then(|b| sets.get(&b.level_key)) {
            set.subtract(old);
        }
    }
}

/// The panel's one-round projection of a region (`0x00A727D0`).
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    /// The factors now (`0x00A72990`: a copy refreshed with the region's set), with the migration factor
    /// set to the projected migrants' share.
    pub current: PopulationState,
    /// The religion breakdown now, normalised by that refresh (`0x00AA4860`).
    pub current_religions: Vec<(String, f32)>,
    /// The population after the round's growth.
    pub population: u32,
    /// The factors after the round (refreshed with the predicted set, [`predicted_effect_set`]) and the
    /// growth's trend.
    pub predicted: PopulationState,
    /// The religion breakdown after the round's conversion, normalised by the refresh.
    pub religions: Vec<(String, f32)>,
    /// The predicted set the projection's public order reads.
    pub set: EffectSet,
}

/// The projection of `region` (`0x00A727D0`): the current factors (`0x00A72990`: refreshed with the
/// region's set, religions normalised), one growth step and the conversion on a copy (`0x00AB4070` with
/// the region's set), the copy's factors refreshed with the predicted set ([`predicted_effect_set`];
/// religions normalised), then the migration factor of both set to migrants × 100 / population (0
/// here, see the module) and their totals recomputed (`0x00AAA010`, `0x00AB3EA0`). `None` for a region
/// not in the campaign.
pub fn project(model: &CampaignModel, region: RegionId) -> Option<Projection> {
    let reg = model.world.regions.get(&region)?;
    let rebel = model.is_rebel_faction(reg.owner);
    let set = super::economy::region_effect_set(model, reg);
    let hostile = hostile_units(model, reg);
    let mut current = growth_factors(model, reg, reg.population, &reg.population_state, &FactorInputs { set: &set, hostile_units: hostile });
    let mut current_religions = reg.religions.clone();
    if !rebel {
        normalise_religions(&mut current_religions);
    }
    // The growth rule in use (seam `population.grow`); a projection drops the rule's mod-state writes.
    let (population, grown) = model.rules.seams.population_grow.rule()(model, reg, reg.population, &current, &mut ModWrites::default());
    let mut religions = current_religions.clone();
    let flows = model.conversion_flows_for(reg, population, &religions, &set);
    super::religion::apply_conversion(&mut religions, &flows, population);
    let predicted_set = predicted_effect_set(model, reg);
    let mut predicted = growth_factors(model, reg, population, &grown, &FactorInputs { set: &predicted_set, hostile_units: hostile });
    if !rebel {
        normalise_religions(&mut religions);
    }
    for s in [&mut current, &mut predicted] {
        s.factors[5] = hundredths(0.0);
        s.growth = total(&s.factors);
    }
    Some(Projection { current, current_religions, population, predicted, religions, set: predicted_set })
}

impl CampaignModel {
    /// Every region's population factors refreshed (`0x00AA9C10` with the region's set, then the religion
    /// breakdown normalised, `0x00AA4860`): the round start (`0x008F1F60` → `0x00AAE710`) and the round
    /// end's first pass (`0x008E1030` → `0x00AB3E70`) run it for every faction's regions.
    pub fn refresh_population_factors(&mut self) {
        let updates: Vec<(RegionId, PopulationState)> = self
            .world
            .regions
            .values()
            .map(|r| {
                let set = super::economy::region_effect_set(self, r);
                let input = FactorInputs { set: &set, hostile_units: hostile_units(self, r) };
                (r.id, growth_factors(self, r, r.population, &r.population_state, &input))
            })
            .collect();
        for (id, state) in updates {
            let rebel = self.world.regions.get(&id).is_some_and(|r| self.is_rebel_faction(r.owner));
            if let Some(r) = self.world.regions.get_mut(&id) {
                if !rebel {
                    normalise_religions(&mut r.religions);
                }
                r.population_state = state;
            }
        }
    }

    /// The round end's population step of `faction`'s regions (`0x00AB42F0` → `0x00AB3FF0` → `0x00AB4070`):
    /// each grows by its stored factors, then its religions convert on the new population (`0x00A63FE0`).
    /// The growth is the rule in use (seam `population.grow`, [`grow`] in vanilla); its mod-state writes
    /// apply before the next region grows.
    pub fn population_round_end(&mut self, faction: super::ids::FactionId) {
        let regions: Vec<RegionId> = self.world.regions.values().filter(|r| r.owner == faction).map(|r| r.id).collect();
        let mut writes = ModWrites::default();
        for id in regions {
            let Some(reg) = self.world.regions.get(&id) else { continue };
            let (pop, state) = self.rules.seams.population_grow.rule()(self, reg, reg.population, &reg.population_state, &mut writes);
            if let Some(r) = self.world.regions.get_mut(&id) {
                r.population = pop;
                r.population_state = state;
            }
            self.mod_state.apply(&mut writes);
            self.convert_region(id);
        }
    }
}

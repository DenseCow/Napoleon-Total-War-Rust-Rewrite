//! Region population: the seven growth factors, the round-end growth and the one-round projection the
//! region details panel shows (CAMPAIGN_FIDELITY.md §Population; every rule CONFIRMED from the exe unless
//! tagged).
//!
//! The exe keeps a population object at region +0x28 (saved as `POPULATION`); its `REGION_FACTORS` part
//! (region +0x4C, saved as `POPULATION/REGION_FACTORS`) holds the live population (+0x30, #2), the
//! capacity (+0x34, #3) and its base (+0x38, #4), the seven growth factors in percent (+0x04..+0x1C, #0),
//! their total (+0x3C, #5), the trend of the last growth (+0x40, #6: 1 up, 2 unchanged, 3 down), the
//! overcrowded flag (+0x44, #7), the last round's migrants (+0x58, #8), the policing cost (+0x5C, #9), the
//! classes and the religion breakdown; the population object's +0x14 is the region's policing tracker (saved as
//! `POPULATION` #4, [`PopulationState::policing_booked`]).
//!
//! - **Factors** (`0x00AA9C10`, run for the regions of every faction in the game except the rebels by the
//!   round start `0x008F1F60` → `0x00AAE710` and by the round end's first pass `0x008E1030` → `0x00AB3E70`
//!   before any faction's economy, see [`CampaignModel::refresh_population_factors`]; also on a copy for the
//!   panel): see [`growth_factors`]. A rebel-owned region (`0x008CEEF0`) has all factors 0.
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

use std::collections::BTreeSet;

use super::effects::EffectSet;
use super::ids::{FactionId, RegionId};
use super::mod_state::ModWrites;
use super::world::{CampaignModel, PopulationClassState, Region};

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
    /// #9: the policing cost (+0x5C, [`policing_cost`]), written by every refresh of the factors.
    #[cfg_attr(feature = "serde", serde(default))]
    pub policing_cost: i32,
    /// The amount the region's policing tracker has booked into its owner's running economics record
    /// (expense category 21, `0x00BBE930` group 3; CONFIRMED): the tracker at the population object's
    /// +0x14 ({+4 owner, +8 amount}, `0x00A2C9F0`), saved as `POPULATION` #4 (the loader `0x00A456D0`
    /// reads it after `REGION_FACTORS` and the three u32 and sets it through `0x00A51230`; CONFIRMED in
    /// `orig_fr_may1811`: France's regions sum to its record's category 21, 2177). The round end's first
    /// pass sets it to the policing cost ([`CampaignModel::prepare_round_end`]); an owner change moves it
    /// from the old owner's record to the new one's (`0x00AAF220`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub policing_booked: i32,
}

/// The policing cost of a region (`0x00AA9C10` at its end, stored at `REGION_FACTORS` +0x5C; CONFIRMED by
/// disassembly): 0 unless the owner is human (faction +0x6E0; an AI owner pays none, and a rebel-owned
/// region is zeroed by the rebel branch), else `FISTP(var × Σ × √pop × max(0, policing_cost_mod × 0.01 +
/// 1))` in single precision, where `var` is campaign variable 18 `policing_automated_cost_per_pop`, Σ the
/// automated policing (repression slot 3, class +0x4C) of the classes that are the owner's government's
/// upper or lower class (`0x008CE320` / `0x008CE260`), refreshed just before in the same pass
/// (`0x008EDF70` with the object it refreshes: `input`'s set, classes and religions), and `pop` the
/// population the factors are for.
pub fn policing_cost(model: &CampaignModel, reg: &Region, input: &FactorInputs<'_>, pop: u32) -> i32 {
    if model.is_rebel_faction(reg.owner) || !model.is_human(reg.owner) {
        return 0;
    }
    let set = input.set;
    let (upper, lower) = super::economy::government_classes(model, reg.owner);
    let classes = super::economy::public_order_factors_with(model, reg, set, input.religions, input.classes, super::economy::garrison_units(model, reg));
    let policing: i32 = classes.iter().filter(|c| c.class == upper || c.class == lower).map(|c| c.repression[3]).sum();
    // The modifier through the x87 registers (the f32 constant 0.01 at 0x01318034, + 1.0), stored as a
    // float and clamped at 0 (MAXSS); the product in single precision, then FISTP.
    let modifier = ((f64::from(set.get("policing_cost_mod")) * f64::from(0.01f32) + 1.0) as f32).max(0.0);
    let per_pop = model.rules.var("policing_automated_cost_per_pop", 0.0);
    super::commands::fistp(per_pop * policing as f32 * (pop as f32).sqrt() * modifier)
}

/// What the population factors read besides the region (`0x00AA9C10`'s arguments and the population
/// object it refreshes: the region's own, or the panel's projected copy, [`project`]).
pub struct FactorInputs<'a> {
    /// The region's effect set (`0x00A67530`, or the predicted set `0x00A9B930` for the panel).
    pub set: &'a EffectSet,
    /// The units of hostile armies standing in the region ([`hostile_units`]).
    pub hostile_units: u32,
    /// The object's classes (`REGION_FACTORS` +0x2C; [`Region::class_bases`], or the projected copy's).
    pub classes: &'a [PopulationClassState],
    /// The object's religion breakdown ([`Region::religions`], or the projected copy's after its
    /// conversion), as the public order reads it before the refresh normalises it (`0x00AA4860`).
    pub religions: &'a [(String, f32)],
}

impl<'a> FactorInputs<'a> {
    /// The inputs of `reg`'s own population object: `set` and its classes and religions.
    pub fn of(model: &CampaignModel, reg: &'a Region, set: &'a EffectSet) -> Self {
        Self { set, hostile_units: hostile_units(model, reg), classes: &reg.class_bases, religions: &reg.religions }
    }
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
/// each rounded to hundredths ([`hundredths`]); the growth is their sum; then the [`policing_cost`]. A
/// rebel-owned region has every factor and its policing cost 0 and keeps its capacity and growth.
pub fn growth_factors(model: &CampaignModel, reg: &Region, pop: u32, prev: &PopulationState, input: &FactorInputs<'_>) -> PopulationState {
    if model.is_rebel_faction(reg.owner) {
        return PopulationState { factors: [0.0; 7], overcrowded: false, policing_cost: 0, ..prev.clone() };
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
    let policing_cost = policing_cost(model, reg, input, pop);
    PopulationState { factors: f, capacity, overcrowded, growth: total(&f), policing_cost, ..prev.clone() }
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

/// What moves a population class's war factor ([`PopulationClassState::war`], class +0x20): the kind
/// `0x008A94D0` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassWarEvent {
    /// Kinds 0..=5, a battle result's step (`0x008ACCF0`, for each side's capital): the factor gains
    /// [`BATTLE_WAR_CHANGE`]`[kind]`. PROVISIONAL: never called: the battle results' step `0x008ACCF0`
    /// (from `0x008F7880` at `0x008F7B7C`) is not modelled, the battle results lack the victory type it reads.
    Battle(u8),
    /// Kind 6: the region's owner declared war on the other faction (`0x00B27070` → `0x00A63490`).
    WarDeclared,
}

/// The war factor's change per battle-result kind (the i32 table `0x014579F0`, CONFIRMED by reading
/// memory): kinds 0 and 1 for the loser (result codes 13/14 and 15/16), 2 and 3 for the winner (8/9 and
/// 10/11), 4 and 5 for the loser and the winner of the battles `0x008ACCF0` tests first.
pub const BATTLE_WAR_CHANGE: [i32; 6] = [-1, -2, 1, 2, -4, 4];

/// The war factor's range (`0x008A94D0` clamps after every change: below −15 to −15, above 6 to 6).
pub const CLASS_WAR_RANGE: (i32, i32) = (-15, 6);

/// A class's war factor after one event (`0x008A94D0`, CONFIRMED by decompilation): `owner` is the
/// region's owner, `other` the other side (the war's target, the battle's opponent). Nothing changes when
/// either is the rebels (`IsFactionWithoutRecord` `0x008CEEF0`); otherwise the value is clamped to
/// [`CLASS_WAR_RANGE`] after the change, whatever it was. A war declaration
/// ([`ClassWarEvent::WarDeclared`]) reads the attitude category of `owner` to `other` (`0x00B0DBA0`) and the
/// two governments' ranks ([`CampaignRules::government_ranks`](super::rules::CampaignRules::government_ranks),
/// `government_types` #3): with `lower` = the class is `owner`'s government's lower class (`0x008CE230`),
/// `upper` = its upper class (`0x008CE2F0`), `own` / `theirs` the ranks:
/// - hostile (0): +4 if (`lower` and `own` ≤ `theirs`) or (`upper` and `theirs` ≤ `own`);
/// - friendly (3): −3, very friendly (4): −6, if (`upper` and `own` ≤ `theirs`) or (`lower` and `theirs` ≤ `own`);
/// - otherwise nothing.
///
/// The original's rule of the seam `population.class_war` ([`super::seams`]): the model calls the rule in
/// use, never this directly.
pub fn class_war_change(model: &CampaignModel, class: &str, event: ClassWarEvent, owner: FactionId, other: FactionId, war: i32) -> i32 {
    if model.is_rebel_faction(owner) || model.is_rebel_faction(other) {
        return war;
    }
    let change = match event {
        ClassWarEvent::Battle(kind) => {
            debug_assert!(usize::from(kind) < BATTLE_WAR_CHANGE.len(), "battle war kind {kind}");
            BATTLE_WAR_CHANGE.get(usize::from(kind)).copied().unwrap_or(0)
        }
        ClassWarEvent::WarDeclared => {
            let (upper, lower) = super::economy::government_classes(model, owner);
            let rank = |f: FactionId| {
                model.world.factions.get(&f).and_then(|x| model.rules.government_ranks.get(&x.government_key)).copied().unwrap_or(0)
            };
            let (own, theirs) = (rank(owner), rank(other));
            let (lower, upper) = (class == lower, class == upper);
            match model.attitude_category(owner, other) {
                0 if (lower && own <= theirs) || (upper && theirs <= own) => 4,
                3 if (upper && own <= theirs) || (lower && theirs <= own) => -3,
                4 if (upper && own <= theirs) || (lower && theirs <= own) => -6,
                _ => 0,
            }
        }
    };
    war.saturating_add(change).clamp(CLASS_WAR_RANGE.0, CLASS_WAR_RANGE.1)
}

/// One class's round-end pass (`0x008F9690`'s first part, CONFIRMED by decompilation; the riot checks
/// that follow it are not modelled): the war factor one step toward 0, a negative resistance base +1, a
/// positive education base −2 (1 or 2 become 0). (Also, not stored here: a negative bankruptcy factor,
/// class +0x28, +1 and a positive repression slot 5, +0x54, −1; the model keeps neither, both are 0.)
pub fn class_round_step(class: &mut PopulationClassState) {
    class.war -= class.war.signum();
    if class.resistance < 0 {
        class.resistance += 1;
    }
    if class.education > 0 {
        class.education = if class.education < 3 { 0 } else { class.education - 2 };
    }
}

/// A region's classes after its round-end passes (`GrowRegionPopulation` `0x00AB4070`, CONFIRMED by
/// disassembly; each pass is [`class_round_step`]). A rebel owner: one pass per class. Otherwise three
/// passes, on the first class that is the owner's government's lower class (`0x00A60FE0`), on the first
/// that is its upper class (`0x00A61320`) and on the first that is neither (`0x00A89C90`); a find with no
/// match takes the first class, so with fewer than three classes that one has more than one pass. No
/// classes: nothing. The original's rule of the seam `population.class_round` ([`super::seams`]).
pub fn class_round(model: &CampaignModel, reg: &Region) -> Vec<PopulationClassState> {
    let mut classes = reg.class_bases.clone();
    if classes.is_empty() {
        return classes;
    }
    if model.is_rebel_faction(reg.owner) {
        classes.iter_mut().for_each(class_round_step);
        return classes;
    }
    let (upper, lower) = super::economy::government_classes(model, reg.owner);
    let find = |classes: &[PopulationClassState], pick: &dyn Fn(&str) -> bool| classes.iter().position(|c| pick(&c.class)).unwrap_or(0);
    let passes = [
        find(&classes, &|c| c == lower),
        find(&classes, &|c| c == upper),
        find(&classes, &|c| c != upper && c != lower),
    ];
    for i in passes {
        class_round_step(&mut classes[i]);
    }
    classes
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

/// The smallest religion share [`normalise_religions`] keeps (`0x00AA4860`).
const RELIGION_SHARE_MIN: f32 = 0.0005;

/// The sum of the shares [`normalise_religions`] keeps, added in order as `0x00AA4860` does.
fn kept_religion_sum(religions: &[(String, f32)]) -> f32 {
    religions.iter().map(|(_, s)| *s).filter(|s| *s >= RELIGION_SHARE_MIN).fold(0.0f32, |a, s| a + s)
}

/// `0x00AA4860`: shares below 0.0005 become 0 and the rest are scaled to sum to 1 (left as they are
/// when they already do); with nothing left the first religion takes everything. ORIGINAL BUG: the
/// exe writes that first share even when the breakdown is empty (a write through a null list, a
/// crash for a region without religions); an empty breakdown stays empty here.
pub fn normalise_religions(religions: &mut [(String, f32)]) {
    let sum = kept_religion_sum(religions);
    for (i, (_, share)) in religions.iter_mut().enumerate() {
        *share = normalised_entry(*share, sum, i);
    }
}

/// Entry `i`'s share after [`normalise_religions`], without changing the breakdown (`i` in range).
pub fn normalised_share(religions: &[(String, f32)], i: usize) -> f32 {
    normalised_entry(religions[i].1, kept_religion_sum(religions), i)
}

/// The one rule of `0x00AA4860` for entry `i` holding `share`, given the [`kept_religion_sum`]
/// `sum`: nothing kept, the first entry takes everything; a share below the minimum (or NaN) is 0;
/// the rest are scaled by `1 / sum` unless the sum is exactly 1.
fn normalised_entry(share: f32, sum: f32, i: usize) -> f32 {
    if sum == 0.0 {
        if i == 0 { 1.0 } else { 0.0 }
    } else if share.is_nan() || share < RELIGION_SHARE_MIN {
        0.0
    } else if sum == 1.0 {
        share
    } else {
        share * (1.0 / sum)
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
pub fn predicted_faction_change(model: &CampaignModel, faction: FactionId) -> EffectSet {
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
    /// The classes after the round's passes on the copy (`0x00AB4070` runs them on the copy before its
    /// growth; seam `population.class_round`): the next round's classes, which the predicted factors read.
    pub classes: Vec<PopulationClassState>,
    /// The predicted set the projection's public order reads.
    pub set: EffectSet,
}

/// The projection of `region` (`0x00A727D0`): the current factors (`0x00A72990`: refreshed with the
/// region's set, religions normalised), the classes' round-end passes, one growth step and the conversion
/// on a copy (`0x00AB4070` with the region's set; the passes are the seam `population.class_round`, CONFIRMED
/// by disassembly: `0x00A727D0` calls `0x00AB4070` with the copy at `0x00A72844`, and each of its class passes
/// `0x008F9690` takes a class of that copy), the copy's factors (with its classes and converted religions,
/// `0x00AA9C10` → `0x008EDF70`) refreshed with the predicted set ([`predicted_effect_set`];
/// religions normalised), then the migration factor of both set to migrants × 100 / population (0
/// here, see the module) and their totals recomputed (`0x00AAA010`, `0x00AB3EA0`). `None` for a region
/// not in the campaign.
pub fn project(model: &CampaignModel, region: RegionId) -> Option<Projection> {
    let reg = model.world.regions.get(&region)?;
    let rebel = model.is_rebel_faction(reg.owner);
    let set = super::economy::region_effect_set(model, reg);
    let own = FactorInputs::of(model, reg, &set);
    let mut current = growth_factors(model, reg, reg.population, &reg.population_state, &own);
    let mut current_religions = reg.religions.clone();
    if !rebel {
        normalise_religions(&mut current_religions);
    }
    // The class and growth rules in use (seams `population.class_round`, `population.grow`), in the round
    // end's order; a projection drops the rules' mod-state writes.
    let classes = model.rules.seams.population_class_round.rule()(model, reg, &mut ModWrites::default());
    let (population, grown) = model.rules.seams.population_grow.rule()(model, reg, reg.population, &current, &mut ModWrites::default());
    let mut religions = current_religions.clone();
    let flows = model.conversion_flows_for(reg, population, &religions, &set);
    super::religion::apply_conversion(&mut religions, &flows, population);
    let predicted_set = predicted_effect_set(model, reg);
    let input = FactorInputs { set: &predicted_set, hostile_units: own.hostile_units, classes: &classes, religions: &religions };
    let mut predicted = growth_factors(model, reg, population, &grown, &input);
    if !rebel {
        normalise_religions(&mut religions);
    }
    for s in [&mut current, &mut predicted] {
        s.factors[5] = hundredths(0.0);
        s.growth = total(&s.factors);
    }
    Some(Projection { current, current_religions, population, predicted, religions, classes, set: predicted_set })
}

impl CampaignModel {
    /// The population factors of every region owned by a faction that is not the rebels and is still in the
    /// game refreshed (`0x00AA9C10` with the region's set, then the religion breakdown normalised,
    /// `0x00AA4860`). Both passes that run it walk the same regions (disassembly, CONFIRMED):
    /// - the round start, `0x008F1F60` → `0x00AAE710` per owned region, skips the rebels
    ///   (`IsFactionWithoutRecord` `0x008CEEF0`) and a faction with +0x824 (out of the game) set;
    /// - the round end's first pass, `0x00948CF0` → `0x008E1030` → `0x00AB3E70` per owned region, runs only for
    ///   the listed factions (world +0x2C / +0x30, which never hold the rebels at +0x1C) and inside
    ///   `0x008E1030` only when +0x824 is clear.
    ///
    /// So the rebels' and out-of-game factions' regions keep their factors through both.
    pub fn refresh_population_factors(&mut self) {
        // In any order: both passes walk the faction region list (`0x008F1FB3`, `0x008E1040`), but each region's
        // factors read only that region's own state, which no other region's refresh writes.
        // The faction set once per refresh: `in_the_game` may scan every region and character (old saves).
        let listed: BTreeSet<FactionId> =
            self.world.factions.keys().copied().filter(|&f| self.in_the_game_with_record(f)).collect();
        let updates: Vec<(RegionId, PopulationState)> = self
            .world
            .regions
            .values()
            .filter(|r| listed.contains(&r.owner))
            .map(|r| {
                let set = super::economy::region_effect_set(self, r);
                let input = FactorInputs::of(self, r, &set);
                (r.id, growth_factors(self, r, r.population, &r.population_state, &input))
            })
            .collect();
        // No owner here is the rebels, so every breakdown is normalised (`0x00AA9C10` skips it only for them).
        for (id, state) in updates {
            if let Some(r) = self.world.regions.get_mut(&id) {
                normalise_religions(&mut r.religions);
                r.population_state = state;
            }
        }
    }

    /// The round end's population step of `faction`'s regions (`0x00AB42F0` → `0x00AB3FF0` → `0x00AB4070`):
    /// each region's classes take their round-end passes first (seam `population.class_round`, [`class_round`]
    /// in vanilla), then it grows by its stored factors, then its religions convert on the new population
    /// (`0x00A63FE0`). The growth is the rule in use (seam `population.grow`, [`grow`] in vanilla). Each rule's
    /// mod-state writes apply before the next rule runs.
    ///
    /// The regions in the faction's region list order (`0x008BC68E`–`0x008BC6C3` walks +0x778 / +0x77C,
    /// CONFIRMED; [`super::world::World::faction_regions_in_list_order`]). The vanilla rules read only their
    /// own region, so the order shows only through a mod rule's state writes, which the next region's rule sees.
    pub fn population_round_end(&mut self, faction: FactionId) {
        let regions = self.world.faction_regions_in_list_order(faction);
        let mut writes = ModWrites::default();
        for id in regions {
            let Some(reg) = self.world.regions.get(&id) else { continue };
            let classes = self.rules.seams.population_class_round.rule()(self, reg, &mut writes);
            if let Some(r) = self.world.regions.get_mut(&id) {
                r.class_bases = classes;
            }
            self.mod_state.apply(&mut writes);
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

    /// Gives `region` its government's two classes ([`super::economy::government_class_bases`] of `faction`,
    /// the one place that rule lives) when it has none (a region loaded without `POPULATION_CLASS` entries:
    /// every region of the original's files has them, `0x008861E0`). What a class change needs to have
    /// something to change.
    pub fn ensure_population_classes(&mut self, region: RegionId, faction: FactionId) {
        if self.world.regions.get(&region).is_some_and(|r| r.class_bases.is_empty()) {
            let classes = super::economy::government_class_bases(self, faction);
            if let Some(r) = self.world.regions.get_mut(&region) {
                r.class_bases = classes;
            }
        }
    }

    /// `region`'s classes' war factor moved by `event` against `other` (`0x00A633F0`, reached through the thunk
    /// `0x00A63490`, CONFIRMED by disassembly): only while the region is its owner's capital (`0x008B3790`
    /// compared with the region); then each class (`0x00A63430`) takes the rule in use (seam
    /// `population.class_war`, [`class_war_change`] in vanilla) with the region's owner and `other`.
    pub fn move_class_war(&mut self, region: RegionId, event: ClassWarEvent, other: FactionId) {
        let Some(owner) = self.world.regions.get(&region).map(|r| r.owner) else { return };
        if !self.world.is_capital(region) {
            return;
        }
        self.ensure_population_classes(region, owner);
        let Some(reg) = self.world.regions.get(&region) else { return };
        let mut writes = ModWrites::default();
        let rule = self.rules.seams.population_class_war.rule();
        let war: Vec<i32> = reg.class_bases.iter().map(|c| rule(self, &c.class, event, owner, other, c.war, &mut writes)).collect();
        if let Some(r) = self.world.regions.get_mut(&region) {
            for (c, w) in r.class_bases.iter_mut().zip(war) {
                c.war = w;
            }
        }
        self.mod_state.apply(&mut writes);
    }
}

//! The effects system: what technologies, buildings, traits, ancillaries, ministers, government types
//! and the difficulty give a faction, a region or a character, summed the way the original does.
//! Spec: `analysis/fidelity/EFFECTS_FIDELITY.md` (slot 0-F; the exe's store and sums §1–§2).
//!
//! - The original keeps every effect sum in a sorted container keyed by (bonus type, qualifiers,
//!   bonus); a missing key reads 0 ([`EffectSet`]). Every DB effect key reaches its bonus through the
//!   `effect_bonus_value_*_junction` tables; a key with no junction row has no effect (CONFIRMED: the
//!   DB-load compiler only writes mapped keys).
//! - [`EffectRules`] holds each source's compiled set (built from the DB by `ntw_campaign`).
//! - [`Effects::compute`] builds the sums for a model:
//!   - **character** = Σ its traits' current levels + Σ its ancillaries (character +0x48C: CONFIRMED as
//!     the set the attribute getters read; its builder, INFERRED `0x009CDF10`, sums the trait-level
//!     sets and a second list's sets, taken to be the ancillaries);
//!   - **faction** (faction +0x6FC, `0x008B16C0`) = +0x8D4 (the saved base +0x8C4 and the difficulty
//!     handicap, [`apply_start_handicaps`]) + Σ owned regions' faction-wide
//!     building effects (buildings at health ≥ 100) + researched technologies + government type +
//!     every ministerial post holder (his post's level effects + his character set);
//!   - **region** (`0x00A67530`) = its buildings' local effects (health ≥ 100) + its owner's faction
//!     set (+ the governor's sets outside the home theatre: never the case in the shipped campaigns, not
//!     modelled, PROVISIONAL).
//!
//! Query with [`Effects::faction`], [`Effects::region`], [`Effects::character`], [`Effects::character_total`] and the qualified
//! forms. Keys are the engine bonus names (`effect_bonus_value_basic_junction` column 2).

use std::collections::BTreeMap;

use super::ids::{CharacterId, FactionId, RegionId};
use super::world::CampaignModel;

/// The bonus types the junction tables map to (the first word of the exe's key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BonusKind {
    /// No qualifier (`effect_bonus_value_basic_junction`; exe type 1).
    Basic,
    /// Qualified by a unit category (`_unit_category_junction`; exe type 0xE).
    UnitCategory,
    /// Qualified by a unit class (`_unit_class_junction`).
    UnitClass,
    /// Qualified by a population class (`_population_class_junction`; exe type 6).
    PopClass,
    /// Qualified by an agent type (`_agent_junction`; exe type 0).
    Agent,
    /// Qualified by a religion (`_religion_junction`; exe type 7): `conversion` per religion.
    Religion,
    /// A saved entry of another exe bonus type (the number); its bonus is the raw id as text.
    Saved(u32),
}

/// One entry key: the bonus kind, the engine bonus name and its qualifier ("" for basic).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectKey {
    /// Which junction the key comes from.
    pub kind: BonusKind,
    /// The engine bonus name.
    pub bonus: String,
    /// The qualifier (unit category, class, population class, agent); "" for basic.
    pub qualifier: String,
}

impl EffectKey {
    /// A basic (unqualified) key.
    pub fn basic(bonus: &str) -> Self {
        Self { kind: BonusKind::Basic, bonus: bonus.to_string(), qualifier: String::new() }
    }
}

/// A sum of effects (the exe's container: sorted keys, values added on merge, missing = 0).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectSet {
    /// The summed values.
    pub values: BTreeMap<EffectKey, f32>,
}

impl EffectSet {
    /// Adds `value` under `key`.
    pub fn add(&mut self, key: EffectKey, value: f32) {
        *self.values.entry(key).or_insert(0.0) += value;
    }

    /// Adds every entry of `other` (the exe's merge, `0x00E04760`).
    pub fn merge(&mut self, other: &EffectSet) {
        for (k, v) in &other.values {
            self.add(k.clone(), *v);
        }
    }

    /// A basic bonus (0 when absent).
    pub fn get(&self, bonus: &str) -> f32 {
        self.get_qualified(BonusKind::Basic, bonus, "")
    }

    /// A qualified bonus (0 when absent).
    pub fn get_qualified(&self, kind: BonusKind, bonus: &str, qualifier: &str) -> f32 {
        self.values
            .iter()
            .find(|(k, _)| k.kind == kind && k.bonus == bonus && k.qualifier.eq_ignore_ascii_case(qualifier))
            .map_or(0.0, |(_, v)| *v)
    }

    /// The int form the exe uses in integer formulas (`0x00E23E10`: rounded half to even).
    pub fn get_int(&self, bonus: &str) -> i32 {
        f64::from(self.get(bonus)).round_ties_even() as i32
    }

    /// No entries.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// The engine bonus names by id (the table at `0x0145B4A8`, CONFIRMED; ids 0..183). Saved basic
/// entries (type 1) carry the id.
pub const BONUS_NAMES: [&str; 184] = ["agent_research_slots", "building_prestige", "commodity_export_vol", "education_happy_mod", "gdp_farm", "gdp_industry", "gdp_mine", "gdp_mod_all", "gdp_port_trade", "happiness_active_lower_gov_type", "happiness_active_lower_tax", "happiness_active_upper_gov_type", "happiness_active_upper_tax", "happiness_character_trait_or_ancillary", "happiness_events_factional", "happiness_events_regional", "happiness_ministerial_position", "happiness_mod_religious_unrest", "happy_war_results_home_region", "maxpop_modifier", "mod_research_rate", "naval_recruitment_points", "policing_cost_mod", "pop_growth_farm", "pop_growth_government", "pop_growth_port_fishing", "pop_growth_tax_modifier", "pop_growth_tax_modifier_multiplier", "pop_growth_tech", "pop_maxpop_modifier_tech_mod", "recruitment_mod_cost_land_all", "recruitment_mod_cost_naval_all", "recruitment_points", "recruitment_points_home_region", "region_turns_to_surrender", "repression_gov_building", "repression_gov_type", "repression_ministers", "repression_policing_cap", "research_points", "research_points_military", "research_points_industry", "research_points_enlightenment", "research_rate_mod", "research_rate_mod_army_tech", "research_rate_mod_navy_tech", "tax_bonus_building", "tax_bonus_character", "tax_cap", "tech_prestige", "trade_route_all_mod_growth_rate", "tw_growth", "tw_growth_education", "tw_growth_government", "tw_growth_home_region", "tw_growth_industry", "tw_growth_mod_all", "tw_growth_port", "tw_growth_roads", "tw_growth_tax_modifier", "tw_growth_tax_modifier_fixed", "tw_growth_technologies", "upkeep_cost_mod_land_all", "upkeep_cost_mod_naval_all", "promote_general_in_field", "promote_admiral_at_sea", "trade_route_cap_land", "trade_route_cap_sea", "enable_gov_republic", "enable_gov_constitutional_monarchy", "enable_top_gallants", "charge_bonus_for_bayonet_units", "mod_ship_movement_campaign", "mod_ship_movement_campaign_top_gallants", "mod_ship_movement_battle", "mod_ship_movement_battle_top_gallants", "mod_reload_land", "mod_misfire_land", "mod_reload_naval", "mod_misfire_naval", "campaign_map_stealth", "prestige_faction_leader", "diplomacy_bonus_faction_leader", "sea_command_attack", "sea_command_defence", "land_command_attack", "land_command_defence", "land_command_ambush", "land_command_siege_attack", "land_command_siege_defence", "land_command_america", "land_command_europe", "land_command_india", "management_army", "management_navy", "management_finance", "management_justice", "subterfuge_spying", "subterfuge_assassination", "subterfuge_counterspying", "security_versus_assassination", "morale", "hates_ottomans", "zeal_america", "zeal_europe", "zeal_india", "diplomatic_negotiation", "subterfuge_sabotage", "morale_land", "morale_naval", "tax_bonus_minister", "command_land_artillery", "command_land_cavalry", "command_land_infantry", "command_naval_frigates", "command_naval_ships_of_the_line", "character_death_chance_turn_end", "hates_europeans", "prestige_enlightenment_generic", "prestige_military_generic", "character_recruitment_general_refill_1", "character_recruitment_general_refill_2", "character_recruitment_admiral_refill_1", "character_recruitment_admiral_refill_2", "tax_bonus_technology", "admin_cost_mod", "diplomacy_bonus_enlightenment", "replenishment_percentage_bonus", "general_admiral_action_point_bonus", "gentleman_happiness_bonus", "tw_growth_factionwide", "reinforcement_distance", "line_of_sight_extension", "observation_balloon", "attrition_increase", "looting_increase", "ship_repair_increase", "mod_land_movement_campaign", "ai_region_resistance_modifier", "attrition_difficulty_addition", "guerrilla_reward_units_count_mod", "guerrilla_experience_mod", "guerrilla_replenishment_percentage_bonus", "guerrilla_cost_mod", "guerrilla_upkeep_mod", "auxiliary_experience_mod", "auxiliary_replenishment_percentage_bonus", "auxiliary_cost_mod", "auxiliary_upkeep_mod", "trade_node_supply_mod", "hates_british", "hates_french", "mod_cost", "mod_gdp", "mod_tw_growth", "mod_commodity_production", "production", "demand", "happiness_character", "happiness_clamour_for_reform", "happiness_culture", "happiness_entertainment", "happiness_government", "happiness_industry", "happiness_tax", "happiness", "damage", "reload_mod", "enable", "happiness", "conversion", "production", "enable", "cost_mod", "upkeep_mod", "melee_attack_mod", "movement_mod", "training_mod", "land_command", "sea_command", "cost_mod", "upkeep_mod", "melee_attack_mod", "experience_mod"];

/// One saved effect entry (`CAMPAIGN_BONUS_VALUE`: u32 type, i32 bonus id, f32 value, qualifier).
#[derive(Debug, Clone, Default)]
pub struct SavedBonus {
    /// Bonus type (1 = basic, 0 = agent, ...).
    pub kind: u32,
    /// Bonus id (for type 1 an index of [`BONUS_NAMES`]).
    pub bonus: i32,
    /// The value.
    pub value: f32,
    /// The qualifier (agent type etc.; "" when none).
    pub qualifier: String,
}

// Compared bit for bit (the value is stored exactly), so details holding it stay `Eq`.
impl PartialEq for SavedBonus {
    fn eq(&self, o: &Self) -> bool {
        self.kind == o.kind && self.bonus == o.bonus && self.value.to_bits() == o.value.to_bits() && self.qualifier == o.qualifier
    }
}
impl Eq for SavedBonus {}

impl SavedBonus {
    /// The key this entry adds to: type 1 by its engine name, other types kept raw.
    pub fn key(&self) -> EffectKey {
        match usize::try_from(self.bonus).ok().and_then(|i| BONUS_NAMES.get(i)) {
            Some(name) if self.kind == 1 => EffectKey::basic(name),
            _ => EffectKey { kind: BonusKind::Saved(self.kind), bonus: self.bonus.to_string(), qualifier: self.qualifier.clone() },
        }
    }
}

/// A set from saved entries.
pub fn saved_set(entries: &[SavedBonus]) -> EffectSet {
    let mut s = EffectSet::default();
    for e in entries {
        s.add(e.key(), e.value);
    }
    s
}

/// One trait level: (threshold points, level number, level key).
pub type TraitLevelRow = (i32, i32, String);

/// One tax bundle: its DB rows and their compiled set ([`EffectRules::set_tax_bundle`]).
type TaxBundle = (Vec<(String, f32)>, EffectSet);

/// Each effect source's compiled set (built from the DB by `ntw_campaign::rules_from_db`). The mapping
/// is fixed when the rules are built, and every compiled set is read-only and inserted only through
/// this type's `insert_*` methods, which compile its rows through that mapping: no set can disagree
/// with the mapping.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectRules {
    /// DB effect key → the bonus keys it adds to (one effect may feed several, e.g. all three
    /// population classes). Fixed when the rules are built ([`Self::new`]).
    mapping: BTreeMap<String, Vec<EffectKey>>,
    /// `technology_effects_junction`: a technology's effects on its faction.
    technology: BTreeMap<String, EffectSet>,
    /// `building_effects_junction`: a building level's effects on its own region.
    building_local: BTreeMap<String, EffectSet>,
    /// `taxes_keys` + `taxes_effects_jct`: class key → level key → the tax bundle's rows and their set
    /// compiled through the mapping, as the exe compiles it once at DB load. Set only through
    /// [`Self::set_tax_bundle`], so rows and set always agree.
    tax: BTreeMap<String, BTreeMap<String, TaxBundle>>,
    /// `building_factionwide_effects_junctions`: a building level's effects on its whole faction.
    building_factionwide: BTreeMap<String, EffectSet>,
    /// `government_types_to_effects`.
    government: BTreeMap<String, EffectSet>,
    /// Trait → its levels, sorted by threshold.
    pub trait_levels: BTreeMap<String, Vec<TraitLevelRow>>,
    /// Trait level key → effects.
    trait_level: BTreeMap<String, EffectSet>,
    /// Trait level key → agent attribute bonuses (`trait_attribute_effects`).
    pub trait_attribute: BTreeMap<String, Vec<(String, i32)>>,
    /// Ancillary → agent attribute bonuses (`ancillary_to_attribute_effects`).
    pub ancillary_attribute: BTreeMap<String, Vec<(String, i32)>>,
    /// `ancillary_to_effects`.
    ancillary: BTreeMap<String, EffectSet>,
    /// (post, level 0..9) → effects.
    ministerial: BTreeMap<(String, i32), EffectSet>,
    /// (government type, value) → level modifier (`ministerial_effectiveness_modifiers`).
    pub ministerial_effectiveness: BTreeMap<(String, i32), i32>,
    /// (difficulty −2..2, human list) → effects.
    difficulty: BTreeMap<(i32, bool), EffectSet>,
}

/// The read accessor and the insert method of each compiled-set field of [`EffectRules`]: the set is
/// read-only outside this module and inserted only through the mapping.
macro_rules! compiled_sets {
    ($($field:ident, $insert:ident, $key:ty;)*) => {
        impl EffectRules {
            $(
                #[doc = concat!("The compiled `", stringify!($field), "` sets (read-only; see [`Self::", stringify!($insert), "`]).")]
                pub fn $field(&self) -> &BTreeMap<$key, EffectSet> {
                    &self.$field
                }

                #[doc = concat!("Compiles `rows` (DB effect key, value) through the mapping into the `", stringify!($field), "` set of `key`.")]
                pub fn $insert<'a>(&mut self, key: $key, rows: impl IntoIterator<Item = (&'a str, f32)>) {
                    let set = self.compile(rows);
                    self.$field.insert(key, set);
                }
            )*
        }
    };
}

compiled_sets! {
    technology, insert_technology, String;
    building_local, insert_building_local, String;
    building_factionwide, insert_building_factionwide, String;
    government, insert_government, String;
    trait_level, insert_trait_level, String;
    ancillary, insert_ancillary, String;
    ministerial, insert_ministerial, (String, i32);
    difficulty, insert_difficulty, (i32, bool);
}

impl EffectRules {
    /// Effect rules on `mapping` (DB effect key → bonus keys, the `effect_bonus_value_*_junction`
    /// rows), with no compiled sets yet. The mapping is fixed for the life of the rules: every set is
    /// compiled through it when inserted (the `insert_*` methods, [`Self::set_tax_bundle`]), so all agree.
    pub fn new(mapping: BTreeMap<String, Vec<EffectKey>>) -> Self {
        EffectRules { mapping, ..Default::default() }
    }

    /// Compiles `(effect key, value)` rows into a set through the mapping (the exe's
    /// DB-load compiler `0x00F88590`; unmapped keys add nothing).
    fn compile<'a>(&self, rows: impl IntoIterator<Item = (&'a str, f32)>) -> EffectSet {
        let mut s = EffectSet::default();
        for (effect, value) in rows {
            if let Some(keys) = self.mapping.get(effect) {
                for k in keys {
                    s.add(k.clone(), value);
                }
            }
        }
        s
    }

    /// Does the mapping map DB effect key `effect` (for the tests' check that every key they insert counts)?
    #[cfg(test)]
    pub(crate) fn maps(&self, effect: &str) -> bool {
        self.mapping.contains_key(effect)
    }

    /// Sets the tax bundle of (`class`, `level`) to `rows` and their set compiled through the mapping.
    pub fn set_tax_bundle(&mut self, class: &str, level: &str, rows: Vec<(String, f32)>) {
        let set = self.compile(rows.iter().map(|(e, v)| (e.as_str(), *v)));
        self.tax.entry(class.to_owned()).or_default().insert(level.to_owned(), (rows, set));
    }

    /// The DB rows of a (class key, level key) tax bundle.
    pub fn tax_rows(&self, class: &str, level: &str) -> Option<&Vec<(String, f32)>> {
        self.tax.get(class)?.get(level).map(|(rows, _)| rows)
    }

    /// The compiled set of a (class key, level key) tax bundle.
    pub fn tax_set(&self, class: &str, level: &str) -> Option<&EffectSet> {
        self.tax.get(class)?.get(level).map(|(_, set)| set)
    }

    /// The level a trait is at with `points`: the highest level whose threshold ≤ max(points, 0), none
    /// below the first threshold (CONFIRMED, `0x008B5380`). Not modelled: when
    /// points fall, the level does not drop below the trait's no-going-back level once reached, and a
    /// trait whose points fall below 1 is removed (`0x008D21B0`); the model never lowers trait points yet.
    pub fn trait_level_key(&self, trait_key: &str, points: i32) -> Option<&str> {
        let levels = self.trait_levels.get(trait_key)?;
        levels.iter().rev().find(|(t, _, _)| points >= *t).map(|(_, _, k)| k.as_str())
    }
}

/// The effect sums of a model (see the module docs). Derived state: recompute after anything that
/// changes a source (turn end, research, construction, trait gain, post change).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Effects {
    /// Each faction's sum (faction +0x6FC).
    pub faction: BTreeMap<FactionId, EffectSet>,
    /// Each region's own building effects (health ≥ 100), without the faction part.
    pub region_local: BTreeMap<RegionId, EffectSet>,
    /// Each region's faction-wide building effects (region +0x198).
    pub region_factionwide: BTreeMap<RegionId, EffectSet>,
    /// Each character's traits + ancillaries.
    pub character: BTreeMap<CharacterId, EffectSet>,
    region_owner: BTreeMap<RegionId, FactionId>,
    character_owner: BTreeMap<CharacterId, FactionId>,
}

impl Effects {
    /// Builds every sum for `model` (see the module docs for the composition).
    pub fn compute(model: &CampaignModel) -> Self {
        let rules = &model.rules.effects;
        let w = &model.world;
        let mut fx = Effects::default();
        // Characters: traits by level + ancillaries.
        for (id, d) in &w.character_details {
            let mut s = EffectSet::default();
            for t in &d.traits {
                if let Some(level) = rules.trait_level_key(&t.key, t.points)
                    && let Some(set) = rules.trait_level.get(level)
                {
                    s.merge(set);
                }
            }
            for a in &d.ancillaries {
                if let Some(set) = rules.ancillary.get(a) {
                    s.merge(set);
                }
            }
            fx.character.insert(*id, s);
            if let Some(c) = w.characters.get(id) {
                fx.character_owner.insert(*id, c.faction);
            }
        }
        // Regions: local and faction-wide building effects.
        let gov = w.governing_factions();
        for (id, r) in &w.regions {
            let mut local = EffectSet::default();
            let mut wide = EffectSet::default();
            for b in r.effect_buildings() {
                if let Some(set) = rules.building_local.get(&b.level_key) {
                    local.merge(set);
                }
                if let Some(set) = rules.building_factionwide.get(&b.level_key) {
                    wide.merge(set);
                }
            }
            fx.region_local.insert(*id, local);
            fx.region_factionwide.insert(*id, wide);
            fx.region_owner.insert(*id, gov.get(id).copied().unwrap_or(r.owner));
        }
        // Factions.
        for id in w.factions.keys() {
            let s = faction_set(model, *id, |c| fx.character.get(&c).cloned().unwrap_or_default(), |r| fx.region_factionwide[&r].clone());
            fx.faction.insert(*id, s);
        }
        fx
    }

    /// The part of [`Self::compute`] that one faction's turn reads, at a fraction of the cost: the
    /// regions it owns or governs (local and faction-wide sets), its characters, and the sums of the
    /// faction and of its regions' governing factions. Every query about those gives the same value as
    /// `compute`; queries about other factions' regions or characters give 0.
    pub fn compute_for(model: &CampaignModel, faction: FactionId) -> Self {
        let rules = &model.rules.effects;
        let w = &model.world;
        let gov = w.governing_factions();
        let mut fx = Effects::default();
        let mut factions = std::collections::BTreeSet::from([faction]);
        for (id, r) in &w.regions {
            let g = gov.get(id).copied().unwrap_or(r.owner);
            if r.owner != faction && g != faction {
                continue;
            }
            let mut local = EffectSet::default();
            let mut wide = EffectSet::default();
            for b in r.effect_buildings() {
                if let Some(set) = rules.building_local.get(&b.level_key) {
                    local.merge(set);
                }
                if let Some(set) = rules.building_factionwide.get(&b.level_key) {
                    wide.merge(set);
                }
            }
            fx.region_local.insert(*id, local);
            fx.region_factionwide.insert(*id, wide);
            fx.region_owner.insert(*id, g);
            factions.insert(g);
        }
        for (id, d) in &w.character_details {
            if w.characters.get(id).is_some_and(|c| c.faction == faction) {
                fx.character.insert(*id, character_set(rules, Some(d)));
                fx.character_owner.insert(*id, faction);
            }
        }
        for f in factions.into_iter().filter(|f| w.factions.contains_key(f)) {
            fx.faction.insert(f, Self::faction_sum(model, f));
        }
        fx
    }

    /// A character's effective agent attribute (`0x009C7610`, CONFIRMED shape): the saved level
    /// (`AgentAttributes`), plus — when the level is not −1 (the agent type has it) — the bonuses of his
    /// trait levels (`trait_attribute_effects`) and ancillaries (`ancillary_to_attribute_effects`),
    /// floored at 0. `None` when he does not have the attribute.
    pub fn character_attribute(model: &CampaignModel, c: CharacterId, attribute: &str) -> Option<i32> {
        let rules = &model.rules.effects;
        let d = model.world.character_details.get(&c)?;
        let level = d.attributes.iter().find(|(k, _)| k == attribute).map(|(_, v)| *v)?;
        if level < 0 {
            return Some(level);
        }
        let mut bonus = 0;
        for t in &d.traits {
            if let Some(lk) = rules.trait_level_key(&t.key, t.points) {
                bonus += rules.trait_attribute.get(lk).into_iter().flatten().filter(|(a, _)| a == attribute).map(|(_, v)| *v).sum::<i32>();
            }
        }
        for a in &d.ancillaries {
            bonus += rules.ancillary_attribute.get(a).into_iter().flatten().filter(|(x, _)| x == attribute).map(|(_, v)| *v).sum::<i32>();
        }
        Some(level + bonus.max(0))
    }

    /// One character's own set (traits by level + ancillaries, character +0x48C).
    pub fn character_effects(model: &CampaignModel, c: CharacterId) -> EffectSet {
        character_set(&model.rules.effects, model.world.character_details.get(&c))
    }

    /// One faction's sum (the same as `compute(model).faction[f]`) without computing every other
    /// faction, region and character: for callers that need one faction's effects often (public
    /// order per region).
    pub fn faction_sum(model: &CampaignModel, f: FactionId) -> EffectSet {
        let rules = &model.rules.effects;
        let w = &model.world;
        faction_set(model, f, |c| character_set(rules, w.character_details.get(&c)), |r| {
            let mut wide = EffectSet::default();
            if let Some(reg) = w.regions.get(&r) {
                for b in reg.effect_buildings() {
                    if let Some(set) = rules.building_factionwide.get(&b.level_key) {
                        wide.merge(set);
                    }
                }
            }
            wide
        })
    }

    /// A faction's basic bonus (faction +0x6FC).
    pub fn faction(&self, f: FactionId, bonus: &str) -> f32 {
        self.faction.get(&f).map_or(0.0, |s| s.get(bonus))
    }

    /// A faction's qualified bonus, e.g. (`UnitCategory`, `upkeep_mod`, `infantry`).
    pub fn faction_qualified(&self, f: FactionId, kind: BonusKind, bonus: &str, qualifier: &str) -> f32 {
        self.faction.get(&f).map_or(0.0, |s| s.get_qualified(kind, bonus, qualifier))
    }

    /// A region's basic bonus: its own buildings plus its governing faction's set (`0x00A67530`; normally
    /// the owner, see [`super::World::governing_faction`]).
    /// Not added (PROVISIONAL): region +0x1DC and the governorship container (UNKNOWN contents), and the
    /// governor's post-level set and his +0x48C, which the exe adds only outside the faction's home
    /// theatre; in the shipped campaigns every faction has one governorship, its home (tested).
    pub fn region(&self, r: RegionId, bonus: &str) -> f32 {
        let local = self.region_local.get(&r).map_or(0.0, |s| s.get(bonus));
        let owner = self.region_owner.get(&r).map_or(0.0, |f| self.faction(*f, bonus));
        local + owner
    }

    /// A character's full view (`0x008AFC00`, CONFIRMED structure): his faction's sum (+0x6FC) plus his
    /// own set (+0x48C). The exe uses his army's container instead of the faction sum when the army has
    /// one (army +0x98 → +0x24; content UNKNOWN, not modelled: PROVISIONAL).
    pub fn character_total(&self, c: CharacterId, bonus: &str) -> f32 {
        let faction = self.character_owner.get(&c).map_or(0.0, |f| self.faction(*f, bonus));
        faction + self.character(c, bonus)
    }

    /// Only a region's own building effects (no faction part), e.g. `tax_bonus_building`.
    pub fn region_local(&self, r: RegionId, bonus: &str) -> f32 {
        self.region_local.get(&r).map_or(0.0, |s| s.get(bonus))
    }

    /// A character's basic bonus (traits + ancillaries).
    pub fn character(&self, c: CharacterId, bonus: &str) -> f32 {
        self.character.get(&c).map_or(0.0, |s| s.get(bonus))
    }
}

/// `FACTION_TECHNOLOGY_MANAGER` state of a researched technology (CONFIRMED, set on completion by `0x008EED20`).
pub const TECH_RESEARCHED: u32 = 0;

/// The level 0..9 a ministerial post's effects are looked up with (`0x008D9D70`, CONFIRMED shape):
/// the holder's rank (`0x00A198D0`): the main attribute of his character type (`agents` #8: `management`
/// for a minister, `command_land` for a general such as Napoleon), its raw saved level without trait / ancillary
/// bonuses (`base`), plus his `management_<post>` bonus (army, navy, finance, justice), clamped to −1..9,
/// plus the `ministerial_effectiveness_modifiers` modifier of the government type for that value
/// (`0x008C6A70`); clamped to 0..9.
pub fn minister_level(rules: &EffectRules, government: &str, post: &str, base: i32, ch: &EffectSet) -> i32 {
    let bonus = match post {
        "army" => ch.get_int("management_army"),
        "navy" => ch.get_int("management_navy"),
        "finance" => ch.get_int("management_finance"),
        "justice" => ch.get_int("management_justice"),
        _ => 0,
    };
    let value = (base + bonus).clamp(-1, 9);
    let modifier = rules.ministerial_effectiveness.get(&(government.to_string(), value)).copied().unwrap_or(0);
    (value + modifier).clamp(0, 9)
}


/// The campaign start (`0x008DD090`, per faction): faction +0x8D4 = the base +0x8C4 + the difficulty
/// handicap list `campaign_difficulty_handicap_effects` (`0x00F9F970`: difficulty clamped to −2..2,
/// the flag picks the human or the AI rows). CONFIRMED structure; a loaded save keeps its saved #55 and
/// never runs this. Human factions use their own difficulty with the human rows; every other faction
/// takes the campaign setup's difficulty with the AI rows, here the negated difficulty of the first
/// human (INFERRED, as `ntw_ai::campaign::data::ai_handicap`; AI_RESEARCH.md "Difficulty"), which is
/// also written to its `difficulty` as the exe writes +0x6E4. PROVISIONAL: the front end has no
/// difficulty choice yet, so the human's difficulty is the one stored in the file.
pub fn apply_start_handicaps(model: &mut CampaignModel) {
    let humans = model.turn.humans.clone();
    let player = humans.first().and_then(|h| model.world.faction_details.get(h)).map_or(0, |d| d.difficulty);
    let rules = &model.rules.effects;
    for (id, d) in &mut model.world.faction_details {
        let human = humans.contains(id);
        if !human {
            d.difficulty = -player;
        }
        let mut entries = d.bonus_base.clone();
        if let Some(set) = rules.difficulty.get(&(d.difficulty.clamp(-2, 2), human)) {
            for (k, v) in &set.values {
                add_saved(&mut entries, k, *v);
            }
        }
        d.bonus_with_difficulty = entries;
    }
}

/// Adds `value` under `key` to saved entries (the exe's add-one, `0x00E04BC0`): an existing entry of
/// the same key grows, else a new one goes in after the last entry with a smaller (type, id). Keys
/// that have no saved form (qualified kinds other than [`BonusKind::Saved`]) are skipped (PROVISIONAL:
/// the handicap rows only map to basic bonuses).
fn add_saved(entries: &mut Vec<SavedBonus>, key: &EffectKey, value: f32) {
    let (kind, bonus, qualifier) = match key.kind {
        BonusKind::Basic => match BONUS_NAMES.iter().position(|n| *n == key.bonus) {
            Some(i) => (1, i as i32, String::new()),
            None => return,
        },
        BonusKind::Saved(t) => match key.bonus.parse() {
            Ok(b) => (t, b, key.qualifier.clone()),
            Err(_) => return,
        },
        _ => return,
    };
    if let Some(e) = entries.iter_mut().find(|e| e.kind == kind && e.bonus == bonus && e.qualifier == qualifier) {
        e.value += value;
        return;
    }
    let at = entries.iter().rposition(|e| (e.kind, e.bonus) <= (kind, bonus)).map_or(0, |i| i + 1);
    entries.insert(at, SavedBonus { kind, bonus, value, qualifier });
}


/// A character's own set: traits by level + ancillaries.
fn character_set(rules: &EffectRules, d: Option<&super::details::CharacterDetails>) -> EffectSet {
    let mut s = EffectSet::default();
    let Some(d) = d else { return s };
    for t in &d.traits {
        if let Some(level) = rules.trait_level_key(&t.key, t.points)
            && let Some(set) = rules.trait_level.get(level)
        {
            s.merge(set);
        }
    }
    for a in &d.ancillaries {
        if let Some(set) = rules.ancillary.get(a) {
            s.merge(set);
        }
    }
    s
}

/// A faction's sum (faction +0x6FC): the saved base + difficulty set, its regions' faction-wide
/// building effects, researched technologies, the government type, and each minister's post set
/// and own set.
fn faction_set(
    model: &CampaignModel,
    id: FactionId,
    character: impl Fn(super::ids::CharacterId) -> EffectSet,
    region_wide: impl Fn(RegionId) -> EffectSet,
) -> EffectSet {
    let rules = &model.rules.effects;
    let w = &model.world;
    let Some(f) = w.factions.get(&id) else { return EffectSet::default() };
    let details = w.faction_details.get(&id);
    // Faction +0x8D4: the base (+0x8C4, agent caps and what scripts and events gave) plus the
    // difficulty handicap, built at campaign start ([`apply_start_handicaps`]) and saved (#55).
    let mut s = details.map_or_else(EffectSet::default, |d| {
        saved_set(if d.bonus_with_difficulty.is_empty() { &d.bonus_base } else { &d.bonus_with_difficulty })
    });
    for (rid, r) in &w.regions {
        if r.owner == id {
            s.merge(&region_wide(*rid));
        }
    }
    if let Some(d) = details {
        for (tech, state) in &d.technologies {
            if *state == TECH_RESEARCHED
                && let Some(set) = rules.technology.get(tech)
            {
                s.merge(set);
            }
        }
    }
    if let Some(set) = rules.government.get(&f.government_key) {
        s.merge(set);
    }
    if let Some(d) = details {
        for post in d.posts.iter().filter(|p| p.governorship.is_none()) {
            let Some(holder) = post.holder else { continue };
            let ch = character(holder);
            // The raw saved level of the type's main attribute (trait and ancillary attribute bonuses do not count here:
            // with them Spain's head of government would be one level too high in every vanilla save).
            let base = w.characters.get(&holder).zip(w.character_details.get(&holder)).and_then(|(c, d)| {
                let main = super::agents::main_attribute(c.kind);
                d.attributes.iter().find(|(k, _)| k == main).map(|(_, v)| *v)
            }).unwrap_or(0);
            let level = minister_level(rules, &f.government_key, &post.key, base, &ch);
            if let Some(set) = rules.ministerial.get(&(post.key.clone(), level)) {
                s.merge(set);
            }
            s.merge(&ch);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> EffectRules {
        let mut r = EffectRules::new(BTreeMap::from([
            ("tax_bonus_technology".to_string(), vec![EffectKey::basic("tax_bonus_technology")]),
            ("happy_all".to_string(), ["lower", "upper"].iter().map(|c| EffectKey { kind: BonusKind::PopClass, bonus: "happiness".into(), qualifier: (*c).into() }).collect()),
        ]));
        r.trait_levels.insert("T".into(), vec![(4, 1, "T_1".into()), (8, 2, "T_2".into()), (16, 3, "T_3".into())]);
        r
    }

    #[test]
    fn compile_maps_and_drops_unmapped_keys() {
        let r = rules();
        let s = r.compile([("tax_bonus_technology", 2.0), ("tax_bonus_technology", 3.0), ("nonsense", 9.0), ("happy_all", 1.0)]);
        assert_eq!(s.get("tax_bonus_technology"), 5.0);
        assert_eq!(s.get("nonsense"), 0.0);
        assert_eq!(s.get_qualified(BonusKind::PopClass, "happiness", "upper"), 1.0);
        assert_eq!(s.values.len(), 3);
    }

    #[test]
    fn trait_levels_by_threshold() {
        let r = rules();
        assert_eq!(r.trait_level_key("T", 3), None);
        assert_eq!(r.trait_level_key("T", 4), Some("T_1"));
        assert_eq!(r.trait_level_key("T", 15), Some("T_2"));
        assert_eq!(r.trait_level_key("T", 99), Some("T_3"));
    }

    #[test]
    fn merge_and_int_rounding() {
        let mut a = EffectSet::default();
        a.add(EffectKey::basic("x"), 1.5);
        let mut b = EffectSet::default();
        b.add(EffectKey::basic("x"), 1.0);
        a.merge(&b);
        assert_eq!(a.get("x"), 2.5);
        assert_eq!(a.get_int("x"), 2, "half to even");
    }

    #[test]
    fn minister_level_from_raw_rank_plus_post_bonus() {
        let mut r = EffectRules::default();
        r.ministerial_effectiveness.insert(("absolute_monarchy".into(), 5), 1);
        let mut ch = EffectSet::default();
        ch.add(EffectKey::basic("management_finance"), 2.0);
        // A faction leader takes only his raw rank plus the government modifier for that value.
        assert_eq!(minister_level(&r, "absolute_monarchy", "faction_leader", 5, &ch), 6);
        // A finance minister adds his management_finance bonus before the modifier lookup.
        assert_eq!(minister_level(&r, "absolute_monarchy", "finance", 3, &ch), 6);
        assert_eq!(minister_level(&r, "republic", "finance", 9, &ch), 9, "clamped");
        assert_eq!(minister_level(&r, "republic", "army", -4, &ch), 0, "clamped at 0");
    }
}

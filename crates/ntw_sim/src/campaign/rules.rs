//! Game data the campaign rules read: tunables, unit and building records, tax levels and
//! effects.
//!
//! The simulation crate has no I/O and does not depend on the database crate, so the loader
//! (`ntw_campaign::rules_from_db`) copies what the rules need from the player's own DB tables into
//! this plain struct (`campaign_variables`, `units`, `building_levels`, `building_effects_junction`,
//! `building_units_allowed`, `building_upgrades_junction`, `building_chain_to_slots`,
//! `taxes_levels` / `taxes_keys` / `taxes_effects_jct`, `government_types_to_effects`, `agents`,
//! `units_to_exclusive_faction_permissions`, `factions`, `unit_stats_land`).
//!
//! It is game data, not state: it is not saved and not part of [`CampaignModel::state_hash`].
//! [`CampaignRules::test_rules`] holds MADE-UP numbers for tests (never game data).
//!
//! [`CampaignModel::state_hash`]: super::CampaignModel::state_hash

use std::collections::BTreeMap;

/// What the campaign rules need to know about one unit (`units` + `unit_stats_land` rows).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnitRules {
    /// `units` #4 recruitment cost (column name INFERRED M).
    pub cost: i32,
    /// `units` #8 upkeep per turn (INFERRED M).
    pub upkeep: i32,
    /// `units` #6 (`unknown_38`): recruitment time in turns. CONFIRMED: a new land recruitment item
    /// (`0x00B58DD0` → `0x00AECEE0` → `0x00AF3F80`) takes its turns (item +0x1C, saved as
    /// `RECRUITMENT_ITEM` #3) from `UNIT_RECORD` +0x34; the record keeps the builder's ints 4 bytes
    /// lower (upkeep: builder +0x40, read at record +0x3C by `0x008F9B10`), so +0x34 is builder +0x38 = #6.
    pub turns: u32,
    /// `units` #2 category starts with `naval` (recruited in ports, cannot walk).
    pub is_naval: bool,
    /// Men of a full unit (`unit_stats_land.num_men`; 0 for ships or when unknown).
    pub men: u32,
    /// What autoresolve needs (land units with `unit_stats_land` rows; `None` for ships).
    pub autoresolve: Option<UnitAutoresolve>,
    /// `units` #2 category key (`infantry`, `naval_*`, ...): qualifier of the unit-category effects.
    pub category: String,
    /// `units` #3 class key (`infantry_line`, ...): qualifier of the unit-class effects.
    pub unit_class: String,
    /// `units` #7 (meaning UNKNOWN, about 0.8 × the cost; `UNIT_RECORD` +0x38, CONFIRMED offset):
    /// the commander pick prefers the higher value (`characters::commander_order`).
    pub value_7: i32,
    /// `units` #21 (flag, meaning UNKNOWN; `UNIT_RECORD` +0x78, CONFIRMED offset): the commander
    /// pick puts land units without it first (`characters::commander_order`).
    pub flag_21: bool,
    /// `unit_stats_land` #70: the militia flag (`UNIT_RECORD` +0x136, CONFIRMED by 0-G): such a unit counts twice in
    /// the garrison repression (`0x008B1C30`).
    pub militia: bool,
    /// `unit_stats_land` #69 (`UNIT_RECORD` +0x135, CONFIRMED copy from builder +0x204 in `0x00E8CFF0`):
    /// the unit can hide on the campaign map (light cavalry, skirmishers, guerrillas; the stealth
    /// test `0x009D1010` lets a force whose units all have it hide anywhere).
    pub campaign_stealth: bool,
}

/// A land unit's autoresolve inputs, from its `units` / `unit_stats_land` rows.
///
/// The potentials are the battle's record potentials (`0x00757120` melee, `0x007575A0` missile,
/// CONFIRMED, ported in `ntw_sim::battle::strength`) for a card of `men` men. Both are linear in
/// the men count, so the loader stores `per_man * men + base` and
/// [`melee`](Self::melee) / [`missile`](Self::missile) give the potential for any strength.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UnitAutoresolve {
    /// Melee potential per man.
    pub melee_per_man: f32,
    /// Melee potential that does not depend on the men (morale and ability terms).
    pub melee_base: f32,
    /// Missile potential per man.
    pub missile_per_man: f32,
    /// Missile potential that does not depend on the men.
    pub missile_base: f32,
    /// `unit_stats_land` morale (col 42): the rout point of the pair query uses it.
    pub morale: f32,
    /// The unit category code of `units` #2 (compare chain `0x00EED2B0`, CONFIRMED): 0 cavalry,
    /// 1 artillery, 2 infantry, 3 dragoons, 4 elephants, 5 camels, 6 anything else.
    pub category: u8,
}

impl UnitAutoresolve {
    /// Melee potential of a card of `men` men (never below 0).
    pub fn melee(&self, men: u32) -> f32 {
        (self.melee_per_man * men as f32 + self.melee_base).max(0.0)
    }

    /// Missile potential of a card of `men` men (never below 0).
    pub fn missile(&self, men: u32) -> f32 {
        (self.missile_per_man * men as f32 + self.missile_base).max(0.0)
    }
}

/// `building_levels` by key ([`CampaignRules::buildings`]): the map itself, read through `Deref`,
/// plus a chain index (each chain's highest level) built on first use. Every mutable access goes
/// through `DerefMut`, which drops the index, so it can never describe an older table.
#[derive(Debug, Clone, Default)]
pub struct BuildingTable {
    levels: BTreeMap<String, BuildingRules>,
    chain_max: std::sync::OnceLock<BTreeMap<String, i32>>,
}

impl BuildingTable {
    /// `level_key`'s 0-based level in its chain and the chain's highest level (the tooltips'
    /// `Level` / `MaxLevel`, the building browser's `level` / `max_level`). `None` only for an
    /// unknown level: a known level always reports its own level.
    pub fn chain_levels(&self, level_key: &str) -> Option<(i32, i32)> {
        let b = self.levels.get(level_key)?;
        let index = self.chain_max.get_or_init(|| {
            let mut max: BTreeMap<String, i32> = BTreeMap::new();
            for l in self.levels.values() {
                let m = max.entry(l.chain.clone()).or_insert(l.level);
                *m = (*m).max(l.level);
            }
            max
        });
        // Present: the index is built from these very levels, `b` among them.
        Some((b.level, index[&b.chain]))
    }
}

impl std::ops::Deref for BuildingTable {
    type Target = BTreeMap<String, BuildingRules>;
    fn deref(&self) -> &Self::Target {
        &self.levels
    }
}

impl std::ops::DerefMut for BuildingTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.chain_max.take();
        &mut self.levels
    }
}

impl PartialEq for BuildingTable {
    fn eq(&self, other: &Self) -> bool {
        self.levels == other.levels
    }
}

impl<'a> IntoIterator for &'a BuildingTable {
    type Item = (&'a String, &'a BuildingRules);
    type IntoIter = std::collections::btree_map::Iter<'a, String, BuildingRules>;
    fn into_iter(self) -> Self::IntoIter {
        self.levels.iter()
    }
}

/// What the rules need to know about one building level.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BuildingRules {
    /// `building_levels` #1 chain.
    pub chain: String,
    /// `building_levels` #2 level index in the chain (0-based).
    pub level: i32,
    /// `building_levels` #5 construction cost (INFERRED M).
    pub cost: i32,
    /// `building_levels` #4 construction time in turns (INFERRED M).
    pub turns: u32,
    /// `building_effects_junction` rows: (effect key, value), in file order.
    pub effects: Vec<(String, f32)>,
    /// `building_units_allowed`: unit keys this level can recruit, in file order.
    pub units_allowed: Vec<String>,
    /// `building_upgrades_junction`: levels this one upgrades to.
    pub upgrades_to: Vec<String>,
    /// The chain's `building_chains` #2 number (chain record +0x18; empty = 0, CONFIRMED parse): the
    /// building sabotage chance reads it (`super::agents::building_sabotage_chance`).
    pub chain_class: i32,
    /// `building_faction_variants`: the factions that may build this level.
    pub factions: Vec<String>,
    /// `building_culture_variants`: the cultures (`cultures_subcultures` #1) that may
    /// build this level.
    pub cultures: Vec<String>,
}

impl BuildingRules {
    /// Sum of this level's values for one effect key.
    pub fn effect(&self, key: &str) -> f32 {
        self.effects.iter().filter(|(k, _)| k == key).map(|(_, v)| v).sum()
    }
}

/// One `historical_characters` row the recruitment pools may offer (CHARACTERS_FIDELITY.md §8).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoricalCandidate {
    /// The row key (`eur_jean_rapp`), kept in the campaign model's created list once he appears.
    pub key: String,
    /// `m` / `f` (#2): generic candidates are men.
    pub male: bool,
    /// The agent type (`agents` key; #3).
    pub kind: String,
    /// The faction key (#4).
    pub faction: String,
    /// The years he may appear in (#5 ..= #6).
    pub years: (i32, i32),
}

/// All game data the campaign rules use. See the module docs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CampaignRules {
    /// `campaign_variables`, with the campaign's `campaigns_campaign_variables_junctions`
    /// overrides already applied.
    pub variables: BTreeMap<String, f32>,
    /// Units by key.
    pub units: BTreeMap<String, UnitRules>,
    /// Building levels by key.
    pub buildings: BuildingTable,
    /// `building_chain_to_slots`: chain → slot types it may stand in.
    pub chain_slots: BTreeMap<String, Vec<String>>,
    /// `taxes_levels`: level key → rate in percent (INFERRED percent).
    pub tax_levels: BTreeMap<String, i32>,
    /// `taxes_keys` + `taxes_effects_jct`: (class key, level key) → effects.
    pub tax_effects: BTreeMap<(String, String), Vec<(String, f32)>>,
    /// `government_types_to_effects`: government key → effects.
    pub government_effects: BTreeMap<String, Vec<(String, f32)>>,
    /// `agents` #1: character type (ESF string) → base action points.
    pub agent_action_points: BTreeMap<String, i32>,
    /// `agents` #9 by agent key: the religion a missionary converts to (see [`super::religion`]).
    pub agent_religions: BTreeMap<String, String>,
    /// `agents` #2: character type → sight radius in map units (character +0x2EC from record +0x10,
    /// CONFIRMED against the saved sight boxes: General 15, colonel 10, admiral 20, rake 18, ...).
    pub agent_sight: BTreeMap<String, i32>,
    /// `campaign_ground_types` keys whose #2 is set: ground an army can hide on (the stealth test,
    /// CHARACTERS_FIDELITY.md §10).
    pub hiding_ground: std::collections::BTreeSet<String>,
    /// `agent_culture_details` #3 for `General`: culture → the unit a newly hired general leads.
    pub general_units: BTreeMap<String, String>,
    /// `historical_characters`: the named recruitment-pool candidates, in table order
    /// ([`super::pool`]; CHARACTERS_FIDELITY.md §8).
    pub historical: Vec<HistoricalCandidate>,
    /// `units_to_exclusive_faction_permissions`: unit → factions allowed (only units that
    /// have rows are restricted).
    pub unit_factions: BTreeMap<String, Vec<String>>,
    /// `factions` #3 category (`playable`, `minor`, `non-expansionist`, `rebel`).
    pub faction_categories: BTreeMap<String, String>,
    /// The campaign key (`campaigns` row) the rules were built for: the exe hard-codes a few rules by
    /// campaign (e.g. `spa_napoleon` trade, [`CampaignModel::trade_home_value`]).
    ///
    /// [`CampaignModel::trade_home_value`]: super::CampaignModel::trade_home_value
    pub campaign: String,
    /// `trade_nodes` rows by node key: (commodity key, base volume, per extra ship, cap).
    pub trade_nodes: BTreeMap<String, (String, i32, f32, f32)>,
    /// `diplomatic_relations_religion`: (religion, other religion) → value.
    pub religion_relations: BTreeMap<(String, String), f32>,
    /// `government_types`: government key → (upper class, lower class). A missing key governs `upper`
    /// and `lower`.
    pub government_classes: BTreeMap<String, (String, String)>,
    /// `technologies` with their `technology_required_technology_junctions` rows, by key.
    pub technologies: BTreeMap<String, TechRules>,
    /// `unit_required_technology_junctions`: unit key → the technologies it needs.
    pub unit_techs: BTreeMap<String, Vec<String>>,
    /// `building_level_required_technology_junctions`: building level → the technologies it needs.
    pub building_techs: BTreeMap<String, Vec<String>>,
    /// `building_chains` #2 as an integer (chain record +0x18; empty → 0), by chain key. The capture
    /// preview damages on occupation only buildings whose chain has 0 (`0x00B14930`, CONFIRMED).
    pub chain_kinds: BTreeMap<String, i32>,
    /// `unit_stats_naval` by ship key: what the naval autoresolve reads ([`super::naval::ShipRules`]).
    pub ships: BTreeMap<String, super::naval::ShipRules>,
    /// `religion_conversion_mods`: (religion, other religion) → modifier (the matrix `0x00877A60` builds; see
    /// [`super::religion`]).
    pub conversion_mods: BTreeMap<(String, String), f32>,
    /// `diplomatic_relations_religion` #2: (religion, other religion) → the `religion` attitude factor value.
    pub religion_attitudes: BTreeMap<(String, String), i32>,
    /// `diplomatic_relations_government_type`: (own government, the record's target government) →
    /// (#2 the value a government change shocks the factor to, #3 the steady value / limit). The campaign
    /// setup writes #3 (`0x00B45A40`); `super::treaties::CampaignModel::change_government` (`0x00B1B5A0`)
    /// writes #2 and drifts to #3.
    ///
    /// **The key is the PAIR**, which is what the exe looks up: `record_index` takes one string, but it
    /// is the composite `own + SEP + target` that `0x00B1B5A0` / `0x00B1B190` / `0x00B45A40` splice
    /// together before the call (0-B round 14, CONFIRMED from the bytes and from the shipped rows: 16
    /// rows, 4 government types x 4, and both #2 and #3 take four different values inside a single
    /// column). Do not collapse it to one government.
    pub government_relations: BTreeMap<(String, String), (i32, i32)>,
    /// `diplomatic_relations_attitudes`: (key, threshold), e.g. (`hostile`, -85). Empty: the shipped values are
    /// used (see [`super::treaties::attitude_category`]).
    pub attitude_thresholds: BTreeMap<String, i32>,
    /// `factions` #2 subculture of each faction key: factions of the same subculture share some attitude
    /// changes (the faction record +0x10 compared by `0x00B13840` / `0x00B0E420`, CONFIRMED).
    pub faction_subcultures: BTreeMap<String, String>,
    /// The culture of each faction key (`cultures_subcultures` of its subculture; the building permission test,
    /// [`CampaignModel::building_permitted`](super::CampaignModel::building_permitted)).
    pub faction_cultures: BTreeMap<String, String>,
    /// `commodities_demand_junction` rows: (commodity key, driver key, factor, weight).
    pub commodity_demand: Vec<(String, String, f32, f32)>,
    /// The effect sources, compiled (slot 0-F, [`super::effects`]).
    pub effects: super::effects::EffectRules,
    /// The character rules (traits, ancillaries; slot 0-G).
    pub characters: super::characters::CharacterRules,
    /// `unit_stats_land_experience_bonuses` / `unit_stats_naval_experience_bonuses`: what a
    /// veteran costs more than a recruit ([`XpCostTables`], CONFIRMED by `0x00ED49A0`).
    /// Empty unless the loader copies the two tables in, and then the cost is the plain
    /// `units` cost, as before.
    pub xp_cost: XpCostTables,
}

/// One row of the experience-adjusted **cost**: the flat term and the multiplier of
/// `unit_stats_land_experience_bonuses` (`row+0x24` / `row+0x28`) or of its naval twin
/// `unit_stats_naval_experience_bonuses` (`row+0x1C` / `row+0x20`).
///
/// CONFIRMED by `0x00ED49A0`, which returns `flat + ROUND(base × mult)` — the base is **scaled**,
/// not added (BATTLE_FIDELITY.md §19a). Land rank 9 is `+360` and `×1.9` (so a 100-cost recruit
/// costs 550), naval rank 9 `+255` and `×1.45` (400).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct XpCostRow {
    /// The flat term (`row+0x24` land, `row+0x1C` naval).
    pub flat: i32,
    /// The multiplier term (`row+0x28` land, `row+0x20` naval).
    pub mult: f32,
}

impl XpCostRow {
    /// What a unit of this experience rank costs instead of `base`: `flat + ROUND(base × mult)`,
    /// with the exe's `ROUND` (half away from zero — *not* a truncation).
    pub fn adjust(&self, base: i32) -> i32 {
        self.flat + (base as f32 * self.mult).round() as i32
    }
}

/// The two experience-adjusted cost tables, by experience rank (chevrons 0..9). The naval table is
/// the one `0x00ED49A0` reads when the unit type has `+0xA0 != 0` (a ship), the land one otherwise;
/// a rank neither table has leaves the cost alone (the exe's else branch).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct XpCostTables {
    /// `unit_stats_land_experience_bonuses` (rank → row).
    pub land: BTreeMap<u8, XpCostRow>,
    /// `unit_stats_naval_experience_bonuses` (rank → row).
    pub naval: BTreeMap<u8, XpCostRow>,
}

impl XpCostTables {
    /// The cost of a unit of experience `experience` instead of `base` (`0x00ED49A0`): the naval
    /// row for a ship, the land row otherwise, and `base` itself when the rank has no row.
    pub fn adjust_cost(&self, naval: bool, experience: u8, base: i32) -> i32 {
        let table = if naval { &self.naval } else { &self.land };
        table.get(&experience).map_or(base, |row| row.adjust(base))
    }
}

/// The tax classes of `taxes_keys` (CONFIRMED keys).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaxClass {
    /// `lower_classes`.
    Lower,
    /// `upper_classes`.
    Upper,
}

impl TaxClass {
    /// The `taxes_keys` class key.
    pub fn key(self) -> &'static str {
        match self {
            TaxClass::Lower => "lower_classes",
            TaxClass::Upper => "upper_classes",
        }
    }
}

/// The tax level of a faction whose file has no governorship taxes (the levels are stored per
/// governorship, `GOVERNORSHIP_TAXES`, and loaded from there): `tax_normal`, the middle row of
/// `taxes_levels` and the level of every governorship in the shipped start positions.
pub const DEFAULT_TAX_LEVEL: &str = "tax_normal";

/// The most units an army can hold. INFERRED (the original's army card bar has 20 slots and
/// armies in every shipped start position hold at most 20 units); not found in a table.
pub const MAX_UNITS_PER_FORCE: usize = 20;

/// MADE-UP autoresolve data for the test units (infantry, 1 potential per man plus 50).
pub const TEST_AUTORESOLVE: UnitAutoresolve =
    UnitAutoresolve { melee_per_man: 1.0, melee_base: 50.0, missile_per_man: 0.0, missile_base: 0.0, morale: 8.0, category: 2 };

impl CampaignRules {
    /// A `campaign_variables` value, or `default` if the table has no such row.
    pub fn var(&self, key: &str, default: f32) -> f32 {
        self.variables.get(key).copied().unwrap_or(default)
    }

    /// The action point cost per map unit at a road level 0..=3:
    /// `road_level_<n>_action_point_cost` (CONFIRMED keys; values 0.67 / 0.6 / 0.5 / 0.4).
    /// INFERRED meaning: the cost of moving one map unit along a road of that level.
    pub fn road_cost(&self, level: u8) -> f32 {
        let key = format!("road_level_{}_action_point_cost", level.min(3));
        self.var(&key, OFF_ROAD_COST)
    }

    /// A tax level's rate in percent (0 if the level is unknown).
    pub fn tax_rate(&self, level: &str) -> i32 {
        self.tax_levels.get(level).copied().unwrap_or(0)
    }

    /// What a unit of experience `experience` costs instead of `base`: the campaign's
    /// **experience-adjusted cost** (`0x00ED49A0` — the army auto-build spends it against its
    /// budget, `0x0045CB50`, and the unit info panel shows it as "XpAdjustedCost",
    /// `0x005CD340`). `naval` picks the naval table, as the exe's unit-type flag `+0xA0` does.
    /// [`XpCostTables::adjust_cost`] leaves the cost alone when the table has no such rank, so an
    /// unfilled table is exactly the old behaviour.
    pub fn xp_adjusted_cost(&self, naval: bool, experience: u8, base: i32) -> i32 {
        self.xp_cost.adjust_cost(naval, experience, base)
    }

    /// `level_key`'s level and its chain's highest level ([`BuildingTable::chain_levels`]).
    pub fn chain_levels(&self, level_key: &str) -> Option<(i32, i32)> {
        self.buildings.chain_levels(level_key)
    }

    /// May `faction` recruit `unit` at all (exclusive permissions)?
    pub fn faction_may_recruit(&self, faction: &str, unit: &str) -> bool {
        match self.unit_factions.get(unit) {
            Some(list) => list.iter().any(|f| f == faction),
            None => true,
        }
    }

    /// MADE-UP rules for unit tests. Not game data: every key starts with `test_` and the
    /// numbers are round.
    pub fn test_rules() -> Self {
        let mut r = CampaignRules::default();
        for (k, v) in [
            ("faction_gdp_other", 100.0),
            ("road_level_0_action_point_cost", 1.0),
            ("road_level_1_action_point_cost", 0.5),
            ("road_level_2_action_point_cost", 0.5),
            ("road_level_3_action_point_cost", 0.5),
            ("unit_minimum_strength", 0.05),
            ("autoresolve_stat_massacre_chance", 0.1),
            ("autoresolve_unit_losses_fuzziness", 0.1),
            ("autoresolve_gaussian_boundary", 3.0),
            ("autoresolve_gaussian_standard_deviation", 1.0),
            ("autoresolve_min_combat_potential_only_win_chance", 0.5),
            ("autoresolve_minimum_win_chance_to_win", 0.2),
            ("autoresolve_commander_star_rating_impact", 0.1),
            ("autoresolve_major_land_victory_percent", 0.1),
            ("autoresolve_minor_land_victory_percent", 0.1),
            ("autoresolve_minimum_casualties_on_win", 0.1),
            ("autoresolve_minimum_casualties_on_lose", 0.5),
            ("autoresolve_advantage_over_enemy_wipeout_threshold", 0.6),
            ("losing_unit_minimum_strength", 0.2),
        ] {
            r.variables.insert(k.into(), v);
        }
        r.units.insert(
            "test_unit".into(),
            UnitRules { cost: 500, upkeep: 10, turns: 2, is_naval: false, men: 100, autoresolve: Some(TEST_AUTORESOLVE), category: "infantry".into(), unit_class: "infantry_line".into(), value_7: 0, flag_21: false, militia: false, campaign_stealth: false },
        );
        r.units.insert(
            "test_recruit".into(),
            UnitRules { cost: 500, upkeep: 10, turns: 2, is_naval: false, men: 100, autoresolve: Some(TEST_AUTORESOLVE), category: "infantry".into(), unit_class: "infantry_line".into(), value_7: 0, flag_21: false, militia: false, campaign_stealth: false },
        );
        r.buildings.insert(
            "test_building_level".into(),
            BuildingRules {
                chain: "test_chain".into(),
                level: 0,
                cost: 300,
                turns: 2,
                effects: vec![("recruitment_points".into(), 2.0), ("happy_culture_all".into(), 1.0)],
                units_allowed: vec!["test_unit".into(), "test_recruit".into()],
                upgrades_to: vec!["test_building_level_2".into()],
                chain_class: 0,
                ..Default::default()
            },
        );
        r.buildings.insert(
            "test_building_level_2".into(),
            BuildingRules { chain: "test_chain".into(), level: 1, cost: 600, turns: 3, ..Default::default() },
        );
        r.chain_slots.insert("test_chain".into(), vec!["test_slot".into()]);
        for (k, v) in [("tax_low", 10), ("tax_normal", 15), ("tax_high", 20)] {
            r.tax_levels.insert(k.into(), v);
        }
        r.tax_effects.insert(
            ("lower_classes".into(), "tax_high".into()),
            vec![("happy_active_lower_tax".into(), -10.0)],
        );
        r.agent_action_points.insert("General".into(), 30);
        // A MADE-UP effect mapping in the shape of `effect_bonus_value_*_junction` (DB key → engine
        // bonus), and the test buildings' local effects compiled through it.
        use super::effects::{BonusKind, EffectKey};
        let pop = |bonus: &str| -> Vec<EffectKey> {
            ["lower", "middle", "upper"].iter().map(|c| EffectKey { kind: BonusKind::PopClass, bonus: bonus.into(), qualifier: (*c).into() }).collect()
        };
        r.effects.mapping.insert("recruitment_points".into(), vec![EffectKey::basic("recruitment_points")]);
        r.effects.mapping.insert("happy_active_lower_tax".into(), vec![EffectKey::basic("happiness_active_lower_tax")]);
        r.effects.mapping.insert("happy_culture_all".into(), pop("happiness_culture"));
        for b in ["test_building_level", "test_building_level_2"] {
            let set = r.effects.compile(r.buildings[b].effects.iter().map(|(k, v)| (k.as_str(), *v)));
            r.effects.building_local.insert(b.into(), set);
        }
        r
    }
}

/// PLACEHOLDER cost per map unit away from roads. UNKNOWN in the original (its pathfinder data,
/// `pathfinding.esf`, is not decoded yet); 1.0 makes roads of every level cheaper than open land.
pub const OFF_ROAD_COST: f32 = 1.0;

/// One technology (`technologies` row, CAMPAIGN_FIDELITY.md §Research).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TechRules {
    /// #3 research cost (the points the research needs).
    pub cost: i32,
    /// #1 the building level it needs: the faction must own that chain (or a variant such as
    /// `sAdminSpain` for `sAdmin`) at this level or higher before it becomes available.
    pub building_level: String,
    /// The technologies it requires (`technology_required_technology_junctions`).
    pub requires: Vec<String>,
}

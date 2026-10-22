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
//! [`CampaignRules::test_rules`] holds rules for tests: mostly MADE-UP, plus a few vanilla rows.
//!
//! [`CampaignModel::state_hash`]: super::CampaignModel::state_hash

use std::collections::BTreeMap;

/// What the campaign rules need to know about one unit (`units` + `unit_stats_land` rows).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UnitRules {
    /// `units` #4 (`UNIT_RECORD` +0x2C): the battle army-setup price (custom and multiplayer
    /// battles: the `0x00ED49A0` base), **not** what the campaign charges ([`Self::campaign_cost`]).
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
    /// `units` #7 (`UNIT_RECORD` +0x38): the campaign recruitment cost before effects. CONFIRMED:
    /// the recruitable entry's cost `0x00B0D220` scales record +0x38
    /// ([`super::economy::recruitment_cost`]), and the queued items of the saves the original wrote hold it
    /// with the effects applied (`economy_check` `ECON_RECRUITCOST`: 392 of 406 items; CAMPAIGN_FIDELITY.md
    /// §Recruitment cost and money). The commander pick prefers the higher
    /// value (`characters::commander_order`).
    pub campaign_cost: i32,
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
    /// `units` #15 (`UNIT_RECORD` +0x68): the most units of this type the faction may hold and have queued
    /// together, 0 = no limit (CONFIRMED: `0x008F68B0`, see [`super::CampaignModel::recruitable_entry_flags`]).
    pub unit_cap: i32,
    /// `units` #9 (`UNIT_RECORD` +0x40, CONFIRMED: the record builder `0x00E91320` copies builder +0x44 there
    /// at `0x00E91388`): the path cost a unit recruited through a commander covers per turn on its way to him.
    /// The commander's recruitment panel divides the path cost from the source settlement by it (`0x00B41F60`,
    /// [`super::CampaignModel::commander_recruitment`]); 23..55 in the vanilla data. A value below 1 (mod data) is read as 1.
    pub travel_speed: i32,
    /// `units` #14 (`recruitment_scope`, `UNIT_RECORD` +0x98): the `region_unit_resources` key the unit
    /// needs, `None` when empty. CONFIRMED: the `units` row builder `0x00EDC000` looks a non-empty value
    /// up in `region_unit_resources` and the record ctor `0x00E91320` stores the found record at +0x98;
    /// `0x00AA1660` then lists the unit only in a region whose `RESOURCES_ARRAY`
    /// ([`super::world::Region::unit_resources`]) holds it (vanilla: `global`, `horses`, ...). A unit whose
    /// value is not a key never reaches the rules: the loader drops its row, as `0x00EDC000` does.
    pub unit_resource: Option<String>,
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
    /// 1 artillery (and any key the original does not know), 2 infantry, 3 dragoons, 4 elephants, 5 camels,
    /// 6 the naval ones ([`crate::unit_kind::category`]).
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
    /// The chain's `building_chains` #2 number (chain record +0x14, copied from the row's +0x18 by the
    /// builder `0x00E55C30`; empty = 0, CONFIRMED parse): the building sabotage chance reads it
    /// (`super::agents::building_sabotage_chance`).
    pub chain_class: i32,
    /// The chain's technology thread (chain record +0xC, CONFIRMED `0x00F21E60` from `building_chains` #1,
    /// exact case): 0 `military`, 1 `industry`, 2 `enlightenment`, 3 any other group or none.
    pub chain_thread: i32,
    /// `building_faction_variants`: the factions that may build this level.
    pub factions: Vec<String>,
    /// `building_culture_variants`: the cultures (`cultures_subcultures` #1) that may
    /// build this level.
    pub cultures: Vec<String>,
}

/// The thread of a chain without one of the three technology groups (`0x00F21E60`'s fallback).
pub const NO_THREAD: i32 = 3;

impl BuildingRules {
    /// The chain record's pair at +0x10 / +0x14 (builder `0x00E55C30`, CONFIRMED): (0, #2 number) for a
    /// chain of one of the three technology groups, (−1, #2 number) otherwise. A technology's building
    /// requirement matches every chain with the same pair (`0x008B13E0`), so `sAdminSpain` (enlightenment,
    /// 0) serves `sAdmin` (enlightenment, 0) but `tFactorySpain` (no group) does not serve `tFactory`.
    pub fn chain_pair(&self) -> (i32, i32) {
        (if self.chain_thread == NO_THREAD { -1 } else { 0 }, self.chain_class)
    }

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
    /// #7: the Peninsular guerrilla leaders' `guerrilla#<region>#<NNN>` gives their portrait
    /// ([`super::portraits::historical_portrait`]); the other rows' notes do not matter to it.
    pub note: String,
}

/// The culture a culture-keyed row lookup falls back to ([`CampaignRules::culture_row`], `0x00F54F80`,
/// string `0x0132EE78`).
pub const FALLBACK_CULTURE: &str = "european";

/// All game data the campaign rules use. See the module docs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CampaignRules {
    /// The gameplay caps a mod may change ([`crate::limits`]); a new campaign takes its units per
    /// army / navy from them ([`ForceCaps::new_campaign`]).
    pub limits: crate::limits::GameLimits,
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
    /// `government_types_to_effects`: government key → effects.
    pub government_effects: BTreeMap<String, Vec<(String, f32)>>,
    /// `agents` #1: character type (ESF string) → base action points.
    pub agent_action_points: BTreeMap<String, i32>,
    /// `agents` #11 / #10 by agent key: (the base number of characters of the type a faction may
    /// have, the ceiling on it with its `cap` bonus added, 0 = none). Agent record +0x1A8 / +0x1A4,
    /// CONFIRMED copied by the row reader at `0x00F931A7` and read by `0x009D1E00` (the pool's
    /// `MaxGeneralsAllowed`, [`super::world::CampaignModel::commanders_max`]). Vanilla: General
    /// (4, 0), admiral (3, 0), the spies and missionaries (0, 12). A type without a row has none.
    pub agent_caps: BTreeMap<String, (i32, i32)>,
    /// `agents` #9 by agent key: the religion a missionary converts to (see [`super::religion`]).
    pub agent_religions: BTreeMap<String, String>,
    /// `agents` #2: character type → sight radius in map units (character +0x2EC from record +0x10,
    /// CONFIRMED against the saved sight boxes: General 15, colonel 10, admiral 20, rake 18, ...).
    pub agent_sight: BTreeMap<String, i32>,
    /// `agents` #4 false: the character types that never count against a settlement's room for
    /// characters without a force (agent record +0x18 via `0x00A0B9C0`, read by the can-enter check
    /// `0x00B165B0`, CONFIRMED; vanilla: minister and bandit). A type without a row counts.
    pub residence_exempt_agents: std::collections::BTreeSet<String>,
    /// `agents` #8 (record +0x44) by agent key: the type's main attribute, which its rank and the cards'
    /// primary attribute picture are read from (`0x00A198C0`, CONFIRMED data: General and colonel
    /// `command_land`, admiral and captain `command_sea`, rake `subterfuge`, ...). A type without one has
    /// no rank.
    pub agent_primary_attributes: BTreeMap<String, String>,
    /// `agents` #6 by agent key, where set: the agent type's portrait folder, which also names the
    /// deck category its new characters draw from ([`super::portraits`]; admiral → `General`,
    /// missionaries and gentleman → `minister`, guerilla → `guerrilla`, Eastern_Scholar →
    /// `scholar`). An agent type without one uses its own key.
    pub agent_portrait_folders: BTreeMap<String, String>,
    /// `campaign_ground_types` keys whose #2 is set: ground an army can hide on (the stealth test,
    /// CHARACTERS_FIDELITY.md §10).
    pub hiding_ground: std::collections::BTreeSet<String>,
    /// `agent_culture_details` #3 for `General`: culture → the unit a newly hired general leads.
    pub general_units: BTreeMap<String, String>,
    /// `historical_characters`: the named recruitment-pool candidates, in table order
    /// ([`super::pool`]; CHARACTERS_FIDELITY.md §8).
    pub historical: Vec<HistoricalCandidate>,
    /// The names new characters are drawn from ([`super::names`]): the `names` table's pools by
    /// faction and the historical characters' on-screen names, filled by the campaign source.
    pub names: super::names::NameRules,
    /// Faction → the units it may recruit at all (`FACTION_RECORD` +0x1AC, built by `0x00F74CB0` from its
    /// military grouping and its `units_to_exclusive_faction_permissions` rows: [`faction_unit_set`]). A unit
    /// outside the set is never recruitable (`0x00B43CA0`), and a faction with no entry recruits nothing.
    pub faction_units: BTreeMap<String, std::collections::BTreeSet<String>>,
    /// `units_to_gov_type_permissions`: unit → the government types its `enabled` rows name (`UNIT_RECORD`
    /// +0xDC). A unit listed here is recruitable only under one of them ([`Self::government_may_recruit`]).
    pub unit_governments: BTreeMap<String, Vec<String>>,
    /// `factions` #3 category (`playable`, `minor`, `non-expansionist`, `rebel`).
    pub faction_categories: BTreeMap<String, String>,
    /// The campaign key (`campaigns` row) the rules were built for. The rules never switch on it: the
    /// rules the exe hard-codes by campaign are in [`Self::features`].
    pub campaign: String,
    /// The campaign's rule switches (the exe's `spa_napoleon` / `mp_eur_napoleon` rules), filled by the
    /// campaign's source.
    pub features: super::features::CampaignFeatures,
    /// The rules in use for every rule seam ([`super::seams`]): the original's by default, a mod's
    /// where data selects one. The model, the UI and the AI call those rules only through these.
    pub seams: super::seams::CampaignSeams,
    /// The campaign AI's personalities and their tunables (`campaign_ai_personalities`,
    /// `campaign_ai_personality_junctions`), read by the model's AI rules ([`super::capture_ai`]) and the
    /// campaign AI.
    pub ai_personalities: super::personalities::AiPersonalities,
    /// The campaign director's data ([`super::director`]): `cdir_configs` and its relation weights.
    pub director: super::director::DirectorRules,
    /// `trade_nodes` rows by node key: (commodity key, base volume, per extra ship, cap).
    pub trade_nodes: BTreeMap<String, (String, i32, f32, f32)>,
    /// `diplomatic_relations_religion`: (religion, other religion) → value.
    pub religion_relations: BTreeMap<(String, String), f32>,
    /// `government_types`: government key → (upper class, lower class). A missing key governs `upper`
    /// and `lower`.
    pub government_classes: BTreeMap<String, (String, String)>,
    /// `government_types` #3 by government key (government record +0x14, CONFIRMED copied there by the
    /// record's constructor `0x00F76A70`): the rank a war declaration compares between the two factions'
    /// governments when it moves the declarer's classes' war factor
    /// ([`super::population::class_war_change`]). A missing key ranks 0.
    pub government_ranks: BTreeMap<String, i32>,
    /// `technologies` with their `technology_required_technology_junctions` and
    /// `technology_required_building_levels_junctions` rows, by key.
    pub technologies: BTreeMap<String, TechRules>,
    /// `unit_required_technology_junctions`: unit key → the technologies it needs (rows naming no unit or
    /// no technology dropped at load, as the exe's linker `0x00EA9B10`).
    pub unit_techs: BTreeMap<String, Vec<String>>,
    /// `building_level_required_technology_junctions`: building level → the technologies it needs
    /// (building level record +0x118 / +0x11C, linked by `0x00DDF300`: a row naming no technology is
    /// dropped, so only `technologies` keys are kept).
    pub building_techs: BTreeMap<String, Vec<String>>,
    /// `building_chains` #2 as an integer (chain record +0x14; empty → 0), by chain key. The capture
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
    /// `diplomacy_negotiation_strings`: (event, culture, government key) → `diplomacy_strings` key
    /// ([`super::negotiation`]).
    pub negotiation_strings: BTreeMap<(String, String, String), String>,
    /// `diplomacy_negotiation_faction_override_strings`: (event, culture, government key, faction key)
    /// → `diplomacy_strings` key ([`super::negotiation`]).
    pub negotiation_overrides: BTreeMap<(String, String, String, String), String>,
    /// `factions` #2 subculture of each faction key: factions of the same subculture share some attitude
    /// changes (the faction record +0x10 compared by `0x00B13840` / `0x00B0E420`, CONFIRMED).
    pub faction_subcultures: BTreeMap<String, String>,
    /// The culture of each faction key (`cultures_subcultures` of its subculture; the building permission test,
    /// [`CampaignModel::building_permitted`](super::CampaignModel::building_permitted)).
    pub faction_cultures: BTreeMap<String, String>,
    /// `cultures` #2 by culture key (empty when the row has none): the culture a culture's movie rows
    /// also stand in for ([`super::outcome`]), and the fallback portrait folder of the UI's
    /// `MinisterPortraitPath` when the culture's own folder lacks the minister card (ntw_script
    /// `ui/campaign/diplomacy.rs`; INFERRED there, `0x008C05B0` is not decoded).
    pub culture_alternates: BTreeMap<String, String>,
    /// `factions` #39, the faction's movie id, by faction key (empty when it names none; [`super::outcome`]).
    pub faction_movie_ids: BTreeMap<String, String>,
    /// `movie_event_strings` rows in table order ([`super::outcome`]; the lookups depend on the order).
    pub movie_events: Vec<super::outcome::MovieEventRow>,
    /// `message_event_strings` rows in table order ([`super::message_events`]; the culture lookup depends on
    /// the order).
    pub message_events: Vec<super::message_events::MessageEventRow>,
    /// `commodities_demand_junction` rows: (commodity key, driver key, factor, weight).
    pub commodity_demand: Vec<(String, String, f32, f32)>,
    /// The effect sources, compiled (slot 0-F, [`super::effects`]).
    pub effects: super::effects::EffectRules,
    /// The character rules (traits, ancillaries; slot 0-G).
    pub characters: super::characters::CharacterRules,
    /// The round start's historical events ([`super::historical_events`]).
    pub historical_events: super::historical_events::HistoricalEventRules,
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

/// The most units an army and a navy can hold: campaign state, as in the original (CONFIRMED). The
/// original keeps them in two globals (`0x014576B0` army, `0x014576B4` navy) that the army's and
/// the navy's capacity virtual (`+0x48`: `0x008D3920` / `0x008D3930`) return. The campaign loader
/// `0x00872550` reads them from `CAMPAIGN_MODEL` #23 / #24 (20 / 20 for files older than version
/// 6); then, when it is handed a campaign setup (a new campaign, `0x008B8930`; a loaded game,
/// `0x008B8A60`, hands none), it overwrites both with the setup's `+0x48` / `+0x4C`; the writer
/// `0x008EBAB0` saves them back. So the shipped start positions' 20 / 14 (the start-position
/// builder `0x008742C0` writes 20 / 14) never reach play: every one of the original's own saves,
/// turn-0 saves included, holds 20 / 20 (CONFIRMED by result, `campaign_unit_multiplier` 1).
/// Ours: imported from the same fields, set by [`ForceCaps::new_campaign`] for a new campaign,
/// saved with the model; [`CampaignModel::max_units`](super::CampaignModel::max_units) is the one
/// rule every unit-count check asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ForceCaps {
    /// Units per army (`CAMPAIGN_MODEL` #23).
    pub army: u32,
    /// Units (ships) per navy (`CAMPAIGN_MODEL` #24).
    pub navy: u32,
}

/// The `campaign_unit_multiplier` preference's default (`0x004032F0` registers it as float
/// preference `0x6D` with 0.75; CONFIRMED). The player's shipped `preferences.script.txt` may set
/// another value (1 on the user's install).
pub const CAMPAIGN_UNIT_MULTIPLIER_DEFAULT: f32 = 0.75;
/// The preference's key in `preferences.script.txt`.
pub const CAMPAIGN_UNIT_MULTIPLIER_KEY: &str = "campaign_unit_multiplier";

impl Default for ForceCaps {
    /// 20 / 20: the loader's caps for a file without them (`0x00872550`), and the caps of a model
    /// built in code (tests).
    fn default() -> Self {
        ForceCaps { army: crate::limits::ORIGINAL_MAX_LAND_UNITS, navy: crate::limits::ORIGINAL_MAX_LAND_UNITS }
    }
}

impl ForceCaps {
    /// A new campaign's caps: the units per army and per fleet
    /// ([`crate::limits::GameLimits::max_units`]) at the `campaign_unit_multiplier` preference
    /// (`None`: its default 0.75), i.e. 20 and 6 / 8 / 10 / 20 by the multiplier's unit-size step.
    /// CONFIRMED (traced through the stack): the front end's `StartCampaign` script call
    /// (`0x00478840`) builds the campaign setup in `0x00488720`: `0x00878060` fills the setup's
    /// part at `+0x1C` with `return 20` (`0x00851550`) at part `+0x2C` and
    /// `MaxUnitsFromUnitScaleFactor` (`0x00DACE60`) of preference `0x6D` at part `+0x30`, i.e. the
    /// setup's `+0x48` / `+0x4C`, which `0x00872310` copies (`0x00877DB0`) and the loader writes to
    /// the caps (`0x00873BD2`). The command-line `campaign` start (`0x00485B90`) builds the same.
    /// The multiplayer lobby's campaign (`0x004AA900`, game type 3) passes a fixed 20 / 14 instead,
    /// at the lobby's unit scale (not modelled: no multiplayer campaign yet).
    pub fn new_campaign(limits: &crate::limits::GameLimits, campaign_unit_multiplier: Option<f32>) -> Self {
        let (army, navy) = limits.max_units(campaign_unit_multiplier.unwrap_or(CAMPAIGN_UNIT_MULTIPLIER_DEFAULT));
        ForceCaps { army, navy }
    }
}

/// MADE-UP autoresolve data for the test units (infantry, 1 potential per man plus 50).
pub const TEST_AUTORESOLVE: UnitAutoresolve =
    UnitAutoresolve { melee_per_man: 1.0, melee_base: 50.0, missile_per_man: 0.0, missile_base: 0.0, morale: 8.0, category: 2 };

/// A faction's unit set (`FACTION_RECORD` +0x1AC, CONFIRMED, `0x00F74CB0`): the list starts as the faction's
/// military grouping's units (`units_to_groupings_military_permissions`, grouping = `factions` #34), each
/// `units_to_exclusive_faction_permissions` row of the faction then appends its unit when `allowed`, else
/// queues it as denied, and each denied unit removes its first copy from the list (`0x00EA38C0`); what is
/// left becomes the set. So a unit no row names is not recruitable, and a unit both in the grouping and
/// allowed once survives one denied row.
pub fn faction_unit_set<'a>(
    grouping_units: impl IntoIterator<Item = &'a str>,
    faction_rows: impl IntoIterator<Item = (&'a str, bool)>,
) -> std::collections::BTreeSet<String> {
    let mut allowed: Vec<&str> = grouping_units.into_iter().collect();
    let mut denied = Vec::new();
    for (unit, ok) in faction_rows {
        if ok {
            allowed.push(unit);
        } else {
            denied.push(unit);
        }
    }
    for unit in denied {
        if let Some(i) = allowed.iter().position(|u| *u == unit) {
            allowed.remove(i);
        }
    }
    allowed.into_iter().map(str::to_owned).collect()
}

impl CampaignRules {
    /// The row of one event's `rows` (in table order) a faction of `culture` gets (`0x00F54F80`,
    /// CONFIRMED by disassembly; the movie queuers and the event message queuer `0x009D3250` call it):
    /// a row of the faction's own culture is taken at once; a row whose culture's `cultures` #2
    /// ([`culture_alternates`](Self::culture_alternates)) is the faction's culture becomes the pick, the
    /// last such row winning; otherwise the first row is the pick, which, until such a #2 match is seen,
    /// a [`FALLBACK_CULTURE`] row replaces unless the pick is already of it. A faction whose culture is
    /// unknown takes only the first / fallback steps. `row_culture` reads a row's culture key.
    pub fn culture_row<'a, R: 'a>(&self, rows: impl IntoIterator<Item = &'a R>, culture: Option<&str>, row_culture: impl Fn(&R) -> &str) -> Option<&'a R> {
        let mut pick: Option<&R> = None;
        let mut alternate_seen = false;
        for row in rows {
            let rc = row_culture(row);
            if culture == Some(rc) {
                return Some(row);
            }
            let alternate = self.culture_alternates.get(rc).map(String::as_str);
            if culture.is_some() && alternate == culture {
                alternate_seen = true;
                pick = Some(row);
            } else if pick.is_none_or(|p| !alternate_seen && row_culture(p) != FALLBACK_CULTURE && rc == FALLBACK_CULTURE) {
                pick = Some(row);
            }
        }
        pick
    }

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
    /// `level_key`'s level and its chain's highest level ([`BuildingTable::chain_levels`]).
    pub fn chain_levels(&self, level_key: &str) -> Option<(i32, i32)> {
        self.buildings.chain_levels(level_key)
    }

    /// May a faction with the `government` type recruit `unit` (CONFIRMED, `0x00EA9810` on the government's
    /// type record, faction +0x70C → +0xB0, `0x008C8CF0`)? Yes when the unit names no `enabled` government type,
    /// else only under one of them. Failing it leaves the unit out of the recruitable list (`0x00B43CA0`).
    pub fn government_may_recruit(&self, government: &str, unit: &str) -> bool {
        self.unit_governments.get(unit).is_none_or(|list| list.is_empty() || list.iter().any(|g| g == government))
    }

    /// Is `unit` a ship, so that its queued items sit in a port's naval queue (slot +0x1E8) rather than the
    /// region's land queue (region +0x124)? An unknown key counts as land.
    pub fn is_naval_unit(&self, unit: &str) -> bool {
        self.units.get(unit).is_some_and(|u| u.is_naval)
    }

    /// May `faction` recruit `unit` at all: is it in the faction's unit set ([`Self::faction_units`])? The
    /// recruitable list's first gate (`0x00B43CA0` looks the unit up in faction +0x1AC, CONFIRMED).
    pub fn faction_may_recruit(&self, faction: &str, unit: &str) -> bool {
        self.faction_units.get(faction).is_some_and(|set| set.contains(unit))
    }

    /// Rules for unit tests. Mostly MADE-UP (round numbers, `test_` keys); the `agents` #8 main
    /// attributes are a copy of the vanilla rows.
    pub fn test_rules() -> Self {
        Self::test_rules_with([])
    }

    /// [`Self::test_rules`] whose effect mapping also maps `extra` (DB effect key → bonus keys): each
    /// test that compiles its own effect sets brings the keys they use, so no test's keys sit in the
    /// shared fixture. Panics if an `extra` key is one of the fixture's own.
    pub fn test_rules_with(extra: impl IntoIterator<Item = (String, Vec<super::effects::EffectKey>)>) -> Self {
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
            UnitRules { cost: 500, upkeep: 10, turns: 2, is_naval: false, men: 100, autoresolve: Some(TEST_AUTORESOLVE), category: "infantry".into(), unit_class: "infantry_line".into(), campaign_cost: 400, flag_21: false, militia: false, campaign_stealth: false, unit_cap: 0, travel_speed: 30, unit_resource: None },
        );
        r.units.insert(
            "test_recruit".into(),
            UnitRules { cost: 500, upkeep: 10, turns: 2, is_naval: false, men: 100, autoresolve: Some(TEST_AUTORESOLVE), category: "infantry".into(), unit_class: "infantry_line".into(), campaign_cost: 400, flag_21: false, militia: false, campaign_stealth: false, unit_cap: 0, travel_speed: 30, unit_resource: None },
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
        r.agent_action_points.insert("General".into(), 30);
        // A copy of the vanilla `agents` #8 values; ntw_campaign's `vanilla_agent_main_attributes`
        // asserts the same pairs against the install's merged database (not against this copy).
        for (agent, main) in [
            ("General", "command_land"),
            ("colonel", "command_land"),
            ("admiral", "command_sea"),
            ("captain", "command_sea"),
            ("gentleman", "research"),
            ("Eastern_Scholar", "research"),
            ("minister", "management"),
            ("rake", "subterfuge"),
            ("assassin", "subterfuge"),
            ("guerilla", "subterfuge"),
            ("catholic_missionary", "zeal"),
            ("orthodox_missionary", "zeal"),
            ("Protestant_Missionary", "zeal"),
        ] {
            r.agent_primary_attributes.insert(agent.into(), main.into());
        }
        // A MADE-UP effect mapping in the shape of `effect_bonus_value_*_junction` (DB key → engine
        // bonus): the fixture's own keys (the test buildings' local effects and the tax bundle below)
        // plus the test's `extra` keys. The mapping is fixed once the rules are built, so a test
        // compiling its own sets passes the keys they use here.
        use super::effects::{BonusKind, EffectKey, EffectRules};
        let mut mapping = BTreeMap::from([
            ("recruitment_points".to_string(), vec![EffectKey::basic("recruitment_points")]),
            // The vanilla rows of the trade port effects (the route cap, effect 0x43, and the export port, effect 2).
            ("trade_routes_mod_max_sea".to_string(), vec![EffectKey::basic("trade_route_cap_sea")]),
            ("export_capacity_port".to_string(), vec![EffectKey::basic("commodity_export_vol")]),
            ("happy_active_lower_tax".to_string(), vec![EffectKey::basic("happiness_active_lower_tax")]),
            (
                "happy_culture_all".to_string(),
                ["lower", "middle", "upper"]
                    .iter()
                    .map(|c| EffectKey { kind: BonusKind::PopClass, bonus: "happiness_culture".into(), qualifier: (*c).into() })
                    .collect(),
            ),
        ]);
        for (key, keys) in extra {
            assert!(mapping.insert(key.clone(), keys).is_none(), "test mapping key {key:?} is already in the fixture's mapping");
        }
        r.effects = EffectRules::new(mapping);
        for b in ["test_building_level", "test_building_level_2"] {
            let rows: Vec<(&str, f32)> = r.buildings[b].effects.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            r.effects.insert_building_local(b.into(), rows);
        }
        r.effects.set_tax_bundle("lower_classes", "tax_high", vec![("happy_active_lower_tax".into(), -10.0)]);
        // The test world's factions (and the AI tests' `test_faction`) may recruit every fixture unit (one `allowed`
        // row each).
        for f in ["test_faction_a", "test_faction_b", "test_faction_c", "test_faction"] {
            let set = faction_unit_set([], r.units.keys().map(|u| (u.as_str(), true)));
            r.faction_units.insert(f.into(), set);
        }
        r
    }

    /// Test helper: lets every faction that has a unit set recruit `unit` too (a unit a test adds to the
    /// fixture after [`Self::test_rules`]).
    pub fn allow_unit_for_all(&mut self, unit: &str) {
        for set in self.faction_units.values_mut() {
            set.insert(unit.to_owned());
        }
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
    /// #1 the building level it needs (record +0x5C): the faction must own a building of a chain with
    /// the same [`BuildingRules::chain_pair`] at this level or higher before it becomes available.
    pub building_level: String,
    /// The technologies it requires (`technology_required_technology_junctions` rows naming a
    /// `technologies` key, in table order; the linker `0x00F21A30` drops the others). The exe splits
    /// them into [`Self::parent`] (+0x60) and the list +0x70; availability checks them all.
    pub requires: Vec<String>,
    /// The single requirement (record +0x60, linker `0x00F21A30`, CONFIRMED): the last of
    /// [`Self::requires`] whose building level's chain has the same [`BuildingRules::chain_pair`] as this
    /// one's (the same tree). The grant walk (`GrantFactionTechnology` `0x008CDCB0`) follows it.
    pub parent: Option<String>,
    /// The building levels it also needs, exact chain at that level or higher
    /// (`technology_required_building_levels_junctions` rows naming a `building_levels` key; list +0x7C,
    /// linker `0x00F21A30`; checked by `0x008B13C0`). Empty in the shipped DB.
    pub required_buildings: Vec<String>,
    /// The research thread (record +0x8C = its building level's chain record +0xC, linker `0x00F21A30`,
    /// CONFIRMED): 0 military, 1 industry, 2 enlightenment, 3 none ([`BuildingRules::chain_thread`]). The
    /// school rate adds the matching thread effect (`0x008EA9F0`).
    pub thread: usize,
    /// The factions that may have it (`technology_faction_junctions`, gathered per technology by the
    /// technologies table's row builder `0x00F20F00` into the record's set at +0xC0): empty means every
    /// faction (`0x00F1EFA0`, CONFIRMED).
    pub factions: Vec<String>,
}

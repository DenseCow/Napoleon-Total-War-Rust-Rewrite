//! Campaign-map tables: tunables, buildings, recruitment, taxes, agents and map slots.
//!
//! Layouts: Worker 3 `DB_CAMPAIGN_TABLES.md` (CONFIRMED by exact-EOF parses) and Worker 1
//! `DB_BUILDERS.md`; re-checked against the install by the campaign-play worker with
//! `pack_probe db <table> <codes>` (every table below parses to EOF). Column **names** are
//! INFERRED (the files have none), as everywhere in this crate.

use crate::record::{db_record, Table};

db_record! {
    /// `campaign_variables` (`sf`, 121 rows): a global campaign tunable (DB_CAMPAIGN_TABLES §8).
    pub struct CampaignVariable in "campaign_variables", key = key {
        /// #0 H: variable key, e.g. `road_level_0_action_point_cost`.
        key: String,
        /// #1 H: value (f32; the exe copies 4 bytes, the values only make sense as floats).
        value: f32,
    }
}

db_record! {
    /// `campaigns_campaign_variables_junctions` (`ssf`, 25 rows): per-campaign overrides of
    /// `campaign_variables` (CONFIRMED values, e.g. `character_recruitment_max_distance`
    /// eur 550).
    pub struct CampaignVariableOverride in "campaigns_campaign_variables_junctions", key = variable {
        /// #0 H: variable key.
        variable: String,
        /// #1 H: campaign key, e.g. `eur_napoleon`.
        campaign: String,
        /// #2 H: value.
        value: f32,
    }
}

db_record! {
    /// `building_effects_junction` (`ssf`, 354 rows): an effect a building level gives.
    pub struct BuildingEffect in "building_effects_junction", key = building {
        /// #0 H: building level key.
        building: String,
        /// #1 H: effect key (FK `effects`), e.g. `recruitment_points`, `gdp_industry`.
        effect: String,
        /// #2 H: value.
        value: f32,
    }
}

db_record! {
    /// `building_units_allowed` (`ssib`, 1708 rows): units a building level lets a region recruit.
    pub struct BuildingUnitAllowed in "building_units_allowed", key = building {
        /// #0 H: building level key.
        building: String,
        /// #1 H: unit key (FK `units`).
        unit: String,
        /// #2: UNKNOWN (0 in the rows checked).
        unknown_i: i32,
        /// #3: UNKNOWN flag.
        unknown_b: bool,
    }
}

db_record! {
    /// `building_upgrades_junction` (`ss`, 89 rows): a level and the level it upgrades to.
    pub struct BuildingUpgrade in "building_upgrades_junction", key = building {
        /// #0 H: building level key.
        building: String,
        /// #1 H: the level it upgrades to.
        upgrade: String,
    }
}

db_record! {
    /// `building_chains` (`sooo`, 48 rows): the chains (row reader `0x00E550D0`, DB_BUILDERS.md).
    pub struct BuildingChain in "building_chains", key = key {
        /// #0 H: chain key, e.g. `sAdmin`.
        key: String,
        /// #1: an optional key (`enlightenment`, `military`, `industry` on three chains; UNKNOWN use).
        unknown_1: Option<String>,
        /// #2: an optional number (`0` sAdmin, `1` sArmy, `2` tFactory), parsed into chain record +0x18
        /// (`0x004F3720`: empty = 0; CONFIRMED). The sabotage chance reads it.
        class_number: Option<String>,
        /// #3: the chain's category (`military`, `money`, `government`, ...).
        category: Option<String>,
    }
}

db_record! {
    /// `building_chain_to_slots` (`ss`, 82 rows): which slot types a building chain may stand in.
    pub struct BuildingChainSlot in "building_chain_to_slots", key = chain {
        /// #0 H: chain key (FK `building_chains`), e.g. `sRoads`.
        chain: String,
        /// #1 H: slot type, e.g. `settlement_road`, `settlement_3_slot`, `port`.
        slot_type: String,
    }
}

db_record! {
    /// `taxes_levels` (`si`, 5 rows): tax level and its rate (INFERRED percent: 5..25).
    pub struct TaxLevel in "taxes_levels", key = key {
        /// #0 H: level key, e.g. `tax_normal`.
        key: String,
        /// #1 M: rate in percent.
        rate: i32,
    }
}

db_record! {
    /// `taxes_keys` (`sss`, 10 rows): (class, tax level) → effect bundle.
    pub struct TaxKey in "taxes_keys", key = class {
        /// #0 H: class, `lower_classes` / `upper_classes`.
        class: String,
        /// #1 H: tax level key.
        level: String,
        /// #2 H: effect bundle key, e.g. `lower_high`.
        bundle: String,
    }
}

db_record! {
    /// `taxes_effects_jct` (`ssf`, 55 rows): effects of a tax bundle.
    pub struct TaxEffect in "taxes_effects_jct", key = bundle {
        /// #0 H: bundle key.
        bundle: String,
        /// #1 H: effect key, e.g. `happy_active_lower_tax`.
        effect: String,
        /// #2 H: value.
        value: f32,
    }
}

db_record! {
    /// `unit_required_technology_junctions` (`ss`, 46 rows): a unit and the technology it needs.
    pub struct UnitTechnologyRecord in "unit_required_technology_junctions", key = unit {
        /// #0 H: the unit key.
        unit: String,
        /// #1 H: the technology.
        technology: String,
    }
}

db_record! {
    /// `building_level_required_technology_junctions` (`ss`, 21 rows): a building level and the technology
    /// it needs.
    pub struct BuildingTechnologyRecord in "building_level_required_technology_junctions", key = building {
        /// #0 H: the building level key.
        building: String,
        /// #1 H: the technology.
        technology: String,
    }
}

db_record! {
    /// `building_faction_variants` (`ssooo`, 44 rows): a building level one faction may build, with
    /// its pictures (e.g. `sPrest_france_arcdetriomphe` / `france`; the Peninsular campaign's Spain
    /// chains for the `spa_*` factions).
    pub struct BuildingFactionVariant in "building_faction_variants", key = building {
        /// #0 H: the building level key.
        building: String,
        /// #1 H: the faction key.
        faction: String,
        /// #2: picture / model name (UNKNOWN use).
        art_a: Option<String>,
        /// #3: description-text key.
        description: Option<String>,
        /// #4: icon name.
        icon: Option<String>,
    }
}

db_record! {
    /// `building_culture_variants` (`ssooooo`, 264 rows): a building level one culture
    /// (`cultures_subcultures` #1, e.g. `european`, `egy_middle_east`) may build, with its pictures.
    pub struct BuildingCultureVariant in "building_culture_variants", key = building {
        /// #0 H: the building level key.
        building: String,
        /// #1 H: the culture variant.
        culture: String,
        /// #2: UNKNOWN (model).
        art_a: Option<String>,
        /// #3: icon name.
        icon_a: Option<String>,
        /// #4: UNKNOWN.
        art_b: Option<String>,
        /// #5: description-text key.
        description: Option<String>,
        /// #6: icon name.
        icon_b: Option<String>,
    }
}

db_record! {
    /// `technology_required_technology_junctions` (`ss`, 19 rows): a technology and one technology it needs.
    pub struct TechnologyRequirementRecord in "technology_required_technology_junctions", key = technology {
        /// #0 H: the technology.
        technology: String,
        /// #1 H: the technology it requires.
        required: String,
    }
}

db_record! {
    /// `technology_faction_junctions` (`ss`, 3212 rows): a faction that may research a technology.
    pub struct TechnologyFactionRecord in "technology_faction_junctions", key = technology {
        /// #0 H: the technology.
        technology: String,
        /// #1 H: the faction key.
        faction: String,
    }
}

db_record! {
    /// `government_types` (`sbbiss`, 4 rows, CONFIRMED layout from the vanilla file): a government type
    /// and the two population classes it governs. The exe compares a class with the type's lower class
    /// (government +0xB0 → +0x1C, `0x008CE210`) to pick the lower or upper government-type and tax
    /// happiness; a class that is neither gets no public-order factors (`0x008EDF70`).
    pub struct GovernmentTypeRecord in "government_types", key = key {
        /// #0 H: government key, e.g. `gov_constitutional_monarchy`.
        key: String,
        /// #1: UNKNOWN flag (true for absolute monarchy and republic).
        flag_1: bool,
        /// #2: UNKNOWN flag (true for absolute monarchy and empire).
        flag_2: bool,
        /// #3: UNKNOWN (3, 2, 1, 0).
        value: i32,
        /// #4 H: the upper class (`upper`, or `middle` for a republic and an empire).
        upper_class: String,
        /// #5 H: the lower class (`lower`, or `middle` for a constitutional monarchy).
        lower_class: String,
    }
}

db_record! {
    /// `government_types_to_effects` (`ssf`, 24 rows): effects of a government type.
    pub struct GovernmentEffect in "government_types_to_effects", key = government {
        /// #0 H: government key, e.g. `gov_empire` (the ESF `GOVERNMENT` #1 string).
        government: String,
        /// #1 H: effect key.
        effect: String,
        /// #2 H: value.
        value: f32,
    }
}

db_record! {
    /// `agents` (exe reader 0x00F94940, 12 columns, file v1): one row per character type.
    pub struct AgentRecord in "agents", key = key {
        /// #0 @0x00 H: agent type, the ESF `CHARACTER` #3 string (`General`, `colonel`, ...).
        key: String,
        /// #1 @0x18 H: base action points (CONFIRMED: equals `LOCOMOTABLE` #8 of every
        /// character of the type in the eur startpos and in saves: General 26, admiral 90, ...).
        action_points: i32,
        /// #2 @0x1C: sight radius in map units (character +0x2EC = record +0x10, CONFIRMED against
        /// the saved sight boxes; slot 0-G, CHARACTERS_FIDELITY.md §10).
        unknown_1c: i32,
        /// #3 @0x20: UNKNOWN (0..20).
        unknown_20: i32,
        /// #4 @0x24: UNKNOWN flag (false only for minister and bandit).
        unknown_24: bool,
        /// #5 @0x28 M: model kind, `human` / `ship`.
        model_kind: String,
        /// #6 @0x34 L: base agent type.
        base_agent: Option<String>,
        /// #7 @0x40: UNKNOWN flag.
        unknown_40: bool,
        /// #8 @0x44 M: primary attribute, e.g. `command_land`.
        primary_attribute: String,
        /// #9 @0x50: the religion a missionary converts to (`rel_*`, or an alignment in spa; agent record +0x1A0, read by the conversion step `0x00A63FE0`, CONFIRMED use).
        unknown_50: Option<String>,
        /// #10 @0x5C: UNKNOWN.
        unknown_5c: i32,
        /// #11 @0x60 {v>=1}: UNKNOWN.
        unknown_60: i32 => since 1,
    }
}

db_record! {
    /// `units_to_exclusive_faction_permissions` (`ssb`, 1012 rows): a unit limited to factions.
    /// INFERRED rule: a unit with rows here may be recruited only by the factions whose row
    /// holds `true`.
    pub struct UnitFactionPermission in "units_to_exclusive_faction_permissions", key = unit {
        /// #0 H: unit key.
        unit: String,
        /// #1 H: faction key.
        faction: String,
        /// #2 M: allowed.
        allowed: bool,
    }
}

db_record! {
    /// `campaign_map_slots` (`sssib`, 145 rows): resource slots on the map.
    pub struct MapSlotRecord in "campaign_map_slots", key = key {
        /// #0 H: slot key, the ESF `REGION_SLOT` #3 string, e.g. `gold:eur_austria:rauris`.
        key: String,
        /// #1 H: region key.
        region: String,
        /// #2 H: slot type (FK `slots`), e.g. `gold`.
        slot_type: String,
        /// #3: UNKNOWN.
        unknown_i: i32,
        /// #4: UNKNOWN.
        unknown_b: bool,
    }
}

db_record! {
    /// `campaign_map_towns_and_ports` (`sss`, 233 rows): towns and ports on the map.
    pub struct MapTownRecord in "campaign_map_towns_and_ports", key = key {
        /// #0 H: slot key, e.g. `town:eur_austria:graz`.
        key: String,
        /// #1 H: slot type, `town-commercial` / `town-industrial` / `town-intellectual` / `port`.
        slot_type: String,
        /// #2 H: display name.
        name: String,
    }
}

db_record! {
    /// `slots_art` (exe row reader `0x00F06D20`, 13 columns, file v0; `DB_BUILDERS.md`): the art a
    /// map slot draws -- its diffuse, its one or two **model template** keys and its own model stem,
    /// per (slot type, culture). 72 shipped rows = **12 slot types x 6 cultures**, which is what
    /// makes the key `slot_type` + `culture` rather than either alone (INFERRED; the exe looks the
    /// row up by one composite string).
    ///
    /// The two template columns are the part that was missing: **#4 and #6 are foreign keys into
    /// `slots_templates_models`**, CONFIRMED over all 72 shipped rows (every value either absent or
    /// a key of that table; install test `slots_install::the_slot_art_templates_are_template_keys`).
    /// #4 is the one the exe reads at the row's `+0x30` in the settlement fortification model
    /// builder `0x00B42B90`, which then appends the literal `_slot_fortifications_lvl` and the
    /// fortification level (an exe reading whose decompile is not kept: INFERRED; the `@0x..`
    /// record offsets below are the row reader's, from the same unkept reading).
    pub struct SlotArtRecord in "slots_art", key = slot_type {
        /// #0 @0x0C H: slot type (the FK `campaign_map_slots` #2 uses), e.g. `fort`,
        /// `town-commercial`, `gold`.
        slot_type: String,
        /// #1 @0x18 H: the culture the art is for: `european`, `indian`, `middle_east`,
        /// `tribal`, `egy_european`, `egy_middle_east`.
        culture: String,
        /// #2 @0x24: diffuse texture name, e.g. `eu_city_diffuse`. Absent when the slot draws no
        /// own texture (the fort rows have none).
        diffuse: Option<String>,
        /// #3 @0x68: `#2` is present. CONFIRMED over the 72 shipped rows: true in exactly the rows
        /// that carry a diffuse name.
        has_diffuse: bool,
        /// #4 @0x30: model template key (FK `slots_templates_models`), e.g. `EU_Settlement`,
        /// `nap_eur_town_com`. This is the string `0x00B42B90` reads at `+0x30`.
        template: Option<String>,
        /// #5 @0x69: `#6` is present (CONFIRMED: true in exactly the port and town rows, which
        /// also have a `#9`/`#12` of 1).
        has_second_template: bool,
        /// #6 @0x3C: the second template key (FK `slots_templates_models`), e.g. `EU_Village` --
        /// the model of the little building that stands in the slot.
        second_template: Option<String>,
        /// #7 @0x6A: UNKNOWN (false in every shipped row).
        unknown_6a: bool,
        /// #8 @0x6B: UNKNOWN (false in every shipped row).
        unknown_6b: bool,
        /// #9 @0x60: 1 for the settlement slots (town / port / settlement) and 0 for the resource
        /// and fort slots. CONFIRMED over the 72 shipped rows. Could be read as f32 by the exe;
        /// the value is only ever 0 or 1.
        is_settlement_slot: i32,
        /// #10 @0x48: the slot's own model stem, e.g. `Fort_lvl1`, `slot_resource_mining_gold_lvl0`,
        /// `EU_building_placeholder`. **The region fort's model name is this** -- `fort` rows say
        /// `Fort_lvl1` for every one of the six cultures.
        model: Option<String>,
        /// #11 @0x54: a second model stem, used where #10 is empty (e.g. `horses` says
        /// `slot_resource_pasture`).
        model_alt: Option<String>,
        /// #12 @0x64: same 0/1 split as #9 (1 for settlement slots).
        is_settlement_slot_2: i32,
    }
}

db_record! {
    /// `slots_templates_models` (exe row reader `0x00E53FB0`, 3 columns, file v0; 28 shipped rows):
    /// a model template key -> the model name to draw and the folder it lives in. The folder is
    /// what `0x00B42B90` reaches through the game data at `+0x30` to build
    /// `<folder>/<model>_slot_fortifications_lvl<level>`.
    pub struct SlotTemplateModelRecord in "slots_templates_models", key = key {
        /// #0 @0x00 H: the key `slots_art` #4 / #6 name, e.g. `EU_Settlement`, `EU_Village`.
        key: String,
        /// #1 @0x0C H: the model name, e.g. `EU_City`, `EU_Town_Village_slot`. The settlement's
        /// on-map models are its lower-cased stem (`eu_city`, `eu_town_village`).
        model: String,
        /// #2 @0x18 H: the folder inside the pack, e.g. `RigidModels/CampaignBuildings/Templates/EU`.
        folder: String,
    }
}

db_record! {
    /// `trade_nodes` (`ssiffs`, 24 rows): the off-map trade nodes (row reader 0x00FBF5C0; read by
    /// `0x00BC9930` at record +0x0C commodity, +0x10 base volume, +0x14 per extra ship, +0x18 cap,
    /// CONFIRMED).
    pub struct TradeNodeRecord in "trade_nodes", key = key {
        /// #0 H: node key, e.g. `eur_aegean_sea_01`.
        key: String,
        /// #1 H: commodity (resource key, e.g. `res_coffee`).
        commodity: String,
        /// #2 H: base volume.
        base: i32,
        /// #3 H: volume factor per extra ship.
        per_ship: f32,
        /// #4 H: cap of the factor.
        cap: f32,
        /// #5 M: destination theatre.
        theatre: String => since 1,
    }
}

db_record! {
    /// `diplomatic_relations_religion` (`ssif`, 121 rows): per (religion, other religion) a diplomatic
    /// attitude (#2) and the unrest factor (#3) the religion factor of public order reads (`0x00F55820`
    /// `RELIGIONS_RELATIONS_MOD_RECORD`, key = region religion + state religion; CONFIRMED by the vanilla
    /// saves, CAMPAIGN_FIDELITY.md §Public order).
    pub struct ReligionRelationRecord in "diplomatic_relations_religion", key = religion {
        /// #0 H: religion key (the region's religion).
        religion: String,
        /// #1 H: the other religion (the owner's state religion).
        other: String,
        /// #2 M: diplomatic attitude between the two.
        attitude: i32,
        /// #3 H: the public-order unrest factor.
        value: f32,
    }
}

db_record! {
    /// `commodities_demand_junction` (`ssff`, 16 rows): a commodity's demand drivers (read by
    /// `0x00AB49F0`, CONFIRMED: demand += round(#3 × #2 × driver value)).
    pub struct CommodityDemandRecord in "commodities_demand_junction", key = commodity {
        /// #0 H: commodity key.
        commodity: String,
        /// #1 H: demand driver (`ddr_GDP`, `ddr_TW`, `ddr_textile_production`, ...).
        driver: String,
        /// #2 H: factor.
        factor: f32,
        /// #3 H: weight.
        weight: f32,
    }
}

db_record! {
    /// `diplomatic_relations_government_type` (`ssii`, 16 rows): per (government, other government) the
    /// `government_type` attitude factor. The campaign setup writes the fixed #3 (`0x00B45A40`,
    /// CONFIRMED); a government change (`0x00B1B5A0`, CONFIRMED from the bytes) sets the factor to #2
    /// with #3 as the limit, so it walks #2 -> #3 at the `government_type` event's drift of 2.
    pub struct GovernmentRelationRecord in "diplomatic_relations_government_type", key = government {
        /// #0 H: government key.
        government: String,
        /// #1 H: the other government key.
        other: String,
        /// #2: the value a government change shocks the factor to (`[row + 8]`, `0x00B1B686`).
        limit: i32,
        /// #3: the steady value, the limit a government change drifts to and the setup's fixed
        /// value (`[row + 0xC]`, `0x00B1B684` / `0x00B45BCB`).
        value: i32,
    }
}

db_record! {
    /// `diplomatic_relations_attitudes` (`si`, 5 rows): the attitude thresholds (hostile -85, unfriendly -45,
    /// neutral 0, friendly 45, very_friendly 85). The attitude category `0x00B0DBA0` puts its boundaries
    /// half-way between neighbouring rows (CONFIRMED).
    pub struct AttitudeThresholdRecord in "diplomatic_relations_attitudes", key = key {
        /// #0 H: `hostile`, `unfriendly`, `neutral`, `friendly`, `very_friendly`.
        key: String,
        /// #1 H: the threshold.
        value: i32,
    }
}

db_record! {
    /// `building_chains` (`sooo`, 48 rows): a building chain. Row reader `0x00E550D0`: #2 is also
    /// parsed as an integer into record +0x18 (CONFIRMED from the reader; empty → 0). The capture
    /// preview `0x00B14930` damages on occupation only the buildings whose chain has 0 there
    /// (`sArmy` 1 and `tFactory` 2 are spared; CONFIRMED use).
    pub struct BuildingChainRecord in "building_chains", key = key {
        /// #0 H: chain key, e.g. `sAdmin`.
        key: String,
        /// #1: the chain's group (`enlightenment`, `military`, `industry`, ...).
        group: Option<String>,
        /// #2: a small integer as text (`0` sAdmin, `1` sArmy, `2` tFactory; empty elsewhere).
        kind: Option<String>,
        /// #3: the chain's category (`government`, `military`, `money`, ...).
        category: Option<String>,
    }
}

db_record! {
    /// `unit_stats_naval` (121 columns, file v2; row reader `0x00F05BA0`): per ship type. Only a few columns
    /// have known meanings (the naval autoresolve inputs, CAMPAIGN_FIDELITY.md §Naval autoresolve); the numbered
    /// fields keep the raw 32 bits (some hold floats: use `f32::from_bits`).
    pub struct NavalStatsRecord in "unit_stats_naval", key = key {
        /// #0: the ship type key (`units` key of naval units).
        key: String,
        /// #1: UNKNOWN.
        c1: i32,
        /// #2: UNKNOWN.
        c2: i32,
        /// #3: UNKNOWN.
        c3: i32,
        /// #4: a gun deck's gun type (`naval_*_pounder`).
        c4: Option<String>,
        /// #5: a gun deck's gun type (`naval_*_pounder`).
        c5: Option<String>,
        /// #6: a gun deck's gun type (`naval_*_pounder`).
        c6: Option<String>,
        /// #7: a gun deck's gun type (`naval_*_pounder`).
        c7: Option<String>,
        /// #8: a gun deck's gun type (`naval_*_pounder`).
        c8: Option<String>,
        /// #9: a gun deck's gun type (`naval_*_pounder`).
        c9: Option<String>,
        /// #10: a gun deck's gun type (`naval_*_pounder`).
        c10: Option<String>,
        /// #11: a gun deck's gun type (`naval_*_pounder`).
        c11: Option<String>,
        /// #12: the ship model.
        c12: String,
        /// #13: the admiral type.
        c13: String,
        /// #14: an officer type.
        c14: Option<String>,
        /// #15: an officer type.
        c15: Option<String>,
        /// #16: an officer type.
        c16: Option<String>,
        /// #17..#19: the three crews (equal to the saved `SHIP_DAMAGE_INFO` crews of a new ship).
        c17: i32,
        c18: i32,
        c19: i32,
        c20: i32,
        c21: i32,
        /// #22: the hand weapon (`matchlock`).
        c22: String,
        /// #23: raw.
        c23: i32,
        /// #24: raw.
        c24: i32,
        /// #25: raw.
        c25: i32,
        /// #26: raw.
        c26: i32,
        /// #27: raw.
        c27: i32,
        /// #28: raw.
        c28: i32,
        /// #29: raw.
        c29: i32,
        /// #30: raw.
        c30: i32,
        /// #31: raw.
        c31: i32,
        /// #32: raw.
        c32: i32,
        /// #33: raw.
        c33: i32,
        /// #34: raw.
        c34: i32,
        /// #35: raw.
        c35: i32,
        /// #36: raw.
        c36: i32,
        /// #37: raw.
        c37: i32,
        /// #38: raw.
        c38: i32,
        /// #39: raw.
        c39: i32,
        /// #40: raw.
        c40: i32,
        /// #41: raw.
        c41: i32,
        /// #42: raw.
        c42: i32,
        /// #43: raw.
        c43: i32,
        /// #44: raw.
        c44: i32,
        /// #45: raw.
        c45: i32,
        /// #46: raw.
        c46: i32,
        /// #47: raw.
        c47: i32,
        /// #48: raw.
        c48: i32,
        /// #49: raw.
        c49: i32,
        /// #50: raw.
        c50: i32,
        /// #51: raw.
        c51: i32,
        /// #52: raw.
        c52: i32,
        /// #53: raw.
        c53: i32,
        /// #54: raw.
        c54: i32,
        /// #55: raw.
        c55: i32,
        /// #56: raw.
        c56: i32,
        /// #57: raw.
        c57: i32,
        /// #58: raw.
        c58: i32,
        /// #59: raw.
        c59: i32,
        /// #60: UNKNOWN flag.
        c60: bool,
        /// #61..#114: 18 ship-part triples {i32, i32 hit points, f32 sink weight}.
        c61: i32,
        c62: i32,
        c63: i32,
        c64: i32,
        c65: i32,
        c66: i32,
        c67: i32,
        c68: i32,
        c69: i32,
        c70: i32,
        c71: i32,
        c72: i32,
        c73: i32,
        c74: i32,
        c75: i32,
        c76: i32,
        c77: i32,
        c78: i32,
        c79: i32,
        c80: i32,
        c81: i32,
        c82: i32,
        c83: i32,
        c84: i32,
        c85: i32,
        c86: i32,
        c87: i32,
        c88: i32,
        c89: i32,
        c90: i32,
        c91: i32,
        c92: i32,
        c93: i32,
        c94: i32,
        c95: i32,
        c96: i32,
        c97: i32,
        c98: i32,
        c99: i32,
        c100: i32,
        c101: i32,
        c102: i32,
        c103: i32,
        c104: i32,
        c105: i32,
        c106: i32,
        c107: i32,
        c108: i32,
        c109: i32,
        c110: i32,
        c111: i32,
        c112: i32,
        c113: i32,
        c114: i32,
        /// #115: UNKNOWN flag.
        c115: bool,
        /// #116: UNKNOWN flag.
        c116: bool,
        /// #117: UNKNOWN flag.
        c117: bool,
        /// #118: a size class (`Low`, ...).
        c118: String,
        /// #119 {v>=1}: UNKNOWN.
        c119: i32 => since 1,
        /// #120 {v>=2}: propulsion (`sail`, ...).
        c120: String => since 2,
    }
}

db_record! {
    /// `campaign_ground_types` (`sfbbb`, 19 rows, v1): the campaign map's ground types, which
    /// `regions.esf` `groundtypes` paints on the map (CHARACTERS_FIDELITY.md §10).
    pub struct CampaignGroundType in "campaign_ground_types", key = key {
        /// #0 H: key, e.g. `light_forest`, `hilly_light_forest_cold_att`.
        key: String,
        /// #1: UNKNOWN f32 (plains 1.3, grassland 0.9, light forest 0.5, swamp 0.2, dense forest 0).
        unknown_1: f32,
        /// #2: an army can hide here (ground record +0x14, read by the stealth test `0x009D1010`;
        /// CONFIRMED by the hidden flags of the vanilla saves: set for `light_forest`,
        /// `hilly_light_forest` and their `_cold_att` forms).
        hides: bool,
        /// #3: UNKNOWN flag (only `desert`; INFERRED heat attrition).
        unknown_3: bool,
        /// #4: UNKNOWN flag (the `_cold_att` types; INFERRED cold attrition).
        unknown_4: bool,
    }
}

db_record! {
    /// `agent_culture_details` (`ssso`, 53 rows, v0): per agent type and culture, the campaign model and, for
    /// `General`, the unit a new general leads (`european` → `Gen_Generals_Staff`, `middle_east` →
    /// `Gen_Generals_Bodyguard`; read through the agent record by `0x008E27D0`, CHARACTERS_FIDELITY.md §8).
    pub struct AgentCultureDetail in "agent_culture_details", key = agent {
        /// #0 H: agent type (`agents` key).
        agent: String,
        /// #1 H: culture (`cultures` key).
        culture: String,
        /// #2 M: campaign model.
        campaign_model: String,
        /// #3 H: the unit a new character of this type leads (generals only).
        unit: Option<String>,
    }
}

db_record! {
    /// `historical_characters` (`sbsssiis`, 505 rows): the named characters a faction may receive as
    /// recruitment-pool candidates. The exe condition `CanGenerateHistoricalCharacter` (`0x0089B830`,
    /// CHARACTERS_FIDELITY.md §8) takes a row when its faction and agent type match the pool, the year
    /// lies in `year_from..=year_to` and the key is not in the campaign model's
    /// `HISTORICAL_CHARACTER_MANAGER` created list; `historical_character` (`0x008C9580`) then offers it.
    pub struct HistoricalCharacter in "historical_characters", key = key {
        /// #0: the key, e.g. `eur_jean_rapp` (campaign prefix `eur_` / `spa_` / `egy_` / `ita_`).
        key: String,
        /// #1: a flag (true for a few admirals; meaning UNKNOWN, not read by the condition).
        flag_1: bool,
        /// #2: gender, `m` or `f`.
        gender: String,
        /// #3: the agent type (`agents` key: `General`, `admiral`, `gentleman`, ...).
        agent: String,
        /// #4: the faction key.
        faction: String,
        /// #5: the first year he may appear.
        year_from: i32,
        /// #6: the last year.
        year_to: i32,
        /// #7: a note (`n`, a trait hint such as `math4`, `British Admiral`; read only in the
        /// Peninsular campaign by the condition: UNKNOWN use).
        note: String,
    }
}

/// Every campaign-map table this crate loads. Empty in [`crate::GameDatabase::test_fixture`].
#[derive(Debug, Clone, Default)]
pub struct CampaignTables {
    /// `campaign_variables`.
    pub variables: Table<CampaignVariable>,
    /// `campaigns_campaign_variables_junctions`.
    pub variable_overrides: Table<CampaignVariableOverride>,
    /// `building_effects_junction`.
    pub building_effects: Table<BuildingEffect>,
    /// `building_units_allowed`.
    pub building_units: Table<BuildingUnitAllowed>,
    /// `building_upgrades_junction`.
    pub building_upgrades: Table<BuildingUpgrade>,
    /// `diplomatic_relations_government_type`.
    pub government_relations: Table<GovernmentRelationRecord>,
    /// `diplomatic_relations_attitudes`.
    pub attitude_thresholds: Table<AttitudeThresholdRecord>,
    /// `unit_stats_naval`.
    pub naval_stats: Table<NavalStatsRecord>,
    /// `building_chains`.
    pub chains: Table<BuildingChainRecord>,
    /// `building_chain_to_slots`.
    pub chain_slots: Table<BuildingChainSlot>,
    /// `building_chains`.
    pub building_chains: Table<BuildingChain>,
    /// `taxes_levels`.
    pub tax_levels: Table<TaxLevel>,
    /// `taxes_keys`.
    pub tax_keys: Table<TaxKey>,
    /// `taxes_effects_jct`.
    pub tax_effects: Table<TaxEffect>,
    /// `government_types_to_effects`.
    pub government_effects: Table<GovernmentEffect>,
    /// `agents`.
    pub agents: Table<AgentRecord>,
    /// `units_to_exclusive_faction_permissions`.
    pub unit_factions: Table<UnitFactionPermission>,
    /// `campaign_map_slots`.
    pub map_slots: Table<MapSlotRecord>,
    /// `campaign_map_towns_and_ports`.
    pub map_towns: Table<MapTownRecord>,
    /// `slots_art`: the art a map slot draws, per (slot type, culture) -- see
    /// [`slot_art`](CampaignTables::slot_art).
    pub slot_art: Table<SlotArtRecord>,
    /// `slots_templates_models`: model template key -> model name + pack folder.
    pub slot_templates_models: Table<SlotTemplateModelRecord>,
    /// `trade_nodes`.
    pub trade_nodes: Table<TradeNodeRecord>,
    /// `technology_required_technology_junctions`.
    pub tech_requirements: Table<TechnologyRequirementRecord>,
    /// `technology_faction_junctions`.
    pub tech_factions: Table<TechnologyFactionRecord>,
    /// `unit_required_technology_junctions`.
    pub unit_techs: Table<UnitTechnologyRecord>,
    /// `building_level_required_technology_junctions`.
    pub building_techs: Table<BuildingTechnologyRecord>,
    /// `building_faction_variants`.
    pub building_faction_variants: Table<BuildingFactionVariant>,
    /// `building_culture_variants`.
    pub building_culture_variants: Table<BuildingCultureVariant>,
    /// `government_types`.
    pub government_types: Table<GovernmentTypeRecord>,
    /// `diplomatic_relations_religion`.
    pub religion_relations: Table<ReligionRelationRecord>,
    /// `commodities_demand_junction`.
    pub commodity_demand: Table<CommodityDemandRecord>,
    /// `campaign_ground_types`.
    pub ground_types: Table<CampaignGroundType>,
    /// `agent_culture_details`.
    pub agent_cultures: Table<AgentCultureDetail>,
    /// `historical_characters`: the named recruitment-pool candidates (CHARACTERS_FIDELITY.md §8).
    pub historical_characters: Table<HistoricalCharacter>,
    /// The effect tables (slot 0-F).
    pub effects: crate::effects::EffectTables,
    /// The character tables (slot 0-G).
    pub characters: crate::characters::CharacterTables,
}

impl CampaignTables {
    /// A variable's value for a campaign: the per-campaign override if there is one, else the
    /// global value.
    pub fn variable(&self, campaign: &str, key: &str) -> Option<f32> {
        self.variable_overrides
            .iter()
            .find(|o| o.variable == key && o.campaign == campaign)
            .map(|o| o.value)
            .or_else(|| self.variables.get(key).map(|v| v.value))
    }

    /// The `slots_art` row for one slot type and culture. The table's own key is the slot type
    /// alone, which is not unique, so both columns are matched (the shipped table is exactly
    /// 12 slot types x 6 cultures).
    pub fn slot_art(&self, slot_type: &str, culture: &str) -> Option<&SlotArtRecord> {
        self.slot_art
            .iter()
            .find(|r| r.slot_type == slot_type && r.culture == culture)
    }

    /// The `slots_templates_models` row for a key -- the model name and its pack folder.
    pub fn slot_template_model(&self, key: &str) -> Option<&SlotTemplateModelRecord> {
        self.slot_templates_models.get(key)
    }
}

impl SlotArtRecord {
    /// The model stem this slot draws: column #10 when it is set, else column #11.
    ///
    /// CONFIRMED over the 72 shipped rows: every slot type has one of the two, and the rows where
    /// #10 is empty (`horses`) put `slot_resource_pasture` in #11.
    pub fn model_stem(&self) -> Option<&str> {
        self.model.as_deref().or(self.model_alt.as_deref())
    }
}

/// The literal the exe appends to a settlement template's own name to ask for that settlement's
/// fortification mesh. The spelling is CONFIRMED by the shipped files, which carry exactly this
/// infix (install test `slots_install`). That the string occurs once in `Napoleon.exe`, at
/// `0x0137D604`, referenced only from `0x00B42B90`, is an exe reading whose dump is not kept in
/// the repo or the sandbox evidence: INFERRED.
pub const SLOT_FORTIFICATIONS_SUFFIX: &str = "_slot_fortifications_lvl";

/// The slot type `slots_art` uses for a settlement's own slot (the one that carries the model
/// template the fortification mesh is named from).
pub const SLOT_TYPE_SETTLEMENT: &str = "settlement";

impl SlotTemplateModelRecord {
    /// The pack folder this template's models live in, with the pack's own `\` separators.
    pub fn folder_path(&self) -> String {
        self.folder.replace('/', "\\")
    }

    /// The template's own mesh for slot `<n>`: `<folder>\<stem>_<n>_slot.rigid_model` -- the model
    /// the campaign map draws for a settlement with no walls (`eu_city_2_slot.rigid_model`).
    pub fn slot_model(&self, n: u32) -> String {
        format!("{}\\{}_{n}_slot.rigid_model", self.folder_path(), self.model.to_ascii_lowercase())
    }

    /// The settlement's **fortification** mesh for slot `<n>` at file level `<level>`:
    /// `<folder>\<stem>_<n>_slot_fortifications_lvl<level>.rigid_model`, the name `0x00B42B90`
    /// builds by appending [`SLOT_FORTIFICATIONS_SUFFIX`] and the level.
    ///
    /// `<level>` is read as the `sFortifications` chain level **+ 1**, so `0` would be the "not
    /// fortified yet" mesh -- INFERRED. CONFIRMED from the install only that the three files exist
    /// and rise in vertex count `_lvl0 < _lvl1 < _lvl2` for every culture and every `<n>` that has
    /// any of them (install test
    /// `slots_install::the_settlement_fortification_levels_are_a_rising_ladder`); a vertex ladder
    /// does not say which building level asks for which file.
    pub fn fortification_slot_model(&self, n: u32, level: u32) -> String {
        format!("{}\\{}_{n}{SLOT_FORTIFICATIONS_SUFFIX}{level}.rigid_model", self.folder_path(), self.model.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::{SLOT_FORTIFICATIONS_SUFFIX, SLOT_TYPE_SETTLEMENT, SlotTemplateModelRecord};

    /// The two names `0x00B42B90` builds, spelled the way the shipped files are spelled: the
    /// template's own mesh, and that same name with the literal suffix and the level appended.
    /// The install test `the_settlement_fortification_levels_are_a_rising_ladder` reads the files
    /// these produce.
    #[test]
    fn the_settlement_mesh_names_follow_the_templates_own_stem() {
        let eu = SlotTemplateModelRecord {
            key: "EU_Settlement".into(),
            model: "EU_City".into(),
            folder: "RigidModels/CampaignBuildings/Templates/EU".into(),
        };
        assert_eq!(eu.folder_path(), r"RigidModels\CampaignBuildings\Templates\EU");
        assert_eq!(eu.slot_model(2), r"RigidModels\CampaignBuildings\Templates\EU\eu_city_2_slot.rigid_model");
        for (level, want) in [
            (0, r"RigidModels\CampaignBuildings\Templates\EU\eu_city_2_slot_fortifications_lvl0.rigid_model"),
            (1, r"RigidModels\CampaignBuildings\Templates\EU\eu_city_2_slot_fortifications_lvl1.rigid_model"),
            (2, r"RigidModels\CampaignBuildings\Templates\EU\eu_city_2_slot_fortifications_lvl2.rigid_model"),
        ] {
            assert_eq!(eu.fortification_slot_model(2, level), want);
        }
        // The suffix is the exe's own literal, so a typo here would be a silent miss at load time.
        assert_eq!(SLOT_FORTIFICATIONS_SUFFIX, "_slot_fortifications_lvl");
        assert_eq!(SLOT_TYPE_SETTLEMENT, "settlement");
    }
}

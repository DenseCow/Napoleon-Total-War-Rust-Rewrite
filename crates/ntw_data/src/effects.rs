//! Effect tables: what each effect source gives (technologies, buildings, traits, ancillaries,
//! ministers, government types, difficulty) and how a DB effect key maps to the engine's bonus
//! (`effect_bonus_value_*_junction`). Notes: `analysis/fidelity/EFFECTS_FIDELITY.md` (slot 0-F).
//! Layouts: `analysis/worker3/DB_CAMPAIGN_TABLES.md` / `analysis/worker2/schemas.md`; every table
//! here parses to its end on the install (`tests/real_install.rs`). Column names INFERRED.

use crate::record::{db_record, Table};

db_record! {
    /// `effect_bonus_value_basic_junction` (`ss`, 143 rows): a DB effect key → an engine bonus with no
    /// qualifier (e.g. `tw_growth_industry_global` → `tw_growth_industry`). CONFIRMED role (the exe
    /// compiles every source's effects through these junctions at DB load, `0x00F88590`).
    pub struct EffectBonusBasic in "effect_bonus_value_basic_junction", key = effect {
        /// #0 effect key (FK `effects`).
        effect: String,
        /// #1 engine bonus name (the enum at `0x0145B4A8`).
        bonus: String,
    }
}

db_record! {
    /// `effect_bonus_value_unit_category_junction` (`sss`, 35 rows): effect → (bonus, unit category).
    pub struct EffectBonusUnitCategory in "effect_bonus_value_unit_category_junction", key = effect {
        effect: String,
        bonus: String,
        category: String,
    }
}

db_record! {
    /// `effect_bonus_value_unit_class_junction` (`sss`, 6 rows): effect → (bonus, unit class).
    pub struct EffectBonusUnitClass in "effect_bonus_value_unit_class_junction", key = effect {
        effect: String,
        bonus: String,
        class: String,
    }
}

db_record! {
    /// `effect_bonus_value_population_class_junction` (`sss`, 30 rows): effect → (bonus, class:
    /// `lower` / `middle` / `upper`).
    pub struct EffectBonusPopClass in "effect_bonus_value_population_class_junction", key = effect {
        effect: String,
        bonus: String,
        class: String,
    }
}

db_record! {
    /// `effect_bonus_value_religion_junction` (`sss`, 12 rows): effect → (bonus, religion), e.g.
    /// `conversion_catholic` → (`conversion`, `rel_catholic`); spa also keys the alignments (`align_pro_french`).
    pub struct EffectBonusReligion in "effect_bonus_value_religion_junction", key = effect {
        effect: String,
        bonus: String,
        religion: String,
    }
}

db_record! {
    /// `effect_bonus_value_building_chain_junctions` (`sss`): effect → (bonus, building chain), e.g.
    /// `gdp_mod_farms` → (`mod_gdp`, `rFarm`). The exe keys these as bonus type 2 (`mod_cost` 0, `mod_gdp` 1,
    /// `mod_tw_growth` 2, `mod_commodity_production` 3) qualified by the chain.
    pub struct EffectBonusChain in "effect_bonus_value_building_chain_junctions", key = effect {
        effect: String,
        bonus: String,
        chain: String,
    }
}

db_record! {
    /// `religion_conversion_mods` (`ssf`, 74 rows): (religion, other religion, modifier). The exe builds a
    /// religion × religion matrix from it (`0x00877A60`, model +0xF98): row = #0, column = #1 (INFERRED from the
    /// record order); the conversion step reads (losing religion, converting religion).
    pub struct ReligionConversionMod in "religion_conversion_mods", key = religion {
        religion: String,
        other: String,
        value: f32,
    }
}

db_record! {
    /// `effect_bonus_value_agent_junction` (`sss`, 62 rows): effect → (bonus, agent type).
    pub struct EffectBonusAgent in "effect_bonus_value_agent_junction", key = effect {
        effect: String,
        bonus: String,
        agent: String,
    }
}

db_record! {
    /// `technology_effects_junction` (`ssf`, 106 rows).
    pub struct TechnologyEffect in "technology_effects_junction", key = technology {
        technology: String,
        effect: String,
        value: f32,
    }
}

db_record! {
    /// `building_factionwide_effects_junctions` (`ssf`, 98 rows): effects a building level gives its
    /// whole faction.
    pub struct BuildingFactionwideEffect in "building_factionwide_effects_junctions", key = building {
        building: String,
        effect: String,
        value: f32,
    }
}

db_record! {
    /// `character_trait_levels` (`sisi`, 475 rows): level key, level 1..n, trait, threshold points.
    pub struct TraitLevel in "character_trait_levels", key = level_key {
        level_key: String,
        level: i32,
        trait_key: String,
        threshold: i32,
    }
}

db_record! {
    /// `trait_level_effects` (`ssf`, 564 rows).
    pub struct TraitLevelEffect in "trait_level_effects", key = level_key {
        level_key: String,
        effect: String,
        value: f32,
    }
}

db_record! {
    /// `trait_attribute_effects` (`ssi`, 157 rows): a trait level's agent attribute bonus (e.g. research +1).
    pub struct TraitAttributeEffect in "trait_attribute_effects", key = level_key {
        level_key: String,
        attribute: String,
        value: i32,
    }
}

db_record! {
    /// `ancillary_to_attribute_effects` (`ssi`, 58 rows): an ancillary's agent attribute bonus.
    pub struct AncillaryAttributeEffect in "ancillary_to_attribute_effects", key = ancillary {
        ancillary: String,
        attribute: String,
        value: i32,
    }
}

db_record! {
    /// `ancillary_to_effects` (`ssf`, 317 rows).
    pub struct AncillaryEffect in "ancillary_to_effects", key = ancillary {
        ancillary: String,
        effect: String,
        value: f32,
    }
}

db_record! {
    /// `ministerial_positions_to_effects` (`sisii`, 260 rows): post, level 0..9, effect, value, UI
    /// order.
    pub struct MinisterialEffect in "ministerial_positions_to_effects", key = post {
        post: String,
        level: i32,
        effect: String,
        value: i32,
        order: i32,
    }
}

db_record! {
    /// `ministerial_effectiveness_modifiers` (`isi`, 30 rows): (value, government type, modifier)
    /// (INFERRED: a minister's management value → the level modifier under that government).
    pub struct MinisterialEffectiveness in "ministerial_effectiveness_modifiers", key = government {
        value: i32,
        government: String,
        modifier: i32,
    }
}

db_record! {
    /// `campaign_difficulty_handicap_effects` (`ibsf`, 88 rows): difficulty −2..2, the list flag
    /// (human), effect, value (`0x00F9F970`, CONFIRMED lookup).
    pub struct DifficultyHandicapEffect in "campaign_difficulty_handicap_effects", key = effect {
        difficulty: i32,
        human: bool,
        effect: String,
        value: f32,
    }
}

/// Every effect table. Empty in [`crate::GameDatabase::test_fixture`].
#[derive(Debug, Clone, Default)]
pub struct EffectTables {
    pub bonus_basic: Table<EffectBonusBasic>,
    pub bonus_unit_category: Table<EffectBonusUnitCategory>,
    pub bonus_unit_class: Table<EffectBonusUnitClass>,
    pub bonus_pop_class: Table<EffectBonusPopClass>,
    pub bonus_agent: Table<EffectBonusAgent>,
    pub bonus_religion: Table<EffectBonusReligion>,
    pub bonus_chain: Table<EffectBonusChain>,
    pub religion_conversion: Table<ReligionConversionMod>,
    pub technology: Table<TechnologyEffect>,
    pub building_factionwide: Table<BuildingFactionwideEffect>,
    pub trait_levels: Table<TraitLevel>,
    pub trait_level: Table<TraitLevelEffect>,
    pub ancillary: Table<AncillaryEffect>,
    pub trait_attribute: Table<TraitAttributeEffect>,
    pub ancillary_attribute: Table<AncillaryAttributeEffect>,
    pub ministerial: Table<MinisterialEffect>,
    pub ministerial_effectiveness: Table<MinisterialEffectiveness>,
    pub difficulty: Table<DifficultyHandicapEffect>,
}

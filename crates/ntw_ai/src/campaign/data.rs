//! The campaign AI's game data, read from the DB (so mods that change these tables take effect).
//!
//! | table | what the AI uses it for |
//! |---|---|
//! | `campaign_ai_personalities` + `_junctions` | ~245 tunables per personality (`default` + overlays) |
//! | `campaign_ai_managers` + `_manager_behaviour_junctions` | which behaviours a manager runs, and their priorities |
//! | `cdir_unit_balances` | target army composition by army size |
//! | `cdir_unit_qualities` | each unit's quality (strength value) |
//! | `building_chains`, `building_levels` | construction options, costs, chain category |
//! | `units` | unit category, upkeep and the quality stand-in |
//!
//! What a unit costs, whether the faction may recruit it and its difficulty handicap come from the campaign
//! model (`world::AiRecruitable`), not from these tables.

use std::collections::BTreeMap;

use ntw_data::GameDatabase;
use ntw_formats::pack::Vfs;

use crate::tables::{self, TableError, col_bool, col_f32, col_i32, col_str, keys, layouts};

/// One row of `cdir_unit_balances` (CONFIRMED layout; column meanings INFERRED from the values).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitBalance {
    /// Config key (`default`).
    pub config: String,
    /// Smallest army size (units) the row applies to.
    pub min_units: i32,
    /// Largest army size the row applies to.
    pub max_units: i32,
    /// Balance group: `infantry`, `cavalry`, `artillery`, `default_navy`.
    pub group: String,
    /// Target share of the army (0..1).
    pub target: f32,
    /// Allowed deviation from the target (0..1).
    pub tolerance: f32,
    /// Minimum count of this group (e.g. 1 gun from 5 units, 2 from 11).
    pub min_count: i32,
}

/// A unit as the campaign AI sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUnitInfo {
    /// `units.category` (infantry / cavalry / artillery / naval_*).
    pub category: String,
    /// `units.class`.
    pub class: String,
    /// `units` #4, the battle army-setup price: only the stand-in quality of a unit `cdir_unit_qualities` does not
    /// list (`AiWorld::army_strength`, the recruitment pick). Not what recruiting costs: that is the model's entry price, `world::AiRecruitable`.
    pub battle_cost: i32,
    /// Upkeep per turn.
    pub upkeep: i32,
    /// `cdir_unit_qualities` value, if listed.
    pub quality: Option<i32>,
}

/// A building level as the campaign AI sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct AiBuildingInfo {
    /// Chain key.
    pub chain: String,
    /// Level index in the chain.
    pub level: i32,
    /// Construction cost.
    pub cost: i32,
    /// Construction time in turns.
    pub turns: i32,
}

/// Everything the campaign AI reads from the DB.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CampaignAiData {
    /// Personality key → its own tunables (overlays only hold the keys they change).
    pub personalities: BTreeMap<String, BTreeMap<String, f32>>,
    /// The personality flagged as default (`campaign_ai_personalities` col 1, INFERRED meaning).
    pub default_personality: String,
    /// Manager key → behaviour → priority.
    pub managers: BTreeMap<String, BTreeMap<String, f32>>,
    /// `cdir_unit_balances` rows.
    pub unit_balances: Vec<UnitBalance>,
    /// Unit key → info.
    pub units: BTreeMap<String, AiUnitInfo>,
    /// Building level key → info.
    pub buildings: BTreeMap<String, AiBuildingInfo>,
    /// Chain key → category (`military`, `money`, `agriculture`, `research`, `happiness`, `government`).
    pub chain_category: BTreeMap<String, String>,
    /// Mod table files that did not decode and were skipped (one line each; the caller logs them).
    pub load_warnings: Vec<String>,
}

impl CampaignAiData {
    /// Opens the install's packs and loads everything (read-only).
    pub fn from_install(data_dir: impl AsRef<std::path::Path>, db: &GameDatabase) -> Result<Self, TableError> {
        let vfs = Vfs::open_install(data_dir.as_ref())
            .map_err(|e| TableError::Read(data_dir.as_ref().display().to_string(), e.to_string()))?;
        Self::load(&vfs, db)
    }

    /// Loads everything from a mounted VFS and the typed database.
    pub fn load(vfs: &Vfs, db: &GameDatabase) -> Result<Self, TableError> {
        let mut d = CampaignAiData::default();
        let mut warnings = Vec::new();
        let w = &mut warnings;
        for r in tables::load_raw(vfs, "campaign_ai_personalities", layouts::CAMPAIGN_AI_PERSONALITIES, keys::first, w)?.rows {
            let key = col_str(&r, 0).to_string();
            if col_bool(&r, 1) {
                d.default_personality = key.clone();
            }
            d.personalities.entry(key).or_default();
        }
        for r in tables::load_raw(vfs, "campaign_ai_personality_junctions", layouts::CAMPAIGN_AI_PERSONALITY_JUNCTIONS, keys::junction, w)?.rows {
            d.personalities
                .entry(col_str(&r, 0).to_string())
                .or_default()
                .insert(col_str(&r, 1).to_string(), col_f32(&r, 2));
        }
        for r in tables::load_raw(vfs, "campaign_ai_managers", layouts::CAMPAIGN_AI_MANAGERS, keys::first, w)?.rows {
            d.managers.entry(col_str(&r, 0).to_string()).or_default();
        }
        for r in tables::load_raw(vfs, "campaign_ai_manager_behaviour_junctions", layouts::CAMPAIGN_AI_MANAGER_BEHAVIOUR_JUNCTIONS, keys::junction, w)?.rows {
            d.managers
                .entry(col_str(&r, 0).to_string())
                .or_default()
                .insert(col_str(&r, 1).to_string(), col_f32(&r, 2));
        }
        for r in tables::load_raw(vfs, "cdir_unit_balances", "siisffi", keys::cdir_unit_balances, w)?.rows {
            d.unit_balances.push(UnitBalance {
                config: col_str(&r, 0).to_string(),
                min_units: col_i32(&r, 1),
                max_units: col_i32(&r, 2),
                group: col_str(&r, 3).to_string(),
                target: col_f32(&r, 4),
                tolerance: col_f32(&r, 5),
                min_count: col_i32(&r, 6),
            });
        }
        let mut quality = BTreeMap::new();
        for r in tables::load_raw(vfs, "cdir_unit_qualities", layouts::CDIR_UNIT_QUALITIES, keys::cdir_unit_qualities, w)?.rows {
            if col_str(&r, 0) == "default" {
                quality.insert(col_str(&r, 2).to_string(), col_i32(&r, 3));
            }
        }
        for u in db.units.rows() {
            d.units.insert(
                u.key.clone(),
                AiUnitInfo {
                    category: u.category.clone(),
                    class: u.unit_class.clone(),
                    battle_cost: u.recruitment_cost,
                    upkeep: u.upkeep,
                    quality: quality.get(&u.key).copied(),
                },
            );
        }
        for b in db.building_levels.rows() {
            d.buildings.insert(
                b.key.clone(),
                AiBuildingInfo { chain: b.chain.clone(), level: b.level, cost: b.cost, turns: b.construction_turns },
            );
        }
        for r in tables::load_raw(vfs, "building_chains", layouts::BUILDING_CHAINS, keys::first, w)?.rows {
            d.chain_category.insert(col_str(&r, 0).to_string(), col_str(&r, 3).to_string());
        }
        d.load_warnings = warnings;
        Ok(d)
    }

    /// A personality tunable: the personality's own value, else the default personality's.
    pub fn tunable(&self, personality: &str, key: &str) -> Option<f32> {
        self.personalities
            .get(personality)
            .and_then(|p| p.get(key))
            .or_else(|| self.personalities.get(&self.default_personality).and_then(|p| p.get(key)))
            .copied()
    }
}

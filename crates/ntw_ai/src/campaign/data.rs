//! The campaign AI's game data, read from the DB (so mods that change these tables take effect).
//!
//! | table | what the AI uses it for |
//! |---|---|
//! | `campaign_ai_personalities` + `_junctions` | ~245 tunables per personality (`default` + overlays) |
//! | `campaign_ai_managers` + `_manager_behaviour_junctions` | which behaviours a manager runs, and their priorities |
//! | `cdir_unit_balances` | target army composition by army size |
//! | `cdir_unit_qualities` | each unit's quality (strength value) |
//! | `campaign_difficulty_handicap_effects` | AI cost modifiers per difficulty |
//! | `building_units_allowed` | which buildings recruit which units |
//! | `building_chains`, `building_levels` | construction options, costs, chain category |
//! | `units`, `factions`, `units_to_*_permissions` | unit category/cost, who may recruit what |

use std::collections::{BTreeMap, BTreeSet};

use ntw_data::GameDatabase;
use ntw_formats::pack::Vfs;

use crate::tables::{self, TableError, col_bool, col_f32, col_i32, col_str, layouts};

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
    /// Recruitment cost.
    pub cost: i32,
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
    /// (chain, level) → building level key.
    pub chain_levels: BTreeMap<(String, i32), String>,
    /// Chain key → category (`military`, `money`, `agriculture`, `research`, `happiness`, `government`).
    pub chain_category: BTreeMap<String, String>,
    /// Building level key → units it lets a region recruit.
    pub units_allowed: BTreeMap<String, Vec<String>>,
    /// Unit key → factions with an exclusive permission (value: allowed).
    pub exclusive: BTreeMap<String, BTreeMap<String, bool>>,
    /// Unit key → military groupings allowed.
    pub groupings: BTreeMap<String, BTreeSet<String>>,
    /// Faction key → faction group (`factions` col 34).
    pub faction_group: BTreeMap<String, String>,
    /// `campaign_difficulty_handicap_effects`: (difficulty, bool column, effect) → value. The bool
    /// column is matched against the faction's `+0x6E0` flag (CONFIRMED lookup `0x00F9F970`; the flag
    /// is INFERRED to be "is human", AI_RESEARCH §4 "Difficulty").
    pub handicaps: BTreeMap<(i32, bool, String), f32>,
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
        for r in tables::load_raw(vfs, "campaign_ai_personalities", layouts::CAMPAIGN_AI_PERSONALITIES)?.rows {
            let key = col_str(&r, 0).to_string();
            if col_bool(&r, 1) {
                d.default_personality = key.clone();
            }
            d.personalities.entry(key).or_default();
        }
        for r in tables::load_raw(vfs, "campaign_ai_personality_junctions", layouts::CAMPAIGN_AI_PERSONALITY_JUNCTIONS)?.rows {
            d.personalities
                .entry(col_str(&r, 0).to_string())
                .or_default()
                .insert(col_str(&r, 1).to_string(), col_f32(&r, 2));
        }
        for r in tables::load_raw(vfs, "campaign_ai_managers", layouts::CAMPAIGN_AI_MANAGERS)?.rows {
            d.managers.entry(col_str(&r, 0).to_string()).or_default();
        }
        for r in tables::load_raw(vfs, "campaign_ai_manager_behaviour_junctions", layouts::CAMPAIGN_AI_MANAGER_BEHAVIOUR_JUNCTIONS)?.rows {
            d.managers
                .entry(col_str(&r, 0).to_string())
                .or_default()
                .insert(col_str(&r, 1).to_string(), col_f32(&r, 2));
        }
        for r in tables::load_raw(vfs, "cdir_unit_balances", "siisffi")?.rows {
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
        for r in tables::load_raw(vfs, "cdir_unit_qualities", layouts::CDIR_UNIT_QUALITIES)?.rows {
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
                    cost: u.recruitment_cost,
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
            d.chain_levels.insert((b.chain.clone(), b.level), b.key.clone());
        }
        for r in tables::load_raw(vfs, "building_chains", layouts::BUILDING_CHAINS)?.rows {
            d.chain_category.insert(col_str(&r, 0).to_string(), col_str(&r, 3).to_string());
        }
        for r in tables::load_raw(vfs, "building_units_allowed", layouts::BUILDING_UNITS_ALLOWED)?.rows {
            d.units_allowed.entry(col_str(&r, 0).to_string()).or_default().push(col_str(&r, 1).to_string());
        }
        for r in tables::load_raw(vfs, "units_to_exclusive_faction_permissions", "ssb")?.rows {
            d.exclusive
                .entry(col_str(&r, 0).to_string())
                .or_default()
                .insert(col_str(&r, 1).to_string(), col_bool(&r, 2));
        }
        for r in tables::load_raw(vfs, "units_to_groupings_military_permissions", "ss")?.rows {
            d.groupings.entry(col_str(&r, 0).to_string()).or_default().insert(col_str(&r, 1).to_string());
        }
        for f in db.factions.rows() {
            d.faction_group.insert(f.key.clone(), f.faction_group.clone());
        }
        for r in tables::load_raw(vfs, "campaign_difficulty_handicap_effects", layouts::CAMPAIGN_DIFFICULTY_HANDICAP_EFFECTS)?.rows {
            d.handicaps.insert((col_i32(&r, 0), col_bool(&r, 1), col_str(&r, 2).to_string()), col_f32(&r, 3));
        }
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

    /// INFERRED rule: a faction may recruit `unit` if it has an exclusive permission for it, or
    /// (when the unit has no exclusive rows at all) if its faction group is allowed.
    pub fn faction_may_recruit(&self, faction_key: &str, unit: &str) -> bool {
        if let Some(ex) = self.exclusive.get(unit) {
            return ex.get(faction_key).copied().unwrap_or(false);
        }
        let group = self.faction_group.get(faction_key).map(String::as_str).unwrap_or("");
        self.groupings.get(unit).is_some_and(|g| g.contains(group) || g.contains(faction_key))
    }

    /// The handicap rows that apply to a faction with difficulty `difficulty` and flag `is_human`
    /// (`0x00F9F970`, CONFIRMED: the difficulty is clamped to −2..2 and the flag picks the list).
    pub fn handicap_effects(&self, difficulty: i32, is_human: bool) -> Vec<(&str, f32)> {
        let d = handicap_index(difficulty);
        self.handicaps
            .iter()
            .filter(|((rd, rb, _), _)| *rd == d && *rb == is_human)
            .map(|((_, _, e), v)| (e.as_str(), *v))
            .collect()
    }

    /// One handicap value (0 when absent), lookup as [`Self::handicap_effects`].
    pub fn handicap(&self, difficulty: i32, is_human: bool, effect: &str) -> f32 {
        self.handicaps.get(&(handicap_index(difficulty), is_human, effect.to_string())).copied().unwrap_or(0.0)
    }

    /// The handicap value an **AI** faction gets in a campaign played at `player_difficulty`
    /// (−2 very hard .. 1 easy, the preference's scale). INFERRED: AI factions use the negated
    /// player difficulty with the flag clear (their rows are bonuses that grow with the difficulty;
    /// the writer of the AI factions' difficulty is not found yet), so PROVISIONAL.
    pub fn ai_handicap(&self, player_difficulty: i32, effect: &str) -> f32 {
        self.handicap(-player_difficulty, false, effect)
    }
}

/// `0x00F9F970` (CONFIRMED): the difficulty a handicap list is looked up with, clamped to −2..2.
pub fn handicap_index(difficulty: i32) -> i32 {
    difficulty.clamp(-2, 2)
}

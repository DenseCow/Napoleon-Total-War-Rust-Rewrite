//! Raw loading of the AI's DB tables (`campaign_ai_*`, `cdir_*`, `building_units_allowed`, ...).
//!
//! These tables are not (yet) typed in `ntw_data`, so the AI reads them here with the exe's own
//! row layouts (`analysis/worker1/DB_BUILDERS.md`, CONFIRMED from the loader code; the
//! int-vs-float choice of each 4-byte column is INFERRED from the data, see each schema).
//!
//! **Mods:** every file inside `db/<table>_tables/` is read, in VFS path order, and the rows are
//! concatenated. Later rows override earlier rows with the same key in the keyed lookups built on
//! top. PROVISIONAL: the original's exact merge rule for several files of one table is UNKNOWN
//! (`analysis/mods/MOD_LOADING.md`).

use std::fmt;

use ntw_formats::db::{DbError, DbTable, DbValue, Schema};
use ntw_formats::pack::Vfs;

/// A table read with a raw schema: just rows of values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawTable {
    /// Table name without the `_tables` suffix, e.g. `campaign_ai_personalities`.
    pub name: String,
    /// All rows of all files of the table, in VFS path order.
    pub rows: Vec<Vec<DbValue>>,
}

/// Why an AI table could not be read.
#[derive(Debug)]
pub enum TableError {
    /// The VFS could not read a file.
    Read(String, String),
    /// A file did not match the schema.
    Decode(String, DbError),
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableError::Read(p, e) => write!(f, "cannot read {p}: {e}"),
            TableError::Decode(p, e) => write!(f, "cannot decode {p}: {e:?}"),
        }
    }
}

impl std::error::Error for TableError {}

/// The exe row layouts of the tables the AI reads, as `Schema::from_codes` strings
/// (`s` string, `o` optional string, `b` bool, `i` i32, `f` f32). Source: DB_BUILDERS.md.
pub mod layouts {
    /// `campaign_ai_personalities` (reader 0xDD29B0): key, bool.
    pub const CAMPAIGN_AI_PERSONALITIES: &str = "sb";
    /// `campaign_ai_personality_junctions` (reader 0xDD22A0): personality, tunable, value (f32 INFERRED).
    pub const CAMPAIGN_AI_PERSONALITY_JUNCTIONS: &str = "ssf";
    /// `campaign_ai_managers` (reader 0xDD2220): key.
    pub const CAMPAIGN_AI_MANAGERS: &str = "s";
    /// `campaign_ai_manager_behaviour_junctions` (reader 0xDD22A0): manager, behaviour, priority.
    pub const CAMPAIGN_AI_MANAGER_BEHAVIOUR_JUNCTIONS: &str = "ssf";
    /// `cdir_configs` (reader 0xFCD150): key, opt string, string, opt string.
    pub const CDIR_CONFIGS: &str = "soso";
    /// `cdir_desire_priorities` (reader 0xFCF410).
    pub const CDIR_DESIRE_PRIORITIES: &str = "ssi";
    /// `cdir_unit_qualities` (reader 0xFC7970).
    pub const CDIR_UNIT_QUALITIES: &str = "sssi";
    /// `cdir_unit_balance_groups` (reader 0xFA71C0).
    pub const CDIR_UNIT_BALANCE_GROUPS: &str = "sb";
    /// `cdir_unit_balance_group_qualities` (reader 0xFC9A60).
    pub const CDIR_UNIT_BALANCE_GROUP_QUALITIES: &str = "sssi";
    /// `cdir_faction_junctions` / `cdir_campaign_junctions` (reader 0xF088F0).
    pub const CDIR_JUNCTIONS: &str = "so";
    /// `campaign_difficulty_handicap_effects` (reader at 0xF9F1CB): difficulty, is_ai, effect, value.
    pub const CAMPAIGN_DIFFICULTY_HANDICAP_EFFECTS: &str = "ibsf";
    /// `building_units_allowed` (reader at 0xDD289F): building level, unit, i32, opt string.
    pub const BUILDING_UNITS_ALLOWED: &str = "ssio";
    /// `building_chains` (reader at 0xE550FF): key, opt, opt, opt.
    pub const BUILDING_CHAINS: &str = "sooo";
    /// `diplomatic_relations_attitudes` (reader 0xF08373): key, value.
    pub const DIPLOMATIC_RELATIONS_ATTITUDES: &str = "si";
}

/// Reads every file of `db/<name>_tables/` with the given layout codes.
pub fn load_raw(vfs: &Vfs, name: &str, codes: &str) -> Result<RawTable, TableError> {
    // The schema strings above are plain ASCII; filter defensively so a stray character cannot
    // turn into an unknown code.
    let clean: String = codes.chars().filter(|c| "sobifh".contains(*c)).collect();
    let schema = Schema::from_codes(&clean).expect("valid layout codes");
    let prefix = format!("db/{name}_tables/");
    let mut rows = Vec::new();
    for path in vfs.list(&prefix) {
        let bytes = vfs
            .read(path)
            .map_err(|e| TableError::Read(path.to_string(), e.to_string()))?;
        let table =
            DbTable::read(&bytes, &schema).map_err(|e| TableError::Decode(path.to_string(), e))?;
        rows.extend(table.rows);
    }
    Ok(RawTable { name: name.to_string(), rows })
}

/// Column `i` of `row` as text (`""` for an absent optional string or a non-string).
pub fn col_str(row: &[DbValue], i: usize) -> &str {
    row.get(i).and_then(DbValue::as_str_or_empty).unwrap_or("")
}

/// Column `i` of `row` as f32 (an `I32` column is converted).
pub fn col_f32(row: &[DbValue], i: usize) -> f32 {
    match row.get(i) {
        Some(DbValue::F32(v)) => *v,
        Some(DbValue::I32(v)) => *v as f32,
        _ => 0.0,
    }
}

/// Column `i` of `row` as i32 (an `F32` column is truncated, like the exe's `cvttss2si`).
pub fn col_i32(row: &[DbValue], i: usize) -> i32 {
    match row.get(i) {
        Some(DbValue::I32(v)) => *v,
        Some(DbValue::F32(v)) => *v as i32,
        _ => 0,
    }
}

/// Column `i` of `row` as bool.
pub fn col_bool(row: &[DbValue], i: usize) -> bool {
    matches!(row.get(i), Some(DbValue::Bool(true)))
}

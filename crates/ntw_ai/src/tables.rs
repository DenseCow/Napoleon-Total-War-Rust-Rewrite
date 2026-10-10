//! Raw loading of the AI's DB tables (`campaign_ai_*`, `cdir_*`, `building_units_allowed`, ...).
//!
//! These tables are not (yet) typed in `ntw_data`, so the AI reads them here with the exe's own
//! row layouts (`analysis/worker1/DB_BUILDERS.md`, CONFIRMED from the loader code; the
//! int-vs-float choice of each 4-byte column is INFERRED from the data, see each schema).
//!
//! **Mods:** every file of `db/<table>_tables/` is read and the rows are merged by key the way
//! the original merges every table ([`ntw_data::load_merged_rows`], `analysis/mods/MOD_LOADING.md`
//! §3), with the key the exe's loader for that table hashes ([`keys`], §3.1).

use std::fmt;

use ntw_data::DataError;
use ntw_formats::db::{DbTable, DbValue, Schema};
use ntw_formats::pack::Vfs;

/// A table read with a raw schema: just rows of values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawTable {
    /// Table name without the `_tables` suffix, e.g. `campaign_ai_personalities`.
    pub name: String,
    /// The merged rows of all files of the table, in the order [`ntw_data::load_merged_rows`] gives.
    pub rows: Vec<Vec<DbValue>>,
}

/// Why an AI table could not be read.
#[derive(Debug)]
pub enum TableError {
    /// The VFS could not read a file.
    Read(String, String),
    /// A file of the install could not be read or did not match the schema.
    Data(DataError),
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableError::Read(p, e) => write!(f, "cannot read {p}: {e}"),
            TableError::Data(e) => write!(f, "{e}"),
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

/// The row key each table's loader hashes when it merges the files of the table (CONFIRMED per
/// table: the loader template instance, its row reader, and what it hashes; MOD_LOADING.md §3.1).
pub mod keys {
    use super::{DbValue, col_i32, col_str};

    /// Column 0 (the record's first string). `campaign_ai_managers` (loader 0xDBC9C0, reader
    /// 0xDD2220), `campaign_ai_personalities` (0xDC7DB0, 0xDD29B0), `building_chains` (0xE4C220,
    /// 0xE550D0): the reader returns the record, whose first field is column 0.
    pub fn first(row: &[DbValue]) -> String {
        col_str(row, 0).to_owned()
    }

    /// `<col 0>;<col 1>`: the junction tables `campaign_ai_personality_junctions` and
    /// `campaign_ai_manager_behaviour_junctions` (loader 0xDC6B50 builds it after reader 0xDD22A0,
    /// separator `;` at 0x013305F8).
    pub fn junction(row: &[DbValue]) -> String {
        format!("{};{}", col_str(row, 0), col_str(row, 1))
    }

    /// `<col 0>_<col 1>_<col 2>`: `cdir_unit_qualities` (reader 0xFC7970 builds it into the
    /// record's first field, which loader 0xFC6570 hashes; `_` at 0x013EA700 / 0x0131A420).
    pub fn cdir_unit_qualities(row: &[DbValue]) -> String {
        format!("{}_{}_{}", col_str(row, 0), col_str(row, 1), col_str(row, 2))
    }

    /// `<col 0>_<col 1>_<col 2>_<col 3>` with the two numbers written as unsigned decimals:
    /// `cdir_unit_balances` (reader 0xFCB550 builds it at record+0x2C with 0x004F2FC0, the u32
    /// decimal writer; loader 0xFCA150 hashes record+0x2C).
    pub fn cdir_unit_balances(row: &[DbValue]) -> String {
        format!("{}_{}_{}_{}", col_str(row, 0), col_i32(row, 1) as u32, col_i32(row, 2) as u32, col_str(row, 3))
    }
}

/// Reads `db/<name>_tables/` with the given layout codes: every file, merged by `key` (one of
/// [`keys`], the table's own) with the original's rules. Files from mods that do not decode are
/// skipped and described in `warnings`; a file of the install that does not decode is an error.
pub fn load_raw(
    vfs: &Vfs,
    name: &'static str,
    codes: &str,
    key: fn(&[DbValue]) -> String,
    warnings: &mut Vec<String>,
) -> Result<RawTable, TableError> {
    let schema = schema(codes);
    let decode = |bytes: &[u8]| -> Result<Vec<(String, Vec<DbValue>)>, DataError> {
        let table = DbTable::read(bytes, &schema).map_err(|error| DataError::Db { table: name, error })?;
        Ok(table.rows.into_iter().map(|r| (key(&r), r)).collect())
    };
    let rows = ntw_data::load_merged_rows(vfs, name, warnings, decode, |(k, _)| k).map_err(TableError::Data)?;
    Ok(RawTable { name: name.to_string(), rows: rows.into_iter().map(|(_, r)| r).collect() })
}

/// The [`Schema`] of layout codes (see [`layouts`]).
pub fn schema(codes: &str) -> Schema {
    // The schema strings above are plain ASCII; filter defensively so a stray character cannot
    // turn into an unknown code.
    let clean: String = codes.chars().filter(|c| "sobifh".contains(*c)).collect();
    Schema::from_codes(&clean).expect("valid layout codes")
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

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::pack::PackFile;

    /// A PFH0 pack with the given type and files.
    fn pack(pack_type: u32, files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut index = Vec::new();
        for (path, data) in files {
            index.extend_from_slice(&(data.len() as u32).to_le_bytes());
            index.extend_from_slice(path.as_bytes());
            index.push(0);
        }
        let mut b = b"PFH0".to_vec();
        for v in [pack_type, 0, 0, files.len() as u32, index.len() as u32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&index);
        for (_, data) in files {
            b.extend_from_slice(data);
        }
        b
    }

    fn junctions(rows: &[(&str, &str, f32)]) -> Vec<u8> {
        let rows = rows.iter().map(|(a, b, v)| vec![DbValue::Str((*a).into()), DbValue::Str((*b).into()), DbValue::F32(*v)]).collect();
        DbTable { version: 0, has_version_marker: false, flag: 1, rows }
            .to_bytes(&schema(layouts::CAMPAIGN_AI_PERSONALITY_JUNCTIONS))
            .unwrap()
    }

    /// The AI's tables go through the merged view: a mod file's row replaces the vanilla row with
    /// the same key (here the junction key `personality;tunable`), new keys are added, and a mod
    /// file that does not decode is skipped with a warning.
    #[test]
    fn load_raw_merges_mod_files_by_the_tables_key() {
        let dir = std::env::temp_dir().join(format!("ntw_ai_tables_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let folder = "db/campaign_ai_personality_junctions_tables/";
        let base = junctions(&[("p1", "t1", 1.0), ("p1", "t2", 2.0), ("p2", "t1", 3.0)]);
        let modded = junctions(&[("p1", "t2", 9.0), ("p3", "t1", 4.0)]);
        std::fs::write(dir.join("data.pack"), pack(1, &[(&format!("{folder}base"), base)])).unwrap();
        std::fs::write(dir.join("mod.pack"), pack(3, &[(&format!("{folder}mod"), modded), (&format!("{folder}broken"), vec![1, 2])]))
            .unwrap();
        let vfs = Vfs::from_packs(vec![PackFile::open(dir.join("data.pack")).unwrap(), PackFile::open(dir.join("mod.pack")).unwrap()]);
        let mut warnings = Vec::new();
        let t = load_raw(&vfs, "campaign_ai_personality_junctions", layouts::CAMPAIGN_AI_PERSONALITY_JUNCTIONS, keys::junction, &mut warnings)
            .unwrap();
        let got: Vec<(String, f32)> = t.rows.iter().map(|r| (keys::junction(r), col_f32(r, 2))).collect();
        let want = [("p1;t1", 1.0), ("p1;t2", 9.0), ("p2;t1", 3.0), ("p3;t1", 4.0)].map(|(k, v)| (k.to_owned(), v));
        assert_eq!(got, want);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("broken"));
        // A table with no file: no rows (as before), not an error.
        assert!(load_raw(&vfs, "building_chains", layouts::BUILDING_CHAINS, keys::first, &mut warnings).unwrap().rows.is_empty());
        drop(vfs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The composite keys the exe builds (0xFC7970, 0xFCB550 with the u32 writer 0x004F2FC0).
    #[test]
    fn composite_keys_match_the_exe() {
        let s = |v: &str| DbValue::Str(v.into());
        assert_eq!(keys::cdir_unit_qualities(&[s("default"), s("a"), s("unit"), DbValue::I32(3)]), "default_a_unit");
        let row = [s("cfg"), DbValue::I32(-1), DbValue::I32(12), s("grp"), DbValue::F32(0.5)];
        assert_eq!(keys::cdir_unit_balances(&row), "cfg_4294967295_12_grp");
        assert_eq!(keys::junction(&[s("p"), s("t"), DbValue::F32(1.0)]), "p;t");
        assert_eq!(keys::first(&[s("k"), DbValue::Bool(true)]), "k");
    }
}

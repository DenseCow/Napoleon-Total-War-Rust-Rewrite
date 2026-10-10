//! The one reader of game tables: every file of a table's `db\<table>_tables\` folder, merged by
//! key the way the original's table loaders merge them (`DB_LoadTableFolderMergedByKey`
//! 0x00E778A0, `analysis/mods/MOD_LOADING.md` §3). A mod's additive file (`db\units_tables\my_mod`)
//! and its `bob_` file reach every reader only because they all come through here: no other code
//! names a `db\..._tables` path (the `table_path_guard` test in the `napoleon` crate fails if one
//! does).
//!
//! Two ways in:
//! * [`merged_rows`]: a table with its own decoder and key (`ntw_data`'s typed tables, the
//!   campaign AI's raw tables, `models_building`).
//! * [`RawTable::read`]: a table cut into rows with a generic [`Schema`], keyed by the rule the
//!   exe's loader for that table hashes ([`RowKey`]). The tables read this way are listed in
//!   [`tables`], each with its schema and its traced key, so two readers of one table cannot
//!   disagree.

use std::fmt;

use crate::db::{DbError, DbTable, DbValue, Schema};
use crate::pack::{FolderFile, PackError, Vfs};

/// The folder that holds a table's files, e.g. `db\units_tables\`.
pub fn folder(table: &str) -> String {
    format!("db\\{table}_tables\\")
}

/// Reads and decodes every file of one table in the original's read order
/// ([`Vfs::table_files`]), each with the file it came from. A file that fails to read or decode is
/// skipped with a line in `warnings` unless it belongs to the install's own packs, where it is an
/// error. A table with no file gives no files.
pub fn read_files<R, E>(
    vfs: &Vfs,
    table: &str,
    warnings: &mut Vec<String>,
    decode: impl Fn(&[u8]) -> Result<R, E>,
) -> Result<(Vec<FolderFile>, Vec<R>), E>
where
    E: From<PackError> + fmt::Display,
{
    let (mut files, mut out) = (Vec::new(), Vec::new());
    for file in vfs.table_files(&folder(table)) {
        let install = vfs.layers()[file.layer].kind.is_install_pack();
        match vfs.read_folder_file(&file).map_err(E::from).and_then(|b| decode(&b)) {
            Ok(t) => {
                files.push(file);
                out.push(t);
            }
            Err(e) if !install => warnings.push(format!("{} ({}): {e}; skipped", file.path, vfs.layer_path(&vfs.layers()[file.layer]).display())),
            Err(e) => return Err(e),
        }
    }
    Ok((files, out))
}

/// Merges the rows of several files of one table by key, the way `DB_LoadTableFolderMergedByKey`
/// (0x00E778A0, CONFIRMED) does across files: a key's row comes from the first file that has it,
/// and a later file's row with the same key replaces it only when `replaces(holder, file)`
/// (indexes into `files`); otherwise the later row is dropped.
///
/// PROVISIONAL (MOD_LOADING.md §3): a replacing row takes the replaced row's position and repeated
/// keys inside one file are all kept, as before mods; the exe also merges keys inside one file and
/// emits the rows in key hash-map order, which waits for each table's key reader to be checked.
pub fn merge_keyed<R>(files: Vec<Vec<R>>, key: impl Fn(&R) -> &str, replaces: impl Fn(usize, usize) -> bool) -> Vec<R> {
    let mut rows: Vec<Option<R>> = Vec::new();
    // Key -> (file that holds it, its row positions).
    let mut holders: std::collections::HashMap<String, (usize, Vec<usize>)> = std::collections::HashMap::new();
    for (file, file_rows) in files.into_iter().enumerate() {
        for row in file_rows {
            match holders.get_mut(key(&row)) {
                None => {
                    holders.insert(key(&row).to_owned(), (file, vec![rows.len()]));
                    rows.push(Some(row));
                }
                Some((holder, positions)) if *holder == file => {
                    positions.push(rows.len());
                    rows.push(Some(row));
                }
                Some((holder, positions)) => {
                    if replaces(*holder, file) {
                        for &p in &positions[1..] {
                            rows[p] = None;
                        }
                        rows[positions[0]] = Some(row);
                        positions.truncate(1);
                        *holder = file;
                    }
                }
            }
        }
    }
    rows.into_iter().flatten().collect()
}

/// The rows of one table through the merged view: every file in `db\<table>_tables\` decoded by
/// `decode`, in the original's order, merged by `key` with the original's row rule
/// ([`Vfs::db_row_replaces`]). `key` must be the key the exe's loader for that table hashes. A
/// table with no file gives no rows; files from mods that fail to decode are skipped and described
/// in `warnings`.
pub fn merged_rows<R, E>(
    vfs: &Vfs,
    table: &str,
    warnings: &mut Vec<String>,
    decode: impl Fn(&[u8]) -> Result<Vec<R>, E>,
    key: impl Fn(&R) -> &str,
) -> Result<Vec<R>, E>
where
    E: From<PackError> + fmt::Display,
{
    let (files, tables) = read_files(vfs, table, warnings, decode)?;
    Ok(merge_keyed(tables, key, |holder, new| vfs.db_row_replaces(&files[holder], &files[new])))
}

/// What the exe's loader of a table hashes for a row, so two files' rows with the same key are
/// merged (one per table in [`tables`], each traced to its loader and row reader).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKey {
    /// The text of these string columns, joined with nothing between (an absent optional string is
    /// `""`, as the exe reads it).
    Concat(&'static [usize]),
    /// The text of these string columns with `sep` between them.
    Joined(&'static [usize], &'static str),
    /// A 4-byte integer column written in decimal (`%d`).
    Decimal(usize),
    /// The `warscape_*_lod` key the row reader builds in place of column 0: the mesh path
    /// normalised as the VFS does ([`lod_path`]), `_`, the middle column (a float as `%f`, a string
    /// as is), `_`, the last column.
    Lod { path: usize, middle: usize, last: usize },
}

impl RowKey {
    /// The key of one decoded row.
    pub fn of(&self, row: &[DbValue]) -> String {
        let text = |i: usize| row.get(i).and_then(DbValue::as_str_or_empty).unwrap_or_default();
        match *self {
            Self::Concat(cols) => cols.iter().map(|&i| text(i)).collect(),
            Self::Joined(cols, sep) => cols.iter().map(|&i| text(i)).collect::<Vec<_>>().join(sep),
            Self::Decimal(i) => row.get(i).and_then(DbValue::as_i32).unwrap_or_default().to_string(),
            Self::Lod { path, middle, last } => {
                let middle = match row.get(middle) {
                    Some(DbValue::F32(v)) => format!("{:.6}", f64::from(*v)),
                    Some(v) => v.as_str_or_empty().unwrap_or_default().to_owned(),
                    None => String::new(),
                };
                format!("{}_{middle}_{}", lod_path(text(path)), text(last))
            }
        }
    }
}

/// The path normalisation the `warscape_*_lod` row readers apply before building their key (VFS
/// vfunc +0xB8 = 0x01065830 with 0x00EDEE40, CONFIRMED): `/` becomes `\`, ASCII capitals become
/// lower case, a `\` right after another `\` is dropped (from the third character on), then a
/// leading `data\` and a leading `\` are cut. (0x01065830 also cuts a working-directory prefix,
/// which only an absolute path can carry; the tables hold relative ones.)
pub fn lod_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev = '\0';
    for (i, c) in path.chars().enumerate() {
        let c = if c == '/' { '\\' } else { c.to_ascii_lowercase() };
        if c == '\\' && prev == '\\' && i >= 2 {
            continue;
        }
        out.push(c);
        prev = c;
    }
    let out = out.strip_prefix("data\\").unwrap_or(&out);
    out.strip_prefix('\\').unwrap_or(out).to_owned()
}

/// A table read with a generic schema: its name, its column codes ([`Schema::from_codes`]) and the
/// key its rows merge by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawTable {
    /// The table name: the files are in `db\<name>_tables\`.
    pub name: &'static str,
    /// The column codes ([`Schema::from_codes`]).
    pub codes: &'static str,
    /// What the exe's loader hashes for a row.
    pub key: RowKey,
}

/// Why a [`RawTable`] could not be read.
#[derive(Debug)]
pub enum TableError {
    /// The table has no file at all.
    Missing(&'static str),
    /// A file of the install's own packs could not be read.
    Pack(PackError),
    /// A file of the install's own packs does not match the schema.
    Db { table: &'static str, error: DbError },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(t) => write!(f, "{t}: no file in {}", folder(t)),
            Self::Pack(e) => write!(f, "{e}"),
            Self::Db { table, error } => write!(f, "{table}: {error}"),
        }
    }
}

impl std::error::Error for TableError {}

impl From<PackError> for TableError {
    fn from(e: PackError) -> Self {
        Self::Pack(e)
    }
}

impl RawTable {
    /// The table's rows through the merged view (every file of its folder, in the original's
    /// order, merged by [`RawTable::key`] with the original's row rule). A mod file that does not
    /// read or decode is skipped with a `WARN` line; a table with no file is [`TableError::Missing`].
    /// For a table read once (an index built at load time); a reader called again and again uses
    /// [`read_with`](Self::read_with) and logs the warnings once.
    pub fn read(&self, vfs: &Vfs) -> Result<Vec<Vec<DbValue>>, TableError> {
        let mut warnings = Vec::new();
        let rows = self.read_with(vfs, &mut warnings);
        for w in &warnings {
            eprintln!("WARN ntw_formats: {w}");
        }
        rows
    }

    /// [`read`](Self::read) with the skipped mod files described in `warnings` instead of logged.
    pub fn read_with(&self, vfs: &Vfs, warnings: &mut Vec<String>) -> Result<Vec<Vec<DbValue>>, TableError> {
        let schema = Schema::from_codes(self.codes).expect("table schemas in `tables` are valid codes");
        let table = self.name;
        let decode = |b: &[u8]| -> Result<Vec<(String, Vec<DbValue>)>, TableError> {
            let t = DbTable::read(b, &schema).map_err(|error| TableError::Db { table, error })?;
            Ok(t.rows.into_iter().map(|r| (self.key.of(&r), r)).collect())
        };
        let (files, tables) = read_files(vfs, table, warnings, decode)?;
        if files.is_empty() {
            return Err(TableError::Missing(table));
        }
        let rows = merge_keyed(tables, |r| r.0.as_str(), |holder, new| vfs.db_row_replaces(&files[holder], &files[new]));
        Ok(rows.into_iter().map(|(_, r)| r).collect())
    }
}

/// The tables read with a generic schema, each with the key its loader hashes (CONFIRMED: loader
/// = the `DB_LoadTableFolderMergedByKey` template instance its folder name is passed to, reader =
/// the row reader passed with it; MOD_LOADING.md §3.2 lists both addresses per table).
pub mod tables {
    use super::{RawTable, RowKey};

    const fn table(name: &'static str, codes: &'static str, key: RowKey) -> RawTable {
        RawTable { name, codes, key }
    }

    /// (mount, model key, weight).
    pub const MOUNT_VARIANTS: RawTable = table("mount_variants", "s,s,f", RowKey::Concat(&[0, 1]));
    /// (model key, texture stem, kind).
    pub const WARSCAPE_ANIMATED: RawTable = table("warscape_animated", "s,s,s", RowKey::Concat(&[0]));
    /// (id, mesh path, distance, model key).
    pub const WARSCAPE_ANIMATED_LOD: RawTable =
        table("warscape_animated_lod", "s,s,f,s", RowKey::Lod { path: 1, middle: 2, last: 3 });
    /// (key, folder, kind).
    pub const WARSCAPE_RIGID: RawTable = table("warscape_rigid", "s,s,s", RowKey::Concat(&[0]));
    /// (id, mesh path, distance text, model key).
    pub const WARSCAPE_RIGID_LOD: RawTable = table("warscape_rigid_lod", "s,s,s,s", RowKey::Lod { path: 1, middle: 2, last: 3 });
    /// (uniform, faction, variant file, unit).
    pub const UNIFORMS: RawTable = table("uniforms", "s,s,s,s", RowKey::Concat(&[1, 3]));
    /// (uniform, faction, 9 colour channels).
    pub const UNIFORM_TO_FACTION_COLOURS: RawTable =
        table("uniform_to_faction_colours", "s,s,i,i,i,i,i,i,i,i,i", RowKey::Concat(&[0, 1]));
    /// (faction, 9 colour channels).
    pub const FACTION_UNIFORM_COLOURS: RawTable = table("faction_uniform_colours", "s,i,i,i,i,i,i,i,i,i", RowKey::Concat(&[0]));
    /// (species, season, path).
    pub const WARSCAPE_TREES: RawTable = table("warscape_trees", "s,s,s", RowKey::Concat(&[0, 1]));
    /// (group, name, type, gender, weight, noble, id).
    pub const NAMES: RawTable = table("names", "s,s,s,s,i,b,s", RowKey::Concat(&[6]));
    /// (key, category, ..).
    pub const BATTLEFIELD_BUILDINGS: RawTable = table("battlefield_buildings", "s,s,s,s,i,o,o,i", RowKey::Concat(&[0]));
    /// (ground type, 4 speed factors).
    pub const UNIT_MOVEMENT_MODIFIERS: RawTable = table("unit_movement_modifiers", "s,f,f,f,f", RowKey::Concat(&[0]));
    /// (type, composition, size, era, 7 limits).
    pub const BATTLE_TYPE_SETUP_LIMITS: RawTable =
        table("battle_type_setup_limits", "s,s,s,s,i,i,i,i,i,i,i", RowKey::Concat(&[0, 1, 2, 3]));
    /// (faction, setup id, preset id): keyed by the preset id in decimal.
    pub const BATTLE_TYPE_FACTION_PRESETS: RawTable = table("battle_type_faction_presets", "s,i,i", RowKey::Decimal(2));
    /// (key, preset id, unit, experience).
    pub const BATTLE_TYPE_UNIT_TO_FACTION_PRESETS: RawTable =
        table("battle_type_unit_to_faction_presets", "s,i,s,i", RowKey::Concat(&[0]));
    /// (id, names group, name, faction).
    pub const SHIP_NAMES: RawTable = table("ship_names", "s,s,s,o", RowKey::Concat(&[0]));
    /// (key, ?, 2 floats, order).
    pub const WIND_LEVELS: RawTable = table("wind_levels", "s,s,f,f,i", RowKey::Concat(&[0]));
    /// (battle, sky type).
    pub const BATTLES_TO_BATTLE_SKY_TYPES_JUNCTIONS: RawTable =
        table("battles_to_battle_sky_types_junctions", "s,s", RowKey::Concat(&[0, 1]));
    /// (key, ?, weather, time of day, ..).
    pub const BATTLE_SKY_TYPES: RawTable = table("battle_sky_types", "s,s,s,s,b,s,s,s", RowKey::Concat(&[0]));
    /// (key).
    pub const BATTLE_TYPES: RawTable = table("battle_types", "s", RowKey::Concat(&[0]));
    /// (key, type, naval, spec, screenshot, .., movie, year).
    pub const BATTLES: RawTable = table("battles", "s,s,b,s,o,i,i,b,b,b,b,o,i", RowKey::Concat(&[0]));
    /// (government, position, 3 more strings).
    pub const MINISTERIAL_POSITIONS_BY_GOV_TYPES: RawTable =
        table("ministerial_positions_by_gov_types", "s,s,s,s,s", RowKey::Concat(&[0, 1, 2, 3]));
    /// (position, number).
    pub const MINISTERIAL_POSITIONS: RawTable = table("ministerial_positions", "s,i", RowKey::Concat(&[0]));
    /// (key, value).
    pub const STATE_GIFT_VALUES: RawTable = table("state_gift_values", "s,i", RowKey::Concat(&[0]));
    /// (level key, attitude value).
    pub const DIPLOMATIC_RELATIONS_ATTITUDES: RawTable = table("diplomatic_relations_attitudes", "s,i", RowKey::Concat(&[0]));
    /// (key, int, pip picture).
    pub const RELIGIONS: RawTable = table("religions", "s,i,s", RowKey::Concat(&[0]));
    /// (key, int, fallback culture).
    pub const CULTURES: RawTable = table("cultures", "s,i,o", RowKey::Concat(&[0]));
    /// (building level, culture, 5 optional strings).
    pub const BUILDING_CULTURE_VARIANTS: RawTable = table("building_culture_variants", "s,s,o,o,o,o,o", RowKey::Concat(&[0, 1]));
    /// (technology, required technology).
    pub const TECHNOLOGY_REQUIRED_TECHNOLOGY_JUNCTIONS: RawTable =
        table("technology_required_technology_junctions", "s,s", RowKey::Concat(&[0, 1]));
    /// (theme, 5 more columns).
    pub const WARSCAPE_EQUIPMENT_THEMES: RawTable = table("warscape_equipment_themes", "s,o,o,b,o,o", RowKey::Concat(&[0]));
    /// (theme, item).
    pub const WARSCAPE_EQUIPMENT_ITEMS: RawTable = table("warscape_equipment_items", "s,s", RowKey::Joined(&[0, 1], "_"));
    /// (key, 4 more strings).
    pub const BATTLE_PERSONALITIES: RawTable = table("battle_personalities", "s,s,s,s,s", RowKey::Concat(&[0]));
    /// (key, 2 strings, 10 floats, a string, 6 floats, an int).
    pub const BATTLE_ENTITIES: RawTable =
        table("battle_entities", "s,s,s,f,f,f,f,f,f,f,f,f,f,s,f,f,f,f,f,f,i", RowKey::Concat(&[0]));
    /// (factor key, pip picture, optional text).
    pub const PUBLIC_ORDER_FACTORS: RawTable = table("public_order_factors", "s,s,o", RowKey::Concat(&[0]));
    /// (factor key, pip picture, text).
    pub const TOWN_WEALTH_GROWTH_FACTORS: RawTable = table("town_wealth_growth_factors", "s,s,s", RowKey::Concat(&[0]));

    /// Every table above (for checks over all of them).
    pub const ALL: [RawTable; 35] = [
        MOUNT_VARIANTS, WARSCAPE_ANIMATED, WARSCAPE_ANIMATED_LOD, WARSCAPE_RIGID, WARSCAPE_RIGID_LOD, UNIFORMS,
        UNIFORM_TO_FACTION_COLOURS, FACTION_UNIFORM_COLOURS, WARSCAPE_TREES, NAMES, BATTLEFIELD_BUILDINGS,
        UNIT_MOVEMENT_MODIFIERS, BATTLE_TYPE_SETUP_LIMITS, BATTLE_TYPE_FACTION_PRESETS, BATTLE_TYPE_UNIT_TO_FACTION_PRESETS,
        SHIP_NAMES, WIND_LEVELS, BATTLES_TO_BATTLE_SKY_TYPES_JUNCTIONS, BATTLE_SKY_TYPES, BATTLE_TYPES, BATTLES,
        MINISTERIAL_POSITIONS_BY_GOV_TYPES, MINISTERIAL_POSITIONS, STATE_GIFT_VALUES, DIPLOMATIC_RELATIONS_ATTITUDES,
        RELIGIONS, CULTURES, BUILDING_CULTURE_VARIANTS, TECHNOLOGY_REQUIRED_TECHNOLOGY_JUNCTIONS, WARSCAPE_EQUIPMENT_THEMES,
        WARSCAPE_EQUIPMENT_ITEMS, BATTLE_PERSONALITIES, BATTLE_ENTITIES, PUBLIC_ORDER_FACTORS, TOWN_WEALTH_GROWTH_FACTORS,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::test_util::{build_pack, temp_dir};
    use crate::pack::PackFile;

    fn row(cells: &[&str]) -> Vec<DbValue> {
        cells.iter().map(|c| DbValue::Str((*c).to_owned())).collect()
    }

    #[test]
    fn keys_follow_the_traced_rules() {
        assert_eq!(RowKey::Concat(&[0, 1]).of(&row(&["horse_a", "horse_b", "x"])), "horse_ahorse_b");
        assert_eq!(RowKey::Concat(&[0]).of(&[DbValue::OptStr(None)]), "");
        assert_eq!(RowKey::Decimal(2).of(&[DbValue::Str("f".into()), DbValue::I32(1), DbValue::I32(-7)]), "-7");
        let lod = [DbValue::Str("id".into()), DbValue::Str("Data/UnitModels//Horse/A.mesh".into()), DbValue::F32(10.0), DbValue::Str("horse_a".into())];
        assert_eq!(RowKey::Lod { path: 1, middle: 2, last: 3 }.of(&lod), "unitmodels\\horse\\a.mesh_10.000000_horse_a");
        let rigid = row(&["id", "\\models\\x.rigid_model", "20", "key"]);
        assert_eq!(RowKey::Lod { path: 1, middle: 2, last: 3 }.of(&rigid), "models\\x.rigid_model_20_key");
    }

    #[test]
    fn lod_path_keeps_a_leading_double_separator() {
        // The doubled-separator rule starts at the third character (0x00EDEE40).
        assert_eq!(lod_path("\\\\a\\\\b"), "\\a\\b");
        assert_eq!(lod_path("DATA\\x"), "x");
    }

    /// A mod's additive file and its `bob_` file reach a [`RawTable`] reader, merged by the
    /// table's key.
    #[test]
    fn raw_table_merges_mod_files() {
        let dir = temp_dir("db_folder_raw");
        let encode = |rows: &[[&str; 3]]| {
            let t = DbTable {
                version: 0,
                has_version_marker: false,
                flag: 1,
                rows: rows.iter().map(|r| vec![DbValue::Str(r[0].into()), DbValue::Str(r[1].into()), DbValue::F32(r[2].parse().unwrap())]).collect(),
            };
            t.to_bytes(&Schema::from_codes("s,s,f").unwrap()).unwrap()
        };
        let vanilla = encode(&[["m", "a", "1"], ["m", "b", "1"]]);
        let additive = encode(&[["m", "a", "5"], ["m", "c", "2"]]);
        let bob = encode(&[["m", "b", "9"]]);
        std::fs::write(dir.join("rel.pack"), build_pack(1, &[("db\\mount_variants_tables\\mount_variants", &vanilla)])).unwrap();
        std::fs::write(
            dir.join("mod.pack"),
            build_pack(1, &[("db\\mount_variants_tables\\more", &additive), ("db\\mount_variants_tables\\bob_fix", &bob)]),
        )
        .unwrap();
        let mut vfs = Vfs::new();
        vfs.mount(PackFile::open(dir.join("rel.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("mod.pack")).unwrap());
        let rows = tables::MOUNT_VARIANTS.read(&vfs).unwrap();
        let got: Vec<(String, f32)> =
            rows.iter().map(|r| (r[1].as_str().unwrap().to_owned(), r[2].as_f32().unwrap())).collect();
        // Same priority: `ma` stays vanilla (no `bob_`), `mb` is replaced by the `bob_` file, `mc` is added.
        assert_eq!(got, [("a".to_owned(), 1.0), ("b".to_owned(), 9.0), ("c".to_owned(), 2.0)]);
        assert!(matches!(tables::WARSCAPE_TREES.read(&vfs), Err(TableError::Missing("warscape_trees"))));
    }
}

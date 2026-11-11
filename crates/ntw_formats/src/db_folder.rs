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

/// The tables the exe reads through its table loader 0x00DBDB90, one per caller (CONFIRMED,
/// `analysis/mods/MOD_LOADING.md` §3.3). Their row readers (0x00DD2250, 0x00DD22A0, 0x00E85910) put
/// column 0 at record+0 and column 1 at record+0xC, and the loader hashes the two joined with nothing
/// between (0x004F1200 at 0x00DBE511), so a later file's row merges into an earlier one only when
/// both columns match. This list is the one statement of that key, and every reader checks it: an
/// `ntw_data` `db_record!` of one of these tables that does not merge on exactly its first two
/// columns (`key = <column 0> + <column 1>`) fails to compile, a [`RawTable`] of one must be keyed
/// `RowKey::Concat(&[0, 1])` ([`RawTable::merges_like_the_loader`]), and [`merged_rows`], whose key
/// is a closure nothing can check, refuses them.
pub const COLUMN_0_1_KEYED_TABLES: [&str; 16] = [
    "building_effects_junction",
    "building_factionwide_effects_junctions",
    "units_to_exclusive_faction_permissions",
    "technology_effects_junction",
    "trait_ability_effects",
    "trait_attribute_effects",
    "trait_attribute_situation_effects",
    "trait_level_effects",
    "agent_spawning_to_government_types",
    "agent_spawning_to_building_chains",
    "agent_spawning_to_policies",
    "agent_to_agent_abilities",
    "ancillary_to_ability_effects",
    "ancillary_to_attribute_effects",
    "ancillary_to_attribute_situation_effects",
    "ancillary_to_effects",
];

/// `a == b` in a `const` context.
pub const fn const_str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// True when `table` is one of [`COLUMN_0_1_KEYED_TABLES`] (`const`, for the compile-time checks).
pub const fn is_column_0_1_keyed(table: &str) -> bool {
    let mut i = 0;
    while i < COLUMN_0_1_KEYED_TABLES.len() {
        if const_str_eq(COLUMN_0_1_KEYED_TABLES[i], table) {
            return true;
        }
        i += 1;
    }
    false
}

/// The rows of one table through the merged view: every file in `db\<table>_tables\` decoded by
/// `decode`, in the original's order, merged by `key` with the original's row rule
/// ([`Vfs::db_row_replaces`]). `key` must be the key the exe's loader for that table hashes. A
/// table with no file gives no rows; files from mods that fail to decode are skipped and described
/// in `warnings`.
///
/// # Panics
/// For a table in [`COLUMN_0_1_KEYED_TABLES`]: nothing can check that a closure joins columns 0 and
/// 1, so those tables are read as a typed `ntw_data` record or a [`RawTable`], which are checked.
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
    assert!(
        !is_column_0_1_keyed(table),
        "{table}: the exe's loader 0x00DBDB90 merges this table's files on columns 0 + 1 (COLUMN_0_1_KEYED_TABLES); read it as a typed record or a RawTable keyed RowKey::Concat(&[0, 1]), whose key is checked"
    );
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
    /// False for a table in [`COLUMN_0_1_KEYED_TABLES`] whose [`key`](Self::key) is not columns 0
    /// and 1 joined with nothing between, the key its loader hashes; any other table passes.
    /// Checked at compile time for [`tables`] and by [`read_with`](Self::read_with) for a table
    /// built elsewhere.
    pub const fn merges_like_the_loader(&self) -> bool {
        if !is_column_0_1_keyed(self.name) {
            return true;
        }
        match self.key {
            RowKey::Concat(cols) => cols.len() == 2 && cols[0] == 0 && cols[1] == 1,
            _ => false,
        }
    }

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
    ///
    /// # Panics
    /// When the table does not [merge like its loader](Self::merges_like_the_loader): a wrong key
    /// in the code, which would merge away a mod's rows that share only column 0.
    pub fn read_with(&self, vfs: &Vfs, warnings: &mut Vec<String>) -> Result<Vec<Vec<DbValue>>, TableError> {
        assert!(
            self.merges_like_the_loader(),
            "{}: the exe's loader 0x00DBDB90 merges this table's files on columns 0 + 1; key it RowKey::Concat(&[0, 1]) (COLUMN_0_1_KEYED_TABLES)",
            self.name
        );
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
        let table = RawTable { name, codes, key };
        // A compile error for a constant below (COLUMN_0_1_KEYED_TABLES).
        assert!(table.merges_like_the_loader(), "the exe's loader 0x00DBDB90 merges this table's files on columns 0 + 1: key it RowKey::Concat(&[0, 1])");
        table
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
    /// (script event name): the events the gate `0x008F9EE0` lets reach Lua (the table is built on
    /// first use by `0x00E30810`). Key column 0 (CONFIRMED): the table's entry_pack `0x00E9AFF0` (from
    /// its constructor `0x00E6DD40`) keys each loaded row by a copy of row +0x00, the event name, and
    /// looks it up in the loaded source's key map (+0x14, `0x0047AA40`; "not a valid key" when absent).
    pub const TRIGGER_EVENTS: RawTable = table("trigger_events", "s", RowKey::Concat(&[0]));
    /// (script event name, agent type key): the agent types the gate keeps an event from. Key columns
    /// 0 + 1 (CONFIRMED): the entry_pack `0x00E9B370` (constructor `0x00E6DE50`, folder
    /// `trigger_event_to_excluded_agent_types_tables`, `0x00E8FEA0`) keys each loaded row by row +0x00
    /// joined with row +0x0C (`0x004F1200`), looks that up in the source's key map and indexes the
    /// records by it. In memory +0x00 is the event key and +0x0C the agent key (CONFIRMED,
    /// `0x00E8FD50` matches the first with the event record's key and keeps the second). INFERRED: the
    /// file's column order is the record's (event, then agent type); vanilla ships the table empty, so
    /// no file shows it, and the row reader that fills +0x00 / +0x0C (the loader that builds the rows
    /// `0x00E8FEA0` is given) is not traced: tracing it settles this.
    pub const TRIGGER_EVENT_TO_EXCLUDED_AGENT_TYPES: RawTable =
        table("trigger_event_to_excluded_agent_types", "s,s", RowKey::Concat(&[0, 1]));

    /// Every table above (for checks over all of them).
    pub const ALL: [RawTable; 37] = [
        MOUNT_VARIANTS, WARSCAPE_ANIMATED, WARSCAPE_ANIMATED_LOD, WARSCAPE_RIGID, WARSCAPE_RIGID_LOD, UNIFORMS,
        UNIFORM_TO_FACTION_COLOURS, FACTION_UNIFORM_COLOURS, WARSCAPE_TREES, NAMES, BATTLEFIELD_BUILDINGS,
        UNIT_MOVEMENT_MODIFIERS, BATTLE_TYPE_SETUP_LIMITS, BATTLE_TYPE_FACTION_PRESETS, BATTLE_TYPE_UNIT_TO_FACTION_PRESETS,
        SHIP_NAMES, WIND_LEVELS, BATTLES_TO_BATTLE_SKY_TYPES_JUNCTIONS, BATTLE_SKY_TYPES, BATTLE_TYPES, BATTLES,
        MINISTERIAL_POSITIONS_BY_GOV_TYPES, MINISTERIAL_POSITIONS, STATE_GIFT_VALUES, DIPLOMATIC_RELATIONS_ATTITUDES,
        RELIGIONS, CULTURES, BUILDING_CULTURE_VARIANTS, TECHNOLOGY_REQUIRED_TECHNOLOGY_JUNCTIONS, WARSCAPE_EQUIPMENT_THEMES,
        WARSCAPE_EQUIPMENT_ITEMS, BATTLE_PERSONALITIES, BATTLE_ENTITIES, PUBLIC_ORDER_FACTORS, TOWN_WEALTH_GROWTH_FACTORS,
        TRIGGER_EVENTS, TRIGGER_EVENT_TO_EXCLUDED_AGENT_TYPES,
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

    /// A row of `schema` whose every cell depends on `n` (a bool on its parity), so rows with
    /// different `n` have different keys under every [`RowKey`].
    fn sample_row(schema: &Schema, n: i32) -> Vec<DbValue> {
        use crate::db::FieldType;
        schema
            .fields
            .iter()
            .map(|f| match f.ty {
                FieldType::Str => DbValue::Str(format!("s{n}")),
                FieldType::OptStr => DbValue::OptStr(Some(format!("o{n}"))),
                FieldType::Bool => DbValue::Bool(n % 2 == 0),
                FieldType::I32 => DbValue::I32(n),
                FieldType::F32 => DbValue::F32(n as f32),
                FieldType::U16 => DbValue::U16(n as u16),
            })
            .collect()
    }

    /// Two mounted packs hold `files`: the vanilla pack (an install pack) and a mod pack.
    fn two_pack_vfs(test: &str, vanilla: &[(&str, &[u8])], modded: &[(&str, &[u8])]) -> Vfs {
        let dir = temp_dir(test);
        std::fs::write(dir.join("rel.pack"), build_pack(1, vanilla)).unwrap();
        std::fs::write(dir.join("mod.pack"), build_pack(1, modded)).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount(PackFile::open(dir.join("rel.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("mod.pack")).unwrap());
        vfs
    }

    /// For every [`tables::ALL`] table, the key reader tells rows apart: a mod file that repeats
    /// the vanilla row merges into it (one row), and a row with other key columns is added. A key
    /// reader that read the wrong columns (or none) would fail one of the two. The vanilla install
    /// alone cannot show this, its one file per table keeps every row whatever the key.
    #[test]
    fn every_raw_table_key_separates_rows_across_two_files() {
        for t in tables::ALL {
            let schema = Schema::from_codes(t.codes).unwrap();
            let encode = |rows: Vec<Vec<DbValue>>| DbTable { version: 0, has_version_marker: false, flag: 1, rows }.to_bytes(&schema).unwrap();
            let (a, b) = (sample_row(&schema, 1), sample_row(&schema, 2));
            let vanilla = encode(vec![a.clone()]);
            let more = encode(vec![a.clone(), b.clone()]);
            let path = |f: &str| format!("db\\{}_tables\\{f}", t.name);
            let vfs = two_pack_vfs(
                &format!("db_folder_keys_{}", t.name),
                &[(&path(t.name), &vanilla)],
                &[(&path("more"), &more)],
            );
            let rows = t.read(&vfs).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(rows, [a, b], "{}", t.name);
        }
    }

    /// A table with its own decoder (`models_building`, the typed tables, the campaign AI's raw
    /// tables) merges the same way through [`merged_rows`].
    #[test]
    fn merged_rows_merges_a_decoded_table_across_two_files() {
        use crate::bytes::utf16_bytes;
        let file = |keys: &[&str]| {
            let mut b = vec![1u8];
            b.extend((keys.len() as u32).to_le_bytes());
            for k in keys {
                b.extend(utf16_bytes(k));
                b.extend(utf16_bytes("buildings\\x_tech.cs2.parsed"));
                b.extend(1u32.to_le_bytes());
                b.extend(0u32.to_le_bytes());
            }
            b
        };
        let vfs = two_pack_vfs(
            "db_folder_merged_rows",
            &[("db\\models_building_tables\\models_building", &file(&["farm"]))],
            &[("db\\models_building_tables\\more", &file(&["farm", "mill"]))],
        );
        let mut warnings = Vec::new();
        let rows = merged_rows(&vfs, "models_building", &mut warnings, |b| crate::models_building::read(b).map_err(|error| TableError::Db { table: "models_building", error }), |r| r.key.as_str()).unwrap();
        assert_eq!(rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["farm", "mill"]);
        assert!(warnings.is_empty());
    }

    /// Only columns 0 and 1 joined with nothing between is the key of a table of the loader
    /// 0x00DBDB90; any other table takes any key.
    #[test]
    fn a_listed_table_merges_like_the_loader_only_on_columns_0_and_1() {
        for name in COLUMN_0_1_KEYED_TABLES {
            let keyed = |key| RawTable { name, codes: "s,s,f", key };
            assert!(keyed(RowKey::Concat(&[0, 1])).merges_like_the_loader(), "{name}");
            for wrong in [
                RowKey::Concat(&[0]),
                RowKey::Concat(&[1, 0]),
                RowKey::Concat(&[0, 1, 2]),
                RowKey::Concat(&[]),
                RowKey::Joined(&[0, 1], "_"),
                RowKey::Decimal(0),
            ] {
                assert!(!keyed(wrong).merges_like_the_loader(), "{name}");
            }
        }
        assert!(tables::ALL.iter().all(RawTable::merges_like_the_loader));
        assert!(!is_column_0_1_keyed("trait_level_effect") && !is_column_0_1_keyed("trait_level_effects_") && !is_column_0_1_keyed(""));
    }

    /// A raw read of a listed table keyed on column 0 alone is refused before any file is read
    /// (it would merge away a mod's row that shares only its column 0).
    #[test]
    #[should_panic(expected = "trait_level_effects: the exe's loader 0x00DBDB90 merges this table's files on columns 0 + 1")]
    fn a_raw_column_0_read_of_a_listed_table_is_refused() {
        let _ = RawTable { name: "trait_level_effects", codes: "s,s,f", key: RowKey::Concat(&[0]) }.read(&Vfs::new());
    }

    /// [`merged_rows`] cannot check its key closure, so it refuses a listed table whatever the key.
    #[test]
    #[should_panic(expected = "ancillary_to_effects: the exe's loader 0x00DBDB90 merges this table's files on columns 0 + 1")]
    fn merged_rows_refuses_a_listed_table() {
        let decode = |_: &[u8]| -> Result<Vec<String>, TableError> { Ok(Vec::new()) };
        let _ = merged_rows(&Vfs::new(), "ancillary_to_effects", &mut Vec::new(), decode, |k| k.as_str());
    }

    /// The same listed table keyed as its loader keys it: a mod row that shares only column 0 with
    /// a vanilla row is a new row, and one that shares both merges into it (the vanilla row stays,
    /// the file being no `bob_` one).
    #[test]
    fn a_listed_raw_table_keeps_rows_that_share_only_column_0() {
        let schema = Schema::from_codes("s,s,f").unwrap();
        let encode = |rows: &[(&str, &str, f32)]| {
            let rows = rows.iter().map(|r| vec![DbValue::Str(r.0.into()), DbValue::Str(r.1.into()), DbValue::F32(r.2)]).collect();
            DbTable { version: 0, has_version_marker: false, flag: 1, rows }.to_bytes(&schema).unwrap()
        };
        let vfs = two_pack_vfs(
            "db_folder_listed_raw",
            &[("db\\trait_level_effects_tables\\trait_level_effects", &encode(&[("level", "a", 1.0)]))],
            &[("db\\trait_level_effects_tables\\more", &encode(&[("level", "a", 2.0), ("level", "b", 3.0)]))],
        );
        let table = RawTable { name: "trait_level_effects", codes: "s,s,f", key: RowKey::Concat(&[0, 1]) };
        let rows = table.read(&vfs).unwrap();
        let cells: Vec<(&DbValue, &DbValue)> = rows.iter().map(|r| (&r[1], &r[2])).collect();
        assert_eq!(cells, [(&DbValue::Str("a".into()), &DbValue::F32(1.0)), (&DbValue::Str("b".into()), &DbValue::F32(3.0))]);
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

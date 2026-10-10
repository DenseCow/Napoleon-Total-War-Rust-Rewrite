//! Where script files come from: the install's [`Vfs`] (read-only).
//!
//! The original keeps its campaign scripts in two places (W3 §1, CONFIRMED):
//! - loose files under `data\`: `all_scripted.lua`, `campaigns\<c>\scripting.lua`;
//! - inside `data.pack`: `episodicscripting.lua`, `events.lua`, `export_*.lua`, ... at the pack
//!   root, and UI bytecode such as `ui\coreutils.luac`.
//!
//! Both are layers of the one [`Vfs`] (loose `data\` files are a layer of it, as are mods), so
//! a mod's script replaces the original's at the same path. [`ScriptSource::find`] resolves one
//! path the way our `require` needs it (INFERRED rules, see DESIGN §3.5): strip a leading
//! `data/`, try the path, then the same path with a `c` appended (`.lua` → `.luac`). Lookups are
//! case-insensitive (the VFS normalises paths).

use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use ntw_formats::db::DbValue;
use ntw_formats::db_folder::RawTable;
use ntw_formats::pack::Vfs;

/// A merged raw table, shared by every reader of it.
pub type SharedRows = Arc<Vec<Vec<DbValue>>>;

/// A found script file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptFile {
    /// The name Lua shows in error messages, e.g. `@episodicscripting.lua`.
    pub chunk_name: String,
    /// The file contents (source text or bytecode).
    pub bytes: Vec<u8>,
}

/// The places scripts are read from. Never writes anything.
#[derive(Default)]
pub struct ScriptSource {
    vfs: Option<Vfs>,
    /// Extra in-memory files (used by tests; checked first). Keys are normalised paths.
    memory: Vec<(String, Vec<u8>)>,
    /// Tables whose read failure was already logged (each is logged once, not on every call).
    table_warned: Mutex<HashSet<&'static str>>,
    /// Merged raw tables read so far (a failed read is kept as `None`: not retried).
    rows_cache: Mutex<HashMap<&'static str, Option<SharedRows>>>,
    /// Typed tables loaded so far, by record type.
    typed_cache: Mutex<HashMap<TypeId, Option<Arc<dyn Any + Send + Sync>>>>,
}

/// Lowercase, `/` separators, no leading separator, no leading `data/`.
fn normalise(path: &str) -> String {
    let p: String = path
        .trim_start_matches(['/', '\\'])
        .chars()
        .map(|c| if c == '\\' { '/' } else { c.to_ascii_lowercase() })
        .collect();
    let p = p.trim_start_matches("./");
    let p = p.strip_prefix("data/").unwrap_or(p);
    // `a/b/../c` → `a/c`: layout script overrides are relative paths such as
    // `../layout_scripts/battle_hud.lua` (CONFIRMED strings in the battle UI layouts).
    if !p.contains("..") {
        return p.to_string();
    }
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

impl ScriptSource {
    /// No files at all (add some with [`ScriptSource::with_memory_file`]).
    pub fn empty() -> Self {
        Self::default()
    }

    /// The player's install, through [`Vfs::open_install`] (its packs, loose `data\` files and
    /// the mods that are set).
    pub fn from_install(data_dir: impl AsRef<Path>) -> Result<Self, ntw_formats::pack::PackError> {
        Ok(Self::from_vfs(Vfs::open_install(data_dir)?))
    }

    /// Scripts from an already opened [`Vfs`].
    pub fn from_vfs(vfs: Vfs) -> Self {
        ScriptSource { vfs: Some(vfs), ..Self::default() }
    }

    /// A game table's rows through the one merged table reader ([`RawTable::read`]: every file of
    /// its folder, mods included), read and merged once per source: a script calls these on every
    /// Lua call, and the install does not change under a running host. `None` without a VFS
    /// (tables come from the VFS only, never from memory files) or when the table is missing or
    /// does not read; that is logged once per table (and not retried).
    pub fn table_rows_shared(&self, table: &RawTable) -> Option<SharedRows> {
        let vfs = self.vfs.as_ref()?;
        let mut cache = self.rows_cache.lock().ok()?;
        cache
            .entry(table.name)
            .or_insert_with(|| {
                let mut warnings = Vec::new();
                let rows = table.read_with(vfs, &mut warnings);
                self.log_once(table.name, &warnings, rows.as_ref().err().map(|e| e as &dyn std::fmt::Display));
                rows.ok().map(Arc::new)
            })
            .clone()
    }

    /// [`table_rows_shared`](Self::table_rows_shared) as an owned copy.
    pub fn table_rows(&self, table: &RawTable) -> Option<Vec<Vec<DbValue>>> {
        self.table_rows_shared(table).map(|r| r.as_ref().clone())
    }

    /// A typed game table through the merged view (`ntw_data::load_table`), loaded once per
    /// source. `None` as for [`table_rows`](Self::table_rows).
    pub fn typed_table<T: ntw_data::DbRecord + Send + Sync + 'static>(&self) -> Option<Arc<ntw_data::Table<T>>> {
        let vfs = self.vfs.as_ref()?;
        let mut cache = self.typed_cache.lock().ok()?;
        cache
            .entry(TypeId::of::<T>())
            .or_insert_with(|| {
                let mut warnings = Vec::new();
                let table = ntw_data::load_table::<T>(vfs, &mut warnings);
                self.log_once(T::TABLE, &warnings, table.as_ref().err().map(|e| e as &dyn std::fmt::Display));
                table.ok().map(|t| Arc::new(t) as Arc<dyn Any + Send + Sync>)
            })
            .clone()
            .and_then(|t| t.downcast::<ntw_data::Table<T>>().ok())
    }

    /// Logs a table's skipped mod files and its read failure, once per table for this source (the
    /// readers run on every call of some script functions).
    fn log_once(&self, table: &'static str, warnings: &[String], error: Option<&dyn std::fmt::Display>) {
        if (warnings.is_empty() && error.is_none()) || !self.table_warned.lock().is_ok_and(|mut w| w.insert(table)) {
            return;
        }
        for w in warnings {
            eprintln!("WARN ntw_script: {w}");
        }
        if let Some(e) = error {
            eprintln!("WARN ntw_script: table {table} not read ({e}); what it feeds stays empty");
        }
    }

    /// Adds an in-memory file (our own test scripts; never game files).
    pub fn with_memory_file(mut self, path: &str, contents: impl Into<Vec<u8>>) -> Self {
        self.memory.push((normalise(path), contents.into()));
        self
    }

    /// Finds `path` (see the module docs for the search order).
    pub fn find(&self, path: &str) -> Option<ScriptFile> {
        let norm = normalise(path);
        for candidate in [norm.clone(), format!("{norm}c")] {
            if let Some((_, b)) = self.memory.iter().find(|(p, _)| *p == candidate) {
                return Some(ScriptFile { chunk_name: format!("@{candidate}"), bytes: b.clone() });
            }
            if let Some(vfs) = &self.vfs
                && let Ok(bytes) = vfs.read(&candidate)
            {
                return Some(ScriptFile { chunk_name: format!("@{candidate}"), bytes });
            }
        }
        None
    }

    /// Whether [`ScriptSource::find`] would find `path` (the same places in the same order),
    /// without reading it.
    pub fn exists(&self, path: &str) -> bool {
        let norm = normalise(path);
        [norm.clone(), format!("{norm}c")].iter().any(|candidate| {
            self.memory.iter().any(|(p, _)| p == candidate) || self.vfs.as_ref().is_some_and(|vfs| vfs.contains(candidate))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalising() {
        assert_eq!(normalise("data/events.lua"), "events.lua");
        assert_eq!(normalise("Data\\UI\\CoreUtils.lua"), "ui/coreutils.lua");
        assert_eq!(normalise("./EpisodicScripting.lua"), "episodicscripting.lua");
    }

    #[test]
    fn memory_files_and_luac_fallback() {
        let s = ScriptSource::empty()
            .with_memory_file("a.lua", "x = 1")
            .with_memory_file("ui/b.luac", b"\x1bLua".to_vec());
        assert_eq!(s.find("data/A.lua").unwrap().bytes, b"x = 1");
        assert_eq!(s.find("data/ui/B.lua").unwrap().chunk_name, "@ui/b.luac");
        assert!(s.find("missing.lua").is_none());
        assert!(s.exists("data/A.lua") && s.exists("data/ui/B.lua"));
        assert!(!s.exists("missing.lua"));
    }

    /// A mod's additive file of a table reaches the script readers (BACKLOG §11 "One table
    /// path"): the `battles` rows of both files, merged by key; a table that is missing gives `None`.
    #[test]
    fn a_mods_additive_table_rows_reach_the_readers() {
        use ntw_formats::db::{DbTable, Schema};
        use ntw_formats::db_folder::tables::{BATTLES, WIND_LEVELS};
        use ntw_formats::pack::LayerKind;
        let dir = std::env::temp_dir().join(format!("ntw_script_table_rows_{}", std::process::id()));
        let folder = dir.join("db").join("battles_tables");
        std::fs::create_dir_all(&folder).unwrap();
        let schema = Schema::from_codes(BATTLES.codes).unwrap();
        let file = |keys: &[&str]| {
            let row = |k: &str| {
                let mut r = vec![DbValue::Str(k.into()), DbValue::Str("classic".into()), DbValue::Bool(false), DbValue::Str("x.xml".into())];
                r.extend([DbValue::OptStr(None), DbValue::I32(0), DbValue::I32(0)]);
                r.extend(vec![DbValue::Bool(false); 4]);
                r.extend([DbValue::OptStr(None), DbValue::I32(1805)]);
                r
            };
            let t = DbTable { version: 0, has_version_marker: false, flag: 1, rows: keys.iter().map(|k| row(k)).collect() };
            t.to_bytes(&schema).unwrap()
        };
        std::fs::write(folder.join("battles"), file(&["vanilla_a", "vanilla_b"])).unwrap();
        std::fs::write(folder.join("my_mod"), file(&["vanilla_a", "mod_c"])).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount_dir(&dir, LayerKind::Loose).unwrap();
        let source = ScriptSource::from_vfs(vfs);
        let keys: Vec<String> = crate::ui::frontend::read_battles(&source).into_iter().map(|b| b.key).collect();
        assert_eq!(keys, ["vanilla_a", "vanilla_b", "mod_c"]);
        // Read and merged once per source: a second call is the same rows, not a re-read (a
        // missing table is remembered too).
        assert!(Arc::ptr_eq(&source.table_rows_shared(&BATTLES).unwrap(), &source.table_rows_shared(&BATTLES).unwrap()));
        assert!(source.table_rows(&WIND_LEVELS).is_none());
        assert!(source.table_rows_shared(&WIND_LEVELS).is_none());
        assert!(ScriptSource::empty().table_rows(&BATTLES).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}

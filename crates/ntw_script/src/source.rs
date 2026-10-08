//! Where script files come from: the pack VFS and the loose files of the install (read-only).
//!
//! The original keeps its campaign scripts in two places (W3 §1, CONFIRMED):
//! - loose files under `data\`: `all_scripted.lua`, `campaigns\<c>\scripting.lua`;
//! - inside `data.pack`: `episodicscripting.lua`, `events.lua`, `export_*.lua`, ... at the pack
//!   root, and UI bytecode such as `ui\coreutils.luac`.
//!
//! [`ScriptSource::find`] resolves one path the way our `require` needs it (INFERRED rules, see
//! DESIGN §3.5): strip a leading `data/`, try the pack VFS, then the same path with a `c` appended
//! (`.lua` → `.luac`), then a loose file under the data folder. Pack lookups are case-insensitive
//! (the VFS normalises paths); loose files rely on Windows' case-insensitive file system.

use std::path::{Path, PathBuf};

use ntw_formats::pack::Vfs;

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
    data_dir: Option<PathBuf>,
    /// Extra in-memory files (used by tests; checked first). Keys are normalised paths.
    memory: Vec<(String, Vec<u8>)>,
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

    /// The player's install: mounts every pack in `data_dir` and also reads loose files there.
    pub fn from_install(data_dir: impl AsRef<Path>) -> Result<Self, ntw_formats::pack::PackError> {
        let data_dir = data_dir.as_ref();
        Ok(ScriptSource {
            vfs: Some(Vfs::open_install(data_dir)?),
            data_dir: Some(data_dir.to_path_buf()),
            memory: Vec::new(),
        })
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
            if let Some(dir) = &self.data_dir {
                let file = dir.join(candidate.replace('/', std::path::MAIN_SEPARATOR_STR));
                if let Ok(bytes) = std::fs::read(&file) {
                    return Some(ScriptFile { chunk_name: format!("@{candidate}"), bytes });
                }
            }
        }
        None
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
    }
}

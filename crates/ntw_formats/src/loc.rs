//! `.loc` localisation files: key → on-screen text.
//!
//! # Where they live
//! `text\localisation.loc` and `text\ui.loc` in the language packs
//! (`local_en.pack`, and newer copies in `local_en_patch.pack`).
//!
//! # Layout (Worker 3 `DB_CAMPAIGN_TABLES.md` §1, CONFIRMED by an exact-EOF parse of all 4 shipped files)
//! ```text
//! 0x00  FF FE                 UTF-16LE byte-order mark
//! 0x02  4C 4F 43 00           "LOC\0"
//! 0x06  u32 version           1
//! 0x0A  u32 entry_count
//! 0x0E  entry_count x {
//!         u16 n, n UTF-16LE units   key
//!         u16 m, m UTF-16LE units   text
//!         u8                        flag (meaning UNKNOWN; set on a few ui.loc entries)
//!       }
//! EOF
//! ```
//!
//! # Overriding
//! The patch pack contains files at the *same paths* as the release pack, so the
//! [`Vfs`] already hands you the patched file. [`Localisation::from_vfs`] loads the two
//! files the original opens (`localisation.loc`, `ui.loc`) into one lookup. A mod that
//! changes text ships the whole file at the same path (whole-file replacement, as in the
//! original); see `analysis/mods/MOD_LOADING.md`.

use std::collections::HashMap;
use std::fmt;

use crate::bytes::{Cursor, ReadError};
use crate::pack::{LayerKind, PackError, Vfs};

/// The 6 bytes every `.loc` file starts with: a UTF-16LE BOM and `"LOC\0"`.
pub const LOC_MAGIC: [u8; 6] = [0xFF, 0xFE, b'L', b'O', b'C', 0];

/// One localisation entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocEntry {
    /// The lookup key, e.g. `units_onscreen_name_euro_line_infantry`.
    pub key: String,
    /// The displayed text.
    pub text: String,
    /// The trailing flag byte (meaning UNKNOWN).
    pub flag: bool,
}

/// One parsed `.loc` file, with entries in file order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocFile {
    /// Format version (1 in every shipped file).
    pub version: u32,
    /// The entries, in file order.
    pub entries: Vec<LocEntry>,
}

impl LocFile {
    /// Parses a `.loc` file. The entries must end exactly at the end of the data.
    pub fn read(bytes: &[u8]) -> Result<Self, LocError> {
        let mut c = Cursor::new(bytes);
        if c.peek(6) != Some(&LOC_MAGIC[..]) {
            return Err(LocError::BadMagic);
        }
        c.take(6)?;
        let version = c.u32()?;
        let count = c.u32()?;
        // Each entry is at least 5 bytes (two empty strings and the flag).
        let mut entries = Vec::with_capacity((count as usize).min(c.remaining() / 5));
        for _ in 0..count {
            let key = c.utf16()?;
            let text = c.utf16()?;
            let flag = c.u8()? != 0;
            entries.push(LocEntry { key, text, flag });
        }
        if c.remaining() != 0 {
            return Err(LocError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { version, entries })
    }
}

/// All loaded text, keyed for lookup. If two files define the same key, the one added later wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Localisation {
    map: HashMap<String, String>,
}

impl Localisation {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds every entry of `file`, overriding existing keys.
    pub fn add(&mut self, file: &LocFile) {
        for e in &file.entries {
            self.map.insert(e.key.clone(), e.text.clone());
        }
    }

    /// Loads the game's text the way the original does, through the VFS (so a patch or mod
    /// copy at the same path replaces the whole file):
    ///
    /// 1. `text\localisation.loc`, then `text\ui.loc`. These are the only two paths the
    ///    original opens (CONFIRMED: both are hard-coded strings in `Napoleon.exe`, and
    ///    `text/localisation.loc` is referenced by the database loader at `0x00E20760`). A
    ///    differently named `.loc` in an original-style mod pack is ignored, as in the original.
    /// 2. Our extension: any other `text\*.loc` that comes from the `mods\` folder layer
    ///    ([`LayerKind::ModsFolder`]), lowest priority first, so a mod can ship only the
    ///    strings it changes.
    ///
    /// Missing files are skipped (a bare test VFS may have neither).
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, LocFromVfsError> {
        const FIXED: [&str; 2] = ["text\\localisation.loc", "text\\ui.loc"];
        let mut paths: Vec<&str> = FIXED.iter().copied().filter(|p| vfs.contains(p)).collect();
        let mut extra: Vec<(usize, &str)> = vfs
            .list("text/")
            .into_iter()
            .filter(|p| p.ends_with(".loc") && !FIXED.contains(p))
            .filter_map(|p| {
                let layer = vfs.origin_index(p)?;
                (vfs.layers()[layer].kind == LayerKind::ModsFolder).then_some((layer, p))
            })
            .collect();
        // Lowest priority first, so later adds win: in the mods folder the first listed (lowest
        // layer index) wins ties.
        extra.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        paths.extend(extra.into_iter().map(|(_, p)| p));
        let mut loc = Self::new();
        for path in paths {
            let bytes = vfs.read(path).map_err(LocFromVfsError::Pack)?;
            let file = LocFile::read(&bytes)
                .map_err(|error| LocFromVfsError::Loc { path: path.to_owned(), error })?;
            loc.add(&file);
        }
        Ok(loc)
    }

    /// The text for `key`, if any.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// Every (key, text) pair, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True if no keys are loaded.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Errors from [`LocFile::read`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocError {
    /// The file does not start with `FF FE "LOC\0"`.
    BadMagic,
    /// The data ended mid-entry.
    UnexpectedEof {
        /// Where the read started.
        offset: usize,
        /// How many bytes were needed.
        needed: usize,
    },
    /// A string had an unpaired UTF-16 surrogate.
    InvalidUtf16 {
        /// Where the string starts.
        offset: usize,
    },
    /// Bytes remain after the last entry.
    TrailingBytes {
        /// Where they start.
        offset: usize,
        /// How many.
        count: usize,
    },
}

impl From<ReadError> for LocError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for LocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "not a .loc file (missing FF FE \"LOC\\0\")"),
            Self::UnexpectedEof { offset, needed } => {
                write!(f, ".loc data ended at 0x{offset:x} (needed {needed} more bytes)")
            }
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 string at 0x{offset:x}"),
            Self::TrailingBytes { offset, count } => write!(f, "{count} unexpected bytes at 0x{offset:x}"),
        }
    }
}

impl std::error::Error for LocError {}

/// Errors from [`Localisation::from_vfs`].
#[derive(Debug)]
pub enum LocFromVfsError {
    /// Reading a file from the packs failed.
    Pack(PackError),
    /// A file was read but is not a valid `.loc`.
    Loc {
        /// The file's path.
        path: String,
        /// What was wrong with it.
        error: LocError,
    },
}

impl fmt::Display for LocFromVfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(e) => write!(f, "{e}"),
            Self::Loc { path, error } => write!(f, "{path}: {error}"),
        }
    }
}

impl std::error::Error for LocFromVfsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Pack(e) => Some(e),
            Self::Loc { error, .. } => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::utf16_bytes;

    fn loc_bytes(entries: &[(&str, &str, u8)]) -> Vec<u8> {
        let mut b = LOC_MAGIC.to_vec();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (k, t, f) in entries {
            b.extend(utf16_bytes(k));
            b.extend(utf16_bytes(t));
            b.push(*f);
        }
        b
    }

    #[test]
    fn reads_entries() {
        let f = LocFile::read(&loc_bytes(&[("greeting", "Bonjour \u{e0} tous", 0), ("empty", "", 1)])).unwrap();
        assert_eq!(f.version, 1);
        assert_eq!(f.entries.len(), 2);
        assert_eq!(f.entries[0].text, "Bonjour \u{e0} tous");
        assert!(f.entries[1].flag);
    }

    #[test]
    fn later_files_override() {
        let mut loc = Localisation::new();
        loc.add(&LocFile::read(&loc_bytes(&[("a", "old", 0), ("b", "kept", 0)])).unwrap());
        loc.add(&LocFile::read(&loc_bytes(&[("a", "new", 0)])).unwrap());
        assert_eq!((loc.get("a"), loc.get("b"), loc.len()), (Some("new"), Some("kept"), 2));
        assert_eq!(loc.get("zzz"), None);
    }

    #[test]
    fn rejects_bad_input() {
        let good = loc_bytes(&[("k", "v", 0)]);
        for len in 0..good.len() {
            assert!(LocFile::read(&good[..len]).is_err(), "len {len}");
        }
        let mut bad = good.clone();
        bad[0] = 0;
        assert_eq!(LocFile::read(&bad), Err(LocError::BadMagic));
        let mut extra = good.clone();
        extra.push(9);
        assert!(matches!(LocFile::read(&extra), Err(LocError::TrailingBytes { count: 1, .. })));
        let mut huge = good;
        huge[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(LocFile::read(&huge).is_err());
    }

    #[test]
    fn from_vfs_uses_patched_file() {
        use crate::pack::test_util::{build_pack, temp_dir};
        let dir = temp_dir("loc_vfs");
        let release = loc_bytes(&[("a", "release", 0)]);
        let patch = loc_bytes(&[("a", "patch", 0)]);
        let ui = loc_bytes(&[("u", "ui", 0)]);
        std::fs::write(
            dir.join("local_en.pack"),
            build_pack(1, &[("text\\localisation.loc", &release), ("text\\ui.loc", &ui)]),
        )
        .unwrap();
        std::fs::write(dir.join("local_en_patch.pack"), build_pack(2, &[("text\\localisation.loc", &patch)]))
            .unwrap();
        let vfs = Vfs::open_install(&dir).unwrap();
        let loc = Localisation::from_vfs(&vfs).unwrap();
        assert_eq!((loc.get("a"), loc.get("u")), (Some("patch"), Some("ui")));
    }
}

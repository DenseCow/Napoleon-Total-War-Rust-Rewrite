//! `.pack` archives ("PFH0") and the [`Vfs`] that layers them in load order.
//!
//! # What a pack is
//! Almost every game file (DB tables, textures, models, scripts, text) lives inside
//! one of the ~11 `.pack` files in the install's `data\` folder. A pack is a simple
//! uncompressed archive: a header, an index of `(size, path)` pairs, then all the
//! file contents back to back.
//!
//! # Layout (Worker 2, verified on all 11 shipped packs; Worker 3's reader agrees)
//! All integers are little-endian.
//! ```text
//! 0x00  [u8;4]  magic "PFH0"
//! 0x04  u32     pack type: 0 boot, 1 release, 2 patch, 3 mod, 4 movie
//! 0x08  u32     dependency count        (0 in every shipped pack)
//! 0x0C  u32     dependency block bytes  (0 in every shipped pack)
//! 0x10  u32     file count
//! 0x14  u32     index block bytes
//! 0x18  dependency names, NUL-terminated
//! ....  index: file_count x { u32 size; NUL-terminated path using '\' }
//! ....  payload: files back to back, uncompressed, in index order
//! ```
//! A file's offset is `0x18 + dep_bytes + index_bytes` plus the sizes of all files before it.
//!
//! # Read-only by design
//! [`PackFile`] opens files with [`std::fs::File::open`] (read-only) and has no
//! method that writes. Only the index is kept in memory. Entry contents are fetched
//! on demand with a seek and read, so a 4 GB pack never has to fit in RAM.

mod file;
mod vfs;

pub use file::{PackEntry, PackFile, PackHeader, PackType};
pub use vfs::{Vfs, effective_language, install_language, installed_languages, set_language_override};

use std::fmt;

/// Turns a game path into the form used for lookups: `/` becomes `\`, ASCII letters
/// become lowercase, and leading separators are removed.
///
/// `"DB/units_tables/units"` and `"db\\units_tables\\units"` both become `db\units_tables\units`.
pub fn normalize_path(path: &str) -> String {
    path.trim_start_matches(['/', '\\'])
        .chars()
        .map(|c| if c == '/' { '\\' } else { c.to_ascii_lowercase() })
        .collect()
}

/// Everything that can go wrong while reading packs.
#[derive(Debug)]
pub enum PackError {
    /// The operating system failed to open, seek or read a file.
    Io(std::io::Error),
    /// The file does not start with `PFH0`.
    BadMagic([u8; 4]),
    /// The header, dependency list or index runs past the end of the file.
    Truncated {
        /// What was being read.
        what: &'static str,
    },
    /// The index block is malformed (an unterminated path, or leftover bytes).
    BadIndex {
        /// Which entry (0-based) was being read.
        entry: usize,
    },
    /// An entry's data would extend past the end of the pack file.
    EntryOutOfBounds {
        /// The entry's path.
        path: String,
    },
    /// [`Vfs::read`] was asked for a path that no mounted pack contains.
    NotFound(String),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::BadMagic(m) => write!(f, "not a PFH0 pack (magic {m:02x?})"),
            Self::Truncated { what } => write!(f, "pack truncated while reading the {what}"),
            Self::BadIndex { entry } => write!(f, "malformed pack index at entry {entry}"),
            Self::EntryOutOfBounds { path } => write!(f, "entry {path:?} extends past the end of the pack"),
            Self::NotFound(p) => write!(f, "{p:?} is not in any mounted pack"),
        }
    }
}

impl std::error::Error for PackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PackError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    //! Builds small packs in the OS temp folder for unit tests.
    use std::path::PathBuf;

    /// Encodes a pack with the given type and `(path, contents)` files.
    pub fn build_pack(pack_type: u32, files: &[(&str, &[u8])]) -> Vec<u8> {
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

    /// A fresh, empty temp directory unique to this test.
    pub fn temp_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ntw_formats_{}_{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize_path("/DB/Units_Tables/units"), "db\\units_tables\\units");
        assert_eq!(normalize_path("text\\ui.loc"), "text\\ui.loc");
    }
}

//! [`GameFiles`]: the read-only view of the install's files that the format readers take (campaign
//! maps, materials, terrain images), and [`FileSpan`], where a game file's bytes lie on disk.

use super::Vfs;

/// Where game files are read from: the install's [`Vfs`], which also mounts the loose files under
/// `data\` (the campaign maps and start positions ship as loose files, `data\campaign_maps\`,
/// `data\campaigns\`; the original reads loose files through its VFS, "non_pack") and the mods, so
/// a mod can replace any of them.
#[derive(Clone, Copy)]
pub struct GameFiles<'a> {
    /// The install's files (packs, loose `data\` files and mods).
    pub vfs: &'a Vfs,
}

/// A game file's bytes on disk: `len` bytes at `offset` in `file` (a pack, or the loose file at 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpan {
    /// The file holding the bytes.
    pub file: std::path::PathBuf,
    /// Where they start in it.
    pub offset: u64,
    /// How many there are.
    pub len: u64,
}

impl FileSpan {
    /// Reads `len` bytes starting `start` bytes into the span from `file` (this span's file, open);
    /// an error past the span's end.
    pub fn read_at(&self, file: &mut std::fs::File, start: u64, len: usize) -> Result<Vec<u8>, String> {
        use std::io::{Read, Seek, SeekFrom};
        if start.checked_add(len as u64).is_none_or(|end| end > self.len) {
            return Err(format!("{}: bytes {start}..+{len} past its end ({})", self.file.display(), self.len));
        }
        file.seek(SeekFrom::Start(self.offset + start)).map_err(|e| format!("{}: {e}", self.file.display()))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).map_err(|e| format!("{}: {e}", self.file.display()))?;
        Ok(buf)
    }
}

impl GameFiles<'_> {
    /// Reads a game path (case-insensitive, `/` or `\`), read-only.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        self.vfs.read(path).map_err(|e| format!("{path}: {e}"))
    }

    /// Where the winning copy of a game path lies on disk (the copy [`read`](Self::read) gives): a
    /// pack's file at its entry, or the loose file. For reading parts of a large file (the
    /// supertexture's tiles) by range without reading all of it, and without holding the Vfs.
    pub fn locate(&self, path: &str) -> Result<FileSpan, String> {
        if let Some((pack, entry)) = self.vfs.find(path) {
            return Ok(FileSpan { file: pack.path().to_owned(), offset: entry.offset, len: u64::from(entry.size) });
        }
        let file = self.vfs.loose_path(path).ok_or_else(|| format!("{path}: not found"))?;
        let len = self.vfs.file_size(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(FileSpan { file, offset: 0, len })
    }

    /// All files whose normalized path starts with `prefix` (a folder ending in `/` or `\`),
    /// normalized and sorted.
    pub fn list(&self, prefix: &str) -> Vec<String> {
        self.vfs.list(prefix).into_iter().map(str::to_owned).collect()
    }
}

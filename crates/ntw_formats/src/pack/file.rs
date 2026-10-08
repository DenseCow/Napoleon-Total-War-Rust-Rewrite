//! One `.pack` file: header, index and on-demand entry reads.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{PackError, normalize_path};

/// The pack's role, from header bytes 0x04..0x08. It decides the load order in [`super::Vfs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackType {
    /// 0: loaded first, before everything else (e.g. `boot.pack`).
    Boot,
    /// 1: the main game content (e.g. `data.pack`, `local_en.pack`).
    Release,
    /// 2: official patches that override release content (e.g. `local_en_patch.pack`).
    Patch,
    /// 3: user mods.
    Mod,
    /// 4: Bink movies (`media.pack`).
    Movie,
    /// Any other value; kept so nothing is lost.
    Other(u32),
}

impl From<u32> for PackType {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Boot,
            1 => Self::Release,
            2 => Self::Patch,
            3 => Self::Mod,
            4 => Self::Movie,
            other => Self::Other(other),
        }
    }
}

/// The parsed fixed header and dependency list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackHeader {
    /// Always `*b"PFH0"`.
    pub magic: [u8; 4],
    /// The raw type value (0..=4 for shipped packs).
    pub raw_type: u32,
    /// `raw_type` as an enum.
    pub pack_type: PackType,
    /// Names of packs this pack depends on (empty in every shipped pack).
    pub dependencies: Vec<String>,
    /// Number of files in the index.
    pub file_count: u32,
    /// Size of the index block in bytes.
    pub index_size: u32,
    /// Absolute offset where the first file's data starts.
    pub data_start: u64,
}

/// One file inside a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackEntry {
    /// The path exactly as stored, with `\` separators, e.g. `db\units_tables\units`.
    pub path: String,
    /// Size of the file's data in bytes.
    pub size: u32,
    /// Absolute byte offset of the file's data inside the pack.
    pub offset: u64,
}

/// An open `.pack` file. It holds the header and index in memory, never the contents.
///
/// ```no_run
/// use ntw_formats::pack::PackFile;
/// let pack = PackFile::open(r"C:\Games\Napoleon Total War\data\data.pack").unwrap();
/// let entry = pack.find("db/units_tables/units").unwrap();
/// let bytes = pack.read_entry(entry).unwrap();
/// ```
#[derive(Debug)]
pub struct PackFile {
    path: PathBuf,
    header: PackHeader,
    file_len: u64,
    entries: Vec<PackEntry>,
    /// Normalized path -> index into `entries`.
    lookup: HashMap<String, usize>,
    /// Read-only handle. The mutex lets `read_entry` take `&self` while seeking safely from several threads.
    file: Mutex<File>,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Reads exactly `len` bytes, reporting a short file as [`PackError::Truncated`].
fn read_block(f: &mut File, len: usize, what: &'static str) -> Result<Vec<u8>, PackError> {
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::UnexpectedEof => PackError::Truncated { what },
        _ => PackError::Io(e),
    })?;
    Ok(buf)
}

impl PackFile {
    /// Opens a pack **read-only** and parses its header and index.
    ///
    /// Every entry is checked to lie inside the file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PackError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let file_len = file.metadata()?.len();

        let h = read_block(&mut file, 24, "header")?;
        let magic = [h[0], h[1], h[2], h[3]];
        if &magic != b"PFH0" {
            return Err(PackError::BadMagic(magic));
        }
        let raw_type = u32_at(&h, 4);
        let dep_count = u32_at(&h, 8) as usize;
        let dep_bytes = u32_at(&h, 12);
        let file_count = u32_at(&h, 16);
        let index_size = u32_at(&h, 20);

        // Check the sizes against the real file length *before* allocating buffers for them.
        let data_start = 24 + u64::from(dep_bytes) + u64::from(index_size);
        if data_start > file_len {
            return Err(PackError::Truncated { what: "index" });
        }
        let deps = read_block(&mut file, dep_bytes as usize, "dependency list")?;
        let dependencies = deps
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .take(dep_count)
            .map(|s| s.iter().map(|&c| char::from(c)).collect())
            .collect();
        let index = read_block(&mut file, index_size as usize, "index")?;

        // Each index entry is at least 5 bytes (size + NUL), so this caps the allocation.
        let mut entries = Vec::with_capacity((file_count as usize).min(index.len() / 5));
        let (mut p, mut offset) = (0usize, data_start);
        for i in 0..file_count as usize {
            if p + 4 > index.len() {
                return Err(PackError::BadIndex { entry: i });
            }
            let size = u32_at(&index, p);
            let name_start = p + 4;
            let nul = index[name_start..]
                .iter()
                .position(|&c| c == 0)
                .ok_or(PackError::BadIndex { entry: i })?;
            // Paths are ASCII in practice. Decode as Latin-1 so no byte is ever rejected.
            let entry_path: String =
                index[name_start..name_start + nul].iter().map(|&c| char::from(c)).collect();
            if offset + u64::from(size) > file_len {
                return Err(PackError::EntryOutOfBounds { path: entry_path });
            }
            entries.push(PackEntry { path: entry_path, size, offset });
            offset += u64::from(size);
            p = name_start + nul + 1;
        }
        if p != index.len() {
            return Err(PackError::BadIndex { entry: file_count as usize });
        }

        let mut lookup = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            // If a pack lists the same path twice, the later entry wins (matches Vfs overriding).
            lookup.insert(normalize_path(&e.path), i);
        }
        let header = PackHeader {
            magic,
            raw_type,
            pack_type: raw_type.into(),
            dependencies,
            file_count,
            index_size,
            data_start,
        };
        Ok(Self { path, header, file_len, entries, lookup, file: Mutex::new(file) })
    }

    /// Where this pack lives on disk.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The parsed header.
    pub fn header(&self) -> &PackHeader {
        &self.header
    }

    /// The pack's type (boot / release / patch / mod / movie).
    pub fn pack_type(&self) -> PackType {
        self.header.pack_type
    }

    /// Size of the pack file in bytes.
    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    /// All entries in index order.
    pub fn entries(&self) -> &[PackEntry] {
        &self.entries
    }

    /// Finds an entry by path, ignoring ASCII case and accepting `/` or `\`.
    pub fn find(&self, path: &str) -> Option<&PackEntry> {
        self.lookup.get(&normalize_path(path)).map(|&i| &self.entries[i])
    }

    /// Reads one entry's contents (seek + read; nothing else is loaded).
    pub fn read_entry(&self, entry: &PackEntry) -> Result<Vec<u8>, PackError> {
        self.read_entry_prefix(entry, entry.size as usize)
    }

    /// Reads at most the first `max_len` bytes of an entry (handy for sniffing magic numbers).
    pub fn read_entry_prefix(&self, entry: &PackEntry, max_len: usize) -> Result<Vec<u8>, PackError> {
        self.read_entry_range(entry, 0, max_len)
    }

    /// Reads at most `max_len` bytes of an entry, starting `start` bytes into it. Used to stream
    /// large files such as movies piece by piece. Reads past the entry's end are clamped.
    pub fn read_entry_range(&self, entry: &PackEntry, start: u64, max_len: usize) -> Result<Vec<u8>, PackError> {
        let start = start.min(entry.size as u64);
        let len = (max_len as u64).min(entry.size as u64 - start) as usize;
        if entry.offset + start + len as u64 > self.file_len {
            return Err(PackError::EntryOutOfBounds { path: entry.path.clone() });
        }
        // A poisoned lock only means another thread panicked mid-read. The handle is still fine,
        // because every read seeks first.
        let mut file = self.file.lock().unwrap_or_else(|p| p.into_inner());
        file.seek(SeekFrom::Start(entry.offset + start))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{build_pack, temp_dir};
    use super::*;

    #[test]
    fn opens_and_reads_entries() {
        let dir = temp_dir("pack_open");
        let p = dir.join("t.pack");
        std::fs::write(&p, build_pack(1, &[("db\\a_tables\\a", b"hello"), ("text\\ui.loc", b"xy")])).unwrap();
        let pack = PackFile::open(&p).unwrap();
        assert_eq!(pack.pack_type(), PackType::Release);
        assert_eq!(pack.entries().len(), 2);
        let e = &pack.entries()[1];
        assert_eq!((e.path.as_str(), e.size, e.offset), ("text\\ui.loc", 2, pack.header().data_start + 5));
        assert_eq!(pack.read_entry(pack.find("DB/A_tables/a").unwrap()).unwrap(), b"hello");
        assert_eq!(pack.read_entry(e).unwrap(), b"xy");
        assert_eq!(pack.read_entry_prefix(&pack.entries()[0], 2).unwrap(), b"he");
        assert!(pack.find("missing").is_none());
    }

    #[test]
    fn rejects_bad_magic_and_truncation() {
        let dir = temp_dir("pack_bad");
        let good = build_pack(0, &[("a", b"1234")]);

        let mut bad = good.clone();
        bad[3] = b'X';
        std::fs::write(dir.join("m.pack"), &bad).unwrap();
        assert!(matches!(PackFile::open(dir.join("m.pack")), Err(PackError::BadMagic(_))));

        // Every truncation must fail cleanly. Cutting into the data gives EntryOutOfBounds.
        for len in 0..good.len() {
            std::fs::write(dir.join("t.pack"), &good[..len]).unwrap();
            assert!(PackFile::open(dir.join("t.pack")).is_err(), "len {len} opened");
        }
    }

    #[test]
    fn rejects_unterminated_index_and_huge_counts() {
        let dir = temp_dir("pack_index");
        let mut b = build_pack(1, &[("a", b"z")]);
        b[24 + 5] = b'b'; // overwrite the path's NUL terminator
        std::fs::write(dir.join("u.pack"), &b).unwrap();
        assert!(matches!(PackFile::open(dir.join("u.pack")), Err(PackError::BadIndex { .. })));

        let mut b = build_pack(1, &[("a", b"z")]);
        b[16..20].copy_from_slice(&u32::MAX.to_le_bytes()); // absurd file count
        std::fs::write(dir.join("c.pack"), &b).unwrap();
        assert!(matches!(PackFile::open(dir.join("c.pack")), Err(PackError::BadIndex { .. })));

        let mut b = build_pack(1, &[("a", b"z")]);
        b[20..24].copy_from_slice(&u32::MAX.to_le_bytes()); // index larger than the file
        std::fs::write(dir.join("i.pack"), &b).unwrap();
        assert!(matches!(PackFile::open(dir.join("i.pack")), Err(PackError::Truncated { .. })));
    }
}

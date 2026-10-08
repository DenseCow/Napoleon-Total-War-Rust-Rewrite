//! Read-only reader for Creative Assembly "PFH0" pack files as shipped with
//! Napoleon: Total War (2010).
//!
//! Only the 24-byte header and the file index are read; payloads are fetched
//! on demand with `seek` + `read_exact` so a 4 GB pack is never loaded.
//!
//! # Layout (verified against all 11 shipped packs)
//! ```text
//! 0x00  [u8;4]  magic  "PFH0"
//! 0x04  u32     pack type   0=boot 1=release 2=patch 3=mod 4=movie
//! 0x08  u32     dependency count        (0 in every shipped pack)
//! 0x0C  u32     dependency block bytes  (0 in every shipped pack)
//! 0x10  u32     file count
//! 0x14  u32     index block bytes
//! 0x18  dependency names, NUL-terminated ASCII
//! ....  index: file_count x { u32 size; NUL-terminated path using '\\' }
//! ....  payload: files stored back-to-back, uncompressed, in index order
//! ```
//! Payload start = 0x18 + dep_bytes + index_bytes. Each file's offset is the
//! running sum of earlier sizes; the last file ends exactly at EOF.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Default install location (opened read-only only).
pub const DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

/// The `type` field of the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackType {
    Boot,
    Release,
    Patch,
    Mod,
    Movie,
    Other(u32),
}

impl From<u32> for PackType {
    fn from(v: u32) -> Self {
        match v {
            0 => PackType::Boot,
            1 => PackType::Release,
            2 => PackType::Patch,
            3 => PackType::Mod,
            4 => PackType::Movie,
            x => PackType::Other(x),
        }
    }
}

/// One file inside a pack.
#[derive(Debug, Clone)]
pub struct PackEntry {
    /// Path inside the pack, with '\\' separators, as stored.
    pub path: String,
    pub size: u32,
    /// Absolute byte offset of the payload in the pack file.
    pub offset: u64,
}

/// Parsed header + index of a pack.
#[derive(Debug)]
pub struct PackIndex {
    pub file: PathBuf,
    pub magic: [u8; 4],
    pub raw_type: u32,
    pub pack_type: PackType,
    pub dependencies: Vec<String>,
    pub data_start: u64,
    pub file_len: u64,
    pub entries: Vec<PackEntry>,
}

fn rd_u32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}

impl PackIndex {
    /// Open a pack read-only and parse header + index.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let mut f = File::open(path)?;
        let file_len = f.metadata()?.len();
        let mut hdr = [0u8; 24];
        f.read_exact(&mut hdr)?;
        let magic = [hdr[0], hdr[1], hdr[2], hdr[3]];
        if &magic[..3] != b"PFH" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a PFH pack"));
        }
        let raw_type = rd_u32(&hdr, 4);
        let ndep = rd_u32(&hdr, 8) as usize;
        let dep_bytes = rd_u32(&hdr, 12) as usize;
        let nfiles = rd_u32(&hdr, 16) as usize;
        let idx_bytes = rd_u32(&hdr, 20) as usize;
        let mut dep = vec![0u8; dep_bytes];
        f.read_exact(&mut dep)?;
        let dependencies = dep
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .take(ndep)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        let mut idx = vec![0u8; idx_bytes];
        f.read_exact(&mut idx)?;
        let data_start = (24 + dep_bytes + idx_bytes) as u64;
        let mut entries = Vec::with_capacity(nfiles);
        let (mut p, mut off) = (0usize, data_start);
        for _ in 0..nfiles {
            let size = rd_u32(&idx, p);
            let s = p + 4;
            let e = s + idx[s..].iter().position(|&c| c == 0).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "unterminated path")
            })?;
            // Paths are ASCII in practice; latin-1 decode to be safe.
            let path: String = idx[s..e].iter().map(|&c| c as char).collect();
            entries.push(PackEntry { path, size, offset: off });
            off += size as u64;
            p = e + 1;
        }
        if p != idx_bytes {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "index size mismatch"));
        }
        Ok(PackIndex {
            file: path.to_path_buf(),
            magic,
            raw_type,
            pack_type: raw_type.into(),
            dependencies,
            data_start,
            file_len,
            entries,
        })
    }

    /// End offset of the last payload (should equal `file_len`).
    pub fn payload_end(&self) -> u64 {
        self.entries.last().map(|e| e.offset + e.size as u64).unwrap_or(self.data_start)
    }

    /// Case-insensitive lookup by path.
    pub fn find(&self, path: &str) -> Option<&PackEntry> {
        let p = path.replace('/', "\\").to_ascii_lowercase();
        self.entries.iter().find(|e| e.path.to_ascii_lowercase() == p)
    }

    /// Read (at most `max` bytes of) one entry by seeking; never touches other payloads.
    pub fn read(&self, e: &PackEntry, max: Option<usize>) -> io::Result<Vec<u8>> {
        let mut f = File::open(&self.file)?;
        f.seek(SeekFrom::Start(e.offset))?;
        let n = max.map(|m| m.min(e.size as usize)).unwrap_or(e.size as usize);
        let mut buf = vec![0u8; n];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// Resolve a pack name ("data" / "data.pack" / full path) to a path.
pub fn resolve(name: &str) -> PathBuf {
    let p = Path::new(name);
    if p.exists() {
        return p.to_path_buf();
    }
    let n = if name.ends_with(".pack") { name.to_string() } else { format!("{name}.pack") };
    Path::new(DATA_DIR).join(n)
}

/// All `*.pack` files in the data dir, sorted by name.
pub fn all_packs() -> io::Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(DATA_DIR)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "pack").unwrap_or(false))
        .collect();
    v.sort();
    Ok(v)
}

//! Minimal read-only PFH0 pack reader (own implementation; format as documented
//! by Worker 2: 24-byte header, index of {u32 size, NUL-terminated path}, payloads
//! back-to-back in index order). Payloads are fetched with seek + read_exact.
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

#[derive(Clone, Debug)]
pub struct Entry { pub path: String, pub size: u32, pub offset: u64 }

pub struct Pack { pub file: PathBuf, pub entries: Vec<Entry> }

fn u32le(b: &[u8], p: usize) -> u32 { u32::from_le_bytes([b[p], b[p+1], b[p+2], b[p+3]]) }

impl Pack {
    pub fn open(p: impl AsRef<Path>) -> io::Result<Pack> {
        let path = p.as_ref().to_path_buf();
        let mut f = File::open(&path)?;
        let mut h = [0u8; 24];
        f.read_exact(&mut h)?;
        if &h[0..4] != b"PFH0" { return Err(io::Error::new(io::ErrorKind::InvalidData, "not PFH0")); }
        let dep_bytes = u32le(&h, 12) as usize;
        let n = u32le(&h, 16) as usize;
        let idx_bytes = u32le(&h, 20) as usize;
        let mut buf = vec![0u8; dep_bytes + idx_bytes];
        f.read_exact(&mut buf)?;
        let idx = &buf[dep_bytes..];
        let mut off = (24 + dep_bytes + idx_bytes) as u64;
        let mut p = 0usize;
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let size = u32le(idx, p);
            let s = p + 4;
            let e = s + idx[s..].iter().position(|&c| c == 0).unwrap();
            entries.push(Entry { path: idx[s..e].iter().map(|&c| c as char).collect(), size, offset: off });
            off += size as u64;
            p = e + 1;
        }
        Ok(Pack { file: path, entries })
    }
    pub fn read(&self, e: &Entry) -> io::Result<Vec<u8>> {
        let mut f = File::open(&self.file)?;
        f.seek(SeekFrom::Start(e.offset))?;
        let mut v = vec![0u8; e.size as usize];
        f.read_exact(&mut v)?;
        Ok(v)
    }
    pub fn read_head(&self, e: &Entry, n: usize) -> io::Result<Vec<u8>> {
        let mut f = File::open(&self.file)?;
        f.seek(SeekFrom::Start(e.offset))?;
        let mut v = vec![0u8; n.min(e.size as usize)];
        f.read_exact(&mut v)?;
        Ok(v)
    }
    pub fn find(&self, path: &str) -> Option<&Entry> {
        let l = path.to_ascii_lowercase().replace('/', "\\");
        self.entries.iter().find(|e| e.path.to_ascii_lowercase() == l)
    }
}

pub const PACKS: &[&str] = &["boot.pack", "data.pack", "local_en.pack", "local_en_patch.pack", "battleterrain.pack",
    "buildings.pack", "rigidmodels.pack", "sound.pack", "variantmodels.pack", "variantmodels2.pack", "media.pack"];

pub fn data_path(name: &str) -> PathBuf { Path::new(DATA_DIR).join(name) }

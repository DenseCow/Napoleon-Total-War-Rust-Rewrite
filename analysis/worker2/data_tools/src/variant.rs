//! `.unit_variant` ("VRNT") reader: which mesh parts and textures make up one
//! soldier / officer / musician / standard bearer of a unit.
//!
//! Layout (see UNIT_VARIANT_AND_TEXTURES.md for the confidence of each field):
//! ```text
//! 0x00 [4]  "VRNT"
//! 0x04 u32  version (0 in all shipped files)
//! 0x08 u32  category_count
//! 0x0C u32  header_size = category table offset (always 0x14)
//! 0x10 u32  mesh_table_offset (= 0x14 + category_count * 528)
//! 0x14      category_count x CATEGORY (528 bytes):
//!             [u16;256] name, UTF-16LE, NUL-padded ("hands", "Heads", "Hats", ...)
//!             u32 index, u32 unk, u32 mesh_count, u32 first_mesh
//! mesh_table_offset: N x MESH (1026 bytes):
//!             [u16;256] mesh path     (UTF-16LE, NUL-padded; '/' separators, no extension)
//!             [u16;256] texture path / texture folder
//!             u16 unk
//! ```

use std::fmt;

pub const CATEGORY_SIZE: usize = 528;
pub const MESH_SIZE: usize = 1026;

#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    pub index: u32,
    pub unk: u32,
    pub mesh_count: u32,
    pub first_mesh: u32,
}

#[derive(Debug, Clone)]
pub struct MeshEntry {
    pub mesh: String,
    pub texture: String,
    pub unk: u16,
}

#[derive(Debug, Clone)]
pub struct UnitVariant {
    pub version: u32,
    pub header_size: u32,
    pub mesh_table_offset: u32,
    pub categories: Vec<Category>,
    pub meshes: Vec<MeshEntry>,
}

#[derive(Debug)]
pub struct VariantError(pub String);
impl fmt::Display for VariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn u32_at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}

/// Fixed-size UTF-16LE field, NUL-terminated/padded. Returns (text, bytes after the NUL are all zero).
pub fn fixed_utf16(b: &[u8]) -> (String, bool) {
    let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    let clean = units[end..].iter().all(|&c| c == 0);
    (String::from_utf16_lossy(&units[..end]), clean)
}

/// Parse a whole file. Strict: the size must be exactly header + categories + meshes.
pub fn parse(b: &[u8]) -> Result<UnitVariant, VariantError> {
    let err = |s: String| Err(VariantError(s));
    if b.len() < 20 || &b[0..4] != b"VRNT" {
        return err("bad magic".into());
    }
    let version = u32_at(b, 4);
    let ncat = u32_at(b, 8) as usize;
    let header_size = u32_at(b, 12);
    if header_size != 20 {
        return err(format!("header size {header_size} != 20"));
    }
    let mesh_off = u32_at(b, 16) as usize;
    if mesh_off != 20 + ncat * CATEGORY_SIZE {
        return err(format!("mesh offset {mesh_off} != 20+{ncat}*528"));
    }
    if b.len() < mesh_off || (b.len() - mesh_off) % MESH_SIZE != 0 {
        return err(format!("size {} not header+k*1026", b.len()));
    }
    let mut categories = Vec::with_capacity(ncat);
    for i in 0..ncat {
        let p = 20 + i * CATEGORY_SIZE;
        let (name, clean) = fixed_utf16(&b[p..p + 512]);
        if !clean {
            return err(format!("category {i} name has bytes after NUL"));
        }
        categories.push(Category {
            name,
            index: u32_at(b, p + 512),
            unk: u32_at(b, p + 516),
            mesh_count: u32_at(b, p + 520),
            first_mesh: u32_at(b, p + 524),
        });
    }
    let nmesh = (b.len() - mesh_off) / MESH_SIZE;
    let mut meshes = Vec::with_capacity(nmesh);
    for i in 0..nmesh {
        let p = mesh_off + i * MESH_SIZE;
        let (mesh, c1) = fixed_utf16(&b[p..p + 512]);
        let (texture, c2) = fixed_utf16(&b[p + 512..p + 1024]);
        if !c1 || !c2 {
            return err(format!("mesh {i} string has bytes after NUL"));
        }
        meshes.push(MeshEntry { mesh, texture, unk: u16::from_le_bytes([b[p + 1024], b[p + 1025]]) });
    }
    Ok(UnitVariant { version, header_size, mesh_table_offset: mesh_off as u32, categories, meshes })
}

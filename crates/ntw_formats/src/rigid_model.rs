//! `.rigid_model` static meshes (buildings, props, campaign pieces) and their
//! `.rigid_model_header` side files.
//!
//! Full field-by-field spec, with the Napoleon.exe functions it was checked
//! against: `analysis/worker1/GRAPHICS_EXE.md`. Short version:
//!
//! # Layout (all little-endian)
//! ```text
//! u32 mesh_count                                         CONFIRMED (exe 0x011D9D80)
//! mesh_count x MESH
//! f32 x3 bbox_min, f32 x3 bbox_max                       CONFIRMED (read after the meshes)
//! EOF                                                    CONFIRMED (every shipped file)
//!
//! MESH:
//!   [u32 0x12345678, u32 version]   optional; absent => version 0       CONFIRMED (0x01104810)
//!   MATERIAL (depends on version)                                       CONFIRMED (0x012234A0)
//!   u32 vertex_count, vertex_count x VERTEX (56/72/80 bytes by version)  CONFIRMED (0x011B2AB0)
//!   u32 index_count,  index_count  x u32 (triangle list)                CONFIRMED
//! ```
//! Each **mesh** is one draw call with one material. Each **file** is one level
//! of detail: LODs are separate files (`..._lod01.rigid_model`, `_lod03`, ...)
//! chosen by the `warscape_rigid_lod` DB tables (see GRAPHICS_EXE.md).
//!
//! # Coordinates
//! Positions are Direct3D style: left-handed, Y up, units in metres (INFERRED from
//! building sizes). To show them in a right-handed engine such as Bevy, negate Z of
//! positions, normals, tangents and binormals, and keep the index order (mirroring
//! one axis turns D3D's clockwise front faces into counter-clockwise ones).

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// The optional per-mesh marker that announces an explicit version number.
pub const MESH_MAGIC: u32 = 0x1234_5678;
/// First u32 of a `.rigid_model_header` file.
pub const HEADER_MAGIC: u32 = 0x000F_EABC;

/// One texture slot of a version-2+ material: a flag byte and a name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextureRef {
    /// 0 in almost every file: `name` is relative to the model's texture folder.
    /// 1: the slot holds a full path to a placeholder such as
    /// `RigidModels\DummyTextures\dummy_normal` (INFERRED meaning; see the spec).
    pub flag: u8,
    /// The texture's name **without** `.dds`, e.g. `eu_diffuse0`.
    pub name: String,
}

/// The material of one mesh.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Material {
    /// Versions 0 and 1 store a single base name; the game appends
    /// `_diffuse`, `_normal`, `_gloss_map`, `_dirty_map` (CONFIRMED, 0x012234A0).
    pub base_name: Option<String>,
    /// Versions 2+: diffuse (colour) texture.
    pub diffuse: Option<TextureRef>,
    /// Versions 2+: normal map.
    pub normal: Option<TextureRef>,
    /// Versions 2+: gloss (specular) map.
    pub gloss: Option<TextureRef>,
    /// Versions 3+: a fourth texture name (no flag byte). Empty in most files.
    /// INFERRED to be the dirt map (it is stored where version 0/1 builds `_dirty_map`).
    pub extra: Option<String>,
    /// Versions 4+: named float shader constants (`specpower`, `bumpfactor`, ...).
    pub float_params: Vec<(String, f32)>,
    /// Versions 4+: named 4-float shader constants (`rimcolor`, `specfactor`).
    pub vec4_params: Vec<(String, [f32; 4])>,
}

impl Material {
    /// The diffuse texture name (without `.dds`), whatever the version.
    pub fn diffuse_name(&self) -> Option<String> {
        self.slot_name(|m| m.diffuse.as_ref(), "_diffuse")
    }

    /// The normal map name (without `.dds`), whatever the version.
    pub fn normal_name(&self) -> Option<String> {
        self.slot_name(|m| m.normal.as_ref(), "_normal")
    }

    /// The gloss map name (without `.dds`), whatever the version.
    pub fn gloss_name(&self) -> Option<String> {
        self.slot_name(|m| m.gloss.as_ref(), "_gloss_map")
    }

    fn slot_name(&self, slot: impl Fn(&Self) -> Option<&TextureRef>, suffix: &str) -> Option<String> {
        if let Some(base) = &self.base_name {
            return (!base.is_empty()).then(|| format!("{base}{suffix}"));
        }
        slot(self).filter(|t| !t.name.is_empty()).map(|t| t.name.clone())
    }
}

/// One vertex as stored in the file (the game repacks it to 44 bytes on load).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vertex {
    /// Position (D3D left-handed).
    pub position: [f32; 3],
    /// Unit normal.
    pub normal: [f32; 3],
    /// Texture coordinate. Values outside 0..1 tile (wrap addressing). V points down, as in D3D.
    pub uv: [f32; 2],
    /// Tangent (unit).
    pub tangent: [f32; 3],
    /// Binormal / bitangent (unit).
    pub binormal: [f32; 3],
    /// Versions 1+: four floats the game clamps to 0..1 and packs into a D3DCOLOR.
    /// INFERRED to be a vertex colour / blend weights. Version 0 gets `[1, 1, 0, 1]` (CONFIRMED default).
    pub color: [f32; 4],
    /// Versions 3+: a second texture coordinate (INFERRED; zero in all files inspected by hand).
    pub uv2: [f32; 2],
}

/// Bytes per stored vertex for a mesh version: 56 (v0), 72 (v1, v2), 80 (v3+).
pub fn vertex_size(version: u32) -> usize {
    56 + if version >= 1 { 16 } else { 0 } + if version >= 3 { 8 } else { 0 }
}

/// One draw call: a material plus a triangle list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    /// Format version of this mesh (0..=5 in shipped files).
    pub version: u32,
    /// True if the mesh started with the `0x12345678` marker.
    pub has_magic: bool,
    /// Textures and shader constants.
    pub material: Material,
    /// The vertices.
    pub vertices: Vec<Vertex>,
    /// Triangle list indices (3 per triangle). Stored as u32 in the file;
    /// the game keeps only the low 16 bits (CONFIRMED), so every index is < 65536.
    pub indices: Vec<u32>,
}

/// A parsed `.rigid_model` file (one level of detail of one model).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RigidModel {
    /// The meshes, in file order.
    pub meshes: Vec<Mesh>,
    /// Axis-aligned bounding box, minimum corner.
    pub bbox_min: [f32; 3],
    /// Axis-aligned bounding box, maximum corner.
    pub bbox_max: [f32; 3],
    /// False only for old marker-less test files that have no stored box
    /// (the box was then computed from the vertices).
    pub bbox_in_file: bool,
}

/// Why a `.rigid_model` could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RigidModelError {
    /// The data ended early.
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
    /// A mesh version outside 0..=5.
    UnsupportedVersion {
        /// Mesh index.
        mesh: usize,
        /// The version found.
        version: u32,
    },
    /// An index points past the vertex list.
    IndexOutOfRange {
        /// Mesh index.
        mesh: usize,
        /// The bad index.
        index: u32,
    },
    /// Bytes remain after the bounding box.
    TrailingBytes {
        /// Where they start.
        offset: usize,
        /// How many.
        count: usize,
    },
    /// A `.rigid_model_header` did not start with `0x000FEABC`, version 1.
    BadHeader,
}

impl From<ReadError> for RigidModelError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for RigidModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => write!(f, "unexpected end of data at {offset} (needed {needed})"),
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 string at {offset}"),
            Self::UnsupportedVersion { mesh, version } => write!(f, "mesh {mesh}: unsupported version {version}"),
            Self::IndexOutOfRange { mesh, index } => write!(f, "mesh {mesh}: index {index} out of range"),
            Self::TrailingBytes { offset, count } => write!(f, "{count} trailing bytes at {offset}"),
            Self::BadHeader => write!(f, "not a rigid_model_header (magic 0x000FEABC, version 1)"),
        }
    }
}

impl std::error::Error for RigidModelError {}

/// Fails before allocating if `count` items of `size` bytes cannot fit in the rest of the data.
fn check_room(c: &Cursor<'_>, count: u32, size: usize) -> Result<usize, RigidModelError> {
    let count = count as usize;
    let needed = count.saturating_mul(size);
    if needed > c.remaining() {
        return Err(RigidModelError::UnexpectedEof { offset: c.pos(), needed });
    }
    Ok(count)
}

fn vec3(c: &mut Cursor<'_>) -> Result<[f32; 3], ReadError> {
    Ok([c.f32()?, c.f32()?, c.f32()?])
}

fn texture_ref(c: &mut Cursor<'_>) -> Result<TextureRef, ReadError> {
    let flag = c.u8()?;
    let name = c.utf16()?;
    Ok(TextureRef { flag, name })
}

fn material(c: &mut Cursor<'_>, version: u32) -> Result<Material, RigidModelError> {
    let mut m = Material::default();
    if version < 2 {
        m.base_name = Some(c.utf16()?);
        return Ok(m);
    }
    m.diffuse = Some(texture_ref(c)?);
    m.normal = Some(texture_ref(c)?);
    m.gloss = Some(texture_ref(c)?);
    if version >= 3 {
        m.extra = Some(c.utf16()?);
    }
    if version >= 4 {
        let n = c.u32()?;
        check_room(c, n, 6)?;
        for _ in 0..n {
            let name = c.utf16()?;
            m.float_params.push((name, c.f32()?));
        }
        let n = c.u32()?;
        check_room(c, n, 18)?;
        for _ in 0..n {
            let name = c.utf16()?;
            m.vec4_params.push((name, [c.f32()?, c.f32()?, c.f32()?, c.f32()?]));
        }
    }
    Ok(m)
}

fn vertex(c: &mut Cursor<'_>, version: u32) -> Result<Vertex, ReadError> {
    let mut v = Vertex {
        position: vec3(c)?,
        normal: vec3(c)?,
        uv: [c.f32()?, c.f32()?],
        tangent: vec3(c)?,
        binormal: vec3(c)?,
        color: [1.0, 1.0, 0.0, 1.0],
        uv2: [0.0, 0.0],
    };
    if version >= 1 {
        v.color = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
    }
    if version >= 3 {
        v.uv2 = [c.f32()?, c.f32()?];
    }
    Ok(v)
}

fn mesh(c: &mut Cursor<'_>, index: usize) -> Result<Mesh, RigidModelError> {
    let mut m = Mesh::default();
    if c.peek(4) == Some(&MESH_MAGIC.to_le_bytes()[..]) {
        c.u32()?;
        m.has_magic = true;
        m.version = c.u32()?;
    }
    if m.version > 5 {
        return Err(RigidModelError::UnsupportedVersion { mesh: index, version: m.version });
    }
    m.material = material(c, m.version)?;
    let n = c.u32()?;
    let n = check_room(c, n, vertex_size(m.version))?;
    m.vertices.reserve_exact(n);
    for _ in 0..n {
        m.vertices.push(vertex(c, m.version)?);
    }
    let n = c.u32()?;
    let n = check_room(c, n, 4)?;
    m.indices.reserve_exact(n);
    for _ in 0..n {
        let i = c.u32()?;
        if i as usize >= m.vertices.len() {
            return Err(RigidModelError::IndexOutOfRange { mesh: index, index: i });
        }
        m.indices.push(i);
    }
    Ok(m)
}

impl RigidModel {
    /// Parses a whole `.rigid_model` file. The bounding box must end exactly at the end of the data.
    pub fn read(bytes: &[u8]) -> Result<Self, RigidModelError> {
        let mut c = Cursor::new(bytes);
        let count = c.u32()?;
        // The smallest possible mesh (version 0, empty name, no vertices) is 10 bytes.
        let count = check_room(&c, count, 10)?;
        let mut model = RigidModel::default();
        for i in 0..count {
            model.meshes.push(mesh(&mut c, i)?);
        }
        if c.remaining() == 0 && model.meshes.iter().all(|m| !m.has_magic) {
            // The 5 old-style test files in data.pack (testdata\..., no 0x12345678 marker)
            // end right after the last mesh. Compute the box instead (CONFIRMED by bytes;
            // the shipped game never loads these files as far as we know).
            model.bbox_in_file = false;
            model.recompute_bbox();
            return Ok(model);
        }
        model.bbox_in_file = true;
        model.bbox_min = vec3(&mut c)?;
        model.bbox_max = vec3(&mut c)?;
        if c.remaining() != 0 {
            return Err(RigidModelError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(model)
    }

    /// Sets the bounding box from the vertex positions (all zero if there are none).
    pub fn recompute_bbox(&mut self) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for v in self.meshes.iter().flat_map(|m| &m.vertices) {
            for k in 0..3 {
                lo[k] = lo[k].min(v.position[k]);
                hi[k] = hi[k].max(v.position[k]);
            }
        }
        if lo[0] > hi[0] {
            (lo, hi) = ([0.0; 3], [0.0; 3]);
        }
        (self.bbox_min, self.bbox_max) = (lo, hi);
    }

    /// Total vertices over all meshes.
    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|m| m.vertices.len()).sum()
    }

    /// Total indices over all meshes.
    pub fn index_count(&self) -> usize {
        self.meshes.iter().map(|m| m.indices.len()).sum()
    }
}

/// A `.rigid_model_header`: a small summary file the game reads before the model
/// (CONFIRMED layout, exe 0x011DA5A0).
#[derive(Debug, Clone, PartialEq)]
pub struct RigidModelHeader {
    /// True for `*_anim.rigid_model_header`, which describe a `.rigid_model_animation`.
    pub animated: bool,
    /// Total vertex count of the model (CONFIRMED against the model files by the real-install test).
    pub vertex_count: u32,
    /// Total index count of the model (same).
    pub index_count: u32,
    /// Bounding box minimum.
    pub bbox_min: [f32; 3],
    /// Bounding box maximum.
    pub bbox_max: [f32; 3],
    /// Only when `animated`: one more u32 (meaning UNKNOWN; maybe a frame count).
    pub anim_value: Option<u32>,
}

impl RigidModelHeader {
    /// Parses a `.rigid_model_header`.
    pub fn read(bytes: &[u8]) -> Result<Self, RigidModelError> {
        let mut c = Cursor::new(bytes);
        if c.u32()? != HEADER_MAGIC || c.u32()? != 1 {
            return Err(RigidModelError::BadHeader);
        }
        let animated = c.u8()? != 0;
        let vertex_count = c.u32()?;
        let index_count = c.u32()?;
        let bbox_min = vec3(&mut c)?;
        let bbox_max = vec3(&mut c)?;
        let anim_value = if animated { Some(c.u32()?) } else { None };
        if c.remaining() != 0 {
            return Err(RigidModelError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { animated, vertex_count, index_count, bbox_min, bbox_max, anim_value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::utf16_bytes;

    fn f(b: &mut Vec<u8>, v: &[f32]) {
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }

    fn tex(b: &mut Vec<u8>, flag: u8, s: &str) {
        b.push(flag);
        b.extend(utf16_bytes(s));
    }

    /// A one-quad version-5 model shaped like `eu_city_1_slot_fortified.rigid_model`.
    fn v5_quad() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&MESH_MAGIC.to_le_bytes());
        b.extend_from_slice(&5u32.to_le_bytes());
        tex(&mut b, 0, "eu_diffuse0");
        tex(&mut b, 0, "eu_normal0");
        tex(&mut b, 0, "eu_specular_map0");
        b.extend(utf16_bytes(""));
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend(utf16_bytes("specpower"));
        f(&mut b, &[2.0]);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        for i in 0..4 {
            f(&mut b, &[i as f32, 0.0, 1.0, 0.0, 1.0, 0.0, 0.5, 0.25, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0]);
            f(&mut b, &[0.0; 6]);
        }
        b.extend_from_slice(&6u32.to_le_bytes());
        for i in [0u32, 1, 2, 2, 3, 0] {
            b.extend_from_slice(&i.to_le_bytes());
        }
        f(&mut b, &[0.0, 0.0, 1.0, 3.0, 0.0, 1.0]);
        b
    }

    #[test]
    fn parses_v5_quad() {
        let m = RigidModel::read(&v5_quad()).unwrap();
        assert_eq!(m.meshes.len(), 1);
        let mesh = &m.meshes[0];
        assert_eq!(mesh.version, 5);
        assert!(mesh.has_magic);
        assert_eq!(mesh.material.diffuse_name().as_deref(), Some("eu_diffuse0"));
        assert_eq!(mesh.material.gloss_name().as_deref(), Some("eu_specular_map0"));
        assert_eq!(mesh.material.float_params, [("specpower".to_owned(), 2.0)]);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.vertices[3].position, [3.0, 0.0, 1.0]);
        assert_eq!(mesh.vertices[0].uv, [0.5, 0.25]);
        assert_eq!(mesh.indices, [0, 1, 2, 2, 3, 0]);
        assert_eq!(m.bbox_max, [3.0, 0.0, 1.0]);
    }

    #[test]
    fn version0_without_magic_uses_base_name() {
        let mut b = Vec::new();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend(utf16_bytes("legtest"));
        b.extend_from_slice(&1u32.to_le_bytes());
        f(&mut b, &[1.0; 14]);
        b.extend_from_slice(&0u32.to_le_bytes());
        f(&mut b, &[0.0; 6]);
        let m = RigidModel::read(&b).unwrap();
        assert_eq!(m.meshes[0].version, 0);
        assert_eq!(m.meshes[0].material.normal_name().as_deref(), Some("legtest_normal"));
        assert_eq!(m.meshes[0].vertices[0].color, [1.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn errors_instead_of_panicking() {
        let b = v5_quad();
        for cut in [0, 3, 10, 40, b.len() - 1] {
            assert!(RigidModel::read(&b[..cut]).is_err());
        }
        let mut long = b.clone();
        long.push(0);
        assert!(matches!(RigidModel::read(&long), Err(RigidModelError::TrailingBytes { .. })));
        let mut huge = b;
        huge[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(RigidModel::read(&huge).is_err());
    }

    #[test]
    fn parses_header() {
        let mut b = Vec::new();
        b.extend_from_slice(&HEADER_MAGIC.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.push(1);
        b.extend_from_slice(&10u32.to_le_bytes());
        b.extend_from_slice(&30u32.to_le_bytes());
        f(&mut b, &[-1.0, -2.0, -3.0, 1.0, 2.0, 3.0]);
        b.extend_from_slice(&63u32.to_le_bytes());
        let h = RigidModelHeader::read(&b).unwrap();
        assert!(h.animated);
        assert_eq!((h.vertex_count, h.index_count, h.anim_value), (10, 30, Some(63)));
    }
}

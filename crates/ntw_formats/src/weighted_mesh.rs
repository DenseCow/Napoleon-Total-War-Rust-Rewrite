//! `.variant_weighted_mesh`: skinned whole-body meshes (horses, camels, campaign agents).
//!
//! Horses are not assembled from `.unit_variant` part lists like soldiers. Each horse
//! model is one file `unitmodels\horse\horse_<letter>_<type>_lod<N>.variant_weighted_mesh`
//! (data.pack) holding several named pieces (head, mane, body, saddle, stirrups ...).
//! See `analysis/units/CAVALRY.md`.
//!
//! Layout (all little-endian), reverse-engineered from the shipped files; the reader is
//! strict (every field to EOF) and `tests/real_install.rs::every_weighted_mesh_parses`
//! checks it on every file with the magic:
//! ```text
//! u32  magic          0x12345678
//! u32  version        1
//! u32  scalar_count   { u16 n; [u16; n] name; f32 value }           (light_scale, bumpfactor, ...)
//! u32  vector_count   { u16 n; [u16; n] name; f32 x4 }              (colourmapfactor, specfactor)
//! u32  piece_count    { u16 n; [u16; n] name; u32 V; u32 I }        (table of contents)
//! piece_count x {
//!     u32 V                                   (== the table's V)
//!     V x VERTEX
//!     u32 I                                   (== the table's I)
//!     I x u32 index                           (triangle list)
//! }
//! u32  attachment count   (0 for mounts; 1 for Napoleon's outfit, 134 in euro_equipment),
//!                         then the attachments (see [`WeightedAttachment`])
//! Older headerless layouts (campaign agents, testdata): see `read_headerless`. All 286 files
//! in the packs read to the end (`tests/real_install.rs::all_286_weighted_meshes_parse`).
//! VERTEX:
//!     f32 x2 uv
//!     f32 x3 normal (model space)             INFERRED
//!     f32 x3 tangent (model space)            INFERRED
//!     u32 influence_count (1..)
//!     influence_count x { u32 bone; f32 x3 position in the bone's frame;
//!                         f32 x3 normal in the bone's frame; f32 weight }
//!     f32 x4 UNKNOWN (0.0 in the files checked)
//! ```
//! Like the soldier part meshes, each influence carries its own bone-local position, so a
//! posed vertex is `sum_k w_k * (M_k * p_k)` with the model-space bone matrices of any
//! animation frame and no bind pose. Bone indices refer to the clip skeleton of the model's
//! animation table (e.g. `horse_stand.anim` for horses).

use std::fmt;

use crate::anim::{transform_point, transform_vector};
use crate::bytes::{Cursor, ReadError};

/// File magic (`78 56 34 12`).
pub const WEIGHTED_MESH_MAGIC: u32 = 0x1234_5678;

/// A parsed `.variant_weighted_mesh`.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedMesh {
    pub version: u32,
    pub scalars: Vec<(String, f32)>,
    pub vectors: Vec<(String, [f32; 4])>,
    pub pieces: Vec<WeightedPiece>,
    /// Rigid equipment meshes carried by the model (empty for every mount).
    pub attachments: Vec<WeightedAttachment>,
}

/// A rigid equipment mesh stored after a weighted mesh's pieces (`napoleon_battleoutfit_*`: his
/// sword; `euro_equipment`: 134 muskets, swords, ...). Layout (CONFIRMED by reading the files to
/// the end): magic, u32 version (5), utf16 name (`rigid_equip_euro_cutlass01`), u32 (UNKNOWN: 1
/// or 2), 3 x {u8, utf16 texture} (diffuse, normal, gloss map), 2 bytes, then the same body as the
/// main mesh's materials (scalars, vectors), then a rigid mesh: u32 V, V x 20 f32, u32 I, I x u32.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedAttachment {
    pub version: u32,
    pub name: String,
    /// UNKNOWN u32 after the name.
    pub unknown: u32,
    /// {u8 UNKNOWN, texture path} x 3.
    pub textures: Vec<(u8, String)>,
    /// Two UNKNOWN bytes after the textures (0, 0 in every file).
    pub flags: [u8; 2],
    pub scalars: Vec<(String, f32)>,
    pub vectors: Vec<(String, [f32; 4])>,
    /// Floats per rigid vertex: 20 (INFERRED: position 3, normal 3, uv 2, tangent 3, binormal 3,
    /// colour 4, uv 2), or 14 in the older testdata attachments (no colour and second uv).
    pub vertex_size: usize,
    /// The rigid vertices, `vertex_size` floats each, back to back.
    pub vertices: Vec<f32>,
    /// Triangle list.
    pub indices: Vec<u32>,
}

/// One named piece (e.g. `emp_horse_A_body01`, `BASIC_SADDLE`).
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedPiece {
    pub name: String,
    pub vertices: Vec<WeightedVertex>,
    pub indices: Vec<u32>,
}

/// One skinned vertex.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedVertex {
    pub uv: [f32; 2],
    /// INFERRED: model-space normal and tangent of the authoring pose.
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub influences: Vec<Influence>,
    /// UNKNOWN trailing four floats.
    pub extra: [f32; 4],
}

/// One bone influence of a vertex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Influence {
    pub bone: u32,
    /// Position in the bone's frame.
    pub position: [f32; 3],
    /// Normal in the bone's frame.
    pub normal: [f32; 3],
    pub weight: f32,
}

/// Why a weighted mesh could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeightedMeshError {
    UnexpectedEof { offset: usize, needed: usize },
    InvalidUtf16 { offset: usize },
    BadMagic(u32),
    TooLarge { offset: usize },
    /// A piece body's count differs from the table of contents.
    CountMismatch { piece: usize, offset: usize },
    /// The final u32 is not 0 (UNKNOWN meaning).
    UnknownTrailer { offset: usize },
    TrailingBytes { offset: usize, count: usize },
}

impl From<ReadError> for WeightedMeshError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for WeightedMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => write!(f, "unexpected end of data at {offset} (needed {needed})"),
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 at {offset}"),
            Self::BadMagic(m) => write!(f, "bad magic {m:#x}"),
            Self::TooLarge { offset } => write!(f, "count too large at {offset}"),
            Self::CountMismatch { piece, offset } => write!(f, "piece {piece}: count differs from the table at {offset}"),
            Self::UnknownTrailer { offset } => write!(f, "non-zero trailer at {offset}"),
            Self::TrailingBytes { offset, count } => write!(f, "{count} trailing bytes at {offset}"),
        }
    }
}

impl std::error::Error for WeightedMeshError {}

fn f32s<const N: usize>(c: &mut Cursor<'_>) -> Result<[f32; N], ReadError> {
    let mut v = [0f32; N];
    for x in &mut v {
        *x = c.f32()?;
    }
    Ok(v)
}

fn count(c: &mut Cursor<'_>, min_size: usize) -> Result<usize, WeightedMeshError> {
    let offset = c.pos();
    let n = c.u32()? as usize;
    if n > c.remaining() / min_size.max(1) {
        return Err(WeightedMeshError::TooLarge { offset });
    }
    Ok(n)
}

impl WeightedMesh {
    /// Parses a complete file.
    pub fn read(bytes: &[u8]) -> Result<Self, WeightedMeshError> {
        let mut c = Cursor::new(bytes);
        let magic = c.u32()?;
        if magic != WEIGHTED_MESH_MAGIC {
            // The older, headerless layout (the campaign agents `campaign_*_lod3/4`,
            // `campaign_soldier_base`): no magic, version or materials; the file starts with the
            // piece table. CONFIRMED by reading those files to the end. Reported as version 0.
            return Self::read_headerless(bytes).map_err(|_| WeightedMeshError::BadMagic(magic));
        }
        let version = c.u32()?;
        let (scalars, vectors, pieces) = read_body(&mut c)?;
        // Attachments: u32 count (0 in every mount file), then count rigid equipment meshes
        // (`napoleon_battleoutfit_*`: 1, `euro_equipment`: 134). CONFIRMED by reading to EOF.
        let mut attachments = Vec::new();
        for _ in 0..count(&mut c, 8)? {
            attachments.push(read_attachment(&mut c)?);
        }
        if c.remaining() != 0 {
            return Err(WeightedMeshError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { version, scalars, vectors, pieces, attachments })
    }

    /// The headerless layouts (no magic, version or materials), tried in order; each must read
    /// to the end of the file (CONFIRMED on every such file):
    /// 1. piece table + current vertices + current attachments: the campaign agents
    ///    (`campaign_*_lod3/4`, `campaign_soldier_base`);
    /// 2. piece table + vertices without the 4 extra floats + old attachments
    ///    {utf16 name, u32 (3), u32 V, V x 14 f32, u32 I, I x u32}: `testdata\diplomat`, `euroline`;
    /// 3. a count header {u32 pieces, u32 total V, u32 total I} + unnamed pieces with
    ///    {uv, influences} vertices: `testdata\test`, `ranger_test_lod*`;
    /// 4. the same header + unnamed pieces with {uv, normal, tangent, influences} vertices:
    ///    `testdata\ranger\*`, `musketman`.
    fn read_headerless(bytes: &[u8]) -> Result<Self, WeightedMeshError> {
        fn finish(c: &Cursor<'_>, pieces: Vec<WeightedPiece>, attachments: Vec<WeightedAttachment>) -> Result<WeightedMesh, WeightedMeshError> {
            if c.remaining() != 0 {
                return Err(WeightedMeshError::TrailingBytes { offset: c.pos(), count: c.remaining() });
            }
            Ok(WeightedMesh { version: 0, scalars: Vec::new(), vectors: Vec::new(), pieces, attachments })
        }
        let table = |layout: VertexLayout, old: bool| -> Result<Self, WeightedMeshError> {
            let mut c = Cursor::new(bytes);
            let pieces = read_pieces_with(&mut c, None, layout)?;
            let mut attachments = Vec::new();
            for _ in 0..count(&mut c, 8)? {
                attachments.push(if old { read_old_attachment(&mut c)? } else { read_attachment(&mut c)? });
            }
            finish(&c, pieces, attachments)
        };
        let counted = |layout: VertexLayout| -> Result<Self, WeightedMeshError> {
            let mut c = Cursor::new(bytes);
            let n = c.u32()? as usize;
            let _total_vertices = c.u32()?;
            let _total_indices = c.u32()?;
            let pieces = read_pieces_with(&mut c, Some(n), layout)?;
            finish(&c, pieces, Vec::new())
        };
        table(VertexLayout::Full, false)
            .or_else(|_| table(VertexLayout::NoExtra, true))
            .or_else(|_| counted(VertexLayout::Short))
            .or_else(|_| counted(VertexLayout::NoExtra))
    }

    /// A piece by name (case-insensitive).
    pub fn piece(&self, name: &str) -> Option<&WeightedPiece> {
        self.pieces.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// A piece posed in model space (file coordinates: left-handed, Y up).
#[derive(Debug, Clone, Default)]
pub struct PosedPiece {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
}

impl WeightedPiece {
    /// Poses the piece with model-space bone matrices (`Anim::world_matrices`). Bones
    /// beyond the skeleton count as the identity.
    pub fn pose(&self, bones: &[[f32; 16]]) -> PosedPiece {
        const ID: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
        let mut out = PosedPiece::default();
        for v in &self.vertices {
            let (mut p, mut n) = ([0f32; 3], [0f32; 3]);
            let total: f32 = v.influences.iter().map(|i| i.weight).sum::<f32>().max(1e-6);
            for inf in &v.influences {
                let m = bones.get(inf.bone as usize).unwrap_or(&ID);
                let (pp, nn) = (transform_point(m, inf.position), transform_vector(m, inf.normal));
                let w = inf.weight / total;
                for k in 0..3 {
                    p[k] += pp[k] * w;
                    n[k] += nn[k] * w;
                }
            }
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            out.positions.push(p);
            out.normals.push(n.map(|x| x / len));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str, out: &mut Vec<u8>) {
        out.extend((s.len() as u16).to_le_bytes());
        for u in s.encode_utf16() {
            out.extend(u.to_le_bytes());
        }
    }

    /// A one-piece, one-triangle file built by hand.
    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(WEIGHTED_MESH_MAGIC.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        utf16("light_scale", &mut b);
        b.extend(1.0f32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        utf16("body", &mut b);
        b.extend(3u32.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        for k in 0..3 {
            for x in [0.5f32, 0.5, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0] {
                b.extend(x.to_le_bytes());
            }
            b.extend(1u32.to_le_bytes());
            b.extend(2u32.to_le_bytes());
            for x in [k as f32, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0] {
                b.extend(x.to_le_bytes());
            }
            for _ in 0..4 {
                b.extend(0f32.to_le_bytes());
            }
        }
        b.extend(3u32.to_le_bytes());
        for i in [0u32, 1, 2] {
            b.extend(i.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes());
        b
    }

    #[test]
    fn reads_and_poses_sample() {
        let m = WeightedMesh::read(&sample()).unwrap();
        assert_eq!(m.scalars, vec![("light_scale".into(), 1.0)]);
        let p = m.piece("BODY").unwrap();
        assert_eq!(p.vertices.len(), 3);
        assert_eq!(p.vertices[2].influences[0].bone, 2);
        // Bone 2 translated by (0, 1, 0): vertex 2 (local x = 2) lands at (2, 1, 0).
        let id = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
        let mut t = id;
        t[13] = 1.0;
        let posed = p.pose(&[id, id, t]);
        assert_eq!(posed.positions[2], [2.0, 1.0, 0.0]);
    }

    #[test]
    fn rejects_trailing_and_bad_magic() {
        let mut b = sample();
        b.push(0);
        assert!(matches!(WeightedMesh::read(&b), Err(WeightedMeshError::TrailingBytes { .. })));
        b[0] = 0;
        assert!(matches!(WeightedMesh::read(&b), Err(WeightedMeshError::BadMagic(_))));
    }
}

type Body = (Vec<(String, f32)>, Vec<(String, [f32; 4])>, Vec<WeightedPiece>);

/// Material scalars, vectors, the piece table and the pieces.
fn read_body(c: &mut Cursor<'_>) -> Result<Body, WeightedMeshError> {
    let (scalars, vectors) = read_materials(c)?;
    let pieces = read_pieces(c)?;
    Ok((scalars, vectors, pieces))
}

/// The piece table and the pieces.
fn read_pieces(c: &mut Cursor<'_>) -> Result<Vec<WeightedPiece>, WeightedMeshError> {
    read_pieces_with(c, None, VertexLayout::Full)
}

/// Vertex layouts of the different file generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VertexLayout {
    /// uv, normal, tangent, influences, 4 extra floats (current files).
    Full,
    /// uv, normal, tangent, influences (older testdata).
    NoExtra,
    /// uv, influences (oldest testdata).
    Short,
}

/// The pieces: after a piece table when `unnamed` is `None`, else that many unnamed pieces, each
/// with its own counts (the vertex count then the index count, as in the table layout).
fn read_pieces_with(c: &mut Cursor<'_>, unnamed: Option<usize>, layout: VertexLayout) -> Result<Vec<WeightedPiece>, WeightedMeshError> {
    let toc: Vec<(String, Option<(usize, usize)>)> = match unnamed {
        None => {
            let mut toc = Vec::new();
            for _ in 0..count(c, 10)? {
                let name = c.utf16()?;
                toc.push((name, Some((c.u32()? as usize, c.u32()? as usize))));
            }
            toc
        }
        Some(n) => {
            if n > c.remaining() / 8 {
                return Err(WeightedMeshError::TooLarge { offset: 0 });
            }
            (0..n).map(|_| (String::new(), None)).collect()
        }
    };
    let mut pieces = Vec::with_capacity(toc.len());
    for (pi, (name, counts)) in toc.into_iter().enumerate() {
        let offset = c.pos();
        let v = count(c, 40)?;
        if counts.is_some_and(|(tv, _)| tv != v) {
            return Err(WeightedMeshError::CountMismatch { piece: pi, offset });
        }
        let mut vertices = Vec::with_capacity(v);
        for _ in 0..v {
            let uv = f32s::<2>(c)?;
            let (normal, tangent) = if layout == VertexLayout::Short { ([0.0; 3], [0.0; 3]) } else { (f32s::<3>(c)?, f32s::<3>(c)?) };
            let n = count(c, 32)?;
            let mut influences = Vec::with_capacity(n);
            for _ in 0..n {
                let bone = c.u32()?;
                let position = f32s::<3>(c)?;
                let normal = f32s::<3>(c)?;
                let weight = c.f32()?;
                influences.push(Influence { bone, position, normal, weight });
            }
            let extra = if layout == VertexLayout::Full { f32s::<4>(c)? } else { [0.0; 4] };
            vertices.push(WeightedVertex { uv, normal, tangent, influences, extra });
        }
        let offset = c.pos();
        let i = count(c, 4)?;
        if counts.is_some_and(|(_, ti)| ti != i) {
            return Err(WeightedMeshError::CountMismatch { piece: pi, offset });
        }
        let mut indices = Vec::with_capacity(i);
        for _ in 0..i {
            indices.push(c.u32()?);
        }
        pieces.push(WeightedPiece { name, vertices, indices });
    }
    Ok(pieces)
}

/// An old (testdata) attachment: utf16 name, u32 (3), u32 V, V x 14 f32, u32 I, I x u32.
fn read_old_attachment(c: &mut Cursor<'_>) -> Result<WeightedAttachment, WeightedMeshError> {
    let name = c.utf16()?;
    let unknown = c.u32()?;
    let mut vertices = Vec::new();
    for _ in 0..count(c, 56)? {
        vertices.extend_from_slice(&f32s::<14>(c)?);
    }
    let mut indices = Vec::new();
    for _ in 0..count(c, 4)? {
        indices.push(c.u32()?);
    }
    Ok(WeightedAttachment {
        version: 0,
        name,
        unknown,
        textures: Vec::new(),
        flags: [0, 0],
        scalars: Vec::new(),
        vectors: Vec::new(),
        vertex_size: 14,
        vertices,
        indices,
    })
}

/// One attachment (see [`WeightedAttachment`]).
fn read_attachment(c: &mut Cursor<'_>) -> Result<WeightedAttachment, WeightedMeshError> {
    let magic = c.u32()?;
    if magic != WEIGHTED_MESH_MAGIC {
        return Err(WeightedMeshError::BadMagic(magic));
    }
    let version = c.u32()?;
    let name = c.utf16()?;
    let unknown = c.u32()?;
    let mut textures = Vec::new();
    for _ in 0..3 {
        let flag = c.u8()?;
        textures.push((flag, c.utf16()?));
    }
    let flags = [c.u8()?, c.u8()?];
    let (scalars, vectors) = read_materials(c)?;
    let mut vertices = Vec::new();
    for _ in 0..count(c, 80)? {
        vertices.extend_from_slice(&f32s::<20>(c)?);
    }
    let mut indices = Vec::new();
    for _ in 0..count(c, 4)? {
        indices.push(c.u32()?);
    }
    Ok(WeightedAttachment { version, name, unknown, textures, flags, scalars, vectors, vertex_size: 20, vertices, indices })
}

/// Named material scalars and named material vectors.
type Materials = (Vec<(String, f32)>, Vec<(String, [f32; 4])>);

/// Material scalars and vectors.
fn read_materials(c: &mut Cursor<'_>) -> Result<Materials, WeightedMeshError> {
    let mut scalars = Vec::new();
    for _ in 0..count(c, 6)? {
        let name = c.utf16()?;
        scalars.push((name, c.f32()?));
    }
    let mut vectors = Vec::new();
    for _ in 0..count(c, 18)? {
        let name = c.utf16()?;
        vectors.push((name, f32s::<4>(c)?));
    }
    Ok((scalars, vectors))
}

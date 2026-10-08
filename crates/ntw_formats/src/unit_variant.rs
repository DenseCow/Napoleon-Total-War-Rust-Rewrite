//! Unit-variant files used to assemble soldiers, officers, musicians, and standard bearers.
//!
//! The formats in this module were surveyed, without modifying the game install, in
//! `analysis/worker2/UNIT_VARIANT_AND_TEXTURES.md`:
//! - `.unit_variant` (`VRNT`) is **CONFIRMED** on all 3,126 shipped files: a fixed-size
//!   category table followed by fixed-size mesh references.
//! - `.variant_part_mesh` (`VMPF`) headers and the LOD walk below are **CONFIRMED** for
//!   the 297 ordinary part files. Vertex records are kept raw and decoded on demand by
//!   [`VariantPartMeshLod::decode_vertices`] (field map INFERRED, see [`VariantVertex`]).
//!   The first LOD block is the most detailed in 287 of 297 files (CONFIRMED survey).
//! - Attachment records: `[u16; 16]` name, 4x4 f32, u32 bone (CONFIRMED size, all files).
//! - VMPF format 2 is an equipment-piece container: named rigid pieces bound to bones
//!   (CONFIRMED walk to EOF on both shipped containers, see [`EquipmentPiece`]).
//!
//! The older preliminary notes in `analysis/worker1/GRAPHICS_EXE.md` §7 are superseded
//! for the table offsets and VMPF LOD layout by the full-file survey cited above.

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// Magic at the beginning of a `.unit_variant` file.
pub const UNIT_VARIANT_MAGIC: [u8; 4] = *b"VRNT";
/// Magic at the beginning of a `.variant_part_mesh` file.
pub const VARIANT_PART_MESH_MAGIC: [u8; 4] = *b"VMPF";

/// Number of bytes in one `VRNT` category record.
pub const UNIT_VARIANT_CATEGORY_SIZE: usize = 528;
/// Number of bytes in one `VRNT` mesh-reference record.
pub const UNIT_VARIANT_MESH_SIZE: usize = 1026;

/// A `.unit_variant` (`VRNT`) file.
///
/// The game chooses one entry from each category for an individual soldier.  How it
/// makes that choice is **UNKNOWN**; this type only represents the authored choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitVariant {
    /// The stored version word. It is zero in every shipped file.
    pub version: u32,
    /// Offset of the category table. This is 20 in every shipped file.
    pub category_table_offset: u32,
    /// Offset of the mesh-reference table.
    pub mesh_table_offset: u32,
    /// Categories in file order. `index` is retained as stored rather than used as an
    /// array index: 14 shipped records differ from their position and its meaning is
    /// still **UNKNOWN**.
    pub categories: Vec<UnitVariantCategory>,
    /// All mesh references, in file order.
    pub mesh_references: Vec<UnitVariantMeshRef>,
}

impl UnitVariant {
    /// Parses a complete `.unit_variant` file.
    ///
    /// The category ranges are checked because the shipped files cover the mesh table
    /// contiguously and in order (a full-file **CONFIRMED** invariant).
    pub fn read(bytes: &[u8]) -> Result<Self, UnitVariantError> {
        let mut c = Cursor::new(bytes);
        let magic = take_magic(&mut c)?;
        if magic != UNIT_VARIANT_MAGIC {
            return Err(UnitVariantError::BadMagic {
                expected: UNIT_VARIANT_MAGIC,
                found: magic,
            });
        }
        let version = c.u32()?;
        let category_count = c.u32()?;
        let category_table_offset = c.u32()?;
        if category_table_offset != 20 {
            return Err(UnitVariantError::InvalidCategoryTableOffset {
                found: category_table_offset,
            });
        }
        let mesh_table_offset = c.u32()?;
        let expected_mesh_table_offset = 20usize
            .checked_add(
                (category_count as usize)
                    .checked_mul(UNIT_VARIANT_CATEGORY_SIZE)
                    .ok_or(UnitVariantError::CountOverflow)?,
            )
            .ok_or(UnitVariantError::CountOverflow)?;
        if mesh_table_offset as usize != expected_mesh_table_offset {
            return Err(UnitVariantError::InvalidMeshTableOffset {
                found: mesh_table_offset,
                expected: expected_mesh_table_offset,
            });
        }
        if expected_mesh_table_offset > bytes.len() {
            return Err(UnitVariantError::UnexpectedEof {
                offset: c.pos(),
                needed: expected_mesh_table_offset - c.pos(),
            });
        }
        let mesh_bytes = bytes.len() - expected_mesh_table_offset;
        if !mesh_bytes.is_multiple_of(UNIT_VARIANT_MESH_SIZE) {
            return Err(UnitVariantError::MisalignedMeshTable {
                byte_count: mesh_bytes,
            });
        }

        let mut categories = Vec::new();
        for category in 0..category_count as usize {
            let name = fixed_utf16(&mut c, 256, "category name", category)?;
            categories.push(UnitVariantCategory {
                name,
                index: c.u32()?,
                unknown: c.u32()?,
                mesh_count: c.u32()?,
                first_mesh: c.u32()?,
            });
        }

        let mesh_count = mesh_bytes / UNIT_VARIANT_MESH_SIZE;
        let mut mesh_references = Vec::new();
        for mesh in 0..mesh_count {
            mesh_references.push(UnitVariantMeshRef {
                mesh: fixed_utf16(&mut c, 256, "mesh path", mesh)?,
                texture_stem: fixed_utf16(&mut c, 256, "texture stem", mesh)?,
                kind: c.u16()?,
            });
        }

        debug_assert_eq!(c.remaining(), 0);
        validate_category_ranges(&categories, mesh_references.len())?;
        Ok(Self {
            version,
            category_table_offset,
            mesh_table_offset,
            categories,
            mesh_references,
        })
    }

    /// Mesh references belonging to `category`, if it exists.
    pub fn category_meshes(&self, category: usize) -> Option<&[UnitVariantMeshRef]> {
        let category = self.categories.get(category)?;
        let start = category.first_mesh as usize;
        let end = start.checked_add(category.mesh_count as usize)?;
        self.mesh_references.get(start..end)
    }
}

/// One fixed-size category in a [`UnitVariant`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitVariantCategory {
    /// Category name, e.g. `heads`, `hats`, or `equipment_primary_weapon`.
    pub name: String,
    /// Stored category identifier. It usually matches the category's file position;
    /// the meaning of exceptions is **UNKNOWN**.
    pub index: u32,
    /// A word that is zero in every shipped file; its meaning is **UNKNOWN**.
    pub unknown: u32,
    /// Number of entries for this category.
    pub mesh_count: u32,
    /// Index of the first entry in [`UnitVariant::mesh_references`].
    pub first_mesh: u32,
}

/// One mesh reference in a [`UnitVariant`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitVariantMeshRef {
    /// Path without `.variant_part_mesh` for kind 0, or a bare equipment-piece name
    /// for kind 1.
    pub mesh: String,
    /// Texture stem without a DDS suffix. It is empty for known kind-1 equipment
    /// references.
    pub texture_stem: String,
    /// Stored kind: 0 means a part-mesh path; 1 means an equipment-piece name. These
    /// meanings are **CONFIRMED** by cross-checking the shipped pack index.
    pub kind: u16,
}

/// Why a [`UnitVariant`] could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitVariantError {
    /// The data ended early.
    UnexpectedEof { offset: usize, needed: usize },
    /// The first four bytes were not `VRNT`.
    BadMagic { expected: [u8; 4], found: [u8; 4] },
    /// The category table did not start at byte 20.
    InvalidCategoryTableOffset { found: u32 },
    /// The mesh table did not follow the fixed-size category table.
    InvalidMeshTableOffset { found: u32, expected: usize },
    /// A count overflowed a byte-offset calculation.
    CountOverflow,
    /// The remaining mesh table bytes were not a whole number of records.
    MisalignedMeshTable { byte_count: usize },
    /// A fixed-size UTF-16 string has invalid Unicode.
    InvalidUtf16 {
        field: &'static str,
        record: usize,
        offset: usize,
    },
    /// Bytes after the first UTF-16 NUL in a fixed-size field were not zero.
    NonZeroStringPadding {
        field: &'static str,
        record: usize,
        offset: usize,
    },
    /// A category points outside the mesh-reference table.
    InvalidCategoryRange {
        category: usize,
        first_mesh: u32,
        mesh_count: u32,
        total_meshes: usize,
    },
    /// Categories did not cover the mesh-reference table contiguously in file order.
    NonContiguousCategory {
        category: usize,
        expected_first_mesh: usize,
        found_first_mesh: u32,
    },
    /// Categories left entries uncovered at the end of the mesh-reference table.
    UncoveredMeshReferences {
        first_uncovered: usize,
        total_meshes: usize,
    },
}

impl From<ReadError> for UnitVariantError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 {
                field: "variable string",
                record: 0,
                offset,
            },
        }
    }
}

impl fmt::Display for UnitVariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => {
                write!(f, "unexpected end of data at {offset} (needed {needed})")
            }
            Self::BadMagic { found, .. } => write!(f, "not a unit_variant (magic {found:02x?})"),
            Self::InvalidCategoryTableOffset { found } => {
                write!(f, "category table offset {found} is not 20")
            }
            Self::InvalidMeshTableOffset { found, expected } => {
                write!(
                    f,
                    "mesh table offset {found} does not follow the category table at {expected}"
                )
            }
            Self::CountOverflow => write!(f, "record count overflows an offset"),
            Self::MisalignedMeshTable { byte_count } => {
                write!(f, "{byte_count} mesh-table bytes is not divisible by 1026")
            }
            Self::InvalidUtf16 {
                field,
                record,
                offset,
            } => write!(f, "invalid UTF-16 in {field} {record} at {offset}"),
            Self::NonZeroStringPadding {
                field,
                record,
                offset,
            } => {
                write!(f, "non-zero padding in {field} {record} at {offset}")
            }
            Self::InvalidCategoryRange {
                category,
                first_mesh,
                mesh_count,
                total_meshes,
            } => write!(
                f,
                "category {category} range {first_mesh}..{} is outside {total_meshes} mesh references",
                first_mesh.saturating_add(*mesh_count)
            ),
            Self::NonContiguousCategory {
                category,
                expected_first_mesh,
                found_first_mesh,
            } => write!(
                f,
                "category {category} starts at {found_first_mesh}, expected {expected_first_mesh}"
            ),
            Self::UncoveredMeshReferences {
                first_uncovered,
                total_meshes,
            } => {
                write!(
                    f,
                    "mesh references {first_uncovered}..{total_meshes} are not assigned to a category"
                )
            }
        }
    }
}

impl std::error::Error for UnitVariantError {}

/// A `.variant_part_mesh` (`VMPF`) file.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantPartMesh {
    /// Header words shared by all observed VMPF files.
    pub header: VariantPartMeshHeader,
    /// The parsed LOD data, or an opaque equipment-container body.
    pub body: VariantPartMeshBody,
}

impl VariantPartMesh {
    /// Parses a complete `.variant_part_mesh` file.
    ///
    /// Vertex records are retained as raw bytes; decode them with
    /// [`VariantPartMeshLod::decode_vertices`].
    pub fn read(bytes: &[u8]) -> Result<Self, VariantPartMeshError> {
        let mut c = Cursor::new(bytes);
        let magic = take_magic(&mut c).map_err(VariantPartMeshError::from)?;
        if magic != VARIANT_PART_MESH_MAGIC {
            return Err(VariantPartMeshError::BadMagic {
                expected: VARIANT_PART_MESH_MAGIC,
                found: magic,
            });
        }
        let header = VariantPartMeshHeader {
            unknown_04: c.u32()?,
            vertex_format: VariantVertexFormat::from_raw(c.u32()?),
            attachment_count: c.u32()?,
            lod_count: c.u32()?,
            total_vertices: c.u32()?,
            total_indices: c.u32()?,
            scalar_parameter_count: c.u32()?,
            vector_parameter_count: c.u32()?,
        };

        if header.vertex_format == VariantVertexFormat::EquipmentContainer {
            return read_equipment_container(c, header);
        }
        let Some(vertex_stride) = header.vertex_format.vertex_stride() else {
            let VariantVertexFormat::Unknown(format) = header.vertex_format else {
                unreachable!()
            };
            return Err(VariantPartMeshError::UnsupportedVertexFormat { format });
        };
        let mut lods = Vec::new();
        let mut vertices_total = 0u32;
        let mut indices_total = 0u32;
        for lod_index in 0..header.lod_count as usize {
            let vertex_count = c.u32()?;
            let index_count = c.u32()?;
            let vertex_bytes = checked_bytes(vertex_count, vertex_stride, c.pos())?;
            let index_bytes = checked_bytes(index_count, 2, c.pos())?;
            let lod_bytes = vertex_bytes
                .checked_add(index_bytes)
                .ok_or(VariantPartMeshError::CountOverflow)?;
            if lod_bytes > c.remaining() {
                return Err(VariantPartMeshError::UnexpectedEof {
                    offset: c.pos(),
                    needed: lod_bytes,
                });
            }
            let vertices = c.take(vertex_bytes)?.to_vec();
            let mut indices = Vec::new();
            for _ in 0..index_count {
                indices.push(c.u16()?);
            }
            vertices_total = vertices_total
                .checked_add(vertex_count)
                .ok_or(VariantPartMeshError::CountOverflow)?;
            indices_total = indices_total
                .checked_add(index_count)
                .ok_or(VariantPartMeshError::CountOverflow)?;
            lods.push(VariantPartMeshLod {
                vertex_count,
                index_count,
                vertices,
                indices,
            });
            debug_assert_eq!(lods[lod_index].vertices.len(), vertex_bytes);
        }
        if (vertices_total, indices_total) != (header.total_vertices, header.total_indices) {
            return Err(VariantPartMeshError::IncorrectTotals {
                expected_vertices: header.total_vertices,
                found_vertices: vertices_total,
                expected_indices: header.total_indices,
                found_indices: indices_total,
            });
        }

        let mut attachments = Vec::new();
        for attachment in 0..header.attachment_count as usize {
            let name = fixed_utf16_part(&mut c, 16, "attachment name", attachment)?;
            let mut matrix = [0f32; 16];
            for value in &mut matrix {
                *value = c.f32()?;
            }
            let bone = c.u32()?;
            attachments.push(VariantPartMeshAttachment { name, matrix, bone });
        }
        let (scalar_parameters, vector_parameters) = read_parameters(&mut c, &header)?;
        Ok(Self {
            header,
            body: VariantPartMeshBody::Part {
                lods,
                attachments,
                scalar_parameters,
                vector_parameters,
            },
        })
    }
}

/// Header of a [`VariantPartMesh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariantPartMeshHeader {
    /// Zero in every shipped VMPF file; its meaning is **UNKNOWN**.
    pub unknown_04: u32,
    /// The observed vertex-record format.
    pub vertex_format: VariantVertexFormat,
    /// Number of 100-byte attachment records ([u16;16] name, 16 f32, u32).
    pub attachment_count: u32,
    /// Number of LOD blocks.
    pub lod_count: u32,
    /// Sum of the per-LOD vertex counts.
    pub total_vertices: u32,
    /// Sum of the per-LOD index counts.
    pub total_indices: u32,
    /// Number of 68-byte scalar parameter records after the LODs and attachments.
    pub scalar_parameter_count: u32,
    /// Number of variable-length vector parameter records after scalar parameters.
    pub vector_parameter_count: u32,
}

/// VMPF vertex format word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantVertexFormat {
    /// Format 0: a 64-byte rigid vertex record (plumes; equipment pieces).
    Bytes64,
    /// Format 1: a 40-byte two-bone skinned vertex record.
    Bytes40,
    /// Format 2: a named equipment-piece container.
    EquipmentContainer,
    /// A format not observed in the shipped game.
    Unknown(u32),
}

impl VariantVertexFormat {
    fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Bytes64,
            1 => Self::Bytes40,
            2 => Self::EquipmentContainer,
            other => Self::Unknown(other),
        }
    }

    /// Size of one vertex record for ordinary part meshes, if known.
    pub fn vertex_stride(self) -> Option<usize> {
        match self {
            Self::Bytes64 => Some(64),
            Self::Bytes40 => Some(40),
            Self::EquipmentContainer | Self::Unknown(_) => None,
        }
    }
}

/// Body of a [`VariantPartMesh`].
#[derive(Debug, Clone, PartialEq)]
pub enum VariantPartMeshBody {
    /// An ordinary part mesh with a fully walked LOD and parameter stream.
    Part {
        /// LOD blocks in file order (the first is the most detailed in 287/297 files).
        lods: Vec<VariantPartMeshLod>,
        /// Attachment points (name, transform, bone).
        attachments: Vec<VariantPartMeshAttachment>,
        /// Named scalar material parameters.
        scalar_parameters: Vec<VariantPartMeshScalarParameter>,
        /// Named four-float material parameters.
        vector_parameters: Vec<VariantPartMeshVectorParameter>,
    },
    /// Format-2 equipment container: named rigid pieces (muskets, swords, bags ...),
    /// each bound to one skeleton bone. `lod_count` in the header is the piece count.
    EquipmentContainer {
        pieces: Vec<EquipmentPiece>,
        scalar_parameters: Vec<VariantPartMeshScalarParameter>,
        vector_parameters: Vec<VariantPartMeshVectorParameter>,
    },
}

/// One LOD block of an ordinary VMPF part mesh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantPartMeshLod {
    /// Number of fixed-stride records in [`Self::vertices`].
    pub vertex_count: u32,
    /// Number of triangle-list indices.
    pub index_count: u32,
    /// Raw fixed-stride vertex records; see [`VariantPartMeshLod::decode_vertices`].
    pub vertices: Vec<u8>,
    /// 16-bit triangle-list indices, in file order.
    pub indices: Vec<u16>,
}

/// An attachment record after a VMPF LOD stream (100 bytes).
///
/// Layout CONFIRMED by size accounting on all 89 files with an attachment: `[u16; 16]`
/// name, 16 f32, u32. Meaning INFERRED from `austrian_bearskin`: the floats are a 4x4
/// row-major transform whose last column is the translation (orthonormal 3x3 part,
/// last row `0 0 0 1`), and the u32 is a skeleton bone index the attachment follows.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantPartMeshAttachment {
    /// Attachment name (e.g. `plumes`).
    pub name: String,
    /// 4x4 transform, file order (INFERRED row-major, translation in elements 3, 7, 11).
    pub matrix: [f32; 16],
    /// INFERRED: bone index the attachment point is relative to.
    pub bone: u32,
}

/// A named scalar material parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantPartMeshScalarParameter {
    pub name: String,
    pub value: f32,
}

/// A named four-float material parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantPartMeshVectorParameter {
    /// Fixed-size parameter name.
    pub name: String,
    /// Variable-length UTF-16 name stored alongside the value. It has matched the
    /// fixed name in files surveyed so far; retaining both avoids assuming that is a
    /// required invariant.
    pub value_name: String,
    pub value: [f32; 4],
}

/// Why a [`VariantPartMesh`] could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantPartMeshError {
    UnexpectedEof {
        offset: usize,
        needed: usize,
    },
    BadMagic {
        expected: [u8; 4],
        found: [u8; 4],
    },
    /// A VMPF vertex format other than the three observed values (0, 1, and 2).
    UnsupportedVertexFormat {
        format: u32,
    },
    CountOverflow,
    InvalidUtf16 {
        field: &'static str,
        record: usize,
        offset: usize,
    },
    NonZeroStringPadding {
        field: &'static str,
        record: usize,
        offset: usize,
    },
    /// Header totals did not equal the sum of the parsed ordinary-part LODs.
    IncorrectTotals {
        expected_vertices: u32,
        found_vertices: u32,
        expected_indices: u32,
        found_indices: u32,
    },
    TrailingBytes {
        offset: usize,
        count: usize,
    },
}

impl From<ReadError> for VariantPartMeshError {
    fn from(error: ReadError) -> Self {
        match error {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 {
                field: "variable string",
                record: 0,
                offset,
            },
        }
    }
}

impl fmt::Display for VariantPartMeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => {
                write!(f, "unexpected end of data at {offset} (needed {needed})")
            }
            Self::BadMagic { found, .. } => {
                write!(f, "not a variant_part_mesh (magic {found:02x?})")
            }
            Self::UnsupportedVertexFormat { format } => {
                write!(f, "unsupported variant-part vertex format {format}")
            }
            Self::CountOverflow => write!(f, "record count overflows an offset"),
            Self::InvalidUtf16 {
                field,
                record,
                offset,
            } => write!(f, "invalid UTF-16 in {field} {record} at {offset}"),
            Self::NonZeroStringPadding {
                field,
                record,
                offset,
            } => {
                write!(f, "non-zero padding in {field} {record} at {offset}")
            }
            Self::IncorrectTotals {
                expected_vertices,
                found_vertices,
                expected_indices,
                found_indices,
            } => write!(
                f,
                "LOD totals ({found_vertices} vertices, {found_indices} indices) do not match header ({expected_vertices}, {expected_indices})"
            ),
            Self::TrailingBytes { offset, count } => {
                write!(f, "{count} trailing bytes at {offset}")
            }
        }
    }
}

impl std::error::Error for VariantPartMeshError {}

fn take_magic(c: &mut Cursor<'_>) -> Result<[u8; 4], ReadError> {
    let bytes = c.take(4)?;
    Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn fixed_utf16(
    c: &mut Cursor<'_>,
    units: usize,
    field: &'static str,
    record: usize,
) -> Result<String, UnitVariantError> {
    let offset = c.pos();
    let bytes = c.take(
        units
            .checked_mul(2)
            .ok_or(UnitVariantError::CountOverflow)?,
    )?;
    fixed_utf16_from_bytes(bytes, field, record, offset).map_err(|kind| match kind {
        FixedUtf16Error::Invalid => UnitVariantError::InvalidUtf16 {
            field,
            record,
            offset,
        },
        FixedUtf16Error::Padding => UnitVariantError::NonZeroStringPadding {
            field,
            record,
            offset,
        },
    })
}

fn fixed_utf16_part(
    c: &mut Cursor<'_>,
    units: usize,
    field: &'static str,
    record: usize,
) -> Result<String, VariantPartMeshError> {
    let offset = c.pos();
    let bytes = c.take(
        units
            .checked_mul(2)
            .ok_or(VariantPartMeshError::CountOverflow)?,
    )?;
    fixed_utf16_from_bytes(bytes, field, record, offset).map_err(|kind| match kind {
        FixedUtf16Error::Invalid => VariantPartMeshError::InvalidUtf16 {
            field,
            record,
            offset,
        },
        FixedUtf16Error::Padding => VariantPartMeshError::NonZeroStringPadding {
            field,
            record,
            offset,
        },
    })
}

enum FixedUtf16Error {
    Invalid,
    Padding,
}

fn fixed_utf16_from_bytes(
    bytes: &[u8],
    _field: &'static str,
    _record: usize,
    _offset: usize,
) -> Result<String, FixedUtf16Error> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>().0.iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    if units[end..].iter().any(|&unit| unit != 0) {
        return Err(FixedUtf16Error::Padding);
    }
    String::from_utf16(&units[..end]).map_err(|_| FixedUtf16Error::Invalid)
}

fn utf16_units(
    bytes: &[u8],
    field: &'static str,
    record: usize,
    offset: usize,
) -> Result<String, VariantPartMeshError> {
    let units: Vec<u16> = bytes
        .as_chunks::<2>().0.iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| VariantPartMeshError::InvalidUtf16 {
        field,
        record,
        offset,
    })
}

fn validate_category_ranges(
    categories: &[UnitVariantCategory],
    total_meshes: usize,
) -> Result<(), UnitVariantError> {
    let mut expected_first_mesh = 0usize;
    for (category, entry) in categories.iter().enumerate() {
        let first_mesh = entry.first_mesh as usize;
        let Some(end) = first_mesh.checked_add(entry.mesh_count as usize) else {
            return Err(UnitVariantError::InvalidCategoryRange {
                category,
                first_mesh: entry.first_mesh,
                mesh_count: entry.mesh_count,
                total_meshes,
            });
        };
        if end > total_meshes {
            return Err(UnitVariantError::InvalidCategoryRange {
                category,
                first_mesh: entry.first_mesh,
                mesh_count: entry.mesh_count,
                total_meshes,
            });
        }
        if first_mesh != expected_first_mesh {
            return Err(UnitVariantError::NonContiguousCategory {
                category,
                expected_first_mesh,
                found_first_mesh: entry.first_mesh,
            });
        }
        expected_first_mesh = end;
    }
    if expected_first_mesh != total_meshes {
        return Err(UnitVariantError::UncoveredMeshReferences {
            first_uncovered: expected_first_mesh,
            total_meshes,
        });
    }
    Ok(())
}

fn checked_bytes(
    count: u32,
    item_size: usize,
    offset: usize,
) -> Result<usize, VariantPartMeshError> {
    let bytes = (count as usize)
        .checked_mul(item_size)
        .ok_or(VariantPartMeshError::CountOverflow)?;
    if bytes > usize::MAX - offset {
        return Err(VariantPartMeshError::CountOverflow);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_utf16_bytes(text: &str, units: usize) -> Vec<u8> {
        let encoded: Vec<u16> = text.encode_utf16().collect();
        assert!(encoded.len() < units);
        let mut bytes = Vec::with_capacity(units * 2);
        for unit in encoded.into_iter().chain(std::iter::repeat(0)).take(units) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    fn u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn unit_variant_fixture() -> Vec<u8> {
        let mut bytes = UNIT_VARIANT_MAGIC.to_vec();
        u32(&mut bytes, 0);
        u32(&mut bytes, 2);
        u32(&mut bytes, 20);
        u32(&mut bytes, (20 + 2 * UNIT_VARIANT_CATEGORY_SIZE) as u32);
        for (name, index, mesh_count, first_mesh) in [("heads", 9, 1, 0), ("equipment", 3, 1, 1)] {
            bytes.extend(fixed_utf16_bytes(name, 256));
            u32(&mut bytes, index);
            u32(&mut bytes, 0);
            u32(&mut bytes, mesh_count);
            u32(&mut bytes, first_mesh);
        }
        for (mesh, texture, kind) in [
            ("unitparts/euro/head_a", "units/euro/head_a", 0u16),
            ("rigid_equip_euro_sabre", "", 1u16),
        ] {
            bytes.extend(fixed_utf16_bytes(mesh, 256));
            bytes.extend(fixed_utf16_bytes(texture, 256));
            bytes.extend_from_slice(&kind.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn reads_unit_variant_and_category_ranges() {
        let variant = UnitVariant::read(&unit_variant_fixture()).unwrap();
        assert_eq!(variant.version, 0);
        assert_eq!(
            variant.categories[0].index, 9,
            "stored index is not the category position"
        );
        assert_eq!(
            variant.category_meshes(0).unwrap()[0].mesh,
            "unitparts/euro/head_a"
        );
        assert_eq!(variant.category_meshes(1).unwrap()[0].kind, 1);
        assert!(variant.category_meshes(2).is_none());
    }

    #[test]
    fn rejects_malformed_unit_variant_without_panicking() {
        let bytes = unit_variant_fixture();
        for cut in [0, 3, 19, 20, bytes.len() - 1] {
            assert!(UnitVariant::read(&bytes[..cut]).is_err());
        }
        let mut padded = bytes.clone();
        padded[20 + 12] = 1;
        assert!(matches!(
            UnitVariant::read(&padded),
            Err(UnitVariantError::NonZeroStringPadding { .. })
        ));
        let mut gap = bytes;
        let second_category_first_mesh = 20 + UNIT_VARIANT_CATEGORY_SIZE + 524;
        gap[second_category_first_mesh..second_category_first_mesh + 4]
            .copy_from_slice(&2u32.to_le_bytes());
        assert!(matches!(
            UnitVariant::read(&gap),
            Err(UnitVariantError::InvalidCategoryRange { .. })
        ));
    }

    fn vmpf_fixture(format: u32) -> Vec<u8> {
        let stride = if format == 0 { 64 } else { 40 };
        let mut bytes = VARIANT_PART_MESH_MAGIC.to_vec();
        u32(&mut bytes, 0);
        u32(&mut bytes, format);
        u32(&mut bytes, 1);
        u32(&mut bytes, 2);
        u32(&mut bytes, 3);
        u32(&mut bytes, 6);
        u32(&mut bytes, 1);
        u32(&mut bytes, 1);
        for (vertices, indices, fill) in [
            (1u32, [0u16, 0, 0].as_slice(), 0x11),
            (2, [0u16, 1, 1].as_slice(), 0x22),
        ] {
            u32(&mut bytes, vertices);
            u32(&mut bytes, indices.len() as u32);
            bytes.extend(std::iter::repeat_n(fill, vertices as usize * stride));
            for index in indices {
                bytes.extend_from_slice(&index.to_le_bytes());
            }
        }
        bytes.extend(fixed_utf16_bytes("plumes", 16));
        for i in 0..16 {
            bytes.extend_from_slice(&(i as f32).to_le_bytes());
        }
        u32(&mut bytes, 19);
        bytes.extend(fixed_utf16_bytes("light_scale", 32));
        bytes.extend_from_slice(&2.5f32.to_le_bytes());
        bytes.extend(fixed_utf16_bytes("specfactor", 32));
        let name: Vec<u16> = "specfactor".encode_utf16().collect();
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        for unit in name {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        for value in [1.0f32, 2.0, 3.0, 4.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn reads_40_byte_part_mesh_without_decoding_vertex_fields() {
        let mesh = VariantPartMesh::read(&vmpf_fixture(1)).unwrap();
        assert_eq!(mesh.header.vertex_format, VariantVertexFormat::Bytes40);
        let VariantPartMeshBody::Part {
            lods,
            attachments,
            scalar_parameters,
            vector_parameters,
        } = mesh.body
        else {
            panic!("expected ordinary part mesh");
        };
        assert_eq!(lods.iter().map(|lod| lod.vertex_count).sum::<u32>(), 3);
        assert_eq!(lods[0].vertices.len(), 40);
        assert_eq!(lods[1].indices, [0, 1, 1]);
        assert_eq!(attachments[0].name, "plumes");
        assert_eq!(attachments[0].matrix[15], 15.0);
        assert_eq!(attachments[0].bone, 19);
        assert_eq!(scalar_parameters[0].name, "light_scale");
        assert_eq!(scalar_parameters[0].value, 2.5);
        assert_eq!(vector_parameters[0].value_name, "specfactor");
        assert_eq!(vector_parameters[0].value, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn reads_64_byte_part_mesh_and_keeps_equipment_container_opaque() {
        let mesh = VariantPartMesh::read(&vmpf_fixture(0)).unwrap();
        let VariantPartMeshBody::Part { lods, .. } = mesh.body else {
            panic!("expected ordinary part mesh")
        };
        assert_eq!(lods[0].vertices.len(), 64);

        let mut equipment = VARIANT_PART_MESH_MAGIC.to_vec();
        for word in [0, 2, 0, 1, 1, 3, 0, 0] {
            u32(&mut equipment, word);
        }
        equipment.extend(fixed_utf16_bytes("rigid_equip_euro_musket01", 40));
        for word in [1, 1, 3] {
            u32(&mut equipment, word);
        }
        let mut vertex = [0u8; 64];
        vertex[0..4].copy_from_slice(&0.5f32.to_le_bytes());
        vertex[16..19].copy_from_slice(&[0x80, 0x80, 0xFF]);
        equipment.extend(vertex);
        equipment.extend([0u8; 6]);
        let mesh = VariantPartMesh::read(&equipment).unwrap();
        let VariantPartMeshBody::EquipmentContainer { pieces, .. } = &mesh.body else {
            panic!("expected container")
        };
        assert_eq!(pieces[0].name, "rigid_equip_euro_musket01");
        assert_eq!(pieces[0].bone, Some(1));
        let v = pieces[0].lod.decode_vertices(VariantVertexFormat::Bytes64);
        assert_eq!(v[0].positions[0], [0.5, 0.0, 0.0]);
        assert!((v[0].normals[0][0] - 1.0).abs() < 0.01, "z,y,x byte order");
    }

    #[test]
    fn rejects_malformed_part_mesh_without_panicking() {
        let bytes = vmpf_fixture(1);
        for cut in [0, 3, 35, 40, bytes.len() - 1] {
            assert!(VariantPartMesh::read(&bytes[..cut]).is_err());
        }
        let mut wrong_totals = bytes;
        wrong_totals[20..24].copy_from_slice(&4u32.to_le_bytes());
        assert!(matches!(
            VariantPartMesh::read(&wrong_totals),
            Err(VariantPartMeshError::IncorrectTotals { .. })
        ));
    }
}

/// Converts an IEEE 754 binary16 (half float) to `f32`. Soldier part meshes store their
/// vertex positions and UVs as half floats (D3DDECLTYPE_FLOAT16_2/4).
pub fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0f32 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1F);
    let mant = f32::from(h & 0x3FF);
    match exp {
        0 => sign * mant * 2f32.powi(-24),
        31 if mant == 0.0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + mant / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// One decoded soldier part-mesh vertex.
///
/// 40-byte format 1 (skinned), byte map (INFERRED from a full-pack byte survey of
/// 322,982 vertices plus hand checks; field names are ours):
/// ```text
///  0 f16 x3  position relative to bone A      6 f16 u
///  8 f16 x3  position relative to bone B     14 f16 v
/// 16 [u8;4]  always 0            20 f16 x2  always (1.0, 1.0)            (UNKNOWN)
/// 24 u8 x3   normal in bone A space  27 u8 bone A index
/// 28 u8 x3   normal in bone B space  31 u8 bone B index  (all four 0 when unused)
/// 32 u8 x3   tangent             35 u8 weight of bone A, 0x80..0xFF (/255)
/// 36 u8 x3   binormal            39 u8 always 0
/// ```
/// Unit vectors are biased bytes, `(b - 127.5) / 127.5`, stored in z, y, x order
/// (D3DCOLOR b, g, r). Triangles wind so that `(p1 - p0) x (p2 - p0)` points outward
/// in file space (same statistical check).
/// Bone indices refer to the 41-bone soldier skeleton of the `.anim` files (e.g. a hat
/// on bone 19 `Head`, trousers on 0 `Hips` / 4 `LeftUpLeg` / 8 `LeftLeg`). Each
/// position is in its bone's own frame, so a skinned position is
/// `w * (M_A * pos_A) + (1 - w) * (M_B * pos_B)` with the bones' model-space
/// matrices from an animation frame; no bind pose is needed.
///
/// 64-byte format 0 (plumes, not skinned): f32 x4 position (w = 1), u8x4 normal,
/// tangent, binormal, f32 u, v, then 28 bytes (UNKNOWN; mostly zero, ends with
/// f32 1.0, 1.0). Positions are relative to the parent part's attachment point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VariantVertex {
    /// Positions relative to bone A and bone B (format 0: `[p, p]`).
    pub positions: [[f32; 3]; 2],
    /// Normals in bone A and bone B space (format 0: `[n, n]`).
    pub normals: [[f32; 3]; 2],
    pub tangent: [f32; 3],
    pub binormal: [f32; 3],
    pub uv: [f32; 2],
    /// Bone A / bone B indices; `None` for unskinned format-0 vertices.
    pub bones: Option<[u8; 2]>,
    /// Weight of bone A (bone B gets `1 - weight`).
    pub weight: f32,
}

fn unit_byte(b: u8) -> f32 {
    (f32::from(b) - 127.5) / 127.5
}

/// Bytes are stored D3DCOLOR-style (b, g, r) = (z, y, x): CONFIRMED statistically, the
/// stored normal agrees in sign with the geometric face normal for 193,183 of 196,145
/// single-bone triangles in this order (only 90,959 net for x, y, z order).
fn unit3(b: &[u8]) -> [f32; 3] {
    [unit_byte(b[2]), unit_byte(b[1]), unit_byte(b[0])]
}

fn h(b: &[u8], at: usize) -> f32 {
    f16_to_f32(u16::from_le_bytes([b[at], b[at + 1]]))
}

fn f(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

impl VariantPartMeshLod {
    /// Decodes the raw vertex records of this LOD (see [`VariantVertex`]).
    pub fn decode_vertices(&self, format: VariantVertexFormat) -> Vec<VariantVertex> {
        let Some(stride) = format.vertex_stride() else {
            return Vec::new();
        };
        self.vertices
            .chunks_exact(stride)
            .map(|v| match format {
                VariantVertexFormat::Bytes40 => {
                    let (bone_a, bone_b) = (v[27], v[31]);
                    let weight = f32::from(v[35]) / 255.0;
                    VariantVertex {
                        positions: [[h(v, 0), h(v, 2), h(v, 4)], [h(v, 8), h(v, 10), h(v, 12)]],
                        normals: [unit3(&v[24..27]), unit3(&v[28..31])],
                        tangent: unit3(&v[32..35]),
                        binormal: unit3(&v[36..39]),
                        uv: [h(v, 6), h(v, 14)],
                        bones: Some([bone_a, bone_b]),
                        weight,
                    }
                }
                _ => {
                    let p = [f(v, 0), f(v, 4), f(v, 8)];
                    let n = unit3(&v[16..19]);
                    VariantVertex {
                        positions: [p, p],
                        normals: [n, n],
                        tangent: unit3(&v[20..23]),
                        binormal: unit3(&v[24..27]),
                        uv: [f(v, 28), f(v, 32)],
                        bones: None,
                        weight: 1.0,
                    }
                }
            })
            .collect()
    }
}

/// One named piece of a format-2 equipment container.
///
/// Layout (CONFIRMED by walking both shipped containers to EOF, see the real-install
/// test): `[u16; 40]` name, i32 bone (-1 for the `Reference` piece), u32 V, u32 I,
/// V x 64-byte vertex (same record as format 0), I x u16 index. Piece names carry an
/// `_lodN` suffix; `.unit_variant` kind-1 entries name them without it (INFERRED that
/// an unsuffixed piece is LOD 0).
#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentPiece {
    pub name: String,
    /// Skeleton bone the piece is rigidly bound to (e.g. 1 = `Weapon1`); positions are
    /// in that bone's frame (INFERRED). `None` for -1.
    pub bone: Option<u32>,
    /// Raw 64-byte vertex records, decoded with [`VariantPartMeshLod::decode_vertices`].
    pub lod: VariantPartMeshLod,
}

fn read_equipment_container(
    mut c: Cursor<'_>,
    header: VariantPartMeshHeader,
) -> Result<VariantPartMesh, VariantPartMeshError> {
    let mut pieces = Vec::new();
    let (mut vertices_total, mut indices_total) = (0u32, 0u32);
    for piece in 0..header.lod_count as usize {
        let name = fixed_utf16_part(&mut c, 40, "equipment piece name", piece)?;
        let bone = c.u32()?;
        let vertex_count = c.u32()?;
        let index_count = c.u32()?;
        let vertex_bytes = checked_bytes(vertex_count, 64, c.pos())?;
        let index_bytes = checked_bytes(index_count, 2, c.pos())?;
        if vertex_bytes.saturating_add(index_bytes) > c.remaining() {
            return Err(VariantPartMeshError::UnexpectedEof {
                offset: c.pos(),
                needed: vertex_bytes + index_bytes,
            });
        }
        let vertices = c.take(vertex_bytes)?.to_vec();
        let mut indices = Vec::with_capacity(index_count as usize);
        for _ in 0..index_count {
            indices.push(c.u16()?);
        }
        vertices_total = vertices_total.saturating_add(vertex_count);
        indices_total = indices_total.saturating_add(index_count);
        pieces.push(EquipmentPiece {
            name,
            bone: (bone != u32::MAX).then_some(bone),
            lod: VariantPartMeshLod {
                vertex_count,
                index_count,
                vertices,
                indices,
            },
        });
    }
    if (vertices_total, indices_total) != (header.total_vertices, header.total_indices) {
        return Err(VariantPartMeshError::IncorrectTotals {
            expected_vertices: header.total_vertices,
            found_vertices: vertices_total,
            expected_indices: header.total_indices,
            found_indices: indices_total,
        });
    }
    let (scalar_parameters, vector_parameters) = read_parameters(&mut c, &header)?;
    Ok(VariantPartMesh {
        header,
        body: VariantPartMeshBody::EquipmentContainer {
            pieces,
            scalar_parameters,
            vector_parameters,
        },
    })
}

type Parameters = (
    Vec<VariantPartMeshScalarParameter>,
    Vec<VariantPartMeshVectorParameter>,
);

/// Reads the scalar and vector material parameters that end every VMPF file, and
/// checks that the file ends right after them.
fn read_parameters(
    c: &mut Cursor<'_>,
    header: &VariantPartMeshHeader,
) -> Result<Parameters, VariantPartMeshError> {
    let mut scalar_parameters = Vec::new();
    for parameter in 0..header.scalar_parameter_count as usize {
        scalar_parameters.push(VariantPartMeshScalarParameter {
            name: fixed_utf16_part(c, 32, "scalar parameter name", parameter)?,
            value: c.f32()?,
        });
    }
    let mut vector_parameters = Vec::new();
    for parameter in 0..header.vector_parameter_count as usize {
        let name = fixed_utf16_part(c, 32, "vector parameter name", parameter)?;
        let value_name_units = usize::from(c.u16()?);
        let name_bytes = value_name_units
            .checked_mul(2)
            .ok_or(VariantPartMeshError::CountOverflow)?;
        let value_name = utf16_units(
            c.take(name_bytes)?,
            "vector value name",
            parameter,
            c.pos() - name_bytes,
        )?;
        let value = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
        vector_parameters.push(VariantPartMeshVectorParameter {
            name,
            value_name,
            value,
        });
    }
    if c.remaining() != 0 {
        return Err(VariantPartMeshError::TrailingBytes {
            offset: c.pos(),
            count: c.remaining(),
        });
    }
    Ok((scalar_parameters, vector_parameters))
}

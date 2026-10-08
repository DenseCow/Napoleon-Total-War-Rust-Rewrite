//! From a `units` key to a drawable soldier: DB lookups, variant files, part meshes,
//! equipment pieces, textures, faction colours and CPU skinning.
//!
//! The chain (analysis/worker2/UNIT_VARIANT_AND_TEXTURES.md §2, CONFIRMED by a full
//! cross-check there and by `tests/real_install.rs::every_unit_resolves_to_parts`):
//! ```text
//! units.key -> uniforms (uniform, faction, variant, unit)
//!           -> variantmodels/units/<lower(variant)>.<role>.unit_variant
//!           -> per category: kind 0 = part mesh path + texture stem,
//!                            kind 1 = equipment piece name (equipment/mesh*.variant_part_mesh)
//! colours:  uniform_to_faction_colours (uniform, faction) else faction_uniform_colours (faction)
//! ```
//! What the exe does that is still UNKNOWN (stand-ins are marked PROVISIONAL):
//! - how one mesh per category is picked for each man (we pick by a caller seed);
//! - which categories are optional (we draw every non-empty category except
//!   `equipment_secondary_weapon`, which the man would carry sheathed or not at all);
//! - the shader's use of the colour mask (see [`tint`]).

use std::collections::HashMap;

use crate::anim::{mat_mul, transform_point, transform_vector};
use crate::db::{DbTable, DbValue, Schema};
use crate::pack::{PackError, Vfs};
use crate::unit_variant::{
    EquipmentPiece, UnitVariant, UnitVariantMeshRef, VariantPartMesh, VariantPartMeshBody,
    VariantVertex, VariantVertexFormat,
};

/// An RGB colour from the uniform colour tables (0..=255 each).
pub type Rgb = [u8; 3];

/// One `uniforms` row (schema str, str, str, str; CONFIRMED by parsing to EOF).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniformRow {
    pub uniform: String,
    pub faction: String,
    /// Variant file stem (case as stored; files are looked up lower-cased).
    pub variant: String,
    /// `units` key.
    pub unit: String,
}

/// The three uniform colours. INFERRED: they tint the red, green and blue channels of a
/// part's `_colour_mask` texture respectively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniformColours {
    pub primary: Rgb,
    pub secondary: Rgb,
    pub tertiary: Rgb,
}

impl UniformColours {
    fn from_row(values: &[DbValue]) -> Option<Self> {
        let mut c = [0u8; 9];
        for (i, v) in values.iter().take(9).enumerate() {
            c[i] = v.as_i32()?.clamp(0, 255) as u8;
        }
        Some(Self {
            primary: [c[0], c[1], c[2]],
            secondary: [c[3], c[4], c[5]],
            tertiary: [c[6], c[7], c[8]],
        })
    }
}

/// Which model of a unit: the rank-and-file man or one of the command figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariantRole {
    Soldier,
    Officer,
    Musician,
    StandardBearer,
}

impl VariantRole {
    /// The file-name infix (`<variant>.<role>.unit_variant`).
    pub fn file_infix(self) -> &'static str {
        match self {
            Self::Soldier => "soldier",
            Self::Officer => "officer",
            Self::Musician => "musician",
            Self::StandardBearer => "standard_bearer",
        }
    }
}

/// Path of the variant file for a variant name and role.
pub fn unit_variant_path(variant: &str, role: VariantRole) -> String {
    format!("variantmodels/units/{}.{}.unit_variant", variant.to_ascii_lowercase(), role.file_infix())
}

/// Why the unit-model data could not be loaded.
#[derive(Debug)]
pub enum UnitModelError {
    Pack(PackError),
    Db { table: &'static str, error: crate::db::DbError },
    Variant { path: String, error: crate::unit_variant::UnitVariantError },
    Mesh { path: String, error: crate::unit_variant::VariantPartMeshError },
}

impl std::fmt::Display for UnitModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(e) => write!(f, "{e}"),
            Self::Db { table, error } => write!(f, "{table}: {error}"),
            Self::Variant { path, error } => write!(f, "{path}: {error}"),
            Self::Mesh { path, error } => write!(f, "{path}: {error}"),
        }
    }
}

impl std::error::Error for UnitModelError {}

impl From<PackError> for UnitModelError {
    fn from(e: PackError) -> Self {
        Self::Pack(e)
    }
}

fn read_table(vfs: &Vfs, table: &'static str, codes: &str) -> Result<DbTable, UnitModelError> {
    let bytes = vfs.read(&format!("db/{table}_tables/{table}"))?;
    let schema = Schema::from_codes(codes).expect("valid schema codes");
    DbTable::read(&bytes, &schema).map_err(|error| UnitModelError::Db { table, error })
}

fn cell(row: &[DbValue], i: usize) -> String {
    row.get(i).and_then(DbValue::as_str).unwrap_or_default().to_owned()
}

/// The three DB tables that tie unit keys to variant files and colours.
#[derive(Debug, Clone, Default)]
pub struct UnitModelIndex {
    pub uniforms: Vec<UniformRow>,
    /// (lower uniform, lower faction) -> colours.
    uniform_colours: HashMap<(String, String), UniformColours>,
    /// lower faction -> colours.
    faction_colours: HashMap<String, UniformColours>,
}

impl UnitModelIndex {
    /// Loads `uniforms`, `uniform_to_faction_colours` and `faction_uniform_colours`.
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, UnitModelError> {
        let uniforms = read_table(vfs, "uniforms", "s,s,s,s")?
            .rows
            .iter()
            .map(|r| UniformRow { uniform: cell(r, 0), faction: cell(r, 1), variant: cell(r, 2), unit: cell(r, 3) })
            .collect();
        let mut uniform_colours = HashMap::new();
        for r in read_table(vfs, "uniform_to_faction_colours", "s,s,i,i,i,i,i,i,i,i,i")?.rows {
            if let Some(c) = UniformColours::from_row(&r[2..]) {
                uniform_colours.insert((cell(&r, 0).to_ascii_lowercase(), cell(&r, 1).to_ascii_lowercase()), c);
            }
        }
        let mut faction_colours = HashMap::new();
        for r in read_table(vfs, "faction_uniform_colours", "s,i,i,i,i,i,i,i,i,i")?.rows {
            if let Some(c) = UniformColours::from_row(&r[1..]) {
                faction_colours.insert(cell(&r, 0).to_ascii_lowercase(), c);
            }
        }
        Ok(Self { uniforms, uniform_colours, faction_colours })
    }

    /// The uniform rows of a unit key (case-insensitive). With `faction`, rows of that
    /// faction come first.
    pub fn uniforms_for_unit(&self, unit: &str, faction: Option<&str>) -> Vec<&UniformRow> {
        let mut rows: Vec<&UniformRow> =
            self.uniforms.iter().filter(|u| u.unit.eq_ignore_ascii_case(unit)).collect();
        if let Some(f) = faction {
            rows.sort_by_key(|u| !u.faction.eq_ignore_ascii_case(f));
        }
        rows
    }

    /// Colours for a uniform: the per-uniform row, else the faction default
    /// (INFERRED precedence).
    pub fn colours(&self, uniform: &UniformRow) -> Option<UniformColours> {
        let faction = uniform.faction.to_ascii_lowercase();
        self.uniform_colours
            .get(&(uniform.uniform.to_ascii_lowercase(), faction.clone()))
            .or_else(|| self.faction_colours.get(&faction))
            .copied()
    }

    /// The faction's default colours.
    pub fn faction_colours(&self, faction: &str) -> Option<UniformColours> {
        self.faction_colours.get(&faction.to_ascii_lowercase()).copied()
    }
}

/// The DDS files that exist for a texture stem.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextureFiles {
    pub diffuse: Option<String>,
    pub normal: Option<String>,
    pub gloss: Option<String>,
    pub colour_mask: Option<String>,
}

/// Resolves a texture stem to its DDS files. Skin/head stems have numbered sets
/// (`texture[01]_diffuse.dds` ...); `face` picks one (1-based, wraps; PROVISIONAL: how the
/// game picks a face per man is UNKNOWN). Missing numbered files fall back to the plain stem.
pub fn texture_files(vfs: &Vfs, stem: &str, face: usize) -> TextureFiles {
    let find = |suffix: &str| -> Option<String> {
        let numbered = (0..16).map(|k| format!("{stem}[{:02}]{suffix}", (face.max(1) - 1 + k) % 16 + 1));
        std::iter::once(format!("{stem}{suffix}")).chain(numbered).find(|p| vfs.contains(p))
    };
    TextureFiles {
        diffuse: find("_diffuse.dds"),
        normal: find("_normal.dds"),
        gloss: find("_gloss_map.dds"),
        colour_mask: find("_colour_mask.dds"),
    }
}

/// Container files with equipment pieces and the texture stem each container uses
/// (INFERRED pairing by file name: `mesh` -> `texture`, `mesh2` -> `texture2`).
pub const EQUIPMENT_CONTAINERS: [(&str, &str); 2] = [
    ("variantmodels/equipment/mesh.variant_part_mesh", "variantmodels/equipment/texture"),
    ("variantmodels/equipment/mesh2.variant_part_mesh", "variantmodels/equipment/texture2"),
];

/// All equipment pieces from both containers, looked up by name.
#[derive(Debug, Clone, Default)]
pub struct EquipmentLibrary {
    pieces: Vec<(EquipmentPiece, usize)>,
    by_name: HashMap<String, usize>,
}

impl EquipmentLibrary {
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, UnitModelError> {
        let mut lib = Self::default();
        for (container, (path, _)) in EQUIPMENT_CONTAINERS.iter().enumerate() {
            let mesh = VariantPartMesh::read(&vfs.read(path)?)
                .map_err(|error| UnitModelError::Mesh { path: (*path).into(), error })?;
            if let VariantPartMeshBody::EquipmentContainer { pieces, .. } = mesh.body {
                for p in pieces {
                    lib.by_name.entry(p.name.to_ascii_lowercase()).or_insert(lib.pieces.len());
                    lib.pieces.push((p, container));
                }
            }
        }
        Ok(lib)
    }

    /// A piece by the name a `.unit_variant` uses: tries `<name>_lod1` (the most detailed
    /// suffix; INFERRED from vertex counts lod1 > lod2), then the bare name. Returns the
    /// piece and its texture stem.
    pub fn find(&self, name: &str) -> Option<(&EquipmentPiece, &'static str)> {
        let lower = name.to_ascii_lowercase();
        let i = self.by_name.get(&format!("{lower}_lod1")).or_else(|| self.by_name.get(&lower))?;
        let (piece, container) = &self.pieces[*i];
        Some((piece, EQUIPMENT_CONTAINERS[*container].1))
    }

    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

/// What a soldier part is bound to.
#[derive(Debug, Clone, PartialEq)]
pub enum PartBinding {
    /// Two-bone skinned 40-byte vertices (bodies, heads, hats ...).
    Skinned,
    /// Rigid on one bone (equipment pieces).
    Bone(u32),
    /// Rigid on another part's attachment point: `bone` and a column-major transform.
    Attachment { bone: u32, matrix: [f32; 16] },
}

/// One drawable part of a soldier, as decoded data.
#[derive(Debug, Clone)]
pub struct SoldierPart {
    /// Category name from the variant (`torsos`, `equipment_primary_weapon` ...).
    pub category: String,
    /// The mesh path or equipment piece name.
    pub source: String,
    pub binding: PartBinding,
    pub vertices: Vec<VariantVertex>,
    pub indices: Vec<u16>,
    pub textures: TextureFiles,
    /// The less detailed LODs after `vertices`/`indices`, most detailed first (empty for
    /// equipment pieces, whose LODs are separate pieces). The battle switches by camera
    /// distance (`unit_lod`).
    pub lower_lods: Vec<(Vec<VariantVertex>, Vec<u16>)>,
}

/// A soldier assembled from one variant file: one pick per category.
#[derive(Debug, Clone, Default)]
pub struct SoldierModel {
    pub parts: Vec<SoldierPart>,
}

/// Categories left out of the assembled figure (PROVISIONAL, see module docs).
pub fn category_is_drawn(category: &str) -> bool {
    !category.eq_ignore_ascii_case("equipment_secondary_weapon")
}

/// The LOD blocks other than `drawn`, most detailed first (by vertex count; in 287 of 297
/// files that is the file order, which is what the exe indexes, see [`unit_lod`]).
fn lower_lods(
    lods: &[crate::unit_variant::VariantPartMeshLod],
    drawn: &crate::unit_variant::VariantPartMeshLod,
    format: VariantVertexFormat,
) -> Vec<(Vec<VariantVertex>, Vec<u16>)> {
    let mut rest: Vec<_> = lods.iter().filter(|l| !std::ptr::eq(*l, drawn)).collect();
    rest.sort_by_key(|l| std::cmp::Reverse(l.vertex_count));
    rest.into_iter().map(|l| (l.decode_vertices(format), l.indices.clone())).collect()
}

/// The soldier LOD switch distances (m): the exe's tweaks `variant_lod1/2/3` = 5, 10, 15
/// (CONFIRMED, `VariantModelManager.cpp` registrations at `0x00444420..0x004444F8`).
pub const UNIT_LOD_DISTANCES: [f32; 3] = [5.0, 10.0, 15.0];

/// Which LOD of a part mesh with `lod_count` LODs the exe draws at `distance_sq` (squared
/// camera distance, m², `0x0125ED60` with `override_variant_lod` on, CONFIRMED): LOD 3 from
/// 15 m when there are 4, LOD 2 from 10 m when there are at least 3, LOD 1 from 5 m when there
/// are at least 2, else LOD 0.
pub fn unit_lod(distance_sq: f32, lod_count: usize) -> usize {
    let [l1, l2, l3] = UNIT_LOD_DISTANCES;
    if l3 * l3 <= distance_sq && lod_count > 3 {
        3
    } else if l2 * l2 <= distance_sq && lod_count > 2 {
        2
    } else if l1 * l1 <= distance_sq && lod_count > 1 {
        1
    } else {
        0
    }
}

/// Small deterministic hash used for per-man picks (PROVISIONAL stand-in for the game's
/// UNKNOWN selection).
fn pick(seed: u64, salt: usize, n: usize) -> usize {
    let mut x = seed ^ (salt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    (x % n.max(1) as u64) as usize
}

impl SoldierModel {
    /// Assembles one man from a variant file. `seed` picks one entry per category and the
    /// face texture. Parts whose files fail to load are skipped (and listed in the error
    /// vector) so one bad file does not hide the whole man.
    pub fn assemble(
        vfs: &Vfs,
        variant: &UnitVariant,
        equipment: &EquipmentLibrary,
        seed: u64,
    ) -> (Self, Vec<String>) {
        let mut parts = Vec::new();
        let mut problems = Vec::new();
        // Attachment points published by parts (name -> bone, matrix), for plumes.
        let mut attachments: HashMap<String, (u32, [f32; 16])> = HashMap::new();
        let mut deferred: Vec<(String, &UnitVariantMeshRef)> = Vec::new();
        for (ci, cat) in variant.categories.iter().enumerate() {
            let Some(refs) = variant.category_meshes(ci) else { continue };
            if refs.is_empty() || !category_is_drawn(&cat.name) {
                continue;
            }
            let r = &refs[pick(seed, ci, refs.len())];
            if r.kind == 1 {
                match equipment.find(&r.mesh) {
                    Some((piece, stem)) => parts.push(SoldierPart {
                        category: cat.name.to_ascii_lowercase(),
                        source: r.mesh.clone(),
                        binding: PartBinding::Bone(piece.bone.unwrap_or(0)),
                        vertices: piece.lod.decode_vertices(VariantVertexFormat::Bytes64),
                        indices: piece.lod.indices.clone(),
                        textures: texture_files(vfs, stem, 1),
                    lower_lods: Vec::new(),
                }),
                    None => problems.push(format!("equipment piece {} not found", r.mesh)),
                }
                continue;
            }
            let path = format!("{}.variant_part_mesh", r.mesh);
            let mesh = match vfs.read(&path).map_err(UnitModelError::from).and_then(|b| {
                VariantPartMesh::read(&b).map_err(|error| UnitModelError::Mesh { path: path.clone(), error })
            }) {
                Ok(m) => m,
                Err(e) => {
                    problems.push(e.to_string());
                    continue;
                }
            };
            let VariantPartMeshBody::Part { lods, attachments: points, .. } = &mesh.body else {
                problems.push(format!("{path}: not a part mesh"));
                continue;
            };
            for a in points {
                // File order is row-major with the translation in the last column
                // (INFERRED); transpose into our column-major layout.
                let m = a.matrix;
                let cm = std::array::from_fn(|i| m[(i % 4) * 4 + i / 4]);
                attachments.insert(a.name.to_ascii_lowercase(), (a.bone, cm));
            }
            if mesh.header.vertex_format == VariantVertexFormat::Bytes64 {
                // Unskinned: hangs off an attachment point named after its category.
                deferred.push((cat.name.to_ascii_lowercase(), r));
                continue;
            }
            // Draw the most detailed LOD (the one with most vertices; the file order varies,
            // see the LOD survey in the real-install tests).
            let Some(lod) = lods.iter().max_by_key(|l| l.vertex_count) else { continue };
            parts.push(SoldierPart {
                category: cat.name.to_ascii_lowercase(),
                source: r.mesh.clone(),
                binding: PartBinding::Skinned,
                vertices: lod.decode_vertices(mesh.header.vertex_format),
                indices: lod.indices.clone(),
                textures: texture_files(vfs, &r.texture_stem, 1 + pick(seed, 1000, 16)),
                lower_lods: lower_lods(lods, lod, mesh.header.vertex_format),
            });
        }
        for (category, r) in deferred {
            let Some(&(bone, matrix)) = attachments.get(&category) else {
                problems.push(format!("{}: no attachment point '{category}'", r.mesh));
                continue;
            };
            let path = format!("{}.variant_part_mesh", r.mesh);
            let Ok(mesh) = vfs.read(&path).map_err(UnitModelError::from).and_then(|b| {
                VariantPartMesh::read(&b).map_err(|error| UnitModelError::Mesh { path: path.clone(), error })
            }) else {
                continue;
            };
            if let VariantPartMeshBody::Part { lods, .. } = &mesh.body
                && let Some(lod) = lods.iter().max_by_key(|l| l.vertex_count)
            {
                parts.push(SoldierPart {
                    category,
                    source: r.mesh.clone(),
                    binding: PartBinding::Attachment { bone, matrix },
                    vertices: lod.decode_vertices(mesh.header.vertex_format),
                    indices: lod.indices.clone(),
                    textures: texture_files(vfs, &r.texture_stem, 1),
                    lower_lods: lower_lods(lods, lod, mesh.header.vertex_format),
                });
            }
        }
        (Self { parts }, problems)
    }
}

/// A part posed in model space (file coordinates: left-handed, Y up).
#[derive(Debug, Clone, Default)]
pub struct PosedMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u16>,
}

impl SoldierPart {
    /// Poses this part with model-space bone matrices (from `Anim::world_matrices`).
    /// Skinned vertices: `w * (M_A * p_A) + (1 - w) * (M_B * p_B)`, which is exact for this
    /// format (each influence stores its own bone-local position). Bone indices beyond the
    /// skeleton fall back to the identity.
    pub fn pose(&self, bones: &[[f32; 16]]) -> PosedMesh {
        const ID: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
        let bone = |i: u32| bones.get(i as usize).copied().unwrap_or(ID);
        let rigid = match &self.binding {
            PartBinding::Skinned => None,
            PartBinding::Bone(b) => Some(bone(*b)),
            PartBinding::Attachment { bone: b, matrix } => Some(mat_mul(&bone(*b), matrix)),
        };
        let mut out = PosedMesh { indices: self.indices.clone(), ..Default::default() };
        for v in &self.vertices {
            let (p, n) = match (rigid, v.bones) {
                (Some(m), _) => (transform_point(&m, v.positions[0]), transform_vector(&m, v.normals[0])),
                (None, None) => (v.positions[0], v.normals[0]),
                (None, Some([a, b])) => {
                    let (ma, mb) = (bone(u32::from(a)), bone(u32::from(b)));
                    let w = v.weight;
                    let (pa, pb) = (transform_point(&ma, v.positions[0]), transform_point(&mb, v.positions[1]));
                    let (na, nb) = (transform_vector(&ma, v.normals[0]), transform_vector(&mb, v.normals[1]));
                    let p = [0, 1, 2].map(|k| pa[k] * w + pb[k] * (1.0 - w));
                    let n = if w > 0.999 { na } else { [0, 1, 2].map(|k| na[k] * w + nb[k] * (1.0 - w)) };
                    (p, n)
                }
            };
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            out.positions.push(p);
            out.normals.push(n.map(|x| x / len));
            out.uvs.push(v.uv);
        }
        out
    }
}

/// Applies the uniform colours to a diffuse texel using the colour-mask texel.
///
/// PROVISIONAL: the original shader is UNKNOWN. We treat mask R, G, B as weights for the
/// primary, secondary and tertiary colour and multiply the diffuse by the blended colour:
/// `out = diffuse * lerp(1, colour_k, mask_k)` applied for k = R, G, B in turn.
pub fn tint(diffuse: [u8; 4], mask: [u8; 4], colours: &UniformColours) -> [u8; 4] {
    let mut c = [diffuse[0], diffuse[1], diffuse[2]].map(f32::from);
    for (k, col) in [colours.primary, colours.secondary, colours.tertiary].iter().enumerate() {
        let m = f32::from(mask[k]) / 255.0;
        for ch in 0..3 {
            let f = 1.0 + (f32::from(col[ch]) / 255.0 - 1.0) * m;
            c[ch] *= f;
        }
    }
    [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, diffuse[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_paths_and_roles() {
        assert_eq!(
            unit_variant_path("France_Inf_Line_French_Fusiliers", VariantRole::StandardBearer),
            "variantmodels/units/france_inf_line_french_fusiliers.standard_bearer.unit_variant"
        );
    }

    #[test]
    fn tint_leaves_unmasked_texels_alone() {
        let c = UniformColours { primary: [255, 0, 0], secondary: [0, 255, 0], tertiary: [0, 0, 255] };
        assert_eq!(tint([200, 100, 50, 255], [0, 0, 0, 0], &c), [200, 100, 50, 255]);
        assert_eq!(tint([200, 100, 50, 7], [255, 0, 0, 0], &c), [200, 0, 0, 7]);
    }

    #[test]
    fn lod_switch_distances() {
        assert_eq!(unit_lod(4.9 * 4.9, 4), 0);
        assert_eq!(unit_lod(25.0, 4), 1);
        assert_eq!(unit_lod(120.0, 4), 2);
        assert_eq!(unit_lod(15.0 * 15.0, 4), 3);
        assert_eq!(unit_lod(1.0e6, 3), 2, "no LOD 3 to switch to");
        assert_eq!(unit_lod(1.0e6, 1), 0);
    }

    #[test]
    fn pose_blends_two_bones() {
        let t = |x: f32| [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., x, 0., 0., 1.];
        let bones = [t(0.0), t(2.0)];
        let part = SoldierPart {
            category: "legs".into(),
            source: String::new(),
            binding: PartBinding::Skinned,
            vertices: vec![VariantVertex {
                positions: [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]],
                normals: [[0.0, 1.0, 0.0]; 2],
                tangent: [0.0; 3],
                binormal: [0.0; 3],
                uv: [0.0; 2],
                bones: Some([0, 1]),
                weight: 0.5,
            }],
            indices: vec![],
            textures: TextureFiles::default(),
            lower_lods: Vec::new(),
        };
        let posed = part.pose(&bones);
        assert_eq!(posed.positions[0], [1.0, 0.0, 0.0]);
        assert_eq!(posed.normals[0], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn picks_are_in_range() {
        for s in 0..100 {
            assert!(pick(s, 3, 7) < 7);
        }
        assert_eq!(pick(5, 0, 1), 0);
    }
}

/// One `warscape_equipment_themes` row (schema str, ostr, ostr, bool, ostr, ostr; parses
/// to EOF). Column meanings INFERRED from the values: weapon sets, a flag, the carried
/// kit set (`french_infantry_equipment`) and a musical instrument set.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EquipmentTheme {
    pub key: String,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub flag: bool,
    pub ambient: Option<String>,
    pub instrument: Option<String>,
}

/// `warscape_equipment_themes` + `warscape_equipment_items` (item, set).
///
/// A unit's soldier theme is `unit_stats_land` column #10 (values such as
/// `france_musket` equal theme keys: INFERRED from that match; ntw_data calls the column
/// `weapon_anim_group`). Item names map to container pieces as
/// `rigid_equip_<item>[NN]_lodK` (INFERRED from the names).
#[derive(Debug, Clone, Default)]
pub struct EquipmentThemes {
    themes: HashMap<String, EquipmentTheme>,
    items_by_set: HashMap<String, Vec<String>>,
}

impl EquipmentThemes {
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, UnitModelError> {
        let opt = |r: &[DbValue], i: usize| match r.get(i) {
            Some(DbValue::OptStr(s)) => s.clone().filter(|s| !s.is_empty()),
            _ => None,
        };
        let mut themes = HashMap::new();
        for r in read_table(vfs, "warscape_equipment_themes", "s,o,o,b,o,o")?.rows {
            let t = EquipmentTheme {
                key: cell(&r, 0),
                primary: opt(&r, 1),
                secondary: opt(&r, 2),
                flag: r.get(3).and_then(DbValue::as_bool).unwrap_or(false),
                ambient: opt(&r, 4),
                instrument: opt(&r, 5),
            };
            themes.insert(t.key.to_ascii_lowercase(), t);
        }
        let mut items_by_set: HashMap<String, Vec<String>> = HashMap::new();
        for r in read_table(vfs, "warscape_equipment_items", "s,s")?.rows {
            items_by_set.entry(cell(&r, 1).to_ascii_lowercase()).or_default().push(cell(&r, 0));
        }
        Ok(Self { themes, items_by_set })
    }

    pub fn theme(&self, key: &str) -> Option<&EquipmentTheme> {
        self.themes.get(&key.to_ascii_lowercase())
    }

    /// Items of an equipment set (e.g. `euro_musket` -> [`euro_musket`]).
    pub fn items(&self, set: &str) -> &[String] {
        self.items_by_set.get(&set.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// The equipment sets a man of this theme carries, with the category name we file
    /// them under. PROVISIONAL: the secondary weapon is left out (sheathed/unknown).
    pub fn drawn_sets(theme: &EquipmentTheme) -> Vec<(&'static str, &str)> {
        Self::displayed_sets(theme, &[])
    }

    /// The sets shown under an animation fragment's `default_equipment_display` list
    /// (`primary_weapon`, `secondary_weapon`, `ambient`, `personal`; see
    /// `battle_animation`). The theme's instrument column is filed as `personal`
    /// (INFERRED: drummer, bugler and standard-bearer fragments display only `personal`,
    /// and their themes put the drum kit / bugle / flagpole in that column). An empty
    /// list means primary + ambient + instrument (PROVISIONAL default).
    pub fn displayed_sets<'a>(theme: &'a EquipmentTheme, display: &[String]) -> Vec<(&'static str, &'a str)> {
        let all = [
            ("primary_weapon", "equipment_primary_weapon", &theme.primary),
            ("secondary_weapon", "equipment_secondary_weapon", &theme.secondary),
            ("ambient", "equipment_ambient", &theme.ambient),
            ("personal", "equipment_personal", &theme.instrument),
        ];
        let shown = |name: &str| {
            if display.is_empty() {
                name != "secondary_weapon"
            } else {
                display.iter().any(|d| d.eq_ignore_ascii_case(name))
            }
        };
        all.into_iter()
            .filter(|(name, _, _)| shown(name))
            .filter_map(|(_, cat, set)| set.as_deref().map(|s| (cat, s)))
            .collect()
    }
}

impl EquipmentLibrary {
    /// Most-detailed pieces for an item: `rigid_equip_<item>` with an optional two-digit
    /// number, preferring `_lod1`, then no suffix. Sorted by name.
    pub fn pieces_for_item(&self, item: &str) -> Vec<(&EquipmentPiece, &'static str)> {
        let prefix = format!("rigid_equip_{}", item.to_ascii_lowercase());
        let mut names: Vec<&String> = self
            .by_name
            .keys()
            .filter(|n| {
                n.strip_prefix(&prefix).is_some_and(|rest| {
                    let rest = rest.strip_suffix("_lod1").unwrap_or(rest);
                    rest.is_empty() || (rest.len() == 2 && rest.bytes().all(|b| b.is_ascii_digit()))
                })
            })
            .collect();
        names.sort();
        // Drop a bare name when the same piece also exists with `_lod1`.
        let mut out = Vec::new();
        for n in &names {
            if !n.ends_with("_lod1") && names.iter().any(|m| m.as_str() == format!("{n}_lod1")) {
                continue;
            }
            let (piece, container) = &self.pieces[self.by_name[*n]];
            out.push((piece, EQUIPMENT_CONTAINERS[*container].1));
        }
        out
    }
}

impl SoldierModel {
    /// Replaces the variant's equipment (kind-1 entries, many of which name pieces that
    /// no longer exist) with the DB equipment theme's items. PROVISIONAL: which source the
    /// exe uses is UNKNOWN; the theme route is the only one that resolves for every unit.
    pub fn equip_from_theme(
        &mut self,
        vfs: &Vfs,
        themes: &EquipmentThemes,
        theme: &EquipmentTheme,
        equipment: &EquipmentLibrary,
        display: &[String],
        seed: u64,
    ) -> Vec<String> {
        let mut problems = Vec::new();
        self.parts.retain(|p| !p.category.starts_with("equipment"));
        for (si, (category, set)) in EquipmentThemes::displayed_sets(theme, display).into_iter().enumerate() {
            for (ii, item) in themes.items(set).iter().enumerate() {
                let pieces = equipment.pieces_for_item(item);
                if pieces.is_empty() {
                    problems.push(format!("no piece for item {item} (set {set})"));
                    continue;
                }
                let (piece, stem) = pieces[pick(seed, 2000 + si * 100 + ii, pieces.len())];
                self.parts.push(SoldierPart {
                    category: category.to_owned(),
                    source: piece.name.clone(),
                    binding: PartBinding::Bone(piece.bone.unwrap_or(0)),
                    vertices: piece.lod.decode_vertices(VariantVertexFormat::Bytes64),
                    indices: piece.lod.indices.clone(),
                    textures: texture_files(vfs, stem, 1),
                    lower_lods: Vec::new(),
                });
            }
        }
        problems
    }
}

/// One `battle_personalities` row (schema str x5, parses to EOF). Column meanings
/// INFERRED from the values: `euro_drummer`, `euroline`, `personality_drummer`,
/// `generic_drummer`, `drummer`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BattlePersonality {
    pub key: String,
    /// `euroline`, `Napoleon_BattleOutfit` ... (UNKNOWN use: a model/culture set).
    pub model_set: String,
    /// Animation table (`animations\animation_tables\animation_tables.txt`).
    pub animation_table: String,
    /// `warscape_equipment_themes` key.
    pub equipment_theme: String,
    /// `officer`, `drummer`, `bugler`, `flutist`, `standard_bearer`, `admiral` ...
    pub role: String,
}

/// One `battle_entities` row (schema s,s,s,f x10,s,f x6,i). Only the columns we use are
/// named; meanings INFERRED from the values (worker2 schemas.md: infantry walk 1.4 m/s,
/// run 3.6; horse_heavy walk 2.6, run 10, charge 11.5).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BattleEntity {
    pub key: String,
    /// `infantry`, `cavalry`, `artillery` ...
    pub class: String,
    /// `man`, `horse`, `camel` ...
    pub skeleton: String,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub charge_speed: f32,
    /// Column 12: the entity's radius in metres (men 0.35, horses 2.0, guns 1.5; INFERRED from the
    /// values; the garrison slot spacing reads it, `0x00E619D0`).
    pub radius: f32,
}

/// `battle_personalities` + `battle_entities`, by lower-case key.
#[derive(Debug, Clone, Default)]
pub struct BattleTables {
    personalities: HashMap<String, BattlePersonality>,
    entities: HashMap<String, BattleEntity>,
}

impl BattleTables {
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, UnitModelError> {
        let mut personalities = HashMap::new();
        for r in read_table(vfs, "battle_personalities", "s,s,s,s,s")?.rows {
            let p = BattlePersonality {
                key: cell(&r, 0),
                model_set: cell(&r, 1),
                animation_table: cell(&r, 2),
                equipment_theme: cell(&r, 3),
                role: cell(&r, 4),
            };
            personalities.insert(p.key.to_ascii_lowercase(), p);
        }
        let mut entities = HashMap::new();
        let f = |r: &[DbValue], i: usize| r.get(i).and_then(DbValue::as_f32).unwrap_or(0.0);
        for r in read_table(vfs, "battle_entities", "s,s,s,f,f,f,f,f,f,f,f,f,f,s,f,f,f,f,f,f,i")?.rows {
            let e = BattleEntity {
                key: cell(&r, 0),
                class: cell(&r, 1),
                skeleton: cell(&r, 2),
                walk_speed: f(&r, 3),
                run_speed: f(&r, 4),
                charge_speed: f(&r, 7),
                radius: f(&r, 12),
            };
            entities.insert(e.key.to_ascii_lowercase(), e);
        }
        Ok(Self { personalities, entities })
    }

    pub fn personality(&self, key: &str) -> Option<&BattlePersonality> {
        self.personalities.get(&key.to_ascii_lowercase())
    }

    pub fn entity(&self, key: &str) -> Option<&BattleEntity> {
        self.entities.get(&key.to_ascii_lowercase())
    }

    /// All entities.
    pub fn entities(&self) -> impl Iterator<Item = &BattleEntity> {
        self.entities.values()
    }
}

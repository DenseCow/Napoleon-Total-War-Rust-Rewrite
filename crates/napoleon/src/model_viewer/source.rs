//! Finding models and their textures inside the install (read-only), and turning a
//! parsed `.rigid_model` into plain vertex arrays a renderer can use.
//!
//! Nothing here depends on Bevy, so it can be unit-tested without a window.

use std::collections::HashMap;

use ntw_formats::db::DbValue;
use ntw_formats::db_folder::{RawTable, tables};
use ntw_formats::pack::{Vfs, normalize_path};
use ntw_formats::rigid_model::{Material, Mesh as RigidMesh};

/// The install's packs plus the lookup tables needed to find a model's textures.
pub struct ModelSource {
    /// All packs, layered in game load order.
    pub vfs: Vfs,
    /// Normalized model path -> its `warscape_rigid` key (from `warscape_rigid_lod`).
    lod_key: HashMap<String, String>,
    /// `warscape_rigid` key (lowercase) -> texture folder.
    folders: HashMap<String, String>,
    /// Lowercase texture file stem (no folder, no `.dds`) -> full path. Last resort.
    dds_by_stem: HashMap<String, String>,
}

/// Where a texture came from, for the on-screen/log report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureHit {
    /// Found via the DB texture folder for this model (the game's own route, INFERRED).
    DbFolder(String),
    /// Found next to the model (`<model dir>\textures\...` and similar guesses).
    NearModel(String),
    /// Found only by searching every `.dds` for the same file name.
    Search(String),
}

impl TextureHit {
    /// The VFS path that was found.
    pub fn path(&self) -> &str {
        match self {
            Self::DbFolder(p) | Self::NearModel(p) | Self::Search(p) => p,
        }
    }
}

impl ModelSource {
    /// Opens the install's `data` folder and reads the two `warscape_rigid` DB tables.
    /// Missing tables are not fatal (texture lookup then falls back to guessing).
    pub fn open(data_dir: &std::path::Path) -> Result<Self, String> {
        let vfs = Vfs::open_install(data_dir).map_err(|e| format!("cannot open {}: {e}", data_dir.display()))?;
        let mut lod_key = HashMap::new();
        let mut folders = HashMap::new();
        // Column layouts from analysis/worker2/schemas.md (all strings), CONFIRMED by parsing.
        if let Some(rows) = read_table(&vfs, &tables::WARSCAPE_RIGID_LOD) {
            for row in &rows {
                if let (Some(path), Some(key)) = (row[1].as_str(), row[3].as_str()) {
                    lod_key.insert(normalize_path(path), key.to_ascii_lowercase());
                }
            }
        }
        if let Some(rows) = read_table(&vfs, &tables::WARSCAPE_RIGID) {
            for row in &rows {
                if let (Some(key), Some(folder)) = (row[0].as_str(), row[1].as_str()) {
                    folders.insert(key.to_ascii_lowercase(), folder.to_owned());
                }
            }
        }
        let mut dds_by_stem = HashMap::new();
        for p in vfs.list("") {
            if let Some(stem) = p.strip_suffix(".dds") {
                let name = stem.rsplit('\\').next().unwrap_or(stem);
                dds_by_stem.entry(name.to_owned()).or_insert_with(|| p.to_owned());
            }
        }
        Ok(Self { vfs, lod_key, folders, dds_by_stem })
    }

    /// Resolves what the user typed to a `.rigid_model` path: an exact VFS path, or
    /// else the first `.rigid_model` whose path contains the text (so `american_church`
    /// finds `..._piece01_destruct01_lod01.rigid_model`).
    pub fn find_model(&self, query: &str) -> Option<String> {
        if self.vfs.contains(query) {
            return Some(normalize_path(query));
        }
        let q = normalize_path(query);
        let mut hits: Vec<&str> =
            self.vfs.list("").into_iter().filter(|p| p.ends_with(".rigid_model") && p.contains(&q)).collect();
        // Prefer the most detailed level of detail of the first piece.
        hits.sort_by_key(|p| (!p.contains("lod01"), !p.contains("destruct01"), p.len(), p.to_string()));
        hits.first().map(|s| (*s).to_owned())
    }

    /// Every `.rigid_model` path containing `query` (normalized).
    pub fn list_models(&self, query: &str) -> Vec<&str> {
        let q = normalize_path(query);
        self.vfs.list("").into_iter().filter(|p| p.ends_with(".rigid_model") && p.contains(&q)).collect()
    }

    /// The DB texture folder for a model, if the `warscape_rigid*` tables list it.
    pub fn db_texture_folder(&self, model_path: &str) -> Option<&str> {
        let key = self.lod_key.get(&normalize_path(model_path))?;
        self.folders.get(key).map(String::as_str)
    }

    /// Finds a texture by name (no extension) for a model.
    ///
    /// Order: a name that already contains a folder is used as is; then the DB
    /// folder; then folders near the model; then any `.dds` with the same file name.
    pub fn find_texture(&self, model_path: &str, name: &str) -> Option<TextureHit> {
        if name.is_empty() {
            return None;
        }
        let file = format!("{name}.dds");
        if name.contains(['\\', '/']) && self.vfs.contains(&file) {
            return Some(TextureHit::DbFolder(normalize_path(&file)));
        }
        if let Some(folder) = self.db_texture_folder(model_path) {
            let p = format!("{folder}\\{file}");
            if self.vfs.contains(&p) {
                return Some(TextureHit::DbFolder(normalize_path(&p)));
            }
        }
        let model = normalize_path(model_path);
        let mut dir = model.rsplit_once('\\').map(|(d, _)| d.to_owned()).unwrap_or_default();
        for _ in 0..3 {
            for sub in ["textures", "aaa_textures", ""] {
                let p = if sub.is_empty() { format!("{dir}\\{file}") } else { format!("{dir}\\{sub}\\{file}") };
                if self.vfs.contains(&p) {
                    return Some(TextureHit::NearModel(normalize_path(&p)));
                }
            }
            match dir.rsplit_once('\\') {
                Some((parent, _)) => dir = parent.to_owned(),
                None => break,
            }
        }
        let stem = name.rsplit(['\\', '/']).next().unwrap_or(name).to_ascii_lowercase();
        self.dds_by_stem.get(&stem).map(|p| TextureHit::Search(p.clone()))
    }
}

/// A table through the merged table reader; a missing or unreadable one is logged (the viewer
/// then guesses textures).
fn read_table(vfs: &Vfs, table: &RawTable) -> Option<Vec<Vec<DbValue>>> {
    table.read(vfs).map_err(|e| eprintln!("WARN model viewer: {e}")).ok()
}

/// Render flags the game derives from the model's **file name** (CONFIRMED, exe 0x011D9D80).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NameFlags {
    /// `_alphatest` in the name: cut-out transparency.
    pub alpha_test: bool,
    /// `_alphablend` in the name: blended transparency.
    pub alpha_blend: bool,
    /// `_twosided` in the name: draw back faces too.
    pub two_sided: bool,
}

impl NameFlags {
    /// Reads the flags from a model path.
    pub fn from_path(path: &str) -> Self {
        let p = path.to_ascii_lowercase();
        Self { alpha_test: p.contains("_alphatest"), alpha_blend: p.contains("_alphablend"), two_sided: p.contains("_twosided") }
    }
}

/// One mesh converted to a right-handed, Y-up coordinate system (Bevy's).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshArrays {
    /// Positions.
    pub positions: Vec<[f32; 3]>,
    /// Normals.
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates.
    pub uvs: Vec<[f32; 2]>,
    /// Tangents with handedness in `w` (Bevy's convention).
    pub tangents: Vec<[f32; 4]>,
    /// Triangle list.
    pub indices: Vec<u32>,
}

/// Converts a rigid mesh: D3D is left-handed, Bevy right-handed, so Z is negated.
/// In both systems a triangle's front is the side its edge cross product
/// `(b - a) x (c - a)` points to (D3D: clockwise + left-handed; Bevy:
/// counter-clockwise + right-handed). Mirroring Z reverses that cross product, so
/// each triangle's 2nd and 3rd index are swapped to keep the same side in front.
pub fn convert_mesh(mesh: &RigidMesh) -> MeshArrays {
    let flip = |v: [f32; 3]| [v[0], v[1], -v[2]];
    let mut out = MeshArrays::default();
    for tri in mesh.indices.as_chunks::<3>().0.iter() {
        out.indices.extend_from_slice(&[tri[0], tri[2], tri[1]]);
    }
    for v in &mesh.vertices {
        let n = flip(v.normal);
        let t = flip(v.tangent);
        let b = flip(v.binormal);
        out.positions.push(flip(v.position));
        out.normals.push(n);
        out.uvs.push(v.uv);
        // w = +1 if (n x t) points along b, else -1.
        let c = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
        let w = if c[0] * b[0] + c[1] * b[1] + c[2] * b[2] < 0.0 { -1.0 } else { 1.0 };
        out.tangents.push([t[0], t[1], t[2], w]);
    }
    out
}

/// Short description of a material for the log.
pub fn describe_material(m: &Material) -> String {
    format!(
        "diffuse {:?}, normal {:?}, gloss {:?}{}",
        m.diffuse_name().unwrap_or_default(),
        m.normal_name().unwrap_or_default(),
        m.gloss_name().unwrap_or_default(),
        if m.float_params.is_empty() { String::new() } else { format!(", {} shader constants", m.float_params.len()) }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::rigid_model::Vertex;

    #[test]
    fn convert_flips_z_and_keeps_winding() {
        let v = |x: f32, z: f32| Vertex {
            position: [x, 0.0, z],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, 0.0],
            binormal: [0.0, 0.0, 1.0],
            ..Default::default()
        };
        // In D3D this triangle faces +Y: (b-a) x (c-a) = (0,0,1) x (1,0,0) = (0,1,0).
        let mesh = RigidMesh { vertices: vec![v(0.0, 0.0), v(0.0, 1.0), v(1.0, 0.0)], indices: vec![0, 1, 2], ..Default::default() };
        let a = convert_mesh(&mesh);
        assert_eq!(a.positions[1], [0.0, 0.0, -1.0]);
        assert_eq!(a.indices, [0, 2, 1]);
        // n x t = (0,1,0) x (1,0,0) = (0,0,-1); flipped binormal is (0,0,-1): same direction.
        assert_eq!(a.tangents[0], [1.0, 0.0, 0.0, 1.0]);
        // After conversion the triangle must still face +Y (like its normal).
        let [i0, i1, i2] = [0, 1, 2].map(|k| a.indices[k] as usize);
        let (p0, p1, p2) = (a.positions[i0], a.positions[i1], a.positions[i2]);
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let ny = e1[2] * e2[0] - e1[0] * e2[2]; // y component of e1 x e2
        assert!(ny > 0.0);
    }

    #[test]
    fn name_flags() {
        let f = NameFlags::from_path(r"RigidModels\x\fence_lod05_ALPHATEST_twosided.rigid_model");
        assert!(f.alpha_test && f.two_sided && !f.alpha_blend);
    }
}

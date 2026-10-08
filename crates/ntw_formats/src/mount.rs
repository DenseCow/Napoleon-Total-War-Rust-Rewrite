//! Mounts (horses, camels): from a unit's `mount` key to a posable weighted mesh.
//!
//! The chain (`analysis/units/CAVALRY.md` §2; CONFIRMED by
//! `tests/real_install.rs::every_mounted_unit_resolves_to_horse_and_rider`):
//! ```text
//! unit_stats_land #14 mount ("horse_hussar_mixed")              (FK mounts)
//!   -> mount_variants (mount, model key, weight)                (one or more colours)
//!        "horse_hussar_mixed" -> horse_hussar_dark_brown 1, horse_hussar_black 1, ...
//!   -> warscape_animated (model key, texture stem, "animation")
//!        horse_hussar_black -> "UnitModels/horse/textures/horse"  (+ _diffuse/_normal/_gloss_map.dds)
//!   -> warscape_animated_lod (id, mesh path, distance, model key)  (one row per LOD)
//!        horse_hussar_black -> UnitModels\horse\horse_C_hussar_lod1..4.variant_weighted_mesh
//! ```
//! The coat colour is a UV offset into the shared horse texture: the six letter models
//! A..F (light brown, grey, black, dark brown, white, brown) have the same geometry and
//! different texture coordinates (CONFIRMED for A vs C by a UV survey). The saddle type
//! (basic, covered, hussar, iberian, artillery) is part of the file name.
//!
//! A mesh file holds every optional piece (two manes `hat01/02`, three head markings
//! `head01..03`, a body, a saddle, stirrups, artillery harness). We draw one piece per
//! group (PROVISIONAL: how the game picks is UNKNOWN; the identical head geometry with
//! different UVs suggests one head per horse).

use std::collections::HashMap;

use crate::db::{DbTable, DbValue, Schema};
use crate::pack::{PackError, Vfs};
use crate::unit_model::{TextureFiles, texture_files};
use crate::weighted_mesh::{WeightedMesh, WeightedMeshError, WeightedPiece};

/// Why mount data could not be loaded.
#[derive(Debug)]
pub enum MountError {
    Pack(PackError),
    Db { table: &'static str, error: crate::db::DbError },
    UnknownMount(String),
    UnknownModel(String),
    NoLod(String),
    Mesh { path: String, error: WeightedMeshError },
}

impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(e) => write!(f, "{e}"),
            Self::Db { table, error } => write!(f, "{table}: {error}"),
            Self::UnknownMount(m) => write!(f, "mount {m} has no mount_variants row"),
            Self::UnknownModel(m) => write!(f, "model {m} has no warscape_animated row"),
            Self::NoLod(m) => write!(f, "model {m} has no warscape_animated_lod rows"),
            Self::Mesh { path, error } => write!(f, "{path}: {error}"),
        }
    }
}

impl std::error::Error for MountError {}

impl From<PackError> for MountError {
    fn from(e: PackError) -> Self {
        Self::Pack(e)
    }
}

fn read_table(vfs: &Vfs, table: &'static str, codes: &str) -> Result<DbTable, MountError> {
    let bytes = vfs.read(&format!("db/{table}_tables/{table}"))?;
    let schema = Schema::from_codes(codes).expect("valid schema codes");
    DbTable::read(&bytes, &schema).map_err(|error| MountError::Db { table, error })
}

fn text(row: &[DbValue], i: usize) -> String {
    row.get(i).and_then(DbValue::as_str).unwrap_or_default().to_owned()
}

/// A `warscape_animated` model with its LOD meshes.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimatedModel {
    pub key: String,
    /// Texture stem (`UnitModels/horse/textures/horse`).
    pub texture_stem: String,
    /// `animation`, `campaign_animation`, `campaign_building`.
    pub kind: String,
    /// (mesh path, distance) sorted most detailed first. INFERRED: the distance is the
    /// switch distance of the LOD and 0 means "beyond the last" (lod1 10, lod2 20,
    /// lod3 50, lod4 0 for every horse).
    pub lods: Vec<(String, f32)>,
}

impl AnimatedModel {
    /// The most detailed mesh path.
    pub fn best_lod(&self) -> Option<&str> {
        self.lods.first().map(|(p, _)| p.as_str())
    }
}

/// `mount_variants` + `warscape_animated` + `warscape_animated_lod`.
#[derive(Debug, Clone, Default)]
pub struct MountIndex {
    /// lower mount -> (model key, weight), in table order.
    variants: HashMap<String, Vec<(String, f32)>>,
    /// lower model key -> model.
    models: HashMap<String, AnimatedModel>,
}

impl MountIndex {
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, MountError> {
        let mut variants: HashMap<String, Vec<(String, f32)>> = HashMap::new();
        for r in read_table(vfs, "mount_variants", "s,s,f")?.rows {
            let w = r.get(2).and_then(DbValue::as_f32).unwrap_or(1.0);
            variants.entry(text(&r, 0).to_ascii_lowercase()).or_default().push((text(&r, 1), w));
        }
        let mut models: HashMap<String, AnimatedModel> = HashMap::new();
        for r in read_table(vfs, "warscape_animated", "s,s,s")?.rows {
            let key = text(&r, 0);
            models.insert(
                key.to_ascii_lowercase(),
                AnimatedModel { key, texture_stem: text(&r, 1), kind: text(&r, 2), lods: Vec::new() },
            );
        }
        for r in read_table(vfs, "warscape_animated_lod", "s,s,f,s")?.rows {
            if let Some(m) = models.get_mut(&text(&r, 3).to_ascii_lowercase()) {
                m.lods.push((text(&r, 1), r.get(2).and_then(DbValue::as_f32).unwrap_or(0.0)));
            }
        }
        for m in models.values_mut() {
            let order = |d: f32| if d <= 0.0 { f32::MAX } else { d };
            m.lods.sort_by(|a, b| order(a.1).total_cmp(&order(b.1)).then_with(|| a.0.cmp(&b.0)));
        }
        Ok(Self { variants, models })
    }

    /// Mount keys that have variants.
    pub fn mounts(&self) -> impl Iterator<Item = &str> {
        self.variants.keys().map(String::as_str)
    }

    /// The colour variants of a mount: (model key, weight).
    pub fn variants(&self, mount: &str) -> &[(String, f32)] {
        self.variants.get(&mount.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    pub fn model(&self, key: &str) -> Option<&AnimatedModel> {
        self.models.get(&key.to_ascii_lowercase())
    }

    /// Picks a variant by weight with a caller seed (PROVISIONAL: the game's RNG use is
    /// UNKNOWN; the weights are the table's).
    pub fn pick_variant(&self, mount: &str, seed: u64) -> Option<&str> {
        let v = self.variants(mount);
        let total: f32 = v.iter().map(|(_, w)| w.max(0.0)).sum();
        if v.is_empty() || total <= 0.0 {
            return v.first().map(|(k, _)| k.as_str());
        }
        let mut x = (mix(seed, 77) % 1_000_000) as f32 / 1_000_000.0 * total;
        for (k, w) in v {
            if x < w.max(0.0) {
                return Some(k);
            }
            x -= w.max(0.0);
        }
        v.last().map(|(k, _)| k.as_str())
    }
}

fn mix(seed: u64, salt: u64) -> u64 {
    let mut x = seed ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    x
}

/// The group a piece belongs to: its name without a `emp_horse_<X>_` prefix and without
/// trailing digits and underscores (`emp_horse_A_head02` -> `head`, `stirrups_light_1` ->
/// `stirrups_light`, `BASIC_SADDLE` -> `basic_saddle`).
pub fn piece_group(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let mut s = lower.as_str();
    if let Some(rest) = s.strip_prefix("emp_horse_")
        && let Some((_, tail)) = rest.split_once('_')
    {
        s = tail;
    }
    s.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_').to_owned()
}

/// A mount ready to pose: the chosen pieces and the texture files.
#[derive(Debug, Clone)]
pub struct MountModel {
    /// The `warscape_animated` key (`horse_hussar_black`).
    pub model_key: String,
    /// The mesh file used.
    pub mesh_path: String,
    pub pieces: Vec<WeightedPiece>,
    pub textures: TextureFiles,
    /// The less detailed LODs, same picks (one entry per piece of `pieces`), with the
    /// `warscape_animated_lod` distance of each LOD including the first: `lod_distances[0]` is
    /// the most detailed LOD's (see [`animated_lod`]). Empty when a LOD file's pieces do not
    /// match the first's.
    pub lower_lods: Vec<Vec<WeightedPiece>>,
    pub lod_distances: Vec<f32>,
}

/// Which `warscape_animated_lod` LOD an animated model draws at `distance` (m): the first
/// whose distance is 0 (no limit) or at least the camera distance × `lod_distance_scaler` (1.0).
/// CONFIRMED loop (`0x01146B20`); whether the compared value is metres or squared metres on both
/// sides is INFERRED (the same either way when both are squared).
pub fn animated_lod(distances: &[f32], distance: f32) -> usize {
    distances.iter().position(|&d| d <= 0.0 || distance <= d).unwrap_or(distances.len().saturating_sub(1))
}

fn pick_pieces(mesh: WeightedMesh, seed: u64) -> Vec<WeightedPiece> {
    let mut groups: Vec<(String, Vec<WeightedPiece>)> = Vec::new();
    for p in mesh.pieces {
        let g = piece_group(&p.name);
        match groups.iter_mut().find(|(k, _)| *k == g) {
            Some((_, v)) => v.push(p),
            None => groups.push((g, vec![p])),
        }
    }
    groups
        .into_iter()
        .enumerate()
        .map(|(gi, (_, mut v))| {
            v.sort_by_key(|p| p.name.to_ascii_lowercase());
            let i = (mix(seed, 100 + gi as u64) % v.len() as u64) as usize;
            v.swap_remove(i)
        })
        .collect()
}

impl MountModel {
    /// Assembles a mount for a `mounts` key: a weighted colour variant, its most detailed
    /// LOD, one piece per group (seeded); the other LODs with the same picks.
    pub fn assemble(vfs: &Vfs, index: &MountIndex, mount: &str, seed: u64) -> Result<Self, MountError> {
        let key = index.pick_variant(mount, seed).ok_or_else(|| MountError::UnknownMount(mount.into()))?;
        let model = index.model(key).ok_or_else(|| MountError::UnknownModel(key.into()))?;
        let path = model.best_lod().ok_or_else(|| MountError::NoLod(key.into()))?.to_owned();
        let mesh = WeightedMesh::read(&vfs.read(&path)?).map_err(|error| MountError::Mesh { path: path.clone(), error })?;
        let pieces = pick_pieces(mesh, seed);
        let mut lower_lods = Vec::new();
        for (p, _) in model.lods.iter().skip(1) {
            let Some(m) = vfs.read(p).ok().and_then(|b| WeightedMesh::read(&b).ok()) else { break };
            let picked = pick_pieces(m, seed);
            if picked.len() != pieces.len() {
                lower_lods.clear();
                break;
            }
            lower_lods.push(picked);
        }
        let lod_distances = model.lods.iter().take(1 + lower_lods.len()).map(|(_, d)| *d).collect();
        Ok(Self {
            model_key: model.key.clone(),
            mesh_path: path,
            pieces,
            textures: texture_files(vfs, &model.texture_stem, 1),
            lower_lods,
            lod_distances,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animated_lod_picks_the_first_range_that_covers() {
        let d = [10.0, 20.0, 50.0, 0.0];
        assert_eq!(animated_lod(&d, 5.0), 0);
        assert_eq!(animated_lod(&d, 15.0), 1);
        assert_eq!(animated_lod(&d, 50.0), 2);
        assert_eq!(animated_lod(&d, 500.0), 3);
        assert_eq!(animated_lod(&[10.0], 500.0), 0);
    }

    #[test]
    fn groups_pieces() {
        assert_eq!(piece_group("emp_horse_A_head02"), "head");
        assert_eq!(piece_group("emp_horse_B_hat01"), "hat");
        assert_eq!(piece_group("emp_horse_A_body"), "body");
        assert_eq!(piece_group("emp_horse_A_body01"), "body");
        assert_eq!(piece_group("BASIC_SADDLE"), "basic_saddle");
        assert_eq!(piece_group("stirrups_light_1"), "stirrups_light");
        assert_eq!(piece_group("Harness_1"), "harness");
    }

    #[test]
    fn weighted_pick_covers_all_variants() {
        let mut idx = MountIndex::default();
        idx.variants.insert("m".into(), vec![("a".into(), 1.0), ("b".into(), 1.0), ("c".into(), 0.0)]);
        let picks: std::collections::HashSet<&str> = (0..64).filter_map(|s| idx.pick_variant("M", s)).collect();
        assert!(picks.contains("a") && picks.contains("b"));
        assert!(!picks.contains("c"));
        assert_eq!(idx.pick_variant("none", 0), None);
    }
}

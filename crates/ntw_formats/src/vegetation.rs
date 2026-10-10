//! Battle vegetation: which SpeedTree a tree-list species uses, its size, and its pictures in
//! the climate's composite textures.
//!
//! Notes: `analysis/worker5/BATTLE_TERRAIN.md` §9. Tags: CONFIRMED (checked on every shipped
//! file), INFERRED, UNKNOWN.
//!
//! ```text
//! bmd.tree_list species ("lc_tundra-alaskacedar_tree")          (battle_terrain::TreeGroup)
//!   -> DB warscape_trees (species, season, path)                 366 rows, s,s,s
//!        path "\lc_tundra\lc_tundra-AlaskaCedar_TREE.spt", under rigidmodels\vegetation\battle
//!   -> <climate>\data.tree_model: one record per .spt (bounds)   (TreeModelFile)
//!   -> <climate>\<name>_compositemap.txt: per .spt the UV rects  (CompositeMap)
//!        of its leaf and frond cards in textures\<name>_diffuse.dds and of its 360° billboard
//!        pictures in textures\<name>_diffuse_billboards.dds
//! ```
//!
//! The `.spt` files themselves are SpeedTree 4 tree descriptions (`"__IdvSpt_02_"`, tagged
//! tokens with Bezier-spline parameters, CONFIRMED by their header). The branch, frond and leaf
//! geometry is generated from them at run time by the SpeedTree library, which we do not have, so
//! the near-LOD tree geometry is UNKNOWN to us; the billboards are the trees' own pictures.

use std::collections::HashMap;

use crate::db::DbValue;
use crate::pack::Vfs;

/// Folder that `warscape_trees` paths are relative to (CONFIRMED: every path resolves there).
pub const VEGETATION_DIR: &str = r"rigidmodels\vegetation\battle";

/// One record of a `data.tree_model` file.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeModelRecord {
    /// The `.spt` path, as written (e.g. `\lc_tundra\lc_tundra-AlaskaCedar_TREE.spt`).
    pub path: String,
    /// The 14 floats after the path (CONFIRMED count). `[8..11]` / `[11..14]` are INFERRED to be a
    /// bounding box min / max in the game's Y-up axes: the vertical extent is symmetric on most
    /// records (311 of 345, `-h..h`), so the box looks centred; `[5]` grows with the height
    /// (INFERRED: the centre height). `[0..5]`, `[6]`, `[7]` are UNKNOWN.
    pub values: [f32; 14],
}

impl TreeModelRecord {
    /// Bounding box minimum `[x, y, z]` (INFERRED, see [`values`](Self::values)).
    pub fn bounds_min(&self) -> [f32; 3] {
        [self.values[8], self.values[9], self.values[10]]
    }

    /// Bounding box maximum `[x, y, z]` (INFERRED).
    pub fn bounds_max(&self) -> [f32; 3] {
        [self.values[11], self.values[12], self.values[13]]
    }

    /// Height of the tree in metres: the vertical extent of the box (INFERRED).
    pub fn height(&self) -> f32 {
        self.values[12] - self.values[9]
    }

    /// Horizontal radius: the largest horizontal half-extent of the box (INFERRED).
    pub fn radius(&self) -> f32 {
        [self.values[8], self.values[10], self.values[11], self.values[13]].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }
}

/// A whole `data.tree_model`: `f32` (UNKNOWN, ~0.05), `u32` version (1), `u32` record count, then
/// records `{ u16 n, n × UTF-16 path, 14 × f32 }`. CONFIRMED to the last byte on all 11 files
/// (test `every_tree_model_parses`). Records repeat (the same `.spt` appears in several groups).
#[derive(Debug, Clone, PartialEq)]
pub struct TreeModelFile {
    /// The leading float (UNKNOWN).
    pub unknown: f32,
    /// The version (1 in every file).
    pub version: u32,
    /// Records in file order.
    pub records: Vec<TreeModelRecord>,
}

impl TreeModelFile {
    /// Parses a `data.tree_model`.
    pub fn read(b: &[u8]) -> Result<Self, String> {
        let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        let f32_at = |o: usize| u32_at(o).map(f32::from_bits);
        let unknown = f32_at(0).ok_or("short header")?;
        let version = u32_at(4).ok_or("short header")?;
        let count = u32_at(8).ok_or("short header")? as usize;
        let mut o = 12;
        let mut records = Vec::with_capacity(count);
        for i in 0..count {
            let n = b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize).ok_or(format!("record {i}: end of file"))?;
            o += 2;
            let units: Vec<u16> = b
                .get(o..o + 2 * n)
                .ok_or(format!("record {i}: path past end"))?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            o += 2 * n;
            let path = String::from_utf16(&units).map_err(|e| format!("record {i}: {e}"))?;
            let mut values = [0.0f32; 14];
            for (k, v) in values.iter_mut().enumerate() {
                *v = f32_at(o + 4 * k).ok_or(format!("record {i}: values past end"))?;
            }
            o += 56;
            records.push(TreeModelRecord { path, values });
        }
        if o != b.len() {
            return Err(format!("{} bytes left after {count} records", b.len() - o));
        }
        Ok(Self { unknown, version, records })
    }

    /// The record of `.spt` file name `file` (case-insensitive, file name only).
    pub fn record(&self, file: &str) -> Option<&TreeModelRecord> {
        self.records.iter().find(|r| file_name(&r.path).eq_ignore_ascii_case(file_name(file)))
    }
}

/// A UV rectangle `[u_min, v_min, u_max, v_max]` in texture space (v = 0 at the TOP row of the DDS).
/// The composite-map files store v with 0 at the BOTTOM (OpenGL style); [`read_composite_map`] flips
/// it. CONFIRMED: only the flipped rectangles each frame exactly one picture of the billboard atlas
/// (e.g. `lc_boreal_compositemap_diffuse_billboards.dds`, whose rows hold 5 or 7 pictures each).
pub type UvRect = [f32; 4];

/// One `.spt` block of a composite-map file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompositeEntry {
    /// The `.spt` file name, as written.
    pub spt: String,
    /// Leaf card rectangles in `<name>_diffuse.dds`.
    pub leaves: Vec<UvRect>,
    /// Frond rectangles in `<name>_diffuse.dds`.
    pub fronds: Vec<UvRect>,
    /// 360° billboard pictures in `<name>_diffuse_billboards.dds`, in file order. Trees have 8
    /// (CONFIRMED on the files checked), shrubs none (CONFIRMED: empty `Billboards` section), so
    /// shrubs are drawn only as geometry in the original. Each picture covers 360°/n of view
    /// directions; which direction picture 0 shows is UNKNOWN.
    pub billboards: Vec<UvRect>,
}

/// Parses a `*_compositemap.txt` (text, CONFIRMED layout): a line `<file>.spt`, then the section
/// names `Leaves`, `Fronds`, `Billboards`, each followed by lines of 8 comma-separated numbers
/// (four `u, v` corners: `(u1,v1) (u0,v1) (u0,v0) (u1,v0)`).
pub fn read_composite_map(text: &str) -> Vec<CompositeEntry> {
    let mut out: Vec<CompositeEntry> = Vec::new();
    let mut section = 0u8;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.to_ascii_lowercase().ends_with(".spt") {
            out.push(CompositeEntry { spt: line.to_owned(), ..Default::default() });
            section = 0;
            continue;
        }
        match line {
            "Leaves" => section = 1,
            "Fronds" => section = 2,
            "Billboards" => section = 3,
            _ => {
                let v: Vec<f32> = line.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                let (Some(entry), true) = (out.last_mut(), v.len() == 8) else { continue };
                let us = [v[0], v[2], v[4], v[6]];
                // The file counts v from the bottom of the texture; flip to top-down texture space.
                let vs = [1.0 - v[1], 1.0 - v[3], 1.0 - v[5], 1.0 - v[7]];
                let min = |a: [f32; 4]| a.iter().copied().fold(f32::MAX, f32::min);
                let max = |a: [f32; 4]| a.iter().copied().fold(f32::MIN, f32::max);
                let rect = [min(us), min(vs), max(us), max(vs)];
                match section {
                    1 => entry.leaves.push(rect),
                    2 => entry.fronds.push(rect),
                    3 => entry.billboards.push(rect),
                    _ => {}
                }
            }
        }
    }
    out
}

/// Everything needed to draw one species.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeAppearance {
    /// Species key from the tree list.
    pub species: String,
    /// Full pack path of the `.spt`.
    pub spt_path: String,
    /// Its `data.tree_model` record, if any.
    pub model: Option<TreeModelRecord>,
    /// Its composite-map block.
    pub composite: CompositeEntry,
    /// `<folder>\textures\<map>_diffuse.dds` (leaf and frond cards).
    pub diffuse_texture: String,
    /// `<folder>\textures\<map>_diffuse_billboards.dds`.
    pub billboard_texture: String,
}

/// One `warscape_trees` row.
#[derive(Debug, Clone, PartialEq)]
pub struct WarscapeTree {
    /// Species key (e.g. `lc_tundra-AlaskaCedar_tree`).
    pub species: String,
    /// Season key (`season_summer`, `season_winter`, ...).
    pub season: String,
    /// `.spt` path relative to [`VEGETATION_DIR`].
    pub path: String,
}

/// `warscape_trees` plus lazily parsed climate folders.
#[derive(Debug, Default)]
pub struct VegetationIndex {
    /// All `warscape_trees` rows.
    pub trees: Vec<WarscapeTree>,
    folders: HashMap<String, Folder>,
}

#[derive(Debug, Default)]
struct Folder {
    model: Option<TreeModelFile>,
    /// (composite map stem, its entries)
    maps: Vec<(String, Vec<CompositeEntry>)>,
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

impl VegetationIndex {
    /// Reads `warscape_trees` (schema `s,s,s`, CONFIRMED: 366 rows decode exactly).
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, String> {
        let rows = crate::db_folder::tables::WARSCAPE_TREES.read(vfs).map_err(|e| e.to_string())?;
        let cell = |r: &[DbValue], i: usize| r.get(i).and_then(DbValue::as_str).unwrap_or_default().to_owned();
        let trees = rows
            .iter()
            .map(|r| WarscapeTree { species: cell(r, 0), season: cell(r, 1), path: cell(r, 2) })
            .collect();
        Ok(Self { trees, folders: HashMap::new() })
    }

    /// The `warscape_trees` row for a tree-list `species` in `season` (a map's `definition.xml`
    /// season, e.g. `season_winter`; a bare `winter` is accepted too).
    ///
    /// The tree lists do not always use the DB key: `lc_boreal-christmasscotchpine_tree` is the
    /// lower-case file stem of the row `lc_boreal-Chrsitmas_Scotch_Pine_tree` (sic) whose
    /// summer path is `\lc_boreal\lc_boreal-ChristmasScotchPine_TREE.spt` (CONFIRMED on every
    /// preset: each species matches a DB key or a `.spt` stem, test `every_tree_species_resolves`).
    /// So a species matches a row by key, else by the file stem of any of its rows' paths
    /// (INFERRED: the editor wrote the stem). Season: the row of that key in `season`, else
    /// `season_summer`, else any (PROVISIONAL: the original's fallback is UNKNOWN).
    pub fn row(&self, species: &str, season: &str) -> Option<&WarscapeTree> {
        let season = if season.starts_with("season_") { season.to_owned() } else { format!("season_{season}") };
        let stem = |t: &WarscapeTree| {
            let f = file_name(&t.path);
            f.get(..f.len().saturating_sub(4)).unwrap_or(f).to_owned()
        };
        let key = self
            .trees
            .iter()
            .find(|t| t.species.eq_ignore_ascii_case(species))
            .or_else(|| self.trees.iter().find(|t| stem(t).eq_ignore_ascii_case(species)))?
            .species
            .clone();
        let of = |s: &str| self.trees.iter().find(|t| t.species == key && t.season.eq_ignore_ascii_case(s));
        of(&season).or_else(|| of("season_summer")).or_else(|| self.trees.iter().find(|t| t.species == key))
    }

    fn folder(&mut self, vfs: &Vfs, dir: &str) -> &Folder {
        self.folders.entry(dir.to_ascii_lowercase()).or_insert_with(|| {
            let model = vfs.read(&format!("{dir}\\data.tree_model")).ok().and_then(|b| TreeModelFile::read(&b).ok());
            let prefix = format!("{dir}\\");
            let mut maps = Vec::new();
            for p in vfs.list(&prefix) {
                let Some(stem) = p.strip_prefix(&prefix).and_then(|n| n.strip_suffix(".txt")) else { continue };
                if !stem.ends_with("_compositemap") || stem.contains('\\') {
                    continue;
                }
                if let Ok(b) = vfs.read(p) {
                    maps.push((stem.to_owned(), read_composite_map(&String::from_utf8_lossy(&b))));
                }
            }
            Folder { model, maps }
        })
    }

    /// Resolves a tree-list species to its `.spt`, bounds and composite pictures.
    ///
    /// Route: the `warscape_trees` row ([`row`](Self::row)) if its `.spt` exists, else the file
    /// `<climate>\<species>[_<season>].spt` named directly by the species (the climate is the
    /// part before `-`). The direct route is needed for `lc_sand_desert-lowbrush_shrub`, whose
    /// DB row points at the fan palm. Which route the exe takes is UNKNOWN. Species of the
    /// American climates (`lc_am_*`, Empire leftovers on two MP maps) have no files at all
    /// (CONFIRMED) and resolve to `None`.
    pub fn resolve(&mut self, vfs: &Vfs, species: &str, season: &str) -> Option<TreeAppearance> {
        let from_db = self
            .row(species, season)
            .map(|r| format!("{VEGETATION_DIR}\\{}", r.path.trim_start_matches(['\\', '/']).replace('/', "\\")))
            .filter(|p| vfs.contains(p));
        let direct = || {
            let climate = species.split('-').next()?;
            let bare = season.trim_start_matches("season_");
            let base = format!("{VEGETATION_DIR}\\{climate}\\{species}");
            let mut candidates = Vec::new();
            if bare != "summer" {
                candidates.push(format!("{base}_{bare}.spt"));
            }
            candidates.push(format!("{base}.spt"));
            candidates.into_iter().find(|p| vfs.contains(p))
        };
        let spt_path = from_db.or_else(direct)?;
        let dir = spt_path.rsplit_once('\\').map(|(d, _)| d.to_owned())?;
        let file = file_name(&spt_path).to_owned();
        let folder = self.folder(vfs, &dir);
        let model = folder.model.as_ref().and_then(|m| m.record(&file)).cloned();
        let (stem, composite) = folder
            .maps
            .iter()
            .find_map(|(stem, entries)| entries.iter().find(|e| e.spt.eq_ignore_ascii_case(&file)).map(|e| (stem.clone(), e.clone())))?;
        Some(TreeAppearance {
            species: species.to_owned(),
            spt_path,
            model,
            composite,
            diffuse_texture: format!("{dir}\\textures\\{stem}_diffuse.dds"),
            billboard_texture: format!("{dir}\\textures\\{stem}_diffuse_billboards.dds"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_map_blocks() {
        let text = "a_TREE.spt\nLeaves\n0.5, 0.5, 0.25, 0.5, 0.25, 0.25, 0.5, 0.25\nFronds\nBillboards\n\
                    0.2, 0.4, 0.1, 0.4, 0.1, 0.3, 0.2, 0.3\n0.3, 0.4, 0.2, 0.4, 0.2, 0.3, 0.3, 0.3\nb_SHRUB.spt\nLeaves\nFronds\nBillboards\n";
        let m = read_composite_map(text);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].leaves, vec![[0.25, 0.5, 0.5, 0.75]]);
        assert_eq!(m[0].billboards.len(), 2);
        assert_eq!(m[0].billboards[1], [0.2, 0.6, 0.3, 0.7]);
        assert!(m[1].billboards.is_empty());
    }

    #[test]
    fn tree_model_layout() {
        let mut b = Vec::new();
        b.extend(0.05f32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        let path: Vec<u16> = r"\x\x-a_TREE.spt".encode_utf16().collect();
        b.extend((path.len() as u16).to_le_bytes());
        path.iter().for_each(|c| b.extend(c.to_le_bytes()));
        for v in [0.0, 0.0, 0.0, 0.3, 0.0, 5.0, 0.0, 2.0, -2.0, -6.0, -1.5, 2.5, 6.0, 1.5f32] {
            b.extend(v.to_le_bytes());
        }
        let f = TreeModelFile::read(&b).unwrap();
        let r = f.record("X-A_tree.SPT").unwrap();
        assert_eq!(r.height(), 12.0);
        assert_eq!(r.radius(), 2.5);
        assert!(TreeModelFile::read(&b[..b.len() - 1]).is_err());
    }
}

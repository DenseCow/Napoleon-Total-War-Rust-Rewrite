//! The campaign map as the map display draws it (DESIGN.md §3.5.1, MODDING_AUDIT.md §3.2, the
//! `CampaignMap` row): our own type, whatever the campaign's source. The display reads only this;
//! it never opens a map file or knows a map folder's layout. The original's importer
//! ([`crate::source::OriginalSource`]) fills it from `campaign_maps\<map>\`; a generator fills it
//! from an open-format campaign's images.
//!
//! The geometry types below ([`RegionMap`], [`Heightmap`], [`CoastMesh`], [`RigidTrees`]) are plain
//! decoded data (bounds, region triangles and outlines, label and slot positions, a grey height
//! grid, strip meshes, tree positions) that any source can build directly; their file parsers
//! belong to the importer. What still meant a file layout (the supertexture's `.stpd` offsets,
//! spline folders in display units, the map folder's texture and model paths) is behind
//! [`GroundTexture`], [`MapLine`] and the bytes the source hands over.

pub use ntw_formats::campaign_map::{CoastMesh, CoastVertex, GroundTypeRegion, Heightmap, MapRegion, MapSlot, RegionMap, RegionSettlement, RigidTrees};

/// The terrain's colour picture: a pyramid of square tiles covering the map's bounds, level 0 the
/// finest, row 0 the north edge. Read on request (the original's finest level is 32,768 pixels
/// wide), from any thread.
pub trait GroundTexture: Send + Sync {
    /// The tile edge in pixels.
    fn tile_size(&self) -> u32;
    /// Tiles across and down, per level, finest first.
    fn levels(&self) -> &[(u32, u32)];
    /// The RGBA8 pixels (north row first) of the `nx` × `ny` tiles of `level` starting at tile
    /// (`tx0`, `ty0`): `nx · tile_size` pixels wide.
    fn window_rgba(&self, level: usize, tx0: u32, ty0: u32, nx: u32, ny: u32) -> Result<Vec<u8>, String>;
}

/// What a map line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineKind {
    /// A region border.
    Border,
    /// A river.
    River,
    /// A road.
    Road,
    /// A sea trade route.
    TradeRoute,
}

/// One line drawn on the map.
#[derive(Debug, Clone, PartialEq)]
pub struct MapLine {
    /// What it is.
    pub kind: LineKind,
    /// Logic map (x, z) points: 3k + 1 points are k cubic Bezier segments (four control points
    /// each, sharing their ends), any other count a plain polyline.
    pub points: Vec<(f32, f32)>,
}

/// Everything the campaign map display draws.
pub struct MapDisplay {
    /// The map's key (the original's map folder name, e.g. `nap_europe`).
    pub key: String,
    /// The regions: the map and theatre bounds, region triangles and outlines, settlements and
    /// their slots, label positions, trade nodes and ground types.
    pub regions: RegionMap,
    /// The height grid over the map's bounds, north row first.
    pub heightmap: Heightmap,
    /// Logic height units per height-grid step.
    pub height_scale: f32,
    /// The terrain's colour, if the map has one.
    pub ground: Option<Box<dyn GroundTexture>>,
    /// Borders, rivers, roads and trade routes.
    pub lines: Vec<MapLine>,
    /// The surf strips along the coasts.
    pub coast: Vec<CoastMesh>,
    /// The trees, if the map has any.
    pub trees: Option<RigidTrees>,
    /// The rivers' texture (DDS bytes), or why there is none.
    pub river_texture: Result<Vec<u8>, String>,
    /// The movement arrow's model (rigid model bytes; the arrows take its proportions), or why
    /// there is none.
    pub arrow_model: Result<Vec<u8>, String>,
}

impl MapDisplay {
    /// Terrain height (logic units) at logic (x, z), bilinear.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let (mn, mx) = (self.regions.bounds_min, self.regions.bounds_max);
        let u = (x - mn.0) / (mx.0 - mn.0) * self.heightmap.width as f32;
        let v = (mx.1 - z) / (mx.1 - mn.1) * self.heightmap.height as f32;
        self.heightmap.sample(u, v) * self.height_scale
    }

    /// The lines of one kind.
    pub fn lines(&self, kind: LineKind) -> impl Iterator<Item = &MapLine> {
        self.lines.iter().filter(move |l| l.kind == kind)
    }
}

/// A whole level of `ground` as one RGBA8 picture (north row first): `(width, height, pixels)`.
pub fn level_rgba(ground: &dyn GroundTexture, level: usize) -> Result<(u32, u32, Vec<u8>), String> {
    let &(nx, ny) = ground.levels().get(level).ok_or_else(|| format!("no ground texture level {level}"))?;
    let rgba = ground.window_rgba(level, 0, 0, nx, ny)?;
    Ok((nx * ground.tile_size(), ny * ground.tile_size(), rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up ground texture: one level of 2 × 1 tiles, 2 px each, every pixel its tile index.
    struct Made;

    impl GroundTexture for Made {
        fn tile_size(&self) -> u32 {
            2
        }
        fn levels(&self) -> &[(u32, u32)] {
            &[(2, 1)]
        }
        fn window_rgba(&self, _level: usize, tx0: u32, _ty0: u32, nx: u32, ny: u32) -> Result<Vec<u8>, String> {
            let w = (nx * 2) as usize;
            Ok((0..w * (ny * 2) as usize).flat_map(|i| [(tx0 + (i % w) as u32 / 2) as u8; 4]).collect())
        }
    }

    fn made_up_map() -> MapDisplay {
        MapDisplay {
            key: "made_up_map".into(),
            regions: RegionMap {
                bounds_min: (-10.0, -5.0),
                bounds_max: (10.0, 5.0),
                theatre: ((-10.0, -5.0), (10.0, 5.0)),
                labels: Vec::new(),
                vertices: Vec::new(),
                regions: Vec::new(),
                trade_nodes: Vec::new(),
                ground_types: Vec::new(),
            },
            // West half 0, east half 255.
            heightmap: Heightmap { width: 2, height: 1, values: vec![0, 255] },
            height_scale: 0.5,
            ground: Some(Box::new(Made)),
            lines: vec![
                MapLine { kind: LineKind::River, points: vec![(0.0, 0.0), (1.0, 1.0)] },
                MapLine { kind: LineKind::Road, points: vec![(2.0, 0.0), (3.0, 1.0)] },
            ],
            coast: Vec::new(),
            trees: None,
            river_texture: Err("none".into()),
            arrow_model: Err("none".into()),
        }
    }

    /// A map no file was ever read for: heights in its own scale, lines by kind, the ground
    /// texture's whole level.
    #[test]
    fn a_made_up_map_draws_from_its_own_data() {
        let m = made_up_map();
        assert_eq!(m.height_at(-9.0, 0.0), 0.0);
        assert_eq!(m.height_at(9.0, 0.0), 255.0 * 0.5);
        assert_eq!(m.lines(LineKind::Road).map(|l| l.points[0]).collect::<Vec<_>>(), [(2.0, 0.0)]);
        assert_eq!(m.lines(LineKind::Border).count(), 0);
        let (w, h, px) = level_rgba(m.ground.as_deref().expect("ground"), 0).expect("level 0");
        assert_eq!((w, h, px.len()), (4, 2, 4 * 2 * 4));
        assert_eq!(px.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [0, 0, 1, 1, 0, 0, 1, 1]);
        assert!(level_rgba(m.ground.as_deref().expect("ground"), 1).is_err());
    }
}

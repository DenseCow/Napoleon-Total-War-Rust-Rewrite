//! Builds the simulation's movement grid ([`PathGrid`]) from a campaign map's files.
//!
//! **From the original pathfinding data** (`pathfinding.esf`, read by
//! [`ntw_formats::campaign_pathfinding`]; `analysis/campaign/CAMPAIGN_DATA.md` §1):
//! - the grid covers the original's own pathfinding grid (the theatre), in cells of
//!   [`CELL_SIZE`] (half the original's 2-unit cells, so every movement cell lies in one
//!   original cell);
//! - a cell's kind comes from the original polygon under its centre: land where the polygon's
//!   region set is a land region and its kind is land or road, sea where the set is the sea, and
//!   blocked where the polygon is off-map (region id 1023: mountains, the land outside the
//!   theatre) (CONFIRMED data, INFERRED meaning of the flags);
//! - a cell is a road cell if any original road polygon (flags kind 6/7: the strips along the
//!   roads and the settlement octagons) overlaps it;
//! - the region of a cell is the polygon's region (for the shared strips along borders, the
//!   `regions.esf` region under the centre).
//!
//! The grid also carries the original's polygon map ([`poly_map`], `PathGrid::poly`): paths come
//! from the original's search over it (`ntw_sim::campaign::polypath`,
//! `analysis/campaign/PATHFINDING.md`). The raster serves region lookups. Without
//! `pathfinding.esf` (or if it does not read) the old raster of `regions.esf` triangles and road
//! splines is used instead, with our PROVISIONAL raster A*.

use ntw_formats::campaign_map::{CampaignMap, DISPLAY_TO_LOGIC};
use ntw_formats::campaign_pathfinding::{point_in_polygon, GridCell, PathfindingArea, PathfindingFile, PolygonKind};
use ntw_sim::campaign::pathing::{CellKind, PathGrid};
use ntw_sim::campaign::polypath::{CellInput, PolyInput, PolyMap};

/// Grid cell size in logic map units: half of the original's 2-unit cell (CONFIRMED `grid_data`
/// cell size 2.0 on every map). PROVISIONAL choice of subdivision.
pub const CELL_SIZE: f32 = 1.0;

/// Builds the movement grid: from `pathfinding.esf` when the map has it, else from `regions.esf`.
pub fn build_grid(map: &CampaignMap) -> PathGrid {
    let mut g = match map.pathfinding.as_ref().and_then(|pf| grid_from_pathfinding(pf, map)) {
        Some(g) => g,
        None => build_grid_from_regions(map),
    };
    g.harbours = harbours(map);
    g.ground_types = map.regions.ground_types.iter().map(|t| ntw_sim::campaign::pathing::GroundType { key: t.key.clone(), areas: t.areas.clone() }).collect();
    g
}

/// The ports' harbours (`ntw_sim::campaign::embark::Harbour`) from the map's `port` slots: the
/// slot position, its dock point and its third footprint outline.
pub fn harbours(map: &CampaignMap) -> Vec<ntw_sim::campaign::embark::Harbour> {
    map.regions
        .regions
        .iter()
        .filter_map(|r| r.settlement.as_ref())
        .flat_map(|s| s.slots.iter())
        .filter(|s| s.slot_type == "port")
        .map(|s| ntw_sim::campaign::embark::Harbour {
            port: (s.position.0, s.position.2),
            dock: s.dock,
            outline: s.footprints.get(2).or(s.footprints.last()).cloned().unwrap_or_default(),
        })
        .collect()
}

/// The movement grid from the original pathfinding polygons (see the module docs). `None` if the
/// file has no area or its region table is malformed.
pub fn grid_from_pathfinding(pf: &PathfindingFile, map: &CampaignMap) -> Option<PathGrid> {
    let area = pf.areas.first()?;
    let pg = &area.grid;
    let sets = pg.region_sets()?;
    let rm = &map.regions;
    let sub = (pg.cell_units() / CELL_SIZE).round().max(1.0) as u32;
    let (width, height) = (pg.cols * sub, pg.rows * sub);
    let mut g = PathGrid::new(pg.origin_units(), CELL_SIZE, width, height);
    g.region_keys = rm.regions.iter().map(|r| r.key.clone()).collect();
    let cells = pg.expand();
    let w = pg.cols as usize;
    for (ci, cell) in cells.iter().enumerate() {
        let polys = area.cell_polygons(ci, cell);
        let (pc, pr) = ((ci % w) as u32, (ci / w) as u32);
        for sr in 0..sub {
            for sc in 0..sub {
                let i = ((pr * sub + sr) * width + pc * sub + sc) as usize;
                let (x, z) = g.centre(i);
                let hit = if polys.len() == 1 {
                    polys.first()
                } else {
                    polys.iter().find(|(_, p)| point_in_polygon(p, x, z))
                };
                let Some((b, _)) = hit else { continue };
                let rid = usize::from(b.region());
                let set = sets.get(rid);
                let (kind, region) = match (b.kind(), set) {
                    (PolygonKind::OffMap, _) | (_, None) => (CellKind::Blocked, None),
                    (_, Some(s)) if s.is_empty() => (CellKind::Sea, None),
                    (PolygonKind::Land | PolygonKind::Road, Some(s)) => {
                        let r = if s.len() == 1 {
                            Some(s[0] as usize)
                        } else {
                            rm.region_at(x, z).and_then(|r| rm.regions.iter().position(|q| q.key == r.key))
                        };
                        (CellKind::Land, r.or(Some(s[0] as usize)))
                    }
                    (PolygonKind::Water, Some(_)) => (CellKind::Sea, None),
                    (PolygonKind::Other, Some(_)) => (CellKind::Blocked, None),
                };
                g.kind[i] = kind as u8;
                if let Some(r) = region {
                    g.region[i] = r as u16;
                }
                // Road: any road polygon of this original cell overlapping the movement cell.
                if kind == CellKind::Land {
                    let half = CELL_SIZE * 0.5;
                    let sq = (x - half, z - half, x + half, z + half);
                    g.road[i] = polys.iter().any(|(b, p)| b.kind() == PolygonKind::Road && overlaps_square(p, sq));
                }
            }
        }
    }
    g.poly = Some(poly_map(area, &cells, &sets));
    Some(g)
}

/// The original's polygon map for its path search ([`ntw_sim::campaign::polypath`]): every
/// cell's header bytes and polygons with exact Fixed20 outlines (corner indices 0..3 = the cell's
/// SW, NW, SE, NE corners; a cell with a single boundary is the whole square).
pub fn poly_map(area: &PathfindingArea, cells: &[GridCell], sets: &[Vec<i16>]) -> PolyMap {
    let g = &area.grid;
    let w = g.cols.max(1) as usize;
    let cs = g.cell_size;
    let inputs: Vec<CellInput> = cells
        .iter()
        .enumerate()
        .map(|(ci, cell)| {
            let (x0, z0) = (g.origin.0 + (ci % w) as i32 * cs, g.origin.1 + (ci / w) as i32 * cs);
            let corner = [(x0, z0), (x0, z0 + cs), (x0 + cs, z0), (x0 + cs, z0 + cs)];
            let polys = cell
                .boundaries
                .iter()
                .filter_map(|&b| {
                    let outline: Vec<(i32, i32)> = if cell.boundaries.len() == 1 {
                        vec![corner[0], corner[2], corner[3], corner[1]]
                    } else {
                        let off = b.list_offset();
                        let n = *area.lists.get(off)? as usize;
                        let ids = area.lists.get(off + 1..off + 1 + n)?;
                        ids.iter()
                            .map(|&v| if v < 4 { Some(corner[v as usize]) } else { area.vertices.get(v as usize).copied() })
                            .collect::<Option<_>>()?
                    };
                    Some(PolyInput { kind: (b.flags & 0xF) as u8, region_id: b.region(), outline })
                })
                .collect();
            CellInput { header: cell.header, polys }
        })
        .collect();
    let region_sets = sets.iter().map(|s| s.iter().filter(|&&r| r >= 0).map(|&r| r as u16).collect()).collect();
    PolyMap::build(g.origin, cs, g.cols, g.rows, &inputs, region_sets)
}

/// True if polygon `p` overlaps the axis-aligned square (x0, z0, x1, z1).
fn overlaps_square(p: &[(f32, f32)], sq: (f32, f32, f32, f32)) -> bool {
    let (x0, z0, x1, z1) = sq;
    if p.iter().any(|&(x, z)| x >= x0 && x <= x1 && z >= z0 && z <= z1) {
        return true;
    }
    let corners = [(x0, z0), (x1, z0), (x1, z1), (x0, z1)];
    if corners.iter().any(|&(x, z)| point_in_polygon(p, x, z)) {
        return true;
    }
    let edges = [(corners[0], corners[1]), (corners[1], corners[2]), (corners[2], corners[3]), (corners[3], corners[0])];
    let mut j = p.len().wrapping_sub(1);
    for i in 0..p.len() {
        let (a, b) = (p[j], p[i]);
        if edges.iter().any(|&(c, d)| segments_cross(a, b, c, d)) {
            return true;
        }
        j = i;
    }
    false
}

fn segments_cross(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    let o = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// The fallback grid: rasterises the map's `regions.esf` triangles and road splines over
/// `regions.esf`'s bounds (PROVISIONAL, used only without `pathfinding.esf`).
pub fn build_grid_from_regions(map: &CampaignMap) -> PathGrid {
    let rm = &map.regions;
    let (mn, mx) = (rm.bounds_min, rm.bounds_max);
    let width = ((mx.0 - mn.0) / CELL_SIZE).ceil().max(1.0) as u32;
    let height = ((mx.1 - mn.1) / CELL_SIZE).ceil().max(1.0) as u32;
    let mut g = PathGrid::new(mn, CELL_SIZE, width, height);
    g.region_keys = rm.regions.iter().map(|r| r.key.clone()).collect();

    let v = |i: u32| rm.vertices.get(i as usize).copied().unwrap_or_default();
    for (ri, region) in rm.regions.iter().enumerate() {
        let kind = if region.is_sea { CellKind::Sea } else { CellKind::Land } as u8;
        for area in &region.areas {
            for t in area.faces.chunks_exact(3) {
                let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
                let lo = (a.0.min(b.0).min(c.0), a.1.min(b.1).min(c.1));
                let hi = (a.0.max(b.0).max(c.0), a.1.max(b.1).max(c.1));
                let c0 = (((lo.0 - mn.0) / CELL_SIZE).floor().max(0.0)) as u32;
                let r0 = (((lo.1 - mn.1) / CELL_SIZE).floor().max(0.0)) as u32;
                let c1 = (((hi.0 - mn.0) / CELL_SIZE).floor() as u32).min(width - 1);
                let r1 = (((hi.1 - mn.1) / CELL_SIZE).floor() as u32).min(height - 1);
                for row in r0..=r1 {
                    for col in c0..=c1 {
                        let i = (row * width + col) as usize;
                        let (x, z) = g.centre(i);
                        if inside(a, b, c, x, z) {
                            g.kind[i] = kind;
                            g.region[i] = ri as u16;
                        }
                    }
                }
            }
        }
    }

    // Roads: every spline of the map's `roads` folder, in logic units (display x 39.37).
    for (folder, spline) in &map.splines {
        if folder != "roads" {
            continue;
        }
        let pts: Vec<(f32, f32)> =
            spline.points.iter().map(|p| (p[0] * DISPLAY_TO_LOGIC, p[2] * DISPLAY_TO_LOGIC)).collect();
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let n = (len / (CELL_SIZE * 0.5)).ceil().max(1.0) as usize;
            for k in 0..=n {
                let t = k as f32 / n as f32;
                if let Some(i) = g.cell_at(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
                    && g.kind[i] == CellKind::Land as u8
                {
                    g.road[i] = true;
                }
            }
        }
    }
    g
}

/// Point-in-triangle (either winding), as `RegionMap::region_at`.
fn inside(a: (f32, f32), b: (f32, f32), c: (f32, f32), x: f32, z: f32) -> bool {
    let s = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (z - p.1) - (q.1 - p.1) * (x - p.0);
    let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_overlap() {
        let tri = [(0.0, 0.0), (4.0, 0.0), (0.0, 4.0)];
        assert!(overlaps_square(&tri, (1.0, 1.0, 2.0, 2.0)));
        assert!(!overlaps_square(&tri, (3.0, 3.0, 4.0, 4.0)));
        // A thin strip crossing the square without a vertex inside.
        let strip = [(-5.0, 0.4), (5.0, 0.4), (5.0, 0.6), (-5.0, 0.6)];
        assert!(overlaps_square(&strip, (0.0, 0.0, 1.0, 1.0)));
    }
}

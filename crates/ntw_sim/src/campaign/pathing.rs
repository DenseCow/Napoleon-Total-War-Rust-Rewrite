//! Campaign map movement: a grid over the map and an A* path search.
//!
//! The grid's data comes from the original: `ntw_campaign::pathing` builds it from the
//! polygons of `campaign_maps\<map>\pathfinding.esf` (land, sea, off-map, road strips; see
//! `analysis/campaign/CAMPAIGN_DATA.md` §1), with the `regions.esf` raster as a fallback:
//! - land / sea / blocked per cell;
//! - road cells;
//! - the region of each cell, so a road cell costs the region's road level
//!   (`road_level_<n>_action_point_cost`, CONFIRMED keys, INFERRED meaning).
//!
//! When the map has `pathfinding.esf`, [`PathGrid::poly`] holds the original's polygon map and
//! paths come from the original's search ([`super::polypath`], CONFIRMED algorithm). The raster
//! below stays for region lookups and as the fallback without that file.
//!
//! **PROVISIONAL (fallback only):** 8-neighbour A* on integer costs (milli action points), ties
//! broken by cell index, so the result is the same on every machine. Diagonal steps may not cut
//! corners.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// What a cell is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CellKind {
    /// Outside every region (off the map), never passable.
    Blocked = 0,
    /// A land region.
    Land = 1,
    /// A sea region.
    Sea = 2,
}

/// Who is moving: armies and agents walk on land, navies sail on sea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Domain {
    /// Land cells only.
    Land,
    /// Sea cells only.
    Sea,
}

/// The movement grid. Cell `(col, row)` covers logic x in `[origin.0 + col*cell, +cell)` and
/// logic z in `[origin.1 + row*cell, +cell)` (z = north, as in the startpos).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathGrid {
    /// Logic (x, z) of the south-west corner of cell (0, 0).
    pub origin: (f32, f32),
    /// Cell size in logic map units.
    pub cell: f32,
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// [`CellKind`] per cell, row-major.
    pub kind: Vec<u8>,
    /// True if a road spline crosses the cell.
    pub road: Vec<bool>,
    /// Index into [`PathGrid::region_keys`] per cell: its land or sea region (`u32::MAX` = none;
    /// the director finds a human's fleets in a sea area by it). `u32` so a map may have
    /// any number of regions (the original's `pathfinding.esf` stores 10-bit ids per polygon).
    pub region: Vec<u32>,
    /// Region keys (e.g. `eur_france`) indexed by [`PathGrid::region`].
    pub region_keys: Vec<String>,
    /// The original's polygon map (`pathfinding.esf`): when present, paths come from the
    /// original's search ([`super::polypath`]) instead of the raster A* below.
    pub poly: Option<super::polypath::PolyMap>,
    /// The ports' harbours from the map (`regions.esf` port slots), for the way out of a port
    /// ([`super::embark::Harbour`]).
    pub harbours: Vec<super::embark::Harbour>,
    /// The map's ground types (`regions.esf` `groundtypes`), for the stealth test
    /// ([`PathGrid::ground_type_at`]).
    pub ground_types: Vec<GroundType>,
    /// Cells of each region (ascending cell index), built on first use by
    /// [`PathGrid::nearest_cell_per_region`]. Derived from [`PathGrid::region`]: fill `region` and
    /// `region_keys` before the first lookup, or call [`PathGrid::invalidate_region_cells`] after
    /// changing them.
    region_cells: RegionCells,
}

/// Lazy per-region cell lists; equal for any two grids (derived data, not part of the identity).
#[derive(Debug, Default)]
struct RegionCells(std::sync::OnceLock<Vec<Vec<usize>>>);

/// A clone starts empty and rebuilds on its first lookup.
impl Clone for RegionCells {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for RegionCells {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// One area of a ground type: its (outer?, outline points) loops.
pub type GroundArea = Vec<(bool, Vec<(f32, f32)>)>;

/// One campaign ground type's polygons (`regions.esf` `groundtypes`; the types partition the map).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroundType {
    /// The `campaign_ground_types` key.
    pub key: String,
    /// Areas, each a list of (outer?, outline points) loops; a point is in an area when it is
    /// inside an outer loop and in none of the holes.
    pub areas: Vec<GroundArea>,
}

fn point_in_loop(pts: &[(f32, f32)], x: f32, z: f32) -> bool {
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.1 > z) != (b.1 > z) && x < (b.0 - a.0) * (z - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl GroundType {
    /// Whether (x, z) is on this ground type.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        self.areas.iter().any(|a| {
            a.iter().any(|(outer, p)| *outer && point_in_loop(p, x, z)) && !a.iter().any(|(outer, p)| !*outer && point_in_loop(p, x, z))
        })
    }
}

/// A path found by [`PathGrid::find_path`].
#[derive(Debug, Clone, PartialEq)]
pub struct GridPath {
    /// Points along the path (logic x, z), starting at the start position.
    pub points: Vec<(f32, f32)>,
    /// Action points needed to reach each point (same length as `points`, `costs[0] == 0`).
    pub costs: Vec<f32>,
    /// The original polygon of each point (from [`super::polypath`]); empty for raster paths.
    pub polys: Vec<u32>,
}

impl GridPath {
    /// The total action point cost of the whole path.
    pub fn total_cost(&self) -> f32 {
        self.costs.last().copied().unwrap_or(0.0)
    }

    /// The index of the farthest point reachable with `points` action points.
    pub fn reachable(&self, points: f32) -> usize {
        self.costs.iter().rposition(|&c| c <= points + 1e-3).unwrap_or(0)
    }
}

/// Integer cost scale: 1 action point = 1000 cost units.
const MILLI: f32 = 1000.0;

impl PathGrid {
    /// An empty grid of `width` x `height` blocked cells.
    pub fn new(origin: (f32, f32), cell: f32, width: u32, height: u32) -> Self {
        let n = (width * height) as usize;
        PathGrid {
            origin,
            cell,
            width,
            height,
            kind: vec![CellKind::Blocked as u8; n],
            road: vec![false; n],
            region: vec![u32::MAX; n],
            region_keys: Vec::new(),
            poly: None,
            harbours: Vec::new(),
            ground_types: Vec::new(),
            region_cells: RegionCells::default(),
        }
    }

    /// The ground type key at (x, z) (the original's ground lookup `0x00A88EB0`), if the map has
    /// ground types and the point is on one.
    pub fn ground_type_at(&self, x: f32, z: f32) -> Option<&str> {
        self.ground_types.iter().find(|g| g.contains(x, z)).map(|g| g.key.as_str())
    }

    /// The cell holding logic point (x, z), if it is on the grid.
    pub fn cell_at(&self, x: f32, z: f32) -> Option<usize> {
        let c = ((x - self.origin.0) / self.cell).floor();
        let r = ((z - self.origin.1) / self.cell).floor();
        if c < 0.0 || r < 0.0 || c >= self.width as f32 || r >= self.height as f32 {
            return None;
        }
        Some(r as usize * self.width as usize + c as usize)
    }

    /// The centre of a cell in logic (x, z).
    pub fn centre(&self, i: usize) -> (f32, f32) {
        let w = self.width as usize;
        (
            self.origin.0 + ((i % w) as f32 + 0.5) * self.cell,
            self.origin.1 + ((i / w) as f32 + 0.5) * self.cell,
        )
    }

    /// The kind of a cell.
    pub fn kind_of(&self, i: usize) -> CellKind {
        match self.kind.get(i) {
            Some(1) => CellKind::Land,
            Some(2) => CellKind::Sea,
            _ => CellKind::Blocked,
        }
    }

    /// The region key of the cell under (x, z), if any.
    pub fn region_at(&self, x: f32, z: f32) -> Option<&str> {
        let i = self.cell_at(x, z)?;
        self.region_keys.get(*self.region.get(i)? as usize).map(String::as_str)
    }

    fn passable(&self, i: usize, domain: Domain) -> bool {
        matches!(
            (self.kind_of(i), domain),
            (CellKind::Land, Domain::Land) | (CellKind::Sea, Domain::Sea)
        )
    }

    /// The nearest passable cell to (x, z) within `radius` cells (ring by ring, lowest index first
    /// within a ring), so a position on a coast or a settlement just off the grid still works.
    pub fn nearest_passable(&self, x: f32, z: f32, domain: Domain, radius: i32) -> Option<usize> {
        let c0 = ((x - self.origin.0) / self.cell).floor() as i32;
        let r0 = ((z - self.origin.1) / self.cell).floor() as i32;
        for d in 0..=radius {
            let mut best: Option<usize> = None;
            for dr in -d..=d {
                for dc in -d..=d {
                    if dr.abs().max(dc.abs()) != d {
                        continue;
                    }
                    let (c, r) = (c0 + dc, r0 + dr);
                    if c < 0 || r < 0 || c >= self.width as i32 || r >= self.height as i32 {
                        continue;
                    }
                    let i = r as usize * self.width as usize + c as usize;
                    if self.passable(i, domain) && best.is_none_or(|b| i < b) {
                        best = Some(i);
                    }
                }
            }
            if best.is_some() {
                return best;
            }
        }
        None
    }

    /// The index of region `key` in [`PathGrid::region_keys`], if the grid has it.
    pub fn region_index(&self, key: &str) -> Option<u32> {
        self.region_keys.iter().position(|k| k == key).and_then(|i| u32::try_from(i).ok())
    }

    /// For each region index, the cell of that region nearest to (x, z) by the squared distance to
    /// its centre (ties: the lowest cell index), whatever its kind; `None` for a region with no cell.
    /// Scans only each region's own cells (lists built once, on first use).
    pub fn nearest_cell_per_region(&self, x: f32, z: f32) -> Vec<Option<usize>> {
        let lists = self.region_cells.0.get_or_init(|| {
            let mut lists = vec![Vec::new(); self.region_keys.len()];
            for (i, &r) in self.region.iter().enumerate() {
                if let Some(l) = lists.get_mut(r as usize) {
                    l.push(i);
                }
            }
            lists
        });
        lists
            .iter()
            .map(|cells| {
                let mut best: Option<(f32, usize)> = None;
                for &i in cells {
                    let (cx, cz) = self.centre(i);
                    let d = (cx - x).powi(2) + (cz - z).powi(2);
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, i));
                    }
                }
                best.map(|(_, i)| i)
            })
            .collect()
    }

    /// Drops the per-region cell lists; call after changing [`PathGrid::region`] or
    /// [`PathGrid::region_keys`] once [`PathGrid::nearest_cell_per_region`] has been used.
    pub fn invalidate_region_cells(&mut self) {
        self.region_cells = RegionCells::default();
    }

    /// The nearest place to (x, z) less than `radius` map units away on a cell passable for
    /// `domain` (and of region index `region`, when given): (x, z) itself when its own cell
    /// qualifies, else the centre of the nearest such cell (ties: the lowest cell index).
    pub fn nearest_place_within(&self, x: f32, z: f32, region: Option<u32>, domain: Domain, radius: f32) -> Option<(f32, f32)> {
        let fits = |i: usize| self.passable(i, domain) && region.is_none_or(|r| self.region.get(i) == Some(&r));
        if self.cell_at(x, z).is_some_and(fits) {
            return Some((x, z));
        }
        let c0 = ((x - self.origin.0) / self.cell).floor() as i64;
        let r0 = ((z - self.origin.1) / self.cell).floor() as i64;
        let span = (radius / self.cell).ceil() as i64 + 1;
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let mut best: Option<(f32, usize)> = None;
        for r in (r0 - span).max(0)..=(r0 + span).min(h - 1) {
            for c in (c0 - span).max(0)..=(c0 + span).min(w - 1) {
                let i = (r * w + c) as usize;
                if !fits(i) {
                    continue;
                }
                let (cx, cz) = self.centre(i);
                let d = (cx - x).powi(2) + (cz - z).powi(2);
                if d < radius * radius && best.is_none_or(|(b, bi)| d < b || (d == b && i < bi)) {
                    best = Some((d, i));
                }
            }
        }
        best.map(|(_, i)| self.centre(i))
    }

    /// Finds the cheapest path from `from` to `to` for `domain`. `cost_per_unit(cell)` is the
    /// action point cost of moving one map unit inside that cell (the cell being entered pays).
    /// `min_cost_per_unit` must not exceed any value `cost_per_unit` returns (it keeps the A*
    /// heuristic admissible). `None` if either end has no passable cell nearby or the goal
    /// cannot be reached.
    pub fn find_path(
        &self,
        from: (f32, f32),
        to: (f32, f32),
        domain: Domain,
        min_cost_per_unit: f32,
        cost_per_unit: impl Fn(usize) -> f32,
    ) -> Option<GridPath> {
        let start = self.nearest_passable(from.0, from.1, domain, 3)?;
        let goal = self.nearest_passable(to.0, to.1, domain, 3)?;
        let w = self.width as i64;
        let n = self.kind.len();
        // Cost of one unit step (orthogonal) and diagonal, per entered cell, in milli-AP.
        let ortho = |i: usize| (cost_per_unit(i) * self.cell * MILLI).round().max(1.0) as u64;
        let diag = |i: usize| (cost_per_unit(i) * self.cell * std::f32::consts::SQRT_2 * MILLI).round().max(1.0) as u64;
        // Admissible heuristic: octile distance at the cheapest cost any cell can have.
        let min_cost = min_cost_per_unit.max(0.0);
        let h = |i: usize| -> u64 {
            let (dx, dy) = (((i as i64 % w) - (goal as i64 % w)).abs(), ((i as i64 / w) - (goal as i64 / w)).abs());
            let (lo, hi) = (dx.min(dy) as f32, dx.max(dy) as f32);
            ((lo * std::f32::consts::SQRT_2 + (hi - lo)) * self.cell * min_cost * MILLI * 0.999) as u64
        };
        let mut g = vec![u64::MAX; n];
        let mut came = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        g[start] = 0;
        open.push(Reverse((h(start), start)));
        while let Some(Reverse((_, i))) = open.pop() {
            if i == goal {
                break;
            }
            let gi = g[i];
            let (c, r) = (i as i64 % w, i as i64 / w);
            for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nc, nr) = (c + dc, r + dr);
                if nc < 0 || nr < 0 || nc >= w || nr >= self.height as i64 {
                    continue;
                }
                let j = (nr * w + nc) as usize;
                if !self.passable(j, domain) {
                    continue;
                }
                let step = if dc != 0 && dr != 0 {
                    // No corner cutting.
                    let a = (r * w + nc) as usize;
                    let b = (nr * w + c) as usize;
                    if !self.passable(a, domain) || !self.passable(b, domain) {
                        continue;
                    }
                    diag(j)
                } else {
                    ortho(j)
                };
                let ng = gi + step;
                if ng < g[j] {
                    g[j] = ng;
                    came[j] = i;
                    open.push(Reverse((ng + h(j), j)));
                }
            }
        }
        if g[goal] == u64::MAX {
            return None;
        }
        let mut cells = vec![goal];
        let mut i = goal;
        while i != start {
            i = came[i];
            cells.push(i);
        }
        cells.reverse();
        let mut points = Vec::with_capacity(cells.len() + 1);
        let mut costs = Vec::with_capacity(cells.len() + 1);
        points.push(from);
        costs.push(0.0);
        for &c in &cells[1..] {
            points.push(self.centre(c));
            costs.push(g[c] as f32 / MILLI);
        }
        // End exactly on the target when it is inside the goal cell.
        if self.cell_at(to.0, to.1) == Some(goal) {
            if cells.len() > 1 {
                *points.last_mut().unwrap() = to;
            } else {
                let d = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
                points.push(to);
                costs.push(d * cost_per_unit(goal));
            }
        }
        Some(GridPath { points, costs, polys: Vec::new() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10x5 made-up grid: land everywhere except a sea column at col 5 rows 0..=3.
    fn grid() -> PathGrid {
        let mut g = PathGrid::new((0.0, 0.0), 1.0, 10, 5);
        for i in 0..50 {
            let (c, r) = (i % 10, i / 10);
            g.kind[i] = if c == 5 && r <= 3 { CellKind::Sea as u8 } else { CellKind::Land as u8 };
        }
        g
    }

    #[test]
    fn region_indices_past_u16() {
        // The per-cell region index was a u16 (wrapping past 65,535 regions).
        let mut g = grid();
        g.region_keys = (0..70_001).map(|i| format!("r{i}")).collect();
        g.region[0] = 70_000;
        assert_eq!(g.region_at(0.5, 0.5), Some("r70000"));
        assert_eq!(g.region_at(1.5, 0.5), None);
    }

    #[test]
    fn cached_region_cells_match_brute_force_with_ties() {
        let mut g = grid();
        g.region_keys = vec!["a".into(), "b".into(), "empty".into()];
        for i in 0..50 {
            g.region[i] = if i % 3 == 0 { 0 } else if i % 7 == 0 { u32::MAX } else { 1 };
        }
        let brute = |g: &PathGrid, x: f32, z: f32| -> Vec<Option<usize>> {
            let mut best: Vec<Option<(f32, usize)>> = vec![None; g.region_keys.len()];
            for (i, &r) in g.region.iter().enumerate() {
                let Some(slot) = best.get_mut(r as usize) else { continue };
                let (cx, cz) = g.centre(i);
                let d = (cx - x).powi(2) + (cz - z).powi(2);
                if slot.is_none_or(|(b, _)| d < b) {
                    *slot = Some((d, i));
                }
            }
            best.into_iter().map(|b| b.map(|(_, i)| i)).collect()
        };
        // Points on cell corners and edges are equidistant from several cells.
        for (x, z) in [(1.0, 1.0), (0.5, 0.5), (4.0, 2.0), (5.0, 3.5), (9.5, 4.5), (-3.0, 2.0)] {
            assert_eq!(g.nearest_cell_per_region(x, z), brute(&g, x, z), "({x}, {z})");
        }
        assert_eq!(g.nearest_cell_per_region(0.5, 0.5)[2], None);
        // A change plus invalidation is seen.
        g.region[0] = 1;
        g.invalidate_region_cells();
        assert_eq!(g.nearest_cell_per_region(0.5, 0.5), brute(&g, 0.5, 0.5));
    }


    #[test]
    fn cloned_grid_starts_with_empty_region_cache() {
        let mut g = grid();
        g.region_keys = vec!["a".into(), "b".into()];
        for i in 0..50 {
            g.region[i] = (i % 2) as u32;
        }
        let first = g.nearest_cell_per_region(1.0, 1.0);
        assert!(g.region_cells.0.get().is_some());
        let c = g.clone();
        assert!(c.region_cells.0.get().is_none());
        assert_eq!(c.nearest_cell_per_region(1.0, 1.0), first);
        assert!(c.region_cells.0.get().is_some());
    }
    #[test]
    fn nearest_places_and_region_cells() {
        let mut g = grid();
        g.region_keys = vec!["west".into(), "east".into(), "sea".into()];
        for i in 0..50 {
            let c = i % 10;
            g.region[i] = if g.kind[i] == CellKind::Sea as u8 { 2 } else if c < 5 { 0 } else { 1 };
        }
        assert_eq!(g.region_index("east"), Some(1));
        assert_eq!(g.region_index("nowhere"), None);
        // From (0.5, 0.5): west's nearest cell is its own (0); east's is (6, 0), 6.0 away (the land
        // cell (5, 4) is sqrt(41) away); the sea's is (5, 0).
        assert_eq!(g.nearest_cell_per_region(0.5, 0.5), vec![Some(0), Some(6), Some(5)]);
        // A point on a fitting cell stays itself; otherwise the nearest fitting cell's centre,
        // closer than the radius.
        assert_eq!(g.nearest_place_within(1.2, 0.3, Some(0), Domain::Land, 100.0), Some((1.2, 0.3)));
        assert_eq!(g.nearest_place_within(1.2, 0.3, Some(1), Domain::Land, 100.0), Some((6.5, 0.5)));
        assert_eq!(g.nearest_place_within(1.2, 0.3, Some(1), Domain::Land, 5.0), None);
        assert_eq!(g.nearest_place_within(4.5, 0.5, None, Domain::Sea, 100.0), Some((5.5, 0.5)));
        assert_eq!(g.nearest_place_within(4.5, 0.5, Some(9), Domain::Land, 100.0), None);
    }

    #[test]
    fn walks_around_the_sea() {
        let g = grid();
        let p = g.find_path((0.5, 0.5), (9.5, 0.5), Domain::Land, 1.0, |_| 1.0).unwrap();
        // Must pass row 4 at column 5 (the only land there).
        assert!(p.points.iter().any(|&(x, z)| x == 5.5 && z == 4.5));
        assert!(p.total_cost() > 9.0);
        // Deterministic.
        assert_eq!(p, g.find_path((0.5, 0.5), (9.5, 0.5), Domain::Land, 1.0, |_| 1.0).unwrap());
        // Navies cannot cross land.
        assert!(g.find_path((5.5, 0.5), (0.5, 0.5), Domain::Sea, 1.0, |_| 1.0).is_none_or(|p| p.points.len() <= 2));
    }

    #[test]
    fn roads_are_preferred() {
        let mut g = grid();
        // A road along row 4.
        for c in 0..10 {
            g.road[40 + c] = true;
        }
        let cost = |i: usize| if g.road[i] { 0.4 } else { 1.0 };
        let p = g.find_path((0.5, 3.5), (9.5, 3.5), Domain::Land, 0.4, cost).unwrap();
        assert!(p.points.iter().filter(|&&(_, z)| z == 4.5).count() >= 5);
        let reach = p.reachable(3.0);
        assert!(reach > 0 && reach < p.points.len() - 1);
        assert!(p.costs[reach] <= 3.0);
    }
}

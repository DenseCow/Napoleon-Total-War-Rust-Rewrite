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
    /// Index into [`PathGrid::region_keys`] per cell (`u16::MAX` = none).
    pub region: Vec<u16>,
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
            region: vec![u16::MAX; n],
            region_keys: Vec::new(),
            poly: None,
            harbours: Vec::new(),
            ground_types: Vec::new(),
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

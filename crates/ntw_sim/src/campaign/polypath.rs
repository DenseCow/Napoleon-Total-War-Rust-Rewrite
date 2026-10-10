//! The original's campaign path search over the `pathfinding.esf` polygons
//! (`analysis/campaign/PATHFINDING.md` §2; the exe's A* `0x00AC62E0`).
//!
//! The map is a grid of square cells (2 map units on every shipped map), each cut into polygons
//! that have a kind (land, sea, road, ...) and a pathfinding region id. The search runs over
//! **(cell, polygon) nodes placed at the cell centre** (CONFIRMED):
//! - a node's neighbours are the polygons of the same cell and of the 8 neighbour cells that the
//!   mover may enter and that share an outline edge with it (CONFIRMED `0x00B12CD0`); a diagonal
//!   step needs the same polygon kind (CONFIRMED `0x00B27EB0`) and a shared corner point
//!   (CONFIRMED `0x00B125A0`: the two outlines share a vertex, which across a diagonal is the corner);
//! - a step into another cell costs `cell · len · m` (len 1 or √2), where `m` is the current cell's
//!   header byte for that direction mapped by `byte · 0.0099502485 + 0.7960199`, or, when the
//!   current polygon is a road (kind 6), the road cost of the polygon's region id (CONFIRMED
//!   `0x00B0B260`); a step inside the cell costs 0;
//! - the first step is measured from the start point, the last one to the goal point;
//! - every node also carries a second cost, the summed distance of the path's points from the
//!   straight line start → goal, used only to break ties (CONFIRMED);
//! - the open list is ordered by (g + h) lexicographically over the two costs; the heuristic is the
//!   octile cell distance at 0.33 per map unit (CONFIRMED `0x00B45330`).
//!
//! Costs here are in **action points** (map units x `m`): the original divides them by the mover's
//! maximum action points (cost in turns) and adds the part of the turn already spent, which orders
//! the search the same way (CONFIRMED `0x00B11570`, `0x00AEA2D0`).
//!
//! PROVISIONAL: locating a start/goal point that is not inside an enterable polygon (the original
//! first moves it with its "nearest valid position" floods, not decoded); the cost of moving
//! inside a single polygon; embarking/landing and ports (not modelled in the simulation yet); the
//! raw walked points (cell centres moved into their polygons, see `inside_point`; the smoothed line
//! comes from [`super::polysmooth`]); no node budget.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Polygon kinds (the low 4 bits of a boundary's flags, CONFIRMED getter `0x00B77EC0`).
pub mod kind {
    /// Open land.
    pub const LAND: u8 = 0;
    /// Sea.
    pub const SEA: u8 = 1;
    /// Rare kind 4 (enterable by fleets; meaning UNKNOWN).
    pub const FOUR: u8 = 4;
    /// Road strip / settlement octagon.
    pub const ROAD: u8 = 6;
    /// River strip (INFERRED from the data: lies along the river splines); nobody may enter it.
    pub const RIVER: u8 = 3;
    /// Road over water (INFERRED bridge / ford / port): enterable by armies and fleets.
    pub const SHARED: u8 = 7;
}

/// Who moves: the exe's mover type modulo 3 (CONFIRMED kind table in `0x00AF0910`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mover {
    /// Armies: kinds 0, 6, 7 (types 0, 3, 6, 9).
    Land,
    /// Agents (characters without a force): kinds 0, 6, 7 and 8, the enemy zones of control
    /// (types 1, 4, 7, 10; CONFIRMED kind table, `PATHFINDING_PORTS.md` §9.2).
    Agent,
    /// Fleets: kinds 1, 4, 7.
    Sea,
}

impl Mover {
    /// May this mover enter a polygon of kind `k`? Kinds 10 and 11 (run-time polygons) pass for all.
    pub fn may_enter(self, k: u8) -> bool {
        match self {
            Mover::Land => matches!(k, 0 | 6 | 7 | 10 | 11),
            Mover::Agent => matches!(k, 0 | 6 | 7 | 8 | 10 | 11),
            Mover::Sea => matches!(k, 1 | 4 | 7 | 10 | 11),
        }
    }
}

/// One polygon of a cell, as given to [`PolyMap::build`].
#[derive(Debug, Clone, PartialEq)]
pub struct PolyInput {
    /// Kind (flags & 0xF).
    pub kind: u8,
    /// The pathfinding region id: an index into [`PolyMap::region_sets`] (the original's file stores
    /// 10 bits, 1023 = none; ours may hold any number).
    pub region_id: u32,
    /// Outline, Fixed20 (x, z) map coordinates (exact, so shared vertices compare equal).
    pub outline: Vec<(i32, i32)>,
}

/// One grid cell, as given to [`PolyMap::build`].
#[derive(Debug, Clone, PartialEq)]
pub struct CellInput {
    /// The 8 header bytes: the cost byte per direction (see [`dir_index`]).
    pub header: [u8; 8],
    /// The cell's polygons.
    pub polys: Vec<PolyInput>,
}

/// The polygon map and its precomputed adjacency.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PolyMap {
    /// Map (x, z) of the south-west corner of cell (0, 0).
    pub origin: (f32, f32),
    /// Cell size in map units.
    pub cell: f32,
    /// Columns.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    /// Per cell (row-major from the south-west), the first polygon index; one more entry at the end.
    pub cell_first: Vec<u32>,
    /// Header bytes per cell.
    pub header: Vec<[u8; 8]>,
    /// Per polygon: kind.
    pub kind: Vec<u8>,
    /// Per polygon: region id.
    pub region_id: Vec<u32>,
    /// Per polygon: its cell.
    pub poly_cell: Vec<u32>,
    /// Per polygon: start of its outline in [`PolyMap::points`] (one more entry at the end).
    pub outline_first: Vec<u32>,
    /// Outline points in map units.
    pub points: Vec<(f32, f32)>,
    /// Per polygon: start of its neighbours in [`PolyMap::adj`] (one more entry at the end).
    pub adj_first: Vec<u32>,
    /// Neighbour polygon indices.
    pub adj: Vec<u32>,
    /// Region id -> indices into the grid's region keys (empty = sea / no region).
    pub region_sets: Vec<Vec<u32>>,
    /// Per polygon, its connected component for armies (`[0]`) and fleets (`[1]`): polygons the
    /// mover may enter, joined by adjacency; `u32::MAX` where the mover may not enter. A search
    /// between two components fails at once (the original compares a component id before
    /// searching, INFERRED from `0x00B17AC0`; with or without the check the result is the same).
    pub component: [Vec<u32>; 2],
}

/// The exe's direction index of a step by (dc, dr) cells: `(3·dr + dc − 1) mod 9`:
/// 0 E, 1 NW, 2 N, 3 NE, 4 SW, 5 S, 6 SE, 7 W, 8 same cell (CONFIRMED `0x00B0B260`; rows grow north).
pub fn dir_index(dc: i32, dr: i32) -> usize {
    (3 * dr + dc - 1).rem_euclid(9) as usize
}

/// Step length per direction index, in cells (CONFIRMED table `0x0137DFBC`).
pub const DIR_LEN: [f32; 9] = [1.0, std::f32::consts::SQRT_2, 1.0, std::f32::consts::SQRT_2, std::f32::consts::SQRT_2, 1.0, std::f32::consts::SQRT_2, 1.0, 1.0];

/// A header cost byte as a cost multiplier: `byte · 0.0099502485 + 0.7960199` (CONFIRMED
/// `0x00B205D0`; = (2·byte + 160) / 201).
pub fn byte_cost(b: u8) -> f32 {
    f32::from(b) * 0.009_950_249 + 0.796_019_9
}

/// How far (in cells) [`PolyMap::find_path_avoiding`] looks for a free polygon when the goal is
/// blocked: 10 cells = 20 map units, beyond a navy's 12-unit zone (PROVISIONAL).
pub const BLOCKED_GOAL_RADIUS: i32 = 10;

/// Heuristic cost per map unit (CONFIRMED: 0.66 per 2-unit cell, `0x00B45330`).
pub const HEURISTIC_PER_UNIT: f32 = 0.33;

const FIXED: f32 = 1.0 / (1 << 20) as f32;

/// A path found by [`PolyMap::find_path`].
#[derive(Debug, Clone, PartialEq)]
pub struct PolyPath {
    /// Points (map x, z): the start, the centres of the cells passed, the goal.
    pub points: Vec<(f32, f32)>,
    /// Action points spent to reach each point (`costs[0] == 0`).
    pub costs: Vec<f32>,
    /// The polygon of each point (the start's and the goal's own polygons at the ends).
    pub polys: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq)]
struct Cost(f32, f32);

impl Cost {
    fn add(self, o: Cost) -> Cost {
        Cost(self.0 + o.0, self.1 + o.1)
    }
    fn cmp_lex(self, o: Cost) -> Ordering {
        self.0.total_cmp(&o.0).then(self.1.total_cmp(&o.1))
    }
}

#[derive(Clone, Copy)]
struct Open {
    f: Cost,
    seq: u64,
    node: u32,
}

impl PartialEq for Open {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    // Max-heap: the smallest f (then the earliest pushed) is the greatest.
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.cmp_lex(self.f).then(o.seq.cmp(&self.seq))
    }
}

impl PolyMap {
    /// Builds the map and its adjacency. `cells` are row-major from the south-west, `cols * rows`
    /// of them; `origin` and `cell` in Fixed20.
    pub fn build(origin: (i32, i32), cell: i32, cols: u32, rows: u32, cells: &[CellInput], region_sets: Vec<Vec<u32>>) -> PolyMap {
        let mut m = PolyMap {
            origin: (origin.0 as f32 * FIXED, origin.1 as f32 * FIXED),
            cell: cell as f32 * FIXED,
            cols,
            rows,
            region_sets,
            ..Default::default()
        };
        // Exact outlines (Fixed20) for the adjacency tests.
        let mut exact: Vec<&[(i32, i32)]> = Vec::new();
        for (ci, c) in cells.iter().enumerate() {
            m.cell_first.push(m.kind.len() as u32);
            m.header.push(c.header);
            for p in &c.polys {
                m.kind.push(p.kind);
                m.region_id.push(p.region_id);
                m.poly_cell.push(ci as u32);
                m.outline_first.push(m.points.len() as u32);
                m.points.extend(p.outline.iter().map(|&(x, z)| (x as f32 * FIXED, z as f32 * FIXED)));
                exact.push(&p.outline);
            }
        }
        m.cell_first.push(m.kind.len() as u32);
        m.outline_first.push(m.points.len() as u32);
        let n = m.kind.len();
        let (w, h) = (cols as i32, rows as i32);
        for a in 0..n {
            m.adj_first.push(m.adj.len() as u32);
            let ca = m.poly_cell[a] as i32;
            let (ac, ar) = (ca % w, ca / w);
            for dr in -1..=1 {
                for dc in -1..=1 {
                    let (bc, br) = (ac + dc, ar + dr);
                    if bc < 0 || br < 0 || bc >= w || br >= h {
                        continue;
                    }
                    let cb = (br * w + bc) as usize;
                    for b in m.cell_first[cb] as usize..m.cell_first[cb + 1] as usize {
                        if b == a {
                            continue;
                        }
                        let ok = if dc != 0 && dr != 0 {
                            // The corner shared by the two cells.
                            let corner = (
                                origin.0 + (ac + i32::from(dc > 0)) * cell,
                                origin.1 + (ar + i32::from(dr > 0)) * cell,
                            );
                            m.kind[a] == m.kind[b] && m.kind[a] != 2 && exact[a].contains(&corner) && exact[b].contains(&corner)
                        } else {
                            share_edge(exact[a], exact[b])
                        };
                        if ok {
                            m.adj.push(b as u32);
                        }
                    }
                }
            }
        }
        m.adj_first.push(m.adj.len() as u32);
        m.component = [m.components(Mover::Land), m.components(Mover::Sea)];
        m
    }

    /// Labels the connected components of the polygons `mover` may enter (see
    /// [`PolyMap::component`]).
    fn components(&self, mover: Mover) -> Vec<u32> {
        let n = self.len();
        let mut comp = vec![u32::MAX; n];
        let mut next = 0u32;
        let mut stack = Vec::new();
        for s in 0..n {
            if comp[s] != u32::MAX || !mover.may_enter(self.kind[s]) {
                continue;
            }
            comp[s] = next;
            stack.push(s);
            while let Some(p) = stack.pop() {
                for &q in self.neighbours(p) {
                    let q = q as usize;
                    if comp[q] == u32::MAX && mover.may_enter(self.kind[q]) {
                        comp[q] = next;
                        stack.push(q);
                    }
                }
            }
            next += 1;
        }
        comp
    }

    /// Number of polygons.
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    /// True without polygons.
    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    /// A polygon's outline in map units.
    pub fn outline(&self, p: usize) -> &[(f32, f32)] {
        &self.points[self.outline_first[p] as usize..self.outline_first[p + 1] as usize]
    }

    /// A polygon's neighbours.
    pub fn neighbours(&self, p: usize) -> &[u32] {
        &self.adj[self.adj_first[p] as usize..self.adj_first[p + 1] as usize]
    }

    /// The static map as a [`View`] (no run-time polygons).
    pub fn view(&self) -> View<'_> {
        View { map: self, overlay: None }
    }

    /// The map with the run-time polygons of `overlay` cut in (see [`Overlay`]).
    pub fn with<'a>(&'a self, overlay: &'a Overlay) -> View<'a> {
        View { map: self, overlay: Some(overlay) }
    }

    /// The cell (col, row) holding map point (x, z).
    pub fn cell_of(&self, x: f32, z: f32) -> Option<(u32, u32)> {
        let c = ((x - self.origin.0) / self.cell).floor();
        let r = ((z - self.origin.1) / self.cell).floor();
        (c >= 0.0 && r >= 0.0 && c < self.cols as f32 && r < self.rows as f32).then_some((c as u32, r as u32))
    }

    /// The centre of a cell (CONFIRMED `0x00B5B6C0`: origin + index · cell + cell / 2).
    pub fn centre(&self, cell: u32) -> (f32, f32) {
        let (c, r) = (cell % self.cols, cell / self.cols);
        (self.origin.0 + (c as f32 + 0.5) * self.cell, self.origin.1 + (r as f32 + 0.5) * self.cell)
    }

    /// The polygon under (x, z), if any.
    pub fn polygon_at(&self, x: f32, z: f32) -> Option<usize> {
        self.view().polygon_at(x, z)
    }

    /// The polygon a mover at (x, z) stands in: the polygon under the point if the mover may enter
    /// it, else the nearest enterable polygon within `radius` cells (PROVISIONAL stand-in for the
    /// original's "nearest valid position" searches).
    pub fn locate(&self, x: f32, z: f32, mover: Mover, radius: i32) -> Option<usize> {
        self.view().locate(x, z, mover, radius)
    }

    /// The polygons that come within `radius` map units of (x, z) (inside or nearer than
    /// `radius` to their outline; touching at exactly `radius` does not count), in index order.
    pub fn polygons_within(&self, x: f32, z: f32, radius: f32) -> Vec<u32> {
        self.view().polygons_within(x, z, radius)
    }

    /// The (column, row) of polygon `p`'s cell.
    pub fn cell_rc(&self, p: usize) -> (i32, i32) {
        let c = self.poly_cell[p];
        ((c % self.cols) as i32, (c / self.cols) as i32)
    }

    /// The cost multiplier of a step out of polygon `p` in direction `d` (see [`View::multiplier`]).
    pub fn multiplier(&self, p: usize, d: usize, road_cost: &[f32]) -> f32 {
        self.view().multiplier(p, d, road_cost)
    }

    /// The last step into the goal (see [`View::goal_step`]).
    pub fn goal_step(&self, p: usize, gp: usize, d: usize, from: (f32, f32), g: (f32, f32), road_cost: &[f32]) -> f32 {
        self.view().goal_step(p, gp, d, from, g, road_cost)
    }

    /// Finds the original's path from `from` to `to` for `mover`. `road_cost[id]` is the cost
    /// multiplier on road polygons of region id `id` (see [`PolyMap::road_costs`]). `None` if
    /// either end has no enterable polygon nearby or the goal cannot be reached.
    pub fn find_path(&self, from: (f32, f32), to: (f32, f32), mover: Mover, road_cost: &[f32]) -> Option<PolyPath> {
        self.view().find_path_avoiding(from, to, mover, road_cost, &|_| false)
    }

    /// [`PolyMap::find_path`] with extra polygons the mover may not enter (see
    /// [`View::find_path_avoiding`]).
    pub fn find_path_avoiding(
        &self,
        from: (f32, f32),
        to: (f32, f32),
        mover: Mover,
        road_cost: &[f32],
        blocked: &dyn Fn(usize) -> bool,
    ) -> Option<PolyPath> {
        self.view().find_path_avoiding(from, to, mover, road_cost, blocked)
    }

    /// The road cost per region id from the road cost of each region (indexed like the grid's
    /// region keys): a single region's own value, a border group's minimum over its regions
    /// (CONFIRMED `0x00B62760`), `default` for the sea group and ids without regions.
    pub fn road_costs(&self, region_cost: impl Fn(usize) -> f32, default: f32) -> Vec<f32> {
        self.region_sets
            .iter()
            .map(|s| s.iter().map(|&r| region_cost(r as usize)).reduce(f32::min).unwrap_or(default))
            .collect()
    }
}


/// Run-time polygons over a [`PolyMap`]: the original's obstacle cuts (zones of control, cores;
/// `PATHFINDING_PORTS.md` §9.1, built by [`super::rtcut`]). A base polygon keeps its index; a cut
/// gives it a new kind and/or outline (its first piece, as the original reuses the polygon's slot,
/// `0x00B13520`) and appends further pieces after the base polygons. Neighbour lists of every
/// polygon around a changed cell are replaced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    /// Number of base polygons (pieces start here).
    pub base_len: usize,
    /// Kind per base polygon, `NO_KIND` where unchanged (empty = nothing changed).
    pub base_kind: Vec<u8>,
    /// New outlines of base polygons.
    pub base_outline: std::collections::HashMap<u32, Vec<(f32, f32)>>,
    /// New neighbour lists of base polygons.
    pub base_adj: std::collections::HashMap<u32, Vec<u32>>,
    /// Appended pieces: kind, region id, cell, outline, neighbours and the base polygon each was
    /// cut from.
    pub kind: Vec<u8>,
    /// See [`Overlay::kind`].
    pub region_id: Vec<u32>,
    /// See [`Overlay::kind`].
    pub poly_cell: Vec<u32>,
    /// See [`Overlay::kind`].
    pub outline: Vec<Vec<(f32, f32)>>,
    /// See [`Overlay::kind`].
    pub adj: Vec<Vec<u32>>,
    /// See [`Overlay::kind`].
    pub origin: Vec<u32>,
    /// The pieces appended to each cell.
    pub cell_extra: std::collections::HashMap<u32, Vec<u32>>,
    /// The cells whose polygons were cut (rebuilt), ascending.
    pub rebuilt: Vec<u32>,
}

/// Marks an unchanged kind in [`Overlay::base_kind`].
pub const NO_KIND: u8 = 0xFF;

impl Overlay {
    /// An overlay that changes nothing yet.
    pub fn new(map: &PolyMap) -> Overlay {
        Overlay { base_len: map.len(), ..Default::default() }
    }

    /// True when it changes nothing.
    pub fn is_empty(&self) -> bool {
        self.base_kind.is_empty() && self.base_outline.is_empty() && self.base_adj.is_empty() && self.kind.is_empty()
    }
}

/// A [`PolyMap`] seen with or without an [`Overlay`]: what the search, the smoothing and the zone
/// floods read.
#[derive(Debug, Clone, Copy)]
pub struct View<'a> {
    /// The static map.
    pub map: &'a PolyMap,
    /// The run-time polygons, if any.
    pub overlay: Option<&'a Overlay>,
}

impl<'a> View<'a> {
    /// Number of polygons (base and pieces).
    pub fn len(&self) -> usize {
        self.map.len() + self.overlay.map_or(0, |o| o.kind.len())
    }

    /// True without polygons.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The piece index of polygon `p`, if it is a piece.
    fn piece(&self, p: usize) -> Option<(&'a Overlay, usize)> {
        let o = self.overlay?;
        (p >= o.base_len).then(|| (o, p - o.base_len))
    }

    /// Kind of polygon `p`.
    pub fn kind(&self, p: usize) -> u8 {
        match self.overlay {
            Some(o) if p >= o.base_len => o.kind[p - o.base_len],
            Some(o) => match o.base_kind.get(p) {
                Some(&k) if k != NO_KIND => k,
                _ => self.map.kind[p],
            },
            None => self.map.kind[p],
        }
    }

    /// Region id of polygon `p`.
    pub fn region_id(&self, p: usize) -> u32 {
        match self.piece(p) {
            Some((o, i)) => o.region_id[i],
            None => self.map.region_id[p],
        }
    }

    /// Cell of polygon `p`.
    pub fn poly_cell(&self, p: usize) -> u32 {
        match self.piece(p) {
            Some((o, i)) => o.poly_cell[i],
            None => self.map.poly_cell[p],
        }
    }

    /// The base polygon `p` was cut from (`p` itself for a base polygon).
    pub fn origin_of(&self, p: usize) -> usize {
        match self.piece(p) {
            Some((o, i)) => o.origin[i] as usize,
            None => p,
        }
    }

    /// Outline of polygon `p` in map units.
    pub fn outline(&self, p: usize) -> &'a [(f32, f32)] {
        match self.overlay {
            Some(o) if p >= o.base_len => &o.outline[p - o.base_len],
            Some(o) => match o.base_outline.get(&(p as u32)) {
                Some(v) => v,
                None => self.map.outline(p),
            },
            None => self.map.outline(p),
        }
    }

    /// Neighbours of polygon `p`.
    pub fn neighbours(&self, p: usize) -> &'a [u32] {
        match self.overlay {
            Some(o) if p >= o.base_len => &o.adj[p - o.base_len],
            Some(o) => match o.base_adj.get(&(p as u32)) {
                Some(v) => v,
                None => self.map.neighbours(p),
            },
            None => self.map.neighbours(p),
        }
    }

    /// The polygons of cell `ci` (base polygons, then pieces).
    pub fn cell_polys(&self, ci: usize) -> impl Iterator<Item = usize> + 'a {
        let base = self.map.cell_first[ci] as usize..self.map.cell_first[ci + 1] as usize;
        let extra: &'a [u32] = self.overlay.and_then(|o| o.cell_extra.get(&(ci as u32))).map_or(&[], |v| v.as_slice());
        base.chain(extra.iter().map(|&p| p as usize))
    }

    /// The component (`[0]` land, `[1]` sea) of polygon `p`: that of the base polygon it was cut
    /// from (a superset: a cut may split a component, the search then fails by exhausting it).
    pub fn component(&self, which: usize, p: usize) -> Option<u32> {
        self.map.component[which].get(self.origin_of(p)).copied()
    }

    /// The (column, row) of polygon `p`'s cell.
    pub fn cell_rc(&self, p: usize) -> (i32, i32) {
        let c = self.poly_cell(p);
        ((c % self.map.cols) as i32, (c / self.map.cols) as i32)
    }

    /// The polygon under (x, z), if any.
    pub fn polygon_at(&self, x: f32, z: f32) -> Option<usize> {
        let (c, r) = self.map.cell_of(x, z)?;
        let ci = (r * self.map.cols + c) as usize;
        let polys: Vec<usize> = self.cell_polys(ci).collect();
        if polys.len() == 1 {
            return Some(polys[0]);
        }
        polys.iter().copied().find(|&p| point_in_polygon(self.outline(p), x, z)).or_else(|| {
            // On an edge: the nearest polygon of the cell.
            polys.iter().copied().min_by(|&a, &b| dist_to_polygon(self.outline(a), x, z).total_cmp(&dist_to_polygon(self.outline(b), x, z)))
        })
    }

    /// The polygon a mover at (x, z) stands in (see [`PolyMap::locate`]).
    pub fn locate(&self, x: f32, z: f32, mover: Mover, radius: i32) -> Option<usize> {
        self.locate_where(x, z, radius, &|p| mover.may_enter(self.kind(p)))
    }

    /// The polygon under (x, z) if `ok` accepts it, else the nearest accepted polygon within
    /// `radius` cells (lowest index on ties).
    pub fn locate_where(&self, x: f32, z: f32, radius: i32, ok: &dyn Fn(usize) -> bool) -> Option<usize> {
        if let Some(p) = self.polygon_at(x, z)
            && ok(p)
        {
            return Some(p);
        }
        let m = self.map;
        let c0 = ((x - m.origin.0) / m.cell).floor() as i32;
        let r0 = ((z - m.origin.1) / m.cell).floor() as i32;
        let mut best: Option<(f32, usize)> = None;
        for r in r0 - radius..=r0 + radius {
            for c in c0 - radius..=c0 + radius {
                if c < 0 || r < 0 || c >= m.cols as i32 || r >= m.rows as i32 {
                    continue;
                }
                let ci = (r as u32 * m.cols + c as u32) as usize;
                for p in self.cell_polys(ci) {
                    if !ok(p) {
                        continue;
                    }
                    let d = dist_to_polygon(self.outline(p), x, z);
                    if best.is_none_or(|(bd, bp)| d < bd || (d == bd && p < bp)) {
                        best = Some((d, p));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }

    /// A point of polygon `poly` near `p`: the closest outline point, nudged 0.05 units towards
    /// the polygon's inside (or a point inside it, see `inside_point`).
    pub fn point_in_near(&self, poly: usize, p: (f32, f32)) -> (f32, f32) {
        let o = self.outline(poly);
        if o.is_empty() || point_in_polygon(o, p.0, p.1) {
            return p;
        }
        let mut best = (f32::INFINITY, p);
        for i in 0..o.len() {
            let (a, b) = (o[i], o[(i + 1) % o.len()]);
            let (vx, vz) = (b.0 - a.0, b.1 - a.1);
            let len2 = vx * vx + vz * vz;
            let t = if len2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vz) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let q = (a.0 + vx * t, a.1 + vz * t);
            let d = (q.0 - p.0).powi(2) + (q.1 - p.1).powi(2);
            if d < best.0 {
                best = (d, q);
            }
        }
        let inner = self.inside_point(poly, self.map.centre(self.poly_cell(poly)));
        let q = best.1;
        let d = ((inner.0 - q.0).powi(2) + (inner.1 - q.1).powi(2)).sqrt();
        if d > 0.0 {
            let f = (0.05 / d).min(1.0);
            let r = (q.0 + (inner.0 - q.0) * f, q.1 + (inner.1 - q.1) * f);
            if point_in_polygon(o, r.0, r.1) {
                return r;
            }
        }
        inner
    }

    /// The polygons that come within `radius` map units of (x, z) (see
    /// [`PolyMap::polygons_within`]).
    pub fn polygons_within(&self, x: f32, z: f32, radius: f32) -> Vec<u32> {
        let m = self.map;
        let reach = (radius / m.cell).ceil() as i32 + 1;
        let c0 = ((x - m.origin.0) / m.cell).floor() as i32;
        let r0 = ((z - m.origin.1) / m.cell).floor() as i32;
        let mut out = Vec::new();
        for r in r0 - reach..=r0 + reach {
            for c in c0 - reach..=c0 + reach {
                if c < 0 || r < 0 || c >= m.cols as i32 || r >= m.rows as i32 {
                    continue;
                }
                let ci = (r as u32 * m.cols + c as u32) as usize;
                for p in self.cell_polys(ci) {
                    if dist_to_polygon(self.outline(p), x, z) < radius {
                        out.push(p as u32);
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// The cost multiplier of a step out of polygon `p` in direction `d` (CONFIRMED `0x00B0B260`):
    /// the region's road cost when `p` is a road and the step leaves the cell, else the cell's
    /// header byte `d`.
    pub fn multiplier(&self, p: usize, d: usize, road_cost: &[f32]) -> f32 {
        if d == 8 {
            return 0.0;
        }
        if self.kind(p) == kind::ROAD
            && let Some(&c) = road_cost.get(self.region_id(p) as usize)
        {
            return c;
        }
        byte_cost(self.map.header[self.poly_cell(p) as usize][d])
    }

    /// The last step: from the centre of polygon `p`'s cell into the goal point `g` (polygon `gp`)
    /// in the neighbour cell in direction `d` (CONFIRMED `0x00B0B260`, constants √2/2 and 1.5·√2
    /// per map unit, i.e. for 2-unit cells): the straight part of the offset `| |dx| − |dz| |` costs
    /// `m(d)`; the diagonal part `min(|dx|, |dz|)` costs `√2 · lerp(a, b, (t − 1) / 2)` where `a` and
    /// `b` are the header bytes for the diagonal `d2` of the current cell and of the goal's cell
    /// (raw bytes, no road cost), `t = max(|dx|, |dz|)` and `d2` is the diagonal next to `d` on the
    /// goal's side (`0x00B31E40`; `d2 = d` for a diagonal `d`).
    pub fn goal_step(&self, p: usize, gp: usize, d: usize, from: (f32, f32), g: (f32, f32), road_cost: &[f32]) -> f32 {
        let (dx, dz) = (g.0 - from.0, g.1 - from.1);
        let (ax, az) = (dx.abs(), dz.abs());
        let m = self.multiplier(p, d, road_cost);
        let d2 = match d {
            0 => if dz >= 0.0 { 3 } else { 6 },
            7 => if dz >= 0.0 { 1 } else { 4 },
            2 => if dx > 0.0 { 3 } else { 1 },
            5 => if dx > 0.0 { 6 } else { 4 },
            other => other,
        };
        let a = byte_cost(self.map.header[self.poly_cell(p) as usize][d2]);
        let b = byte_cost(self.map.header[self.poly_cell(gp) as usize][d2]);
        let t = ax.max(az);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        (ax - az).abs() * m + ax.min(az) * (a * (1.5 * std::f32::consts::SQRT_2 - h * t) + b * (h * t - h))
    }

    /// Finds the original's path from `from` to `to` for `mover` (see [`PolyMap::find_path`]).
    pub fn find_path(&self, from: (f32, f32), to: (f32, f32), mover: Mover, road_cost: &[f32]) -> Option<PolyPath> {
        self.find_path_avoiding(from, to, mover, road_cost, &|_| false)
    }

    /// [`View::find_path`] with extra polygons the mover may not enter (`blocked`). The start
    /// polygon is never blocked. When the goal lies in a polygon the mover may not enter (a
    /// run-time cut, or `blocked`) while the static map would let it, the path ends in the nearest
    /// enterable polygon within [`BLOCKED_GOAL_RADIUS`] cells, at the point of it nearest the goal
    /// (PROVISIONAL stand-in for the original's "nearest valid position" floods with radius 3, 12
    /// or 24 units).
    pub fn find_path_avoiding(
        &self,
        from: (f32, f32),
        to: (f32, f32),
        mover: Mover,
        road_cost: &[f32],
        blocked: &dyn Fn(usize) -> bool,
    ) -> Option<PolyPath> {
        let free = |p: usize| mover.may_enter(self.kind(p)) && !blocked(p);
        let sp = self.locate(from.0, from.1, mover, 2)?;
        let mut to = to;
        // Is the goal held by a run-time cut or a blocked polygon (rather than off the mover's
        // ground altogether)?
        let under = self.polygon_at(to.0, to.1);
        let held = under.is_some_and(|u| u != sp && !free(u) && mover.may_enter(self.map.kind[self.origin_of(u)]));
        let gp = if held {
            let g = self.locate_where(to.0, to.1, BLOCKED_GOAL_RADIUS, &|p| free(p) || p == sp)?;
            to = self.point_in_near(g, to);
            g
        } else {
            let g = self.locate(to.0, to.1, mover, 2)?;
            if g != sp && blocked(g) {
                let g = self.locate_where(to.0, to.1, BLOCKED_GOAL_RADIUS, &|p| free(p) || p == sp)?;
                to = self.point_in_near(g, to);
                g
            } else {
                g
            }
        };
        let ci = usize::from(mover == Mover::Sea);
        if let (Some(a), Some(b)) = (self.component(ci, sp), self.component(ci, gp))
            && a != b
        {
            return None;
        }
        let seg = (from, to);
        let off_line = |p: (f32, f32)| dist_to_segment(p, seg.0, seg.1);
        if sp == gp {
            // Same polygon: the original's search ends at once (UNKNOWN cost); we charge the
            // straight distance at the cell's multiplier towards the goal. PROVISIONAL.
            let (dx, dz) = (to.0 - from.0, to.1 - from.1);
            let d = octant_dir(dx, dz);
            let cost = (dx * dx + dz * dz).sqrt() * self.multiplier(sp, d, road_cost);
            return Some(PolyPath { points: vec![from, to], costs: vec![0.0, cost], polys: vec![sp as u32, gp as u32] });
        }
        let (gc, gr) = self.cell_rc(gp);
        let cell = self.map.cell;
        let h = |p: usize| -> Cost {
            let (c, r) = self.cell_rc(p);
            let (dx, dy) = ((c - gc).abs() as f32, (r - gr).abs() as f32);
            let (lo, hi) = (dx.min(dy), dx.max(dy));
            let per_cell = cell * HEURISTIC_PER_UNIT;
            Cost((hi - lo) * per_cell + lo * per_cell * std::f32::consts::SQRT_2, 0.0)
        };
        let n = self.len();
        // Node n = the start location; 0..n = (cell, polygon) nodes.
        let start = n;
        let mut g = vec![Cost(f32::INFINITY, f32::INFINITY); n + 1];
        let mut parent = vec![u32::MAX; n + 1];
        let mut closed = vec![false; n + 1];
        let mut heap = BinaryHeap::new();
        let mut seq = 0u64;
        g[start] = Cost(0.0, 0.0);
        heap.push(Open { f: h(sp), seq, node: start as u32 });
        let pos = |node: usize| if node == start { from } else if node == gp { to } else { self.map.centre(self.poly_cell(node)) };
        let mut found = false;
        while let Some(Open { f, node, .. }) = heap.pop() {
            let node = node as usize;
            if closed[node] {
                continue;
            }
            let hn = if node == start { h(sp) } else { h(node) };
            if f != g[node].add(hn) {
                continue; // stale entry
            }
            if node == gp {
                found = true;
                break;
            }
            closed[node] = true;
            let poly = if node == start { sp } else { node };
            let (c0, r0) = self.cell_rc(poly);
            let here = pos(node);
            for &nb in self.neighbours(poly) {
                let nb = nb as usize;
                // Out of a kind 10 or 11 polygon only the kinds the link table allows (10 -> 10, 11, 7;
                // 11 -> 11; CONFIRMED `0x00B573C0`): a run-time zone entered for its goal is not left.
                let here_kind = self.kind(poly);
                if matches!(here_kind, 10 | 11) && !super::rtcut::links(here_kind, self.kind(nb)) {
                    continue;
                }
                if !mover.may_enter(self.kind(nb)) || (nb != gp && blocked(nb)) {
                    continue;
                }
                let (c1, r1) = self.cell_rc(nb);
                let d = dir_index(c1 - c0, r1 - r0);
                let there = pos(nb);
                let step = if d == 8 {
                    Cost(0.0, 0.0)
                } else if node == start {
                    let (dx, dz) = (there.0 - here.0, there.1 - here.1);
                    let dev = if nb == gp { 0.0 } else { off_line(there) };
                    Cost((dx * dx + dz * dz).sqrt() * self.multiplier(poly, d, road_cost), dev)
                } else if nb == gp {
                    Cost(self.goal_step(poly, gp, d, here, to, road_cost), off_line(there))
                } else {
                    Cost(cell * DIR_LEN[d] * self.multiplier(poly, d, road_cost), off_line(there))
                };
                let ng = g[node].add(step);
                let old = g[nb];
                // Update rule of 0x00AC62E0: clearly cheaper, or about equal and straighter.
                let better = !old.0.is_finite() || (ng.0 * 0.9999 < old.0 && (ng.0 * 1.0001 < old.0 || ng.1 < old.1));
                if better {
                    g[nb] = ng;
                    parent[nb] = node as u32;
                    closed[nb] = false;
                    seq += 1;
                    heap.push(Open { f: ng.add(h(nb)), seq, node: nb as u32 });
                }
            }
        }
        if !found {
            return None;
        }
        let mut chain = vec![gp];
        let mut i = gp;
        while i != start {
            i = parent[i] as usize;
            chain.push(i);
        }
        chain.reverse();
        let mut path = PolyPath { points: Vec::new(), costs: Vec::new(), polys: Vec::new() };
        for &node in &chain {
            // The search measures every node at its cell centre (CONFIRMED); the walked point is
            // moved into the node's own polygon when the centre lies outside it (PROVISIONAL: the
            // original's path smoothing is UNKNOWN), so a mover never stops in a polygon it may
            // not enter.
            let p = if node == start || node == gp { pos(node) } else { self.inside_point(node, pos(node)) };
            let poly = if node == start { sp } else { node } as u32;
            // Polygons of one cell share its centre: keep one point (the later one).
            if node != start && path.points.last() == Some(&p) && node != gp {
                *path.costs.last_mut().expect("non-empty") = g[node].0;
                *path.polys.last_mut().expect("non-empty") = poly;
                continue;
            }
            path.points.push(p);
            path.costs.push(g[node].0);
            path.polys.push(poly);
        }
        Some(path)
    }

    /// `p` if it lies inside polygon `poly`, else a point inside the polygon near it: the vertex
    /// average, or failing that the midpoint of the polygon's first edge nudged inwards.
    pub fn inside_point(&self, poly: usize, p: (f32, f32)) -> (f32, f32) {
        let o = self.outline(poly);
        if o.is_empty() || point_in_polygon(o, p.0, p.1) {
            return p;
        }
        let n = o.len() as f32;
        let avg = (o.iter().map(|v| v.0).sum::<f32>() / n, o.iter().map(|v| v.1).sum::<f32>() / n);
        if point_in_polygon(o, avg.0, avg.1) {
            return avg;
        }
        // Concave: walk from each edge midpoint a little towards the vertex average.
        for i in 0..o.len() {
            let (a, b) = (o[i], o[(i + 1) % o.len()]);
            let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
            let q = (mid.0 + (avg.0 - mid.0) * 0.01, mid.1 + (avg.1 - mid.1) * 0.01);
            if point_in_polygon(o, q.0, q.1) {
                return q;
            }
        }
        avg
    }
}

/// The direction index whose step points closest to (dx, dz).
pub fn octant_dir(dx: f32, dz: f32) -> usize {
    if dx == 0.0 && dz == 0.0 {
        return 8;
    }
    let a = dz.atan2(dx);
    let k = ((a / std::f32::consts::FRAC_PI_4).round() as i32).rem_euclid(8);
    // k: 0 E, 1 NE, 2 N, 3 NW, 4 W, 5 SW, 6 S, 7 SE.
    let (dc, dr) = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)][k as usize];
    dir_index(dc, dr)
}

/// True if the two outlines share an edge (the same two points, either direction).
fn share_edge(a: &[(i32, i32)], b: &[(i32, i32)]) -> bool {
    fn edges(p: &[(i32, i32)]) -> impl Iterator<Item = ((i32, i32), (i32, i32))> + '_ {
        (0..p.len()).map(move |i| (p[i], p[(i + 1) % p.len()])).filter(|(u, v)| u != v)
    }
    edges(a).any(|(u, v)| edges(b).any(|(x, y)| (u == x && v == y) || (u == y && v == x)))
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(p: &[(f32, f32)], x: f32, z: f32) -> bool {
    let mut inside = false;
    let mut j = p.len().wrapping_sub(1);
    for i in 0..p.len() {
        let (a, b) = (p[i], p[j]);
        if (a.1 > z) != (b.1 > z) && x < (b.0 - a.0) * (z - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Distance from `p` to the segment `a`-`b`.
pub fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vz) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vz * vz;
    let t = if len2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vz) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qz) = (a.0 + vx * t - p.0, a.1 + vz * t - p.1);
    (qx * qx + qz * qz).sqrt()
}

/// Distance from (x, z) to a polygon (0 inside).
fn dist_to_polygon(p: &[(f32, f32)], x: f32, z: f32) -> f32 {
    if point_in_polygon(p, x, z) {
        return 0.0;
    }
    (0..p.len()).map(|i| dist_to_segment((x, z), p[i], p[(i + 1) % p.len()])).fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: i32 = 1 << 20;

    /// A made-up `w` x `h` map of 2-unit cells, one square polygon per cell; `kind(c, r)` and
    /// `byte(c, r)` give each cell's kind and its header bytes (all directions alike).
    fn map(w: u32, h: u32, kind: impl Fn(u32, u32) -> u8, byte: impl Fn(u32, u32) -> u8) -> PolyMap {
        let mut cells = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let (x0, z0) = (c as i32 * 2 * U, r as i32 * 2 * U);
                let sq = vec![(x0, z0), (x0 + 2 * U, z0), (x0 + 2 * U, z0 + 2 * U), (x0, z0 + 2 * U)];
                cells.push(CellInput { header: [byte(c, r); 8], polys: vec![PolyInput { kind: kind(c, r), region_id: 0, outline: sq }] });
            }
        }
        PolyMap::build((0, 0), 2 * U, w, h, &cells, vec![vec![0]])
    }

    #[test]
    fn directions_match_the_exe_table() {
        assert_eq!(dir_index(1, 0), 0);
        assert_eq!(dir_index(-1, 1), 1);
        assert_eq!(dir_index(0, 1), 2);
        assert_eq!(dir_index(1, 1), 3);
        assert_eq!(dir_index(-1, -1), 4);
        assert_eq!(dir_index(0, -1), 5);
        assert_eq!(dir_index(1, -1), 6);
        assert_eq!(dir_index(-1, 0), 7);
        assert_eq!(dir_index(0, 0), 8);
        assert!((byte_cost(0) - 0.796_02).abs() < 1e-4);
        assert!((byte_cost(255) - 3.333_33).abs() < 1e-3);
    }

    #[test]
    fn straight_line_costs_the_header_bytes() {
        // 10 x 1 land, byte 20 (m ~ 0.995) everywhere.
        let m = map(10, 1, |_, _| kind::LAND, |_, _| 20);
        let p = m.find_path((1.0, 1.0), (19.0, 1.0), Mover::Land, &[0.5]).unwrap();
        assert_eq!(p.points.first(), Some(&(1.0, 1.0)));
        assert_eq!(p.points.last(), Some(&(19.0, 1.0)));
        // 18 units at m(20).
        assert!((p.costs.last().unwrap() - 18.0 * byte_cost(20)).abs() < 1e-3, "{:?}", p.costs);
        // Monotone costs; cell centres in between.
        assert!(p.costs.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(p.points[1], (3.0, 1.0));
    }

    #[test]
    fn goes_round_water_and_through_cheap_cells() {
        // 9 x 5: a sea wall at column 4 except row 4; row 0 is expensive (byte 255).
        let m = map(9, 5, |c, r| if c == 4 && r < 4 { kind::SEA } else { kind::LAND }, |_, r| if r == 0 { 255 } else { 0 });
        let p = m.find_path((1.0, 3.0), (17.0, 3.0), Mover::Land, &[0.5]).unwrap();
        assert!(p.polys.iter().all(|&q| m.kind[q as usize] == kind::LAND));
        assert!(p.points.iter().any(|&(x, z)| x == 9.0 && z == 9.0), "crosses at the gap: {:?}", p.points);
        // Fleets stay at sea and cannot reach land.
        assert!(m.find_path((9.0, 1.0), (9.0, 5.0), Mover::Sea, &[0.5]).is_some());
        assert!(m.find_path((9.0, 1.0), (1.0, 1.0), Mover::Sea, &[0.5]).is_none_or(|p| p.polys.iter().all(|&q| m.kind[q as usize] == kind::SEA)));
        // Deterministic.
        assert_eq!(p, m.find_path((1.0, 3.0), (17.0, 3.0), Mover::Land, &[0.5]).unwrap());
    }

    #[test]
    fn roads_use_the_region_cost() {
        // Row 1 is road (kind 6, region id 0), others land byte 100 (m ~ 1.79).
        let m = map(10, 3, |_, r| if r == 1 { kind::ROAD } else { kind::LAND }, |_, _| 100);
        let p = m.find_path((1.0, 3.0), (19.0, 3.0), Mover::Land, &[0.4]).unwrap();
        // 16 units of road steps at 0.4 plus the ends.
        assert!(*p.costs.last().unwrap() < 18.0 * byte_cost(100) * 0.5, "{:?}", p.costs);
        assert!(m.road_costs(|_| 0.6, 0.67) == vec![0.6]);
    }

    #[test]
    fn region_ids_past_u16_keep_their_road_cost() {
        // Region ids and region indices were u16 (wrapping past 65,535); a map may have more.
        const ID: u32 = 70_000;
        let sq = vec![(0, 0), (2 * U, 0), (2 * U, 2 * U), (0, 2 * U)];
        let cells = [CellInput { header: [100; 8], polys: vec![PolyInput { kind: kind::ROAD, region_id: ID, outline: sq }] }];
        let mut sets = vec![Vec::new(); ID as usize + 1];
        sets[ID as usize] = vec![ID + 5];
        let m = PolyMap::build((0, 0), 2 * U, 1, 1, &cells, sets);
        let costs = m.road_costs(|r| if r == ID as usize + 5 { 0.3 } else { 1.0 }, 0.67);
        assert_eq!(m.view().region_id(0), ID);
        assert_eq!(costs[ID as usize], 0.3);
        assert_eq!(m.view().multiplier(0, 0, &costs), 0.3);
    }

    #[test]
    fn last_step_blends_the_two_cells_diagonal_bytes() {
        // Column 0 byte 0, column 1 byte 200: the last step from the centre (1, 1) of cell (0, 0)
        // to (4, 1.8) in cell (1, 0) is 2.2 straight at m(E) of cell 0 plus 0.8 diagonal; with
        // t = 3 the diagonal part costs √2 · the goal cell's NE byte.
        let m = map(3, 2, |_, _| kind::LAND, |c, _| if c == 0 { 0 } else { 200 });
        let g = m.goal_step(0, 1, 0, (1.0, 1.0), (4.0, 1.8), &[]);
        let want = 2.2 * byte_cost(0) + 0.8 * std::f32::consts::SQRT_2 * byte_cost(200);
        assert!((g - want).abs() < 1e-3, "{g} vs {want}");
        // At t = 1 (goal just across the side) the current cell's byte counts instead.
        let g = m.goal_step(0, 1, 0, (1.0, 1.0), (2.0, 1.5), &[]);
        let want = 0.5 * byte_cost(0) + 0.5 * std::f32::consts::SQRT_2 * byte_cost(0);
        assert!((g - want).abs() < 1e-3, "{g} vs {want}");
    }
}

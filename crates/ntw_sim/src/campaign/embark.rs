//! Ports, embarking and landing on the campaign map (`analysis/campaign/PATHFINDING_PORTS.md`).
//!
//! On every shipped map the sea (polygon kind 1) never touches open land (kind 0): a strip of
//! kind 2 polygons lies between them (CONFIRMED from the data). Fleets and armies therefore meet
//! only through two special nodes the original adds while it expands a naval location
//! (CONFIRMED `0x00B2CC20`):
//! - a **port node**: from a naval location in a kind 7 polygon (or next to one), an open port
//!   within √2 map units of that polygon's point gives one node at the port's position;
//! - **landing nodes**, for a fleet whose goal is a land mover's position: every coastal polygon
//!   (kind 2 or 3) in the cells around gives a land position: the nearest point of a polygon the
//!   army may stand on (kinds 0 and 6) within the 3 x 3 cells around the coastal polygon's point,
//!   accepted when closer than 1.5 map units (CONFIRMED `0x00B373F0` / `0x00B41860`).
//!
//! A step from a naval to a land location (landing) **ends the turn**: its cost in turns becomes
//! `ceil(max(g, 1)) + the army's spent fraction of its turn`, or `ceil(g) + 1` when the fleet
//! lands out of a kind 7 polygon (CONFIRMED `0x00B11570` / `0x00B07A70`). Costs here are in
//! **turns** like the original's (action points divided by the mover's maximum), because a
//! transport path mixes the fleet's and the army's action points.
//!
//! The polygon point ([`polygon_point`], `0x00B489E0`) and the landing valid-position check
//! ([`land_position`], `0x00B6A0D0`) follow the original (CONFIRMED); ports are nodes for their
//! owner and for factions at war with it ([`CampaignModel::ports_for`], CONFIRMED).
//!
//! INFERRED: leaving a harbour through the port's footprint ([`Harbour`]: the run-time cutter cuts
//! building footprints only for armies and agents, PATHFINDING_PORTS.md §9.1, so the fleet side is
//! not found; the data fits at all 67 ports). PROVISIONAL: the cost
//! of steps into and out of the special nodes (straight distance at the cell's multiplier, like the
//! step out of the start); embarking: the original's embark search reaches the fleet through
//! run-time polygons cut around it (kinds 8 / 10, not in the data), ours through the landing
//! positions around the fleet ([`embark_points`]).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use super::commands::{CommandError, PlannedMove};
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId};
use super::pathing::GridPath;
use super::polypath::{dir_index, dist_to_segment, kind, octant_dir, point_in_polygon, Mover, PolyMap, DIR_LEN, HEURISTIC_PER_UNIT};
use super::rules::OFF_ROAD_COST;
use super::world::{CampaignModel, Character, Stance};
use crate::fixed::Fixed20;

/// A landing place: (the land polygon, the position).
pub type Landing = (usize, (f32, f32));

/// A port is used when it lies closer than this to the kind 7 polygon's point (CONFIRMED:
/// distance² < 2 map units², `0x00B2CC20`).
pub const PORT_RADIUS: f32 = std::f32::consts::SQRT_2;

/// A landing position must lie closer than this to the coastal polygon's point (CONFIRMED 1.5 map
/// units, `0x00B373F0`).
pub const LANDING_RADIUS: f32 = 1.5;

/// True for the polygon kinds a fleet lands from: the coastal strip (2) and the river strip (3)
/// (CONFIRMED `0x00B2CC20`).
pub fn is_coast(k: u8) -> bool {
    k == 2 || k == kind::RIVER
}

/// The polygon kinds an army may be put down on (CONFIRMED `0x00B41860`, mover type 0: kinds 0
/// and 6; kinds 10 / 11 are run-time polygons, not in the data).
pub fn army_stands_on(k: u8) -> bool {
    k == kind::LAND || k == kind::ROAD
}

/// A port a fleet may enter, as the caller sees it for the moving fleet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Port {
    /// The port's map position (x, z).
    pub position: (f32, f32),
    /// The fleet may use it as a port node: its owner is the fleet's faction or at war with it
    /// (CONFIRMED `0x00B2CC20` with `0x008CE9B0`, see [`CampaignModel::ports_for`]); the original
    /// also skips a port whose virtual `+0x18` is set (UNKNOWN meaning). Decided by the caller.
    pub open: bool,
}

/// The movers of a transport: the fleet, and the army it carries (or the agent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transport {
    /// The fleet's maximum action points.
    pub fleet_max_ap: f32,
    /// The fleet's spent part of this turn, `1 − AP left / AP max` (CONFIRMED move context +4).
    pub fleet_spent: f32,
    /// The army's maximum action points.
    pub army_max_ap: f32,
    /// The army's spent part of this turn (CONFIRMED move context +0x18, added on landing).
    pub army_spent: f32,
}

/// A transport path: the fleet's points, then, after the landing, the army's.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportPath {
    /// Points (map x, z), from the fleet's position to the goal.
    pub points: Vec<(f32, f32)>,
    /// Cost in turns to reach each point (`costs[0]` = the fleet's spent fraction).
    pub costs: Vec<f32>,
    /// The polygon of each point.
    pub polys: Vec<u32>,
    /// Whether each point is a naval location.
    pub sea: Vec<bool>,
}

impl TransportPath {
    /// Index of the landing point (the first land point), if the path lands.
    pub fn landing(&self) -> Option<usize> {
        self.sea.iter().position(|s| !s)
    }

    /// Index of the last point reached within the current turn (cost ≤ 1 turn).
    pub fn reachable_this_turn(&self) -> usize {
        self.costs.iter().rposition(|&c| c <= 1.0 + 1e-4).unwrap_or(0)
    }
}

/// The original's reference point of polygon `p` (CONFIRMED `0x00B489E0`, a scanline sweep): the
/// polygon's non-horizontal edges are swept from the lowest z upwards, level by level (the edges'
/// end points); at each level the edges ending there are dropped and those starting there added,
/// their x at that level sorted ascending, and leading pairs with equal x dropped; the first level
/// with two different x gives `(smallest x, level)`. So it is a point **on the outline**: the
/// left end of the polygon's lowest stretch of positive width (for a flat bottom, its left corner).
/// The cell centre when the sweep finds nothing (the original returns (0, 0); its fallback sweep in
/// 1/2^20 steps is not needed for these outlines).
pub fn polygon_point(pm: &PolyMap, p: usize) -> (f32, f32) {
    let o = pm.outline(p);
    let edges: Vec<((f32, f32), (f32, f32))> = (0..o.len())
        .map(|i| (o[i], o[(i + 1) % o.len()]))
        .filter(|(a, b)| a.1 != b.1)
        .map(|(a, b)| if a.1 < b.1 { (a, b) } else { (b, a) })
        .collect();
    let mut levels: Vec<f32> = edges.iter().flat_map(|(a, b)| [a.1, b.1]).collect();
    levels.sort_by(f32::total_cmp);
    levels.dedup();
    for &z in &levels {
        // Edges active at this level: starting at or below it and ending above it.
        let mut xs: Vec<f32> = edges
            .iter()
            .filter(|(a, b)| a.1 <= z && b.1 > z)
            .map(|(a, b)| a.0 + (z - a.1) * (b.0 - a.0) / (b.1 - a.1))
            .collect();
        xs.sort_by(f32::total_cmp);
        let mut i = 0;
        while i + 1 < xs.len() && xs[i] == xs[i + 1] {
            i += 2;
        }
        if i + 1 < xs.len() {
            return (xs[i], z);
        }
    }
    pm.centre(pm.poly_cell[p])
}

/// The nearest point of an outline to `p`, and its distance (`p` itself and 0 inside).
fn nearest_on_polygon(o: &[(f32, f32)], p: (f32, f32)) -> ((f32, f32), f32) {
    if point_in_polygon(o, p.0, p.1) {
        return (p, 0.0);
    }
    let mut best = (p, f32::INFINITY);
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        let (vx, vz) = (b.0 - a.0, b.1 - a.1);
        let len2 = vx * vx + vz * vz;
        let t = if len2 > 0.0 { (((p.0 - a.0) * vx + (p.1 - a.1) * vz) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let q = (a.0 + vx * t, a.1 + vz * t);
        let d = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
        if d < best.1 {
            best = (q, d);
        }
    }
    best
}

/// Polygons of the cells within `r` cells of (c, r0) (on the map).
fn polys_around(pm: &PolyMap, c: i32, r0: i32, r: i32) -> impl Iterator<Item = usize> + '_ {
    (r0 - r..=r0 + r)
        .flat_map(move |rr| (c - r..=c + r).map(move |cc| (cc, rr)))
        .filter(|&(cc, rr)| cc >= 0 && rr >= 0 && cc < pm.cols as i32 && rr < pm.rows as i32)
        .flat_map(move |(cc, rr)| {
            let ci = (rr as u32 * pm.cols + cc as u32) as usize;
            pm.cell_first[ci] as usize..pm.cell_first[ci + 1] as usize
        })
}

/// The tolerance of the original's "nearest valid position" step for a landing place (CONFIRMED
/// 0.001 map units, `0x00B6A1F0` → `0x00B38FC0`).
pub const VALID_RADIUS: f32 = 0.001;

/// The land position an army is put down at near `at` (CONFIRMED `0x00B373F0` / `0x00B41860`): the
/// nearest point of a polygon it may stand on ([`army_stands_on`]) in the 3 x 3 cells around `at`,
/// if closer than [`LANDING_RADIUS`]; then the valid-position check (CONFIRMED `0x00B6A0D0` →
/// `0x00B6A1F0` → `0x00B53930`): the polygon **containing** the point must be one the army may
/// stand on, else the point is moved to a valid one within [`VALID_RADIUS`] (a point on the shared
/// edge with the coast is nudged that far into the land polygon). The original also counts the
/// run-time polygons cut around forces (not in our model). Returns (polygon, position).
pub fn land_position(pm: &PolyMap, at: (f32, f32)) -> Option<(usize, (f32, f32))> {
    let c = ((at.0 - pm.origin.0) / pm.cell).floor() as i32;
    let r = ((at.1 - pm.origin.1) / pm.cell).floor() as i32;
    let mut best: Option<(f32, usize, (f32, f32))> = None;
    for p in polys_around(pm, c, r, 1) {
        if !army_stands_on(pm.kind[p]) {
            continue;
        }
        let (q, d) = nearest_on_polygon(pm.outline(p), at);
        if best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, p, q));
        }
    }
    let (d, p, q) = best?;
    if d >= LANDING_RADIUS {
        return None;
    }
    valid_position(pm, p, q).map(|q| (p, q))
}

/// `q` if the polygon containing it is one an army may stand on, else `q` moved up to
/// [`VALID_RADIUS`] into polygon `p` (towards `p`'s vertex average); `None` if that fails.
fn valid_position(pm: &PolyMap, p: usize, q: (f32, f32)) -> Option<(f32, f32)> {
    let ok = |q: (f32, f32)| pm.polygon_at(q.0, q.1).is_some_and(|x| army_stands_on(pm.kind[x]) && point_in_polygon(pm.outline(x), q.0, q.1));
    if ok(q) {
        return Some(q);
    }
    let o = pm.outline(p);
    let n = o.len().max(1) as f32;
    let avg = (o.iter().map(|v| v.0).sum::<f32>() / n, o.iter().map(|v| v.1).sum::<f32>() / n);
    let (dx, dz) = (avg.0 - q.0, avg.1 - q.1);
    let len = (dx * dx + dz * dz).sqrt();
    if len <= 0.0 {
        return None;
    }
    let step = VALID_RADIUS.min(len);
    let moved = (q.0 + dx / len * step, q.1 + dz / len * step);
    ok(moved).then_some(moved)
}

/// The landing position of a coastal polygon (`None` if it is not coastal or has none).
pub fn landing_of(pm: &PolyMap, coast: usize) -> Option<(usize, (f32, f32))> {
    if !is_coast(pm.kind[coast]) {
        return None;
    }
    land_position(pm, polygon_point(pm, coast))
}

/// Where an army can board a fleet at `fleet`: the landing positions of the coastal polygons in the
/// 3 x 3 cells around the fleet, and the fleet's own position when it lies in a kind 7 polygon (a
/// port or landing place both may enter), nearest first. PROVISIONAL (see the module docs).
pub fn embark_points(pm: &PolyMap, fleet: (f32, f32)) -> Vec<(usize, (f32, f32))> {
    let mut out: Vec<(usize, (f32, f32))> = Vec::new();
    if let Some(p) = pm.polygon_at(fleet.0, fleet.1)
        && pm.kind[p] == kind::SHARED
    {
        out.push((p, fleet));
    }
    let Some((c, r)) = pm.cell_of(fleet.0, fleet.1) else { return out };
    for q in polys_around(pm, c as i32, r as i32, 1) {
        if let Some(l) = landing_of(pm, q)
            && !out.iter().any(|o| o.1 == l.1)
        {
            out.push(l);
        }
    }
    let d = |p: (f32, f32)| (p.0 - fleet.0).powi(2) + (p.1 - fleet.1).powi(2);
    out.sort_by(|a, b| d(a.1).total_cmp(&d(b.1)));
    out
}

/// A port's harbour from the map (`regions.esf`, the `port` slot description).
///
/// A port's kind 7 polygons touch only land and the coastal strip, never a sea polygon (CONFIRMED
/// on all 5 maps), and an ordinary step never enters kind 7 from another kind (`0x00B16A30`), so a
/// fleet gets into a port only through the port node, and the static polygons give it no way out.
/// The port slot's third outline (the union of its land and sea footprints) reaches into a sea
/// polygon at every one of the 67 ports, and its second coordinate (the dock) lies in the sea
/// footprint (CONFIRMED data). INFERRED: the fleet leaves through it (the run-time cutter, decoded,
/// cuts building footprints as kind 7 only for armies and agents, so how fleets leave is not found);
/// we let a fleet in a port's kind 7 polygons step to the sea polygons the footprint reaches.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Harbour {
    /// The port slot's position (x, z).
    pub port: (f32, f32),
    /// The slot's dock point (x, z).
    pub dock: (f32, f32),
    /// The slot's third outline (the whole footprint).
    pub outline: Vec<(f32, f32)>,
}

fn segments_cross(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    let cross = |o: (f32, f32), p: (f32, f32), q: (f32, f32)| (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0);
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

/// True if two outlines overlap (a vertex of one inside the other, or crossing edges).
fn outlines_overlap(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    a.iter().any(|v| point_in_polygon(b, v.0, v.1))
        || b.iter().any(|v| point_in_polygon(a, v.0, v.1))
        || (0..a.len()).any(|i| {
            let (p, q) = (a[i], a[(i + 1) % a.len()]);
            (0..b.len()).any(|j| segments_cross(p, q, b[j], b[(j + 1) % b.len()]))
        })
}

/// The sea polygons (kinds 1 and 4) a fleet at `at` in a port's kind 7 polygons may sail out to:
/// those the footprint of a harbour holding `at` (or whose port lies within [`PORT_RADIUS`]) reaches.
pub fn harbour_exits(pm: &PolyMap, harbours: &[Harbour], at: (f32, f32)) -> Vec<usize> {
    let mut out = Vec::new();
    for h in harbours {
        let near = (h.port.0 - at.0).powi(2) + (h.port.1 - at.1).powi(2) < PORT_RADIUS * PORT_RADIUS;
        if h.outline.len() < 3 || !(near || point_in_polygon(&h.outline, at.0, at.1)) {
            continue;
        }
        let (lo, hi) = h.outline.iter().fold(((f32::INFINITY, f32::INFINITY), (f32::NEG_INFINITY, f32::NEG_INFINITY)), |(lo, hi), v| {
            ((lo.0.min(v.0), lo.1.min(v.1)), (hi.0.max(v.0), hi.1.max(v.1)))
        });
        let (Some((c0, r0)), Some((c1, r1))) = (pm.cell_of(lo.0.max(pm.origin.0), lo.1.max(pm.origin.1)), pm.cell_of(hi.0, hi.1)) else { continue };
        for r in r0..=r1 {
            for c in c0..=c1 {
                let ci = (r * pm.cols + c) as usize;
                for q in pm.cell_first[ci] as usize..pm.cell_first[ci + 1] as usize {
                    if matches!(pm.kind[q], kind::SEA | kind::FOUR) && !out.contains(&q) && outlines_overlap(&h.outline, pm.outline(q)) {
                        out.push(q);
                    }
                }
            }
        }
    }
    out
}

/// The open port within [`PORT_RADIUS`] of polygon `p`'s point, if any (the nearest).
pub fn port_near(pm: &PolyMap, p: usize, ports: &[Port]) -> Option<(f32, f32)> {
    let at = polygon_point(pm, p);
    ports
        .iter()
        .filter(|port| port.open)
        .map(|port| (port.position, (port.position.0 - at.0).powi(2) + (port.position.1 - at.1).powi(2)))
        .filter(|&(_, d2)| d2 < PORT_RADIUS * PORT_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(pos, _)| pos)
}

#[derive(Clone, Copy, PartialEq)]
struct Cost(f32, f32);

impl Cost {
    fn cmp_lex(self, o: Cost) -> Ordering {
        self.0.total_cmp(&o.0).then(self.1.total_cmp(&o.1))
    }
}

#[derive(Clone, Copy)]
struct Open {
    f: Cost,
    seq: u64,
    node: usize,
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
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.cmp_lex(self.f).then(o.seq.cmp(&self.seq))
    }
}

/// A search location.
#[derive(Clone, Copy, PartialEq)]
struct Loc {
    sea: bool,
    poly: u32,
    pos: (f32, f32),
    /// Positioned (start, goal, port or landing) rather than a cell centre.
    point: bool,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
enum Key {
    Poly(bool, u32),
    Point(bool, u32, u32),
}

/// Finds the original's transport path: a fleet at `from` (sea) to `to`, where the goal is a land
/// position (`goal == Mover::Land`: the fleet carries an army and lands it, then the army walks on)
/// or a naval one (a fleet move that may enter `ports`). `road_cost` as for
/// [`PolyMap::find_path`]. `None` if no path.
#[allow(clippy::too_many_arguments)] // the original's query takes the same inputs
pub fn find_transport_path(
    pm: &PolyMap,
    from: (f32, f32),
    to: (f32, f32),
    goal: Mover,
    ports: &[Port],
    harbours: &[Harbour],
    road_cost: &[f32],
    t: &Transport,
) -> Option<TransportPath> {
    let sp = pm.locate(from.0, from.1, Mover::Sea, 2)?;
    let gp = pm.locate(to.0, to.1, goal, 2)?;
    let goal_sea = goal == Mover::Sea;
    let mut locs: Vec<Loc> = Vec::new();
    let mut index: HashMap<Key, usize> = HashMap::new();
    let mut node = |locs: &mut Vec<Loc>, key: Key, loc: Loc| -> usize {
        *index.entry(key).or_insert_with(|| {
            locs.push(loc);
            locs.len() - 1
        })
    };
    let start = node(&mut locs, Key::Point(true, from.0.to_bits(), from.1.to_bits()), Loc { sea: true, poly: sp as u32, pos: from, point: true });
    let goal_node = node(&mut locs, Key::Poly(goal_sea, gp as u32), Loc { sea: goal_sea, poly: gp as u32, pos: to, point: true });
    let (gc, gr) = pm.cell_rc(gp);
    let divisor = t.fleet_max_ap.max(t.army_max_ap).max(1e-3);
    let h = |p: usize| -> f32 {
        let (c, r) = pm.cell_rc(p);
        let (dx, dy) = ((c - gc).abs() as f32, (r - gr).abs() as f32);
        let (lo, hi) = (dx.min(dy), dx.max(dy));
        let per_cell = pm.cell * HEURISTIC_PER_UNIT;
        ((hi - lo) * per_cell + lo * per_cell * std::f32::consts::SQRT_2) / divisor
    };
    let off_line = |p: (f32, f32)| dist_to_segment(p, from, to);
    let mut landing_cache: HashMap<usize, Option<Landing>> = HashMap::new();
    let mut g: Vec<Cost> = vec![Cost(t.fleet_spent.max(0.0), 0.0)];
    let mut parent: Vec<usize> = vec![usize::MAX];
    let mut closed: Vec<bool> = vec![false];
    let grow = |g: &mut Vec<Cost>, parent: &mut Vec<usize>, closed: &mut Vec<bool>, n: usize| {
        while g.len() < n {
            g.push(Cost(f32::INFINITY, f32::INFINITY));
            parent.push(usize::MAX);
            closed.push(false);
        }
    };
    grow(&mut g, &mut parent, &mut closed, locs.len());
    let mut heap = BinaryHeap::new();
    let mut seq = 0u64;
    heap.push(Open { f: Cost(g[start].0 + h(sp), 0.0), seq, node: start });
    let mut found = false;
    while let Some(Open { f, node: a, .. }) = heap.pop() {
        if closed[a] || f != Cost(g[a].0 + h(locs[a].poly as usize), g[a].1) {
            continue;
        }
        if a == goal_node {
            found = true;
            break;
        }
        closed[a] = true;
        let la = locs[a];
        let pa = la.poly as usize;
        let mover = if la.sea { Mover::Sea } else { Mover::Land };
        // Candidate successors (key, loc).
        let mut next: Vec<(Key, Loc)> = Vec::new();
        let mut port_added = false;
        if la.sea
            && pm.kind[pa] == kind::SHARED
            && let Some(pos) = port_near(pm, pa, ports)
            && pos != la.pos
        {
            next.push((Key::Point(true, pos.0.to_bits(), pos.1.to_bits()), Loc { sea: true, poly: pa as u32, pos, point: true }));
            port_added = true;
        }
        let neigh = pm.neighbours(pa);
        let (c0, r0) = pm.cell_rc(pa);
        // Leaving a harbour (see [`Harbour`]): from a naval location in a port's kind 7 polygons,
        // the sea polygons its footprint reaches. INFERRED from the data.
        let exits: Vec<usize> = if la.sea && pm.kind[pa] == kind::SHARED { harbour_exits(pm, harbours, la.pos) } else { Vec::new() };
        for q in polys_around(pm, c0, r0, 1).chain(exits.iter().copied()) {
            let exit = exits.contains(&q);
            let normal = (q == pa && la.point) || neigh.contains(&(q as u32));
            // `0x00B16A30` forbids stepping into kind 7 from another kind for mover types 0/1/2 and
            // 6/7/8 (CONFIRMED), not for 3/4/5 and 9/10/11; settlements lie on kind 7, so the
            // armies that reach them must be of the second family (INFERRED). Which family our
            // movers are is UNKNOWN, so no such block here (as in `polypath`).
            if (normal || exit) && mover.may_enter(pm.kind[q]) {
                let pos = if q == gp && la.sea == goal_sea { to } else { pm.centre(pm.poly_cell[q]) };
                next.push((Key::Poly(la.sea, q as u32), Loc { sea: la.sea, poly: q as u32, pos, point: q == gp && la.sea == goal_sea }));
            } else if la.sea && !port_added && pm.kind[q] == kind::SHARED
                && let Some(pos) = port_near(pm, q, ports)
                && pos != la.pos
            {
                next.push((Key::Point(true, pos.0.to_bits(), pos.1.to_bits()), Loc { sea: true, poly: q as u32, pos, point: true }));
                port_added = true;
            }
        }
        if la.sea && !goal_sea && !port_added {
            for q in polys_around(pm, c0, r0, 1) {
                if !is_coast(pm.kind[q]) {
                    continue;
                }
                let l = *landing_cache.entry(q).or_insert_with(|| landing_of(pm, q));
                if let Some((lp, pos)) = l {
                    next.push((Key::Point(false, pos.0.to_bits(), pos.1.to_bits()), Loc { sea: false, poly: lp as u32, pos, point: true }));
                }
            }
        }
        for (key, lb) in next {
            let b = if key == Key::Poly(goal_sea, gp as u32) { goal_node } else { node(&mut locs, key, lb) };
            if b == a {
                continue;
            }
            grow(&mut g, &mut parent, &mut closed, locs.len());
            let lb = locs[b];
            let pb = lb.poly as usize;
            let ng = if la.sea && !lb.sea {
                // Landing ends the turn (CONFIRMED 0x00B11570 / 0x00B07A70).
                let turns = if pm.kind[pa] == kind::SHARED { g[a].0.ceil() + 1.0 } else { g[a].0.max(1.0).ceil() + t.army_spent.max(0.0) };
                Cost(turns, g[a].1 + off_line(lb.pos))
            } else {
                let (c1, r1) = pm.cell_rc(pb);
                let mut d = dir_index(c1 - c0, r1 - r0);
                let ap = if la.point || (lb.point && b != goal_node) {
                    let (dx, dz) = (lb.pos.0 - la.pos.0, lb.pos.1 - la.pos.1);
                    if d == 8 {
                        d = octant_dir(dx, dz);
                    }
                    if d == 8 { 0.0 } else { (dx * dx + dz * dz).sqrt() * pm.multiplier(pa, d, road_cost) }
                } else if d == 8 {
                    0.0
                } else if b == goal_node {
                    pm.goal_step(pa, gp, d, la.pos, to, road_cost)
                } else {
                    pm.cell * DIR_LEN[d] * pm.multiplier(pa, d, road_cost)
                };
                let max = if la.sea { t.fleet_max_ap } else { t.army_max_ap }.max(1e-3);
                Cost(g[a].0 + ap / max, g[a].1 + if b == goal_node { 0.0 } else { off_line(lb.pos) })
            };
            let old = g[b];
            let better = !old.0.is_finite() || (ng.0 * 0.9999 < old.0 && (ng.0 * 1.0001 < old.0 || ng.1 < old.1));
            if better {
                g[b] = ng;
                parent[b] = a;
                closed[b] = false;
                seq += 1;
                heap.push(Open { f: Cost(ng.0 + h(pb), ng.1), seq, node: b });
            }
        }
    }
    if !found {
        return None;
    }
    let mut chain = vec![goal_node];
    let mut i = goal_node;
    while i != start {
        i = parent[i];
        chain.push(i);
    }
    chain.reverse();
    let mut path = TransportPath { points: Vec::new(), costs: Vec::new(), polys: Vec::new(), sea: Vec::new() };
    for &n in &chain {
        let l = locs[n];
        let mut p = l.pos;
        let o = pm.outline(l.poly as usize);
        if !l.point && !o.is_empty() && !point_in_polygon(o, p.0, p.1) {
            p = polygon_point(pm, l.poly as usize);
        }
        if !l.point && path.points.last() == Some(&p) {
            *path.costs.last_mut().expect("non-empty") = g[n].0;
            continue;
        }
        path.points.push(p);
        path.costs.push(g[n].0);
        path.polys.push(l.poly);
        path.sea.push(l.sea);
    }
    Some(path)
}

fn to_fixed(p: (f32, f32)) -> (Fixed20, Fixed20) {
    (Fixed20::from_f64(f64::from(p.0)), Fixed20::from_f64(f64::from(p.1)))
}

fn to_f32(p: (Fixed20, Fixed20)) -> (f32, f32) {
    (p.0.to_f32(), p.1.to_f32())
}

/// The spent part of a character's turn, `1 − AP left / AP max` (CONFIRMED move context +4).
fn spent(ch: &Character) -> f32 {
    if ch.max_movement_points <= 0 {
        return 1.0;
    }
    (1.0 - ch.movement_points as f32 / ch.max_movement_points as f32).clamp(0.0, 1.0)
}

/// Embarking and landing in the campaign model (the commands `Embark` / `Disembark`, the moves of
/// a navy with an army aboard, a navy's path into a port).
///
/// Rules: CONFIRMED as in the module docs (landing positions, landing ends the turn, port nodes).
/// A navy carries one army; a second army that boards joins it when both fit into one army
/// (CONFIRMED in `0x0091A560`: against the carried army's capacity virtual `+0x48`, the
/// campaign's units per army, [`CampaignModel::max_units`]).
/// Boarding spends no action points itself (INFERRED: the boarding code sets none; the walk to
/// the fleet does). The original saves the link as NAVY #4 / ARMY #7 (CONFIRMED; our
/// loader reads it, our writer does not yet: see [`World::embarked`](super::world::World::embarked)).
impl CampaignModel {
    /// The navy carrying `army`, if it is aboard one.
    pub fn carrier_of(&self, army: ForceId) -> Option<ForceId> {
        self.world.embarked.get(&army).copied()
    }

    /// The armies aboard `navy`.
    pub fn passengers_of(&self, navy: ForceId) -> Vec<ForceId> {
        self.world.embarked.iter().filter(|&(_, &n)| n == navy).map(|(&a, _)| a).collect()
    }

    /// The polygon map and the road cost per region id, as [`CampaignModel::plan_path`] uses them.
    fn poly_and_roads(&self) -> Option<(&PolyMap, Vec<f32>, &[Harbour])> {
        let grid = &self.terrain.as_ref()?.0;
        let pm = grid.poly.as_ref()?;
        let road: Vec<f32> = grid.region_keys.iter().map(|k| self.rules.road_cost(self.road_level(k))).collect();
        let by_id = pm.road_costs(|r| road.get(r).copied().unwrap_or(OFF_ROAD_COST), self.rules.road_cost(0));
        Some((pm, by_id, &grid.harbours))
    }

    /// The ports as `faction`'s fleets see them: every port slot with a position, open (a port node)
    /// when its region belongs to the faction or to a faction it is **at war** with (CONFIRMED
    /// `0x00B2CC20`: owner == mover, or `0x008CE9B0`, which is the "at war" test: the binding of
    /// `force_declare_war` declares war only when it is false; it holds between different factions
    /// when either has no diplomacy object, INFERRED the rebels, or their relationship says war).
    /// Allied ports are not entered (a third case, the owner being a faction the caller names in
    /// the move context, is UNKNOWN and not used).
    pub fn ports_for(&self, faction: FactionId) -> Vec<Port> {
        self.world
            .regions
            .values()
            .flat_map(|r| r.slots.iter().filter(|s| s.port).filter_map(move |s| s.position.map(|p| (r.owner, p))))
            .map(|(owner, p)| Port {
                position: to_f32(p),
                open: owner == faction || self.at_war(faction, owner),
            })
            .collect()
    }

    /// The original's "at war" test `0x008CE9B0` as our model can answer it: different factions
    /// whose stance is war, or either of them the rebel faction (stored with an empty key; the
    /// original: a faction without a diplomacy object, INFERRED the rebels).
    pub fn at_war(&self, a: FactionId, b: FactionId) -> bool {
        if a == b {
            return false;
        }
        let rebel = |f: FactionId| self.world.factions.get(&f).is_some_and(|x| x.key.is_empty());
        rebel(a) || rebel(b) || self.world.stance(a, b) == Stance::War
    }

    /// The transport path of an embarked army to `to`: its navy sails, lands it and it walks on
    /// (costs in turns). `None` if the army is not aboard a navy or there is no path.
    pub fn plan_transport(&self, army: ForceId, to: (Fixed20, Fixed20)) -> Option<TransportPath> {
        let navy = self.carrier_of(army)?;
        let ac = self.world.forces.get(&army)?.commander.and_then(|c| self.world.characters.get(&c))?;
        let nc = self.world.forces.get(&navy)?.commander.and_then(|c| self.world.characters.get(&c))?;
        let t = Transport {
            fleet_max_ap: nc.max_movement_points as f32,
            fleet_spent: spent(nc),
            army_max_ap: ac.max_movement_points as f32,
            army_spent: spent(ac),
        };
        let from = to_f32(nc.position);
        let goal = to_f32(to);
        match self.poly_and_roads() {
            Some((pm, roads, harbours)) => {
                let ports = self.ports_for(nc.faction);
                find_transport_path(pm, from, goal, Mover::Land, &ports, harbours, &roads, &t)
            }
            None => {
                // No terrain: sail and land in one straight step (PROVISIONAL, tests without a map).
                let d = ((goal.0 - from.0).powi(2) + (goal.1 - from.1).powi(2)).sqrt();
                let sail = t.fleet_spent + d * self.rules.road_cost(0) / t.fleet_max_ap.max(1.0);
                let land = sail.max(1.0).ceil() + t.army_spent;
                Some(TransportPath { points: vec![from, goal], costs: vec![t.fleet_spent, land], polys: vec![0, 0], sea: vec![true, false] })
            }
        }
    }

    /// [`CampaignModel::plan_path`] for an embarked army: the transport path with its costs in the
    /// army's action points (turns × the army's maximum).
    pub(crate) fn plan_transport_move(&self, army: ForceId, to: (Fixed20, Fixed20)) -> Option<PlannedMove> {
        let p = self.plan_transport(army, to)?;
        let max = self.world.forces.get(&army)?.commander.and_then(|c| self.world.characters.get(&c))?.max_movement_points as f32;
        let reachable = p.reachable_this_turn();
        let base = p.costs[0];
        let costs = p.costs.iter().map(|c| (c - base).max(0.0) * max).collect();
        Some(PlannedMove { path: GridPath { points: p.points, costs, polys: p.polys }, reachable })
    }

    /// A navy's path into an open port at `to` (the port node, CONFIRMED) or out of the harbour it
    /// lies in (a kind 7 polygon, see the harbour exit in [`find_transport_path`]); costs in the
    /// navy's action points. `None` otherwise (the caller then plans an ordinary sea path).
    pub(crate) fn plan_port_entry(&self, navy_commander: CharacterId, to: (Fixed20, Fixed20)) -> Option<PlannedMove> {
        let ch = self.world.characters.get(&navy_commander)?;
        let goal = to_f32(to);
        let ports = self.ports_for(ch.faction);
        let (pm, roads, harbours) = self.poly_and_roads()?;
        let from = to_f32(ch.position);
        let into_port = ports.iter().any(|p| p.open && (p.position.0 - goal.0).powi(2) + (p.position.1 - goal.1).powi(2) < PORT_RADIUS * PORT_RADIUS);
        let in_harbour = pm.polygon_at(from.0, from.1).is_some_and(|p| pm.kind[p] == kind::SHARED);
        if !into_port && !in_harbour {
            return None;
        }
        let max = ch.max_movement_points as f32;
        let t = Transport { fleet_max_ap: max, fleet_spent: 0.0, army_max_ap: max, army_spent: 0.0 };
        let p = find_transport_path(pm, to_f32(ch.position), goal, Mover::Sea, &ports, harbours, &roads, &t)?;
        let costs: Vec<f32> = p.costs.iter().map(|c| c * max.max(1.0)).collect();
        let path = GridPath { points: p.points, costs, polys: p.polys };
        let reachable = path.reachable(ch.movement_points as f32);
        Some(PlannedMove { path, reachable })
    }

    /// Moves the commanders of the armies aboard the navy `character` commands to its position.
    pub(crate) fn sync_passengers(&mut self, character: CharacterId) {
        let Some(navy) = self.force_of(character) else { return };
        let Some(pos) = self.world.characters.get(&character).map(|c| c.position) else { return };
        for army in self.passengers_of(navy) {
            if let Some(c) = self.world.forces.get(&army).and_then(|f| f.commander)
                && let Some(ch) = self.world.characters.get_mut(&c)
            {
                ch.position = pos;
            }
        }
    }

    /// `Embark`: the army walks to the nearest place where it can board the navy and, once there,
    /// boards it.
    pub(crate) fn embark(&mut self, force: ForceId, navy: ForceId) -> Result<Vec<CampaignEvent>, CommandError> {
        let c = self.commander_of(force)?;
        let (army_faction, army_units, army_is_navy) = {
            let f = &self.world.forces[&force];
            (f.faction, f.units.len(), f.is_navy)
        };
        let n = self.world.forces.get(&navy).ok_or(CommandError::UnknownForce(navy))?;
        if army_is_navy || !n.is_navy {
            return Err(CommandError::Unsupported("only an army can board a navy"));
        }
        if n.faction != army_faction {
            return Err(CommandError::WrongFaction);
        }
        if n.commander.is_none() {
            return Err(CommandError::NoCommander(navy));
        }
        if self.carrier_of(force).is_some() {
            return Err(CommandError::Unsupported("the army is already aboard a navy"));
        }
        // The navy carries one army: a second one joins it, which the original allows when both armies'
        // units fit into one (CONFIRMED comparison in `0x0091A560`: the carried army's units plus the
        // boarding army's against the carried army's maximum: its `+0x48`, the units per army `0x008D3920`).
        let carried = self.passengers_of(navy).first().copied();
        let carried_units = carried.and_then(|a| self.world.forces.get(&a)).map_or(0, |a| a.units.len());
        if army_units + carried_units > self.max_units(false) {
            return Err(CommandError::Unsupported("the navy cannot carry this army"));
        }
        let navy_pos = self.force_position(navy).ok_or(CommandError::UnknownForce(navy))?;
        let here = to_f32(self.world.characters[&c].position);
        // Where to board: the embark point the army reaches most cheaply.
        let target = match self.poly_and_roads() {
            Some((pm, roads, _)) => {
                let mut best: Option<(f32, (f32, f32))> = None;
                for (_, pt) in embark_points(pm, to_f32(navy_pos)).into_iter().take(8) {
                    let cost = if (pt.0 - here.0).abs() < 1e-3 && (pt.1 - here.1).abs() < 1e-3 {
                        0.0
                    } else {
                        match pm.find_path(here, pt, Mover::Land, &roads) {
                            Some(p) => *p.costs.last().unwrap_or(&f32::INFINITY),
                            None => continue,
                        }
                    };
                    if best.is_none_or(|(b, _)| cost < b) {
                        best = Some((cost, pt));
                    }
                }
                best.ok_or(CommandError::NoPath)?.1
            }
            None => to_f32(navy_pos),
        };
        let mut events = Vec::new();
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) < 1e-6;
        if !close(here, target) {
            let w = self.walk(c, to_fixed(target), 0.0)?;
            events.extend(w.events);
            if !w.arrived {
                return Ok(events);
            }
        }
        self.leave_garrison(c);
        let from = to_f32(self.world.characters[&c].position);
        // Boarding itself spends no action points (INFERRED: the boarding code `0x0091A560` sets none;
        // the walk to the fleet did): an army that walked to its fleet has spent part of its turn,
        // so it cannot land again before next turn (the landing rule).
        self.world.characters.get_mut(&c).expect("checked").position = navy_pos;
        events.push(CampaignEvent::CharacterMoved { character: c, path: vec![from, to_f32(navy_pos)] });
        events.push(CampaignEvent::CharacterEmbarksNavy { character: c, navy });
        match carried {
            Some(a) => self.merge_units(force, a, &mut events),
            None => {
                self.world.embarked.insert(force, navy);
            }
        }
        Ok(events)
    }

    /// `Disembark` (or any move order to an embarked army): the navy sails towards the landing and
    /// lands the army when the landing is reached this turn (landing ends the army's turn); else the
    /// navy gets as far as it can with the army aboard.
    pub(crate) fn disembark(&mut self, force: ForceId, to: (Fixed20, Fixed20)) -> Result<Vec<CampaignEvent>, CommandError> {
        let c = self.commander_of(force)?;
        let navy = self.carrier_of(force).ok_or(CommandError::Unsupported("the army is not aboard a navy"))?;
        let nc = self.world.forces.get(&navy).and_then(|f| f.commander).ok_or(CommandError::NoCommander(navy))?;
        let plan = self.plan_transport(force, to).ok_or(CommandError::NoPath)?;
        let landing = plan.landing();
        let reach = plan.reachable_this_turn();
        let last_sea = landing.map_or(plan.points.len() - 1, |l| l - 1);
        let fleet_end = reach.min(last_sea);
        let lands = landing.is_some_and(|l| reach >= l);
        if fleet_end == 0 && !lands {
            let max = self.world.characters[&c].max_movement_points.max(1) as f32;
            let needed = (plan.costs.get(1).copied().unwrap_or(1.0) - plan.costs[0]).max(0.0) * max;
            return Err(CommandError::NotEnoughMovementPoints { needed: needed.ceil() as i32, available: self.world.characters[&nc].movement_points });
        }
        let mut events = Vec::new();
        if fleet_end > 0 {
            let ch = self.world.characters.get_mut(&nc).expect("checked");
            let used = ((plan.costs[fleet_end] - plan.costs[0]) * ch.max_movement_points as f32).ceil() as i32;
            let had = ch.movement_points;
            ch.movement_points = (ch.movement_points - used).max(0);
            ch.position = to_fixed(plan.points[fleet_end]);
            events.push(CampaignEvent::CharacterMoved { character: nc, path: plan.points[..=fleet_end].to_vec() });
            if had > 0 && ch.movement_points == 0 {
                events.push(CampaignEvent::MovementPointsExhausted { character: nc });
            }
            self.sync_passengers(nc);
        }
        if let Some(l) = landing
            && lands
        {
            let ch = self.world.characters.get_mut(&c).expect("checked");
            ch.position = to_fixed(plan.points[l]);
            ch.movement_points = 0;
            self.world.embarked.remove(&force);
            events.push(CampaignEvent::CharacterMoved { character: c, path: vec![plan.points[l - 1], plan.points[l]] });
            events.push(CampaignEvent::CharacterDisembarksNavy { character: c, navy });
        }
        Ok(events)
    }

    /// Drops embark entries whose army or navy no longer exists (called after forces are removed).
    pub(crate) fn tidy_embarked(&mut self) {
        let forces = &self.world.forces;
        self.world.embarked.retain(|a, n| forces.contains_key(a) && forces.contains_key(n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::polypath::{CellInput, PolyInput};

    const U: i32 = 1 << 20;

    /// A `w` x `h` map of 2-unit cells. `kinds(c, r)` gives the cell's polygons from west to east
    /// as (kind, width in units); one entry = the whole cell.
    fn split_map(w: u32, h: u32, kinds: impl Fn(u32, u32) -> Vec<(u8, i32)>) -> PolyMap {
        let mut cells = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let (x0, z0) = (c as i32 * 2 * U, r as i32 * 2 * U);
                let mut polys = Vec::new();
                let mut x = x0;
                for (k, units) in kinds(c, r) {
                    let x1 = x + units * U;
                    polys.push(PolyInput { kind: k, region_id: 0, outline: vec![(x, z0), (x1, z0), (x1, z0 + 2 * U), (x, z0 + 2 * U)] });
                    x = x1;
                }
                cells.push(CellInput { header: [50; 8], polys });
            }
        }
        PolyMap::build((0, 0), 2 * U, w, h, &cells, vec![vec![0]])
    }

    fn map(w: u32, h: u32, kind: impl Fn(u32, u32) -> u8) -> PolyMap {
        split_map(w, h, |c, r| vec![(kind(c, r), 2)])
    }

    /// Sea in x 0..9, a 1-unit coastal strip at x 9..10 (the east half of column 4), land from
    /// x 10 (column 5).
    fn coast_map() -> PolyMap {
        split_map(10, 6, |c, _| match c {
            0..=3 => vec![(kind::SEA, 2)],
            4 => vec![(kind::SEA, 1), (2, 1)],
            _ => vec![(kind::LAND, 2)],
        })
    }

    fn movers(spent: f32) -> Transport {
        Transport { fleet_max_ap: 60.0, fleet_spent: 0.0, army_max_ap: 20.0, army_spent: spent }
    }

    #[test]
    fn polygon_point_is_the_left_end_of_the_lowest_span() {
        // A square: its bottom-left corner.
        let pm = map(2, 2, |_, _| kind::LAND);
        assert_eq!(polygon_point(&pm, 3), (2.0, 2.0));
        // A diamond (pointed bottom): the lowest level has no width, the next level (the side
        // corners) gives its left corner.
        let d = PolyInput { kind: kind::LAND, region_id: 0, outline: vec![(U, 0), (2 * U, U), (U, 2 * U), (0, U)] };
        let pm = PolyMap::build((0, 0), 2 * U, 1, 1, &[CellInput { header: [50; 8], polys: vec![d] }], vec![vec![0]]);
        assert_eq!(polygon_point(&pm, 0), (0.0, 1.0));
    }

    #[test]
    fn landing_position_is_the_nearest_valid_land_point_within_1_5_units() {
        let pm = coast_map();
        // The coastal strip of row 2 (x 9..10, z 4..6): its point is (9, 4); land starts at x 10.
        let coast = (0..pm.len()).find(|&p| pm.kind[p] == 2 && pm.cell_rc(p) == (4, 2)).unwrap();
        assert_eq!(polygon_point(&pm, coast), (9.0, 4.0));
        let (p, pos) = landing_of(&pm, coast).expect("a landing position");
        assert_eq!(pm.kind[p], kind::LAND);
        // The nearest point (10, 4) lies on the shared edge: valid as it is or moved at most 0.001
        // into the land.
        assert!((pos.0 - 10.0).abs() < 0.0011 && (pos.1 - 4.0).abs() < 0.0011, "{pos:?}");
        let inside = pm.polygon_at(pos.0, pos.1).unwrap();
        assert_eq!(pm.kind[inside], kind::LAND);
        // A sea polygon is not a coast; a point 1.5 units or more from land has no landing position.
        assert!(landing_of(&pm, (0..pm.len()).find(|&p| pm.kind[p] == kind::SEA).unwrap()).is_none());
        assert!(land_position(&pm, (8.4, 5.0)).is_none());
    }

    #[test]
    fn a_fleet_lands_its_army_and_landing_ends_the_turn() {
        let pm = coast_map();
        let road = vec![1.0];
        let p = find_transport_path(&pm, (1.0, 5.0), (15.0, 5.0), Mover::Land, &[], &[], &road, &movers(0.0)).expect("a path");
        let l = p.landing().expect("lands");
        assert!(p.sea[..l].iter().all(|&s| s) && p.sea[l..].iter().all(|&s| !s));
        // The fleet sails ~8 units of 60 AP: well within the turn; the landing jumps to turn 1.
        assert!(p.costs[l - 1] < 1.0);
        assert!((p.costs[l] - 1.0).abs() < 1e-5, "{:?}", p.costs);
        assert_eq!(p.reachable_this_turn(), l);
        // The army walks on next turn.
        assert!(*p.costs.last().unwrap() > 1.0);
        assert_eq!(*p.points.last().unwrap(), (15.0, 5.0));
    }

    #[test]
    fn an_army_that_moved_cannot_land_this_turn() {
        let pm = coast_map();
        let p = find_transport_path(&pm, (1.0, 5.0), (11.0, 5.0), Mover::Land, &[], &[], &[1.0], &movers(0.25)).expect("a path");
        let l = p.landing().unwrap();
        assert!((p.costs[l] - 1.25).abs() < 1e-5, "{:?}", p.costs);
        assert!(p.reachable_this_turn() < l);
    }

    /// A port on a kind 7 cell (3, 2): sea in x 0..5, the coastal strip around the port, land from
    /// x 9. Its harbour footprint reaches the sea polygon of cell (2, 2).
    fn port_map() -> (PolyMap, (f32, f32), Harbour) {
        // Like the real ports, the kind 7 polygon touches only the coastal strip (kind 2).
        let pm = split_map(8, 5, |c, r| match (c, r) {
            (0..=1, _) => vec![(kind::SEA, 2)],
            (2, _) => vec![(kind::SEA, 1), (2, 1)],
            (3, 2) => vec![(kind::SHARED, 2)],
            (3, _) => vec![(2, 2)],
            (4, _) => vec![(2, 1), (kind::LAND, 1)],
            _ => vec![(kind::LAND, 2)],
        });
        let port = (6.4, 4.4);
        let h = Harbour { port, dock: (5.5, 5.0), outline: vec![(4.6, 4.2), (7.0, 4.2), (7.0, 5.8), (4.6, 5.8)] };
        (pm, port, h)
    }

    #[test]
    fn fleets_enter_ports_through_the_port_node_only() {
        let (pm, port, _) = port_map();
        let k7 = (0..pm.len()).find(|&p| pm.kind[p] == kind::SHARED).unwrap();
        // The port lies within the root of 2 of the kind 7 polygon's point (6, 4).
        assert_eq!(polygon_point(&pm, k7), (6.0, 4.0));
        let open = [Port { position: port, open: true }];
        let p = find_transport_path(&pm, (1.0, 1.0), port, Mover::Sea, &open, &[], &[1.0], &movers(0.0)).expect("a path");
        assert_eq!(*p.points.last().unwrap(), port);
        assert!(p.sea.iter().all(|&s| s));
        // Without an open port the kind 7 polygon cannot be entered by an ordinary step.
        let closed = [Port { position: port, open: false }];
        assert!(find_transport_path(&pm, (1.0, 1.0), port, Mover::Sea, &closed, &[], &[1.0], &movers(0.0)).is_none());
        assert_eq!(port_near(&pm, k7, &[Port { position: (8.0, 5.5), open: true }]), None, "too far");
    }

    #[test]
    fn fleets_leave_ports_through_the_harbour_footprint() {
        let (pm, port, h) = port_map();
        let hs = std::slice::from_ref(&h);
        let exits = harbour_exits(&pm, hs, port);
        assert!(!exits.is_empty() && exits.iter().all(|&q| pm.kind[q] == kind::SEA), "{exits:?}");
        let p = find_transport_path(&pm, port, (1.0, 1.0), Mover::Sea, &[], hs, &[1.0], &movers(0.0)).expect("out of the port");
        assert_eq!(*p.points.last().unwrap(), (1.0, 1.0));
        // Without the harbour the port is a dead end.
        assert!(find_transport_path(&pm, port, (1.0, 1.0), Mover::Sea, &[], &[], &[1.0], &movers(0.0)).is_none());
    }

    #[test]
    fn landing_out_of_a_kind_7_polygon_costs_a_whole_turn() {
        // A fleet starting in the port (kind 7, g = 0) lands the army on the coast east of it:
        // ceil(g) + 1 = 1 turn.
        let (pm, port, h) = port_map();
        let p = find_transport_path(&pm, port, (11.0, 5.0), Mover::Land, &[], std::slice::from_ref(&h), &[1.0], &movers(0.0)).expect("a path");
        let l = p.landing().expect("lands");
        assert_eq!(pm.kind[p.polys[l - 1] as usize], kind::SHARED, "{:?}", p.polys);
        assert!((p.costs[l] - (p.costs[l - 1].ceil() + 1.0)).abs() < 1e-5, "{:?}", p.costs);
    }

    #[test]
    fn embark_points_are_the_landing_positions_around_the_fleet() {
        let pm = coast_map();
        let pts = embark_points(&pm, (8.5, 5.0));
        assert!(!pts.is_empty());
        assert!(pts.iter().all(|&(p, pos)| pm.kind[p] == kind::LAND && pos.0 >= 10.0 - 1e-4));
        assert!(embark_points(&pm, (1.0, 5.0)).is_empty(), "no coast near");
    }
}

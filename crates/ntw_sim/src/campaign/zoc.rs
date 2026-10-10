//! Zones of control: the original's character obstacles (`analysis/campaign/PATHFINDING.md` §8,
//! `PATHFINDING_PORTS.md` §9).
//!
//! Every force commander is an obstacle with two shapes (CONFIRMED structure):
//! - the **zone**: the polygons reachable from the character within [`ARMY_ZONE`] map units
//!   (an army) or [`NAVY_ZONE`] (a navy), measured as pure walking distance over the polygons
//!   its own force may enter (no terrain bytes, no roads), a neighbour being taken while the
//!   distance so far plus half the step stays within the limit (CONFIRMED `0x00ACA170`,
//!   `0x00B11820`); +[`GARRISON_BONUS`] for a commander inside a settlement (INFERRED);
//! - the **core** round the character itself: a 24-sided polygon of radius [`CORE_RADIUS`] (1 map
//!   unit) centred on it (CONFIRMED `0x009CC0E0`; a 0.25-unit triangle for a commander in a special
//!   state, `0x009D3CB0`, not modelled).
//!
//! The limits: 6 for an army, 12 for a navy (CONFIRMED `0x00A2A1C0` with the army / fleet tests
//! `0x009D0F50` / `0x009D0F80` of `PATHFINDING_PORTS.md` §9.2; the saved obstacle boxes of the
//! eur_napoleon start position match these floods, `ntw_campaign` test `zoc_install.rs`).
//!
//! For a moving character, the obstacle of character C counts when C is not the mover, both are
//! navies or both are not (agents meet armies; CONFIRMED vtable slot 4 `0x00B57260`, which compares
//! the fleet test), and they are closer than [`RELEVANCE_DISTANCE`]. Then (CONFIRMED mode rules,
//! `0x00B3F4F0` → `0x00B69C20`; at-war test INFERRED):
//! - C's faction is the mover's or not at war with it: the **core** is cut in as kind 9 (nobody
//!   enters);
//! - at war: the **zone** too, as kind 8 (agents may enter it, armies and navies may not).
//!
//! The shapes are cut into the map by [`super::rtcut`] (the original's run-time cutter): the zone
//! changes its polygons' kinds, the core is clipped exactly.
//!
//! Around the ends of a search (CONFIRMED, `PATHFINDING_PORTS.md` §12): an enemy zone holding the
//! mover's start is dropped for the search (mode 1, core only, `0x00B65FC0`); an enemy zone holding
//! the goal is cut as kind 10 (mode 4, `0x00B551A0` → `0x00B54B00`): the mover may walk in to its
//! goal but cannot cross it, as nothing but kinds 10, 11 and 7 follow a kind 10 polygon.
//!
//! The order's target (CONFIRMED, `0x00AF5E80` → `0x00B54900` / `0x00B54E50` → `0x00B0D9C0`): for the
//! search the target character's obstacle goes to mode 2 (its zone and core layer re-kinded 8 → 10,
//! 9 → 11, `0x00B545B0`), or mode 3 (core only) from mode 1, so the mover may walk in and end there
//! but not cross it; a target building's obstacle (id = building +0x158 | 0x40000000, run-time
//! only) gets the same. PROVISIONAL: the target is the character within [`TARGET_DISTANCE`] of the
//! goal (the original is told it). An obstacle hidden from the mover's
//! faction (mode 5: outside its sight, or a hidden character it has not exposed) is left out by the
//! caller (`plan_path`, CHARACTERS_FIDELITY.md §10; the stealth test that hides armies is not
//! ported).

use std::collections::BinaryHeap;

use super::polypath::{dir_index, Mover, Overlay, PolyMap, View, DIR_LEN};
use super::rtcut::{self, Cut, Shape};

/// An army's zone limit in map units (CONFIRMED `0x00A2A1C0`: 6.0 for the army test `0x009D0F50`).
pub const ARMY_ZONE: f32 = 6.0;
/// A navy's zone limit in map units (CONFIRMED `0x00A2A1C0`: 12.0 for the fleet test `0x009D0F80`).
pub const NAVY_ZONE: f32 = 12.0;
/// Extra zone for a commander inside a settlement (CONFIRMED value 2.0; condition INFERRED).
pub const GARRISON_BONUS: f32 = 2.0;
/// The core's radius in map units (CONFIRMED `0x009CC0E0`: a 24-gon of radius 1).
pub const CORE_RADIUS: f32 = 1.0;
/// The core of a character inside a settlement: a triangle of this radius (CONFIRMED
/// `0x009CC0E0`, special state `0x009D3CB0`; the saved cell ranges fit it).
pub const GARRISON_CORE_RADIUS: f32 = 0.25;
/// Obstacles further away than this from the mover are ignored (CONFIRMED: distance² < 40000).
pub const RELEVANCE_DISTANCE: f32 = 200.0;
/// A character this close to the goal is taken as the order's target (PROVISIONAL stand-in: the
/// original is told the target, `0x00AF5E80`); its obstacle is cut in mode 2 / 3.
pub const TARGET_DISTANCE: f32 = 3.0;
/// The run-time kind of a zone (CONFIRMED `0x00B09030`).
pub const ZONE_KIND: u8 = 8;
/// The run-time kind of an enemy zone that holds the mover's goal: mode 4 re-kinds the zone layer's
/// kind 8 to 10 (`0x00B54B00` → `0x00B54AE0`, CONFIRMED): every mover may enter it, but from a kind
/// 10 polygon only kinds 10, 11 and 7 follow (kind link table), so a path can end inside, not cross.
pub const GOAL_ZONE_KIND: u8 = 10;
/// The run-time kind of a character's core (CONFIRMED obstacle kind field 9).
pub const CORE_KIND: u8 = 9;
/// The run-time kind of the order target's core: mode 2 / 3 re-kind 9 to 11 (`0x00B545B0` →
/// `0x00B54570`, CONFIRMED): from 11 only 11 follows, so a path may end in it.
pub const TARGET_CORE_KIND: u8 = 11;

#[derive(Clone, Copy, PartialEq)]
struct Open(f32, usize);
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

/// The polygons reachable from `pos` within `limit` map units of walking distance for `mover`
/// (the zone flood, see the module docs). Empty if `pos` has no enterable polygon nearby or the
/// limit is not positive.
pub fn reach(map: &PolyMap, pos: (f32, f32), mover: Mover, limit: f32) -> Vec<u32> {
    reach_in(&map.view(), pos, mover, limit)
}

/// [`reach`] on a map with run-time polygons (the original floods with the other obstacles cut
/// in, `0x00B7BAF0` → `0x00AF5200`); the result indexes the view (pieces included).
pub fn reach_in(map: &View<'_>, pos: (f32, f32), mover: Mover, limit: f32) -> Vec<u32> {
    if limit <= 0.0 {
        return Vec::new();
    }
    let Some(sp) = map.locate(pos.0, pos.1, mover, 2) else { return Vec::new() };
    let mut g: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
    let mut heap = BinaryHeap::new();
    g.insert(sp, 0.0);
    heap.push(Open(0.0, sp));
    let cols = map.map.cols;
    let rc = |p: usize| {
        let c = map.poly_cell(p);
        ((c % cols) as i32, (c / cols) as i32)
    };
    while let Some(Open(d, p)) = heap.pop() {
        if g.get(&p).is_some_and(|&b| d > b) {
            continue;
        }
        let (c0, r0) = rc(p);
        for &nb in map.neighbours(p) {
            let nb = nb as usize;
            if !mover.may_enter(map.kind(nb)) {
                continue;
            }
            let (c1, r1) = rc(nb);
            let dir = dir_index(c1 - c0, r1 - r0);
            let step = if dir == 8 {
                0.0
            } else if p == sp {
                let c = map.map.centre(map.poly_cell(nb));
                ((c.0 - pos.0).powi(2) + (c.1 - pos.1).powi(2)).sqrt()
            } else {
                map.map.cell * DIR_LEN[dir]
            };
            if d + step * 0.5 > limit {
                continue;
            }
            let nd = d + step;
            if g.get(&nb).is_none_or(|&b| nd < b) {
                g.insert(nb, nd);
                heap.push(Open(nd, nb));
            }
        }
    }
    let mut out: Vec<u32> = g.into_keys().map(|p| p as u32).collect();
    out.sort_unstable();
    out
}

/// One character obstacle as seen by a moving character.
#[derive(Debug, Clone, PartialEq)]
pub struct Obstacle {
    /// The character's position (map units).
    pub pos: (f32, f32),
    /// Whether its force is a navy.
    pub navy: bool,
    /// Whether it is inside a settlement (zone +[`GARRISON_BONUS`]).
    pub garrisoned: bool,
    /// Whether its faction is at war with the mover's.
    pub at_war: bool,
}

/// What a mover is, for [`cuts`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoverInfo {
    /// Position (map units).
    pub pos: (f32, f32),
    /// Navy (true) or army / agent (false).
    pub navy: bool,
    /// An agent (no force): it may enter zones (kind 8), not cores.
    pub agent: bool,
}

impl MoverInfo {
    /// The search's mover kind.
    pub fn mover(&self) -> Mover {
        if self.navy {
            Mover::Sea
        } else if self.agent {
            Mover::Agent
        } else {
            Mover::Land
        }
    }
}

/// The zone limit of a commander (module docs).
pub fn zone_limit(navy: bool, garrisoned: bool) -> f32 {
    (if navy { NAVY_ZONE } else { ARMY_ZONE }) + if garrisoned { GARRISON_BONUS } else { 0.0 }
}

/// A character's core: the 1-unit 24-gon, or the 0.25-unit triangle inside a settlement.
pub fn core_shape(pos: (f32, f32), garrisoned: bool) -> Vec<(f32, f32)> {
    if garrisoned { rtcut::triangle(pos) } else { rtcut::circle(pos, CORE_RADIUS) }
}

/// The shapes the obstacles cut for the mover heading for `goal`, in order (see the module docs).
pub fn cuts(map: &PolyMap, mover: MoverInfo, goal: (f32, f32), obstacles: &[Obstacle]) -> Vec<Cut> {
    let walk = if mover.navy { Mover::Sea } else { Mover::Land };
    let start = map.locate(mover.pos.0, mover.pos.1, walk, 2).map(|p| p as u32);
    let goal_poly = map.locate(goal.0, goal.1, walk, 2).map(|p| p as u32);
    let d2 = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2);
    let mut out = Vec::new();
    for o in obstacles {
        if o.navy != mover.navy {
            continue;
        }
        if d2(o.pos, mover.pos) >= RELEVANCE_DISTANCE * RELEVANCE_DISTANCE {
            continue;
        }
        // The order's target is cut in mode 2 (`0x00AF5E80` → `0x00B54900` → `0x00B0D9C0`, CONFIRMED):
        // its zone and core layer re-kinded 8 → 10 and 9 → 11, so the mover may walk into it and end
        // there but not cross it; mode 3 (core only, as 11) when it would have been mode 1.
        let target = d2(o.pos, goal) <= TARGET_DISTANCE * TARGET_DISTANCE;
        // Zones are left out for agents: kind 8 does not stop them anyway.
        if o.at_war && !mover.agent {
            let own = if o.navy { Mover::Sea } else { Mover::Land };
            let zone = reach(map, o.pos, own, zone_limit(o.navy, o.garrisoned));
            // A zone holding the start: mode 1, core only (`0x00B65FC0`); a zone holding the goal:
            // mode 4, the zone as kind 10 (`0x00B551A0`); otherwise mode 0, kind 8 (CONFIRMED).
            if !start.is_some_and(|sp| zone.binary_search(&sp).is_ok()) && !zone.is_empty() {
                let kind = if target || goal_poly.is_some_and(|gp| zone.binary_search(&gp).is_ok()) { GOAL_ZONE_KIND } else { ZONE_KIND };
                out.push(Cut { shape: Shape::Polygons(zone), kind });
            }
        }
        let r = if o.garrisoned { GARRISON_CORE_RADIUS } else { CORE_RADIUS };
        if d2(o.pos, mover.pos) >= r * r {
            out.push(Cut { shape: Shape::Convex(core_shape(o.pos, o.garrisoned)), kind: if target { TARGET_CORE_KIND } else { CORE_KIND } });
        }
    }
    out
}

/// The obstacles cut into `map` for the mover heading for `goal` ([`cuts`] through
/// [`rtcut::build`]); search with `map.with(&overlay)` and [`MoverInfo::mover`].
pub fn overlay(map: &PolyMap, mover: MoverInfo, goal: (f32, f32), obstacles: &[Obstacle]) -> Overlay {
    rtcut::build(map, &cuts(map, mover, goal, obstacles))
}

/// Who owns a character obstacle (the zone it gets, [`zone_limit`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// An army commander (zone 6, +2 garrisoned).
    Army,
    /// A navy commander (zone 12).
    Navy,
    /// A character without a force (an agent): core only.
    Agent,
}

/// One run-time polygon an obstacle cuts (see [`ObstacleRecord::pieces`]).
#[derive(Debug, Clone, PartialEq)]
pub struct RtPiece {
    /// The grid cell (row-major from the south-west).
    pub cell: u32,
    /// The static polygon it was cut from.
    pub origin: u32,
    /// Its kind (9 inside the core, the static kind outside).
    pub kind: u8,
    /// Its outline (map units).
    pub outline: Vec<(f32, f32)>,
}

/// What a save writer needs to give a character a pathfinder obstacle like the original's: the
/// `OBSTACLE` record of `CAMPAIGN_PATHFINDER/PATHFINDING_GRID[0]/OBSTACLE_LISTS/CHARACTER_OBSTACLE[]`
/// (`PATHFINDING_PORTS.md` §10.4). Fields are given in save units where the record has them.
#[derive(Debug, Clone, PartialEq)]
pub struct ObstacleRecord {
    /// `#1..#4`: the obstacle's mode state (`+0x64`, `+0x68`, `+0x6C`, `+0x70`, the last search's
    /// mode and two flags; values as the original's start positions hold them: commanders
    /// `1, 0, 0, 1`, agents `1, 1, 1, 1` (the most common values in the vanilla start positions);
    /// INFERRED harmless, the next search sets them again).
    pub mode: [u32; 4],
    /// `#6`: the kind field (9 for characters, CONFIRMED).
    pub kind: u32,
    /// `#7..#10` (Fixed20 in the save): the bounding box (x0, z0, x1, z1) in map units: the cell
    /// range of `#11..#14` (of `#15..#18` without a zone) widened by one more cell (CONFIRMED:
    /// equal for every start-position obstacle whose zone flood matches, `zoc_install.rs`).
    pub bbox: (f32, f32, f32, f32),
    /// `#11..#14` (col0, row0, col1, row1; u16 in the save, u32 here so a map may have any size):
    /// the cells the zone-and-core cut rebuilds (the rule of `0x00B0C240` on their bounding box;
    /// CONFIRMED on the same data); all 0 without a zone.
    pub zone_cells: (u32, u32, u32, u32),
    /// `#15..#18`: the same for the core alone (CONFIRMED on the same data, `zoc_install.rs`).
    pub core_cells: (u32, u32, u32, u32),
    /// The zone's polygons (static indices; empty for an agent): they turn kind 8 for factions at
    /// war with the owner.
    pub zone: Vec<u32>,
    /// The core outline ([`core_shape`]: 24 points of radius 1, or the 0.25 triangle inside a
    /// settlement), Fixed20-rounded.
    pub core: Vec<(f32, f32)>,
    /// The core's run-time polygons on the static map (the pieces of every cut cell, inside and
    /// outside the core), for the boundary pools of the record (`#0` slot 1 and the grid's
    /// `OBSTACLE_BOUNDARY_MANAGER`; written into a save by `ntw_campaign::grid_obstacle`).
    pub pieces: Vec<RtPiece>,
}

/// The [`ObstacleRecord`] of a character of `owner` standing at `pos` (`garrisoned`: inside a
/// settlement, for the zone bonus).
pub fn obstacle_record(map: &PolyMap, pos: (f32, f32), owner: Owner, garrisoned: bool) -> ObstacleRecord {
    let zone = match owner {
        Owner::Army => reach(map, pos, Mover::Land, zone_limit(false, garrisoned)),
        Owner::Navy => reach(map, pos, Mover::Sea, zone_limit(true, garrisoned)),
        Owner::Agent => Vec::new(),
    };
    let core = core_shape(pos, garrisoned);
    let bounds = |pts: &mut dyn Iterator<Item = (f32, f32)>| {
        pts.fold(None, |m: Option<(f32, f32, f32, f32)>, v| {
            Some(m.map_or((v.0, v.1, v.0, v.1), |m| (m.0.min(v.0), m.1.min(v.1), m.2.max(v.0), m.3.max(v.1))))
        })
    };
    let zone_pts = || zone.iter().flat_map(|&p| map.outline(p as usize).iter().copied());
    let all = bounds(&mut zone_pts().chain(core.iter().copied())).expect("the core has points");
    let c = map.cell;
    let (ox, oz) = map.origin;
    // The cells a cut rebuilds (CONFIRMED `0x00B0C240`): from the cell of (min − one cell + one
    // Fixed20 unit) to the cell of max plus one, clamped to the grid.
    let cd = f64::from(c);
    let tiny = 1.0 / 1_048_576.0 / cd;
    // (`open_top`: a max exactly on a cell edge does not reach into the next cell; this fits the
    // saved zone ranges, whose outlines end on cell edges, INFERRED: the zone outline stops a
    // Fixed20 unit short; the cores fit without it.)
    let range = |b: (f32, f32, f32, f32), open_top: bool| {
        let top = if open_top { tiny } else { 0.0 };
        let u = |v: f32, o: f32| (f64::from(v) - f64::from(o)) / cd;
        let lo = |v: f32, o: f32| (u(v, o) - 1.0 + tiny).floor().max(0.0) as u32;
        let hi = |v: f32, o: f32, n: u32| ((u(v, o) - top).floor() + 1.0).clamp(0.0, f64::from(n)) as u32;
        (lo(b.0, ox), lo(b.1, oz), hi(b.2, ox, map.cols), hi(b.3, oz, map.rows))
    };
    // Layer 0 (zone and core) and layer 1 (core only), `0x00B09030`.
    let zone_cells = if zone.is_empty() { (0, 0, 0, 0) } else { range(all, true) };
    let core_cells = range(bounds(&mut core.iter().copied()).expect("the core has points"), false);
    // The box: the outer range (the core's without a zone) widened by one more cell.
    let r = if zone.is_empty() { core_cells } else { zone_cells };
    let at = |i: u32, o: f32| o + i as f32 * c;
    let bbox = (at(r.0, ox) - c, at(r.1, oz) - c, at(r.2, ox) + 2.0 * c, at(r.3, oz) + 2.0 * c);
    let ov = rtcut::build(map, &[Cut { shape: Shape::Convex(core.clone()), kind: CORE_KIND }]);
    let view = map.with(&ov);
    let pieces = ov
        .rebuilt
        .iter()
        .flat_map(|&ci| view.cell_polys(ci as usize).map(move |p| (ci, p)))
        .map(|(ci, p)| RtPiece { cell: ci, origin: view.origin_of(p) as u32, kind: view.kind(p), outline: view.outline(p).to_vec() })
        .collect();
    let mode = if owner == Owner::Agent { [1, 1, 1, 1] } else { [1, 0, 0, 1] };
    ObstacleRecord { mode, kind: u32::from(CORE_KIND), bbox, zone_cells, core_cells, zone, core, pieces }
}

#[cfg(test)]
mod tests {
    use super::super::polypath::{kind, CellInput, PolyInput};
    use super::*;

    const U: i32 = 1 << 20;

    fn open(w: u32, h: u32) -> PolyMap {
        let mut cells = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let (x0, z0) = (c as i32 * 2 * U, r as i32 * 2 * U);
                let sq = vec![(x0, z0), (x0 + 2 * U, z0), (x0 + 2 * U, z0 + 2 * U), (x0, z0 + 2 * U)];
                cells.push(CellInput { header: [0; 8], polys: vec![PolyInput { kind: kind::LAND, region_id: 0, outline: sq }] });
            }
        }
        PolyMap::build((0, 0), 2 * U, w, h, &cells, vec![vec![0]])
    }

    #[test]
    fn zone_reaches_about_the_limit() {
        let m = open(30, 30);
        let z = reach(&m, (31.0, 31.0), Mover::Land, NAVY_ZONE);
        // Cell centres reached along the row: up to 12 + 1 units away (half-step rule).
        // (The army limit 6 reaches 6 + 1.)
        let row: Vec<f32> = z.iter().map(|&p| m.centre(m.poly_cell[p as usize])).filter(|c| c.1 == 31.0).map(|c| c.0).collect();
        let (lo, hi) = (row.iter().copied().fold(f32::INFINITY, f32::min), row.iter().copied().fold(0.0, f32::max));
        assert_eq!((lo, hi), (19.0, 43.0), "{row:?}");
        assert!(reach(&m, (31.0, 31.0), Mover::Land, 0.0).is_empty());
        let a = reach(&m, (31.0, 31.0), Mover::Land, ARMY_ZONE);
        let row: Vec<f32> = a.iter().map(|&p| m.centre(m.poly_cell[p as usize])).filter(|c| c.1 == 31.0).map(|c| c.0).collect();
        assert_eq!(row.iter().copied().fold(0.0, f32::max), 37.0);
        // The core: a 1-unit circle at a cell centre touches only that cell; at a corner, four.
        assert_eq!(m.polygons_within(31.0, 31.0, CORE_RADIUS).len(), 1);
        assert_eq!(m.polygons_within(30.0, 30.0, CORE_RADIUS).len(), 4);
    }

    #[test]
    fn enemies_cut_their_zone_friends_their_core() {
        let m = open(40, 20);
        let me = MoverInfo { pos: (3.0, 21.0), navy: false, agent: false };
        let enemy = Obstacle { pos: (41.0, 21.0), navy: false, garrisoned: false, at_war: true };
        let friend = Obstacle { at_war: false, ..enemy.clone() };
        let goal = (77.0, 21.0);
        let zone_ov = overlay(&m, me, goal, std::slice::from_ref(&enemy));
        let core_ov = overlay(&m, me, goal, std::slice::from_ref(&friend));
        let zv = m.with(&zone_ov);
        let cv = m.with(&core_ov);
        let blocked = |v: &crate::campaign::polypath::View<'_>| (0..v.len()).filter(|&p| !Mover::Land.may_enter(v.kind(p))).count();
        assert!(blocked(&zv) > 30, "{}", blocked(&zv));
        assert_eq!(blocked(&cv), 1);
        assert_eq!(cv.kind(cv.polygon_at(41.0, 21.0).unwrap()), CORE_KIND);
        // An agent passes the zone (kind 8), not the core; a navy ignores armies.
        let agent = MoverInfo { agent: true, ..me };
        let av = overlay(&m, agent, goal, std::slice::from_ref(&enemy));
        assert_eq!((0..m.with(&av).len()).filter(|&p| !Mover::Agent.may_enter(m.with(&av).kind(p))).count(), 1);
        assert!(overlay(&m, MoverInfo { navy: true, ..me }, goal, std::slice::from_ref(&enemy)).is_empty());
        // The order's target (mode 2): its zone turns 10 and its core 11; the path walks in and ends
        // on it.
        let t_ov = overlay(&m, me, enemy.pos, std::slice::from_ref(&enemy));
        let tv = m.with(&t_ov);
        assert_eq!(tv.kind(tv.polygon_at(enemy.pos.0, enemy.pos.1).unwrap()), TARGET_CORE_KIND);
        let to_t = tv.find_path(me.pos, enemy.pos, Mover::Land, &[]).unwrap();
        assert_eq!(*to_t.points.last().unwrap(), enemy.pos);
        assert!(to_t.polys.iter().all(|&p| tv.kind(p as usize) != ZONE_KIND));
        // The path bends round the enemy's zone and costs more than the straight line.
        let free = m.find_path(me.pos, goal, Mover::Land, &[]).unwrap();
        let bent = zv.find_path(me.pos, goal, Mover::Land, &[]).unwrap();
        assert!(bent.polys.iter().all(|&p| zv.kind(p as usize) != ZONE_KIND));
        assert!(bent.costs.last() > free.costs.last());
        // A goal inside the zone: the path stops at its edge (6 units round the enemy).
        let stop = zv.find_path(me.pos, (39.0, 21.0), Mover::Land, &[]).unwrap();
        let end = *stop.points.last().unwrap();
        assert!(end.0 < 36.0 && end.0 > 30.0, "{end:?}");
        // Sent into the zone (mode 4): the zone turns kind 10, the path enters it and ends at the
        // goal, and once inside it never steps back out.
        let in_ov = overlay(&m, me, (37.0, 21.0), std::slice::from_ref(&enemy));
        let iv = m.with(&in_ov);
        assert_eq!(iv.kind(iv.polygon_at(37.0, 21.0).unwrap()), GOAL_ZONE_KIND);
        let into = iv.find_path(me.pos, (37.0, 21.0), Mover::Land, &[]).unwrap();
        assert_eq!(*into.points.last().unwrap(), (37.0, 21.0));
        let first = into.polys.iter().position(|&p| iv.kind(p as usize) == GOAL_ZONE_KIND).unwrap();
        assert!(into.polys[first..].iter().all(|&p| iv.kind(p as usize) == GOAL_ZONE_KIND));
        // A path across a kind 10 zone to a goal beyond it goes round instead (no way out of 10).
        let beyond = iv.find_path((29.0, 21.0), goal, Mover::Land, &[]);
        assert!(beyond.is_none_or(|p| p.polys.iter().all(|&q| iv.kind(q as usize) != GOAL_ZONE_KIND)));
        // A friend's core: the path passes close by, round the 1-unit circle.
        let past = cv.find_path(me.pos, goal, Mover::Land, &[]).unwrap();
        assert!(past.polys.iter().all(|&p| cv.kind(p as usize) != CORE_KIND));
    }

    #[test]
    fn obstacle_record_box_follows_the_saved_rule() {
        let m = open(30, 30);
        // An agent-like core only (limit 0 is not a commander, so use the core alone): a
        // commander at (31.7, 30.5): its zone floods 6 units.
        let r = obstacle_record(&m, (31.7, 30.5), Owner::Army, false);
        assert_eq!(r.kind, 9);
        assert_eq!(r.core.len(), 24);
        assert!(r.bbox.0 <= 31.7 - 6.0 - 4.0 && r.bbox.2 >= 31.7 + 6.0 + 4.0, "{:?}", r.bbox);
        // Zone cells: one cell below the rounded box, up to its top cell edge.
        assert_eq!(r.zone_cells.0 as f32, (r.bbox.0 + 4.0) / 2.0 - 1.0);
        assert_eq!(r.zone_cells.2 as f32, (r.bbox.2 - 4.0) / 2.0);
        // The core at (31.7, 30.5) cuts columns 15..16 and rows 14..15: widened by one.
        assert_eq!(r.core_cells, (14, 13, 17, 16));
        assert!(r.pieces.iter().any(|p| p.kind == 9));
        // An agent: no zone.
        let a = obstacle_record(&m, (31.7, 30.5), Owner::Agent, false);
        assert_eq!((a.zone_cells, a.mode), ((0, 0, 0, 0), [1, 1, 1, 1]));
        assert_eq!((a.bbox.0, a.bbox.2), (26.0, 38.0));
    }
}

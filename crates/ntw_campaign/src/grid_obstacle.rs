//! A pathfinder obstacle for a new character, written into a save's `PATHFINDING_GRID` the way
//! the original stores one (`analysis/campaign/PATHFINDING_PORTS.md` §11, decoded from the writer
//! `0x00AFE020` and the vanilla saves and start positions).
//!
//! The grid keeps, per **layer** of a character obstacle (`0x00B09030`: layer 0 = zone and core,
//! layer 1 = core only; a character without a zone has layer 1 only), one **cell version** for
//! every cell of the layer's cell range (`#11..#14` / `#15..#18`, the cut cells) and of the ring of
//! cells around it (re-linked only):
//! - `#3 OBSTACLE_BOUNDARIES` (u32 array): per version `n`, `n` x (flags, link), the cell key
//!   (col | row << 16), a 0 byte as u32. A link is a static boundary's own link (region << 22 |
//!   outline list offset) or a run-time piece (region << 22 | 0x200000 | piece index); the flags are
//!   the kind (bits 0..3), for the cell itself and its 4 side cells a 4-bit mask of the polygons
//!   (counted among those the kind link table allows) it shares an edge with (bits 4..23: S, W,
//!   self, E, N), and a bit per direction with any such neighbour (bits 24..31, the search's
//!   direction index). Recomputed for every polygon of a version (CONFIRMED rule: it reproduces
//!   99.8 % of the static flags of nap_europe from the geometry).
//! - `#1` (u32 array) and `#0` (its count): the run-time pieces, each `n`, `n` Fixed20 points, and
//!   the number of versions using it (CONFIRMED: equal in every vanilla save).
//! - `#4 OBSTACLE_BASE_GRID_NODE[]`: one node per cell with versions: {key = the cell's first static
//!   boundary index, the cell's static boundary count, list 1 rows {version, x, true, pair list},
//!   list 2 rows {true, pair list}} where the pair list names the obstacle layers the version
//!   applies ((character | 2, layer) pairs) and x counts those for which the cell is only in the
//!   ring (CONFIRMED: x equals the BOUNDARIES high bit on every single-pair row). `#5` maps cell keys
//!   to nodes.
//! - `#2 OBSTACLE_BOUNDARY_MANAGER`: the set of pair lists.
//! - the `OBSTACLE` record: `BOUNDARIES` slot per layer = its versions in row-major order, ring
//!   versions with the high bit; `MANAGED_OBSTACLE_BOUNDARY` slot per layer = {true, its pair
//!   list}; `#1..#4` the mode state; `#6` = 9; `#7..#10` the box; `#11..#18` the cell ranges
//!   (`ntw_sim::campaign::zoc::obstacle_record`).
//!
//! PROVISIONAL: the pieces' shapes (our clipper: inside, and left / right / above / below the core;
//! the original's clipper output is not decoded beyond kinds), the zone flood (on the static map;
//! the original floods with the other obstacles cut in), protected polygons crossed by a core
//! keep their outline (the original adds the crossing points to it).

use std::collections::{BTreeMap, HashMap, HashSet};

use ntw_formats::campaign_pathfinding::{GridCell, PathfindingArea};
use ntw_formats::esf::{EsfNode, EsfRecord, EsfRecordArray};
use ntw_sim::campaign::polypath::{dir_index, PolyMap};
use ntw_sim::campaign::rtcut::{cut_polygon, links, resolve, PROTECTED};
use ntw_sim::campaign::zoc::{self, Owner};

/// The static grid of a campaign map: the pathfinding area, its expanded cells and our polygon
/// map built from them (polygon indices = the area's boundary indices).
pub struct StaticGrid<'a> {
    /// The area (`pathfinding.esf`).
    pub area: &'a PathfindingArea,
    /// Its cells (`GridData::expand`).
    pub cells: &'a [GridCell],
    /// Our polygon map of the same cells (`pathing::poly_map`).
    pub map: &'a PolyMap,
}

/// A character to give an obstacle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewObstacle {
    /// The character id.
    pub character: u32,
    /// Its position, Fixed20 (x, z) as in `LOCOMOTABLE`.
    pub pos: (i32, i32),
    /// Army or navy commander, or agent.
    pub owner: Owner,
    /// Inside a settlement (zone +2, triangle core).
    pub garrisoned: bool,
}

/// What [`add_character_obstacle`] added.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Added {
    /// Cell versions per layer (0 zone and core, 1 core).
    pub versions: [usize; 2],
    /// New run-time pieces.
    pub pieces: usize,
    /// New grid nodes.
    pub nodes: usize,
}

const FIX: f64 = 1_048_576.0;

/// A cut part wound clockwise (or flat) with less doubled area (Fixed20²) than this is a sliver and
/// is dropped: 1e-4 map units² (PROVISIONAL threshold; the slivers seen are about 1.4e-6 map
/// units²). Counter-clockwise parts are kept whatever their size.
const MIN_DOUBLED_AREA: i128 = (FIX * FIX * 2e-4) as i128;

/// Twice the signed area of an outline in Fixed20 (positive = counter-clockwise, the way the
/// original's pieces run).
fn doubled_area(o: &[(i32, i32)]) -> i128 {
    (0..o.len())
        .map(|i| {
            let (a, b) = (o[i], o[(i + 1) % o.len()]);
            i128::from(a.0) * i128::from(b.1) - i128::from(b.0) * i128::from(a.1)
        })
        .sum()
}

/// A polygon of a cell version.
#[derive(Debug, Clone, PartialEq)]
struct VPoly {
    kind: u8,
    region: u16,
    outline: Vec<(i32, i32)>,
    /// The static link when the outline is the static boundary's.
    link: Option<u32>,
}

impl<'a> StaticGrid<'a> {
    fn cols(&self) -> u32 {
        self.area.grid.cols
    }
    fn rows(&self) -> u32 {
        self.area.grid.rows
    }
    /// Exact outlines of a cell's static boundaries (as `pathing::poly_map` builds them).
    fn base(&self, ci: usize) -> Vec<VPoly> {
        let g = &self.area.grid;
        let w = g.cols as usize;
        let cs = g.cell_size;
        let (x0, z0) = (g.origin.0 + (ci % w) as i32 * cs, g.origin.1 + (ci / w) as i32 * cs);
        let corner = [(x0, z0), (x0, z0 + cs), (x0 + cs, z0), (x0 + cs, z0 + cs)];
        let cell = &self.cells[ci];
        cell.boundaries
            .iter()
            .map(|&b| {
                let outline = if cell.boundaries.len() == 1 {
                    vec![corner[0], corner[2], corner[3], corner[1]]
                } else {
                    let off = b.list_offset();
                    let n = self.area.lists.get(off).copied().unwrap_or(0) as usize;
                    self.area.lists.get(off + 1..off + 1 + n).unwrap_or(&[]).iter().filter_map(|&v| if v < 4 { Some(corner[v as usize]) } else { self.area.vertices.get(v as usize).copied() }).collect()
                };
                VPoly { kind: (b.flags & 0xF) as u8, region: b.region(), outline, link: Some(b.link) }
            })
            .collect()
    }
    fn key(&self, ci: usize) -> u32 {
        let w = self.cols() as usize;
        (ci % w) as u32 | (((ci / w) as u32) << 16)
    }
}

/// The core outline in Fixed20, as the original builds it (`0x009CC0E0`).
fn core_fixed(pos: (i32, i32), garrisoned: bool) -> Vec<(i32, i32)> {
    let (n, r) = if garrisoned { (3, 0.25) } else { (24, 1.0) };
    (0..n)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::TAU / f64::from(n);
            (pos.0 + (r * a.cos() * FIX).round() as i32, pos.1 + (r * a.sin() * FIX).round() as i32)
        })
        .collect()
}

/// One layer's versions: the cut cells (`inner`) and every cell of the ring around them.
fn layer_versions(sg: &StaticGrid<'_>, inner: CellRange, zone: &HashSet<usize>, core: &[(i32, i32)]) -> BTreeMap<usize, (bool, Vec<VPoly>)> {
    let (cols, rows) = (sg.cols() as i64, sg.rows() as i64);
    let core_q: Vec<(f64, f64)> = core.iter().map(|p| (f64::from(p.0) / FIX, f64::from(p.1) / FIX)).collect();
    let mut out = BTreeMap::new();
    for r in i64::from(inner.1) - 1..=i64::from(inner.3) + 1 {
        for c in i64::from(inner.0) - 1..=i64::from(inner.2) + 1 {
            if c < 0 || r < 0 || c >= cols || r >= rows {
                continue;
            }
            let ci = (r * cols + c) as usize;
            let is_inner = c >= i64::from(inner.0) && c <= i64::from(inner.2) && r >= i64::from(inner.1) && r <= i64::from(inner.3);
            let base = sg.base(ci);
            if !is_inner {
                out.insert(ci, (true, base));
                continue;
            }
            let first = sg.map.cell_first[ci] as usize;
            let mut version = Vec::new();
            for (k, mut p) in base.into_iter().enumerate() {
                if zone.contains(&(first + k)) {
                    p.kind = resolve(p.kind, zoc::ZONE_KIND);
                }
                if core.is_empty() || PROTECTED.contains(&p.kind) {
                    version.push(p);
                    continue;
                }
                let q: Vec<(f64, f64)> = p.outline.iter().map(|v| (f64::from(v.0) / FIX, f64::from(v.1) / FIX)).collect();
                let (ins, outs) = cut_polygon(&q, &core_q);
                if ins.is_empty() {
                    version.push(p);
                    continue;
                }
                if outs.is_empty() && ins.len() == 1 {
                    p.kind = resolve(p.kind, zoc::CORE_KIND);
                    version.push(p);
                    continue;
                }
                // Snap to the exact points of the polygon and the core, then round to Fixed20.
                let exact: Vec<(i32, i32)> = p.outline.iter().chain(core.iter()).copied().collect();
                let fix = |v: &(f64, f64)| -> (i32, i32) {
                    let r = ((v.0 * FIX).round() as i32, (v.1 * FIX).round() as i32);
                    exact.iter().copied().find(|e| (e.0 - r.0).abs() <= 64 && (e.1 - r.1).abs() <= 64).unwrap_or(r)
                };
                let kin = resolve(p.kind, zoc::CORE_KIND);
                for (k, part) in ins.iter().map(|i| (kin, i)).chain(outs.iter().map(|o| (p.kind, o))) {
                    let mut o: Vec<(i32, i32)> = part.iter().map(fix).collect();
                    o.dedup();
                    while o.len() > 1 && o.first() == o.last() {
                        o.pop();
                    }
                    // The original's used pieces always have 3+ points and run counter-clockwise
                    // (CONFIRMED in 11 vanilla saves, `save_check`). Snapping can fold a hair-thin
                    // part over (or flatten it): such slivers are dropped; a real part that comes
                    // out clockwise is turned round.
                    let a2 = doubled_area(&o);
                    if o.len() < 3 || (a2 <= 0 && -a2 < MIN_DOUBLED_AREA) {
                        continue;
                    }
                    if a2 < 0 {
                        o.reverse();
                    }
                    version.push(VPoly { kind: k, region: p.region, outline: o, link: None });
                }
            }
            out.insert(ci, (false, version));
        }
    }
    out
}

/// Do two outlines share an edge stretch (collinear, overlapping by more than a hair)?
fn share_edge(a: &[(i32, i32)], b: &[(i32, i32)]) -> bool {
    let q = |p: (i32, i32)| (f64::from(p.0) / FIX, f64::from(p.1) / FIX);
    for i in 0..a.len() {
        let (u, v) = (q(a[i]), q(a[(i + 1) % a.len()]));
        let len = ((v.0 - u.0).powi(2) + (v.1 - u.1).powi(2)).sqrt();
        if len < 1e-6 {
            continue;
        }
        let d = ((v.0 - u.0) / len, (v.1 - u.1) / len);
        for j in 0..b.len() {
            let (x, y) = (q(b[j]), q(b[(j + 1) % b.len()]));
            let off = |p: (f64, f64)| (p.0 - u.0) * d.1 - (p.1 - u.1) * d.0;
            if off(x).abs() > 3e-6 || off(y).abs() > 3e-6 {
                continue;
            }
            let t = |p: (f64, f64)| (p.0 - u.0) * d.0 + (p.1 - u.1) * d.1;
            let (tx, ty) = (t(x), t(y));
            if tx.max(ty).min(len) - tx.min(ty).max(0.0) > 1e-5 {
                return true;
            }
        }
    }
    false
}

/// The flags of every polygon of the versions (module docs), with `other(ci)` for cells without a
/// version.
fn flags_of(sg: &StaticGrid<'_>, versions: &BTreeMap<usize, (bool, Vec<VPoly>)>) -> HashMap<usize, Vec<u32>> {
    let (cols, rows) = (sg.cols() as i64, sg.rows() as i64);
    let cs = sg.area.grid.cell_size;
    let mut cache: HashMap<usize, Vec<VPoly>> = HashMap::new();
    let mut out = HashMap::new();
    for (&ci, (_ring, polys)) in versions {
        let (c, r) = ((ci as i64) % cols, (ci as i64) / cols);
        let mut flags = Vec::with_capacity(polys.len());
        for a in polys {
            let mut f = u32::from(a.kind);
            for dr in -1i64..=1 {
                for dc in -1i64..=1 {
                    let (cc, rr) = (c + dc, r + dr);
                    if cc < 0 || rr < 0 || cc >= cols || rr >= rows {
                        continue;
                    }
                    let cb = (rr * cols + cc) as usize;
                    let nb: &Vec<VPoly> = match versions.get(&cb) {
                        Some((_, v)) => v,
                        None => cache.entry(cb).or_insert_with(|| sg.base(cb)),
                    };
                    let d = dir_index(dc as i32, dr as i32);
                    let diag = dc != 0 && dr != 0;
                    let corner = (
                        sg.area.grid.origin.0 + (c + i64::from(dc > 0)) as i32 * cs,
                        sg.area.grid.origin.1 + (r + i64::from(dr > 0)) as i32 * cs,
                    );
                    let mut idx = 0u32;
                    for b in nb.iter() {
                        if cb == ci && std::ptr::eq(b, a) {
                            idx += u32::from(links(a.kind, b.kind));
                            continue;
                        }
                        if !links(a.kind, b.kind) {
                            continue;
                        }
                        let linked = if diag {
                            a.kind == b.kind && a.kind != 2 && a.outline.contains(&corner) && b.outline.contains(&corner)
                        } else {
                            share_edge(&a.outline, &b.outline)
                        };
                        if linked {
                            if d != 8 {
                                f |= 1 << (24 + d);
                            }
                            if !diag && idx < 4 {
                                let blk = ((dr + 1) * 2 + dc) as u32;
                                f |= 0x10 << (idx + blk * 4);
                            }
                        }
                        idx += 1;
                    }
                }
            }
            flags.push(f);
        }
        out.insert(ci, flags);
    }
    out
}

fn rec_array(name: &str, version: u8, items: Vec<Vec<EsfNode>>) -> EsfNode {
    EsfNode::RecordArray(Box::new(EsfRecordArray { name: name.to_string(), version, items }))
}

type CellRange = (u32, u32, u32, u32);

/// The `OBSTACLE` record's `#11..#18` (the zone and core cell ranges), which the save stores as
/// u16: a grid too wide for them cannot be written in the original's format (an error, checked
/// before the grid is touched; never truncated).
fn cell_range_fields(zone: CellRange, core: CellRange) -> Result<Vec<EsfNode>, String> {
    let (r, c) = (zone, core);
    [r.0, r.1, r.2, r.3, c.0, c.1, c.2, c.3]
        .into_iter()
        .map(|v| u16::try_from(v).map(EsfNode::U16))
        .collect::<Result<_, _>>()
        .map_err(|_| format!("obstacle cell range {r:?} / {c:?} does not fit the save's u16 fields"))
}

/// Adds the obstacle of `ob` to `grid` (the children of `PATHFINDING_GRID[0]`), as the original
/// would store it right after creating it (module docs). The character must not have one yet.
pub fn add_character_obstacle(grid: &mut [EsfNode], sg: &StaticGrid<'_>, ob: &NewObstacle) -> Result<Added, String> {
    let posf = (ob.pos.0 as f32 / FIX as f32, ob.pos.1 as f32 / FIX as f32);
    let rec = zoc::obstacle_record(sg.map, posf, ob.owner, ob.garrisoned);
    let cells = cell_range_fields(rec.zone_cells, rec.core_cells)?;
    let core = core_fixed(ob.pos, ob.garrisoned);
    let zone: HashSet<usize> = rec.zone.iter().map(|&p| p as usize).collect();
    let pair_owner = ob.character | 2;
    // Layers: (slot, cut cells, zone).
    type Layer = (u32, CellRange, HashSet<usize>);
    let mut layers: Vec<Layer> = Vec::new();
    if !zone.is_empty() {
        layers.push((0, rec.zone_cells, zone));
    }
    layers.push((1, rec.core_cells, HashSet::new()));

    // Positions of the parts of the grid.
    let idx_of = |grid: &[EsfNode], name: &str| grid.iter().position(|n| n.as_record().is_some_and(|r| r.name == name) || n.as_record_array().is_some_and(|a| a.name == name));
    let nodes_at = idx_of(grid, "OBSTACLE_BASE_GRID_NODE").ok_or("no grid nodes")?;
    let bounds_at = idx_of(grid, "OBSTACLE_BOUNDARIES").ok_or("no OBSTACLE_BOUNDARIES")?;
    let mgr_at = idx_of(grid, "OBSTACLE_BOUNDARY_MANAGER").ok_or("no boundary manager")?;
    let lists_at = idx_of(grid, "OBSTACLE_LISTS").ok_or("no OBSTACLE_LISTS")?;
    let map_at = nodes_at + 1;

    // Existing versions: count them (records follow one another in #3).
    let ob_u32 = |grid: &[EsfNode]| grid[bounds_at].as_record().and_then(|r| r.children.first()).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    let mut bounds = ob_u32(grid);
    let mut n_versions = 0u32;
    let mut j = 0;
    while j < bounds.len() {
        j += 3 + 2 * bounds[j] as usize;
        n_versions += 1;
    }
    let mut pool = grid[1].as_u32_array().ok_or("no piece pool")?.to_vec();
    let mut n_pieces = grid[0].as_u32().ok_or("no piece count")?;
    let mut cell_map: Vec<u32> = grid[map_at].as_u32_array().ok_or("no cell map")?.to_vec();
    let mut node_of: HashMap<u32, usize> = cell_map.chunks(2).map(|c| (c[0], c[1] as usize)).collect();

    let mut added = Added::default();
    let mut slots: [Vec<u32>; 6] = Default::default();
    let mut new_rows: Vec<(u32, u32, u32, Vec<u32>)> = Vec::new(); // (cell key, version, x, pairs)
    for (slot, inner, zone) in &layers {
        let core_here: &[(i32, i32)] = &core;
        let versions = layer_versions(sg, *inner, zone, core_here);
        let flags = flags_of(sg, &versions);
        let pairs = vec![pair_owner, *slot];
        for (&ci, (ring, polys)) in &versions {
            let v = n_versions;
            n_versions += 1;
            bounds.push(polys.len() as u32);
            for (k, p) in polys.iter().enumerate() {
                let link = match p.link {
                    Some(l) => l,
                    None => {
                        pool.push(p.outline.len() as u32);
                        for &(x, z) in &p.outline {
                            pool.push(x as u32);
                            pool.push(z as u32);
                        }
                        pool.push(1); // used by this one version
                        let idx = n_pieces;
                        n_pieces += 1;
                        added.pieces += 1;
                        (u32::from(p.region) << 22) | 0x20_0000 | idx
                    }
                };
                bounds.push(flags[&ci][k]);
                bounds.push(link);
            }
            bounds.push(sg.key(ci));
            bounds.push(0);
            slots[*slot as usize].push(v | if *ring { 0x8000_0000 } else { 0 });
            new_rows.push((sg.key(ci), v, u32::from(*ring), pairs.clone()));
            added.versions[*slot as usize] += 1;
        }
    }
    // Write back the pools.
    grid[0] = EsfNode::U32(n_pieces);
    grid[1] = EsfNode::U32Array(pool);
    if let Some(r) = grid[bounds_at].as_record() {
        let mut r = r.clone();
        r.children = vec![EsfNode::U32Array(bounds)];
        grid[bounds_at] = EsfNode::Record(Box::new(r));
    }
    // Grid nodes and the cell map.
    let (nodes_name, nodes_ver, mut items) = match &grid[nodes_at] {
        EsfNode::RecordArray(a) => (a.name.clone(), a.version, a.items.clone()),
        _ => return Err("grid nodes are not a record array".into()),
    };
    let mob_ver = items
        .iter()
        .find_map(|it| it.iter().find_map(EsfNode::as_record_array).map(|a| a.version))
        .unwrap_or(1);
    for (key, v, x, pairs) in &new_rows {
        let ni = match node_of.get(key) {
            Some(&n) => n,
            None => {
                let (col, row) = (key & 0xFFFF, key >> 16);
                let ci = (row * sg.cols() + col) as usize;
                let first = sg.map.cell_first[ci];
                let count = sg.map.cell_first[ci + 1] - first;
                items.push(vec![EsfNode::U32(first), EsfNode::U32(count), rec_array("MANAGED_OBSTACLE_BOUNDARY", mob_ver, Vec::new()), rec_array("MANAGED_OBSTACLE_BOUNDARY", mob_ver, Vec::new())]);
                let n = items.len() - 1;
                node_of.insert(*key, n);
                cell_map.push(*key);
                cell_map.push(n as u32);
                added.nodes += 1;
                n
            }
        };
        let node = &mut items[ni];
        let mut lists = node.iter_mut().filter_map(|n| match n {
            EsfNode::RecordArray(a) => Some(a),
            _ => None,
        });
        if let Some(l1) = lists.next() {
            l1.items.push(vec![EsfNode::U32(*v), EsfNode::U32(*x), EsfNode::Bool(true), EsfNode::U32Array(pairs.clone())]);
        }
        if let Some(l2) = lists.next() {
            l2.items.push(vec![EsfNode::Bool(true), EsfNode::U32Array(pairs.clone())]);
        }
    }
    grid[nodes_at] = EsfNode::RecordArray(Box::new(EsfRecordArray { name: nodes_name, version: nodes_ver, items }));
    grid[map_at] = EsfNode::U32Array(cell_map);
    // The boundary manager: the layers' pair lists.
    if let EsfNode::Record(m) = &mut grid[mgr_at] {
        for ch in m.children.iter_mut() {
            if let EsfNode::RecordArray(a) = ch
                && a.name == "OBSTACLE_BOUNDARY"
            {
                for (slot, _, _) in &layers {
                    let want = vec![pair_owner, *slot];
                    if !a.items.iter().any(|it| it.iter().find_map(EsfNode::as_u32_array) == Some(want.as_slice())) {
                        a.items.push(vec![EsfNode::U32Array(want)]);
                    }
                }
            }
        }
    }
    // The OBSTACLE record.
    let boundaries: Vec<Vec<EsfNode>> = slots.iter().map(|s| vec![EsfNode::U32Array(s.clone())]).collect();
    let managed: Vec<Vec<EsfNode>> = (0..6u32)
        .map(|k| if layers.iter().any(|l| l.0 == k) { vec![EsfNode::Bool(true), EsfNode::U32Array(vec![pair_owner, k])] } else { vec![EsfNode::Bool(false)] })
        .collect();
    let mode = if ob.owner == Owner::Agent { [1, 1, 1, 1] } else { [1, 0, 0, 1] };
    let fx = |v: f32| (f64::from(v) * FIX).round() as i32;
    let mut children = vec![rec_array("BOUNDARIES", 0, boundaries)];
    children.extend(mode.iter().map(|&m| EsfNode::U32(m)));
    children.push(rec_array("MANAGED_OBSTACLE_BOUNDARY", 1, managed));
    children.push(EsfNode::U32(rec.kind));
    children.extend([fx(rec.bbox.0), fx(rec.bbox.1), fx(rec.bbox.2), fx(rec.bbox.3)].map(EsfNode::I32));
    children.extend(cells);
    let obstacle = EsfRecord { name: "OBSTACLE".into(), version: 4, children };
    if let EsfNode::Record(lists) = &mut grid[lists_at] {
        if let Some(EsfNode::U32Array(ids)) = lists.children.get_mut(2) {
            ids.push(ob.character);
        }
        for ch in lists.children.iter_mut() {
            if let EsfNode::RecordArray(a) = ch
                && a.name == "CHARACTER_OBSTACLE"
            {
                a.items.push(vec![EsfNode::Record(Box::new(obstacle.clone())), EsfNode::U32(ob.character)]);
            }
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_ranges_past_u16_are_an_error() {
        // The model's ranges are u32 (a map of any size); the save's fields are u16.
        let ok = cell_range_fields((1, 2, 3, 4), (5, 6, 7, 65_535)).unwrap();
        assert_eq!(ok.len(), 8);
        assert_eq!(ok[7], EsfNode::U16(65_535));
        assert!(cell_range_fields((1, 2, 3, 4), (5, 6, 7, 65_536)).is_err());
    }
}

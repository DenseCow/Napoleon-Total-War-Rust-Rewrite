//! The original's run-time polygon cutter (`analysis/campaign/PATHFINDING_PORTS.md` §9.1):
//! obstacle shapes cut into the cells they cover, giving a [`polypath::Overlay`].
//!
//! What it follows (CONFIRMED unless tagged):
//! - an obstacle queues (outline, kind) shapes (`0x00B09030` / `0x00B69C20`): a character's zone as
//!   kind 8 and its core as kind 9; the cells under the shapes' bbox are rebuilt (`0x00B0C3A0`);
//! - a piece's kind (`0x00B13520`): a polygon of kind 2, 3, 5, 7, 9 or 11 keeps its kind (and its
//!   shape: the clipper merges such pieces back, `0x00ADEAB0`); otherwise the **last** shape
//!   covering the piece gives the kind, a kind 12 shape is transparent, and kind 10 over kind 8
//!   stays 8; a piece no shape covers keeps its kind ([`resolve`]);
//! - a piece reuses its polygon's slot, further pieces are appended (`0x00B13520`);
//! - the rebuilt cells' polygons are re-linked to the polygons of the 3 x 3 cells around them
//!   when they share an edge and the **kind link table** allows it (`0x00B79280`, `0x00B573C0`,
//!   [`links`]).
//!
//! Ours: a zone is given as the base polygons of its flood (the original's zone outline is the
//! outline of those polygons, so cutting it splits nothing: the polygons just change kind); a core
//! or any other outline is a convex polygon clipped exactly: inside part, plus the outside part as
//! convex wedges along the shape's edge lines (concave cell polygons are triangulated first).
//! Then every vertex lying on another polygon's edge in the rebuilt area is inserted into that
//! edge, so neighbours share whole edges (no T-junctions; PROVISIONAL detail: the original's
//! clipper output is not decoded beyond its kinds), and links are recomputed with an
//! edge-overlap test. Diagonal links keep the static rule (same kind, shared cell corner).

use std::collections::{BTreeSet, HashMap};

use super::polypath::{Overlay, PolyMap, NO_KIND};
use super::polysmooth::triangulate;

/// The kinds a cut never changes (CONFIRMED `0x00B13520`, `0x00B495D0`).
pub const PROTECTED: [u8; 6] = [2, 3, 5, 7, 9, 11];

/// The kind of a polygon of kind `cur` covered by a shape of kind `shape` (module docs).
pub fn resolve(cur: u8, shape: u8) -> u8 {
    if PROTECTED.contains(&cur) || shape == 12 || (shape == 10 && cur == 8) {
        cur
    } else {
        shape
    }
}

/// The kind link table (CONFIRMED `0x00B573C0`): may a rebuilt polygon of kind `a` link to a
/// neighbour of kind `b`?
pub fn links(a: u8, b: u8) -> bool {
    match a {
        0 | 6 => matches!(b, 0 | 6 | 8 | 10 | 11 | 7 | 5),
        1 => matches!(b, 1 | 10 | 11 | 7 | 4),
        4 => matches!(b, 0 | 6 | 1 | 10 | 11 | 7 | 5 | 4),
        5 => matches!(b, 0 | 6 | 5 | 4 | 10 | 11),
        7 | 8 => matches!(b, 0 | 6 | 1 | 8 | 10 | 11 | 7),
        10 => matches!(b, 10 | 11 | 7),
        11 => b == 11,
        _ => false,
    }
}

/// One shape to cut.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Whole base polygons (a zone of control: the flood's polygons).
    Polygons(Vec<u32>),
    /// A convex outline in map units (a core).
    Convex(Vec<(f32, f32)>),
}

/// A shape and the kind it gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    /// The shape.
    pub shape: Shape,
    /// Its kind (8 zone, 9 core, ...).
    pub kind: u8,
}

type Q = (f64, f64);
/// A point (x, z) in map units, for the cutting functions.
pub type Point = (f64, f64);

/// A polygon of a rebuilt cell while cutting.
#[derive(Debug, Clone)]
struct Piece {
    outline: Vec<Q>,
    kind: u8,
    region: u16,
    origin: u32,
}

const EPS: f64 = 2e-4;
const AREA_EPS: f64 = 1e-7;

fn cross(o: Q, a: Q, b: Q) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn area(p: &[Q]) -> f64 {
    (0..p.len()).map(|i| cross((0.0, 0.0), p[i], p[(i + 1) % p.len()])).sum::<f64>() * 0.5
}

fn convex(p: &[Q]) -> bool {
    let n = p.len();
    let (mut pos, mut neg) = (false, false);
    for i in 0..n {
        let c = cross(p[i], p[(i + 1) % n], p[(i + 2) % n]);
        if c > 1e-12 {
            pos = true;
        } else if c < -1e-12 {
            neg = true;
        }
    }
    !(pos && neg)
}

/// Sutherland–Hodgman: the part of convex `p` on the left of the line a → b (`left`) or right.
fn clip(p: &[Q], a: Q, b: Q, left: bool) -> Vec<Q> {
    let side = |q: Q| {
        let c = cross(a, b, q);
        if left { c } else { -c }
    };
    let mut out = Vec::with_capacity(p.len() + 2);
    for i in 0..p.len() {
        let (u, v) = (p[i], p[(i + 1) % p.len()]);
        let (su, sv) = (side(u), side(v));
        if su >= 0.0 {
            out.push(u);
        }
        if (su >= 0.0) != (sv >= 0.0) {
            let t = su / (su - sv);
            out.push((u.0 + (v.0 - u.0) * t, u.1 + (v.1 - u.1) * t));
        }
    }
    dedup(out)
}

fn dedup(mut v: Vec<Q>) -> Vec<Q> {
    v.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9);
    while v.len() > 1 && (v[0].0 - v[v.len() - 1].0).abs() < 1e-9 && (v[0].1 - v[v.len() - 1].1).abs() < 1e-9 {
        v.pop();
    }
    v
}

fn ccw(mut p: Vec<Q>) -> Vec<Q> {
    if area(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// Splits convex counter-clockwise `t` by convex counter-clockwise `c`: (inside, outside parts).
/// The outside is cut into few simple polygons by columns (PROVISIONAL shape: the original's
/// clipper output is not decoded beyond its kinds): the part left of `c`'s x-range, the part right
/// of it, and in between the parts above and below `c`. Inside empty: `t` untouched (returned as
/// the only outside part); outside empty: `t` wholly covered.
pub fn cut_convex(t: &[Point], c: &[Point]) -> (Vec<Point>, Vec<Vec<Point>>) {
    let mut inside = t.to_vec();
    for i in 0..c.len() {
        inside = clip(&inside, c[i], c[(i + 1) % c.len()], true);
        if inside.len() < 3 {
            break;
        }
    }
    let ta = area(t).abs();
    if inside.len() < 3 || area(&inside).abs() <= AREA_EPS {
        return (Vec::new(), vec![t.to_vec()]);
    }
    if (ta - area(&inside).abs()).abs() <= AREA_EPS.max(ta * 1e-9) {
        return (inside, Vec::new());
    }
    let span = |p: &[Q]| p.iter().fold((f64::MAX, f64::MIN), |m, q| (m.0.min(q.0), m.1.max(q.0)));
    let (tx0, tx1) = span(t);
    let (cx0, cx1) = span(c);
    let mut outside = Vec::new();
    // Left and right of c: convex clips of t by vertical lines.
    if tx0 < cx0 - 1e-12 {
        let l = clip(t, (cx0, 0.0), (cx0, 1.0), true);
        if l.len() >= 3 && area(&l).abs() > AREA_EPS {
            outside.push(l);
        }
    }
    if tx1 > cx1 + 1e-12 {
        let r = clip(t, (cx1, 1.0), (cx1, 0.0), true);
        if r.len() >= 3 && area(&r).abs() > AREA_EPS {
            outside.push(r);
        }
    }
    // The middle columns: above and below c.
    let (x0, x1) = (tx0.max(cx0), tx1.min(cx1));
    if x1 - x0 > 1e-12 {
        let mut xs: Vec<f64> = t.iter().chain(c.iter()).map(|q| q.0).filter(|&x| x > x0 && x < x1).collect();
        xs.push(x0);
        xs.push(x1);
        for i in 0..t.len() {
            for j in 0..c.len() {
                if let Some(x) = seg_cross_x(t[i], t[(i + 1) % t.len()], c[j], c[(j + 1) % c.len()])
                    && x > x0
                    && x < x1
                {
                    xs.push(x);
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        let cols: Vec<(f64, Point, Point)> = xs.iter().filter_map(|&x| Some((x, column(t, x)?, column(c, x)?))).collect();
        // Above c: from max(t low, c high) to t high; below: from t low to min(t high, c low).
        let above: Vec<(f64, f64, f64)> = cols.iter().map(|&(x, (tl, th), (_cl, ch))| (x, tl.max(ch), th)).collect();
        let below: Vec<(f64, f64, f64)> = cols.iter().map(|&(x, (tl, th), (cl, _ch))| (x, tl, th.min(cl))).collect();
        for band in [above, below] {
            outside.extend(bands(&band));
        }
    }
    (inside, outside)
}

/// [`cut_convex`] for any simple polygon `p` (either winding; a concave one is triangulated
/// first) and a convex `c`: (inside parts, outside parts), counter-clockwise. Both empty when
/// `p` is degenerate; `(vec![], vec![p])` when `c` misses it.
pub fn cut_polygon(p: &[Point], c: &[Point]) -> (Vec<Vec<Point>>, Vec<Vec<Point>>) {
    let p = ccw(dedup(p.to_vec()));
    let c = ccw(c.to_vec());
    if p.len() < 3 {
        return (Vec::new(), Vec::new());
    }
    let parts: Vec<Vec<Q>> = if convex(&p) {
        vec![p.clone()]
    } else {
        let f: Vec<(f32, f32)> = p.iter().map(|q| (q.0 as f32, q.1 as f32)).collect();
        triangulate(&f).into_iter().map(|t| ccw(t.iter().map(|q| (f64::from(q.0), f64::from(q.1))).collect())).collect()
    };
    let (mut ins, mut outs) = (Vec::new(), Vec::new());
    for part in &parts {
        let (i, o) = cut_convex(part, &c);
        if i.len() >= 3 {
            ins.push(i);
        }
        outs.extend(o);
    }
    if ins.is_empty() {
        return (Vec::new(), vec![p]);
    }
    (ins, outs)
}

/// The z-range of convex polygon `p` on the vertical line at `x` (None off it).
fn column(p: &[Q], x: f64) -> Option<(f64, f64)> {
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        if (a.0 - x).abs() < 1e-12 {
            lo = lo.min(a.1);
            hi = hi.max(a.1);
        }
        if (a.0 < x && b.0 > x) || (a.0 > x && b.0 < x) {
            let z = a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0);
            lo = lo.min(z);
            hi = hi.max(z);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// The x of the crossing of segments a-b and c-d, if they cross.
fn seg_cross_x(a: Q, b: Q, c: Q, d: Q) -> Option<f64> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-18 {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / den;
    let u = ((c.0 - a.0) * r.1 - (c.1 - a.1) * r.0) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(a.0 + r.0 * t)
}

/// Polygons of the bands (x, low, high) where high > low: each run of columns with a positive
/// height becomes one polygon (lows left to right, highs right to left), with the points where a
/// band closes added.
fn bands(cols: &[(f64, f64, f64)]) -> Vec<Vec<Q>> {
    let mut out = Vec::new();
    let mut run: Vec<(f64, f64, f64)> = Vec::new();
    let flush = |run: &mut Vec<(f64, f64, f64)>, out: &mut Vec<Vec<Q>>| {
        if run.len() >= 2 {
            let mut poly: Vec<Q> = run.iter().map(|&(x, lo, _)| (x, lo)).collect();
            poly.extend(run.iter().rev().map(|&(x, _, hi)| (x, hi)));
            let poly = simplify(dedup(poly));
            if poly.len() >= 3 && area(&poly).abs() > AREA_EPS {
                out.push(ccw(poly));
            }
        }
        run.clear();
    };
    for i in 0..cols.len() {
        let (x, lo, hi) = cols[i];
        let h = hi - lo;
        if h > 1e-9 {
            if run.is_empty() && i > 0 {
                // The band opens between the previous column and this one.
                let (px, plo, phi) = cols[i - 1];
                let ph = phi - plo;
                if ph < 0.0 {
                    let f = ph / (ph - h);
                    let xo = px + (x - px) * f;
                    let zo = plo + (lo - plo) * f;
                    run.push((xo, zo, zo));
                } else {
                    run.push((px, plo, plo));
                }
            }
            run.push((x, lo, hi));
        } else {
            if !run.is_empty() {
                let (px, plo, phi) = *run.last().expect("non-empty");
                let ph = phi - plo;
                if h < 0.0 {
                    let f = ph / (ph - h);
                    let xo = px + (x - px) * f;
                    let zo = plo + (lo - plo) * f;
                    run.push((xo, zo, zo));
                } else {
                    run.push((x, lo, lo));
                }
            }
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Drops points that lie on the straight line between their neighbours.
fn simplify(p: Vec<Q>) -> Vec<Q> {
    let mut v = p;
    let mut changed = true;
    while changed && v.len() > 3 {
        changed = false;
        for i in 0..v.len() {
            let (a, b, c) = (v[(i + v.len() - 1) % v.len()], v[i], v[(i + 1) % v.len()]);
            let len = ((c.0 - a.0).powi(2) + (c.1 - a.1).powi(2)).sqrt();
            if len > 0.0 && (cross(a, c, b) / len).abs() < 1e-9 && (b.0 - a.0) * (c.0 - b.0) + (b.1 - a.1) * (c.1 - b.1) >= 0.0 {
                v.remove(i);
                changed = true;
                break;
            }
        }
    }
    v
}

/// Cuts `cuts` (in order) into `map` (module docs).
pub fn build(map: &PolyMap, cuts: &[Cut]) -> Overlay {
    let mut ov = Overlay::new(map);
    let mut base_kind: Vec<u8> = Vec::new();
    // Rebuilt cells: their current polygons.
    let mut cells: HashMap<u32, Vec<Piece>> = HashMap::new();
    let to_q = |p: &(f32, f32)| (f64::from(p.0), f64::from(p.1));
    for cut in cuts {
        match &cut.shape {
            Shape::Polygons(ps) => {
                for &p in ps {
                    let cell = map.poly_cell[p as usize];
                    if let Some(pieces) = cells.get_mut(&cell) {
                        for pc in pieces.iter_mut().filter(|pc| pc.origin == p) {
                            pc.kind = resolve(pc.kind, cut.kind);
                        }
                    } else {
                        if base_kind.is_empty() {
                            base_kind = vec![NO_KIND; map.len()];
                        }
                        let cur = if base_kind[p as usize] == NO_KIND { map.kind[p as usize] } else { base_kind[p as usize] };
                        let k = resolve(cur, cut.kind);
                        if k != map.kind[p as usize] {
                            base_kind[p as usize] = k;
                        }
                    }
                }
            }
            Shape::Convex(outline) => {
                if outline.len() < 3 {
                    continue;
                }
                let c = ccw(outline.iter().map(to_q).collect());
                let (mut x0, mut z0, mut x1, mut z1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for q in &c {
                    x0 = x0.min(q.0);
                    z0 = z0.min(q.1);
                    x1 = x1.max(q.0);
                    z1 = z1.max(q.1);
                }
                let cs = f64::from(map.cell);
                let (ox, oz) = (f64::from(map.origin.0), f64::from(map.origin.1));
                let c0 = (((x0 - ox) / cs).floor() as i64).max(0);
                let c1 = (((x1 - ox) / cs).floor() as i64).min(i64::from(map.cols) - 1);
                let r0 = (((z0 - oz) / cs).floor() as i64).max(0);
                let r1 = (((z1 - oz) / cs).floor() as i64).min(i64::from(map.rows) - 1);
                for r in r0..=r1 {
                    for col in c0..=c1 {
                        let ci = (r as u32) * map.cols + col as u32;
                        let pieces = cells.entry(ci).or_insert_with(|| {
                            let range = map.cell_first[ci as usize]..map.cell_first[ci as usize + 1];
                            range
                                .map(|p| {
                                    let k = base_kind.get(p as usize).copied().filter(|&k| k != NO_KIND).unwrap_or(map.kind[p as usize]);
                                    Piece { outline: map.outline(p as usize).iter().map(to_q).collect(), kind: k, region: map.region_id[p as usize], origin: p }
                                })
                                .collect()
                        });
                        let mut next = Vec::with_capacity(pieces.len());
                        for pc in pieces.drain(..) {
                            if PROTECTED.contains(&pc.kind) || pc.outline.len() < 3 {
                                next.push(pc);
                                continue;
                            }
                            let shape = ccw(pc.outline.clone());
                            let parts: Vec<Vec<Q>> = if convex(&shape) {
                                vec![shape]
                            } else {
                                let f: Vec<(f32, f32)> = shape.iter().map(|q| (q.0 as f32, q.1 as f32)).collect();
                                triangulate(&f).into_iter().map(|t| ccw(t.iter().map(to_q).collect())).collect()
                            };
                            let mut ins = Vec::new();
                            let mut outs = Vec::new();
                            for part in &parts {
                                let (i, o) = cut_convex(part, &c);
                                if i.len() >= 3 {
                                    ins.push(i);
                                }
                                outs.extend(o);
                            }
                            if ins.is_empty() {
                                next.push(pc); // untouched
                                continue;
                            }
                            if outs.is_empty() && parts.len() == 1 {
                                next.push(Piece { kind: resolve(pc.kind, cut.kind), ..pc }); // wholly covered
                                continue;
                            }
                            let k = resolve(pc.kind, cut.kind);
                            for i in ins {
                                next.push(Piece { outline: i, kind: k, region: pc.region, origin: pc.origin });
                            }
                            for o in outs {
                                next.push(Piece { outline: o, kind: pc.kind, region: pc.region, origin: pc.origin });
                            }
                        }
                        *pieces = next;
                    }
                }
            }
        }
    }
    // Cells whose polygons were only re-kinded (wholly covered or untouched) need no rebuild.
    cells.retain(|&ci, pieces| {
        let range = map.cell_first[ci as usize]..map.cell_first[ci as usize + 1];
        let same = pieces.len() == range.len()
            && pieces.iter().zip(range).all(|(pc, p)| pc.origin == p && pc.outline.len() == map.outline(p as usize).len());
        if same {
            for pc in pieces.iter() {
                let p = pc.origin as usize;
                if pc.kind != map.kind[p] {
                    if base_kind.is_empty() {
                        base_kind = vec![NO_KIND; map.len()];
                    }
                    base_kind[p] = pc.kind;
                } else if !base_kind.is_empty() {
                    base_kind[p] = NO_KIND;
                }
            }
        }
        !same
    });
    if !base_kind.is_empty() {
        ov.base_kind = base_kind;
    }
    if cells.is_empty() {
        return ov;
    }
    // Index the pieces: the first piece of each base polygon keeps its slot.
    let base_len = map.len();
    let mut cell_keys: Vec<u32> = cells.keys().copied().collect();
    cell_keys.sort_unstable();
    let rebuilt: BTreeSet<u32> = cell_keys.iter().copied().collect();
    ov.rebuilt = cell_keys.clone();
    // The polygons of the area (rebuilt cells and their 8 neighbours) in one arena.
    let mut arena: Vec<APoly> = Vec::new();
    let mut by_cell: HashMap<u32, Vec<usize>> = HashMap::new();
    for &ci in &cell_keys {
        let mut used = BTreeSet::new();
        let mut list = Vec::new();
        for pc in &cells[&ci] {
            let f: Vec<(f32, f32)> = pc.outline.iter().map(|q| (q.0 as f32, q.1 as f32)).collect();
            let idx = if used.insert(pc.origin) {
                pc.origin
            } else {
                let i = (base_len + ov.kind.len()) as u32;
                ov.kind.push(pc.kind);
                ov.region_id.push(pc.region);
                ov.poly_cell.push(ci);
                ov.outline.push(Vec::new());
                ov.adj.push(Vec::new());
                ov.origin.push(pc.origin);
                ov.cell_extra.entry(ci).or_default().push(i);
                i
            };
            list.push(arena.len());
            arena.push(APoly::new(idx, ci, pc.kind, f, true));
        }
        // A base polygon that lost every piece (degenerate): no entry, no links.
        for p in map.cell_first[ci as usize]..map.cell_first[ci as usize + 1] {
            if !used.contains(&p) {
                list.push(arena.len());
                arena.push(APoly::new(p, ci, 9, Vec::new(), true));
            }
        }
        by_cell.insert(ci, list);
    }
    let cols = map.cols as i64;
    let rows = map.rows as i64;
    let around = |ci: u32| {
        let (c, r) = (i64::from(ci) % cols, i64::from(ci) / cols);
        (-1..=1).flat_map(move |dr| (-1..=1).map(move |dc| (c + dc, r + dr))).filter(|&(c, r)| c >= 0 && r >= 0 && c < cols && r < rows).map(move |(c, r)| (r * cols + c) as u32)
    };
    let mut area_cells: BTreeSet<u32> = BTreeSet::new();
    for &ci in &cell_keys {
        area_cells.extend(around(ci));
    }
    for &ci in &area_cells {
        if let std::collections::hash_map::Entry::Vacant(slot) = by_cell.entry(ci) {
            let mut list = Vec::new();
            for p in map.cell_first[ci as usize]..map.cell_first[ci as usize + 1] {
                let k = ov.base_kind.get(p as usize).copied().filter(|&k| k != NO_KIND).unwrap_or(map.kind[p as usize]);
                list.push(arena.len());
                arena.push(APoly::new(p, ci, k, map.outline(p as usize).to_vec(), false));
            }
            slot.insert(list);
        }
    }
    // Conform: insert every new vertex (from the rebuilt cells) that lies inside an edge of a
    // polygon of the same or an adjacent cell.
    for &ci in &area_cells {
        let mut verts: Vec<(f32, f32)> = around(ci)
            .filter(|c| rebuilt.contains(c))
            .flat_map(|c| by_cell[&c].iter())
            .flat_map(|&a| arena[a].outline.iter().copied())
            .collect();
        verts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        verts.dedup();
        if verts.is_empty() {
            continue;
        }
        for &a in &by_cell[&ci] {
            let o = &arena[a].outline;
            if o.len() < 2 {
                continue;
            }
            let bb = arena[a].bb;
            if !verts.iter().any(|v| v.0 >= bb.0 - 1e-3 && v.0 <= bb.2 + 1e-3 && v.1 >= bb.1 - 1e-3 && v.1 <= bb.3 + 1e-3) {
                continue;
            }
            let mut out: Vec<(f32, f32)> = Vec::with_capacity(o.len());
            let mut grew = false;
            for i in 0..o.len() {
                let (pa, pb) = (o[i], o[(i + 1) % o.len()]);
                out.push(pa);
                let (aq, bq) = ((f64::from(pa.0), f64::from(pa.1)), (f64::from(pb.0), f64::from(pb.1)));
                let len = ((bq.0 - aq.0).powi(2) + (bq.1 - aq.1).powi(2)).sqrt();
                if len <= EPS {
                    continue;
                }
                let (ex0, ex1) = (pa.0.min(pb.0) - 1e-3, pa.0.max(pb.0) + 1e-3);
                let (ez0, ez1) = (pa.1.min(pb.1) - 1e-3, pa.1.max(pb.1) + 1e-3);
                let mut on: Vec<(f64, (f32, f32))> = Vec::new();
                for &v in &verts {
                    if v == pa || v == pb || v.0 < ex0 || v.0 > ex1 || v.1 < ez0 || v.1 > ez1 {
                        continue;
                    }
                    let vq = (f64::from(v.0), f64::from(v.1));
                    let t = ((vq.0 - aq.0) * (bq.0 - aq.0) + (vq.1 - aq.1) * (bq.1 - aq.1)) / (len * len);
                    if t * len <= EPS || (1.0 - t) * len <= EPS {
                        continue;
                    }
                    if (cross(aq, bq, vq) / len).abs() <= EPS {
                        on.push((t, v));
                    }
                }
                if !on.is_empty() {
                    on.sort_by(|x, y| x.0.total_cmp(&y.0));
                    on.dedup_by(|x, y| x.1 == y.1);
                    out.extend(on.into_iter().map(|(_, v)| v));
                    grew = true;
                }
            }
            if grew {
                arena[a].outline = out;
                arena[a].conformed = true;
            }
        }
    }
    // Store outlines and kinds.
    for ap in &arena {
        if ap.rebuilt {
            store(&mut ov, map, ap.idx, &ap.outline, ap.kind);
        } else if ap.conformed {
            ov.base_outline.insert(ap.idx, ap.outline.clone());
        }
    }
    // Links of every polygon in the area: a rebuilt polygon against all polygons of the 3 x 3
    // cells (kind link table), a static one keeps its static links outside the rebuilt cells and
    // gains those into them.
    let edges: Vec<Vec<Edge>> = arena.iter().map(|ap| edges_of(&ap.outline)).collect();
    for &ci in &area_cells {
        let (ac, ar) = (i64::from(ci) % cols, i64::from(ci) / cols);
        let own_rebuilt = rebuilt.contains(&ci);
        for &a in &by_cell[&ci] {
            let pa = &arena[a];
            let mut list = Vec::new();
            if !own_rebuilt {
                list.extend(map.neighbours(pa.idx as usize).iter().copied().filter(|&b| !rebuilt.contains(&map.poly_cell[b as usize])));
            }
            if !pa.outline.is_empty() {
                for cb in around(ci) {
                    if !own_rebuilt && !rebuilt.contains(&cb) {
                        continue;
                    }
                    let (bc, br) = (i64::from(cb) % cols, i64::from(cb) / cols);
                    let diag = bc != ac && br != ar;
                    for &b in &by_cell[&cb] {
                        let pb = &arena[b];
                        if b == a || pb.outline.is_empty() {
                            continue;
                        }
                        if own_rebuilt && !links(pa.kind, pb.kind) {
                            continue;
                        }
                        if pa.bb.0 > pb.bb.2 + 1e-3 || pb.bb.0 > pa.bb.2 + 1e-3 || pa.bb.1 > pb.bb.3 + 1e-3 || pb.bb.1 > pa.bb.3 + 1e-3 {
                            continue;
                        }
                        let ok = if diag {
                            let corner = (
                                map.origin.0 + (ac + i64::from(bc > ac)) as f32 * map.cell,
                                map.origin.1 + (ar + i64::from(br > ar)) as f32 * map.cell,
                            );
                            pa.kind == pb.kind && pa.kind != 2 && has_vertex(&pa.outline, corner) && has_vertex(&pb.outline, corner)
                        } else {
                            edges_meet(&edges[a], &edges[b])
                        };
                        if ok {
                            list.push(pb.idx);
                        }
                    }
                }
            }
            if (pa.idx as usize) < base_len {
                if own_rebuilt || list.as_slice() != map.neighbours(pa.idx as usize) {
                    ov.base_adj.insert(pa.idx, list);
                }
            } else {
                ov.adj[pa.idx as usize - base_len] = list;
            }
        }
    }
    ov
}

/// A polygon of the rebuilt area.
struct APoly {
    idx: u32,
    kind: u8,
    outline: Vec<(f32, f32)>,
    bb: (f32, f32, f32, f32),
    rebuilt: bool,
    conformed: bool,
}

impl APoly {
    fn new(idx: u32, _cell: u32, kind: u8, outline: Vec<(f32, f32)>, rebuilt: bool) -> APoly {
        let bb = outline.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |m, v| (m.0.min(v.0), m.1.min(v.1), m.2.max(v.0), m.3.max(v.1)));
        APoly { idx, kind, outline, bb, rebuilt, conformed: false }
    }
}

/// An edge: start, unit direction, length.
#[derive(Clone, Copy)]
struct Edge {
    u: Q,
    dir: Q,
    len: f64,
}

fn edges_of(o: &[(f32, f32)]) -> Vec<Edge> {
    let q = |p: (f32, f32)| (f64::from(p.0), f64::from(p.1));
    (0..o.len())
        .filter_map(|i| {
            let (u, v) = (q(o[i]), q(o[(i + 1) % o.len()]));
            let len = ((v.0 - u.0).powi(2) + (v.1 - u.1).powi(2)).sqrt();
            (len > EPS).then(|| Edge { u, dir: ((v.0 - u.0) / len, (v.1 - u.1) / len), len })
        })
        .collect()
}

/// True if an edge of `a` and an edge of `b` lie on one line and overlap by more than [`EPS`].
fn edges_meet(a: &[Edge], b: &[Edge]) -> bool {
    for e in a {
        for f in b {
            // Parallel (either direction)?
            if (e.dir.0 * f.dir.1 - e.dir.1 * f.dir.0).abs() > 1e-3 {
                continue;
            }
            let off = |p: Q| (p.0 - e.u.0) * e.dir.1 - (p.1 - e.u.1) * e.dir.0;
            let f_end = (f.u.0 + f.dir.0 * f.len, f.u.1 + f.dir.1 * f.len);
            if off(f.u).abs() > EPS || off(f_end).abs() > EPS {
                continue;
            }
            let t = |p: Q| (p.0 - e.u.0) * e.dir.0 + (p.1 - e.u.1) * e.dir.1;
            let (t0, t1) = (t(f.u), t(f_end));
            let lo = t0.min(t1).max(0.0);
            let hi = t0.max(t1).min(e.len);
            if hi - lo > EPS {
                return true;
            }
        }
    }
    false
}

fn store(ov: &mut Overlay, map: &PolyMap, p: u32, outline: &[(f32, f32)], kind: u8) {
    let base_len = map.len();
    if (p as usize) < base_len {
        if outline != map.outline(p as usize) {
            ov.base_outline.insert(p, outline.to_vec());
        }
        if kind != map.kind[p as usize] {
            if ov.base_kind.is_empty() {
                ov.base_kind = vec![NO_KIND; base_len];
            }
            ov.base_kind[p as usize] = kind;
        } else if !ov.base_kind.is_empty() {
            ov.base_kind[p as usize] = NO_KIND;
        }
    } else {
        let i = p as usize - base_len;
        ov.outline[i] = outline.to_vec();
        ov.kind[i] = kind;
    }
}

fn has_vertex(o: &[(f32, f32)], v: (f32, f32)) -> bool {
    o.iter().any(|&p| (f64::from(p.0) - f64::from(v.0)).abs() <= EPS && (f64::from(p.1) - f64::from(v.1)).abs() <= EPS)
}


/// The core of a character obstacle: 24 points on a circle of radius `r` round `c`, starting at
/// angle 0 and going counter-clockwise (CONFIRMED `0x009CC0E0`; the original rounds them to
/// Fixed20).
pub fn circle(c: (f32, f32), r: f32) -> Vec<(f32, f32)> {
    regular(c, r, 24)
}

/// The core of a character inside a settlement: a triangle of radius 0.25 (CONFIRMED
/// `0x009CC0E0` in its special state `0x009D3CB0`; the saved cell ranges of every garrisoned
/// commander of the eur_napoleon start position fit it, `zoc_install.rs`).
pub fn triangle(c: (f32, f32)) -> Vec<(f32, f32)> {
    regular(c, 0.25, 3)
}

/// `n` points on a circle of radius `r` round `c` from angle 0 counter-clockwise, Fixed20-rounded.
pub fn regular(c: (f32, f32), r: f32, n: u32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::TAU / f64::from(n);
            let fx = |v: f64| ((v * 1_048_576.0).round() / 1_048_576.0) as f32;
            (fx(f64::from(c.0) + f64::from(r) * a.cos()), fx(f64::from(c.1) + f64::from(r) * a.sin()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::polypath::{kind, CellInput, Mover, PolyInput};
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
    fn kinds_and_links_follow_the_exe_tables() {
        assert_eq!(resolve(0, 9), 9);
        assert_eq!(resolve(7, 9), 7);
        assert_eq!(resolve(8, 10), 8);
        assert_eq!(resolve(0, 12), 0);
        assert_eq!(resolve(8, 9), 9);
        assert_eq!(resolve(9, 8), 9);
        assert!(links(0, 7) && links(8, 1) && !links(1, 8) && !links(9, 0) && !links(10, 0));
    }

    #[test]
    fn a_core_cuts_a_hole_and_the_rest_stays_walkable() {
        let m = open(10, 10);
        // A core in the middle of cell (4, 4): the cell becomes a kind 9 24-gon and wedges.
        let ov = build(&m, &[Cut { shape: Shape::Convex(circle((9.0, 9.0), 0.6)), kind: 9 }]);
        let v = m.with(&ov);
        let cell = 4 * 10 + 4;
        let polys: Vec<usize> = v.cell_polys(cell).collect();
        assert_eq!(polys.len(), 5, "hole, left, right, above, below");
        let hole: Vec<usize> = polys.iter().copied().filter(|&p| v.kind(p) == 9).collect();
        assert_eq!(hole.len(), 1);
        assert!(v.neighbours(hole[0]).is_empty());
        // Areas add up to the cell.
        let a: f64 = polys.iter().map(|&p| area(&v.outline(p).iter().map(|q| (f64::from(q.0), f64::from(q.1))).collect::<Vec<_>>()).abs()).sum();
        assert!((a - 4.0).abs() < 1e-3, "{a}");
        // The point under the core is kind 9; a path across the cell goes round it.
        assert_eq!(v.kind(v.polygon_at(9.0, 9.0).unwrap()), 9);
        let p = v.find_path((1.0, 9.0), (17.0, 9.0), Mover::Land, &[]).expect("path round the core");
        assert!(p.polys.iter().all(|&q| v.kind(q as usize) != 9));
        // Through the cut cell: the wedges link to the neighbours.
        let p = v.find_path((9.0, 8.2), (9.0, 1.0), Mover::Land, &[]).expect("path out of a wedge");
        assert!(p.polys.len() >= 2);
        // Nothing to cut: an empty overlay.
        assert!(build(&m, &[]).is_empty());
    }

    #[test]
    fn convex_cuts_tile_the_polygon() {
        let sq = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
        for &(cx, cz) in &[(1.0, 1.0), (0.0, 0.0), (2.0, 1.0), (0.3, 1.7), (2.5, 1.0), (1.0, -0.5), (5.0, 5.0)] {
            let c: Vec<(f64, f64)> = circle((cx, cz), 1.0).iter().map(|p| (f64::from(p.0), f64::from(p.1))).collect();
            let (i, o) = cut_convex(&sq, &c);
            let total = area(&i).abs() + o.iter().map(|p| area(p).abs()).sum::<f64>();
            assert!((total - 4.0).abs() < 1e-6, "{cx},{cz}: {total} inside {} parts {:?} {:?}", area(&i), o.iter().map(|p| area(p)).collect::<Vec<_>>(), o);
            assert!(o.len() <= 6, "{cx},{cz}: {} outside parts", o.len());
            for p in o.iter().chain(std::iter::once(&i)).filter(|p| !p.is_empty()) {
                assert!(area(p) > 0.0, "counter-clockwise");
            }
        }
    }

    #[test]
    fn a_zone_changes_kinds_only() {
        let m = open(6, 6);
        let ov = build(&m, &[Cut { shape: Shape::Polygons(vec![7, 8]), kind: 8 }, Cut { shape: Shape::Convex(circle((5.0, 3.0), 1.0)), kind: 9 }]);
        let v = m.with(&ov);
        assert_eq!(v.kind(7), 8);
        assert!(!Mover::Land.may_enter(v.kind(7)) && Mover::Agent.may_enter(v.kind(7)));
        // The core over the zone: kind 9 wins (9 is protected, 8 is not).
        assert_eq!(v.kind(v.polygon_at(5.0, 3.0).unwrap()), 9);
    }
}

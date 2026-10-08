//! Path smoothing after the original's campaign search (`analysis/campaign/PATHFINDING.md` §7).
//!
//! The original turns the (cell, polygon) path of its A* into the walked line in four steps
//! (CONFIRMED structure, `0x00B6E6E0` → `0x00B6AD30`):
//! 1. a **corridor** of the path's polygons, with an extra polygon of the side cell inserted where
//!    the path steps diagonally (`0x00B6AD30`);
//! 2. the corridor polygons **cut into triangles**, linked across shared edges (`0x00AF3610`);
//! 3. a **search over those triangles** from the start's to the goal's (`0x00ACAB90` → `0x00AC4C20`);
//! 4. the line pulled **taut through the triangle sequence** (`0x00B6BC60`: angles and
//!    segment/edge intersections), then each cell step's cost of the search spread over the new
//!    points between the matching corridor positions (`0x00B15CD0`).
//!
//! Ours follows the same steps. PROVISIONAL details (the original's exact rules are not decoded):
//! ear-clipping triangulation, a shortest-centroid-distance triangle search, the "simple stupid
//! funnel" for step 4, interpolated costs, and the turning points moved a hair into their
//! triangle so a mover never stops exactly on a corner shared with a polygon it may not enter.
//! If any step fails (e.g. a T-junction in the data breaks the triangle links) the unsmoothed
//! path is returned unchanged.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use super::polypath::{PolyMap, PolyPath, View};

type P = (f32, f32);
/// An edge as its two end points' exact bit patterns (lower first).
type EdgeKey = ((u32, u32), (u32, u32));

fn cross(o: P, a: P, b: P) -> f32 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn dist(a: P, b: P) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Ear-clipping triangulation of a simple polygon (either winding). Returns vertex triples.
pub(crate) fn triangulate(poly: &[P]) -> Vec<[P; 3]> {
    let mut v: Vec<P> = Vec::with_capacity(poly.len());
    for &p in poly {
        if v.last() != Some(&p) {
            v.push(p);
        }
    }
    while v.len() > 1 && v.first() == v.last() {
        v.pop();
    }
    if v.len() < 3 {
        return Vec::new();
    }
    let area: f32 = (0..v.len()).map(|i| cross((0.0, 0.0), v[i], v[(i + 1) % v.len()])).sum();
    if area < 0.0 {
        v.reverse();
    }
    let mut out = Vec::with_capacity(v.len() - 2);
    let mut guard = 0;
    while v.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = v.len();
        let mut best: Option<(usize, f32)> = None;
        let mut clipped = false;
        for i in 0..n {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            let cr = cross(a, b, c);
            if best.is_none_or(|(_, bc)| cr > bc) {
                best = Some((i, cr));
            }
            if cr <= 0.0 {
                continue;
            }
            let inside = v.iter().enumerate().any(|(j, &p)| {
                j != i && j != (i + n - 1) % n && j != (i + 1) % n && p != a && p != b && p != c
                    && cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
            });
            if !inside {
                out.push([a, b, c]);
                v.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Degenerate (collinear runs): clip the most convex vertex.
            let (i, _) = best.expect("n > 3");
            let n = v.len();
            out.push([v[(i + n - 1) % n], v[i], v[(i + 1) % n]]);
            v.remove(i);
        }
    }
    if v.len() == 3 {
        out.push([v[0], v[1], v[2]]);
    }
    out
}

fn in_triangle(t: &[P; 3], p: P) -> bool {
    let (d1, d2, d3) = (cross(t[0], t[1], p), cross(t[1], t[2], p), cross(t[2], t[0], p));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

fn centroid(t: &[P; 3]) -> P {
    ((t[0].0 + t[1].0 + t[2].0) / 3.0, (t[0].1 + t[1].1 + t[2].1) / 3.0)
}

fn key(p: P) -> (u32, u32) {
    (p.0.to_bits(), p.1.to_bits())
}

#[derive(Clone, Copy, PartialEq)]
struct Open(f32, usize);
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

/// The corridor: the path's polygons in order (no repeats in a row), plus, at each diagonal step
/// between cells, a polygon of one of the two side cells that touches both (CONFIRMED rule shape,
/// `0x00B6AD30`; which side cell is tried first: ours, the column neighbour).
fn corridor(map: &View<'_>, path: &PolyPath, blocked: &dyn Fn(usize) -> bool) -> Vec<(usize, usize)> {
    // (polygon, index of the search point it belongs to)
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (j, &q) in path.polys.iter().enumerate() {
        let q = q as usize;
        if let Some(&(prev, _)) = out.last() {
            if prev == q {
                continue;
            }
            let (pc, qc) = (map.poly_cell(prev), map.poly_cell(q));
            let (cols, rows) = (map.map.cols, map.map.rows);
            let (c0, r0) = ((pc % cols) as i64, (pc / cols) as i64);
            let (c1, r1) = ((qc % cols) as i64, (qc / cols) as i64);
            if c1 != c0 && r1 != r0 {
                let sides = [(c1, r0), (c0, r1)];
                let found = sides.iter().find_map(|&(c, r)| {
                    if c < 0 || r < 0 || c >= cols as i64 || r >= rows as i64 {
                        return None;
                    }
                    let ci = (r * cols as i64 + c) as usize;
                    map.cell_polys(ci).find(|&s| {
                        map.kind(s) == map.kind(prev)
                            && !blocked(s)
                            && map.neighbours(prev).contains(&(s as u32))
                            && map.neighbours(s).contains(&(q as u32))
                    })
                });
                if let Some(s) = found {
                    out.push((s, j - 1));
                }
            }
        }
        out.push((q, j));
    }
    out
}

/// Smooths a path found by [`PolyMap::find_path`] (see the module docs). The first and last
/// points stay; costs keep their total and grow along the new points.
pub fn smooth(map: &PolyMap, path: &PolyPath) -> PolyPath {
    smooth_avoiding(map, path, &|_| false)
}

/// [`smooth`] for a path found by [`PolyMap::find_path_avoiding`]: the corridor never takes a
/// blocked side polygon, so the line stays out of the blocked polygons.
pub fn smooth_avoiding(map: &PolyMap, path: &PolyPath, blocked: &dyn Fn(usize) -> bool) -> PolyPath {
    smooth_view(&map.view(), path, blocked)
}

/// [`smooth_avoiding`] on a [`View`] (a map with run-time polygons).
pub fn smooth_view(map: &View<'_>, path: &PolyPath, blocked: &dyn Fn(usize) -> bool) -> PolyPath {
    try_smooth(map, path, blocked).unwrap_or_else(|| path.clone())
}

fn try_smooth(map: &View<'_>, path: &PolyPath, blocked: &dyn Fn(usize) -> bool) -> Option<PolyPath> {
    let n = path.points.len();
    if n < 3 {
        return None;
    }
    let (start, goal) = (path.points[0], path.points[n - 1]);
    let cor = corridor(map, path, blocked);
    // Triangles of the corridor, each with its corridor index.
    let mut tris: Vec<([P; 3], usize)> = Vec::new();
    for (k, &(q, _)) in cor.iter().enumerate() {
        for t in triangulate(map.outline(q)) {
            tris.push((t, k));
        }
    }
    if tris.is_empty() {
        return None;
    }
    // Links across shared edges (exact points), and at single shared corners between consecutive
    // corridor polygons (diagonal steps without a side polygon): portal (left, right) pairs.
    let mut by_edge: HashMap<EdgeKey, Vec<usize>> = HashMap::new();
    for (i, (t, _)) in tris.iter().enumerate() {
        for e in 0..3 {
            let (a, b) = (key(t[e]), key(t[(e + 1) % 3]));
            by_edge.entry(if a < b { (a, b) } else { (b, a) }).or_default().push(i);
        }
    }
    let mut adj: Vec<Vec<(usize, P, P)>> = vec![Vec::new(); tris.len()];
    for (i, (t, _)) in tris.iter().enumerate() {
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            for &j in &by_edge[&if ka < kb { (ka, kb) } else { (kb, ka) }] {
                if j != i {
                    adj[i].push((j, a, b));
                }
            }
        }
    }
    let mut group: Vec<Vec<usize>> = vec![Vec::new(); cor.len()];
    for (i, (_, k)) in tris.iter().enumerate() {
        group[*k].push(i);
    }
    for k in 1..cor.len() {
        let linked = group[k - 1].iter().any(|&i| adj[i].iter().any(|&(j, _, _)| tris[j].1 == k));
        if linked {
            continue;
        }
        for &i in &group[k - 1] {
            for &j in &group[k] {
                if let Some(&v) = tris[i].0.iter().find(|v| tris[j].0.contains(v)) {
                    adj[i].push((j, v, v));
                    adj[j].push((i, v, v));
                }
            }
        }
    }
    // Every triangle holding the start (or the goal); without one, the nearest by centroid.
    let holding = |p: P| -> Vec<usize> {
        let hits: Vec<usize> = (0..tris.len()).filter(|&i| in_triangle(&tris[i].0, p)).collect();
        if !hits.is_empty() {
            return hits;
        }
        (0..tris.len())
            .min_by(|&a, &b| dist(centroid(&tris[a].0), p).total_cmp(&dist(centroid(&tris[b].0), p)))
            .into_iter()
            .collect()
    };
    let (sources, targets) = (holding(start), holding(goal));
    // Triangle search: shortest start → centroids → goal distance.
    let mut g = vec![f32::INFINITY; tris.len()];
    let mut prev = vec![usize::MAX; tris.len()];
    let mut heap = BinaryHeap::new();
    for &s in &sources {
        g[s] = dist(start, centroid(&tris[s].0));
        heap.push(Open(g[s], s));
    }
    let mut best: Option<(f32, usize)> = None;
    while let Some(Open(d, i)) = heap.pop() {
        if d > g[i] {
            continue;
        }
        if best.is_some_and(|(b, _)| d >= b) {
            break;
        }
        if targets.contains(&i) {
            let total = d + dist(centroid(&tris[i].0), goal);
            if best.is_none_or(|(b, _)| total < b) {
                best = Some((total, i));
            }
        }
        for &(j, _, _) in &adj[i] {
            let nd = d + dist(centroid(&tris[i].0), centroid(&tris[j].0));
            if nd < g[j] {
                g[j] = nd;
                prev[j] = i;
                heap.push(Open(nd, j));
            }
        }
    }
    let (_, tg) = best?;
    let mut chain = vec![tg];
    while prev[*chain.last()?] != usize::MAX {
        chain.push(prev[*chain.last()?]);
    }
    chain.reverse();
    let ts = chain[0];
    // Portals (left, right) as seen when walking the chain, then the goal.
    let mut portals: Vec<(P, P, usize)> = vec![(start, start, chain[0])];
    for w in chain.windows(2) {
        let (i, j) = (w[0], w[1]);
        let &(_, a, b) = adj[i].iter().find(|x| x.0 == j)?;
        let c = centroid(&tris[i].0);
        let (l, r) = if cross(c, a, b) > 0.0 { (b, a) } else { (a, b) };
        portals.push((l, r, j));
    }
    portals.push((goal, goal, tg));
    // Simple stupid funnel: emitted points with the triangle they lead into.
    let mut pts: Vec<(P, usize)> = vec![(start, ts)];
    let (mut apex, mut left, mut right) = (start, start, start);
    let (mut li, mut ri) = (0usize, 0usize);
    let mut i = 1;
    let mut guard = 0;
    while i < portals.len() && guard < 100_000 {
        guard += 1;
        let (pl, pr, _) = portals[i];
        // Right side.
        if cross(apex, right, pr) >= 0.0 {
            if apex == right || cross(apex, left, pr) < 0.0 {
                right = pr;
                ri = i;
            } else {
                apex = left;
                pts.push((apex, portals[li].2));
                let a = li;
                (left, right, ri) = (apex, apex, a);
                i = a + 1;
                continue;
            }
        }
        // Left side.
        if cross(apex, left, pl) <= 0.0 {
            if apex == left || cross(apex, right, pl) > 0.0 {
                left = pl;
                li = i;
            } else {
                apex = right;
                pts.push((apex, portals[ri].2));
                let a = ri;
                (left, right, li) = (apex, apex, a);
                i = a + 1;
                continue;
            }
        }
        i += 1;
    }
    if pts.last().map(|p| p.0) != Some(goal) {
        pts.push((goal, tg));
    }
    // Interior turning points: a hair into the triangle they lead into.
    let last = pts.len() - 1;
    for (k, (p, t)) in pts.iter_mut().enumerate() {
        if k > 0 && k < last {
            let c = centroid(&tris[*t].0);
            let d = dist(*p, c);
            if d > 0.0 {
                let f = (0.01 / d).min(0.5);
                *p = (p.0 + (c.0 - p.0) * f, p.1 + (c.1 - p.1) * f);
            }
        }
    }
    // Costs: each point's corridor position mapped back onto the search's points and their costs.
    let total = *path.costs.last()?;
    let mut out = PolyPath { points: Vec::with_capacity(pts.len()), costs: Vec::new(), polys: Vec::new() };
    let mut best = 0.0f32;
    for (k, &(p, t)) in pts.iter().enumerate() {
        let cost = if k == 0 {
            0.0
        } else if k == last {
            total
        } else {
            let j = cor[tris[t].1].1;
            let (a, b) = (j.saturating_sub(1), j);
            let (pa, pb) = (path.points[a], path.points[b]);
            let len2 = (pb.0 - pa.0).powi(2) + (pb.1 - pa.1).powi(2);
            let f = if len2 > 0.0 { (((p.0 - pa.0) * (pb.0 - pa.0) + (p.1 - pa.1) * (pb.1 - pa.1)) / len2).clamp(0.0, 1.0) } else { 1.0 };
            (path.costs[a] + (path.costs[b] - path.costs[a]) * f).min(total)
        };
        best = best.max(cost);
        out.points.push(p);
        out.costs.push(best);
        out.polys.push(if k == 0 { path.polys[0] } else if k == last { path.polys[n - 1] } else { cor[tris[t].1].0 as u32 });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::super::polypath::{kind, CellInput, Mover, PolyInput};
    use super::*;

    const U: i32 = 1 << 20;

    fn map(w: u32, h: u32, k: impl Fn(u32, u32) -> u8) -> PolyMap {
        let mut cells = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let (x0, z0) = (c as i32 * 2 * U, r as i32 * 2 * U);
                let sq = vec![(x0, z0), (x0 + 2 * U, z0), (x0 + 2 * U, z0 + 2 * U), (x0, z0 + 2 * U)];
                cells.push(CellInput { header: [0; 8], polys: vec![PolyInput { kind: k(c, r), region_id: 0, outline: sq }] });
            }
        }
        PolyMap::build((0, 0), 2 * U, w, h, &cells, vec![vec![0]])
    }

    #[test]
    fn triangulates_squares_and_concave_shapes() {
        assert_eq!(triangulate(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]).len(), 2);
        // An L shape (6 points) gives 4 triangles covering its area 3.
        let l = [(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)];
        let t = triangulate(&l);
        assert_eq!(t.len(), 4);
        let area: f32 = t.iter().map(|t| cross(t[0], t[1], t[2]).abs() * 0.5).sum();
        assert!((area - 3.0).abs() < 1e-5);
    }

    #[test]
    fn straightens_a_staircase_and_keeps_the_cost() {
        // Open 8 x 8 land: a diagonal-ish trip becomes (nearly) a straight line.
        let m = map(8, 8, |_, _| kind::LAND);
        let p = m.find_path((1.0, 1.0), (15.0, 9.0), Mover::Land, &[]).unwrap();
        let s = smooth(&m, &p);
        assert_eq!(s.points.first(), p.points.first());
        assert_eq!(s.points.last(), p.points.last());
        assert!(s.points.len() <= 3, "{:?}", s.points);
        assert_eq!(s.costs.last(), p.costs.last());
        assert!(s.costs.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn bends_round_an_obstacle_corner() {
        // A sea wall at column 3, rows 0..=4; the path from the south-west to the south-east
        // must turn at the wall's top end and every point must stay on land.
        let m = map(7, 7, |c, r| if c == 3 && r <= 4 { kind::SEA } else { kind::LAND });
        let p = m.find_path((1.0, 1.0), (13.0, 1.0), Mover::Land, &[]).unwrap();
        let s = smooth(&m, &p);
        assert!(s.points.len() >= 3 && s.points.len() < p.points.len(), "{:?}", s.points);
        for &q in &s.points {
            let poly = m.polygon_at(q.0, q.1).unwrap();
            assert_eq!(m.kind[poly], kind::LAND, "{q:?}");
        }
        // The turning points lie near the wall's top corners (6, 10) and (8, 10).
        assert!(s.points.iter().any(|q| (q.1 - 10.0).abs() < 0.1));
    }
}

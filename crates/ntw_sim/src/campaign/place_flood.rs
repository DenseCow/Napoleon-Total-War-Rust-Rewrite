//! Where a force is put down near a point: the original's placement flood over the path grid and
//! the exact geometry it runs on (CONFIRMED from the disassembly; analysis/fidelity/CAMPAIGN_FIDELITY.md,
//! "Placement"). The region expulsion ([`super::expulsion`]) is its caller.
//!
//! Points are raw Fixed20 map coordinates (x, z), as the grid's file stores them
//! ([`PolyMap::points_fixed`]): the original runs these tests on integers, with single-precision
//! steps in set places, and every step here keeps its order of operations.
//!
//! - [`outline_point`] (`0x008AF750`): the point of a region's settlement area outlines nearest to
//!   a position; the point a candidate region is entered at.
//! - [`distance_key`]: the squared distance the candidates are sorted by (`0x008DCB30`).
//! - [`in_region`] (`0x00B4D800`): whether a position lies in a region, by the polygon under it.
//! - [`flood`] (`0x00B35640`): the nearest valid position of a region to a point, searched best
//!   first over the grid's cells (cell scan `0x00B41A60`, nearest outline point `0x00B1CA50` with
//!   the row search `0x00B1C800` / `0x00B1CCB0`).
//!
//! The free-spot search of a newly raised army ([`View::free_spot`], `0x00B39FE0`) runs on the same
//! pieces: [`cell_of`], [`cell_priority`], the per-polygon step [`Scratch::offer`] and the heap.

use std::collections::HashSet;

use super::polypath::{kind, PolyMap, View};
use super::zoc::{GOAL_ZONE_KIND, TARGET_CORE_KIND};
use crate::msvc_sort;

/// A raw Fixed20 map position (x, z).
pub type Point = (i32, i32);

/// One map unit in raw Fixed20.
const ONE: f32 = 1_048_576.0;
/// Raw Fixed20 to map units.
const FIX: f32 = 1.0 / ONE;
/// A product of two raw Fixed20 values to map units squared.
const FIX_SQ: f32 = FIX * FIX;

/// The original's conversion of a single-precision value to Fixed20 by truncation (inlined in
/// `0x008AF750` and `0x009257A0`, CONFIRMED): the magnitude is split into its whole and fractional
/// parts, the fraction times 2^20 is truncated, the whole part goes above bit 20, and the sign is
/// put back. (A rounding conversion would differ in the last bit.) The original's whole part wraps
/// past 2048 map units; no map coordinate reaches that.
fn fix_trunc(v: f32) -> i32 {
    let a = f64::from(v.abs());
    let whole = a.trunc();
    let frac = (a - whole) as f32;
    let bits = ((frac * ONE) as u32) | (((whole as f32) as u32) << 20);
    if v < 0.0 { (bits as i32).wrapping_neg() } else { bits as i32 }
}

/// The two ends of an edge, the lower first by (z, x): the order the original's edge tests put
/// them in (`0x00B1FA80`, `0x00B1CCB0`).
fn low_first(a: Point, b: Point) -> (Point, Point) {
    if (a.1, a.0) <= (b.1, b.0) { (a, b) } else { (b, a) }
}

/// The point of `outlines` nearest to `p` (`0x008AF750`, CONFIRMED): the place a candidate region is
/// entered at. `outlines` are the region's settlement area outlines in map units
/// ([`super::world::World::settlement_area_outlines`]: the areas the settlement stands on).
///
/// In single precision, with `p` converted first (raw × 2^-20): each outline's edges, from (last,
/// first) on; the position's projection on the edge, its parameter clamped to 0..1 (0 on an edge
/// of no length); the projected point with the strictly smallest squared distance wins, so the
/// first of two equally near points is kept. The result is converted back by truncation
/// ([`fix_trunc`]). Without an outline the result is `p` itself after that round trip, as in the
/// original, whose result starts as the position.
pub fn outline_point(outlines: &[Vec<(f32, f32)>], p: Point) -> Point {
    let (px, pz) = (p.0 as f32 * FIX, p.1 as f32 * FIX);
    let mut best = f32::MAX;
    let mut out = (px, pz);
    for outline in outlines {
        let Some(&last) = outline.last() else { continue };
        let mut a = last;
        for &b in outline {
            let (dx, dz) = (b.0 - a.0, b.1 - a.1);
            let (qx, qz) = (px - a.0, pz - a.1);
            let len = (dx * dx + dz * dz).sqrt();
            let t = if len <= 0.0 {
                0.0
            } else {
                let inv = 1.0 / len;
                let t = (inv * dx) * (qx * inv) + (inv * dz) * (inv * qz);
                if t >= 0.0 {
                    if t < 1.0 { t } else { 1.0 }
                } else {
                    0.0
                }
            };
            let c = (a.0 + dx * t, a.1 + dz * t);
            let d = (c.0 - px) * (c.0 - px) + (c.1 - pz) * (c.1 - pz);
            if best > d {
                best = d;
                out = c;
            }
            a = b;
        }
    }
    (fix_trunc(out.0), fix_trunc(out.1))
}

/// The squared distance between two positions in map units, as the original takes it: the raw
/// differences converted to single precision, squared, added and scaled by 2^-40 (the candidate
/// sort's key in `0x008DCB30`; the square root of it is the distance of `0x00B41A60` and
/// `0x00B1CA50`; CONFIRMED).
pub fn distance_key(a: Point, b: Point) -> f32 {
    let (dx, dz) = (b.0.wrapping_sub(a.0) as f32, b.1.wrapping_sub(a.1) as f32);
    (dz * dz + dx * dx) * FIX_SQ
}

/// The original's inside test (`0x00B1FA80`, 64-bit integers; CONFIRMED): the edges from (last,
/// first) on, each with its ends lower first ([`low_first`]); an edge counts when the lower end's z
/// ≤ the point's z < the upper end's, and toggles the answer when the point is on it or to its
/// right. So a polygon holds its west and south edges and not its east and north ones.
fn inside(outline: &[Point], p: Point) -> bool {
    let Some(&last) = outline.last() else { return false };
    let mut a = last;
    let mut hit = false;
    for &b in outline {
        let (lo, hi) = low_first(a, b);
        if lo.1 <= p.1 && p.1 < hi.1 {
            let l = (i64::from(hi.0) - i64::from(lo.0)) * (i64::from(p.1) - i64::from(lo.1));
            let r = (i64::from(hi.1) - i64::from(lo.1)) * (i64::from(p.0) - i64::from(lo.0));
            if l <= r {
                hit = !hit;
            }
        }
        a = b;
    }
    hit
}

/// The grid cell (column, row) of a position (`0x00B44740`, CONFIRMED): per axis the distance from
/// the grid's origin, not below 0, divided by the cell size in fixed point and truncated. So a
/// position west or south of the grid falls in its first column or row. `None` past the grid's
/// east or north end, where the original finds no tile for the position, and on a grid without a
/// cell size.
///
/// The original keeps the cell in two 16-bit numbers and the quotient in 32 bits, so its grid
/// holds fewer than 2048 cells a side; ours has no such limit.
pub(super) fn cell_of(map: &PolyMap, p: Point) -> Option<(u32, u32)> {
    if map.cell_fixed <= 0 {
        return None;
    }
    let axis = |v: i32, origin: i32| {
        let d = (i64::from(v) - i64::from(origin)).max(0);
        ((d << 20) / i64::from(map.cell_fixed)) >> 20
    };
    let (col, row) = (axis(p.0, map.origin_fixed.0), axis(p.1, map.origin_fixed.1));
    (col < i64::from(map.cols) && row < i64::from(map.rows)).then_some((col as u32, row as u32))
}

/// The index of cell (col, row) in the grid's row-major cell list.
fn cell_index(map: &PolyMap, col: u32, row: u32) -> usize {
    row as usize * map.cols as usize + col as usize
}

/// Whether position `p` lies in the region with index `region` (an index into the grid's region
/// keys) on the static grid (`0x00B4D800`, CONFIRMED): the polygons of the position's cell
/// ([`cell_of`]) in their order, the first that holds the position ([`inside`]) and has a known
/// region id deciding. A single-region id (the layer's slots below `+0x140`) must be the region's
/// own; a group id (the shared list `+0x154`) must list the region. A polygon without a region id
/// (the file's 1023) is passed over. No polygon: false.
pub fn in_region(map: &PolyMap, p: Point, region: u32) -> bool {
    let Some((col, row)) = cell_of(map, p) else { return false };
    let ci = cell_index(map, col, row);
    for poly in map.cell_first[ci] as usize..map.cell_first[ci + 1] as usize {
        let outline = &map.points_fixed[map.outline_first[poly] as usize..map.outline_first[poly + 1] as usize];
        if !inside(outline, p) {
            continue;
        }
        if let Some(set) = map.region_sets.get(map.region_id[poly] as usize) {
            return set.contains(&region);
        }
    }
    false
}

/// How far cell (col, row) is from `p`, as the flood and the free-spot search order their cells
/// (`0x00AF1590` / `0x00AF0910` on the cell's rectangle `0x00B44880`; single precision, CONFIRMED): minus the smallest of the position's four
/// signed distances to the cell's edges (north, south, west, east, in that order). So 0 or less for
/// a position in the cell, and the larger of its two axis distances outside it.
pub(super) fn cell_priority(map: &PolyMap, col: u32, row: u32, p: Point) -> f32 {
    let (ox, oz, cell) = (map.origin_fixed.0 as f32, map.origin_fixed.1 as f32, map.cell_fixed as f32);
    let cs = cell * FIX;
    let (c, r) = (col as f32, row as f32);
    let (x0, z0) = (ox * FIX + c * cs, oz * FIX + r * cs);
    let (x1, z1) = ((ox + cell) * FIX + c * cs, (oz + cell) * FIX + r * cs);
    let (px, pz) = (p.0 as f32 * FIX, p.1 as f32 * FIX);
    let edge = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let inv = 1.0 / (dx * dx + dz * dz).sqrt();
        (dz * inv) * (px - a.0) + ((-dx) * inv) * (pz - a.1)
    };
    let mut least = edge((x0, z1), (x1, z1));
    for d in [edge((x1, z0), (x0, z0)), edge((x0, z0), (x0, z1)), edge((x1, z1), (x1, z0))] {
        if d <= least {
            least = d;
        }
    }
    -least
}

/// The point of the segment `a`..`b` nearest to `p` (`0x009257A0` with no flags, CONFIRMED): the
/// projection's parameter in single precision (0 on a segment of no length), clamped to 0..1 and
/// converted to Fixed20 by truncation ([`fix_trunc`]); the point is `a` plus the segment times
/// that, each product shifted down 20 bits.
fn segment_point(a: Point, b: Point, p: Point) -> Point {
    let (dx, dz) = (b.0.wrapping_sub(a.0), b.1.wrapping_sub(a.1));
    let (fx, fz) = (dx as f32, dz as f32);
    let len = ((fz * fz + fx * fx) * FIX_SQ).sqrt();
    if len <= 0.0 {
        return a;
    }
    let inv = 1.0 / len;
    let t = (((fz * (p.1.wrapping_sub(a.1) as f32) + fx * (p.0.wrapping_sub(a.0) as f32)) * FIX_SQ) * inv * inv).clamp(0.0, 1.0);
    let fix = i64::from(fix_trunc(t));
    (a.0.wrapping_add(((fix * i64::from(dx)) >> 20) as i32), a.1.wrapping_add(((fix * i64::from(dz)) >> 20) as i32))
}

/// One place an outline's edge crosses a row: the x of the crossing and the edge's two ends.
#[derive(Clone, Copy)]
struct Crossing {
    x: i32,
    a: Point,
    b: Point,
}

/// The x on row `row` nearest to `px` that lies in `outline`, with its cost, the squared distance
/// along the row (`0x00B1CCB0`, CONFIRMED). `None`: the outline has nothing on that row.
///
/// The edges with exactly one end above the row are crossed with it (64-bit integers, rounded half
/// away from zero) and the crossings sorted by x (std::sort `0x00AC2830`, [`msvc_sort::sort_by`]);
/// each pair is a span, left end inside and right end outside. A left end that the inside test
/// would put outside its own edge moves in by one. Spans of no width are passed over. `px` inside a
/// span costs 0; left of it the squared distance to the left end; at or past the right end the
/// squared distance to it, the place being one before it. The squares wrap in 32 bits and compare
/// unsigned, as the original's do.
///
/// `recs` is the caller's scratch list (a closed outline crosses a row an even number of times; a
/// last crossing without a partner is left out).
fn row_snap(outline: &[Point], px: i32, row: i32, recs: &mut Vec<Crossing>) -> Option<(u32, i32)> {
    recs.clear();
    let mut a = *outline.last()?;
    for &b in outline {
        if (a.1 > row) != (b.1 > row) {
            let (lo, hi) = low_first(a, b);
            let dz = i64::from(hi.1.wrapping_sub(lo.1));
            let dx = i64::from(hi.0.wrapping_sub(lo.0));
            let half = if dx < 0 { -(dz >> 1) } else { dz >> 1 };
            let x = lo.0.wrapping_add(((half + i64::from(row.wrapping_sub(lo.1)) * dx) / dz) as i32);
            recs.push(Crossing { x, a, b });
        }
        a = b;
    }
    msvc_sort::sort_by(recs, |l, r| l.x < r.x);
    let mut best = u32::MAX;
    let mut out = px;
    for span in recs.as_chunks::<2>().0 {
        let (mut left, right) = (span[0].x, span[1].x);
        if left == right {
            continue;
        }
        let (lo, hi) = low_first(span[0].a, span[0].b);
        let on_edge = i64::from(hi.0.wrapping_sub(lo.0)) * i64::from(row.wrapping_sub(lo.1))
            <= i64::from(left.wrapping_sub(lo.0)) * i64::from(hi.1.wrapping_sub(lo.1));
        if !on_edge {
            left = left.wrapping_add(1);
            if left == right {
                continue;
            }
        }
        let d = px.wrapping_sub(left);
        if d > 0 {
            let e = px.wrapping_sub(right);
            if e < 0 {
                if best != 0 {
                    best = 0;
                    out = px;
                }
            } else {
                let cost = (e as u32).wrapping_mul(e as u32);
                if cost < best {
                    best = cost;
                    out = right.wrapping_sub(1);
                }
            }
        } else {
            let cost = (d as u32).wrapping_mul(d as u32);
            if cost < best {
                best = cost;
                out = left;
            }
        }
    }
    (best != u32::MAX).then_some((best, out))
}

/// The position in `outline` nearest to `p`, searched row by row (`0x00B1C800`, CONFIRMED): the
/// position's own row first ([`row_snap`]), then the rows `i` above and below it for `i` = 1, 2, ...
/// while `i`² is below the best cost so far; a row costs its own cost plus `i`² (saturating), and a
/// strictly lower cost wins. Every row is searched from the position's own x. A row at or above the
/// outline's highest z is not searched, and the search ends at the first row below its lowest z
/// once that has happened. `None`: no row of the outline holds anything.
///
/// The original has no other bound: an outline with nothing on any row (one of no area) is walked
/// over its whole z range, one raw unit a step. The grid's polygons all have an area.
///
/// Cost bound: each row is one [`row_snap`] (a pass over the outline's n edges, then a sort and a pass
/// over its crossings). The only caller, [`nearest_point`], starts on one of the outline's edges, so
/// `p`'s row lies in the outline's z range and the loop ends after at most z_max − z_min + 1 steps
/// (`hit_top`, then the first row below z_min); a polygon lies in one grid cell (2 map units = 2^21 raw
/// rows), so that is at most 2^21 + 1 steps, two rows each. Once a row has found a place the loop
/// also ends at i² ≥ that cost, so i < 2^16 (the cost is a u32), which bounds every outline with an
/// area; only a sliver with no span on most rows walks its whole z range.
fn row_search(outline: &[Point], p: Point, recs: &mut Vec<Crossing>) -> Option<Point> {
    let z_min = outline.iter().map(|q| q.1).min()?;
    let z_max = outline.iter().map(|q| q.1).max()?;
    let mut best = u32::MAX;
    let mut out = p;
    if let Some((cost, x)) = row_snap(outline, p.0, p.1, recs) {
        best = cost;
        out = (x, p.1);
    }
    let mut try_row = |z: i32, i2: u32, best: &mut u32, out: &mut Point| {
        if let Some((cost, x)) = row_snap(outline, p.0, z, recs) {
            let cost = cost.saturating_add(i2);
            if cost < *best {
                *best = cost;
                *out = (x, z);
            }
        }
    };
    let (mut i, mut i2, mut hit_top) = (1i32, 1u32, false);
    while i2 < best {
        let up = p.1.wrapping_add(i);
        if up >= z_max {
            hit_top = true;
        } else {
            try_row(up, i2, &mut best, &mut out);
        }
        let down = p.1.wrapping_sub(i);
        if down >= z_min {
            try_row(down, i2, &mut best, &mut out);
        } else if hit_top {
            break;
        }
        i = i.wrapping_add(1);
        i2 = (i as u32).wrapping_mul(i as u32);
    }
    (best != u32::MAX).then_some(out)
}

/// The position in `outline` nearest to a position `p` outside it (`0x00B1CA50`, CONFIRMED): the
/// edge (from (last, first) on) whose nearest point ([`segment_point`]) is strictly nearest to `p`
/// (the root of [`distance_key`]), then the row search from that point ([`row_search`]), which
/// moves it onto a place the inside test accepts. `None` for an empty outline or when the row
/// search finds nothing.
fn nearest_point(outline: &[Point], p: Point, recs: &mut Vec<Crossing>) -> Option<Point> {
    let mut best = f32::MAX;
    let mut edge = None;
    let mut a = *outline.last()?;
    for &b in outline {
        let d = distance_key(p, segment_point(a, b, p)).sqrt();
        if best > d {
            best = d;
            edge = Some((a, b));
        }
        a = b;
    }
    let (a, b) = edge?;
    row_search(outline, segment_point(a, b, p), recs)
}

/// The scratch buffers of the grid searches ([`flood`] and [`View::free_spot`]), kept over their
/// cell scans, and the per-polygon steps both run on.
#[derive(Default)]
pub(super) struct Scratch {
    outline: Vec<Point>,
    recs: Vec<Crossing>,
}

impl Scratch {
    /// Whether polygon `poly` of `view` holds `p` (`0x00B1FA60`, the wrapper of the inside test
    /// [`inside`], `0x00B1FA80`).
    pub(super) fn holds(&mut self, view: &View<'_>, poly: usize, p: Point) -> bool {
        view.outline_fixed_into(poly, &mut self.outline);
        inside(&self.outline, p)
    }

    /// One polygon's step of a cell scan, the same in the flood's (`0x00B41A60`) and the free-spot
    /// search's (`0x00B421D0`; CONFIRMED, both read): a polygon holding `p` makes the result `p`
    /// itself at distance 0, whatever was best before; any other gives its nearest position
    /// ([`nearest_point`]), kept when its distance (the root of [`distance_key`]) is strictly less
    /// than the best so far, so the first of two equally near polygons keeps the spot.
    pub(super) fn offer(&mut self, view: &View<'_>, poly: usize, p: Point, best: &mut f32, out: &mut Point) {
        if self.holds(view, poly, p) {
            *best = 0.0;
            *out = p;
        } else if let Some(q) = nearest_point(&self.outline, p, &mut self.recs) {
            let d = distance_key(p, q).sqrt();
            if *best > d {
                *best = d;
                *out = q;
            }
        }
    }
}

/// One cell of the flood's scan (`0x00B41A60` in mode 0, CONFIRMED): every polygon of the cell, in
/// its order, that a land force may be put on (kinds 0 land and 6 road, and the run-time kinds 10
/// and 11, which pass in every mode) and whose region id is `slot`, offered in turn
/// ([`Scratch::offer`]). The scan does not stop at a hit.
fn scan_cell(view: &View<'_>, ci: usize, slot: u32, p: Point, best: &mut f32, out: &mut Point, scratch: &mut Scratch) {
    for poly in view.cell_polys(ci) {
        if !matches!(view.kind(poly), kind::LAND | kind::ROAD | GOAL_ZONE_KIND | TARGET_CORE_KIND) || view.region_id(poly) != slot {
            continue;
        }
        scratch.offer(view, poly, p, best, out);
    }
}

/// A cell on the flood's open list.
#[derive(Clone, Copy)]
struct Node {
    priority: f32,
    col: u32,
    row: u32,
}

/// The nearest position to `point` where a land force can be put down in the region with index
/// `region`, on the grid as `view` shows it (`0x00B35640`, CONFIRMED). `None` when the position has
/// no cell ([`cell_of`]) or nothing closer than `radius` is found.
///
/// The region is looked for by its own region id, the first that names it alone (`0x00B62290`: the
/// layer's single-region slots): a polygon of a group id is never a place. Polygons the view has
/// re-kinded 8 (a zone of control) or 9 (a core) are not places either ([`scan_cell`]), which is
/// how the caller's view keeps the force out of other characters' way.
///
/// A best-first search over the cells, kept in the library's heap ([`msvc_sort::push_heap`] /
/// [`msvc_sort::pop_heap`], smallest [`cell_priority`] on top; the order of cells of equal priority
/// is the heap's and decides between equally near places). It starts with the position's cell and
/// stops when the list is empty or the best distance is no more than the top cell's priority.
/// Otherwise the top cell is scanned and taken off, and its 3×3 block (rows outer, itself
/// included) is added: each cell of the grid whose priority is below the radius and that was not
/// added before. The first cell is not marked when it starts the list, so it is scanned twice.
pub fn flood(view: &View<'_>, point: Point, region: u32, radius: f32) -> Option<Point> {
    let map = view.map;
    let (col, row) = cell_of(map, point)?;
    // The original searches with slot -1 when the region has none: no polygon carries it.
    let slot = map.region_sets.iter().position(|set| set.as_slice() == [region])? as u32;
    let less = |a: &Node, b: &Node| a.priority > b.priority;
    let mut open = Vec::new();
    msvc_sort::push_heap(&mut open, Node { priority: cell_priority(map, col, row, point), col, row }, less);
    let mut added: HashSet<(u32, u32)> = HashSet::new();
    let mut scratch = Scratch::default();
    let mut best = f32::MAX;
    let mut out = (0, 0);
    while let Some(top) = open.first().copied() {
        if best <= top.priority {
            break;
        }
        scan_cell(view, cell_index(map, top.col, top.row), slot, point, &mut best, &mut out, &mut scratch);
        msvc_sort::pop_heap(&mut open, less);
        for dr in -1i64..=1 {
            for dc in -1i64..=1 {
                let (c, r) = (i64::from(top.col) + dc, i64::from(top.row) + dr);
                if c < 0 || r < 0 || c >= i64::from(map.cols) || r >= i64::from(map.rows) {
                    continue;
                }
                let (c, r) = (c as u32, r as u32);
                let priority = cell_priority(map, c, r, point);
                if radius <= priority || !added.insert((c, r)) {
                    continue;
                }
                msvc_sort::push_heap(&mut open, Node { priority, col: c, row: r }, less);
            }
        }
    }
    if radius <= best { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::polypath::{CellInput, PolyInput};
    use crate::campaign::rtcut::{self, Cut, Shape};
    use crate::campaign::zoc::{CORE_KIND, ZONE_KIND};

    const U: i32 = 1 << 20;
    /// Half a map unit.
    const H: i32 = U / 2;

    fn square(x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<Point> {
        vec![(x0, z0), (x1, z0), (x1, z1), (x0, z1)]
    }

    /// A `w` x `h` grid of 2-unit cells from the origin, one square polygon per cell with the kind
    /// and region id `cell(c, r)` gives.
    fn grid(w: u32, h: u32, sets: Vec<Vec<u32>>, cell: impl Fn(u32, u32) -> (u8, u32)) -> PolyMap {
        grid_of(w, h, sets, |c, r| {
            let (kind, region_id) = cell(c, r);
            let (x0, z0) = (c as i32 * 2 * U, r as i32 * 2 * U);
            vec![PolyInput { kind, region_id, outline: square(x0, z0, x0 + 2 * U, z0 + 2 * U) }]
        })
    }

    /// The same grid with each cell's polygons given outright.
    fn grid_of(w: u32, h: u32, sets: Vec<Vec<u32>>, polys: impl Fn(u32, u32) -> Vec<PolyInput>) -> PolyMap {
        let cells: Vec<CellInput> = (0..h).flat_map(|r| (0..w).map(move |c| (c, r))).map(|(c, r)| CellInput { header: [50; 8], polys: polys(c, r) }).collect();
        PolyMap::build((0, 0), 2 * U, w, h, &cells, sets)
    }

    #[test]
    fn the_fixed_point_conversion_truncates_and_keeps_the_sign() {
        assert_eq!(fix_trunc(0.0), 0);
        assert_eq!(fix_trunc(1.5), U + H);
        assert_eq!(fix_trunc(-1.5), -(U + H));
        assert_eq!(fix_trunc(2.75), 2 * U + 3 * U / 4);
        // 0.1 in single precision is 104857.6 raw units: truncated, where rounding gives 104858.
        assert_eq!(fix_trunc(0.1), 104_857);
        assert_eq!(fix_trunc(-0.1), -104_857);
    }

    #[test]
    fn the_outline_point_is_the_nearest_point_of_the_edges() {
        let sq = vec![vec![(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]];
        assert_eq!(outline_point(&sq, (6 * U, 2 * U)), (4 * U, 2 * U));
        assert_eq!(outline_point(&sq, (U, -3 * U)), (U, 0));
        // Past a corner the parameter is clamped to the edge's end.
        assert_eq!(outline_point(&sq, (6 * U, 6 * U)), (4 * U, 4 * U));
        assert_eq!(outline_point(&sq, (-U, -U)), (0, 0));
        // A position inside is moved to the outline too.
        assert_eq!(outline_point(&sq, (U, 2 * U)), (0, 2 * U));
    }

    #[test]
    fn an_edge_of_no_length_gives_its_own_point() {
        // The parameter is 0 there (no division by the zero length).
        assert_eq!(outline_point(&[vec![(5.0, 5.0), (5.0, 5.0)]], (0, 0)), (5 * U, 5 * U));
    }

    #[test]
    fn the_first_of_two_equally_near_outline_points_is_kept() {
        let two = vec![vec![(0.0, 0.0), (0.0, 2.0)], vec![(2.0, 0.0), (2.0, 2.0)]];
        assert_eq!(outline_point(&two, (U, U)), (0, U));
        let swapped = vec![two[1].clone(), two[0].clone()];
        assert_eq!(outline_point(&swapped, (U, U)), (2 * U, U));
    }

    #[test]
    fn without_an_outline_the_position_comes_back_through_single_precision() {
        assert_eq!(outline_point(&[], (3 * U + 1, -2 * U - 1)), (3 * U + 1, -2 * U - 1));
        assert_eq!(outline_point(&[vec![]], (3 * U + 1, 7)), (3 * U + 1, 7));
        // 300 units and one raw unit does not fit single precision: the raw unit is lost.
        assert_eq!(outline_point(&[], (300 * U + 1, 0)), (300 * U, 0));
    }

    #[test]
    fn the_sort_key_is_the_squared_distance_in_map_units() {
        assert_eq!(distance_key((0, 0), (3 * U, 4 * U)), 25.0);
        assert_eq!(distance_key((3 * U, 4 * U), (0, 0)), 25.0);
        assert_eq!(distance_key((U, U), (U, U)), 0.0);
        assert_eq!(distance_key((-U, 0), (H, 2 * U)), 6.25);
    }

    #[test]
    fn a_polygon_holds_its_west_and_south_edges_only() {
        for sq in [square(0, 0, 2 * U, 2 * U), square(0, 0, 2 * U, 2 * U).into_iter().rev().collect()] {
            assert!(inside(&sq, (U, U)));
            assert!(inside(&sq, (0, U)), "west edge");
            assert!(inside(&sq, (U, 0)), "south edge");
            assert!(inside(&sq, (0, 0)), "south-west corner");
            assert!(!inside(&sq, (2 * U, U)), "east edge");
            assert!(!inside(&sq, (U, 2 * U)), "north edge");
            assert!(!inside(&sq, (2 * U, 2 * U)));
            assert!(!inside(&sq, (0, 2 * U)));
            assert!(!inside(&sq, (2 * U, 0)));
            assert!(!inside(&sq, (-1, U)));
            assert!(!inside(&sq, (3 * U, U)));
        }
        assert!(!inside(&[], (0, 0)));
        // A slanted edge: the triangle (0,0) (4,0) (0,4) holds what is left of x + z = 4.
        let tri = vec![(0, 0), (4 * U, 0), (0, 4 * U)];
        assert!(inside(&tri, (U, U)));
        assert!(inside(&tri, (2 * U - 1, 2 * U)));
        assert!(!inside(&tri, (2 * U, 2 * U)), "on the slanted edge");
        assert!(!inside(&tri, (3 * U, 2 * U)));
    }

    #[test]
    fn the_cell_of_a_position_is_clamped_west_and_south_only() {
        let m = grid(3, 2, vec![vec![0]], |_, _| (kind::LAND, 0));
        assert_eq!(cell_of(&m, (U, U)), Some((0, 0)));
        assert_eq!(cell_of(&m, (5 * U, 3 * U)), Some((2, 1)));
        assert_eq!(cell_of(&m, (2 * U, 0)), Some((1, 0)), "a cell holds its west edge");
        assert_eq!(cell_of(&m, (2 * U - 1, 4 * U - 1)), Some((0, 1)));
        // West or south of the origin: the first column or row.
        assert_eq!(cell_of(&m, (-7 * U, U)), Some((0, 0)));
        assert_eq!(cell_of(&m, (3 * U, -U)), Some((1, 0)));
        // Past the east or north end: no cell.
        assert_eq!(cell_of(&m, (6 * U, U)), None);
        assert_eq!(cell_of(&m, (U, 4 * U)), None);
        assert_eq!(cell_of(&PolyMap::default(), (0, 0)), None, "a grid without a cell size");
        // The origin is taken off first.
        let cells = [CellInput { header: [50; 8], polys: Vec::new() }, CellInput { header: [50; 8], polys: Vec::new() }];
        let off = PolyMap::build((-4 * U, 10 * U), 3 * U, 2, 1, &cells, Vec::new());
        assert_eq!(cell_of(&off, (-U - 1, 11 * U)), Some((0, 0)));
        assert_eq!(cell_of(&off, (-U, 12 * U)), Some((1, 0)));
    }

    #[test]
    fn the_region_test_reads_the_polygon_under_the_position() {
        // Region ids 0 and 1 are single regions, 2 is the group of both; 1023 names nothing.
        let sets = vec![vec![0], vec![1], vec![0, 1]];
        let m = grid_of(4, 1, sets, |c, _| {
            let x0 = c as i32 * 2 * U;
            let sq = square(x0, 0, x0 + 2 * U, 2 * U);
            let poly = |region_id: u32| PolyInput { kind: kind::LAND, region_id, outline: sq.clone() };
            match c {
                0 => vec![poly(0)],
                1 => vec![poly(1)],
                2 => vec![poly(2)],
                // A polygon without a region id is passed over for the next one holding the position.
                _ => vec![poly(1023), poly(1)],
            }
        });
        assert!(in_region(&m, (U, U), 0));
        assert!(!in_region(&m, (U, U), 1));
        assert!(in_region(&m, (3 * U, U), 1));
        assert!(!in_region(&m, (3 * U, U), 0));
        assert!(in_region(&m, (5 * U, U), 0), "a group id lists the region");
        assert!(in_region(&m, (5 * U, U), 1));
        assert!(!in_region(&m, (5 * U, U), 2));
        assert!(in_region(&m, (7 * U, U), 1));
        assert!(!in_region(&m, (7 * U, U), 0));
        assert!(!in_region(&m, (9 * U, U), 1), "past the grid");
        assert!(!in_region(&m, (U, 2 * U), 0), "the north edge is outside, and so is the grid's end");
        // The first polygon holding the position decides, whatever follows it.
        let both = grid_of(1, 1, vec![vec![0], vec![1]], |_, _| {
            vec![
                PolyInput { kind: kind::LAND, region_id: 0, outline: square(0, 0, 2 * U, 2 * U) },
                PolyInput { kind: kind::LAND, region_id: 1, outline: square(0, 0, 2 * U, 2 * U) },
            ]
        });
        assert!(in_region(&both, (U, U), 0));
        assert!(!in_region(&both, (U, U), 1));
    }

    #[test]
    fn a_cells_priority_is_its_axis_distance_from_the_position() {
        let m = grid(4, 2, vec![vec![0]], |_, _| (kind::LAND, 0));
        // Cell (1, 0) spans x 2..4, z 0..2.
        assert_eq!(cell_priority(&m, 1, 0, (3 * U, U)), -1.0, "its centre: one unit inside every edge");
        assert_eq!(cell_priority(&m, 1, 0, (3 * U, H)), -0.5);
        assert_eq!(cell_priority(&m, 1, 0, (2 * U, U)), 0.0, "on its edge");
        assert_eq!(cell_priority(&m, 1, 0, (7 * U, U)), 3.0);
        assert_eq!(cell_priority(&m, 1, 0, (U, U)), 1.0);
        // Diagonally off: the larger of the two axis distances, not the corner's distance.
        assert_eq!(cell_priority(&m, 1, 0, (7 * U, 4 * U)), 3.0);
        assert_eq!(cell_priority(&m, 1, 0, (5 * U, 7 * U)), 5.0);
    }

    #[test]
    fn the_segment_point_is_the_clamped_projection() {
        let (a, b) = ((0, 0), (4 * U, 0));
        assert_eq!(segment_point(a, b, (U + H, 5 * U)), (U + H, 0));
        assert_eq!(segment_point(a, b, (-3 * U, U)), a);
        assert_eq!(segment_point(a, b, (9 * U, U)), b);
        assert_eq!(segment_point(a, a, (9 * U, U)), a, "no length");
        // The parameter is single precision: on this diagonal it is 16 x (1 / sqrt 32)^2, a little
        // under one half, and truncated; so the point lies 4 raw units before the middle.
        assert_eq!(segment_point((0, 4 * U), (4 * U, 0), (4 * U, 4 * U)), (2 * U - 4, 2 * U + 4));
        // The parameter is truncated to Fixed20 and the product shifted down: a third of 3 raw
        // units is 0.99999 of one, so the point stays on the first.
        assert_eq!(segment_point((0, 0), (3, 0), (1, 0)), (0, 0));
    }

    /// A quadrilateral in raw units with a slanted west edge from (0, 0) to (1, 3).
    fn slanted() -> Vec<Point> {
        vec![(0, 0), (10, 0), (10, 3), (1, 3)]
    }

    fn snap(outline: &[Point], px: i32, row: i32) -> Option<(u32, i32)> {
        row_snap(outline, px, row, &mut Vec::new())
    }

    #[test]
    fn the_row_snap_takes_the_nearest_place_of_the_rows_spans() {
        let sq = square(0, 0, 2 * U, 2 * U);
        assert_eq!(snap(&sq, U, U), Some((0, U)), "inside the span: no cost, the position itself");
        assert_eq!(snap(&sq, 0, U), Some((0, 0)), "on the left end");
        assert_eq!(snap(&sq, -3, U), Some((9, 0)), "left of it: the left end");
        assert_eq!(snap(&sq, 2 * U, U), Some((0, 2 * U - 1)), "the right end is outside: one before it");
        assert_eq!(snap(&sq, 2 * U + 2, U), Some((4, 2 * U - 1)));
        assert_eq!(snap(&sq, U, 0), Some((0, U)), "the south row is in");
        assert_eq!(snap(&sq, U, 2 * U), None, "the north row is not");
        assert_eq!(snap(&sq, U, -1), None);
        assert_eq!(snap(&[], U, U), None);
        // Two spans on one row: the nearer one wins, the first on a tie.
        let comb: Vec<Point> = vec![(0, 0), (10, 0), (10, 4), (8, 4), (8, 1), (2, 1), (2, 4), (0, 4)];
        assert_eq!(snap(&comb, 3, 2), Some((1, 1)), "one past the left tooth, which ends before 2");
        assert_eq!(snap(&comb, 6, 2), Some((4, 8)));
        assert_eq!(snap(&comb, 5, 2), Some((9, 1)), "3 from either tooth: the first found stays");
    }

    #[test]
    fn a_left_end_off_its_edge_moves_in_by_one() {
        let q = slanted();
        // Row 0: the edge starts at x 0 exactly.
        assert_eq!(snap(&q, 0, 0), Some((0, 0)));
        // Row 1 crosses at x 1/3, rounded to 0, which the inside test puts outside: 1.
        assert_eq!(snap(&q, 0, 1), Some((1, 1)));
        // Row 2 crosses at x 2/3, rounded to 1: inside, kept.
        assert_eq!(snap(&q, 0, 2), Some((1, 1)));
        // An edge leaning the other way rounds half away from zero too: (1, 0) to (0, 3).
        let r: Vec<Point> = vec![(1, 0), (10, 0), (10, 3), (0, 3)];
        assert_eq!(snap(&r, -5, 1), Some((36, 1)), "2/3 rounds to 1");
        assert_eq!(snap(&r, -5, 2), Some((36, 1)), "1/3 rounds to 0, moved in to 1");
    }

    #[test]
    fn the_row_search_looks_above_and_below_while_rows_are_cheaper() {
        let search = |outline: &[Point], p: Point| row_search(outline, p, &mut Vec::new());
        let sq = square(0, 0, 2 * U, 2 * U);
        assert_eq!(search(&sq, (U, U)), Some((U, U)));
        // On the north edge (outside): the row below, at cost 1.
        assert_eq!(search(&sq, (U, 2 * U)), Some((U, 2 * U - 1)));
        assert_eq!(search(&sq, (U, 2 * U + 2)), Some((U, 2 * U - 1)));
        // Below the outline: the rows under it are skipped, its south row is taken.
        assert_eq!(search(&sq, (U, -2)), Some((U, 0)));
        // A farther row with a nearer span wins over the position's own row: row 2 costs 9
        // (x 1), row 0 costs 4 + 4 (x 0).
        assert_eq!(search(&slanted(), (-2, 2)), Some((0, 0)));
        // A nearer row keeps it: from (-2, 3), row 2 costs 9 + 1; rows 1 and 0 cost 13.
        assert_eq!(search(&slanted(), (-2, 3)), Some((1, 2)));
        assert_eq!(search(&[], (0, 0)), None);
        // An outline of no area has nothing on any row; the search ends at its z range.
        assert_eq!(search(&[(0, 0), (5, 0)], (2, 0)), None);
        assert_eq!(search(&[(0, 0), (0, 40), (0, 0)], (0, 20)), None);
    }

    #[test]
    fn the_nearest_point_of_an_outline_is_a_place_inside_it() {
        let near = |outline: &[Point], p: Point| nearest_point(outline, p, &mut Vec::new());
        let sq = square(0, 0, 2 * U, 2 * U);
        assert_eq!(near(&sq, (-3 * U, U)), Some((0, U)), "the west edge is in");
        assert_eq!(near(&sq, (5 * U, U)), Some((2 * U - 1, U)), "the east edge is not");
        assert_eq!(near(&sq, (U, -U)), Some((U, 0)));
        assert_eq!(near(&sq, (U, 9 * U)), Some((U, 2 * U - 1)));
        assert_eq!(near(&sq, (9 * U, 9 * U)), Some((2 * U - 1, 2 * U - 1)), "the north-east corner");
        assert_eq!(near(&sq, (-U, -U)), Some((0, 0)));
        assert_eq!(near(&[], (0, 0)), None);
        for p in [(-3 * U, U), (5 * U, U), (U, 9 * U), (9 * U, 9 * U), (-U, -U)] {
            assert!(near(&sq, p).is_some_and(|q| inside(&sq, q)), "{p:?}");
        }
    }

    #[test]
    fn a_position_on_the_regions_land_is_its_own_place() {
        let m = grid(3, 1, vec![vec![0]], |_, _| (kind::LAND, 0));
        assert_eq!(flood(&m.view(), (3 * U + 5, U - 7), 0, 100.0), Some((3 * U + 5, U - 7)));
        assert_eq!(flood(&m.view(), (9 * U, U), 0, 100.0), None, "no cell under the position");
        assert_eq!(flood(&m.view(), (U, U), 4, 100.0), None, "a region the grid does not hold");
        // West of the grid the first column is searched from.
        assert_eq!(flood(&m.view(), (-3 * U, U), 0, 100.0), Some((0, U)));
    }

    #[test]
    fn the_flood_finds_the_regions_nearest_land_within_the_radius() {
        // Sea, sea, land of region 0, sea, sea.
        let m = grid(5, 1, vec![vec![0], Vec::new()], |c, _| if c == 2 { (kind::LAND, 0) } else { (kind::SEA, 1) });
        assert_eq!(flood(&m.view(), (U, U), 0, 100.0), Some((4 * U, U)));
        assert_eq!(flood(&m.view(), (9 * U, U), 0, 100.0), Some((6 * U - 1, U)), "from the east: one before the edge");
        // The place is 3 away: found only when the radius is larger.
        assert_eq!(flood(&m.view(), (U, U), 0, 3.5), Some((4 * U, U)));
        assert_eq!(flood(&m.view(), (U, U), 0, 3.0), None);
        assert_eq!(flood(&m.view(), (U, U), 0, 1.0), None);
    }

    #[test]
    fn only_the_regions_own_polygons_of_a_fitting_kind_are_places() {
        // The position stands in cell 0 (sea); cell 1 is tried as `near`, cell 3 is region 0's land.
        let with = |near: (u8, u32)| {
            let sets = vec![vec![0], vec![1], vec![0, 1]];
            let m = grid(4, 1, sets, move |c, _| match c {
                1 => near,
                3 => (kind::LAND, 0),
                _ => (kind::SEA, 1023),
            });
            flood(&m.view(), (U, U), 0, 100.0)
        };
        let (near, far) = (Some((2 * U, U)), Some((6 * U, U)));
        assert_eq!(with((kind::LAND, 0)), near);
        assert_eq!(with((kind::ROAD, 0)), near);
        assert_eq!(with((GOAL_ZONE_KIND, 0)), near);
        assert_eq!(with((TARGET_CORE_KIND, 0)), near);
        assert_eq!(with((kind::SEA, 0)), far);
        assert_eq!(with((kind::RIVER, 0)), far);
        assert_eq!(with((kind::SHARED, 0)), far);
        assert_eq!(with((ZONE_KIND, 0)), far, "an enemy zone of control");
        assert_eq!(with((CORE_KIND, 0)), far, "another character's core");
        assert_eq!(with((kind::LAND, 1)), far, "another region's land");
        assert_eq!(with((kind::LAND, 2)), far, "a group id that lists the region is not its own");
        assert_eq!(with((kind::LAND, 1023)), far);
    }

    #[test]
    fn equally_near_places_are_taken_in_the_library_heaps_order() {
        // The position is the centre of the middle cell of a 3 x 3 sea. Two land polygons lie
        // exactly 2.5 away: one in the north cell (1, 2), nearest at (3, 5.5), one in the north-east
        // cell (2, 2), nearest at its corner (4.5, 5). All eight neighbours have priority 1; the
        // 3 x 3 block adds (1, 2) before (2, 2), but the library's heap gives the cells back as
        // (2, 0), (0, 2), (2, 2), (2, 1), (1, 2), ..., so the north-east polygon is scanned first
        // and the equally near northern one does not replace it.
        let m = grid_of(3, 3, vec![vec![0]], |c, r| match (c, r) {
            (1, 2) => vec![PolyInput { kind: kind::LAND, region_id: 0, outline: square(2 * U + H, 5 * U + H, 3 * U + H, 6 * U) }],
            (2, 2) => vec![PolyInput { kind: kind::LAND, region_id: 0, outline: square(4 * U + H, 5 * U, 5 * U + H, 6 * U) }],
            _ => vec![PolyInput { kind: kind::SEA, region_id: 0, outline: square(c as i32 * 2 * U, r as i32 * 2 * U, (c as i32 + 1) * 2 * U, (r as i32 + 1) * 2 * U) }],
        });
        let p = (3 * U, 3 * U);
        assert_eq!(distance_key(p, (3 * U, 5 * U + H)).sqrt(), 2.5);
        assert_eq!(distance_key(p, (4 * U + H, 5 * U)).sqrt(), 2.5);
        assert_eq!(flood(&m.view(), p, 0, 100.0), Some((4 * U + H, 5 * U)));
        // Each alone is found.
        let only = |cell: (u32, u32)| {
            let m = grid_of(3, 3, vec![vec![0]], |c, r| match (c, r) {
                (1, 2) if cell == (1, 2) => vec![PolyInput { kind: kind::LAND, region_id: 0, outline: square(2 * U + H, 5 * U + H, 3 * U + H, 6 * U) }],
                (2, 2) if cell == (2, 2) => vec![PolyInput { kind: kind::LAND, region_id: 0, outline: square(4 * U + H, 5 * U, 5 * U + H, 6 * U) }],
                _ => Vec::new(),
            });
            flood(&m.view(), p, 0, 100.0)
        };
        assert_eq!(only((1, 2)), Some((3 * U, 5 * U + H)));
        assert_eq!(only((2, 2)), Some((4 * U + H, 5 * U)));
    }

    #[test]
    fn a_nearer_place_in_a_later_cell_still_wins() {
        // From the centre of cell (1, 1): the west cell's place is one raw unit farther than the
        // east cell's (its east edge is outside), whichever is scanned first.
        let m = grid(3, 3, vec![vec![0]], |c, r| if r == 1 && c != 1 { (kind::LAND, 0) } else { (kind::SEA, 0) });
        assert_eq!(flood(&m.view(), (3 * U, 3 * U), 0, 100.0), Some((4 * U, 3 * U)));
    }

    #[test]
    fn a_core_cut_into_the_view_is_not_a_place() {
        let m = grid(4, 4, vec![vec![0]], |_, _| (kind::LAND, 0));
        let centre = (4 * U, 4 * U);
        let ov = rtcut::build(&m, &[Cut { shape: Shape::Convex(rtcut::circle((4.0, 4.0), 1.0)), kind: CORE_KIND }]);
        let view = m.with(&ov);
        assert_eq!(flood(&m.view(), centre, 0, 100.0), Some(centre), "the bare grid");
        let q = flood(&view, centre, 0, 100.0).expect("land round the core");
        // Just outside the 24-sided core of radius 1 (its sides are 0.9914 from the centre).
        let d = distance_key(centre, q).sqrt();
        assert!((0.99..=1.01).contains(&d), "{q:?} is {d} from the core's centre");
        let on = view.polygon_at(q.0 as f32 * FIX, q.1 as f32 * FIX).map(|p| view.kind(p));
        assert_ne!(on, Some(CORE_KIND));
        // A zone of control over the whole grid leaves no place at all.
        let all: Vec<u32> = (0..m.len() as u32).collect();
        let zone = rtcut::build(&m, &[Cut { shape: Shape::Polygons(all.into()), kind: ZONE_KIND }]);
        assert_eq!(flood(&m.with(&zone), centre, 0, 100.0), None);
    }
}

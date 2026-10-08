//! The OUTFLANK point test (`0x0078E3E0`) and side choice (`0x007AE2C0`), CONFIRMED round 6
//! (`analysis/ai/AI_RESEARCH.md` §3.1d "Outflank point test").
//!
//! * The target battlegroup's rectangle (centre, 16-bit facing, width, depth) is the reference.
//! * For a side angle `s` (± `DAT_0150BD14` = 16384, i.e. ±90° off the target's facing) and a
//!   distance `d` (200 m in CLOSE APPROACH, else 100 m, `0x007AA0D0`), up to 10 tries `k = 0..9`
//!   turn the direction by `0, +1, −1, +3, −3, +5, −5, +7, −7, +9` × 1820 units (≈ 10°); the point
//!   `centre + dir × d` is pushed out by 10 m steps while it is closer than `d` to the target
//!   rectangle, then pulled back by `0.2 d` up to 4 times while it is outside the battle area;
//!   the first point whose `d × d` square (oriented along `dir`) passes the terrain test wins.
//! * With points on both sides, the cost `≈|point − our group centre| − 5 × min(h(target) −
//!   h(point), 15)` picks the side (`≤`: the `+` side); `≈` is the exe's bit-trick square root.

use super::geom::{P, add, dot, scale, sub};

/// `DAT_0150BD14`: the outflank side angle, 16384 units (90°), written by `0x00415BB0`.
pub const SIDE_ANGLE: i32 = 16384;
/// The per-try turn: `round(1820.4443)` units (10°).
pub const TURN_STEP: i32 = 1820;
/// Units per full turn of the 16-bit angle.
pub const FULL_TURN: f32 = 65536.0;

/// The target battlegroup's rectangle (the `0x0078B070` record).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetRect {
    pub centre: P,
    /// Facing in 16-bit angle units (0..65535, counter-clockwise from +x).
    pub angle: i32,
    /// Extent across the facing.
    pub width: f32,
    /// Extent along the facing.
    pub depth: f32,
}

/// The battle area (`map +0x84..+0x90`: min x, min y, max x, max y; min inclusive, max exclusive).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub min: P,
    pub max: P,
}

impl Area {
    pub fn contains(&self, p: P) -> bool {
        self.min.0 <= p.0 && self.min.1 <= p.1 && p.0 < self.max.0 && p.1 < self.max.1
    }
}

/// Unit direction of a 16-bit angle (the exe reads a cos/sin table, `0x0176CFF8`).
pub fn dir16(angle: i32) -> P {
    let a = (angle & 0xFFFF) as f32 * std::f32::consts::TAU / FULL_TURN;
    (a.cos(), a.sin())
}

/// A radians angle in 16-bit units.
pub fn to16(radians: f32) -> i32 {
    ((radians / std::f32::consts::TAU * FULL_TURN).round() as i64).rem_euclid(65536) as i32
}

/// `0x005C3250`: squared distance from `p` to the rectangle, 0 inside.
pub fn rect_dist2(t: &TargetRect, p: P) -> f32 {
    let f = dir16(t.angle);
    let r = (f.1, -f.0);
    let d = sub(p, t.centre);
    let along = dot(d, f).abs() - t.depth * 0.5;
    let across = dot(d, r).abs() - t.width * 0.5;
    let (a, c) = (along.max(0.0), across.max(0.0));
    a * a + c * c
}

/// The turn of try `k` (`0x0078E3E0`): even `k` → `−max(k − 1, 0)` steps, odd → `+k` steps.
pub fn try_offset(k: i32) -> i32 {
    if k & 1 == 0 { -(k - 1).max(0) * TURN_STEP } else { k * TURN_STEP }
}

/// `0x0078E3E0`: the first valid outflank point at distance `d` and side angle `side` (± [`SIDE_ANGLE`])
/// from the target. `valid(point, dir, half)` is the terrain test of the `2·half` square
/// (PROVISIONAL in our callers: the battle area).
pub fn point_test(t: &TargetRect, d: f32, side: i32, area: Option<&Area>, valid: &dyn Fn(P, P, f32) -> bool) -> Option<P> {
    let d2 = d * d;
    let back = d * 0.2;
    let base = (t.angle + side) as i16 as i32;
    for k in 0..10 {
        let a = (try_offset(k) + base) & 0xFFFF;
        let dir = dir16(a);
        let mut p = add(t.centre, scale(dir, d));
        let mut guard = 0;
        while rect_dist2(t, p) < d2 && guard < 10_000 {
            p = add(p, scale(dir, 10.0));
            guard += 1;
        }
        if let Some(area) = area {
            for _ in 0..4 {
                if area.contains(p) {
                    break;
                }
                p = sub(p, scale(dir, back));
            }
        }
        if valid(p, dir, d * 0.5) {
            return Some(p);
        }
    }
    None
}

/// The exe's approximate square root (`((bits − 0x3F800000) >> 1) + 0x3F800000`).
pub fn fast_sqrt(x: f32) -> f32 {
    f32::from_bits((((x.to_bits() as i32).wrapping_sub(0x3F80_0000) >> 1).wrapping_add(0x3F80_0000)) as u32)
}

/// `0x007AE2C0`: the outflank side and point. Returns `(+1, point)` for the `+90°` side,
/// `(−1, point)` for `−90°`; `None` when neither side has a point. `height` is the ground height
/// (`0x0064C5F0`).
pub fn choose_side(
    t: &TargetRect,
    d: f32,
    our_centre: P,
    area: Option<&Area>,
    valid: &dyn Fn(P, P, f32) -> bool,
    height: &dyn Fn(P) -> f32,
) -> Option<(i32, P)> {
    let minus = point_test(t, d, -SIDE_ANGLE, area, valid);
    let plus = point_test(t, d, SIDE_ANGLE, area, valid);
    match (minus, plus) {
        (None, None) => None,
        (Some(m), None) => Some((-1, m)),
        (None, Some(p)) => Some((1, p)),
        (Some(m), Some(p)) => {
            let cost = |q: P| {
                let dd = sub(our_centre, q);
                let rise = (height(t.centre) - height(q)).min(15.0);
                fast_sqrt(dd.0 * dd.0 + dd.1 * dd.1) - rise * 5.0
            };
            if cost(p) <= cost(m) { Some((1, p)) } else { Some((-1, m)) }
        }
    }
}

/// `0x007AA0D0`: the outflank distance, 200 m in CLOSE APPROACH (phase 4), else 100 m.
pub fn distance(close_approach: bool) -> f32 {
    if close_approach { 200.0 } else { 100.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> TargetRect {
        TargetRect { centre: (0.0, 0.0), angle: 0, width: 300.0, depth: 40.0 }
    }

    #[test]
    fn offsets_alternate() {
        let v: Vec<i32> = (0..10).map(|k| try_offset(k) / TURN_STEP).collect();
        assert_eq!(v, vec![0, 1, -1, 3, -3, 5, -5, 7, -7, 9]);
    }

    #[test]
    fn points_clear_the_target_by_d() {
        let t = target();
        let all = |_: P, _: P, _: f32| true;
        // Facing +x; +90° is +y, beyond the 150 m half-width: pushed out until 200 m clear.
        let p = point_test(&t, 200.0, SIDE_ANGLE, None, &all).unwrap();
        assert!(p.0.abs() < 1e-3 && p.1 >= 350.0 && p.1 < 360.0, "{p:?}");
        assert!(rect_dist2(&t, p) >= 200.0 * 200.0);
        // A battle area that ends at y = 300 pulls the point back (4 × 40 m at most).
        let area = Area { min: (-1000.0, -1000.0), max: (1000.0, 300.0) };
        let inside = |q: P, _: P, _: f32| area.contains(q);
        let q = point_test(&t, 200.0, SIDE_ANGLE, Some(&area), &inside).unwrap();
        assert!(area.contains(q), "{q:?}");
    }

    #[test]
    fn side_choice_prefers_the_nearer_point() {
        let t = target();
        let all = |_: P, _: P, _: f32| true;
        let flat = |_: P| 0.0;
        let (s, p) = choose_side(&t, 100.0, (0.0, -500.0), None, &all, &flat).unwrap();
        assert_eq!(s, -1);
        assert!(p.1 < 0.0);
        let (s, _) = choose_side(&t, 100.0, (0.0, 0.0), None, &all, &flat).unwrap();
        assert_eq!(s, 1, "ties go to the + side");
        assert!((fast_sqrt(16.0) - 4.0).abs() < 0.5);
    }
}

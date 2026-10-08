//! Small 2D helpers on battlefield metres `(x, y)`. Plain f32 maths, so results are the same on
//! every machine for the same inputs (no randomness, no parallelism).

/// A point or vector on the battlefield plane, in metres.
pub type P = (f32, f32);

/// `a + b`.
pub fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}

/// `a - b`.
pub fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

/// `a * k`.
pub fn scale(a: P, k: f32) -> P {
    (a.0 * k, a.1 * k)
}

/// Length of `a`.
pub fn len(a: P) -> f32 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}

/// Distance between two points.
pub fn dist(a: P, b: P) -> f32 {
    len(sub(a, b))
}

/// `a` scaled to length 1, or `fallback` when `a` is (almost) zero.
pub fn norm_or(a: P, fallback: P) -> P {
    let l = len(a);
    if l > 1e-4 { scale(a, 1.0 / l) } else { fallback }
}

/// The right-hand perpendicular of a facing direction `d` (x right, y up): `(d.y, -d.x)`.
pub fn right_of(d: P) -> P {
    (d.1, -d.0)
}

/// Wraps an angle to (-π, π].
pub fn wrap(mut a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    while a > PI {
        a -= TAU;
    }
    while a <= -PI {
        a += TAU;
    }
    a
}

/// Mean of a list of points, or `None` for an empty list.
pub fn centroid(points: impl IntoIterator<Item = P>) -> Option<P> {
    let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0u32);
    for p in points {
        sx += p.0;
        sy += p.1;
        n += 1;
    }
    (n > 0).then(|| (sx / n as f32, sy / n as f32))
}

/// Dot product.
pub fn dot(a: P, b: P) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

/// `a` rotated by `radians` (counter-clockwise).
pub fn rotate(a: P, radians: f32) -> P {
    let (s, c) = radians.sin_cos();
    (a.0 * c - a.1 * s, a.0 * s + a.1 * c)
}

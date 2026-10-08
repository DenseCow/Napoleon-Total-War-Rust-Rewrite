//! SpeedTree's text-encoded Bezier splines (`"BezierSpline min max variance { n  x y tx ty w ... }"`),
//! embedded as strings in the `.spt` (branch level profiles, frond and supplemental curves).
//!
//! Evidence (exe, `analysis/speedtree/SPEEDTREE.md` §3): the text is split into whitespace tokens
//! (`sscanf("%s")` after skipping `' ' \t \n \r`); numbers go through SpeedTreeRT's own float
//! parser (not the C runtime's), which we copy in [`parse_float`]. Each point is
//! `x y tx ty w`: a position, a tangent direction (normalised on load) and a tangent length. The
//! cubic segments are `P[i], P[i] + T[i]·w[i], P[i+1] − T[i+1]·w[i+1], P[i+1]` (CONFIRMED). After
//! loading, the curve is sampled into a 500-entry lookup table (CONFIRMED: `FUN_012c11a0(500)`).

/// Size of the sampled table the exe builds for every spline (CONFIRMED).
pub const SPLINE_SAMPLES: usize = 500;

/// One spline control point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplinePoint {
    /// Position `(x, y)`; x is the curve parameter (0..1 along the branch), y the value (0..1).
    pub pos: [f32; 2],
    /// Tangent direction, normalised on load like the exe does.
    pub tangent: [f32; 2],
    /// Tangent length (the Bezier handle length on both sides of the point).
    pub weight: f32,
}

/// A parsed `BezierSpline`.
#[derive(Debug, Clone, PartialEq)]
pub struct BezierSpline {
    /// First header number. The exe stores it at field +4 (INFERRED: the low end of the value range).
    pub min: f32,
    /// Second header number, field +8 (INFERRED: the high end of the value range).
    pub max: f32,
    /// Third header number, field +0 (INFERRED: random variance added to the value).
    pub variance: f32,
    /// Control points in file order.
    pub points: Vec<SplinePoint>,
    /// The exe's 500-entry sample table: `(x, y)` pairs (see [`Self::build_table`]).
    pub table: Vec<[f32; 2]>,
}

impl Default for BezierSpline {
    /// The values of a spline that is never loaded (CONFIRMED, `FUN_012c0d80`): all zero except
    /// the second header field (+8) which is 1.0, no points and no table.
    fn default() -> Self {
        Self { min: 0.0, max: 1.0, variance: 0.0, points: Vec::new(), table: Vec::new() }
    }
}

/// SpeedTreeRT's float parser (`FUN_012c09f0`, CONFIRMED by decompile): leading whitespace,
/// optional sign, digits accumulated as `v = v * 10 + d` in single precision, a fraction added as
/// `v += d * f` with `f` starting at 0.1 and multiplied by 0.1 per digit, and an optional
/// `e`/`E` exponent applied as `v *= 10^exp`. Anything else ends the number.
pub fn parse_float(s: &str) -> f32 {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut v = 0.0f32;
    while i < b.len() && b[i].is_ascii_digit() {
        v = v * 10.0 + f32::from(b[i] - b'0');
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let mut f = 0.1f32;
        while i < b.len() && b[i].is_ascii_digit() {
            v += f32::from(b[i] - b'0') * f;
            f *= 0.1;
            i += 1;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let exp = atoi(&s[i + 1..]);
        v *= 10f32.powi(exp);
    }
    if neg { -v } else { v }
}

/// The C runtime `atoi` (leading whitespace, sign, digits).
fn atoi(s: &str) -> i32 {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r']);
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut v: i64 = 0;
    for c in t.bytes().take_while(u8::is_ascii_digit) {
        v = (v * 10 + i64::from(c - b'0')).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl BezierSpline {
    /// Parses the spline text. Like the exe, text that does not start with `BezierSpline` gives
    /// the default spline, and a missing `{` gives a spline with header values but no points.
    pub fn parse(text: &str) -> Self {
        let mut toks = text.split([' ', '\t', '\n', '\r']).filter(|t| !t.is_empty());
        let mut s = Self::default();
        if toks.next() != Some("BezierSpline") {
            return s;
        }
        let next_f = |t: &mut dyn Iterator<Item = &str>| t.next().map(parse_float).unwrap_or(0.0);
        s.min = next_f(&mut toks);
        s.max = next_f(&mut toks);
        s.variance = next_f(&mut toks);
        if toks.next() == Some("{") {
            let n = toks.next().map(atoi).unwrap_or(0);
            for _ in 0..n.max(0) {
                let x = next_f(&mut toks);
                let y = next_f(&mut toks);
                let tx = next_f(&mut toks);
                let ty = next_f(&mut toks);
                let w = next_f(&mut toks);
                // FUN_012bd540: v *= 1/len when len != 0.
                let len = (tx * tx + ty * ty).sqrt();
                let tangent = if len != 0.0 { [tx * (1.0 / len), ty * (1.0 / len)] } else { [tx, ty] };
                s.points.push(SplinePoint { pos: [x, y], tangent, weight: w });
            }
        }
        s.build_table(SPLINE_SAMPLES);
        s
    }

    /// The Bezier control polygon: `P0, P0+T0·w0, P1−T1·w1, P1, P1+T1·w1, ...` (CONFIRMED).
    pub fn control_points(&self) -> Vec<[f32; 2]> {
        let mut c = Vec::with_capacity(self.points.len() * 3);
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                let q = &self.points[i - 1];
                c.push([q.pos[0] + q.tangent[0] * q.weight, q.pos[1] + q.tangent[1] * q.weight]);
                c.push([p.pos[0] - p.tangent[0] * p.weight, p.pos[1] - p.tangent[1] * p.weight]);
            }
            c.push(p.pos);
        }
        c
    }

    /// Builds the exe's sample table (`FUN_012c11a0`, CONFIRMED structure; only for 2+ points):
    /// 1. walk the cubic segments with a parameter step of `(points − 1) / (samples − 1)`,
    ///    carrying the remainder into the next segment, evaluating each sample by de Casteljau
    ///    (at most `samples` values, then the last point if room is left);
    /// 2. resample that polyline at uniform x: entry 0 is the first point, entry `i` (1 ≤ i <
    ///    samples − 1) is the polyline's y at x = i / samples, the last entry is the last point.
    ///
    /// Entry `i` of the stored table is `(x, y)` with `x = i / samples` (entry 0 and the last hold
    /// the end points' x).
    pub fn build_table(&mut self, samples: usize) {
        self.table.clear();
        let n = self.points.len();
        if n < 2 || samples < 2 {
            return;
        }
        let c = self.control_points();
        let lerp = |a: f32, b: f32, t: f32| (b - a) * t + a;
        let mut poly: Vec<[f32; 2]> = Vec::with_capacity(samples);
        let step = (n - 1) as f32 / (samples - 1) as f32;
        let mut t = 0.0f32;
        'segs: for seg in 0..n - 1 {
            let p = &c[seg * 3..seg * 3 + 4];
            while t < 1.0 {
                if poly.len() >= samples {
                    break 'segs;
                }
                let ax = lerp(p[0][0], p[1][0], t);
                let bx = lerp(p[1][0], p[2][0], t);
                let cx = lerp(p[2][0], p[3][0], t);
                let ay = lerp(p[0][1], p[1][1], t);
                let by = lerp(p[1][1], p[2][1], t);
                let cy = lerp(p[2][1], p[3][1], t);
                let abx = lerp(ax, bx, t);
                let bcx = lerp(bx, cx, t);
                let aby = lerp(ay, by, t);
                let bcy = lerp(by, cy, t);
                poly.push([lerp(abx, bcx, t), lerp(aby, bcy, t)]);
                t += step;
            }
            t -= 1.0;
        }
        let last = self.points[n - 1].pos;
        if poly.len() < samples {
            poly.push(last);
        }
        let mut table = vec![[0.0f32; 2]; samples];
        table[0] = self.points[0].pos;
        let inv = 1.0 / samples as f32;
        let mut k = 0usize;
        for (i, entry) in table.iter_mut().enumerate().take(samples - 1).skip(1) {
            let x = i as f32 * inv;
            // advance to the polyline segment holding x
            while k < samples - 1 && k + 1 < poly.len() && (x < poly[k][0] || poly[k + 1][0] <= x) {
                k += 1;
            }
            let (a, b) = (poly[k], poly[(k + 1).min(poly.len() - 1)]);
            let f = if b[0] != a[0] { (x - a[0]) / (b[0] - a[0]) } else { 0.0 };
            *entry = [x, (b[1] - a[1]) * f + a[1]];
        }
        table[samples - 1] = last;
        self.table = table;
    }
}

impl BezierSpline {
    /// The table's y at entry `i` (0 when the spline has no table; PROVISIONAL: the exe's table
    /// memory of a never-loaded spline is not initialised by its constructor).
    fn y(&self, i: i32) -> f32 {
        self.table.get(i.clamp(0, SPLINE_SAMPLES as i32 - 1) as usize).map_or(0.0, |e| e[1])
    }

    /// `FUN_012c1650` (CONFIRMED): the profile value at `x` without randomness,
    /// `(max − min) · y(x) + min`, with `y` linearly interpolated between table entries `i/499`.
    pub fn eval_fixed(&self, x: f32) -> f32 {
        let i = (x * 499.0) as i32;
        let mut y = self.y(i);
        if i != 499 {
            y = (x - i as f32 * 0.002_004_008) * 499.0 * (self.y(i + 1) - y) + y;
        }
        (self.max - self.min) * y + self.min
    }

    /// `FUN_012c1540` (CONFIRMED): the profile value plus a random variance. Always draws one
    /// random number (`uniform(−variance, variance)`), scaled by `y(x)` when `scale_variance`.
    /// Returned in double precision like the exe's x87 result; callers round where it stores.
    pub fn eval(&self, x: f32, scale_variance: bool, rng: &mut super::rng::Newran) -> f64 {
        let i = (x * 499.0) as i32;
        let mut y = self.y(i);
        if i != 499 {
            y = (x - i as f32 * 0.002_004_008) * (self.y(i + 1) - y) * 499.0 + y;
        }
        let base = (self.max - self.min) * y + self.min;
        let mut r = rng.uniform(-self.variance, self.variance);
        if scale_variance {
            r *= f64::from(y);
        }
        r + f64::from(base)
    }

    /// `FUN_012c18e0` (CONFIRMED): only the random part, `uniform(−v, v)` with
    /// `v = variance · y(round(x · 499))`.
    pub fn variance_only(&self, x: f32, rng: &mut super::rng::Newran) -> f64 {
        let y = self.y((x * 499.0 + 0.5) as i32);
        let v = self.variance * y;
        rng.uniform(-v, v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // 0.707107 is the literal text being parsed, not an approximation of 1/sqrt(2).
    #[allow(clippy::approx_constant)]
    fn float_parser_matches_simple_values() {
        assert_eq!(parse_float("0"), 0.0);
        assert_eq!(parse_float("-90"), -90.0);
        assert!((parse_float("0.707107") - 0.707107).abs() < 1e-6);
        assert!((parse_float("-0.00356672") + 0.00356672).abs() < 1e-8);
        assert!((parse_float("1e-005") - 1e-5).abs() < 1e-10);
    }

    #[test]
    fn straight_line_spline() {
        let s = BezierSpline::parse(
            "BezierSpline 0 1 8\n{\n\t2\n\t0 0 0.707107 0.707107 0.079604\n\t1 1 0.707107 0.707107 0.107006\n\n}\n",
        );
        assert_eq!((s.min, s.max, s.variance), (0.0, 1.0, 8.0));
        assert_eq!(s.points.len(), 2);
        assert_eq!(s.table.len(), SPLINE_SAMPLES);
        // a diagonal line: y = x everywhere
        for e in &s.table {
            assert!((e[0] - e[1]).abs() < 1e-3, "{e:?}");
        }
        assert_eq!(s.table[SPLINE_SAMPLES - 1], [1.0, 1.0]);
    }

    #[test]
    fn not_a_spline_gives_default() {
        assert_eq!(BezierSpline::parse("garbage"), BezierSpline::default());
    }
}

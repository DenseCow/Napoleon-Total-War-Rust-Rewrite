//! SpeedWind2 (SpeedTree's wind engine as compiled into the exe), configured by
//! `RigidModels/Vegetation/Wind/SpeedWind.ini`. Spec: `analysis/speedtree/SPEEDTREE.md` §7.
//!
//! CONFIRMED from the exe: the ini keys and where they go (`FUN_012b8c40`), the damped followers
//! (`FUN_012b7be0`), the oscillators (`FUN_012b8990`) and `Advance` (`FUN_012b7c90`): gusts, strength,
//! direction, branch/leaf strength exponents, and the wind matrices (bend about the horizontal axis
//! across the wind by `oscY[i] + maxBend·strength`, then a turn by `oscX[i]`). Matrices are in
//! SpeedTree's Z-up space and used as `pos' = pos · M` (row vectors) by the vertex shaders.
//! PROVISIONAL: gusts draw from our own `rand()` (the exe shares the C runtime's global one).

use super::rng::MsvcRand;

/// A damped value (target, current, last velocity, last step) — `FUN_012b7be0`.
#[derive(Debug, Clone, Copy, Default)]
struct Follower {
    target: f32,
    current: f32,
    velocity: f32,
    last_dt: f32,
}

impl Follower {
    fn advance(&mut self, dt: f32, response: f32, limit: f32) {
        let dt = dt.min(0.03);
        let mut d = (self.target - self.current) * dt * response;
        if limit != 0.0 {
            let v = self.velocity;
            if v.abs() < d.abs() && 0.0 < dt && 0.0 < self.last_dt {
                d = (d / dt - v / self.last_dt).clamp(-limit, limit) * dt + v;
            }
            self.velocity = d;
            self.last_dt = dt;
        }
        self.current += d;
    }
}

/// One oscillator `La Ls Ha Hs` (low/high amplitude and speed) with a value per matrix/angle —
/// `FUN_012b8990`.
#[derive(Debug, Clone, Default)]
pub struct Oscillator {
    pub low_amplitude: f32,
    pub high_amplitude: f32,
    pub low_speed: f32,
    pub high_speed: f32,
    phase: f32,
    pub values: Vec<f32>,
}

impl Oscillator {
    fn advance(&mut self, dt: f32, strength: f32) {
        let amp = (self.high_amplitude - self.low_amplitude) * strength + self.low_amplitude;
        self.phase += ((self.high_speed - self.low_speed) * strength + self.low_speed) * dt;
        for (i, v) in self.values.iter_mut().enumerate() {
            *v = (i as f32 + self.phase).sin() * amp;
        }
    }
}

/// The five wind strengths the game ships, in m/s, weakest first.
///
/// **CONFIRMED** from `db\wind_levels_tables\wind_levels` (schema `s,s,f,f,i`, 10 rows, decoded
/// whole): every row's columns 3 and 4 are a wind vector and **every one has length 3, 6, 9, 12 or
/// 16** — `calm_westerly` (3, 0), `light_westerly` (6, 0), `moderate_westerly` (9, 0),
/// `strong_westerly` (12, 0), `storm_westerly` (16, 0) and the five `*_northern` rows
/// (0, −3), (0, −6), (0, −9), (0, −12), (0, −16). So the game's wind speeds are exactly this
/// ladder, and its only two wind directions are westerly and northerly (one axis each, never a
/// diagonal): **CONFIRMED, no other magnitude or direction occurs in the table** (install test
/// `speedtree_install::wind_ladder_is_the_shipped_wind_levels_table`).
///
/// The rows' second column is the wind audio sample (`wind_level_0` .. `wind_level_4`) and it
/// rises with the magnitude — 3 → level 0, 6 → 1, 9 → 2, 12 → 3, 16 → 4, identically for both
/// directions — so a wind's level is its position on this ladder. That the *exe* picks the same
/// audio by comparing a normalised `|v|` against 0.58317894 / 0.68722874 / 0.82029885 / 0.91646695
/// (an exe reading whose decompile is not kept: INFERRED)
/// does **not** follow from dividing by 16 (that gives 0.1875, 0.375, 0.5625, 0.75, 1.0, which the
/// thresholds would band 0, 0, 0, 2, 4), so the thresholds act on a **time-varying** battle wind,
/// not on this table's authored rows. The two are different mechanisms and this ladder does not
/// decode them.
pub const WIND_LADDER: [f32; 5] = [3.0, 6.0, 9.0, 12.0, 16.0];

/// The wind speed's level on [`WIND_LADDER`]: 0 (weakest) .. 4 (strongest), or `None` for a speed
/// the game's own table does not carry (still-air, and anything outside 3 ..= 16).
pub fn wind_level(speed: f32) -> Option<u32> {
    WIND_LADDER.iter().position(|&s| (s - speed).abs() < 1e-3).map(|i| i as u32)
}

/// The wind engine.
#[derive(Debug, Clone)]
pub struct SpeedWind {
    pub branch_x: Oscillator,
    pub branch_y: Oscillator,
    pub leaf_rocking: Oscillator,
    pub leaf_rustling: Oscillator,
    /// `MaxBendAngle` (degrees).
    pub max_bend: f32,
    /// `WindResponseAndLimit`.
    pub response: f32,
    pub response_limit: f32,
    /// `GustStrengthMinMax`, `GustFrequency`, `GustDurationMinMax`.
    pub gust_strength: [f32; 2],
    pub gust_frequency: f32,
    pub gust_duration: [f32; 2],
    /// `BranchExponent`, `LeafExponent`.
    pub branch_exponent: f32,
    pub leaf_exponent: f32,
    /// The base strength the game sets (0..1).
    pub strength: f32,
    last_time: f32,
    gust: f32,
    gust_stop: f32,
    final_strength: Follower,
    direction: [Follower; 3],
    bend: f32,
    rand: MsvcRand,
    /// The wind matrices (row-major 3×3, Z-up, used as `pos · M`).
    pub matrices: Vec<[f32; 9]>,
}

fn osc(n: usize) -> Oscillator {
    Oscillator { values: vec![0.0; n], ..Default::default() }
}

impl SpeedWind {
    /// Parses `SpeedWind.ini` (the keys `FUN_012b8c40` accepts).
    pub fn parse(text: &str) -> Self {
        let mut w = Self {
            branch_x: osc(6),
            branch_y: osc(6),
            leaf_rocking: osc(8),
            leaf_rustling: osc(8),
            max_bend: 0.0,
            response: 0.0,
            response_limit: 0.0,
            gust_strength: [0.0; 2],
            gust_frequency: 0.0,
            gust_duration: [0.0; 2],
            branch_exponent: 1.0,
            leaf_exponent: 1.0,
            strength: 0.0,
            last_time: 0.0,
            gust: 0.0,
            gust_stop: 0.0,
            final_strength: Follower::default(),
            direction: [Follower::default(); 3],
            bend: 0.0,
            rand: MsvcRand::default(),
            matrices: vec![[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]; 6],
        };
        for line in text.lines() {
            let mut it = line.split_whitespace();
            let Some(key) = it.next() else { continue };
            let v: Vec<f32> = it.filter_map(|s| s.parse().ok()).collect();
            let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
            let set_osc = |o: &mut Oscillator| {
                // file order La Ls Ha Hs; stored La, Ha, Ls, Hs
                o.low_amplitude = g(0);
                o.low_speed = g(1);
                o.high_amplitude = g(2);
                o.high_speed = g(3);
            };
            match key {
                "NumWindMatricesAndLeafAngles" => {
                    let (m, a) = ((g(0) as usize).max(1), (g(1) as usize).max(1));
                    w.branch_x.values = vec![0.0; m];
                    w.branch_y.values = vec![0.0; m];
                    w.leaf_rocking.values = vec![0.0; a];
                    w.leaf_rustling.values = vec![0.0; a];
                    w.matrices = vec![[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]; m];
                }
                "WindResponseAndLimit" => (w.response, w.response_limit) = (g(0), g(1)),
                "MaxBendAngle" => w.max_bend = g(0),
                "BranchExponent" => w.branch_exponent = g(0),
                "LeafExponent" => w.leaf_exponent = g(0),
                "GustStrengthMinMax" => w.gust_strength = [g(0), g(1)],
                "GustDurationMinMax" => w.gust_duration = [g(0), g(1)],
                "GustFrequency" => w.gust_frequency = g(0),
                "GustStrengthFreqDuration" => {
                    // the older key: strength ×0.25/×1.25, frequency, duration ×0.5/×1.5
                    w.gust_strength = [g(0) * 0.25, g(0) * 1.25];
                    w.gust_frequency = g(1);
                    w.gust_duration = [g(2) * 0.5, g(2) * 1.5];
                }
                "BranchOscillationX_LaLsHaHs" => set_osc(&mut w.branch_x),
                "BranchOscillationY_LaLsHaHs" => set_osc(&mut w.branch_y),
                "LeafRocking_LaLsHaHs" => set_osc(&mut w.leaf_rocking),
                "LeafRustling_LaLsHaHs" => set_osc(&mut w.leaf_rustling),
                _ => {}
            }
        }
        w
    }

    /// `SetWindStrengthAndDirection` (`FUN_012b9680`): strength clamped to 0..1, direction
    /// normalised, both as targets for the followers. Direction in SpeedTree axes (Z up).
    pub fn set_wind(&mut self, strength: f32, dir: [f32; 3]) {
        self.strength = strength.clamp(0.0, 1.0);
        let l2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        let d = if l2 != 0.0 {
            let i = 1.0 / l2.sqrt();
            [dir[0] * i, dir[1] * i, dir[2] * i]
        } else {
            dir
        };
        for (f, v) in self.direction.iter_mut().zip(d) {
            f.target = v;
        }
    }

    /// `Advance` (`FUN_012b7c90`) with the matrices updated (the game passes true) and the leaf
    /// angle matrices not (false).
    pub fn advance(&mut self, time: f32) {
        let dt = time - self.last_time;
        self.last_time = time;
        if self.gust_stop < time {
            self.gust = 0.0;
            if (self.rand.rand() as f32) * 3.051_851e-5 < self.gust_frequency * dt {
                let r = self.rand.rand() as f32;
                self.gust = (self.gust_strength[1] - self.gust_strength[0]) * r * 3.051_851e-5 + self.gust_strength[0];
                let r = self.rand.rand() as f32;
                self.gust_stop = (self.gust_duration[1] - self.gust_duration[0]) * r * 3.051_851e-5 + time + self.gust_duration[0];
            }
        }
        self.final_strength.target = (self.gust + self.strength).clamp(0.0, 1.0);
        self.final_strength.advance(dt, self.response, self.response_limit);
        self.final_strength.current = self.final_strength.current.clamp(0.0, 1.0);
        for f in &mut self.direction {
            f.advance(dt, self.response, self.response_limit);
        }
        let s = self.final_strength.current;
        let branch = s.powf(self.branch_exponent);
        let leaf = s.powf(self.leaf_exponent);
        let d = [self.direction[0].current, self.direction[1].current, self.direction[2].current];
        let len = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let len = if len != 0.0 { len.sqrt() } else { len };
        self.bend = self.max_bend * branch * len;
        self.branch_x.advance(dt, branch);
        self.branch_y.advance(dt, branch);
        self.leaf_rocking.advance(dt, leaf);
        self.leaf_rustling.advance(dt, leaf);
        let theta = d[0].atan2(d[1]);
        let (s0, c0) = (-theta).sin_cos();
        let (ax, ay) = (-c0, -s0);
        for (i, m) in self.matrices.iter_mut().enumerate() {
            let a = (self.branch_y.values[i] + self.bend) * 0.017_453_292;
            let (s, c) = a.sin_cos();
            let f = (1.0 - c) * ax;
            let m1 = [f * ax + c, f * ay, -(s * ay), f * ay, (1.0 - c) * ay * ay + c, s * ax, s * ay, -(s * ax), c];
            let b = self.branch_x.values[i] * 0.017_453_292;
            let (sb, cb) = b.sin_cos();
            let ay2 = s0;
            let g = (1.0 - cb) * ay2;
            let (r00, r01, r02) = (g * ay2 + cb, g * ax, -(sb * ax));
            let (r10, r11, r12) = (g * ax, (1.0 - cb) * ax * ax + cb, sb * ay2);
            let (r20, r21, r22) = (sb * ax, -(sb * ay2), cb);
            let row = |k: usize| {
                let (x, y, z) = (m1[3 * k], m1[3 * k + 1], m1[3 * k + 2]);
                [y * r10 + x * r00 + z * r20, y * r11 + x * r01 + z * r21, y * r12 + x * r02 + z * r22]
            };
            let (n0, n1, n2) = (row(0), row(1), row(2));
            *m = [n0[0], n0[1], n0[2], n1[0], n1[1], n1[2], n2[0], n2[1], n2[2]];
        }
    }

    /// The current wind strength after gusts and damping.
    pub fn current_strength(&self) -> f32 {
        self.final_strength.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = "NumWindMatricesAndLeafAngles 6 8\nWindResponseAndLimit 0.4 0.01\nMaxBendAngle 35\n\
        BranchExponent 2\nLeafExponent 2\nGustStrengthMinMax 0.25 1\nGustDurationMinMax 2 5\n\
        GustFrequency 0.00166667\nBranchOscillationX_LaLsHaHs 0.3 3 6 3\n";

    #[test]
    fn the_ladder_is_the_game_s_own_five_speeds() {
        assert_eq!(WIND_LADDER, [3.0, 6.0, 9.0, 12.0, 16.0]);
        for (i, &s) in WIND_LADDER.iter().enumerate() {
            assert_eq!(wind_level(s), Some(i as u32), "{s} m/s");
        }
        // still air and anything off the ladder have no level.
        assert_eq!(wind_level(0.0), None);
        assert_eq!(wind_level(5.0), None);
        assert_eq!(wind_level(17.0), None);
    }

    #[test]
    fn parses_and_bends() {
        let mut w = SpeedWind::parse(INI);
        assert_eq!(w.matrices.len(), 6);
        assert_eq!(w.leaf_rocking.values.len(), 8);
        assert_eq!((w.max_bend, w.response), (35.0, 0.4));
        w.set_wind(1.0, [1.0, 0.0, 0.0]);
        for k in 1..2000 {
            w.advance(k as f32 * 0.02);
        }
        // a strong wind bends the trees: matrices are rotations away from identity
        let m = w.matrices[0];
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6]) + m[2] * (m[3] * m[7] - m[4] * m[6]);
        assert!((det - 1.0).abs() < 1e-3, "rotation: det {det}");
        assert!(m[8] < 0.999, "bent: {m:?}");
    }
}

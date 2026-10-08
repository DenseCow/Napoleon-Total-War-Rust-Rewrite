//! The random number generators SpeedTreeRT uses, recreated bit for bit
//! (`analysis/speedtree/SPEEDTREE.md` §4).
//!
//! - [`Newran`]: Robert Davies' Newran base generator as linked into the exe (error strings
//!   "Newran: seed out of range", "Random number generator not initialised"): a MINSTD
//!   multiplicative congruential generator (a = 16807, m = 2^31 − 1, Schrage-style with wrapping
//!   32-bit arithmetic) feeding a 128-entry shuffle table of `f32`s. CONFIRMED from
//!   `FUN_012dc450` (seed), `FUN_012dc2a0` (next) and `FUN_012da0f0` (uniform in a range).
//! - [`MsvcRand`]: the C runtime `rand()`/`srand()` (`FUN_0127fb28` / `FUN_0127fb49`), used only for
//!   the trunk flares.

/// SpeedTreeRT's Newran generator (one global instance in the exe).
#[derive(Clone)]
pub struct Newran {
    state: i32,
    buffer: [f32; 128],
    seeded: bool,
}

impl std::fmt::Debug for Newran {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Newran").field("state", &self.state).finish_non_exhaustive()
    }
}

impl Default for Newran {
    fn default() -> Self {
        Self { state: 0, buffer: [0.0; 128], seeded: false }
    }
}

/// One MINSTD step exactly as the exe computes it (32-bit wrapping arithmetic).
fn minstd(s: i32) -> i32 {
    let v = s.wrapping_mul(16807).wrapping_add((s / 127_773).wrapping_mul(-0x7fff_ffff));
    if v > 0 { v } else { v.wrapping_add(0x7fff_ffff) }
}

impl Newran {
    /// Seeds with an integer (`FUN_012da110(seed)` for `seed != -1`, i.e. `FUN_012dc450`).
    pub fn seed(&mut self, seed: i32) {
        self.state = seed;
        for b in &mut self.buffer {
            self.state = minstd(self.state);
            *b = self.state as f32 * 4.656_613e-10;
        }
        self.seeded = true;
    }

    /// Whether a seed was ever set (the exe seeds from the clock the first time otherwise).
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// The next uniform number in [0, 1) (`FUN_012dc2a0`).
    #[allow(clippy::should_implement_trait)] // an endless generator, not an Iterator
    pub fn next(&mut self) -> f32 {
        let a = minstd(self.state);
        // PROVISIONAL: index 128 (a ≥ 2^31 − 64, probability ~3e-8) reads past the table in the
        // exe; we clamp.
        let i = ((a as f32 * 5.960_464_5e-8) as i32).clamp(0, 127) as usize;
        let r = self.buffer[i];
        let b = minstd(a);
        self.state = b;
        self.buffer[i] = b as f32 * 4.656_613e-10;
        r
    }

    /// Uniform in `[lo, hi)` (`FUN_012da0f0`: `r * (hi - lo) + lo` on the x87 stack; we keep the
    /// value in double precision like the callers do until they store it).
    pub fn uniform(&mut self, lo: f32, hi: f32) -> f64 {
        let r = f64::from(self.next());
        r * (f64::from(hi) - f64::from(lo)) + f64::from(lo)
    }
}

/// The MSVC C runtime `rand()` (LCG 214013 / 2531011, 15-bit output).
#[derive(Debug, Clone, Default)]
pub struct MsvcRand {
    state: u32,
}

impl MsvcRand {
    /// `srand(seed)`.
    pub fn srand(&mut self, seed: u32) {
        self.state = seed;
    }

    /// `rand()`: 0..=32767.
    pub fn rand(&mut self) -> i32 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.state >> 16) & 0x7fff) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minstd_matches_reference() {
        // The classic MINSTD check: starting from 1, the 10000th value is 1043618065.
        let mut s = 1;
        for _ in 0..10_000 {
            s = minstd(s);
        }
        assert_eq!(s, 1_043_618_065);
    }

    #[test]
    fn newran_is_deterministic_and_in_range() {
        let mut a = Newran::default();
        let mut b = Newran::default();
        a.seed(4897);
        b.seed(4897);
        for _ in 0..1000 {
            let (x, y) = (a.next(), b.next());
            assert_eq!(x.to_bits(), y.to_bits());
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn msvc_rand_sequence() {
        let mut r = MsvcRand::default();
        r.srand(1);
        // MSVC rand() with the default seed 1 starts 41, 18467, 6334, 26500.
        assert_eq!([r.rand(), r.rand(), r.rand(), r.rand()], [41, 18467, 6334, 26500]);
    }
}

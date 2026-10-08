//! The game's random number generator, `CaRng`.
//!
//! W1 §12.1 (CONFIRMED): the original uses the classic MSVC linear congruential generator
//! (`state = state * 214013 + 2531011`), but returns the **full** upper 16 bits (0..=65535) instead
//! of the 15-bit value that the C runtime's `rand()` returns. The same LCG is inlined at 732 sites;
//! every "owner" (the battle, the campaign, ...) keeps its own `u32` state.
//!
//! All arithmetic wraps around on overflow exactly like the 32-bit C++ code, which is why we use
//! `wrapping_mul` / `wrapping_add` everywhere.

/// LCG multiplier. W1 §12.1 (CONFIRMED).
pub const LCG_MUL: u32 = 214_013;
/// LCG increment. W1 §12.1 (CONFIRMED).
pub const LCG_ADD: u32 = 2_531_011;
/// Scale used by `unit_float` and `float_range`: 1/65535 as an `f32`. W1 §12.1, 0x005FC070 (CONFIRMED).
pub const INV_65535: f32 = 1.525_902_2e-5;
/// Scale used by the stateless hash: 1/16384 as an `f32` (range 0..4). W1 §12.1, 0x00535B30 (CONFIRMED).
pub const INV_16384: f32 = 6.103_609e-5;

/// The game's RNG: a single `u32` of state. W1 §8 "RNG object" and §12.1 (CONFIRMED).
///
/// `Clone` lets you copy an RNG (useful in tests); there is no hidden global state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaRng {
    /// The raw 32-bit LCG state. Public so that saves / desync logs can read and restore it.
    pub state: u32,
}

impl CaRng {
    /// Creates an RNG whose state is `seed`. (How the original seeds each owner is UNKNOWN, W1 §10.7;
    /// we simply store the seed as the state.)
    pub fn new(seed: u32) -> Self {
        CaRng { state: seed }
    }

    /// Advances the LCG once and returns the upper 16 bits (0..=65535, **not** masked to 0x7FFF).
    ///
    /// W1 §12.1 (CONFIRMED): `state = state*214013 + 2531011; return state >> 16`.
    pub fn next16(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
        self.state >> 16
    }

    /// Returns a value in `0..n` using the original's rejection sampling, which rejects **low** values.
    ///
    /// W1 §12.1, 0x005F0830 (CONFIRMED), exactly as compiled:
    /// ```text
    /// loop { r = next16(); if r > (0xFFFF % n) { return (r % n) & 0xFFFF } }
    /// ```
    /// i.e. any `r <= 0xFFFF % n` is thrown away and a new number is drawn.
    ///
    /// # Panics
    /// Panics if `n == 0` (the original would fault on the division) or if `n > 0xFFFF`
    /// (the original would loop forever, because `0xFFFF % n == 0xFFFF` rejects every `r`).
    pub fn uniform_below(&mut self, n: u32) -> u32 {
        assert!(n != 0, "uniform_below(0): division by zero in the original");
        assert!(
            n <= 0xFFFF,
            "uniform_below(n > 0xFFFF) never terminates in the original"
        );
        let reject_up_to = 0xFFFF % n;
        loop {
            let r = self.next16();
            if r > reject_up_to {
                return (r % n) & 0xFFFF;
            }
        }
    }

    /// Returns a value in `0..=100`. W1 §12.1, 0x007ADD80 (CONFIRMED):
    /// ```text
    /// loop { r = next16(); if r >= 88 { return r % 101 } }
    /// ```
    /// (65536 - 88 = 65448 = 101 * 648, so the result is exactly uniform.)
    pub fn percent_0_100(&mut self) -> u32 {
        loop {
            let r = self.next16();
            if r >= 88 {
                return r % 101;
            }
        }
    }

    /// Returns an integer in `lo..=hi`. W1 §12.1, 0x005F0880 (CONFIRMED), using `u32` math:
    /// ```text
    /// span = (hi - lo) as u32; r = ((span + 1) * next16()) / 0xFFFF
    /// return lo + min(r, span)
    /// ```
    /// Note that the multiplication can wrap for very large spans, exactly as in the original.
    pub fn int_range(&mut self, lo: i32, hi: i32) -> i32 {
        let span = hi.wrapping_sub(lo) as u32;
        let r = span.wrapping_add(1).wrapping_mul(self.next16()) / 0xFFFF;
        lo.wrapping_add(r.min(span) as i32)
    }

    /// Returns a float in `[0, 1]` (both ends inclusive). W1 §12.1, 0x005FC070 (CONFIRMED):
    /// `next16() as f32 * 1.5259022e-05`.
    pub fn unit_float(&mut self) -> f32 {
        self.next16() as f32 * INV_65535
    }

    /// Returns a float in `[a, b]`. W1 §12.1, 0x00A9DA10 / 0x00BBD6E0 (CONFIRMED):
    /// `next16() as f32 * 1.5259022e-05 * (b - a) + a`, evaluated left to right in `f32`.
    pub fn float_range(&mut self, a: f32, b: f32) -> f32 {
        self.next16() as f32 * INV_65535 * (b - a) + a
    }
}

/// Stateless hash used e.g. by the wind setup. W1 §12.1, 0x00535B30 (CONFIRMED):
/// `((seed*214013 + 2531011) >> 16) as f32 * 6.103609e-05`, a float in `[0, 4)`.
pub fn hash_float16384(seed: u32) -> f32 {
    (seed.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD) >> 16) as f32 * INV_16384
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Multiplicative inverse of an odd number modulo 2^32 (Newton's iteration).
    /// Used to build an RNG state that produces a chosen next output.
    fn inverse_mod_2_32(a: u32) -> u32 {
        let mut x = a; // correct to 3 bits for odd a
        for _ in 0..5 {
            x = x.wrapping_mul(2u32.wrapping_sub(a.wrapping_mul(x)));
        }
        x
    }

    /// Returns an RNG whose next `next16()` returns exactly `out`.
    fn rng_that_outputs(out: u32) -> CaRng {
        let wanted_next_state = out << 16;
        let prev = wanted_next_state
            .wrapping_sub(LCG_ADD)
            .wrapping_mul(inverse_mod_2_32(LCG_MUL));
        CaRng::new(prev)
    }

    #[test]
    fn helper_builds_wanted_output() {
        for out in [0, 1, 2, 87, 88, 65535] {
            assert_eq!(rng_that_outputs(out).next16(), out);
        }
    }

    #[test]
    fn seed_zero_sequence_by_hand() {
        // Hand computation (also checked with shell arithmetic):
        // s1 = 0*214013 + 2531011 = 2531011 = 0x00269EC3 -> 0x26 = 38
        // s2 = 2531011*214013 + 2531011 mod 2^32 = 505908858 -> 7719
        // s3 = 3539360597 -> 54006 (MSVC rand() would give 54006 & 0x7FFF = 21238)
        let mut rng = CaRng::new(0);
        let outs: Vec<u32> = (0..6).map(|_| rng.next16()).collect();
        assert_eq!(outs, vec![38, 7719, 54006, 2437, 41623, 11797]);
        assert_eq!(rng.state, 773_150_046);
    }

    #[test]
    fn seed_one_matches_msvc_rand_low_bits() {
        // MSVC srand(1) gives 41, 18467, 6334: our unmasked values must agree in the low 15 bits.
        let mut rng = CaRng::new(1);
        let outs: Vec<u32> = (0..3).map(|_| rng.next16()).collect();
        assert_eq!(outs, vec![41, 51235, 6334]);
        assert_eq!(51235 & 0x7FFF, 18467);
    }

    #[test]
    fn uniform_below_rejects_low_values() {
        // n = 7: 0xFFFF % 7 == 1, so outputs 0 and 1 are rejected; 2 is accepted.
        assert_eq!(0xFFFF % 7, 1);
        let mut rng = rng_that_outputs(1);
        let mut copy = rng;
        copy.next16(); // skip the rejected value
        let expected = copy.next16() % 7;
        assert_eq!(rng.uniform_below(7), expected);

        let mut rng = rng_that_outputs(0);
        let mut copy = rng;
        copy.next16();
        assert_eq!(rng.uniform_below(7), copy.next16() % 7);

        // The value exactly one above the rejection bound is accepted at once.
        let mut rng = rng_that_outputs(2);
        assert_eq!(rng.uniform_below(7), 2);
    }

    #[test]
    fn uniform_below_n1_rejects_only_zero() {
        // 0xFFFF % 1 == 0: r = 0 is rejected, any other r returns 0.
        let mut rng = rng_that_outputs(5);
        assert_eq!(rng.uniform_below(1), 0);
        let before = rng_that_outputs(0);
        let mut rng = before;
        assert_eq!(rng.uniform_below(1), 0);
        // It consumed exactly two numbers: the rejected 0, then an accepted one
        // (the next output after 0 is non-zero for this state).
        let mut two = before;
        two.next16();
        assert_ne!(two.next16(), 0);
        assert_eq!(rng.state, two.state);
    }

    #[test]
    fn uniform_below_max_n() {
        // n = 0xFFFF: 0xFFFF % 0xFFFF == 0, so only r = 0 is rejected; r = 65535 -> 0.
        let mut rng = rng_that_outputs(65535);
        assert_eq!(rng.uniform_below(0xFFFF), 0);
    }

    #[test]
    #[should_panic]
    fn uniform_below_zero_panics() {
        CaRng::new(0).uniform_below(0);
    }

    #[test]
    fn percent_rejects_below_88() {
        let mut rng = rng_that_outputs(88);
        assert_eq!(rng.percent_0_100(), 88);
        let mut rng = rng_that_outputs(87);
        let mut copy = rng;
        copy.next16();
        assert_eq!(rng.percent_0_100(), copy.next16() % 101);
        let mut rng = rng_that_outputs(65535);
        assert_eq!(rng.percent_0_100(), 65535 % 101);
    }

    #[test]
    fn int_range_edges() {
        // r = 0 -> lo
        assert_eq!(rng_that_outputs(0).int_range(0, 9), 0);
        // r = 65535 -> (10 * 65535) / 65535 = 10, clamped to span 9 -> hi
        assert_eq!(rng_that_outputs(65535).int_range(0, 9), 9);
        // negative lo
        assert_eq!(rng_that_outputs(65535).int_range(-5, 5), 5);
        assert_eq!(rng_that_outputs(0).int_range(-5, 5), -5);
        // middle: (11 * 32768) / 65535 = 5 -> -5 + 5 = 0
        assert_eq!(rng_that_outputs(32768).int_range(-5, 5), 0);
        // lo == hi always returns lo
        assert_eq!(CaRng::new(123).int_range(7, 7), 7);
    }

    #[test]
    fn floats() {
        assert_eq!(rng_that_outputs(0).unit_float(), 0.0);
        let one = rng_that_outputs(65535).unit_float();
        assert!((one - 1.0).abs() < 1e-6);
        let mid = rng_that_outputs(0).float_range(2.0, 4.0);
        assert_eq!(mid, 2.0);
        let top = rng_that_outputs(65535).float_range(2.0, 4.0);
        assert!((top - 4.0).abs() < 1e-5);
    }

    #[test]
    fn hash_is_stateless_and_in_range() {
        assert_eq!(hash_float16384(0), 38.0 * INV_16384);
        assert_eq!(hash_float16384(12345), hash_float16384(12345));
        // 12345 -> state 2644521496 -> 40352
        assert_eq!(hash_float16384(12345), 40352.0 * INV_16384);
        assert!(hash_float16384(0xFFFF_FFFF) < 4.0);
    }
}

//! Fixed-point numbers used for campaign-map positions.
//!
//! W3 §2.4 (CONFIRMED): logical campaign positions are stored as `i32` with **20 fractional bits**,
//! so `world = raw / 1048576` (2^20). Example: Paris is stored as `(-222517056, 2406541)`,
//! which is `(-212.2088, 2.29506)` in world units.

/// Number of fractional bits. W3 §2.4 (CONFIRMED).
pub const FRACTION_BITS: u32 = 20;
/// 2^20 = 1048576, the value of `1.0` in raw units.
pub const ONE_RAW: i32 = 1 << FRACTION_BITS;

/// A fixed-point number: `raw / 2^20`. W3 §2.4 (CONFIRMED).
///
/// The raw value is public so saves can round-trip it bit-exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed20(pub i32);

impl Fixed20 {
    /// Wraps a raw stored value.
    pub const fn from_raw(raw: i32) -> Self {
        Fixed20(raw)
    }

    /// Returns the raw stored value.
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Converts to `f64` exactly (every `i32 / 2^20` fits in an `f64`).
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / ONE_RAW as f64
    }

    /// Converts to `f32` (may lose precision for large values, like the original's float math).
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / ONE_RAW as f32
    }

    /// Converts a world value to fixed point, rounding to the nearest raw step and saturating at the
    /// `i32` limits. INFERRED: the original's float→fixed rounding rule is not known; nearest is our choice.
    pub fn from_f64(value: f64) -> Self {
        Fixed20((value * ONE_RAW as f64).round() as i32)
    }

    /// Converts a whole number of world units to fixed point (wrapping like 32-bit C++ would).
    pub const fn from_int(value: i32) -> Self {
        Fixed20(value.wrapping_shl(FRACTION_BITS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paris_coordinates() {
        // W3 §2.4: Paris (-222517056, 2406541) -> (-212.2088, 2.29506)
        let x = Fixed20::from_raw(-222_517_056).to_f64();
        let y = Fixed20::from_raw(2_406_541).to_f64();
        assert!((x - -212.2088).abs() < 1e-4, "x = {x}");
        assert!((y - 2.29506).abs() < 1e-5, "y = {y}");
    }

    #[test]
    fn pathfinding_origin() {
        // W3 §2.4: (-429916160, -199229440) is exactly (-410, -190).
        assert_eq!(Fixed20::from_raw(-429_916_160).to_f64(), -410.0);
        assert_eq!(Fixed20::from_raw(-199_229_440).to_f64(), -190.0);
        assert_eq!(Fixed20::from_int(-410).raw(), -429_916_160);
    }

    #[test]
    fn round_trip() {
        let f = Fixed20::from_raw(-222_517_056);
        assert_eq!(Fixed20::from_f64(f.to_f64()), f);
        assert_eq!(Fixed20::from_f64(1.0).raw(), ONE_RAW);
    }
}

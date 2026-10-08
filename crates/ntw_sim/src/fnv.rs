//! A tiny, stable 64-bit FNV-1a hasher used for desync checks (`state_hash`) in the battle and
//! campaign models. This is our own tool, not something from the original game.
//!
//! std's `DefaultHasher` is not guaranteed to give the same result across Rust versions, so we
//! use this instead.

/// Minimal FNV-1a 64-bit hasher.
pub(crate) struct Fnv64(u64);

impl Fnv64 {
    /// Starts a new hash with the standard FNV-1a offset basis.
    pub(crate) fn new() -> Self {
        Fnv64(0xcbf2_9ce4_8422_2325)
    }

    /// Feeds raw bytes.
    pub(crate) fn bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Feeds a `u32` as 4 little-endian bytes.
    pub(crate) fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    /// Feeds an `i32` (its bit pattern).
    pub(crate) fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }

    /// Feeds a string as its byte length followed by its UTF-8 bytes (the length prefix keeps
    /// `"ab" + "c"` and `"a" + "bc"` from hashing the same).
    pub(crate) fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.bytes(s.as_bytes());
    }

    /// Returns the hash value.
    pub(crate) fn finish(&self) -> u64 {
        self.0
    }
}

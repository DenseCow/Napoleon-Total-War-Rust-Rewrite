//! The Bink bit reader: bits are taken least-significant first from little-endian 32-bit words
//! (equivalently, LSB-first from each byte in order). CONFIRMED by the format notes and by
//! every shipped plane ending exactly at its stored size (`BINK.md` §3).

/// Reads bits LSB-first. Reading past the end yields zero bits and sets [`BitReader::overrun`],
/// so decoders never panic on bad data; callers check `overrun` at plane ends.
#[derive(Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Next byte to load into `cache`.
    byte: usize,
    cache: u64,
    /// Valid bits in `cache`.
    avail: u32,
    /// Bits consumed so far.
    pos: usize,
    overrun: bool,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, byte: 0, cache: 0, avail: 0, pos: 0, overrun: false }
    }

    #[inline]
    fn refill(&mut self) {
        // Load whole bytes while there is room for 8 more bits.
        if self.byte + 8 <= self.data.len() && self.avail <= 32 {
            let w = u32::from_le_bytes(self.data[self.byte..self.byte + 4].try_into().unwrap());
            self.cache |= (w as u64) << self.avail;
            self.avail += 32;
            self.byte += 4;
            return;
        }
        while self.avail <= 56 {
            let b = match self.data.get(self.byte) {
                Some(&b) => b,
                None => break,
            };
            self.cache |= (b as u64) << self.avail;
            self.avail += 8;
            self.byte += 1;
        }
    }

    /// Reads `n` bits (0..=32) as an unsigned value.
    #[inline]
    pub fn read(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        if self.avail < n {
            self.refill();
            if self.avail < n {
                // Past the end: pad with zeros.
                self.overrun = true;
                let v = self.cache as u32 & mask(self.avail);
                self.pos += n as usize;
                self.cache = 0;
                self.avail = 0;
                return v;
            }
        }
        let v = (self.cache as u32) & mask(n);
        self.cache >>= n;
        self.avail -= n;
        self.pos += n as usize;
        v
    }

    #[inline]
    pub fn bit(&mut self) -> bool {
        self.read(1) != 0
    }

    /// Looks at the next `n` bits (n <= 32) without consuming them.
    #[inline]
    pub fn peek(&mut self, n: u32) -> u32 {
        if self.avail < n {
            self.refill();
        }
        (self.cache as u32) & mask(n.min(32))
    }

    /// Consumes `n` bits after a [`peek`](Self::peek).
    #[inline]
    pub fn skip(&mut self, n: u32) {
        let mut n = n;
        while n > 32 {
            self.read(32);
            n -= 32;
        }
        self.read(n);
    }

    /// Bits consumed so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Total bits in the data.
    pub fn len_bits(&self) -> usize {
        self.data.len() * 8
    }

    /// Bits left (0 when past the end).
    pub fn remaining(&self) -> usize {
        self.len_bits().saturating_sub(self.pos)
    }

    /// True once a read went past the end of the data.
    pub fn overrun(&self) -> bool {
        self.overrun || self.pos > self.len_bits()
    }

    /// Skips to the next multiple of 32 bits.
    pub fn align32(&mut self) {
        let r = self.pos & 31;
        if r != 0 {
            self.skip(32 - r as u32);
        }
    }

    /// Moves to an absolute bit position (forwards or backwards).
    pub fn seek(&mut self, bit: usize) {
        let byte = (bit / 8).min(self.data.len());
        self.byte = byte;
        self.cache = 0;
        self.avail = 0;
        self.pos = byte * 8;
        self.overrun = bit > self.len_bits();
        let r = (bit - byte * 8) as u32;
        if r > 0 && !self.overrun {
            self.read(r);
        }
        self.pos = bit;
    }
}

#[inline]
fn mask(n: u32) -> u32 {
    if n >= 32 { u32::MAX } else { (1u32 << n) - 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lsb_first() {
        let d = [0b1010_0110u8, 0xFF, 0x01, 0x80, 0x12, 0x34, 0x56, 0x78, 0x9A];
        let mut r = BitReader::new(&d);
        assert_eq!(r.read(1), 0);
        assert_eq!(r.read(2), 0b11);
        assert_eq!(r.read(5), 0b10100);
        assert_eq!(r.read(16), 0x01FF);
        assert_eq!(r.position(), 24);
        assert_eq!(r.read(32), 0x5634_1280);
        assert_eq!(r.peek(4), 0x8);
        r.align32();
        assert_eq!(r.position(), 64);
        assert_eq!(r.read(8), 0x9A);
        assert!(!r.overrun());
        r.read(3);
        assert!(r.overrun());
        r.seek(12);
        assert_eq!(r.read(8), 0x1F);
    }
}

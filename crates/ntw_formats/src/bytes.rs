//! A small bounds-checked little-endian cursor shared by the `db` and `loc` readers.

/// A low-level read failure. Each format turns this into its own error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadError {
    /// Not enough bytes left.
    Eof { offset: usize, needed: usize },
    /// A UTF-16 string contained an unpaired surrogate.
    Utf16 { offset: usize },
}

/// Reads values from a byte slice, failing (never panicking) at the end of the data.
pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        if n > self.remaining() {
            return Err(ReadError::Eof { offset: self.pos, needed: n });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Looks at the next `n` bytes without consuming them.
    pub(crate) fn peek(&self, n: usize) -> Option<&'a [u8]> {
        self.data.get(self.pos..self.pos.checked_add(n)?)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, ReadError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, ReadError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, ReadError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, ReadError> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, ReadError> {
        Ok(self.u32()? as i32)
    }

    /// u16 byte count, then that many 8-bit characters (decoded as Latin-1, so it never fails).
    pub(crate) fn ascii(&mut self) -> Result<String, ReadError> {
        let count = usize::from(self.u16()?);
        Ok(self.take(count)?.iter().map(|&b| char::from(b)).collect())
    }

    /// u16 count of UTF-16 code units, then that many UTF-16LE units.
    pub(crate) fn utf16(&mut self) -> Result<String, ReadError> {
        let start = self.pos;
        let count = usize::from(self.u16()?);
        let bytes = self.take(count * 2)?;
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16(&units).map_err(|_| ReadError::Utf16 { offset: start })
    }
}

/// Encodes a string the way `Cursor::utf16` reads it. Test helper.
#[cfg(test)]
pub(crate) fn utf16_bytes(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut b = (units.len() as u16).to_le_bytes().to_vec();
    for u in units {
        b.extend_from_slice(&u.to_le_bytes());
    }
    b
}

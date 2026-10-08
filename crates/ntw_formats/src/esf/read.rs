//! Turns ESF bytes into an [`EsfFile`] tree.
//!
//! The reader works on a byte slice already in memory. The biggest shipped ESF,
//! a 1.3 save, is about 37 MB.
//! Every read is bounds-checked against the end of the *enclosing block*, so a
//! corrupt child can never read into its parent's sibling. Every absolute end
//! offset is checked to lie between the current position and the parent's end.

use super::node::codes::*;
use super::{EsfError, EsfFile, EsfHeader, EsfNode, EsfRecord, EsfRecordArray, MAGIC, MAX_DEPTH};

type Result<T> = std::result::Result<T, EsfError>;

/// Byte size of one element of a fixed-size primitive, or `None` for strings.
pub(crate) fn element_size(primitive: u8) -> Option<usize> {
    Some(match primitive {
        BOOL | I8 | U8 => 1,
        I16 | U16 | ANGLE => 2,
        I32 | U32 | F32 => 4,
        I64 | U64 | F64 | COORD2D => 8,
        COORD3D => 12,
        _ => return None,
    })
}

/// A read cursor over the whole file plus the already-parsed name table.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    names: &'a [String],
}

impl<'a> Reader<'a> {
    /// Takes `n` bytes, failing if that would cross `limit` (the enclosing block's end).
    fn take(&mut self, n: usize, limit: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= limit);
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => Err(EsfError::UnexpectedEof { offset: self.pos, needed: n }),
        }
    }

    fn array<const N: usize>(&mut self, limit: usize) -> Result<[u8; N]> {
        let s = self.take(N, limit)?;
        // `take` returned exactly N bytes, so this conversion cannot fail.
        let mut out = [0u8; N];
        out.copy_from_slice(s);
        Ok(out)
    }

    fn u8(&mut self, limit: usize) -> Result<u8> {
        Ok(self.array::<1>(limit)?[0])
    }
    fn u16(&mut self, limit: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array(limit)?))
    }
    fn u32(&mut self, limit: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array(limit)?))
    }
    fn u64(&mut self, limit: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array(limit)?))
    }
    fn f32(&mut self, limit: usize) -> Result<f32> {
        Ok(f32::from_bits(self.u32(limit)?))
    }

    /// u16 character count, then UTF-16LE units.
    fn utf16(&mut self, limit: usize) -> Result<String> {
        let start = self.pos;
        let count = usize::from(self.u16(limit)?);
        let bytes = self.take(count * 2, limit)?;
        let units: Vec<u16> =
            bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        String::from_utf16(&units).map_err(|_| EsfError::InvalidUtf16 { offset: start })
    }

    /// u16 byte count, then single-byte (Latin-1) characters.
    fn ascii(&mut self, limit: usize) -> Result<String> {
        let count = usize::from(self.u16(limit)?);
        Ok(self.take(count, limit)?.iter().map(|&b| char::from(b)).collect())
    }

    /// Reads a u32 absolute end offset and checks `self.pos <= end <= limit`
    /// (the check runs after the offset itself has been read).
    fn end_offset(&mut self, node_start: usize, limit: usize) -> Result<usize> {
        let end = self.u32(limit)?;
        let e = end as usize;
        if e < self.pos || e > limit {
            return Err(EsfError::BadEndOffset { offset: node_start, end, min: self.pos, max: limit });
        }
        Ok(e)
    }

    fn name(&self, index: u16, node_start: usize) -> Result<String> {
        self.names.get(usize::from(index)).cloned().ok_or(EsfError::NameIndexOutOfRange {
            offset: node_start,
            index,
            name_count: self.names.len(),
        })
    }

    /// Reads one scalar (non-array, non-record) value of type `code`.
    fn scalar(&mut self, code: u8, start: usize, limit: usize) -> Result<EsfNode> {
        Ok(match code {
            BOOL => EsfNode::Bool(self.u8(limit)? != 0),
            I8 => EsfNode::I8(self.u8(limit)? as i8),
            I16 => EsfNode::I16(self.u16(limit)? as i16),
            I32 => EsfNode::I32(self.u32(limit)? as i32),
            I64 => EsfNode::I64(self.u64(limit)? as i64),
            U8 => EsfNode::U8(self.u8(limit)?),
            U16 => EsfNode::U16(self.u16(limit)?),
            U32 => EsfNode::U32(self.u32(limit)?),
            U64 => EsfNode::U64(self.u64(limit)?),
            F32 => EsfNode::F32(self.f32(limit)?),
            F64 => EsfNode::F64(f64::from_bits(self.u64(limit)?)),
            COORD2D => EsfNode::Coord2d(self.f32(limit)?, self.f32(limit)?),
            COORD3D => EsfNode::Coord3d(self.f32(limit)?, self.f32(limit)?, self.f32(limit)?),
            UTF16 => EsfNode::Utf16String(self.utf16(limit)?),
            ASCII => EsfNode::AsciiString(self.ascii(limit)?),
            ANGLE => EsfNode::Angle(self.u16(limit)?),
            _ => return Err(EsfError::UnknownTypeCode { code, offset: start }),
        })
    }

    /// Reads a packed array (type `0x40 | primitive`): u32 end offset, then elements up to it.
    fn packed_array(&mut self, code: u8, start: usize, limit: usize) -> Result<EsfNode> {
        let primitive = code & !ARRAY_FLAG;
        let end = self.end_offset(start, limit)?;
        if primitive == UTF16 || primitive == ASCII {
            let mut strings = Vec::new();
            while self.pos < end {
                strings.push(if primitive == UTF16 { self.utf16(end)? } else { self.ascii(end)? });
            }
            return Ok(if primitive == UTF16 {
                EsfNode::Utf16Array(strings)
            } else {
                EsfNode::AsciiArray(strings)
            });
        }
        let size = element_size(primitive)
            .ok_or(EsfError::UnknownTypeCode { code, offset: start })?;
        let bytes = self.take(end - self.pos, end)?;
        if bytes.len() % size != 0 {
            return Err(EsfError::BadArrayLength { offset: start, code, len: bytes.len() });
        }
        // Decodes fixed-size little-endian chunks. The length check above means no bytes are left over.
        fn le<const N: usize, T>(bytes: &[u8], f: impl Fn([u8; N]) -> T) -> Vec<T> {
            bytes.as_chunks::<N>().0.iter().map(|c| f(*c)).collect()
        }
        let f32_at = |c: &[u8], i: usize| f32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
        Ok(match primitive {
            BOOL => EsfNode::BoolArray(bytes.iter().map(|&b| b != 0).collect()),
            I8 => EsfNode::I8Array(bytes.iter().map(|&b| b as i8).collect()),
            U8 => EsfNode::U8Array(bytes.to_vec()),
            I16 => EsfNode::I16Array(le(bytes, i16::from_le_bytes)),
            U16 => EsfNode::U16Array(le(bytes, u16::from_le_bytes)),
            ANGLE => EsfNode::AngleArray(le(bytes, u16::from_le_bytes)),
            I32 => EsfNode::I32Array(le(bytes, i32::from_le_bytes)),
            U32 => EsfNode::U32Array(le(bytes, u32::from_le_bytes)),
            F32 => EsfNode::F32Array(le(bytes, f32::from_le_bytes)),
            I64 => EsfNode::I64Array(le(bytes, i64::from_le_bytes)),
            U64 => EsfNode::U64Array(le(bytes, u64::from_le_bytes)),
            F64 => EsfNode::F64Array(le(bytes, f64::from_le_bytes)),
            COORD2D => EsfNode::Coord2dArray(le(bytes, |c: [u8; 8]| (f32_at(&c, 0), f32_at(&c, 4)))),
            COORD3D => EsfNode::Coord3dArray(le(bytes, |c: [u8; 12]| {
                (f32_at(&c, 0), f32_at(&c, 4), f32_at(&c, 8))
            })),
            _ => return Err(EsfError::UnknownTypeCode { code, offset: start }),
        })
    }

    /// Reads children until exactly `end`.
    fn children(&mut self, end: usize, depth: usize) -> Result<Vec<EsfNode>> {
        let mut out = Vec::new();
        while self.pos < end {
            out.push(self.node(end, depth)?);
        }
        Ok(out)
    }

    /// Record body (after the 0x80 byte): u16 name, u8 version, u32 end, children.
    fn record(&mut self, start: usize, limit: usize, depth: usize) -> Result<EsfRecord> {
        if depth > MAX_DEPTH {
            return Err(EsfError::TooDeep { offset: start });
        }
        let name_index = self.u16(limit)?;
        let name = self.name(name_index, start)?;
        let version = self.u8(limit)?;
        let end = self.end_offset(start, limit)?;
        let children = self.children(end, depth + 1)?;
        Ok(EsfRecord { name, version, children })
    }

    /// Record-array body (after the 0x81 byte):
    /// u16 name, u8 version, u32 end, u32 count, then count x { u32 item end, children }.
    fn record_array(&mut self, start: usize, limit: usize, depth: usize) -> Result<EsfRecordArray> {
        if depth > MAX_DEPTH {
            return Err(EsfError::TooDeep { offset: start });
        }
        let name_index = self.u16(limit)?;
        let name = self.name(name_index, start)?;
        let version = self.u8(limit)?;
        let end = self.end_offset(start, limit)?;
        let count = self.u32(end)? as usize;
        // Every item needs at least 4 bytes, so cap the up-front allocation by what can fit.
        let mut items = Vec::with_capacity(count.min((end - self.pos) / 4));
        for _ in 0..count {
            let item_start = self.pos;
            let item_end = self.end_offset(item_start, end)?;
            items.push(self.children(item_end, depth + 1)?);
        }
        if self.pos != end {
            return Err(EsfError::BlockOverrun { offset: start, end, reached: self.pos });
        }
        Ok(EsfRecordArray { name, version, items })
    }

    /// Reads one node of any type. `limit` is the end of the enclosing block.
    fn node(&mut self, limit: usize, depth: usize) -> Result<EsfNode> {
        let start = self.pos;
        let code = self.u8(limit)?;
        match code {
            RECORD => Ok(EsfNode::Record(Box::new(self.record(start, limit, depth)?))),
            RECORD_ARRAY => {
                Ok(EsfNode::RecordArray(Box::new(self.record_array(start, limit, depth)?)))
            }
            c if c & 0xC0 == ARRAY_FLAG => self.packed_array(c, start, limit),
            c => self.scalar(c, start, limit),
        }
    }
}

/// Reads the name table at `offset`: u16 count, then count x { u16 len, bytes }.
fn read_names(data: &[u8], offset: usize) -> Result<Vec<String>> {
    let mut r = Reader { data, pos: offset, names: &[] };
    let count = r.u16(data.len())?;
    let mut names = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        names.push(r.ascii(data.len())?);
    }
    if r.pos != data.len() {
        return Err(EsfError::TrailingBytes { count: data.len() - r.pos });
    }
    Ok(names)
}

/// Parses a complete ESF file from bytes. See [`EsfFile::from_bytes`].
pub(crate) fn read_file(data: &[u8]) -> Result<EsfFile> {
    let mut hr = Reader { data, pos: 0, names: &[] };
    let magic = hr.u32(data.len())?;
    if magic != MAGIC {
        return Err(EsfError::BadMagic(magic));
    }
    let unknown_04 = hr.u32(data.len())?;
    let timestamp = hr.u32(data.len())?;
    let names_offset = hr.u32(data.len())?;
    let names_at = names_offset as usize;
    if names_at < 16 || names_at > data.len() {
        return Err(EsfError::BadNamesOffset { names_offset, file_len: data.len() });
    }
    let names = read_names(data, names_at)?;

    let mut r = Reader { data, pos: 16, names: &names };
    let code = r.u8(names_at)?;
    if code != RECORD {
        return Err(EsfError::RootNotRecord { type_code: code });
    }
    let root = r.record(16, names_at, 0)?;
    if r.pos != names_at {
        return Err(EsfError::RootEndMismatch { root_end: r.pos, names_offset: names_at });
    }
    let header = EsfHeader { magic, unknown_04, timestamp, names_offset };
    Ok(EsfFile { header, names, root })
}

//! Turns an [`EsfFile`] tree back into bytes.
//!
//! ESF stores *absolute* end offsets in front of every record, record-array item
//! and packed array. The writer cannot know an end offset until it has written
//! the children. So it writes a 4-byte placeholder, writes the children, then
//! goes back and fills in ("back-patches") the real offset.
//!
//! Writing to memory is the only option, so an install can never be modified by accident.

use std::collections::HashMap;

use super::{EsfError, EsfFile, EsfNode, EsfRecord, EsfRecordArray};

type Result<T> = std::result::Result<T, EsfError>;

/// Serializes ESF trees into bytes.
///
/// The name table keeps the order of [`EsfFile::names`], so `write(read(bytes)) == bytes`.
/// A record whose name is not in that table yet gets it appended to the end.
///
/// ```
/// use ntw_formats::esf::{EsfFile, EsfRecord, EsfNode, EsfWriter};
/// let mut root = EsfRecord::new("ROOT", 1);
/// root.children.push(EsfNode::U32(7));
/// let file = EsfFile::new(root);
/// let bytes = EsfWriter::write(&file).unwrap();
/// assert_eq!(EsfFile::from_bytes(&bytes).unwrap().root, file.root);
/// ```
#[derive(Debug, Default)]
pub struct EsfWriter {
    buf: Vec<u8>,
    names: Vec<String>,
    name_index: HashMap<String, u16>,
}

impl EsfWriter {
    /// Writes a whole file (header, root record, name table) and returns its bytes.
    ///
    /// The header's `names_offset` is recomputed. `magic`, `unknown_04` and
    /// `timestamp` are written as stored in [`EsfFile::header`].
    pub fn write(file: &EsfFile) -> Result<Vec<u8>> {
        let mut w = EsfWriter::default();
        for name in &file.names {
            w.intern_seed(name)?;
        }
        w.put_u32(file.header.magic);
        w.put_u32(file.header.unknown_04);
        w.put_u32(file.header.timestamp);
        let names_slot = w.placeholder();
        w.record(&file.root)?;
        w.patch(names_slot)?;

        let count = u16::try_from(w.names.len()).map_err(|_| EsfError::TooManyNames)?;
        let names = std::mem::take(&mut w.names);
        w.put_u16(count);
        for name in &names {
            w.ascii(name)?;
        }
        Ok(w.buf)
    }

    /// Adds a name from the existing table, keeping its position (the first copy wins on duplicates).
    fn intern_seed(&mut self, name: &str) -> Result<()> {
        let index = u16::try_from(self.names.len()).map_err(|_| EsfError::TooManyNames)?;
        self.names.push(name.to_owned());
        self.name_index.entry(name.to_owned()).or_insert(index);
        Ok(())
    }

    /// The table index of `name`, appending it if new.
    fn intern(&mut self, name: &str) -> Result<u16> {
        if let Some(&i) = self.name_index.get(name) {
            return Ok(i);
        }
        let index = u16::try_from(self.names.len()).map_err(|_| EsfError::TooManyNames)?;
        self.names.push(name.to_owned());
        self.name_index.insert(name.to_owned(), index);
        Ok(index)
    }

    fn put_u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn put_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn put_f32(&mut self, v: f32) {
        self.put_u32(v.to_bits());
    }

    /// Reserves 4 bytes for an offset that is filled in later; returns where they are.
    fn placeholder(&mut self) -> usize {
        let at = self.buf.len();
        self.put_u32(0);
        at
    }

    /// Fills the placeholder at `at` with the current output length (an absolute offset).
    fn patch(&mut self, at: usize) -> Result<()> {
        let here = u32::try_from(self.buf.len()).map_err(|_| EsfError::OutputTooLarge)?;
        self.buf[at..at + 4].copy_from_slice(&here.to_le_bytes());
        Ok(())
    }

    fn utf16(&mut self, s: &str) -> Result<()> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let len = u16::try_from(units.len()).map_err(|_| EsfError::StringTooLong { len: units.len() })?;
        self.put_u16(len);
        for u in units {
            self.put_u16(u);
        }
        Ok(())
    }

    /// Single-byte string: every char must be U+0000..=U+00FF (Latin-1), matching the reader.
    fn ascii(&mut self, s: &str) -> Result<()> {
        let bytes = s
            .chars()
            .map(|ch| u8::try_from(u32::from(ch)).map_err(|_| EsfError::NotSingleByte { ch }))
            .collect::<Result<Vec<u8>>>()?;
        let len = u16::try_from(bytes.len()).map_err(|_| EsfError::StringTooLong { len: bytes.len() })?;
        self.put_u16(len);
        self.buf.extend_from_slice(&bytes);
        Ok(())
    }

    fn record(&mut self, r: &EsfRecord) -> Result<()> {
        let index = self.intern(&r.name)?;
        self.put_u8(super::node::codes::RECORD);
        self.put_u16(index);
        self.put_u8(r.version);
        let end = self.placeholder();
        for child in &r.children {
            self.node(child)?;
        }
        self.patch(end)
    }

    fn record_array(&mut self, a: &EsfRecordArray) -> Result<()> {
        let index = self.intern(&a.name)?;
        self.put_u8(super::node::codes::RECORD_ARRAY);
        self.put_u16(index);
        self.put_u8(a.version);
        let end = self.placeholder();
        let count = u32::try_from(a.items.len()).map_err(|_| EsfError::TooManyItems)?;
        self.put_u32(count);
        for item in &a.items {
            let item_end = self.placeholder();
            for child in item {
                self.node(child)?;
            }
            self.patch(item_end)?;
        }
        self.patch(end)
    }

    /// Writes a packed array body: end-offset placeholder, elements, patch.
    /// (The type byte has already been written by [`node`](Self::node).)
    fn packed<T>(&mut self, items: &[T], mut put: impl FnMut(&mut Self, &T)) -> Result<()> {
        let end = self.placeholder();
        for item in items {
            put(self, item);
        }
        self.patch(end)
    }

    /// Like [`packed`](Self::packed) for string elements, whose encoding can fail.
    fn packed_strings(&mut self, items: &[String], utf16: bool) -> Result<()> {
        let end = self.placeholder();
        for s in items {
            if utf16 { self.utf16(s)? } else { self.ascii(s)? }
        }
        self.patch(end)
    }

    fn node(&mut self, n: &EsfNode) -> Result<()> {
        // Records write their own type byte; every other node starts with it.
        match n {
            EsfNode::Record(r) => return self.record(r),
            EsfNode::RecordArray(a) => return self.record_array(a),
            _ => self.put_u8(n.type_code()),
        }
        match n {
            // Scalars.
            EsfNode::Bool(v) => self.put_u8(u8::from(*v)),
            EsfNode::I8(v) => self.put_u8(*v as u8),
            EsfNode::U8(v) => self.put_u8(*v),
            EsfNode::I16(v) => self.put_u16(*v as u16),
            EsfNode::U16(v) | EsfNode::Angle(v) => self.put_u16(*v),
            EsfNode::I32(v) => self.put_u32(*v as u32),
            EsfNode::U32(v) => self.put_u32(*v),
            EsfNode::F32(v) => self.put_f32(*v),
            EsfNode::I64(v) => self.put_u64(*v as u64),
            EsfNode::U64(v) => self.put_u64(*v),
            EsfNode::F64(v) => self.put_u64(v.to_bits()),
            EsfNode::Coord2d(x, y) => {
                self.put_f32(*x);
                self.put_f32(*y);
            }
            EsfNode::Coord3d(x, y, z) => {
                self.put_f32(*x);
                self.put_f32(*y);
                self.put_f32(*z);
            }
            EsfNode::Utf16String(s) => self.utf16(s)?,
            EsfNode::AsciiString(s) => self.ascii(s)?,

            // Packed arrays.
            EsfNode::BoolArray(v) => self.packed(v, |w, x| w.put_u8(u8::from(*x)))?,
            EsfNode::I8Array(v) => self.packed(v, |w, x| w.put_u8(*x as u8))?,
            EsfNode::U8Array(v) => self.packed(v, |w, x| w.put_u8(*x))?,
            EsfNode::I16Array(v) => self.packed(v, |w, x| w.put_u16(*x as u16))?,
            EsfNode::U16Array(v) | EsfNode::AngleArray(v) => self.packed(v, |w, x| w.put_u16(*x))?,
            EsfNode::I32Array(v) => self.packed(v, |w, x| w.put_u32(*x as u32))?,
            EsfNode::U32Array(v) => self.packed(v, |w, x| w.put_u32(*x))?,
            EsfNode::F32Array(v) => self.packed(v, |w, x| w.put_f32(*x))?,
            EsfNode::I64Array(v) => self.packed(v, |w, x| w.put_u64(*x as u64))?,
            EsfNode::U64Array(v) => self.packed(v, |w, x| w.put_u64(*x))?,
            EsfNode::F64Array(v) => self.packed(v, |w, x| w.put_u64(x.to_bits()))?,
            EsfNode::Coord2dArray(v) => self.packed(v, |w, (x, y)| {
                w.put_f32(*x);
                w.put_f32(*y);
            })?,
            EsfNode::Coord3dArray(v) => self.packed(v, |w, (x, y, z)| {
                w.put_f32(*x);
                w.put_f32(*y);
                w.put_f32(*z);
            })?,
            EsfNode::Utf16Array(v) => self.packed_strings(v, true)?,
            EsfNode::AsciiArray(v) => self.packed_strings(v, false)?,

            // Already written by the first match.
            EsfNode::Record(_) | EsfNode::RecordArray(_) => {}
        }
        Ok(())
    }
}

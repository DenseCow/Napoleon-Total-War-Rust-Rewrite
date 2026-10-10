//! Binary DB tables: the game's spreadsheets (units, buildings, factions, rules, ...).
//!
//! # Where they live
//! Each table is one file in `data.pack`, at `db\<name>_tables\<name>`, for example
//! `db\units_tables\units`. Read it through a [`crate::pack::Vfs`].
//!
//! # Layout (CONFIRMED from the exe's generic loader `0x00E730D0`, Worker 1 `DB_BUILDERS.md` §1)
//! ```text
//! [FC FD FE FF  u32 version]   optional; if the first 4 bytes are not the marker, version = 0
//! u8   flag                    kept; meaning UNKNOWN (1 in every shipped table)
//! u32  row_count
//! rows                         no per-row framing, no column names, no GUID
//! ```
//!
//! # Schemas
//! The file does **not** say what its columns are. You must supply a [`Schema`]:
//! the list of field types in order, as recovered from the exe. Some fields exist
//! only from a certain table version on (`min_version`); in older tables they are
//! absent and get a default value instead.
//!
//! **Pitfall:** an empty string is `00 00`, which also looks like two `false` bools,
//! and an absent optional string is `00`, which looks like one `false`. So the reader
//! always follows the schema and never guesses. A wrong schema shows up as an error
//! (usually leftover bytes or a short read).
//!
//! This crate defines **no real table schemas**. Those belong in `ntw_data`.
//!
//! ```
//! use ntw_formats::db::{DbTable, FieldType, Schema};
//! // version-0 table, flag 1, one row: "abc", 7
//! let bytes = [1, 1,0,0,0, 3,0, b'a',0, b'b',0, b'c',0, 7,0,0,0];
//! let schema = Schema::new().field(FieldType::Str).field(FieldType::I32);
//! let table = DbTable::read(&bytes, &schema).unwrap();
//! assert_eq!(table.rows[0][0].as_str(), Some("abc"));
//! assert_eq!(table.rows[0][1].as_i32(), Some(7));
//! ```

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// The 4 bytes that announce a version number: `FC FD FE FF` (the u32 `0xFFFEFDFC`).
pub const VERSION_MARKER: [u8; 4] = [0xFC, 0xFD, 0xFE, 0xFF];

/// The wire type of one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldType {
    /// u16 count of UTF-16 units, then UTF-16LE text.
    Str,
    /// u8 flag; if non-zero, a `Str` follows.
    OptStr,
    /// One byte: 0 = false, anything else = true.
    Bool,
    /// 4 bytes read as a signed integer.
    I32,
    /// 4 bytes read as an IEEE-754 float. On disk it looks just like `I32`; only the schema tells them apart.
    F32,
    /// 2 bytes, unsigned.
    U16,
}

/// What an absent field (table version below `min_version`) becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IfAbsent {
    /// The type's default: `""`, `None`, `false`, `0`, `0.0`.
    #[default]
    Default,
    /// A copy of the previous field's value in the same row. The exe does this for
    /// `units` column 5 in version 0. The first field falls back to the default.
    CopyPrevious,
}

/// One column of a [`Schema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldDef {
    /// The wire type.
    pub ty: FieldType,
    /// The field is present only when the table version is at least this. Use 0 for "always".
    pub min_version: u32,
    /// What the field becomes when it is absent.
    pub if_absent: IfAbsent,
}

/// The ordered list of columns of one table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Schema {
    /// The columns in file order.
    pub fields: Vec<FieldDef>,
}

impl Schema {
    /// An empty schema; add columns with [`field`](Self::field) and friends.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a column that is always present.
    pub fn field(self, ty: FieldType) -> Self {
        self.field_since(ty, 0)
    }

    /// Adds a column present only when `version >= min_version` (default value otherwise).
    pub fn field_since(mut self, ty: FieldType, min_version: u32) -> Self {
        self.fields.push(FieldDef { ty, min_version, if_absent: IfAbsent::Default });
        self
    }

    /// Adds a column present only when `version >= min_version`. Otherwise it copies the previous column.
    pub fn field_since_or_copy(mut self, ty: FieldType, min_version: u32) -> Self {
        self.fields.push(FieldDef { ty, min_version, if_absent: IfAbsent::CopyPrevious });
        self
    }

    /// Builds an unversioned schema from one-letter codes (spaces and commas are ignored):
    /// `s` Str, `o` OptStr, `b` Bool, `i` I32, `f` F32, `h` U16.
    /// Returns `None` on any other character.
    ///
    /// ```
    /// # use ntw_formats::db::Schema;
    /// assert_eq!(Schema::from_codes("s,i,b").unwrap().fields.len(), 3);
    /// ```
    pub fn from_codes(codes: &str) -> Option<Self> {
        let mut s = Self::new();
        for c in codes.chars().filter(|c| !matches!(c, ' ' | ',')) {
            s = s.field(match c {
                's' => FieldType::Str,
                'o' => FieldType::OptStr,
                'b' => FieldType::Bool,
                'i' => FieldType::I32,
                'f' => FieldType::F32,
                'h' => FieldType::U16,
                _ => return None,
            });
        }
        Some(s)
    }
}

/// One cell of a table.
#[derive(Debug, Clone, PartialEq)]
pub enum DbValue {
    /// A string.
    Str(String),
    /// An optional string (`None` = absent flag).
    OptStr(Option<String>),
    /// A bool.
    Bool(bool),
    /// A 4-byte integer.
    I32(i32),
    /// A 4-byte float.
    F32(f32),
    /// A 2-byte unsigned integer.
    U16(u16),
}

impl DbValue {
    fn default_for(ty: FieldType) -> Self {
        match ty {
            FieldType::Str => Self::Str(String::new()),
            FieldType::OptStr => Self::OptStr(None),
            FieldType::Bool => Self::Bool(false),
            FieldType::I32 => Self::I32(0),
            FieldType::F32 => Self::F32(0.0),
            FieldType::U16 => Self::U16(0),
        }
    }

    /// The text of a `Str`, or of an `OptStr` that is present.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) | Self::OptStr(Some(s)) => Some(s),
            _ => None,
        }
    }

    /// The text of a `Str` or `OptStr`, with an absent `OptStr` read as `""` (what the exe does).
    pub fn as_str_or_empty(&self) -> Option<&str> {
        match self {
            Self::OptStr(None) => Some(""),
            other => other.as_str(),
        }
    }

    /// The value of a `Bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The value of an `I32`.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Self::I32(v) => Some(*v),
            _ => None,
        }
    }

    /// The value of an `F32`.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// The value of a `U16`.
    pub fn as_u16(&self) -> Option<u16> {
        match self {
            Self::U16(v) => Some(*v),
            _ => None,
        }
    }
}

/// The table header, readable without a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbHeader {
    /// Table version (0 if there was no `FC FD FE FF` marker).
    pub version: u32,
    /// Whether the version marker was present.
    pub has_version_marker: bool,
    /// The u8 after the version (meaning UNKNOWN; 1 in shipped tables).
    pub flag: u8,
    /// Number of rows that follow.
    pub row_count: u32,
    /// Byte offset of the first row.
    pub data_offset: usize,
}

impl DbHeader {
    /// Reads just the header.
    pub fn read(bytes: &[u8]) -> Result<Self, DbError> {
        let mut c = Cursor::new(bytes);
        Self::read_from(&mut c)
    }

    fn read_from(c: &mut Cursor<'_>) -> Result<Self, DbError> {
        let has_version_marker = c.peek(4) == Some(&VERSION_MARKER[..]);
        let version = if has_version_marker {
            c.take(4)?;
            c.u32()?
        } else {
            0
        };
        let flag = c.u8()?;
        let row_count = c.u32()?;
        Ok(Self { version, has_version_marker, flag, row_count, data_offset: c.pos() })
    }
}

/// A decoded table: header fields plus rows of [`DbValue`]s, one per schema field.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTable {
    /// Table version (selects which versioned fields are present).
    pub version: u32,
    /// Whether the file had the `FC FD FE FF` version marker.
    pub has_version_marker: bool,
    /// The UNKNOWN header byte, kept as read.
    pub flag: u8,
    /// The rows. Every row has exactly `schema.fields.len()` values, absent ones defaulted.
    pub rows: Vec<Vec<DbValue>>,
}

impl DbTable {
    /// Decodes a whole table with `schema`. The rows must end **exactly** at the end of
    /// `bytes`. Leftover bytes mean the schema is wrong, so that is an error.
    pub fn read(bytes: &[u8], schema: &Schema) -> Result<Self, DbError> {
        let mut c = Cursor::new(bytes);
        let header = DbHeader::read_from(&mut c)?;
        let present = schema.fields.iter().filter(|f| header.version >= f.min_version).count();
        if present == 0 && header.row_count > 0 {
            // Rows of zero bytes would let a corrupt count loop for billions of iterations.
            return Err(DbError::EmptySchema);
        }
        // Each row consumes at least one byte, so the remaining length caps the allocation.
        let mut rows = Vec::with_capacity((header.row_count as usize).min(c.remaining()));
        for _ in 0..header.row_count {
            let mut row: Vec<DbValue> = Vec::with_capacity(schema.fields.len());
            for f in &schema.fields {
                let value = if header.version >= f.min_version {
                    read_value(&mut c, f.ty)?
                } else {
                    match (f.if_absent, row.last()) {
                        (IfAbsent::CopyPrevious, Some(prev)) => prev.clone(),
                        _ => DbValue::default_for(f.ty),
                    }
                };
                row.push(value);
            }
            rows.push(row);
        }
        if c.remaining() != 0 {
            return Err(DbError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { version: header.version, has_version_marker: header.has_version_marker, flag: header.flag, rows })
    }

    /// Encodes the table back to file bytes, **in memory** (for modding tools and tests;
    /// nothing is written to disk). Fields absent at this version are not written, so
    /// `DbTable::read(&t.to_bytes(&s)?, &s)` gives `t` back. Returns `None` if a cell's type
    /// does not match its schema field or a row has the wrong number of cells.
    pub fn to_bytes(&self, schema: &Schema) -> Option<Vec<u8>> {
        let mut b = Vec::new();
        if self.has_version_marker {
            b.extend_from_slice(&VERSION_MARKER);
            b.extend_from_slice(&self.version.to_le_bytes());
        }
        b.push(self.flag);
        b.extend_from_slice(&(self.rows.len() as u32).to_le_bytes());
        let utf16 = |b: &mut Vec<u8>, s: &str| {
            let units: Vec<u16> = s.encode_utf16().collect();
            b.extend_from_slice(&(units.len() as u16).to_le_bytes());
            for u in units {
                b.extend_from_slice(&u.to_le_bytes());
            }
        };
        for row in &self.rows {
            if row.len() != schema.fields.len() {
                return None;
            }
            for (f, v) in schema.fields.iter().zip(row) {
                if self.version < f.min_version {
                    continue;
                }
                match (f.ty, v) {
                    (FieldType::Str, DbValue::Str(s)) => utf16(&mut b, s),
                    (FieldType::OptStr, DbValue::OptStr(None)) => b.push(0),
                    (FieldType::OptStr, DbValue::OptStr(Some(s))) => {
                        b.push(1);
                        utf16(&mut b, s);
                    }
                    (FieldType::Bool, DbValue::Bool(x)) => b.push(u8::from(*x)),
                    (FieldType::I32, DbValue::I32(x)) => b.extend_from_slice(&x.to_le_bytes()),
                    (FieldType::F32, DbValue::F32(x)) => b.extend_from_slice(&x.to_bits().to_le_bytes()),
                    (FieldType::U16, DbValue::U16(x)) => b.extend_from_slice(&x.to_le_bytes()),
                    _ => return None,
                }
            }
        }
        Some(b)
    }
}

fn read_value(c: &mut Cursor<'_>, ty: FieldType) -> Result<DbValue, DbError> {
    Ok(match ty {
        FieldType::Str => DbValue::Str(c.utf16()?),
        FieldType::OptStr => DbValue::OptStr(if c.u8()? != 0 { Some(c.utf16()?) } else { None }),
        FieldType::Bool => DbValue::Bool(c.u8()? != 0),
        FieldType::I32 => DbValue::I32(c.u32()? as i32),
        FieldType::F32 => DbValue::F32(f32::from_bits(c.u32()?)),
        FieldType::U16 => DbValue::U16(c.u16()?),
    })
}

/// Everything that can go wrong decoding a DB table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbError {
    /// The data ended in the middle of the header or a row.
    UnexpectedEof {
        /// Where the read started.
        offset: usize,
        /// How many bytes were needed.
        needed: usize,
    },
    /// A string had an unpaired UTF-16 surrogate.
    InvalidUtf16 {
        /// Where the string starts.
        offset: usize,
    },
    /// All rows were read but bytes remain, so the schema does not match the table.
    TrailingBytes {
        /// Where the leftover bytes start.
        offset: usize,
        /// How many there are.
        count: usize,
    },
    /// The schema has no fields at this table version, but the table has rows.
    EmptySchema,
}

impl From<ReadError> for DbError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => {
                write!(f, "DB table ended at 0x{offset:x} (needed {needed} more bytes)")
            }
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 string at 0x{offset:x}"),
            Self::TrailingBytes { offset, count } => {
                write!(f, "{count} bytes left over at 0x{offset:x}; the schema does not match")
            }
            Self::EmptySchema => write!(f, "schema has no fields for this table version"),
        }
    }
}

impl std::error::Error for DbError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::utf16_bytes;

    /// Builds a table: optional version marker, flag 1, row count, then raw row bytes.
    fn table(version: Option<u32>, rows: u32, body: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        if let Some(v) = version {
            b.extend_from_slice(&VERSION_MARKER);
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(1);
        b.extend_from_slice(&rows.to_le_bytes());
        b.extend_from_slice(body);
        b
    }

    /// A made-up schema (not a real table): key, optional text, bool, count, ratio, u16.
    fn schema() -> Schema {
        Schema::from_codes("s o b i f h").unwrap()
    }

    #[test]
    fn reads_unversioned_table() {
        let mut body = utf16_bytes("key_\u{e9}");
        body.extend_from_slice(&[1]);
        body.extend(utf16_bytes("opt"));
        body.extend_from_slice(&[1]);
        body.extend_from_slice(&(-5i32).to_le_bytes());
        body.extend_from_slice(&1.5f32.to_le_bytes());
        body.extend_from_slice(&300u16.to_le_bytes());
        // Row 2: empty string, absent optional, false, 0, 0.0, 0.
        body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        let t = DbTable::read(&table(None, 2, &body), &schema()).unwrap();
        assert_eq!((t.version, t.has_version_marker, t.flag), (0, false, 1));
        assert_eq!(
            t.rows[0],
            vec![
                DbValue::Str("key_\u{e9}".into()),
                DbValue::OptStr(Some("opt".into())),
                DbValue::Bool(true),
                DbValue::I32(-5),
                DbValue::F32(1.5),
                DbValue::U16(300)
            ]
        );
        assert_eq!(t.rows[1][0].as_str(), Some(""));
        assert_eq!(t.rows[1][1], DbValue::OptStr(None));
        assert_eq!(t.rows[1][1].as_str_or_empty(), Some(""));
    }

    #[test]
    fn empty_table_is_five_bytes() {
        let t = DbTable::read(&[1, 0, 0, 0, 0], &schema()).unwrap();
        assert!(t.rows.is_empty());
    }

    #[test]
    fn version_guards_and_copy_previous() {
        let s = Schema::new()
            .field(FieldType::I32)
            .field_since_or_copy(FieldType::I32, 1)
            .field_since(FieldType::OptStr, 2);
        // v0: only the first field is on disk; the second copies it, the third is defaulted.
        let t = DbTable::read(&table(None, 1, &7i32.to_le_bytes()), &s).unwrap();
        assert_eq!(t.rows[0], vec![DbValue::I32(7), DbValue::I32(7), DbValue::OptStr(None)]);
        // v1: two fields on disk.
        let mut body = 7i32.to_le_bytes().to_vec();
        body.extend_from_slice(&9i32.to_le_bytes());
        let t = DbTable::read(&table(Some(1), 1, &body), &s).unwrap();
        assert_eq!((t.version, t.has_version_marker), (1, true));
        assert_eq!(t.rows[0][1], DbValue::I32(9));
        // v2: all three.
        body.push(0);
        assert_eq!(DbTable::read(&table(Some(2), 1, &body), &s).unwrap().rows[0].len(), 3);
    }

    #[test]
    fn to_bytes_round_trips() {
        let s = Schema::new()
            .field(FieldType::Str)
            .field(FieldType::OptStr)
            .field(FieldType::Bool)
            .field(FieldType::F32)
            .field_since(FieldType::U16, 1)
            .field_since_or_copy(FieldType::I32, 2);
        for (version, marker) in [(0, false), (1, true), (2, true)] {
            let row = |k: &str, o: Option<&str>| {
                vec![
                    DbValue::Str(k.into()),
                    DbValue::OptStr(o.map(Into::into)),
                    DbValue::Bool(true),
                    DbValue::F32(1.5),
                    DbValue::U16(if version >= 1 { 7 } else { 0 }),
                    DbValue::I32(if version >= 2 { -3 } else if version >= 1 { 7 } else { 0 }),
                ]
            };
            let t = DbTable { version, has_version_marker: marker, flag: 1, rows: vec![row("é", Some("x")), row("", None)] };
            let bytes = t.to_bytes(&s).unwrap();
            let back = DbTable::read(&bytes, &s).unwrap();
            // Absent columns are filled in by the reader, so compare bytes and the always-present columns.
            assert_eq!(back.rows.len(), 2);
            assert_eq!(back.to_bytes(&s).unwrap(), bytes, "v{version}");
            assert_eq!(back.rows[0][..4], t.rows[0][..4]);
        }
        let bad = DbTable { version: 0, has_version_marker: false, flag: 1, rows: vec![vec![DbValue::I32(1)]] };
        assert!(bad.to_bytes(&s).is_none());
    }

    #[test]
    fn empty_string_is_not_two_bools() {
        // "00 00" read as a string with schema "s" leaves nothing over...
        assert!(DbTable::read(&table(None, 1, &[0, 0]), &Schema::from_codes("s").unwrap()).is_ok());
        // ...and with the wrong schema "b" it leaves a byte, which is reported, not guessed around.
        assert!(matches!(
            DbTable::read(&table(None, 1, &[0, 0]), &Schema::from_codes("b").unwrap()),
            Err(DbError::TrailingBytes { count: 1, .. })
        ));
    }

    #[test]
    fn errors_never_panic() {
        let mut body = utf16_bytes("abc");
        body.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let good = table(Some(3), 1, &body);
        assert!(DbTable::read(&good, &schema()).is_ok());
        for len in 0..good.len() {
            assert!(DbTable::read(&good[..len], &schema()).is_err(), "len {len}");
        }
        // Huge row count with tiny data.
        let huge = table(None, u32::MAX, &[0, 0]);
        assert!(matches!(DbTable::read(&huge, &schema()), Err(DbError::UnexpectedEof { .. })));
        assert!(matches!(DbTable::read(&huge, &Schema::new()), Err(DbError::EmptySchema)));
        // Bad UTF-16.
        let bad = table(None, 1, &[1, 0, 0x00, 0xDC]);
        assert!(matches!(
            DbTable::read(&bad, &Schema::from_codes("s").unwrap()),
            Err(DbError::InvalidUtf16 { offset: 5 })
        ));
    }

    #[test]
    fn header_only() {
        let h = DbHeader::read(&table(Some(4), 442, &[])).unwrap();
        assert_eq!((h.version, h.flag, h.row_count, h.data_offset), (4, 1, 442, 13));
        assert!(Schema::from_codes("sx").is_none());
    }
}

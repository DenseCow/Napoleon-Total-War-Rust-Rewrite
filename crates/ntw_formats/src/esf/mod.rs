//! ESF: Creative Assembly's binary tree format (the "ABCE" variant used by Napoleon).
//!
//! # What it is used for
//! Campaign start positions (`startpos.esf`), save games (`*.save`), campaign map
//! data (`regions.esf`, `pathfinding.esf`, ...) and some battle-map lists inside
//! packs (`.tree_list`, `.building_list`, ...) all use this one container.
//!
//! # Layout (Worker 3 report §2, byte-exact, CONFIRMED on every shipped file)
//! All integers are little-endian.
//! ```text
//! 0x00  u32 magic         0x0000ABCE (bytes CE AB 00 00)
//! 0x04  u32 unknown       0 in every file seen
//! 0x08  u32 timestamp     unix seconds
//! 0x0C  u32 names_offset  absolute offset of the record-name table
//! 0x10  root node         always a record (type 0x80)
//! names_offset:
//!       u16 name_count, then name_count x { u16 len; len bytes }
//! ```
//! Each node starts with a one-byte type code (see [`codes`]):
//! * `0x01..=0x10`: a primitive value (bool, ints, floats, coordinates, strings, angle);
//! * `0x40 + t`: a packed array of primitive `t`: a u32 **absolute end offset**, then
//!   the elements back to back until that offset;
//! * `0x80`: a record: u16 name index, u8 version, u32 absolute end offset, children;
//! * `0x81`: a record array: u16 name index, u8 version, u32 absolute end offset,
//!   u32 item count, then per item a u32 absolute end offset and its children.
//!
//! Only records have names. Values inside a record have no names, so code finds
//! them by position. Records carry a `version` byte, and different versions of the
//! same record can have different layouts.
//!
//! # Lossless
//! [`EsfFile::from_bytes`] keeps everything: the header, the name table in its
//! original order, every record's version and every child in order. So
//! `EsfFile::from_bytes(b)?.to_bytes()? == b` for every valid file.
//!
//! # Example
//! ```no_run
//! use ntw_formats::esf::EsfFile;
//! let bytes = std::fs::read("startpos.esf").unwrap();
//! let esf = EsfFile::from_bytes(&bytes).unwrap();
//! let world = esf.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
//! println!("WORLD v{} has {} children", world.version, world.children.len());
//! ```

mod error;
mod node;
mod read;
mod write;

pub use error::EsfError;
pub use node::{
    EsfNode, EsfPathTarget, EsfRecord, EsfRecordArray, Fixed20, codes, type_name,
};
pub use write::EsfWriter;

/// The magic number at the start of every ESF file this crate reads.
pub const MAGIC: u32 = 0x0000_ABCE;

/// How deep records may nest before the file is rejected as corrupt.
/// Real files nest about 30 deep; the limit stops a crafted file from overflowing the stack.
pub const MAX_DEPTH: usize = 512;

/// The 16-byte file header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EsfHeader {
    /// Always [`MAGIC`].
    pub magic: u32,
    /// Bytes 0x04..0x08. 0 in every file seen; meaning UNKNOWN. Kept for round-tripping.
    pub unknown_04: u32,
    /// Unix time in seconds: when the file was made (a save stores the time of saving).
    pub timestamp: u32,
    /// Absolute offset of the name table as read from the file.
    /// When writing, this is recomputed and the stored value is ignored.
    pub names_offset: u32,
}

/// A whole parsed ESF file.
#[derive(Debug, Clone, PartialEq)]
pub struct EsfFile {
    /// The 16-byte header.
    pub header: EsfHeader,
    /// The record-name table in file order. Record names in the tree are already
    /// resolved to strings, so you only need this for byte-exact rewriting.
    pub names: Vec<String>,
    /// The root record (e.g. `CAMPAIGN_STARTPOS` in a startpos file).
    pub root: EsfRecord,
}

impl EsfFile {
    /// Makes a new file around `root`, with timestamp 0 and an empty name table
    /// (the writer adds names as it meets them).
    pub fn new(root: EsfRecord) -> Self {
        Self {
            header: EsfHeader { magic: MAGIC, unknown_04: 0, timestamp: 0, names_offset: 0 },
            names: Vec::new(),
            root,
        }
    }

    /// Parses ESF bytes. Fails with an [`EsfError`] on any malformed input. It never panics.
    ///
    /// Checks that the root record ends exactly at the name table and that the name
    /// table ends exactly at the end of the data.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EsfError> {
        read::read_file(bytes)
    }

    /// Reads and parses a file from disk (read-only).
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_bytes(&bytes)?)
    }

    /// Serializes back to bytes. See [`EsfWriter::write`].
    pub fn to_bytes(&self) -> Result<Vec<u8>, EsfError> {
        EsfWriter::write(self)
    }

    /// Follows a path below the root record, e.g. `"CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD"`.
    /// See [`EsfRecord::lookup`] for the syntax (including `NAME[i]`).
    pub fn find_path(&self, path: &str) -> Option<&EsfRecord> {
        self.root.find_path(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny hand-made ESF file. Offsets were computed by hand:
    /// ```text
    /// 0x10 record ROOT v1, end 0x4B
    /// 0x18   utf16 "Hi"
    /// 0x1F   i32[] [1, -2], end 0x2C
    /// 0x2C   record_array ITEMS v2, end 0x41, 1 item
    /// 0x38     item end 0x41
    /// 0x3C       u32 7
    /// 0x41   record CHILD v3, end 0x4B
    /// 0x49     bool true
    /// 0x4B name table: ROOT, ITEMS, CHILD
    /// ```
    fn tiny() -> Vec<u8> {
        let mut b = vec![
            0xCE, 0xAB, 0x00, 0x00, // magic
            0x00, 0x00, 0x00, 0x00, // unknown
            0x13, 0xB4, 0x27, 0x4B, // timestamp
            0x4B, 0x00, 0x00, 0x00, // names_offset
            0x80, 0x00, 0x00, 0x01, 0x4B, 0x00, 0x00, 0x00, // ROOT
            0x0E, 0x02, 0x00, b'H', 0x00, b'i', 0x00, // utf16 "Hi"
            0x44, 0x2C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xFE, 0xFF, 0xFF, 0xFF, // i32[]
            0x81, 0x01, 0x00, 0x02, 0x41, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, // ITEMS
            0x41, 0x00, 0x00, 0x00, // item end
            0x08, 0x07, 0x00, 0x00, 0x00, // u32 7
            0x80, 0x02, 0x00, 0x03, 0x4B, 0x00, 0x00, 0x00, // CHILD
            0x01, 0x01, // bool true
            0x03, 0x00, // 3 names
        ];
        for name in ["ROOT", "ITEMS", "CHILD"] {
            b.extend_from_slice(&(name.len() as u16).to_le_bytes());
            b.extend_from_slice(name.as_bytes());
        }
        b
    }

    #[test]
    fn reads_tiny_file() {
        let f = EsfFile::from_bytes(&tiny()).unwrap();
        assert_eq!(f.header.magic, MAGIC);
        assert_eq!(f.header.timestamp, 0x4B27_B413);
        assert_eq!(f.header.names_offset, 0x4B);
        assert_eq!(f.names, ["ROOT", "ITEMS", "CHILD"]);
        assert_eq!(f.root.name, "ROOT");
        assert_eq!(f.root.version, 1);
        assert_eq!(f.root.get_str(0), Some("Hi"));
        assert_eq!(f.root.get(1).unwrap().as_i32_array(), Some(&[1, -2][..]));
        let items = f.root.record_array("ITEMS").unwrap();
        assert_eq!(items.version, 2);
        assert_eq!(items.items, vec![vec![EsfNode::U32(7)]]);
        let child = f.find_path("CHILD").unwrap();
        assert_eq!((child.version, child.get_bool(0)), (3, Some(true)));
        assert_eq!(f.root.find_item("ITEMS[0]").unwrap()[0].as_u32(), Some(7));
    }

    #[test]
    fn tiny_round_trips_byte_exact() {
        let bytes = tiny();
        assert_eq!(EsfFile::from_bytes(&bytes).unwrap().to_bytes().unwrap(), bytes);
    }

    /// A tree that uses every node type, built in code.
    fn every_type() -> EsfFile {
        let mut inner = EsfRecord::new("INNER", 9);
        inner.children = vec![EsfNode::I8(-3), EsfNode::I16(-300), EsfNode::I64(-5_000_000_000)];
        let mut arr = EsfRecordArray::new("LIST", 4);
        arr.items = vec![vec![], vec![EsfNode::Record(Box::new(inner))], vec![EsfNode::U8(1), EsfNode::U8(2)]];
        let mut root = EsfRecord::new("ROOT", 0);
        root.children = vec![
            EsfNode::Bool(false),
            EsfNode::I32(-1),
            EsfNode::U8(200),
            EsfNode::U16(60_000),
            EsfNode::U32(4_000_000_000),
            EsfNode::U64(u64::MAX),
            EsfNode::F32(1.5),
            EsfNode::F64(-2.25),
            EsfNode::Coord2d(1.0, 2.0),
            EsfNode::Coord3d(1.0, 2.0, 3.0),
            EsfNode::Utf16String("Napol\u{e9}on \u{1F600}".into()),
            EsfNode::AsciiString("caf\u{e9}".into()),
            EsfNode::Angle(16_384),
            EsfNode::BoolArray(vec![true, false]),
            EsfNode::I8Array(vec![-1, 1]),
            EsfNode::I16Array(vec![-1, 1]),
            EsfNode::I32Array(vec![]),
            EsfNode::I64Array(vec![i64::MIN]),
            EsfNode::U8Array(vec![1, 2, 3]),
            EsfNode::U16Array(vec![9]),
            EsfNode::U32Array(vec![1, 2]),
            EsfNode::U64Array(vec![7]),
            EsfNode::F32Array(vec![0.5]),
            EsfNode::F64Array(vec![0.25]),
            EsfNode::Coord2dArray(vec![(1.0, 2.0)]),
            EsfNode::Coord3dArray(vec![(1.0, 2.0, 3.0)]),
            EsfNode::Utf16Array(vec!["a".into(), String::new()]),
            EsfNode::AsciiArray(vec!["land".into(), "sea".into()]),
            EsfNode::AngleArray(vec![1, 2]),
            EsfNode::RecordArray(Box::new(arr)),
            EsfNode::Record(Box::new(EsfRecord::new("EMPTY", 1))),
        ];
        EsfFile::new(root)
    }

    #[test]
    fn every_type_round_trips() {
        let file = every_type();
        let bytes = file.to_bytes().unwrap();
        let back = EsfFile::from_bytes(&bytes).unwrap();
        assert_eq!(back.root, file.root);
        // The writer appended names in first-use order.
        assert_eq!(back.names, ["ROOT", "LIST", "INNER", "EMPTY"]);
        assert_eq!(back.to_bytes().unwrap(), bytes);
    }

    #[test]
    fn writer_keeps_existing_name_order() {
        let mut file = EsfFile::new(EsfRecord::new("B", 0));
        file.names = vec!["unused".into(), "B".into()];
        let back = EsfFile::from_bytes(&file.to_bytes().unwrap()).unwrap();
        assert_eq!(back.names, ["unused", "B"]);
    }

    #[test]
    fn writer_rejects_unencodable_ascii() {
        let mut root = EsfRecord::new("R", 0);
        root.children.push(EsfNode::AsciiString("\u{263A}".into()));
        assert!(matches!(EsfFile::new(root).to_bytes(), Err(EsfError::NotSingleByte { .. })));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = tiny();
        b[0] = 0;
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::BadMagic(_))));
    }

    #[test]
    fn rejects_every_truncation_without_panicking() {
        let b = tiny();
        for len in 0..b.len() {
            assert!(EsfFile::from_bytes(&b[..len]).is_err(), "len {len} parsed");
        }
    }

    #[test]
    fn rejects_every_single_byte_corruption_without_panicking() {
        // Flip each byte; the result may parse or not, but must never panic.
        let b = tiny();
        for i in 0..b.len() {
            for v in [0x00, 0xFF, b[i] ^ 0x40, b[i].wrapping_add(1)] {
                let mut c = b.clone();
                c[i] = v;
                let _ = EsfFile::from_bytes(&c);
            }
        }
    }

    #[test]
    fn rejects_end_offset_past_parent() {
        let mut b = tiny();
        b[0x1F + 1] = 0x60; // i32[] end -> 0x60, beyond ROOT's end 0x4B
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::BadEndOffset { offset: 0x1F, .. })));
    }

    #[test]
    fn rejects_end_offset_backwards() {
        let mut b = tiny();
        b[0x41 + 4] = 0x10; // CHILD end -> 0x10
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::BadEndOffset { offset: 0x41, .. })));
    }

    #[test]
    fn rejects_odd_array_length() {
        let mut b = tiny();
        b[0x1F] = 0x47; // i32[] of 8 bytes -> u16[]: fine. Use coord3d[] (12-byte elements) instead.
        assert!(EsfFile::from_bytes(&b).is_ok());
        b[0x1F] = 0x4D;
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::BadArrayLength { code: 0x4D, .. })));
    }

    #[test]
    fn rejects_unknown_type_code() {
        let mut b = tiny();
        b[0x18] = 0x11;
        assert!(matches!(
            EsfFile::from_bytes(&b),
            Err(EsfError::UnknownTypeCode { code: 0x11, offset: 0x18 })
        ));
    }

    #[test]
    fn rejects_bad_name_index() {
        let mut b = tiny();
        b[0x41 + 1] = 9;
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::NameIndexOutOfRange { index: 9, .. })));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut b = tiny();
        b.push(0);
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::TrailingBytes { count: 1 })));
    }

    #[test]
    fn rejects_root_that_is_not_a_record() {
        let mut b = tiny();
        b[0x10] = 0x01;
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::RootNotRecord { type_code: 1 })));
    }

    #[test]
    fn rejects_record_array_with_short_items() {
        let mut b = tiny();
        b[0x2C + 8] = 0; // ITEMS claims 0 items but has bytes up to its end
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::BlockOverrun { offset: 0x2C, .. })));
    }

    #[test]
    fn rejects_huge_item_count_without_allocating() {
        let mut b = tiny();
        b[0x2C + 8..0x2C + 12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(EsfFile::from_bytes(&b).is_err());
    }

    #[test]
    fn rejects_excessive_nesting() {
        let mut rec = EsfRecord::new("N", 0);
        for _ in 0..MAX_DEPTH + 5 {
            let mut outer = EsfRecord::new("N", 0);
            outer.children.push(EsfNode::Record(Box::new(rec)));
            rec = outer;
        }
        let bytes = EsfFile::new(rec).to_bytes().unwrap();
        assert!(matches!(EsfFile::from_bytes(&bytes), Err(EsfError::TooDeep { .. })));
    }

    #[test]
    fn rejects_unpaired_surrogate() {
        let mut b = tiny();
        b[0x1B..0x1D].copy_from_slice(&0xD800u16.to_le_bytes());
        assert!(matches!(EsfFile::from_bytes(&b), Err(EsfError::InvalidUtf16 { offset: 0x19 })));
    }
}

//! The error type shared by the ESF reader and writer.

use std::fmt;

/// Everything that can go wrong while reading or writing an ESF file.
///
/// Every variant that comes from reading carries the byte `offset` where the
/// problem was found, so a bad file can be inspected in a hex editor.
/// The reader never panics on bad input: it returns one of these instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EsfError {
    /// The file ended (or a block ended) before a value could be read completely.
    UnexpectedEof {
        /// Where the read started.
        offset: usize,
        /// How many bytes were needed.
        needed: usize,
    },
    /// The first four bytes are not `CE AB 00 00`.
    BadMagic(u32),
    /// The header's `names_offset` points outside the file or into the header.
    BadNamesOffset {
        /// The value stored in the header.
        names_offset: u32,
        /// Total file length.
        file_len: usize,
    },
    /// The root node is not a record (type 0x80).
    RootNotRecord {
        /// The type byte found at offset 0x10.
        type_code: u8,
    },
    /// A type byte that is not part of the format.
    UnknownTypeCode {
        /// The unknown type byte.
        code: u8,
        /// Where it was found.
        offset: usize,
    },
    /// An absolute end offset points backwards, or past its parent block.
    BadEndOffset {
        /// Where the node that stores the offset starts.
        offset: usize,
        /// The end offset stored in the file.
        end: u32,
        /// Smallest allowed value (the current read position).
        min: usize,
        /// Largest allowed value (the parent's end).
        max: usize,
    },
    /// The children of a block did not finish exactly at the block's end offset.
    BlockOverrun {
        /// Where the block starts.
        offset: usize,
        /// The block's declared end.
        end: usize,
        /// Where reading actually stopped.
        reached: usize,
    },
    /// A packed array's byte length is not a multiple of its element size.
    BadArrayLength {
        /// Where the array node starts.
        offset: usize,
        /// The array's type byte.
        code: u8,
        /// The array's payload length in bytes.
        len: usize,
    },
    /// A record refers to a name that is not in the name table.
    NameIndexOutOfRange {
        /// Where the record starts.
        offset: usize,
        /// The bad index.
        index: u16,
        /// How many names the table holds.
        name_count: usize,
    },
    /// A UTF-16 string contains an unpaired surrogate.
    InvalidUtf16 {
        /// Where the string's length prefix starts.
        offset: usize,
    },
    /// The root record did not end exactly where the name table starts.
    RootEndMismatch {
        /// Where the root record ended.
        root_end: usize,
        /// The header's `names_offset`.
        names_offset: usize,
    },
    /// There are bytes after the name table (no shipped file has any).
    TrailingBytes {
        /// How many extra bytes there are.
        count: usize,
    },
    /// Records are nested deeper than [`super::MAX_DEPTH`]; the file is almost certainly corrupt.
    TooDeep {
        /// Where the record that is too deep starts.
        offset: usize,
    },
    /// Writer: a string is longer than 65535 units and cannot be stored with a u16 length.
    StringTooLong {
        /// The string's length in units (bytes for ASCII, UTF-16 units otherwise).
        len: usize,
    },
    /// Writer: an ASCII string or record name contains a character above U+00FF,
    /// which cannot be stored as one byte.
    NotSingleByte {
        /// The character that does not fit.
        ch: char,
    },
    /// Writer: there are more than 65535 distinct record names.
    TooManyNames,
    /// Writer: a record array has more than `u32::MAX` items.
    TooManyItems,
    /// Writer: the output grew past 4 GiB, so absolute offsets no longer fit in a u32.
    OutputTooLarge,
}

impl fmt::Display for EsfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => {
                write!(f, "unexpected end of data at 0x{offset:x} (needed {needed} bytes)")
            }
            Self::BadMagic(m) => write!(f, "bad ESF magic 0x{m:08x} (expected 0x0000abce)"),
            Self::BadNamesOffset { names_offset, file_len } => write!(
                f,
                "name table offset 0x{names_offset:x} is outside the file (length 0x{file_len:x})"
            ),
            Self::RootNotRecord { type_code } => {
                write!(f, "root node has type 0x{type_code:02x}, expected a record (0x80)")
            }
            Self::UnknownTypeCode { code, offset } => {
                write!(f, "unknown node type 0x{code:02x} at 0x{offset:x}")
            }
            Self::BadEndOffset { offset, end, min, max } => write!(
                f,
                "node at 0x{offset:x} has end offset 0x{end:x}, allowed range 0x{min:x}..=0x{max:x}"
            ),
            Self::BlockOverrun { offset, end, reached } => write!(
                f,
                "block at 0x{offset:x} should end at 0x{end:x} but its children reached 0x{reached:x}"
            ),
            Self::BadArrayLength { offset, code, len } => write!(
                f,
                "array 0x{code:02x} at 0x{offset:x} has {len} bytes, not a whole number of elements"
            ),
            Self::NameIndexOutOfRange { offset, index, name_count } => write!(
                f,
                "record at 0x{offset:x} uses name index {index}, but the table has {name_count} names"
            ),
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 string at 0x{offset:x}"),
            Self::RootEndMismatch { root_end, names_offset } => write!(
                f,
                "root record ends at 0x{root_end:x} but the name table starts at 0x{names_offset:x}"
            ),
            Self::TrailingBytes { count } => write!(f, "{count} unexpected bytes after the name table"),
            Self::TooDeep { offset } => write!(f, "records nested too deeply at 0x{offset:x}"),
            Self::StringTooLong { len } => write!(f, "string of length {len} does not fit a u16 length"),
            Self::NotSingleByte { ch } => {
                write!(f, "character {ch:?} cannot be stored in a single-byte string")
            }
            Self::TooManyNames => write!(f, "more than 65535 distinct record names"),
            Self::TooManyItems => write!(f, "record array has more than u32::MAX items"),
            Self::OutputTooLarge => write!(f, "output exceeds 4 GiB; offsets do not fit in u32"),
        }
    }
}

impl std::error::Error for EsfError {}

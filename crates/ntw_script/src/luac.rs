//! Converting the original's compiled Lua (`.luac`) so our Lua can load it.
//!
//! # Why this is needed (plain words)
//! A `.luac` file is a Lua script that was compiled ahead of time into "bytecode". Its header says
//! how big some C types were on the machine that compiled it. The original game's header is
//! `1B 4C 75 61 | 51 | 00 | 01 | 04 04 04 04 | 00` (W3 §6.1, CONFIRMED):
//! Lua 5.1, official format, little-endian, `int` 4 bytes, `size_t` 4 bytes, instruction 4 bytes,
//! **number 4 bytes (a `float`)**.
//!
//! Our Lua (mlua's bundled Lua 5.1, see DESIGN §3.5) uses 8-byte `size_t` on 64-bit Windows and
//! 8-byte `double` numbers, so it refuses the original header. [`convert_chunk`] rewrites the
//! chunk into the host layout:
//! - every string length (`size_t`) is widened from 4 to 8 bytes;
//! - every number constant is widened from `f32` to `f64`. This is **exact**: every `f32` value is
//!   also a `f64` value, so the script sees precisely the original's constants (CONFIRMED by
//!   construction);
//! - everything else (instructions, line numbers, names) is copied byte for byte.
//!
//! The chunk layout below is the documented Lua 5.1 format (the "No-Frills Introduction to Lua 5.1
//! VM Instructions" layout): a header, then one function prototype, recursively:
//! ```text
//! function := source:String  linedefined:int  lastlinedefined:int
//!             nups:u8  numparams:u8  is_vararg:u8  maxstacksize:u8
//!             code: int n, n × Instruction
//!             constants: int n, n × (type:u8, then bool:u8 | number | String)
//!             protos: int n, n × function
//!             lineinfo: int n, n × int
//!             locvars: int n, n × (name:String, startpc:int, endpc:int)
//!             upvalues: int n, n × String
//! String   := size_t len (0 = no string), then len bytes (including the trailing NUL)
//! ```

use std::fmt;

/// The 4-byte signature at the start of every Lua chunk.
pub const SIGNATURE: &[u8; 4] = b"\x1bLua";
/// Size of the Lua 5.1 header in bytes.
pub const HEADER_LEN: usize = 12;

/// The type sizes a chunk was compiled for (the variable part of the header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkLayout {
    /// `sizeof(int)`.
    pub int_size: u8,
    /// `sizeof(size_t)`.
    pub size_t_size: u8,
    /// `sizeof(Instruction)`.
    pub instruction_size: u8,
    /// `sizeof(lua_Number)`.
    pub number_size: u8,
    /// 1 if `lua_Number` is an integer type, 0 for floating point.
    pub integral: u8,
}

impl ChunkLayout {
    /// The original game's layout (W3 §6.1, CONFIRMED): everything 4 bytes, float numbers.
    pub const ORIGINAL: ChunkLayout = ChunkLayout {
        int_size: 4,
        size_t_size: 4,
        instruction_size: 4,
        number_size: 4,
        integral: 0,
    };

    /// The layout of the Lua compiled into this program: `int` 4, `size_t` = pointer size,
    /// instruction 4, `double` numbers. (INFERRED: standard `luaconf.h` on our targets.)
    pub fn host() -> ChunkLayout {
        ChunkLayout {
            int_size: 4,
            size_t_size: std::mem::size_of::<usize>() as u8,
            instruction_size: 4,
            number_size: 8,
            integral: 0,
        }
    }
}

/// Why a chunk could not be converted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuacError {
    /// Does not start with `\x1bLua`, or is not version 5.1 / official format / little-endian.
    NotLua51,
    /// The header's type sizes are not the original game's.
    UnsupportedLayout(ChunkLayout),
    /// The data ended in the middle of something.
    Truncated(&'static str),
    /// A constant had an unknown type tag.
    BadConstantType(u8),
    /// Bytes were left over after the main function.
    TrailingBytes(usize),
}

impl fmt::Display for LuacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LuacError::NotLua51 => write!(f, "not a Lua 5.1 little-endian chunk"),
            LuacError::UnsupportedLayout(l) => write!(f, "unsupported chunk layout {l:?}"),
            LuacError::Truncated(what) => write!(f, "chunk truncated while reading {what}"),
            LuacError::BadConstantType(t) => write!(f, "unknown constant type {t}"),
            LuacError::TrailingBytes(n) => write!(f, "{n} bytes left after the main function"),
        }
    }
}

impl std::error::Error for LuacError {}

/// True if `bytes` starts like a compiled Lua chunk.
pub fn is_bytecode(bytes: &[u8]) -> bool {
    bytes.starts_with(SIGNATURE)
}

/// Reads a chunk's header layout. `Err` if it is not a Lua 5.1 little-endian official chunk.
pub fn read_layout(bytes: &[u8]) -> Result<ChunkLayout, LuacError> {
    if bytes.len() < HEADER_LEN {
        return Err(LuacError::Truncated("header"));
    }
    // signature, version 0x51, format 0 (official), endianness 1 (little).
    if &bytes[0..4] != SIGNATURE || bytes[4] != 0x51 || bytes[5] != 0 || bytes[6] != 1 {
        return Err(LuacError::NotLua51);
    }
    Ok(ChunkLayout {
        int_size: bytes[7],
        size_t_size: bytes[8],
        instruction_size: bytes[9],
        number_size: bytes[10],
        integral: bytes[11],
    })
}

/// Rewrites an original-game chunk ([`ChunkLayout::ORIGINAL`]) into the host layout
/// ([`ChunkLayout::host`]). A chunk that is already in the host layout is returned unchanged.
pub fn convert_chunk(bytes: &[u8]) -> Result<Vec<u8>, LuacError> {
    let layout = read_layout(bytes)?;
    let host = ChunkLayout::host();
    if layout == host {
        return Ok(bytes.to_vec());
    }
    if layout != ChunkLayout::ORIGINAL {
        return Err(LuacError::UnsupportedLayout(layout));
    }
    let mut c = Converter {
        src: bytes,
        pos: HEADER_LEN,
        out: Vec::with_capacity(bytes.len() * 3 / 2),
        wide_size_t: host.size_t_size == 8,
    };
    c.out.extend_from_slice(&bytes[..7]);
    c.out.extend_from_slice(&[
        host.int_size,
        host.size_t_size,
        host.instruction_size,
        host.number_size,
        host.integral,
    ]);
    c.function()?;
    if c.pos != bytes.len() {
        return Err(LuacError::TrailingBytes(bytes.len() - c.pos));
    }
    Ok(c.out)
}

/// Walks the original chunk and writes the host chunk.
struct Converter<'a> {
    src: &'a [u8],
    pos: usize,
    out: Vec<u8>,
    /// Host `size_t` is 8 bytes (otherwise 4, and lengths are copied as they are).
    wide_size_t: bool,
}

impl Converter<'_> {
    fn take(&mut self, n: usize, what: &'static str) -> Result<&[u8], LuacError> {
        let end = self.pos.checked_add(n).ok_or(LuacError::Truncated(what))?;
        let s = self.src.get(self.pos..end).ok_or(LuacError::Truncated(what))?;
        self.pos = end;
        Ok(s)
    }

    fn copy(&mut self, n: usize, what: &'static str) -> Result<(), LuacError> {
        let s = self.take(n, what)?.to_vec();
        self.out.extend_from_slice(&s);
        Ok(())
    }

    /// Copies a 4-byte `int` and returns it.
    fn int(&mut self, what: &'static str) -> Result<i32, LuacError> {
        let b = self.take(4, what)?;
        let v = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        self.out.extend_from_slice(&v.to_le_bytes());
        Ok(v)
    }

    /// A count (`int n`), rejected if negative.
    fn count(&mut self, what: &'static str) -> Result<usize, LuacError> {
        usize::try_from(self.int(what)?).map_err(|_| LuacError::Truncated(what))
    }

    fn byte(&mut self, what: &'static str) -> Result<u8, LuacError> {
        let b = self.take(1, what)?[0];
        self.out.push(b);
        Ok(b)
    }

    /// A String: 4-byte length in, host-size length out, then the bytes.
    fn string(&mut self, what: &'static str) -> Result<(), LuacError> {
        let b = self.take(4, what)?;
        let len = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if self.wide_size_t {
            self.out.extend_from_slice(&u64::from(len).to_le_bytes());
        } else {
            self.out.extend_from_slice(&len.to_le_bytes());
        }
        self.copy(len as usize, what)
    }

    fn function(&mut self) -> Result<(), LuacError> {
        self.string("source")?;
        self.int("linedefined")?;
        self.int("lastlinedefined")?;
        self.copy(4, "nups/numparams/is_vararg/maxstacksize")?;
        let n = self.count("code size")?;
        self.copy(n.checked_mul(4).ok_or(LuacError::Truncated("code"))?, "code")?;
        let n = self.count("constant count")?;
        for _ in 0..n {
            match self.byte("constant type")? {
                0 => {}                                    // nil
                1 => self.copy(1, "boolean constant")?,    // boolean
                3 => {
                    // number: f32 in, f64 out (exact widening)
                    let b = self.take(4, "number constant")?;
                    let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    self.out.extend_from_slice(&f64::from(v).to_le_bytes());
                }
                4 => self.string("string constant")?,
                t => return Err(LuacError::BadConstantType(t)),
            }
        }
        let n = self.count("proto count")?;
        for _ in 0..n {
            self.function()?;
        }
        let n = self.count("lineinfo size")?;
        self.copy(n.checked_mul(4).ok_or(LuacError::Truncated("lineinfo"))?, "lineinfo")?;
        let n = self.count("locvar count")?;
        for _ in 0..n {
            self.string("locvar name")?;
            self.int("startpc")?;
            self.int("endpc")?;
        }
        let n = self.count("upvalue count")?;
        for _ in 0..n {
            self.string("upvalue name")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-made original-layout chunk (our own bytes, not game data): a main function with one
    /// number constant 0.95f32 and one string constant "hi", no code.
    fn tiny_original_chunk() -> Vec<u8> {
        let mut b = vec![0x1b, b'L', b'u', b'a', 0x51, 0, 1, 4, 4, 4, 4, 0];
        let s = |b: &mut Vec<u8>, s: &[u8]| {
            b.extend_from_slice(&(s.len() as u32 + 1).to_le_bytes());
            b.extend_from_slice(s);
            b.push(0);
        };
        s(&mut b, b"@t.lua"); // source
        b.extend_from_slice(&0i32.to_le_bytes()); // linedefined
        b.extend_from_slice(&0i32.to_le_bytes()); // lastlinedefined
        b.extend_from_slice(&[0, 0, 2, 2]); // nups, params, vararg, maxstack
        b.extend_from_slice(&1i32.to_le_bytes()); // 1 instruction
        b.extend_from_slice(&0x0080_001Eu32.to_le_bytes()); // RETURN 0 1
        b.extend_from_slice(&2i32.to_le_bytes()); // 2 constants
        b.push(3);
        b.extend_from_slice(&0.95f32.to_le_bytes());
        b.push(4);
        s(&mut b, b"hi");
        for _ in 0..4 {
            b.extend_from_slice(&0i32.to_le_bytes()); // protos, lineinfo, locvars, upvalues
        }
        b
    }

    #[test]
    fn reads_the_original_header() {
        let c = tiny_original_chunk();
        assert_eq!(read_layout(&c), Ok(ChunkLayout::ORIGINAL));
        assert!(is_bytecode(&c));
        assert_eq!(read_layout(b"print(1)"), Err(LuacError::Truncated("header")));
    }

    #[test]
    fn converts_numbers_and_lengths() {
        let out = convert_chunk(&tiny_original_chunk()).unwrap();
        assert_eq!(read_layout(&out), Ok(ChunkLayout::host()));
        // The widened constant is exactly the f32 value.
        let want = f64::from(0.95f32).to_le_bytes();
        assert!(out.windows(8).any(|w| w == want));
        // Converting a host chunk is a no-op.
        assert_eq!(convert_chunk(&out).unwrap(), out);
        // Truncation is reported, never a panic.
        let c = tiny_original_chunk();
        for cut in HEADER_LEN..c.len() {
            assert!(convert_chunk(&c[..cut]).is_err());
        }
    }

    #[test]
    fn converted_chunk_runs_in_lua() {
        let lua = mlua::Lua::new();
        let out = convert_chunk(&tiny_original_chunk()).unwrap();
        lua.load(&out[..]).exec().unwrap();
    }
}

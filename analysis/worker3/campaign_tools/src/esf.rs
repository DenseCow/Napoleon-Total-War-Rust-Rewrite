//! ESF (ABCE variant) reader, written from byte inspection of Napoleon files.
//!
//! Header (16 bytes):
//!   0x00 u32 magic 0x0000ABCE
//!   0x04 u32 unknown (always 0 observed)
//!   0x08 u32 timestamp (unix seconds)
//!   0x0C u32 absolute offset of node-name table
//! Body: one root record node at 0x10.
//! Node-name table: u16 count, then count x { u16 len, len bytes ASCII }.
use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub enum Val {
    Bool(bool), I8(i8), I16(i16), I32(i32), I64(i64), U8(u8), U16(u16), U32(u32), U64(u64),
    F32(f32), F64(f64), C2(f32, f32), C3(f32, f32, f32), Utf16(String), Ascii(String), Angle(u16),
    /// array of primitive type `code & 0x3F`; payload bytes [start,end)
    Arr { code: u8, start: u32, end: u32 },
}

#[derive(Clone, Debug)]
pub enum Item { V(Val), Rec(Rec), RecArr(RecArr) }

#[derive(Clone, Debug)]
pub struct Rec { pub name: u16, pub ver: u8, pub off: u32, pub children: Vec<Item> }

#[derive(Clone, Debug)]
pub struct RecArr { pub name: u16, pub ver: u8, pub off: u32, pub items: Vec<Vec<Item>> }

pub struct Esf {
    pub data: Vec<u8>,
    pub magic: u32, pub unk: u32, pub timestamp: u32, pub names_off: u32,
    pub names: Vec<String>,
    pub trailing: usize,
    pub root: Rec,
}

pub fn type_name(code: u8) -> &'static str {
    match code {
        0x01 => "bool", 0x02 => "i8", 0x03 => "i16", 0x04 => "i32", 0x05 => "i64",
        0x06 => "u8", 0x07 => "u16", 0x08 => "u32", 0x09 => "u64", 0x0A => "f32", 0x0B => "f64",
        0x0C => "coord2d", 0x0D => "coord3d", 0x0E => "utf16", 0x0F => "ascii", 0x10 => "angle",
        0x80 => "record", 0x81 => "record_array",
        c if c & 0x40 != 0 => "array", _ => "?",
    }
}
pub fn elem_size(code: u8) -> Option<usize> {
    Some(match code & 0x3F {
        0x01 | 0x02 | 0x06 => 1, 0x03 | 0x07 | 0x10 => 2, 0x04 | 0x08 | 0x0A => 4,
        0x05 | 0x09 | 0x0B | 0x0C => 8, 0x0D => 12, _ => return None,
    })
}

struct Cur<'a> { b: &'a [u8], p: usize }
type R<T> = Result<T, String>;
impl<'a> Cur<'a> {
    fn need(&self, n: usize) -> R<()> { if self.p + n > self.b.len() { Err(format!("eof at 0x{:x}", self.p)) } else { Ok(()) } }
    fn u8(&mut self) -> R<u8> { self.need(1)?; let v = self.b[self.p]; self.p += 1; Ok(v) }
    fn u16(&mut self) -> R<u16> { self.need(2)?; let v = u16::from_le_bytes([self.b[self.p], self.b[self.p+1]]); self.p += 2; Ok(v) }
    fn u32(&mut self) -> R<u32> { self.need(4)?; let v = u32::from_le_bytes(self.b[self.p..self.p+4].try_into().unwrap()); self.p += 4; Ok(v) }
    fn u64(&mut self) -> R<u64> { self.need(8)?; let v = u64::from_le_bytes(self.b[self.p..self.p+8].try_into().unwrap()); self.p += 8; Ok(v) }
    fn f32(&mut self) -> R<f32> { Ok(f32::from_bits(self.u32()?)) }
    fn utf16(&mut self) -> R<String> { let n = self.u16()? as usize; self.need(n*2)?;
        let u: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([self.b[self.p+2*i], self.b[self.p+2*i+1]])).collect();
        self.p += n*2; Ok(String::from_utf16_lossy(&u)) }
    fn ascii(&mut self) -> R<String> { let n = self.u16()? as usize; self.need(n)?;
        let s = self.b[self.p..self.p+n].iter().map(|&c| c as char).collect(); self.p += n; Ok(s) }
}

fn read_prim(c: &mut Cur, code: u8) -> R<Val> {
    Ok(match code {
        0x01 => Val::Bool(c.u8()? != 0), 0x02 => Val::I8(c.u8()? as i8), 0x03 => Val::I16(c.u16()? as i16),
        0x04 => Val::I32(c.u32()? as i32), 0x05 => Val::I64(c.u64()? as i64), 0x06 => Val::U8(c.u8()?),
        0x07 => Val::U16(c.u16()?), 0x08 => Val::U32(c.u32()?), 0x09 => Val::U64(c.u64()?),
        0x0A => Val::F32(c.f32()?), 0x0B => Val::F64(f64::from_bits(c.u64()?)),
        0x0C => Val::C2(c.f32()?, c.f32()?), 0x0D => Val::C3(c.f32()?, c.f32()?, c.f32()?),
        0x0E => Val::Utf16(c.utf16()?), 0x0F => Val::Ascii(c.ascii()?), 0x10 => Val::Angle(c.u16()?),
        _ => return Err(format!("unknown type code 0x{:02x} at 0x{:x}", code, c.p - 1)),
    })
}

fn read_item(c: &mut Cur) -> R<Item> {
    let code = c.u8()?;
    match code {
        0x80 => {
            let off = (c.p - 1) as u32;
            let name = c.u16()?; let ver = c.u8()?; let end = c.u32()? as usize;
            let children = read_children(c, end)?;
            Ok(Item::Rec(Rec { name, ver, off, children }))
        }
        0x81 => {
            let off = (c.p - 1) as u32;
            let name = c.u16()?; let ver = c.u8()?; let end = c.u32()? as usize; let count = c.u32()?;
            let mut items = Vec::with_capacity(count as usize);
            for _ in 0..count { let iend = c.u32()? as usize; items.push(read_children(c, iend)?); }
            if c.p != end { return Err(format!("record_array @0x{:x} ended at 0x{:x} expected 0x{:x}", off, c.p, end)); }
            Ok(Item::RecArr(RecArr { name, ver, off, items }))
        }
        c2 if c2 & 0xC0 == 0x40 => {
            let end = c.u32()?; let start = c.p as u32;
            if end as usize > c.b.len() || (end as usize) < c.p { return Err(format!("bad array end at 0x{:x}", start)); }
            c.p = end as usize;
            Ok(Item::V(Val::Arr { code: c2, start, end }))
        }
        _ => Ok(Item::V(read_prim(c, code)?)),
    }
}

fn read_children(c: &mut Cur, end: usize) -> R<Vec<Item>> {
    let mut v = Vec::new();
    while c.p < end { v.push(read_item(c)?); }
    if c.p != end { return Err(format!("overran block end 0x{:x} (at 0x{:x})", end, c.p)); }
    Ok(v)
}

impl Esf {
    pub fn parse(data: Vec<u8>) -> R<Esf> {
        let (magic, unk, timestamp, names_off, root, names, trailing);
        {
            let mut c = Cur { b: &data, p: 0 };
            magic = c.u32()?; unk = c.u32()?; timestamp = c.u32()?; names_off = c.u32()?;
            if magic != 0xABCE { return Err(format!("magic 0x{:x} not ABCE", magic)); }
            root = match read_item(&mut c)? { Item::Rec(r) => r, _ => return Err("root not record".into()) };
            if c.p != names_off as usize { return Err(format!("root ended 0x{:x} != names 0x{:x}", c.p, names_off)); }
            let n = c.u16()?;
            let mut nm = Vec::new();
            for _ in 0..n { nm.push(c.ascii()?); }
            names = nm;
            trailing = data.len() - c.p;
        }
        Ok(Esf { data, magic, unk, timestamp, names_off, names, trailing, root })
    }
    pub fn name(&self, i: u16) -> &str { self.names.get(i as usize).map(|s| s.as_str()).unwrap_or("?") }

    /// Decode a primitive array into display strings (up to `max` elements). Returns (count, shown).
    pub fn arr_elems(&self, code: u8, start: u32, end: u32, max: usize) -> (usize, Vec<String>) {
        let b = &self.data[start as usize..end as usize];
        let base = code & 0x3F;
        let mut out = Vec::new();
        if let Some(sz) = elem_size(base) {
            let n = b.len() / sz;
            let mut c = Cur { b, p: 0 };
            for _ in 0..n.min(max) { out.push(fmt_val(&read_prim(&mut c, base).unwrap())); }
            (n, out)
        } else {
            let mut c = Cur { b, p: 0 }; let mut n = 0;
            while c.p < b.len() {
                let v = if base == 0x0E { c.utf16() } else { c.ascii() };
                match v { Ok(s) => { if out.len() < max { out.push(format!("{:?}", s)); } n += 1; } Err(_) => break }
            }
            (n, out)
        }
    }
    /// Raw primitive values of an array as f64 (numeric types only).
    pub fn arr_nums(&self, code: u8, start: u32, end: u32) -> Vec<f64> {
        let b = &self.data[start as usize..end as usize];
        let base = code & 0x3F;
        let mut out = Vec::new();
        if let Some(sz) = elem_size(base) {
            let mut c = Cur { b, p: 0 };
            for _ in 0..b.len() / sz {
                match read_prim(&mut c, base).unwrap() {
                    Val::Bool(x) => out.push(x as u8 as f64), Val::I8(x) => out.push(x as f64), Val::I16(x) => out.push(x as f64),
                    Val::I32(x) => out.push(x as f64), Val::I64(x) => out.push(x as f64), Val::U8(x) => out.push(x as f64),
                    Val::U16(x) => out.push(x as f64), Val::U32(x) => out.push(x as f64), Val::U64(x) => out.push(x as f64),
                    Val::F32(x) => out.push(x as f64), Val::F64(x) => out.push(x), Val::Angle(x) => out.push(x as f64),
                    Val::C2(a, b2) => { out.push(a as f64); out.push(b2 as f64); }
                    Val::C3(a, b2, c3) => { out.push(a as f64); out.push(b2 as f64); out.push(c3 as f64); }
                    _ => {}
                }
            }
        }
        out
    }
    pub fn arr_strings(&self, code: u8, start: u32, end: u32) -> Vec<String> {
        let b = &self.data[start as usize..end as usize];
        let mut c = Cur { b, p: 0 }; let mut v = Vec::new();
        while c.p < b.len() { match if code & 0x3F == 0x0E { c.utf16() } else { c.ascii() } { Ok(s) => v.push(s), Err(_) => break } }
        v
    }
}

pub fn fmt_val(v: &Val) -> String {
    match v {
        Val::Bool(x) => format!("{}", x), Val::I8(x) => format!("{}", x), Val::I16(x) => format!("{}", x),
        Val::I32(x) => format!("{}", x), Val::I64(x) => format!("{}", x), Val::U8(x) => format!("{}", x),
        Val::U16(x) => format!("{}", x), Val::U32(x) => format!("{}", x), Val::U64(x) => format!("{}", x),
        Val::F32(x) => format!("{}", x), Val::F64(x) => format!("{}", x), Val::C2(a, b) => format!("({}, {})", a, b),
        Val::C3(a, b, c) => format!("({}, {}, {})", a, b, c), Val::Utf16(s) => format!("u{:?}", s),
        Val::Ascii(s) => format!("a{:?}", s), Val::Angle(x) => format!("angle {}", x),
        Val::Arr { code, start, end } => format!("{}[] {} bytes", type_name(code & 0x3F), end - start),
    }
}
pub fn val_code(v: &Val) -> u8 {
    match v { Val::Bool(_) => 1, Val::I8(_) => 2, Val::I16(_) => 3, Val::I32(_) => 4, Val::I64(_) => 5, Val::U8(_) => 6,
        Val::U16(_) => 7, Val::U32(_) => 8, Val::U64(_) => 9, Val::F32(_) => 10, Val::F64(_) => 11, Val::C2(..) => 12,
        Val::C3(..) => 13, Val::Utf16(_) => 14, Val::Ascii(_) => 15, Val::Angle(_) => 16, Val::Arr { code, .. } => *code }
}

/// Short type signature for a child list, e.g. "u32,utf16,{FOO},[BAR]".
pub fn sig(e: &Esf, items: &[Item]) -> String {
    let mut s = String::new();
    for (i, it) in items.iter().enumerate() {
        if i > 0 { s.push(','); }
        match it {
            Item::V(v) => { let c = val_code(v); if c & 0x40 != 0 { let _ = write!(s, "{}[]", type_name(c & 0x3F)); } else { s.push_str(type_name(c)); } }
            Item::Rec(r) => { let _ = write!(s, "{{{}}}", e.name(r.name)); }
            Item::RecArr(r) => { let _ = write!(s, "[{}]", e.name(r.name)); }
        }
    }
    s
}

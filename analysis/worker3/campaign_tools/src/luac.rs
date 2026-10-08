//! Lua 5.1 bytecode ("\x1bLuaQ") reader for the UI .luac files.
//! Header observed in every file: 1b 4c 75 61 | 51 | 00 | 01 | 04 04 04 04 | 00
//!   version 0x51, format 0, little-endian, sizeof(int)=4, sizeof(size_t)=4,
//!   sizeof(Instruction)=4, sizeof(lua_Number)=4 (!), integral flag 0 => lua_Number is f32.
//! Call-site extraction is a reconstructed register-tracking heuristic (labelled INFERRED).
use crate::pack;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub enum K { Nil, Bool(bool), Num(f32), Str(String) }

pub struct Proto { pub source: String, pub line: i32, pub code: Vec<u32>, pub k: Vec<K>, pub protos: Vec<Proto>, pub upvals: Vec<String>, pub nparams: u8 }

struct C<'a> { b: &'a [u8], p: usize }
impl<'a> C<'a> {
    fn u8(&mut self) -> Result<u8, String> { let v = *self.b.get(self.p).ok_or("eof")?; self.p += 1; Ok(v) }
    fn i32(&mut self) -> Result<i32, String> { if self.p + 4 > self.b.len() { return Err("eof".into()); } let v = i32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap()); self.p += 4; Ok(v) }
    fn string(&mut self) -> Result<String, String> { let n = self.i32()? as usize; if n == 0 { return Ok(String::new()); } if self.p + n > self.b.len() { return Err("eof str".into()); }
        let s: String = self.b[self.p..self.p + n - 1].iter().map(|&c| c as char).collect(); self.p += n; Ok(s) }
    fn proto(&mut self) -> Result<Proto, String> {
        let source = self.string()?; let line = self.i32()?; let _last = self.i32()?;
        let _nups = self.u8()?; let nparams = self.u8()?; let _va = self.u8()?; let _ms = self.u8()?;
        let n = self.i32()? as usize; let mut code = Vec::with_capacity(n); for _ in 0..n { code.push(self.i32()? as u32); }
        let n = self.i32()? as usize; let mut k = Vec::with_capacity(n);
        for _ in 0..n { let t = self.u8()?; k.push(match t { 0 => K::Nil, 1 => K::Bool(self.u8()? != 0), 3 => K::Num(f32::from_bits(self.i32()? as u32)), 4 => K::Str(self.string()?), x => return Err(format!("bad const type {} at {}", x, self.p)) }); }
        let n = self.i32()? as usize; let mut protos = Vec::new(); for _ in 0..n { protos.push(self.proto()?); }
        let n = self.i32()? as usize; for _ in 0..n { self.i32()?; }
        let n = self.i32()? as usize; for _ in 0..n { self.string()?; self.i32()?; self.i32()?; }
        let n = self.i32()? as usize; let mut upvals = Vec::new(); for _ in 0..n { upvals.push(self.string()?); }
        Ok(Proto { source, line, code, k, protos, upvals, nparams })
    }
}

pub fn parse(b: &[u8]) -> Result<(Vec<u8>, Proto), String> {
    if b.len() < 12 || &b[0..4] != b"\x1bLua" { return Err("not luac".into()); }
    let hdr = b[4..12].to_vec();
    let mut c = C { b, p: 12 };
    let p = c.proto()?;
    if c.p != b.len() { return Err(format!("trailing {} bytes", b.len() - c.p)); }
    Ok((hdr, p))
}

#[derive(Default)]
pub struct Agg { pub calls: BTreeMap<String, (usize, BTreeSet<String>)>, pub strings: BTreeMap<String, usize>, pub events: BTreeMap<String, usize>, pub globals: BTreeMap<String, usize> }

fn kstr(p: &Proto, idx: u32) -> Option<String> { if let Some(K::Str(s)) = p.k.get(idx as usize) { Some(s.clone()) } else { None } }
fn rk(p: &Proto, regs: &[Option<String>], x: u32) -> Option<String> { if x & 0x100 != 0 { kstr(p, x & 0xFF) } else { regs.get(x as usize).cloned().flatten().map(|_| String::from("?")) } }

pub fn walk_pub(p: &Proto, file: &str, agg: &mut Agg) { walk(p, file, agg) }
fn walk(p: &Proto, file: &str, agg: &mut Agg) {
    for k in &p.k { if let K::Str(s) = k { *agg.strings.entry(s.clone()).or_default() += 1; } }
    let mut regs: Vec<Option<String>> = vec![None; 256];
    for &ins in &p.code {
        let op = ins & 0x3F; let a = ((ins >> 6) & 0xFF) as usize; let c = (ins >> 14) & 0x1FF; let b = (ins >> 23) & 0x1FF; let bx = ins >> 14;
        match op {
            0 => regs[a] = regs[b as usize].clone(),
            4 => regs[a] = p.upvals.get(b as usize).cloned(),
            5 => { let g = kstr(p, bx); if let Some(ref s) = g { *agg.globals.entry(s.clone()).or_default() += 1; } regs[a] = g; }
            6 => { let base = regs[b as usize].clone(); let key = if c & 0x100 != 0 { kstr(p, c & 0xFF) } else { None };
                regs[a] = match (base, key) { (Some(bn), Some(k)) => { if bn == "events" { *agg.events.entry(k.clone()).or_default() += 1; } Some(format!("{}.{}", bn, k)) } (None, Some(k)) => Some(format!("?.{}", k)), _ => None }; }
            11 => { let base = regs[b as usize].clone(); let key = rk(p, &regs, c);
                regs[a + 1] = base.clone(); regs[a] = Some(format!("{}:{}", base.unwrap_or("?".into()), key.unwrap_or("?".into()))); }
            28 | 29 => { if let Some(n) = regs[a].clone() { let e = agg.calls.entry(n).or_default(); e.0 += 1; e.1.insert(file.to_string()); } for r in regs.iter_mut().skip(a) { *r = None; } }
            7 | 8 | 9 | 22..=27 | 30 | 31 | 33 | 34 | 35 => {}
            _ => { if a < regs.len() { regs[a] = None; } }
        }
    }
    for c in &p.protos { walk(c, file, agg); }
}

pub fn run(a: &[String]) {
    let mode = a.first().map(|s| s.as_str()).unwrap_or("calls");
    let p = pack::Pack::open(pack::data_path("data.pack")).unwrap();
    let mut agg = Agg::default();
    let mut hdrs: BTreeMap<String, usize> = BTreeMap::new();
    let mut sources: BTreeMap<String, usize> = BTreeMap::new();
    let (mut ok, mut bad) = (0, 0);
    for e in &p.entries {
        if !e.path.to_ascii_lowercase().ends_with(".luac") { continue; }
        let b = p.read(e).unwrap();
        match parse(&b) {
            Ok((h, pr)) => { ok += 1; *hdrs.entry(h.iter().map(|x| format!("{:02x}", x)).collect::<Vec<_>>().join(" ")).or_default() += 1;
                let dir = pr.source.rsplitn(2, '/').nth(1).unwrap_or("").to_string(); *sources.entry(dir).or_default() += 1; let _ = pr.line; let _ = pr.nparams;
                walk(&pr, &e.path, &mut agg); }
            Err(m) => { bad += 1; println!("ERR {} {}", e.path, m); }
        }
    }
    match mode {
        "summary" => { println!("luac files ok={} bad={}", ok, bad); for (h, c) in &hdrs { println!("header[4..12] {} x{}", h, c); } for (s, c) in &sources { println!("source dir {} x{}", s, c); } }
        "globals" => { for (k, c) in &agg.globals { println!("{}\t{}", k, c); } }
        "events" => { for (k, c) in &agg.events { println!("{}\t{}", k, c); } }
        "strings" => { for (k, c) in &agg.strings { println!("{}\t{}", k, c); } }
        _ => { for (k, (c, f)) in &agg.calls { println!("{}\t{}\tfiles={}", k, c, f.len()); } }
    }
}

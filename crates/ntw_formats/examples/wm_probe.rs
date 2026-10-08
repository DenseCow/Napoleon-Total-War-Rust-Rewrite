//! Research probe: which `.variant_weighted_mesh` files fail to parse, and why (read-only).
//! `cargo run -p ntw_formats --example wm_probe [-- hex <path substring>]`
use ntw_formats::pack::Vfs;
use ntw_formats::weighted_mesh::{WeightedMesh, WeightedMeshError};

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let paths: Vec<String> = vfs.list("").into_iter().filter(|p| p.ends_with(".variant_weighted_mesh")).map(str::to_owned).collect();
    let (mut ok, mut bad) = (0, 0);
    for p in &paths {
        let b = vfs.read(p).unwrap();
        match WeightedMesh::read(&b) {
            Ok(_) => ok += 1,
            Err(e) if p.starts_with("testdata") => { bad += 1; let mut hits = Vec::new(); for named in [false, true] { for pre in [0, 6] { for post in [0, 4] { if legacy2(&b, named, pre, post).is_ok() { hits.push((named, pre, post)); } for isz in [2, 4] { if table_layout(&b, pre, post, named, isz).is_ok() { hits.push((true, 100 * isz + pre, post)); } } } } } println!("{p}: {e}; layouts {hits:?}"); }
            Err(e) => {
                bad += 1;
                println!("{p} ({} bytes): {e}", b.len());
                if args.first().map(String::as_str) == Some("hex") && p.contains(args[1].as_str()) {
                    let at = match e {
                        WeightedMeshError::UnknownTrailer { offset } | WeightedMeshError::TrailingBytes { offset, .. } => offset,
                        _ => 0,
                    };
                    for row in b[at..(at + 512).min(b.len())].chunks(16) {
                        println!("   {}", row.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" "));
                    }
                }
            }
        }
    }
    if args.first().map(String::as_str) == Some("walk") { walk_first(&vfs.read(&args[1]).unwrap()); walk_all(&vfs.read(&args[1]).unwrap()); }
    println!("{ok} ok, {bad} failed");
}

/// Hypothesis test for the testdata layout: u32 a, u32 b, u32 c, then pieces
/// {u32 V, V x (uv2, u32 n, n x (u32 bone, f32 x7)), u32 I, I x u32}.
/// `legacy`'s result: the three header words and the pieces' (vertex, index) counts.
type Legacy = (u32, u32, u32, Vec<(usize, usize)>);

#[allow(dead_code)]
pub fn legacy(b: &[u8]) -> Result<Legacy, String> {
    let rd = |o: usize| -> Result<u32, String> { b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(format!("eof at {o}")) };
    let (a, bb, c) = (rd(0)?, rd(4)?, rd(8)?);
    let mut o = 12;
    let mut pieces = Vec::new();
    while o < b.len() {
        let v = rd(o)? as usize;
        o += 4;
        if v > b.len() { return Err(format!("V {v} at {}", o - 4)); }
        for _ in 0..v {
            o += 8;
            let n = rd(o)? as usize;
            o += 4;
            if n > 8 { return Err(format!("influences {n} at {}", o - 4)); }
            o += n * 32;
        }
        let i = rd(o)? as usize;
        o += 4;
        if i > b.len() { return Err(format!("I {i} at {}", o - 4)); }
        o += i * 4;
        pieces.push((v, i));
    }
    if o != b.len() { return Err(format!("ends at {o} of {}", b.len())); }
    Ok((a, bb, c, pieces))
}

/// Variant hypothesis: header (u32 pieces, u32 total V, u32 total I), optional piece names
/// (utf16 before each piece), vertex = uv2 + `pre` floats + influences + `post` floats.
#[allow(dead_code)]
pub fn legacy2(b: &[u8], named: bool, pre: usize, post: usize) -> Result<usize, String> {
    let rd = |o: usize| -> Result<u32, String> { b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(format!("eof at {o}")) };
    let n = rd(0)? as usize;
    let mut o = 12;
    for _ in 0..n {
        if named {
            let l = b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or("eof")? as usize;
            o += 2 + 2 * l;
        }
        let v = rd(o)? as usize;
        o += 4;
        if v > b.len() { return Err(format!("V {v} at {}", o - 4)); }
        for _ in 0..v {
            o += 8 + 4 * pre;
            let k = rd(o)? as usize;
            o += 4;
            if k > 32 { return Err(format!("influences {k} at {}", o - 4)); }
            o += k * 32 + 4 * post;
        }
        let i = rd(o)? as usize;
        o += 4;
        if i > b.len() { return Err(format!("I {i} at {}", o - 4)); }
        o += i * 4;
    }
    if o != b.len() { return Err(format!("ends at {o} of {}", b.len())); }
    Ok(n)
}

/// Headerless with a piece table: u32 n, n x {utf16, u32 V, u32 I}, pieces, optional u32 trailer.
#[allow(dead_code)]
pub fn table_layout(b: &[u8], pre: usize, post: usize, trailer: bool, isz: usize) -> Result<usize, String> {
    let rd = |o: usize| -> Result<u32, String> { b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(format!("eof at {o}")) };
    let n = rd(0)? as usize;
    let mut o = 4;
    for _ in 0..n {
        let l = b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or("eof")? as usize;
        o += 2 + 2 * l + 8;
    }
    for _ in 0..n {
        let v = rd(o)? as usize;
        o += 4;
        if v > b.len() { return Err(format!("V {v}")); }
        for _ in 0..v {
            o += 8 + 4 * pre;
            let k = rd(o)? as usize;
            o += 4;
            if k > 32 { return Err(format!("influences {k} at {}", o - 4)); }
            o += k * 32 + 4 * post;
        }
        let i = rd(o)? as usize;
        o += 4 + i * isz;
    }
    if trailer { o += 4; }
    if o != b.len() { return Err(format!("ends at {o} of {}", b.len())); }
    Ok(n)
}

/// Walks the first piece of a table-layout file and prints where its vertices end.
#[allow(dead_code)]
pub fn walk_first(b: &[u8]) {
    let rd = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let n = rd(0) as usize;
    let mut o = 4;
    for _ in 0..n {
        let l = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
        o += 2 + 2 * l + 8;
    }
    let v = rd(o) as usize;
    o += 4;
    for k in 0..v {
        o += 8 + 24;
        let inf = rd(o) as usize;
        if k < 3 || k == v - 1 { println!("  vertex {k} influences {inf} at {o}"); }
        o += 4 + inf * 32;
    }
    println!("  after {v} vertices at {o}: {:?}", &b[o..o + 32]);
}

/// Walks all pieces of a table-layout file (pre = 6 floats), checking counts against the table.
#[allow(dead_code)]
pub fn walk_all(b: &[u8]) {
    let rd = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let n = rd(0) as usize;
    let mut o = 4;
    let mut toc = Vec::new();
    for _ in 0..n {
        let l = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
        let name = String::from_utf16_lossy(&b[o + 2..o + 2 + 2 * l].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>());
        o += 2 + 2 * l;
        toc.push((name, rd(o), rd(o + 4)));
        o += 8;
    }
    for (name, tv, ti) in &toc {
        let v = rd(o);
        println!("  piece {name} table V {tv} I {ti} at {o}: V {v}");
        if v != *tv { println!("  mismatch: next bytes {:?}", &b[o..o + 24]); return; }
        o += 4;
        for _ in 0..v { o += 32; let inf = rd(o) as usize; o += 4 + inf * 32; }
        let i = rd(o);
        if i != *ti { println!("  I mismatch {i}: next {:?}", &b[o..o + 24]); return; }
        o += 4 + 4 * i as usize;
    }
    println!("  end at {o} of {}: {:?}", b.len(), &b[o..(o + 24).min(b.len())]);
}

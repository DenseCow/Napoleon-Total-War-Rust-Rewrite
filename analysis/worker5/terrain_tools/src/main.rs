//! terrain_tools: read-only research tool for Napoleon: Total War battlefield terrain.
//!
//! Reads packs IN PLACE through `ntw_formats` and prints small statistics. It never writes
//! extracted assets anywhere.
//!
//! Usage: `terrain_tools <command> [args]`. Set `NTW_DATA` to override the install data dir.

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::{PackFile, Vfs};

const DEFAULT_DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> String {
    std::env::var("NTW_DATA").unwrap_or_else(|_| DEFAULT_DATA.to_string())
}

fn open_pack(name: &str) -> PackFile {
    let p = std::path::Path::new(&data_dir()).join(name);
    PackFile::open(&p).unwrap_or_else(|e| panic!("open {}: {e}", p.display()))
}

fn ext_of(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match file.find('.') {
        Some(i) => file[i..].to_ascii_lowercase(),
        None => "(none)".into(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match cmd {
        "inventory" => inventory(&a(1), args.get(2).map(|s| s.parse().unwrap()).unwrap_or(2)),
        "ls" => ls(&a(1), &a(2), args.get(3).map(|s| s.parse().unwrap()).unwrap_or(200)),
        "hex" => hex(&a(1), &a(2), args.get(3).map(|s| s.parse().unwrap()).unwrap_or(256)),
        "esf" => esf_dump(&a(1), &a(2), args.get(3).map(|s| s.parse().unwrap()).unwrap_or(4)),
        "vfsfind" => vfs_find(&a(1)),
        "l16" => l16_stats(&a(1), &a(2)),
        "hexf" => hexf(&a(1), args.get(2).map(|s| s.parse().unwrap()).unwrap_or(256), args.get(3).map(|s| s.parse().unwrap()).unwrap_or(0)),
        "spln-survey" => spln_survey(&args[1..]),
        "stp" => stp(&a(1)),
        "l16-all" => l16_all(&a(1)),
        _ => eprintln!("commands: inventory <pack> [depth] | ls <pack> <substr> [max] | hex <pack> <path> [n] | esf <pack> <path> [depth] | vfsfind <substr>"),
    }
}

/// Counts and sizes per extension, and per folder prefix of `depth` components.
fn inventory(pack: &str, depth: usize) {
    let p = open_pack(pack);
    let h = p.header();
    println!("pack {pack}: type {:?}, {} files, deps {:?}", h.pack_type, h.file_count, h.dependencies);
    let mut by_ext: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut by_dir: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for e in p.entries() {
        let x = by_ext.entry(ext_of(&e.path)).or_default();
        x.0 += 1;
        x.1 += e.size as u64;
        let parts: Vec<&str> = e.path.split(['/', '\\']).collect();
        let n = depth.min(parts.len().saturating_sub(1));
        let d = by_dir.entry(parts[..n].join("/")).or_default();
        d.0 += 1;
        d.1 += e.size as u64;
    }
    println!("\n== by extension (count, MB) ==");
    for (k, (c, s)) in &by_ext {
        println!("{k:40} {c:7} {:10.1}", *s as f64 / 1e6);
    }
    println!("\n== by folder (depth {depth}) ==");
    for (k, (c, s)) in &by_dir {
        println!("{k:70} {c:7} {:10.1}", *s as f64 / 1e6);
    }
}

fn ls(pack: &str, sub: &str, max: usize) {
    let p = open_pack(pack);
    let sub = sub.to_ascii_lowercase();
    let mut n = 0;
    for e in p.entries() {
        if e.path.to_ascii_lowercase().contains(&sub) {
            println!("{:10} {}", e.size, e.path);
            n += 1;
            if n >= max {
                break;
            }
        }
    }
    let total = p.entries().iter().filter(|e| e.path.to_ascii_lowercase().contains(&sub)).count();
    println!("({n} shown of {total})");
}

fn read(pack: &str, path: &str) -> Vec<u8> {
    let p = open_pack(pack);
    let e = p
        .entries()
        .iter()
        .find(|e| e.path.eq_ignore_ascii_case(path) || e.path.replace('\\', "/").eq_ignore_ascii_case(path))
        .unwrap_or_else(|| panic!("not found: {path}"));
    p.read_entry(e).unwrap()
}

fn hex(pack: &str, path: &str, n: usize) {
    let b = read(pack, path);
    println!("{} bytes", b.len());
    for (i, chunk) in b[..n.min(b.len())].chunks(16).enumerate() {
        let hexs: Vec<String> = chunk.iter().map(|x| format!("{x:02x}")).collect();
        let asc: String = chunk.iter().map(|&x| if (32..127).contains(&x) { x as char } else { '.' }).collect();
        println!("{:08x}  {:48} {}", i * 16, hexs.join(" "), asc);
    }
}

fn summarize(n: &EsfNode) -> String {
    match n {
        EsfNode::Record(r) => format!("REC {} v{} ({} children)", r.name, r.version, r.children.len()),
        EsfNode::RecordArray(a) => format!("ARR {} v{} ({} items)", a.name, a.version, a.items.len()),
        EsfNode::U8Array(v) => format!("u8[{}]", v.len()),
        EsfNode::U16Array(v) => format!("u16[{}]", v.len()),
        EsfNode::U32Array(v) => format!("u32[{}] {:?}", v.len(), &v[..v.len().min(6)]),
        EsfNode::I32Array(v) => format!("i32[{}] {:?}", v.len(), &v[..v.len().min(6)]),
        EsfNode::F32Array(v) => format!("f32[{}] {:?}", v.len(), &v[..v.len().min(6)]),
        EsfNode::Coord2dArray(v) => format!("c2d[{}] {:?}", v.len(), &v[..v.len().min(3)]),
        EsfNode::Coord3dArray(v) => format!("c3d[{}] {:?}", v.len(), &v[..v.len().min(3)]),
        EsfNode::BoolArray(v) => format!("bool[{}]", v.len()),
        other => format!("{other:?}"),
    }
}

fn dump_rec(r: &EsfRecord, depth: usize, max_depth: usize, max_children: usize) {
    let pad = "  ".repeat(depth);
    for (i, c) in r.children.iter().enumerate() {
        if i >= max_children {
            println!("{pad}... ({} more)", r.children.len() - i);
            break;
        }
        println!("{pad}{}", summarize(c));
        if depth + 1 < max_depth {
            match c {
                EsfNode::Record(sub) => dump_rec(sub, depth + 1, max_depth, max_children),
                EsfNode::RecordArray(arr) => {
                    for (k, item) in arr.items.iter().enumerate().take(2) {
                        println!("{pad}  [{k}]");
                        let tmp = EsfRecord { name: String::new(), version: 0, children: item.clone() };
                        dump_rec(&tmp, depth + 2, max_depth + 1, max_children);
                    }
                }
                _ => {}
            }
        }
    }
}

fn esf_dump(pack: &str, path: &str, depth: usize) {
    let b = read(pack, path);
    let f = EsfFile::from_bytes(&b).unwrap();
    println!("ESF magic {:#x}, root {} v{}", f.header.magic, f.root.name, f.root.version);
    dump_rec(&f.root, 0, depth, 24);
}

fn vfs_find(sub: &str) {
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let mut hits = vfs.list("");
    hits.retain(|p| p.contains(&sub.to_ascii_lowercase()));
    for h in hits.iter().take(200) {
        println!("{h}");
    }
    println!("({} hits)", hits.len());
}

/// Min / max / distinct count of an uncompressed 16-bit luminance DDS (heightfield).
/// Returns (width, height, min, max) or None when the file is not L16.
fn l16_info(b: &[u8]) -> Option<(u32, u32, u16, u16)> {
    let u = |p: usize| u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
    if b.len() < 128 || &b[0..4] != b"DDS " || u(0x58) != 16 || u(0x5C) != 0xFFFF {
        return None;
    }
    let (h, w) = (u(0x0C), u(0x10));
    let n = (w * h) as usize;
    let px = &b[128..128 + 2 * n];
    let (mut lo, mut hi) = (u16::MAX, 0u16);
    for c in px.chunks_exact(2) {
        let v = u16::from_le_bytes([c[0], c[1]]);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    Some((w, h, lo, hi))
}

fn l16_stats(pack: &str, path: &str) {
    let b = read(pack, path);
    match l16_info(&b) {
        Some((w, h, lo, hi)) => println!("{w}x{h} min {lo} max {hi}"),
        None => println!("not an L16 DDS"),
    }
}

/// For every L16 DDS in a pack: size histogram and how many use the full 0..65535 range.
fn l16_all(pack: &str) {
    let p = open_pack(pack);
    let mut dims: BTreeMap<String, usize> = BTreeMap::new();
    let (mut full, mut total) = (0, 0);
    for e in p.entries().iter().filter(|e| e.path.to_ascii_lowercase().ends_with(".dds")) {
        let b = p.read_entry(e).unwrap();
        if let Some((w, h, lo, hi)) = l16_info(&b) {
            total += 1;
            if lo == 0 && hi == u16::MAX {
                full += 1;
            }
            let name = e.path.rsplit(['/', '\\']).next().unwrap().to_ascii_lowercase();
            *dims.entry(format!("{w}x{h} {name}")).or_default() += 1;
        }
    }
    println!("L16 DDS: {total}, using the full 0..65535 range: {full}");
    for (k, n) in dims {
        println!("{n:6} {k}");
    }
}

/// Hex dump of a LOOSE install file (read-only), `n` bytes from `skip`.
fn hexf(path: &str, n: usize, skip: usize) {
    let b = std::fs::read(path).unwrap();
    println!("{} bytes", b.len());
    let end = (skip + n).min(b.len());
    for (i, chunk) in b[skip.min(end)..end].chunks(16).enumerate() {
        let hexs: Vec<String> = chunk.iter().map(|x| format!("{x:02x}")).collect();
        let asc: String = chunk.iter().map(|&x| if (32..127).contains(&x) { x as char } else { '.' }).collect();
        println!("{:08x}  {:48} {}", skip + i * 16, hexs.join(" "), asc);
    }
}

/// `.rigid_spline` ("SPLN") parse:
/// "SPLN" u32 version, u32 spline_count, then per spline:
/// u16 n + n UTF-16 name, u32 flag, u32 point_count, point_count x f32[3].
/// Returns (version, [(name, flag, points)]) or an error string. Strict: must end at EOF.
fn parse_spln(b: &[u8]) -> Result<(u32, Vec<(String, u32, Vec<[f32; 3]>)>), String> {
    let u = |p: usize| -> Result<u32, String> {
        b.get(p..p + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or("short".to_string())
    };
    if b.len() < 12 || &b[0..4] != b"SPLN" {
        return Err("bad magic".into());
    }
    let version = u(4)?;
    let count = u(8)? as usize;
    let mut p = 12;
    let mut out = vec![];
    for _ in 0..count {
        let n = u16::from_le_bytes(b.get(p..p + 2).ok_or("short")?.try_into().unwrap()) as usize;
        p += 2;
        let units: Vec<u16> = b.get(p..p + 2 * n).ok_or("short")?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        p += 2 * n;
        let flag = u(p)?;
        let np = u(p + 4)? as usize;
        p += 8;
        let mut pts = Vec::with_capacity(np);
        for k in 0..np {
            let f = |o: usize| -> Result<f32, String> { Ok(f32::from_bits(u(p + 12 * k + o)?)) };
            pts.push([f(0)?, f(4)?, f(8)?]);
        }
        p += 12 * np;
        out.push((String::from_utf16_lossy(&units), flag, pts));
    }
    if p != b.len() {
        return Err(format!("{} trailing bytes", b.len() as isize - p as isize));
    }
    Ok((version, out))
}

/// Parse every .rigid_spline under the given LOOSE folders and print counts only.
fn spln_survey(dirs: &[String]) {
    fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rigid_spline")) {
                out.push(p);
            }
        }
    }
    let mut files = vec![];
    for d in dirs {
        walk(std::path::Path::new(d), &mut files);
    }
    let mut ok = 0;
    let mut errs: BTreeMap<String, usize> = BTreeMap::new();
    let mut versions: BTreeMap<u32, usize> = BTreeMap::new();
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    let mut flags: BTreeMap<u32, usize> = BTreeMap::new();
    let mut prefixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut y_zero = 0usize;
    let mut closed = 0usize;
    let (mut pts_total, mut splines) = (0usize, 0usize);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for f in &files {
        let b = std::fs::read(f).unwrap();
        match parse_spln(&b) {
            Ok((v, sp)) => {
                ok += 1;
                *versions.entry(v).or_default() += 1;
                *counts.entry(sp.len()).or_default() += 1;
                for (name, flag, pts) in &sp {
                    splines += 1;
                    pts_total += pts.len();
                    *flags.entry(*flag).or_default() += 1;
                    *prefixes.entry(name.split(':').next().unwrap_or("").to_string()).or_default() += 1;
                    if pts.iter().all(|q| q[1] == 0.0) {
                        y_zero += 1;
                    }
                    if pts.len() > 1 && pts.first() == pts.last() {
                        closed += 1;
                    }
                    for q in pts {
                        for k in 0..3 {
                            lo[k] = lo[k].min(q[k]);
                            hi[k] = hi[k].max(q[k]);
                        }
                    }
                }
            }
            Err(e) => *errs.entry(e).or_default() += 1,
        }
    }
    println!("files {} parsed exactly to EOF {ok}; errors {errs:?}", files.len());
    println!("versions {versions:?}; splines per file {counts:?}; flag values {flags:?}");
    println!("name prefixes {prefixes:?}");
    println!("splines {splines}, points {pts_total}, all-y==0 {y_zero}, first==last {closed}");
    println!("bounds x {:.1}..{:.1}  y {:.2}..{:.2}  z {:.1}..{:.1}", lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
}

/// Campaign supertexture: walk `<dir>/supertexture.stpi` and check every tile against `.stpd`.
/// Layout hypothesis (see CAMPAIGN_MAP_GRAPHICS.md §3):
/// stpi: u32 levels, u32 width, u32 height, u32 tile_size, u32 tile_bytes, u32 (=levels again?),
///       then per level { u32 tiles_x, u32 tiles_y, tiles_x*tiles_y x
///                        { u32 stpd_offset, u32 stpd_size, u32 raw_size, u32 tile_id, u32 flags } }
/// stpd: per tile, chunks { u32 compressed_size, u32 raw_size, zlib stream } summing to raw_size.
fn stp(dir: &str) {
    let i = std::fs::read(format!("{dir}/supertexture.stpi")).unwrap();
    let dlen = std::fs::metadata(format!("{dir}/supertexture.stpd")).unwrap().len();
    let mut d = std::fs::File::open(format!("{dir}/supertexture.stpd")).unwrap();
    let u = |b: &[u8], p: usize| u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
    let levels = u(&i, 0);
    println!("stpi {} bytes: levels {levels}, size {}x{}, tile {}, tile bytes {}", i.len(), u(&i, 4), u(&i, 8), u(&i, 12), u(&i, 16));
    println!("u32 @0x14 = {}", u(&i, 20));
    let mut p = 24;
    let mut flags: BTreeMap<u32, usize> = BTreeMap::new();
    let (mut tiles, mut chunk_ok, mut chunk_bad, mut zlib_hdr, mut covered) = (0, 0, 0, 0, 0u64);
    let mut next_off = 0u64;
    let mut contiguous = true;
    for _ in 0..levels {
        if p + 12 > i.len() {
            println!("ran out of stpi at level header");
            return;
        }
        let (tx, ty) = (u(&i, p), u(&i, p + 4));
        p += 8;
        println!("  level: {tx} x {ty} tiles");
        for _ in 0..tx * ty {
            let (off, size, raw, _id, fl) = (u(&i, p), u(&i, p + 4), u(&i, p + 8), u(&i, p + 12), u(&i, p + 16));
            p += 20;
            tiles += 1;
            *flags.entry(fl).or_default() += 1;
            if off as u64 != next_off {
                contiguous = false;
            }
            next_off = off as u64 + size as u64;
            covered += size as u64;
            // walk the chunks of this tile
            use std::io::{Read, Seek, SeekFrom};
            let mut buf = vec![0u8; size as usize];
            d.seek(SeekFrom::Start(off as u64)).unwrap();
            d.read_exact(&mut buf).unwrap();
            let (mut q, mut raw_sum) = (0usize, 0u32);
            let mut good = true;
            while q + 8 <= buf.len() {
                let (cs, rs) = (u(&buf, q), u(&buf, q + 4));
                if buf.get(q + 8) == Some(&0x78) {
                    zlib_hdr += 1;
                }
                raw_sum += rs;
                q += 8 + cs as usize;
            }
            if q != buf.len() || raw_sum != raw {
                good = false;
            }
            if good { chunk_ok += 1 } else { chunk_bad += 1 }
        }
    }
    println!("stpi parsed to {p} of {} bytes; trailing u32s {:?}", i.len(), (p..i.len()).step_by(4).map(|q| u(&i, q)).collect::<Vec<_>>());
    println!("tiles {tiles}; tiles whose chunks sum to raw_size and end exactly: {chunk_ok}, bad {chunk_bad}; chunks starting with zlib 0x78: {zlib_hdr}");
    println!("tile records contiguous in stpd: {contiguous}; bytes covered {covered} of stpd {dlen}");
    println!("flags field values {flags:?}");
}

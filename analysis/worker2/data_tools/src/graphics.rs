//! Graphics-file surveys: `.unit_variant` (VRNT), `.variant_part_mesh` (VMPF), `.atlas`, `.dds`.
//!
//! Everything here is read-only and prints structure / counts only (no bulk game data).
//! Findings are written up in `analysis/worker2/UNIT_VARIANT_AND_TEXTURES.md`.
//!
//! Subcommands (wired in `main.rs`):
//!   variant <pack> <path>          dump one .unit_variant
//!   variant-survey                 parse every .unit_variant, check every mesh/texture reference resolves
//!   vmpf-head [substr] [n]         print the first u32 words of matching .variant_part_mesh files
//!   vmpf-survey                    check the VMPF header hypothesis on every file
//!   atlas-survey                   parse every .atlas
//!   dds-survey [pack...]           DDS format / size / mip histogram (all packs if none given)
//!   u32s <pack> <substr> [n] [skip] first n u32 words of matching entries (any format)

use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crate::pack::{self, PackIndex};
use crate::variant;

fn u32_at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], p: usize) -> f32 {
    f32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}

/// Lower-case, backslash path, as the pack index stores it.
fn norm(p: &str) -> String {
    p.replace('/', "\\").to_ascii_lowercase()
}

/// Every path in every pack (lower-case) -> pack file name.
fn all_paths() -> io::Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    for p in pack::all_packs()? {
        let pi = PackIndex::open(&p)?;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        for e in &pi.entries {
            m.insert(e.path.to_ascii_lowercase(), name.clone());
        }
    }
    Ok(m)
}

fn bump<K: Ord>(m: &mut BTreeMap<K, usize>, k: K) {
    *m.entry(k).or_default() += 1;
}

fn print_hist<K: std::fmt::Display>(title: &str, m: &BTreeMap<K, usize>, max: usize) {
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    println!("-- {title} ({} distinct)", v.len());
    for (k, n) in v.iter().take(max) {
        println!("  {n:>6}  {k}");
    }
}

pub fn cmd_variant(rest: &[String]) -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve(&rest[0]))?;
    let e = pi.find(&rest[1]).expect("no such entry");
    let b = pi.read(e, None)?;
    match variant::parse(&b) {
        Ok(v) => {
            println!("version={} header_size={} mesh_table_offset={} categories={} meshes={}",
                v.version, v.header_size, v.mesh_table_offset, v.categories.len(), v.meshes.len());
            for c in &v.categories {
                println!("  cat {:<16} index={} unk={} mesh_count={} first_mesh={}", c.name, c.index, c.unk, c.mesh_count, c.first_mesh);
            }
            for (i, m) in v.meshes.iter().enumerate() {
                println!("  mesh[{i}] unk={} {} | {}", m.unk, m.mesh, m.texture);
            }
        }
        Err(e) => println!("parse error: {e}"),
    }
    Ok(())
}

pub fn cmd_variant_survey() -> io::Result<()> {
    let paths = all_paths()?;
    let pi = PackIndex::open(pack::resolve("variantmodels"))?;
    let mut ok = 0usize;
    let mut bad: Vec<String> = vec![];
    let mut versions = BTreeMap::new();
    let mut header_sizes = BTreeMap::new();
    let mut ncat = BTreeMap::new();
    let mut nmesh = BTreeMap::new();
    let mut cat_names = BTreeMap::new();
    let mut cat_unk = BTreeMap::new();
    let mut cat_index_is_position = 0usize;
    let mut cat_index_total = 0usize;
    let mut cat_ranges_ok = 0usize;
    let mut cat_ranges_total = 0usize;
    let mut mesh_unk = BTreeMap::new();
    let mut mesh_resolves = BTreeMap::new();
    let mut tex_resolves = BTreeMap::new();
    let mut tex_suffixes = BTreeMap::new();
    let mut empty_tex = 0usize;
    let mut kind = BTreeMap::new();
    let mut missing_examples: BTreeSet<String> = BTreeSet::new();
    for e in pi.entries.iter().filter(|e| e.path.ends_with(".unit_variant")) {
        let k = e.path.rsplit('\\').next().unwrap();
        let k = k.split('.').nth(1).unwrap_or("?").to_string();
        bump(&mut kind, k);
        let b = pi.read(e, None)?;
        let v = match variant::parse(&b) {
            Ok(v) => v,
            Err(err) => {
                bad.push(format!("{}: {err}", e.path));
                continue;
            }
        };
        ok += 1;
        bump(&mut versions, v.version);
        bump(&mut header_sizes, v.header_size);
        bump(&mut ncat, v.categories.len());
        bump(&mut nmesh, v.meshes.len());
        let mut next = 0u32;
        for (i, c) in v.categories.iter().enumerate() {
            bump(&mut cat_names, c.name.to_ascii_lowercase());
            bump(&mut cat_unk, c.unk);
            cat_index_total += 1;
            if c.index as usize == i {
                cat_index_is_position += 1;
            }
            cat_ranges_total += 1;
            if c.first_mesh == next && (c.first_mesh + c.mesh_count) as usize <= v.meshes.len() {
                cat_ranges_ok += 1;
            }
            next = c.first_mesh + c.mesh_count;
        }
        for m in &v.meshes {
            bump(&mut mesh_unk, m.unk);
            let mp = norm(&m.mesh);
            let mut found = "missing".to_string();
            for ext in [".variant_part_mesh", ".rigid_model", ""] {
                if let Some(pk) = paths.get(&format!("{mp}{ext}")) {
                    found = format!("{pk} +\"{ext}\"");
                    break;
                }
            }
            if found == "missing" && missing_examples.len() < 8 {
                missing_examples.insert(format!("mesh {}", m.mesh));
            }
            bump(&mut mesh_resolves, found);
            if m.texture.is_empty() {
                empty_tex += 1;
                continue;
            }
            let tp = norm(&m.texture);
            // which suffixes exist for this texture stem?
            let mut found: Vec<String> = vec![];
            for (p, _) in paths.range(tp.clone()..) {
                if !p.starts_with(&tp) {
                    break;
                }
                found.push(p[tp.len()..].to_string());
            }
            if found.is_empty() {
                bump(&mut tex_resolves, "missing".to_string());
                if missing_examples.len() < 16 {
                    missing_examples.insert(format!("tex {}", m.texture));
                }
            } else {
                bump(&mut tex_resolves, "found".to_string());
                found.sort();
                bump(&mut tex_suffixes, found.join(" "));
            }
        }
    }
    println!("unit_variant files parsed: {ok}, failed: {}", bad.len());
    for b in bad.iter().take(10) {
        println!("  FAIL {b}");
    }
    print_hist("kind (file name part)", &kind, 10);
    print_hist("version", &versions, 10);
    print_hist("header_size (u32 @0x0C)", &header_sizes, 10);
    print_hist("category count", &ncat, 10);
    print_hist("mesh count", &nmesh, 20);
    print_hist("category names", &cat_names, 40);
    print_hist("category unk (u32 @+516)", &cat_unk, 10);
    println!("category.index == position: {cat_index_is_position}/{cat_index_total}");
    println!("category mesh ranges contiguous and in bounds: {cat_ranges_ok}/{cat_ranges_total}");
    print_hist("mesh unk (u16 @+1024)", &mesh_unk, 10);
    print_hist("mesh path resolves to", &mesh_resolves, 10);
    println!("empty texture stems: {empty_tex}");
    print_hist("texture stem resolves", &tex_resolves, 10);
    print_hist("texture suffixes found for a stem", &tex_suffixes, 15);
    for m in &missing_examples {
        println!("  e.g. missing {m}");
    }
    Ok(())
}

pub fn cmd_u32s(rest: &[String]) -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve(&rest[0]))?;
    let sub = rest[1].to_ascii_lowercase();
    let n: usize = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(16);
    let skip: usize = rest.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let max: usize = rest.get(4).and_then(|s| s.parse().ok()).unwrap_or(20);
    for e in pi.entries.iter().filter(|e| e.path.to_ascii_lowercase().contains(&sub)).take(max) {
        let b = pi.read(e, Some(skip + n * 4))?;
        let w: Vec<String> = (0..n).filter(|i| skip + i * 4 + 4 <= b.len()).map(|i| u32_at(&b, skip + i * 4).to_string()).collect();
        println!("{:>9} {} | {}", e.size, e.path, w.join(" "));
    }
    Ok(())
}

/// VMPF header hypothesis check, see UNIT_VARIANT_AND_TEXTURES.md §3.
pub fn cmd_vmpf_survey() -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve("variantmodels"))?;
    let mut words: Vec<BTreeMap<u32, usize>> = vec![BTreeMap::new(); 12];
    let mut n = 0;
    let mut fits = BTreeMap::new();
    for e in pi.entries.iter().filter(|e| e.path.ends_with(".variant_part_mesh")) {
        n += 1;
        let b = pi.read(e, None)?;
        if &b[0..4] != b"VMPF" {
            bump(&mut fits, "bad magic".to_string());
            continue;
        }
        for (i, w) in words.iter_mut().enumerate() {
            if 4 + i * 4 + 4 <= b.len() {
                bump(w, u32_at(&b, 4 + i * 4));
            }
        }
        bump(&mut fits, vmpf_explain(&b));
    }
    println!("variant_part_mesh files: {n}");
    for (i, w) in words.iter().enumerate() {
        let distinct = w.len();
        let mut v: Vec<_> = w.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        let top: Vec<String> = v.iter().take(6).map(|(k, c)| format!("{k}x{c}")).collect();
        println!("  u32 @0x{:02X}: {distinct:>4} distinct; top {}", 4 + i * 4, top.join(", "));
    }
    print_hist("size accounting", &fits, 20);
    Ok(())
}

/// Try to account for the whole file size from the header. Returns a short verdict string.
fn vmpf_explain(b: &[u8]) -> String {
    // Single-part layout (INFERRED from belt_1 / head_austrian, checked here on every file):
    // 0x00 "VMPF" | 0x04 u32 0 | 0x08 u32 part_count | 0x0C u32 0 | 0x10 u32 lod_count
    // 0x14 u32 total_vertices | 0x18 u32 total_indices | 0x1C u32 9 | 0x20 u32 2
    // then lod_count x { u32 V; u32 I; V x 40-byte vertex; I x u16 index }
    // then the material parameter block (starts with UTF-16 "light_scale").
    let parts = u32_at(b, 8);
    let tag = format!("w08={parts} w0C={} w1C={} w20={}", u32_at(b, 0x0C), u32_at(b, 0x1C), u32_at(b, 0x20));
    if parts == 2 {
        return format!("part_count={parts} (not walked)");
    }
    let stride = if parts == 0 { 64 } else { 40 };
    let lods = u32_at(b, 0x10) as usize;
    let (tv, ti) = (u32_at(b, 0x14) as usize, u32_at(b, 0x18) as usize);
    let mut p = 0x24;
    let (mut sv, mut si) = (0usize, 0usize);
    for _ in 0..lods {
        if p + 8 > b.len() {
            return "lod walk ran past EOF".into();
        }
        let (v, i) = (u32_at(b, p) as usize, u32_at(b, p + 4) as usize);
        sv += v;
        si += i;
        p += 8 + v * stride + i * 2;
    }
    if p > b.len() {
        return "lod walk ran past EOF".into();
    }
    let sums = if (sv, si) == (tv, ti) { "lod sums == header totals" } else { "lod sums != header totals" };
    let rest = b.len() - p;
    let (name, _) = variant::fixed_utf16(&b[p..(p + 64).min(b.len())]);
    format!("{tag}: lods={lods}, {sums}, then {rest} B starting {name:?}")
}

pub fn cmd_atlas_survey() -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve("variantmodels"))?;
    let mut res = BTreeMap::new();
    let mut kinds = BTreeMap::new();
    let mut px = BTreeMap::new();
    let mut entries_total = 0usize;
    let mut in_unit_rect = 0usize;
    for e in pi.entries.iter().filter(|e| e.path.ends_with(".atlas")) {
        let b = pi.read(e, None)?;
        let kind = e.path.rsplit('_').next().unwrap().to_string();
        bump(&mut kinds, kind);
        let ver = u32_at(&b, 0);
        let unk = u32_at(&b, 4);
        let count = u32_at(&b, 8) as usize;
        let fits = b.len() == 12 + count * 1048;
        bump(&mut res, format!("v{ver} unk{unk} size==12+count*1048:{fits}"));
        if !fits {
            continue;
        }
        for i in 0..count {
            let p = 12 + i * 1048 + 1024;
            let r: Vec<f32> = (0..6).map(|k| f32_at(&b, p + k * 4)).collect();
            entries_total += 1;
            let s1 = variant::fixed_utf16(&b[12 + i * 1048..12 + i * 1048 + 512]).0;
            let s2 = variant::fixed_utf16(&b[12 + i * 1048 + 512..12 + i * 1048 + 1024]).0;
            if entries_total == 1 { println!("example entry: {s1:?} | {s2:?} | {r:?}"); }
            bump(&mut px, format!("second string {}", if s2.is_empty() { "empty" } else if s1 == s2 { "== first" } else { "differs" }));
            if r[..4].iter().all(|x| (0.0..=1.0).contains(x)) {
                in_unit_rect += 1;
            }
            bump(&mut px, format!("{}x{}", r[4], r[5]));
        }
    }
    print_hist("atlas files by suffix", &kinds, 10);
    print_hist("atlas header / size check", &res, 10);
    println!("atlas entries: {entries_total}, uv rect inside [0,1]: {in_unit_rect}");
    print_hist("entry pixel size (floats 5,6)", &px, 15);
    Ok(())
}

/// Describes a DDS pixel format from the 128-byte header.
fn dds_format(b: &[u8]) -> String {
    let flags = u32_at(b, 0x50);
    let fourcc = &b[0x54..0x58];
    let bits = u32_at(b, 0x58);
    if flags & 0x4 != 0 {
        let s = String::from_utf8_lossy(fourcc).to_string();
        if s.chars().all(|c| c.is_ascii_alphanumeric()) {
            return s;
        }
        return format!("fourcc#{}", u32_at(b, 0x54));
    }
    let (r, g, bm, a) = (u32_at(b, 0x5C), u32_at(b, 0x60), u32_at(b, 0x64), u32_at(b, 0x68));
    let kind = if flags & 0x40 != 0 { "RGB" } else if flags & 0x20000 != 0 { "L" } else if flags & 0x2 != 0 { "A" } else { "?" };
    let alpha = if flags & 0x1 != 0 { "A" } else { "" };
    format!("{kind}{alpha}{bits} r{r:X} g{g:X} b{bm:X} a{a:X}")
}

fn bytes_per_block(fmt: &str) -> Option<(usize, bool)> {
    match fmt {
        "DXT1" => Some((8, true)),
        "DXT3" | "DXT5" | "ATI2" => Some((16, true)),
        _ => None,
    }
}

pub fn cmd_dds_survey(rest: &[String]) -> io::Result<()> {
    let packs = if rest.is_empty() { pack::all_packs()? } else { rest.iter().map(|a| pack::resolve(a)).collect() };
    let mut fmt_all: BTreeMap<String, usize> = BTreeMap::new();
    let mut fmt_bytes: BTreeMap<String, u64> = BTreeMap::new();
    let mut dims = BTreeMap::new();
    let mut mips = BTreeMap::new();
    let mut caps = BTreeMap::new();
    let mut size_ok = BTreeMap::new();
    let mut not_pow2 = 0usize;
    let mut per_pack = String::new();
    for p in packs {
        let pi = PackIndex::open(&p)?;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let mut fmt_pack: BTreeMap<String, usize> = BTreeMap::new();
        for e in pi.entries.iter().filter(|e| e.path.to_ascii_lowercase().ends_with(".dds")) {
            let b = pi.read(e, Some(128))?;
            if b.len() < 128 || &b[0..4] != b"DDS " {
                bump(&mut fmt_pack, "not DDS".to_string());
                continue;
            }
            let h = u32_at(&b, 0x0C) as usize;
            let w = u32_at(&b, 0x10) as usize;
            let mipc = u32_at(&b, 0x1C).max(1) as usize;
            let caps2 = u32_at(&b, 0x70);
            let f = dds_format(&b);
            bump(&mut fmt_pack, f.clone());
            bump(&mut fmt_all, f.clone());
            *fmt_bytes.entry(f.clone()).or_default() += e.size as u64;
            bump(&mut dims, format!("{w}x{h}"));
            bump(&mut mips, mipc);
            bump(&mut caps, if caps2 & 0x200 != 0 { "cubemap" } else if caps2 & 0x200000 != 0 { "volume" } else { "2d" });
            if !w.is_power_of_two() || !h.is_power_of_two() {
                not_pow2 += 1;
            }
            // expected payload for 2D textures
            let expect = if let Some((bpb, _)) = bytes_per_block(&f) {
                let mut s = 0usize;
                let (mut x, mut y) = (w, h);
                for _ in 0..mipc {
                    s += x.div_ceil(4).max(1) * y.div_ceil(4).max(1) * bpb;
                    x = (x / 2).max(1);
                    y = (y / 2).max(1);
                }
                Some(s)
            } else {
                let bits = u32_at(&b, 0x58) as usize;
                if bits > 0 {
                    let mut s = 0usize;
                    let (mut x, mut y) = (w, h);
                    for _ in 0..mipc {
                        s += x * y * bits / 8;
                        x = (x / 2).max(1);
                        y = (y / 2).max(1);
                    }
                    Some(s)
                } else {
                    None
                }
            };
            let faces = if caps2 & 0x200 != 0 { 6 } else { 1 };
            let verdict = match expect {
                Some(x) if 128 + x * faces == e.size as usize => "exact".to_string(),
                Some(x) if 128 + x * faces < e.size as usize => format!("extra {} B", e.size as usize - 128 - x * faces),
                Some(_) => "short".to_string(),
                None => "unknown fmt".to_string(),
            };
            bump(&mut size_ok, verdict);
        }
        if !fmt_pack.is_empty() {
            let mut v: Vec<_> = fmt_pack.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            per_pack += &format!("  {name}: {}\n", v.iter().map(|(k, n)| format!("{k}={n}")).collect::<Vec<_>>().join(", "));
        }
    }
    println!("-- per pack\n{per_pack}");
    let mut v: Vec<_> = fmt_all.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    println!("-- format: count, total MB");
    for (k, n) in v {
        println!("  {n:>6}  {:>9.1} MB  {k}", fmt_bytes[k] as f64 / 1e6);
    }
    print_hist("dimensions", &dims, 25);
    print_hist("mip count", &mips, 15);
    print_hist("kind", &caps, 5);
    print_hist("file size vs header (128 + mip chain)", &size_ok, 10);
    println!("non power-of-two: {not_pow2}");
    Ok(())
}

/// Debug: offsets where the pair (u32 9, u32 2) occurs, with the two u32 before it.
pub fn cmd_find_pair(rest: &[String]) -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve(&rest[0]))?;
    let e = pi.find(&rest[1]).expect("no such entry");
    let b = pi.read(e, None)?;
    let a: u32 = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(9);
    let c: u32 = rest.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
    println!("size {}", b.len());
    for p in (8..b.len().saturating_sub(8)).step_by(1) {
        if u32_at(&b, p) == a && u32_at(&b, p + 4) == c {
            println!("  @{p:#x} ({p}): prev {} {}", u32_at(&b, p - 8), u32_at(&b, p - 4));
        }
    }
    Ok(())
}

/// VMPF layout probe: for every file, where does the trailing parameter block start,
/// and what vertex stride follows if the index list (u16) sits right before it?
pub fn cmd_vmpf_probe() -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve("variantmodels"))?;
    let mut strides = BTreeMap::new();
    let mut trailer = BTreeMap::new();
    let mut first_names = BTreeMap::new();
    for e in pi.entries.iter().filter(|e| e.path.ends_with(".variant_part_mesh")) {
        let b = pi.read(e, None)?;
        let parts = u32_at(&b, 8);
        if parts != 1 {
            bump(&mut strides, format!("parts={parts} (skipped)"));
            continue;
        }
        let v = u32_at(&b, 20) as usize;
        let i = u32_at(&b, 24) as usize;
        let needle: Vec<u8> = "light_scale".bytes().flat_map(|c| [c, 0]).collect();
        let Some(ps) = b.windows(needle.len()).position(|w| w == &needle[..]) else {
            bump(&mut strides, "no param block".to_string());
            continue;
        };
        let (name, _) = variant::fixed_utf16(&b[ps..(ps + 64).min(b.len())]);
        bump(&mut first_names, name);
        bump(&mut trailer, b.len() - ps);
        let vbytes = ps as isize - 44 - (i as isize) * 2;
        let s = format!("40*V + {}", vbytes - 40 * v as isize);
        bump(&mut strides, s);
    }
    print_hist("implied vertex stride (bytes)", &strides, 20);
    print_hist("param block size (bytes)", &trailer, 10);
    print_hist("first param name", &first_names, 10);
    Ok(())
}

/// Debug: offsets of a 4-byte hex pattern, printing only where (offset mod stride) changes.
pub fn cmd_lattice(rest: &[String]) -> io::Result<()> {
    let pi = PackIndex::open(pack::resolve(&rest[0]))?;
    let e = pi.find(&rest[1]).expect("no such entry");
    let b = pi.read(e, None)?;
    let pat: Vec<u8> = (0..rest[2].len() / 2).map(|i| u8::from_str_radix(&rest[2][2 * i..2 * i + 2], 16).unwrap()).collect();
    let stride: usize = rest.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
    let mut last: Option<usize> = None;
    let mut prev = 0usize;
    let mut n = 0usize;
    for p in 0..b.len().saturating_sub(pat.len()) {
        if b[p..p + pat.len()] == pat[..] {
            n += 1;
            if last != Some(p % stride) {
                println!("  @{p} (mod {stride} = {}), previous hit @{prev}, hits so far {n}", p % stride);
                last = Some(p % stride);
            }
            prev = p;
        }
    }
    println!("last hit @{prev}, total {n}, size {}", b.len());
    Ok(())
}

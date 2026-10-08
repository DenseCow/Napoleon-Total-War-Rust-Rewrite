//! Research probe for battle vegetation (read-only, prints to stdout, writes nothing).
//!
//! ```text
//! cargo run -p ntw_formats --example tree_probe -- model <folder>     dump data.tree_model raw
//! cargo run -p ntw_formats --example tree_probe -- dds <path>          DDS header
//! cargo run -p ntw_formats --example tree_probe -- survey             every data.tree_model vs composite maps
//! ```
use ntw_formats::dds::Dds;
use ntw_formats::pack::Vfs;

const DEFAULT_DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn dump_model(b: &[u8]) {
    println!("header f32 {} u32 {} u32 {}  ({} bytes)", f32_at(b, 0), u32_at(b, 4), u32_at(b, 8), b.len());
    let mut o = 12;
    while o + 2 <= b.len() {
        let n = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
        o += 2;
        let s: String = char::decode_utf16((0..n).map(|i| u16::from_le_bytes([b[o + 2 * i], b[o + 2 * i + 1]])))
            .map(|c| c.unwrap_or('?'))
            .collect();
        o += 2 * n;
        let f: Vec<String> = (0..14).map(|i| format!("{:.3}", f32_at(b, o + 4 * i))).collect();
        o += 56;
        println!("{s}  [{}]", f.join(", "));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = std::env::var("NAPOLEON_INSTALL_DIR")
        .map(|d| format!(r"{d}\data"))
        .unwrap_or_else(|_| DEFAULT_DATA.to_string());
    let vfs = Vfs::open_install(&data).expect("open install");
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match arg(0).as_str() {
        "model" => {
            let p = format!(r"rigidmodels\vegetation\battle\{}\data.tree_model", arg(1));
            dump_model(&vfs.read(&p).expect("read"));
        }
        "table" => ascii_runs(&vfs.read(&arg(1)).expect("read")),
        "dds" => {
            let b = vfs.read(&arg(1)).expect("read");
            let d = Dds::parse(&b).expect("dds");
            println!("{}x{} mips {} format {:?}", d.width, d.height, d.mip_count, d.format);
        }
        "survey" => {
            for p in vfs.list(r"rigidmodels\vegetation\battle\") {
                if p.ends_with("data.tree_model") {
                    println!("== {p}");
                    dump_model(&vfs.read(p).unwrap());
                }
            }
        }
        "ratio" => {
            let mut rects = std::collections::HashMap::new();
            for p in vfs.list(r"rigidmodels\vegetation\battle\") {
                if !p.ends_with("_compositemap.txt") {
                    continue;
                }
                let text = String::from_utf8_lossy(&vfs.read(p).unwrap()).to_string();
                let mut name = String::new();
                let mut section = "";
                for line in text.lines().map(str::trim) {
                    if line.to_ascii_lowercase().ends_with(".spt") {
                        name = line.to_ascii_lowercase();
                    } else if ["Leaves", "Fronds", "Billboards"].contains(&line) {
                        section = if line == "Billboards" { "b" } else { "" };
                    } else if section == "b" && !rects.contains_key(&name) {
                        let v: Vec<f32> = line.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                        if v.len() == 8 {
                            rects.insert(name.clone(), (v[0] - v[2], v[1] - v[5], v.len()));
                        }
                    }
                }
            }
            for p in vfs.list(r"rigidmodels\vegetation\battle\") {
                if !p.ends_with("data.tree_model") {
                    continue;
                }
                let b = vfs.read(p).unwrap();
                let mut o = 12;
                let mut seen = std::collections::HashSet::new();
                while o + 2 <= b.len() {
                    let n = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
                    o += 2;
                    let s: String = char::decode_utf16((0..n).map(|i| u16::from_le_bytes([b[o + 2 * i], b[o + 2 * i + 1]])))
                        .map(|c| c.unwrap_or('?'))
                        .collect();
                    o += 2 * n;
                    let f: Vec<f32> = (0..14).map(|i| f32_at(&b, o + 4 * i)).collect();
                    o += 56;
                    let file = s.rsplit('\\').next().unwrap().to_ascii_lowercase();
                    if !seen.insert(file.clone()) {
                        continue;
                    }
                    let Some(&(du, dv, _)) = rects.get(&file) else {
                        println!("{file}: no billboard");
                        continue;
                    };
                    let r = du / dv;
                    let hx = f[11].max(-f[8]);
                    let hz = f[13].max(-f[10]);
                    let hy = f[12];
                    println!(
                        "{file:50} r {r:.3} | 2hx/2hy {:.3} diag/2hy {:.3} 2hx/(f5+hy) {:.3} | f3 {:.3} f4 {:.3} f5 {:.3} f7 {:.3} box {:.2},{:.2},{:.2} {:.2},{:.2},{:.2}",
                        hx / hy,
                        (hx * hx + hz * hz).sqrt() / hy,
                        2.0 * hx / (f[5] + hy),
                        f[3], f[4], f[5], f[7], f[8], f[9], f[10], f[11], f[12], f[13]
                    );
                }
            }
        }
        // ground <map>: ground-type histogram over the map and under its trees
        "ground" => {
            let map = ntw_formats::battle_terrain::BattleMap::load(&vfs, &arg(1)).expect("map");
            let gt = map.ground_types.as_ref().expect("ground types");
            let w = map.definition.base_terrain_width;
            let mut all = [0usize; 256];
            for &c in &gt.cells {
                all[c as usize] += 1;
            }
            let mut under = [0usize; 256];
            for l in &map.trees {
                for g in &l.groups {
                    for i in &g.instances {
                        under[gt.at(i.position.0, i.position.1, w, w) as usize] += 1;
                    }
                }
            }
            let (na, nu) = (gt.cells.len() as f64, under.iter().sum::<usize>().max(1) as f64);
            for k in 0..256 {
                if all[k] + under[k] > 0 {
                    let p = gt.palette.get(k).copied().unwrap_or_default();
                    println!("{k:3} {:?} map {:6.2}%  trees {:6.2}%", p, 100.0 * all[k] as f64 / na, 100.0 * under[k] as f64 / nu);
                }
            }
            let mut vars = std::collections::BTreeMap::new();
            for l in &map.trees {
                for g in &l.groups {
                    let e = vars.entry(g.species.clone()).or_insert((0usize, 255u8, 0u8, 0i32));
                    for i in &g.instances {
                        e.0 += 1;
                        e.1 = e.1.min(i.variation);
                        e.2 = e.2.max(i.variation);
                        e.3 |= i.flags;
                    }
                }
            }
            let mut hist = [0usize; 16];
            for l in &map.trees {
                for g in &l.groups {
                    for i in &g.instances {
                        hist[(i.variation / 16) as usize] += 1;
                    }
                }
            }
            println!("variation histogram (/16): {hist:?}  lists {} flags {:?}", map.trees.len(), map.trees.iter().map(|l| (l.flag, l.groups.iter().map(|g| g.instances.len()).sum::<usize>())).collect::<Vec<_>>());
            for (li, l) in map.trees.iter().enumerate() {
                let mut ext = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                let mut inside = 0;
                for g in &l.groups {
                    for i in &g.instances {
                        let (x, y) = i.position;
                        ext = [ext[0].min(x), ext[1].min(y), ext[2].max(x), ext[3].max(y)];
                        if x.abs() < 1024.0 && y.abs() < 1024.0 { inside += 1; }
                    }
                }
                println!("list {li}: extent {ext:?} inside 2km {inside} species {:?}", l.groups.iter().map(|g| (g.species.as_str(), g.instances.len())).collect::<Vec<_>>());
            }
            for (k, v) in vars {
                println!("{k}: n {} var {}..{} flags|{:#x}", v.0, v.1, v.2, v.3);
            }
        }
        "palette" => {
            let map = ntw_formats::battle_terrain::BattleMap::load(&vfs, &arg(1)).expect("map");
            for (i, p) in map.ground_types.as_ref().unwrap().palette.iter().enumerate() {
                println!("{i:2} {p:?}");
            }
        }
        // strings <file> <word,word,...>: offsets of NUL-terminated ASCII / UTF-16 copies of words
        // ptrs <exe> <file offset> <file offset end>: for each NUL-terminated string in the range,
        // its virtual address and every place in the file that holds that address as a u32
        "ptrs" => {
            let b = std::fs::read(arg(1)).expect("read file");
            let num = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap() as usize;
            let (start, end) = (num(&arg(2)), num(&arg(3)));
            let pe = u32_at(&b, 0x3c) as usize;
            let nsec = u16::from_le_bytes([b[pe + 6], b[pe + 7]]) as usize;
            let opt = u16::from_le_bytes([b[pe + 20], b[pe + 21]]) as usize;
            let base = u32_at(&b, pe + 24 + 28) as usize;
            let sec0 = pe + 24 + opt;
            let secs: Vec<(usize, usize, usize)> = (0..nsec)
                .map(|i| {
                    let s = sec0 + 40 * i;
                    (u32_at(&b, s + 12) as usize, u32_at(&b, s + 16) as usize, u32_at(&b, s + 20) as usize)
                })
                .collect();
            let va_of = |off: usize| {
                secs.iter().find(|(_, size, raw)| off >= *raw && off < raw + size).map(|(va, _, raw)| base + va + off - raw)
            };
            let mut o = start;
            while o < end {
                if b[o] == 0 {
                    o += 1;
                    continue;
                }
                let e = o + b[o..].iter().position(|&c| c == 0).unwrap();
                let s = String::from_utf8_lossy(&b[o..e]).to_string();
                let va = va_of(o).unwrap_or(0) as u32;
                let refs: Vec<String> = b
                    .windows(4)
                    .enumerate()
                    .filter(|(_, w)| *w == va.to_le_bytes())
                    .map(|(i, _)| format!("{i:#x}"))
                    .collect();
                println!("{o:#x} va {va:#x} {s:30} refs {}", refs.join(" "));
                o = e;
            }
        }
        "strings" => {
            let b = std::fs::read(arg(1)).expect("read file");
            let mut hits = Vec::new();
            for w in arg(2).split(',') {
                let mut a = vec![0u8];
                a.extend(w.as_bytes());
                a.push(0);
                let mut u = vec![0u8, 0];
                for c in w.encode_utf16() {
                    u.extend(c.to_le_bytes());
                }
                u.extend([0, 0]);
                for (kind, pat) in [("A", &a), ("U", &u)] {
                    for (i, win) in b.windows(pat.len()).enumerate() {
                        if win == &pat[..] {
                            hits.push((i, kind, w.to_owned()));
                        }
                    }
                }
            }
            hits.sort();
            for (i, k, w) in hits {
                println!("{i:#010x} {k} {w}");
            }
        }
        _ => eprintln!("usage: tree_probe model|dds|survey|table|ratio|ground|..."),
    }
}

/// Every NUL-terminated ASCII run in a packed file, with its offset: enough to read a packed
/// table's column order without guessing a schema. UTF-16 is scanned at both byte alignments --
/// the string tables are not always an even number of bytes from the file start.
fn ascii_runs(b: &[u8]) {
    for (name, unit, align) in [("ascii", 1usize, 0usize), ("utf16", 2, 0), ("utf16", 2, 1)] {
        let mut start = None;
        let mut i = align;
        while i + unit <= b.len() {
            let printable = if unit == 1 {
                (0x20..0x7f).contains(&b[i])
            } else {
                (0x20..0x7f).contains(&b[i]) && b[i + 1] == 0
            };
            if printable {
                start.get_or_insert(i);
            } else if let Some(s) = start.take()
                && i - s >= 4 * unit
            {
                let raw = &b[s..i];
                let text = if unit == 1 {
                    String::from_utf8_lossy(raw).into_owned()
                } else {
                    raw.chunks_exact(2).map(|c| char::from(c[0])).collect()
                };
                println!("{name}{align} {s:#08x} {text}");
            }
            i += unit;
        }
    }
}

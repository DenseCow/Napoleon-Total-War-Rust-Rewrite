//! Worker 3 campaign tools: ESF reader, Lua scanner, UI layout probe. std only.
mod esf;
mod pack;
mod luascan;
mod luac;
mod startpos;
mod ui;
mod api;
mod db;

use esf::{Esf, Item, Val};
use std::collections::{BTreeMap, HashMap};
use std::fs;

fn load_esf(path: &str) -> Esf {
    let data = if let Some(rest) = path.strip_prefix("pack:") {
        let (pk, ent) = rest.split_once(':').expect("pack:<pack>:<entry>");
        let p = pack::Pack::open(pack::data_path(pk)).unwrap();
        let e = p.find(ent).expect("entry not found").clone();
        p.read(&e).unwrap()
    } else { fs::read(path).expect("read") };
    match Esf::parse(data) { Ok(e) => e, Err(m) => { eprintln!("PARSE ERROR {}: {}", path, m); std::process::exit(1) } }
}

#[derive(Default)]
struct Stats { recs: usize, recarrs: usize, items: usize, vals: BTreeMap<u8, usize>, names: HashMap<String, usize>, maxdepth: usize }

fn walk_stats(e: &Esf, items: &[Item], d: usize, s: &mut Stats) {
    s.maxdepth = s.maxdepth.max(d);
    for it in items {
        match it {
            Item::V(v) => *s.vals.entry(esf::val_code(v)).or_default() += 1,
            Item::Rec(r) => { s.recs += 1; *s.names.entry(e.name(r.name).to_string()).or_default() += 1; walk_stats(e, &r.children, d + 1, s) }
            Item::RecArr(r) => { s.recarrs += 1; s.items += r.items.len(); *s.names.entry(format!("{}[]", e.name(r.name))).or_default() += 1;
                for i in &r.items { walk_stats(e, i, d + 1, s) } }
        }
    }
}

/// Schema node: unique path of record names.
#[derive(Default)]
struct Sch { order: usize, count: usize, vers: Vec<u8>, sigs: HashMap<String, usize>, children: BTreeMap<String, Sch>, is_arr: bool, arr_len_min: usize, arr_len_max: usize }

fn build_schema(e: &Esf, items: &[Item], node: &mut Sch, ctr: &mut usize) {
    for it in items {
        match it {
            Item::Rec(r) => {
                let k = e.name(r.name).to_string();
                let ch = node.children.entry(k).or_insert_with(|| { *ctr += 1; Sch { order: *ctr, arr_len_min: usize::MAX, ..Default::default() } });
                ch.count += 1; if !ch.vers.contains(&r.ver) { ch.vers.push(r.ver) }
                *ch.sigs.entry(esf::sig(e, &r.children)).or_default() += 1;
                build_schema(e, &r.children, ch, ctr);
            }
            Item::RecArr(r) => {
                let k = format!("{}[]", e.name(r.name));
                let ch = node.children.entry(k).or_insert_with(|| { *ctr += 1; Sch { order: *ctr, is_arr: true, arr_len_min: usize::MAX, ..Default::default() } });
                ch.count += 1; if !ch.vers.contains(&r.ver) { ch.vers.push(r.ver) }
                ch.arr_len_min = ch.arr_len_min.min(r.items.len()); ch.arr_len_max = ch.arr_len_max.max(r.items.len());
                for i in &r.items { *ch.sigs.entry(esf::sig(e, i)).or_default() += 1; build_schema(e, i, ch, ctr); }
            }
            _ => {}
        }
    }
}

fn print_schema(name: &str, n: &Sch, d: usize, maxd: usize, sigw: usize) {
    let mut sigs: Vec<_> = n.sigs.iter().collect();
    sigs.sort_by(|a, b| b.1.cmp(a.1));
    let sig = sigs.first().map(|s| s.0.as_str()).unwrap_or("");
    let sig = if sig.len() > sigw { format!("{}...", &sig[..sig.char_indices().take_while(|(i, _)| *i < sigw).last().map(|x| x.0).unwrap_or(0)]) } else { sig.to_string() };
    let arr = if n.is_arr { format!(" len {}..{}", n.arr_len_min, n.arr_len_max) } else { String::new() };
    println!("{}{} x{} v{:?}{} nsig={} :: {}", "  ".repeat(d), name, n.count, n.vers, arr, n.sigs.len(), sig);
    if d >= maxd { if !n.children.is_empty() { println!("{}  ... {} child kinds", "  ".repeat(d), n.children.len()); } return; }
    let mut ch: Vec<_> = n.children.iter().collect();
    ch.sort_by_key(|c| c.1.order);
    for (k, c) in ch { print_schema(k, c, d + 1, maxd, sigw); }
}

fn cmd_summary(path: &str, maxd: usize) {
    let e = load_esf(path);
    println!("file {} size {} magic 0x{:X} unk {} timestamp {} names_off 0x{:X} names {} trailing_bytes {}",
        path, e.data.len(), e.magic, e.unk, e.timestamp, e.names_off, e.names.len(), e.trailing);
    let mut s = Stats::default();
    walk_stats(&e, &e.root.children, 1, &mut s);
    println!("records {} record_arrays {} array_items {} max_depth {}", s.recs + 1, s.recarrs, s.items, s.maxdepth);
    let vs: Vec<String> = s.vals.iter().map(|(k, v)| format!("{}(0x{:02x})={}", if k & 0x40 != 0 { format!("{}[]", esf::type_name(k & 0x3F)) } else { esf::type_name(*k).into() }, k, v)).collect();
    println!("values: {}", vs.join(" "));
    let mut ctr = 0;
    let mut root = Sch { arr_len_min: usize::MAX, ..Default::default() };
    let wrapper = vec![Item::Rec(e.root.clone())];
    build_schema(&e, &wrapper, &mut root, &mut ctr);
    println!("--- schema (record paths; xN occurrences; vVERSIONS; most common child signature)");
    for (k, c) in &root.children { print_schema(k, c, 0, maxd, 160); }
}

fn print_items(e: &Esf, items: &[Item], d: usize, maxd: usize, maxi: usize) {
    let ind = "  ".repeat(d);
    let mut shown_vals = 0;
    for it in items {
        match it {
            Item::V(Val::Arr { code, start, end }) => {
                let (n, el) = e.arr_elems(*code, *start, *end, 12);
                println!("{}{}[{}] = [{}{}]", ind, esf::type_name(code & 0x3F), n, el.join(", "), if n > 12 { ", ..." } else { "" });
            }
            Item::V(v) => { shown_vals += 1; if shown_vals <= 400 { println!("{}{} {}", ind, esf::type_name(esf::val_code(v)), esf::fmt_val(v)); } }
            Item::Rec(r) => {
                println!("{}{{{}}} v{} @0x{:x} ({} children)", ind, e.name(r.name), r.ver, r.off, r.children.len());
                if d < maxd { print_items(e, &r.children, d + 1, maxd, maxi) }
            }
            Item::RecArr(r) => {
                println!("{}[{}] v{} @0x{:x} count {}", ind, e.name(r.name), r.ver, r.off, r.items.len());
                if d < maxd { for (i, x) in r.items.iter().enumerate().take(maxi) { println!("{}  #{}", ind, i); print_items(e, x, d + 2, maxd, maxi) } }
            }
        }
    }
}

/// Resolve a path like "A/B[3]/C" (record names; [i] picks a record-array item or the i-th same-named record).
pub fn resolve<'a>(e: &'a Esf, path: &str) -> Vec<&'a [Item]> {
    let mut cur: Vec<&[Item]> = Vec::new();
    let root_children: &[Item] = &e.root.children;
    let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.first().map(|p| p.starts_with(e.name(e.root.name))).unwrap_or(false) { parts.remove(0); }
    cur.push(root_children);
    for p in parts {
        let (nm, idx) = match p.find('[') { Some(i) => (&p[..i], Some(p[i + 1..p.len() - 1].parse::<usize>().unwrap())), None => (p, None) };
        let mut next = Vec::new();
        for items in &cur {
            let mut k = 0;
            for it in items.iter() {
                match it {
                    Item::Rec(r) if e.name(r.name) == nm => { if idx.map(|i| i == k).unwrap_or(true) { next.push(&r.children[..]); } k += 1; }
                    Item::RecArr(r) if e.name(r.name) == nm => {
                        match idx { Some(i) => if let Some(x) = r.items.get(i) { next.push(&x[..]) }, None => for x in &r.items { next.push(&x[..]) } }
                    }
                    _ => {}
                }
            }
        }
        cur = next;
    }
    cur
}

fn cmd_tree(path: &str, sub: &str, maxd: usize, maxi: usize, maxmatch: usize) {
    let e = load_esf(path);
    let r = resolve(&e, sub);
    println!("{} matches", r.len());
    for (i, items) in r.iter().enumerate().take(maxmatch) { println!("== match {}", i); print_items(&e, items, 0, maxd, maxi); }
}

fn cmd_scan_all() {
    let mut files = Vec::new();
    fn rec(dir: &std::path::Path, out: &mut Vec<String>) {
        if let Ok(rd) = fs::read_dir(dir) { for d in rd.flatten() { let p = d.path(); if p.is_dir() { rec(&p, out) } else if p.extension().map(|x| x.eq_ignore_ascii_case("esf")).unwrap_or(false) { out.push(p.to_string_lossy().into_owned()) } } }
    }
    rec(std::path::Path::new(pack::DATA_DIR), &mut files);
    for f in &files {
        let data = fs::read(f).unwrap();
        let len = data.len();
        match Esf::parse(data) {
            Ok(e) => { let mut s = Stats::default(); walk_stats(&e, &e.root.children, 1, &mut s);
                println!("OK  {} size {} ts {} root {{{}}} v{} names {} recs {} recarrs {} depth {} trailing {}", f, len, e.timestamp, e.name(e.root.name), e.root.ver, e.names.len(), s.recs + 1, s.recarrs, s.maxdepth, e.trailing) }
            Err(m) => println!("ERR {} {}", f, m),
        }
    }
    for pk in pack::PACKS {
        if let Ok(p) = pack::Pack::open(pack::data_path(pk)) {
            for en in &p.entries { if en.path.to_ascii_lowercase().ends_with(".esf") { println!("PACK {} {} {}", pk, en.path, en.size) } }
            let mut abce = 0;
            for en in &p.entries { if en.size >= 4 && en.size < 50_000_000 && !en.path.ends_with(".dds") && !en.path.ends_with(".mp3") && !en.path.ends_with(".wav") && !en.path.ends_with(".tga") {
                let h = p.read_head(en, 4).unwrap(); if h == [0xCE, 0xAB, 0, 0] { abce += 1; if abce < 40 { println!("PACK-ABCE {} {} {}", pk, en.path, en.size) } } } }
            println!("pack {} entries with ABCE magic: {}", pk, abce);
        }
    }
}

fn cmd_pack_ls(pk: &str, filt: &str) {
    let p = pack::Pack::open(pack::data_path(pk)).unwrap();
    for e in &p.entries { if e.path.to_ascii_lowercase().contains(&filt.to_ascii_lowercase()) { println!("{}\t{}\t{}", e.size, e.offset, e.path) } }
}
fn cmd_pack_head(pk: &str, ent: &str, n: usize) {
    let p = pack::Pack::open(pack::data_path(pk)).unwrap();
    let e = p.find(ent).expect("no entry").clone();
    let b = p.read_head(&e, n).unwrap();
    hexdump(&b, 0);
}
fn cmd_pack_text(pk: &str, ent: &str) {
    let p = pack::Pack::open(pack::data_path(pk)).unwrap();
    let e = p.find(ent).expect("no entry").clone();
    let b = p.read(&e).unwrap();
    print!("{}", String::from_utf8_lossy(&b));
}
pub fn hexdump(b: &[u8], base: usize) {
    for (i, ch) in b.chunks(16).enumerate() {
        let hx: Vec<String> = ch.iter().map(|x| format!("{:02x}", x)).collect();
        let asc: String = ch.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect();
        println!("{:08x}  {:<48} {}", base + i * 16, hx.join(" "), asc);
    }
}
fn cmd_hex(file: &str, off: usize, n: usize) {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(file).unwrap();
    f.seek(SeekFrom::Start(off as u64)).unwrap();
    let mut b = vec![0u8; n]; let k = f.read(&mut b).unwrap(); b.truncate(k);
    hexdump(&b, off);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g = |i: usize, d: &str| a.get(i).cloned().unwrap_or_else(|| d.to_string());
    match a.get(1).map(|s| s.as_str()) {
        Some("esf-summary") => cmd_summary(&a[2], g(3, "3").parse().unwrap()),
        Some("esf-tree") => cmd_tree(&a[2], &g(3, ""), g(4, "2").parse().unwrap(), g(5, "3").parse().unwrap(), g(6, "3").parse().unwrap()),
        Some("esf-scan-all") => cmd_scan_all(),
        Some("esf-paths") => cmd_paths(&a[2]),
        Some("pack-ls") => cmd_pack_ls(&a[2], &g(3, "")),
        Some("pack-head") => cmd_pack_head(&a[2], &a[3], g(4, "256").parse().unwrap()),
        Some("pack-text") => cmd_pack_text(&a[2], &a[3]),
        Some("hex") => cmd_hex(&a[2], usize::from_str_radix(g(3, "0").trim_start_matches("0x"), 16).unwrap(), g(4, "256").parse().unwrap()),
        Some("startpos") => startpos::run(&load_esf(&a[2]), a.get(3).map(|s| s.as_str())),
        Some("lua-scan") => luascan::run(&a[2..]),
        Some("luac-info") => luac::run(&a[2..]),
        Some("ui-probe") => ui::run(&a[2..]),
        Some("lua-api") => api::run(&a[2..]),
        Some(c) if c.starts_with("db") => db::run(&a[1..]),
        Some(c) if c.starts_with("loc") => db::run_loc(&a[1..]),
        _ => eprintln!("usage: campaign_tools <esf-summary FILE [depth] | esf-tree FILE PATH [depth] [items] [matches] | esf-scan-all | pack-ls PACK [filter] | pack-head PACK ENTRY [n] | pack-text PACK ENTRY | hex FILE OFFHEX N | startpos FILE | lua-scan ... | luac-info ... | ui-probe ...>\n FILE may be pack:<pack>:<entry>"),
    }
}

fn walk_paths(e: &Esf, items: &[Item], prefix: &str, out: &mut BTreeMap<String, (usize, String)>) {
    for it in items {
        match it {
            Item::Rec(r) => { let p = format!("{}/{}", prefix, e.name(r.name)); let ent = out.entry(p.clone()).or_insert((0, String::new())); ent.0 += 1; if ent.1.is_empty() { ent.1 = format!("v{} {}", r.ver, esf::sig(e, &r.children)); } walk_paths(e, &r.children, &p, out) }
            Item::RecArr(r) => { let p = format!("{}/{}[]", prefix, e.name(r.name)); let ent = out.entry(p.clone()).or_insert((0, String::new())); ent.0 += r.items.len(); if ent.1.is_empty() { ent.1 = format!("v{} {}", r.ver, r.items.first().map(|i| esf::sig(e, i)).unwrap_or_default()); } for i in &r.items { walk_paths(e, i, &p, out) } }
            _ => {}
        }
    }
}
pub fn cmd_paths(path: &str) {
    let e = load_esf(path);
    let mut out = BTreeMap::new();
    walk_paths(&e, &e.root.children, "", &mut out);
    for (k, (c, s)) in out { println!("{}\t{}\t{}", k, c, s); }
}

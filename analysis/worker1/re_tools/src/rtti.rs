//! MSVC x86 RTTI recovery: TypeDescriptor -> CompleteObjectLocator -> vtable, and
//! ClassHierarchyDescriptor -> BaseClassArray -> base classes.
use crate::demangle::demangle_td;
use crate::pe::Pe;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

pub struct Td { pub va: u32, pub raw: String, pub name: String, pub demangled_ok: bool }
pub struct Vt { pub va: u32, pub col: u32, pub offset: u32, pub nmethods: usize, pub first: Vec<u32> }
pub struct Class { pub td: Td, pub vtables: Vec<Vt>, pub chd: Option<u32>, pub chd_attr: u32, pub bases_flat: Vec<(String, u32, u32)>, pub direct_bases: Vec<String> }

pub fn find(p: &Pe) -> Vec<Class> {
    let d = &p.d;
    // 1. type descriptors
    let mut tds: Vec<Td> = vec![];
    let mut i = 0;
    while let Some(pos) = memfind(&d[i..], b".?A") {
        let off = i + pos;
        i = off + 3;
        let k = d.get(off + 3).copied().unwrap_or(0);
        if k != b'V' && k != b'U' && k != b'W' { continue; }
        if off < 8 { continue; }
        let raw = p.cstr(off, 2048);
        if !raw.ends_with("@@") && !raw.starts_with(".?AW") { continue; }
        let Some(va) = p.off2va(off - 8) else { continue };
        let (name, ok) = match demangle_td(&raw) { Some(n) => (n, true), None => (raw.clone(), false) };
        tds.push(Td { va, raw, name, demangled_ok: ok });
    }
    let td_idx: HashMap<u32, usize> = tds.iter().enumerate().map(|(i, t)| (t.va, i)).collect();
    // 2. COLs in readable non-code sections
    let mut cols: HashMap<u32, (usize, u32, u32, u32)> = HashMap::new(); // colva -> (td idx, offset, cdoff, chd)
    let data_secs: Vec<_> = p.sections.iter().filter(|s| s.ch & 0x20000000 == 0 && s.name != ".reloc" && s.name != ".rsrc").cloned().collect();
    for s in &data_secs {
        let (a, b) = (s.raw as usize, (s.raw + s.rsize) as usize);
        let mut o = a;
        while o + 20 <= b {
            if p.u32at(o) == 0 {
                let tdp = p.u32at(o + 12);
                if let Some(&ti) = td_idx.get(&tdp) {
                    let chd = p.u32at(o + 16);
                    if p.va2off(chd).is_some() {
                        cols.insert(p.off2va(o).unwrap(), (ti, p.u32at(o + 4), p.u32at(o + 8), chd));
                    }
                }
            }
            o += 4;
        }
    }
    // 3. vtables: dword == COL va, followed by code pointer
    let mut vts: HashMap<usize, Vec<Vt>> = HashMap::new();
    for s in &data_secs {
        let (a, b) = (s.raw as usize, (s.raw + s.rsize) as usize);
        let mut o = a;
        while o + 8 <= b {
            let v = p.u32at(o);
            if let Some(&(ti, off, _, _)) = cols.get(&v) {
                let first = p.u32at(o + 4);
                if p.is_code_va(first) {
                    let mut n = 0; let mut q = o + 4; let mut firsts = vec![];
                    while q + 4 <= b {
                        let f = p.u32at(q);
                        if !p.is_code_va(f) { break; }
                        if n > 0 && cols.contains_key(&p.u32at(q)) { break; }
                        if n > 0 && cols.contains_key(&p.u32at(q - 0)) { break; }
                        if firsts.len() < 8 { firsts.push(f); }
                        n += 1; q += 4;
                        if cols.contains_key(&p.u32at(q)) { break; } // next vtable's COL slot
                    }
                    vts.entry(ti).or_default().push(Vt { va: p.off2va(o + 4).unwrap(), col: v, offset: off, nmethods: n, first: firsts });
                }
            }
            o += 4;
        }
    }
    // chd per td (from any COL)
    let mut chd_of: HashMap<usize, u32> = HashMap::new();
    for (_, &(ti, _, _, chd)) in &cols { chd_of.entry(ti).or_insert(chd); }
    // 4. classes
    let mut out = vec![];
    let names: Vec<String> = tds.iter().map(|t| t.name.clone()).collect();
    for (ti, td) in tds.into_iter().enumerate() {
        let mut c = Class { td, vtables: vts.remove(&ti).unwrap_or_default(), chd: chd_of.get(&ti).copied(), chd_attr: 0, bases_flat: vec![], direct_bases: vec![] };
        c.vtables.sort_by_key(|v| v.offset);
        if let Some(chd) = c.chd {
            if let Some(co) = p.va2off(chd) {
                c.chd_attr = p.u32at(co + 4);
                let nb = p.u32at(co + 8) as usize;
                let bca = p.u32at(co + 12);
                if nb < 512 {
                    if let Some(bo) = p.va2off(bca) {
                        let mut flat = vec![];
                        for k in 0..nb {
                            let bcd = p.u32at(bo + 4 * k);
                            let Some(bdo) = p.va2off(bcd) else { break };
                            let btd = p.u32at(bdo);
                            let ncont = p.u32at(bdo + 4);
                            let mdisp = p.u32at(bdo + 8);
                            let nm = td_idx.get(&btd).map(|&i| names[i].clone()).unwrap_or(format!("?td@{:x}", btd));
                            flat.push((nm, ncont, mdisp));
                        }
                        // direct bases: children of root in pre-order with subtree sizes
                        let mut k = 1;
                        while k < flat.len() {
                            c.direct_bases.push(flat[k].0.clone());
                            k += 1 + flat[k].1 as usize;
                        }
                        c.bases_flat = flat;
                    }
                }
            }
        }
        out.push(c);
    }
    out
}

fn memfind(h: &[u8], n: &[u8]) -> Option<usize> { h.windows(n.len()).position(|w| w == n) }

pub fn top_ns(name: &str) -> String {
    // outermost scope, ignoring template args
    let mut depth = 0; let b = name.as_bytes();
    for i in 0..b.len() {
        match b[i] { b'<' => depth += 1, b'>' => depth -= 1, b':' if depth == 0 && i + 1 < b.len() && b[i + 1] == b':' => return name[..i].to_string(), _ => {} }
    }
    "(global)".into()
}

pub fn run(p: &Pe, outdir: &str) {
    let classes = find(p);
    let mut f = String::new();
    let _ = writeln!(f, "# RTTI classes recovered from Napoleon.exe by re_tools rtti (Rust). CONFIRMED byte-level structures.");
    let _ = writeln!(f, "# columns: demangled_name | TD_va | vtables[va(+offset,methods)] | direct_bases | all_bases(BCA order) | raw");
    let mut sorted: Vec<&Class> = classes.iter().collect();
    sorted.sort_by(|a, b| a.td.name.cmp(&b.td.name));
    for c in &sorted {
        let vt: Vec<String> = c.vtables.iter().map(|v| format!("0x{:08x}(+{},{}m)", v.va, v.offset, v.nmethods)).collect();
        let all: Vec<String> = c.bases_flat.iter().skip(1).map(|b| b.0.clone()).collect();
        let _ = writeln!(f, "{} | 0x{:08x} | {} | {} | {} | {}", c.td.name, c.td.va, vt.join(" "), c.direct_bases.join(", "), all.join(", "), c.td.raw);
    }
    std::fs::write(format!("{}/rtti_classes.txt", outdir), f).unwrap();
    // vtable tsv for Ghidra
    let mut t = String::new();
    for c in &sorted { for v in &c.vtables { let _ = writeln!(t, "0x{:08x}\t{}\t{}\t{}", v.va, v.nmethods, v.offset, c.td.name); } }
    std::fs::write(format!("{}/rtti_vtables.tsv", outdir), t).unwrap();
    // namespace summary
    let mut ns: BTreeMap<String, Vec<&Class>> = BTreeMap::new();
    for c in &sorted { ns.entry(top_ns(&c.td.name)).or_default().push(c); }
    let mut s = String::new();
    let _ = writeln!(s, "Total type descriptors: {}  with vtable: {}  with CHD: {}  demangle failures: {}", classes.len(), classes.iter().filter(|c| !c.vtables.is_empty()).count(), classes.iter().filter(|c| c.chd.is_some()).count(), classes.iter().filter(|c| !c.td.demangled_ok).count());
    let mut nsv: Vec<_> = ns.iter().collect();
    nsv.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (k, v) in &nsv { let _ = writeln!(s, "{:>6}  {}", v.len(), k); }
    std::fs::write(format!("{}/rtti_namespaces.txt", outdir), &s).unwrap();
    // inheritance trees for gameplay keywords
    let keys = ["UNIT", "ARMY", "CHARACTER", "FACTION", "REGION", "SETTLEMENT", "BATTLE", "CAMPAIGN", "AI", "FORMATION", "PATH", "NAVY", "FLEET", "SHIP", "AGENT", "GARRISON", "DIPLOMA", "BUILDING", "PROJECTILE", "SIEGE"];
    let mut tr = String::new();
    for c in &sorted {
        let up = c.td.name.to_ascii_uppercase();
        if !keys.iter().any(|k| up.contains(k)) { continue; }
        if c.td.name.starts_with("std::") { continue; }
        let _ = writeln!(tr, "{}  [vt: {}]", c.td.name, c.vtables.iter().map(|v| format!("0x{:x}/{}m", v.va, v.nmethods)).collect::<Vec<_>>().join(" "));
        // print BCA as tree using ncontained
        fn rec(tr: &mut String, flat: &[(String, u32, u32)], k: &mut usize, depth: usize) {
            let (n, nc, md) = &flat[*k];
            if depth > 0 { let _ = writeln!(tr, "{}<- {}{}", "    ".repeat(depth), n, if *md != 0 { format!(" (@+{})", md) } else { String::new() }); }
            *k += 1;
            let end = *k + *nc as usize;
            while *k < end && *k < flat.len() { rec(tr, flat, k, depth + 1); }
        }
        if !c.bases_flat.is_empty() { let mut k = 0; rec(&mut tr, &c.bases_flat, &mut k, 0); }
    }
    std::fs::write(format!("{}/rtti_gameplay_trees.txt", outdir), tr).unwrap();
    print!("{}", s.lines().take(60).collect::<Vec<_>>().join("\n"));
    println!();
}

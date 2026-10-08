//! Lightweight x86 cross-reference scanner (NOT a disassembler).
//! Finds imm32/disp32 occurrences of a target VA inside executable sections, guesses the
//! enclosing function start (16-byte aligned entry after CC/C3 padding), and summarises the
//! function's calls (E8 rel32), import calls (FF 15 [IAT]) and string references.
use crate::pe::Pe;
use std::collections::HashMap;
use std::fmt::Write as _;

pub struct Ctx<'a> { pub p: &'a Pe, pub iat: HashMap<u32, String>, pub strs: HashMap<u32, String>, text: (usize, usize) }

impl<'a> Ctx<'a> {
    pub fn new(p: &'a Pe) -> Self {
        let mut iat = HashMap::new();
        for (dll, fs) in p.imports() { for (f, rva) in fs { iat.insert((p.image_base as u32).wrapping_add(rva), format!("{}!{}", dll, f)); } }
        let mut strs = HashMap::new();
        for s in &p.sections {
            if s.ch & 0x20000000 != 0 || s.name == ".reloc" || s.name == ".rsrc" { continue; }
            let (a, b) = (s.raw as usize, (s.raw + s.rsize) as usize);
            let d = &p.d[a..b];
            let mut i = 0;
            while i < d.len() {
                // ascii
                if (0x20..0x7f).contains(&d[i]) && (i == 0 || d[i - 1] == 0) {
                    let st = i; while i < d.len() && (0x20..0x7f).contains(&d[i]) { i += 1; }
                    if i < d.len() && d[i] == 0 && i - st >= 4 { strs.insert(p.off2va(a + st).unwrap(), String::from_utf8_lossy(&d[st..i]).to_string()); continue; }
                    // try utf16 at st
                }
                if i + 1 < d.len() && (0x20..0x7f).contains(&d[i]) && d[i + 1] == 0 && (i < 2 || (d[i - 1] == 0 && d[i - 2] == 0)) && i % 2 == 0 {
                    let st = i; let mut s = String::from("L\"");
                    while i + 1 < d.len() && (0x20..0x7f).contains(&d[i]) && d[i + 1] == 0 { s.push(d[i] as char); i += 2; }
                    if (i - st) / 2 >= 4 { s.push('"'); strs.insert(p.off2va(a + st).unwrap(), s); continue; }
                }
                i += 1;
            }
        }
        let t = p.section(".text").unwrap();
        Ctx { p, iat, strs, text: (t.raw as usize, (t.raw + t.rsize) as usize) }
    }
    pub fn text_off_to_va(&self, o: usize) -> u32 { self.p.off2va(o).unwrap() }

    /// All offsets in .text where the 4 bytes == va, with a classification of the instruction
    pub fn refs_to(&self, va: u32) -> Vec<(usize, &'static str)> {
        let pat = va.to_le_bytes();
        let d = &self.p.d;
        let mut v = vec![];
        let (a, b) = self.text;
        let mut i = a;
        while i + 4 <= b {
            if d[i] == pat[0] && d[i + 1..i + 4] == pat[1..] {
                let k = match (d[i - 1], d[i - 2]) {
                    (0x68, _) => "push imm32",
                    (x, _) if (0xb8..=0xbf).contains(&x) => "mov r32,imm32",
                    (0x05, 0xc7) => "mov [abs],imm32",
                    (m, 0x8d) if m & 0xc7 == 0x05 => "lea r32,[disp32]",
                    (m, 0x8b) if m & 0xc7 == 0x05 => "mov r32,[disp32]",
                    (0xa1, _) => "mov eax,[moffs]",
                    (0xa3, _) => "mov [moffs],eax",
                    (m, 0x89) if m & 0xc7 == 0x05 => "mov [disp32],r32",
                    (0x15, 0xff) => "call [mem]",
                    (0x25, 0xff) => "jmp [mem]",
                    _ => {
                        // mov dword [esp+x]/[ebp+x], imm32 : C7 44 24 xx imm / C7 45 xx imm / C7 84 24 xx xx xx xx imm
                        if i >= 4 && d[i - 4] == 0xc7 && d[i - 3] == 0x44 && d[i - 2] == 0x24 { "mov [esp+x],imm32" }
                        else if d[i - 3] == 0xc7 && d[i - 2] == 0x45 { "mov [ebp+x],imm32" }
                        else if d[i - 3] == 0xc7 && (d[i - 2] & 0xc0) == 0x40 { "mov [r+x],imm32" }
                        else if d[i - 2] == 0xc7 && (d[i - 1] & 0xc0) == 0x00 { "mov [r],imm32" }
                        else { "imm32/disp32 (unclassified)" }
                    }
                };
                v.push((i, k));
            }
            i += 1;
        }
        v
    }

    /// guess function start for an offset in .text
    pub fn func_start(&self, off: usize) -> usize {
        let d = &self.p.d;
        let mut o = off;
        let lim = off.saturating_sub(0x20000).max(self.text.0);
        while o > lim {
            // candidate: 16-aligned va, previous byte CC (padding) or C3/C2 ret, and current not CC
            let va = self.text_off_to_va(o);
            if va & 0xf == 0 && d[o] != 0xcc && (d[o - 1] == 0xcc) { return o; }
            o -= 1;
        }
        lim
    }
    pub fn func_end(&self, start: usize) -> usize {
        let d = &self.p.d;
        let mut o = start + 1;
        while o + 2 < self.text.1 {
            if d[o] == 0xcc && d[o + 1] == 0xcc { return o; }
            // padding may be a single CC before the next 16-aligned function
            if d[o] == 0xcc && self.text_off_to_va(o + 1) & 0xf == 0 && (d[o - 1] == 0xc3 || d[o - 1] == 0xc2 || d[o - 3] == 0xc2) { return o; }
            o += 1;
            if o - start > 0x40000 { break; }
        }
        o
    }

    /// summary of a function: calls, imports, strings
    pub fn summarize(&self, start: usize, end: usize) -> (Vec<u32>, Vec<String>, Vec<(u32, String)>) {
        let d = &self.p.d;
        let mut calls = vec![]; let mut imps = vec![]; let mut ss = vec![];
        let mut i = start;
        while i + 5 <= end {
            if d[i] == 0xe8 {
                let rel = i32::from_le_bytes(d[i + 1..i + 5].try_into().unwrap());
                let tgt = (self.text_off_to_va(i + 5) as i64 + rel as i64) as u32;
                if self.p.is_code_va(tgt) && !calls.contains(&tgt) { calls.push(tgt); }
            }
            if d[i] == 0xff && (d[i + 1] == 0x15 || d[i + 1] == 0x25) && i + 6 <= end {
                let m = u32::from_le_bytes(d[i + 2..i + 6].try_into().unwrap());
                if let Some(n) = self.iat.get(&m) { if !imps.contains(n) { imps.push(n.clone()); } }
            }
            if i + 4 <= end {
                let m = u32::from_le_bytes(d[i..i + 4].try_into().unwrap());
                if let Some(s) = self.strs.get(&m) { if !ss.iter().any(|x: &(u32, String)| x.0 == m) { ss.push((m, s.clone())); } }
                if let Some(n) = self.iat.get(&m) { if !imps.contains(n) { imps.push(n.clone()); } } // mov esi,[IAT]
            }
            i += 1;
        }
        (calls, imps, ss)
    }

    pub fn describe(&self, start: usize, maxs: usize) -> String {
        let end = self.func_end(start);
        let (c, im, ss) = self.summarize(start, end);
        let mut w = String::new();
        let _ = writeln!(w, "  func 0x{:08x} (approx size 0x{:x}, prologue {:02x?}) calls={} imports=[{}]", self.text_off_to_va(start), end - start, &self.p.d[start..start + 6], c.len(), im.join(", "));
        for (va, s) in ss.iter().take(maxs) { let _ = writeln!(w, "     str 0x{:08x} {:?}", va, s.chars().take(140).collect::<String>()); }
        if ss.len() > maxs { let _ = writeln!(w, "     ... {} more strings", ss.len() - maxs); }
        w
    }

    pub fn find_string_vas(&self, needle: &str, exact: bool) -> Vec<(u32, String)> {
        let mut v: Vec<(u32, String)> = self.strs.iter().filter(|(_, s)| {
            let core = s.strip_prefix("L\"").and_then(|x| x.strip_suffix('"')).unwrap_or(s);
            if exact { core == needle } else { core.contains(needle) }
        }).map(|(a, s)| (*a, s.clone())).collect();
        v.sort();
        v
    }

    pub fn callers_of(&self, va: u32) -> Vec<usize> {
        let d = &self.p.d; let (a, b) = self.text; let mut v = vec![];
        let mut i = a;
        while i + 5 <= b {
            if d[i] == 0xe8 || d[i] == 0xe9 {
                let rel = i32::from_le_bytes(d[i + 1..i + 5].try_into().unwrap());
                if (self.text_off_to_va(i + 5) as i64 + rel as i64) as u32 == va { v.push(i); }
            }
            i += 1;
        }
        v
    }
}

/// xref <exe> <out> <mode:sub|exact> <needle>...
pub fn run(p: &Pe, out: &str, exact: bool, needles: &[String]) {
    let cx = Ctx::new(p);
    let mut w = String::new();
    for n in needles {
        let hits = cx.find_string_vas(n, exact);
        let _ = writeln!(w, "\n==== needle {:?}: {} string(s)", n, hits.len());
        for (va, s) in hits.iter().take(12) {
            let refs = cx.refs_to(*va);
            let _ = writeln!(w, "-- string 0x{:08x} {:?} : {} code ref(s)", va, s.chars().take(120).collect::<String>(), refs.len());
            let mut seen = vec![];
            for (o, kind) in refs.iter().take(8) {
                let fs = cx.func_start(*o);
                let _ = writeln!(w, "   ref at 0x{:08x} [{}] -> func 0x{:08x}", cx.text_off_to_va(*o), kind, cx.text_off_to_va(fs));
                if !seen.contains(&fs) { seen.push(fs); w.push_str(&cx.describe(fs, 25)); }
            }
        }
    }
    std::fs::OpenOptions::new().create(true).append(true).open(out).and_then(|mut f| { use std::io::Write; f.write_all(w.as_bytes()) }).unwrap();
    print!("{}", w);
}

/// func <exe> <va>... : describe function containing va plus its callers
pub fn run_func(p: &Pe, vas: &[u32]) {
    let cx = Ctx::new(p);
    for &va in vas {
        let off = p.va2off(va).unwrap();
        let fs = cx.func_start(off);
        print!("{}", cx.describe(fs, 60));
        let fva = cx.text_off_to_va(fs);
        let callers = cx.callers_of(fva);
        println!("   callers of 0x{:08x}: {}", fva, callers.len());
        for c in callers.iter().take(10) { let cs = cx.func_start(*c); println!("     from 0x{:08x} in func 0x{:08x}", cx.text_off_to_va(*c), cx.text_off_to_va(cs)); }
    }
}

/// imm <exe> <hexvalue>... : find raw 32-bit constants in executable code, report enclosing funcs
pub fn run_imm(p: &Pe, vals: &[u32]) {
    let cx = Ctx::new(p);
    for &v in vals {
        let refs = cx.refs_to(v);
        println!("== constant 0x{:08x} ({}) : {} hit(s) in .text", v, v as i32, refs.len());
        let mut seen = vec![];
        for (o, kind) in refs.iter().take(100000) {
            let fs = cx.func_start(*o);
            println!("   at 0x{:08x} [{}] func 0x{:08x}", cx.text_off_to_va(*o), kind, cx.text_off_to_va(fs));
            if !seen.contains(&fs) { seen.push(fs); }
        }
    }
}

/// iat <exe> <hexva>... : name the import at an IAT slot VA
pub fn run_iat(p: &Pe, vals: &[u32]) {
    let cx = Ctx::new(p);
    for v in vals { println!("0x{:08x} -> {}", v, cx.iat.get(v).cloned().unwrap_or("?".into())); }
}

/// impref <exe> <name>... : find IAT slot for import name(s) and list code refs (call [IAT] / mov reg,[IAT])
pub fn run_impref(p: &Pe, names: &[String]) {
    let cx = Ctx::new(p);
    for n in names {
        for (va, full) in cx.iat.iter().filter(|(_, f)| f.ends_with(&format!("!{}", n))) {
            let refs = cx.refs_to(*va);
            println!("== {} IAT 0x{:08x}: {} refs", full, va, refs.len());
            let mut seen = vec![];
            for (o, k) in refs.iter().take(40) {
                let fs = cx.func_start(*o);
                if !seen.contains(&fs) { seen.push(fs); println!("   0x{:08x} [{}] func~0x{:08x}", cx.text_off_to_va(*o), k, cx.text_off_to_va(fs)); }
            }
        }
    }
}

/// tweakuse <exe> <tweakers.tsv> <funcVA>... : in-order list of `mov ecx,OBJ ... call` where OBJ is a TWEAKER object
pub fn run_tweakuse(p: &Pe, tsv: &str, vas: &[u32]) {
    let cx = Ctx::new(p);
    let mut map = std::collections::HashMap::new();
    for l in std::fs::read_to_string(tsv).unwrap().lines() {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() > 6 && c[5].starts_with("0x") {
            if let Ok(o) = u32::from_str_radix(c[5].trim_start_matches("0x"), 16) { map.insert(o, format!("{} (default {})", c[0], c[3])); }
        }
    }
    let d = &p.d;
    for &va in vas {
        let off = p.va2off(va).unwrap();
        let fs = cx.func_start(off);
        let fe = cx.func_end(fs);
        println!("== func 0x{:08x}..0x{:08x}", cx.text_off_to_va(fs), cx.text_off_to_va(fe));
        let mut i = fs;
        while i + 5 < fe {
            if d[i] == 0xb9 {
                let o = u32::from_le_bytes(d[i + 1..i + 5].try_into().unwrap());
                if let Some(n) = map.get(&o) {
                    // find next call
                    let mut k = i + 5; let mut tgt = 0;
                    while k < i + 24 { if d[k] == 0xe8 { let rel = i32::from_le_bytes(d[k + 1..k + 5].try_into().unwrap()); tgt = (cx.text_off_to_va(k + 5) as i64 + rel as i64) as u32; break; } k += 1; }
                    println!("   0x{:08x}: this=0x{:08x} {}  -> call 0x{:x}", cx.text_off_to_va(i), o, n, tgt);
                }
            }
            i += 1;
        }
    }
}

/// findptr <exe> <hexva>... : find 4-byte occurrences of value in non-code sections (vtables, tables)
pub fn run_findptr(p: &Pe, vals: &[u32]) {
    for &v in vals {
        let pat = v.to_le_bytes();
        print!("0x{:08x}:", v);
        for s in &p.sections {
            if s.ch & 0x20000000 != 0 { continue; }
            let (a, b) = (s.raw as usize, (s.raw + s.rsize) as usize);
            let mut i = a;
            while i + 4 <= b { if p.d[i..i + 4] == pat { print!(" {}@0x{:08x}", s.name, p.off2va(i).unwrap()); } i += 4; }
        }
        println!();
    }
}

/// storescan <exe> <disp>... : find functions that store (mov [r32+disp32], r32) to ALL given displacements
pub fn run_storescan(p: &Pe, disps: &[u32]) {
    let cx = Ctx::new(p);
    let t = p.section(".text").unwrap();
    let (a, b) = (t.raw as usize, (t.raw + t.rsize) as usize);
    let d = &p.d;
    let mut hits: std::collections::BTreeMap<usize, std::collections::BTreeSet<u32>> = Default::default();
    let mut i = a;
    while i + 6 < b {
        if (d[i] == 0x89 || d[i] == 0xc7) && (d[i + 1] & 0xc0) == 0x80 && (d[i + 1] & 7) != 4 {
            let disp = u32::from_le_bytes(d[i + 2..i + 6].try_into().unwrap());
            if disps.contains(&disp) { let fs = cx.func_start(i); hits.entry(fs).or_default().insert(disp); }
        }
        i += 1;
    }
    for (f, s) in hits { if s.len() == disps.len() { println!("func 0x{:08x}", cx.text_off_to_va(f)); } }
}

/// vcall <exe> <hexdisp> : find `call dword ptr [r32+disp32]` (FF 90..97 disp) sites and their functions
pub fn run_vcall(p: &Pe, disp: u32) {
    let cx = Ctx::new(p);
    let t = p.section(".text").unwrap();
    let (a, b) = (t.raw as usize, (t.raw + t.rsize) as usize);
    let d = &p.d;
    let pat = disp.to_le_bytes();
    let mut fs = std::collections::BTreeMap::new();
    let mut i = a;
    while i + 6 < b {
        if d[i] == 0xff && (0x90..=0x97).contains(&d[i + 1]) && d[i + 1] != 0x94 && d[i + 2..i + 6] == pat {
            *fs.entry(cx.func_start(i)).or_insert(0) += 1;
        }
        i += 1;
    }
    for (f, n) in fs { println!("func 0x{:08x} x{}", cx.text_off_to_va(f), n); }
}

//! KV table loaders register one TWEAKER per key inside a holder object:
//!   push edi ; push esi ; push KEY ; lea ecx,[edi+DISP] ; call ADD   (CONFIRMED at 0xf42950)
//! ADD = 0xf3a830 -> int tweaker (value truncated from float), 0xf3a9b0 -> float tweaker.
use crate::pe::Pe;
use crate::xref::Ctx;
use std::fmt::Write as _;

pub fn run(p: &Pe, outdir: &str, funcs: &[(String, u32)]) {
    let cx = Ctx::new(p);
    let d = &p.d;
    let rd = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut w = String::new();
    let _ = writeln!(w, "# KV holder layout recovered by re_tools kvmap (Rust). table\tindex\tkey\tholder_offset\tvalue_type");
    for (table, va) in funcs {
        let fs = p.va2off(*va).unwrap();
        let fe = cx.func_end(fs);
        let mut i = fs; let mut last_key: Option<String> = None; let mut idx = 0;
        while i + 10 < fe {
            if d[i] == 0x68 { if let Some(s) = cx.strs.get(&rd(i + 1)) { last_key = Some(s.clone()); } i += 5; continue; }
            // lea ecx,[edi+disp8] = 8D 4F xx ; lea ecx,[edi+disp32] = 8D 8F xx xx xx xx
            let (disp, len) = if d[i] == 0x8d && d[i + 1] == 0x4f { (d[i + 2] as u32, 3) } else if d[i] == 0x8d && d[i + 1] == 0x8f { (rd(i + 2), 6) } else { (u32::MAX, 0) };
            let mut cpos = 0usize; if disp != u32::MAX { let mut q = i + len; while q < i + len + 28 { if d[q] == 0xe8 { cpos = q; break; } if d[q] == 0x68 { break; } q += 1; } }
            if disp != u32::MAX && cpos != 0 {
                let len = cpos - i;
                let rel = rd(i + len + 1) as i32;
                let tgt = (cx.text_off_to_va(i + len + 5) as i64 + rel as i64) as u32;
                let ty = match tgt { 0xf3a830 => "int(trunc)", 0xf3a9b0 => "float", _ => "other" };
                if let Some(k) = last_key.take() {
                    let _ = writeln!(w, "{}\t{}\t{}\t0x{:x}\t{}\t(add=0x{:x})", table, idx, k, disp, ty, tgt);
                    idx += 1;
                }
                i += len + 5; continue;
            }
            i += 1;
        }
    }
    std::fs::write(format!("{}/kv_layout.tsv", outdir), &w).unwrap();
    print!("{}", w);
}

/// kvuse <exe> <outdir> <kv_layout.tsv> : for every call to a KV getter, find the following
/// `lea ecx,[eax+DISP]` / `lea reg,[eax+DISP]` / `add eax,DISP` and map DISP to the key. Writes kv_usage.tsv
pub fn run_use(p: &Pe, outdir: &str, layout: &str) {
    let cx = Ctx::new(p);
    let getters: [(u32, &str); 4] = [(0xe20560, "kv_rules"), (0xe203c0, "kv_morale"), (0xe202f0, "kv_fatigue"), (0xe20490, "kv_naval_morale")];
    let mut keys = std::collections::HashMap::new();
    for l in std::fs::read_to_string(layout).unwrap().lines().skip(1) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() >= 5 { keys.insert((c[0].to_string(), u32::from_str_radix(c[3].trim_start_matches("0x"), 16).unwrap()), (c[2].to_string(), c[4].to_string())); }
    }
    let d = &p.d;
    let rd = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut rows: Vec<(String, String, String, u32, u32)> = vec![];
    let mut w = String::new();
    let _ = writeln!(w, "# KV key usage index (re_tools kvuse, Rust). table\tkey\ttype\tsite\tfunc_guess");
    for (g, table) in getters.iter() {
        for c in cx.callers_of(*g) {
            // scan forward for lea r32,[eax+disp] or add eax,imm
            let mut q = c + 5; let lim = c + 5 + 48;
            let mut found = 0;
            while q < lim && found < 4 {
                let (disp, len) = if d[q] == 0x8d && (d[q + 1] & 0xc7) == 0x40 && (d[q + 1] & 0x07) == 0 { (d[q + 2] as u32, 3) }
                    else if d[q] == 0x8d && (d[q + 1] & 0xc7) == 0x80 && (d[q + 1] & 0x07) == 0 { (rd(q + 2), 6) }
                    else if d[q] == 0x05 { (rd(q + 1), 5) }
                    else if d[q] == 0x83 && d[q + 1] == 0xc0 { (d[q + 2] as u32, 3) }
                    else { (u32::MAX, 1) };
                if disp != u32::MAX {
                    if let Some((k, ty)) = keys.get(&(table.to_string(), disp)) {
                        let fs = cx.func_start(c);
                        rows.push((table.to_string(), k.clone(), ty.clone(), cx.text_off_to_va(q), cx.text_off_to_va(fs)));
                        found += 1;
                    }
                }
                if d[q] == 0xe8 && found > 0 { break; }
                q += len;
            }
        }
    }
    rows.sort();
    for (t, k, ty, s, f) in &rows { let _ = writeln!(w, "{}\t{}\t{}\t0x{:08x}\t0x{:08x}", t, k, ty, s, f); }
    std::fs::write(format!("{}/kv_usage.tsv", outdir), &w).unwrap();
    println!("kv usages: {}", rows.len());
}

/// getteruse: calls to TWEAKER getters (int 0x586430, float 0x45c180) whose `this` is
/// `lea ecx,[reg+DISP]` with DISP = 0x10 + 0x60*k  (KV holder slot) -> candidate keys per table
pub fn run_getteruse(p: &Pe, outdir: &str, layout: &str) {
    let cx = Ctx::new(p);
    let mut keys: std::collections::BTreeMap<u32, Vec<String>> = std::collections::BTreeMap::new();
    for l in std::fs::read_to_string(layout).unwrap().lines().skip(1) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() >= 5 { keys.entry(u32::from_str_radix(c[3].trim_start_matches("0x"), 16).unwrap()).or_default().push(format!("{}:{}", c[0], c[2])); }
    }
    let d = &p.d;
    let rd = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut w = String::new();
    let _ = writeln!(w, "# KV slot reads via TWEAKER getters (re_tools getteruse). site\tfunc_guess\tgetter\tdisp\tcandidates");
    let mut n = 0;
    for (g, gname) in [(0x586430u32, "int_get"), (0x45c180u32, "float_get")] {
        for c in cx.callers_of(g) {
            // look back up to 12 bytes for lea ecx,[r32+disp32] (8D 88..8F) or lea ecx,[r+disp8] (8D 48..4F) or add ecx,imm32 (81 C1)
            let mut disp = None;
            for back in 3..=12 {
                let q = c - back;
                if d[q] == 0x8d && (d[q + 1] & 0xf8) == 0x88 && back >= 6 { disp = Some(rd(q + 2)); break; }
                if d[q] == 0x8d && (d[q + 1] & 0xf8) == 0x48 { disp = Some(d[q + 2] as u32); break; }
                if d[q] == 0x81 && d[q + 1] == 0xc1 && back >= 6 { disp = Some(rd(q + 2)); break; }
                if d[q] == 0x83 && d[q + 1] == 0xc1 { disp = Some(d[q + 2] as u32); break; }
            }
            if let Some(ds) = disp {
                if ds >= 0x10 && (ds - 0x10) % 0x60 == 0 && ds <= 0x2400 {
                    let fs = cx.func_start(c);
                    let cand = keys.get(&ds).map(|v| v.join(" | ")).unwrap_or_default();
                    let _ = writeln!(w, "0x{:08x}\t0x{:08x}\t{}\t0x{:x}\t{}", cx.text_off_to_va(c), cx.text_off_to_va(fs), gname, ds, cand);
                    n += 1;
                }
            }
        }
    }
    std::fs::write(format!("{}/kv_getter_reads.tsv", outdir), &w).unwrap();
    println!("kv slot reads: {}", n);
}

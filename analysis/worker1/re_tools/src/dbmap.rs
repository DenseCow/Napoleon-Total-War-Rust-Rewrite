//! Map DB table names to their row-reader callbacks.
//! Pattern (CONFIRMED at 0xE86640): push "<name>_tables" ; call str_ctor ; push CALLBACK ; ... ; call TABLE_LOAD
//! CALLBACK is a tiny thunk: push [esp+8]; mov ecx,[esp+8]; call READER ; ret
use crate::pe::Pe;
use crate::xref::Ctx;
use std::fmt::Write as _;

pub fn run(p: &Pe, outdir: &str) {
    let cx = Ctx::new(p);
    let d = &p.d;
    let rd = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut names: Vec<(u32, String)> = cx.strs.iter().filter(|(_, s)| s.ends_with("_tables") && !s.contains(' ')).map(|(a, s)| (*a, s.clone())).collect();
    names.sort_by(|a, b| a.1.cmp(&b.1));
    let mut w = String::new();
    let _ = writeln!(w, "table\tname_getter\tcallback\treader\ttable_load\tlinker_site");
    let mut n = 0;
    for (va, name) in &names {
        let refs = cx.refs_to(*va);
        let mut cb = 0u32; let mut reader = 0u32; let mut load = 0u32; let mut getter = 0u32; let mut other = vec![];
        for (o, kind) in &refs {
            if *kind != "push imm32" { continue; }
            let fs = cx.func_start(*o);
            let mut q = *o + 5; let mut found_cb = 0u32; let mut ld = 0u32;
            while q < *o + 48 {
                if d[q] == 0x68 && found_cb == 0 { let v = rd(q + 1); if p.is_code_va(v) { found_cb = v; } q += 5; continue; }
                if d[q] == 0xe8 && found_cb != 0 { let rel = rd(q + 1) as i32; ld = (cx.text_off_to_va(q + 5) as i64 + rel as i64) as u32; break; }
                q += 1;
            }
            if found_cb != 0 {
                cb = found_cb; load = ld; getter = cx.text_off_to_va(fs);
                // decode thunk: find first call
                if let Some(co) = p.va2off(cb) {
                    let mut k = co; while k < co + 24 { if d[k] == 0xe8 { let rel = rd(k + 1) as i32; reader = (cx.text_off_to_va(k + 5) as i64 + rel as i64) as u32; break; } k += 1; }
                }
            } else { other.push(format!("0x{:x}", cx.text_off_to_va(fs))); }
        }
        if cb != 0 { n += 1; }
        let _ = writeln!(w, "{}\t0x{:x}\t0x{:x}\t0x{:x}\t0x{:x}\t{}", name, getter, cb, reader, load, other.join(","));
    }
    std::fs::write(format!("{}/db_readers.tsv", outdir), &w).unwrap();
    println!("tables {} with reader {}", names.len(), n);
}

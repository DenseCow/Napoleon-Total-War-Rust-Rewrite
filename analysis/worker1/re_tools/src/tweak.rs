//! Recover TWEAKER<T> static registrations (name, category, description, default value).
//! Code pattern (CONFIRMED at 0x417810):
//!   push 0 ; push desc ; push name ; push category ; push LINE ; push FILE(.cpp) ;
//!   lea eax,[esp+X] ; mov dword [esp+X], DEFAULT ; push eax ; push name ; mov ecx, OBJ ; call CTOR
use crate::pe::Pe;
use crate::xref::Ctx;
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub fn run(p: &Pe, outdir: &str) {
    let cx = Ctx::new(p);
    let t = p.section(".text").unwrap();
    let (a, b) = (t.raw as usize, (t.raw + t.rsize) as usize);
    let d = &p.d;
    let rd = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
    let mut rows = vec![];
    let mut ctor_count: BTreeMap<u32, usize> = BTreeMap::new();
    let mut i = a;
    while i + 40 < b {
        // push imm32 x3
        if d[i] == 0x68 && d[i + 5] == 0x68 && d[i + 10] == 0x68 {
            let (s1, s2, s3) = (rd(i + 1), rd(i + 6), rd(i + 11));
            let mut j = i + 15;
            let line;
            if d[j] == 0x6a { line = d[j + 1] as u32; j += 2; } else if d[j] == 0x68 { line = rd(j + 1); j += 5; } else { i += 1; continue; }
            if d[j] != 0x68 { i += 1; continue; }
            let f = rd(j + 1);
            let file = cx.strs.get(&f).cloned().unwrap_or_default();
            if !file.ends_with(".cpp") && !file.ends_with(".h") { i += 1; continue; }
            j += 5;
            let (Some(desc), Some(name), Some(cat)) = (cx.strs.get(&s1), cx.strs.get(&s2), cx.strs.get(&s3)) else { i += 1; continue };
            // scan forward for default value, object and ctor
            let mut val: Option<(u32, u8)> = None; // (raw, width)
            let mut obj = 0u32; let mut ctor = 0u32;
            let mut k = j;
            while k < j + 48 {
                if d[k] == 0xc7 && d[k + 1] == 0x44 && d[k + 2] == 0x24 && val.is_none() { val = Some((rd(k + 4), 4)); k += 8; continue; }
                if d[k] == 0xc6 && d[k + 1] == 0x44 && d[k + 2] == 0x24 && val.is_none() { val = Some((d[k + 4] as u32, 1)); k += 5; continue; }
                if d[k] == 0x68 { k += 5; continue; }
                if d[k] == 0xb9 { obj = rd(k + 1); k += 5; continue; }
                if d[k] == 0xe8 { let rel = rd(k + 1) as i32; ctor = (cx.text_off_to_va(k + 5) as i64 + rel as i64) as u32; break; }
                k += 1;
            }
            *ctor_count.entry(ctor).or_default() += 1;
            rows.push((name.clone(), cat.clone(), desc.clone(), line, file, val, obj, ctor, cx.text_off_to_va(i)));
            i = j;
            continue;
        }
        i += 1;
    }
    let mut w = String::new();
    let _ = writeln!(w, "# TWEAKER registrations recovered by re_tools tweak (Rust). value_as_float shown when width=4.");
    let _ = writeln!(w, "# ctor histogram (ctor VA -> count): {:?}", ctor_count.iter().map(|(k, v)| format!("0x{:x}:{}", k, v)).collect::<Vec<_>>());
    let _ = writeln!(w, "name\tcategory\tdefault_raw\tdefault_f32\tctor\tobject\tinit_func\tfile:line\tdescription");
    rows.sort_by(|x, y| x.1.cmp(&y.1).then(x.0.cmp(&y.0)));
    for (name, cat, desc, line, file, val, obj, ctor, at) in &rows {
        let (raw, fl) = match val { Some((v, 4)) => (format!("0x{:08x}", v), format!("{}", f32::from_bits(*v))), Some((v, _)) => (format!("byte {}", v), String::new()), None => ("?".into(), String::new()) };
        let ty = match *ctor { 0x454300 => "pref_int", 0x454380 => "pref_float", 0x454400 => "pref_string", 0x454480 => "pref_bool", 0x454670 => "tweak_int", 0x454730 => "tweak_float", 0x4548c0 => "tweak_bool", 0x4545b0 => "tweak_int(-1 default)", _ => "unknown" };
        let fl = if ty.ends_with("float") { fl } else if let Some((v, 4)) = val { format!("{}", *v as i32) } else { fl };
        let short = file.rsplit(char::from(92u8)).next().unwrap_or(file);
        let _ = writeln!(w, "{}\t{}\t{}\t{}\t0x{:x}\t0x{:x}\t0x{:x}\t{}:{}\t{}", name, cat, raw, fl, ctor, obj, at, short, line, desc);
    }
    std::fs::write(format!("{}/tweakers.tsv", outdir), &w).unwrap();
    println!("tweakers: {}  ctors: {}", rows.len(), ctor_count.len());
    for (k, v) in &ctor_count { println!("  ctor 0x{:x}: {}", k, v); }
}

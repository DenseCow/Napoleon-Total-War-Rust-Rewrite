//! Recover script-binding registrations: push DESC ; push NAME ; push FUNC(code) ; mov ecx,OBJ ; call REG
//! (pattern CONFIRMED at 0x40bcc0 for "TickPeriod").
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
    let mut regs: BTreeMap<u32, usize> = BTreeMap::new();
    let mut i = a;
    while i + 26 < b {
        if d[i] == 0x68 && d[i + 5] == 0x68 && d[i + 10] == 0x68 && d[i + 15] == 0xb9 && d[i + 20] == 0xe8 {
            let (s1, s2, f, obj) = (rd(i + 1), rd(i + 6), rd(i + 11), rd(i + 16));
            if let (Some(desc), Some(name)) = (cx.strs.get(&s1), cx.strs.get(&s2)) {
                if p.is_code_va(f) {
                    let rel = rd(i + 21) as i32;
                    let reg = (cx.text_off_to_va(i + 25) as i64 + rel as i64) as u32;
                    *regs.entry(reg).or_default() += 1;
                    rows.push((name.clone(), f, obj, reg, desc.clone(), cx.text_off_to_va(i)));
                    i += 25; continue;
                }
            }
        }
        i += 1;
    }
    // group by registrar (=> which script interface table)
    let mut w = String::new();
    let _ = writeln!(w, "# Script (Lua) bindings recovered by re_tools luabind (Rust): name, C handler VA, binding table object, registrar, description");
    let _ = writeln!(w, "# registrar histogram: {:?}", regs.iter().map(|(k, v)| format!("0x{:x}:{}", k, v)).collect::<Vec<_>>());
    let _ = writeln!(w, "name\thandler\tobject\tregistrar\tsite\tdescription");
    rows.sort_by(|x, y| x.2.cmp(&y.2).then(x.0.cmp(&y.0)));
    for (n, f, o, r, ds, s) in &rows { let _ = writeln!(w, "{}\t0x{:08x}\t0x{:08x}\t0x{:x}\t0x{:x}\t{}", n, f, o, r, s, ds.replace('\t', " ")); }
    std::fs::write(format!("{}/script_bindings.tsv", outdir), &w).unwrap();
    let mut objs: BTreeMap<u32, usize> = BTreeMap::new();
    for r in &rows { *objs.entry(r.2).or_default() += 1; }
    println!("bindings: {} objects: {}", rows.len(), objs.len());
    for (k, v) in &regs { println!("  registrar 0x{:x}: {}", k, v); }
}

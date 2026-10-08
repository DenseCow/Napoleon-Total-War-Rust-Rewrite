//! Turn ghidra_out/db_fields_raw.tsv (DbReaderScan.java output) into per-table binary row layouts.
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Clone)]
struct Row { reader: String, addr: String, prim: String, dst: String, size: String, guard: String }

pub struct Field { pub ty: String, pub bytes: String, pub off: String, pub guard: String, pub note: String, pub addr: String }

pub fn layout(rows: &[Row]) -> Vec<Field> {
    let mut out = vec![];
    let mut i = 0;
    while i < rows.len() {
        let r = &rows[i];
        let off = if r.dst.starts_with("this+") { r.dst[5..].to_string() } else { r.dst.clone() };
        match r.prim.as_str() {
            "byte" if r.dst == "local" => {
                if i + 1 < rows.len() && rows[i + 1].prim == "string" && rows[i + 1].guard == r.guard {
                    let s = &rows[i + 1];
                    let o2 = if s.dst.starts_with("this+") { s.dst[5..].to_string() } else { s.dst.clone() };
                    out.push(Field { ty: "optional_string".into(), bytes: "u8 flag; if flag!=0: u16 len + len*UTF-16LE".into(), off: o2, guard: r.guard.clone(), note: "flag==0 -> empty string".into(), addr: r.addr.clone() });
                    i += 2; continue;
                }
                out.push(Field { ty: "u8 (not stored)".into(), bytes: "1".into(), off: "-".into(), guard: r.guard.clone(), note: "read into a local".into(), addr: r.addr.clone() });
            }
            "byte" => out.push(Field { ty: "bool".into(), bytes: "1".into(), off, guard: r.guard.clone(), note: String::new(), addr: r.addr.clone() }),
            "u16" if r.dst == "local" => out.push(Field { ty: "string".into(), bytes: "u16 len + len*UTF-16LE".into(), off: if i == 0 { "0x0".into() } else { "?".into() }, guard: r.guard.clone(), note: "string read inlined".into(), addr: r.addr.clone() }),
            "u16" => out.push(Field { ty: "u16".into(), bytes: "2".into(), off, guard: r.guard.clone(), note: String::new(), addr: r.addr.clone() }),
            "string" => {
                let o = if r.dst.starts_with("this+") { off } else if out.is_empty() { "0x0".into() } else { "?".into() };
                let note = if r.dst.starts_with("this+") { String::new() } else { "destination offset inferred".into() };
                out.push(Field { ty: "string".into(), bytes: "u16 len + len*UTF-16LE".into(), off: o, guard: r.guard.clone(), note, addr: r.addr.clone() });
            }
            "raw" => {
                let sz = r.size.replace("0+0x", "").replace("0x", "");
                let n = u32::from_str_radix(&sz, 16).unwrap_or(0);
                out.push(Field { ty: if n == 4 { "int32 or float32".into() } else { format!("raw{}", n) }, bytes: n.to_string(), off, guard: r.guard.clone(), note: String::new(), addr: r.addr.clone() });
            }
            p if p.starts_with("call:") => {
                let what = match p { "call:FUN_004f3720" => "derived: parse previous string as int into", "call:FUN_004f1200" | "call:FUN_004f0660" | "call:FUN_004f0240" => "derived: string copy into", _ => "derived/nested call writing" };
                out.push(Field { ty: "(no file bytes)".into(), bytes: "0".into(), off: r.dst.replace("this+", ""), guard: r.guard.clone(), note: format!("{} ({})", what, p), addr: r.addr.clone() });
            }
            _ => {}
        }
        i += 1;
    }
    out
}

pub fn run(raw: &str, readers: &str, out_md: &str, priority: &[String]) {
    let mut by: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for l in std::fs::read_to_string(raw).unwrap().lines() {
        if l.starts_with('#') { continue; }
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() < 7 { continue; }
        by.entry(c[0].to_string()).or_default().push(Row { reader: c[1].into(), addr: c[3].into(), prim: c[4].into(), dst: c[5].into(), size: c[6].into(), guard: c.get(7).unwrap_or(&"").to_string() });
    }
    let mut meta: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in std::fs::read_to_string(readers).unwrap().lines().skip(1) {
        let c: Vec<&str> = l.split('\t').collect();
        meta.insert(c[0].to_string(), c.iter().map(|s| s.to_string()).collect());
    }
    // shared readers
    let mut shared: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (t, rows) in &by { shared.entry(rows[0].reader.clone()).or_default().push(t.clone()); }
    let mut w = String::new();
    let mut emit = |w: &mut String, t: &str, rows: &Vec<Row>| {
        let m = meta.get(t).cloned().unwrap_or_default();
        let f = layout(rows);
        let filebytes: Vec<&Field> = f.iter().filter(|x| x.bytes != "0").collect();
        let versioned = f.iter().any(|x| !x.guard.is_empty());
        let _ = writeln!(w, "\n### {}\n", t);
        let _ = writeln!(w, "- Row reader **0x{}** (callback {}, table loader {}, name getter {}). Fields read from the file: {}.{}",
            rows[0].reader.trim_start_matches("00"), m.get(2).cloned().unwrap_or_default(), m.get(4).cloned().unwrap_or_default(), m.get(1).cloned().unwrap_or_default(), filebytes.len(),
            if versioned { " **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard)." } else { "" });
        let sh = shared.get(&rows[0].reader).map(|v| v.iter().filter(|x| x.as_str() != t).cloned().collect::<Vec<_>>()).unwrap_or_default();
        if !sh.is_empty() { let _ = writeln!(w, "- The same reader (and therefore the same row layout) is used by: {}", sh.join(", ")); }
        let _ = writeln!(w, "\n| # | type | bytes in file | BUILDER offset | version guard | note | read site |\n|---|---|---|---|---|---|---|");
        let mut n = 0;
        for x in &f {
            let idx = if x.bytes == "0" { "-".to_string() } else { n += 1; (n - 1).to_string() };
            let _ = writeln!(w, "| {} | {} | {} | {} | {} | {} | 0x{} |", idx, x.ty, x.bytes, x.off, pretty_guard(&x.guard), x.note, x.addr.trim_start_matches("00"));
        }
    };
    let _ = writeln!(w, "\n## Priority tables\n");
    for p in priority { if let Some(r) = by.get(p) { emit(&mut w, p, r); } else { let _ = writeln!(w, "\n### {}\n\n- No row reader was found (the table name is not loaded through the standard DATABASE_TABLE path).", p); } }
    let _ = writeln!(w, "\n## All other tables (automatic, same method)\n");
    for (t, r) in &by { if !priority.contains(t) { emit(&mut w, t, r); } }
    let mut prev = std::fs::read_to_string(out_md).unwrap_or_default();
    if let Some(k) = prev.find("\n## Priority tables") { prev.truncate(k); }
    std::fs::write(out_md, prev + &w).unwrap();
    println!("tables {}", by.len());
}

pub fn pretty_guard(g: &str) -> String {
    if g.is_empty() { return String::new(); }
    let mut parts = vec![];
    for p in g.split(']').filter(|s| !s.is_empty()) {
        let p = p.trim_start_matches('[');
        let s = match p {
            _ if p.starts_with("!(ver==") => format!("version != {}", &p[7..p.len() - 1]),
            _ if p.starts_with("!(ver<") => format!("version >= {}", &p[6..p.len() - 1]),
            _ if p.starts_with("!(ver!=") => format!("version == {}", &p[7..p.len() - 1]),
            _ if p.starts_with("ver==") => format!("version == {}", &p[5..]),
            _ if p.starts_with("ver!=") => format!("version != {}", &p[5..]),
            _ if p.starts_with("ver<") => format!("version < {}", &p[4..]),
            _ if p.ends_with("<ver") => format!("version > {}", &p[..p.len() - 4]),
            _ if p.starts_with("!(") && p.ends_with("<ver)") => format!("version <= {}", &p[2..p.len() - 5]),
            _ => p.to_string(),
        };
        parts.push(s);
    }
    parts.join(" and ")
}

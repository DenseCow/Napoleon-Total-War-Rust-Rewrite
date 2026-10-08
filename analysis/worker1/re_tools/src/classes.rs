//! Harvest C++ qualified type names (NS::NAME) from __FUNCSIG__/__FUNCTION__ and
//! diagnostic strings. Used because Napoleon.exe game code is built without RTTI.
use crate::pe::Pe;
use crate::strings::extract;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

fn is_id(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }

/// returns all maximal qualified identifiers containing at least one "::"
pub fn qualified_names(s: &str) -> Vec<String> {
    let b: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if is_id(b[i]) && (i == 0 || !is_id(b[i - 1])) {
            let st = i;
            let mut j = i;
            loop {
                while j < b.len() && is_id(b[j]) { j += 1; }
                if j + 2 < b.len() && b[j] == ':' && b[j + 1] == ':' && is_id(b[j + 2]) { j += 2; continue; }
                break;
            }
            let t: String = b[st..j].iter().collect();
            if t.contains("::") && !t.starts_with(|c: char| c.is_ascii_digit()) { out.push(t); }
            i = j.max(i + 1);
        } else { i += 1; }
    }
    out
}

pub fn run(p: &Pe, outdir: &str) {
    let v = extract(p, 6);
    // name -> (count, first offset, example kinds)
    let mut names: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut records: BTreeSet<String> = BTreeSet::new();
    let mut lua_bound: BTreeSet<String> = BTreeSet::new();
    let mut tables: BTreeSet<String> = BTreeSet::new();
    for s in &v {
        for n in qualified_names(&s.s) {
            let e = names.entry(n.clone()).or_insert((0, s.off));
            e.0 += 1;
        }
        if let Some(k) = s.s.find("DATABASE_TABLE<struct ") {
            let rest = &s.s[k + 22..];
            let end = rest.find(|c: char| c == ',' || c == '>').unwrap_or(rest.len());
            records.insert(rest[..end].to_string());
        }
        if let Some(k) = s.s.find("State::operator <<<") {
            let rest = &s.s[k + 19..];
            let rest = rest.trim_start_matches("const ").trim_start_matches("class ").trim_start_matches("struct ");
            let end = rest.find('>').unwrap_or(rest.len());
            lua_bound.insert(rest[..end].to_string());
        }
        if let Some(k) = s.s.find("In table ") {
            let rest = &s.s[k + 9..];
            let end = rest.find(':').unwrap_or(rest.len());
            tables.insert(rest[..end].to_string());
        }
    }
    let mut by_ns: BTreeMap<String, Vec<(&String, usize, usize)>> = BTreeMap::new();
    for (n, (c, o)) in &names {
        let parts: Vec<&str> = n.split("::").collect();
        let ns = parts[0].to_string();
        by_ns.entry(ns).or_default().push((n, *c, *o));
    }
    let mut w = String::new();
    let _ = writeln!(w, "# Qualified C++ names harvested from strings (__FUNCSIG__, template diagnostics). Source: re_tools classes (Rust).");
    let _ = writeln!(w, "# Total distinct qualified names: {}", names.len());
    let mut nsv: Vec<_> = by_ns.iter().collect();
    nsv.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    let _ = writeln!(w, "\n## Namespace counts (outermost qualifier)");
    for (ns, l) in &nsv { let _ = writeln!(w, "{:>6}  {}", l.len(), ns); }
    let _ = writeln!(w, "\n## Lua-bound types (UTILITYDLL::LUA::State::operator<< <T>) [{}]", lua_bound.len());
    for t in &lua_bound { let _ = writeln!(w, "  {}", t); }
    let _ = writeln!(w, "\n## DATABASE_TABLE record types [{}]", records.len());
    for t in &records { let _ = writeln!(w, "  {}", t); }
    let _ = writeln!(w, "\n## 'In table X' table identifiers [{}]", tables.len());
    for t in &tables { let _ = writeln!(w, "  {}", t); }
    let _ = writeln!(w, "\n## All names by namespace (name  count  first_file_offset)");
    for (ns, l) in &nsv {
        let _ = writeln!(w, "\n### {} ({})", ns, l.len());
        for (n, c, o) in l.iter() { let _ = writeln!(w, "  {}  x{}  @0x{:08x}", n, c, o); }
    }
    std::fs::write(format!("{}/class_names_from_strings.txt", outdir), &w).unwrap();
    for (ns, l) in nsv.iter().take(40) { println!("{:>6}  {}", l.len(), ns); }
    println!("lua_bound={} records={} tables={}", lua_bound.len(), records.len(), tables.len());
}

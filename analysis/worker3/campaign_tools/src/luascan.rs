//! Lua *source* scanner: tokenises Lua 5.1 source (comments/strings handled) and
//! extracts call chains (a.b:c(...)), argument counts, event registrations and
//! `context.*` usage. Heuristic, std only.
use crate::pack;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok { Name(String), Str(String), Num, Op(String) }

fn long_bracket(b: &[char], i: usize) -> Option<(usize, usize)> {
    if b.get(i) != Some(&'[') { return None; }
    let mut j = i + 1; let mut lvl = 0;
    while b.get(j) == Some(&'=') { lvl += 1; j += 1; }
    if b.get(j) == Some(&'[') { Some((lvl, j - i + 1)) } else { None }
}
fn find_close(b: &[char], mut j: usize, lvl: usize) -> usize {
    while j < b.len() {
        if b[j] == ']' { let mut k = j + 1; let mut l = 0; while b.get(k) == Some(&'=') { l += 1; k += 1; } if l == lvl && b.get(k) == Some(&']') { return k + 1; } }
        j += 1;
    }
    b.len()
}

pub fn tokenize(src: &str) -> Vec<Tok> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0; let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() { i += 1; continue; }
        if c == '-' && b.get(i + 1) == Some(&'-') {
            if let Some((lvl, ol)) = long_bracket(&b, i + 2) { i = find_close(&b, i + 2 + ol, lvl); }
            else { while i < b.len() && b[i] != '\n' { i += 1; } }
            continue;
        }
        if let Some((lvl, ol)) = long_bracket(&b, i) {
            let e = find_close(&b, i + ol, lvl);
            let end = e.saturating_sub(lvl + 2).max(i + ol);
            out.push(Tok::Str(b[i + ol..end].iter().collect())); i = e; continue;
        }
        if c == '"' || c == '\'' {
            let q = c; let mut j = i + 1; let mut s = String::new();
            while j < b.len() && b[j] != q && b[j] != '\n' { if b[j] == '\\' { j += 1; } if j < b.len() { s.push(b[j]); } j += 1; }
            out.push(Tok::Str(s)); i = j + 1; continue;
        }
        if c.is_alphabetic() || c == '_' { let mut j = i; while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') { j += 1; } out.push(Tok::Name(b[i..j].iter().collect())); i = j; continue; }
        if c.is_ascii_digit() { let mut j = i + 1; while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '.' || ((b[j] == '-' || b[j] == '+') && (b[j - 1] == 'e' || b[j - 1] == 'E'))) { j += 1; } out.push(Tok::Num); i = j; continue; }
        let three: String = b[i..(i + 3).min(b.len())].iter().collect();
        let two: String = b[i..(i + 2).min(b.len())].iter().collect();
        if three == "..." { out.push(Tok::Op(three)); i += 3; continue; }
        if ["==", "~=", "<=", ">=", ".."].contains(&two.as_str()) { out.push(Tok::Op(two)); i += 2; continue; }
        out.push(Tok::Op(c.to_string())); i += 1;
    }
    out
}

const KEYWORDS: &[&str] = &["and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while"];

#[derive(Default)]
pub struct Agg {
    pub calls: BTreeMap<String, (usize, BTreeMap<usize, usize>, BTreeSet<String>)>,
    pub events: BTreeMap<String, (usize, BTreeSet<String>)>,
    pub context_fields: BTreeMap<String, usize>,
    pub strings_first_arg: BTreeMap<String, BTreeMap<String, usize>>,
    pub defined_funcs: BTreeMap<String, usize>,
}

fn count_args(t: &[Tok], open: usize) -> usize {
    let mut depth = 0i32; let mut argc = 0; let mut any = false; let mut j = open;
    while j < t.len() {
        match &t[j] {
            Tok::Op(o) if o == "(" || o == "{" || o == "[" => { if depth >= 1 { any = true; } depth += 1; }
            Tok::Op(o) if o == ")" || o == "}" || o == "]" => { depth -= 1; if depth == 0 { return if any { argc + 1 } else { 0 }; } }
            Tok::Op(o) if o == "," && depth == 1 => { argc += 1; }
            _ => { if depth >= 1 { any = true; } }
        }
        j += 1;
    }
    argc
}

pub fn scan(src: &str, file: &str, agg: &mut Agg) {
    let t = tokenize(src);
    let mut i = 0;
    while i < t.len() {
        if t[i] == Tok::Name("function".into()) {
            let mut j = i + 1; let mut nm = String::new();
            while j < t.len() { match &t[j] { Tok::Name(n) => nm.push_str(n), Tok::Op(o) if o == "." || o == ":" => nm.push_str(o), _ => break } j += 1; }
            if !nm.is_empty() { *agg.defined_funcs.entry(nm).or_default() += 1; }
        }
        if let (Some(Tok::Name(a)), Some(Tok::Op(d)), Some(Tok::Name(ev)), Some(Tok::Op(br))) = (t.get(i), t.get(i + 1), t.get(i + 2), t.get(i + 3)) {
            if a == "events" && d == "." && br == "[" && (i == 0 || t[i - 1] != Tok::Op(".".into())) {
                let e = agg.events.entry(ev.clone()).or_default(); e.0 += 1; e.1.insert(file.to_string());
            }
        }
        // method call on an expression result: `...):name(` or `...]:name(`
        if let (Some(Tok::Op(p)), Some(Tok::Op(col)), Some(Tok::Name(m)), Some(nx)) = (t.get(i), t.get(i + 1), t.get(i + 2), t.get(i + 3)) {
            if (p == ")" || p == "]") && col == ":" && (matches!(nx, Tok::Op(o) if o == "(") || matches!(nx, Tok::Str(_))) {
                let chain = format!("<expr>:{}", m);
                let argc = if let Tok::Op(_) = nx { count_args(&t, i + 3) } else { 1 };
                let e = agg.calls.entry(chain).or_default(); e.0 += 1; *e.1.entry(argc).or_default() += 1; e.2.insert(file.to_string());
                i += 3; continue;
            }
        }
        if let Tok::Name(n0) = &t[i] {
            if KEYWORDS.contains(&n0.as_str()) || (i > 0 && (t[i - 1] == Tok::Op(".".into()) || t[i - 1] == Tok::Op(":".into()))) { i += 1; continue; }
            let mut chain = n0.clone(); let mut j = i + 1;
            loop {
                match (t.get(j), t.get(j + 1)) {
                    (Some(Tok::Op(o)), Some(Tok::Name(n))) if o == "." || o == ":" => { chain.push_str(o); chain.push_str(n); j += 2; }
                    _ => break,
                }
            }
            if n0 == "context" && chain.contains('.') { *agg.context_fields.entry(chain.clone()).or_default() += 1; }
            let is_call = matches!(t.get(j), Some(Tok::Op(o)) if o == "(") || matches!(t.get(j), Some(Tok::Str(_)));
            let is_def = i > 0 && t[i - 1] == Tok::Name("function".into());
            if is_call && !is_def {
                let argc = if let Some(Tok::Op(_)) = t.get(j) { count_args(&t, j) } else { 1 };
                let e = agg.calls.entry(chain.clone()).or_default();
                e.0 += 1; *e.1.entry(argc).or_default() += 1; e.2.insert(file.to_string());
                let first = match (t.get(j), t.get(j + 1)) { (Some(Tok::Op(_)), Some(Tok::Str(s))) => Some(s.clone()), (Some(Tok::Str(s)), _) => Some(s.clone()), _ => None };
                if let Some(s) = first {
                    *agg.strings_first_arg.entry(chain.clone()).or_default().entry(s.clone()).or_default() += 1;
                    if chain.ends_with("AddEventCallBack") { let e = agg.events.entry(s).or_default(); e.0 += 1; e.1.insert(file.to_string()); }
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
}

pub fn gather_sources() -> Vec<(String, String)> {
    let mut v = Vec::new();
    let d = std::path::Path::new(pack::DATA_DIR);
    for f in ["all_scripted.lua", "battle_scripted.lua"] { if let Ok(s) = std::fs::read(d.join(f)) { v.push((format!("loose:{}", f), String::from_utf8_lossy(&s).into_owned())); } }
    if let Ok(rd) = std::fs::read_dir(d.join("campaigns")) { for e in rd.flatten() { let p = e.path().join("scripting.lua"); if let Ok(s) = std::fs::read(&p) { v.push((format!("loose:campaigns/{}/scripting.lua", e.file_name().to_string_lossy()), String::from_utf8_lossy(&s).into_owned())); } } }
    for pk in ["data.pack", "sound.pack"] {
        if let Ok(p) = pack::Pack::open(pack::data_path(pk)) {
            for e in &p.entries { let l = e.path.to_ascii_lowercase(); if l.ends_with(".lua") || l.ends_with(".battle_script") || (pk == "sound.pack" && l.ends_with(".script")) {
                let b = p.read(e).unwrap(); if b.starts_with(b"\x1bLua") { continue; } v.push((format!("{}:{}", pk, e.path), String::from_utf8_lossy(&b).into_owned())); } }
        }
    }
    v
}

pub fn run(a: &[String]) {
    let mode = a.first().map(|s| s.as_str()).unwrap_or("calls");
    let filter = a.get(1).cloned().unwrap_or_default();
    let srcs = gather_sources();
    let mut agg = Agg::default();
    for (name, s) in &srcs { if name.contains(&filter) { scan(s, name, &mut agg); } }
    match mode {
        "files" => { for (n, s) in &srcs { println!("{}\t{} bytes\t{} lines", n, s.len(), s.lines().count()); } }
        "events" => { for (k, (c, f)) in &agg.events { println!("{}\t{}\t{}", k, c, f.iter().map(|x| x.rsplit(['\\', '/', ':']).next().unwrap_or(x).to_string()).collect::<Vec<_>>().join(",")); } }
        "context" => { for (k, c) in &agg.context_fields { println!("{}\t{}", k, c); } }
        "defs" => { for (k, c) in &agg.defined_funcs { println!("{}\t{}", k, c); } }
        "firstargs" => { let pat = a.get(2).map(|s| s.as_str()).unwrap_or(""); for (k, m) in &agg.strings_first_arg { if k.contains(pat) { let mut v: Vec<_> = m.iter().collect(); v.sort_by(|x, y| y.1.cmp(x.1)); println!("{}\t{}", k, v.iter().take(80).map(|(s, c)| format!("{}={}", s, c)).collect::<Vec<_>>().join(" ")); } } }
        _ => {
            for (k, (c, ac, f)) in &agg.calls {
                let acs: Vec<String> = ac.iter().map(|(a, n)| format!("{}:{}", a, n)).collect();
                println!("{}\t{}\targc[{}]\tfiles={}", k, c, acs.join(","), f.len());
            }
        }
    }
}

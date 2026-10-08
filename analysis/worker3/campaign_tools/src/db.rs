//! NTW DB table reader following Worker 2's format notes:
//!   [FC FD FE FF u32 version]  (only when version > 0)
//!   u8 0x01 marker, u32 row_count, rows back-to-back.
//! Field codes: s = UTF-16 string (u16 count + UTF-16LE), o = optional string (u8 flag + s),
//!              b = bool (u8), i = i32, f = f32, h = i16, a = ASCII string (u16 len + bytes).
//! `db-tail` resolves fixed-width tails after a known prefix by exhaustive search (reconstructed heuristic).
use crate::pack;
use std::collections::BTreeMap;

pub struct Table { pub name: String, pub version: u32, pub rows: u32, pub data: Vec<u8>, pub body: usize }

pub fn load(name: &str) -> Table {
    let p = pack::Pack::open(pack::data_path("data.pack")).unwrap();
    let path = format!("db\\{}_tables\\{}", name, name);
    let e = p.find(&path).unwrap_or_else(|| panic!("no table {}", path)).clone();
    let data = p.read(&e).unwrap();
    let (mut version, mut o) = (0u32, 0usize);
    if data.len() >= 8 && data[0..4] == [0xFC, 0xFD, 0xFE, 0xFF] { version = u32::from_le_bytes(data[4..8].try_into().unwrap()); o = 8; }
    assert_eq!(data[o], 1, "marker");
    let rows = u32::from_le_bytes(data[o + 1..o + 5].try_into().unwrap());
    Table { name: name.into(), version, rows, data, body: o + 5 }
}

fn rd_str(b: &[u8], p: &mut usize) -> Option<String> {
    if *p + 2 > b.len() { return None; }
    let n = u16::from_le_bytes([b[*p], b[*p + 1]]) as usize;
    if *p + 2 + 2 * n > b.len() { return None; }
    let u: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([b[*p + 2 + 2 * i], b[*p + 3 + 2 * i]])).collect();
    if u.iter().any(|&c| c < 0x20 && c != 9 && c != 10 && c != 13) { return None; }
    *p += 2 + 2 * n; Some(String::from_utf16_lossy(&u))
}

pub fn parse_field(b: &[u8], p: &mut usize, c: char) -> Option<String> {
    match c {
        's' => rd_str(b, p).map(|s| format!("{:?}", s)),
        'a' => { if *p + 2 > b.len() { return None; } let n = u16::from_le_bytes([b[*p], b[*p + 1]]) as usize; if *p + 2 + n > b.len() { return None; } let s: String = b[*p + 2..*p + 2 + n].iter().map(|&c| c as char).collect(); *p += 2 + n; Some(format!("a{:?}", s)) }
        'o' => { let f = *b.get(*p)?; if f > 1 { return None; } *p += 1; if f == 1 { rd_str(b, p).map(|s| format!("{:?}", s)) } else { Some("None".into()) } }
        'b' => { let v = *b.get(*p)?; if v > 1 { return None; } *p += 1; Some((v == 1).to_string()) }
        'i' => { if *p + 4 > b.len() { return None; } let v = i32::from_le_bytes(b[*p..*p + 4].try_into().unwrap()); *p += 4; Some(v.to_string()) }
        'f' => { if *p + 4 > b.len() { return None; } let v = f32::from_le_bytes(b[*p..*p + 4].try_into().unwrap()); *p += 4; Some(format!("{}", v)) }
        'h' => { if *p + 2 > b.len() { return None; } let v = i16::from_le_bytes([b[*p], b[*p + 1]]); *p += 2; Some(v.to_string()) }
        _ => None,
    }
}

/// Parse every row with schema; returns rows or error message.
pub fn parse_all(t: &Table, schema: &str) -> Result<Vec<Vec<String>>, String> {
    let mut p = t.body; let mut out = Vec::new();
    for r in 0..t.rows {
        let mut row = Vec::new();
        for c in schema.chars() { match parse_field(&t.data, &mut p, c) { Some(v) => row.push(v), None => return Err(format!("row {} field '{}' at 0x{:x} failed", r, c, p)) } }
        out.push(row);
    }
    if p != t.data.len() { return Err(format!("ended at 0x{:x}, file len 0x{:x}", p, t.data.len())); }
    Ok(out)
}

fn nice_i(v: i32) -> bool { v.abs() <= 1_000_000 && !(v & 0xFF == 0 && v != 0 && v % 100 != 0) }
fn nice_f(v: f32) -> bool { v.is_finite() && (v == 0.0 || (v.abs() >= 1e-4 && v.abs() <= 1e7)) }

/// Find a constant tail length L such that prefix + L bytes per row reaches EOF exactly.
fn tail_len(t: &Table, prefix: &str) -> Option<usize> {
    'l: for l in 0..512 {
        let mut p = t.body;
        for _ in 0..t.rows { for c in prefix.chars() { if parse_field(&t.data, &mut p, c).is_none() { continue 'l; } } p += l; if p > t.data.len() { continue 'l; } }
        if p == t.data.len() { return Some(l); }
    }
    None
}

pub fn run(a: &[String]) {
    let cmd = a[0].as_str();
    let t = load(&a[1]);
    match cmd {
        "db" => {
            let schema = &a[2]; let n: usize = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(5);
            match parse_all(&t, schema) {
                Ok(rows) => { println!("# {} v{} rows={} schema={} OK(exact EOF)", t.name, t.version, t.rows, schema); for r in rows.iter().take(n) { println!("{}", r.join("\t")); } }
                Err(e) => println!("# {} v{} rows={} schema={} FAIL {}", t.name, t.version, t.rows, schema, e),
            }
        }
        "db-col" => { // histogram of column k
            let schema = &a[2]; let k: usize = a[3].parse().unwrap();
            let rows = parse_all(&t, schema).expect("parse");
            let mut h: BTreeMap<String, usize> = BTreeMap::new(); for r in &rows { *h.entry(r[k].clone()).or_default() += 1; }
            let mut v: Vec<_> = h.into_iter().collect(); v.sort_by(|x, y| y.1.cmp(&x.1));
            println!("{} distinct: {}", v.len(), v.iter().take(40).map(|(k, c)| format!("{}x{}", k, c)).collect::<Vec<_>>().join(" "));
        }
        "db-hex" => {
            let n: usize = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(256);
            println!("# {} v{} rows={} len={} body=0x{:x}", t.name, t.version, t.rows, t.data.len(), t.body);
            crate::hexdump(&t.data[..n.min(t.data.len())], 0);
        }
        "db-tail" => {
            let prefix = &a[2]; let maxb: usize = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(9);
            let l = match tail_len(&t, prefix) { Some(l) => l, None => { println!("no constant tail for prefix {}", prefix); return; } };
            println!("# {} rows={} prefix={} tail_len={}", t.name, t.rows, prefix, l);
            // collect tail slices
            let mut tails = Vec::new(); let mut p = t.body;
            for _ in 0..t.rows { for c in prefix.chars() { parse_field(&t.data, &mut p, c); } tails.push(&t.data[p..p + l]); p += l; }
            let mut results: Vec<(i64, String)> = Vec::new();
            for nb in 0..=maxb { if nb > l || (l - nb) % 4 != 0 { continue; } let ni = (l - nb) / 4; let n = ni + nb;
                // choose positions of bools among n slots
                let mut idx: Vec<usize> = (0..nb).collect();
                loop {
                    let mut sch = vec!['i'; n]; for &k in &idx { sch[k] = 'b'; }
                    // evaluate
                    let mut score: i64 = 0; let mut ok = true; let mut col_types = String::new();
                    let mut off = 0usize;
                    for &c in &sch {
                        if c == 'b' { if tails.iter().any(|tl| tl[off] > 1) { ok = false; break; } col_types.push('b'); score -= 1; off += 1; }
                        else { let ni_ = tails.iter().all(|tl| nice_i(i32::from_le_bytes(tl[off..off + 4].try_into().unwrap())));
                            let nf_ = tails.iter().all(|tl| nice_f(f32::from_le_bytes(tl[off..off + 4].try_into().unwrap())));
                            if ni_ { col_types.push('i'); score += 4; } else if nf_ { col_types.push('f'); score += 4; } else { col_types.push('?'); score -= 20; }
                            off += 4; }
                    }
                    if ok { results.push((score, col_types)); }
                    // next combination
                    if nb == 0 { break; }
                    let mut k = nb; let mut done = true;
                    while k > 0 { k -= 1; if idx[k] < n - nb + k { idx[k] += 1; for j in k + 1..nb { idx[j] = idx[j - 1] + 1; } done = false; break; } }
                    if done { break; }
                }
            }
            results.sort_by(|x, y| y.0.cmp(&x.0));
            let best = results.first().map(|x| x.0).unwrap_or(0);
            println!("{} valid layouts; best score {}; ties at best: {}", results.len(), best, results.iter().filter(|x| x.0 == best).count());
            for (s, c) in results.iter().take(8) { println!("  score {:>4}  {}{}", s, prefix, c); }
        }
        "db-check" => println!("{}", check(&a[1], &a[2])),
        _ => {}
    }
}

/// Sanity-check a schema: parse, then report per-numeric-column how many values look implausible.
pub fn check(name: &str, schema: &str) -> String {
    let t = load(name);
    match parse_all(&t, schema) {
        Err(e) => format!("{} v{} rows={} {} FAIL {}", name, t.version, t.rows, schema, e),
        Ok(rows) => {
            let mut flags = Vec::new();
            for (k, c) in schema.chars().enumerate() {
                let bad = rows.iter().filter(|r| match c { 'i' => r[k].parse::<i64>().map(|v| !nice_i(v as i32)).unwrap_or(true), 'f' => r[k].parse::<f32>().map(|v| !nice_f(v)).unwrap_or(true), _ => false }).count();
                if bad > 0 { flags.push(format!("col{}({}):{}bad", k, c, bad)); }
            }
            format!("{} v{} rows={} {} OK {}", name, t.version, t.rows, schema, flags.join(" "))
        }
    }
}

/// .loc reader: FF FE, "LOC\0" (ASCII bytes 4C 4F 43 00), u32 version, u32 count,
/// count x { utf16 key (u16 count), utf16 text (u16 count), u8 bool }.
pub fn load_loc(pk: &str, path: &str) -> (u32, Vec<(String, String, bool)>) {
    let p = pack::Pack::open(pack::data_path(pk)).unwrap();
    let e = p.find(path).unwrap().clone();
    let b = p.read(&e).unwrap();
    assert_eq!(&b[0..6], &[0xFF, 0xFE, b'L', b'O', b'C', 0]);
    let ver = u32::from_le_bytes(b[6..10].try_into().unwrap());
    let n = u32::from_le_bytes(b[10..14].try_into().unwrap());
    let mut p2 = 14; let mut v = Vec::new();
    for _ in 0..n {
        let rd = |b: &[u8], p: &mut usize| { let n = u16::from_le_bytes([b[*p], b[*p + 1]]) as usize; let u: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([b[*p + 2 + 2 * i], b[*p + 3 + 2 * i]])).collect(); *p += 2 + 2 * n; String::from_utf16_lossy(&u) };
        let k = rd(&b, &mut p2); let t = rd(&b, &mut p2); let f = b[p2] != 0; p2 += 1; v.push((k, t, f));
    }
    assert_eq!(p2, b.len(), "loc must end exactly");
    (ver, v)
}

pub fn run_loc(a: &[String]) {
    let files = [("local_en.pack", r"text\localisation.loc"), ("local_en.pack", r"text\ui.loc"), ("local_en_patch.pack", r"text\localisation.loc"), ("local_en_patch.pack", r"text\ui.loc")];
    match a[0].as_str() {
        "loc-info" => for (pk, f) in files { let (v, e) = load_loc(pk, f); let t = e.iter().filter(|x| x.2).count();
            let mut pre: BTreeMap<String, usize> = BTreeMap::new();
            for (k, _, _) in &e { let parts: Vec<&str> = k.splitn(3, '_').collect(); let p = if parts.len() >= 2 { format!("{}_{}", parts[0], parts[1]) } else { k.clone() }; *pre.entry(p).or_default() += 1; }
            let mut pv: Vec<_> = pre.into_iter().collect(); pv.sort_by(|x, y| y.1.cmp(&x.1));
            println!("{} {} version={} entries={} bool_true={} top_prefixes: {}", pk, f, v, e.len(), t, pv.iter().take(25).map(|(k, c)| format!("{}={}", k, c)).collect::<Vec<_>>().join(" ")); },
        "loc-get" => for (pk, f) in files { let (_, e) = load_loc(pk, f); for k in &a[1..] { for x in e.iter().filter(|x| &x.0 == k) { println!("{}:{}\t{}\t{:?}\t{}", pk, f, x.0, x.1, x.2); } } },
        "loc-find" => { let mx: usize = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(20); for (pk, f) in files { let (_, e) = load_loc(pk, f); for x in e.iter().filter(|x| x.0.contains(&a[1])).take(mx) { println!("{}:{}\t{}\t{:?}\t{}", pk, f, x.0, x.1.chars().take(80).collect::<String>(), x.2); } } }
        _ => {}
    }
}

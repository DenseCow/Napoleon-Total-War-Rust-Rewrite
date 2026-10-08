//! UI layout probe. Binary layouts are extensionless files under ui\ in data.pack/boot.pack
//! starting with ASCII "VersionNNN". Only the header and length-prefixed strings are decoded.
use crate::pack;
use std::collections::BTreeMap;

/// Find u16-length-prefixed ASCII strings (len >= 3) in a buffer. Returns (offset, string).
pub fn lp_strings(b: &[u8]) -> Vec<(usize, String)> {
    let mut out = Vec::new(); let mut i = 0;
    while i + 2 < b.len() {
        let n = u16::from_le_bytes([b[i], b[i + 1]]) as usize;
        if n >= 3 && n < 300 && i + 2 + n <= b.len() && b[i + 2..i + 2 + n].iter().all(|&c| (32..127).contains(&c)) {
            out.push((i, b[i + 2..i + 2 + n].iter().map(|&c| c as char).collect())); i += 2 + n; continue;
        }
        i += 1;
    }
    out
}

pub fn run(a: &[String]) {
    let mode = a.first().map(|s| s.as_str()).unwrap_or("summary");
    let mut versions: BTreeMap<String, usize> = BTreeMap::new();
    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pk in ["boot.pack", "data.pack"] {
        let p = pack::Pack::open(pack::data_path(pk)).unwrap();
        for e in &p.entries {
            let l = e.path.to_ascii_lowercase();
            if !l.starts_with("ui\\") { continue; }
            let fname = l.rsplit('\\').next().unwrap();
            if fname.contains('.') { continue; }
            let h = p.read_head(e, 64).unwrap();
            if h.len() >= 16 && &h[0..7] == b"Version" {
                let v = String::from_utf8_lossy(&h[0..10]).into_owned();
                *versions.entry(v.clone()).or_default() += 1;
                let rootlen = u16::from_le_bytes([h[14], h[15]]) as usize;
                let root: String = h[16..(16 + rootlen).min(h.len())].iter().map(|&c| c as char).collect();
                let dir = e.path.rsplitn(2, '\\').nth(1).unwrap_or("").to_string();
                if mode == "strings" && a.get(1).map(|f| l.contains(&f.to_ascii_lowercase())).unwrap_or(false) {
                    let b = p.read(e).unwrap();
                    println!("== {} {} bytes header {} this=0x{:08x} root_id={:?}", e.path, e.size, v, u32::from_le_bytes([h[10], h[11], h[12], h[13]]), root);
                    for (o, s) in lp_strings(&b) { println!("  0x{:06x} {}", o, s); }
                }
                if mode == "detail" {
                    let b = p.read(e).unwrap();
                    let strs = lp_strings(&b);
                    let imgs = strs.iter().filter(|(_, s)| { let s = s.to_ascii_lowercase(); s.ends_with(".tga") || s.ends_with(".dds") || s.ends_with(".png") }).count();
                    println!("{}\t{}\t{}\troot={}\tstrings={}\timages={}", e.path, e.size, v, root, strs.len(), imgs);
                }
                dirs.entry(dir).or_default().push(format!("{}({})", fname, root));
            } else if mode == "summary" {
                println!("non-layout extensionless: {} {} {:?}", e.path, e.size, String::from_utf8_lossy(&h[..h.len().min(16)]));
            }
        }
    }
    if mode == "summary" {
        println!("layout header versions: {:?}", versions);
        for (d, v) in &dirs { println!("{} ({}): {}", d, v.len(), v.join(" ")); }
    }
}

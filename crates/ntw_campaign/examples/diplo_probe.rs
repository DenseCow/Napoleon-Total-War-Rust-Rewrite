//! Survey of `DIPLOMACY_RELATIONSHIP` values over startpos/save files (BACKLOG §1 diplomacy fields;
//! notes in `analysis/campaign/S1_LEFTOVERS.md`). Reads the files only.
//!
//! ```text
//! cargo run -p ntw_campaign --example diplo_probe -- stats <file.esf>...
//! cargo run -p ntw_campaign --example diplo_probe -- rows <file.esf> [faction id]
//! ```
//! * `stats`: per field (and per attitude slot / per entry of the u32[14]) the value histogram.
//! * `rows`: one line per relationship of one file (all, or those owned by one faction id).

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

type Hist = BTreeMap<String, BTreeMap<String, usize>>;

fn val(n: &EsfNode) -> String {
    match n {
        EsfNode::Utf16String(s) | EsfNode::AsciiString(s) => s.clone(),
        EsfNode::Bool(b) => (*b as u8).to_string(),
        EsfNode::RecordArray(a) => format!("[{} items]", a.items.len()),
        EsfNode::U32Array(a) => format!("{a:?}"),
        n => n.as_int().map_or_else(|| n.type_name().to_string(), |v| v.to_string()),
    }
}

fn add(h: &mut Hist, key: String, v: String) {
    *h.entry(key).or_default().entry(v).or_default() += 1;
}

fn relationships(esf: &EsfFile) -> Vec<(i64, &EsfRecord)> {
    // (owner faction id, record): the owner is the FACTION record's id (#0) holding the manager.
    let mut out = Vec::new();
    esf.root.walk(&mut |r: &EsfRecord| {
        if r.name == "FACTION" {
            let owner = r.get(0).and_then(EsfNode::as_int).unwrap_or(-1);
            r.walk(&mut |x: &EsfRecord| {
                if x.name == "DIPLOMACY_RELATIONSHIP" {
                    out.push((owner, x));
                }
            });
        }
    });
    out
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("usage: diplo_probe stats FILE... | rows FILE [owner]");
        std::process::exit(2);
    };
    let open = |p: &str| EsfFile::from_bytes(&std::fs::read(p).expect("read")).expect("esf");
    match cmd.as_str() {
        "stats" => {
            let mut h = Hist::new();
            let mut n = 0;
            for f in &args[1..] {
                let esf = open(f);
                for (_, r) in relationships(&esf) {
                    n += 1;
                    add(&mut h, "version".into(), r.version.to_string());
                    for (i, c) in r.children.iter().enumerate() {
                        match c {
                            EsfNode::RecordArray(a) if i == 1 => {
                                for (k, it) in a.items.iter().enumerate() {
                                    let s: Vec<String> = it.iter().map(val).collect();
                                    for (j, v) in s.iter().enumerate() {
                                        add(&mut h, format!("#01 att{k:02}.{j}"), v.clone());
                                    }
                                }
                            }
                            EsfNode::RecordArray(a) => {
                                add(&mut h, format!("#{i:02} count"), a.items.len().to_string());
                                for it in &a.items {
                                    let s: Vec<String> = it.iter().map(val).collect();
                                    add(&mut h, format!("#{i:02} item"), s.join(","));
                                }
                            }
                            EsfNode::U32Array(a) => {
                                for (k, v) in a.iter().enumerate() {
                                    add(&mut h, format!("#{i:02}[{k:02}]"), v.to_string());
                                }
                            }
                            c => add(&mut h, format!("#{i:02} {}", c.type_name()), val(c)),
                        }
                    }
                }
            }
            println!("{n} relationships");
            for (k, m) in &h {
                let mut v: Vec<_> = m.iter().collect();
                v.sort_by(|a, b| b.1.cmp(a.1));
                let shown: Vec<String> = v.iter().take(14).map(|(a, b)| format!("{a}x{b}")).collect();
                println!("{k:<22} ({} distinct) {}", m.len(), shown.join(" "));
            }
        }
        "rows" => {
            let esf = open(&args[1]);
            let want: Option<i64> = args.get(2).and_then(|s| s.parse().ok());
            for (owner, r) in relationships(&esf) {
                if want.is_some_and(|w| w != owner) {
                    continue;
                }
                let mut s: Vec<String> = r.children.iter().enumerate().filter(|(i, _)| *i != 1).map(|(i, c)| format!("{i}={}", val(c))).collect();
                if let Some(a) = r.get(1).and_then(EsfNode::as_record_array) {
                    for (k, it) in a.items.iter().enumerate() {
                        let v: Vec<String> = it.iter().map(val).collect();
                        if v.iter().any(|x| x != "0") {
                            s.push(format!("a{k}={}", v.join("/")));
                        }
                    }
                }
                println!("{owner} {}", s.join(" "));
            }
        }
        _ => eprintln!("unknown command {cmd}"),
    }
}

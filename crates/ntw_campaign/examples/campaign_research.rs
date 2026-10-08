//! Read-only research helper for the campaign gameplay work (no game data is written).
//!
//! ```text
//! cargo run -p ntw_campaign --release --example campaign_research -- ap <file.esf>...
//!     histogram of CHARACTER type -> LOCOMOTABLE (#8, #9) pairs
//! cargo run -p ntw_campaign --release --example campaign_research -- recruit <file.esf>...
//!     every RECRUITMENT_ITEM (unit key, #3, #4, ...) found in the file
//! cargo run -p ntw_campaign --release --example campaign_research -- construct <file.esf>...
//!     every BUILDING_CONSTRUCTION_ITEM found in the file
//! ```

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

fn walk<'a>(r: &'a EsfRecord, f: &mut impl FnMut(&'a EsfRecord)) {
    r.walk(f);
}

fn show(n: &EsfNode) -> String {
    if let Some(s) = n.as_str() {
        return format!("{s:?}");
    }
    if let Some(v) = n.as_int() {
        return v.to_string();
    }
    if let Some(v) = n.as_bool() {
        return v.to_string();
    }
    if let Some(v) = n.as_f32() {
        return v.to_string();
    }
    if let Some(r) = n.as_record() {
        return format!("{{{}}}", r.name);
    }
    n.type_name().to_string()
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().cloned().unwrap_or_default();
    // `find <RECORD> <max> <file>...`: every occurrence's direct children, one line each.
    // `hist <RECORD> <child index> <file>...`: value histogram of one child.
    let (pattern, num) = if cmd == "find" || cmd == "hist" {
        let p = args.remove(1);
        let n: usize = args.remove(1).parse().unwrap_or(10);
        (p, n)
    } else {
        (String::new(), 0)
    };
    for file in &args[1..] {
        let esf = EsfFile::open(file).expect("open");
        println!("== {file}");
        match cmd.as_str() {
            "find" => {
                let mut k = 0;
                walk(&esf.root, &mut |r| {
                    if r.name == pattern && k < num {
                        k += 1;
                        let vals: Vec<String> = r.children.iter().map(|c| match c {
                            EsfNode::RecordArray(a) => format!("[{} x{}]", a.name, a.items.len()),
                            EsfNode::U8Array(v) => format!("u8{v:?}"),
                            EsfNode::U32Array(v) => format!("u32{:?}", &v[..v.len().min(12)]),
                            EsfNode::I32Array(v) => format!("i32{:?}", &v[..v.len().min(12)]),
                            other => show(other),
                        }).collect();
                        println!("v{} {}", r.version, vals.join(" | "));
                    }
                });
            }
            "hist" => {
                let mut h: BTreeMap<String, usize> = BTreeMap::new();
                walk(&esf.root, &mut |r| {
                    if r.name == pattern {
                        *h.entry(r.get(num).map(show).unwrap_or_else(|| "-".into())).or_default() += 1;
                    }
                });
                println!("{h:?}");
            }
            "ap" => {
                let mut hist: BTreeMap<(String, i32, i32), usize> = BTreeMap::new();
                walk(&esf.root, &mut |r| {
                    if r.name == "CHARACTER" {
                        let ty = r.get_str(3).unwrap_or("?").to_string();
                        if let Some(loc) = r.get(0).and_then(EsfNode::as_record) {
                            let a = loc.get_i32(8).unwrap_or(-1);
                            let b = loc.get_i32(9).unwrap_or(-1);
                            *hist.entry((ty, a, b)).or_default() += 1;
                        }
                    }
                });
                for ((t, a, b), n) in hist {
                    println!("{t:<22} #8={a:<4} #9={b:<4} x{n}");
                }
            }
            "recruit" | "construct" => {
                let want = if cmd == "recruit" { "RECRUITMENT_ITEM" } else { "BUILDING_CONSTRUCTION_ITEM" };
                walk(&esf.root, &mut |r| {
                    if r.name == want && r.children.iter().all(|c| c.as_record().is_none()) {
                        let vals: Vec<String> = r.children.iter().map(show).collect();
                        println!("{}", vals.join(" | "));
                    }
                });
            }
            "region" => {
                walk(&esf.root, &mut |r| {
                    if r.name == "REGION" && r.get_str(0).is_some() {
                        let vals: Vec<String> = (6..=19).filter_map(|i| r.get(i)).map(show).collect();
                        let gdp = r.get(32).map(|n| format!("{n:?}")).unwrap_or_default();
                        let pop = r.get(1).and_then(EsfNode::as_record).and_then(|p| p.get_u32(1)).unwrap_or(0);
                        println!(
                            "{:<26} pop {pop:<9} {} {}",
                            r.get_str(0).unwrap_or(""),
                            vals.join(" "),
                            gdp.chars().take(90).collect::<String>()
                        );
                    }
                });
            }
            _ => eprintln!("unknown command {cmd}"),
        }
    }
}

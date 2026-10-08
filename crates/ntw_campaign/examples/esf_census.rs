//! Research helper: counts every record name (with its versions) and every record array's total
//! item count in each save, side by side, printing only the rows that differ.
//!   cargo run -p ntw_campaign --release --example esf_census -- A.save B.save [C.save ...]
use std::collections::{BTreeMap, BTreeSet};
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

fn census(r: &EsfRecord, out: &mut BTreeMap<String, usize>) {
    *out.entry(format!("R {} v{}", r.name, r.version)).or_default() += 1;
    for c in &r.children {
        match c {
            EsfNode::Record(x) => census(x, out),
            EsfNode::RecordArray(a) => {
                *out.entry(format!("A {}[] items", a.name)).or_default() += a.items.len();
                for it in &a.items {
                    for n in it {
                        if let EsfNode::Record(x) = n {
                            census(x, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let all: Vec<BTreeMap<String, usize>> = files
        .iter()
        .map(|p| {
            let mut m = BTreeMap::new();
            census(&EsfFile::open(p).unwrap().root, &mut m);
            m
        })
        .collect();
    let keys: BTreeSet<&String> = all.iter().flat_map(|m| m.keys()).collect();
    for (i, f) in files.iter().enumerate() {
        println!("# {i}: {f}");
    }
    for k in keys {
        let v: Vec<usize> = all.iter().map(|m| m.get(k).copied().unwrap_or(0)).collect();
        if v.windows(2).any(|w| w[0] != w[1]) {
            println!("{:60} {}", k, v.iter().map(|n| format!("{n:8}")).collect::<String>());
        }
    }
}

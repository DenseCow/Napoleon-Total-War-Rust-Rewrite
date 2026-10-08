//! A small survey tool: prints the *structure* of an ESF file (startpos or save).
//!
//! It was used to write `analysis/worker3/STARTPOS_LAYOUT.md`. It reads the file only.
//!
//! ```text
//! cargo run -p ntw_campaign --example esf_dump -- schema <file.esf> [max_depth]
//! cargo run -p ntw_campaign --example esf_dump -- tree   <file.esf> <PATH> [depth] [max_items]
//! ```
//! * `schema` lists every distinct record path once, with how often it occurs, its versions and
//!   the child signature (the list of child types) of its first occurrence.
//! * `tree` prints one subtree with its values, e.g. path
//!   `CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY[0]/FACTION`.

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfFile, EsfNode, EsfPathTarget};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: esf_dump schema FILE [max_depth] | tree FILE PATH [depth] [max_items]";
    let (Some(cmd), Some(file)) = (args.first(), args.get(1)) else {
        eprintln!("{usage}");
        std::process::exit(2);
    };
    let esf = match EsfFile::open(file) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            std::process::exit(1);
        }
    };
    match cmd.as_str() {
        "schema" => {
            let max_depth = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(64);
            let mut paths = BTreeMap::new();
            let mut order = Vec::new();
            collect(
                &esf.root.children,
                &esf.root.name,
                esf.root.version,
                0,
                max_depth,
                &mut paths,
                &mut order,
            );
            for p in order {
                let e = &paths[&p];
                println!("{p}  x{}  v{:?}  [{}]", e.count, e.versions, e.signature);
            }
        }
        "tree" => {
            let path = args.get(2).map(String::as_str).unwrap_or("");
            let depth = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);
            let max_items = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(3);
            match esf.root.lookup(path) {
                Some(EsfPathTarget::Record(r)) => {
                    println!("{} v{}", r.name, r.version);
                    dump(&r.children, 1, depth, max_items);
                }
                Some(EsfPathTarget::RecordArray(a)) => {
                    println!("{}[] v{} ({} items)", a.name, a.version, a.items.len());
                    for (i, item) in a.items.iter().take(max_items).enumerate() {
                        println!("  [{i}]");
                        dump(item, 2, depth, max_items);
                    }
                }
                Some(EsfPathTarget::Item(item)) => dump(item, 1, depth, max_items),
                None => {
                    eprintln!("path not found: {path}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

struct PathInfo {
    count: usize,
    versions: Vec<u8>,
    signature: String,
}

fn signature(children: &[EsfNode]) -> String {
    children
        .iter()
        .map(|n| match n {
            EsfNode::Record(r) => format!("{{{}}}", r.name),
            EsfNode::RecordArray(a) => format!("[{}]", a.name),
            other => other.type_name().to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn collect(
    children: &[EsfNode],
    path: &str,
    version: u8,
    depth: usize,
    max_depth: usize,
    paths: &mut BTreeMap<String, PathInfo>,
    order: &mut Vec<String>,
) {
    let e = paths.entry(path.to_string()).or_insert_with(|| {
        order.push(path.to_string());
        PathInfo {
            count: 0,
            versions: Vec::new(),
            signature: signature(children),
        }
    });
    e.count += 1;
    if !e.versions.contains(&version) {
        e.versions.push(version);
    }
    if depth >= max_depth {
        return;
    }
    for n in children {
        match n {
            EsfNode::Record(r) => collect(
                &r.children,
                &format!("{path}/{}", r.name),
                r.version,
                depth + 1,
                max_depth,
                paths,
                order,
            ),
            EsfNode::RecordArray(a) => {
                for item in &a.items {
                    collect(
                        item,
                        &format!("{path}/{}[]", a.name),
                        a.version,
                        depth + 1,
                        max_depth,
                        paths,
                        order,
                    );
                }
            }
            _ => {}
        }
    }
}

fn short(n: &EsfNode) -> String {
    let s = match n {
        EsfNode::Utf16String(s) => format!("utf16 {s:?}"),
        EsfNode::AsciiString(s) => format!("ascii {s:?}"),
        EsfNode::U32Array(v) => format!("u32[{}] {:?}", v.len(), &v[..v.len().min(8)]),
        EsfNode::I32Array(v) => format!("i32[{}] {:?}", v.len(), &v[..v.len().min(8)]),
        EsfNode::U8Array(v) => format!("u8[{}]", v.len()),
        EsfNode::U16Array(v) => format!("u16[{}]", v.len()),
        EsfNode::F32Array(v) => format!("f32[{}] {:?}", v.len(), &v[..v.len().min(8)]),
        EsfNode::BoolArray(v) => format!("bool[{}]", v.len()),
        EsfNode::Coord2dArray(v) => format!("coord2d[{}]", v.len()),
        EsfNode::Coord3dArray(v) => format!("coord3d[{}]", v.len()),
        other => format!("{} {:?}", other.type_name(), other).replace(['(', ')'], " "),
    };
    if s.len() > 120 {
        format!(
            "{}...",
            &s[..s.char_indices().nth(117).map_or(s.len(), |c| c.0)]
        )
    } else {
        s
    }
}

fn dump(children: &[EsfNode], indent: usize, depth: usize, max_items: usize) {
    let pad = "  ".repeat(indent);
    for (i, n) in children.iter().enumerate() {
        match n {
            EsfNode::Record(r) => {
                println!("{pad}#{i} {{{}}} v{}", r.name, r.version);
                if depth > 0 {
                    dump(&r.children, indent + 1, depth - 1, max_items);
                }
            }
            EsfNode::RecordArray(a) => {
                println!(
                    "{pad}#{i} [{}] v{} ({} items)",
                    a.name,
                    a.version,
                    a.items.len()
                );
                if depth > 0 {
                    for (j, item) in a.items.iter().take(max_items).enumerate() {
                        println!("{pad}  [{j}]");
                        dump(item, indent + 2, depth - 1, max_items);
                    }
                }
            }
            other => println!("{pad}#{i} {}", short(other)),
        }
    }
}

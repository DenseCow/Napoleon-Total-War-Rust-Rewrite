//! Research helper: walks a campaign `.esf` (a start position or a save, read-only) and prints the
//! **path** of every record whose name contains a substring, with the record's version and its
//! children when it has few enough. Unlike `campaign_probe tree`, which prints the first 40
//! children of every record, this finds a record wherever it is nested.
//!
//!   cargo run -p ntw_formats --example esf_find -- "<vfs path or file path>" <name substring> [--children <n>]
//!
//! The trailing name pool (every interned record name, `u16` length + ASCII) is skipped: it makes
//! every record name "appear" in every file.
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let needle = args[1].to_ascii_uppercase();
    let max_children: usize = args
        .iter()
        .position(|a| a == "--children")
        .map(|i| args[i + 1].parse().unwrap_or(12))
        .unwrap_or(12);
    let vfs = Vfs::open_install(&dir).unwrap();
    let bytes = if std::path::Path::new(&args[0]).is_file() {
        std::fs::read(&args[0]).unwrap()
    } else {
        vfs.read(&args[0]).unwrap()
    };
    let esf = EsfFile::from_bytes(&bytes).unwrap();
    let mut found = 0usize;
    walk(&esf.root, "", &needle, max_children, &mut found);
    println!("{found} matches");
}

fn walk(rec: &ntw_formats::esf::EsfRecord, path: &str, needle: &str, max_children: usize, found: &mut usize) {
    let here = if path.is_empty() { rec.name.clone() } else { format!("{}/{}", path, rec.name) };
    let hit = rec.name.to_ascii_uppercase().contains(needle)
        || rec.children.iter().any(|c| match c {
            EsfNode::Record(r) => r.name.to_ascii_uppercase().contains(needle),
            EsfNode::RecordArray(a) => a.name.to_ascii_uppercase().contains(needle),
            _ => false,
        });
    if hit {
        *found += 1;
        println!("{here} v{}", rec.version);
        for (i, c) in rec.children.iter().enumerate().take(max_children) {
            let line = format!("{c:?}");
            let line = if line.len() > 120 { format!("{}...", &line[..120]) } else { line };
            println!("  #{i} {line}");
        }
        if rec.children.len() > max_children {
            println!("  ... {} more children", rec.children.len() - max_children);
        }
    }
    for c in &rec.children {
        match c {
            EsfNode::Record(r) => walk(r, &here, needle, max_children, found),
            EsfNode::RecordArray(a) => {
                let ap = format!("{here}/{}", a.name);
                if a.name.to_ascii_uppercase().contains(needle) {
                    *found += 1;
                    println!("{ap} v{} ({} items)", a.version, a.items.len());
                    for (i, item) in a.items.iter().enumerate().take(max_children) {
                        for (j, ic) in item.iter().enumerate() {
                            let line = format!("{ic:?}");
                            let line = if line.len() > 120 { format!("{}...", &line[..120]) } else { line };
                            println!("  [item {i}] #{j} {line}");
                        }
                    }
                }
                for item in &a.items {
                    for ic in item {
                        if let EsfNode::Record(r) = ic {
                            walk(r, &ap, needle, max_children, found);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
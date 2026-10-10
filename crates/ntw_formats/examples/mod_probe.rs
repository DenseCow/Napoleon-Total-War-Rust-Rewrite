//! Research helper for mod loading (read-only).
//!   cargo run -p ntw_formats --example mod_probe -- packs      header of every pack in data\
//!   cargo run -p ntw_formats --example mod_probe -- loose      loose files in data\ vs the pack versions
//!   cargo run -p ntw_formats --example mod_probe -- dirs       db\ / text\ folders that hold more than one file
use ntw_formats::pack::{PackFile, Vfs, normalize_path};
use std::collections::BTreeMap;
use std::path::Path;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            out.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    match args.first().map(String::as_str).unwrap_or("packs") {
        "packs" => {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pack")) {
                    let pk = PackFile::open(&p).unwrap();
                    let h = pk.header();
                    println!(
                        "{:<28} type {} ({:?}) deps {:?} files {}",
                        p.file_name().unwrap().to_string_lossy(),
                        h.raw_type,
                        h.pack_type,
                        h.dependencies,
                        h.file_count
                    );
                }
            }
        }
        "loose" => {
            let vfs = Vfs::open_install(&dir).unwrap();
            let mut files = Vec::new();
            walk(Path::new(&dir), Path::new(&dir), &mut files);
            let (mut same, mut diff, mut only) = (0, 0, 0);
            for f in files {
                if f.to_ascii_lowercase().ends_with(".pack") || f.eq_ignore_ascii_case("language.txt") {
                    continue;
                }
                match vfs.find(&f) {
                    Some((pk, en)) => {
                        let disk = std::fs::read(Path::new(&dir).join(&f)).unwrap();
                        let packed = pk.read_entry(en).unwrap();
                        let tag = if disk == packed { same += 1; "same" } else { diff += 1; "DIFF" };
                        if tag == "DIFF" || args.get(1).is_some() {
                            println!("{tag} {f} disk {} pack {} [{}]", disk.len(), packed.len(),
                                pk.path().file_name().unwrap().to_string_lossy());
                        }
                    }
                    None => {
                        only += 1;
                        if args.get(1).is_some() {
                            println!("loose-only {f}");
                        }
                    }
                }
            }
            println!("same {same} diff {diff} loose-only {only}");
        }
        "dirs" => {
            let vfs = Vfs::open_install(&dir).unwrap();
            // folder -> [(file, pack)] over ALL packs, not just the winners.
            let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for pk in vfs.packs() {
                for e in pk.entries() {
                    let n = normalize_path(&e.path);
                    if !(n.starts_with("db\\") || n.starts_with("text\\")) {
                        continue;
                    }
                    let (folder, file) = n.rsplit_once('\\').unwrap();
                    m.entry(folder.to_owned()).or_default().push(format!(
                        "{file}[{}]",
                        pk.path().file_name().unwrap().to_string_lossy()
                    ));
                }
            }
            for (k, v) in m {
                if v.len() > 1 {
                    println!("{k}: {}", v.join(" "));
                }
            }
        }
        _ => eprintln!("unknown"),
    }
}

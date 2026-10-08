//! Research helper: list / survey / hex-dump files inside the install's packs.
//! Read-only. Usage:
//!   cargo run -p ntw_formats --example pack_probe -- exts
//!   cargo run -p ntw_formats --example pack_probe -- list <substring>
//!   cargo run -p ntw_formats --example pack_probe -- hex <path> [max_bytes]
//!   cargo run -p ntw_formats --example pack_probe -- extract <path> <out_file>   (scratch dir only!)
//!   cargo run -p ntw_formats --example pack_probe -- extract-prefix <prefix> <out_dir>   (scratch dir only!)
use ntw_formats::pack::Vfs;
use std::collections::BTreeMap;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    match args[0].as_str() {
        "exts" => {
            let mut m: BTreeMap<String, (usize, u64)> = BTreeMap::new();
            for p in vfs.packs() {
                for e in p.entries() {
                    let name = e.path.rsplit(['\\', '/']).next().unwrap_or("");
                    let ext = name.split_once('.').map(|x| x.1.to_ascii_lowercase()).unwrap_or_default();
                    let v = m.entry(format!("{:<28} {}", ext, p.path().file_name().unwrap().to_string_lossy())).or_default();
                    v.0 += 1;
                    v.1 += e.size as u64;
                }
            }
            for (k, v) in m { println!("{k:<50} {:>7} {:>12}", v.0, v.1); }
        }
        "list" => {
            for p in vfs.packs() {
                for e in p.entries() {
                    if e.path.to_ascii_lowercase().contains(&args[1].to_ascii_lowercase()) {
                        println!("{:>10} {} [{}]", e.size, e.path, p.path().file_name().unwrap().to_string_lossy());
                    }
                }
            }
        }
        "hex" => {
            let b = vfs.read(&args[1]).unwrap();
            let max: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(512);
            let off: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(0);
            println!("len {}", b.len());
            for (i, c) in b[off..b.len().min(off + max)].chunks(16).enumerate() {
                let hex: Vec<String> = c.iter().map(|x| format!("{x:02x}")).collect();
                let asc: String = c.iter().map(|&x| if (32..127).contains(&x) { x as char } else { '.' }).collect();
                println!("{:08x}  {:<48} {}", off + i * 16, hex.join(" "), asc);
            }
        }
        "heads" => {
            // Histogram of the first 8 bytes of every file with the given extension.
            let mut m: BTreeMap<String, (usize, String)> = BTreeMap::new();
            for p in vfs.packs() {
                for e in p.entries() {
                    if !e.path.to_ascii_lowercase().ends_with(&args[1].to_ascii_lowercase()) { continue; }
                    let b = p.read_entry_prefix(e, 16).unwrap();
                    let key: Vec<String> = b.iter().take(args.get(2).map(|s| s.parse().unwrap()).unwrap_or(8)).map(|x| format!("{x:02x}")).collect();
                    let v = m.entry(format!("{} {}", key.join(" "), p.path().file_name().unwrap().to_string_lossy())).or_insert((0, e.path.clone()));
                    v.0 += 1;
                }
            }
            for (k, v) in m { println!("{k:<60} {:>6} e.g. {}", v.0, v.1); }
        }
        "ddsfmt" => {
            // Histogram of DDS pixel formats (fourCC or RGB bit count) and flags (cubemap/volume).
            let mut m: BTreeMap<String, (usize, String)> = BTreeMap::new();
            for p in vfs.packs() {
                for e in p.entries() {
                    if !e.path.to_ascii_lowercase().ends_with(".dds") { continue; }
                    let b = p.read_entry_prefix(e, 128).unwrap();
                    if b.len() < 128 { continue; }
                    let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                    let pf_flags = u(80);
                    let fmt = if pf_flags & 4 != 0 { String::from_utf8_lossy(&b[84..88]).into_owned() } else { format!("rgb{}bit flags{pf_flags:x} masks {:x}/{:x}/{:x}/{:x}", u(88), u(92), u(96), u(100), u(104)) };
                    let caps2 = u(112);
                    let mips = u(28);
                    let key = format!("{fmt} caps2={caps2:x} mips>1={}", mips > 1);
                    m.entry(key).or_insert((0, e.path.clone())).0 += 1;
                }
            }
            for (k, v) in m { println!("{k:<70} {:>6} e.g. {}", v.0, v.1); }
        }
        "db" => {
            // db <table> <schema codes, e.g. "s,s,s">: print every row tab-separated.
            use ntw_formats::db::{DbTable, Schema};
            let bytes = vfs.read(&format!("db/{0}_tables/{0}", args[1])).unwrap();
            let t = DbTable::read(&bytes, &Schema::from_codes(&args[2]).unwrap()).unwrap();
            for r in &t.rows {
                let cells: Vec<String> = r.iter().map(|v| format!("{v:?}")).collect();
                println!("{}", cells.join("\t"));
            }
        }
        "extract" => {
            std::fs::write(&args[2], vfs.read(&args[1]).unwrap()).unwrap();
        }
        "extract-prefix" => {
            // extract-prefix <path prefix> <out_dir>: every file under the prefix, flattened by file
            // name (scratch dir only, never into the repo)
            let mut n = 0;
            for p in vfs.list(&args[1]) {
                let name = p.rsplit(['\\', '/']).next().unwrap();
                std::fs::write(std::path::Path::new(&args[2]).join(name), vfs.read(p).unwrap()).unwrap();
                n += 1;
            }
            println!("{n} files");
        }
        _ => eprintln!("unknown command"),
    }
}

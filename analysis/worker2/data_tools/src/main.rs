//! data_tools: read-only analysis tools for Napoleon: Total War data files.
//!
//! Subcommands:
//!   pack-index [pack...]            write pack_indexes/<pack>.txt + pack_summary_rust.txt (all packs if none given)
//!   ls <pack> [substr]              list entries (size, offset, path)
//!   hex <pack> <path> [n] [skip]    hex-dump n bytes of one entry
//!   cat <pack> <path> [n] [skip]    print an entry as text
//!   grep <pack> <path-substr> <s>   find ASCII / UTF-16LE string in matching entries
//!   db-list                         list every DB table with version + row count
//!   db-infer [table...]             infer schemas (all tables if none) -> db_schemas.tsv
//!   db <table> [rows] [schema]      decode a table (schema: comma list s,o,b,n,i,f,h; default inferred)
//!   db-col <table> <col> [schema]   value histogram of one column
//!   loc <pack> <path> [n]           dump a .loc file's header + first n entries
//!   loc-find <substr> [max]         search keys in local_en + local_en_patch .loc files
//!   variant / variant-survey / vmpf-survey / atlas-survey / dds-survey / u32s: graphics surveys, see graphics.rs

mod db;
mod graphics;
mod loc;
mod pack;
mod variant;

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub fn out_dir() -> PathBuf {
    // the folder containing this crate (analysis/worker2), wherever the repo is checked out
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

pub fn hexdump(b: &[u8], base: usize) {
    for (i, ch) in b.chunks(16).enumerate() {
        let hex: Vec<String> = ch.iter().map(|c| format!("{c:02x}")).collect();
        let asc: String = ch.iter().map(|&c| if (32..127).contains(&c) { c as char } else { '.' }).collect();
        println!("{:06x}  {:<48} {}", base + i * 16, hex.join(" "), asc);
    }
}

pub fn db_tables(pi: &pack::PackIndex) -> BTreeMap<String, &pack::PackEntry> {
    let mut m = BTreeMap::new();
    for e in &pi.entries {
        let parts: Vec<&str> = e.path.split('\\').collect();
        if parts.len() == 3 && parts[0].eq_ignore_ascii_case("db") {
            let t = parts[1].strip_suffix("_tables").unwrap_or(parts[1]);
            m.insert(t.to_string(), e);
        }
    }
    m
}

fn cmd_pack_index(args: &[String]) -> std::io::Result<()> {
    let packs: Vec<PathBuf> = if args.is_empty() { pack::all_packs()? } else { args.iter().map(|a| pack::resolve(a)).collect() };
    let dir = out_dir().join("pack_indexes");
    fs::create_dir_all(&dir)?;
    let mut summary = String::new();
    for p in packs {
        let pi = pack::PackIndex::open(&p)?;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let mut top: BTreeMap<String, usize> = BTreeMap::new();
        let mut ext: BTreeMap<String, (usize, u64)> = BTreeMap::new();
        let total: u64 = pi.entries.iter().map(|e| e.size as u64).sum();
        let mut f = fs::File::create(dir.join(format!("{name}.txt")))?;
        writeln!(f, "# {name} magic={} type={}({:?}) deps={:?} files={} data_start=0x{:X} file_len={} payload_sum={} payload_end_matches_eof={}",
            String::from_utf8_lossy(&pi.magic), pi.raw_type, pi.pack_type, pi.dependencies, pi.entries.len(), pi.data_start, pi.file_len, total, pi.payload_end() == pi.file_len)?;
        writeln!(f, "# size\toffset\tpath")?;
        for e in &pi.entries {
            writeln!(f, "{}\t{}\t{}", e.size, e.offset, e.path)?;
            *top.entry(e.path.split('\\').next().unwrap().to_string()).or_default() += 1;
            let x = e.path.rsplit('\\').next().unwrap();
            let x = x.rfind('.').map(|i| x[i..].to_ascii_lowercase()).unwrap_or("(none)".into());
            let s = ext.entry(x).or_default();
            s.0 += 1;
            s.1 += e.size as u64;
        }
        let mut topv: Vec<_> = top.into_iter().collect();
        topv.sort_by(|a, b| b.1.cmp(&a.1));
        let mut extv: Vec<_> = ext.into_iter().collect();
        extv.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        summary += &format!("=== {name} magic={} type={} ({:?}) deps={:?} files={} data_start=0x{:X} file_len={} payload_sum={} eof_ok={}\n",
            String::from_utf8_lossy(&pi.magic), pi.raw_type, pi.pack_type, pi.dependencies, pi.entries.len(), pi.data_start, pi.file_len, total, pi.payload_end() == pi.file_len);
        summary += &format!("  top: {}\n", topv.iter().map(|(k, v)| format!("{k}({v})")).collect::<Vec<_>>().join(", "));
        summary += &format!("  ext: {}\n", extv.iter().map(|(k, (n, s))| format!("{k}({n}, {:.1}MB)", *s as f64 / 1e6)).collect::<Vec<_>>().join(", "));
    }
    fs::write(out_dir().join("pack_summary_rust.txt"), &summary)?;
    print!("{summary}");
    Ok(())
}

fn find_entry<'a>(pi: &'a pack::PackIndex, path: &str) -> &'a pack::PackEntry {
    pi.find(path).unwrap_or_else(|| panic!("no entry {path}"))
}

pub fn schema_str(s: &[db::FieldType]) -> String {
    s.iter().map(|t| t.code()).collect::<Vec<_>>().join(",")
}

fn get_schema_for(table: &str, b: &[u8], arg: Option<&String>) -> Vec<db::FieldType> {
    if arg.is_none() {
        if let Some(k) = db::known_schema(table) {
            return k.split(',').map(|x| db::FieldType::parse(x).unwrap()).collect();
        }
    }
    get_schema(b, arg)
}

fn get_schema(b: &[u8], arg: Option<&String>) -> Vec<db::FieldType> {
    match arg {
        Some(s) => s.split(',').map(|x| db::FieldType::parse(x).expect("bad type")).collect(),
        None => db::infer_adaptive(b, 200_000).unwrap().schema.expect("inference failed"),
    }
}

fn main() -> std::io::Result<()> {
    // deep recursion in schema search: run on a big stack
    std::thread::Builder::new().stack_size(1 << 30).spawn(real_main).unwrap().join().unwrap()
}

fn real_main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    let rest = &args[1.min(args.len())..];
    match cmd {
        "pack-index" => cmd_pack_index(rest)?,
        "ls" => {
            let pi = pack::PackIndex::open(pack::resolve(&rest[0]))?;
            let sub = rest.get(1).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
            for e in pi.entries.iter().filter(|e| e.path.to_ascii_lowercase().contains(&sub)) {
                println!("{}\t{}\t{}", e.size, e.offset, e.path);
            }
        }
        "hex" | "cat" => {
            let pi = pack::PackIndex::open(pack::resolve(&rest[0]))?;
            let e = find_entry(&pi, &rest[1]);
            let n: usize = rest.get(2).and_then(|s| s.parse().ok()).unwrap_or(256);
            let skip: usize = rest.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            let b = pi.read(e, Some(n + skip))?;
            let b = &b[skip.min(b.len())..];
            if cmd == "hex" { hexdump(b, skip) } else { print!("{}", String::from_utf8_lossy(b)) }
        }
        "grep" => {
            let pi = pack::PackIndex::open(pack::resolve(&rest[0]))?;
            let sub = rest[1].to_ascii_lowercase();
            let needle = rest[2].as_bytes().to_vec();
            let n16: Vec<u8> = rest[2].bytes().flat_map(|c| [c, 0]).collect();
            for e in pi.entries.iter().filter(|e| e.path.to_ascii_lowercase().contains(&sub)) {
                if e.size > 64_000_000 { continue; }
                let b = pi.read(e, None)?;
                for nd in [&needle, &n16] {
                    if let Some(i) = b.windows(nd.len()).position(|w| w == &nd[..]) {
                        println!("{} @{} (len {})", e.path, i, e.size);
                    }
                }
            }
        }
        "db-list" => {
            let pi = pack::PackIndex::open(pack::resolve("data"))?;
            for (t, e) in db_tables(&pi) {
                let b = pi.read(e, Some(16))?;
                let h = db::read_header(&b).unwrap();
                println!("{t}\tv{}\tmarker={}\trows={}\tbytes={}", h.version, h.marker, h.rows, e.size);
            }
        }
        "db-infer" => {
            let pi = pack::PackIndex::open(pack::resolve("data"))?;
            let tables = db_tables(&pi);
            let names: Vec<String> = if rest.is_empty() { tables.keys().cloned().collect() } else { rest.to_vec() };
            let mut out = String::from("table\tversion\trows\tbytes\tstatus\tschema\tclassified\n");
            let mut ex = String::new();
            for t in names {
                let e = tables[&t];
                let b = pi.read(e, None)?;
                let r = if t == "models_building" || t == "models_naval" {
                    // huge float rows; search is very slow and the data is visual-only
                    db::InferResult { header: db::read_header(&b).unwrap(), schema: None, nodes: 0 }
                } else if let Some(k) = db::known_schema(&t) {
                    db::InferResult { header: db::read_header(&b).unwrap(), schema: Some(k.split(',').map(|x| db::FieldType::parse(x).unwrap()).collect()), nodes: 0 }
                } else {
                    db::infer_adaptive(&b, 200_000).unwrap()
                };
                let (status, s1, s2) = match &r.schema {
                    Some(s) => {
                        let (ty, rows) = db::decode(&b, s).unwrap();
                        ex += &format!("## {t} v{} rows={} schema={}\n", r.header.version, r.header.rows, schema_str(&ty));
                        for row in rows.iter().take(3) {
                            ex += &row.iter().map(|v| { let s = v.to_string(); if s.chars().count() > 60 { format!("{}..", s.chars().take(58).collect::<String>()) } else { s } }).collect::<Vec<_>>().join(" | ");
                            ex.push('\n');
                        }
                        ("ok", schema_str(s), schema_str(&ty))
                    }
                    None => ("FAIL", String::new(), String::new()),
                };
                println!("{t:<58} v{} rows={:<6} {status} cols={} nodes={} {}", r.header.version, r.header.rows, s2.split(',').filter(|x| !x.is_empty()).count(), r.nodes, s2);
                out += &format!("{t}\t{}\t{}\t{}\t{status}\t{s1}\t{s2}\n", r.header.version, r.header.rows, e.size);
                if rest.is_empty() {
                    fs::write(out_dir().join("db_schemas.tsv"), &out)?;
                    fs::write(out_dir().join("db_examples.txt"), &ex)?;
                }
            }
            if rest.is_empty() {
                fs::write(out_dir().join("db_schemas.tsv"), out)?;
                fs::write(out_dir().join("db_examples.txt"), ex)?;
            }
        }
        "db" => {
            let pi = pack::PackIndex::open(pack::resolve("data"))?;
            let tables = db_tables(&pi);
            let b = pi.read(tables[&rest[0]], None)?;
            let nrows: usize = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);
            let schema = get_schema_for(&rest[0], &b, rest.get(2));
            let h = db::read_header(&b).unwrap();
            match db::decode(&b, &schema) {
                Some((ty, rows)) => {
                    println!("# {} v{} rows={} valid={} schema={}", rest[0], h.version, h.rows, db::validate(&b, &h, &schema), schema_str(&ty));
                    for r in rows.iter().take(nrows) {
                        println!("{}", r.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("\t"));
                    }
                }
                None => println!("decode failed"),
            }
        }
        "db-col" => {
            let pi = pack::PackIndex::open(pack::resolve("data"))?;
            let tables = db_tables(&pi);
            let b = pi.read(tables[&rest[0]], None)?;
            let schema = get_schema_for(&rest[0], &b, rest.get(2));
            let (_, rows) = db::decode(&b, &schema).unwrap();
            let ci: usize = rest[1].parse().unwrap();
            let mut hist: BTreeMap<String, usize> = BTreeMap::new();
            for r in &rows {
                *hist.entry(r[ci].to_string()).or_default() += 1;
            }
            let mut v: Vec<_> = hist.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            println!("distinct={}", v.len());
            for (k, n) in v.iter().take(40) {
                println!("{n}\t{k}");
            }
        }
        "db-starts" => debug_starts(&rest[0])?,
        "variant" => graphics::cmd_variant(rest)?,
        "variant-survey" => graphics::cmd_variant_survey()?,
        "vmpf-survey" => graphics::cmd_vmpf_survey()?,
        "atlas-survey" => graphics::cmd_atlas_survey()?,
        "dds-survey" => graphics::cmd_dds_survey(rest)?,
        "u32s" => graphics::cmd_u32s(rest)?,
        "find-pair" => graphics::cmd_find_pair(rest)?,
        "vmpf-probe" => graphics::cmd_vmpf_probe()?,
        "lattice" => graphics::cmd_lattice(rest)?,
        "loc" => loc::cmd_loc(rest)?,
        "loc-find" => loc::cmd_loc_find(rest)?,
        _ => println!("see source header for usage"),
    }
    Ok(())
}

#[allow(dead_code)]
pub fn debug_starts(table: &str) -> std::io::Result<()> {
    let pi = pack::PackIndex::open(pack::resolve("data"))?;
    let tables = db_tables(&pi);
    let b = pi.read(tables[table], None)?;
    let h = db::read_header(&b).unwrap();
    let s = db::debug_signature_counts(&b, &h);
    println!("{s}");
    Ok(())
}

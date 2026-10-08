//! Research helper (0-G, read-only): lists the `db\*_tables\*` files in the install's `data.pack`
//! and, for one named table, prints its raw header (version + row count) so a candidate table can
//! be matched to an exe struct name without Ghidra.
//!
//! Usage: `cargo run --release -p ntw_data --example table_list -- [table_substring]`
//!
//! **Review fix:** `Vfs::list` returns file paths (`db\x_tables\x`), not directories, so the old
//! `strip_suffix("_tables")` never matched and the tool printed nothing; and the header was read as
//! `byte 0 = version, bytes 1..5 = rows`, which is wrong for every table carrying the
//! `FC FD FE FF` version marker. Both now go through the table's own path and `DbHeader`.
use ntw_formats::db::DbHeader;

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let dir = std::env::var("NTW_DATA_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data".into());
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).expect("vfs");
    if filter == "--paths" {
        for p in vfs.list("db") {
            println!("{p}");
        }
        return;
    }
    // Every `db\<name>_tables\<file>` path, keyed by the table name.
    let mut paths: Vec<(String, String)> = vfs
        .list("db")
        .into_iter()
        .filter_map(|p| {
            let rest = p.strip_prefix("db\\")?;
            let (dir, _) = rest.split_once('\\')?;
            Some((dir.strip_suffix("_tables")?.to_string(), p.to_string()))
        })
        .collect();
    paths.sort();
    for (name, path) in &paths {
        if !name.contains(&filter) {
            continue;
        }
        match vfs.read(path) {
            Ok(b) => match DbHeader::read(&b) {
                Ok(h) => println!("{path:64} version={} rows={} bytes={}", h.version, h.row_count, b.len()),
                Err(e) => println!("{path:64} bad header ({e}), bytes={}", b.len()),
            },
            Err(e) => println!("{path:64} ERROR {e}"),
        }
    }
}

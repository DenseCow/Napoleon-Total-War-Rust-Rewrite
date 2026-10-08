//! Research helper: tries candidate column schemas on a DB table from the packs and prints the
//! ones that read every row, with the first rows. Read-only. Codes as `Schema::from_codes`
//! (s string, o optional string, i i32, f f32, b bool).
//!   cargo run -p ntw_formats --release --example db_schema_probe -- <table path> <codes> [<codes>...]
use ntw_formats::db::{DbTable, Schema};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let bytes = vfs.read(&args[0]).unwrap();
    for codes in &args[1..] {
        let Some(schema) = Schema::from_codes(codes) else { continue };
        match DbTable::read(&bytes, &schema) {
            Ok(t) => {
                println!("{codes}: OK, {} rows (v{})", t.rows.len(), t.version);
                for r in t.rows.iter().take(std::env::var("ROWS").ok().and_then(|s| s.parse().ok()).unwrap_or(6)) {
                    println!("  {r:?}");
                }
            }
            Err(e) => println!("{codes}: {e:?}"),
        }
    }
}

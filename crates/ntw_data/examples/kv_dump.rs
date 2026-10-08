//! Research helper (read-only): prints every `_kv_morale` and `_kv_fatigue` entry.
//!   cargo run -p ntw_data --example kv_dump -- [filter]
use ntw_data::GameDatabase;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = GameDatabase::from_install(&dir).unwrap();
    for (table, raw) in [("kv_morale", &db.kv_morale_raw), ("kv_fatigue", &db.kv_fatigue_raw)] {
        for (k, v) in raw.entries() {
            if k.contains(&filter) {
                println!("{table} {k} = {v}");
            }
        }
    }
}

//! Research helper (read-only): prints `battle_weather_types`, `fatigue_effects` and the land units'
//! category / class pairs (to map units to a fatigue-effects category).
//!   cargo run -p ntw_data --example weather_fatigue_probe
use ntw_data::GameDatabase;
use std::collections::BTreeMap;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = GameDatabase::from_install(&dir).unwrap();
    for w in db.battle_weather_types.iter() {
        println!("weather {:16} intensity {} kind {}", w.key, w.intensity, w.kind);
    }
    for f in db.fatigue_effects.iter() {
        println!("fatigue {:22} {:20} speed {:5} charge {:5} control {:5} attack {:5}", f.threshold, f.category, f.speed, f.charge, f.control, f.attack);
    }
    let mut pairs: BTreeMap<(String, String), u32> = BTreeMap::new();
    for u in db.units.iter().filter(|u| !u.category.starts_with("naval")) {
        *pairs.entry((u.category.clone(), u.unit_class.clone())).or_default() += 1;
    }
    for ((c, k), n) in pairs {
        println!("unit {c:12} {k:28} {n}");
    }
}

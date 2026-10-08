//! Research helper (read-only): for each boolean column of `unit_stats_land` (cols 52..65,
//! 69..84, 88; the unit attribute flags) prints how many units set it and a few of their keys,
//! to help name the flags. Usage:
//!   cargo run -p ntw_data --example attr_flag_probe -- [max_keys]
use ntw_data::GameDatabase;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let max_keys: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(12);
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = GameDatabase::from_install(&dir).unwrap();
    type Get = fn(&ntw_data::UnitStatsLand) -> bool;
    let cols: [(u32, &str, Get); 31] = [
        (52, "1e8", |s| s.unknown_1e8),
        (53, "1e9", |s| s.unknown_1e9),
        (54, "1ea", |s| s.unknown_1ea),
        (55, "1eb", |s| s.unknown_1eb),
        (56, "1ec", |s| s.unknown_1ec),
        (57, "1ed", |s| s.unknown_1ed),
        (58, "1ee", |s| s.unknown_1ee),
        (59, "1ef", |s| s.unknown_1ef),
        (60, "1f0", |s| s.unknown_1f0),
        (61, "1f1", |s| s.unknown_1f1),
        (62, "1f2", |s| s.unknown_1f2),
        (63, "1f3", |s| s.unknown_1f3),
        (64, "1f4", |s| s.unknown_1f4),
        (65, "1f5", |s| s.unknown_1f5),
        (69, "204", |s| s.unknown_204),
        (70, "205", |s| s.unknown_205),
        (71, "206", |s| s.unknown_206),
        (72, "207", |s| s.unknown_207),
        (73, "208", |s| s.unknown_208),
        (74, "209", |s| s.unknown_209),
        (75, "20a", |s| s.unknown_20a),
        (76, "20b", |s| s.unknown_20b),
        (77, "20c", |s| s.unknown_20c),
        (78, "20d", |s| s.unknown_20d),
        (79, "20e", |s| s.unknown_20e),
        (80, "20f", |s| s.unknown_20f),
        (81, "210", |s| s.unknown_210),
        (82, "211", |s| s.unknown_211),
        (83, "212", |s| s.unknown_212),
        (84, "213", |s| s.unknown_213),
        (88, "238", |s| s.unknown_238),
    ];
    let total = db.unit_stats_land.iter().count();
    println!("{total} unit_stats_land rows");
    for (col, off, get) in cols {
        let keys: Vec<&str> = db.unit_stats_land.iter().filter(|s| get(s)).map(|s| s.key.as_str()).collect();
        let shown: Vec<&str> = keys.iter().take(max_keys).copied().collect();
        println!("col {col:2} @0x{off}: {:4} | {}", keys.len(), shown.join(" "));
    }
    // Artillery rows: own projectile (col 30) set? The strength code reads only that one.
    let art: Vec<_> = db.unit_stats_land.iter().filter(|s| s.is_artillery).collect();
    let with_proj = art.iter().filter(|s| s.projectile.as_deref().is_some_and(|p| !p.is_empty())).count();
    println!("artillery rows: {}, with a col 30 projectile: {with_proj}", art.len());
}

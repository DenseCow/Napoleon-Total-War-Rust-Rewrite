//! Research helper (read-only): prints a battle map's near and far building lists in file order,
//! to match the battle scripts' `battle:buildings():item(n)`.
//!   cargo run -p ntw_formats --example building_order_probe -- <preset name, e.g. hb_lodi> [max]
use ntw_formats::battle_terrain::BattleMap;
use ntw_formats::pack::Vfs;


fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "hb_lodi".into());
    let max: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(60);
    let vfs = Vfs::open_install(ntw_formats::install::data_dir()).unwrap();
    let map = BattleMap::load(&vfs, &name).unwrap();
    for (label, list) in [("near", &map.buildings_near), ("far", &map.buildings_far)] {
        println!("{label}: {} buildings", list.len());
        for (i, b) in list.iter().enumerate().take(max) {
            println!("  {:3} {:40} ({:.1}, {:.1})", i + 1, b.key, b.position.0, b.position.1);
        }
    }
}

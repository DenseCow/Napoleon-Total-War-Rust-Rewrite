//! Research helper (read-only): how the saved international trade routes map onto the model's
//! (exporter, importer) trade partners (SAVE_COMPAT.md §12).
//!
//! ```text
//! cargo run --release -p ntw_campaign --example trade_check -- FILE...
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::esf::EsfFile;

fn main() {
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let db = GameDatabase::from_install(&dir).expect("db");
    for p in std::env::args().skip(1) {
        let bytes = std::fs::read(&p).expect("read");
        let esf = EsfFile::from_bytes(&bytes).expect("esf");
        let Ok(l) = ntw_campaign::read(&bytes, &db) else {
            println!("{p}: does not load");
            continue;
        };
        let m = &l.model;
        let routes = ntw_campaign::trade::routes(&esf);
        // The commerce-raid lists (+0x60) and the routes with an accumulated value (+0x3C).
        let raided = routes.iter().filter(|r| r.raids.is_some_and(|n| n > 0)).count();
        let no_raid_list = routes.iter().filter(|r| r.raids.is_none()).count();
        let accumulated = routes.iter().filter(|r| r.accumulated != 0).count();
        let mut per_pair: BTreeMap<(i32, i32), usize> = BTreeMap::new();
        let (mut partner, mut not, mut unknown) = (0, 0, 0);
        for r in &routes {
            let Some(importer) = r.importer(m) else {
                unknown += 1;
                continue;
            };
            *per_pair.entry((r.exporter.raw(), importer.raw())).or_default() += 1;
            if ntw_sim::campaign::economy::trade_partners(m, r.exporter).contains(&importer) {
                partner += 1;
            } else {
                not += 1;
            }
        }
        let partners: usize = m.world.factions.keys().map(|&f| ntw_sim::campaign::economy::trade_partners(m, f).len()).sum();
        let dup = per_pair.values().filter(|&&n| n > 1).count();
        println!(
            "{p}: {} routes; importer is a model trade partner {partner}, not {not}, unknown {unknown}; pairs {} (with several routes {dup}); model partner pairs {partners}; raid list not empty {raided} (no raid list {no_raid_list}); accumulated not 0 {accumulated}",
            routes.len(),
            per_pair.len()
        );
    }
}

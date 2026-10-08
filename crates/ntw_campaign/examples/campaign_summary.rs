//! Loads start positions or saves and prints what ended up in the campaign model.
//!
//! ```text
//! cargo run -p ntw_campaign --release --example campaign_summary -- <data dir> <file.esf|file.save>...
//! ```
//! `<data dir>` is the install's `data` folder (used for the game database). Files are only read.

use std::collections::BTreeMap;

use ntw_data::GameDatabase;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((data, files)) = args.split_first() else {
        eprintln!("usage: campaign_summary <data dir> <file>...");
        std::process::exit(2);
    };
    let db = match GameDatabase::from_install(data) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("cannot load the game database from {data}: {e}");
            std::process::exit(1);
        }
    };
    for file in files {
        let l = match ntw_campaign::read_file(file, &db) {
            Ok(l) => l,
            Err(e) => {
                println!("{file}: ERROR {e}");
                continue;
            }
        };
        let w = &l.model.world;
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for c in w.characters.values() {
            *kinds.entry(c.kind.esf_name()).or_default() += 1;
        }
        let units: usize = w.forces.values().map(|f| f.units.len()).sum();
        let queued: usize = w.regions.values().map(|r| r.recruitment_queue.len()).sum();
        println!(
            "{file}\n  {:?} campaign {} map {} build {:?}\n  date {:?} turn {} seed {}\n  factions {} regions {} characters {} {:?}\n  armies {} navies {} units {} recruiting {}",
            l.info.kind,
            l.info.campaign_key,
            l.info.map_key,
            l.info.build_version,
            l.model.calendar.date,
            l.model.calendar.turn_number(),
            l.model.rng.state,
            w.factions.len(),
            w.regions.len(),
            w.characters.len(),
            kinds,
            w.forces.values().filter(|f| !f.is_navy).count(),
            w.forces.values().filter(|f| f.is_navy).count(),
            units,
            queued,
        );
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for warning in &l.warnings {
            let text = warning.to_string();
            let kind = text
                .split(':')
                .next_back()
                .unwrap_or_default()
                .trim()
                .to_string();
            *counts.entry(kind).or_default() += 1;
        }
        for (text, n) in counts.iter().take(40) {
            println!("  warning x{n}: {text}");
        }
    }
}

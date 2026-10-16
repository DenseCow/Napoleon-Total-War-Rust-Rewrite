//! The diplomacy manager counters (`DIPLOMACY_MANAGER` #1..#4: alliances broken, access abuses,
//! treaty breaks, access cancel marks; reader `0x00AE8E10`, writer `0x00AFC160`) against the shipped
//! start position and the vanilla evidence saves (read-only; each test skips without its files).

mod common;

use std::path::PathBuf;

use ntw_campaign::save;
use ntw_data::GameDatabase;
use ntw_formats::esf::EsfFile;
use ntw_sim::campaign::{CampaignModel, World};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// Every faction with a manager (all but the rebels) holds the constructors' values: treaty breaks
/// 1, access abuses 0, cancel marks 0; `alliances` is how many hold an alliance-broken count of 1
/// (all others 0).
fn assert_vanilla_counters(m: &CampaignModel, what: &str, alliances: usize) {
    let w: &World = &m.world;
    let managed = w.factions.len() - 1;
    assert_eq!(w.treaty_breaks.len(), managed, "{what}: one entry per faction with a manager");
    assert!(w.treaty_breaks.values().all(|&n| n == 1), "{what}: {:?}", w.treaty_breaks);
    assert!(w.factions.keys().all(|&f| w.treaty_breaks_of(f) == 1), "{what}: the rebels default to the preset");
    assert_eq!(w.access_abuses.len(), managed, "{what}");
    assert!(w.access_abuses.values().all(|&n| n == 0), "{what}: {:?}", w.access_abuses);
    assert_eq!(w.access_cancel_marks.len(), managed, "{what}");
    assert!(w.access_cancel_marks.values().all(|&n| n == 0), "{what}: {:?}", w.access_cancel_marks);
    assert_eq!(w.alliances_broken.len(), managed, "{what}");
    assert_eq!(w.alliances_broken.values().filter(|&&n| n == 1).count(), alliances, "{what}: {:?}", w.alliances_broken);
    assert!(w.alliances_broken.values().all(|&n| n <= 1), "{what}");
}

/// The original's own saves hold a treaty-break count of 1 for every faction (the constructors'
/// preset, `0x00AE8DD0`); the autosave also holds one faction that broke an alliance while committed.
#[test]
fn vanilla_saves_hold_one_treaty_break_per_faction() {
    let dir = data_dir();
    let ev = common::evidence_dir();
    if !dir.is_dir() || !ev.is_dir() {
        println!("SKIP: no install at {} or no evidence saves at {}", dir.display(), ev.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    for (name, alliances) in [("orig_fr_t1.save", 0), ("orig_fr_may1811.save", 0), (r"original\auto_save.save", 1)] {
        let path = ev.join(name);
        if !path.is_file() {
            println!("SKIP: no {}", path.display());
            continue;
        }
        let m = ntw_campaign::read_file(&path, &db).expect("save loads").model;
        assert_vanilla_counters(&m, name, alliances);
        println!("{name}: counters OK");
    }
}

/// The start position loads the four counters, and a save written from the model reloads with
/// the values the model holds (a faction without an entry writes the constructors' values).
#[test]
fn counters_round_trip_through_a_save() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let bytes = std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap();
    let esf = EsfFile::from_bytes(&bytes).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    assert_vanilla_counters(&l.model, "eur_napoleon startpos", 0);
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let austria = m.faction_by_key("austria").unwrap().id;
    let w = &mut m.world;
    w.alliances_broken.insert(france, 2);
    w.access_abuses.insert(france, 3);
    w.treaty_breaks.insert(france, 7);
    w.access_cancel_marks.insert(france, 16);
    w.treaty_breaks.remove(&austria);
    let human = l.info.header.faction_key.clone();
    let out = save::write_save(&esf, &m, &human, 1).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap().model.world;
    assert_eq!((back.alliances_broken[&france], back.access_abuses[&france], back.treaty_breaks[&france], back.access_cancel_marks[&france]), (2, 3, 7, 16));
    assert_eq!(back.treaty_breaks[&austria], 1, "no entry writes the preset");
    m.world.treaty_breaks.insert(austria, 1);
    for (a, b) in [(&back.alliances_broken, &m.world.alliances_broken), (&back.access_abuses, &m.world.access_abuses), (&back.treaty_breaks, &m.world.treaty_breaks), (&back.access_cancel_marks, &m.world.access_cancel_marks)] {
        assert_eq!(a, b);
    }
}

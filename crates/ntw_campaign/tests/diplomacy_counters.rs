//! The diplomacy manager counters (`DIPLOMACY_MANAGER` #1..#4: alliances broken, access abuses,
//! treaty breaks, access cancel marks; reader `0x00AE8E10`, writer `0x00AFC160`) against the shipped
//! start position and the vanilla evidence saves (read-only; each test skips without its files).
//! Also the relationships' treaty money (`DIPLOMACY_RELATIONSHIP` #8 / #9 / #14) through a save, and the
//! factions' war-started flag and the diplomacy records' order.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use ntw_campaign::save;
use ntw_data::GameDatabase;
use ntw_formats::esf::EsfFile;
use ntw_sim::campaign::{CampaignModel, FactionId, Stance, World};

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
    let out = save::write_save(&esf, &m, &human, 1, None).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap().model.world;
    assert_eq!((back.alliances_broken[&france], back.access_abuses[&france], back.treaty_breaks[&france], back.access_cancel_marks[&france]), (2, 3, 7, 16));
    assert_eq!(back.treaty_breaks[&austria], 1, "no entry writes the preset");
    m.world.treaty_breaks.insert(austria, 1);
    for (a, b) in [(&back.alliances_broken, &m.world.alliances_broken), (&back.access_abuses, &m.world.access_abuses), (&back.treaty_breaks, &m.world.treaty_breaks), (&back.access_cancel_marks, &m.world.access_cancel_marks)] {
        assert_eq!(a, b);
    }
}

/// Each faction's `DIPLOMACY_RELATIONSHIP` records, keyed by (owner index, target id).
fn relationship_records(esf: &EsfFile) -> Vec<((usize, i32), ntw_formats::esf::EsfRecord)> {
    let mut out = Vec::new();
    let mut owner = 0;
    esf.root.walk(&mut |r| {
        if r.name == "DIPLOMACY_MANAGER" {
            owner += 1;
        }
        if r.name == "DIPLOMACY_RELATIONSHIP" {
            out.push(((owner, r.get_i32(0).unwrap()), r.clone()));
        }
    });
    out
}

/// The treaty money of `DIPLOMACY_RELATIONSHIP` (#8 protectorate tribute, #9 patron income, #14
/// `REGULAR_PAYMENTS[]`; writer `0x00AFC340`): an unchanged model writes the three fields back as
/// loaded, and each value changed in the model reloads as written.
#[test]
fn treaty_money_round_trips_through_a_save() {
    let dir = data_dir();
    let path = common::evidence_dir().join("orig_fr_t1.save");
    if !dir.is_dir() || !path.is_file() {
        println!("SKIP: no install at {} or no {}", dir.display(), path.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let esf = EsfFile::from_bytes(&std::fs::read(&path).unwrap()).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let human = l.info.header.faction_key.clone();

    let same = save::write_save(&esf, &l.model, &human, 1, None).unwrap();
    let (src, out) = (relationship_records(&esf), relationship_records(&same));
    assert_eq!(src.len(), out.len());
    for ((k, a), (_, b)) in src.iter().zip(&out) {
        for i in [8, 9, 14] {
            assert_eq!(a.get(i), b.get(i), "relationship {k:?} #{i} changed by an unchanged model");
        }
    }

    let mut m = l.model.clone();
    let owner = m.faction_by_key(&human).expect("the human faction").id;
    // Two of the human's partners (any two hold the same fields).
    let mut partners = m.world.relationships.keys().filter(|(a, b)| *a == owner && *b != owner && m.world.relationships.contains_key(&(*b, owner))).map(|k| k.1);
    let (first, second) = (partners.next().expect("a partner"), partners.next().expect("a second partner"));
    let payments = vec![
        ntw_sim::campaign::details::RegularPayment { amount: 750, turns: 9 },
        ntw_sim::campaign::details::RegularPayment { amount: 120, turns: 1 },
    ];
    {
        let rels = &mut m.world.relationships;
        let fa = rels.get_mut(&(owner, first)).expect("owner -> first partner");
        fa.protectorate_tribute = 431;
        fa.payments = payments.clone();
        rels.get_mut(&(first, owner)).expect("first -> owner").protectorate_income = 431;
        rels.get_mut(&(second, owner)).expect("second -> owner").protectorate_income = -7;
    }
    let out = save::write_save(&esf, &m, &human, 1, None).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap().model.world.relationships;
    let fa = &back[&(owner, first)];
    assert_eq!((fa.protectorate_tribute, fa.protectorate_income), (431, 0));
    assert_eq!(fa.payments, payments);
    assert_eq!(back[&(first, owner)].protectorate_income, 431);
    assert_eq!(back[&(second, owner)].protectorate_income, -7);
    for (k, r) in &back {
        let want = &m.world.relationships[k];
        assert_eq!((r.protectorate_tribute, r.protectorate_income, &r.payments), (want.protectorate_tribute, want.protectorate_income, &want.payments), "{k:?}");
    }

    // Payments taken off again: the list empties in the file.
    m.world.relationships.get_mut(&(owner, first)).unwrap().payments.clear();
    let out2 = save::write_save(&out, &m, &human, 1, None).unwrap();
    let back = ntw_campaign::read(&out2.to_bytes().unwrap(), &db).unwrap().model.world.relationships;
    assert!(back[&(owner, first)].payments.is_empty());
}

/// The `WarStartedCharacterFaction` flag (`FACTION` #63, faction +0x925, read at `0x0087B769`): the start
/// position holds it set on the factions at war, and a save written from the model reloads what the model
/// holds. The diplomacy records' order (`World::diplomacy_record_order`) is the file's: every faction's
/// list names each of its records once.
#[test]
fn war_started_and_record_order_load_and_round_trip() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let bytes = std::fs::read(dir.join(r"campaigns\eur_napoleon\startpos.esf")).unwrap();
    let esf = EsfFile::from_bytes(&bytes).unwrap();
    let l = ntw_campaign::read_esf(&esf, &db).unwrap();
    let w = &l.model.world;
    // The start position was saved after its opening wars were declared: the flag is set on exactly the
    // factions at war with another (non-rebel) faction, until their first turn end.
    let started: BTreeSet<FactionId> = w.faction_details.iter().filter(|(_, d)| d.war_started).map(|(&f, _)| f).collect();
    let at_war: BTreeSet<FactionId> = w.factions.keys().copied().filter(|&a| w.factions.keys().any(|&b| a != b && !w.is_rebel_faction(b) && w.stance(a, b) == Stance::War)).collect();
    assert_eq!(started, at_war);
    assert!(!started.is_empty());
    for (&owner, order) in &w.diplomacy_record_order {
        let held = w.relationships.keys().filter(|(o, _)| *o == owner).count();
        assert_eq!(order.len(), held, "{owner:?}: one entry per record");
        let unique: BTreeSet<_> = order.iter().collect();
        assert_eq!(unique.len(), order.len(), "{owner:?}: no record twice");
    }
    assert!(!w.diplomacy_record_order.is_empty());
    let mut m = l.model.clone();
    let france = m.faction_by_key("france").unwrap().id;
    let austria = m.faction_by_key("austria").unwrap().id;
    let portugal = m.faction_by_key("portugal").unwrap().id;
    assert!(started.contains(&france) && started.contains(&austria) && !started.contains(&portugal));
    m.world.faction_details.get_mut(&france).unwrap().war_started = false;
    m.world.faction_details.get_mut(&portugal).unwrap().war_started = true;
    let human = l.info.header.faction_key.clone();
    let out = save::write_save(&esf, &m, &human, 1, None).unwrap();
    let back = ntw_campaign::read(&out.to_bytes().unwrap(), &db).unwrap().model.world;
    assert!(!back.faction_details[&france].war_started);
    assert!(back.faction_details[&portugal].war_started);
    assert!(back.faction_details[&austria].war_started, "the others keep the model's");
    assert_eq!(back.diplomacy_record_order, w.diplomacy_record_order, "the records keep their order");
}

/// The vanilla saves hold the diplomacy records out of faction-id order (the order `0x00AE8E10` appends them
/// in), which is the order the allies of a war are called in (`0x00B268B0`).
#[test]
fn vanilla_saves_hold_the_records_out_of_id_order() {
    let dir = data_dir();
    let path = common::evidence_dir().join("orig_fr_may1811.save");
    if !dir.is_dir() || !path.is_file() {
        println!("SKIP: no install at {} or no {}", dir.display(), path.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let w = ntw_campaign::read_file(&path, &db).expect("save loads").model.world;
    let unsorted = w.diplomacy_record_order.values().filter(|o| o.windows(2).any(|p| p[0] > p[1])).count();
    println!("orig_fr_may1811: {unsorted} of {} factions' records out of id order", w.diplomacy_record_order.len());
    assert!(unsorted > 0);
    let started: Vec<&str> = w.faction_details.iter().filter(|(_, d)| d.war_started).filter_map(|(f, _)| w.factions.get(f).map(|x| x.key.as_str())).collect();
    println!("orig_fr_may1811: war_started set for {started:?}");
}

//! Settlement residents (`SETTLEMENT/SIEGEABLE_GARRISON_RESIDENCE` #14, the settlement residence's
//! list +0xC8 the loader `0x00AF4B00` fills through `0x008CFA70`) against a vanilla save: the
//! importer makes each listed agent resident, and our save writes the model's residents back, with
//! `CHARACTER` #7 naming the residence. Skips (passes, printing a note) without the install or the
//! evidence save.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::{CharacterId, RegionId};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// Region id → (settlement residence id, #14 resident ids) as stored.
fn stored_residents(esf: &EsfFile) -> BTreeMap<u32, (u32, Vec<u32>)> {
    let regions = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY").expect("regions");
    let mut out = BTreeMap::new();
    for r in regions.records() {
        let (Some(id), Some(g)) = (r.get_i32(4), r.child("SETTLEMENT").and_then(|s| s.child("SIEGEABLE_GARRISON_RESIDENCE"))) else { continue };
        let ids = g.get(14).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
        out.insert(id as u32, (g.get_u32(1).unwrap_or(0), ids));
    }
    out
}

/// Character id → `CHARACTER` #7 as stored.
fn stored_residence_of(esf: &EsfFile) -> BTreeMap<i32, u32> {
    let mut out = BTreeMap::new();
    let mut visit = |r: &EsfRecord| {
        if r.name == "CHARACTER"
            && let (Some(id), Some(res)) = (r.get_i32(2), r.get_u32(7))
        {
            out.insert(id, res);
        }
    };
    esf.root.walk(&mut visit);
    out
}

/// The vanilla Great Britain April 1811 save holds five missionaries resident in Spanish
/// settlements (e.g. 865679480 in `spa_alentejo`, whose `CHARACTER` #7 is that settlement's
/// residence id). Every #14 list loads as the model's residents; an unchanged model writes the lists
/// and #7 back as stored; one resident taken out and one agent put in write and reload as the model
/// has them, the leaver's #7 cleared and the newcomer's set.
#[test]
fn vanilla_save_settlement_residents_load_and_write_back() {
    let dir = data_dir();
    let path = common::evidence_dir().join("orig_gb_apr1811.save");
    if !dir.is_dir() || !path.is_file() {
        eprintln!("skipped: no install or no evidence save");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let esf = EsfFile::from_bytes(&std::fs::read(&path).expect("read")).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let m = &loaded.model;
    let stored = stored_residents(&esf);
    let mut count = 0;
    for (&region, (_, ids)) in &stored {
        let mut want: Vec<CharacterId> = ids.iter().map(|&c| CharacterId(c as i32)).collect();
        want.sort();
        assert_eq!(m.settlement_residents(RegionId(region)), want, "region {region}");
        count += ids.len();
    }
    assert_eq!(count, 5);
    let alentejo = m.world.regions.values().find(|r| r.key == "spa_alentejo").expect("spa_alentejo").id;
    assert!(m.settlement_residents(alentejo).contains(&CharacterId(865_679_480)));
    let res_of = stored_residence_of(&esf);
    assert_eq!(res_of[&865_679_480], stored[&alentejo.raw()].0);

    let human = &loaded.info.header.faction_key;
    let out = ntw_campaign::save::write_save(&esf, m, human, 1, None).expect("save");
    assert_eq!(stored_residents(&out), stored);
    assert_eq!(stored_residence_of(&out), res_of);

    // 865679480 leaves; a forceless agent of another region's owner moves into spa_alentejo.
    let mut m2 = m.clone();
    m2.world.characters.get_mut(&CharacterId(865_679_480)).expect("resident").garrisoned_in = None;
    let newcomer = m2
        .world
        .characters
        .values()
        .find(|c| c.garrisoned_in.is_none() && c.id != CharacterId(865_679_480) && m2.force_of(c.id).is_none())
        .map(|c| c.id)
        .expect("a forceless agent");
    m2.world.characters.get_mut(&newcomer).expect("agent").garrisoned_in = Some(alentejo);
    let out = ntw_campaign::save::write_save(&esf, &m2, human, 1, None).expect("save");
    let res_of = stored_residence_of(&out);
    assert_eq!((res_of[&865_679_480], res_of[&newcomer.raw()]), (0, stored[&alentejo.raw()].0));
    let back = ntw_campaign::read(&out.to_bytes().expect("bytes"), &db).expect("reload").model;
    for r in m2.world.regions.keys() {
        assert_eq!(back.settlement_residents(*r), m2.settlement_residents(*r), "{r:?}");
    }
    assert!(back.settlement_residents(alentejo).contains(&newcomer));
}

/// residence: a General hired into a garrison army of the human faction (`0x00A164C0`) takes its
/// command inside, and the old commander rides on with his unit, not a settlement resident (the hire
/// reaches no residence list): the save writes him into no #14 list and his `CHARACTER` #7 is 0, as
/// a garrisoned commander's is in the vanilla saves; the reload keeps the lists as the model has them.
#[test]
fn a_general_hired_into_a_garrison_writes_no_resident() {
    let dir = data_dir();
    let path = common::evidence_dir().join("orig_gb_apr1811.save");
    if !dir.is_dir() || !path.is_file() {
        eprintln!("skipped: no install or no evidence save");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let esf = EsfFile::from_bytes(&std::fs::read(&path).expect("read")).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let human = &loaded.info.header.faction_key;
    let mut m = loaded.model.clone();
    let fa = m.faction_by_key(human).expect("the human faction").id;
    let candidate = *m.world.faction_details[&fa].general_pool.0.first().expect("a General in the pool");
    // A garrison army of the human faction under a commander who is not a General.
    let (region, force, old) = m
        .world
        .regions
        .values()
        .filter_map(|r| {
            let f = m.world.forces.get(&r.garrison?)?;
            let c = f.commander?;
            (f.faction == fa && !f.is_navy && m.world.characters[&c].kind != ntw_sim::campaign::CharacterKind::General).then_some((r.id, f.id, c))
        })
        .next()
        .expect("a garrison army under a colonel");
    m.apply(ntw_sim::campaign::CampaignCommand::HireGeneral { character: candidate, into: Some(force) }).expect("hire");
    assert_eq!(m.world.forces[&force].commander, Some(candidate));
    assert_eq!(m.world.characters[&candidate].garrisoned_in, Some(region));
    assert_eq!(m.world.residence_of(old), None);
    let out = ntw_campaign::save::write_save(&esf, &m, human, 1, None).expect("save");
    let lists = stored_residents(&out);
    assert!(lists.values().all(|(_, ids)| !ids.contains(&(old.raw() as u32)) && !ids.contains(&(candidate.raw() as u32))));
    let res_of = stored_residence_of(&out);
    assert_eq!(res_of.get(&old.raw()), Some(&0), "the old commander's #7");
    assert_eq!(res_of.get(&candidate.raw()), Some(&0), "the new commander's #7");
    let back = ntw_campaign::read(&out.to_bytes().expect("bytes"), &db).expect("reload").model;
    for r in m.world.regions.keys() {
        assert_eq!(back.settlement_residents(*r), m.settlement_residents(*r), "{r:?}");
    }
    assert_eq!(back.world.residence_of(old), None);
}

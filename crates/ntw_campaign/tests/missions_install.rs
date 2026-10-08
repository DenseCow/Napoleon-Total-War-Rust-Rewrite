//! Script missions loaded into the campaign model with their targets re-linked
//! (S1_MISSIONS_UI.md, "Re-linking"), on the real eur startpos (read-only). Skips without an install.

use std::path::PathBuf;

use ntw_campaign::missions::{settlement_regions, write_manager, Mission, MissionKind, MissionManager, MissionObjectives, MissionRewards, GrantAgent};
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::details::MissionTarget;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// The `FACTION` record with this key (its plain value #1).
fn faction_mut<'a>(r: &'a mut EsfRecord, key: &str) -> Option<&'a mut EsfRecord> {
    if r.name == "FACTION" && r.values().nth(1).and_then(EsfNode::as_str) == Some(key) {
        return Some(r);
    }
    for c in &mut r.children {
        let found = match c {
            EsfNode::Record(b) => faction_mut(b, key),
            EsfNode::RecordArray(a) => a.items.iter_mut().flatten().find_map(|n| match n {
                EsfNode::Record(b) => faction_mut(b, key),
                _ => None,
            }),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

#[test]
fn missions_load_with_their_targets_relinked() {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let mut esf = EsfFile::open(dir.join("campaigns/eur_napoleon/startpos.esf")).expect("startpos");
    let base = ntw_campaign::read_esf(&esf, &db).expect("load");
    let w = &base.model.world;
    assert!(w.missions.is_empty(), "the startpos has no missions");
    // Real ids from the file: Vienna's settlement, Austria, a character, two regions.
    let austria = w.factions.values().find(|f| f.key == "austria").expect("austria").id;
    let vienna = w.regions.values().find(|r| r.key == "eur_austria").expect("eur_austria").id;
    let settlements = settlement_regions(&esf.root);
    assert_eq!(settlements.len(), w.regions.len(), "one settlement per region");
    let vienna_settlement = *settlements.iter().find(|(_, r)| **r == vienna).expect("vienna settlement").0;
    let character = *w.characters.keys().next().expect("a character");
    let bohemia = w.regions.values().find(|r| r.key == "eur_bohemia").expect("eur_bohemia").id;
    let mission = Mission {
        script_key: "eur_take_vienna".into(),
        elapsed: 3,
        objectives: MissionObjectives {
            kind: MissionKind::CaptureCity,
            settlement: vienna_settlement,
            faction: austria.raw() as u32,
            character: character.raw() as u32,
            region: vienna.raw(),
            regions: vec![bohemia.raw(), 0x0BAD_0008],
            fort: 0x1234_5678,
            ..Default::default()
        },
        rewards: MissionRewards {
            money: 2000,
            agents: vec![GrantAgent { agent: "spy".into(), region: bohemia.raw() }],
            ..Default::default()
        },
        ..Default::default()
    };
    let france = faction_mut(&mut esf.root, "france").expect("france");
    france.children.push(EsfNode::Record(Box::new(write_manager(&MissionManager { flag: false, missions: vec![mission] }))));
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load with a mission");
    let w = &loaded.model.world;
    let fr = w.factions.values().find(|f| f.key == "france").unwrap().id;
    let ms = &w.missions[&fr];
    assert_eq!(ms.len(), 1);
    let m = &ms[0];
    assert_eq!(m.script_key, "eur_take_vienna");
    assert_eq!(m.kind, 0);
    assert_eq!(m.settlement, Some(MissionTarget::Found(vienna)));
    assert_eq!(m.faction, Some(MissionTarget::Found(austria)));
    assert_eq!(m.character, Some(MissionTarget::Found(character)));
    assert_eq!(m.region, Some(MissionTarget::Found(vienna)));
    assert_eq!(m.regions, vec![MissionTarget::Found(bohemia), MissionTarget::Unresolved(0x0BAD_0008)]);
    assert_eq!(m.fort, 0x1234_5678);
    assert_eq!(m.reward_money, 2000);
    assert_eq!(m.reward_agents, vec![("spy".to_string(), MissionTarget::Found(bohemia))]);
    assert_eq!(m.reward_takeover, None);
}

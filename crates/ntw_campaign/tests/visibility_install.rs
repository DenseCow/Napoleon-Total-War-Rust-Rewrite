//! The campaign sight model against the install (slot 0-G, `CHARACTERS_FIDELITY.md` §10): the
//! saved shrouds and region sight shapes load, the quad tree codec round-trips every saved tree,
//! and the sight the model computes covers what the original saved as visible. Skipped without an
//! install.

use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::CampaignModel;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

fn startpos(campaign: &str) -> Option<(EsfFile, CampaignModel)> {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let bytes = GameFiles { vfs: &vfs }.read(&format!("campaigns/{campaign}/startpos.esf")).expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let l = ntw_campaign::read_esf(&esf, &db).expect("load");
    Some((esf, l.model))
}

fn trees<'a>(r: &'a EsfRecord, out: &mut Vec<&'a EsfRecord>) {
    if r.name == "QUAD_TREE_BIT_ARRAY" {
        out.push(r);
        return;
    }
    for c in &r.children {
        match c {
            EsfNode::Record(x) => trees(x, out),
            EsfNode::RecordArray(a) => {
                for it in &a.items {
                    for n in it {
                        if let EsfNode::Record(x) = n {
                            trees(x, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[test]
fn startpos_sight_loads_and_round_trips() {
    for campaign in ["eur_napoleon", "spa_napoleon"] {
        let Some((esf, m)) = startpos(campaign) else { return };
        let grid = m.world.sight_grid.expect("a sight grid");
        println!("{campaign}: grid {}x{} root {}, {} shrouds, {} region shapes", grid.cols, grid.rows, grid.root, m.world.shrouds.len(), m.world.region_sight.len());
        let mut all = Vec::new();
        trees(&esf.root, &mut all);
        assert!(!all.is_empty());
        for q in all {
            let set = ntw_campaign::shroud::decode(q).expect("decode");
            let back = ntw_campaign::shroud::encode(&set, q.get_u32(2).unwrap());
            assert_eq!(ntw_campaign::shroud::decode(&back).as_ref(), Some(&set));
        }
        for (f, s) in &m.world.shrouds {
            assert!(s.visible.cells().all(|(x, z)| s.explored.get(x, z)), "{f:?}: visible outside explored");
            let computed = m.compute_visible(*f).expect("grid");
            let saved = s.visible.len();
            let covered = s.visible.cells().filter(|&(x, z)| computed.get(x, z)).count();
            let extra = computed.cells().filter(|&(x, z)| !s.visible.get(x, z)).count();
            println!("  {}: saved {saved}, computed {}, covered {covered}, extra {extra}", m.world.factions[f].key, computed.len());
            // The start positions' shrouds are close to (not exactly) what the sources give: 97..100 %
            // covered, at most 0.4 % more (Britain, Portugal in the Peninsular campaign).
            assert!(covered * 100 >= saved * 96, "{f:?}: {covered} of {saved} covered");
            assert!(extra * 100 <= saved, "{f:?}: {extra} cells too many");
        }
    }
}

#[test]
fn shroud_follows_the_turns_and_hides_far_armies() {
    let Some((esf, mut m)) = startpos("eur_napoleon") else { return };
    drop(esf);
    let france = m.faction_by_key("france").unwrap().id;
    // Every character's saved radius (#17) is loaded; at the start it is his type's.
    let g = m.world.characters.values().find(|c| c.faction == france && c.kind.esf_name() == "General").unwrap().id;
    assert_eq!(m.sight_radius(g), 15.0);
    // Own commanders are seen; some foreign commander is not.
    let own = m.world.characters[&g].position;
    assert!(m.sees(france, (own.0.to_f32(), own.1.to_f32())));
    let hidden = m.world.forces.values().filter(|f| f.faction != france).filter_map(|f| m.world.characters.get(&f.commander?)).find(|c| !m.sees(france, (c.position.0.to_f32(), c.position.1.to_f32())));
    assert!(hidden.is_some(), "a foreign commander outside France's sight");
    // A faction without a shroud sees everything.
    let other = m.world.factions.keys().copied().find(|f| !m.world.shrouds.contains_key(f)).unwrap();
    assert!(m.sees(other, (0.0, 0.0)));
    // Turns: explored only grows, visible is what the sources give at the faction's turn start.
    let before = m.world.shrouds[&france].explored.clone();
    m.end_turn();
    let s = &m.world.shrouds[&france];
    assert!(before.cells().all(|(x, z)| s.explored.get(x, z)));
    assert!(s.visible.cells().all(|(x, z)| s.explored.get(x, z)));
}

/// The vanilla saves (read-only evidence copies; `NTW_EVIDENCE_DIR`), if present.
fn evidence_dir() -> PathBuf {
    std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(r"Documents\ntw-evidence\saves"))
}

/// The stealth test (`0x009D1010`) gives every saved hidden flag (`CHARACTER` #22) of the vanilla
/// saves: armies on light forest ground, or made only of units that can hide (an Austrian colonel
/// with one hussar unit on grassland), are hidden; the others are not. Rebels are left out (their
/// flags are not re-evaluated, INFERRED).
#[test]
fn stealth_rule_matches_the_saved_hidden_flags() {
    let dir = data_dir();
    let ev = evidence_dir();
    if !dir.is_dir() || !ev.is_dir() {
        eprintln!("skipped: no install or no evidence saves");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let files = GameFiles { vfs: &vfs };
    let mut checked = 0;
    let mut hidden_seen = 0;
    for name in ["auto_nr4_t4", "auto_after_c8", "orig_fr_t1", "orig_fr_may1811"] {
        let path = ev.join(format!("{name}.save"));
        if !path.is_file() {
            continue;
        }
        let esf = EsfFile::open(&path).expect("save");
        let mut l = ntw_campaign::read_esf(&esf, &db).expect("load");
        let map = ntw_formats::campaign_map::CampaignMap::load(&files, &l.info.map_key).expect("map");
        l.model.terrain = Some(ntw_sim::campaign::Terrain(std::sync::Arc::new(ntw_campaign::pathing::build_grid(&map))));
        let m = &l.model;
        for f in m.world.forces.values() {
            let Some(c) = f.commander.and_then(|c| m.world.characters.get(&c)) else { continue };
            if c.garrisoned_in.is_some() || m.world.factions[&c.faction].key.is_empty() {
                continue;
            }
            let saved = m.world.character_details[&c.id].hidden;
            assert_eq!(m.stealthy(c.id), saved, "{name}: {:?} of {}", c.id, m.world.factions[&c.faction].key);
            checked += 1;
            hidden_seen += usize::from(saved);
        }
    }
    println!("checked {checked} commanders, {hidden_seen} hidden");
    assert!(checked == 0 || hidden_seen >= 5);
}

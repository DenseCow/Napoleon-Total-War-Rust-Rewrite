//! Zones of control (`ntw_sim::campaign::zoc`) against the original's saved obstacles: the
//! eur_napoleon start position stores every commander's obstacle (bbox and cell range), which
//! the zone flood with the CONFIRMED limits must reproduce (`PATHFINDING_PORTS.md` §9.2, §10).
//! Skips (passes, printing a note) without an install.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;

use ntw_campaign::pathing::poly_map;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::polypath::Mover;
use ntw_sim::campaign::zoc::{obstacle_record, obstacle_record_with_zone, reach, Owner, ARMY_ZONE, GARRISON_BONUS, NAVY_ZONE};
use ntw_sim::campaign::CharacterId;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

#[test]
fn saved_obstacle_boxes_match_the_zone_limits() {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let files = GameFiles { vfs: &vfs };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let mut loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    // The stored zones as the load pass floods them (`ntw_sim::campaign::zones`).
    let terrain = ntw_sim::campaign::Terrain(std::sync::Arc::new(ntw_campaign::pathing::build_grid(&map)));
    let t0 = std::time::Instant::now();
    loaded.model.attach_terrain(terrain);
    let stored = loaded.model.stored_zones().expect("a polygon map").len();
    eprintln!("load pass: {stored} zones in {:.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
    let model_pm = loaded.model.terrain.as_ref().and_then(|t| t.0.poly.as_ref()).expect("a polygon map");
    let pf = map.pathfinding.as_ref().expect("pathfinding");
    let a = &pf.areas[0];
    let pm = poly_map(a, &a.grid.expand(), &a.grid.region_sets().expect("regions"));
    let w = &loaded.model.world;
    let navy_of: HashMap<i32, bool> = w.forces.values().filter_map(|f| Some((f.commander?.raw(), f.is_navy))).collect();
    let grid = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).expect("grid");
    let lists = grid.items[0].iter().filter_map(EsfNode::as_record).find(|r| r.name == "OBSTACLE_LISTS").expect("lists");
    let fx = |n: &EsfNode| n.as_int().expect("int") as f32 / 1_048_576.0;
    // (owner) -> (matching boxes with our limits, with the swapped limits, matching zone cell
    // ranges, total, matching core cell ranges, matching boxes and zone cell ranges from the
    // stored zones)
    let mut tally: HashMap<&str, [usize; 7]> = HashMap::new();
    for item in &lists.record_array("CHARACTER_OBSTACLE").expect("obstacles").items {
        let Some(o) = item.first().and_then(EsfNode::as_record) else { continue };
        let id = item.get(1).and_then(EsfNode::as_int).expect("id") as i32;
        let Some(ch) = w.characters.get(&CharacterId(id)) else { continue };
        let owner = match navy_of.get(&id) {
            Some(true) => Owner::Navy,
            Some(false) => Owner::Army,
            None => Owner::Agent,
        };
        let navy = owner == Owner::Navy;
        assert_eq!(o.children[6].as_int(), Some(9), "obstacle kind field");
        let pos = (ch.position.0.to_f32(), ch.position.1.to_f32());
        let garrisoned = ch.garrisoned_in.is_some();
        let saved = (fx(&o.children[7]), fx(&o.children[8]), fx(&o.children[9]), fx(&o.children[10]));
        let core_cells: Vec<i64> = (15..19).map(|k| o.children[k].as_int().expect("u16")).collect();
        let cells: Vec<i64> = (11..15).map(|k| o.children[k].as_int().expect("u16")).collect();
        let rec = obstacle_record(&pm, pos, owner, garrisoned);
        // The same box with the limits the other way round.
        let swapped = {
            let limit = if navy { ARMY_ZONE } else { NAVY_ZONE } + if garrisoned { GARRISON_BONUS } else { 0.0 };
            let z = reach(&pm, pos, if navy { Mover::Sea } else { Mover::Land }, limit);
            let mut b = (pos.0 - 1.0, pos.1 - 1.0, pos.0 + 1.0, pos.1 + 1.0);
            for v in z.iter().flat_map(|&p| pm.outline(p as usize).iter()) {
                b = (b.0.min(v.0), b.1.min(v.1), b.2.max(v.0), b.3.max(v.1));
            }
            ((b.0 / 2.0).floor() * 2.0 - 4.0, (b.1 / 2.0).floor() * 2.0 - 4.0, (b.2 / 2.0).ceil() * 2.0 + 4.0, (b.3 / 2.0).ceil() * 2.0 + 4.0)
        };
        let t = tally.entry(match owner { Owner::Army => "army", Owner::Navy => "navy", Owner::Agent => "agent" }).or_default();
        t[0] += usize::from(rec.bbox == saved);
        t[1] += usize::from(swapped == saved);
        t[2] += usize::from(cells == [i64::from(rec.zone_cells.0), i64::from(rec.zone_cells.1), i64::from(rec.zone_cells.2), i64::from(rec.zone_cells.3)]);
        t[3] += 1;
        t[4] += usize::from(core_cells == [i64::from(rec.core_cells.0), i64::from(rec.core_cells.1), i64::from(rec.core_cells.2), i64::from(rec.core_cells.3)]);
        if owner != Owner::Agent {
            let zone = loaded.model.stored_zone(CharacterId(id)).expect("a stored zone").to_vec();
            let s = obstacle_record_with_zone(model_pm, pos, owner, garrisoned, zone);
            t[5] += usize::from(s.bbox == saved);
            t[6] += usize::from(cells == [i64::from(s.zone_cells.0), i64::from(s.zone_cells.1), i64::from(s.zone_cells.2), i64::from(s.zone_cells.3)]);
        }
    }
    eprintln!("[box, swapped box, zone cells, total, core cells, stored box, stored zone cells]: {tally:?}");
    let (army, navy) = (tally["army"], tally["navy"]);
    assert!(army[3] > 30 && navy[3] > 10);
    // Our limits reproduce most saved boxes exactly; the swapped ones none.
    assert!(army[0] * 10 >= army[3] * 6 && navy[0] * 10 >= navy[3] * 9, "{army:?} {navy:?}");
    assert_eq!((army[1], navy[1]), (0, 0));
    // The cell ranges follow: the zone range wherever the box matches, the core range always
    // (it does not depend on the flood).
    assert!(army[2] >= army[0] && navy[2] >= navy[0], "{army:?} {navy:?}");
    for (owner, t) in &tally {
        assert_eq!(t[4], t[3], "{owner}: core cell ranges");
    }
    // The stored zones (the load pass: each flood cut by those before it, with the region gate)
    // give 64 of the 87 saved commander boxes against 60 for the bare floods (2026-10-19), so the
    // save writer (`grid_obstacle::NewObstacle::zone`) takes the stored zone (PATHFINDING.md §8).
    assert!(army[5] + navy[5] >= 64 && army[5] + navy[5] > army[0] + navy[0], "{army:?} {navy:?}");
    let agent = tally.get("agent").copied().unwrap_or_default();
    assert_eq!(agent[0], agent[3], "agents: boxes");
}

/// The faction lists the load pass walks (faction +0x768) hold their characters in `CHARACTER_ARRAY`
/// order (the loader `0x0087A190`, the writer `0x00892A80`): the importer keeps that order in
/// `World::character_order`, an original-written save lists characters out of id order, and a save
/// written by us lists each faction's characters again in the same order (`ntw_sim::campaign::zones`,
/// PATHFINDING.md §8). Skips without the install or the evidence save.
#[test]
fn vanilla_save_faction_lists_keep_the_file_order() {
    let dir = data_dir();
    let path = common::evidence_dir().join("orig_fr_may1811.save");
    if !dir.is_dir() || !path.is_file() {
        eprintln!("skipped: no install or no evidence save");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let bytes = std::fs::read(&path).expect("read");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let lists = |w: &ntw_sim::campaign::World| {
        let mut out: std::collections::BTreeMap<_, Vec<CharacterId>> = Default::default();
        for id in w.characters_in_list_order() {
            out.entry(w.characters[&id].faction).or_default().push(id);
        }
        out
    };
    let model = lists(&loaded.model.world);
    let world = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").expect("world");
    let mut factions: Vec<_> = world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    factions.extend(world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")));
    let (mut seen, mut descents) = (0, 0);
    for f in factions {
        let ids: Vec<CharacterId> = f
            .record_array("CHARACTER_ARRAY")
            .into_iter()
            .flat_map(|a| a.records())
            .filter(|c| c.name == "CHARACTER")
            .filter_map(|c| c.get_i32(2))
            .map(CharacterId)
            .filter(|id| loaded.model.world.characters.contains_key(id))
            .collect();
        if ids.is_empty() {
            continue;
        }
        let faction = loaded.model.world.characters[&ids[0]].faction;
        assert_eq!(model.get(&faction), Some(&ids), "{faction:?}");
        descents += ids.windows(2).filter(|w| w[0] > w[1]).count();
        seen += ids.len();
    }
    eprintln!("{seen} characters, {descents} descents");
    assert!(seen > 100, "{seen} characters");
    // Not id order: 57 places where a character follows a higher id (2026-10-20).
    assert!(descents > 10, "{descents} descents");
    // Our save writes each list back in order.
    let out = ntw_campaign::save::write_save(&esf, &loaded.model, &loaded.info.header.faction_key, 1, None).expect("save");
    let back = ntw_campaign::read(&out.to_bytes().expect("bytes"), &db).expect("reload").model.world;
    assert_eq!(lists(&back), model);
}

/// A vanilla save's risen armies (`World::region_risen_armies`, read from each `REGION`'s id list):
/// every id names a rebel land army of the save, as the region's set +0x220 does in the exe
/// (`0x00A45950`); a French May 1811 save holds some, and a save writes the model's lists back. Skips
/// without the evidence save.
#[test]
fn vanilla_save_risen_armies_are_rebel_armies() {
    let dir = data_dir();
    let path = common::evidence_dir().join("orig_fr_may1811.save");
    if !dir.is_dir() || !path.is_file() {
        eprintln!("skipped: no install or no evidence save");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let bytes = std::fs::read(&path).expect("read");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let w = &loaded.model.world;
    let risen: Vec<_> = w.region_risen_armies.values().flatten().collect();
    eprintln!("risen armies: {:?}", w.region_risen_armies);
    assert!(!risen.is_empty());
    for id in risen {
        let f = &w.forces[id];
        assert!(w.is_rebel_faction(f.faction) && !f.is_navy, "{id:?}");
    }
    // A save writes the model's lists back: one army taken off a list stays off after a reload.
    let mut m = loaded.model.clone();
    let (&region, _) = m.world.region_risen_armies.iter().next().expect("a list");
    let gone = m.world.region_risen_armies.get_mut(&region).expect("a list").remove(0);
    let out = ntw_campaign::save::write_save(&esf, &m, &loaded.info.header.faction_key, 1, None).expect("save");
    let back = ntw_campaign::read(&out.to_bytes().expect("bytes"), &db).expect("reload").model.world;
    let kept = |l: &std::collections::BTreeMap<_, Vec<_>>| l.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>();
    assert_eq!(kept(&back.region_risen_armies), kept(&m.world.region_risen_armies), "{gone:?}");
    assert_ne!(kept(&back.region_risen_armies), kept(&w.region_risen_armies));
}

//! Zones of control (`ntw_sim::campaign::zoc`) against the original's saved obstacles: the
//! eur_napoleon start position stores every commander's obstacle (bbox and cell range), which
//! the zone flood with the CONFIRMED limits must reproduce (`PATHFINDING_PORTS.md` §9.2, §10).
//! Skips (passes, printing a note) without an install.

use std::collections::HashMap;
use std::path::PathBuf;

use ntw_campaign::pathing::poly_map;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::polypath::Mover;
use ntw_sim::campaign::zoc::{obstacle_record, reach, Owner, ARMY_ZONE, GARRISON_BONUS, NAVY_ZONE};
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
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    let pf = map.pathfinding.as_ref().expect("pathfinding");
    let a = &pf.areas[0];
    let pm = poly_map(a, &a.grid.expand(), &a.grid.region_sets().expect("regions"));
    let w = &loaded.model.world;
    let navy_of: HashMap<i32, bool> = w.forces.values().filter_map(|f| Some((f.commander?.raw(), f.is_navy))).collect();
    let grid = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).expect("grid");
    let lists = grid.items[0].iter().filter_map(EsfNode::as_record).find(|r| r.name == "OBSTACLE_LISTS").expect("lists");
    let fx = |n: &EsfNode| n.as_int().expect("int") as f32 / 1_048_576.0;
    // (owner) -> (matching boxes with our limits, with the swapped limits, matching zone cell
    // ranges, total, matching core cell ranges)
    let mut tally: HashMap<&str, [usize; 5]> = HashMap::new();
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
    }
    eprintln!("[box, swapped box, zone cells, total, core cells]: {tally:?}");
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
    let agent = tally.get("agent").copied().unwrap_or_default();
    assert_eq!(agent[0], agent[3], "agents: boxes");
}

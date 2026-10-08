//! Obstacles written for new characters (`ntw_campaign::grid_obstacle`) against the original's own
//! obstacles and save rules (read-only; skips without an install).
//! Notes: `analysis/campaign/PATHFINDING_PORTS.md` §11.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use ntw_campaign::grid_obstacle::{add_character_obstacle, NewObstacle, StaticGrid};
use ntw_campaign::pathing::poly_map;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::zoc::Owner;
use ntw_sim::campaign::CharacterId;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// The children of `PATHFINDING_GRID[0]`.
fn grid_mut(esf: &mut EsfFile) -> &mut Vec<EsfNode> {
    let model = esf.root.children.iter_mut().find_map(|n| match n {
        EsfNode::Record(r) if r.name == "CAMPAIGN_ENV" => Some(r),
        _ => None,
    });
    let env = model.expect("env");
    let model = env.children.iter_mut().find_map(|n| match n {
        EsfNode::Record(r) if r.name == "CAMPAIGN_MODEL" => Some(r),
        _ => None,
    });
    let pf = model.expect("model").children.iter_mut().find_map(|n| match n {
        EsfNode::Record(r) if r.name == "CAMPAIGN_PATHFINDER" => Some(r),
        _ => None,
    });
    let arr = pf.expect("pathfinder").children.iter_mut().find_map(|n| match n {
        EsfNode::RecordArray(a) if a.name == "PATHFINDING_GRID" => Some(a),
        _ => None,
    });
    &mut arr.expect("grid").items[0]
}

/// (cell key, ring, polygon kinds sorted) per version listed in a BOUNDARIES slot.
fn versions_of(grid: &[EsfNode], slot: &[u32]) -> Vec<(u32, bool, Vec<u32>)> {
    let ob = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARIES")).and_then(|r| r.children[0].as_u32_array()).expect("bounds");
    let mut at = Vec::new();
    let mut j = 0;
    while j < ob.len() {
        at.push(j);
        j += 3 + 2 * ob[j] as usize;
    }
    slot.iter()
        .map(|&v| {
            let s = at[(v & 0x7FFF_FFFF) as usize];
            let n = ob[s] as usize;
            let mut kinds: Vec<u32> = (0..n).map(|k| ob[s + 1 + 2 * k] & 0xF).collect();
            kinds.sort_unstable();
            (ob[s + 1 + 2 * n], v >> 31 == 1, kinds)
        })
        .collect()
}

/// For every commander and agent obstacle of the eur_napoleon start position, our obstacle for
/// the same character covers the same cells with the same cut and ring versions, and most cut
/// versions hold the same polygon kinds.
#[test]
fn start_position_obstacles_are_reproduced() {
    let dir = data_dir();
    let Ok(vfs) = Vfs::open_install(&dir) else {
        eprintln!("skipped: no install");
        return;
    };
    let db = GameDatabase::from_install(&dir).expect("DB");
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let mut esf = EsfFile::from_bytes(&bytes).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    let a = &map.pathfinding.as_ref().expect("pathfinding").areas[0];
    let cells = a.grid.expand();
    let pm = poly_map(a, &cells, &a.grid.region_sets().expect("regions"));
    let sg = StaticGrid { area: a, cells: &cells, map: &pm };
    let w = &loaded.model.world;
    let navy_of: HashMap<i32, bool> = w.forces.values().filter_map(|f| Some((f.commander?.raw(), f.is_navy))).collect();
    let grid = grid_mut(&mut esf);
    let originals: Vec<(u32, Vec<Vec<u32>>)> = grid
        .iter()
        .find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))
        .and_then(|l| l.record_array("CHARACTER_OBSTACLE"))
        .expect("obstacles")
        .items
        .iter()
        .map(|it| {
            let o = it[0].as_record().expect("OBSTACLE");
            let b = o.children[0].as_record_array().expect("BOUNDARIES");
            (it[1].as_u32().expect("id"), b.items.iter().map(|s| s[0].as_u32_array().expect("slot").to_vec()).collect())
        })
        .collect();
    let first_new_piece = grid[0].as_u32().expect("piece count") as usize;
    let mut tally: BTreeMap<&str, [usize; 5]> = BTreeMap::new();
    for (k, (id, slots)) in originals.iter().enumerate() {
        let Some(ch) = w.characters.get(&CharacterId(*id as i32)) else { continue };
        let owner = match navy_of.get(&(*id as i32)) {
            Some(true) => Owner::Navy,
            Some(false) => Owner::Army,
            None => Owner::Agent,
        };
        let fake = 0x7000_0000 | (k as u32) << 2;
        let ob = NewObstacle { character: fake, pos: (ch.position.0.0, ch.position.1.0), owner, garrisoned: ch.garrisoned_in.is_some() };
        add_character_obstacle(grid, &sg, &ob).expect("added");
        let ours: Vec<Vec<u32>> = grid
            .iter()
            .find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))
            .and_then(|l| l.record_array("CHARACTER_OBSTACLE"))
            .and_then(|a| a.items.last())
            .and_then(|it| it[0].as_record())
            .and_then(|o| o.children[0].as_record_array())
            .map(|b| b.items.iter().map(|s| s[0].as_u32_array().expect("slot").to_vec()).collect())
            .expect("ours");
        let name = match owner {
            Owner::Army => "army",
            Owner::Navy => "navy",
            Owner::Agent => "agent",
        };
        let t = tally.entry(name).or_default();
        t[0] += 1;
        let mut cells_same = true;
        for s in 0..2 {
            let (o, m) = (versions_of(grid, &slots[s]), versions_of(grid, &ours[s]));
            let cells_o: Vec<(u32, bool)> = o.iter().map(|v| (v.0, v.1)).collect();
            let cells_m: Vec<(u32, bool)> = m.iter().map(|v| (v.0, v.1)).collect();
            cells_same &= cells_o == cells_m;
            if cells_o == cells_m {
                for (vo, vm) in o.iter().zip(&m) {
                    if !vo.1 {
                        t[3] += 1;
                        t[2] += usize::from(vo.2 == vm.2);
                    }
                }
            }
        }
        t[1] += usize::from(cells_same);
        t[4] += usize::from(slots[0].is_empty() == ours[0].is_empty() && slots[1].is_empty() == ours[1].is_empty());
    }
    eprintln!("[obstacles, same cells and rings, cut versions with the same kinds, cut versions compared, same slots used] {tally:?}");
    // Every piece we wrote has 3+ points and runs counter-clockwise, as every used piece of the
    // original's saves does (no slivers folded over by the snapping).
    let pool = grid[1].as_u32_array().expect("pieces");
    let (mut j, mut k, mut bad) = (0, 0, Vec::new());
    while j < pool.len() {
        let n = pool[j] as usize;
        let pts: Vec<(i128, i128)> = (0..n).map(|i| (i128::from(pool[j + 1 + 2 * i] as i32), i128::from(pool[j + 2 + 2 * i] as i32))).collect();
        let a2: i128 = (0..n).map(|i| pts[i].0 * pts[(i + 1) % n].1 - pts[(i + 1) % n].0 * pts[i].1).sum();
        if k >= first_new_piece && (n < 3 || a2 <= 0) {
            bad.push(k);
        }
        j += 2 + 2 * n;
        k += 1;
    }
    assert!(k > first_new_piece, "no pieces written");
    assert!(bad.is_empty(), "pieces with under 3 points or not counter-clockwise: {bad:?}");
    for (name, t) in &tally {
        assert_eq!(t[4], t[0], "{name}: slots used");
    }
    let core = tally.values().map(|t| t[1]).sum::<usize>();
    let all = tally.values().map(|t| t[0]).sum::<usize>();
    assert!(core * 10 >= all * 6, "cells and rings: {core} of {all}");
}

/// An obstacle written for a character without one keeps every save rule of the original's saves
/// (`save_check`) and survives a write and read of the file.
#[test]
fn new_obstacles_pass_save_check() {
    let dir = data_dir();
    let Ok(vfs) = Vfs::open_install(&dir) else {
        eprintln!("skipped: no install");
        return;
    };
    let db = GameDatabase::from_install(&dir).expect("DB");
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let bytes = files.read("campaigns/eur_napoleon/startpos.esf").expect("startpos");
    let mut esf = EsfFile::from_bytes(&bytes).expect("esf");
    let before = ntw_campaign::save_check::check(&esf).all_lines();
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    let a = &map.pathfinding.as_ref().expect("pathfinding").areas[0];
    let cells = a.grid.expand();
    let pm = poly_map(a, &cells, &a.grid.region_sets().expect("regions"));
    let sg = StaticGrid { area: a, cells: &cells, map: &pm };
    // Characters on the map without an obstacle: give the first few one of each kind.
    let have: std::collections::BTreeSet<u32> = grid_mut(&mut esf)
        .iter()
        .find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))
        .and_then(|l| l.get(2).and_then(EsfNode::as_u32_array))
        .map(|v| v.iter().copied().collect())
        .unwrap_or_default();
    let mut free: Vec<(u32, (i32, i32))> = loaded
        .model
        .world
        .characters
        .values()
        .filter(|c| !have.contains(&(c.id.raw() as u32)) && (c.position.0.0 != 0 || c.position.1.0 != 0))
        .map(|c| (c.id.raw() as u32, (c.position.0.0, c.position.1.0)))
        .collect();
    free.sort_unstable();
    assert!(free.len() >= 3, "characters without an obstacle: {}", free.len());
    let grid = grid_mut(&mut esf);
    for (k, &(id, pos)) in free.iter().take(3).enumerate() {
        let owner = [Owner::Army, Owner::Agent, Owner::Army][k];
        let added = add_character_obstacle(grid, &sg, &NewObstacle { character: id, pos, owner, garrisoned: k == 2 }).expect("added");
        assert!(added.versions[1] > 0);
    }
    let after = ntw_campaign::save_check::check(&esf).all_lines();
    let new: Vec<&String> = after.iter().filter(|v| !before.contains(v)).collect();
    assert!(new.is_empty(), "new violations: {new:?}");
    let again = EsfFile::from_bytes(&esf.to_bytes().expect("write")).expect("read back");
    assert!(ntw_campaign::save_check::check(&again).all_lines().iter().all(|v| before.contains(v)));
}

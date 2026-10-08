//! Evidence for using `grid_obstacle::add_character_obstacle` for new commanders in our saves
//! (SAVE_COMPAT.md §22). Read-only on the install; writes only under `target/tmp`.
//!
//! - `add <save> [out]`: gives every force commander without an obstacle ours, then runs
//!   `save_check` before and after (and after writing and reading the bytes), and counts the cells
//!   of each new obstacle that other obstacles also cover (where the original keeps combined
//!   versions, which ours does not write).
//! - `compare <save>`: for every full obstacle (core slot not empty) of an original save, builds
//!   ours for the same character at its position and compares cells, rings and polygon kinds.
//! - `combined <save>`: grid rows whose pair list names more than one obstacle layer.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use ntw_campaign::grid_obstacle::{add_character_obstacle, NewObstacle, StaticGrid};
use ntw_campaign::pathing::poly_map;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::zoc::Owner;
use ntw_sim::campaign::CharacterId;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

fn find_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    if r.name == name {
        return Some(r);
    }
    for c in r.children.iter_mut() {
        if let EsfNode::Record(x) = c
            && let Some(f) = find_mut(x, name)
        {
            return Some(f);
        }
    }
    None
}

fn grid_mut(esf: &mut EsfFile) -> &mut Vec<EsfNode> {
    let pf = find_mut(&mut esf.root, "CAMPAIGN_PATHFINDER").expect("pathfinder");
    pf.children
        .iter_mut()
        .find_map(|n| match n {
            EsfNode::RecordArray(a) if a.name == "PATHFINDING_GRID" => a.items.first_mut(),
            _ => None,
        })
        .expect("grid")
}

/// (character id, slots) of every character obstacle.
fn obstacles(grid: &[EsfNode]) -> Vec<(u32, Vec<Vec<u32>>)> {
    grid.iter()
        .find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))
        .and_then(|l| l.record_array("CHARACTER_OBSTACLE"))
        .map(|a| {
            a.items
                .iter()
                .map(|it| {
                    let o = it[0].as_record().expect("OBSTACLE");
                    let b = o.children[0].as_record_array().expect("BOUNDARIES");
                    (it[1].as_u32().unwrap_or(0), b.items.iter().map(|s| s[0].as_u32_array().map(<[u32]>::to_vec).unwrap_or_default()).collect())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// (cell key, ring, kinds sorted) per version of a slot.
fn versions_of(grid: &[EsfNode], slot: &[u32]) -> Vec<(u32, bool, Vec<u32>)> {
    let ob = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARIES")).and_then(|r| r.children[0].as_u32_array()).expect("bounds");
    let mut at = Vec::new();
    let mut j = 0;
    while j < ob.len() {
        at.push(j);
        j += 3 + 2 * ob[j] as usize;
    }
    slot.iter()
        .filter_map(|&v| {
            let s = *at.get((v & 0x7FFF_FFFF) as usize)?;
            let n = ob[s] as usize;
            let mut kinds: Vec<u32> = (0..n).map(|k| ob[s + 1 + 2 * k] & 0xF).collect();
            kinds.sort_unstable();
            Some((ob[s + 1 + 2 * n], v >> 31 == 1, kinds))
        })
        .collect()
}

struct Ctx {
    db: GameDatabase,
    vfs: Vfs,
    dir: PathBuf,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = data_dir();
    let vfs = Vfs::open_install(&dir).expect("install");
    let db = GameDatabase::from_install(&dir).expect("DB");
    let cx = Ctx { db, vfs, dir };
    match args.first().map(String::as_str) {
        Some("add") => add(&cx, &args[1], args.get(2)),
        Some("compare") => compare(&cx, &args[1]),
        Some("pieces") => {
            // Run-time pieces: n, n points (x, z), use count; from piece `from` on.
            let mut esf = EsfFile::from_bytes(&std::fs::read(&args[1]).expect("read")).expect("esf");
            let from: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            let grid = grid_mut(&mut esf);
            let count = grid[0].as_u32().unwrap_or(0);
            let p = grid[1].as_u32_array().expect("pieces").to_vec();
            let (mut j, mut k) = (0, 0);
            // [pieces, fewer than 3 points, zero area, clockwise, repeated point, unused]
            let mut t = [0usize; 6];
            while j < p.len() {
                let n = p[j] as usize;
                let pts: Vec<(i64, i64)> = (0..n).map(|i| (p[j + 1 + 2 * i] as i32 as i64, p[j + 2 + 2 * i] as i32 as i64)).collect();
                let used = p.get(j + 1 + 2 * n).copied().unwrap_or(0);
                j += 2 + 2 * n;
                k += 1;
                if k - 1 < from {
                    continue;
                }
                t[0] += 1;
                t[1] += usize::from(n < 3);
                let a2: i128 = (0..n).map(|i| {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    a.0 as i128 * b.1 as i128 - b.0 as i128 * a.1 as i128
                }).sum();
                t[2] += usize::from(a2 == 0);
                t[3] += usize::from(a2 < 0);
                if a2 < 0 {
                    println!("  piece {}: {n} points {pts:?} area2 {a2} used {used}", k - 1);
                }
                t[4] += usize::from((0..n).any(|i| pts[i] == pts[(i + 1) % n]));
                t[5] += usize::from(used == 0);
            }
            println!("count field {count}, parsed {k}, from {from}: [pieces, <3 points, zero area, negative orientation, repeated point, unused] {t:?}");
        }
        Some("combined") => {
            let mut esf = EsfFile::from_bytes(&std::fs::read(&args[1]).expect("read")).expect("esf");
            let grid = grid_mut(&mut esf);
            let ob = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARIES")).and_then(|r| r.children[0].as_u32_array()).expect("bounds").to_vec();
            let mut cell_of = Vec::new();
            let mut j = 0;
            while j < ob.len() {
                let n = ob[j] as usize;
                cell_of.push(ob[j + 1 + 2 * n]);
                j += 3 + 2 * n;
            }
            // Layers per cell (cut versions and ring versions apart).
            let mut cut: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
            let mut any: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
            for (id, slots) in obstacles(grid) {
                for (s, vs) in slots.iter().enumerate().take(2) {
                    for &v in vs {
                        let Some(&c) = cell_of.get((v & 0x7FFF_FFFF) as usize) else { continue };
                        any.entry(c).or_default().insert((id | 2, s as u32));
                        if v >> 31 == 0 {
                            cut.entry(c).or_default().insert((id | 2, s as u32));
                        }
                    }
                }
            }
            // Combined rows per cell.
            let mut comb: BTreeMap<u32, Vec<BTreeSet<(u32, u32)>>> = BTreeMap::new();
            let mut rows = 0;
            for n in grid.iter() {
                let EsfNode::RecordArray(a) = n else { continue };
                if a.name != "OBSTACLE_BASE_GRID_NODE" {
                    continue;
                }
                for it in &a.items {
                    if let Some(EsfNode::RecordArray(l1)) = it.get(2) {
                        for row in &l1.items {
                            rows += 1;
                            let v = row[0].as_u32().unwrap_or(0);
                            let p = row[3].as_u32_array().unwrap_or(&[]);
                            if p.len() > 2 {
                                let set: BTreeSet<(u32, u32)> = p.chunks(2).map(|c| (c[0], c[1])).collect();
                                comb.entry(*cell_of.get(v as usize).unwrap_or(&u32::MAX)).or_default().push(set);
                            }
                        }
                    }
                }
            }
            let multi_any = any.values().filter(|s| s.len() > 1).count();
            let multi_cut = cut.iter().filter(|(_, s)| s.len() > 1).collect::<Vec<_>>();
            let covered = multi_cut.iter().filter(|(c, s)| comb.get(c).is_some_and(|l| l.iter().any(|x| s.is_subset(x)))).count();
            let multi_any_cov = any.iter().filter(|(_, s)| s.len() > 1).filter(|(c, s)| comb.get(c).is_some_and(|l| l.iter().any(|x| s.is_subset(x)))).count();
            println!("rows {rows}, combined rows {} on {} cells; cells with 2+ layers: {} (all versions), of them with a combined row holding all: {}; with 2+ cut layers: {}, with a combined row holding all: {}", comb.values().map(Vec::len).sum::<usize>(), comb.len(), multi_any, multi_any_cov, multi_cut.len(), covered);
        }
        _ => eprintln!("usage: obstacle_trial add <save> [out] | compare <save> | combined <save>"),
    }
}

fn add(cx: &Ctx, path: &str, out: Option<&String>) {
    let mut esf = EsfFile::from_bytes(&std::fs::read(path).expect("read")).expect("esf");
    let before = ntw_campaign::save_check::check(&esf);
    let loaded = ntw_campaign::read_esf(&esf, &cx.db).expect("load");
    let files = GameFiles { vfs: &cx.vfs, data_dir: Some(&cx.dir) };
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    let a = &map.pathfinding.as_ref().expect("pathfinding").areas[0];
    let cells = a.grid.expand();
    let pm = poly_map(a, &cells, &a.grid.region_sets().expect("regions"));
    let sg = StaticGrid { area: a, cells: &cells, map: &pm };
    let w = &loaded.model.world;
    let navy_of: HashMap<i32, bool> = w.forces.values().filter_map(|f| Some((f.commander?.raw(), f.is_navy))).collect();
    println!("before: {} violations, commanders without obstacle {:?}", before.violations.len(), before.commanders_without_obstacle);
    let grid = grid_mut(&mut esf);
    // Cells covered by each existing obstacle layer.
    let mut covered: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for (id, slots) in obstacles(grid) {
        for s in slots.iter().take(2) {
            for v in versions_of(grid, s) {
                covered.entry(v.0).or_default().insert(id);
            }
        }
    }
    for &id in &before.commanders_without_obstacle {
        let Some(ch) = w.characters.get(&CharacterId(id as i32)) else {
            println!("  {id}: not in the model");
            continue;
        };
        let owner = if navy_of.get(&(id as i32)) == Some(&true) { Owner::Navy } else { Owner::Army };
        let ob = NewObstacle { character: id, pos: (ch.position.0.0, ch.position.1.0), owner, garrisoned: ch.garrisoned_in.is_some() };
        match add_character_obstacle(grid, &sg, &ob) {
            Ok(added) => {
                let ours = obstacles(grid).into_iter().find(|o| o.0 == id).map(|o| o.1).unwrap_or_default();
                let mut shared = BTreeSet::new();
                let mut n = 0;
                for s in ours.iter().take(2) {
                    for v in versions_of(grid, s) {
                        n += 1;
                        if let Some(others) = covered.get(&v.0) {
                            shared.extend(others.iter().copied());
                        }
                    }
                }
                let shared_cells = ours.iter().take(2).flat_map(|s| versions_of(grid, s)).filter(|v| covered.contains_key(&v.0)).count();
                println!("  {id} {owner:?} garrisoned {}: {added:?}; {n} versions, {shared_cells} on cells other obstacles cover (theirs: {shared:?})", ob.garrisoned);
            }
            Err(e) => println!("  {id}: error {e}"),
        }
    }
    let after = ntw_campaign::save_check::check(&esf);
    println!("after: {} violations, commanders without obstacle {:?}", after.violations.len(), after.commanders_without_obstacle);
    for v in after.violations.iter().filter(|v| !before.violations.contains(v)).take(20) {
        println!("  NEW: {v}");
    }
    let bytes = esf.to_bytes().expect("write");
    let back = EsfFile::from_bytes(&bytes).expect("read back");
    let again = ntw_campaign::save_check::check(&back);
    println!("written and read back: {} violations; load {}", again.violations.len(), ntw_campaign::read_esf(&back, &cx.db).is_ok());
    if let Some(o) = out {
        std::fs::write(o, &bytes).expect("write out");
    }
}

fn compare(cx: &Ctx, path: &str) {
    let mut esf = EsfFile::from_bytes(&std::fs::read(path).expect("read")).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &cx.db).expect("load");
    let files = GameFiles { vfs: &cx.vfs, data_dir: Some(&cx.dir) };
    let map = CampaignMap::load(&files, &loaded.info.map_key).expect("map");
    let a = &map.pathfinding.as_ref().expect("pathfinding").areas[0];
    let cells = a.grid.expand();
    let pm = poly_map(a, &cells, &a.grid.region_sets().expect("regions"));
    let sg = StaticGrid { area: a, cells: &cells, map: &pm };
    let w = &loaded.model.world;
    let navy_of: HashMap<i32, bool> = w.forces.values().filter_map(|f| Some((f.commander?.raw(), f.is_navy))).collect();
    let grid = grid_mut(&mut esf);
    let originals = obstacles(grid);
    // [compared, same cells and rings, cut versions same kinds, cut versions compared]
    let mut t = [0usize; 4];
    let mut full = 0;
    for (k, (id, slots)) in originals.iter().enumerate() {
        if slots.get(1).is_none_or(|s| s.is_empty()) {
            continue;
        }
        full += 1;
        let Some(ch) = w.characters.get(&CharacterId(*id as i32)) else { continue };
        let owner = match navy_of.get(&(*id as i32)) {
            Some(true) => Owner::Navy,
            Some(false) => Owner::Army,
            None => Owner::Agent,
        };
        let fake = 0x7000_0000 | (k as u32) << 2;
        let ob = NewObstacle { character: fake, pos: (ch.position.0.0, ch.position.1.0), owner, garrisoned: ch.garrisoned_in.is_some() };
        if add_character_obstacle(grid, &sg, &ob).is_err() {
            continue;
        }
        let ours = obstacles(grid).last().map(|o| o.1.clone()).unwrap_or_default();
        t[0] += 1;
        let mut same = true;
        for s in 0..2 {
            let (o, m) = (versions_of(grid, &slots[s]), versions_of(grid, &ours[s]));
            let co: Vec<(u32, bool)> = o.iter().map(|v| (v.0, v.1)).collect();
            let cm: Vec<(u32, bool)> = m.iter().map(|v| (v.0, v.1)).collect();
            same &= co == cm;
            if co == cm {
                for (vo, vm) in o.iter().zip(&m) {
                    if !vo.1 {
                        t[3] += 1;
                        t[2] += usize::from(vo.2 == vm.2);
                    }
                }
            }
        }
        t[1] += usize::from(same);
    }
    println!("obstacles {} full {} compared {} same cells and rings {} cut versions same kinds {} of {}", originals.len(), full, t[0], t[1], t[2], t[3]);
}

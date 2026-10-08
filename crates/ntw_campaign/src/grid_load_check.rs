//! A replay of the original game's pathfinder-grid loader on a save's `PATHFINDING_GRID[0]`:
//! it follows the same lists and indexes the exe follows and reports where the original would
//! read a bad pointer (SAVE_COMPAT.md §29). Every step is CONFIRMED in the decompiled loader
//! `0x00AF89C0` and the functions it calls, unless tagged otherwise.
//!
//! What the loader does, in order:
//! 1. The run-time pieces (`#0` count, `#1` data: `n`, `n` points, use count).
//! 2. `OBSTACLE_BOUNDARY_MANAGER/OBSTACLE_BOUNDARY[]` into a hash map keyed by the pair list's
//!    value (`0x00AFB3C0`; an equal list is found, not inserted twice).
//! 3. `OBSTACLE_BOUNDARIES` #0: the cell versions, a linked list in file order (`0x00AEE750`:
//!    `n`, `n` x (flags, link), cell key, a byte as u32). Every version's grid-node pointer
//!    (+0x18) starts as the node list's begin, which is still the list's own sentinel
//!    (`PUSH [grid+0x90]` at `0x00AF9534`, before any node exists).
//! 4. `OBSTACLE_BASE_GRID_NODE[]` (`0x00AEE2E0`): per list-1 row the version by position (an
//!    every-32nd index of the version list plus a walk), the pair list looked up in the manager
//!    (`0x00AECFF0` → `0x00B53800`; missing = `INC [8+0x14]`, the 0x00AECFE6 crash) and an entry
//!    in a map keyed by the pair list (`0x00B46CC0`: find, then insert; the FIRST row with a list
//!    wins). List 2's rows: the manager lookup only; they become the node's linked list. Then,
//!    with the node appended, `0x00B5BF60` gives the version of every MAP ENTRY the node as its
//!    grid-node pointer (`0x00B5BFE0`). A row whose pair list another row of the node already
//!    had is not in the map: its version keeps the sentinel.
//! 5. `#5`: (cell key, node index) into the cell hash map, the node by position.
//! 6. `OBSTACLE_LISTS` (`0x00AFA4F0`): each `OBSTACLE`'s `BOUNDARIES` slot entries resolve by
//!    position (`entry & 0x7fffffff`, the high bit a flag; `0x00AED950`), each true
//!    `MANAGED_OBSTACLE_BOUNDARY` list is looked up in the manager (same crash when missing).
//! 7. The last step, `0x00B78C60`: for every obstacle, the versions of its `BOUNDARIES` slot
//!    `#2` (+0x68) give their grid nodes; each node's list 2 is walked (`0x00B07780` →
//!    `0x00B07550`, from node + 8 + 0x2c). A version still on the sentinel makes it walk from
//!    sentinel + 8 + 0x2c = the obstacle manager's hash-map load factor 1.0f = 0x3f800000:
//!    `MOV ECX,[ECX]` at `0x00B1D3E0` faults with ECX = 0x3f800008 (every NR-5..NR-10 crash
//!    dump: EAX the static-init epoch, EBX/EBP the loop counter and count of `0x00B78C60`).
//!    The walk itself: a row with one pair looks its owner's obstacle up and compares the pair's
//!    slot with the obstacle's `#1`; a row with another pair count ends the walk.
//!
//! The report separates **faults** (the loader reads a bad pointer: the save crashes) from
//! **rule breaks** (states the original never writes, CONFIRMED on every vanilla save kept:
//! a version without a row, a version in two nodes, two rows of one node with the same pair
//! list, a piece use count not equal to its links).

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::EsfNode;

/// The result of [`check`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Places where the original's loader would follow a bad pointer, one line each.
    pub faults: Vec<String>,
    /// States the original never writes (CONFIRMED on the vanilla saves), one line each.
    pub rule_breaks: Vec<String>,
    /// Counts: versions, grid rows (list 1), nodes, obstacles.
    pub counts: [usize; 4],
}

/// One cell version of `OBSTACLE_BOUNDARIES` #0 (CONFIRMED layout, PATHFINDING_PORTS.md §11.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// (flags, link) per polygon of the cell.
    pub links: Vec<(u32, u32)>,
    /// The cell key (col | row << 16).
    pub cell: u32,
    /// The trailing byte, stored as a u32 (0 in every vanilla save).
    pub byte: u32,
}

/// A link into the run-time piece pool: `region << 22 | 0x200000 | piece index` (CONFIRMED).
pub const PIECE_LINK: u32 = 0x20_0000;
/// The piece index bits of a piece link.
pub const PIECE_INDEX: u32 = 0x1f_ffff;

/// Parses the versions array; `None` when it does not have the layout.
pub fn parse_versions(a: &[u32]) -> Option<Vec<Version>> {
    let mut out = Vec::new();
    let mut j = 0;
    while j < a.len() {
        let n = a[j] as usize;
        let end = j + 3 + 2 * n;
        if end > a.len() {
            return None;
        }
        out.push(Version { links: (0..n).map(|i| (a[j + 1 + 2 * i], a[j + 2 + 2 * i])).collect(), cell: a[j + 1 + 2 * n], byte: a[j + 2 + 2 * n] });
        j = end;
    }
    Some(out)
}

/// The versions array for `versions` (the inverse of [`parse_versions`]).
pub fn write_versions(versions: &[Version]) -> Vec<u32> {
    let mut out = Vec::with_capacity(versions.iter().map(|v| 3 + 2 * v.links.len()).sum());
    for v in versions {
        out.push(v.links.len() as u32);
        for &(f, l) in &v.links {
            out.push(f);
            out.push(l);
        }
        out.push(v.cell);
        out.push(v.byte);
    }
    out
}

/// The position of each piece's use count in the piece pool array (`#1`: `n`, `n` points, use
/// count), in piece order; `None` when the array does not have the layout.
pub fn piece_use_positions(a: &[u32]) -> Option<Vec<usize>> {
    let mut out = Vec::new();
    let mut j = 0;
    while j < a.len() {
        let n = a[j] as usize;
        let pos = j + 1 + 2 * n;
        if pos >= a.len() {
            return None;
        }
        out.push(pos);
        j = pos + 1;
    }
    Some(out)
}

/// A list-1 row of a grid node.
struct Row {
    version: u32,
    pairs: Vec<u32>,
}

struct Node {
    key: u32,
    list1: Vec<Row>,
    list2: Vec<Vec<u32>>,
}

struct Obstacle {
    kind: String,
    owner: u32,
    /// `BOUNDARIES` slot entries.
    slots: Vec<Vec<u32>>,
    /// #1..#4.
    mode: [u32; 4],
    /// The true `MANAGED_OBSTACLE_BOUNDARY` lists.
    managed: Vec<Vec<u32>>,
}

fn pairs_of(row: &[EsfNode]) -> Vec<u32> {
    row.iter()
        .find_map(|n| match n {
            EsfNode::U32Array(v) => Some(v.clone()),
            EsfNode::Record(r) => r.children.iter().find_map(|c| c.as_u32_array().map(<[u32]>::to_vec)),
            _ => None,
        })
        .unwrap_or_default()
}

fn nodes_of(grid: &[EsfNode]) -> Vec<Node> {
    let Some(a) = grid.iter().find_map(|n| n.as_record_array().filter(|a| a.name == "OBSTACLE_BASE_GRID_NODE")) else { return Vec::new() };
    a.items
        .iter()
        .map(|it| {
            let lists: Vec<&ntw_formats::esf::EsfRecordArray> = it.iter().filter_map(EsfNode::as_record_array).collect();
            Node {
                key: it.first().and_then(EsfNode::as_u32).unwrap_or(0),
                list1: lists.first().map(|l| l.items.iter().map(|r| Row { version: r.first().and_then(EsfNode::as_u32).unwrap_or(u32::MAX), pairs: pairs_of(r) }).collect()).unwrap_or_default(),
                list2: lists.get(1).map(|l| l.items.iter().map(|r| pairs_of(r)).collect()).unwrap_or_default(),
            }
        })
        .collect()
}

fn obstacles_of(grid: &[EsfNode]) -> Vec<Obstacle> {
    let mut out = Vec::new();
    let Some(ol) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS")) else { return out };
    for c in &ol.children {
        let EsfNode::RecordArray(a) = c else { continue };
        for it in &a.items {
            let Some(o) = it.iter().find_map(EsfNode::as_record) else { continue };
            let owner = it.iter().find_map(EsfNode::as_u32).unwrap_or(0);
            let mut mode = [0u32; 4];
            for (i, m) in mode.iter_mut().enumerate() {
                *m = o.get(i + 1).and_then(EsfNode::as_u32).unwrap_or(0);
            }
            out.push(Obstacle {
                kind: a.name.clone(),
                owner,
                slots: o.record_array("BOUNDARIES").map(|b| b.items.iter().map(|s| s.iter().find_map(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default()).collect()).unwrap_or_default(),
                mode,
                managed: o
                    .record_array("MANAGED_OBSTACLE_BOUNDARY")
                    .map(|m| m.items.iter().filter(|s| s.first().and_then(EsfNode::as_bool) == Some(true)).map(|s| s.iter().find_map(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default()).collect())
                    .unwrap_or_default(),
            });
        }
    }
    out
}

/// Replays the loader on one `PATHFINDING_GRID` item (the children of `PATHFINDING_GRID[0]`).
pub fn check(grid: &[EsfNode]) -> Report {
    let mut r = Report::default();
    let fault = &mut r.faults;
    let rules = &mut r.rule_breaks;
    // 1. Pieces.
    let pieces = grid.get(1).and_then(EsfNode::as_u32_array).unwrap_or_default();
    let use_pos = piece_use_positions(pieces).unwrap_or_default();
    // 2. The manager.
    let mut manager: BTreeSet<Vec<u32>> = BTreeSet::new();
    if let Some(m) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARY_MANAGER")) {
        for c in &m.children {
            let EsfNode::RecordArray(a) = c else { continue };
            for it in &a.items {
                manager.insert(it.iter().find_map(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default());
            }
        }
    }
    // 3. The versions.
    let versions = grid
        .iter()
        .find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARIES"))
        .and_then(|r| r.get(0))
        .and_then(EsfNode::as_u32_array)
        .and_then(parse_versions)
        .unwrap_or_default();
    let mut links = vec![0u32; use_pos.len()];
    for v in &versions {
        for &(_, l) in &v.links {
            if l & PIECE_LINK != 0 {
                match links.get_mut((l & PIECE_INDEX) as usize) {
                    Some(c) => *c += 1,
                    None => rules.push(format!("a cell version links piece {} of {} pieces", l & PIECE_INDEX, use_pos.len())),
                }
            }
        }
    }
    let wrong_use: Vec<usize> = (0..use_pos.len()).filter(|&i| pieces[use_pos[i]] != links[i]).collect();
    if !wrong_use.is_empty() {
        rules.push(format!("{} pieces whose use count is not the number of cell-version links to them (first: piece {} count {} links {})", wrong_use.len(), wrong_use[0], pieces[use_pos[wrong_use[0]]], links[wrong_use[0]]));
    }
    // 4. The grid nodes.
    let nodes = nodes_of(grid);
    let mut node_of: Vec<Option<usize>> = vec![None; versions.len()];
    let mut rows_of: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut rows = 0;
    let (mut missing, mut dup_rows, mut out_of_range) = (Vec::new(), Vec::new(), Vec::new());
    for (ni, n) in nodes.iter().enumerate() {
        let mut map: BTreeMap<&[u32], usize> = BTreeMap::new();
        for (ri, row) in n.list1.iter().enumerate() {
            rows += 1;
            if row.version as usize >= versions.len() {
                out_of_range.push(format!("node {ni} (key {}) list-1 row {ri} names version {} of {}", n.key, row.version, versions.len()));
                continue;
            }
            rows_of.entry(row.version).or_default().push(ni);
            if !manager.contains(&row.pairs) {
                missing.push(format!("node {ni} (key {}) list-1 row {ri}: pair list {:?}", n.key, row.pairs));
            }
            match map.get(row.pairs.as_slice()) {
                Some(&first) => dup_rows.push(format!("node {ni} (key {}): row {ri} (version {}) has the pair list of row {first} (version {}): {:?}", n.key, row.version, n.list1[first].version, row.pairs)),
                None => {
                    map.insert(&row.pairs, ri);
                    node_of[row.version as usize] = Some(ni);
                }
            }
        }
        for (ri, pairs) in n.list2.iter().enumerate() {
            if !manager.contains(pairs) {
                missing.push(format!("node {ni} (key {}) list-2 row {ri}: pair list {pairs:?}", n.key));
            }
        }
    }
    if !out_of_range.is_empty() {
        fault.push(format!("{} grid rows name a version beyond the version list (the loader's index walk reads past it, 0x00AEE2E0); first: {}", out_of_range.len(), out_of_range[0]));
    }
    if !missing.is_empty() {
        fault.push(format!("{} grid-row pair lists are not in the boundary manager (0x00B53800 finds nothing, 0x00AECFE6 faults); first: {}", missing.len(), missing[0]));
    }
    if !dup_rows.is_empty() {
        rules.push(format!("{} list-1 rows repeat another row's pair list in the same node (their versions get no grid node: 0x00B46CC0 keeps the first); first: {}", dup_rows.len(), dup_rows[0]));
    }
    let dead = node_of.iter().enumerate().filter(|(v, _)| !rows_of.contains_key(&(*v as u32))).count();
    if dead > 0 {
        rules.push(format!("{dead} of {} cell versions have no grid row", versions.len()));
    }
    let twice: Vec<(&u32, &Vec<usize>)> = rows_of.iter().filter(|(_, n)| n.len() > 1).collect();
    if !twice.is_empty() {
        rules.push(format!("{} cell versions have rows in more than one node; first: version {} in nodes {:?}", twice.len(), twice[0].0, twice[0].1));
    }
    // 5. The cell map.
    if let Some(pos) = grid.iter().position(|n| matches!(n, EsfNode::RecordArray(a) if a.name == "OBSTACLE_BASE_GRID_NODE"))
        && let Some(map) = grid.get(pos + 1).and_then(EsfNode::as_u32_array)
    {
        let bad: Vec<&[u32]> = map.chunks(2).filter(|p| p.len() == 2 && p[1] as usize >= nodes.len()).collect();
        if !bad.is_empty() {
            fault.push(format!("{} cell-map entries name a node beyond the {} nodes (the index walk reads past the list); first: cell {:#x} node {}", bad.len(), nodes.len(), bad[0][0], bad[0][1]));
        }
    }
    // 6. The obstacles.
    let obstacles = obstacles_of(grid);
    let owners: BTreeSet<u32> = obstacles.iter().map(|o| o.owner).collect();
    let (mut bad_entries, mut bad_managed) = (Vec::new(), Vec::new());
    for o in &obstacles {
        for (k, s) in o.slots.iter().enumerate() {
            for (i, &e) in s.iter().enumerate() {
                if (e & 0x7fff_ffff) as usize >= versions.len() {
                    bad_entries.push(format!("{} {} slot {k} entry {i}: {e:#x} of {} versions", o.kind, o.owner, versions.len()));
                }
            }
        }
        for (k, m) in o.managed.iter().enumerate() {
            if !manager.contains(m) {
                bad_managed.push(format!("{} {} managed slot {k}: {m:?}", o.kind, o.owner));
            }
        }
    }
    if !bad_entries.is_empty() {
        fault.push(format!("{} obstacle boundary entries name a version beyond the list (0x00AED950 reads past it); first: {}", bad_entries.len(), bad_entries[0]));
    }
    if !bad_managed.is_empty() {
        fault.push(format!("{} managed obstacle lists are not in the boundary manager (0x00AECFE6 faults); first: {}", bad_managed.len(), bad_managed[0]));
    }
    // 7. The re-registration from the grid nodes.
    let (mut sentinel, mut no_owner) = (Vec::new(), Vec::new());
    for o in &obstacles {
        let s = o.mode[1] as usize;
        let Some(entries) = o.slots.get(s) else { continue };
        for (i, &e) in entries.iter().enumerate() {
            let v = (e & 0x7fff_ffff) as usize;
            let Some(slot) = node_of.get(v) else { continue };
            let Some(ni) = slot else {
                let why = match rows_of.get(&(v as u32)) {
                    Some(ns) => format!("its row in node {} (key {}) repeats another row's pair list", ns[0], nodes[ns[0]].key),
                    None => "no grid row names it".to_string(),
                };
                sentinel.push(format!("{} {} slot {s} entry {i} = version {v}: {why}", o.kind, o.owner));
                continue;
            };
            for (ri, pairs) in nodes[*ni].list2.iter().enumerate() {
                if pairs.len() != 2 {
                    break;
                }
                // Character obstacles carry tag 2 (CONFIRMED); barrier and fort obstacles (tags
                // 0 / 1, keyed by raw values, e.g. the tutorial start position) are not checked.
                if pairs[0] & 3 == 2 && !owners.contains(&(pairs[0] & !3)) {
                    no_owner.push(format!("node {ni} (key {}) list-2 row {ri}: owner {:#x} has no obstacle", nodes[*ni].key, pairs[0]));
                }
            }
        }
    }
    if !sentinel.is_empty() {
        fault.push(format!(
            "{} obstacle boundary versions have no grid node, so 0x00B78C60 walks the version list's sentinel (+8+0x2c = the load factor 1.0f): the 0x00B1D3E0 crash; first: {}",
            sentinel.len(),
            sentinel[0]
        ));
    }
    if !no_owner.is_empty() {
        fault.push(format!("{} walked list-2 rows name an owner without an obstacle (0x00B07550 reads a missing obstacle); first: {}", no_owner.len(), no_owner[0]));
    }
    r.counts = [versions.len(), rows, nodes.len(), obstacles.len()];
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::esf::{EsfRecord, EsfRecordArray};

    fn rec(name: &str, children: Vec<EsfNode>) -> EsfNode {
        EsfNode::Record(Box::new(EsfRecord { name: name.into(), version: 1, children }))
    }
    fn arr(name: &str, items: Vec<Vec<EsfNode>>) -> EsfNode {
        EsfNode::RecordArray(Box::new(EsfRecordArray { name: name.into(), version: 1, items }))
    }

    /// A grid with one piece, the manager lists given, the versions given (one link each), the
    /// nodes given as (key, list-1 rows (version, pairs)) with list 2 the same lists reversed,
    /// and one character obstacle 800 whose slot 0 lists `listed`.
    /// (node key, list-1 rows (version, pairs)).
    type Nodes = Vec<(u32, Vec<(u32, Vec<u32>)>)>;

    fn grid(manager: Vec<Vec<u32>>, versions: usize, nodes: Nodes, listed: Vec<u32>) -> Vec<EsfNode> {
        let vs: Vec<Version> = (0..versions).map(|i| Version { links: vec![(9, PIECE_LINK)], cell: i as u32, byte: 0 }).collect();
        let node_items: Vec<Vec<EsfNode>> = nodes
            .iter()
            .map(|(k, rows)| {
                vec![
                    EsfNode::U32(*k),
                    EsfNode::U32(1),
                    arr("MANAGED_OBSTACLE_BOUNDARY", rows.iter().map(|(v, p)| vec![EsfNode::U32(*v), EsfNode::U32(0), EsfNode::Bool(true), EsfNode::U32Array(p.clone())]).collect()),
                    arr("MANAGED_OBSTACLE_BOUNDARY", rows.iter().rev().map(|(_, p)| vec![EsfNode::Bool(true), EsfNode::U32Array(p.clone())]).collect()),
                ]
            })
            .collect();
        let cell_map: Vec<u32> = nodes.iter().enumerate().flat_map(|(i, (k, _))| [*k, i as u32]).collect();
        let obstacle = rec(
            "OBSTACLE",
            vec![
                arr("BOUNDARIES", vec![vec![EsfNode::U32Array(listed)], vec![EsfNode::U32Array(vec![])]]),
                EsfNode::U32(1),
                EsfNode::U32(0),
                EsfNode::U32(0),
                EsfNode::U32(1),
                arr("MANAGED_OBSTACLE_BOUNDARY", vec![vec![EsfNode::Bool(true), EsfNode::U32Array(vec![802, 0])], vec![EsfNode::Bool(false)]]),
                EsfNode::U32(9),
            ],
        );
        vec![
            EsfNode::U32(1),
            EsfNode::U32Array(vec![3, 0, 0, 1, 0, 0, 1, versions as u32]),
            rec("OBSTACLE_BOUNDARY_MANAGER", vec![arr("OBSTACLE_BOUNDARY", manager.into_iter().map(|m| vec![EsfNode::U32Array(m)]).collect())]),
            rec("OBSTACLE_BOUNDARIES", vec![EsfNode::U32Array(write_versions(&vs))]),
            arr("OBSTACLE_BASE_GRID_NODE", node_items),
            EsfNode::U32Array(cell_map),
            rec("OBSTACLE_LISTS", vec![EsfNode::U32Array(vec![]), arr("BARRIER_OBSTACLE", vec![]), EsfNode::U32Array(vec![800]), arr("CHARACTER_OBSTACLE", vec![vec![obstacle, EsfNode::U32(800)]])]),
        ]
    }

    #[test]
    fn a_consistent_grid_passes() {
        let g = grid(vec![vec![802, 0], vec![810, 0], vec![802, 0, 810, 0]], 3, vec![(7, vec![(0, vec![802, 0]), (1, vec![810, 0]), (2, vec![802, 0, 810, 0])])], vec![0, 2]);
        let r = check(&g);
        assert!(r.faults.is_empty() && r.rule_breaks.is_empty(), "{r:?}");
        assert_eq!(r.counts, [3, 3, 1, 1]);
    }

    #[test]
    fn a_repeated_pair_list_in_a_node_loses_its_grid_node() {
        // Our old writer's state after obstacle 810 left: its pairs filtered out of the combined
        // version, which now repeats row 0's list. Version 2 is listed by obstacle 800: the walk
        // starts from the sentinel.
        let g = grid(vec![vec![802, 0]], 3, vec![(7, vec![(0, vec![802, 0]), (2, vec![802, 0])])], vec![0, 2]);
        let r = check(&g);
        assert_eq!(r.faults.len(), 1, "{r:?}");
        assert!(r.faults[0].contains("0x00B1D3E0") && r.faults[0].contains("version 2"), "{}", r.faults[0]);
        assert!(r.rule_breaks.iter().any(|b| b.contains("repeat another row's pair list")), "{r:?}");
        assert!(r.rule_breaks.iter().any(|b| b.contains("1 of 3 cell versions have no grid row")), "{r:?}");
    }

    #[test]
    fn a_wrong_piece_use_count_is_a_rule_break() {
        let mut g = grid(vec![vec![802, 0]], 1, vec![(7, vec![(0, vec![802, 0])])], vec![0]);
        let r = check(&g);
        assert!(r.faults.is_empty() && r.rule_breaks.is_empty(), "{r:?}");
        g[1] = EsfNode::U32Array(vec![3, 0, 0, 1, 0, 0, 1, 5]);
        let r = check(&g);
        assert!(r.faults.is_empty(), "{r:?}");
        assert!(r.rule_breaks.iter().any(|b| b.contains("piece 0 count 5 links 1")), "{r:?}");
    }

    #[test]
    fn a_missing_manager_list_is_the_lookup_fault() {
        let g = grid(vec![vec![802, 0]], 2, vec![(7, vec![(0, vec![802, 0]), (1, vec![810, 0])])], vec![0]);
        let r = check(&g);
        assert!(r.faults.iter().any(|f| f.contains("0x00AECFE6")), "{r:?}");
    }

    #[test]
    fn versions_round_trip() {
        let a = [2, 1, 2, 3, 4, 77, 0, 0, 5, 1];
        let v = parse_versions(&a).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], Version { links: vec![(1, 2), (3, 4)], cell: 77, byte: 0 });
        assert_eq!(write_versions(&v), a);
        assert_eq!(parse_versions(&[1, 2]), None);
        assert_eq!(piece_use_positions(&[2, 0, 0, 1, 1, 4, 0, 1]), Some(vec![5, 7]));
    }
}

//! The pathfinder's character obstacles in a written save (SAVE_COMPAT.md §6, §18, §19, §29).
//!
//! `CAMPAIGN_PATHFINDER/PATHFINDING_GRID[0]` keeps one obstacle per character standing on the map
//! (every force commander and some agents): `OBSTACLE_LISTS` #2 lists their character ids and
//! `CHARACTER_OBSTACLE[]` holds {`OBSTACLE`, u32 character id} in the same order. Everything else
//! refers to an obstacle by pairs (character id | 2, slot) in u32 arrays: the obstacle's own
//! `MANAGED_OBSTACLE_BOUNDARY` list, `OBSTACLE_BOUNDARY_MANAGER/OBSTACLE_BOUNDARY[]` and the
//! `OBSTACLE_BASE_GRID_NODE[]` lists (CONFIRMED for every pair in the saves checked). A grid row
//! is one **cell version** (an entry of `OBSTACLE_BOUNDARIES`, PATHFINDING_PORTS.md §11.1): the
//! cell's polygons cut for the layers its pair list names, which may be several obstacles at once
//! (a combined version).
//!
//! The original looks each obstacle's character up when it loads and dereferences it, so an
//! obstacle whose character is gone crashes it (INFERRED from `FUN_00afa4f0`). The writer:
//! * removes the obstacles of characters that died, and of characters that commanded a force in
//!   the source but no longer do (merged into another army: off the map, as in the original);
//! * gives a new commander the obstacle of a source obstacle at exactly the same position (a
//!   garrison in the same settlement): the template is renamed if it is being removed, else
//!   copied, with every pair that names it duplicated for the copy;
//! * puts the obstacle of a character that moved in the state the original leaves it in until
//!   its next refresh: only the zone layer stays (`clear_core`).
//!
//! **A removed layer takes its cell versions with it, whole** (CONFIRMED in the original's saves,
//! SAVE_COMPAT.md §29: wherever an obstacle B left a cell that held the versions {A}, {B} and
//! {A, B}, the original's next save holds {A} alone, in hundreds of nodes of NR-4 → its auto_save
//! and of the Peninsula pair). Filtering B out of {A, B} instead, as the writer did before, left
//! two rows with the pair list {A} in one node, and the original's loader keeps only the first
//! of them when it hands the versions their grid node (`0x00B46CC0` finds before it inserts,
//! `0x00B5BF60` walks the map): the other version then made the last load step walk the version
//! list's sentinel, the 0x00B1D3E0 crash of every NR-5..NR-10 (`grid_load_check`). So every
//! version whose pair list names a removed layer goes: its row (both lists), its entry in the
//! versions array (the survivors renumbered everywhere, the high bit kept), its entries in the
//! other obstacles' `BOUNDARIES`, and one use of each piece it links; the manager loses the pair
//! lists naming the layer. Nodes left without rows go with their cell-map entries.
//!
//! A new commander with no template gets no obstacle (CONFIRMED accepted by the original, §17).

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::campaign::CampaignModel;

use crate::grid_load_check::{PIECE_INDEX, PIECE_LINK, parse_versions, piece_use_positions, write_versions};
use crate::save::SourceIndex;

/// The tag in the low bits of an obstacle reference (CONFIRMED: always 2 for character obstacles).
const TAG_MASK: u32 = 3;

/// What happens to each obstacle owner.
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct ObstaclePlan {
    /// Owners whose obstacle goes.
    pub remove: BTreeSet<u32>,
    /// Removed owner → the new commander who takes the obstacle over.
    pub rename: BTreeMap<u32, u32>,
    /// Kept owner → new commanders who get a copy of its obstacle.
    pub copy: BTreeMap<u32, Vec<u32>>,
    /// Kept owners whose character moved: their core is taken out of the grid as the original does
    /// when a character moves (SAVE_COMPAT.md §18).
    pub clear_core: BTreeSet<u32>,
}

impl ObstaclePlan {
    fn is_empty(&self) -> bool {
        self.remove.is_empty() && self.copy.is_empty() && self.clear_core.is_empty()
    }

    /// Whether the layer (owner, slot) of a pair leaves the grid: a removed (not renamed) owner's
    /// layers, and the core and higher layers of a moved character (the zone, slot 0, stays).
    fn drops_layer(&self, tagged_owner: u32, slot: u32) -> bool {
        let o = tagged_owner & !TAG_MASK;
        (self.remove.contains(&o) && !self.rename.contains_key(&o)) || (slot != ZONE_SLOT && self.clear_core.contains(&o))
    }

    /// Whether a pair list names a dropped layer: the cell version with that list goes whole.
    fn drops_list(&self, pairs: &[u32]) -> bool {
        pairs.chunks(2).any(|p| p.len() == 2 && self.drops_layer(p[0], p[1]))
    }

    /// The pairs of a u32 pair array after the plan: removed owners dropped, renamed ones renamed,
    /// copied ones followed by the copies' pairs.
    fn pairs(&self, a: &[u32]) -> Vec<u32> {
        self.pairs_with(a, true)
    }

    /// [`Self::pairs`]; `copies` false leaves out the copies' pairs (for an obstacle's own list).
    fn pairs_with(&self, a: &[u32], copies: bool) -> Vec<u32> {
        let mut out = Vec::with_capacity(a.len());
        for p in a.chunks(2) {
            let owner = p[0] & !TAG_MASK;
            let tag = p[0] & TAG_MASK;
            let rest = &p[1..];
            if let Some(&to) = self.rename.get(&owner) {
                out.push(to | tag);
                out.extend_from_slice(rest);
            } else if self.remove.contains(&owner) {
                continue;
            } else {
                out.extend_from_slice(p);
                for &to in self.copy.get(&owner).into_iter().flatten().filter(|_| copies) {
                    out.push(to | tag);
                    out.extend_from_slice(rest);
                }
            }
        }
        out
    }
}

/// Plans the obstacle changes for `owners` (the source's obstacle owners).
pub(crate) fn plan(owners: &[u32], model: &CampaignModel, ix: &SourceIndex) -> ObstaclePlan {
    let w = &model.world;
    let commanders_now: BTreeSet<u32> = w.forces.values().filter_map(|f| f.commander).map(|c| c.raw() as u32).collect();
    let commanders_before: BTreeSet<u32> = ix.force_commander.values().copied().filter(|&c| c != 0).collect();
    let alive = |c: u32| w.characters.contains_key(&ntw_sim::campaign::CharacterId(c as i32));
    let mut p = ObstaclePlan::default();
    for &o in owners {
        if !alive(o) || (commanders_before.contains(&o) && !commanders_now.contains(&o)) {
            p.remove.insert(o);
        }
    }
    for &o in owners {
        if p.remove.contains(&o) {
            continue;
        }
        let Some(ch) = w.characters.get(&ntw_sim::campaign::CharacterId(o as i32)) else { continue };
        if ix.char_pos.get(&(o as i32)).is_some_and(|&q| q != (ch.position.0.raw(), ch.position.1.raw())) {
            p.clear_core.insert(o);
        }
    }
    let have: BTreeSet<u32> = owners.iter().copied().collect();
    for &c in &commanders_now {
        if have.contains(&c) {
            continue;
        }
        let Some(ch) = w.characters.get(&ntw_sim::campaign::CharacterId(c as i32)) else { continue };
        let pos = (ch.position.0.raw(), ch.position.1.raw());
        let at = |o: &&u32| ix.char_pos.get(&(**o as i32)) == Some(&pos);
        // Prefer an obstacle that is going anyway (rename), else copy a kept one.
        if let Some(&t) = owners.iter().filter(at).find(|o| p.remove.contains(o) && !p.rename.contains_key(o)) {
            p.rename.insert(t, c);
        } else if let Some(&t) = owners.iter().filter(at).find(|o| !p.remove.contains(o)) {
            p.copy.entry(t).or_default().push(c);
        }
    }
    p
}

/// Applies the plan to the pathfinder of a `CAMPAIGN_MODEL` record.
pub(crate) fn update(model_rec: &mut EsfRecord, model: &CampaignModel, ix: &SourceIndex) {
    let Some(item) = model_rec
        .children
        .iter_mut()
        .find_map(|c| match c {
            EsfNode::Record(r) if r.name == "CAMPAIGN_PATHFINDER" => Some(r),
            _ => None,
        })
        .and_then(|p| {
            p.children.iter_mut().find_map(|c| match c {
                EsfNode::RecordArray(a) if a.name == "PATHFINDING_GRID" => a.items.first_mut(),
                _ => None,
            })
        })
    else {
        return;
    };
    let owners: Vec<u32> = obstacle_lists(item).and_then(|l| l.get(2).and_then(EsfNode::as_u32_array)).map(<[u32]>::to_vec).unwrap_or_default();
    let p = plan(&owners, model, ix);
    if p.is_empty() {
        return;
    }
    // The versions that go: every list-1 row naming a dropped layer. Survivors are renumbered.
    let renumber = drop_versions(item, &p);
    // The obstacle list and its id list.
    let mut ids = Vec::new();
    if let Some(lists) = obstacle_lists(item) {
        for c in &mut lists.children {
            let EsfNode::RecordArray(a) = c else { continue };
            if a.name != "CHARACTER_OBSTACLE" {
                continue;
            }
            let mut items = Vec::with_capacity(a.items.len());
            for it in std::mem::take(&mut a.items) {
                let owner = it.iter().find_map(EsfNode::as_u32).unwrap_or(0);
                let copies = p.copy.get(&owner).cloned().unwrap_or_default();
                let keep = match p.rename.get(&owner) {
                    Some(&to) => Some(retarget(it.clone(), owner, to, &p)),
                    None if p.remove.contains(&owner) => None,
                    None => Some(clear_core(own_pairs(it.clone(), &p), owner, &p)),
                };
                if let Some(mut k) = keep {
                    renumber_boundaries(&mut k, &renumber);
                    ids.push(k.iter().find_map(EsfNode::as_u32).unwrap_or(0));
                    items.push(k);
                }
                for to in copies {
                    let mut k = retarget(it.clone(), owner, to, &p);
                    renumber_boundaries(&mut k, &renumber);
                    ids.push(to);
                    items.push(k);
                }
            }
            a.items = items;
        }
        if let Some(slot) = lists.children.get_mut(2)
            && matches!(slot, EsfNode::U32Array(_))
        {
            *slot = EsfNode::U32Array(ids);
        }
    }
    // Every other pair list in the grid.
    for n in item.iter_mut() {
        match n {
            EsfNode::Record(r) if r.name == "OBSTACLE_BOUNDARY_MANAGER" => {
                for c in &mut r.children {
                    let EsfNode::RecordArray(a) = c else { continue };
                    a.items.retain_mut(|it| {
                        let mut any = false;
                        for x in it.iter_mut() {
                            if let EsfNode::U32Array(v) = x {
                                if p.drops_list(v) {
                                    return false;
                                }
                                *v = p.pairs(v);
                                any |= !v.is_empty();
                            }
                        }
                        any
                    });
                }
            }
            EsfNode::RecordArray(a) if a.name == "OBSTACLE_BASE_GRID_NODE" => {
                for node in &mut a.items {
                    grid_node(node, &p, &renumber);
                }
            }
            _ => {}
        }
    }
    drop_empty_grid_nodes(item);
    sync_manager(item, &p.clear_core);
}

fn obstacle_lists(item: &mut [EsfNode]) -> Option<&mut EsfRecord> {
    item.iter_mut().find_map(|n| match n {
        EsfNode::Record(r) if r.name == "OBSTACLE_LISTS" => Some(&mut **r),
        _ => None,
    })
}

/// Old version index → new one (`None` for a dropped version); empty when nothing is dropped
/// or the arrays are not understood (then every index stays).
type Renumber = Vec<Option<u32>>;

fn renumbered(renumber: &Renumber, v: u32) -> Option<u32> {
    if renumber.is_empty() {
        return Some(v);
    }
    renumber.get((v & 0x7fff_ffff) as usize).copied().flatten().map(|n| n | (v & 0x8000_0000))
}

/// Takes the cell versions whose list-1 row names a dropped layer out of `OBSTACLE_BOUNDARIES`
/// #0, gives back one use of each piece they link (`#1`), and returns the renumbering of the
/// survivors (SAVE_COMPAT.md §29; the rows themselves go in [`grid_node`]).
fn drop_versions(item: &mut [EsfNode], p: &ObstaclePlan) -> Renumber {
    let Some(nodes) = item.iter().find_map(|n| n.as_record_array().filter(|a| a.name == "OBSTACLE_BASE_GRID_NODE")) else { return Vec::new() };
    let mut dropped: BTreeSet<u32> = BTreeSet::new();
    for it in &nodes.items {
        let Some(l1) = it.iter().find_map(EsfNode::as_record_array) else { continue };
        for row in &l1.items {
            let Some(v) = row.first().and_then(EsfNode::as_u32) else { continue };
            if row_pair_lists(row).iter().any(|pairs| p.drops_list(pairs)) {
                dropped.insert(v);
            }
        }
    }
    if dropped.is_empty() {
        return Vec::new();
    }
    let Some(versions) = item
        .iter()
        .find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARIES"))
        .and_then(|r| r.get(0))
        .and_then(EsfNode::as_u32_array)
        .and_then(parse_versions)
    else {
        return Vec::new();
    };
    // One use less per piece link of a dropped version.
    if let Some(EsfNode::U32Array(pieces)) = item.get_mut(1)
        && let Some(use_pos) = piece_use_positions(pieces)
    {
        for &v in &dropped {
            for &(_, link) in versions.get(v as usize).map(|v| v.links.as_slice()).unwrap_or_default() {
                if link & PIECE_LINK != 0
                    && let Some(&pos) = use_pos.get((link & PIECE_INDEX) as usize)
                {
                    pieces[pos] = pieces[pos].saturating_sub(1);
                }
            }
        }
    }
    let mut renumber = Vec::with_capacity(versions.len());
    let mut next = 0u32;
    let mut kept = Vec::with_capacity(versions.len() - dropped.len());
    for (i, v) in versions.into_iter().enumerate() {
        if dropped.contains(&(i as u32)) {
            renumber.push(None);
        } else {
            renumber.push(Some(next));
            next += 1;
            kept.push(v);
        }
    }
    if let Some(EsfNode::Record(r)) = item.iter_mut().find(|n| matches!(n, EsfNode::Record(r) if r.name == "OBSTACLE_BOUNDARIES"))
        && let Some(EsfNode::U32Array(a)) = r.children.first_mut()
    {
        *a = write_versions(&kept);
    }
    renumber
}

/// Renumbers the `BOUNDARIES` slot entries of a `CHARACTER_OBSTACLE` item (dropped versions go,
/// the ring flag in the high bit stays).
fn renumber_boundaries(it: &mut [EsfNode], renumber: &Renumber) {
    if renumber.is_empty() {
        return;
    }
    for n in it.iter_mut() {
        let EsfNode::Record(o) = n else { continue };
        for c in &mut o.children {
            let EsfNode::RecordArray(a) = c else { continue };
            if a.name != "BOUNDARIES" {
                continue;
            }
            for slot in &mut a.items {
                for x in slot.iter_mut() {
                    if let EsfNode::U32Array(v) = x {
                        *v = v.iter().filter_map(|&e| renumbered(renumber, e)).collect();
                    }
                }
            }
        }
    }
}

/// Drops the grid nodes whose boundary lists are left empty, and their cells from the cell → node
/// map that follows the node array (a u32 array of (cell, node index) pairs, one per node,
/// CONFIRMED: a permutation of the node indexes in every save). The original never writes an
/// empty node (it drops the node and its cell, CONFIRMED: its saves after removals have fewer
/// nodes and none empty; SAVE_COMPAT.md §18).
fn drop_empty_grid_nodes(item: &mut [EsfNode]) {
    let Some(pos) = item.iter().position(|n| matches!(n, EsfNode::RecordArray(a) if a.name == "OBSTACLE_BASE_GRID_NODE")) else { return };
    let EsfNode::RecordArray(nodes) = &mut item[pos] else { return };
    let empty = |it: &Vec<EsfNode>| {
        let lists: Vec<usize> = it.iter().filter_map(|n| match n {
            EsfNode::RecordArray(a) => Some(a.items.len()),
            _ => None,
        }).collect();
        !lists.is_empty() && lists.iter().all(|&l| l == 0)
    };
    let keep: Vec<bool> = nodes.items.iter().map(|it| !empty(it)).collect();
    if keep.iter().all(|&k| k) {
        return;
    }
    // Old node index → new one.
    let mut new_index = Vec::with_capacity(keep.len());
    let mut next = 0u32;
    for &k in &keep {
        new_index.push(k.then_some(next));
        next += u32::from(k);
    }
    let mut i = 0;
    nodes.items.retain(|_| {
        i += 1;
        keep[i - 1]
    });
    if let Some(EsfNode::U32Array(map)) = item.get_mut(pos + 1)
        && map.len() == 2 * keep.len()
    {
        let mut out = Vec::with_capacity(2 * next as usize);
        for p in map.chunks(2) {
            if let Some(Some(n)) = new_index.get(p[1] as usize) {
                out.push(p[0]);
                out.push(*n);
            }
        }
        *map = out;
    }
}

/// The pair arrays of one grid-node row (a `MANAGED_OBSTACLE_BOUNDARY` item: {u32 piece, u32 x,
/// bool, u32[] pairs} in the first list, {bool, u32[] pairs} in the second; the record form keeps
/// the same fields inside a record).
fn row_pairs(mob: &mut [EsfNode]) -> Vec<&mut Vec<u32>> {
    let mut out = Vec::new();
    for x in mob.iter_mut() {
        match x {
            EsfNode::U32Array(v) => out.push(v),
            EsfNode::Record(r) => {
                for y in &mut r.children {
                    if let EsfNode::U32Array(v) = y {
                        out.push(v);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// [`row_pairs`] on a shared row.
fn row_pair_lists(mob: &[EsfNode]) -> Vec<&[u32]> {
    let mut out = Vec::new();
    for x in mob {
        match x {
            EsfNode::U32Array(v) => out.push(v.as_slice()),
            EsfNode::Record(r) => out.extend(r.children.iter().filter_map(EsfNode::as_u32_array)),
            _ => {}
        }
    }
    out
}

/// Makes the boundary manager the set the loader needs (SAVE_COMPAT.md §19, CONFIRMED in every
/// original save): distinct pair lists, none empty, and every grid row's pair list among them
/// (the loader looks each row's list up there by value and dereferences the result,
/// 0x00B53800 -> 0x00AECFE0). Lists that became equal go; a row list the plan made that the
/// manager lacks (a copy's pairs added to a shared list) is added. Unused lists may stay (the
/// original keeps some). A cleared obstacle keeps the one list of its core slot, [owner | 2, 1]
/// (CONFIRMED: one such entry per moved obstacle in the original's saves, matching its managed
/// slot 1).
fn sync_manager(item: &mut [EsfNode], cleared: &BTreeSet<u32>) {
    let mut rows: BTreeSet<Vec<u32>> = cleared.iter().map(|&o| vec![o | 2, CORE_SLOT]).collect();
    for n in item.iter_mut() {
        let EsfNode::RecordArray(a) = n else { continue };
        if a.name != "OBSTACLE_BASE_GRID_NODE" {
            continue;
        }
        for node in &mut a.items {
            for x in node.iter_mut() {
                let EsfNode::RecordArray(l) = x else { continue };
                for mob in &mut l.items {
                    for v in row_pairs(mob) {
                        rows.insert(v.clone());
                    }
                }
            }
        }
    }
    for n in item.iter_mut() {
        let EsfNode::Record(r) = n else { continue };
        if r.name != "OBSTACLE_BOUNDARY_MANAGER" {
            continue;
        }
        for c in &mut r.children {
            let EsfNode::RecordArray(a) = c else { continue };
            let mut seen: BTreeSet<Vec<u32>> = BTreeSet::new();
            a.items.retain(|it| {
                let v = it.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().to_vec();
                !v.is_empty() && seen.insert(v)
            });
            for v in rows.difference(&seen) {
                if !v.is_empty() {
                    a.items.push(vec![EsfNode::U32Array(v.clone())]);
                }
            }
        }
    }
}

/// Applies the plan to one grid node {u32 key, u32, list 1 [{u32 version, u32 x, bool, pairs}],
/// list 2 [{bool, pairs}]}. List 2 holds the same pair lists as list 1 in another order
/// (CONFIRMED: a permutation in every node of every original save, `save_audit list_perm`). A
/// row goes, in either list, when its pair list names a dropped layer (the whole cell version,
/// as the original does, SAVE_COMPAT.md §29); the lists stay permutations of each other. The
/// survivors' pairs follow the plan (renames, copies) and their version index the renumbering.
/// No other field is touched (the second u32 is not a pair count). Returns the rows dropped.
fn grid_node(node: &mut [EsfNode], p: &ObstaclePlan, renumber: &Renumber) -> usize {
    let mut dropped = 0;
    for n in node.iter_mut() {
        let EsfNode::RecordArray(l) = n else { continue };
        let before = l.items.len();
        l.items.retain_mut(|mob| {
            if row_pair_lists(mob).iter().any(|v| p.drops_list(v)) {
                return false;
            }
            let mut any = false;
            for v in row_pairs(mob) {
                *v = p.pairs(v);
                any |= !v.is_empty();
            }
            if let Some(EsfNode::U32(v)) = mob.first_mut() {
                match renumbered(renumber, *v) {
                    Some(nv) => *v = nv,
                    None => return false,
                }
            }
            any
        });
        dropped += before - l.items.len();
    }
    dropped
}

/// A `CHARACTER_OBSTACLE` item with its pair lists updated by the plan.
fn own_pairs(mut it: Vec<EsfNode>, p: &ObstaclePlan) -> Vec<EsfNode> {
    for n in &mut it {
        if let EsfNode::Record(o) = n {
            walk_pairs(o, &|v| p.pairs_with(v, false));
        }
    }
    it
}

/// A `CHARACTER_OBSTACLE` item moved from `from` to `to`: owner id and own pairs renamed, other
/// owners' pairs updated by the plan.
fn retarget(mut it: Vec<EsfNode>, from: u32, to: u32, p: &ObstaclePlan) -> Vec<EsfNode> {
    for n in &mut it {
        match n {
            EsfNode::U32(v) if *v == from => *v = to,
            EsfNode::Record(o) => walk_pairs(o, &|v| {
                let mut out = Vec::with_capacity(v.len());
                for pair in v.chunks(2) {
                    if pair[0] & !TAG_MASK == from {
                        out.push(to | (pair[0] & TAG_MASK));
                        out.extend_from_slice(&pair[1..]);
                    } else {
                        out.extend(p.pairs_with(pair, false));
                    }
                }
                out
            }),
            _ => {}
        }
    }
    it
}

/// Applies `f` to the pair arrays of an obstacle's `MANAGED_OBSTACLE_BOUNDARY` list.
fn walk_pairs(o: &mut EsfRecord, f: &dyn Fn(&[u32]) -> Vec<u32>) {
    for c in &mut o.children {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "MANAGED_OBSTACLE_BOUNDARY" {
            continue;
        }
        for item in &mut a.items {
            for x in item.iter_mut() {
                if let EsfNode::U32Array(v) = x {
                    *v = f(v);
                }
            }
        }
    }
}

/// The core slot of a character obstacle's boundary lists (slot 0 is the zone; CONFIRMED by the
/// original's moved obstacles, SAVE_COMPAT.md §18).
const CORE_SLOT: u32 = 1;

/// The zone slot of a character obstacle's boundary lists (CONFIRMED, SAVE_COMPAT.md §18).
const ZONE_SLOT: u32 = 0;

/// A kept obstacle whose character moved, put in the state the original leaves a moved
/// character's obstacle in until its next refresh (CONFIRMED in its vanilla saves, 13/13 and 8/8
/// moved obstacles, SAVE_COMPAT.md §18-§19): only the zone stays (slot 0: its `BOUNDARIES`, its
/// managed slot, box, zone cells and grid pairs); `BOUNDARIES` slots 1.. are empty, managed slot 1
/// stays ([owner | 2, 1], with its boundary-manager list), managed slots 2.. are off, #4 = 0 and
/// the core cell range #15..#18 = 0.
fn clear_core(mut it: Vec<EsfNode>, owner: u32, p: &ObstaclePlan) -> Vec<EsfNode> {
    if !p.clear_core.contains(&owner) {
        return it;
    }
    for n in &mut it {
        let EsfNode::Record(o) = n else { continue };
        if o.name != "OBSTACLE" {
            continue;
        }
        for c in &mut o.children {
            let EsfNode::RecordArray(a) = c else { continue };
            if a.name == "BOUNDARIES" {
                for item in a.items.iter_mut().skip(CORE_SLOT as usize) {
                    for x in item.iter_mut() {
                        if let EsfNode::U32Array(v) = x {
                            v.clear();
                        }
                    }
                }
            } else if a.name == "MANAGED_OBSTACLE_BOUNDARY" {
                for item in a.items.iter_mut().skip(CORE_SLOT as usize + 1) {
                    *item = vec![EsfNode::Bool(false)];
                }
            }
        }
        if matches!(o.children.get(4), Some(EsfNode::U32(_))) {
            o.children[4] = EsfNode::U32(0);
        }
        for i in 15..=18 {
            if matches!(o.children.get(i), Some(EsfNode::U16(_))) {
                o.children[i] = EsfNode::U16(0);
            }
        }
    }
    it
}


#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::esf::EsfRecordArray;

    #[test]
    fn pairs_follow_the_plan() {
        let mut p = ObstaclePlan::default();
        p.remove.insert(800);
        p.remove.insert(808);
        p.rename.insert(808, 1000);
        p.copy.insert(816, vec![1008]);
        // (800|2, 0) dropped, (808|2, 1) renamed, (816|2, 3) kept and copied, (824|2, 0) kept.
        let a = [802, 0, 810, 1, 818, 3, 826, 0];
        assert_eq!(p.pairs(&a), vec![1002, 1, 818, 3, 1010, 3, 826, 0]);
        // A list naming a removed layer goes whole; a renamed owner's does not.
        assert!(p.drops_list(&[818, 3, 802, 0]));
        assert!(!p.drops_list(&[810, 1, 818, 3]));
        p.clear_core.insert(824);
        assert!(!p.drops_list(&[826, 0]));
        assert!(p.drops_list(&[826, 1]));
    }

    #[test]
    fn a_removed_layer_takes_its_versions_whole() {
        let mut p = ObstaclePlan::default();
        p.remove.insert(800);
        let l1 = |rows: Vec<Vec<EsfNode>>| EsfNode::RecordArray(Box::new(EsfRecordArray { name: "MANAGED_OBSTACLE_BOUNDARY".into(), version: 1, items: rows }));
        let mut node = vec![
            EsfNode::U32(7),
            EsfNode::U32(1),
            l1(vec![
                vec![EsfNode::U32(5), EsfNode::U32(0), EsfNode::Bool(true), EsfNode::U32Array(vec![818, 1])],
                vec![EsfNode::U32(6), EsfNode::U32(0), EsfNode::Bool(true), EsfNode::U32Array(vec![802, 0, 818, 1])],
                vec![EsfNode::U32(9), EsfNode::U32(0), EsfNode::Bool(true), EsfNode::U32Array(vec![802, 0])],
            ]),
            l1(vec![
                vec![EsfNode::Bool(true), EsfNode::U32Array(vec![802, 0])],
                vec![EsfNode::Bool(true), EsfNode::U32Array(vec![802, 0, 818, 1])],
                vec![EsfNode::Bool(true), EsfNode::U32Array(vec![818, 1])],
            ]),
        ];
        // Versions 6 and 9 go (the combined one is not reduced to [818, 1], which would repeat
        // row 0's list); version 5 is renumbered to 3.
        let renumber: Renumber = vec![Some(0), Some(1), Some(2), None, None, Some(3), None, None, None, None];
        assert_eq!(grid_node(&mut node, &p, &renumber), 4);
        let EsfNode::RecordArray(a) = &node[2] else { panic!() };
        assert_eq!(a.items, vec![vec![EsfNode::U32(3), EsfNode::U32(0), EsfNode::Bool(true), EsfNode::U32Array(vec![818, 1])]]);
        let EsfNode::RecordArray(b) = &node[3] else { panic!() };
        assert_eq!(b.items, vec![vec![EsfNode::Bool(true), EsfNode::U32Array(vec![818, 1])]]);
    }

    #[test]
    fn boundary_entries_are_renumbered_with_the_ring_flag_kept() {
        let renumber: Renumber = vec![Some(0), None, Some(1)];
        assert_eq!(renumbered(&renumber, 2), Some(1));
        assert_eq!(renumbered(&renumber, 0x8000_0002), Some(0x8000_0001));
        assert_eq!(renumbered(&renumber, 1), None);
        assert_eq!(renumbered(&Vec::new(), 0x8000_0007), Some(0x8000_0007));
    }
}

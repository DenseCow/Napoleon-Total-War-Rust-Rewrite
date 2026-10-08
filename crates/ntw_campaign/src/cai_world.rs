//! Keeping the campaign AI block's world mirrors in step with the world (SAVE_COMPAT.md §10).
//!
//! The AI block (`CAI_INTERFACE`, version 13) mirrors every character, force and unit in
//! `CAI_WORLD` (CONFIRMED on every save of the original: exactly one mirror per object, every
//! mirror's object exists). The relations it stores, all CONFIRMED (100 % on the user's original
//! saves, `cai_audit mirror_rel` / `mirror_loc`):
//! * `CAI_UNIT` {#0 the attached character's mirror (0 none), #1 unit id, #2 its force's mobile};
//! * `CAI_RESOURCE_MOBILE` {#0 the leader's mirror, #4 the characters in it (leader + characters
//!   attached to its units, as a set), #5 its units' mirrors (as a set), #10 force id (0 for an
//!   agent's own mobile)}, with `OWNED_DIRECT` {faction component} and `CAI_SITUATED` {#0/#1 the
//!   leader's `LOCOMOTABLE` position, #2 the region component, #3 that region's theatres};
//! * `CAI_CHARACTER` {#0 the mobile it leads, #1 the unit it is attached to, #2 the mobile whose #4
//!   holds it, #3 character id}, with `OWNED_INDIRECT` {faction component};
//! * `CAI_FACTION` #3 = the faction's mobiles, #4 = its characters; `CAI_REGION` #9 = the mobiles
//!   situated in the region; `THEATRE` #1 = the mobiles in the theatre; a settlement's
//!   `CAI_GARRISONABLE` #0 = its garrison's mobile.
//!
//! [`sync`] removes the mirrors of objects that are gone (with everything that depends on them,
//! `cai::remove_components`), creates mirrors for new objects (empty BDI links, as a component is
//! when created; PROVISIONAL: the original's AI adds its beliefs on its next turn), sets every
//! relation above from the world, and advances the next-id counter. [`check`] tests the same
//! relations on a save.

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

use crate::cai::{self, Removal};

/// A character as the AI block needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharView {
    /// `FACTION` #8 of its faction.
    pub faction: u32,
    /// `LOCOMOTABLE` #0/#1 (20-bit fixed point).
    pub pos: (i32, i32),
    /// The force it commands (0 none).
    pub commands: u32,
    /// The unit it is attached to (0 none).
    pub unit: u32,
}

/// A force as the AI block needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForceView {
    /// `FACTION` #8 of its faction.
    pub faction: u32,
    /// Its commander.
    pub commander: u32,
    /// Its units, in order.
    pub units: Vec<u32>,
}

/// The region key at a position (20-bit fixed point).
pub type RegionAt = Box<dyn Fn(i32, i32) -> Option<String>>;

/// The world as the AI block mirrors it.
#[derive(Default)]
pub struct WorldView {
    /// Characters by id.
    pub characters: BTreeMap<u32, CharView>,
    /// Forces by id.
    pub forces: BTreeMap<u32, ForceView>,
    /// Units by id: (force, attached character or 0).
    pub units: BTreeMap<u32, (u32, u32)>,
    /// Settlement residence id → the garrisoned force.
    pub garrisons: BTreeMap<u32, u32>,
    /// The forces garrisoned in slot residences (forts, ports).
    pub slot_garrisons: BTreeSet<u32>,
    /// The region key at a position (20-bit fixed point), when the map is known.
    pub region_at: Option<RegionAt>,
}

impl WorldView {
    /// The world of a save's `WORLD` record (for checks and tests).
    pub fn from_save(esf: &EsfFile) -> WorldView {
        let mut w = WorldView::default();
        let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return w };
        let mut fs: Vec<(&EsfRecord, bool)> = world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).map(|f| (f, false)).collect();
        // Rebel armies are mirrored (commander, force, units), rebel characters without a force
        // are not (CONFIRMED: the original's Peninsula save after 3 End Turns mirrors its 8 rebel
        // armies in full; the Early April 1806 save's 2 forceless rebel colonels have none;
        // SAVE_COMPAT.md §18).
        fs.extend(world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")).map(|f| (f, true)));
        for (f, rebel) in fs {
            let fid = f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32).unwrap_or(0) as u32;
            for c in f
                .record_array("CHARACTER_ARRAY")
                .into_iter()
                .flat_map(|a| a.records())
                .filter(|c| c.name == "CHARACTER" && (!rebel || c.get_u32(4).is_some_and(|x| x != 0)))
            {
                let loco = c.children.first().and_then(EsfNode::as_record);
                w.characters.insert(
                    c.get_i32(2).unwrap_or(0) as u32,
                    CharView {
                        faction: fid,
                        pos: (loco.and_then(|l| l.get_i32(0)).unwrap_or(0), loco.and_then(|l| l.get_i32(1)).unwrap_or(0)),
                        commands: c.get_u32(4).unwrap_or(0),
                        unit: c.get_u32(5).unwrap_or(0),
                    },
                );
            }
            for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
                let Some(m) = a.child("MILITARY_FORCE") else { continue };
                let id = m.get_u32(0).unwrap_or(0);
                let mut units = Vec::new();
                for u in a.record_array("UNITS_ARRAY").into_iter().flat_map(|x| x.records()).filter_map(|x| x.child("UNIT")) {
                    let uid = u.get_i32(4).unwrap_or(0) as u32;
                    units.push(uid);
                    w.units.insert(uid, (id, u.get_u32(10).unwrap_or(0)));
                }
                w.forces.insert(id, ForceView { faction: fid, commander: m.get_u32(1).unwrap_or(0), units });
            }
        }
        if let Some(rm) = world.child("REGION_MANAGER") {
            for r in rm.record_array("REGIONS_ARRAY").into_iter().flat_map(|a| a.records()) {
                if let Some(g) = r.child("SETTLEMENT").and_then(|s| s.child("SIEGEABLE_GARRISON_RESIDENCE")) {
                    let f = g.get_u32(12).unwrap_or(0);
                    if f != 0 {
                        w.garrisons.insert(g.get_u32(1).unwrap_or(0), f);
                    }
                }
                let slots = r.child("REGION_SLOT_MANAGER").and_then(|m| m.record_array("REGION_SLOT_ARRAY")).into_iter().flat_map(|a| a.records());
                for s in slots {
                    if let Some(f) = s.child("SIEGEABLE_GARRISON_RESIDENCE").and_then(|g| g.get_u32(12)).filter(|&f| f != 0) {
                        w.slot_garrisons.insert(f);
                    }
                }
            }
        }
        w
    }
}

/// What [`sync`] did.
#[derive(Debug, Default)]
pub struct SyncReport {
    /// Mirrors removed because their object is gone, and their dependants.
    pub removal: Removal,
    /// Mirrors created: characters, mobiles, units.
    pub added: [usize; 3],
}

const LIST_CHARS: &str = "CAI_WORLD_CHARACTERS";
const LIST_MOBILES: &str = "CAI_WORLD_RESOURCE_MOBILES";
const LIST_UNITS: &str = "CAI_WORLD_UNITS";

fn class_rec<'a>(item: &'a [EsfNode], name: &str) -> Option<&'a EsfRecord> {
    item.iter().filter_map(EsfNode::as_record).find(|r| r.name == name)
}

fn class_rec_mut<'a>(item: &'a mut [EsfNode], name: &str) -> Option<&'a mut EsfRecord> {
    item.iter_mut().find_map(|n| match n {
        EsfNode::Record(r) if r.name == name => Some(&mut **r),
        _ => None,
    })
}

fn child_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    r.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(b) if b.name == name => Some(&mut **b),
        _ => None,
    })
}

fn array_items_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut Vec<Vec<EsfNode>>> {
    r.children.iter_mut().find_map(|c| match c {
        EsfNode::RecordArray(a) if a.name == name => Some(&mut a.items),
        _ => None,
    })
}

fn item_id(item: &[EsfNode]) -> Option<u32> {
    cai::block_pos(item).map(|p| item[p + 1].as_u32().unwrap_or(0))
}

fn u32s(r: &EsfRecord, i: usize) -> Vec<u32> {
    r.get(i).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default()
}

fn set_u32(r: &mut EsfRecord, i: usize, v: u32) {
    if let Some(n @ EsfNode::U32(_)) = r.children.get_mut(i) {
        *n = EsfNode::U32(v);
    }
}

fn set_i32(r: &mut EsfRecord, i: usize, v: i32) {
    if let Some(n @ EsfNode::I32(_)) = r.children.get_mut(i) {
        *n = EsfNode::I32(v);
    }
}

fn set_list(r: &mut EsfRecord, i: usize, v: Vec<u32>) {
    if let Some(n @ EsfNode::U32Array(_)) = r.children.get_mut(i) {
        *n = EsfNode::U32Array(v);
    }
}

/// `want` as a list that keeps the order of `old` for the values kept and appends the new ones
/// in `want` order.
fn merged(old: &[u32], want: &[u32]) -> Vec<u32> {
    let set: BTreeSet<u32> = want.iter().copied().collect();
    let mut out: Vec<u32> = old.iter().copied().filter(|v| set.contains(v)).collect();
    let have: BTreeSet<u32> = out.iter().copied().collect();
    out.extend(want.iter().copied().filter(|v| !have.contains(v)));
    out
}

/// What the AI block says about the world: game id → component, by kind.
#[derive(Default)]
struct Index {
    chars: BTreeMap<u32, u32>,
    forces: BTreeMap<u32, u32>,
    units: BTreeMap<u32, u32>,
    /// Agent mobiles (#10 = 0): leader component → mobile component.
    agents: BTreeMap<u32, u32>,
    /// Faction id → faction component.
    factions: BTreeMap<u32, u32>,
    /// Region key → (component, theatres).
    regions: BTreeMap<String, (u32, Vec<u32>)>,
}

fn index(cai: &EsfRecord) -> Index {
    let mut ix = Index::default();
    let Some(w) = cai.child("CAI_WORLD") else { return ix };
    let items = |list: &str| w.record_array(list).into_iter().flat_map(|a| a.items.iter());
    for it in items(LIST_CHARS) {
        if let (Some(c), Some(r)) = (item_id(it), class_rec(it, "CAI_CHARACTER")) {
            ix.chars.insert(r.get_u32(3).unwrap_or(0), c);
        }
    }
    for it in items(LIST_MOBILES) {
        if let (Some(c), Some(r)) = (item_id(it), class_rec(it, "CAI_RESOURCE_MOBILE")) {
            match r.get_u32(10).unwrap_or(0) {
                0 => {
                    ix.agents.insert(r.get_u32(0).unwrap_or(0), c);
                }
                f => {
                    ix.forces.insert(f, c);
                }
            }
        }
    }
    for it in items(LIST_UNITS) {
        if let (Some(c), Some(r)) = (item_id(it), class_rec(it, "CAI_UNIT")) {
            ix.units.insert(r.get_u32(1).unwrap_or(0), c);
        }
    }
    for it in items("CAI_WORLD_FACTIONS") {
        if let (Some(c), Some(r)) = (item_id(it), class_rec(it, "CAI_FACTION")) {
            ix.factions.insert(r.get_u32(6).unwrap_or(0), c);
        }
    }
    for it in items("CAI_WORLD_REGIONS") {
        if let (Some(c), Some(r)) = (item_id(it), class_rec(it, "CAI_REGION")) {
            ix.regions.insert(r.get_str(10).unwrap_or_default().to_string(), (c, u32s(r, 0)));
        }
    }
    ix
}

/// Resets a cloned item's component block to a freshly created component with id `id`: no
/// links, no counters (CONFIRMED shape of the original's new mirrors, apart from the links its AI
/// adds on its turn).
fn reset_block(item: &mut [EsfNode], id: u32) {
    let Some(p) = cai::block_pos(item) else { return };
    item[p + 1] = EsfNode::U32(id);
    for off in [8, 9, 12, 13, 17, 18, 19, 20] {
        if let Some(n @ EsfNode::U32Array(_)) = item.get_mut(p + off) {
            *n = EsfNode::U32Array(Vec::new());
        }
    }
    if let Some(EsfNode::RecordArray(a)) = item.get_mut(p + cai::LINK_OWNS) {
        a.items.clear();
    }
    for off in [10, 11, 15, 16, 22] {
        if let Some(n @ EsfNode::U32(_)) = item.get_mut(p + off) {
            *n = EsfNode::U32(0);
        }
    }
    for off in [7, 21, 23] {
        if let Some(n @ EsfNode::Bool(_)) = item.get_mut(p + off) {
            *n = EsfNode::Bool(false);
        }
    }
}

/// Brings the AI block (`CAI_INTERFACE`) in step with `w` (see the module doc).
pub fn sync(cai_rec: &mut EsfRecord, w: &WorldView) -> SyncReport {
    let mut report = SyncReport::default();
    // 1. Mirrors of objects that are gone.
    let ix = index(cai_rec);
    let mut gone: BTreeSet<u32> = BTreeSet::new();
    gone.extend(ix.chars.iter().filter(|(g, _)| !w.characters.contains_key(g)).map(|(_, c)| *c));
    gone.extend(ix.forces.iter().filter(|(g, _)| !w.forces.contains_key(g)).map(|(_, c)| *c));
    gone.extend(ix.units.iter().filter(|(g, _)| !w.units.contains_key(g)).map(|(_, c)| *c));
    let gone_chars: BTreeSet<u32> = ix.chars.iter().filter(|(g, _)| !w.characters.contains_key(g)).map(|(_, c)| *c).collect();
    gone.extend(ix.agents.iter().filter(|(leader, _)| gone_chars.contains(leader)).map(|(_, m)| *m));
    if !gone.is_empty() {
        report.removal = cai::remove_components(cai_rec, &gone);
    }
    let mut ix = index(cai_rec);
    // 2. New mirrors.
    let mut next = cai_rec.child("CAI_CENTRAL_BDI_POOL").and_then(|p| p.get_u32(0)).unwrap_or(0);
    let mut alloc = || {
        next += 1;
        next - 1
    };
    let Some(world) = child_mut(cai_rec, "CAI_WORLD") else { return report };
    let template = |world: &mut EsfRecord, list: &str| array_items_mut(world, list).and_then(|a| a.first().cloned());
    let (t_char, t_mob, t_unit) = (template(world, LIST_CHARS), template(world, LIST_MOBILES), template(world, LIST_UNITS));
    let mut new_units = Vec::new();
    let missing: Vec<u32> = w.units.keys().copied().filter(|g| !ix.units.contains_key(g)).collect();
    for g in missing {
        let Some(mut it) = t_unit.clone() else { break };
        let id = alloc();
        reset_block(&mut it, id);
        if let Some(r) = class_rec_mut(&mut it, "CAI_UNIT") {
            set_u32(r, 0, 0);
            set_u32(r, 1, g);
            set_u32(r, 2, 0);
        }
        ix.units.insert(g, id);
        new_units.push(it);
    }
    let mut new_chars = Vec::new();
    let missing: Vec<(u32, &CharView)> = w.characters.iter().filter(|(g, _)| !ix.chars.contains_key(g)).map(|(g, c)| (*g, c)).collect();
    for (g, c) in missing {
        let Some(mut it) = t_char.clone() else { break };
        let id = alloc();
        reset_block(&mut it, id);
        if let Some(EsfNode::Record(o)) = it.first_mut().filter(|n| matches!(n, EsfNode::Record(r) if r.name == "OWNED_INDIRECT")) {
            set_u32(o, 0, ix.factions.get(&c.faction).copied().unwrap_or(0));
        }
        if let Some(r) = class_rec_mut(&mut it, "CAI_CHARACTER") {
            for k in [0, 1, 2, 4, 5] {
                set_u32(r, k, 0);
            }
            set_u32(r, 3, g);
        }
        ix.chars.insert(g, id);
        new_chars.push(it);
    }
    let mut new_mobiles = Vec::new();
    let missing: Vec<(u32, &ForceView)> = w.forces.iter().filter(|(g, _)| !ix.forces.contains_key(g)).map(|(g, f)| (*g, f)).collect();
    for (g, f) in missing {
        let Some(mut it) = t_mob.clone() else { break };
        let id = alloc();
        reset_block(&mut it, id);
        if let Some(EsfNode::Record(o)) = it.first_mut().filter(|n| matches!(n, EsfNode::Record(r) if r.name == "OWNED_DIRECT")) {
            set_u32(o, 0, ix.factions.get(&f.faction).copied().unwrap_or(0));
        }
        if let Some(r) = class_rec_mut(&mut it, "CAI_RESOURCE_MOBILE") {
            // A freshly raised force's mobile as the original writes it (CONFIRMED values).
            for k in [0, 1, 2, 9, 12, 13, 15] {
                set_u32(r, k, 0);
            }
            if let Some(EsfNode::RecordArray(a)) = r.children.get_mut(3) {
                a.items.clear();
            }
            for k in [4, 5, 11, 14] {
                set_list(r, k, Vec::new());
            }
            for (k, b) in [(6, true), (7, false), (8, false)] {
                if let Some(n @ EsfNode::Bool(_)) = r.children.get_mut(k) {
                    *n = EsfNode::Bool(b);
                }
            }
            set_u32(r, 10, g);
        }
        ix.forces.insert(g, id);
        new_mobiles.push(it);
    }
    report.added = [new_chars.len(), new_mobiles.len(), new_units.len()];
    if let Some(a) = array_items_mut(world, LIST_UNITS) {
        a.extend(new_units);
    }
    if let Some(a) = array_items_mut(world, LIST_CHARS) {
        a.extend(new_chars);
    }
    if let Some(a) = array_items_mut(world, LIST_MOBILES) {
        a.extend(new_mobiles);
    }
    // 3. Relations.
    let comp_char = |g: u32| ix.chars.get(&g).copied().unwrap_or(0);
    let comp_force = |g: u32| ix.forces.get(&g).copied().unwrap_or(0);
    let comp_unit = |g: u32| ix.units.get(&g).copied().unwrap_or(0);
    // Characters inside each force mobile: the commander and the characters attached to its units.
    let mut members: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&g, f) in &w.forces {
        let mut m = vec![comp_char(f.commander)];
        m.extend(f.units.iter().filter_map(|u| w.units.get(u)).map(|u| u.1).filter(|&c| c != 0 && c != f.commander).map(comp_char));
        m.retain(|&c| c != 0);
        members.insert(comp_force(g), m);
    }
    let agent_of: BTreeMap<u32, u32> = ix.agents.clone();
    if let Some(a) = array_items_mut(world, LIST_UNITS) {
        for it in a.iter_mut() {
            let Some(r) = class_rec_mut(it, "CAI_UNIT") else { continue };
            let Some(&(force, ch)) = r.get_u32(1).and_then(|g| w.units.get(&g)) else { continue };
            set_u32(r, 0, comp_char(ch));
            set_u32(r, 2, comp_force(force));
        }
    }
    let mut situated: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new(); // mobile -> (region, theatres)
    let mut owner_of_mobile: BTreeMap<u32, u32> = BTreeMap::new();
    if let Some(a) = array_items_mut(world, LIST_MOBILES) {
        for it in a.iter_mut() {
            let Some(id) = item_id(it) else { continue };
            let (force, leader) = class_rec(it, "CAI_RESOURCE_MOBILE").map_or((0, 0), |r| (r.get_u32(10).unwrap_or(0), r.get_u32(0).unwrap_or(0)));
            let leader_game = if force != 0 { w.forces.get(&force).map(|f| f.commander) } else { ix.chars.iter().find(|(_, c)| **c == leader).map(|(g, _)| *g) };
            if force != 0
                && let Some(f) = w.forces.get(&force)
                && let Some(r) = class_rec_mut(it, "CAI_RESOURCE_MOBILE")
            {
                set_u32(r, 0, comp_char(f.commander));
                let old4 = u32s(r, 4);
                set_list(r, 4, merged(&old4, members.get(&id).map_or(&[][..], Vec::as_slice)));
                let units: Vec<u32> = f.units.iter().map(|&u| comp_unit(u)).filter(|&c| c != 0).collect();
                let old5 = u32s(r, 5);
                set_list(r, 5, merged(&old5, &units));
            }
            // Position: the leader's.
            let pos = leader_game.and_then(|g| w.characters.get(&g)).map(|c| c.pos);
            if let Some(o) = it.first().and_then(EsfNode::as_record).filter(|o| o.name == "OWNED_DIRECT") {
                owner_of_mobile.insert(id, o.get_u32(0).unwrap_or(0));
            }
            if let Some(s) = class_rec_mut(it, "CAI_SITUATED") {
                if let Some((x, z)) = pos {
                    let moved = s.get_i32(0) != Some(x) || s.get_i32(1) != Some(z) || s.get_u32(2) == Some(0);
                    set_i32(s, 0, x);
                    set_i32(s, 1, z);
                    if moved
                        && let Some(key) = w.region_at.as_ref().and_then(|f| f(x, z))
                        && let Some((rc, th)) = ix.regions.get(&key)
                    {
                        set_u32(s, 2, *rc);
                        set_list(s, 3, th.clone());
                    }
                }
                situated.insert(id, (s.get_u32(2).unwrap_or(0), u32s(s, 3)));
            }
        }
    }
    // Characters.
    let mut char_owner: BTreeMap<u32, u32> = BTreeMap::new();
    if let Some(a) = array_items_mut(world, LIST_CHARS) {
        for it in a.iter_mut() {
            let Some(id) = item_id(it) else { continue };
            if let Some(o) = it.first().and_then(EsfNode::as_record) {
                char_owner.insert(id, o.get_u32(0).unwrap_or(0));
            }
            let Some(r) = class_rec_mut(it, "CAI_CHARACTER") else { continue };
            let Some(c) = r.get_u32(3).and_then(|g| w.characters.get(&g)) else { continue };
            let agent = agent_of.get(&id).copied().unwrap_or(0);
            let lead = if c.commands != 0 { comp_force(c.commands) } else { agent };
            let inside = if c.commands != 0 {
                comp_force(c.commands)
            } else if c.unit != 0 {
                w.units.get(&c.unit).map_or(0, |u| comp_force(u.0))
            } else {
                agent
            };
            set_u32(r, 0, lead);
            set_u32(r, 1, comp_unit(c.unit));
            set_u32(r, 2, inside);
        }
    }
    // Membership lists: factions, regions, theatres, garrisons.
    let mut fac_mobiles: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&m, &f) in &owner_of_mobile {
        fac_mobiles.entry(f).or_default().push(m);
    }
    let mut fac_chars: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&c, &f) in &char_owner {
        fac_chars.entry(f).or_default().push(c);
    }
    let mut region_mobiles: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut theatre_mobiles: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (&m, (r, th)) in &situated {
        region_mobiles.entry(*r).or_default().push(m);
        for &t in th {
            theatre_mobiles.entry(t).or_default().push(m);
        }
    }
    let empty: Vec<u32> = Vec::new();
    if let Some(a) = array_items_mut(world, "CAI_WORLD_FACTIONS") {
        for it in a.iter_mut() {
            let Some(id) = item_id(it) else { continue };
            if let Some(r) = class_rec_mut(it, "CAI_FACTION") {
                let (o3, o4) = (u32s(r, 3), u32s(r, 4));
                set_list(r, 3, merged(&o3, fac_mobiles.get(&id).unwrap_or(&empty)));
                set_list(r, 4, merged(&o4, fac_chars.get(&id).unwrap_or(&empty)));
            }
        }
    }
    if let Some(a) = array_items_mut(world, "CAI_WORLD_REGIONS") {
        for it in a.iter_mut() {
            let Some(id) = item_id(it) else { continue };
            if let Some(r) = class_rec_mut(it, "CAI_REGION") {
                let o = u32s(r, 9);
                set_list(r, 9, merged(&o, region_mobiles.get(&id).unwrap_or(&empty)));
            }
        }
    }
    if let Some(a) = array_items_mut(world, "CAI_WORLD_THEATRES") {
        for it in a.iter_mut() {
            let Some(id) = item_id(it) else { continue };
            if let Some(r) = class_rec_mut(it, "THEATRE") {
                let o = u32s(r, 1);
                set_list(r, 1, merged(&o, theatre_mobiles.get(&id).unwrap_or(&empty)));
            }
        }
    }
    if let Some(a) = array_items_mut(world, "CAI_WORLD_SETTLEMENTS") {
        for it in a.iter_mut() {
            let res = class_rec(it, "CAI_SETTLEMENT").and_then(|s| s.get_u32(2)).unwrap_or(0);
            let want = w.garrisons.get(&res).map_or(0, |&f| comp_force(f));
            if let Some(g) = class_rec_mut(it, "CAI_GARRISONABLE") {
                set_u32(g, 0, want);
            }
        }
    }
    // Slot garrisons (forts, ports): the slot mirrors' `CAI_GARRISONABLE` #0, as a set, are the
    // mobiles of the forces garrisoned in slots (CONFIRMED on every original save). The model
    // never puts a force into a slot, so only garrisons that left are cleared.
    let slot_mobiles: BTreeSet<u32> = w.slot_garrisons.iter().map(|&f| comp_force(f)).collect();
    if let Some(a) = array_items_mut(world, "CAI_WORLD_REGION_SLOTS") {
        for it in a.iter_mut() {
            if let Some(g) = class_rec_mut(it, "CAI_GARRISONABLE")
                && g.get_u32(0).is_some_and(|m| m != 0 && !slot_mobiles.contains(&m))
            {
                set_u32(g, 0, 0);
            }
        }
    }
    // 4. The next-id counter.
    if let Some(p) = child_mut(cai_rec, "CAI_CENTRAL_BDI_POOL") {
        set_u32(p, 0, next);
    }
    report
}

/// The mirror rules of the module doc on a save: one line per broken rule (at most `max`).
pub fn check(esf: &EsfFile, max: usize) -> Vec<String> {
    let Some(cai_rec) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE") else { return Vec::new() };
    let w = WorldView::from_save(esf);
    let ix = index(cai_rec);
    let mut out = Vec::new();
    let mut bad = |s: String| {
        if out.len() < max {
            out.push(s);
        }
    };
    for (kind, have, want) in [
        ("character", ix.chars.keys().copied().collect::<BTreeSet<_>>(), w.characters.keys().copied().collect::<BTreeSet<_>>()),
        ("force", ix.forces.keys().copied().collect(), w.forces.keys().copied().collect()),
        ("unit", ix.units.keys().copied().collect(), w.units.keys().copied().collect()),
    ] {
        for g in want.difference(&have) {
            bad(format!("AI block: {kind} {g} has no mirror"));
        }
        for g in have.difference(&want) {
            bad(format!("AI block: mirror of missing {kind} {g}"));
        }
    }
    let Some(world) = cai_rec.child("CAI_WORLD") else { return out };
    let items = |list: &str| world.record_array(list).into_iter().flat_map(|a| a.items.iter());
    let c = |g: u32| ix.chars.get(&g).copied().unwrap_or(0);
    let f = |g: u32| ix.forces.get(&g).copied().unwrap_or(0);
    let u = |g: u32| ix.units.get(&g).copied().unwrap_or(0);
    for it in items(LIST_UNITS) {
        let Some(r) = class_rec(it, "CAI_UNIT") else { continue };
        let Some(&(force, ch)) = r.get_u32(1).and_then(|g| w.units.get(&g)) else { continue };
        if r.get_u32(0) != Some(c(ch)) || r.get_u32(2) != Some(f(force)) {
            bad(format!("AI block: unit mirror {:?} does not match its unit", item_id(it)));
        }
    }
    let sorted = |mut v: Vec<u32>| {
        v.sort_unstable();
        v
    };
    for it in items(LIST_MOBILES) {
        let Some(r) = class_rec(it, "CAI_RESOURCE_MOBILE") else { continue };
        let Some(fv) = r.get_u32(10).and_then(|g| w.forces.get(&g)) else { continue };
        let mut m = vec![c(fv.commander)];
        m.extend(fv.units.iter().filter_map(|x| w.units.get(x)).map(|x| x.1).filter(|&x| x != 0 && x != fv.commander).map(c));
        let units: Vec<u32> = fv.units.iter().map(|&x| u(x)).collect();
        if r.get_u32(0) != Some(c(fv.commander)) || sorted(u32s(r, 4)) != sorted(m) || sorted(u32s(r, 5)) != sorted(units) {
            bad(format!("AI block: mobile {:?} does not match its force", item_id(it)));
        }
    }
    let slot_want: BTreeSet<u32> = w.slot_garrisons.iter().map(|&g| f(g)).collect();
    let slot_have: BTreeSet<u32> = items("CAI_WORLD_REGION_SLOTS").filter_map(|it| class_rec(it, "CAI_GARRISONABLE")?.get_u32(0)).filter(|&v| v != 0).collect();
    if slot_have != slot_want {
        bad(format!("AI block: slot garrisons {slot_have:?}, world {slot_want:?}"));
    }
    for it in items("CAI_WORLD_SETTLEMENTS") {
        let res = class_rec(it, "CAI_SETTLEMENT").and_then(|s| s.get_u32(2)).unwrap_or(0);
        let want = w.garrisons.get(&res).map_or(0, |&g| f(g));
        let have = class_rec(it, "CAI_GARRISONABLE").and_then(|g| g.get_u32(0));
        // A mirror still at 0 for a garrison that entered is allowed: the original's own saves
        // have it (its auto_save after our NR-C8, SAVE_COMPAT.md §18; it updates the mirror later).
        if have != Some(want) && have != Some(0) {
            bad(format!(
                "AI block: settlement mirror {:?} does not name its garrison (residence {res}: mirror {have:?}, world force {:?} -> mirror {want})",
                item_id(it),
                w.garrisons.get(&res)
            ));
        }
    }
    let agents: BTreeMap<u32, u32> = ix.agents.clone();
    for it in items(LIST_CHARS) {
        let Some(r) = class_rec(it, "CAI_CHARACTER") else { continue };
        let Some(cv) = r.get_u32(3).and_then(|g| w.characters.get(&g)) else { continue };
        let id = item_id(it).unwrap_or(0);
        let agent = agents.get(&id).copied().unwrap_or(0);
        let lead = if cv.commands != 0 { f(cv.commands) } else { agent };
        let inside = if cv.commands != 0 {
            f(cv.commands)
        } else if cv.unit != 0 {
            w.units.get(&cv.unit).map_or(0, |x| f(x.0))
        } else {
            agent
        };
        if r.get_u32(0) != Some(lead) || r.get_u32(1) != Some(u(cv.unit)) || r.get_u32(2) != Some(inside) {
            bad(format!("AI block: character mirror {id} does not match its character"));
        }
    }
    out
}

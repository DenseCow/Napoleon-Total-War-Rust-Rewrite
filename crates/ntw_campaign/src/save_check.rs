//! The links the original game needs in a save, checked on a whole ESF tree
//! (`analysis/campaign/SAVE_COMPAT.md` §4-§6). Every save the original wrote passes (tested on the
//! user's saves); the tests run it on every save we write.
//!
//! What is checked (each a CONFIRMED pattern of the original's saves unless noted):
//! * the player: exactly the header's faction has the `CAMPAIGN_PLAYER_SETUP` human flag, in the
//!   setup and in its own `FACTION` record (the copy the game plays by);
//! * unit names: a regiment / ship list entry is flagged exactly when one unit of the faction carries
//!   it (§20);
//! * victory: the human's tested `CAMPAIGN_VICTORY_CONDITIONS` is not the start position's default
//!   (type 5 with no regions outside the Peninsular campaign), which the original counts as met at
//!   once (CONFIRMED `0x0096FCE0`, SAVE_COMPAT.md §17);
//! * regions: every `GARRISON_RESIDENCE` in a region names its owner, apart from at most one slot
//!   (INFERRED from every original save; a capture hands them all over);
//! * the AI managers: the human faction has the HUMAN manager type (10) in `CAI_INTERFACE` #29 and
//!   its `CAI_FACTION_MANAGER`, no other faction has it;
//! * ids: character, force and unit ids are unique across all three kinds (one global id map);
//! * characters: `CHARACTER_DETAILS` #10 = the id; #4 ≠ 0 names a force it commands; #5 ≠ 0 names
//!   a unit whose #10 is the character;
//! * forces: at least one unit; a commander (`MILITARY_FORCE` #1) who exists in the same faction
//!   and whose #4 is the force; every unit's #10 ≠ 0 names a character of the faction whose #5 is
//!   the unit;
//! * garrisons: residence #12 ≠ 0 names an `ARMY` whose #5 is the residence, and back;
//! * government posts name living characters;
//! * the other id sites (`save_audit refsites`): recruitment pools name characters of the faction
//!   without a force; `CHARACTER` #6, `MILITARY_FORCE` #2, residence #14, `PORT_GARRISON_MANAGER`
//!   #0 (a navy), `COMMERCE_RAIDS` #0 and `PENDING_BATTLE_PARTICIPANT` #0 name existing objects;
//!   `ARMY` #4 = its force id;
//! * pathfinder: every character obstacle's owner exists; `OBSTACLE_LISTS` #2 lists the
//!   `CHARACTER_OBSTACLE` owners in order; every obstacle pair names an obstacle; no grid node
//!   with empty lists, both lists of a node equally long, the cell map indexes every node (§18);
//!   the loader replay (`grid_load_check`, §29) finds no bad pointer and none of the states the
//!   original never writes (a row repeating another row's pair list in one node, a version
//!   without a row or in two nodes, a piece use count not equal to its links);
//! * AI block: when it is the loadable version (13), every character, force and unit it names
//!   exists (else the original's post-load fixup fails, SAVE_COMPAT.md §5).
//!
//! The pathfinder and AI-block rules only guard the original game's loader; since compatibility
//! with the original is out of scope they come back in [`Report::informational`], not in
//! [`Report::violations`] (`original_loader_only`).
//!
//! [`Report::commanders_without_obstacle`] lists commanders the pathfinder has no obstacle for:
//! not a rule violation (CONFIRMED accepted by the original, which keeps such commanders without
//! one over several End Turns, SAVE_COMPAT.md §17), reported for the user tests.

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

/// The result of [`check`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// Broken rules that matter to OUR game's own load (or to the save's plain consistency), one
    /// line each.
    pub violations: Vec<String>,
    /// Broken rules that only guard the ORIGINAL game's loader: the pathfinder's obstacle and
    /// boundary-manager bookkeeping, the loader replay (`grid_load_check`) and the AI block's
    /// graph. Our loader reads neither `CAMPAIGN_PATHFINDER` nor `CAI_INTERFACE` (it builds the
    /// grid from the map and plays the AI itself), so these are information since compatibility
    /// with the original is out of scope (2026-10-04, the user's decision).
    pub informational: Vec<String>,
    /// Force commanders without a pathfinder obstacle.
    pub commanders_without_obstacle: Vec<u32>,
    /// True when the AI block is not the loadable version (the original rebuilds it).
    pub ai_block_rebuilt: bool,
    /// Regions with more than one `GARRISON_RESIDENCE` not of the region's owner (information:
    /// the original's own fresh saves never have one, our writer hands them over on capture, but
    /// the original loads and keeps them: NR-C8b, SAVE_COMPAT.md §18).
    pub regions_with_foreign_residences: Vec<String>,
}

/// See [`Report::regions_with_foreign_residences`].
pub fn regions_with_foreign_residences(esf: &EsfFile) -> Vec<String> {
    let mut out = Vec::new();
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return out };
    for reg in world.child("REGION_MANAGER").and_then(|m| m.record_array("REGIONS_ARRAY")).into_iter().flat_map(|a| a.records()) {
        let owner = reg.get_u32(20);
        let mut foreign = 0;
        reg.walk(&mut |x: &EsfRecord| {
            if x.name == "GARRISON_RESIDENCE" && x.get_u32(0) != owner {
                foreign += 1;
            }
        });
        if foreign > 1 {
            out.push(format!("{}: {foreign} residences not of its owner", reg.get_str(0).unwrap_or("")));
        }
    }
    out
}

/// The `CAI_INTERFACE` version the original loads (`FUN_00851540` returns 13; CONFIRMED).
pub const LOADABLE_CAI_VERSION: u8 = 13;

struct Unit {
    id: i32,
    character: u32,
}

struct Force {
    id: u32,
    faction: i32,
    navy: bool,
    commander: u32,
    residence: u32,
    units: Vec<Unit>,
}

struct Char {
    faction: i32,
    force: u32,
    unit: u32,
    details_id: Option<u32>,
    /// `CHARACTER` #8: the government post id he holds, 0 = none.
    post: u32,
}

/// Checks a save tree (`CAMPAIGN_SAVE_GAME`).
pub fn check(esf: &EsfFile) -> Report {
    let mut r = Report { regions_with_foreign_residences: regions_with_foreign_residences(esf), ..Report::default() };
    let mut bad = |s: String| r.violations.push(s);
    let root = &esf.root;
    let Some(env) = root.child("CAMPAIGN_ENV") else {
        bad("no CAMPAIGN_ENV".into());
        return r;
    };
    // The player.
    let header = root.child("SAVE_GAME_HEADER").and_then(|h| h.get_str(0)).unwrap_or_default().to_string();
    let humans: Vec<String> = crate::save::player_flags(esf).into_iter().filter(|(_, h)| *h).map(|(k, _)| k).collect();
    if humans != [header.clone()] {
        bad(format!("human flags {humans:?}, header faction {header:?}"));
    }
    // The faction's own copy, the one the game plays by (SAVE_COMPAT.md §2).
    let faction_humans: Vec<String> = crate::save::faction_player_flags(esf).into_iter().filter(|(_, h)| *h).map(|(k, _)| k).collect();
    if faction_humans != [header.clone()] {
        bad(format!("faction human flags {faction_humans:?}, header faction {header:?}"));
    }
    // The human's victory conditions must not count as met at once (SAVE_COMPAT.md §17).
    for f in crate::victory::met_at_once(esf) {
        bad(format!("faction {f}: victory conditions met at once (type 5 without spa_france)"));
    }
    let Some(model) = env.child("CAMPAIGN_MODEL") else {
        bad("no CAMPAIGN_MODEL".into());
        return r;
    };
    let Some(world) = model.child("WORLD") else {
        bad("no WORLD".into());
        return r;
    };

    // Regiment and ship name lists: a name is flagged exactly when one unit of the faction carries
    // it, and no listed name is carried twice (SAVE_COMPAT.md §20, CONFIRMED in every vanilla
    // original save and start position).
    for f in world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        let mut carried: BTreeMap<String, usize> = BTreeMap::new();
        f.walk(&mut |r: &EsfRecord| {
            if r.name == "UNIT"
                && let Some(n) = r.child("CAMPAIGN_LOCALISATION").and_then(|c| c.get_str(0)).filter(|n| !n.is_empty())
            {
                *carried.entry(n.to_string()).or_default() += 1;
            }
        });
        let mut wrong = 0;
        f.walk(&mut |r: &EsfRecord| {
            if r.name == "UNIT_CLASS_NAME_ALLOCATOR" {
                for it in r.record_array("UNIT_CLASS_NAMES_LIST").into_iter().flat_map(|a| a.items.iter()) {
                    let n = it.first().and_then(EsfNode::as_record).and_then(|c| c.get_str(0)).unwrap_or("");
                    let c = carried.get(n).copied().unwrap_or(0);
                    if it.get(1).and_then(EsfNode::as_bool) != Some(c > 0) || c > 1 {
                        wrong += 1;
                    }
                }
            }
        });
        if wrong > 0 {
            bad(format!("faction {key}: {wrong} unit name list entries whose in-use flag does not match the units"));
        }
    }

    // Collect characters, forces, units.
    let mut chars: BTreeMap<u32, Char> = BTreeMap::new();
    let mut forces: BTreeMap<u32, Force> = BTreeMap::new();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut dup = |kind: &str, id: u32, bad: &mut dyn FnMut(String)| {
        if !seen.insert(id) {
            bad(format!("duplicate id {id} ({kind})"));
        }
    };
    let mut factions: Vec<&EsfRecord> = world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    factions.extend(world.child("REBEL_FACTION").and_then(|f| f.child("FACTION")));
    let mut post_holders: Vec<(u32, u32)> = Vec::new();
    for f in &factions {
        let fid = f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32).unwrap_or(0);
        for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
            let id = c.get_i32(2).unwrap_or(0) as u32;
            dup("character", id, &mut bad);
            let details = c.children.get(1).and_then(EsfNode::as_record);
            let details_id = details.and_then(|d| d.get_u32(10));
            // Traits and ancillaries (SAVE_COMPAT.md §23, CONFIRMED in every original save): each
            // key once per character, non-empty, trait points above 0, at most 3 ancillaries
            // (`max_ancillaries`).
            if let Some(d) = details {
                let traits: Vec<(String, i32)> = d
                    .child("TRAITS")
                    .and_then(|t| t.record_array("TRAIT"))
                    .into_iter()
                    .flat_map(|a| a.items.iter())
                    .map(|it| (it.first().and_then(EsfNode::as_str).unwrap_or("").to_string(), it.get(1).and_then(EsfNode::as_i32).unwrap_or(0)))
                    .collect();
                let anc: Vec<String> = d.record_array("AgentAncillaries").into_iter().flat_map(|a| a.items.iter()).map(|it| it.first().and_then(EsfNode::as_str).unwrap_or("").to_string()).collect();
                let keys: BTreeSet<&str> = traits.iter().map(|t| t.0.as_str()).collect();
                let akeys: BTreeSet<&str> = anc.iter().map(String::as_str).collect();
                if keys.len() != traits.len() || traits.iter().any(|t| t.0.is_empty() || t.1 <= 0) {
                    bad(format!("character {id}: traits repeated, empty or without points: {traits:?}"));
                }
                if akeys.len() != anc.len() || anc.iter().any(String::is_empty) || anc.len() > 3 {
                    bad(format!("character {id}: ancillaries repeated, empty or more than 3: {anc:?}"));
                }
            }
            chars.insert(id, Char { faction: fid, force: c.get_u32(4).unwrap_or(0), unit: c.get_u32(5).unwrap_or(0), details_id, post: c.get_u32(8).unwrap_or(0) });
        }
        for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
            let Some(mf) = a.child("MILITARY_FORCE") else {
                bad(format!("{} without MILITARY_FORCE", a.name));
                continue;
            };
            let id = mf.get_u32(0).unwrap_or(0);
            dup("force", id, &mut bad);
            let mut units = Vec::new();
            for w in a.record_array("UNITS_ARRAY").into_iter().flat_map(|x| x.records()) {
                let Some(u) = w.child("UNIT") else { continue };
                let uid = u.get_i32(4).unwrap_or(0);
                dup("unit", uid as u32, &mut bad);
                units.push(Unit { id: uid, character: u.get_u32(10).unwrap_or(0) });
            }
            let navy = a.name == "NAVY";
            forces.insert(
                id,
                Force { id, faction: fid, navy, commander: mf.get_u32(1).unwrap_or(0), residence: if navy { 0 } else { a.get_u32(5).unwrap_or(0) }, units },
            );
        }
        if let Some(g) = f.child("GOVERNMENT") {
            for p in g.record_array("POSTS_ARRAY").into_iter().flat_map(|a| a.records()) {
                if let Some(h) = p.get_u32(2).filter(|&h| h != 0) {
                    post_holders.push((h, p.get_i32(0).unwrap_or(0) as u32));
                }
            }
        }
    }
    let units: BTreeMap<i32, (u32, u32)> = forces.values().flat_map(|f| f.units.iter().map(move |u| (u.id, (f.id, u.character)))).collect();

    for (&id, c) in &chars {
        if c.details_id.is_some_and(|d| d != id) {
            bad(format!("character {id}: CHARACTER_DETAILS #10 = {:?}", c.details_id));
        }
        if c.force != 0 && forces.get(&c.force).is_none_or(|f| f.commander != id) {
            bad(format!("character {id}: #4 = {} is not a force it commands", c.force));
        }
        if c.unit != 0 && units.get(&(c.unit as i32)).is_none_or(|u| u.1 != id) {
            bad(format!("character {id}: #5 = {} is not a unit attached to it", c.unit));
        }
    }
    for f in forces.values() {
        if f.units.is_empty() {
            bad(format!("force {} has no units", f.id));
        }
        match chars.get(&f.commander) {
            None => bad(format!("force {} has no commander (#1 = {})", f.id, f.commander)),
            Some(c) => {
                if c.faction != f.faction {
                    bad(format!("force {}: commander {} is of another faction", f.id, f.commander));
                }
                if c.force != f.id {
                    bad(format!("force {}: commander {} has #4 = {}", f.id, f.commander, c.force));
                }
                // The commander rides on one of the force's own units (SAVE_COMPAT.md §23,
                // CONFIRMED in every vanilla save kept): a successor who takes over keeps his unit.
                if !f.units.iter().any(|u| u.character == f.commander && u.id as u32 == c.unit) {
                    bad(format!("force {}: commander {} is on no unit of the force (#5 = {})", f.id, f.commander, c.unit));
                }
            }
        }
        for u in &f.units {
            if u.character != 0 {
                match chars.get(&u.character) {
                    None => bad(format!("unit {} of force {}: #10 = {} is no character", u.id, f.id, u.character)),
                    Some(c) if c.unit != u.id as u32 => bad(format!("unit {}: character {} has #5 = {}", u.id, u.character, c.unit)),
                    Some(c) if c.faction != f.faction => bad(format!("unit {}: character {} of another faction", u.id, u.character)),
                    Some(_) => {}
                }
            }
        }
    }
    // Posts and their holders point at each other (SAVE_COMPAT.md §23, CONFIRMED in every vanilla
    // save kept): `CHARACTER_POST` #2 = the holder, whose `CHARACTER` #8 = the post id; a character
    // with #8 != 0 holds that post.
    for &(h, post) in &post_holders {
        match chars.get(&h) {
            None => bad(format!("government post held by missing character {h}")),
            Some(c) if c.post != post => bad(format!("post {post}: holder {h} has #8 = {}", c.post)),
            Some(_) => {}
        }
    }
    for (&id, c) in &chars {
        if c.post != 0 && !post_holders.contains(&(id, c.post)) {
            bad(format!("character {id}: #8 = {} is not a post he holds", c.post));
        }
    }
    // Embarked armies: ARMY #7 = the carrying navy, NAVY #4 = the carried army, 0 = none; the two
    // ends agree (CONFIRMED storage, the ports worker's loaders 0x00870FD0 / 0x008822F0).
    let mut army_navy: BTreeMap<u32, u32> = BTreeMap::new();
    let mut navy_army: BTreeMap<u32, u32> = BTreeMap::new();
    for f in &factions {
        for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
            let id = a.child("MILITARY_FORCE").and_then(|m| m.get_u32(0)).unwrap_or(0);
            let (field, map) = if a.name == "NAVY" { (4, &mut navy_army) } else { (7, &mut army_navy) };
            if let Some(v) = a.get_u32(field).filter(|&v| v != 0) {
                map.insert(id, v);
            }
        }
    }
    for (&army, &navy) in &army_navy {
        if navy_army.get(&navy) != Some(&army) {
            bad(format!("army {army}: ARMY #7 = {navy}, but that navy's #4 is {:?}", navy_army.get(&navy)));
        }
    }
    for (&navy, &army) in &navy_army {
        if army_navy.get(&army) != Some(&navy) {
            bad(format!("navy {navy}: NAVY #4 = {army}, but that army's #7 is {:?}", army_navy.get(&army)));
        }
    }
    // The other places the original stores character and force ids (`save_audit refsites` on its
    // saves): recruitment pools (candidates without a force), characters with a force (#6),
    // characters attached to a force (`MILITARY_FORCE` #2) or present in a residence (#14), the
    // navy in a port, trade raiders, pending battle participants.
    for f in &factions {
        let fid = f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32).unwrap_or(0);
        if let Some(m) = f.child("CHARACTER_RECRUITMENT_MANAGER") {
            for pool in m.children.iter().filter_map(EsfNode::as_record) {
                for &c in pool.get(0).and_then(EsfNode::as_u32_array).unwrap_or_default() {
                    match chars.get(&c) {
                        None => bad(format!("{} names missing character {c}", pool.name)),
                        Some(ch) if ch.faction != fid || ch.force != 0 => bad(format!("{}: character {c} is of another faction or commands a force", pool.name)),
                        Some(_) => {}
                    }
                }
            }
        }
        for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
            if let Some(v) = c.get_u32(6).filter(|&v| v != 0)
                && !forces.contains_key(&v)
            {
                bad(format!("character {}: #6 = {v} is no force", c.get_i32(2).unwrap_or(0)));
            }
        }
        for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
            let Some(mf) = a.child("MILITARY_FORCE") else { continue };
            let id = mf.get_u32(0).unwrap_or(0);
            if a.name == "ARMY" && a.get_i32(4).map(|v| v as u32) != Some(id) {
                bad(format!("army {id}: #4 = {:?} is not its id", a.get_i32(4)));
            }
            for &c in mf.get(2).and_then(EsfNode::as_u32_array).unwrap_or_default() {
                if !chars.contains_key(&c) {
                    bad(format!("force {id}: MILITARY_FORCE #2 names missing character {c}"));
                }
            }
        }
    }
    let mut refs: Vec<String> = Vec::new();
    world.walk(&mut |x| match x.name.as_str() {
        "SIEGEABLE_GARRISON_RESIDENCE" => {
            for &c in x.get(14).and_then(EsfNode::as_u32_array).unwrap_or_default() {
                if !chars.contains_key(&c) {
                    refs.push(format!("residence {}: #14 names missing character {c}", x.get_u32(1).unwrap_or(0)));
                }
            }
        }
        "PORT_GARRISON_MANAGER" => {
            if let Some(v) = x.get_u32(0).filter(|&v| v != 0)
                && forces.get(&v).is_none_or(|f| !f.navy)
            {
                refs.push(format!("PORT_GARRISON_MANAGER #0 = {v} is no navy"));
            }
        }
        _ => {}
    });
    model.walk(&mut |x| match x.name.as_str() {
        "COMMERCE_RAIDS" => {
            if let Some(v) = x.get_u32(0).filter(|&v| v != 0)
                && !chars.contains_key(&v)
            {
                refs.push(format!("COMMERCE_RAIDS names missing character {v}"));
            }
        }
        "PENDING_BATTLE_PARTICIPANT" => {
            if let Some(v) = x.get_i32(0).filter(|&v| v != 0)
                && !forces.contains_key(&(v as u32))
            {
                refs.push(format!("PENDING_BATTLE_PARTICIPANT names missing force {v}"));
            }
        }
        _ => {}
    });
    for s in refs {
        bad(s);
    }

    // Garrisons.
    let mut residences: BTreeMap<u32, u32> = BTreeMap::new();
    if let Some(rm) = world.child("REGION_MANAGER") {
        rm.walk(&mut |x| {
            if x.name == "SIEGEABLE_GARRISON_RESIDENCE" {
                residences.insert(x.get_u32(1).unwrap_or(0), x.get_u32(12).unwrap_or(0));
            }
        });
    }
    for (&res, &force) in &residences {
        if force != 0 && forces.get(&force).is_none_or(|f| f.navy || f.residence != res) {
            bad(format!("residence {res}: #12 = {force} is not an army garrisoned there"));
        }
    }
    for f in forces.values().filter(|f| f.residence != 0) {
        if residences.get(&f.residence) != Some(&f.id) {
            bad(format!("army {}: #5 = {} is not a residence that holds it", f.id, f.residence));
        }
    }

    // Pathfinder obstacles.
    let grid = model
        .child("CAMPAIGN_PATHFINDER")
        .and_then(|p| p.record_array("PATHFINDING_GRID"))
        .and_then(|a| a.items.first());
    let mut owners: BTreeSet<u32> = BTreeSet::new();
    if let Some(grid) = grid {
        // Run-time pieces (#1: n, n points, use count; SAVE_COMPAT.md §22, CONFIRMED in all 11
        // vanilla saves kept, ~70 000 pieces): a used piece has 3+ points and a positive signed
        // area; only freed pieces (use count 0) have fewer than 3 points.
        if let Some(p) = grid.get(1).and_then(EsfNode::as_u32_array) {
            let (mut j, mut k, mut bad_pieces) = (0, 0, Vec::new());
            while j < p.len() {
                let n = p[j] as usize;
                let Some(used) = p.get(j + 1 + 2 * n).copied() else { break };
                let pt = |i: usize| (p[j + 1 + 2 * i] as i32 as i128, p[j + 2 + 2 * i] as i32 as i128);
                let a2: i128 = (0..n).map(|i| pt(i).0 * pt((i + 1) % n).1 - pt((i + 1) % n).0 * pt(i).1).sum();
                if used > 0 && (n < 3 || a2 <= 0) {
                    bad_pieces.push(k);
                }
                j += 2 + 2 * n;
                k += 1;
            }
            if !bad_pieces.is_empty() {
                bad(format!("pathfinder: used pieces with under 3 points or not counter-clockwise: {bad_pieces:?}"));
            }
        }
        // Grid nodes (SAVE_COMPAT.md §18, CONFIRMED in every original save): no node with empty
        // lists, the two lists of a node equally long, and the u32 array after the nodes a
        // (cell, node index) map with one entry per node.
        if let Some(pos) = grid.iter().position(|n| matches!(n, EsfNode::RecordArray(a) if a.name == "OBSTACLE_BASE_GRID_NODE"))
            && let Some(nodes) = grid[pos].as_record_array()
        {
            let (mut empty, mut uneven) = (0, 0);
            for it in &nodes.items {
                let lens: Vec<usize> = it.iter().filter_map(EsfNode::as_record_array).map(|a| a.items.len()).collect();
                empty += usize::from(!lens.is_empty() && lens.iter().all(|&l| l == 0));
                uneven += usize::from(lens.windows(2).any(|w| w[0] != w[1]));
            }
            if empty > 0 {
                bad(format!("pathfinder: {empty} grid nodes with empty lists"));
            }
            if uneven > 0 {
                bad(format!("pathfinder: {uneven} grid nodes with lists of different lengths"));
            }
            let map = grid.get(pos + 1).and_then(EsfNode::as_u32_array).unwrap_or_default();
            let idx: BTreeSet<u32> = map.chunks(2).filter_map(|p| p.get(1).copied()).collect();
            let n = nodes.items.len();
            if map.len() != 2 * n || idx.len() != n || idx.last().is_some_and(|&l| l as usize != n - 1) {
                bad(format!("pathfinder: the cell map ({} values) does not index the {n} grid nodes", map.len()));
            }
            // The boundary manager is a set of distinct, non-empty pair lists; every grid row's
            // list is in it (the loader looks rows up there, 0x00B53800); list 2 of a node holds
            // the same lists as list 1 (a permutation); a list-1 row of piece P names exactly the
            // obstacles whose BOUNDARIES list P (SAVE_COMPAT.md §19, CONFIRMED in every original save).
            let mut mgr: BTreeSet<Vec<u32>> = BTreeSet::new();
            let mut dups = 0;
            if let Some(m) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARY_MANAGER")) {
                for b in m.record_array("OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()) {
                    let v = b.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().to_vec();
                    if v.is_empty() || !mgr.insert(v) {
                        dups += 1;
                    }
                }
            }
            if dups > 0 {
                bad(format!("pathfinder: {dups} boundary-manager lists empty or repeated"));
            }
            let (mut missing, mut not_perm) = (0, 0);
            let mut row_owners: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
            let mut row_pairs_all: Vec<(u32, u32)> = Vec::new();
            for it in &nodes.items {
                let ls: Vec<&ntw_formats::esf::EsfRecordArray> = it.iter().filter_map(EsfNode::as_record_array).collect();
                let get = |l: &ntw_formats::esf::EsfRecordArray| -> Vec<Vec<u32>> {
                    l.items.iter().map(|r| r.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().to_vec()).collect()
                };
                let all: Vec<Vec<Vec<u32>>> = ls.iter().map(|l| get(l)).collect();
                missing += all.iter().flatten().filter(|v| !mgr.contains(*v)).count();
                row_pairs_all.extend(all.iter().flatten().flat_map(|v| v.chunks(2).filter(|p| p.len() == 2 && p[0] & 3 == 2).map(|p| (p[0] & !3, p[1])).collect::<Vec<_>>()));
                if let [a, b] = &all[..] {
                    let (mut a, mut b) = (a.clone(), b.clone());
                    a.sort();
                    b.sort();
                    not_perm += usize::from(a != b);
                }
                if let Some(l1) = ls.first() {
                    for r in &l1.items {
                        let piece = r.first().and_then(EsfNode::as_u32).unwrap_or(0);
                        let pairs = r.iter().find_map(EsfNode::as_u32_array).unwrap_or_default();
                        row_owners.entry(piece).or_default().extend(pairs.chunks(2).filter(|p| p.len() == 2).map(|p| (p[0] & !3, p[1])));
                    }
                }
            }
            if missing > 0 {
                bad(format!("pathfinder: {missing} grid-row pair lists not in the boundary manager"));
            }
            if not_perm > 0 {
                bad(format!("pathfinder: {not_perm} grid nodes whose list 2 is not a permutation of list 1"));
            }
            let mut listing: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
            if let Some(ol) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS")) {
                for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
                    let (Some(o), Some(id)) = (it.first().and_then(EsfNode::as_record), it.get(1).and_then(EsfNode::as_u32)) else { continue };
                    for (k, b) in o.record_array("BOUNDARIES").into_iter().flat_map(|a| a.items.iter()).enumerate() {
                        for &v in b.iter().filter_map(EsfNode::as_u32_array).flatten().filter(|v| *v & 0x8000_0000 == 0) {
                            listing.entry(v).or_default().insert((id, k as u32));
                        }
                    }
                }
            }
            let wrong = listing.iter().filter(|(p, o)| row_owners.get(p) != Some(o)).count();
            if wrong > 0 {
                bad(format!("pathfinder: {wrong} obstacle boundary pieces whose grid row does not name exactly the obstacles listing them"));
            }
            // The obstacles' managed slots are exactly the (owner, slot) pairs of the manager, and
            // the non-empty BOUNDARIES slots exactly those of the grid rows (SAVE_COMPAT.md §19,
            // CONFIRMED in every original save, `save_audit obstacle_consistency`).
            let mut managed: BTreeSet<(u32, u32)> = BTreeSet::new();
            let mut nonempty: BTreeSet<(u32, u32)> = BTreeSet::new();
            if let Some(ol) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS")) {
                for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
                    let (Some(o), Some(id)) = (it.first().and_then(EsfNode::as_record), it.get(1).and_then(EsfNode::as_u32)) else { continue };
                    for (k, m) in o.record_array("MANAGED_OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()).enumerate() {
                        if m.first().and_then(EsfNode::as_bool) == Some(true) {
                            managed.insert((id, k as u32));
                        }
                    }
                    for (k, b) in o.record_array("BOUNDARIES").into_iter().flat_map(|a| a.items.iter()).enumerate() {
                        if b.iter().find_map(EsfNode::as_u32_array).is_some_and(|v| !v.is_empty()) {
                            nonempty.insert((id, k as u32));
                        }
                    }
                }
            }
            let in_mgr: BTreeSet<(u32, u32)> = mgr.iter().flat_map(|v| v.chunks(2).filter(|p| p.len() == 2 && p[0] & 3 == 2).map(|p| (p[0] & !3, p[1])).collect::<Vec<_>>()).collect();
            let in_grid: BTreeSet<(u32, u32)> = row_pairs_all.iter().copied().collect();
            if managed != in_mgr {
                bad(format!("pathfinder: managed obstacle slots ({}) differ from the boundary manager's ({})", managed.len(), in_mgr.len()));
            }
            if nonempty != in_grid {
                bad(format!("pathfinder: non-empty obstacle boundary slots ({}) differ from the grid's ({})", nonempty.len(), in_grid.len()));
            }
        }
        // The loader replay (`grid_load_check`, SAVE_COMPAT.md §29): where the original would
        // follow a bad pointer, and the version / row / piece-count patterns it never writes.
        let g = crate::grid_load_check::check(grid);
        for f in g.faults {
            bad(format!("pathfinder loader: {f}"));
        }
        for b in g.rule_breaks {
            bad(format!("pathfinder: {b}"));
        }
        let lists = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"));
        if let Some(lists) = lists {
            let ids: Vec<u32> = lists.get(2).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
            let listed: Vec<u32> = lists
                .record_array("CHARACTER_OBSTACLE")
                .into_iter()
                .flat_map(|a| a.items.iter())
                .map(|it| it.iter().find_map(EsfNode::as_u32).unwrap_or(0))
                .collect();
            if ids != listed {
                bad(format!("OBSTACLE_LISTS #2 ({} ids) differs from the CHARACTER_OBSTACLE owners ({})", ids.len(), listed.len()));
            }
            for &o in &listed {
                if !chars.contains_key(&o) {
                    bad(format!("character obstacle of missing character {o}"));
                }
                owners.insert(o);
            }
        }
        let mut unknown = BTreeSet::new();
        for n in grid {
            pair_owners(n, &mut |o| {
                if !owners.contains(&o) {
                    unknown.insert(o);
                }
            });
        }
        for o in unknown {
            bad(format!("pathfinder pair names obstacle {o}, which does not exist"));
        }
    }
    r.commanders_without_obstacle = forces.values().map(|f| f.commander).filter(|c| *c != 0 && !owners.contains(c)).collect();

    // The AI managers: the human faction's is HUMAN, no other faction's (SAVE_COMPAT.md §2).
    let human_id = factions
        .iter()
        .find(|f| f.child("CAMPAIGN_PLAYER_SETUP").and_then(|p| p.get_str(2)) == Some(header.as_str()))
        .and_then(|f| f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32))
        .map(|v| v as u32);
    for m in crate::cai::manager_types(esf) {
        let want_human = Some(m.faction) == human_id;
        for (place, t) in [("CAI_INTERFACE #29", m.listed), ("CAI_FACTION_MANAGER", m.manager)] {
            let is_human = t == Some(crate::cai::MANAGER_HUMAN);
            if t.is_some() && is_human != want_human {
                bad(format!("faction {}: AI manager type {t:?} in {place} (human: {want_human})", m.faction));
            }
        }
    }

    // Shrouds: three trees on one grid (CONFIRMED on the vanilla saves kept and every startpos;
    // CHARACTERS_FIDELITY.md §10). Not a rule: "every visible cell explored" holds in the saves
    // but not in the tutorial startpos (tut_france, cell (406, 175)).
    if let Some(fa) = world.record_array("FACTION_ARRAY") {
        for f in fa.records() {
            let Some(sh) = f.child("CAMPAIGN_SHROUD") else { continue };
            let key = f.get_str(9).unwrap_or_default();
            let trees: Vec<&EsfRecord> = sh.children_named("QUAD_TREE_BIT_ARRAY").collect();
            let heads: BTreeSet<(Option<u32>, Option<u32>, Option<u32>)> = trees.iter().map(|q| (q.get_u32(0), q.get_u32(1), q.get_u32(2))).collect();
            if trees.len() != 3 || heads.len() != 1 {
                bad(format!("faction {key}: shroud with {} trees on {} grids", trees.len(), heads.len()));
            }
        }
    }

    // The AI block.
    if let Some(cai) = model.child("CAI_INTERFACE") {
        if cai.version == LOADABLE_CAI_VERSION {
            let mut missing = BTreeSet::new();
            cai.walk(&mut |x| {
                let (idx, kind) = match x.name.as_str() {
                    "CAI_CHARACTER" => (3, 0),
                    "CAI_RESOURCE_MOBILE" => (10, 1),
                    "CAI_UNIT" => (1, 2),
                    _ => return,
                };
                let Some(v) = x.get_u32(idx).filter(|&v| v != 0) else { return };
                let ok = match kind {
                    0 => chars.contains_key(&v),
                    1 => forces.contains_key(&v),
                    _ => units.contains_key(&(v as i32)),
                };
                if !ok {
                    missing.insert((x.name.clone(), v));
                }
            });
            for (n, v) in missing {
                bad(format!("AI block {n} names missing object {v}"));
            }
            // The block's own graph and its mirrors of the world (SAVE_COMPAT.md §10).
            for l in crate::cai::check_block(cai, 10) {
                bad(l);
            }
            for l in crate::cai_world::check(esf, 10) {
                bad(l);
            }
        } else {
            // Any other version makes the original rebuild its AI without the director pool:
            // the INFERRED cause of the round-1 crash (SAVE_COMPAT.md §5).
            r.ai_block_rebuilt = true;
            bad(format!("AI block version {} is not the loadable {LOADABLE_CAI_VERSION}", cai.version));
        }
    }
    let (info, ours): (Vec<String>, Vec<String>) = r.violations.drain(..).partition(|l| original_loader_only(l));
    r.violations = ours;
    r.informational = info;
    r
}

/// Whether a rule line guards only the original game's loader (see [`Report::informational`]).
pub fn original_loader_only(line: &str) -> bool {
    ["pathfinder", "OBSTACLE_LISTS", "character obstacle of missing", "AI block"].iter().any(|p| line.starts_with(p))
}

impl Report {
    /// Every broken rule, ours first, then the original-loader ones.
    pub fn all_lines(&self) -> Vec<String> {
        self.violations.iter().chain(&self.informational).cloned().collect()
    }
}

/// Calls `f` with the owner of every obstacle pair (u32 arrays whose even elements carry tag 2)
/// inside the boundary manager, the grid nodes and the obstacles' own lists.
fn pair_owners(n: &EsfNode, f: &mut dyn FnMut(u32)) {
    fn rec(r: &EsfRecord, inside: bool, f: &mut dyn FnMut(u32)) {
        let inside = inside || r.name == "OBSTACLE_BOUNDARY_MANAGER";
        for c in &r.children {
            node(c, inside, f);
        }
    }
    fn node(n: &EsfNode, inside: bool, f: &mut dyn FnMut(u32)) {
        match n {
            EsfNode::Record(r) => rec(r, inside, f),
            EsfNode::RecordArray(a) => {
                let inside = inside || matches!(a.name.as_str(), "OBSTACLE_BASE_GRID_NODE" | "MANAGED_OBSTACLE_BOUNDARY" | "OBSTACLE_BOUNDARY");
                for it in &a.items {
                    for x in it {
                        node(x, inside, f);
                    }
                }
            }
            EsfNode::U32Array(v) if inside && v.len() % 2 == 0 && !v.is_empty() && v.chunks(2).all(|p| p[0] & 3 == 2 && p[0] < 0x8000_0000) => {
                for p in v.chunks(2) {
                    f(p[0] & !3);
                }
            }
            _ => {}
        }
    }
    node(n, false, f);
}

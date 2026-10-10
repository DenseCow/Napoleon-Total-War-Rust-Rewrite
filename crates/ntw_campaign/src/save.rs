//! Writing the campaign back to an ESF save (`CAMPAIGN_SAVE_GAME`).
//!
//! A save is the start position (or the save it was loaded from) with the modelled fields
//! patched in place. Everything the model does not understand is kept byte for byte (W3 §9.1
//! "lossless passthrough"), so the file keeps all the blocks the original needs (AI, trade,
//! shroud, ...). [`write_save`] takes that source tree and the model and returns the new tree;
//! `EsfFile::to_bytes` (the existing writer) serialises it.
//!
//! What is written (field positions: `analysis/worker3/STARTPOS_LAYOUT.md`):
//! | model | ESF | tag |
//! |---|---|---|
//! | root | `CAMPAIGN_STARTPOS` → `CAMPAIGN_SAVE_GAME`, `CAMPAIGN_PREOPEN_MAP_INFO` dropped | CONFIRMED difference (W3 §4) |
//! | header | `SAVE_GAME_HEADER` #0 faction, #2 turn, #3 year, `DATE` | CONFIRMED fields |
//! | calendar, RNG | `CAMPAIGN_CALENDAR` #0..#3 + `DATE`, `RandSeed` #0 | CONFIRMED |
//! | treasury | `FACTION_ECONOMICS` #1 | CONFIRMED position |
//! | characters | `LOCOMOTABLE` #0/#1 position, #8 base and #9 current action points; removed characters' items are dropped | CONFIRMED positions |
//! | forces | `MILITARY_FORCE` #0/#1, `UNITS_ARRAY` rebuilt from the model (existing unit items kept and patched: `UNIT` #5 men, #6 max men; new units cloned from a template unit item); destroyed forces dropped; new forces cloned from a template `ARMY`/`NAVY` item | CONFIRMED positions; cloning PROVISIONAL |
//! | regions | `REGION` #20 owner, garrison residence owner, slot buildings, road building, `BUILDING_CONSTRUCTION_ITEM`s (source items kept, repair items kept), the recruitment queues (region and port managers; source items kept), the economy fields (`write_region_economy`), the population state and religion shares (`write_region_population`) | CONFIRMED layouts from the user's saves |
//! | taxes | every `GOVERNORSHIP_TAXES` | CONFIRMED position (CAMPAIGN_DATA.md §3) |
//! | stances | `DIPLOMACY_RELATIONSHIP` #4 (old stance to #20) | #4 CONFIRMED, #20 INFERRED |
//! | script slots | `EPISODIC_RESTRICTIONS/LUA[]` ([`write_save_with`]) | CONFIRMED (CAMPAIGN_DATA.md §4) |
//! | links | `CHARACTER` #4 (force commanded) / #5 (unit attached), `UNIT` #10, `ARMY` #5 and residence #12 (garrisons), new colonels / captains cloned from a template character | CONFIRMED links (SAVE_COMPAT.md §4); cloned names PROVISIONAL |
//! | other id sites | recruitment pools, `CHARACTER` #6, `MILITARY_FORCE` #2, residence #14, `PORT_GARRISON_MANAGER` #0, `COMMERCE_RAIDS`: cleared of objects the model no longer has | sites CONFIRMED (`save_audit refsites`) |
//! | AI block | `CAI_INTERFACE` kept while the world's objects are unchanged, else version 12 (the original rebuilds it) | CONFIRMED loader rule, SAVE_COMPAT.md §5 |
//! | pathfinder | character obstacles of characters gone removed, new garrison commanders given a copy (`obstacles.rs`) | SAVE_COMPAT.md §6 |
//!
//! An unchanged model wrote every user save back byte for byte (measured once, see SAVE_COMPAT.md; the test was removed), and
//! every save written passes `save_check` (tests). Not written yet (kept as loaded): forces'
//! positions inside settlements, character details and government posts (the model does not change
//! them), `PENDING_BATTLE`.

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord, EsfRecordArray};
use ntw_sim::campaign::{BuildingRef, CampaignModel, CampaignUnit, Faction, MilitaryForce, Region, SlotRef};

use crate::{SAVE_ROOT, STARTPOS_ROOT};

/// Why a save could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The source tree is not a start position or save.
    WrongRoot(String),
    /// A record the writer needs is missing from the source tree.
    Missing(&'static str),
    /// The model has something to write but the source tree has no template to clone for it.
    NoTemplate(&'static str),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::WrongRoot(r) => write!(f, "root record {r} is not a campaign"),
            SaveError::Missing(what) => write!(f, "the source has no {what}"),
            SaveError::NoTemplate(what) => write!(f, "no template {what} to clone"),
        }
    }
}

impl std::error::Error for SaveError {}

fn child_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    r.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(b) if b.name == name => Some(&mut **b),
        _ => None,
    })
}

fn array_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecordArray> {
    r.children.iter_mut().find_map(|c| match c {
        EsfNode::RecordArray(b) if b.name == name => Some(&mut **b),
        _ => None,
    })
}

/// `FACTION_ECONOMICS` #0 (`history`) from the model's records, oldest first, each an
/// `ECONOMICS_DATA` of the version the file already uses (5 in every save) with the categories in
/// the saver's groups (`0x00BD46E0` → `0x00B985B0`, CONFIRMED layout).
fn write_economy_history(e: &mut EsfRecord, history: &[ntw_sim::campaign::EconomyRecord]) {
    let Some(arr) = array_mut(e, "history") else {
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::warn!("save: a FACTION_ECONOMICS has no `history` array; its economics history is not written (logged once)");
        }
        return;
    };
    let version = arr.items.iter().find_map(|i| match i.first() {
        Some(EsfNode::Record(r)) => Some(r.version),
        _ => None,
    });
    let version = version.unwrap_or(5);
    arr.items = history
        .iter()
        .map(|r| {
            let mut at = 0;
            let children = crate::world::ECONOMICS_GROUPS
                .iter()
                .map(|&len| {
                    let g = EsfNode::I32Array(r[at..at + len].to_vec());
                    at += len;
                    g
                })
                .collect();
            vec![EsfNode::Record(Box::new(EsfRecord { name: "ECONOMICS_DATA".into(), version, children }))]
        })
        .collect();
}

fn first_rec_mut(item: &mut [EsfNode]) -> Option<&mut EsfRecord> {
    match item.first_mut() {
        Some(EsfNode::Record(b)) => Some(&mut **b),
        _ => None,
    }
}

fn first_rec(item: &[EsfNode]) -> Option<&EsfRecord> {
    item.first().and_then(EsfNode::as_record)
}

/// Sets child `i` if it holds a value of the same ESF type (a wrong type means the layout is not
/// what we expect, and the value is left alone).
fn set(r: &mut EsfRecord, i: usize, v: EsfNode) {
    if let Some(slot) = r.children.get_mut(i)
        && slot.type_code() == v.type_code()
    {
        *slot = v;
    }
}

/// `CAMPAIGN_PLAYER_SETUP` #3 is the human flag, and a save holds it twice: in the setup
/// (`CAMPAIGN_SETUP/CAMPAIGN_PLAYERS_SETUP/PLAYERS_ARRAY[]`) and in each faction's own record
/// (`FACTION` #1). A startpos has both false for every faction; a real save has both true for exactly
/// the player's faction (`britain` in the user's Coalition saves, CONFIRMED). The game plays by the
/// faction's copy: the faction loader (`FUN_0087a190`) reads it into the faction at +0x638, so the
/// flag sits at faction +0x6e0, which ~60 functions test (CONFIRMED in the exe). The user's test
/// (2026-10-03): with only the setup copy set (NR-1) every turn still ran as AI, and the original's
/// own `auto_save` of that game kept the faction copy false (SAVE_COMPAT.md §2).
fn write_human_flags(env: &mut EsfRecord, human: &str) {
    let mark = |p: &mut EsfRecord| {
        let is_human = p.get_str(2).is_some_and(|key| key == human);
        set(p, 3, EsfNode::Bool(is_human));
    };
    if let Some(array) = child_mut(env, "CAMPAIGN_SETUP")
        .and_then(|s| child_mut(s, "CAMPAIGN_PLAYERS_SETUP"))
        .and_then(|p| array_mut(p, "PLAYERS_ARRAY"))
    {
        for item in &mut array.items {
            if let Some(p) = first_rec_mut(item) {
                mark(p);
            }
        }
    }
    let Some(world) = child_mut(env, "CAMPAIGN_MODEL").and_then(|m| child_mut(m, "WORLD")) else { return };
    if let Some(a) = array_mut(world, "FACTION_ARRAY") {
        for item in &mut a.items {
            if let Some(p) = first_rec_mut(item).and_then(|f| child_mut(f, "CAMPAIGN_PLAYER_SETUP")) {
                mark(p);
            }
        }
    }
    if let Some(p) = child_mut(world, "REBEL_FACTION").and_then(|r| child_mut(r, "FACTION")).and_then(|f| child_mut(f, "CAMPAIGN_PLAYER_SETUP")) {
        mark(p);
    }
}

/// The human flag of each faction's own `FACTION/CAMPAIGN_PLAYER_SETUP` (#3), the copy the game
/// plays by: (faction key, flag), in `FACTION_ARRAY` order (rebels last).
pub fn faction_player_flags(esf: &EsfFile) -> Vec<(String, bool)> {
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return Vec::new() };
    faction_records(world)
        .into_iter()
        .filter_map(|f| f.child("CAMPAIGN_PLAYER_SETUP"))
        .map(|p| (p.get_str(2).unwrap_or_default().to_string(), p.get_bool(3).unwrap_or(false)))
        .collect()
}

/// The `CAMPAIGN_PLAYER_SETUP` entries of a startpos or save: (faction key, #3 human flag), in
/// file order.
pub fn player_flags(esf: &EsfFile) -> Vec<(String, bool)> {
    esf.root
        .find_path("CAMPAIGN_ENV/CAMPAIGN_SETUP/CAMPAIGN_PLAYERS_SETUP")
        .and_then(|p| p.record_array("PLAYERS_ARRAY"))
        .into_iter()
        .flat_map(|a| a.records())
        .map(|p| (p.get_str(2).unwrap_or_default().to_string(), p.get_bool(3).unwrap_or(false)))
        .collect()
}

/// Makes `human` the human player (and every other faction AI) in a startpos or save tree: the
/// human flags and the AI manager types.
pub fn mark_human(esf: &mut EsfFile, human: &str) {
    let id = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").and_then(|w| {
        faction_records(w)
            .into_iter()
            .find(|f| f.child("CAMPAIGN_PLAYER_SETUP").and_then(|p| p.get_str(2)) == Some(human))
            .and_then(faction_id)
    });
    if let Some(env) = child_mut(&mut esf.root, "CAMPAIGN_ENV") {
        write_human_flags(env, human);
        if let (Some(id), Some(cai)) = (id, child_mut(env, "CAMPAIGN_MODEL").and_then(|m| child_mut(m, "CAI_INTERFACE"))) {
            crate::cai::write_manager_types(cai, id as u32);
        }
    }
}

fn write_date(r: &mut EsfRecord, d: &ntw_sim::calendar::Date) {
    set(r, 0, EsfNode::U32(d.year));
    set(r, 1, EsfNode::U32(d.season));
    set(r, 2, EsfNode::U32(d.month));
    set(r, 3, EsfNode::U32(d.half));
}

/// Writes `model` into a copy of `source` (the start position or save it was loaded from) and
/// returns the save tree. `human` is the faction key the save header names; `timestamp` goes in
/// the ESF header (unix seconds; tests pass a constant so the output is reproducible).
pub fn write_save(source: &EsfFile, model: &CampaignModel, human: &str, timestamp: u32) -> Result<EsfFile, SaveError> {
    write_save_with(source, model, human, timestamp, None)
}

/// [`write_save`], also writing the scripts' `save_value` slots (`EPISODIC_RESTRICTIONS/LUA[]`)
/// when `script_values` is given (else the source's slots are kept as they are).
pub fn write_save_with(
    source: &EsfFile,
    model: &CampaignModel,
    human: &str,
    timestamp: u32,
    script_values: Option<&[crate::script_values::ScriptSaveValue]>,
) -> Result<EsfFile, SaveError> {
    let mut out = write_save_tree(source, model, human, timestamp)?;
    if let Some(v) = script_values {
        crate::script_values::write_script_values(&mut out, v);
    }
    Ok(out)
}

fn write_save_tree(source: &EsfFile, model: &CampaignModel, human: &str, timestamp: u32) -> Result<EsfFile, SaveError> {
    let mut out = source.clone();
    out.header.timestamp = timestamp;
    let root = &mut out.root;
    // A new campaign: the human gets its first victory option, as the front end gives it
    // (SAVE_COMPAT.md §17; the start position's default makes the original declare victory).
    let mut victory = None;
    let new_campaign = root.name == STARTPOS_ROOT;
    match root.name.as_str() {
        STARTPOS_ROOT => {
            victory = crate::victory::options(root, human).and_then(|o| o.into_iter().nth(crate::victory::DEFAULT_OPTION));
            root.name = SAVE_ROOT.to_string();
            root.children.retain(|c| !matches!(c, EsfNode::Record(r) if r.name == "CAMPAIGN_PREOPEN_MAP_INFO"));
        }
        SAVE_ROOT => {}
        other => return Err(SaveError::WrongRoot(other.to_string())),
    }

    let cal = &model.calendar;
    if let Some(h) = child_mut(root, "SAVE_GAME_HEADER") {
        set(h, 0, EsfNode::Utf16String(human.to_string()));
        set(h, 2, EsfNode::U32(cal.turn_number()));
        set(h, 3, EsfNode::U32(cal.date.year));
        // #4 the season's name, from the calendar (the start position's is stale after a turn).
        if let Some(season) = cal.date.season_header_name() {
            set(h, 4, EsfNode::Utf16String(season.to_string()));
        }
        if let Some(d) = child_mut(h, "DATE") {
            write_date(d, &cal.date);
        }
    }
    let env = child_mut(root, "CAMPAIGN_ENV").ok_or(SaveError::Missing("CAMPAIGN_ENV"))?;
    write_human_flags(env, human);
    if let Some(v) = &victory {
        crate::victory::write_human(env, human, v);
    }
    // A new campaign: only the human keeps a mission manager and a shroud (as in the original's
    // own turn-1 save of a new campaign, SAVE_COMPAT.md §18).
    if new_campaign {
        trim_ai_faction_blocks(env, human);
    }
    let m = child_mut(env, "CAMPAIGN_MODEL").ok_or(SaveError::Missing("CAMPAIGN_MODEL"))?;
    if let Some(seed) = child_mut(m, "RandSeed") {
        set(seed, 0, EsfNode::U32(model.rng.state));
    }
    // The portrait decks the model drew from (CHARACTERS_FIDELITY.md §14).
    crate::portraits::write(m, model);
    if let Some(c) = child_mut(m, "CAMPAIGN_CALENDAR") {
        set(c, 0, EsfNode::U32(cal.turns_per_year));
        set(c, 1, EsfNode::U32(cal.turn_in_year));
        set(c, 3, EsfNode::U32(cal.turns_elapsed));
        if let Some(d) = child_mut(c, "DATE") {
            write_date(d, &cal.date);
        }
    }
    // The units per army / navy (#23 / #24, after `OSMOSIS_CULTURES`), as the original's writer
    // `0x008EBAB0` saves its globals.
    if m.children.get(crate::OSMOSIS_CULTURES_AT).and_then(EsfNode::as_record_array).is_some_and(|a| a.name == "OSMOSIS_CULTURES") {
        let caps = model.force_caps;
        // #21 / #22: the deal inflation's first net and factor (`0x008EC094` / `0x008EC0DE`).
        set(m, crate::OSMOSIS_CULTURES_AT + 1, EsfNode::U32(model.deal_inflation.first));
        set(m, crate::OSMOSIS_CULTURES_AT + 2, EsfNode::F32(model.deal_inflation.factor));
        set(m, crate::OSMOSIS_CULTURES_AT + 3, EsfNode::U32(caps.army));
        set(m, crate::OSMOSIS_CULTURES_AT + 4, EsfNode::U32(caps.navy));
    }
    let world = child_mut(m, "WORLD").ok_or(SaveError::Missing("WORLD"))?;
    let dropped = drop_duplicate_regions(world);
    if !dropped.is_empty() {
        log::warn!("save: dropped the repeated REGION records of ids {dropped:?} (the loader reads only the first record of an id)");
    }
    let ix = SourceIndex::build(world);
    let residences = ix.residences(model);
    write_factions(world, model, &ix, &residences)?;
    // Regiment and ship names of new units, and the name lists' in-use flags (SAVE_COMPAT.md §20).
    let new_units: BTreeSet<i32> = model.world.forces.values().flat_map(|f| f.units.iter()).map(|u| u.id.raw()).filter(|id| !ix.unit_items.contains_key(id)).collect();
    crate::regiments::write_regiment_names(world, model, &new_units);
    // The model's names of new unit officers and its name decks (SAVE_COMPAT.md §21; new
    // characters' names go with their details).
    crate::charnames::write_model_names(world, model, &new_units);
    write_regions(world, model)?;
    write_residences(world, &residences);
    drop_dangling_refs(m, model, &ix);
    // The trade routes' accumulated values (SAVE_COMPAT.md §12).
    crate::trade::write_accumulated(m, model);
    // Commodity prices, their history and trends (CAMPAIGN_TRADE_MANAGER #3-#7).
    crate::trade::write_market(m, model);
    // The human faction gets the HUMAN AI manager (SAVE_COMPAT.md §2).
    if let (Some(f), Some(cai)) = (model.faction_by_key(human), child_mut(m, "CAI_INTERFACE")) {
        crate::cai::write_manager_types(cai, f.id.raw() as u32);
    }
    // The AI block mirrors every character, force and unit; keep it in step with the world as the
    // original does (SAVE_COMPAT.md §10). It stays version 13: any other version makes the
    // original rebuild its AI without the director pool, the INFERRED cause of the round-1 crash
    // (§5). Untouched when the world is as loaded (so a loaded save is written back unchanged).
    if (ix.world_changed(model) || ix.anyone_moved(model))
        && let Some(cai) = child_mut(m, "CAI_INTERFACE")
    {
        let view = ai_world_view(model, &ix, &residences);
        crate::cai_world::sync(cai, &view);
    }
    crate::obstacles::update(m, model, &ix);
    Ok(out)
}

/// The world as the AI block mirrors it (`cai_world::WorldView`): every faction's characters,
/// forces and units, except rebel characters that command no force (CONFIRMED, SAVE_COMPAT.md §18:
/// rebel armies are mirrored with their commander and units, a rebel character without a force is
/// not).
fn ai_world_view(model: &CampaignModel, ix: &SourceIndex, residences: &BTreeMap<u32, u32>) -> crate::cai_world::WorldView {
    use crate::cai_world::{CharView, ForceView, WorldView};
    let w = &model.world;
    let skip = |f: ntw_sim::campaign::FactionId| ix.rebel_faction == Some(f.raw());
    let mut view = WorldView::default();
    let commands: BTreeMap<i32, u32> = w.forces.values().filter_map(|f| Some((f.commander?.raw(), f.id.raw()))).collect();
    let attached: BTreeMap<i32, i32> = w.forces.values().flat_map(|f| f.units.iter()).filter_map(|u| Some((u.character?.raw(), u.id.raw()))).collect();
    for c in w.characters.values().filter(|c| !skip(c.faction) || commands.contains_key(&c.id.raw())) {
        view.characters.insert(
            c.id.raw() as u32,
            CharView {
                faction: c.faction.raw() as u32,
                pos: (c.position.0.raw(), c.position.1.raw()),
                commands: commands.get(&c.id.raw()).copied().unwrap_or(0),
                unit: attached.get(&c.id.raw()).map_or(0, |&u| u as u32),
            },
        );
    }
    for f in w.forces.values() {
        let id = f.id.raw();
        view.forces.insert(
            id,
            ForceView { faction: f.faction.raw() as u32, commander: f.commander.map_or(0, |c| c.raw() as u32), units: f.units.iter().map(|u| u.id.raw() as u32).collect() },
        );
        for u in &f.units {
            view.units.insert(u.id.raw() as u32, (id, u.character.map_or(0, |c| c.raw() as u32)));
        }
    }
    let settlements: BTreeSet<u32> = ix.settlement_residence.values().copied().collect();
    for (&force, &res) in residences {
        if settlements.contains(&res) {
            view.garrisons.insert(res, force);
        } else if ix.slot_residences.contains(&res) {
            view.slot_garrisons.insert(force);
        }
    }
    if let Some(t) = model.terrain.clone() {
        let scale = f32::from(1u16 << 10) * f32::from(1u16 << 10);
        view.region_at = Some(Box::new(move |x, z| t.0.region_at(x as f32 / scale, z as f32 / scale).map(str::to_string)));
    }
    view
}

/// Every `SIEGEABLE_GARRISON_RESIDENCE` #12 (settlements and slots) from the written residences:
/// the force whose `ARMY` #5 names it, else 0 (SAVE_COMPAT.md §4).
fn write_residences(world: &mut EsfRecord, residences: &BTreeMap<u32, u32>) {
    let by_residence: BTreeMap<u32, u32> = residences.iter().map(|(&f, &r)| (r, f)).collect();
    let Some(rm) = child_mut(world, "REGION_MANAGER") else { return };
    fn visit(r: &mut EsfRecord, by: &BTreeMap<u32, u32>) {
        if r.name == "SIEGEABLE_GARRISON_RESIDENCE" {
            let id = r.get_u32(1).unwrap_or(0);
            set(r, 12, EsfNode::U32(by.get(&id).copied().unwrap_or(0)));
            return;
        }
        for c in &mut r.children {
            match c {
                EsfNode::Record(x) => visit(x, by),
                EsfNode::RecordArray(a) => a.items.iter_mut().flatten().for_each(|n| {
                    if let EsfNode::Record(x) = n {
                        visit(x, by);
                    }
                }),
                _ => {}
            }
        }
    }
    visit(rm, &by_residence);
}

/// Calls `f` on `r` and every record below it, and `g` on every record array below it (also the
/// arrays and records held directly in array items).
fn visit_mut(r: &mut EsfRecord, f: &mut dyn FnMut(&mut EsfRecord), g: &mut dyn FnMut(&mut EsfRecordArray)) {
    fn nodes(ns: &mut [EsfNode], f: &mut dyn FnMut(&mut EsfRecord), g: &mut dyn FnMut(&mut EsfRecordArray)) {
        for n in ns {
            match n {
                EsfNode::Record(x) => visit_mut(x, f, g),
                EsfNode::RecordArray(a) => {
                    g(a);
                    for item in &mut a.items {
                        nodes(item, f, g);
                    }
                }
                _ => {}
            }
        }
    }
    f(r);
    nodes(&mut r.children, f, g);
}

/// The other places the original stores character and force ids (`save_audit refsites`,
/// SAVE_COMPAT.md §4), cleared of objects the model no longer has (the original resolves ids
/// through its global map; a missing one is a null object):
/// * recruitment pools (`CHARACTER_RECRUITMENT_MANAGER/{GENERAL,ADMIRAL}_RECRUITMENT` #0): living
///   characters without a force only (CONFIRMED rule in every save of the original);
/// * `CHARACTER` #6 (a force), `MILITARY_FORCE` #2 and residence #14 (characters): kept while the
///   object exists;
/// * `PORT_GARRISON_MANAGER` #0 (the navy in a port): kept while the navy exists with the same
///   commander at the same position, else 0 (as for slot garrisons);
/// * `COMMERCE_RAIDS` items whose raider is gone are dropped.
///
/// `PENDING_BATTLE` is left as it is (`save_check` reports a participant that is gone).
fn drop_dangling_refs(m: &mut EsfRecord, model: &CampaignModel, ix: &SourceIndex) {
    use ntw_sim::campaign::{CharacterId, ForceId};
    let w = &model.world;
    let commanders: BTreeSet<i32> = w.forces.values().filter_map(|f| f.commander.map(|c| c.raw())).collect();
    let alive = |c: u32| w.characters.contains_key(&CharacterId(c as i32));
    let navy_in_place = |id: u32| {
        let Some(f) = w.forces.get(&ForceId(id)).filter(|f| f.is_navy) else { return false };
        let Some(c) = f.commander else { return false };
        ix.force_commander.get(&id) == Some(&(c.raw() as u32))
            && w.characters.get(&c).map(|ch| (ch.position.0.raw(), ch.position.1.raw())) == ix.char_pos.get(&c.raw()).copied()
    };
    let keep_chars = |r: &mut EsfRecord, i: usize, pool: bool| {
        if let Some(EsfNode::U32Array(v)) = r.children.get_mut(i) {
            v.retain(|&c| alive(c) && !(pool && commanders.contains(&(c as i32))));
        }
    };
    let mut records = |r: &mut EsfRecord| match r.name.as_str() {
        "GENERAL_RECRUITMENT" | "ADMIRAL_RECRUITMENT" => keep_chars(r, 0, true),
        "MILITARY_FORCE" => keep_chars(r, 2, false),
        "SIEGEABLE_GARRISON_RESIDENCE" => keep_chars(r, 14, false),
        "CHARACTER" if r.get_u32(6).is_some_and(|f| f != 0 && !w.forces.contains_key(&ForceId(f))) => set(r, 6, EsfNode::U32(0)),
        "PORT_GARRISON_MANAGER" if r.get_u32(0).is_some_and(|f| f != 0 && !navy_in_place(f)) => set(r, 0, EsfNode::U32(0)),
        _ => {}
    };
    let mut arrays = |a: &mut EsfRecordArray| {
        if a.name == "COMMERCE_RAIDS" {
            a.items.retain(|item| item.first().and_then(EsfNode::as_u32).is_none_or(|c| c == 0 || alive(c)));
        }
    };
    visit_mut(m, &mut records, &mut arrays);
}

/// The faction record of a `FACTION_ARRAY` item or of `REBEL_FACTION`, with its id.
fn faction_id(rec: &EsfRecord) -> Option<i32> {
    rec.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_)))?.as_i32()
}

/// Every faction record of a `WORLD`: the `FACTION_ARRAY` items, then `REBEL_FACTION/FACTION`.
fn faction_records(world: &EsfRecord) -> Vec<&EsfRecord> {
    let mut out: Vec<&EsfRecord> = world.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    out.extend(world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")));
    out
}

fn loco_position(c: &EsfRecord) -> Option<(i32, i32)> {
    let loco = c.children.first()?.as_record()?;
    Some((loco.get_i32(0)?, loco.get_i32(1)?))
}

/// What the writer needs to know about the source tree before it changes it: templates for new
/// records and the links the original stores between characters, forces and residences
/// (SAVE_COMPAT.md §4).
#[derive(Default)]
pub(crate) struct SourceIndex {
    /// Character id → `LOCOMOTABLE` position.
    pub char_pos: BTreeMap<i32, (i32, i32)>,
    /// Force id → its commander (`MILITARY_FORCE` #1, 0 = none).
    pub force_commander: BTreeMap<u32, u32>,
    /// Force id → its unit ids, in order.
    pub force_units: BTreeMap<u32, Vec<i32>>,
    /// Force id → `ARMY` #5 (the residence it garrisons, 0 = none).
    pub army_residence: BTreeMap<u32, u32>,
    /// Region id → its settlement's residence id (`SIEGEABLE_GARRISON_RESIDENCE` #1).
    pub settlement_residence: BTreeMap<u32, u32>,
    /// Region id → its settlement residence's #12 as stored.
    pub settlement_garrison: BTreeMap<u32, u32>,
    /// Region id → owner as stored (`REGION` #20).
    pub region_owner: BTreeMap<u32, u32>,
    /// The residence ids of slots (towns, ports, forts).
    pub slot_residences: BTreeSet<u32>,
    /// `CHARACTER_ARRAY` items to clone for new colonels / captains: (faction id, type, item).
    pub char_templates: Vec<(i32, String, Vec<EsfNode>)>,
    land_unit: Option<Vec<EsfNode>>,
    naval_unit: Option<Vec<EsfNode>>,
    army: Option<Vec<EsfNode>>,
    navy: Option<Vec<EsfNode>>,
    /// Every unit item by unit id (also the templates for new units of the same key).
    unit_items: BTreeMap<i32, Vec<EsfNode>>,
    /// Unit key → the id of the first unit of that key (a template for new units of the key).
    unit_by_key: BTreeMap<String, i32>,
    /// The rebels' faction id (`REBEL_FACTION/FACTION`).
    pub rebel_faction: Option<i32>,
}

impl SourceIndex {
    pub(crate) fn build(world: &EsfRecord) -> Self {
        let mut ix = SourceIndex::default();
        for f in faction_records(world) {
            let fid = faction_id(f).unwrap_or(0);
            if let Some(a) = f.record_array("CHARACTER_ARRAY") {
                for item in &a.items {
                    let Some(c) = first_rec(item).filter(|r| r.name == "CHARACTER") else { continue };
                    let Some(id) = c.get_i32(2) else { continue };
                    if let Some(p) = loco_position(c) {
                        ix.char_pos.insert(id, p);
                    }
                    let kind = c.get_str(3).unwrap_or_default();
                    if (kind == "colonel" || kind == "captain" || kind == "minister") && !ix.char_templates.iter().any(|(f, k, _)| *f == fid && k == kind) {
                        ix.char_templates.push((fid, kind.to_string(), item.clone()));
                    }
                }
            }
            for item in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.items.iter()) {
                let Some(r) = first_rec(item) else { continue };
                let navy = r.name == "NAVY";
                let slot = if navy { &mut ix.navy } else { &mut ix.army };
                if slot.is_none() {
                    *slot = Some(item.clone());
                }
                let Some(mf) = r.child("MILITARY_FORCE") else { continue };
                let fid = mf.get_u32(0).unwrap_or(0);
                ix.force_commander.insert(fid, mf.get_u32(1).unwrap_or(0));
                if !navy {
                    ix.army_residence.insert(fid, r.get_u32(5).unwrap_or(0));
                }
                let mut ids = Vec::new();
                for u in r.record_array("UNITS_ARRAY").into_iter().flat_map(|a| a.items.iter()) {
                    let Some(w) = first_rec(u) else { continue };
                    let unit = w.child("UNIT");
                    if let Some(id) = unit.and_then(|u| u.get_i32(4)) {
                        ids.push(id);
                        ix.unit_items.entry(id).or_insert_with(|| u.clone());
                        if let Some(k) = unit.and_then(|u| u.child("UNIT_RECORD_KEY")).and_then(|k| k.get_str(0)) {
                            ix.unit_by_key.entry(k.to_string()).or_insert(id);
                        }
                    }
                    let slot = match w.name.as_str() {
                        "LAND_UNIT" => &mut ix.land_unit,
                        "NAVAL_UNIT" => &mut ix.naval_unit,
                        _ => continue,
                    };
                    // Prefer a plain unit (#10 = 0: no attached character) as the template.
                    let plain = |x: &Vec<EsfNode>| first_rec(x).and_then(|w| w.child("UNIT")).and_then(|u| u.get_u32(10)) == Some(0);
                    if slot.is_none() || plain(u) && !slot.as_ref().is_some_and(plain) {
                        *slot = Some(u.clone());
                    }
                }
                ix.force_units.insert(fid, ids);
            }
        }
        ix.rebel_faction = world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")).and_then(faction_id);
        if let Some(a) = world.child("REGION_MANAGER").and_then(|m| m.record_array("REGIONS_ARRAY")) {
            for r in a.records() {
                let Some(id) = r.get_i32(4) else { continue };
                let id = id as u32;
                ix.region_owner.insert(id, r.get_u32(20).unwrap_or(0));
                if let Some(g) = r.child("SETTLEMENT").and_then(|s| s.child("SIEGEABLE_GARRISON_RESIDENCE")) {
                    ix.settlement_residence.insert(id, g.get_u32(1).unwrap_or(0));
                    ix.settlement_garrison.insert(id, g.get_u32(12).unwrap_or(0));
                }
                for slot in r.child("REGION_SLOT_MANAGER").and_then(|m| m.record_array("REGION_SLOT_ARRAY")).into_iter().flat_map(|a| a.records()) {
                    if let Some(res) = slot.child("SIEGEABLE_GARRISON_RESIDENCE").and_then(|g| g.get_u32(1)) {
                        ix.slot_residences.insert(res);
                    }
                }
            }
        }
        ix
    }

    /// Did the world's objects or their links change from the source? Then the AI block, which
    /// names every one of them, no longer matches (SAVE_COMPAT.md §5).
    /// Did any character move from where the source has it?
    pub(crate) fn anyone_moved(&self, model: &CampaignModel) -> bool {
        model.world.characters.values().any(|c| self.char_pos.get(&c.id.raw()).is_some_and(|&p| p != (c.position.0.raw(), c.position.1.raw())))
    }

    pub(crate) fn world_changed(&self, model: &CampaignModel) -> bool {
        let w = &model.world;
        let chars: BTreeSet<i32> = self.char_pos.keys().copied().collect();
        let model_chars: BTreeSet<i32> = w.characters.keys().map(|c| c.raw()).collect();
        if model_chars != chars || w.forces.len() != self.force_commander.len() {
            return true;
        }
        for f in w.forces.values() {
            let Some(&cmd) = self.force_commander.get(&f.id.raw()) else { return true };
            if cmd != f.commander.map_or(0, |c| c.raw() as u32) {
                return true;
            }
            let units: Vec<i32> = f.units.iter().map(|u| u.id.raw()).collect();
            if self.force_units.get(&f.id.raw()) != Some(&units) {
                return true;
            }
        }
        w.regions.values().any(|r| self.region_owner.get(&r.id.raw()).is_some_and(|&o| o != r.owner.raw() as u32))
    }

    /// The residence each force garrisons in the written save (`ARMY` #5 / residence #12):
    /// a settlement's garrison from the model; a slot residence (town, fort) kept from the source
    /// while the force and its commander are unchanged and the commander has not moved.
    pub(crate) fn residences(&self, model: &CampaignModel) -> BTreeMap<u32, u32> {
        let w = &model.world;
        let mut out = BTreeMap::new();
        for r in w.regions.values() {
            let Some(g) = r.garrison.filter(|g| w.forces.get(g).is_some_and(|f| !f.is_navy)) else { continue };
            if let Some(&res) = self.settlement_residence.get(&r.id.raw()) {
                out.insert(g.raw(), res);
            }
        }
        let taken: BTreeSet<u32> = out.values().copied().collect();
        for (&force, &res) in &self.army_residence {
            if res == 0 || !self.slot_residences.contains(&res) || taken.contains(&res) || out.contains_key(&force) {
                continue;
            }
            let Some(f) = w.forces.get(&ntw_sim::campaign::ForceId(force)) else { continue };
            let Some(c) = f.commander else { continue };
            let same_cmd = self.force_commander.get(&force) == Some(&(c.raw() as u32));
            let pos = w.characters.get(&c).map(|ch| (ch.position.0.raw(), ch.position.1.raw()));
            if same_cmd && pos.is_some() && pos == self.char_pos.get(&c.raw()).copied() {
                out.insert(force, res);
            }
        }
        out
    }
}

fn write_factions(world: &mut EsfRecord, model: &CampaignModel, ix: &SourceIndex, residences: &BTreeMap<u32, u32>) -> Result<(), SaveError> {
    let patch = |f: &mut EsfRecord| -> Result<(), SaveError> {
        let Some(id) = faction_id(f) else { return Ok(()) };
        let Some(faction) = model.world.factions.values().find(|x| x.id.raw() == id) else { return Ok(()) };
        write_faction(f, faction, model, ix, residences)
    };
    if let Some(a) = array_mut(world, "FACTION_ARRAY") {
        for item in &mut a.items {
            if let Some(f) = first_rec_mut(item) {
                patch(f)?;
            }
        }
    }
    if let Some(r) = child_mut(world, "REBEL_FACTION").and_then(|r| child_mut(r, "FACTION")) {
        patch(r)?;
    }
    Ok(())
}

/// A new character's `CHARACTER_ARRAY` item: a colonel / captain of the same file cloned (same
/// record version), preferring one of the same faction, with the ids, faction, position and
/// action points set; traits and ancillaries emptied as in a freshly raised colonel of the
/// original (`auto_save` france colonel 819984184). The names are the template's (PROVISIONAL:
/// the original draws them from the faction's name lists).
fn new_character_item(ch: &ntw_sim::campaign::Character, faction_key: &str, ix: &SourceIndex) -> Result<Vec<EsfNode>, SaveError> {
    let kind = ch.kind.esf_name();
    let fid = ch.faction.raw();
    let t = ix
        .char_templates
        .iter()
        .find(|(f, k, _)| *f == fid && k == kind)
        .or_else(|| ix.char_templates.iter().find(|(_, k, _)| k == kind))
        .or_else(|| ix.char_templates.first())
        .ok_or(SaveError::NoTemplate("CHARACTER"))?;
    let mut item = t.2.clone();
    let c = first_rec_mut(&mut item).ok_or(SaveError::NoTemplate("CHARACTER"))?;
    let (x, z) = (ch.position.0.raw(), ch.position.1.raw());
    if let Some(EsfNode::Record(loco)) = c.children.get_mut(0) {
        for (i, v) in [x, z, x, z].into_iter().enumerate() {
            set(loco, i, EsfNode::I32(v));
        }
        set(loco, 8, EsfNode::I32(ch.base_movement_points));
        set(loco, 9, EsfNode::I32(ch.movement_points));
    }
    if let Some(EsfNode::Record(d)) = c.children.get_mut(1) {
        // CHARACTER_DETAILS: #0 TRAITS {TRAIT[]}, #9 faction key, #10 the id again, #11 ancillaries.
        if let Some(EsfNode::Record(t)) = d.children.get_mut(0)
            && let Some(EsfNode::RecordArray(a)) = t.children.get_mut(0).filter(|a| matches!(a, EsfNode::RecordArray(x) if x.name == "TRAIT"))
        {
            a.items.clear();
        }
        set(d, 9, EsfNode::Utf16String(faction_key.to_string()));
        set(d, 10, EsfNode::U32(ch.id.raw() as u32));
        if let Some(EsfNode::RecordArray(a)) = d.children.get_mut(11).filter(|a| matches!(a, EsfNode::RecordArray(x) if x.name == "AgentAncillaries")) {
            a.items.clear();
        }
    }
    set(c, 2, EsfNode::I32(ch.id.raw()));
    set(c, 3, EsfNode::Utf16String(kind.to_string()));
    Ok(item)
}

/// The recruitment pools from the model (FACTION #75 `CHARACTER_RECRUITMENT_MANAGER`:
/// `GENERAL_RECRUITMENT` / `ADMIRAL_RECRUITMENT` {u32[] candidate ids, u32 refill timer}; layout
/// CONFIRMED, CHARACTERS_FIDELITY.md §8). [`drop_dangling_refs`] then keeps only living characters
/// without a force, as every original save has them.
fn write_pools(f: &mut EsfRecord, details: Option<&ntw_sim::campaign::details::FactionDetails>) {
    let Some(d) = details else { return };
    let Some(m) = child_mut(f, "CHARACTER_RECRUITMENT_MANAGER") else { return };
    for (name, (ids, timer)) in [("GENERAL_RECRUITMENT", &d.general_pool), ("ADMIRAL_RECRUITMENT", &d.admiral_pool)] {
        if let Some(p) = child_mut(m, name) {
            set(p, 0, EsfNode::U32Array(ids.iter().map(|c| c.raw() as u32).collect()));
            set(p, 1, EsfNode::U32(*timer));
        }
    }
}

/// `CHARACTER_DETAILS` #0 `TRAITS/TRAIT[]` {key, points} and #11 `AgentAncillaries[]` {key} from
/// the model, for every character (traits and ancillaries change in play; layouts CONFIRMED in
/// every original save; SAVE_COMPAT.md §23).
fn write_traits(d: &mut EsfRecord, details: &ntw_sim::campaign::details::CharacterDetails) {
    if d.name != "CHARACTER_DETAILS" {
        return;
    }
    if let Some(EsfNode::Record(t)) = d.children.get_mut(0)
        && t.name == "TRAITS"
        && let Some(EsfNode::RecordArray(a)) = t.children.get_mut(0)
        && a.name == "TRAIT"
    {
        a.items = details.traits.iter().map(|t| vec![EsfNode::Utf16String(t.key.clone()), EsfNode::I32(t.points)]).collect();
    }
    if let Some(EsfNode::RecordArray(a)) = d.children.get_mut(11)
        && a.name == "AgentAncillaries"
    {
        a.items = details.ancillaries.iter().map(|k| vec![EsfNode::Utf16String(k.clone())]).collect();
    }
}

/// `FACTION_TECHNOLOGY_MANAGER` `techs[]` {utf16 key, u32 state, f32 progress, u32 researcher,
/// u32[], u32} (layout CONFIRMED; CAMPAIGN_FIDELITY.md §Research): #1 the state, #2 the progress,
/// #3 the researching school's slot id and #5 the traded count (entry +0x28, saver `0x00894430`)
/// from the model, by key. Technologies the model does not
/// list are left as stored. A value equal to the stored one is not rewritten, so an unchanged save
/// writes back byte for byte (a stored -0.0 stays).
fn write_technologies(f: &mut EsfRecord, details: Option<&ntw_sim::campaign::details::FactionDetails>) {
    let Some(d) = details else { return };
    let Some(tm) = child_mut(f, "FACTION_TECHNOLOGY_MANAGER") else { return };
    let states: BTreeMap<&str, u32> = d.technologies.iter().map(|(k, s)| (k.as_str(), *s)).collect();
    for c in tm.children.iter_mut() {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "techs" {
            continue;
        }
        for it in a.items.iter_mut() {
            let Some(key) = it.first().and_then(EsfNode::as_str).map(str::to_owned) else { continue };
            let Some(&state) = states.get(key.as_str()) else { continue };
            let r = d.research.get(&key);
            let (progress, researcher, traded) = r.map_or((0.0, 0, 0), |r| (r.progress, r.researcher, r.traded));
            if it.get(1).and_then(EsfNode::as_u32) != Some(state)
                && let Some(n) = it.get_mut(1)
            {
                *n = EsfNode::U32(state);
            }
            if it.get(2).and_then(EsfNode::as_f32).is_none_or(|p| p != progress)
                && let Some(n) = it.get_mut(2)
            {
                *n = EsfNode::F32(progress);
            }
            if it.get(3).and_then(EsfNode::as_u32) != Some(researcher)
                && let Some(n) = it.get_mut(3)
            {
                *n = EsfNode::U32(researcher);
            }
            if it.get(5).and_then(EsfNode::as_u32) != Some(traded)
                && let Some(n) = it.get_mut(5)
            {
                *n = EsfNode::U32(traded);
            }
        }
    }
}

/// A new character's `CHARACTER_DETAILS` from the model where the model knows them: #1 / #2 names
/// (the names the model gave him when it created him, or after the family for a new faction leader;
/// a model without the names table leaves the template's), #4 the regnal numeral (a new monarch's;
/// empty for everyone else, so a template copied from an old monarch does not keep his) and #5 the
/// birth date.
fn write_new_details(d: &mut EsfRecord, details: &ntw_sim::campaign::details::CharacterDetails) {
    if d.name != "CHARACTER_DETAILS" {
        return;
    }
    if !details.forename.is_empty() {
        for (i, v) in [(1, &details.forename), (2, &details.surname)] {
            if let Some(EsfNode::Record(l)) = d.children.get_mut(i)
                && let Some(n @ EsfNode::Utf16String(_)) = l.children.first_mut()
            {
                *n = EsfNode::Utf16String(v.clone());
            }
        }
    }
    if let Some(n @ EsfNode::Utf16String(_)) = d.children.get_mut(4) {
        *n = EsfNode::Utf16String(details.regnal_numeral.clone());
    }
    if let Some(b) = &details.birth
        && let Some(EsfNode::Record(r)) = d.children.get_mut(5)
        && r.name == "DATE"
    {
        write_date(r, b);
    }
}

/// A character's `CHARACTER_DETAILS` #8 `PORTRAIT_DETAILS` from the model (card, custom name, info,
/// number): a new character's drawn portrait, a promoted one's new pictures. Only changed values
/// are written, so a portrait the model never touched stays bit for bit. A character the model
/// has no portrait for (number -1, no pictures) keeps the record's: a new minister keeps his
/// template's (the family's PLACEHOLDER, CHARACTERS_FIDELITY.md §5c).
fn write_portrait(d: &mut EsfRecord, p: &ntw_sim::campaign::details::Portrait) {
    if d.name != "CHARACTER_DETAILS" || *p == ntw_sim::campaign::details::Portrait::default() {
        return;
    }
    let Some(EsfNode::Record(r)) = d.children.get_mut(8) else { return };
    if r.name != "PORTRAIT_DETAILS" {
        return;
    }
    for (i, v) in [(0, &p.card), (1, &p.alternative), (2, &p.info)] {
        if matches!(r.children.get(i), Some(EsfNode::Utf16String(s)) if s != v) {
            set(r, i, EsfNode::Utf16String(v.clone()));
        }
    }
    if matches!(r.children.get(3), Some(EsfNode::I32(n)) if *n != p.index) {
        set(r, 3, EsfNode::I32(p.index));
    }
}

/// `GOVERNMENT/POSTS_ARRAY` holders (`CHARACTER_POST` #2) from the model, matched by post id (a
/// post whose holder died has the new holder, CHARACTERS_FIDELITY.md §5c).
fn write_posts(f: &mut EsfRecord, details: Option<&ntw_sim::campaign::details::FactionDetails>) {
    let Some(d) = details else { return };
    let Some(posts) = child_mut(f, "GOVERNMENT").and_then(|g| array_mut(g, "POSTS_ARRAY")) else { return };
    for item in &mut posts.items {
        let Some(p) = first_rec_mut(item).filter(|p| p.name == "CHARACTER_POST") else { continue };
        let Some(id) = p.get_i32(0) else { continue };
        if let Some(m) = d.posts.iter().find(|m| m.id == id) {
            set(p, 2, EsfNode::U32(m.holder.map_or(0, |h| h.raw() as u32)));
        }
    }
}

/// `FAMILY` from the model (layout CONFIRMED, `0x00890490` writes it): each member's fields in
/// place (#15 present exactly when #14 is not 0, a copy of #0's record shape), the heir index
/// (#10 u8) and the ordinal pairs.
fn write_family(r: &mut EsfRecord, fam: &ntw_sim::campaign::family::Family) {
    let locs = |template: &EsfRecord, names: &[String]| {
        let mut l = template.clone();
        l.children = names.iter().map(|s| EsfNode::Utf16String(s.clone())).collect();
        l
    };
    let mut k = 0;
    for c in r.children.iter_mut() {
        let EsfNode::Record(m) = c else { continue };
        if m.name != "FAMILY::MONARCHY_INFO_CHARACTER" {
            continue;
        }
        let Some(x) = fam.members.get(k) else { break };
        k += 1;
        let Some(EsfNode::Record(loc0)) = m.children.first().cloned() else { continue };
        m.children[0] = EsfNode::Record(Box::new(locs(&loc0, &x.names)));
        set(m, 1, EsfNode::Bool(x.male));
        set(m, 2, EsfNode::Bool(x.exists));
        set(m, 3, EsfNode::U8(x.children));
        set(m, 4, EsfNode::I32(x.trait_4));
        set(m, 5, EsfNode::I32(x.age));
        set(m, 6, EsfNode::I32(x.regnal));
        for (i, a) in x.child_ages.iter().enumerate() {
            set(m, 7 + i, EsfNode::I32(*a));
        }
        if let Some(EsfNode::Record(p)) = m.children.get_mut(11) {
            set(p, 0, EsfNode::Utf16String(x.portrait.card.clone()));
            set(p, 1, EsfNode::Utf16String(x.portrait.alternative.clone()));
            set(p, 2, EsfNode::Utf16String(x.portrait.info.clone()));
            set(p, 3, EsfNode::I32(x.portrait.index));
        }
        set(m, 12, EsfNode::Utf16String(x.religion.clone()));
        set(m, 13, EsfNode::U32(x.owner));
        set(m, 14, EsfNode::U32(x.married_to));
        m.children.truncate(15);
        if x.married_to != 0 {
            m.children.push(EsfNode::Record(Box::new(locs(&loc0, &x.spouse_names))));
        }
    }
    let heir = fam.heir_index();
    if let Some(n) = r.children.iter_mut().find(|n| matches!(n, EsfNode::U8(_))) {
        *n = EsfNode::U8(heir);
    }
    if let Some(a) = array_mut(r, "ORDINAL_PAIR")
        && let Some(template) = a.items.first().cloned()
    {
        a.items = fam
            .ordinals
            .iter()
            .map(|(name, n)| {
                let mut it = template.clone();
                if let Some(EsfNode::Record(l)) = it.get_mut(0) {
                    l.children = vec![EsfNode::Utf16String(name.clone())];
                }
                if let Some(v) = it.get_mut(1) {
                    *v = EsfNode::I32(*n);
                }
                it
            })
            .collect();
    }
}

fn write_faction(
    f: &mut EsfRecord,
    faction: &Faction,
    model: &CampaignModel,
    ix: &SourceIndex,
    residences: &BTreeMap<u32, u32>,
) -> Result<(), SaveError> {
    use ntw_sim::campaign::CharacterId;
    if let Some(e) = child_mut(f, "FACTION_ECONOMICS") {
        set(e, 1, EsfNode::I32(faction.treasury));
        // #3: bankrupt turns in a row (economics +0x460, CONFIRMED by its saver 0x00BD46E0).
        set(e, 3, EsfNode::U32(model.world.bankrupt_turns.get(&faction.id).copied().unwrap_or(0)));
        // #0: the economics history, oldest first, one ECONOMICS_DATA per record in the groups
        // the saver 0x00B985B0 writes (5/3/4/1/5/7 categories).
        if let Some(history) = model.world.economy_history.get(&faction.id) {
            write_economy_history(e, history);
        }
    }
    write_taxes(f, faction, model);
    write_stances(f, faction, model);
    write_bonus_values(f, model.world.faction_details.get(&faction.id));
    write_pools(f, model.world.faction_details.get(&faction.id));
    write_technologies(f, model.world.faction_details.get(&faction.id));
    let w = &model.world;
    // Who commands what, and which unit each character is attached to (CHARACTER #4 / #5).
    let commands: BTreeMap<i32, u32> = w.forces.values().filter_map(|fo| Some((fo.commander?.raw(), fo.id.raw()))).collect();
    let attached: BTreeMap<i32, i32> =
        w.forces.values().flat_map(|fo| fo.units.iter()).filter_map(|u| Some((u.character?.raw(), u.id.raw()))).collect();
    // Characters: patch survivors, drop the dead, add the new.
    if let Some(chars) = array_mut(f, "CHARACTER_ARRAY") {
        chars.items.retain(|item| {
            first_rec(item)
                .filter(|r| r.name == "CHARACTER")
                .and_then(|r| r.get_i32(2))
                .is_none_or(|id| w.characters.contains_key(&CharacterId(id)))
        });
        for item in &mut chars.items {
            let Some(c) = first_rec_mut(item).filter(|r| r.name == "CHARACTER") else { continue };
            let Some(id) = c.get_i32(2) else { continue };
            let Some(ch) = w.characters.get(&CharacterId(id)) else { continue };
            if let Some(EsfNode::Record(loco)) = c.children.get_mut(0) {
                set(loco, 0, EsfNode::I32(ch.position.0.raw()));
                set(loco, 1, EsfNode::I32(ch.position.1.raw()));
                set(loco, 8, EsfNode::I32(ch.base_movement_points));
                set(loco, 9, EsfNode::I32(ch.movement_points));
            }
        }
        let have: BTreeSet<i32> = chars.items.iter().filter_map(|i| first_rec(i)?.get_i32(2)).collect();
        for ch in w.characters.values().filter(|c| c.faction == faction.id && !have.contains(&c.id.raw())) {
            chars.items.push(new_character_item(ch, &faction.key, ix)?);
        }
        for item in &mut chars.items {
            let Some(c) = first_rec_mut(item).filter(|r| r.name == "CHARACTER") else { continue };
            let Some(id) = c.get_i32(2) else { continue };
            set(c, 4, EsfNode::U32(commands.get(&id).copied().unwrap_or(0)));
            set(c, 5, EsfNode::U32(attached.get(&id).map_or(0, |&u| u as u32)));
            // CHARACTER #17: the sight radius (f32, character +0x2EC; `World::sight_radius`).
            // Written only when it changed, so a stored value the model never touched stays
            // bit for bit.
            if let Some(&r) = w.sight_radius.get(&CharacterId(id))
                && matches!(c.children.get(17), Some(EsfNode::F32(v)) if v.to_bits() != r.to_bits())
            {
                set(c, 17, EsfNode::F32(r));
            }
            if let Some(d) = w.character_details.get(&CharacterId(id)) {
                // CHARACTER #8: the post he holds (a new minister's; the others keep theirs).
                set(c, 8, EsfNode::U32(d.post));
                // CHARACTER #36 / #37: duels lost / won (u32, CONFIRMED positions; 0-G's duels).
                if matches!(c.children.get(36), Some(EsfNode::U32(_))) {
                    set(c, 36, EsfNode::U32(d.duels_lost));
                }
                if matches!(c.children.get(37), Some(EsfNode::U32(_))) {
                    set(c, 37, EsfNode::U32(d.duels_won));
                }
                // CHARACTER #22: hidden (bool, CHARACTERS_FIDELITY.md §10; 0-G's agents).
                if matches!(c.children.get(22), Some(EsfNode::Bool(b)) if *b != d.hidden) {
                    set(c, 22, EsfNode::Bool(d.hidden));
                }
                // CHARACTER #27: fled a duel (bool), #14 / #15 and #28..#30: the turn-end counters
                // (CHARACTERS_FIDELITY.md §6, §7; positions CONFIRMED from the loader `0x00991520`).
                if matches!(c.children.get(27), Some(EsfNode::Bool(b)) if *b != d.fled) {
                    set(c, 27, EsfNode::Bool(d.fled));
                }
                if matches!(c.children.get(14), Some(EsfNode::Bool(b)) if *b != d.no_action) {
                    set(c, 14, EsfNode::Bool(d.no_action));
                }
                for (i, v) in [(15, d.idle_turns), (28, d.turns_at_sea), (29, d.turns_in_enemy_lands), (30, d.turns_at_home)] {
                    if matches!(c.children.get(i), Some(EsfNode::U32(x)) if *x != v) {
                        set(c, i, EsfNode::U32(v));
                    }
                }
                if let Some(EsfNode::Record(dr)) = c.children.get_mut(1) {
                    write_traits(dr, d);
                    write_portrait(dr, &d.portrait);
                    if !have.contains(&id) {
                        write_new_details(dr, d);
                    }
                }
            }
        }
    }
    write_posts(f, model.world.faction_details.get(&faction.id));
    write_exposed(f, model.world.faction_details.get(&faction.id));
    write_shroud(f, model.world.shrouds.get(&faction.id), model.world.sight_grid);
    if let Some(fam) = model.world.faction_details.get(&faction.id).and_then(|d| d.family.as_ref())
        && let Some(r) = child_mut(f, "FAMILY")
    {
        write_family(r, fam);
    }
    // Forces: keep the ones still in the model (rebuilding their units), drop the rest, add new.
    let mine: Vec<&MilitaryForce> = w.forces.values().filter(|x| x.faction == faction.id).collect();
    let Some(armies) = array_mut(f, "ARMY_ARRAY") else {
        return if mine.is_empty() { Ok(()) } else { Err(SaveError::Missing("ARMY_ARRAY")) };
    };
    let force_id = |item: &Vec<EsfNode>| first_rec(item).and_then(|r| r.child("MILITARY_FORCE")).and_then(|m| m.get_u32(0));
    armies.items.retain(|item| force_id(item).is_none_or(|id| mine.iter().any(|f| f.id.raw() == id)));
    for force in &mine {
        if !armies.items.iter().any(|item| force_id(item) == Some(force.id.raw())) {
            armies.items.push(new_force_item(force, ix)?);
        }
    }
    for item in &mut armies.items {
        let Some(id) = force_id(item) else { continue };
        let Some(force) = mine.iter().find(|f| f.id.raw() == id) else { continue };
        let Some(r) = first_rec_mut(item) else { continue };
        if let Some(mf) = child_mut(r, "MILITARY_FORCE") {
            set(mf, 1, EsfNode::U32(force.commander.map_or(0, |c| c.raw() as u32)));
        }
        if r.name == "ARMY" {
            set(r, 5, EsfNode::U32(residences.get(&id).copied().unwrap_or(0)));
            // ARMY #7 = the navy carrying it, 0 = none (CONFIRMED by the ports worker from the
            // loaders 0x00870FD0 / 0x008822F0 and the writer 0x008FAB60).
            let navy = w.embarked.get(&force.id).map_or(0, |n| n.raw());
            set(r, 7, EsfNode::U32(navy));
        } else {
            // NAVY #4 = the army it carries, 0 = none (same source).
            let carried = w.embarked.iter().find(|(_, n)| **n == force.id).map_or(0, |(a, _)| a.raw());
            set(r, 4, EsfNode::U32(carried));
        }
        let Some(units) = array_mut(r, "UNITS_ARRAY") else { continue };
        units.items = force.units.iter().map(|u| unit_item(u, force.is_navy, model, ix)).collect::<Result<_, _>>()?;
    }
    Ok(())
}

/// A new force's `ARMY_ARRAY` item: the first `ARMY` / `NAVY` of the file with the fields of a
/// freshly raised one as the original writes it (`auto_save` france army 728689440):
/// `MILITARY_FORCE` {id, commander, []}, `ARMY` #2/#3 empty, #4 = the id, #5 residence (set by the
/// caller), #6 false, #7 0, #8 false; `NAVY` #2/#3 empty, #4 0, #6 0 (INFERRED from its new navy).
fn new_force_item(force: &MilitaryForce, ix: &SourceIndex) -> Result<Vec<EsfNode>, SaveError> {
    let t = if force.is_navy { &ix.navy } else { &ix.army };
    let mut item = t.clone().ok_or(SaveError::NoTemplate(if force.is_navy { "NAVY" } else { "ARMY" }))?;
    if let Some(r) = first_rec_mut(&mut item) {
        if let Some(mf) = child_mut(r, "MILITARY_FORCE") {
            set(mf, 0, EsfNode::U32(force.id.raw()));
            set(mf, 2, EsfNode::U32Array(Vec::new()));
        }
        set(r, 2, EsfNode::U32Array(Vec::new()));
        set(r, 3, EsfNode::U32Array(Vec::new()));
        if r.name == "ARMY" {
            set(r, 4, EsfNode::I32(force.id.raw() as i32));
            set(r, 5, EsfNode::U32(0));
            set(r, 6, EsfNode::Bool(false));
            set(r, 7, EsfNode::U32(0));
            set(r, 8, EsfNode::Bool(false));
        } else {
            set(r, 4, EsfNode::U32(0));
            set(r, 6, EsfNode::U32(0));
        }
    }
    Ok(item)
}

/// `FACTION` `EXPOSED_CHARACTERS` (items {i32 character id}, CONFIRMED layout in
/// `orig_fr_may1811`) from `FactionDetails::exposed`; rewritten only when the list changed.
fn write_exposed(f: &mut EsfRecord, details: Option<&ntw_sim::campaign::details::FactionDetails>) {
    let (Some(d), Some(a)) = (details, array_mut(f, "EXPOSED_CHARACTERS")) else { return };
    let stored: Vec<i32> = a.items.iter().filter_map(|it| it.first()?.as_i32()).collect();
    let now: Vec<i32> = d.exposed.iter().map(|c| c.raw()).collect();
    if stored != now {
        a.items = now.into_iter().map(|id| vec![EsfNode::I32(id)]).collect();
    }
}

/// `FACTION` `CAMPAIGN_SHROUD` trees #0 (explored) and #1 (visible) from `World::shrouds`
/// (`shroud::encode`, root = the sight grid's; our encoder writes every tree of the vanilla saves
/// back identically, 39 of 39 checked). A tree is rewritten only when its cells changed; a faction
/// without a shroud in the model, or without the record, is left as it is. #2 and the flag are
/// not touched (#2 is empty in every vanilla save).
fn write_shroud(f: &mut EsfRecord, shroud: Option<&ntw_sim::campaign::visibility::Shroud>, grid: Option<ntw_sim::campaign::visibility::SightGrid>) {
    let (Some(sh), Some(grid)) = (shroud, grid) else { return };
    let Some(rec) = child_mut(f, "CAMPAIGN_SHROUD") else { return };
    let trees: Vec<usize> = rec
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c, EsfNode::Record(r) if r.name == "QUAD_TREE_BIT_ARRAY"))
        .map(|(i, _)| i)
        .collect();
    for (k, set) in [&sh.explored, &sh.visible].into_iter().enumerate() {
        let Some(&i) = trees.get(k) else { continue };
        let EsfNode::Record(q) = &rec.children[i] else { continue };
        if crate::shroud::decode(q).as_ref() == Some(set) {
            continue;
        }
        rec.children[i] = EsfNode::Record(Box::new(crate::shroud::encode(set, grid.root)));
    }
}

/// The tax levels into every governorship's `GOVERNORSHIP_TAXES` {u32 lower, u32 upper, u8 lower
/// rate, u8 upper rate} (CAMPAIGN_DATA.md §3; level index order INFERRED). A level the model
/// does not know is left as stored.
fn write_taxes(f: &mut EsfRecord, faction: &Faction, model: &CampaignModel) {
    use ntw_sim::campaign::details::GovernorshipTaxes;
    let rate = |k: &str| model.rules.tax_levels.get(k).copied().or_else(|| {
        // Without DB rules: the shipped `taxes_levels` rates (CONFIRMED values).
        Some([5, 10, 15, 20, 25][GovernorshipTaxes::level_index(k)? as usize])
    });
    let (Some(lo), Some(up)) = (GovernorshipTaxes::level_index(&faction.tax_lower), GovernorshipTaxes::level_index(&faction.tax_upper)) else {
        return;
    };
    let (Some(lo_rate), Some(up_rate)) = (rate(&faction.tax_lower), rate(&faction.tax_upper)) else { return };
    // Each governorship's own levels from the model (`FactionDetails::posts`, matched by post id;
    // `SetTaxLevel` updates them); a governorship the model does not have gets the faction's levels.
    let model_posts: Vec<(i32, GovernorshipTaxes)> = model
        .world
        .faction_details
        .get(&faction.id)
        .map(|d| d.posts.iter().filter_map(|p| Some((p.id, p.governorship.as_ref()?.taxes))).collect())
        .unwrap_or_default();
    let faction_levels = GovernorshipTaxes { lower: lo, upper: up, lower_rate: lo_rate, upper_rate: up_rate };
    let Some(posts) = child_mut(f, "GOVERNMENT").and_then(|g| array_mut(g, "POSTS_ARRAY")) else { return };
    // The save's rate fields are u8; a modded rate outside 0..=255 is written clamped, logged once
    // per faction.
    let mut unfit = None;
    let mut rate_u8 = |v: i32| {
        u8::try_from(v).unwrap_or_else(|_| {
            unfit.get_or_insert(v);
            v.clamp(0, 255) as u8
        })
    };
    for item in &mut posts.items {
        let Some(p) = first_rec_mut(item) else { continue };
        let id = p.get_i32(0);
        let Some(t) = child_mut(p, "GOVERNORSHIP").and_then(|g| child_mut(g, "GOVERNORSHIP_TAXES")) else {
            continue;
        };
        let taxes = model_posts.iter().find(|(pid, _)| Some(*pid) == id).map_or(faction_levels, |(_, t)| *t);
        set(t, 0, EsfNode::U32(taxes.lower));
        set(t, 1, EsfNode::U32(taxes.upper));
        set(t, 2, EsfNode::U8(rate_u8(taxes.lower_rate)));
        set(t, 3, EsfNode::U8(rate_u8(taxes.upper_rate)));
    }
    if let Some(v) = unfit {
        log::warn!("save: {}: tax rate {v} does not fit the save's u8 field; written clamped to 0..=255", faction.key);
    }
}

/// The model's stances into `DIPLOMACY_RELATIONSHIP` #4 (CONFIRMED position). When a stance
/// changes, the stored one moves to #20 (INFERRED "previous stance"). #18, the scripted diplomacy
/// permissions (`Relationship::diplomacy_options`, the u32[14] at relationship +0x7EC that
/// `force_diplomacy` sets, `0x00B28670`), is written from the model. The other fields are kept.
fn write_stances(f: &mut EsfRecord, faction: &Faction, model: &CampaignModel) {
    let Some(rels) = child_mut(f, "DIPLOMACY_MANAGER").and_then(|d| array_mut(d, "DIPLOMACY_RELATIONSHIPS_ARRAY")) else { return };
    for item in &mut rels.items {
        let Some(r) = first_rec_mut(item).filter(|r| r.name == "DIPLOMACY_RELATIONSHIP") else { continue };
        let Some(target) = r.get_i32(0) else { continue };
        let target = ntw_sim::campaign::FactionId(target);
        let want = faction.diplomacy.get(&target).copied().unwrap_or_default().esf_name();
        let old = r.get_str(4).unwrap_or_default().to_string();
        if old != want {
            set(r, 4, EsfNode::Utf16String(want.to_string()));
            set(r, 20, EsfNode::Utf16String(old));
        }
        let Some(rel) = model.world.relationships.get(&(faction.id, target)) else { continue };
        match r.children.get_mut(18) {
            Some(EsfNode::U32Array(v)) => *v = rel.diplomacy_options.to_vec(),
            _ => {
                static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("save: a DIPLOMACY_RELATIONSHIP has no u32 array at #18; its force_diplomacy permissions are not written (logged once)");
                }
            }
        }
    }
}

/// A unit's `UNITS_ARRAY` item: the stored one (men and attached character updated), or for a new
/// unit a stored unit of the same key (else the first plain unit of the kind) with the fields of a
/// freshly raised unit as the original writes it (`auto_save` france unit 911044776): history date
/// = today, `COMMANDER_DETAILS` faction = the owner, no traits, #7 0, #8/#9/#11/#12 0, #13 0,
/// #15 -1, #16 1.0. The regiment name is the template's (PROVISIONAL).
fn unit_item(u: &CampaignUnit, navy: bool, model: &CampaignModel, ix: &SourceIndex) -> Result<Vec<EsfNode>, SaveError> {
    let mut item = match ix.unit_items.get(&u.id.raw()) {
        Some(i) => i.clone(),
        None => {
            let same = ix.unit_by_key.get(&u.unit_key).and_then(|id| ix.unit_items.get(id));
            let same = same.filter(|i| first_rec(i).is_some_and(|w| (w.name == "NAVAL_UNIT") == navy));
            let t = same.or(if navy { ix.naval_unit.as_ref() } else { ix.land_unit.as_ref() });
            let mut i = t.cloned().ok_or(SaveError::NoTemplate("unit"))?;
            let owner = model
                .world
                .forces
                .values()
                .find(|f| f.units.iter().any(|x| x.id == u.id))
                .and_then(|f| model.world.factions.get(&f.faction))
                .map(|f| f.key.clone());
            if let Some(w) = first_rec_mut(&mut i) {
                // LAND_RECORD_KEY / NAVAL_RECORD_KEY {utf16 key}.
                if let Some(EsfNode::Record(k)) = w.children.get_mut(0) {
                    set(k, 0, EsfNode::Utf16String(u.unit_key.clone()));
                }
                if let Some(unit) = child_mut(w, "UNIT") {
                    if let Some(k) = child_mut(unit, "UNIT_RECORD_KEY") {
                        set(k, 0, EsfNode::Utf16String(u.unit_key.clone()));
                    }
                    if let Some(h) = child_mut(unit, "UNIT_HISTORY") {
                        if let Some(d) = child_mut(h, "DATE") {
                            write_date(d, &model.calendar.date);
                        }
                        set(h, 1, EsfNode::U32(0));
                        set(h, 2, EsfNode::U32(0));
                    }
                    if let (Some(cd), Some(key)) = (child_mut(unit, "COMMANDER_DETAILS"), owner) {
                        set(cd, 2, EsfNode::Utf16String(key));
                    }
                    if let Some(t) = child_mut(unit, "TRAITS")
                        && let Some(EsfNode::RecordArray(a)) = t.children.get_mut(0)
                    {
                        a.items.clear();
                    }
                    set(unit, 4, EsfNode::I32(u.id.raw()));
                    set(unit, 7, EsfNode::I32(0));
                    for i in [8, 9, 11, 12] {
                        set(unit, i, EsfNode::U32(0));
                    }
                    set(unit, 13, EsfNode::U8(0));
                    set(unit, 15, EsfNode::I32(-1));
                    set(unit, 16, EsfNode::F32(1.0));
                }
            }
            i
        }
    };
    if let Some(unit) = first_rec_mut(&mut item).and_then(|w| child_mut(w, "UNIT")) {
        set(unit, 5, EsfNode::U32(u.men));
        set(unit, 6, EsfNode::U32(u.max_men));
        set(unit, 10, EsfNode::U32(u.character.map_or(0, |c| c.raw() as u32)));
    }
    Ok(item)
}

/// A `REGION` id repeated in `REGIONS_ARRAY` (malformed). The loader reads
/// only the first record of an id (`world::load_world`, with a `LoadWarning::DuplicateId`), so the save
/// drops the others before anything reads the source: the written file then holds exactly the regions
/// the model was loaded from, and the source index reads those records. A repeated record left in place
/// would keep its old owner (#20), residences and queue. Returns the dropped ids, for one log line.
fn drop_duplicate_regions(world: &mut EsfRecord) -> Vec<i32> {
    let Some(regions) = child_mut(world, "REGION_MANAGER").and_then(|rm| array_mut(rm, "REGIONS_ARRAY")) else { return Vec::new() };
    let mut seen = BTreeSet::new();
    let mut dropped = Vec::new();
    regions.items.retain(|item| {
        let Some(id) = first_rec(item).filter(|r| r.name == "REGION").and_then(|r| r.get_i32(4)) else { return true };
        let first = seen.insert(id);
        if !first {
            dropped.push(id);
        }
        first
    });
    dropped
}

fn write_regions(world: &mut EsfRecord, model: &CampaignModel) -> Result<(), SaveError> {
    let gov_of = |f: ntw_sim::campaign::FactionId| model.world.factions.get(&f);
    // A BUILDING record to clone for new buildings.
    let mut template: Option<EsfRecord> = None;
    if let Some(a) = world.child("REGION_MANAGER").and_then(|m| m.record_array("REGIONS_ARRAY")) {
        'find: for r in a.records() {
            let mut found = None;
            r.walk(&mut |x| {
                if found.is_none() && x.name == "BUILDING" {
                    found = Some(x.clone());
                }
            });
            if found.is_some() {
                template = found;
                break 'find;
            }
        }
    }
    let rm = child_mut(world, "REGION_MANAGER").ok_or(SaveError::Missing("REGION_MANAGER"))?;
    let Some(regions) = array_mut(rm, "REGIONS_ARRAY") else { return Err(SaveError::Missing("REGIONS_ARRAY")) };
    let mut links = RecruitmentLinks::new(&model.world.recruitment_sources);
    // Each id has one record here: `write_save_tree` has dropped the repeated ones (`drop_duplicate_regions`).
    for item in &mut regions.items {
        let Some(r) = first_rec_mut(item).filter(|r| r.name == "REGION") else { continue };
        let Some(id) = r.get_i32(4) else { continue };
        let Some(region) = model.world.regions.get(&ntw_sim::campaign::RegionId(id as u32)) else { continue };
        let owner = gov_of(region.owner);
        write_region(r, region, owner, template.as_ref(), &|k| model.rules.is_naval_unit(k), &mut links, &model.rules)?;
    }
    if links.broken > 0 {
        // Logged once per save: the source tree is not the file the model was loaded from.
        log::warn!("save: {} queued recruitment items no longer match the record they were loaded from; written as new records", links.broken);
    }
    if !links.repaired.is_empty() {
        log::warn!("save: malformed {}", links.repaired.join("; malformed "));
    }
    if links.misplaced > 0 {
        log::warn!("save: {} queued ships are in regions without a port recruitment manager; written to the region's own manager", links.misplaced);
    }
    Ok(())
}

/// The region's economy fields the model changes each round (REGION writer 0x00A51E30, child →
/// region offset, CAMPAIGN_FIDELITY.md "Economy"; CONFIRMED positions): #9 base GDP, #10 GDP,
/// #12 town wealth, #14 stored town wealth (= #12 in every region of every original save,
/// CONFIRMED), #15 town wealth growth, #17 bankruptcy growth offset (u32 in the files; `set` only
/// writes a value of the stored type), #18 the discontent part of the growth, #19 tax exempt. #13
/// (town wealth at the last wealth level) and #16 (wealth level count) are not modelled: kept.
fn write_region_economy(r: &mut EsfRecord, region: &Region) {
    set(r, 9, EsfNode::U32(region.base_gdp));
    set(r, 10, EsfNode::U32(region.gdp));
    set(r, 12, EsfNode::U32(region.town_wealth));
    set(r, 14, EsfNode::U32(region.town_wealth));
    set(r, 15, EsfNode::I32(region.town_wealth_growth));
    set(r, 17, EsfNode::U32(region.wealth_growth_offset.max(0) as u32));
    set(r, 17, EsfNode::I32(region.wealth_growth_offset));
    set(r, 18, EsfNode::I32(region.discontent_growth));
    set(r, 19, EsfNode::Bool(region.tax_exempt));
}

/// The region's population fields the model changes each round (`POPULATION/REGION_FACTORS`, the
/// layout of the reader `0x00A4C340`, CONFIRMED): #0 the growth factors, #2 the population, #3 the
/// capacity, #4 its base, #5 the growth, #6 the trend, #7 overcrowded, #8 the migrants, and each
/// `RELIGION_BREAKDOWN` item's share (#1) by religion key. `POPULATION` #1..#4 and the classes' stored
/// factors are not modelled: kept.
fn write_region_population(r: &mut EsfRecord, region: &Region) {
    let Some(f) = child_mut(r, "POPULATION").and_then(|p| child_mut(p, "REGION_FACTORS")) else { return };
    let p = &region.population_state;
    set(f, 0, EsfNode::F32Array(p.factors.to_vec()));
    set(f, 2, EsfNode::U32(region.population));
    set(f, 3, EsfNode::U32(p.capacity));
    set(f, 4, EsfNode::U32(p.base_capacity));
    set(f, 5, EsfNode::F32(p.growth));
    set(f, 6, EsfNode::U32(p.trend));
    set(f, 7, EsfNode::Bool(p.overcrowded));
    set(f, 8, EsfNode::I32(p.migrants));
    let Some(a) = array_mut(f, "RELIGION_BREAKDOWN") else { return };
    for item in &mut a.items {
        let Some(key) = item.first().and_then(EsfNode::as_str).map(str::to_owned) else { continue };
        if let (Some((_, share)), Some(slot)) = (region.religions.iter().find(|(k, _)| *k == key), item.get_mut(1))
            && slot.type_code() == EsfNode::F32(0.0).type_code()
        {
            *slot = EsfNode::F32(*share);
        }
    }
}

/// Every `GARRISON_RESIDENCE` #0 under `r` that names `old` gets `new`.
fn hand_over_residences(r: &mut EsfRecord, old: u32, new: u32) {
    if r.name == "GARRISON_RESIDENCE" && r.get_u32(0) == Some(old) {
        set(r, 0, EsfNode::U32(new));
    }
    for c in &mut r.children {
        match c {
            EsfNode::Record(x) => hand_over_residences(x, old, new),
            EsfNode::RecordArray(a) => {
                for x in a.items.iter_mut().flat_map(|it| it.iter_mut()) {
                    if let EsfNode::Record(x) = x {
                        hand_over_residences(x, old, new);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The load-time links of the recruitment items (`World::recruitment_sources`), and what went
/// wrong during one save (each logged once at its end): links that no longer matched their
/// record, ships written to a region's own manager because it has no port manager, and the
/// malformed `REGION` records repaired, with what each got ([`repair_recruitment_managers`]).
struct RecruitmentLinks<'a> {
    sources: &'a BTreeMap<ntw_sim::campaign::RecruitmentItemId, ntw_sim::campaign::RecruitmentSource>,
    broken: usize,
    misplaced: usize,
    repaired: Vec<String>,
}

impl<'a> RecruitmentLinks<'a> {
    fn new(sources: &'a BTreeMap<ntw_sim::campaign::RecruitmentItemId, ntw_sim::campaign::RecruitmentSource>) -> Self {
        RecruitmentLinks { sources, broken: 0, misplaced: 0, repaired: Vec::new() }
    }
}

/// An empty `REGION_RECRUITMENT_ITEM_ARRAY` v0.
fn empty_recruitment_array() -> EsfNode {
    EsfNode::RecordArray(Box::new(EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0)))
}

/// Repairs a malformed `REGION` record in the save's copy of its source (SAVE_COMPAT.md §32), so the
/// writer can count on the managers it writes new items to: `targets` names them (`None` the region's
/// own, `Some(i)` the manager of port slot `i`, which exists: a port is a slot holding one).
/// [`write_region`] runs it only for a region that has a new item to write (one without a valid load
/// link), before writing any item, and only on the managers those items go to; every other manager,
/// and a record no new item needs, is left as it is, so a file saved untouched keeps its bytes. Every
/// region of the 8 shipped startpos files and the vanilla saves has a `REGION_RECRUITMENT_MANAGER` v1
/// = {`REGION_RECRUITMENT_ITEM_ARRAY` v0, bool, i32} at #27, and every port slot's manager holds its
/// array (CONFIRMED). One rule: a missing part is an empty one appended after its parent's last
/// child, a missing manager to the `REGION`, a missing array to its manager (for a record holding
/// exactly #0..#26 that is the original's #27; for an empty manager, #0). Our loader finds both by
/// type (`world::read_region`, `world::read_recruitment`), so an appended part moves no field. The
/// added manager's bool is false (as in every file) and its i32, a unique object id referenced
/// nowhere else and not read by our loader, is 0 (no id). Returns what was added, for the one log
/// line of the save.
fn repair_recruitment_managers(r: &mut EsfRecord, targets: &BTreeSet<Option<usize>>) -> Vec<&'static str> {
    let mut added = Vec::new();
    if targets.contains(&None) && r.child("REGION_RECRUITMENT_MANAGER").is_none() {
        let mut m = EsfRecord::new("REGION_RECRUITMENT_MANAGER", 1);
        m.children = vec![empty_recruitment_array(), EsfNode::Bool(false), EsfNode::I32(0)];
        r.children.push(EsfNode::Record(Box::new(m)));
        added.push("REGION_RECRUITMENT_MANAGER");
    }
    for &target in targets {
        if let Some(m) = manager_record(r, target)
            && m.record_array("REGION_RECRUITMENT_ITEM_ARRAY").is_none()
        {
            m.children.push(empty_recruitment_array());
            added.push(if target.is_some() { "port REGION_RECRUITMENT_ITEM_ARRAY" } else { "REGION_RECRUITMENT_ITEM_ARRAY" });
        }
    }
    added
}

/// Where a queued item is written: back to the record it was loaded from, or as a new record to a manager.
enum QueueDest {
    Linked(ntw_sim::campaign::RecruitmentSource),
    New(Option<usize>),
}

fn write_region(
    r: &mut EsfRecord,
    region: &Region,
    owner: Option<&Faction>,
    template: Option<&EsfRecord>,
    is_naval: &dyn Fn(&str) -> bool,
    links: &mut RecruitmentLinks<'_>,
    rules: &ntw_sim::campaign::rules::CampaignRules,
) -> Result<(), SaveError> {
    // A captured region: the original hands every residence of the old owner in it (settlement
    // and slots: ports, forts, towns, roads) to the new owner (CONFIRMED: in its saves every
    // residence names the region's owner, apart from a slot another faction holds; our NR-5 had
    // the slots left with the old owner, SAVE_COMPAT.md §17).
    let new_owner = region.owner.raw() as u32;
    if let Some(old) = r.get_u32(20).filter(|&o| o != new_owner) {
        hand_over_residences(r, old, new_owner);
    }
    set(r, 20, EsfNode::U32(new_owner));
    write_region_economy(r, region);
    write_region_population(r, region);
    if let Some(gr) = child_mut(r, "SETTLEMENT")
        .and_then(|s| child_mut(s, "SIEGEABLE_GARRISON_RESIDENCE"))
        .and_then(|g| child_mut(g, "GARRISON_RESIDENCE"))
    {
        set(gr, 0, EsfNode::U32(region.owner.raw() as u32));
    }
    let (fkey, gkey) = owner.map_or((String::new(), String::new()), |f| (f.key.clone(), f.government_key.clone()));
    if let Some(sm) = child_mut(r, "REGION_SLOT_MANAGER") {
        write_slot_manager(sm, region, &fkey, &gkey, template, rules)?;
    }
    // Recruitment: land items in the region's own manager (REGION #27), naval items in the slot
    // managers (ports; W3 §3.4; a region can have several). A loaded item keeps the exact record
    // it was read from (`World::recruitment_sources`, linked at load), ids and all, in that
    // manager; only its turns and cost are updated, and its id when the loader gave it a new one.
    // Items made since have no link and are built fresh (see `recruitment_item`): naval ones go
    // to the first port's manager, land ones to the region's, as the original's managers hold them
    // (a land unit never in a port's). Only the managers such new items go to are repaired first
    // (`repair_recruitment_managers`), so each exists with its array; nothing else in the record
    // changes but the managers' own items. A ship in a region without a port manager is a state the
    // original never reaches (the recruit command refuses a ship there: its naval points come from
    // port buildings only, `recruitment_points`, CONFIRMED 0x00B61EE0): PROVISIONAL, see below. No
    // queued item is lost. Nothing is matched by id or unit.
    // Each item's linked record, if the source still has it there (a source other than the loaded
    // file, or a record of another unit, is not used).
    let linked: Vec<Option<ntw_sim::campaign::RecruitmentSource>> = region
        .recruitment_queue
        .iter()
        .map(|it| {
            let s = *links.sources.get(&it.id)?;
            let e = manager(r, s.port_slot)?.items.get(s.index)?;
            (recruitment_unit_key(e) == Some(it.unit_key.as_str())).then_some(s)
        })
        .collect();
    links.broken += region.recruitment_queue.iter().zip(&linked).filter(|(it, s)| s.is_none() && links.sources.contains_key(&it.id)).count();
    // The first port: the first slot holding a recruitment manager (as the loader marks a port).
    let port = manager_records(r).into_iter().find_map(|(m, _)| m);
    let dests: Vec<QueueDest> = region
        .recruitment_queue
        .iter()
        .zip(&linked)
        .map(|(it, s)| match s {
            Some(s) => QueueDest::Linked(*s),
            None if !is_naval(&it.unit_key) => QueueDest::New(None),
            None if port.is_some() => QueueDest::New(port),
            // PROVISIONAL (SAVE_COMPAT.md §32): a ship in a region without a port manager goes to
            // the region's own manager, and loads back from there as a queued ship of the region.
            // The original has no such state to copy.
            None => {
                links.misplaced += 1;
                QueueDest::New(None)
            }
        })
        .collect();
    let targets: BTreeSet<Option<usize>> = dests.iter().filter_map(|d| if let QueueDest::New(m) = d { Some(*m) } else { None }).collect();
    if !targets.is_empty() {
        let added = repair_recruitment_managers(r, &targets);
        if !added.is_empty() {
            links.repaired.push(format!("REGION {}: added {}", region.id.raw(), added.join(", ")));
        }
    }
    write_recruitment(r, region, &dests, is_naval)
}

/// Writes `region`'s queue to its managers, item `i` to `dests[i]` (the repair has run, so every
/// manager a new item goes to holds its array; the repair only appends, so every linked record is
/// still at its index). Every item's manager and record are resolved into new lists before any
/// manager is changed, so a failure (a missing manager or record: a `SaveError`) leaves the managers
/// as they were. A manager's items not in the queue (cancelled, finished) are dropped.
fn write_recruitment(r: &mut EsfRecord, region: &Region, dests: &[QueueDest], is_naval: &dyn Fn(&str) -> bool) -> Result<(), SaveError> {
    let mut managers = managers_mut(r);
    let mut out: Vec<Vec<Vec<EsfNode>>> = vec![Vec::new(); managers.len()];
    for (it, d) in region.recruitment_queue.iter().zip(dests) {
        let slot = match d {
            QueueDest::Linked(s) => s.port_slot,
            QueueDest::New(m) => *m,
        };
        let mi = managers.iter().position(|(m, _)| *m == slot).ok_or(SaveError::Missing("REGION_RECRUITMENT_ITEM_ARRAY"))?;
        let item = match d {
            QueueDest::Linked(s) => {
                let mut e = managers[mi].1.items.get(s.index).ok_or(SaveError::Missing("REGION_RECRUITMENT_ITEM"))?.clone();
                if let Some(inner) =
                    first_rec_mut(&mut e).and_then(|o| first_rec_mut(&mut o.children)).and_then(|w| child_mut(w, "RECRUITMENT_ITEM"))
                {
                    // The id in the record's own type: a u32 stays a u32 (the loader reads it bit for
                    // bit); anything else that is not this id (0, a repeated id, a non-integer) becomes
                    // the item's i32 id, so the next load reads the same id.
                    let id = match inner.children.first() {
                        Some(EsfNode::U32(_)) => EsfNode::U32(it.id.raw() as u32),
                        _ => EsfNode::I32(it.id.raw()),
                    };
                    if let Some(slot) = inner.children.first_mut() {
                        *slot = id;
                    }
                    // #2 the target in the record's own type (a target the loader dropped becomes 0).
                    let target = it.target.map_or(0, |c| c.raw());
                    let target = match inner.children.get(2) {
                        Some(EsfNode::U32(_)) => EsfNode::U32(target as u32),
                        _ => EsfNode::I32(target),
                    };
                    set(inner, 2, target);
                    set(inner, 3, EsfNode::U32(it.turns_remaining));
                    if inner.get_u32(4) != Some(it.cost.max(0) as u32) {
                        set(inner, 4, EsfNode::U32(it.cost.max(0) as u32));
                        set(inner, 7, EsfNode::U32(it.cost.max(0) as u32));
                    }
                }
                e
            }
            QueueDest::New(_) => {
                let naval = is_naval(&it.unit_key);
                vec![EsfNode::Record(Box::new(recruitment_item(region, it, naval)))]
            }
        };
        out[mi].push(item);
    }
    for ((_, a), items) in managers.iter_mut().zip(out) {
        a.items = items;
    }
    Ok(())
}

/// The buildings and construction items of a region's `REGION_SLOT_MANAGER`: every slot of its
/// `REGION_SLOT_ARRAY`, the road and the walls (`ROAD_SLOT` / `FORTIFICATION_SLOT`, each found
/// as the reader finds it, [`crate::world::wrapped_slot`]).
fn write_slot_manager(
    sm: &mut EsfRecord,
    region: &Region,
    faction: &str,
    government: &str,
    template: Option<&EsfRecord>,
    rules: &ntw_sim::campaign::rules::CampaignRules,
) -> Result<(), SaveError> {
    if let Some(slots) = array_mut(sm, "REGION_SLOT_ARRAY") {
        for (i, item) in slots.items.iter_mut().enumerate() {
            let Some(slot) = first_rec_mut(item) else { continue };
            let Some(model_slot) = region.slots.get(i) else { continue };
            let c = region.construction.iter().find(|c| c.slot == SlotRef::Slot(i));
            write_building_manager(slot, model_slot.building.as_ref(), c, faction, government, template, rules)?;
        }
    }
    for (name, slot_ref, building) in [("ROAD_SLOT", SlotRef::Road, &region.road), ("FORTIFICATION_SLOT", SlotRef::Walls, &region.fortification)] {
        if let Some(slot) = wrapped_slot_mut(sm, name) {
            let c = region.construction.iter().find(|c| c.slot == slot_ref);
            write_building_manager(slot, building.as_ref(), c, faction, government, template, rules)?;
        }
    }
    Ok(())
}

/// [`crate::world::wrapped_slot`], to write: the `name` child's `REGION_SLOT`, or the child itself
/// when it holds none.
fn wrapped_slot_mut<'a>(sm: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    let slot = child_mut(sm, name)?;
    if slot.child("REGION_SLOT").is_some() { child_mut(slot, "REGION_SLOT") } else { Some(slot) }
}

/// The unit key (inner `RECRUITMENT_ITEM` #6) of a `REGION_RECRUITMENT_ITEM_ARRAY` item.
fn recruitment_unit_key(item: &[EsfNode]) -> Option<&str> {
    first_rec(item)?.children.first()?.as_record()?.child("RECRUITMENT_ITEM")?.get_str(6)
}

/// The two places recruitment managers live in a region, found once for every lookup: its own
/// (the first `REGION_RECRUITMENT_MANAGER` child) and the `REGION_SLOT_ARRAY` of the first
/// `REGION_SLOT_MANAGER`, whose item `i` (the raw index, as the loader counts it) may hold one.
fn manager_places(r: &mut EsfRecord) -> (Option<&mut EsfRecord>, Option<&mut EsfRecordArray>) {
    let (mut own, mut slots) = (None, None);
    for c in &mut r.children {
        let EsfNode::Record(b) = c else { continue };
        let first = match b.name.as_str() {
            "REGION_RECRUITMENT_MANAGER" => &mut own,
            "REGION_SLOT_MANAGER" => &mut slots,
            _ => continue,
        };
        if first.is_none() {
            *first = Some(&mut **b);
        }
    }
    (own, slots.and_then(|sm| array_mut(sm, "REGION_SLOT_ARRAY")))
}

/// The recruitment manager of slot array item `item`: its first record's `REGION_RECRUITMENT_MANAGER`.
fn slot_manager(item: &mut [EsfNode]) -> Option<&mut EsfRecord> {
    child_mut(first_rec_mut(item)?, "REGION_RECRUITMENT_MANAGER")
}

/// Every recruitment manager record of a region, in order: its own (`None`) and each slot's that
/// holds one (`Some(i)`). Built from [`manager_places`] and [`slot_manager`], the same rule
/// [`manager_record`] follows for one.
fn manager_records(r: &mut EsfRecord) -> Vec<(Option<usize>, &mut EsfRecord)> {
    let (own, slots) = manager_places(r);
    let mut out: Vec<(Option<usize>, &mut EsfRecord)> = own.into_iter().map(|m| (None, m)).collect();
    if let Some(array) = slots {
        out.extend(array.items.iter_mut().enumerate().filter_map(|(i, item)| Some((Some(i), slot_manager(item)?))));
    }
    out
}

/// A recruitment manager record of a region: its own (`None`) or slot `i`'s. Nothing is collected.
fn manager_record(r: &mut EsfRecord, slot: Option<usize>) -> Option<&mut EsfRecord> {
    let (own, slots) = manager_places(r);
    match slot {
        None => own,
        Some(i) => slot_manager(slots?.items.get_mut(i)?),
    }
}

/// Every recruitment queue of a region (each manager holding its item array), all at once.
fn managers_mut(r: &mut EsfRecord) -> Vec<(Option<usize>, &mut EsfRecordArray)> {
    manager_records(r).into_iter().filter_map(|(m, rec)| Some((m, array_mut(rec, "REGION_RECRUITMENT_ITEM_ARRAY")?))).collect()
}

/// A recruitment queue of a region: its own (`None`) or slot `i`'s.
fn manager(r: &mut EsfRecord, slot: Option<usize>) -> Option<&mut EsfRecordArray> {
    array_mut(manager_record(r, slot)?, "REGION_RECRUITMENT_ITEM_ARRAY")
}

/// `BUILDING_MANAGER` v1 = {bool has building, [BUILDING], bool has construction,
/// [BUILDING_CONSTRUCTION_ITEM]} (CONFIRMED in the user's saves).
fn write_building_manager(
    slot: &mut EsfRecord,
    building: Option<&BuildingRef>,
    construction: Option<&ntw_sim::campaign::ConstructionItem>,
    faction: &str,
    government: &str,
    template: Option<&EsfRecord>,
    rules: &ntw_sim::campaign::rules::CampaignRules,
) -> Result<(), SaveError> {
    let Some(bm) = child_mut(slot, "BUILDING_MANAGER") else { return Ok(()) };
    let old = bm.child("BUILDING").cloned();
    let old_item = bm.child("BUILDING_CONSTRUCTION_ITEM").cloned();
    let mut children = Vec::new();
    children.push(EsfNode::Bool(building.is_some()));
    if let Some(b) = building {
        let mut rec = match old {
            Some(o) => o,
            None => template.cloned().ok_or(SaveError::NoTemplate("BUILDING"))?,
        };
        let same = rec.get_str(1) == Some(b.level_key.as_str());
        set(&mut rec, 0, EsfNode::U32(b.health));
        set(&mut rec, 1, EsfNode::Utf16String(b.level_key.clone()));
        if !same {
            set(&mut rec, 2, EsfNode::Utf16String(faction.to_string()));
            set(&mut rec, 3, EsfNode::Utf16String(government.to_string()));
        }
        children.push(EsfNode::Record(Box::new(rec)));
    }
    // A repair item ({u32 2, bool, u32, u32 turns, u32 cost, u32 health}, no key: CONFIRMED shape
    // in the user's saves, under damaged buildings) is not modelled: keep it as stored.
    let repair = old_item.clone().filter(|o| construction.is_none() && o.get_str(5).is_none());
    if let Some(o) = repair {
        children.push(EsfNode::Bool(true));
        children.push(EsfNode::Record(Box::new(o)));
        bm.children = children;
        return Ok(());
    }
    children.push(EsfNode::Bool(construction.is_some()));
    // The same item as in the source: keep its record (all its fields), only advance the turns.
    let kept = construction.and_then(|c| {
        let mut o = old_item.clone().filter(|o| o.get_str(5) == Some(c.level_key.as_str()))?;
        let total = o.get_u32(3)?;
        if total >= c.turns_remaining {
            set(&mut o, 2, EsfNode::U32(total - c.turns_remaining));
        } else {
            set(&mut o, 2, EsfNode::U32(0));
            set(&mut o, 3, EsfNode::U32(c.turns_remaining));
        }
        Some(o)
    });
    if let Some(o) = kept {
        children.push(EsfNode::Record(Box::new(o)));
    } else if let Some(c) = construction {
        // #2 turns done / #3 total (CONFIRMED pattern in the original's saves: done counts up to
        // the level's build time). The total is the level's `building_levels` time.
        let total = rules.buildings.get(&c.level_key).map_or(0, |b| b.turns).max(c.turns_remaining).max(1);
        let mut rec = EsfRecord::new("BUILDING_CONSTRUCTION_ITEM", 1);
        rec.children = vec![
            // #0: 1 for a new building in an empty slot, 0 for an upgrade (INFERRED from the
            // user's saves); #1 true in every sample.
            EsfNode::U32(u32::from(building.is_none())),
            EsfNode::Bool(true),
            EsfNode::U32(total - c.turns_remaining.min(total)),
            EsfNode::U32(total),
            EsfNode::U32(c.cost as u32),
            EsfNode::Utf16String(c.level_key.clone()),
        ];
        children.push(EsfNode::Record(Box::new(rec)));
    }
    bm.children = children;
    Ok(())
}

/// One land recruitment item, laid out as in the user's saves (W3 §3.4): outer
/// `RECRUITMENT_ITEM` v2 → `LAND_UNIT_RECRUITMENT_ITEM` v1 {inner `RECRUITMENT_ITEM` v2, u32 0}.
/// Inner: #0 i32 the item's id (`RecruitmentItem::id`; pointer-like and unique in the original: a new
/// item's comes from `World::alloc_id`, above every id of the model and the source),
/// #1 i32 region id (CONFIRMED match), #2 the target commander's id or 0 (INFERRED: an id like #1's; item +0x18,
/// `0x00B5C900`), #3 turns, #4 cost, #5 true, #6 unit key, #7 cost, #8 0, #9 false, #10 0, #11 0,
/// #12 true (constants as in every sample; meanings UNKNOWN).
fn recruitment_item(region: &Region, it: &ntw_sim::campaign::RecruitmentItem, naval: bool) -> EsfRecord {
    let rid = region.id.raw() as i32;
    let mut inner = EsfRecord::new("RECRUITMENT_ITEM", 2);
    inner.children = vec![
        EsfNode::I32(it.id.raw()),
        EsfNode::I32(rid),
        EsfNode::I32(it.target.map_or(0, |c| c.raw())),
        EsfNode::U32(it.turns_remaining),
        EsfNode::U32(it.cost.max(0) as u32),
        EsfNode::Bool(true),
        EsfNode::Utf16String(it.unit_key.clone()),
        EsfNode::U32(it.cost.max(0) as u32),
        EsfNode::U32(0),
        EsfNode::Bool(false),
        EsfNode::I32(0),
        EsfNode::U32(0),
        EsfNode::Bool(true),
    ];
    // The naval wrapper name is CONFIRMED (W3 §3.4); its version and second child are INFERRED
    // to match the land one.
    let wrapper = if naval { "NAVAL_UNIT_RECRUITMENT_ITEM" } else { "LAND_UNIT_RECRUITMENT_ITEM" };
    let mut land = EsfRecord::new(wrapper, 1);
    land.children = vec![EsfNode::Record(Box::new(inner)), EsfNode::U32(0)];
    let mut outer = EsfRecord::new("RECRUITMENT_ITEM", 2);
    outer.children = vec![EsfNode::Record(Box::new(land))];
    outer
}

/// Was this save written by NapoleonRust rather than by the original game? Tests that treat the
/// user's save folder as evidence of what the original writes must skip these
/// (`analysis/campaign/SAVE_COMPAT.md` §1). The rule:
/// * the file name starts with `NR-` (any case): the name the user gives our test saves when
///   copying them into the original's folder; or
/// * the tree is a `CAMPAIGN_SAVE_GAME` that still holds start-position record versions
///   (`CHARACTER` v12 or `SETTLEMENT` v2). The original upgrades them when it saves (CONFIRMED:
///   its `auto_save` written from our turn-1 save has only `CHARACTER` v14 / `SETTLEMENT` v3),
///   while our writer keeps the source's records, so every save we write from a start position
///   has them.
///
/// A save we write from an original save (load, play, save) has no structural marker; only the
/// name rule catches it.
pub fn written_by_napoleonrust(file_name: &str, esf: Option<&EsfFile>) -> bool {
    if file_name.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("NR-")) {
        return true;
    }
    let Some(esf) = esf else { return false };
    if esf.root.name != SAVE_ROOT {
        return false;
    }
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return false };
    let old_char = world
        .record_array("FACTION_ARRAY")
        .into_iter()
        .flat_map(|a| a.records())
        .flat_map(|f| f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()))
        .any(|c| c.name == "CHARACTER" && c.version < 14);
    let old_settlement = world
        .child("REGION_MANAGER")
        .and_then(|m| m.record_array("REGIONS_ARRAY"))
        .into_iter()
        .flat_map(|a| a.records())
        .filter_map(|r| r.child("SETTLEMENT"))
        .any(|s| s.version < 3);
    old_char || old_settlement
}

/// [`write_save`] and serialise with the existing ESF writer.
pub fn save_bytes(source: &EsfFile, model: &CampaignModel, human: &str, timestamp: u32) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(write_save(source, model, human, timestamp)?.to_bytes()?)
}


/// `FACTION` #55 (the second `CAMPAIGN_BONUS_VALUES`, faction +0x8D4 = base + difficulty handicap,
/// EFFECTS_FIDELITY.md §1) from the model, when it differs from the source: one
/// `CAMPAIGN_BONUS_VALUE_BLOCK` item per entry holding `CAMPAIGN_BONUS_VALUE` {u32 type, i32 bonus, f32
/// value} plus the qualifier string for non-basic types (as the original writes: type 0 agent caps
/// carry it, type 1 entries do not; other types INFERRED). Record versions are taken from the source.
fn write_bonus_values(f: &mut EsfRecord, details: Option<&ntw_sim::campaign::FactionDetails>) {
    let Some(d) = details else { return };
    if d.bonus_with_difficulty.is_empty() {
        return;
    }
    let Some(rec) = f.children.iter_mut().filter_map(|c| match c {
        EsfNode::Record(r) if r.name == "CAMPAIGN_BONUS_VALUES" => Some(r.as_mut()),
        _ => None,
    }).nth(1) else { return };
    let Some(arr) = array_mut(rec, "CAMPAIGN_BONUS_VALUE_BLOCK") else { return };
    let current: Vec<ntw_sim::campaign::effects::SavedBonus> = arr
        .items
        .iter()
        .filter_map(|it| {
            let v = first_rec(it)?;
            Some(ntw_sim::campaign::effects::SavedBonus {
                kind: v.get_u32(0)?,
                bonus: v.get_i32(1)?,
                value: v.get_f32(2)?,
                qualifier: v.get_str(3).unwrap_or_default().to_string(),
            })
        })
        .collect();
    if current == d.bonus_with_difficulty {
        return;
    }
    let version = arr.items.iter().find_map(|it| first_rec(it)).map_or(0, |r| r.version);
    arr.items = d
        .bonus_with_difficulty
        .iter()
        .map(|b| {
            let mut v = EsfRecord::new("CAMPAIGN_BONUS_VALUE", version);
            v.children.extend([EsfNode::U32(b.kind), EsfNode::I32(b.bonus), EsfNode::F32(b.value)]);
            if b.kind != 1 {
                v.children.push(EsfNode::Utf16String(b.qualifier.clone()));
            }
            vec![EsfNode::Record(Box::new(v))]
        })
        .collect();
}

/// What the original leaves out of the AI factions when it starts a new campaign (CONFIRMED in its
/// own turn-1 save of a new spa_napoleon campaign, `orig_fr_t1`, and the Coalition saves: only the
/// human's `FACTION` keeps them): the `CAMPAIGN_MISSION_MANAGER` record (no flag before it) and the
/// `CAMPAIGN_SHROUD` record (its bool flag before it becomes false). The original also loads saves
/// that keep them for AI factions (its auto_save of our NR-4 keeps all five), so this is fidelity,
/// not a load fix.
fn trim_ai_faction_blocks(env: &mut EsfRecord, human: &str) {
    let Some(world) = child_mut(env, "CAMPAIGN_MODEL").and_then(|m| child_mut(m, "WORLD")) else { return };
    let Some(a) = array_mut(world, "FACTION_ARRAY") else { return };
    for item in &mut a.items {
        let Some(f) = first_rec_mut(item) else { continue };
        if f.child("CAMPAIGN_PLAYER_SETUP").and_then(|p| p.get_str(2)) == Some(human) {
            continue;
        }
        f.children.retain(|c| !matches!(c, EsfNode::Record(r) if r.name == crate::missions::MANAGER));
        if let Some(i) = f.children.iter().position(|c| matches!(c, EsfNode::Record(r) if r.name == "CAMPAIGN_SHROUD"))
            && i > 0
            && matches!(f.children[i - 1], EsfNode::Bool(true))
        {
            f.children[i - 1] = EsfNode::Bool(false);
            f.children.remove(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(key: &str, human: bool) -> Vec<EsfNode> {
        let mut p = EsfRecord::new("CAMPAIGN_PLAYER_SETUP", 3);
        p.children = vec![
            EsfNode::Record(Box::new(EsfRecord::new("CAMPAIGN_VICTORY_CONDITIONS", 0))),
            EsfNode::Record(Box::new(EsfRecord::new("CAMPAIGN_INGAME_MODIFIABLES", 0))),
            EsfNode::Utf16String(key.into()),
            EsfNode::Bool(human),
            EsfNode::Bool(true),
            EsfNode::Bool(false),
        ];
        vec![EsfNode::Record(Box::new(p))]
    }

    /// `CAMPAIGN_PLAYER_SETUP` #3 is set for the player's faction only (SAVE_COMPAT.md §2).
    #[test]
    fn human_flag_set_for_the_player_only() {
        let mut arr = EsfRecordArray::new("PLAYERS_ARRAY", 0);
        arr.items = vec![player("france", false), player("britain", true), player("austria", false)];
        let mut players = EsfRecord::new("CAMPAIGN_PLAYERS_SETUP", 0);
        players.children = vec![EsfNode::RecordArray(Box::new(arr))];
        let mut setup = EsfRecord::new("CAMPAIGN_SETUP", 3);
        setup.children = vec![EsfNode::Utf16String("eur_napoleon".into()), EsfNode::Record(Box::new(players))];
        let mut env = EsfRecord::new("CAMPAIGN_ENV", 2);
        env.children = vec![EsfNode::Bool(false), EsfNode::Bool(false), EsfNode::Record(Box::new(setup))];
        let mut root = EsfRecord::new(SAVE_ROOT, 5);
        root.children = vec![EsfNode::Record(Box::new(env))];
        let mut esf = EsfFile::new(root);
        mark_human(&mut esf, "france");
        assert_eq!(
            player_flags(&esf),
            vec![("france".to_string(), true), ("britain".to_string(), false), ("austria".to_string(), false)]
        );
    }

    fn rec(name: &str, children: Vec<EsfNode>) -> EsfNode {
        let mut r = EsfRecord::new(name, 0);
        r.children = children;
        EsfNode::Record(Box::new(r))
    }

    /// A region owned by faction 1, with nothing built or queued.
    fn test_region() -> Region {
        use ntw_sim::campaign::{FactionId, RegionId, Settlement};
        use ntw_sim::fixed::Fixed20;
        Region {
            id: RegionId(1),
            key: "test_region".into(),
            owner: FactionId(1),
            settlement: Settlement { key: "settlement:test_region:town".into(), position: (Fixed20::from_raw(0), Fixed20::from_raw(0)) },
            slots: Vec::new(),
            road: None,
            fortification: None,
            population: 0,
            base_gdp: 0,
            gdp: 0,
            wealth_growth_offset: 0,
            discontent_growth: 0,
            town_wealth: 0,
            town_wealth_growth: 0,
            tax_exempt: false,
            religions: Vec::new(),
            class_bases: Vec::new(),
            population_state: Default::default(),
            recruitment_queue: Vec::new(),
            construction: Vec::new(),
            garrison: None,
            fleet: None,
        }
    }

    /// The walls slot in both shapes the reader accepts -- `FORTIFICATION_SLOT/REGION_SLOT` and a
    /// bare `FORTIFICATION_SLOT` -- keeps its building and its construction item over a save
    /// (review: the writer handled only the wrapped shape, so a bare one lost the walls).
    #[test]
    fn walls_round_trip_in_both_slot_shapes() {
        use ntw_sim::campaign::ConstructionItem;
        let mut region = Region {
            fortification: Some(BuildingRef { level_key: "test_walls_1".into(), health: 100 }),
            construction: vec![ConstructionItem { slot: SlotRef::Walls, level_key: "test_walls_2".into(), turns_remaining: 2, cost: 300 }],
            ..test_region()
        };
        let EsfNode::Record(template) = rec("BUILDING", vec![EsfNode::U32(100), EsfNode::Utf16String(String::new()), EsfNode::Utf16String(String::new()), EsfNode::Utf16String(String::new())]) else {
            unreachable!()
        };
        // The item's int cost is written bit for bit, so a negative cost (a repair in debt, FISTP's 0x80000000)
        // survives the round trip; the cancel refund credits it back.
        for (wrapped, cost) in [(true, 300), (false, 300), (true, -20), (false, i32::MIN)] {
            region.construction[0].cost = cost;
            let manager = rec("BUILDING_MANAGER", vec![EsfNode::Bool(false), EsfNode::Bool(false)]);
            let slot = if wrapped { rec("FORTIFICATION_SLOT", vec![rec("REGION_SLOT", vec![manager])]) } else { rec("FORTIFICATION_SLOT", vec![manager]) };
            let EsfNode::Record(mut sm) = rec("REGION_SLOT_MANAGER", vec![slot]) else { unreachable!() };
            write_slot_manager(&mut sm, &region, "test_faction", "test_gov", Some(&template), &Default::default()).unwrap();
            let slot = crate::world::wrapped_slot(&sm, "FORTIFICATION_SLOT").unwrap();
            let building = slot.child("BUILDING_MANAGER").and_then(|m| m.child("BUILDING")).and_then(|b| b.get_str(1));
            assert_eq!(building, Some("test_walls_1"), "wrapped: {wrapped}");
            let mut items = Vec::new();
            crate::world::read_construction(slot, SlotRef::Walls, &mut items);
            assert_eq!(items, region.construction, "wrapped: {wrapped}");
        }
    }

    #[test]
    fn napoleonrust_saves_are_recognised_by_name() {
        assert!(written_by_napoleonrust("NR-1 new campaign turn1.save", None));
        assert!(written_by_napoleonrust("nr-x.save", None));
        assert!(!written_by_napoleonrust("Great Britain Early April 1806.save", None));
        assert!(!written_by_napoleonrust("NR", None));
    }

    /// Review (polish rounds): records were matched to queued items by id and unit key. A ship and
    /// a foot item sharing id 700 (ports are read first, so the foot one is renumbered): the ship
    /// took the foot record and was saved as a foot unit; with the kept-700 item cancelled, the
    /// renumbered one took that record; a new recruit took a cancelled item's record. Now each
    /// item keeps the record it was loaded from (`World::recruitment_sources`) and nothing else.
    #[test]
    fn each_recruitment_item_keeps_the_record_it_was_loaded_from() {
        use ntw_sim::campaign::{RecruitmentItem, RecruitmentItemId, RecruitmentSource};
        // A source item; the trailing `U32(mark)` names the record.
        let item = |id: EsfNode, key: &str, mark: u32| {
            let inner =
                rec("RECRUITMENT_ITEM", vec![id, EsfNode::I32(1), EsfNode::I32(0), EsfNode::U32(2), EsfNode::U32(300), EsfNode::Bool(true), EsfNode::Utf16String(key.into())]);
            vec![rec("RECRUITMENT_ITEM", vec![rec("UNIT_RECRUITMENT_ITEM", vec![inner, EsfNode::U32(mark)])])]
        };
        let manager = |items: Vec<Vec<EsfNode>>| {
            let mut a = EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0);
            a.items = items;
            rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::RecordArray(Box::new(a))])
        };
        let mut slots = EsfRecordArray::new("REGION_SLOT_ARRAY", 0);
        slots.items = vec![vec![rec("REGION_SLOT", vec![manager(Vec::new())])], vec![rec("REGION_SLOT", vec![manager(vec![item(EsfNode::U32(700), "ship", 5)])])]];
        let own = vec![
            item(EsfNode::I32(600), "foot", 1),
            item(EsfNode::I32(700), "foot", 2),
            item(EsfNode::Utf16String("x".into()), "foot", 3),
            item(EsfNode::I32(800), "foot", 4),
        ];
        let EsfNode::Record(mut r) = rec("REGION", vec![manager(own), rec("REGION_SLOT_MANAGER", vec![EsfNode::RecordArray(Box::new(slots))])]) else {
            unreachable!()
        };
        let q = |id: i32, key: &str, turns: u32| RecruitmentItem { id: RecruitmentItemId(id), unit_key: key.into(), turns_remaining: turns, cost: 300, target: None };
        // As loaded: ship 700 (port 1), foot 600, foot 700 renumbered 908, the id-less foot 912,
        // foot 800. Then 600 and 800 are cancelled and 920 recruited.
        // 930: a ship whose link names a foot record (a source the model was not loaded from).
        let queue = vec![q(700, "ship", 1), q(908, "foot", 2), q(912, "foot", 3), q(920, "foot", 1), q(930, "ship", 1)];
        let region = Region { recruitment_queue: queue, ..test_region() };
        let at = |port_slot: Option<usize>, index: usize| RecruitmentSource { port_slot, index };
        let sources = BTreeMap::from([
            (RecruitmentItemId(700), at(Some(1), 0)),
            (RecruitmentItemId(600), at(None, 0)),
            (RecruitmentItemId(908), at(None, 1)),
            (RecruitmentItemId(912), at(None, 2)),
            (RecruitmentItemId(800), at(None, 3)),
            (RecruitmentItemId(930), at(None, 0)),
        ]);
        let mut links = RecruitmentLinks::new(&sources);
        write_region(&mut r, &region, None, None, &|k| k == "ship", &mut links, &Default::default()).unwrap();
        assert_eq!(links.broken, 1, "the ship linked to a foot record is counted (and logged once per save)");
        // (id, unit key, turns, mark) of each record.
        type Fields = (Option<EsfNode>, Option<String>, Option<u32>, Option<u32>);
        let fields = |m: Option<usize>, r: &mut EsfRecord| -> Vec<Fields> {
            manager_items(r, m)
                .iter()
                .map(|e| {
                    let wrapper = first_rec(e).and_then(|o| o.children.first()).and_then(EsfNode::as_record).unwrap();
                    let inner = wrapper.child("RECRUITMENT_ITEM").unwrap();
                    (inner.children.first().cloned(), inner.get_str(6).map(str::to_owned), inner.get_u32(3), wrapper.get_u32(1))
                })
                .collect()
        };
        let own = fields(None, &mut r);
        let foot = Some("foot".to_owned());
        assert_eq!(own[..2], [(Some(EsfNode::I32(908)), foot.clone(), Some(2), Some(2)), (Some(EsfNode::I32(912)), foot.clone(), Some(3), Some(3))]);
        assert_eq!((own.len(), &own[2].1), (3, &foot));
        assert!(own[2].3 != Some(1) && own[2].3 != Some(4), "the new recruit is built, not given a cancelled item's record");
        let port0 = fields(Some(0), &mut r);
        assert_eq!(port0.iter().map(|f| f.1.as_deref()).collect::<Vec<_>>(), [Some("ship")], "the mismatched ship is built fresh");
        assert_eq!(links.misplaced, 0);
        assert_eq!(fields(Some(1), &mut r), vec![(Some(EsfNode::U32(700)), Some("ship".to_owned()), Some(1), Some(5))], "the ship keeps its own record and type");
    }

    /// The unit keys of a manager's items.
    fn unit_keys(r: &mut EsfRecord, slot: Option<usize>) -> Vec<String> {
        manager_items(r, slot).iter().filter_map(|e| recruitment_unit_key(e).map(str::to_owned)).collect()
    }

    fn new_item(id: i32, key: &str) -> ntw_sim::campaign::RecruitmentItem {
        ntw_sim::campaign::RecruitmentItem { id: ntw_sim::campaign::RecruitmentItemId(id), unit_key: key.into(), turns_remaining: 1, cost: 0, target: None }
    }

    /// The array item of a queued unit as a file holds it, with the values of [`new_item`] (1 turn, cost 0).
    fn loaded_item(id: i32, key: &str) -> Vec<EsfNode> {
        let inner = rec(
            "RECRUITMENT_ITEM",
            vec![EsfNode::I32(id), EsfNode::I32(1), EsfNode::I32(0), EsfNode::U32(1), EsfNode::U32(0), EsfNode::Bool(true), EsfNode::Utf16String(key.into())],
        );
        vec![rec("RECRUITMENT_ITEM", vec![rec("LAND_UNIT_RECRUITMENT_ITEM", vec![inner, EsfNode::U32(0)])])]
    }

    /// An empty `REGION_RECRUITMENT_MANAGER` with its array (as `rec` builds it: version 0).
    fn full_manager() -> EsfNode {
        rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::RecordArray(Box::new(EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0)))])
    }

    /// Polish: a malformed REGION without its recruitment manager, or a manager without its item array,
    /// lost queued items or got a duplicate manager at save time. The save's copy is now repaired once,
    /// by one rule: a missing part is appended after its parent's last child (for a record holding
    /// exactly #0..#26 that is #27; our loader reads both by type, so nothing moves).
    #[test]
    fn a_malformed_region_record_is_repaired_once() {
        for len in [26, 27, 30] {
            let EsfNode::Record(mut r) = rec("REGION", (0..len).map(|_| EsfNode::U32(7)).collect()) else { unreachable!() };
            assert_eq!(repair_recruitment_managers(&mut r, &BTreeSet::from([None])), ["REGION_RECRUITMENT_MANAGER"], "len {len}");
            assert_eq!(r.children.len(), len + 1);
            assert_eq!(r.children.iter().filter(|c| matches!(c, EsfNode::U32(7))).count(), len, "no field replaced");
            let m = r.children[len].as_record().unwrap();
            assert_eq!((m.name.as_str(), m.version, m.get_bool(1), m.get_i32(2)), ("REGION_RECRUITMENT_MANAGER", 1, Some(false), Some(0)));
            assert!(m.record_array("REGION_RECRUITMENT_ITEM_ARRAY").is_some());
            assert!(repair_recruitment_managers(&mut r, &BTreeSet::from([None])).is_empty(), "once repaired, nothing more to add");
        }
        // A manager without its array (holding its bool and i32, or empty in a port slot).
        let mut slots = EsfRecordArray::new("REGION_SLOT_ARRAY", 0);
        slots.items = vec![vec![rec("REGION_SLOT", vec![rec("REGION_RECRUITMENT_MANAGER", Vec::new())])]];
        let own = rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::Bool(false), EsfNode::I32(5)]);
        let EsfNode::Record(mut r) = rec("REGION", vec![own, rec("REGION_SLOT_MANAGER", vec![EsfNode::RecordArray(Box::new(slots))])]) else { unreachable!() };
        // Only the managers named are repaired: the region's own first, the port's when a ship goes there.
        assert_eq!(repair_recruitment_managers(&mut r, &BTreeSet::from([None])), ["REGION_RECRUITMENT_ITEM_ARRAY"]);
        assert!(manager(&mut r, Some(0)).is_none(), "the port is not touched");
        assert_eq!(repair_recruitment_managers(&mut r, &BTreeSet::from([None, Some(0)])), ["port REGION_RECRUITMENT_ITEM_ARRAY"]);
        let m = r.child("REGION_RECRUITMENT_MANAGER").unwrap();
        assert_eq!((m.get_bool(0), m.get_i32(1), m.children.len()), (Some(false), Some(5), 3), "appended after the fields");
        assert_eq!(r.children.iter().filter(|c| c.as_record().is_some_and(|x| x.name == "REGION_RECRUITMENT_MANAGER")).count(), 1, "no second manager");
        assert!(manager(&mut r, None).is_some() && manager(&mut r, Some(0)).is_some());
    }

    /// Polish: one new land item made the repair add an item array to every port manager without one
    /// as well, changing more of the record than the item needs. Only the manager the item goes to is
    /// repaired now: the rest of the record is exactly as it was.
    #[test]
    fn a_new_land_item_changes_only_the_regions_own_manager() {
        let mut slots = EsfRecordArray::new("REGION_SLOT_ARRAY", 0);
        let port_without_array = || vec![rec("REGION_SLOT", vec![rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::Bool(false), EsfNode::I32(3)])])];
        slots.items = vec![port_without_array(), vec![rec("REGION_SLOT", Vec::new())], port_without_array()];
        let mut children: Vec<EsfNode> = (0..27).map(|_| EsfNode::U32(0)).collect();
        children[3] = rec("REGION_SLOT_MANAGER", vec![EsfNode::RecordArray(Box::new(slots))]);
        children[20] = EsfNode::U32(1);
        for own_has_array in [true, false] {
            let own = if own_has_array { full_manager() } else { rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::Bool(false), EsfNode::I32(5)]) };
            let mut c = children.clone();
            c.push(own);
            let EsfNode::Record(mut r) = rec("REGION", c) else { unreachable!() };
            let before = r.clone();
            let sources = BTreeMap::new();
            let mut links = RecruitmentLinks::new(&sources);
            let region = Region { recruitment_queue: vec![new_item(8, "foot")], ..test_region() };
            write_region(&mut r, &region, None, None, &|k| k == "ship", &mut links, &Default::default()).unwrap();
            assert_eq!(unit_keys(&mut r, None), ["foot"]);
            assert_eq!(links.repaired.len(), usize::from(!own_has_array));
            // The expected record: the source with only the region's own manager changed (holding the item).
            let mut expected = before.clone();
            *manager_record(&mut expected, None).unwrap() = manager_record(&mut r, None).unwrap().clone();
            assert_eq!(r, expected, "own array: {own_has_array}");
            assert!(manager(&mut r, Some(0)).is_none() && manager(&mut r, Some(2)).is_none(), "no port gets an array");
        }
    }

    /// Polish: the recruitment writer took every manager's items before it looked up where a new item
    /// goes, so a failed lookup returned an error with the managers emptied. Every item is resolved
    /// first now: a failure leaves the record as it was.
    #[test]
    fn a_failed_recruitment_write_leaves_the_managers_as_they_were() {
        use ntw_sim::campaign::RecruitmentSource;
        let mut items = EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0);
        items.items = vec![loaded_item(8, "foot")];
        let mut children: Vec<EsfNode> = (0..27).map(|_| EsfNode::U32(0)).collect();
        children.push(rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::RecordArray(Box::new(items))]));
        let EsfNode::Record(mut r) = rec("REGION", children) else { unreachable!() };
        let before = r.clone();
        // A linked item, then a new ship sent to a port manager that does not exist (a state the
        // repair never leaves, forced here: slot 4 is not there).
        let region = Region { recruitment_queue: vec![new_item(8, "foot"), new_item(9, "ship")], ..test_region() };
        let dests = [QueueDest::Linked(RecruitmentSource { port_slot: None, index: 0 }), QueueDest::New(Some(4))];
        assert!(matches!(write_recruitment(&mut r, &region, &dests, &|k| k == "ship"), Err(SaveError::Missing(_))));
        assert_eq!(r, before, "nothing written");
        // The same with the ship sent to the region's own manager is written.
        let dests = [QueueDest::Linked(RecruitmentSource { port_slot: None, index: 0 }), QueueDest::New(None)];
        write_recruitment(&mut r, &region, &dests, &|k| k == "ship").unwrap();
        assert_eq!(unit_keys(&mut r, None), ["foot", "ship"]);
    }

    /// A region with a new item gets its malformed record repaired before any item is written, so no
    /// queued item is lost: a record without its manager gets one and keeps the item. Every new item
    /// is kept whatever the queue order: a land unit in the region's own manager, a ship in the port's,
    /// or (PROVISIONAL, §32) in the region's when there is no port.
    #[test]
    fn new_items_go_to_their_manager_in_any_order() {
        let sources = BTreeMap::new();
        let ships = |k: &str| k == "ship";
        let EsfNode::Record(mut bare) = rec("REGION", (0..27).map(|_| EsfNode::U32(7)).collect()) else { unreachable!() };
        let region = Region { recruitment_queue: vec![new_item(8, "foot")], ..test_region() };
        let mut links = RecruitmentLinks::new(&sources);
        write_region(&mut bare, &region, None, None, &ships, &mut links, &Default::default()).unwrap();
        assert_eq!(links.repaired, ["REGION 1: added REGION_RECRUITMENT_MANAGER"]);
        assert_eq!(unit_keys(&mut bare, None), ["foot"]);
        for with_port in [false, true] {
            for order in [["foot", "ship"], ["ship", "foot"]] {
                let mut slots = EsfRecordArray::new("REGION_SLOT_ARRAY", 0);
                slots.items = vec![vec![rec("REGION_SLOT", if with_port { vec![full_manager()] } else { Vec::new() })]];
                let mut children: Vec<EsfNode> = (0..27).map(|_| EsfNode::U32(7)).collect();
                children[3] = rec("REGION_SLOT_MANAGER", vec![EsfNode::RecordArray(Box::new(slots))]);
                let EsfNode::Record(mut r) = rec("REGION", children) else { unreachable!() };
                let region = Region { recruitment_queue: vec![new_item(8, order[0]), new_item(9, order[1])], ..test_region() };
                let mut links = RecruitmentLinks::new(&sources);
                write_region(&mut r, &region, None, None, &ships, &mut links, &Default::default()).unwrap();
                if with_port {
                    assert_eq!((unit_keys(&mut r, None), unit_keys(&mut r, Some(0)), links.misplaced), (vec!["foot".to_string()], vec!["ship".to_string()], 0));
                } else {
                    assert_eq!((unit_keys(&mut r, None), links.misplaced), (order.map(String::from).to_vec(), 1), "{order:?}");
                    let (mut queue, mut read) = (Vec::new(), Vec::new());
                    let db = ntw_data::GameDatabase::test_fixture();
                    let mut check = crate::world::Checker { db: &db, warnings: Vec::new() };
                    crate::world::read_recruitment(r.child("REGION_RECRUITMENT_MANAGER").unwrap(), "test_region", None, &mut queue, &mut read, &mut check);
                    assert_eq!(queue.iter().map(|i| i.unit_key.as_str()).collect::<Vec<_>>(), order, "our loader reads both back");
                }
            }
        }
    }

    /// Review: the repair ran on every region, so saving a loaded file untouched gave its malformed
    /// records empty managers and arrays. Only a region with a new item is repaired now: a record whose
    /// queue is empty, or whose items all keep their loaded records, is written back exactly as it was.
    #[test]
    fn a_malformed_record_without_new_items_is_left_as_it_was() {
        use ntw_sim::campaign::{RecruitmentItemId, RecruitmentSource};
        // Fields that already hold what `write_region` writes for `test_region` (owner 1 at #20, zeros).
        let mut fields: Vec<EsfNode> = (0..27).map(|_| EsfNode::U32(0)).collect();
        fields[20] = EsfNode::U32(1);
        // No manager at all, nothing queued.
        let EsfNode::Record(mut r) = rec("REGION", fields.clone()) else { unreachable!() };
        let before = r.clone();
        let none = BTreeMap::new();
        let mut links = RecruitmentLinks::new(&none);
        write_region(&mut r, &test_region(), None, None, &|_| false, &mut links, &Default::default()).unwrap();
        assert_eq!(r, before);
        assert!(links.repaired.is_empty());
        // The region's own manager without its array; a ship queued in the port, loaded from there.
        let mut port = EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0);
        port.items = vec![loaded_item(8, "ship")];
        let mut slots = EsfRecordArray::new("REGION_SLOT_ARRAY", 0);
        slots.items = vec![vec![rec("REGION_SLOT", vec![rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::RecordArray(Box::new(port))])])]];
        let mut children = fields;
        children[3] = rec("REGION_SLOT_MANAGER", vec![EsfNode::RecordArray(Box::new(slots))]);
        children.push(rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::Bool(false), EsfNode::I32(5)]));
        let EsfNode::Record(mut r) = rec("REGION", children) else { unreachable!() };
        let before = r.clone();
        let sources = BTreeMap::from([(RecruitmentItemId(8), RecruitmentSource { port_slot: Some(0), index: 0 })]);
        let ships = |k: &str| k == "ship";
        let mut region = Region { recruitment_queue: vec![new_item(8, "ship")], ..test_region() };
        let mut links = RecruitmentLinks::new(&sources);
        write_region(&mut r, &region, None, None, &ships, &mut links, &Default::default()).unwrap();
        assert_eq!(r, before);
        assert_eq!((links.broken, links.repaired.len()), (0, 0));
        // A new land unit needs the region's own array: now the record is repaired, and both items kept.
        region.recruitment_queue.push(new_item(9, "foot"));
        let mut links = RecruitmentLinks::new(&sources);
        write_region(&mut r, &region, None, None, &ships, &mut links, &Default::default()).unwrap();
        assert_eq!(links.repaired, ["REGION 1: added REGION_RECRUITMENT_ITEM_ARRAY"]);
        assert_eq!((unit_keys(&mut r, None), unit_keys(&mut r, Some(0))), (vec!["foot".to_string()], vec!["ship".to_string()]));
    }

    /// A well-formed record holding a linked item: the repair adds nothing and the item keeps its record.
    #[test]
    fn a_well_formed_record_with_a_linked_item_is_not_changed_by_the_repair() {
        use ntw_sim::campaign::{RecruitmentItemId, RecruitmentSource};
        let mut items = EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0);
        items.items = vec![loaded_item(8, "foot")];
        let mut children: Vec<EsfNode> = (0..27).map(|_| EsfNode::U32(7)).collect();
        children.push(rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::RecordArray(Box::new(items)), EsfNode::Bool(false), EsfNode::I32(5)]));
        let EsfNode::Record(mut r) = rec("REGION", children) else { unreachable!() };
        let before = r.clone();
        assert!(repair_recruitment_managers(&mut r, &BTreeSet::from([None])).is_empty());
        assert_eq!(r, before);
        let sources = BTreeMap::from([(RecruitmentItemId(8), RecruitmentSource { port_slot: None, index: 0 })]);
        let region = Region { recruitment_queue: vec![new_item(8, "foot")], ..test_region() };
        let mut links = RecruitmentLinks::new(&sources);
        write_region(&mut r, &region, None, None, &|_| false, &mut links, &Default::default()).unwrap();
        assert_eq!((links.broken, links.misplaced), (0, 0));
        assert_eq!(r.child("REGION_RECRUITMENT_MANAGER"), before.child("REGION_RECRUITMENT_MANAGER"), "the linked record is kept as it was");
    }

    /// Polish: a `REGION` id repeated in the file (malformed) gave every record of that id the model's
    /// region, recruitment queue and all; then the repeated record was left unwritten, so after an
    /// owner change it kept the old owner and residences. The loader reads only the first record of an
    /// id, so the save drops the others before writing: the file holds what the loader read.
    #[test]
    fn a_duplicate_region_record_is_dropped() {
        use ntw_sim::campaign::{FactionId, RegionId, World};
        let manager = || rec("REGION_RECRUITMENT_MANAGER", vec![EsfNode::RecordArray(Box::new(EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0)))]);
        let region_rec = |id: i32, owner: u32| {
            let mut children: Vec<EsfNode> = (0..21).map(|_| EsfNode::U32(0)).collect();
            children[4] = EsfNode::I32(id);
            children[20] = EsfNode::U32(owner);
            children.push(manager());
            vec![rec("REGION", children)]
        };
        let mut regions = EsfRecordArray::new("REGIONS_ARRAY", 0);
        regions.items = vec![region_rec(1, 0), region_rec(2, 0), region_rec(1, 7), region_rec(1, 8)];
        let EsfNode::Record(mut world) = rec("WORLD", vec![rec("REGION_MANAGER", vec![EsfNode::RecordArray(Box::new(regions))])]) else { unreachable!() };
        assert_eq!(drop_duplicate_regions(&mut world), [1, 1]);
        assert!(drop_duplicate_regions(&mut world).is_empty(), "nothing left to drop");
        let mut w = World::default();
        w.regions.insert(RegionId(1), Region { owner: FactionId(5), recruitment_queue: vec![new_item(8, "foot")], ..test_region() });
        let model = CampaignModel::new(
            ntw_sim::calendar::Calendar::new(ntw_sim::calendar::Date { year: 1805, season: 1, month: 0, half: ntw_sim::calendar::HALF_EARLY }, 0),
            ntw_sim::rng::CaRng::new(1),
            w,
        );
        write_regions(&mut world, &model).unwrap();
        let items = &mut array_mut(child_mut(&mut world, "REGION_MANAGER").unwrap(), "REGIONS_ARRAY").unwrap().items;
        let ids: Vec<Option<i32>> = items.iter().map(|it| first_rec(it).and_then(|r| r.get_i32(4))).collect();
        assert_eq!(ids, [Some(1), Some(2)], "the first record of each id, in file order");
        let first = first_rec_mut(&mut items[0]).unwrap();
        assert_eq!((first.get_u32(20), unit_keys(first, None)), (Some(5), vec!["foot".to_string()]));
    }

    /// Polish: saves written before the cost was stored bit for bit (commit 317f02e4) hold a negative
    /// construction cost (a repair in debt) clamped to 0. Such an item loads with cost 0, is written back
    /// unchanged, and its cancel credits nothing (`cancel_construction` refunds the stored cost).
    #[test]
    fn a_construction_item_from_the_old_clamped_writer_loads_with_cost_0() {
        // As the old writer laid out a new item: #0 0 (an upgrade), true, done, total, cost clamped to 0, key.
        let item = rec(
            "BUILDING_CONSTRUCTION_ITEM",
            vec![EsfNode::U32(0), EsfNode::Bool(true), EsfNode::U32(1), EsfNode::U32(3), EsfNode::U32(0), EsfNode::Utf16String("test_walls_2".into())],
        );
        let building = rec("BUILDING", vec![EsfNode::U32(100), EsfNode::Utf16String("test_walls_1".into()), EsfNode::Utf16String(String::new()), EsfNode::Utf16String(String::new())]);
        let manager = rec("BUILDING_MANAGER", vec![EsfNode::Bool(true), building, EsfNode::Bool(true), item]);
        let EsfNode::Record(mut sm) = rec("REGION_SLOT_MANAGER", vec![rec("FORTIFICATION_SLOT", vec![rec("REGION_SLOT", vec![manager])])]) else { unreachable!() };
        let read = |sm: &EsfRecord| {
            let mut items = Vec::new();
            crate::world::read_construction(crate::world::wrapped_slot(sm, "FORTIFICATION_SLOT").unwrap(), SlotRef::Walls, &mut items);
            items
        };
        let loaded = read(&sm);
        assert_eq!(loaded.iter().map(|c| (c.level_key.as_str(), c.turns_remaining, c.cost)).collect::<Vec<_>>(), [("test_walls_2", 2, 0)]);
        let before = sm.clone();
        let region = Region { fortification: Some(BuildingRef { level_key: "test_walls_1".into(), health: 100 }), construction: loaded.clone(), ..test_region() };
        write_slot_manager(&mut sm, &region, "test_faction", "test_gov", None, &Default::default()).unwrap();
        assert_eq!(sm, before, "the kept item is written back as it was");
        assert_eq!(read(&sm), loaded);
        let mut treasury = -50;
        ntw_sim::campaign::treasury::credit(&mut treasury, loaded[0].cost);
        assert_eq!(treasury, -50);
    }

    fn manager_items(r: &mut EsfRecord, slot: Option<usize>) -> Vec<Vec<EsfNode>> {
        manager(r, slot).map(|a| a.items.clone()).unwrap_or_default()
    }
}

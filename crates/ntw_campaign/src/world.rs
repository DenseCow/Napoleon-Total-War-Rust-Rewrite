//! `CAMPAIGN_MODEL/WORLD` → [`ntw_sim::campaign::World`].
//!
//! The ESF nests things by owner: each `FACTION` holds its own characters (`CHARACTER_ARRAY`)
//! and its own armies and navies (`ARMY_ARRAY`). The simulation keeps them in flat, id-keyed maps
//! instead and remembers the owner in a field. So we walk every faction (and the rebel faction),
//! and copy what we find into those maps.
//!
//! Every field position used here is described in `analysis/worker3/STARTPOS_LAYOUT.md`
//! (`#n` = child index inside the record).

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::campaign::{
    BuildingRef, CampaignUnit, Character, CharacterId, CharacterKind, ConstructionItem, Faction,
    FactionId, Fort, FortId, ForceId, GovernmentType, MilitaryForce, RecruitmentItem, RecruitmentItemId, RecruitmentSource,
    Region, RegionId, RegionSlot, Settlement, SlotRef, Stance, UnitId, World,
};
use ntw_sim::campaign::rules::DEFAULT_TAX_LEVEL;
use ntw_sim::fixed::Fixed20;

use crate::fields::{array, child, i32_at, rec_at, str_at, u32_at, value};
use crate::{LoadError, LoadWarning};

/// What [`load_world`] produces.
pub(crate) struct LoadedWorld {
    pub world: World,
    pub rebel_faction: Option<FactionId>,
    /// Each region's recruitment items' source records, parallel to its queue (see
    /// [`link_recruitment_sources`]).
    pub recruitment_sources: BTreeMap<RegionId, Vec<RecruitmentSource>>,
}

/// Collects warnings and checks keys against the game database.
pub(crate) struct Checker<'a> {
    pub db: &'a GameDatabase,
    pub warnings: Vec<LoadWarning>,
}

/// A faction's contents gathered in the first pass, before cross-references are checked.
struct RawFaction<'a> {
    faction: Faction,
    record: &'a EsfRecord,
    /// `None` for the rebel faction, which has no `DIPLOMACY_MANAGER`.
    diplomacy: Option<&'a EsfRecord>,
}

/// Reads `WORLD` (`path` is its ESF path, for error messages).
pub(crate) fn load_world(
    world_rec: &EsfRecord,
    path: &str,
    check: &mut Checker<'_>,
) -> Result<LoadedWorld, LoadError> {
    let mut world = World::default();

    // Pass 1: factions (ids, keys, treasury, government).
    let mut raw = Vec::new();
    let faction_array = array(world_rec, "FACTION_ARRAY", path)?;
    for (i, item) in faction_array.items.iter().enumerate() {
        let fpath = format!("{path}/FACTION_ARRAY[{i}]/FACTION");
        let rec = first_record(item, "FACTION").ok_or_else(|| LoadError::MissingRecord {
            path: fpath.clone(),
        })?;
        raw.push(read_faction(rec, &fpath, false, check)?);
    }
    let mut rebel_faction = None;
    if let Some(rebel) = world_rec
        .child("REBEL_FACTION")
        .and_then(|r| r.child("FACTION"))
    {
        let rf = read_faction(rebel, &format!("{path}/REBEL_FACTION/FACTION"), true, check)?;
        rebel_faction = Some(rf.faction.id);
        raw.push(rf);
    }
    // Turn order: FACTION_ARRAY order, then the rebels (INFERRED, see `World::turn_order`).
    for rf in &raw {
        if !world.turn_order.contains(&rf.faction.id) {
            world.turn_order.push(rf.faction.id);
        }
    }
    for rf in &raw {
        if world.factions.contains_key(&rf.faction.id) {
            check.warnings.push(LoadWarning::DuplicateId {
                kind: "faction",
                id: rf.faction.id.raw().into(),
            });
            continue;
        }
        // Details: government posts, governorship taxes, capital, religion (CAMPAIGN_DATA.md §3).
        let (details, taxes) = crate::details::faction_details(rf.record);
        let mut faction = rf.faction.clone();
        if let Some((lower, upper)) = taxes {
            faction.tax_lower = lower;
            faction.tax_upper = upper;
        }
        world.faction_details.insert(faction.id, details);
        world.factions.insert(faction.id, faction);
    }
    let faction_ids: BTreeSet<FactionId> = world.factions.keys().copied().collect();

    // Pass 2: diplomacy, characters and forces of each faction.
    // (army force id, navy force id) pairs from the transport link fields.
    let mut transport_links: Vec<(u32, u32)> = Vec::new();
    for rf in &raw {
        let fid = rf.faction.id;
        let fkey = rf.faction.key.clone();
        let fpath = format!("{path}/FACTION[{fkey}]");
        if let Some(dm) = rf.diplomacy {
            let stances = read_diplomacy(dm, &fpath, &fkey, &faction_ids, check)?;
            if let Some(f) = world.factions.get_mut(&fid) {
                f.diplomacy = stances;
            }
            for rel in dm.record_array("DIPLOMACY_RELATIONSHIPS_ARRAY").into_iter().flat_map(|a| a.records()) {
                if let Some((target, r)) = crate::details::relationship(rel)
                    && faction_ids.contains(&target)
                {
                    world.relationships.insert((fid, target), r);
                }
            }
        }
        if let Some(chars) = rf.record.record_array("CHARACTER_ARRAY") {
            for (i, item) in chars.items.iter().enumerate() {
                let cpath = format!("{fpath}/CHARACTER_ARRAY[{i}]/CHARACTER");
                let Some(rec) = first_record(item, "CHARACTER") else {
                    continue;
                };
                if let Some(c) = read_character(rec, &cpath, fid, check)? {
                    if !world.characters.contains_key(&c.id) {
                        if let Some(d) = crate::details::character_details(rec) {
                            world.character_details.insert(c.id, d);
                        }
                        // #17 f32: the sight radius (character +0x2EC, CHARACTERS_FIDELITY.md §10).
                        if let Some(r) = rec.get(17).and_then(EsfNode::as_f32) {
                            world.sight_radius.insert(c.id, r);
                        }
                    }
                    match world.characters.entry(c.id) {
                        Entry::Occupied(_) => check.warnings.push(LoadWarning::DuplicateId {
                            kind: "character",
                            id: c.id.raw().into(),
                        }),
                        Entry::Vacant(slot) => {
                            slot.insert(c);
                        }
                    }
                }
            }
        }
        if let Some(armies) = rf.record.record_array("ARMY_ARRAY") {
            for (i, item) in armies.items.iter().enumerate() {
                let Some(rec) = item.first().and_then(EsfNode::as_record) else {
                    continue;
                };
                let is_navy = match rec.name.as_str() {
                    "ARMY" => false,
                    "NAVY" => true,
                    _ => continue, // UNKNOWN item kind: skip.
                };
                let apath = format!("{fpath}/ARMY_ARRAY[{i}]/{}", rec.name);
                if let Some(other) = transport_link(rec, is_navy) {
                    let own = rec.child("MILITARY_FORCE").and_then(|m| m.get_u32(0)).unwrap_or(0);
                    transport_links.push(if is_navy { (other, own) } else { (own, other) });
                }
                let force = read_force(rec, &apath, fid, is_navy, check)?;
                if is_navy {
                    read_ship_states(rec, &mut world.ship_states);
                }
                match world.forces.entry(force.id) {
                    Entry::Occupied(_) => check.warnings.push(LoadWarning::DuplicateId {
                        kind: "force",
                        id: force.id.raw().into(),
                    }),
                    Entry::Vacant(slot) => {
                        slot.insert(force);
                    }
                }
            }
        }
    }

    // Commanders must be loaded characters.
    for force in world.forces.values_mut() {
        if let Some(c) = force.commander
            && !world.characters.contains_key(&c)
        {
            check.warnings.push(LoadWarning::DanglingCommander {
                force: force.id.raw(),
                commander: c.raw(),
            });
            force.commander = None;
        }
    }

    // Regions.
    let rm_path = format!("{path}/REGION_MANAGER");
    let rm = child(world_rec, "REGION_MANAGER", path)?;
    let regions = array(rm, "REGIONS_ARRAY", &rm_path)?;
    // Recruitment item ids seen so far, in file order (see `keep_recruitment_ids_unique`).
    let mut recruitment_ids = BTreeSet::new();
    let mut recruitment_sources = BTreeMap::new();
    for (i, item) in regions.items.iter().enumerate() {
        let rpath = format!("{rm_path}/REGIONS_ARRAY[{i}]/REGION");
        let rec = first_record(item, "REGION").ok_or_else(|| LoadError::MissingRecord {
            path: rpath.clone(),
        })?;
        let (mut region, sources) = read_region(rec, &rpath, check)?;
        // A region id repeated in the file (malformed): only the first record of an id is read, all of
        // it (forts and rebel faction too); the save drops the others (`save::drop_duplicate_regions`).
        if world.regions.contains_key(&region.id) {
            check.warnings.push(LoadWarning::DuplicateId { kind: "region", id: region.id.raw().into() });
            continue;
        }
        // REGION/FORT_ARRAY: the forts standing in this region (see `Fort`). Empty in every shipped
        // file, so this only ever fills from a file that already has forts.
        for fort in read_forts(rec, region.id, &rpath) {
            match world.forts.entry(fort.id) {
                Entry::Occupied(_) => check.warnings.push(LoadWarning::DuplicateId {
                    kind: "fort",
                    id: fort.id.raw().into(),
                }),
                Entry::Vacant(slot) => {
                    slot.insert(fort);
                }
            }
        }
        // REGION #24: the rebel faction key (CONFIRMED position, STARTPOS_LAYOUT.md §3.4); the faction a
        // capture may liberate (see `World::region_rebel_factions`).
        if let Some(k) = rec.get_str(24).filter(|k| !k.is_empty()) {
            world.region_rebel_factions.insert(region.id, k.to_string());
        }
        if !faction_ids.contains(&region.owner) {
            check.warnings.push(LoadWarning::DanglingRegionOwner {
                region: region.key.clone(),
                owner: region.owner.raw(),
            });
        }
        keep_recruitment_ids_unique(&mut region.recruitment_queue, &region.key, &mut recruitment_ids, check);
        recruitment_sources.insert(region.id, sources);
        world.regions.insert(region.id, region);
    }

    // Garrisons: keep a settlement's #12 only if it names one of the owner's armies with a
    // commander, and put that commander inside.
    let forces = world.forces.clone();
    for r in world.regions.values_mut() {
        r.garrison = r.garrison.filter(|g| forces.get(g).is_some_and(|f| !f.is_navy && f.faction == r.owner && f.commander.is_some()));
        if let Some(c) = r.garrison.and_then(|g| forces[&g].commander)
            && let Some(ch) = world.characters.get_mut(&c)
        {
            ch.garrisoned_in = Some(r.id);
        }
    }

    // Armies aboard navies. The original's record (CONFIRMED, loaders 0x00870FD0 / 0x008822F0,
    // writer 0x008FAB60): ARMY's u32 before its last bool = the force id of the navy carrying it,
    // NAVY #4 u32 = the force id of the army aboard (0 = none); the two sides are one link.
    let mut embarked = std::collections::BTreeMap::new();
    for &(army, navy) in &transport_links {
        let (a, n) = (ForceId(army), ForceId(navy));
        let ok = world.forces.get(&a).is_some_and(|f| !f.is_navy)
            && world.forces.get(&n).is_some_and(|f| f.is_navy && Some(f.faction) == world.forces.get(&a).map(|x| x.faction));
        if ok && !embarked.contains_key(&a) && !embarked.values().any(|&v| v == n) {
            embarked.insert(a, n);
        }
    }
    // Fallback only for our own older saves, written before the writer stored the link
    // (SAVE_COMPAT.md §16): no stored link at all, and the file still has start-position CHARACTER
    // records (< v14; the original upgrades them to v14 when it saves, CONFIRMED). There, an army
    // whose commander stands exactly where a navy of its faction has its commander is embarked.
    let mut startpos_records = false;
    world_rec.walk(&mut |r| {
        if r.name == "CHARACTER" && r.version < 14 {
            startpos_records = true;
        }
    });
    let fallback = transport_links.is_empty() && startpos_records;
    for army in world.forces.values().filter(|f| !f.is_navy && fallback) {
        let Some(ac) = army.commander.and_then(|c| world.characters.get(&c)) else { continue };
        if ac.garrisoned_in.is_some() {
            continue;
        }
        let navy = world.forces.values().find(|n| {
            n.is_navy
                && n.faction == army.faction
                && n.commander.and_then(|c| world.characters.get(&c)).is_some_and(|nc| nc.position == ac.position)
        });
        if let Some(n) = navy
            && !embarked.contains_key(&army.id)
            && !embarked.values().any(|&v| v == n.id)
        {
            embarked.insert(army.id, n.id);
        }
    }
    world.embarked = embarked;

    // Script missions: re-link their saved target ids as the original does after loading.
    let settlement_region = crate::missions::settlement_regions(rm);
    for rf in &raw {
        let missions: Vec<_> = crate::missions::find_managers(rf.record)
            .into_iter()
            .filter_map(|(_, m)| crate::missions::read_manager(m).ok())
            .flat_map(|m| m.missions)
            .map(|m| crate::missions::to_model(&m, &world, &settlement_region))
            .collect();
        if !missions.is_empty() {
            world.missions.insert(rf.faction.id, missions);
        }
        // FACTION_ECONOMICS #3 u32 = the economics object's +0x460, the bankrupt turns in a row
        // (CONFIRMED: written by the economics saver 0x00BD46E0 as its last field).
        if let Some(n) = rf.record.child("FACTION_ECONOMICS").and_then(|e| e.get_u32(3)).filter(|&n| n != 0) {
            world.bankrupt_turns.insert(rf.faction.id, n);
        }
        // The last turn's income: categories 5..11 = #1 and #2 of the last ECONOMICS_DATA of
        // FACTION_ECONOMICS #0 (the saver 0x00B985B0 writes the 25 categories as groups
        // 5/3/4/1/5/7; `0x00BBCC40` sums 5..11, UI_FIDELITY.md 4.7).
        if let Some(n) = last_income(rf.record) {
            world.last_income.insert(rf.faction.id, n);
        }
    }

    Ok(LoadedWorld {
        world,
        rebel_faction,
        recruitment_sources,
    })
}

/// The last turn's income of a `FACTION`: the sum of `ECONOMICS_DATA` #1 and #2 (categories 5..11)
/// of the last item of `FACTION_ECONOMICS` #0; `None` without a history.
fn last_income(faction: &EsfRecord) -> Option<i32> {
    let last = faction.child("FACTION_ECONOMICS")?.get(0)?.as_record_array()?.items.last()?;
    let data = first_record(last, "ECONOMICS_DATA")?;
    let sum = |i: usize| data.get(i).and_then(EsfNode::as_i32_array).map_or(0i32, |a| a.iter().fold(0i32, |s, v| s.wrapping_add(*v)));
    Some(sum(1).wrapping_add(sum(2)))
}

/// The first record in a record-array item, if it has the expected name.
fn first_record<'a>(item: &'a [EsfNode], name: &str) -> Option<&'a EsfRecord> {
    item.first()
        .and_then(EsfNode::as_record)
        .filter(|r| r.name == name)
}

/// `FACTION` v18. Its child positions differ between factions (optional records such as
/// `CAMPAIGN_MISSION_MANAGER` come and go), so the id and key are read as the first plain values:
/// i32 id, utf16 key, utf16 display name (CONFIRMED in all 8 startpos files and the saves).
fn read_faction<'a>(
    rec: &'a EsfRecord,
    path: &str,
    is_rebel: bool,
    check: &mut Checker<'_>,
) -> Result<RawFaction<'a>, LoadError> {
    let bad = |index, expected| LoadError::BadField {
        path: path.to_string(),
        index,
        expected,
        found: value(rec, index).map(EsfNode::type_name),
    };
    let id = value(rec, 0)
        .and_then(EsfNode::as_i32)
        .ok_or_else(|| bad(0, "i32 (plain value 0)"))?;
    let key = value(rec, 1)
        .and_then(EsfNode::as_str)
        .ok_or_else(|| bad(1, "string (plain value 1)"))?;

    // Treasury: FACTION_ECONOMICS #1 i32 (W3 §3.3; values match the documented start funds).
    let econ_path = format!("{path}/FACTION_ECONOMICS");
    let treasury = i32_at(child(rec, "FACTION_ECONOMICS", path)?, 1, &econ_path)?;

    // Government: the record name of the single GOV_IMP item (CONFIRMED names).
    let gov_name = rec
        .child("GOVERNMENT")
        .and_then(|g| g.record_array("GOV_IMP"))
        .and_then(|a| a.records().next())
        .map(|r| r.name.clone())
        .unwrap_or_default();
    let government = match GovernmentType::from_esf_name(&gov_name) {
        Some(g) => g,
        None => {
            // The rebel faction has no GOVERNMENT block at all; that is expected, not a warning.
            if !is_rebel {
                check.warnings.push(LoadWarning::UnknownGovernment {
                    faction: key.to_string(),
                    name: gov_name,
                });
            }
            GovernmentType::AbsoluteMonarchy // PLACEHOLDER
        }
    };
    // GOVERNMENT #1: the government_types key (e.g. "gov_empire"; CONFIRMED in the eur startpos).
    let government_key = rec
        .child("GOVERNMENT")
        .and_then(|g| g.get_str(1))
        .unwrap_or_default()
        .to_string();
    if !is_rebel && check.db.faction(key).is_none() {
        check
            .warnings
            .push(LoadWarning::UnknownFactionKey(key.to_string()));
    }
    Ok(RawFaction {
        faction: Faction {
            id: FactionId(id),
            key: key.to_string(),
            treasury,
            government,
            government_key,
            tax_lower: DEFAULT_TAX_LEVEL.to_string(),
            tax_upper: DEFAULT_TAX_LEVEL.to_string(),
            diplomacy: BTreeMap::new(),
        },
        record: rec,
        diplomacy: rec.child("DIPLOMACY_MANAGER"),
    })
}

/// `DIPLOMACY_MANAGER/DIPLOMACY_RELATIONSHIPS_ARRAY[]/DIPLOMACY_RELATIONSHIP` v14:
/// #0 i32 target faction id, #4 utf16 stance (CONFIRMED). Every stored stance is kept, including
/// "neutral", exactly as the file has it.
fn read_diplomacy(
    dm: &EsfRecord,
    fpath: &str,
    fkey: &str,
    factions: &BTreeSet<FactionId>,
    check: &mut Checker<'_>,
) -> Result<BTreeMap<FactionId, Stance>, LoadError> {
    let dpath = format!("{fpath}/DIPLOMACY_MANAGER");
    let mut out = BTreeMap::new();
    for (i, rel) in array(dm, "DIPLOMACY_RELATIONSHIPS_ARRAY", &dpath)?
        .records()
        .enumerate()
    {
        let rpath = format!("{dpath}/DIPLOMACY_RELATIONSHIPS_ARRAY[{i}]/DIPLOMACY_RELATIONSHIP");
        let target = i32_at(rel, 0, &rpath)?;
        let stance_str = str_at(rel, 4, &rpath)?;
        if !factions.contains(&FactionId(target)) {
            check.warnings.push(LoadWarning::DanglingDiplomacy {
                faction: fkey.into(),
                target,
            });
            continue;
        }
        match Stance::from_esf_name(stance_str) {
            Some(s) => {
                out.insert(FactionId(target), s);
            }
            None => check.warnings.push(LoadWarning::UnknownStance {
                faction: fkey.into(),
                stance: stance_str.into(),
            }),
        }
    }
    Ok(out)
}

/// `CHARACTER` v12 (startpos) / v14 (1.3 saves): #0 `LOCOMOTABLE`, #2 i32 id, #3 utf16 type.
/// `LOCOMOTABLE` v2: #0/#1 i32 x/z (20-bit fixed point, CONFIRMED), #8/#9 i32 movement points
/// (INFERRED current/max).
fn read_character(
    rec: &EsfRecord,
    path: &str,
    faction: FactionId,
    check: &mut Checker<'_>,
) -> Result<Option<Character>, LoadError> {
    let id = i32_at(rec, 2, path)?;
    let kind_str = str_at(rec, 3, path)?;
    let lpath = format!("{path}/LOCOMOTABLE");
    let loco = rec_at(rec, 0, path)?;
    let x = i32_at(loco, 0, &lpath)?;
    let z = i32_at(loco, 1, &lpath)?;
    // #8 = the type's base action points (CONFIRMED = `agents` #1), #9 = action points left
    // (INFERRED from the saves, see `Character::movement_points`).
    let max_mp = i32_at(loco, 8, &lpath)?;
    let mp = i32_at(loco, 9, &lpath)?;
    let Some(kind) = CharacterKind::from_esf_name(kind_str) else {
        check.warnings.push(LoadWarning::UnknownCharacterType {
            id,
            kind: kind_str.into(),
        });
        return Ok(None);
    };
    Ok(Some(Character {
        id: CharacterId(id),
        faction,
        kind,
        position: (Fixed20::from_raw(x), Fixed20::from_raw(z)),
        movement_points: mp,
        max_movement_points: max_mp,
        base_movement_points: max_mp,
        garrisoned_in: None,
    }))
}

/// `ARMY` v2 / `NAVY` v1: #0 `MILITARY_FORCE` {#0 u32 force id, #1 u32 commander character id
/// (0 = none)}, #1 `UNITS_ARRAY` of `LAND_UNIT` / `NAVAL_UNIT` (CONFIRMED).
fn read_force(
    rec: &EsfRecord,
    path: &str,
    faction: FactionId,
    is_navy: bool,
    check: &mut Checker<'_>,
) -> Result<MilitaryForce, LoadError> {
    let mpath = format!("{path}/MILITARY_FORCE");
    let mf = child(rec, "MILITARY_FORCE", path)?;
    let id = u32_at(mf, 0, &mpath)?;
    let commander_raw = u32_at(mf, 1, &mpath)?;
    // The commander is stored as u32 but character ids are i32: same 32 bits (CONFIRMED by value
    // match with CHARACTER #2).
    let commander = (commander_raw != 0).then_some(CharacterId(commander_raw as i32));
    let mut units = Vec::new();
    for (i, item) in array(rec, "UNITS_ARRAY", path)?.items.iter().enumerate() {
        let Some(wrapper) = item.first().and_then(EsfNode::as_record) else {
            continue;
        };
        let upath = format!("{path}/UNITS_ARRAY[{i}]/{}/UNIT", wrapper.name);
        let unit_rec = child(
            wrapper,
            "UNIT",
            &format!("{path}/UNITS_ARRAY[{i}]/{}", wrapper.name),
        )?;
        let unit = read_unit(unit_rec, &upath)?;
        if check.db.unit(&unit.unit_key).is_none() {
            check.warnings.push(LoadWarning::UnknownUnitKey {
                force: id,
                key: unit.unit_key.clone(),
            });
        }
        units.push(unit);
    }
    Ok(MilitaryForce {
        id: ForceId(id),
        faction,
        commander,
        units,
        is_navy,
    })
}

/// `NAVAL_UNIT` #5 `SHIP_DAMAGE_INFO` v2 {f32 ×5 damage, i32 ×3 crews, i32 ×3 full crews, u32 guns, bool sunk,
/// u32 full guns} (CONFIRMED layout; meanings from the exe's uses, `ntw_sim::campaign::naval::ShipState`).
fn read_ship_states(rec: &EsfRecord, out: &mut std::collections::BTreeMap<UnitId, ntw_sim::campaign::naval::ShipState>) {
    let Some(units) = rec.record_array("UNITS_ARRAY") else { return };
    for u in units.records().filter(|u| u.name == "NAVAL_UNIT") {
        let (Some(unit), Some(d)) = (u.child("UNIT"), u.child("SHIP_DAMAGE_INFO")) else { continue };
        let Some(id) = unit.get_i32(4) else { continue };
        let f = |i: usize| d.get(i).and_then(EsfNode::as_f32).unwrap_or(0.0);
        let n = |i: usize| d.get(i).and_then(EsfNode::as_int).unwrap_or(0) as i32;
        out.insert(
            UnitId(id),
            ntw_sim::campaign::naval::ShipState {
                damage: [f(0), f(1), f(2), f(3), f(4)],
                crews: [n(5), n(6), n(7)],
                max_crews: [n(8), n(9), n(10)],
                guns: n(11) as u32,
                sunk: d.get_bool(12).unwrap_or(false),
                max_guns: n(13) as u32,
            },
        );
    }
}

/// `UNIT` v3: #0 `UNIT_RECORD_KEY` {utf16 key}, #4 i32 unit id, #5 u32 men, #6 u32 max men
/// (CONFIRMED structure; men/max INFERRED meaning).
fn read_unit(rec: &EsfRecord, path: &str) -> Result<CampaignUnit, LoadError> {
    let key_rec = child(rec, "UNIT_RECORD_KEY", path)?;
    Ok(CampaignUnit {
        id: UnitId(i32_at(rec, 4, path)?),
        unit_key: str_at(key_rec, 0, &format!("{path}/UNIT_RECORD_KEY"))?.to_string(),
        men: u32_at(rec, 5, path)?,
        max_men: u32_at(rec, 6, path)?,
        // #10: the attached character (CONFIRMED, see `CampaignUnit::character`).
        character: rec.get_u32(10).filter(|&c| c != 0).map(|c| CharacterId(c as i32)),
        // The officer's name: `COMMANDER_DETAILS` #0 / #1 {utf16 localisation key} (CONFIRMED form,
        // SAVE_COMPAT.md §21); empty when the record is not there.
        officer_name: {
            let cd = rec.child("COMMANDER_DETAILS");
            let name = |i: usize| cd.and_then(|cd| cd.get(i)).and_then(EsfNode::as_record).and_then(|l| l.get_str(0)).unwrap_or("").to_string();
            (name(0), name(1))
        },
    })
}

/// `REGION` v5: #0 utf16 key, #1 `POPULATION` {#1 u32 total}, #3 `REGION_SLOT_MANAGER`,
/// #4 i32 region id, #5 `SETTLEMENT`, #20 u32 owner faction id (CONFIRMED by cross-check with
/// `CAMPAIGN_PREOPEN_MAP_INFO` ownership), #27 `REGION_RECRUITMENT_MANAGER`. The recruitment
/// managers (the region's at #27, a port slot's) and their `REGION_RECRUITMENT_ITEM_ARRAY` are found
/// by type, not position: a malformed record without them reads as an empty queue, with a
/// `LoadWarning`, and the save repairs the manager a new item goes to
/// (`save::repair_recruitment_managers`, SAVE_COMPAT.md §32).
fn read_region(rec: &EsfRecord, path: &str, check: &mut Checker<'_>) -> Result<(Region, Vec<RecruitmentSource>), LoadError> {
    let key = str_at(rec, 0, path)?.to_string();
    // Region id: stored as i32; `RegionId` wraps a u32, so we keep the same 32 bits.
    let id = i32_at(rec, 4, path)? as u32;
    let owner = u32_at(rec, 20, path)? as i32;
    let pop_rec = child(rec, "POPULATION", path)?;
    let total = u32_at(pop_rec, 1, &format!("{path}/POPULATION"))?;
    // `REGION_FACTORS` (the reader `0x00A4C340`, CONFIRMED layout): #0 f32[7] growth factors, #2 the live
    // population, #3 capacity, #4 base capacity, #5 growth, #6 trend, #7 overcrowded, #8 migrants. Without
    // the record the population is `POPULATION` #1 (equal to #2 until it has grown) and the state starts
    // empty, as a new population object does (`0x00A4C340`'s defaults: trend 2).
    let factors_rec = pop_rec.child("REGION_FACTORS");
    let population = factors_rec.and_then(|f| f.get_u32(2)).unwrap_or(total);
    let population_state = match factors_rec {
        Some(f) => {
            let mut factors = [0.0f32; 7];
            if let Some(a) = f.get(0).and_then(EsfNode::as_f32_array) {
                for (d, s) in factors.iter_mut().zip(a) {
                    *d = *s;
                }
            }
            ntw_sim::campaign::population::PopulationState {
                factors,
                capacity: f.get_u32(3).unwrap_or(0),
                base_capacity: f.get_u32(4).unwrap_or(0),
                growth: f.get(5).and_then(EsfNode::as_f32).unwrap_or(0.0),
                trend: f.get_u32(6).unwrap_or(2),
                overcrowded: f.get_bool(7).unwrap_or(false),
                migrants: f.get_i32(8).unwrap_or(0),
            }
        }
        None => ntw_sim::campaign::population::PopulationState { trend: 2, ..Default::default() },
    };

    let spath = format!("{path}/SETTLEMENT");
    let settlement = child(rec, "SETTLEMENT", path)?;
    let gpath = format!("{spath}/SIEGEABLE_GARRISON_RESIDENCE");
    let garrison_res = child(settlement, "SIEGEABLE_GARRISON_RESIDENCE", &spath)?;
    let settlement = Settlement {
        key: str_at(settlement, 3, &spath)?.to_string(),
        position: (
            Fixed20::from_raw(i32_at(garrison_res, 10, &gpath)?),
            Fixed20::from_raw(i32_at(garrison_res, 11, &gpath)?),
        ),
    };

    // Slots: REGION_SLOT_MANAGER/REGION_SLOT_ARRAY[]/REGION_SLOT {#0 garrison residence,
    // #1 BUILDING_MANAGER, #2 u32 id, #3 utf16 key, ...}; the BUILDING_MANAGER's optional BUILDING
    // is {#0 u32 health, #1 utf16 level key, ...} (CONFIRMED). ROAD_SLOT is a REGION_SLOT too.
    let smpath = format!("{path}/REGION_SLOT_MANAGER");
    let sm = child(rec, "REGION_SLOT_MANAGER", path)?;
    let mut slots = Vec::new();
    let mut construction = Vec::new();
    let mut recruitment_queue = Vec::new();
    let mut recruitment_sources = Vec::new();
    if let Some(array) = sm.record_array("REGION_SLOT_ARRAY") {
        // The raw item index (as the save writer finds a port's manager), not the record count.
        for (i, slot) in array.items.iter().enumerate().filter_map(|(i, item)| Some((i, item.first()?.as_record()?))) {
            let spath = format!("{smpath}/REGION_SLOT_ARRAY[{i}]/REGION_SLOT");
            let slot_key = slot.get_str(3).unwrap_or_default().to_string();
            let building = read_building(slot, &spath, &key, check)?;
            read_construction(slot, SlotRef::Slot(slots.len()), &mut construction);
            slots.push(RegionSlot { slot_type: slot_type(&slot_key, check.db), key: slot_key, building,
                // #6/#7: map position (CONFIRMED, see `RegionSlot::position`).
                position: slot.get_i32(6).zip(slot.get_i32(7)).map(|(x, z)| (Fixed20::from_raw(x), Fixed20::from_raw(z))),
                port: slot.child("REGION_RECRUITMENT_MANAGER").is_some(),
                // #0 SIEGEABLE_GARRISON_RESIDENCE #0 GARRISON_RESIDENCE #0: the holding faction's id.
                holder: slot
                    .child("SIEGEABLE_GARRISON_RESIDENCE")
                    .and_then(|s| s.child("GARRISON_RESIDENCE"))
                    .and_then(|g| g.get_u32(0))
                    .filter(|&f| f != 0)
                    .map(|f| FactionId(f as i32)),
                id: slot.get_u32(2).unwrap_or(0) });
            // Port slots have their own (naval) recruitment queue.
            if let Some(m) = slot.child("REGION_RECRUITMENT_MANAGER") {
                read_recruitment(m, &key, Some(i), &mut recruitment_queue, &mut recruitment_sources, check);
            }
        }
    }
    let mut road = None;
    if let Some(slot) = wrapped_slot(sm, "ROAD_SLOT") {
        road = read_building(slot, &format!("{smpath}/ROAD_SLOT"), &key, check)?;
        read_construction(slot, SlotRef::Road, &mut construction);
    }
    // FORTIFICATION_SLOT: the settlement's fortification building (a REGION_SLOT like the road slot, CONFIRMED structure).
    let mut fortification = None;
    if let Some(slot) = wrapped_slot(sm, "FORTIFICATION_SLOT") {
        fortification = read_building(slot, &format!("{smpath}/FORTIFICATION_SLOT"), &key, check)?;
        read_construction(slot, SlotRef::Walls, &mut construction);
    }
    match rec.child("REGION_RECRUITMENT_MANAGER") {
        Some(m) => read_recruitment(m, &key, None, &mut recruitment_queue, &mut recruitment_sources, check),
        None => check.warnings.push(LoadWarning::RegionWithoutRecruitmentManager { region: key.clone() }),
    }
    // REGION #10 GDP, #12 town wealth, #15 town wealth growth, #19 tax exempt (CONFIRMED from the
    // REGION writer 0x00A51E30, see `Region` and CAMPAIGN_FIDELITY.md).
    let base_gdp = rec.get_u32(9).unwrap_or(0);
    let gdp = rec.get_u32(10).unwrap_or(0);
    let wealth_growth_offset = rec.get(17).and_then(EsfNode::as_int).unwrap_or(0) as i32;
    let discontent_growth = rec.get(18).and_then(EsfNode::as_int).unwrap_or(0) as i32;
    let town_wealth = rec.get_u32(12).unwrap_or(0);
    let town_wealth_growth = rec.get_i32(15).unwrap_or(0);
    let tax_exempt = rec.get_bool(19).unwrap_or(false);
    // Public-order data of the population (CONFIRMED layout: POPULATION #0 REGION_FACTORS #1 classes,
    // #11 RELIGION_BREAKDOWN).
    let factors = rec.child("POPULATION").and_then(|p| p.child("REGION_FACTORS"));
    let religions: Vec<(String, f32)> = factors
        .and_then(|f| f.record_array("RELIGION_BREAKDOWN"))
        .into_iter()
        .flat_map(|a| a.items.iter())
        .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_f32()?)))
        .collect();
    let class_bases: Vec<(String, i32, i32)> = factors
        .and_then(|f| f.record_array("POPULATION CLASSES"))
        .into_iter()
        .flat_map(|a| a.records())
        .filter_map(|c| Some((c.get_str(0)?.to_string(), c.get_i32(11).unwrap_or(0), c.get_i32(12).unwrap_or(0))))
        .collect();

    if check.db.region(&key).is_none() {
        check
            .warnings
            .push(LoadWarning::UnknownRegionKey(key.clone()));
    }
    let region = Region {
        id: RegionId(id),
        key,
        owner: FactionId(owner),
        settlement,
        slots,
        road,
        fortification,
        population,
        population_state,
        base_gdp,
        gdp,
        wealth_growth_offset,
        discontent_growth,
        town_wealth,
        town_wealth_growth,
        tax_exempt,
        religions,
        class_bases,
        recruitment_queue,
        construction,
        // SIEGEABLE_GARRISON_RESIDENCE #12: the garrisoned army (CONFIRMED, SAVE_COMPAT.md §4);
        // checked against the forces in `load_world`.
        garrison: garrison_res.get_u32(12).filter(|&f| f != 0).map(ForceId),
        fleet: None,
    };
    Ok((region, recruitment_sources))
}

/// The forts of one `REGION`: its `FORT_ARRAY` items.
///
/// `FORT_ARRAY` is a child of `REGION` (after `RELIGIOUS_MISSION_BUILDING_ARRAY`, before
/// `RESOURCES_ARRAY`), **CONFIRMED from the shipped data**: every shipped file has it there -- as an
/// **empty** array (0 items in all eight `campaigns/*/startpos.esf` and in all ten vanilla saves).
/// So this loop never runs on any file we have.
///
/// **The per-item layout is INFERRED** (review 0-E: the free model's notes cite read-only decompiles
/// of `0x00A51E30` and `0x00AEB190`, but no decompile of either is kept in the evidence folder, and
/// no shipped file has an item to check it against). As recorded: per item the region's reader
/// (`0x00A51E30`) reads **one `u32` itself** and then lets the fort object read the rest
/// (`FUN_00AFD010`); the fort object's loader `FUN_00AEB190` reads a map position -- two `f32`,
/// stored at fort +0x120 -- and then a string (fort +0x188), plus a second string when the record's
/// version byte says 2 and a third when it says 4. Those version-gated strings are not read here.
/// PROVISIONAL: only the item's top-level nodes are searched; if the fort object's fields sit in a
/// nested record, the position and key are not found (`position: None`, empty key) rather than
/// misread.
///
/// Because the items carry no shipped example, the fields are found **by shape** rather than by a
/// fixed index: the leading integer as the id, the first coordinate pair as the position, the first
/// string as the key. A record that fits none of those is skipped rather than misread. The meaning of
/// the id, the key and the position's exact encoding is **UNKNOWN** (see [`Fort`]).
fn read_forts(rec: &EsfRecord, region: RegionId, _path: &str) -> Vec<Fort> {
    let Some(array) = rec.record_array("FORT_ARRAY") else { return Vec::new() };
    array
        .items
        .iter()
        .filter_map(|item| {
            let id = item.iter().find_map(EsfNode::as_int).map(|v| FortId(v as u32))?;
            let position = item.iter().find_map(|n| match n {
                EsfNode::Coord2d(x, y) => Some((Fixed20::from_f64(f64::from(*x)), Fixed20::from_f64(f64::from(*y)))),
                _ => None,
            });
            let key = item.iter().find_map(EsfNode::as_str).unwrap_or_default().to_string();
            Some(Fort { id, region, position, key })
        })
        .collect()
}

/// The slot type of a `REGION_SLOT` key: the 4th part of `settlement:<region>:<town>:<type>:<n>`,
/// else the `campaign_map_slots` / `campaign_map_towns_and_ports` type of the key (DB), else the
/// key's first part (e.g. `pasture`).
pub(crate) fn slot_type(key: &str, db: &GameDatabase) -> String {
    let parts: Vec<&str> = key.split(':').collect();
    if parts.first() == Some(&"settlement") {
        return parts.get(3).copied().unwrap_or("settlement").to_string();
    }
    if let Some(s) = db.campaign.map_slots.get(key) {
        return s.slot_type.clone();
    }
    if let Some(t) = db.campaign.map_towns.get(key) {
        return t.slot_type.clone();
    }
    parts.first().copied().unwrap_or_default().to_string()
}

/// A `REGION_SLOT_MANAGER` child slot (`ROAD_SLOT` / `FORTIFICATION_SLOT`): its `REGION_SLOT`
/// (CONFIRMED shape in the shipped files), or the child itself when it holds none (accepted for
/// files that store the building manager directly). The save writer finds the slot the same way.
pub(crate) fn wrapped_slot<'a>(sm: &'a EsfRecord, name: &str) -> Option<&'a EsfRecord> {
    let slot = sm.child(name)?;
    slot.child("REGION_SLOT").or(Some(slot))
}

/// The building in a slot record's `BUILDING_MANAGER`, if any.
fn read_building(
    slot: &EsfRecord,
    spath: &str,
    region: &str,
    check: &mut Checker<'_>,
) -> Result<Option<BuildingRef>, LoadError> {
    let Some(b) = slot.child("BUILDING_MANAGER").and_then(|m| m.child("BUILDING")) else {
        return Ok(None);
    };
    let bpath = format!("{spath}/BUILDING_MANAGER/BUILDING");
    let level_key = str_at(b, 1, &bpath)?.to_string();
    if check.db.building_level(&level_key).is_none() {
        check.warnings.push(LoadWarning::UnknownBuildingKey {
            region: region.to_string(),
            key: level_key.clone(),
        });
    }
    Ok(Some(BuildingRef { level_key, health: u32_at(b, 0, &bpath)? }))
}

/// `BUILDING_MANAGER/BUILDING_CONSTRUCTION_ITEM` (saves): {u32, bool, #2 u32 turns done,
/// #3 u32 total turns, #4 u32 cost, #5 utf16 level key}. #4 is the item's stored cost (+0x14), kept bit for bit (the
/// refund credits exactly it, CONFIRMED in `0x00B1A790`); which save field holds which member is INFERRED from the user's
/// saves (#2 < #3 always; #4 near the `building_levels` cost).
pub(crate) fn read_construction(slot: &EsfRecord, index: SlotRef, out: &mut Vec<ConstructionItem>) {
    let Some(item) = slot.child("BUILDING_MANAGER").and_then(|m| m.child("BUILDING_CONSTRUCTION_ITEM")) else {
        return;
    };
    if let (Some(done), Some(total), Some(cost), Some(key)) = (item.get_u32(2), item.get_u32(3), item.get_u32(4), item.get_str(5)) {
        out.push(ConstructionItem {
            slot: index,
            level_key: key.to_string(),
            turns_remaining: total.saturating_sub(done).max(1),
            // The item's int cost (+0x14), taken bit for bit: a negative cost survives the round trip.
            cost: cost as i32,
        });
    }
}

/// `REGION_RECRUITMENT_MANAGER/REGION_RECRUITMENT_ITEM_ARRAY[]/RECRUITMENT_ITEM/
/// {LAND|NAVAL}_UNIT_RECRUITMENT_ITEM/RECRUITMENT_ITEM` (saves only; startpos queues are empty).
/// Inner `RECRUITMENT_ITEM` v2: #0 i32 the item's id (`RecruitmentItemId`), #3 u32 turns
/// remaining (INFERRED: values 1..6, counting down),
/// #4 u32 cost (INFERRED: repeated at #7), #6 utf16 unit key (CONFIRMED). An item whose #0 is not
/// an integer (i32, or u32 taken bit for bit) or is 0 is kept with id 0 and a warning; the loader
/// gives it a new id ([`assign_missing_recruitment_ids`]). An item without turns or unit key is
/// skipped with a warning (UNKNOWN shape). A manager without its array (malformed: every manager of
/// the shipped files has one, SAVE_COMPAT.md §32) is read as an empty queue, with a warning.
pub(crate) fn read_recruitment(
    manager: &EsfRecord,
    region: &str,
    port_slot: Option<usize>,
    queue: &mut Vec<RecruitmentItem>,
    sources: &mut Vec<RecruitmentSource>,
    check: &mut Checker<'_>,
) {
    let Some(items) = manager.record_array("REGION_RECRUITMENT_ITEM_ARRAY") else {
        check.warnings.push(LoadWarning::RecruitmentManagerWithoutItems { region: region.to_owned(), port_slot });
        return;
    };
    for (index, item) in items.items.iter().enumerate() {
        let Some(outer) = item.first().and_then(EsfNode::as_record) else { continue };
        let Some(inner) = outer.children.first().and_then(EsfNode::as_record).and_then(|kind| kind.child("RECRUITMENT_ITEM")) else {
            check.warnings.push(LoadWarning::UnreadableRecruitmentItem { region: region.to_owned() });
            continue;
        };
        let (Some(turns), Some(key)) = (inner.get_u32(3), inner.get_str(6)) else {
            check.warnings.push(LoadWarning::UnreadableRecruitmentItem { region: region.to_owned() });
            continue;
        };
        let id = inner.get_i32(0).or_else(|| inner.get_u32(0).map(|v| v as i32)).filter(|&v| v != 0);
        if id.is_none() {
            check.warnings.push(LoadWarning::RecruitmentItemWithoutId { region: region.to_owned(), unit: key.to_owned() });
        }
        queue.push(RecruitmentItem {
            id: RecruitmentItemId(id.unwrap_or(0)),
            unit_key: key.to_string(),
            turns_remaining: turns,
            cost: inner.get_u32(4).unwrap_or(0) as i32,
        });
        sources.push(RecruitmentSource { port_slot, index });
    }
}

/// Recruitment item ids must be unique across the world: `CancelRecruitment` names an item by its
/// id alone and the UI finds the item's region from it (`CampaignModel::recruitment_item_region`),
/// so a repeated id would cancel and refund another region's item. The original's ids are object
/// addresses (SAVE_COMPAT.md §3), so its saves never repeat one. An item of `queue` whose id an
/// earlier item (file order, `seen`) already has gets id 0, so [`assign_missing_recruitment_ids`]
/// gives it a new one, and a warning: the queued unit is kept, unlike the other duplicate kinds.
fn keep_recruitment_ids_unique(
    queue: &mut [RecruitmentItem],
    region: &str,
    seen: &mut BTreeSet<RecruitmentItemId>,
    check: &mut Checker<'_>,
) {
    for item in queue.iter_mut().filter(|i| i.id.raw() != 0) {
        if !seen.insert(item.id) {
            check.warnings.push(LoadWarning::DuplicateRecruitmentItemId {
                region: region.to_owned(),
                unit: item.unit_key.clone(),
                id: item.id.raw(),
            });
            item.id = RecruitmentItemId(0);
        }
    }
}

/// Gives every recruitment item kept without an id (id 0, never a real id: the original's ids are
/// object addresses), from [`read_recruitment`] or [`keep_recruitment_ids_unique`], a new one from
/// [`World::alloc_id`], in region then queue order. Run once the loader has set `World::next_id`.
pub(crate) fn assign_missing_recruitment_ids(world: &mut World) {
    // Usually empty (no allocation): only a file with id-less or repeated items lists any.
    let missing: Vec<(RegionId, usize)> = world
        .regions
        .values()
        .flat_map(|r| r.recruitment_queue.iter().enumerate().filter(|(_, i)| i.id.raw() == 0).map(move |(n, _)| (r.id, n)))
        .collect();
    for (region, n) in missing {
        let id = RecruitmentItemId(world.alloc_id() as i32);
        world.regions.get_mut(&region).expect("listed").recruitment_queue[n].id = id;
    }
}

/// Links every loaded recruitment item to the record it came from (`World::recruitment_sources`),
/// once [`assign_missing_recruitment_ids`] has given each its final id. `sources` is each region's
/// list from [`read_region`], parallel to its queue (neither step reorders the queue).
pub(crate) fn link_recruitment_sources(world: &mut World, sources: BTreeMap<RegionId, Vec<RecruitmentSource>>) {
    for (region, list) in sources {
        let Some(r) = world.regions.get(&region) else { continue };
        debug_assert_eq!(r.recruitment_queue.len(), list.len(), "one source per loaded item");
        for (item, source) in r.recruitment_queue.iter().zip(list) {
            world.recruitment_sources.insert(item.id, source);
        }
    }
}

/// The transport link of an `ARMY` / `NAVY` record: the force id of the navy carrying the army
/// (ARMY: the u32 just before its trailing bool, after an optional record; loader `0x00870FD0`
/// links army +0xEC to navy +0xD8) or of the army aboard the navy (NAVY #4, the u32 before
/// `THEATRE_TRANSITION_INFO`; loader `0x008822F0`). CONFIRMED structure; `None` when 0 or absent.
/// No recovered original save holds an embarked army (all links 0), so the values come from the
/// code only.
fn transport_link(rec: &EsfRecord, is_navy: bool) -> Option<u32> {
    let id = if is_navy {
        let t = rec.children.iter().position(|n| n.as_record().is_some_and(|r| r.name == "THEATRE_TRANSITION_INFO"))?;
        rec.children.get(t.checked_sub(1)?)?.as_u32()?
    } else {
        let mut it = rec.children.iter().rev();
        let mut last = it.next()?;
        if last.as_bool().is_some() {
            last = it.next()?;
        }
        if let EsfNode::U32(v) = last { *v } else { return None }
    };
    (id != 0).then_some(id)
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    fn rec(name: &str, version: u8, children: Vec<EsfNode>) -> EsfRecord {
        let mut r = EsfRecord::new(name, version);
        r.children = children;
        r
    }

    #[test]
    fn transport_link_fields() {
        let mf = || EsfNode::Record(Box::new(rec("MILITARY_FORCE", 1, vec![EsfNode::U32(5), EsfNode::U32(6), EsfNode::U32Array(vec![])])));
        let army = |navy: u32| {
            rec("ARMY", 2, vec![
                mf(),
                EsfNode::U32Array(vec![]),
                EsfNode::U32Array(vec![]),
                EsfNode::I32(5),
                EsfNode::U32(0),
                EsfNode::Bool(false),
                EsfNode::U32(navy),
                EsfNode::Bool(false),
            ])
        };
        assert_eq!(transport_link(&army(77), false), Some(77));
        assert_eq!(transport_link(&army(0), false), None);
        let theatre = || EsfNode::Record(Box::new(rec("THEATRE_TRANSITION_INFO", 1, vec![])));
        let navy = rec("NAVY", 1, vec![mf(), EsfNode::U32Array(vec![]), EsfNode::U32Array(vec![]), EsfNode::U32(9), theatre(), EsfNode::U32(0)]);
        assert_eq!(transport_link(&navy, true), Some(9));
    }
}

#[cfg(test)]
mod recruitment_tests {
    use super::*;
    use ntw_formats::esf::EsfRecordArray;

    fn item(id: EsfNode, key: &str) -> Vec<EsfNode> {
        let mut inner = EsfRecord::new("RECRUITMENT_ITEM", 2);
        inner.children = vec![id, EsfNode::I32(1), EsfNode::I32(0), EsfNode::U32(2), EsfNode::U32(300), EsfNode::Bool(true), EsfNode::Utf16String(key.into())];
        let mut land = EsfRecord::new("LAND_UNIT_RECRUITMENT_ITEM", 1);
        land.children = vec![EsfNode::Record(Box::new(inner)), EsfNode::U32(0)];
        let mut outer = EsfRecord::new("RECRUITMENT_ITEM", 2);
        outer.children = vec![EsfNode::Record(Box::new(land))];
        vec![EsfNode::Record(Box::new(outer))]
    }

    /// Review: an item whose #0 was not an exact `I32` was dropped without a word (and so erased
    /// by the next save). Now a u32 id is read bit for bit, and an item with no integer id is kept
    /// with a warning and gets a new id once `next_id` is set.
    #[test]
    fn a_recruitment_item_without_an_i32_id_is_kept() {
        let mut arr = EsfRecordArray::new("REGION_RECRUITMENT_ITEM_ARRAY", 0);
        arr.items.push(item(EsfNode::I32(500), "a"));
        arr.items.push(item(EsfNode::U32(600), "b"));
        arr.items.push(item(EsfNode::Utf16String("x".into()), "c"));
        let mut manager = EsfRecord::new("REGION_RECRUITMENT_MANAGER", 0);
        manager.children = vec![EsfNode::RecordArray(Box::new(arr))];
        let db = GameDatabase::test_fixture();
        let mut check = Checker { db: &db, warnings: Vec::new() };
        let mut queue = Vec::new();
        read_recruitment(&manager, "r", None, &mut queue, &mut Vec::new(), &mut check);
        assert_eq!(queue.iter().map(|i| (i.id.raw(), i.unit_key.as_str())).collect::<Vec<_>>(), vec![(500, "a"), (600, "b"), (0, "c")]);
        assert_eq!(check.warnings, vec![LoadWarning::RecruitmentItemWithoutId { region: "r".into(), unit: "c".into() }]);
    }

    /// Review: a recruitment manager without its item array was read as an empty queue without a
    /// word. It still is (nothing to read), now with one warning naming the manager.
    #[test]
    fn a_recruitment_manager_without_its_item_array_warns() {
        let mut manager = EsfRecord::new("REGION_RECRUITMENT_MANAGER", 1);
        manager.children = vec![EsfNode::Bool(false), EsfNode::I32(5)];
        let db = GameDatabase::test_fixture();
        let mut check = Checker { db: &db, warnings: Vec::new() };
        let (mut queue, mut sources) = (Vec::new(), Vec::new());
        read_recruitment(&manager, "r", None, &mut queue, &mut sources, &mut check);
        read_recruitment(&manager, "r", Some(2), &mut queue, &mut sources, &mut check);
        assert!(queue.is_empty() && sources.is_empty());
        assert_eq!(
            check.warnings,
            vec![
                LoadWarning::RecruitmentManagerWithoutItems { region: "r".into(), port_slot: None },
                LoadWarning::RecruitmentManagerWithoutItems { region: "r".into(), port_slot: Some(2) },
            ]
        );
        assert_eq!(check.warnings[1].to_string(), "region r: the recruitment manager of port slot 2 has no item array (read as an empty queue)");
    }

    /// Review (round 12): ids were not checked for uniqueness on load, so with an id repeated in
    /// two regions `CancelRecruitment` in the second region removed and refunded the first one's
    /// item. A repeated id is now cleared (and given a new one later) with a warning; id 0 items
    /// are left for `assign_missing_recruitment_ids`.
    #[test]
    fn a_repeated_recruitment_id_is_cleared_with_a_warning() {
        let q = |ids: &[i32]| -> Vec<RecruitmentItem> {
            ids.iter().map(|&id| RecruitmentItem { id: RecruitmentItemId(id), unit_key: format!("u{id}"), turns_remaining: 1, cost: 0 }).collect()
        };
        let db = GameDatabase::test_fixture();
        let mut check = Checker { db: &db, warnings: Vec::new() };
        let mut seen = BTreeSet::new();
        let mut a = q(&[500, 0, 600]);
        let mut b = q(&[700, 500, 0]);
        keep_recruitment_ids_unique(&mut a, "a", &mut seen, &mut check);
        keep_recruitment_ids_unique(&mut b, "b", &mut seen, &mut check);
        assert_eq!(a.iter().map(|i| i.id.raw()).collect::<Vec<_>>(), vec![500, 0, 600]);
        assert_eq!(b.iter().map(|i| i.id.raw()).collect::<Vec<_>>(), vec![700, 0, 0]);
        assert_eq!(check.warnings, vec![LoadWarning::DuplicateRecruitmentItemId { region: "b".into(), unit: "u500".into(), id: 500 }]);
    }
}

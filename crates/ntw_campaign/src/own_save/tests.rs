use std::collections::BTreeMap;

use ntw_sim::calendar::{Calendar, Date, HALF_EARLY};
use ntw_sim::campaign::{
    CampaignModel, CampaignUnit, Character, CharacterId, CharacterKind, Faction, FactionAiKeys, FactionId, ForceId, GovernmentType,
    MilitaryForce, PendingBattle, Region, RegionId, RegionSlot, Settlement, Stance, UnitId, World,
};
use ntw_sim::campaign::commerce_raid::CommerceRaid;
use ntw_sim::campaign::trade::{TradeLeg, TradePath, TradeWaypoint};
use ntw_sim::campaign::visibility::TradeSegmentSight;
use ntw_sim::fixed::Fixed20;
use ntw_sim::rng::CaRng;

use super::*;
use crate::{FileKind, HeaderMap, PlayerSetup, SaveHeader};

pub(crate) const A: FactionId = FactionId(7);
const B: FactionId = FactionId(-3);

fn faction(id: FactionId, key: &str, other: FactionId) -> Faction {
    Faction {
        id,
        key: key.into(),
        treasury: -250,
        government: GovernmentType::Republic,
        government_key: "gov_republic".into(),
        tax_lower: "tax_high".into(),
        tax_upper: "tax_low".into(),
        diplomacy: BTreeMap::from([(other, Stance::War)]),
    }
}

fn region(id: u32, owner: FactionId) -> Region {
    Region {
        id: RegionId(id),
        key: format!("made_up_region_{id}"),
        owner,
        settlement: Settlement { key: format!("settlement:made_up_region_{id}:town"), position: (Fixed20::from_int(id as i32), Fixed20::from_int(-4)) },
        slots: Vec::new(),
        road: None,
        fortification: None,
        road_holder: None,
        fortification_holder: None,
        population: 123_456,
        population_state: Default::default(),
        base_gdp: 900,
        gdp: 1000,
        wealth_growth_offset: -2,
        discontent_growth: 3,
        town_wealth: 17,
        town_wealth_growth: 1,
        tax_exempt: true,
        religions: Vec::new(),
        class_bases: Vec::new(),
        unit_resources: Vec::new(),
        recruitment_queue: Vec::new(),
        construction: Vec::new(),
        garrison: None,
        fleet: None,
    }
}

/// A MADE-UP campaign that never was an ESF file: two factions at war, three regions, an army and
/// a navy with the army aboard, AI keys, a pending battle.
pub(crate) fn made_up_model() -> CampaignModel {
    let mut w = World::default();
    w.factions.insert(A, faction(A, "made_up_a", B));
    w.factions.insert(B, faction(B, "made_up_b", A));
    w.turn_order = vec![B, A];
    for r in [region(1, A), region(2, B), region(5_000_000, A)] {
        w.regions.insert(r.id, r);
    }
    for (id, f, kind) in [(100, A, CharacterKind::General), (101, A, CharacterKind::Admiral), (102, B, CharacterKind::General)] {
        w.characters.insert(
            CharacterId(id),
            Character {
                id: CharacterId(id),
                faction: f,
                kind,
                position: (Fixed20::from_raw(1_310_721), Fixed20::from_int(id)),
                movement_points: 7,
                max_movement_points: 30,
                base_movement_points: 28,
                garrisoned_in: None,
            },
        );
    }
    let unit = |id: i32| CampaignUnit { id: UnitId(id), unit_key: "made_up_unit".into(), men: 77, max_men: 120, character: None, officer_name: Default::default() };
    w.forces.insert(ForceId(1), MilitaryForce { id: ForceId(1), faction: A, commander: Some(CharacterId(100)), units: (0..40).map(unit).collect(), is_navy: false });
    w.forces.insert(ForceId(2), MilitaryForce { id: ForceId(2), faction: A, commander: Some(CharacterId(101)), units: vec![unit(50)], is_navy: true });
    w.forces.insert(ForceId(3), MilitaryForce { id: ForceId(3), faction: B, commander: Some(CharacterId(102)), units: vec![unit(60)], is_navy: false });
    w.embarked.insert(ForceId(1), ForceId(2));
    // B's route to A is built in play (no stored route): its accumulated value is kept per pair.
    w.trade_accumulated.insert((B, A), 4321);
    // A route leaving by sea from A's capital (a capital port route) and a land route.
    w.faction_details.entry(A).or_default().capital = Some(RegionId(1));
    let hop = |region: u32, sea: bool| TradeWaypoint { region: RegionId(region), from: 3, to: 4, sea, from_pos: None, to_pos: None };
    w.trade_paths.insert(
        (A, B),
        vec![
            TradePath { waypoints: vec![hop(1, true), hop(2, false)], capital_port: true, volumes: vec![5, 0], accumulated: 4000 },
            TradePath { waypoints: vec![hop(1, false), hop(2, false)], capital_port: false, volumes: vec![1, 2], accumulated: 321 },
        ],
    );
    // A trade leg along segments 0 and 1, segment 0's course, and general 100 raiding it for B.
    w.trade_network = vec![TradeLeg { from: 3, to: 4, length: 2.5, splines: vec![0, 3] }];
    let course = [(1.0, 2.0), (3.0, 2.0), (5.0, 2.5), (7.0, 3.0)];
    w.trade_sight = vec![TradeSegmentSight { cells: vec![(1, 2)], domestic: vec![A], international: Vec::new(), curves: vec![course] }, TradeSegmentSight::default()];
    w.commerce_raids = vec![CommerceRaid { character: CharacterId(100), segment: 0, target: B }];
    w.ai_keys.insert(A, FactionAiKeys { manager: "made_up_manager".into(), personality: "made_up_personality".into(), extra: ["default".into(), "x".into()] });
    w.region_base_values.insert(RegionId(2), 15_025);
    w.deal_regions_received.insert(A, 3);
    w.restricted_buildings.insert("made_up_level".into());
    w.next_id = 4096;
    let start = Date { year: 1805, season: 1, month: 3, half: HALF_EARLY };
    let mut m = CampaignModel::new(Calendar::new(start, 5), CaRng::new(987_654), w);
    m.pending_battle = Some(PendingBattle { attacker: ForceId(1), defenders: vec![ForceId(3)], settlement: Some(RegionId(2)), resume: None });
    m
}

pub(crate) fn info() -> CampaignInfo {
    CampaignInfo {
        kind: FileKind::Save,
        timestamp: 1_790_000_000,
        build_id: "napoleonrust".into(),
        build_version: "0.1.0".into(),
        header: SaveHeader {
            faction_key: "made_up_a".into(),
            portrait: "ui/portraits/made_up.tga".into(),
            turn_number: 6,
            year: 1805,
            season_name: "Spring".into(),
            flag_path: "ui/flags/made_up".into(),
            date: Some(Date { year: 1805, season: 1, month: 3, half: HALF_EARLY }),
            maps: vec![HeaderMap { theatre: "made_up_main".into(), width: 2, height: 1, pitch: 8, pixels: vec![0xFF00_FF00, 0xFF12_3456] }],
        },
        campaign_key: "made_up_campaign".into(),
        map_key: "made_up_map".into(),
        players: vec![PlayerSetup { faction_key: "made_up_a".into(), is_human: true, is_playable: true }],
    }
}

/// A campaign that never came from an ESF file saves and loads back field for field: header,
/// model, script slots and restricted units.
#[test]
fn a_made_up_campaign_round_trips() {
    let mut model = made_up_model();
    model.world.restricted_units.insert("made_up_unit".into());
    let data = SaveData {
        human: "made_up_a".into(),
        model,
        rebel_faction: Some(B),
        script_values: vec![ScriptSaveValue::Bool(true), ScriptSaveValue::Int(-12)],
        restricted_units: Vec::new(),
    };
    let bytes = write(&info(), &data).expect("write");
    assert!(is_own_save(&bytes));
    assert_eq!(read_info(&bytes).expect("header"), info());
    assert_eq!(crate::read_info(&bytes).expect("header through the dispatcher"), info());
    let (h, back) = read_parts(&bytes).expect("read");
    assert_eq!(h, info());
    assert_eq!(back, data);
}

/// recruit-gates: the scripts' restricted units moved from the save body into the model
/// (`World::restricted_units`); a save written before that, with the list beside the model, loads with
/// it merged into the model.
#[test]
fn a_legacy_restricted_unit_list_loads_into_the_model() {
    let mut model = made_up_model();
    model.world.restricted_units.insert("made_up_kept".into());
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: vec!["made_up_legacy".into()] };
    let loaded = read(&write(&info(), &data).expect("write"), &ntw_data::GameDatabase::test_fixture()).expect("read");
    assert_eq!(loaded.model.world.restricted_units.iter().map(String::as_str).collect::<Vec<_>>(), ["made_up_kept", "made_up_legacy"]);
}

/// The values mods' rules keep (DESIGN.md §3.3.1) save and load back value for value, in key order,
/// with the same state hash; a model without them saves none.
#[test]
fn mod_state_round_trips() {
    use ntw_sim::campaign::mod_state::{ModKey, ModScope, ModValue};
    let mut model = made_up_model();
    let values = [
        (ModKey::new("made_up_mod", ModScope::Region(RegionId(5_000_000)), "famine"), ModValue::Int(i64::MIN)),
        (ModKey::new("made_up_mod", ModScope::Faction(B), "grain"), ModValue::Float(0.1)),
        (ModKey::new("another_mod", ModScope::Campaign, "era"), ModValue::Text("meiji".into())),
        (ModKey::new("another_mod", ModScope::Character(CharacterId(-5)), "list"), ModValue::List(vec![ModValue::Bool(true), ModValue::List(Vec::new())])),
        (ModKey::new("another_mod", ModScope::Force(ForceId(u32::MAX)), "x"), ModValue::Float(-1.0e300)),
    ];
    for (k, v) in values.clone() {
        model.mod_state.set(k, v);
    }
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let (_, back) = read_parts(&write(&info(), &data).expect("write")).expect("read");
    assert_eq!(back.model.mod_state, data.model.mod_state);
    assert_eq!(back.model.mod_state.len(), values.len());
    assert_eq!(back.model.state_hash(), data.model.state_hash());
    // A vanilla model's save has no mod values and loads with none.
    let vanilla = SaveData { model: made_up_model(), ..data };
    let (_, back) = read_parts(&write(&info(), &vanilla).expect("write")).expect("read");
    assert!(back.model.mod_state.is_empty());
}

/// The campaign director's state (its RNG, counter and queued transfers), the victory regions and
/// the CAI map areas save and load back, with the same state hash; a save from before these fields
/// loads a new campaign's director and none of the others.
#[test]
fn director_victory_regions_and_ai_areas_round_trip() {
    use ntw_sim::campaign::AiArea;
    use ntw_sim::campaign::director::{DirectorState, PendingTransfer, RelationBelief};
    let mut model = made_up_model();
    let mut rng = CaRng::new(0x0006_1266);
    rng.float_range(0.0, 1.0);
    model.director = DirectorState {
        rng,
        transfer_counter: u32::MAX,
        pending: vec![
            PendingTransfer { receiver: A, region: RegionId(2), queued_owner: B },
            PendingTransfer { receiver: B, region: RegionId(5_000_000), queued_owner: A },
        ],
        autoresolver_pairs: vec![(B, A), (A, B)],
        xp_counters: [(A, 0), (B, u32::MAX)].into(),
        beliefs: [(A, RelationBelief { relations: vec![(B, 0.25)], enemies: vec![B] })].into(),
    };
    model.world.ai_factions = vec![B, A];
    model.world.victory_regions.insert(A, [RegionId(2), RegionId(5_000_000)].into());
    model.world.ai_areas = vec![
        AiArea { key: "made_up_region_1".into(), links: vec![1, 2] },
        AiArea { key: "made_up_sea".into(), links: vec![0] },
        AiArea { key: "made_up_region_2".into(), links: vec![0, 0] },
    ];
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let (_, back) = read_parts(&write(&info(), &data).expect("write")).expect("read");
    assert_eq!(back.model.director, data.model.director);
    assert_eq!(back.model.world.victory_regions, data.model.world.victory_regions);
    assert_eq!(back.model.world.ai_areas, data.model.world.ai_areas);
    assert_eq!(back.model.state_hash(), data.model.state_hash());
    assert_eq!(back, data);
    // The made-up model has none of them: they load back as a new campaign's.
    let plain = SaveData { model: made_up_model(), ..data };
    let (_, back) = read_parts(&write(&info(), &plain).expect("write")).expect("read");
    assert_eq!(back.model.director, DirectorState::default());
    assert!(back.model.world.victory_regions.is_empty() && back.model.world.ai_areas.is_empty());
}

/// The fields the original does not save come back empty (as after the original's own load).
#[test]
fn fields_the_original_does_not_save_are_not_saved() {
    let mut model = made_up_model();
    model.world.sabotaged.insert(ForceId(3));
    model.world.agents_acted.insert(CharacterId(100));
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let (_, back) = read_parts(&write(&info(), &data).expect("write")).expect("read");
    assert!(back.model.world.sabotaged.is_empty() && back.model.world.agents_acted.is_empty());
    assert_eq!(back.model.world.factions, data.model.world.factions);
}

/// A newer format version, a truncated file and a damaged body give errors, never a panic.
#[test]
fn bad_saves_are_errors() {
    let data = SaveData { human: "made_up_a".into(), model: made_up_model(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let bytes = write(&info(), &data).expect("write");
    let mut newer = bytes.clone();
    newer[16..20].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
    assert!(matches!(read_parts(&newer), Err(FormatError::Newer { found }) if found == FORMAT_VERSION + 1));
    for cut in [10, 18, 22, 30, bytes.len() - 1] {
        assert!(read_parts(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    let mut damaged = bytes.clone();
    let last = damaged.len() - 20;
    damaged[last] ^= 0x5A;
    assert!(read_parts(&damaged).is_err());
}

/// The header of a new save: the season name follows the calendar (review: it stayed the start
/// position's), and every theatre of the campaign is kept even when its pictures did not load (review:
/// a save without pictures lost its theatre, and with it the HUD's home theatre).
#[test]
fn the_save_header_follows_the_calendar_and_keeps_the_theatres() {
    let mut model = made_up_model();
    model.calendar.date.set_quarter(1805, 11 * 4);
    let info = save_header(&info(), &model, "made_up_a", 1_800_000_000, &[]);
    assert_eq!(info.header.season_name, "Winter", "December");
    assert_eq!(info.header.date, Some(model.calendar.date));
    assert_eq!(info.theatres().collect::<Vec<_>>(), ["made_up_main"]);
    assert_eq!((info.header.maps[0].width, info.header.maps[0].height), (0, 0), "no picture");
    assert!(info.header.maps[0].pixels.is_empty());
}

/// A save written from a borrowed model reads back equal to one written from an owned model.
#[test]
fn a_save_written_from_a_borrowed_model_equals_one_from_an_owned_model() {
    use ntw_sim::campaign::mod_state::{ModKey, ModScope, ModValue};
    let mut model = made_up_model();
    model.mod_state.set(ModKey::new("made_up_mod", ModScope::Faction(B), "grain"), ModValue::Float(0.1));
    model.mod_state.set(ModKey::new("another_mod", ModScope::Campaign, "era"), ModValue::Text("meiji".into()));
    let owned = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: Some(B), script_values: Vec::new(), restricted_units: Vec::new() };
    let borrowed = SaveDataRef { human: "made_up_a", model: &model, rebel_faction: Some(B), script_values: Vec::new() };
    assert_eq!(write(&info(), &owned).expect("owned"), write(&info(), &borrowed).expect("borrowed"));
}

/// `bytes` (a save of the current format) as a format-version-1 save wrote it: see [`as_version`].
pub(crate) fn as_version_1(bytes: &[u8]) -> Vec<u8> {
    as_version(bytes, 1)
}

/// `bytes` (a save of the current format) as a save of format `version` wrote it: that version word,
/// without the region fields later versions added (2: `unit_resources`, 3: `road_holder` and
/// `fortification_holder`), the route fields (5: `capital_port`, 7: `accumulated`), the world's access-abuse counts (6: `access_abuses`)
/// and the commerce raids with the trade network's course (8: `commerce_raids`, `splines`, `curves`).
/// A list or map value ends at its first `]` / `}` (the made-up keys hold none), an
/// `Option` of a faction id at its first `,`, the route's last field (`,accumulated:`; the leading comma
/// keeps `trade_accumulated` out) at its struct's `)`, which is kept.
pub(crate) fn as_version(bytes: &[u8], version: u32) -> Vec<u8> {
    let (_, header, body) = sections(bytes).expect("sections");
    let mut text = String::new();
    flate2::read::ZlibDecoder::new(body).read_to_string(&mut text).expect("inflate");
    let added: [(u32, &str, char); 9] = [
        (2, "unit_resources:[", ']'),
        (3, "road_holder:", ','),
        (3, "fortification_holder:", ','),
        (5, "capital_port:", ','),
        (6, "access_abuses:{", '}'),
        (7, ",accumulated:", ')'),
        (8, "commerce_raids:[", ']'),
        (8, "splines:[", ']'),
        (8, "curves:[", ']'),
    ];
    for (_, field, end_char) in added.into_iter().filter(|&(v, ..)| v > version) {
        let mut old = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = rest.find(field) {
            old.push_str(&rest[..at]);
            let end = at + rest[at..].find(end_char).expect("value end") + usize::from(end_char != ')');
            rest = if matches!(end_char, ']' | '}') { rest[end..].strip_prefix(',').unwrap_or(&rest[end..]) } else { &rest[end..] };
        }
        old.push_str(rest);
        assert!(old.len() < text.len() && !old.contains(field), "no {field} to strip");
        text = old;
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(text.as_bytes()).expect("deflate");
    let body = z.finish().expect("deflate");
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(header);
    out.extend_from_slice(&body);
    out
}

/// recruit-gates review: format version 2 added the regions' unit resources. A version-1 save takes them
/// from the campaign's start, region by key; read without a start, or with a start missing one of its
/// regions, it is an error, never a load with regions left without resources.
#[test]
fn a_version_1_save_takes_its_unit_resources_from_the_start() {
    let mut start = made_up_model();
    for (i, r) in start.world.regions.values_mut().enumerate() {
        r.unit_resources = vec!["global".into(), format!("made_up_resource_{i}")];
    }
    let data = SaveData { human: "made_up_a".into(), model: start.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let old = as_version_1(&write(&info(), &data).expect("write"));
    let (version, _, stored) = read_stored(&old).expect("stored");
    assert_eq!(version, 1);
    assert!(stored.model.world.regions.values().all(|r| r.unit_resources.is_empty()));
    let db = ntw_data::GameDatabase::test_fixture();
    assert!(matches!(read(&old, &db), Err(FormatError::NeedsStart { found: 1, .. })));
    assert!(matches!(read_parts(&old), Err(FormatError::NeedsStart { found: 1, .. })));
    assert!(matches!(read_with_start(&old, &db, || Err("no start".into())), Err(FormatError::NeedsStart { found: 1, .. })));
    let loaded = read_with_start(&old, &db, || Ok(start.clone())).expect("converted");
    for (id, r) in &loaded.model.world.regions {
        assert_eq!(r.unit_resources, start.world.regions[id].unit_resources);
    }
    // The start lacks one of the save's regions: an error naming it.
    let mut short = start.clone();
    short.world.regions.retain(|id, _| id.0 != 2);
    match read_with_start(&old, &db, || Ok(short)) {
        Err(e @ FormatError::NeedsStart { .. }) => assert!(e.to_string().contains("made_up_region_2"), "{e}"),
        other => panic!("loaded without region 2's resources: {:?}", other.map(|l| l.info.campaign_key)),
    }
    // The current version never calls the start.
    let current = write(&info(), &data).expect("write");
    let loaded = read_with_start(&current, &db, || panic!("start read for a current save")).expect("read");
    assert_eq!(loaded.model.world.regions, start.world.regions);
}

/// recruit-followups: format version 3 added the regions' road and walls holders (`0x00A4DCB0` builds
/// those slots like any other, `0x00A6AE40` merges their effects only when held by the owner). An older
/// save takes them from the campaign's start, kept while the start's holder still holds one of the
/// region's slots, or, holding none, while the start's owner still owns the region.
#[test]
fn a_version_2_save_takes_its_road_and_walls_holders_from_the_start() {
    let slot = |holder: Option<FactionId>| RegionSlot { key: "made_up_slot".into(), slot_type: "made_up".into(), building: None, position: None, port: false, holder, id: 1 };
    let (r1, r2, r3) = (RegionId(1), RegionId(2), RegionId(5_000_000));
    let mut start = made_up_model();
    {
        let regions = &mut start.world.regions;
        // Region 1 (A's): B holds a slot, the road and the walls, as Bavaria in Austria's eur_bavaria.
        let r = regions.get_mut(&r1).unwrap();
        r.slots = vec![slot(Some(B)), slot(None)];
        (r.road_holder, r.fortification_holder) = (Some(B), Some(B));
        // Region 2 (B's) and region 3 (A's): the owner holds the road and walls, and no slot.
        let r = regions.get_mut(&r2).unwrap();
        (r.road_holder, r.fortification_holder) = (Some(B), None);
        let r = regions.get_mut(&r3).unwrap();
        (r.road_holder, r.fortification_holder) = (Some(A), Some(A));
    }
    // The save, some turns on: region 1 still as at the start; region 2 taken by A (B's holds cleared);
    // region 3 unchanged.
    let mut model = start.clone();
    let r = model.world.regions.get_mut(&r2).unwrap();
    (r.owner, r.road_holder) = (A, None);
    let data = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let current = write(&info(), &data).expect("write");
    let old = as_version(&current, 2);
    let (version, _, stored) = read_stored(&old).expect("stored");
    assert_eq!(version, 2);
    assert!(stored.model.world.regions.values().all(|r| r.road_holder.is_none() && r.fortification_holder.is_none()));
    let db = ntw_data::GameDatabase::test_fixture();
    assert!(matches!(read(&old, &db), Err(FormatError::NeedsStart { found: 2, .. })));
    let loaded = read_with_start(&old, &db, || Ok(start.clone())).expect("converted");
    assert_eq!(loaded.model.world.regions, model.world.regions);
    // Region 1 after B lost its slot there: the road and walls go with it.
    let mut lost = model.clone();
    lost.world.regions.get_mut(&r1).unwrap().slots[0].holder = None;
    let data = SaveData { model: lost, ..data };
    let loaded = read_with_start(&as_version(&write(&info(), &data).expect("write"), 2), &db, || Ok(start.clone())).expect("converted");
    let r = &loaded.model.world.regions[&r1];
    assert_eq!((r.road_holder, r.fortification_holder), (None, None));
    // The current version keeps them as written.
    let loaded = read_with_start(&current, &db, || panic!("start read for a current save")).expect("read");
    assert_eq!(loaded.model.world.regions, model.world.regions);
}

/// construction2: format version 4 keeps each construction item's kind. An older save stored a repair as a
/// build of the standing level, which loads as a repair item (`CampaignModel::convert_legacy_repairs`); a
/// version 1 or 2 save, made when construction stepped at each faction's turn start, gives the items the turn
/// starts already ran this round their turn back (the round end steps them now), once.
#[test]
fn older_saves_get_their_repairs_and_this_rounds_construction_step_back() {
    use ntw_sim::campaign::{BuildingRef, ConstructionItem, ConstructionKind, SlotRef, TurnState};
    let build = |slot, level_key: &str, turns_remaining| ConstructionItem { slot, level_key: level_key.into(), turns_remaining, cost: 0, kind: ConstructionKind::Build, active: true };
    let mut model = made_up_model();
    {
        // Region 1 (A's): walls being repaired (stored the old way) and a road upgrade.
        let r = model.world.regions.get_mut(&RegionId(1)).unwrap();
        r.fortification = Some(BuildingRef { level_key: "made_up_walls".into(), health: 40 });
        r.road = Some(BuildingRef { level_key: "made_up_road".into(), health: 100 });
        r.construction = vec![build(SlotRef::Walls, "made_up_walls", 1), build(SlotRef::Road, "made_up_road_2", 2)];
        // Region 2 (B's): a road upgrade in a road A holds, which no turn start stepped.
        let r = model.world.regions.get_mut(&RegionId(2)).unwrap();
        r.road_holder = Some(A);
        r.construction = vec![build(SlotRef::Road, "made_up_road_2", 3)];
    }
    let db = ntw_data::GameDatabase::test_fixture();
    let load = |model: &CampaignModel, current: FactionId, version: u32| {
        let mut model = model.clone();
        model.turn = TurnState::in_turn_of(current, vec![A]);
        let data = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
        let bytes = as_version(&write(&info(), &data).expect("write"), version);
        let loaded = read_with_start(&bytes, &db, || Ok(model.clone())).expect("read").model;
        let items = |id| loaded.world.regions[&RegionId(id)].construction.iter().map(|c| (c.slot, c.turns_remaining, c.kind)).collect::<Vec<_>>();
        (items(1), items(2), loaded.world.regions[&RegionId(1)].fortification.as_ref().map(|b| b.health))
    };
    // No level turns in the fixture: the repair is max(1, floor(0.6 × 1)) = 1 turn of 60.
    let repair = |health_per_turn, turns| ConstructionKind::Repair { health_per_turn, turns };
    let (one, two, health) = load(&model, A, 3);
    assert_eq!(one, vec![(SlotRef::Walls, 1, repair(60, 1)), (SlotRef::Road, 2, ConstructionKind::Build)]);
    assert_eq!(two, vec![(SlotRef::Road, 3, ConstructionKind::Build)]);
    assert_eq!(health, Some(40));
    // Version 2 in A's turn (order B, A): both turn starts ran; A's items take their turn back, B's road (held by
    // A) was never stepped. The repair keeps the length it now has left.
    let (one, two, health) = load(&model, A, 2);
    assert_eq!(one, vec![(SlotRef::Walls, 2, repair(30, 2)), (SlotRef::Road, 3, ConstructionKind::Build)]);
    assert_eq!(two, vec![(SlotRef::Road, 3, ConstructionKind::Build)]);
    assert_eq!(health, Some(40));
    // In B's turn, A's turn start has not run yet.
    let (one, ..) = load(&model, B, 2);
    assert_eq!(one, vec![(SlotRef::Walls, 1, repair(60, 1)), (SlotRef::Road, 2, ConstructionKind::Build)]);
    // The current version reads its items as written.
    let (one, ..) = load(&model, A, FORMAT_VERSION);
    assert_eq!(one, vec![(SlotRef::Walls, 1, ConstructionKind::Build), (SlotRef::Road, 2, ConstructionKind::Build)]);
}

/// construction2: format version 4 keeps each construction item's active byte (`ConstructionItem::active`,
/// ESF #1, +0x8; `0x00B7880E` steps only an active item). An older save has no such field: its items read
/// active, as every item the exe makes is (`0x00AE6E80`).
#[test]
fn construction_items_keep_their_active_byte_and_older_saves_read_active() {
    use ntw_sim::campaign::{ConstructionItem, ConstructionKind, SlotRef};
    let item = ConstructionItem { slot: SlotRef::Road, level_key: "made_up_road_2".into(), turns_remaining: 2, cost: 0, kind: ConstructionKind::Build, active: false };
    let mut model = made_up_model();
    model.world.regions.get_mut(&RegionId(1)).unwrap().construction = vec![item.clone()];
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let db = ntw_data::GameDatabase::test_fixture();
    let loaded = read(&write(&info(), &data).expect("write"), &db).expect("read");
    assert_eq!(loaded.model.world.regions[&RegionId(1)].construction, vec![item.clone()]);
    // An item as an older build wrote it: no `active` field.
    let text = ron::to_string(&ConstructionItem { active: true, ..item.clone() }).expect("encode");
    let older = text.replace("active:true", "");
    assert_ne!(older, text, "the field is in the text");
    let read_back: ConstructionItem = ron::from_str(&older).expect("decode");
    assert_eq!(read_back, ConstructionItem { active: true, ..item });
}

/// broadcasters3: format version 5 keeps each trade route's capital port flag (`TradePath::capital_port`). An
/// older save has none: its routes that leave by sea from their exporter's capital get it, the others not.
#[test]
fn trade_routes_keep_their_capital_port_flag_and_older_saves_derive_it() {
    let db = ntw_data::GameDatabase::test_fixture();
    let model = made_up_model();
    let load = |model: &CampaignModel, version: u32| {
        let data = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
        let bytes = as_version(&write(&info(), &data).expect("write"), version);
        let loaded = read_with_start(&bytes, &db, || Ok(model.clone())).expect("read").model;
        loaded.world.trade_paths[&(A, B)].iter().map(|p| p.capital_port).collect::<Vec<_>>()
    };
    assert_eq!(load(&model, 4), [true, false]);
    // The current version reads the flag as written, whatever the route.
    let mut flipped = model.clone();
    for p in flipped.world.trade_paths.get_mut(&(A, B)).unwrap() {
        p.capital_port = !p.capital_port;
    }
    assert_eq!(load(&flipped, FORMAT_VERSION), [false, true]);
    assert_eq!(load(&flipped, 4), [true, false]);
}

/// dipcounters review: format version 6 counts treaty breaks from the exe's preset 1 (`0x00AE8DD0` /
/// `0x00AE8E10`) and keeps the access-abuse counts. A version-5 save's treaty-break entries, counted
/// from 0, get 1 added; a faction without one reads the preset; it has no access-abuse counts (read 0).
#[test]
fn a_version_5_save_moves_its_treaty_break_counts_to_the_preset_start() {
    let db = ntw_data::GameDatabase::test_fixture();
    let mut model = made_up_model();
    model.world.treaty_breaks = BTreeMap::from([(A, 3)]);
    model.world.access_abuses = BTreeMap::from([(A, 2)]);
    let load = |version: u32| {
        let data = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
        let bytes = as_version(&write(&info(), &data).expect("write"), version);
        read_with_start(&bytes, &db, || Ok(model.clone())).expect("read").model.world
    };
    let old = load(5);
    assert_eq!(old.treaty_breaks, BTreeMap::from([(A, 4)]), "a stored count, from 0, gets the preset added");
    assert_eq!(old.treaty_breaks_of(B), ntw_sim::campaign::TREATY_BREAKS_START, "no entry reads the preset");
    assert!(old.access_abuses.is_empty(), "a version-5 save kept no access-abuse counts");
    // The current version reads both as written.
    let now = load(FORMAT_VERSION);
    assert_eq!((now.treaty_breaks[&A], now.access_abuses[&A]), (3, 2));
}

/// leftroutes: the route a broken agreement leaves (`0x00BC0120` removes only the first each way) is saved
/// and still earns after a load (`0x00BB3490` sums every route of the exporter).
#[test]
fn a_route_left_after_a_broken_agreement_still_earns_after_a_load() {
    use ntw_sim::campaign::economy;
    let mut model = made_up_model();
    let mut rules = (*model.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    model.rules = std::sync::Arc::new(rules);
    model.relationship_mut(A, B).trade_agreement = true;
    model.relationship_mut(B, A).trade_agreement = true;
    model.break_trade(A, B).expect("break");
    assert_eq!(model.world.trade_paths[&(A, B)].len(), 1);
    assert!(economy::trade_partners(&model, A).is_empty());
    let value = economy::trade_routes_value(&model, A);
    assert!(value > 0, "the leftover route earns its GDP part");
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let (_, mut back) = read_parts(&write(&info(), &data).expect("write")).expect("read");
    // The rules come from the data, not the save.
    back.model.rules = data.model.rules.clone();
    assert_eq!(back.model.world.trade_paths, data.model.world.trade_paths);
    assert_eq!(economy::trade_route_importers(&back.model, A), vec![B]);
    assert_eq!(economy::trade_routes_value(&back.model, A), value);
}

/// routeacc: format version 7 keeps each trade route's accumulated value (`TradePath::accumulated`, +0x3C,
/// `0x00B05CC0`) and the pair-level value only for a pair's built route. A version-6 save kept one value
/// per pair: a pair with stored routes gets it on its first route, a pair without keeps it.
#[test]
fn trade_routes_keep_their_own_accumulated_value_and_a_version_6_save_moves_its_pair_value() {
    let db = ntw_data::GameDatabase::test_fixture();
    let load = |model: &CampaignModel, version: u32| {
        let data = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
        let bytes = as_version(&write(&info(), &data).expect("write"), version);
        read_with_start(&bytes, &db, || Ok(model.clone())).expect("read").model.world
    };
    let model = made_up_model();
    let now = load(&model, FORMAT_VERSION);
    assert_eq!(now.trade_paths[&(A, B)].iter().map(|p| p.accumulated).collect::<Vec<_>>(), [4000, 321]);
    assert_eq!(now.trade_accumulated, BTreeMap::from([((B, A), 4321)]));
    // A version-6 save: the pair's total and no per-route field (`as_version` strips `accumulated:`, so
    // the routes' 4000 / 321 are not in the bytes and read 0).
    let mut old = model.clone();
    old.world.trade_accumulated.insert((A, B), 4321);
    let v6 = load(&old, 6);
    assert_eq!(v6.trade_paths[&(A, B)].iter().map(|p| p.accumulated).collect::<Vec<_>>(), [4321, 0]);
    assert_eq!(v6.trade_accumulated, BTreeMap::from([((B, A), 4321)]), "a pair without a stored route keeps its value");
}

/// traderaid: format version 8 keeps the commerce raids and the trade network's course they need (each leg's
/// spline ids, each segment's curves). A version-7 save holds no raids and takes the course from the
/// campaign's start; without a start it loads with a warning and no course.
#[test]
fn commerce_raids_round_trip_and_a_version_7_save_takes_the_trade_course_from_the_start() {
    let db = ntw_data::GameDatabase::test_fixture();
    let model = made_up_model();
    let bytes = |version: u32| {
        let data = SaveData { human: "made_up_a".into(), model: model.clone(), rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
        as_version(&write(&info(), &data).expect("write"), version)
    };
    let now = read_with_start(&bytes(FORMAT_VERSION), &db, || panic!("start read for a current save")).expect("read").model.world;
    assert_eq!(now.commerce_raids, model.world.commerce_raids);
    assert_eq!(now.trade_network, model.world.trade_network);
    assert_eq!(now.trade_sight, model.world.trade_sight);
    let old = read_with_start(&bytes(7), &db, || Ok(model.clone())).expect("read").model.world;
    assert!(old.commerce_raids.is_empty(), "a version-7 save holds no raids");
    assert_eq!(old.trade_network, model.world.trade_network, "the spline ids come from the start");
    assert_eq!(old.trade_sight, model.world.trade_sight, "the curves come from the start");
    let bare = read_with_start(&bytes(7), &db, || Err("no start".into())).expect("loads without a start").model.world;
    assert!(bare.trade_network.iter().all(|l| l.splines.is_empty()) && bare.trade_sight.iter().all(|s| s.curves.is_empty()));
}

/// The campaign-over flag (campaign +0xFE9, `TurnState::campaign_over`) saves and loads back; a save
/// of ours written before the field was kept loads it as false (`serde(default)`, no format bump).
#[test]
fn campaign_over_round_trips_and_an_older_save_loads_it_as_false() {
    let mut model = made_up_model();
    model.end_campaign();
    assert!(model.turn.queue.is_empty(), "ending the campaign clears the queue");
    let data = SaveData { human: "made_up_a".into(), model, rebel_faction: None, script_values: Vec::new(), restricted_units: Vec::new() };
    let (_, back) = read_parts(&write(&info(), &data).expect("write")).expect("read");
    assert!(back.model.turn.campaign_over);
    // An older turn state: the same record without the field, removed from the parsed value (not
    // from the text, so the test does not depend on RON's layout).
    let ron::Value::Map(mut fields) = ron::from_str::<ron::Value>(&ron::to_string(&back.model.turn).expect("encode")).expect("parse") else {
        panic!("a turn state is a record");
    };
    assert_eq!(fields.remove(&ron::Value::String("campaign_over".into())), Some(ron::Value::Bool(true)));
    let turn: ntw_sim::campaign::TurnState = ron::Value::Map(fields).into_rust().expect("an older turn state loads");
    assert!(!turn.campaign_over);
    assert_eq!(turn.humans, back.model.turn.humans, "the other fields load as they were");
    assert_eq!(turn.queue, back.model.turn.queue, "the (empty) queue loads as it was");
}

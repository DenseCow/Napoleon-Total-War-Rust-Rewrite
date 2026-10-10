use std::collections::BTreeMap;

use ntw_sim::calendar::{Calendar, Date, HALF_EARLY};
use ntw_sim::campaign::{
    CampaignModel, CampaignUnit, Character, CharacterId, CharacterKind, Faction, FactionAiKeys, FactionId, ForceId, GovernmentType,
    MilitaryForce, PendingBattle, Region, RegionId, Settlement, Stance, UnitId, World,
};
use ntw_sim::fixed::Fixed20;
use ntw_sim::rng::CaRng;

use super::*;
use crate::{FileKind, HeaderMap, PlayerSetup, SaveHeader};

const A: FactionId = FactionId(7);
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
    w.trade_accumulated.insert((A, B), 4321);
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
    let data = SaveData {
        human: "made_up_a".into(),
        model: made_up_model(),
        rebel_faction: Some(B),
        script_values: vec![ScriptSaveValue::Bool(true), ScriptSaveValue::Int(-12)],
        restricted_units: vec!["made_up_unit".into()],
    };
    let bytes = write(&info(), &data).expect("write");
    assert!(is_own_save(&bytes));
    assert_eq!(read_info(&bytes).expect("header"), info());
    assert_eq!(crate::read_info(&bytes).expect("header through the dispatcher"), info());
    let (h, back) = read_parts(&bytes).expect("read");
    assert_eq!(h, info());
    assert_eq!(back, data);
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

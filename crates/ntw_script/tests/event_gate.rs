//! The gate between a posted script event and Lua (`0x008F9EE0`), the context each event class
//! answers, and the dispatcher's live read of the handler list (`0x00E142B0`). Our own tiny scripts
//! and made-up tables: no install needed.

mod common;

use std::path::PathBuf;

use ntw_formats::db::{DbTable, DbValue, Schema};
use ntw_formats::db_folder::RawTable;
use ntw_formats::db_folder::tables::{TRIGGER_EVENTS, TRIGGER_EVENT_TO_EXCLUDED_AGENT_TYPES};
use ntw_formats::pack::{LayerKind, Vfs};
use ntw_script::{ScriptContext, ScriptHost, ScriptSource};
use ntw_sim::campaign::{
    BuildingRef, CampaignEvent, CampaignModel, Character, CharacterId, CharacterKind, Region, RegionId, Settlement, SlotRef,
};
use ntw_sim::fixed::Fixed20;

/// The event lists a handler can join, and one that counts every call by event name.
const EVENTS: &str = r#"
module(..., package.seeall)
CharacterTurnEnd = {}
CharacterCompletedBattle = {}
FactionTurnStart = {}
"#;

const SCRIPT: &str = r#"
local events = require "data.events"
calls = {}
for _, name in ipairs({ "CharacterTurnEnd", "CharacterCompletedBattle", "FactionTurnStart" }) do
    events[name][1] = function(context) calls[#calls+1] = name end
end
"#;

/// A temp folder holding made-up `trigger_events` and exclusion tables, removed on drop.
struct Tables(PathBuf);

impl Tables {
    fn new(tag: &str, events: &[&str], excluded: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("ntw_script_event_gate_{tag}_{}", std::process::id()));
        let write = |table: &RawTable, rows: Vec<Vec<DbValue>>| {
            let folder = dir.join("db").join(format!("{}_tables", table.name));
            std::fs::create_dir_all(&folder).unwrap();
            let schema = Schema::from_codes(table.codes).unwrap();
            let t = DbTable { version: 0, has_version_marker: false, flag: 1, rows };
            std::fs::write(folder.join(table.name), t.to_bytes(&schema).unwrap()).unwrap();
        };
        write(&TRIGGER_EVENTS, events.iter().map(|e| vec![DbValue::Str((*e).into())]).collect());
        if !excluded.is_empty() {
            let rows = excluded.iter().map(|(e, a)| vec![DbValue::Str((*e).into()), DbValue::Str((*a).into())]).collect();
            write(&TRIGGER_EVENT_TO_EXCLUDED_AGENT_TYPES, rows);
        }
        Tables(dir)
    }

    fn source(&self) -> ScriptSource {
        let mut vfs = Vfs::new();
        vfs.mount_dir(&self.0, LayerKind::Loose).unwrap();
        ScriptSource::from_vfs(vfs).with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT)
    }
}

impl Drop for Tables {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn add_character(model: &mut CampaignModel, id: i32, kind: CharacterKind) -> CharacterId {
    let faction = model.faction_by_key("france").unwrap().id;
    let c = CharacterId(id);
    model.world.characters.insert(
        c,
        Character {
            id: c,
            faction,
            kind,
            position: (Fixed20::from_int(0), Fixed20::from_int(0)),
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: None,
        },
    );
    c
}

/// A host over `source` with a general (7) and a colonel (8) of france's.
fn host(source: ScriptSource) -> ScriptHost {
    let mut model = common::tiny_model();
    add_character(&mut model, 7, CharacterKind::General);
    add_character(&mut model, 8, CharacterKind::Colonel);
    let h = ScriptHost::new(model, "france", source).unwrap();
    h.run_file("data/test.lua").unwrap();
    h
}

fn of(character: i32) -> ScriptContext {
    ScriptContext { character: Some(character), ..Default::default() }
}

fn calls(h: &ScriptHost) -> Vec<String> {
    h.lua().load("return calls").eval().unwrap()
}

/// Step 1 of `0x008F9EE0`: a gated class's event with no character is not handed to Lua, whatever
/// else the caller gave it; an open class's event (gate `0x00447880`) needs none.
#[test]
fn a_gated_event_without_a_character_never_reaches_lua() {
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = host(src);
    assert_eq!(h.fire("CharacterTurnEnd", ScriptContext::default()).handlers, 0);
    assert_eq!(h.fire("CharacterTurnEnd", ScriptContext::for_faction("france")).handlers, 0);
    assert_eq!(h.fire("CharacterTurnEnd", of(7)).handlers, 1);
    assert_eq!(h.fire("FactionTurnStart", ScriptContext::default()).handlers, 1);
    assert_eq!(calls(&h), ["CharacterTurnEnd", "FactionTurnStart"]);
}

/// Step 2: a gated event whose name is not a `trigger_events` key is not handed to Lua (the exe logs
/// the name as not a valid key); one with a row is. An open class's event never asks the table.
#[test]
fn a_gated_event_needs_its_trigger_events_row() {
    let tables = Tables::new("row", &["CharacterTurnEnd"], &[]);
    let h = host(tables.source());
    assert_eq!(h.fire("CharacterCompletedBattle", of(7)).handlers, 0, "no row: no Lua");
    assert_eq!(h.fire("CharacterCompletedBattle", of(7)).handlers, 0, "asked again: still none");
    assert_eq!(h.fire("CharacterTurnEnd", of(7)).handlers, 1, "its row is there");
    assert_eq!(h.fire("FactionTurnStart", ScriptContext::for_faction("france")).handlers, 1, "not a gated class");
    assert_eq!(calls(&h), ["CharacterTurnEnd", "FactionTurnStart"]);
    // The handle the UI and the app fire through shares the gate.
    assert_eq!(h.handle().fire("CharacterCompletedBattle", of(7)).handlers, 0);
}

/// Step 3 (`0x00EC5360`): a row of `trigger_event_to_excluded_agent_types` keeps the event from the
/// handlers when its character is of that agent type, and only then. The keys are compared exactly
/// (`0x004EFB20`), so a row spelt in another case excludes nobody.
#[test]
fn an_excluded_agent_type_stops_the_event() {
    let events = ["CharacterTurnEnd", "CharacterCompletedBattle"];
    let tables = Tables::new("excl", &events, &[("CharacterTurnEnd", "General"), ("CharacterCompletedBattle", "COLONEL")]);
    let h = host(tables.source());
    assert_eq!(h.fire("CharacterTurnEnd", of(7)).handlers, 0, "the general's type is excluded");
    assert_eq!(h.fire("CharacterTurnEnd", of(8)).handlers, 1, "the colonel's is not");
    assert_eq!(h.fire("CharacterCompletedBattle", of(7)).handlers, 1, "the row names another event");
    assert_eq!(h.fire("CharacterCompletedBattle", of(8)).handlers, 1, "COLONEL is not the key colonel");
    assert_eq!(calls(&h), ["CharacterTurnEnd", "CharacterCompletedBattle", "CharacterCompletedBattle"]);
}

fn region(id: u32, owner: ntw_sim::campaign::FactionId) -> Region {
    Region {
        id: RegionId(id),
        key: format!("test_region_{id}"),
        owner,
        settlement: Settlement { key: format!("settlement:test_region_{id}:town"), position: (Fixed20::from_int(0), Fixed20::from_int(0)) },
        slots: Vec::new(),
        road: None,
        fortification: Some(BuildingRef { level_key: "test_walls_1".into(), health: 100 }),
        road_holder: None,
        fortification_holder: None,
        population: 1000,
        base_gdp: 100,
        gdp: 100,
        wealth_growth_offset: 0,
        discontent_growth: 0,
        town_wealth: 0,
        town_wealth_growth: 0,
        town_wealth_refresh: Default::default(),
        commodity_demand: Vec::new(),
        tax_exempt: false,
        religions: Vec::new(),
        class_bases: Vec::new(),
        unit_resources: Vec::new(),
        population_state: Default::default(),
        recruitment_queue: Vec::new(),
        construction: Vec::new(),
        garrison: None,
        fleet: None,
        siege: Default::default(),
        traits: Vec::new(),
    }
}

/// What the host gathers for a model event is what the event's exe class answers: an accessor the
/// class's vtable leaves to the null stub (`0x00461180`) gives the handlers nothing, whoever the
/// event concerns.
#[test]
fn a_model_events_context_is_what_its_class_answers() {
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    let c = add_character(&mut model, 7, CharacterKind::General);
    let r = RegionId(1);
    model.world.regions.insert(r, region(1, fr));
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(model, "france", src).unwrap();
    let town = Some("settlement:test_region_1:town".to_string());
    let only_character = ScriptContext { character: Some(7), ..Default::default() };
    let only_settlement = ScriptContext { settlement: town.clone(), ..Default::default() };
    let only_region = ScriptContext { region: Some("test_region_1".into()), ..Default::default() };
    // SettlementOccupied (vtable 0x01355EC0): the settlement; +0x38 (faction) and +0x28 (region) are null.
    assert_eq!(h.context_for(&CampaignEvent::SettlementOccupied { region: r, faction: fr }), only_settlement);
    // The character classes (e.g. 0x013558E0): the character and no faction.
    assert_eq!(h.context_for(&CampaignEvent::CharacterCompletedBattle { character: c }), only_character);
    assert_eq!(h.context_for(&CampaignEvent::CharacterTurnEnd { character: c }), only_character);
    assert_eq!(h.context_for(&CampaignEvent::CharacterCreated { character: c }), only_character);
    // CharacterEntersGarrison (0x013561B0): the character and the settlement it entered.
    assert_eq!(
        h.context_for(&CampaignEvent::CharacterEntersGarrison { character: c, region: r }),
        ScriptContext { character: Some(7), settlement: town.clone(), ..Default::default() }
    );
    // The region classes (0x01376DB8 / 0x01376CFC / 0x01376F30): the region, no settlement or faction.
    assert_eq!(h.context_for(&CampaignEvent::RegionTurnStart { region: r }), only_region);
    assert_eq!(h.context_for(&CampaignEvent::RegionTurnEnd { region: r }), only_region);
    // The unrest classes (0x013566D4 / 0x0135684C / 0x01356618 / 0x01356790): RegionTurnStart's accessors.
    assert_eq!(h.context_for(&CampaignEvent::RegionRiots { region: r }), only_region);
    assert_eq!(h.context_for(&CampaignEvent::RegionStrikes { region: r }), only_region);
    assert_eq!(h.context_for(&CampaignEvent::RegionIssuesDemands { region: r }), only_region);
    assert_eq!(h.context_for(&CampaignEvent::RegionRebels { region: r }), only_region);
    // The building classes: the settlement (through the slot's residence), and for a finished one its level.
    assert_eq!(h.context_for(&CampaignEvent::CampaignBuildingDamaged { region: r, slot: SlotRef::Walls }), only_settlement);
    assert_eq!(
        h.context_for(&CampaignEvent::BuildingCompleted { region: r, slot: SlotRef::Walls }),
        ScriptContext { settlement: town, building_level: Some("test_walls_1".into()), ..Default::default() }
    );
    // The faction classes keep their faction.
    assert_eq!(h.context_for(&CampaignEvent::FactionTurnStart { faction: fr }), ScriptContext::for_faction("france"));
    assert_eq!(h.context_for(&CampaignEvent::PendingBankruptcy { faction: fr }), ScriptContext::for_faction("france"));
    // Classes with nothing our context carries (tax payload +0xAC, research: no faction accessor).
    assert_eq!(h.context_for(&CampaignEvent::BuildingConstructionIssuedByPlayer { region: r }), ScriptContext::default());
    assert_eq!(h.context_for(&CampaignEvent::RecruitmentItemIssuedByPlayer { region: r }), ScriptContext::default());
    assert_eq!(
        h.context_for(&CampaignEvent::ResearchCompleted { faction: fr, technology: "test_tech".into() }),
        ScriptContext::default()
    );
}

/// A `SettlementOccupied` handler cannot ask the faction (the class has no accessor: `FactionName` and
/// `FactionIsLocal` are false), only the settlement.
#[test]
fn a_settlement_occupied_handler_reads_the_settlement_and_no_faction() {
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    model.world.regions.insert(RegionId(1), region(1, fr));
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.exec(
        r#"local e = require "data.events"
           e.SettlementOccupied = {}
           e.SettlementOccupied[1] = function(context)
               occupied = {
                   conditions.SettlementName("settlement:test_region_1:town", context),
                   conditions.FactionName("france", context),
                   conditions.FactionIsLocal(context),
               }
           end"#,
    )
    .unwrap();
    let ev = CampaignEvent::SettlementOccupied { region: RegionId(1), faction: fr };
    // Even a caller that hands the event a faction: the class masks it.
    let ctx = ScriptContext { faction: Some("france".into()), ..h.context_for(&ev) };
    let r = h.fire("SettlementOccupied", ctx);
    assert_eq!((r.handlers, r.errors), (1, vec![]));
    let seen: Vec<bool> = h.lua().load("return occupied").eval().unwrap();
    assert_eq!(seen, [true, false, false]);
}

/// A list entry that is not a function is reported and passed over, and the entries after it still
/// run (`0x01057D80` logs the type mismatch and calls nothing).
#[test]
fn an_entry_that_is_not_a_function_is_reported_and_passed_over() {
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = host(src);
    h.exec(
        r#"local e = require "data.events"
           e.FactionTurnStart = { "not a handler", function() after = true end }"#,
    )
    .unwrap();
    let r = h.fire("FactionTurnStart", ScriptContext::for_faction("france"));
    assert_eq!(r.handlers, 1);
    assert_eq!(r.errors, ["FactionTurnStart: handler 1 is not a function (string)"]);
    assert!(h.lua().load("return after").eval::<bool>().unwrap());
    assert!(h.state().log.iter().any(|l| l.contains("handler 1 is not a function")));
}

/// The dispatcher `0x00E142B0` takes the list's length once and reads each entry when its turn
/// comes: a handler that replaces a later entry changes what runs this time; one it appends waits
/// for the next post.
#[test]
fn the_handler_list_is_read_live_but_its_length_once() {
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = host(src);
    h.exec(
        r#"local e = require "data.events"
           ran = {}
           e.FactionTurnStart = {
               function()
                   ran[#ran+1] = "first"
                   e.FactionTurnStart[2] = function() ran[#ran+1] = "replacement" end
                   e.FactionTurnStart[3] = function() ran[#ran+1] = "appended" end
               end,
               function() ran[#ran+1] = "second" end,
           }"#,
    )
    .unwrap();
    assert_eq!(h.fire("FactionTurnStart", ScriptContext::for_faction("france")).handlers, 2);
    let ran: Vec<String> = h.lua().load("return ran").eval().unwrap();
    assert_eq!(ran, ["first", "replacement"]);
    assert_eq!(h.fire("FactionTurnStart", ScriptContext::for_faction("france")).handlers, 3);
    let ran: Vec<String> = h.lua().load("return ran").eval().unwrap();
    assert_eq!(ran, ["first", "replacement", "first", "replacement", "appended"]);
}

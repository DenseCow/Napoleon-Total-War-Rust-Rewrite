//! The exe's campaign script event classes: what each one's context answers, and which are gated.
//!
//! The exe posts a script event as a C++ object (`0x00E30700`, synchronous: no queue). The object's
//! vtable has 47 slots; slot 0 names the event, the last one (+0xB8) is the gate the post path asks
//! before Lua, and the ones between are the accessors the `conditions.*` functions call on the
//! context. A class that does not have an object answers that accessor with a null stub
//! (`0x00461180`), and the condition reading it is then false. [`CLASSES`] is that fact per event,
//! read from the vtable bytes (CONFIRMED, each row's address), for the accessors [`ScriptContext`]
//! carries:
//!
//! | slot | accessor | read by (handler) |
//! |---|---|---|
//! | +0x14 | string | `IsComponentType` `0x00DA9750` |
//! | +0x18 | UI component | `IsComponentType` |
//! | +0x1C | character | `CharacterType` `0x0089E0D0`, `CharacterFactionName` `0x0089CAE0` |
//! | +0x28 | region | |
//! | +0x30 | settlement | `SettlementName` `0x008A45A0`, `SettlementIsLocal` `0x008A4520` |
//! | +0x38 | faction | `FactionName` `0x0089FC90`, `FactionIsLocal` `0x0089FA60`, `FactionTreasury` `0x008A0860` |
//! | +0x48 | historical event candidates | |
//! | +0x4C | mission | `MissionName` `0x009BE760` |
//! | +0x64 | building | `BuildingLevelName` `0x00B01ED0` |
//!
//! The slots our context has no field for (+0x20 second character, +0x24 unit, +0x2C slot, +0x34 fort,
//! +0x40 garrison residence, +0x5C trade route, +0x68 technology, +0xA0..+0xA8 battle results, +0xAC
//! the tax event's payload) are in the analysis notes (CAMPAIGN_FIDELITY.md "Script event classes").
//!
//! These are engine classes, not content: a mod adds handlers and `trigger_events` rows, never a
//! class, so the table is code. An event with no row here is one the exe has no class for (or one
//! whose vtable is not read yet): it is offered as its caller built it and is not gated.
//!
//! [`EventGate`] is the one place both facts are applied, where the host hands an event to Lua.

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ntw_formats::db::DbValue;
use ntw_formats::db_folder::tables::{TRIGGER_EVENTS, TRIGGER_EVENT_TO_EXCLUDED_AGENT_TYPES};
use ntw_sim::campaign::{CharacterId, CharacterKind};

use crate::source::ScriptSource;
use crate::state::{ScriptContext, ScriptState};

/// A set of context accessors (the vtable slots of the module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offers(u16);

impl Offers {
    /// No accessor our context carries.
    pub const NONE: Offers = Offers(0);
    /// +0x14.
    pub const STRING: Offers = Offers(1);
    /// +0x18.
    pub const COMPONENT: Offers = Offers(1 << 1);
    /// +0x1C.
    pub const CHARACTER: Offers = Offers(1 << 2);
    /// +0x28.
    pub const REGION: Offers = Offers(1 << 3);
    /// +0x30.
    pub const SETTLEMENT: Offers = Offers(1 << 4);
    /// +0x38.
    pub const FACTION: Offers = Offers(1 << 5);
    /// +0x48.
    pub const HISTORICAL_EVENTS: Offers = Offers(1 << 6);
    /// +0x4C.
    pub const MISSION: Offers = Offers(1 << 7);
    /// +0x64.
    pub const BUILDING: Offers = Offers(1 << 8);

    /// Both sets.
    pub const fn and(self, other: Offers) -> Offers {
        Offers(self.0 | other.0)
    }

    /// True when every accessor of `other` is in this set.
    pub const fn has(self, other: Offers) -> bool {
        self.0 & other.0 == other.0
    }
}

/// One script event class of the exe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventClass {
    /// The event's name (vtable slot 0 returns it).
    pub name: &'static str,
    /// The class vtable in Napoleon.exe; `None` for the one event of this table the exe has no class for
    /// (`CharacterTurnStart`, see [`CLASSES`]).
    pub vtable: Option<u32>,
    /// The class's gate (+0xB8) is `0x008F9EE0`: the event reaches Lua only with a character, a
    /// `trigger_events` row of its name and an agent type the row does not exclude ([`EventGate`]).
    /// False: the gate is the "always" stub `0x00447880`.
    pub gated: bool,
    /// The accessors the class has.
    pub offers: Offers,
}

impl EventClass {
    /// `ctx` with only what this class's accessors answer: the rest reads as the exe's null stubs do.
    pub fn offer(&self, ctx: ScriptContext) -> ScriptContext {
        let keep = |o: Offers| self.offers.has(o);
        ScriptContext {
            string: ctx.string.filter(|_| keep(Offers::STRING)),
            component: ctx.component.filter(|_| keep(Offers::COMPONENT)),
            faction: ctx.faction.filter(|_| keep(Offers::FACTION)),
            region: ctx.region.filter(|_| keep(Offers::REGION)),
            settlement: ctx.settlement.filter(|_| keep(Offers::SETTLEMENT)),
            character: ctx.character.filter(|_| keep(Offers::CHARACTER)),
            mission: ctx.mission.filter(|_| keep(Offers::MISSION)),
            building_level: ctx.building_level.filter(|_| keep(Offers::BUILDING)),
            historical_events: ctx.historical_events && keep(Offers::HISTORICAL_EVENTS),
        }
    }
}

/// The class of script event `name`, if the exe has one in [`CLASSES`].
pub fn class_of(name: &str) -> Option<&'static EventClass> {
    CLASSES.binary_search_by(|c| c.name.cmp(name)).ok().map(|i| &CLASSES[i])
}

const fn gated(name: &'static str, vtable: u32) -> EventClass {
    EventClass { name, vtable: Some(vtable), gated: true, offers: Offers::CHARACTER }
}

const fn open(name: &'static str, vtable: u32, offers: Offers) -> EventClass {
    EventClass { name, vtable: Some(vtable), gated: false, offers }
}

const SETTLEMENT_CHARACTER: Offers = Offers::SETTLEMENT.and(Offers::CHARACTER);

/// The classes, sorted by name ([`class_of`] searches it; a test checks the order).
///
/// The 32 gated classes all answer a character and nothing else our context carries: none has a
/// faction, region or settlement accessor (a character condition finds the faction through the
/// character). The four garrison events answer the settlement through their garrison residence
/// (+0x40, the event's first object; `0x008EF740` asks the residence), never a faction or a region.
/// `0x008EF740` returns the residence's vtable +0x58, which answers a settlement only for the settlement
/// residence itself (vtable `0x0137C9A4` +0x58 is `0x004470B0`, "return this"); a fort's (`0x0137D3E4`)
/// and a region slot's (`0x0137695C`) +0x58 is `0x00461180`, null (CONFIRMED, vtable reads). The two
/// building events carry the building's slot object (`0x00B6D260`): `CampaignBuildingDamaged` always
/// (`0x00B452BC`), `BuildingCompleted` unless `0x00A8B790` (that object's +0x1DC, then +0x40) sends it
/// the region's settlement instead (vt+0x34, then +0xFC; `0x00B78352`). INFERRED: a slot outside the
/// town (a farm, a port) is a region slot there, so the exe offers no settlement for it, while ours
/// (host.rs) answers the region's settlement as the town's does; which object `0x00B6D260` returns per
/// slot kind, and what the +0x40 flag means, are not traced.
///
/// ORIGINAL BUG: `CharacterTurnStart` is declared to the scripts (`0x00419EB0`: "Fired for every
/// character at the start of their turn"; its name string `0x0134F898` has that one reference in the
/// whole image) and the shipped scripts register five handlers for it, but no event class returns
/// the name, so the exe never posts it and those handlers never run. Ours posts it
/// (`ntw_sim::campaign::turn`), as every other `Character*` event is: gated, with the character.
pub const CLASSES: &[EventClass] = &[
    gated("ArmySabotageAttemptSuccess", 0x0136_1EE8),
    gated("AssassinationAttemptSuccess", 0x0136_1D70),
    open("BattleCompleted", 0x0135_6B3C, Offers::NONE),
    // The character is the event's second object (+0x1C returns event +0x14), the building its third.
    open("BuildingCompleted", 0x0137_CEEC, SETTLEMENT_CHARACTER.and(Offers::BUILDING)),
    open("BuildingConstructionIssuedByPlayer", 0x0137_D2F8, Offers::BUILDING),
    // Its second character (+0x20, event +0x14) has no field in our context.
    gated("CampaignArmiesMerge", 0x0135_5824),
    open("CampaignBuildingDamaged", 0x0137_CE30, Offers::SETTLEMENT),
    gated("CharacterAttacksAlly", 0x0135_599C),
    gated("CharacterBlockadedPort", 0x0137_6C40),
    gated("CharacterBrokePortBlockade", 0x0135_5C8C),
    gated("CharacterBuildsSpyNetwork", 0x0135_5BD0),
    gated("CharacterCanLiberate", 0x0137_CD74),
    gated("CharacterCandidateBecomesMinister", 0x0135_5D48),
    gated("CharacterCompletedBattle", 0x0135_58E0),
    gated("CharacterCreated", 0x0135_5768),
    gated("CharacterCriticallyFailsAssassination", 0x0136_24C8),
    gated("CharacterDisembarksNavy", 0x0135_5B14),
    gated("CharacterEmbarksNavy", 0x0135_5A58),
    gated("CharacterEntersAttritionalArea", 0x0136_AA38),
    open("CharacterEntersGarrison", 0x0135_61B0, SETTLEMENT_CHARACTER),
    gated("CharacterFactionSpyAttemptSuccessful", 0x0136_2584),
    gated("CharacterFactionSuffersSuccessfulSpyAttempt", 0x0136_2640),
    gated("CharacterLootedSettlement", 0x0137_CCB8),
    gated("CharacterPromoted", 0x0136_A97C),
    gated("CharacterSelected", 0x0136_A804),
    gated("CharacterTurnEnd", 0x0136_A748),
    EventClass { name: "CharacterTurnStart", vtable: None, gated: true, offers: Offers::CHARACTER },
    gated("DuelFought", 0x0136_1BF8),
    gated("EspionageAgentApprehended", 0x0136_21D8),
    open("FactionBecomesLiberationProtectorate", 0x0137_D064, Offers::FACTION),
    open("FactionGovernmentTypeChanged", 0x0135_64A0, Offers::FACTION),
    open("FactionRoundStart", 0x0135_63E4, Offers::FACTION),
    open("FactionTurnEnd", 0x0135_626C, Offers::FACTION),
    open("FactionTurnStart", 0x0135_6328, Offers::FACTION),
    // Only its own payload accessor (+0xAC); no faction.
    open("GovernorshipTaxRateChanged", 0x0135_6BF8, Offers::NONE),
    gated("HarassmentAttemptSuccess", 0x0136_1FA4),
    open("HistoricalEvents", 0x0135_8184, Offers::HISTORICAL_EVENTS),
    gated("LandTradeRouteRaided", 0x0136_2350),
    gated("MovementPointsExhausted", 0x0136_A8C0),
    gated("MultiTurnMove", 0x0136_2294),
    open("PendingBankruptcy", 0x0135_655C, Offers::FACTION),
    open("RecruitmentItemIssuedByPlayer", 0x0137_DB44, Offers::NONE),
    // The four unrest events' vtables are `RegionTurnStart`'s but for slot 0 (the name).
    open("RegionIssuesDemands", 0x0135_6618, Offers::REGION),
    open("RegionRebels", 0x0135_6790, Offers::REGION),
    open("RegionRiots", 0x0135_66D4, Offers::REGION),
    open("RegionStrikes", 0x0135_684C, Offers::REGION),
    open("RegionTurnEnd", 0x0137_6CFC, Offers::REGION),
    open("RegionTurnStart", 0x0137_6DB8, Offers::REGION),
    // The character (+0x1C, the event's first object) and the technology (+0x68); no faction.
    open("ResearchCompleted", 0x0135_6038, Offers::CHARACTER),
    gated("SabotageAttemptSuccess", 0x0136_1E2C),
    gated("SeaTradeRouteRaided", 0x0136_240C),
    open("SettlementOccupied", 0x0135_5EC0, Offers::SETTLEMENT),
    // The region (+0x28, event +0x14) and the slot (+0x2C); no settlement.
    open("SlotTurnStart", 0x0137_6F30, Offers::REGION),
    gated("SpyingAttemptSuccess", 0x0136_1CB4),
    gated("SufferAssassinationAttempt", 0x0136_211C),
    gated("SufferSpyingAttempt", 0x0136_2060),
    open("TimeTrigger", 0x0137_0C4C, Offers::STRING),
    open("TradeRouteEstablished", 0x0138_0C70, Offers::FACTION),
    // Only the unit (+0x24).
    open("UnitTrained", 0x0137_D120, Offers::NONE),
    open("UnitTurnEnd", 0x0135_69C4, Offers::NONE),
    // Its vtable differs from the met class's only in the name (slot 0) and the faction getter (+0x38,
    // the null stub 0x00461180 here); the gate (+0xB8) is the "always" stub 0x00447880 in both.
    open("VictoryConditionFailed", 0x0135_FD98, Offers::NONE),
    open("VictoryConditionMet", 0x0135_FE54, Offers::FACTION),
];

/// What the gate `0x008F9EE0` reads from the database (mods included).
struct TriggerEvents {
    /// The `trigger_events` keys.
    names: HashSet<String>,
    /// Event name → the agent type keys of its `trigger_event_to_excluded_agent_types` rows (the
    /// list the record constructor `0x00E8FD50` gathers for each `trigger_events` record).
    excluded: HashMap<String, Vec<String>>,
}

impl TriggerEvents {
    /// Both tables through the merged database; `None` when `trigger_events` is not there (no VFS, or
    /// the table is missing, which the source logs once).
    fn read(source: &ScriptSource) -> Option<Self> {
        let text = |row: &[DbValue], i: usize| row.get(i).and_then(DbValue::as_str).map(str::to_owned);
        let names = source.table_rows_shared(&TRIGGER_EVENTS)?.iter().filter_map(|r| text(r, 0)).collect();
        let mut excluded: HashMap<String, Vec<String>> = HashMap::new();
        for row in source.table_rows_or_empty(&TRIGGER_EVENT_TO_EXCLUDED_AGENT_TYPES).iter() {
            if let (Some(event), Some(agent)) = (text(row, 0), text(row, 1)) {
                excluded.entry(event).or_default().push(agent);
            }
        }
        Some(TriggerEvents { names, excluded })
    }
}

/// The gate between a posted script event and Lua: slot +0xB8 of the event's class, which the post
/// path `0x00E30700` asks before the dispatcher `0x00E142B0`. One per Lua state.
///
/// For the 32 classes whose slot is `0x008F9EE0` (and `CharacterTurnStart`, see [`CLASSES`]) the event
/// reaches Lua only when, in the exe's order (CONFIRMED):
/// 1. its character (+0x1C) is not null (ours: the context names a character; one the model no
///    longer has passes, since the exe posts while he is there and ours fires after the step that
///    may have removed him, e.g. a duellist or an assassination target killed);
/// 2. `trigger_events` has a record of the event's name (the exe logs the name as not a valid key
///    otherwise; ours logs it once per name);
/// 3. the character's agent type (character +0x1AC, the `agents` record; its key) is not in that
///    record's exclusion list (`0x00EC5360`; keys compared exactly, `0x004F0F40` →
///    `CompareUniStrings`). Ours reads the type the event captured when it was posted
///    ([`ntw_sim::campaign::CampaignEvent::agent_type`]), else the model's; with neither (a
///    hand-fired event naming a character the model lacks) the step is skipped, logged once per
///    event name.
///
/// Every other class's slot is `0x00447880`, which always says yes.
///
/// The tables are read on the first gated event (the exe builds its table on first use too,
/// `0x00E30810`) and kept: the source reads a table once and the install does not change under a
/// running host. Without the table (a host with no VFS: our own test scripts) only step 1 applies;
/// the exe cannot run without its database.
pub struct EventGate {
    source: Rc<ScriptSource>,
    tables: OnceCell<Option<TriggerEvents>>,
    /// The names step 2 refused and logged.
    refused: RefCell<HashSet<&'static str>>,
    /// The names step 3 was skipped for (no agent type known), logged once each.
    unchecked: RefCell<HashSet<&'static str>>,
}

impl EventGate {
    /// The gate reading `source`'s database.
    pub fn new(source: Rc<ScriptSource>) -> Self {
        EventGate { source, tables: OnceCell::new(), refused: RefCell::new(HashSet::new()), unchecked: RefCell::new(HashSet::new()) }
    }

    /// What Lua is handed for event `name` built with `ctx`: the context its class's accessors
    /// answer ([`EventClass::offer`]), or `None` when the class's gate keeps the event from Lua.
    /// `agent_type` is the context character's type when the event was posted, `None` to read it
    /// from the model.
    pub fn admit(&self, state: &ScriptState, name: &str, ctx: ScriptContext, agent_type: Option<CharacterKind>) -> Option<ScriptContext> {
        let Some(class) = class_of(name) else { return Some(ctx) };
        let ctx = class.offer(ctx);
        if !class.gated {
            return Some(ctx);
        }
        let id = CharacterId(ctx.character?);
        let Some(tables) = self.tables.get_or_init(|| TriggerEvents::read(&self.source)) else { return Some(ctx) };
        if !tables.names.contains(name) {
            if self.refused.borrow_mut().insert(class.name) {
                eprintln!("WARN ntw_script: script event {name} is not a trigger_events key; its handlers never run (0x008F9EE0)");
            }
            return None;
        }
        if let Some(agents) = tables.excluded.get(name) {
            let Some(kind) = agent_type.or_else(|| state.model.world.agent_type(id)) else {
                if self.unchecked.borrow_mut().insert(class.name) {
                    eprintln!(
                        "WARN ntw_script: script event {name} names character {}, whose agent type is not known; its agent-type exclusions are not checked (0x008F9EE0)",
                        id.0
                    );
                }
                return Some(ctx);
            };
            if agents.iter().any(|a| a == kind.esf_name()) {
                return None;
            }
        }
        Some(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_classes_are_sorted_and_found_by_name() {
        assert!(CLASSES.windows(2).all(|w| w[0].name < w[1].name), "class_of needs the names in order");
        assert_eq!(CLASSES.iter().filter(|c| c.gated && c.vtable.is_some()).count(), 32, "the classes sharing 0x008F9EE0");
        for c in CLASSES {
            assert_eq!(class_of(c.name), Some(c));
        }
        assert_eq!(class_of("PreBattle"), None);
    }

    /// A gated class answers its character and nothing else, whatever the caller gathered.
    #[test]
    fn a_class_offers_only_its_accessors() {
        let all = ScriptContext {
            string: Some("s".into()),
            component: Some("c".into()),
            faction: Some("france".into()),
            region: Some("r".into()),
            settlement: Some("t".into()),
            character: Some(7),
            mission: Some("m".into()),
            building_level: Some("b".into()),
            historical_events: true,
        };
        let only = |name: &str| class_of(name).unwrap().offer(all.clone());
        assert_eq!(only("CharacterCompletedBattle"), ScriptContext { character: Some(7), ..Default::default() });
        assert_eq!(only("SettlementOccupied"), ScriptContext { settlement: Some("t".into()), ..Default::default() });
        assert_eq!(only("RegionTurnStart"), ScriptContext { region: Some("r".into()), ..Default::default() });
        assert_eq!(only("BattleCompleted"), ScriptContext::default());
        assert_eq!(only("VictoryConditionFailed"), ScriptContext::default());
        assert_eq!(only("VictoryConditionMet"), ScriptContext { faction: Some("france".into()), ..Default::default() });
        assert_eq!(only("HistoricalEvents"), ScriptContext { historical_events: true, ..Default::default() });
    }
}

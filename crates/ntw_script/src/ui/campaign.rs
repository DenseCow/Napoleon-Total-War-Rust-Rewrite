//! The campaign HUD's engine side: the `CampaignUI.*` functions the original campaign UI scripts
//! call, and the calls the engine makes into the HUD's Lua (selection, review-panel tabs, funds).
//!
//! Every `CampaignUI` function here is the original's script binding of the same name (handler
//! addresses in `analysis/worker1/script_bindings.tsv`, registrar `0x00998C50`). Return shapes come
//! from the exe (Ghidra, see `analysis/campaign/CAMPAIGN_UI.md`) and from the fields the original
//! scripts read. Tags: CONFIRMED / INFERRED / UNKNOWN; our stand-ins PROVISIONAL / PLACEHOLDER.
//! Functions not implemented here stay logging stubs (`UNKNOWN CampaignUI.X`).
//!
//! The campaign model is read live from the campaign script host's state
//! ([`crate::ScriptHost::shared_state`]); the HUD never changes the model itself. What the player
//! asks for (end turn, recruit, build, tax) is queued as a [`CampaignRequest`] for the game to
//! apply through the script host, so the campaign scripts see every event.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, LightUserData, Lua, Table, Value, Variadic};
use ntw_data::GameDatabase;
use ntw_sim::campaign::{
    CampaignCommand, CampaignModel, CharacterId, CharacterKind, ConstructionOption, ForceId, FactionId, MilitaryForce, RecruitmentItemId, RegionId,
    SlotRef, UnitId, economy,
};

use super::host::{Inner, UiScriptHost, addr, log, state_function_guard};
use super::world::{NodeId, UiRect};
use crate::ScriptState;

const PRELUDE: &str = include_str!("campaign_prelude.lua");

/// What the HUD needs from the running campaign.
pub struct CampaignLink {
    /// The campaign script host's state (it owns the model). Only borrowed while a `CampaignUI`
    /// function runs; never while the campaign scripts run.
    pub state: Rc<RefCell<ScriptState>>,
    /// The human player's faction key.
    pub human: String,
    /// Campaign key, e.g. `eur_napoleon`.
    pub campaign: String,
    /// The game database (units, buildings, factions).
    pub db: Rc<GameDatabase>,
}

/// What is selected on the campaign map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignSelection {
    /// Nothing.
    None,
    /// A character (a general with his army, an admiral with his navy, or an agent).
    Character(CharacterId),
    /// A settlement (its region).
    Settlement(RegionId),
    /// A fort, as an entity of its own. CONFIRMED that the original has one: the `campaign_fort`
    /// entity (Ghidra `0x00965550`), its two builders `player_fort` / `non_player_fort`
    /// (`0x00DE55F0`) and the game script event `FortSelected` (`worker3/lua_api.txt:627`, declared,
    /// one registration). Its forts are the save's `FORT_ARRAY` -- a child of `REGION`, **CONFIRMED**
    /// present and **empty (0 items) in all eight shipped start positions and all ten vanilla
    /// saves** -- which `ntw_campaign` now loads into [`ntw_sim::campaign::World::forts`] (0-E,
    /// 2026-10-06). The fort's model is closed from the data by 0-D round 9 (`CAMPAIGN_MAP.md` §10.2:
    /// chain `fFort` level *n* draws `fort_lvl<n+1>`); nothing draws it yet.
    ///
    /// PROVISIONAL payload: the **region**. The fort belongs to a region, so the region is the handle
    /// the whole chain carries (`play::MapPick`, `CampaignSim::selected_fort`, this enum). It is not
    /// the settlement walls ([`ntw_sim::campaign::Region::fortification`]), which are the settlement
    /// construction panel's last slot card. The cost: **two forts in one region cannot be told apart** (`World::forts` keys them by id, nothing else does yet).
    ///
    /// The fort panel's tab builder was never traced (only the character panel's `FUN_00985F40` and
    /// the settlement panel's `FUN_0099A200` are), so the tabs a fort shows are INFERRED (see
    /// `tabs_for`). A settlement's own `infrastructure_tab` is its road slot (CONFIRMED,
    /// `0x0099A200` / `0x00A021B0`), not this fort panel.
    Fort(RegionId),
}

/// Something the HUD asked the game to do.
#[derive(Debug, Clone, PartialEq)]
pub enum CampaignRequest {
    /// `CampaignUI.EndTurn()`.
    EndTurn,
    /// A model command to apply through the campaign script host (recruit, build, tax, ...).
    Command(CampaignCommand),
    /// `CampaignUI.SelectAndZoomTo*` / a card double click: select this and centre the camera.
    Select(CampaignSelection),
    /// Centre the camera on a map position (logic x, y).
    CameraTo(f32, f32),
}

/// A review-panel tab (INFERRED from the exe's tab objects, see CAMPAIGN_UI.md §Review panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Army,
    Navy,
    Recruitment,
    /// The naval half of the recruitment panel (0-E round N+2). The tab's key is CONFIRMED as the
    /// exe string `naval_recruitment_tab` (0x013CC71C, beside `recruitment_tab` 0x013CC70C); its
    /// **generator is PROVISIONAL**: no `GenerateNaval*` registration exists (CONFIRMED, §4.3), so
    /// this reuses `GenerateRecruitmentPanel` (`0x009FE7B0`), the one generator that carries a
    /// `naval` category and a per-card `is_naval`. What switches the generator between land and
    /// naval is the manager bool at `+0xA4` (0x009C7C70), whose writer is UNKNOWN; PROVISIONAL: the
    /// script global that sets it is the tab itself. UNKNOWN: `CampaignShipCard`, a template no
    /// shipped script references -- whether the naval cards are drawn with it stays open (§4.5 item 6).
    NavalRecruitment,
    Agents,
    Construction,
    /// A settlement's infrastructure tab: its **road** slot (CONFIRMED, own Ghidra copy 2026-10-07).
    /// `ConstructSettlementPanelTabs` `0x0099A200` adds it (text id 0x50, `0x0098C260`) when the
    /// settlement's road slot (`+0x1C0`) exists; it is a construction panel tab (the same tab vtable
    /// `0x0136B2F4` as the construction tab, so its generator is `GenerateConstructionPanel`, called
    /// by `0x009C7CF0`) whose info builder `BuildSettlementInfrastructureInfoTable` `0x00A021B0`
    /// lists the road slot alone and sets `infrastructure` = true. Key CONFIRMED (`infrastructure_tab`).
    Infrastructure,
    /// A map fort's own panel (`CampaignSelection::Fort`): `construction_manager.
    /// GenerateFortConstructionPanel`, registered at `0x009C7B50`, info builder
    /// `BuildFortPanelInfoTable` `0x009FDFE0` (CONFIRMED). Key CONFIRMED (own Ghidra copy,
    /// 2026-10-07): the fort's tab set `ConstructFortPanelTabs` `0x009988C0` makes this tab first
    /// with `ConstructFortConstructionTab` `0x00989BB0`, text / key id 0x52 -- `construction_tab`,
    /// the same key as a settlement's construction tab (the two never share one selection) -- with
    /// the tab vtable `0x0136B350` (state handler `0x009C7B50`, info builder `0x009FDFE0`).
    Fort,
}

impl Tab {
    /// The tab's key: both its component id and its `random_localisation_strings` key
    /// (`FUN_00f555c0(i)` / `FUN_00a0cb30(i)`, enum 0x4B..0x52; strings CONFIRMED in .rdata, the
    /// order army, navy, recruitment, naval_recruitment, agents, infrastructure, siege,
    /// construction INFERRED from their order there and every use).
    fn key(self) -> &'static str {
        match self {
            Tab::Army => "army_tab",
            Tab::Navy => "navy_tab",
            Tab::Recruitment => "recruitment_tab",
            Tab::NavalRecruitment => "naval_recruitment_tab",
            Tab::Agents => "agents_tab",
            Tab::Construction | Tab::Fort => "construction_tab",
            Tab::Infrastructure => "infrastructure_tab",
        }
    }
    /// The root-layout global that fills the panel (CONFIRMED names in the tab objects' vtables).
    fn generator(self) -> &'static str {
        match self {
            Tab::Army => "GenerateArmyPanel",
            Tab::Navy => "GenerateNavyPanel",
            // PROVISIONAL for `NavalRecruitment`: no naval generator is registered (CONFIRMED, §4.3).
            Tab::Recruitment | Tab::NavalRecruitment => "GenerateRecruitmentPanel",
            Tab::Agents => "GenerateAgentsPanel",
            Tab::Construction | Tab::Infrastructure => "GenerateConstructionPanel",
            Tab::Fort => "GenerateFortConstructionPanel",
        }
    }
}

/// Review-panel tab states passed to the root's `ReviewPanelTabInit` / `CreateReviewPanelTabAtPosition`:
/// 1 = not selected, 2 = selected (CONFIRMED: `0x00A20620` changes the old tab to 1 and the new one
/// to 2, and the tab's state change `0x009C7D80` passes that number to `ReviewPanelTabInit`; the
/// construction tab's `0x009C7CF0` generates its panel only on 2).
const TAB_UNSELECTED: i32 = 1;
const TAB_SELECTED: i32 = 2;

/// Colour → palette index of a palette picture (see `CampaignUi::palette_index`).
type PaletteIndex = Rc<HashMap<[u8; 3], i32>>;
/// A theatre's bounds in map units: (min x, min y), (max x, max y).
type TheatreBounds = ((f32, f32), (f32, f32));
/// The radar view mapping UpdateRadarView gives: map size, theatre offset, theatre size.
type RadarMapping = ((f32, f32), (f32, f32), (f32, f32));

/// The pending deal of the diplomacy panel: the host's stand-in for the exe's
/// `UIDiplomacyNegotiation` userdata (ctor 0x00A102B0, 0xB4 bytes). Its +0xAC holds the
/// counterparty faction id, so a negotiation with no `target` answers every accessor with
/// nothing, as the original does.
#[derive(Debug, Default)]
struct NegotiationState {
    /// The faction proposing the deal (the exe's "Proposer").
    proposer: Option<String>,
    /// The counterparty: the exe's +0xAC field.
    target: Option<String>,
    /// Whether a negotiation is in progress.
    active: bool,
    /// The "Offers" rows.
    offers: Vec<NegotiationItem>,
    /// The "Demands" rows.
    demands: Vec<NegotiationItem>,
    /// The stance-declaration actions still on offer (see `BuildPossibleActions`).
    possible_actions: Vec<String>,
}

impl NegotiationState {
    /// Start a negotiation between two factions, as the panel's open does.
    fn open(&mut self, proposer: String, target: String) {
        self.proposer = Some(proposer);
        self.target = Some(target);
        self.active = true;
        self.clear();
    }

    /// Drop the deal and the action list, keeping the two factions.
    fn clear(&mut self) {
        self.offers.clear();
        self.demands.clear();
        self.possible_actions.clear();
    }
}

/// One item of the pending deal. The variants are the model's `DiplomaticAction`s plus the region,
/// payment and technology items (see `apply_deal` for which applier each has).
///
/// Nothing constructs an item yet, and that is the traced state of the original, not a gap in the
/// host: no CONFIRMED method takes the deal in. `BuildOfferAndDemandStrings` (0x009B48B0) reads no
/// Lua argument and only *reports* the rows, `ProposeDeal` (0x009BFCF0, 142 bytes) is the deal's
/// validation/error path, and `Propose` (0x009BF3C0, 2285 bytes, 40 callees) does the work on the
/// engine's own object. In the exe the rows are mutated by the engine when the panel's subpopup
/// OK buttons run, so the host waits for those callback bodies (UI_FIDELITY.md §4 open item 3)
/// before it fills the list.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
enum NegotiationItem {
    /// Region keys to hand over (the exe's region-transfer action, id 6 in
    /// `BuildOfferAndDemandStrings`). The exe's applier is `0x00B449F0` (CONFIRMED address); the
    /// model has no command for it yet, so the row is dropped (DEFERRED, see `deal_item_commands`).
    Regions(Vec<String>),
    /// Technology keys to hand over. Still **UNKNOWN**: no granting address is reachable from the
    /// deal path, so the row is dropped (see `apply_deal`).
    Technologies(Vec<String>),
    /// A payment: an amount and how many turns it runs for. `turns == 0` is the lump sum the commit
    /// path runs as **`0x00BB3810(amount, 3)`** (CONFIRMED, the same money mover as the capture loot
    /// `0x00BB3810(money, 0)`); a positive `turns` is the per-turn schedule, one of the container's
    /// two optional single items (`+0x20` / `+0x24`, UNKNOWN which) -- INFERRED that it is the
    /// payment, since that is what a schedule on the deal would be for. The model has both: a state
    /// gift (a lump sum, `0x00B44590`) and a regular payment (per turn, `treaties::DiplomaticAction`).
    Payment {
        /// The amount moved.
        amount: i32,
        /// How many turns it runs for; 0 for the lump sum.
        turns: u32,
    },
    /// A diplomatic action with a model command (`DiplomaticAction`).
    Action(ntw_sim::campaign::treaties::DiplomaticAction),
}

/// The HUD's campaign state.
pub(super) struct CampaignUi {
    link: CampaignLink,
    /// The script host's `Lua`. Only needed to build an address the first time one is asked for (see
    /// [`CampaignUi::entity`]); a clone, so every other method can stay `&self`.
    lua: Lua,
    /// Interned addresses, by `(tag, id)` -- see [`CampaignUi::entity`]. Strongly held on purpose: an
    /// address that could be collected and rebuilt would make `==` answer false for the same entity.
    addresses: RefCell<HashMap<(usize, i32), Table>>,
    /// One metatable per address kind, holding that kind's `__tostring`.
    address_meta: RefCell<HashMap<usize, Table>>,
    requests: RefCell<Vec<CampaignRequest>>,
    selection: Cell<CampaignSelection>,
    tabs: RefCell<Vec<Tab>>,
    current_tab: Cell<usize>,
    /// A selection change is building the tab list: the exe holds no tab set then (manager
    /// +0xEEC is 0 from the old set's release to the new set's store), see [`UiScriptHost::campaign_select`].
    building_tabs: Cell<bool>,
    /// The refused tab request during a tab-list build was logged (logged once per HUD).
    refused_tab_request_logged: Cell<bool>,
    /// The unit details of each unit key asked for so far (see [`unit_details`]).
    unit_details: RefCell<HashMap<String, Rc<UnitDetails>>>,
    /// `start_pos_settlements_onscreen_name_<settlement key>-<n>` → settlement name.
    settlement_names: HashMap<String, String>,
    /// Slot addresses handed to the scripts (`TAG_SLOT`): the payload is the slot's id here,
    /// interned per (region, slot) so the same slot is the same address. No packing of region and
    /// slot into one integer, so neither has a size limit.
    slot_ids: RefCell<Interner<(RegionId, SlotRef)>>,
    /// `building_culture_variants`, see [`building_variants`].
    variants: std::collections::BTreeMap<(String, String), (String, String)>,
    /// Camera position the game reported (see `campaign_set_view`).
    camera: Cell<(f32, f32, f32)>,
    /// Settlements on screen: (region, screen x, screen y).
    visible: RefCell<Vec<(RegionId, f32, f32)>>,
    /// The settlement under the pointer.
    over: Cell<Option<RegionId>>,
    started: std::time::Instant,
    /// See `CampaignUi::map_folder`.
    map_folder: std::cell::OnceCell<String>,
    /// `campaign_map_playable_areas`, see `CampaignUi::playable_area`.
    playable_areas: std::cell::OnceCell<Vec<ntw_data::CampaignMapPlayableArea>>,
    /// See `CampaignUi::palette_index`.
    palettes: RefCell<HashMap<String, PaletteIndex>>,
    /// See `CampaignUi::theatre_bounds`.
    theatre_bounds: std::cell::OnceCell<Option<TheatreBounds>>,
    /// The camera target the game reported: map x, zoom, map y (see `CameraTarget`).
    camera_target: Cell<(f32, f32, f32)>,
    /// See `CampaignUi::slot_types`.
    slot_types: std::cell::OnceCell<HashMap<String, ntw_data::SlotTypeRecord>>,
    /// AttachRadarView's component and UpdateRadarView's mapping.
    radar_view: RefCell<(Option<NodeId>, Option<RadarMapping>)>,
    /// See `CampaignUi::attitude_levels`.
    attitude_levels: std::cell::OnceCell<HashMap<String, i32>>,
    /// See `CampaignUi::religion_icon`.
    religion_icons: std::cell::OnceCell<HashMap<String, String>>,
    /// The capture whose screen is open (see `campaign_capture_screen`).
    capture_shown: Cell<Option<RegionId>>,
    /// Negotiation state for the diplomacy panel.
    negotiation: RefCell<NegotiationState>,
}

// ---------------------------------------------------------------------------------------------
// Entity addresses. The original hands scripts `UTILITYDLL::LUA::Pointer<T>` **userdata** for
// characters, regions, units, military forces, region slots, forts and theatres; scripts hand them
// straight back to `CampaignUI.*`, compare them with `==`, and `tostring` one to ask its type.
//
// CONFIRMED (UI_FIDELITY.md 9, read-only Ghidra on the exe):
//   * the shared metatable for the whole family is exactly `type`, `__tostring` and `__eq`
//     (`0x0105AE10`, 53 registering call sites), and there is **no** `__index`;
//   * `__eq` (`0x01058F20`) compares the wrapped pointers, so `==` is **identity**, not a value
//     comparison;
//   * `__tostring` (`0x01058F60`) is `sprintf("%s (0x0%x)", metatable.type, pointer)`;
//   * `metatable.type` is the registration's own name, which for this family is the full
//     `State::operator <<<T>(const Lua::Pointer<T> &)` signature string interned at run time.
//
// Light userdata cannot carry `__tostring`, so ours is a **table** with the light userdata payload in
// an `Address` field and a per-tag metatable -- interned per `(tag, id)` so `==` stays identity. That
// is the whole reason for the table: a fresh table per call would make every `==` between two
// addresses answer false.

const TAG_CHARACTER: usize = 1 << 40;
const TAG_REGION: usize = 2 << 40;
const TAG_UNIT: usize = 3 << 40;
const TAG_FORCE: usize = 4 << 40;
const TAG_THEATRE: usize = 6 << 40;
const TAG_SLOT: usize = 7 << 40;
const TAG_FORT: usize = 8 << 40;
/// `FactionDetails`' `Address` field (`FUN_009E2CB0`'s table, CONFIRMED field name; 16 shipped call
/// sites read it back). 0-E round 5: this used to be `TAG_FORCE | (1 << 39)` -- a tag outside
/// `TAG_MASK`, so it never collided with a real force address but had no entry in
/// [`address_type_name`] and therefore stringified as the `void` stand-in. Its own kind, with its own
/// signature string, read from the exe's constant pool in round 5 exactly like the others.
const TAG_FACTION: usize = 9 << 40;
const TAG_MASK: usize = 0xFF << 40;

/// The `metatable.type` the original's registration for this kind of address carries: the C++
/// signature string the exe interns for its `Lua::Pointer<T>` binding, copied verbatim from the exe's
/// own constant pool (UI_FIDELITY.md 9.5/9.6). CONFIRMED to be the value `__tostring` prints, and
/// CONFIRMED to be what makes `string.find(tostring(target), "CHARACTER")` match
/// (`agent_options.lua:34`) -- that substring test is the only use any shipped script makes of it.
/// INFERRED, immaterial: which of the `const` / non-`const` variant a given value came out of.
fn address_type_name(tag: usize) -> &'static str {
    match tag {
        TAG_CHARACTER => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                          <<<const class EMPIRECAMPAIGN::CHARACTER>(const class \
                          UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::CHARACTER const > &)",
        TAG_REGION => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                       <<<const class EMPIRECAMPAIGN::REGION>(const class \
                       UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::REGION const > &)",
        TAG_UNIT => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                     <<<const class EMPIRECAMPAIGN::UNIT>(const class \
                     UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::UNIT const > &)",
        TAG_FORCE => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                      <<<const class EMPIRECAMPAIGN::MILITARY_FORCE>(const class \
                      UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::MILITARY_FORCE const > &)",
        TAG_THEATRE => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                        <<<const class EMPIRECAMPAIGN::CAMPAIGN_THEATRE>(const class \
                        UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::CAMPAIGN_THEATRE const > &)",
        TAG_SLOT => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                     <<<const class EMPIRECAMPAIGN::REGION_SLOT>(const class \
                     UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::REGION_SLOT const > &)",
        TAG_FORT => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                     <<<const class EMPIRECAMPAIGN::FORT>(const class \
                     UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::FORT const > &)",
        TAG_FACTION => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator \
                        <<<const class EMPIRECAMPAIGN::FACTION>(const class \
                        UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::FACTION const > &)",
        // `TAG_QUEUE_ITEM` lands here: the recruitment card hands `item_ptr` straight back to
        // `CampaignUI.CancelRecruitment` (`template.RecruitmentCard.lua:99` pc 22-30) and nothing
        // else reads it, so no shipped script can stringify it and no signature is guessed. See
        // `CancelRecruitment` above, which decodes it by tag.
        // Unreachable for a shipped tag; a wrong tag must still stringify as something.
        _ => "class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator <<<void>(const void &)",
    }
}

/// The light userdata payload an address carries: our stand-in for the original's C++ pointer, a
/// kind tag above bit 40 (far from UI component addresses, which are small numbers) and the id.
fn entity_payload(tag: usize, id: i32) -> Value {
    Value::LightUserData(LightUserData((tag | id as u32 as usize) as *mut std::ffi::c_void))
}

fn entity_of(v: &Value, tag: usize) -> Option<i32> {
    match v {
        Value::LightUserData(p) if (p.0 as usize) & TAG_MASK == tag => Some((p.0 as usize & 0xFFFF_FFFF) as u32 as i32),
        Value::Table(t) => t.raw_get::<Value>("Address").ok().as_ref().and_then(|a| entity_of(a, tag)),
        _ => None,
    }
}

/// `CampaignUI.CharactersRelationshipToPlayersFaction`'s answer. The values are CONFIRMED from
/// `template.CampaignCharacterCard.luac`: its locals `0, 1, 2, 3` with `RTPF_OWNED = 0` (the card
/// drops its faction badge) and `displayed_rtpf_states = {"neutral", "ally", "foe"}` indexed by the
/// answer. Which stance gives which answer is INFERRED from those state names.
fn relationship_to_players_faction(own: bool, stance: ntw_sim::campaign::Stance) -> i32 {
    use ntw_sim::campaign::Stance;
    if own {
        return 0;
    }
    match stance {
        Stance::Neutral => 1,
        Stance::Allied | Stance::Protectorate | Stance::Patron => 2,
        Stance::War => 3,
    }
}

/// The string a Lua argument holds, if it is one. Used where the .luac argument list is not
/// decompiled and the host takes whichever argument names what it needs (see
/// `CanDemolishBuilding`).
fn value_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => s.to_str().ok().map(|b| String::from(&*b)),
        _ => None,
    }
}

/// The slot a construction argument list names: the **last** string argument that resolves to a
/// slot of some region. The building actions are called with different argument lists in the
/// shipped scripts -- `Construction.lua:366` gives `DemolishBuilding(building_key, slot_key)` and
/// `building_information.luac:36` gives `DemolishBuilding(slot_key)` (both CONFIRMED) -- and a
/// building key never names a slot, so taking the last slot key is order- and count-independent.
fn slot_of_args(ui: &CampaignUi, args: &Variadic<Value>) -> Option<(RegionId, SlotRef)> {
    args.iter().filter_map(value_str).filter_map(|k| slot_by_key(ui, &k)).next_back()
}

fn character_value(ui: &CampaignUi, id: CharacterId) -> Value {
    ui.entity(TAG_CHARACTER, id.0)
}
fn region_value(ui: &CampaignUi, id: RegionId) -> Value {
    ui.entity(TAG_REGION, id.0 as i32)
}
fn unit_value(ui: &CampaignUi, id: UnitId) -> Value {
    ui.entity(TAG_UNIT, id.0)
}
fn force_value(ui: &CampaignUi, id: ForceId) -> Value {
    ui.entity(TAG_FORCE, id.0 as i32)
}
/// The address a script sees for a fort ([`CampaignSelection::Fort`]). PROVISIONAL: the original's
/// is its `campaign_fort` pointer (`0x00965550`); ours carries the region id, as the selection does.
fn fort_value(ui: &CampaignUi, id: RegionId) -> Value {
    ui.entity(TAG_FORT, id.0 as i32)
}
/// The address `FactionDetails` hands out (`Address`). CONFIRMED kind: the exe's constant pool
/// carries the `EMPIRECAMPAIGN::FACTION` signature string alongside the other seven
/// (`0x01371770`, 0-E round 5), and a faction is what the 16 shipped `FactionDetails` call sites
/// already hold -- so it gets its own tag rather than borrowing the force one.
fn faction_value(ui: &CampaignUi, id: FactionId) -> Value {
    ui.entity(TAG_FACTION, id.0)
}

impl CampaignUi {
    /// The address a script sees for `(tag, id)`: an interned table whose `Address` field is the
    /// light userdata payload [`entity_payload`] builds, with a metatable carrying this kind's
    /// `__tostring` (UI_FIDELITY.md 9).
    ///
    /// **Interning is load-bearing, not an optimisation.** The original's `__eq` compares the wrapped
    /// pointers, so `==` between two addresses is identity; Lua tables compare by reference, so
    /// handing out a fresh table per call would make every `==` between two addresses answer false.
    /// The first call for a `(tag, id)` builds the table and keeps it, so the same entity is the same
    /// table for the rest of the campaign -- which is also what a recycled id should do.
    ///
    /// A failure here means `Lua` refused to make a table, which for a fresh `Lua` it does not; it
    /// falls back to the bare payload so a script still gets a usable (if un-stringifiable) address
    /// rather than nothing.
    fn entity(&self, tag: usize, id: i32) -> Value {
        if let Some(t) = self.addresses.borrow().get(&(tag, id)) {
            return Value::Table(t.clone());
        }
        let payload = entity_payload(tag, id);
        let Ok(address) = self.lua.create_table() else { return payload };
        if address.raw_set("Address", payload.clone()).is_err() {
            return payload;
        }
        // One metatable per kind, so a campaign does not build a closure per address. The lookup's `Ref`
        // must be dropped before the insert below, so clone the answer out of it first.
        let known = self.address_meta.borrow().get(&tag).cloned();
        let meta = match known {
            Some(m) => m,
            None => {
                let name = address_type_name(tag).to_owned();
                let Ok(m) = self.lua.create_table() else { return payload };
                let Ok(tostring) = self.lua.create_function(move |lua, (this,): (Table,)| {
                    // The original reads its metatable's `type` and the wrapped pointer; ours reads
                    // the same two things, so `tostring` matches byte for byte given the same value.
                    let raw = this.raw_get::<Value>("Address")?;
                    let pointer = match raw {
                        Value::LightUserData(p) => p.0 as usize,
                        _ => 0,
                    };
                    lua.create_string(format!("{name} (0x0{pointer:x})"))
                }) else { return payload };
                if m.set("__tostring", tostring).is_err() {
                    return payload;
                }
                self.address_meta.borrow_mut().insert(tag, m.clone());
                m
            }
        };
        if address.set_metatable(Some(meta)).is_err() {
            return payload;
        }
        self.addresses.borrow_mut().insert((tag, id), address.clone());
        Value::Table(address)
    }

    /// Drops the interned addresses of queue items no region holds any more (finished or
    /// cancelled), so [`CampaignUi::addresses`] does not grow with every item ever shown. A queued
    /// item keeps its table (identity). If an id comes back later (a reloaded save can hand out an
    /// id again), it names a different item, so a fresh table is right for it; a script that kept
    /// the old item's address sees it unequal to the new one.
    fn forget_finished_queue_items(&self) {
        let m = self.model();
        let live: std::collections::HashSet<i32> =
            m.world.regions.values().flat_map(|r| r.recruitment_queue.iter().map(|i| i.id.raw())).collect();
        self.addresses.borrow_mut().retain(|&(tag, id), _| tag != TAG_QUEUE_ITEM || live.contains(&id));
    }

    fn model(&self) -> std::cell::Ref<'_, CampaignModel> {
        std::cell::Ref::map(self.link.state.borrow(), |s| &s.model)
    }

    fn push(&self, r: CampaignRequest) {
        self.requests.borrow_mut().push(r);
    }
}

fn loc(inner: &Inner, key: &str) -> Option<String> {
    inner.loc.get(key).map(str::to_owned)
}

/// A faction's on-screen name: loc `factions_screen_name_<key>`, else the DB name (as the front
/// end's `CampaignDetails`).
fn faction_name(inner: &Inner, db: &GameDatabase, key: &str) -> String {
    loc(inner, &format!("factions_screen_name_{key}"))
        .or_else(|| db.faction(key).map(|r| r.screen_name.clone()))
        .unwrap_or_else(|| key.to_owned())
}

/// A region's on-screen name: loc `regions_onscreen_<key>` (CONFIRMED keys, e.g.
/// `regions_onscreen_eur_france` = "France").
pub fn region_name(inner_loc: &ntw_formats::loc::Localisation, key: &str) -> String {
    inner_loc.get(&format!("regions_onscreen_{key}")).map(str::to_owned).unwrap_or_else(|| key.to_owned())
}

/// Settlement names from the loc table `start_pos_settlements_onscreen_name_<settlement key>-<n>`
/// (CONFIRMED keys, e.g. `..._settlement:eur_france:paris-1491848185` = "Paris"; the numeric
/// suffix differs per start position and is ignored, INFERRED).
pub fn settlement_names(loc: &ntw_formats::loc::Localisation) -> HashMap<String, String> {
    const PREFIX: &str = "start_pos_settlements_onscreen_name_";
    let mut out = HashMap::new();
    for (k, t) in loc.iter() {
        if let Some(rest) = k.strip_prefix(PREFIX) {
            let key = match rest.rfind('-') {
                Some(i) if rest[i + 1..].bytes().all(|b| b.is_ascii_digit()) => &rest[..i],
                _ => rest,
            };
            out.entry(key.to_owned()).or_insert_with(|| t.to_owned());
        }
    }
    out
}

impl CampaignUi {
    fn settlement_name(&self, inner: &Inner, region: RegionId) -> String {
        let m = self.model();
        let Some(r) = m.world.regions.get(&region) else { return String::new() };
        self.settlement_names.get(&r.settlement.key).cloned().unwrap_or_else(|| region_name(&inner.loc, &r.key))
    }

    /// The text the selection bar shows: `"<a>, <b>"` (CONFIRMED: the engine joins two names with
    /// ", " (`DAT_01315ff0`), and `SetSelectedEntity` cuts at the comma when the text does not fit
    /// one line). For a settlement: settlement, region (INFERRED from the region getter used);
    /// for a character: PROVISIONAL "<agent type>, <faction>" until character names are read
    /// from the save (the model does not keep them yet). A fort: PROVISIONAL, its settlement and
    /// region like the settlement's (the model's only fort is the settlement's fortification, which
    /// has no name of its own; the original's `campaign_fort` naming is UNKNOWN).
    fn selection_name(&self, inner: &Inner, sel: CampaignSelection) -> String {
        match sel {
            CampaignSelection::None => String::new(),
            CampaignSelection::Settlement(r) | CampaignSelection::Fort(r) => {
                let region_key = self.model().world.regions.get(&r).map(|r| r.key.clone()).unwrap_or_default();
                format!("{}, {}", self.settlement_name(inner, r), region_name(&inner.loc, &region_key))
            }
            CampaignSelection::Character(c) => {
                let faction = {
                    let m = self.model();
                    let faction = m.world.characters.get(&c).and_then(|ch| m.world.factions.get(&ch.faction));
                    faction.map_or_else(String::new, |f| faction_name(inner, &self.link.db, &f.key))
                };
                if faction.is_empty() {
                    return String::new();
                }
                // The character's own name when the model has it, else his agent type's name.
                let name = character_name(inner, self, c).unwrap_or_else(|| character_type_name(inner, self, c));
                format!("{name}, {faction}")
            }
        }
    }
}

/// A character's name: its `CHARACTER_DETAILS` forename and surname, which are loc keys
/// (`names_name_names_frenchNapoléon` = "Napoléon", CONFIRMED), joined by a space; `None` if the
/// model has no names for it (CAMPAIGN_DATA.md §3).
fn character_name(inner: &Inner, ui: &CampaignUi, c: CharacterId) -> Option<String> {
    let m = ui.model();
    let d = m.world.character_details.get(&c)?;
    let part = |k: &str| if k.is_empty() { None } else { Some(loc(inner, k).unwrap_or_else(|| k.to_owned())) };
    let name = [part(&d.forename), part(&d.surname)].into_iter().flatten().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some(name)
}

/// A character's portrait card picture (PORTRAIT_DETAILS #0, e.g.
/// `ui/portraits/european/Cards/...`), `None` when the model has none for him.
fn portrait_card(m: &CampaignModel, c: CharacterId) -> Option<String> {
    m.world.character_details.get(&c).map(|d| d.portrait.card.clone()).filter(|p| !p.is_empty())
}

/// An agent type's on-screen name in a culture: loc
/// `agent_culture_details_onscreen_name_<agent><culture>`, the agent type's `agents` key followed
/// by the culture's `cultures` key with nothing between them.
///
/// **CONFIRMED from the shipped data:** the install's localisation has exactly 53
/// `agent_culture_details_onscreen_name_` keys and `agent_culture_details` has exactly 53 rows,
/// and each row `<agent>, <culture>` matches the key `<agent><culture>` exactly (both spellings
/// as the DB has them, so `General` and `Eastern_Scholar` keep their capitals). Test
/// `every_agent_culture_row_has_its_onscreen_name_key` (`ntw_script`, install).
///
/// The name is culture-specific in the data -- a rake is a "Spy" in `european`/`egy_european` and
/// a "Scout" in `tribal`, an assassin a "Hashishin" in `middle_east` and a "Thugee" in `indian`
/// (all read from the install) -- so the culture must be the character's own
/// ([`character_type_name`]), not a fixed one.
///
/// Falls back to the ESF type string when the pair has no key (a faction whose culture the model
/// does not know, or a type outside `agent_culture_details` such as `bandit` in `egy_european`).
/// INFERRED as a stand-in: the original's answer for such a pair is UNKNOWN.
fn agent_type_name(inner: &Inner, kind: CharacterKind, culture: &str) -> String {
    let key = kind.esf_name();
    loc(inner, &format!("agent_culture_details_onscreen_name_{key}{culture}")).unwrap_or_else(|| key.to_owned())
}

/// [`agent_type_name`] for a character: his agent type in **his faction's** culture. The culture of
/// a character is his faction's (`CharacterCultureType`, 0x0089C240 CONFIRMED:
/// `factions` #2 subculture → `cultures_subcultures` culture), and the per-culture agent record is
/// the one the exe reads through the agent record (`0x008E27D0`, CHARACTERS_FIDELITY.md §8).
fn character_type_name(inner: &Inner, ui: &CampaignUi, c: CharacterId) -> String {
    let m = ui.model();
    let (kind, culture) = match m.world.characters.get(&c) {
        Some(ch) => (ch.kind, m.world.factions.get(&ch.faction).map_or_else(String::new, |f| m.rules.characters.culture(&f.key).to_owned())),
        None => return String::new(),
    };
    drop(m);
    agent_type_name(inner, kind, &culture)
}

// ---------------------------------------------------------------------------------------------
// Detail tables.

/// `FactionDetails(key)` → {Key, Address, Name, FlagPath, UniformColour, PrimaryColour,
/// WealthRanking, PowerRanking, PrestigeRanking, Leader, VictoryConditions}: CONFIRMED field names
/// (`FUN_009e2cb0` builds the table, `FUN_009af1a0` the faction fields). PROVISIONAL: rankings 0,
/// colours as {r, g, b} (sub-table keys UNKNOWN), Leader / VictoryConditions empty.
fn faction_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(f) = m.faction_by_key(key) else { return Ok(Value::Nil) };
    let t = lua.create_table()?;
    t.set("Key", key)?;
    t.set("Address", faction_value(ui, f.id))?;
    drop(m);
    t.set("Name", faction_name(inner, &ui.link.db, key))?;
    let rec = ui.link.db.faction(key);
    t.set("FlagPath", rec.map(|r| r.flag_path.clone()))?;
    let colour = |c: [u8; 3]| -> mlua::Result<Table> {
        let ct = lua.create_table()?;
        ct.set("r", c[0])?;
        ct.set("g", c[1])?;
        ct.set("b", c[2])?;
        Ok(ct)
    };
    if let Some(r) = rec {
        t.set("PrimaryColour", colour(r.primary_colour())?)?;
        t.set("UniformColour", colour(r.secondary_colour())?)?;
    }
    t.set("WealthRanking", 0)?;
    t.set("PowerRanking", 0)?;
    t.set("PrestigeRanking", 0)?;
    // Leader: the holder of the faction_leader post (CAMPAIGN_DATA.md §3), as a character details
    // table (0x009AD250, see `character_details`); absent when the faction has no leader (CONFIRMED
    // test 0x008CF600). Portrait is ours (the card picture path), kept for older callers.
    let lid = {
        let m = ui.model();
        m.faction_by_key(key).and_then(|f| m.world.faction_leader(f.id))
    };
    if let Some(c) = lid
        && let Value::Table(leader) = character_details(lua, inner, ui, c)?
    {
        let portrait = portrait_card(&ui.model(), c).unwrap_or_default();
        leader.set("Portrait", portrait)?;
        t.set("Leader", leader)?;
    }
    // VictoryConditions {ConquestDescription, PrestigeDescription, DeadlineDescription,
    // TotalRegionsNeeded, Regions = {[i] = {Address, Name}}} (0x009ACFB0, CONFIRMED keys, from the
    // faction's CAMPAIGN_VICTORY_CONDITIONS). PROVISIONAL: the conditions are not in the model yet
    // (ntw_campaign::victory reads them from the save only), so the descriptions are empty and no
    // region is listed.
    let vc = lua.create_table()?;
    vc.set("ConquestDescription", "")?;
    vc.set("PrestigeDescription", "")?;
    vc.set("DeadlineDescription", "")?;
    vc.set("TotalRegionsNeeded", 0)?;
    vc.set("Regions", lua.create_table()?)?;
    t.set("VictoryConditions", vc)?;
    Ok(Value::Table(t))
}

/// One unit card entry, the fields `template.CampaignUnitCard.lua` reads (CONFIRMED names from
/// its bytecode): Id, Address, Key, Name, Description, Icon, Men, Max, MenAsPercent, Experience,
/// IsNaval, Guns, CommanderType, CharacterPtr, CommandersName, DisplayAsUnit, InTransit,
/// UnitRecord, ... Values from the model and the `units` table; PROVISIONAL where noted.
///
/// `display_as_unit` is the card info's byte `+0x130` the card builder `0x009ABE00` hands out as
/// `DisplayAsUnit` (own Ghidra copy, 2026-10-07; CONFIRMED writers): the army panel's
/// `BuildArmyPanelInfoTable` `0x009FCFB0` clears it (its cards are false), a character's
/// `CommandedUnit` in `BuildCharacterDetailsInfoTable` `0x009AD250` sets it (true, at
/// `0x009ADF80`).
fn unit_entry(lua: &Lua, inner: &Inner, ui: &CampaignUi, force: &MilitaryForce, index: usize, mut display_as_unit: bool) -> mlua::Result<Table> {
    let u = &force.units[index];
    let t = lua.create_table()?;
    // Id: a unique component id for the card (INFERRED: ExistingUnitCard matches cards by Id).
    t.set("Id", format!("unit_card_{}", u.id.0))?;
    t.set("Address", unit_value(ui, u.id))?;
    t.set("Key", u.unit_key.as_str())?;
    // UnitRecord: the unit's details table, not its key. The card keeps it and reads its `UnitLimit`
    // and `Key` on mouse-on, then hands it to the unit tooltip's `Initialise` as the unit record
    // (template.CampaignUnitCard.lua:122 / 406-413, CONFIRMED from the bytecode) -- the table a
    // recruitment card's `unit_record` is ([`unit_details`], built once per unit key; INFERRED that
    // both are the same).
    t.set("UnitRecord", unit_details(lua, inner, ui, &u.unit_key)?)?;
    let rec = ui.link.db.unit(&u.unit_key);
    let name = loc(inner, &format!("units_on_screen_name_{}", u.unit_key)).unwrap_or_else(|| rec.map(|r| r.dev_name.clone()).unwrap_or_default());
    t.set("Name", name)?;
    t.set("Description", loc(inner, &format!("unit_description_texts_description_text_{}", u.unit_key)).unwrap_or_default())?;
    // Unit card picture: `ui/units/icons/<faction>_<unit>_icon.tga` (CONFIRMED file names, e.g.
    // `france_inf_line_french_fusiliers_icon.tga`; INFERRED: the owning faction's key, else the
    // bare info key as some generic cards are named). The card script appends ".tga" (CONFIRMED).
    let faction = ui.model().world.factions.get(&force.faction).map(|f| f.key.clone()).unwrap_or_default();
    let info = rec.map(|r| r.info_key.clone()).unwrap_or_else(|| u.unit_key.clone());
    let icon = [format!("{faction}_{}_icon", u.unit_key), format!("{faction}_{info}_icon"), info.clone()]
        .into_iter()
        .find(|p| inner.source.find(&format!("ui/units/icons/{p}.tga")).is_some())
        .unwrap_or(info);
    t.set("Icon", format!("data/ui/units/icons/{icon}"))?;
    t.set("Men", u.men)?;
    t.set("Max", u.max_men)?;
    let pct = if u.max_men > 0 { (u.men * 100) as f32 / u.max_men as f32 } else { 0.0 };
    t.set("MenAsPercent", pct)?;
    t.set("EstimatedMenAsUnary", pct / 100.0)?;
    t.set("ReplenishmentLevel", 0)?;
    t.set("Replenished", false)?;
    t.set("SufferingAttrition", false)?;
    // PROVISIONAL: the unit's chevrons (`unit+0xD48`, printed as "Experience" by `0x005CD340`) are
    // not in the model: the ESF index for a loaded chevron count is UNKNOWN.
    t.set("Experience", 0)?;
    t.set("IsNaval", force.is_navy)?;
    t.set("Guns", 0)?;
    t.set("InTransit", false)?;
    // PromotionCost: what the field promotion of this unit's commander charges
    // ([`CampaignModel::promotion_cost`], PROVISIONAL value; CONFIRMED field and place -- the unit
    // rows carry it and `army.lua:825`'s `SelectedUnitsPromotionCost` sums it over the selected
    // units). Not a General or admiral's own unit: the promotion is of the unit's commander.
    let promo = {
        let m = ui.model();
        m.promotion_cost(force.id, index)
    };
    t.set("PromotionCost", promo.unwrap_or(0))?;
    // What the player may know about this unit (`utilities.lua`'s `knowledge_mask` and
    // `spying_data_level`, CONFIRMED values: mask bits 1 icon / 2 men / 4 guns / 8 experience, the
    // owned mask 15; levels -1 invalid / 0 passive / 1 basic / 2 advanced / 3 owned). The level comes
    // from the model's sight and knowledge (INFERRED mapping, `spying_level_character`); the
    // experience is a PROVISIONAL 0 anyway.
    let level = {
        let m = ui.model();
        match m.faction_by_key(&ui.link.human).map(|h| h.id) {
            Some(human) if force.faction != human => spying_level_unit(&m, human, u.id),
            Some(_) => LEVEL_OWNED,
            None => LEVEL_INVALID,
        }
    };
    t.set("knowledge_mask", knowledge_mask(level))?;
    t.set("spying_data_level", level)?;
    // CommanderType: Utilities.lua's CT_* values (CONFIRMED): 0 primary general, 1 secondary
    // general, 2 primary admiral, 3 secondary admiral, 4 commodore, 5 brigadier, 6 naval unit,
    // 7 land unit. INFERRED: the first unit of a force with a commander is his own card (the
    // startpos armies lead with the general's bodyguard).
    // Every card carries a Portrait (`0x009AA5E0` writes it always, empty unless set below; an
    // admiral's card in character format calls `string.len` on it).
    t.set("Portrait", "")?;
    let mut ct = if force.is_navy { 6 } else { 7 };
    if index == 0
        && let Some(c) = force.commander
    {
        let kind = ui.model().world.characters.get(&c).map(|ch| ch.kind);
        // Portrait: the card snapshot's `0x008C9EF0` -- "data/" + the unit's character's portrait
        // path when his agent type is 0, the General (`0x00F9C6C0`: agent record `+0x2C` == 0),
        // else empty (CONFIRMED, own Ghidra copy 2026-10-07). The card shows it instead of the
        // unit picture when DisplayAsUnit is false and CommanderType is a general or admiral
        // (`template.CampaignUnitCard.lua:66-99`), so an admiral's card keeps its picture. The
        // path is the character's PORTRAIT_DETAILS card picture (INFERRED: the exe reads the
        // character's `+0x370`; it is the path the character card's CardImage uses too).
        if kind == Some(CharacterKind::General) {
            match portrait_card(&ui.model(), c) {
                Some(card) => t.set("Portrait", format!("data/{card}"))?,
                // PLACEHOLDER: a general with no portrait in the model (the recruitment pool's
                // hires and promoted generals get none yet, BACKLOG §0 "portraits of generated
                // characters") keeps his unit card instead of a portrait-less character card.
                None => display_as_unit = true,
            }
        }
        t.set("CharacterPtr", character_value(ui, c))?;
        ct = match kind {
            Some(CharacterKind::Admiral) => 2,
            Some(CharacterKind::Captain) => 4,
            Some(CharacterKind::Colonel) => 5,
            _ => 0,
        };
        let name = character_name(inner, ui, c).or_else(|| Some(character_type_name(inner, ui, c)));
        t.set("CommandersName", name.unwrap_or_default())?;
        // Attributes = {PrimaryAttributePath, PrimaryLevel, PrimaryAttributeName} (CONFIRMED names:
        // exe strings 0x0136EAE8.., read by the card), the commander's main attribute
        // ([`attributes_table`]).
        t.set("Attributes", attributes_table(lua, inner, ui, Some(c), Pips::Without)?)?;
    }
    t.set("CommanderType", ct)?;
    t.set("DisplayAsUnit", display_as_unit)?;
    Ok(t)
}

/// `ui/army.luac`'s fort building states (its main chunk, lines 8-10, CONFIRMED by disassembly):
/// `FBS_ABLE` = 0 (`AbleToBuildFort` is `g_fort_building_status == FBS_ABLE`, line 817) and
/// `FBS_UNABLE` = 1 (what `GenerateNavyPanel` sets, line 256).
const FBS_UNABLE: i32 = 1;

/// The army panel's `build_fort_cost`: the exe's `0x004613B0`, which always returns -1 (CONFIRMED).
const FORT_COST_NONE: i32 = -1;

/// The info table `GenerateArmyPanel` / `GenerateNavyPanel` receive: {controlable, commander,
/// military_force, can_build_fort_status, build_fort_cost, units_info = {Units|Ships = {...}}}
/// (CONFIRMED names: Army.lua reads them, strings at 0x0136D630..0x0136D6D0). `can_build_fort_status`
/// and `build_fort_cost` are the army panel's `g_button_fort` state / tooltip (the luac calls
/// `AbleToBuildFort` and `g_button_fort:SetState/SetTooltipText/SetVisible`, CONFIRMED call sites).
///
/// INFERRED (static only, own Ghidra copy, 2026-10-07; not yet seen in the original): the engine's
/// writer is the army panel info builder `0x009FCFB0` (an orphan until now, so the two key strings
/// showed no code reference; it refers to them by immediate push). It writes
/// `can_build_fort_status` = `0x0047AF90()`, a folded `return 1` (= [`FBS_UNABLE`]), and
/// `build_fort_cost` = `0x004613B0()`, a folded `return -1` -- constants, as `BuildFort` never
/// succeeds (see there). So the button is always greyed with its plain inactive tooltip
/// (`ShowArmyButtons`, `Army.lua:1127-1131`: `0 < g_fort_cost` is false). Stays INFERRED until a
/// side-by-side check shows the original's army-panel fort button greyed too.
fn force_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, force: ForceId) -> mlua::Result<Value> {
    let f = match ui.model().world.forces.get(&force) {
        Some(f) => f.clone(),
        None => return Ok(Value::Nil),
    };
    let human = ui.model().faction_by_key(&ui.link.human).map(|h| h.id);
    let t = lua.create_table()?;
    t.set("controlable", Some(f.faction) == human)?;
    t.set("commander", f.commander.map(|c| character_value(ui, c)))?;
    t.set("military_force", force_value(ui, f.id))?;
    t.set("can_build_fort_status", FBS_UNABLE)?;
    t.set("build_fort_cost", FORT_COST_NONE)?;
    let units = lua.create_table()?;
    for i in 0..f.units.len() {
        units.set(i + 1, unit_entry(lua, inner, ui, &f, i, false)?)?;
    }
    let info = lua.create_table()?;
    info.set(if f.is_navy { "Ships" } else { "Units" }, units)?;
    t.set("units_info", info)?;
    Ok(Value::Table(t))
}

// ---------------------------------------------------------------------------------------------
const TAG_QUEUE_ITEM: usize = 5 << 40;

/// The unit card picture's path without ".tga": `ui/units/icons/<faction>_<unit>_icon`
/// (CONFIRMED file names), else the bare info key (INFERRED fallback).
fn unit_icon(inner: &Inner, db: &GameDatabase, faction: &str, unit_key: &str) -> String {
    let info = db.unit(unit_key).map(|r| r.info_key.clone()).unwrap_or_else(|| unit_key.to_owned());
    let icon = [format!("{faction}_{unit_key}_icon"), format!("{faction}_{info}_icon"), info.clone()]
        .into_iter()
        .find(|p| inner.source.find(&format!("ui/units/icons/{p}.tga")).is_some())
        .unwrap_or(info);
    format!("data/ui/units/icons/{icon}")
}

/// Is `key` a ship? CONFIRMED structure: a naval unit has no `unit_stats_land` row (its stats are in
/// `unit_stats_naval`), and the `units` #2 category is `naval_*` for the war-ships. Both are used so
/// that a vessel whose category is not `naval_*` (the trade ships) still counts. INFERRED: the exe's
/// per-card `is_naval` = `[card+0xA0] != 0` (0x009FE7B0) is one flag on the card; which field it is
/// built from was not decoded, and no script-facing name for it exists.
fn is_ship(db: &GameDatabase, key: &str) -> bool {
    db.unit(key).is_some_and(|u| u.category.starts_with("naval")) || (db.unit(key).is_some() && db.unit_stats(key).is_none())
}

/// The card state a unit category picks (`artillery`, `infantry`, `cavalry`, `naval`: the
/// RecruitmentCard states, CONFIRMED; the mapping from `units` #2 INFERRED). A ship is `naval`
/// whatever its category says (the trade vessels' category is not `naval_*`, and they are ships).
fn card_category(db: &GameDatabase, unit_key: &str) -> &'static str {
    if is_ship(db, unit_key) {
        return "naval";
    }
    match db.unit(unit_key).map(|u| u.category.as_str()).unwrap_or("") {
        "cavalry" => "cavalry",
        "artillery" => "artillery",
        _ => "infantry",
    }
}

/// `GenerateRecruitmentPanel(info)`: {recruitable_units, enqueued_units, faction_colour,
/// uniform_colour, recruitment_capacity, player_owned} (CONFIRMED names: the exe strings the
/// generator itself pushes, `recruitable_units` 0x0136D734, `recruitable_unit` 0x0136D748,
/// the card-id markers at 0x0136D7A0, `recruitment_capacity` 0x0136D87C and `Available`
/// 0x0136D71C, plus Recruitment.lua). Entries carry the fields Recruitment.lua and
/// template.RecruitmentCard.lua read: item_ptr, manager, record, character, unit_record, status
/// ("Available" / "Unavailable" / "Enqueued"), reasons_unavailable (bit i = the i-th reason of the
/// card's list: no slot, unaffordable, population, damaged, occupied, siege, limit, technology,
/// path), name, description, image_path, cost, upkeep, turns, card_id, category, class, experience,
/// slot, faction_key. card_id = "<unit>!recruitable!<n>" / "<unit>!enqueued!<n>" (the card script
/// cuts the id at "!"; the exe strings "!recruitable!" and "!enqueued!" CONFIRMED, the index suffix
/// INFERRED). PROVISIONAL: the entries' experience 0 (a unit being raised is a fresh recruit);
/// reasons only "no slot" and "unaffordable"; the queue's turns are the remaining turns.
///
/// Naval (0-E round N+1). The generator is `0x009FE7B0` (14,199 bytes), CONFIRMED as the owner of
/// every string named above. There is no `GenerateNaval` symbol in the exe: the only generator
/// names it registers are `GenerateRecruitmentPanel` (0x009C7C70), `GenerateFortConstructionPanel`
/// (0x009C7B50), `GenerateConstructionPanel` (0x009C7CF0, 0x009C77A0), `GenerateAgentsPanel`
/// (0x009C7AE7), `GenerateArmyPanel` and `GenerateNavyPanel` (0x009C7BE0), so the naval recruitment
/// tab reuses this one generator (CONFIRMED). Inside it a switch on `[obj+0x8]` (0..13, jump table
/// 0x00A01F28) picks a category name, and one case pushes `naval` (0x0130CFFC, at 0x00A01217); every
/// card gets `is_naval` = (`[card+0xA0] != 0`) (0x013305C0, at 0x009FF497, 0x00A004E3, 0x00A00F21).
/// The generator is registered with a bool at `+0xA4` beside its pointer at `+0xA0` (0x009C7C70),
/// the likely land/naval selector (INFERRED), and the panel's `naval_recruitment_tab` component is
/// CONFIRMED (exe string 0x013CC71C, beside `recruitment_tab` 0x013CC70C). The naval half is WIRED
/// (0-E round N+2): the infrastructure/naval tabs below pass `naval = true`, which asks
/// `recruitment_points(region, true)` (the modelled `naval_recruitment_points` per port, `0x00B61EE0`
/// CONFIRMED) and keeps only the cards whose unit category is `naval_*` (`units` #2, the source of the
/// generator's own per-card `is_naval` = `[card+0xA0] != 0`, CONFIRMED). PROVISIONAL: that `naval`
/// argument stands in for the manager bool at `+0xA4`, whose writer is UNKNOWN, and the tab is placed
/// where 0-G's trace of the tab builder `FUN_0099A200` puts it -- see [`Tab::NavalRecruitment`].
/// UNKNOWN: `CampaignShipCard`, a template no shipped script references, so we cannot say whether
/// the naval cards use it.
/// A recruitment item's `unit_record`: the unit's details table, which template.RecruitmentCard.lua
/// keeps and hands to the unit card tooltip (template.unitcard_tooltip.lua's Initialise reads Name,
/// Class, Description, IsNaval, IsArtillery, Men, Guns, Range, Accuracy, Melee, Charge, Defence,
/// Morale; naval: Firepower, Speed, Manoeuvrability, HullStrength, Seamen, Gunners; the card reads
/// Key and UnitLimit). CONFIRMED field names. Values: Class = the class's on-screen name, Range =
/// the projectile's effective range, Melee / Charge / Defence / Morale / Accuracy =
/// `unit_stats_land` melee attack / charge bonus / melee defence / morale / accuracy (INFERRED
/// columns); naval stats are not given (PROVISIONAL).
///
/// `UnitLimit` is always an integer: the exe's card-record builder `0x009AA5E0` sets it with its
/// integer setter (`0x0044DE40`) from the card snapshot (+0x94), never leaving it out (CONFIRMED),
/// and every shipped reader tests `0 < UnitLimit` before the limited-unit text
/// (template.CampaignUnitCard.lua:406, template.recruitmentcard.lua:259, CONFIRMED from the
/// bytecode), so 0 is the "no limit" path. Where the exe's value comes from is UNKNOWN; ours is 0
/// (PROVISIONAL: no unit limit in the model yet).
///
/// The values are worked out once per unit key and kept on the campaign HUD (they come from the
/// database and the localisation, both fixed for the HUD's lifetime, so nothing goes stale); each
/// call hands out a fresh table, so a script that writes to its card's record changes no other
/// card's.
///
/// No borrow of the cache is held while the table is built (building it can run Lua).
fn unit_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Table> {
    let cached = ui.unit_details.borrow().get(key).cloned();
    let d = match cached {
        Some(d) => d,
        None => {
            let d = Rc::new(UnitDetails::new(inner, &ui.link.db, key));
            ui.unit_details.borrow_mut().insert(key.to_owned(), d.clone());
            d
        }
    };
    d.to_table(lua)
}

/// The values of one unit's [`unit_details`] table.
struct UnitDetails {
    key: String,
    name: String,
    description: String,
    class: String,
    naval: bool,
    artillery: bool,
    /// From `unit_stats_land`, when the unit has a row there.
    stats: Option<UnitDetailStats>,
}

/// The `unit_stats_land` part of [`UnitDetails`].
struct UnitDetailStats {
    men: i64,
    /// Only for a unit with guns.
    guns: Option<i64>,
    range: i64,
    accuracy: i64,
    melee: i64,
    charge: i64,
    defence: i64,
    morale: i64,
}

impl UnitDetails {
    fn new(inner: &Inner, db: &GameDatabase, key: &str) -> Self {
        let u = db.unit(key);
        let class = u.map(|u| u.unit_class.clone()).unwrap_or_default();
        let s = db.unit_stats(key);
        UnitDetails {
            key: key.to_owned(),
            name: loc(inner, &format!("units_on_screen_name_{key}")).unwrap_or_else(|| key.to_owned()),
            description: loc(inner, &format!("unit_description_texts_description_text_{key}")).unwrap_or_default(),
            class: loc(inner, &format!("unit_class_onscreen_{class}")).unwrap_or(class),
            naval: u.is_some_and(|u| u.category.starts_with("naval")),
            artillery: s.is_some_and(|s| s.is_artillery),
            stats: s.map(|s| UnitDetailStats {
                men: i64::from(s.num_men),
                guns: (s.num_guns > 0).then(|| i64::from(s.num_guns)),
                // Range is always a number, also for a melee unit: the exe's army unit-card builder
                // `0x009AA5E0` sets it on every card with its float setter (`0x0044DEF0`, from the
                // card snapshot +0x5C, no branch; CONFIRMED for that card; INFERRED for the
                // recruitment card's record, whose builder is not traced), and the unit tooltip
                // compares it with 0 for artillery (template.unitcard_tooltip.lua:93). The value is
                // the card snapshot's (`0x008DF190`): the gun type's longest range, else the unit's
                // own projectile's, else 0 (`GameDatabase::unit_card_range`, CONFIRMED).
                range: i64::from(db.unit_card_range(s)),
                accuracy: i64::from(s.accuracy),
                melee: i64::from(s.melee_attack),
                charge: i64::from(s.charge_bonus),
                defence: i64::from(s.melee_defence),
                morale: i64::from(s.morale),
            }),
        }
    }

    /// A fresh table with these values.
    fn to_table(&self, lua: &Lua) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("Key", self.key.as_str())?;
        t.set("Name", self.name.as_str())?;
        t.set("Description", self.description.as_str())?;
        t.set("Class", self.class.as_str())?;
        t.set("IsNaval", self.naval)?;
        t.set("IsArtillery", self.artillery)?;
        t.set("UnitLimit", 0)?;
        if let Some(s) = &self.stats {
            t.set("Men", s.men)?;
            if let Some(g) = s.guns {
                t.set("Guns", g)?;
            }
            t.set("Range", s.range)?;
            t.set("Accuracy", s.accuracy)?;
            t.set("Melee", s.melee)?;
            t.set("Charge", s.charge)?;
            t.set("Defence", s.defence)?;
            t.set("Morale", s.morale)?;
        }
        Ok(t)
    }
}

/// `GenerateRecruitmentPanel(info)` for the recruitment tab. `naval` asks for the naval half of the
/// same generator (the `naval_recruitment_tab`, see [`Tab::NavalRecruitment`]): the region's naval
/// recruitment capacity instead of the land one, and only the ships among the region's recruitable
/// units. See the module note above for what is CONFIRMED and what is PROVISIONAL here.
fn recruitment_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId, naval: bool) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let treasury = m.world.factions.get(&r.owner).map_or(0, |f| f.treasury);
    let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let capacity = m.recruitment_points(region, naval);
    let used = r.recruitment_queue.len() as u32;
    let recruitable = m.recruitable_units(region);
    let is_naval = |k: &str| is_ship(&ui.link.db, k);
    // The naval tab shows the ships only (`units` #2 category, CONFIRMED source of the generator's
    // own `is_naval`), both among the recruitable units and in the queue.
    let shown = |k: &String| !naval || is_naval(k);
    let recruitable: Vec<&String> = recruitable.iter().filter(|k| shown(k)).collect();
    let queue: Vec<(RecruitmentItemId, String, u32, i32)> =
        r.recruitment_queue.iter().filter(|q| shown(&q.unit_key)).map(|q| (q.id, q.unit_key.clone(), q.turns_remaining, q.cost)).collect();
    // The price the exe's card shows is the **experience-adjusted** recruitment cost (`0x0045CB50`
    // -> `0x00ED49A0`, the value the queue item records), not the bare `units` #4 cost: go through
    // the same `economy::recruit_cost` the treasury is charged by, so the card can never drift from
    // the charge. A unit being raised afresh has no chevrons, so the rank is 0 and this is the
    // plain cost - the tables land in `rules.xp_cost` through `ntw_campaign::rules_from_db`.
    // `upkeep` stays the unit type's own `UpkeepCost` (`card+0x3C`), which is what the panel shows.
    let rules: Vec<(i32, i32, u32)> = recruitable
        .iter()
        .map(|k| {
            m.rules.units.get(*k).map_or((0, 0, 1), |u| {
                (economy::recruit_cost(&m.rules, u, 0), u.upkeep, u.turns)
            })
        })
        .collect();
    let player_owned = Some(r.owner) == human;
    drop(m);
    ui.forget_finished_queue_items();
    let db = &ui.link.db;
    let entry = |key: &str, status: &str, cost: i32, upkeep: i32, turns: u32, card_id: String, slot: Option<usize>| -> mlua::Result<Table> {
        let e = lua.create_table()?;
        e.set("manager", region_value(ui, region))?;
        e.set("record", key)?;
        e.set("unit_record", unit_details(lua, inner, ui, key)?)?;
        e.set("is_naval", is_naval(key))?;
        e.set("status", status)?;
        e.set("name", loc(inner, &format!("units_on_screen_name_{key}")).unwrap_or_else(|| key.to_owned()))?;
        e.set("description", loc(inner, &format!("unit_description_texts_description_text_{key}")).unwrap_or_default())?;
        e.set("image_path", unit_icon(inner, db, &owner_key, key))?;
        e.set("cost", cost)?;
        e.set("upkeep", upkeep)?;
        e.set("turns", turns)?;
        e.set("turns_to_completion", turns)?;
        e.set("card_id", card_id)?;
        e.set("category", card_category(db, key))?;
        e.set("class", db.unit(key).map(|u| u.unit_class.clone()).unwrap_or_default())?;
        e.set("experience", 0)?;
        e.set("faction_key", owner_key.as_str())?;
        e.set("affordable", cost <= treasury)?;
        if let Some(s) = slot {
            e.set("slot", s)?;
        }
        Ok(e)
    };
    let units = lua.create_table()?;
    for (i, (key, (cost, upkeep, turns))) in recruitable.iter().zip(rules).enumerate() {
        let mut reasons = 0;
        if used >= capacity {
            reasons |= 1;
        }
        if cost > treasury {
            reasons |= 2;
        }
        let e = entry(key.as_str(), if reasons == 0 { "Available" } else { "Unavailable" }, cost, upkeep, turns, format!("{key}!recruitable!{i}"), None)?;
        e.set("reasons_unavailable", reasons)?;
        units.set(i + 1, e)?;
    }
    let enqueued = lua.create_table()?;
    for (i, (item, key, turns, cost)) in queue.iter().enumerate() {
        let e = entry(key, "Enqueued", *cost, 0, *turns, format!("{key}!enqueued!{i}"), Some(i))?;
        // The item's own id is the payload, not its queue position: an `item_ptr` a script kept
        // names the same item after other items were cancelled. Each item's address is interned in
        // `CampaignUi::addresses` the first time it is shown and kept while the item is queued
        // (`CampaignUi::forget_finished_queue_items`).
        e.set("item_ptr", ui.entity(TAG_QUEUE_ITEM, item.raw()))?;
        e.set("reasons_unavailable", 0)?;
        enqueued.set(i + 1, e)?;
    }
    recruitment_table(lua, ui, units, enqueued, &owner_key, capacity, player_owned)
}

/// The `GenerateRecruitmentPanel` info table around its two card lists: the faction's colours,
/// the capacity and whether the player owns the manager.
fn recruitment_table(lua: &Lua, ui: &CampaignUi, units: Table, enqueued: Table, faction_key: &str, capacity: u32, player_owned: bool) -> mlua::Result<Value> {
    let t = lua.create_table()?;
    t.set("recruitable_units", units)?;
    t.set("enqueued_units", enqueued)?;
    if let Some(rec) = ui.link.db.faction(faction_key) {
        let c = |c: [u8; 3]| -> mlua::Result<Table> {
            let ct = lua.create_table()?;
            ct.set("r", c[0])?;
            ct.set("g", c[1])?;
            ct.set("b", c[2])?;
            Ok(ct)
        };
        t.set("faction_colour", c(rec.primary_colour())?)?;
        t.set("uniform_colour", c(rec.secondary_colour())?)?;
    }
    t.set("recruitment_capacity", capacity)?;
    t.set("player_owned", player_owned)?;
    Ok(Value::Table(t))
}

/// An army's recruitment panel with nothing to recruit (PLACEHOLDER, see `generate_current_tab`):
/// the army's faction's colours, no cards, capacity 0.
fn empty_recruitment_info(lua: &Lua, ui: &CampaignUi, commander: CharacterId) -> mlua::Result<Value> {
    let (faction_key, player_owned) = {
        let m = ui.model();
        let faction = m.world.characters.get(&commander).and_then(|c| m.world.factions.get(&c.faction));
        (faction.map(|f| f.key.clone()).unwrap_or_default(), faction.is_some_and(|f| f.key == ui.link.human))
    };
    recruitment_table(lua, ui, lua.create_table()?, lua.create_table()?, &faction_key, 0, player_owned)
}

/// `building_culture_variants` rows: (building level, culture) → (icon, description key)
/// (schema s,s,o,o,o,o,o reads all 264 rows to the end; columns 0 level, 1 culture, 6 icon
/// (`ui/buildings/icons/<icon>.tga`, CONFIRMED file names; column 3 repeats it for settlement
/// buildings only and is the fallback), 5 description key (loc
/// `building_description_texts_*_description_<key>`, CONFIRMED keys); the others UNKNOWN).
pub fn building_variants(source: &crate::ScriptSource) -> std::collections::BTreeMap<(String, String), (String, String)> {
    use ntw_formats::db::{DbTable, Schema};
    let mut out = std::collections::BTreeMap::new();
    let Some(file) = source.find("db/building_culture_variants_tables/building_culture_variants") else { return out };
    let Some(schema) = Schema::from_codes("s,s,o,o,o,o,o") else { return out };
    let Ok(t) = DbTable::read(&file.bytes, &schema) else { return out };
    for r in &t.rows {
        let s = |i: usize| r.get(i).and_then(|v| v.as_str()).unwrap_or("").to_owned();
        let icon = if s(6).is_empty() { s(3) } else { s(6) };
        out.insert((s(0), s(1)), (icon, s(5)));
    }
    out
}

/// One building slot for the construction panel: a settlement slot, the settlement's
/// fortification slot or its road slot.
struct SlotInfo {
    key: String,
    /// The standing building: (level, health).
    standing: Option<(String, u32)>,
    /// The slot's construction item: (level, turns left, total turns, is a repair).
    building: Option<(String, u32, u32, bool)>,
    /// Construction options (`0x00B43300`).
    options: Vec<ConstructionOption>,
    can_repair: bool,
    repair_cost: i32,
}

/// The panel entry of `slot`, keyed `key` (what `BeginConstruction` & co. get back), or `None`
/// when the slot has nothing to show. CONFIRMED (`0x00B7A0E0`, which `0x00A01F50` and
/// `0x00A021B0` ask for every slot, the walls and the road included): a slot is listed when a
/// building stands in it or is being built there, else only when its option list is not empty.
/// (The shipped `Construction.lua:106` indexes the entry's first building, so an entry with no
/// card would be a script error.)
fn slot_info(m: &CampaignModel, region: RegionId, slot: SlotRef, key: String) -> Option<SlotInfo> {
    let r = m.world.regions.get(&region)?;
    let (_, standing) = r.construction_slot(slot)?;
    let standing = standing.map(|b| (b.level_key.clone(), b.health));
    let building = r.construction.iter().find(|c| c.slot == slot).map(|c| {
        // A repair is an item of the standing level; its length is the repair's own
        // ([`CampaignModel::repair_turns`]), not the level's build time.
        let repair = standing.as_ref().is_some_and(|(l, _)| *l == c.level_key);
        let full = if repair { m.repair_turns(region, slot) } else { m.rules.buildings.get(&c.level_key).map_or(c.turns_remaining, |b| b.turns.max(1)) };
        (c.level_key.clone(), c.turns_remaining, full.max(c.turns_remaining), repair)
    });
    // The model repairs slots and the walls, not the road ([`CampaignModel::can_repair`]). The
    // original offers no road repair either: the road is only on the infrastructure panel, whose
    // `infrastructure` flag hides the repair / demolish buttons (`Construction.lua:408`, `:429`),
    // and a damaged road gets no upgrade cards there (`0x009FBAB0`: upgrades at full health only).
    let options = m.construction_options_in(region, slot, None);
    if standing.is_none() && building.is_none() && options.is_empty() {
        return None;
    }
    // `repair_cost` is the slot's repair cost whatever the repair state (CONFIRMED: the row builder
    // `0x009C8190` writes `0x00B66410(slot)` unconditionally, beside `being_repaired` `0x00B4DA20`
    // and `can_repair` `0x00B1A6B0`; `0x00B66410` gives 0 only for a slot with no building), so a
    // building under repair, or the road, still shows what repairing its damage costs.
    let can_repair = m.can_repair(region, slot);
    let repair_cost = m.repair_cost(region, slot);
    Some(SlotInfo { key, standing, building, options, can_repair, repair_cost })
}

/// The `slot_key` prefix of a region's fortification (walls) slot. PROVISIONAL format: the exe's
/// fortification slot is a `REGION_SLOT` with a key of its own, which the model does not keep.
const WALLS_KEY_PREFIX: &str = "fortification:";
/// The `slot_key` prefix of a region's road slot (PROVISIONAL format, as the walls').
const ROAD_KEY_PREFIX: &str = "road:";

/// The `slot_key` of `slot` in region `r` (`None` when the index names no slot): a settlement
/// or map slot's own `REGION_SLOT` key, or the walls' / road's key.
fn slot_key_of(r: &ntw_sim::campaign::Region, slot: SlotRef) -> Option<String> {
    match slot {
        SlotRef::Walls => Some(fortification_slot_key(&r.key)),
        SlotRef::Slot(i) => r.slots.get(i).map(|s| s.key.clone()),
        SlotRef::Road => Some(road_slot_key(&r.key)),
    }
}

/// The `slot_key` of `region_key`'s fortification (walls) slot.
fn fortification_slot_key(region_key: &str) -> String {
    format!("{WALLS_KEY_PREFIX}{region_key}")
}

/// The `slot_key` of `region_key`'s road slot.
fn road_slot_key(region_key: &str) -> String {
    format!("{ROAD_KEY_PREFIX}{region_key}")
}

/// Which construction panel a settlement tab shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstructionPanel {
    /// The construction tab: `BuildSettlementConstructionInfoTable` `0x00A01F50`.
    Settlement,
    /// The infrastructure tab: `BuildSettlementInfrastructureInfoTable` `0x00A021B0`.
    Infrastructure,
}

/// `GenerateConstructionPanel(info)` for a settlement's construction or infrastructure tab
/// (CONFIRMED names: Construction.lua's ResetConstructionPanel / GenerateConstructionPanel).
///
/// **The exe (own Ghidra copy, 2026-10-07; the walls placement CONFIRMED at runtime and in game).**
/// - Construction tab, `BuildSettlementConstructionInfoTable` `0x00A01F50`: `slots` = one entry per
///   slot of the settlement's slot list, then **the settlement's fortification slot (`+0x1C4`) as the
///   last entry**; `faction_key`; `controlable`. No `infrastructure`, no `fort_ptr`. Runtime: a
///   breakpoint on `0x00A01F50` hit when the user selected London (call chain `0x00A0F14D`
///   `HandleCampaignMapMouseEvent` -> `0x009C38A1` `HandleSettlementSelected` -> `0x0099A492`
///   `ConstructSettlementPanelTabs` -> `0x009DA2EB` `OpenFirstEnabledPanelTab` -> `0x009C7D4C`
///   `ChangeConstructionTabStateGeneratePanel`), and the user saw "Small Star Fort"
///   (`sFortifications1_settlement_fortifications`) as the last card of London's construction panel.
/// - Infrastructure tab, `BuildSettlementInfrastructureInfoTable` `0x00A021B0`: `slots` = the road
///   slot (`+0x1C0`) alone, `faction_key`, `controlable`, and `infrastructure` = true -- which the
///   script turns into `g_infrastructure` (`Construction.lua:75`, `info.infrastructure ~= nil`) and
///   which hides the repair / demolish buttons (`Construction.lua:408`, `:429`).
/// - Each slot entry, `BuildConstructionSlotEntryTable` `0x009FBAB0`: {buildings = {entry...},
///   upgrades = {entry...}}: `buildings` holds the slot's construction item (type 2) or else its
///   standing building (type 1), then, for an empty slot, every construction option (type 3;
///   Construction.lua's CreateBuildingFoundation makes one card per entry); `upgrades` (type 4) is
///   filled when the standing building is at full health. An entry carries the fields
///   CreateBuildingFrame reads: type (Utilities.lua BUILDING_ICON_TYPE_*: 0 empty, 1 built,
///   2 constructing, 3 constructable, 4 upgrade, 5 alternative chain; CONFIRMED values), name,
///   short/long_description, building_key, region_key, slot_key, health, being_repaired,
///   can_repair, can_afford_repair, repair_cost (`0x009C8190`), dismantling, alternate_chain,
///   availability (bit 1 affordable, bit 2 technology present: Construction.lua's locals,
///   CONFIRMED), image, cost, turns_to_completion, percent_complete, technologies.
///
/// The walls card is an ordinary slot card, so the frame's own calls build it:
/// `BeginConstruction` / `BeginUpgrade(building_key, slot_key)`, `CancelConstruction`,
/// `RepairBuilding`, `DemolishBuilding` with the walls' slot key ([`fortification_slot_key`],
/// resolved by [`slot_by_key`] to `SlotRef::Walls`). INFERRED: the panel lists the settlement's
/// own slots (`settlement:` keys). Every slot passes the exe's filter `0x00B7A0E0` ([`slot_info`]:
/// a building, a construction item or a non-empty option list). The walls and the road slot objects
/// exist in every `REGION_SLOT_MANAGER` of all eight shipped startpos files (CONFIRMED,
/// `esf_find` on the install: 72/72 eur, 30/30 egy, 25/25 ita, 31/31 spa, 8/8 tut, mp alike); the
/// model keeps no empty slot, so a mod region without one still gets them (PROVISIONAL).
/// PROVISIONAL: technologies empty, no alternative chains or dismantling.
fn construction_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId, panel: ConstructionPanel) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let treasury = m.world.factions.get(&r.owner).map_or(0, |f| f.treasury);
    let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let mut slots = Vec::new();
    match panel {
        ConstructionPanel::Settlement => {
            for (i, s) in r.slots.iter().enumerate().filter(|(_, s)| s.key.starts_with("settlement:")) {
                slots.extend(slot_info(&m, region, SlotRef::Slot(i), s.key.clone()));
            }
            // The walls: the last entry (CONFIRMED, see above); every region has the slot.
            slots.extend(slot_info(&m, region, SlotRef::Walls, fortification_slot_key(&r.key)));
        }
        ConstructionPanel::Infrastructure => {
            slots.extend(slot_info(&m, region, SlotRef::Road, road_slot_key(&r.key)));
        }
    }
    let region_key = r.key.clone();
    let controlable = Some(r.owner) == human;
    drop(m);
    // An entry: level, type, its slot, cost, turns, percent complete, availability.
    let entry = |level: &str, kind: i32, s: &SlotInfo, cost: i32, turns: u32, percent: f32, availability: i32| -> mlua::Result<Table> {
        let e = lua.create_table()?;
        e.set("type", kind)?;
        e.set("region_key", region_key.as_str())?;
        e.set("slot_key", s.key.as_str())?;
        let (name, short, long) = building_texts_for(inner, ui, &owner_key, level);
        e.set("building_key", level)?;
        e.set("name", name)?;
        e.set("short_description", short)?;
        e.set("long_description", long)?;
        e.set("image", ui.building_icon(&owner_key, level))?;
        // The repair fields describe the slot's building (0x009C8190 reads them from the slot).
        let on_slot = kind == 1 || kind == 2;
        e.set("health", if on_slot { s.standing.as_ref().map_or(100, |(_, h)| *h) } else { 100 })?;
        e.set("being_repaired", on_slot && s.building.as_ref().is_some_and(|b| b.3))?;
        e.set("can_repair", on_slot && s.can_repair)?;
        e.set("can_afford_repair", on_slot && ntw_sim::campaign::treasury::can_pay_repair(treasury, s.repair_cost))?;
        e.set("repair_cost", if on_slot { s.repair_cost } else { 0 })?;
        e.set("dismantling", false)?;
        e.set("alternate_chain", false)?;
        e.set("availability", availability)?;
        e.set("cost", cost)?;
        e.set("turns_to_completion", turns)?;
        e.set("percent_complete", percent)?;
        e.set("technologies", lua.create_table()?)?;
        Ok(e)
    };
    let availability = |o: &ConstructionOption| i32::from(o.affordable) | (i32::from(o.tech) << 1);
    let list = lua.create_table()?;
    for (n, s) in slots.iter().enumerate() {
        let st = lua.create_table()?;
        let buildings = lua.create_table()?;
        match (&s.building, &s.standing) {
            (Some((level, left, total, _)), _) => {
                let done = total.saturating_sub(*left) as f32 / (*total).max(1) as f32 * 100.0;
                buildings.raw_push(entry(level, 2, s, 0, *left, done, 3)?)?;
            }
            (None, Some((level, _))) => buildings.raw_push(entry(level, 1, s, 0, 0, 100.0, 3)?)?,
            (None, None) => {
                for o in &s.options {
                    buildings.raw_push(entry(&o.level_key, 3, s, o.cost, o.turns, 0.0, availability(o))?)?;
                }
            }
        }
        st.set("buildings", buildings)?;
        let upgrades = lua.create_table()?;
        if s.building.is_none() && s.standing.as_ref().is_some_and(|(_, h)| *h > 99) {
            for o in &s.options {
                upgrades.raw_push(entry(&o.level_key, 4, s, o.cost, o.turns, 0.0, availability(o))?)?;
            }
        }
        st.set("upgrades", upgrades)?;
        list.set(n + 1, st)?;
    }
    let t = lua.create_table()?;
    t.set("slots", list)?;
    t.set("controlable", controlable)?;
    t.set("faction_key", owner_key)?;
    if panel == ConstructionPanel::Infrastructure {
        t.set("infrastructure", true)?;
    }
    Ok(Value::Table(t))
}

/// `construction_manager.GenerateFortConstructionPanel(info)` -- a **map fort's** panel (`fFort`,
/// [`CampaignSelection::Fort`]; not the settlement walls, which are [`construction_info`]'s last slot).
/// Two rounds of evidence, and the second is stronger:
///
/// **The exe (own Ghidra copy, §4.4).** The generator is registered at **`0x009C7B50`** (guarded on
/// the manager's `+0xAC`, generator pointer `+0xB4`, bool `+0xB8`) and its info builder is
/// **`0x009FDFE0`** (282 bytes), which logs "Fort tables" and pushes `forts` (the table `0x009FC010`
/// builds), `fort_ptr` = the panel's fort object (`+0x70`; ours the constant 1, PROVISIONAL) and
/// `controlable` (the caller's bool). `0x009FC010` fills
/// every row through **`0x009C9170`**, the construction panel's own row builder (its only caller).
/// `can_build_fort_status` / `build_fort_cost` belong to the army panel's info (`0x009FCFB0`, see
/// [`force_info`]), not to this one, so they are absent here.
///
/// **The panel script (`ui/construction.luac`, CONFIRMED by disassembly -- `Construction.lua`, the
/// function `GenerateFortConstructionPanel` at line 879).** It is a one-slot panel and it says so:
/// - `ResetConstructionPanel(info)` (line 61) reads **`fort_ptr`**, **`controlable`**,
///   **`faction_key`**, `infrastructure` and, if present, `slots`; `g_num_slots` defaults to **1**,
///   which is exactly the fortification's one slot. So `faction_key` and `infrastructure` are needed
///   as well, and `slots` must stay absent.
/// - it loops over `info.forts` and handles only three row types: `BUILDING_ICON_TYPE_BUILT` (1),
///   `BUILDING_ICON_TYPE_CONSTRUCTING` (2) and `BUILDING_ICON_TYPE_UPGRADE` (4). **There is no
///   branch for `BUILDING_ICON_TYPE_CONSTRUCTABLE` (3)**: a constructable row is skipped, leaving
///   `g_building_slot_components` empty, and the two calls it ends with --
///   `SelectPassiveConstructionSlotExclusive(1)` / `SelectExplicitConstructionSlotExclusive(1)` --
///   then trip `assert(g_building_slot_components[1])` at `Construction.lua:624`. So the list must
///   always carry one frame row (1 or 2); an empty fortification slot is shown as a `BUILT` frame
///   with no building key, named `Construction_site` (the module's own string constant).
/// - each frame row it reads: `type`, `name`, `image`, `description`, `long_description`,
///   `building_key`, `slot_key`, `health`, `percent_complete`, `being_repaired`, **`repairable`**,
///   `can_repair`, `can_afford_repair`, `repair_cost`, `turns_to_completion`; each upgrade row also
///   reads **`cost`**, **`affordable`** and **`tech_present`** (as separate keys, not the
///   construction panel's packed `availability` bits).
/// - the frame it creates is a **`BuildingFrame`** template in the construction panel's
///   `ConstructionCardGroup`, and its own script (`template.buildingframe.luac:417`, upvalue
///   `g_fort_ptr`) calls `UpgradeFort(g_fort_ptr)`, `CancelFortRepair(g_fort_ptr)` and
///   `CancelUpgradeFort(g_fort_ptr)` when `fort_ptr` is set, and `BeginConstruction` /
///   `BeginUpgrade` / `CancelConstruction` / `RepairBuilding(building_key, slot_key)` otherwise.
///
/// INFERRED, called out because it is the only guess left here: `description` (the row builder's key
/// that [`construction_info`] has no source for -- ours keeps `short_description`, and here it
/// repeats it, which is what the key name suggests). PROVISIONAL: the script compares
/// `being_repaired` and `dismantling` with the **string** `"true"`, so our booleans do not match and
/// a repair click falls through to the CONSTRUCTING branch -- whether the engine's `SetGlobal`
/// stringifies is UNKNOWN, and the settlement panel's rows have always been booleans here.
fn fort_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId) -> mlua::Result<Value> {
    let state = ui.link.state.borrow();
    let m = &state.model;
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let region_key = r.key.clone();
    // `BuildFortPanelRowsTable` `0x009FC010`: the standing level's row, then -- not while building --
    // the next level ([`CampaignModel::map_fort_next_level`], `FindNextFortUpgradeLevel` `0x00B430C0`)
    // as one upgrade row whose `affordable` is `0x0047BA10()`, constant false (CONFIRMED). The fort's
    // own level is PROVISIONAL ([`CampaignModel::map_fort_standing`]); a map fort has no construction or damage in
    // the model, so the standing row is BUILT at full health with no repair.
    let levels = m.map_fort_levels();
    let standing = m.map_fort_standing(region, &levels);
    let next = standing
        .as_ref()
        .and_then(|(index, _)| m.map_fort_next_level(*index, &levels))
        .map(|k| (k.to_owned(), m.construction_cost(region, k), m.rules.buildings.get(k).map_or(1, |b| b.turns.max(1)), m.building_tech_ok(r.owner, k)));
    // A level's upkeep: the construction row builder's `upkeep` key (CONFIRMED). PROVISIONAL which
    // effect that is; ours is `building_maintenance_cost`, the one the model sums for a region's
    // income.
    let upkeep_of = |level: &str| -> i32 {
        m.rules
            .buildings
            .get(level)
            .and_then(|b| b.effects.iter().find(|(k, _)| k == "building_maintenance_cost").map(|(_, v)| *v as i32))
            .unwrap_or(0)
    };
    let upkeeps: HashMap<String, i32> = standing.iter().map(|(_, k)| k).chain(next.iter().map(|n| &n.0)).map(|l| (l.clone(), upkeep_of(l))).collect();
    let controlable = fort_controlable(m, &ui.link.human, region);
    drop(state);

    let entry = |level: &str, kind: i32, cost: i32, turns: u32, percent: f32, affordable: bool, tech: bool| -> mlua::Result<Table> {
        let e = lua.create_table()?;
        let image = ui.building_icon(&owner_key, level);
        let (name, short, long) = building_texts_for(inner, ui, &owner_key, level);
        e.set("type", kind)?;
        e.set("region_key", region_key.as_str())?;
        e.set("slot_key", "")?;
        e.set("building_key", level)?;
        e.set("name", name)?;
        e.set("description", short.as_str())?;
        e.set("short_description", short)?;
        e.set("long_description", long)?;
        e.set("image", image)?;
        e.set("health", 100)?;
        e.set("being_repaired", false)?;
        e.set("repairable", false)?;
        e.set("can_repair", false)?;
        e.set("can_afford_repair", false)?;
        e.set("repair_cost", 0)?;
        e.set("dismantling", false)?;
        e.set("alternate_chain", false)?;
        e.set("availability", i32::from(affordable) | (i32::from(tech) << 1))?;
        // The fort panel's upgrade branch reads these three as separate keys (CONFIRMED).
        e.set("affordable", affordable)?;
        e.set("tech_present", tech)?;
        e.set("upkeep", upkeeps.get(level).copied().unwrap_or(0))?;
        e.set("cost", cost)?;
        e.set("turns_to_completion", turns)?;
        e.set("percent_complete", percent)?;
        e.set("technologies", lua.create_table()?)?;
        Ok(e)
    };
    let forts = lua.create_table()?;
    match standing.as_ref().map(|(_, k)| k) {
        Some(level) => {
            forts.raw_push(entry(level, 1, 0, 0, 100.0, true, true)?)?;
            if let Some((level, cost, turns, tech)) = &next {
                // `affordable` = `0x0047BA10()`: always false in the shipped exe (CONFIRMED).
                forts.raw_push(entry(level, 4, *cost, *turns, 0.0, false, *tech)?)?;
            }
        }
        // No fort chain the owner may hold (made-up data): a BUILT frame with no building, as the
        // panel has no CONSTRUCTABLE branch and asserts on `g_building_slot_components[1]`
        // (Construction.lua:624).
        None => {
            let site = loc(inner, "random_localisation_strings_string_Construction_site").unwrap_or_else(|| "Construction site".into());
            let empty = entry("", 1, 0, 0, 0.0, false, false)?;
            empty.set("name", site)?;
            empty.set("image", "data/ui/buildings/icons/eu_building_placeholder.tga")?;
            forts.raw_push(empty)?;
        }
    }
    let t = lua.create_table()?;
    t.set("forts", forts)?;
    t.set("fort_ptr", 1)?;
    t.set("controlable", controlable)?;
    // Read by the shared ResetConstructionPanel (CONFIRMED, Construction.lua:61); `BuildFortPanelInfoTable`
    // itself pushes only `forts` / `fort_ptr` / `controlable` (CONFIRMED). PROVISIONAL: these two
    // are kept for the script's globals; `infrastructure` set hides the repair / demolish buttons.
    t.set("faction_key", owner_key)?;
    t.set("infrastructure", true)?;
    Ok(Value::Table(t))
}

/// Ids handed to the scripts for values that have no id of their own (slots, queue items), one per
/// value: the first request for a value gives it the next id, and that id names that value for the
/// rest of the session (reloads included), so an address a script kept can never come to name
/// another value. No packing of the value's parts into one integer, so none of them has a size limit.
#[derive(Debug)]
struct Interner<K> {
    values: Vec<K>,
    ids: HashMap<K, i32>,
}

impl<K: Copy + Eq + std::hash::Hash> Interner<K> {
    fn new() -> Self {
        Self { values: Vec::new(), ids: HashMap::new() }
    }

    /// The id of `value`, handing out the next one the first time.
    fn id(&mut self, value: K) -> i32 {
        let values = &mut self.values;
        *self.ids.entry(value).or_insert_with(|| {
            values.push(value);
            (values.len() - 1) as i32
        })
    }

    /// The value `id` names.
    fn get(&self, id: i32) -> Option<K> {
        self.values.get(usize::try_from(id).ok()?).copied()
    }
}

/// The address a script sees for a building slot (see `CampaignUi::slot_ids`). PROVISIONAL: the
/// original's is the slot object's pointer.
fn slot_value(ui: &CampaignUi, region: RegionId, slot: SlotRef) -> Value {
    let id = ui.slot_ids.borrow_mut().id((region, slot));
    ui.entity(TAG_SLOT, id)
}

/// The region and slot a slot address names ([`slot_value`]).
fn slot_from_entity(ui: &CampaignUi, v: &Value) -> Option<(RegionId, SlotRef)> {
    ui.slot_ids.borrow().get(entity_of(v, TAG_SLOT)?)
}

/// `CampaignUI.ConstructBuildingTree(slot, parent)`'s data (`0x009E1CB0` → `0x009B8830`, CONFIRMED
/// structure): {nodes = {{key, parent (index in nodes, 0 = a root), state, image, tooltip}...},
/// region_key, faction_key, slot_key}, built breadth first:
/// - roots: the levels an empty slot of this type takes (level 0 of a chain allowed in the slot
///   type), each passing the permission test `0x008BE7F0`; children: each node's upgrade levels
///   (`building_upgrades_junction`, record +0x160), each passing it too;
/// - state (`0x009B9120`): "normal" for the slot's standing level and every node above it, else
///   "available" / "unavailable" from `0x009B5A60` (status 0 / 1; 2 hides the node). For a
///   built slot the status asks `0x00DDFD40` (INFERRED: the level could be built there now, cost
///   aside); for an empty slot a level is available when it is one of the slot's roots;
/// - the node's picture is its culture icon ("{<key>:1}<path>", CONFIRMED string pieces
///   0x0132E1B8 / 0x0136F224: the node's component id is the level key), its tooltip the name and
///   the random string `right_click_info` (0xFC) joined by "\n" (INFERRED order).
///
/// PROVISIONAL: the level record's +0x5C flag (hides a node outside some region, `0x00A8B5A0`)
/// is not known and not applied.
fn building_tree(lua: &Lua, inner: &Inner, ui: &CampaignUi, slot: &Value) -> mlua::Result<Value> {
    let m = ui.model();
    let Some((region, index)) = slot_from_entity(ui, slot) else { return Ok(Value::Nil) };
    let Some(r) = m.world.regions.get(&region) else { return Ok(Value::Nil) };
    let Some((slot_type, standing)) = r.construction_slot(index) else { return Ok(Value::Nil) };
    let (slot_type, standing) = (slot_type.to_owned(), standing.map(|b| b.level_key.clone()));
    let Some(slot_key) = slot_key_of(r, index) else { return Ok(Value::Nil) };
    let owner = r.owner;
    let owner_key = m.world.factions.get(&owner).map(|f| f.key.clone()).unwrap_or_default();
    let permitted = |k: &str| {
        m.building_permitted(owner, k)
            && m.rules.buildings.get(k).is_some_and(|b| m.rules.chain_slots.get(&b.chain).is_some_and(|t| t.contains(&slot_type)))
    };
    let roots: Vec<String> = m.rules.buildings.iter().filter(|(k, b)| b.level == 0 && permitted(k)).map(|(k, _)| k.clone()).collect();
    // (level, parent index + 1)
    let mut nodes: Vec<(String, usize)> = roots.iter().map(|k| (k.clone(), 0)).collect();
    let mut i = 0;
    while i < nodes.len() {
        let ups = m.rules.buildings.get(&nodes[i].0).map(|b| b.upgrades_to.clone()).unwrap_or_default();
        for u in ups {
            // A chain never loops, but guard against bad data.
            if permitted(&u) && !nodes.iter().any(|(k, _)| *k == u) {
                nodes.push((u, i + 1));
            }
        }
        i += 1;
    }
    // Available: what the slot can start now by the model's one option rule (researched levels
    // only, as `can_build` checks), less the scripts' restricted levels.
    let startable: Vec<String> = m.construction_options_in(region, index, None).into_iter().filter(|o| o.tech).map(|o| o.level_key).collect();
    let available = |k: &str| startable.iter().any(|s| s == k);
    // The standing level and its ancestors are drawn "normal".
    let mut normal = vec![false; nodes.len()];
    if let Some(s) = &standing
        && let Some(mut at) = nodes.iter().position(|(k, _)| k == s)
    {
        loop {
            normal[at] = true;
            match nodes[at].1 {
                0 => break,
                p => at = p - 1,
            }
        }
    }
    let states: Vec<&str> =
        nodes.iter().enumerate().map(|(j, (k, _))| if normal[j] { "normal" } else if available(k) { "available" } else { "unavailable" }).collect();
    let region_key = r.key.clone();
    drop(m);
    let right_click = loc(inner, "random_localisation_strings_string_right_click_info").unwrap_or_default();
    let list = lua.create_table()?;
    for (j, (key, parent)) in nodes.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("key", key.as_str())?;
        e.set("parent", *parent)?;
        e.set("state", states[j])?;
        e.set("image", ui.building_icon(&owner_key, key))?;
        let name = building_texts(inner, ui, key).0;
        e.set("tooltip", if right_click.is_empty() { name } else { format!("{name}\n{right_click}") })?;
        list.raw_push(e)?;
    }
    let t = lua.create_table()?;
    t.set("nodes", list)?;
    t.set("region_key", region_key)?;
    t.set("faction_key", owner_key)?;
    t.set("slot_key", slot_key)?;
    Ok(Value::Table(t))
}

/// `CampaignUI.BuildingBrowserDetails(region)` (0x009DDD40 → 0x009B5AF0, CONFIRMED keys) →
/// `{slots = {entry...}, region = address, region_name}` for the region given, else the selected
/// settlement when the player owns it, else the player's capital. One entry per slot that has a
/// building or could get one (the region's slot list, then its road), in the region's order:
/// `chains` ({key, tooltip} per chain the slot type allows), `slot`, `faction_key`, `slot_key`,
/// `region_key`, `type`, `location`, and either `level`, `max_level`, `building_key`, `image`,
/// `name`, `entry_tooltip1`, `entry_tooltip2` or, for an empty slot, `level` = `max_level` = -1,
/// `building_key` = "empty", `image` = "", `name`, `slot_type`, the two tooltips.
/// `type` (CONFIRMED order of the exe's tests; labels from building_browser.lua): 1 capital (a
/// settlement slot of the faction capital), 2 region capital (any other settlement slot), 3 town,
/// 4 port, 8 resource, 5 farm, 6 road, 7 fortification; other slots are left out.
/// Tooltips (CONFIRMED pieces, random loc strings 0xFC right_click_info, 0xFD
/// left_click_view_tree, 0xFE left_click_return_list, joined with "\n"): name + view tree + right
/// click info / name + return to list + right click info; empty slots: "Construction site" + view
/// tree / + return to list. `max_level`: the highest level of the chain (the exe counts only
/// levels the faction may build: PROVISIONAL). `location`: the settlement name for types 1 and 2,
/// else the slot's loc name (`campaign_map_slots_onscreen_<key>` /
/// `campaign_map_towns_and_ports_onscreen_name_<key>`, CONFIRMED keys). PROVISIONAL: a slot is
/// listed when it has a building or a level-0 building of an allowed chain can be built there;
/// chain tooltips are empty (the exe's chain string +0x24 is UNKNOWN); the road's slot key is ours
/// (`road:<region>`).
fn building_browser_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, arg: Option<RegionId>) -> mlua::Result<Value> {
    let m = ui.model();
    let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let selected = match ui.selection.get() {
        CampaignSelection::Settlement(r) if human.is_some() && m.world.regions.get(&r).map(|x| x.owner) == human => Some(r),
        _ => None,
    };
    let region = arg.or(selected).or_else(|| human.and_then(|h| m.world.capital(h)));
    let out = lua.create_table()?;
    let slots = lua.create_table()?;
    out.set("slots", slots.clone())?;
    let Some(r) = region.and_then(|r| m.world.regions.get(&r)) else { return Ok(Value::Table(out)) };
    let owner_key = m.world.factions.get(&r.owner).map(|f| f.key.clone()).unwrap_or_default();
    let capital = m.world.is_capital(r.id);
    let word = |k: &str| loc(inner, &format!("random_localisation_strings_string_{k}")).unwrap_or_default();
    let (view_tree, back, info) = (word("left_click_view_tree"), word("left_click_return_list"), word("right_click_info"));
    let site = word("Construction_site");
    let slot_types = ui.slot_types(inner);
    // The level-0 candidates per slot type, once for all the region's empty slots (as the AI's snapshot).
    let new_levels = m.new_levels_by_slot_type();
    // (slot, key, slot type, building): the region's slots, then the walls, then the road -- the
    // exe's order (`0x009B5AF0` walks the slot manager's list, then `+0x24` fortification, then
    // `+0x20` road; CONFIRMED).
    let list: Vec<(SlotRef, String, String, Option<String>)> = (0..r.slots.len())
        .map(SlotRef::Slot)
        .chain([SlotRef::Walls, SlotRef::Road])
        .filter_map(|slot| {
            let (slot_type, b) = r.construction_slot(slot)?;
            Some((slot, slot_key_of(r, slot)?, slot_type.to_owned(), b.map(|b| b.level_key.clone())))
        })
        .collect();
    let mut n = 0;
    for (slot, key, slot_type, building) in list {
        let kind = if key.starts_with("settlement:") && slot_type != "settlement_road" && slot_type != "settlement_fortification" {
            if capital { 1 } else { 2 }
        } else if slot_type == "settlement_road" {
            6
        } else if slot_type == "settlement_fortification" {
            7
        } else {
            match slot_types.get(&slot_type) {
                Some(t) if t.town => 3,
                Some(t) if t.port => 4,
                Some(t) if t.resource => 8,
                Some(t) if t.farm => 5,
                _ => continue,
            }
        };
        let chains: Vec<&String> = m.rules.chain_slots.iter().filter(|(_, types)| types.contains(&slot_type)).map(|(c, _)| c).collect();
        // An empty slot is listed when a level can be started there (the model's one option
        // rule with the scripts' restricted levels, researched levels only, as `can_build` checks).
        if building.is_none() && !m.construction_options_in(r.id, slot, Some(&new_levels)).iter().any(|o| o.tech) {
            continue;
        }
        let e = lua.create_table()?;
        let ct = lua.create_table()?;
        for (j, c) in chains.iter().enumerate() {
            let ce = lua.create_table()?;
            ce.set("key", c.as_str())?;
            ce.set("tooltip", "")?;
            ct.set(j + 1, ce)?;
        }
        e.set("chains", ct)?;
        e.set("slot", slot_value(ui, r.id, slot))?;
        e.set("faction_key", owner_key.as_str())?;
        e.set("slot_key", key.as_str())?;
        e.set("region_key", r.key.as_str())?;
        e.set("type", kind)?;
        let location = match kind {
            1 | 2 => ui.settlement_name(inner, r.id),
            _ => loc(inner, &format!("campaign_map_slots_onscreen_{key}"))
                .or_else(|| loc(inner, &format!("campaign_map_towns_and_ports_onscreen_name_{key}")))
                .unwrap_or_else(|| ui.settlement_name(inner, r.id)),
        };
        e.set("location", location)?;
        match &building {
            Some(level_key) => {
                let (level, max) = m.rules.chain_levels(level_key).unwrap_or((0, 0));
                let (name, _) = building_texts(inner, ui, level_key);
                e.set("level", level)?;
                e.set("max_level", max)?;
                e.set("building_key", level_key.as_str())?;
                e.set("image", ui.building_icon(&owner_key, level_key))?;
                e.set("entry_tooltip1", format!("{name}\n{view_tree}\n{info}"))?;
                e.set("entry_tooltip2", format!("{name}\n{back}\n{info}"))?;
                e.set("name", name)?;
            }
            None => {
                let name = match kind {
                    3 => word("town"),
                    4 => word("port"),
                    5 => word("farm"),
                    6 => word("road"),
                    7 => word("fortification"),
                    8 => loc(inner, "advice_levels_advice_item_title_-332744214").unwrap_or_default(),
                    _ => String::new(),
                };
                e.set("level", -1)?;
                e.set("max_level", -1)?;
                e.set("building_key", "empty")?;
                e.set("image", "")?;
                e.set("name", name)?;
                e.set("slot_type", slot_type.as_str())?;
                e.set("entry_tooltip1", format!("{site}\n{view_tree}"))?;
                e.set("entry_tooltip2", format!("{site}\n{back}"))?;
            }
        }
        n += 1;
        slots.set(n, e)?;
    }
    out.set("region", region_value(ui, r.id))?;
    out.set("region_name", region_name(&inner.loc, &r.key))?;
    Ok(Value::Table(out))
}


/// Attitude level of an attitude total (0x00B0DBA0, CONFIRMED rule): the
/// `diplomatic_relations_attitudes` values (hostile, unfriendly, neutral, friendly, very_friendly)
/// give four thresholds, each the mean of two neighbours; total ≤ t1 → 0 hostile, ≤ t2 → 1
/// unfriendly, ≤ t3 → 2 neutral, ≤ t4 → 3 friendly, else 4 very friendly.
fn attitude_level(levels: &HashMap<String, i32>, total: i32) -> usize {
    let v = |k: &str| levels.get(k).copied().unwrap_or(0);
    let t = [(v("hostile") + v("unfriendly")) / 2, (v("unfriendly") + v("neutral")) / 2, (v("neutral") + v("friendly")) / 2, (v("friendly") + v("very_friendly")) / 2];
    t.iter().position(|&x| total <= x).unwrap_or(4)
}

/// The attitude factors of a relationship as the exe lists them (0x00B27760, CONFIRMED pieces):
/// one "[ALIGN:L]<factor>: [ALIGN:R]<±n>" line per factor that counts, the factor named by loc
/// `diplomacy_factor_strings_<positive|negative>_factor_string_<factor>`. PROVISIONAL: the exact
/// spacing and the peace-treaty special case of the original are not reproduced.
fn relationship_details(inner: &Inner, rel: &ntw_sim::campaign::Relationship) -> String {
    let mut lines = Vec::new();
    for (i, key) in ntw_sim::campaign::details::ATTITUDE_FACTORS.iter().enumerate() {
        let Some(f) = rel.attitudes.get(i) else { continue };
        let n = f.contribution();
        if n == 0 {
            continue;
        }
        let sign = if n > 0 { "positive" } else { "negative" };
        let name = loc(inner, &format!("diplomacy_factor_strings_{sign}_factor_string_{key}")).unwrap_or_else(|| (*key).to_owned());
        lines.push(format!("[ALIGN:L]{name}: [ALIGN:R]{n:+}"));
    }
    lines.join("\n")
}

/// `CampaignUI.RetrieveFactionListForDiplomacy()` (0x009F2EA0, CONFIRMED keys) → a table keyed by
/// faction key, one entry per faction except pirates: Name, Key, IsHuman, IsMajor, FlagPath,
/// ReligionIcon, ReligionName, Government, GovernmentName; for factions other than the player
/// also Relationship ("allied", "at war", "protectorate" or nil, the player's stance towards it),
/// Trading ("trading" with a trade agreement, else "can_trade" / "cannot_trade"), LandTrade,
/// TradingTooltip (random loc `trade_status_tooltip_already_trading` / `_can` / `_cannot`),
/// Attitude (diplomacy loc `relationship_<level>` of the faction's attitude towards the player,
/// 0x00B64CC0), AttitudeValue (that level, 0..4), PlayersRelationshipDetails and
/// FactionRelationshipDetailsTowardsPlayer (the attitude factor lists). PROVISIONAL: can_trade
/// when not at war (the exe's trade-route tests 0x00C1A7D0 / 0x00B27FE0 are not ported),
/// LandTrade false; destroyed factions are kept (the exe skips factions with +0x824 set, UNKNOWN).
fn faction_list_for_diplomacy(lua: &Lua, inner: &Inner, ui: &CampaignUi) -> mlua::Result<Value> {
    let m = ui.model();
    let out = lua.create_table()?;
    let me = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let levels = ui.attitude_levels(inner);
    let word = |k: &str| loc(inner, &format!("random_localisation_strings_string_{k}")).unwrap_or_default();
    for f in m.world.factions.values() {
        if f.key == "pirates" {
            continue;
        }
        let e = lua.create_table()?;
        e.set("Name", faction_name(inner, &ui.link.db, &f.key))?;
        e.set("Key", f.key.as_str())?;
        e.set("IsHuman", m.turn.humans.contains(&f.id))?;
        let details = m.world.faction_details.get(&f.id);
        e.set("IsMajor", details.and_then(|d| d.major).unwrap_or(false))?;
        e.set("FlagPath", ui.link.db.faction(&f.key).map(|r| r.flag_path.clone()).unwrap_or_default())?;
        let religion = details.map(|d| d.religion.clone()).unwrap_or_default();
        e.set("ReligionIcon", ui.religion_icon(inner, &religion))?;
        e.set("ReligionName", loc(inner, &format!("religions_onscreen_{religion}")).unwrap_or(religion))?;
        e.set("Government", f.government_key.as_str())?;
        e.set("GovernmentName", loc(inner, &format!("government_types_onscreen_{}", f.government_key)).unwrap_or_default())?;
        if let Some(me) = me
            && me != f.id
        {
            let stance = m.world.factions.get(&me).and_then(|p| p.diplomacy.get(&f.id).copied()).unwrap_or_default();
            let relationship = match stance {
                ntw_sim::campaign::Stance::Allied => Some("allied"),
                ntw_sim::campaign::Stance::War => Some("at war"),
                ntw_sim::campaign::Stance::Protectorate | ntw_sim::campaign::Stance::Patron => Some("protectorate"),
                ntw_sim::campaign::Stance::Neutral => None,
            };
            e.set("Relationship", relationship)?;
            let towards_me = m.world.relationships.get(&(f.id, me));
            let mine = m.world.relationships.get(&(me, f.id));
            if stance != ntw_sim::campaign::Stance::War {
                let (trading, tip) = if mine.is_some_and(|r| r.trade_agreement) {
                    ("trading", "trade_status_tooltip_already_trading")
                } else {
                    ("can_trade", "trade_status_tooltip_can")
                };
                e.set("Trading", trading)?;
                e.set("LandTrade", false)?;
                e.set("TradingTooltip", word(tip))?;
            }
            let level = attitude_level(&levels, towards_me.map_or(0, |r| r.attitude_total()));
            const NAMES: [&str; 5] = ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"];
            e.set("Attitude", loc(inner, &format!("diplomacy_strings_string_relationship_{}", NAMES[level])).unwrap_or_default())?;
            e.set("AttitudeValue", level)?;
            e.set("PlayersRelationshipDetails", mine.map(|r| relationship_details(inner, r)).unwrap_or_default())?;
            e.set("FactionRelationshipDetailsTowardsPlayer", towards_me.map(|r| relationship_details(inner, r)).unwrap_or_default())?;
        }
        out.set(f.key.as_str(), e)?;
    }
    Ok(Value::Table(out))
}


/// `CampaignUI.RetrieveDiplomacyDetails(faction)` (0x009F2750 → 0x009B2690, CONFIRMED keys) →
/// `{AtWar, Allies, TradeRights, Protectorates, ProtectorOf}`: the factions the given faction is at
/// war with (stance 0), allied to (2), trades with (trade agreement), has as protectorates (3),
/// is the protectorate of (4). Each entry (0x009B1B50): Name, Label (the faction key), Flag
/// ("<flag path>/small.tga"). Nil for an unknown faction. Our stances map as Protectorate → 3,
/// Patron → 4 (INFERRED).
fn diplomacy_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Value> {
    use ntw_sim::campaign::Stance;
    let m = ui.model();
    let Some(f) = m.faction_by_key(key) else { return Ok(Value::Nil) };
    let lists: Vec<Table> = (0..5).map(|_| lua.create_table()).collect::<mlua::Result<_>>()?;
    for (other, stance) in &f.diplomacy {
        let Some(o) = m.world.factions.get(other) else { continue };
        let e = lua.create_table()?;
        e.set("Name", faction_name(inner, &ui.link.db, &o.key))?;
        e.set("Label", o.key.as_str())?;
        let flag = ui.link.db.faction(&o.key).map(|r| r.flag_path.clone()).unwrap_or_default();
        e.set("Flag", format!("{flag}/small.tga"))?;
        let list = match stance {
            Stance::War => Some(0),
            Stance::Allied => Some(1),
            Stance::Protectorate => Some(3),
            Stance::Patron => Some(4),
            Stance::Neutral => None,
        };
        if let Some(i) = list {
            lists[i].set(o.key.as_str(), e.clone())?;
        }
        if m.world.relationships.get(&(f.id, *other)).is_some_and(|r| r.trade_agreement) {
            lists[2].set(o.key.as_str(), e)?;
        }
    }
    let out = lua.create_table()?;
    for (i, name) in ["AtWar", "Allies", "TradeRights", "Protectorates", "ProtectorOf"].iter().enumerate() {
        out.set(*name, lists[i].clone())?;
    }
    Ok(Value::Table(out))
}


/// A character's details table as the exe builds it for character cards and panels
/// (0x009AD250, CONFIRMED key names; used by FactionDetails' Leader, the government ministers,
/// ...): Name, Age, AgentType, Title, Address, Flag ("<flag>/portrait_flags.tga"), SmallFlag
/// ("<flag>/small.tga"), ActionPoints, ActionPointsPerTurn, InfoImage and CardImage ("data/" +
/// the portrait pictures), IsGuerilla, IsNaval, CommanderType, ShowAsCharacter, Playable,
/// Location, FactionColour / UniformColour {r, g, b}, Attributes {PrimaryLevel,
/// PrimaryAttributePath, PrimaryAttributeName, [i] = {Value, PipPath, AttributeName}}, Traits,
/// Ancillaries, plus spying_data_level (read by Utilities.CreateCharacterCard).
/// PROVISIONAL: Title empty, Traits / Ancillaries / PostEffects as key lists only, spying_data_level
/// 3 (everything known). The attribute pictures are `agent_attributes`' ([`attribute_icon`]). The card
/// template also reads ShowAttributes, ShowFlag and TechImage (CONFIRMED reads); they are not given
/// (UNKNOWN values).
fn character_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, c: CharacterId) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(ch) = m.world.characters.get(&c) else { return Ok(Value::Nil) };
    let d = m.world.character_details.get(&c);
    let faction_key = m.world.factions.get(&ch.faction).map(|f| f.key.clone()).unwrap_or_default();
    let t = lua.create_table()?;
    t.set("Address", character_value(ui, c))?;
    t.set("AgentType", ch.kind.esf_name())?;
    t.set("Title", "")?;
    if let Some(b) = d.and_then(|d| d.birth) {
        t.set("Age", (m.calendar.date.year as i32 - b.year as i32).max(0))?;
    }
    let rec = ui.link.db.faction(&faction_key);
    let flag = rec.map(|r| r.flag_path.clone()).unwrap_or_default();
    t.set("Flag", format!("{flag}/portrait_flags.tga"))?;
    t.set("SmallFlag", format!("{flag}/small.tga"))?;
    t.set("ActionPoints", ch.movement_points)?;
    t.set("ActionPointsPerTurn", ch.max_movement_points)?;
    if let Some(p) = d.map(|d| &d.portrait) {
        if !p.card.is_empty() {
            t.set("CardImage", format!("data/{}", p.card))?;
        }
        if !p.info.is_empty() {
            t.set("InfoImage", format!("data/{}", p.info))?;
        }
    }
    // IsGuerilla (CONFIRMED name; Agents.lua's buttons read it): INFERRED from the character type
    // `guerilla` (the spa start's guerillas).
    t.set("IsGuerilla", ch.kind == CharacterKind::Guerilla)?;
    let naval = matches!(ch.kind, CharacterKind::Admiral | CharacterKind::Captain);
    t.set("IsNaval", naval)?;
    t.set("CommanderType", if naval { 2 } else { 0 })?;
    // ShowAsCharacter / CommandedUnit / Soldiers (own Ghidra copy, 2026-10-07; CONFIRMED in
    // `0x009AD250`): the character is shown as himself unless he commands a unit and his agent
    // type is neither 0 (General) nor 1 (admiral) (`0x00F9C6C0` / `0x00F9C690` on the agent record
    // `+0x2C`); otherwise CommandedUnit is his unit's card with DisplayAsUnit true. Soldiers is his
    // force's soldier count (force vfunc `+0x74`, ours the units' men) whenever he has a force. So
    // the Lists rows (`template.row_template_army.lua:65-71`) give generals their portrait card and
    // colonels their unit's picture. PLACEHOLDER: a general or admiral with no portrait in the
    // model (pool hires, promoted generals; BACKLOG §0 "portraits of generated characters") keeps his unit card.
    let force = m.force_of(c).and_then(|f| m.world.forces.get(&f));
    let commanded = force.filter(|f| f.commander == Some(c) && !f.units.is_empty());
    let show_as_character = commanded.is_none()
        || (matches!(ch.kind, CharacterKind::General | CharacterKind::Admiral) && portrait_card(&m, c).is_some());
    t.set("ShowAsCharacter", show_as_character)?;
    if let Some(f) = &force {
        t.set("Soldiers", f.units.iter().map(|u| u.men).sum::<u32>())?;
    }
    t.set("Playable", rec.is_some_and(|r| r.category == "playable"))?;
    t.set("Location", ui.location_name(inner, (ch.position.0.to_f32(), ch.position.1.to_f32())))?;
    let colour = |c: [u8; 3]| -> mlua::Result<Table> {
        let ct = lua.create_table()?;
        ct.set("r", c[0])?;
        ct.set("g", c[1])?;
        ct.set("b", c[2])?;
        Ok(ct)
    };
    if let Some(r) = rec {
        t.set("FactionColour", colour(r.primary_colour())?)?;
        t.set("UniformColour", colour(r.secondary_colour())?)?;
    }
    t.set("Attributes", attributes_table(lua, inner, ui, Some(c), Pips::With)?)?;
    let traits = lua.create_table()?;
    for (i, tr) in d.map(|d| d.traits.as_slice()).unwrap_or_default().iter().enumerate() {
        traits.set(i + 1, tr.key.as_str())?;
    }
    t.set("Traits", traits)?;
    let anc = lua.create_table()?;
    for (i, a) in d.map(|d| d.ancillaries.as_slice()).unwrap_or_default().iter().enumerate() {
        anc.set(i + 1, a.as_str())?;
    }
    t.set("Ancillaries", anc)?;
    // What the player may know about this character (`utilities.lua`'s `spying_data_level`,
    // CONFIRMED values; see `spying_level_character` for the INFERRED mapping onto the model).
    // A foreign character under the shroud shows only what the player knows of him.
    t.set("spying_data_level", match m.faction_by_key(&ui.link.human).map(|h| h.id) {
        Some(human) => spying_level_character(&m, human, c),
        None => LEVEL_INVALID,
    })?;
    // knowledge_mask: the same answer as the unit rows carry (bit 1 the icon, 15 one's own).
    t.set("knowledge_mask", knowledge_mask(match m.faction_by_key(&ui.link.human).map(|h| h.id) {
        Some(human) => spying_level_character(&m, human, c),
        None => LEVEL_INVALID,
    }))?;
    // Post / PostName / PostEffects (CONFIRMED names): the government post the character holds,
    // its name for the faction's government (`ministerial_positions_by_gov_types`: faction, post,
    // government → string key, loc `ministerial_positions_strings_on_screen_<key>`; column use
    // INFERRED from the rows). PROVISIONAL: PostEffects empty.
    let post = m.world.faction_details.get(&ch.faction).and_then(|d| d.posts.iter().find(|p| p.holder == Some(c)).map(|p| p.key.clone()));
    if let Some(post) = post {
        let gov = m.world.factions.get(&ch.faction).map(|f| f.government_key.clone()).unwrap_or_default();
        let name = small_table(inner, "db/ministerial_positions_by_gov_types_tables/ministerial_positions_by_gov_types", "s,s,s,s,s")
            .into_iter()
            .find(|r| {
                r.first().and_then(|v| v.as_str()) == Some(faction_key.as_str())
                    && r.get(1).and_then(|v| v.as_str()) == Some(post.as_str())
                    && r.get(2).and_then(|v| v.as_str()) == Some(gov.as_str())
            })
            .and_then(|r| r.get(4).and_then(|v| v.as_str()).map(str::to_owned))
            .and_then(|k| loc(inner, &format!("ministerial_positions_strings_on_screen_{k}")))
            .unwrap_or_else(|| post.clone());
        t.set("Post", post)?;
        t.set("PostName", name)?;
        t.set("PostEffects", lua.create_table()?)?;
    }
    let commanded = commanded.filter(|_| !show_as_character).cloned();
    drop(m);
    if let Some(f) = commanded {
        t.set("CommandedUnit", unit_entry(lua, inner, ui, &f, 0, true)?)?;
    }
    t.set("Name", character_name(inner, ui, c).unwrap_or_else(|| character_type_name(inner, ui, c)))?;
    Ok(Value::Table(t))
}

// ---------------------------------------------------------------------------------------------
// The agents tab (`ui/agents.luac`, CHARACTER_UI_HOOKS.md "Agents panel"). What the panel does with
// its info and which engine calls its buttons make is CONFIRMED from the panel's bytecode (read on
// the install 2026-10-05); what the engine puts in the info and answers is built here from the
// model, each part tagged.

/// The five abilities the agent buttons test (`card.character.Abilities.<key> == true`, CONFIRMED
/// keys read by `ShowAgentButtons`).
const AGENT_BUTTON_ABILITIES: [&str; 5] = ["can_assassinate", "can_sabotage", "can_sabotage_army", "can_research", "can_duel"];

/// A spy type: the types the model's assassination gate accepts (`0x009225D0`, CONFIRMED kind test).
fn is_spy_kind(kind: CharacterKind) -> bool {
    matches!(kind, CharacterKind::Rake | CharacterKind::Assassin | CharacterKind::Guerilla)
}

/// A character listed on the agents tab: not a commander type (general, colonel, admiral, captain)
/// and not a minister (off-map). INFERRED: the tab is about agents; the engine's list is not traced.
fn is_listed_agent_kind(kind: CharacterKind) -> bool {
    !matches!(
        kind,
        CharacterKind::General | CharacterKind::Colonel | CharacterKind::Admiral | CharacterKind::Captain | CharacterKind::Minister
    )
}

/// Whether a character has one of [`AGENT_BUTTON_ABILITIES`].
/// - With saved `AgentAbilities` (CONFIRMED data, `CharacterDetails::abilities`): the model's ability
///   level ([`ntw_sim::campaign::agents::ability`], `0x009C7610`) is at least 1, the threshold the
///   model's own action gates use (`duel_chance`, `spy_chance`, `army_sabotage_chance`,
///   `building_sabotage_chance`). `can_assassinate` also needs a spy type, the model's
///   assassination gate.
/// - Without (a character the model made itself): INFERRED from the type, following which type
///   each model action is for: spies (rake, assassin, guerilla) assassinate and sabotage
///   (buildings and armies); gentlemen and scholars duel and research (steal: the model's
///   `steal_step` is a gentleman's). Missionaries have none of the five.
fn agent_has_ability(m: &CampaignModel, c: CharacterId, key: &str) -> bool {
    let Some(kind) = m.world.characters.get(&c).map(|ch| ch.kind) else { return false };
    let saved = m.world.character_details.get(&c).is_some_and(|d| !d.abilities.is_empty());
    if saved {
        return ntw_sim::campaign::agents::ability(m, c, key) >= 1 && (key != "can_assassinate" || is_spy_kind(kind));
    }
    let scholar = matches!(kind, CharacterKind::Gentleman | CharacterKind::EasternScholar);
    match key {
        "can_assassinate" | "can_sabotage" | "can_sabotage_army" => is_spy_kind(kind),
        "can_research" | "can_duel" => scholar,
        _ => false,
    }
}

/// Where an agent is "in" (the original's residences: settlements, ports and other map slots,
/// `SIEGEABLE_GARRISON_RESIDENCE` / `GARRISON_RESIDENCE`, which the model does not load).
/// INFERRED from the model's own fields:
/// - garrisoned in a region → its settlement;
/// - else standing on a non-settlement slot's position (a port, a school, a town; the model's
///   `steal_step` places a thief the same way) → that slot;
/// - else standing on a settlement's position → that settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Residence {
    Settlement(RegionId),
    Slot(RegionId, usize),
}

fn residence_of(m: &CampaignModel, c: CharacterId) -> Option<Residence> {
    let ch = m.world.characters.get(&c)?;
    if let Some(r) = ch.garrisoned_in {
        return Some(Residence::Settlement(r));
    }
    for r in m.world.regions.values() {
        if let Some(i) = r.slots.iter().position(|s| !s.key.starts_with("settlement:") && s.position == Some(ch.position)) {
            return Some(Residence::Slot(r.id, i));
        }
    }
    m.world.regions.values().find(|r| r.settlement.position == ch.position).map(|r| Residence::Settlement(r.id))
}

impl Residence {
    fn region(self) -> RegionId {
        match self {
            Residence::Settlement(r) | Residence::Slot(r, _) => r,
        }
    }
    /// The faction holding it: a slot's holder, else the region's owner.
    fn owner(self, m: &CampaignModel) -> Option<FactionId> {
        let r = m.world.regions.get(&self.region())?;
        Some(match self {
            Residence::Settlement(_) => r.owner,
            Residence::Slot(_, i) => r.slots.get(i).and_then(|s| s.holder).unwrap_or(r.owner),
        })
    }
    /// The address the scripts get: the region's for a settlement (as `SetSelectedEntity`), the
    /// building browser's slot address for a slot. PROVISIONAL (the original's is the residence
    /// object's pointer).
    fn value(self, ui: &CampaignUi) -> Value {
        match self {
            Residence::Settlement(r) => region_value(ui, r),
            Residence::Slot(r, i) => slot_value(ui, r, SlotRef::Slot(i)),
        }
    }
}

/// The agents the agents tab of `region`'s settlement lists, by character id. INFERRED (the
/// engine's list is not traced): every agent type ([`is_listed_agent_kind`]) standing in the region
/// ([`ntw_sim::campaign::economy::in_region`]) that the human's faction knows of (its own always;
/// a foreign one when [`ntw_sim::campaign::agents::knows_character`]: hidden spies stay off the tab).
fn listed_agents(ui: &CampaignUi, region: RegionId) -> Vec<CharacterId> {
    let m = ui.model();
    let Some(reg) = m.world.regions.get(&region) else { return Vec::new() };
    let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Vec::new() };
    m.world
        .characters
        .values()
        .filter(|c| is_listed_agent_kind(c.kind) && ntw_sim::campaign::economy::in_region(&m, reg, c))
        .filter(|c| ntw_sim::campaign::agents::knows_character(&m, human, c.id))
        .map(|c| c.id)
        .collect()
}

/// The agents tab's `GenerateAgentsPanel(info)`: {agents, characters, controlable}.
/// CONFIRMED from `ui/agents.luac`: `agents[i]` and `characters[i]` are parallel lists (the same
/// agent); the panel reads `agents[i].card_id` (the card map's key and the card component's id, so a
/// string) and `agents[i].name` (the card tooltip text, `SetTooltipText(name, true)`), and hands
/// `agents[i]` to the hover tooltip's `InitialiseAgent`, which reads `name` and `agent_type_name`
/// and nothing else; `characters[i]` goes to `Utilities.CreateCharacterCard` and to the buttons
/// (`Abilities.*`, `IsGuerilla`, `Address`); the panel sets `characters[i].PlayerControlled` itself;
/// `controlable` is read and dropped (`GenerateAgentCards` takes two arguments).
/// - `card_id`: PROVISIONAL format `agent_<character id>` (unique per agent);
/// - `name`: the character's name, else his type's (as `character_details`' `Name`);
/// - `agent_type_name`: his type's name in his faction's culture (`agent_culture_details` loc
///   key, [`agent_type_name`]; CONFIRMED 53 keys for 53 rows on the install);
/// - `characters[i]`: [`character_details`] plus `Abilities` ([`agent_has_ability`]);
/// - `controlable`: an empty table (unused, CONFIRMED).
fn agents_info(lua: &Lua, inner: &Inner, ui: &CampaignUi, region: RegionId) -> mlua::Result<Value> {
    let agents = lua.create_table()?;
    let characters = lua.create_table()?;
    for (i, c) in listed_agents(ui, region).into_iter().enumerate() {
        let Value::Table(details) = character_details(lua, inner, ui, c)? else { continue };
        let abilities = lua.create_table()?;
        {
            let m = ui.model();
            for key in AGENT_BUTTON_ABILITIES {
                abilities.set(key, agent_has_ability(&m, c, key))?;
            }
        }
        details.set("Abilities", abilities)?;
        let a = lua.create_table()?;
        a.set("card_id", format!("agent_{}", c.0))?;
        a.set("name", details.get::<Value>("Name")?)?;
        // The card's "unitcard" hover tooltip hands `agents[i]` to the tooltip template's
        // `InitialiseAgent`, which reads exactly two fields of it (CONFIRMED from
        // `ui\templates\template.unitcard_tooltip.luac`, the proto at line 115): `name` (the line
        // it shows) and `agent_type_name` (in brackets after it). Everything else that proto does
        // is fixed: the function component is set to the `strat_army` state, the crew and the four
        // `dy_stat` rows are hidden and the tooltip resizes expanded.
        a.set("agent_type_name", character_type_name(inner, ui, c))?;
        agents.set(i + 1, a)?;
        characters.set(i + 1, details)?;
    }
    let t = lua.create_table()?;
    t.set("agents", agents)?;
    t.set("characters", characters)?;
    t.set("controlable", lua.create_table()?)?;
    Ok(Value::Table(t))
}

/// The agent an agent-panel call names (every one passes the agent's `Address`, CONFIRMED arity 1).
fn agent_of(m: &CampaignModel, v: &Value) -> Option<(CharacterId, FactionId)> {
    let c = entity_of(v, TAG_CHARACTER).map(CharacterId)?;
    m.world.characters.get(&c).map(|ch| (c, ch.faction))
}

/// `ValidAssassinationTargets(agent)`: whether at least one character the agent may assassinate
/// exists. Validity is the model's (`assassination_chance` is `Some`, CONFIRMED gate; another
/// faction, as `approach_to` requires) and the target is known to the agent's faction
/// (`knows_character`). PROVISIONAL: the candidates are the whole map -- the original surely limits
/// them (reach, sight), but that path is not traced.
fn valid_assassination_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    use ntw_sim::campaign::agents::{assassination_chance, knows_character};
    m.world
        .characters
        .values()
        .any(|t| t.faction != own && knows_character(m, own, t.id) && assassination_chance(m, agent, t.id).is_some())
}

/// `ValidSabotageArmyTarget(agent)`: whether a force exists the agent may sabotage: another
/// faction's, with a known commander, and `army_sabotage_chance` is `Some` (CONFIRMED gate).
/// PROVISIONAL: the whole map, as [`valid_assassination_targets`].
fn valid_sabotage_army_target(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    use ntw_sim::campaign::agents::{army_sabotage_chance, knows_character};
    m.world.forces.values().any(|f| {
        f.faction != own
            && f.commander.is_some_and(|c| knows_character(m, own, c))
            && army_sabotage_chance(m, agent, f.id).is_some()
    })
}

/// `ValidSabotageTarget(agent)`: whether a building the agent may sabotage stands in his residence
/// (INFERRED: the residence's buildings -- a slot's own building, or the settlement's slots), held by
/// another faction, with `building_sabotage_chance` `Some` (CONFIRMED gate). No residence: false.
fn valid_sabotage_target(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    let Some(res) = residence_of(m, agent) else { return false };
    let Some(r) = m.world.regions.get(&res.region()) else { return false };
    let slots: Vec<usize> = match res {
        Residence::Slot(_, i) => vec![i],
        Residence::Settlement(_) => (0..r.slots.len()).filter(|&i| r.slots[i].key.starts_with("settlement:")).collect(),
    };
    slots.into_iter().any(|i| {
        r.slots[i].holder.unwrap_or(r.owner) != own && ntw_sim::campaign::agents::building_sabotage_chance(m, agent, r.id, i).is_some()
    })
}

/// `ValidDuelTargetsInResidence(agent)`: whether another faction's character known to his own
/// stands in the same residence ([`residence_of`]) and the two may duel (`duel_chance` is `Some`,
/// CONFIRMED gate; the weapon does not change it). INFERRED: "in residence" is that residence.
fn valid_duel_targets_in_residence(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    use ntw_sim::campaign::agents::{duel_chance, knows_character, Weapon};
    let Some(res) = residence_of(m, agent) else { return false };
    m.world.characters.values().any(|t| {
        t.faction != own
            && residence_of(m, t.id) == Some(res)
            && knows_character(m, own, t.id)
            && duel_chance(m, agent, t.id, Weapon::Pistols).is_some()
    })
}

/// `CharacterInValidEnemyUniversity(agent)`: whether his residence is another faction's school (the
/// model's `school`: a full-health building with `research_points`, CONFIRMED) -- where the model's
/// `steal_step` lets a foreign gentleman steal. INFERRED meaning from the name.
fn in_valid_enemy_university(m: &CampaignModel, agent: CharacterId, own: FactionId) -> bool {
    match residence_of(m, agent) {
        Some(Residence::Slot(r, i)) => m.school(r, i).is_some_and(|owner| owner != own),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// The fog of war as the interface sees it (CHARACTER_UI_HOOKS.md H4). What the player may know
// about a character or a force is the model's own sight and knowledge model (`CampaignModel::sees`
// and `agents::knows_character`, both CONFIRMED, CHARACTERS_FIDELITY.md §10); the five data levels
// and the knowledge bit mask are the original's `Utilities.lua` globals (CONFIRMED values, read on
// the install 2026-10-05, `utilities.lua` lines 55..81).

/// `SPYING_DATA_LEVEL_INVALID` (`-1`): the address is not a character or a force.
const LEVEL_INVALID: i32 = -1;
/// `SPYING_DATA_LEVEL_PASSIVE` (`0`): only a marker on the map is known.
const LEVEL_PASSIVE: i32 = 0;
/// `SPYING_DATA_LEVEL_BASIC` (`1`): the piece is in the player's lists.
const LEVEL_BASIC: i32 = 1;
/// `SPYING_DATA_LEVEL_ADVANCED` (`2`): the player may open its details -- the root's double-click
/// handler only opens the character / unit panel when the level reaches this
/// (`layout.root.lua:1093`, CONFIRMED).
const LEVEL_ADVANCED: i32 = 2;
/// `SPYING_DATA_LEVEL_OWNED` (`3`): the player's own faction.
const LEVEL_OWNED: i32 = 3;

/// `SPYING_UNIT_DATA_ICON_KNOWN | MEN | GUNS | XP` = 15 = `SPYING_UNIT_DATA_OWNED` (CONFIRMED).
const KNOWLEDGE_OWNED: i32 = 15;
/// What is known about a foreign unit without opening it: the icon only (`SPYING_UNIT_DATA_ICON_KNOWN`,
/// CONFIRMED).
const KNOWLEDGE_ICON_ONLY: i32 = 1;

/// The knowledge bit mask for a data level (the `unit_record.knowledge_mask` the card reads,
/// CONFIRMED in `utilities.lua`'s `Initialise`: the owned mask for one's own units).
fn knowledge_mask(level: i32) -> i32 {
    match level {
        LEVEL_OWNED => KNOWLEDGE_OWNED,
        LEVEL_ADVANCED => KNOWLEDGE_OWNED,
        LEVEL_BASIC | LEVEL_PASSIVE => KNOWLEDGE_ICON_ONLY,
        _ => 0,
    }
}

/// `CampaignUI.SpyingDataLevelCharacter(address)`: how much the human's faction knows about a
/// character. Arity 1 and the `>= ADVANCED` gate CONFIRMED (`layout.root.lua:1093`); the exe's
/// handler is not traced.
///
/// The mapping onto the model is INFERRED, each step from a CONFIRMED model rule:
/// - own faction → `OWNED`;
/// - a foreign character the faction knows ([`agents::knows_character`]: the hidden-flag test and the
///   exposed lists) and whose cell the shroud currently has visible ([`CampaignModel::sees`]) →
///   `ADVANCED` (the details may be opened);
/// - a known one under the shroud → `BASIC`;
/// - one the faction does not know → `PASSIVE`.
fn spying_level_character(m: &CampaignModel, human: FactionId, c: CharacterId) -> i32 {
    let Some(ch) = m.world.characters.get(&c) else { return LEVEL_INVALID };
    if ch.faction == human {
        return LEVEL_OWNED;
    }
    if !ntw_sim::campaign::agents::knows_character(m, human, c) {
        return LEVEL_PASSIVE;
    }
    let at = (ch.position.0.to_f32(), ch.position.1.to_f32());
    if m.sees(human, at) { LEVEL_ADVANCED } else { LEVEL_BASIC }
}

/// `CampaignUI.SpyingDataLevelUnit(address)`: the same for a force's unit row, read from the
/// commander the unit belongs to. Arity 1 CONFIRMED (`layout.root.lua:1093`). INFERRED mapping (the
/// model's knowledge is per character, §10).
fn spying_level_unit(m: &CampaignModel, human: FactionId, unit: UnitId) -> i32 {
    let Some(owner) = m.world.forces.values().find(|f| f.units.iter().any(|u| u.id == unit)) else {
        return LEVEL_INVALID;
    };
    // The player's own force is OWNED whether or not it has a commander (review 0-G: a commanderless
    // own force used to fall through to PASSIVE).
    if owner.faction == human {
        return LEVEL_OWNED;
    }
    match owner.commander {
        Some(c) => spying_level_character(m, human, c),
        None => LEVEL_PASSIVE,
    }
}

// ---------------------------------------------------------------------------------------------
// The commander pool panel (`ui/enlist_commander_scripts/enlist_commander.luac`, read on the
// install 2026-10-05, CONFIRMED): the army / navy panel's Promote button opens it
// (`army.lua:582` `panel_manager:OpenPanel("enlist_commander", ..., "InitEnlistCommander",
// g_panel_is_navy, g_military_force)`), which asks the engine for the candidates and the pool state
// and then the player picks one.

/// The pool a force recruits from: admirals for a navy, generals for an army (the panel's own
/// `SetAsRecruitmentType(is_navy)` switch, CONFIRMED).
fn pool_kind_of(m: &CampaignModel, force: ForceId) -> Option<(ntw_sim::campaign::pool::PoolKind, FactionId)> {
    use ntw_sim::campaign::pool::PoolKind;
    let f = m.world.forces.get(&force)?;
    Some((if f.is_navy { PoolKind::Admiral } else { PoolKind::General }, f.faction))
}

/// `CampaignUI.AvailableCommandersForRecruitment(force, is_navy)`: the pool of the faction the force
/// belongs to. CONFIRMED names and arity (2: the force's address, `is_navy`) from
/// `enlist_commander.lua:38`, which reads `CurrentNumGenerals`, `MaxGeneralsAllowed`,
/// `MaxDistanceToTrack`, `DistanceToCapital`, `TurnsToNextPoolFill` and `#table` candidates.
///
/// Per candidate CONFIRMED fields (`enlist_commander_entry.lua:14`): `commander_pointer` (what
/// `CampaignUI.PromoteUnits` is handed), `Name`, `RecruitmentCost`, `Attributes.PrimaryLevel`,
/// `UniqueId`, `IsRecruitable`, `InfoImage`, `Traits`.
///
/// - the price is the model's hire cost measured at the force ([`CampaignModel::hire_cost_into`],
///   the CONFIRMED formula: 400 + 300 × rank + the distance part);
/// - `DistanceToCapital` / `MaxDistanceToTrack`: the force's distance to its capital and
///   `character_recruitment_max_distance` (the panel divides them for its bar; INFERRED pair, the
///   model's own distance part uses the same maximum);
/// - `TurnsToNextPoolFill`: the pool timer's turns;
/// - `CurrentNumGenerals` / `MaxGeneralsAllowed`: the faction's commanders of that kind in the world
///   and that count plus the pool's cap (`character_recruitment_pool_cap`).
///
///   **PROVISIONAL, and the absence is now a checked negative rather than a gap.** The shipped
///   data contains no general limit to read: `db\campaign_variables_tables\campaign_variables`
///   holds exactly ten `character_recruitment*` keys (base cost, cost per command star, max
///   distance, pool cap, and six refill rates) and **none** of them is a cap on the number of
///   generals; `db\effect_bonus_value_basic_junction_tables\effect_bonus_value_basic_junction`
///   has four `character_recruitment*` rows and none grants such a bonus either; and no key
///   containing `generals` or `num_generals` exists in any of the 86,977 files in `data.pack`.
///   So **there is no data-level general limit in the vanilla game**, and whatever the original
///   shows here is either a fixed constant, a difficulty setting or something not in the DB.
///   Ours answers "commanders of that kind in the world" and "that plus the pool cap"; the panel
///   only uses the pair for its "3 / 5" label, so the number is cosmetic.
///
/// One row of the commander pool panel (`enlist_commander_entry.lua`), gathered from the model.
struct CommanderRow {
    character: CharacterId,
    cost: Option<i32>,
    type_name: String,
    recruitable: bool,
    historical: bool,
    /// At most four trait keys (CONFIRMED: the entry shows four).
    traits: Vec<String>,
    card: String,
}

fn commanders_for_recruitment(lua: &Lua, inner: &Inner, ui: &CampaignUi, force: ForceId) -> mlua::Result<Value> {
    // Everything the model answers, gathered first (the model's borrow is a `Ref`, so it must be
    // dropped before the tables are built).
    let gathered = {
        let m = ui.model();
        let Some((kind, faction)) = pool_kind_of(&m, force) else { return Ok(Value::Nil) };
        let list = m.world.faction_details.get(&faction).map(|d| match kind {
            ntw_sim::campaign::pool::PoolKind::General => &d.general_pool,
            ntw_sim::campaign::pool::PoolKind::Admiral => &d.admiral_pool,
        });
        let (candidates, timer) = list.cloned().unwrap_or_default();
        let purse = m.world.factions.get(&faction).map_or(0, |f| f.treasury);
        let at = m.force_position(force);
        let capital = m.world.faction_details.get(&faction).and_then(|d| d.capital);
        let distance = match (at, capital) {
            (Some(at), Some(r)) => m
                .world
                .regions
                .get(&r)
                .map(|reg| {
                    let (dx, dz) = (reg.settlement.position.0.to_f32() - at.0.to_f32(), reg.settlement.position.1.to_f32() - at.1.to_f32());
                    (dx * dx + dz * dz).sqrt()
                })
                .unwrap_or(0.0),
            _ => 0.0,
        };
        let rows: Vec<CommanderRow> = candidates
            .iter()
            .map(|&c| {
                let cost = m.hire_cost_into(c, force);
                let details = m.world.character_details.get(&c);
                CommanderRow {
                    character: c,
                    cost,
                    type_name: character_type_name(inner, ui, c),
                    recruitable: cost.is_some_and(|c| c <= purse),
                    historical: details.and_then(|d| d.historical_key.as_deref()).is_some(),
                    traits: details.map(|d| d.traits.iter().take(4).map(|t| t.key.clone()).collect()).unwrap_or_default(),
                    card: portrait_card(&m, c).unwrap_or_default(),
                }
            })
            .collect();
        let in_command = m
            .world
            .characters
            .values()
            .filter(|c| c.faction == faction)
            .filter(|c| match kind {
                ntw_sim::campaign::pool::PoolKind::General => c.kind == CharacterKind::General,
                ntw_sim::campaign::pool::PoolKind::Admiral => c.kind == CharacterKind::Admiral,
            })
            .count();
        (rows, in_command, m.pool_cap(), m.rules.var("character_recruitment_max_distance", 1000.0), distance, timer.saturating_sub(m.calendar.turns_elapsed))
    };
    let (rows, in_command, cap, max_distance, distance, turns) = gathered;
    let t = lua.create_table()?;
    for (i, row_in) in rows.into_iter().enumerate() {
        let CommanderRow { character: c, cost, type_name, recruitable, historical, traits, card } = row_in;
        // The character's own name if the model has one (our own save writers keep it), else his type.
        let name = character_name(inner, ui, c).unwrap_or(type_name);
        let row = lua.create_table()?;
        row.set("commander_pointer", character_value(ui, c))?;
        row.set("Name", name)?;
        // A string: the row puts it straight into `dy_cost`'s state text (CONFIRMED).
        row.set("RecruitmentCost", cost.unwrap_or(0).to_string())?;
        // Only the `Primary*` fields (the entry reads no pips).
        row.set("Attributes", attributes_table(lua, inner, ui, Some(c), Pips::Without)?)?;
        // PROVISIONAL format (the original's is the agent record's pointer); unique per candidate.
        row.set("UniqueId", format!("commander_{}", c.0))?;
        row.set("IsRecruitable", recruitable)?;
        row.set("InfoImage", if card.is_empty() { String::new() } else { format!("data/{card}") })?;
        let traits_table = lua.create_table()?;
        for (j, key) in traits.iter().enumerate() {
            // The entry shows at most four traits (CONFIRMED: `enlist_commander_entry.lua:36`).
            // `ui\templates\character_trait_entry.luac` then hands each row to `InitialiseTrait`
            // (the proto at line 5: `Name` into `tx_enables`, `IconFilename` into `effect_icon`)
            // and `SetTraitTooltip` (the proto at line 17), which calls
            // `Utilities.GetEffectList(row.Effects, row.AttributeEffects)`. That function takes
            // `#attribute_effects` on its *second* argument first (utilities.lua:182, pc 2), so
            // **both fields must be tables or the panel raises** -- which is exactly what stopped
            // the enlist-commander panel: the click reached `PanelManager.OpenPanel`, built every
            // row, then died inside the first trait's tooltip, so no commander could be picked.
            // CONFIRMED: the two field names, that they are lists, and the four the tooltip also
            // reads (`ColourText`, `ExplanationText`, `RemovalText`).
            // PROVISIONAL: the lists are empty and the three texts empty -- the effect
            // descriptions behind a trait come from a table we do not load.
            let e = lua.create_table()?;
            e.set("Name", key.as_str())?;
            e.set("IconFilename", String::new())?;
            e.set("Effects", lua.create_table()?)?;
            e.set("AttributeEffects", lua.create_table()?)?;
            e.set("ColourText", String::new())?;
            e.set("ExplanationText", String::new())?;
            e.set("RemovalText", String::new())?;
            traits_table.set(j + 1, e)?;
        }
        row.set("Traits", traits_table)?;
        // A historical candidate is badged (the model's `historical_key`, CONFIRMED kept).
        row.set("IsHistorical", historical)?;
        t.set(i + 1, row)?;
    }
    t.set("CurrentNumGenerals", in_command)?;
    t.set("MaxGeneralsAllowed", in_command + cap)?;
    t.set("MaxDistanceToTrack", max_distance)?;
    t.set("DistanceToCapital", distance)?;
    t.set("TurnsToNextPoolFill", turns)?;
    Ok(Value::Table(t))
}

// ---------------------------------------------------------------------------------------------
// The agent action popups (`ui/campaign ui/agent_options.luac` and `agent_action.luac`, read on the
// install 2026-10-05, CONFIRMED): the agents panel's buttons are the engine's `CampaignUI.Agent*`
// calls, which open the options popup; a button there asks the engine for the target list and, when
// it is not empty, the action popup lists the targets; picking one calls `Instigate*` (or
// `SabotageArmy`, `MoveIntoTarget`). The targets are handed over as a table per action.

/// The targets `CampaignUI.RequestDuelTargets(agent, target)` offers: a duel partner in the agent's
/// own residence ([`valid_duel_targets_in_residence`], its gate CONFIRMED) -- the list the action
/// popup shows, each row a character.
fn duel_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> Vec<CharacterId> {
    use ntw_sim::campaign::agents::{Weapon, duel_chance, knows_character};
    let Some(res) = residence_of(m, agent) else { return Vec::new() };
    let mut out: Vec<CharacterId> = m
        .world
        .characters
        .values()
        .filter(|t| t.id != agent && t.faction != own && residence_of(m, t.id) == Some(res) && knows_character(m, own, t.id))
        .filter(|t| duel_chance(m, agent, t.id, Weapon::Pistols).is_some())
        .map(|t| t.id)
        .collect();
    out.sort_by_key(|c| c.0);
    out
}

/// The targets `CampaignUI.RequestAssassinationTargets(agent, target)` offers (the model's
/// assassination gate, CONFIRMED; the candidate set PROVISIONAL as in
/// [`valid_assassination_targets`]).
fn assassination_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> Vec<CharacterId> {
    use ntw_sim::campaign::agents::{assassination_chance, knows_character};
    let mut out: Vec<CharacterId> = m
        .world
        .characters
        .values()
        .filter(|t| t.id != agent && t.faction != own && knows_character(m, own, t.id))
        .filter(|t| assassination_chance(m, agent, t.id).is_some())
        .map(|t| t.id)
        .collect();
    out.sort_by_key(|c| c.0);
    out
}

/// The targets `CampaignUI.RequestSabotageTargets(agent, target)` offers: the buildings in the
/// agent's own residence the model's sabotage gate accepts (CONFIRMED gate; the residence INFERRED,
/// as in [`valid_sabotage_target`]). Each row is a (region, slot) pair -- the popup's
/// `sabotage_entry` template.
fn sabotage_targets(m: &CampaignModel, agent: CharacterId, own: FactionId) -> Vec<(RegionId, usize)> {
    let Some(res) = residence_of(m, agent) else { return Vec::new() };
    let Some(r) = m.world.regions.get(&res.region()) else { return Vec::new() };
    let slots: Vec<usize> = match res {
        Residence::Slot(_, i) => vec![i],
        Residence::Settlement(_) => (0..r.slots.len()).filter(|&i| r.slots[i].key.starts_with("settlement:")).collect(),
    };
    slots
        .into_iter()
        .filter(|&i| r.slots.get(i).is_some_and(|s| s.building.is_some()))
        .filter(|&i| r.slots[i].holder.unwrap_or(r.owner) != own)
        .filter(|&i| ntw_sim::campaign::agents::building_sabotage_chance(m, agent, r.id, i).is_some())
        .map(|i| (r.id, i))
        .collect()
}

/// One row of a `Request*Targets` list: the fields the shipped row templates read, taken off the
/// model's own gate so the picker shows the real number.
///
/// CONFIRMED field names, and the row is the **address field `Address`**, not `target`:
/// - `ui\templates\template.character_duel_info_pane.luac`, `InitCharacter(action, row)`
///   (proto at line 15, arity 2 CONFIRMED from `agent_action.lua:21` pc 79-83): `row.Address`,
///   `row.Name`, `row.Chance` and `row.Faction.{Name, Key}`, plus `row.Faction.FlagPath` which
///   `agent_action.lua:21` pc 37-40 concatenates into the template's picture argument.
/// - `ui\templates\template.sabotage_entry.luac`, `Initialise(row)`: the same `Address`, `Name`,
///   `Chance` and `Faction.FlagPath`, plus `row.IconFilename`, `row.ShortDescription`, `row.Level`
///   and `row.MaxLevel` for the icon, the tooltip and the six level pips.
///
/// Both templates' buttons hand `CampaignUI.Instigate*(m_attacker, row.Address)` back -- the
/// **address**, not the row (`character_duel_info_pane.lua:52/57` and `sabotage_entry.lua:32`, each
/// `UIComponent(this):Parent("agent_action"):LuaCall("InstigateDuel"/"InstigateAssassination"/"InstigateSabotage", m_target)`).
struct TargetRow {
    address: Value,
    faction: FactionId,
    chance: i32,
    /// A character row's on-screen name, or a sabotage row's building name.
    name: String,
    /// Sabotage rows only; empty for a character row.
    icon: String,
    short: String,
    level: i32,
    max_level: i32,
    /// The row's character, whose `Attributes` ([`attributes_table`]) the card shows: it reads
    /// `Attributes.PrimaryAttributePath` (`utilities.lua:107` pc 29-31) and its template the rest.
    /// `None` for a sabotage row.
    character: Option<CharacterId>,
}

/// An agent attribute's picture (`PipPath` / `PrimaryAttributePath`): the one mapping every card
/// and panel uses, `agent_attributes`' icon column (see `CharacterTables::attribute_icon` for the
/// evidence and tags).
///
/// A `PLACEHOLDER` row becomes the empty path here, at the UI boundary. INFERRED: the exe hands the
/// literal on (its resolver `0x00A06F20` finds no separator and no `<skin>/PLACEHOLDER` file, so it
/// keeps the string) and no file of that name exists, so the original draws nothing; the empty path
/// draws the same nothing without a texture-not-found warning.
fn attribute_icon(ui: &CampaignUi, key: &str) -> String {
    match ui.link.db.campaign.characters.attribute_icon(key) {
        "PLACEHOLDER" => String::new(),
        icon => icon.to_owned(),
    }
}

/// An attribute's on-screen name (`agent_attributes_onscreen_name_<key>`, else the key).
fn attribute_name(inner: &Inner, key: &str) -> String {
    loc(inner, &format!("agent_attributes_onscreen_name_{key}")).unwrap_or_else(|| key.to_owned())
}

/// Whether an [`attributes_table`] carries the per-attribute pips or only the `Primary*` fields.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pips {
    /// `[i] = {Value, PipPath, AttributeName}` too (character details, agent-action target rows).
    With,
    /// Only `Primary*` (the recruitment rows and the commander's unit card read no pips).
    Without,
}

/// The `Attributes` table every character card and panel reads (character details, recruitment
/// rows, agent-action target rows, the commander's unit card): `PrimaryAttributePath`,
/// `PrimaryLevel`, `PrimaryAttributeName`, then, with [`Pips::With`], `[i] = {Value, PipPath,
/// AttributeName}` for the character's own attribute list.
///
/// The primary attribute is the character type's main attribute (`agents::main_attribute`): the exe's
/// builder `0x009AD250` takes the character's main-attribute index (`0x00A198C0`) and fills the three
/// `Primary*` fields only when it is a valid index (< 14), CONFIRMED. `PrimaryLevel` =
/// `min(rank + 1, 9)` with the rank from `0x00A198D0` (`0x009AE759..0x009AE768`): the formula is
/// CONFIRMED; the rank it reads is our `agents::rank`, CONFIRMED for agents but PROVISIONAL for
/// generals, admirals, ministers and missionaries (their theatre / battle / army-make-up /
/// minister-post bonuses are not modelled, see `GetCharacterRank` in `CHARACTERS_FIDELITY.md`), so
/// their `PrimaryLevel` is PROVISIONAL too.
///
/// `character` is `None` for rows that are not a character (sabotage targets), and a character id
/// with no record has no type: PROVISIONAL empty `Primary*` values so the card's unguarded reads still
/// find strings (the exe always has the record, so it never takes this path).
///
/// PROVISIONAL: the exe's per-attribute part is all 14 attributes keyed by attribute key (its loop
/// `0x009AE87D..`, value from `0x009CB560`, not traced, so whether it is the base or the effective
/// level is UNKNOWN); ours is the character's own attribute list (base values) as an array.
fn attributes_table(lua: &Lua, inner: &Inner, ui: &CampaignUi, character: Option<CharacterId>, pips: Pips) -> mlua::Result<Table> {
    let attrs = lua.create_table()?;
    let (primary, list) = match character {
        Some(c) => {
            let m = ui.model();
            let primary = m.world.characters.get(&c).map(|ch| {
                let level = (ntw_sim::campaign::agents::rank(&m, c) + 1).min(9);
                (ntw_sim::campaign::agents::main_attribute(ch.kind), level)
            });
            let list = if pips == Pips::With { character_attributes(&m, c) } else { Vec::new() };
            (primary, list)
        }
        None => (None, Vec::new()),
    };
    match primary {
        Some((key, level)) => {
            attrs.set("PrimaryLevel", level)?;
            attrs.set("PrimaryAttributePath", attribute_icon(ui, key))?;
            attrs.set("PrimaryAttributeName", attribute_name(inner, key))?;
        }
        None => {
            attrs.set("PrimaryLevel", 0)?;
            attrs.set("PrimaryAttributePath", "")?;
            attrs.set("PrimaryAttributeName", "")?;
        }
    }
    for (i, (key, value)) in list.iter().enumerate() {
        let a = lua.create_table()?;
        a.set("Value", *value)?;
        a.set("PipPath", attribute_icon(ui, key))?;
        a.set("AttributeName", attribute_name(inner, key))?;
        attrs.set(i + 1, a)?;
    }
    Ok(attrs)
}

/// A character's own attribute list (`character_details` `Attributes`' pips, the same list the
/// agents tab uses); the primary attribute is chosen by [`attributes_table`].
fn character_attributes(m: &CampaignModel, c: CharacterId) -> Vec<(String, i32)> {
    m.world.character_details.get(&c).map(|d| d.attributes.clone()).unwrap_or_default()
}

/// The candidate rows of one `Request*Targets` call.
fn target_candidates(ui: &CampaignUi, inner: &Inner, action: &str, agent: CharacterId, own: FactionId) -> Vec<TargetRow> {
    let m = ui.model();
    match action {
        "duel" => duel_targets(&m, agent, own)
            .into_iter()
            .map(|c| {
                // The chance the model's duel is actually rolled at. `CampaignModel::duel` fights with
                // `duel_weapon` (`0x00AAAC30`, CONFIRMED rule): the target picks the weapon with the
                // *lower* chance for the challenger, a coin flip on a tie -- so the number is the
                // smaller of the two `duel_chance`s (`0x00922940`, CONFIRMED), whichever way the
                // flip goes. (Review 0-E: this used to show the pistols chance alone, which is not
                // the number the duel is rolled at whenever swords are worse for the challenger.)
                // INFERRED that the original's row shows the same number: the row carries one
                // `Chance` and the shipped pane does not say which weapon it is for.
                use ntw_sim::campaign::agents::{Weapon, duel_chance};
                let chance = duel_chance(&m, agent, c, Weapon::Pistols)
                    .into_iter()
                    .chain(duel_chance(&m, agent, c, Weapon::Swords))
                    .min()
                    .unwrap_or(0);
                TargetRow {
                    name: character_name(inner, ui, c).unwrap_or_else(|| character_type_name(inner, ui, c)),
                    address: character_value(ui, c),
                    faction: m.world.characters[&c].faction,
                    chance,
                    icon: String::new(),
                    short: String::new(),
                    level: 0,
                    max_level: 0,
                    character: Some(c),
                }
            })
            .collect(),
        "assassinate" => assassination_targets(&m, agent, own)
            .into_iter()
            .map(|c| {
                let chance = ntw_sim::campaign::agents::assassination_chance(&m, agent, c).unwrap_or(0);
                TargetRow {
                    name: character_name(inner, ui, c).unwrap_or_else(|| character_type_name(inner, ui, c)),
                    address: character_value(ui, c),
                    faction: m.world.characters[&c].faction,
                    chance,
                    icon: String::new(),
                    short: String::new(),
                    level: 0,
                    max_level: 0,
                    character: Some(c),
                }
            })
            .collect(),
        _ => {
            let owner = residence_of(&m, agent).and_then(|x| x.owner(&m)).unwrap_or(FactionId(-1));
            sabotage_targets(&m, agent, own)
                .into_iter()
                .map(|(region, i)| {
                    let chance = ntw_sim::campaign::agents::building_sabotage_chance(&m, agent, region, i).unwrap_or(0);
                    let level_key = m
                        .world
                        .regions
                        .get(&region)
                        .and_then(|r| r.slots.get(i))
                        .and_then(|s| s.building.as_ref())
                        .map(|b| b.level_key.clone())
                        .unwrap_or_default();
                    let owner_key = m.world.factions.get(&owner).map(|f| f.key.clone()).unwrap_or_default();
                    let name = loc(inner, &format!("buildings_name_{level_key}")).unwrap_or_else(|| level_key.clone());
                    let short = loc(inner, &format!("buildings_short_description_{level_key}")).unwrap_or_default();
                    TargetRow {
                        address: slot_value(ui, region, SlotRef::Slot(i)),
                        faction: owner,
                        chance,
                        name,
                        icon: ui.building_icon(&owner_key, &level_key),
                        short,
                        level: 0,
                        max_level: 0,
                        character: None,
                    }
                })
                .collect()
        }
    }
}

/// The model commands the agent actions queue. One entry per `CampaignUI` call; a call whose
/// addresses do not name what it needs returns `None` and nothing is queued.
fn agent_action_command(ui: &CampaignUi, name: &str, agent: &Value, target: &Value) -> Option<CampaignCommand> {
    let a = entity_of(agent, TAG_CHARACTER).map(CharacterId);
    match name {
        "InstigateDuel" => Some(CampaignCommand::Duel { challenger: a?, target: entity_of(target, TAG_CHARACTER).map(CharacterId)? }),
        "InstigateAssassination" => Some(CampaignCommand::Assassinate { agent: a?, target: entity_of(target, TAG_CHARACTER).map(CharacterId)? }),
        "InstigateSabotage" => {
            let (region, slot) = slot_from_entity(ui, target)?;
            // Only a building of the region's slot list (the walls and road are not sabotage targets).
            let slot = match slot {
                SlotRef::Slot(i) if ui.model().world.regions.get(&region).is_some_and(|r| i < r.slots.len()) => i,
                _ => return None,
            };
            Some(CampaignCommand::SabotageBuilding { agent: a?, region, slot })
        }
        "SabotageArmy" => Some(CampaignCommand::SabotageArmy { agent: a?, force: entity_of(target, TAG_FORCE).map(|f| ForceId(f as u32))? }),
        _ => None,
    }
}


// ---------------------------------------------------------------------------------------------
// The agent action mask. `ui\campaign ui\agent_options.luac`'s `Initialise(src, target, mask, pct,
// pct)` (arity 5 CONFIRMED, proto at line 34) shows one button per action whose bit is in `mask`
// (`bit.band(mask, action.Mask) ~= 0`, pc 45-56) and **divorces** the button from the panel when it
// is not, so a bit that is not set is a button that does not appear at all.
//
// **Which parameter is which is CONFIRMED from the same prototype (0-E round 5):** pc 0-1 store
// `R[0]`/`R[1]` as the upvalues `m_src`/`m_target`, pc 47 reads `R[2]` as the mask, pc 82 reads `R[3]`
// into the Infiltrate button's `percent_dy` and pc 116 reads `R[4]` into the Sabotage Army button's.
// So the engine call is exactly
// `OpenAgentOptionsPopup(src, target, mask, infiltrate_pct, sabotage_army_pct)`, and the two
// percentages are the last two arguments -- not, as round 3 recorded, "one of Initialise's last two
// arguments".

/// What the player acted on when the agent action menu opened: `agent_options.Initialise`'s
/// `m_target`.
///
/// **CONFIRMED (0-E round 5) from the exe's own registration document, not from a decompile.**
/// `CampaignUI.MoveIntoTarget` is registered at `0x004295D0` with the five-instruction shape the
/// whole binding family uses (`push <doc>; push <name>; push <fn>; mov ecx, <id>; call 0x00998C50`
/// -- `RegionsPublicOrders` does the same at `0x00429E90`, which is the control that makes the
/// reading a reading rather than a coincidence): the doc is at `0x01364B50`, the name `MoveIntoTarget`
/// at `0x01364BB8`, the function at `0x009EE150`. The doc reads, verbatim:
///
/// ```text
/// In: Character (agent), character or settlement (target), bool (research - steal if enemy settlement)
/// ```
///
/// So **`m_target` is a character or a settlement**, never a bare military force, and the third
/// argument is the flag that separates the two research actions -- which is why exactly `research`
/// (16) and `steal_research` (32) pass it and `visit`/`embed`/`counterspy` do not
/// (`agent_options.lua:149`/`:154` against `:139`/`:144`/`:164`, all CONFIRMED).
///
/// This also explains `Initialise`'s first instruction. `string.find(tostring(target), "CHARACTER")`
/// (`agent_options.lua:2-9`) is how one popup serves both kinds: only the character branch builds the
/// `CampaignCharacter` handle the teardown releases at `:89`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMenuTarget {
    Character(CharacterId),
    Settlement(RegionId),
}

/// The military force an [`AgentMenuTarget`] names, for the Sabotage Army percentage.
///
/// **INFERRED**, and named as such because the original's choice is the engine's: the documented
/// target kinds are a character and a settlement, and `agent_options.lua:159` calls
/// `CampaignUI.SabotageArmy(agent, force)` with a force the engine picked without asking. A force has
/// no position of its own in the model -- it moves with its commander (`MilitaryForce`'s own doc) --
/// so a target character is read as "the force he commands" and a target settlement as "the foreign
/// force whose commander stands in it" (`residence_of`, the same residence reading the agents tab
/// uses). The named alternative is *any* force the agent may sabotage anywhere, which is what
/// [`valid_sabotage_army_target`] tests; that reading is not refuted by anything here, it is just not
/// the one the target-based menu implies.
fn agent_menu_force(m: &CampaignModel, agent: CharacterId, target: AgentMenuTarget, own: FactionId) -> Option<ForceId> {
    use ntw_sim::campaign::agents::{army_sabotage_chance, knows_character};
    m.world
        .forces
        .values()
        .filter(|f| {
            f.faction != own && f.commander.is_some_and(|c| knows_character(m, own, c)) && army_sabotage_chance(m, agent, f.id).is_some()
        })
        .find(|f| {
            let c = f.commander.expect("filtered above");
            match target {
                AgentMenuTarget::Character(t) => t == c,
                AgentMenuTarget::Settlement(r) => residence_of(m, c) == Some(Residence::Settlement(r)),
            }
        })
        .map(|f| f.id)
}

/// The two numbers `agent_options.Initialise` puts on the **Infiltrate** and **Sabotage Army**
/// buttons, in that order.
///
/// **CONFIRMED which is which** -- the prototype reads `R[3]` into the button whose `Mask` is
/// `AGENT_ACTION_RAKE_EMBED` (`agent_options.lua:71-87`) and `R[4]` into the one whose `Mask` is
/// `AGENT_ACTION_RAKE_SABOTAGE_ARMY` (`:88-120`), each as `tostring(percent) .. "%"`. So they are
/// `Initialise`'s fourth and fifth parameters.
///
/// **CONFIRMED what they mean** -- a chance of success, in percent, from the game's own advisor
/// (loc, 0-E round 5): *"To infiltrate a city, select your spy and then right-click on the city in
/// question. A menu will appear giving you the options of sabotage, assassination or infiltration.
/// The percentages show the spy's chances of success in each activity."* and *"As with all subterfuge
/// actions there is a percentage chance of success, and failure may mean capture and execution."*
/// (These are the strings behind `advisor\1207..1210_campaign_advice_ui_agent_options_panel_*`,
/// four mp3s that exist for nothing else.)
///
/// **Which model chance each is: INFERRED / PROVISIONAL (review 0-E downgraded this from
/// "real").** The formulas used are the model's existing CONFIRMED ones; *that these are the
/// numbers the original shows* is not. The free model's own exe reading (recorded from
/// `llvm-objdump`, no kept decompile) says the two engine call sites (`0x009C1E20` / `0x009C1EE0`,
/// reached from `0x00A0C3CD` / `0x00A0C442`) ask one interface for the **agent alone**, with ids
/// **5** and **11** (`0x009D1FC0` / `0x009D2000`) -- which are the indices of `can_spy` and
/// `can_sabotage_army` in [`ntw_sim::campaign::agents::ABILITIES`]. If that reading holds, the
/// original's numbers **do not depend on the target**, while both formulas below do (a settlement's
/// protector, a force's size and commander). So either the interface turns an ability id into a
/// target-free number (an ability level, or some other per-agent value) and these are the wrong
/// numbers, or the reading is incomplete. Named lead: decompile `0x009D1FC0` and keep it.
/// - **Infiltrate** = [`spy_chance`] on the target settlement (`0x00922A70`, CONFIRMED formula),
///   chosen because the advisor says *"Select the infiltrate option. If the spy has enough movement
///   points he will automatically try to enter the city."* and infiltrating is spying on arrival.
/// - **Sabotage Army** = [`army_sabotage_chance`] (`0x00922BA0`, CONFIRMED formula) on the force the
///   target names ([`agent_menu_force`], INFERRED -- see there).
///
/// **0 when the target is nothing we can compute from**, which is honest rather than blank: with no
/// target the original had one of these queries answer something and we do not know what.
fn agent_options_percentages(
    m: &CampaignModel,
    agent: CharacterId,
    own: FactionId,
    target: Option<AgentMenuTarget>,
) -> (i32, i32) {
    use ntw_sim::campaign::agents::{SpyTarget, army_sabotage_chance, spy_chance};
    let infiltrate = match target {
        Some(AgentMenuTarget::Settlement(r)) => spy_chance(m, agent, SpyTarget::Settlement(r)).unwrap_or(0),
        _ => 0,
    };
    let sabotage_army = target
        .and_then(|t| agent_menu_force(m, agent, t, own))
        .and_then(|x| army_sabotage_chance(m, agent, x))
        .unwrap_or(0);
    (infiltrate, sabotage_army)
}

/// The nine action bits, CONFIRMED from the module's main chunk (`agent_options.lua:0`, pc 13-85):
/// it computes `bit.lshift(1, n)` for `n` = 0..8 and pairs each with its action name in the order
/// `assassinate`(1), `sabotage`(2), `embed`(3), `research`(4), `steal_research`(5), `duel`(6),
/// `visit`(0), `sabotage_army`(7), `counterspy`(8). The array it stores holds **eight** entries
/// (`SETLIST n=8`, pc 86), so `counterspy`'s bit is computed and dropped -- which is why the popup
/// can never show its Counterspy button even though the layout has one.
mod action_bit {
    // Five of the nine are named but never set: see `agent_options_mask` below, which is where the
    // reason is written down. They are here so the table is the shipped one, complete.
    #![allow(dead_code)]
    pub const VISIT: i32 = 1 << 0;
    pub const ASSASSINATE: i32 = 1 << 1;
    pub const SABOTAGE: i32 = 1 << 2;
    pub const EMBED: i32 = 1 << 3;
    pub const RESEARCH: i32 = 1 << 4;
    pub const STEAL_RESEARCH: i32 = 1 << 5;
    pub const DUEL: i32 = 1 << 6;
    pub const SABOTAGE_ARMY: i32 = 1 << 7;
    /// Built and dropped by the shipped module, so the popup never shows this one.
    pub const COUNTERSPY: i32 = 1 << 8;
}

/// The mask `agent_options.Initialise` is given for `src`.
///
/// **The mask is the engine's**, and only four of its nine bits can be derived from rules we
/// already hold. Each of those four is exactly what the popup's own button does when clicked
/// (`agent_options.lua:109/118/128`, all CONFIRMED): `Duel`, `Assassinate` and `Sabotage` call
/// `CampaignUI.Request*Targets(src, target)`, close themselves, and open the target popup only
/// `if 0 < #targets`. So the bit is set iff that list is not empty -- the same predicate our
/// `Request*Targets` bindings use, which keeps the popup and the list in step (a bit whose list is
/// empty would show a button that then opens nothing).
///
/// **The other five bits are not set, and the reason changed in 0-E round 5.** It used to be "the
/// contract of `MoveIntoTarget` is UNKNOWN"; it is not any more. `MoveIntoTarget`'s parameter kinds
/// and its third flag are CONFIRMED from the exe's own registration document (see
/// [`AgentMenuTarget`]): `(agent, character-or-settlement target[, steal-research])`. What is still
/// missing is the *behaviour* -- what the agent does on arrival, which is the model's agent-order
/// machinery and not a scripting fact. Queueing a bare `CampaignCommand::MoveCharacter` would put an
/// agent in a foreign city with nothing to do there, which is worse than no button. So:
/// - `visit`(1), `embed`(8), `research`(16), `steal_research`(32) and `counterspy`(256) all end in
///   `CampaignUI.MoveIntoTarget(src, target[, true])` (`agent_options.lua:139/144/149/154/164`,
///   CONFIRMED, no other caller), and `MoveIntoTarget` stays a logging stub so the route can never
///   raise. **Next round's target, now sharp:** `embed`'s on-arrival action is the model's
///   `CampaignModel::spy(agent, SpyTarget::Settlement(...))` (`0x0094CCB0`, CONFIRMED) on the advisor's
///   own wording, so wiring `embed` needs the *move-then-resolve* step and nothing else.
/// - `embed` and `sabotage_army` are the two actions `Initialise` puts a percentage on
///   (`percent_dy`, pc 71-87 and 88-120); those two numbers are the model's own chances (which chance each one is: INFERRED), see
///   [`agent_options_percentages`].
///
/// `target` is still accepted and ignored here, deliberately: our `Request*Targets` bindings ignore
/// their second argument too (see [`target_candidates`]), so narrowing here would offer an action the
/// list then refuses. INFERRED that the original narrows the mask by the picked target -- which the
/// two engine call sites are consistent with, as each of them is reached only after a target test.
fn agent_options_mask(m: &CampaignModel, src: CharacterId, own: FactionId) -> i32 {
    let mut mask = 0;
    if !assassination_targets(m, src, own).is_empty() {
        mask |= action_bit::ASSASSINATE;
    }
    if !sabotage_targets(m, src, own).is_empty() {
        mask |= action_bit::SABOTAGE;
    }
    if !duel_targets(m, src, own).is_empty() {
        mask |= action_bit::DUEL;
    }
    if valid_sabotage_army_target(m, src, own) {
        mask |= action_bit::SABOTAGE_ARMY;
    }
    mask
}

/// The [`AgentMenuTarget`] a script argument names, if any: a character address is a character, a
/// region (settlement) address is a settlement, anything else (or nothing) is no target. The two are
/// what the exe's own `MoveIntoTarget` document allows, so nothing else is invented here.
fn menu_target_of(m: &CampaignModel, v: &Value) -> Option<AgentMenuTarget> {
    if let Some(c) = entity_of(v, TAG_CHARACTER)
        && m.world.characters.contains_key(&CharacterId(c))
    {
        return Some(AgentMenuTarget::Character(CharacterId(c)));
    }
    entity_of(v, TAG_REGION).map(|r| AgentMenuTarget::Settlement(RegionId(r as u32)))
}

/// Calls a global of the HUD root layout's script -- the engine side of the `root:LuaCall(name, ...)`
/// the shipped scripts use to reach it (`agent_options.lua:109`, `agent_action.lua:49`).
/// `layout.root.lua:1187` is `OpenAgentActionPopup(action, src, targets)` (arity 3 CONFIRMED) and
/// `layout.root.lua:1191` is `OpenAgentOptionsPopup(src, target, mask, pct, pct)` (arity 5); both
/// forward to `panel_manager.OpenPanel(<panel>, nil, "Initialise", ...)`.
fn call_root_global(lua: &Lua, inner: &Inner, name: &str, args: impl mlua::IntoLuaMulti) -> bool {
    let Some(root) = inner.root_node() else { return false };
    let r = (|| -> mlua::Result<bool> {
        let f: Function = lua.globals().get("__ntw_call_if_defined")?;
        let mut a = args.into_lua_multi(lua)?;
        a.push_front(Value::String(lua.create_string(name)?));
        a.push_front(addr(root));
        f.call::<bool>(a)
    })();
    match r {
        Ok(b) => b,
        Err(e) => {
            log(inner, format!("ERROR in {name}: {e}"));
            false
        }
    }
}

/// The engine side of the agents panel's action buttons: open the target picker for the button's
/// action. `ui\agents.luac` calls `CampaignUI.AgentRakeAssassinate` / `AgentRakeSubterfuge` /
/// `AgentGentlemanDuel` with the selected agent's `Address` and nothing else (arity 1 CONFIRMED);
/// what the original's exe did next is the three lines `agent_options.Assassinate` / `.Sabotage` /
/// `.Duel` run (CONFIRMED, `agent_options.lua:118/128/109`): ask `Request*Targets(src, target)`,
/// and if the list is not empty hand it to the root's `OpenAgentActionPopup(action, src, rows)`.
/// INFERRED that the panel's own button skips the `agent_options` menu in between: that popup's
/// first act is `string.find(tostring(target), "CHARACTER")` (pc 2-9), so it is for a target the
/// player picked on the map, not for a button on the settlement's agents tab.
///
/// An action with no valid target opens nothing and says so in the log -- the original is silent
/// there (`#targets > 0` is its only gate), but a no-op that logs is the difference between a
/// known gap and a broken button.
fn open_agent_action_popup(lua: &Lua, inner: &Inner, ui: &CampaignUi, agent: &Value, action: &str) {
    let m = ui.model();
    let Some((c, own)) = agent_of(&m, agent) else {
        log(inner, format!("agent action {action}: the argument is not a character address"));
        return;
    };
    let rows = target_candidates(ui, inner, action, c, own);
    if rows.is_empty() {
        log(inner, format!("agent action {action}: no valid target"));
        return;
    }
    match target_rows(lua, inner, ui, &rows) {
        Ok(t) => {
            call_root_global(lua, inner, "OpenAgentActionPopup", (action, agent.clone(), t));
        }
        Err(e) => log(inner, format!("agent action {action}: {e}")),
    }
}

/// The list the target popups get: one table per [`TargetRow`], with the field names the shipped row
/// templates read (see [`TargetRow`] for the CONFIRMED list).
fn target_rows(lua: &Lua, inner: &Inner, ui: &CampaignUi, rows: &[TargetRow]) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (i, row) in rows.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("Address", row.address.clone())?;
        e.set("Name", row.name.clone())?;
        e.set("Chance", row.chance)?;
        e.set("IconFilename", row.icon.clone())?;
        e.set("ShortDescription", row.short.clone())?;
        e.set("Level", row.level)?;
        e.set("MaxLevel", row.max_level)?;
        let fac = lua.create_table()?;
        let key = ui.model().world.factions.get(&row.faction).map(|f| f.key.clone()).unwrap_or_default();
        let flag = ui.link.db.faction(&key).map(|r| r.flag_path.clone()).unwrap_or_default();
        let name = faction_name(inner, &ui.link.db, &key);
        fac.set("Key", key)?;
        fac.set("Name", name)?;
        fac.set("FlagPath", flag.clone())?;
        e.set("Faction", fac)?;
        // `Utilities.CreateCharacterCard` concatenates `"{Flags:1}" .. info.Flag` with no guard
        // (`utilities.lua:111` pc 4-6), so this one is mandatory: it is the flag picture the
        // character card draws, the same `<folder>/small.tga` form the diplomacy lists use
        // (`FactionDetails`, line 1632).
        e.set("Flag", format!("{flag}/small.tga"))?;
        // `CampaignCharacterCard.Initialise(info)` indexes `info.Attributes` without a guard (reached
        // through `Utilities.CreateCharacterCard`, whose `LuaCall("Initialise", info, ...)` is the
        // call `utilities.lua:122` reports), and `CreateCharacterCard` itself reads
        // `info.Attributes.PrimaryAttributePath` (pc 29-31). Same shape the agents tab builds.
        e.set("Attributes", attributes_table(lua, inner, ui, row.character, Pips::With)?)?;
        t.set(i + 1, e)?;
    }
    Ok(t)
}


/// `CampaignUI.RetrieveExistingTreaties(a, b)` → one string listing the treaties of `a` with `b`
/// (0x009F2B80 → 0x00B750D0, CONFIRMED pieces), each line a diplomacy loc string
/// `current_treaty_<x>`: protectorate_of_player, at_war, alliance, trade_agreement,
/// giving_military_access_indefinite / _turns, has_military_access_indefinite / _turns,
/// trade_embargoed, embargoing_trade ("%d" = turns). PROVISIONAL: the non-player protectorate
/// lines and the peace-treaty countdown are not listed; lines are joined with "\n" (the exe's
/// joiner 0x00B0B120 is not decoded).
fn existing_treaties(inner: &Inner, ui: &CampaignUi, a: &str, b: &str) -> Option<String> {
    use ntw_sim::campaign::Stance;
    let m = ui.model();
    let fa = m.faction_by_key(a)?;
    let fb = m.faction_by_key(b)?;
    let text = |k: &str| loc(inner, &format!("diplomacy_strings_string_current_treaty_{k}")).unwrap_or_default();
    let turns = |k: &str, n: i32| text(k).replace("%d", &n.to_string());
    let mut lines = Vec::new();
    let stance = fa.diplomacy.get(&fb.id).copied().unwrap_or_default();
    if stance == Stance::Patron {
        lines.push(text("protectorate_of_player"));
    }
    match stance {
        Stance::War => lines.push(text("at_war")),
        Stance::Allied => lines.push(text("alliance")),
        _ => {}
    }
    let ab = m.world.relationships.get(&(fa.id, fb.id));
    let ba = m.world.relationships.get(&(fb.id, fa.id));
    if ab.is_some_and(|r| r.trade_agreement) {
        lines.push(text("trade_agreement"));
    }
    match ab.map_or(0, |r| r.military_access_turns) {
        0 => {}
        n if n < 0 => lines.push(text("giving_military_access_indefinite")),
        n => lines.push(turns("giving_military_access_turns", n)),
    }
    match ba.map_or(0, |r| r.military_access_turns) {
        0 => {}
        n if n < 0 => lines.push(text("has_military_access_indefinite")),
        n => lines.push(turns("has_military_access_turns", n)),
    }
    if let Some(n) = ba.map(|r| r.trade_embargo_turns).filter(|n| *n > 0) {
        lines.push(turns("trade_embargoed", n as i32));
    }
    if let Some(n) = ab.map(|r| r.trade_embargo_turns).filter(|n| *n > 0) {
        lines.push(turns("embargoing_trade", n as i32));
    }
    Some(lines.join("\n"))
}

/// The diplomacy loc name of `from`'s attitude towards `to` (0x00B64CC0: `relationship_<level>`).
fn attitude_text(inner: &Inner, ui: &CampaignUi, from: &str, to: &str) -> Option<String> {
    let m = ui.model();
    let (f, t) = (m.faction_by_key(from)?.id, m.faction_by_key(to)?.id);
    let total = m.world.relationships.get(&(f, t)).map_or(0, |r| r.attitude_total());
    const NAMES: [&str; 5] = ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"];
    let level = attitude_level(&ui.attitude_levels(inner), total);
    loc(inner, &format!("diplomacy_strings_string_relationship_{}", NAMES[level]))
}


/// `CampaignUI.RetrieveGovernorshipDetails(key)` (0x009F41D0 → 0x009BC8B0 and the finance summary
/// 0x009BB910, CONFIRMED key names) → `{Governor, UpperTaxRate, LowerTaxRate, TaxIncomeUpper,
/// TaxIncomeLower, UpperTaxEffects, LowerTaxEffects, UpperEffects, LowerEffects, Theatre,
/// LowestPublicOrder = {Name, Value, Address, IsUpper}, AutomanageTaxes, AutomanageConstruction,
/// and the finance fields}` for the player's governorship. Tax rates are the `taxes_levels` index
/// (0 minimal .. 4 extortionate) of the faction's current levels; the effect lists are {Icon, Tooltip}
/// entries (government_screens.lua AddTaxEffects). PROVISIONAL: one governorship per
/// theatre (the key is not checked), the class incomes halve the tax total, the effect
/// lists empty, automanage off.
fn governorship_details(lua: &Lua, inner: &Inner, ui: &CampaignUi) -> mlua::Result<Value> {
    use ntw_sim::campaign::details::TAX_LEVELS;
    let m = ui.model();
    let Some(f) = m.faction_by_key(&ui.link.human) else { return Ok(Value::Nil) };
    let fid = f.id;
    let index = |k: &str| TAX_LEVELS.iter().position(|l| *l == k).unwrap_or(2);
    let (upper, lower) = (index(&f.tax_upper), index(&f.tax_lower));
    let income = ntw_sim::campaign::economy::faction_income(&m, fid);
    let governor = m.world.faction_details.get(&fid).and_then(|d| d.posts.iter().find(|p| p.governorship.is_some()).and_then(|p| p.holder));
    // The lowest public order of the governed regions (the worse class).
    let lowest = m
        .world
        .regions
        .values()
        .filter(|r| r.owner == fid)
        .map(|r| {
            let po = ntw_sim::campaign::economy::public_order(&m, r.id);
            (r.id, r.key.clone(), po.worst(), po.upper < po.lower)
        })
        .min_by(|a, b| a.2.total_cmp(&b.2));
    drop(m);
    let t = lua.create_table()?;
    if let Some(c) = governor {
        t.set("Governor", character_details(lua, inner, ui, c)?)?;
    }
    t.set("UpperTaxRate", upper)?;
    t.set("LowerTaxRate", lower)?;
    t.set("TaxIncomeUpper", income.taxes / 2)?;
    t.set("TaxIncomeLower", income.taxes - income.taxes / 2)?;
    t.set("UpperTaxEffects", lua.create_table()?)?;
    t.set("LowerTaxEffects", lua.create_table()?)?;
    t.set("UpperEffects", lua.create_table()?)?;
    t.set("LowerEffects", lua.create_table()?)?;
    t.set("Theatre", ui.theatre(inner).map(|r| theatre_name(inner, &r)))?;
    if let Some((id, key, value, is_upper)) = lowest {
        let l = lua.create_table()?;
        l.set("Name", region_name(&inner.loc, &key))?;
        l.set("Value", value.round() as i32)?;
        l.set("Address", region_value(ui, id))?;
        l.set("IsUpper", is_upper)?;
        t.set("LowestPublicOrder", l)?;
    }
    t.set("AutomanageTaxes", false)?;
    t.set("AutomanageConstruction", false)?;
    t.set("TaxIncomeTotal", income.taxes)?;
    t.set("FactionTaxIncomeUpper", income.taxes / 2)?;
    t.set("FactionTaxIncomeLower", income.taxes - income.taxes / 2)?;
    t.set("Trade", income.trade)?;
    t.set("ArmyUpkeep", income.upkeep)?;
    t.set("NavyUpkeep", 0)?;
    t.set("Policing", 0)?;
    t.set("OtherIncome", income.other)?;
    t.set("OtherIncomeTooltip", "")?;
    t.set("AnnualIncome", income.revenue() - income.upkeep)?;
    Ok(Value::Table(t))
}


/// Each technology's `ParentXoffset` / `ParentYoffset` (the tech entry's link to its parent in the
/// tree, which `template.tech_entry.lua:47-94` draws as one horizontal and one vertical
/// `general_purpose_pixel`). Not table columns: the technology table's link step `0x00F21A30`
/// (own Ghidra copy, 2026-10-07; CONFIRMED) computes them at load into record `+0x88` / `+0x84`,
/// which `0x009ABB50` hands out as ParentXoffset / ParentYoffset. For each
/// `technology_required_technology_junctions` row (technology T requires R, in table order) whose
/// R stands in the same tree column as T (records `+0x90` / `+0x94` equal: the building chain's
/// fields, read through the building level; INFERRED equivalent to "same chain", as the three tech
/// chains differ): X = R.pos % 4 − T.pos % 4 and Y = row(T) − row(R), with pos the
/// `tree_column` (C remainder) and row = 2 × building level + (pos > 3) (four positions per row,
/// two rows per level; the level is the building level record's `+0x10`, INFERRED to be its
/// `level`). A later row for the same T overwrites an earlier one; a requirement from another
/// column draws no link (it goes to the record's other list `+0x68..`), and a technology without
/// a same-column requirement keeps 0, 0. (Before 2026-10-07 ours passed the table's columns 6 and
/// 7, which drew a stray link 640 px long across the panel's title.)
fn technology_parent_offsets<'a>(db: &'a GameDatabase, required: &'a [(String, String)]) -> HashMap<&'a str, (i32, i32)> {
    let place = |key: &str| {
        let t = db.technology(key)?;
        let b = db.building_level(&t.building_level)?;
        Some((b.chain.as_str(), b.level, t.tree_column))
    };
    let row = |level: i32, pos: i32| level * 2 + i32::from(pos > 3);
    let mut out = HashMap::new();
    for (t, r) in required {
        let (Some((t_chain, t_level, t_pos)), Some((r_chain, r_level, r_pos))) = (place(t), place(r)) else { continue };
        if t_chain == r_chain {
            out.insert(t.as_str(), (r_pos % 4 - t_pos % 4, row(t_level, t_pos) - row(r_level, r_pos)));
        }
    }
    out
}

/// The technology screen's data (`TechnologyPlayerDetails`, 0x009F9590 → 0x009C5630 →
/// 0x0099A5C0: 0x0099A550 "technologies" + 0x009A5F40 layout + 0x009A71A0 universities; CONFIRMED
/// key names). `{layout = {[category] = {[chain] = {[level] = {tech_entries = {[1..8] = entry},
/// constructed, constructable, building_icon_name, building_name, building_key, faction_key,
/// slots = {{key, address}...}}}}}, faction_key, universities = {...}, starting_university_index}`.
///
/// The tree (CONFIRMED shape): three categories (the screen's tabs: military, industry,
/// enlightenment), each a list of building chains (the layout's columns Civil, Military,
/// Industrial: sAdmin, sArmy, tFactory, the chains that hold technologies), each chain its levels (level < 6, buildable by
/// the faction) in level order, and each level eight positions (a technology's `tree_column`)
/// holding the technologies researched at that building level (`technologies.building_level`).
/// A tech entry (0x009ABB50): Key, Name, BuildingLevel, ChainPosition, PointsRequired,
/// ParentXoffset, ParentYoffset, IconFilename ("Data/UI/Campaign UI/Technologies/<text key>.tga"),
/// LongDescription, ShortDescription, Category, Record, dependancies = {[i] = entry + Status},
/// tech_status (Utilities.lua TECHNOLOGY_STATUS_*: 0 researched, 1 being researched, 2
/// available, 3 available to steal or trade, 4 unavailable, 5 not present).
/// INFERRED / PROVISIONAL: a technology's category (tab) is its key prefix (military / economy /
/// admin), every tab lists all three chains; status: researched when the
/// faction's technology list has it with state 0, being researched with state 1, otherwise
/// available when every required technology is researched, else unavailable (the exe's own test
/// 0x008B7850 is not decoded); research itself is not modelled, so no university is researching.
fn technology_details(lua: &Lua, inner: &Inner, ui: &CampaignUi) -> mlua::Result<Value> {
    let db = &ui.link.db;
    let m = ui.model();
    let Some(f) = m.faction_by_key(&ui.link.human) else { return Ok(Value::Nil) };
    let (fid, faction_key) = (f.id, f.key.clone());
    let known: HashMap<String, u32> =
        m.world.faction_details.get(&fid).map(|d| d.technologies.iter().cloned().collect()).unwrap_or_default();
    // Required technologies (`technology_required_technology_junctions`: technology, required).
    let required: Vec<(String, String)> =
        small_table(inner, "db/technology_required_technology_junctions_tables/technology_required_technology_junctions", "s,s")
            .into_iter()
            .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(1)?.as_str()?.to_owned())))
            .collect();
    let status = |key: &str| -> i32 {
        match known.get(key) {
            Some(0) => 0,
            Some(1) => 1,
            _ => {
                let deps_done = required.iter().filter(|(t, _)| t == key).all(|(_, r)| known.get(r) == Some(&0));
                if deps_done { 2 } else { 4 }
            }
        }
    };
    // Buildings standing per level key (constructed) and where.
    let mut standing: HashMap<String, Vec<(String, Value)>> = HashMap::new();
    for r in m.world.regions.values().filter(|r| r.owner == fid) {
        for (i, s) in r.slots.iter().enumerate() {
            if let Some(b) = &s.building {
                standing.entry(b.level_key.clone()).or_default().push((s.key.clone(), slot_value(ui, r.id, SlotRef::Slot(i))));
            }
        }
    }
    drop(m);
    // A technology's tab: its key's prefix (military / economy / admin; INFERRED).
    let tech_category = |key: &str| -> usize {
        match key.split(|c: char| c.is_ascii_digit() || c == '_').next() {
            Some("military") => 0,
            Some("economy") => 1,
            _ => 2,
        }
    };
    let mut chains: Vec<String> = db.technologies.rows().iter().filter_map(|t| db.building_level(&t.building_level).map(|b| b.chain.clone())).collect();
    chains.sort();
    chains.dedup();
    let parent_offsets = technology_parent_offsets(db, &required);
    let entry = |t: &ntw_data::Technology, with_deps: bool| -> mlua::Result<Table> {
        let (parent_x, parent_y) = parent_offsets.get(t.key.as_str()).copied().unwrap_or((0, 0));
        let e = lua.create_table()?;
        e.set("Key", t.key.as_str())?;
        e.set("Name", loc(inner, &format!("technologies_onscreen_name_{}", t.key)).unwrap_or_else(|| t.key.clone()))?;
        e.set("BuildingLevel", t.building_level.as_str())?;
        e.set("ChainPosition", t.tree_column)?;
        e.set("PointsRequired", t.research_cost)?;
        e.set("ParentXoffset", parent_x)?;
        e.set("ParentYoffset", parent_y)?;
        e.set("IconFilename", format!("Data/UI/Campaign UI/Technologies/{}.tga", t.text_key))?;
        e.set("LongDescription", loc(inner, &format!("technologies_long_description_{}", t.key)).unwrap_or_default())?;
        e.set("ShortDescription", loc(inner, &format!("technologies_short_description_{}", t.key)).unwrap_or_default())?;
        e.set("Category", tech_category(&t.key))?;
        if with_deps {
            let deps = lua.create_table()?;
            for (i, (_, r)) in required.iter().filter(|(k, _)| *k == t.key).enumerate() {
                if let Some(rt) = db.technology(r) {
                    let d = lua.create_table()?;
                    d.set("Key", rt.key.as_str())?;
                    d.set("Name", loc(inner, &format!("technologies_onscreen_name_{}", rt.key)).unwrap_or_else(|| rt.key.clone()))?;
                    d.set("Status", status(&rt.key))?;
                    deps.set(i + 1, d)?;
                }
            }
            e.set("dependancies", deps)?;
        }
        Ok(e)
    };
    let layout = lua.create_table()?;
    for cat in 0..3 {
        let ct = lua.create_table()?;
        for (ci, chain) in chains.iter().enumerate() {
            let chain_t = lua.create_table()?;
            let mut levels: Vec<&ntw_data::BuildingLevel> = db.building_levels.rows().iter().filter(|b| &b.chain == chain && b.level < 6).collect();
            levels.sort_by_key(|b| b.level);
            for (li, b) in levels.iter().enumerate() {
                let lt = lua.create_table()?;
                let techs = lua.create_table()?;
                let (mut any_open, mut any_unavailable) = (false, false);
                for pos in 0..8 {
                    let te = match db.technologies.rows().iter().find(|t| t.building_level == b.key && t.tree_column == pos && tech_category(&t.key) == cat) {
                        Some(t) => {
                            let s = status(&t.key);
                            any_open |= s <= 3;
                            any_unavailable |= s == 4;
                            let e = entry(t, true)?;
                            e.set("tech_status", s)?;
                            e
                        }
                        None => {
                            let e = lua.create_table()?;
                            e.set("tech_status", 5)?;
                            e
                        }
                    };
                    techs.set(pos + 1, te)?;
                }
                lt.set("tech_entries", techs)?;
                let built = standing.get(&b.key);
                lt.set("constructed", built.is_some() || (any_open && !any_unavailable))?;
                lt.set("constructable", built.is_none())?;
                let icon = ui.building_icon(&faction_key, &b.key);
                let icon_name = icon.rsplit('/').next().unwrap_or("").trim_end_matches(".tga").to_owned();
                lt.set("building_icon_name", icon_name)?;
                lt.set("building_name", building_texts(inner, ui, &b.key).0)?;
                lt.set("building_key", b.key.as_str())?;
                lt.set("faction_key", faction_key.as_str())?;
                let slots = lua.create_table()?;
                for (si, (key, address)) in built.map(|v| v.as_slice()).unwrap_or_default().iter().enumerate() {
                    let s = lua.create_table()?;
                    s.set("key", key.as_str())?;
                    s.set("address", address.clone())?;
                    slots.set(si + 1, s)?;
                }
                lt.set("slots", slots)?;
                chain_t.set(li + 1, lt)?;
            }
            ct.set(ci + 1, chain_t)?;
        }
        layout.set(cat + 1, ct)?;
    }
    let t = lua.create_table()?;
    t.set("layout", layout)?;
    t.set("faction_key", faction_key.as_str())?;
    t.set("universities", lua.create_table()?)?;
    t.set("starting_university_index", 1)?;
    Ok(Value::Table(t))
}

/// The region and slot of a construction panel `slot_key`: a `REGION_SLOT` key, or the walls' /
/// road's own key ([`fortification_slot_key`], [`road_slot_key`]).
fn slot_by_key(ui: &CampaignUi, key: &str) -> Option<(RegionId, SlotRef)> {
    let m = ui.model();
    // The walls' / road's own keys ([`fortification_slot_key`], [`road_slot_key`]) name the region.
    let walls = key.strip_prefix(WALLS_KEY_PREFIX);
    let road = key.strip_prefix(ROAD_KEY_PREFIX);
    m.world.regions.values().find_map(|r| {
        if walls == Some(r.key.as_str()) {
            Some((r.id, SlotRef::Walls))
        } else if road == Some(r.key.as_str()) {
            Some((r.id, SlotRef::Road))
        } else {
            r.slots.iter().position(|s| s.key == key).map(|i| (r.id, SlotRef::Slot(i)))
        }
    })
}

// The CampaignUI table.

fn install_functions(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f {
        ($name:literal, |$lua:ident, $inner:ident, $ui:ident, $args:tt : $ty:ty| $body:expr) => {{
            let $inner = inner.clone();
            let $ui = ui.clone();
            // The arguments are converted here, not by mlua, so a wrong-typed argument names the
            // binding in the error ("CampaignUI.X: error converting Lua boolean to String").
            t.raw_set($name, lua.create_function(move |$lua, mv: mlua::MultiValue| {
                let $args: $ty = <$ty as mlua::FromLuaMulti>::from_lua_multi(mv, $lua)
                    .map_err(|e| mlua::Error::runtime(format!("CampaignUI.{}: {e}", $name)))?;
                #[allow(unused_variables)]
                let (_, _) = (&$inner, &$ui);
                $body
            })?)?;
        }};
    }

    // ScreenSize() → width, height, each at least 1280 x 960 (CONFIRMED clamp, 0x009F4B90).
    f!("ScreenSize", |_l, inner, ui, _a: Variadic<Value>| {
        let (w, h) = inner.screen();
        Ok((w.max(1280.0), h.max(960.0)))
    });
    // PlayerFactionId(): "the db key of the players faction" (CONFIRMED description).
    f!("PlayerFactionId", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.link.human.clone()));
    f!("PlayersFactionKey", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.link.human.clone()));
    f!("CampaignKey", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.link.campaign.clone()));
    f!("FactionDetails", |lua, inner, ui, key: Option<String>| {
        let key = key.unwrap_or_else(|| ui.link.human.clone());
        faction_details(lua, &inner, &ui, &key)
    });
    // IsPlayersTurn(): true while the human faction is inside its turn.
    f!("IsPlayersTurn", |_l, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
        Ok(m.turn.in_turn && m.turn.current == human || !m.turn.started)
    });
    f!("PlayerInControl", |_l, inner, ui, _a: Variadic<Value>| Ok(true));
    f!("CurrentFactionIsHuman", |_l, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        Ok(m.turn.current.is_none_or(|c| m.turn.humans.contains(&c)))
    });
    f!("IsMultiplayer", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("IsTimedMultiplayerGame", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("IsTimedMultiplayer", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("CampaignIsEpisodic", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("DisplayingTurns", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    f!("CurrentTurn", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.model().calendar.turn_number()));
    f!("CurrentYear", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.model().calendar.date.year));
    // CurrentSeasonString() → the season's on-screen name (0x009E1F70 → 0x008ED110, CONFIRMED:
    // the date's season code 0..3 picks season_summer / _winter / _spring / _autumn, whose
    // `seasons` record names it; loc `seasons_onscreen_<key>`).
    f!("CurrentSeasonString", |_l, inner, ui, _a: Variadic<Value>| {
        const KEYS: [&str; 4] = ["season_summer", "season_winter", "season_spring", "season_autumn"];
        let code = ui.model().calendar.date.season as usize;
        Ok(KEYS.get(code).and_then(|k| loc(&inner, &format!("seasons_onscreen_{k}"))).unwrap_or_default())
    });
    // Time(): "the current models time in seconds", CONFIRMED `0x009F9B80` → `0x00A0EB80`: a u32
    // ms counter × 0.001 -- the same getter whose value (× 1000) the campaign UI frame hands to
    // `OnUpdatePulse` (see `UiScriptHost::pulse`). So ours is the pulse clock in seconds.
    // template.BuildingFrame.lua times its drop-down from Time() against the pulse argument; a
    // separate wall clock (bug 2026-10-07) delayed it by the gap (0.4 s at start, growing with
    // every end turn: seconds after a few turns). UNKNOWN: what advances the exe's counter (the
    // object at manager `+0x9FC`, field `+0x90`). PROVISIONAL: ours runs on real time (the campaign
    // HUD's clock), never pauses, and stands still between pulses (a harness that pulses rarely sees
    // it so). The result is a 32-bit float as in the exe (`(float)ms * 0.001f`).
    f!("Time", |_l, inner, ui, _a: Variadic<Value>| Ok(inner.ui_time_ms.get() as f32 * 0.001_f32));
    // WindowsTime(): "the current windows time in seconds", CONFIRMED `0x009FB0B0`: whole seconds,
    // `(int)(timeGetTime() * 0.001)` pushed as an integer. Ours counts from the HUD's start
    // instead of the system's (scripts only take differences).
    f!("WindowsTime", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.started.elapsed().as_secs() as i64));
    // LocalisationString(key): random_localisation_strings (CONFIRMED description).
    f!("LocalisationString", |_l, inner, ui, key: String| {
        Ok(loc(&inner, &format!("random_localisation_strings_string_{key}")).or_else(|| loc(&inner, &key)))
    });
    f!("UILocalisationString", |_l, inner, ui, key: String| Ok(loc(&inner, &key)));
    // FormatString(format, s): CONFIRMED description; INFERRED: the first "%S"/"%s"/"%d" is
    // replaced.
    f!("FormatString", |_l, inner, ui, (fmt, s): (String, Value)| {
        let s = match s {
            Value::String(s) => s.to_string_lossy(),
            Value::Integer(i) => i.to_string(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        };
        for pat in ["%S", "%s", "%d", "%i"] {
            if let Some(i) = fmt.find(pat) {
                return Ok(format!("{}{}{}", &fmt[..i], s, &fmt[i + 2..]));
            }
        }
        Ok(fmt)
    });
    f!("EndTurn", |_l, inner, ui, _a: Variadic<Value>| {
        ui.push(CampaignRequest::EndTurn);
        Ok(())
    });
    f!("CanEndTurn", |_l, inner, ui, _a: Variadic<Value>| Ok(true));
    f!("CanSave", |_l, inner, ui, _a: Variadic<Value>| Ok(true));
    f!("PlayerHasFunds", |_l, inner, ui, amount: Option<f64>| {
        let m = ui.model();
        Ok(m.faction_by_key(&ui.link.human).is_some_and(|f| f64::from(f.treasury) >= amount.unwrap_or(0.0)))
    });
    // AttachRadarView(component): the component the engine draws the camera's view outline on
    // (0x009DD050 → 0x009C1AB0, CONFIRMED: stores it); UpdateRadarView({map_dimensions = {w, h},
    // theatre_offset = {x, y}, theatre_dimensions = {w, h}}) (0x009FAE60 → 0x0099BE80, CONFIRMED
    // keys): how map units map onto it. The outline itself is the four screen corners of the
    // camera projected onto the ground (0x00A27D50, CONFIRMED), see `campaign_radar_outline`.
    f!("AttachRadarView", |_l, inner, ui, c: Value| {
        ui.radar_view.borrow_mut().0 = super::host::node_of(&c);
        Ok(())
    });
    f!("UpdateRadarView", |_l, inner, ui, t: Option<Table>| {
        let Some(t) = t else { return Ok(()) };
        let pair = |k: &str, a: &str, b: &str| -> Option<(f32, f32)> {
            let s: Table = t.get(k).ok()?;
            Some((s.get(a).ok()?, s.get(b).ok()?))
        };
        if let (Some(map), Some(off), Some(dim)) =
            (pair("map_dimensions", "w", "h"), pair("theatre_offset", "x", "y"), pair("theatre_dimensions", "w", "h"))
        {
            ui.radar_view.borrow_mut().1 = Some((map, off, dim));
        }
        Ok(())
    });
    // Debug and engine-side notifications with nothing to do in our engine.
    for name in [
        "DebugViewLuaComponentPtr",
        "TriggerPanelOpenEvent",
        "TriggerPanelClosedEvent",
        "TriggerAdviceForPanel",
        "TriggerTooltipAdvice",
        "TriggerUnitSelectedEvent",
        "TriggerMessageOpenedEvent",
        "TriggerMessageDropEvent",
        "EnableShortcutHandler",
        "PauseCampaign",
        "StopCamera",
        "InformAdviceReachedRender",
        "ClearSecondarySelectionContext",
    ] {
        t.raw_set(name, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    }
    // IsMergingUnit(unit) → true while the unit is part of a merge the player is setting up
    // (0x009EC670 → 0x00A0B8F0, CONFIRMED bool result; meaning INFERRED from the name). Unit
    // merging is not in our HUD yet (PROVISIONAL), so false.
    f!("IsMergingUnit", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    // TechnologyPlayerDetails(entity) → see `technology_details`.
    f!("TechnologyPlayerDetails", |lua, inner, ui, _a: Variadic<Value>| technology_details(lua, &inner, &ui));
    f!("UnitSelectionChanged", |_l, inner, ui, _a: Variadic<Value>| Ok(()));
    // ReviewPanelTabSelectionSet_1_Indexed(i): the player clicked review-panel tab i (1-based).
    // The exe (handler `0x009F48E0` → `0x009C14B0` → `0x00A20620`, CONFIRMED) checks i against the
    // CURRENT selection's tab list and does nothing (returns 0, no message) for an index out of
    // range; in range it clears the review panel, tells the old tab it is deselected, makes i
    // current and generates it, all inside the call. Ours: the same, see [`select_tab`]. The exe
    // also refuses a tab whose state (+0x24, the state `0x009C7D80` stores through the tab's +4
    // sub-object) is 2 = selected, i.e. the tab already current: no rebuild (CONFIRMED); ours
    // refuses the current tab the same way. State 0 is refused too (meaning UNKNOWN; our tabs
    // never have it).
    //
    // While a selection change builds its tab list the exe holds no tab set (manager +0xEEC is
    // 0) and the request handler (`0x009C14B0`, ECX = [manager+0xEEC], no null check) passes
    // that NULL to `0x00A20620`, which reads +0x14 of it: an access violation that Lua's
    // setjmp/longjmp protection does not catch (CONFIRMED static; whether an outer SEH handler
    // does is UNKNOWN; no shipped script asks then). That covers the generators the build runs
    // too: the kept tab's opening (`0x009C97B0`) and `OpenFirstEnabledPanelTab` (`0x009DA2A0`)
    // run inside the tab set's ctor (`ConstructSettlementPanelTabs 0x0099A200`), which returns
    // before `HandleSettlementSelected` stores the set at +0xEEC, and the Lua handler
    // (`0x009F48E0`) hands the manager global straight to `0x009C14B0` (CONFIRMED, round 16).
    // Ours refuses the request and logs it once per HUD (round 15).
    f!("ReviewPanelTabSelectionSet_1_Indexed", |lua, inner, ui, i: Option<usize>| {
        if ui.building_tabs.get() {
            if !ui.refused_tab_request_logged.replace(true) {
                log(
                    &inner,
                    format!(
                        "ERROR ReviewPanelTabSelectionSet_1_Indexed({i:?}) while a selection change builds the tab list: \
                         the original reads a null tab set here (0x009C14B0 -> 0x00A20620); refused (logged once)"
                    ),
                );
            }
            return Ok(());
        }
        if let Some(i) = i
            && i >= 1
            && i <= ui.tabs.borrow().len()
            && i != ui.current_tab.get()
        {
            state_function_guard(|| select_tab(lua, &inner, &ui, i))?;
        }
        Ok(())
    });
    // ReviewPanelInfo(): the current army/navy panel's info, for a soft refresh (Army.lua).
    f!("ReviewPanelInfo", |lua, inner, ui, _a: Variadic<Value>| {
        match selected_force(&ui) {
            Some(f) => force_info(lua, &inner, &ui, f),
            None => Ok(Value::Nil),
        }
    });
    // Theatres. TheatreList(no_sea_trade) → one entry per theatre of the campaign map (0x009F96C0
    // → 0x009B2080 → 0x009ACE50, CONFIRMED): Address, Id (the `campaign_map_playable_areas` row's
    // area column, e.g. "europe_main"), Name, Key (the theatre key = the row's key, e.g.
    // "1244818741", also the theatre's name in regions.esf), SeaTrade (the row's bool column; with
    // `true` those theatres are left out). Name: loc
    // `campaign_map_playable_areas_onscreen_name_<key>` (the exe reads a theatre string, INFERRED
    // to be that loc). HomeTheatre(faction) → the theatre key (CONFIRMED 0x009E5180).
    // GovernorshipList(faction) → {Name, Key, TheatreKey} (CONFIRMED names 0x009E4D50).
    // PROVISIONAL: the campaign's theatre is found from the campaign key (`theatre_of`); each
    // Napoleon map has one.
    // RetrieveExistingTreaties(a, b) → see `existing_treaties`.
    f!("RetrieveExistingTreaties", |_l, inner, ui, (a, b): (String, String)| Ok(existing_treaties(&inner, &ui, &a, &b)));
    // RetrieveDiplomaticOpinions(a, b) → two attitude texts (0x009F2830, CONFIRMED: two
    // `relationship_<level>` strings, one per direction; order INFERRED: b's opinion of a, then
    // a's opinion of b).
    f!("RetrieveDiplomaticOpinions", |_l, inner, ui, (a, b): (String, String)| {
        Ok((attitude_text(&inner, &ui, &b, &a), attitude_text(&inner, &ui, &a, &b)))
    });
    // RetrieveRemainingMilitaryAccessTurns(a, b) → two numbers, one per direction: the
    // relationship's military access turns (+0x78C), or -1 when the stance is 4 (protector)
    // (0x009F4380, CONFIRMED; order INFERRED as b → a, then a → b; our Patron stance = 4).
    f!("RetrieveRemainingMilitaryAccessTurns", |_l, inner, ui, (a, b): (String, String)| {
        let m = ui.model();
        let (Some(fa), Some(fb)) = (m.faction_by_key(&a), m.faction_by_key(&b)) else { return Ok((None, None)) };
        let turns = |x: &ntw_sim::campaign::Faction, y: &ntw_sim::campaign::Faction| {
            if x.diplomacy.get(&y.id) == Some(&ntw_sim::campaign::Stance::Patron) {
                -1
            } else {
                m.world.relationships.get(&(x.id, y.id)).map_or(0, |r| r.military_access_turns)
            }
        };
        Ok((Some(turns(fb, fa)), Some(turns(fa, fb))))
    });
    // RetrieveDiplomaticStanceString(a, b): the stance string for a relationship, CONFIRMED one call
    // site and UNKNOWN shape (no argument reads, no literal string in the wrapper). The value here
    // is INFERRED: the stance's debug name, and the loc key is left alone because the wrapper gives
    // no key to build one from.
    f!("RetrieveDiplomaticStanceString", |_l, inner, ui, (a, b): (String, String)| {
        let m = ui.model();
        let (Some(fa), Some(fb)) = (m.faction_by_key(&a), m.faction_by_key(&b)) else { return Ok(String::new()) };
        let stance = fa.diplomacy.get(&fb.id).copied().unwrap_or(ntw_sim::campaign::Stance::Neutral);
        Ok(format!("{stance:?}").to_lowercase())
    });
    // InviteAlliesIntoWar(a, b): CONFIRMED one call site, UNKNOWN shape. PLACEHOLDER: the model has
    // no "call the allies into the war" command (the open end of `call_allies`, UI_FIDELITY.md §4
    // open item 4), so this only reports whether there is anyone to call (INFERRED) and never
    // invites anybody.
    f!("InviteAlliesIntoWar", |_l, inner, ui, (a, b): (String, String)| {
        let m = ui.model();
        let (Some(fa), Some(fb)) = (m.faction_by_key(&a), m.faction_by_key(&b)) else { return Ok(false) };
        let has_allies = m.world.factions.values().any(|f| {
            f.id != fa.id
                && f.id != fb.id
                && m.in_the_game(f.id)
                && matches!(
                    m.world.stance(f.id, fa.id),
                    ntw_sim::campaign::Stance::Allied
                        | ntw_sim::campaign::Stance::Patron
                        | ntw_sim::campaign::Stance::Protectorate
                )
                && m.world.stance(f.id, fb.id) != ntw_sim::campaign::Stance::War
        });
        Ok(has_allies)
    });
    // IsMultiplayerOttomansFrenchDiplomacy(): only in multiplayer (CONFIRMED description); false.
    f!("IsMultiplayerOttomansFrenchDiplomacy", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    // StateGiftValues() → {v1, v2, v3}: the `state_gift_values` values in ascending order
    // (0x009F79B0, CONFIRMED: copies the table's values, sorts them, appends each).
    f!("StateGiftValues", |lua, inner, ui, _a: Variadic<Value>| {
        let mut v: Vec<i32> =
            small_table(&inner, "db/state_gift_values_tables/state_gift_values", "s,i").iter().filter_map(|r| r.get(1)?.as_i32()).collect();
        v.sort();
        lua.create_sequence_from(v)
    });
    // IsCharacterPlayerControlled(character) → true if the character belongs to the human player's faction.
    // Called by agent panel scripts to enable/disable action buttons (CONFIRMED call in Agents.lua).
    f!("IsCharacterPlayerControlled", |_l, inner, ui, entity: Value| {
        let Some(c) = entity_of(&entity, TAG_CHARACTER).map(CharacterId) else { return Ok(false) };
        let m = ui.model();
        let Some(ch) = m.world.characters.get(&c) else { return Ok(false) };
        let human = m.faction_by_key(&ui.link.human).map(|f| f.id);
        Ok(human.is_some_and(|h| ch.faction == h))
    });
    // CharactersRelationshipToPlayersFaction(address) → 0..3 (see `relationship_to_players_faction`).
    // An address that is not a character answers nil, as before the binding (UNKNOWN in the exe).
    f!("CharactersRelationshipToPlayersFaction", |_l, _inner, ui, entity: Value| {
        let Some(c) = entity_of(&entity, TAG_CHARACTER).map(CharacterId) else { return Ok(None) };
        let m = ui.model();
        let Some(ch) = m.world.characters.get(&c) else { return Ok(None) };
        let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(None) };
        Ok(Some(relationship_to_players_faction(ch.faction == human, m.world.stance(human, ch.faction))))
    });
    // The agent panel's action calls (CHARACTER_UI_HOOKS.md "Agents panel"). CONFIRMED from
    // `ui/agents.luac`: each takes only the selected agent's Address (arity 1): the duel, assassinate,
    // subterfuge and sabotage-army buttons; `AgentEmbarkOrDisembark` only with a current card during
    // the player's turn; `AgentCardSelectionChanged` gets the card's `ItemAddress`.
    // `ShowAgents`, `ShowAgentButtons`, `AgentCardPosition`, `SelectAgentCard`,
    // `ShowStealingTechnologies` and `AgentsPanelActive` are script globals, not `CampaignUI.*`
    // calls, so they are not bound.
    //
    // The three that have a target list now open it (see [`open_agent_action_popup`]): the click
    // reaches the picker, the picker reaches `Instigate*`, and that queues the model's command.
    // `AgentCardSelectionChanged` is a card-group callback, not an action, and
    // `AgentEmbarkOrDisembark` needs an agent embark the model does not have -- both stay no-ops.
    for (name, action) in [
        ("AgentGentlemanDuel", "duel"),
        ("AgentRakeAssassinate", "assassinate"),
        ("AgentRakeSubterfuge", "sabotage"),
    ] {
        let ui2 = ui.clone();
        let inner2 = inner.clone();
        t.raw_set(
            name,
            lua.create_function(move |lua, agent: Value| {
                open_agent_action_popup(lua, &inner2, &ui2, &agent, action);
                Ok(())
            })?,
        )?;
    }
    for name in ["AgentCardSelectionChanged", "AgentEmbarkOrDisembark"] {
        t.raw_set(name, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    }
    // AgentRogueSabotageArmy(agent): the harass / sabotage-army button. STILL A NO-OP, and now for a
    // named reason: its only target is an **army**, and unlike the three above there is no
    // `Request*Targets` list for one -- `SabotageArmy(agent, force)` is called straight from the
    // popup button (`agent_options.lua:159`), so the original's exe picked the force itself. With no
    // force list to show, opening a picker we cannot fill would be a worse answer than a logged gap.
    t.raw_set("AgentRogueSabotageArmy", {
        let inner2 = inner.clone();
        lua.create_function(move |_, _: Variadic<Value>| {
            log(&inner2, "agent action sabotage_army: no army target list (named open item)".to_owned());
            Ok(())
        })?
    })?;
    // CampaignUI.MoveIntoTarget(src, target [, research]) -- what five of the nine `agent_options`
    // actions end in (`agent_options.lua:139/144/149/154/164`, CONFIRMED, and no other caller).
    // **The contract is known** (0-E round 5): the exe's registration document reads
    // "In: Character (agent), character or settlement (target), bool (research - steal if enemy
    // settlement)" -- see [`AgentMenuTarget`]. What is still missing is the behaviour on arrival,
    // which is the model's agent-order machinery, so this stays a deliberate no-op that LOGS rather
    // than silently succeeding, and the five bits that reach it stay out of [`agent_options_mask`].
    t.raw_set("MoveIntoTarget", {
        let inner2 = inner.clone();
        lua.create_function(move |_, _: Variadic<Value>| {
            log(&inner2, "MoveIntoTarget is not implemented (named open item)".to_owned());
            Ok(())
        })?
    })?;
    // CanAgentEmbarkOrDisembark(agent) -> false. Arity 1 CONFIRMED; asked only in a port residence.
    // PROVISIONAL answer: the model has no agent embark (only a force's `Embark` / `Disembark`).
    t.raw_set("CanAgentEmbarkOrDisembark", lua.create_function(|_, _: Variadic<Value>| Ok(false))?)?;
    // CampaignUI.__CanHarrass(agent): the answer `agent_options.Initialise` gets from
    // `CampaignCharacter(m_src):CanHarrass()` (pc 92-96, arity 0 CONFIRMED) to choose the Sabotage
    // Army button's state. The same two questions the agents panel asks for that button
    // (`agents.lua:77-108`): the ability, then a valid target. INFERRED: that the engine's
    // `CanHarrass` asks exactly those two is our reading -- the method is the exe's and is not traced.
    f!("__CanHarrass", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| agent_has_ability(&m, c, "can_sabotage_army") && valid_sabotage_army_target(&m, c, own)))
    });
    // The engine's entry to the agent action menu: `root:LuaCall("OpenAgentOptionsPopup", src,
    // target, mask, pct, pct)` -- `layout.root.lua:1191` (arity 5 CONFIRMED) forwards it to
    // `panel_manager.OpenPanel("agent_options", nil, "Initialise", ...)`, which is what the original's
    // exe called when the player acts on a target with an agent. Ours is the same call with the mask
    // [`agent_options_mask`] computes and the two percentages [`agent_options_percentages`]
    // computes. On `CampaignUI` with a `__` prefix because the engine half is a host call:
    // `UiScriptHost::agent_options_popup` goes through this binding, so the host and the scripts
    // cannot drift apart.
    f!("__OpenAgentOptionsPopup", |l, inner, ui, (src, target): (Value, Value)| {
        let m = ui.model();
        let Some((c, own)) = agent_of(&m, &src) else { return Ok(false) };
        let mask = agent_options_mask(&m, c, own);
        let (infiltrate, sabotage_army) = agent_options_percentages(&m, c, own, menu_target_of(&m, &target));
        Ok(call_root_global(l, &inner, "OpenAgentOptionsPopup", (src, target, mask, infiltrate, sabotage_army)))
    });
    // The agent action popups (`agent_options.luac` / `agent_action.luac`, CONFIRMED on the install
    // 2026-10-05). The agents panel's three buttons open the target picker above; these are the calls
    // that picker makes, and they are wired to the model's commands:
    //   RequestDuelTargets(agent, target) / RequestAssassinationTargets / RequestSabotageTargets
    //     -> a list of targets (each with `target` and `Faction.FlagPath`, CONFIRMED fields);
    //        empty when there is none, and the popup is then not opened (`#targets > 0`);
    //   InstigateDuel(agent, target) / InstigateAssassination / InstigateSabotage -> the model's
    //     `Duel` / `Assassinate` / `SabotageBuilding` (the target of a duel or an assassination is
    //     the row's `target`, a character's address; a sabotage target is a slot's);
    //   SabotageArmy(agent, force) -> the model's `SabotageArmy`.
    // The second argument of the Request* calls is the interaction's other party, which
    // `agent_options.lua:34` shows to be a character address; INFERRED: the currently selected
    // entity, and it is not needed for the candidate lists (the model's own gates decide).
    for (name, action) in [("RequestDuelTargets", "duel"), ("RequestAssassinationTargets", "assassinate"), ("RequestSabotageTargets", "sabotage")] {
        let ui2 = ui.clone();
        let inner2 = inner.clone();
        t.raw_set(
            name,
            lua.create_function(move |lua, (agent, _target): (Value, Value)| {
                let m = ui2.model();
                let rows = agent_of(&m, &agent).map(|(c, own)| target_candidates(&ui2, &inner2, action, c, own)).unwrap_or_default();
                target_rows(lua, &inner2, &ui2, &rows)
            })?,
        )?;
    }
    // InstigateDuel / InstigateAssassination / InstigateSabotage(agent, target) and
    // SabotageArmy(agent, force): the player's own action, queued as the model's command. A duel
    // target is a character's address (the popup's `character_duel_info_pane` row), a sabotage
    // target a slot's (`sabotage_entry` row), CONFIRMED (`agent_action.lua:21`). The weapon of a
    // duel is the model's AI rule (PROVISIONAL, CHARACTER_UI_HOOKS.md H3).
    for name in ["InstigateDuel", "InstigateAssassination", "InstigateSabotage", "SabotageArmy"] {
        let ui2 = ui.clone();
        t.raw_set(
            name,
            lua.create_function(move |_, (agent, target): (Value, Value)| {
                if let Some(c) = agent_action_command(&ui2, name, &agent, &target) {
                    ui2.push(CampaignRequest::Command(c));
                }
                Ok(())
            })?,
        )?;
    }
    // The commander pool (the army / navy panel's Promote button, `army.lua:582` ->
    // `enlist_commander.lua`, CONFIRMED on the install 2026-10-05). CanRecruitCommander(force,
    // is_navy) is asked with the force and the panel's kind (`army.lua:708`); the second argument
    // of both calls is redundant for us (the force knows whether it is a navy).
    f!("CanRecruitCommander", |_l, _inner, ui, (force, _is_navy): (Value, Value)| {
        let m = ui.model();
        Ok(entity_of(&force, TAG_FORCE).map(|f| m.can_recruit_commander(ForceId(f as u32))).unwrap_or(false))
    });
    f!("AvailableCommandersForRecruitment", |lua, inner, ui, (force, _is_navy): (Value, Option<bool>)| {
        let Some(f) = entity_of(&force, TAG_FORCE).map(|f| ForceId(f as u32)) else { return Ok(Value::Nil) };
        commanders_for_recruitment(lua, &inner, &ui, f)
    });
    // PromoteUnits(force, commander): what the enlist-commander panel calls when the player confirms
    // a candidate (`enlist_commander.lua:106`, CONFIRMED arity 2) -- the original's name for hiring
    // a pool candidate into a force. The model: `HireGeneral { into }` for an army,
    // `HireAdmiral { fleet }` for a navy (CONFIRMED).
    f!("PromoteUnits", |_l, _inner, ui, (force, commander): (Value, Value)| {
        let cmd = {
            let m = ui.model();
            match (entity_of(&force, TAG_FORCE).map(|f| ForceId(f as u32)), entity_of(&commander, TAG_CHARACTER).map(CharacterId)) {
                (Some(f), Some(c)) => match m.world.forces.get(&f).map(|x| x.is_navy) {
                    Some(true) => Some(CampaignCommand::HireAdmiral { character: c, fleet: f }),
                    Some(false) => Some(CampaignCommand::HireGeneral { character: c, into: Some(f) }),
                    None => None,
                },
                _ => None,
            }
        };
        if let Some(c) = cmd {
            ui.push(CampaignRequest::Command(c));
        }
        Ok(())
    });
    // CanPromoteUnit(unit card address): the exe's gate for the field promotion of a unit's commander
    // (`0x009E0AF0`, the unit's slot 16; arity 1 CONFIRMED from `army.lua:692`, which asks it with
    // the card's `ItemAddress` only when the unit is not a General or admiral). Answered from the
    // model's own conditions ([`CampaignModel::can_promote_unit`]).
    f!("CanPromoteUnit", |_l, _inner, ui, unit: Value| {
        let m = ui.model();
        let Some(u) = entity_of(&unit, TAG_UNIT).map(UnitId) else { return Ok(false) };
        Ok(m.world.forces.values().any(|f| m.can_promote_unit(f.id, f.units.iter().position(|x| x.id == u).unwrap_or(usize::MAX))))
    });
    // SpyingDataLevelCharacter(address) / SpyingDataLevelUnit(address): what the player may know about
    // a character or a unit (the root's double-click handler opens the details only from
    // `SPYING_DATA_LEVEL_ADVANCED` up, `layout.root.lua:1093`; arity 1 CONFIRMED). The levels are the
    // model's sight and knowledge (see `spying_level_character`).
    f!("SpyingDataLevelCharacter", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(LEVEL_INVALID) };
        Ok(entity_of(&v, TAG_CHARACTER).map_or(LEVEL_INVALID, |c| spying_level_character(&m, human, CharacterId(c))))
    });
    f!("SpyingDataLevelUnit", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        let Some(human) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(LEVEL_INVALID) };
        Ok(entity_of(&v, TAG_UNIT).map_or(LEVEL_INVALID, |u| spying_level_unit(&m, human, UnitId(u))))
    });
    // The questions `ShowAgentButtons` asks, each with the agent's Address (CONFIRMED names, arity and
    // use as the button's active flag). An address that is not a character answers false / nil.
    // CharacterResidence(agent) -> the residence address or nil (INFERRED from the model, see `Residence`).
    f!("CharacterResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).and_then(|(c, _)| residence_of(&m, c)).map_or(Value::Nil, |r| r.value(&ui)))
    });
    // IsCharacterInPortResidence(agent) -> his residence is a port slot (INFERRED: `RegionSlot::port`).
    f!("IsCharacterInPortResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(match agent_of(&m, &v).and_then(|(c, _)| residence_of(&m, c)) {
            Some(Residence::Slot(r, i)) => m.world.regions.get(&r).and_then(|r| r.slots.get(i)).is_some_and(|s| s.port),
            _ => false,
        })
    });
    // CharacterInEnemyResidence(agent) -> his residence is held by another faction (INFERRED: "enemy"
    // as the model's agent actions take it, any other faction; no stance test).
    f!("CharacterInEnemyResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        let Some((c, own)) = agent_of(&m, &v) else { return Ok(false) };
        Ok(residence_of(&m, c).and_then(|r| r.owner(&m)).is_some_and(|o| o != own))
    });
    // ValidAssassinationTargets(agent): model gate CONFIRMED, candidate set PROVISIONAL (whole map).
    f!("ValidAssassinationTargets", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_assassination_targets(&m, c, own)))
    });
    // ValidSabotageTarget(agent): a building in his residence (INFERRED), model gate CONFIRMED.
    f!("ValidSabotageTarget", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_sabotage_target(&m, c, own)))
    });
    // ValidSabotageArmyTarget(agent): model gate CONFIRMED, candidate set PROVISIONAL (whole map).
    f!("ValidSabotageArmyTarget", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_sabotage_army_target(&m, c, own)))
    });
    // CharacterInValidEnemyUniversity(agent): another faction's school (INFERRED meaning; the model's
    // school test CONFIRMED).
    f!("CharacterInValidEnemyUniversity", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| in_valid_enemy_university(&m, c, own)))
    });
    // ValidDuelTargetsInResidence(agent): a duel partner in the same residence (INFERRED), model
    // gate CONFIRMED.
    f!("ValidDuelTargetsInResidence", |_l, _inner, ui, v: Value| {
        let m = ui.model();
        Ok(agent_of(&m, &v).is_some_and(|(c, own)| valid_duel_targets_in_residence(&m, c, own)))
    });
    // MinisterPortraitPath(faction) → a minister's portrait for the negotiation screen (0x009ED8A0
    // picks one through the minister agent record, "random" per the description). PROVISIONAL:
    // the card picture of the faction's first character whose portrait is a minister's, else of
    // its leader; "" if none.
    f!("MinisterPortraitPath", |_l, inner, ui, key: Option<String>| {
        let m = ui.model();
        let Some(f) = m.faction_by_key(&key.unwrap_or_else(|| ui.link.human.clone())).map(|f| f.id) else { return Ok(String::new()) };
        let cards: Vec<String> = m
            .world
            .characters
            .values()
            .filter(|c| c.faction == f)
            .filter_map(|c| portrait_card(&m, c.id))
            .collect();
        let pick = cards.iter().find(|p| p.to_ascii_lowercase().contains("/minister/")).or(cards.first());
        Ok(pick.map(|p| format!("data/{p}")).unwrap_or_default())
    });
    // RetrieveDiplomacyDetails(key) is also how the host learns which pair the panel is about: the
    // panel shows one faction at a time and the player (the exe's "Proposer") opens the diplomacy
    // on it, so the read starts the negotiation with those two factions (INFERRED; the exe's own
    // open path is UI_FIDELITY.md §4 open item 1). Re-reading the same pair keeps the deal; a
    // different pair starts a new one.
    f!("RetrieveDiplomacyDetails", |lua, inner, ui, key: Option<String>| {
        let key = key.unwrap_or_else(|| ui.link.human.clone());
        let proposer = ui.link.human.clone();
        if key != proposer {
            let mut state = ui.negotiation.borrow_mut();
            let same_pair = state.proposer.as_deref() == Some(proposer.as_str())
                && state.target.as_deref() == Some(key.as_str());
            if !same_pair {
                state.open(proposer.clone(), key.clone());
            }
        }
        diplomacy_details(lua, &inner, &ui, &key)
    });
    f!("RetrieveFactionListForDiplomacy", |lua, inner, ui, _a: Variadic<Value>| faction_list_for_diplomacy(lua, &inner, &ui));
    // ===== Negotiation (`UIDiplomacyNegotiation`, userdata ctor 0x00A102B0) =====
    //
    // The panel script drives a *negotiation object*, not the CampaignUI table: the 19
    // `negotiation:*` receivers in `worker3/lua_api.txt` are calls on one object, and the exe's
    // tolua ctor 0x00A102B0 allocates a 0xB4-byte userdata named "UIDiplomacyNegotiation"
    // (CONFIRMED, own Ghidra copy) whose fields start with the counterparty faction id at +0xAC
    // (every accessor tested returns nothing while +0xAC is 0). The host therefore keeps the
    // pending deal in `ui.negotiation` and hands the script the same methods, on the
    // `negotiation` global set up by `open_negotiation` when the panel opens.
    //
    // Confidence tags below are per shape:
    //   CONFIRMED - read out of the decompiled wrapper (arg reads, pushed return values, and the
    //               literal key strings the wrapper pushes);
    //   INFERRED  - the shape is confirmed but the value comes from the model;
    //   UNKNOWN   - the wrapper's shape gave nothing, so the host returns the neutral value.

    /// The stance-declaration action names the exe compares against (CONFIRMED literal strings in
    /// `FactionListsForStanceDeclarations`, 0x009BAB90): the pending declaration filters the two
    /// faction lists it returns.
    const STANCE_DECLARATIONS: [&str; 3] = ["request_join_war", "break_trade", "break_alliance"];

    // BuildPossibleActions(): the actions the panel may offer. CONFIRMED that it fills a Lua
    // table; its entries are string constants in the wrapper's own rodata, not read out here, so
    // the list is the three CONFIRMED stance-declaration names only (UNKNOWN: the full set) and
    // each is kept only while it is legal for the pair's stance (INFERRED filter).
    f!("BuildPossibleActions", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        if let (Some(a), Some(b)) = negotiation_factions(&ui) {
            let stance = m.world.stance(a, b);
            for name in STANCE_DECLARATIONS {
                let legal = match name {
                    "break_alliance" => {
                        matches!(stance, ntw_sim::campaign::Stance::Allied | ntw_sim::campaign::Stance::Protectorate)
                    }
                    "break_trade" => m.world.relationships.get(&(a, b)).is_some_and(|r| r.trade_agreement),
                    "request_join_war" => stance != ntw_sim::campaign::Stance::War,
                    _ => true,
                };
                if legal {
                    t.set(name, name)?;
                }
            }
        }
        Ok(Value::Table(t))
    });

    // BuildOfferAndDemandStrings(): CONFIRMED result shape `{ Offers = {rows}, Demands = {rows} }`
    // (0x009B48B0 pushes the literal keys "Offers" and "Demands"); each row carries "Action" and,
    // for the region-transfer action (the id the wrapper tests == 6, INFERRED), a "Regions" list.
    // The action *ids* are UNKNOWN, so rows are keyed by the host's action name (INFERRED) and the
    // region list is filled from the deal's region items.
    f!("BuildOfferAndDemandStrings", |lua, inner, ui, _a: Variadic<Value>| {
        let state = ui.negotiation.borrow();
        let t = lua.create_table()?;
        for (key, items) in [("Offers", &state.offers), ("Demands", &state.demands)] {
            let list = lua.create_table()?;
            for (i, item) in items.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("Action", negotiation_action_name(item))?;
                if let NegotiationItem::Regions(keys) = item {
                    let regions = lua.create_table()?;
                    for (j, r) in keys.iter().enumerate() {
                        regions.set(j + 1, r.as_str())?;
                    }
                    row.set("Regions", regions)?;
                }
                list.set(i + 1, row)?;
            }
            t.set(key, list)?;
        }
        Ok(Value::Table(t))
    });

    // TradeableRegions(): CONFIRMED shape (0x009C5770)
    // `{ Proposer = {{ Region = <region>, "CurrentlyOffered" = bool }, ...},
    //    Recipient = {{ Region = <region>, "CurrentlyDemanded" = bool }, ...} }`, one entry per
    // region of that faction; the wrapper returns nothing unless the campaign flag at +0xF9C and
    // the counterparty (+0xAC) are both set (CONFIRMED guard). Region lists: the faction's own
    // regions (INFERRED).
    f!("TradeableRegions", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        let (Some(proposer), Some(recipient)) = negotiation_factions(&ui) else { return Ok(Value::Table(t)) };
        for (key, faction, offered) in [("Proposer", proposer, true), ("Recipient", recipient, false)] {
            let list = lua.create_table()?;
            for (i, region) in m.world.regions.values().filter(|r| r.owner == faction).enumerate() {
                let row = lua.create_table()?;
                row.set("Region", region.key.as_str())?;
                row.set(if offered { "CurrentlyOffered" } else { "CurrentlyDemanded" }, region_in_deal(&ui, region.key.as_str(), offered))?;
                list.set(i + 1, row)?;
            }
            t.set(key, list)?;
        }
        Ok(Value::Table(t))
    });

    // TradeableTechnologies(): CONFIRMED shape (0x009C5AA0)
    // `{ Proposer = {rows}, Recipient = {rows} }`; each row carries the literal key "FactionKey"
    // and the numeric "tech_status" (the wrapper pushes `FUN_0044de40("tech_status", 0)`); the
    // rows are built by the card helper 0x009ABB50, which also sets "BuildingLevel" and the icon
    // path `Data/UI/Campaign UI/Technologies/%S.tga`. Every technology of the faction is listed
    // with its real state (0 researched / 2 available / 4 not yet available, CONFIRMED in
    // `details.rs`), so the script can filter: the tradeable set (which side may give what) is
    // UNKNOWN. The two remaining row keys are string constants at 0x009C5B9B / 0x009C5BC0 and are
    // left out rather than invented.
    f!("TradeableTechnologies", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        let (Some(proposer), Some(recipient)) = negotiation_factions(&ui) else { return Ok(Value::Table(t)) };
        for (key, faction) in [("Proposer", proposer), ("Recipient", recipient)] {
            let list = lua.create_table()?;
            let mut i = 1;
            if let Some(details) = m.world.faction_details.get(&faction) {
                for (tech, status) in &details.technologies {
                    let row = lua.create_table()?;
                    row.set("FactionKey", tech.as_str())?;
                    row.set("tech_status", *status as i64)?;
                    list.set(i, row)?;
                    i += 1;
                }
            }
            t.set(key, list)?;
        }
        Ok(Value::Table(t))
    });

    // FactionListsForStanceDeclarations(): CONFIRMED shape (0x009BAB90) `{ offered = {...},
    // demanded = {...} }` (literal keys), filtered by the pending declaration: the wrapper
    // compares the negotiation's pending action against the CONFIRMED names "request_join_war",
    // "break_trade", "break_alliance" (INFERRED that each splits the factions by stance: at war
    // with the proposer, or allied to it).
    f!("FactionListsForStanceDeclarations", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        let (Some(proposer), Some(recipient)) = negotiation_factions(&ui) else { return Ok(Value::Table(t)) };
        let offered = lua.create_table()?;
        let demanded = lua.create_table()?;
        let mut io = 1;
        let mut id = 1;
        for faction in m.world.factions.values() {
            if faction.id == proposer || faction.id == recipient || !m.in_the_game(faction.id) {
                continue;
            }
            let at_war = m.world.stance(proposer, faction.id) == ntw_sim::campaign::Stance::War;
            let row = lua.create_table()?;
            row.set("FactionKey", faction.key.as_str())?;
            row.set("Name", faction.key.as_str())?;
            if at_war {
                offered.set(io, row)?;
                io += 1;
            } else {
                demanded.set(id, row)?;
                id += 1;
            }
        }
        t.set("offered", offered)?;
        t.set("demanded", demanded)?;
        Ok(Value::Table(t))
    });

    // MaxPlayerPaymentAllowed() / MaxOppositionPaymentAllowed(): CONFIRMED shape (0x009BE720 /
    // 0x009BE6B0): no arguments, one number out — the wrapper pushes `FUN_00BCAFE0(counterparty)`,
    // the shared treasury-cap query (22 callers, also used by the construction cost code). The
    // "opposition" cap is the *other* side of the negotiation, the player's cap the proposer's
    // (INFERRED value: the faction's treasury).
    f!("MaxPlayerPaymentAllowed", |_l, inner, ui, _a: Variadic<Value>| {
        Ok(negotiation_factions(&ui).0.and_then(|f| treasury_of(&ui, f)).unwrap_or(0))
    });
    f!("MaxOppositionPaymentAllowed", |_l, inner, ui, _a: Variadic<Value>| {
        Ok(negotiation_factions(&ui).1.and_then(|f| treasury_of(&ui, f)).unwrap_or(0))
    });

    // ProposerId(): CONFIRMED one value out, the proposer's faction (the native wraps the faction
    // userdata, FUN_008E5060; we have no userdata, so the faction key string is returned instead —
    // INFERRED). Nothing while no counterparty is set (CONFIRMED guard).
    f!("ProposerId", |_l, inner, ui, _a: Variadic<Value>| {
        Ok(negotiation_factions(&ui).0.and_then(|_| ui.negotiation.borrow().proposer.clone()))
    });

    // The deal-application entry points. `Propose` (10 call sites), `ProposeDeal` (2),
    // `AcceptOffer` (1), `End` (3), `Cancel` (2), `DeclineOffer` (2), `RemoveAction` (1),
    // `PrepareCounterOffer` (1), `CanPropose` (3), `CanThreaten` (1), `Finished` (1),
    // `IsNegotiation` (not called from Lua at all) — all UNKNOWN shape: no argument reads and no
    // pushed literals in the wrappers, so each keeps the pending deal in the host state and pushes
    // nothing but a neutral value. The one applier with a model path is `apply_deal`, which the
    // wrappers below call; see the PLACEHOLDER note there.
    f!("Propose", |_l, inner, ui, _a: Variadic<Value>| { apply_deal(&ui); Ok(()) });
    f!("ProposeDeal", |_l, inner, ui, _a: Variadic<Value>| { apply_deal(&ui); Ok(()) });
    f!("AcceptOffer", |_l, inner, ui, _a: Variadic<Value>| { apply_deal(&ui); Ok(()) });
    f!("End", |_l, inner, ui, _a: Variadic<Value>| {
        let mut state = ui.negotiation.borrow_mut();
        state.active = false;
        Ok(())
    });
    f!("Cancel", |_l, inner, ui, _a: Variadic<Value>| {
        ui.negotiation.borrow_mut().clear();
        Ok(())
    });
    f!("DeclineOffer", |_l, inner, ui, _a: Variadic<Value>| {
        ui.negotiation.borrow_mut().clear();
        Ok(())
    });
    f!("RemoveAction", |_l, inner, ui, action: Option<String>| {
        if let Some(a) = action {
            ui.negotiation.borrow_mut().possible_actions.retain(|x| x != &a);
        }
        Ok(())
    });
    // CanPropose(): the panel asks before enabling SendOffer. UNKNOWN shape; INFERRED rule: a
    // deal with at least one item on either side.
    f!("CanPropose", |_l, inner, ui, _a: Variadic<Value>| {
        let state = ui.negotiation.borrow();
        Ok(!state.offers.is_empty() || !state.demands.is_empty())
    });
    // CanThreaten(): UNKNOWN shape; INFERRED: any counterparty not already at war.
    f!("CanThreaten", |_l, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        Ok(match negotiation_factions(&ui) {
            (Some(a), Some(b)) => m.world.stance(a, b) != ntw_sim::campaign::Stance::War,
            _ => false,
        })
    });
    f!("PrepareCounterOffer", |_l, inner, ui, _a: Variadic<Value>| {
        // UNKNOWN shape; the AI's counter-offer is not modelled (no negotiation evaluation).
        Ok(false)
    });
    f!("Finished", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.negotiation.borrow().active));
    // IsNegotiation() is not called from any shipped script (`worker3/lua_api.txt` has no
    // `negotiation:IsNegotiation`); the method exists in the exe (0x009BD7C0). Registered so a
    // script that asks gets the host's answer rather than a nil call error.
    f!("IsNegotiation", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.negotiation.borrow().active));

    f!("TheatreList", |lua, inner, ui, no_sea_trade: Option<bool>| {
        let t = lua.create_table()?;
        if let Some(row) = ui.theatre(&inner)
            && !(no_sea_trade.unwrap_or(false) && row.sea_trade)
        {
            let e = lua.create_table()?;
            e.set("Address", ui.entity(TAG_THEATRE, 0))?;
            e.set("Id", row.area.as_str())?;
            e.set("Name", theatre_name(&inner, &row))?;
            e.set("Key", row.id.as_str())?;
            e.set("SeaTrade", row.sea_trade)?;
            t.set(1, e)?;
        }
        Ok(t)
    });
    // TheatreMapDimensions(theatre) → x, y, width, height of the theatre in map units (the theatre
    // bounds' min corner and size, CONFIRMED 0x009F9760; then a u32 from the theatre's record, not
    // given: UNKNOWN). Nothing for an unknown theatre.
    f!("TheatreMapDimensions", |_l, inner, ui, theatre: Option<String>| {
        let ok = match (ui.theatre(&inner), theatre) {
            (Some(row), Some(t)) => row.id == t || row.area == t,
            _ => false,
        };
        Ok(match (ok, ui.theatre_bounds(&inner)) {
            (true, Some(((x0, y0), (x1, y1)))) => Variadic::from_iter([x0, y0, x1 - x0, y1 - y0]),
            _ => Variadic::new(),
        })
    });
    // SetCameraTarget(x, y): centre the camera on a map position (the radar's map_overlay.lua sends
    // the clicked point; CONFIRMED call; a theatre key alone, from campaign_hud.lua, is ignored).
    f!("SetCameraTarget", |_l, inner, ui, (x, y): (Value, Option<f32>)| {
        if let (Value::Number(_) | Value::Integer(_), Some(y)) = (&x, y) {
            let x = match x {
                Value::Number(n) => n as f32,
                Value::Integer(i) => i as f32,
                _ => 0.0,
            };
            ui.push(CampaignRequest::CameraTo(x, y));
        }
        Ok(())
    });
    // RegionsInTheatre(theatre, faction, sub_key, mode) → {Map, Overlay, Radar, [i] = region}
    // (0x009EFC30, CONFIRMED keys and per-mode fields; called by template.map_image.lua's
    // InitRegionMap with mode = diplomacy_status 0, diplomacy_attitude_* 1, region_list 2,
    // public_order 3, ownership / radar 4).
    // * Map / Overlay / Radar: the theatre's `campaign_map_playable_areas` row's three pictures,
    //   each as "<campaign map folder>/<file>" (CONFIRMED format "%S/%S"; the folder string,
    //   "data/campaign_maps/<map>", INFERRED).
    // * one entry per region of the theatre: PaletteEntry (the lookup picture's palette index whose
    //   colour is the region's `regions` DB colour, -1 if none: CONFIRMED 0x00A97680 /
    //   0x00A9D850), Key, Name, Address, Owner (faction name), OwnerKey, TaxExempt (region +0xE4,
    //   CONFIRMED 0x00AAF270); modes 0 and 4 add OwnerRGB {r, g, b} (the owner's colour × 255,
    //   CONFIRMED; the primary colour INFERRED); mode 1, for regions the faction does not own,
    //   OwnerAttitude (the owner's attitude total towards the faction) and
    //   FactionAttitudeTowardsOwner (the faction's towards the owner) (CONFIRMED total 0x00B0DB60,
    //   direction INFERRED from the names); mode 3 adds OrderRGB {r, g, b, a} and PublicOrder.
    // PROVISIONAL: every region of the model is in the (single) theatre of a Napoleon campaign
    // map; the exe also drops regions a position test hides (0x008E0500, UNKNOWN). Mode 1's
    // OwnerStatus / *RelationshipDetails texts and mode 3's Lower / Upper detail tables are not
    // given yet; mode 3's colour is a PLACEHOLDER (the exe's 0x00A727D0 is not decoded).
    f!("RegionsInTheatre", |lua, inner, ui, (theatre, faction, _sub, mode): (Option<String>, Option<String>, Value, Option<i32>)| {
        let mode = mode.unwrap_or(4);
        let theatre = theatre.unwrap_or_else(|| theatre_of(&ui.link.campaign).0.to_owned());
        let faction = faction.unwrap_or_else(|| ui.link.human.clone());
        let out = lua.create_table()?;
        let folder = ui.map_folder(&inner);
        let row = ui.playable_area(&inner, &theatre);
        let mut lookup = None;
        if let Some(row) = &row {
            let file = |f: &Option<String>| f.as_deref().filter(|s| !s.is_empty()).map(|s| format!("{folder}/{s}"));
            if let Some(p) = file(&row.map) {
                out.set("Map", p)?;
            }
            if let Some(p) = file(&row.lookup) {
                out.set("Overlay", p.clone())?;
                lookup = Some(p);
            }
            if let Some(p) = file(&row.radar) {
                out.set("Radar", p)?;
            }
        }
        let palette = lookup.map(|p| ui.palette_index(&inner, &p)).unwrap_or_default();
        let m = ui.model();
        let me = m.faction_by_key(&faction).map(|f| f.id);
        let mut regions: Vec<_> = m.world.regions.values().collect();
        regions.sort_by_key(|r| r.id);
        for (n, r) in regions.into_iter().enumerate() {
            let e = lua.create_table()?;
            let colour = ui.link.db.region(&r.key).map(|rec| rec.colour());
            let entry = colour.and_then(|c| palette.get(&c).copied()).unwrap_or(-1);
            e.set("PaletteEntry", entry)?;
            e.set("Key", r.key.as_str())?;
            e.set("Name", region_name(&inner.loc, &r.key))?;
            e.set("Address", region_value(&ui, r.id))?;
            let owner = m.world.factions.get(&r.owner);
            let owner_key = owner.map(|f| f.key.clone()).unwrap_or_default();
            e.set("Owner", faction_name(&inner, &ui.link.db, &owner_key))?;
            e.set("OwnerKey", owner_key.as_str())?;
            e.set("TaxExempt", r.tax_exempt)?;
            if mode == 0 || mode == 4 {
                let [cr, cg, cb] = ui.link.db.faction(&owner_key).map_or([128, 128, 128], |f| f.primary_colour());
                let rgb = lua.create_table()?;
                rgb.set("r", cr)?;
                rgb.set("g", cg)?;
                rgb.set("b", cb)?;
                e.set("OwnerRGB", rgb)?;
            }
            if mode == 1
                && let Some(me) = me
                && me != r.owner
            {
                if let Some(rel) = m.world.relationships.get(&(r.owner, me)) {
                    e.set("OwnerAttitude", rel.attitude_total())?;
                }
                if let Some(rel) = m.world.relationships.get(&(me, r.owner)) {
                    e.set("FactionAttitudeTowardsOwner", rel.attitude_total())?;
                }
            }
            if mode == 3 {
                let order = ntw_sim::campaign::economy::public_order(&m, r.id).worst();
                // PLACEHOLDER colour scale: red when rioting, green when content.
                let t = ((order + 10.0) / 20.0).clamp(0.0, 1.0);
                let rgb = lua.create_table()?;
                rgb.set("r", ((1.0 - t) * 255.0).round())?;
                rgb.set("g", (t * 255.0).round())?;
                rgb.set("b", 0)?;
                rgb.set("a", 160)?;
                e.set("OrderRGB", rgb)?;
                e.set("PublicOrder", order)?;
            }
            out.set(n + 1, e)?;
        }
        Ok(out)
    });
    f!("HomeTheatre", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.theatre(&inner).map(|r| r.id)));
    // PlayerPlayingAsRevolutionaries() → the player's faction has its +0x814 flag set and not its
    // +0x81C one (0x009EED40, CONFIRMED test; the flags' meaning INFERRED from the description:
    // the player sided with the revolutionaries). PROVISIONAL: neither flag is in our model yet,
    // so false.
    f!("PlayerPlayingAsRevolutionaries", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    // PROVISIONAL governorship: one per theatre, keyed by the radar's theatre id ("europe").
    f!("GovernorshipList", |lua, inner, ui, _a: Variadic<Value>| {
        let (_, id) = theatre_of(&ui.link.campaign);
        let key = ui.theatre(&inner).map(|r| r.id).unwrap_or_default();
        let e = lua.create_table()?;
        e.set("Key", id)?;
        e.set("TheatreKey", key)?;
        e.set("Name", loc(&inner, &format!("governorships_onscreen_{id}")).unwrap_or_else(|| id.to_owned()))?;
        let t = lua.create_table()?;
        t.set(1, e)?;
        Ok(t)
    });
    // RecruitUnit(character, manager, record): queue a unit (CONFIRMED call in
    // template.RecruitmentCard.lua; the manager is the recruiting region's, the record the unit key,
    // INFERRED). CancelRecruitment(item_ptr): remove that queue item (CONFIRMED call).
    f!("RecruitUnit", |_l, inner, ui, (_c, manager, record): (Value, Value, Option<String>)| {
        if let (Some(r), Some(unit_key)) = (entity_of(&manager, TAG_REGION), record) {
            ui.push(CampaignRequest::Command(CampaignCommand::Recruit { region: RegionId(r as u32), unit_key }));
        }
        Ok(())
    });
    f!("CancelRecruitment", |_l, inner, ui, item: Value| {
        let item = entity_of(&item, TAG_QUEUE_ITEM).map(RecruitmentItemId::from_raw);
        let found = item.and_then(|i| ui.model().recruitment_item_region(i).map(|r| (r, i)));
        if let Some((region, item)) = found {
            ui.push(CampaignRequest::Command(CampaignCommand::CancelRecruitment { region, item }));
        }
        Ok(())
    });
    // BeginConstruction(building_key, slot_key) / BeginUpgrade(building_key, slot_key): build in an
    // empty slot / upgrade the standing building (CONFIRMED calls in template.BuildingFrame.lua;
    // both become the model's ConstructBuilding).
    // `template.buildingframe.luac:417` calls `BeginConstruction(building_key, slot_key)` and
    // `BeginUpgrade(building_key, slot_key)` (CONFIRMED); both become the model's ConstructBuilding.
    for name in ["BeginConstruction", "BeginUpgrade"] {
        let ui2 = ui.clone();
        t.raw_set(name, lua.create_function(move |_, a: Variadic<Value>| {
            let level = a.iter().find_map(value_str);
            if let (Some(level_key), Some((region, slot))) = (level, slot_of_args(&ui2, &a)) {
                ui2.push(CampaignRequest::Command(CampaignCommand::ConstructBuilding { region, slot, level_key }));
            }
            Ok(())
        })?)?;
    }
    // CancelConstruction(building_key, slot_key): cancel the slot's construction or repair item
    // (CONFIRMED call in template.buildingframe.luac:417, a click on a building under construction
    // or on one marked "being repaired"; handler 0x009E0F10 → 0x009B7AA0 queues the slot's cancel).
    f!("CancelConstruction", |_l, inner, ui, a: Variadic<Value>| {
        if let Some((region, slot)) = slot_of_args(&ui, &a) {
            ui.push(CampaignRequest::Command(CampaignCommand::CancelConstruction { region, slot }));
        }
        Ok(())
    });
    // UnitScaleFactor([index]) → scale, index (0x009FAB10; the same description as the front end's
    // 0x004795F0, see battle_setup.rs): the unit card tooltip multiplies a unit's men by it.
    f!("UnitScaleFactor", |_l, inner, ui, index: Option<i64>| {
        let idx = index.or_else(|| inner.prefs.borrow().get("gfx_unit_scale").and_then(|v| v.trim().parse().ok())).unwrap_or(3).clamp(0, 3) as usize;
        Ok((super::battle_setup::UNIT_SCALES[idx], idx))
    });
    // RepairBuilding(building_key, slot_key): the repair button on a damaged building
    // (Construction.lua's RepairCurrentSelection, CONFIRMED call; handler 0x009F1300) → the
    // model's RepairBuilding.
    f!("RepairBuilding", |_l, inner, ui, a: Variadic<Value>| {
        // The model refuses a road repair (`CampaignModel::can_repair`, PROVISIONAL).
        if let Some((region, slot)) = slot_of_args(&ui, &a) {
            ui.push(CampaignRequest::Command(CampaignCommand::RepairBuilding { region, slot }));
        }
        Ok(())
    });
    // CanDemolishBuilding(slot_key) -- CONFIRMED shape, from `Construction.lua:366`
    // (`construction.DemolishCurrentSelection`: it reads the frame's `slot_key` and `building_key`
    // globals with `UIComponent:GlobalExists` -- a getter, CONFIRMED in ui_prelude.lua -- and calls
    // `CampaignUI.CanDemolishBuilding(slot_key)`, comparing the answer with `true`). Handler
    // `0x009E0930` -> `0x009B7920` = slot resolve `0x009BB6E0` && not the `settlement_road` slot
    // (`0x00A8B970`) && slot free of pending work (`0x00A91FC0`, INFERRED) && a selected item
    // present -- CONFIRMED shape, 0-G round N+2. -> `CampaignModel::can_demolish`, which refuses
    // the road as the exe does.
    f!("CanDemolishBuilding", |_l, inner, ui, a: Variadic<Value>| {
        Ok(slot_of_args(&ui, &a).is_some_and(|(r, s)| ui.model().can_demolish(r, s)))
    });
    // DemolishBuilding(building_key, slot_key) -- CONFIRMED, from two call sites with different
    // argument lists: `Construction.lua:366` passes the frame's two globals, and
    // `ui/campaign ui/building_information_scripts/building_information.luac:36` passes only the
    // slot key (it picks between `DemolishBuilding` and `DemolishFort` on `g_details.slot_key`).
    // Handler `0x009E2380` -> `0x009B9590` -> queue id `0x84`, CONFIRMED, 0-G round N+2 ->
    // `CampaignCommand::DemolishBuilding` on the slot the key names.
    f!("DemolishBuilding", |_l, inner, ui, a: Variadic<Value>| {
        if let Some((region, slot)) = slot_of_args(&ui, &a) {
            ui.push(CampaignRequest::Command(CampaignCommand::DemolishBuilding { region, slot }));
        }
        Ok(())
    });
    // The map fort's actions. CONFIRMED names and, from the panel script's own bytecode, their
    // argument lists: `template.buildingframe.luac:417` (the frame's select handler, upvalue
    // `g_fort_ptr`) and `Construction.lua:366` / `:383` call
    // - `CampaignUI.UpgradeFort(g_fort_ptr)`      (frame, type UPGRADE)
    // - `CampaignUI.CancelUpgradeFort(g_fort_ptr)` (frame, type CONSTRUCTING)
    // - `CampaignUI.CancelFortRepair(g_fort_ptr)`  (frame, `being_repaired`)
    // - `CampaignUI.DemolishFort(g_fort_ptr)`      (Construction.lua:366)
    // - `CampaignUI.RepairFort(g_fort_ptr)`        (Construction.lua:383)
    // only when `fort_ptr` is set, i.e. on the map fort's own panel ([`fort_info`]); and
    // `CampaignUI.BuildFort(g_general)` from `ui/army.luac`'s `g_button_fort` (`Army.lua:382`).
    // The settlement walls are NOT built through these: they are the settlement construction
    // panel's last slot card ([`construction_info`]), built by `BeginConstruction` & co.
    // (CONFIRMED at runtime and in game, 2026-10-07; UI_FIDELITY.md fort section).
    //
    // `UpgradeFort` is `CCQ_UPGRADE_FORT` (registered by `0x00423480`), executed by `0x009389A0` ->
    // `ProcessFortUpgradeConstruction` `0x00B4E620`, whose first step asks `0x0047BA10`, a folded
    // folded function that always returns false, and returns: the shipped exe never upgrades a map fort
    // (CONFIRMED by the bytes). So ours sends nothing either.
    //
    // `BuildFort` is `CCQ_CHARACTER_BUILD_FORT` (`0x0041FF00`), executed by `0x00932670` ->
    // `0x009200F0` on the army's general: after the character's action check it asks `0x00462CB0`,
    // a constant-false stub, so it never builds and only posts the failure event (`0xCE`; success
    // would be `0xD6`). CONFIRMED by the bytes: the field fort is not buildable in the shipped game,
    // so ours does nothing too, and `force_info` reports the button as `FBS_UNABLE` (INFERRED in
    // game). PLACEHOLDER: the failure event (advisor / message) is not posted.
    //
    // RepairFort / CancelUpgradeFort / CancelFortRepair / DemolishFort: CONFIRMED (own Ghidra copy)
    // that each handler -- `0x009F1370`, `0x009E1240`, `0x009E0F80`, `0x009E23D0` -- reads one fort
    // userdata argument and sends a command carrying only it. PLACEHOLDER: no-ops -- the model's map
    // fort (`World::forts`) has no building, health or queue of its own, and no shipped startpos or
    // save holds a fort (`FORT_ARRAY` is empty in all of them), nor can one be built (above).
    f!("BuildFort", |_l, inner, ui, _general: Variadic<Value>| Ok(()));
    for name in ["UpgradeFort", "RepairFort", "CancelUpgradeFort", "CancelFortRepair", "DemolishFort"] {
        t.raw_set(name, lua.create_function(|_, _: Variadic<Value>| Ok(()))?)?;
    }
    // FortDetails(fort_ptr, building_key) / FortEffects(...): CONFIRMED call (2026-10-07,
    // `template.buildingframe.luac` proto at line 372, the frame's tooltip):
    // `FortDetails(g_fort_ptr, building_key)` / `FortEffects(g_fort_ptr, building_key)` -- the
    // card's own `building_key` -- handed to the tooltip's `InitialiseBuilding`.
    // CONFIRMED body (handler `0x009E49D0` -> `0x009B2AA0` -> `0x009AA250`, own Ghidra copy): the
    // second argument, when given and not nil, is a key; an empty key means the fort's standing
    // level (`0x00B42E40`), any other key is looked up in **all** of `building_levels` (no
    // fortification check). The table always carries the same eight keys, filled from that level:
    // `Key`, `Name`, `ShortDescription`, `LongDescription`, `IconFilename`, `InfoFilename`, `Level`,
    // `MaxLevel` -- no `region_key` / `slot_key`. The fort is the map fort (`fFort`, see
    // [`fort_info`]; its standing level is [`CampaignModel::map_fort_standing`]). With no level (ours: no fort
    // chain the owner may hold, which the exe's always-built fort entity never lacks; or an unknown
    // key, which the exe logs as "not a valid key"; or no fort addressed, e.g. a settlement selected)
    // the details stay default-constructed: INFERRED empty strings and 0, which
    // the tooltip shows without the nil-description error (`TechTreeItem_Tooltip.lua:75`).
    // INFERRED: `IconFilename` / `InfoFilename` are the culture variant's icon under
    // `data/ui/buildings/icons/` and `.../info/` (the exe's two path prefixes). A region or fort
    // address argument names the fort; otherwise the selection's ([`fort_region`]).
    f!("FortDetails", |lua, inner, ui, args: Variadic<Value>| {
        let region = fort_region(&ui, &args);
        let m = ui.model();
        // No fort addressed (no selection, no fort argument): still the eight default keys, never
        // nil -- the tooltip indexes the table unconditionally.
        let r = region.and_then(|r| m.world.regions.get(&r));
        let owner = r.map_or_else(|| m.faction_by_key(&ui.link.human).map(|f| f.id), |r| Some(r.owner));
        let owner_key = owner.and_then(|o| m.world.factions.get(&o)).map(|f| f.key.clone()).unwrap_or_default();
        let shown = match args.get(1).and_then(value_str).filter(|k| !k.is_empty()) {
            None => region.and_then(|reg| m.map_fort_standing(reg, &m.map_fort_levels()).map(|(_, k)| k)),
            Some(k) if m.rules.buildings.contains_key(&k) => Some(k),
            Some(k) => {
                log(&inner, format!("WARN CampaignUI.FortDetails: '{k}' is not a valid building level key"));
                None
            }
        };
        let levels = shown.as_deref().and_then(|k| m.rules.chain_levels(k));
        drop(m);
        let (key, (name, short, long), (icon, info), (level, max)) = match &shown {
            Some(key) => {
                // The info picture only for a real variant: the placeholder icon has none.
                let files = ui.building_variant(&owner_key, key).map_or_else(
                    || (ui.building_icon(&owner_key, key), String::new()),
                    |(_, stem, _)| {
                        let stem = stem.to_ascii_lowercase();
                        (format!("data/ui/buildings/icons/{stem}.tga"), format!("data/ui/buildings/info/{stem}.tga"))
                    },
                );
                (key.clone(), building_texts_for(&inner, &ui, &owner_key, key), files, levels.unwrap_or_default())
            }
            None => (String::new(), Default::default(), (String::new(), String::new()), (0, 0)),
        };
        let t = lua.create_table()?;
        t.set("Key", key)?;
        t.set("Name", name)?;
        t.set("ShortDescription", short)?;
        t.set("LongDescription", long)?;
        t.set("InfoFilename", info)?;
        t.set("IconFilename", icon)?;
        t.set("Level", level)?;
        t.set("MaxLevel", max)?;
        Ok(Value::Table(t))
    });
    // PROVISIONAL: empty, as `BuildingEffects` / `TechEffects` above (the effect texts are not built).
    t.raw_set("FortEffects", lua.create_function(|lua, _: Variadic<Value>| lua.create_table())?)?;
    // InformLootingSelection("loot" | "occupy" | "liberate"): the capture screen's answer
    // (settlement_captured.lua's LootSettlement / OccupySettlement / LiberateSettlement, CONFIRMED
    // strings) → the model's ChooseCapture.
    f!("InformLootingSelection", |_l, inner, ui, choice: Option<String>| {
        use ntw_sim::campaign::CaptureChoice;
        let c = match choice.as_deref() {
            Some("loot") => Some(CaptureChoice::Loot),
            Some("occupy") => Some(CaptureChoice::Occupy),
            Some("liberate") => Some(CaptureChoice::Liberate),
            _ => None,
        };
        if let Some(choice) = c {
            ui.push(CampaignRequest::Command(CampaignCommand::ChooseCapture { choice }));
        }
        Ok(())
    });
    // TriggerBuildingCardSelectedEvent(building_key) (0x009FA0D0): posts a UI event with the key
    // (event vtable 0x0136C30C, INFERRED for the advisor). No listener in our game: a no-op.
    f!("TriggerBuildingCardSelectedEvent", |_l, inner, ui, _a: Variadic<Value>| Ok(()));
    // BuildingDetails(building_key, region_key, faction_key, slot_key) → the building tooltip's
    // details: {Key, Name, ShortDescription, Level, MaxLevel, ...} (CONFIRMED names read by
    // template.TechTreeItem_Tooltip.lua; "initialises a table of details about the building",
    // CONFIRMED description). Level and MaxLevel are 0-based: the chain level index and the highest
    // index in the chain, as the browser's `level` / `max_level`. CONFIRMED from the tooltip's pip
    // function (`TechTreeItem_Tooltip.lua:172`, lines 177-184): it shows pips 1..MaxLevel+1 as
    // "empty" and 1..Level+1 as "filled", finding each `Level Pip <n>`; 1-based values ran one pip
    // past the template's last and errored (`pip` nil at line 179). The binding of the two
    // arguments to Level / MaxLevel is INFERRED (the caller was not dumped).
    // BuildingEffects / TechEffects → effect lists; PROVISIONAL: empty (the effect texts are not
    // built yet).
    f!("BuildingDetails", |lua, inner, ui, (key, region_key, _f, slot_key): (Option<String>, Value, Value, Value)| {
        let Some(key) = key else { return Ok(Value::Nil) };
        let (level, max) = ui.model().rules.chain_levels(&key).unwrap_or((0, 0));
        let t = lua.create_table()?;
        let (name, desc) = building_texts(&inner, &ui, &key);
        t.set("Key", key.as_str())?;
        t.set("Name", name)?;
        t.set("ShortDescription", desc)?;
        t.set("Level", level)?;
        t.set("MaxLevel", max)?;
        t.set("region_key", region_key)?;
        t.set("slot_key", slot_key)?;
        Ok(Value::Table(t))
    });
    // BuildingBrowserDetails(region) → see `building_browser_details`.
    // __BuildingTreeNodes(slot): the tree view data (see `building_tree`); campaign_prelude.lua's
    // ConstructBuildingTree builds the components from it.
    f!("__BuildingTreeNodes", |lua, inner, ui, slot: Value| building_tree(lua, &inner, &ui, &slot));
    f!("BuildingBrowserDetails", |lua, inner, ui, region: Value| {
        let r = entity_of(&region, TAG_REGION).map(|r| RegionId(r as u32));
        building_browser_details(lua, &inner, &ui, r)
    });
    for name in ["BuildingEffects", "TechEffects"] {
        t.raw_set(name, lua.create_function(|lua, _: Variadic<Value>| lua.create_table())?)?;
    }
    // Map labels. CameraPosition() / CameraTarget() → x, y, z (CONFIRMED descriptions; Labels.lua
    // only compares them to notice camera moves). RetrieveVisibleEnitityDetails() → {Settlements =
    // {{Address, ScreenPos = {X, Y}}...}, Resources = {...}} (CONFIRMED names read by Labels.lua):
    // the settlements on screen, at the screen position the game reports with
    // `campaign_set_view`. PROVISIONAL: no resource icons yet.
    f!("CameraPosition", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.camera.get()));
    // CameraTarget() → the point the camera looks at (map x), the zoom (camera +0x138), the map y,
    // then the key of the theatre holding that point (0x009DFEC0, CONFIRMED order; the radar's
    // UpdateCamera reads all four). Reported by the game with `campaign_set_camera_target`.
    // PROVISIONAL: the theatre is the campaign's one whenever the point is inside its bounds.
    f!("CameraTarget", |_l, inner, ui, _a: Variadic<Value>| {
        let (x, zoom, y) = ui.camera_target.get();
        let inside = ui.theatre_bounds(&inner).is_some_and(|((x0, y0), (x1, y1))| x >= x0 && x <= x1 && y >= y0 && y <= y1);
        let theatre = if inside { ui.theatre(&inner).map(|r| r.id) } else { None };
        Ok((x, zoom, y, theatre))
    });
    f!("RetrieveVisibleEnitityDetails", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let list = lua.create_table()?;
        // The fog of war: a settlement the player has never seen has no label. This is
        // `CampaignModel::knows` (visible now OR explored), **not** `sees`: the original draws
        // the labels from the shroud's *explored* tree (CHARACTERS_FIDELITY.md §10; INFERRED --
        // no Ghidra output for it was kept, review 0-G), so a
        // settlement that has been seen and is now only explored keeps its label while the
        // terrain renderer dims it. Filtering with `sees` dropped those, leaving dimmed ground
        // with no label on it. INFERRED, from the name and the explored tree.
        let human = ui.model().faction_by_key(&ui.link.human).map(|h| h.id);
        let mut n = 0;
        for (r, x, y) in ui.visible.borrow().iter() {
            let seen = human.is_none_or(|h| {
                ui.model()
                    .world
                    .regions
                    .get(r)
                    .is_some_and(|reg| ui.model().knows(h, (reg.settlement.position.0.to_f32(), reg.settlement.position.1.to_f32())))
            });
            if !seen {
                continue;
            }
            let e = lua.create_table()?;
            e.set("Address", region_value(&ui, *r))?;
            let p = lua.create_table()?;
            p.set("X", *x)?;
            p.set("Y", *y)?;
            e.set("ScreenPos", p)?;
            e.set("Selected", ui.selection.get() == CampaignSelection::Settlement(*r))?;
            e.set("Over", ui.over.get() == Some(*r))?;
            n += 1;
            list.set(n, e)?;
        }
        t.set("Settlements", list)?;
        t.set("Resources", lua.create_table()?)?;
        Ok(t)
    });
    // ShouldShowLabelBottomRow(): "Out: Selected settlement, mouse over settlement" (CONFIRMED
    // description): the labels whose bottom row (wealth, population) is shown.
    f!("ShouldShowLabelBottomRow", |_l, inner, ui, _a: Variadic<Value>| {
        let sel = match ui.selection.get() {
            CampaignSelection::Settlement(r) => region_value(&ui, r),
            _ => Value::Nil,
        };
        Ok((sel, ui.over.get().map(|r| region_value(&ui, r)).unwrap_or(Value::Nil)))
    });
    // The CampaignSettlement(address) object's LabelDetails() (CONFIRMED names read by
    // template.city_info_bar.lua): {Name, IsCapital, FactionRGB = {R, G, B}, PopulationGrowthString,
    // Region = {Name, Key, Address, Region, Wealth, WealthChange, PopulationChange, ReligionKey}}.
    // CONFIRMED placement from the bottom-row function (`city_info_bar.lua:110`, lines 125-131):
    // `change_rates[details.Region.PopulationChange + 1]`, the same for `Region.WealthChange`,
    // `tostring(details.Region.Wealth)`, `religion:SetState(details.Region.ReligionKey)`,
    // `pop_text:SetStateText(details.PopulationGrowthString)` and
    // `CampaignUI.RegionsPublicOrders(details.Region.Region)`. So the changes are 0-based indices
    // into the template's `change_rates`, and `Region.Region` is the region's address (INFERRED: it
    // is what RegionsPublicOrders takes). IsCapital from the owner's capital (CAMPAIGN_DATA.md §3).
    // PROVISIONAL: both changes index 0, no religion.
    f!("__LabelDetails", |lua, inner, ui, a: Value| {
        let Some(r) = entity_of(&a, TAG_REGION).map(|r| RegionId(r as u32)) else { return Ok(Value::Nil) };
        let (key, owner, gdp, pop) = {
            let m = ui.model();
            let Some(reg) = m.world.regions.get(&r) else { return Ok(Value::Nil) };
            let owner = m.world.factions.get(&reg.owner).map(|f| f.key.clone()).unwrap_or_default();
            (reg.key.clone(), owner, reg.gdp, reg.population)
        };
        let t = lua.create_table()?;
        t.set("Name", ui.settlement_name(&inner, r))?;
        let rt = lua.create_table()?;
        rt.set("Name", region_name(&inner.loc, &key))?;
        rt.set("Key", key.as_str())?;
        rt.set("Address", region_value(&ui, r))?;
        rt.set("Region", region_value(&ui, r))?;
        rt.set("Wealth", gdp)?;
        rt.set("WealthChange", 0)?;
        rt.set("PopulationChange", 0)?;
        rt.set("ReligionKey", "")?;
        t.set("Region", rt)?;
        t.set("IsCapital", ui.model().world.is_capital(r))?;
        let c = ui.link.db.faction(&owner).map_or([128, 128, 128], |f| f.primary_colour());
        let rgb = lua.create_table()?;
        rgb.set("R", c[0])?;
        rgb.set("G", c[1])?;
        rgb.set("B", c[2])?;
        t.set("FactionRGB", rgb)?;
        t.set("Population", pop)?;
        t.set("PopulationGrowthString", "")?;
        Ok(Value::Table(t))
    });
    // RegionsPublicOrders(region) → upper, lower: the region's public order for the two classes
    // (CONFIRMED name, registered at 0x00429E90; CONFIRMED two results, upper first, from
    // `city_info_bar.lua:131-133`, which feeds them to `upper_order` / `lower_order` through the
    // template's StateFromOrder). Values as the lists panel's UpperOrder / LowerOrder.
    f!("RegionsPublicOrders", |_l, inner, ui, a: Value| {
        let Some(r) = entity_of(&a, TAG_REGION).map(|r| RegionId(r as u32)) else { return Ok((Value::Nil, Value::Nil)) };
        let m = ui.model();
        if !m.world.regions.contains_key(&r) {
            return Ok((Value::Nil, Value::Nil));
        }
        let po = ntw_sim::campaign::economy::public_order(&m, r);
        Ok((Value::Number(po.upper as f64), Value::Number(po.lower as f64)))
    });
    // The lists panel (entity_lists.lua, the "button_lists" HUD button): CONFIRMED function names
    // and the fields its row templates read (row_template_army / _naval / _region / _agent).
    // RetrieveFactionMilitaryForceLists(faction, armies) → one character details table per
    // commander ([`character_details`]: Address, Name, Location, Soldiers, ActionPoints,
    // ActionPointsPerTurn, CommandedUnit, ShowAsCharacter, ...).
    // RetrieveFactionRegionList(faction) → {{Address, Name, Settlement, SettlementAddress,
    // IsCapital, Wealth, WealthChange, Population, PopulationNumber, PopulationChange, UpperOrder,
    // LowerOrder, UpperTax, LowerTax}...}
    // RetrieveFactionAgentsList(faction) → {{Address, Name, Location, AgentType, ActionPoints,
    // ActionPointsPerTurn}...}
    // PROVISIONAL: a character without a name in the model shows his agent type; Location is the
    // region of the nearest settlement; changes 0; tax rates from the faction's levels.
    // The fog of war (INFERRED, as for the labels): another faction's force is listed only when the
    // player knows its commander ([`agents::knows_character`], CONFIRMED: the hidden-flag test and
    // the exposed lists), so an army hidden in the fog stays off the list.
    f!("RetrieveFactionMilitaryForceLists", |lua, inner, ui, (faction, armies): (Option<String>, Option<bool>)| {
        let key = faction.unwrap_or_else(|| ui.link.human.clone());
        let armies = armies.unwrap_or(true);
        let rows: Vec<CharacterId> = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&key).map(|f| f.id) else { return lua.create_table() };
            let me = m.faction_by_key(&ui.link.human).map(|h| h.id);
            m.world
                .forces
                .values()
                .filter(|x| x.faction == f && x.is_navy != armies)
                .filter(|x| Some(f) == me || x.commander.is_some_and(|c| me.is_some_and(|me| ntw_sim::campaign::agents::knows_character(&m, me, c))))
                .filter_map(|x| x.commander.filter(|c| m.world.characters.contains_key(c)))
                .collect()
        };
        // Each row is the commander's character details table (`0x009AD250`: it carries Soldiers,
        // ShowAsCharacter and CommandedUnit, CONFIRMED -- see `character_details`).
        let out = lua.create_table()?;
        for (i, c) in rows.iter().enumerate() {
            out.set(i + 1, character_details(lua, &inner, &ui, *c)?)?;
        }
        Ok(out)
    });
    f!("RetrieveFactionRegionList", |lua, inner, ui, faction: Option<String>| {
        let key = faction.unwrap_or_else(|| ui.link.human.clone());
        let rows: Vec<(RegionId, String, u32, u32, f32, f32)> = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&key).map(|f| f.id) else { return lua.create_table() };
            m.world
                .regions
                .values()
                .filter(|r| r.owner == f)
                .map(|r| {
                    let po = ntw_sim::campaign::economy::public_order(&m, r.id);
                    (r.id, r.key.clone(), r.gdp, r.population, po.upper, po.lower)
                })
                .collect()
        };
        let (upper_tax, lower_tax) = {
            let m = ui.model();
            m.faction_by_key(&key).map_or((0, 0), |f| (m.rules.tax_rate(&f.tax_upper), m.rules.tax_rate(&f.tax_lower)))
        };
        let out = lua.create_table()?;
        for (i, (r, rkey, gdp, pop, upper, lower)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Address", region_value(&ui, *r))?;
            e.set("Name", region_name(&inner.loc, rkey))?;
            e.set("Settlement", ui.settlement_name(&inner, *r))?;
            e.set("SettlementAddress", region_value(&ui, *r))?;
            e.set("IsCapital", ui.model().world.is_capital(*r))?;
            e.set("Wealth", *gdp)?;
            e.set("WealthChange", 0)?;
            e.set("Population", *pop)?;
            e.set("PopulationNumber", *pop)?;
            e.set("PopulationChange", 0)?;
            e.set("UpperOrder", *upper)?;
            e.set("LowerOrder", *lower)?;
            e.set("UpperTax", upper_tax)?;
            e.set("LowerTax", lower_tax)?;
            out.set(i + 1, e)?;
        }
        Ok(out)
    });
    f!("RetrieveFactionAgentsList", |lua, inner, ui, faction: Option<String>| {
        let key = faction.unwrap_or_else(|| ui.link.human.clone());
        let rows: Vec<(CharacterId, CharacterKind, i32, i32, (f32, f32))> = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&key).map(|f| f.id) else { return lua.create_table() };
            let commanders: std::collections::HashSet<CharacterId> = m.world.forces.values().filter_map(|x| x.commander).collect();
            m.world
                .characters
                .values()
                .filter(|c| c.faction == f && !commanders.contains(&c.id) && c.kind != CharacterKind::Minister)
                .filter(|c| !matches!(c.kind, CharacterKind::General | CharacterKind::Admiral | CharacterKind::Colonel | CharacterKind::Captain))
                .map(|c| (c.id, c.kind, c.movement_points, c.max_movement_points, (c.position.0.to_f32(), c.position.1.to_f32())))
                .collect()
        };
        let out = lua.create_table()?;
        for (i, (c, kind, ap, max_ap, pos)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Address", character_value(&ui, *c))?;
            e.set("Name", character_name(&inner, &ui, *c).unwrap_or_else(|| character_type_name(&inner, &ui, *c)))?;
            e.set("AgentType", kind.esf_name())?;
            e.set("Location", ui.location_name(&inner, *pos))?;
            e.set("ActionPoints", *ap)?;
            e.set("ActionPointsPerTurn", *max_ap)?;
            out.set(i + 1, e)?;
        }
        Ok(out)
    });
    // InitialiseGovernmentDetails() → the government screen's details (0x009E54D0, CONFIRMED
    // names): {DatabaseKey (government_types key), Government, Religion, ReligionIcon, Home
    // ("%S, %S": the capital's settlement and region), Treasury, Population, Prosperity, Prestige,
    // HadElectionInThisTurn, ...} and, from 0x009B1D40, ElectedMinisters (bool), Ministers
    // {<post key> = character details, FactionLeader = character details} and MinisterPool.
    // PROVISIONAL: Prosperity = summed region GDP, Prestige 0, Popularity / NextElections /
    // Governorships not given, the minister pool empty, ElectedMinisters false.
    f!("InitialiseGovernmentDetails", |lua, inner, ui, _a: Variadic<Value>| {
        let (gov, treasury, pop, gdp, capital, religion, posts, leader) = {
            let m = ui.model();
            let Some(f) = m.faction_by_key(&ui.link.human) else { return Ok(Value::Nil) };
            let owned: Vec<&ntw_sim::campaign::Region> = m.world.regions.values().filter(|r| r.owner == f.id).collect();
            let d = m.world.faction_details.get(&f.id);
            (
                f.government_key.clone(),
                f.treasury,
                owned.iter().map(|r| r.population).sum::<u32>(),
                owned.iter().map(|r| r.gdp).sum::<u32>(),
                m.world.capital(f.id).or_else(|| owned.first().map(|r| r.id)),
                d.map(|d| d.religion.clone()).unwrap_or_default(),
                d.map(|d| d.posts.iter().filter(|p| p.key != "faction_leader").filter_map(|p| Some((p.key.clone(), p.holder?))).collect::<Vec<_>>())
                    .unwrap_or_default(),
                m.world.faction_leader(f.id),
            )
        };
        let t = lua.create_table()?;
        t.set("DatabaseKey", gov.as_str())?;
        t.set("Name", faction_name(&inner, &ui.link.db, &ui.link.human))?;
        t.set("Government", loc(&inner, &format!("government_types_onscreen_{gov}")).unwrap_or_else(|| gov.clone()))?;
        t.set("Religion", loc(&inner, &format!("religions_onscreen_{religion}")).unwrap_or_else(|| religion.clone()))?;
        t.set("ReligionIcon", ui.religion_icon(&inner, &religion))?;
        if let Some(c) = capital {
            let key = ui.model().world.regions.get(&c).map(|r| r.key.clone()).unwrap_or_default();
            t.set("Home", format!("{}, {}", ui.settlement_name(&inner, c), region_name(&inner.loc, &key)))?;
        }
        t.set("Treasury", treasury)?;
        t.set("Population", pop)?;
        t.set("Prosperity", gdp)?;
        t.set("Prestige", 0)?;
        // Ministers are keyed by the post's `ministerial_positions` number (CONFIRMED: the record's
        // +0xC int is the key; head_of_government 1, finance 2, justice 3, army 4, navy 5,
        // governors 6.., government_screens.lua reads Ministers[1..5]).
        let numbers: HashMap<String, i32> = small_table(&inner, "db/ministerial_positions_tables/ministerial_positions", "s,i")
            .into_iter()
            .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(1)?.as_i32()?)))
            .collect();
        let ministers = lua.create_table()?;
        for (post, c) in posts {
            let Some(n) = numbers.get(&post) else { continue };
            ministers.set(*n, character_details(lua, &inner, &ui, c)?)?;
        }
        if let Some(c) = leader {
            ministers.set("FactionLeader", character_details(lua, &inner, &ui, c)?)?;
        }
        t.set("Ministers", ministers)?;
        t.set("MinisterPool", lua.create_table()?)?;
        t.set("ElectedMinisters", false)?;
        t.set("HadElectionInThisTurn", false)?;
        // Popularity "%d %%" and PopularityStatus "up" / "down" / "falling_fast" (0x008DEA60 /
        // 0x008DEB10, CONFIRMED shape: clamp(clamp(a + b, -20, 20) + round(mean of two values) + 10,
        // 0, 100); the status compares it with the last stored value, -10 or worse = falling_fast).
        // PROVISIONAL: the inputs are not decoded; we use the capital's public order (mean of the
        // two classes) as the mean term and 0 for a + b, and the status is always "up";
        // NextElections 0.
        let popularity = capital.map_or(50, |c| {
            let po = ntw_sim::campaign::economy::public_order(&ui.model(), c);
            (((po.lower + po.upper) / 2.0).round() as i32 + 10).clamp(0, 100)
        });
        t.set("Popularity", format!("{popularity} %"))?;
        t.set("PopularityStatus", "up")?;
        t.set("NextElections", 0)?;
        // The finance summary on the same table (0x009BB910, CONFIRMED names): TaxIncomeTotal,
        // AnnualIncome, FactionTaxIncomeUpper / Lower, Trade, ArmyUpkeep, NavyUpkeep, Policing,
        // OtherIncome, OtherIncomeTooltip; values from the model's income (`faction_income`).
        // PROVISIONAL: the upkeep is all shown as army upkeep, policing 0, the class split of the
        // taxes halves the total, AnnualIncome = revenue - upkeep.
        let income = {
            let m = ui.model();
            m.faction_by_key(&ui.link.human).map(|f| ntw_sim::campaign::economy::faction_income(&m, f.id)).unwrap_or_default()
        };
        t.set("TaxIncomeTotal", income.taxes)?;
        t.set("FactionTaxIncomeUpper", income.taxes / 2)?;
        t.set("FactionTaxIncomeLower", income.taxes - income.taxes / 2)?;
        t.set("Trade", income.trade)?;
        t.set("ArmyUpkeep", income.upkeep)?;
        t.set("NavyUpkeep", 0)?;
        t.set("Policing", 0)?;
        t.set("OtherIncome", income.other)?;
        t.set("OtherIncomeTooltip", "")?;
        t.set("AnnualIncome", income.revenue() - income.upkeep)?;
        Ok(Value::Table(t))
    });
    // PrestigeDetails() → {factions = {[i] = {name, key, flag_path, faction_colour {r, g, b},
    // is_ally, is_enemy, is_neighbouring, history = {[j] = {enlightenment, military, naval,
    // economics, overall}}}}, scale} (0x009EF290 → 0x009A5470, CONFIRMED keys). PROVISIONAL:
    // prestige is not in the model: the major powers are listed with one all-zero history entry,
    // scale 1; is_neighbouring false.
    f!("PrestigeDetails", |lua, inner, ui, _a: Variadic<Value>| {
        use ntw_sim::campaign::Stance;
        let m = ui.model();
        let me = m.faction_by_key(&ui.link.human).map(|f| f.id);
        let rows: Vec<(String, Stance)> = m
            .world
            .factions
            .values()
            .filter(|f| m.world.faction_details.get(&f.id).and_then(|d| d.major).unwrap_or(false))
            .map(|f| {
                let stance = me.and_then(|me| m.world.factions.get(&me)?.diplomacy.get(&f.id).copied()).unwrap_or_default();
                (f.key.clone(), stance)
            })
            .collect();
        drop(m);
        let factions = lua.create_table()?;
        for (i, (key, stance)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            e.set("name", faction_name(&inner, &ui.link.db, key))?;
            e.set("key", key.as_str())?;
            let rec = ui.link.db.faction(key);
            e.set("flag_path", rec.map(|r| r.flag_path.clone()).unwrap_or_default())?;
            let [r, g, b] = rec.map_or([128, 128, 128], |r| r.primary_colour());
            let c = lua.create_table()?;
            c.set("r", r)?;
            c.set("g", g)?;
            c.set("b", b)?;
            e.set("faction_colour", c)?;
            e.set("is_ally", *stance == Stance::Allied)?;
            e.set("is_enemy", *stance == Stance::War)?;
            e.set("is_neighbouring", false)?;
            let h = lua.create_table()?;
            let entry = lua.create_table()?;
            for k in ["enlightenment", "military", "naval", "economics", "overall"] {
                entry.set(k, 0)?;
            }
            h.set(1, entry)?;
            e.set("history", h)?;
            factions.set(i + 1, e)?;
        }
        let t = lua.create_table()?;
        t.set("factions", factions)?;
        t.set("scale", 1)?;
        Ok(Value::Table(t))
    });
    // MissionsDetails() → {active_missions = {...}, expired_missions = {}} for the player
    // (0x009EDA20 → 0x009B3BD0, CONFIRMED keys). Each active mission: Title, Activity (""),
    // Description, Issuer (""), Reward, Penalty ("No penalty"), Objective, RemainingTime, Year,
    // and location {X, Y} / Location when the mission has a place. The exe copies texts the
    // mission keeps (+0xA0 title, +0xAC description, +0xB8 objective, +0xD0 reward); ours come
    // from the loc the scripts' mission keys use: `mission_text_text_<key>_heading` / `_text`
    // (CONFIRMED keys exist), the objective `mission_activities_description_<activity>` (kind →
    // activity INFERRED), the reward the money as text (PROVISIONAL). RemainingTime = turns -
    // elapsed (INFERRED); the location is the target settlement's position.
    f!("MissionsDetails", |lua, inner, ui, _a: Variadic<Value>| {
        const ACTIVITY: [&str; 16] = [
            "capture_city",
            "protectorate_region_capture",
            "build",
            "recruit",
            "make_alliance",
            "capture_fort",
            "engage_faction",
            "blockade_port",
            "spy_on_city",
            "research",
            "assassination",
            "make_trade_agreement",
            "engage_faction",
            "engage_faction",
            "capture_city",
            "spy_on_city",
        ];
        let m = ui.model();
        let Some(me) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(Value::Nil) };
        let year = m.calendar.date.year;
        let rows: Vec<(ntw_sim::campaign::details::CampaignMission, Option<(f32, f32)>)> = m
            .world
            .missions
            .get(&me)
            .map(|list| {
                list.iter()
                    .map(|mi| {
                        let pos = mi
                            .settlement
                            .as_ref()
                            .or(mi.region.as_ref())
                            .and_then(|t| t.found())
                            .and_then(|r| m.world.regions.get(r))
                            .map(|r| (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32()));
                        (mi.clone(), pos)
                    })
                    .collect()
            })
            .unwrap_or_default();
        drop(m);
        let active = lua.create_table()?;
        for (i, (mi, pos)) in rows.iter().enumerate() {
            let e = lua.create_table()?;
            let text = |suffix: &str| loc(&inner, &format!("mission_text_text_{}_{suffix}", mi.script_key));
            e.set("Title", text("heading").unwrap_or_else(|| mi.script_key.clone()))?;
            e.set("Activity", "")?;
            e.set("Description", text("text").unwrap_or_default())?;
            e.set("Issuer", "")?;
            e.set("Reward", if mi.reward_money > 0 { mi.reward_money.to_string() } else { String::new() })?;
            e.set("Penalty", "No penalty")?;
            let activity = ACTIVITY.get(mi.kind as usize).copied().unwrap_or("capture_city");
            e.set("Objective", loc(&inner, &format!("mission_activities_description_{activity}")).unwrap_or_default())?;
            e.set("RemainingTime", mi.turns.saturating_sub(mi.elapsed))?;
            e.set("Year", year)?;
            if let Some((x, y)) = pos {
                let l = lua.create_table()?;
                l.set("X", *x)?;
                l.set("Y", *y)?;
                e.set("location", l.clone())?;
                e.set("Location", l)?;
            }
            active.set(i + 1, e)?;
        }
        let t = lua.create_table()?;
        t.set("active_missions", active)?;
        t.set("expired_missions", lua.create_table()?)?;
        Ok(Value::Table(t))
    });
    // RegionsOwnedByFactionOrByProtectorates(faction) → a sequence of {Address, Name,
    // OwnedByProtectorate}: the faction's regions, then its protectorates' (0x009F0DA0, CONFIRMED
    // keys; the protectorates are the factions it has stance 4 with, our Patron, INFERRED).
    f!("RegionsOwnedByFactionOrByProtectorates", |lua, inner, ui, key: Option<String>| {
        let m = ui.model();
        let Some(f) = m.faction_by_key(&key.unwrap_or_else(|| ui.link.human.clone())) else { return Ok(Value::Nil) };
        let protectorates: Vec<_> =
            f.diplomacy.iter().filter(|(_, s)| **s == ntw_sim::campaign::Stance::Patron).map(|(id, _)| *id).collect();
        let mut rows: Vec<(RegionId, String, bool)> =
            m.world.regions.values().filter(|r| r.owner == f.id).map(|r| (r.id, r.key.clone(), false)).collect();
        rows.extend(m.world.regions.values().filter(|r| protectorates.contains(&r.owner)).map(|r| (r.id, r.key.clone(), true)));
        drop(m);
        let t = lua.create_table()?;
        for (i, (id, key, prot)) in rows.into_iter().enumerate() {
            let e = lua.create_table()?;
            e.set("Address", region_value(&ui, id))?;
            e.set("Name", region_name(&inner.loc, &key))?;
            e.set("OwnedByProtectorate", prot)?;
            t.set(i + 1, e)?;
        }
        Ok(Value::Table(t))
    });
    // TradeInfo() → {prices, price_changes, supply, export} for the player (0x009F9F00: table
    // "trade_info_table"; routes from 0x00A299D0 with key, name, type, value, pips = {pip = {type,
    // tooltip, icon}}; prices / price_changes per commodity from 0x00A28FB0; CONFIRMED names).
    // government_screens.lua lists `supply` as imports and `export` as exports, `type` indexing
    // {Sea, Land, Blockaded, SeaRaided, Piracy, LandRaided, Banditry}. PROVISIONAL: every trade
    // partner (`economy::trade_partners`) is listed in both lists as a sea route worth the pair's
    // trade value, without pips; commodity prices are not given.
    f!("TradeInfo", |lua, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        let Some(me) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(Value::Nil) };
        let partners: Vec<(String, i32)> = ntw_sim::campaign::economy::trade_partners(&m, me)
            .into_iter()
            .filter_map(|p| Some((m.world.factions.get(&p)?.key.clone(), ntw_sim::campaign::economy::trade_pair_value(&m, me, p))))
            .collect();
        drop(m);
        let list = || -> mlua::Result<Table> {
            let t = lua.create_table()?;
            for (i, (key, value)) in partners.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("key", key.as_str())?;
                e.set("name", faction_name(&inner, &ui.link.db, key))?;
                e.set("type", 1)?;
                e.set("value", *value)?;
                e.set("pips", lua.create_table()?)?;
                t.set(i + 1, e)?;
            }
            Ok(t)
        };
        let t = lua.create_table()?;
        t.set("prices", lua.create_table()?)?;
        t.set("price_changes", lua.create_table()?)?;
        t.set("supply", list()?)?;
        t.set("export", list()?)?;
        Ok(Value::Table(t))
    });
    // RetrieveGovernorshipDetails(key) → see `governorship_details`.
    f!("RetrieveGovernorshipDetails", |lua, inner, ui, _key: Value| governorship_details(lua, &inner, &ui));
    // SetGovernorshipTaxRate(governorship, upper, rate): the tax slider (CONFIRMED call shape in
    // government_screens.lua: false = lower classes, true = upper). INFERRED: `rate` is the
    // `taxes_levels` index 0..4; it becomes the model's SetTaxLevel for the player's faction.
    f!("SetGovernorshipTaxRate", |_l, inner, ui, (_g, upper, rate): (Value, bool, f64)| {
        let m = ui.model();
        let Some(faction) = m.faction_by_key(&ui.link.human).map(|f| f.id) else { return Ok(()) };
        drop(m);
        let i = rate.round().clamp(0.0, 4.0) as usize;
        let class = if upper { ntw_sim::campaign::rules::TaxClass::Upper } else { ntw_sim::campaign::rules::TaxClass::Lower };
        let level = ntw_sim::campaign::details::TAX_LEVELS[i].to_owned();
        ui.push(CampaignRequest::Command(CampaignCommand::SetTaxLevel { faction, class, level }));
        Ok(())
    });
    // RegionFromSelection(): the selected settlement's region (button_selector.lua opens the
    // region info for it), nil otherwise. A fort answers with its region too (PROVISIONAL: what the
    // original returns for its `campaign_fort` is UNKNOWN; ours has exactly one region).
    f!("RegionFromSelection", |_l, inner, ui, _a: Variadic<Value>| {
        Ok(match ui.selection.get() {
            CampaignSelection::Settlement(r) | CampaignSelection::Fort(r) => region_value(&ui, r),
            _ => Value::Nil,
        })
    });
    f!("SelectAndZoomToCharacter", |_l, inner, ui, args: Variadic<Value>| {
        if let Some(c) = args.iter().find_map(|v| entity_of(v, TAG_CHARACTER)) {
            ui.push(CampaignRequest::Select(CampaignSelection::Character(CharacterId(c))));
        }
        Ok(())
    });
    f!("SelectAndZoomToRegion", |_l, inner, ui, args: Variadic<Value>| {
        if let Some(r) = args.iter().find_map(|v| entity_of(v, TAG_REGION)) {
            ui.push(CampaignRequest::Select(CampaignSelection::Settlement(RegionId(r as u32))));
        }
        Ok(())
    });
    Ok(())
}

/// The force the current selection shows in its army/navy tab.
fn selected_force(ui: &CampaignUi) -> Option<ForceId> {
    let m = ui.model();
    match ui.selection.get() {
        CampaignSelection::Character(c) => m.force_of(c),
        CampaignSelection::Settlement(r) => m.world.regions.get(&r).and_then(|r| r.garrison),
        // PROVISIONAL: the model's fort (the settlement's fortification) has no garrison of its own;
        // the original's forts do (slot residences, `cai_world` "forts, ports"), which the model does
        // not load.
        CampaignSelection::Fort(_) | CampaignSelection::None => None,
    }
}

/// Does the human player own `region`'s fort? The fort panel's `controlable` (`0x009FDFE0`'s
/// caller's bool, CONFIRMED key). PROVISIONAL owner: the region's -- the exe asks the fort itself
/// (its vtable owner call), but the model's `Fort` carries no owner (what the `FORT_ARRAY` record
/// stores beyond position and strings is UNKNOWN, and no shipped file holds a fort).
fn fort_controlable(m: &CampaignModel, human: &str, region: RegionId) -> bool {
    let owner = m.world.regions.get(&region).map(|r| r.owner);
    owner.is_some() && owner == m.faction_by_key(human).map(|f| f.id)
}

/// The region whose map fort `FortDetails` addresses: a fort address among its arguments (ours
/// carries the fort's region, [`fort_value`]), else a region address (that region's map fort),
/// else the selected fort. A selected settlement names no fort: its walls are a slot, not the map
/// fort. The exe's `fort_ptr` is the panel's fort object (`0x009FDFE0` pushes the panel's `+0x70`;
/// CONFIRMED, re-traced 2026-10-07 -- the `1` beside it is the push's flag byte, not the value).
/// Ours passes the constant 1 (PROVISIONAL), which names nothing, so the panel's calls use the
/// selection.
fn fort_region(ui: &CampaignUi, args: &[Value]) -> Option<RegionId> {
    let addressed = |tag| args.iter().find_map(|v| entity_of(v, tag)).map(|r| RegionId(r as u32));
    if let Some(r) = addressed(TAG_FORT) {
        return Some(r);
    }
    // A region address names that region's map fort only when one stands there
    // ([`CampaignModel::has_map_fort`]); a region without one names nothing (empty details).
    if let Some(r) = addressed(TAG_REGION) {
        return ui.model().has_map_fort(r).then_some(r);
    }
    match ui.selection.get() {
        CampaignSelection::Fort(r) => Some(r),
        _ => None,
    }
}

/// The region whose ports the naval recruitment tab recruits at: the settlement the selected naval
/// character stands in, else the faction's capital (INFERRED -- the exe's naval recruitment manager
/// is the region's, but which port a navy uses is UNKNOWN, and `CampaignShipCard` is a template no
/// shipped script references, §4.5 item 6).
fn naval_region(ui: &CampaignUi) -> Option<RegionId> {
    let m = ui.model();
    let standing = match ui.selection.get() {
        CampaignSelection::Character(c) => m.world.characters.get(&c).map(|ch| (ch.garrisoned_in, ch.faction)),
        _ => None,
    };
    let (standing, faction) = standing?;
    standing.or_else(|| m.world.capital(faction))
}

// ---------------------------------------------------------------------------------------------
// Engine → HUD calls.

impl UiScriptHost {
    /// Turns this host into the campaign HUD's host: fills `CampaignUI` with the engine
    /// functions above and runs the campaign prelude. Call before loading the layout.
    pub fn install_campaign(&self, link: CampaignLink) -> mlua::Result<()> {
        // Panels keep their authored size: only the front end stretches its pages to the screen
        // (as the battle HUD, see `battle.rs`).
        self.set_pages_fill_screen(false);
        // The HUD scripts work in the layout's 1280x960 frame (UiWorld::script_rect).
        self.set_script_frame(true);
        let names = settlement_names(&self.inner().loc);
        let lua = self.lua().clone();
        let ui = Rc::new(CampaignUi {
            link,
            lua,
            addresses: RefCell::new(HashMap::new()),
            address_meta: RefCell::new(HashMap::new()),
            requests: RefCell::new(Vec::new()),
            selection: Cell::new(CampaignSelection::None),
            tabs: RefCell::new(Vec::new()),
            current_tab: Cell::new(0),
            building_tabs: Cell::new(false),
            refused_tab_request_logged: Cell::new(false),
            unit_details: RefCell::new(HashMap::new()),
            settlement_names: names,
            slot_ids: RefCell::new(Interner::new()),
            variants: building_variants(&self.inner().source),
            camera: Cell::new((0.0, 0.0, 0.0)),
            visible: RefCell::new(Vec::new()),
            over: Cell::new(None),
            started: std::time::Instant::now(),
            map_folder: std::cell::OnceCell::new(),
            playable_areas: std::cell::OnceCell::new(),
            palettes: RefCell::new(HashMap::new()),
            theatre_bounds: std::cell::OnceCell::new(),
            camera_target: Cell::new((0.0, 0.0, 0.0)),
            slot_types: std::cell::OnceCell::new(),
            radar_view: RefCell::new((None, None)),
            attitude_levels: std::cell::OnceCell::new(),
            religion_icons: std::cell::OnceCell::new(),
            capture_shown: Cell::new(None),
            negotiation: RefCell::new(NegotiationState::default()),
        });
        let lua = self.lua();
        let t: Table = lua.globals().get("CampaignUI")?;
        install_functions(lua, self.inner(), &ui, &t)?;
        install_negotiation_object(lua, &ui)?;
        self.inner().campaign_mode.set(true);
        lua.load(PRELUDE).set_name("@ntw_campaign_prelude.lua").exec()?;
        *self.campaign.borrow_mut() = Some(ui);
        Ok(())
    }

    fn campaign_ui(&self) -> Option<Rc<CampaignUi>> {
        self.campaign.borrow().clone()
    }

    /// What the HUD asked the game to do since the last call; drained.
    pub fn take_campaign_requests(&self) -> Vec<CampaignRequest> {
        self.campaign_ui().map(|ui| std::mem::take(&mut *ui.requests.borrow_mut())).unwrap_or_default()
    }

    /// The current selection.
    pub fn campaign_selection(&self) -> CampaignSelection {
        self.campaign_ui().map_or(CampaignSelection::None, |ui| ui.selection.get())
    }

    /// The engine's start of the HUD, once the layout is loaded: the card groups get their
    /// manager modules (see `campaign_prelude.lua`) and nothing is selected (`ClearHud`, which
    /// empties the selection bar; its layout text is the designers' sample "ygT").
    pub fn campaign_ready(&self) {
        // Slot and queue-item addresses are kept over a reload: an id always names the same value
        // (`Interner`), so an address a script kept from before still names its own slot, never
        // another one that was given its id.
        let Some(root) = self.root() else { return };
        let r = (|| -> mlua::Result<()> {
            let f: Function = self.lua().globals().get("__ntw_campaign_ready")?;
            f.call::<()>(addr(root))
        })();
        if let Err(e) = r {
            log(self.inner(), format!("ERROR initialising the card groups: {e}"));
        }
        self.campaign_select(CampaignSelection::None);
    }

    /// The engine's call into the agent action menu: `root:LuaCall("OpenAgentOptionsPopup", src,
    /// target, mask, pct, pct)` (`layout.root.lua:1191`, arity 5 CONFIRMED), which opens the
    /// `agent_options` panel through `panel_manager` and hands it the mask that decides which of the
    /// nine action buttons appear plus the two percentages it puts on the Infiltrate and Sabotage Army
    /// buttons. The original's exe made this call when the player acted on a target with an agent; the
    /// mask is the engine's, and ours is computed from the model's gates in [`agent_options_mask`] --
    /// see there for the four bits that are derived and the five that are a named open item -- and the
    /// two percentages from the model's own chances in [`agent_options_percentages`].
    ///
    /// `target` is the thing the player picked, and it is a **character or a settlement** -- both
    /// CONFIRMED (see [`AgentMenuTarget`]). It reaches `agent_options.Initialise` as its `m_target`,
    /// which builds a `CampaignCharacter` handle only when `string.find(tostring(target), "CHARACTER")`
    /// matches (pc 2-9) -- **CONFIRMED satisfied since 0-E round 4**: an address is an interned table
    /// whose `__tostring` prints the original's own type string, so the match is taken and the popup
    /// gets its character handle (see `UI_FIDELITY.md` 9 and [`CampaignUi::entity`]). That handle's
    /// only use is the popup's teardown, which calls `Release()` on it (`agent_options.lua:89`).
    ///
    /// Nothing in `crates/napoleon` calls this yet: the agents tab's own buttons go straight to
    /// `OpenAgentActionPopup` (INFERRED, see [`open_agent_action_popup`]), so the options menu is
    /// reachable in our build only through this entry point.
    pub fn agent_options_popup(&self, agent: CharacterId, target: Option<AgentMenuTarget>) -> bool {
        let Some(ui) = self.campaign_ui() else {
            return false;
        };
        let src = ui.entity(TAG_CHARACTER, agent.0);
        let target = match target {
            Some(AgentMenuTarget::Character(c)) => ui.entity(TAG_CHARACTER, c.0),
            Some(AgentMenuTarget::Settlement(r)) => ui.entity(TAG_REGION, r.0 as i32),
            None => Value::Nil,
        };
        let r = (|| -> mlua::Result<bool> {
            let lua = self.lua();
            let t: Table = lua.globals().get("CampaignUI")?;
            let f: Function = t.get("__OpenAgentOptionsPopup")?;
            f.call::<bool>((src, target))
        })();
        match r {
            Ok(b) => b,
            Err(e) => {
                log(self.inner(), format!("ERROR in the agent options popup: {e}"));
                false
            }
        }
    }

    /// The engine's `UpdateFactionFundsAndDate{funds, season, year, round_description}` call
    /// (CONFIRMED from `layout.root.luac`).
    pub fn campaign_update_funds(&self, round_description: &str) {
        let Some(ui) = self.campaign_ui() else { return };
        let (funds, season, year) = {
            let m = ui.model();
            let funds = m.faction_by_key(&ui.link.human).map_or(0, |f| f.treasury);
            (funds, m.calendar.date.season, m.calendar.date.year)
        };
        let t = (|| -> mlua::Result<Table> {
            let t = self.lua().create_table()?;
            t.set("funds", funds.to_string())?;
            t.set("season", season)?;
            t.set("year", year.to_string())?;
            t.set("round_description", round_description)?;
            Ok(t)
        })();
        if let Ok(t) = t {
            call_root_global(self.lua(), self.inner(), "UpdateFactionFundsAndDate", t);
        }
    }

    /// The engine's selection change, in the exe's order (CONFIRMED, every selection handler:
    /// `HandleSettlementSelected 0x009C37A0`, `HandleFortSelected`, `0x009C2AF0`, `0x009C3FB0`,
    /// `0x009B8470`; UI_FIDELITY.md "Round 15 trace"):
    /// 1. The old tab set is freed and the manager holds none (+0xEEC = 0) until the new one is
    ///    stored: a tab request meanwhile is refused ([`CampaignUi::building_tabs`]).
    /// 2. `ClearReviewPanelTabs()`, then `ClearHud()`, on every selection (the tab set's base
    ///    ctor `0x00998BC0`). Nothing else when nothing is selected. No `ClearReviewPanel`.
    /// 3. Per tab, in order: `CreateReviewPanelTabAtPosition(title, key, index, 1)` (its ctor,
    ///    `0x0099A0B0`), then on adding it (`0x009C97B0`) either, for the tab kept from the same
    ///    selection, its opening at once: `ReviewPanelTabInit(title, index, 2)` and its
    ///    `Generate*Panel(info)` ([`generate_current_tab`]), or `ReviewPanelTabInit(title,
    ///    index, 1)`.
    /// 4. If no tab was opened, the first one is (`OpenFirstEnabledPanelTab 0x009DA2A0`; ours are
    ///    all enabled).
    /// 5. The new set is stored, then `SetSelectedEntity(entity, name)`.
    ///
    /// The tab kept on a refresh is found by its identity (`previous`, same selection only),
    /// never by its position (INFERRED: the exe's tab builders `FUN_00985F40` / `FUN_0099A200`
    /// take the tab to select from their caller). Ours hides the tooltip first: the components
    /// under the pointer are rebuilt (the pointer shows it again on the new component).
    pub fn campaign_select(&self, sel: CampaignSelection) {
        let Some(ui) = self.campaign_ui() else { return };
        let (lua, inner) = (self.lua(), self.inner());
        self.hover(None);
        let previous = (ui.selection.get() == sel)
            .then(|| ui.tabs.borrow().get(ui.current_tab.get().wrapping_sub(1)).copied())
            .flatten();
        ui.selection.set(sel);
        ui.tabs.borrow_mut().clear();
        ui.current_tab.set(0);
        ui.building_tabs.set(true);
        call_root_global(lua, inner, "ClearReviewPanelTabs", ());
        call_root_global(lua, inner, "ClearHud", ());
        if sel == CampaignSelection::None {
            ui.building_tabs.set(false);
            return;
        }
        let tabs = tabs_for(&ui, sel);
        for (i, tab) in tabs.iter().enumerate() {
            let index = i + 1;
            let title = tab_title(inner, *tab);
            call_root_global(lua, inner, "CreateReviewPanelTabAtPosition", (title.as_str(), tab.key(), index, TAB_UNSELECTED));
            ui.tabs.borrow_mut().push(*tab);
            if previous == Some(*tab) {
                ui.current_tab.set(index);
                generate_current_tab(lua, inner, &ui);
            } else {
                call_root_global(lua, inner, "ReviewPanelTabInit", (title, index, TAB_UNSELECTED));
            }
        }
        if ui.current_tab.get() == 0 && !tabs.is_empty() {
            ui.current_tab.set(1);
            generate_current_tab(lua, inner, &ui);
        }
        ui.building_tabs.set(false);
        let name = ui.selection_name(inner, sel);
        let entity = match sel {
            CampaignSelection::Character(c) => character_value(&ui, c),
            CampaignSelection::Settlement(r) => region_value(&ui, r),
            CampaignSelection::Fort(r) => fort_value(&ui, r),
            CampaignSelection::None => Value::Nil,
        };
        call_root_global(lua, inner, "SetSelectedEntity", (entity, name));
    }

    /// The capture screen: while the model holds a capture the human must answer
    /// (`CampaignModel::pending_capture`), the root's `SettlementLootingOptions(options)` opens the
    /// `settlement_captured` panel once (CONFIRMED: layout.root.lua opens it through the panel
    /// manager with the table; the exe fills it in `0x00A13DB0` → `0x009AB1B0`). The table:
    /// `SettlementName` ("settlement, region"), `Looting` / `Occupation` = {Loot, TownWealth,
    /// LowerOrder, UpperOrder, DamagedBuildings = {{Building, Damage}...}} and, only when a
    /// liberation target exists, `Liberation` = {FactionLocation} (CONFIRMED names). INFERRED:
    /// FactionLocation is the liberated faction's on-screen name; orders are the previewed public
    /// order after the choice, rounded. When the capture is answered the root's
    /// `ClearSettlementLootingOptions` closes the panel. Returns true when it opened the screen.
    pub fn campaign_capture_screen(&self) -> bool {
        let Some(ui) = self.campaign_ui() else { return false };
        let pending = ui.model().pending_capture.clone();
        let Some(p) = pending.filter(|p| ui.model().world.factions.get(&p.faction).is_some_and(|f| f.key == ui.link.human)) else {
            if ui.capture_shown.take().is_some() {
                call_root_global(self.lua(), self.inner(), "ClearSettlementLootingOptions", ());
            }
            return false;
        };
        if ui.capture_shown.get() == Some(p.region) {
            return false;
        }
        let r = (|| -> mlua::Result<Table> {
            let lua = self.lua();
            let inner = self.inner();
            let option = |o: &ntw_sim::campaign::CaptureOutcome| -> mlua::Result<Table> {
                let t = lua.create_table()?;
                t.set("Loot", o.money)?;
                t.set("TownWealth", o.town_wealth)?;
                let (lower, upper) = o.public_order_after.unwrap_or((0.0, 0.0));
                t.set("LowerOrder", lower.round() as i32)?;
                t.set("UpperOrder", upper.round() as i32)?;
                let damaged = lua.create_table()?;
                let m = ui.model();
                for (slot, health) in &o.buildings {
                    let level = m.world.regions.get(&p.region).and_then(|r| r.building_at(SlotRef::Slot(*slot))).map(|b| b.level_key.clone()).unwrap_or_default();
                    let d = lua.create_table()?;
                    d.set("Building", building_texts(inner, &ui, &level).0)?;
                    d.set("Damage", *health)?;
                    damaged.raw_push(d)?;
                }
                t.set("DamagedBuildings", damaged)?;
                Ok(t)
            };
            let t = lua.create_table()?;
            t.set("SettlementName", ui.selection_name(inner, CampaignSelection::Settlement(p.region)))?;
            t.set("Looting", option(&p.loot)?)?;
            t.set("Occupation", option(&p.occupy)?)?;
            if let Some(f) = p.liberate {
                let key = ui.model().world.factions.get(&f).map(|f| f.key.clone()).unwrap_or_default();
                let l = lua.create_table()?;
                l.set("FactionLocation", faction_name(inner, &ui.link.db, &key))?;
                t.set("Liberation", l)?;
            }
            Ok(t)
        })();
        match r {
            Ok(t) => {
                ui.capture_shown.set(Some(p.region));
                call_root_global(self.lua(), self.inner(), "SettlementLootingOptions", t)
            }
            Err(e) => {
                log(self.inner(), format!("ERROR building the capture screen: {e}"));
                false
            }
        }
    }

    /// Re-sends the current selection (after the model changed).
    pub fn campaign_refresh_selection(&self) {
        let sel = self.campaign_selection();
        if sel != CampaignSelection::None {
            self.campaign_select(sel);
        }
    }
}

/// Review-panel tab `i` (1-based, in range, not the current tab) becomes the current tab, inside
/// the `ReviewPanelTabSelectionSet_1_Indexed` call that asked for it, as the exe's `0x00A20620`
/// does (CONFIRMED): clear the review panel, the old tab's state change to deselected (its handler
/// calls the root's `ReviewPanelTabInit(.., index, 1)`, `0x009C7D80`), then `i` made current and
/// its state change to selected (`.., 2`), which generates its panel ([`generate_current_tab`]).
/// A script these calls run may ask for another tab in turn: that change nests inside this one,
/// as in the exe, which has no limit (the caller runs this under the state-function stack guard,
/// so a runaway ends with Lua's "C stack overflow" error, never a crash). The script that asked
/// reads the new panel's geometry current, as every read is (`host.rs` `lay_out_if_stale`).
fn select_tab(lua: &Lua, inner: &Rc<Inner>, ui: &CampaignUi, i: usize) {
    let old = ui.current_tab.get();
    call_root_global(lua, inner, "ClearReviewPanel", ());
    let old_tab = ui.tabs.borrow().get(old.wrapping_sub(1)).copied();
    if let Some(tab) = old_tab {
        call_root_global(lua, inner, "ReviewPanelTabInit", (tab_title(inner, tab), old, TAB_UNSELECTED));
    }
    ui.current_tab.set(i);
    generate_current_tab(lua, inner, ui);
}

/// A review-panel tab's title (`random_localisation_strings_string_<key>`, empty when missing).
fn tab_title(inner: &Inner, tab: Tab) -> String {
    loc(inner, &format!("random_localisation_strings_string_{}", tab.key())).unwrap_or_default()
}

/// The current review-panel tab's state change to selected: the root's
/// `ReviewPanelTabInit(title, index, 2)`, then the tab's `Generate*Panel(info)`.
fn generate_current_tab(lua: &Lua, inner: &Rc<Inner>, ui: &CampaignUi) {
    let i = ui.current_tab.get();
    let Some(tab) = ui.tabs.borrow().get(i.wrapping_sub(1)).copied() else { return };
    call_root_global(lua, inner, "ReviewPanelTabInit", (tab_title(inner, tab), i, TAB_SELECTED));
    let info = match tab {
        Tab::Army | Tab::Navy => match selected_force(ui) {
            Some(f) => force_info(lua, inner, ui, f).ok(),
            None => None,
        },
        Tab::Construction => match ui.selection.get() {
            CampaignSelection::Settlement(r) => construction_info(lua, inner, ui, r, ConstructionPanel::Settlement).ok(),
            _ => None,
        },
        Tab::Recruitment => match ui.selection.get() {
            CampaignSelection::Settlement(r) => recruitment_info(lua, inner, ui, r, false).ok(),
            // An army's recruitment tab ([`tabs_for`]). The exe's info builder `0x009FE7B0` reads
            // the tab's own recruitment manager (through the commander, `+0x34` → `+0x124`); what
            // that manager holds is not traced. PLACEHOLDER: the panel opens with no cards, so
            // nothing can be recruited through it (BACKLOG §0 "army recruitment tab contents").
            CampaignSelection::Character(c) => empty_recruitment_info(lua, ui, c).ok(),
            _ => None,
        },
        // The naval recruitment tab sits on a naval character (0-G's trace, §4.3); its manager is
        // the port the admiral stands in, falling back to the faction's capital (both INFERRED --
        // which port a navy recruits at is UNKNOWN, and the model keeps one naval queue per
        // region, so `recruitment_points(region, true)` is that region's whole naval capacity). PROVISIONAL:
        // the naval tab's manager (`0x009FE7B0`) is untraced, like the land army tab's.
        Tab::NavalRecruitment => match naval_region(ui) {
            Some(r) => recruitment_info(lua, inner, ui, r, true).ok(),
            None => None,
        },
        Tab::Agents => match ui.selection.get() {
            CampaignSelection::Settlement(r) => agents_info(lua, inner, ui, r).ok(),
            _ => None,
        },
        Tab::Infrastructure => match ui.selection.get() {
            CampaignSelection::Settlement(r) => construction_info(lua, inner, ui, r, ConstructionPanel::Infrastructure).ok(),
            _ => None,
        },
        Tab::Fort => match ui.selection.get() {
            CampaignSelection::Fort(r) => fort_info(lua, inner, ui, r).ok(),
            _ => None,
        },
    };
    match info {
        Some(Value::Table(t)) => {
            call_root_global(lua, inner, tab.generator(), t);
        }
        _ => log(inner, format!("UNKNOWN review panel info for {}", tab.key())),
    }
}

/// The review-panel tabs for a selection, in the engine's order (INFERRED from `FUN_00985f40`
/// (character) and `FUN_0099a200` (settlement): construction, recruitment, ..., army, agents).
/// PROVISIONAL: only the tabs whose panels we fill are listed.
///
/// 0-E round N+2, from 0-G's read-only trace of `FUN_0099A200` (its own Ghidra copy, §4.3): the
/// settlement's tab list is `construction_tab`, `recruitment_tab` (only when
/// `recruitment_points(region, false) > 0`), **`infrastructure_tab`**, `army_tab` and `agents_tab`,
/// and `naval_recruitment_tab` is **never** among them -- it is placed on the character panel, whose
/// generator is `GenerateNavyPanel`. So the naval tab below goes on a naval character. Re-traced
/// 2026-10-07 (own Ghidra copy, `ConstructSettlementPanelTabs`): the infrastructure tab is added when
/// the settlement has a road slot (`+0x1C0`) and shows that road slot (`0x00A021B0`), not the walls.
fn tabs_for(ui: &CampaignUi, sel: CampaignSelection) -> Vec<Tab> {
    let m = ui.model();
    let mut tabs = Vec::new();
    match sel {
        // A commanded force's tab set (own Ghidra copy, 2026-10-07; CONFIRMED structure): the map
        // entity's selection `0x009C2AF0` builds `0x009855B0` for an army -- `army_tab`, then
        // `recruitment_tab` when [`commander_recruits`], then `agents_tab` when the force carries
        // an agent (`0x008B77C0` over force `+0x6C/+0x70`) -- and `0x009990B0` for a navy:
        // `navy_tab`, `army_tab` when it carries an army, `naval_recruitment_tab` when
        // [`commander_recruits`], `agents_tab`. Each tab is created at index = tabs so far + 1, and
        // the root's `CreateReviewPanelTabAtPosition` places tab i at (i - 1) x its width
        // (`layout.root.lua:600-623`), so Army is left of Recruitment, wherever the army stands
        // (no position test in `0x009855B0`). The model carries no agents or troops aboard a
        // force, so those two optional tabs never appear (PROVISIONAL).
        CampaignSelection::Character(c) => {
            if let Some(f) = m.force_of(c).and_then(|f| m.world.forces.get(&f)) {
                let recruits = f.commander.is_some_and(|k| commander_recruits(&m, k));
                if f.is_navy {
                    tabs.push(Tab::Navy);
                    if recruits {
                        tabs.push(Tab::NavalRecruitment);
                    }
                } else {
                    tabs.push(Tab::Army);
                    if recruits {
                        tabs.push(Tab::Recruitment);
                    }
                }
            }
        }
        CampaignSelection::Settlement(r) => {
            // Recruitment when the region can recruit (FUN_0099A200 asks FUN_00B44DA0; INFERRED:
            // its recruitment points).
            tabs.push(Tab::Construction);
            if m.recruitment_points(r, false) > 0 {
                tabs.push(Tab::Recruitment);
            }
            // The infrastructure (road) tab, after recruitment as in `ConstructSettlementPanelTabs`
            // `0x0099A200`, which adds it when the settlement's road slot (`+0x1C0`) exists
            // (CONFIRMED: its test is the settlement's road slot pointer, not
            // `IsConstructionSlotShown` `0x00B7A0E0`, which only `0x00A021B0` asks, for the panel's
            // entry). Every `REGION_SLOT_MANAGER` of every shipped startpos holds a road slot
            // (CONFIRMED, see [`construction_info`]) and the model keeps no empty slot, so ours adds
            // the tab for every settlement (PROVISIONAL for a mod region without one). With nothing
            // to show the panel gets an empty `slots` table, as the exe's does.
            tabs.push(Tab::Infrastructure);
            if let Some(g) = m.world.regions.get(&r).and_then(|r| r.garrison)
                && m.world.forces.get(&g).is_some_and(|f| !f.units.is_empty())
            {
                tabs.push(Tab::Army);
            }
            // `agents_tab`, last (CONFIRMED in `FUN_0099A200`, added there with no visible test).
            // INFERRED condition: the settlement lists at least one agent (`listed_agents`). Its
            // panel is `ui/agents.luac`'s `GenerateAgentsPanel(info)`, see `agents_info`.
            drop(m);
            if !listed_agents(ui, r).is_empty() {
                tabs.push(Tab::Agents);
            }
            return tabs;
        }
        // A fort's own panel: `ConstructFortPanelTabs` `0x009988C0` (CONFIRMED structure) adds the
        // fort construction tab (`construction_tab`, generator `GenerateFortConstructionPanel`) first,
        // then the army / recruitment tabs when the fort has a garrison and the agents tab. The
        // model's fort has no garrison or agents of its own (it does not load them), so ours shows
        // the construction tab only (PROVISIONAL).
        CampaignSelection::Fort(r) => {
            if m.world.regions.contains_key(&r) {
                tabs.push(Tab::Fort);
            }
        }
        CampaignSelection::None => {}
    }
    tabs
}

/// Does a force's commander get a recruitment tab on his force's panel? The exe's `0x009D1CB0`
/// on the commander (own Ghidra copy, 2026-10-07): true when his agent record's per-culture row
/// (`0x009CBC40`: the hash in the agent record `+0x1AC`, keyed by the faction's culture,
/// returning the row's `+0x2C`) names a unit, or his agent type (record `+0x2C`) is 1, the
/// admiral. That row is `agent_culture_details` #3 (CHARACTERS_FIDELITY.md §8: `0x008E27D0`
/// reads the same hash for the general's unit), which only `General` rows fill (`european` /
/// `egy_european` → `Gen_Generals_Staff`, `middle_east` / `egy_middle_east` →
/// `Gen_Generals_Bodyguard`; none for `indian` / `tribal`). So: a general of a culture with a
/// general's unit, or an admiral; a colonel or a captain gets none (CONFIRMED: `0x009D1CB0` + user
/// check 2026-10-07, Henry Fox, colonel: Army tab only; admirals: `0x009D1CB0` + user
/// statement). The culture is the commander's faction's (INFERRED: `0x00A257C0` reads it through the character, falling back to
/// the human faction's).
fn commander_recruits(m: &CampaignModel, commander: CharacterId) -> bool {
    let Some(ch) = m.world.characters.get(&commander) else { return false };
    match ch.kind {
        CharacterKind::Admiral => true,
        CharacterKind::General => m.general_unit(ch.faction).is_some(),
        _ => false,
    }
}


impl CampaignUi {
    /// The campaign map folder as the original's file paths name it: "data/campaign_maps/<map>"
    /// (the map from the start position's CAMPAIGN_MAP_DATA; INFERRED prefix). Read once.
    fn map_folder(&self, inner: &Inner) -> String {
        self.map_folder
            .get_or_init(|| {
                let map = inner
                    .source
                    .find(&format!("campaigns/{}/startpos.esf", self.link.campaign))
                    .and_then(|f| ntw_campaign::read_info(&f.bytes).ok())
                    .map(|i| i.map_key)
                    .unwrap_or_else(|| format!("nap_{}", theatre_of(&self.link.campaign).1));
                let map = map.trim_start_matches("campaign_maps/").trim_start_matches("campaign_maps\\").to_owned();
                format!("data/campaign_maps/{map}")
            })
            .clone()
    }

    /// The `campaign_map_playable_areas` row of a theatre (matched on the area key, e.g.
    /// "europe_main", or the first column). Read once.
    fn playable_area(&self, inner: &Inner, theatre: &str) -> Option<ntw_data::CampaignMapPlayableArea> {
        let rows = self.playable_areas.get_or_init(|| {
            let path = <ntw_data::CampaignMapPlayableArea as ntw_data::DbRecord>::path();
            inner
                .source
                .find(&path)
                .and_then(|f| ntw_data::Table::<ntw_data::CampaignMapPlayableArea>::from_bytes(&f.bytes).ok())
                .map(|t| t.rows().to_vec())
                .unwrap_or_default()
        });
        rows.iter().find(|r| r.area.eq_ignore_ascii_case(theatre) || r.id == theatre).cloned()
    }

    /// The campaign's theatre: its `campaign_map_playable_areas` row (see `theatre_of`).
    fn theatre(&self, inner: &Inner) -> Option<ntw_data::CampaignMapPlayableArea> {
        self.playable_area(inner, theatre_of(&self.link.campaign).0)
    }

    /// `diplomatic_relations_attitudes` (level key → attitude value), read once.
    fn attitude_levels(&self, inner: &Inner) -> HashMap<String, i32> {
        self.attitude_levels
            .get_or_init(|| {
                small_table(inner, "db/diplomatic_relations_attitudes_tables/diplomatic_relations_attitudes", "s,i")
                    .into_iter()
                    .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(1)?.as_i32()?)))
                    .collect()
            })
            .clone()
    }

    /// A religion's pip picture (`religions` column 2, e.g. "data/ui/campaign ui/pips/animism.tga";
    /// CONFIRMED as the diplomacy list's ReligionIcon), read once.
    fn religion_icon(&self, inner: &Inner, religion: &str) -> Option<String> {
        self.religion_icons
            .get_or_init(|| {
                small_table(inner, "db/religions_tables/religions", "s,i,s")
                    .into_iter()
                    .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(2)?.as_str()?.to_owned())))
                    .collect()
            })
            .get(religion)
            .cloned()
    }

    /// The `slots` table (slot type → its kind flags), read once.
    fn slot_types(&self, inner: &Inner) -> &HashMap<String, ntw_data::SlotTypeRecord> {
        self.slot_types.get_or_init(|| {
            let path = <ntw_data::SlotTypeRecord as ntw_data::DbRecord>::path();
            inner
                .source
                .find(&path)
                .and_then(|f| ntw_data::Table::<ntw_data::SlotTypeRecord>::from_bytes(&f.bytes).ok())
                .map(|t| t.rows().iter().map(|r| (r.key.clone(), r.clone())).collect())
                .unwrap_or_default()
        })
    }

    /// The `building_culture_variants` row of `level` for `faction`'s culture, else any culture's
    /// row (the first culture in key order, so the fallback is deterministic): (culture, icon file
    /// stem, description key). The culture is `CampaignRules::faction_cultures`
    /// (`cultures_subcultures` of the faction's subculture), whose values are the table's culture keys
    /// (`european`, `middle_east`, `egy_european`, CONFIRMED in the data). PROVISIONAL source for
    /// every caller (icons and texts of all panels): INFERRED, not traced, that the exe picks the same: its details filler `0x008B1480` reads the culture through the faction's
    /// `+0x70C` record, not the `factions` table's `culture_variant` column. The one resolver for
    /// every building icon and text the HUD shows.
    fn building_variant(&self, faction: &str, level: &str) -> Option<(String, String, String)> {
        let culture = self.model().rules.faction_cultures.get(faction).cloned().unwrap_or_else(|| "european".into());
        let key = (level.to_owned(), culture);
        let row = self.variants.get_key_value(&key).or_else(|| {
            self.variants.range((level.to_owned(), String::new())..).next().filter(|((b, _), _)| b == level)
        });
        row.map(|((_, c), (icon, desc))| (c.clone(), icon.clone(), desc.clone()))
    }

    /// A building level's picture for a faction's culture (`building_culture_variants`, see
    /// `construction_info`), else the placeholder.
    fn building_icon(&self, faction: &str, level: &str) -> String {
        match self.building_variant(faction, level) {
            Some((_, icon, _)) => format!("data/ui/buildings/icons/{}.tga", icon.to_ascii_lowercase()),
            None => "data/ui/buildings/icons/eu_building_placeholder.tga".into(),
        }
    }

    /// The theatre's bounds in map units, (min x, min y), (max x, max y): what the game reported
    /// (`campaign_set_theatre_bounds`), else read once from the map's regions.esf (its
    /// `theatres_and_region_keys` theatre, CONFIRMED to be the theatre's area there).
    fn theatre_bounds(&self, inner: &Inner) -> Option<TheatreBounds> {
        *self.theatre_bounds.get_or_init(|| {
            let path = format!("{}/regions.esf", self.map_folder(inner));
            let bytes = inner.source.find(&path)?.bytes;
            ntw_formats::campaign_map::RegionMap::read(&bytes).ok().map(|m| m.theatre)
        })
    }

    /// Colour → palette index of a palette picture (the first index of each colour). Read once
    /// per picture.
    fn palette_index(&self, inner: &Inner, path: &str) -> PaletteIndex {
        if let Some(p) = self.palettes.borrow().get(path) {
            return p.clone();
        }
        let mut map = HashMap::new();
        if let Some(t) = inner.source.find(path).and_then(|f| ntw_formats::tga::Tga::decode(&f.bytes).ok()) {
            for (i, c) in t.palette.iter().enumerate() {
                map.entry([c[0], c[1], c[2]]).or_insert(i as i32);
            }
        }
        let map = Rc::new(map);
        self.palettes.borrow_mut().insert(path.to_owned(), map.clone());
        map
    }
}

// ===== Negotiation helpers =====

/// The two factions of the pending deal, as ids (the exe's "Proposer" and the +0xAC counterparty).
fn negotiation_factions(ui: &CampaignUi) -> (Option<FactionId>, Option<FactionId>) {
    let state = ui.negotiation.borrow();
    let m = ui.model();
    (
        state.proposer.as_deref().and_then(|k| m.faction_by_key(k)).map(|f| f.id),
        state.target.as_deref().and_then(|k| m.faction_by_key(k)).map(|f| f.id),
    )
}

/// A faction's treasury: the value behind the shared cap query 0x00BCAFE0 that both
/// `MaxPlayerPaymentAllowed` and `MaxOppositionPaymentAllowed` call (INFERRED).
fn treasury_of(ui: &CampaignUi, faction: FactionId) -> Option<i32> {
    ui.model().world.factions.get(&faction).map(|f| f.treasury)
}

/// Whether a region is already in the deal's offer (`offered`) or demand list.
fn region_in_deal(ui: &CampaignUi, key: &str, offered: bool) -> bool {
    let state = ui.negotiation.borrow();
    let items = if offered { &state.offers } else { &state.demands };
    items.iter().any(|i| matches!(i, NegotiationItem::Regions(keys) if keys.iter().any(|k| k == key)))
}

/// The "Action" a deal row reports (CONFIRMED key). The exe's action *ids* are UNKNOWN (only the
/// region action's 6 is known, from `BuildOfferAndDemandStrings`), so these are the host's names
/// (INFERRED) except the region action, whose name is fixed by the id.
fn negotiation_action_name(item: &NegotiationItem) -> &'static str {
    match item {
        NegotiationItem::Regions(_) => "transfer_region",
        NegotiationItem::Technologies(_) => "transfer_technology",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::Alliance) => "alliance",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::BreakAlliance) => "break_alliance",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::TradeAgreement) => "trade_agreement",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::BreakTrade) => "break_trade",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::Embargo) => "embargo",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::BecomeProtectorate) => "protectorate",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::GrantMilitaryAccess(_)) => "military_access",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::CancelMilitaryAccess) => "cancel_military_access",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::StateGift(_)) => "state_gift",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::RegularPayment(_, _)) => "payments",
        NegotiationItem::Payment { .. } => "payments",
    }
}

/// Apply the pending deal: every offer and demand the model can express becomes a
/// `CampaignCommand`, and the deal is then closed.
///
/// The exe's appliers (own Ghidra copy, all CONFIRMED addresses):
/// - **offer applier** `0x00B58C00(item)` and **demand applier** `0x00B58A30(faction)` walk the
///   same deal container — the sub-object at `*(vt+0x38)+0x120`, whose `+0x14` is the row count and
///   `+0x18` the first row, plus two *optional single* items at `+0x20` and `+0x24`. Per row they
///   test the row's `+0x1E8` "applied" flag, call `0x00B1A760(1)` and then the per-item applier
///   `0x00A6CBE0(0)`; the two single items go through the same `0x00A6CBE0(0)`.
///   `0x00B58560` and `0x00B58890` are two more appliers with the same walk (the first also builds
///   a loc string, id `0xFD`).
/// - **per-item / commit** `0x00A6CBE0` is an 11-byte forward to **`0x00B1A790(flag)`**, the deal
///   commit: when `0x00B4E4B0()` and `flag` agree and the pending deal's `+0x14` amount is
///   non-zero it runs **`0x00BB3810(amount, 3)`** — that is the **payments / tribute applier** — then
///   frees the pending deal (`0x0126E016(deal, 0x18)`), clears it and notifies two listeners.
///   `0x00B1A760(flag)` drains the pending list at `+0x4C`/`+0x50` through `0x00B1A820(item, flag)`,
///   which sets the item's vtable `+0x1C` slot, applies it and erases it from the vector.
/// - **region transfer** `0x00B449F0(faction, region_item, 1, 1, 0)` is applied *once*, after the
///   rows, by all four of those appliers; the demand path passes a null `region_item`, the offer
///   path the region it was handed. `0x00B58A10(faction, a, b)` is the wrapper that calls
///   `0x00B449F0(faction, 0, 0, a, b)` (the liberate path, CAMPAIGN_FIDELITY.md §Capture).
///
/// WIRED (0-E round N+2), one `CampaignCommand` per row type that now has an applier:
/// - **stance / access / gift terms** → `CampaignCommand::Diplomacy`. INFERRED mapping from
///   `negotiation_action_name` onto `DiplomaticAction`; the exe's own per-row `DiplomaticAction`
///   appliers are `0x00B55090` (trade), `0x00B29BB0` (break trade), `0x00B28DB0` (embargo),
///   `0x00B44550` (military access), `0x00B67BD0` (cancel access), `0x00B44590` (state gift) and
///   `0x00B105C0` (protectorate) — CONFIRMED addresses, already ported by 0-G in `treaties.rs`.
/// - **payments** → `CampaignCommand::Diplomacy` again: the lump sum (`turns == 0`) as a
///   `StateGift(amount)`, which is the model's mover for "this faction gives that faction this much
///   now" (`treaties::state_gift`, `0x00B44590` → `0x00B446B0`) — INFERRED as the stand-in for
///   **`0x00BB3810(amount, 3)`**, the CONFIRMED lump sum the commit runs, which moves money and also
///   raises the receiver's `state_gift` factor (the factor's exact contribution on that path is
///   UNKNOWN). The per-turn schedule becomes `RegularPayment(amount, turns)`, one of the container's
///   two optional single items (`+0x20` / `+0x24`, UNKNOWN which) — INFERRED that it is the payment.
///
/// STILL DROPPED (PLACEHOLDER): **technologies** → UNKNOWN. The rows go through `0x00A6CBE0` →
/// `0x00B1A790`, which only commits money; the per-row-type work happens behind the row vtable
/// (`0x00B1A820` calls vtable `+0x1C`) and **no technology-granting address has been traced from the
/// deal path** — reachable only through that vtable. The model has no `GrantTechnology` command, and
/// 0-E proved none is reachable, so no address is guessed here. Left open: UI_FIDELITY.md §4.5 item 8.
///
/// DEFERRED: **regions** (`0x00B449F0`, CONFIRMED address). The sandbox mapped them onto a
/// `TransferRegion` command whose effect was INFERRED from the capture path (it left the old owner's
/// garrison army behind in the settlement); main has no such command, so the rows are dropped until
/// `0x00B449F0` is decoded.
///
/// INFERRED (this file): `Propose`, `ProposeDeal` and `AcceptOffer` all end in this one applier.
/// Their wrappers read no Lua argument and push no literal, so nothing in the exe distinguishes
/// them here; the exe applies a deal from its own engine-side object instead.
///
/// Nothing fills the deal from Lua yet (see [`NegotiationItem`]), so in play this closes an empty
/// deal.
fn apply_deal(ui: &CampaignUi) {
    let (offers, demands) = {
        let mut state = ui.negotiation.borrow_mut();
        let taken = (std::mem::take(&mut state.offers), std::mem::take(&mut state.demands));
        state.active = false;
        state.possible_actions.clear();
        taken
    };
    let (Some(proposer), Some(recipient)) = negotiation_factions(ui) else { return };
    // An offer is the proposer acting on the recipient; a demand is the other way round.
    for (items, actor, target) in [(offers, proposer, recipient), (demands, recipient, proposer)] {
        for item in &items {
            for cmd in deal_item_commands(item, actor, target) {
                ui.push(CampaignRequest::Command(cmd));
            }
        }
    }
}

/// The `CampaignCommand`s one pending deal row becomes: `actor` acts on `target`. Split out of
/// [`apply_deal`] so the per-row mapping can be tested without a running campaign.
fn deal_item_commands(item: &NegotiationItem, actor: FactionId, target: FactionId) -> Vec<CampaignCommand> {
    use ntw_sim::campaign::treaties::DiplomaticAction;
    match item {
        NegotiationItem::Action(action) => vec![CampaignCommand::Diplomacy { a: actor, b: target, action: *action }],
        // INFERRED: a lump sum is the CONFIRMED `0x00BB3810(amount, 3)` money move, which the model
        // applies as a state gift; a schedule is the container's per-turn single item.
        NegotiationItem::Payment { amount, turns } => {
            let action = match *turns {
                0 => DiplomaticAction::StateGift(*amount),
                t => DiplomaticAction::RegularPayment(*amount, t),
            };
            vec![CampaignCommand::Diplomacy { a: actor, b: target, action }]
        }
        // DEFERRED: `0x00B449F0` has no model command on main (see `apply_deal`).
        NegotiationItem::Regions(_) => Vec::new(),
        // PLACEHOLDER: UNKNOWN, no granting address is reachable. See `apply_deal`.
        NegotiationItem::Technologies(_) => Vec::new(),
    }
}

/// Put the `negotiation` object in the script's globals. The panel script reaches its negotiation
/// through the bare name `negotiation` (the receivers in `worker3/lua_api.txt` are
/// `negotiation:<Method>`), and the exe binds that name to the `UIDiplomacyNegotiation` userdata
/// before the panel script runs; the host binds it to the same method set installed on
/// `CampaignUI`. INFERRED: how the exe pushes the object into the script state (open item 1 of
/// §4 in UI_FIDELITY.md) is still unknown, but the name is CONFIRMED by the call sites.
fn install_negotiation_object(lua: &Lua, _ui: &Rc<CampaignUi>) -> mlua::Result<()> {
    let methods: Table = lua.globals().get("CampaignUI")?;
    let obj = lua.create_table()?;
    for name in [
        "BuildPossibleActions",
        "BuildOfferAndDemandStrings",
        "TradeableRegions",
        "TradeableTechnologies",
        "FactionListsForStanceDeclarations",
        "MaxPlayerPaymentAllowed",
        "MaxOppositionPaymentAllowed",
        "ProposerId",
        "Propose",
        "ProposeDeal",
        "AcceptOffer",
        "End",
        "Cancel",
        "DeclineOffer",
        "RemoveAction",
        "CanPropose",
        "CanThreaten",
        "PrepareCounterOffer",
        "Finished",
        "IsNegotiation",
    ] {
        if let Ok(m) = methods.raw_get::<Value>(name) {
            obj.set(name, m)?;
        }
    }
    // The exe's scripts also name the class: `UIDiplomacyNegotiation` is a global (2 call sites in
    // the api dump) that constructs the userdata (0x00A102B0).
    let class = lua.create_table()?;
    class.set("new", obj.clone())?;
    lua.globals().set("negotiation", obj)?;
    lua.globals().set("UIDiplomacyNegotiation", class)?;
    Ok(())
}

/// A theatre's on-screen name: loc `campaign_map_playable_areas_onscreen_name_<key>` (CONFIRMED
/// key, e.g. "... _1244818741" = "Europe"), else the area key.
fn theatre_name(inner: &Inner, row: &ntw_data::CampaignMapPlayableArea) -> String {
    loc(inner, &format!("campaign_map_playable_areas_onscreen_name_{}", row.id)).unwrap_or_else(|| row.area.clone())
}

/// The campaign's theatre: (`campaign_map_playable_areas` area key, the radar's theatre button
/// id). PROVISIONAL mapping by the campaign key's prefix (each Napoleon campaign has one map and
/// one playable area: italy_main, egypt_main, europe_main, spain_main).
fn theatre_of(campaign: &str) -> (&'static str, &'static str) {
    let c = campaign.strip_prefix("mp_").unwrap_or(campaign);
    match c.split('_').next().unwrap_or("") {
        "ita" => ("italy_main", "italy"),
        "egy" => ("egypt_main", "egypt"),
        "spa" => ("spain_main", "spain"),
        _ => ("europe_main", "europe"),
    }
}

impl UiScriptHost {
    /// The campaign HUD's [`UiScriptHost::hover`].
    pub fn campaign_hover(&self, node: Option<NodeId>) {
        self.hover(node);
    }
}

/// A building level's on-screen name and short description for the owner's culture (see
/// `construction_info`; any culture's variant when the faction's own is missing).
fn building_texts(inner: &Inner, ui: &CampaignUi, level: &str) -> (String, String) {
    let (name, short, _) = building_texts_long(inner, ui, level);
    (name, short)
}

/// [`building_texts`] plus the long description (`building_description_texts_long_description_*`).
fn building_texts_long(inner: &Inner, ui: &CampaignUi, level: &str) -> (String, String, String) {
    building_texts_for(inner, ui, &ui.link.human, level)
}

/// A building level's name, short and long description for `faction_key`'s culture variant (any
/// culture's variant when that one is missing). `FortDetails` uses the fort owner's: the exe's
/// details filler `0x008B1480` runs on the fort's faction (`+0x210`, INFERRED owner), for the
/// texts and the icon alike.
fn building_texts_for(inner: &Inner, ui: &CampaignUi, faction_key: &str, level: &str) -> (String, String, String) {
    let (c, desc) = ui.building_variant(faction_key, level).map_or_else(|| (String::new(), level.to_owned()), |(c, _, d)| (c, d));
    let name = loc(inner, &format!("building_culture_variants_name_{level}{c}")).unwrap_or_else(|| level.to_owned());
    let short = loc(inner, &format!("building_description_texts_short_description_{desc}")).unwrap_or_default();
    let long = loc(inner, &format!("building_description_texts_long_description_{desc}")).unwrap_or_default();
    (name, short, long)
}

impl UiScriptHost {
    /// The game's view of the map for the settlement labels: the camera position (any values that
    /// change when the view changes), the settlements on screen with their screen positions, and
    /// the settlement under the pointer. The root layout's pulse then updates the labels
    /// (Labels.UpdateEntityLabels, original script).
    pub fn campaign_set_view(&self, camera: (f32, f32, f32), settlements: Vec<(RegionId, f32, f32)>, over: Option<RegionId>) {
        let Some(ui) = self.campaign_ui() else { return };
        ui.camera.set(camera);
        *ui.visible.borrow_mut() = settlements;
        ui.over.set(over);
    }

    /// The point the campaign camera looks at, in map units (x, y with y north), and its zoom
    /// (`CampaignUI.CameraTarget`, which the radar follows).
    pub fn campaign_set_camera_target(&self, x: f32, y: f32, zoom: f32) {
        if let Some(ui) = self.campaign_ui() {
            ui.camera_target.set((x, zoom, y));
        }
    }

    /// The campaign theatre's bounds in map units (min, max), when the game has the map loaded
    /// (otherwise the HUD reads them from the map's regions.esf itself).
    pub fn campaign_set_theatre_bounds(&self, min: (f32, f32), max: (f32, f32)) {
        if let Some(ui) = self.campaign_ui() {
            let _ = ui.theatre_bounds.set(Some((min, max)));
        }
    }

    /// The radar's camera outline in UI units: the ground points under the screen's four corners
    /// (map units, top-left, top-right, bottom-right, bottom-left), placed on the component
    /// AttachRadarView named with UpdateRadarView's mapping (x: offset by the theatre's corner,
    /// scaled by map width / theatre width; y: north up). None until the radar scripts set both.
    /// Also the clip rectangle of that component.
    pub fn campaign_radar_outline(&self, ground: [(f32, f32); 4]) -> Option<([(f32, f32); 4], UiRect)> {
        let ui = self.campaign_ui()?;
        let (node, mapping) = *ui.radar_view.borrow();
        let ((mw, mh), (ox, oy), (tw, th)) = mapping?;
        let w = self.world();
        let n = w.get(node?)?;
        if !n.visible || tw <= 0.0 || th <= 0.0 {
            return None;
        }
        let r = n.rect;
        let clip = w.clip_rect(node?).map_or(r, |c| c.intersect(&r));
        let pt = |(x, y): (f32, f32)| (r.x + (x - ox) / tw * mw, r.y + (1.0 - (y - oy) / th) * mh);
        Some((ground.map(pt), clip))
    }
}

impl CampaignUi {
    /// The name of the region whose settlement is nearest to a map position (PROVISIONAL
    /// "location" of a character until region areas are used).
    fn location_name(&self, inner: &Inner, pos: (f32, f32)) -> String {
        let m = self.model();
        m.world
            .regions
            .values()
            .min_by(|a, b| {
                let d = |r: &ntw_sim::campaign::Region| (r.settlement.position.0.to_f32() - pos.0).hypot(r.settlement.position.1.to_f32() - pos.1);
                d(a).total_cmp(&d(b))
            })
            .map(|r| region_name(&inner.loc, &r.key))
            .unwrap_or_default()
    }
}

/// The rows of a small DB table read with a generic schema (`ntw_formats::db` codes), empty if
/// the table is missing or does not match.
fn small_table(inner: &Inner, path: &str, schema: &str) -> Vec<Vec<ntw_formats::db::DbValue>> {
    use ntw_formats::db::{DbTable, Schema};
    let Some(file) = inner.source.find(path) else { return Vec::new() };
    let Some(schema) = Schema::from_codes(schema) else { return Vec::new() };
    DbTable::read(&file.bytes, &schema).map(|t| t.rows).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::campaign::treaties::DiplomaticAction as D;

    const A: FactionId = FactionId(1);
    const B: FactionId = FactionId(2);


    /// The negotiation appliers (`0x00BB3810` / `0x00B1A790` for the money, the treaty appliers for
    /// the stance rows), with the row names `BuildOfferAndDemandStrings` reports. Region rows
    /// (`0x00B449F0`) and technology rows have no model command and are dropped.
    #[test]
    fn negotiation_rows_map_to_their_appliers() {
        // The region rows are DEFERRED: no command is sent for them.
        assert_eq!(deal_item_commands(&NegotiationItem::Regions(vec!["eur_france".into()]), A, B), Vec::new());
        // The lump sum (0x00BB3810(amount, 3)) is a state gift, the schedule a regular payment.
        assert_eq!(
            deal_item_commands(&NegotiationItem::Payment { amount: 500, turns: 0 }, A, B),
            vec![CampaignCommand::Diplomacy { a: A, b: B, action: D::StateGift(500) }]
        );
        assert_eq!(
            deal_item_commands(&NegotiationItem::Payment { amount: 100, turns: 3 }, A, B),
            vec![CampaignCommand::Diplomacy { a: A, b: B, action: D::RegularPayment(100, 3) }]
        );
        // The stance / access / gift rows pass straight through.
        for action in [D::Alliance, D::BreakTrade, D::CancelMilitaryAccess, D::BecomeProtectorate] {
            assert_eq!(deal_item_commands(&NegotiationItem::Action(action), A, B), vec![CampaignCommand::Diplomacy { a: A, b: B, action }]);
        }
        // Technologies are still dropped: no granting address is reachable (see `apply_deal`).
        assert_eq!(deal_item_commands(&NegotiationItem::Technologies(vec!["admin1_public_schooling".into()]), A, B), Vec::new());
        // Every row type has a name the panel's rows report.
        assert_eq!(negotiation_action_name(&NegotiationItem::Regions(vec![])), "transfer_region");
        assert_eq!(negotiation_action_name(&NegotiationItem::Technologies(vec![])), "transfer_technology");
        assert_eq!(negotiation_action_name(&NegotiationItem::Payment { amount: 1, turns: 0 }), "payments");
        assert_eq!(negotiation_action_name(&NegotiationItem::Payment { amount: 1, turns: 2 }), "payments");
    }

    // ----- A campaign HUD host without the install: a MADE-UP one-region world and a made-up root
    // script that records what the engine calls on it (no game file is read).

    const HUMAN: &str = "test_faction_a";
    const REGION: RegionId = RegionId(10);
    const FORT_LEVEL: &str = "test_fort_level";

    /// The made-up root script: every engine call the selection makes is recorded in `calls`, the
    /// panel infos in `fort_panel` / `selected_entity`.
    const ROOT: &str = "calls = {}\n\
        function ClearHud() table.insert(calls, 'ClearHud') end\n\
        function ClearReviewPanel() end\n\
        function ClearReviewPanelTabs() end\n\
        function ReviewPanelTabInit() end\n\
        function CreateReviewPanelTabAtPosition(title, key, i, state) table.insert(calls, key) end\n\
        function GenerateFortConstructionPanel(info) fort_panel = info end\n\
        function GenerateConstructionPanel(info) end\n\
        function GenerateAgentsPanel(info) agents_panel = info end\n\
        function GenerateArmyPanel(info) army_panel = info end\n\
        function GenerateNavyPanel(info) army_panel = info end\n\
        function SetSelectedEntity(e, name) selected_entity = e end\n";

    struct TestHud {
        _scripts: crate::ScriptHost,
        host: UiScriptHost,
        root: NodeId,
    }

    fn test_hud() -> TestHud {
        test_hud_with(GameDatabase::test_fixture())
    }

    /// [`test_hud`] over database `db` (the fixture with some made-up rows added).
    fn test_hud_with(db: GameDatabase) -> TestHud {
        use ntw_sim::calendar::{Calendar, Date, HALF_EARLY};
        use ntw_sim::campaign::{BuildingRef, Faction, GovernmentType, Region, Settlement, World};
        use ntw_sim::fixed::Fixed20;
        let mut w = World::default();
        w.factions.insert(
            A,
            Faction {
                id: A,
                key: HUMAN.into(),
                treasury: 1000,
                government: GovernmentType::AbsoluteMonarchy,
                government_key: String::new(),
                tax_lower: "tax_normal".into(),
                tax_upper: "tax_normal".into(),
                diplomacy: Default::default(),
            },
        );
        w.turn_order = vec![A];
        w.regions.insert(
            REGION,
            Region {
                id: REGION,
                key: "test_region_10".into(),
                owner: A,
                settlement: Settlement { key: "settlement:test_region_10:town".into(), position: (Fixed20::from_int(10), Fixed20::from_int(0)) },
                slots: Vec::new(),
                road: None,
                fortification: Some(BuildingRef { level_key: FORT_LEVEL.into(), health: 100 }),
                population: 1000,
                base_gdp: 100,
                gdp: 100,
                wealth_growth_offset: 0,
                discontent_growth: 0,
                town_wealth: 0,
                town_wealth_growth: 0,
                tax_exempt: false,
                religions: Vec::new(),
                class_bases: Vec::new(),
                recruitment_queue: Vec::new(),
                construction: Vec::new(),
                garrison: None,
                fleet: None,
            },
        );
        let start = Date { year: 1805, season: 1, month: 0, half: HALF_EARLY };
        let mut model = CampaignModel::new(Calendar::new(start, 0), ntw_sim::rng::CaRng::new(1), w);
        model.rules = std::sync::Arc::new(ntw_sim::campaign::CampaignRules::test_rules());
        let scripts = crate::ScriptHost::new(model, HUMAN, crate::ScriptSource::empty()).unwrap();
        let source = crate::ScriptSource::empty().with_memory_file("ui/test/root", super::super::host::tests::layout_bytes_with_root(ROOT, ""));
        let host =
            UiScriptHost::new(source, ntw_formats::loc::Localisation::new(), super::super::host::tests::facts(), (100.0, 100.0)).unwrap();
        host.install_campaign(CampaignLink {
            state: scripts.shared_state(),
            human: HUMAN.into(),
            campaign: "test_campaign".into(),
            db: Rc::new(db),
        })
        .unwrap();
        let root = host.load_root_layout("ui/test/root").unwrap();
        host.campaign_ready();
        TestHud { _scripts: scripts, host, root }
    }

    impl TestHud {
        /// The interned address a script would see for `c`. The `*_value` helpers need the
        /// interning store (`UI_FIDELITY.md` 9.8), which lives on the campaign HUD.
        fn char_addr(&self, c: CharacterId) -> Value {
            character_value(&self.host.campaign_ui().unwrap(), c)
        }
        fn region_addr(&self, r: RegionId) -> Value {
            region_value(&self.host.campaign_ui().unwrap(), r)
        }
        fn unit_addr(&self, u: UnitId) -> Value {
            unit_value(&self.host.campaign_ui().unwrap(), u)
        }
        fn force_addr(&self, f: ForceId) -> Value {
            force_value(&self.host.campaign_ui().unwrap(), f)
        }
        fn fort_addr(&self, r: RegionId) -> Value {
            fort_value(&self.host.campaign_ui().unwrap(), r)
        }
        /// The root script's environment (its globals).
        fn root_env(&self) -> Table {
            self.host.script_env(self.root).expect("the root has scripts")
        }
        /// The tabs created since the last call, in order.
        fn tabs(&self) -> Vec<String> {
            let env = self.root_env();
            let calls: Table = env.get("calls").unwrap();
            let out = calls.sequence_values::<String>().map(Result::unwrap).filter(|c| c.ends_with("_tab")).collect();
            env.set("calls", self.host.lua().create_table().unwrap()).unwrap();
            out
        }
        fn errors(&self) -> Vec<String> {
            self.host.take_log().into_iter().filter(|l| l.starts_with("ERROR") || l.starts_with("UNKNOWN CampaignUI")).collect()
        }
        /// MADE-UP walls rules: the standing `FORT_LEVEL` is level 0 of `test_fort` (a
        /// `settlement_fortification` chain) and upgrades to two level 1s, `FORT_KEEP` (400) first,
        /// then the cheaper `FORT_BASTION` (50) -- a modded slot that offers several levels.
        fn with_fort_chain(&self) {
            use ntw_sim::campaign::BuildingRules;
            let mut st = self._scripts.state_mut();
            let rules = std::sync::Arc::make_mut(&mut st.model.rules);
            let level = |level: i32, cost: i32| BuildingRules { chain: "test_fort".into(), level, cost, turns: 2, ..Default::default() };
            rules.buildings.insert(FORT_LEVEL.into(), BuildingRules { upgrades_to: vec![FORT_KEEP.into(), FORT_BASTION.into()], ..level(0, 100) });
            rules.buildings.insert(FORT_KEEP.into(), level(1, 400));
            rules.buildings.insert(FORT_BASTION.into(), level(1, 50));
            rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
        }
        fn set_treasury(&self, gold: i32) {
            self._scripts.state_mut().model.world.factions.get_mut(&A).unwrap().treasury = gold;
        }
        /// The commands the calls in `script` request.
        fn requests_of(&self, script: &str) -> Vec<CampaignRequest> {
            self.host.take_campaign_requests();
            self.host.lua().load(script).exec().unwrap();
            self.host.take_campaign_requests()
        }
    }

    const FORT_KEEP: &str = "test_fort_keep";
    const FORT_BASTION: &str = "test_fort_bastion";
    const MAP_FORT0: &str = "test_map_fort_0";
    const MAP_FORT1: &str = "test_map_fort_1";
    const ROAD0: &str = "test_road_0";

    impl TestHud {
        /// MADE-UP map fort rules: chain `test_map_fort` of slot type `fort` (as vanilla `fFort`),
        /// levels `MAP_FORT0` (0) and `MAP_FORT1` (1).
        fn with_map_fort_chain(&self) {
            use ntw_sim::campaign::BuildingRules;
            let mut st = self._scripts.state_mut();
            let rules = std::sync::Arc::make_mut(&mut st.model.rules);
            let level = |level: i32| BuildingRules { chain: "test_map_fort".into(), level, cost: 300, turns: 2, ..Default::default() };
            rules.buildings.insert(MAP_FORT0.into(), BuildingRules { upgrades_to: vec![MAP_FORT1.into()], ..level(0) });
            rules.buildings.insert(MAP_FORT1.into(), level(1));
            rules.chain_slots.insert("test_map_fort".into(), vec!["fort".into()]);
        }
        /// MADE-UP road rules: chain `test_road` of slot type `settlement_road`, level `ROAD0`.
        fn with_road_chain(&self) {
            use ntw_sim::campaign::BuildingRules;
            let mut st = self._scripts.state_mut();
            let rules = std::sync::Arc::make_mut(&mut st.model.rules);
            rules.buildings.insert(ROAD0.into(), BuildingRules { chain: "test_road".into(), level: 0, cost: 10, turns: 1, ..Default::default() });
            rules.chain_slots.insert("test_road".into(), vec!["settlement_road".into()]);
        }
        /// The info a settlement panel tab hands `GenerateConstructionPanel`.
        fn construction(&self, panel: ConstructionPanel) -> Table {
            let ui = self.host.campaign_ui().unwrap();
            match construction_info(self.host.lua(), self.host.inner(), &ui, REGION, panel).unwrap() {
                Value::Table(t) => t,
                v => panic!("no construction info: {v:?}"),
            }
        }
    }

    fn slots_of(info: &Table) -> Vec<Table> {
        info.get::<Table>("slots").unwrap().sequence_values().map(Result::unwrap).collect()
    }
    fn keys_of(list: &Table) -> Vec<(String, i32)> {
        list.sequence_values::<Table>().map(Result::unwrap).map(|e| (e.get("building_key").unwrap(), e.get("type").unwrap())).collect()
    }

    /// The settlement walls are the construction panel's **last** slot card, built like any other
    /// slot (CONFIRMED: `0x00A01F50` appends the settlement's fortification slot after its slot
    /// list; runtime breakpoint and the user's in-game check, 2026-10-07). The frame's ordinary
    /// calls -- `BeginUpgrade` / `BeginConstruction`, `CancelConstruction`, `RepairBuilding`,
    /// `DemolishBuilding` -- address `SlotRef::Walls` through the card's `slot_key`, and the
    /// construction tab's info carries no `infrastructure` key (the exe pushes none).
    #[test]
    fn walls_are_the_construction_panels_last_slot_card() {
        let hud = test_hud();
        hud.with_fort_chain();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let info = hud.construction(ConstructionPanel::Settlement);
        assert!(info.get::<Value>("infrastructure").unwrap().is_nil(), "the construction tab is no infrastructure panel");
        let slots = slots_of(&info);
        let walls = slots.last().expect("the walls slot");
        let built = keys_of(&walls.get("buildings").unwrap());
        assert_eq!(built, vec![(FORT_LEVEL.to_owned(), 1)], "the standing walls, BUILT");
        let first: Table = walls.get::<Table>("buildings").unwrap().get(1).unwrap();
        let key: String = first.get("slot_key").unwrap();
        assert_eq!(key, fortification_slot_key("test_region_10"));
        assert_eq!(keys_of(&walls.get("upgrades").unwrap()), vec![(FORT_KEEP.to_owned(), 4), (FORT_BASTION.to_owned(), 4)]);
        // The frame's own calls build, cancel, repair and demolish the walls slot.
        let reqs = hud.requests_of(&format!(
            "CampaignUI.BeginUpgrade('{FORT_KEEP}', '{key}'); CampaignUI.CancelConstruction('{FORT_KEEP}', '{key}'); \
             CampaignUI.RepairBuilding('{FORT_LEVEL}', '{key}'); CampaignUI.DemolishBuilding('{FORT_LEVEL}', '{key}')"
        ));
        assert_eq!(
            reqs,
            vec![
                CampaignRequest::Command(CampaignCommand::ConstructBuilding { region: REGION, slot: SlotRef::Walls, level_key: FORT_KEEP.into() }),
                CampaignRequest::Command(CampaignCommand::CancelConstruction { region: REGION, slot: SlotRef::Walls }),
                CampaignRequest::Command(CampaignCommand::RepairBuilding { region: REGION, slot: SlotRef::Walls }),
                CampaignRequest::Command(CampaignCommand::DemolishBuilding { region: REGION, slot: SlotRef::Walls }),
            ]
        );
        let can: bool = hud.host.lua().load(format!("return CampaignUI.CanDemolishBuilding('{key}')")).eval().unwrap();
        assert!(can, "the standing walls can be demolished");
        // An empty walls slot offers the fortification chain's level 0 as a constructable card.
        hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().fortification = None;
        let slots = slots_of(&hud.construction(ConstructionPanel::Settlement));
        assert_eq!(keys_of(&slots.last().unwrap().get("buildings").unwrap()), vec![(FORT_LEVEL.to_owned(), 3)]);
        // With that level restricted by the scripts there is nothing to show: no walls entry
        // (`0x00B7A0E0`).
        hud._scripts.state_mut().model.world.restricted_buildings.insert(FORT_LEVEL.into());
        assert!(slots_of(&hud.construction(ConstructionPanel::Settlement)).is_empty());
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The infrastructure tab is the settlement's **road** slot (CONFIRMED: `0x00A021B0` lists the
    /// road slot alone and sets `infrastructure`), the tab added whenever the road slot exists
    /// (`0x0099A200`); it is a construction panel (`GenerateConstructionPanel`), not the fort panel.
    #[test]
    fn the_infrastructure_tab_is_the_road_slot() {
        let hud = test_hud();
        hud.tabs();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        // No road standing, building or on offer: the tab stays (the exe tests the road slot's
        // presence), and its panel's entry is dropped (`0x00B7A0E0`).
        assert!(hud.tabs().contains(&"infrastructure_tab".to_owned()), "the road slot exists, so its tab does");
        assert!(slots_of(&hud.construction(ConstructionPanel::Infrastructure)).is_empty());
        hud.with_road_chain();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let tabs = hud.tabs();
        assert!(tabs[0] == "construction_tab" && tabs.contains(&"infrastructure_tab".to_owned()), "{tabs:?}");
        let info = hud.construction(ConstructionPanel::Infrastructure);
        assert!(info.get::<bool>("infrastructure").unwrap());
        let slots = slots_of(&info);
        assert_eq!(slots.len(), 1, "the road slot alone");
        assert_eq!(keys_of(&slots[0].get("buildings").unwrap()), vec![(ROAD0.to_owned(), 3)]);
        let reqs = hud.requests_of(&format!("CampaignUI.BeginConstruction('{ROAD0}', '{}')", road_slot_key("test_region_10")));
        assert_eq!(reqs, vec![CampaignRequest::Command(CampaignCommand::ConstructBuilding { region: REGION, slot: SlotRef::Road, level_key: ROAD0.into() })]);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// A repair's card shows its progress over the repair's own length
    /// (`CampaignModel::repair_turns`), not over the level's build time.
    #[test]
    fn a_repair_card_counts_the_repair_length() {
        let hud = test_hud();
        hud.with_fort_chain();
        {
            let mut st = hud._scripts.state_mut();
            std::sync::Arc::make_mut(&mut st.model.rules).buildings.get_mut(FORT_LEVEL).unwrap().turns = 10;
            let r = st.model.world.regions.get_mut(&REGION).unwrap();
            r.fortification.as_mut().unwrap().health = 50;
            // floor(0.5 × 10) = 5 turns of repair, 3 of them left.
            r.construction.push(ntw_sim::campaign::ConstructionItem { slot: SlotRef::Walls, level_key: FORT_LEVEL.into(), turns_remaining: 3, cost: 0 });
        }
        let slots = slots_of(&hud.construction(ConstructionPanel::Settlement));
        let card: Table = slots.last().unwrap().get::<Table>("buildings").unwrap().get(1).unwrap();
        assert!(card.get::<bool>("being_repaired").unwrap());
        assert_eq!(card.get::<u32>("turns_to_completion").unwrap(), 3);
        assert_eq!(card.get::<f32>("percent_complete").unwrap(), 40.0);
        // The repair cost is shown while the repair runs (`0x009C8190` writes `0x00B66410(slot)`
        // whatever the repair state; review: it showed 0), though no second repair can start.
        assert!(!card.get::<bool>("can_repair").unwrap());
        let cost = hud._scripts.state().model.repair_cost(REGION, SlotRef::Walls);
        assert!(cost > 0);
        assert_eq!(card.get::<i32>("repair_cost").unwrap(), cost);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// A slot address keeps naming its own slot over a reload (`campaign_ready`): ids are never
    /// handed to another slot, so an address a script kept cannot come to name a different one.
    #[test]
    fn slot_addresses_survive_a_reload() {
        let hud = test_hud();
        let ui = hud.host.campaign_ui().unwrap();
        let walls = slot_value(&ui, REGION, SlotRef::Walls);
        hud.host.campaign_ready();
        let road = slot_value(&ui, REGION, SlotRef::Road);
        assert_eq!(slot_from_entity(&ui, &walls), Some((REGION, SlotRef::Walls)));
        assert_eq!(slot_from_entity(&ui, &road), Some((REGION, SlotRef::Road)));
        let again = slot_value(&ui, REGION, SlotRef::Walls);
        let (Value::Table(a), Value::Table(b)) = (&walls, &again) else { panic!("slot addresses are tables") };
        assert_eq!(a.to_pointer(), b.to_pointer(), "the same slot is the same address");
    }

    /// The map fort's panel (`GenerateFortConstructionPanel`) shows the fort chain (`fFort`, slot
    /// type `fort`): the standing level and one upgrade card that is always greyed (`affordable` =
    /// `0x0047BA10()`, constant false), and none of the fort's actions builds anything --
    /// `UpgradeFort` and `BuildFort` are switched off in the exe (CONFIRMED by the bytes), the rest
    /// are PLACEHOLDER no-ops (the model's map fort has no building state).
    #[test]
    fn the_map_fort_panel_never_builds() {
        let hud = test_hud();
        hud.with_map_fort_chain();
        hud.with_fort_chain();
        hud.set_treasury(1_000_000);
        hud.tabs();
        hud.host.campaign_select(CampaignSelection::Fort(REGION));
        assert_eq!(hud.tabs(), vec!["construction_tab".to_owned()], "a fort's panel is its construction tab (0x52)");
        let env = hud.root_env();
        let info: Table = env.get("fort_panel").expect("the fort panel was generated");
        assert!(info.get::<bool>("controlable").unwrap());
        let forts: Vec<Table> = info.get::<Table>("forts").unwrap().sequence_values().map(Result::unwrap).collect();
        let rows: Vec<(String, i32, bool)> =
            forts.iter().map(|r| (r.get("building_key").unwrap(), r.get("type").unwrap(), r.get("affordable").unwrap())).collect();
        assert_eq!(rows, vec![(MAP_FORT0.to_owned(), 1, true), (MAP_FORT1.to_owned(), 4, false)], "the walls are not on the fort panel");
        let entity: Value = env.get("selected_entity").unwrap();
        assert_eq!(entity_of(&entity, TAG_FORT), Some(REGION.0 as i32), "the scripts get a fort address");
        let fort = hud.fort_addr(REGION);
        let f = hud.host.lua().create_function(move |_, ()| Ok(fort.clone())).unwrap();
        hud.host.lua().globals().set("__test_fort", f).unwrap();
        let reqs = hud.requests_of(
            "local f = __test_fort(); CampaignUI.UpgradeFort(f); CampaignUI.UpgradeFort(1); CampaignUI.RepairFort(f); \
             CampaignUI.CancelUpgradeFort(f); CampaignUI.CancelFortRepair(f); CampaignUI.DemolishFort(f); CampaignUI.BuildFort(1)",
        );
        assert!(reqs.is_empty(), "{reqs:?}");
        let details: Table = hud.host.lua().load("return CampaignUI.FortDetails(__test_fort())").eval().unwrap();
        assert_eq!(details.get::<String>("Key").unwrap(), MAP_FORT0);
        // The settlement's own tabs: construction first, and no fort panel. With a settlement
        // selected, `FortDetails` addresses no map fort: the default details.
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let details: Table = hud.host.lua().load("return CampaignUI.FortDetails(1)").eval().unwrap();
        assert_eq!(details.get::<String>("Key").unwrap(), "", "a settlement has no map fort");
        let tabs = hud.tabs();
        assert_eq!(tabs[0], "construction_tab", "{tabs:?}");
        let entity: Value = hud.root_env().get("selected_entity").unwrap();
        assert_eq!(entity_of(&entity, TAG_REGION), Some(REGION.0 as i32));
        hud.host.campaign_select(CampaignSelection::None);
        assert!(hud.tabs().is_empty());
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// A loaded fort's own level is its standing level (`Fort::key`, INFERRED the record's level
    /// key), and the upgrade card is the next level index after it.
    #[test]
    fn a_loaded_fort_shows_its_own_level() {
        let hud = test_hud();
        hud.with_map_fort_chain();
        {
            let mut st = hud._scripts.state_mut();
            let fort = ntw_sim::campaign::Fort { id: ntw_sim::campaign::FortId(1), region: REGION, position: None, key: MAP_FORT1.into() };
            st.model.world.forts.insert(fort.id, fort);
        }
        hud.host.campaign_select(CampaignSelection::Fort(REGION));
        let info: Table = hud.root_env().get("fort_panel").unwrap();
        let rows: Vec<(String, i32)> =
            info.get::<Table>("forts").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|r| (r.get("building_key").unwrap(), r.get("type").unwrap())).collect();
        assert_eq!(rows, vec![(MAP_FORT1.to_owned(), 1)], "standing at level 1, no level 2 to offer");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// `FindNextFortUpgradeLevel` (`0x00B430C0`, CONFIRMED) takes the first record of the next
    /// level index and gives none when the scripts restricted it: a second fort chain's level of
    /// the same index is not offered instead (review: the panel skipped to it).
    #[test]
    fn a_restricted_next_fort_level_gives_no_upgrade_card() {
        let hud = test_hud();
        hud.with_map_fort_chain();
        {
            use ntw_sim::campaign::BuildingRules;
            let mut st = hud._scripts.state_mut();
            let rules = std::sync::Arc::make_mut(&mut st.model.rules);
            // Sorts after `MAP_FORT1` among the level-1 records.
            rules.buildings.insert("test_more_fort_1".into(), BuildingRules { chain: "test_more_fort".into(), level: 1, cost: 300, turns: 2, ..Default::default() });
            rules.chain_slots.insert("test_more_fort".into(), vec!["fort".into()]);
            st.model.world.restricted_buildings.insert(MAP_FORT1.into());
        }
        hud.host.campaign_select(CampaignSelection::Fort(REGION));
        let info: Table = hud.root_env().get("fort_panel").unwrap();
        let rows: Vec<(String, i32)> =
            info.get::<Table>("forts").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|r| (r.get("building_key").unwrap(), r.get("type").unwrap())).collect();
        assert_eq!(rows, vec![(MAP_FORT0.to_owned(), 1)]);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The building browser lists an empty slot only when the model's option rule, with the
    /// scripts' restricted levels, leaves something to start there (review: a restricted walls
    /// slot was listed as a construction site).
    #[test]
    fn the_building_browser_skips_a_fully_restricted_empty_slot() {
        let hud = test_hud();
        hud.with_fort_chain();
        hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().fortification = None;
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let rows = || -> Vec<i32> {
            let t: Table = hud.host.lua().load("return CampaignUI.BuildingBrowserDetails()").eval().unwrap();
            t.get::<Table>("slots").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|e| e.get("type").unwrap()).collect()
        };
        assert_eq!(rows(), vec![7], "the empty walls slot can take the fortification's level 0");
        hud._scripts.state_mut().model.world.restricted_buildings.insert(FORT_LEVEL.into());
        assert!(rows().is_empty(), "nothing to start there once the scripts restrict it");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The building browser lists the walls slot between the region's slots and the road, as the
    /// exe's `0x009B5AF0` does (type 7, fortification).
    #[test]
    fn the_building_browser_lists_the_walls() {
        let hud = test_hud();
        hud.with_fort_chain();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let t: Table = hud.host.lua().load("return CampaignUI.BuildingBrowserDetails()").eval().unwrap();
        let rows: Vec<(i32, String)> =
            t.get::<Table>("slots").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|e| (e.get("type").unwrap(), e.get("building_key").unwrap())).collect();
        assert_eq!(rows, vec![(7, FORT_LEVEL.to_owned())]);
        // "View tree" on the walls row (review crash: the walls' slot index was taken as a
        // `Region::slots` index): the tree of the walls slot, its upgrades available.
        let tree: Table = hud
            .host
            .lua()
            .load("local t = CampaignUI.BuildingBrowserDetails(); return CampaignUI.__BuildingTreeNodes(t.slots[1].slot)")
            .eval()
            .unwrap();
        assert_eq!(tree.get::<String>("slot_key").unwrap(), fortification_slot_key("test_region_10"));
        let nodes: Vec<(String, String)> =
            tree.get::<Table>("nodes").unwrap().sequence_values::<Table>().map(Result::unwrap).map(|n| (n.get("key").unwrap(), n.get("state").unwrap())).collect();
        assert!(nodes.contains(&(FORT_LEVEL.to_owned(), "normal".to_owned())), "{nodes:?}");
        assert!(nodes.contains(&(FORT_KEEP.to_owned(), "available".to_owned())), "{nodes:?}");
        // An unresearched level is not available (the model's option rule checks technology).
        {
            let mut st = hud._scripts.state_mut();
            std::sync::Arc::make_mut(&mut st.model.rules).building_techs.insert(FORT_KEEP.into(), vec!["test_missing_tech".into()]);
        }
        let tree: Table = hud
            .host
            .lua()
            .load("local t = CampaignUI.BuildingBrowserDetails(); return CampaignUI.__BuildingTreeNodes(t.slots[1].slot)")
            .eval()
            .unwrap();
        let keep: Option<String> =
            tree.get::<Table>("nodes").unwrap().sequence_values::<Table>().map(Result::unwrap).find(|n| n.get::<String>("key").unwrap() == FORT_KEEP).map(|n| n.get("state").unwrap());
        assert_eq!(keep.as_deref(), Some("unavailable"));
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// `FortDetails` as the exe's `0x009E49D0` -> `0x009AA250`: one subject (the named level, an
    /// empty key = the map fort's standing level), always the same eight keys, and default empty
    /// strings / 0 when there is no level (no fort chain, an unknown key) -- never a nil the
    /// tooltip trips on.
    #[test]
    fn fort_details_describe_one_subject() {
        const KEYS: [&str; 8] = ["Key", "Name", "ShortDescription", "LongDescription", "IconFilename", "InfoFilename", "Level", "MaxLevel"];
        let hud = test_hud();
        hud.host.campaign_select(CampaignSelection::Fort(REGION));
        let lua = hud.host.lua();
        let details = |script: &str| -> Table {
            let t: Table = lua.load(script).eval().unwrap();
            let keys: std::collections::BTreeSet<String> = t.pairs::<String, Value>().map(|p| p.unwrap().0).collect();
            assert_eq!(keys, KEYS.iter().map(|k| (*k).to_owned()).collect(), "{script}");
            t
        };
        let key_level = |t: &Table| -> (String, i32, i32) { (t.get("Key").unwrap(), t.get("Level").unwrap(), t.get("MaxLevel").unwrap()) };
        // No fort chain at all: the default details, strings empty and levels 0.
        let empty = details("return CampaignUI.FortDetails(1, '')");
        assert_eq!(key_level(&empty), (String::new(), 0, 0));
        assert_eq!(empty.get::<String>("Name").unwrap(), "");
        assert_eq!(empty.get::<String>("ShortDescription").unwrap(), "");
        hud.with_map_fort_chain();
        // The standing fort: no key, a nil key or an empty key (the frame's `building_key`).
        for script in ["return CampaignUI.FortDetails(1)", "return CampaignUI.FortDetails(1, nil)", "return CampaignUI.FortDetails(1, '')"] {
            assert_eq!(key_level(&details(script)), (MAP_FORT0.to_owned(), 0, 1), "{script}");
        }
        // The upgrade card's level, as the card's tooltip asks (`FortDetails(g_fort_ptr, key)`).
        assert_eq!(key_level(&details(&format!("return CampaignUI.FortDetails(1, '{MAP_FORT1}')"))), (MAP_FORT1.to_owned(), 1, 1));
        // Any building level is described as itself (the exe looks the key up in all of
        // `building_levels`); its own level never falls back to 0.
        let other = details("return CampaignUI.FortDetails(1, 'test_building_level_2')");
        assert_eq!(key_level(&other), ("test_building_level_2".to_owned(), 1, 1));
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
        // An unknown key: the default details.
        let unknown = details("return CampaignUI.FortDetails(1, 'test_no_such_level')");
        assert_eq!(key_level(&unknown), (String::new(), 0, 0));
        // No fort addressed at all (nothing selected, no fort argument): the default table, not nil.
        hud.host.campaign_select(CampaignSelection::None);
        let none = details("return CampaignUI.FortDetails(1, '')");
        assert_eq!(key_level(&none), (String::new(), 0, 0));
        assert_eq!(key_level(&details(&format!("return CampaignUI.FortDetails(1, '{MAP_FORT1}')"))), (MAP_FORT1.to_owned(), 1, 1));
        // A region address names that region's map fort, whatever is selected -- and nothing when
        // no fort stands there (review: it described the level-0 fort).
        let region = region_value(&hud.host.campaign_ui().unwrap(), REGION);
        let f = lua.create_function(move |_, ()| Ok(region.clone())).unwrap();
        lua.globals().set("__test_region", f).unwrap();
        hud.host.campaign_select(CampaignSelection::Fort(REGION));
        assert_eq!(key_level(&details("return CampaignUI.FortDetails(__test_region())")), (String::new(), 0, 0));
        {
            let mut st = hud._scripts.state_mut();
            let fort = ntw_sim::campaign::Fort { id: ntw_sim::campaign::FortId(1), region: REGION, position: None, key: MAP_FORT1.into() };
            st.model.world.forts.insert(fort.id, fort);
        }
        hud.host.campaign_select(CampaignSelection::None);
        assert_eq!(key_level(&details("return CampaignUI.FortDetails(__test_region())")), (MAP_FORT1.to_owned(), 1, 1));
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        assert_eq!(key_level(&details("return CampaignUI.FortDetails(__test_region(), '')")), (MAP_FORT1.to_owned(), 1, 1));
    }

    /// A tab a state function asks for is generated on pointer events that fire no Lua event but
    /// run state functions too: a release elsewhere, and a click on a disabled state (both still
    /// transition, `0x0102E340`).
    #[test]
    fn pointer_events_without_a_lua_event_still_generate_a_requested_tab() {
        use super::super::world::PointerEvent;
        for (event, old_disabled) in [(PointerEvent::LeftUpElsewhere, false), (PointerEvent::LeftUp, true)] {
            let hud = test_hud();
            // A settlement: construction (current) and infrastructure (2), both generated by
            // GenerateConstructionPanel, which records `generated`; the state function asks for tab 2.
            hud.host.campaign_select(CampaignSelection::Settlement(REGION));
            assert_eq!(hud.tabs()[1], "infrastructure_tab");
            let env = hud.root_env();
            hud.host.lua().load("function GenerateConstructionPanel(info) generated = true end").set_environment(env.clone()).exec().unwrap();
            let button = hud.host.world().find(hud.root, "button").unwrap();
            super::super::host::tests::add_state_transition(&hud.host, button, event, |s| {
                s[0].disabled = old_disabled;
                s[1].enter_function = "PickTab".into();
            });
            let benv = super::super::host::tests::component_env(&hud.host, button);
            hud.host.lua().load("function PickTab() CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(2) end").set_environment(benv).exec().unwrap();
            hud.host.pointer(button, event);
            assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 2, "{event:?}");
            assert_eq!(env.get::<Option<bool>>("generated").unwrap(), Some(true), "{event:?}: the selected tab was generated");
            assert!(hud.errors().is_empty());
        }
    }

    /// The card template indexes `{"neutral", "ally", "foe"}` with the answer and drops the badge on 0.
    #[test]
    fn relationship_answers_index_the_card_badge_states() {
        use ntw_sim::campaign::Stance;
        assert_eq!(relationship_to_players_faction(true, Stance::War), 0);
        assert_eq!(relationship_to_players_faction(false, Stance::Neutral), 1);
        for s in [Stance::Allied, Stance::Protectorate, Stance::Patron] {
            assert_eq!(relationship_to_players_faction(false, s), 2);
        }
        assert_eq!(relationship_to_players_faction(false, Stance::War), 3);
        let hud = test_hud();
        let r: Value = hud.host.lua().load("return CampaignUI.CharactersRelationshipToPlayersFaction(1)").eval().unwrap();
        assert!(r.is_nil(), "a non-character address answers nil");
        assert!(hud.errors().is_empty(), "the call is bound, not an UNKNOWN stub");
    }

    /// The agent panel's `CampaignUI` calls are bound as PROVISIONAL no-ops: they exist (no UNKNOWN
    /// stub), queue nothing, and `CanAgentEmbarkOrDisembark` answers false.
    #[test]
    fn agent_panel_calls_are_bound_as_no_ops() {
        let hud = test_hud();
        hud.host.take_log();
        let lua = hud.host.lua();
        for name in
            ["AgentCardSelectionChanged", "AgentEmbarkOrDisembark", "AgentGentlemanDuel", "AgentRakeAssassinate", "AgentRakeSubterfuge", "AgentRogueSabotageArmy"]
        {
            let r: Value = lua.load(format!("return CampaignUI.{name}(1, 'x')")).eval().unwrap();
            assert!(r.is_nil(), "{name}");
        }
        assert!(!lua.load("return CampaignUI.CanAgentEmbarkOrDisembark(1)").eval::<bool>().unwrap());
        assert!(hud.host.take_campaign_requests().is_empty());
        assert!(hud.errors().is_empty(), "no UNKNOWN stub was reached");
        // A settlement with no agent has no agents tab (INFERRED condition, see `tabs_for`).
        hud.tabs();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        assert!(!hud.tabs().contains(&"agents_tab".to_owned()));
    }

    const OTHER: &str = "test_faction_b";
    const RAKE: CharacterId = CharacterId(100);
    const FOREIGN_GENTLEMAN: CharacterId = CharacterId(101);
    const GENERAL: CharacterId = CharacterId(102);
    const OWN_GENTLEMAN: CharacterId = CharacterId(103);

    impl TestHud {
        /// Puts a made-up character into the model; `abilities` become his saved `AgentAbilities`.
        fn add_character(&self, id: CharacterId, faction: FactionId, kind: CharacterKind, garrisoned_in: Option<RegionId>, abilities: &[(&str, i32)]) {
            use ntw_sim::fixed::Fixed20;
            let mut st = self._scripts.state_mut();
            let w = &mut st.model.world;
            if !w.factions.contains_key(&B) {
                let mut f = w.factions[&A].clone();
                f.id = B;
                f.key = OTHER.into();
                w.factions.insert(B, f);
            }
            let position = w.regions[&REGION].settlement.position;
            w.characters.insert(
                id,
                ntw_sim::campaign::Character {
                    id,
                    faction,
                    kind,
                    position: if garrisoned_in.is_some() { position } else { (Fixed20::from_int(500), Fixed20::from_int(500)) },
                    movement_points: 10,
                    max_movement_points: 10,
                    base_movement_points: 10,
                    garrisoned_in,
                },
            );
            if !abilities.is_empty() {
                let d = w.character_details.entry(id).or_default();
                d.abilities = abilities.iter().map(|(k, l)| ((*k).to_owned(), *l, String::new())).collect();
            }
        }

        /// Opens review-panel tab `key` of the current selection, as a click on it does.
        fn open_tab(&self, key: &str) {
            let ui = self.host.campaign_ui().unwrap();
            let i = ui.tabs.borrow().iter().position(|t| t.key() == key).expect("tab listed") + 1;
            self.host.lua().load(format!("CampaignUI.ReviewPanelTabSelectionSet_1_Indexed({i})")).exec().unwrap();
        }

        /// Evaluates `CampaignUI.<name>(<character address>)`.
        fn ask(&self, name: &str, c: CharacterId) -> Value {
            let lua = self.host.lua();
            let f: Function = lua.load(format!("return function(a) return CampaignUI.{name}(a) end")).eval().unwrap();
            f.call(self.char_addr(c)).unwrap()
        }
    }

    /// Makes the root record, in its `order` list, the calls a review-panel tab change makes
    /// (`clear`, `init <index>=<state>`, `gen <tab>` .. `end <tab>` around tab 1's
    /// (construction) and tab 2's (infrastructure) generator, both `GenerateConstructionPanel`, told apart by the
    /// index just opened), `entity` for `SetSelectedEntity`).
    /// The generators call the globals `on_gen1` / `on_gen2` when a test sets them.
    fn record_tab_calls(hud: &TestHud) {
        hud.host
            .lua()
            .load(
                "order = {}\n\
                 local function note(s) order[#order + 1] = s end\n\
                 function ClearReviewPanel() note('clear') end\n\
                 function ReviewPanelTabInit(title, i, state) note('init ' .. i .. '=' .. state) if state == 2 then opening = i end end\n\
                 function GenerateConstructionPanel(info)\n\
                     local i = opening\n\
                     note('gen ' .. i)\n\
                     local hook = (i == 1 and on_gen1) or (i == 2 and on_gen2)\n\
                     if hook then hook() end\n\
                     note('end ' .. i)\n\
                 end\n\
                 function SetSelectedEntity(e, name) note('entity') end\n\
                 function request(i) CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(i) end",
            )
            .set_environment(hud.root_env())
            .exec()
            .unwrap();
    }

    /// The calls [`record_tab_calls`] recorded since the last call; drained.
    fn take_tab_calls(hud: &TestHud) -> Vec<String> {
        let env = hud.root_env();
        let order: Vec<String> = env.get::<Table>("order").unwrap().sequence_values().map(Result::unwrap).collect();
        env.set("order", hud.host.lua().create_table().unwrap()).unwrap();
        order
    }

    /// A tab request takes effect inside its call, on the current selection's tabs (as the exe's
    /// `0x00A20620` does): an index out of range or the tab already current is ignored; in range
    /// the review panel is cleared, the old tab deselected (state 1), the new one selected
    /// (state 2) and generated, before the call returns. Each request does that (there and back
    /// again rebuilds both). The tab is kept by its identity when the same selection is refreshed;
    /// a different selection opens its own first tab.
    #[test]
    fn a_tab_request_applies_to_the_selection_it_was_made_for() {
        let hud = test_hud();
        // A second settlement of the same faction, so that both selections have several tabs.
        const OTHER_REGION: RegionId = RegionId(12);
        {
            let mut st = hud._scripts.state_mut();
            let mut r = st.model.world.regions[&REGION].clone();
            r.id = OTHER_REGION;
            r.key = "test_region_12".into();
            r.settlement.key = "settlement:test_region_12:town".into();
            st.model.world.regions.insert(OTHER_REGION, r);
        }
        hud.host.campaign_select(CampaignSelection::Settlement(OTHER_REGION));
        assert!(hud.tabs().len() >= 2, "{:?}", hud.tabs());
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let tabs = hud.tabs();
        assert_eq!(tabs[..2], ["construction_tab", "infrastructure_tab"]);
        record_tab_calls(&hud);
        let ui = hud.host.campaign_ui().unwrap();
        let request = |i: usize| hud.host.lua().load(format!("CampaignUI.ReviewPanelTabSelectionSet_1_Indexed({i})")).exec().unwrap();
        request(tabs.len() + 1);
        assert_eq!((ui.current_tab.get(), take_tab_calls(&hud)), (1, vec![]), "out of range: ignored");
        request(2);
        assert_eq!(ui.current_tab.get(), 2);
        assert_eq!(take_tab_calls(&hud), ["clear", "init 1=1", "init 2=2", "gen 2", "end 2"], "inside the call");
        request(2);
        assert!(take_tab_calls(&hud).is_empty(), "the tab already current: no rebuild");
        request(1);
        request(2);
        assert_eq!(
            take_tab_calls(&hud),
            ["clear", "init 2=1", "init 1=2", "gen 1", "end 1", "clear", "init 1=1", "init 2=2", "gen 2", "end 2"],
            "there and back again: both rebuilt"
        );
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        assert_eq!(ui.tabs.borrow()[ui.current_tab.get() - 1].key(), tabs[1], "the same selection keeps that tab");
        hud.host.campaign_select(CampaignSelection::Settlement(OTHER_REGION));
        assert_eq!(ui.current_tab.get(), 1, "a different selection opens its first tab");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// [`record_tab_calls`], plus the tab-list calls of a selection change: `tabs clear`
    /// (`ClearReviewPanelTabs`), `hud` (`ClearHud`) and `create <index>=<state>`
    /// (`CreateReviewPanelTabAtPosition`).
    fn record_selection_calls(hud: &TestHud) {
        record_tab_calls(hud);
        hud.host
            .lua()
            .load(
                "local function note(s) order[#order + 1] = s end\n\
                 function ClearReviewPanelTabs() note('tabs clear') end\n\
                 function ClearHud() note('hud') end\n\
                 function CreateReviewPanelTabAtPosition(title, key, i, state) note('create ' .. i .. '=' .. state) end",
            )
            .set_environment(hud.root_env())
            .exec()
            .unwrap();
    }

    /// A selection change makes the exe's calls in the exe's order (round 15, CONFIRMED): no
    /// `ClearReviewPanel`; `ClearReviewPanelTabs` and `ClearHud` every time (regression: ClearHud
    /// only on a deselection); per tab its creation, then either its opening (the tab kept on a
    /// refresh) or `ReviewPanelTabInit(.., 1)` (regression: one ReviewPanelTabInit for the current
    /// tab after all the creations); the first tab opened after the list when none was kept;
    /// `SetSelectedEntity` last.
    #[test]
    fn a_selection_change_builds_its_tabs_in_the_exes_order() {
        let hud = test_hud();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let n = hud.tabs().len();
        assert!(n >= 2);
        hud.host.lua().load("CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(2)").set_environment(hud.root_env()).exec().unwrap();
        record_selection_calls(&hud);
        let rest = |from: usize| (from..=n).flat_map(|i| [format!("create {i}=1"), format!("init {i}=1")]).collect::<Vec<_>>();

        // A refresh keeps tab 2 (infrastructure): opened at its addition.
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let mut want: Vec<String> = ["tabs clear", "hud", "create 1=1", "init 1=1", "create 2=1", "init 2=2", "gen 2", "end 2"].map(String::from).to_vec();
        want.extend(rest(3));
        want.push("entity".into());
        assert_eq!(take_tab_calls(&hud), want);
        assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 2);

        // Another selection: the first tab opened after the whole list.
        hud.host.campaign_select(CampaignSelection::None);
        assert_eq!(take_tab_calls(&hud), ["tabs clear", "hud"], "a deselection");
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let mut want: Vec<String> = ["tabs clear", "hud"].map(String::from).to_vec();
        want.extend(rest(1));
        want.extend(["init 1=2", "gen 1", "end 1", "entity"].map(String::from));
        assert_eq!(take_tab_calls(&hud), want);
        assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 1);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// A tab request made while a selection change builds the tab list (from ClearHud, a tab's
    /// creation, or the opened tab's own generator) is refused, as the exe has no tab set then
    /// (its request would read a null pointer, round 15), and logged once per HUD (regression:
    /// silently ignored, or acted on mid-build). A request after the change works.
    #[test]
    fn a_tab_request_while_the_tab_list_is_built_is_refused_and_logged_once() {
        let hud = test_hud();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        record_selection_calls(&hud);
        hud.host
            .lua()
            .load(
                "local hud_ = ClearHud\nfunction ClearHud() hud_() request(2) end\n\
                 local create = CreateReviewPanelTabAtPosition\nfunction CreateReviewPanelTabAtPosition(...) create(...) request(2) end\n\
                 function on_gen1() request(2) end",
            )
            .set_environment(hud.root_env())
            .exec()
            .unwrap();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let calls = take_tab_calls(&hud);
        assert!(!calls.iter().any(|c| c == "gen 2" || c == "clear"), "no request took effect: {calls:?}");
        assert_eq!(calls.iter().filter(|c| *c == "gen 1").count(), 1, "{calls:?}");
        assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 1);
        let errors = hud.errors();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("null tab set"), "{errors:?}");
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        assert!(hud.errors().is_empty(), "logged once");

        hud.host.lua().load("on_gen1 = nil request(2)").set_environment(hud.root_env()).exec().unwrap();
        assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 2, "after the change, a request works");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// A generator that asks for another tab during a tab change: that change nests inside the
    /// first one (as in the exe), each tab generated once, nothing left pending.
    #[test]
    fn a_generator_asking_for_a_tab_during_a_tab_change_gets_it_inside_that_change() {
        let hud = test_hud();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        record_tab_calls(&hud);
        hud.host.lua().load("function on_gen2() on_gen2 = nil request(1) end").set_environment(hud.root_env()).exec().unwrap();
        hud.host.lua().load("request(2)").set_environment(hud.root_env()).exec().unwrap();
        assert_eq!(
            take_tab_calls(&hud),
            ["clear", "init 1=1", "init 2=2", "gen 2", "clear", "init 2=1", "init 1=2", "gen 1", "end 1", "end 2"]
        );
        assert_eq!(hud.host.campaign_ui().unwrap().current_tab.get(), 1);
        hud.host.hover(None);
        assert!(take_tab_calls(&hud).is_empty(), "nothing pending at the event's end");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// Generators that keep asking for each other's tab nest without a limit of ours (the exe has
    /// none) until Lua's nested C-call limit stops them with its error, which is logged; the HUD
    /// keeps working afterwards.
    #[test]
    fn an_endless_tab_ping_pong_ends_with_luas_error_not_a_crash() {
        let hud = test_hud();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        record_tab_calls(&hud);
        let env = hud.root_env();
        hud.host.lua().load("function on_gen1() request(2) end\nfunction on_gen2() request(1) end").set_environment(env.clone()).exec().unwrap();
        hud.host.lua().load("request(2)").set_environment(env.clone()).exec().unwrap();
        let errors = hud.errors();
        assert!(!errors.is_empty() && errors.iter().all(|e| e.contains("C stack overflow")), "{errors:?}");
        assert!(take_tab_calls(&hud).len() > 20, "it nested many times first");
        env.set("on_gen1", Value::Nil).unwrap();
        env.set("on_gen2", Value::Nil).unwrap();
        let next = 3 - hud.host.campaign_ui().unwrap().current_tab.get();
        hud.host.lua().load(format!("request({next})")).set_environment(env).exec().unwrap();
        assert_eq!(take_tab_calls(&hud).last().map(String::as_str), Some(if next == 1 { "end 1" } else { "end 2" }));
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The agents tab: listed last when the settlement has an agent, and its info is the shape
    /// `ui/agents.luac` reads (CONFIRMED): parallel `agents` / `characters` lists, `card_id` a string,
    /// `name`, the character details with `Abilities` and `IsGuerilla`, and a `controlable` table.
    #[test]
    fn agents_tab_lists_the_settlements_agents_with_the_info_the_panel_reads() {
        let hud = test_hud();
        // A general alone is no agent: no tab.
        hud.add_character(GENERAL, A, CharacterKind::General, Some(REGION), &[]);
        hud.tabs();
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        assert!(!hud.tabs().contains(&"agents_tab".to_owned()));

        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[]);
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        let tabs = hud.tabs();
        assert_eq!(tabs.last().map(String::as_str), Some("agents_tab"), "the agents tab comes last: {tabs:?}");
        hud.open_tab("agents_tab");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());

        let info: Table = hud.root_env().get("agents_panel").expect("GenerateAgentsPanel was called");
        let agents: Table = info.get("agents").unwrap();
        let characters: Table = info.get("characters").unwrap();
        assert!(info.get::<Table>("controlable").is_ok());
        assert_eq!((agents.raw_len(), characters.raw_len()), (2, 2), "the two agents, not the general");
        let card = |i: usize| -> (String, String, String, Table) {
            let a: Table = agents.get(i).unwrap();
            (a.get("card_id").unwrap(), a.get("name").unwrap(), a.get("agent_type_name").unwrap(), characters.get(i).unwrap())
        };
        let (id1, name1, type1, rake) = card(1);
        let (id2, _, _, gentleman) = card(2);
        assert_eq!((id1.as_str(), id2.as_str()), ("agent_100", "agent_101"), "unique string card ids");
        assert_eq!(name1, rake.get::<String>("Name").unwrap());
        // `agents[i].agent_type_name`, the second of the only two fields `InitialiseAgent` reads.
        // This fixture has no localisation and no cultures, so the fallback (the ESF type name)
        // shows; the loc key and the culture are the install test's business.
        assert_eq!(type1, CharacterKind::Rake.esf_name());
        // Parallel lists: characters[i] is agents[i]'s character.
        assert_eq!(entity_of(&rake.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(RAKE.0));
        assert_eq!(entity_of(&gentleman.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(FOREIGN_GENTLEMAN.0));
        // The details the card reads, and the buttons' fields.
        for key in ["Flag", "SmallFlag", "CommanderType", "Attributes"] {
            assert!(!rake.get::<Value>(key).unwrap().is_nil(), "{key}");
        }
        assert!(!rake.get::<bool>("IsGuerilla").unwrap());
        let abilities = |t: &Table| -> Vec<(String, bool)> {
            let a: Table = t.get("Abilities").unwrap();
            AGENT_BUTTON_ABILITIES.iter().map(|k| ((*k).to_owned(), a.get::<bool>(*k).unwrap())).collect()
        };
        // No saved abilities: from the type (INFERRED mapping).
        let on = |v: Vec<(String, bool)>| v.into_iter().filter(|x| x.1).map(|x| x.0).collect::<Vec<_>>();
        assert_eq!(on(abilities(&rake)), ["can_assassinate", "can_sabotage", "can_sabotage_army"]);
        assert_eq!(on(abilities(&gentleman)), ["can_research", "can_duel"]);

        // Saved abilities win: a rake whose save gives only `can_assassinate`.
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2), ("can_sabotage", -1)]);
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        hud.open_tab("agents_tab");
        let info: Table = hud.root_env().get("agents_panel").unwrap();
        let rake: Table = info.get::<Table>("characters").unwrap().get(1).unwrap();
        assert_eq!(on(abilities(&rake)), ["can_assassinate"]);
        assert!(hud.errors().is_empty());
    }

    /// The questions `ShowAgentButtons` asks with the agent's Address, from the model.
    #[test]
    fn agent_button_questions_answer_from_the_model() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
        hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
        hud.add_character(OWN_GENTLEMAN, A, CharacterKind::Gentleman, Some(REGION), &[("can_receive_duel", 1)]);
        hud.host.take_log();
        let truth = |name: &str, c: CharacterId| match hud.ask(name, c) {
            Value::Boolean(b) => b,
            v => panic!("{name} answered {v:?}"),
        };
        // The residence: the settlement the agent is garrisoned in (its region's address).
        assert_eq!(entity_of(&hud.ask("CharacterResidence", RAKE), TAG_REGION), Some(REGION.0 as i32));
        assert!(!truth("CharacterInEnemyResidence", RAKE), "his own settlement");
        assert!(truth("CharacterInEnemyResidence", FOREIGN_GENTLEMAN), "another faction's settlement");
        assert!(!truth("IsCharacterInPortResidence", RAKE));
        // Assassination: the rake has a known foreign target the model's gate accepts.
        assert!(truth("ValidAssassinationTargets", RAKE));
        assert!(!truth("ValidAssassinationTargets", FOREIGN_GENTLEMAN), "a gentleman is no spy");
        // A duel: the foreign gentleman meets one who may receive it in the same residence.
        assert!(truth("ValidDuelTargetsInResidence", FOREIGN_GENTLEMAN));
        assert!(!truth("ValidDuelTargetsInResidence", RAKE), "no can_duel");
        // Nothing to base a yes on: no forces, no buildings, no schools.
        assert!(!truth("ValidSabotageArmyTarget", RAKE));
        assert!(!truth("ValidSabotageTarget", RAKE));
        assert!(!truth("CharacterInValidEnemyUniversity", FOREIGN_GENTLEMAN));
        // An address that is not a character answers false / nil.
        let lua = hud.host.lua();
        assert!(lua.load("return CampaignUI.CharacterResidence(1)").eval::<Value>().unwrap().is_nil());
        assert!(!lua.load("return CampaignUI.ValidAssassinationTargets(1)").eval::<bool>().unwrap());

        // A port: an agent standing on a port slot's position is in that slot's residence.
        {
            let mut st = hud._scripts.state_mut();
            let w = &mut st.model.world;
            let at = (ntw_sim::fixed::Fixed20::from_int(40), ntw_sim::fixed::Fixed20::from_int(0));
            w.regions.get_mut(&REGION).unwrap().slots.push(ntw_sim::campaign::RegionSlot {
                key: "port:test_region_10:harbour".into(),
                slot_type: "port".into(),
                building: None,
                position: Some(at),
                port: true,
                holder: None,
                id: 7,
            });
            let ch = w.characters.get_mut(&RAKE).unwrap();
            ch.garrisoned_in = None;
            ch.position = at;
        }
        assert!(truth("IsCharacterInPortResidence", RAKE));
        let ui = hud.host.campaign_ui().unwrap();
        assert_eq!(slot_from_entity(&ui, &hud.ask("CharacterResidence", RAKE)), Some((REGION, SlotRef::Slot(0))));
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
        assert!(hud.host.take_campaign_requests().is_empty(), "questions queue nothing");
    }

    /// The agents panel's hover tooltip reads `agent.name` and `agent.agent_type_name`
    /// (CONFIRMED, `template.unitcard_tooltip.lua:115`), so the tab's info must carry both.
    #[test]
    fn the_agents_info_carries_the_hover_tooltip_fields() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        hud.host.campaign_select(CampaignSelection::Settlement(REGION));
        hud.open_tab("agents_tab");
        let info: Table = hud.root_env().get("agents_panel").expect("the panel ran");
        let a: Table = info.get::<Table>("agents").unwrap().get(1).unwrap();
        assert_eq!(a.get::<String>("name").unwrap(), "rake", "the agent type's on-screen name");
        assert_eq!(a.get::<String>("agent_type_name").unwrap(), "rake");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The agent action popups: `RequestDuelTargets` / `RequestAssassinationTargets` /
    /// `RequestSabotageTargets` answer the model's own target lists (CONFIRMED arity 2 and the
    /// fields of a row, `agent_options.lua:109/118/128` and `agent_action.lua:21`), and
    /// `InstigateDuel` / `InstigateAssassination` / `InstigateSabotage` / `SabotageArmy` queue the
    /// model's commands.
    #[test]
    fn the_agent_action_calls_answer_the_model_and_queue_its_commands() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
        hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
        hud.add_character(OWN_GENTLEMAN, A, CharacterKind::Gentleman, Some(REGION), &[("can_receive_duel", 1)]);
        hud.host.take_log();
        let call2 = |name: &str, a: Value, b: Value| -> Value {
            let lua = hud.host.lua();
            let f: Function = lua.load(format!("return function(a, b) return CampaignUI.{name}(a, b) end")).eval().unwrap();
            f.call::<Value>((a, b)).unwrap()
        };
        // The assassination list: the foreign gentleman the model's gate accepts, as rows. The
        // address field is `Address`, CONFIRMED from both shipped row templates -- it is NOT `target`
        // (see [`TargetRow`]); `Name`, `Chance`, `Flag` and `Attributes` are read too, and
        // `Utilities.CreateCharacterCard` concatenates `Flag` unguarded.
        let list = call2("RequestAssassinationTargets", hud.char_addr(RAKE), hud.char_addr(FOREIGN_GENTLEMAN));
        let rows: Table = list.as_table().expect("a list of targets").clone();
        assert_eq!(rows.raw_len(), 1, "one known foreign target the model's gate accepts");
        let row: Table = rows.get(1).unwrap();
        assert_eq!(entity_of(&row.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(FOREIGN_GENTLEMAN.0));
        // `Faction.FlagPath` is the flag **folder** -- `agent_action.lua:21` pc 37-40 appends
        // "/small.tga" to it -- and `Faction.Key`/`Name` are what the duel pane's stance line reads.
        let fac: Table = row.get("Faction").unwrap();
        assert_eq!(fac.get::<String>("Key").unwrap(), OTHER);
        assert!(!fac.get::<String>("Name").unwrap().is_empty());
        assert!(fac.get::<String>("FlagPath").is_ok());
        assert!(row.get::<String>("Flag").unwrap().ends_with("/small.tga"), "the character card's flag");
        assert!(!row.get::<String>("Name").unwrap().is_empty(), "the target's name");
        assert!(row.get::<i64>("Chance").unwrap() > 0, "the model's own percentage");
        assert!(row.get::<Table>("Attributes").is_ok(), "the card indexes Attributes unguarded");
        // A gentleman may not assassinate: an empty list, and the popup is then not opened.
        assert_eq!(call2("RequestAssassinationTargets", hud.char_addr(FOREIGN_GENTLEMAN), hud.char_addr(RAKE)).as_table().unwrap().raw_len(), 0);
        // Duel targets: the foreign gentleman (who may duel) meets the A-side gentleman in the same
        // residence, who may receive it.
        let duels = call2("RequestDuelTargets", hud.char_addr(FOREIGN_GENTLEMAN), hud.char_addr(OWN_GENTLEMAN));
        assert_eq!(duels.as_table().unwrap().raw_len(), 1);
        let row: Table = duels.as_table().unwrap().get(1).unwrap();
        assert_eq!(entity_of(&row.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(OWN_GENTLEMAN.0));
        // The duel row's `Chance` is the number the model's duel is rolled at: the target picks the
        // weapon worse for the challenger (`duel_weapon`), so the smaller of the two chances. Give
        // the challenger pistols skill only: pistols would read 95, swords reads 50, the row 50.
        hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().attributes =
            vec![("duelling_pistols".into(), 6)];
        let row: Table = call2("RequestDuelTargets", hud.char_addr(FOREIGN_GENTLEMAN), hud.char_addr(OWN_GENTLEMAN)).as_table().unwrap().get(1).unwrap();
        {
            use ntw_sim::campaign::agents::{Weapon, duel_chance};
            let m = &hud._scripts.state().model;
            let pistols = duel_chance(m, FOREIGN_GENTLEMAN, OWN_GENTLEMAN, Weapon::Pistols).unwrap();
            let swords = duel_chance(m, FOREIGN_GENTLEMAN, OWN_GENTLEMAN, Weapon::Swords).unwrap();
            assert!(pistols > swords, "the fixture must make the weapons differ: {pistols} vs {swords}");
            assert_eq!(row.get::<i64>("Chance").unwrap(), i64::from(swords), "the weapon the target would pick");
        }
        // No building to sabotage in this settlement: an empty list.
        let sab = call2("RequestSabotageTargets", hud.char_addr(RAKE), hud.char_addr(FOREIGN_GENTLEMAN));
        assert_eq!(sab.as_table().unwrap().raw_len(), 0);
        // An address that is not a character answers an empty list.
        assert_eq!(call2("RequestDuelTargets", Value::Integer(1), Value::Integer(2)).as_table().unwrap().raw_len(), 0);

        // The actions themselves queue the model's commands.
        let queued = hud.host.take_campaign_requests();
        assert!(queued.is_empty(), "the target lists queue nothing: {queued:?}");
        call2("InstigateDuel", hud.char_addr(OWN_GENTLEMAN), hud.char_addr(FOREIGN_GENTLEMAN));
        call2("InstigateAssassination", hud.char_addr(RAKE), hud.char_addr(FOREIGN_GENTLEMAN));
        call2("SabotageArmy", hud.char_addr(RAKE), hud.force_addr(ntw_sim::campaign::ForceId(4242)));
        let queued = hud.host.take_campaign_requests();
        let cmds: Vec<CampaignCommand> = queued.into_iter().filter_map(|r| match r {
            CampaignRequest::Command(c) => Some(c),
            _ => None,
        }).collect();
        assert_eq!(
            cmds,
            vec![
                CampaignCommand::Duel { challenger: OWN_GENTLEMAN, target: FOREIGN_GENTLEMAN },
                CampaignCommand::Assassinate { agent: RAKE, target: FOREIGN_GENTLEMAN },
                // A force that does not exist: the command is queued anyway and the model refuses it.
                CampaignCommand::SabotageArmy { agent: RAKE, force: ntw_sim::campaign::ForceId(4242) },
            ]
        );
        // A target of the wrong kind queues nothing.
        call2("InstigateDuel", hud.char_addr(OWN_GENTLEMAN), hud.region_addr(REGION));
        assert!(hud.host.take_campaign_requests().is_empty());
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The agent action mask. `agent_options.Initialise` shows a button per bit and **divorces** the
    /// ones whose bit is clear (`agent_options.lua:0` pc 45-56), so a bit that is not set is a
    /// button that never appears -- which is why the five `MoveIntoTarget` actions are left out
    /// rather than faked (see [`agent_options_mask`]).
    #[test]
    fn the_agent_options_mask_is_the_models_own_gates() {
        let hud = test_hud();
        // A rake who may assassinate, a foreign gentleman in the same residence who may duel, and a
        // French gentleman who may receive one -- the fixture of the call test above.
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
        hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
        hud.add_character(OWN_GENTLEMAN, A, CharacterKind::Gentleman, Some(REGION), &[("can_receive_duel", 1)]);
        let mask = |c: CharacterId| {
            let st = hud._scripts.state();
            let m = &st.model;
            agent_options_mask(m, c, m.world.characters[&c].faction)
        };
        use action_bit::*;
        // The rake has exactly one assassination candidate and no building to sabotage.
        assert_eq!(mask(RAKE), ASSASSINATE, "one known foreign target the model's gate accepts");
        // The foreign gentleman has one duel partner in his residence and nothing else.
        assert_eq!(mask(FOREIGN_GENTLEMAN), DUEL);
        // Our own gentleman is nobody's target: no bits at all, so the popup shows no button.
        assert_eq!(mask(OWN_GENTLEMAN), 0);
        // The five `MoveIntoTarget` bits and the dead `counterspy` bit are never set, whatever the
        // model says -- `MoveIntoTarget` is a logging stub, so a button for one of them would be a
        // button that does nothing.
        let never = VISIT | EMBED | RESEARCH | STEAL_RESEARCH | COUNTERSPY;
        for c in [RAKE, FOREIGN_GENTLEMAN, OWN_GENTLEMAN] {
            assert_eq!(mask(c) & never, 0, "the MoveIntoTarget actions stay out of the mask");
        }
    }

    /// The agents panel's three action buttons open the target picker, and an action with no valid
    /// target says so in the log instead of doing nothing silently (CONFIRMED arity 1 for each call,
    /// `ui/agents.luac`; the three lines it stands for, `agent_options.lua:109/118/128`).
    ///
    /// The fixture root script does not define `OpenAgentActionPopup` (the real
    /// `layout.root.luac` does, `layout.root.lua:1187`, and the install test
    /// `an_agent_action_button_opens_the_target_picker` drives that), so here only the gate is
    /// checked: with a valid target the call reaches the root, without one it logs.
    #[test]
    fn an_agent_action_button_reports_itself_when_there_is_no_target() {
        let hud = test_hud();
        // A rake alone in his own residence: no foreign character, so no assassination candidate.
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
        hud.host.take_log();
        let lua = hud.host.lua();
        for name in ["AgentRakeAssassinate", "AgentRakeSubterfuge", "AgentGentlemanDuel"] {
            let f: Function = lua
                .load(format!("return function(a) return CampaignUI.{name}(a) end"))
                .eval()
                .unwrap();
            f.call::<mlua::MultiValue>(hud.char_addr(RAKE)).unwrap();
        }
        let log = hud.host.take_log();
        for name in ["assassinate", "sabotage", "duel"] {
            assert!(log.iter().any(|l| l == &format!("agent action {name}: no valid target")), "{log:?}");
        }
        // The two remaining gaps say which they are, so a click is never a silent no-op.
        for name in ["AgentRogueSabotageArmy", "MoveIntoTarget"] {
            let f: Function = lua
                .load(format!("return function(...) return CampaignUI.{name}(...) end"))
                .eval()
                .unwrap();
            f.call::<mlua::MultiValue>(mlua::MultiValue::new()).unwrap();
        }
        let log = hud.host.take_log();
        assert!(log.iter().any(|l| l.contains("no army target list")), "{log:?}");
        assert!(log.iter().any(|l| l.contains("MoveIntoTarget is not implemented")), "{log:?}");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The address representation, which is the thing `agent_options.lua:34` reads
    /// (`string.find(tostring(target), "CHARACTER")`, pc 2-9). CONFIRMED against the exe, so these
    /// are the original's own rules rather than ours (`UI_FIDELITY.md` 9):
    ///
    /// - an address is a **userdata with a metatable**, not light userdata, because it has to be able
    ///   to answer `__tostring`;
    /// - `__tostring` is `sprintf("%s (0x0%x)", metatable.type, pointer)` (`0x01058F60`), and
    ///   `metatable.type` is the C++ signature string the exe's registration interns for its
    ///   `Lua::Pointer<T>` binding -- which for a character contains `CHARACTER`;
    /// - `__eq` compares the wrapped pointers (`0x01058F20`), so `==` is **identity**. Lua tables
    ///   compare by reference, so the interning per `(tag, id)` is what makes `==` come out right:
    ///   the same entity must be the *same* table, and two different entities must not be.
    #[test]
    fn an_address_stringifies_with_the_originals_type_name_and_compares_by_identity() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        let lua = hud.host.lua();
        let fn_of = |src: &str| lua.load(src).eval::<Function>().unwrap();

        // `tostring` reaches the `__tostring` metamethod and prints the original's format, with the
        // exe's own type string in place 1 and `0x0` + the pointer in place 2.
        let printed: String = fn_of("return function(a) return tostring(a) end").call(hud.char_addr(RAKE)).unwrap();
        assert_eq!(
            printed,
            format!("{} (0x0{:x})", address_type_name(TAG_CHARACTER), TAG_CHARACTER | RAKE.0 as u32 as usize),
            "the original's __tostring format, verbatim"
        );
        assert!(printed.contains("CHARACTER"), "and that is what agent_options.lua:34 tests for: {printed}");
        assert!(printed.starts_with("class UTILITYDLL::LUA::State &__thiscall"), "the type name is the exe's own signature string: {printed}");

        // `==` is identity, in both directions: the same entity is the same table even across two
        // separate calls, and two entities are never the same.
        let same: bool = fn_of("return function(a) local b = a return a == b end").call::<bool>(hud.char_addr(RAKE)).unwrap();
        assert!(same, "one address compared with itself");
        let two_calls: bool =
            fn_of("return function(a, b) return a == b end").call::<bool>((hud.char_addr(RAKE), hud.char_addr(RAKE))).unwrap();
        assert!(two_calls, "two separate builds of the same entity's address are interned to one table");
        let different: bool =
            fn_of("return function(a, b) return a == b end").call::<bool>((hud.char_addr(RAKE), hud.char_addr(OWN_GENTLEMAN))).unwrap();
        assert!(!different, "two different characters are never the same address");
        let cross_kind: bool =
            fn_of("return function(a, b) return a == b end").call::<bool>((hud.char_addr(RAKE), hud.region_addr(REGION))).unwrap();
        assert!(!cross_kind, "a character address is not a region address");

        // And the round trip that all of the above exists for: the shipped type test now matches, so
        // `agent_options.Initialise` builds its `CampaignCharacter` handle. CONFIRMED the branch is
        // `string.find(tostring(target), "CHARACTER") and CampaignCharacter(target) or nil`
        // (pc 2-21), and the only reader of the handle is the popup's teardown, which calls
        // `Release()` (the proto at line 89).
        let handle_is_nil: bool =
            fn_of(r#"return function(a) return string.find(tostring(a), "CHARACTER") == nil end"#).call::<bool>(hud.char_addr(RAKE)).unwrap();
        assert!(!handle_is_nil, "agent_options.lua:34's type test now takes the character branch");
        let handle_is_nil: bool =
            fn_of(r#"return function(a) return string.find(tostring(a), "CHARACTER") == nil end"#).call::<bool>(hud.region_addr(REGION)).unwrap();
        assert!(handle_is_nil, "and a region address still takes the other branch, as in the original");
    }

    /// The interning itself, and that the payload every `CampaignUI.*` binding recovers the id from
    /// still round trips -- including from a row's `Address` field, which is how the shipped row
    /// templates hand an address back (`character_duel_info_pane.lua:52/57`).
    #[test]
    fn one_entity_keeps_the_same_address_object_and_its_payload_round_trips() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        let first = hud.char_addr(RAKE);
        let second = hud.char_addr(RAKE);
        assert!(
            matches!((&first, &second), (Value::Table(a), Value::Table(b)) if a == b),
            "interned: one table per (tag, id)"
        );
        assert_eq!(entity_of(&first, TAG_CHARACTER), Some(RAKE.0));
        let row: Table = hud.host.lua().create_table().unwrap();
        row.set("Address", first.clone()).unwrap();
        assert_eq!(entity_of(&row.get::<Value>("Address").unwrap(), TAG_CHARACTER), Some(RAKE.0), "the Address field round trips");
        assert_eq!(entity_of(&first, TAG_REGION), None, "and the tag still decides which kind it is");
    }

    /// Round 5's item 1, as a test: the agent action menu's target is a **character or a
    /// settlement** and nothing else. The reading is CONFIRMED from the exe's own registration
    /// document for `CampaignUI.MoveIntoTarget` (see [`AgentMenuTarget`]), so this pins the two
    /// kinds and the refusal of everything else rather than a guess.
    #[test]
    fn the_agent_menu_target_is_a_character_or_a_settlement() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        let m = &hud._scripts.state().model;
        assert_eq!(menu_target_of(m, &hud.char_addr(RAKE)), Some(AgentMenuTarget::Character(RAKE)));
        assert_eq!(menu_target_of(m, &hud.region_addr(REGION)), Some(AgentMenuTarget::Settlement(REGION)));
        // Neither an unknown character nor a non-address is a target: the original's menu is opened
        // for a thing that is there, and each of the two engine call sites tests its target first.
        let ghost = entity_payload(TAG_CHARACTER, 4242);
        assert_eq!(menu_target_of(m, &ghost), None, "a character id that is not in the world");
        assert_eq!(menu_target_of(m, &Value::Table(hud.host.lua().create_table().unwrap())), None);
        assert_eq!(menu_target_of(m, &Value::Integer(1)), None);
        assert_eq!(menu_target_of(m, &Value::Nil), None);
        assert_eq!(menu_target_of(m, &hud.fort_addr(REGION)), None, "a fort is not one of the two documented kinds");
    }

    /// Round 5's item 2, as a test: the two percentages `agent_options.Initialise` puts on the
    /// Infiltrate and Sabotage Army buttons are the model's own success chances -- `spy_chance` on the
    /// target settlement and `army_sabotage_chance` on the target's force -- and not the two zeros the
    /// engine call used to be handed. See [`agent_options_percentages`] for what is CONFIRMED here and
    /// what is INFERRED (which force a target names).
    #[test]
    fn the_two_agent_menu_percentages_are_the_models_own_success_chances() {
        use ntw_sim::campaign::ForceId;
        use ntw_sim::campaign::agents::{SpyTarget, army_sabotage_chance, spy_chance};
        let hud = test_hud();
        // The rake is A's, in A's own settlement; a second settlement belongs to B, which is the only
        // kind of target either chance accepts (`spy_chance` refuses the agent's own faction).
        const FOREIGN_REGION: RegionId = RegionId(11);
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_sabotage_army", 1), ("can_spy", 1)]);
        {
            let mut st = hud._scripts.state_mut();
            let mut r = st.model.world.regions[&REGION].clone();
            r.id = FOREIGN_REGION;
            r.key = "test_region_11".into();
            r.settlement.key = "settlement:test_region_11:town".into();
            r.owner = B;
            r.settlement.position = (ntw_sim::fixed::Fixed20::from_int(140), ntw_sim::fixed::Fixed20::from_int(40));
            r.garrison = None;
            r.fleet = None;
            r.fortification = None;
            st.model.world.regions.insert(FOREIGN_REGION, r);
        }
        // A foreign army standing in that settlement, under a known colonel: the target a Sabotage
        // Army percentage is read off.
        let force = ForceId(5150);
        let colonel = CharacterId(140);
        hud.add_character(colonel, B, CharacterKind::Colonel, Some(FOREIGN_REGION), &[]);
        {
            let mut st = hud._scripts.state_mut();
            let w = &mut st.model.world;
            let u = ntw_sim::campaign::CampaignUnit {
                id: ntw_sim::campaign::UnitId(80),
                unit_key: "test_unit".into(),
                men: 100,
                max_men: 100,
                character: Some(colonel),
            };
            w.forces.insert(force, ntw_sim::campaign::MilitaryForce {
                id: force,
                faction: B,
                commander: Some(colonel),
                units: vec![u],
                is_navy: false,
            });
        }
        let m = &hud._scripts.state().model;
        let pcts = |t| agent_options_percentages(m, RAKE, A, t);

        // A settlement target: the Infiltrate percentage is the model's spying chance on it, and the
        // Sabotage Army percentage is that army's.
        let (infiltrate, sabotage_army) = pcts(Some(AgentMenuTarget::Settlement(FOREIGN_REGION)));
        assert_eq!(infiltrate, spy_chance(m, RAKE, SpyTarget::Settlement(FOREIGN_REGION)).expect("the rake may spy on it"));
        assert_eq!(sabotage_army, army_sabotage_chance(m, RAKE, force).expect("the rake may sabotage that army"));

        // A character target names no settlement, so there is no Infiltrate percentage to show -- and
        // naming the army's own commander does find the force.
        let (infiltrate_by_character, sabotage_by_commander) = pcts(Some(AgentMenuTarget::Character(colonel)));
        assert_eq!(infiltrate_by_character, 0);
        assert_eq!(sabotage_by_commander, sabotage_army);

        // No target: nothing to compute from, and nothing invented.
        assert_eq!(pcts(None), (0, 0));
    }

    /// Round 5's audit of the address representation: what the metatable does **not** carry, and the
    /// one kind that was missing a name.
    ///
    /// The original's address metatable is exactly `type` + `__tostring` + `__eq`, with **no
    /// `__index`** (`0x0105AE10`'s raw listing, `UI_FIDELITY.md` 9.2). So an address is not a table of
    /// methods: reading a field off one gives nil, and there is no metamethod to catch a script that
    /// indexes one. Ours answers nil for the same reads, so every operation a shipped script performs
    /// on an address gives the same answer -- the scripts only stringify one (`agent_options.lua:34`,
    /// and `string.find` over that), and never index or iterate it. **PROVISIONAL difference:** a
    /// script that indexed an address would *raise* in the original and read nil here; a script that
    /// `pairs`'d one would raise there and walk the single payload field here. Neither happens in any
    /// shipped `.luac`, and this records it rather than pretending it away.
    ///
    /// The second half is the audit's one real find: `FactionDetails`' `Address` had no signature of
    /// its own and stringified as the `void` stand-in. It now prints the exe's own
    /// `EMPIRECAMPAIGN::FACTION` signature (`0x01371770`).
    #[test]
    fn an_address_has_no_index_and_a_faction_address_names_its_own_kind() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        let lua = hud.host.lua();
        // The shipped way to reach an address's kind: `Utilities.CreateCharacterCard` and friends do
        // `tostring(address)`; nothing reads a field off one, because there is no `__index` to read.
        let printed: String = lua
            .load("return function(a) return tostring(a) end")
            .eval::<Function>()
            .unwrap()
            .call(hud.char_addr(RAKE))
            .unwrap();
        assert!(printed.contains("EMPIRECAMPAIGN::CHARACTER"), "{printed}");
        assert!(
            !printed.contains("operator <<<void>"),
            "a kind we have a signature for never falls back to the void stand-in: {printed}"
        );
        // Reading an unknown field off an address is nil, not a method call: there is no `__index`.
        let fields_are_nil: bool = lua
            .load("return function(a) return a.Release == nil and a[1] == nil and a.Address ~= nil end")
            .eval::<Function>()
            .unwrap()
            .call(hud.char_addr(RAKE))
            .unwrap();
        assert!(fields_are_nil, "no __index on an address; only the payload field is readable");

        // `FactionDetails`' `Address`, read through the same `__tostring` the diplomacy panel would.
        let details: Table = lua.load("return CampaignUI.FactionDetails('test_faction_a')").eval().unwrap();
        let addr: Value = details.get("Address").expect("Address is a CONFIRMED field of FactionDetails");
        let printed: String =
            lua.load("return function(a) return tostring(a) end").eval::<Function>().unwrap().call(addr.clone()).unwrap();
        assert!(printed.contains("EMPIRECAMPAIGN::FACTION"), "0-E round 5: {printed}");
        assert_eq!(entity_of(&addr, TAG_FACTION), Some(A.0));
        assert_eq!(entity_of(&addr, TAG_FORCE), None, "and it is not a force address any more");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The commander pool panel: `CanRecruitCommander(force, is_navy)` and
    /// `AvailableCommandersForRecruitment(force, is_navy)` (CONFIRMED arity 2,
    /// `enlist_commander.lua:38` / `army.lua:708`), and `PromoteUnits(force, candidate)` queues the
    /// hire (`enlist_commander.lua:106`).
    #[test]
    fn the_commander_pool_answers_from_the_model_and_hiring_queues_the_command() {
        use ntw_sim::campaign::ForceId;
        let hud = test_hud();
        // An army of the human faction under a colonel, and a General candidate in his pool.
        let force = ForceId(4242);
        {
            let mut st = hud._scripts.state_mut();
            let elapsed = st.model.calendar.turns_elapsed;
            let w = &mut st.model.world;
            if !w.factions.contains_key(&B) {
                let mut f = w.factions[&A].clone();
                f.id = B;
                f.key = OTHER.into();
                w.factions.insert(B, f);
            }
            w.faction_details.entry(A).or_default().capital = Some(REGION);
            w.characters.insert(CharacterId(120), ntw_sim::campaign::Character {
                id: CharacterId(120),
                faction: A,
                kind: CharacterKind::Colonel,
                position: w.regions[&REGION].settlement.position,
                movement_points: 10,
                max_movement_points: 10,
                base_movement_points: 10,
                garrisoned_in: Some(REGION),
            });
            let mut u = ntw_sim::campaign::CampaignUnit { id: UnitId(70), unit_key: "test_unit".into(), men: 100, max_men: 100, character: Some(CharacterId(120)) };
            u.men = 100;
            w.forces.insert(force, ntw_sim::campaign::MilitaryForce { id: force, faction: A, commander: Some(CharacterId(120)), units: vec![u], is_navy: false });
            let c = CharacterId(121);
            w.characters.insert(c, ntw_sim::campaign::Character {
                id: c,
                faction: A,
                kind: CharacterKind::General,
                position: w.regions[&REGION].settlement.position,
                movement_points: 10,
                max_movement_points: 10,
                base_movement_points: 10,
                garrisoned_in: None,
            });
            w.faction_details.get_mut(&A).unwrap().general_pool = (vec![c], elapsed + 2);
        }
        hud.host.take_log();
        let call2 = |name: &str, a: Value, b: Value| -> Value {
            let lua = hud.host.lua();
            let f: Function = lua.load(format!("return function(a, b) return CampaignUI.{name}(a, b) end")).eval().unwrap();
            f.call::<Value>((a, b)).unwrap()
        };
        let ask_gate = |f: ForceId, navy: bool| -> bool {
            let lua = hud.host.lua();
            let fun: Function = lua.load("return function(f, n) return CampaignUI.CanRecruitCommander(f, n) end").eval().unwrap();
            fun.call::<bool>((hud.force_addr(f), navy)).unwrap()
        };
        // The model's gate: the treasury is 1000 and the candidate is affordable. A campaign that
        // has not started yet lets the faction act (`may_act`), so it answers true.
        assert!(ask_gate(force, false));
        assert!(!ask_gate(ForceId(999), false), "no such force");
        {
            let mut st = hud._scripts.state_mut();
            st.model.turn.humans = vec![A];
            st.model.start_campaign();
        }
        assert!(ask_gate(force, false));
        // The second argument is redundant for us: the pool follows the force's own kind (an army
        // takes generals), so asking with `true` gives the same answer as with `false`.
        assert!(ask_gate(force, true));
        // Another faction's turn, and a purse too small for the candidate (the state borrow is
        // released before each question).
        {
            let mut st = hud._scripts.state_mut();
            st.model.turn.current = Some(B);
        }
        assert!(!ask_gate(force, false), "not A's turn");
        {
            let mut st = hud._scripts.state_mut();
            st.model.turn.current = Some(A);
            st.model.world.factions.get_mut(&A).unwrap().treasury = 1;
        }
        assert!(!ask_gate(force, false), "the treasury cannot pay");
        {
            let mut st = hud._scripts.state_mut();
            st.model.world.factions.get_mut(&A).unwrap().treasury = 1000;
        }

        let list = call2("AvailableCommandersForRecruitment", hud.force_addr(force), Value::Boolean(false));
        let t = list.as_table().expect("the pool table").clone();
        assert_eq!(t.raw_len(), 1, "one candidate");
        let row: Table = t.get(1).unwrap();
        assert_eq!(entity_of(&row.get::<Value>("commander_pointer").unwrap(), TAG_CHARACTER), Some(121));
        assert_eq!(row.get::<String>("Name").unwrap(), "General", "the agent type's on-screen name");
        assert!(row.get::<String>("RecruitmentCost").unwrap().parse::<i32>().unwrap() > 0);
        assert!(row.get::<bool>("IsRecruitable").unwrap(), "the treasury can pay");
        assert_eq!(t.get::<i32>("TurnsToNextPoolFill").unwrap(), 2);
        assert!(t.get::<f32>("MaxDistanceToTrack").unwrap() > 0.0);
        assert!(t.get::<i32>("MaxGeneralsAllowed").unwrap() > t.get::<i32>("CurrentNumGenerals").unwrap());
        // No force, no table.
        assert!(call2("AvailableCommandersForRecruitment", hud.force_addr(ForceId(999)), Value::Boolean(false)).is_nil());

        // The player confirms the candidate: the hire is queued.
        call2("PromoteUnits", hud.force_addr(force), hud.char_addr(CharacterId(121)));
        let cmds: Vec<CampaignCommand> = hud
            .host
            .take_campaign_requests()
            .into_iter()
            .filter_map(|r| match r {
                CampaignRequest::Command(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(cmds, vec![CampaignCommand::HireGeneral { character: CharacterId(121), into: Some(force) }]);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// `CanPromoteUnit(card address)` (CONFIRMED arity 1, `army.lua:692`) and the unit row's
    /// `PromotionCost` (CONFIRMED field, `army.lua:825`).
    #[test]
    fn the_promote_gate_and_price_come_from_the_model() {
        use ntw_sim::campaign::{ForceId, MilitaryForce};
        let hud = test_hud();
        let force = ForceId(4242);
        let unit = UnitId(70);
        {
            use ntw_sim::campaign::effects::SavedBonus;
            let mut st = hud._scripts.state_mut();
            let w = &mut st.model.world;
            w.faction_details.entry(A).or_default().capital = Some(REGION);
            w.characters.insert(CharacterId(120), ntw_sim::campaign::Character {
                id: CharacterId(120),
                faction: A,
                kind: CharacterKind::Colonel,
                position: w.regions[&REGION].settlement.position,
                movement_points: 10,
                max_movement_points: 10,
                base_movement_points: 10,
                garrisoned_in: Some(REGION),
            });
            w.forces.insert(force, MilitaryForce {
                id: force,
                faction: A,
                commander: Some(CharacterId(120)),
                units: vec![ntw_sim::campaign::CampaignUnit { id: unit, unit_key: "test_unit".into(), men: 100, max_men: 100, character: Some(CharacterId(120)) }],
                is_navy: false,
            });
            w.faction_details.get_mut(&A).unwrap().bonus_base = vec![SavedBonus { kind: 1, bonus: 64, value: 1.0, qualifier: String::new() }];
            st.model.turn.humans = vec![A];
            st.model.start_campaign();
        }
        hud.host.take_log();
        hud.host.campaign_select(CampaignSelection::Character(CharacterId(120)));
        let lua = hud.host.lua();
        let gate = |u: UnitId| -> bool {
            let f: Function = lua.load("return function(u) return CampaignUI.CanPromoteUnit(u) end").eval().unwrap();
            f.call::<bool>(hud.unit_addr(u)).unwrap()
        };
        assert!(gate(unit), "the faction may promote in the field and has no General yet");
        assert!(!gate(UnitId(999)), "no such unit");
        assert!(!lua.load("return CampaignUI.CanPromoteUnit(1)").eval::<bool>().unwrap(), "not a unit address");
        // The army panel's unit row carries the promotion price and what the player may know of it.
        hud.tabs();
        hud.host.campaign_select(CampaignSelection::Character(CharacterId(120)));
        hud.open_tab("army_tab");
        let info: Table = hud.root_env().get("army_panel").expect("GenerateArmyPanel ran");
        let units: Table = info.get::<Table>("units_info").unwrap().get("Units").unwrap();
        let row: Table = units.get(1).unwrap();
        assert!(row.get::<i32>("PromotionCost").unwrap() > 0, "the field promotion has a price");
        assert_eq!(row.get::<i32>("spying_data_level").unwrap(), LEVEL_OWNED);
        assert_eq!(row.get::<i32>("knowledge_mask").unwrap(), KNOWLEDGE_OWNED);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
        // A **naval** promotion is free (CONFIRMED: the naval unit class's slot +0x44 is a return-0
        // stub, so `0x008E2260` pays `0x00BAF500(0, 2)`), and the row must not show a price for it:
        // `army.lua:825`'s `SelectedUnitsPromotionCost` sums the rows, so a non-zero value here
        // would put a price on the button for a promotion the model charges nothing for.
        let ship = UnitId(71);
        {
            let mut st = hud._scripts.state_mut();
            let w = &mut st.model.world;
            w.characters.insert(
                CharacterId(121),
                ntw_sim::campaign::Character {
                    id: CharacterId(121),
                    faction: A,
                    kind: CharacterKind::Captain,
                    position: w.regions[&REGION].settlement.position,
                    movement_points: 10,
                    max_movement_points: 10,
                    base_movement_points: 10,
                    garrisoned_in: Some(REGION),
                },
            );
            w.forces.insert(
                ForceId(4243),
                ntw_sim::campaign::MilitaryForce {
                    id: ForceId(4243),
                    faction: A,
                    commander: Some(CharacterId(121)),
                    units: vec![ntw_sim::campaign::CampaignUnit { id: ship, unit_key: "test_ship".into(), men: 10, max_men: 10, character: Some(CharacterId(121)) }],
                    is_navy: true,
                },
            );
            // The admiral-at-sea effect the naval gate needs (`promote_admiral_at_sea`, bonus 65); the
            // difficulty handicap (faction +0x8D4) would otherwise win over the base list.
            let d = w.faction_details.get_mut(&A).unwrap();
            d.bonus_with_difficulty = Vec::new();
            d.bonus_base.push(ntw_sim::campaign::effects::SavedBonus { kind: 1, bonus: 65, value: 1.0, qualifier: String::new() });
        }
        assert!(hud.host.lua().load("return function(u) return CampaignUI.CanPromoteUnit(u) end").eval::<Function>().unwrap().call::<bool>(hud.unit_addr(ship)).unwrap());
        hud.host.campaign_select(CampaignSelection::Character(CharacterId(121)));
        hud.open_tab("navy_tab");
        let info: Table = hud.root_env().get("army_panel").expect("GenerateNavyPanel ran");
        // A navy's rows are under `Ships` (CONFIRMED: `force_info` fills `Units` or `Ships`).
        let ships: Table = info.get::<Table>("units_info").unwrap().get("Ships").unwrap();
        let row: Table = ships.get(1).unwrap();
        assert_eq!(row.get::<i32>("PromotionCost").unwrap(), 0, "a naval promotion is free: no price on the row");
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// The fog of war as the interface sees it: `SpyingDataLevelCharacter` /
    /// `SpyingDataLevelUnit` (CONFIRMED arity 1 and the `>= ADVANCED` gate the root's double click
    /// uses, `layout.root.lua:1093`), the levels `utilities.lua` defines (CONFIRMED values) and the
    /// model's own knowledge.
    #[test]
    fn a_unit_of_the_players_own_force_with_no_commander_is_owned() {
        let hud = test_hud();
        let unit = UnitId(81);
        {
            let mut st = hud._scripts.state_mut();
            let force = ntw_sim::campaign::ForceId(5151);
            st.model.world.forces.insert(force, ntw_sim::campaign::MilitaryForce {
                id: force,
                faction: A,
                commander: None,
                units: vec![ntw_sim::campaign::CampaignUnit { id: unit, unit_key: "test_unit".into(), men: 10, max_men: 10, character: None }],
                is_navy: false,
            });
        }
        let m = &hud._scripts.state().model;
        assert_eq!(spying_level_unit(m, A, unit), LEVEL_OWNED, "own force, no commander");
        assert_eq!(spying_level_unit(m, B, unit), LEVEL_PASSIVE, "a foreign commanderless force is unchanged");
    }

    #[test]
    fn the_spying_data_levels_follow_the_models_sight() {
        let hud = test_hud();
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[]);
        hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[]);
        hud.host.take_log();
        let level = |name: &str, a: Value| -> i32 {
            let lua = hud.host.lua();
            let f: Function = lua.load(format!("return function(a) return CampaignUI.{name}(a) end")).eval().unwrap();
            f.call::<i32>(a).unwrap()
        };
        // One's own character is OWNED; the foreign one stands in the settlement the player sees, so
        // it is at least ADVANCED. No shroud in the test model: `sees` is true everywhere.
        assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(RAKE)), LEVEL_OWNED);
        assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(FOREIGN_GENTLEMAN)), LEVEL_ADVANCED);
        // A character the faction does not know about (a hidden spy) is only PASSIVE.
        {
            use ntw_sim::campaign::visibility::{CellSet, Shroud, SightGrid};
            let mut st = hud._scripts.state_mut();
            let w = &mut st.model.world;
            let ch = w.characters.get_mut(&FOREIGN_GENTLEMAN).unwrap();
            ch.position = (ntw_sim::fixed::Fixed20::from_int(900), ntw_sim::fixed::Fixed20::from_int(900));
            // A shroud that has never seen anything: the foreign character is unknown.
            let grid = SightGrid::centred(8, 8, 8);
            w.sight_grid = Some(grid);
            let visible = {
                let mut s = CellSet::new(8, 8);
                let cells: Vec<(u32, u32)> = w
                    .characters
                    .values()
                    .filter(|c| c.faction == A)
                    .flat_map(|c| grid.disc((c.position.0.to_f32(), c.position.1.to_f32()), 1.0))
                    .collect();
                for (x, z) in cells {
                    s.set(x, z);
                }
                s
            };
            w.shrouds.insert(A, Shroud { explored: visible.clone(), visible, hidden: CellSet::new(8, 8), active: true });
        }
        assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(RAKE)), LEVEL_OWNED);
        // Known, but under the shroud: only the basic level (its card may not be opened).
        assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(FOREIGN_GENTLEMAN)), LEVEL_BASIC);
        // Hidden: the faction does not know him at all, so even the list leaves him out.
        hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().hidden = true;
        assert_eq!(level("SpyingDataLevelCharacter", hud.char_addr(FOREIGN_GENTLEMAN)), LEVEL_PASSIVE);
        hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().hidden = false;
        // The unit of a force the player has never seen is unknown as well.
        {
            use ntw_sim::campaign::{ForceId, MilitaryForce};
            let mut st = hud._scripts.state_mut();
            let w = &mut st.model.world;
            w.forces.insert(ForceId(77), MilitaryForce {
                id: ForceId(77),
                faction: B,
                commander: Some(FOREIGN_GENTLEMAN),
                units: vec![ntw_sim::campaign::CampaignUnit { id: UnitId(78), unit_key: "test_unit".into(), men: 10, max_men: 10, character: None }],
                is_navy: false,
            });
        }
        assert_eq!(level("SpyingDataLevelUnit", hud.unit_addr(UnitId(78))), LEVEL_BASIC);
        hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().hidden = true;
        assert_eq!(level("SpyingDataLevelUnit", hud.unit_addr(UnitId(78))), LEVEL_PASSIVE);
        // Not a character / unit address at all.
        assert_eq!(level("SpyingDataLevelCharacter", hud.region_addr(REGION)), LEVEL_INVALID);
        assert_eq!(level("SpyingDataLevelUnit", Value::Integer(1)), LEVEL_INVALID);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }
    /// The `Attributes` table of a character card ([`attributes_table`]): the primary attribute is
    /// the character type's main attribute (`agents::main_attribute`, the exe's `0x00A198C0`), not
    /// the highest one; `PrimaryLevel` is his rank + 1, at most 9 (`0x009AE759..0x009AE768`); a
    /// `PLACEHOLDER` picture row gives the empty path; the pips keep their own pictures. The
    /// `agent_attributes` rows are MADE UP (the fixture has none).
    #[test]
    fn character_cards_show_the_main_attribute_and_its_rank() {
        use ntw_data::characters::AgentAttributeRecord;
        let mut db = GameDatabase::test_fixture();
        db.campaign.characters.attributes = ntw_data::Table::from_rows(
            0,
            vec![
                AgentAttributeRecord { key: "research".into(), icon: "PLACEHOLDER".into() },
                AgentAttributeRecord { key: "duelling_pistols".into(), icon: "made/up/pistols.tga".into() },
                AgentAttributeRecord { key: "subterfuge".into(), icon: "made/up/spying.tga".into() },
            ],
        );
        let hud = test_hud_with(db);
        hud.add_character(RAKE, A, CharacterKind::Rake, Some(REGION), &[("can_assassinate", 2)]);
        hud.add_character(FOREIGN_GENTLEMAN, B, CharacterKind::Gentleman, Some(REGION), &[("can_duel", 1)]);
        let attributes = |target: CharacterId| -> Table {
            let lua = hud.host.lua();
            let f: Function = lua.load("return function(a, b) return CampaignUI.RequestAssassinationTargets(a, b) end").eval().unwrap();
            let rows: Table = f.call((hud.char_addr(RAKE), hud.char_addr(target))).unwrap();
            let row: Table = rows.get(1).expect("the gentleman is a target");
            row.get("Attributes").unwrap()
        };
        let set_attributes = |list: Vec<(String, i32)>| {
            hud._scripts.state_mut().model.world.character_details.entry(FOREIGN_GENTLEMAN).or_default().attributes = list;
        };

        // Pistols is his highest attribute, but a gentleman's main attribute is research: the
        // primary is research (rank 3 -> level 4), whose row is `PLACEHOLDER` -> the empty path.
        set_attributes(vec![("duelling_pistols".into(), 6), ("research".into(), 3)]);
        let a = attributes(FOREIGN_GENTLEMAN);
        assert_eq!(a.get::<String>("PrimaryAttributeName").unwrap(), "research");
        assert_eq!(a.get::<i64>("PrimaryLevel").unwrap(), 4);
        assert_eq!(a.get::<String>("PrimaryAttributePath").unwrap(), "", "a PLACEHOLDER row names no file");
        let pip: Table = a.get(1).unwrap();
        assert_eq!(pip.get::<String>("PipPath").unwrap(), "made/up/pistols.tga");
        assert_eq!(pip.get::<i64>("Value").unwrap(), 6);

        // A tie (and the old fallback) cannot pick another attribute: still research.
        set_attributes(vec![("duelling_pistols".into(), 3), ("research".into(), 3)]);
        assert_eq!(attributes(FOREIGN_GENTLEMAN).get::<String>("PrimaryAttributeName").unwrap(), "research");
        // Rank 9 -> level 10, clamped to 9.
        set_attributes(vec![("research".into(), 9)]);
        assert_eq!(attributes(FOREIGN_GENTLEMAN).get::<i64>("PrimaryLevel").unwrap(), 9);
        // No attribute list at all: still research (not `command_land`), rank -1 -> level 0.
        set_attributes(Vec::new());
        let a = attributes(FOREIGN_GENTLEMAN);
        assert_eq!(a.get::<String>("PrimaryAttributeName").unwrap(), "research");
        assert_eq!(a.get::<i64>("PrimaryLevel").unwrap(), 0);
        assert!(hud.errors().is_empty(), "{:?}", hud.errors());
    }

    /// Polish: `CampaignUi::addresses` kept the address of every queue item ever shown. A finished
    /// or cancelled item's address is dropped at the next panel build; a queued item keeps its own
    /// table (identity).
    #[test]
    fn finished_queue_items_leave_the_address_store() {
        let hud = test_hud();
        let item = |id: i32| ntw_sim::campaign::RecruitmentItem { id: RecruitmentItemId(id), unit_key: "x".into(), turns_remaining: 1, cost: 0 };
        hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().recruitment_queue = vec![item(500), item(501)];
        let ui = hud.host.campaign_ui().unwrap();
        let build = || recruitment_info(hud.host.lua(), hud.host.inner(), &ui, REGION, false).unwrap();
        let held = |id: i32| ui.addresses.borrow().get(&(TAG_QUEUE_ITEM, id)).cloned();
        build();
        let kept = held(501).expect("shown, so interned");
        assert!(held(500).is_some());
        hud._scripts.state_mut().model.world.regions.get_mut(&REGION).unwrap().recruitment_queue.remove(0);
        build();
        assert!(held(500).is_none(), "the finished item's address is gone");
        assert_eq!(held(501), Some(kept), "the queued item keeps its own table");
    }
}

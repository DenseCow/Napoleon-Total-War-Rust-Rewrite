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

use mlua::{Function, IntoLuaMulti, LightUserData, Lua, MultiValue, Table, Value, Variadic};
use ntw_data::GameDatabase;
use ntw_formats::db_folder::{RawTable, tables};
use ntw_sim::campaign::{
    CampaignCommand, CampaignModel, CharacterId, CharacterKind, ConstructionOption, ForceId, FactionId, MilitaryForce, RecruitmentItemId, RegionId,
    SlotRef, UnitId, economy, treasury,
};

use super::host::{CallTarget, Inner, PostedCall, UiScriptHost, addr, call_entry, log, state_function_guard};
use super::world::{NodeId, UiRect};
use crate::ScriptState;

const PRELUDE: &str = include_str!("../campaign_prelude.lua");

/// One `CampaignUI.*` binding: sets `$name` in the `CampaignUI` table `$t` to a function that
/// converts its arguments to `$ty` and runs `$body` with clones of the host's `$inner0` and the
/// HUD's `$ui0`. Each screen's `install` defines a local `f!` that passes its own `t`, `lua`,
/// `inner` and `ui` here.
macro_rules! campaign_fn {
    ($t:ident, $lua0:ident, $inner0:ident, $ui0:ident; $name:literal, |$lua:ident, $inner:ident, $ui:ident, $args:tt : $ty:ty| $body:expr) => {{
        let $inner = $inner0.clone();
        let $ui = $ui0.clone();
        // The arguments are converted here, not by mlua, so a wrong-typed argument names the
        // binding in the error ("CampaignUI.X: error converting Lua boolean to String").
        $t.raw_set($name, $lua0.create_function(move |$lua, mv: mlua::MultiValue| {
            let $args: $ty = <$ty as mlua::FromLuaMulti>::from_lua_multi(mv, $lua)
                .map_err(|e| mlua::Error::runtime(format!("CampaignUI.{}: {e}", $name)))?;
            #[allow(unused_variables)]
            let (_, _) = (&$inner, &$ui);
            $body
        })?)?;
    }};
}

mod agents;
mod army;
mod characters;
mod diplomacy;
mod government;
mod map;
mod region_info;
mod settlement;
mod tabs;
mod technology;
#[cfg(test)]
mod tests;

use agents::*;
use army::*;
use characters::*;
use diplomacy::*;
use map::*;
use settlement::*;
use tabs::*;
use technology::*;

/// What the HUD needs from the running campaign.
pub struct CampaignLink {
    /// The campaign script host's state (it owns the model). Only borrowed while a `CampaignUI`
    /// function runs; never while the campaign scripts run.
    pub state: Rc<RefCell<ScriptState>>,
    /// The human player's faction key.
    pub human: String,
    /// Campaign key, e.g. `eur_napoleon`.
    pub campaign: String,
    /// The campaign's map key, e.g. `nap_europe` (from its campaign source, `ntw_campaign::source`).
    pub map: String,
    /// The campaign's theatres (area keys, e.g. `europe_main`; [`ntw_campaign::CampaignInfo::theatres`]):
    /// the first is its home theatre.
    pub theatres: Vec<String>,
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
    /// A navy's recruitment tab. The tab's key is CONFIRMED as the exe string
    /// `naval_recruitment_tab` (0x013CC71C, beside `recruitment_tab` 0x013CC70C). It is the same
    /// recruitment tab object as an army's (`0x0098BEA0`, built by the navy tab set `0x009990B0`),
    /// whose generator `GenerateRecruitmentPanel` (`0x009FE7B0`) takes its commander path: what makes
    /// it naval is the commander's navy, which picks the ports' queues as sources (`0x00B624C0`;
    /// UI_FIDELITY.md §4.10). The registration's bool `+0xA4` is not a land/naval selector: it is the
    /// tab set's byte +0x24 (manager +0xA94), the same for every tab. UNKNOWN: `CampaignShipCard`, a
    /// template no shipped script references -- whether the naval cards are drawn with it stays open
    /// (§4.5 item 6).
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
/// A row of the agents list (`RetrieveFactionAgentsList`): character, agent type, action points,
/// action points per turn, map position.
type AgentRow = (CharacterId, CharacterKind, i32, i32, (f32, f32));

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
    /// The technology tree's links (see [`TechLinks`]), read once.
    tech_links: std::cell::OnceCell<TechLinks>,
    /// `ministerial_positions_by_gov_types` rows (faction, post, government, _, string key), read
    /// once (see [`character_details`]).
    post_names: std::cell::OnceCell<crate::source::SharedRows>,
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
    /// See `CampaignUi::religion_icon`.
    religion_icons: std::cell::OnceCell<HashMap<String, String>>,
    /// See `CampaignUi::order_factor_pip`.
    order_factor_pips: std::cell::OnceCell<HashMap<String, (String, String)>>,
    /// See `CampaignUi::town_factor_pip`.
    town_factor_pips: std::cell::OnceCell<HashMap<String, String>>,
    /// See `CampaignUi::culture_fallback`.
    portrait_folders: std::cell::OnceCell<HashMap<String, String>>,
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

/// [`loc`] for a key the data must have: a missing string gives "" and is logged once per key.
fn loc_required(inner: &Inner, key: &str) -> String {
    loc(inner, key).unwrap_or_else(|| {
        inner.log_once_for("missing loc string", key, || format!("UNKNOWN no loc string {key:?}: shown as \"\" (logged once per key)"));
        String::new()
    })
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

/// A character's name as `BuildCharacterDisplayName` (`0x00A0FE80`, CONFIRMED) builds it: the
/// forename, then " " + the surname when there is one, then " " + the regnal numeral when there is
/// one ("George III"). Forename and surname are loc keys (`names_name_names_frenchNapoléon` =
/// "Napoléon", CONFIRMED); the numeral is plain text (`CHARACTER_DETAILS` #4). `None` if the model
/// has no names for it (CAMPAIGN_DATA.md §3).
fn character_name(inner: &Inner, ui: &CampaignUi, c: CharacterId) -> Option<String> {
    let m = ui.model();
    let d = m.world.character_details.get(&c)?;
    let part = |k: &str| if k.is_empty() { None } else { Some(loc(inner, k).unwrap_or_else(|| k.to_owned())) };
    let numeral = (!d.regnal_numeral.is_empty()).then(|| d.regnal_numeral.clone());
    let name = [part(&d.forename), part(&d.surname), numeral].into_iter().flatten().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some(name)
}

/// A character's portrait card as the UI shows it: "data/" + his card picture (PORTRAIT_DETAILS
/// #0, e.g. `ui/portraits/european/Cards/...`), or empty when the model has none for him (the
/// army card then keeps its unit picture). Why he has none, or why it is wrong
/// ([`CampaignModel::portrait_problem`], as the agent type his portrait resolves as), is logged
/// once per character and problem: a new problem (a promotion to an agent type without a folder)
/// is logged again; a character without one is not remembered.
fn portrait_image(inner: &Inner, m: &CampaignModel, c: CharacterId) -> String {
    let agent = m.portrait_agent(c);
    if let Some(why) = m.portrait_problem(c, agent) {
        // The id is formatted into a stack buffer: a character with a problem is drawn every frame.
        let mut buf = [0u8; 20];
        let mut cursor = std::io::Cursor::new(&mut buf[..]);
        let _ = std::io::Write::write_fmt(&mut cursor, format_args!("{}", c.0));
        let len = cursor.position() as usize;
        let id = std::str::from_utf8(&buf[..len]).unwrap_or_default();
        inner.log_once_for_pair("character portrait", why, id, || {
            format!("WARN character {} ({agent}): {why} (logged once)", c.0)
        });
    }
    match m.world.character_details.get(&c).map(|d| d.portrait.card.as_str()) {
        Some(card) if !card.is_empty() => format!("data/{card}"),
        _ => String::new(),
    }
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
/// (`FUN_009e2cb0` builds the table, `FUN_009af1a0` the faction fields). PROVISIONAL: colours as
/// {r, g, b} (sub-table keys UNKNOWN), VictoryConditions empty. The rankings are strings
/// (CONFIRMED, `GetFactionRankingStrings` 0x008C7170 clears them to "" and fills them from
/// `BuildFactionRankingTable` 0x00949630, rebuilt on every call): the model's categories
/// ([`ntw_sim::campaign::CampaignModel::faction_rankings`]) named
/// `random_localisation_strings_string_{power,wealth,prestige}_category_<c+1>` ("Terrifying" ...
/// "Feeble"; UI_FIDELITY.md 4.7).
fn faction_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Value> {
    let m = ui.model();
    let Some(f) = m.faction_by_key(key) else { return Ok(Value::Nil) };
    let t = lua.create_table()?;
    t.set("Key", key)?;
    t.set("Address", faction_value(ui, f.id))?;
    let rankings = m.faction_rankings();
    let ranks = rankings.ranks.get(&f.id).copied();
    drop(m);
    // The power counts a unit with no `units` record as 0: logged once per unit key.
    for unit in &rankings.unknown_units {
        inner.log_once_for("ranking unknown unit", unit, || {
            format!("UNKNOWN faction power: no units record for {unit:?}, counted as 0 (logged once per unit)")
        });
    }
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
    // A faction the ranking leaves out (the rebels) keeps the "" the exe starts from.
    let rank = |kind: &str, c: Option<u8>| {
        c.map(|c| loc_required(inner, &format!("random_localisation_strings_string_{kind}_category_{}", c + 1))).unwrap_or_default()
    };
    t.set("WealthRanking", rank("wealth", ranks.map(|r| r.wealth)))?;
    t.set("PowerRanking", rank("power", ranks.map(|r| r.power)))?;
    t.set("PrestigeRanking", rank("prestige", ranks.map(|r| r.prestige)))?;
    // Leader: the holder of the faction_leader post (CAMPAIGN_DATA.md §3), as a character details
    // table (0x009AD250, see `character_details`: its CardImage / InfoImage carry the portraits);
    // absent when the faction has no leader (CONFIRMED test 0x008CF600).
    let lid = {
        let m = ui.model();
        m.faction_by_key(key).and_then(|f| m.world.faction_leader(f.id))
    };
    if let Some(c) = lid
        && let Value::Table(leader) = character_details(lua, inner, ui, c)?
    {
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

// The CampaignUI table.

fn install_functions(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

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
    // HUD's clock) and never pauses. Between two pulses it holds the last pulse's time, so a host
    // that pulses rarely must advance the time it pulses with (`examples/campaign_hud_probe.rs`
    // adds a second per step). The result is a 32-bit float as in the exe (`(float)ms * 0.001f`).
    f!("Time", |_l, inner, ui, _a: Variadic<Value>| Ok(inner.ui_time_secs()));
    // WindowsTime(): "the current windows time in seconds", CONFIRMED `0x009FB0B0`: whole seconds,
    // `(int)(timeGetTime() * 0.001f)` pushed as a number ([`super::host::windows_time_secs`]).
    f!("WindowsTime", |_l, inner, ui, _a: Variadic<Value>| Ok(super::host::windows_time_secs()));
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
    // PlayerPlayingAsRevolutionaries() → the player's faction has its +0x814 flag set and not its
    // +0x81C one (0x009EED40, CONFIRMED test; the flags' meaning INFERRED from the description:
    // the player sided with the revolutionaries). PROVISIONAL: neither flag is in our model yet,
    // so false.
    f!("PlayerPlayingAsRevolutionaries", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
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
    agents::install(lua, inner, ui, t)?;
    army::install(lua, inner, ui, t)?;
    diplomacy::install(lua, inner, ui, t)?;
    government::install(lua, inner, ui, t)?;
    map::install(lua, inner, ui, t)?;
    settlement::install(lua, inner, ui, t)?;
    tabs::install(lua, inner, ui, t)?;
    technology::install(lua, inner, ui, t)?;
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

// ---------------------------------------------------------------------------------------------
// Engine → HUD calls.

impl UiScriptHost {
    /// Turns this host into the campaign HUD's host: fills `CampaignUI` with the engine
    /// functions above and runs the campaign prelude. Call before loading the layout.
    pub fn install_campaign(&self, link: CampaignLink) -> mlua::Result<()> {
        // Panels keep their authored size (only the front end stretches its pages to the screen),
        // and the HUD scripts work in the layout's 1280x960 frame (UiWorld::script_rect).
        self.set_frame(super::host::UiFrame::ScriptFrame);
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
            unit_details: RefCell::new(HashMap::new()),
            settlement_names: names,
            slot_ids: RefCell::new(Interner::new()),
            variants: building_variants(&self.inner().source),
            camera: Cell::new((0.0, 0.0, 0.0)),
            visible: RefCell::new(Vec::new()),
            over: Cell::new(None),
            tech_links: std::cell::OnceCell::new(),
            post_names: std::cell::OnceCell::new(),
            map_folder: std::cell::OnceCell::new(),
            playable_areas: std::cell::OnceCell::new(),
            palettes: RefCell::new(HashMap::new()),
            theatre_bounds: std::cell::OnceCell::new(),
            camera_target: Cell::new((0.0, 0.0, 0.0)),
            slot_types: std::cell::OnceCell::new(),
            radar_view: RefCell::new((None, None)),
            religion_icons: std::cell::OnceCell::new(),
            order_factor_pips: std::cell::OnceCell::new(),
            town_factor_pips: std::cell::OnceCell::new(),
            portrait_folders: std::cell::OnceCell::new(),
            capture_shown: Cell::new(None),
            negotiation: RefCell::new(NegotiationState::default()),
        });
        let lua = self.lua();
        let t: Table = lua.globals().get("CampaignUI")?;
        install_functions(lua, self.inner(), &ui, &t)?;
        install_negotiation_object(lua, self.inner(), &ui)?;
        self.inner().campaign_mode.set(true);
        lua.load(PRELUDE).set_name("@ntw_campaign_prelude.lua").exec()?;
        *self.campaign.borrow_mut() = Some(ui);
        Ok(())
    }

    fn campaign_ui(&self) -> Option<Rc<CampaignUi>> {
        self.campaign.borrow().clone()
    }

    /// The campaign's part of the start of a UI frame, before the posted calls are made: the
    /// campaign events due since the last frame become posted calls (`diplomacy::sync_negotiation`).
    pub(in crate::ui) fn campaign_frame(&self) {
        let Some(ui) = self.campaign_ui() else { return };
        if let Err(e) = diplomacy::sync_negotiation(self.lua(), self.inner(), &ui) {
            log(self.inner(), format!("ERROR posting the negotiation events: {e}"));
        }
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

impl CampaignUi {
    /// The campaign map folder as the original's file paths name it: "data/campaign_maps/<map>"
    /// (the campaign's map key, [`CampaignLink::map`], from its campaign source; INFERRED prefix).
    /// Made once.
    fn map_folder(&self) -> String {
        self.map_folder
            .get_or_init(|| {
                let map = self.link.map.trim_start_matches("campaign_maps/").trim_start_matches("campaign_maps\\").to_owned();
                format!("data/campaign_maps/{map}")
            })
            .clone()
    }

    /// The `campaign_map_playable_areas` row of a theatre (matched on the area key, e.g.
    /// "europe_main", or the first column). Read once.
    fn playable_area(&self, inner: &Inner, theatre: &str) -> Option<ntw_data::CampaignMapPlayableArea> {
        let rows = self.playable_areas.get_or_init(|| {
            inner.source.typed_table::<ntw_data::CampaignMapPlayableArea>().map(|t| t.rows().to_vec()).unwrap_or_default()
        });
        rows.iter().find(|r| r.area.eq_ignore_ascii_case(theatre) || r.id == theatre).cloned()
    }

    /// The campaign's home theatre: its first theatre's area key ([`CampaignLink::theatres`]; "" for a
    /// campaign without one).
    fn home_theatre(&self) -> &str {
        self.link.theatres.first().map_or("", String::as_str)
    }

    /// The campaign's home theatre's `campaign_map_playable_areas` row.
    fn theatre(&self, inner: &Inner) -> Option<ntw_data::CampaignMapPlayableArea> {
        self.playable_area(inner, self.home_theatre())
    }

    /// A public order factor's pip picture for its sign (`public_order_factors` column 1 for a
    /// positive factor, column 2 for a negative one: the record's `+0xC` / `+0x18`, which
    /// `0x00887960` copies, CONFIRMED), read once. A key the table lacks has none (the exe logs it
    /// as not a valid key, `0x008E02E0`), logged once per key.
    fn order_factor_pip(&self, inner: &Inner, key: &str, positive: bool) -> Option<String> {
        let pips = self.order_factor_pips.get_or_init(|| {
            small_table(inner, &tables::PUBLIC_ORDER_FACTORS)
                .iter()
                .filter_map(|r| {
                    let s = |i: usize| r.get(i).and_then(|v| v.as_str()).unwrap_or("").to_owned();
                    Some((r.first()?.as_str()?.to_owned(), (s(1), s(2))))
                })
                .collect()
        });
        match pips.get(key) {
            Some((up, down)) => Some(if positive { up } else { down }.clone()),
            None => {
                inner.log_once_for("public order factor", key, || format!("UNKNOWN public_order_factors has no {key:?}: no pip (logged once per key)"));
                None
            }
        }
    }

    /// A town wealth growth factor's pip picture: `town_wealth_growth_factors` column 1 (the record's
    /// `+0xC`, which `0x00A4E250` copies whatever the factor's sign, CONFIRMED), read once. A key the
    /// table lacks has none (the exe skips such a factor, `0x00A995A0`), logged once per key.
    fn town_factor_pip(&self, inner: &Inner, key: &str) -> Option<String> {
        let pips = self.town_factor_pips.get_or_init(|| {
            small_table(inner, &tables::TOWN_WEALTH_GROWTH_FACTORS)
                .iter()
                .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(1)?.as_str()?.to_owned())))
                .collect()
        });
        let pip = pips.get(key).cloned();
        if pip.is_none() {
            inner.log_once_for("town wealth factor", key, || format!("UNKNOWN town_wealth_growth_factors has no {key:?}: factor skipped (logged once per key)"));
        }
        pip
    }

    /// A religion's pip picture (`religions` column 2, e.g. "data/ui/campaign ui/pips/animism.tga";
    /// CONFIRMED as the diplomacy list's ReligionIcon), read once.
    fn religion_icon(&self, inner: &Inner, religion: &str) -> Option<String> {
        self.religion_icons
            .get_or_init(|| {
                small_table(inner, &tables::RELIGIONS)
                    .iter()
                    .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(2)?.as_str()?.to_owned())))
                    .collect()
            })
            .get(religion)
            .cloned()
    }

    /// A culture's fallback culture: the `cultures` row's third column (egy_european → european,
    /// middle_east → indian, indian → middle_east; empty for european), layout string, int,
    /// optional string (DB_BUILDERS.md `cultures_tables`). Read once. INFERRED use: the portrait
    /// folder when the culture's own folder lacks the picture (`MinisterPortraitPath`).
    fn culture_fallback(&self, inner: &Inner, culture: &str) -> Option<String> {
        self.portrait_folders
            .get_or_init(|| {
                small_table(inner, &tables::CULTURES)
                    .iter()
                    .filter_map(|r| Some((r.first()?.as_str()?.to_owned(), r.get(2)?.as_str()?.to_owned())))
                    .collect()
            })
            .get(culture)
            .cloned()
    }

    /// The `slots` table (slot type → its kind flags), read once.
    fn slot_types(&self, inner: &Inner) -> &HashMap<String, ntw_data::SlotTypeRecord> {
        self.slot_types.get_or_init(|| {
            inner
                .source
                .typed_table::<ntw_data::SlotTypeRecord>()
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
            let path = format!("{}/regions.esf", self.map_folder());
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

/// The rows of a small DB table through the merged table reader, empty if the table is missing or
/// does not read (logged once, `ScriptSource::table_rows_shared`).
fn small_table(inner: &Inner, table: &RawTable) -> crate::source::SharedRows {
    inner.source.table_rows_or_empty(table)
}


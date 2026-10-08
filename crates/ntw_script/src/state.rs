//! The data the scripting layer keeps: the campaign model the scripts drive, plus everything that
//! only scripts use (time triggers, restricted units, missions, saved values, the log).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use mlua::{UserData, UserDataFields};
use ntw_sim::campaign::{CampaignModel, FactionId};

/// One value stored by `save_value` / read back by `load_value`. Numbers are kept as `f32`, the
/// original's `lua_Number` (W3 §6.1, CONFIRMED).
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptValue {
    /// `nil`.
    Nil,
    /// A boolean.
    Bool(bool),
    /// A number (rounded to `f32` at the boundary, DESIGN §3.5).
    Number(f32),
    /// A string.
    String(String),
}

/// A pending `add_time_trigger(name, seconds)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeTrigger {
    /// The trigger name; delivered as `context.string` of the `TimeTrigger` event.
    pub name: String,
    /// Script time (seconds since the host started) at which it fires.
    pub fire_at: f32,
}

/// A mission started by `trigger_custom_mission` (W3 §7). Only recorded so far: mission
/// tracking and rewards are not implemented (PLACEHOLDER).
#[derive(Debug, Clone, PartialEq)]
pub struct CustomMission {
    /// Mission key (argument 1), e.g. `"eur_take_vienna"`.
    pub key: String,
    /// Faction key (argument 2).
    pub faction: String,
    /// Mission type (argument 3), e.g. `"capture_city"`.
    pub kind: String,
    /// Target (argument 5), e.g. a region key.
    pub target: String,
    /// Reward strings (arguments 13 onwards), e.g. `"money:2000"`.
    pub rewards: Vec<String>,
}

/// The `context` object passed to every event handler (W3 §6.2, CONFIRMED that one is passed).
///
/// Scripts read only `context.string` and `context.component` directly (W3 §6.3, CONFIRMED);
/// everything else is read by the `conditions.*` functions, so the other fields are our own way of
/// carrying "who/what this event is about" (INFERRED: the original's context is a C++ object with
/// this information).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptContext {
    /// `context.string`, e.g. the UI name for `UICreated` or the trigger name for `TimeTrigger`.
    pub string: Option<String>,
    /// `context.component`: the UI component id (we have no UI component tree yet).
    pub component: Option<String>,
    /// The faction the event is about (faction key).
    pub faction: Option<String>,
    /// The region the event is about (region key).
    pub region: Option<String>,
    /// The settlement the event is about (settlement key, e.g. `"settlement:eur_austria:vienna"`).
    pub settlement: Option<String>,
    /// The character the event is about (character id).
    pub character: Option<i32>,
    /// The mission the event is about (mission key).
    pub mission: Option<String>,
    /// The building level the event is about (building level key).
    pub building_level: Option<String>,
}

impl ScriptContext {
    /// A context whose `string` is `s`.
    pub fn with_string(s: &str) -> Self {
        ScriptContext { string: Some(s.to_string()), ..Default::default() }
    }

    /// A context about faction `key`.
    pub fn for_faction(key: &str) -> Self {
        ScriptContext { faction: Some(key.to_string()), ..Default::default() }
    }
}

impl UserData for ScriptContext {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("string", |_, this| Ok(this.string.clone()));
        fields.add_field_method_get("component", |_, this| Ok(this.component.clone()));
    }
}

/// Everything the scripts can see or change. Owned by [`crate::ScriptHost`].
#[derive(Debug, Clone)]
pub struct ScriptState {
    /// The campaign model the scripts drive.
    pub model: CampaignModel,
    /// Faction key of the local (human) player. Used by `conditions.FactionIsLocal` and friends.
    pub local_faction: String,
    /// The campaign key (e.g. `"eur_napoleon"`), for `conditions.CampaignName`.
    pub campaign: String,
    /// `game_interface:is_new_game()`.
    pub is_new_game: bool,
    /// Log lines: `out.*`, `print`, and every stub call (tagged UNKNOWN).
    pub log: Vec<String>,
    /// Seconds of script time elapsed (advanced by [`crate::ScriptHost::advance_time`]).
    pub time: f32,
    /// Pending time triggers, in the order they were added.
    pub time_triggers: Vec<TimeTrigger>,
    /// Values written by `save_value` during the current `SavingGame` event.
    pub saved_values: Vec<ScriptValue>,
    /// Values waiting to be read by `load_value` (filled before `LoadingGame`).
    pub values_to_load: VecDeque<ScriptValue>,
    /// `add_restricted_unit_record` keys (units the player may not recruit).
    pub restricted_units: BTreeSet<String>,
    /// `force_diplomacy(a, b, option, offer, accept)`: the latest flags per (a, b, option).
    pub diplomacy_options: BTreeMap<(String, String, String), (bool, bool)>,
    /// `other_income_mod(faction, amount)`: summed per faction. UNKNOWN whether the original adds or
    /// replaces, and how it feeds the turn income; not applied to the treasury yet.
    pub other_income: BTreeMap<String, f32>,
    /// Missions started by `trigger_custom_mission`.
    pub missions: Vec<CustomMission>,
    /// What the trait and ancillary actions did (gains, losses, refusals), in order (slot 0-G).
    pub character_log: Vec<String>,
}

impl ScriptState {
    /// A fresh state around `model`.
    pub fn new(model: CampaignModel, local_faction: &str) -> Self {
        ScriptState {
            model,
            local_faction: local_faction.to_string(),
            campaign: String::new(),
            is_new_game: true,
            log: Vec::new(),
            time: 0.0,
            time_triggers: Vec::new(),
            saved_values: Vec::new(),
            values_to_load: VecDeque::new(),
            restricted_units: BTreeSet::new(),
            diplomacy_options: BTreeMap::new(),
            other_income: BTreeMap::new(),
            missions: Vec::new(),
            character_log: Vec::new(),
        }
    }

    /// The id of the faction with key `key`.
    pub fn faction_id(&self, key: &str) -> Option<FactionId> {
        self.model.world.factions.values().find(|f| f.key == key).map(|f| f.id)
    }

    /// The key of faction `id`.
    pub fn faction_key(&self, id: FactionId) -> Option<String> {
        self.model.world.factions.get(&id).map(|f| f.key.clone())
    }
}

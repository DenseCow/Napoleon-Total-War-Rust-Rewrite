//! [`ScriptHost`]: one Lua state running the original campaign scripts against a campaign model.

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Table, Value};
use ntw_sim::campaign::{CampaignEvent, CampaignModel, FactionId, TurnStep};

use crate::game::{self, Shared};
use crate::luac;
use crate::source::ScriptSource;
use crate::state::{ScriptContext, ScriptState, ScriptValue};
use crate::{bit, conditions};

/// Our own prelude: stand-ins for the engine's UI and logging tables (UNKNOWN behaviour).
const PRELUDE: &str = include_str!("prelude.lua");

/// What went wrong while loading or running scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    /// A script file was not found in the packs or the data folder.
    NotFound(String),
    /// The pack files could not be opened.
    Pack(String),
    /// Lua reported an error (syntax or runtime), with Lua's message and traceback.
    Lua(String),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::NotFound(p) => write!(f, "script not found: {p}"),
            ScriptError::Pack(e) => write!(f, "pack error: {e}"),
            ScriptError::Lua(e) => write!(f, "Lua error: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {}

impl From<mlua::Error> for ScriptError {
    fn from(e: mlua::Error) -> Self {
        ScriptError::Lua(e.to_string())
    }
}

/// The result of firing one event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FireReport {
    /// How many handler functions were called.
    pub handlers: usize,
    /// Error messages from handlers that failed. The other handlers still ran (INFERRED: the
    /// original logs a script error and carries on; its exact behaviour is UNKNOWN).
    pub errors: Vec<String>,
}

/// One Lua 5.1 state with the original's script environment.
///
/// The campaign AI's turn: called with the script state when a faction's `TurnStep::AiTurn` is the
/// next step (the model then accepts that faction's commands). Returns the events its commands
/// caused; the host fires them to the scripts. Set by `ntw_ai::campaign::driver::install`.
pub type AiTurnHook = Box<dyn FnMut(&mut ScriptState, FactionId) -> Vec<CampaignEvent>>;

/// Typical use:
/// ```no_run
/// # use ntw_script::{ScriptHost, ScriptSource, ScriptContext};
/// # fn demo(model: ntw_sim::campaign::CampaignModel) -> Result<(), ntw_script::ScriptError> {
/// let source = ScriptSource::from_install(r"C:\...\Napoleon Total War\data")
///     .map_err(|e| ntw_script::ScriptError::Pack(e.to_string()))?;
/// let mut host = ScriptHost::new(model, "france", source)?;
/// host.load_campaign("eur_napoleon")?;
/// host.fire("NewSession", ScriptContext::default());
/// host.fire("NewCampaignStarted", ScriptContext::default());
/// # Ok(()) }
/// ```
pub struct ScriptHost {
    lua: Lua,
    state: Shared,
    source: Rc<ScriptSource>,
    /// Plays AI factions during the turn loop (see [`ScriptHost::set_ai_turn_hook`]).
    ai_turn: Option<AiTurnHook>,
}

impl ScriptHost {
    /// Creates the Lua state: standard libraries, our `require` loader, `GAME`, `conditions`,
    /// `effect`, `bit` and the UI/logging stand-ins.
    pub fn new(model: CampaignModel, local_faction: &str, source: ScriptSource) -> Result<Self, ScriptError> {
        let lua = Lua::new();
        let state: Shared = Rc::new(RefCell::new(ScriptState::new(model, local_faction)));
        let source = Rc::new(source);
        let host = ScriptHost { lua, state, source: source.clone(), ai_turn: None };
        host.install_globals(source)?;
        Ok(host)
    }

    fn install_globals(&self, source: Rc<ScriptSource>) -> Result<(), ScriptError> {
        let lua = &self.lua;
        let g = lua.globals();

        let s = self.state.clone();
        g.set(
            "__ntw_log",
            lua.create_function(move |_, text: String| {
                game::log(&s, text);
                Ok(())
            })?,
        )?;
        lua.load(PRELUDE).set_name("@ntw_prelude.lua").exec()?;

        // GAME(context) → the single game_interface object.
        let gi = game::create(lua, &self.state)?;
        g.set("GAME", lua.create_function(move |_, _ctx: MultiValue| Ok(gi.clone()))?)?;
        g.set("conditions", conditions::create_conditions(lua, &self.state)?)?;
        g.set("effect", conditions::create_effect(lua, &self.state)?)?;
        g.set("bit", bit::create(lua)?)?;

        // `require` through the VFS: our searcher goes in package.loaders[2], right after the
        // preload searcher (Lua 5.1 layout). INFERRED default path "?.lua"; the episodic module
        // sets its own `package.path` (";?.lua;data/ui/templates/?.lua;data/ui/?.lua").
        let package: Table = g.get("package")?;
        package.set("path", "?.lua")?;
        let loaders: Table = package.get("loaders")?;
        let searcher = lua.create_function(move |lua, name: String| {
            let package: Table = lua.globals().get("package")?;
            let path: String = package.get("path")?;
            let file = name.replace('.', "/");
            let mut tried = String::new();
            for template in path.split(';').filter(|t| !t.is_empty()) {
                let candidate = template.replace('?', &file);
                match source.find(&candidate) {
                    Some(f) => return Ok(Value::Function(load_chunk(lua, &f.bytes, &f.chunk_name)?)),
                    None => tried.push_str(&format!("\n\tno file '{candidate}' in the install")),
                }
            }
            Ok(Value::String(lua.create_string(&tried)?))
        })?;
        // Shift the C-module searchers up by one and put ours at index 2.
        let n = loaders.raw_len();
        for i in (2..=n).rev() {
            let v: Value = loaders.raw_get(i)?;
            loaders.raw_set(i + 1, v)?;
        }
        loaders.raw_set(2, searcher)?;

        Ok(())
    }

    /// Runs one script file by path (e.g. `"data/all_scripted.lua"`) through the same search as
    /// `require` (VFS, `.luac`, loose file).
    pub fn run_file(&self, path: &str) -> Result<(), ScriptError> {
        let f = self.source.find(path).ok_or_else(|| ScriptError::NotFound(path.to_string()))?;
        load_chunk(&self.lua, &f.bytes, &f.chunk_name)?.call::<()>(())?;
        Ok(())
    }

    /// Runs a piece of Lua source (for tests and tools).
    pub fn exec(&self, code: &str) -> Result<(), ScriptError> {
        self.lua.load(code).set_name("@ntw_exec").exec()?;
        Ok(())
    }

    /// Loads a campaign's scripts in the INFERRED engine order: `data/all_scripted.lua` (the
    /// generated trigger tables, W3 §6.2), then `data/campaigns/<campaign>/scripting.lua`.
    pub fn load_campaign(&mut self, campaign: &str) -> Result<(), ScriptError> {
        self.state.borrow_mut().campaign = campaign.to_string();
        self.run_file("data/all_scripted.lua")?;
        self.run_file(&format!("data/campaigns/{campaign}/scripting.lua"))
    }

    /// The event-handler table: `package.loaded["data.events"]` (the module that `events.lua`
    /// declares and every script appends to), else the global `events` (INFERRED).
    fn events_table(&self) -> Result<Option<Table>, ScriptError> {
        let package: Table = self.lua.globals().get("package")?;
        let loaded: Table = package.get("loaded")?;
        if let Value::Table(t) = loaded.get::<Value>("data.events")? {
            return Ok(Some(t));
        }
        Ok(match self.lua.globals().get::<Value>("events")? {
            Value::Table(t) => Some(t),
            _ => None,
        })
    }

    /// Fires script event `name`: calls every function in `events[name]`, in list order, with a
    /// `context` object (W3 §6.2, CONFIRMED mechanism; order INFERRED). The list is copied before
    /// the calls, so handlers added or removed during the event take effect next time (INFERRED).
    pub fn fire(&self, name: &str, ctx: ScriptContext) -> FireReport {
        let mut report = FireReport::default();
        let handlers: Vec<Function> = match self.events_table() {
            Ok(Some(events)) => match events.get::<Value>(name) {
                Ok(Value::Table(list)) => {
                    (1..=list.raw_len()).filter_map(|i| list.raw_get::<Function>(i).ok()).collect()
                }
                _ => Vec::new(),
            },
            Ok(None) => Vec::new(),
            Err(e) => {
                report.errors.push(e.to_string());
                Vec::new()
            }
        };
        let ud = match self.lua.create_userdata(ctx) {
            Ok(ud) => ud,
            Err(e) => {
                report.errors.push(e.to_string());
                return report;
            }
        };
        for f in handlers {
            report.handlers += 1;
            if let Err(e) = f.call::<()>(ud.clone()) {
                let msg = format!("{name}: {e}");
                game::log(&self.state, format!("SCRIPT ERROR {msg}"));
                report.errors.push(msg);
            }
        }
        report
    }

    /// Ends the turn in the model and forwards every resulting script event to Lua, in the order
    /// the model produced them (PROVISIONAL order, see `ntw_sim::campaign::turn`).
    pub fn end_turn(&mut self) -> Vec<(CampaignEvent, FireReport)> {
        let started = self.state.borrow().model.turn.started;
        let mut out = Vec::new();
        if !started {
            out.extend(self.start_campaign());
        }
        self.state.borrow_mut().model.begin_end_turn();
        out.extend(self.run_steps());
        out
    }

    /// Starts turn 1 of a new campaign (`ntw_sim` `start_campaign`), firing each step's script
    /// events before the next step runs.
    pub fn start_campaign(&mut self) -> Vec<(CampaignEvent, FireReport)> {
        self.state.borrow_mut().model.begin_start_campaign();
        self.run_steps()
    }

    /// Applies a command to the model and fires the script events it caused (`EndTurn` steps
    /// through the turn like [`end_turn`](Self::end_turn)).
    pub fn apply(
        &mut self,
        cmd: ntw_sim::campaign::CampaignCommand,
    ) -> Result<Vec<(CampaignEvent, FireReport)>, ntw_sim::campaign::CommandError> {
        if cmd == ntw_sim::campaign::CampaignCommand::EndTurn {
            if self.state.borrow().model.pending_battle.is_some() {
                return Err(ntw_sim::campaign::CommandError::BattlePending);
            }
            return Ok(self.end_turn());
        }
        let events = self.state.borrow_mut().model.apply(cmd)?;
        // Every event is returned (project events such as `CharacterMoved` with an empty report).
        Ok(self.fire_all(events, |_| true))
    }

    /// Sets the campaign AI that plays the non-human factions in the turn loop (see
    /// [`AiTurnHook`]). Without one, AI factions do nothing.
    pub fn set_ai_turn_hook(&mut self, hook: AiTurnHook) {
        self.ai_turn = Some(hook);
    }

    /// Runs the model's queued turn steps one at a time; after each step its script events are
    /// fired, so handlers see the state of that moment (PROVISIONAL phase order, see
    /// `ntw_sim::campaign::turn`). The model is not borrowed while Lua runs.
    fn run_steps(&mut self) -> Vec<(CampaignEvent, FireReport)> {
        let mut out = Vec::new();
        loop {
            // An AI faction's turn: the AI hook plays it before the (empty) `AiTurn` step runs.
            let next = self.state.borrow().model.turn.queue.front().copied();
            if let (Some(TurnStep::AiTurn(faction)), Some(hook)) = (next, self.ai_turn.as_mut()) {
                let events = hook(&mut self.state.borrow_mut(), faction);
                out.extend(self.fire_all(events, CampaignEvent::is_diagnostic));
            }
            let step = self.state.borrow_mut().model.step();
            let Some(events) = step else { break };
            out.extend(self.fire_all(events, CampaignEvent::is_diagnostic));
        }
        out
    }

    /// Fires every script event in `events`, in order, and returns it with its report; of the
    /// other events, those `keep` accepts are returned unfired (a command's caller gets them all,
    /// the turn steps only the diagnostics ([`CampaignEvent::is_diagnostic`]) for the app's log).
    fn fire_all(&mut self, events: Vec<CampaignEvent>, keep: fn(&CampaignEvent) -> bool) -> Vec<(CampaignEvent, FireReport)> {
        let mut out = Vec::new();
        for ev in events {
            if let Some(name) = ev.script_name() {
                let ctx = self.context_for(&ev);
                let report = self.fire(name, ctx);
                out.push((ev, report));
            } else if keep(&ev) {
                out.push((ev, FireReport::default()));
            }
        }
        out
    }

    /// The context for a model event: which faction / region / character it concerns.
    pub fn context_for(&self, ev: &CampaignEvent) -> ScriptContext {
        let st = self.state.borrow();
        let w = &st.model.world;
        let faction_of = |id| st.faction_key(id);
        let mut c = ScriptContext::default();
        match ev {
            CampaignEvent::FactionRoundStart { faction }
            | CampaignEvent::FactionTurnStart { faction }
            | CampaignEvent::FactionTurnEnd { faction }
            | CampaignEvent::GovernorshipTaxRateChanged { faction } => c.faction = faction_of(*faction),
            CampaignEvent::RegionTurnStart { region }
            | CampaignEvent::RegionTurnEnd { region }
            | CampaignEvent::SlotTurnStart { region, .. }
            | CampaignEvent::RecruitmentItemIssuedByPlayer { region } => {
                if let Some(r) = w.regions.get(region) {
                    c.region = Some(r.key.clone());
                    c.settlement = Some(r.settlement.key.clone());
                    c.faction = faction_of(r.owner);
                }
            }
            CampaignEvent::CharacterTurnStart { character }
            | CampaignEvent::CharacterTurnEnd { character }
            | CampaignEvent::CharacterCreated { character }
            | CampaignEvent::CharacterPromoted { character }
            | CampaignEvent::SufferSpyingAttempt { character }
            | CampaignEvent::SpyingAttemptSuccess { character }
            | CampaignEvent::CharacterFactionSpyAttemptSuccessful { character }
            | CampaignEvent::CharacterFactionSuffersSuccessfulSpyAttempt { character }
            | CampaignEvent::EspionageAgentApprehended { character }
            | CampaignEvent::SufferAssassinationAttempt { character }
            | CampaignEvent::AssassinationAttemptSuccess { character }
            | CampaignEvent::CharacterCriticallyFailsAssassination { character }
            | CampaignEvent::SabotageAttemptSuccess { character }
            | CampaignEvent::ArmySabotageAttemptSuccess { character }
            | CampaignEvent::HarassmentAttemptSuccess { character }
            | CampaignEvent::DuelFought { character }
            | CampaignEvent::CharacterBuildsSpyNetwork { character }
            | CampaignEvent::MovementPointsExhausted { character } => {
                c.character = Some(character.raw());
                c.faction = w.characters.get(character).and_then(|ch| faction_of(ch.faction));
            }
            CampaignEvent::UnitTrained { force, .. } | CampaignEvent::UnitTurnEnd { force, .. } => {
                c.faction = w.forces.get(force).and_then(|f| faction_of(f.faction));
            }
            CampaignEvent::PendingBankruptcy { faction } => c.faction = faction_of(*faction),
            CampaignEvent::ResearchCompleted { faction, .. } => c.faction = faction_of(*faction),
            CampaignEvent::BattleCompleted { attacker_faction, .. } => c.faction = faction_of(*attacker_faction),
            CampaignEvent::BuildingConstructionIssuedByPlayer { region }
            | CampaignEvent::BuildingCompleted { region, .. } => {
                if let Some(r) = w.regions.get(region) {
                    c.region = Some(r.key.clone());
                    c.settlement = Some(r.settlement.key.clone());
                    c.faction = faction_of(r.owner);
                }
            }
            CampaignEvent::CaptureChoicePending { region, faction } | CampaignEvent::CaptureResolved { region, faction, .. } => {
                if let Some(r) = w.regions.get(region) {
                    c.region = Some(r.key.clone());
                    c.settlement = Some(r.settlement.key.clone());
                }
                c.faction = faction_of(*faction);
            }
            CampaignEvent::SettlementOccupied { region, faction } => {
                if let Some(r) = w.regions.get(region) {
                    c.region = Some(r.key.clone());
                    c.settlement = Some(r.settlement.key.clone());
                }
                c.faction = faction_of(*faction);
            }
            CampaignEvent::CharacterEntersGarrison { character, region } => {
                c.character = Some(character.raw());
                c.faction = w.characters.get(character).and_then(|ch| faction_of(ch.faction));
                if let Some(r) = w.regions.get(region) {
                    c.region = Some(r.key.clone());
                    c.settlement = Some(r.settlement.key.clone());
                }
            }
            CampaignEvent::CharacterCompletedBattle { character }
            | CampaignEvent::CharacterMoved { character, .. }
            | CampaignEvent::CharacterEmbarksNavy { character, .. }
            | CampaignEvent::CharacterDisembarksNavy { character, .. } => {
                c.character = Some(character.raw());
                c.faction = w.characters.get(character).and_then(|ch| faction_of(ch.faction));
            }
            CampaignEvent::CampaignArmiesMerge { force, .. }
            | CampaignEvent::PreBattle { attacker: force }
            | CampaignEvent::ForceDestroyed { force } => {
                c.faction = w.forces.get(force).and_then(|f| faction_of(f.faction));
            }
            CampaignEvent::CharacterHired { character, .. } => {
                c.character = Some(character.raw());
                c.faction = w.characters.get(character).and_then(|ch| faction_of(ch.faction));
            }
            CampaignEvent::AgentActionResolved { agent, .. } => {
                c.character = Some(agent.raw());
                c.faction = w.characters.get(agent).and_then(|ch| faction_of(ch.faction));
            }
            CampaignEvent::StanceChanged { .. } => {}
            CampaignEvent::GovernmentChanged { .. } | CampaignEvent::ConstructionItemDropped { .. } => {}
        }
        c
    }

    /// Advances script time by `seconds` and fires `TimeTrigger` (context.string = trigger name)
    /// for every trigger that is due, earliest first (ties in the order they were added).
    pub fn advance_time(&mut self, seconds: f32) -> Vec<FireReport> {
        let due = {
            let mut st = self.state.borrow_mut();
            st.time += seconds;
            let now = st.time;
            let mut due: Vec<_> = st.time_triggers.iter().filter(|t| t.fire_at <= now).cloned().collect();
            st.time_triggers.retain(|t| t.fire_at > now);
            due.sort_by(|a, b| a.fire_at.total_cmp(&b.fire_at)); // stable: keeps insertion order on ties
            due
        };
        due.iter().map(|t| self.fire("TimeTrigger", ScriptContext::with_string(&t.name))).collect()
    }

    /// Fires `SavingGame` and returns the values the scripts saved with `save_value`, in order.
    pub fn save_values(&mut self) -> (Vec<ScriptValue>, FireReport) {
        self.state.borrow_mut().saved_values.clear();
        let report = self.fire("SavingGame", ScriptContext::default());
        (std::mem::take(&mut self.state.borrow_mut().saved_values), report)
    }

    /// Queues `values` for `load_value` and fires `LoadingGame`. Marks the game as not new.
    pub fn load_values(&mut self, values: Vec<ScriptValue>) -> FireReport {
        {
            let mut st = self.state.borrow_mut();
            st.values_to_load = values.into();
            st.is_new_game = false;
        }
        self.fire("LoadingGame", ScriptContext::default())
    }

    /// Read access to the script state (model, log, missions, ...). Do not hold it across a call
    /// that runs Lua.
    pub fn state(&self) -> Ref<'_, ScriptState> {
        self.state.borrow()
    }

    /// Write access to the script state. Do not hold it across a call that runs Lua.
    pub fn state_mut(&self) -> RefMut<'_, ScriptState> {
        self.state.borrow_mut()
    }

    /// The shared script state, for readers that live next to the host (the campaign HUD's
    /// `CampaignUI` functions read the model through it). Never hold a borrow across a call
    /// that runs this host's Lua.
    pub fn shared_state(&self) -> Rc<RefCell<ScriptState>> {
        self.state.clone()
    }

    /// Read access to the campaign model.
    pub fn model(&self) -> Ref<'_, CampaignModel> {
        Ref::map(self.state.borrow(), |s| &s.model)
    }

    /// The raw Lua state (for tools and tests).
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}

/// Loads a chunk: source text as it is, bytecode after [`luac::convert_chunk`].
pub(crate) fn load_chunk(lua: &Lua, bytes: &[u8], chunk_name: &str) -> mlua::Result<Function> {
    if luac::is_bytecode(bytes) {
        let converted = luac::convert_chunk(bytes).map_err(|e| mlua::Error::runtime(format!("{chunk_name}: {e}")))?;
        lua.load(&converted[..])
            .set_name(chunk_name)
            .set_mode(mlua::chunk::ChunkMode::Binary)
            .into_function()
    } else {
        lua.load(bytes).set_name(chunk_name).set_mode(mlua::chunk::ChunkMode::Text).into_function()
    }
}

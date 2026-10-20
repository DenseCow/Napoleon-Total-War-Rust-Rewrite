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

/// Which file a campaign opens from: `0x00987070`'s new-campaign argument (its stack slot
/// `[ESP+0x3cc]`), which picks `startpos.esf` over the save (`0x00987634`) and gates
/// `NewCampaignStarted` (`0x00988A1F`, CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignOpening {
    /// A new campaign from its start position.
    NewCampaign,
    /// A loaded save.
    LoadedSave,
}

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
/// # use ntw_script::{CampaignOpening, ScriptHost, ScriptSource, ScriptContext};
/// # fn demo(loaded: ntw_campaign::LoadedCampaign) -> Result<(), ntw_script::ScriptError> {
/// let source = ScriptSource::from_install(r"C:\...\Napoleon Total War\data")
///     .map_err(|e| ntw_script::ScriptError::Pack(e.to_string()))?;
/// let mut host = ScriptHost::new(loaded.model, "france", source)?;
/// host.load_campaign("eur_napoleon")?;
/// // A new campaign: LoadingGame reads the start position's stored values.
/// host.open_campaign(ScriptContext::for_faction("france"), CampaignOpening::NewCampaign, &loaded.script_values);
/// host.start_campaign();
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

    /// Fires script event `name`: calls every function in `events[name]`, in list order, with a
    /// `context` object (W3 §6.2, CONFIRMED mechanism; order INFERRED). The list is copied before
    /// the calls, so handlers added or removed during the event take effect next time (INFERRED).
    pub fn fire(&self, name: &str, ctx: ScriptContext) -> FireReport {
        fire_in(&self.lua, &self.state, name, ctx)
    }

    /// Ends the turn in the model and forwards every resulting script event to Lua, in the order
    /// the model produced them (PROVISIONAL order, see `ntw_sim::campaign::turn`), and returns every
    /// event of the turn, the AI turns' included ([`fire_all`](Self::fire_all)).
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
    /// through the turn like [`end_turn`](Self::end_turn); `RespondToAllyWarCall` resumes the turn
    /// steps a waiting call to war had paused, `ntw_sim` `CampaignModel::step`).
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
        let resumes = matches!(cmd, ntw_sim::campaign::CampaignCommand::RespondToAllyWarCall { .. });
        let events = self.state.borrow_mut().model.apply(cmd)?;
        let mut out = self.fire_all(events);
        out.extend(self.fire_script_events());
        if resumes {
            out.extend(self.run_steps());
        }
        Ok(out)
    }

    /// Fires the model events that script commands queued while the last Lua ran
    /// (`ScriptState::script_events`: `grant_faction_handover`'s destruction, `force_make_protectorate`'s
    /// peace and protectorate), then those their own
    /// handlers queued, until none is left, through [`Self::fire_all`] like a command's events, so
    /// the caller gets every one (`OutcomeMovie` with an empty report). Run after
    /// each batch of handlers in [`Self::apply`] and the turn steps; a command issued by a handler of
    /// the opening events, a time trigger or a bare [`Self::fire`] waits for the next of those.
    /// INFERRED: they fire after the batch whose handler issued the command, not nested inside that
    /// handler; the exe's order for events posted from inside a script handler is not traced.
    fn fire_script_events(&mut self) -> Vec<(CampaignEvent, FireReport)> {
        let mut out = Vec::new();
        loop {
            let queued = std::mem::take(&mut self.state.borrow_mut().script_events);
            if queued.is_empty() {
                return out;
            }
            out.extend(self.fire_all(queued));
        }
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
                out.extend(self.fire_all(events));
                out.extend(self.fire_script_events());
            }
            let step = self.state.borrow_mut().model.step();
            let Some(events) = step else { break };
            out.extend(self.fire_all(events));
            out.extend(self.fire_script_events());
        }
        out
    }

    /// The one path from the model's events to the scripts and the caller (commands, turn steps and
    /// AI turns alike): fires every script event in `events`, in order, and returns every event, a
    /// script event with its report, a project event (`CharacterMoved`, `ForceDestroyed`,
    /// `OutcomeMovie`, the diagnostics...: no script name, as the exe declares no such script event)
    /// unfired with an empty report, for the app's move animations, outcome movies and logs.
    fn fire_all(&mut self, events: Vec<CampaignEvent>) -> Vec<(CampaignEvent, FireReport)> {
        let mut out = Vec::new();
        for ev in events {
            if let Some(name) = ev.script_name() {
                let ctx = self.context_for(&ev);
                let report = self.fire(name, ctx);
                out.push((ev, report));
            } else {
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
            | CampaignEvent::GovernorshipTaxRateChanged { faction }
            | CampaignEvent::FactionBecomesLiberationProtectorate { faction } => c.faction = faction_of(*faction),
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
            | CampaignEvent::BuildingCompleted { region, .. }
            | CampaignEvent::CampaignBuildingDamaged { region, .. } => {
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
            // The exe's event answers only its character getter (vtable +0x1C); its faction getter (+0x38)
            // is the null stub 0x00461180, so no faction is set (CONFIRMED, vtable 0x0137CCB8).
            CampaignEvent::CharacterLootedSettlement { character } => c.character = Some(character.raw()),
            CampaignEvent::AgentActionResolved { agent, .. } => {
                c.character = Some(agent.raw());
                c.faction = w.characters.get(agent).and_then(|ch| faction_of(ch.faction));
            }
            CampaignEvent::StanceChanged { .. } => {}
            CampaignEvent::GovernmentChanged { .. }
            | CampaignEvent::ConstructionItemDropped { .. }
            | CampaignEvent::FactionDestroyed { .. }
            | CampaignEvent::OutcomeMovie { .. }
            | CampaignEvent::AllyWarCallPending { .. }
            | CampaignEvent::Message { .. } => {}
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
        save_values_in(&self.lua, &self.state)
    }

    /// Fires the script events of a campaign's opening, in the exe's order, and returns each with its
    /// report. `lua_values` are the opened file's `EPISODIC_RESTRICTIONS/LUA[]` items: the start
    /// position's for a new campaign, the save's for a load.
    ///
    /// The campaign model's constructor (`0x00872550`, run by `0x00987070` for a new campaign from its
    /// start position and for a loaded save alike, CONFIRMED) posts `NewSession` first (`0x009968A0` at
    /// `0x00996B06`), then `LoadingGame` whenever the opened file's EPISODIC_RESTRICTIONS holds a `LUA`
    /// block (`0x0099878E`, with no test of the new-campaign flag): every shipped start position carries
    /// one (some empty) and the save writer `0x009A1070` opens it on every path (`0x009A200C`), so both
    /// paths fire it, a new campaign reading its start position's items (CONFIRMED,
    /// CAMPAIGN_DATA.md §4.1). Our openings always fire it; a campaign without stored items (our open
    /// map format, an empty block) gives the handlers their defaults. It then builds the factions (each
    /// running record rebuilt by `0x00B95B90`, which our load did already: `ntw_campaign::read_esf` /
    /// `own_save`) and posts `ModelCreated` last (`0x0087426F`). `0x00987070` then posts
    /// `NewCampaignStarted` only for a new campaign (`0x00988A3F`; the load passes no start,
    /// `0x008B8AE3`) and `WorldCreated` for both (`0x00988A72`). So a `ModelCreated` handler books into
    /// the rebuilt record (spa_napoleon's `other_income_mod` boosts). Turn 1 of a new campaign
    /// ([`start_campaign`](Self::start_campaign)) and `UICreated` come later.
    ///
    /// `is_new_game` is the opening's new-campaign flag (`0x009968A0` keeps it at +4 of the object the
    /// campaign holds at +0xFA8, which the getter at `0x009798E0` returns), not whether `LoadingGame` ran.
    pub fn open_campaign(
        &mut self,
        ctx: ScriptContext,
        opening: CampaignOpening,
        lua_values: &[ntw_campaign::script_values::ScriptSaveValue],
    ) -> Vec<(&'static str, FireReport)> {
        let is_new = opening == CampaignOpening::NewCampaign;
        self.state.borrow_mut().is_new_game = is_new;
        let mut out = vec![("NewSession", self.fire("NewSession", ctx.clone()))];
        out.push(("LoadingGame", self.load_values(lua_values.iter().map(|&v| v.into()).collect())));
        out.push(("ModelCreated", self.fire("ModelCreated", ctx.clone())));
        if is_new {
            out.push(("NewCampaignStarted", self.fire("NewCampaignStarted", ctx.clone())));
        }
        out.push(("WorldCreated", self.fire("WorldCreated", ctx)));
        out
    }

    /// Queues `values` for `load_value` and fires `LoadingGame`. Values its handlers leave unread are
    /// dropped after it: `load_value` reads only through the `LoadingGame` event's reader (slot +0x98 of
    /// its vtable `0x01370D08` is `0x0044A500`); the other opening events' slot (NewSession `0x01370E80`,
    /// WorldCreated `0x01371020`, NewCampaignStarted `0x013710DC`) is `0x00461180`, which returns none,
    /// so there it gives the default (CONFIRMED).
    pub fn load_values(&mut self, values: Vec<ScriptValue>) -> FireReport {
        self.state.borrow_mut().values_to_load = values.into();
        let report = self.fire("LoadingGame", ScriptContext::default());
        self.state.borrow_mut().values_to_load.clear();
        report
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

    /// A cheap handle on this host's Lua state and script state, for code that lives beside the
    /// host and must fire events while the host itself is owned elsewhere (the campaign HUD's save
    /// callback fires `SavingGame`, see [`ScriptHandle`]).
    pub fn handle(&self) -> ScriptHandle {
        ScriptHandle { lua: self.lua.clone(), state: self.state.clone() }
    }
}

/// A clone of a [`ScriptHost`]'s Lua state and script state ([`ScriptHost::handle`]): fires events
/// and reads the state like the host does, without owning it. Same rule as the host: never hold a
/// borrow of the state across a call that runs Lua.
#[derive(Clone)]
pub struct ScriptHandle {
    lua: Lua,
    state: Shared,
}

impl ScriptHandle {
    /// [`ScriptHost::fire`].
    pub fn fire(&self, name: &str, ctx: ScriptContext) -> FireReport {
        fire_in(&self.lua, &self.state, name, ctx)
    }

    /// [`ScriptHost::save_values`].
    pub fn save_values(&self) -> (Vec<ScriptValue>, FireReport) {
        save_values_in(&self.lua, &self.state)
    }

    /// [`ScriptHost::state`].
    pub fn state(&self) -> Ref<'_, ScriptState> {
        self.state.borrow()
    }
}

/// The event-handler table: `package.loaded["data.events"]` (the module that `events.lua`
/// declares and every script appends to), else the global `events` (INFERRED).
fn events_table(lua: &Lua) -> Result<Option<Table>, ScriptError> {
    let package: Table = lua.globals().get("package")?;
    let loaded: Table = package.get("loaded")?;
    if let Value::Table(t) = loaded.get::<Value>("data.events")? {
        return Ok(Some(t));
    }
    Ok(match lua.globals().get::<Value>("events")? {
        Value::Table(t) => Some(t),
        _ => None,
    })
}

/// [`ScriptHost::fire`] on a Lua state and its script state.
fn fire_in(lua: &Lua, state: &Shared, name: &str, ctx: ScriptContext) -> FireReport {
    let mut report = FireReport::default();
    let handlers: Vec<Function> = match events_table(lua) {
        Ok(Some(events)) => match events.get::<Value>(name) {
            Ok(Value::Table(list)) => (1..=list.raw_len()).filter_map(|i| list.raw_get::<Function>(i).ok()).collect(),
            _ => Vec::new(),
        },
        Ok(None) => Vec::new(),
        Err(e) => {
            report.errors.push(e.to_string());
            Vec::new()
        }
    };
    let ud = match lua.create_userdata(ctx) {
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
            game::log(state, format!("SCRIPT ERROR {msg}"));
            report.errors.push(msg);
        }
    }
    report
}

/// Fires `SavingGame` and returns the values the scripts saved with `save_value`, in order.
fn save_values_in(lua: &Lua, state: &Shared) -> (Vec<ScriptValue>, FireReport) {
    state.borrow_mut().saved_values.clear();
    let report = fire_in(lua, state, "SavingGame", ScriptContext::default());
    (std::mem::take(&mut state.borrow_mut().saved_values), report)
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

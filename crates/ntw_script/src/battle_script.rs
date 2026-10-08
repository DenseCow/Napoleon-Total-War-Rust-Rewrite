//! Battle scripts: runs a historical battle's ORIGINAL `.battle_script` (Lua 5.1, read from the
//! player's install) and the game's `data/scripting_library.lua` against our battle model.
//!
//! The engine side is the binding table of `Napoleon.exe` (names CONFIRMED from the exe's
//! (name, function) tables at `0x01450E00..0x01453000`, BATTLE_FIDELITY.md §16): `empire_battle`,
//! alliances / armies / units, unit controllers, `battle_vector`, timers, phase handlers,
//! `battle_sound_effect`, camera and UI stand-ins. Our implementation is split in two:
//! - Rust primitives in the table `__nb` (this file): the battle snapshot ([`BattleScriptFacts`]),
//!   timers, handlers, requests and the log;
//! - our own Lua classes on top of them (`battle_script_prelude.lua`).
//!
//! The game reads [`BattleScriptHost::take_requests`] and applies them to the model (orders,
//! `deploy_reinforcement`, script control of units). Functions with no model behaviour yet are
//! logging stubs tagged UNKNOWN (one log line, return `nil` or a stand-in), so scripts keep running.
//!
//! Clock (INFERRED): timers run on battle time, which only advances while the battle runs (not
//! in deployment); a timer registered at `t` with period `p` fires at `t + p`, `t + 2p`, ... Timers
//! due in the same step fire in due-time order, then registration order (deterministic).

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::source::ScriptSource;

const PRELUDE: &str = include_str!("battle_script_prelude.lua");

/// One unit as the scripts see it (written by the game before each script step).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptUnit {
    /// Model unit id.
    pub id: u32,
    /// Display name (`unit:name()`).
    pub name: String,
    /// The battle file's `script_name` (`units:item("Name")` finds a unit by it).
    pub script_name: String,
    /// Men now / at the start.
    pub men: u32,
    pub initial_men: u32,
    /// Engine position `(x, height, z)`; z is the map y (INFERRED, battle-file frame).
    pub position: [f32; 3],
    /// Facing in degrees, 0 = +z, clockwise (INFERRED, like the battle-file orientation).
    pub bearing: f32,
    pub moving: bool,
    pub routing: bool,
    pub leaving: bool,
    pub cavalry: bool,
    pub infantry: bool,
    pub artillery: bool,
    /// Missile range in metres (0 without a missile weapon).
    pub missile_range: f32,
    pub ammo: u32,
    pub starting_ammo: u32,
    /// The unit is a reinforcement that has not arrived yet.
    pub off_field: bool,
    /// The building the unit is in: its 1-based `battle:buildings()` index.
    pub garrison: Option<u32>,
}

/// The battle as the scripts see it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BattleScriptFacts {
    /// Battle time in milliseconds (`battle:game_time()` is seconds, INFERRED).
    pub time_ms: u64,
    pub units: Vec<ScriptUnit>,
    /// Alliance → army → indices into `units`, in battle-file order (reinforcement armies after
    /// the alliance's armies, as `alliance:armies():item(n)` sees them in the scripts).
    pub alliances: Vec<Vec<Vec<usize>>>,
    /// The battle's buildings in `battle:buildings():item(n)` order: the map's near building list in
    /// file order (INFERRED from Lodi: item 2 = the town hall, item 52 = the farmhouse, as the script
    /// names them). Key and engine position (x, height, z).
    pub buildings: Vec<(String, [f32; 3])>,
    /// The camera at the start (position, look-at target), engine coordinates: what
    /// `camera:position()` / `camera:target()` return until a script moves it.
    pub camera: ([f32; 3], [f32; 3]),
}

/// The camera input action names the input handlers get, by action number (table `0x014518E0`,
/// CONFIRMED strings).
pub const INPUT_EVENT_NAMES: [&str; 17] = [
    "move forward", "move forward fast", "move backward", "move left", "move right", "rotate right", "rotate left",
    "move up", "move down", "rotate up", "rotate down", "edge rotate right", "edge rotate left", "edge move left",
    "edge move right", "edge move forward", "edge move backward",
];

/// What a script asked the game to do.
#[derive(Debug, Clone, PartialEq)]
pub enum BattleScriptRequest {
    /// `unit:deploy_reinforcement(b)` (CONFIRMED meaning, BATTLE_FIDELITY.md §13).
    DeployReinforcement { unit: u32, deploy: bool },
    /// `controller:take_control()`: the units obey the script, not the AI or the player.
    TakeControl { units: Vec<u32> },
    /// `controller:release_control()`.
    ReleaseControl { units: Vec<u32> },
    Halt { units: Vec<u32> },
    FireAtWill { units: Vec<u32>, on: bool },
    /// `attack_unit(target, ?, run)` (argument meanings INFERRED from the scripts).
    AttackUnit { units: Vec<u32>, target: u32, run: bool },
    /// `goto_location(pos, run)` and `goto_location_angle_width(pos, angle, width, run)`; the
    /// angle/width are carried but not used by the model yet (PROVISIONAL).
    Move { units: Vec<u32>, x: f32, z: f32, run: bool, angle: Option<f32>, width: Option<f32> },
    /// `camera:move_to(target, position, seconds)` (argument order INFERRED from the scripts' variable
    /// names): engine coordinates (x, height, z).
    Camera { target: [f32; 3], position: [f32; 3], seconds: f32 },
    /// `battle:ui_component(name):set_visible(b)`.
    UiVisible { name: String, visible: bool },
    /// `controller:guard_mode(b)`.
    GuardMode { units: Vec<u32>, on: bool },
    /// `controller:set_invincible(b)`.
    SetInvincible { units: Vec<u32>, on: bool },
    /// `controller:skirmish(b)` (binding `0x00613B10`, CONFIRMED bool argument).
    Skirmish { units: Vec<u32>, on: bool },
    /// `controller:select_deployable_object(name)` (spaces become `_`, CONFIRMED `0x00613D50`).
    SelectDeployable { units: Vec<u32>, name: String },
    /// `controller:perform_special_ability(name)` (spaces become `_`, CONFIRMED `0x00645970`).
    SpecialAbility { units: Vec<u32>, name: String },
    /// `controller:change_shot_type(name)`.
    ShotType { units: Vec<u32>, name: String },
    /// `controller:morale_behavior_fearless()` / `_default()` / `_rout()`: mode 0 / 1 / 2
    /// (CONFIRMED command modes).
    ScriptMorale { units: Vec<u32>, mode: u8 },
    /// `controller:defend_building(building, run)`: the building's 1-based `battle:buildings()` index.
    DefendBuilding { units: Vec<u32>, building: usize, run: bool },
    /// A script marker's whole state (`battle:marker(name)` and its setters): engine position
    /// (x, height, z), rotation in degrees, scale, visibility.
    Marker { id: u32, name: String, position: [f32; 3], rotation_deg: f32, scale: f32, visible: bool },
    /// Any other controller order: name and unit ids (UNKNOWN effect in the model; logged).
    Other { order: String, units: Vec<u32> },
}

#[derive(Debug, Clone)]
struct Timer {
    name: String,
    due: u64,
    period: Option<u64>,
    seq: u64,
}

#[derive(Debug, Default)]
struct State {
    facts: BattleScriptFacts,
    timers: Vec<Timer>,
    seq: u64,
    phase_handler: Option<String>,
    selection_handler: Option<String>,
    input_handler: Option<String>,
    command_handlers: Vec<String>,
    requests: Vec<BattleScriptRequest>,
    log: Vec<String>,
    rng: u32,
    version: u64,
}

type Shared = Rc<RefCell<State>>;

/// A Lua state running one battle's script.
pub struct BattleScriptHost {
    lua: Lua,
    state: Shared,
}

impl BattleScriptHost {
    /// Creates the Lua state with the engine primitives, our prelude and `require` through the
    /// install (so `require "Scripting_Library"` finds `data/scripting_library.lua`).
    pub fn new(source: ScriptSource, facts: BattleScriptFacts, seed: u32) -> mlua::Result<Self> {
        let lua = Lua::new();
        let state: Shared = Rc::new(RefCell::new(State { facts, rng: seed.max(1), ..Default::default() }));
        let host = BattleScriptHost { lua, state };
        host.install(Rc::new(source))?;
        Ok(host)
    }

    fn install(&self, source: Rc<ScriptSource>) -> mlua::Result<()> {
        let lua = &self.lua;
        let nb = lua.create_table()?;
        let s = self.state.clone();
        nb.set("log", lua.create_function(move |_, text: String| {
            s.borrow_mut().log.push(text);
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("time", lua.create_function(move |_, ()| Ok(s.borrow().facts.time_ms as f64))?)?;
        let s = self.state.clone();
        nb.set("building", lua.create_function(move |lua, i: usize| {
            let st = s.borrow();
            let Some((key, p)) = i.checked_sub(1).and_then(|i| st.facts.buildings.get(i)) else { return Ok(Value::Nil) };
            let t = lua.create_table()?;
            t.set("key", key.as_str())?;
            t.set("x", p[0])?;
            t.set("y", p[1])?;
            t.set("z", p[2])?;
            Ok(Value::Table(t))
        })?)?;
        let s = self.state.clone();
        nb.set("camera", lua.create_function(move |_, ()| {
            let (p, t) = s.borrow().facts.camera;
            Ok((p[0], p[1], p[2], t[0], t[1], t[2]))
        })?)?;
        let s = self.state.clone();
        nb.set("building_count", lua.create_function(move |_, ()| Ok(s.borrow().facts.buildings.len()))?)?;
        let s = self.state.clone();
        nb.set("building_garrisoned", lua.create_function(move |_, i: u32| {
            Ok(s.borrow().facts.units.iter().any(|u| u.garrison == Some(i)))
        })?)?;
        let s = self.state.clone();
        nb.set("version", lua.create_function(move |_, ()| Ok(s.borrow().version as f64))?)?;
        // Structure: { {army unit index lists...} per alliance } with 1-based unit indices.
        let s = self.state.clone();
        nb.set("structure", lua.create_function(move |lua, ()| {
            let st = s.borrow();
            let out = lua.create_table()?;
            for (ai, armies) in st.facts.alliances.iter().enumerate() {
                let a = lua.create_table()?;
                for (ri, units) in armies.iter().enumerate() {
                    let r = lua.create_table()?;
                    for (k, &u) in units.iter().enumerate() {
                        r.raw_set(k + 1, u + 1)?;
                    }
                    a.raw_set(ri + 1, r)?;
                }
                out.raw_set(ai + 1, a)?;
            }
            Ok(out)
        })?)?;
        // unit(i) → a fresh table of the unit's fields (1-based index into the facts).
        let s = self.state.clone();
        nb.set("unit", lua.create_function(move |lua, i: usize| {
            let st = s.borrow();
            let Some(u) = i.checked_sub(1).and_then(|i| st.facts.units.get(i)) else { return Ok(Value::Nil) };
            let t = lua.create_table()?;
            t.set("id", u.id)?;
            t.set("name", u.name.as_str())?;
            t.set("script_name", u.script_name.as_str())?;
            t.set("men", u.men)?;
            t.set("initial_men", u.initial_men)?;
            t.set("x", u.position[0])?;
            t.set("y", u.position[1])?;
            t.set("z", u.position[2])?;
            t.set("bearing", u.bearing)?;
            t.set("moving", u.moving)?;
            t.set("routing", u.routing)?;
            t.set("leaving", u.leaving)?;
            t.set("cavalry", u.cavalry)?;
            t.set("infantry", u.infantry)?;
            t.set("artillery", u.artillery)?;
            t.set("missile_range", u.missile_range)?;
            t.set("ammo", u.ammo)?;
            t.set("starting_ammo", u.starting_ammo)?;
            t.set("off_field", u.off_field)?;
            t.set("garrisoned", u.garrison.is_some())?;
            Ok(Value::Table(t))
        })?)?;
        let s = self.state.clone();
        nb.set("timer", lua.create_function(move |_, (name, ms, repeating): (String, f64, bool)| {
            let mut st = s.borrow_mut();
            let ms = ms.max(0.0) as u64;
            let due = st.facts.time_ms + ms;
            st.seq += 1;
            let seq = st.seq;
            // INFERRED: registering a name again replaces the old timer.
            st.timers.retain(|t| t.name != name);
            st.timers.push(Timer { name, due, period: repeating.then_some(ms.max(1)), seq });
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("untimer", lua.create_function(move |_, name: String| {
            s.borrow_mut().timers.retain(|t| t.name != name);
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("phase_handler", lua.create_function(move |_, name: Option<String>| {
            s.borrow_mut().phase_handler = name;
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("command_handler", lua.create_function(move |_, (name, on): (Option<String>, bool)| {
            let mut st = s.borrow_mut();
            match name {
                Some(name) => {
                    st.command_handlers.retain(|n| *n != name);
                    if on {
                        st.command_handlers.push(name);
                    }
                }
                // `unregister_command_handler()` without a name (the tutorial): all of them (INFERRED).
                None => st.command_handlers.clear(),
            }
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("selection_handler", lua.create_function(move |_, name: Option<String>| {
            s.borrow_mut().selection_handler = name;
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("input_handler", lua.create_function(move |_, name: Option<String>| {
            s.borrow_mut().input_handler = name;
            Ok(())
        })?)?;
        let s = self.state.clone();
        nb.set("random", lua.create_function(move |_, n: Option<f64>| {
            // Deterministic stand-in (the exe's generator is UNKNOWN): xorshift32.
            let mut st = s.borrow_mut();
            let mut x = st.rng;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            st.rng = x;
            let n = n.unwrap_or(1.0).max(1.0) as u32;
            Ok(f64::from(x % n))
        })?)?;
        let s = self.state.clone();
        nb.set("request", lua.create_function(move |_, (kind, args): (String, Table)| {
            let ids = |t: &Table| -> mlua::Result<Vec<u32>> {
                let list: Table = t.get("units")?;
                list.sequence_values::<u32>().collect()
            };
            let r = match kind.as_str() {
                "deploy_reinforcement" => {
                    BattleScriptRequest::DeployReinforcement { unit: args.get("unit")?, deploy: args.get("deploy")? }
                }
                "take_control" => BattleScriptRequest::TakeControl { units: ids(&args)? },
                "release_control" => BattleScriptRequest::ReleaseControl { units: ids(&args)? },
                "halt" => BattleScriptRequest::Halt { units: ids(&args)? },
                "fire_at_will" => BattleScriptRequest::FireAtWill { units: ids(&args)?, on: args.get("on")? },
                "attack_unit" => BattleScriptRequest::AttackUnit {
                    units: ids(&args)?,
                    target: args.get("target")?,
                    run: args.get::<Option<bool>>("run")?.unwrap_or(false),
                },
                "move" => BattleScriptRequest::Move {
                    units: ids(&args)?,
                    x: args.get("x")?,
                    z: args.get("z")?,
                    run: args.get::<Option<bool>>("run")?.unwrap_or(false),
                    angle: args.get("angle")?,
                    width: args.get("width")?,
                },
                "camera" => {
                    let v = |k: &str| -> mlua::Result<[f32; 3]> {
                        let t: Table = args.get(k)?;
                        Ok([t.get::<Option<f32>>("x")?.unwrap_or(0.0), t.get::<Option<f32>>("y")?.unwrap_or(0.0), t.get::<Option<f32>>("z")?.unwrap_or(0.0)])
                    };
                    BattleScriptRequest::Camera { target: v("target")?, position: v("position")?, seconds: args.get::<Option<f32>>("seconds")?.unwrap_or(0.0) }
                }
                "ui_visible" => BattleScriptRequest::UiVisible { name: args.get("name")?, visible: args.get("visible")? },
                "guard_mode" => BattleScriptRequest::GuardMode { units: ids(&args)?, on: args.get::<Option<bool>>("on")?.unwrap_or(true) },
                "marker" => BattleScriptRequest::Marker {
                    id: args.get("id")?,
                    name: args.get("name")?,
                    position: [args.get("x")?, args.get("y")?, args.get("z")?],
                    rotation_deg: args.get("rotation")?,
                    scale: args.get("scale")?,
                    visible: args.get("visible")?,
                },
                "set_invincible" => BattleScriptRequest::SetInvincible { units: ids(&args)?, on: args.get::<Option<bool>>("on")?.unwrap_or(false) },
                "skirmish" => BattleScriptRequest::Skirmish { units: ids(&args)?, on: args.get::<Option<bool>>("on")?.unwrap_or(false) },
                "select_deployable" => BattleScriptRequest::SelectDeployable { units: ids(&args)?, name: args.get("name")? },
                "special_ability" => BattleScriptRequest::SpecialAbility { units: ids(&args)?, name: args.get("name")? },
                "shot_type" => BattleScriptRequest::ShotType { units: ids(&args)?, name: args.get("name")? },
                "script_morale" => BattleScriptRequest::ScriptMorale { units: ids(&args)?, mode: args.get("mode")? },
                "defend_building" => BattleScriptRequest::DefendBuilding {
                    units: ids(&args)?,
                    building: args.get("building")?,
                    run: args.get::<Option<bool>>("run")?.unwrap_or(false),
                },
                _ => BattleScriptRequest::Other { order: kind, units: ids(&args).unwrap_or_default() },
            };
            s.borrow_mut().requests.push(r);
            Ok(())
        })?)?;
        lua.globals().set("__nb", nb)?;

        // `require` through the install (the battle scripts set package.path to "...;data/?.lua").
        let package: Table = lua.globals().get("package")?;
        let loaders: Table = package.get("loaders")?;
        let searcher = lua.create_function(move |lua, name: String| {
            let package: Table = lua.globals().get("package")?;
            let path: String = package.get("path")?;
            let file = name.replace('.', "/");
            let mut tried = String::new();
            for template in path.split(';').filter(|t| !t.is_empty()) {
                let candidate = template.replace('?', &file);
                match source.find(&candidate) {
                    Some(f) => return Ok(Value::Function(crate::host::load_chunk(lua, &f.bytes, &f.chunk_name)?)),
                    None => tried.push_str(&format!("\n\tno file '{candidate}' in the install")),
                }
            }
            Ok(Value::String(lua.create_string(&tried)?))
        })?;
        let n = loaders.raw_len();
        for i in (2..=n).rev() {
            let v: Value = loaders.raw_get(i)?;
            loaders.raw_set(i + 1, v)?;
        }
        loaders.raw_set(2, searcher)?;
        lua.load(PRELUDE).set_name("@ntw_battle_script_prelude.lua").exec()
    }

    /// Runs the battle script (the file's bytes; text or compiled Lua).
    pub fn load(&self, chunk_name: &str, bytes: &[u8]) -> mlua::Result<()> {
        crate::host::load_chunk(&self.lua, bytes, chunk_name)?.call::<()>(())
    }

    /// Replaces the battle snapshot (call before [`BattleScriptHost::advance`] / [`BattleScriptHost::phase`]).
    pub fn set_facts(&self, facts: BattleScriptFacts) {
        let mut st = self.state.borrow_mut();
        // Buildings do not change: an empty list keeps the one given before.
        let old_camera = st.facts.camera;
        let old = std::mem::take(&mut st.facts.buildings);
        st.facts = facts;
        if st.facts.camera == ([0.0; 3], [0.0; 3]) {
            st.facts.camera = old_camera;
        }
        if st.facts.buildings.is_empty() {
            st.facts.buildings = old;
        }
        st.version += 1;
    }

    /// Battle phase change: calls the registered phase handler with an event whose `get_name()` is
    /// `name` ("Deployment", "Deployed", ... CONFIRMED names used by the scripts).
    pub fn phase(&self, name: &str) {
        let handler = self.state.borrow().phase_handler.clone();
        if let Some(h) = handler {
            self.call_event(&h, Some(name), None);
        }
    }

    /// The battle's result for the command handlers ("Battle Results", `get_bool1()` = the player
    /// won; CONFIRMED use in `scripting_library.lua`).
    pub fn results(&self, player_won: bool) {
        let handlers = self.state.borrow().command_handlers.clone();
        for h in handlers {
            self.call_event(&h, Some("Battle Results"), Some(player_won));
        }
    }

    /// Fires every timer due up to the snapshot's time (call after [`BattleScriptHost::set_facts`]).
    pub fn advance(&self) {
        loop {
            let next = {
                let mut st = self.state.borrow_mut();
                let now = st.facts.time_ms;
                let Some(i) = (0..st.timers.len())
                    .filter(|&i| st.timers[i].due <= now)
                    .min_by_key(|&i| (st.timers[i].due, st.timers[i].seq))
                else {
                    break;
                };
                let t = st.timers[i].clone();
                match t.period {
                    Some(p) => st.timers[i].due = t.due + p,
                    None => {
                        st.timers.remove(i);
                    }
                }
                t.name
            };
            self.call_event(&next, None, None);
        }
    }

    /// An event object (`nb.event`): name, `get_bool1`, `get_string1` and `get_unit` (the unit with
    /// model id `unit`, as the same Lua object the script holds).
    fn event(&self, name: &str, bool1: bool, string1: &str, unit: Option<u32>) -> Option<Value> {
        let index = unit.and_then(|id| self.unit_index(id));
        let mk: Function = match self.lua.globals().get::<Table>("__nb").and_then(|t| t.get("event")) {
            Ok(f) => f,
            Err(e) => {
                self.state.borrow_mut().log.push(format!("ERROR: {e}"));
                return None;
            }
        };
        match mk.call::<Value>((name, bool1, string1, index)) {
            Ok(v) => Some(v),
            Err(e) => {
                self.state.borrow_mut().log.push(format!("ERROR: {e}"));
                None
            }
        }
    }

    /// The 1-based index of model unit `id` in the snapshot (the scripts' unit objects).
    fn unit_index(&self, id: u32) -> Option<usize> {
        self.state.borrow().facts.units.iter().position(|u| u.id == id).map(|i| i + 1)
    }

    fn call_event(&self, name: &str, event: Option<&str>, bool1: Option<bool>) {
        let args = match event {
            Some(ev) => match self.event(ev, bool1.unwrap_or(false), "", None) {
                Some(v) => MultiValue::from_vec(vec![v]),
                None => return,
            },
            None => MultiValue::new(),
        };
        self.call_global(name, args);
    }

    fn call_global(&self, name: &str, args: MultiValue) {
        let f: Option<Function> = self.lua.globals().get(name).ok();
        let Some(f) = f else {
            self.state.borrow_mut().log.push(format!("ERROR: no script function {name}"));
            return;
        };
        if let Err(e) = f.call::<()>(args) {
            self.state.borrow_mut().log.push(format!("ERROR in {name}: {e}"));
        }
    }

    /// A player command for the command handlers: the original's event names ("Move", "Attack
    /// Unit", "Fire At Will", "Change Speed", "Double Click", "Halt", ... CONFIRMED strings next
    /// to the binding table), `get_unit()` = the unit the command is about (for "Attack Unit" the
    /// target, INFERRED from the tutorial), `get_bool1()` / `get_string1()`.
    pub fn command(&self, name: &str, unit: Option<u32>, bool1: bool, string1: &str) {
        let handlers = self.state.borrow().command_handlers.clone();
        for h in handlers {
            if let Some(v) = self.event(name, bool1, string1, unit) {
                self.call_global(&h, MultiValue::from_vec(vec![v]));
            }
        }
    }

    /// A selection change for the unit selection handler: `handler(unit, selected)` (INFERRED
    /// arguments from the tutorial's `Currently_Selected(Unit, selected)`); `unit` `None` = nothing
    /// selected.
    pub fn selection(&self, unit: Option<u32>, selected: bool) {
        let Some(h) = self.state.borrow().selection_handler.clone() else { return };
        let obj: Value = match unit.and_then(|id| self.unit_index(id)) {
            Some(i) => match self.lua.globals().get::<Table>("__nb").and_then(|t| t.get::<Function>("unit_object")).and_then(|f| f.call::<Value>(i)) {
                Ok(v) => v,
                Err(e) => {
                    self.state.borrow_mut().log.push(format!("ERROR: {e}"));
                    return;
                }
            },
            None => Value::Nil,
        };
        self.call_global(&h, MultiValue::from_vec(vec![obj, Value::Boolean(selected)]));
    }

    /// A camera input action for the input handler: `handler(name)` with the original's action
    /// names ([`INPUT_EVENT_NAMES`]).
    pub fn input(&self, name: &str) {
        let Some(h) = self.state.borrow().input_handler.clone() else { return };
        let s = match self.lua.create_string(name) {
            Ok(s) => s,
            Err(_) => return,
        };
        self.call_global(&h, MultiValue::from_vec(vec![Value::String(s)]));
    }

    /// Requests made since the last call.
    pub fn take_requests(&self) -> Vec<BattleScriptRequest> {
        std::mem::take(&mut self.state.borrow_mut().requests)
    }

    /// Log lines since the last call (script `out` calls, UNKNOWN stubs, errors).
    pub fn take_log(&self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().log)
    }

    /// Names of the timers still registered (tests and debugging).
    pub fn timer_names(&self) -> Vec<String> {
        self.state.borrow().timers.iter().map(|t| t.name.clone()).collect()
    }

    /// The Lua state (tests).
    pub fn lua(&self) -> &Lua {
        &self.lua
    }
}

//! # ntw_script: the Lua scripting layer
//!
//! Napoleon: Total War drives its campaigns' story with Lua scripts: the missions ("take Vienna"),
//! the starting money, which factions may make peace, the advisor's hints and the generated trait
//! and ancillary rules. This crate runs those **original scripts, unchanged**, read at runtime from
//! the player's own install (never copied into this project), against our
//! [`CampaignModel`](ntw_sim::campaign::CampaignModel).
//!
//! How it works, in plain words (details and evidence in `docs/DESIGN.md` §3.5):
//! 1. [`ScriptHost::new`] creates a Lua 5.1 interpreter (the same Lua version as the original,
//!    W3 §6.1 CONFIRMED) and fills it with what the engine normally provides: `GAME(context)`
//!    (the "game_interface", [`game`]), `conditions.*` and `effect.*` ([`conditions`]), `bit`, and
//!    stand-ins for the UI tables (`prelude.lua`).
//! 2. `require "x.y"` is answered from the game's `.pack` files and loose files ([`source`]).
//!    Precompiled UI scripts (`.luac`) are converted on the fly ([`luac`]) because the original
//!    compiled them with 4-byte float numbers.
//! 3. [`ScriptHost::load_campaign`] runs `data/all_scripted.lua` and then
//!    `data/campaigns/<campaign>/scripting.lua`. Those scripts register their handlers in the
//!    `events` tables (`scripting.AddEventCallBack("FactionTurnStart", f)`).
//! 4. [`ScriptHost::fire`] fires an event: every registered function is called with a `context`
//!    ([`ScriptContext`]). [`ScriptHost::end_turn`] runs the model's end of turn and forwards its
//!    events (`FactionTurnStart`, `RegionTurnStart`, ...) to the scripts.
//!
//! **Numbers:** the original's Lua uses 4-byte `float` numbers; ours uses `double`. Every number
//! that crosses between Rust and Lua is rounded to `f32`; arithmetic inside scripts stays `double`
//! (a documented fidelity gap, DESIGN §3.5).
//!
//! Evidence tags as elsewhere: CONFIRMED / INFERRED / UNKNOWN / PLACEHOLDER. Every engine function
//! that has no real behaviour yet is a **logging stub tagged UNKNOWN**: it writes a line to
//! [`ScriptState::log`] and returns `nil`, so the scripts keep running.

mod bit;
pub mod conditions;
pub mod characters;
pub mod game;
pub mod host;
pub mod battle_script;
pub mod luac;
pub mod source;
pub mod state;
pub mod ui;

pub use host::{AiTurnHook, FireReport, ScriptError, ScriptHost};
pub use source::{ScriptFile, ScriptSource};
pub use state::{CustomMission, ScriptContext, ScriptState, ScriptValue, TimeTrigger};

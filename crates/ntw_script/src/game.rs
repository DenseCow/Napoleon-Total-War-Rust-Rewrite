//! The campaign `game_interface` object that `GAME(context)` returns (W3 §6.2, CONFIRMED: the
//! episodic scripting module does `game_interface = GAME(context)` on `NewSession`).
//!
//! Scripts call it with method syntax, `scripting.game_interface:treasury_mod("france", 2000)`, so
//! every function below receives the object itself as argument 0 and the real arguments from 1.
//!
//! All 63 method names the shipped scripts use (W3 `lua_api.txt`, CONFIRMED names and argument
//! counts) exist. The ones listed in [`dispatch`] do something; every other one is a **logging stub
//! (UNKNOWN behaviour)**. A name not in the list also resolves to a stub, so modded scripts load.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Table, Value};
use ntw_sim::campaign::CampaignCommand;

use crate::state::{CustomMission, ScriptState, ScriptValue, TimeTrigger};

/// The shared script state, as seen from the Rust functions bound into Lua.
pub(crate) type Shared = Rc<RefCell<ScriptState>>;

/// The 63 game_interface methods called by the shipped scripts (W3 `lua_api.txt`, CONFIRMED).
pub const GAME_INTERFACE_METHODS: [&str; 63] = [
    "add_attack_of_opportunity_overrides",
    "add_building_model_override",
    "add_custom_battlefield",
    "add_exclusion_zone",
    "add_location_trigger",
    "add_marker",
    "add_restricted_building_level_record",
    "add_restricted_unit_record",
    "add_settlement_model_override",
    "add_time_trigger",
    "add_visibility_trigger",
    "advance_to_next_campaign",
    "award_experience_level",
    "cancel_actions_for",
    "disable_elections",
    "disable_movement_for_character",
    "disable_rebellions_and_revolutions_worldwide",
    "disable_saving_game",
    "disable_shopping_for_ai_under_shroud",
    "disable_town_spawning",
    "enable_auto_generated_missions",
    "enable_movement_for_character",
    "exempt_region_from_tax",
    "force_add_trait",
    "force_assassination_success_for_human",
    "force_declare_war",
    "force_diplomacy",
    "force_garrison_infiltration_success_for_human",
    "force_make_peace",
    "force_make_protectorate",
    "force_make_trade_agreement",
    "force_rebellion_in_region",
    "grant_faction_handover",
    "grant_unit",
    "is_new_game",
    "join_garrison",
    "load_value",
    "other_income_mod",
    "register_instant_movie",
    "register_movies",
    "register_outro_movie",
    "remove_barrier",
    "remove_restricted_building_level_record",
    "remove_restricted_unit_record",
    "remove_time_trigger",
    "save_value",
    "set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour",
    "set_liberation_options_disabled",
    "set_looting_options_disabled_for_human",
    "set_map_bounds",
    "set_non_scripted_ancillaries_disabled",
    "set_non_scripted_traits_disabled",
    "set_tax_rate",
    "set_technology_research_disabled",
    "set_ui_notification_of_victory_disabled",
    "set_zoom_limit",
    "show_message_event",
    "show_shroud",
    "spawn_town_level",
    "steal_user_input",
    "treasury_mod",
    "trigger_custom_mission",
    "unveil_black_shroud",
];

// ---------------------------------------------------------------------------------------------
// Argument helpers (shared with `conditions`)
// ---------------------------------------------------------------------------------------------

/// Argument `i` as a string (numbers are converted, as Lua's C API does).
pub(crate) fn arg_str(args: &MultiValue, i: usize) -> Option<String> {
    match args.get(i)? {
        Value::String(s) => Some(s.to_string_lossy()),
        Value::Number(n) => Some(n.to_string()),
        Value::Integer(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Argument `i` as a number, **rounded to `f32`** (the original's `lua_Number`, DESIGN §3.5).
pub(crate) fn arg_f32(args: &MultiValue, i: usize) -> Option<f32> {
    match args.get(i)? {
        Value::Number(n) => Some(*n as f32),
        Value::Integer(n) => Some(*n as f32),
        Value::String(s) => s.to_string_lossy().trim().parse().ok(),
        _ => None,
    }
}

/// Argument `i` with Lua truthiness (only `nil` and `false` are false).
pub(crate) fn arg_bool(args: &MultiValue, i: usize) -> bool {
    !matches!(args.get(i), None | Some(Value::Nil) | Some(Value::Boolean(false)))
}

/// A short readable form of the arguments, for log lines.
pub(crate) fn describe(args: &MultiValue, skip: usize) -> String {
    args.iter()
        .skip(skip)
        .map(|v| match v {
            Value::String(s) => format!("{:?}", s.to_string_lossy()),
            Value::Number(n) => format!("{}", *n as f32),
            Value::Integer(n) => n.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Nil => "nil".to_string(),
            other => other.type_name().to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A Lua value as a [`ScriptValue`] (tables, functions and userdata become `Nil`: UNKNOWN whether
/// the original can save them; the shipped scripts only save booleans and numbers).
pub(crate) fn to_script_value(v: &Value) -> ScriptValue {
    match v {
        Value::Boolean(b) => ScriptValue::Bool(*b),
        Value::Number(n) => ScriptValue::Number(*n as f32),
        Value::Integer(n) => ScriptValue::Number(*n as f32),
        Value::String(s) => ScriptValue::String(s.to_string_lossy()),
        _ => ScriptValue::Nil,
    }
}

/// A [`ScriptValue`] as a Lua value (`f32` widens exactly to Lua's `double`).
pub(crate) fn from_script_value(lua: &Lua, v: &ScriptValue) -> mlua::Result<Value> {
    Ok(match v {
        ScriptValue::Nil => Value::Nil,
        ScriptValue::Bool(b) => Value::Boolean(*b),
        ScriptValue::Number(n) => Value::Number(f64::from(*n)),
        ScriptValue::String(s) => Value::String(lua.create_string(s)?),
    })
}

/// Appends a log line.
pub(crate) fn log(shared: &Shared, line: String) {
    shared.borrow_mut().log.push(line);
}

// ---------------------------------------------------------------------------------------------
// The object
// ---------------------------------------------------------------------------------------------

/// Builds the game_interface table. One object per host (INFERRED: `GAME(context)` returns the
/// campaign's single interface; calling it again returns the same table).
pub(crate) fn create(lua: &Lua, shared: &Shared) -> mlua::Result<Table> {
    let gi = lua.create_table()?;
    for name in GAME_INTERFACE_METHODS {
        gi.set(name, method(lua, shared, name)?)?;
    }
    // Any other name: a logging stub too (UNKNOWN), created on first use.
    let meta = lua.create_table()?;
    let s = shared.clone();
    meta.set(
        "__index",
        lua.create_function(move |lua, (_t, key): (Table, String)| {
            let s = s.clone();
            lua.create_function(move |_, args: MultiValue| {
                log(&s, format!("UNKNOWN game_interface:{key}({}) (not a known method)", describe(&args, 1)));
                Ok(())
            })
        })?,
    )?;
    gi.set_metatable(Some(meta))?;
    Ok(gi)
}

fn method(lua: &Lua, shared: &Shared, name: &'static str) -> mlua::Result<Function> {
    let s = shared.clone();
    lua.create_function(move |lua, args: MultiValue| dispatch(lua, &s, name, &args))
}

/// One game_interface call. `args[0]` is the object itself.
fn dispatch(lua: &Lua, s: &Shared, name: &'static str, args: &MultiValue) -> mlua::Result<Value> {
    let text = format!("game_interface:{name}({})", describe(args, 1));
    match name {
        // CONFIRMED name; true until the campaign has been saved and loaded (INFERRED meaning).
        "is_new_game" => return Ok(Value::Boolean(s.borrow().is_new_game)),

        // treasury_mod(faction, amount): INFERRED "add amount to the treasury". The f32 amount is
        // truncated towards zero into the i32 treasury (INFERRED: a C float→int cast).
        "treasury_mod" => {
            let (Some(faction), Some(amount)) = (arg_str(args, 1), arg_f32(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            match st.faction_id(&faction) {
                Some(id) => {
                    let f = st.model.world.factions.get_mut(&id).expect("id from lookup");
                    f.treasury = f.treasury.wrapping_add(amount as i32);
                    st.log.push(text);
                }
                None => st.log.push(format!("{text}: unknown faction")),
            }
        }

        // show_shroud(on) / unveil_black_shroud(on): the fog of war (CONFIRMED names and argument
        // count from the shipped campaign scripts -- 26 and 15 call sites, one boolean each).
        // INFERRED meaning, from the names and what the model holds (`World::shrouds`, shroud #3
        // "the shroud is on; off, everything is visible"):
        //   show_shroud(true/false)      -> the local player's shroud on / off;
        //   unveil_black_shroud(true)    -> the whole map is explored for the local player (the
        //                                 black shroud is lifted), so nothing is hidden from him;
        //   unveil_black_shroud(false)   -> nothing to put back, so it only logs.
        // Which faction the calls name is UNKNOWN (they take no faction); the shroud is the human's
        // (only he keeps one, INFERRED: the start positions).
        "show_shroud" | "unveil_black_shroud" => {
            let on = arg_bool(args, 1);
            let mut st = s.borrow_mut();
            let human = st.local_faction.clone();
            let Some(id) = st.faction_id(&human) else {
                st.log.push(format!("{text}: unknown faction {human:?}"));
                return Ok(Value::Nil);
            };
            let grid = st.model.world.sight_grid;
            let Some(shroud) = st.model.world.shrouds.get_mut(&id) else {
                st.log.push(format!("{text}: {human} has no shroud"));
                return Ok(Value::Nil);
            };
            if name == "show_shroud" {
                shroud.active = on;
            } else if on
                && let Some(grid) = grid
            {
                // Every cell of the grid explored: what "unveiling" means for the player.
                let cols = grid.cols;
                let rows = grid.rows;
                for x in 0..cols {
                    for z in 0..rows {
                        shroud.explored.set(x, z);
                    }
                }
            }
            st.log.push(text);
        }

        // other_income_mod(faction, amount): stored only (see ScriptState::other_income). UNKNOWN
        // how the original applies it.
        "other_income_mod" => {
            let (Some(faction), Some(amount)) = (arg_str(args, 1), arg_f32(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            *st.other_income.entry(faction).or_insert(0.0) += amount;
            st.log.push(text);
        }

        // force_make_peace / force_declare_war(a, b) → the model's own diplomacy commands.
        "force_make_peace" | "force_declare_war" => {
            let (Some(a), Some(b)) = (arg_str(args, 1), arg_str(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            let (Some(a_id), Some(b_id)) = (st.faction_id(&a), st.faction_id(&b)) else {
                st.log.push(format!("{text}: unknown faction"));
                return Ok(Value::Nil);
            };
            let cmd = if name == "force_make_peace" {
                CampaignCommand::MakePeace { a: a_id, b: b_id }
            } else {
                CampaignCommand::DeclareWar { a: a_id, b: b_id }
            };
            let result = st.model.apply(cmd);
            st.log.push(match result {
                Ok(_) => text,
                Err(e) => format!("{text}: refused by the model ({e:?})"),
            });
        }

        // force_diplomacy(a, b, option, offer, accept): CONFIRMED argument count (5); INFERRED
        // meaning "may a offer / accept <option> with b". Stored; the diplomacy AI that would read
        // it does not exist yet.
        "force_diplomacy" => {
            let (Some(a), Some(b), Some(option)) = (arg_str(args, 1), arg_str(args, 2), arg_str(args, 3))
            else {
                return bad_args(s, text);
            };
            let flags = (arg_bool(args, 4), arg_bool(args, 5));
            s.borrow_mut().diplomacy_options.insert((a, b, option), flags);
        }

        // add_time_trigger(name, seconds) / remove_time_trigger(name): fires the `TimeTrigger` event
        // with context.string = name after `seconds` of script time (INFERRED from the scripts).
        "add_time_trigger" => {
            let (Some(trigger), Some(secs)) = (arg_str(args, 1), arg_f32(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            let fire_at = st.time + secs;
            st.time_triggers.push(TimeTrigger { name: trigger, fire_at });
            st.log.push(text);
        }
        "remove_time_trigger" => {
            let Some(trigger) = arg_str(args, 1) else { return bad_args(s, text) };
            let mut st = s.borrow_mut();
            st.time_triggers.retain(|t| t.name != trigger);
            st.log.push(text);
        }

        // save_value(v, context) / load_value(default, context): positional values (W3 §9,
        // CONFIRMED "LUA[] positional values"). load_value returns the next stored value, or the
        // default when there is none (INFERRED: a new game has none).
        "save_value" => {
            let v = args.get(1).map_or(ScriptValue::Nil, to_script_value);
            s.borrow_mut().saved_values.push(v);
        }
        "load_value" => {
            let next = s.borrow_mut().values_to_load.pop_front();
            return match next {
                Some(v) => from_script_value(lua, &v),
                None => Ok(args.get(1).cloned().unwrap_or(Value::Nil)),
            };
        }

        "add_restricted_unit_record" | "remove_restricted_unit_record" => {
            let Some(key) = arg_str(args, 1) else { return bad_args(s, text) };
            let mut st = s.borrow_mut();
            if name.starts_with("add") {
                st.restricted_units.insert(key);
            } else {
                st.restricted_units.remove(&key);
            }
            st.log.push(text);
        }
        "add_restricted_building_level_record" | "remove_restricted_building_level_record" => {
            let Some(key) = arg_str(args, 1) else { return bad_args(s, text) };
            let mut st = s.borrow_mut();
            if name.starts_with("add") {
                st.model.world.restricted_buildings.insert(key);
            } else {
                st.model.world.restricted_buildings.remove(&key);
            }
            st.log.push(text);
        }

        // trigger_custom_mission(key, faction, type, 0, target, heading, text, reward_text, 0, "",
        // context, bool, rewards...) (W3 §7, CONFIRMED argument layout). Recorded only:
        // PLACEHOLDER, missions are not tracked or rewarded yet.
        "trigger_custom_mission" => {
            let mission = CustomMission {
                key: arg_str(args, 1).unwrap_or_default(),
                faction: arg_str(args, 2).unwrap_or_default(),
                kind: arg_str(args, 3).unwrap_or_default(),
                target: arg_str(args, 5).unwrap_or_default(),
                rewards: (13..args.len()).filter_map(|i| arg_str(args, i)).collect(),
            };
            let mut st = s.borrow_mut();
            st.missions.push(mission);
            st.log.push(format!("PLACEHOLDER {text}"));
        }

        // Everything else: UNKNOWN behaviour, logged so the effect of a script can be inspected.
        _ => log(s, format!("UNKNOWN stub {text}")),
    }
    Ok(Value::Nil)
}

fn bad_args(s: &Shared, text: String) -> mlua::Result<Value> {
    log(s, format!("{text}: unexpected arguments, ignored"));
    Ok(Value::Nil)
}

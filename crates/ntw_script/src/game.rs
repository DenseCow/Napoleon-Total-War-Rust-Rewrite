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

/// game_interface methods the exe registers (its script method table, e.g. `display_turns` at
/// `0x01458158`) that no shipped script calls but a mod's script may (CONFIRMED name and behaviour
/// at each one's `dispatch` arm).
pub const MOD_GAME_INTERFACE_METHODS: [&str; 1] = ["display_turns"];

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

/// The type the exe's argument readers name in their script error ("found type %s", `lua_typename`
/// of the stack top, Lua 5.1): a missing argument finds the slot below the arguments, the called
/// function itself (see `faction_key_args`, ui/campaign/diplomacy.rs); a light userdata is
/// "userdata" and every number is "number" (mlua's own names "lightuserdata" and "integer" are not
/// Lua's).
pub(crate) fn lua_found_type(v: Option<&Value>) -> &'static str {
    match v {
        None => "function",
        Some(Value::LightUserData(_)) => "userdata",
        Some(Value::Integer(_) | Value::Number(_)) => "number",
        Some(v) => v.type_name(),
    }
}

/// Logs the exe's argument-type script error "Trying to retrieve type `want`, found type `found`"
/// (`0x01095090` → the logger `0x0106BD10`, no Lua error raised) once per method and type found.
/// The exe logs it on every call, and its cleared ok flag silences the later argument reads of
/// that script context too (the ORIGINAL BUG noted at `CreateFromLayout`, ui/host.rs); ours fails
/// just the call.
fn type_error_once(s: &Shared, name: &'static str, want: &str, found: &'static str) {
    let mut st = s.borrow_mut();
    if st.type_errors_logged.insert((name, found)) {
        st.log.push(format!("ERROR game_interface:{name}: Trying to retrieve type {want}, found type {found} (logged once)"));
    }
}

/// The argument of a one-boolean binding as the exe's reader `0x01055840` takes it (CONFIRMED by
/// disassembly): the top of the Lua stack (`lua_type(L, -1)`, then `lua_settop(L, -2)`), i.e. the
/// **last** value passed (any before it are ignored, and the binding never checks the object: its
/// `this` comes from the registration, not the stack), and only a real Lua boolean (a number,
/// string or nil is not read, whatever its truthiness). Anything else is the type error, logged
/// once ([`type_error_once`]), and `None`; with no argument the top is the object itself
/// (`game_interface:show_shroud()`), or the called function when nothing at all is passed.
fn top_bool(s: &Shared, name: &'static str, args: &MultiValue) -> Option<bool> {
    match args.back() {
        Some(Value::Boolean(b)) => Some(*b),
        v => {
            type_error_once(s, name, "boolean", lua_found_type(v));
            None
        }
    }
}

/// The argument of a one-string binding as the exe's reader `0x01055760` takes it (CONFIRMED by
/// disassembly): the top of the Lua stack, as [`top_bool`], when `lua_isstring` (a string, or a
/// number turned into its string); anything else is the type error, logged once
/// ([`type_error_once`]), and `None`.
fn top_str(lua: &Lua, s: &Shared, name: &'static str, args: &MultiValue) -> Option<String> {
    let top = args.back();
    if let Some(Ok(Some(text))) = top.map(|v| lua.coerce_string(v.clone())) {
        return Some(text.to_string_lossy());
    }
    type_error_once(s, name, "string", lua_found_type(top));
    None
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
    for name in GAME_INTERFACE_METHODS.into_iter().chain(MOD_GAME_INTERFACE_METHODS) {
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

        // treasury_mod(faction, amount): CONFIRMED (`0x0097BAD0`) the amount is rounded (FISTP) and
        // credited only when above 0; an unknown faction changes nothing
        // (ntw_sim::campaign::treasury::script_credit).
        "treasury_mod" => {
            let (Some(faction), Some(amount)) = (arg_str(args, 1), arg_f32(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            match st.faction_id(&faction) {
                Some(id) => {
                    let f = st.model.world.factions.get_mut(&id).expect("id from lookup");
                    ntw_sim::campaign::treasury::script_credit(&mut f.treasury, amount);
                    st.log.push(text);
                }
                None => st.log.push(format!("{text}: unknown faction")),
            }
        }

        // show_shroud(on): CONFIRMED (binding `0x00979260`, one boolean read by `0x01055840`; 26
        // shipped call sites, one boolean each): every faction's shroud on or off
        // (`CampaignModel::show_shroud`). ORIGINAL BUG: `0x00979260` does not clear its result
        // before the read (`0x01055840` writes it only for a real Lua boolean, and the slot is the
        // lua_State argument's), so a non-boolean (number, nil, no argument) stores that pointer's
        // low byte, an arbitrary value, into every shroud. Ours stores false, as `display_turns`.
        "show_shroud" => {
            let on = top_bool(s, name, args).unwrap_or(false);
            let mut st = s.borrow_mut();
            st.model.show_shroud(on);
            st.log.push(text);
        }

        // unveil_black_shroud(key): CONFIRMED (binding `0x00977A90`, one string read by `0x01055760`;
        // the tutorial's 9 call sites pass "shroud_map_1_1" .. "shroud_map_1_9", once each; a data.pack
        // feature-list script (`m_features`, "show_shroud_1_1" ..) passes 1_1 .. 1_3 and 2_1 .. 2_3):
        // every shroud's black shroud is lifted inside the map's outlines of that name
        // (`regions.esf` `black_shroud_outlines`, `CampaignModel::unveil_black_shroud`). A name no
        // outline has does nothing in the exe; ours says so in the log line. On a non-string the
        // reader logs the type error and leaves the name empty, and `0x00977A90` still calls
        // `0x008F7760` with it (no ok-flag check), as ours does.
        "unveil_black_shroud" => {
            let key = top_str(lua, s, name, args).unwrap_or_default();
            let mut st = s.borrow_mut();
            if st.model.unveil_black_shroud(&key) == 0 {
                st.log.push(format!("{text}: no black shroud outline {key:?} on this map"));
            } else {
                st.log.push(text);
            }
        }

        // other_income_mod(faction, amount): CONFIRMED (`0x0097BB70` → `0x00A15B30`) a lasting other
        // income entry (or, negative and for an AI faction, an expense entry) in the faction's running
        // economics record (ntw_sim::campaign::economy::script_other_income_mod); an unknown faction
        // changes nothing.
        "other_income_mod" => {
            let (Some(faction), Some(amount)) = (arg_str(args, 1), arg_f32(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            match st.faction_id(&faction) {
                Some(id) => {
                    ntw_sim::campaign::economy::script_other_income_mod(&mut st.model, id, amount);
                    st.log.push(text);
                }
                None => st.log.push(format!("{text}: unknown faction")),
            }
        }

        // grant_faction_handover(killer, target, min_turn, max_turn): CONFIRMED (binding `0x00978950`):
        // the turns are read first, each a Lua number only (`0x01055700`, type 3; the f32 `lua_Number`,
        // DESIGN §3.5) rounded ties to even (`0x010556D0`), then the two keys. Another type logs the
        // type error and clears the reader's ok flag, so the keys read empty, no faction is found and
        // nothing happens (ours: logged).
        // Unknown killer or target: nothing. The rule (window, draw, destruction) is the model's
        // (`CampaignModel::grant_faction_handover`). Its events (the forces destroyed, the killer's new
        // garrison colonel, a human target's `FactionDestroyed` and defeat movie) are queued for the host,
        // which fires them once the calling Lua returns (`ScriptHost::fire_script_events`).
        "grant_faction_handover" => {
            let int = |i: usize| -> Option<i32> {
                let x = match args.get(i)? {
                    Value::Integer(n) => *n as f32,
                    Value::Number(n) => *n as f32,
                    _ => return None,
                };
                let r = x.round_ties_even();
                // Out of range is the x87 "integer indefinite".
                Some(if (-2_147_483_648.0..2_147_483_648.0).contains(&r) { r as i32 } else { i32::MIN })
            };
            let (Some(killer), Some(target), Some(min_turn), Some(max_turn)) = (arg_str(args, 1), arg_str(args, 2), int(3), int(4)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            let (Some(killer_id), Some(target_id)) = (st.faction_id(&killer), st.faction_id(&target)) else {
                st.log.push(format!("{text}: unknown faction"));
                return Ok(Value::Nil);
            };
            let line = match st.model.grant_faction_handover(killer_id, target_id, min_turn, max_turn) {
                Some(events) => {
                    st.script_events.extend(events);
                    format!("{text}: {target} destroyed by {killer}")
                }
                None => text,
            };
            st.log.push(line);
        }

        // force_make_peace / force_declare_war(a, b) → the model's own diplomacy commands.
        // PROVISIONAL: the exe's force_make_peace (`0x0097AC90`) also queues the move-armies-out
        // command (`0x009838B0`, `CampaignCommand::MoveArmiesOutOfRegions`) after the peace when the
        // two are at war (CONFIRMED); ours sends only the peace (BACKLOG §0).
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
                CampaignCommand::MakePeace { a: a_id, b: b_id, suppress_messages: false }
            } else {
                CampaignCommand::DeclareWar { a: a_id, b: b_id }
            };
            let result = st.model.apply(cmd);
            st.log.push(match result {
                Ok(_) => text,
                Err(e) => format!("{text}: refused by the model ({e})"),
            });
        }

        // force_make_protectorate(patron, protectorate) (`HandleScriptForceMakeProtectorate`
        // `0x0097AA20`, CONFIRMED; vanilla: `force_make_protectorate("ita_french_republic",
        // "ita_piedmont")`, France the patron): nothing unless both factions exist and differ; at
        // war, first a peace with its messages off (`CCQ_MAKE_PEACE`, suppress byte 1); then, unless
        // either side already has a patron (`0x008CEEA0`), the protectorate (`CCQ_MAKE_PROTECTORATE`,
        // whose handler checks again); then always each side's forces out of the other's regions
        // (`CCQ_MOVE_ARMIES_OUT_OF_REGIONS`). The exe queues the three commands; ours applies them
        // in the same order. The peace holds in any faction's turn: the `CCQ_MAKE_PEACE` handler
        // `0x00936710` checks only that both factions exist, differ and are at war (`0x008CE9B0`)
        // before `0x00B262C0` (`CampaignCommand::MakePeace` has no turn test). Like every
        // command, each of the three is refused while a battle is pending (`CampaignModel::apply`).
        // Their events are queued for the host, as `grant_faction_handover`'s.
        "force_make_protectorate" => {
            let (Some(patron), Some(protectorate)) = (arg_str(args, 1), arg_str(args, 2)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            let (Some(patron), Some(protectorate)) = (st.faction_id(&patron), st.faction_id(&protectorate)) else {
                st.log.push(format!("{text}: unknown faction"));
                return Ok(Value::Nil);
            };
            if patron == protectorate {
                st.log.push(format!("{text}: the same faction twice"));
                return Ok(Value::Nil);
            }
            let mut results = Vec::with_capacity(3);
            if st.model.at_war(patron, protectorate) {
                results.push(st.model.apply(CampaignCommand::MakePeace { a: patron, b: protectorate, suppress_messages: true }));
            }
            if !st.model.has_patron(patron) && !st.model.has_patron(protectorate) {
                results.push(st.model.apply(CampaignCommand::MakeProtectorate { patron, protectorate }));
            }
            results.push(st.model.apply(CampaignCommand::MoveArmiesOutOfRegions { a: patron, b: protectorate }));
            let mut refused = Vec::new();
            for result in results {
                match result {
                    Ok(events) => st.script_events.extend(events),
                    Err(e) => refused.push(e.to_string()),
                }
            }
            st.log.push(if refused.is_empty() { text } else { format!("{text}: refused by the model ({})", refused.join(", ")) });
        }

        // force_diplomacy(a, b, option, offer, accept) (`SetDiplomacyOptionFromScript`
        // `0x009792D0`, CONFIRMED): a and b are exact faction keys (no "all" or culture form), the
        // option an exact `DIPLOMACY_OPTIONS` key, offer and accept Lua booleans (another type is a
        // script error there, nothing stored). Sets a's permission towards b only; the model's
        // negotiation panel, AI and saves read it from there.
        "force_diplomacy" => {
            let (Some(a), Some(b), Some(option)) = (arg_str(args, 1), arg_str(args, 2), arg_str(args, 3)) else {
                return bad_args(s, text);
            };
            let (Some(Value::Boolean(offer)), Some(Value::Boolean(accept))) = (args.get(4), args.get(5)) else {
                return bad_args(s, text);
            };
            let mut st = s.borrow_mut();
            let (Some(a_id), Some(b_id)) = (st.faction_id(&a), st.faction_id(&b)) else {
                st.log.push(format!("{text}: unknown faction"));
                return Ok(Value::Nil);
            };
            let Some(index) = ntw_sim::campaign::details::diplomacy_option_index(&option) else {
                st.log.push(format!("{text}: unknown option"));
                return Ok(Value::Nil);
            };
            let stored = st.model.set_diplomacy_option(a_id, b_id, index, *offer, *accept);
            st.log.push(if stored { text } else { format!("{text}: refused by the model (same faction, or no relationship between them)") });
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
        // default when none is left (`0x009797F0`): a new campaign's LoadingGame reads its start
        // position's items, and outside LoadingGame nothing is queued (`ScriptHost::load_values`).
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
            // The model's list rule (`0x009C9700` / `0x00A1D3B0`): an add appends, a remove erases the first match.
            let mut st = s.borrow_mut();
            let known = if name.starts_with("add") { st.model.add_restricted_unit(&key) } else { st.model.remove_restricted_unit(&key) };
            st.log.push(if known { text } else { format!("{text}: '{key}' is not a valid unit key, ignored") });
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

        // disable_shopping_for_ai_under_shroud(on): CONFIRMED (binding `0x00979910`, one boolean read
        // by `0x01055840`, then `0x00AA9B40` on the CAI interface): the one human's shroud holds the
        // AI's shopping back where it hides (ntw_sim `CampaignModel::set_ai_shopping_shroud`). Only a
        // data.pack feature-list script calls it (4 calls, next to `unveil_black_shroud`).
        // ORIGINAL BUG: `0x00979910` does not clear its result before the read either (see
        // `show_shroud`), so a non-boolean passes an arbitrary value; ours false.
        "disable_shopping_for_ai_under_shroud" => {
            let on = top_bool(s, name, args).unwrap_or(false);
            let mut st = s.borrow_mut();
            st.model.set_ai_shopping_shroud(on);
            st.log.push(text);
        }

        // disable_saving_game(on): CONFIRMED (`0x0097B840`, one boolean read with default false,
        // stored at campaign +0x115): while set, the save refuses (`0x00A1F000`) and
        // `CampaignUI.CanSave` answers false. Tutorial only.
        "disable_saving_game" => {
            // `0x0097B840` clears its slot first, so a non-boolean argument gives false.
            let on = top_bool(s, name, args).unwrap_or(false);
            let mut st = s.borrow_mut();
            st.model.world.saving_disabled = on;
            st.log.push(text);
        }

        // display_turns(on): CONFIRMED (`0x00979600`, one boolean stored at episodic restrictions
        // +0xD1): the HUD shows the turn number instead of the date (World::display_turns). No
        // vanilla script calls it. ORIGINAL BUG: `0x00979600` does not clear its result before the
        // boolean read (`0x01055840` writes the slot only for a real Lua boolean, and the slot is the
        // lua_State argument's), so a non-boolean (number, nil, no argument) stores that pointer's low
        // byte, an arbitrary value. Ours stores false, as `disable_saving_game` does.
        "display_turns" => {
            let on = top_bool(s, name, args).unwrap_or(false);
            let mut st = s.borrow_mut();
            st.model.world.display_turns = on;
            st.log.push(text);
        }

        // The WorldCreated switches: CONFIRMED (handlers `0x0097B900` / `0x0097B940` / `0x0097B980`,
        // each one boolean read with its slot cleared first, so a non-boolean gives false, stored at
        // episodic restrictions +0x118 / +0x124 / +0x125; saved and loaded with the campaign).
        // In the exe, set_technology_research_disabled empties the building screens' unlocked-technology lists
        // (World::technology_research_disabled; ita), set_liberation_options_disabled stops a
        // capture's liberation option (World::liberation_options_disabled; ita, egy, tut, mp_ita,
        // mp_egy) and set_ui_notification_of_victory_disabled skips the campaign-won message and the
        // winners' victory event (World::victory_notification_disabled; tut). Ours stores all three; the field docs say
        // which readers ours has yet (the technology switch: CampaignModel::building_info_technologies).
        "set_technology_research_disabled" | "set_liberation_options_disabled" | "set_ui_notification_of_victory_disabled" => {
            let on = top_bool(s, name, args).unwrap_or(false);
            let mut st = s.borrow_mut();
            let world = &mut st.model.world;
            match name {
                "set_technology_research_disabled" => world.technology_research_disabled = on,
                "set_liberation_options_disabled" => world.liberation_options_disabled = on,
                _ => world.victory_notification_disabled = on,
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

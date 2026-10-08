//! Script host tests with our own tiny in-memory scripts (no install needed).

mod common;

use ntw_script::{ScriptContext, ScriptHost, ScriptSource, ScriptValue};

/// Our own minimal stand-in for the shipped `events.lua` (same `module` mechanism, 5 events).
const EVENTS: &str = r#"
module(..., package.seeall)
NewSession = {}
FactionTurnStart = {}
TimeTrigger = {}
SavingGame = {}
LoadingGame = {}
"#;

/// Our own test campaign script using the same patterns as the shipped ones.
const SCRIPT: &str = r#"
local events = require "data.events"
game_interface = nil
seen = {}
local counter = 0
events.NewSession[#events.NewSession+1] = function(context) game_interface = GAME(context) end
events.FactionTurnStart[#events.FactionTurnStart+1] = function(context)
    if conditions.FactionIsLocal(context) then
        seen[#seen+1] = "local turn " .. conditions.TurnNumber(context)
        game_interface:treasury_mod("france", 250.7)
        game_interface:add_time_trigger("pan", 2.5)
        game_interface:force_diplomacy("france", "austria", "peace", false, false)
        game_interface:show_shroud(false)            -- the fog of war: now implemented
        game_interface:unveil_black_shroud(true)    -- idem
        game_interface:some_future_method(1, "x")    -- not a known name: still a stub
        CampaignUI.SetCameraZoom(0.95)              -- UI stub
        UIComponent(context.component):Find("x"):SetVisible(false)
        out.ting("hello", 1)
    end
end
events.TimeTrigger[#events.TimeTrigger+1] = function(context) seen[#seen+1] = "trigger " .. context.string end
events.SavingGame[#events.SavingGame+1] = function(context)
    game_interface:save_value(true, context)
    game_interface:save_value(counter + 0.1, context)
end
events.LoadingGame[#events.LoadingGame+1] = function(context)
    loaded_a = game_interface:load_value(false, context)
    loaded_b = game_interface:load_value(0, context)
    loaded_c = game_interface:load_value("default", context)
end
"#;

fn host() -> ScriptHost {
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(common::tiny_model(), "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h
}

#[test]
fn events_reach_registered_handlers() {
    let h = host();
    assert_eq!(h.fire("NewSession", ScriptContext::default()).handlers, 1);
    let r = h.fire("FactionTurnStart", ScriptContext::for_faction("france"));
    assert_eq!((r.handlers, r.errors.clone()), (1, vec![]));
    // Another faction's turn: the handler runs but the condition is false.
    assert!(h.fire("FactionTurnStart", ScriptContext::for_faction("austria")).errors.is_empty());
    let seen: Vec<String> = h.lua().load("return seen").eval::<Vec<String>>().unwrap();
    assert_eq!(seen, vec!["local turn 1"]);
    // An event nobody registered for is fine.
    assert_eq!(h.fire("NoSuchEvent", ScriptContext::default()).handlers, 0);
}

#[test]
fn game_interface_changes_the_model() {
    let h = host();
    h.fire("NewSession", ScriptContext::default());
    h.fire("FactionTurnStart", ScriptContext::for_faction("france"));
    let st = h.state();
    let france = st.model.world.factions.values().find(|f| f.key == "france").unwrap();
    // 250.7 → f32 → truncated to 250 (INFERRED C cast).
    assert_eq!(france.treasury, 1250);
    assert_eq!(
        st.diplomacy_options.get(&("france".into(), "austria".into(), "peace".into())),
        Some(&(false, false))
    );
    // The fog of war is implemented (0-G): show_shroud / unveil_black_shroud are no longer stubs,
    // so they are logged as done and leave no UNKNOWN line. The tiny model has no shroud for France,
    // so the call says so instead of doing anything.
    assert!(st.log.iter().any(|l| l.contains("show_shroud")));
    assert!(!st.log.iter().any(|l| l.contains("UNKNOWN stub game_interface:show_shroud")));
    assert!(st.log.iter().any(|l| l.contains("some_future_method")));
    assert!(st.log.iter().any(|l| l == "out.ting: hello 1"));
}

#[test]
fn time_triggers_fire_after_their_delay() {
    let mut h = host();
    h.fire("NewSession", ScriptContext::default());
    h.fire("FactionTurnStart", ScriptContext::for_faction("france"));
    assert!(h.advance_time(2.0).is_empty());
    let reports = h.advance_time(0.5);
    assert_eq!(reports.len(), 1);
    let seen: Vec<String> = h.lua().load("return seen").eval().unwrap();
    assert_eq!(seen.last().unwrap(), "trigger pan");
    assert!(h.state().time_triggers.is_empty());
}

#[test]
fn save_and_load_values_are_positional_and_f32() {
    let mut h = host();
    h.fire("NewSession", ScriptContext::default());
    let (values, report) = h.save_values();
    assert!(report.errors.is_empty());
    // 0.1 crosses the boundary as an f32.
    assert_eq!(values, vec![ScriptValue::Bool(true), ScriptValue::Number(0.1f32)]);
    let mut h2 = host();
    h2.fire("NewSession", ScriptContext::default());
    assert!(h2.load_values(values).errors.is_empty());
    let (a, b, c): (bool, f64, String) = h2.lua().load("return loaded_a, loaded_b, loaded_c").eval().unwrap();
    // Two stored values, then the default.
    assert_eq!((a, b, c.as_str()), (true, f64::from(0.1f32), "default"));
    assert!(!h2.state().is_new_game);
}

#[test]
fn handler_errors_are_reported_and_others_still_run() {
    let h = host();
    h.exec(
        r#"local e = require "data.events"
           e.NewSession[#e.NewSession+1] = function() error("boom") end
           e.NewSession[#e.NewSession+1] = function() after_error = true end"#,
    )
    .unwrap();
    let r = h.fire("NewSession", ScriptContext::default());
    assert_eq!(r.handlers, 3);
    assert_eq!(r.errors.len(), 1);
    assert!(r.errors[0].contains("boom"));
    assert!(h.lua().load("return after_error").eval::<bool>().unwrap());
}

#[test]
fn unknown_conditions_are_safe_stubs() {
    let h = host();
    let (a, b): (bool, f64) = h
        .lua()
        .load(r#"return conditions.CharacterHasAncillary("x", nil), conditions.CharacterTrait("y", nil)"#)
        .eval()
        .unwrap();
    assert_eq!((a, b), (false, 0.0));
    h.exec(r#"assert(bit.band(12, 10) == 8 and bit.bor(1, 2) == 3 and bit.lshift(1, 4) == 16)"#).unwrap();
}

#[test]
fn end_turn_forwards_model_events() {
    let mut h = host();
    h.fire("NewSession", ScriptContext::default());
    let fired = h.end_turn();
    let names: Vec<&str> = fired.iter().filter_map(|(e, _)| e.script_name()).collect();
    assert!(names.contains(&"FactionTurnStart"));
    assert!(fired.iter().all(|(_, r)| r.errors.is_empty()));
    let seen: Vec<String> = h.lua().load("return seen").eval().unwrap();
    assert!(seen.iter().any(|s| s.starts_with("local turn")), "{seen:?}");
}

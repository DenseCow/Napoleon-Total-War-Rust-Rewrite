//! The `HistoricalEvents` script event (CAMPAIGN_FIDELITY.md "Historical events"): the round start
//! fires it through the host between its two steps, and the handlers' `IsTriggerableHistoricalEvent` /
//! `historical_event` calls decide which events the round start posts. Our own in-memory scripts and
//! MADE-UP events (no install needed).

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use ntw_script::{ScriptHost, ScriptSource};
use ntw_sim::campaign::events::{CampaignEvent, CampaignMessage};
use ntw_sim::campaign::historical_events::{EventGrouping, HistoricalEventRule, HistoricalEventRules, event_date};
use ntw_sim::campaign::FactionId;

/// Our own stand-in for the shipped `events.lua`.
const EVENTS: &str = r#"
module(..., package.seeall)
HistoricalEvents = {}
FactionRoundStart = {}
"#;

/// The vanilla handlers' shape (`export_historic_events.lua`, generated from `events`): one per event,
/// posting it when the condition passes. MADE-UP dates: `a` on the test date, `b` a turn later.
const VANILLA: &str = r#"
local events = require "data.events"
events.HistoricalEvents[#events.HistoricalEvents+1] = function(context)
    if conditions.IsTriggerableHistoricalEvent(11, "a", 1806, "season_summer", context) then
        effect.historical_event("a", context)
        return true
    end
    return false
end
events.HistoricalEvents[#events.HistoricalEvents+1] = function(context)
    if conditions.IsTriggerableHistoricalEvent(12, "b", 1806, "season_summer", context) then
        effect.historical_event("b", context)
        return true
    end
    return false
end
"#;

const FRANCE: FactionId = FactionId(100);

/// A host on the tiny model, France human, its calendar on turn 11 of 24 in 1806, two MADE-UP events
/// every faction views, and `script` loaded.
fn host(script: &str) -> ScriptHost {
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", script);
    let mut model = common::tiny_model();
    model.turn.humans = vec![FRANCE];
    model.calendar.turns_per_year = 24;
    model.calendar.date = event_date(1806, 11, 24).expect("24 turns");
    let factions: Vec<String> = model.world.factions.values().map(|f| f.key.clone()).collect();
    let event = |key: &str, once: bool| HistoricalEventRule {
        key: key.into(),
        once,
        view_groups: vec!["all".into()],
        posts_message: true,
        ..Default::default()
    };
    Arc::make_mut(&mut model.rules).historical_events = HistoricalEventRules {
        events: vec![event("a", true), event("b", false)],
        groupings: BTreeMap::from([("all".to_owned(), EventGrouping { factions, ..Default::default() })]),
        region_continents: BTreeMap::new(),
    };
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h
}

/// The historical-event messages of a campaign start (its round start), with the fired events' names.
fn start(h: &mut ScriptHost) -> (Vec<(FactionId, String)>, Vec<&'static str>) {
    let fired = h.start_campaign();
    let names = fired.iter().filter_map(|(e, _)| e.script_name()).collect();
    let messages = fired
        .into_iter()
        .filter_map(|(e, _)| match e {
            CampaignEvent::Message { recipient, message: CampaignMessage::HistoricalEvent { event } } => Some((recipient, event)),
            _ => None,
        })
        .collect();
    (messages, names)
}

/// `HistoricalEvents` fires after the listed factions' `FactionRoundStart`s; the vanilla handlers push
/// the event of the date only, and a once-only event is marked by its condition.
#[test]
fn the_vanilla_handlers_pick_the_event_of_the_date() {
    let mut h = host(VANILLA);
    let (messages, names) = start(&mut h);
    assert_eq!(messages, [(FRANCE, "a".to_owned())]);
    let at = names.iter().position(|n| *n == "HistoricalEvents").expect("the event fires");
    assert!(names[..at].iter().all(|n| *n == "FactionRoundStart") && at > 0, "{names:?}");
    let m = h.model();
    assert_eq!(m.historical_candidates, None, "the candidates last one round start");
    assert!(m.world.historical_events_triggered.iter().any(|k| k == "a"), "a once-only event is marked");
}

/// A mod's handler decides as the exe lets it: it may post an event without the condition (`b` is not
/// on the date) or skip the vanilla ones; a key with no event adds nothing.
#[test]
fn a_mod_handler_posts_what_it_pushes() {
    let mut h = host(
        r#"
local events = require "data.events"
events.HistoricalEvents[#events.HistoricalEvents+1] = function(context)
    effect.historical_event("no_such_event", context)
    effect.historical_event("b", context)
end
"#,
    );
    let (messages, _) = start(&mut h);
    assert_eq!(messages, [(FRANCE, "b".to_owned())]);
    assert!(h.model().world.historical_events_triggered.is_empty(), "no condition ran");
}

/// Outside `HistoricalEvents` the context carries no candidates: the condition is false, the effect
/// adds nothing and nothing is marked; with no `HistoricalEvents` handler no event is posted.
#[test]
fn outside_the_event_the_calls_do_nothing() {
    let mut h = host(
        r#"
local events = require "data.events"
answers = {}
events.FactionRoundStart[#events.FactionRoundStart+1] = function(context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(11, "a", 1806, "season_summer", context)
    effect.historical_event("a", context)
end
"#,
    );
    let (messages, names) = start(&mut h);
    assert!(names.contains(&"HistoricalEvents"));
    assert!(messages.is_empty());
    let answers: Vec<bool> = h.lua().load("return answers").eval().unwrap();
    assert!(!answers.is_empty() && answers.iter().all(|a| !a), "{answers:?}");
    assert!(h.model().world.historical_events_triggered.is_empty());
}

/// The condition's integers are read as the exe reads them (`0x010556D0`: rounded ties to even), and a
/// wrong-typed argument answers false with a log line.
#[test]
fn the_condition_reads_its_arguments_as_the_exe() {
    let mut h = host(
        r#"
local events = require "data.events"
answers = {}
events.HistoricalEvents[#events.HistoricalEvents+1] = function(context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent("11", "a", 1806, "season_summer", context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(10.5, "b", 1806.4, "season_summer", context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(10.6, "b", 1806, "season_summer", context)
end
"#,
    );
    start(&mut h);
    let answers: Vec<bool> = h.lua().load("return answers").eval().unwrap();
    // "11" is no number; 10.5 rounds to 10 (not b's date, 11); 10.6 rounds to 11.
    assert_eq!(answers, [false, false, true]);
    assert!(h.state().log.iter().any(|l| l.contains("IsTriggerableHistoricalEvent") && l.contains("Trying to retrieve type")));
}

/// `0x008A1DC0` reads the season first (`0x01055760`, `lua_isstring`: a string or a number); a season
/// of another type fails the reader's sticky check, so nothing after it is read and the call answers
/// false without marking the once-only event; the type error is logged once per type found.
#[test]
fn a_season_of_the_wrong_type_fails_the_whole_call() {
    let mut h = host(
        r#"
local events = require "data.events"
answers = {}
events.HistoricalEvents[#events.HistoricalEvents+1] = function(context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(11, "a", 1806, nil, context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(11, "a", 1806, true, context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(11, "a", 1806, {}, context)
    answers[#answers+1] = conditions.IsTriggerableHistoricalEvent(11, "a", 1806, 7, context)
end
"#,
    );
    start(&mut h);
    let answers: Vec<bool> = h.lua().load("return answers").eval().unwrap();
    // The failed calls mark nothing, so the number season (read as a string) still finds `a` unmarked.
    assert_eq!(answers, [false, false, false, true]);
    let logged = h.state().log.iter().filter(|l| l.contains("IsTriggerableHistoricalEvent")).count();
    assert_eq!(logged, 3, "logged once per type found (nil, boolean, table)");
}

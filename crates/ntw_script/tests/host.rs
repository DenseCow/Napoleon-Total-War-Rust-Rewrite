//! Script host tests with our own tiny in-memory scripts (no install needed).

mod common;

use ntw_campaign::script_values::ScriptSaveValue;
use ntw_script::{CampaignOpening, ScriptContext, ScriptHost, ScriptSource, ScriptValue};

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
        game_interface:treasury_mod("france", -100)  -- ignored: only an amount above 0 is credited
        game_interface:add_time_trigger("pan", 2.5)
        game_interface:force_diplomacy("france", "austria", "peace", false, false)
        game_interface:force_diplomacy("france", "austria", "war", true, false)
        game_interface:force_diplomacy("france", "france", "war", false, false) -- same faction: refused
        game_interface:force_diplomacy("france", "prussia", "war", false, false) -- no relationship: refused
        game_interface:force_diplomacy("france", "austria", "Peace", false, false) -- exact keys only
        game_interface:force_diplomacy("france", "austria", "war", "no", false) -- not a boolean
        game_interface:show_shroud(false)            -- the fog of war: now implemented
        game_interface:unveil_black_shroud("shroud_map_1_1") -- idem: no outline on this map
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
    // Relationships between france and austria only (the model never creates one for a script).
    let mut model = common::tiny_model();
    let (fr, au) = (model.faction_by_key("france").unwrap().id, model.faction_by_key("austria").unwrap().id);
    model.relationship_mut(fr, au);
    model.relationship_mut(au, fr);
    let h = ScriptHost::new(model, "france", src).unwrap();
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
    // 250.7 → f32 → rounded to 251 (FISTP, 0x0097BAD0); the -100 is ignored (CONFIRMED, amount > 0 only).
    assert_eq!(france.treasury, 1251);
    // force_diplomacy lands in the model (0x009792D0): a's relationship to b only, value
    // !accept + 2·!offer; the panel and the deal rules read it from there.
    let (fr, au) = (st.faction_id("france").unwrap(), st.faction_id("austria").unwrap());
    use ntw_sim::campaign::negotiation::NegotiationAction as A;
    assert_eq!(st.model.diplomacy_option(fr, au, A::Peace.option()), 3);
    assert_eq!(st.model.diplomacy_option(fr, au, A::War.option()), 1);
    assert_eq!(st.model.diplomacy_option(au, fr, A::Peace.option()), 0, "one direction only");
    assert!(!st.model.world.relationships.contains_key(&(fr, fr)), "same faction refused");
    let pr = st.faction_id("prussia").unwrap();
    assert!(!st.model.world.relationships.contains_key(&(fr, pr)), "no relationship: refused, none created");
    let actions = st.model.negotiation_actions(fr, au);
    let forbidden = |x: A| actions.iter().find(|r| r.action == x).unwrap().forbidden;
    assert!(forbidden(A::Peace), "offer bit off: france may not propose peace");
    assert!(!forbidden(A::War), "offer bit on: france may propose war");
    assert!(st.model.may_propose(au, fr, A::War.option()), "austria's own side is untouched");
    assert!(!st.model.deal_goal_allowed(au, fr, A::War.option()), "france declines war from austria");
    assert!(st.log.iter().any(|l| l.contains("\"france\", \"france\"") && l.contains("refused")));
    assert!(st.log.iter().any(|l| l.contains("\"Peace\"") && l.contains("unknown option")));
    assert!(st.log.iter().any(|l| l.contains("\"no\"") && l.contains("unexpected arguments")));
    // The fog of war is implemented (0-G): show_shroud / unveil_black_shroud are no longer stubs,
    // so they are logged as done and leave no UNKNOWN line. The tiny model has no black shroud
    // outline, so unveil_black_shroud says so instead of doing anything.
    assert!(st.log.iter().any(|l| l.contains("no black shroud outline \"shroud_map_1_1\"")));
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
    // Unread values do not outlive LoadingGame, and LoadingGame alone does not mark a load.
    assert!(h2.state().values_to_load.is_empty());
    assert!(h2.state().is_new_game);
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
fn war_started_character_faction_reads_the_faction_flag() {
    // 0x008A66C0: the context character's faction +0x925 (`FactionDetails::war_started`); false
    // without a character.
    use ntw_sim::campaign::{Character, CharacterId, CharacterKind};
    use ntw_sim::fixed::Fixed20;
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    let c = CharacterId(7);
    model.world.characters.insert(
        c,
        Character {
            id: c,
            faction: fr,
            kind: CharacterKind::General,
            position: (Fixed20::from_int(0), Fixed20::from_int(0)),
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: None,
        },
    );
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.exec(
        r#"local e = require "data.events"
           e.CharacterTurnEnd = {}
           e.CharacterTurnEnd[1] = function(context) war_started[#war_started+1] = conditions.WarStartedCharacterFaction(context) end
           war_started = {}"#,
    )
    .unwrap();
    let ctx = ScriptContext { character: Some(c.0), ..Default::default() };
    assert!(h.fire("CharacterTurnEnd", ctx.clone()).errors.is_empty());
    h.state_mut().model.world.faction_details.entry(fr).or_default().war_started = true;
    h.fire("CharacterTurnEnd", ctx);
    h.fire("CharacterTurnEnd", ScriptContext::default());
    let seen: Vec<bool> = h.lua().load("return war_started").eval().unwrap();
    assert_eq!(seen, vec![false, true, false]);
}

#[test]
fn grant_faction_handover_destroys_the_target_and_sets_the_killers_flag() {
    // Binding 0x00978950: the turns must be Lua numbers (else nothing happens), the keys known
    // factions; the model's rule decides (window, draw, `0x008AB6C0` with a killer). The killer's
    // +0x924 then reads through FactionDestroyedByCharacterFaction (0x0089F4B0).
    use ntw_sim::campaign::{Character, CharacterId, CharacterKind};
    use ntw_sim::fixed::Fixed20;
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    let au = model.faction_by_key("austria").unwrap().id;
    let c = CharacterId(7);
    model.world.characters.insert(
        c,
        Character {
            id: c,
            faction: fr,
            kind: CharacterKind::General,
            position: (Fixed20::from_int(0), Fixed20::from_int(0)),
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: None,
        },
    );
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.exec(
        r#"local e = require "data.events"
           e.CharacterTurnEnd = {}
           e.CharacterTurnEnd[1] = function(context) destroyed[#destroyed+1] = conditions.FactionDestroyedByCharacterFaction(context) end
           destroyed = {}"#,
    )
    .unwrap();
    let ctx = ScriptContext { character: Some(c.0), ..Default::default() };
    let out = |h: &ScriptHost| h.model().world.faction_details.get(&au).and_then(|d| d.out_of_game);
    h.fire("CharacterTurnEnd", ctx.clone());
    for call in [
        r#"game_interface:grant_faction_handover("france", "austria", "0", 0)"#, // not a number
        r#"game_interface:grant_faction_handover("france", "nowhere", 0, 0)"#,
        r#"game_interface:grant_faction_handover("france", "austria", 3, 9)"#, // turn 0: outside
    ] {
        h.exec(call).unwrap();
        assert_eq!(out(&h), Some(false), "{call}");
    }
    h.exec(r#"game_interface:grant_faction_handover("france", "austria", 0, 0.4)"#).unwrap(); // 0.4 → 0: certain
    assert_eq!(out(&h), Some(true));
    h.fire("CharacterTurnEnd", ctx);
    let seen: Vec<bool> = h.lua().load("return destroyed").eval().unwrap();
    assert_eq!(seen, vec![false, true]);
    let st = h.state_mut();
    assert!(st.log.iter().any(|l| l.contains("\"0\"") && l.contains("unexpected arguments")), "{:?}", st.log);
    assert!(st.log.iter().any(|l| l.contains("\"nowhere\"") && l.contains("unknown faction")));
    assert!(st.log.iter().any(|l| l.ends_with("austria destroyed by france")));
}

#[test]
fn a_handover_from_a_handler_returns_its_destruction_events() {
    // The destruction's model events (here a human target's `FactionDestroyed`) were dropped on the
    // script command path; the host now fires/returns them after the handler's batch.
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    let au = model.faction_by_key("austria").unwrap().id;
    model.turn.humans = vec![fr, au];
    let mut h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.exec(
        r#"local e = require "data.events"
           handed = false
           e.FactionTurnStart[#e.FactionTurnStart+1] = function(context)
               if not handed then
                   handed = true
                   local t = conditions.TurnNumber(context) - 1
                   game_interface:grant_faction_handover("france", "austria", t, t)
               end
           end"#,
    )
    .unwrap();
    let fired = h.end_turn();
    let start = fired.iter().position(|(e, _)| e.script_name() == Some("FactionTurnStart")).expect("a turn started");
    let destroyed = fired
        .iter()
        .position(|(e, _)| matches!(e, ntw_sim::campaign::CampaignEvent::FactionDestroyed { faction } if *faction == au))
        .expect("the handover's FactionDestroyed is returned");
    assert!(start < destroyed, "after the batch whose handler issued it");
    assert_eq!(h.model().world.faction_details.get(&au).and_then(|d| d.out_of_game), Some(true));
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

#[test]
fn force_make_protectorate_makes_peace_then_the_protectorate() {
    // 0x0097AA20: at war, a peace first (messages off), then the protectorate, then the armies out.
    use ntw_sim::campaign::{CampaignCommand, Stance};
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let id = |m: &ntw_sim::campaign::CampaignModel, k: &str| m.faction_by_key(k).unwrap().id;
    let (fr, au, pr) = (id(&model, "france"), id(&model, "austria"), id(&model, "prussia"));
    model.apply(CampaignCommand::DeclareWar { a: fr, b: au }).unwrap();
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.lua().load(r#"game_interface:force_make_protectorate("france", "austria")"#).exec().unwrap();
    assert_eq!((h.model().world.stance(au, fr), h.model().world.stance(fr, au)), (Stance::Protectorate, Stance::Patron));
    // Austria has a patron: no second protectorate (the armies still move out); the same faction or
    // an unknown one does nothing.
    h.lua()
        .load(
            r#"game_interface:force_make_protectorate("prussia", "austria")
               game_interface:force_make_protectorate("france", "france")
               game_interface:force_make_protectorate("france", "nobody")"#,
        )
        .exec()
        .unwrap();
    assert_eq!(h.model().world.stance(au, pr), Stance::Neutral);
    let st = h.state();
    let log: Vec<&String> = st.log.iter().filter(|l| l.contains("force_make_protectorate")).collect();
    assert_eq!(log.len(), 4, "{log:?}");
    assert!(!log[0].contains("refused") && !log[1].contains("refused"), "{log:?}");
    assert!(log[2].contains("same faction") && log[3].contains("unknown faction"), "{log:?}");
}

#[test]
fn force_make_protectorate_outside_the_patrons_turn_returns_its_events() {
    // 0x0097AA20 from a handler in another faction's turn: the peace (`CCQ_MAKE_PEACE` handler
    // 0x00936710 has no turn test) and the protectorate both hold, and their stance events reach the
    // host after the handler's batch (they were dropped). The peace's messages are off.
    use ntw_sim::campaign::{CampaignCommand, CampaignEvent, Stance};
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let (fr, au) = (model.faction_by_key("france").unwrap().id, model.faction_by_key("austria").unwrap().id);
    model.apply(CampaignCommand::DeclareWar { a: fr, b: au }).unwrap();
    model.turn.humans = vec![fr, au];
    let mut h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.exec(
        r#"local e = require "data.events"
           starts = 0
           e.FactionTurnStart[#e.FactionTurnStart+1] = function(context)
               starts = starts + 1
               if starts == 2 then -- Austria's turn, after France's
                   game_interface:force_make_protectorate("france", "austria")
               end
           end"#,
    )
    .unwrap();
    let fired = h.end_turn();
    let start = fired.iter().position(|(e, _)| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == au)).expect("Austria's turn started");
    let stance_event = |stance: Stance| {
        fired.iter().position(|(e, _)| matches!(e, CampaignEvent::StanceChanged { a, b, stance: s } if *s == stance && [*a, *b].contains(&fr) && [*a, *b].contains(&au)))
    };
    let peace = stance_event(Stance::Neutral).expect("the peace's event is returned");
    let protectorate = stance_event(Stance::Protectorate).expect("the protectorate's event is returned");
    assert!(start < peace && peace < protectorate, "{start} {peace} {protectorate}");
    assert!(!fired.iter().any(|(e, _)| matches!(e, CampaignEvent::Message { .. })), "the peace posts no ceasefire messages (suppress byte 1)");
    assert_eq!((h.model().world.stance(au, fr), h.model().world.stance(fr, au)), (Stance::Protectorate, Stance::Patron));
    let st = h.state();
    assert!(st.log.iter().any(|l| l.contains("force_make_protectorate") && !l.contains("refused")), "{:?}", st.log);
}

#[test]
fn force_make_peace_outside_the_first_factions_turn_makes_peace() {
    // 0x0097AC90 queues CCQ_MAKE_PEACE, whose handler 0x00936710 has no turn test: France's peace with
    // Austria holds when a handler in Austria's turn calls it.
    use ntw_sim::campaign::{CampaignCommand, CampaignEvent, Stance};
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let (fr, au) = (model.faction_by_key("france").unwrap().id, model.faction_by_key("austria").unwrap().id);
    model.apply(CampaignCommand::DeclareWar { a: fr, b: au }).unwrap();
    model.turn.humans = vec![fr, au];
    let mut h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.exec(
        r#"local e = require "data.events"
           starts = 0
           e.FactionTurnStart[#e.FactionTurnStart+1] = function(context)
               starts = starts + 1
               if starts == 2 then -- Austria's turn, after France's
                   game_interface:force_make_peace("france", "austria")
               end
           end"#,
    )
    .unwrap();
    let fired = h.end_turn();
    assert!(fired.iter().any(|(e, _)| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == au)), "Austria's turn started");
    assert_eq!(h.model().world.stance(fr, au), Stance::Neutral);
    let st = h.state();
    assert!(st.log.iter().any(|l| l.contains("force_make_peace") && !l.contains("refused")), "{:?}", st.log);
}

#[test]
fn force_make_protectorate_with_a_battle_pending_changes_nothing() {
    // The three commands go through `CampaignModel::apply`, so a pending battle refuses each of them:
    // no peace without the protectorate and the expulsion.
    use ntw_sim::campaign::{CampaignCommand, ForceId, PendingBattle};
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let (fr, au) = (model.faction_by_key("france").unwrap().id, model.faction_by_key("austria").unwrap().id);
    model.apply(CampaignCommand::DeclareWar { a: fr, b: au }).unwrap();
    model.pending_battle = Some(PendingBattle { attacker: ForceId(1), defenders: vec![ForceId(2)], settlement: None, resume: None });
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    let before = h.model().world.clone();
    h.lua().load(r#"game_interface:force_make_protectorate("france", "austria")"#).exec().unwrap();
    assert!(h.model().world == before, "the world changed");
    let st = h.state();
    let line = st.log.iter().find(|l| l.contains("force_make_protectorate")).expect("logged");
    assert_eq!(line.matches("a battle is pending").count(), 3, "{line}");
}

#[test]
fn turn_steps_return_every_model_event_in_order() {
    use ntw_sim::campaign::{CampaignEvent, CharacterId, FactionId, ForceId};
    // Project events (no script name: the exe declares no `CharacterMoved` / `ForceDestroyed` script
    // event) from an AI turn reach the caller with the script events, in the model's order, as a
    // command's do: the app animates the moves and shows the outcome movies from them.
    let mut h = host();
    let (fr, au) = (FactionId(100), FactionId(101));
    h.state_mut().model.turn.humans = vec![fr];
    h.set_ai_turn_hook(Box::new(move |_, faction| {
        if faction != au {
            return Vec::new();
        }
        vec![CampaignEvent::CharacterMoved { character: CharacterId(7), path: vec![(0.0, 0.0), (1.0, 1.0)] }, CampaignEvent::ForceDestroyed { force: ForceId(8) }]
    }));
    let fired: Vec<CampaignEvent> = h.end_turn().into_iter().map(|(e, _)| e).collect();
    let at = |want: &dyn Fn(&CampaignEvent) -> bool| fired.iter().position(want);
    let start = at(&|e| *e == CampaignEvent::FactionTurnStart { faction: au }).expect("austria's turn start");
    let moved = at(&|e| matches!(e, CampaignEvent::CharacterMoved { character: CharacterId(7), .. })).expect("the AI's move returned");
    let gone = at(&|e| *e == CampaignEvent::ForceDestroyed { force: ForceId(8) }).expect("the AI's destroyed force returned");
    let end = at(&|e| *e == CampaignEvent::FactionTurnEnd { faction: au }).expect("austria's turn end");
    assert!(start < moved && moved < gone && gone < end, "{start} {moved} {gone} {end}");
}

#[test]
fn disable_shopping_for_ai_under_shroud_stores_the_one_humans_shroud() {
    // 0x00AA9B40: on → the single human's shroud (france here), off → none.
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    model.turn.humans = vec![fr];
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.lua().load("game_interface:disable_shopping_for_ai_under_shroud(true)").exec().unwrap();
    assert_eq!(h.model().world.ai_shopping_shroud, Some(fr));
    h.lua().load("game_interface:disable_shopping_for_ai_under_shroud(false)").exec().unwrap();
    assert_eq!(h.model().world.ai_shopping_shroud, None);
}

#[test]
fn disable_saving_game_sets_the_saving_switch() {
    // 0x0097B840: the boolean goes to campaign +0x115 (World::saving_disabled); no argument = false.
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(common::tiny_model(), "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    assert!(!h.model().world.saving_disabled);
    h.lua().load("game_interface:disable_saving_game(true)").exec().unwrap();
    assert!(h.model().world.saving_disabled);
    h.lua().load("game_interface:disable_saving_game()").exec().unwrap();
    assert!(!h.model().world.saving_disabled);
}

#[test]
fn display_turns_sets_the_turns_display_switch() {
    // 0x00979600: the boolean goes to episodic restrictions +0xD1 (World::display_turns); a call
    // without one stores false (ORIGINAL BUG fixed: the exe stores a leftover byte).
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(common::tiny_model(), "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    assert!(!h.model().world.display_turns);
    h.lua().load("game_interface:display_turns(true)").exec().unwrap();
    assert!(h.model().world.display_turns);
    h.lua().load("game_interface:display_turns()").exec().unwrap();
    assert!(!h.model().world.display_turns);
}

#[test]
fn restricted_unit_records_are_the_models_list() {
    // 0x009C9700 appends (a unit added twice is listed twice), 0x00A1D3B0 erases the first match only, and
    // a key the units table lacks is logged and stored nowhere (ORIGINAL BUG fixed: the exe stores a null
    // record its next save crashes on, 0x009A1070).
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    model.rules = std::sync::Arc::new(ntw_sim::campaign::rules::CampaignRules::test_rules());
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    let listed = |h: &ScriptHost| h.model().world.restricted_units.clone();
    h.lua().load(r#"game_interface:add_restricted_unit_record("test_recruit") game_interface:add_restricted_unit_record("made_up_none") game_interface:add_restricted_unit_record("test_recruit")"#).exec().unwrap();
    assert_eq!(listed(&h), ["test_recruit", "test_recruit"]);
    h.lua().load(r#"game_interface:remove_restricted_unit_record("test_recruit")"#).exec().unwrap();
    assert_eq!(listed(&h), ["test_recruit"], "added twice, removed once: still restricted");
    assert!(h.model().world.unit_restricted("test_recruit"));
    h.lua().load(r#"game_interface:remove_restricted_unit_record("test_recruit") game_interface:remove_restricted_unit_record("test_recruit")"#).exec().unwrap();
    assert!(listed(&h).is_empty(), "removing an unlisted unit does nothing");
    assert!(h.state().log.iter().any(|l| l.contains("'made_up_none' is not a valid unit key, ignored")));
}

#[test]
fn saving_and_turns_switches_read_only_real_booleans() {
    // 0x01055840 writes the argument slot only for a Lua boolean: a number is not read, so both
    // switches end false (0x0097B840 clears its slot first; 0x00979600's leftover is our false).
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(common::tiny_model(), "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.lua().load("game_interface:disable_saving_game(1) game_interface:display_turns(1)").exec().unwrap();
    assert!(!h.model().world.saving_disabled);
    assert!(!h.model().world.display_turns);
    h.lua().load("game_interface:disable_saving_game(true) game_interface:display_turns(true)").exec().unwrap();
    assert!(h.model().world.saving_disabled);
    assert!(h.model().world.display_turns);
    h.lua().load("game_interface:disable_saving_game(0) game_interface:display_turns(0)").exec().unwrap();
    assert!(!h.model().world.saving_disabled);
    assert!(!h.model().world.display_turns);
}

#[test]
fn the_world_created_switches_set_the_world_flags() {
    // 0x0097B900 / 0x0097B940 / 0x0097B980: one boolean each, stored at episodic restrictions
    // +0x118 / +0x124 / +0x125; each clears its slot first, so a non-boolean stores false.
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let h = ScriptHost::new(common::tiny_model(), "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    let flags = |h: &ScriptHost| {
        let m = h.model();
        (m.world.technology_research_disabled, m.world.liberation_options_disabled, m.world.victory_notification_disabled)
    };
    assert_eq!(flags(&h), (false, false, false));
    h.lua().load("game_interface:set_liberation_options_disabled(true)").exec().unwrap();
    assert_eq!(flags(&h), (false, true, false));
    h.lua()
        .load("game_interface:set_technology_research_disabled(true) game_interface:set_ui_notification_of_victory_disabled(true)")
        .exec()
        .unwrap();
    assert_eq!(flags(&h), (true, true, true));
    h.lua()
        .load(
            "game_interface:set_technology_research_disabled(1) game_interface:set_liberation_options_disabled() \
             game_interface:set_ui_notification_of_victory_disabled(\"true\")",
        )
        .exec()
        .unwrap();
    assert_eq!(flags(&h), (false, false, false));
}

#[test]
fn shroud_switches_read_the_last_argument_and_only_its_own_type() {
    // 0x01055840 / 0x01055760 read the top of the Lua stack (the last argument) and pop it; a
    // wrong type is the script error "Trying to retrieve type %s, found type %s" (0x01095090),
    // logged once per method and type. show_shroud (0x00979260) and
    // disable_shopping_for_ai_under_shroud (0x00979910) leave their slot uncleared on that error
    // (ORIGINAL BUG: an arbitrary byte); ours stores false.
    use ntw_sim::campaign::visibility::CellSet;
    use ntw_sim::campaign::Shroud;
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    model.turn.humans = vec![fr];
    let empty = CellSet::new(8, 8);
    model.world.shrouds.insert(fr, Shroud { explored: empty.clone(), visible: empty.clone(), hidden: empty, active: true });
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    let active = || h.model().world.shrouds[&fr].active;
    let errors = |method: &str| h.state().log.iter().filter(|l| l.starts_with(&format!("ERROR game_interface:{method}:"))).cloned().collect::<Vec<_>>();

    h.lua().load("game_interface:show_shroud(1)").exec().unwrap();
    assert!(!active(), "a number is not a boolean");
    h.lua().load("game_interface:show_shroud(true)").exec().unwrap();
    assert!(active());
    h.lua().load("game_interface:show_shroud(1) game_interface:show_shroud(true) game_interface:show_shroud(2.5)").exec().unwrap();
    assert!(!active());
    assert_eq!(errors("show_shroud"), vec!["ERROR game_interface:show_shroud: Trying to retrieve type boolean, found type number (logged once)".to_owned()]);
    h.lua().load("game_interface:show_shroud(true, false)").exec().unwrap();
    assert!(!active(), "the last argument is read");
    h.lua().load("game_interface:show_shroud(false, true)").exec().unwrap();
    assert!(active());
    h.lua().load("game_interface.show_shroud(false)").exec().unwrap();
    assert!(!active(), "the object is never read: a dot call reads its one argument");
    h.lua().load("game_interface:show_shroud(true) game_interface:show_shroud()").exec().unwrap();
    assert!(!active(), "no argument: the top is the object itself");
    assert!(errors("show_shroud")[1].ends_with("found type table (logged once)"), "{:?}", errors("show_shroud"));

    h.lua().load("game_interface:disable_shopping_for_ai_under_shroud(true)").exec().unwrap();
    assert_eq!(h.model().world.ai_shopping_shroud, Some(fr));
    h.lua().load("game_interface:disable_shopping_for_ai_under_shroud(\"yes\")").exec().unwrap();
    assert_eq!(h.model().world.ai_shopping_shroud, None, "a string is not a boolean");
    assert_eq!(errors("disable_shopping_for_ai_under_shroud").len(), 1);

    // unveil_black_shroud (0x00977A90) reads a string; a number is turned into its string, a
    // boolean is the type error and the empty name, which no outline has.
    h.lua().load("game_interface:unveil_black_shroud(true) game_interface:unveil_black_shroud(12)").exec().unwrap();
    assert_eq!(errors("unveil_black_shroud"), vec!["ERROR game_interface:unveil_black_shroud: Trying to retrieve type string, found type boolean (logged once)".to_owned()]);
    assert!(h.state().log.iter().any(|l| l.contains("no black shroud outline \"\"")));
    assert!(h.state().log.iter().any(|l| l.contains("no black shroud outline \"12\"")));
}

#[test]
fn other_income_mod_books_the_running_record() {
    // 0x0097BB70 → 0x00A15B30: a positive amount (FISTP-rounded) adds to category 11; a negative one is an
    // expense (category 18) for an AI faction and is dropped for a human one; an unknown faction does nothing.
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let (fr, au) = (model.faction_by_key("france").unwrap().id, model.faction_by_key("austria").unwrap().id);
    model.turn.humans = vec![fr];
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    let record = |f| *h.model().world.last_economy_record(f).unwrap_or(&[0; 25]);
    let (fr11, au11) = (record(fr)[11], record(au)[11]);
    h.lua()
        .load(
            r#"game_interface:other_income_mod("france", 2500.4)
               game_interface:other_income_mod("france", -300)
               game_interface:other_income_mod("austria", 1000)
               game_interface:other_income_mod("austria", -250.5)
               game_interface:other_income_mod("nobody", 50)"#,
        )
        .exec()
        .unwrap();
    assert_eq!((record(fr)[11], record(fr)[18]), (fr11 + 2500, 0));
    assert_eq!((record(au)[11], record(au)[18]), (au11 + 1000, 250));
    assert!(h.state().log.iter().any(|l| l.contains("\"nobody\"") && l.contains("unknown faction")));
}

/// Our own stand-in for a campaign with the opening's handlers (spa_napoleon's pattern: LoadingGame reads
/// the stored boost and marks the game loaded, `ModelCreated` books the boost again through
/// `other_income_mod`; `is_new_game` is read once the world exists).
const OPENING: &str = r#"
local events = require "data.events"
order = {}
local boost, load_game = 0, false
for _, name in ipairs({"NewSession", "LoadingGame", "ModelCreated", "NewCampaignStarted", "WorldCreated"}) do
    events[name] = events[name] or {}
    events[name][#events[name]+1] = function(context) order[#order+1] = name end
end
events.NewSession[#events.NewSession+1] = function(context) game_interface = GAME(context) end
events.LoadingGame[#events.LoadingGame+1] = function(context)
    boost = game_interface:load_value(0, context)
    load_game = true
end
events.ModelCreated[#events.ModelCreated+1] = function(context)
    if load_game and boost ~= 0 then game_interface:other_income_mod("austria", boost) end
end
events.WorldCreated[#events.WorldCreated+1] = function(context)
    new_at_world = game_interface:is_new_game()
    late_value = game_interface:load_value(-1, context)
end
"#;

fn opening_host() -> (ScriptHost, ntw_sim::campaign::FactionId) {
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("opening.lua", OPENING);
    let mut model = common::tiny_model();
    let au = model.faction_by_key("austria").unwrap().id;
    // A running record, as a loaded save has one (made-up figures).
    let mut record = [0; 25];
    record[11] = 500;
    model.world.economy_history.insert(au, vec![record]);
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/opening.lua").unwrap();
    (h, au)
}

fn opening_order(h: &ScriptHost) -> Vec<String> {
    h.lua().load("return order").eval::<Vec<String>>().unwrap()
}

/// What the WorldCreated handler saw: `is_new_game()`, and `load_value(-1)` outside LoadingGame.
fn seen_at_world_created(h: &ScriptHost) -> (bool, f64) {
    h.lua().load("return new_at_world, late_value").eval::<(bool, f64)>().unwrap()
}

#[test]
fn a_load_fires_model_created_once_after_loading_game_and_its_boost_reaches_the_record() {
    let (mut h, au) = opening_host();
    // A second stored value the handler leaves unread.
    let saved = [ScriptSaveValue::Int(2500), ScriptSaveValue::Int(7)];
    let fired = h.open_campaign(ScriptContext::for_faction("france"), CampaignOpening::LoadedSave, &saved);
    assert!(fired.iter().all(|(_, r)| r.errors.is_empty()), "{fired:?}");
    let names: Vec<&str> = fired.iter().map(|(n, _)| *n).collect();
    // 0x009968A0 (NewSession, LoadingGame), 0x0087426F (ModelCreated), 0x00988A72 (WorldCreated).
    assert_eq!(names, ["NewSession", "LoadingGame", "ModelCreated", "WorldCreated"]);
    assert_eq!(opening_order(&h), ["NewSession", "LoadingGame", "ModelCreated", "WorldCreated"]);
    // ModelCreated's other_income_mod books into the loaded running record (category 11).
    assert_eq!(h.state().model.world.last_economy_record(au).unwrap()[11], 500 + 2500);
    assert!(!h.state().is_new_game);
    // Outside LoadingGame load_value has no reader (0x00461180): the unread 7 is gone, the default comes back.
    assert_eq!(seen_at_world_created(&h), (false, -1.0));
}

#[test]
fn a_new_campaign_fires_loading_game_with_its_start_position_values_and_stays_new() {
    let (mut h, au) = opening_host();
    // The start position's own LUA[] items (0x009968A0 posts LoadingGame for them, 0x0099878E).
    let startpos = [ScriptSaveValue::Int(700)];
    let fired = h.open_campaign(ScriptContext::for_faction("france"), CampaignOpening::NewCampaign, &startpos);
    assert!(fired.iter().all(|(_, r)| r.errors.is_empty()), "{fired:?}");
    let names: Vec<&str> = fired.iter().map(|(n, _)| *n).collect();
    // 0x009968A0 (NewSession, LoadingGame), 0x0087426F (ModelCreated), 0x00988A3F (NewCampaignStarted),
    // 0x00988A72 (WorldCreated).
    assert_eq!(names, ["NewSession", "LoadingGame", "ModelCreated", "NewCampaignStarted", "WorldCreated"]);
    assert_eq!(opening_order(&h), names);
    // The handler read the start position's value, so ModelCreated booked it (the exe does the same).
    assert_eq!(h.state().model.world.last_economy_record(au).unwrap()[11], 500 + 700);
    // is_new_game is the opening's flag (0x009798E0), not "LoadingGame did not run".
    assert!(h.state().is_new_game);
    assert_eq!(seen_at_world_created(&h), (true, -1.0));
}

#[test]
fn a_new_campaign_with_an_empty_block_gives_the_handlers_their_defaults() {
    let (mut h, au) = opening_host();
    let fired = h.open_campaign(ScriptContext::for_faction("france"), CampaignOpening::NewCampaign, &[]);
    assert!(fired.iter().all(|(_, r)| r.errors.is_empty()), "{fired:?}");
    assert_eq!(fired.iter().filter(|(n, r)| *n == "LoadingGame" && r.handlers > 0).count(), 1);
    assert_eq!(h.state().model.world.last_economy_record(au).unwrap()[11], 500, "default 0: no boost");
    assert!(h.state().is_new_game);
}

/// `FactionBecomesLiberationProtectorate` gives a handler the liberator as its faction (event vtable
/// `0x0137D064` +0x38 `0x0044A500`, read by `FactionTreasury` `0x008A0860`) and nothing else: the
/// event has no region, settlement or character accessor.
#[test]
fn the_liberation_protectorate_event_gives_handlers_the_liberator() {
    use ntw_sim::campaign::CampaignEvent;
    let h = host();
    let fr = h.model().faction_by_key("france").unwrap().id;
    h.state_mut().model.world.factions.get_mut(&fr).unwrap().treasury = 1234;
    h.exec(
        r#"local e = require "data.events"
           e.FactionBecomesLiberationProtectorate = {}
           e.FactionBecomesLiberationProtectorate[1] = function(context)
               liberation = { conditions.FactionName("france", context), conditions.FactionTreasury(context) }
           end"#,
    )
    .unwrap();
    let ev = CampaignEvent::FactionBecomesLiberationProtectorate { faction: fr };
    let ctx = h.context_for(&ev);
    assert_eq!(ctx.faction.as_deref(), Some("france"));
    assert_eq!((ctx.region, ctx.settlement, ctx.character), (None, None, None));
    let r = h.fire(ev.script_name().unwrap(), h.context_for(&ev));
    assert_eq!((r.handlers, r.errors), (1, vec![]));
    let (is_france, treasury): (bool, f64) = h.lua().load("return liberation[1], liberation[2]").eval().unwrap();
    assert_eq!((is_france, treasury), (true, 1234.0));
}

/// The human's `ChooseCapture` liberate answer fires `FactionBecomesLiberationProtectorate` through the
/// host with the liberator (france, the region's owner when it is posted, `0x00B5089D`) as the handler's
/// faction, before the region passes to the liberated faction.
#[test]
fn a_liberate_choice_fires_the_protectorate_event_with_the_liberator() {
    use ntw_sim::campaign::{
        CampaignCommand, CampaignEvent, CaptureChoice, Region, RegionId, Settlement,
    };
    use ntw_sim::fixed::Fixed20;
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let id = |m: &ntw_sim::campaign::CampaignModel, k: &str| m.faction_by_key(k).unwrap().id;
    let (fr, au, pr) = (id(&model, "france"), id(&model, "austria"), id(&model, "prussia"));
    model.turn.humans = vec![fr];
    // Made-up region of austria's whose rebels are prussia, out of the game: a liberation target.
    let region = RegionId(1);
    model.world.regions.insert(
        region,
        Region {
            id: region,
            key: "test_region_1".into(),
            owner: au,
            settlement: Settlement { key: "settlement:test_region_1:town".into(), position: (Fixed20::from_int(0), Fixed20::from_int(0)) },
            slots: Vec::new(),
            road: None,
            fortification: None,
            road_holder: None,
            fortification_holder: None,
            population: 1000,
            base_gdp: 100,
            gdp: 100,
            wealth_growth_offset: 0,
            discontent_growth: 0,
            town_wealth: 0,
            town_wealth_growth: 0,
            tax_exempt: false,
            religions: Vec::new(),
            class_bases: Vec::new(),
            unit_resources: Vec::new(),
            population_state: Default::default(),
            recruitment_queue: Vec::new(),
            construction: Vec::new(),
            garrison: None,
            fleet: None,
            siege: Default::default(),
        },
    );
    model.world.region_rebel_factions.insert(region, "prussia".into());
    model.world.faction_details.entry(pr).or_default().out_of_game = Some(true);
    model.capture_by_force(region, fr, None);
    assert_eq!(model.pending_capture.as_ref().and_then(|p| p.liberate), Some(pr));
    let mut h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.exec(
        r#"local e = require "data.events"
           e.FactionBecomesLiberationProtectorate = {}
           e.FactionBecomesLiberationProtectorate[1] = function(context)
               liberator_is_france = conditions.FactionName("france", context)
           end"#,
    )
    .unwrap();
    let fired = h.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    let (event, report) = fired
        .iter()
        .find(|(e, _)| matches!(e, CampaignEvent::FactionBecomesLiberationProtectorate { .. }))
        .expect("the liberation posts the event");
    assert_eq!(*event, CampaignEvent::FactionBecomesLiberationProtectorate { faction: fr });
    assert_eq!((report.handlers, report.errors.clone()), (1, vec![]));
    assert!(h.lua().load("return liberator_is_france").eval::<bool>().unwrap());
    assert_eq!(h.model().world.regions[&region].owner, pr);
}

#[test]
fn a_call_to_war_pauses_the_turn_until_the_human_ally_answers() {
    // An AI faction's war on a human's ally asks the human (0x00B268B0 → the pending offer); the turn
    // steps wait (0x008F4120) and the answer's command resumes them without replaying the AI's turn.
    use ntw_sim::campaign::{CampaignCommand, CampaignEvent, Stance};
    use std::cell::Cell;
    use std::rc::Rc;
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let id = |m: &ntw_sim::campaign::CampaignModel, k: &str| m.faction_by_key(k).unwrap().id;
    let (fr, au, pr, ru) = (id(&model, "france"), id(&model, "austria"), id(&model, "prussia"), id(&model, "russia"));
    model.turn.humans = vec![fr, au];
    model.ask_human_allies = true;
    model.form_alliance(au, pr).unwrap();
    let mut h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    h.fire("NewSession", ScriptContext::default());
    h.end_turn();
    let played = Rc::new(Cell::new(0));
    let count = played.clone();
    h.set_ai_turn_hook(Box::new(move |state, faction| {
        if faction != ru {
            return Vec::new();
        }
        count.set(count.get() + 1);
        state.model.declare_war_rules(ru, pr).unwrap()
    }));
    let fired = h.end_turn();
    assert!(!fired.iter().any(|(e, _)| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == fr)), "france's next turn waits");
    assert_eq!(h.model().pending_ally_war_calls.len(), 1, "austria is asked");
    assert!(!h.model().turn.queue.is_empty(), "the turn waits");
    assert_ne!(h.model().world.stance(ru, pr), Stance::War);
    let fired = h.apply(CampaignCommand::RespondToAllyWarCall { side: Some(pr), accept: true }).unwrap();
    assert_eq!(h.model().world.stance(ru, pr), Stance::War);
    assert_eq!(h.model().world.stance(au, ru), Stance::War, "austria joined");
    assert!(fired.iter().any(|(e, _)| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == fr)), "the turn went on");
    assert_eq!(played.get(), 1, "russia's turn is not played again");
}

#[test]
fn character_looted_settlement_gives_the_character_and_no_faction() {
    // The exe's event (vtable 0x0137CCB8) answers only the character getter (+0x1C); the faction
    // getter (+0x38) is the null stub 0x00461180, so FactionIsLocal is false even for the local
    // faction's own looter, while the character conditions read the looter.
    use ntw_sim::campaign::{CampaignEvent, Character, CharacterId, CharacterKind};
    use ntw_sim::fixed::Fixed20;
    let src = ScriptSource::empty().with_memory_file("events.lua", EVENTS).with_memory_file("test.lua", SCRIPT);
    let mut model = common::tiny_model();
    let fr = model.faction_by_key("france").unwrap().id;
    let c = CharacterId(7);
    model.world.characters.insert(
        c,
        Character {
            id: c,
            faction: fr,
            kind: CharacterKind::General,
            position: (Fixed20::from_int(0), Fixed20::from_int(0)),
            movement_points: 10,
            max_movement_points: 10,
            base_movement_points: 10,
            garrisoned_in: None,
        },
    );
    let h = ScriptHost::new(model, "france", src).unwrap();
    h.run_file("data/test.lua").unwrap();
    let ev = CampaignEvent::CharacterLootedSettlement { character: c };
    assert_eq!(ev.script_name(), Some("CharacterLootedSettlement"));
    let ctx = h.context_for(&ev);
    assert_eq!(ctx, ScriptContext { character: Some(c.0), ..Default::default() });
    h.exec(
        r#"local e = require "data.events"
           e.CharacterLootedSettlement = {}
           e.CharacterLootedSettlement[1] = function(context)
               looted[#looted+1] = tostring(conditions.CharacterType("General", context)) .. " " .. tostring(conditions.FactionIsLocal(context))
           end
           looted = {}"#,
    )
    .unwrap();
    let r = h.fire("CharacterLootedSettlement", ctx);
    assert_eq!((r.handlers, r.errors.clone()), (1, vec![]));
    let seen: Vec<String> = h.lua().load("return looted").eval().unwrap();
    assert_eq!(seen, vec!["true false"]);
}

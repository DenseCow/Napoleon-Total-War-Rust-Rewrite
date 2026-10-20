//! Runs the ORIGINAL eur_napoleon campaign scripts from the player's install (read-only).
//! Skipped (passes with a message) when the install is not there.
//! Override the location with the `NTW_DATA_DIR` environment variable.
//! See the log with `cargo test -p ntw_script --test real_install -- --nocapture`.

mod common;

use std::path::PathBuf;

use ntw_script::{CampaignOpening, ScriptContext, ScriptHost, ScriptSource};
use ntw_sim::campaign::{CampaignCommand, Stance};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

#[test]
fn eur_napoleon_scripts_load_and_handle_events() {
    let dir = data_dir();
    if !dir.join("campaigns").join("eur_napoleon").join("scripting.lua").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let mut host = ScriptHost::new(common::tiny_model(), "france", source).unwrap();
    host.load_campaign("eur_napoleon").expect("scripts load without Lua errors");

    let ui = ScriptContext { string: Some("Campaign UI".into()), ..Default::default() };
    let sequence = [
        ("NewSession", ScriptContext::default()),
        ("WorldCreated", ScriptContext::default()),
        ("NewCampaignStarted", ScriptContext::default()),
        ("UICreated", ui),
        ("FactionTurnStart", ScriptContext::for_faction("france")),
        ("FactionTurnStart", ScriptContext::for_faction("austria")),
    ];
    let mut handlers = 0;
    for (name, ctx) in sequence {
        let r = host.fire(name, ctx);
        assert!(r.errors.is_empty(), "{name}: {:?}", r.errors);
        handlers += r.handlers;
    }
    // A full turn through the model, forwarded to the scripts.
    for (ev, r) in host.end_turn() {
        assert!(r.errors.is_empty(), "{ev:?}: {:?}", r.errors);
    }
    // Let any time triggers the scripts started (camera pans, mission timers) fire.
    for r in host.advance_time(600.0) {
        assert!(r.errors.is_empty(), "TimeTrigger: {:?}", r.errors);
    }
    // The first mission (a time trigger the scripts start from an advice handler), and the
    // "Vienna taken" timer, which forces peace between France and Austria through the model.
    let r = host.fire("TimeTrigger", ScriptContext::with_string("trigger_first_mission"));
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let (fr, au) = {
        let st = host.state();
        (st.faction_id("france").unwrap(), st.faction_id("austria").unwrap())
    };
    host.state_mut().model.apply(CampaignCommand::DeclareWar { a: fr, b: au }).unwrap();
    let r = host.fire("TimeTrigger", ScriptContext::with_string("take_vienna_timer"));
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let st = host.state();
    assert_eq!(st.missions.first().map(|m| m.key.as_str()), Some("eur_take_vienna"));
    assert_eq!(st.missions[0].rewards, vec!["money:2000".to_string()]);
    assert_ne!(st.model.world.factions[&fr].diplomacy.get(&au), Some(&Stance::War), "peace was forced");
    eprintln!("{handlers} handlers ran; {} log lines; missions {:?}", st.log.len(), st.missions);
    for line in st.log.iter().take(40) {
        eprintln!("  {line}");
    }
    assert!(handlers > 0);
    assert!(st.log.iter().any(|l| l.contains("Playing episodic campaign: eur_napoleon")));
}

/// The real eur_napoleon start position, with France human: the turn loop steps through the model
/// and fires every phase's events into the original scripts, for turn 1 and three more turns,
/// without a script error.
#[test]
fn eur_napoleon_turn_loop_with_the_real_startpos() {
    let dir = data_dir();
    if !dir.join("campaigns").join("eur_napoleon").join("startpos.esf").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let db = ntw_data::GameDatabase::from_install(&dir).expect("DB");
    let mut loaded = ntw_campaign::read_file(dir.join("campaigns").join("eur_napoleon").join("startpos.esf"), &db).expect("startpos");
    assert!(loaded.set_human("france"));
    // The start position's own LUA[] items (17 in eur_napoleon), read by OnLoadingGame on a new campaign too.
    assert_eq!(loaded.script_values.len(), 17);
    let script_values = loaded.script_values;
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let mut host = ScriptHost::new(loaded.model, "france", source).unwrap();
    host.load_campaign("eur_napoleon").expect("scripts load");
    let opened = host.open_campaign(ScriptContext::for_faction("france"), CampaignOpening::NewCampaign, &script_values);
    for (name, r) in &opened {
        assert!(r.errors.is_empty(), "{name}: {:?}", r.errors);
    }
    let ran = |event: &str| opened.iter().any(|(n, r)| *n == event && r.handlers > 0);
    assert!(ran("LoadingGame") && ran("NewCampaignStarted"), "{:?}", opened.iter().map(|(n, r)| (*n, r.handlers)).collect::<Vec<_>>());
    assert!(host.state().is_new_game);
    let started = host.start_campaign();
    assert!(started.iter().any(|(e, r)| matches!(e, ntw_sim::campaign::CampaignEvent::FactionTurnStart { .. }) && r.handlers > 0));
    for (ev, r) in &started {
        assert!(r.errors.is_empty(), "{ev:?}: {:?}", r.errors);
    }
    for _ in 0..3 {
        for (ev, r) in host.apply(CampaignCommand::EndTurn).unwrap() {
            assert!(r.errors.is_empty(), "{ev:?}: {:?}", r.errors);
        }
    }
    let m = host.model();
    assert_eq!(m.calendar.turns_elapsed, 3);
    assert_eq!(m.turn.current.and_then(|f| m.world.factions.get(&f)).map(|f| f.key.as_str()), Some("france"));
}

/// The user's spa_napoleon save at turn 1 (France human, `orig_fr_t1`): the load books France's other
/// income without the script's boost (the entry is not saved); the opening's `ModelCreated` handler
/// (`OnModelCreated`, after `OnLoadingGame` read `VAR_other_income_boost_france`) adds it again, so
/// France's income is the original's turn-1 figure (UI_FIDELITY.md 4.7 "wealth value": taxes 12030,
/// trade 1856, other income 500 + the 2500 boost).
#[test]
fn spa_napoleon_load_books_the_saved_boosts_again_at_model_created() {
    let dir = data_dir();
    let ev = std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(r"Documents\ntw-evidence\saves"));
    let save = ev.join("orig_fr_t1.save");
    if !dir.join("campaigns").join("spa_napoleon").join("scripting.lua").is_file() || !save.is_file() {
        eprintln!("skipped: no install at {} or no {}", dir.display(), save.display());
        return;
    }
    let db = ntw_data::GameDatabase::from_install(&dir).expect("DB");
    let loaded = ntw_campaign::read_file(&save, &db).expect("save");
    let values = loaded.script_values;
    let fr = loaded.model.faction_by_key("spa_france").expect("spa_france").id;
    assert_eq!(loaded.model.world.last_income(fr), 12030 + 1856 + 500, "the load alone");
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let mut host = ScriptHost::new(loaded.model, "spa_france", source).unwrap();
    host.load_campaign("spa_napoleon").expect("scripts load");
    let fired = host.open_campaign(ScriptContext::for_faction("spa_france"), CampaignOpening::LoadedSave, &values);
    for (name, r) in &fired {
        assert!(r.errors.is_empty(), "{name}: {:?}", r.errors);
    }
    let model_created = fired.iter().filter(|(n, r)| *n == "ModelCreated" && r.handlers > 0).count();
    assert_eq!(model_created, 1, "OnModelCreated ran once");
    assert_eq!(host.model().world.last_income(fr), 12030 + 1856 + 3000, "with the boost OnModelCreated books again");
}

/// A new spa_napoleon campaign also fires `LoadingGame`, with the start position's 30 stored items
/// (`0x009968A0` posts it whenever the file has a `LUA` block, CAMPAIGN_DATA.md §4.1): `OnLoadingGame`
/// sets `BOOL_load_game`, and `OnModelCreated` then books the start position's boosts, all 0, so the
/// opening leaves France's income as the start position has it.
#[test]
fn spa_napoleon_new_campaign_reads_its_start_position_values_at_loading_game() {
    let dir = data_dir();
    let sp = dir.join("campaigns").join("spa_napoleon").join("startpos.esf");
    if !sp.is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let db = ntw_data::GameDatabase::from_install(&dir).expect("DB");
    let mut loaded = ntw_campaign::read_file(&sp, &db).expect("startpos");
    assert!(loaded.set_human("spa_france"));
    assert_eq!(loaded.script_values.len(), 30);
    let values = loaded.script_values;
    let fr = loaded.model.faction_by_key("spa_france").expect("spa_france").id;
    let before = loaded.model.world.last_income(fr);
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let mut host = ScriptHost::new(loaded.model, "spa_france", source).unwrap();
    host.load_campaign("spa_napoleon").expect("scripts load");
    let fired = host.open_campaign(ScriptContext::for_faction("spa_france"), CampaignOpening::NewCampaign, &values);
    for (name, r) in &fired {
        assert!(r.errors.is_empty(), "{name}: {:?}", r.errors);
    }
    let names: Vec<&str> = fired.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, ["NewSession", "LoadingGame", "ModelCreated", "NewCampaignStarted", "WorldCreated"]);
    assert!(fired.iter().any(|(n, r)| *n == "LoadingGame" && r.handlers > 0), "OnLoadingGame ran");
    assert!(host.state().log.iter().any(|l| l.contains("muahahahaha")), "OnModelCreated ran");
    assert_eq!(host.model().world.last_income(fr), before, "the start position's boosts are 0");
    assert!(host.state().is_new_game);
}

/// The vanilla `export_triggers.lua` handler `C_NTW_General_Looter_Good_Trigger` answers the loot
/// choice's `CharacterLootedSettlement` (character-only context, vtable `0x0137CCB8`): for a
/// General it rolls `C_NTW_General_Looter_Good` (1 point at 12, then 1 point at 25), so a French
/// general of the real start position gains it within a few loots, without a script error.
#[test]
fn eur_napoleon_looting_reaches_the_looter_trait_trigger() {
    use ntw_sim::campaign::{CampaignEvent, CharacterKind};
    let dir = data_dir();
    if !dir.join("campaigns").join("eur_napoleon").join("startpos.esf").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let db = ntw_data::GameDatabase::from_install(&dir).expect("DB");
    let mut loaded = ntw_campaign::read_file(dir.join("campaigns").join("eur_napoleon").join("startpos.esf"), &db).expect("startpos");
    assert!(loaded.set_human("france"));
    let fr = loaded.model.faction_by_key("france").unwrap().id;
    let general = loaded
        .model
        .world
        .characters
        .values()
        .filter(|c| c.faction == fr && c.kind == CharacterKind::General)
        .map(|c| c.id)
        .min()
        .expect("a French general");
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let mut host = ScriptHost::new(loaded.model, "france", source).unwrap();
    host.load_campaign("eur_napoleon").expect("scripts load");
    let ctx = host.context_for(&CampaignEvent::CharacterLootedSettlement { character: general });
    for _ in 0..40 {
        let r = host.fire("CharacterLootedSettlement", ctx.clone());
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(r.handlers >= 1, "the vanilla trigger is registered");
    }
    let st = host.state();
    assert!(
        st.character_log.iter().any(|l| l.contains("C_NTW_General_Looter_Good") && l.contains(&general.raw().to_string())),
        "{:?}",
        st.character_log
    );
}

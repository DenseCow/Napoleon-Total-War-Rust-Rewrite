//! Runs the ORIGINAL eur_napoleon campaign scripts from the player's install (read-only).
//! Skipped (passes with a message) when the install is not there.
//! Override the location with the `NTW_DATA_DIR` environment variable.
//! See the log with `cargo test -p ntw_script --test real_install -- --nocapture`.

mod common;

use std::path::PathBuf;

use ntw_script::{ScriptContext, ScriptHost, ScriptSource};
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
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let mut host = ScriptHost::new(loaded.model, "france", source).unwrap();
    host.load_campaign("eur_napoleon").expect("scripts load");
    for name in ["NewSession", "NewCampaignStarted"] {
        assert!(host.fire(name, ScriptContext::for_faction("france")).errors.is_empty());
    }
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

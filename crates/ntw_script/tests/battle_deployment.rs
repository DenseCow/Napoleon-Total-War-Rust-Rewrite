//! The battle HUD's deployment phase with the ORIGINAL battle UI scripts from the player's install
//! (read-only). Skipped when the install is not there.
//!
//! Regression test for the harness report "the battle stays in deployment" (BATTLE_FIDELITY.md
//! §12): nothing ends deployment by itself, and the deployment panel's Start Battle button
//! (`button_battle_start`) does, through `BattleUI.InformOfDeploymentFinished`.

use std::path::PathBuf;

use ntw_formats::loc::Localisation;
use ntw_script::ScriptSource;
use ntw_script::ui::battle::{self as ui_battle, BattleHudFacts, BattleUiRequest, HudPhase};
use ntw_script::ui::{FrontEndFacts, PointerEvent, UiScriptHost};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

#[test]
fn deployment_waits_for_the_start_battle_button() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0)).unwrap();
    ui_battle::install(&host, ScriptSource::from_install(&dir).unwrap()).expect("battle engine functions");
    let bf = BattleHudFacts { phase: HudPhase::Deployment, speed: 1.0, ..Default::default() };
    ui_battle::set_facts(&host, &bf).unwrap();
    let root = ui_battle::load_hud(&host).expect("battle HUD layout");
    ui_battle::call_event(&host, "ShowDeploymentPopup").unwrap();
    ui_battle::call_event(&host, "SetDeploymentPopupAsDeploymentStart").unwrap();

    // Thirty seconds of HUD time pass: deployment does not end on its own.
    for i in 0..300 {
        ui_battle::set_facts(&host, &bf).unwrap();
        host.pulse(f64::from(i) * 100.0);
        let r = ui_battle::take_requests(&host).unwrap();
        assert!(!r.contains(&BattleUiRequest::DeploymentFinished), "deployment ended without a click");
    }

    // A player's click on Start Battle ends it.
    let mut button = None;
    host.world().visit_visible(root, &mut |n, node| {
        if button.is_none() && node.data.id == "button_battle_start" {
            button = Some(n);
        }
    });
    let button = button.expect("Start Battle button visible in deployment");
    host.pointer(button, PointerEvent::Enter);
    host.pointer(button, PointerEvent::LeftDown);
    host.pointer(button, PointerEvent::LeftUp);
    let _ = ui_battle::click(&host, button, false);
    let r = ui_battle::take_requests(&host).unwrap();
    for l in host.take_log() {
        println!("{l}");
    }
    assert!(r.contains(&BattleUiRequest::DeploymentFinished), "requests after the click: {r:?}");
}

/// The battle HUD runs layout state functions too, as the exe does for every UI (`0x01035620`
/// tests no mode, CONFIRMED): the land orders' turn-left button (`land_battle_orders`) has no
/// click binding; its "Unselected_depress" state's enter function `Rotate_Left_Down` starts the
/// rotation and its exit function `Rotate_Left_Up` sends `BattleUI.Current_Selection_Rotate_Left`
/// when the press is released (`hud_rotate.lua`).
#[test]
fn turn_left_order_button_works_through_its_state_functions() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0)).unwrap();
    ui_battle::install(&host, ScriptSource::from_install(&dir).unwrap()).expect("battle engine functions");
    let unit = ui_battle::HudUnit { id: 1, key: "Inf_Line_French_Fusiliers".into(), men: 100, max_men: 100, selected: true, category: "infantry".into(), ..Default::default() };
    let bf = BattleHudFacts { phase: HudPhase::Conflict, speed: 1.0, units: vec![unit], ..Default::default() };
    ui_battle::set_facts(&host, &bf).unwrap();
    let root = ui_battle::load_hud(&host).expect("battle HUD layout");
    ui_battle::update_orders(&host).unwrap();
    host.pulse(0.0);
    let mut button = None;
    host.world().visit_visible(root, &mut |n, node| {
        if button.is_none() && node.data.id == "UC_button_turn_left" {
            button = Some(n);
        }
    });
    let button = button.expect("the turn-left order button is shown");
    assert_eq!(host.world().get(button).unwrap().state_name(), "Unselected", "a unit is selected");
    let _ = ui_battle::take_requests(&host).unwrap();
    host.take_log();
    host.pointer(button, PointerEvent::Enter);
    host.pointer(button, PointerEvent::LeftDown);
    assert_eq!(host.world().get(button).unwrap().state_name(), "Unselected_depress");
    host.pointer(button, PointerEvent::LeftUp);
    ntw_script::ui::test_support::no_errors(&host);
    let r = ui_battle::take_requests(&host).unwrap();
    let turns = r.iter().filter(|q| matches!(q, BattleUiRequest::Order { name, .. } if name == "Current_Selection_Rotate_Left")).count();
    assert_eq!(turns, 1, "one turn-left order on release: {r:?}");
}

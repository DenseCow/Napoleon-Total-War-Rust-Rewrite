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
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
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
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
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

/// Where the original puts the orders bar (traced 2026-10-09, BATTLE_FLOW.md §3 "Where
/// CreateFromLayout puts a layout"): battle_hud.lua's `CreateFromLayout(".../land_battle_orders",
/// "orders", veneer_DY)` creates only the file root's FIRST child (`0x01027400`: the holder's first
/// child is the template), named "orders", at the absolute frame position its file offset gives
/// (`0x010275F0`), drawn with the dock shift of its top-level ancestor `veneer_DY` (dock 8,
/// `0x0102DAD0`). The second panel (land_battle_ordersOLD) does not exist.
#[test]
fn the_orders_bar_is_the_files_first_panel_at_its_own_position() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let file = ntw_formats::ui_layout::UiLayout::read(&vfs.read("ui/battle ui/land_battle_orders").unwrap()).unwrap();
    let first = &file.root.children[0];
    let own = (first.offset.0 as f32, first.offset.1 as f32);
    for (w, h) in [(1280.0, 960.0), (1920.0, 1080.0)] {
        let loc = Localisation::from_vfs(&vfs).unwrap();
        let facts = FrontEndFacts { game_version: "test".into(), ..Default::default() };
        let host = UiScriptHost::new(ScriptSource::from_install(&dir).unwrap(), loc, facts, (w, h), ntw_sim::limits::GameLimits::default()).unwrap();
        ui_battle::install(&host, ScriptSource::from_install(&dir).unwrap()).expect("battle engine functions");
        ui_battle::set_facts(&host, &BattleHudFacts { phase: HudPhase::Conflict, speed: 1.0, ..Default::default() }).unwrap();
        let root = ui_battle::load_hud(&host).expect("battle HUD layout");
        let world = host.world();
        let orders = world.find(root, "orders").expect("the orders panel");
        assert_eq!(world.get(orders).unwrap().layout_file, "ui/battle ui/land_battle_orders");
        assert!(world.find(root, "land_battle_ordersOLD").is_none(), "only the first panel is created");
        // Its absolute frame position is its own file offset, on screen plus veneer_DY's shift.
        let shift = ((w - 1280.0) / 2.0, h - 960.0);
        let r = world.get(orders).unwrap().rect;
        assert_eq!((r.x, r.y), (own.0 + shift.0, own.1 + shift.1), "at {w}x{h}");
        let cards = world.get(world.find(root, "cards_panel").unwrap()).unwrap().rect;
        assert_eq!(r.y + r.h, cards.y, "the bar sits on the cards at {w}x{h}");
    }
}

/// A battle HUD whose facts do not change asks for no redraw (polish: its pulse scripts resize,
/// re-dock and reorder to the values already there every frame, and each call bumped the redraw
/// counter, so the HUD was redrawn every frame).
#[test]
fn a_steady_battle_hud_asks_for_no_redraw() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    ui_battle::install(&host, ScriptSource::from_install(&dir).unwrap()).expect("battle engine functions");
    let bf = BattleHudFacts { phase: HudPhase::Conflict, speed: 1.0, ..Default::default() };
    ui_battle::set_facts(&host, &bf).unwrap();
    let _root = ui_battle::load_hud(&host).expect("battle HUD layout");
    for i in 0..50 {
        ui_battle::set_facts(&host, &bf).unwrap();
        host.pulse(f64::from(i) * 100.0);
    }
    let mut gens = Vec::new();
    for i in 50..55 {
        ui_battle::set_facts(&host, &bf).unwrap();
        let g0 = host.world().generation;
        host.pulse(f64::from(i) * 100.0);
        gens.push(host.world().generation - g0);
    }
    assert_eq!(gens, [0; 5], "redraws asked for by steady pulses");
}

/// A card's `Update` gets the UI time as its second argument, as review_DY.lua's
/// `UpdateCards(units, time)` gives it (the engine's float, `0x0058E3D0`). Without it a unit that
/// fires raised "attempt to compare number with nil" in `UpdateStateIcon`
/// (template.BattleUnitCard.lua:163) every frame (in-game check of 2026-10-10, NHB_Austerlitz).
#[test]
fn a_firing_units_card_updates_without_script_errors() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    ui_battle::install(&host, ScriptSource::from_install(&dir).unwrap()).expect("battle engine functions");
    let unit = ui_battle::HudUnit {
        id: 7,
        key: "test_unit".into(),
        name: "Test".into(),
        men: 100,
        max_men: 120,
        has_ammo: true,
        ammo_percent: 80.0,
        firing: true,
        under_fire: true,
        ..Default::default()
    };
    let bf = BattleHudFacts { phase: HudPhase::Conflict, speed: 1.0, units: vec![unit], ..Default::default() };
    ui_battle::set_facts(&host, &bf).unwrap();
    ui_battle::load_hud(&host).expect("battle HUD layout");
    ui_battle::create_cards(&host).unwrap();
    let _ = host.take_log();
    for i in 0..40 {
        host.set_time(f64::from(i) * 100.0);
        ui_battle::set_facts(&host, &bf).unwrap();
        ui_battle::update_cards(&host).expect("card update");
        host.pulse(f64::from(i) * 100.0);
    }
    // Every script error the card updates and pulses raise (the host logs each as `ERROR ...`), not only
    // those naming the card component.
    let errors: Vec<String> = host.take_log().into_iter().filter(|l| l.starts_with("ERROR")).collect();
    assert!(errors.is_empty(), "script errors: {errors:?}");
}

/// A battle HUD with one player unit (id 7) and its card, at `speed`.
fn hud_with_one_card(dir: &std::path::Path) -> (UiScriptHost, ntw_script::ui::NodeId, BattleHudFacts) {
    let source = ScriptSource::from_install(dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    ui_battle::install(&host, ScriptSource::from_install(dir).unwrap()).expect("battle engine functions");
    let unit = ui_battle::HudUnit { id: 7, key: "test_unit".into(), name: "Test".into(), men: 100, max_men: 120, category: "infantry".into(), ..Default::default() };
    let bf = BattleHudFacts { phase: HudPhase::Conflict, speed: 1.0, units: vec![unit], ..Default::default() };
    ui_battle::set_facts(&host, &bf).unwrap();
    let root = ui_battle::load_hud(&host).expect("battle HUD layout");
    ui_battle::create_cards(&host).unwrap();
    (host, root, bf)
}

fn find(host: &UiScriptHost, root: ntw_script::ui::NodeId, id: &str) -> ntw_script::ui::NodeId {
    let mut found = None;
    host.world().visit_visible(root, &mut |n, node| {
        if found.is_none() && node.data.id == id {
            found = Some(n);
        }
    });
    found.unwrap_or_else(|| panic!("no visible {id}"))
}

fn state(host: &UiScriptHost, id: ntw_script::ui::NodeId) -> String {
    host.world().get(id).unwrap().state_name().to_owned()
}

/// The speed ring (the "selected" state) moves to the button of the speed the battle runs at:
/// the engine tells root.lua `TimeRequestChanged` (0x005E79A0), which asks `BattleUI.TickPeriod`
/// (0x005D3AA0: "pause" / "slow" / "play" / "fwd" / "ffwd") and selects that button.
#[test]
fn the_speed_ring_follows_the_battle_speed() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let (host, root, mut bf) = hud_with_one_card(&dir);
    let buttons = ["pause", "slow_mo", "play", "fwd", "ffwd"];
    for (speed, ring) in [(1.0, "play"), (4.0, "ffwd"), (2.0, "fwd"), (0.4, "slow_mo"), (0.0, "pause"), (1.0, "play")] {
        bf.speed = speed;
        ui_battle::set_facts(&host, &bf).unwrap();
        assert!(ui_battle::time_request_changed(&host, (0, 0), speed).unwrap(), "root.lua defines TimeRequestChanged");
        for b in buttons {
            let want = if b == ring { "selected" } else { "normal" };
            assert_eq!(state(&host, find(&host, root, b)), want, "{b} at speed {speed}");
        }
    }
    let errors: Vec<String> = host.take_log().into_iter().filter(|l| l.starts_with("ERROR")).collect();
    assert!(errors.is_empty(), "script errors: {errors:?}");
}

/// A selected unit's card goes to "Selected" (brighten_t0 +1.0) from "Default" and from the
/// pointer's "roll", and back to "Default" (brighten_t0 -0.3) when deselected: the card manager's
/// rules (0x01019000 / 0x010245D0).
#[test]
fn a_selected_units_card_lights_up_even_under_the_pointer() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let (host, root, mut bf) = hud_with_one_card(&dir);
    let card = find(&host, root, "card_7");
    let shader = |host: &UiScriptHost| {
        let w = host.world();
        let s = w.get(card).unwrap().current().unwrap();
        (s.shader.clone(), s.shader_vars[0])
    };
    let mut select = |selected: bool| {
        bf.units[0].selected = selected;
        ui_battle::set_facts(&host, &bf).unwrap();
        ui_battle::update_cards(&host).unwrap();
    };
    assert_eq!(state(&host, card), "Default");
    assert_eq!(shader(&host), ("brighten_t0".to_owned(), -0.3));
    select(true);
    assert_eq!(state(&host, card), "Selected");
    assert_eq!(shader(&host), ("brighten_t0".to_owned(), 1.0));
    select(false);
    assert_eq!(state(&host, card), "Default");
    // Under the pointer the card is "roll"; selecting it there still lights it up.
    host.pointer(card, PointerEvent::Enter);
    assert_eq!(state(&host, card), "roll");
    select(true);
    assert_eq!(state(&host, card), "Selected");
    host.pointer(card, PointerEvent::Leave);
    assert_eq!(state(&host, card), "Selected", "Selected has no Leave transition");
    select(false);
    assert_eq!(state(&host, card), "Default");
    let log = host.take_log();
    assert!(!log.iter().any(|l| l.starts_with("ERROR") || l.contains("UNKNOWN UIComponent:Shader")), "{log:?}");
}

/// The card manager sets a card's state only when its unit joins or leaves the selection
/// (0x01019000 / 0x010245D0), never while it stays: a selected unit that routs and rallies keeps
/// the state its card's Update gave it (not "Selected" again), and deselecting it sets "Default"
/// from any state but "UnSelectable" / "Routing" (here "roll").
#[test]
fn a_rallied_selected_card_is_not_reselected_and_deselect_goes_default_from_roll() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let (host, root, mut bf) = hud_with_one_card(&dir);
    let card = find(&host, root, "card_7");
    let mut step = |selected: bool, routing: bool| {
        bf.units[0].selected = selected;
        bf.units[0].routing = routing;
        ui_battle::set_facts(&host, &bf).unwrap();
        ui_battle::update_cards(&host).unwrap();
    };
    step(true, false);
    assert_eq!(state(&host, card), "Selected");
    step(true, true);
    assert_eq!(state(&host, card), "Routing");
    step(true, false);
    let rallied = state(&host, card);
    assert_ne!(rallied, "Selected", "a rallied card is not selected again while the selection stays");
    step(true, false);
    assert_eq!(state(&host, card), rallied, "nothing changes while the selection stays");
    host.pointer(card, PointerEvent::Enter);
    assert_eq!(state(&host, card), "roll");
    step(false, false);
    assert_eq!(state(&host, card), "Default", "deselect sets Default from roll");
    let errors: Vec<String> = host.take_log().into_iter().filter(|l| l.starts_with("ERROR")).collect();
    assert!(errors.is_empty(), "script errors: {errors:?}");
}

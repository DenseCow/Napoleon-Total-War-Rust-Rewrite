//! The battle advisor's window with the ORIGINAL UI scripts from the player's install (read-only):
//! root.lua's `ShowPersistentPanels` makes the menu bar and its advice window
//! (`ui/common ui/advice_interface`), whose text panel shows an advice and hands its close button
//! back to the engine (BATTLE_FIDELITY.md §59.8). Skipped when the install is not there.

use std::path::PathBuf;

use ntw_formats::loc::Localisation;
use ntw_script::ScriptSource;
use ntw_script::ui::battle::{self as ui_battle, AdviceView, BattleHudFacts, BattleUiRequest, HudPhase};
use ntw_script::ui::{FrontEndFacts, NodeId, PointerEvent, UiScriptHost};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

/// The first visible component named `id` under `root`.
fn visible(host: &UiScriptHost, root: NodeId, id: &str) -> Option<NodeId> {
    let mut found = None;
    host.world().visit_visible(root, &mut |n, node| {
        if found.is_none() && node.data.id == id {
            found = Some(n);
        }
    });
    found
}

#[test]
fn the_advice_window_shows_an_advice_and_its_close_button_dismisses_it() {
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
    ui_battle::set_facts(&host, &BattleHudFacts { phase: HudPhase::Deployment, speed: 1.0, ..Default::default() }).unwrap();
    let root = ui_battle::load_hud(&host).expect("battle HUD layout");
    // The HUD's own start (checked elsewhere).
    host.take_log();
    assert!(ui_battle::show_persistent_panels(&host).expect("ShowPersistentPanels runs"), "the advice window's text panel");
    assert!(visible(&host, root, "menu_bar").is_some(), "the menu bar is shown");
    assert!(visible(&host, root, "advice_text_panel").is_none(), "no advice yet: the window is hidden");

    let text = "December 2nd, 1805. Today I have given orders to relinquish control of Pratzen Heights.";
    let advice = AdviceView {
        text: text.into(),
        has_audio: true,
        icon_path: Some(r"data\UI\Common UI\Skins\advisor_napoleon_grownup.tga".into()),
        ..Default::default()
    };
    ui_battle::show_advice(&host, &advice, true).expect("UpdateWithNewAdvice runs");
    let panel = visible(&host, root, "advice_text_panel").expect("the advice is shown");
    let node = visible(&host, panel, "Text").expect("the text");
    let shown = host.world().get(node).and_then(|n| n.current().map(|s| s.text.clone()));
    assert_eq!(shown.as_deref(), Some(text));
    let log = host.take_log();
    // The text view's scroll bar tells its parent, which has no Notify (as elsewhere in the UI).
    let unexpected: Vec<_> = log.iter().filter(|l| l.starts_with("UNKNOWN") || l.contains("error") || (l.starts_with("LuaCall") && !l.ends_with("Notify"))).collect();
    assert!(unexpected.is_empty(), "{unexpected:#?}");

    // Its close button tells the engine (BattleUI.DismissCurrentAdvice).
    let close = visible(&host, root, "minimise_button").expect("the close button");
    host.pointer(close, PointerEvent::Enter);
    host.pointer(close, PointerEvent::LeftDown);
    host.pointer(close, PointerEvent::LeftUp);
    let _ = ui_battle::click(&host, close, false);
    assert!(ui_battle::take_requests(&host).unwrap().contains(&BattleUiRequest::DismissAdvice));
    ui_battle::advice_dismissed(&host).expect("CurrentAdviceDismissed runs");
}

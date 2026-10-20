//! Tests of the campaign's key order ([`UiScriptHost::campaign_key`]): the bindings come from a
//! `default_keys.xml`, a focused text field takes every key first and no shortcut fires then
//! (`0x00A27EB0` / `0x00EEE1E0`), and a stolen key goes to its stealer instead of the bindings.

use ntw_formats::default_keys::{Bindings, Held, Shortcuts};

use super::tests::{TestHud, test_hud};
use super::*;

/// A made-up keys file: quick save rebound to CTRL+F5, quick load on CTRL+L, the escape menu on
/// ESCAPE with any modifier, and a func mapped to another component.
const KEYS: &str = r#"<shortcuts version="32">
  <mode>campaign</mode>
  <mode>all</mode>
  <keyset name="fps">
    <category name="campaign">
      <group name="game_play" order="1">
        <func name="quick_save"><key modifier="CTRL">F5</key></func>
        <func name="quick_load"><key modifier="CTRL">L</key></func>
        <func name="show_lists"><key>L</key></func>
        <func name="escape_menu"><key modifier="ANY">ESCAPE</key></func>
      </group>
    </category>
  </keyset>
  <mappings>
    <mode name="campaign">
      <func name="quick_save" component="root"/>
      <func name="quick_load" component="root"/>
      <func name="show_lists" component="button_lists"/>
    </mode>
    <mode name="all">
      <func name="escape_menu" component="root"/>
    </mode>
  </mappings>
</shortcuts>"#;

fn bindings() -> Bindings {
    Shortcuts::parse(KEYS.as_bytes()).unwrap().bindings("campaign", "root")
}

/// The root records each `OnShortcut(name, down)`; the button records its `OnKey` and the
/// characters typed into it.
fn record(hud: &TestHud) -> (Table, Table) {
    let env = hud.host.script_env(hud.root).unwrap();
    hud.host.lua().load("shortcuts = {}\nfunction OnShortcut(name, down) table.insert(shortcuts, name) end").set_environment(env.clone()).exec().unwrap();
    let button = hud.host.world().find(hud.root, "button").unwrap();
    let benv = hud.host.script_env(button).unwrap();
    hud.host
        .lua()
        .load("keys = {}\nfunction OnKey(k, a, up) if up then table.insert(keys, k) end end\nfunction CharacterInput(c) table.insert(keys, 'char ' .. c) end\nUIComponent(Address):SetEventCallback('OnKey', OnKey)")
        .set_environment(benv.clone())
        .exec()
        .unwrap();
    (env.get("shortcuts").unwrap(), benv.get("keys").unwrap())
}

/// Gives the button the input focus, as clicking a text field does (its state takes the focus).
fn focus_button(hud: &TestHud) {
    let button = hud.host.world().find(hud.root, "button").unwrap();
    hud.host.inner().world.borrow_mut().get_mut(button).unwrap().data.states[0].unknown_d4 = 2;
    hud.host.lua().load("local b = ...; UIComponent(b):StealInputFocus(true)").into_function().unwrap().call::<()>(addr(button)).unwrap();
    assert!(hud.host.input_focus_held());
}

fn names(t: &Table) -> Vec<String> {
    t.sequence_values::<String>().map(Result::unwrap).collect()
}

const CTRL: Held = Held { ctrl: true, shift: false, alt: false };

fn press<'a>(key: &'a str, held: Held) -> CampaignKey<'a> {
    CampaignKey { key: Some(key), held, ..Default::default() }
}

#[test]
fn the_bindings_come_from_the_keys_file() {
    let hud = test_hud();
    let (shortcuts, _) = record(&hud);
    let b = bindings();
    // CTRL+F5 is quick save in this file; CTRL+S is nothing.
    assert!(hud.host.campaign_key(&b, press("F5", CTRL)));
    assert!(!hud.host.campaign_key(&b, press("S", CTRL)));
    // CTRL+L: quick load (root); plain L is show_lists, mapped to another component: not the root's.
    assert!(hud.host.campaign_key(&b, press("L", CTRL)));
    assert!(!hud.host.campaign_key(&b, press("L", Held::default())));
    // ESCAPE with any modifier; a key repeat fires nothing.
    assert!(hud.host.campaign_key(&b, press("ESCAPE", Held { shift: true, ..Held::default() })));
    assert!(!hud.host.campaign_key(&b, CampaignKey { repeat: true, ..press("ESCAPE", Held::default()) }));
    assert_eq!(names(&shortcuts), ["quick_save", "quick_load", "escape_menu"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

#[test]
fn a_focused_text_field_takes_the_keys_and_no_shortcut_fires() {
    let hud = test_hud();
    let (shortcuts, keys) = record(&hud);
    let b = bindings();
    focus_button(&hud);
    // The quick save binding does not fire while the field has the focus.
    assert!(!hud.host.campaign_key(&b, press("F5", CTRL)));
    // ESCAPE goes to the field, not to the escape menu.
    assert!(hud.host.campaign_key(&b, CampaignKey { edit: Some("ESCAPE"), ..press("ESCAPE", Held::default()) }));
    // Typed text reaches the field; not while CTRL (without ALT) is held.
    assert!(hud.host.campaign_key(&b, CampaignKey { text: Some("a"), ..press("A", Held::default()) }));
    assert!(!hud.host.campaign_key(&b, CampaignKey { text: Some("l"), ..press("L", CTRL) }));
    assert!(hud.host.campaign_key(&b, CampaignKey { text: Some("@"), ..press("Q", Held { ctrl: true, alt: true, shift: false }) }));
    assert_eq!(names(&shortcuts), Vec::<String>::new());
    assert_eq!(names(&keys), ["ESCAPE", "char a", "char @"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

#[test]
fn a_stolen_key_goes_to_its_stealer_instead_of_the_shortcut() {
    let hud = test_hud();
    let (shortcuts, keys) = record(&hud);
    let b = bindings();
    let button = hud.host.world().find(hud.root, "button").unwrap();
    hud.host.lua().load("local b = ...; UIComponent(b):StealShortcutKey('ESCAPE')").into_function().unwrap().call::<()>(addr(button)).unwrap();
    // The stealer's `OnKey` runs, not the root's `OnShortcut`.
    assert!(hud.host.campaign_key(&b, press("ESCAPE", Held::default())));
    assert_eq!(names(&shortcuts), Vec::<String>::new());
    assert_eq!(names(&keys), ["ESCAPE"]);
    // Given back, ESCAPE opens the escape menu again.
    hud.host.lua().load("local b = ...; UIComponent(b):StealShortcutKey(false)").into_function().unwrap().call::<()>(addr(button)).unwrap();
    assert!(hud.host.campaign_key(&b, press("ESCAPE", Held::default())));
    assert_eq!(names(&shortcuts), ["escape_menu"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

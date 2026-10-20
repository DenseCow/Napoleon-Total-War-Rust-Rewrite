//! Tests of the campaign's key order ([`UiScriptHost::campaign_key`]): the bindings come from a
//! `default_keys.xml` and run through the shortcut handler's state machine (`0x00EEE250`), a
//! focused text field takes every key first and no shortcut fires then (`0x00A27EB0` /
//! `0x00EEE1E0`), and a stolen key goes to its stealer instead of the bindings.

use ntw_formats::default_keys::{Held, Shortcuts};

use super::tests::{TestHud, test_hud};
use super::*;

/// A made-up keys file: quick save rebound to CTRL+F5, quick load on CTRL+L, the escape menu on
/// ESCAPE with any modifier, end turn on the test layout's button, a camera step, and a func
/// mapped to a component the layout does not have.
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
        <func name="end_turn"><key>RETURN</key></func>
        <func name="step_fwd" repeating="1"><key modifier="ALLOW_SHIFT">W</key></func>
      </group>
    </category>
  </keyset>
  <mappings>
    <mode name="campaign">
      <func name="quick_save" component="root"/>
      <func name="quick_load" component="root"/>
      <func name="show_lists" component="button_lists"/>
      <func name="end_turn" component="button"/>
      <func name="step_fwd" component="camera"/>
    </mode>
    <mode name="all">
      <func name="escape_menu" component="root"/>
    </mode>
  </mappings>
</shortcuts>"#;

fn machine() -> ShortcutMachine {
    ShortcutMachine::new(Shortcuts::parse(KEYS.as_bytes()).unwrap().mode_bindings("campaign"))
}

/// The root records each `OnShortcut(name, true)` (the release, where root.lua acts); the button
/// records its `OnShortcut(name, released)`, its `OnKey` releases and the characters typed into it.
fn record(hud: &TestHud) -> (Table, Table) {
    let env = hud.host.script_env(hud.root).unwrap();
    hud.host
        .lua()
        .load("shortcuts = {}\nfunction OnShortcut(name, up) if up then table.insert(shortcuts, name) end end")
        .set_environment(env.clone())
        .exec()
        .unwrap();
    let button = hud.host.world().find(hud.root, "button").unwrap();
    let benv = hud.host.script_env(button).unwrap();
    hud.host
        .lua()
        .load(
            "keys = {}\nfunction OnKey(k, a, up) if up then table.insert(keys, k) end end\n\
             function CharacterInput(c) table.insert(keys, 'char ' .. c) end\n\
             function OnShortcut(name, up) table.insert(keys, name .. ' ' .. tostring(up)) end\n\
             UIComponent(Address):SetEventCallback('OnKey', OnKey)",
        )
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

/// Runs `code` with the button's address as `...`.
fn on_button(hud: &TestHud, code: &str) {
    let button = hud.host.world().find(hud.root, "button").unwrap();
    hud.host.lua().load(code).into_function().unwrap().call::<()>(addr(button)).unwrap();
}

fn names(t: &Table) -> Vec<String> {
    t.sequence_values::<String>().map(Result::unwrap).collect()
}

const NONE: Held = Held { ctrl: false, shift: false, alt: false };
const CTRL: Held = Held { ctrl: true, shift: false, alt: false };

/// One key event at frame time 0.25; camera steps go to `steps`.
fn event(hud: &TestHud, m: &mut ShortcutMachine, key: CampaignKey, steps: &mut Vec<String>) {
    hud.host.campaign_key(m, CampaignKey { time: 0.25, ..key }, &mut |f, t| steps.push(format!("{f} {t}")));
}

/// A key pressed and let go: its down event, then its up event.
fn tap(hud: &TestHud, m: &mut ShortcutMachine, key: &str, held: Held) {
    let mut steps = Vec::new();
    let down = [key];
    event(hud, m, CampaignKey { down: &down, held, ..Default::default() }, &mut steps);
    event(hud, m, CampaignKey { up: true, held, ..Default::default() }, &mut steps);
    assert!(steps.is_empty());
}

#[test]
fn the_bindings_come_from_the_keys_file() {
    let hud = test_hud();
    let (shortcuts, _) = record(&hud);
    let m = &mut machine();
    // CTRL+F5 is quick save in this file; CTRL+S is nothing.
    tap(&hud, m, "F5", CTRL);
    tap(&hud, m, "S", CTRL);
    // CTRL+L: quick load (root); plain L is show_lists, mapped to a component the layout lacks.
    tap(&hud, m, "L", CTRL);
    tap(&hud, m, "L", NONE);
    // ESCAPE with any modifier.
    tap(&hud, m, "ESCAPE", Held { shift: true, ..NONE });
    assert_eq!(names(&shortcuts), ["quick_save", "quick_load", "escape_menu"]);
    // Held down, ESCAPE fires once: released on the first repeat, nothing on the later ones.
    let mut steps = Vec::new();
    let held = CampaignKey { down: &["ESCAPE"], ..Default::default() };
    event(&hud, m, held, &mut steps);
    assert_eq!(names(&shortcuts).len(), 3, "the root acts on the release");
    event(&hud, m, CampaignKey { repeat: true, ..held }, &mut steps);
    event(&hud, m, CampaignKey { repeat: true, ..held }, &mut steps);
    event(&hud, m, CampaignKey { up: true, ..Default::default() }, &mut steps);
    assert_eq!(names(&shortcuts), ["quick_save", "quick_load", "escape_menu", "escape_menu"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

#[test]
fn a_component_binding_hears_its_press_and_release() {
    let hud = test_hud();
    let (_, keys) = record(&hud);
    let m = &mut machine();
    let mut steps = Vec::new();
    // Return: end_turn goes to the button, pressed on the key down and released on the key up.
    event(&hud, m, CampaignKey { down: &["RETURN"], ..Default::default() }, &mut steps);
    assert_eq!(names(&keys), ["end_turn false"]);
    event(&hud, m, CampaignKey { up: true, ..Default::default() }, &mut steps);
    assert_eq!(names(&keys), ["end_turn false", "end_turn true"]);
    // A disabled button takes no shortcut.
    let button = hud.host.world().find(hud.root, "button").unwrap();
    hud.host.inner().world.borrow_mut().get_mut(button).unwrap().data.states[0].disabled = true;
    tap(&hud, m, "RETURN", NONE);
    assert_eq!(names(&keys).len(), 2);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// A binding's component is searched once per UI change: renamed, the button no longer takes
/// `end_turn`; named back, it does again.
#[test]
fn a_bindings_component_is_found_again_after_a_ui_change() {
    let hud = test_hud();
    let (_, keys) = record(&hud);
    let m = &mut machine();
    tap(&hud, m, "RETURN", NONE);
    assert_eq!(names(&keys), ["end_turn false", "end_turn true"]);
    let button = hud.host.world().find(hud.root, "button").unwrap();
    let rename = |name: &str| hud.host.inner().world.borrow_mut().get_mut(button).unwrap().data.id = name.to_owned();
    rename("renamed");
    tap(&hud, m, "RETURN", NONE);
    assert_eq!(names(&keys).len(), 2);
    rename("button");
    tap(&hud, m, "RETURN", NONE);
    assert_eq!(names(&keys).len(), 4);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

#[test]
fn the_camera_steps_on_every_key_event() {
    let hud = test_hud();
    record(&hud);
    let m = &mut machine();
    let mut steps = Vec::new();
    let w = CampaignKey { down: &["W"], ..Default::default() };
    event(&hud, m, w, &mut steps);
    event(&hud, m, CampaignKey { repeat: true, held: Held { shift: true, ..NONE }, ..w }, &mut steps);
    event(&hud, m, CampaignKey { up: true, ..Default::default() }, &mut steps);
    // CTRL is not ALLOW_SHIFT.
    event(&hud, m, CampaignKey { held: CTRL, ..w }, &mut steps);
    assert_eq!(steps, ["step_fwd 0.25", "step_fwd 0.25"]);
}

#[test]
fn enable_shortcut_handler_switches_the_bindings() {
    let hud = test_hud();
    let (shortcuts, _) = record(&hud);
    let m = &mut machine();
    let env = hud.host.script_env(hud.root).unwrap();
    hud.host.lua().load("CampaignUI.EnableShortcutHandler(false)").set_environment(env.clone()).exec().unwrap();
    tap(&hud, m, "F5", CTRL);
    assert_eq!(names(&shortcuts), Vec::<String>::new(), "switched off");
    hud.host.lua().load("CampaignUI.EnableShortcutHandler(true)").set_environment(env).exec().unwrap();
    tap(&hud, m, "F5", CTRL);
    assert_eq!(names(&shortcuts), ["quick_save"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

#[test]
fn a_focused_text_field_takes_the_keys_and_no_shortcut_fires() {
    let hud = test_hud();
    let (shortcuts, keys) = record(&hud);
    let m = &mut machine();
    focus_button(&hud);
    let mut steps = Vec::new();
    let mut key = |k: CampaignKey| event(&hud, m, k, &mut steps);
    // The quick save binding does not fire while the field has the focus, nor does a camera step.
    key(CampaignKey { down: &["F5"], held: CTRL, ..Default::default() });
    key(CampaignKey { up: true, held: CTRL, ..Default::default() });
    key(CampaignKey { down: &["W"], text: Some("w"), held: CTRL, ..Default::default() });
    // ESCAPE goes to the field, not to the escape menu.
    key(CampaignKey { down: &["ESCAPE"], edit: Some("ESCAPE"), ..Default::default() });
    // Typed text reaches the field; not while CTRL (without ALT) is held.
    key(CampaignKey { down: &["A"], text: Some("a"), ..Default::default() });
    key(CampaignKey { down: &["L"], text: Some("l"), held: CTRL, ..Default::default() });
    key(CampaignKey { down: &["Q"], text: Some("@"), held: Held { ctrl: true, alt: true, shift: false }, ..Default::default() });
    assert!(steps.is_empty());
    assert_eq!(names(&shortcuts), Vec::<String>::new());
    assert_eq!(names(&keys), ["ESCAPE", "char a", "char @"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

#[test]
fn a_stolen_key_goes_to_its_stealer_instead_of_the_shortcut() {
    let hud = test_hud();
    let (shortcuts, keys) = record(&hud);
    let m = &mut machine();
    on_button(&hud, "local b = ...; UIComponent(b):StealShortcutKey('ESCAPE')");
    // The stealer's `OnKey` runs (down, then up on the release), not the root's `OnShortcut`.
    tap(&hud, m, "ESCAPE", NONE);
    assert_eq!(names(&shortcuts), Vec::<String>::new());
    assert_eq!(names(&keys), ["ESCAPE"]);
    // A key no binding has: the stealer hears it too (without modifiers only).
    on_button(&hud, "local b = ...; UIComponent(b):StealShortcutKey('F9')");
    tap(&hud, m, "F9", NONE);
    tap(&hud, m, "F9", CTRL);
    assert_eq!(names(&keys), ["ESCAPE", "F9"]);
    // Given back, ESCAPE opens the escape menu again and F9 is nothing.
    on_button(&hud, "local b = ...; UIComponent(b):StealShortcutKey(false)");
    tap(&hud, m, "ESCAPE", NONE);
    tap(&hud, m, "F9", NONE);
    assert_eq!(names(&shortcuts), ["escape_menu"]);
    assert_eq!(names(&keys), ["ESCAPE", "F9"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

//! Tests of the campaign's key order ([`UiScriptHost::campaign_key`] per key event,
//! [`UiScriptHost::campaign_keys`] once per frame): the bindings come from a `default_keys.xml`
//! and run through the shortcut handler's state machine (`0x00EEE250`), a focused text field
//! takes every key first and no shortcut fires then (`0x00A27EB0` / `0x00EEE1E0`), and a stolen
//! key goes to its stealer instead of the bindings.

use ntw_formats::default_keys::{Held, Shortcuts};

use super::tests::{TestHud, test_hud};
use super::*;

/// A made-up keys file: quick save rebound to CTRL+F5 (`ignore_lock`, as vanilla's), quick load
/// on CTRL+L, the escape menu on ESCAPE with any modifier, end turn on the test layout's button,
/// a camera step, and a func mapped to a component the layout does not have.
const KEYS: &str = r#"<shortcuts version="32">
  <mode>campaign</mode>
  <mode>all</mode>
  <keyset name="fps">
    <category name="campaign">
      <group name="game_play" order="1">
        <func name="quick_save" ignore_lock="1"><key modifier="CTRL">F5</key></func>
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

/// A frame with one key event: the event's fields, and the keys down after it.
#[derive(Clone, Copy, Default)]
struct K<'a> {
    down: &'a [&'a str],
    up: bool,
    edit: Option<&'a str>,
    text: Option<&'a str>,
    repeat: bool,
    held: Held,
}

/// A frame with one key event: the event, then the frame's shortcut poll ([`frame`]).
fn event(hud: &TestHud, m: &mut ShortcutMachine, key: K, steps: &mut Vec<String>) {
    let K { down, up, edit, text, repeat, held } = key;
    hud.host.campaign_key(CampaignKey { up, edit, text, repeat, held });
    frame(hud, m, down, held, steps);
}

/// One frame's shortcut poll at frame time 0.25 with `down` the keys down; camera steps go to
/// `steps`.
fn frame(hud: &TestHud, m: &mut ShortcutMachine, down: &[&str], held: Held, steps: &mut Vec<String>) {
    hud.host.campaign_keys(m, CampaignKeys { down, held, time: 0.25 }, &mut |f, t| steps.push(format!("{f} {t}")));
}

/// A key pressed and let go: its down event, then its up event.
fn tap(hud: &TestHud, m: &mut ShortcutMachine, key: &str, held: Held) {
    let mut steps = Vec::new();
    let down = [key];
    event(hud, m, K { down: &down, held, ..Default::default() }, &mut steps);
    event(hud, m, K { up: true, held, ..Default::default() }, &mut steps);
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
    let held = K { down: &["ESCAPE"], ..Default::default() };
    event(&hud, m, held, &mut steps);
    assert_eq!(names(&shortcuts).len(), 3, "the root acts on the release");
    event(&hud, m, K { repeat: true, ..held }, &mut steps);
    event(&hud, m, K { repeat: true, ..held }, &mut steps);
    event(&hud, m, K { up: true, ..Default::default() }, &mut steps);
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
    event(&hud, m, K { down: &["RETURN"], ..Default::default() }, &mut steps);
    assert_eq!(names(&keys), ["end_turn false"]);
    event(&hud, m, K { up: true, ..Default::default() }, &mut steps);
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

/// Bug (user, 2026-10-20): WASD barely moved the camera. The bindings are polled once per frame
/// (`0x00A27EB0`, vtable `0x0136B40C` +0x20, called by `0x0048BD40` every frame), not per key
/// event: a held W steps the camera every frame, frames without a key event included, each step
/// with the frame's time; a non-repeating binding (Return) presses once per hold.
#[test]
fn the_camera_steps_every_frame_a_key_is_held() {
    let hud = test_hud();
    let (_, keys) = record(&hud);
    let m = &mut machine();
    let mut steps = Vec::new();
    let w = K { down: &["W"], ..Default::default() };
    event(&hud, m, w, &mut steps);
    for _ in 0..3 {
        frame(&hud, m, &["W"], NONE, &mut steps);
    }
    event(&hud, m, K { repeat: true, held: Held { shift: true, ..NONE }, ..w }, &mut steps);
    event(&hud, m, K { up: true, ..Default::default() }, &mut steps);
    frame(&hud, m, &[], NONE, &mut steps);
    // CTRL is not ALLOW_SHIFT.
    event(&hud, m, K { held: CTRL, ..w }, &mut steps);
    assert_eq!(steps, ["step_fwd 0.25"; 5]);
    // Return held over many frames: end_turn is pressed on the first, released on the second.
    for _ in 0..5 {
        frame(&hud, m, &["RETURN"], NONE, &mut steps);
    }
    assert_eq!(names(&keys), ["end_turn false", "end_turn true"]);
    frame(&hud, m, &[], NONE, &mut steps);
    frame(&hud, m, &["RETURN"], NONE, &mut steps);
    assert_eq!(names(&keys).len(), 3, "pressed again on the next hold");
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

/// Bug (user, 2026-10-20): with the save panel open from the ESC menu, Return still ended the
/// turn. A panel PanelManager opens locks the input layer to its priority
/// (`Component.LockPriority`, `0x01018130`): a lower component's binding is locked (`0x00DACBA0`),
/// a func with `ignore_lock` still fires, and from layer 10 the camera is locked (`0x009535B0`).
/// With the old layer back, every binding works again.
#[test]
fn a_priority_lock_locks_the_bindings_of_lower_components() {
    let hud = test_hud();
    let (shortcuts, keys) = record(&hud);
    let m = &mut machine();
    let env = hud.host.script_env(hud.root).unwrap();
    let lock = |n: i64| -> i64 {
        hud.host.lua().load("return Component.LockPriority(...)").set_environment(env.clone()).into_function().unwrap().call(n).unwrap()
    };
    let priority = |name: &str| {
        let id = hud.host.world().find(hud.root, name).unwrap();
        hud.host.world().get(id).unwrap().data.priority
    };
    let above = i64::from(priority("button").max(hud.host.world().get(hud.root).unwrap().data.priority)) + 10;
    assert_eq!(lock(above), 0, "the layer starts at 0");
    let mut steps = Vec::new();
    tap(&hud, m, "RETURN", NONE);
    tap(&hud, m, "L", CTRL);
    tap(&hud, m, "F5", CTRL);
    event(&hud, m, K { down: &["W"], ..Default::default() }, &mut steps);
    event(&hud, m, K { up: true, ..Default::default() }, &mut steps);
    assert_eq!(names(&keys), Vec::<String>::new(), "end_turn is locked");
    assert_eq!(names(&shortcuts), ["quick_save"], "quick_load is locked, quick_save ignores the lock");
    assert!(steps.is_empty(), "the camera is locked");
    // Closing the panel restores the old layer.
    assert_eq!(lock(0), above);
    tap(&hud, m, "RETURN", NONE);
    tap(&hud, m, "L", CTRL);
    event(&hud, m, K { down: &["W"], ..Default::default() }, &mut steps);
    assert_eq!(names(&keys), ["end_turn false", "end_turn true"]);
    assert_eq!(names(&shortcuts), ["quick_save", "quick_load"]);
    assert_eq!(steps, ["step_fwd 0.25"]);
    // The argument is rounded to the nearest integer, ties to even (`0x010556D0`).
    lock(0);
    let half: i64 = hud.host.lua().load("Component.LockPriority(2.5); return Component.LockPriority(0)").set_environment(env).call(()).unwrap();
    assert_eq!(half, 2);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
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
    let mut key = |k: K| event(&hud, m, k, &mut steps);
    // The quick save binding does not fire while the field has the focus, nor does a camera step.
    key(K { down: &["F5"], held: CTRL, ..Default::default() });
    key(K { up: true, held: CTRL, ..Default::default() });
    key(K { down: &["W"], text: Some("w"), held: CTRL, ..Default::default() });
    // ESCAPE goes to the field, not to the escape menu.
    key(K { down: &["ESCAPE"], edit: Some("ESCAPE"), ..Default::default() });
    // Typed text reaches the field; not while CTRL (without ALT) is held.
    key(K { down: &["A"], text: Some("a"), ..Default::default() });
    key(K { down: &["L"], text: Some("l"), held: CTRL, ..Default::default() });
    key(K { down: &["Q"], text: Some("@"), held: Held { ctrl: true, alt: true, shift: false }, ..Default::default() });
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

/// `StealShortcutKey(false)` also clears the input focus, whichever component holds it
/// (`0x00DA9C80`): the shortcuts work again at once.
#[test]
fn giving_back_the_stolen_keys_clears_the_input_focus() {
    let hud = test_hud();
    let (shortcuts, _) = record(&hud);
    let m = &mut machine();
    focus_button(&hud);
    on_button(&hud, "local b = ...; UIComponent(b):StealShortcutKey(false)");
    assert!(!hud.host.input_focus_held());
    tap(&hud, m, "ESCAPE", NONE);
    assert_eq!(names(&shortcuts), ["escape_menu"]);
    assert!(hud.errors().is_empty(), "{:?}", hud.errors());
}

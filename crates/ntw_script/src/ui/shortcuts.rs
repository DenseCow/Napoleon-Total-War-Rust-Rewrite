//! The shortcut handler's binding state machine: what one key event does to the key bindings of
//! `text/default_keys.xml` (CONFIRMED, disassembly of `0x00EEE250` and `0x00ED76E0`; evidence in
//! `analysis/fidelity/UI_FIDELITY.md` §5.1 "Shortcut keys"). Generic: the components, the camera
//! and the stolen keys are behind [`ShortcutTarget`]; the campaign HUD drives it
//! (`UiScriptHost::campaign_key`).
//!
//! On every key event, down or up, `0x00EEE250` polls each binding's keys: a key slot is held when
//! its key is down now (the frame's key snapshot, `0x00E20190`) and its modifier rule matches the
//! modifiers held. `0x00ED76E0` then walks the bindings in order:
//! - no slot held: a binding pressed before is released (its handler's release, `+0x2C`, or the
//!   stealer's key-up when one of its keys is stolen);
//! - a slot held and the binding is the active one and not repeating: released, if pressed (so a
//!   non-repeating binding presses once per hold and releases on the next key event, the first
//!   auto-repeat or the key's release);
//! - a slot held whose key is not stolen: pressed, when its handler is unlocked (or the func
//!   ignores the lock) and ready; it becomes the active binding;
//! - else (the held key is stolen) and not the active binding: the stealer hears the key go down.
//!
//! After the walk the active binding is cleared once none of its keys is held and it is not
//! repeating (or the slot last seen held is stolen).
//!
//! `StealShortcutKey` (`0x00EC3DE0`) marks every binding with the key stolen and clears its repeat
//! flag (given back, `0x00EC3AA0`, the flag returns); a key no binding has gets a binding of its
//! own for the stealer (`0x00EAF250`: the key without modifiers, not repeating), deleted when the
//! key is given back. Ours reads the stolen keys live from the target and keeps those extra
//! bindings in step at the start of each event (the exe deletes them at the end of the event after
//! the give-back; a stolen key with no binding is the only case either sees).
//!
//! PROVISIONAL: the bindings are in the keyset file's order; the exe's are in the order the
//! handlers registered (`0x00EDD1B0`, from each component's constructor `0x00DA6860`), which only
//! matters when one key event presses two bindings.

use ntw_formats::default_keys::{Binding, Bindings, Held, KeySlot, Modifier};

/// What the bindings act on: the UI components (by name), the camera, the stolen keys.
pub trait ShortcutTarget {
    /// The component a binding goes to exists (its handler is registered). A binding of a
    /// component that does not exist is released but never pressed.
    fn registered(&self, component: &str) -> bool;
    /// The component's handler is not locked (UI component: `0x00DACBA0`, its priority is at least
    /// the lowest input layer). A func with `ignore_lock` skips this test.
    fn unlocked(&self, component: &str) -> bool;
    /// The component's handler takes shortcuts now (UI component: `0x00DACBB0`, its current state
    /// is not disabled).
    fn ready(&self, component: &str) -> bool;
    /// The binding pressed (handler `+0x28`, with the event's frame time).
    fn press(&mut self, component: &str, func: &str, time: f32);
    /// The binding released (handler `+0x2C`).
    fn release(&mut self, component: &str, func: &str);
    /// A component stole the key (`StealShortcutKey`) and holds it.
    fn stolen(&self, key: &str) -> bool;
    /// Every stolen key, in the order they were first stolen.
    fn stolen_keys(&self) -> Vec<String>;
    /// The stealers of `keys` hear them go down or up (`0x00EA5090` / `0x00EB0900`). True when one
    /// heard it.
    fn send_stolen(&mut self, keys: &[&str], down: bool) -> bool;
}

/// One binding and its state.
#[derive(Debug, Clone)]
struct Slot {
    binding: Binding,
    /// A stealer's own binding for a key no binding has (flag bit1).
    stealer: bool,
    /// Pressed and not released yet (flag bit2).
    pressed: bool,
}

/// The bindings of one game mode and the handler's state ([`Self::key_event`]).
#[derive(Debug, Clone, Default)]
pub struct ShortcutMachine {
    slots: Vec<Slot>,
    /// The active binding (manager `+0x24`).
    active: Option<usize>,
    /// The key slot (0 or 1) last seen held (manager `+0x28`).
    held_slot: Option<usize>,
}

impl ShortcutMachine {
    /// The machine for one mode's bindings (`Shortcuts::mode_bindings`).
    pub fn new(bindings: Bindings) -> Self {
        Self { slots: bindings.0.into_iter().map(|binding| Slot { binding, stealer: false, pressed: false }).collect(), active: None, held_slot: None }
    }

    /// One key event (a key down, an auto-repeat or a key up), after the event's key went down or
    /// up: `is_down` says which keys (the exe's names) are down now, `held` the modifiers, `time`
    /// the frame time the press passes on.
    ///
    /// PROVISIONAL: the handler lock. `0x00EEE250` runs no binding while the handler's lock
    /// (+0x84, `0x00A22430`, set by `steal_user_input` `0x00979680` → `0x00A22400`) is set,
    /// unless the key is ESCAPE; ours has no lock, so every event runs the bindings.
    pub fn key_event(&mut self, is_down: &dyn Fn(&str) -> bool, held: Held, time: f32, target: &mut dyn ShortcutTarget) {
        self.sync_stealer_bindings(target);
        let slot_held = |k: Option<&KeySlot>| k.is_some_and(|k| is_down(&k.key) && k.modifier.matches(held));
        let entries: Vec<[bool; 2]> = self.slots.iter().map(|s| [slot_held(s.binding.keys.first()), slot_held(s.binding.keys.get(1))]).collect();
        for (i, &[k1, k2]) in entries.iter().enumerate() {
            let (registered, stolen, repeating) = self.state(i, target);
            if !k1 && !k2 {
                if self.slots[i].pressed {
                    self.release(i, registered, stolen, target);
                }
                continue;
            }
            self.held_slot = Some(if k1 { 0 } else { 1 });
            if !registered || (!repeating && self.active == Some(i)) {
                if self.slots[i].pressed {
                    self.release(i, registered, stolen, target);
                }
            } else if (!k1 || !stolen[0]) && (!k2 || !stolen[1]) {
                let b = &self.slots[i].binding;
                if (target.unlocked(&b.component) || b.ignore_lock) && target.ready(&b.component) {
                    target.press(&b.component, &b.func, time);
                    self.active = Some(i);
                    self.slots[i].pressed = true;
                }
            } else if self.active != Some(i) {
                if target.send_stolen(&self.stolen_keys(i, stolen), true) {
                    self.active = Some(i);
                }
                self.slots[i].pressed = true;
            }
        }
        if let Some(a) = self.active.filter(|&a| a < entries.len()) {
            let (_, stolen, repeating) = self.state(a, target);
            let stolen_held = self.held_slot.is_some_and(|h| stolen[h]);
            if (stolen_held || !repeating) && entries[a] == [false, false] {
                self.active = None;
            }
        }
    }

    /// Binding `i`'s handler exists, which of its key slots are stolen, and whether it repeats
    /// now (a stolen key turns the repeat off).
    fn state(&self, i: usize, target: &dyn ShortcutTarget) -> (bool, [bool; 2], bool) {
        let s = &self.slots[i];
        let stolen = [0, 1].map(|j| s.binding.keys.get(j).is_some_and(|k| target.stolen(&k.key)));
        let registered = s.stealer || target.registered(&s.binding.component);
        (registered, stolen, s.binding.repeating && !stolen[0] && !stolen[1])
    }

    /// Releases binding `i`: the handler's release, or the stealer's key-up when a key is stolen.
    fn release(&mut self, i: usize, registered: bool, stolen: [bool; 2], target: &mut dyn ShortcutTarget) {
        self.slots[i].pressed = false;
        if !registered {
            return;
        }
        if stolen[0] || stolen[1] {
            if target.send_stolen(&self.stolen_keys(i, stolen), false) {
                self.active = Some(i);
            }
        } else {
            let b = &self.slots[i].binding;
            target.release(&b.component, &b.func);
        }
    }

    fn stolen_keys(&self, i: usize, stolen: [bool; 2]) -> Vec<&str> {
        self.slots[i].binding.keys.iter().zip(stolen).filter(|(_, s)| *s).map(|(k, _)| k.key.as_str()).collect()
    }

    /// Adds a stealer's own binding for each stolen key no existing binding has, and removes the
    /// ones whose key is no longer stolen (fixing the active index as `0x00ED76E0`'s deletion does).
    fn sync_stealer_bindings(&mut self, target: &dyn ShortcutTarget) {
        let stolen = target.stolen_keys();
        let mut i = 0;
        while i < self.slots.len() {
            let s = &self.slots[i];
            if s.stealer && !stolen.iter().any(|k| *k == s.binding.keys[0].key) {
                self.slots.remove(i);
                self.active = match self.active {
                    Some(a) if a == i => None,
                    Some(a) if a > i => Some(a - 1),
                    a => a,
                };
            } else {
                i += 1;
            }
        }
        for key in stolen {
            let bound = self.slots.iter().any(|s| (s.stealer || target.registered(&s.binding.component)) && s.binding.keys.iter().any(|k| k.key == key));
            if !bound {
                let binding = Binding { keys: vec![KeySlot { key, modifier: Modifier::None }], ..Binding::default() };
                self.slots.push(Slot { binding, stealer: true, pressed: false });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::default_keys::Shortcuts;

    const KEYS: &str = r#"<shortcuts version="32">
  <mode>campaign</mode>
  <keyset name="fps">
    <category name="c">
      <group name="g">
        <func name="end_turn"><key>RETURN</key></func>
        <func name="quick_save" ignore_lock="1"><key modifier="CTRL">S</key></func>
        <func name="step_fwd" repeating="1"><key modifier="ALLOW_SHIFT">W</key></func>
        <func name="show_lists"><key>L</key></func>
      </group>
    </category>
  </keyset>
  <mappings>
    <mode name="campaign">
      <func name="end_turn" component="button_end_turn"/>
      <func name="quick_save" component="root"/>
      <func name="step_fwd" component="camera"/>
      <func name="show_lists" component="button_lists"/>
    </mode>
  </mappings>
</shortcuts>"#;

    #[derive(Default)]
    struct Target {
        log: Vec<String>,
        missing: Vec<&'static str>,
        locked: bool,
        disabled: Vec<&'static str>,
        stolen: Vec<String>,
    }

    impl ShortcutTarget for Target {
        fn registered(&self, c: &str) -> bool {
            !self.missing.contains(&c)
        }
        fn unlocked(&self, _: &str) -> bool {
            !self.locked
        }
        fn ready(&self, c: &str) -> bool {
            !self.disabled.contains(&c)
        }
        fn press(&mut self, c: &str, f: &str, t: f32) {
            self.log.push(format!("press {c} {f} {t}"));
        }
        fn release(&mut self, c: &str, f: &str) {
            self.log.push(format!("release {c} {f}"));
        }
        fn stolen(&self, key: &str) -> bool {
            self.stolen.iter().any(|k| k == key)
        }
        fn stolen_keys(&self) -> Vec<String> {
            self.stolen.clone()
        }
        fn send_stolen(&mut self, keys: &[&str], down: bool) -> bool {
            self.log.push(format!("stolen {} {}", keys.join(","), if down { "down" } else { "up" }));
            true
        }
    }

    fn machine() -> ShortcutMachine {
        ShortcutMachine::new(Shortcuts::parse(KEYS.as_bytes()).unwrap().mode_bindings("campaign"))
    }

    /// One key event with `down` keys down and `held` modifiers; returns the target's log since.
    fn event(m: &mut ShortcutMachine, t: &mut Target, down: &[&str], held: Held) -> Vec<String> {
        let is_down = |k: &str| down.contains(&k);
        m.key_event(&is_down, held, 0.5, t);
        std::mem::take(&mut t.log)
    }

    const NONE: Held = Held { ctrl: false, shift: false, alt: false };
    const CTRL: Held = Held { ctrl: true, shift: false, alt: false };

    #[test]
    fn a_binding_presses_once_and_releases_on_the_next_event() {
        let (mut m, mut t) = (machine(), Target::default());
        assert_eq!(event(&mut m, &mut t, &["RETURN"], NONE), ["press button_end_turn end_turn 0.5"]);
        // The first auto-repeat releases it; later repeats do nothing until the key goes up.
        assert_eq!(event(&mut m, &mut t, &["RETURN"], NONE), ["release button_end_turn end_turn"]);
        assert!(event(&mut m, &mut t, &["RETURN"], NONE).is_empty());
        assert!(event(&mut m, &mut t, &[], NONE).is_empty());
        // Pressed and let go: press on the down, release on the up.
        assert_eq!(event(&mut m, &mut t, &["RETURN"], NONE), ["press button_end_turn end_turn 0.5"]);
        assert_eq!(event(&mut m, &mut t, &[], NONE), ["release button_end_turn end_turn"]);
    }

    #[test]
    fn a_repeating_binding_presses_on_every_event() {
        let (mut m, mut t) = (machine(), Target::default());
        let shift = Held { shift: true, ..NONE };
        assert_eq!(event(&mut m, &mut t, &["W"], NONE), ["press camera step_fwd 0.5"]);
        assert_eq!(event(&mut m, &mut t, &["W"], shift), ["press camera step_fwd 0.5"]);
        // Another key's event while W is held presses it again; CTRL does not match ALLOW_SHIFT.
        assert_eq!(event(&mut m, &mut t, &["W", "L"], NONE), ["press camera step_fwd 0.5", "press button_lists show_lists 0.5"]);
        assert_eq!(event(&mut m, &mut t, &["W", "L"], CTRL), ["release camera step_fwd", "release button_lists show_lists"]);
    }

    #[test]
    fn a_modifier_pressed_after_the_key_fires_the_binding() {
        let (mut m, mut t) = (machine(), Target::default());
        assert!(event(&mut m, &mut t, &["S"], NONE).is_empty());
        assert_eq!(event(&mut m, &mut t, &["S", "LCONTROL"], CTRL), ["press root quick_save 0.5"]);
    }

    #[test]
    fn locks_disabled_and_missing_components() {
        let mut t = Target { locked: true, ..Target::default() };
        let mut m = machine();
        // Locked: only an ignore_lock func fires.
        assert!(event(&mut m, &mut t, &["RETURN"], NONE).is_empty());
        assert_eq!(event(&mut m, &mut t, &["S"], CTRL), ["press root quick_save 0.5"]);
        let mut t = Target { disabled: vec!["button_end_turn"], missing: vec!["button_lists"], ..Target::default() };
        // The held quick save is released; a disabled or missing component is not pressed.
        assert_eq!(event(&mut m, &mut t, &["RETURN", "L"], NONE), ["release root quick_save"]);
        // A pressed binding whose component went away is let go without a release call.
        t.missing.clear();
        assert_eq!(event(&mut m, &mut t, &["L"], NONE), ["press button_lists show_lists 0.5"]);
        t.missing.push("button_lists");
        assert!(event(&mut m, &mut t, &[], NONE).is_empty());
    }

    #[test]
    fn a_stolen_key_goes_to_its_stealer() {
        let (mut m, mut t) = (machine(), Target::default());
        // A bound key: the binding sends the stealer the key down, then up on the next event.
        t.stolen.push("RETURN".into());
        assert_eq!(event(&mut m, &mut t, &["RETURN"], NONE), ["stolen RETURN down"]);
        assert_eq!(event(&mut m, &mut t, &["RETURN"], NONE), ["stolen RETURN up"]);
        assert!(event(&mut m, &mut t, &[], NONE).is_empty());
        // A stolen repeating key no longer repeats.
        t.stolen.push("W".into());
        assert_eq!(event(&mut m, &mut t, &["W"], NONE), ["stolen W down"]);
        assert_eq!(event(&mut m, &mut t, &[], NONE), ["stolen W up"]);
        // A key no binding has gets the stealer's own binding, without modifiers.
        t.stolen.push("ESCAPE".into());
        assert_eq!(event(&mut m, &mut t, &["ESCAPE"], NONE), ["stolen ESCAPE down"]);
        assert_eq!(event(&mut m, &mut t, &[], NONE), ["stolen ESCAPE up"]);
        assert!(event(&mut m, &mut t, &["ESCAPE"], CTRL).is_empty());
        // Given back: the bindings work again, and the stealer's own binding is gone.
        t.stolen.clear();
        assert_eq!(event(&mut m, &mut t, &["RETURN"], NONE), ["press button_end_turn end_turn 0.5"]);
        assert_eq!(event(&mut m, &mut t, &["ESCAPE"], NONE), ["release button_end_turn end_turn"]);
        assert_eq!(m.slots.len(), 4);
    }
}

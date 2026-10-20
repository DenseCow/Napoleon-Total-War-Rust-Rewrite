//! The original's key names for the keyboard shortcuts of `text/default_keys.xml`
//! (`ntw_formats::default_keys`): a physical key → the name the exe's key table gives it.

use bevy::prelude::*;
use ntw_formats::default_keys::Held;

/// The exe's name of a physical key (CONFIRMED: its key table `0x0139FEC8`, 143 entries of
/// {code, DirectInput scan code, name}, which the keys file's parser `0x00E20290` searches by
/// name, case-sensitive). Bevy's `KeyCode` is the physical key, as the exe's DirectInput scan
/// codes are. None for a key Bevy has no code for (AT, COLON, UNDERLINE, KANJI, STOP, AX,
/// ABNT_C2: Japanese and Brazilian keyboard keys) and for Bevy codes the table lacks.
pub(crate) fn exe_key_name(k: KeyCode) -> Option<&'static str> {
    use KeyCode::*;
    Some(match k {
        Escape => "ESCAPE",
        Digit1 => "1",
        Digit2 => "2",
        Digit3 => "3",
        Digit4 => "4",
        Digit5 => "5",
        Digit6 => "6",
        Digit7 => "7",
        Digit8 => "8",
        Digit9 => "9",
        Digit0 => "0",
        Minus => "MINUS",
        Equal => "EQUALS",
        Backspace => "BACK",
        Tab => "TAB",
        KeyQ => "Q",
        KeyW => "W",
        KeyE => "E",
        KeyR => "R",
        KeyT => "T",
        KeyY => "Y",
        KeyU => "U",
        KeyI => "I",
        KeyO => "O",
        KeyP => "P",
        BracketLeft => "LBRACKET",
        BracketRight => "RBRACKET",
        Enter => "RETURN",
        ControlLeft => "LCONTROL",
        KeyA => "A",
        KeyS => "S",
        KeyD => "D",
        KeyF => "F",
        KeyG => "G",
        KeyH => "H",
        KeyJ => "J",
        KeyK => "K",
        KeyL => "L",
        Semicolon => "SEMICOLON",
        Quote => "APOSTROPHE",
        Backquote => "GRAVE",
        ShiftLeft => "LSHIFT",
        Backslash => "BACKSLASH",
        KeyZ => "Z",
        KeyX => "X",
        KeyC => "C",
        KeyV => "V",
        KeyB => "B",
        KeyN => "N",
        KeyM => "M",
        Comma => "COMMA",
        Period => "PERIOD",
        Slash => "SLASH",
        ShiftRight => "RSHIFT",
        NumpadMultiply => "MULTIPLY",
        AltLeft => "LALT",
        Space => "SPACE",
        CapsLock => "CAPITAL",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        NumLock => "NUMLOCK",
        ScrollLock => "SCROLL",
        Numpad7 => "NUMPAD7",
        Numpad8 => "NUMPAD8",
        Numpad9 => "NUMPAD9",
        NumpadSubtract => "SUBTRACT",
        Numpad4 => "NUMPAD4",
        Numpad5 => "NUMPAD5",
        Numpad6 => "NUMPAD6",
        NumpadAdd => "ADD",
        Numpad1 => "NUMPAD1",
        Numpad2 => "NUMPAD2",
        Numpad3 => "NUMPAD3",
        Numpad0 => "NUMPAD0",
        NumpadDecimal => "DECIMAL",
        IntlBackslash => "OEM_102",
        F11 => "F11",
        F12 => "F12",
        F13 => "F13",
        F14 => "F14",
        F15 => "F15",
        KanaMode => "KANA",
        IntlRo => "ABNT_C1",
        Convert => "CONVERT",
        NonConvert => "NOCONVERT",
        IntlYen => "YEN",
        NumpadEqual => "NUMPADEQUALS",
        MediaTrackPrevious => "PREVTRACK",
        MediaTrackNext => "NEXTTRACK",
        NumpadEnter => "NUMPADENTER",
        ControlRight => "RCONTROL",
        AudioVolumeMute => "MUTE",
        LaunchApp2 => "CALCULATOR",
        MediaPlayPause => "PLAYPAUSE",
        MediaStop => "MEDIASTOP",
        AudioVolumeDown => "VOLUMEDOWN",
        AudioVolumeUp => "VOLUMEUP",
        BrowserHome => "WEBHOME",
        NumpadComma => "NUMPADCOMMA",
        NumpadDivide => "DIVIDE",
        PrintScreen => "SYSRQ",
        AltRight => "RALT",
        Pause => "PAUSE",
        Home => "HOME",
        ArrowUp => "UP",
        PageUp => "PAGE_UP",
        ArrowLeft => "LEFT",
        ArrowRight => "RIGHT",
        End => "END",
        ArrowDown => "DOWN",
        PageDown => "PAGE_DOWN",
        Insert => "INSERT",
        Delete => "DELETE",
        SuperLeft => "LWIN",
        SuperRight => "RWIN",
        ContextMenu => "APPS",
        Power => "POWER",
        Sleep => "SLEEP",
        WakeUp => "WAKE",
        BrowserSearch => "WEBSEARCH",
        BrowserFavorites => "WEBFAVORITES",
        BrowserRefresh => "WEBREFRESH",
        BrowserStop => "WEBSTOP",
        BrowserForward => "WEBFORWARD",
        BrowserBack => "WEBBACK",
        LaunchApp1 => "MYCOMPUTER",
        LaunchMail => "MAIL",
        MediaSelect => "MEDIASELECT",
        _ => return None,
    })
}

/// The keys down at each of a frame's key events, as the exe's frame key snapshot (`0x00E20190`)
/// has them when a key event runs the bindings (`0x00EEE250`). Bevy's [`ButtonInput`] only holds
/// the frame's end state, so [`Self::rewind`] winds it back to the frame's start and
/// [`Self::apply`] replays the events in order (a key pressed and let go in one frame is still
/// down at its press).
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct KeysDown(pub Vec<KeyCode>);

impl KeysDown {
    /// Winds back to the keys down before the frame's `events` (key, pressed): the keys down at
    /// the end of the frame without an event, and each key with events in the opposite of its
    /// first event's state. Reuses this set's buffer (no allocation once it has grown).
    pub(crate) fn rewind(&mut self, end: impl Iterator<Item = KeyCode>, events: &[(KeyCode, bool)]) {
        self.0.clear();
        self.0.extend(end.filter(|k| !events.iter().any(|(e, _)| e == k)));
        for (i, &(k, pressed)) in events.iter().enumerate() {
            if !pressed && !events[..i].iter().any(|(e, _)| *e == k) {
                self.0.push(k);
            }
        }
    }

    /// One key event: `key` goes down or up.
    pub(crate) fn apply(&mut self, key: KeyCode, pressed: bool) {
        if !pressed {
            self.0.retain(|k| *k != key);
        } else if !self.0.contains(&key) {
            self.0.push(key);
        }
    }

    /// The exe's names of the keys down ([`exe_key_name`]) into `out` (emptied first; no
    /// allocation once it has grown).
    pub(crate) fn names_into(&self, out: &mut Vec<&'static str>) {
        out.clear();
        out.extend(self.0.iter().filter_map(|k| exe_key_name(*k)));
    }

    /// The modifier keys held, as the exe's modifier test `0x00EC53A0` reads them (CONFIRMED: CTRL
    /// is LCONTROL or RCONTROL, SHIFT LSHIFT or RSHIFT, ALT LALT or RALT).
    pub(crate) fn modifiers(&self) -> Held {
        let any = |a: KeyCode, b: KeyCode| self.0.contains(&a) || self.0.contains(&b);
        Held {
            ctrl: any(KeyCode::ControlLeft, KeyCode::ControlRight),
            shift: any(KeyCode::ShiftLeft, KeyCode::ShiftRight),
            alt: any(KeyCode::AltLeft, KeyCode::AltRight),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_keys_get_the_exes_names() {
        assert_eq!(exe_key_name(KeyCode::KeyS), Some("S"));
        assert_eq!(exe_key_name(KeyCode::Escape), Some("ESCAPE"));
        assert_eq!(exe_key_name(KeyCode::ControlLeft), Some("LCONTROL"));
        assert_eq!(exe_key_name(KeyCode::NumpadEnter), Some("NUMPADENTER"));
        assert_eq!(exe_key_name(KeyCode::PageUp), Some("PAGE_UP"));
        assert_eq!(exe_key_name(KeyCode::Fn), None);
    }

    #[test]
    fn modifiers_are_either_side() {
        let keys = KeysDown(vec![KeyCode::ControlRight, KeyCode::AltLeft]);
        assert_eq!(keys.modifiers(), Held { ctrl: true, shift: false, alt: true });
    }

    /// A frame's events replay from the frame's start: a key tapped within the frame is down at its
    /// press, a key let go is down before its release, an untouched key stays down throughout.
    #[test]
    fn a_frames_key_events_replay_from_its_start() {
        use KeyCode::{ControlLeft as C, KeyS as S, KeyW as W};
        // End of frame: W and CTRL down. Events: S down, S up, CTRL up, CTRL down.
        let events = [(S, true), (S, false), (C, false), (C, true)];
        let mut keys = KeysDown::default();
        keys.rewind([W, C].into_iter(), &events);
        assert_eq!(keys, KeysDown(vec![W, C]));
        let mut seen = Vec::new();
        for (k, p) in events {
            keys.apply(k, p);
            let mut names = Vec::new();
            keys.names_into(&mut names);
            seen.push(names);
        }
        assert_eq!(seen, [vec!["W", "LCONTROL", "S"], vec!["W", "LCONTROL"], vec!["W"], vec!["W", "LCONTROL"]]);
    }
}

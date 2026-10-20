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

/// The modifier keys held, as the exe's modifier test `0x00EC53A0` reads them (CONFIRMED: CTRL is
/// LCONTROL or RCONTROL, SHIFT LSHIFT or RSHIFT, ALT LALT or RALT).
pub(crate) fn held_modifiers(keys: &ButtonInput<KeyCode>) -> Held {
    Held {
        ctrl: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
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
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::ControlRight);
        keys.press(KeyCode::AltLeft);
        assert_eq!(held_modifiers(&keys), Held { ctrl: true, shift: false, alt: true });
    }
}

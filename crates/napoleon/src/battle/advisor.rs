//! The battle advisor (BATTLE_FIDELITY.md §59.8): the advice the battle scripts show
//! (`battle:show_advisor_message`, `0x006646D0`), its speech, and the advice window it goes to.
//!
//! - The scripts (`super::scripts`) call [`Advisor::show`] / [`Advisor::close`]; the speech starts
//!   at once ([`crate::audio::AdviceSpeech`]), and `battle:advice_finished()` asks whether it
//!   still plays (`0x0062A350`).
//! - The HUD (`super::hud`) drains [`Advisor::take_for_hud`] into the original advice window
//!   (`ui/common ui/advice_interface`, made by root.lua's `ShowPersistentPanels`), and hands the
//!   window's buttons back ([`Advisor::repeat`], [`Advisor::close`], the audio and text modes).

use bevy::prelude::*;
use ntw_data::GameDatabase;
use ntw_formats::loc::Localisation;
use ntw_script::ui::battle::AdviceView;

use crate::audio::AdviceSpeech;

/// What the advice window still has to be told, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum ToHud {
    /// `UpdateWithNewAdvice` with the advice ([`advice_view`] of this key and message) and the
    /// text mode.
    Show { key: String, message: String, show_text: bool },
    /// `CurrentAdviceDismissed`.
    Dismissed,
    /// `AdviceTextSettingChanged(on)`.
    TextSetting(bool),
}

/// The engine's advisor for one battle (made with both modes on, `0x00709BB0`: text byte +0 and
/// audio byte +1 both 1). Reset on entering a battle.
#[derive(Resource, Debug)]
pub struct Advisor {
    to_hud: Vec<ToHud>,
    /// The key of the advice shown last (its speech file), for [`Advisor::repeat`].
    current: Option<String>,
    text_on: bool,
    audio_on: bool,
}

impl Default for Advisor {
    fn default() -> Self {
        Self { to_hud: Vec::new(), current: None, text_on: true, audio_on: true }
    }
}

impl Advisor {
    /// `battle:show_advisor_message(key, message)` (`0x006646D0` → `0x00E1A970`): the advice goes
    /// to the window and its speech (`advisor\<key>.mp3`, when the key is not empty) starts.
    /// INFERRED: the audio mode off keeps the speech from starting.
    pub fn show(&mut self, speech: &mut AdviceSpeech, key: &str, message: &str) {
        self.to_hud.push(ToHud::Show { key: key.to_owned(), message: message.to_owned(), show_text: self.text_on });
        self.current = Some(key.to_owned());
        if key.is_empty() || !self.audio_on {
            speech.stop();
        } else {
            speech.play(key);
        }
    }

    /// `battle:close_advisor()` and the window's close button (`DismissCurrentAdvice`), both
    /// `0x0076DB70`: the speech stops and the window is told.
    pub fn close(&mut self, speech: &mut AdviceSpeech) {
        speech.stop();
        self.to_hud.push(ToHud::Dismissed);
    }

    /// `BattleUI.RepeatAdvice()`: the current advice's speech again. PROVISIONAL: the exe's
    /// repeat (`0x007AE8E0`) is not traced; only the speech is replayed.
    pub fn repeat(&mut self, speech: &mut AdviceSpeech) {
        if let Some(key) = self.current.as_deref().filter(|k| !k.is_empty())
            && self.audio_on
        {
            speech.play(key);
        }
    }

    /// `BattleUI.ChangeAdviceAudioMode(on)` (`0x0075C370`, CONFIRMED: it only sets the mode).
    pub fn set_audio_mode(&mut self, on: bool) {
        self.audio_on = on;
    }

    /// `BattleUI.ChangeAdviceTextMode(on)` (`0x0075C380`, CONFIRMED): the mode, told to the window.
    pub fn set_text_mode(&mut self, on: bool) {
        self.text_on = on;
        self.to_hud.push(ToHud::TextSetting(on));
    }

    /// What the window has to be told since the last call.
    pub fn take_for_hud(&mut self) -> Vec<ToHud> {
        std::mem::take(&mut self.to_hud)
    }
}

/// The advice window's content for a script's `show_advisor_message(key, message)`
/// (`0x006646D0`, CONFIRMED): a non-empty key with a `battle_script_strings` row gives that row's
/// text (the localisation `battle_script_strings_string_<key>`, else the row's own) and speaker;
/// otherwise `message` with no speaker. The title and subtitle stay empty, the advice is not
/// locatable, it has audio when the key is not empty (`0x005AB820` asks `0x00E0FA40`, the key),
/// and its portrait is the speaker's `advisors` row (`0x00E0E3E0`; none without a row).
pub fn advice_view(db: &GameDatabase, loc: &Localisation, key: &str, message: &str) -> AdviceView {
    let row = if key.is_empty() { None } else { db.battle_script_strings.get(key) };
    let (text, speaker) = match row {
        Some(r) => {
            let text = loc.get(&format!("battle_script_strings_string_{}", r.key)).unwrap_or(&r.string);
            (text.to_owned(), r.speaker.as_str())
        }
        None => (message.to_owned(), ""),
    };
    let icon_path = db.advisors.get(speaker).map(|a| a.icon_path.clone()).filter(|p| !p.is_empty());
    AdviceView { title: String::new(), subtitle: String::new(), text, locatable: false, has_audio: !key.is_empty(), icon_path }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_data::{AdvisorRecord, BattleScriptString, Table};
    use ntw_formats::loc::{LocEntry, LocFile};

    fn db() -> GameDatabase {
        let mut db = GameDatabase::test_fixture();
        db.battle_script_strings = Table::from_rows(
            1,
            vec![BattleScriptString { key: "CA.Intro_1".into(), string: "English text".into(), speaker: "Napoleon".into() }],
        );
        db.advisors = Table::from_rows(1, vec![AdvisorRecord { key: "Napoleon".into(), icon_path: "data\\napoleon.tga".into() }]);
        db
    }

    #[test]
    fn a_keyed_advice_shows_its_rows_localised_text_and_its_speakers_portrait() {
        let mut loc = Localisation::new();
        let entry = LocEntry { key: "battle_script_strings_string_CA.Intro_1".into(), text: "Localised text".into(), flag: false };
        loc.add(&LocFile { version: 1, entries: vec![entry] });
        let v = advice_view(&db(), &loc, "CA.Intro_1", "ignored");
        assert_eq!(v.text, "Localised text");
        assert_eq!(v.icon_path.as_deref(), Some("data\\napoleon.tga"));
        assert!(v.has_audio && !v.locatable && v.title.is_empty() && v.subtitle.is_empty());
        // Without the localisation: the row's own text.
        assert_eq!(advice_view(&db(), &Localisation::new(), "CA.Intro_1", "").text, "English text");
    }

    #[test]
    fn an_unknown_key_shows_the_message_and_still_names_its_speech() {
        let v = advice_view(&db(), &Localisation::new(), "CA.Nothing", "Fallback");
        assert_eq!((v.text.as_str(), v.icon_path, v.has_audio), ("Fallback", None, true));
        let v = advice_view(&db(), &Localisation::new(), "", "Text only");
        assert_eq!((v.text.as_str(), v.has_audio), ("Text only", false));
    }

    #[test]
    fn the_advice_speech_follows_the_audio_mode_and_counts_as_playing_at_once() {
        let mut a = Advisor::default();
        let mut speech = AdviceSpeech::default();
        a.show(&mut speech, "CA.Intro_1", "");
        assert!(speech.playing(), "a speech just asked for counts as playing");
        assert_eq!(a.take_for_hud(), vec![ToHud::Show { key: "CA.Intro_1".into(), message: String::new(), show_text: true }]);
        a.close(&mut speech);
        assert!(!speech.playing());
        assert_eq!(a.take_for_hud(), vec![ToHud::Dismissed]);
        a.repeat(&mut speech);
        assert!(speech.playing(), "repeat plays the last advice again");
        a.set_audio_mode(false);
        a.set_text_mode(false);
        a.show(&mut speech, "CA.Intro_2", "");
        assert!(!speech.playing(), "no speech with the audio mode off");
        assert_eq!(
            a.take_for_hud(),
            vec![ToHud::TextSetting(false), ToHud::Show { key: "CA.Intro_2".into(), message: String::new(), show_text: false }]
        );
    }
}

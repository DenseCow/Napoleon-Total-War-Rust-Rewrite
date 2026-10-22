//! Campaign event messages: the `message_event_strings` row a message shows (CAMPAIGN_FIDELITY.md
//! "Allies called into a war" → "The human ally's message", CONFIRMED by disassembly).
//!
//! The exe's message queuer (`0x009D3250`) loads the table lazily (`0x00E225F0`), picks the row of the
//! message's event for the receiving faction's culture (its subculture's culture, `0x008F3490` → +0x10)
//! through the culture lookup `0x00F54F80` ([`CampaignRules::culture_row`](super::rules::CampaignRules::culture_row)),
//! builds the message from it (`0x0099A4B0`; the record builder `0x00F57390` → `0x00F43090`) and posts
//! it; no row posts nothing. The UI turns the message into the `details` table of the root script's
//! `ShowMessage` (`0x00A22B30`); ntw_script `ui/campaign/ally_attacked.rs` builds that table.

use super::ids::FactionId;
use super::world::CampaignModel;

/// The event of the message a human ally gets when an AI declares war on the faction it is allied
/// to (message type 0xD, posted by `0x009CA900`; its row selects the `ally_attacked` layout).
pub const ALLY_ATTACKED: &str = "ally_attacked";

/// One `message_event_strings` row (the DB record `MessageEventStringRecord`), as the message reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageEventRow {
    /// #0: the message event (the exe resolves it to a message type id, `0x00F54EC0`).
    pub event: String,
    /// #1: the culture the row is for.
    pub culture: String,
    /// #2: the text's loc key suffix ([`text_key`](Self::text_key)).
    pub text: String,
    /// #3: the picture under `ui/EventPics/<culture folder>/`, the message's `Image` (CONFIRMED by
    /// disassembly: the table's row reader `0x00F3B110` (the callback `0x00F58EB0` the loader
    /// `0x00F27570` calls per row) reads the file's columns into the builder at +0x00, +0x0C, +0x24,
    /// +0x18, +0x3C, +0x54, +0x55, +0x30, +0x48 in that order, so #3 is builder +0x18; the record
    /// builder `0x00F43090` resolves builder +0x18 through `0x00F840E0` into record +0x1C, which the
    /// details converter `0x00A22B30` hands out as `Image` at `0x00A22E79`).
    pub picture: String,
    /// #4: the `event_message` layout the UI script selects.
    pub layout: String,
    /// #5: the message needs an answer (`RequiresResponse`, record +0x58).
    pub requires_response: bool,
    /// #6: the message opens by itself (`AutoShow`, record +0x59).
    pub auto_show: bool,
    /// #7: the icon file under `ui/EventIcons/` ([`icon_path`](Self::icon_path)).
    pub icon: String,
    /// #8: a second picture name (builder +0x48, copied to record +0x4C by `0x00F43090` at
    /// `0x00F4316E`); the details converter `0x00A22B30` does not read it.
    pub image: String,
}

impl MessageEventRow {
    /// The loc key of the message's title, `message_event_strings_title_<event><culture>` (CONFIRMED:
    /// `0x00F57390` formats `"message_event_strings_title_%S"`, `0x013D409C`, with the row's event and
    /// culture keys).
    pub fn title_key(&self) -> String {
        format!("message_event_strings_title_{}{}", self.event, self.culture)
    }

    /// The loc key of the message's text, `message_event_text_text_<#2>` (CONFIRMED: `0x00F57390`
    /// formats `"message_event_text_text_%S"`, `0x013D4080`, with the row's text column). The message
    /// shows that loc string through [`message_text`].
    pub fn text_key(&self) -> String {
        format!("message_event_text_text_{}", self.text)
    }

    /// The message's `Icon`: `data/ui/EventIcons/<#7>` (CONFIRMED: `0x00F43090` formats
    /// `"data/ui/EventIcons/%S"`, `0x013D40CC`).
    pub fn icon_path(&self) -> String {
        format!("data/ui/EventIcons/{}", self.icon)
    }
}

/// The message's text from the loc string of [`MessageEventRow::text_key`]: a text that is exactly
/// one line break is empty, any other text is kept. CONFIRMED by disassembly: `0x00F57390` builds
/// `"\n"` (the ANSI string `0x013139B4`, bytes 0A 00) at `0x00F574C9`, compares the looked-up text
/// with it (`0x004F0F40` → `0x004EFB20`: equal when `CompareUniStrings` gives 0) and, when equal,
/// hands the record builder `0x00F43090` the empty static string `0x0164EB58` (made once by the
/// default constructor `0x004F0910`) in its place (`CMOVZ` at `0x00F574F4`).
pub fn message_text(text: &str) -> &str {
    if text == "\n" { "" } else { text }
}

/// The folder the message's picture is looked up under (`0x00F43090` passes `"ui/EventPics"`,
/// `0x013D40BC`, to the culture picture resolver `0x00F840E0`; the found path gets `"data/"`).
pub const EVENT_PICTURE_DIR: &str = "ui/EventPics";

impl CampaignModel {
    /// The `message_event_strings` row a message of `event` to `faction` shows: of the event's rows,
    /// the one the culture lookup picks for the faction's culture (`0x009D3250` → `0x00F54F80`, see the
    /// module docs), or `None` (the exe then posts nothing). A query.
    pub fn message_event_row(&self, event: &str, faction: FactionId) -> Option<&MessageEventRow> {
        let culture = self.world.factions.get(&faction).and_then(|f| self.rules.faction_cultures.get(&f.key)).map(String::as_str);
        self.rules.culture_row(self.rules.message_events.iter().filter(|r| r.event == event), culture, |r| r.culture.as_str())
    }

    /// The `Image` of a message showing `row`: the culture picture resolver (`0x00F840E0`, CONFIRMED by
    /// disassembly) on the row's own culture (`0x00F57390` looks the row's culture key up in the
    /// `cultures` table and hands that record to `0x00F43090`), under [`EVENT_PICTURE_DIR`]: the first of
    /// `<dir>/<culture>/<picture>` and `<dir>/<culture's cultures #2>/<picture>` that `exists` (the exe
    /// asks its file system), else `<dir>/european/<picture>` whether it exists or not; then `"data/"`
    /// in front (`0x01357CCC`).
    pub fn message_picture_path(&self, row: &MessageEventRow, exists: impl Fn(&str) -> bool) -> String {
        let alternate = self.rules.culture_alternates.get(&row.culture).map_or("", String::as_str);
        let path = [row.culture.as_str(), alternate]
            .into_iter()
            .map(|folder| format!("{EVENT_PICTURE_DIR}/{folder}/{}", row.picture))
            .find(|p| exists(p))
            .unwrap_or_else(|| format!("{EVENT_PICTURE_DIR}/{}/{}", super::rules::FALLBACK_CULTURE, row.picture));
        format!("data/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::message_text;

    /// `0x00F57390`: a text of exactly "\n" becomes the empty string; any other text, a text holding
    /// a line break among other characters included, is kept.
    #[test]
    fn a_text_of_one_line_break_is_empty() {
        assert_eq!(message_text("\n"), "");
        assert_eq!(message_text(""), "");
        assert_eq!(message_text("Our ally is attacked.\n"), "Our ally is attacked.\n");
        assert_eq!(message_text("\n\n"), "\n\n");
        // The two characters backslash and n are not a line break.
        assert_eq!(message_text(r"\n"), r"\n");
    }
}

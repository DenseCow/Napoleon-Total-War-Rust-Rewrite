//! Everything the sound system reads at start-up, loaded from a [`Vfs`] in one go.
//!
//! - `sounds_packed\sound_events` and `sounds_packed\sound_bank_database`: the data the
//!   original game plays from (required);
//! - the shipped sources `sounds\events\*.csv`, `sounds\banks\*.xml` and
//!   `sounds\sound_settings.xml`: names only (optional; without them everything still works
//!   by number).
//!
//! Everything goes through the Vfs, so mod packs that replace any of these files (or any
//! sound file) are picked up exactly as the original picks them up.

use std::collections::HashMap;

use super::bank_xml::{BankVocabulary, BANK_XML_DIR};
use super::banks::{SoundBankDatabase, SETTING_COUNT};
use super::events::SoundEvents;
use super::names::{EventNames, CSV_DIR};
use super::slots::slot_by_name;
use crate::pack::Vfs;

/// The sound settings XML (names for the settings values).
pub const SETTINGS_XML: &str = r"sounds\sound_settings.xml";

/// Why the sound data could not be loaded.
#[derive(Debug)]
pub enum SoundLibraryError {
    Missing(&'static str, String),
    Events(super::events::SoundEventsError),
    Banks(super::banks::SoundBankError),
}

impl std::fmt::Display for SoundLibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(p, e) => write!(f, "{p}: {e}"),
            Self::Events(e) => write!(f, "{e}"),
            Self::Banks(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SoundLibraryError {}

/// The loaded sound data.
#[derive(Debug, Clone)]
pub struct SoundLibrary {
    pub events: SoundEvents,
    pub banks: SoundBankDatabase,
    pub names: EventNames,
    pub vocabulary: BankVocabulary,
    /// Settings by name (from `sound_settings.xml` element order; only names whose XML value
    /// equals the packed value are kept, so a stale XML cannot mislabel a number).
    pub settings: HashMap<String, f32>,
}

impl SoundLibrary {
    /// Loads everything from the Vfs.
    pub fn load(vfs: &Vfs) -> Result<Self, SoundLibraryError> {
        let read = |p: &'static str| vfs.read(p).map_err(|e| SoundLibraryError::Missing(p, e.to_string()));
        let events = SoundEvents::read(&read(SoundEvents::PATH)?).map_err(SoundLibraryError::Events)?;
        let banks = SoundBankDatabase::read(&read(SoundBankDatabase::PATH)?).map_err(SoundLibraryError::Banks)?;
        let texts = |dir: &str, ext: &str| -> Vec<(String, String)> {
            vfs.list(dir)
                .into_iter()
                .filter(|p| p.to_ascii_lowercase().ends_with(ext))
                .filter_map(|p| vfs.read(p).ok().map(|b| (p.to_owned(), crate::xml::decode_text(&b))))
                .collect()
        };
        let csvs = texts(CSV_DIR, ".csv");
        let names = if csvs.is_empty() { EventNames::from_packed(&events) } else { EventNames::from_csvs(&events, &csvs) };
        let vocabulary = BankVocabulary::build(&banks, &names, &texts(BANK_XML_DIR, ".xml"));
        let settings = vfs.read(SETTINGS_XML).map(|b| name_settings(&banks.settings, &crate::xml::decode_text(&b))).unwrap_or_default();
        Ok(Self { events, banks, names, vocabulary, settings })
    }

    /// The event in a built-in slot, by slot name (e.g. `"MUSIC_FRONTEND"`).
    pub fn slot_event(&self, slot_name: &str) -> Option<usize> {
        self.events.slot_event(slot_by_name(slot_name)?)
    }

    /// A named event (UI component names, unit voices, ...) or, failing that, an event whose
    /// CSV name matches. Case-insensitive; the first match in file order.
    pub fn event_by_name(&self, name: &str) -> Option<usize> {
        self.names.find(name).first().copied()
    }

    /// A setting by its `sound_settings.xml` name.
    pub fn setting(&self, name: &str) -> Option<f32> {
        self.settings.get(&name.to_ascii_uppercase()).copied()
    }
}

/// Names the packed settings values from the XML's element order.
fn name_settings(values: &[f32], xml: &str) -> HashMap<String, f32> {
    let mut out = HashMap::new();
    let mut i = 0;
    for line in xml.lines() {
        let l = line.trim();
        if l.starts_with("<!--") || l.starts_with("<?") || l.starts_with("</") {
            continue;
        }
        let Some(rest) = l.strip_prefix('<') else { continue };
        let Some((tag, after)) = rest.split_once('>') else { continue };
        let Some((text, _)) = after.split_once("</") else { continue };
        if i >= SETTING_COUNT.min(values.len()) {
            break;
        }
        if let Ok(v) = text.trim().parse::<f32>()
            && (v - values[i]).abs() <= 1e-4 * v.abs().max(1.0)
        {
            out.entry(tag.trim().to_ascii_uppercase()).or_insert(values[i]);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_names_follow_xml_order() {
        let xml = "<?xml?>\n<dataroot>\n<!-- c -->\n<SS_A>3.00</SS_A>\n<SS_B>7.00</SS_B>\n<ss_c>1</ss_c>\n</dataroot>";
        let m = name_settings(&[3.0, 9.0, 1.0], xml);
        assert_eq!(m.get("SS_A"), Some(&3.0));
        assert_eq!(m.get("SS_B"), None, "value differs from the packed one");
        assert_eq!(m.get("SS_C"), Some(&1.0));
    }
}

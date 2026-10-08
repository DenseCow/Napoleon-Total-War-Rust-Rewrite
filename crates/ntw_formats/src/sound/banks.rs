//! `sounds_packed\sound_bank_database`: the sound settings and the sound banks.
//!
//! A *sound bank* answers "which event plays for this situation?". Each bank entry
//! names one event (an index into [`super::SoundEvents::events`]) and lists the
//! condition values it applies to (gun type, shot type, audio distance, music state,
//! subculture, ...). The source of each bank is one `sounds\banks\sound_bank_*.xml`
//! file; the packed form drops all names and stores only numbers.
//!
//! Layout (AUDIO_FORMAT.md §4; exe reader `0x00DF3640`, CONFIRMED by an exact-EOF
//! parse of the shipped file):
//! ```text
//! u32 header                                0x3F800000 (meaning UNKNOWN)
//! 154 x f32 settings                        sound_settings.xml values, exe order (see [`SETTING_COUNT`])
//! for bank type t in 0..109:                only the types listed in BANK_LAYOUTS have data
//!   u32 n; n x entry
//! entry = u32 event, then one list per condition: u32 m; m x (u32 | u8)
//! end of file
//! ```

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// Number of settings values. CONFIRMED (`0x00DF3640` reads 0x9A values).
pub const SETTING_COUNT: usize = 154;
/// Number of bank type ids the reader walks. CONFIRMED (0x6D).
pub const BANK_TYPE_COUNT: usize = 109;

/// Element size of each condition list, per bank type. Types not listed have no
/// data in the file (the exe's bank factory `0x00E15D20` returns null for them).
/// CONFIRMED from the per-type entry readers (AUDIO_FORMAT.md §4.2): every list is a
/// u32 count followed by u32 values, except three lists of bytes.
pub const BANK_LAYOUTS: &[(u32, &[u8])] = &[
    (0, &[4, 4, 4, 4]),
    (1, &[4, 4, 4, 4]),
    (2, &[4, 4, 4, 4]),
    (4, &[4, 4, 4]),
    (5, &[4, 1, 1]),
    (6, &[4, 4, 4, 4]),
    (7, &[4, 4, 4, 4]),
    (8, &[4, 4, 4, 4]),
    (9, &[4, 4, 4, 4]),
    (10, &[4, 4]),
    (11, &[4, 4]),
    (12, &[4]),
    (13, &[4, 4, 4, 4, 4]),
    (14, &[4, 4, 4, 4, 4, 4, 1]),
    (15, &[4, 4, 4, 4]),
    (16, &[4, 4, 4, 4, 4, 4, 1]),
    (17, &[4, 4, 4, 4]),
    (18, &[4, 4, 4, 4]),
    (19, &[4, 4, 4, 4, 4]),
    (20, &[4]),
    (21, &[4, 4]),
    (22, &[4, 4]),
    (23, &[4, 4]),
    (24, &[4, 4]),
    (25, &[4, 4]),
    (26, &[4, 4]),
    (27, &[4, 4]),
    (28, &[4, 4]),
];

/// The condition-list element sizes of a bank type (`None` = the type has no data).
pub fn bank_layout(bank_type: u32) -> Option<&'static [u8]> {
    BANK_LAYOUTS.iter().find(|(t, _)| *t == bank_type).map(|(_, l)| *l)
}

/// One bank entry: an event and the condition values it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankEntry {
    /// Event index ([`super::SoundEvents::events`]).
    pub event: u32,
    /// One value list per condition, in the bank type's order. An empty list means
    /// "any value" (INFERRED from the XML sources, where an omitted condition matches all).
    pub conditions: Vec<Vec<u32>>,
}

/// One bank (all entries of one bank type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundBank {
    pub bank_type: u32,
    pub entries: Vec<BankEntry>,
}

/// The parsed file.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundBankDatabase {
    pub header: u32,
    pub settings: Vec<f32>,
    pub banks: Vec<SoundBank>,
}

/// Why the bank database failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundBankError {
    UnexpectedEof { offset: usize, needed: usize },
    TrailingBytes { offset: usize, count: usize },
}

impl From<ReadError> for SoundBankError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::UnexpectedEof { offset, needed: 0 },
        }
    }
}

impl fmt::Display for SoundBankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => write!(f, "sound_bank_database: need {needed} bytes at {offset:#x}"),
            Self::TrailingBytes { offset, count } => write!(f, "sound_bank_database: {count} trailing bytes at {offset:#x}"),
        }
    }
}

impl std::error::Error for SoundBankError {}

impl SoundBankDatabase {
    /// The path the exe reads.
    pub const PATH: &'static str = r"sounds_packed\sound_bank_database";

    /// Parses the file. It must end exactly at the end of the data.
    pub fn read(bytes: &[u8]) -> Result<Self, SoundBankError> {
        let mut c = Cursor::new(bytes);
        let header = c.u32()?;
        let mut settings = Vec::with_capacity(SETTING_COUNT);
        for _ in 0..SETTING_COUNT {
            settings.push(c.f32()?);
        }
        let mut banks = Vec::new();
        for t in 0..BANK_TYPE_COUNT as u32 {
            let Some(layout) = bank_layout(t) else { continue };
            let n = c.u32()?;
            let mut entries = Vec::with_capacity((n as usize).min(c.remaining() / 4));
            for _ in 0..n {
                let event = c.u32()?;
                let mut conditions = Vec::with_capacity(layout.len());
                for &size in layout {
                    let m = c.u32()?;
                    let mut list = Vec::with_capacity((m as usize).min(c.remaining()));
                    for _ in 0..m {
                        list.push(if size == 1 { u32::from(c.u8()?) } else { c.u32()? });
                    }
                    conditions.push(list);
                }
                entries.push(BankEntry { event, conditions });
            }
            banks.push(SoundBank { bank_type: t, entries });
        }
        if c.remaining() != 0 {
            return Err(SoundBankError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { header, settings, banks })
    }

    /// The bank of a type (`None` if the type has no data).
    pub fn bank(&self, bank_type: u32) -> Option<&SoundBank> {
        self.banks.iter().find(|b| b.bank_type == bank_type)
    }
}

impl SoundBank {
    /// Entries whose conditions all accept the given values. `query[i]` is the value
    /// for condition list `i`; `None` skips that condition. An empty list accepts any
    /// value. Entries are returned in file order.
    pub fn matching<'a>(&'a self, query: &'a [Option<u32>]) -> impl Iterator<Item = &'a BankEntry> + 'a {
        self.entries.iter().filter(move |e| {
            query.iter().enumerate().all(|(i, q)| match (q, e.conditions.get(i)) {
                (Some(v), Some(list)) => list.is_empty() || list.contains(v),
                _ => true,
            })
        })
    }
}

impl SoundBank {
    /// The entry the game plays for a query: among the [matching](Self::matching) entries,
    /// the most specific one (the most queried conditions met by a non-empty list); ties
    /// go to the earlier entry. PROVISIONAL rule (INFERRED): every bank starts with a
    /// catch-all entry (e.g. `SILENT` with no conditions), so "first match" would always
    /// pick it, and the original clearly plays the specific sounds.
    pub fn best_match<'a>(&'a self, query: &'a [Option<u32>]) -> Option<&'a BankEntry> {
        let mut best: Option<(usize, &BankEntry)> = None;
        for e in self.matching(query) {
            let score = query
                .iter()
                .zip(&e.conditions)
                .filter(|(q, list)| q.is_some() && !list.is_empty())
                .count();
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, e));
            }
        }
        best.map(|(_, e)| e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn parses_minimal_file() {
        let mut b = le(&[0x3F80_0000]);
        b.extend(le(&[0; SETTING_COUNT]));
        for (t, layout) in BANK_LAYOUTS {
            if *t == 12 {
                // one entry: event 7, one list [3, 4]
                b.extend(le(&[1, 7, 2, 3, 4]));
            } else if *t == 5 {
                // one entry: event 9, lists: [1], bytes [2], bytes []
                b.extend(le(&[1, 9, 1, 1, 1]));
                b.push(2);
                b.extend(le(&[0]));
                assert_eq!(layout.len(), 3);
            } else {
                b.extend(le(&[0]));
            }
        }
        let db = SoundBankDatabase::read(&b).unwrap();
        assert_eq!(db.settings.len(), SETTING_COUNT);
        let bank = db.bank(12).unwrap();
        assert_eq!(bank.entries[0], BankEntry { event: 7, conditions: vec![vec![3, 4]] });
        assert_eq!(db.bank(5).unwrap().entries[0].conditions, vec![vec![1], vec![2], vec![]]);
        assert_eq!(bank.matching(&[Some(4)]).count(), 1);
        assert_eq!(bank.matching(&[Some(5)]).count(), 0);
        assert!(db.bank(3).is_none());
        b.push(0);
        assert!(matches!(SoundBankDatabase::read(&b), Err(SoundBankError::TrailingBytes { .. })));
    }
}

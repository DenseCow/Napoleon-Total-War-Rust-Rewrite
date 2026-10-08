//! `sounds_packed\sound_events`: every sound event the game can play.
//!
//! See `analysis/audio/AUDIO_FORMAT.md` §3 for the full spec and its evidence.
//! The exe reads this file at start-up (`0x0048C1B0` -> sound manager vtable slot 2,
//! reader `0x0100FDA0`). CONFIRMED by the exe reader and an exact-EOF parse of the
//! shipped file.
//!
//! ```text
//! u32 header                         0x3F800000 in the shipped file (meaning UNKNOWN; the reader keeps it)
//! u32 n; n x { str name; f32 volume }       categories ("uncategorised", "unit_voices", "ui", ...)
//! u32 6;  6 x u32 category index            special categories (meaning UNKNOWN)
//! u32 n; n x 35 x u32                       parameter sets (35 four-byte values, see [`SoundParams`])
//! u32 n; n x {                              events
//!   u32 category
//!   str name                                ONLY when the category is a named one (see [`is_named_category`])
//!   u32 params                              index into the parameter sets
//!   u32 m; m x str file                     sound files (paths in the Vfs, '\' or '/')
//! }
//! u32 n; n x { str map; u32 m; m x emitter }   campaign-map ambience emitters (emitter = u32 id, u32 event,
//!                                           f32 min_dist, f32 max_dist, f32 x, f32 y, f32 z)
//! u32 401; 401 x u32 event index            the built-in event slots (see [`super::slots`])
//! u32 n; n x { str name; f32 volume }       movie volume multipliers
//! end of file
//! ```
//! `str` = u16 count + UTF-16LE units (the same strings as DB tables and `.loc` files).

use std::collections::HashMap;
use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// Number of four-byte values in one parameter set. CONFIRMED (`0x0100AA70` loops 0x23 times).
pub const PARAM_COUNT: usize = 35;
/// Number of built-in event slots. CONFIRMED (`0x0100FDA0` requires exactly 0x191).
pub const SLOT_COUNT: usize = 401;
/// Number of special category indices. CONFIRMED (`0x0100FDA0` requires exactly 6).
pub const SPECIAL_CATEGORY_COUNT: usize = 6;

/// Categories whose events carry their own name (the UI looks them up by component name).
/// CONFIRMED: the reader compares the category name with exactly these six strings.
pub const NAMED_CATEGORIES: [&str; 6] = ["ui", "interface", "advisor", "building_destroyed", "unit_voices", "mouse_over"];

/// `true` if events in this category have a name in the file.
pub fn is_named_category(name: &str) -> bool {
    NAMED_CATEGORIES.iter().any(|c| c.eq_ignore_ascii_case(name))
}

/// A sound category with its volume multiplier.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundCategory {
    pub name: String,
    /// Volume multiplier (1.0 for every shipped category).
    pub volume: f32,
}

/// The 35 raw four-byte values of a parameter set, in file order.
///
/// They are the 35 columns after `name, category` of the source CSVs, in the same order,
/// every one stored as an `f32` (CONFIRMED: for 3,463 CSV rows matched to their packed
/// events, 34 of the 35 columns agree on all but a handful of rows; see AUDIO_FORMAT.md §3.3).
/// Text columns are stored as small numbers: see [`Param`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundParams(pub [u32; PARAM_COUNT]);

/// The parameter columns, in file order (names from the CSV header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Param {
    /// 0..=1 (the shipped values are small: 0.07 for UI clicks, 0.3 for the front-end music).
    Volume = 0,
    /// Pitch range (semitones INFERRED; e.g. -4..2 for musket shots).
    MinPitch,
    MaxPitch,
    /// 3D distances (metres).
    MinDist,
    MaxDist,
    Falloff,
    LowPassFreqCutoff,
    Priority,
    /// 1 = loop.
    Looped,
    /// `random` = 0, `random_cycle` = 0 or 1 (both seen), 3 (one stray CSV value "linear").
    Playback,
    /// Volume group: `music` = 0, `sfx` = 1, `interface` = 3 (2 = speech, INFERRED).
    Group,
    Probability,
    /// 1 = plays without a position.
    Is2d,
    Streamed,
    FadeIn,
    FadeOut,
    StartDelay,
    RandomTriggerDelay,
    RandVolume,
    PriorityReductionByDistance,
    /// Loop region as byte offsets into the sound file (Miles' loop block; CONFIRMED unit, see
    /// `sound::loop_points`); applied only when both are non-zero and the end is inside the data.
    LoopStartBlock,
    LoopEndBlock,
    MuteWhenGameSpeedChanged,
    /// `none` = 0, `battle` = 2, `campaign` = 3, `land` = 4, `naval` = 5.
    GameModeStayInMemory,
    ProbabilityReductionSameEvents,
    /// `default` = 0.
    SpeakerOutput,
    ApplyLaunchDelayRelativeToDistance,
    ProbabilityReductionAnyEvents,
    /// `linear` = 0, `equal_power` = 1.
    FadeType,
    Reserved3,
    Reserved4,
    Reserved5,
    Reserved6,
    MaxNumberPlayingAtOnce,
    DelayBeforeCanPlayAgain,
}

impl SoundParams {
    /// A parameter as `f32`.
    pub fn get(&self, p: Param) -> f32 {
        f32::from_bits(self.0[p as usize])
    }

    /// A flag parameter (non-zero = true).
    pub fn flag(&self, p: Param) -> bool {
        self.get(p) != 0.0
    }
}

/// One sound event.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvent {
    /// Index into [`SoundEvents::categories`].
    pub category: u32,
    /// The event's own name, present only for [named categories](is_named_category).
    pub name: Option<String>,
    /// Index into [`SoundEvents::params`].
    pub params: u32,
    /// Sound file paths; the game picks among them (see the `playback` parameter).
    pub files: Vec<String>,
}

/// A positional ambience sound on a campaign map (source: `sounds<map>.csv`, columns
/// `ID, event, x pos, y pos, z pos, min_dist, max_dist`; field order in the packed file
/// CONFIRMED by matching the shipped values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CampaignEmitter {
    pub id: u32,
    /// Event index.
    pub event: u32,
    pub min_dist: f32,
    pub max_dist: f32,
    /// Campaign-map position (x, y = height, z).
    pub pos: [f32; 3],
}

/// The whole parsed file.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEvents {
    pub header: u32,
    pub categories: Vec<SoundCategory>,
    pub special_categories: [u32; SPECIAL_CATEGORY_COUNT],
    pub params: Vec<SoundParams>,
    pub events: Vec<SoundEvent>,
    /// Campaign map name (e.g. `nap_europe`) -> its ambience emitters.
    pub emitters: Vec<(String, Vec<CampaignEmitter>)>,
    /// Built-in slot -> event index.
    pub slots: Vec<u32>,
    /// Movie file name (lower case) -> volume multiplier (source `soundsmovie_volumes.csv`).
    pub movies: Vec<(String, f32)>,
}

/// Why `sound_events` failed to parse.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundEventsError {
    /// The data ended mid-record.
    UnexpectedEof { offset: usize, needed: usize },
    /// A string had an unpaired UTF-16 surrogate.
    InvalidUtf16 { offset: usize },
    /// A count that the exe requires to be fixed had another value.
    BadFixedCount { what: &'static str, expected: usize, found: u32 },
    /// An index pointed past the end of its table.
    BadIndex { what: &'static str, index: u32, len: usize },
    TrailingBytes { offset: usize, count: usize },
}

impl From<ReadError> for SoundEventsError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for SoundEventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => write!(f, "sound_events: need {needed} bytes at {offset:#x}"),
            Self::InvalidUtf16 { offset } => write!(f, "sound_events: bad UTF-16 at {offset:#x}"),
            Self::BadFixedCount { what, expected, found } => write!(f, "sound_events: {what} count {found}, expected {expected}"),
            Self::BadIndex { what, index, len } => write!(f, "sound_events: {what} index {index} out of range ({len})"),
            Self::TrailingBytes { offset, count } => write!(f, "sound_events: {count} trailing bytes at {offset:#x}"),
        }
    }
}

impl std::error::Error for SoundEventsError {}

/// Capacity hint that can't be blown up by a corrupt count.
fn cap(c: &Cursor, n: u32, min_size: usize) -> usize {
    (n as usize).min(c.remaining() / min_size.max(1))
}

fn check(i: u32, len: usize, what: &'static str) -> Result<(), SoundEventsError> {
    if (i as usize) < len { Ok(()) } else { Err(SoundEventsError::BadIndex { what, index: i, len }) }
}

impl SoundEvents {
    /// The path the exe reads.
    pub const PATH: &'static str = r"sounds_packed\sound_events";

    /// Parses the file. It must end exactly at the end of the data.
    pub fn read(bytes: &[u8]) -> Result<Self, SoundEventsError> {
        let mut c = Cursor::new(bytes);
        let header = c.u32()?;

        let n = c.u32()?;
        let mut categories = Vec::with_capacity(cap(&c, n, 6));
        for _ in 0..n {
            let name = c.utf16()?;
            let volume = c.f32()?;
            categories.push(SoundCategory { name, volume });
        }

        let n = c.u32()?;
        if n as usize != SPECIAL_CATEGORY_COUNT {
            return Err(SoundEventsError::BadFixedCount { what: "special category", expected: SPECIAL_CATEGORY_COUNT, found: n });
        }
        let mut special_categories = [0u32; SPECIAL_CATEGORY_COUNT];
        for s in &mut special_categories {
            *s = c.u32()?;
            check(*s, categories.len(), "special category")?;
        }

        let n = c.u32()?;
        let mut params = Vec::with_capacity(cap(&c, n, 4 * PARAM_COUNT));
        for _ in 0..n {
            let mut p = [0u32; PARAM_COUNT];
            for v in &mut p {
                *v = c.u32()?;
            }
            params.push(SoundParams(p));
        }

        let named: Vec<bool> = categories.iter().map(|k| is_named_category(&k.name)).collect();
        let n = c.u32()?;
        let mut events = Vec::with_capacity(cap(&c, n, 12));
        for _ in 0..n {
            let category = c.u32()?;
            check(category, categories.len(), "event category")?;
            let name = if named[category as usize] { Some(c.utf16()?) } else { None };
            let p = c.u32()?;
            check(p, params.len(), "event params")?;
            let m = c.u32()?;
            let mut files = Vec::with_capacity(cap(&c, m, 2));
            for _ in 0..m {
                files.push(c.utf16()?);
            }
            events.push(SoundEvent { category, name, params: p, files });
        }

        let n = c.u32()?;
        let mut emitters = Vec::with_capacity(cap(&c, n, 6));
        for _ in 0..n {
            let key = c.utf16()?;
            let m = c.u32()?;
            let mut list = Vec::with_capacity(cap(&c, m, 28));
            for _ in 0..m {
                let id = c.u32()?;
                let event = c.u32()?;
                let min_dist = c.f32()?;
                let max_dist = c.f32()?;
                let pos = [c.f32()?, c.f32()?, c.f32()?];
                list.push(CampaignEmitter { id, event, min_dist, max_dist, pos });
            }
            emitters.push((key, list));
        }

        let n = c.u32()?;
        if n as usize != SLOT_COUNT {
            return Err(SoundEventsError::BadFixedCount { what: "slot", expected: SLOT_COUNT, found: n });
        }
        let mut slots = Vec::with_capacity(SLOT_COUNT);
        for _ in 0..SLOT_COUNT {
            slots.push(c.u32()?);
        }

        let n = c.u32()?;
        let mut movies = Vec::with_capacity(cap(&c, n, 6));
        for _ in 0..n {
            let name = c.utf16()?;
            let v = c.f32()?;
            movies.push((name, v));
        }

        if c.remaining() != 0 {
            return Err(SoundEventsError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { header, categories, special_categories, params, events, emitters, slots, movies })
    }

    /// Category name of an event.
    pub fn category_name(&self, event: &SoundEvent) -> &str {
        &self.categories[event.category as usize].name
    }

    /// Index of the category with this name (case-insensitive).
    pub fn category_index(&self, name: &str) -> Option<usize> {
        self.categories.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The event index in a built-in slot (`None` if the slot or its event is out of range).
    pub fn slot_event(&self, slot: usize) -> Option<usize> {
        let i = *self.slots.get(slot)? as usize;
        (i < self.events.len()).then_some(i)
    }

    /// The parameter set of an event.
    pub fn params_of(&self, event: &SoundEvent) -> &SoundParams {
        &self.params[event.params as usize]
    }

    /// Lookup of named events (lower-cased name -> event indices, in file order).
    pub fn named_index(&self) -> HashMap<String, Vec<usize>> {
        let mut m: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in self.events.iter().enumerate() {
            if let Some(n) = &e.name {
                m.entry(n.to_ascii_lowercase()).or_default().push(i);
            }
        }
        m
    }
}

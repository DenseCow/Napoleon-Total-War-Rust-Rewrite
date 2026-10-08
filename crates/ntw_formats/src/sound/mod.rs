//! The sound system's data: event tables, sound banks and audio decoding.
//!
//! Spec and evidence: `analysis/audio/AUDIO_FORMAT.md`.
//!
//! | module | file | what |
//! |---|---|---|
//! | [`events`] | `sounds_packed\sound_events` | every sound event: category, parameters, sound files; built-in slots |
//! | [`banks`] | `sounds_packed\sound_bank_database` | sound settings + banks (situation -> event) |
//! | [`names`], [`bank_xml`] | `sounds\events\*.csv`, `sounds\banks\*.xml` | names for events and bank conditions (optional) |
//! | [`library`] | all of the above | [`SoundLibrary::load`] |
//! | [`anim_events`] | `animations\...\*.anim_sound_event` | sound cues on animation timelines |
//! | [`slots`] | (exe table) | the names of the 401 built-in event slots |
//! | [`decode`] | `.mp3`, `.wav` (PCM, IMA-ADPCM; MS-ADPCM by our own decoder) | decoding to interleaved `f32` PCM |
//! | [`loop_points`] | `.mp3`, `.wav` | `loop_start/end_block` byte offsets -> sample frames |
//!
//! The original game never reads the CSV/XML sources under `sounds\` at run time
//! (they are only used by a developer rebuild path); it reads the two packed files
//! under `sounds_packed\`. CONFIRMED (`0x0048C1B0`).

pub mod anim_events;
pub mod bank_xml;
pub mod banks;
pub mod decode;
pub mod events;
pub mod library;
pub mod loop_points;
mod ms_adpcm;
pub mod names;
pub mod slots;
/// Small sound files built in memory, for tests (here, and other crates' tests through the
/// `test-files` feature).
#[cfg(any(test, feature = "test-files"))]
pub mod test_files;

pub use banks::{BankEntry, SoundBank, SoundBankDatabase};
pub use decode::{decode, decode_timed, decode_timed_i16, DecodeError, Pcm, Pcm16, PcmStream, SharedBytes};
pub use events::{Param, SoundEvent, SoundEvents, SoundEventsError, SoundParams};
pub use library::SoundLibrary;

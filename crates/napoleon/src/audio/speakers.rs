//! The speaker setup the original's sound manager opens, and with it the speakers or headphones
//! volume multiplier (MIDDLEWARE_VERIFY.md §1.10). All CONFIRMED in Napoleon.exe 1.3 and Miles
//! `mss32.dll`:
//! - The manager lists twelve Miles channel setups in a fixed order (`0x01003EA0`): Windows
//!   default, stereo speakers, mono, stereo headphones, Dolby Surround, SRS Circle Surround,
//!   4.0, 5.1, 6.1, 7.1, 8.1, DirectSound 3D hardware ([`PROVIDERS`]). Only the ones whose driver
//!   opens are offered; the `sound_provider` preference is an index into that list.
//! - At start-up it opens the first that opens (`0x01003A70`); the preferences apply then switches
//!   to `sound_provider` when it is in range (`0x004837D0` -> manager slot `+0xE8` = `0x01006600`,
//!   which ignores an index outside `0..count`).
//! - After each driver open (`0x01003AD0`) and each change of the two multiplier settings,
//!   `0x01004390` asks Miles for the open setup (`AIL_speaker_configuration`) and uses
//!   `SS_HEADPHONES_VOLUME_MULTIPLIER` when it is headphones (`0x20`), else
//!   `SS_SPEAKERS_VOLUME_MULTIPLIER`.
//! - "Windows default" (`0x10`) asks DirectSound for the Windows speaker configuration and turns
//!   it into a Miles setup (Miles `0x211328F0`): only `DSSPEAKER_HEADPHONE` (1) gives headphones.
//!   When that query fails the driver does not open, so the entry is not offered.
//!
//! Our only use of the setup is that multiplier: we always mix stereo.

/// Miles' "use the system configuration" setup (`MSS_MC_USE_SYSTEM_CONFIG`).
const MC_USE_SYSTEM_CONFIG: u32 = 0x10;
/// Miles' headphones setup (`MSS_MC_HEADPHONES`): the one that takes the headphones multiplier.
const MC_HEADPHONES: u32 = 0x20;
/// `DSSPEAKER_HEADPHONE`, the low byte of DirectSound's speaker configuration.
const DSSPEAKER_HEADPHONE: u32 = 1;

/// The Miles channel setups the manager tries, in its list order (`0x01003EA0`).
pub const PROVIDERS: [u32; 12] = [MC_USE_SYSTEM_CONFIG, 0x02, 0x01, MC_HEADPHONES, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0];

/// Whether a DirectSound speaker configuration makes Miles' "Windows default" open as headphones
/// (Miles `0x211328F0` switches on the low byte; only 1 maps to `0x20`).
pub fn system_is_headphones(ds_speaker_config: u32) -> bool {
    ds_speaker_config & 0xFF == DSSPEAKER_HEADPHONE
}

/// Whether the setup the original opens for `sound_provider` is headphones. `system_config` is
/// DirectSound's speaker configuration (`None` when the query fails: then "Windows default" does
/// not open and is not in the list, so the indices shift down by one).
///
/// INFERRED: the other eleven setups are taken to open. Which ones Miles opens on a given machine
/// is not traced, and it matters: a setup that fails shrinks the list, so a saved index can fall
/// out of range, where the exe keeps the start-up setup (index 0, e.g. Windows default on a
/// headphones configuration) while we would pick the setup at that index.
pub fn opened_is_headphones(sound_provider: i64, system_config: Option<u32>) -> bool {
    let mut offered = PROVIDERS.iter().copied().filter(|&p| p != MC_USE_SYSTEM_CONFIG || system_config.is_some());
    let count = PROVIDERS.len() - usize::from(system_config.is_none());
    let index = usize::try_from(sound_provider).ok().filter(|&i| i < count).unwrap_or(0);
    match offered.nth(index) {
        Some(MC_USE_SYSTEM_CONFIG) => system_config.is_some_and(system_is_headphones),
        Some(p) => p == MC_HEADPHONES,
        None => false,
    }
}

/// The Windows speaker configuration as DirectSound reports it (`IDirectSound::GetSpeakerConfig`
/// on the default device, as Miles does), queried once; `None` when DirectSound fails.
pub fn system_speaker_config() -> Option<u32> {
    static CONFIG: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
    *CONFIG.get_or_init(query_system_speaker_config)
}

#[cfg(windows)]
fn query_system_speaker_config() -> Option<u32> {
    use windows::Win32::Media::Audio::DirectSound::{DirectSoundCreate, IDirectSound};
    let mut ds: Option<IDirectSound> = None;
    // SAFETY: a null device GUID and no aggregation, as Miles calls it; `ds` outlives the call.
    let created = unsafe { DirectSoundCreate(None, &mut ds, None) };
    let result = created.and_then(|()| {
        let ds = ds.ok_or_else(windows::core::Error::empty)?;
        // SAFETY: `ds` is a live DirectSound object.
        unsafe { ds.GetSpeakerConfig() }
    });
    match result {
        Ok(c) => Some(c),
        Err(e) => {
            bevy::log::warn!("sound: DirectSound speaker configuration query failed ({e}); \"Windows default\" is not offered");
            None
        }
    }
}

#[cfg(not(windows))]
fn query_system_speaker_config() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dsspeaker_headphone_is_headphones() {
        assert!(system_is_headphones(1));
        // DSSPEAKER_CONFIG packs a geometry byte above the configuration byte.
        assert!(system_is_headphones(0x0A_01));
        for c in [0, 2, 3, 4, 5, 6, 7, 8, 9] {
            assert!(!system_is_headphones(c), "{c}");
        }
    }

    #[test]
    fn provider_index_picks_from_the_offered_list() {
        let stereo = Some(4);
        // Default 0 = Windows default, which follows the system configuration.
        assert!(!opened_is_headphones(0, stereo));
        assert!(opened_is_headphones(0, Some(1)));
        // 3 = "Stereo headphones".
        assert!(opened_is_headphones(3, stereo));
        assert!(!opened_is_headphones(1, stereo));
        assert!(!opened_is_headphones(2, stereo));
        assert!(!opened_is_headphones(4, stereo));
    }

    #[test]
    fn out_of_range_index_keeps_the_first_setup() {
        // `0x01006600` ignores it: the start-up driver (index 0) stays open.
        assert!(opened_is_headphones(-1, Some(1)));
        assert!(opened_is_headphones(12, Some(1)));
        assert!(!opened_is_headphones(12, Some(4)));
        assert!(!opened_is_headphones(11, None), "11 entries without Windows default");
    }

    #[test]
    fn failed_system_query_shifts_the_indices() {
        // Without "Windows default" the list starts at stereo speakers: headphones is index 2.
        assert!(opened_is_headphones(2, None));
        assert!(!opened_is_headphones(3, None));
        assert!(!opened_is_headphones(0, None));
    }
}

//! `.anim_sound_event` files: sound cues on an animation's timeline.
//!
//! Each `animations\...\<clip>.anim_sound_event` in `sound.pack` sits next to the clip of
//! the same name and lists the moments a sound should start (e.g. a musket clip's shot).
//!
//! ```text
//! u32 version          1 in every shipped file
//! u32 n
//! n x { f32 time_seconds; u32 cue }
//! end of file
//! ```
//! Layout CONFIRMED by an exact-EOF parse of all 2,617 shipped files.
//!
//! `cue` (INFERRED, 2026-10-04, `analysis/fidelity/MIDDLEWARE_VERIFY.md` §3): for cues 27..=106 the cue is
//! the built-in slot `cue + 187` ([`super::slots::SLOT_NAMES`]): the clips that use each cue match the slot
//! names one for one (archer aim / fire = `BOW_DRAW` / `BOW_RELEASE`, the cannon, rocket and mortar crew
//! clips = the `CANNON_*` / `ROCKET_*` / `MORTAR_*` slots in order, carbine / musket reloads =
//! `MUSKET_RAM_SHORT` / `_LONG`, `horse_cheval_*` = `HORSE_CHEVAL`, pistol deaths = `PISTOL_IMPACT_GROUND`,
//! elephant attacks = `ELEPHANT_MELEE`). Cues below 27 (gallops, footsteps; up to 6,000 uses) and above 106
//! (campaign ships) do not fit that offset: UNKNOWN (likely footstep / hoof cues resolved through the
//! footstep bank). See [`cue_slot`].

use crate::bytes::Cursor;

/// One cue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimSoundCue {
    /// Seconds from the start of the clip.
    pub time: f32,
    /// Cue type (see the module docs and [`cue_slot`]).
    pub cue: u32,
}

/// A parsed `.anim_sound_event` file.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimSoundEvents {
    pub version: u32,
    pub cues: Vec<AnimSoundCue>,
}

impl AnimSoundEvents {
    /// Parses a file; `None` if it is truncated or has trailing bytes.
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let mut c = Cursor::new(bytes);
        let version = c.u32().ok()?;
        let n = c.u32().ok()?;
        let mut cues = Vec::with_capacity((n as usize).min(c.remaining() / 8));
        for _ in 0..n {
            let time = c.f32().ok()?;
            let cue = c.u32().ok()?;
            cues.push(AnimSoundCue { time, cue });
        }
        (c.remaining() == 0).then_some(Self { version, cues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let mut b = vec![1, 0, 0, 0, 1, 0, 0, 0];
        b.extend(0.25f32.to_le_bytes());
        b.extend(30u32.to_le_bytes());
        let a = AnimSoundEvents::read(&b).unwrap();
        assert_eq!(a.cues, vec![AnimSoundCue { time: 0.25, cue: 30 }]);
        b.push(0);
        assert!(AnimSoundEvents::read(&b).is_none());
    }
}

/// The built-in sound slot a cue plays, where the `cue + 187` mapping holds (cues 27..=106,
/// INFERRED from the clips that use each cue; module docs). `None` outside that range (UNKNOWN).
pub fn cue_slot(cue: u32) -> Option<&'static str> {
    if (27..=106).contains(&cue) {
        super::slots::SLOT_NAMES.get(cue as usize + 187).copied()
    } else {
        None
    }
}

#[cfg(test)]
mod cue_tests {
    use super::cue_slot;

    #[test]
    fn cue_slots_match_the_clip_families() {
        assert_eq!(cue_slot(35), Some("BOW_DRAW"));
        assert_eq!(cue_slot(36), Some("BOW_RELEASE"));
        assert_eq!(cue_slot(40), Some("CANNON_LIGHT_FUSE"));
        assert_eq!(cue_slot(101), Some("HORSE_CHEVAL"));
        assert_eq!(cue_slot(106), Some("ELEPHANT_MELEE"));
        assert_eq!(cue_slot(23), None);
        assert_eq!(cue_slot(107), None);
    }
}

//! The campaign camera's zoom limit: the tilt bounds the scripts set with `set_zoom_limit`.
//!
//! The campaign camera has one input, its tilt (camera +0x138): 0.5 is the far, high view and 1.0
//! the near, low one ([`distance_for_tilt`]). The world's episodic restrictions object (campaign
//! +0xFA8) holds two bounds on it, +0xBC (the largest tilt, the nearest view) and +0xC0 (the
//! smallest, the farthest view) (CONFIRMED, `analysis/fidelity/UI_FIDELITY.md` "Zoom limit"):
//!
//! - **Defaults.** Its constructor `0x009968A0` sets them from the camera tweakers
//!   `CAMPAIGN_CAMERA_MAXIMUM_TILT_ANGLE` (`0x015C3B20`, 1.0) and
//!   `CAMPAIGN_CAMERA_MINIMUM_TILT_ANGLE` (`0x015C4078`, 0.5) (stores at `0x009969E6` / `0x009969FC`).
//! - **Saved.** The same function reads them from `CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS`: #13 is
//!   the smallest tilt (read in every version at `0x00997B9D`; only its position moves, #12 in a v1
//!   record that lacks #8) and #14 the largest (only above version 2, `0x00997BBA`); the writer
//!   `0x009A1070` writes both (`0x009A1957`,
//!   `0x009A19A1`).
//! - **Set.** Their only writer is the script method `set_zoom_limit(a, b)` (`0x00977CB0`,
//!   [`ZoomLimit::from_script`]).
//! - **Applied.** Their only reader is the tilt step `0x00920D00` ([`ZoomLimit::clamp`]), run after
//!   every tilt change: the mouse wheel (`0x00956E10` → `0x00977100`), the camera keys
//!   (`0x0095A510` → `0x00977100`), a tilt set outright (`0x009770E0`) and the camera build
//!   (`0x00900E50`). `set_zoom_limit` itself moves no camera: the view is clamped at its next
//!   tilt change.

/// `CAMPAIGN_CAMERA_MINIMUM_DISTANCE` (`0x015C3A30`): the eye's distance at the largest tilt, 1.0.
pub const MIN_DISTANCE: f32 = 15.0;
/// `CAMPAIGN_CAMERA_MAXIMUM_DISTANCE` (`0x015C2FA0`): the eye's distance at the smallest tilt, 0.5.
pub const MAX_DISTANCE: f32 = 100.0;
/// `CAMPAIGN_CAMERA_MINIMUM_TILT_ANGLE` (`0x015C4078`).
pub const MIN_TILT: f32 = 0.5;
/// `CAMPAIGN_CAMERA_MAXIMUM_TILT_ANGLE` (`0x015C3B20`).
pub const MAX_TILT: f32 = 1.0;

/// The smallest bound `set_zoom_limit` takes (the float `0x013359F8`): a smaller argument becomes it.
const SCRIPT_FLOOR: f32 = 0.02;
/// The margin `set_zoom_limit` keeps below the minimum distance (the double `0x0133BC08`).
const SCRIPT_MARGIN: f64 = 0.02;

/// The eye's distance from its target for a tilt: the linear law of the eye placement
/// `0x009208B0`, `MIN_DISTANCE` at `MAX_TILT` and `MAX_DISTANCE` at `MIN_TILT`, not clamped (a
/// script bound outside 0.5..1.0 gives a distance outside 15..100, as in the exe).
pub fn distance_for_tilt(tilt: f32) -> f32 {
    MAX_DISTANCE - (tilt - MIN_TILT) / (MAX_TILT - MIN_TILT) * (MAX_DISTANCE - MIN_DISTANCE)
}

/// The tilt [`distance_for_tilt`] maps to a distance (its inverse), not clamped.
pub fn tilt_for_distance(distance: f32) -> f32 {
    MIN_TILT + (MAX_DISTANCE - distance) / (MAX_DISTANCE - MIN_DISTANCE) * (MAX_TILT - MIN_TILT)
}

/// The tilt bounds of the campaign camera (episodic restrictions +0xBC / +0xC0).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ZoomLimit {
    /// The largest tilt, the nearest view (+0xBC, `EPISODIC_RESTRICTIONS` #14).
    pub max_tilt: f32,
    /// The smallest tilt, the farthest view (+0xC0, `EPISODIC_RESTRICTIONS` #13).
    pub min_tilt: f32,
}

impl Default for ZoomLimit {
    /// The constructor's values `0x009968A0`: the tweakers' full range.
    fn default() -> Self {
        Self { max_tilt: MAX_TILT, min_tilt: MIN_TILT }
    }
}

impl ZoomLimit {
    /// The bounds `set_zoom_limit(a, b)` stores (`0x00977CB0`): each argument is raised to 0.02
    /// and lowered to the tilt a hair above the one at the minimum distance
    /// (`MAX_TILT + (MIN_DISTANCE / (MAX_DISTANCE - MIN_DISTANCE) - 0.02) × (MAX_TILT - MIN_TILT)`,
    /// about 1.0782 with the vanilla tweakers); the larger is the new largest tilt, the smaller the
    /// new smallest, so the order of the two arguments does not matter.
    /// The arguments are the original's `lua_Number`, an `f32` (DESIGN §3.5), and the exe works in
    /// `f32` SSE arithmetic but for the margin, subtracted in `f64` (`0x00977D21`..`0x00977D2C`). A
    /// NaN passes both tests (`COMISS` + `JBE`) and is stored as it is, as the larger bound when it
    /// is the last argument and as the smaller otherwise (`0x00977D9F`).
    pub fn from_script(a: f32, b: f32) -> Self {
        let margin = (f64::from(MIN_DISTANCE / (MAX_DISTANCE - MIN_DISTANCE)) - SCRIPT_MARGIN) as f32;
        let upper = margin * (MAX_TILT - MIN_TILT) + MAX_TILT;
        // The exe's order: the floor test first, the upper test only when the floor passed.
        let bound = |x: f32| {
            if SCRIPT_FLOOR > x {
                SCRIPT_FLOOR
            } else if x > upper {
                upper
            } else {
                x
            }
        };
        // `0x01055700` reads the stack top first: `b`, then `a`.
        let (first, second) = (bound(b), bound(a));
        if second > first {
            Self { max_tilt: second, min_tilt: first }
        } else {
            Self { max_tilt: first, min_tilt: second }
        }
    }

    /// The tilt step's clamp `0x00920D00`: below the smallest tilt gives the smallest, else above
    /// the largest gives the largest; anything else (NaN included) is kept.
    pub fn clamp(&self, tilt: f32) -> f32 {
        if self.min_tilt > tilt {
            self.min_tilt
        } else if tilt > self.max_tilt {
            self.max_tilt
        } else {
            tilt
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_tweakers() {
        let z = ZoomLimit::default();
        assert_eq!((z.max_tilt, z.min_tilt), (1.0, 0.5));
        assert_eq!(distance_for_tilt(z.max_tilt), MIN_DISTANCE);
        assert_eq!(distance_for_tilt(z.min_tilt), MAX_DISTANCE);
    }

    /// The vanilla scripts' calls (`data/campaigns/*/scripting.lua`).
    #[test]
    fn script_bounds_follow_the_exe() {
        let z = ZoomLimit::from_script(1.0, 0.61);
        assert_eq!((z.max_tilt, z.min_tilt), (1.0, 0.61));
        assert!((distance_for_tilt(z.min_tilt) - 81.3).abs() < 1e-3);
        // Either order: the larger is the largest tilt.
        assert_eq!(ZoomLimit::from_script(0.93, 1.0), ZoomLimit { max_tilt: 1.0, min_tilt: 0.93 });
        // Below the floor: 0.02.
        assert_eq!(ZoomLimit::from_script(1.0, 0.0), ZoomLimit { max_tilt: 1.0, min_tilt: 0.02 });
        assert_eq!(ZoomLimit::from_script(-5.0, -1.0), ZoomLimit { max_tilt: 0.02, min_tilt: 0.02 });
        // Above the upper bound: (15/85 - 0.02) × 0.5 + 1, the exe's f32 steps.
        let upper = ((f64::from(15.0f32 / 85.0) - 0.02) as f32) * 0.5 + 1.0;
        assert_eq!(ZoomLimit::from_script(3.0, 0.5), ZoomLimit { max_tilt: upper, min_tilt: 0.5 });
        assert!((upper - 1.0782).abs() < 1e-4);
        // The upper bound keeps the eye just short of the target (a distance above 0).
        assert!(distance_for_tilt(upper) > 0.0);
    }

    /// A NaN passes both tests and is stored: the larger bound as the last argument (`0x00977D9F`).
    #[test]
    fn a_nan_bound_is_kept_as_the_exe_keeps_it() {
        let z = ZoomLimit::from_script(0.7, f32::NAN);
        assert!(z.max_tilt.is_nan() && z.min_tilt == 0.7);
        let z = ZoomLimit::from_script(f32::NAN, 0.7);
        assert!(z.max_tilt == 0.7 && z.min_tilt.is_nan());
    }

    #[test]
    fn clamp_order_and_nan() {
        let z = ZoomLimit::from_script(1.0, 0.61);
        assert_eq!(z.clamp(0.5), 0.61);
        assert_eq!(z.clamp(1.2), 1.0);
        assert_eq!(z.clamp(0.8), 0.8);
        assert!(z.clamp(f32::NAN).is_nan());
        // Crossed bounds (only a save can hold them): the smallest wins below it, the largest above.
        let crossed = ZoomLimit { max_tilt: 0.6, min_tilt: 0.9 };
        assert_eq!(crossed.clamp(0.7), 0.9);
        assert_eq!(crossed.clamp(0.95), 0.6);
    }

    #[test]
    fn distance_law_round_trips() {
        for t in [0.02f32, 0.5, 0.61, 0.75, 1.0, 1.07] {
            assert!((tilt_for_distance(distance_for_tilt(t)) - t).abs() < 1e-5);
        }
        assert_eq!(distance_for_tilt(0.65), 74.5);
    }
}

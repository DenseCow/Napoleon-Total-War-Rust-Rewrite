//! The campaign camera's map bounds: the rectangle the scripts set with `set_map_bounds`.
//!
//! The world's episodic restrictions object (campaign +0xFA8) holds one rectangle on the camera's
//! target, in the engine's axes (x east, z north), at +0xAC (west, smallest x), +0xB0 (south,
//! smallest z), +0xB4 (east, largest x) and +0xB8 (north, largest z) (CONFIRMED,
//! `analysis/fidelity/UI_FIDELITY.md` "Map bounds"):
//!
//! - **Defaults.** The restrictions' reader `0x009968A0` sets them to -1280, -640, 1280, 640
//!   (stores at `0x009969B9`..`0x009969D7`) before it reads the record; a new campaign's world
//!   (`0x008742C0`) copies them from its map object (`0x00874D98`), and the start position's record
//!   then gives them anyway. Every vanilla Europe save holds -320, -320, 320, 320.
//! - **Saved.** `CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS` #9..#12 in that order (#8..#11 in a v1
//!   record that lacks #8), read in every version (`0x00997B51`..`0x00997B98`) and written by
//!   `0x009A1070` (`0x009A180E`..`0x009A18E9`).
//! - **Set.** By the script method
//!   `set_map_bounds(left, top, right, bottom)` (`0x00977C10`, [`MapBounds::from_script`]).
//! - **Applied.** The camera's key steps (`0x00956E70` / `0x00956F10` / `0x00956FA0` /
//!   `0x009570F0`, [`MapBounds::step_south`] and the rest) and its jump to a point (`0x00955A00`,
//!   [`MapBounds::clamp`], the bookmarks' view) clamp the target to them.
//!   `set_map_bounds` itself moves no camera.

/// The camera target's bounds (episodic restrictions +0xAC..+0xB8), engine axes: x east, z north.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MapBounds {
    /// The smallest x, west (+0xAC, `EPISODIC_RESTRICTIONS` #9).
    pub min_x: f32,
    /// The smallest z, south (+0xB0, #10).
    pub min_z: f32,
    /// The largest x, east (+0xB4, #11).
    pub max_x: f32,
    /// The largest z, north (+0xB8, #12).
    pub max_z: f32,
}

impl Default for MapBounds {
    /// The restrictions' reader's values before it reads the record (`0x009968A0`).
    fn default() -> Self {
        Self { min_x: -1280.0, min_z: -640.0, max_x: 1280.0, max_z: 640.0 }
    }
}

impl MapBounds {
    /// The bounds `set_map_bounds(left, top, right, bottom)` stores (`0x00977C10`): the four
    /// numbers as they are, unchecked and unordered (the tutorial's
    /// `set_map_bounds(-130, -75, -113, -98)` gives x -130..-113, z -98..-75).
    pub fn from_script(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { min_x: left, min_z: bottom, max_x: right, max_z: top }
    }

    /// The target's x after a step `d` (≥ 0) east (`0x009570F0`): the largest x when the step would
    /// pass it, so a target east of it comes back to it; otherwise `x + d` (a NaN never clamps,
    /// `COMISS` + `JBE`).
    pub fn step_east(&self, x: f32, d: f32) -> f32 {
        if x + d > self.max_x { self.max_x } else { x + d }
    }

    /// The target's x after a step `d` west (`0x00956FA0`), as [`Self::step_east`] against the
    /// smallest x.
    pub fn step_west(&self, x: f32, d: f32) -> f32 {
        if self.min_x > x - d { self.min_x } else { x - d }
    }

    /// The target's z after a step `d` north (`0x00956F10`), against the largest z.
    pub fn step_north(&self, z: f32, d: f32) -> f32 {
        if z + d > self.max_z { self.max_z } else { z + d }
    }

    /// The target's z after a step `d` south (`0x00956E70`), against the smallest z.
    pub fn step_south(&self, z: f32, d: f32) -> f32 {
        if self.min_z > z - d { self.min_z } else { z - d }
    }

    /// The point (`x` east, `z` north) the camera's jump to a point (`0x00955A00`) moves the target
    /// to: each axis on its own, the smallest value when below it, else the largest when above it,
    /// else as it is (`0x00955A61`..`0x00955A9E`, `COMISS` + `JA` / `JBE`, so a NaN never clamps and
    /// unordered bounds give the smallest value first).
    pub fn clamp(&self, x: f32, z: f32) -> (f32, f32) {
        let axis = |v: f32, lo: f32, hi: f32| if lo > v { lo } else if v > hi { hi } else { v };
        (axis(x, self.min_x, self.max_x), axis(z, self.min_z, self.max_z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tutorial's calls (`data/campaigns/tut_napoleon/scripting.lua`): left, top, right, bottom.
    #[test]
    fn script_bounds_are_left_top_right_bottom() {
        let b = MapBounds::from_script(-365.0, 30.0, -105.0, -113.0);
        assert_eq!(b, MapBounds { min_x: -365.0, min_z: -113.0, max_x: -105.0, max_z: 30.0 });
    }

    /// A step stops at the bound it would pass; a target already outside that bound comes back to
    /// it, one outside the other bound steps on.
    #[test]
    fn steps_stop_at_the_bounds() {
        let b = MapBounds::from_script(-10.0, 10.0, 10.0, -10.0);
        assert_eq!(b.step_east(5.0, 2.0), 7.0);
        assert_eq!(b.step_east(9.0, 2.0), 10.0);
        assert_eq!(b.step_east(15.0, 2.0), 10.0);
        assert_eq!(b.step_east(-15.0, 2.0), -13.0);
        assert_eq!(b.step_west(-9.0, 2.0), -10.0);
        assert_eq!(b.step_west(-15.0, 2.0), -10.0);
        assert_eq!(b.step_north(9.0, 2.0), 10.0);
        assert_eq!(b.step_north(0.0, 2.0), 2.0);
        assert_eq!(b.step_south(-9.0, 2.0), -10.0);
        assert_eq!(b.step_south(20.0, 2.0), 18.0);
        assert!(b.step_east(f32::NAN, 2.0).is_nan());
    }

    /// The jump to a point clamps each axis to its bounds (`0x00955A00`); a NaN stays NaN, and
    /// unordered bounds (smallest above largest) give the smallest.
    #[test]
    fn a_jump_clamps_the_point_to_the_bounds() {
        let b = MapBounds::from_script(-10.0, 10.0, 10.0, -10.0);
        assert_eq!(b.clamp(5.0, -3.0), (5.0, -3.0));
        assert_eq!(b.clamp(-20.0, 30.0), (-10.0, 10.0));
        assert_eq!(b.clamp(20.0, -30.0), (10.0, -10.0));
        assert!(b.clamp(f32::NAN, 0.0).0.is_nan());
        let unordered = MapBounds::from_script(5.0, 0.0, -5.0, 0.0);
        assert_eq!(unordered.clamp(0.0, 0.0), (5.0, 0.0));
    }
}

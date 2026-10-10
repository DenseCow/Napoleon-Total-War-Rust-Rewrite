//! The original's gameplay caps as data values a mod can change (CLAUDE.md "No engine limits";
//! user, 2026-10-10: moddable data, not player settings).
//!
//! [`GameLimits::default`] is the original game: every field states the original's value and where
//! the exe keeps it. A mod changes them with `_kv_rules` rows under the keys named below; the exe
//! never reads these keys (they are not in `ntw_data::kv::KV_RULES_KEYS`), so they change nothing
//! in the original. `ntw_data::kv::game_limits` reads them from the merged `_kv_rules` (every mod's
//! file, in load order). A list is read as `<key>_0`, `<key>_1`, ... up to the highest index
//! present; a missing entry keeps the original's, so a mod ships only the entries it replaces or
//! appends.
//!
//! The units per army and per fleet at a unit scale ([`GameLimits::max_units`]) are one rule for
//! the custom battle army setup and for a new campaign's caps (the campaign then keeps its caps in
//! its saves, [`crate::campaign::rules::ForceCaps`]).

use crate::battle::unit_scale;

/// `_kv_rules` list key: the unit-size option's scales ([`GameLimits::unit_scales`]).
pub const KEY_UNIT_SCALE: &str = "unit_scale";
/// `_kv_rules` key: land units per army ([`GameLimits::max_land_units`]).
pub const KEY_MAX_LAND_UNITS: &str = "max_land_units";
/// `_kv_rules` list key: ships per fleet by unit-size index ([`GameLimits::max_naval_units`]).
pub const KEY_MAX_NAVAL_UNITS: &str = "max_naval_units";
/// `_kv_rules` list key: custom battle land funds by army size ([`GameLimits::custom_battle_funds_land`]).
pub const KEY_CUSTOM_BATTLE_FUNDS_LAND: &str = "custom_battle_funds_land";
/// `_kv_rules` list key: custom battle naval funds by fleet size ([`GameLimits::custom_battle_funds_naval`]).
pub const KEY_CUSTOM_BATTLE_FUNDS_NAVAL: &str = "custom_battle_funds_naval";
/// `_kv_rules` key: reinforcement units an army brings onto the field
/// ([`GameLimits::max_reinforcement_units`]).
pub const KEY_MAX_REINFORCEMENT_UNITS: &str = "max_reinforcement_units";

/// Land units per army at any unit scale: 20. The exe's land function is folded into a shared
/// `return 20` (`0x00851550`), called with the scale by the custom battle army setup
/// (`0x0045CB50`) and the new campaign setup (`0x00485B90`; CONFIRMED).
pub const ORIGINAL_MAX_LAND_UNITS: u32 = 20;
/// Ships per fleet by unit-size index: `MaxUnitsFromUnitScaleFactor` `0x00DACE60` returns
/// 6 / 8 / 10 / 20 (CONFIRMED; the same two callers as [`ORIGINAL_MAX_LAND_UNITS`]).
pub const ORIGINAL_MAX_NAVAL_UNITS: [u32; 4] = [6, 8, 10, 20];
/// Custom battle funds by size, land: `0x0131A190` {5000, 10000, 14000} (`ArmyFundsForSize`,
/// `0x004A2910`, CONFIRMED).
pub const ORIGINAL_FUNDS_LAND: [i32; 3] = [5000, 10000, 14000];
/// Custom battle funds by size, naval: `0x0131A19C` {5000, 14000, 24000} (CONFIRMED).
pub const ORIGINAL_FUNDS_NAVAL: [i32; 3] = [5000, 14000, 24000];
/// Reinforcements: the arrival step `0x00608200` stops bringing units of an entry group once it
/// holds 20 (`+0xF0 < 0x14`, CONFIRMED, BATTLE_FIDELITY.md §13).
pub const ORIGINAL_MAX_REINFORCEMENT_UNITS: u32 = 20;

/// The gameplay caps (see the module docs). Lists read from data always have their entries; a
/// hand-built empty list reads as scale 1.0, its last naval entry as 20 and funds as 0.
#[derive(Debug, Clone, PartialEq)]
pub struct GameLimits {
    /// The unit-size option's scale per index (`unit_scale_<i>`). Original: the four steps of
    /// `0x01392770`, [`unit_scale::STEPS`]. A step above 1.0 gives units more men than their card
    /// (the original's clamp stops at 1.0, [`unit_scale::MAX`]).
    pub unit_scales: Vec<f32>,
    /// Land units per army at any unit scale (`max_land_units`): 20.
    pub max_land_units: u32,
    /// Ships per fleet per unit-size index (`max_naval_units_<i>`): 6 / 8 / 10 / 20. An index past
    /// the list takes its last entry.
    pub max_naval_units: Vec<u32>,
    /// Custom battle funds per army size, land (`custom_battle_funds_land_<i>`).
    pub custom_battle_funds_land: Vec<i32>,
    /// Custom battle funds per fleet size, naval (`custom_battle_funds_naval_<i>`).
    pub custom_battle_funds_naval: Vec<i32>,
    /// Reinforcement units brought in per army (`max_reinforcement_units`): 20.
    pub max_reinforcement_units: u32,
}

impl Default for GameLimits {
    fn default() -> Self {
        GameLimits {
            unit_scales: unit_scale::STEPS.to_vec(),
            max_land_units: ORIGINAL_MAX_LAND_UNITS,
            max_naval_units: ORIGINAL_MAX_NAVAL_UNITS.to_vec(),
            custom_battle_funds_land: ORIGINAL_FUNDS_LAND.to_vec(),
            custom_battle_funds_naval: ORIGINAL_FUNDS_NAVAL.to_vec(),
            max_reinforcement_units: ORIGINAL_MAX_REINFORCEMENT_UNITS,
        }
    }
}

impl GameLimits {
    /// The scale of unit-size index `index` (`0x00DAFBB0`): an index outside the list takes the
    /// last step, as the original's read past its four entries ends on 1.0 after the clamp.
    pub fn unit_scale(&self, index: i32) -> f32 {
        usize::try_from(index).ok().and_then(|i| self.unit_scales.get(i)).copied().unwrap_or_else(|| self.last_scale())
    }

    /// The unit-size index of a scale (`0x00DAFBC0`, CONFIRMED): the first step not below it,
    /// else the last.
    pub fn unit_scale_index(&self, scale: f32) -> usize {
        self.unit_scales.iter().position(|s| scale <= *s).unwrap_or(self.unit_scales.len().saturating_sub(1))
    }

    /// A `gfx_unit_scale` preference index as a unit-size index: missing → the last step (the
    /// frontend's default), otherwise clamped into the list.
    pub fn unit_scale_setting(&self, setting: Option<i64>) -> usize {
        let last = self.unit_scales.len().saturating_sub(1);
        setting.map_or(last, |i| usize::try_from(i.max(0)).unwrap_or(last).min(last))
    }

    /// The highest scale a battle may use: the original's 1.0, or a modded step above it.
    pub fn max_scale(&self) -> f32 {
        self.unit_scales.iter().copied().fold(unit_scale::MAX, f32::max)
    }

    /// Units at unit scale `scale`: (land units per army, ships per fleet). The custom battle
    /// army setup (`0x0045CB50`: land or naval by the army) and a new campaign's caps
    /// (`0x00485B90`, with the `campaign_unit_multiplier` preference) both use it.
    pub fn max_units(&self, scale: f32) -> (u32, u32) {
        let i = self.unit_scale_index(scale);
        let naval = &self.max_naval_units;
        (self.max_land_units, naval.get(i).or(naval.last()).copied().unwrap_or(ORIGINAL_MAX_LAND_UNITS))
    }

    /// The most units the custom battle's `ValidateArmySetup` (`0x00479690`) keeps in a saved
    /// setup: a literal 20 for both kinds in the exe (`0x00479A79` land, `0x00479EE7` sea;
    /// CONFIRMED); ours the largest army or fleet these limits allow (20 for both in the
    /// original), so a mod's bigger armies validate.
    pub fn max_setup_units(&self, naval: bool) -> u32 {
        if naval { self.max_naval_units.iter().copied().max().unwrap_or(ORIGINAL_MAX_LAND_UNITS) } else { self.max_land_units }
    }

    /// Custom battle funds for army size `size` (clamped into the list, as `0x004A2910` clamps to
    /// 0..=2).
    pub fn custom_battle_funds(&self, size: i64, naval: bool) -> i32 {
        let list = if naval { &self.custom_battle_funds_naval } else { &self.custom_battle_funds_land };
        let i = usize::try_from(size.max(0)).unwrap_or(usize::MAX);
        list.get(i).or(list.last()).copied().unwrap_or(0)
    }

    fn last_scale(&self) -> f32 {
        self.unit_scales.last().copied().unwrap_or(unit_scale::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_original() {
        let l = GameLimits::default();
        assert_eq!(l.unit_scales, [0.25, 0.5, 0.75, 1.0]);
        assert_eq!((0..4).map(|i| l.unit_scale(i)).collect::<Vec<_>>(), [0.25, 0.5, 0.75, 1.0]);
        assert_eq!((l.unit_scale(-1), l.unit_scale(4)), (1.0, 1.0));
        assert_eq!(l.max_scale(), 1.0);
        assert_eq!([0.25, 0.5, 0.75, 1.0].map(|s| l.max_units(s)), [(20, 6), (20, 8), (20, 10), (20, 20)]);
        // 0x00DAFBC0: the first step not below the scale; past the last, the last.
        assert_eq!((l.max_units(0.3), l.max_units(0.0), l.max_units(5.0)), ((20, 8), (20, 6), (20, 20)));
        assert_eq!((0..4).map(|s| l.custom_battle_funds(s, false)).collect::<Vec<_>>(), [5000, 10000, 14000, 14000]);
        assert_eq!((-1..3).map(|s| l.custom_battle_funds(s, true)).collect::<Vec<_>>(), [5000, 5000, 14000, 24000]);
        assert_eq!((l.unit_scale_setting(None), l.unit_scale_setting(Some(-2)), l.unit_scale_setting(Some(9))), (3, 0, 3));
        assert_eq!((l.max_setup_units(false), l.max_setup_units(true)), (20, 20));
    }

    #[test]
    fn modded_lists_go_past_the_original() {
        let mut l = GameLimits::default();
        l.unit_scales.push(2.0);
        l.max_naval_units.push(40);
        l.max_land_units = 40;
        assert_eq!(l.max_scale(), 2.0);
        assert_eq!(l.unit_scale(4), 2.0);
        assert_eq!(l.unit_scale_setting(Some(9)), 4);
        assert_eq!(l.max_units(1.5), (40, 40));
        // A scale list longer than the naval list: the last naval entry.
        l.unit_scales.push(3.0);
        assert_eq!(l.max_units(3.0), (40, 40));
        // Hand-built empty lists do not panic.
        let empty = GameLimits { unit_scales: Vec::new(), max_naval_units: Vec::new(), custom_battle_funds_land: Vec::new(), ..GameLimits::default() };
        assert_eq!((empty.unit_scale(2), empty.max_units(1.0), empty.custom_battle_funds(1, false)), (1.0, (20, 20), 0));
    }
}

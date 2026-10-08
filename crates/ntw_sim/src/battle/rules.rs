//! The `kv_rules` table: the combat rule constants read by melee and missile resolution.
//!
//! Key list and value types: W1 `kv_layout.tsv` (CONFIRMED). The original reads them through a
//! rules adapter (vtable 0x01337A28) whose method `+4*i` returns key `i` (W1 §12.9, CONFIRMED).
//!
//! Most keys are truncated to `i32` on load (`cvttss2si`, W1 §9); the keys listed as floats in W1 §9
//! stay `f32`. The four `special_ability_*` keys use a third, not yet understood adder
//! (type "other"); we keep them as `f32` until that is resolved (INFERRED).
//!
//! **No game data here**: `Default` is all zeros. Values come from the player's own game files.

/// All `kv_rules` keys in data order (W1 `kv_layout.tsv`). The **order is load order**: the exe
/// reads the key `i` of `kv_rules_table` through a rules adapter whose vtable method at `+4*i`
/// returns it (CONFIRMED, BATTLE_FIDELITY.md §57 (2): the fatigue multiplier is read at `+4`, the
/// attack-direction factors at `+0x5C..+0x68` = keys 23..26, and the height min / max / divisor at
/// `+0x10 / +0x14 / +0x18` = keys 4 / 5 / 6).
#[allow(missing_docs)] // each field is named exactly like its kv_rules key
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct KvRules {
    /// Key 0. CONFIRMED **loaded but never read**: `0x00F42950` puts it in the rules list at index
    /// 0, so the adapter exposes it at slot `+0`, and no function in the melee blow-resolution
    /// chain (`0x00DAA290` → `0x00DAB5F0` hit number → `0x00DADA40` kill chance → `0x00DAC2A0`
    /// clamp → `0x00DADB20` roll) reads that slot. Its slots are 4, `0x10`, `0x14`, `0x18`,
    /// `0x5C`, `0x60`, `0x64`, `0x68` only. So the original's melee has **no experience term**,
    /// and [`super::melee::hit_number`] correctly has none either (BATTLE_FIDELITY.md §57 (1)).
    /// Kept because the table ships the key and the loader reads it.
    pub relative_melee_experience_multiplier: i32,
    /// Key 1. The only "relative" multiplier the melee code uses: the "Melee Fatigue Factor"
    /// `0x00DAD990` is `this × (defender fatigue − attacker fatigue)` (CONFIRMED).
    pub relative_melee_fatigue_multiplier: i32,
    pub melee_charge_factor_power_divisor: i32,
    pub melee_entrenchement_level_multiplier: i32,
    pub melee_height_delta_min: f32,
    pub melee_height_delta_max: f32,
    pub relative_melee_height_delta_divisor: f32,
    pub hnbonus_bayonet: i32,
    pub hnbonus_melee_cavalry_v_infantry: i32,
    pub hnbonus_melee_cavalry_v_squareinfantry: i32,
    pub missile_cover_factor_none: i32,
    pub missile_cover_factor_slight: i32,
    pub missile_cover_factor_some: i32,
    pub missile_cover_factor_good: i32,
    pub missile_cover_factor_excellent: i32,
    pub armour_missile_penetrating_divisor: i32,
    pub armour_missile_piercing_divisor: i32,
    pub armour_melee_penetrating_divisor: i32,
    pub armour_melee_piercing_divisor: i32,
    pub defense_missile_penetrating_divisor: i32,
    pub defense_missile_piercing_divisor: i32,
    pub defense_melee_penetrating_divisor: i32,
    pub defense_melee_piercing_divisor: i32,
    pub factor_attackdir_flankleft: i32,
    pub factor_attackdir_flankright: i32,
    pub factor_attackdir_rear: i32,
    pub factor_attackdir_front: i32,
    pub attackpower_long_range_multiplier: f32,
    pub attackpower_extreme_range_multplier: f32,
    pub bayonet_melee_attack_bonus: i32,
    pub bayonet_ring_reload_time_penalty: f32,
    pub melee_xholds_knockdown_0: i32,
    pub melee_xholds_knockback_0: i32,
    pub melee_xholds_stepback_0: i32,
    pub melee_xholds_knockdown_1: i32,
    pub melee_xholds_knockback_1: i32,
    pub melee_xholds_stepback_1: i32,
    pub melee_xholds_knockdown_2: i32,
    pub melee_xholds_knockback_2: i32,
    pub melee_xholds_stepback_2: i32,
    pub melee_xholds_knockdown_3: i32,
    pub melee_xholds_knockback_3: i32,
    pub melee_xholds_stepback_3: i32,
    pub melee_xholds_knockdown_4: i32,
    pub melee_xholds_knockback_4: i32,
    pub melee_xholds_stepback_4: i32,
    pub melee_hn_to_xholds_0_max: i32,
    pub melee_hn_to_xholds_1_max: i32,
    pub melee_hn_to_xholds_2_max: i32,
    pub melee_hn_to_xholds_3_max: i32,
    pub missile_xholds_knockdown_0: i32,
    pub missile_xholds_knockdown_1: i32,
    pub missile_xholds_knockdown_2: i32,
    pub missile_xholds_knockdown_3: i32,
    pub missile_xholds_knockdown_4: i32,
    pub missile_hn_to_xholds_0_max: i32,
    pub missile_hn_to_xholds_1_max: i32,
    pub missile_hn_to_xholds_2_max: i32,
    pub missile_hn_to_xholds_3_max: i32,
    pub missile_distance_for_half_chance_hit: i32,
    pub missile_distance_for_half_chance_hit_artillery: i32,
    pub missile_distance_for_half_chance_hit_naval: i32,
    pub projectile_damage_shield_divisor: i32,
    pub projectile_damage_armour_divisor: i32,
    pub projectile_damage_defense_divisor: i32,
    pub projectile_damage_distance_multiplier: f32,
    pub misfire_musket_matchlock: f32,
    pub misfire_musket_flintlock: f32,
    pub misfire_musket_percussion_cap: f32,
    pub misfire_cannon_matchlock: f32,
    pub misfire_cannon_flintlock: f32,
    pub projectile_calibration_target_area: f32,
    pub artillery_projectile_calibration_target_area: f32,
    pub naval_projectile_calibration_target_area: f32,
    pub land_mortar_projectile_calibration_target_area: f32,
    pub firing_drill_mass_fire_reload_modifier: i32,
    pub firing_drill_rank_fire_reload_modifier: i32,
    pub firing_drill_platoon_fire_reload_modifier: i32,
    pub firing_drill_improved_platoon_fire_reload_modifier: i32,
    pub fire_and_advance_reload_modifier: i32,
    pub fire_on_walls_accuracy_modifier: i32,
    pub fire_on_walls_range_modifier: i32,
    pub fire_on_walls_reload_modifier: i32,
    pub broadsides_damage_modifier: f32,
    pub ship_bonus_close_mod: f32,
    pub ship_bonus_range: i32,
    pub ship_penalty_far_mod: f32,
    pub ship_penalty_range: i32,
    pub magazine_explosion_interrupt_chance: f32,
    pub special_ability_artillery_rof_boost: f32,
    pub special_ability_artillery_accuracy_boost: f32,
    pub special_ability_inspire_unit_marksmanship_bonus: f32,
    pub special_ability_inspire_unit_melee_attack_bonus: f32,
    pub ship_repair_rate: f32,
}

impl KvRules {
    /// `(knockdown, knockback, stepback)` offsets for xholds tier `0..=4` (W1 §12.9).
    /// Tiers above 4 are treated as 4.
    pub fn melee_xholds(&self, tier: usize) -> (i32, i32, i32) {
        match tier {
            0 => (
                self.melee_xholds_knockdown_0,
                self.melee_xholds_knockback_0,
                self.melee_xholds_stepback_0,
            ),
            1 => (
                self.melee_xholds_knockdown_1,
                self.melee_xholds_knockback_1,
                self.melee_xholds_stepback_1,
            ),
            2 => (
                self.melee_xholds_knockdown_2,
                self.melee_xholds_knockback_2,
                self.melee_xholds_stepback_2,
            ),
            3 => (
                self.melee_xholds_knockdown_3,
                self.melee_xholds_knockback_3,
                self.melee_xholds_stepback_3,
            ),
            _ => (
                self.melee_xholds_knockdown_4,
                self.melee_xholds_knockback_4,
                self.melee_xholds_stepback_4,
            ),
        }
    }
}

/// C-style integer division (truncates toward zero) that does not panic.
///
/// The original would fault on a zero divisor; we return 0 instead so that an all-zero
/// (unloaded) `KvRules` cannot crash the simulation. `i32::MIN / -1` wraps like 32-bit x86 would
/// not (it faults there too); it is unreachable with real data.
pub fn c_div(a: i32, b: i32) -> i32 {
    if b == 0 { 0 } else { a.wrapping_div(b) }
}

/// Rounds like the x87 FPU in its default mode: to nearest, **ties to even** (W1 §12.9 "x87
/// default rounding"), then converts to `i32` (saturating; NaN → 0).
pub fn x87_round(x: f32) -> i32 {
    x.round_ties_even() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_div_truncates_toward_zero() {
        assert_eq!(c_div(7, 2), 3);
        assert_eq!(c_div(-7, 2), -3);
        assert_eq!(c_div(5, 0), 0);
    }

    #[test]
    fn x87_round_ties_to_even() {
        assert_eq!(x87_round(0.5), 0);
        assert_eq!(x87_round(1.5), 2);
        assert_eq!(x87_round(2.5), 2);
        assert_eq!(x87_round(-2.5), -2);
        assert_eq!(x87_round(2.6), 3);
        assert_eq!(x87_round(f32::NAN), 0);
    }

    #[test]
    fn xholds_tiers() {
        let r = KvRules {
            melee_xholds_knockdown_2: 1,
            melee_xholds_knockback_2: 2,
            melee_xholds_stepback_2: 3,
            melee_xholds_stepback_4: 9,
            ..Default::default()
        };
        assert_eq!(r.melee_xholds(2), (1, 2, 3));
        assert_eq!(r.melee_xholds(4).2, 9);
        assert_eq!(r.melee_xholds(7).2, 9);
    }
}

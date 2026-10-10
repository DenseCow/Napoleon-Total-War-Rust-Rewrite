//! Key-value tables: `_kv_rules`, `_kv_morale` and `_kv_fatigue`.
//!
//! # The format
//! Each row is `{ string key, f32 value }`; the table is a list of named tuning constants.
//!
//! # How the game reads them (Worker 1 §9, `kv_layout.tsv`, CONFIRMED)
//! The exe does **not** keep the floats. For most keys the loader converts the value to an
//! integer with the x86 instruction `cvttss2si` (at 0x00F3A899), which *truncates toward
//! zero*: 2.9 → 2, -2.9 → -2. Only a short list of `_kv_rules` keys stays as floats.
//! [`exe_truncate`] reproduces `cvttss2si` exactly, including its out-of-range result.
//!
//! * `_kv_morale` → [`KvMorale`] (from `ntw_sim`), all integers.
//! * `_kv_fatigue` → [`KvFatigue`] (from `ntw_sim`), all integers.
//! * `_kv_rules` → [`KvRules`], read per key with the exe's int/float choice ([`KV_RULES_KEYS`]).
//!
//! Rows the exe does not ask for (the files have a few extra) are kept in [`KvTable`] but unused.

use std::collections::HashMap;

use ntw_formats::db::{DbTable, FieldType, Schema};
use ntw_sim::battle::fatigue::KvFatigue;
use ntw_sim::battle::morale::KvMorale;

use crate::DataError;

/// Converts a float to an integer exactly like the x86 `cvttss2si` instruction does.
///
/// It truncates toward zero. NaN and anything outside the `i32` range give `i32::MIN`
/// (`0x80000000`, the "integer indefinite" value), whereas Rust's `as i32` would saturate.
pub fn exe_truncate(v: f32) -> i32 {
    // -2^31 is exactly representable; 2^31 is the first value that does not fit.
    if v.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&v) { i32::MIN } else { v as i32 }
}

/// A raw key-value table: every row of the file, in order, with lookup by key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KvTable {
    /// The table name, e.g. `"_kv_morale"`.
    pub name: &'static str,
    entries: Vec<(String, f32)>,
    index: HashMap<String, usize>,
}

impl KvTable {
    /// The file layout: one string, one float.
    pub fn schema() -> Schema {
        Schema::new().field(FieldType::Str).field(FieldType::F32)
    }

    /// Decodes a kv table file.
    pub fn from_bytes(name: &'static str, bytes: &[u8]) -> Result<Self, DataError> {
        let raw = DbTable::read(bytes, &Self::schema()).map_err(|error| DataError::Db { table: name, error })?;
        let mut entries = Vec::with_capacity(raw.rows.len());
        for (row, cells) in raw.rows.iter().enumerate() {
            let key = cells[0].as_str().ok_or(DataError::BadColumn { table: name, row, field: "key" })?;
            let value = cells[1].as_f32().ok_or(DataError::BadColumn { table: name, row, field: "value" })?;
            entries.push((key.to_owned(), value));
        }
        Ok(Self::from_entries(name, entries))
    }

    /// Builds a table from `(key, value)` pairs made in code (used by the test fixture).
    /// If a key repeats, the first one wins for lookups.
    pub fn from_entries(name: &'static str, entries: Vec<(String, f32)>) -> Self {
        let mut index = HashMap::with_capacity(entries.len());
        for (i, (k, _)) in entries.iter().enumerate() {
            index.entry(k.clone()).or_insert(i);
        }
        Self { name, entries, index }
    }

    /// Merges the files of one kv table by key, with the same rule as
    /// [`crate::record::Table::merged`].
    pub fn merged(name: &'static str, files: Vec<Self>, replaces: impl Fn(usize, usize) -> bool) -> Self {
        let entries = ntw_formats::db_folder::merge_keyed(
            files.into_iter().map(|t| t.entries).collect(),
            |(k, _): &(String, f32)| k.as_str(),
            replaces,
        );
        Self::from_entries(name, entries)
    }

    /// The stored float, exactly as in the file.
    pub fn raw(&self, key: &str) -> Option<f32> {
        self.index.get(key).map(|&i| self.entries[i].1)
    }

    /// The value as the exe sees an integer key: truncated with [`exe_truncate`].
    pub fn int(&self, key: &str) -> Option<i32> {
        self.raw(key).map(exe_truncate)
    }

    /// Like [`int`](Self::int), but a missing key is an error.
    pub fn require_int(&self, key: &'static str) -> Result<i32, DataError> {
        self.int(key).ok_or(DataError::MissingKvKey { table: self.name, key })
    }

    /// All rows in file order.
    pub fn entries(&self) -> &[(String, f32)] {
        &self.entries
    }
}

/// How the exe stores one `_kv_rules` value (W1 `kv_layout.tsv`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KvType {
    /// Truncated to an integer on load (`cvttss2si`).
    Int,
    /// Kept as a float.
    Float,
    /// Stored by a third adder (0x00F3A8F0) whose conversion is UNKNOWN; kept as the raw float here.
    Other,
}

/// A value read from `_kv_rules` with the exe's conversion applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KvValue {
    /// An integer key (already truncated).
    Int(i32),
    /// A float key.
    Float(f32),
}

impl KvValue {
    /// The value as a float (integers converted back).
    pub fn as_f32(self) -> f32 {
        match self {
            Self::Int(i) => i as f32,
            Self::Float(f) => f,
        }
    }
}

/// Every key the exe reads from `_kv_rules`, in holder order, with its storage type
/// (W1 `kv_layout.tsv`, CONFIRMED). These are names only; no game values.
///
/// The order is the exe's own load order in `0x00F42950`, and the exe reads key `i` through a
/// rules adapter whose vtable method `+4*i` returns it (CONFIRMED, BATTLE_FIDELITY.md §57 (2)), so
/// this list doubles as the slot index. `relative_melee_experience_multiplier` is index 0 and the
/// melee code never reads slot 0: the original's melee has no experience term.
pub const KV_RULES_KEYS: &[(&str, KvType)] = {
    use KvType::{Float as F, Int as I, Other as O};
    &[
        ("relative_melee_experience_multiplier", I), // slot 0: never read (see the note above)
        ("relative_melee_fatigue_multiplier", I),    // slot 4 read by the "Melee Fatigue Factor"
        ("melee_charge_factor_power_divisor", I),
        ("melee_entrenchement_level_multiplier", I),
        ("melee_height_delta_min", F),
        ("melee_height_delta_max", F),
        ("relative_melee_height_delta_divisor", F),
        ("hnbonus_bayonet", I),
        ("hnbonus_melee_cavalry_v_infantry", I),
        ("hnbonus_melee_cavalry_v_squareinfantry", I),
        ("missile_cover_factor_none", I),
        ("missile_cover_factor_slight", I),
        ("missile_cover_factor_some", I),
        ("missile_cover_factor_good", I),
        ("missile_cover_factor_excellent", I),
        ("armour_missile_penetrating_divisor", I),
        ("armour_missile_piercing_divisor", I),
        ("armour_melee_penetrating_divisor", I),
        ("armour_melee_piercing_divisor", I),
        ("defense_missile_penetrating_divisor", I),
        ("defense_missile_piercing_divisor", I),
        ("defense_melee_penetrating_divisor", I),
        ("defense_melee_piercing_divisor", I),
        ("factor_attackdir_flankleft", I),
        ("factor_attackdir_flankright", I),
        ("factor_attackdir_rear", I),
        ("factor_attackdir_front", I),
        ("attackpower_long_range_multiplier", F),
        ("attackpower_extreme_range_multplier", F),
        ("bayonet_melee_attack_bonus", I),
        ("bayonet_ring_reload_time_penalty", F),
        ("melee_xholds_knockdown_0", I),
        ("melee_xholds_knockback_0", I),
        ("melee_xholds_stepback_0", I),
        ("melee_xholds_knockdown_1", I),
        ("melee_xholds_knockback_1", I),
        ("melee_xholds_stepback_1", I),
        ("melee_xholds_knockdown_2", I),
        ("melee_xholds_knockback_2", I),
        ("melee_xholds_stepback_2", I),
        ("melee_xholds_knockdown_3", I),
        ("melee_xholds_knockback_3", I),
        ("melee_xholds_stepback_3", I),
        ("melee_xholds_knockdown_4", I),
        ("melee_xholds_knockback_4", I),
        ("melee_xholds_stepback_4", I),
        ("melee_hn_to_xholds_0_max", I),
        ("melee_hn_to_xholds_1_max", I),
        ("melee_hn_to_xholds_2_max", I),
        ("melee_hn_to_xholds_3_max", I),
        ("missile_xholds_knockdown_0", I),
        ("missile_xholds_knockdown_1", I),
        ("missile_xholds_knockdown_2", I),
        ("missile_xholds_knockdown_3", I),
        ("missile_xholds_knockdown_4", I),
        ("missile_hn_to_xholds_0_max", I),
        ("missile_hn_to_xholds_1_max", I),
        ("missile_hn_to_xholds_2_max", I),
        ("missile_hn_to_xholds_3_max", I),
        ("missile_distance_for_half_chance_hit", I),
        ("missile_distance_for_half_chance_hit_artillery", I),
        ("missile_distance_for_half_chance_hit_naval", I),
        ("projectile_damage_shield_divisor", I),
        ("projectile_damage_armour_divisor", I),
        ("projectile_damage_defense_divisor", I),
        ("projectile_damage_distance_multiplier", F),
        ("misfire_musket_matchlock", F),
        ("misfire_musket_flintlock", F),
        ("misfire_musket_percussion_cap", F),
        ("misfire_cannon_matchlock", F),
        ("misfire_cannon_flintlock", F),
        ("projectile_calibration_target_area", F),
        ("artillery_projectile_calibration_target_area", F),
        ("naval_projectile_calibration_target_area", F),
        ("land_mortar_projectile_calibration_target_area", F),
        ("firing_drill_mass_fire_reload_modifier", I),
        ("firing_drill_rank_fire_reload_modifier", I),
        ("firing_drill_platoon_fire_reload_modifier", I),
        ("firing_drill_improved_platoon_fire_reload_modifier", I),
        ("fire_and_advance_reload_modifier", I),
        ("fire_on_walls_accuracy_modifier", I),
        ("fire_on_walls_range_modifier", I),
        ("fire_on_walls_reload_modifier", I),
        ("broadsides_damage_modifier", F),
        ("ship_bonus_close_mod", F),
        // W1 to confirm: ship_bonus_range (0.35 in the shipped file) and ship_penalty_range (0.8)
        // are typed int(trunc), so the exe would read both as 0. Kept as the exe behaviour for now.
        ("ship_bonus_range", I),
        ("ship_penalty_far_mod", F),
        ("ship_penalty_range", I),
        ("magazine_explosion_interrupt_chance", F),
        ("special_ability_artillery_rof_boost", O),
        ("special_ability_artillery_accuracy_boost", O),
        ("special_ability_inspire_unit_marksmanship_bonus", O),
        ("special_ability_inspire_unit_melee_attack_bonus", O),
        ("ship_repair_rate", F),
    ]
};

/// `_kv_rules` with the exe's per-key int/float conversion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KvRules {
    /// The raw rows.
    pub table: KvTable,
}

impl KvRules {
    /// How the exe stores `key`, or `None` if the exe never reads it.
    pub fn key_type(key: &str) -> Option<KvType> {
        KV_RULES_KEYS.iter().find(|(k, _)| *k == key).map(|&(_, t)| t)
    }

    /// The value as the exe sees it: truncated for integer keys, the float otherwise.
    /// Keys the exe does not read are returned as their raw float.
    pub fn get(&self, key: &str) -> Option<KvValue> {
        let raw = self.table.raw(key)?;
        Some(match Self::key_type(key) {
            Some(KvType::Int) => KvValue::Int(exe_truncate(raw)),
            _ => KvValue::Float(raw),
        })
    }

    /// An integer key's value (`None` if missing or not an integer key).
    pub fn int(&self, key: &str) -> Option<i32> {
        match self.get(key)? {
            KvValue::Int(i) => Some(i),
            KvValue::Float(_) => None,
        }
    }

    /// A float key's value (`None` if missing or an integer key).
    pub fn float(&self, key: &str) -> Option<f32> {
        match self.get(key)? {
            KvValue::Float(f) => Some(f),
            KvValue::Int(_) => None,
        }
    }

    /// Fails if any key the exe reads is missing.
    pub fn check_complete(&self) -> Result<(), DataError> {
        for &(key, _) in KV_RULES_KEYS {
            if self.table.raw(key).is_none() {
                return Err(DataError::MissingKvKey { table: self.table.name, key });
            }
        }
        Ok(())
    }
}

/// Builds `$ty` from a kv table, one truncated integer per field (field name = kv key), and
/// also defines a constant listing the keys.
macro_rules! kv_struct {
    ($fn_name:ident, $keys:ident, $ty:ident, [$($f:ident),* $(,)?]) => {
        /// Every key of this table that the exe reads, in game-data order (names only).
        pub const $keys: &[&str] = &[$(stringify!($f)),*];

        /// Builds the `ntw_sim` struct from the raw table, truncating every value like the exe.
        /// Fails if a key is missing.
        pub fn $fn_name(table: &KvTable) -> Result<$ty, DataError> {
            Ok($ty { $( $f: table.require_int(stringify!($f))?, )* })
        }
    };
}

kv_struct!(kv_morale_from_table, KV_MORALE_KEYS, KvMorale, [
    morale_base,
    ums_impetuous_threshold_lower,
    ums_eager_threshold_upper,
    ums_eager_threshold_lower,
    ums_confident_threshold_upper,
    ums_confident_threshold_lower,
    ums_steady_threshold_upper,
    ums_steady_threshold_lower,
    ums_shaken_threshold_upper,
    ums_shaken_threshold_lower,
    ums_wavering_threshold_upper,
    ums_wavering_threshold_lower,
    ums_broken_threshold_upper,
    ums_broken_threshold_lower,
    charge_bonus,
    charge_timeout,
    ume_encouraged_fortification,
    ume_encouraged_fortification_compromised,
    waver_base_timeout,
    broken_finish_base_timeout,
    surprise_timeout,
    routing_unit_effect_distance_front,
    routing_unit_effect_distance_flank,
    fear_effect_range,
    recent_casualties_shock_threshold,
    recent_casualties_penalty_6,
    recent_casualties_penalty_10,
    recent_casualties_penalty_15,
    recent_casualties_penalty_33,
    recent_casualties_penalty_50,
    extended_casualties_penalty_10,
    extended_casualties_penalty_15,
    extended_casualties_penalty_33,
    extended_casualties_penalty_50,
    extended_casualties_penalty_80,
    total_casualties_penalty_20,
    total_casualties_penalty_40,
    total_casualties_penalty_60,
    total_casualties_penalty_80,
    total_casualties_penalty_90,
    ume_concerned_attacked_by_artillery,
    ume_concerned_attacked_by_projectile,
    ume_concerned_surprised,
    ume_concerned_panic,
    ume_encouraged_flanks_secure,
    ume_concerned_flanks_exposed_single,
    ume_concerned_flanks_exposed_multiple,
    ume_concerned_army_destruction,
    ume_concerned_general_dead,
    ume_concerned_general_fled_recently,
    ume_concerned_general_died_recently,
    ume_encouraged_on_the_hill,
    ume_concerned_unit_frightened,
    ume_concerned_horses_frightened,
    ume_encouraged_inspired,
    ume_concerned_tired,
    ume_concerned_very_tired,
    ume_concerned_exhausted,
    was_attacked_in_front,
    was_attacked_in_flank,
    was_attacked_in_rear,
    fighting_cavalry,
    blood_bonus_5,
    blood_bonus_7,
    blood_bonus_12,
    ume_encouraged_column_formation,
    special_ability_inspire_unit_bonus,
    special_ability_rally_min_bonus,
    special_ability_rally_max_bonus,
]);

kv_struct!(kv_fatigue_from_table, KV_FATIGUE_KEYS, KvFatigue, [
    idle,
    idle_in_building,
    idle_rain,
    idle_snow,
    limbering,
    ready,
    shooting,
    reloading,
    reloading_artillery,
    working,
    walking,
    walking_artillery,
    walking_horse_artillery,
    running,
    running_artillery_horse,
    running_cavalry,
    running_cavalry_light,
    charging,
    combat,
    tight_formation,
    under_fire_artillery,
    under_fire_small_arms,
    threshold_fresh,
    threshold_active,
    threshold_winded,
    threshold_tired,
    threshold_very_tired,
    threshold_exhausted,
    threshold_max,
    gradient_shallow_movement_multiplier,
    gradient_steep_movement_multiplier,
    gradient_very_steep_movement_multiplier,
]);

impl TryFrom<&KvTable> for KvMorale {
    type Error = DataError;
    fn try_from(table: &KvTable) -> Result<Self, DataError> {
        kv_morale_from_table(table)
    }
}

impl TryFrom<&KvTable> for KvFatigue {
    type Error = DataError;
    fn try_from(table: &KvTable) -> Result<Self, DataError> {
        kv_fatigue_from_table(table)
    }
}

// ---------------------------------------------------------------------------------------------
// _kv_rules -> ntw_sim::battle::rules::KvRules
// ---------------------------------------------------------------------------------------------

/// A field type of the simulation's `KvRules`: how a raw kv float becomes that field.
pub trait FromKv: Sized {
    /// The storage kind this Rust type stands for (`i32` = [`KvType::Int`], `f32` = float).
    const KIND: KvType;
    /// Converts the raw float the way the exe does.
    fn from_kv(raw: f32) -> Self;
}

impl FromKv for i32 {
    const KIND: KvType = KvType::Int;
    fn from_kv(raw: f32) -> Self {
        exe_truncate(raw)
    }
}

impl FromKv for f32 {
    const KIND: KvType = KvType::Float;
    fn from_kv(raw: f32) -> Self {
        raw
    }
}

fn rules_field<T: FromKv>(table: &KvTable, key: &'static str) -> Result<T, DataError> {
    table.raw(key).map(T::from_kv).ok_or(DataError::MissingKvKey { table: table.name, key })
}

/// The storage kind of a struct field, found from its type.
fn kind_of<T: FromKv>(_: &T) -> KvType {
    T::KIND
}

/// Generates the `_kv_rules` → `SimKvRules` conversion. A struct literal must name every
/// field, so a new field in `ntw_sim` without a key here is a compile error.
macro_rules! kv_rules_sim {
    ([$($f:ident),* $(,)?]) => {
        /// Builds the simulation's `KvRules` from `_kv_rules`: integer fields are truncated
        /// like the exe, float fields are kept. Fails if a key is missing.
        pub fn kv_rules_sim_from_table(table: &KvTable) -> Result<SimKvRules, DataError> {
            Ok(SimKvRules { $( $f: rules_field(table, stringify!($f))?, )* })
        }

        /// Each sim field's name and the storage kind implied by its Rust type.
        pub fn kv_rules_sim_field_kinds() -> Vec<(&'static str, KvType)> {
            let v = SimKvRules::default();
            vec![$( (stringify!($f), kind_of(&v.$f)) ),*]
        }
    };
}

pub use ntw_sim::battle::rules::KvRules as SimKvRules;

kv_rules_sim!([
    relative_melee_experience_multiplier,
    relative_melee_fatigue_multiplier,
    melee_charge_factor_power_divisor,
    melee_entrenchement_level_multiplier,
    melee_height_delta_min,
    melee_height_delta_max,
    relative_melee_height_delta_divisor,
    hnbonus_bayonet,
    hnbonus_melee_cavalry_v_infantry,
    hnbonus_melee_cavalry_v_squareinfantry,
    missile_cover_factor_none,
    missile_cover_factor_slight,
    missile_cover_factor_some,
    missile_cover_factor_good,
    missile_cover_factor_excellent,
    armour_missile_penetrating_divisor,
    armour_missile_piercing_divisor,
    armour_melee_penetrating_divisor,
    armour_melee_piercing_divisor,
    defense_missile_penetrating_divisor,
    defense_missile_piercing_divisor,
    defense_melee_penetrating_divisor,
    defense_melee_piercing_divisor,
    factor_attackdir_flankleft,
    factor_attackdir_flankright,
    factor_attackdir_rear,
    factor_attackdir_front,
    attackpower_long_range_multiplier,
    attackpower_extreme_range_multplier,
    bayonet_melee_attack_bonus,
    bayonet_ring_reload_time_penalty,
    melee_xholds_knockdown_0,
    melee_xholds_knockback_0,
    melee_xholds_stepback_0,
    melee_xholds_knockdown_1,
    melee_xholds_knockback_1,
    melee_xholds_stepback_1,
    melee_xholds_knockdown_2,
    melee_xholds_knockback_2,
    melee_xholds_stepback_2,
    melee_xholds_knockdown_3,
    melee_xholds_knockback_3,
    melee_xholds_stepback_3,
    melee_xholds_knockdown_4,
    melee_xholds_knockback_4,
    melee_xholds_stepback_4,
    melee_hn_to_xholds_0_max,
    melee_hn_to_xholds_1_max,
    melee_hn_to_xholds_2_max,
    melee_hn_to_xholds_3_max,
    missile_xholds_knockdown_0,
    missile_xholds_knockdown_1,
    missile_xholds_knockdown_2,
    missile_xholds_knockdown_3,
    missile_xholds_knockdown_4,
    missile_hn_to_xholds_0_max,
    missile_hn_to_xholds_1_max,
    missile_hn_to_xholds_2_max,
    missile_hn_to_xholds_3_max,
    missile_distance_for_half_chance_hit,
    missile_distance_for_half_chance_hit_artillery,
    missile_distance_for_half_chance_hit_naval,
    projectile_damage_shield_divisor,
    projectile_damage_armour_divisor,
    projectile_damage_defense_divisor,
    projectile_damage_distance_multiplier,
    misfire_musket_matchlock,
    misfire_musket_flintlock,
    misfire_musket_percussion_cap,
    misfire_cannon_matchlock,
    misfire_cannon_flintlock,
    projectile_calibration_target_area,
    artillery_projectile_calibration_target_area,
    naval_projectile_calibration_target_area,
    land_mortar_projectile_calibration_target_area,
    firing_drill_mass_fire_reload_modifier,
    firing_drill_rank_fire_reload_modifier,
    firing_drill_platoon_fire_reload_modifier,
    firing_drill_improved_platoon_fire_reload_modifier,
    fire_and_advance_reload_modifier,
    fire_on_walls_accuracy_modifier,
    fire_on_walls_range_modifier,
    fire_on_walls_reload_modifier,
    broadsides_damage_modifier,
    ship_bonus_close_mod,
    // W1 to confirm: these two are int(trunc), so 0.35 / 0.8 in the shipped file become 0.
    ship_bonus_range,
    ship_penalty_range,
    ship_penalty_far_mod,
    magazine_explosion_interrupt_chance,
    special_ability_artillery_rof_boost,
    special_ability_artillery_accuracy_boost,
    special_ability_inspire_unit_marksmanship_bonus,
    special_ability_inspire_unit_melee_attack_bonus,
    ship_repair_rate,
]);

impl TryFrom<&KvTable> for SimKvRules {
    type Error = DataError;
    fn try_from(table: &KvTable) -> Result<Self, DataError> {
        kv_rules_sim_from_table(table)
    }
}

impl TryFrom<&KvRules> for SimKvRules {
    type Error = DataError;
    fn try_from(rules: &KvRules) -> Result<Self, DataError> {
        kv_rules_sim_from_table(&rules.table)
    }
}

/// The highest `_kv_rules` list index [`game_limits`] reads: its lists hold unit-size steps and
/// army sizes (four and three entries in the original), so 1,024 is far above any real list.
pub const MAX_LIST_INDEX: usize = 1023;

/// The gameplay caps from the merged `_kv_rules` ([`ntw_sim::limits`]: our own keys, which the exe
/// never reads; the original's values where a key is missing). A value that is not a usable
/// number (not finite, a count below 1, a scale not above 0, negative funds) keeps the original's
/// and is described in `warnings`; a list runs to its highest index present, a missing entry
/// keeping the original's.
pub fn game_limits(table: &KvTable, warnings: &mut Vec<String>) -> ntw_sim::limits::GameLimits {
    use ntw_sim::limits::*;
    let d = GameLimits::default();
    let count = |v: f32| v.is_finite() && v >= 1.0 && v <= u32::MAX as f32;
    let scale = |v: f32| v.is_finite() && v > 0.0;
    let funds = |v: f32| v.is_finite() && (0.0..=i32::MAX as f32).contains(&v);
    // `<key>_<i>` for every i up to the highest index present, over `default`: a missing or
    // unusable entry keeps the default's; past the default's end a missing run repeats the entry
    // before it (one warning per run), so a mod may ship only the entries it changes or adds.
    // Indices above `MAX_LIST_INDEX` are ignored (one warning per list): a typo such as
    // `unit_scale_4000000000` must not fill billions of entries.
    let mut list = |key: &str, default: Vec<f32>, ok: fn(f32) -> bool| -> Vec<f32> {
        let prefix = format!("{key}_");
        let indices: Vec<usize> = table.entries().iter().filter_map(|(k, _)| k.strip_prefix(&prefix)?.parse::<usize>().ok()).collect();
        let past = indices.iter().filter(|&&i| i > MAX_LIST_INDEX).count();
        if past > 0 {
            warnings.push(format!("_kv_rules {key}_<i>: {past} entries above index {MAX_LIST_INDEX} are ignored (a list holds unit-size steps or army sizes)"));
        }
        let mut out = default;
        let Some(top) = indices.into_iter().filter(|&i| i <= MAX_LIST_INDEX).max() else { return out };
        let mut gap: Option<(usize, f32)> = None;
        for i in 0..=top {
            let k = format!("{prefix}{i}");
            let raw = table.raw(&k);
            if let Some(v) = raw.filter(|&v| !ok(v)) {
                warnings.push(format!("_kv_rules {k} = {v} is not a usable limit; the original's value is kept"));
            }
            let usable = raw.filter(|&v| ok(v));
            if usable.is_some()
                && let Some((from, prev)) = gap.take()
            {
                warnings.push(format!("_kv_rules {prefix}{from}..{prefix}{} are missing below a higher index; they repeat {prev}", i - 1));
            }
            match (usable, i < out.len()) {
                (Some(v), true) => out[i] = v,
                (Some(v), false) => out.push(v),
                (None, true) => {}
                (None, false) => {
                    let Some(&prev) = out.last() else { break };
                    gap.get_or_insert((i, prev));
                    out.push(prev);
                }
            }
        }
        if let Some((from, prev)) = gap {
            warnings.push(format!("_kv_rules {prefix}{from}..{prefix}{top} are missing or unusable; they repeat {prev}"));
        }
        out
    };
    let floats = |v: &[u32]| v.iter().map(|&x| x as f32).collect::<Vec<_>>();
    let unit_scales = list(KEY_UNIT_SCALE, d.unit_scales.clone(), scale);
    let max_naval_units = list(KEY_MAX_NAVAL_UNITS, floats(&d.max_naval_units), count).into_iter().map(|v| v as u32).collect();
    let as_i32 = |v: Vec<f32>| v.into_iter().map(|v| v as i32).collect::<Vec<_>>();
    let funds_land = as_i32(list(KEY_CUSTOM_BATTLE_FUNDS_LAND, d.custom_battle_funds_land.iter().map(|&x| x as f32).collect(), funds));
    let funds_naval = as_i32(list(KEY_CUSTOM_BATTLE_FUNDS_NAVAL, d.custom_battle_funds_naval.iter().map(|&x| x as f32).collect(), funds));
    let mut single = |key: &str, default: u32| match table.raw(key) {
        Some(v) if count(v) => v as u32,
        Some(v) => {
            warnings.push(format!("_kv_rules {key} = {v} is not a usable limit; the original's value is kept"));
            default
        }
        None => default,
    };
    GameLimits {
        unit_scales,
        max_land_units: single(KEY_MAX_LAND_UNITS, d.max_land_units),
        max_naval_units,
        custom_battle_funds_land: funds_land,
        custom_battle_funds_naval: funds_naval,
        max_reinforcement_units: single(KEY_MAX_REINFORCEMENT_UNITS, d.max_reinforcement_units),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_matches_cvttss2si() {
        assert_eq!(exe_truncate(2.9), 2);
        assert_eq!(exe_truncate(-2.9), -2);
        assert_eq!(exe_truncate(0.999), 0);
        assert_eq!(exe_truncate(-0.5), 0);
        assert_eq!(exe_truncate(-2_147_483_648.0), i32::MIN);
        assert_eq!(exe_truncate(3e9), i32::MIN);
        assert_eq!(exe_truncate(-3e9), i32::MIN);
        assert_eq!(exe_truncate(f32::NAN), i32::MIN);
        assert_eq!(exe_truncate(f32::INFINITY), i32::MIN);
    }

    /// Builds a kv table file: flag, row count, then {string, f32} rows. Values are made up.
    fn kv_bytes(rows: &[(&str, f32)]) -> Vec<u8> {
        let mut b = vec![1];
        b.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        for (k, v) in rows {
            b.extend_from_slice(&(k.len() as u16).to_le_bytes());
            for c in k.encode_utf16() {
                b.extend_from_slice(&c.to_le_bytes());
            }
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn decodes_and_truncates() {
        let t = KvTable::from_bytes("_kv_test", &kv_bytes(&[("a", 7.75), ("b", -1.5)])).unwrap();
        assert_eq!((t.raw("a"), t.int("a"), t.int("b")), (Some(7.75), Some(7), Some(-1)));
        assert!(matches!(t.require_int("zzz"), Err(DataError::MissingKvKey { key: "zzz", .. })));
    }

    #[test]
    fn rules_keep_floats_only_where_the_exe_does() {
        let t = KvTable::from_entries(
            "_kv_rules",
            vec![("hnbonus_bayonet".into(), 3.7), ("misfire_musket_flintlock".into(), 0.25), ("extra".into(), 1.5)],
        );
        let rules = KvRules { table: t };
        assert_eq!(rules.get("hnbonus_bayonet"), Some(KvValue::Int(3)));
        assert_eq!(rules.float("misfire_musket_flintlock"), Some(0.25));
        assert_eq!(rules.int("misfire_musket_flintlock"), None);
        assert_eq!(rules.get("extra"), Some(KvValue::Float(1.5)));
        assert!(rules.check_complete().is_err());
        assert_eq!(KV_RULES_KEYS.len(), 94);
    }

    /// Every sim field has a W1 key, every W1 key has a sim field, and the Rust type agrees
    /// with the exe's int/float rule (the unknown "other" keys are kept as f32).
    #[test]
    fn sim_kv_rules_fields_match_the_exe_types() {
        let kinds = kv_rules_sim_field_kinds();
        assert_eq!(kinds.len(), KV_RULES_KEYS.len());
        for (name, kind) in &kinds {
            let exe = KvRules::key_type(name).unwrap_or_else(|| panic!("{name} is not a W1 kv_rules key"));
            let expected = if exe == KvType::Int { KvType::Int } else { KvType::Float };
            assert_eq!(*kind, expected, "{name}");
        }
    }

    #[test]
    fn kv_rules_list_order_is_the_exe_slot_index() {
        // The exe reads key `i` of `kv_rules_table` through a rules adapter at vtable slot `+4*i`
        // (CONFIRMED, BATTLE_FIDELITY.md §57 (2)), so the list position is the slot. These five
        // positions are anchored by decompiled readers:
        //  slot 4  `0x00DAD990` the "Melee Fatigue Factor"
        //  slot 0x10 / 0x14 / 0x18  `0x00DAD9C0` the height min / max / divisor
        //  slot 0x5C..0x68  `0x00DAC7A0` the four attack-direction factors
        //  slot 0 is never read: no melee function uses it, so no experience term exists.
        for (slot, name) in [
            (0usize, "relative_melee_experience_multiplier"),
            (4, "relative_melee_fatigue_multiplier"),
            (0x10, "melee_height_delta_min"),
            (0x14, "melee_height_delta_max"),
            (0x18, "relative_melee_height_delta_divisor"),
            (0x5C, "factor_attackdir_flankleft"),
            (0x60, "factor_attackdir_flankright"),
            (0x64, "factor_attackdir_rear"),
            (0x68, "factor_attackdir_front"),
        ] {
            assert_eq!(KV_RULES_KEYS[slot / 4].0, name, "slot {slot:#x}");
        }
    }

    #[test]
    fn builds_sim_kv_rules_from_made_up_values() {
        // Made-up values: (index + 1) + 0.5, so every integer field is non-zero and truncated.
        let entries: Vec<(String, f32)> =
            KV_RULES_KEYS.iter().enumerate().map(|(i, (k, _))| (k.to_string(), i as f32 + 1.5)).collect();
        let rules = KvRules { table: KvTable::from_entries("_kv_rules", entries.clone()) };
        let sim = SimKvRules::try_from(&rules).unwrap();
        assert_eq!(sim.relative_melee_experience_multiplier, 1); // 1.5 -> 1
        assert_eq!(sim.melee_height_delta_min, 5.5); // float kept
        assert_eq!(sim.ship_repair_rate, 94.5);

        let mut missing = entries;
        missing.retain(|(k, _)| k != "armour_melee_piercing_divisor");
        assert!(matches!(
            SimKvRules::try_from(&KvTable::from_entries("_kv_rules", missing)),
            Err(DataError::MissingKvKey { key: "armour_melee_piercing_divisor", .. })
        ));
    }

    #[test]
    fn builds_sim_structs_from_made_up_values() {
        // Made-up values: key index + 0.9, so every value also tests truncation.
        let morale = KvTable::from_entries(
            "_kv_morale",
            KV_MORALE_KEYS.iter().enumerate().map(|(i, k)| (k.to_string(), i as f32 + 0.9)).collect(),
        );
        let m = KvMorale::try_from(&morale).unwrap();
        assert_eq!((m.morale_base, m.special_ability_rally_max_bonus), (0, 68));
        assert_eq!(KV_MORALE_KEYS.len(), 69);

        let mut rows: Vec<(String, f32)> =
            KV_FATIGUE_KEYS.iter().enumerate().map(|(i, k)| (k.to_string(), -(i as f32) - 0.5)).collect();
        let f = KvFatigue::try_from(&KvTable::from_entries("_kv_fatigue", rows.clone())).unwrap();
        assert_eq!((f.idle, f.ready), (0, -5));
        assert_eq!(KV_FATIGUE_KEYS.len(), 32);

        rows.retain(|(k, _)| k != "combat");
        assert!(matches!(
            KvFatigue::try_from(&KvTable::from_entries("_kv_fatigue", rows)),
            Err(DataError::MissingKvKey { key: "combat", .. })
        ));
    }

    #[test]
    fn game_limits_default_to_the_original_and_take_mod_rows() {
        use ntw_sim::limits::GameLimits;
        let mut warnings = Vec::new();
        // No limit rows (vanilla): the original's values.
        let vanilla = KvTable::from_entries("_kv_rules", vec![("ship_repair_rate".into(), 1.0)]);
        assert_eq!(game_limits(&vanilla, &mut warnings), GameLimits::default());
        assert!(warnings.is_empty());
        // A mod past the old limits: 40-unit armies, a 1.5 unit-size step, a fourth army size.
        let rows = [
            ("max_naval_units_1", 8.0),
            ("unit_scale_4", 1.5),
            ("unit_scale_0", 0.2),
            ("unit_scale_1", 0.5),
            ("unit_scale_2", 0.75),
            ("unit_scale_3", 1.0),
            ("max_land_units", 40.0),
            ("max_reinforcement_units", 30.0),
            ("max_naval_units_4", 40.0),
            ("max_naval_units_0", 6.0),
            ("custom_battle_funds_land_3", 20000.0),
            ("custom_battle_funds_land_0", 5000.0),
            ("custom_battle_funds_land_1", 10000.0),
            ("custom_battle_funds_land_2", 14000.0),
            ("custom_battle_funds_naval_0", -1.0),
        ];
        let modded = KvTable::from_entries("_kv_rules", rows.iter().map(|(k, v)| (k.to_string(), *v)).collect());
        let l = game_limits(&modded, &mut warnings);
        assert_eq!(l.unit_scales, [0.2, 0.5, 0.75, 1.0, 1.5]);
        assert_eq!((l.max_land_units, l.max_reinforcement_units), (40, 30));
        // `_2`, `_3` missing: they keep the original's; `_4` is appended.
        assert_eq!(l.max_naval_units, [6, 8, 10, 20, 40]);
        assert_eq!(l.custom_battle_funds_land, [5000, 10000, 14000, 20000]);
        // A negative fund keeps the original's and is reported.
        assert_eq!(l.custom_battle_funds_naval, [5000, 14000, 24000]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(l.custom_battle_funds(3, false), 20000);
        assert_eq!(l.max_units(1.5), (40, 40));
        // A mod shipping only the entry it changes or adds (vanilla has no `_0` row).
        let only = |k: &str, v: f32| KvTable::from_entries("_kv_rules", vec![(k.to_string(), v)]);
        let mut w = Vec::new();
        assert_eq!(game_limits(&only("max_naval_units_2", 15.0), &mut w).max_naval_units, [6, 8, 15, 20]);
        assert_eq!(game_limits(&only("unit_scale_4", 1.5), &mut w).unit_scales, [0.25, 0.5, 0.75, 1.0, 1.5]);
        assert!(w.is_empty(), "{w:?}");
        // A gap past the original's entries repeats the entry before it, reported.
        assert_eq!(game_limits(&only("unit_scale_5", 2.0), &mut w).unit_scales, [0.25, 0.5, 0.75, 1.0, 1.0, 2.0]);
        assert_eq!(w.len(), 1, "{w:?}");
        // A longer gap is one warning; a typo far past any list is ignored, once, without filling it.
        let mut w = Vec::new();
        assert_eq!(game_limits(&only("unit_scale_9", 3.0), &mut w).unit_scales, [0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 3.0]);
        assert_eq!(w.len(), 1, "{w:?}");
        let mut w = Vec::new();
        assert_eq!(game_limits(&only("unit_scale_4000000000", 3.0), &mut w), GameLimits::default());
        assert_eq!(w.len(), 1, "{w:?}");
    }
}

//! Land-unit morale: the 8-state machine and its modifiers.
//!
//! - [`evaluate`] is the full morale evaluation, W1 §12.3, `0x00584020` (CONFIRMED structure).
//! - [`modifiers`] is the casualty / attack-direction modifier pass, W1 §12.4, `0x0053C720`
//!   (thresholds CONFIRMED, key names INFERRED from the threshold numbers).
//! - [`light_update`] runs on the ticks where the full evaluation does not (`0x00585BE0`); its
//!   contents are UNKNOWN, so it is a placeholder.
//!
//! How it works in one sentence: every evaluation recomputes a whole-number morale value from
//! `morale_base` + the unit's morale stat + a list of named effects, and then moves the unit at most
//! **one** state up or down, using separate "upper" and "lower" thresholds per state (hysteresis)
//! so that a unit does not flicker between two states. All comparisons are **strict** (`<` / `>`).
//!
//! The threshold numbers themselves live in the game's `kv_morale` table; they are passed in as
//! [`KvMorale`] and are never hard-coded here.

/// Morale effect ids passed to `add_effect` (0x0054E3C0). The numbers are CONFIRMED from the code
/// (W1 §12.3 / §12.4); the names are ours, taken from the `kv_morale` key that supplies each value.
pub mod effect_id {
    /// Positive category bonus (+6 / +4 / +2). W1 §12.4 (CONFIRMED).
    pub const CATEGORY_BONUS: i32 = 0x05;
    /// Kill ratio ("blood") bonus. W1 §12.4 (CONFIRMED).
    pub const BLOOD: i32 = 0x06;
    /// Charge bonus while the charge timer runs. W1 §12.3 (CONFIRMED).
    pub const CHARGE: i32 = 0x0E;
    /// Negative category penalty (−4 / −8). W1 §12.4 (CONFIRMED).
    pub const CATEGORY_PENALTY: i32 = 0x17;
    /// Attacked in the front (per attacker). W1 §12.4 (CONFIRMED).
    pub const ATTACKED_FRONT: i32 = 0x18;
    /// Attacked in a flank. W1 §12.4 (CONFIRMED).
    pub const ATTACKED_FLANK: i32 = 0x19;
    /// Attacked in the rear. W1 §12.4 (CONFIRMED).
    pub const ATTACKED_REAR: i32 = 0x1A;
    /// Total casualties penalty. W1 §12.4 (CONFIRMED).
    pub const TOTAL_CASUALTIES: i32 = 0x20;
    /// Recent casualties penalty. W1 §12.4 (CONFIRMED).
    pub const RECENT_CASUALTIES: i32 = 0x21;
    /// Extended casualties penalty. W1 §12.4 (CONFIRMED).
    pub const EXTENDED_CASUALTIES: i32 = 0x22;
    /// Under artillery fire. W1 §12.3 (CONFIRMED).
    pub const UNDER_ARTILLERY: i32 = 0x23;
    /// Under projectile fire. W1 §12.3 (CONFIRMED).
    pub const UNDER_PROJECTILES: i32 = 0x24;
    /// Fighting cavalry. W1 §12.4 (CONFIRMED).
    pub const FIGHTING_CAVALRY: i32 = 0x25;
    /// Surprised. W1 §12.3 (CONFIRMED).
    pub const SURPRISED: i32 = 0x28;
}

// ---------------------------------------------------------------------------------------------
// States and component
// ---------------------------------------------------------------------------------------------

/// The 8 morale states (component field `[0xA]`, values 0..7). W1 §12.3 (CONFIRMED values).
///
/// Names are INFERRED from the `ums_*` threshold key prefixes; state 7 has no `ums_` key and is
/// named "Shattered" because it maps to behaviour mode 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MoraleState {
    /// 0, `ums_impetuous_*`.
    Impetuous = 0,
    /// 1, `ums_eager_*`.
    Eager = 1,
    /// 2, `ums_confident_*`.
    Confident = 2,
    /// 3, `ums_steady_*`.
    Steady = 3,
    /// 4, `ums_shaken_*`.
    Shaken = 4,
    /// 5, `ums_wavering_*`.
    Wavering = 5,
    /// 6, `ums_broken_*` (the unit routs).
    Broken = 6,
    /// 7, no way back (behaviour "shattered").
    Shattered = 7,
}

impl MoraleState {
    /// Converts the raw 0..7 value. Returns `None` for anything else.
    pub fn from_index(i: u8) -> Option<MoraleState> {
        use MoraleState::*;
        [
            Impetuous, Eager, Confident, Steady, Shaken, Wavering, Broken, Shattered,
        ]
        .get(i as usize)
        .copied()
    }
}

/// The behaviour mode (component field `[0xB]`). W1 §8 (CONFIRMED values; value 1 never seen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MoraleBehaviour {
    /// 0: fighting normally.
    Normal = 0,
    /// 2: routing (morale state 6).
    Routing = 2,
    /// 3: shattered (morale state 7).
    Shattered = 3,
    /// 4: a special case whose meaning is UNKNOWN (W1 §12.3 "[special case mode 4]").
    Special = 4,
    /// 5: routed by a script (`morale_behavior_rout`, `0x00555970` mode 2 sets state 7 and
    /// behaviour 5, CONFIRMED writes). INFERRED: the unit runs like a routing one.
    ScriptRout = 5,
}

/// One entry of the transient effect array (component `[0x8]/[0x9]`, 12 bytes each). W1 §8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransientEffect {
    /// Effect id.
    pub id: i32,
    /// Morale value added.
    pub value: i32,
    /// Seconds left (`f32`). CONFIRMED (`0x00585BE0`): every light update subtracts 0.1 and removes
    /// the entry once the result is `<= 0`.
    pub seconds_left: f32,
}

/// One node of the active-effect list (component `[0xF..0x10]`, a linked list in the original:
/// node `+0x8` id, `+0xC` sort key, `+0x10` value). Kept in the original's sorted order, see
/// [`MoraleComponent::add_effect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActiveEffect {
    /// Effect id (see [`effect_id`]).
    pub id: i32,
    /// Morale value of this effect.
    pub value: i32,
}

/// Most entries the active list takes: `add_effect` returns without adding once it holds more
/// than 42 (`cmp count, 0x2a; ja return`). CONFIRMED.
pub const MAX_ACTIVE_EFFECTS: usize = 43;

/// Sort key of an effect value (`0x0054E3C0`, CONFIRMED): `|v|` for `v >= 0`, `|2v|` for `v < 0`
/// (negative effects sort as if twice as large), 32-bit wrapping like the original.
pub fn effect_sort_key(value: i32) -> i32 {
    let v = if value < 0 { value.wrapping_mul(2) } else { value };
    v.wrapping_abs()
}

/// A unit's morale component, mirroring the `int[]` that `0x00584020` works on. W1 §8.
/// Offsets are CONFIRMED as accessed; meanings are INFERRED.
#[derive(Debug, Clone, PartialEq)]
pub struct MoraleComponent {
    /// `[0x8]/[0x9]`: transient effects `(id, value, ?)`, re-added on every full evaluation.
    pub transient_effects: Vec<TransientEffect>,
    /// `[0xA]`: the morale state.
    pub state: MoraleState,
    /// `[0xB]`: the behaviour mode.
    pub behaviour: MoraleBehaviour,
    /// `[0xC]`: the current morale value (an integer).
    pub morale: i32,
    /// `[0xD]`: persistent morale bonus.
    pub persistent_bonus: i32,
    /// `[0xF..0x10]`: the active effects summed into `morale`.
    pub active_effects: Vec<ActiveEffect>,
    /// `[0x17]`: surprise timer. Active while `>= 0`.
    pub surprise_timer: i32,
    /// `[0x18]`: broken/rout timer. State 6 can only change while it is `< 0`.
    pub rout_timer: i32,
    /// `[0x19]`: waver timer. State 5 can only change while it is `< 0`; the clamp needs `> 0`.
    pub waver_timer: i32,
    /// `[0x1A]`: charge-bonus timer. Active while `> 0`.
    pub charge_timer: i32,
    /// byte `+0x51`: "suppress shaken clamp" (set by surprise or recent-casualty shock).
    pub suppress_shaken_clamp: bool,
    /// bytes `+0x55`, `+0x56`, `+0x57`: if any is set the full evaluation returns at once. `+0x55`
    /// is set by `morale_behavior_fearless`, `+0x56` by `morale_behavior_rout` (and it blocks the
    /// rally), both cleared by `morale_behavior_default` (`0x00555970`, CONFIRMED writes; see
    /// [`set_script_morale`]). `+0x57` is UNKNOWN.
    pub skip_flags: [bool; 3],
    /// byte `+0x5A`: number of times this unit has routed (wraps at 255 like a C++ byte).
    pub times_routed: u8,
    /// byte `+0x54`: set when the unit rallies (`0x00572AC0`), cleared when its state improves to
    /// Shaken or better (CONFIRMED writes; INFERRED meaning "has rallied").
    pub rally_flag: bool,
}

impl Default for MoraleComponent {
    /// A fresh component as the unit constructor builds it (`0x0051D0F0`, CONFIRMED,
    /// BATTLE_FIDELITY.md §14): state 2 (Confident), behaviour 0, morale 0, all four timers −1,
    /// no effects, flags and rout count cleared. The persistent bonus depends on the army level
    /// ([`MoraleComponent::for_army_level`]); 0 here (army level 0).
    fn default() -> Self {
        MoraleComponent {
            transient_effects: Vec::new(),
            state: MoraleState::Confident,
            behaviour: MoraleBehaviour::Normal,
            morale: 0,
            persistent_bonus: 0,
            active_effects: Vec::new(),
            surprise_timer: -1,
            rout_timer: -1,
            waver_timer: -1,
            charge_timer: -1,
            suppress_shaken_clamp: false,
            skip_flags: [false; 3],
            times_routed: 0,
            rally_flag: false,
        }
    }
}

impl MoraleComponent {
    /// A fresh component for a unit of an army with the given level (army `+0x234`, from the
    /// army setup `+0x88`) and level flag (army `+0x224`, setup `+0x78`); the same pair drives the
    /// reload and melee-attack army bonuses. Persistent bonus `[0xD]` (`0x0051D0F0`, CONFIRMED):
    /// flag clear: level 1 → +2, 2 → +4, −1 → −1; flag set: level 1 → +3, −2 → −1; anything
    /// else 0. INFERRED: the level is a difficulty handicap.
    pub fn for_army_level(level: i32, flag: bool) -> Self {
        let persistent_bonus = match (flag, level) {
            (true, 1) => 3,
            (true, -2) => -1,
            (false, 1) => 2,
            (false, 2) => 4,
            (false, -1) => -1,
            _ => 0,
        };
        MoraleComponent { persistent_bonus, ..MoraleComponent::default() }
    }
}

impl MoraleComponent {
    /// Adds an effect to the active list (`0x0054E3C0`, CONFIRMED):
    /// - no de-duplication: the same id may appear several times, and all of them count;
    /// - nothing is added once the list holds more than 42 entries ([`MAX_ACTIVE_EFFECTS`]);
    /// - sorted insert, descending by [`effect_sort_key`]; a new entry goes before the first entry
    ///   with a smaller key (so after entries with an equal key).
    pub fn add_effect(&mut self, id: i32, value: i32) {
        if self.active_effects.len() >= MAX_ACTIVE_EFFECTS {
            return;
        }
        let key = effect_sort_key(value);
        let pos = self
            .active_effects
            .iter()
            .position(|e| effect_sort_key(e.value) < key)
            .unwrap_or(self.active_effects.len());
        self.active_effects.insert(pos, ActiveEffect { id, value });
    }

    /// Value of the active effect `id`, if present.
    pub fn effect(&self, id: i32) -> Option<i32> {
        self.active_effects
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.value)
    }

    /// Sum of all active effect values (wrapping like 32-bit C++).
    pub fn effects_sum(&self) -> i32 {
        self.active_effects
            .iter()
            .fold(0i32, |acc, e| acc.wrapping_add(e.value))
    }

    /// True if the unit is routing or shattered.
    pub fn is_routing_or_shattered(&self) -> bool {
        matches!(
            self.behaviour,
            MoraleBehaviour::Routing | MoraleBehaviour::Shattered | MoraleBehaviour::ScriptRout
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Game data (kv_morale)
// ---------------------------------------------------------------------------------------------

/// The `kv_morale` table: every key in game-data order (W1 `kv_layout.tsv`, CONFIRMED key list).
///
/// All values are integers because the loader truncates them (W1 §9, `cvttss2si` at 0x00F3A899),
/// except the `special_ability_*` keys whose third adder is not yet understood (kept as `i32` here).
///
/// **No numbers are hard-coded**: `Default` gives all zeros. Real values are loaded from the
/// player's game files; tests use obviously made-up placeholders.
#[allow(missing_docs)] // each field is named exactly like its kv_morale key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KvMorale {
    pub morale_base: i32,
    pub ums_impetuous_threshold_lower: i32,
    pub ums_eager_threshold_upper: i32,
    pub ums_eager_threshold_lower: i32,
    pub ums_confident_threshold_upper: i32,
    pub ums_confident_threshold_lower: i32,
    pub ums_steady_threshold_upper: i32,
    pub ums_steady_threshold_lower: i32,
    pub ums_shaken_threshold_upper: i32,
    pub ums_shaken_threshold_lower: i32,
    pub ums_wavering_threshold_upper: i32,
    pub ums_wavering_threshold_lower: i32,
    pub ums_broken_threshold_upper: i32,
    pub ums_broken_threshold_lower: i32,
    pub charge_bonus: i32,
    pub charge_timeout: i32,
    pub ume_encouraged_fortification: i32,
    pub ume_encouraged_fortification_compromised: i32,
    pub waver_base_timeout: i32,
    pub broken_finish_base_timeout: i32,
    pub surprise_timeout: i32,
    pub routing_unit_effect_distance_front: i32,
    pub routing_unit_effect_distance_flank: i32,
    pub fear_effect_range: i32,
    pub recent_casualties_shock_threshold: i32,
    pub recent_casualties_penalty_6: i32,
    pub recent_casualties_penalty_10: i32,
    pub recent_casualties_penalty_15: i32,
    pub recent_casualties_penalty_33: i32,
    pub recent_casualties_penalty_50: i32,
    pub extended_casualties_penalty_10: i32,
    pub extended_casualties_penalty_15: i32,
    pub extended_casualties_penalty_33: i32,
    pub extended_casualties_penalty_50: i32,
    pub extended_casualties_penalty_80: i32,
    pub total_casualties_penalty_20: i32,
    pub total_casualties_penalty_40: i32,
    pub total_casualties_penalty_60: i32,
    pub total_casualties_penalty_80: i32,
    pub total_casualties_penalty_90: i32,
    pub ume_concerned_attacked_by_artillery: i32,
    pub ume_concerned_attacked_by_projectile: i32,
    pub ume_concerned_surprised: i32,
    pub ume_concerned_panic: i32,
    pub ume_encouraged_flanks_secure: i32,
    pub ume_concerned_flanks_exposed_single: i32,
    pub ume_concerned_flanks_exposed_multiple: i32,
    pub ume_concerned_army_destruction: i32,
    pub ume_concerned_general_dead: i32,
    pub ume_concerned_general_fled_recently: i32,
    pub ume_concerned_general_died_recently: i32,
    pub ume_encouraged_on_the_hill: i32,
    pub ume_concerned_unit_frightened: i32,
    pub ume_concerned_horses_frightened: i32,
    pub ume_encouraged_inspired: i32,
    pub ume_concerned_tired: i32,
    pub ume_concerned_very_tired: i32,
    pub ume_concerned_exhausted: i32,
    pub was_attacked_in_front: i32,
    pub was_attacked_in_flank: i32,
    pub was_attacked_in_rear: i32,
    pub fighting_cavalry: i32,
    pub blood_bonus_5: i32,
    pub blood_bonus_7: i32,
    pub blood_bonus_12: i32,
    pub ume_encouraged_column_formation: i32,
    pub special_ability_inspire_unit_bonus: i32,
    pub special_ability_rally_min_bonus: i32,
    pub special_ability_rally_max_bonus: i32,
}

// ---------------------------------------------------------------------------------------------
// Per-evaluation inputs from the unit
// ---------------------------------------------------------------------------------------------

/// Direction an attacker hits this unit from. W1 §12.4 (the three effects are CONFIRMED; how the
/// original classifies a direction is UNKNOWN, the caller decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttackDirection {
    /// From the front.
    Front,
    /// From the left or right flank.
    Flank,
    /// From behind.
    Rear,
}

/// One enemy unit currently attacking this unit (input to [`modifiers`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Attacker {
    /// Where the attack comes from.
    pub direction: AttackDirection,
    /// The attacker is in the original's "excluded set" (UNKNOWN which units; W1 §12.4).
    pub excluded: bool,
    /// Result of `FUN_0055AC20()`, which doubles the front value. UNKNOWN meaning (W1 §12.4).
    pub doubles_front_value: bool,
    /// The "another condition" that gives +2 in the formation adjust. UNKNOWN meaning (W1 §12.4).
    pub plus_two_condition: bool,
}

/// Everything [`evaluate`] reads from the land unit (W1 §8 "Land unit" table), plus the results of
/// unknown helper functions, which the caller must supply.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoraleInputs {
    /// `+0xAA0`: active state. The evaluation only runs when this is exactly 1.
    pub active_state: i32,
    /// `+0x180`: the unit's morale stat (added to `morale_base`).
    pub morale_stat: i32,
    /// `+0x18A`: impetuous allowed (needed for Eager → Impetuous).
    pub impetuous_allowed: bool,
    /// `+0x194`: formation/type class used by the front-attack adjust and fighting-cavalry scaling.
    pub formation_class: u8,
    /// `+0xD08`: morale-modifier category 1..7 (meaning UNKNOWN, W1 §10.6).
    pub category: i32,
    /// `+0xC84`: under projectile fire.
    pub under_projectile_fire: bool,
    /// `+0xC80`: under artillery fire.
    pub under_artillery_fire: bool,
    /// `+0xCB4`: total casualty ratio (0..1).
    pub total_casualty_ratio: f32,
    /// `+0xCBC`: recent casualty ratio (0..1).
    pub recent_casualty_ratio: f32,
    /// `+0xCC4`: extended casualty ratio (0..1).
    pub extended_casualty_ratio: f32,
    /// `+0xCC0`: kill ratio ("blood").
    pub kill_ratio: f32,
    /// Enemies attacking this unit (in a deterministic order chosen by the caller).
    pub attackers: Vec<Attacker>,
    /// The unit is fighting cavalry.
    pub fighting_cavalry: bool,
    /// Result of `FUN_0053E980()`: if false, the suppress flag is cleared. CONFIRMED formula:
    /// `!unit_state(1) && !unit flag +0x189` (see [`shock_persists`]).
    pub shock_persists: bool,
    /// Result of `FUN_00532370()`: needed for Wavering → Broken. CONFIRMED to be true except under
    /// a battle-wide scripted condition (BATTLE_FIDELITY.md §2.5); the caller decides.
    pub may_break: bool,
    /// Byte `unit+0xD48`, the experience level (chevrons) 0..9: staggers the waver and rout
    /// timers ([`waver_timeout`], [`rout_timeout`]). CONFIRMED meaning (exposed as "Experience" by
    /// `0x005ABF40` and `0x005CD340`; read by `0x0053E4D0`, `0x0053A720` and the fatigue bonus
    /// `0x00670F40`).
    pub experience: u8,
    /// `FUN_0055AC20()`: the unit's current order object says it is charging (INFERRED meaning).
    /// Restarts the charge-bonus timer in [`light_update`]. CONFIRMED use.
    pub charging: bool,
    /// `unit+0xC70`: the unit's fatigue level 0..5 (integer mean of its soldiers' levels), read by
    /// [`sub_fatigue`]. CONFIRMED.
    pub fatigue_level: u8,
    /// Unit state 0x15 is active (`FUN_0055C9A0(0x15)`, INFERRED: column formation), read by
    /// [`sub_column_formation`]. CONFIRMED use.
    pub column_formation: bool,
    /// Result of the rally test `0x0055C500` for a routing unit (see [`rally_test`]). Ignored
    /// unless the unit is routing.
    pub can_rally: bool,
    /// The condition that gives behaviour mode 4 instead of routing (CONFIRMED test:
    /// `unit+0xC84 != 0 && unit+0xC88 == 0 && unit+0xC80 == 0 && FUN_0055C1C0() && component byte
    /// +0x50`; the meaning of mode 4 is UNKNOWN). The caller decides.
    pub mode4_condition: bool,
    /// Sub-evaluator `0x0053B970`: an active enemy unit with the "frightens enemy" attribute (unit
    /// `+0x18C`, `unit_stats_land` col 74) is within 100 m (see [`near_100m`]). CONFIRMED test.
    pub enemy_frightens_near: bool,
    /// Same, an enemy with "frightens horses" (`+0x18B`, col 73) while this unit passes
    /// `FUN_0055C2A0` (INFERRED: it is cavalry and not in unit state 0xF).
    pub horses_frightened: bool,
    /// Same, a friendly unit (not this one) with "inspires" (`+0x18D`, col 75) within 100 m.
    pub friend_inspires_near: bool,
    /// Sub-evaluator `0x0053BC70`: the army's general (see [`GeneralStatus`]).
    pub general: GeneralStatus,
    /// The unit has the steadfast attribute (unit `+0x189`): "general died recently" reads as
    /// "general dead" for it (`0x0053BC70`, CONFIRMED).
    pub steadfast: bool,
    /// `FUN_0054C840`: units in the alliance's armies after the first (allies on the field).
    pub allied_units: i32,
    /// `0x00532000`: the army destruction test (see [`army_destruction`]).
    pub army_destruction: bool,
    /// Sub-evaluator `0x0053CBD0`: friendly units within 160 m (see [`Neighbour`]).
    pub friends: Vec<Neighbour>,
    /// Sub-evaluator `0x0053CBD0`: enemy units within 160 m.
    pub enemies: Vec<Neighbour>,
    /// This unit's men (`+0x204`), for `0x0053CBD0`.
    pub own_men: i32,
    /// This unit's `0x006AFB70 + 0x006B05B0`, for `0x0053CBD0`.
    pub own_strength: f32,
}

/// `0x0053E980` (CONFIRMED): the shock/suppress flag survives unless the unit is in state 1
/// (UNKNOWN which) or has the flag `+0x189` (`unit_stats_land` col 71, "steadfast", see
/// [`super::attributes`]; it also makes rallying easier).
pub fn shock_persists(unit_state_1: bool, unit_flag_189: bool) -> bool {
    !unit_state_1 && !unit_flag_189
}

/// The rally test `0x0055C500` (CONFIRMED structure), for a routing unit:
/// - `times_routed < 3`, and the unit is not shattered;
/// - after the 1st rout the unit needs at least `start_men / 4` men, after the 2nd `start_men / 2`
///   (C integer division of the signed `unit+0x30`);
/// - not engaged (`FUN_0054EE90`, the caller's `engaged`);
/// - every enemy unit within 80 m that has not left the field has a strength below
///   `own_strength * 1/3` (`* 1/2` with unit flag `+0x189`), all `f32`.
///
/// The other CONFIRMED conditions (skip flag `+0x56`, the "order state 4/5" test `FUN_0055ADC0`, and
/// `unit+0xAA0 != 2`) are the caller's `extra_ok`.
#[allow(clippy::too_many_arguments)]
pub fn rally_test(
    c: &MoraleComponent,
    men: u32,
    start_men: i32,
    engaged: bool,
    own_strength: f32,
    unit_flag_189: bool,
    enemy_strengths_within_80m: &[f32],
    extra_ok: bool,
) -> bool {
    if c.behaviour != MoraleBehaviour::Routing
        || c.skip_flags[1]
        || c.times_routed >= 3
        || !extra_ok
        || c.state == MoraleState::Shattered
    {
        return false;
    }
    let need = match c.times_routed {
        1 => start_men / 4,
        2 => start_men / 2,
        _ => i32::MIN,
    };
    if (men as i64) < need as i64 {
        return false;
    }
    if engaged {
        return false;
    }
    let threshold = own_strength * if unit_flag_189 { 0.5 } else { 0.333_333_34 };
    enemy_strengths_within_80m.iter().all(|&e| threshold > e)
}

/// What happened during one [`evaluate`] call (useful for UI events and tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoraleOutcome {
    /// False if the evaluation returned early (inactive unit or a skip flag).
    pub evaluated: bool,
    /// State before the evaluation.
    pub previous_state: MoraleState,
    /// State after the evaluation.
    pub new_state: MoraleState,
    /// Shaken → Wavering fires an event in the original. W1 §12.3 (CONFIRMED that it fires).
    pub wavering_event: bool,
    /// The behaviour switched to Routing this evaluation (rout timer set, rout count +1).
    pub started_routing: bool,
}

// ---------------------------------------------------------------------------------------------
// Placeholders for code whose contents are UNKNOWN
// ---------------------------------------------------------------------------------------------

/// Waver timer length set on entering Wavering, `0x0053E4D0` (CONFIRMED):
/// `waver_base_timeout + experience * 5` (experience = byte `unit+0xD48`), in light-update ticks.
pub fn waver_timeout(kv: &KvMorale, experience: u8) -> i32 {
    (experience as i32 * 5).wrapping_add(kv.waver_base_timeout)
}

/// Rout timer length set on starting to rout, `0x0053A720` (CONFIRMED):
/// `max(0, broken_finish_base_timeout - experience * 20)`.
pub fn rout_timeout(kv: &KvMorale, experience: u8) -> i32 {
    kv.broken_finish_base_timeout
        .wrapping_sub(experience as i32 * 20)
        .max(0)
}

/// The general of the unit's army as `0x0053BC70` sees it (army = unit `+0x1EC`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum GeneralStatus {
    /// No general unit in the model for this army (e.g. the AI test fixtures): the sub-evaluator adds
    /// no general effect. The original always has an army object; an army without a living general
    /// gets `ume_concerned_general_dead` (the model's armies with a general use the other values).
    #[default]
    NotModelled,
    /// Army `+0x1B0 == 0` and `+0x1B4 == 0`: no general (dead for a while, or never had one).
    Dead,
    /// Army `+0x1B0 == 0` and `+0x1B4 != 0`: the general died recently (INFERRED reading of
    /// `+0x1B4`).
    DiedRecently,
    /// Army `+0x1B0 != 0` and `+0x1B8 != 0`: the general fled recently (INFERRED reading).
    FledRecently,
    /// The general is with the army. `rank` = the general's unit card `+0xBC` (INFERRED: his
    /// star rank); `distance` = metres to the general's unit (`FUN_0057EB50`), `None` when this
    /// unit is the general's own or the army has no general unit (distance 0 then).
    Present {
        /// The general's rank (card `+0xBC`).
        rank: i32,
        /// Metres to the general, `None` = 0.
        distance: Option<f32>,
    },
}

/// Effect ids of [`sub_army_and_general`] (CONFIRMED ids; the names of 1, 2 and 4 are INFERRED).
pub mod army_effect_id {
    /// Allied armies' units on the field (value `min(n / 4 + 1, 6)`).
    pub const ALLIES: i32 = 1;
    /// The general is near (value scaled by distance and rank).
    pub const GENERAL_NEAR: i32 = 2;
    /// The general is with the army (value `rank / 2 + 1`).
    pub const GENERAL_PRESENT: i32 = 4;
    /// `ume_concerned_army_destruction`.
    pub const ARMY_DESTRUCTION: i32 = 0x13;
    /// `ume_concerned_general_died_recently`.
    pub const GENERAL_DIED_RECENTLY: i32 = 0x14;
    /// `ume_concerned_general_dead`.
    pub const GENERAL_DEAD: i32 = 0x15;
    /// `ume_concerned_general_fled_recently`.
    pub const GENERAL_FLED_RECENTLY: i32 = 0x16;
}

/// `0x00536D20` (CONFIRMED): `x` clamped to `[lo, hi]`, then `x * slope + intercept`, in x87
/// extended precision (we use `f64`).
pub fn clamped_line(x: f32, slope: f32, intercept: f32, lo: f32, hi: f32) -> f64 {
    let x = if x < lo {
        lo
    } else if x <= hi {
        x
    } else {
        hi
    };
    x as f64 * slope as f64 + intercept as f64
}

/// The "general near" weight of `0x0053BC70` (CONFIRMED): 1 within 75 m (`FUN_00555F20` returns
/// 75), falling linearly to 0 at 225 m (`clamped_line(d, 1/(75-225), 225/(225-75), 75, 225)`).
pub fn general_near_weight(distance: f32) -> f64 {
    let r = 75.0f32;
    clamped_line(distance, 1.0 / (r - 225.0), 225.0 / (225.0 - r), r, 225.0)
}

/// Sub-evaluator `0x0053BC70` (CONFIRMED, BATTLE_FIDELITY.md §10):
/// ```text
/// no general:   +0x1B4 == 0 or steadfast (+0x189) -> 0x15 general_dead, else 0x14 general_died_recently
/// fled:         0x16 general_fled_recently
/// present:      within 225 m (or own/no general unit): 0x02 = (int)(weight(d) * ((rank + 1) / 2 + 5))
///               always: 0x04 = rank / 2 + 1
/// allies n > 0: 0x01 = min(n / 4 + 1, 6)
/// army destruction: 0x13 ume_concerned_army_destruction
/// ```
/// `allied_units` (`FUN_0054C840`) = units of the alliance's armies after the first;
/// `army_destruction` see [`army_destruction`].
pub fn sub_army_and_general(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) {
    use army_effect_id::*;
    match i.general {
        GeneralStatus::NotModelled => {}
        GeneralStatus::Dead => c.add_effect(GENERAL_DEAD, kv.ume_concerned_general_dead),
        GeneralStatus::DiedRecently if i.steadfast => {
            c.add_effect(GENERAL_DEAD, kv.ume_concerned_general_dead)
        }
        GeneralStatus::DiedRecently => {
            c.add_effect(GENERAL_DIED_RECENTLY, kv.ume_concerned_general_died_recently)
        }
        GeneralStatus::FledRecently => {
            c.add_effect(GENERAL_FLED_RECENTLY, kv.ume_concerned_general_fled_recently)
        }
        GeneralStatus::Present { rank, distance } => {
            let d = distance.unwrap_or(0.0);
            if d <= 225.0 {
                let scale = (rank.wrapping_add(1) / 2).wrapping_add(5);
                // x87 product, stored as f32 (FSTP float) and then truncated (CVTTSS2SI).
                c.add_effect(GENERAL_NEAR, ((general_near_weight(d) * scale as f64) as f32) as i32);
            }
            c.add_effect(GENERAL_PRESENT, rank / 2 + 1);
        }
    }
    if i.allied_units > 0 {
        c.add_effect(ALLIES, (i.allied_units / 4 + 1).min(6));
    }
    if i.army_destruction {
        c.add_effect(ARMY_DESTRUCTION, kv.ume_concerned_army_destruction);
    }
}

/// `0x00532000` (CONFIRMED): the "army destruction" test. `own` = (start, current) strength of
/// this unit's alliance (alliance `+0x60` / `+0x64`, the sums of `0x006AFB70 + 0x006B05B0` over
/// its units); `enemies` = the same for every other alliance. False when our ratio
/// `current / start` is above 0.1 (0 when start is 0); otherwise true when the other alliances'
/// summed ratio is at least 7 times ours (0 when their summed start is 0).
pub fn army_destruction(own: (f32, f32), enemies: &[(f32, f32)]) -> bool {
    let own_ratio = if own.0 == 0.0 { 0.0 } else { own.1 / own.0 };
    if own_ratio > 0.1 {
        return false;
    }
    let (start, cur) = enemies.iter().fold((0.0f32, 0.0f32), |a, e| (a.0 + e.0, a.1 + e.1));
    let theirs = if start > 0.0 { cur / start } else { 0.0 };
    theirs >= own_ratio * 7.0
}

/// One unit within 160 m for [`sub_terrain`], as the caller sees it (the lists `0x006C9E40` /
/// `0x00701310` with N = 160 m, units that left the field already removed).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Neighbour {
    /// Distance in metres (`FUN_0057EB50`; INFERRED: centre distance).
    pub distance: f32,
    /// Men (`+0x204`).
    pub men: i32,
    /// `0x006AFB70 + 0x006B05B0` of that unit.
    pub strength: f32,
    /// Routing or shattered (`FUN_0055C780`: behaviour 2 or 3; INFERRED that the battle flag
    /// `+0x2704` it also tests is set in a normal battle).
    pub routing: bool,
    /// Friends only: inside the box of 100 m ahead/behind and 75 m to either side of this unit's
    /// facing (`FUN_0055C7B0`, CONFIRMED sizes; which axis is "ahead" INFERRED).
    pub in_box: bool,
    /// Enemies only: this unit is not at least 5 m higher (`own_h < enemy_h + 5`).
    pub blocks_hill: bool,
    /// Enemies only: weight halved (CONFIRMED test: this unit rides and its `FUN_00565150(1)`
    /// value, INFERRED speed, is above the enemy's).
    pub halved: bool,
    /// Enemies only: `FUN_0055B100` differs between the two units (INFERRED: one is in a
    /// fortified position and the other not); such enemies do not count.
    pub fortification_differs: bool,
}

/// Effect ids of [`sub_terrain`] (CONFIRMED ids; names INFERRED except 0xB).
pub mod terrain_effect_id {
    /// `ume_encouraged_on_the_hill`.
    pub const ON_THE_HILL: i32 = 0xB;
    /// Enemies routing nearby (positive).
    pub const ENEMIES_ROUTING: i32 = 0xC;
    /// We heavily outnumber the enemies nearby (+4).
    pub const OUTNUMBERING: i32 = 0xD;
    /// Friends routing nearby (negative).
    pub const FRIENDS_ROUTING: i32 = 0x26;
    /// Outnumbered (-1 to -7).
    pub const OUTNUMBERED: i32 = 0x27;
}

/// The distance weight of `0x0053CBD0` (CONFIRMED): `(int)clamped_line(d, -0.01, 2.0, 48, 160)`,
/// i.e. 1 up to about 100 m and 0 beyond.
pub fn neighbour_weight(distance: f32) -> i32 {
    // Stored as f32 (FSTP float) before the truncation (CVTTSS2SI), as compiled.
    (clamped_line(distance, -0.01, 2.0, 48.0, 160.0) as f32) as i32
}

/// Sub-evaluator `0x0053CBD0` (CONFIRMED, BATTLE_FIDELITY.md §10), from `i.friends` and
/// `i.enemies` (within 160 m) and this unit's own men and strength:
/// ```text
/// friends: routing -> in the box: r += 2; else men_f += men*w, str_f += (int)strength*w
/// enemies: hill = any enemy and none blocks it; routing -> r -= (state Impetuous ? 3 : 2);
///          else if same fortification state: w' = halved ? w/2 : w, men_e += men*w', str_e += (int)strength*w'
/// r >= 2: 0x26 = -2*min(r/2, 4) ; r < -1: 0x0C = -2*max(r/2, -4)
/// men_f += own men * 2 ; own = (int)(own strength + str_f)
/// men_f > 0 and own > 0: k = min(men_e / men_f, str_e / own): 6+ -> -7, 5 -> -5, 4 -> -3, 3 -> -2, 2 -> -1 (0x27)
/// hill -> 0x0B ume_encouraged_on_the_hill ; str_e * 3 <= own -> 0x0D = +4
/// ```
pub fn sub_terrain(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) {
    use terrain_effect_id::*;
    let (mut r, mut men_f, mut str_f, mut men_e, mut str_e) = (0i32, 0i32, 0i32, 0i32, 0i32);
    for f in &i.friends {
        if f.routing {
            if f.in_box {
                r += 2;
            }
        } else {
            let w = neighbour_weight(f.distance);
            men_f = men_f.wrapping_add(f.men.wrapping_mul(w));
            str_f = str_f.wrapping_add((f.strength as i32).wrapping_mul(w));
        }
    }
    let mut hill = !i.enemies.is_empty();
    for e in &i.enemies {
        if e.blocks_hill {
            hill = false;
        }
        if e.routing {
            r -= if c.state == MoraleState::Impetuous { 3 } else { 2 };
        } else if !e.fortification_differs {
            let mut w = neighbour_weight(e.distance);
            if e.halved {
                w /= 2;
            }
            men_e = men_e.wrapping_add(e.men.wrapping_mul(w));
            str_e = str_e.wrapping_add((e.strength as i32).wrapping_mul(w));
        }
    }
    if r >= 2 {
        c.add_effect(FRIENDS_ROUTING, -2 * (r / 2).min(4));
    } else if r < -1 {
        c.add_effect(ENEMIES_ROUTING, -2 * (r / 2).max(-4));
    }
    men_f = men_f.wrapping_add(i.own_men.wrapping_mul(2));
    let own = (i.own_strength + str_f as f32) as i32;
    if men_f > 0 && own > 0 {
        let k = (men_e / men_f).min(str_e / own);
        let v = match k {
            k if k >= 6 => -7,
            5 => -5,
            4 => -3,
            3 => -2,
            2 => -1,
            _ => 0,
        };
        if v != 0 {
            c.add_effect(OUTNUMBERED, v);
        }
    }
    if hill {
        c.add_effect(ON_THE_HILL, kv.ume_encouraged_on_the_hill);
    }
    if str_e.wrapping_mul(3) <= own {
        c.add_effect(OUTNUMBERING, 4);
    }
}

/// Sub-evaluator `0x0053E450` (fortified position → effect 8, compromised → 9). CONFIRMED logic. The
/// test `0x0055B100` is "in a fort area, or in a wall (`fort`) building"; the model has no fort areas
/// and nobody garrisons walls, so it adds nothing (BATTLE_FIDELITY.md §38).
pub fn sub_fortification(_c: &mut MoraleComponent, _kv: &KvMorale, _i: &MoraleInputs) {}

/// Sub-evaluator `0x0053BB40` (flank cover from the neighbour slots `+0xD5C/+0xD60/+0xD64`).
/// CONFIRMED logic (BATTLE_FIDELITY.md §2.5); the model has no neighbour slots. PLACEHOLDER: adds
/// nothing.
pub fn sub_flanks(_c: &mut MoraleComponent, _kv: &KvMorale, _i: &MoraleInputs) {}

/// Effect ids of [`sub_fear_and_inspiration`] (CONFIRMED).
pub mod fear_effect_id {
    /// `ume_concerned_unit_frightened`.
    pub const UNIT_FRIGHTENED: i32 = 0x2A;
    /// `ume_concerned_horses_frightened`.
    pub const HORSES_FRIGHTENED: i32 = 0x29;
    /// `ume_encouraged_inspired`.
    pub const INSPIRED: i32 = 0xF;
}

/// The "within N m" test of the unit lists `0x00701310` (enemies) and `0x006C9E40` (friends,
/// skipping the unit itself), CONFIRMED: centre distance (x, z plane) `< r_self + N + r_other`, with
/// `r` = the formation object's `+0x670` (INFERRED: the formation's radius). Strict `<`.
pub fn near(dist_sq: f32, r_self: f32, r_other: f32, n: f32) -> bool {
    let reach = r_self + n + r_other;
    dist_sq < reach * reach
}

/// [`near`] with N = 100 m (the fear and inspiration range, `0x42C80000`).
pub fn near_100m(dist_sq: f32, r_self: f32, r_other: f32) -> bool {
    near(dist_sq, r_self, r_other, 100.0)
}

/// Sub-evaluator `0x0053B970` (CONFIRMED): each of the three conditions adds its effect once, at
/// most, in this order: frightened (0x2A), horses frightened (0x29), inspired (0xF). The caller
/// works out the conditions ([`MoraleInputs::enemy_frightens_near`] etc.).
pub fn sub_fear_and_inspiration(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) {
    if i.enemy_frightens_near {
        c.add_effect(fear_effect_id::UNIT_FRIGHTENED, kv.ume_concerned_unit_frightened);
    }
    if i.horses_frightened {
        c.add_effect(fear_effect_id::HORSES_FRIGHTENED, kv.ume_concerned_horses_frightened);
    }
    if i.friend_inspires_near {
        c.add_effect(fear_effect_id::INSPIRED, kv.ume_encouraged_inspired);
    }
}

/// Effect ids of [`sub_fatigue`] (CONFIRMED).
pub mod fatigue_effect_id {
    /// Tired (fatigue level 3), `ume_concerned_tired`.
    pub const TIRED: i32 = 0x1D;
    /// Very tired (level 4), `ume_concerned_very_tired`.
    pub const VERY_TIRED: i32 = 0x1E;
    /// Exhausted (level 5), `ume_concerned_exhausted`.
    pub const EXHAUSTED: i32 = 0x1F;
}

/// Sub-evaluator `0x0053B7B0` (CONFIRMED): fatigue level (`unit+0xC70`) 3 / 4 / 5 →
/// `ume_concerned_tired` / `_very_tired` / `_exhausted` (effects 0x1D / 0x1E / 0x1F). A value of 0
/// adds nothing; formation class 4 halves it (C division), class 5 adds the effect with value 0.
/// (The first exhausted evaluation also posts a UI event; not modelled.)
pub fn sub_fatigue(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) {
    let (id, v) = match i.fatigue_level {
        3 => (fatigue_effect_id::TIRED, kv.ume_concerned_tired),
        4 => (fatigue_effect_id::VERY_TIRED, kv.ume_concerned_very_tired),
        5 => (fatigue_effect_id::EXHAUSTED, kv.ume_concerned_exhausted),
        _ => return,
    };
    if v == 0 {
        return;
    }
    match i.formation_class {
        4 => c.add_effect(id, v / 2),
        5 => c.add_effect(id, 0),
        _ => c.add_effect(id, v),
    }
}

/// Effect id of [`sub_column_formation`] (CONFIRMED).
pub const COLUMN_FORMATION_EFFECT: i32 = 0x12;

/// Sub-evaluator `0x0053BC20` (CONFIRMED): in unit state 0x15 (INFERRED: column formation), add
/// effect 0x12 = `ume_encouraged_column_formation` when it is not 0.
pub fn sub_column_formation(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) {
    if i.column_formation && kv.ume_encouraged_column_formation != 0 {
        c.add_effect(COLUMN_FORMATION_EFFECT, kv.ume_encouraged_column_formation);
    }
}

/// The three morale modes a battle script can set (`morale_behavior_fearless` / `_default` /
/// `_rout`; the bindings `0x00612F00` / `0x00612EC0` / `0x00612F40` send command
/// `BCQ_UNIT_MORALE_CHANGE` with mode 0 / 1 / 2, CONFIRMED immediates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptMorale {
    /// Mode 0, `morale_behavior_fearless`.
    Fearless,
    /// Mode 1, `morale_behavior_default`.
    Default,
    /// Mode 2, `morale_behavior_rout`.
    Rout,
}

/// The morale mode change `0x00555970` (CONFIRMED writes on the morale component):
/// - fearless: state 1 (Eager), behaviour 0, flag `+0x55` set — the skip flag stops every later
///   evaluation and light update, so the unit stays Eager;
/// - default: flags `+0x55` and `+0x56` cleared (one 16-bit write), nothing else;
/// - rout: state 7, behaviour 5, flag `+0x56` set (no rally). The original also sends a UI event
///   (id 5) for the unit; not modelled.
pub fn set_script_morale(c: &mut MoraleComponent, mode: ScriptMorale) {
    match mode {
        ScriptMorale::Fearless => {
            c.state = MoraleState::Eager;
            c.behaviour = MoraleBehaviour::Normal;
            c.skip_flags[0] = true;
        }
        ScriptMorale::Default => {
            c.skip_flags[0] = false;
            c.skip_flags[1] = false;
        }
        ScriptMorale::Rout => {
            c.state = MoraleState::Shattered;
            c.behaviour = MoraleBehaviour::ScriptRout;
            c.skip_flags[1] = true;
        }
    }
}

/// The light morale update, `0x00585BE0` (CONFIRMED). It runs on every tick: alone on the 4 ticks
/// out of 5 without a full evaluation, and as the first step of [`evaluate`] on the fifth.
/// ```text
/// if a skip flag is set: return
/// if state == Wavering and waver_timer >= 0: waver_timer -= 1
/// elif state == Broken and rout_timer >= 0: rout_timer -= 1
/// if surprise_timer >= 0: surprise_timer -= 1
/// if charging and charge_timer <= 0: charge_timer = charge_timeout
/// elif charge_timer >= 0: charge_timer -= 1
/// every transient effect: seconds_left -= 0.1 ; removed when <= 0
/// ```
pub fn light_update(c: &mut MoraleComponent, kv: &KvMorale, charging: bool) {
    if c.skip_flags.iter().any(|&f| f) {
        return;
    }
    if c.state == MoraleState::Wavering {
        if c.waver_timer >= 0 {
            c.waver_timer -= 1;
        }
    } else if c.state == MoraleState::Broken && c.rout_timer >= 0 {
        c.rout_timer -= 1;
    }
    if c.surprise_timer >= 0 {
        c.surprise_timer -= 1;
    }
    if charging && c.charge_timer <= 0 {
        c.charge_timer = kv.charge_timeout;
    } else if c.charge_timer >= 0 {
        c.charge_timer -= 1;
    }
    c.transient_effects.retain_mut(|t| {
        t.seconds_left -= 0.1;
        t.seconds_left > 0.0
    });
}

// ---------------------------------------------------------------------------------------------
// 0x0053C720: modifiers
// ---------------------------------------------------------------------------------------------

/// Total-casualty ladder, strictly greater. W1 §12.4 (CONFIRMED thresholds 0.9/0.8/0.6/0.4/0.2).
fn total_casualty_penalty(kv: &KvMorale, ratio: f32) -> Option<i32> {
    if ratio > 0.9 {
        Some(kv.total_casualties_penalty_90)
    } else if ratio > 0.8 {
        Some(kv.total_casualties_penalty_80)
    } else if ratio > 0.6 {
        Some(kv.total_casualties_penalty_60)
    } else if ratio > 0.4 {
        Some(kv.total_casualties_penalty_40)
    } else if ratio > 0.2 {
        Some(kv.total_casualties_penalty_20)
    } else {
        None
    }
}

/// Recent-casualty ladder, greater-or-equal. W1 §12.4 (CONFIRMED thresholds 0.5/0.33/0.15/0.10/0.06).
fn recent_casualty_penalty(kv: &KvMorale, ratio: f32) -> Option<i32> {
    if ratio >= 0.5 {
        Some(kv.recent_casualties_penalty_50)
    } else if ratio >= 0.33 {
        Some(kv.recent_casualties_penalty_33)
    } else if ratio >= 0.15 {
        Some(kv.recent_casualties_penalty_15)
    } else if ratio >= 0.10 {
        Some(kv.recent_casualties_penalty_10)
    } else if ratio >= 0.06 {
        Some(kv.recent_casualties_penalty_6)
    } else {
        None
    }
}

/// Extended-casualty ladder, greater-or-equal. W1 §12.4 (CONFIRMED thresholds 0.8/0.5/0.33/0.15/0.1).
fn extended_casualty_penalty(kv: &KvMorale, ratio: f32) -> Option<i32> {
    if ratio >= 0.8 {
        Some(kv.extended_casualties_penalty_80)
    } else if ratio >= 0.5 {
        Some(kv.extended_casualties_penalty_50)
    } else if ratio >= 0.33 {
        Some(kv.extended_casualties_penalty_33)
    } else if ratio >= 0.15 {
        Some(kv.extended_casualties_penalty_15)
    } else if ratio >= 0.1 {
        Some(kv.extended_casualties_penalty_10)
    } else {
        None
    }
}

/// Blood (kill ratio) ladder, greater-or-equal. W1 §12.4 (CONFIRMED thresholds 0.125/0.075/0.05).
fn blood_bonus(kv: &KvMorale, ratio: f32) -> Option<i32> {
    if ratio >= 0.125 {
        Some(kv.blood_bonus_12)
    } else if ratio >= 0.075 {
        Some(kv.blood_bonus_7)
    } else if ratio >= 0.05 {
        Some(kv.blood_bonus_5)
    } else {
        None
    }
}

/// Fighting-cavalry value scaled by the unit class. W1 §12.4 (CONFIRMED):
/// class 0/1 → full, 2 → `/2`, 3 → `/4` (**signed shift**, so −5 >> 1 = −3), 4/5 → none.
/// Classes above 5 are not described; we treat them as "none" (INFERRED).
pub fn fighting_cavalry_value(kv: &KvMorale, formation_class: u8) -> Option<i32> {
    let v = kv.fighting_cavalry;
    match formation_class {
        0 | 1 => Some(v),
        2 => Some(v >> 1),
        3 => Some(v >> 2),
        _ => None,
    }
}

/// Formation adjust added to the front-attack value. W1 §12.4 (CONFIRMED numbers):
/// class 0/1 → −4, class 2 → −2, otherwise +2 "under another condition" (UNKNOWN condition,
/// supplied as [`Attacker::plus_two_condition`]).
fn front_formation_adjust(formation_class: u8, plus_two_condition: bool) -> i32 {
    match formation_class {
        0 | 1 => -4,
        2 => -2,
        _ if plus_two_condition => 2,
        _ => 0,
    }
}

/// The morale-modifier pass, `0x0053C720`. W1 §12.4.
///
/// Adds, in this order: category effect, front attacks, flank, rear, fighting cavalry,
/// total / recent / extended casualties, blood. Also sets the shock (suppress) flag when the
/// recent casualty ratio reaches `recent_casualties_shock_threshold * 0.01`.
///
/// INFERRED details: the per-attacker front logic is applied to `Front` attackers only; the class
/// used by the formation adjust is this unit's own `+0x194`; all ratios are compared as `f32`.
pub fn modifiers(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) {
    // Category (+0xD08): hard-coded values, CONFIRMED.
    match i.category {
        1 => c.add_effect(effect_id::CATEGORY_BONUS, 6),
        2 => c.add_effect(effect_id::CATEGORY_BONUS, 4),
        3 => c.add_effect(effect_id::CATEGORY_BONUS, 2),
        6 => c.add_effect(effect_id::CATEGORY_PENALTY, -4),
        7 => c.add_effect(effect_id::CATEGORY_PENALTY, -8),
        _ => {}
    }

    // Front attackers: one effect per attacker; they all count (add_effect does not de-duplicate).
    for a in i
        .attackers
        .iter()
        .filter(|a| a.direction == AttackDirection::Front)
    {
        if a.excluded {
            continue;
        }
        let mut v = kv.was_attacked_in_front;
        if a.doubles_front_value {
            v = v.wrapping_mul(2);
        }
        v = v.wrapping_add(front_formation_adjust(
            i.formation_class,
            a.plus_two_condition,
        ));
        c.add_effect(effect_id::ATTACKED_FRONT, v);
    }
    if i.attackers
        .iter()
        .any(|a| a.direction == AttackDirection::Flank)
    {
        c.add_effect(effect_id::ATTACKED_FLANK, kv.was_attacked_in_flank);
    }
    if i.attackers
        .iter()
        .any(|a| a.direction == AttackDirection::Rear)
    {
        c.add_effect(effect_id::ATTACKED_REAR, kv.was_attacked_in_rear);
    }

    if i.fighting_cavalry
        && let Some(v) = fighting_cavalry_value(kv, i.formation_class)
    {
        c.add_effect(effect_id::FIGHTING_CAVALRY, v);
    }

    if let Some(v) = total_casualty_penalty(kv, i.total_casualty_ratio) {
        c.add_effect(effect_id::TOTAL_CASUALTIES, v);
    }

    // Recent casualties: shock flag first, then the ladder.
    if i.recent_casualty_ratio >= kv.recent_casualties_shock_threshold as f32 * 0.01 {
        c.suppress_shaken_clamp = true;
    }
    if let Some(v) = recent_casualty_penalty(kv, i.recent_casualty_ratio) {
        c.add_effect(effect_id::RECENT_CASUALTIES, v);
    }

    if let Some(v) = extended_casualty_penalty(kv, i.extended_casualty_ratio) {
        c.add_effect(effect_id::EXTENDED_CASUALTIES, v);
    }

    if let Some(v) = blood_bonus(kv, i.kill_ratio) {
        c.add_effect(effect_id::BLOOD, v);
    }
}

// ---------------------------------------------------------------------------------------------
// 0x00584020: full evaluation
// ---------------------------------------------------------------------------------------------

/// The hysteresis step: at most one state change, all comparisons strict. W1 §12.3 (CONFIRMED).
fn hysteresis(c: &MoraleComponent, kv: &KvMorale, i: &MoraleInputs, m: i32) -> MoraleState {
    use MoraleState::*;
    match c.state {
        Impetuous => {
            if m < kv.ums_impetuous_threshold_lower {
                Eager
            } else {
                Impetuous
            }
        }
        Eager => {
            if m > kv.ums_eager_threshold_upper && i.impetuous_allowed {
                Impetuous
            } else if m < kv.ums_eager_threshold_lower {
                Confident
            } else {
                Eager
            }
        }
        Confident => {
            if m > kv.ums_confident_threshold_upper {
                Eager
            } else if m < kv.ums_confident_threshold_lower {
                Steady
            } else {
                Confident
            }
        }
        Steady => {
            if m > kv.ums_steady_threshold_upper {
                Confident
            } else if m < kv.ums_steady_threshold_lower {
                Shaken
            } else {
                Steady
            }
        }
        Shaken => {
            if m > kv.ums_shaken_threshold_upper {
                Steady
            } else if m < kv.ums_shaken_threshold_lower {
                Wavering
            } else {
                Shaken
            }
        }
        // Gate: Wavering can only change once the waver timer has run out (< 0).
        Wavering if c.waver_timer < 0 => {
            if m > kv.ums_wavering_threshold_upper {
                Shaken
            } else if m < kv.ums_wavering_threshold_lower && i.may_break {
                Broken
            } else {
                Wavering
            }
        }
        // Gate: Broken can only change once the rout timer has run out (< 0).
        Broken if c.rout_timer < 0 => {
            if m > kv.ums_broken_threshold_upper {
                Wavering
            } else if m < kv.ums_broken_threshold_lower {
                Shattered
            } else {
                Broken
            }
        }
        // Gated Wavering/Broken, and Shattered (no way back).
        other => other,
    }
}

/// Full morale evaluation of one unit, `0x00584020` (CONFIRMED structure and order, W1 §12.3 and
/// BATTLE_FIDELITY.md §2):
/// 1. [`light_update`] (timers, transient effects). Then return unless `active_state == 1` and no
///    skip flag is set.
/// 2. Clear the suppress flag (`+0x51`, 16-bit store with `+0x52`) and the active-effect list;
///    `morale = morale_base + morale_stat + persistent_bonus`.
/// 3. Sub-evaluators in order `0x53BC70, 0x53CBD0, 0x53C720 (modifiers), 0x53E450, 0x53BB40,
///    0x53B970, 0x53B7B0, 0x53BC20`.
/// 4. Transient effects; projectile fire; artillery fire; surprise (also sets suppress); charge.
/// 5. `morale += sum(active effects)`.
/// 6. Clear suppress if `FUN_53E980()` is false.
/// 7. Wavering clamp: state 5, `morale < wavering_lower`, not suppressed, `waver_timer > 0`
///    → `morale = wavering_lower`.
/// 8. Routing units: without `can_rally` only step 9 runs, then the evaluation ends; with
///    `can_rally` and an expired rout timer the unit rallies (`0x00572AC0`: Wavering, behaviour
///    normal, rout timer −1, a new waver timer).
/// 9. State 6 and `morale < broken_lower` → state 7.
/// 10. Hysteresis (one step, strict comparisons, timer gates on states 5 and 6).
/// 11. Behaviour: states 6/7 → mode 4 if `mode4_condition`, else 3 (state 7) / 2 (state 6);
///     other states → 0. A change to a state < 5 clears bytes `+0x53/+0x54`; a change to 5 sets
///     the waver timer; a change of behaviour to routing sets the rout timer and counts the rout.
pub fn evaluate(c: &mut MoraleComponent, kv: &KvMorale, i: &MoraleInputs) -> MoraleOutcome {
    // 1. Light update first, even for inactive units.
    light_update(c, kv, i.charging);
    let previous_state = c.state;
    let mut outcome = MoraleOutcome {
        evaluated: false,
        previous_state,
        new_state: previous_state,
        wavering_event: false,
        started_routing: false,
    };
    if i.active_state != 1 || c.skip_flags.iter().any(|&f| f) {
        return outcome;
    }
    outcome.evaluated = true;

    // 2. Base value.
    c.suppress_shaken_clamp = false;
    c.active_effects.clear();
    let mut morale = kv
        .morale_base
        .wrapping_add(i.morale_stat)
        .wrapping_add(c.persistent_bonus);

    // 3. Sub-evaluators, in the CONFIRMED call order.
    sub_army_and_general(c, kv, i);
    sub_terrain(c, kv, i);
    modifiers(c, kv, i);
    sub_fortification(c, kv, i);
    sub_flanks(c, kv, i);
    sub_fear_and_inspiration(c, kv, i);
    sub_fatigue(c, kv, i);
    sub_column_formation(c, kv, i);

    // 4. Transient and situational effects.
    let transients = c.transient_effects.clone();
    for t in &transients {
        c.add_effect(t.id, t.value);
    }
    if i.under_projectile_fire {
        c.add_effect(
            effect_id::UNDER_PROJECTILES,
            kv.ume_concerned_attacked_by_projectile,
        );
    }
    if i.under_artillery_fire {
        c.add_effect(
            effect_id::UNDER_ARTILLERY,
            kv.ume_concerned_attacked_by_artillery,
        );
    }
    if c.surprise_timer >= 0 {
        c.add_effect(effect_id::SURPRISED, kv.ume_concerned_surprised);
        c.suppress_shaken_clamp = true;
    }
    if c.charge_timer > 0 {
        c.add_effect(effect_id::CHARGE, kv.charge_bonus);
    }

    // 5. Sum of effects.
    morale = morale.wrapping_add(c.effects_sum());
    c.morale = morale;

    // 6. Suppress flag only survives while FUN_53E980() says so.
    if c.suppress_shaken_clamp && !i.shock_persists {
        c.suppress_shaken_clamp = false;
    }

    // 7. Wavering clamp.
    if c.state == MoraleState::Wavering
        && c.morale < kv.ums_wavering_threshold_lower
        && !c.suppress_shaken_clamp
        && c.waver_timer > 0
    {
        c.morale = kv.ums_wavering_threshold_lower;
    }
    let morale = c.morale;

    // 8. Routing units: rally test.
    let mut transitions = true;
    if c.behaviour == MoraleBehaviour::Routing {
        if !i.can_rally {
            transitions = false;
        } else if c.rout_timer < 0 {
            // 0x00572AC0: rally.
            c.state = MoraleState::Wavering;
            c.behaviour = MoraleBehaviour::Normal;
            c.rally_flag = true;
            c.rout_timer = -1;
            c.waver_timer = waver_timeout(kv, i.experience);
        }
    }
    let state_before = c.state;

    // 9. Broken units below the broken lower threshold shatter regardless of the rout timer.
    if c.state == MoraleState::Broken && morale < kv.ums_broken_threshold_lower {
        c.state = MoraleState::Shattered;
    }
    if !transitions {
        outcome.new_state = c.state;
        return outcome;
    }
    let state_before = if c.state == MoraleState::Shattered { c.state } else { state_before };

    // 10. Hysteresis.
    let new_state = hysteresis(c, kv, i, morale);
    if new_state == MoraleState::Wavering && c.state == MoraleState::Shaken {
        outcome.wavering_event = true;
    }
    c.state = new_state;

    // 11. Behaviour mode.
    let mode_before = c.behaviour;
    c.behaviour = match c.state {
        MoraleState::Broken | MoraleState::Shattered if i.mode4_condition => {
            MoraleBehaviour::Special
        }
        MoraleState::Shattered => MoraleBehaviour::Shattered,
        MoraleState::Broken => MoraleBehaviour::Routing,
        _ => MoraleBehaviour::Normal,
    };
    if c.state != state_before {
        if (c.state as u8) < 5 {
            c.rally_flag = false;
        } else if c.state == MoraleState::Wavering {
            c.waver_timer = waver_timeout(kv, i.experience);
        }
    }
    if c.behaviour != mode_before && c.behaviour == MoraleBehaviour::Routing {
        c.rout_timer = rout_timeout(kv, i.experience);
        c.times_routed = c.times_routed.wrapping_add(1);
        outcome.started_routing = true;
    }

    outcome.new_state = c.state;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use MoraleState::*;

    #[test]
    fn army_and_general_effects() {
        use army_effect_id::*;
        let kv = KvMorale {
            ume_concerned_general_dead: -10,
            ume_concerned_general_died_recently: -6,
            ume_concerned_general_fled_recently: -8,
            ume_concerned_army_destruction: -20,
            ..KvMorale::default()
        };
        let run = |i: MoraleInputs| {
            let mut c = MoraleComponent::default();
            sub_army_and_general(&mut c, &kv, &i);
            c
        };
        // Not modelled: nothing.
        assert!(run(MoraleInputs::default()).active_effects.is_empty());
        assert_eq!(run(MoraleInputs { general: GeneralStatus::Dead, ..Default::default() }).effect(GENERAL_DEAD), Some(-10));
        let recent = MoraleInputs { general: GeneralStatus::DiedRecently, ..Default::default() };
        assert_eq!(run(recent.clone()).effect(GENERAL_DIED_RECENTLY), Some(-6));
        // Steadfast units read "died recently" as "dead".
        assert_eq!(run(MoraleInputs { steadfast: true, ..recent }).effect(GENERAL_DEAD), Some(-10));
        assert_eq!(run(MoraleInputs { general: GeneralStatus::FledRecently, ..Default::default() }).effect(GENERAL_FLED_RECENTLY), Some(-8));
        // Present, rank 3, within 75 m: the x87 weight is 0.99999999 (f32 slope 1/(75-225)), but the
        // product is stored as f32 (7.0) before truncation -> 7; presence 3/2+1 = 2.
        let near = run(MoraleInputs { general: GeneralStatus::Present { rank: 3, distance: Some(50.0) }, ..Default::default() });
        assert_eq!((near.effect(GENERAL_NEAR), near.effect(GENERAL_PRESENT)), (Some(7), Some(2)));
        // 150 m: weight (225-150)/150 = 0.5 -> (int)3.5 = 3.
        let mid = run(MoraleInputs { general: GeneralStatus::Present { rank: 3, distance: Some(150.0) }, ..Default::default() });
        assert_eq!(mid.effect(GENERAL_NEAR), Some(3));
        // Beyond 225 m: no "near" effect, presence still.
        let far = run(MoraleInputs { general: GeneralStatus::Present { rank: 3, distance: Some(300.0) }, ..Default::default() });
        assert_eq!((far.effect(GENERAL_NEAR), far.effect(GENERAL_PRESENT)), (None, Some(2)));
        // Allies: min(n/4 + 1, 6); army destruction.
        assert_eq!(run(MoraleInputs { allied_units: 9, ..Default::default() }).effect(ALLIES), Some(3));
        assert_eq!(run(MoraleInputs { allied_units: 40, ..Default::default() }).effect(ALLIES), Some(6));
        assert_eq!(run(MoraleInputs { army_destruction: true, ..Default::default() }).effect(ARMY_DESTRUCTION), Some(-20));
    }

    #[test]
    fn component_starts_confident_with_the_army_level_bonus() {
        let c = MoraleComponent::default();
        assert_eq!((c.state, c.behaviour, c.morale), (Confident, MoraleBehaviour::Normal, 0));
        assert_eq!((c.surprise_timer, c.rout_timer, c.waver_timer, c.charge_timer), (-1, -1, -1, -1));
        let b = |l, f| MoraleComponent::for_army_level(l, f).persistent_bonus;
        assert_eq!((b(0, false), b(1, false), b(2, false), b(-1, false), b(-2, false)), (0, 2, 4, -1, 0));
        assert_eq!((b(0, true), b(1, true), b(2, true), b(-1, true), b(-2, true)), (0, 3, 0, 0, -1));
    }

    #[test]
    fn army_destruction_test() {
        // Our side at 10 % or less and the enemies at least 7x our ratio.
        assert!(army_destruction((1000.0, 100.0), &[(1000.0, 700.0)]));
        assert!(!army_destruction((1000.0, 100.0), &[(1000.0, 690.0)]));
        assert!(!army_destruction((1000.0, 110.0), &[(1000.0, 1000.0)]));
        // Several other alliances are summed.
        assert!(army_destruction((1000.0, 50.0), &[(500.0, 100.0), (500.0, 300.0)]));
    }

    #[test]
    fn neighbour_effects() {
        use terrain_effect_id::*;
        let kv = KvMorale { ume_encouraged_on_the_hill: 5, ..KvMorale::default() };
        let run = |state: MoraleState, i: &MoraleInputs| {
            let mut c = MoraleComponent { state, ..MoraleComponent::default() };
            sub_terrain(&mut c, &kv, i);
            c
        };
        // Weights: 1 up to 100 m, 0 beyond.
        assert_eq!((neighbour_weight(10.0), neighbour_weight(100.0), neighbour_weight(101.0)), (1, 1, 0));
        let enemy = |d: f32, men: i32, s: f32| Neighbour { distance: d, men, strength: s, ..Default::default() };
        // Outnumbered 3:1 in men and strength -> -2; not on a hill (an enemy is as high).
        let i = MoraleInputs {
            own_men: 100,
            own_strength: 500.0,
            enemies: vec![enemy(50.0, 600, 3000.0), Neighbour { blocks_hill: true, ..enemy(150.0, 600, 3000.0) }],
            ..Default::default()
        };
        let c = run(Steady, &i);
        // men: 600 / (100*2) = 3; strength: 3000 / 500 = 6 -> min 3 -> -2.
        assert_eq!(c.effect(OUTNUMBERED), Some(-2));
        assert_eq!(c.effect(ON_THE_HILL), None);
        assert_eq!(c.effect(OUTNUMBERING), None);
        // Alone on a hill above every enemy, enemies far (weight 0): hill +5 and "outnumbering" +4.
        let i = MoraleInputs { own_men: 100, own_strength: 500.0, enemies: vec![enemy(150.0, 600, 3000.0)], ..Default::default() };
        let c = run(Steady, &i);
        assert_eq!((c.effect(ON_THE_HILL), c.effect(OUTNUMBERING)), (Some(5), Some(4)));
        // Routing: two friends in the box -> r = 4 -> -4 ; three routing enemies -> r = -6 -> +6,
        // impetuous units count 3 per enemy -> r = -9 -> /2 = -4 -> +8.
        let fr = Neighbour { routing: true, in_box: true, ..Default::default() };
        let c = run(Steady, &MoraleInputs { own_strength: -1.0, friends: vec![fr, fr], ..Default::default() });
        assert_eq!(c.effect(FRIENDS_ROUTING), Some(-4));
        let er = Neighbour { routing: true, blocks_hill: true, ..Default::default() };
        let i = MoraleInputs { own_strength: -1.0, enemies: vec![er, er, er], ..Default::default() };
        assert_eq!(run(Steady, &i).effect(ENEMIES_ROUTING), Some(6));
        assert_eq!(run(Impetuous, &i).effect(ENEMIES_ROUTING), Some(8));
        // A routing friend outside the box does not count.
        let out = Neighbour { routing: true, in_box: false, ..Default::default() };
        assert!(run(Steady, &MoraleInputs { own_strength: -1.0, friends: vec![out, out], ..Default::default() }).active_effects.is_empty());
    }

    #[test]
    fn fear_and_inspiration_effects() {
        let kv = KvMorale {
            ume_concerned_unit_frightened: -8,
            ume_concerned_horses_frightened: -6,
            ume_encouraged_inspired: 4,
            ..KvMorale::default()
        };
        let mut c = MoraleComponent::default();
        let i = MoraleInputs { enemy_frightens_near: true, horses_frightened: true, friend_inspires_near: true, ..Default::default() };
        sub_fear_and_inspiration(&mut c, &kv, &i);
        assert_eq!(c.effect(fear_effect_id::UNIT_FRIGHTENED), Some(-8));
        assert_eq!(c.effect(fear_effect_id::HORSES_FRIGHTENED), Some(-6));
        assert_eq!(c.effect(fear_effect_id::INSPIRED), Some(4));
        assert_eq!(c.effects_sum(), -10);
        // Range test: strict, radii on both sides.
        assert!(near_100m(99.9 * 99.9, 0.0, 0.0));
        assert!(!near_100m(100.0 * 100.0, 0.0, 0.0));
        assert!(near(119.0 * 119.0, 10.0, 10.0, 100.0));
    }

    /// OBVIOUSLY MADE-UP placeholder values for tests. These are NOT game data; they are just a
    /// tidy ladder of round numbers so that each threshold is easy to hit exactly.
    fn kv() -> KvMorale {
        KvMorale {
            morale_base: 0,
            ums_impetuous_threshold_lower: 90,
            ums_eager_threshold_upper: 95,
            ums_eager_threshold_lower: 70,
            ums_confident_threshold_upper: 75,
            ums_confident_threshold_lower: 50,
            ums_steady_threshold_upper: 55,
            ums_steady_threshold_lower: 30,
            ums_shaken_threshold_upper: 35,
            ums_shaken_threshold_lower: 10,
            ums_wavering_threshold_upper: 15,
            ums_wavering_threshold_lower: -10,
            ums_broken_threshold_upper: -5,
            ums_broken_threshold_lower: -30,
            waver_base_timeout: 111,
            broken_finish_base_timeout: 222,
            charge_bonus: 7,
            ume_concerned_attacked_by_projectile: -3,
            ume_concerned_attacked_by_artillery: -5,
            ume_concerned_surprised: -11,
            was_attacked_in_front: -10,
            was_attacked_in_flank: -20,
            was_attacked_in_rear: -30,
            fighting_cavalry: -5,
            recent_casualties_shock_threshold: 25,
            total_casualties_penalty_20: -1,
            total_casualties_penalty_40: -2,
            total_casualties_penalty_60: -3,
            total_casualties_penalty_80: -4,
            total_casualties_penalty_90: -5,
            recent_casualties_penalty_6: -10,
            recent_casualties_penalty_10: -20,
            recent_casualties_penalty_15: -30,
            recent_casualties_penalty_33: -40,
            recent_casualties_penalty_50: -50,
            extended_casualties_penalty_10: -100,
            extended_casualties_penalty_15: -200,
            extended_casualties_penalty_33: -300,
            extended_casualties_penalty_50: -400,
            extended_casualties_penalty_80: -500,
            blood_bonus_5: 1,
            blood_bonus_7: 2,
            blood_bonus_12: 3,
            ..KvMorale::default()
        }
    }

    /// Inputs that make the evaluated morale exactly `m` (base 0, no effects).
    fn inputs(m: i32) -> MoraleInputs {
        MoraleInputs {
            active_state: 1,
            morale_stat: m,
            impetuous_allowed: true,
            may_break: true,
            // A negative own strength switches off 0x0053CBD0's "no enemy near" +4 (0x0D), so
            // the evaluated morale is exactly `m`.
            own_strength: -1.0,
            ..MoraleInputs::default()
        }
    }

    fn comp(state: MoraleState) -> MoraleComponent {
        MoraleComponent {
            state,
            ..MoraleComponent::default()
        }
    }

    /// Runs one evaluation from `state` at morale `m` and returns the new state.
    fn step(state: MoraleState, m: i32) -> MoraleState {
        let mut c = comp(state);
        evaluate(&mut c, &kv(), &inputs(m)).new_state
    }

    #[test]
    fn impetuous_edges() {
        assert_eq!(step(Impetuous, 90), Impetuous); // exactly at threshold: no change
        assert_eq!(step(Impetuous, 89), Eager);
    }

    #[test]
    fn eager_edges() {
        assert_eq!(step(Eager, 95), Eager);
        assert_eq!(step(Eager, 96), Impetuous);
        assert_eq!(step(Eager, 70), Eager);
        assert_eq!(step(Eager, 69), Confident);
        // Impetuous needs the +0x18A flag.
        let mut c = comp(Eager);
        let mut i = inputs(200);
        i.impetuous_allowed = false;
        assert_eq!(evaluate(&mut c, &kv(), &i).new_state, Eager);
    }

    #[test]
    fn confident_steady_shaken_edges() {
        assert_eq!(step(Confident, 75), Confident);
        assert_eq!(step(Confident, 76), Eager);
        assert_eq!(step(Confident, 50), Confident);
        assert_eq!(step(Confident, 49), Steady);
        assert_eq!(step(Steady, 55), Steady);
        assert_eq!(step(Steady, 56), Confident);
        assert_eq!(step(Steady, 30), Steady);
        assert_eq!(step(Steady, 29), Shaken);
        assert_eq!(step(Shaken, 35), Shaken);
        assert_eq!(step(Shaken, 36), Steady);
        assert_eq!(step(Shaken, 10), Shaken);
        assert_eq!(step(Shaken, 9), Wavering);
    }

    #[test]
    fn only_one_step_per_evaluation() {
        // Even a huge drop moves Steady only to Shaken.
        assert_eq!(step(Steady, -1000), Shaken);
        assert_eq!(step(Shaken, 1000), Steady);
    }

    #[test]
    fn entering_wavering_sets_timer_and_event() {
        let mut c = comp(Shaken);
        let out = evaluate(&mut c, &kv(), &inputs(9));
        assert!(out.wavering_event);
        assert_eq!(c.waver_timer, 111); // placeholder = waver_base_timeout
    }

    #[test]
    fn wavering_edges_with_expired_timer() {
        // Default waver_timer is -1 (expired).
        assert_eq!(step(Wavering, 15), Wavering);
        assert_eq!(step(Wavering, 16), Shaken);
        assert_eq!(step(Wavering, -10), Wavering);
        assert_eq!(step(Wavering, -11), Broken);
        // Wavering -> Broken also needs FUN_532370().
        let mut c = comp(Wavering);
        let mut i = inputs(-11);
        i.may_break = false;
        assert_eq!(evaluate(&mut c, &kv(), &i).new_state, Wavering);
    }

    #[test]
    fn wavering_timer_gate_and_clamp() {
        // Timer running (> 0): no transition, and morale is clamped up to wavering_lower.
        let mut c = comp(Wavering);
        c.waver_timer = 5;
        let out = evaluate(&mut c, &kv(), &inputs(-25));
        assert_eq!(out.new_state, Wavering);
        assert_eq!(c.morale, -10);
        // Upward transitions are gated too.
        let mut c = comp(Wavering);
        c.waver_timer = 5;
        assert_eq!(evaluate(&mut c, &kv(), &inputs(100)).new_state, Wavering);
        // Timer == 0 after the light update: no clamp (needs > 0) and still no transition
        // (needs < 0).
        let mut c = comp(Wavering);
        c.waver_timer = 1;
        let out = evaluate(&mut c, &kv(), &inputs(-25));
        assert_eq!(out.new_state, Wavering);
        assert_eq!(c.morale, -25);
        // Morale exactly at wavering_lower is not clamped (strict <), value unchanged anyway.
        let mut c = comp(Wavering);
        c.waver_timer = 5;
        evaluate(&mut c, &kv(), &inputs(-10));
        assert_eq!(c.morale, -10);
    }

    #[test]
    fn suppress_flag_disables_clamp() {
        // A stale flag is cleared at the start of the evaluation (CONFIRMED 16-bit store), so the
        // flag has to come from this evaluation: here the surprise timer (-11 morale).
        let mut c = comp(Wavering);
        c.waver_timer = 5;
        c.surprise_timer = 5;
        let mut i = inputs(-25);
        i.shock_persists = true; // FUN_53E980() keeps the flag
        evaluate(&mut c, &kv(), &i);
        assert_eq!(c.morale, -36);
        assert!(c.suppress_shaken_clamp);
        // If FUN_53E980() is false the flag is cleared before the clamp.
        let mut c = comp(Wavering);
        c.waver_timer = 5;
        c.surprise_timer = 5;
        i.shock_persists = false;
        evaluate(&mut c, &kv(), &i);
        assert!(!c.suppress_shaken_clamp);
        assert_eq!(c.morale, -10);
        // A flag left over from an earlier evaluation does not survive.
        let mut c = comp(Wavering);
        c.waver_timer = 5;
        c.suppress_shaken_clamp = true;
        evaluate(&mut c, &kv(), &inputs(-25));
        assert_eq!(c.morale, -10);
    }

    #[test]
    fn broken_edges_and_gate() {
        // Rout timer expired (-1).
        assert_eq!(step(Broken, -5), Broken);
        assert_eq!(step(Broken, -4), Wavering);
        assert_eq!(step(Broken, -30), Broken);
        assert_eq!(step(Broken, -31), Shattered);
        // Rout timer running: rallying is blocked...
        let mut c = comp(Broken);
        c.rout_timer = 3;
        assert_eq!(evaluate(&mut c, &kv(), &inputs(100)).new_state, Broken);
        // ...but shattering is not (the pre-check runs before the gate).
        let mut c = comp(Broken);
        c.rout_timer = 3;
        assert_eq!(evaluate(&mut c, &kv(), &inputs(-31)).new_state, Shattered);
        let mut c = comp(Broken);
        c.rout_timer = 0;
        assert_eq!(evaluate(&mut c, &kv(), &inputs(-30)).new_state, Broken);
    }

    #[test]
    fn shattered_is_final() {
        assert_eq!(step(Shattered, 1000), Shattered);
    }

    #[test]
    fn behaviour_and_rout_counter() {
        let mut c = comp(Wavering);
        c.behaviour = MoraleBehaviour::Normal;
        let out = evaluate(&mut c, &kv(), &inputs(-11));
        assert!(out.started_routing);
        assert_eq!(c.behaviour, MoraleBehaviour::Routing);
        assert_eq!(c.rout_timer, 222); // broken_finish_base_timeout - unit index 0 * 20
        assert_eq!(c.times_routed, 1);
        // Shattering while routing (no rally possible): state 7, but the evaluation ends before
        // the behaviour update, so the mode stays "routing" (CONFIRMED early return).
        let mut c2 = c.clone();
        c2.rout_timer = -1;
        evaluate(&mut c2, &kv(), &inputs(-31));
        assert_eq!(c2.state, Shattered);
        assert_eq!(c2.behaviour, MoraleBehaviour::Routing);
        assert_eq!(c2.times_routed, 1);
        // A unit that breaks straight to Shattered from Wavering is mode 3.
        let mut c4 = comp(Broken);
        c4.rout_timer = -1;
        evaluate(&mut c4, &kv(), &inputs(-31));
        assert_eq!(c4.behaviour, MoraleBehaviour::Shattered);
        // Rallying back to Wavering: mode 0 and a new waver timer.
        let mut c3 = c.clone();
        c3.rout_timer = -1;
        let mut i3 = inputs(0);
        i3.can_rally = true; // routing units change state only when the rally test passes
        evaluate(&mut c3, &kv(), &i3);
        assert_eq!(c3.state, Wavering);
        assert_eq!(c3.behaviour, MoraleBehaviour::Normal);
        assert_eq!(c3.waver_timer, 111);
    }

    #[test]
    fn inactive_or_flagged_units_are_skipped() {
        let mut c = comp(Steady);
        let mut i = inputs(-1000);
        i.active_state = 0;
        assert!(!evaluate(&mut c, &kv(), &i).evaluated);
        assert_eq!(c.state, Steady);
        let mut c = comp(Steady);
        c.skip_flags[1] = true;
        assert!(!evaluate(&mut c, &kv(), &inputs(-1000)).evaluated);
        assert_eq!(c.state, Steady);
    }

    #[test]
    fn situational_effects_are_summed() {
        let mut c = comp(Steady);
        c.persistent_bonus = 4;
        // The evaluation runs the light update first, so each timer is one tick longer here.
        c.surprise_timer = 1; // still >= 0 after the light update: active
        c.charge_timer = 2; // still > 0 after the light update: active
        c.transient_effects.push(TransientEffect {
            id: 99,
            value: 13,
            seconds_left: 5.0,
        });
        let mut i = inputs(40);
        i.under_projectile_fire = true;
        i.under_artillery_fire = true;
        i.shock_persists = true;
        evaluate(&mut c, &kv(), &i);
        // 40 + 4 + 13 - 3 - 5 - 11 + 7 = 45
        assert_eq!(c.morale, 45);
        assert!(c.suppress_shaken_clamp);
        assert_eq!(c.effect(effect_id::SURPRISED), Some(-11));
    }

    // ---- modifiers (W1 §12.4) ----

    fn run_mods(i: &MoraleInputs) -> MoraleComponent {
        let mut c = MoraleComponent::default();
        modifiers(&mut c, &kv(), i);
        c
    }

    #[test]
    fn category_table() {
        for (cat, id, v) in [
            (1, effect_id::CATEGORY_BONUS, Some(6)),
            (2, effect_id::CATEGORY_BONUS, Some(4)),
            (3, effect_id::CATEGORY_BONUS, Some(2)),
            (4, effect_id::CATEGORY_BONUS, None),
            (5, effect_id::CATEGORY_PENALTY, None),
            (6, effect_id::CATEGORY_PENALTY, Some(-4)),
            (7, effect_id::CATEGORY_PENALTY, Some(-8)),
        ] {
            let c = run_mods(&MoraleInputs {
                category: cat,
                ..Default::default()
            });
            assert_eq!(c.effect(id), v, "category {cat}");
        }
    }

    fn attacker(direction: AttackDirection) -> Attacker {
        Attacker {
            direction,
            excluded: false,
            doubles_front_value: false,
            plus_two_condition: false,
        }
    }

    #[test]
    fn attack_directions() {
        let mut a = attacker(AttackDirection::Front);
        a.doubles_front_value = true;
        let i = MoraleInputs {
            formation_class: 2,
            attackers: vec![
                a,
                attacker(AttackDirection::Flank),
                attacker(AttackDirection::Rear),
            ],
            ..Default::default()
        };
        let c = run_mods(&i);
        assert_eq!(c.effect(effect_id::ATTACKED_FRONT), Some(-10 * 2 - 2));
        assert_eq!(c.effect(effect_id::ATTACKED_FLANK), Some(-20));
        assert_eq!(c.effect(effect_id::ATTACKED_REAR), Some(-30));

        // Class 0 -> -4; class 4 with the "other condition" -> +2; excluded attackers do nothing.
        let i = MoraleInputs {
            formation_class: 0,
            attackers: vec![attacker(AttackDirection::Front)],
            ..Default::default()
        };
        assert_eq!(run_mods(&i).effect(effect_id::ATTACKED_FRONT), Some(-14));
        let mut a = attacker(AttackDirection::Front);
        a.plus_two_condition = true;
        let i = MoraleInputs {
            formation_class: 4,
            attackers: vec![a],
            ..Default::default()
        };
        assert_eq!(run_mods(&i).effect(effect_id::ATTACKED_FRONT), Some(-8));
        a.excluded = true;
        let i = MoraleInputs {
            formation_class: 4,
            attackers: vec![a],
            ..Default::default()
        };
        assert_eq!(run_mods(&i).effect(effect_id::ATTACKED_FRONT), None);
    }

    #[test]
    fn fighting_cavalry_signed_shift() {
        let k = kv(); // fighting_cavalry = -5 (made up)
        assert_eq!(fighting_cavalry_value(&k, 0), Some(-5));
        assert_eq!(fighting_cavalry_value(&k, 1), Some(-5));
        assert_eq!(fighting_cavalry_value(&k, 2), Some(-3)); // -5 >> 1 = -3, not -2
        assert_eq!(fighting_cavalry_value(&k, 3), Some(-2)); // -5 >> 2 = -2, not -1
        assert_eq!(fighting_cavalry_value(&k, 4), None);
        assert_eq!(fighting_cavalry_value(&k, 5), None);
        let i = MoraleInputs {
            fighting_cavalry: true,
            formation_class: 2,
            ..Default::default()
        };
        assert_eq!(run_mods(&i).effect(effect_id::FIGHTING_CAVALRY), Some(-3));
    }

    fn total(r: f32) -> Option<i32> {
        run_mods(&MoraleInputs {
            total_casualty_ratio: r,
            ..Default::default()
        })
        .effect(effect_id::TOTAL_CASUALTIES)
    }

    #[test]
    fn total_casualties_strictly_greater() {
        assert_eq!(total(0.2), None); // exactly 0.2 is NOT > 0.2
        assert_eq!(total(0.21), Some(-1));
        assert_eq!(total(0.4), Some(-1));
        assert_eq!(total(0.41), Some(-2));
        assert_eq!(total(0.6), Some(-2));
        assert_eq!(total(0.61), Some(-3));
        assert_eq!(total(0.8), Some(-3));
        assert_eq!(total(0.81), Some(-4));
        assert_eq!(total(0.9), Some(-4));
        assert_eq!(total(0.91), Some(-5));
    }

    fn recent(r: f32) -> MoraleComponent {
        run_mods(&MoraleInputs {
            recent_casualty_ratio: r,
            ..Default::default()
        })
    }

    #[test]
    fn recent_casualties_greater_or_equal() {
        assert_eq!(recent(0.059).effect(effect_id::RECENT_CASUALTIES), None);
        assert_eq!(recent(0.06).effect(effect_id::RECENT_CASUALTIES), Some(-10)); // exactly 0.06 counts
        assert_eq!(recent(0.10).effect(effect_id::RECENT_CASUALTIES), Some(-20));
        assert_eq!(recent(0.15).effect(effect_id::RECENT_CASUALTIES), Some(-30));
        assert_eq!(recent(0.33).effect(effect_id::RECENT_CASUALTIES), Some(-40));
        assert_eq!(recent(0.5).effect(effect_id::RECENT_CASUALTIES), Some(-50));
        // Shock flag at shock_threshold * 0.01 = 0.25 (made-up threshold 25).
        assert!(!recent(0.24).suppress_shaken_clamp);
        assert!(recent(0.25).suppress_shaken_clamp);
    }

    #[test]
    fn extended_and_blood_greater_or_equal() {
        let ext = |r: f32| {
            run_mods(&MoraleInputs {
                extended_casualty_ratio: r,
                ..Default::default()
            })
            .effect(effect_id::EXTENDED_CASUALTIES)
        };
        assert_eq!(ext(0.09), None);
        assert_eq!(ext(0.1), Some(-100));
        assert_eq!(ext(0.15), Some(-200));
        assert_eq!(ext(0.33), Some(-300));
        assert_eq!(ext(0.5), Some(-400));
        assert_eq!(ext(0.8), Some(-500));
        let blood = |r: f32| {
            run_mods(&MoraleInputs {
                kill_ratio: r,
                ..Default::default()
            })
            .effect(effect_id::BLOOD)
        };
        assert_eq!(blood(0.049), None);
        assert_eq!(blood(0.05), Some(1));
        assert_eq!(blood(0.075), Some(2));
        assert_eq!(blood(0.125), Some(3));
    }

    #[test]
    fn light_update_matches_0x00585be0() {
        // Steady: only the surprise and charge timers run; waver/rout timers are state-gated.
        let mut c = MoraleComponent {
            waver_timer: 3,
            rout_timer: 3,
            surprise_timer: 0,
            charge_timer: 1,
            ..Default::default()
        };
        c.transient_effects.push(TransientEffect { id: 1, value: 5, seconds_left: 0.15 });
        light_update(&mut c, &kv(), false);
        assert_eq!(
            (c.waver_timer, c.rout_timer, c.surprise_timer, c.charge_timer),
            (3, 3, -1, 0)
        );
        assert_eq!(c.transient_effects.len(), 1, "0.15 - 0.1 > 0 survives");
        light_update(&mut c, &kv(), false);
        assert_eq!(c.charge_timer, -1, "the charge timer runs down to -1");
        assert!(c.transient_effects.is_empty(), "0.05 - 0.1 <= 0 is removed");
        // Wavering: the waver timer runs; Broken: the rout timer runs.
        c.state = Wavering;
        light_update(&mut c, &kv(), false);
        assert_eq!((c.waver_timer, c.rout_timer), (2, 3));
        c.state = Broken;
        light_update(&mut c, &kv(), false);
        assert_eq!((c.waver_timer, c.rout_timer), (2, 2));
        // Charging with an expired timer restarts it at charge_timeout.
        let mut k = kv();
        k.charge_timeout = 30;
        light_update(&mut c, &k, true);
        assert_eq!(c.charge_timer, 30);
        light_update(&mut c, &k, true);
        assert_eq!(c.charge_timer, 29, "a running timer is not restarted");
        // Skip flags stop everything.
        c.skip_flags[0] = true;
        light_update(&mut c, &k, true);
        assert_eq!(c.charge_timer, 29);
    }

    #[test]
    fn timers_scale_with_experience() {
        let k = kv(); // waver 111, rout 222
        assert_eq!(waver_timeout(&k, 0), 111);
        assert_eq!(waver_timeout(&k, 7), 146);
        assert_eq!(rout_timeout(&k, 3), 162);
        assert_eq!(rout_timeout(&k, 200), 0, "floored at 0");
    }

    #[test]
    fn add_effect_keeps_duplicates_sorted_and_capped() {
        let mut c = MoraleComponent::default();
        c.add_effect(1, 5);
        c.add_effect(2, -3); // key 6
        c.add_effect(1, 5); // duplicate id: kept
        c.add_effect(3, 6); // key 6, after the equal key
        let ids: Vec<i32> = c.active_effects.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![2, 3, 1, 1]);
        assert_eq!(c.effects_sum(), 13);
        for _ in 0..60 {
            c.add_effect(9, 1);
        }
        assert_eq!(c.active_effects.len(), MAX_ACTIVE_EFFECTS);
    }

    #[test]
    fn fatigue_and_column_sub_evaluators() {
        let mut k = kv();
        k.ume_concerned_tired = -2;
        k.ume_concerned_very_tired = -4;
        k.ume_concerned_exhausted = -7;
        k.ume_encouraged_column_formation = 3;
        let mut c = MoraleComponent::default();
        let mut i = inputs(0);
        i.fatigue_level = 5;
        sub_fatigue(&mut c, &k, &i);
        assert_eq!(c.effect(fatigue_effect_id::EXHAUSTED), Some(-7));
        i.formation_class = 4;
        i.fatigue_level = 4;
        sub_fatigue(&mut c, &k, &i);
        assert_eq!(c.effect(fatigue_effect_id::VERY_TIRED), Some(-2));
        i.formation_class = 5;
        i.fatigue_level = 3;
        sub_fatigue(&mut c, &k, &i);
        assert_eq!(c.effect(fatigue_effect_id::TIRED), Some(0));
        i.column_formation = true;
        sub_column_formation(&mut c, &k, &i);
        assert_eq!(c.effect(COLUMN_FORMATION_EFFECT), Some(3));
    }

    #[test]
    fn routing_units_rally_only_when_allowed() {
        let k = kv();
        let mut c = comp(Broken);
        c.behaviour = MoraleBehaviour::Routing;
        c.rout_timer = 0; // runs out in this evaluation's light update
        c.times_routed = 1;
        // Not allowed: nothing but the shatter check runs, even with high morale.
        let mut i = inputs(50);
        i.can_rally = false;
        evaluate(&mut c, &k, &i);
        assert_eq!((c.state, c.behaviour), (Broken, MoraleBehaviour::Routing));
        // Allowed with the timer run out: rallies to Wavering with a fresh waver timer.
        i.can_rally = true;
        i.experience = 2;
        evaluate(&mut c, &k, &i);
        assert_eq!((c.state, c.behaviour), (Wavering, MoraleBehaviour::Normal));
        assert_eq!(c.waver_timer, waver_timeout(&k, 2));
        assert!(c.rally_flag);
        // Not allowed and below broken_lower: shatters.
        let mut c = comp(Broken);
        c.behaviour = MoraleBehaviour::Routing;
        c.rout_timer = 50;
        let mut i = inputs(-40);
        i.can_rally = false;
        evaluate(&mut c, &k, &i);
        assert_eq!(c.state, Shattered);
        assert_eq!(c.behaviour, MoraleBehaviour::Routing, "behaviour is not updated");
    }

    #[test]
    fn rally_test_conditions() {
        let mut c = comp(Broken);
        c.behaviour = MoraleBehaviour::Routing;
        c.times_routed = 1;
        assert!(rally_test(&c, 30, 120, false, 90.0, false, &[29.0], true));
        assert!(!rally_test(&c, 29, 120, false, 90.0, false, &[], true), "needs start/4 men");
        assert!(!rally_test(&c, 30, 120, false, 90.0, false, &[30.0], true), "enemy >= 1/3");
        assert!(rally_test(&c, 30, 120, false, 90.0, true, &[44.0], true), "flag +0x189: 1/2");
        assert!(!rally_test(&c, 30, 120, true, 90.0, false, &[], true), "engaged");
        c.times_routed = 3;
        assert!(!rally_test(&c, 120, 120, false, 90.0, false, &[], true), "3 routs");
    }
}

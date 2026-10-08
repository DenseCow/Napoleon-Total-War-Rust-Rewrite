//! Soldier fatigue: per-action accumulation and the 6-state machine.
//!
//! W1 §12.5, `0x00670F40` (accumulation) and `0x00671230` (state machine), CONFIRMED structure.
//!
//! Each soldier has an integer fatigue value (`+0x370`) and a state 0..5 (`+0x374`). Every tick the
//! soldier's current action picks a `kv_fatigue` value (e.g. `walking`, `combat`), slopes scale it,
//! and the result is added, together with the unit's own terms: `+0x18E` stamina, the battle
//! climate's `+0x2C`/`+0x30` (unless the unit is exempt) and column 6 (`+0x20`) of the unit's
//! `unit_stats_land_experience_bonuses` row, picked by its experience level (`+0xD48`). The state
//! then moves at most one step using strict comparisons, and the value is clamped to
//! `[threshold_fresh, threshold_max]`. A unit's fatigue is the integer mean of its soldiers
//! (`0x006FB080`).

/// The 6 fatigue states (`+0x374`). W1 §12.5 (CONFIRMED values 0..5; names from the
/// `threshold_*` keys, INFERRED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum FatigueState {
    /// 0.
    #[default]
    Fresh = 0,
    /// 1, above `threshold_active`.
    Active = 1,
    /// 2, above `threshold_winded`.
    Winded = 2,
    /// 3, above `threshold_tired`.
    Tired = 3,
    /// 4, above `threshold_very_tired`.
    VeryTired = 4,
    /// 5, above `threshold_exhausted`.
    Exhausted = 5,
}

/// The stat multipliers of one fatigue level for a unit (`fatigue_effects`, round 13). The exe
/// gets them through `0x00649520`: the unit record's table `+0xE4` indexed by the unit's fatigue
/// level `+0xC70`, null for the levels without a row (fresh, active, winded), and the readers then
/// use 1.0. Fields (CONFIRMED readers): speed `+0x10` (`0x006543D0`), charge `+0x14`
/// (`0x006B30E0`), control `+0x18` (`0x006B8AA0`), attack `+0x1C` (`0x006A8290`). Each is
/// `1 + the table value` (INFERRED: the readers multiply by it; the table holds −0.05 … −0.5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FatigueEffects {
    /// Speed multiplier (`fatigue_effects` col 2).
    pub speed: f32,
    /// Charge bonus multiplier (col 3).
    pub charge: f32,
    /// Missile "control" base (col 4).
    pub control: f32,
    /// Melee attack multiplier (col 5).
    pub attack: f32,
}

impl FatigueEffects {
    /// No effect (a level without a row).
    pub const NONE: Self = FatigueEffects { speed: 1.0, charge: 1.0, control: 1.0, attack: 1.0 };
}

impl Default for FatigueEffects {
    fn default() -> Self {
        Self::NONE
    }
}

/// The `kv_fatigue` table, every key in game-data order (W1 `kv_layout.tsv`, CONFIRMED key list).
/// All values are integers (truncated on load, W1 §9). `Default` is all zeros: **no game data here**.
#[allow(missing_docs)] // each field is named exactly like its kv_fatigue key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KvFatigue {
    pub idle: i32,
    pub idle_in_building: i32,
    pub idle_rain: i32,
    pub idle_snow: i32,
    pub limbering: i32,
    pub ready: i32,
    pub shooting: i32,
    pub reloading: i32,
    pub reloading_artillery: i32,
    pub working: i32,
    pub walking: i32,
    pub walking_artillery: i32,
    pub walking_horse_artillery: i32,
    pub running: i32,
    pub running_artillery_horse: i32,
    pub running_cavalry: i32,
    pub running_cavalry_light: i32,
    pub charging: i32,
    pub combat: i32,
    pub tight_formation: i32,
    pub under_fire_artillery: i32,
    pub under_fire_small_arms: i32,
    pub threshold_fresh: i32,
    pub threshold_active: i32,
    pub threshold_winded: i32,
    pub threshold_tired: i32,
    pub threshold_very_tired: i32,
    pub threshold_exhausted: i32,
    pub threshold_max: i32,
    pub gradient_shallow_movement_multiplier: i32,
    pub gradient_steep_movement_multiplier: i32,
    pub gradient_very_steep_movement_multiplier: i32,
}

/// What a soldier is doing, named after the `kv_fatigue` key it uses.
///
/// UNKNOWN (W1 §12.5): the original switches on its own action enum (`+0x1B8`, values 0..0x4E);
/// the mapping from those 79 values to these keys has not been extracted yet.
#[allow(missing_docs)] // each variant is named like its kv_fatigue key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FatigueAction {
    Idle,
    IdleInBuilding,
    Limbering,
    Ready,
    Shooting,
    Reloading,
    ReloadingArtillery,
    Working,
    Walking,
    WalkingArtillery,
    WalkingHorseArtillery,
    Running,
    RunningArtilleryHorse,
    RunningCavalry,
    RunningCavalryLight,
    Charging,
    Combat,
    TightFormation,
    UnderFireArtillery,
    UnderFireSmallArms,
}

/// Battle weather that affects idle recovery. W1 §12.5 (CONFIRMED: `idle_rain`, `idle_snow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Weather {
    /// Neither rain nor snow.
    #[default]
    Clear,
    /// Raining.
    Rain,
    /// Snowing.
    Snow,
}

/// Per-tick inputs for one soldier (or the representative soldier of a unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FatigueInputs {
    /// Current action (`+0x1B8`, see [`FatigueAction`]).
    pub action: FatigueAction,
    /// Weather from the battle.
    pub weather: Weather,
    /// The slope test (CONFIRMED: soldier `+0x118 == 1 && (+0x110 & 1 || +0x117 == 0)`, the same
    /// predicate as `0x0064CAA0`; field meanings UNKNOWN, see BATTLE_FIDELITY.md §4).
    pub moving_on_slope: bool,
    /// Ground gradient (`+0x1A0`).
    pub gradient: f32,
    /// Unit flag `+0x18E` (`unit_stats_land` col 76, INFERRED "good stamina"): −1 per tick.
    pub unit_flag_18e: bool,
    /// Climate terms from the battle (`+0x2C`, `+0x30`) after the unit's `+0x18F/+0x190` exemptions,
    /// see [`climate_term`].
    pub climate_term: i32,
    /// The unit's experience (`+0xD48`) row bonus, i.e. column 6 (`+0x20`) of
    /// `unit_stats_land_experience_bonuses`: added to every soldier's fatigue each tick (veterans
    /// tire more slowly). CONFIRMED read in `0x00670F40`; look it up with [`experience_bonuses`].
    pub experience_bonus: i32,
}

/// The experience-bonus row for a unit's experience level, the way `0x00670F40` picks it:
/// **by row position**, `if (experience < count) rows[experience]`, else 0.
///
/// `rows` is column 6 (`+0x20`) of `unit_stats_land_experience_bonuses` in file order (rank 0..9;
/// [`ntw_data::GameDatabase::experience_fatigue_bonuses`] builds it). The exe's else branch leaves
/// the row pointer null and then reads `+0x20` off it (a latent null deref it never hits, because
/// the byte is a 0..9 level and the table ships 10 rows); we return 0 instead.
pub fn experience_bonuses(rows: &[i32], experience: u8) -> i32 {
    rows.get(experience as usize).copied().unwrap_or(0)
}

/// The climate part of `0x00670F40` (CONFIRMED, BATTLE_FIDELITY.md §4): the battle climate's
/// `+0x2C` unless unit flag `+0x18F` (col 78, INFERRED heat resistant), plus its `+0x30` unless unit
/// flag `+0x190` (col 79, INFERRED cold resistant). Without a unit both are added. Which battle
/// values `+0x2C` / `+0x30` hold is UNKNOWN (the caller passes 0 until they are found).
pub fn climate_term(c2c: i32, c30: i32, exempt_2c: bool, exempt_30: bool) -> i32 {
    let mut t = 0i32;
    if !exempt_2c {
        t = t.wrapping_add(c2c);
    }
    if !exempt_30 {
        t = t.wrapping_add(c30);
    }
    t
}

/// The per-action base delta, before slope scaling. W1 §12.5 (CONFIRMED: idle adds `idle_rain` when
/// raining or `idle_snow` when snowing).
pub fn action_delta(kv: &KvFatigue, action: FatigueAction, weather: Weather) -> i32 {
    use FatigueAction::*;
    match action {
        Idle => {
            kv.idle
                + match weather {
                    Weather::Clear => 0,
                    Weather::Rain => kv.idle_rain,
                    Weather::Snow => kv.idle_snow,
                }
        }
        IdleInBuilding => kv.idle_in_building,
        Limbering => kv.limbering,
        Ready => kv.ready,
        Shooting => kv.shooting,
        Reloading => kv.reloading,
        ReloadingArtillery => kv.reloading_artillery,
        Working => kv.working,
        Walking => kv.walking,
        WalkingArtillery => kv.walking_artillery,
        WalkingHorseArtillery => kv.walking_horse_artillery,
        Running => kv.running,
        RunningArtilleryHorse => kv.running_artillery_horse,
        RunningCavalry => kv.running_cavalry,
        RunningCavalryLight => kv.running_cavalry_light,
        Charging => kv.charging,
        Combat => kv.combat,
        TightFormation => kv.tight_formation,
        UnderFireArtillery => kv.under_fire_artillery,
        UnderFireSmallArms => kv.under_fire_small_arms,
    }
}

/// Slope multiplier (percent) for a gradient. W1 §12.5 (CONFIRMED, strict `>`):
/// `g > 0.2` → very steep, `g > 0.1` → steep, `g > 0.05` → shallow, else none.
pub fn gradient_multiplier(kv: &KvFatigue, gradient: f32) -> Option<i32> {
    if gradient > 0.2 {
        Some(kv.gradient_very_steep_movement_multiplier)
    } else if gradient > 0.1 {
        Some(kv.gradient_steep_movement_multiplier)
    } else if gradient > 0.05 {
        Some(kv.gradient_shallow_movement_multiplier)
    } else {
        None
    }
}

/// Applies the slope multiplier with **integer** math: `(m * delta) / 100` (truncates toward zero,
/// like C). W1 §12.5 (CONFIRMED).
pub fn apply_gradient(m: i32, delta: i32) -> i32 {
    m.wrapping_mul(delta) / 100
}

/// The total change for one tick (`0x00670F40`). W1 §12.5.
pub fn tick_delta(kv: &KvFatigue, i: &FatigueInputs) -> i32 {
    let mut delta = action_delta(kv, i.action, i.weather);
    if i.moving_on_slope
        && let Some(m) = gradient_multiplier(kv, i.gradient)
    {
        delta = apply_gradient(m, delta);
    }
    if i.unit_flag_18e {
        delta -= 1;
    }
    delta.wrapping_add(i.climate_term).wrapping_add(i.experience_bonus)
}

/// One step of the state machine (`0x00671230`), strict comparisons. W1 §12.5 (CONFIRMED).
pub fn next_state(kv: &KvFatigue, state: FatigueState, fatigue: i32) -> FatigueState {
    use FatigueState::*;
    match state {
        Fresh if fatigue > kv.threshold_active => Active,
        Active if fatigue < kv.threshold_active => Fresh,
        Active if fatigue > kv.threshold_winded => Winded,
        Winded if fatigue < kv.threshold_winded => Active,
        Winded if fatigue > kv.threshold_tired => Tired,
        Tired if fatigue < kv.threshold_tired => Winded,
        Tired if fatigue > kv.threshold_very_tired => VeryTired,
        VeryTired if fatigue < kv.threshold_very_tired => Tired,
        VeryTired if fatigue > kv.threshold_exhausted => Exhausted,
        Exhausted if fatigue < kv.threshold_exhausted => VeryTired,
        s => s,
    }
}

/// Clamps fatigue to `[threshold_fresh, threshold_max]`. W1 §12.5 (CONFIRMED).
/// Written as two plain comparisons so bad data (fresh > max) cannot panic.
pub fn clamp(kv: &KvFatigue, fatigue: i32) -> i32 {
    if fatigue < kv.threshold_fresh {
        kv.threshold_fresh
    } else if fatigue > kv.threshold_max {
        kv.threshold_max
    } else {
        fatigue
    }
}

/// Runs one full fatigue tick on one soldier: add the delta, step the state, clamp.
/// W1 §12.5 order: `fatigue += delta + terms`, then the state machine, then the clamp.
pub fn tick(kv: &KvFatigue, fatigue: &mut i32, state: &mut FatigueState, i: &FatigueInputs) {
    *fatigue = fatigue.wrapping_add(tick_delta(kv, i));
    *state = next_state(kv, *state, *fatigue);
    *fatigue = clamp(kv, *fatigue);
}

/// A unit's fatigue = integer mean of its soldiers (`0x006FB080`). W1 §12.5 (CONFIRMED).
/// Truncates toward zero like C. An empty unit gives 0 (INFERRED; the original divides by the count).
pub fn unit_fatigue(soldiers: &[i32]) -> i32 {
    if soldiers.is_empty() {
        return 0;
    }
    let sum = soldiers.iter().fold(0i32, |a, &f| a.wrapping_add(f));
    sum / soldiers.len() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use FatigueState::*;

    /// OBVIOUSLY MADE-UP placeholder values (NOT game data): round numbers so edges are easy to hit.
    fn kv() -> KvFatigue {
        KvFatigue {
            idle: -2,
            idle_rain: 1,
            idle_snow: 3,
            walking: 10,
            running: 33,
            combat: 50,
            threshold_fresh: 0,
            threshold_active: 100,
            threshold_winded: 200,
            threshold_tired: 300,
            threshold_very_tired: 400,
            threshold_exhausted: 500,
            threshold_max: 600,
            gradient_shallow_movement_multiplier: 150,
            gradient_steep_movement_multiplier: 200,
            gradient_very_steep_movement_multiplier: 300,
            ..KvFatigue::default()
        }
    }

    #[test]
    fn state_edges_are_strict() {
        let k = kv();
        assert_eq!(next_state(&k, Fresh, 100), Fresh);
        assert_eq!(next_state(&k, Fresh, 101), Active);
        assert_eq!(next_state(&k, Active, 100), Active);
        assert_eq!(next_state(&k, Active, 99), Fresh);
        assert_eq!(next_state(&k, Active, 200), Active);
        assert_eq!(next_state(&k, Active, 201), Winded);
        assert_eq!(next_state(&k, Winded, 199), Active);
        assert_eq!(next_state(&k, Winded, 301), Tired);
        assert_eq!(next_state(&k, Tired, 299), Winded);
        assert_eq!(next_state(&k, Tired, 401), VeryTired);
        assert_eq!(next_state(&k, VeryTired, 399), Tired);
        assert_eq!(next_state(&k, VeryTired, 500), VeryTired);
        assert_eq!(next_state(&k, VeryTired, 501), Exhausted);
        assert_eq!(next_state(&k, Exhausted, 500), Exhausted);
        assert_eq!(next_state(&k, Exhausted, 499), VeryTired);
        // One step at a time.
        assert_eq!(next_state(&k, Fresh, 9999), Active);
    }

    #[test]
    fn gradient_ladder_and_integer_scaling() {
        let k = kv();
        assert_eq!(gradient_multiplier(&k, 0.05), None);
        assert_eq!(gradient_multiplier(&k, 0.051), Some(150));
        assert_eq!(gradient_multiplier(&k, 0.1), Some(150));
        assert_eq!(gradient_multiplier(&k, 0.11), Some(200));
        assert_eq!(gradient_multiplier(&k, 0.2), Some(200));
        assert_eq!(gradient_multiplier(&k, 0.21), Some(300));
        assert_eq!(apply_gradient(150, 33), 49); // 4950 / 100 = 49 (truncated)
        assert_eq!(apply_gradient(150, -3), -4); // -450 / 100 = -4 (toward zero)
    }

    #[test]
    fn deltas() {
        let k = kv();
        let base = FatigueInputs {
            action: FatigueAction::Idle,
            weather: Weather::Clear,
            moving_on_slope: false,
            gradient: 0.0,
            unit_flag_18e: false,
            climate_term: 0,
            experience_bonus: 0,
        };
        assert_eq!(tick_delta(&k, &base), -2);
        assert_eq!(
            tick_delta(
                &k,
                &FatigueInputs {
                    weather: Weather::Rain,
                    ..base
                }
            ),
            -1
        );
        assert_eq!(
            tick_delta(
                &k,
                &FatigueInputs {
                    weather: Weather::Snow,
                    ..base
                }
            ),
            1
        );
        let run_up = FatigueInputs {
            action: FatigueAction::Running,
            moving_on_slope: true,
            gradient: 0.15,
            ..base
        };
        assert_eq!(tick_delta(&k, &run_up), 66);
        assert_eq!(
            tick_delta(
                &k,
                &FatigueInputs {
                    unit_flag_18e: true,
                    climate_term: 5,
                    ..run_up
                }
            ),
            70
        );
    }

    /// The experience term: `0x00670F40` reads column 6 (`+0x20`) of the
    /// `unit_stats_land_experience_bonuses` row at the unit's experience byte. The values below are
    /// the REAL ones for ranks 5..9 (0,0,0,0,0,-1,-1,-2,-2,-3).
    #[test]
    fn experience_bonus_is_added_and_bounded() {
        let rows = [0, 0, 0, 0, 0, -1, -1, -2, -2, -3];
        assert_eq!(experience_bonuses(&rows, 0), 0);
        assert_eq!(experience_bonuses(&rows, 4), 0);
        assert_eq!(experience_bonuses(&rows, 7), -2);
        assert_eq!(experience_bonuses(&rows, 9), -3);
        // The exe's out-of-range branch leaves the row null and reads +0x20 off it; we give 0.
        assert_eq!(experience_bonuses(&rows, 10), 0);
        assert_eq!(experience_bonuses(&[], 9), 0);
        let k = kv();
        let base = FatigueInputs {
            action: FatigueAction::Combat,
            weather: Weather::Clear,
            moving_on_slope: false,
            gradient: 0.0,
            unit_flag_18e: false,
            climate_term: 0,
            experience_bonus: 0,
        };
        assert_eq!(tick_delta(&k, &base), 50);
        assert_eq!(tick_delta(&k, &FatigueInputs { experience_bonus: -3, ..base }), 47);
    }

    #[test]
    fn tick_clamps_after_state_change() {
        let k = kv();
        let mut f = 590;
        let mut s = Exhausted;
        let i = FatigueInputs {
            action: FatigueAction::Combat,
            weather: Weather::Clear,
            moving_on_slope: false,
            gradient: 0.0,
            unit_flag_18e: false,
            climate_term: 0,
            experience_bonus: 0,
        };
        tick(&k, &mut f, &mut s, &i);
        assert_eq!(f, 600);
        let mut f = 1;
        let mut s = Fresh;
        tick(
            &k,
            &mut f,
            &mut s,
            &FatigueInputs {
                action: FatigueAction::Idle,
                ..i
            },
        );
        assert_eq!(f, 0);
    }

    #[test]
    fn unit_mean_truncates() {
        assert_eq!(unit_fatigue(&[10, 11]), 10);
        assert_eq!(unit_fatigue(&[1, 2, 2]), 1);
        assert_eq!(unit_fatigue(&[-3, 0]), -1);
        assert_eq!(unit_fatigue(&[]), 0);
    }
}

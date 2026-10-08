//! Missile range, accuracy, shot dispersion, chance to hit and projectile impact.
//! W1 §12.6 and §12.10 (CONFIRMED formulas).
//!
//! Inputs whose meaning is not yet known (the "state 7" that shortens range, the accuracy
//! "mode") are plain parameters, documented as UNKNOWN, so the caller decides.

use super::rules::{KvRules, c_div, x87_round};
use crate::rng::CaRng;

/// Range multiplier while the unit is in "state 7" (`FUN_0055C9A0(7)`). W1 §12.6, 0x005646A0 (CONFIRMED).
pub const REDUCED_RANGE_MULTIPLIER: f32 = 0.8;
/// Aspect used for owner types 0 and 0xB: 1/√2 as `f32`. W1 §12.6, 0x006A5CE0 (CONFIRMED).
pub const NARROW_ASPECT: f32 = 0.707_106_77;

/// Effective missile range, `0x005646A0`. W1 §12.6 (CONFIRMED):
/// ```text
/// if !has_missile || !weapon: 0
/// elif on_walls: weapon.range + fire_on_walls_range_modifier
/// elif state(7): weapon.range * 0.8
/// else weapon.range
/// ```
/// - `weapon_range`: `None` when the unit has no missile weapon record (`+0xDC4`), else its int range (`+0x60`).
/// - `reduced_range_state`: `FUN_0055C9A0(7)`: ability 7 (fire and advance) is active (INFERRED).
/// - `fire_on_walls_range_modifier`: the kv_rules slot of that name (game data).
pub fn range(
    has_missile: bool,
    weapon_range: Option<i32>,
    on_walls: bool,
    reduced_range_state: bool,
    fire_on_walls_range_modifier: i32,
) -> f32 {
    match weapon_range {
        Some(r) if has_missile => {
            if on_walls {
                r.wrapping_add(fire_on_walls_range_modifier) as f32
            } else if reduced_range_state {
                r as f32 * REDUCED_RANGE_MULTIPLIER
            } else {
                r as f32
            }
        }
        _ => 0.0,
    }
}

/// The accuracy "mode" field (`+8`) of `0x006D8B00`. Meaning UNKNOWN (W1 §12.6: perhaps an
/// ability or experience tier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccuracyMode {
    /// Any value other than 1 or 2: no bonus.
    Other,
    /// Mode 1.
    Mode1,
    /// Mode 2.
    Mode2,
}

/// Missile accuracy, `0x006D8B00`. W1 §12.6 (CONFIRMED):
/// ```text
/// acc = unit record accuracy (f32)
/// if on_walls: acc += fire_on_walls_accuracy_modifier
/// if flag(+0xC) == 0 { mode1 -> +20 ; mode2 -> +30 } else { mode1 -> +15 }
/// ```
/// `flag_c` is the UNKNOWN flag at `+0xC`.
pub fn accuracy(
    base_accuracy: f32,
    on_walls: bool,
    fire_on_walls_accuracy_modifier: i32,
    mode: AccuracyMode,
    flag_c: bool,
) -> f32 {
    let mut acc = base_accuracy;
    if on_walls {
        acc += fire_on_walls_accuracy_modifier as f32;
    }
    match (flag_c, mode) {
        (false, AccuracyMode::Mode1) => acc += 20.0,
        (false, AccuracyMode::Mode2) => acc += 30.0,
        (true, AccuracyMode::Mode1) => acc += 15.0,
        _ => {}
    }
    acc
}

/// Which `*_calibration_target_area` value applies. W1 §12.6 (CONFIRMED four categories;
/// the caller picks: naval when the weapon class is 2, land mortar when it is 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalibrationCategory {
    /// Default (small arms).
    Default,
    /// Artillery.
    Artillery,
    /// Naval (weapon class 2).
    Naval,
    /// Land mortar (weapon class 3).
    LandMortar,
}

/// The four calibration target areas (float kv_rules keys, game data).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CalibrationAreas {
    /// Default area.
    pub default: f32,
    /// Artillery area.
    pub artillery: f32,
    /// Naval area.
    pub naval: f32,
    /// Land mortar area.
    pub land_mortar: f32,
}

impl CalibrationAreas {
    /// The area for a category.
    pub fn get(&self, c: CalibrationCategory) -> f32 {
        match c {
            CalibrationCategory::Default => self.default,
            CalibrationCategory::Artillery => self.artillery,
            CalibrationCategory::Naval => self.naval,
            CalibrationCategory::LandMortar => self.land_mortar,
        }
    }
}

/// Result of [`dispersion`]: the values passed on to `0x006A5BD0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dispersion {
    /// Horizontal spread.
    pub spread_x: f32,
    /// Vertical spread.
    pub spread_y: f32,
    /// `w = sqrt(A / D)`.
    pub w: f32,
}

/// Aspect for an owner type (`+0x4C`): 0.70710677 for types 0 and 0xB, otherwise 1.0.
/// W1 §12.6 (CONFIRMED). What the owner types mean is UNKNOWN.
pub fn aspect_for_owner_type(owner_type: u32) -> f32 {
    if owner_type == 0 || owner_type == 0xB {
        NARROW_ASPECT
    } else {
        1.0
    }
}

/// Shot dispersion, `0x006A5CE0`. W1 §12.6 (CONFIRMED), evaluated left to right in `f32`:
/// ```text
/// w = sqrt(A / D)
/// spread_x = 2 * (0.5 / aspect) * w / R
/// spread_y = 2 * w * aspect * 0.5 / R
/// ```
/// - `area` A: the calibration target area for the projectile category.
/// - `calibration_distance` D: the weapon calibration distance (`FUN_00DAB9D0`).
/// - `target_distance` R: distance to the target (`+0x50`).
pub fn dispersion(
    area: f32,
    owner_type: u32,
    calibration_distance: f32,
    target_distance: f32,
) -> Dispersion {
    let aspect = aspect_for_owner_type(owner_type);
    let w = (area / calibration_distance).sqrt();
    let spread_x = 2.0 * (0.5 / aspect) * w / target_distance;
    let spread_y = 2.0 * w * aspect * 0.5 / target_distance;
    Dispersion {
        spread_x,
        spread_y,
        w,
    }
}

// ---------------------------------------------------------------------------------------------
// W1 §12.10: chance to hit (0x00DAB9D0) and projectile impact (0x00DAADF0)
// ---------------------------------------------------------------------------------------------

/// Inputs to [`chance_to_hit`]: shooter info A and shot info P (W1 §12.10 offsets).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ChanceToHitInputs {
    /// `A+0x10`: core marksmanship.
    pub core_marksmanship: f32,
    /// `P+0x4`: marksmanship bonus.
    pub marksmanship_bonus: f32,
    /// `A+0x28`: control.
    pub control: f32,
    /// `P+0xC`: visibility.
    pub visibility: f32,
    /// `P+0x10`: angle judgement.
    pub angle_judgement: f32,
    /// `A+0x4`: shooter is land artillery.
    pub is_land_artillery: bool,
    /// `A+0x8`: shooter is naval.
    pub is_naval: bool,
    /// `P.f0`: distance to the target.
    pub distance: f32,
    /// `P+0x14`: target in cover.
    pub target_in_cover: bool,
}

/// Chance to hit, 0x00DAB9D0. W1 §12.10 (CONFIRMED):
/// ```text
/// marks = max(0, core + bonus) ; marks *= control * visibility * angle_judgement
/// Dh = half-chance distance (the artillery, then naval, variants take priority)
/// cth = (marks / dist^2) * (Dh*Dh*0.01) ; if in cover: cth -= 0.2 ; clamp(cth, 0.01, 1.0)
/// ```
/// The result feeds the aim-dispersion ellipse ([`dispersion`]); whether a man is struck is then
/// decided by the simulated projectile path (not modelled yet) and [`impact`].
/// INFERRED: `Dh` (an int key) is squared in `f32`.
pub fn chance_to_hit(i: &ChanceToHitInputs, r: &KvRules) -> f32 {
    let mut marks = (i.core_marksmanship + i.marksmanship_bonus).max(0.0);
    marks *= i.control * i.visibility * i.angle_judgement;
    let dh = if i.is_land_artillery {
        r.missile_distance_for_half_chance_hit_artillery
    } else if i.is_naval {
        r.missile_distance_for_half_chance_hit_naval
    } else {
        r.missile_distance_for_half_chance_hit
    } as f32;
    let mut cth = (marks / (i.distance * i.distance)) * (dh * dh * 0.01);
    if i.target_in_cover {
        cth -= 0.2;
    }
    // Written without f32::clamp so a NaN (distance 0 and marks 0) becomes the minimum.
    if cth.is_nan() || cth < 0.01 {
        0.01
    } else if cth > 1.0 {
        1.0
    } else {
        cth
    }
}

// ---------------------------------------------------------------------------------------------
// The shot factors of the chance to hit (round 12, BATTLE_FIDELITY.md §47).
// `0x006A5CE0` builds two adapters on the stack: the shooter adapter (vtable `0x0133A590`, ctor
// `0x00691930`: soldier, army level flag, army level) and the shot adapter (vtable `0x0133A5CC`,
// ctor `0x006919A0`: the shot, the same flag and level). `0x00DAB9D0` calls their slots.
// ---------------------------------------------------------------------------------------------

/// A projectile's trajectory class (`projectiles` col 10; runtime `+0x5C`). The solver
/// `0x006A23D0` branches on 2 (fixed angle) and 3 (rocket); 1 takes the high root and the rest the
/// low root (CONFIRMED). The name → value order low 0, high 1, fixed 2, rocket 3 is INFERRED from the
/// data (mortars and howitzer shells are "fixed" and use their maximum elevation as the angle).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Trajectory {
    /// The flat ballistic root (muskets, cannon).
    #[default]
    Low,
    /// The steep ballistic root.
    High,
    /// Fired at the maximum elevation (mortars, howitzer shells).
    Fixed,
    /// Rockets: aimed along the line of sight.
    Rocket,
}

impl Trajectory {
    /// The value of a `projectiles.trajectory_class` name (`None` for an unknown name).
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            "fixed" => Some(Self::Fixed),
            "rocket" => Some(Self::Rocket),
            _ => None,
        }
    }
}

/// The ballistic data of a projectile that the shot factors read. Runtime offsets are the builder
/// offsets − 0x1C (`+0x60` range ← col 11 @0x7C, `+0x68` ← col 13 @0x84, `+0x6C` ← col 14 @0x88,
/// `+0x70` ← col 15 @0x8C; INFERRED from the four matching shifts).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ballistics {
    /// `+0x5C`: trajectory class (col 10).
    pub trajectory: Trajectory,
    /// `+0x6C`: muzzle velocity in m/s (col 14).
    pub muzzle_velocity: f32,
    /// `+0x68`: maximum elevation in degrees (col 13).
    pub max_elevation_deg: i32,
    /// `+0x70`: the projectile's marksmanship bonus (col 15, "accuracy modifier": howitzer shells
    /// −50, rifled naval shot +50, muskets 0).
    pub accuracy_modifier: f32,
}

/// The shot's launch elevation in radians (shot `+0x4C`, written by the solver `0x006A23D0`),
/// positive upwards; `None` when the target cannot be reached (the shot is not fired).
/// - low / high: `tan θ = (v² ∓ √(v⁴ − g(g·d² + 2·y·v²))) / (g·d)`, `g = 9.8` (CONFIRMED constants
///   96.04 and 19.6); the angle must not exceed the maximum elevation;
/// - fixed: the maximum elevation;
/// - rocket: `atan(y / d)` (INFERRED: the CRT call `0x01285023` takes the ratio).
///
/// `horizontal` is the horizontal distance and `rise` the target's height above the muzzle.
/// APPROXIMATION: the solver first shortens a target vector longer than the weapon range to the
/// range; callers only shoot within range.
pub fn launch_angle(b: &Ballistics, horizontal: f32, rise: f32) -> Option<f32> {
    let max = b.max_elevation_deg as f32 * 0.017_453_292;
    let angle = match b.trajectory {
        Trajectory::Fixed => max,
        Trajectory::Rocket => {
            if horizontal <= 0.0 {
                return None;
            }
            (rise / horizontal).atan()
        }
        Trajectory::Low | Trajectory::High => {
            let v2 = b.muzzle_velocity * b.muzzle_velocity;
            let disc = v2 * v2 - horizontal * horizontal * 96.04 - v2 * 19.6 * rise;
            if disc < 0.0 || horizontal <= 0.0 {
                return None;
            }
            let root = disc.sqrt();
            let num = if b.trajectory == Trajectory::High { v2 + root } else { v2 - root };
            (num / (horizontal * 9.8)).atan()
        }
    };
    (angle <= max).then_some(angle)
}

/// "Angle judgement" (shot slot `+0x10`, `0x006A3410`, CONFIRMED): the launch elevation in degrees,
/// clamped to ±90, gives `1.5 − (deg + 90) / 180`: 1.0 for a level shot, more for shooting down,
/// less for shooting up (a mortar at 45° gets 0.75).
pub fn angle_judgement(launch_angle_rad: f32) -> f32 {
    let deg = launch_angle_rad * 57.295_776;
    // Written so a NaN angle becomes −90, as in the original compare order.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let deg = if !(deg >= -90.0) {
        -90.0
    } else if deg >= 90.0 {
        90.0
    } else {
        deg
    };
    1.5 - (deg + 90.0) * 0.005_555_555_7
}

/// "Visibility" (shot slot `+0xC`, `0x00700EB0`, CONFIRMED): `1 − 0.1 × intensity` of the battle's
/// current weather. The weather comes from the battle's environment object (battle `+0x28 → +8 →
/// +0xB0 → +0x198`, current record `0x005B9510`) and its `+0xC` is `battle_weather_types` col 1
/// (builder offset 0xC, CONFIRMED by the reader): dry 0, light 1, heavy 2, torrential 3. Its `+0x10`
/// is col 2, the kind (0 rain, 1 snow, 2 dust, 3 none), which the rain/snow tests `0x005DBB30` /
/// `0x005DBD60` read.
pub fn visibility(weather_intensity: u32) -> f32 {
    1.0 - weather_intensity as f32 * 0.1
}

/// The shooter's situation that sets its "control" (shooter slot `+0x28`, `0x006B8AA0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlInputs {
    /// The base: `+0x18` of the shooter unit's fatigue-effects record for its fatigue level
    /// (`0x00649520`: unit record `+0xE4 + 4 × unit+0xC70`), 1.0 without one (CONFIRMED): the
    /// `fatigue_effects` "control" column (col 4) as `1 + value` (`fatigue::FatigueEffects`).
    pub fatigue_base: f32,
    /// Unit `+0xC28`, the morale state: shaken (4) −0.1, wavering (5) −0.2 (CONFIRMED).
    pub morale_state: super::morale::MoraleState,
    /// Unit `+0xC84 != 0`, under small-arms fire: −0.1 (CONFIRMED).
    pub under_projectile_fire: bool,
    /// Unit `+0xC80 != 0`, under artillery fire: −0.1 (CONFIRMED).
    pub under_artillery_fire: bool,
    /// The unit's formation is engaged in melee (`0x0055B200`, the predicate behind the script
    /// function `BattlePlayerUnitEngagedInMelee`, CONFIRMED by its binding at `0x004072B0`): −0.2.
    pub in_melee: bool,
}

/// "Control" (shooter slot `+0x28`, `0x006B8AA0`, CONFIRMED order of the terms).
pub fn control(i: &ControlInputs) -> f32 {
    use super::morale::MoraleState;
    let mut c = i.fatigue_base;
    match i.morale_state {
        MoraleState::Shaken => c -= 0.1,
        MoraleState::Wavering => c -= 0.2,
        _ => {}
    }
    if i.under_projectile_fire {
        c -= 0.1;
    }
    if i.under_artillery_fire {
        c -= 0.1;
    }
    if i.in_melee {
        c -= 0.2;
    }
    c
}

/// The army level handicap on core marksmanship (shooter slot `+0x10`, `0x006B8D50`, CONFIRMED).
/// `flag` and `level` are the army's `+0x224` / `+0x234` (`0x006AD6E0`, the same pair as the reload
/// and melee bonuses; INFERRED a difficulty handicap). Core marksmanship is the unit's accuracy
/// (unit `+0x160`), 50 for a soldier with no unit.
pub fn level_core_marksmanship(core: f32, flag: bool, level: i32) -> f32 {
    match (flag, level) {
        (false, 1) => core * 1.3,
        (false, 2) => core * 1.5,
        (false, -1) => core * 0.85,
        (true, 1) => core * 1.25,
        (true, -1) => core * 0.9,
        (true, -2) => core * 0.75,
        _ => core,
    }
}

/// The marksmanship bonus (shot slot `+0x4`, `0x006D8B00`, CONFIRMED): the projectile's accuracy
/// modifier, plus (UNKNOWN kv slot, not modelled) a bonus on walls, plus the army level bonus: flag
/// clear: level 1 → +20, 2 → +30; flag set: level 1 → +15.
pub fn marksmanship_bonus(accuracy_modifier: f32, flag: bool, level: i32) -> f32 {
    accuracy_modifier
        + match (flag, level) {
            (false, 1) => 20.0,
            (false, 2) => 30.0,
            (true, 1) => 15.0,
            _ => 0.0,
        }
}

/// Inputs to [`impact`] (W1 §12.10).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ImpactInputs {
    /// `P.damage` (float).
    pub damage: f32,
    /// `P.horizontal_distance`.
    pub horizontal_distance: f32,
    /// `P.effective_range`.
    pub effective_range: f32,
    /// The attacker's accuracy, or `None` when the shot has no attacker.
    pub attacker_accuracy: Option<f32>,
    /// Defender shield.
    pub defender_shield: i32,
    /// Defender armour.
    pub defender_armour: i32,
    /// Defender defence.
    pub defender_defence: i32,
}

/// Result of [`impact`], with the original's codes. W1 §12.10 (CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ImpactOutcome {
    /// 0.
    Miss = 0,
    /// 3.
    Knockdown = 3,
    /// 5.
    Kill = 5,
}

/// Projectile kill chance before the roll (clamped to 14..=94), or the immediate result when the
/// damage is `<= 0` or `>= 1`. W1 §12.10 (CONFIRMED).
pub fn impact_kill_chance(i: &ImpactInputs, r: &KvRules) -> Result<i32, ImpactOutcome> {
    let dmg = i.damage;
    if dmg <= 0.0 {
        return Err(ImpactOutcome::Miss);
    }
    if dmg >= 1.0 {
        return Err(if i.horizontal_distance > i.effective_range {
            ImpactOutcome::Miss
        } else {
            ImpactOutcome::Kill
        });
    }
    let mut kc = match i.attacker_accuracy {
        Some(acc) => x87_round(60.0 * dmg).wrapping_add(x87_round(acc * 0.666_666_7)),
        None => x87_round(100.0 * dmg),
    };
    kc = kc.wrapping_sub(c_div(i.defender_shield, r.projectile_damage_shield_divisor));
    kc = kc.wrapping_sub(c_div(i.defender_armour, r.projectile_damage_armour_divisor));
    kc = kc.wrapping_sub(c_div(
        i.defender_defence,
        r.projectile_damage_defense_divisor,
    ));
    kc = kc.wrapping_sub(x87_round(
        r.projectile_damage_distance_multiplier * i.horizontal_distance / i.effective_range,
    ));
    // The code uses 0x5E = 94 (the log string says 95, which is stale).
    Ok(kc.clamp(14, 94))
}

/// The 0..=100 roll: `min(100, (next16()*101)/0xFFFF)`. W1 §12.10 (CONFIRMED).
pub fn roll_100(rng: &mut CaRng) -> i32 {
    ((rng.next16() * 101) / 0xFFFF).min(100) as i32
}

/// Maps `d = kc - roll` to an outcome: `d >= 7` Kill, `1..=6` Knockdown, else Miss. W1 §12.10.
pub fn impact_outcome(kc: i32, roll: i32) -> ImpactOutcome {
    let d = kc - roll;
    if d >= 7 {
        ImpactOutcome::Kill
    } else if d >= 1 {
        ImpactOutcome::Knockdown
    } else {
        ImpactOutcome::Miss
    }
}

/// Projectile impact, 0x00DAADF0. W1 §12.10 (CONFIRMED). Consumes **no** roll when the damage is
/// `<= 0` or `>= 1`; otherwise exactly one. (`missile_xholds_*` are not used here.)
pub fn impact(rng: &mut CaRng, i: &ImpactInputs, r: &KvRules) -> ImpactOutcome {
    match impact_kill_chance(i, r) {
        Ok(kc) => impact_outcome(kc, roll_100(rng)),
        Err(done) => done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OBVIOUSLY MADE-UP placeholder rules (NOT game data).
    fn rules() -> KvRules {
        KvRules {
            missile_distance_for_half_chance_hit: 100,
            missile_distance_for_half_chance_hit_artillery: 200,
            missile_distance_for_half_chance_hit_naval: 300,
            projectile_damage_shield_divisor: 2,
            projectile_damage_armour_divisor: 3,
            projectile_damage_defense_divisor: 4,
            projectile_damage_distance_multiplier: 10.0,
            ..KvRules::default()
        }
    }

    #[test]
    fn chance_to_hit_formula() {
        let r = rules();
        let i = ChanceToHitInputs {
            core_marksmanship: 30.0,
            marksmanship_bonus: 10.0,
            control: 1.0,
            visibility: 0.5,
            angle_judgement: 1.0,
            distance: 100.0,
            ..Default::default()
        };
        // marks = 40*0.5 = 20; cth = 20/10000 * (100*100*0.01 = 100) = 0.2
        assert!((chance_to_hit(&i, &r) - 0.2).abs() < 1e-6);
        // Artillery Dh = 200 -> 0.8; naval Dh = 300 -> 1.8 -> clamped to 1.0.
        let art = ChanceToHitInputs {
            is_land_artillery: true,
            ..i
        };
        assert!((chance_to_hit(&art, &r) - 0.8).abs() < 1e-5);
        assert_eq!(
            chance_to_hit(
                &ChanceToHitInputs {
                    is_naval: true,
                    ..i
                },
                &r
            ),
            1.0
        );
        // Cover: 0.2 - 0.2 -> clamp 0.01. Negative marksmanship -> max(0) -> 0.01.
        assert_eq!(
            chance_to_hit(
                &ChanceToHitInputs {
                    target_in_cover: true,
                    ..i
                },
                &r
            ),
            0.01
        );
        let bad = ChanceToHitInputs {
            core_marksmanship: -50.0,
            ..i
        };
        assert_eq!(chance_to_hit(&bad, &r), 0.01);
    }

    fn imp() -> ImpactInputs {
        ImpactInputs {
            damage: 0.5,
            horizontal_distance: 50.0,
            effective_range: 100.0,
            attacker_accuracy: Some(30.0),
            defender_shield: 5,
            defender_armour: 7,
            defender_defence: 9,
        }
    }

    #[test]
    fn impact_kill_chance_terms() {
        let r = rules();
        // 30 + 20 - 5/2 - 7/3 - 9/4 - round(10*50/100) = 50 - 2 - 2 - 2 - 5 = 39
        assert_eq!(impact_kill_chance(&imp(), &r), Ok(39));
        // No attacker: round(100*0.5) = 50 -> same total 39.
        let none = ImpactInputs {
            attacker_accuracy: None,
            ..imp()
        };
        assert_eq!(impact_kill_chance(&none, &r), Ok(39));
        // Clamps 14..94.
        let low = ImpactInputs {
            damage: 0.01,
            attacker_accuracy: None,
            ..imp()
        };
        assert_eq!(impact_kill_chance(&low, &r), Ok(14));
        let high = ImpactInputs {
            damage: 0.99,
            attacker_accuracy: Some(200.0),
            ..imp()
        };
        assert_eq!(impact_kill_chance(&high, &r), Ok(94));
        // Immediate results.
        let zero = ImpactInputs {
            damage: 0.0,
            ..imp()
        };
        assert_eq!(impact_kill_chance(&zero, &r), Err(ImpactOutcome::Miss));
        let full = ImpactInputs {
            damage: 1.0,
            ..imp()
        };
        assert_eq!(impact_kill_chance(&full, &r), Err(ImpactOutcome::Kill));
        let far = ImpactInputs {
            damage: 1.0,
            horizontal_distance: 101.0,
            ..imp()
        };
        assert_eq!(impact_kill_chance(&far, &r), Err(ImpactOutcome::Miss));
    }

    #[test]
    fn impact_roll_and_bands() {
        let r = rules();
        // Seed 0: first next16 = 38 -> roll = 38*101/65535 = 0; kc 39 - 0 >= 7 -> Kill.
        assert_eq!(roll_100(&mut CaRng::new(0)), 0);
        assert_eq!(impact(&mut CaRng::new(0), &imp(), &r), ImpactOutcome::Kill);
        // No roll is consumed for damage >= 1.
        let mut rng = CaRng::new(5);
        impact(
            &mut rng,
            &ImpactInputs {
                damage: 1.0,
                ..imp()
            },
            &r,
        );
        assert_eq!(rng, CaRng::new(5));
        // Band edges.
        assert_eq!(impact_outcome(14, 7), ImpactOutcome::Kill); // d = 7
        assert_eq!(impact_outcome(14, 8), ImpactOutcome::Knockdown); // d = 6
        assert_eq!(impact_outcome(14, 13), ImpactOutcome::Knockdown); // d = 1
        assert_eq!(impact_outcome(14, 14), ImpactOutcome::Miss); // d = 0
        let mut rng = CaRng::new(9);
        for _ in 0..1000 {
            assert!((0..=100).contains(&roll_100(&mut rng)));
        }
    }

    #[test]
    fn range_rules() {
        // Made-up weapon range 100 and walls modifier 25 (not game data).
        assert_eq!(range(false, Some(100), false, false, 25), 0.0);
        assert_eq!(range(true, None, false, false, 25), 0.0);
        assert_eq!(range(true, Some(100), false, false, 25), 100.0);
        assert_eq!(range(true, Some(100), false, true, 25), 80.0);
        // Walls take priority over the ×0.8 state.
        assert_eq!(range(true, Some(100), true, true, 25), 125.0);
        assert_eq!(range(true, Some(101), false, true, 0), 101.0 * 0.8);
    }

    #[test]
    fn accuracy_rules() {
        // Made-up base accuracy 40 and walls modifier 5.
        assert_eq!(accuracy(40.0, false, 5, AccuracyMode::Other, false), 40.0);
        assert_eq!(accuracy(40.0, true, 5, AccuracyMode::Other, false), 45.0);
        assert_eq!(accuracy(40.0, false, 5, AccuracyMode::Mode1, false), 60.0);
        assert_eq!(accuracy(40.0, false, 5, AccuracyMode::Mode2, false), 70.0);
        assert_eq!(accuracy(40.0, false, 5, AccuracyMode::Mode1, true), 55.0);
        assert_eq!(accuracy(40.0, false, 5, AccuracyMode::Mode2, true), 40.0);
        assert_eq!(accuracy(40.0, true, 5, AccuracyMode::Mode2, false), 75.0);
    }

    #[test]
    fn dispersion_formula() {
        // A = 4, D = 1 -> w = 2; R = 2.
        let d = dispersion(4.0, 5, 1.0, 2.0);
        assert_eq!(d.w, 2.0);
        assert_eq!(d.spread_x, 1.0); // 2 * 0.5 * 2 / 2
        assert_eq!(d.spread_y, 1.0);
        // Narrow aspect: x grows by sqrt 2, y shrinks by sqrt 2.
        let n = dispersion(4.0, 0xB, 1.0, 2.0);
        assert!((n.spread_x - std::f32::consts::SQRT_2).abs() < 1e-6);
        assert!((n.spread_y - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert_eq!(aspect_for_owner_type(0), NARROW_ASPECT);
        assert_eq!(aspect_for_owner_type(1), 1.0);
    }

    #[test]
    fn calibration_lookup() {
        let a = CalibrationAreas {
            default: 1.0,
            artillery: 2.0,
            naval: 3.0,
            land_mortar: 4.0,
        };
        assert_eq!(a.get(CalibrationCategory::Naval), 3.0);
        assert_eq!(a.get(CalibrationCategory::LandMortar), 4.0);
    }

    #[test]
    fn shot_factors() {
        use super::super::morale::MoraleState;
        // Angle judgement: level 1.0, mortar 45° 0.75, straight down 1.5, NaN like −90.
        assert!((angle_judgement(0.0) - 1.0).abs() < 1e-6);
        assert!((angle_judgement(45f32.to_radians()) - 0.75).abs() < 1e-5);
        assert!((angle_judgement(-3.0) - 1.5).abs() < 1e-6);
        assert!((angle_judgement(f32::NAN) - 1.5).abs() < 1e-6);
        assert!((visibility(0) - 1.0).abs() < 1e-6 && (visibility(3) - 0.7).abs() < 1e-6);
        // A musket (v 150) at 80 m on the level aims about 1° up.
        let musket = Ballistics { trajectory: Trajectory::Low, muzzle_velocity: 150.0, max_elevation_deg: 88, accuracy_modifier: 0.0 };
        let a = launch_angle(&musket, 80.0, 0.0).unwrap().to_degrees();
        assert!(a > 0.9 && a < 1.1, "{a}");
        assert!(launch_angle(&musket, 80.0, -20.0).unwrap() < 0.0);
        assert!(launch_angle(&musket, 5000.0, 0.0).is_none());
        let mortar = Ballistics { trajectory: Trajectory::Fixed, muzzle_velocity: 86.0, max_elevation_deg: 45, accuracy_modifier: 0.0 };
        assert!((launch_angle(&mortar, 300.0, 0.0).unwrap().to_degrees() - 45.0).abs() < 1e-3);
        let base = ControlInputs { fatigue_base: 1.0, morale_state: MoraleState::Steady, under_projectile_fire: false, under_artillery_fire: false, in_melee: false };
        assert_eq!(control(&base), 1.0);
        let worst = ControlInputs { morale_state: MoraleState::Wavering, under_projectile_fire: true, under_artillery_fire: true, in_melee: true, ..base };
        assert!((control(&worst) - 0.4).abs() < 1e-6);
        assert_eq!(level_core_marksmanship(50.0, false, 0), 50.0);
        assert_eq!(level_core_marksmanship(50.0, false, 2), 75.0);
        assert_eq!(level_core_marksmanship(40.0, true, -2), 30.0);
        assert_eq!(marksmanship_bonus(-50.0, false, 1), -30.0);
        assert_eq!(marksmanship_bonus(0.0, true, 2), 0.0);
    }
}

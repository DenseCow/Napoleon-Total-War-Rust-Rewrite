//! How the battle AI values units: army strength balance, target values and the melee
//! "potential" of an attacker against a target.
//!
//! Sources (`analysis/ai/AI_RESEARCH.md` §3, `analysis/fidelity/BATTLE_FIDELITY.md` §6):
//! - CONFIRMED: the unit ratings the analysers read are the strength terms themselves. The army
//!   update `0x00539E80` stores `0x006AFB70` (melee) at unit `+0xBE8` and `0x006B05B0` (missile) at
//!   `+0xBEC` for every unit, and sums both into the army's strength (`+0x64`). Both functions are
//!   modelled in `ntw_sim::battle::strength`; [`melee_rating`] and [`missile_rating`] call them.
//! - CONFIRMED structure of the strength balance `0x006A31E0`: sum each alliance's unit strengths
//!   (melee term + missile term), `own / (own + enemy)`, 1 when the enemy has nothing, 0 when we
//!   have nothing. The terms are 0 only for shattered units (state 7 / behaviour 3); routing units
//!   still count.
//! - CONFIRMED: the melee-attack analyser's aggression is `b / (1.01 - b)` with `b` the balance.
//! - CONFIRMED: a target's value `0x007D3240` is `0.3 * missile_rating + 0.7 * melee_rating`
//!   (unit `+0xBEC` / `+0xBE8`).
//! - CONFIRMED: the missile base priority `0x00755C50` is `men * 0.01 * value`, x0.25 when the
//!   target is engaged in melee (`0x0054EE90`, INFERRED meaning).
//! - CONFIRMED shape of the melee potential `0x006AFC00` (see [`melee_potential`]).

use ntw_sim::battle::model::{Battle, LandUnit, MELEE_CONTACT_RANGE};
use ntw_sim::battle::morale::MoraleState;
use ntw_sim::battle::strength;

use super::geom::{dist, wrap};

/// The unit's melee rating, unit `+0xBE8` = `0x006AFB70` (CONFIRMED; see
/// `ntw_sim::battle::strength::melee_strength` for the formula and what the model leaves out).
pub fn melee_rating(u: &LandUnit) -> f32 {
    strength::melee_strength(u)
}

/// The unit's missile rating, unit `+0xBEC` = `0x006B05B0` (CONFIRMED; see
/// `ntw_sim::battle::strength::missile_strength`).
pub fn missile_rating(u: &LandUnit) -> f32 {
    strength::missile_strength(u)
}

/// Strength of one unit for the balance: melee term + missile term (CONFIRMED, `0x006A31E0`).
/// Our own extra rule: a unit that is not active (not on the field) counts nothing.
pub fn unit_strength(u: &LandUnit) -> f32 {
    if !u.active {
        return 0.0;
    }
    strength::strength(u)
}

/// `0x006A31E0` (CONFIRMED structure): `own / (own + enemy)` strength of `side`.
pub fn balance(battle: &Battle, side: u8) -> f32 {
    let (mut own, mut enemy) = (0.0f32, 0.0f32);
    for u in &battle.units {
        let s = unit_strength(u);
        if u.side == side {
            own += s;
        } else {
            enemy += s;
        }
    }
    if own == 0.0 {
        0.0
    } else if enemy == 0.0 {
        1.0
    } else {
        own / (own + enemy)
    }
}

/// CONFIRMED (melee-attack analyser, `0x00749E90`): `b / (1.01 - b)`.
pub fn aggression(balance: f32) -> f32 {
    balance / (1.01 - balance)
}

/// `0x007D3240` (CONFIRMED weights): the value of a unit as a target.
pub fn target_value(u: &LandUnit) -> f32 {
    0.3 * missile_rating(u) + 0.7 * melee_rating(u)
}

/// True if `u` is in melee contact with an enemy (INFERRED meaning of `0x0054EE90`).
pub fn engaged(battle: &Battle, u: &LandUnit) -> bool {
    u.in_melee
        || battle.units.iter().any(|e| {
            e.side != u.side && e.men > 0 && e.active && dist(e.position, u.position) <= MELEE_CONTACT_RANGE
        })
}

/// `0x00755C50` (CONFIRMED shape): the missile base priority of a target.
pub fn missile_base_priority(battle: &Battle, target: &LandUnit) -> f32 {
    let p = target.men as f32 * 0.01 * target_value(target);
    if engaged(battle, target) { p * 0.25 } else { p }
}

/// `0x00755BF0` (CONFIRMED): the melee base priority of a target is its value.
pub fn melee_base_priority(target: &LandUnit) -> f32 {
    target_value(target)
}

/// Where `attacker` strikes `target` from, relative to the target's facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackSide {
    /// Within 45° of the target's front.
    Front,
    /// Left or right flank.
    Flank,
    /// Within 45° of the target's rear.
    Rear,
}

/// Classifies the attack direction. Same split as the model's PLACEHOLDER `melee_dir`
/// (the original's `0x006AD890` direction lookup is not decoded yet).
pub fn attack_side(target_pos: (f32, f32), target_facing: f32, attacker_pos: (f32, f32)) -> AttackSide {
    use std::f32::consts::FRAC_PI_4;
    let angle = (attacker_pos.1 - target_pos.1).atan2(attacker_pos.0 - target_pos.0);
    let rel = wrap(angle - target_facing).abs();
    if rel <= FRAC_PI_4 {
        AttackSide::Front
    } else if rel >= 3.0 * FRAC_PI_4 {
        AttackSide::Rear
    } else {
        AttackSide::Flank
    }
}

/// PROVISIONAL per-man bonus for the attack direction in [`melee_potential`], as a fraction of the
/// attacker's melee rating per man. The original looks the direction up in a table held by the
/// analyser (`0x00DAC7A0`) and multiplies it by the attacker's men; the table values are UNKNOWN.
pub fn direction_factor(side: AttackSide) -> f32 {
    match side {
        AttackSide::Front => 0.0,
        AttackSide::Flank => 0.5,
        AttackSide::Rear => 1.0,
    }
}

/// `0x006AFC00` (CONFIRMED shape): how well `attacker` would do in melee against `target`, 0..1.
///
/// `num = A + attacker_men * dir_factor - (target_men - attacker_men) * height_term`, then
/// `clamp(max(num, 0.01) / max(T, 0.01), 0, 2) * 0.5`, with `A`/`T` the two melee ratings.
/// The height term is 0 here (flat battlefield model; the original reads two terrain values).
pub fn melee_potential(attacker: &LandUnit, target: &LandUnit) -> f32 {
    let a = melee_rating(attacker);
    let t = melee_rating(target).max(0.01);
    let side = attack_side(target.position, target.facing, attacker.position);
    let per_man = a / attacker.men.max(1) as f32;
    let num = a + attacker.men as f32 * per_man * direction_factor(side);
    ((num.max(0.01) / t).clamp(0.0, 2.0)) * 0.5
}

/// The missile potential of `attacker` against `target` (`0x006B06F0`, CONFIRMED shape):
/// `clamp((A + men × dir_table[dir] + height term) / T, 0, 2) × 0.5` with `A`, `T` the missile
/// ratings (`+0xBEC`, `T` at least 0.01). The direction table and height term are UNKNOWN /
/// INFERRED and left out here (PROVISIONAL), so this is `clamp(A / T, 0, 2) × 0.5`.
pub fn missile_potential(attacker: &LandUnit, target: &LandUnit) -> f32 {
    let a = missile_rating(attacker).max(0.01);
    let t = missile_rating(target).max(0.01);
    (a / t).clamp(0.0, 2.0) * 0.5
}

/// The sub-objective priority shaping `0x006B0A00(x, m, low)` (CONFIRMED): `x` is clamped to
/// `0..2m`; with `d = x − m`, the value is `1 − d²/m²` below the peak and `1 − (1 − low)·d²/m²`
/// above it. The melee priority uses `m = 1.7, low = 0.3`, the missile priority `m = 1, low = 0.3`.
pub fn priority_shape(x: f32, m: f32, low: f32) -> f32 {
    let x = if x >= 0.0 { x.min(m + m) } else { 0.0 };
    let d = x - m;
    let k = 1.0 / (m * m);
    if d < 0.0 { 1.0 - k * d * d } else { 1.0 - (1.0 - low) * d * d * k }
}

/// The turn a unit needs to face a point: the absolute angle (radians, 0..π) between its facing
/// and the bearing to the point (the original works on a 16-bit circle).
pub fn turn_angle(from: (f32, f32), facing: f32, to: (f32, f32)) -> f32 {
    let bearing = (to.1 - from.1).atan2(to.0 - from.0);
    wrap(bearing - facing).abs()
}

/// The melee distance/turn factor `0x0079C0D0` (CONFIRMED): `1 / (penalty + 1 + d/15)`, the
/// penalty (only beyond 5 m) by the turn: up to 10° 0, 22.5° 0.5, 45° 2, 90° 3.5, more 5. Units in
/// unit state 1 (`+0xB34`, set by OUTFLANK) double the penalty and ignore targets more than 105 m
/// away in front of them (`1 + d/15 > 8`) unless an UNKNOWN position test `0x0064F230` passes.
pub fn melee_distance_factor(d: f32, turn: f32, state1: bool) -> f32 {
    let near = d * (1.0 / 15.0) + 1.0;
    let turn = turn.abs();
    let mut pen = 0.0;
    if d > 5.0 {
        pen = if turn > std::f32::consts::FRAC_PI_2 {
            5.0
        } else if turn > std::f32::consts::FRAC_PI_4 {
            3.5
        } else if turn > std::f32::consts::FRAC_PI_8 {
            2.0
        } else if turn > 10f32.to_radians() {
            0.5
        } else {
            0.0
        };
    }
    if state1 {
        pen += pen;
        if near > 8.0 && turn < std::f32::consts::FRAC_PI_2 {
            return 0.0;
        }
    }
    1.0 / (pen + near)
}

/// The missile distance/turn factor `0x0079C7E0` (CONFIRMED): `1 / (0.5·v + 1)` with `v = d/10`
/// plus, beyond 5 m, 1 (turn over 22.5°), 2.5 (over 45°) or 4 (over 90°). `d` is the distance
/// less an attacker extent `+0x670` (UNKNOWN; we use the plain distance).
pub fn missile_distance_factor(d: f32, turn: f32) -> f32 {
    let mut v = d * 0.1;
    let turn = turn.abs();
    if d > 5.0 {
        v += if turn > std::f32::consts::FRAC_PI_2 {
            4.0
        } else if turn > std::f32::consts::FRAC_PI_4 {
            2.5
        } else if turn > std::f32::consts::FRAC_PI_8 {
            1.0
        } else {
            0.0
        };
    }
    1.0 / (v * 0.5 + 1.0)
}

/// The minimum-priority test, objective virtual `+0x18` (`0x0079C730`, CONFIRMED): the priority
/// divided by the square of the sub-objective's attacker rating (`+0x40`: the melee rating for
/// melee, the missile rating for missile objectives, at least 0.0001) must reach a threshold set by
/// the unit's AI state `+0xB34`: state 1 → 0.005, 2 → 0.3, 3 → 0.0005; any other state passes.
pub fn meets_minimum(priority: f32, attacker_rating: f32, unit_state: u32) -> bool {
    let r = if attacker_rating == 0.0 { 0.0001 } else { attacker_rating };
    let threshold = match unit_state {
        1 => 0.005,
        2 => 0.3,
        3 => 0.0005,
        _ => return true,
    };
    priority / (r * r) >= threshold
}

/// True if a unit's morale is shaken enough that the AI pulls it out of the line
/// (PROVISIONAL rule: wavering or worse, not yet routing).
pub fn should_fall_back(u: &LandUnit) -> bool {
    !u.morale.is_routing_or_shattered() && u.morale.state >= MoraleState::Wavering
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::fatigue::KvFatigue;
    use ntw_sim::battle::morale::{KvMorale, MoraleBehaviour};

    fn unit(id: u32, side: u8, men: u32, pos: (f32, f32), atk: i32) -> LandUnit {
        let mut u = LandUnit::new(id, side, men, pos);
        u.melee_attack = atk;
        u.melee_defence = 5;
        u
    }

    #[test]
    fn balance_and_aggression() {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.add_unit(unit(1, 0, 100, (0.0, 0.0), 5));
        b.add_unit(unit(2, 1, 100, (0.0, 50.0), 5));
        assert!((balance(&b, 0) - 0.5).abs() < 1e-6);
        // A routing unit still counts (0x006AFB70 only skips behaviour 3 / state 7).
        b.units[1].morale.behaviour = MoraleBehaviour::Routing;
        assert!((balance(&b, 0) - 0.5).abs() < 1e-6);
        b.units[1].morale.behaviour = MoraleBehaviour::Shattered;
        assert_eq!(balance(&b, 0), 1.0);
        assert_eq!(balance(&b, 1), 0.0);
        assert!((aggression(0.5) - 0.5 / 0.51).abs() < 1e-6);
    }

    #[test]
    fn rear_attack_beats_frontal() {
        // Target faces +x at the origin.
        let target = unit(2, 1, 100, (0.0, 0.0), 5);
        let front = unit(1, 0, 100, (20.0, 0.0), 5);
        let rear = unit(1, 0, 100, (-20.0, 0.0), 5);
        let pf = melee_potential(&front, &target);
        let pr = melee_potential(&rear, &target);
        assert!((pf - 0.5).abs() < 1e-6, "equal units head on = even");
        assert!(pr > pf);
        assert!(pr <= 1.0);
    }

    #[test]
    fn priority_shape_matches_the_exe() {
        // Peak 1 at m, 0 at 0, `low` at 2m and beyond (0x006B0A00).
        assert!((priority_shape(1.7, 1.7, 0.3) - 1.0).abs() < 1e-6);
        assert!(priority_shape(0.0, 1.7, 0.3).abs() < 1e-6);
        assert!((priority_shape(3.4, 1.7, 0.3) - 0.3).abs() < 1e-6);
        assert!((priority_shape(9.0, 1.7, 0.3) - 0.3).abs() < 1e-6);
        assert!(priority_shape(-1.0, 1.0, 0.3).abs() < 1e-6);
        assert!((priority_shape(0.5, 1.0, 0.3) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn distance_and_turn_factors() {
        use std::f32::consts::PI;
        // Melee: 1 / (penalty + 1 + d/15) (0x0079C0D0).
        assert!((melee_distance_factor(15.0, 0.0, false) - 0.5).abs() < 1e-6);
        assert!((melee_distance_factor(15.0, PI, false) - 1.0 / 7.0).abs() < 1e-6);
        assert!((melee_distance_factor(15.0, 0.3, false) - 1.0 / 2.5).abs() < 1e-6);
        assert!((melee_distance_factor(4.0, PI, false) - 1.0 / (1.0 + 4.0 / 15.0)).abs() < 1e-6, "no turn penalty within 5 m");
        assert!((melee_distance_factor(15.0, PI, true) - 1.0 / 12.0).abs() < 1e-6, "state 1 doubles it");
        assert_eq!(melee_distance_factor(120.0, 0.0, true), 0.0, "state 1 ignores far targets ahead");
        // Missile: 1 / (0.5 (d/10 + penalty) + 1) (0x0079C7E0).
        assert!((missile_distance_factor(20.0, 0.0) - 0.5).abs() < 1e-6);
        assert!((missile_distance_factor(20.0, PI) - 1.0 / 4.0).abs() < 1e-6);
    }

    #[test]
    fn minimum_priority_by_unit_state() {
        // priority / rating² against 0.005 (state 1), 0.3 (2), 0.0005 (3); others pass.
        assert!(meets_minimum(0.0, 10.0, 0));
        assert!(meets_minimum(0.5, 10.0, 1));
        assert!(!meets_minimum(0.49, 10.0, 1));
        assert!(!meets_minimum(29.0, 10.0, 2));
        assert!(meets_minimum(0.05, 10.0, 3));
        assert!(meets_minimum(0.0, 10.0, 4));
    }
}

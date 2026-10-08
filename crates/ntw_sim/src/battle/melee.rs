//! Real-time melee resolution: hit number, kill chance, blow outcome and pair selection.
//!
//! W1 §12.9 (resolves the old UNKNOWN W1 §10.2). The call chain in the original is
//! `0x00664E80 → 0x006AFE20` (pick the fighting pair) `→ 0x00DAA290` (resolve one blow), which calls
//! `0x00DAB5F0` (hit number) and `0x00DADA40` (kill chance). Every term below is CONFIRMED from the
//! disassembly unless marked otherwise; the *meanings* of many combatant flags are INFERRED.
//!
//! Arithmetic is `i32` everywhere (C integer division truncates toward zero, see
//! [`c_div`]) except the height term, which is computed in float and rounded with the x87 default
//! rounding (ties to even, see [`x87_round`]).

use super::rules::{KvRules, c_div, x87_round};
use crate::rng::{CaRng, INV_65535};

/// Encounter attack direction `E.dir`, relative to the defender. W1 §12.9 (CONFIRMED values 0..3
/// and which `factor_attackdir_*` key each selects; how the original computes it, 0x006AD890, is
/// still open).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MeleeDir {
    /// 0, `factor_attackdir_front`.
    Front = 0,
    /// 1, `factor_attackdir_flankleft`.
    FlankLeft = 1,
    /// 2, `factor_attackdir_flankright`.
    FlankRight = 2,
    /// 3, `factor_attackdir_rear`.
    Rear = 3,
}

/// One combatant as seen by the hit-number function (combatant info object, vtable 0x0133A52C).
/// The method offsets in the docs are W1 §12.9's table.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Combatant {
    /// `+0x08`: per-soldier fatigue level (INFERRED).
    pub fatigue: i32,
    /// `+0x18`: melee attack after the fatigue-effect multiplier and army-level bonus
    /// (see [`scaled_stat`], [`army_level_attack_bonus`]).
    pub attack: i32,
    /// `+0x1C`: charge bonus after the fatigue-effect multiplier.
    pub charge: i32,
    /// `+0x0C`: armour (INFERRED meaning).
    pub armour: i32,
    /// `+0x14`: shield (INFERRED meaning).
    pub shield: i32,
    /// `+0x10`: melee defence (INFERRED meaning).
    pub defence: i32,
    /// `+0x28`: bonus vs cavalry (INFERRED meaning).
    pub bonus_vs_cavalry: i32,
    /// `+0x20`: entrenchment. CONFIRMED to be a stub returning 0 for land combatants.
    pub entrenchment: i32,
    /// `+0x24`: environment term. CONFIRMED to be a stub returning 0 for land combatants.
    pub environment: i32,
    /// `+0x38`: anti-charge / spear-like (CONFIRMED logic, INFERRED meaning).
    pub anti_charge: bool,
    /// `+0x40`: charging (entity action == 0xD).
    pub charging: bool,
    /// `+0x44`: braced (entity+0x5C).
    pub braced: bool,
    /// `+0x48`: cavalry.
    pub cavalry: bool,
    /// `+0x4C`: infantry.
    pub infantry: bool,
    /// `+0x50`: the "category 1 variant" flag (meaning UNKNOWN), treated like infantry by cavalry.
    pub f50: bool,
    /// `+0x54`: in square.
    pub in_square: bool,
    /// `+0x58`: on walls.
    pub on_walls: bool,
    /// `+0x5C`: in a building.
    pub in_building: bool,
}

/// Inputs to [`hit_number`]: attacker A, defender D and the encounter E.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeInputs {
    /// Attacker A.
    pub attacker: Combatant,
    /// Defender D.
    pub defender: Combatant,
    /// `E.dir`.
    pub dir: MeleeDir,
    /// `E.height_delta`, see [`height_delta`].
    pub height_delta: f32,
}

/// Outcome of one blow, with the original's result codes. W1 §12.9 (CONFIRMED codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlowOutcome {
    /// 0.
    Miss = 0,
    /// 1.
    Stepback = 1,
    /// 2.
    Knockback = 2,
    /// 3.
    Knockdown = 3,
    /// 5.
    Kill = 5,
}

/// `E.height_delta`, 0x006CCAB0. W1 §12.9 (CONFIRMED): `D.z − A.z`, or, if either side is charging,
/// `(D.z − A.z) / horizontal_distance²` (squared, **not** square-rooted).
pub fn height_delta(
    attacker_z: f32,
    defender_z: f32,
    horizontal_distance_sq: f32,
    either_charging: bool,
) -> f32 {
    let dz = defender_z - attacker_z;
    if either_charging {
        dz / horizontal_distance_sq
    } else {
        dz
    }
}

/// Attack or charge after the fatigue-effect multiplier: `round(stat * mult)`. W1 §12.9
/// (0x006A8290 / 0x006B30E0, CONFIRMED). The multiplier comes from the unit record's
/// fatigue-effects row for the unit's fatigue state (record layout still open). Rounding mode:
/// INFERRED x87 default (ties to even).
pub fn scaled_stat(stat: i32, mult: f32) -> i32 {
    x87_round(stat as f32 * mult)
}

/// The per-army "level" attack bonus (0x006AD6E0). W1 §12.9 (CONFIRMED numbers; INFERRED to be a
/// difficulty handicap): flag set → +4 only at level 1; flag clear → +4 at level 1, +8 at level 2.
/// (The level drop after tick 6 for the stronger side is not modelled here.)
pub fn army_level_attack_bonus(flag_set: bool, level: u8) -> i32 {
    match (flag_set, level) {
        (_, 1) => 4,
        (false, 2) => 8,
        _ => 0,
    }
}

/// The attack-direction factor `R.factor_attackdir[E.dir]`.
fn dir_factor(r: &KvRules, dir: MeleeDir) -> i32 {
    match dir {
        MeleeDir::Front => r.factor_attackdir_front,
        MeleeDir::FlankLeft => r.factor_attackdir_flankleft,
        MeleeDir::FlankRight => r.factor_attackdir_flankright,
        MeleeDir::Rear => r.factor_attackdir_rear,
    }
}

/// Hit number `hn`, 0x00DAB5F0. W1 §12.9 (every term CONFIRMED), in the documented order.
pub fn hit_number(i: &MeleeInputs, r: &KvRules) -> i32 {
    let a = &i.attacker;
    let d = &i.defender;
    let front = i.dir == MeleeDir::Front;

    // Fatigue difference.
    let mut hn = r
        .relative_melee_fatigue_multiplier
        .wrapping_mul(d.fatigue.wrapping_sub(a.fatigue));
    // Attack direction factor.
    hn = hn.wrapping_add(dir_factor(r, i.dir));
    // Attack.
    hn = hn.wrapping_add(a.attack);
    // Charge, halved against a braced defender hit frontally.
    if a.charging {
        let mut cp = a.charge;
        if d.braced && front {
            cp = c_div(cp, 2);
        }
        hn = hn.wrapping_add(cp);
    }
    // "Charge reflect": a braced anti-charge attacker, not charging, struck frontally by a charger.
    if a.braced && front && a.anti_charge && !a.charging && d.charging {
        hn = hn.wrapping_add(c_div(d.charge, r.melee_charge_factor_power_divisor));
    }
    // Environment (0 for land combatants).
    hn = hn.wrapping_add(a.environment);
    // Category bonus.
    let mut cat = 0i32;
    if a.cavalry && (d.infantry || d.f50) {
        if !d.in_square {
            cat = r.hnbonus_melee_cavalry_v_infantry;
        } else if front {
            cat = r.hnbonus_melee_cavalry_v_squareinfantry;
        }
    } else if a.infantry && d.cavalry {
        cat = if a.in_square {
            r.hnbonus_melee_cavalry_v_squareinfantry
        } else {
            r.hnbonus_melee_cavalry_v_infantry
        }
        .wrapping_neg();
    }
    if d.cavalry {
        cat = cat.wrapping_add(a.bonus_vs_cavalry);
    }
    if a.anti_charge && d.cavalry {
        cat = cat.wrapping_add(r.hnbonus_bayonet);
    }
    hn = hn.wrapping_add(cat);
    // Armour ("piercing" is always true for land combatants).
    hn = hn.wrapping_sub(c_div(d.armour, r.armour_melee_piercing_divisor));
    // Shield and defence by direction: front/left → both; right → shield only; rear → neither.
    match i.dir {
        MeleeDir::Front | MeleeDir::FlankLeft => {
            hn = hn.wrapping_sub(
                d.shield
                    .wrapping_add(c_div(d.defence, r.defense_melee_piercing_divisor)),
            );
        }
        MeleeDir::FlankRight => hn = hn.wrapping_sub(d.shield),
        MeleeDir::Rear => {}
    }
    // Entrenchment (0 for land combatants).
    hn = hn.wrapping_sub(
        r.melee_entrenchement_level_multiplier
            .wrapping_mul(d.entrenchment),
    );
    // Height: float clamp, divide, x87 round.
    let h = i.height_delta;
    let clamped = if h < r.melee_height_delta_min {
        r.melee_height_delta_min
    } else if h > r.melee_height_delta_max {
        r.melee_height_delta_max
    } else {
        h
    };
    hn = hn.wrapping_sub(x87_round(clamped / r.relative_melee_height_delta_divisor));
    // Walls / buildings.
    if a.on_walls || d.on_walls {
        hn = hn.wrapping_add(10);
    } else if a.in_building || d.in_building {
        hn = hn.wrapping_add(20);
    }
    hn
}

/// Kill chance (per mille) before the extra-attacker bonus, 0x00DADA40. W1 §12.9 (CONFIRMED from
/// the bytes; the log strings' 154/112/76 are stale):
/// `hn >= -6 → 254+13hn; hn >= -12 → 184+6hn; else 125+3hn`, clamped to 1..=990.
pub fn base_kill_chance(hn: i32) -> i32 {
    let kc = if hn >= -6 {
        254i32.wrapping_add(hn.wrapping_mul(13))
    } else if hn >= -12 {
        184i32.wrapping_add(hn.wrapping_mul(6))
    } else {
        125i32.wrapping_add(hn.wrapping_mul(3))
    };
    kc.clamp(1, 990)
}

/// Kill chance including the extra-attacker bonus. W1 §12.9 (CONFIRMED):
/// `if n > 1 and kc > 0: kc = clamp(kc + (n-1)*round(kc*0.5), 1, 990)`, where `n` is
/// `E.attackers_on_target`. `round` uses the x87 default (ties to even, INFERRED for this call).
pub fn kill_chance(hn: i32, attackers_on_target: u32) -> i32 {
    let kc = base_kill_chance(hn);
    if attackers_on_target > 1 && kc > 0 {
        let extra = (attackers_on_target - 1) as i32;
        kc.wrapping_add(extra.wrapping_mul(x87_round(kc as f32 * 0.5)))
            .clamp(1, 990)
    } else {
        kc
    }
}

/// The xholds tier `xi` (0..=4) for a hit number. W1 §12.9 (CONFIRMED):
/// `hn < max0 → 0; hn < max1 → 1; hn < max2 → 2; hn >= max3 → 4; else 3`.
pub fn xholds_tier(hn: i32, r: &KvRules) -> usize {
    if hn < r.melee_hn_to_xholds_0_max {
        0
    } else if hn < r.melee_hn_to_xholds_1_max {
        1
    } else if hn < r.melee_hn_to_xholds_2_max {
        2
    } else if hn >= r.melee_hn_to_xholds_3_max {
        4
    } else {
        3
    }
}

/// The 1..=1000 roll, 0x00DADB20. W1 §12.9 (CONFIRMED): `1 + min(999, (next16()*1000)/0xFFFF)`.
pub fn roll_1000(rng: &mut CaRng) -> i32 {
    (1 + ((rng.next16() * 1000) / 0xFFFF).min(999)) as i32
}

/// Turns a roll into an outcome. The xholds are cumulative offsets above `kc`. W1 §12.9 (CONFIRMED).
pub fn outcome_for_roll(roll: i32, kc: i32, tier: usize, r: &KvRules) -> BlowOutcome {
    let (knockdown, knockback, stepback) = r.melee_xholds(tier);
    if roll < kc {
        BlowOutcome::Kill
    } else if roll < kc.wrapping_add(knockdown) {
        BlowOutcome::Knockdown
    } else if roll < kc.wrapping_add(knockback) {
        BlowOutcome::Knockback
    } else if roll < kc.wrapping_add(stepback) {
        BlowOutcome::Stepback
    } else {
        BlowOutcome::Miss
    }
}

/// Resolves one blow, 0x00DAA290: hit number → kill chance → tier → **one** RNG roll → outcome.
/// W1 §12.9 (CONFIRMED).
pub fn resolve_blow(
    rng: &mut CaRng,
    i: &MeleeInputs,
    attackers_on_target: u32,
    r: &KvRules,
) -> BlowOutcome {
    let hn = hit_number(i, r);
    let kc = kill_chance(hn, attackers_on_target);
    let tier = xholds_tier(hn, r);
    let roll = roll_1000(rng);
    outcome_for_roll(roll, kc, tier, r)
}

// ---------------------------------------------------------------------------------------------
// Pair selection, 0x006AFE20
// ---------------------------------------------------------------------------------------------

/// Pair-selection priority (0x006AF3B0). W1 §12.9 (CONFIRMED numbers, INFERRED meaning):
/// 0 if not charging; when charging, 1 for entity type 0, 2 for types 1–2 (3 if `FUN_0055C210`
/// passes, supplied as `special`).
pub fn priority(charging: bool, entity_type: u8, special: bool) -> u8 {
    if !charging {
        0
    } else if entity_type == 0 {
        1
    } else if special {
        3
    } else {
        2
    }
}

/// Selection weight: `float(unit.melee)`, ×2 for entity types 1–3. W1 §12.9 (CONFIRMED).
pub fn selection_weight(unit_melee: i32, entity_type: u8) -> f32 {
    let w = unit_melee as f32;
    if (1..=3).contains(&entity_type) {
        w * 2.0
    } else {
        w
    }
}

/// Weighted pick, **RNG roll #1** of an exchange. W1 §12.9 (CONFIRMED roll:
/// `u = next16()*1.5259022e-05*total_weight`, then a binary search over cumulative weights).
///
/// INFERRED search semantics: the first index whose cumulative weight is `> u`; if none (e.g.
/// `u == total`), the last index with a positive weight (or the last index if all are zero).
/// So a zero-weight member is never picked while any member has weight. Returns `None` for an
/// empty slice.
pub fn pick_weighted(rng: &mut CaRng, weights: &[f32]) -> Option<usize> {
    if weights.is_empty() {
        return None;
    }
    let total: f32 = weights.iter().sum();
    let u = rng.next16() as f32 * INV_65535 * total;
    let mut cum = 0.0f32;
    let mut cumulative = Vec::with_capacity(weights.len());
    for w in weights {
        cum += w;
        cumulative.push(cum);
    }
    let idx = cumulative.partition_point(|&c| c <= u);
    if idx < weights.len() {
        return Some(idx);
    }
    Some(
        weights
            .iter()
            .rposition(|&w| w > 0.0)
            .unwrap_or(weights.len() - 1),
    )
}

/// Uniform index pick used for the defender: `min(N-1, (next16()*N)/0xFFFF)`. W1 §12.9 (CONFIRMED).
pub fn pick_index(rng: &mut CaRng, n: usize) -> usize {
    let n32 = n as u32;
    ((rng.next16().wrapping_mul(n32)) / 0xFFFF).min(n32.saturating_sub(1)) as usize
}

/// One member of an encounter list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EncounterMember {
    /// Alliance / side.
    pub side: u8,
    /// Selection priority, see [`priority`].
    pub priority: u8,
    /// Selection weight, see [`selection_weight`].
    pub weight: f32,
}

/// The fighting pair chosen by [`select_pair`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pair {
    /// Index of the attacker in the list.
    pub attacker: usize,
    /// Index of the defender in the list.
    pub defender: usize,
    /// `E.attackers_on_target`: list members on the attacker's side.
    pub attackers_on_target: u32,
    /// True if the attacker's priority is non-zero (it was charging): a MISS triggers a strike back.
    pub attacker_charging: bool,
}

/// Pair selection, 0x006AFE20, steps 1–4. W1 §12.9 (CONFIRMED order of RNG calls):
/// 1. Candidates = members with the highest priority.
/// 2. Roll #1: weighted pick among the candidates.
/// 3. Rolls #2..k: uniform pick over the **whole** list, repeated until the member is on another side.
/// 4. `attackers_on_target` = members on the attacker's side.
///
/// Returns `None` (and consumes no randomness) if the list has fewer than two sides.
pub fn select_pair(rng: &mut CaRng, list: &[EncounterMember]) -> Option<Pair> {
    let first_side = list.first()?.side;
    if list.iter().all(|m| m.side == first_side) {
        return None;
    }
    let top = list.iter().map(|m| m.priority).max()?;
    let candidates: Vec<usize> = (0..list.len())
        .filter(|&k| list[k].priority == top)
        .collect();
    let weights: Vec<f32> = candidates.iter().map(|&k| list[k].weight).collect();
    let attacker = candidates[pick_weighted(rng, &weights)?];
    let side = list[attacker].side;
    let defender = loop {
        let k = pick_index(rng, list.len());
        if list[k].side != side {
            break k;
        }
    };
    let attackers_on_target = list.iter().filter(|m| m.side == side).count() as u32;
    Some(Pair {
        attacker,
        defender,
        attackers_on_target,
        attacker_charging: top != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OBVIOUSLY MADE-UP placeholder rules (NOT game data): distinct small numbers so every term's
    /// contribution can be checked by hand.
    fn rules() -> KvRules {
        KvRules {
            relative_melee_fatigue_multiplier: 2,
            melee_charge_factor_power_divisor: 3,
            melee_entrenchement_level_multiplier: 5,
            melee_height_delta_min: -10.0,
            melee_height_delta_max: 10.0,
            relative_melee_height_delta_divisor: 2.0,
            hnbonus_bayonet: 7,
            hnbonus_melee_cavalry_v_infantry: 11,
            hnbonus_melee_cavalry_v_squareinfantry: 13,
            armour_melee_piercing_divisor: 2,
            defense_melee_piercing_divisor: 4,
            factor_attackdir_front: 0,
            factor_attackdir_flankleft: 100,
            factor_attackdir_flankright: 200,
            factor_attackdir_rear: 300,
            melee_hn_to_xholds_0_max: -10,
            melee_hn_to_xholds_1_max: 0,
            melee_hn_to_xholds_2_max: 10,
            melee_hn_to_xholds_3_max: 20,
            melee_xholds_knockdown_1: 50,
            melee_xholds_knockback_1: 100,
            melee_xholds_stepback_1: 150,
            ..KvRules::default()
        }
    }

    fn base() -> MeleeInputs {
        MeleeInputs {
            attacker: Combatant {
                attack: 10,
                ..Default::default()
            },
            defender: Combatant::default(),
            dir: MeleeDir::Front,
            height_delta: 0.0,
        }
    }

    #[test]
    fn attack_and_fatigue_and_direction() {
        let r = rules();
        assert_eq!(hit_number(&base(), &r), 10);
        let mut i = base();
        i.attacker.fatigue = 3;
        i.defender.fatigue = 8;
        assert_eq!(hit_number(&i, &r), 10 + 2 * 5);
        for (dir, f) in [
            (MeleeDir::FlankLeft, 100),
            (MeleeDir::FlankRight, 200),
            (MeleeDir::Rear, 300),
        ] {
            assert_eq!(hit_number(&MeleeInputs { dir, ..base() }, &r), 10 + f);
        }
    }

    #[test]
    fn charge_halving_truncates() {
        let r = rules();
        let mut i = base();
        i.attacker.charging = true;
        i.attacker.charge = 7;
        assert_eq!(hit_number(&i, &r), 17);
        i.defender.braced = true;
        assert_eq!(hit_number(&i, &r), 13); // 7 / 2 = 3
        i.attacker.charge = -7;
        assert_eq!(hit_number(&i, &r), 7); // -7 / 2 = -3 (toward zero)
        // Not frontal: no halving.
        i.attacker.charge = 7;
        i.dir = MeleeDir::Rear;
        assert_eq!(hit_number(&i, &r), 10 + 300 + 7);
    }

    #[test]
    fn charge_reflect() {
        let r = rules();
        let mut i = base();
        i.attacker.braced = true;
        i.attacker.anti_charge = true;
        i.defender.charging = true;
        i.defender.charge = 10;
        // + 10/3 = 3 (no category bonus: defender not cavalry)
        assert_eq!(hit_number(&i, &r), 13);
        i.attacker.charging = true; // reflect needs the attacker NOT charging
        assert_eq!(hit_number(&i, &r), 10);
    }

    #[test]
    fn category_bonuses() {
        let r = rules();
        // Cavalry vs infantry not in square: +11.
        let mut i = base();
        i.attacker.cavalry = true;
        i.defender.infantry = true;
        assert_eq!(hit_number(&i, &r), 21);
        // vs square, frontal: +13; vs square from the flank: 0.
        i.defender.in_square = true;
        assert_eq!(hit_number(&i, &r), 23);
        i.dir = MeleeDir::FlankRight;
        assert_eq!(hit_number(&i, &r), 10 + 200);
        // Infantry vs cavalry: -11 (or -13 in square), + bonus_vs_cavalry, + bayonet if anti_charge.
        let mut i = base();
        i.attacker.infantry = true;
        i.attacker.bonus_vs_cavalry = 4;
        i.attacker.anti_charge = true;
        i.defender.cavalry = true;
        assert_eq!(hit_number(&i, &r), 10 - 11 + 4 + 7);
        i.attacker.in_square = true;
        assert_eq!(hit_number(&i, &r), 10 - 13 + 4 + 7);
    }

    #[test]
    fn armour_shield_defence_by_direction() {
        let r = rules();
        let mut i = base();
        i.defender.armour = 5; // 5/2 = 2
        i.defender.shield = 3;
        i.defender.defence = 9; // 9/4 = 2
        assert_eq!(hit_number(&i, &r), 10 - 2 - (3 + 2));
        i.dir = MeleeDir::FlankLeft;
        assert_eq!(hit_number(&i, &r), 10 + 100 - 2 - 5);
        i.dir = MeleeDir::FlankRight;
        assert_eq!(hit_number(&i, &r), 10 + 200 - 2 - 3);
        i.dir = MeleeDir::Rear;
        assert_eq!(hit_number(&i, &r), 10 + 300 - 2);
    }

    #[test]
    fn height_clamp_and_round() {
        let r = rules();
        let h = |d: f32| {
            hit_number(
                &MeleeInputs {
                    height_delta: d,
                    ..base()
                },
                &r,
            )
        };
        assert_eq!(h(4.0), 10 - 2);
        assert_eq!(h(5.0), 10 - 2); // 2.5 rounds to even 2
        assert_eq!(h(7.0), 10 - 4); // 3.5 rounds to even 4
        assert_eq!(h(100.0), 10 - 5); // clamped to 10
        assert_eq!(h(-100.0), 10 + 5);
        assert_eq!(height_delta(1.0, 4.0, 9.0, false), 3.0);
        assert_eq!(height_delta(1.0, 4.0, 9.0, true), 3.0 / 9.0);
    }

    #[test]
    fn walls_then_building() {
        let r = rules();
        let mut i = base();
        i.defender.in_building = true;
        assert_eq!(hit_number(&i, &r), 30);
        i.attacker.on_walls = true; // walls take priority: +10 only
        assert_eq!(hit_number(&i, &r), 20);
    }

    #[test]
    fn kill_chance_bands() {
        assert_eq!(base_kill_chance(0), 254);
        assert_eq!(base_kill_chance(-6), 254 - 78);
        assert_eq!(base_kill_chance(-7), 184 - 42);
        assert_eq!(base_kill_chance(-12), 184 - 72);
        assert_eq!(base_kill_chance(-13), 125 - 39);
        assert_eq!(base_kill_chance(-100), 1); // clamp low
        assert_eq!(base_kill_chance(60), 990); // 254 + 780 = 1034 -> clamp high
        // Extra attackers: kc + (n-1)*round(kc*0.5).
        assert_eq!(kill_chance(0, 1), 254);
        assert_eq!(kill_chance(0, 2), 254 + 127);
        assert_eq!(kill_chance(0, 3), 254 + 254);
        assert_eq!(kill_chance(-13, 2), 86 + 43);
        // kc = 125 - 42 = 83 at hn = -14: 41.5 rounds to even 42.
        assert_eq!(kill_chance(-14, 2), 83 + 42);
        assert_eq!(kill_chance(20, 4), 990);
    }

    /// The clamp of `0x00DAC2A0` (CONFIRMED, BATTLE_FIDELITY.md §57 (3)): `1` low, `0x3DE` = 990
    /// high, applied twice (once to the base kill chance, once after the extra-attacker bonus).
    #[test]
    fn kill_chance_clamp_is_1_to_990() {
        assert_eq!(990, 0x3DE);
        assert_eq!(base_kill_chance(999), 990);
        assert_eq!(base_kill_chance(-999), 1);
        // The extra-attacker bonus is clamped as well, not applied raw.
        assert_eq!(kill_chance(100, 8), 990);
        assert_eq!(kill_chance(-999, 8), 1);
        // A single attacker never gets the bonus, whatever the roll-free inputs say.
        assert_eq!(kill_chance(0, 0), base_kill_chance(0));
    }

    /// The original's melee has **no experience term** (CONFIRMED, BATTLE_FIDELITY.md §57 (1)):
    /// `relative_melee_experience_multiplier` is `kv_rules` key 0, so the rules adapter exposes it
    /// at vtable slot `+0`, and no function of the blow-resolution chain reads that slot. This test
    /// locks that in: changing the multiplier must not move the hit number, the kill chance or the
    /// resolved blow, so nobody "fixes" a missing experience term later without new evidence.
    #[test]
    fn no_experience_term_in_the_melee_chain() {
        let mut with = rules();
        with.relative_melee_experience_multiplier = 1;
        let mut without = rules();
        without.relative_melee_experience_multiplier = 999;
        // A melee with different attacker/defender stat lines, so every other term is live.
        let mut i = base();
        i.attacker.attack = 23;
        i.attacker.charge = 9;
        i.attacker.charging = true;
        i.attacker.fatigue = 2;
        i.defender.attack = 17;
        i.defender.armour = 5;
        i.defender.shield = 4;
        i.defender.defence = 6;
        i.defender.fatigue = 7;
        i.dir = MeleeDir::FlankRight;
        i.height_delta = 3.5;
        assert_eq!(hit_number(&i, &with), hit_number(&i, &without));
        // Sanity: the sample really does exercise the fatigue, direction, armour/shield and height
        // terms, so the equality above is not vacuous (attack 23 + charge 9 + flank-right 200
        // + fatigue 2·(7−2) − armour 5/2 − shield 4 − round(3.5/2) = 234).
        assert_eq!(hit_number(&i, &with), 23 + 9 + 200 + 10 - 2 - 4 - 2);
        let mut a = CaRng::new(4242);
        let mut b = CaRng::new(4242);
        assert_eq!(resolve_blow(&mut a, &i, 3, &with), resolve_blow(&mut b, &i, 3, &without));
    }

    #[test]
    fn tiers_and_outcomes() {
        let r = rules();
        assert_eq!(xholds_tier(-11, &r), 0);
        assert_eq!(xholds_tier(-10, &r), 1);
        assert_eq!(xholds_tier(9, &r), 2);
        assert_eq!(xholds_tier(10, &r), 3);
        assert_eq!(xholds_tier(19, &r), 3);
        assert_eq!(xholds_tier(20, &r), 4);
        // kc 100, tier 1 offsets 50/100/150.
        assert_eq!(outcome_for_roll(99, 100, 1, &r), BlowOutcome::Kill);
        assert_eq!(outcome_for_roll(100, 100, 1, &r), BlowOutcome::Knockdown);
        assert_eq!(outcome_for_roll(149, 100, 1, &r), BlowOutcome::Knockdown);
        assert_eq!(outcome_for_roll(150, 100, 1, &r), BlowOutcome::Knockback);
        assert_eq!(outcome_for_roll(200, 100, 1, &r), BlowOutcome::Stepback);
        assert_eq!(outcome_for_roll(250, 100, 1, &r), BlowOutcome::Miss);
    }

    #[test]
    fn roll_range_and_resolve_uses_one_roll() {
        let mut rng = CaRng::new(0);
        for _ in 0..1000 {
            let v = roll_1000(&mut rng);
            assert!((1..=1000).contains(&v));
        }
        // Seed 0: first next16 = 38 -> 1 + 38000/65535 = 1.
        assert_eq!(roll_1000(&mut CaRng::new(0)), 1);
        let mut a = CaRng::new(0);
        let out = resolve_blow(&mut a, &base(), 1, &rules());
        assert_eq!(out, BlowOutcome::Kill); // roll 1 < kc 384
        let mut b = CaRng::new(0);
        b.next16();
        assert_eq!(a, b, "exactly one RNG call per blow");
    }

    #[test]
    fn pickers() {
        assert_eq!(pick_index(&mut CaRng::new(0), 10), 0); // 38*10/65535 = 0
        // Weighted: seed 0 gives u = 38/65535*total, tiny -> first non-zero weight.
        assert_eq!(pick_weighted(&mut CaRng::new(0), &[0.0, 1.0, 1.0]), Some(1));
        assert_eq!(pick_weighted(&mut CaRng::new(0), &[]), None);
        assert_eq!(priority(false, 2, true), 0);
        assert_eq!(priority(true, 0, true), 1);
        assert_eq!(priority(true, 1, false), 2);
        assert_eq!(priority(true, 2, true), 3);
        assert_eq!(selection_weight(10, 0), 10.0);
        assert_eq!(selection_weight(10, 3), 20.0);
    }

    #[test]
    fn pair_selection() {
        let m = |side, priority| EncounterMember {
            side,
            priority,
            weight: 1.0,
        };
        // One charging member on side 1: it must be the attacker.
        let list = [m(0, 0), m(0, 0), m(1, 1), m(1, 0)];
        for seed in 0..50 {
            let p = select_pair(&mut CaRng::new(seed), &list).unwrap();
            assert_eq!(p.attacker, 2);
            assert!(p.defender < 2);
            assert_eq!(p.attackers_on_target, 2);
            assert!(p.attacker_charging);
        }
        assert_eq!(select_pair(&mut CaRng::new(0), &[m(0, 0), m(0, 0)]), None);
    }
}

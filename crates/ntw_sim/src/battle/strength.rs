//! Unit strength as the morale code reads it: the rally test (`0x0055C500`) and the neighbour
//! sub-evaluator (`0x0053CBD0`) both sum `0x006AFB70` (melee term) + `0x006B05B0` (missile term).
//! The battle AI's balance (`0x006A31E0`) sums the same two functions.
//!
//! CONFIRMED (BATTLE_FIDELITY.md §6):
//! - each term is 0 when the unit's morale state is 7 (`unit+0xC28`) or its behaviour is 3
//!   (`unit+0xC2C`), i.e. shattered;
//! - otherwise it is the record-level potential (`0x00757120` melee, `0x007575A0` missile) of the
//!   unit's stat block (`unit+0x20`, the `LAND_UNIT_RECORD` copy) for the unit card's men count
//!   (`(unit+0x1C)+0xC8`), times `1 - total casualty ratio` (`unit+0xCB4`), floored at 0.
//!
//! The model has no unit card, so the potential is computed for the starting men and scaled by
//! `men / start men` (PROVISIONAL: INFERRED that the card count is the starting strength, which
//! makes this the same number). Terms the model cannot feed are left at 0 and listed in the
//! function docs (UNKNOWN fields of the unit card, and two unit-level switches).

use super::ground::MovementClass;
use super::model::LandUnit;
use super::morale::{MoraleBehaviour, MoraleState};

pub use crate::unit_kind::Category;

/// The unit's category ([`crate::unit_kind::category`]: the original's codes, an unknown key
/// artillery). A unit built without data (empty key) falls back to its model flags (PROVISIONAL:
/// artillery weapon → artillery, cavalry → cavalry, otherwise infantry).
pub fn category(u: &LandUnit) -> Category {
    match u.unit_category.as_str() {
        "" if u.missile.is_some_and(|w| w.is_artillery) => Category::Artillery,
        "" if u.is_cavalry => Category::Cavalry,
        "" => Category::Infantry,
        key => crate::unit_kind::category(key),
    }
}

/// `0x0055ABF0` (CONFIRMED static): the unit rides: category cavalry (0) or camels (5), or
/// dragoons (3) without the `dismount` ability (15) active. Its callers in ours: the in-place
/// check (`0x00557BC0`, march.rs), the neighbour weight halving (`0x0053CBD0`, model.rs), the
/// all-men volley (`0x005694D0`, shooting.rs) and the battle AI (`0x007A4EB0`, `0x007536C0`,
/// `0x007D33B0`, `0x007D34A0`). [`riding`] is the other predicate.
pub fn mounted(u: &LandUnit) -> bool {
    let c = category(u);
    mounted_category(c) && (c != Category::Dragoons || !dismount_active(u))
}

/// The categories [`mounted`] counts while no `dismount` is active: cavalry (0), dragoons (3) and
/// camels (5) (`0x0055ABF0`, CONFIRMED static).
fn mounted_category(c: Category) -> bool {
    matches!(c, Category::Cavalry | Category::Dragoons | Category::Camels)
}

/// `0x0055C2A0` (CONFIRMED static): [`mounted`] and the `dismount` ability (15) not active, for
/// every category. It differs from [`mounted`] only for a cavalry or camel unit with `dismount`
/// active. Its callers in ours: frightened horses (`0x0053B970`, model.rs), the morale
/// attack-direction pass (`0x0053C720`: the front adjust and fighting cavalry, model.rs), the firing
/// drill (`0x00554920`, shooting.rs) and the ground column (`0x006543D0`, [`movement_class`]).
pub fn riding(u: &LandUnit) -> bool {
    mounted(u) && !dismount_active(u)
}

/// The `dismount` ability (15) is active (`IsSpecialAbilityActive(+0xDAC, 15)`, CONFIRMED).
pub fn dismount_active(u: &LandUnit) -> bool {
    u.active_abilities & (1 << super::abilities::ability_ids::DISMOUNT) != 0
}

/// The `square_formation` (1) or `pike_square_formation` (0) ability is active
/// (`IsSpecialAbilityActive(+0xDAC, 1)` then `(+0xDAC, 0)`, CONFIRMED at `0x0053C881`–`0x0053C8A1`,
/// where it blocks the fighting-cavalry effect; the attack-direction test `0x0057F310` asks the same
/// pair).
pub fn square_active(u: &LandUnit) -> bool {
    use super::attributes::ability::{PIKE_SQUARE_FORMATION, SQUARE_FORMATION};
    u.active_abilities & ((1 << SQUARE_FORMATION) | (1 << PIKE_SQUARE_FORMATION)) != 0
}

/// The movement class a unit is built with, from its category, its `units` class key and whether
/// it has guns (`unit_stats_land` column 23): the tests of `0x006543D0` with no ability active
/// (CONFIRMED). Artillery with guns (`0x0055AB90`: category 1 and its guns) takes the column of its
/// class (`artillery_foot`, `artillery_horse`) or none; a mounted category (`0x0055ABF0`) the mounted
/// column, whatever mounts the unit has; infantry and gunless artillery (`0x0055C1C0`) the infantry
/// column; any other category (elephants, naval) none. [`movement_class`] then follows `dismount`.
pub fn built_movement_class(category: Category, unit_class: &str, has_guns: bool) -> MovementClass {
    match category {
        Category::Artillery if has_guns => match crate::unit_kind::class_code(unit_class) {
            crate::unit_kind::ARTILLERY_FOOT => MovementClass::FootArtillery,
            crate::unit_kind::ARTILLERY_HORSE => MovementClass::HorseArtillery,
            _ => MovementClass::Fixed,
        },
        c if mounted_category(c) => MovementClass::Mounted,
        Category::Infantry | Category::Artillery => MovementClass::Infantry,
        _ => MovementClass::Fixed,
    }
}

/// The `unit_movement_modifiers` column the unit's men use this tick (`0x006543D0`, CONFIRMED):
/// the class set at setup ([`built_movement_class`]), except that a mounted unit that is not
/// [`riding`] (`0x006544AB`) takes the infantry column 3, like a unit on foot, and so does a unit
/// of another non-artillery category with `dismount` active (`0x0055C1C0`).
pub fn movement_class(u: &LandUnit) -> MovementClass {
    match u.movement_class {
        MovementClass::Mounted if !riding(u) => MovementClass::Infantry,
        MovementClass::Fixed if dismount_active(u) && category(u) != Category::Artillery => MovementClass::Infantry,
        class => class,
    }
}

/// True if the strength functions count this unit (not shattered, CONFIRMED tests).
pub fn counts(u: &LandUnit) -> bool {
    u.men > 0 && u.morale.state != MoraleState::Shattered
        && u.morale.behaviour != MoraleBehaviour::Shattered
}

/// The per-man class term of `0x00757120` (CONFIRMED): cavalry 8 (but `cavalry_missile` 4),
/// dragoons 4, elephants 20, camels 8, everything else (infantry, artillery, naval) 0.
pub fn class_term(u: &LandUnit) -> f32 {
    match category(u) {
        Category::Cavalry if u.unit_class == "cavalry_missile" => 4.0,
        Category::Cavalry | Category::Camels => 8.0,
        Category::Dragoons => 4.0,
        Category::Elephants => 20.0,
        _ => 0.0,
    }
}

/// `0x007578E0` (CONFIRMED): the per-man value of the unit's own missile weapon (the record's
/// projectile `+0xE0`, `unit_stats_land` column 30):
/// `(accuracy * 0.7 + range * 0.5) / ((100 - reload_skill) * 0.01 * reload_time + 12)`, + 0.4 for
/// class `infantry_grenadiers`, halved for category cavalry. 0 without that projectile.
///
/// Artillery rows have no column-30 projectile (CONFIRMED in the shipped data: 0 of 48), so an
/// artillery weapon (from the gun type) gives 0 here.
pub fn missile_per_man(u: &LandUnit) -> f32 {
    let Some(w) = u.missile.filter(|w| !w.is_artillery) else { return 0.0 };
    let reload = (100 - w.reload_skill) as f32 * 0.01 * w.reload_time_s as f32 + 12.0;
    let mut v = (w.accuracy * 0.7 + w.range as f32 * 0.5) / reload;
    if u.unit_class == "infantry_grenadiers" {
        v += 0.4;
    }
    if category(u) == Category::Cavalry {
        v *= 0.5;
    }
    v
}

/// The share of the record potential the unit still has: `1 - total casualty ratio`.
fn remaining(u: &LandUnit) -> f32 {
    u.men as f32 / u.max_men.max(1) as f32
}

/// Melee term `0x006AFB70` over the potential `0x00757120` (CONFIRMED):
/// ```text
/// per_man = (defence + shield) * 0.05 + armour * 0.3 + attack * 0.06 + charge * 0.04
///         + bonus_vs_cavalry * 0.03 + class_term
/// P = per_man * men + morale * (missile_per_man == 0 ? 10 : 5)
///   + 60 col63 + 30 marksmen + 50 steadfast + 50 frightens_horses + 70 frightens_enemy
///   + 30 good_stamina + 70 inspires + card ability terms
/// strength = max(P * (1 - casualty ratio), 0)
/// ```
/// Unit-card terms (CONFIRMED, BATTLE_FIDELITY.md §19): ability `plug_bayonets` +20,
/// `square_formation` +50. Not modelled: the ×10 when `unit+0xE14 != 0 && unit+0xE10 == 0`.
pub fn melee_strength(u: &LandUnit) -> f32 {
    if !counts(u) {
        return 0.0;
    }
    let a = &u.attributes;
    let per_man = (u.melee_defence + u.shield) as f32 * 0.05
        + u.armour as f32 * 0.3
        + u.melee_attack as f32 * 0.06
        + u.charge_bonus as f32 * 0.04
        + u.bonus_vs_cavalry as f32 * 0.03
        + class_term(u);
    let morale_factor = if missile_per_man(u) == 0.0 { 10.0 } else { 5.0 };
    let flag = |on: bool, v: f32| if on { v } else { 0.0 };
    let p = per_man * u.max_men as f32
        + u.morale_stat as f32 * morale_factor
        + flag(a.col63, 60.0)
        + flag(a.marksmen, 30.0)
        + flag(a.steadfast, 50.0)
        + flag(a.frightens_horses, 50.0)
        + flag(a.frightens_enemy, 70.0)
        + flag(a.good_stamina, 30.0)
        + flag(a.inspires, 70.0)
        + card_melee_terms(u);
    (p.max(0.0) * remaining(u)).max(0.0)
}

/// Missile term `0x006B05B0` over the potential `0x007575A0` (CONFIRMED):
/// ```text
/// 0 if the unit has no own projectile and is not artillery
/// base = artillery ? (2 * accuracy + 3 * damage + 0.3 * range)
///                     / max((100 - reload_skill) * 0.01 * reload_time, 0.01) * men
///                  : missile_per_man * men
/// P = base + (missile_per_man > 0 ? morale * 5 : 0) + 70 col64
///   + 150 artillery_foot / 250 artillery_horse + card terms
/// strength = max(P * (1 - casualty ratio), 0)
/// ```
/// The exe does not look at the ammunition here. The artillery projectile is the first of the gun
/// type's list whose shot type passes a test (`+0x24 → +0x10 == 0`, meaning UNKNOWN), else the
/// first; the model uses the weapon the battle setup picked (PROVISIONAL).
/// Unit-card terms (CONFIRMED, BATTLE_FIDELITY.md §19): ability `fire_and_advance` +20; firing
/// drill 2..5 (platoon / rank fire) +70; shot types canister +110, rocket +300, shrapnel +70,
/// explosive / percussive shell, carcass, quicklime +50 each. Not modelled: the reduction by the
/// share of inactive crew (`FUN_0055AB90`), and 0 when `unit+0xE14 != 0 && unit+0xE10 == 0`
/// (UNKNOWN switch).
pub fn missile_strength(u: &LandUnit) -> f32 {
    if !counts(u) {
        return 0.0;
    }
    let artillery = category(u) == Category::Artillery;
    let per_man = missile_per_man(u);
    let own_projectile = u.missile.is_some_and(|w| !w.is_artillery);
    if !own_projectile && !artillery {
        return 0.0;
    }
    let men = u.max_men as f32;
    let base = if artillery {
        match u.missile {
            Some(w) => {
                let reload = ((100 - w.reload_skill) as f32 * 0.01 * w.reload_time_s as f32).max(0.01);
                (2.0 * w.accuracy + 3.0 * w.damage + 0.3 * w.range as f32) / reload * men
            }
            None => 0.0,
        }
    } else {
        men * per_man
    };
    let class_bonus = match u.unit_class.as_str() {
        "artillery_horse" => 250.0,
        "artillery_foot" => 150.0,
        _ => 0.0,
    };
    let p = base
        + if per_man > 0.0 { u.morale_stat as f32 * 5.0 } else { 0.0 }
        + if u.attributes.col64 { 70.0 } else { 0.0 }
        + class_bonus
        + card_missile_terms(u);
    (p.max(0.0) * remaining(u)).max(0.0)
}

/// The melee potential's unit-card terms (`0x00757120`, CONFIRMED): any listed ability 8
/// (`plug_bayonets`) +20, any ability 1 (`square_formation`) +50.
pub fn card_melee_terms(u: &LandUnit) -> f32 {
    use super::attributes::ability;
    let c = &u.capabilities;
    let mut t = 0.0;
    if c.has_ability(ability::PLUG_BAYONETS) {
        t += 20.0;
    }
    if c.has_ability(ability::SQUARE_FORMATION) {
        t += 50.0;
    }
    t
}

/// The missile potential's unit-card terms (`0x007575A0`, CONFIRMED): ability 7
/// (`fire_and_advance`) +20; firing drill 2..5 +70; each listed shot type: 3 canister +110,
/// 10 rocket +300, 4 shrapnel +70, 1 explosive shell / 2 percussive shell / 5 carcass /
/// 6 quicklime +50 (the exe sums its list; ours holds each type once).
pub fn card_missile_terms(u: &LandUnit) -> f32 {
    use super::attributes::ability;
    let c = &u.capabilities;
    let mut t = 0.0;
    if c.has_ability(ability::FIRE_AND_ADVANCE) {
        t += 20.0;
    }
    if (2..=5).contains(&c.firing_drill) {
        t += 70.0;
    }
    for (shot, v) in [(3u8, 110.0), (10, 300.0), (4, 70.0), (1, 50.0), (2, 50.0), (5, 50.0), (6, 50.0)] {
        if c.has_shot_type(shot) {
            t += v;
        }
    }
    t
}

/// Strength as the rally test sums it: melee term + missile term.
pub fn strength(u: &LandUnit) -> f32 {
    melee_strength(u) + missile_strength(u)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::shooting::MissileWeapon;

    fn musket() -> MissileWeapon {
        MissileWeapon {
            range: 80,
            accuracy: 40.0,
            reload_skill: 50,
            reload_time_s: 20,
            damage: 10.0,
            projectiles_per_shot: 1,
            is_artillery: false,
            guns: 0,
            ballistics: Default::default(),
        }
    }

    fn line(men: u32) -> LandUnit {
        let mut u = LandUnit::new(1, 0, men, (0.0, 0.0));
        u.unit_category = "infantry".into();
        u.unit_class = "infantry_line".into();
        u.melee_attack = 5;
        u.melee_defence = 6;
        u.armour = 2;
        u.charge_bonus = 10;
        u.morale_stat = 8;
        u.missile = Some(musket());
        u
    }

    /// `0x0055ABF0`: cavalry and camels ride; dragoons ride unless dismounted; the rest walk.
    #[test]
    fn mounted_follows_the_category_and_dismount() {
        let mut u = line(100);
        assert!(!mounted(&u));
        for key in ["cavalry", "cavalry_camels"] {
            u.unit_category = key.into();
            u.active_abilities = 1 << super::super::abilities::ability_ids::DISMOUNT;
            assert!(mounted(&u), "{key}");
        }
        u.unit_category = "dragoons".into();
        assert!(!mounted(&u), "dismounted dragoons");
        u.active_abilities = 0;
        assert!(mounted(&u));
        u.unit_category = "elephants".into();
        assert!(!mounted(&u));
    }

    /// `0x0055C2A0`: mounted and not dismounted, so cavalry and camels with `dismount` active are
    /// mounted (`0x0055ABF0`) but not riding.
    #[test]
    fn riding_excludes_every_dismounted_unit() {
        let dismount = 1 << super::super::abilities::ability_ids::DISMOUNT;
        let mut u = line(100);
        assert!(!riding(&u));
        for key in ["cavalry", "cavalry_camels", "dragoons"] {
            u.unit_category = key.into();
            u.active_abilities = 0;
            assert!(riding(&u), "{key}");
            u.active_abilities = dismount;
            assert!(!riding(&u), "dismounted {key}");
        }
    }

    /// `0x006543D0`: a horse unit takes the riders' ground column only while riding (`0x006544AB`);
    /// dismounted it walks on the infantry column.
    #[test]
    fn movement_class_follows_riding() {
        let mut u = line(100);
        u.unit_category = "cavalry".into();
        u.movement_class = MovementClass::Mounted;
        assert_eq!(movement_class(&u), MovementClass::Mounted);
        u.active_abilities = 1 << super::super::abilities::ability_ids::DISMOUNT;
        assert_eq!(movement_class(&u), MovementClass::Infantry);
        u.movement_class = MovementClass::FootArtillery;
        assert_eq!(movement_class(&u), MovementClass::FootArtillery, "artillery with guns keeps its column");
        u.unit_category = "elephants".into();
        u.movement_class = MovementClass::Fixed;
        assert_eq!(movement_class(&u), MovementClass::Infantry, "0x0055C1C0: dismount gives the infantry column");
        u.active_abilities = 0;
        assert_eq!(movement_class(&u), MovementClass::Fixed);
    }

    /// `0x006543D0` keys the built class on the category alone: a mount on an infantry-category
    /// unit does not give the mounted column, and elephants or naval units get no modifier.
    #[test]
    fn built_movement_class_follows_the_category() {
        use Category::*;
        for c in [Cavalry, Dragoons, Camels] {
            assert_eq!(built_movement_class(c, "cavalry_light", false), MovementClass::Mounted, "{c:?}");
        }
        assert_eq!(built_movement_class(Infantry, "infantry_line", false), MovementClass::Infantry);
        assert_eq!(built_movement_class(Infantry, "infantry_line", true), MovementClass::Infantry, "guns need category artillery");
        assert_eq!(built_movement_class(Artillery, "artillery_foot", true), MovementClass::FootArtillery);
        assert_eq!(built_movement_class(Artillery, "artillery_horse", true), MovementClass::HorseArtillery);
        assert_eq!(built_movement_class(Artillery, "artillery_fixed", true), MovementClass::Fixed);
        assert_eq!(built_movement_class(Artillery, "artillery_foot", false), MovementClass::Infantry, "gunless crew");
        assert_eq!(built_movement_class(Elephants, "elephant", false), MovementClass::Fixed);
        assert_eq!(built_movement_class(NavalFrigate, "", false), MovementClass::Fixed);
    }

    #[test]
    fn class_terms() {
        let mut u = line(100);
        assert_eq!(class_term(&u), 0.0); // infantry: 0
        u.unit_category = "cavalry".into();
        u.unit_class = "cavalry_light".into();
        assert_eq!(class_term(&u), 8.0);
        u.unit_class = "cavalry_missile".into();
        assert_eq!(class_term(&u), 4.0);
        u.unit_category = "dragoons".into();
        assert_eq!(class_term(&u), 4.0);
        u.unit_category = "elephants".into();
        assert_eq!(class_term(&u), 20.0);
        u.unit_category = "cavalry_camels".into();
        assert_eq!(class_term(&u), 8.0);
        u.unit_category = "artillery".into();
        assert_eq!(class_term(&u), 0.0);
        u.unit_category = "something_else".into(); // unknown key → artillery
        assert_eq!(category(&u), Category::Artillery);
    }

    #[test]
    fn per_man_missile_value() {
        let mut u = line(100);
        // (40*0.7 + 80*0.5) / ((100-50)*0.01*20 + 12) = 68 / 22
        assert!((missile_per_man(&u) - 68.0 / 22.0).abs() < 1e-5);
        u.unit_class = "infantry_grenadiers".into();
        assert!((missile_per_man(&u) - (68.0 / 22.0 + 0.4)).abs() < 1e-5);
        u.unit_category = "cavalry".into();
        assert!((missile_per_man(&u) - (68.0 / 22.0 + 0.4) * 0.5).abs() < 1e-5);
        u.missile = None;
        assert_eq!(missile_per_man(&u), 0.0);
    }

    #[test]
    fn melee_formula_and_morale_factor() {
        let u = line(100);
        let per_man = 6.0 * 0.05 + 2.0 * 0.3 + 5.0 * 0.06 + 10.0 * 0.04;
        let want = per_man * 100.0 + 8.0 * 5.0;
        assert!((melee_strength(&u) - want).abs() < 1e-3);
        // Without its own projectile the morale counts ×10.
        let mut v = line(100);
        v.missile = None;
        assert!((melee_strength(&v) - (per_man * 100.0 + 80.0)).abs() < 1e-3);
        // Attribute bonuses add flat amounts.
        let mut w = line(100);
        w.attributes.inspires = true;
        w.attributes.steadfast = true;
        assert!((melee_strength(&w) - (want + 120.0)).abs() < 1e-3);
        // Losses scale the whole potential (bonuses and morale too).
        let mut h = line(100);
        h.men = 50;
        assert!((melee_strength(&h) - want * 0.5).abs() < 1e-3);
    }

    #[test]
    fn missile_formula() {
        let u = line(100);
        let want = 100.0 * 68.0 / 22.0 + 8.0 * 5.0;
        assert!((missile_strength(&u) - want).abs() < 1e-2);
        // No own projectile and not artillery: 0.
        let mut m = line(100);
        m.missile = None;
        assert_eq!(missile_strength(&m), 0.0);
        // Artillery: the gun formula over the crew, no morale term, class bonus.
        let mut a = line(40);
        a.unit_category = "artillery".into();
        a.unit_class = "artillery_foot".into();
        a.missile = Some(MissileWeapon { is_artillery: true, guns: 2, damage: 30.0, range: 400, ..musket() });
        let reload = 50.0 * 0.01 * 20.0;
        let gun = (2.0 * 40.0 + 3.0 * 30.0 + 0.3 * 400.0) / reload * 40.0;
        assert!((missile_strength(&a) - (gun + 150.0)).abs() < 1e-2);
        // Shattered units count nothing.
        let mut s = line(100);
        s.morale.state = MoraleState::Shattered;
        assert_eq!(strength(&s), 0.0);
    }

    #[test]
    fn unit_card_terms() {
        use crate::battle::attributes::UnitCapabilities;
        let mut u = line(100);
        let (m0, s0) = (melee_strength(&u), missile_strength(&u));
        u.capabilities = UnitCapabilities::from_names(["square_formation", "plug_bayonets", "fire_and_advance"], ["canister"]);
        u.capabilities.firing_drill = 5; // rank_fire
        assert!((melee_strength(&u) - (m0 + 70.0)).abs() < 1e-2);
        assert!((missile_strength(&u) - (s0 + 20.0 + 70.0 + 110.0)).abs() < 1e-2);
        // Drill 1 (mass fire) and unknown names add nothing.
        u.capabilities = UnitCapabilities::from_names(["no_such_ability"], ["bullet"]);
        u.capabilities.firing_drill = 1;
        assert!((missile_strength(&u) - s0).abs() < 1e-2);
    }
}

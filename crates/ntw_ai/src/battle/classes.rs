//! The original's unit category and class enums and the battle AI's class matchups
//! (`analysis/ai/AI_RESEARCH.md` §3.3 "Class codes").
//!
//! CONFIRMED (round 4): a battle unit's type record (`unit+0x2C`) holds the **category** at
//! `+0x1C` and the **class** at `+0x20` (both compared by the script condition `0x00531E50`). The
//! `units` table strings become these codes through the exe's two compare chains, kept in one place:
//! [`ntw_sim::unit_kind`] (category `0x00EED2B0`, an unknown key artillery; class `0x00EED3E0`, an
//! unknown key 0). The script condition gives an empty class key 0x2E ([`class::NONE`]).

use ntw_sim::battle::model::LandUnit;

/// Category codes (`0x00EED2B0`, [`ntw_sim::unit_kind::Category`]).
pub mod category {
    use ntw_sim::unit_kind::Category;
    /// `cavalry`.
    pub const CAVALRY: u8 = Category::Cavalry as u8;
    /// `artillery` (and any unknown key).
    pub const ARTILLERY: u8 = Category::Artillery as u8;
    /// `infantry`.
    pub const INFANTRY: u8 = Category::Infantry as u8;
    /// `dragoons`.
    pub const DRAGOONS: u8 = Category::Dragoons as u8;
    /// `elephants`.
    pub const ELEPHANTS: u8 = Category::Elephants as u8;
    /// `cavalry_camels`.
    pub const CAMELS: u8 = Category::Camels as u8;
}

/// Class codes used by the battle AI.
pub mod class {
    /// `cavalry_heavy`.
    pub const CAVALRY_HEAVY: u8 = 4;
    /// `cavalry_lancers`.
    pub const CAVALRY_LANCERS: u8 = 6;
    /// `cavalry_light`.
    pub const CAVALRY_LIGHT: u8 = 7;
    /// `cavalry_standard`.
    pub const CAVALRY_STANDARD: u8 = 9;
    /// `dragoons`.
    pub const DRAGOONS: u8 = 10;
    /// `general`.
    pub const GENERAL: u8 = 12;
    /// `infantry_elite`.
    pub const INFANTRY_ELITE: u8 = 0xE;
    /// `infantry_light`.
    pub const INFANTRY_LIGHT: u8 = 0x11;
    /// `infantry_line`.
    pub const INFANTRY_LINE: u8 = 0x12;
    /// `infantry_militia`.
    pub const INFANTRY_MILITIA: u8 = 0x14;
    /// `infantry_skirmishers`.
    pub const INFANTRY_SKIRMISHERS: u8 = 0x16;
    /// No key (`0x00531E50` uses 0x2E for an empty string).
    pub const NONE: u8 = 0x2E;
}

/// Category code of a `units` category key ([`ntw_sim::unit_kind::category`]; unknown keys give 1).
pub fn category_code(key: &str) -> u8 {
    ntw_sim::unit_kind::category(key).code()
}

/// Class code of a `units` class key ([`ntw_sim::unit_kind::class_code`]: `0x00EED3E0`, CONFIRMED, an
/// unknown key 0); an empty key [`class::NONE`] as the script condition `0x00531E50`.
pub fn class_code(key: &str) -> u8 {
    if key.is_empty() { class::NONE } else { ntw_sim::unit_kind::class_code(key) }
}

/// `(category, class)` of a model unit. From its `units` keys when the battle set them; else a
/// PROVISIONAL guess from the model's flags (artillery weapon → artillery / artillery_foot,
/// cavalry → cavalry / cavalry_standard, otherwise infantry / infantry_line).
pub fn codes(u: &LandUnit) -> (u8, u8) {
    if !u.unit_category.is_empty() || !u.unit_class.is_empty() {
        return (category_code(&u.unit_category), class_code(&u.unit_class));
    }
    if u.missile.is_some_and(|w| w.is_artillery) {
        (category::ARTILLERY, 1)
    } else if u.is_cavalry {
        (category::CAVALRY, class::CAVALRY_STANDARD)
    } else {
        (category::INFANTRY, class::INFANTRY_LINE)
    }
}

/// `0x0055ABF0` (CONFIRMED): mounted = category cavalry or camels, or dragoons not using
/// ability 0xF (INFERRED: dismounted; the model has no abilities, so dragoons count as mounted).
pub fn is_mounted(cat: u8) -> bool {
    matches!(cat, category::CAVALRY | category::CAMELS | category::DRAGOONS)
}

/// True when `t` faces towards `from` (its facing vector · (from − t) > 0). INFERRED direction:
/// the only reading under which light cavalry's ×0.1 / ×1.5 rule avoids frontal charges.
pub fn faces(t: &LandUnit, from: (f32, f32)) -> bool {
    let d = (from.0 - t.position.0, from.1 - t.position.1);
    t.facing.cos() * d.0 + t.facing.sin() * d.1 > 0.0
}

/// The melee class matchup `0x007D23E0` (CONFIRMED parts; see the notes for what is left out).
/// `enemies_within_160m`: whether any enemy unit is within 160 m of the attacker (the general's
/// rule).
pub fn melee_class_factor(att: &LandUnit, tgt: &LandUnit, enemies_within_160m: bool) -> f32 {
    let (acat, acls) = codes(att);
    let (tcat, tcls) = codes(tgt);
    // The general's own unit (`0x0055AC40`; PROVISIONAL: read as class `general`) never seeks
    // melee: 0 with enemies within 160 m, else 0.01 (CONFIRMED values).
    if acls == class::GENERAL {
        return if enemies_within_160m { 0.0 } else { 0.01 };
    }
    // Artillery takes no melee objectives (category 1 → 0; `0x00532320` also excludes artillery).
    if acat == category::ARTILLERY {
        return 0.0;
    }
    let mut m = 1.0f32;
    // Camels against cavalry or dragoons: ×2 (×3 when the UNKNOWN state test `0x0055B200` holds).
    if acat == category::CAMELS && matches!(tcat, category::CAVALRY | category::DRAGOONS) {
        m = 2.0;
    }
    let facing_us = faces(tgt, att.position);
    match acls {
        class::CAVALRY_LANCERS => {
            if tcls == class::INFANTRY_LIGHT {
                m = 2.0;
            }
            if !facing_us {
                m *= 1.5;
            }
        }
        class::CAVALRY_LIGHT => {
            // (Skipped when the target has the UNKNOWN state flag `+0x278D`.)
            if facing_us {
                m *= 0.1;
            } else if tcat == category::ARTILLERY {
                m *= 1.5;
            }
        }
        class::CAVALRY_STANDARD | class::DRAGOONS => {
            if facing_us {
                m *= 0.1;
            }
        }
        class::INFANTRY_LIGHT => {
            // 0.1 while ability 4 is active (UNKNOWN which; the model has no abilities).
            m = 0.5;
        }
        // 0 while `0x0057A150(1)` holds (INFERRED: the unit can still fire).
        class::INFANTRY_SKIRMISHERS if att.can_shoot() => {
            m = 0.0;
        }
        _ => {}
    }
    m
}

/// The missile class factor `0x007D2CF0` (CONFIRMED parts). `units_near_target`: how many units
/// are within 160 m of the target (`0x006C9E40`; INFERRED: of either side).
pub fn missile_class_factor(att: &LandUnit, tgt: &LandUnit, units_near_target: usize) -> f32 {
    let (acat, acls) = codes(att);
    let (_, tcls) = codes(tgt);
    if acat == category::ARTILLERY {
        // Guns: × the number of units around the target, 1..10 (the ×10 case and the 160 m rule
        // against mounted targets depend on UNKNOWN flags and are left out).
        return (units_near_target as f32).clamp(1.0, 10.0);
    }
    let line_target = matches!(tcls, class::INFANTRY_ELITE | class::INFANTRY_LINE | class::INFANTRY_MILITIA);
    match acls {
        class::CAVALRY_LIGHT => {
            // Shoots only while not facing against the target's facing.
            let dot = att.facing.cos() * tgt.facing.cos() + att.facing.sin() * tgt.facing.sin();
            if dot < 0.0 { 0.0 } else { 1.0 }
        }
        class::INFANTRY_LINE if line_target => 2.0,
        class::INFANTRY_SKIRMISHERS if !is_mounted(acat) => {
            if line_target { 4.0 } else { 2.0 }
        }
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_match_the_exe() {
        assert_eq!(class_code("infantry_line"), 0x12);
        assert_eq!(class_code("infantry_skirmishers"), 0x16);
        assert_eq!(class_code("cavalry_lancers"), 6);
        assert_eq!(class_code("naval_transport"), 0x2D);
        assert_eq!(class_code(""), 0x2E);
        assert_eq!(category_code("cavalry"), 0);
        assert_eq!(category_code("dragoons"), 3);
        assert_eq!(category_code("whatever"), 1);
        assert_eq!(class_code("samurai_archers"), 0, "an unknown class is 0 (0x00EED3E0's last branch)");
    }

    fn unit(cat: &str, cls: &str, pos: (f32, f32), facing: f32) -> LandUnit {
        let mut u = LandUnit::new(1, 0, 100, pos);
        u.unit_category = cat.into();
        u.unit_class = cls.into();
        u.facing = facing;
        u
    }

    #[test]
    fn cavalry_avoids_frontal_attacks() {
        let cav = unit("cavalry", "cavalry_standard", (0.0, 0.0), 0.0);
        // Target at +x facing back towards us (-x): frontal → ×0.1.
        let front = unit("infantry", "infantry_line", (50.0, 0.0), std::f32::consts::PI);
        assert!((melee_class_factor(&cav, &front, true) - 0.1).abs() < 1e-6);
        // Facing away: rear → ×1.
        let rear = unit("infantry", "infantry_line", (50.0, 0.0), 0.0);
        assert_eq!(melee_class_factor(&cav, &rear, true), 1.0);
        // Lancers: light infantry from behind ×2 ×1.5.
        let lancer = unit("cavalry", "cavalry_lancers", (0.0, 0.0), 0.0);
        let light = unit("infantry", "infantry_light", (50.0, 0.0), 0.0);
        assert_eq!(melee_class_factor(&lancer, &light, true), 3.0);
        // Artillery never melees.
        let gun = unit("artillery", "artillery_foot", (0.0, 0.0), 0.0);
        assert_eq!(melee_class_factor(&gun, &rear, true), 0.0);
    }

    #[test]
    fn missile_factors() {
        let line = unit("infantry", "infantry_line", (0.0, 0.0), 0.0);
        let other_line = unit("infantry", "infantry_line", (50.0, 0.0), 0.0);
        let cav = unit("cavalry", "cavalry_standard", (50.0, 0.0), 0.0);
        assert_eq!(missile_class_factor(&line, &other_line, 0), 2.0);
        assert_eq!(missile_class_factor(&line, &cav, 0), 1.0);
        let skirm = unit("infantry", "infantry_skirmishers", (0.0, 0.0), 0.0);
        assert_eq!(missile_class_factor(&skirm, &other_line, 0), 4.0);
        let gun = unit("artillery", "artillery_foot", (0.0, 0.0), 0.0);
        assert_eq!(missile_class_factor(&gun, &cav, 0), 1.0);
        assert_eq!(missile_class_factor(&gun, &cav, 14), 10.0);
    }
}

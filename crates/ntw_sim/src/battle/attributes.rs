//! The unit attribute flags: the boolean columns of `unit_stats_land` that the battle code reads.
//!
//! CONFIRMED layout (BATTLE_FIDELITY.md §5): the record constructor `0x00E8CFF0` copies a stat block
//! from the BUILDER into `LAND_UNIT_RECORD+0x138` (`0x00E8F1D0`), and the battle unit holds the
//! same block 0x20 higher (`unit+0x158`, W1 §12.9). The attribute bytes land at:
//!
//! | record | battle unit | column | read by | name (INFERRED from the units that set it) |
//! |---|---|---|---|---|
//! | `+0x185` | `+0x1A5` | 53 | may skirmish (`0x0053F9C0`, the skirmish default `0x005357B0`) | skirmishers (light infantry, chasseurs, camel gunners, guerrillas) |
//! | `+0x188` | `+0x1A8` | 56 | melee strength +30 | marksmen (rifles, jägers, guerrillas) |
//! | `+0x18F` | `+0x1AF` | 63 | melee strength +60 | (only the Austrian Windbüchse jägers) |
//! | `+0x190` | `+0x1B0` | 64 | missile strength +70 | (only the Austrian Windbüchse jägers) |
//! | `+0x169` | `+0x189` | 71 | rally ½ instead of ⅓, shock never persists, melee +50 | steadfast (heavy cavalry, guards …) |
//! | `+0x16A` | `+0x18A` | 72 | morale: Eager → Impetuous allowed | impetuous (British heavy cavalry) |
//! | `+0x16B` | `+0x18B` | 73 | frightens enemy cavalry within 100 m, melee +50 | frightens horses (camels) |
//! | `+0x16C` | `+0x18C` | 74 | frightens enemies within 100 m, melee +70 | frightens enemy (Old Guard) |
//! | `+0x16D` | `+0x18D` | 75 | inspires friends within 100 m, melee +70 | inspires (guards, guard artillery) |
//! | `+0x16E` | `+0x18E` | 76 | fatigue −1 per tick, melee +30 | good stamina (lancers, hussars) |
//! | `+0x16F` | `+0x18F` | 78 | no battle climate term `+0x2C` in fatigue | heat resistant (Ottoman, Mameluke) |
//! | `+0x170` | `+0x190` | 79 | no battle climate term `+0x30` in fatigue | cold resistant (Russian) |
//!
//! The offsets and columns are CONFIRMED (copy code); the readers are CONFIRMED (fear `0x0053B970`,
//! rally `0x0055C500`, shock `0x0053E980`, fatigue `0x00670F40`, strength `0x00757120` /
//! `0x007575A0`). The names are INFERRED from which units set each column (the data has no names;
//! see `cargo run -p ntw_data --example attr_flag_probe`).

/// The attribute flags a battle unit carries (all `false` by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct UnitAttributes {
    /// Column 53 (unit `+0x1A5`): the unit may skirmish (`0x0053F9C0` returns it unless the unit is
    /// dismounted).
    pub skirmisher: bool,
    /// Column 56 (record `+0x188`): melee strength +30.
    pub marksmen: bool,
    /// Column 63 (record `+0x18F`): melee strength +60.
    pub col63: bool,
    /// Column 64 (record `+0x190`): missile strength +70.
    pub col64: bool,
    /// Column 71 (unit `+0x189`): easier rally, no lasting shock, melee strength +50.
    pub steadfast: bool,
    /// Column 72 (unit `+0x18A`): the morale state may rise to Impetuous.
    pub impetuous: bool,
    /// Column 73 (unit `+0x18B`): frightens enemy horses within 100 m; melee strength +50.
    pub frightens_horses: bool,
    /// Column 74 (unit `+0x18C`): frightens enemies within 100 m; melee strength +70.
    pub frightens_enemy: bool,
    /// Column 75 (unit `+0x18D`): inspires friends within 100 m; melee strength +70.
    pub inspires: bool,
    /// Column 76 (unit `+0x18E`): fatigue −1 per tick; melee strength +30.
    pub good_stamina: bool,
    /// Column 78 (unit `+0x18F`): exempt from the battle climate term `+0x2C`.
    pub climate_exempt_2c: bool,
    /// Column 79 (unit `+0x190`): exempt from the battle climate term `+0x30`.
    pub climate_exempt_30: bool,
}

/// Special abilities a unit card lists (`unit_capabilities/special_ability`), by the exe's enum
/// (`0x0057C950`, name table `0x0131C3A0`, CONFIRMED values) — the ones the strength code reads.
pub mod ability {
    /// `square_formation`.
    pub const SQUARE_FORMATION: u8 = 1;
    /// `fire_and_advance`.
    pub const FIRE_AND_ADVANCE: u8 = 7;
    /// `plug_bayonets`.
    pub const PLUG_BAYONETS: u8 = 8;
}

/// Shot types a unit card lists (`unit_capabilities/shot_type`), by the exe's enum (`0x00F59030`,
/// name table `0x0145C1B8`, CONFIRMED values): 0 round_shot, 1 explosive_shell, 2 percussive_shell,
/// 3 canister, 4 shrapnel, 5 carcass, 6 quicklime, 7 chain, 8 grape, 9 hot_shot, 10 rocket, …
pub const SHOT_TYPE_NAMES: [&str; 26] = [
    "round_shot", "explosive_shell", "percussive_shell", "canister", "shrapnel", "carcass", "quicklime", "chain",
    "grape", "hot_shot", "rocket", "fragment", "bullet", "air_pellet", "fougasse", "arrow", "javelin", "throwing_axe",
    "chakkar_ring", "grenade", "improved_grenade", "grappling_hook", "rifled_naval_cannon", "improved_grape",
    "improved_fougasse", "uniform_shot",
];

/// The shot type enum value of a `projectiles.shot_type` name ([`SHOT_TYPE_NAMES`], case
/// ignored), `None` for a name the enum does not have. The one shot-type lookup: the unit card's
/// shot list, `change_shot_type`, the battle setup and the gun's shot order (`ntw_data`) use it.
pub fn shot_type_value(name: &str) -> Option<u8> {
    SHOT_TYPE_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i as u8)
}

/// Special-ability names by enum value (`0x0131C3A0`, CONFIRMED pairs; value 0x16 = `none`).
pub const ABILITY_NAMES: [(&str, u8); 22] = [
    ("pike_square_formation", 0), ("pike_wall_formation", 6), ("square_formation", 1), ("wedge_formation", 2),
    ("diamond_formation", 3), ("light_infantry_behaviour", 4), ("loose_formation", 5), ("fire_and_advance", 7),
    ("plug_bayonets", 8), ("fougasse_basic", 9), ("fougasse_improved", 10), ("wooden_stakes", 11),
    ("chevaux_de_frise", 12), ("gabionade", 14), ("earthworks", 13), ("dismount", 15), ("unlimber", 16),
    ("rally", 17), ("inspire_unit", 18), ("artillery_accuracy_boost", 19), ("artillery_rof_boost", 20),
    ("column_formation", 21),
];

/// The unit card's capability block (card `+0x78`, filled by the battle-file parser `0x0050CAE0`
/// from `unit_capabilities`; CONFIRMED layout as the strength code reads it): firing drill `[0]`
/// (`fire_volley` 0, `mass_fire` 1, `platoon_fire_dispersed` 2, `platoon_fire_grouped` 3,
/// `platoon_fire_column` 4, `rank_fire` 5; `0x0057C8C0`), the special abilities and the shot types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct UnitCapabilities {
    /// Firing drill enum value (0 = `fire_volley`, the default).
    pub firing_drill: u8,
    /// Bit `n` set = ability `n` listed.
    pub abilities: u32,
    /// Bit `n` set = shot type `n` listed.
    pub shot_types: u32,
}

impl UnitCapabilities {
    /// Builds the block from the battle file's names (unknown names are ignored).
    pub fn from_names<'a>(abilities: impl IntoIterator<Item = &'a str>, shot_types: impl IntoIterator<Item = &'a str>) -> Self {
        let mut c = UnitCapabilities::default();
        for a in abilities {
            // Drill names (the DB lists them as abilities) set the card's drill; PROVISIONAL: with
            // several, the highest enum value wins.
            if let Some(d) = DRILL_NAMES.iter().position(|n| n.eq_ignore_ascii_case(a)) {
                c.firing_drill = c.firing_drill.max(d as u8);
                continue;
            }
            if let Some((_, v)) = ABILITY_NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(a)) {
                c.abilities |= 1 << v;
            }
        }
        for s in shot_types {
            if let Some(v) = shot_type_value(s) {
                c.shot_types |= 1 << v;
            }
        }
        c
    }

    /// True if ability `a` is listed.
    pub fn has_ability(&self, a: u8) -> bool {
        self.abilities & (1 << a) != 0
    }

    /// True if shot type `s` is listed.
    pub fn has_shot_type(&self, s: u8) -> bool {
        self.shot_types & (1 << s) != 0
    }
}

/// Firing-drill names by enum value (`0x0057C8C0`, CONFIRMED).
pub const DRILL_NAMES: [&str; 6] =
    ["fire_volley", "mass_fire", "platoon_fire_dispersed", "platoon_fire_grouped", "platoon_fire_column", "rank_fire"];

#[cfg(test)]
mod tests {
    use super::*;

    /// The one shot-type lookup (`ntw_data`'s gun shot order, the battle setup, the unit card and
    /// `change_shot_type` all call it; regression: three copies): the exe's enum values, case
    /// ignored, `None` for a name the enum lacks.
    #[test]
    fn shot_type_value_is_the_exes_enum() {
        assert_eq!(shot_type_value("round_shot"), Some(0));
        assert_eq!(shot_type_value("Canister"), Some(3));
        assert_eq!(shot_type_value("uniform_shot"), Some(25));
        assert_eq!(shot_type_value("no_such_shot"), None);
    }
}

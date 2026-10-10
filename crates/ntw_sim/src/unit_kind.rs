//! Unit categories and classes: the one place their keys become the original's codes
//! (MODDING_AUDIT.md §1.3).
//!
//! The original compiles both lists into the exe as compare chains over the `units` table's key
//! strings (exact, case-sensitive UTF-16 compares, `0x004F1F30`):
//! - category, `UNIT_RECORD` +0x1C (`0x00EED2B0`, CONFIRMED): [`ORIGINAL_CATEGORIES`]; **any other key
//!   is artillery** (1: the chain's last branch);
//! - class, `UNIT_RECORD` +0x20 (`0x00EED3E0`, CONFIRMED): [`ORIGINAL_CLASSES`] in order (0 … 0x2D);
//!   any other key is 0 (`artillery_fixed`).
//!
//! Every rule that reads a category or class code calls [`category`] / [`class_code`]. The game data
//! carries the key lists (`unit_category`, `unit_class`): a key the data uses but the original's
//! chain does not know gets the exe's fallback, and the DB loader reports it once
//! ([`unknown_categories`]). The saved regiment-name lists (`ntw_campaign::regiments`) are keyed by
//! these class codes too (`0x00880670`).

/// A unit category: the original's code (`UNIT_RECORD` +0x1C) and the behaviour every rule keys on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// 0 `cavalry`.
    Cavalry,
    /// 1 `artillery`; also every key the original's chain does not know.
    Artillery,
    /// 2 `infantry`.
    Infantry,
    /// 3 `dragoons`.
    Dragoons,
    /// 4 `elephants`.
    Elephants,
    /// 5 `cavalry_camels`.
    Camels,
    /// 6 `naval_line_of_battle`.
    NavalLineOfBattle,
    /// 7 `naval_frigate`.
    NavalFrigate,
    /// 8 `naval_galley`.
    NavalGalley,
    /// 9 `naval_specialist`.
    NavalSpecialist,
    /// 10 `naval_auxiliary`.
    NavalAuxiliary,
    /// 11 `naval_merchant`.
    NavalMerchant,
    /// 12 `naval_invasion_fleet`.
    NavalInvasionFleet,
}

/// The original's category keys and codes (`0x00EED2B0`, CONFIRMED).
pub const ORIGINAL_CATEGORIES: [(&str, Category); 13] = [
    ("cavalry", Category::Cavalry),
    ("artillery", Category::Artillery),
    ("infantry", Category::Infantry),
    ("dragoons", Category::Dragoons),
    ("elephants", Category::Elephants),
    ("cavalry_camels", Category::Camels),
    ("naval_line_of_battle", Category::NavalLineOfBattle),
    ("naval_frigate", Category::NavalFrigate),
    ("naval_galley", Category::NavalGalley),
    ("naval_specialist", Category::NavalSpecialist),
    ("naval_auxiliary", Category::NavalAuxiliary),
    ("naval_merchant", Category::NavalMerchant),
    ("naval_invasion_fleet", Category::NavalInvasionFleet),
];

/// The category a key falls back to when the original's chain does not know it (`0x00EED2B0`'s last
/// branch, CONFIRMED).
pub const FALLBACK_CATEGORY: Category = Category::Artillery;

/// The original's class keys in code order (`0x00EED3E0`, CONFIRMED: `artillery_fixed` 0 …
/// `infantry_line` 0x12 … `naval_transport` 0x2D). The vanilla `unit_class` table holds the first 45
/// in the same order (not `naval_transport`).
pub const ORIGINAL_CLASSES: [&str; 46] = [
    "artillery_fixed",
    "artillery_foot",
    "artillery_horse",
    "cavalry_camels",
    "cavalry_heavy",
    "cavalry_irregular",
    "cavalry_lancers",
    "cavalry_light",
    "cavalry_missile",
    "cavalry_standard",
    "dragoons",
    "elephants",
    "general",
    "infantry_berserker",
    "infantry_elite",
    "infantry_grenadiers",
    "infantry_irregulars",
    "infantry_light",
    "infantry_line",
    "infantry_melee",
    "infantry_militia",
    "infantry_mob",
    "infantry_skirmishers",
    "naval_admiral",
    "naval_bomb_ketch",
    "naval_brig",
    "naval_dhow",
    "naval_fifth_rate",
    "naval_first_rate",
    "naval_fourth_rate",
    "naval_galleon",
    "naval_heavy_galley",
    "naval_indiaman",
    "naval_light_galley",
    "naval_lugger",
    "naval_medium_galley",
    "naval_over_first_rate",
    "naval_razee",
    "naval_rocket_ship",
    "naval_second_rate",
    "naval_sixth_rate",
    "naval_sloop",
    "naval_steam_ship",
    "naval_third_rate",
    "naval_xebec",
    "naval_transport",
];

impl Category {
    /// The original's code (`UNIT_RECORD` +0x1C).
    pub fn code(self) -> u8 {
        self as u8
    }

    /// One of the seven naval categories (codes 6..12).
    pub fn is_naval(self) -> bool {
        self.code() >= Category::NavalLineOfBattle.code()
    }

    /// The category of a known key, `None` for a key the original's chain does not know.
    pub fn of_key(key: &str) -> Option<Category> {
        ORIGINAL_CATEGORIES.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
    }
}

/// The category of a `units` category key: the original's chain, an unknown key artillery (see the
/// module docs; the DB loader logs such keys once).
pub fn category(key: &str) -> Category {
    Category::of_key(key).unwrap_or(FALLBACK_CATEGORY)
}

/// The class code of a `units` class key (`0x00EED3E0`): its place in [`ORIGINAL_CLASSES`], an
/// unknown key 0.
pub fn class_code(key: &str) -> u8 {
    ORIGINAL_CLASSES.iter().position(|c| *c == key).map_or(0, |i| i as u8)
}

/// The category keys of `keys` that the original's chain does not know (they count as artillery),
/// each once, in first-seen order: what the DB loader reports.
pub fn unknown_categories<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for k in keys {
        if Category::of_key(k).is_none() && !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codes are the exe's (`0x00EED2B0` / `0x00EED3E0` listings): the enum order, the fallbacks,
    /// exact case.
    #[test]
    fn keys_map_to_the_original_codes() {
        for (i, (key, c)) in ORIGINAL_CATEGORIES.iter().enumerate() {
            assert_eq!(c.code() as usize, i, "{key}");
            assert_eq!(category(key), *c);
        }
        assert_eq!(category("ashigaru"), Category::Artillery, "unknown: artillery, as the exe");
        assert_eq!(category("Infantry"), Category::Artillery, "case-sensitive");
        assert!(Category::NavalFrigate.is_naval() && !Category::Camels.is_naval());
        assert_eq!(class_code("artillery_fixed"), 0);
        assert_eq!(class_code("infantry_line"), 0x12);
        assert_eq!(class_code("naval_transport"), 0x2D);
        assert_eq!(class_code("samurai_archers"), 0, "unknown: 0, as the exe");
        assert_eq!(unknown_categories(["infantry", "ashigaru", "cavalry", "ashigaru", "samurai"]), ["ashigaru", "samurai"]);
    }
}

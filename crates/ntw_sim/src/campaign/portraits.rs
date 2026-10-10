//! Portraits of new characters: the portrait allocator (`CAMPAIGN_MODEL` `PORTRAIT_ALLOCATOR`,
//! campaign model +0xF94) and how a character created in play gets his `PORTRAIT_DETAILS`
//! (CHARACTERS_FIDELITY.md §14; specs in our words, all CONFIRMED in the exe unless tagged).
//!
//! - The allocator holds, per culture, the folder culture of each agent type (`CULTURE_PATHS`: the
//!   culture's own when its folders have pictures, else the culture's fallback) and, per agent
//!   type, four shuffled decks of picture numbers (`PORTRAIT_CATEGORIES`), in the order of the
//!   portrait types (`0x009CBF40`): Info young, Info old, Cards young, Cards old. Only the two Info
//!   decks are drawn from; the number drawn names both the card and the info picture.
//! - A deck ([`PortraitDeck`], `0x009CA8C0`) hands out its numbers in order; once all are used it
//!   starts again after a reshuffle (`0x00A1E910`: one step of the deck's own LCG, then the
//!   `random_shuffle` the name decks use, [`super::names::shuffle_in_place`]). Each deck has its
//!   own seed; the save does not store it: loading a deck seeds it with the high 16 bits of one
//!   step of the campaign RNG (world +0xFB8) per deck in file order (`0x009942D0`, the chain
//!   pointer passed from `0x00872550` through `0x00999870`), done by the campaign source.
//! - A new character (`0x00992E60`, the details constructor; generic characters `0x0098F250`,
//!   historical `0x0098F880`, a promoted unit's officer `0x00990EF0`) starts with picture number -1
//!   and gets a portrait from `0x009CB3C0` → `0x00A05440` for his culture (his faction's), agent
//!   type and age:
//!   - the agent type's portrait folder is `agents` #6, or the key when #6 is empty (General →
//!     `General`, admiral → `General`, the missionaries and the gentleman → `minister`, guerilla →
//!     `guerrilla`, Eastern_Scholar → `scholar`; [`super::rules::CampaignRules::agent_portrait_folders`]);
//!   - while his number is -1 one is drawn: from the category named by that folder if the culture
//!     has one, else from his agent type's own category (so admirals draw from the General decks
//!     and missionaries from the minister decks); the Info old deck when he is 45 or older, else
//!     Info young; an empty deck leaves -1 and no pictures (colonels and captains have none);
//!   - the pictures are `ui/portraits/<CULTURE_PATHS[agent type]>/Cards/<folder>/<young|old>/NNN.tga`
//!     and `.../Info/<folder>/<young|old>/NNN.jpg`, the folder lower-cased (`0x004F4850`), old at
//!     45 or older, NNN the number (`%03d`);
//!   - a character whose agent type changes (a promotion, `0x00A1A300`) is resolved again with his
//!     new type and current age: a number he already has is kept (only the pictures change).
//! - Historical characters pass their `historical_characters` #7 to the constructor through
//!   `0x00A06430`: `guerrilla#<region>#<NNN>` gives the `guerilla` agent type's portrait with number
//!   NNN; anything else (every non-Peninsular row) an empty string, so they draw like a generic
//!   character. A string that is all digits fixes the number; any other non-empty string becomes
//!   the custom picture name (`PORTRAIT_DETAILS` #1, `0x009CBD60`); neither occurs in the shipped
//!   data for created characters.

use super::ids::CharacterId;
use super::names::{lcg_step, shuffle_in_place};
use super::world::CampaignModel;

/// Portrait types, in deck order (`0x009CBF40`'s switch): Info young, Info old, Cards young, Cards old.
pub const PORTRAIT_TYPES: usize = 4;
/// The age from which a character has the old pictures (`0x00A05440`: age > 44, CONFIRMED).
pub const OLD_AGE: i32 = 45;

/// One deck of picture numbers (`PORTRAIT_ALLOCATION`: count, next position, order; the seed is
/// not saved).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortraitDeck {
    /// How many numbers the deck hands out (#0).
    pub count: u32,
    /// The position of the next number in `order` (#1).
    pub cursor: u32,
    /// The shuffled numbers (#2).
    pub order: Vec<u32>,
    /// The deck's own LCG state (deck +0x18; set when the deck is loaded or built).
    pub seed: u32,
}

impl PortraitDeck {
    /// The next number (`0x009CA8C0`), or `None` for an empty deck. Past the last one the deck
    /// starts again from a reshuffle.
    pub fn draw(&mut self) -> Option<u32> {
        if self.count == 0 {
            return None;
        }
        if self.cursor >= self.count {
            self.cursor = 0;
            self.reshuffle();
        }
        let n = self.order.get(self.cursor as usize).copied();
        self.cursor += 1;
        n
    }

    /// `0x00A1E910`: one step of the deck's LCG, then the shuffle of the numbers in place.
    pub fn reshuffle(&mut self) {
        self.seed = lcg_step(self.seed);
        shuffle_in_place(&mut self.order, self.seed);
    }
}

/// One agent type's decks (`PORTRAIT_CATEGORIES` item): its key and its decks in
/// [`PORTRAIT_TYPES`] order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortraitCategory {
    /// The agent type (`PORTRAITS` #0).
    pub key: String,
    /// Info young, Info old, Cards young, Cards old.
    pub decks: Vec<PortraitDeck>,
}

/// One culture's portraits (`PORTRAIT_ALLOCATOR` item).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CulturePortraits {
    /// The culture key (#0).
    pub culture: String,
    /// `CULTURE_PATHS`: agent type → the culture folder its pictures are in.
    pub paths: Vec<(String, String)>,
    /// `PORTRAIT_CATEGORIES`, in save order.
    pub categories: Vec<PortraitCategory>,
}

impl CulturePortraits {
    fn path(&self, agent: &str) -> Option<&str> {
        self.paths.iter().find(|(a, _)| a == agent).map(|(_, p)| p.as_str())
    }

    fn category_mut(&mut self, key: &str) -> Option<&mut PortraitCategory> {
        self.categories.iter_mut().find(|c| c.key == key)
    }
}

/// Picture paths for a number: (card, info) (`0x009CBF40` types 2/3 and 0/1). The number is
/// signed, as the exe's `%03d` prints it (-5 → "-05").
pub fn picture_paths(culture_path: &str, folder: &str, old: bool, index: i32) -> (String, String) {
    let folder = folder.to_ascii_lowercase();
    let age = if old { "old" } else { "young" };
    (
        format!("ui/portraits/{culture_path}/Cards/{folder}/{age}/{index:03}.tga"),
        format!("ui/portraits/{culture_path}/Info/{folder}/{age}/{index:03}.jpg"),
    )
}

/// What a historical character's `historical_characters` #7 gives the details constructor
/// (`0x00A06430`): `guerrilla#<region>#<NNN>` → the `guerilla` agent type and number NNN, or 0
/// without a second '#' (an unparsable number draws one); anything else → nothing (`None`: his
/// own type, drawn).
pub fn historical_portrait(note: &str) -> Option<(&'static str, Option<i32>)> {
    let rest = note.strip_prefix("guerrilla#")?;
    // The constructor builds "guerrilla" + the text after the next '#' ("guerrilla" alone when
    // there is none) and parses what follows the 9 letters as the number.
    let number = parse_whole_int(rest.split_once('#').map_or("", |(_, n)| n));
    Some(("guerilla", number))
}

/// `0x004F3720`'s parse: an optional '-', then digits up to the end; an empty string (or a lone
/// '-') is 0. The value wraps as the exe's u32 arithmetic does.
fn parse_whole_int(s: &str) -> Option<i32> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let v = digits.bytes().fold(0u32, |v, b| v.wrapping_mul(10).wrapping_add(u32::from(b - b'0')));
    Some(if s.starts_with('-') { v.wrapping_neg() } else { v } as i32)
}

impl CampaignModel {
    /// The age of a new character (`0x00A05740`: 21 + min(19, ⌊next16 × 20 / 65535⌋) on the
    /// campaign RNG, CONFIRMED): one draw.
    pub fn draw_new_character_age(&mut self) -> i32 {
        let draw = self.rng.next16();
        21 + (draw * 20 / 65535).min(19) as i32
    }

    /// Gives character `c` his portrait as agent type `agent` (see the module docs), in
    /// `0x00A05440`'s order: a custom picture name (`PORTRAIT_DETAILS` #1) keeps everything as it
    /// is; else a number is drawn while he has none (-1), then the card and info pictures are built
    /// from his number. A culture without a set in the allocator changes nothing (`0x009CB3C0`
    /// checks it) and an empty deck leaves -1 and no pictures. Not reached by the exe with its own
    /// data: an agent type without a `CULTURE_PATHS` folder (`0x009CBF40` does not check its
    /// lookup; the builder gives every agent type one). Ours keeps the number drawn first (the exe
    /// stores it before building the paths) and leaves the pictures as they were (empty for a new
    /// character, the old agent type's on a promotion). The app logs every such
    /// gap ([`Self::portrait_problem`]).
    pub fn assign_portrait(&mut self, c: CharacterId, agent: &str) {
        let Some(culture) = self.portrait_culture(c) else { return };
        // Both early returns change nothing, so finding the set before the custom-name check is the exe's result.
        let Some(si) = self.world.portraits.iter().position(|s| s.culture == culture) else { return };
        let folder = self.rules.agent_portrait_folders.get(agent).map_or(agent, String::as_str);
        let year = self.calendar.date.year as i32;
        let Some(d) = self.world.character_details.get_mut(&c) else { return };
        if !d.portrait.alternative.is_empty() {
            return;
        }
        let age = d.birth.map_or(0, |b| year - b.year as i32);
        let old = age >= OLD_AGE;
        let set = &mut self.world.portraits[si];
        if d.portrait.index == -1 {
            let category = if set.categories.iter().any(|k| k.key == folder) { folder } else { agent };
            let drawn = set.category_mut(category).and_then(|k| k.decks.get_mut(usize::from(old))).and_then(PortraitDeck::draw);
            match drawn {
                Some(n) => d.portrait.index = n as i32,
                None => return,
            }
        }
        let Some(culture_path) = set.path(agent) else { return };
        (d.portrait.card, d.portrait.info) = picture_paths(culture_path, folder, old, d.portrait.index);
    }

    /// A historical character's portrait from his `historical_characters` #7 (see the module
    /// docs): the guerrilla leaders' fixed numbers, everyone else drawn as agent type `agent`.
    /// The number is the details constructor's (`0x00992E60` stores it before any resolve, so a
    /// custom picture name keeps it too); a fixed -1 is drawn over, as any -1.
    pub fn assign_historical_portrait(&mut self, c: CharacterId, agent: &str, note: &str) {
        match historical_portrait(note) {
            Some((guerilla, number)) => {
                if let (Some(n), Some(d)) = (number, self.world.character_details.get_mut(&c)) {
                    d.portrait.index = n;
                }
                self.assign_portrait(c, guerilla);
            }
            None => self.assign_portrait(c, agent),
        }
    }

    /// The culture whose portraits character `c` takes: his faction's.
    fn portrait_culture(&self, c: CharacterId) -> Option<&str> {
        let faction = self.world.characters.get(&c)?.faction;
        let fkey = self.world.factions.get(&faction)?.key.as_str();
        Some(self.rules.characters.culture(fkey))
    }

    /// The agent type character `c`'s portrait resolves as: `guerilla` for a historical guerrilla
    /// leader (his #7, [`historical_portrait`]), else his own agent type.
    pub fn portrait_agent(&self, c: CharacterId) -> &'static str {
        let key = self.world.character_details.get(&c).and_then(|d| d.historical_key.as_deref());
        let row = key.and_then(|k| self.rules.historical.iter().find(|r| r.key == k));
        match row.and_then(|r| historical_portrait(&r.note)) {
            Some((guerilla, _)) => guerilla,
            None => self.world.characters.get(&c).map_or("", |ch| ch.kind.esf_name()),
        }
    }

    /// Why character `c` (portrait agent type `agent`, [`Self::portrait_agent`]) shows no
    /// portrait, or one that is wrong, for the app's log (the model has no logger); `None` when his
    /// card picture is set from a number a deck can hand out.
    pub fn portrait_problem(&self, c: CharacterId, agent: &str) -> Option<&'static str> {
        let Some(d) = self.world.character_details.get(&c) else { return Some("he has no character details; his card shows no portrait") };
        let p = &d.portrait;
        let set = || self.portrait_culture(c).and_then(|k| self.world.portraits.iter().find(|s| s.culture == k));
        if !p.card.is_empty() {
            if !p.alternative.is_empty() {
                return None;
            }
            // `%03d` of a number below -1 (a modded `historical_characters` #7): the exe builds
            // that path too, and no picture has such a name. -1 is the constructor's "none yet"
            // (a start card may carry it), never formatted: a resolve draws over it.
            if p.index < -1 {
                return Some("his portrait number is negative, so his picture path names no file");
            }
            // A resolve as an agent type without a `CULTURE_PATHS` folder (a promotion, modded
            // data) left his old pictures in place. At -1 nothing was ever resolved: the card is
            // the one the start position gave him, not a previous resolve's.
            return set().is_some_and(|s| s.path(agent).is_none()).then_some(if p.index == -1 {
                "his culture has no portrait folder for his agent type, so his card keeps the picture the start position gave it"
            } else {
                "his culture has no portrait folder for his agent type, so his card keeps his previous picture"
            });
        }
        if !p.alternative.is_empty() {
            return Some("his custom picture name has no card picture; his card shows no portrait");
        }
        if self.world.portraits.is_empty() {
            return Some("the campaign has no portrait allocator; his card shows no portrait");
        }
        Some(match set() {
            None => "his culture has no portrait set; his card shows no portrait",
            Some(s) if s.path(agent).is_none() => "his culture has no portrait folder for his agent type; his card shows no portrait",
            Some(_) if p.index == -1 => "his portrait deck has no pictures; his card shows no portrait",
            Some(_) => "his portrait number was never resolved to pictures; his card shows no portrait",
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::calendar::{Calendar, Date};
    use crate::campaign::details::CharacterDetails;
    use crate::campaign::ids::FactionId;
    use crate::campaign::world::{Character, CharacterKind, Faction, GovernmentType, World};
    use crate::rng::CaRng;

    fn deck(count: u32, order: Vec<u32>) -> PortraitDeck {
        PortraitDeck { count, cursor: 0, order, seed: 7 }
    }

    /// A model with one faction of culture `european` and one character aged `age`.
    fn model(age: i32) -> CampaignModel {
        let mut w = World::default();
        let faction = Faction {
            id: FactionId(1),
            key: "france".into(),
            treasury: 0,
            government: GovernmentType::AbsoluteMonarchy,
            government_key: String::new(),
            tax_lower: String::new(),
            tax_upper: String::new(),
            diplomacy: BTreeMap::new(),
        };
        w.factions.insert(FactionId(1), faction);
        let general = Character {
            id: CharacterId(5),
            faction: FactionId(1),
            kind: CharacterKind::General,
            position: Default::default(),
            movement_points: 0,
            max_movement_points: 0,
            base_movement_points: 0,
            garrisoned_in: None,
        };
        w.characters.insert(CharacterId(5), general);
        let date = Date { year: 1805, season: 0, month: 4, half: 0 };
        w.character_details.insert(CharacterId(5), CharacterDetails { birth: Some(Date { year: (1805 - age) as u32, ..date }), ..Default::default() });
        let set = CulturePortraits {
            culture: "european".into(),
            paths: vec![("General".into(), "european".into()), ("admiral".into(), "european".into()), ("colonel".into(), "european".into())],
            categories: vec![
                PortraitCategory { key: "General".into(), decks: vec![deck(3, vec![2, 0, 1]), deck(2, vec![1, 0]), deck(3, vec![0, 1, 2]), deck(2, vec![0, 1])] },
                PortraitCategory { key: "admiral".into(), decks: vec![deck(3, vec![1, 2, 0]), deck(2, vec![0, 1]), deck(3, vec![0, 1, 2]), deck(2, vec![0, 1])] },
                PortraitCategory { key: "colonel".into(), decks: vec![PortraitDeck::default(); PORTRAIT_TYPES] },
            ],
        };
        w.portraits = vec![set];
        let mut m = CampaignModel::new(Calendar::new(date, 2), CaRng::new(1), w);
        let rules = std::sync::Arc::make_mut(&mut m.rules);
        rules.characters.faction_subculture.insert("france".into(), "sc_european".into());
        rules.characters.subculture_culture.insert("sc_european".into(), "european".into());
        rules.agent_portrait_folders.insert("admiral".into(), "General".into());
        m
    }

    #[test]
    fn a_deck_hands_out_its_order_then_reshuffles() {
        let mut d = PortraitDeck { count: 3, cursor: 0, order: vec![2, 0, 1], seed: 99 };
        assert_eq!([d.draw(), d.draw(), d.draw()], [Some(2), Some(0), Some(1)]);
        let mut expected = vec![2, 0, 1];
        shuffle_in_place(&mut expected, lcg_step(99));
        assert_eq!(d.draw(), Some(expected[0]), "the fourth draw comes from the reshuffled deck");
        assert_eq!((d.cursor, d.seed, &d.order), (1, lcg_step(99), &expected));
        assert_eq!(PortraitDeck::default().draw(), None, "an empty deck hands out nothing");
    }

    #[test]
    fn a_new_general_draws_by_age_and_gets_both_pictures() {
        let mut m = model(30);
        m.world.character_details.get_mut(&CharacterId(5)).unwrap().portrait.index = -1;
        m.assign_portrait(CharacterId(5), "General");
        let p = &m.world.character_details[&CharacterId(5)].portrait;
        assert_eq!((p.index, p.card.as_str()), (2, "ui/portraits/european/Cards/general/young/002.tga"));
        assert_eq!(p.info, "ui/portraits/european/Info/general/young/002.jpg");
        // 45 and over: the old deck and pictures.
        let mut m = model(45);
        m.assign_portrait(CharacterId(5), "General");
        let p = &m.world.character_details[&CharacterId(5)].portrait;
        assert_eq!((p.index, p.card.as_str()), (1, "ui/portraits/european/Cards/general/old/001.tga"));
    }

    #[test]
    fn an_admiral_draws_from_the_category_his_folder_names() {
        // agents #6 of admiral is `General`: the General deck, not the admiral one.
        let mut m = model(30);
        m.assign_portrait(CharacterId(5), "admiral");
        let p = m.world.character_details[&CharacterId(5)].portrait.clone();
        assert_eq!((p.index, p.card.as_str()), (2, "ui/portraits/european/Cards/general/young/002.tga"));
        let decks = &m.world.portraits[0].categories;
        assert_eq!((decks[0].decks[0].cursor, decks[1].decks[0].cursor), (1, 0));
    }

    #[test]
    fn an_empty_deck_leaves_no_portrait_and_a_kept_number_is_not_redrawn() {
        let mut m = model(30);
        m.assign_portrait(CharacterId(5), "colonel");
        let p = m.world.character_details[&CharacterId(5)].portrait.clone();
        assert_eq!((p.index, p.card.as_str(), p.info.as_str()), (-1, "", ""));
        // Promoted: drawn now; a second resolution (another type change) keeps the number.
        m.assign_portrait(CharacterId(5), "General");
        m.assign_portrait(CharacterId(5), "admiral");
        assert_eq!(m.world.character_details[&CharacterId(5)].portrait.index, 2);
        assert_eq!(m.world.portraits[0].categories[0].decks[0].cursor, 1, "one draw only");
    }

    #[test]
    fn historical_notes_give_the_guerrilla_leaders_their_numbers() {
        assert_eq!(historical_portrait("guerrilla#spa_badajoz#013"), Some(("guerilla", Some(13))));
        assert_eq!(historical_portrait("guerrilla#spa_badajoz"), Some(("guerilla", Some(0))));
        assert_eq!(historical_portrait("#guerrilla#spa_castilla_la_vieja#004"), None);
        assert_eq!(historical_portrait("British Admiral"), None);
        assert_eq!(historical_portrait("guerrilla#x#01a"), Some(("guerilla", None)));
    }

    #[test]
    fn the_number_is_drawn_before_the_paths_and_each_gap_is_reported() {
        // No `CULTURE_PATHS` entry for the type: drawn first (`0x00A05440` stores the number
        // before `0x009CBF40` builds the paths), no pictures, and the gap is reported.
        let mut m = model(30);
        m.world.portraits[0].paths.retain(|(a, _)| a != "General");
        m.assign_portrait(CharacterId(5), "General");
        let p = m.world.character_details[&CharacterId(5)].portrait.clone();
        assert_eq!((p.index, p.card.as_str(), p.info.as_str()), (2, "", ""));
        assert_eq!(m.world.portraits[0].categories[0].decks[0].cursor, 1, "drawn once");
        assert_eq!(m.portrait_problem(CharacterId(5), "General"), Some("his culture has no portrait folder for his agent type; his card shows no portrait"));
        // An empty deck: -1, reported as such.
        let mut m = model(30);
        m.assign_portrait(CharacterId(5), "colonel");
        assert_eq!(m.portrait_problem(CharacterId(5), "colonel"), Some("his portrait deck has no pictures; his card shows no portrait"));
        // No set for his culture (`0x009CB3C0`): nothing changes.
        m.world.portraits[0].culture = "middle_east".into();
        m.assign_portrait(CharacterId(5), "General");
        assert_eq!(m.world.character_details[&CharacterId(5)].portrait.index, -1);
        assert_eq!(m.portrait_problem(CharacterId(5), "General"), Some("his culture has no portrait set; his card shows no portrait"));
        m.world.portraits.clear();
        assert_eq!(m.portrait_problem(CharacterId(5), "General"), Some("the campaign has no portrait allocator; his card shows no portrait"));
    }

    #[test]
    fn a_guerrilla_leader_keeps_his_fixed_number() {
        let mut m = model(30);
        m.world.portraits[0].paths.push(("guerilla".into(), "european".into()));
        m.assign_historical_portrait(CharacterId(5), "General", "guerrilla#spa_badajoz#013");
        let p = &m.world.character_details[&CharacterId(5)].portrait;
        assert_eq!((p.index, p.card.as_str()), (13, "ui/portraits/european/Cards/guerilla/young/013.tga"));
        assert_eq!(m.portrait_problem(CharacterId(5), "guerilla"), None);
        // A negative number (modded #7) builds the exe's `%03d` path, reported as naming no file.
        let mut m = model(30);
        m.world.portraits[0].paths.push(("guerilla".into(), "european".into()));
        m.assign_historical_portrait(CharacterId(5), "General", "guerrilla#x#-5");
        let p = &m.world.character_details[&CharacterId(5)].portrait;
        assert_eq!((p.index, p.card.as_str()), (-5, "ui/portraits/european/Cards/guerilla/young/-05.tga"));
        assert!(m.portrait_problem(CharacterId(5), "guerilla").is_some());
        // A custom picture name keeps the constructor's number (`0x00992E60` stores it first).
        let mut m = model(30);
        m.world.character_details.get_mut(&CharacterId(5)).unwrap().portrait.alternative = "nelson".into();
        m.assign_historical_portrait(CharacterId(5), "General", "guerrilla#x#007");
        assert_eq!(m.world.character_details[&CharacterId(5)].portrait.index, 7);
    }

    #[test]
    fn portrait_problems_name_only_real_gaps() {
        // A start card still at -1 (the constructor's "none yet") is not a negative number.
        let mut m = model(30);
        m.world.character_details.get_mut(&CharacterId(5)).unwrap().portrait.card = "ui/portraits/european/Cards/general/young/004.tga".into();
        assert_eq!(m.portrait_problem(CharacterId(5), "General"), None);
        // The same gap on a start card never resolved (-1): no previous picture was involved.
        m.world.portraits[0].paths.retain(|(a, _)| a != "General");
        let mut start = m.clone();
        start.world.character_details.get_mut(&CharacterId(5)).unwrap().portrait.index = -1;
        assert_eq!(
            start.portrait_problem(CharacterId(5), "General"),
            Some("his culture has no portrait folder for his agent type, so his card keeps the picture the start position gave it")
        );
        // Resolved again as a type without a `CULTURE_PATHS` folder (removed above): the old pictures stay, reported.
        m.assign_portrait(CharacterId(5), "General");
        let p = m.world.character_details[&CharacterId(5)].portrait.clone();
        assert_eq!((p.index, p.card.as_str()), (2, "ui/portraits/european/Cards/general/young/004.tga"));
        assert_eq!(
            m.portrait_problem(CharacterId(5), "General"),
            Some("his culture has no portrait folder for his agent type, so his card keeps his previous picture")
        );
        // A historical guerrilla leader's portrait resolves as `guerilla`, everyone else as his own type.
        assert_eq!(m.portrait_agent(CharacterId(5)), "General");
        let rules = std::sync::Arc::make_mut(&mut m.rules);
        rules.historical.push(crate::campaign::rules::HistoricalCandidate {
            key: "spa_leader".into(),
            male: true,
            kind: "General".into(),
            faction: "france".into(),
            years: (1800, 1815),
            note: "guerrilla#spa_badajoz#013".into(),
        });
        m.world.character_details.get_mut(&CharacterId(5)).unwrap().historical_key = Some("spa_leader".into());
        assert_eq!(m.portrait_agent(CharacterId(5)), "guerilla");
    }
}

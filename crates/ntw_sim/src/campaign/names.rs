//! Names of new characters and unit officers (SAVE_COMPAT.md §14, §21): the original names a
//! character when it creates him (the naming routine `0x009940A0`, called by the agent `0x008DAAB0`
//! and character `0x008B7340` / `0x008B7070` spawners, CONFIRMED callers), so the model does the same
//! and the name is state like any other: saved, shown and scripted from the moment he exists. Unit
//! officers' names are model state too ([`super::world::CampaignUnit::officer_name`]); the save
//! writers only write what the model holds.
//!
//! The rule (CONFIRMED in the exe; the random stream it runs on is PROVISIONAL, SAVE_COMPAT.md §21:
//! exact names would need the original's own event order):
//! - a forename from the faction's allocator [`POOL_MALE_FORENAME`] and a surname from allocator
//!   [`POOL_SURNAME`]: each draw takes the first deck entry, refilling and reshuffling an empty deck
//!   (`0x008A9FA0`, [`NameAllocator`]); the entry indexes the pool ([`NamePools`], built by the
//!   campaign source from the `names` table like the original's builder `0x00F74CB0`);
//! - the pair is kept unless "forename surname" is a historical character's on-screen name
//!   (`0x00A28CC0`); then it picks again, up to [`MAX_TRIES`] times, from the pools with the world's
//!   random state (world +0xFB8, the model's `rng`): one MS LCG step per name, and `0x008AA290` turns
//!   the step's high 16 bits into two further steps, the second giving the stored key's index and the
//!   first the display text's (a quirk of the original; the stored key is the one that counts);
//! - an allocator whose stored size is not its pool's size (stale start-position sizes) starts with
//!   the pool's size and an empty deck, keeping its seed (the loader `0x0085EF80`, INFERRED).

use std::collections::{BTreeMap, BTreeSet};

use super::ids::{CharacterId, FactionId, ForceId, UnitId};
use super::world::CampaignModel;

/// Male forenames: the allocator (save order) a new character's forename comes from.
pub const POOL_MALE_FORENAME: usize = 0;
/// Surnames: the allocator (save order) a new character's surname comes from.
pub const POOL_SURNAME: usize = 4;
/// The most picks the naming routine makes (CONFIRMED loop bound in `0x009940A0`).
pub const MAX_TRIES: u32 = 1000;

/// MS LCG step (the original's `rand` constants).
pub fn lcg_step(s: u32) -> u32 {
    s.wrapping_mul(0x343FD).wrapping_add(0x269EC3)
}

/// The original's `random_shuffle` of a deck in place from the (already advanced) LCG state
/// `seed`: the stream starts at its high 16 bits; for each position i ≥ 1 one step (more while
/// the draw falls in the rejected top range) picks j in 0..=i, swapped with i unless j = i. The
/// name decks (`0x008EFB70` / `0x0086EA70`) and the portrait decks (`0x00A1E910`) both use it.
pub fn shuffle_in_place(deck: &mut [u32], seed: u32) {
    let mut r = seed >> 16;
    for i in 1..deck.len() {
        let m = (i + 1) as u32;
        let pick = loop {
            r = lcg_step(r);
            let v = r >> 16;
            // Reject the top partial range so every residue is equally likely.
            if u32::MAX / m > v / m || u32::MAX % m == m - 1 {
                break v % m;
            }
        };
        if pick != m - 1 {
            deck.swap(i, pick as usize);
        }
    }
}

/// One name allocator: a faction's shuffled deck over one pool (`NAME_ALLOCATION_DETAILS`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NameAllocator {
    /// Pool size.
    pub size: u32,
    /// LCG state.
    pub seed: u32,
    /// Remaining pool indices, next first. `u32` (the save stores u16, which the ESF writer
    /// checks), so a pool may hold any number of names.
    pub deck: Vec<u32>,
}

impl NameAllocator {
    /// The full shuffled deck a refill makes from the (already advanced) seed `seed`.
    pub fn shuffled(size: u32, seed: u32) -> Vec<u32> {
        let mut deck: Vec<u32> = (0..size).collect();
        shuffle_in_place(&mut deck, seed);
        deck
    }

    /// Refills and shuffles the deck (`0x008EFB70` / `0x0086EA70`).
    fn refill(&mut self) {
        self.seed = lcg_step(self.seed);
        self.deck = Self::shuffled(self.size, self.seed);
    }

    /// Draws the next pool index (`0x008A9FA0`), or `None` for an empty pool.
    pub fn draw(&mut self) -> Option<u32> {
        if self.deck.is_empty() {
            if self.size == 0 {
                return None;
            }
            self.refill();
        }
        Some(self.deck.remove(0))
    }

    /// Fits a stored allocator to its pool of `n` names: a different stored size (stale
    /// start-position sizes) takes the pool's size and an empty deck (INFERRED, `0x0085EF80`).
    pub fn fit(&mut self, n: usize) {
        if self.size as usize != n {
            self.size = n as u32;
            self.deck.clear();
        }
    }
}

/// One pool entry: the localisation key a character stores and its on-screen text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NameEntry {
    /// e.g. `names_name_names_frenchDenis`.
    pub key: String,
    /// e.g. `Denis`.
    pub text: String,
}

/// A faction's two pools, in the order their allocators index them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamePools {
    /// Male forenames.
    pub forenames: Vec<NameEntry>,
    /// Surnames.
    pub surnames: Vec<NameEntry>,
}

/// The naming data of a campaign (game data, built by the campaign source).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NameRules {
    /// Faction key → its pools.
    pub pools: BTreeMap<String, NamePools>,
    /// On-screen names of the historical characters ("Forename Surname"): a drawn name may not be one.
    pub historical: BTreeSet<String>,
    /// Every name key of the `names` table → its on-screen text (pool or not: a unit officer may
    /// carry a historical character's name, whose rows have weight 0).
    pub texts: BTreeMap<String, String>,
}

impl NameRules {
    /// The on-screen "forename surname" of a pair of name keys (a key without text shows as itself).
    fn display(&self, (forename, surname): &(String, String)) -> String {
        let text = |k: &String| self.texts.get(k).cloned().unwrap_or_else(|| k.clone());
        format!("{} {}", text(forename), text(surname))
    }
}

/// `0x008AA290`: an index of a pool of `n` from the high bits `p` of a world-random step; returns
/// (display index, key index).
fn pool_pick(n: usize, p: u32) -> (usize, usize) {
    let u = lcg_step(p);
    let a = (((u >> 16) as u64 * n as u64) / 0xFFFF) as usize;
    let b = (((lcg_step(u) >> 16) as u64 * n as u64) / 0xFFFF) as usize;
    (a.min(n - 1), b.min(n - 1))
}

/// A (forename key, surname key) pair from `pools` (both non-empty): deck draws from `fore` and
/// `sur` (already fitted to the pools), then world-random picks while the name is a historical
/// character's (`world_seed` advances only then). `None` when a pool is empty.
pub fn draw_name(pools: &NamePools, historical: &BTreeSet<String>, fore: &mut NameAllocator, sur: &mut NameAllocator, world_seed: &mut u32) -> Option<(String, String)> {
    let (fp, sp) = (&pools.forenames, &pools.surnames);
    if fp.is_empty() || sp.is_empty() {
        return None;
    }
    let f = fore.draw().map_or(0, |i| i as usize).min(fp.len() - 1);
    let s = sur.draw().map_or(0, |i| i as usize).min(sp.len() - 1);
    let (mut fk, mut ft) = (&fp[f].key, &fp[f].text);
    let (mut sk, mut st) = (&sp[s].key, &sp[s].text);
    let mut tries = 0;
    while tries < MAX_TRIES && historical.contains(&format!("{ft} {st}")) {
        tries += 1;
        *world_seed = lcg_step(*world_seed);
        let (a, b) = pool_pick(fp.len(), *world_seed >> 16);
        (fk, ft) = (&fp[b].key, &fp[a].text);
        *world_seed = lcg_step(*world_seed);
        let (a, b) = pool_pick(sp.len(), *world_seed >> 16);
        (sk, st) = (&sp[b].key, &sp[a].text);
    }
    Some((fk.clone(), sk.clone()))
}

impl CampaignModel {
    /// Runs `f` on `faction`'s naming data, its two decks (fitted to the pools) and the world random
    /// state, or `None` without naming data for it (a model built without the names table; the
    /// campaign source logs a faction that has pools but no decks).
    fn with_decks<R>(&mut self, faction: FactionId, f: impl FnOnce(&NameRules, &NamePools, &mut NameAllocator, &mut NameAllocator, &mut u32) -> R) -> Option<R> {
        let key = &self.world.factions.get(&faction)?.key;
        let rules = &self.rules;
        let pools = rules.names.pools.get(key)?;
        let allocs = self.world.name_allocators.get_mut(&faction).filter(|a| a.len() > POOL_SURNAME)?;
        let (low, high) = allocs.split_at_mut(POOL_SURNAME);
        let (fore, sur) = (&mut low[POOL_MALE_FORENAME], &mut high[0]);
        fore.fit(pools.forenames.len());
        sur.fit(pools.surnames.len());
        Some(f(&rules.names, pools, fore, sur, &mut self.rng.state))
    }

    /// A (forename, surname) pair drawn for `faction` from its pools and decks (advancing them).
    fn draw_faction_name(&mut self, faction: FactionId) -> Option<(String, String)> {
        self.with_decks(faction, |names, pools, fore, sur, seed| draw_name(pools, &names.historical, fore, sur, seed)).flatten()
    }

    /// A unit officer's name `name` for a character made from the unit: an empty forename is drawn
    /// from the forename deck alone, the surname kept (`0x009940A0` with the faction set:
    /// `0x008AA220(male, not noble)` only); the name is kept unless it is a historical character's
    /// on-screen name, then drawn again from the faction's decks (forename
    /// deck, surname deck) up to [`MAX_TRIES`] times while it still is. CONFIRMED: the colonel
    /// `0x008B7EF0`, captain `0x008B7F60`, land promotion `0x008E1C20` (through the unit's slot 1,
    /// `0x008B7EF0`) and naval promotion `0x008E2260` paths pass unit +0x7c to
    /// `0x00990EF0`, whose naming routine `0x009940A0` runs with the faction set, so its re-picks are
    /// the faction's deck draws `0x008AA220(male, not noble)` (faction +0x530) and `0x008AA7E0(not
    /// noble)` (+0x560), both `0x008A9FA0`, with no world random state; the faction saver `0x00892A80`
    /// writes the decks +0x530, +0x548, +0x590, +0x5A8, +0x560, ... in that order, so these are save
    /// order allocators 0 and 4.
    /// Without naming data the name is kept.
    fn officer_name_for_character(&mut self, faction: FactionId, name: (String, String)) -> (String, String) {
        let kept = name.clone();
        self.with_decks(faction, move |names, pools, fore, sur, _| {
            let mut name = name;
            if name.0.is_empty()
                && let Some(f) = fore.draw().and_then(|f| pools.forenames.get(f as usize))
            {
                name.0 = f.key.clone();
            }
            let mut tries = 0;
            while tries < MAX_TRIES && names.historical.contains(&names.display(&name)) {
                tries += 1;
                let (Some(f), Some(s)) = (fore.draw(), sur.draw()) else { break };
                let (Some(f), Some(s)) = (pools.forenames.get(f as usize), pools.surnames.get(s as usize)) else { break };
                name = (f.key.clone(), s.key.clone());
            }
            name
        })
        .unwrap_or(kept)
    }

    /// Names a character just created (see the module docs), unless he already has a name: one made
    /// from a unit (a colonel or captain taking it over, a promotion) takes its officer's name unless
    /// that is a historical character's ([`Self::officer_name_for_character`]); anyone else a name
    /// drawn from his faction's decks. Call it once the character is attached to his unit, if he
    /// has one.
    pub fn name_new_character(&mut self, id: CharacterId) {
        if self.world.character_details.get(&id).is_some_and(|d| !d.forename.is_empty()) {
            return;
        }
        let Some(faction) = self.world.characters.get(&id).map(|c| c.faction) else { return };
        let officer = self.world.forces.values().flat_map(|f| &f.units).find(|u| u.character == Some(id)).map(|u| u.officer_name.clone());
        let name = match officer {
            Some(n) => Some(self.officer_name_for_character(faction, n)),
            None => self.draw_faction_name(faction),
        };
        let Some((forename, surname)) = name else { return };
        let d = self.world.character_details.entry(id).or_default();
        d.forename = forename;
        d.surname = surname;
    }

    /// Names the officer of a unit just raised in `force`, unless it has one: a unit raised with its
    /// own character carries his name, any other a name drawn from the faction's decks (INFERRED
    /// path, SAVE_COMPAT.md §21).
    ///
    /// INFERRED: a unit raised with its character carries his name. The original's saves show it
    /// for colonels (a colonel and his unit both "Andre Guadian") and for generals and their
    /// bodyguard units (39 of 39 in `auto_save`, 11 of 11 in two Peninsula saves). Whether the
    /// unit constructor `0x0088B850` (which draws a name unless it is given one) spends a deck pair
    /// first when a general is hired is not traced: here it draws nothing.
    pub fn name_new_unit(&mut self, force: ForceId, unit: UnitId) {
        let Some(f) = self.world.forces.get(&force) else { return };
        let faction = f.faction;
        let Some(u) = f.units.iter().find(|u| u.id == unit) else { return };
        if !u.officer_name.0.is_empty() {
            return;
        }
        let own = u.character.and_then(|c| self.world.character_details.get(&c)).filter(|d| !d.forename.is_empty()).map(|d| (d.forename.clone(), d.surname.clone()));
        let Some(name) = own.or_else(|| self.draw_faction_name(faction)) else { return };
        if let Some(u) = self.world.forces.get_mut(&force).and_then(|f| f.units.iter_mut().find(|u| u.id == unit)) {
            u.officer_name = name;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str) -> NameEntry {
        NameEntry { key: format!("names_name_g{key}"), text: key.into() }
    }

    fn pools() -> NamePools {
        NamePools { forenames: vec![entry("Michel"), entry("Jean")], surnames: vec![entry("Ney"), entry("Lannes")] }
    }

    #[test]
    fn a_historical_name_is_picked_again_with_the_world_random_state() {
        let historical = BTreeSet::from(["Michel Ney".to_string()]);
        // Deck: forename 0 (Michel), surname 0 (Ney) first: "Michel Ney" is historical.
        let (mut fore, mut sur) = (NameAllocator { size: 2, seed: 7, deck: vec![0, 1] }, NameAllocator { size: 2, seed: 7, deck: vec![0, 1] });
        let mut seed = 12345;
        let (f, s) = draw_name(&pools(), &historical, &mut fore, &mut sur, &mut seed).expect("a name");
        assert_ne!(seed, 12345, "the world random state was used");
        assert!(["names_name_gMichel", "names_name_gJean"].contains(&f.as_str()));
        assert!(["names_name_gNey", "names_name_gLannes"].contains(&s.as_str()));
        assert_eq!((fore.deck.clone(), sur.deck.clone()), (vec![1], vec![1]), "one deck draw each");
        // Without a historical clash the world random state is untouched.
        let mut seed2 = 99;
        assert_eq!(draw_name(&pools(), &historical, &mut fore, &mut sur, &mut seed2), Some(("names_name_gJean".into(), "names_name_gLannes".into())));
        assert_eq!(seed2, 99);
        assert_eq!(draw_name(&NamePools::default(), &historical, &mut fore, &mut sur, &mut seed2), None);
    }

    /// A character the model creates is named from his faction's pools at once (and keeps the
    /// name); without naming data for his faction he stays unnamed.
    #[test]
    fn the_model_names_a_new_character_from_his_factions_pools() {
        use super::super::world::{Character, CharacterKind, Faction, GovernmentType, World};
        use crate::calendar::{Calendar, Date, HALF_EARLY};
        let faction = |id: i32, key: &str| Faction {
            id: FactionId(id),
            key: key.into(),
            treasury: 0,
            government: GovernmentType::Republic,
            government_key: String::new(),
            tax_lower: String::new(),
            tax_upper: String::new(),
            diplomacy: BTreeMap::new(),
        };
        let character = |id: i32, f: i32| Character {
            id: CharacterId(id),
            faction: FactionId(f),
            kind: CharacterKind::Colonel,
            position: Default::default(),
            movement_points: 0,
            max_movement_points: 0,
            base_movement_points: 0,
            garrisoned_in: None,
        };
        let mut w = World::default();
        w.factions.insert(FactionId(1), faction(1, "made_up_named"));
        w.factions.insert(FactionId(2), faction(2, "made_up_unnamed"));
        w.characters.insert(CharacterId(10), character(10, 1));
        w.characters.insert(CharacterId(20), character(20, 2));
        w.name_allocators.insert(FactionId(1), vec![NameAllocator { size: 2, seed: 7, deck: vec![1, 0] }; 5]);
        let mut m = CampaignModel::new(Calendar::new(Date { year: 1805, season: 0, month: 0, half: HALF_EARLY }, 0), crate::rng::CaRng::new(5), w);
        let mut rules = (*m.rules).clone();
        rules.names.pools.insert("made_up_named".into(), pools());
        m.rules = std::sync::Arc::new(rules);
        m.name_new_character(CharacterId(10));
        m.name_new_character(CharacterId(20));
        let d = &m.world.character_details[&CharacterId(10)];
        assert_eq!((d.forename.as_str(), d.surname.as_str()), ("names_name_gJean", "names_name_gLannes"));
        assert_eq!(m.world.name_allocators[&FactionId(1)][POOL_MALE_FORENAME].deck, vec![0], "the deck advanced");
        m.name_new_character(CharacterId(10));
        assert_eq!(m.world.character_details[&CharacterId(10)].forename, "names_name_gJean", "a named character keeps his name");
        assert!(m.world.character_details.get(&CharacterId(20)).is_none_or(|d| d.forename.is_empty()), "no pools: unnamed");

        // A unit raised with him carries his name (no draw); one raised alone gets a drawn name.
        use super::super::world::{CampaignUnit, MilitaryForce};
        let unit = |id: i32, character: Option<CharacterId>, officer: (&str, &str)| CampaignUnit {
            id: UnitId(id),
            unit_key: "made_up_unit".into(),
            men: 1,
            max_men: 1,
            character,
            officer_name: (officer.0.into(), officer.1.into()),
        };
        let force = ForceId(30);
        let units = vec![unit(31, Some(CharacterId(10)), ("", "")), unit(32, None, ("", "")), unit(33, None, ("names_name_gMichel", "names_name_gNey"))];
        m.world.forces.insert(force, MilitaryForce { id: force, faction: FactionId(1), commander: Some(CharacterId(10)), units, is_navy: false });
        let decks = m.world.name_allocators[&FactionId(1)].clone();
        m.name_new_unit(force, UnitId(31));
        assert_eq!(m.world.forces[&force].units[0].officer_name, ("names_name_gJean".into(), "names_name_gLannes".into()));
        assert_eq!(m.world.name_allocators[&FactionId(1)], decks, "sharing draws nothing");
        m.name_new_unit(force, UnitId(32));
        assert_eq!(m.world.forces[&force].units[1].officer_name, ("names_name_gMichel".into(), "names_name_gNey".into()));
        assert_ne!(m.world.name_allocators[&FactionId(1)], decks, "a drawn name advances the decks");
        // A colonel who takes over a unit carries its officer's name.
        m.world.characters.insert(CharacterId(11), character(11, 1));
        m.world.forces.get_mut(&force).unwrap().units[2].character = Some(CharacterId(11));
        let decks = m.world.name_allocators[&FactionId(1)].clone();
        m.name_new_character(CharacterId(11));
        let d = &m.world.character_details[&CharacterId(11)];
        assert_eq!((d.forename.as_str(), d.surname.as_str()), ("names_name_gMichel", "names_name_gNey"));
        assert_eq!(m.world.name_allocators[&FactionId(1)], decks, "no draw");
    }

    /// Wellesley dies; a colonel takes over his bodyguard unit, whose officer carries Wellesley's
    /// name: the colonel does not become a second "Arthur Wellesley", he is drawn again from the
    /// faction's decks (no world random state). A unit officer's ordinary name is kept with no draw.
    #[test]
    fn a_colonel_does_not_take_a_historical_officers_name() {
        use super::super::world::{CampaignUnit, Character, CharacterKind, Faction, GovernmentType, MilitaryForce, World};
        use crate::calendar::{Calendar, Date, HALF_EARLY};
        let mut w = World::default();
        w.factions.insert(
            FactionId(1),
            Faction {
                id: FactionId(1),
                key: "made_up_britain".into(),
                treasury: 0,
                government: GovernmentType::Republic,
                government_key: String::new(),
                tax_lower: String::new(),
                tax_upper: String::new(),
                diplomacy: BTreeMap::new(),
            },
        );
        let colonel = |id: i32| Character {
            id: CharacterId(id),
            faction: FactionId(1),
            kind: CharacterKind::Colonel,
            position: Default::default(),
            movement_points: 0,
            max_movement_points: 0,
            base_movement_points: 0,
            garrisoned_in: None,
        };
        w.characters.insert(CharacterId(10), colonel(10));
        w.characters.insert(CharacterId(11), colonel(11));
        let unit = |id: i32, c: i32, officer: (&str, &str)| CampaignUnit {
            id: UnitId(id),
            unit_key: "made_up_unit".into(),
            men: 1,
            max_men: 1,
            character: Some(CharacterId(c)),
            officer_name: (officer.0.into(), officer.1.into()),
        };
        let units = vec![unit(21, 10, ("names_name_gArthur", "names_name_gWellesley")), unit(22, 11, ("names_name_gJean", "names_name_gNey"))];
        w.forces.insert(ForceId(20), MilitaryForce { id: ForceId(20), faction: FactionId(1), commander: Some(CharacterId(10)), units, is_navy: false });
        w.name_allocators.insert(FactionId(1), vec![NameAllocator { size: 2, seed: 7, deck: vec![1, 0] }; 5]);
        let mut m = CampaignModel::new(Calendar::new(Date { year: 1805, season: 0, month: 0, half: HALF_EARLY }, 0), crate::rng::CaRng::new(5), w);
        let mut rules = (*m.rules).clone();
        rules.names.pools.insert("made_up_britain".into(), pools());
        rules.names.historical.insert("Arthur Wellesley".into());
        // Wellesley's rows are not in the pools (weight 0) but have their text.
        rules.names.texts = [("names_name_gArthur", "Arthur"), ("names_name_gWellesley", "Wellesley"), ("names_name_gJean", "Jean"), ("names_name_gNey", "Ney")]
            .into_iter()
            .map(|(k, t)| (k.to_string(), t.to_string()))
            .collect();
        m.rules = std::sync::Arc::new(rules);
        let seed = m.rng.state;
        m.name_new_character(CharacterId(10));
        let d = &m.world.character_details[&CharacterId(10)];
        assert_eq!((d.forename.as_str(), d.surname.as_str()), ("names_name_gJean", "names_name_gLannes"), "drawn again from the decks");
        assert_eq!(m.world.name_allocators[&FactionId(1)][POOL_MALE_FORENAME].deck, vec![0], "one deck draw");
        assert_eq!(m.rng.state, seed, "no world random state");
        let decks = m.world.name_allocators[&FactionId(1)].clone();
        m.name_new_character(CharacterId(11));
        let d = &m.world.character_details[&CharacterId(11)];
        assert_eq!((d.forename.as_str(), d.surname.as_str()), ("names_name_gJean", "names_name_gNey"), "an ordinary officer's name is kept");
        assert_eq!(m.world.name_allocators[&FactionId(1)], decks, "no draw");
        // An officer without a forename: the forename deck alone draws one, the surname is kept.
        m.world.characters.insert(CharacterId(12), colonel(12));
        m.world.forces.get_mut(&ForceId(20)).unwrap().units.push(unit(23, 12, ("", "names_name_gLannes")));
        let decks = m.world.name_allocators[&FactionId(1)].clone();
        m.name_new_character(CharacterId(12));
        let d = &m.world.character_details[&CharacterId(12)];
        assert_eq!((d.forename.as_str(), d.surname.as_str()), ("names_name_gMichel", "names_name_gLannes"));
        let after = &m.world.name_allocators[&FactionId(1)];
        assert_ne!(after[POOL_MALE_FORENAME], decks[POOL_MALE_FORENAME], "the forename deck drew");
        assert_eq!(after[POOL_SURNAME], decks[POOL_SURNAME], "the surname deck did not");
    }

    #[test]
    fn a_stale_allocator_takes_its_pool_size_and_an_empty_deck() {
        let mut a = NameAllocator { size: 5, seed: 3, deck: vec![4, 2] };
        a.fit(2);
        assert_eq!(a, NameAllocator { size: 2, seed: 3, deck: Vec::new() });
        a.fit(2);
        assert_eq!(a.size, 2);
        let d = a.draw().expect("a refill");
        assert!(d < 2 && a.deck.len() == 1 && a.seed == lcg_step(3));
    }

    #[test]
    fn decks_hold_pools_past_u16() {
        // The deck was u16 (an index past 65,535 wrapped); a pool may have any number of names.
        let deck = NameAllocator::shuffled(70_000, 12345);
        assert_eq!(deck.len(), 70_000);
        assert!(deck.contains(&69_999));
        let mut a = NameAllocator { size: 70_000, seed: 1, deck: Vec::new() };
        assert!(a.draw().is_some_and(|i| i < 70_000));
    }
}

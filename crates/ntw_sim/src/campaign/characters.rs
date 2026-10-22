//! Characters: how traits and ancillaries are gained and lost, read from the exe (slot 0-G).
//! Spec: `analysis/fidelity/CHARACTERS_FIDELITY.md` §1–§2.
//!
//! - The trigger scripts (`export_triggers.lua`, `export_ancillaries.lua`, run by `ntw_script`) call
//!   [`roll_trait`] and [`roll_ancillary`]: a roll on the binding's own RNG, the exclusions, then
//!   [`add_trait_points`] / [`add_ancillary`].
//! - [`add_trait_points`] follows `0x008C2D30`: antitraits absorb points first, a held trait grows,
//!   a new trait needs room (`max_traits`, evicting the lowest priority / fewest points).
//! - [`add_ancillary`] follows `0x00A180C0`: type, date, world / faction uniqueness, agent type,
//!   subculture, exclusions, `max_ancillaries` with eviction by priority.
//!
//! Traits are stored as (key, points, level): the level is the held level record, moved only by
//! [`CharacterRules::update_trait_level`] (`0x008B5380`) as points change, which also keeps the
//! no-going-back level; every reader takes the held level
//! ([`EffectRules::held_level_key`](super::effects::EffectRules::held_level_key)).
//!
//! A character's attribute levels (`AgentAttributes`) are stored with his trait levels' and
//! ancillaries' attribute effects already in them: every gain, loss, level change and eviction above
//! moves those effects in or out of the stored levels (`shift_attributes`), as the exe does. A new
//! character starts from his agent type's levels ([`CharacterRules::starting_attributes`]). His rank,
//! the command stars, is read from them ([`super::agents::rank`]).

use std::collections::BTreeMap;

use super::agents::ATTRIBUTES;
use super::details::{CharacterDetails, CharacterTrait};
use super::effects::EffectRules;
use super::ids::{CharacterId, ForceId};
use super::world::CampaignModel;
use crate::rng::CaRng;

/// One trait's rules (`character_traits` + junctions).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TraitRule {
    /// `character_traits` #1: the no-going-back level (0 = none).
    pub no_going_back_level: i32,
    /// `character_traits` #3: eviction priority.
    pub priority: i32,
    /// `character_traits` #2 hidden: left out of the character's trait list in the UI (CONFIRMED, `0x008C65A0`).
    pub hidden: bool,
    /// Traits that lose points when this one is gained (`trait_to_antitraits` rows (this, x)).
    pub antitraits: Vec<String>,
    /// `trait_to_included_agents`.
    pub agents: Vec<String>,
    /// Level thresholds by level number (`character_trait_levels`), ascending.
    pub thresholds: Vec<(i32, i32)>,
}

/// One ancillary's rules (`ancillaries` + junctions).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AncillaryRule {
    /// Type `character` (the only kind that can be gained).
    pub character: bool,
    /// INFERRED: may exist only once in the world.
    pub world_unique: bool,
    /// INFERRED: may exist only once per faction.
    pub faction_unique: bool,
    /// Eviction priority.
    pub priority: i32,
    /// First year (inclusive).
    pub start_year: i32,
    /// End year (exclusive).
    pub end_year: i32,
    /// `ancillary_to_included_agents`.
    pub agents: Vec<String>,
    /// `ancillary_included_subcultures`.
    pub subcultures: Vec<String>,
    /// `ancillary_to_excluded_ancillaries`: held ancillaries that block this one.
    pub excluded: Vec<String>,
}

/// The character rules (built from the DB by `ntw_campaign`).
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterRules {
    /// Traits by key.
    pub traits: BTreeMap<String, TraitRule>,
    /// Ancillaries by key.
    pub ancillaries: BTreeMap<String, AncillaryRule>,
    /// Faction key → subculture (`factions` #2).
    pub faction_subculture: BTreeMap<String, String>,
    /// Subculture → culture (`cultures_subcultures`).
    pub subculture_culture: BTreeMap<String, String>,
    /// Tweak `max_traits` (default 6, CONFIRMED registration at 0x0041E230).
    pub max_traits: usize,
    /// Tweak `max_ancillaries` (default 3, CONFIRMED registration at 0x0042B270).
    pub max_ancillaries: usize,
    /// Agent type → its starting attribute levels (`agent_to_agent_attributes`, table order):
    /// [`Self::starting_attributes`].
    pub agent_attributes: BTreeMap<String, Vec<(String, i32)>>,
    /// The lowest and highest rank ([`super::agents::rank`]): the original clamps to −1..9
    /// (`0x00A19FA1`..`0x00A19FB4`, CONFIRMED).
    pub rank_limits: (i32, i32),
}

impl Default for CharacterRules {
    fn default() -> Self {
        CharacterRules {
            traits: BTreeMap::new(),
            ancillaries: BTreeMap::new(),
            faction_subculture: BTreeMap::new(),
            subculture_culture: BTreeMap::new(),
            max_traits: MAX_TRAITS,
            max_ancillaries: MAX_ANCILLARIES,
            agent_attributes: BTreeMap::new(),
            rank_limits: RANK_LIMITS,
        }
    }
}

/// Default of the `max_traits` tweak.
pub const MAX_TRAITS: usize = 6;
/// Default of the `max_ancillaries` tweak.
pub const MAX_ANCILLARIES: usize = 3;
/// The original's rank clamp ([`CharacterRules::rank_limits`]).
pub const RANK_LIMITS: (i32, i32) = (-1, 9);
/// Initial state of the two binding RNGs (`effect.trait` 0x01457A70, `effect.ancillary` 0x01457968):
/// both hold 0x61266 in the exe's data and nothing else writes them (CONFIRMED), so they run on from
/// the program start and are not saved.
pub const SCRIPT_RNG_SEED: u32 = 0x61266;

/// The RNGs of the script bindings (process-wide in the original; PROVISIONAL: here they restart at
/// [`SCRIPT_RNG_SEED`] with every model, where the original carries them on across loads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptRngs {
    /// `effect.trait`'s RNG.
    pub trait_rng: CaRng,
    /// `effect.ancillary`'s RNG.
    pub ancillary_rng: CaRng,
}

impl Default for ScriptRngs {
    fn default() -> Self {
        ScriptRngs { trait_rng: CaRng::new(SCRIPT_RNG_SEED), ancillary_rng: CaRng::new(SCRIPT_RNG_SEED) }
    }
}

impl CharacterRules {
    /// The subculture of a faction ("" when unknown).
    pub fn subculture(&self, faction_key: &str) -> &str {
        self.faction_subculture.get(faction_key).map_or("", String::as_str)
    }

    /// The culture of a faction ("" when unknown).
    pub fn culture(&self, faction_key: &str) -> &str {
        self.subculture_culture.get(self.subculture(faction_key)).map_or("", String::as_str)
    }

    /// The level (1..) `points` of a trait reach: the highest whose threshold ≤ max(points, 0), 0 when
    /// below the first (CONFIRMED, `0x008B5380`'s search). What a held trait is at is its own
    /// [`CharacterTrait::level`], moved by [`Self::update_trait_level`].
    pub fn level(&self, trait_key: &str, points: i32) -> i32 {
        let p = points.max(0);
        self.traits
            .get(trait_key)
            .and_then(|t| t.thresholds.iter().rev().find(|(_, th)| *th <= p))
            .map_or(0, |(lvl, _)| *lvl)
    }

    /// `0x008B5380` (CONFIRMED): moves a held trait's level after its points changed, `gaining` when
    /// points were added (`0x008CA0E0`, a new trait `0x008EF070`) and not when an antitrait took them
    /// (`0x008B9EE0`).
    ///
    /// 1. The level its points reach ([`Self::level`]). None (below the first threshold): nothing
    ///    changes, so the trait keeps the level it had, whatever it was.
    /// 2. Losing points to a level under the held one, when the trait has a no-going-back level
    ///    (`character_traits` #1) above 0 and the held level has reached it: the points are set to that
    ///    level's threshold and the trait goes to that level (so a fall from above it lands on it, even
    ///    when the points alone would have stopped higher).
    /// 3. Otherwise the trait goes to the level its points reach.
    ///
    /// The exe tells its caller the level changed only when it moved the expected way (up when
    /// gaining, down when losing), and its callers run the old level's loss slot and the new one's
    /// gain slot only then. ORIGINAL BUG: a gain into a level under a kept higher one (step 1 kept it
    /// above the points) moves the level record with no slot, so the old level's attribute effects stay
    /// on the character and the new level's are never added. Ours moves them on every change (the
    /// callers compare the level before and after, [`shift_trait_attributes`]).
    pub fn update_trait_level(&self, t: &mut CharacterTrait, gaining: bool) {
        let mut reached = self.level(&t.key, t.points);
        if reached == 0 {
            return;
        }
        let ngb = self.traits.get(&t.key).map_or(0, |r| r.no_going_back_level);
        // A no-going-back level at or under the held level is one of its levels, so it has a threshold.
        if !gaining && ngb > 0 && ngb <= t.level && reached < t.level
            && let Some(th) = self.threshold(&t.key, ngb)
        {
            t.points = th;
            reached = ngb;
        }
        t.level = reached;
    }

    fn threshold(&self, trait_key: &str, level: i32) -> Option<i32> {
        self.traits.get(trait_key)?.thresholds.iter().find(|(l, _)| *l == level).map(|(_, th)| *th)
    }

    /// The attribute levels a new character of agent type `agent` starts with (`0x00992E60`,
    /// CONFIRMED): every attribute of [`ATTRIBUTES`] at −1 (the type does not have it, the details
    /// ctor `0x00984AD0`), then the type's `agent_to_agent_attributes` rows in table order
    /// ([`Self::agent_attributes`]; a row naming an attribute outside the list adds it).
    pub fn starting_attributes(&self, agent: &str) -> Vec<(String, i32)> {
        let mut attributes: Vec<(String, i32)> = ATTRIBUTES.iter().map(|a| ((*a).to_owned(), -1)).collect();
        for (attr, level) in self.agent_attributes.get(agent).into_iter().flatten() {
            match attributes.iter_mut().find(|(k, _)| k == attr) {
                Some((_, l)) => *l = *level,
                None => attributes.push((attr.clone(), *level)),
            }
        }
        attributes
    }

    /// The details a new character of agent type `agent` starts with: his
    /// [`Self::starting_attributes`] and the situation slot at 0
    /// ([`starting_attribute_bonuses`](super::details::starting_attribute_bonuses), `0x00992E60` at
    /// `0x00992F5D`, CONFIRMED), everything else empty (the caller fills birth, names, post...).
    pub fn new_character_details(&self, agent: &str) -> CharacterDetails {
        CharacterDetails {
            attributes: self.starting_attributes(agent),
            attribute_bonuses: super::details::starting_attribute_bonuses(),
            ..Default::default()
        }
    }
}

/// Adds `sign` × one source's attribute effects to a character's stored attribute levels
/// (CONFIRMED): a trait level's `trait_attribute_effects` (`0x00A0ECB0`, run by the trait list's
/// gain / loss slots `0x008DC9F0` / `0x008DCAA0`, vtable `0x01354408`) or an ancillary's `ancillary_to_attribute_effects`
/// (`0x00A0EC10`), with no floor or cap; an attribute he lacks counts as −1 and is added. So the
/// stored levels (`AgentAttributes`) are the effective ones every reader takes
/// ([`super::effects::Effects::character_attribute`]).
fn shift_attributes(attributes: &mut Vec<(String, i32)>, effects: Option<&Vec<(String, i32)>>, sign: i32) {
    for (attr, v) in effects.into_iter().flatten() {
        match attributes.iter_mut().find(|(k, _)| k == attr) {
            Some((_, level)) => *level += sign * v,
            None => attributes.push((attr.clone(), -1 + sign * v)),
        }
    }
}

/// Moves trait `key`'s attribute effects from its held level `before` to `after` (0: no level
/// record, or not held), as a level change runs the old level's loss slot then the new level's gain
/// slot (`0x008D21B0`, `0x008C2D30`, CONFIRMED).
fn shift_trait_attributes(fx: &EffectRules, attributes: &mut Vec<(String, i32)>, key: &str, before: i32, after: i32) {
    let old = fx.held_level_key(key, before);
    let new = fx.held_level_key(key, after);
    if old == new {
        return;
    }
    shift_attributes(attributes, old.and_then(|k| fx.trait_attribute.get(k)), -1);
    shift_attributes(attributes, new.and_then(|k| fx.trait_attribute.get(k)), 1);
}


/// A character's attribute levels once he becomes agent type `agent` (a promotion, `0x00A1A300`,
/// CONFIRMED): every attribute back to −1, then the new type's levels
/// ([`CharacterRules::starting_attributes`]), then his held trait levels' and ancillaries' attribute
/// effects again.
///
/// ORIGINAL BUG: `0x00A1A300` copies the new type's levels over the stored ones and adds nothing
/// back, so the effects of the traits and ancillaries he keeps are lost, and losing one of them later
/// (its loss slot) takes its effect off the new levels a second time. Ours adds them back.
pub fn promoted_attributes(rules: &super::rules::CampaignRules, agent: &str, d: &CharacterDetails) -> Vec<(String, i32)> {
    let fx = &rules.effects;
    let mut attributes = rules.characters.starting_attributes(agent);
    for t in &d.traits {
        shift_trait_attributes(fx, &mut attributes, &t.key, 0, t.level);
    }
    for a in &d.ancillaries {
        shift_attributes(&mut attributes, fx.ancillary_attribute.get(a), 1);
    }
    attributes
}

/// What [`add_trait_points`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraitOutcome {
    /// Points went onto a held trait; its level before and after.
    Raised {
        /// Level before.
        from_level: i32,
        /// Level after.
        to_level: i32,
    },
    /// The trait was added.
    Added {
        /// Its level.
        level: i32,
        /// The trait evicted to make room.
        evicted: Option<String>,
    },
    /// Antitraits absorbed every point.
    Absorbed,
    /// The trait list was full and the new trait did not outrank the lowest held one.
    NoRoom,
    /// Unknown trait key or character.
    Unknown,
}

/// `0x008C2D30` (CONFIRMED): adds `points` (> 0) of trait `key` to a character.
/// Any change re-sums his own effect set (`0x009CDF10`), which also refreshes his sight radius
/// ([`CampaignModel::sight_radius`]).
pub fn add_trait_points(model: &mut CampaignModel, c: CharacterId, key: &str, points: i32) -> TraitOutcome {
    let out = add_trait_points_inner(model, c, key, points);
    if !matches!(out, TraitOutcome::NoRoom | TraitOutcome::Unknown) {
        model.update_sight_radius(c);
    }
    out
}

fn add_trait_points_inner(model: &mut CampaignModel, c: CharacterId, key: &str, points: i32) -> TraitOutcome {
    let rules = model.rules.clone();
    let cr = &rules.characters;
    let fx = &rules.effects;
    let Some(rule) = cr.traits.get(key) else { return TraitOutcome::Unknown };
    let Some(d) = model.world.character_details.get_mut(&c) else { return TraitOutcome::Unknown };
    // 1. Every held trait that lists the new one as its antitrait, in list order (`0x008D21B0` /
    //    `0x008B9EE0`): it loses `p` points; `p` becomes what it has left; if that is below 1 it is
    //    removed; if it went below 0 the overflow is what remains to add, otherwise nothing is added.
    //    (As in the exe, a second antitrait in the list then loses the first one's remainder.)
    let mut p = points;
    let mut add = true;
    let mut i = 0;
    while i < d.traits.len() {
        let held = d.traits[i].key.clone();
        let is_anti = held != key && cr.traits.get(&held).is_some_and(|h| h.antitraits.iter().any(|a| a == key));
        if !is_anti || p == 0 {
            i += 1;
            continue;
        }
        let before = d.traits[i].level;
        d.traits[i].points -= p;
        let overflow = if d.traits[i].points < 0 { -d.traits[i].points } else { 0 };
        cr.update_trait_level(&mut d.traits[i], false);
        p = d.traits[i].points;
        if d.traits[i].points < 1 {
            // ORIGINAL BUG: the exe erases the trait (`0x008BDB20`) without running its level's loss
            // slot (`0x008D21B0`), so the attribute effects of the level it last had stay on the
            // character for good. Ours takes them off with the trait.
            d.traits.remove(i);
            shift_trait_attributes(fx, &mut d.attributes, &held, before, 0);
        } else {
            let after = d.traits[i].level;
            shift_trait_attributes(fx, &mut d.attributes, &held, before, after);
            i += 1;
        }
        if overflow == 0 {
            add = false;
        } else {
            add = true;
            p = overflow;
        }
    }
    if !add {
        return TraitOutcome::Absorbed;
    }
    // 2. Held: more points.
    //    (`0x008CA0E0`; the level moves as [`CharacterRules::update_trait_level`] says, and ours moves
    //    its attribute effects on any change, see the ORIGINAL BUG there.)
    if let Some(t) = d.traits.iter_mut().find(|t| t.key == key) {
        let from_level = t.level;
        t.points += p;
        cr.update_trait_level(t, true);
        let to_level = t.level;
        shift_trait_attributes(fx, &mut d.attributes, key, from_level, to_level);
        return TraitOutcome::Raised { from_level, to_level };
    }
    // 3. New: make room if the list is full.
    let mut evicted = None;
    if d.traits.len() >= cr.max_traits {
        let lowest = d
            .traits
            .iter()
            .enumerate()
            .min_by_key(|(_, t)| (cr.traits.get(&t.key).map_or(0, |r| r.priority), t.points))
            .map(|(i, t)| (i, cr.traits.get(&t.key).map_or(0, |r| r.priority), t.points));
        if let Some((i, prio, pts)) = lowest
            && (prio < rule.priority || (prio == rule.priority && pts < p))
        {
            let gone = d.traits.remove(i);
            // Eviction runs the level's loss slot, then erases (`0x008E9830`).
            shift_trait_attributes(fx, &mut d.attributes, &gone.key, gone.level, 0);
            evicted = Some(gone.key);
        }
    }
    if d.traits.len() >= cr.max_traits {
        return TraitOutcome::NoRoom;
    }
    // A new held trait starts with no level record and takes the level its points reach
    // (`0x0088B0B0` → `0x008EF070`: points = max(points, 0), then `0x008B5380` gaining).
    let mut t = CharacterTrait { key: key.to_string(), points: p.max(0), level: 0 };
    cr.update_trait_level(&mut t, true);
    let level = t.level;
    d.traits.push(t);
    // A new trait with a level runs that level's gain slot (`0x008E3F00`).
    shift_trait_attributes(fx, &mut d.attributes, key, 0, level);
    TraitOutcome::Added { level, evicted }
}

/// The held trait levels a loaded model starts with, once its rules are attached (CONFIRMED): the
/// original's save holds a held trait's key and points only (`0x0088AFF0`, read by the trait list's
/// loader `0x0088B110`), and its load sets the points to max(points, 0) and the level from them as for
/// a new trait (`0x008EF070` → [`CharacterRules::update_trait_level`] gaining, from no level record).
/// Only traits at level 0 are set: our own save keeps the level, and one from before it did reads 0.
/// The stored attribute levels are saved with their effects in, so nothing is shifted.
///
/// ORIGINAL BUG: a level kept below the first threshold (`0x008B5380`) is not in the original's save,
/// so its reload drops the trait to no level: the level's effects stop applying (`0x008EDAE0` takes
/// the level record's), while its attribute effects stay in the saved attribute levels and no loss
/// slot can take them off any more. Our own save keeps the level; an original save has no level to
/// read, so its load still gives none there.
pub fn load_trait_levels(model: &mut CampaignModel) {
    let rules = model.rules.clone();
    for d in model.world.character_details.values_mut() {
        for t in d.traits.iter_mut() {
            if t.level == 0 {
                t.points = t.points.max(0);
                rules.characters.update_trait_level(t, true);
            }
            // A held level with no `character_trait_levels` row applies no effects and shows no level.
            // Reported once per trait key per process (not per load), a key with no rows at all apart.
            if t.level != 0 && rules.effects.held_level_row(&t.key, t.level).is_none() {
                static LOGGED: std::sync::Mutex<std::collections::BTreeSet<String>> = std::sync::Mutex::new(std::collections::BTreeSet::new());
                let first = LOGGED.lock().map(|mut s| s.insert(t.key.clone())).unwrap_or(false);
                if first && rules.effects.trait_levels.contains_key(&t.key) {
                    log::warn!("trait {:?} is held at level {} but has no character_trait_levels row for that level; its level effects do not apply", t.key, t.level);
                } else if first {
                    log::warn!("trait {:?} is held at level {} but the data has no character_trait_levels rows for the trait at all; its level effects do not apply", t.key, t.level);
                }
            }
        }
    }
}

/// `remove_trait` (binding `0x008E9F50` → `0x008E2DD0` → `0x008ADE70` → `0x00A1D5E0` → `0x00A1D7B0`,
/// CONFIRMED): drops a held trait unless its level has reached its no-going-back level
/// (`0x008E98A0`: removed only while the level is below `character_traits` #1, so a trait whose
/// no-going-back level is 0 is never removed), taking its level's attribute effects off (the loss
/// slot, then the erase), then re-sums his effects (`0x009CDF10`). Returns whether it was removed.
pub fn remove_trait(model: &mut CampaignModel, c: CharacterId, key: &str) -> bool {
    let rules = model.rules.clone();
    let Some(d) = model.world.character_details.get_mut(&c) else { return false };
    let Some(i) = d.traits.iter().position(|t| t.key == key) else { return false };
    let level = d.traits[i].level;
    // ORIGINAL BUG: `0x008E98A0` reads the level record's number before testing it for null, so a
    // trait held with no level record crashes the game. Ours takes that as level 0. A trait the
    // rules no longer know has no level to keep and is removed.
    let kept = rules.characters.traits.get(key).is_some_and(|r| level >= r.no_going_back_level);
    if kept {
        return false;
    }
    d.traits.remove(i);
    shift_trait_attributes(&rules.effects, &mut d.attributes, key, level, 0);
    model.update_sight_radius(c);
    true
}

/// Why [`add_ancillary`] refused (the exe's own result names, `0x00A180C0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncillaryRefusal {
    /// Not a `character` ancillary, or unknown key / character.
    NotCharacter,
    /// `failed_date_check`.
    Date,
    /// `failed_world_uniqueness`.
    WorldUniqueness,
    /// `failed_faction_uniqueness`.
    FactionUniqueness,
    /// `failed_culture_type` (subculture not included).
    Culture,
    /// `failed_character_type` (agent type not included).
    CharacterType,
    /// `failed_excluded_ancillary`.
    Excluded,
    /// Full and the new one does not outrank the lowest held one (`0x009D1B90` result 4).
    NoRoom,
    /// Already held (result 5).
    AlreadyHeld,
}

/// `0x00A180C0` (CONFIRMED order): gives ancillary `key` to a character. On success returns the
/// ancillary evicted to make room, if any.
pub fn add_ancillary(model: &mut CampaignModel, c: CharacterId, key: &str) -> Result<Option<String>, AncillaryRefusal> {
    let rules = model.rules.clone();
    let cr = &rules.characters;
    let Some(rule) = cr.ancillaries.get(key).filter(|r| r.character) else { return Err(AncillaryRefusal::NotCharacter) };
    let Some(ch) = model.world.characters.get(&c) else { return Err(AncillaryRefusal::NotCharacter) };
    let faction = ch.faction;
    let agent = ch.kind.esf_name();
    let year = model.calendar.date.year as i32;
    if year < rule.start_year || year >= rule.end_year {
        return Err(AncillaryRefusal::Date);
    }
    let w = &model.world;
    let held_by = |pred: &dyn Fn(CharacterId) -> bool| {
        w.character_details.iter().any(|(id, d)| pred(*id) && d.ancillaries.iter().any(|a| a == key))
    };
    if rule.world_unique && held_by(&|_| true) {
        return Err(AncillaryRefusal::WorldUniqueness);
    }
    if rule.faction_unique && held_by(&|id| w.characters.get(&id).is_some_and(|x| x.faction == faction)) {
        return Err(AncillaryRefusal::FactionUniqueness);
    }
    if !rule.agents.iter().any(|a| a == agent) {
        return Err(AncillaryRefusal::CharacterType);
    }
    let fkey = w.factions.get(&faction).map_or("", |f| f.key.as_str());
    let sub = cr.subculture(fkey);
    if !rule.subcultures.iter().any(|s| s == sub) {
        return Err(AncillaryRefusal::Culture);
    }
    let Some(d) = model.world.character_details.get_mut(&c) else { return Err(AncillaryRefusal::NotCharacter) };
    if d.ancillaries.iter().any(|a| a == key) {
        return Err(AncillaryRefusal::AlreadyHeld);
    }
    let prio = |k: &str| cr.ancillaries.get(k).map_or(0, |r| r.priority);
    let mut evict = None;
    if d.ancillaries.len() >= cr.max_ancillaries {
        let lowest = d.ancillaries.iter().enumerate().min_by_key(|(_, a)| prio(a)).map(|(i, a)| (i, prio(a)));
        match lowest {
            Some((i, p)) if rule.priority > p => evict = Some(i),
            _ => return Err(AncillaryRefusal::NoRoom),
        }
    }
    if d.ancillaries.iter().any(|a| rule.excluded.iter().any(|x| x == a)) {
        return Err(AncillaryRefusal::Excluded);
    }
    // The evicted one's attribute effects come off, the new one's go on (`0x009C80C0`).
    let fx = &rules.effects;
    let evicted = evict.map(|i| d.ancillaries.remove(i));
    if let Some(gone) = &evicted {
        shift_attributes(&mut d.attributes, fx.ancillary_attribute.get(gone), -1);
    }
    d.ancillaries.push(key.to_string());
    shift_attributes(&mut d.attributes, fx.ancillary_attribute.get(key), 1);
    model.update_sight_radius(c);
    Ok(evicted)
}

/// `remove_ancillary` (binding `0x008E9990` → `0x00A1CE20` → `0x00A1CEC0`, CONFIRMED): drops a held
/// ancillary, taking its attribute effects off, then re-sums his effects. Returns whether it was held.
pub fn remove_ancillary(model: &mut CampaignModel, c: CharacterId, key: &str) -> bool {
    let rules = model.rules.clone();
    let Some(d) = model.world.character_details.get_mut(&c) else { return false };
    let Some(i) = d.ancillaries.iter().position(|a| a == key) else { return false };
    d.ancillaries.remove(i);
    shift_attributes(&mut d.attributes, rules.effects.ancillary_attribute.get(key), -1);
    model.update_sight_radius(c);
    true
}

/// Whether a character is a recruitment-pool candidate (listed in his faction's general or admiral
/// pool, `CHARACTER_RECRUITMENT_MANAGER`).
pub fn is_pool_candidate(model: &CampaignModel, c: CharacterId) -> bool {
    model.world.characters.get(&c).and_then(|ch| model.world.faction_details.get(&ch.faction)).is_some_and(|f| f.general_pool.0.contains(&c) || f.admiral_pool.0.contains(&c))
}

/// The exclusions both bindings apply (CONFIRMED in `0x008F5070` / `0x008AAC30`):
/// - a recruitment-pool candidate (his +0x520 / +0x521 flags are set until he is recruited, which
///   clears them, `0x00A1B8F0`): his gains go to a pending list instead (`0x008E2DD0` / `0x008E2CB0`;
///   the pending list is not modelled, PROVISIONAL: the gain is dropped);
/// - a governor of the faction's home theatre. In the shipped campaigns every faction has one
///   governorship, its home theatre (EFFECTS_FIDELITY §6), so any governor is excluded.
///
/// Not modelled: the global "non-scripted traits / ancillaries disabled" switches
/// (`set_non_scripted_traits_disabled`).
pub fn excluded_from_scripted_gains(model: &CampaignModel, c: CharacterId) -> bool {
    let w = &model.world;
    let Some(ch) = w.characters.get(&c) else { return true };
    let Some(d) = w.character_details.get(&c) else { return true };
    if is_pool_candidate(model, c) {
        return true;
    }
    d.post != 0
        && w.faction_details.get(&ch.faction).is_some_and(|f| {
            f.posts.iter().any(|p| p.id as u32 == d.post && p.governorship.is_some())
        })
}

/// `effect.trait(key, scope, points, chance)` (binding `0x008F5070`, CONFIRMED): draws r in 0..=100 on
/// the trait RNG and, if r ≤ chance and the character is not excluded, adds the points. Returns the
/// outcome when the roll passed.
pub fn roll_trait(model: &mut CampaignModel, c: CharacterId, key: &str, points: i32, chance: i32) -> Option<TraitOutcome> {
    let r = model.script_rngs.trait_rng.percent_0_100() as i32;
    if chance < r || excluded_from_scripted_gains(model, c) {
        return None;
    }
    Some(add_trait_points(model, c, key, points))
}

/// `effect.ancillary(key, chance)` (binding `0x008AAC30`, CONFIRMED): draws r in 0..=100 on the
/// ancillary RNG and, if r < chance (strict) and the character is not excluded, tries the add.
pub fn roll_ancillary(model: &mut CampaignModel, c: CharacterId, key: &str, chance: i32) -> Option<Result<Option<String>, AncillaryRefusal>> {
    let r = model.script_rngs.ancillary_rng.percent_0_100() as i32;
    if chance <= r || excluded_from_scripted_gains(model, c) {
        return None;
    }
    Some(add_ancillary(model, c, key))
}

/// The death-age distribution built in code by `0x009D0740` (CONFIRMED): per age band, the percent of
/// characters who die in it, spread evenly over its ages (`0x009C9580`); 0 below 51. Sums to 100.
pub const DEATH_BANDS: [(u32, u32, f32); 10] = [
    (51, 55, 3.0),
    (56, 60, 7.0),
    (61, 65, 17.0),
    (66, 70, 27.0),
    (71, 75, 16.0),
    (76, 80, 12.0),
    (81, 85, 8.0),
    (86, 90, 6.0),
    (91, 95, 3.0),
    (96, 100, 1.0),
];

/// The yearly chance of dying at each age 0..=100 (`0x009D0740` + `0x009DBD40`, CONFIRMED): the band
/// density divided by the share still alive at that age, density(a) / Σ density(b ≥ a); 1 at 100.
/// PROVISIONAL detail: the sum is taken in plain order (the exe sums in eight lanes; f32 rounding
/// may differ in the last bit).
pub fn death_hazards() -> [f32; 101] {
    let mut t = [0f32; 101];
    for (from, to, pct) in DEATH_BANDS {
        let each = pct / (to - from + 1) as f32;
        for v in &mut t[from as usize..=to as usize] {
            *v += each;
        }
    }
    for a in 0..t.len() {
        if t[a] > 0.0 {
            let rest: f32 = t[a..].iter().sum();
            t[a] /= rest;
        }
    }
    t
}

/// `0x009D5950` (CONFIRMED): a character of `age` dies this year when u = next16 / 65535 on the campaign
/// RNG is ≤ his age's chance (a draw is made for every age below 101); from 101 on he always dies.
pub fn dies_this_year(rng: &mut CaRng, hazards: &[f32; 101], age: i32) -> bool {
    let Ok(a) = usize::try_from(age) else { return false };
    if a >= hazards.len() {
        return true;
    }
    let u = rng.next16() as f32 * crate::rng::INV_65535;
    let h = hazards[a];
    !(h < u || h <= 0.0)
}

/// What the yearly character pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct YearlyPass {
    /// Characters who died of natural causes, in order.
    pub died: Vec<CharacterId>,
    /// (character, ancillary) pairs that expired.
    pub expired: Vec<(CharacterId, String)>,
}

/// The yearly character pass of the round end (`0x008A9920` → `0x00948CF0` → per faction `0x008BC650`
/// → per character `0x009DA190`, CONFIRMED): it runs at the end of the round whose turn is the year's
/// last (`turn_in_year + 1 == turns_per_year`). For every faction, every character in its list's
/// order ([`super::world::World::characters_in_list_order`]): a death check by
/// age (current year − birth year) unless he appeared less than 3 turns ago, then his ancillaries whose
/// end year has come are dropped (`0x009D2E50`); the dead are removed after the faction's pass
/// (`0x00A0C6F0` reason 3, natural causes).
///
/// Characters with +0x52C (`CHARACTER` #34, [`super::details::CharacterDetails::returns_after_death`])
/// are skipped without a draw. A dying General's own unit is deleted first; a force he commanded
/// goes to a successor ([`CampaignModel::command_vacated`]).
///
/// The family of each faction other than the rebels and still in the game has its year first
/// ([`super::family::family_year`]; `0x008BC650` tests `0x008CEEF0`, then faction +0x824 at `0x008BC6D0`,
/// [`CampaignModel::in_the_game_with_record`], CONFIRMED); the death checks run
/// for every faction, out of the game too. A post whose holder dies gets a new holder at once
/// ([`CampaignModel::refill_post`]).
///
/// PROVISIONAL: the "appeared" turn (+0x4EC) is not in the save, so loaded characters count as old;
/// characters without a birth date are skipped.
pub fn yearly_character_pass(model: &mut CampaignModel) -> YearlyPass {
    let mut out = YearlyPass::default();
    if model.calendar.turn_in_year + 1 != model.calendar.turns_per_year {
        return out;
    }
    let hazards = death_hazards();
    let year = model.calendar.date.year as i32;
    let rules = model.rules.clone();
    for f in model.world.factions_in_turn_order() {
        if model.in_the_game_with_record(f) {
            super::family::family_year(model, f, &hazards);
        }
        // The faction's list (+0x768) in its order (`World::faction_characters_in_list_order`): each
        // draw goes to the character the exe draws for, and the dead die in that order (`0x008BC650`
        // collects them walking the list, CONFIRMED).
        let ids: Vec<CharacterId> = model.world.faction_characters_in_list_order(f);
        let mut dead = Vec::new();
        for c in ids {
            let Some(birth) = model.world.character_details.get(&c).and_then(|d| d.birth) else { continue };
            // Every character draws (the leader too), except those with +0x52C (`CHARACTER` #34),
            // whom `0x009DA190` skips without a draw.
            let exempt = model.world.character_details.get(&c).is_some_and(|d| d.returns_after_death);
            if !exempt && dies_this_year(&mut model.rng, &hazards, year - birth.year as i32) {
                dead.push(c);
            }
            if let Some(d) = model.world.character_details.get_mut(&c) {
                // Each expired one's attribute effects come off as it goes (`0x009D2E50` →
                // `0x00A0EC10`, CONFIRMED).
                let mut i = 0;
                while i < d.ancillaries.len() {
                    if rules.characters.ancillaries.get(&d.ancillaries[i]).is_none_or(|r| year < r.end_year) {
                        i += 1;
                        continue;
                    }
                    let gone = d.ancillaries.remove(i);
                    shift_attributes(&mut d.attributes, rules.effects.ancillary_attribute.get(&gone), -1);
                    out.expired.push((c, gone));
                }
            }
        }
        for (c, _) in &out.expired {
            model.update_sight_radius(*c);
        }
        for c in dead {
            // A General's own unit (his bodyguard) goes first (`0x008BC650`: a General with a unit
            // link, character +0x2B0, has that unit deleted before the kill; CONFIRMED code,
            // INFERRED that +0x2B0 is the unit, `CHARACTER` #5).
            if model.world.characters.get(&c).is_some_and(|x| x.kind == super::world::CharacterKind::General) {
                for force in model.world.forces.values_mut() {
                    force.units.retain(|u| u.character != Some(c));
                }
            }
            model.character_dies(c);
            out.died.push(c);
        }
    }
    out
}

impl CampaignModel {
    /// A character dies: removed with his details; a post he held gets a new holder
    /// ([`Self::refill_post`]); he leaves the recruitment pools; units he was attached to lose him;
    /// the forces he commanded go to a successor ([`Self::command_vacated`]).
    ///
    /// A recruitment item queued through him stays queued, unrefunded, and loses its target (CONFIRMED static
    /// trace 2026-10-10): the character destructor `0x0099D2D0` notifies the observers on his list (+0xD4), and the
    /// item's listener (`0x00B57F40`, land and naval alike) clears its target +0x18 and its byte +0x24 when he is
    /// the item's target; the land spawn (`0x00B1E500`) raises an army that marches to the target only while
    /// +0x18 is set, so the finished unit appears in its settlement like any recruit.
    pub fn character_dies(&mut self, c: CharacterId) {
        let Some(dead) = self.world.remove_character(c) else { return };
        self.world.character_details.remove(&c);
        self.world.sight_radius.remove(&c);
        // INFERRED: no faction keeps a dead character in its exposed list (the loader re-links the ids,
        // `0x008E07F0`, so none may dangle).
        for d in self.world.faction_details.values_mut() {
            d.exposed.retain(|x| *x != c);
        }
        let mut vacated = Vec::new();
        for (f, d) in self.world.faction_details.iter_mut() {
            for (i, p) in d.posts.iter_mut().enumerate().filter(|(_, p)| p.holder == Some(c)) {
                p.holder = None;
                vacated.push((*f, i));
            }
            d.general_pool.0.retain(|&x| x != c);
            d.admiral_pool.0.retain(|&x| x != c);
        }
        for f in self.world.forces.values_mut() {
            for u in &mut f.units {
                if u.character == Some(c) {
                    u.character = None;
                }
            }
        }
        self.drop_recruitment_target(c);
        self.command_vacated(&dead);
        // 0x0099D62E (EndCharacterCommerceRaids): any raid he still holds ends with him (a commander's ended in the hand-over).
        self.clear_character_raids(c);
        for (f, i) in vacated {
            self.refill_post(f, i);
        }
    }

    /// The items queued through `c` lose their target and mark it lost (the item listener `0x00B57F40`
    /// clears +0x18 and +0x24, run by the character destructor `0x0099D2D0`; see
    /// [`Self::character_dies`]). For every path that destroys a character.
    pub(crate) fn drop_recruitment_target(&mut self, c: CharacterId) {
        for r in self.world.regions.values_mut() {
            for i in r.recruitment_queue.iter_mut().filter(|i| i.target == Some(c)) {
                i.target = None;
                i.target_lost = true;
            }
        }
    }

    /// `old` no longer commands his forces (he died, or escaped from a lost battle): each force he
    /// led either goes, when it has no units left (the destructor `0x0099D2D0`: a force whose unit
    /// count is 0 is removed with its characters), or gets a new commander as the original picks
    /// him (`0x008B8E50` → `0x008ED2C0`, CONFIRMED): the first unit in [`Self::commander_unit`]
    /// order gives its own character if it has one, else a new colonel (army) or captain (navy)
    /// attached to it (the unit classes' slot +4: `0x008B7EF0` colonel, `0x008B7F60` captain, both
    /// `0x00990EF0` with agent type 2 / 3). The new commander stands where the old one stood
    /// (`0x00963840`). A save never has a force without a commander (CONFIRMED rule of every
    /// original save, SAVE_COMPAT.md §23); a new colonel takes his unit's officer's name
    /// ([`Self::name_new_character`]). His commerce raids pass to the new commanders
    /// ([`Self::hand_over_raids`], the force's observer `0x008DB3E0`).
    pub(crate) fn command_vacated(&mut self, old: &super::world::Character) {
        use super::world::{Character, CharacterKind};
        let commanded: Vec<ForceId> = self.world.forces.values().filter(|f| f.commander == Some(old.id)).map(|f| f.id).collect();
        let mut successors = Vec::with_capacity(commanded.len());
        for fid in commanded {
            let (navy, empty) = (self.world.forces[&fid].is_navy, self.world.forces[&fid].units.is_empty());
            if let Some(f) = self.world.forces.get_mut(&fid) {
                f.commander = None;
            }
            if empty {
                // He has left the map already: the force stood where he stood.
                let port = self.docked_port_at(fid, old.position);
                self.remove_force(fid, port);
                continue;
            }
            let Some(ui) = self.commander_unit(fid) else { continue };
            if let Some(next) = self.world.forces[&fid].units[ui].character.filter(|n| self.world.characters.contains_key(n)) {
                // The unit's own character (a general riding with the army, a colonel) takes over.
                if let Some(ch) = self.world.characters.get_mut(&next) {
                    ch.position = old.position;
                    ch.garrisoned_in = old.garrisoned_in;
                }
                self.world.forces.get_mut(&fid).expect("force exists").commander = Some(next);
                successors.push(next);
                // The new commander's zones (`0x008B8E50` → `0x008F9400`).
                self.rebuild_force_zones(fid);
                continue;
            }
            let kind = if navy { CharacterKind::Captain } else { CharacterKind::Colonel };
            let mp = self
                .rules
                .agent_action_points
                .get(kind.esf_name())
                .copied()
                .or_else(|| self.world.characters.values().filter(|x| x.kind == kind).map(|x| x.max_movement_points).max())
                .unwrap_or(0);
            let id = CharacterId(self.world.alloc_id() as i32);
            let colonel = Character {
                id,
                faction: old.faction,
                kind,
                position: old.position,
                movement_points: old.movement_points.min(mp),
                max_movement_points: mp,
                base_movement_points: mp,
                garrisoned_in: old.garrisoned_in,
            };
            self.world.characters.insert(id, colonel);
            self.world.character_details.insert(id, self.rules.characters.new_character_details(kind.esf_name()));
            self.update_sight_radius(id);
            let f = self.world.forces.get_mut(&fid).expect("force exists");
            f.commander = Some(id);
            f.units[ui].character = Some(id);
            // He carries his unit's officer's name (`0x008B7EF0` colonel / `0x008B7F60` captain pass unit
            // +0x7c to `0x00990EF0`).
            self.name_new_character(id);
            successors.push(id);
            self.rebuild_force_zones(fid);
        }
        self.hand_over_raids(old.id, old.position, &successors);
        // He commands nothing now: his zone goes (`0x008DB3E0`).
        self.rebuild_zone(old.id);
    }

    /// The unit of `force` that gives a new commander: the first in the original's order
    /// (`0x008ED2C0` sorts the force's units with `0x00899450` → `0x008CF620` on the keys built by
    /// the unit classes' slot +0x60, `0x008CB4A0` land / `0x008CB570` naval; CONFIRMED):
    /// 1. a unit whose character is a General (army) / admiral (navy) comes first;
    /// 2. then the higher rank of the unit's character: his `command_land` / `command_sea`
    ///    attribute (character +0xA4 + 4·{0, 1}; INFERRED to be the saved `AgentAttributes` in
    ///    order, whose first two are these; compared unsigned, so -1 ranks highest; 0 without one);
    /// 3. then the lower unit category (`UNIT_RECORD` +0x1C from `units` #2, `0x00EED2B0`:
    ///    cavalry 0 (elephants count as 0), artillery 1, infantry 2, dragoons 3, camels 5,
    ///    ships of the line 6, frigates 7, galleys 8, specialists 9, auxiliaries 10, merchants 11,
    ///    invasion fleet 12);
    /// 4. land only: `units` #21 clear before set;
    /// 5. the older unit (its creation date, unit +0x58..+0x60): PROVISIONAL, not modelled (the
    ///    model keeps no raise date), skipped;
    /// 6. the higher `units` #7;
    /// 7. the unit created first (unit +4, a running serial): PROVISIONAL, the unit's place in the
    ///    force.
    pub fn commander_unit(&self, force: ForceId) -> Option<usize> {
        use super::world::CharacterKind;
        use std::cmp::Reverse;
        let f = self.world.forces.get(&force)?;
        let navy = f.is_navy;
        let (lead_kind, rank_key) = if navy { (CharacterKind::Admiral, "command_sea") } else { (CharacterKind::General, "command_land") };
        f.units
            .iter()
            .enumerate()
            .min_by_key(|(i, u)| {
                let ch = u.character.and_then(|c| self.world.characters.get(&c));
                let leads = ch.is_some_and(|c| c.kind == lead_kind);
                let rank = ch
                    .and_then(|c| self.world.character_details.get(&c.id))
                    .and_then(|d| d.attributes.iter().find(|a| a.0 == rank_key))
                    .map_or(0, |a| a.1 as u32);
                let r = self.rules.units.get(&u.unit_key);
                let category = r.map_or(1, |r| category_order(&r.category));
                let flag = !navy && r.is_some_and(|r| r.flag_21);
                let campaign_cost = r.map_or(0, |r| r.campaign_cost);
                (Reverse(leads), Reverse(rank), category, flag, Reverse(campaign_cost), *i)
            })
            .map(|(i, _)| i)
    }
}

/// The original's unit category number (`UNIT_RECORD` +0x1C, [`crate::unit_kind::category`]) as the
/// commander pick reads it (elephants, 4, count as 0; artillery and unknown keys are 1).
pub(super) fn category_order(category: &str) -> u32 {
    match crate::unit_kind::category(category) {
        crate::unit_kind::Category::Elephants => 0,
        c => u32::from(c.code()),
    }
}


impl CampaignModel {
    /// A character's turn-end counters (`0x009DA210`, CONFIRMED; run for every character of the
    /// faction at its turn end, before `CharacterTurnEnd`; CHARACTERS_FIDELITY.md §6):
    /// - turns in enemy lands (+0x508) + 1 when the region he stands in (`0x00A1BDF0`) belongs to a
    ///   faction at war with his (`0x008CE9B0`), else 0;
    /// - turns at home (+0x50C) + 1 when it belongs to his faction, else 0;
    /// - turns at sea (+0x504) + 1 for a naval character not in a port (`0x009D3F20` ≠ 2), else 0;
    /// - he acted when his action points differ from his turn's start (`0x00951540`): no-action
    ///   flag (+0x4C5) = not acted, and idle turns (+0x4C8) + 1 when he did not act.
    ///
    /// PROVISIONAL: "in a port" is a fleet within 1 unit of a port slot; a naval character is an
    /// admiral or captain or a navy's commander.
    pub fn turn_end_counters(&mut self, c: CharacterId) {
        let Some(ch) = self.world.characters.get(&c).cloned() else { return };
        let region = self.world.regions.values().find(|r| super::economy::in_region(self, r, &ch)).map(|r| r.owner);
        let enemy = region.is_some_and(|o| o != ch.faction && self.world.stance(ch.faction, o) == super::world::Stance::War);
        let home = region == Some(ch.faction);
        let naval = matches!(ch.kind, super::world::CharacterKind::Admiral | super::world::CharacterKind::Captain)
            || self.force_of(c).and_then(|f| self.world.forces.get(&f)).is_some_and(|f| f.is_navy);
        let in_port = self.port_at(ch.position).is_some();
        let acted = ch.movement_points != ch.max_movement_points || self.world.agents_acted.contains(&c);
        let Some(d) = self.world.character_details.get_mut(&c) else { return };
        d.turns_in_enemy_lands = if enemy { d.turns_in_enemy_lands + 1 } else { 0 };
        d.turns_at_home = if home { d.turns_at_home + 1 } else { 0 };
        d.turns_at_sea = if naval && !in_port { d.turns_at_sea + 1 } else { 0 };
        d.no_action = !acted;
        if !acted {
            d.idle_turns += 1;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_thresholds() {
        let mut r = CharacterRules::default();
        r.traits.insert("T".into(), TraitRule { thresholds: vec![(1, 1), (2, 4), (3, 8)], ..Default::default() });
        assert_eq!([0, 1, 3, 4, 7, 8, 50].map(|p| r.level("T", p)), [0, 1, 1, 2, 2, 3, 3]);
        assert_eq!(r.level("T", -5), 0);
        assert_eq!(r.level("missing", 9), 0);
    }

    #[test]
    fn death_hazards_from_the_bands() {
        let h = death_hazards();
        assert_eq!(h[50], 0.0);
        // 51: 0.6 % density of the 100 % still alive.
        assert!((h[51] - 0.006).abs() < 1e-6);
        // 70: 5.4 / (5.4 + 16 + 12 + 8 + 6 + 3 + 1).
        assert!((h[70] - 5.4 / 51.4).abs() < 1e-5);
        assert_eq!(h[100], 1.0);
        let mut rng = CaRng::new(1);
        assert!(dies_this_year(&mut rng, &h, 101));
        assert!(!dies_this_year(&mut rng, &h, 30));
    }

    #[test]
    fn script_rngs_start_at_the_exe_seed() {
        let s = ScriptRngs::default();
        assert_eq!((s.trait_rng.state, s.ancillary_rng.state), (0x61266, 0x61266));
    }
}

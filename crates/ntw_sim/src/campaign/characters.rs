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
//! Traits are stored as (key, points); the level is derived from the points
//! ([`EffectRules::trait_level_key`](super::effects::EffectRules::trait_level_key)), so the
//! no-going-back rule is kept by never letting points fall below that level's threshold.

use std::collections::BTreeMap;

use super::details::CharacterTrait;
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
    /// `character_traits` #2 (INFERRED hidden).
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
        }
    }
}

/// Default of the `max_traits` tweak.
pub const MAX_TRAITS: usize = 6;
/// Default of the `max_ancillaries` tweak.
pub const MAX_ANCILLARIES: usize = 3;
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

    /// The level (1..) a trait is at with `points`: the highest whose threshold ≤ max(points, 0), 0 when
    /// below the first (CONFIRMED, `0x008B5380`).
    pub fn level(&self, trait_key: &str, points: i32) -> i32 {
        let p = points.max(0);
        self.traits
            .get(trait_key)
            .and_then(|t| t.thresholds.iter().rev().find(|(_, th)| *th <= p))
            .map_or(0, |(lvl, _)| *lvl)
    }

    fn threshold(&self, trait_key: &str, level: i32) -> Option<i32> {
        self.traits.get(trait_key)?.thresholds.iter().find(|(l, _)| *l == level).map(|(_, th)| *th)
    }
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
        let before = cr.level(&held, d.traits[i].points);
        d.traits[i].points -= p;
        let overflow = if d.traits[i].points < 0 { -d.traits[i].points } else { 0 };
        // The no-going-back level holds once reached (`0x008B5380`): points reset to its threshold.
        let ngb = cr.traits.get(&held).map_or(0, |h| h.no_going_back_level);
        if ngb > 0 && before >= ngb && cr.level(&held, d.traits[i].points) < ngb
            && let Some(th) = cr.threshold(&held, ngb)
        {
            d.traits[i].points = th;
        }
        p = d.traits[i].points;
        if d.traits[i].points < 1 {
            d.traits.remove(i);
        } else {
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
    if let Some(t) = d.traits.iter_mut().find(|t| t.key == key) {
        let from_level = cr.level(key, t.points);
        t.points += p;
        return TraitOutcome::Raised { from_level, to_level: cr.level(key, t.points) };
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
            evicted = Some(d.traits.remove(i).key);
        }
    }
    if d.traits.len() >= cr.max_traits {
        return TraitOutcome::NoRoom;
    }
    let pts = p.max(0);
    d.traits.push(CharacterTrait { key: key.to_string(), points: pts });
    TraitOutcome::Added { level: cr.level(key, pts), evicted }
}

/// `remove_trait` (binding `0x008E9F50`): drops a held trait. Returns whether it was held.
pub fn remove_trait(model: &mut CampaignModel, c: CharacterId, key: &str) -> bool {
    let Some(d) = model.world.character_details.get_mut(&c) else { return false };
    let n = d.traits.len();
    d.traits.retain(|t| t.key != key);
    let changed = d.traits.len() != n;
    if changed {
        model.update_sight_radius(c);
    }
    changed
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
    let evicted = evict.map(|i| d.ancillaries.remove(i));
    d.ancillaries.push(key.to_string());
    model.update_sight_radius(c);
    Ok(evicted)
}

/// `remove_ancillary` (binding `0x008E9990`): drops a held ancillary. Returns whether it was held.
pub fn remove_ancillary(model: &mut CampaignModel, c: CharacterId, key: &str) -> bool {
    let Some(d) = model.world.character_details.get_mut(&c) else { return false };
    let n = d.ancillaries.len();
    d.ancillaries.retain(|a| a != key);
    let changed = d.ancillaries.len() != n;
    if changed {
        model.update_sight_radius(c);
    }
    changed
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
/// last (`turn_in_year + 1 == turns_per_year`). For every faction, every character: a death check by
/// age (current year − birth year) unless he appeared less than 3 turns ago, then his ancillaries whose
/// end year has come are dropped (`0x009D2E50`); the dead are removed after the faction's pass
/// (`0x00A0C6F0` reason 3, natural causes).
///
/// Characters with +0x52C (`CHARACTER` #34, [`super::details::CharacterDetails::returns_after_death`])
/// are skipped without a draw. A dying General's own unit is deleted first; a force he commanded
/// goes to a successor ([`CampaignModel::command_vacated`]).
///
/// Each living faction's family has its year first ([`super::family::family_year`]); a post whose
/// holder dies gets a new holder at once ([`CampaignModel::refill_post`]).
///
/// PROVISIONAL: the "appeared" turn (+0x4EC) is not in the save, so loaded characters count as old;
/// characters without a birth date are skipped; a "living faction" is one that owns a region
/// (INFERRED stand-in for the exe's destroyed flag, `0x008CEEF0`).
pub fn yearly_character_pass(model: &mut CampaignModel) -> YearlyPass {
    let mut out = YearlyPass::default();
    if model.calendar.turn_in_year + 1 != model.calendar.turns_per_year {
        return out;
    }
    let hazards = death_hazards();
    let year = model.calendar.date.year as i32;
    let rules = model.rules.clone();
    for f in model.world.factions_in_turn_order() {
        if model.world.regions.values().any(|r| r.owner == f) {
            super::family::family_year(model, f, &hazards);
        }
        let ids: Vec<CharacterId> = model.world.characters.values().filter(|c| c.faction == f).map(|c| c.id).collect();
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
                d.ancillaries.retain(|a| {
                    let keep = rules.characters.ancillaries.get(a).is_none_or(|r| year < r.end_year);
                    if !keep {
                        out.expired.push((c, a.clone()));
                    }
                    keep
                });
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
    pub fn character_dies(&mut self, c: CharacterId) {
        let Some(dead) = self.world.characters.remove(&c) else { return };
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
        self.command_vacated(&dead);
        for (f, i) in vacated {
            self.refill_post(f, i);
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
    /// original save, SAVE_COMPAT.md §23); the save writer names a new colonel after his unit's
    /// officer.
    pub(crate) fn command_vacated(&mut self, old: &super::world::Character) {
        use super::world::{Character, CharacterKind};
        let commanded: Vec<ForceId> = self.world.forces.values().filter(|f| f.commander == Some(old.id)).map(|f| f.id).collect();
        for fid in commanded {
            let (navy, empty) = (self.world.forces[&fid].is_navy, self.world.forces[&fid].units.is_empty());
            if let Some(f) = self.world.forces.get_mut(&fid) {
                f.commander = None;
            }
            if empty {
                self.world.forces.remove(&fid);
                for r in self.world.regions.values_mut() {
                    if r.garrison == Some(fid) {
                        r.garrison = None;
                    }
                    if r.fleet == Some(fid) {
                        r.fleet = None;
                    }
                }
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
            self.update_sight_radius(id);
            let f = self.world.forces.get_mut(&fid).expect("force exists");
            f.commander = Some(id);
            f.units[ui].character = Some(id);
        }
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
                let value_7 = r.map_or(0, |r| r.value_7);
                (Reverse(leads), Reverse(rank), category, flag, Reverse(value_7), *i)
            })
            .map(|(i, _)| i)
    }
}

/// The original's unit category number (`UNIT_RECORD` +0x1C, `0x00EED2B0`, CONFIRMED) as the
/// commander pick reads it (elephants, 4, count as 0; artillery and unknown keys are 1).
fn category_order(category: &str) -> u32 {
    match category {
        "cavalry" | "elephants" => 0,
        "infantry" => 2,
        "dragoons" => 3,
        "cavalry_camels" => 5,
        "naval_line_of_battle" => 6,
        "naval_frigate" => 7,
        "naval_galley" => 8,
        "naval_specialist" => 9,
        "naval_auxiliary" => 10,
        "naval_merchant" => 11,
        "naval_invasion_fleet" => 12,
        _ => 1,
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
        let in_port = self.world.regions.values().flat_map(|r| r.slots.iter()).filter(|s| s.port).filter_map(|s| s.position).any(|p| {
            let (dx, dz) = (p.0.to_f32() - ch.position.0.to_f32(), p.1.to_f32() - ch.position.1.to_f32());
            dx * dx + dz * dz < 1.0
        });
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

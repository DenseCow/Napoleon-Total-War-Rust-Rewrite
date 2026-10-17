//! The campaign director (`CDIR_INTERFACE`, campaign +0xF80): the original's background rules that
//! steer the campaign around the human players. Its one action that changes the map is
//! `CDIR_INTENTION_TRANSFER_REGION_OWNERSHIP` (`0x00B93840`): a minor faction's region far from any
//! human's eyes is handed to a major AI power at war with it, the garrison destroyed. Its other
//! desires (at war, boost, XP, replenish; [`director_desires`]) draw from the same RNG, so their
//! draws are ported even where their effect is not (CAMPAIGN_FIDELITY.md §Campaign director).
//!
//! ## When it runs (CONFIRMED, `AdvanceFactionTurn` `0x008A9920`)
//! At each turn handover, after the starting faction's turn start, [`CampaignModel::director_handover`]
//! ([`super::TurnStep::Handover`]):
//! 1. the queued transfers whose receiver is the starting faction are carried out (`0x00BA90E0` →
//!    `0x00BCA430` → `0x00B58A30`): the region changes hands when its owner is still the one it had
//!    when the transfer was queued and it is still [transferable](transferable); every such entry
//!    leaves the queue either way. An entry whose receiver is out of the game stays queued (and saved):
//!    the turn passes over that faction (`0x00948CF0`, [`CampaignModel::takes_turns`]), so it starts no
//!    turn until it re-emerges, and nothing else purges the queue (CONFIRMED: its only other users are
//!    the step that queues and the save code);
//! 2. when the ending faction is human and the starting one is not (`0x00BC6DF0`), the director's
//!    pool runs (`0x00BA9220` via `0x008F1F40`, loop `0x00BBC880`): the beliefs (each faction's
//!    rival score, [`rival_score`]) are refreshed, then the desires in score order; the
//!    region-transfer desire takes one step ([`region_transfer_step`], `0x00BAE0A0`) through the
//!    seam [`super::seams::DIRECTOR_REGION_TRANSFER`], then the others ([`director_desires`])
//!    through [`super::seams::DIRECTOR_DESIRES`].
//!
//! ## The desire step (`0x00BAE0A0`, CONFIRMED)
//! The chance is `BASE_CHANCE + EXTRA_CHANCE_PER_TURN × counter` (f32, clamped to [0, 1]; the counter
//! counts the failed rolls since the last transfer). A roll in [0, 1] on the director's RNG above the
//! chance adds one to the counter. Otherwise the candidates ([`candidates`]) are gathered; with none,
//! nothing changes; else a second roll in [0, total weight] picks one, the transfer is queued for its
//! receiver's next turn start and the counter goes back to 0.
//!
//! ## The map areas (CONFIRMED)
//! The director walks the CAI world's areas (`CAI_WORLD_REGIONS`, list +0x130/+0x134: land regions,
//! seas, theatres) in their order and their links (`CAI_REGION` #7, the other side by `0x00C1BCE0`),
//! imported as [`super::World::ai_areas`]. In the Europe start position they differ from the map's
//! region neighbours (`regions.esf` connectivity): 52 of the 72 land regions have the same set,
//! 18 the same order, and seas link to land (test `ai_areas_on_the_europe_start`). A map without them (a custom map) uses its regions in
//! id order with [`super::World::region_neighbours`]. The factions are walked in the CAI world's
//! order ([`super::World::ai_factions`]; a map without it uses faction id order).
//!
//! Evidence tags: CONFIRMED unless marked. PROVISIONAL: the transferable test's siege test is never
//! true (no sieges in the model, [`transferable_at`]); the effects the model has no system for
//! (the autoresolver modifiers, unit experience, the teleport desire) and
//! the desires whose gathers are not traced ([`director_desires`]), so the RNG's sequence matches
//! the original's only until one of those draws. PROVISIONAL: of the exe's many readers of the
//! relation belief (`GetCaiRelationBelief` `0x00A74A20`, 185 callers) the model has only the
//! AI's evaluation of a proposed deal ([`CampaignModel::refresh_relation_belief`]), so a belief
//! another reader would have stored during a human's turn is rebuilt later in ours, possibly from
//! newer stances ([`DirectorState::beliefs`]). The transfer expels the other factions' armies and
//! docked navies from the region as every owner change does (`0x00A64AC0` → `0x008D9DC0`,
//! [`super::expulsion`]; `director_transfer`). INFERRED: the turn change's mark
//! of every belief ([`CampaignModel::director_handover`]). INFERRED: the transfer's garrison reads
//! the region recruitment beliefs from the current state where the original reads its cache
//! ([`land_recruitment_belief`]).

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use super::events::CampaignEvent;
use super::ids::{FactionId, RegionId};
use super::mod_state::ModWrites;
use super::world::{CampaignModel, Region, Stance};
use crate::fnv::Fnv64;
use crate::rng::CaRng;

/// The director RNG's seed set by the `CDIR_INTERFACE` constructor (+0x60). A start position or save
/// then sets the RNG from its `CDIR_INTERFACE` record (the vanilla Europe start position holds 37138;
/// `ntw_campaign` director.rs), so this seed stands only for a campaign without that record.
pub const RNG_SEED: u32 = 0x0006_1266;

/// The `cdir_configs` wildcard for the campaign and faction columns.
pub const ALL: &str = "all";

/// `CDIR_CVN_PLAYER_RIVAL_SCORE_DIPLOMATIC_RELATION_PCT` (cv 0x18).
pub const RIVAL_RELATION_PCT: &str = "CDIR_CVN_PLAYER_RIVAL_SCORE_DIPLOMATIC_RELATION_PCT";
/// `CDIR_CVN_PLAYER_RIVAL_SCORE_VICTORY_REGIONS_PCT` (cv 0x19).
pub const RIVAL_VICTORY_REGIONS_PCT: &str = "CDIR_CVN_PLAYER_RIVAL_SCORE_VICTORY_REGIONS_PCT";
/// `CDIR_CVN_PLAYER_RIVAL_SCORE_PROXIMITY_PCT` (cv 0x1A).
pub const RIVAL_PROXIMITY_PCT: &str = "CDIR_CVN_PLAYER_RIVAL_SCORE_PROXIMITY_PCT";
/// `CDIR_CVN_PLAYER_RIVAL_SCORE_MINOR_FACTION_WEIGHT` (cv 0x1B).
pub const RIVAL_MINOR_FACTION_WEIGHT: &str = "CDIR_CVN_PLAYER_RIVAL_SCORE_MINOR_FACTION_WEIGHT";
/// `CDIR_CVN_TRANSFER_REGION_OWNERSHIP_BASE_CHANCE` (cv 0x2C).
pub const TRANSFER_BASE_CHANCE: &str = "CDIR_CVN_TRANSFER_REGION_OWNERSHIP_BASE_CHANCE";
/// `CDIR_CVN_TRANSFER_REGION_OWNERSHIP_EXTRA_CHANCE_PER_TURN` (cv 0x2D).
pub const TRANSFER_EXTRA_CHANCE_PER_TURN: &str = "CDIR_CVN_TRANSFER_REGION_OWNERSHIP_EXTRA_CHANCE_PER_TURN";
/// `CDIR_CVN_TRANSFER_REGION_OWNERSHIP_UBER_FACTION_MODIFIER` (cv 0x2E).
pub const TRANSFER_UBER_FACTION_MODIFIER: &str = "CDIR_CVN_TRANSFER_REGION_OWNERSHIP_UBER_FACTION_MODIFIER";
/// `CDIR_CVN_UBER_FACTION`: the flag that puts a faction on the director's uber list (`0x00BAE300`).
pub const UBER_FACTION: &str = "CDIR_CVN_UBER_FACTION";
/// `CDIR_CVN_BOOST_PLAYER_RIVALS_PERCENTAGE` (cv 0x28, float).
pub const BOOST_PERCENTAGE: &str = "CDIR_CVN_BOOST_PLAYER_RIVALS_PERCENTAGE";
/// `CDIR_CVN_BOOST_PLAYER_RIVALS_MIN` (cv 0x29, unsigned).
pub const BOOST_MIN: &str = "CDIR_CVN_BOOST_PLAYER_RIVALS_MIN";
/// `CDIR_CVN_BOOST_PLAYER_RIVALS_MAX` (cv 0x2A, unsigned).
pub const BOOST_MAX: &str = "CDIR_CVN_BOOST_PLAYER_RIVALS_MAX";
/// `CDIR_CVN_BOOST_PLAYER_RIVALS_AMOUNT` (cv 0x2B, a faction's signed int).
pub const BOOST_AMOUNT: &str = "CDIR_CVN_BOOST_PLAYER_RIVALS_AMOUNT";
/// `CDIR_CVN_REPLENISH_UNITS_FACTION_ARMY_ALLOWED` (cv 0x36, a faction's float).
pub const REPLENISH_FACTION_ARMY_ALLOWED: &str = "CDIR_CVN_REPLENISH_UNITS_FACTION_ARMY_ALLOWED";
/// `CDIR_CVN_REPLENISH_UNITS_MOBILE_ARMY_ALLOWED` (cv 0x38, a faction's float).
pub const REPLENISH_MOBILE_ARMY_ALLOWED: &str = "CDIR_CVN_REPLENISH_UNITS_MOBILE_ARMY_ALLOWED";
/// `CDIR_CVN_REPLENISH_UNITS_UNIT_ARMY_ALLOWED` (cv 0x3A, a faction's float).
pub const REPLENISH_UNIT_ARMY_ALLOWED: &str = "CDIR_CVN_REPLENISH_UNITS_UNIT_ARMY_ALLOWED";
/// `CDIR_CVN_REPLENISH_UNITS_ARMY_MIN_PCT` (cv 0x3C, a faction's float).
pub const REPLENISH_ARMY_MIN_PCT: &str = "CDIR_CVN_REPLENISH_UNITS_ARMY_MIN_PCT";
/// `CDIR_CVN_REPLENISH_UNITS_ARMY_MAX_PCT` (cv 0x3D, a faction's float).
pub const REPLENISH_ARMY_MAX_PCT: &str = "CDIR_CVN_REPLENISH_UNITS_ARMY_MAX_PCT";
/// `CDIR_CVN_UNIT_XP_UPGRADE_ARMY_BASE_CHANCE` (cv 0x40, float).
pub const XP_ARMY_BASE_CHANCE: &str = "CDIR_CVN_UNIT_XP_UPGRADE_ARMY_BASE_CHANCE";
/// `CDIR_CVN_UNIT_XP_UPGRADE_ARMY_EXTRA_CHANCE_PER_TURN` (cv 0x42, float).
pub const XP_ARMY_EXTRA_CHANCE_PER_TURN: &str = "CDIR_CVN_UNIT_XP_UPGRADE_ARMY_EXTRA_CHANCE_PER_TURN";

/// The XP desire's draws per unit rank (`0x00BD4A60`): the (min, max) table at `0x01380978` (ranks
/// 0..=9: the first `min` of `max` steps always count, each other one counts when a roll in [0, 1]
/// is at most the rank's chance, [`XP_RANK_CHANCE`]).
const XP_RANK_STEPS: [(u32, u32); 10] = [(0, 3), (0, 2), (0, 1), (0, 1), (0, 1), (0, 1), (0, 1), (0, 1), (0, 1), (0, 0)];
/// The chance per rank at `0x0138098C`.
const XP_RANK_CHANCE: [f32; 10] = [0.7, 0.6, 0.6, 0.5, 0.4, 0.2, 0.15, 0.1, 0.05, 0.0];

/// Reads a `cdir_configs` float as the exe does (`0x004F38A0`, both float lookups; CONFIRMED): an
/// optional leading `-`; integer digits (`v = v × 10 + d`, f32); a `.` is required (without one the
/// read fails: `"10"` reads as 0); fraction digits (`f = f × 0.1`, `v = d × f + v`); then an
/// optional `e` / `E`, an optional `-` (no `+`) and digits, `v` multiplied by 0.1 (after `-`) or
/// 10 that many times; the text must end there (no spaces). `None` when it does not read.
pub fn parse_cdir_float(s: &str) -> Option<f32> {
    let b = s.as_bytes();
    let mut i = 0;
    let neg = b.first() == Some(&b'-');
    if neg {
        i = 1;
    }
    let digit = |i: usize| b.get(i).filter(|c| c.is_ascii_digit()).map(|c| (c - b'0') as f32);
    let mut v = 0.0f32;
    while let Some(d) = digit(i) {
        v = v * 10.0 + d;
        i += 1;
    }
    if b.get(i) != Some(&b'.') {
        return None;
    }
    i += 1;
    let mut f = 1.0f32;
    while let Some(d) = digit(i) {
        f *= 0.1;
        v += d * f;
        i += 1;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        let neg_exp = b.get(i) == Some(&b'-');
        if neg_exp {
            i += 1;
        }
        let mut exp = 0u32;
        while let Some(c) = b.get(i).filter(|c| c.is_ascii_digit()) {
            exp = exp.wrapping_mul(10).wrapping_add((c - b'0') as u32);
            i += 1;
        }
        let step = if neg_exp { 0.1f32 } else { 10.0 };
        for _ in 0..exp {
            // Once 0 or infinite it stays so: the rest of the loop changes nothing.
            if v == 0.0 || v.is_infinite() {
                break;
            }
            v *= step;
        }
    }
    if i != b.len() {
        return None;
    }
    Some(if neg { -v } else { v })
}

/// Reads a `cdir_configs` unsigned int as the exe does (`0x004F37B0`, the `all` int lookup
/// `0x00BD2920`; CONFIRMED): digits only (no sign, no spaces), wrapping at 32 bits; an empty text
/// reads as 0. `None` when another character is found.
pub fn parse_cdir_uint(s: &str) -> Option<u32> {
    s.bytes().try_fold(0u32, |v, c| c.is_ascii_digit().then(|| v.wrapping_mul(10).wrapping_add((c - b'0') as u32)))
}

/// Reads a `cdir_configs` signed int as the exe does (`0x004F3720`, a faction's int lookup
/// `0x00BD29F0`; CONFIRMED): an optional leading `-`, then [`parse_cdir_uint`]'s digits, negated.
pub fn parse_cdir_int(s: &str) -> Option<i32> {
    match s.strip_prefix('-') {
        Some(rest) => parse_cdir_uint(rest).map(|v| (v as i32).wrapping_neg()),
        None => parse_cdir_uint(s).map(|v| v as i32),
    }
}

/// One `cdir_configs` row. A missing faction or value reads as an empty string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CdirConfig {
    /// #0: campaign key, or [`ALL`].
    pub campaign: String,
    /// #1: faction key, or [`ALL`].
    pub faction: String,
    /// #2: the variable.
    pub variable: String,
    /// #3: the value as text.
    pub value: String,
}

/// The weights of the CAI relation value (`0x00A7A2E0`; the constructor `0x00B88280` of the object
/// at vtable `0x0137F8BC` sets them, +0x90..+0xA0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelationWeights {
    /// +0x90: the attitude, mapped to [0, 1].
    pub attitude: f32,
    /// +0x94: allied.
    pub allied: f32,
    /// +0x98: military access.
    pub access: f32,
    /// +0x9C: a trade agreement.
    pub trade: f32,
    /// +0xA0: not at war.
    pub peace: f32,
}

impl Default for RelationWeights {
    /// The original's (`0x00B88280`).
    fn default() -> Self {
        RelationWeights { attitude: 0.7, allied: 0.2, access: 0.05, trade: 0.05, peace: 0.2 }
    }
}

/// The director's game data (in [`super::CampaignRules::director`]).
#[derive(Debug, Clone, PartialEq)]
pub struct DirectorRules {
    /// `cdir_configs`, in table order (the lookup takes the first matching row).
    pub configs: Vec<CdirConfig>,
    /// The CAI relation value's weights.
    pub relation_weights: RelationWeights,
    /// The faction keys whose rival score is always 0 besides the rebels (the original's importer
    /// sets the key `0x00C3FD50` tests).
    pub no_rival_keys: Vec<String>,
}

impl Default for DirectorRules {
    /// No config rows (every value 0, so no transfer ever happens), the original's relation weights
    /// and no listed keys.
    fn default() -> Self {
        DirectorRules { configs: Vec::new(), relation_weights: RelationWeights::default(), no_rival_keys: Vec::new() }
    }
}

impl DirectorRules {
    /// The value of the first row for (campaign, faction, variable), then (campaign, [`ALL`],
    /// variable) when the faction is not [`ALL`], then ([`ALL`], [`ALL`], variable) when the
    /// campaign is not [`ALL`] (`0x00BD2A40`: each pass scans the rows in table order).
    pub fn value(&self, campaign: &str, faction: &str, variable: &str) -> Option<&str> {
        let find = |c: &str, f: &str| self.configs.iter().find(|r| r.campaign == c && r.faction == f && r.variable == variable).map(|r| r.value.as_str());
        find(campaign, faction)
            .or_else(|| if faction != ALL { find(campaign, ALL) } else { None })
            .or_else(|| if campaign != ALL { find(ALL, ALL) } else { None })
    }

    /// A float variable (`0x00BD29B0`: always looked up with faction [`ALL`]); 0 when there is no
    /// row or its value does not read ([`parse_cdir_float`]).
    pub fn float(&self, campaign: &str, variable: &str) -> f32 {
        self.faction_float(campaign, ALL, variable)
    }

    /// A faction's float variable (`0x00BD2960`), read as [`DirectorRules::float`].
    pub fn faction_float(&self, campaign: &str, faction: &str, variable: &str) -> f32 {
        self.value(campaign, faction, variable).and_then(parse_cdir_float).unwrap_or(0.0)
    }

    /// An unsigned int variable (`0x00BD2920`: faction [`ALL`]); 0 when there is no row or its
    /// value does not read ([`parse_cdir_uint`]).
    pub fn uint(&self, campaign: &str, variable: &str) -> u32 {
        self.value(campaign, ALL, variable).and_then(parse_cdir_uint).unwrap_or(0)
    }

    /// A faction's signed int variable (`0x00BD29F0`); 0 when there is no row or its value does
    /// not read ([`parse_cdir_int`]).
    pub fn faction_int(&self, campaign: &str, faction: &str, variable: &str) -> i32 {
        self.value(campaign, faction, variable).and_then(parse_cdir_int).unwrap_or(0)
    }

    /// A flag variable for a faction (`0x00BD2870`): true only for the value `1`.
    pub fn flag(&self, campaign: &str, faction: &str, variable: &str) -> bool {
        self.value(campaign, faction, variable) == Some("1")
    }
}

/// A queued region transfer (`CDIR_INTERFACE_REGION_OWNERSHIP_CHANGES`, +0x48: 12-byte entries).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingTransfer {
    /// The faction that receives the region at its next turn start (an entry waits while the
    /// receiver is out of the game, which starts no turn).
    pub receiver: FactionId,
    /// The region.
    pub region: RegionId,
    /// The region's owner when the transfer was queued: the transfer happens only if it still is.
    pub queued_owner: FactionId,
}

/// A faction's CAI relation belief (entry `+8` of the faction's `+0x100` list, found by
/// `0x00A89E00`, read through `0x00A74A20`), as `BuildCaiRelationBelief` `0x00ABAFB0` builds it
/// ([`build_relation_belief`]). CONFIRMED.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RelationBelief {
    /// The relation value ([`relation_value`]) towards each other faction of the CAI world, in
    /// [`ai_order`].
    pub relations: Vec<(FactionId, f32)>,
    /// The enemy list (count `+0x4C`, data `+0x50`): the CAI world's factions in [`ai_order`],
    /// without the owner, that are the rebels or in the game and at war with it
    /// ([`CampaignModel::at_war`]).
    pub enemies: Vec<FactionId>,
}

/// The director's state (`CDIR_INTERFACE`): saved and hashed.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirectorState {
    /// +0x60: the director's own RNG (read from the start position or save; [`RNG_SEED`] without one;
    /// an original record older than version 2 seeds it from the campaign RNG).
    pub rng: CaRng,
    /// The region-transfer desire's +0xFC: failed rolls since the last transfer (u32, wrapping).
    pub transfer_counter: u32,
    /// +0x48: the queued transfers, in queue order.
    pub pending: Vec<PendingTransfer>,
    /// +0x34 (`CDIR_INTERFACE_AUTORESOLVER_MODIFIERS`): the (winner, loser) pairs the at-war desire
    /// ([`at_war_step`]) picked, at most one per pair of factions, in list order.
    #[cfg_attr(feature = "serde", serde(default))]
    pub autoresolver_pairs: Vec<(FactionId, FactionId)>,
    /// The XP desire's failed rolls per faction since its last upgrade ([`xp_step`];
    /// `CDIR_DESIRE_UNIT_XP_UPGRADER_FACTION_COUNT`). A faction not listed has 0.
    #[cfg_attr(feature = "serde", serde(default))]
    pub xp_counters: BTreeMap<FactionId, u32>,
    /// The clean CAI relation beliefs ([`RelationBelief`]; the exe keeps them in the CAI, not in
    /// `CDIR_INTERFACE`, and the director reads them): a faction listed has a clean belief, stored
    /// by a read ([`CampaignModel::refresh_relation_belief`]); a faction not listed has a dirty one,
    /// which the next read rebuilds ([`relation_belief_value`], [`believes_at_war`]). Every
    /// diplomacy change empties it ([`CampaignModel::diplomacy_changed`]), and so does each turn
    /// change ([`CampaignModel::director_handover`]). An original save's beliefs are not read: all
    /// start dirty (INFERRED: the CAI file's belief records are not imported).
    #[cfg_attr(feature = "serde", serde(default))]
    pub beliefs: BTreeMap<FactionId, RelationBelief>,
}

impl Default for DirectorState {
    /// A new campaign's.
    fn default() -> Self {
        DirectorState {
            rng: CaRng::new(RNG_SEED),
            transfer_counter: 0,
            pending: Vec::new(),
            autoresolver_pairs: Vec::new(),
            xp_counters: BTreeMap::new(),
            beliefs: BTreeMap::new(),
        }
    }
}

impl DirectorState {
    pub(crate) fn hash_into(&self, h: &mut Fnv64) {
        h.u32(self.rng.state);
        h.u32(self.transfer_counter);
        h.u32(self.pending.len() as u32);
        for p in &self.pending {
            h.i32(p.receiver.raw());
            h.u32(p.region.raw());
            h.i32(p.queued_owner.raw());
        }
        h.u32(self.autoresolver_pairs.len() as u32);
        for (w, l) in &self.autoresolver_pairs {
            h.i32(w.raw());
            h.i32(l.raw());
        }
        h.u32(self.xp_counters.len() as u32);
        for (f, n) in &self.xp_counters {
            h.i32(f.raw());
            h.u32(*n);
        }
        h.u32(self.beliefs.len() as u32);
        for (f, b) in &self.beliefs {
            h.i32(f.raw());
            h.u32(b.relations.len() as u32);
            for (g, v) in &b.relations {
                h.i32(g.raw());
                h.u32(v.to_bits());
            }
            h.u32(b.enemies.len() as u32);
            for g in &b.enemies {
                h.i32(g.raw());
            }
        }
    }
}

/// A region-transfer candidate ([`candidates`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// The region.
    pub region: RegionId,
    /// The faction that would receive it.
    pub receiver: FactionId,
    /// The weight: the receiver's rival score, times the uber modifier for an uber faction.
    pub weight: f32,
}

/// On the director's uber list (`0x00BB5480`; the list `0x00BAE300` holds the factions whose
/// [`UBER_FACTION`] flag is set).
pub fn is_uber(model: &CampaignModel, f: FactionId) -> bool {
    let Some(faction) = model.world.factions.get(&f) else { return false };
    model.rules.director.flag(&model.rules.campaign, &faction.key, UBER_FACTION)
}

/// The CAI relation value of `f` towards `x` in [0, 1] (`0x00A7A2E0`): 0 when either is the rebels;
/// else the weighted mean of the attitude mapped to [0, 1], allied, military access, a trade
/// agreement and not at war ([`RelationWeights`]), in f32 steps.
///
/// ORIGINAL BUG (`0x00A7A2E0`): the attitude's mapping has a branch for below 0 and one for above 0
/// but none for exactly 0, which falls through to 0 ("hostile") instead of the midpoint 0.5 both
/// branches meet at. Ours gives 0.5.
pub fn relation_value(model: &CampaignModel, f: FactionId, x: FactionId) -> f32 {
    if model.is_rebel_faction(f) || model.is_rebel_faction(x) {
        return 0.0;
    }
    let rel = model.world.relationships.get(&(f, x));
    let att = rel.map_or(0, super::details::Relationship::attitude_total);
    let att01 = if att < -200 {
        0.0
    } else if att < 0 {
        0.0025 * att as f32 + 0.5
    } else if att <= 100 {
        // att == 0 included: see ORIGINAL BUG above.
        0.005 * att as f32 + 0.5
    } else {
        1.0
    };
    let w = &model.rules.director.relation_weights;
    let stance = model.world.stance(f, x);
    let flag = |b: bool| if b { 1.0f32 } else { 0.0 };
    let mut v = w.attitude * att01;
    v += w.allied * flag(stance == Stance::Allied);
    v += w.access * flag(rel.is_some_and(|r| r.military_access_turns != 0));
    v += w.trade * flag(rel.is_some_and(|r| r.trade_agreement));
    v -= w.peace * flag(stance == Stance::War);
    v += w.peace;
    v / (w.attitude + w.allied + w.access + w.trade + w.peace)
}

/// `f`'s CAI relation belief as `BuildCaiRelationBelief` `0x00ABAFB0` builds it from the CAI
/// world's faction list (CAI world `+0x110`: count `+0x11C`, data `+0x120`; [`ai_order`]): the
/// relation value towards each other faction ([`relation_value`]) and the enemy list
/// ([`on_enemy_list`]). CONFIRMED.
pub fn build_relation_belief(model: &CampaignModel, f: FactionId) -> RelationBelief {
    let order = ai_order(model);
    let others = || order.iter().copied().filter(move |&g| g != f);
    RelationBelief {
        relations: others().map(|g| (g, relation_value(model, f, g))).collect(),
        enemies: others().filter(|&g| on_enemy_list(model, f, g)).collect(),
    }
}

/// Whether a rebuild of `f`'s relation belief puts `g` (not `f`) on its enemy list (`0x00ABAFB0`):
/// the rebels (`0x00C3FE80`) always; any other faction when it is in the game (`+0x824` clear,
/// [`CampaignModel::in_the_game`]) and `f`'s stance towards it is war (`0x00B64C50` record
/// `+0x790 → +0x10` = 0, [`CampaignModel::at_war`]). CONFIRMED.
fn on_enemy_list(model: &CampaignModel, f: FactionId, g: FactionId) -> bool {
    model.is_rebel_faction(g) || (model.in_the_game(g) && model.at_war(f, g))
}

/// `f`'s relation towards `x` as a read of `f`'s relation belief gets it (`0x00A74A20`): the stored
/// value while the belief is clean ([`DirectorState::beliefs`]), else the value the read's rebuild
/// gives ([`relation_value`], what [`build_relation_belief`] stores). The pool's reads keep
/// nothing: the turn change after the pool marks every belief dirty anyway
/// ([`CampaignModel::director_handover`]).
pub fn relation_belief_value(model: &CampaignModel, f: FactionId, x: FactionId) -> f32 {
    let stored = model.director.beliefs.get(&f).and_then(|b| b.relations.iter().find(|e| e.0 == x));
    stored.map_or_else(|| relation_value(model, f, x), |e| e.1)
}

/// Whether `g` is on `f`'s relation belief's enemy list (`0x00A649F0` on the belief
/// `0x00A74A20` returns): the stored list while the belief is clean, else what the read's rebuild
/// lists ([`on_enemy_list`]). False for `g` = `f`.
pub fn believes_at_war(model: &CampaignModel, f: FactionId, g: FactionId) -> bool {
    match model.director.beliefs.get(&f) {
        Some(b) => b.enemies.contains(&g),
        None => g != f && on_enemy_list(model, f, g),
    }
}

/// A faction's player rival score (`0x00BD21B0`, the belief stored at +0x100): how much of a rival
/// to the human players it is, the highest over the humans of
/// `m × (RELATION_PCT × (1 − relation) + VICTORY_REGIONS_PCT × b + PROXIMITY_PCT × c)`, capped at
/// 1, where `b` is the share of its regions the human's victory conditions name, `c` the share of
/// its regions' neighbours the human owns (its own regions count among them), and `m` 1 for a
/// major power, else `MINOR_FACTION_WEIGHT`. 0 for the rebels, the [`DirectorRules::no_rival_keys`],
/// a human and a faction out of the game, and with no humans.
///
/// The relation is read from the faction's CAI relation belief ([`relation_belief_value`]), a
/// stored copy that `0x00ABAFB0` rebuilds only when it is read while dirty (`0x00CCCF70`;
/// CONFIRMED). The model's diplomacy-changed event (`+0x3A8`, `0x008DBAC0`) marks every belief
/// dirty through its CAI listener (`0x00A8F320` → `0x00A88CE0`); the model fires it at each
/// broadcaster of the exe it models ([`CampaignModel::diplomacy_changed`] lists them: war, peace,
/// the treaties, gifts and payments, a government change, a capture, an agent's detected or blamed
/// misdeed, each faction's round-end drift `0x00B29100`), so a war the human declares mid-turn is
/// seen (test `a_war_declared_mid_turn_counts_for_the_rival_score`). A change
/// without the broadcast leaves a clean belief stale: an embargo without a trade agreement
/// (`0x00B28DB0`) after a deal proposal stored the belief ([`CampaignModel::propose_deal`]) is not
/// seen (test `an_embargo_after_a_proposal_is_not_seen_by_the_rival_score`). The round start's
/// computed factors (`0x00B71EC0`) also skip it, but come after the round-end broadcasts with no
/// read between them, so the pool's read still rebuilds. The Peninsular loot resentment
/// (`0x00B541E0` → `0x00B544F0`) skips it too and is modelled so, as is the threatened factor on a
/// threatening deal's answer (`0x00B72F00`, [`CampaignModel::apply_threatened`]). The other write
/// without the broadcast belongs to an event the model lacks: `0x00AAB440` → `0x00B544F0` (a region
/// revolt or handover, INFERRED: the caller is not traced). A faction's re-emergence (`0x00B2BB00` → `0x00B2BC10` /
/// `0x00B72AC0`, [`CampaignModel::reemerge_faction`]) resets its records without it too and is
/// modelled so; only the protectorate a liberation makes broadcasts, through
/// [`CampaignModel::make_protectorate`] (CAMPAIGN_FIDELITY.md "The relation is a stored copy").
///
/// ORIGINAL BUG (`0x00BD21B0`): for those factions it writes 0, then computes the score anyway
/// and overwrites it. Ours keeps 0.
///
/// ORIGINAL BUG (`0x00BD23F8`, `0x00BD24C0`): the two neighbour lists' "already listed" tests are
/// inverted (a region is appended only when it is already there), so the lists stay empty and the
/// proximity share is always 0, which leaves `PROXIMITY_PCT` dead. Ours lists each neighbour once.
pub fn rival_score(model: &CampaignModel, f: FactionId) -> f32 {
    rival_score_in(model, &areas(model), f)
}

/// [`rival_score`] over the director's areas, built once per pass.
fn rival_score_in(model: &CampaignModel, areas: &[Area], f: FactionId) -> f32 {
    let Some(faction) = model.world.factions.get(&f) else { return 0.0 };
    let rules = &model.rules.director;
    if model.is_rebel_faction(f) || rules.no_rival_keys.contains(&faction.key) || model.is_human(f) || !model.in_the_game(f) {
        return 0.0;
    }
    let campaign = model.rules.campaign.as_str();
    let pct = |v: &str| rules.float(campaign, v) as f64;
    let (relation_pct, victory_pct, proximity_pct) = (pct(RIVAL_RELATION_PCT), pct(RIVAL_VICTORY_REGIONS_PCT), pct(RIVAL_PROXIMITY_PCT));
    let weight = if model.is_major(f) { 1.0 } else { pct(RIVAL_MINOR_FACTION_WEIGHT) };
    let mine: Vec<RegionId> = model.world.regions.values().filter(|r| r.owner == f).map(|r| r.id).collect();
    // The areas linked to its regions' areas, each once (seas included: they count in the share's
    // denominator, never in its numerator).
    let neighbours: BTreeSet<usize> = areas.iter().filter(|a| a.region.is_some_and(|r| r.owner == f)).flat_map(|a| a.links.iter().copied()).collect();
    let mut best = 0.0f32;
    for &h in &model.turn.humans {
        let a = 1.0 - relation_belief_value(model, f, h);
        let b = match model.world.victory_regions.get(&h) {
            Some(named) if !mine.is_empty() => mine.iter().filter(|r| named.contains(r)).count() as f32 / mine.len() as f32,
            _ => 0.0,
        };
        let c = if neighbours.is_empty() {
            0.0
        } else {
            neighbours.iter().filter(|&&n| areas.get(n).and_then(|a| a.region).is_some_and(|r| r.owner == h)).count() as f32 / neighbours.len() as f32
        };
        // The x87 sums, each stored as f32.
        let t = (victory_pct * b as f64) as f32;
        let t = (relation_pct * a as f64 + t as f64) as f32;
        let u = (proximity_pct * c as f64 + t as f64) as f32;
        let mut x = (weight * u as f64) as f32;
        if 1.0 <= x {
            x = 1.0;
        }
        best = if x <= best { best } else { x };
    }
    best
}

/// One of the director's map areas: the CAI world's areas ([`super::World::ai_areas`]) in their
/// order, each with its game region (none for a sea, a theatre or `all`) and its links as indexes
/// into the list. Without them (a custom map, or a file without CAI state) the game regions in id
/// order with [`super::World::region_neighbours`].
struct Area<'a> {
    region: Option<&'a Region>,
    links: Vec<usize>,
    /// The CAI area's key (empty without CAI areas), for a sea area's [`human_in`].
    key: &'a str,
    /// Whether a human has a mobile in it, found on first use by [`human_in`]: only the transfer
    /// checks read it, never the rival score.
    human: OnceCell<bool>,
}

/// The director's map areas ([`Area`]). Cold path: built once per desire step, and once per due
/// transfer (a transfer changes owners).
fn areas(model: &CampaignModel) -> Vec<Area<'_>> {
    let w = &model.world;
    if !w.ai_areas.is_empty() {
        let by_key: BTreeMap<&str, &Region> = w.regions.values().map(|r| (r.key.as_str(), r)).collect();
        return w.ai_areas
            .iter()
            .map(|a| Area { region: by_key.get(a.key.as_str()).copied(), links: a.links.clone(), key: a.key.as_str(), human: OnceCell::new() })
            .collect();
    }
    let index: BTreeMap<RegionId, usize> = w.regions.keys().enumerate().map(|(i, &id)| (id, i)).collect();
    w.regions
        .values()
        .map(|r| Area {
            region: Some(r),
            links: w.region_neighbours.get(&r.id).map_or(&[][..], Vec::as_slice).iter().filter_map(|n| index.get(n).copied()).collect(),
            key: "",
            human: OnceCell::new(),
        })
        .collect()
}

/// Whether a human has a mobile in the area, computed once per area on first use: [`human_inside`]
/// for a game region, else [`human_at_sea`] of its key.
fn human_in(model: &CampaignModel, area: &Area) -> bool {
    *area.human.get_or_init(|| match area.region {
        Some(r) => human_inside(model, r),
        None => human_at_sea(model, area.key),
    })
}

/// Whether a human has a mobile in the area (`0x00BB51B0` over the CAI region's mobiles, #9
/// +0x18C, as the loader `0x00BECCE0` reads them): the mobiles are the forces (at their leader) and
/// the agents situated in it (`cai_world.rs`; matches the original's saves). A land area holds the
/// characters [`super::economy::in_region`] places in its region; a sea area, see [`human_at_sea`].
fn human_inside(model: &CampaignModel, reg: &Region) -> bool {
    model.world.characters.values().any(|c| model.is_human(c.faction) && super::economy::in_region(model, reg, c))
}

/// Whether a human's mobile (not in a garrison) stands in the sea area with the key: the path grid's
/// region under it is the key. The original situates a mobile in the `regions.esf` region under it,
/// sea included (its saves' CAI_SITUATED #2, test `mobiles_are_situated_in_the_region_under_them`),
/// and the grid's sea cells carry that region. A theatre or `all` has no cells, and without the map
/// nothing is found.
fn human_at_sea(model: &CampaignModel, key: &str) -> bool {
    let Some(t) = &model.terrain else { return false };
    model.world.characters.values()
        .filter(|c| model.is_human(c.faction) && c.garrisoned_in.is_none())
        .any(|c| t.0.region_at(c.position.0.to_f32(), c.position.1.to_f32()) == Some(key))
}

/// Whether any human sees the region's settlement or one of its other slots (`0x00BB55A0`): each
/// position is tested for every human with a shroud (`0x00BB5640`; a human without one is skipped)
/// with [`CampaignModel::sees_fully`] (`0x00B7A1D0`). The settlement's own slots (keys
/// `settlement:...`) stand at the settlement and are skipped.
fn seen_by_a_human(model: &CampaignModel, reg: &Region) -> bool {
    let to_f32 = |p: (crate::fixed::Fixed20, crate::fixed::Fixed20)| (p.0.to_f32(), p.1.to_f32());
    let mut positions = std::iter::once(to_f32(reg.settlement.position))
        .chain(reg.slots.iter().filter(|s| !s.key.starts_with("settlement:")).filter_map(|s| s.position.map(to_f32)));
    positions.any(|p| seen_at(model, p))
}

/// Whether any human with a shroud sees the position in full (`0x00BB5640` per human, a human
/// without a shroud skipped; [`CampaignModel::sees_fully`], `0x00B7A1D0`).
fn seen_at(model: &CampaignModel, p: (f32, f32)) -> bool {
    model.turn.humans.iter().any(|&h| model.world.shrouds.contains_key(&h) && model.sees_fully(h, p))
}

/// Whether the director may hand the area's region to another faction (`0x00BB52E0`), in the
/// exe's order: the area is a game region with an owner (a sea or theatre never is); its settlement
/// is not under siege (CAI region +0x12C, whose +0x34 interface's virtual +0x94 is the thunk
/// `0x00C402D0` to the game region's settlement +0xFC virtual +0x98, the `IsUnderSiege` slot of
/// `0x008A1FF0`); the owner is neither a major power nor human; no human has a mobile in it; no
/// linked area is a human's or holds a human's mobile; and no human sees it
/// ([`CampaignModel::sees_fully`] of its settlement and its other slots).
///
/// PROVISIONAL: the siege test is never true, as the campaign model has no sieges yet (no
/// besieging army, no siege state on a settlement); it needs them modelled and read here (BACKLOG
/// "Sieges on the campaign map").
fn transferable_at(model: &CampaignModel, areas: &[Area], i: usize) -> bool {
    let area = &areas[i];
    let Some(reg) = area.region else { return false };
    let owner = reg.owner;
    if !model.world.factions.contains_key(&owner) || model.is_major(owner) || model.is_human(owner) || human_in(model, area) {
        return false;
    }
    for link in area.links.iter().filter_map(|&j| areas.get(j)) {
        if link.region.is_some_and(|n| model.is_human(n.owner)) || human_in(model, link) {
            return false;
        }
    }
    !seen_by_a_human(model, reg)
}

/// Whether the director may hand the region to another faction ([`transferable_at`] on its area);
/// false for a region the director's areas do not hold.
pub fn transferable(model: &CampaignModel, region: RegionId) -> bool {
    let areas = areas(model);
    areas.iter().position(|a| a.region.is_some_and(|r| r.id == region)).is_some_and(|i| transferable_at(model, &areas, i))
}

/// The region-transfer candidates (`0x00BA2B90`) and their total weight (an f32 sum): for each
/// [transferable] area, in the areas' order, each linked area's owner that is another faction, a
/// major power, not human and on the region owner's relation belief's enemy list
/// ([`believes_at_war`]; `0x00A649F0` on the owner's belief `0x00A74A20`), in link order, weighted by its
/// [`rival_score`], times `UBER_FACTION_MODIFIER` for an [uber](is_uber) faction. A receiver
/// bordering the region twice is listed twice, as in the original.
pub fn candidates(model: &CampaignModel) -> (Vec<Candidate>, f32) {
    let uber_modifier = model.rules.director.float(&model.rules.campaign, TRANSFER_UBER_FACTION_MODIFIER);
    let areas = areas(model);
    let mut scores: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut out = Vec::new();
    let mut total = 0.0f32;
    for (i, area) in areas.iter().enumerate() {
        let Some(reg) = area.region.filter(|_| transferable_at(model, &areas, i)) else { continue };
        for n in area.links.iter().filter_map(|&j| areas.get(j)?.region) {
            let o = n.owner;
            if o == reg.owner || !model.world.factions.contains_key(&o) || !model.is_major(o) || model.is_human(o) || !believes_at_war(model, reg.owner, o) {
                continue;
            }
            let mut weight = *scores.entry(o).or_insert_with(|| rival_score_in(model, &areas, o));
            if is_uber(model, o) {
                weight *= uber_modifier;
            }
            total += weight;
            out.push(Candidate { region: reg.id, receiver: o, weight });
        }
    }
    (out, total)
}

/// The candidate a roll in [0, total] picks: the first whose running weight reaches the roll, else
/// the last.
///
/// ORIGINAL BUG (`0x00BAE1F5`..`0x00BAE230`): the pick loop subtracts the front candidate's weight
/// and drops it while the roll is above 0, then takes the new front, so each candidate is picked
/// with the weight of the one before it and the first only when the roll is 0. Ours gives each
/// candidate its own weight.
pub fn pick(candidates: &[Candidate], roll: f32) -> Option<&Candidate> {
    pick_index(candidates.iter().map(|c| c.weight), roll).map(|i| &candidates[i])
}

/// The index a roll in [0, total] picks among the weights: the first whose running (f32) sum
/// reaches the roll, else the last; `None` for no weights. Both of the director's weighted picks
/// ([`pick`], [`xp_step`]) use it.
fn pick_index(weights: impl IntoIterator<Item = f32>, roll: f32) -> Option<usize> {
    let mut sum = 0.0f32;
    let mut last = None;
    for (i, w) in weights.into_iter().enumerate() {
        sum += w;
        if sum >= roll {
            return Some(i);
        }
        last = Some(i);
    }
    last
}

/// A chance clamped to [0, 1] as the director's desires clamp it: NaN becomes 1, as the exe's
/// comparisons give.
fn clamp_chance(chance: f32) -> f32 {
    if chance.is_nan() || chance >= 1.0 {
        1.0
    } else if chance <= 0.0 {
        0.0
    } else {
        chance
    }
}

/// The original's region-transfer desire step (`0x00BAE0A0`; see the module docs): the director's
/// state after one step. The rule behind the seam [`super::seams::DIRECTOR_REGION_TRANSFER`].
pub fn region_transfer_step(model: &CampaignModel) -> DirectorState {
    let mut state = model.director.clone();
    let rules = &model.rules.director;
    let campaign = model.rules.campaign.as_str();
    let base = rules.float(campaign, TRANSFER_BASE_CHANCE);
    let extra = rules.float(campaign, TRANSFER_EXTRA_CHANCE_PER_TURN);
    let chance = clamp_chance(base + extra * state.transfer_counter as f32);
    let roll = state.rng.float_range(0.0, 1.0);
    // Neither is NaN here (chance is clamped, the roll comes from a 16-bit draw).
    if chance < roll {
        state.transfer_counter = state.transfer_counter.wrapping_add(1);
        return state;
    }
    let (list, total) = candidates(model);
    if list.is_empty() {
        return state;
    }
    let roll = state.rng.float_range(0.0, total);
    if let Some(c) = pick(&list, roll) {
        // `ExecuteTransferRegionIntention` `0x00BAF410` → `QueueRegionOwnershipChange` `0x00BA33A0`.
        let queued_owner = model.world.regions.get(&c.region).map_or(FactionId(0), |r| r.owner);
        state.pending.push(PendingTransfer { receiver: c.receiver, region: c.region, queued_owner });
    }
    state.transfer_counter = 0;
    state
}

/// The CAI world's factions in its order ([`super::World::ai_factions`]), those the model has; the
/// model's factions in id order without them (a custom map, or a file without CAI state).
fn ai_order(model: &CampaignModel) -> Vec<FactionId> {
    let w = &model.world;
    let listed: Vec<FactionId> = w.ai_factions.iter().copied().filter(|f| w.factions.contains_key(f)).collect();
    if listed.is_empty() { w.factions.keys().copied().collect() } else { listed }
}

/// A faction the director treats as the pirates (the importer's [`DirectorRules::no_rival_keys`],
/// tested by `0x00C3FD50`).
pub(crate) fn is_pirate(model: &CampaignModel, f: FactionId) -> bool {
    model.world.factions.get(&f).is_some_and(|x| model.rules.director.no_rival_keys.contains(&x.key))
}

/// A faction the boost, XP and replenish desires serve (`0x00BB5160`, `0x00BB50D0`): not human,
/// still in the game, neither the rebels nor the pirates.
fn served(model: &CampaignModel, f: FactionId) -> bool {
    !model.is_human(f) && model.in_the_game(f) && !model.is_rebel_faction(f) && !is_pirate(model, f)
}

/// A mobile's position (CAI mobile, the CAI world's mobiles of a faction: its forces, garrisons
/// included): its commander's; PROVISIONAL: a garrison without a commander stands at its
/// settlement, and a force with neither has no position and so is never seen. The CAI faction's
/// mobile list (#3) is walked in force-id order (PROVISIONAL: the exe's list order is not read
/// from the file).
fn mobile_position(model: &CampaignModel, force: &super::MilitaryForce) -> Option<(f32, f32)> {
    let to_f32 = |p: (crate::fixed::Fixed20, crate::fixed::Fixed20)| (p.0.to_f32(), p.1.to_f32());
    if let Some(c) = force.commander.and_then(|c| model.world.characters.get(&c)) {
        return Some(to_f32(c.position));
    }
    model.world.regions.values().find(|r| r.garrison == Some(force.id)).map(|r| to_f32(r.settlement.position))
}

/// Whether any human sees the mobile ([`seen_at`] of [`mobile_position`]).
fn mobile_seen(model: &CampaignModel, force: &super::MilitaryForce) -> bool {
    mobile_position(model, force).is_some_and(|p| seen_at(model, p))
}

/// The faction's key, for its config lookups ("" for a faction the model lacks).
fn key_of(model: &CampaignModel, f: FactionId) -> &str {
    model.world.factions.get(&f).map_or("", |x| x.key.as_str())
}

/// The at-war desire's step (`0x00BAC560`, CONFIRMED): for each faction `f` in the CAI world's
/// order ([`ai_order`]) that is not human, still in the game and neither the rebels nor the
/// pirates, each faction `g` of `f`'s enemy list that is not human and still in the game, unless
/// this step already took the pair in either order: the pair's winner is drawn. The enemy list is
/// `f`'s CAI relation belief's enemy list (count `+0x4C`, data `+0x50`; read through
/// `0x00A74A20`, iterated by `0x00A649A0` / `0x00A649C0`; [`build_relation_belief`]), in the CAI
/// world's order ([`believes_at_war`]: the stored list while the belief is clean, else
/// [`CampaignModel::at_war`]). With `a` = `f`, `b` = `g`: the pirates or rebels
/// (`b`) lose without a draw; else a roll in [0, score(b) + score(a)] on the director's RNG
/// ([`rival_score`], computed once for the pool) above score(a) makes `b` the winner, else `a`.
/// Each (winner, loser) replaces the first [`DirectorState::autoresolver_pairs`] entry naming the
/// same two factions in either order and is appended (`0x00BC5240`).
///
/// The list is the stored relation belief, rebuilt only when read while dirty; every stance change
/// the model makes broadcasts and so marks it dirty (see [`rival_score`]), so the list a clean
/// belief keeps differs from the current stances only after a change the model makes without the
/// broadcast.
///
/// PROVISIONAL: the autoresolver reads the pairs (`CDIR_INTERFACE_AUTORESOLVER_MODIFIERS`, the
/// `CDIR_CVN_AUTORESOLVER_MODIFIER_*` rows) to tilt a battle between the two; that use is not
/// modelled (BACKLOG §0 "Director autoresolver modifiers").
pub fn at_war_step(model: &mut CampaignModel, order: &[FactionId], scores: &BTreeMap<FactionId, f32>) {
    let score = |f: FactionId| scores.get(&f).copied().unwrap_or(0.0);
    let mut taken: Vec<(FactionId, FactionId)> = Vec::new();
    for &f in order {
        if model.is_human(f) || !model.in_the_game(f) || model.is_rebel_faction(f) || is_pirate(model, f) {
            continue;
        }
        for &g in order {
            if g == f || !believes_at_war(model, f, g) || model.is_human(g) || !model.in_the_game(g) {
                continue;
            }
            if taken.iter().any(|&p| p == (f, g) || p == (g, f)) {
                continue;
            }
            taken.push((f, g));
            // `f` (`a`) is never the rebels or the pirates here: the outer loop skipped them.
            let pair = if model.is_rebel_faction(g) || is_pirate(model, g) {
                (f, g)
            } else {
                let roll = model.director.rng.float_range(0.0, score(g) + score(f));
                if score(f) < roll { (g, f) } else { (f, g) }
            };
            let pairs = &mut model.director.autoresolver_pairs;
            if let Some(i) = pairs.iter().position(|&(w, l)| (w, l) == pair || (l, w) == pair) {
                pairs.remove(i);
            }
            pairs.push(pair);
        }
    }
}

/// The boost desire's step (`0x00BAC310`, CONFIRMED): the factions it [serves](served), in the CAI
/// world's order, sorted by rival score, highest first (the exe's `std::sort`,
/// [`crate::msvc_sort`]); the first `min(max(floor(n × PERCENTAGE), MIN), MAX)` of the `n` each
/// get their `AMOUNT` (a faction's int) credited to the treasury. No RNG draw.
///
/// The exe credits through an intention (treasury4, 2026-10-14, CONFIRMED): the step builds one per
/// chosen faction (`0x00B930D0(desire, faction, amount)`, vtable `0x013800AC`, faction at +0x108, amount at +0x10C), whose
/// action `0x00BAF200` → `0x00BB33A0` is the credit `0x00BB3810(amount, 4)` (the director's
/// category). The intention loop `0x00BBC880` runs a desire's new intentions right after its step,
/// so crediting here is the same order of treasury changes.
pub fn boost_step(model: &mut CampaignModel, order: &[FactionId], scores: &BTreeMap<FactionId, f32>) {
    let rules = std::sync::Arc::clone(&model.rules);
    let (d, campaign) = (&rules.director, rules.campaign.as_str());
    let mut list: Vec<FactionId> = order.iter().copied().filter(|&f| served(model, f)).collect();
    let score = |f: &FactionId| scores.get(f).copied().unwrap_or(0.0);
    crate::msvc_sort::sort_by(&mut list, |a, b| score(a) > score(b));
    let share = (list.len() as f32 * d.float(campaign, BOOST_PERCENTAGE)).floor() as u32;
    let count = share.max(d.uint(campaign, BOOST_MIN)).min(d.uint(campaign, BOOST_MAX)) as usize;
    for f in list.into_iter().take(count) {
        let amount = d.faction_int(campaign, key_of(model, f), BOOST_AMOUNT);
        if let Some(x) = model.world.factions.get_mut(&f) {
            super::treasury::credit(&mut x.treasury, amount);
        }
    }
}

/// The XP desire's step for armies (`0x00BAE910`, CONFIRMED): for each faction it
/// [serves](served), in the CAI world's order, the chance `BASE_CHANCE + EXTRA_CHANCE_PER_TURN ×
/// counter` ([`DirectorState::xp_counters`], added at 0 on first use; clamped as the transfer's) against a roll in [0, 1]:
/// above it, the counter goes up by one. Else its armies with units that no human sees are
/// weighed `n² − (rank sum / n)⁴` (`n` units, the integer mean rank; kept above 0, `0x00BA30B0`
/// → `0x00BB5270`); with none, nothing changes; else a roll in [0, total] picks one
/// ([`pick_index`]), each of its units draws its rank's steps (`0x00BA3240` → `0x00BD4A60`: of
/// [`XP_RANK_STEPS`] `(min, max)`, the first `min` count, each other one draws a roll in [0, 1]
/// and counts when it is at most [`XP_RANK_CHANCE`]) and the counter goes back to 0.
///
/// ORIGINAL BUG (the pick loop at `0x00BAEE3F`..`0x00BAEEDA` in `0x00BAE910`): as the transfer's
/// ([`pick`]), it drops the front army while the roll is above 0, so each army is picked with the
/// weight of the one before it. Ours gives each its own weight.
///
/// PROVISIONAL: the campaign model has no unit experience yet, so every rank is 0 (weights `n²`,
/// three draws per unit at 0.7), and the counted steps (the intention `0x00BA3240` queues, raising
/// the units' experience) are not applied (BACKLOG §0 "Director XP upgrade").
pub fn xp_step(model: &mut CampaignModel, order: &[FactionId]) {
    let rules = std::sync::Arc::clone(&model.rules);
    let (d, campaign) = (&rules.director, rules.campaign.as_str());
    let (base, extra) = (d.float(campaign, XP_ARMY_BASE_CHANCE), d.float(campaign, XP_ARMY_EXTRA_CHANCE_PER_TURN));
    for &f in order {
        if !served(model, f) {
            continue;
        }
        // Every served faction gets a counter (find or insert 0, `0x00BAE9AE`..`0x00BAEAA9`).
        let counter = *model.director.xp_counters.entry(f).or_insert(0);
        let chance = clamp_chance(base + extra * counter as f32);
        if chance < model.director.rng.float_range(0.0, 1.0) {
            model.director.xp_counters.insert(f, counter.wrapping_add(1));
            continue;
        }
        let mut armies: Vec<(super::ids::ForceId, f32)> = Vec::new();
        let mut total = 0.0f32;
        for force in model.world.forces.values().filter(|x| x.faction == f && !x.is_navy && !x.units.is_empty()) {
            if mobile_seen(model, force) {
                continue;
            }
            let n = force.units.len() as u32;
            let rank_sum = 0u32; // PROVISIONAL: no unit experience in the model.
            let weight = (n as f32) * (n as f32) - ((rank_sum / n) as f32).powi(4);
            if weight > 0.0 {
                total += weight;
                armies.push((force.id, weight));
            }
        }
        if armies.is_empty() {
            continue;
        }
        let roll = model.director.rng.float_range(0.0, total);
        let Some(i) = pick_index(armies.iter().map(|a| a.1), roll) else { continue };
        let units = model.world.forces.get(&armies[i].0).map_or(0, |x| x.units.len());
        for _ in 0..units {
            let rank = 0usize; // PROVISIONAL: as above.
            let (min, max) = XP_RANK_STEPS[rank];
            let mut _steps = 0u32;
            for step in 0..max {
                if step < min || model.director.rng.float_range(0.0, 1.0) <= XP_RANK_CHANCE[rank] {
                    _steps += 1;
                }
            }
        }
        model.director.xp_counters.insert(f, 0);
    }
}

/// Whether a unit needs replenishing (`0x00BC0800`, CONFIRMED for an army): fewer men than
/// `floor(max men × ARMY_MIN_PCT)` (the owner's float), and its army is not aboard a navy.
/// PROVISIONAL: a navy's test is not traced; the desire gathers armies only, so a navy never
/// needs it here.
fn needs_replenish(model: &CampaignModel, force: &super::MilitaryForce, unit: &super::CampaignUnit) -> bool {
    if force.is_navy {
        return false;
    }
    let min_pct = model.rules.director.faction_float(&model.rules.campaign, key_of(model, force.faction), REPLENISH_ARMY_MIN_PCT);
    i64::from(unit.men) < ((unit.max_men as f32) * min_pct).floor() as i64 && !model.world.embarked.contains_key(&force.id)
}

/// The replenish desire's step for armies (`0x00BADA40`, CONFIRMED): the faction, army and unit
/// allowances start at the `FACTION_` / `MOBILE_` / `UNIT_ARMY_ALLOWED` floats of the faction (of
/// the owner for an army or unit). For each faction it [serves](served) with a faction allowance
/// of at least 1, in the CAI world's order, the units that [need it](needs_replenish) with a unit
/// allowance of at least 1, of its armies with units, unseen by the humans, with an army
/// allowance of at least 1 (`0x00BA2A00`; in army, then unit, order) are listed; then, as many
/// times as the list was long, while the faction allowance is at least 1: a unit is drawn
/// (`int_range(0, len − 1)` on the director's RNG), queued when its army's and its own allowances
/// are still at least 1 (each of the three allowances then goes down by 1), and taken off the list.
/// After every faction, each queued unit still in need (`0x00BC06E0`) draws a fraction in
/// [`ARMY_MIN_PCT`, `ARMY_MAX_PCT`] (the owner's floats) and is brought to `floor(max men ×
/// fraction)` men.
///
/// PROVISIONAL: the allowances are this step's own (the exe keeps them in the desire, their
/// carry-over between passes and their save are not traced); with `MAX_PCT` below `MIN_PCT` or
/// above 1 (no vanilla row) the men are kept within [men, max men], as the intention's setter
/// `0x008AABB0` is not traced; the navy half (`*_NAVY_*` rows) is not traced (BACKLOG §0
/// "Director replenish").
pub fn replenish_step(model: &mut CampaignModel, order: &[FactionId]) {
    use super::ids::{ForceId, UnitId};
    let rules = std::sync::Arc::clone(&model.rules);
    let (d, campaign) = (&rules.director, rules.campaign.as_str());
    let mut faction_allowed: BTreeMap<FactionId, f32> = BTreeMap::new();
    let mut army_allowed: BTreeMap<ForceId, f32> = BTreeMap::new();
    let mut unit_allowed: BTreeMap<UnitId, f32> = BTreeMap::new();
    let mut queued: Vec<(ForceId, UnitId)> = Vec::new();
    for &f in order {
        let key = key_of(model, f);
        let allowed = |var: &str| d.faction_float(campaign, key, var);
        let fa = *faction_allowed.entry(f).or_insert_with(|| allowed(REPLENISH_FACTION_ARMY_ALLOWED));
        if !served(model, f) || fa < 1.0 {
            continue;
        }
        let mut list: Vec<(ForceId, UnitId)> = Vec::new();
        for force in model.world.forces.values().filter(|x| x.faction == f && !x.is_navy && !x.units.is_empty()) {
            let aa = *army_allowed.entry(force.id).or_insert_with(|| allowed(REPLENISH_MOBILE_ARMY_ALLOWED));
            if aa < 1.0 || mobile_seen(model, force) {
                continue;
            }
            for unit in &force.units {
                let ua = *unit_allowed.entry(unit.id).or_insert_with(|| allowed(REPLENISH_UNIT_ARMY_ALLOWED));
                if needs_replenish(model, force, unit) && ua >= 1.0 {
                    list.push((force.id, unit.id));
                }
            }
        }
        for _ in 0..list.len() {
            if faction_allowed[&f] < 1.0 {
                break;
            }
            let i = model.director.rng.int_range(0, list.len() as i32 - 1) as usize;
            let (force, unit) = list[i];
            // The army's and unit's other gates cannot change within the step: only the
            // allowances are tested again.
            if army_allowed[&force] >= 1.0 && unit_allowed[&unit] >= 1.0 {
                queued.push((force, unit));
                for a in [faction_allowed.get_mut(&f), army_allowed.get_mut(&force), unit_allowed.get_mut(&unit)].into_iter().flatten() {
                    *a -= 1.0;
                }
            }
            list.remove(i);
        }
    }
    for (force_id, unit_id) in queued {
        let Some(force) = model.world.forces.get(&force_id) else { continue };
        let Some(unit) = force.units.iter().find(|u| u.id == unit_id) else { continue };
        if !needs_replenish(model, force, unit) {
            continue;
        }
        let key = key_of(model, force.faction);
        let (lo, hi) = (d.faction_float(campaign, key, REPLENISH_ARMY_MIN_PCT), d.faction_float(campaign, key, REPLENISH_ARMY_MAX_PCT));
        let fraction = model.director.rng.float_range(lo, hi);
        if let Some(u) = model.world.forces.get_mut(&force_id).and_then(|x| x.units.iter_mut().find(|u| u.id == unit_id)) {
            let target = ((u.max_men as f32) * fraction).floor();
            u.men = u.men.max((target.max(0.0) as u32).min(u.max_men));
        }
    }
}

/// The director's desires after the region transfer, in the pool's score order (`0x00BBC880`; the
/// may 1811 save's pool holds one army desire of each), the original's rule behind the seam
/// [`super::seams::DIRECTOR_DESIRES`]: the rival scores ([`rival_score`]) once, then
/// [`at_war_step`] (8000), [`boost_step`] (7000), [`xp_step`] (3000) and [`replenish_step`] (2000).
/// The uber (11000) and teleport (10000) desires draw nothing.
///
/// PROVISIONAL (BACKLOG §0 "Director desires"): the teleport desire's effect (generals moved to
/// the front, `CDIR_CVN_TELEPORT_GENERALS_TO_FRONT`), the force-minimum desire (6000) and the
/// unit balancer (5000) and upgrader (4000) desires' gathers are not traced, so they draw nothing
/// here; the trade-theatre filler (1000) is in state 2 in the vanilla pool, which the pool skips,
/// and the desire states are not read from the file.
pub fn director_desires(model: &mut CampaignModel, _writes: &mut ModWrites) {
    let order = ai_order(model);
    let scores: BTreeMap<FactionId, f32> = {
        let areas = areas(model);
        order.iter().map(|&f| (f, rival_score_in(model, &areas, f))).collect()
    };
    at_war_step(model, &order, &scores);
    boost_step(model, &order, &scores);
    xp_step(model, &order);
    replenish_step(model, &order);
}

impl CampaignModel {
    /// The director's part of a turn handover from `ending` to `starting` (see the module docs):
    /// the transfers queued for `starting` are carried out, then, from a human to an AI faction, the
    /// region-transfer desire steps and the other desires run (each through its seam). Then every
    /// relation belief is marked dirty ([`DirectorState::beliefs`]).
    ///
    /// INFERRED, the turn change's mark: the CAI frame update after the handover (`0x00A761F0` →
    /// `0x00CF7F60`) marks the BDI core's `[W+0xF0]` dirty with no source, and the relation beliefs
    /// get it only by propagation (`0x00CF0B10`), where a belief with a source rebuilds only when
    /// that source's own rebuild returns 1 (`0x00CCCF70`); whether it does is not settled statically
    /// (it needs the debugger). It matters only with two humans or a read after the pool: the
    /// pool runs before it (`0x008A9920`), and each round-end drift (`0x00B29100`) marks every
    /// belief dirty anyway.
    pub fn director_handover(&mut self, ending: FactionId, starting: FactionId, events: &mut Vec<CampaignEvent>) {
        self.apply_region_transfers(starting, events);
        if self.is_human(ending) && !self.is_human(starting) {
            let mut writes = ModWrites::default();
            let rules = std::sync::Arc::clone(&self.rules);
            self.director = rules.seams.director_region_transfer.rule()(self, &mut writes);
            rules.seams.director_desires.rule()(self, &mut writes);
            self.mod_state.apply(&mut writes);
        }
        self.director.beliefs.clear();
    }

    /// The model's diplomacy-changed event (model `+0x3A8`, `BroadcastModelEventToListeners`
    /// `0x008DBAC0`): its CAI listener (`OnCaiDiplomacyChangedMarkRelationsDirty` `0x00A8F320` →
    /// `MarkAllCaiRelationBeliefsDirty` `0x00A88CE0`) marks every relation belief dirty. CONFIRMED.
    /// Fired where the exe fires it: war (`0x00B26700`, also joining one), peace (`0x00B262C0`),
    /// an alliance (`0x00B29C60`) and its break (`0x00B13840`), a protectorate (`0x00B105C0`) and
    /// the end of an old protectorate tie (`0x00B64B10`, [`Self::end_protectorate_tie`]), a
    /// trade agreement (`0x00B55090`) and its break (`0x00B29BB0`), military access granted
    /// (`GrantMilitaryAccessToFaction` `0x00B444B0`) or cancelled (`0x00B67B20`, `0x00B0DA90`), a state gift (`0x00B44590`), a
    /// regular payment (`0x00B54FC0`), a government change (`0x00B1B100`), a capture
    /// (`0x00B62120`, [`Self::take_settlement`]), an agent's detected misdeed (`0x00B0D870`,
    /// `0x00B67FD0`, `0x00B71CF0`) or blamed one (`0x00B0D700`, `0x00B67E60`; both unless the
    /// victim is the rebels) and each in-game faction's round-end drift (`0x00B29100`). Not fired
    /// by an embargo without a trade agreement (`0x00B28DB0`), a one-off payment or the round
    /// start's computed factors (`0x00B71EC0`). The exe's three other broadcasters: the religion
    /// factors set while the start position's diplomacy records are built (`0x00B45EA0` →
    /// `0x00B1B4E0`; [`Self::setup_relationship_factors`]), before any belief exists, so nothing to clear,
    /// and a trade break both ways (`0x00B73430`) towards the killer of a destroyed faction or on a lost
    /// trade route: the model's destruction ([`Self::destroy_faction`]) has no killer, so it posts
    /// none, and the route loss is not in the model (BACKLOG §0-B).
    pub(crate) fn diplomacy_changed(&mut self) {
        self.director.beliefs.clear();
    }

    /// A read of `f`'s relation belief that keeps it (`GetCaiRelationBelief` `0x00A74A20`): a
    /// dirty belief is rebuilt ([`build_relation_belief`]) and stored clean (`0x00CCCF70` →
    /// `0x00ABAFB0`); a clean one stays as it is. CONFIRMED.
    pub(crate) fn refresh_relation_belief(&mut self, f: FactionId) {
        if !self.director.beliefs.contains_key(&f) {
            let belief = build_relation_belief(self, f);
            self.director.beliefs.insert(f, belief);
        }
    }

    /// `0x00BA90E0`: every queued transfer whose receiver is `starting` leaves the queue, and is
    /// carried out when the region's owner is still the queued one and the region is still
    /// [transferable].
    fn apply_region_transfers(&mut self, starting: FactionId, events: &mut Vec<CampaignEvent>) {
        if !self.director.pending.iter().any(|p| p.receiver == starting) {
            return;
        }
        let (due, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.director.pending).into_iter().partition(|p| p.receiver == starting);
        self.director.pending = kept;
        for p in due {
            if self.world.regions.get(&p.region).is_some_and(|r| r.owner == p.queued_owner) && transferable(self, p.region) {
                self.director_transfer(p.region, p.receiver, events);
            }
        }
    }

    /// The director's transfer of a region (`0x00B58A30`): the garrison army is destroyed, the fleet
    /// link cleared and the region changes hands as a capture does ([`Self::take_settlement`]: the
    /// queues emptied, the land units and constructions without refund, the ships with), but no
    /// capture is reported (no `SettlementOccupied`). Then the receiver's new garrison's size is
    /// drawn and that army raised (`0x00BA8400`, CONFIRMED): `int_range(a + 3, 4a + 6)` on the
    /// director's RNG, `a` 1 for an AI receiver, 0 for a human (4 to 10 units for an AI).
    ///
    /// The draw and the spawn need the receiver to own the region and the region to have no
    /// garrison (CONFIRMED): the exe tests the CAI region's settlement (`+0x12C`, `0x008CF5F0`,
    /// constructor `0x00BF6F00`), whose `+0x34` member is its `CAI_GARRISONABLE` (`0x00BF5160`) and
    /// `+0xFC` that garrison's CAI mobile (`0x00C3B7A0`). The link is cleared within the destroy
    /// above: the commander's destruction (`0x00A0C6F0` → `DestroyCampaignCharacter` `0x0099D2D0`)
    /// fires the model's character-removed broadcaster (`+0x948`) directly, and its CAI listener
    /// (`0x00A8DE80` → `0x00C4E930` → `0x00C4F6D0` → `0x00C54470(0)`) clears `+0xFC`. So the skip
    /// holds only for a garrison the capture itself left, which the model never does.
    ///
    /// The count is drawn first; then the units are gathered ([`transfer_garrison_units`], no draw)
    /// and, when there is one, that many units are raised, unit `i` being the `i`-th of the sorted
    /// list taken round and round, into one army at the settlement garrisoned there (CONFIRMED,
    /// `0x00BA8400`). With no unit to raise the draw is still made and nothing is raised. The
    /// original puts every unit in its one army whatever the army cap; ours places them as recruits
    /// are placed, so a mod's cap below the count (vanilla's 20 is above the 10 drawn at most) starts
    /// a second army outside the settlement.
    ///
    /// The handover (`0x00B449F0` →
    /// `TransferRegionToFaction` `0x00A64AC0` with its fourth argument 0) expels every other
    /// faction's land armies in the region, and its navies docked in the port when not at war with
    /// the receiver (`ExpelFactionForcesFromRegion` `0x008D9DC0`, [`Self::expel_after_owner_change`]).
    fn director_transfer(&mut self, region: RegionId, receiver: FactionId, events: &mut Vec<CampaignEvent>) {
        if let Some(garrison) = self.world.regions.get(&region).and_then(|r| r.garrison) {
            self.destroy_force(garrison, events);
        }
        self.take_settlement(region, receiver, None, events);
        if !self.world.regions.get(&region).is_some_and(|r| r.owner == receiver && r.garrison.is_none()) {
            return;
        }
        let ai = i32::from(!self.is_human(receiver));
        let count = self.director.rng.int_range(ai + 3, ai * 4 + 6);
        let units = transfer_garrison_units(self, region, receiver);
        // Unit i is the i-th candidate round the sorted list (the exe's cursor wraps at the end).
        for unit_key in units.iter().cycle().take(count.max(0) as usize) {
            // The first unit finds no garrison (the transfer destroyed it) and raises a new army led by a
            // new colonel, garrisoned in the settlement; the others join it. The original builds its
            // units and army the way a finished land recruit is placed (unit `0x0087FEA0(faction, record,
            // 0, 0, 1)` and army `0x008D25F0(.., 1, 0, 1, 1)` at the settlement, the same calls as the
            // recruit placement `0x008ECAD0`), so ours reuses that placement. Not a finished recruitment
            // item: no `UnitTrained`.
            let _ = self.spawn_recruited_unit(region, unit_key.clone());
        }
    }
}

/// The slot type that the CAI building slot's constructor (`0x00BE6EF0`, from the model slot's type
/// key, model slot +0x1DC → +0x44) maps to type 3: a port slot, whose recruitment belief lists ships.
/// The exe hardcodes this key in that constructor (`0x00BE6EF0`); no merged-db table or data value
/// marks a slot type as a port, so ours names it here.
const PORT_SLOT_TYPE: &str = "port";

/// The units the director's transfer raises its garrison from (`0x00BA8400`, CONFIRMED): the
/// transferred region's recruitable list ([`CampaignModel::recruitable_units`], rebuilt for the new
/// owner by the capture `0x00B58A30` → `0x00A6AE40`), in its unit-key order, keeping the land units
/// (record +0x9C set) that `0x00BAF9A0` accepts: a unit in the land recruitment belief
/// ([`land_recruitment_belief`]) of the region or of a region across one of its CAI boundaries
/// (`GetCaiRegionBoundariesBegin`/`End`, `GetBoundaryOtherRegion`; the director's area links,
/// [`areas`]) that the receiver owns. They are then sorted by [`garrison_priority`], highest first,
/// with the library sort (`0x00B7F510`, [`crate::msvc_sort::sort_by`]).
fn transfer_garrison_units(model: &CampaignModel, region: RegionId, receiver: FactionId) -> Vec<String> {
    let areas = areas(model);
    let mut regions: Vec<&Region> = model.world.regions.get(&region).into_iter().collect();
    for a in areas.iter().filter(|a| a.region.is_some_and(|r| r.id == region)) {
        regions.extend(a.links.iter().filter_map(|&l| areas.get(l)?.region));
    }
    let mut belief: BTreeSet<&str> = BTreeSet::new();
    for r in regions.into_iter().filter(|r| r.owner == receiver) {
        belief.extend(land_recruitment_belief(model, r));
    }
    let mut units: Vec<(&str, u8)> = model
        .recruitable_units(region)
        .iter()
        .filter_map(|e| {
            let (key, unit) = model.rules.units.get_key_value(&e.unit_key)?;
            (!unit.is_naval && belief.contains(key.as_str())).then(|| (key.as_str(), garrison_priority(unit)))
        })
        .collect();
    crate::msvc_sort::sort_by(&mut units, |a, b| a.1 > b.1);
    units.into_iter().map(|(key, _)| key.to_owned()).collect()
}

/// The comparator's rank of a unit (`0x00B7F510` inline, by the unit record's category code +0x1C,
/// CONFIRMED): infantry 3; cavalry, dragoons, elephants and camels 2; artillery (and every key the
/// category chain does not know) 1; the naval categories 0.
fn garrison_priority(unit: &super::rules::UnitRules) -> u8 {
    use crate::unit_kind::Category;
    match crate::unit_kind::category(&unit.category) {
        Category::Infantry => 3,
        Category::Cavalry | Category::Dragoons | Category::Elephants | Category::Camels => 2,
        Category::Artillery => 1,
        _ => 0,
    }
}

/// The land units in a region's recruitment belief (region belief `0x00D5F720`, rebuilt by
/// `0x00D9C9D0`; membership `0x00D5BE50` → `0x00D5BDB0`, the belief's land set), CONFIRMED:
/// * the settlement's belief (`0x00D5F930`): the region's land entries priced and flagged as the land
///   queue does ([`CampaignModel::recruitable_entry_flags`]) with no flag but [`ENTRY_TOO_DEAR`](super::commands::ENTRY_TOO_DEAR)
///   (a region without a settlement has none; every region of the model has one);
/// * plus each slot's belief (`0x00D5F7D0`, rebuilt by `0x00D9CD60`) for a slot with a recruitment
///   manager (`0x00C3F480`: model slot +0x1E4, [`super::world::RegionSlot::port`]): a port slot
///   ([`PORT_SLOT_TYPE`]) lists ships only, so it adds no land unit; any other adds the land units of
///   its building's `0x00B43CA0` entries ([`CampaignModel::building_entries`]) whose flags hold nothing
///   but the price flag: the building at full health (+0x18 ≥ 100), the slot not held by another
///   faction, and the unit's technology researched (a unit without it is skipped).
///
/// INFERRED: the original reads the belief from its cache, rebuilt only when marked dirty
/// (`IsBeliefDirty` `0x00CF1170`; `MarkBeliefDirty` `0x00CF0AB0` marks a belief and its dependents,
/// from 147 call sites); ours builds it from the current state, which matches only while every change
/// the belief reads marks it dirty before the transfer. Which changes do is not traced.
fn land_recruitment_belief<'a>(model: &'a CampaignModel, r: &'a Region) -> BTreeSet<&'a str> {
    use super::commands::ENTRY_TOO_DEAR;
    let set = super::economy::region_effect_set(model, r);
    let counts = model.unit_type_counts(r.owner);
    let mut out = BTreeSet::new();
    for e in model.recruitable_units(r.id) {
        let Some((key, unit)) = model.rules.units.get_key_value(&e.unit_key) else { continue };
        if unit.is_naval {
            continue;
        }
        let cost = super::economy::recruitment_cost_in(&model.rules, &set, key, unit);
        if model.recruitable_entry_flags(r, &e, unit, cost, &counts) & !ENTRY_TOO_DEAR == 0 {
            out.insert(key.as_str());
        }
    }
    for s in r.slots.iter().filter(|s| s.port && s.slot_type != PORT_SLOT_TYPE) {
        let Some(b) = s.building.as_ref() else { continue };
        for (u, flags) in model.building_entries(r, b, r.slot_occupied(s)) {
            if flags == 0 && model.rules.units.get(u).is_some_and(|x| !x.is_naval) {
                out.insert(u.as_str());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::details::{AttitudeFactor, Relationship};
    use super::super::ids::{CharacterId, ForceId};
    use super::super::tests::test_model;
    use super::super::{CampaignCommand, MilitaryForce};
    use super::*;
    use crate::fixed::Fixed20;

    const A: FactionId = FactionId(1);
    const B: FactionId = FactionId(2);
    const C: FactionId = FactionId(3);

    fn row(campaign: &str, faction: &str, variable: &str, value: &str) -> CdirConfig {
        CdirConfig { campaign: campaign.into(), faction: faction.into(), variable: variable.into(), value: value.into() }
    }

    /// The vanilla rows the director reads (`cdir_configs`, `all` campaigns), with this base chance.
    fn vanilla_rows(base_chance: &str) -> Vec<CdirConfig> {
        vec![
            row(ALL, ALL, RIVAL_RELATION_PCT, "0.2"),
            row(ALL, ALL, RIVAL_VICTORY_REGIONS_PCT, "0.4"),
            row(ALL, ALL, RIVAL_PROXIMITY_PCT, "0.4"),
            row(ALL, ALL, RIVAL_MINOR_FACTION_WEIGHT, "0.33"),
            row(ALL, ALL, TRANSFER_BASE_CHANCE, base_chance),
            row(ALL, ALL, TRANSFER_EXTRA_CHANCE_PER_TURN, "0.01"),
            row(ALL, ALL, TRANSFER_UBER_FACTION_MODIFIER, "10.0"),
            row(ALL, ALL, UBER_FACTION, "0"),
        ]
    }

    fn with_rows(m: &mut CampaignModel, edit: impl FnOnce(&mut Vec<CdirConfig>)) {
        let mut rules = (*m.rules).clone();
        edit(&mut rules.director.configs);
        m.rules = Arc::new(rules);
    }

    /// A MADE-UP map on the test model: A (human) holds region 10, B (a major power) 11, C (a minor
    /// faction at war with B) 12; 10 and 12 each border 11 only. No human character stands near 11
    /// or 12, and A's victory conditions name 11 and 12.
    fn world(base_chance: &str) -> CampaignModel {
        let mut m = test_model();
        with_rows(&mut m, |r| *r = vanilla_rows(base_chance));
        m.turn.humans = vec![A];
        m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
        m.world.faction_details.entry(B).or_default().major = Some(true);
        m.world.faction_details.entry(C).or_default().major = Some(false);
        m.world.region_neighbours = [(RegionId(10), vec![RegionId(11)]), (RegionId(11), vec![RegionId(10), RegionId(12)]), (RegionId(12), vec![RegionId(11)])].into();
        m.world.set_stance(B, C, Stance::War).unwrap();
        for c in m.world.characters.values_mut() {
            c.position = (Fixed20::from_int(-500), Fixed20::from_int(-500));
        }
        m.world.victory_regions.insert(A, [RegionId(11), RegionId(12)].into());
        m
    }

    #[test]
    fn the_config_lookup_takes_the_first_row_of_each_pass() {
        let r = DirectorRules {
            configs: vec![row(ALL, ALL, "V", "1"), row("camp", ALL, "V", "2.0"), row("camp", "f", "V", "3"), row("camp", "f", "V", "4"), row(ALL, ALL, "W", "x")],
            ..Default::default()
        };
        assert_eq!(r.value("camp", "f", "V"), Some("3"));
        assert_eq!(r.value("camp", "g", "V"), Some("2.0"));
        assert_eq!(r.value("other", "f", "V"), Some("1"));
        assert_eq!(r.value(ALL, "f", "V"), Some("1"));
        assert_eq!(r.value("camp", "f", "missing"), None);
        // A float is always read with faction `all`; an unreadable value (no `.`: "1") and a
        // missing row read 0.
        assert_eq!((r.float("camp", "V"), r.float("camp", "W"), r.float("camp", "missing"), r.float("other", "V")), (2.0, 0.0, 0.0, 0.0));
        // A flag is true only for the text `1`.
        assert!(r.flag("other", "f", "V") && !r.flag("camp", "g", "V") && !r.flag("camp", "f", "W"));
    }

    #[test]
    fn the_relation_value_weighs_attitude_and_treaties() {
        let mut m = world("0.01");
        // No relationship: attitude 0 maps to the midpoint (ORIGINAL BUG fixed: the exe gives 0), at peace.
        assert!((relation_value(&m, A, B) - (0.7 * 0.5 + 0.2) / 1.2).abs() < 1e-6);
        // At war: the peace weight goes.
        assert!((relation_value(&m, B, C) - 0.35 / 1.2).abs() < 1e-6);
        // Attitude -100 maps to 0.25, 50 to 0.75, beyond -200 / 100 to 0 / 1; trade and access add.
        let rel = |att: i32| Relationship { attitudes: vec![AttitudeFactor { value: att, ..Default::default() }], trade_agreement: true, military_access_turns: 3, ..Default::default() };
        for (att, att01) in [(-100, 0.25f32), (50, 0.75), (-500, 0.0), (300, 1.0)] {
            m.world.relationships.insert((A, B), rel(att));
            let want = (0.7 * att01 + 0.05 + 0.05 + 0.2) / 1.2;
            assert!((relation_value(&m, A, B) - want).abs() < 1e-6, "attitude {att}");
        }
        // The rebels (an empty key) relate at 0.
        m.world.factions.get_mut(&B).unwrap().key.clear();
        assert_eq!(relation_value(&m, A, B), 0.0);
    }

    #[test]
    fn the_rival_score_takes_the_best_human_share() {
        let mut m = world("0.01");
        // B holds 11, which A's conditions name (b = 1); of its neighbours 10 and 12 A holds one
        // (c = 1/2; ORIGINAL BUG fixed: the exe's is always 0).
        let a = 1.0 - relation_value(&m, B, A);
        assert!((rival_score(&m, B) - (0.2 * a + 0.4 + 0.4 * 0.5)).abs() < 1e-6);
        // C, a minor faction: 12 is named (b = 1), its neighbour 11 is B's (c = 0), times 0.33.
        let a = 1.0 - relation_value(&m, C, A);
        assert!((rival_score(&m, C) - 0.33 * (0.2 * a + 0.4)).abs() < 1e-6);
        // Capped at 1.
        let mut capped = m.clone();
        with_rows(&mut capped, |r| r.insert(0, row(ALL, ALL, RIVAL_VICTORY_REGIONS_PCT, "5.0")));
        assert_eq!(rival_score(&capped, B), 1.0);
        // 0 for a human, a listed key, a faction out of the game, and with no humans (ORIGINAL BUG
        // fixed: the exe overwrites its 0 with the computed score).
        assert_eq!(rival_score(&m, A), 0.0);
        let mut pirate = m.clone();
        Arc::make_mut(&mut pirate.rules).director.no_rival_keys = vec!["pirates".into()];
        assert!(rival_score(&pirate, B) > 0.0, "only a listed key");
        pirate.world.factions.get_mut(&B).unwrap().key = "pirates".into();
        assert_eq!(rival_score(&pirate, B), 0.0);
        let mut gone = m.clone();
        gone.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
        gone.world.forces.retain(|_, f| f.faction != B);
        assert_eq!(rival_score(&gone, B), 0.0);
        m.turn.humans.clear();
        assert_eq!(rival_score(&m, B), 0.0);
    }

    /// The human proposes a deal to B: B's evaluation keeps its relation belief (`0x00A74A20`).
    fn proposed_to_b(m: &mut CampaignModel) {
        m.turn = super::super::turn::TurnState::in_turn_of(A, vec![A]);
        m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
        m.apply(CampaignCommand::ProposeDeal { threat: false }).unwrap();
        m.apply(CampaignCommand::EndNegotiation).unwrap();
        assert_eq!(m.director.beliefs.get(&B), Some(&build_relation_belief(m, B)));
    }

    /// A declaration of war marks every relation belief dirty (`0x00A8F320` → `0x00A88CE0`), so the
    /// pool's read at the handover rebuilds it: a war the human declares mid-turn counts, even
    /// after a deal proposal kept B's belief.
    #[test]
    fn a_war_declared_mid_turn_counts_for_the_rival_score() {
        let mut m = world("0.01");
        let at_peace = rival_score(&m, B);
        proposed_to_b(&mut m);
        m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
        assert!(m.director.beliefs.is_empty(), "the war marks every belief dirty");
        let a = 1.0 - relation_value(&m, B, A);
        assert_eq!(m.world.stance(B, A), Stance::War);
        assert!((rival_score(&m, B) - (0.2 * a + 0.4 + 0.4 * 0.5)).abs() < 1e-6);
        assert!(rival_score(&m, B) > at_peace);
        assert!(believes_at_war(&m, B, A));
    }

    /// C embargoing B at A's request without a trade agreement (`0x00B28DB0`) lowers B's attitude to A with no
    /// diplomacy-changed broadcast, so the belief B's deal evaluation kept stays clean and the
    /// handover's pool reads the relation from before the embargo; the turn change then marks it
    /// dirty. With a C–B trade agreement the embargo breaks it first (`0x00B29BB0`), which broadcasts.
    #[test]
    fn an_embargo_after_a_proposal_is_not_seen_by_the_rival_score() {
        let mut m = world("0.01");
        let before = rival_score(&m, B);
        proposed_to_b(&mut m);
        drop(m.embargo(C, B, A).unwrap());
        let live = 0.2 * (1.0 - relation_value(&m, B, A)) + 0.4 + 0.4 * 0.5;
        assert!(live > before + 1e-3, "the embargo lowers B's relation to A");
        assert!(m.director.beliefs.contains_key(&B), "no broadcast: the belief stays clean");
        assert_eq!(rival_score(&m, B), before, "the pool's read is stale");
        m.director_handover(A, B, &mut Vec::new());
        assert!(m.director.beliefs.is_empty(), "the turn change marks every belief dirty");
        assert!((rival_score(&m, B) - live).abs() < 1e-6);

        // Control: with a trade agreement the embargo breaks it, which broadcasts.
        let mut m = world("0.01");
        m.trade_agreement(C, B).unwrap();
        proposed_to_b(&mut m);
        drop(m.embargo(C, B, A).unwrap());
        assert!(m.director.beliefs.is_empty(), "the trade break marks every belief dirty");
        let live = 0.2 * (1.0 - relation_value(&m, B, A)) + 0.4 + 0.4 * 0.5;
        assert!((rival_score(&m, B) - live).abs() < 1e-6);
    }

    /// A named change to a model, for the tables of [`which_changes_mark_the_relation_beliefs_dirty`].
    type NamedChange = (&'static str, fn(&mut CampaignModel));

    /// The model's diplomacy-changed broadcasters mark every belief dirty; an embargo without a
    /// trade agreement, a one-off payment and the round start's computed factors do not; a capture
    /// marks it unless a side is the rebels, a deal's handover never; the Peninsular loot resentment does not,
    /// ending a protectorate tie does.
    #[test]
    fn which_changes_mark_the_relation_beliefs_dirty() {
        let kept = |m: &CampaignModel| m.director.beliefs.contains_key(&B);
        let mut base = world("0.01");
        proposed_to_b(&mut base);
        let marks: [NamedChange; 14] = [
            ("peace", |m| drop(m.make_peace_rules(B, C).unwrap())),
            ("alliance", |m| drop(m.form_alliance(A, B).unwrap())),
            ("trade", |m| drop(m.trade_agreement(A, B).unwrap())),
            ("access", |m| drop(m.grant_military_access(A, B, 5).unwrap())),
            ("cancel access", |m| {
                let d = super::super::treaties::DiplomaticAction::CancelMilitaryAccess;
                drop(m.apply_diplomatic_action(A, B, d).unwrap());
            }),
            ("gift", |m| drop(m.state_gift(A, B, 100).unwrap())),
            ("regular payment", |m| drop(m.regular_payment(A, B, 100, 5).unwrap())),
            ("protectorate", |m| drop(m.make_protectorate(C, A).unwrap())),
            ("join war", |m| drop(m.join_war(A, C, B).unwrap())),
            ("round end", |m| m.diplomacy_round_end(B)),
            ("capture", |m| assert!(m.take_settlement(RegionId(12), A, None, &mut Vec::new()))),
            ("break alliance", |m| {
                m.form_alliance(A, B).unwrap();
                m.refresh_relation_belief(B);
                drop(m.break_alliance(A, B).unwrap());
            }),
            ("detected misdeed", |m| m.detected_misdeed(A, C, super::super::agents::Misdeed::Assassination)),
            ("blamed misdeed", |m| m.blamed_misdeed(A, C, super::super::agents::Misdeed::Sabotage)),
        ];
        for (what, change) in marks {
            let mut m = base.clone();
            change(&mut m);
            assert!(!kept(&m), "{what} marks the beliefs dirty");
        }
        let keeps: [NamedChange; 4] = [
            ("embargo", |m| drop(m.embargo(C, B, A).unwrap())),
            ("one-off payment", |m| drop(m.one_off_payment(A, B, 100).unwrap())),
            ("computed factors", CampaignModel::refresh_computed_factors),
            ("deal handover", |m| assert!(m.transfer_region(RegionId(12), A))),
        ];
        for (what, change) in keeps {
            let mut m = base.clone();
            change(&mut m);
            assert!(kept(&m), "{what} leaves the beliefs clean");
        }
        // A capture from or by the rebels (here C, made the rebels) fires nothing.
        let mut rebel_c = base.clone();
        rebel_c.world.factions.get_mut(&C).unwrap().key.clear();
        let mut m = rebel_c.clone();
        assert!(m.take_settlement(RegionId(12), A, None, &mut Vec::new()));
        assert!(kept(&m), "a capture from the rebels leaves the beliefs clean");
        let mut m = rebel_c;
        assert!(m.take_settlement(RegionId(11), C, None, &mut Vec::new()));
        assert!(kept(&m), "a capture by the rebels leaves the beliefs clean");
        // The Peninsular loot resentment (`0x00B541E0` → `0x00B544F0`) lowers B's attitude to the looter A with no
        // broadcast: the belief rebuilt after the capture's own broadcast stays clean.
        let mut m = base.clone();
        let mut rules = (*m.rules).clone();
        rules.features.loot_resentment = Some(super::super::features::LootResentment {
            homelands: vec![("homeland_rebels".into(), "test_faction_b".into())],
            exempt_looter: None,
            factor: "abandoned_ally_in_war".into(),
            value: -50,
        });
        m.rules = Arc::new(rules);
        m.world.region_rebel_factions.insert(RegionId(12), "homeland_rebels".into());
        drop(m.capture_by_force(RegionId(12), A, None));
        m.refresh_relation_belief(B);
        let before = relation_value(&m, B, A);
        m.apply(CampaignCommand::ChooseCapture { choice: super::super::capture::CaptureChoice::Loot }).unwrap();
        assert!(relation_value(&m, B, A) < before, "the looting lowers B's attitude to A");
        assert!(kept(&m), "loot resentment leaves the beliefs clean");
        // Ending a protectorate tie (`0x00B64B10`) broadcasts.
        let mut m = base.clone();
        m.make_protectorate(C, A).unwrap();
        m.refresh_relation_belief(B);
        m.end_protectorate_tie(C, A).unwrap();
        assert!(!kept(&m), "ending a protectorate tie marks the beliefs dirty");
    }

    #[test]
    fn a_region_is_transferable_only_far_from_the_humans() {
        use super::super::visibility::{CellSet, Shroud, SightGrid};
        let m = world("0.01");
        assert!(transferable(&m, RegionId(12)));
        // Not a major power's (B) or a human's (A) region.
        assert!(!transferable(&m, RegionId(11)) && !transferable(&m, RegionId(10)));
        // A human character in the region, or in a neighbouring one.
        for inside in [12, 11] {
            let mut x = m.clone();
            x.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(inside));
            assert!(!transferable(&x, RegionId(12)), "a human character in region {inside}");
        }
        // A neighbour owned by a human.
        let mut x = m.clone();
        x.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
        assert!(!transferable(&x, RegionId(12)));
        // A human sees the settlement in full: its cell and the four next to it.
        let mut x = m.clone();
        x.world.sight_grid = Some(SightGrid::centred(64, 64, 64));
        let (cx, cz) = x.world.sight_grid.unwrap().cell_of((12.0, 0.0)).unwrap();
        let mut cross = CellSet::new(64, 64);
        for (x, z) in [(cx, cz), (cx - 1, cz), (cx + 1, cz), (cx, cz - 1), (cx, cz + 1)] {
            cross.set(x, z);
        }
        let empty = CellSet::new(64, 64);
        let shroud = |visible: &CellSet| Shroud { explored: visible.clone(), visible: visible.clone(), hidden: empty.clone(), active: true };
        x.world.shrouds.insert(A, shroud(&cross));
        assert!(x.sees_fully(A, (12.0, 0.0)) && !transferable(&x, RegionId(12)));
        // Only its own cell: not seen in full.
        let mut one = CellSet::new(64, 64);
        one.set(cx, cz);
        x.world.shrouds.insert(A, shroud(&one));
        assert!(!x.sees_fully(A, (12.0, 0.0)) && transferable(&x, RegionId(12)));
        // A human without a shroud is skipped.
        x.world.shrouds.clear();
        assert!(transferable(&x, RegionId(12)));
    }

    #[test]
    fn the_candidates_are_the_major_enemies_bordering_a_transferable_region() {
        let m = world("0.01");
        let (list, total) = candidates(&m);
        let w = rival_score(&m, B);
        assert!(w > 0.0);
        assert_eq!(list, vec![Candidate { region: RegionId(12), receiver: B, weight: w }]);
        assert_eq!(total, w);
        // An uber faction's weight is multiplied by the modifier.
        let mut uber = m.clone();
        // A faction's row names the campaign: the lookup never tries (`all`, faction).
        let campaign = uber.rules.campaign.clone();
        with_rows(&mut uber, |r| r.insert(0, row(&campaign, "test_faction_b", UBER_FACTION, "1")));
        assert!(is_uber(&uber, B) && !is_uber(&uber, C));
        assert_eq!(candidates(&uber).1, w * 10.0);
        // At peace, none.
        let mut peace = m.clone();
        peace.world.set_stance(B, C, Stance::Neutral).unwrap();
        assert!(candidates(&peace).0.is_empty());
    }

    /// With the CAI world's areas, their order and links are walked, not the region ids and the map's
    /// neighbours: a MADE-UP region 13 of C comes first, a sea links the minor regions to A's 10,
    /// and the theatre area links nothing.
    #[test]
    fn the_ai_areas_give_the_order_and_the_links() {
        use super::super::AiArea;
        let mut m = world("0.01");
        let mut r13 = m.world.regions[&RegionId(12)].clone();
        r13.id = RegionId(13);
        r13.key = "test_region_13".into();
        m.world.regions.insert(r13.id, r13);
        m.world.region_neighbours.clear();
        let area = |key: &str, links: &[usize]| AiArea { key: key.into(), links: links.to_vec() };
        m.world.ai_areas = vec![
            area("test_region_13", &[1, 3]),
            area("sea", &[0, 2, 4]),
            area("test_region_12", &[1, 3]),
            area("test_region_11", &[2, 4, 1, 0]),
            area("test_region_10", &[1, 3]),
            area("europe", &[]),
        ];
        // The minor regions border B's 11 and the sea (no owner, no human mobile found there).
        let w = rival_score(&m, B);
        let (list, total) = candidates(&m);
        let c = |region: u32| Candidate { region: RegionId(region), receiver: B, weight: w };
        assert_eq!(list, vec![c(13), c(12)], "the areas' order, not the region ids'");
        assert_eq!(total, w + w);
        // B's 11 links 12, 13, A's 10 and the sea: A holds 1 of 4 (the sea counts in the share).
        let a = 1.0 - relation_value(&m, B, A);
        assert!((w - (0.2 * a + 0.4 + 0.4 * 0.25)).abs() < 1e-6);
        // A human's fleet at sea in the sea area (the path grid's sea cell carries the area's key)
        // blocks both minor regions it links; the same character in a garrison does not.
        let mut at_sea = m.clone();
        let mut grid = super::super::pathing::PathGrid::new((0.0, 0.0), 1.0, 1, 1);
        grid.kind[0] = super::super::pathing::CellKind::Sea as u8;
        grid.region[0] = 0;
        grid.region_keys = vec!["sea".into()];
        at_sea.attach_terrain(super::super::world::Terrain(Arc::new(grid)));
        let admiral = at_sea.world.characters.get_mut(&CharacterId(100)).unwrap();
        (admiral.position, admiral.garrisoned_in) = ((Fixed20::from_int(0), Fixed20::from_int(0)), None);
        assert!(candidates(&at_sea).0.is_empty());
        at_sea.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(10));
        assert_eq!(candidates(&at_sea).0.len(), 2);
        // A region the areas do not hold is never transferable.
        m.world.ai_areas.remove(0);
        for a in &mut m.world.ai_areas {
            a.links = a.links.iter().filter(|&&l| l != 0).map(|l| l - 1).collect();
        }
        assert!(!transferable(&m, RegionId(13)) && transferable(&m, RegionId(12)));
    }

    #[test]
    fn the_pick_gives_each_candidate_its_own_weight() {
        let c = |w: f32| Candidate { region: RegionId(0), receiver: FactionId(w as i32), weight: w };
        let list = [c(1.0), c(2.0), c(3.0)];
        let picked = |roll: f32| pick(&list, roll).unwrap().receiver.raw();
        // ORIGINAL BUG fixed: the exe takes the candidate after the one whose weight covers the roll.
        assert_eq!([0.0, 0.5, 1.0, 1.5, 3.0, 3.5, 6.0, 7.0].map(picked), [1, 1, 1, 2, 2, 3, 3, 3]);
        assert!(pick(&[], 0.0).is_none());
    }

    #[test]
    fn the_desire_step_counts_failed_rolls_and_queues_a_transfer() {
        // Chance 0: the roll (above 0) fails, the counter goes up; one draw.
        let mut m = world("0.0");
        with_rows(&mut m, |r| r.retain(|r| r.variable != TRANSFER_EXTRA_CHANCE_PER_TURN));
        let s = region_transfer_step(&m);
        let mut one = CaRng::new(RNG_SEED);
        one.float_range(0.0, 1.0);
        assert_eq!((s.transfer_counter, s.rng, s.pending.len()), (1, one, 0));
        // Chance 1: a transfer to B is queued with the current owner and the counter goes back to
        // 0; two draws.
        let mut m = world("1.0");
        m.director.transfer_counter = 7;
        let s = region_transfer_step(&m);
        let mut two = one;
        two.float_range(0.0, rival_score(&m, B));
        assert_eq!(s.pending, vec![PendingTransfer { receiver: B, region: RegionId(12), queued_owner: C }]);
        assert_eq!((s.transfer_counter, s.rng), (0, two));
        // Chance 1 without a candidate: only the one draw.
        m.world.set_stance(B, C, Stance::Neutral).unwrap();
        let s = region_transfer_step(&m);
        assert_eq!((s.transfer_counter, s.rng, s.pending.len()), (7, one, 0));
    }

    #[test]
    fn a_due_transfer_destroys_the_garrison_and_reports_no_capture() {
        let mut m = world("1.0");
        m.world.forces.insert(ForceId(2000), MilitaryForce { id: ForceId(2000), faction: C, commander: None, units: Vec::new(), is_navy: false });
        m.world.regions.get_mut(&RegionId(12)).unwrap().garrison = Some(ForceId(2000));
        let stale = PendingTransfer { receiver: B, region: RegionId(10), queued_owner: C };
        let later = PendingTransfer { receiver: C, region: RegionId(11), queued_owner: B };
        m.director.pending = vec![stale, PendingTransfer { receiver: B, region: RegionId(12), queued_owner: C }, later];
        let mut events = Vec::new();
        // From an AI faction to an AI faction: only the queue is served.
        m.director_handover(C, B, &mut events);
        let r = &m.world.regions[&RegionId(12)];
        assert_eq!((r.owner, r.garrison), (B, None));
        assert!(!m.world.forces.contains_key(&ForceId(2000)));
        assert!(events.iter().any(|e| matches!(e, CampaignEvent::ForceDestroyed { .. })));
        assert!(!events.iter().any(|e| matches!(e, CampaignEvent::SettlementOccupied { .. })));
        // The stale entry (10 is A's, not C's) left the queue without a transfer; C's entry stays.
        assert_eq!(m.world.regions[&RegionId(10)].owner, A);
        assert_eq!(m.director.pending, vec![later]);
        // No step between two AI factions: the one draw is the new garrison's size (4 to 10 for B).
        let mut one = CaRng::new(RNG_SEED);
        let size = one.int_range(4, 10);
        assert!((4..=10).contains(&size));
        assert_eq!(m.director.rng, one);
    }

    /// A transfer moves none of the old owner's agents (`0x00A64AC0` → `0x008D9DC0` moves only
    /// armies and docked navies): C's gentleman beside the settlement stays where he is, C's.
    #[test]
    fn a_transfer_leaves_the_old_owners_agents_in_place() {
        let mut m = world("1.0");
        // C keeps a capital (a MADE-UP region 13), so it stays in the game.
        let mut r13 = m.world.regions[&RegionId(12)].clone();
        (r13.id, r13.key) = (RegionId(13), "test_region_13".into());
        m.world.regions.insert(r13.id, r13);
        m.world.faction_details.entry(C).or_default().capital = Some(RegionId(13));
        let mut agent = m.world.characters[&CharacterId(102)].clone();
        (agent.id, agent.faction, agent.kind) = (CharacterId(103), C, super::super::world::CharacterKind::Gentleman);
        agent.position = m.world.regions[&RegionId(12)].settlement.position;
        let pos = agent.position;
        m.world.characters.insert(agent.id, agent);
        m.director.pending = vec![PendingTransfer { receiver: B, region: RegionId(12), queued_owner: C }];
        m.director_handover(C, B, &mut Vec::new());
        assert_eq!(m.world.regions[&RegionId(12)].owner, B);
        let agent = &m.world.characters[&CharacterId(103)];
        assert_eq!((agent.faction, agent.position, agent.garrisoned_in), (C, pos, None));
    }

    #[test]
    fn the_turn_loop_steps_after_a_human_turn_and_transfers_at_the_receivers_turn_start() {
        let run = || {
            let mut m = world("1.0");
            m.start_campaign();
            assert_eq!(m.turn.current, Some(A));
            // A's end, C's start and the step (12 queued for B), C's turn, the round end, B's start
            // and the transfer, B's turn, A's start.
            m.apply(CampaignCommand::EndTurn).unwrap();
            m
        };
        let m = run();
        assert_eq!(m.turn.current, Some(A));
        assert_eq!(m.world.regions[&RegionId(12)].owner, B);
        assert!(m.director.pending.is_empty());
        assert_ne!(m.director.rng, CaRng::new(RNG_SEED));
        // Deterministic, and the director's state is hashed.
        assert_eq!(m.state_hash(), run().state_hash());
        let mut other = m.clone();
        other.director.transfer_counter += 1;
        assert_ne!(other.state_hash(), m.state_hash());
    }

    /// A queued transfer to a faction out of the game stays in the queue: the turn passes over that
    /// faction (`0x00948CF0`), so it never starts a turn and `0x00BA90E0` never serves it; once it
    /// is back in the game, its next turn start serves the entry.
    #[test]
    fn a_transfer_to_a_faction_out_of_the_game_waits_for_it_to_come_back() {
        let mut m = world("0.0");
        m.world.faction_details.entry(C).or_default().out_of_game = Some(true);
        let entry = PendingTransfer { receiver: C, region: RegionId(11), queued_owner: B };
        m.director.pending = vec![entry];
        m.start_campaign();
        for _ in 0..2 {
            let events = m.apply(CampaignCommand::EndTurn).unwrap();
            assert!(!events.iter().any(|e| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == C)));
            assert!(events.iter().any(|e| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == B)));
            assert_eq!(m.turn.current, Some(A));
            assert_eq!(m.director.pending, vec![entry]);
        }
        // A liberation brings C back (`0x008AE6E0` clears +0x824).
        m.reemerge_faction(C, B, true, &mut Vec::new());
        assert!(m.in_the_game(C));
        let events = m.apply(CampaignCommand::EndTurn).unwrap();
        assert!(events.iter().any(|e| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == C)));
        assert!(!m.director.pending.contains(&entry));
    }

    #[test]
    fn without_config_rows_nothing_is_transferred() {
        let mut m = world("0.0");
        with_rows(&mut m, Vec::clear);
        m.start_campaign();
        m.apply(CampaignCommand::EndTurn).unwrap();
        assert_eq!(m.world.regions[&RegionId(12)].owner, C);
        assert_eq!(m.director.transfer_counter, 1);
    }

    #[test]
    fn the_config_numbers_read_as_the_exes_parsers_read_them() {
        let f = parse_cdir_float;
        assert_eq!([f("0.125"), f("-0.5"), f("1."), f(".5"), f("1.5e2"), f("2.0E-1")], [Some(0.125), Some(-0.5), Some(1.0), Some(0.5), Some(150.0), Some(2.0 * 0.1)]);
        // A `.` is required; no `+`, no spaces, nothing after the number.
        for bad in ["10", "1e2", "+1.0", " 1.0", "1.0 ", "1.0x", "1.0e+2", ""] {
            assert_eq!(f(bad), None, "{bad:?}");
        }
        assert_eq!([parse_cdir_uint("1500"), parse_cdir_uint(""), parse_cdir_uint("4294967296")], [Some(1500), Some(0), Some(0)]);
        assert_eq!([parse_cdir_uint("-1"), parse_cdir_uint("1.0"), parse_cdir_uint(" 1")], [None, None, None]);
        assert_eq!([parse_cdir_int("-20"), parse_cdir_int("7"), parse_cdir_int("-")], [Some(-20), Some(7), Some(0)]);
        assert_eq!(parse_cdir_int("+7"), None);
    }

    fn at_war_with_scores(m: &mut CampaignModel) {
        let (o, s) = (ai_order(m), scores(m));
        at_war_step(m, &o, &s);
    }

    fn boost_with_scores(m: &mut CampaignModel) {
        let (o, s) = (ai_order(m), scores(m));
        boost_step(m, &o, &s);
    }

    fn xp_step_in_order(m: &mut CampaignModel) {
        let o = ai_order(m);
        xp_step(m, &o);
    }

    fn replenish_step_in_order(m: &mut CampaignModel) {
        let o = ai_order(m);
        replenish_step(m, &o);
    }

    /// The pool's rival scores, as [`director_desires`] computes them.
    fn scores(m: &CampaignModel) -> BTreeMap<FactionId, f32> {
        m.world.factions.keys().map(|&f| (f, rival_score(m, f))).collect()
    }

    #[test]
    fn the_at_war_desire_draws_each_warring_pair_once() {
        let mut m = world("0.01");
        let (sb, sc) = (rival_score(&m, B), rival_score(&m, C));
        assert!(sb > 0.0 && sc > 0.0);
        m.director.autoresolver_pairs = vec![(C, B), (A, B)];
        at_war_with_scores(&mut m);
        // B meets C first (B, C at war; A is human): one draw over both scores, B wins when the
        // roll is at most its score. The old (C, B) entry gives way.
        let mut rng = CaRng::new(RNG_SEED);
        let roll = rng.float_range(0.0, sc + sb);
        let pair = if sb < roll { (C, B) } else { (B, C) };
        assert_eq!((m.director.rng, m.director.autoresolver_pairs.clone()), (rng, vec![(A, B), pair]));
        // The CAI world's order puts C first: C is `a`.
        let mut m = world("0.01");
        m.world.ai_factions = vec![C, B, FactionId(99)];
        at_war_with_scores(&mut m);
        let pair = if sc < roll { (B, C) } else { (C, B) };
        assert_eq!((m.director.rng, m.director.autoresolver_pairs.clone()), (rng, vec![pair]));
        // The pirates lose without a draw.
        let mut m = world("0.01");
        Arc::make_mut(&mut m.rules).director.no_rival_keys = vec!["test_faction_c".into()];
        at_war_with_scores(&mut m);
        assert_eq!((m.director.rng, m.director.autoresolver_pairs.clone()), (CaRng::new(RNG_SEED), vec![(B, C)]));
    }

    /// The at-war desire walks each faction's own enemy list (the relation belief's `+0x40` list,
    /// `0x00ABAFB0`): the CAI world's factions in order, the rebels always, the others at war with
    /// it. B has two enemies, C and D, and the rebels R: B draws against C, then against D (R
    /// loses without a draw); C and D find their pair with B taken. In the reversed order D and C
    /// each draw against B first, as `a`.
    #[test]
    fn the_at_war_desire_walks_a_factions_enemies_in_the_cai_order() {
        const D: FactionId = FactionId(4);
        const R: FactionId = FactionId(5);
        let mut m = world("0.01");
        for (id, key) in [(D, "test_faction_d"), (R, "")] {
            let mut f = m.world.factions[&C].clone();
            (f.id, f.key, f.diplomacy) = (id, key.into(), BTreeMap::new());
            m.world.factions.insert(id, f);
            let force = MilitaryForce { id: ForceId(2000 + id.0 as u32), faction: id, commander: None, units: Vec::new(), is_navy: false };
            m.world.forces.insert(force.id, force);
        }
        m.world.set_stance(B, D, Stance::War).unwrap();
        assert!(!m.at_war(C, D) && m.at_war(B, R) && m.at_war(C, R));
        let s: BTreeMap<FactionId, f32> = [(B, 0.5), (C, 0.25), (D, 0.75)].into();
        let draw = |rng: &mut CaRng, a: FactionId, b: FactionId| {
            let roll = rng.float_range(0.0, s[&b] + s[&a]);
            if s[&a] < roll { (b, a) } else { (a, b) }
        };
        let mut n = m.clone();
        at_war_step(&mut n, &[B, C, D, R], &s);
        let mut rng = CaRng::new(RNG_SEED);
        let (bc, bd) = (draw(&mut rng, B, C), draw(&mut rng, B, D));
        assert_eq!((n.director.rng, n.director.autoresolver_pairs), (rng, vec![bc, bd, (B, R), (C, R), (D, R)]));
        let mut n = m.clone();
        at_war_step(&mut n, &[R, D, C, B], &s);
        let mut rng = CaRng::new(RNG_SEED);
        let (db, cb) = (draw(&mut rng, D, B), draw(&mut rng, C, B));
        assert_eq!((n.director.rng, n.director.autoresolver_pairs), (rng, vec![(D, R), db, (C, R), cb, (B, R)]));
    }

    #[test]
    fn the_boost_desire_pays_the_highest_rivals() {
        let mut m = world("0.01");
        let campaign = m.rules.campaign.clone();
        with_rows(&mut m, |r| {
            r.extend([row(ALL, ALL, BOOST_PERCENTAGE, "0.125"), row(ALL, ALL, BOOST_MIN, "5"), row(ALL, ALL, BOOST_MAX, "1"), row(ALL, ALL, BOOST_AMOUNT, "1500")]);
            r.push(row(&campaign, "test_faction_c", BOOST_AMOUNT, "-20"));
        });
        assert!(rival_score(&m, B) > rival_score(&m, C));
        let treasuries = |m: &CampaignModel| [A, B, C].map(|f| m.world.factions[&f].treasury);
        // MAX 1: only the highest score; the human is never served.
        let mut one = m.clone();
        boost_with_scores(&mut one);
        assert_eq!(treasuries(&one), [1000, 2500, 0]);
        // MAX 5: both, each its own amount; no draw.
        with_rows(&mut m, |r| r.insert(0, row(ALL, ALL, BOOST_MAX, "5")));
        boost_with_scores(&mut m);
        assert_eq!(treasuries(&m), [1000, 2500, -20]);
        assert_eq!(m.director.rng, CaRng::new(RNG_SEED));
    }

    #[test]
    fn the_xp_desire_counts_failed_rolls_and_draws_per_unit() {
        // Chance 1: B's army (two units, weight 4) is picked and each unit draws three steps; C,
        // without an army, draws only its chance roll; its counter is added at 0.
        let mut m = world("0.01");
        with_rows(&mut m, |r| r.push(row(ALL, ALL, XP_ARMY_BASE_CHANCE, "1.0")));
        m.director.xp_counters.insert(B, 6);
        xp_step_in_order(&mut m);
        let mut rng = CaRng::new(RNG_SEED);
        rng.float_range(0.0, 1.0);
        rng.float_range(0.0, 4.0);
        for _ in 0..6 {
            rng.float_range(0.0, 1.0);
        }
        rng.float_range(0.0, 1.0);
        assert_eq!((m.director.rng, m.director.xp_counters.clone()), (rng, [(B, 0), (C, 0)].into()));
        // Chance 0: each served faction's counter goes up, one draw each.
        let mut m = world("0.01");
        with_rows(&mut m, |r| r.push(row(ALL, ALL, XP_ARMY_EXTRA_CHANCE_PER_TURN, "0.0")));
        m.director.xp_counters.insert(C, 2);
        xp_step_in_order(&mut m);
        assert_eq!(m.director.xp_counters, [(B, 1), (C, 3)].into());
        let mut two = CaRng::new(RNG_SEED);
        two.float_range(0.0, 1.0);
        two.float_range(0.0, 1.0);
        assert_eq!(m.director.rng, two);
    }

    #[test]
    fn the_xp_pick_gives_each_army_its_own_weight() {
        assert_eq!([0.0, 4.0, 4.5, 13.0, 20.0].map(|r| pick_index([4.0, 9.0], r)), [Some(0), Some(0), Some(1), Some(1), Some(1)]);
        assert_eq!(pick_index(std::iter::empty(), 1.0), None);
    }

    fn replenish_rows(m: &mut CampaignModel) {
        with_rows(m, |r| {
            for v in [REPLENISH_FACTION_ARMY_ALLOWED, REPLENISH_MOBILE_ARMY_ALLOWED, REPLENISH_UNIT_ARMY_ALLOWED] {
                r.push(row(ALL, ALL, v, "1.0"));
            }
            r.extend([row(ALL, ALL, REPLENISH_ARMY_MIN_PCT, "0.85"), row(ALL, ALL, REPLENISH_ARMY_MAX_PCT, "0.95")]);
        });
    }

    #[test]
    fn the_replenish_desire_tops_up_one_drawn_unit_per_allowance() {
        // B's two units (80 of 100 men, below 85) both need it; the faction allowance of 1 lets one
        // be drawn, then it gets floor(100 × a fraction in [0.85, 0.95]) men.
        let mut m = world("0.01");
        replenish_rows(&mut m);
        replenish_step_in_order(&mut m);
        let mut rng = CaRng::new(RNG_SEED);
        let i = rng.int_range(0, 1) as usize;
        let men = (100.0 * rng.float_range(0.85, 0.95)).floor() as u32;
        assert!((85..=95).contains(&men));
        let mut want = [80, 80];
        want[i] = men;
        assert_eq!(m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).collect::<Vec<_>>(), want);
        assert_eq!(m.director.rng, rng);
        // Allowances of 2 serve both.
        let mut m = world("0.01");
        replenish_rows(&mut m);
        with_rows(&mut m, |r| {
            r.insert(0, row(ALL, ALL, REPLENISH_MOBILE_ARMY_ALLOWED, "2.0"));
            r.insert(0, row(ALL, ALL, REPLENISH_FACTION_ARMY_ALLOWED, "2.0"));
        });
        replenish_step_in_order(&mut m);
        assert!(m.world.forces[&ForceId(1001)].units.iter().all(|u| (85..=95).contains(&u.men)));
        // An army aboard a navy, or with enough men, draws nothing.
        let mut m = world("0.01");
        replenish_rows(&mut m);
        m.world.embarked.insert(ForceId(1001), ForceId(5));
        replenish_step_in_order(&mut m);
        assert_eq!(m.director.rng, CaRng::new(RNG_SEED));
        let mut m = world("0.01");
        replenish_rows(&mut m);
        with_rows(&mut m, |r| r.insert(0, row(ALL, ALL, REPLENISH_ARMY_MIN_PCT, "0.8")));
        replenish_step_in_order(&mut m);
        assert_eq!(m.director.rng, CaRng::new(RNG_SEED));
    }

    #[test]
    fn a_human_to_ai_handover_runs_the_desires_and_hashes_their_state() {
        let mut m = world("0.0");
        with_rows(&mut m, |r| r.extend([row(ALL, ALL, BOOST_MIN, "1"), row(ALL, ALL, BOOST_MAX, "1"), row(ALL, ALL, BOOST_AMOUNT, "1500")]));
        let mut events = Vec::new();
        m.director_handover(A, C, &mut events);
        assert_eq!(m.world.factions[&B].treasury, 2500);
        assert_eq!(m.director.autoresolver_pairs.len(), 1);
        assert_eq!(m.director.xp_counters.len(), 2);
        let mut other = m.clone();
        other.director.xp_counters.insert(B, 9);
        assert_ne!(other.state_hash(), m.state_hash());
        let mut other = m.clone();
        other.director.autoresolver_pairs[0] = (C, A);
        assert_ne!(other.state_hash(), m.state_hash());
    }

    /// [`world`] with a MADE-UP slot in C's region 12 whose building allows a gun, a horse, a foot
    /// unit and a ship (keys in that order), at full health, or damaged when `health` is below 100.
    fn mixed_world(health: u32) -> CampaignModel {
        use super::super::world::{BuildingRef, RegionSlot};
        let mut m = world("1.0");
        let mut rules = (*m.rules).clone();
        let base = rules.units["test_unit"].clone();
        for (key, category, is_naval) in [("a_gun", "artillery", false), ("b_horse", "cavalry", false), ("c_foot", "infantry", false), ("d_ship", "naval_frigate", true)] {
            rules.units.insert(key.into(), super::super::rules::UnitRules { category: category.into(), is_naval, ..base.clone() });
            rules.allow_unit_for_all(key);
        }
        let mut level = rules.buildings["test_building_level"].clone();
        level.units_allowed = ["a_gun", "b_horse", "c_foot", "d_ship"].map(String::from).to_vec();
        rules.buildings.insert("test_mixed_level".into(), level);
        m.rules = Arc::new(rules);
        m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(RegionSlot {
            key: "test_slot_mixed".into(),
            slot_type: "test_slot".into(),
            building: Some(BuildingRef { level_key: "test_mixed_level".into(), health }),
            position: None,
            port: false,
            holder: None,
            id: 0,
        });
        m
    }

    /// The unit keys of the garrison now in `region`.
    fn garrison_units(m: &CampaignModel, region: RegionId) -> Vec<String> {
        let g = m.world.regions[&region].garrison.expect("a garrison");
        m.world.forces[&g].units.iter().map(|u| u.unit_key.clone()).collect()
    }

    /// `0x00BA8400`: the count drawn first (4 to 10 for an AI receiver), then the land units sorted
    /// infantry, cavalry, artillery and taken round and round; the ship is left out. One army, led by
    /// a new colonel garrisoned in the settlement.
    #[test]
    fn a_transfer_raises_its_drawn_count_of_units_sorted_by_kind() {
        for seed in [RNG_SEED, 1, 2, 3, 77] {
            let mut m = mixed_world(100);
            m.director.rng = CaRng::new(seed);
            let mut one = CaRng::new(seed);
            let count = one.int_range(4, 10) as usize;
            m.director_transfer(RegionId(12), B, &mut Vec::new());
            assert_eq!(m.director.rng, one, "one draw, none while the units are gathered");
            let want: Vec<String> = ["c_foot", "b_horse", "a_gun"].iter().cycle().take(count).map(|s| s.to_string()).collect();
            assert_eq!(garrison_units(&m, RegionId(12)), want);
            let army = &m.world.forces[&m.world.regions[&RegionId(12)].garrison.unwrap()];
            assert_eq!(army.faction, B);
            let colonel = &m.world.characters[&army.commander.unwrap()];
            assert_eq!((colonel.kind, colonel.garrisoned_in), (super::super::world::CharacterKind::Colonel, Some(RegionId(12))));
            assert!(army.units.iter().all(|u| u.men == 100 && u.max_men == 100));
        }
    }

    /// A human receiver gets 3 to 6 units.
    #[test]
    fn a_human_receiver_gets_three_to_six_units() {
        for seed in [RNG_SEED, 1, 2, 3, 77] {
            let mut m = mixed_world(100);
            m.director.rng = CaRng::new(seed);
            let count = CaRng::new(seed).int_range(3, 6) as usize;
            m.director_transfer(RegionId(12), A, &mut Vec::new());
            assert_eq!(garrison_units(&m, RegionId(12)).len(), count);
        }
    }

    /// The price flag does not keep a unit out of the belief (`0x00D5F930` keeps entries flagged only
    /// [`ENTRY_TOO_DEAR`](super::super::commands::ENTRY_TOO_DEAR)): a receiver with no money still
    /// gets its garrison.
    #[test]
    fn a_receiver_too_poor_to_recruit_still_gets_the_garrison() {
        let mut m = mixed_world(100);
        m.world.factions.get_mut(&B).unwrap().treasury = 0;
        m.director_transfer(RegionId(12), B, &mut Vec::new());
        assert_eq!(garrison_units(&m, RegionId(12))[0], "c_foot");
    }

    /// A unit the region cannot recruit (its building is damaged) is raised when a region of the
    /// receiver's across a boundary can (`0x00BAF9A0`): B's 11 allows `test_unit` and
    /// `test_recruit`, and of those only `test_unit` is in 12's list, so with 12's own building damaged
    /// only `test_unit` is raised, and only while 11 is B's and linked to 12. Without a unit to raise, the draw is still made and nothing is raised.
    #[test]
    fn a_unit_the_region_cannot_recruit_comes_from_a_receivers_neighbour() {
        let mut m = mixed_world(50);
        let mut rules = (*m.rules).clone();
        rules.buildings.get_mut("test_mixed_level").unwrap().units_allowed.push("test_unit".into());
        m.rules = Arc::new(rules);
        let before = m.clone();
        m.director_transfer(RegionId(12), B, &mut Vec::new());
        assert!(garrison_units(&m, RegionId(12)).iter().all(|u| u == "test_unit"));

        // 11 no longer linked to 12: no unit, no army, the draw made all the same.
        for edit in [
            (|m: &mut CampaignModel| m.world.region_neighbours.clear()) as fn(&mut CampaignModel),
            |m: &mut CampaignModel| m.world.regions.get_mut(&RegionId(11)).unwrap().owner = A,
        ] {
            let mut m = before.clone();
            edit(&mut m);
            let mut one = m.director.rng;
            one.int_range(4, 10);
            let forces = m.world.forces.len();
            m.director_transfer(RegionId(12), B, &mut Vec::new());
            assert_eq!((m.world.regions[&RegionId(12)].garrison, m.world.forces.len()), (None, forces));
            assert_eq!(m.director.rng, one);
        }
    }

    /// A slot with its own recruitment manager adds its building's land units whatever the queue
    /// flags say (`0x00D9CD60`), unless it is a port slot (type 3, `0x00BE6EF0`), which lists ships:
    /// with B at its cap of foot units, the foot unit is raised only from a non-port slot that has one.
    #[test]
    fn a_slot_with_its_own_queue_adds_its_land_units_unless_it_is_a_port() {
        let capped = |port: bool, slot_type: &str| {
            let mut m = mixed_world(100);
            let mut rules = (*m.rules).clone();
            rules.units.get_mut("c_foot").unwrap().unit_cap = 1;
            m.rules = Arc::new(rules);
            m.world.forces.get_mut(&ForceId(1001)).unwrap().units[0].unit_key = "c_foot".into();
            let slot = m.world.regions.get_mut(&RegionId(12)).unwrap().slots.last_mut().unwrap();
            (slot.port, slot.slot_type) = (port, slot_type.into());
            m.director_transfer(RegionId(12), B, &mut Vec::new());
            garrison_units(&m, RegionId(12))[0].clone()
        };
        assert_eq!(capped(false, "test_slot"), "b_horse");
        assert_eq!(capped(true, PORT_SLOT_TYPE), "b_horse");
        assert_eq!(capped(true, "test_slot"), "c_foot");
    }
}

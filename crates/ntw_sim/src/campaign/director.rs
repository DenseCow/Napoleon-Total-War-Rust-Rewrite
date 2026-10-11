//! The campaign director (`CDIR_INTERFACE`, campaign +0xF80): the original's background rules that
//! steer the campaign around the human players. Modelled so far: its one action that changes the
//! map, `CDIR_INTENTION_TRANSFER_REGION_OWNERSHIP` (`0x00B93840`): a minor faction's region far from
//! any human's eyes is handed to a major AI power at war with it, the garrison destroyed. Its other
//! desires are not modelled (CAMPAIGN_FIDELITY.md §Campaign director).
//!
//! ## When it runs (CONFIRMED, `AdvanceFactionTurn` `0x008A9920`)
//! At each turn handover, after the starting faction's turn start, [`CampaignModel::director_handover`]
//! ([`super::TurnStep::Handover`]):
//! 1. the queued transfers whose receiver is the starting faction are carried out (`0x00BA90E0` →
//!    `0x00BCA430` → `0x00B58A30`): the region changes hands when its owner is still the one it had
//!    when the transfer was queued and it is still [transferable](transferable); every such entry
//!    leaves the queue either way;
//! 2. when the ending faction is human and the starting one is not (`0x00BC6DF0`), the director's
//!    pool runs (`0x00BA9220` via `0x008F1F40`, loop `0x00BBC880`): the beliefs (each faction's
//!    rival score, [`rival_score`]) are refreshed, then the desires; the region-transfer desire takes
//!    one step ([`region_transfer_step`], `0x00BAE0A0`), through the seam
//!    [`super::seams::DIRECTOR_REGION_TRANSFER`].
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
//! id order with [`super::World::region_neighbours`].
//!
//! Evidence tags: CONFIRMED unless marked. PROVISIONAL: a human's fleets in a sea area are not
//! seen by the transferable test ([`human_inside`]); its siege test is never true (no sieges in
//! the model, [`transferable_at`]); the relation is not the AI's stored copy after a deal proposal
//! ([`rival_score`]); the director's other desires and intentions
//! draw from the same RNG (`0x00BBD6E0` is also called by `0x00BAC560`, `0x00BAC9C0`,
//! `0x00BC06E0`, `0x00BD4A60` and code at `0x00BAED7A`) and are not modelled, so the RNG's
//! sequence matches the original's only until one of them draws.

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
    /// row or its value does not read as a number.
    pub fn float(&self, campaign: &str, variable: &str) -> f32 {
        self.value(campaign, ALL, variable).and_then(|v| v.trim().parse().ok()).unwrap_or(0.0)
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
    /// The faction that receives the region at its next turn start.
    pub receiver: FactionId,
    /// The region.
    pub region: RegionId,
    /// The region's owner when the transfer was queued: the transfer happens only if it still is.
    pub queued_owner: FactionId,
}

/// The director's state (`CDIR_INTERFACE`): saved and hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirectorState {
    /// +0x60: the director's own RNG (read from the start position or save; [`RNG_SEED`] without one;
    /// an original record older than version 2 seeds it from the campaign RNG).
    pub rng: CaRng,
    /// The region-transfer desire's +0xFC: failed rolls since the last transfer (u32, wrapping).
    pub transfer_counter: u32,
    /// +0x48: the queued transfers, in queue order.
    pub pending: Vec<PendingTransfer>,
}

impl Default for DirectorState {
    /// A new campaign's.
    fn default() -> Self {
        DirectorState { rng: CaRng::new(RNG_SEED), transfer_counter: 0, pending: Vec::new() }
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

/// A major power (faction +0x524, `0x00C3FB20`).
fn is_major(model: &CampaignModel, f: FactionId) -> bool {
    model.world.faction_details.get(&f).and_then(|d| d.major) == Some(true)
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

/// A faction's player rival score (`0x00BD21B0`, the belief stored at +0x100): how much of a rival
/// to the human players it is, the highest over the humans of
/// `m × (RELATION_PCT × (1 − relation) + VICTORY_REGIONS_PCT × b + PROXIMITY_PCT × c)`, capped at
/// 1, where `b` is the share of its regions the human's victory conditions name, `c` the share of
/// its regions' neighbours the human owns (its own regions count among them), and `m` 1 for a
/// major power, else `MINOR_FACTION_WEIGHT`. 0 for the rebels, the [`DirectorRules::no_rival_keys`],
/// a human and a faction out of the game, and with no humans.
///
/// The relation is read from the faction's CAI relation belief (`0x00A74A20`, entry `+8` of the
/// list at `+0x100` found by `0x00A89E00`), a stored copy that `0x00ABAFB0` refreshes only when it
/// is read while dirty (`0x00CCCF70`). CONFIRMED: the belief depends on its own faction's and
/// every CAI world faction's BDI core (`0x00A96CB0`), and the CAI update (`0x00A761F0`, every frame
/// from `0x008F4120`) marks the core of the faction whose turn just ended dirty when the active
/// faction changes (`0x00CF7F60` → `0x00CF0AB0`), which reaches every relation belief. The pool
/// runs in the handover (`0x008A9920`), before that frame, so the copy it reads was last marked at
/// the human's turn start: computing the relation now gives the same value whenever nothing else
/// read the belief during the human's turn (a war the human declares mid-turn is seen; test
/// `a_war_declared_mid_turn_counts_for_the_rival_score`).
/// PROVISIONAL: the AI's evaluation of a human's deal proposal reads (refreshes) the recipient's
/// belief (`0x00CCB810` / `0x00CCB210` → `0x00A74A20`), so in the exe a later change in the same
/// turn (a war declared after a proposal) is not seen until the next turn change; the model's
/// proposal is no model step and keeps no copy, and the other dirty marks (the CAI interface's
/// re-mark flag `+0x6DC`, whose setter is not found) are not traced. BACKLOG §0 "Stored CAI
/// relation belief".
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
    let weight = if is_major(model, f) { 1.0 } else { pct(RIVAL_MINOR_FACTION_WEIGHT) };
    let mine: Vec<RegionId> = model.world.regions.values().filter(|r| r.owner == f).map(|r| r.id).collect();
    // The areas linked to its regions' areas, each once (seas included: they count in the share's
    // denominator, never in its numerator).
    let neighbours: BTreeSet<usize> = areas.iter().filter(|a| a.region.is_some_and(|r| r.owner == f)).flat_map(|a| a.links.iter().copied()).collect();
    let mut best = 0.0f32;
    for &h in &model.turn.humans {
        let a = 1.0 - relation_value(model, f, h);
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
    /// Whether a human has a mobile in it ([`human_inside`]), found once when the areas are built.
    human: bool,
}

/// The director's map areas ([`Area`]), with their human flags. Cold path: built once per desire step,
/// and once per due transfer (a transfer changes owners).
fn areas(model: &CampaignModel) -> Vec<Area<'_>> {
    let w = &model.world;
    if !w.ai_areas.is_empty() {
        let by_key: BTreeMap<&str, &Region> = w.regions.values().map(|r| (r.key.as_str(), r)).collect();
        return w.ai_areas
            .iter()
            .map(|a| {
                let region = by_key.get(a.key.as_str()).copied();
                Area { region, links: a.links.clone(), human: region.is_some_and(|r| human_inside(model, r)) }
            })
            .collect();
    }
    let index: BTreeMap<RegionId, usize> = w.regions.keys().enumerate().map(|(i, &id)| (id, i)).collect();
    w.regions
        .values()
        .map(|r| Area {
            region: Some(r),
            links: w.region_neighbours.get(&r.id).map_or(&[][..], Vec::as_slice).iter().filter_map(|n| index.get(n).copied()).collect(),
            human: human_inside(model, r),
        })
        .collect()
}

/// Whether a human has a mobile in the area (`0x00BB51B0` over the CAI region's mobiles, #9
/// +0x18C, as the loader `0x00BECCE0` reads them): the mobiles are the forces (at their leader) and
/// the agents situated in it (`cai_world.rs`; matches the original's saves). A land area holds the
/// characters [`super::economy::in_region`] places in its region. PROVISIONAL: a sea area's
/// mobiles (the fleets at sea in it) are not found, as the path grid gives sea cells no region.
fn human_inside(model: &CampaignModel, reg: &Region) -> bool {
    model.world.characters.values().any(|c| model.is_human(c.faction) && super::economy::in_region(model, reg, c))
}

/// Whether any human sees the region's settlement or one of its other slots (`0x00BB55A0`): each
/// position is tested for every human with a shroud (`0x00BB5640`; a human without one is skipped)
/// with [`CampaignModel::sees_fully`] (`0x00B7A1D0`). The settlement's own slots (keys
/// `settlement:...`) stand at the settlement and are skipped.
fn seen_by_a_human(model: &CampaignModel, reg: &Region) -> bool {
    let to_f32 = |p: (crate::fixed::Fixed20, crate::fixed::Fixed20)| (p.0.to_f32(), p.1.to_f32());
    let mut positions = std::iter::once(to_f32(reg.settlement.position))
        .chain(reg.slots.iter().filter(|s| !s.key.starts_with("settlement:")).filter_map(|s| s.position.map(to_f32)));
    positions.any(|p| model.turn.humans.iter().any(|&h| model.world.shrouds.contains_key(&h) && model.sees_fully(h, p)))
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
    if !model.world.factions.contains_key(&owner) || is_major(model, owner) || model.is_human(owner) || area.human {
        return false;
    }
    for link in area.links.iter().filter_map(|&j| areas.get(j)) {
        if link.region.is_some_and(|n| model.is_human(n.owner)) || link.human {
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
/// major power, not human and at war with the region's owner, in link order, weighted by its
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
            if o == reg.owner || !model.world.factions.contains_key(&o) || !is_major(model, o) || model.is_human(o) || !model.at_war(reg.owner, o) {
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
    let mut sum = 0.0f32;
    candidates.iter().find(|c| {
        sum += c.weight;
        sum >= roll
    }).or(candidates.last())
}

/// The original's region-transfer desire step (`0x00BAE0A0`; see the module docs): the director's
/// state after one step. The rule behind the seam [`super::seams::DIRECTOR_REGION_TRANSFER`].
pub fn region_transfer_step(model: &CampaignModel) -> DirectorState {
    let mut state = model.director.clone();
    let rules = &model.rules.director;
    let campaign = model.rules.campaign.as_str();
    let base = rules.float(campaign, TRANSFER_BASE_CHANCE);
    let extra = rules.float(campaign, TRANSFER_EXTRA_CHANCE_PER_TURN);
    let mut chance = base + extra * state.transfer_counter as f32;
    // NaN becomes 1, as the exe's comparisons give.
    if chance.is_nan() || chance >= 1.0 {
        chance = 1.0;
    } else if chance <= 0.0 {
        chance = 0.0;
    }
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

impl CampaignModel {
    /// The director's part of a turn handover from `ending` to `starting` (see the module docs):
    /// the transfers queued for `starting` are carried out, then, from a human to an AI faction, the
    /// region-transfer desire steps (through its seam).
    pub fn director_handover(&mut self, ending: FactionId, starting: FactionId, events: &mut Vec<CampaignEvent>) {
        self.apply_region_transfers(starting, events);
        if self.is_human(ending) && !self.is_human(starting) {
            let mut writes = ModWrites::default();
            let rules = std::sync::Arc::clone(&self.rules);
            self.director = rules.seams.director_region_transfer.rule()(self, &mut writes);
            self.mod_state.apply(&mut writes);
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
    /// capture is reported (no `SettlementOccupied`).
    fn director_transfer(&mut self, region: RegionId, receiver: FactionId, events: &mut Vec<CampaignEvent>) {
        if let Some(garrison) = self.world.regions.get(&region).and_then(|r| r.garrison) {
            self.destroy_force(garrison, events);
        }
        self.take_settlement(region, receiver, None);
    }
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
            row(ALL, ALL, TRANSFER_UBER_FACTION_MODIFIER, "10"),
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
            configs: vec![row(ALL, ALL, "V", "1"), row("camp", ALL, "V", "2"), row("camp", "f", "V", "3"), row("camp", "f", "V", "4"), row(ALL, ALL, "W", "x")],
            ..Default::default()
        };
        assert_eq!(r.value("camp", "f", "V"), Some("3"));
        assert_eq!(r.value("camp", "g", "V"), Some("2"));
        assert_eq!(r.value("other", "f", "V"), Some("1"));
        assert_eq!(r.value(ALL, "f", "V"), Some("1"));
        assert_eq!(r.value("camp", "f", "missing"), None);
        // A float is always read with faction `all`; an unreadable value and a missing row read 0.
        assert_eq!((r.float("camp", "V"), r.float("camp", "W"), r.float("camp", "missing")), (2.0, 0.0, 0.0));
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
        with_rows(&mut capped, |r| r.insert(0, row(ALL, ALL, RIVAL_VICTORY_REGIONS_PCT, "5")));
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

    /// The relation belief the rival score reads was last marked dirty at the human's turn start,
    /// so the pool's read at the handover refreshes it: a war the human declares mid-turn counts.
    #[test]
    fn a_war_declared_mid_turn_counts_for_the_rival_score() {
        let mut m = world("0.01");
        let at_peace = rival_score(&m, B);
        m.turn = super::super::turn::TurnState::in_turn_of(A, vec![A]);
        m.apply(super::super::CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
        let a = 1.0 - relation_value(&m, B, A);
        assert_eq!(m.world.stance(B, A), Stance::War);
        assert!((rival_score(&m, B) - (0.2 * a + 0.4 + 0.4 * 0.5)).abs() < 1e-6);
        assert!(rival_score(&m, B) > at_peace);
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
        let mut m = world("0");
        with_rows(&mut m, |r| r.retain(|r| r.variable != TRANSFER_EXTRA_CHANCE_PER_TURN));
        let s = region_transfer_step(&m);
        let mut one = CaRng::new(RNG_SEED);
        one.float_range(0.0, 1.0);
        assert_eq!((s.transfer_counter, s.rng, s.pending.len()), (1, one, 0));
        // Chance 1: a transfer to B is queued with the current owner and the counter goes back to
        // 0; two draws.
        let mut m = world("1");
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
        let mut m = world("1");
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
        assert_eq!(m.director.rng, CaRng::new(RNG_SEED), "no step between two AI factions");
    }

    #[test]
    fn the_turn_loop_steps_after_a_human_turn_and_transfers_at_the_receivers_turn_start() {
        let run = || {
            let mut m = world("1");
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

    #[test]
    fn without_config_rows_nothing_is_transferred() {
        let mut m = world("0");
        with_rows(&mut m, Vec::clear);
        m.start_campaign();
        m.apply(CampaignCommand::EndTurn).unwrap();
        assert_eq!(m.world.regions[&RegionId(12)].owner, C);
        assert_eq!(m.director.transfer_counter, 1);
    }
}

//! A settlement's siege on the campaign map and its round-start attrition (CAMPAIGN_FIDELITY.md
//! "Round start", siege attrition; CONFIRMED by disassembly unless a line says otherwise).
//!
//! The siege state is the settlement's `SIEGEABLE_GARRISON_RESIDENCE` (CONFIRMED, the ESF reader
//! `0x00AF4B00` / writer `0x00B7A5F0`): #2 the turns the garrison holds out before it must surrender
//! (+0x108), #3 the turns it has been besieged (+0x10C) and #13 the besieging forces (+0x12C, each
//! force's +0x7C links back). The first besieger (`0x00B112F0`) sets #2 from the residence's virtual
//! +0xB4 (a settlement: 2 below 25,000 people, 3 below 100,000, 4 below 1,000,000, 5 below
//! 10,000,000, else 6, `0x00B57B10`; a fort: twice its level + 2, `0x00B57B00`) and #3 to 0; the
//! turns left before surrender are #2 − #3, or 0 once #3 reaches #2 (`0x008AF520`, the
//! `siege_turns_until_surrender` script value).
//!
//! At each round start (`0x0096C050` → `0x00AAAF60` per region of world +0x34, in `REGIONS_ARRAY`
//! order, after the computed factors and before the trade update), a settlement under siege
//! (virtual +0x98: its besieger list is not empty, `0x00A8B9C0`) runs `0x00B796B0`, then each of the
//! region's forts under siege does the same:
//!
//! 1. #3 goes up by 1 (no cap), with or without a garrison.
//! 2. With a garrison army (virtual +0x1C): the percent `p` of [`attrition_percent`] from
//!    `n = min(#3, #2)`.
//! 3. Each garrison unit in order, but the commander's own (army +0x74 → character +0x2B0, no draw):
//!    one campaign RNG draw (model +0xFB8, the `0x005F0880` range draw inlined at `0x00B797B2`,
//!    [`crate::rng::CaRng::int_range`]`(0, p)`) gives the percent `r` it loses; `0x00B5C0E0` removes
//!    `men · r / 100` men (all of them at `r` ≥ 100) through `0x008AABB0`. A unit left at or below
//!    its minimum strength (`0x008F68F0`, [`super::economy::unit_minimum_men`]) is kept for step 4.
//! 4. After the loop, each kept unit's character (unit +0xF4) dies (`0x00A0C6F0(1)`) and the unit is
//!    deleted.
//!
//! The siege ends when its besieger list empties. A besieger leaves it through one path,
//! [`CampaignModel::end_force_siege`] (`EndForceSiegeOfSettlement` `0x008BD550`), reached from
//! (CONFIRMED, disassembly):
//! - its destruction: the military force's destructor `0x0088CE30` calls it at `0x0088CE93` when the
//!   force's siege object +0x94 is set and the campaign is not being torn down (+0xFC5), before the
//!   base teardown `0x0088ECC0` ([`CampaignModel::remove_force`]);
//! - any change of the settlement's owner (`TransferSettlementOwnership` `0x00B449F0` fires the
//!   settlement's owner-changed event +0x24 at `0x00B44B31`, after `0x00A64AC0`; the siege object's
//!   listener `0x008DB7D0` ends the siege (its call `0x008DB84A`) when the besieger's faction is the new owner or not at war
//!   with it, `IsFactionAtWarWith` `0x008CE9B0`): captures, deals, liberations and the director's
//!   transfer alike ([`CampaignModel::end_sieges_after_owner_change`]);
//! - a capture, for the captor's own siege, again after the owner change (`0x00B586E9`,
//!   `0x00B5898A`, `0x00B58D3B` in the three capture variants, each guarded by the force's siege
//!   object +0x94: a no-op after the listener);
//! - a peace (`0x00B262C0` → `0x00A25570` per commander of either side, the call `0x00A255C5`):
//!   his force's siege of a settlement the other side owns (target virtual +0x14 equal to it,
//!   `0x00898B20`) ends ([`CampaignModel::end_siege_on_peace`]); a besieger with no commander is
//!   not reached;
//! - the start of the besieging general's next order (the order helper `0x008FF470`, the call at
//!   `0x008FF4B7` through `0x00A0B8C0`; CONFIRMED by decompile): every order but the settlement order onto
//!   the besieged settlement itself ([`CampaignModel::begin_order`]);
//! - a battle's casualty step for a force left with no units (`0x008AC010`, the call at `0x008AC130`;
//!   CONFIRMED by decompile): the emptied force is removed ([`CampaignModel::remove_force`]).
//!
//! Each end that finds a siege also runs the trade price update with flag 0 (`0x00BCB020(0)` at
//! `0x008BD65B`), as the exe's `0x008BD550` does.
//!
//! Its other callers (CONFIRMED, decompile):
//! - the end-siege command (`0x00932760`, the call `0x00932794`; [`CampaignModel::end_siege_command`]);
//! - a siege's fall, which ends every siege of the taken settlement (`0x00B29080`, the call `0x00B290B0`,
//!   reached only from `0x008C2120`; [`CampaignModel::end_all_sieges`]);
//! - the capture variants' own besieger step (`0x00B584ED`, `0x00B5881D`, `0x00B58BD8`: the captor's
//!   siege, as above; `0x00B584A0` also sends event 253);
//! - the revolution's end (`0x008BEB42` in `0x008BEAA0`), the listener `0x008DB7A0` (`0x008DB7BD`), the
//!   CAI's region step (`0x00A787C0` in `0x00A787A0`) and the post-battle placement (`0x008F801B` in
//!   `0x008F7880`: battle types other than 6 to 9 when `0x008CE7B0` fails, the character's army's
//!   siege). PROVISIONAL: these four do not end a siege here yet (BACKLOG "Sieges").
//!
//! How a siege starts, falls and is assaulted (CONFIRMED, decompile):
//! - An army's attack order on a settlement of a faction it is at war with arrives
//!   ([`CampaignModel::attack_settlement`], arrival `0x0094B600`): unless another army besieges it first,
//!   the army lays siege (`0x008AF6A0` → the siege object `0x0088A0D0` → AddBesieger `0x00B112F0`,
//!   [`CampaignModel::start_siege`]), then assaults when the order does not hold and it can assault now
//!   ([`CampaignModel::can_assault_now`], `0x008B26C0`, the
//!   [`siege.assault_allowed`](super::seams::SIEGE_ASSAULT_ALLOWED) seam): the settlement has no walls or the army has a
//!   siege unit ([`CampaignModel::has_siege_units`], `0x008CE1B0`). The assault (`0x008E1A90`) is a
//!   battle ([`CampaignModel::assault`]); the assault command (`0x009365D0`) starts it later.
//! - At its commander's turn start, a besieging army's siege turn starts (`0x008F2290` → `0x008F2F30`
//!   sets the siege object's +0x84, [`CampaignModel::start_siege_turn`]) and the army damages the walls
//!   (the [`siege.wall_damage`](super::seams::SIEGE_WALL_DAMAGE) seam, [`siege_wall_damage`]: the walls'
//!   `FORTIFICATION_DAMAGE_INFO` entry, [`SiegeState::fortifications`], takes a hit every walls level + 1
//!   turns besieged, at most 4, each taking 0.1 off its factor and setting the walls' health to factor · 100;
//!   the capture's loot and occupy and the walls' repair read and reset the same entry). Then at the start of the
//!   faction's turn (turn-phase state 0 of `0x0094A940`, `0x008C2120` called until it returns 0), each
//!   army of the faction whose siege turn started asks once, in character-list order, whether the
//!   settlement falls to it (`0x008F9160`, the [`siege.fall`](super::seams::SIEGE_FALL)
//!   seam, [`siege_falls`]): with no garrison, or once #3 reaches #2. A fallen settlement's garrison
//!   army is destroyed and the army takes it with the capture report (`0x00B58560`), then every other
//!   siege of it ends ([`CampaignModel::siege_falls_at_turn`]).
//!
//! The model has no forts under siege (`World::forts` is empty in every shipped campaign, and its
//! [`super::world::Fort`] carries no residence or fortification entries), so only settlements run any of
//! this. A fort's surrender turns would be twice its level + 2 (`0x00B57B00`); its wall damage interval its
//! level (+0x188) + 1, hitting its first entry of any kind (its +0xC0, `0x00B42A70`) with no building
//! health; and the map-fort repair (`0x00B4E4C0`) restores every entry when it completes (`0x00B01890`,
//! CAMPAIGN_FIDELITY.md "Sieges").
//!
//! The model calls it through the [`siege.attrition`](super::seams::SIEGE_ATTRITION) seam
//! ([`CampaignModel::siege_attrition_round_start`]).

use super::commands::CommandError;
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::mod_state::ModWrites;
use super::world::CampaignModel;

/// The siege state of a region's settlement (its `SIEGEABLE_GARRISON_RESIDENCE` #2, #3, #13 and
/// `FORTIFICATIONS_BLOCK`; see the module docs).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SiegeState {
    /// The forces besieging the settlement (#13, residence +0x12C), in the list's order.
    pub besiegers: Vec<ForceId>,
    /// The turns the garrison holds out before it must surrender (#2, +0x108), set when the siege
    /// starts.
    pub surrender_turns: u32,
    /// The turns the settlement has been besieged (#3, +0x10C).
    pub turns_besieged: u32,
    /// The residence's fortification damage entries (`FORTIFICATIONS_BLOCK`, residence +0x144 capacity,
    /// +0x148 count, +0x14C 28-byte entries), in the block's order. Two paths add them: a loaded campaign
    /// (the ESF reader `0x00AF4B00` at `0x00AF4FFC`) and a finished walls item ([`Self::add_default_fortifications`],
    /// `0x00B143E0`; its other caller `0x00B457D0` runs only in the startpos preprocessor); the wall damage,
    /// loot, occupy and repair read and overwrite the first entry of kind 0 ([`Self::walls_damage`]).
    /// Vanilla (CONFIRMED, `startpos_fortification_damage_loads`): egy_napoleon's egy_palestine,
    /// egy_galilee and egy_north_sinai and ita_napoleon's and mp_ita_napoleon's ita_mantova carry 4
    /// entries (kinds 0 to 3, full health, no hits); every other residence starts with none.
    #[cfg_attr(feature = "serde", serde(default))]
    pub fortifications: Vec<FortificationDamage>,
}

impl SiegeState {
    /// `true` while a force besieges the settlement (the residence's virtual +0x98, `0x00A8B9C0`).
    pub fn under_siege(&self) -> bool {
        !self.besiegers.is_empty()
    }

    /// The turns left before the garrison must surrender: #2 − #3, or 0 once #3 reaches #2
    /// (`0x008AF520`).
    pub fn turns_until_surrender(&self) -> u32 {
        self.surrender_turns.saturating_sub(self.turns_besieged)
    }

    /// The walls' damage entry: the first fortification entry of kind 0 (the settlement's virtual +0xC0,
    /// `0x00B42AA0`, CONFIRMED by decompile: it copies the first 28-byte entry whose +0x14 is 0 and
    /// returns true, false when there is none; its key argument is not read). A fort's +0xC0 is another
    /// query (`0x00B42A70`, fort vtable `0x0137D3E4`: the first entry of any kind), which the model's
    /// forts do not have (module docs).
    pub fn walls_damage(&self) -> Option<&FortificationDamage> {
        self.fortifications.iter().find(|e| e.kind == WALLS_DAMAGE_KIND)
    }

    /// [`Self::walls_damage`] to change in place: the exe changes a copy and writes it back over the first
    /// entry of the same kind (`0x00B69AF0`, CONFIRMED by decompile; no entry of that kind: nothing), which
    /// is this entry.
    pub fn walls_damage_mut(&mut self) -> Option<&mut FortificationDamage> {
        self.fortifications.iter_mut().find(|e| e.kind == WALLS_DAMAGE_KIND)
    }

    /// A finished walls item's entries (`0x00B143E0`, tail-called by `StepSlotConstructionItem`
    /// `0x00B78800` at `0x00B788CF` after any finished build, repair or demolition in the walls slot;
    /// CONFIRMED by decompile): the exe appends four entries, kinds 0 to 3, each at full health (factor 1.0,
    /// no hits, no hit points; push_back `0x00B5E310`), every time, so its list grows without bound.
    /// Every reader of a settlement's list takes the first entry of a kind (`0x00B42AA0`, `0x00B69AF0`), so
    /// appending only the kinds the list lacks gives the same results with a bounded list (our design,
    /// not a fix: no behaviour changes); an entry already there, damaged or not, is kept.
    pub fn add_default_fortifications(&mut self) {
        for kind in 0..DEFAULT_FORTIFICATION_KINDS {
            if !self.fortifications.iter().any(|e| e.kind == kind) {
                self.fortifications.push(FortificationDamage { kind, ..FortificationDamage::default() });
            }
        }
    }
}

/// The kinds a finished walls item gives a settlement's list (`0x00B143E0` appends kinds 0 to 3).
pub const DEFAULT_FORTIFICATION_KINDS: u32 = 4;

/// The kind of the fortification entry the walls read (`0x00B42AA0` looks for +0x14 == 0).
pub const WALLS_DAMAGE_KIND: u32 = 0;
/// The most hits a siege's wall damage gives one entry (`0x008F301A`: no hit once +0x18 reaches 4).
pub const WALL_DAMAGE_MAX_HITS: u32 = 4;
/// What one hit of a siege's wall damage takes off the entry's factor (`0x008F3027`, the f32 0.1).
pub const WALL_DAMAGE_STEP: f32 = 0.1;

/// One `FORTIFICATION_DAMAGE_INFO` entry of a settlement's residence (28 bytes, CONFIRMED by the reader
/// `0x00514CC0` and its users): what the walls have suffered.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FortificationDamage {
    /// +0x04..+0x0C, the record's #0 `HIT_POINTS_BLOCK` (one f32 per item). No campaign code traced reads
    /// the values; the walls' restore zeroes them (`0x00576850`). Vanilla's are empty.
    pub hit_points: Vec<f32>,
    /// +0x10, #1: the walls' remaining strength, 0 to 1 (the walls' health is `factor · 100`).
    pub factor: f32,
    /// +0x14, #2: which part of the fortification the entry is for; the walls read kind 0.
    pub kind: u32,
    /// +0x18, #3: the siege hits taken (at most [`WALL_DAMAGE_MAX_HITS`] from the siege's wall damage).
    pub hits: u32,
}

impl Default for FortificationDamage {
    /// The entry's defaults (the reader `0x00514CC0` and every stack copy: no hit points, factor 1.0,
    /// kind 4, no hits).
    fn default() -> Self {
        FortificationDamage { hit_points: Vec::new(), factor: 1.0, kind: 4, hits: 0 }
    }
}

impl FortificationDamage {
    /// The walls' health this entry gives: `cvttss2si(factor · 100)` (`0x008F30F4`, `0x00B583D0`).
    pub fn walls_health(&self) -> u32 {
        // The factor is clamped to [0, 1] wherever it is written; a loaded one out of range gives the exe's
        // conversion, as an unsigned health.
        super::commands::cvttss2si(self.factor * 100.0) as u32
    }

    /// The walls' repair restores the entry (`0x00576850`, called by `StepRepairConstructionItem`
    /// `0x00B78490` when a walls repair finishes; CONFIRMED by decompile): every hit point 0 (the count
    /// kept), factor 1.0, no hits; the kind stays.
    pub fn restore(&mut self) {
        self.hit_points.iter_mut().for_each(|h| *h = 0.0);
        self.factor = 1.0;
        self.hits = 0;
    }
}

/// The exe's clamp of a fortification factor to [0, 1] after each change (`0x008F303F`, `0x00B5839D`): below
/// 0 gives 0, above 1 gives 1, in the exe's order (first `>= 0`, then `> 1`).
pub fn clamp_fortification_factor(factor: f32) -> f32 {
    if factor >= 0.0 {
        if factor > 1.0 { 1.0 } else { factor }
    } else {
        0.0
    }
}

/// The highest percent of its men a garrison unit can lose to one round of siege attrition
/// (`0x00B79732`).
pub const MAX_ATTRITION_PERCENT: u32 = 30;

/// The siege's attrition percent for a round (`0x00B796E3`–`0x00B79735`): with
/// `n = min(turns_besieged, surrender_turns)`, `trunc((n² − 1) · 1.5)` worked as the exe does (the
/// `u32` `n² − 1` wraps, goes to `f64` unsigned, to `f32`, times the `f32` 1.5 at `0x0131A7B8`, then
/// a truncating `f32` → `i32` conversion that gives `0x80000000` out of range), then 1 when the
/// result is below 1 as unsigned, else at most [`MAX_ATTRITION_PERCENT`]. So n = 1 gives 1, 2 gives
/// 4, 3 gives 12, 4 gives 22, 5 and more 30; n = 0 (a siege loaded with no surrender turns) wraps to
/// 30.
pub fn attrition_percent(turns_besieged: u32, surrender_turns: u32) -> u32 {
    let n = turns_besieged.min(surrender_turns);
    let square = n.wrapping_mul(n).wrapping_sub(1);
    let scaled = (square as f64) as f32 * 1.5f32;
    // CVTTSS2SI: truncation, the "integer indefinite" 0x80000000 when out of range.
    let truncated = if (-2_147_483_648.0..2_147_483_648.0).contains(&scaled) { scaled as i32 as u32 } else { 0x8000_0000 };
    if truncated < 1 {
        1
    } else {
        truncated.min(MAX_ATTRITION_PERCENT)
    }
}

/// The men a unit of `men` loses to `percent` (`0x00B5C0E0`): `men · percent / 100`, truncated, or
/// all of them from 100 up. The product is the exe's 32-bit `IMUL` (wrapping) and the division
/// unsigned (`MUL 0x51EB851F`, `SHR 5`); it wraps only above 143,165,576 men at 30 %, and the result
/// never exceeds `men` then either.
pub fn men_lost(men: u32, percent: u32) -> u32 {
    if percent < 100 {
        men.wrapping_mul(percent) / 100
    } else {
        men
    }
}

/// The turns a settlement's garrison holds out before it must surrender, set by its first besieger
/// (`0x00B57B10`, the settlement's virtual +0xB4, read by AddBesieger `0x00B112F0`; CONFIRMED, decompile):
/// 2 below 25,000 people, 3 below 100,000, 4 below 1,000,000, 5 below 10,000,000, else 6.
pub fn settlement_surrender_turns(population: u32) -> u32 {
    match population {
        0..25_000 => 2,
        25_000..100_000 => 3,
        100_000..1_000_000 => 4,
        1_000_000..10_000_000 => 5,
        _ => 6,
    }
}

/// Whether `region`'s settlement falls to the besieging army `force` at the start of its faction's turn
/// (`0x008F9160` on the army's siege object, CONFIRMED by decompile): when it has no garrison army (the
/// target's virtual +0x1C is null), or once the turns besieged reach the surrender turns. The original's
/// [`siege.fall`](super::seams::SIEGE_FALL) rule. The exe's step also counts the calls in the siege object
/// (+0x64), which nothing traced reads, so the model keeps no such counter.
pub fn siege_falls(model: &CampaignModel, _force: ForceId, region: RegionId) -> bool {
    model.world.regions.get(&region).is_some_and(|r| {
        !r.garrison.is_some_and(|g| model.world.forces.contains_key(&g)) || r.siege.turns_besieged >= r.siege.surrender_turns
    })
}

/// Whether the besieging army `force` can assault `region`'s settlement now (`0x008B26C0`, CONFIRMED by
/// decompile): the settlement has no walls (`GetSettlementFortificationLevel` `0x00B3F7C0` below 0), or the
/// army has a siege unit ([`CampaignModel::has_siege_units`]). The original's
/// [`siege.assault_allowed`](super::seams::SIEGE_ASSAULT_ALLOWED) rule.
pub fn siege_assault_allowed(model: &CampaignModel, force: ForceId, region: RegionId) -> bool {
    model.world.regions.get(&region).is_some_and(|r| r.fortification.is_none()) || model.has_siege_units(force)
}

/// The original's wall damage by the besieging army `force` of `region`'s settlement at its commander's turn
/// start (step 2 of `0x008F2F30`, after the siege turn's +0x84 is set; CONFIRMED by decompile), its
/// [`siege.wall_damage`](super::seams::SIEGE_WALL_DAMAGE) rule:
/// 1. the interval `L` is the walls' level + 1 (`GetSettlementFortificationLevel` `0x00B3F7C0`: the building in
///    the fortification slot, `building_levels` #2 + 1; −1 without walls). A fort's would be its +0x188 + 1; the
///    model has no forts under siege (see the module docs);
/// 2. when `L > 0` and the turns besieged (+0x10C) are a multiple of `L` (unsigned remainder, `0x008F2FAE`),
///    the walls' damage entry ([`SiegeState::walls_damage`], `0x00B42AA0`) with fewer than
///    [`WALL_DAMAGE_MAX_HITS`] hits takes one more and its factor drops by [`WALL_DAMAGE_STEP`], clamped to
///    [0, 1] ([`clamp_fortification_factor`]), written back (`0x00B69AF0`);
/// 3. the walls' health becomes the entry's [`FortificationDamage::walls_health`] (`SetBuildingHealth`
///    `0x00B452A0` at `0x008F30F4`, [`CampaignModel::set_building_health`]: `CampaignBuildingDamaged` below 100).
///
/// With no entry of kind 0 (at the start, every vanilla residence but egy_napoleon's three and
/// ita_napoleon's one; a finished walls item adds one, see [`SiegeState::fortifications`]) nothing happens.
/// A walls level key with no building record (a broken mod) does nothing, logged once per key (it would
/// repeat every besieged turn).
pub fn siege_wall_damage(model: &mut CampaignModel, _force: ForceId, region: RegionId) -> Vec<CampaignEvent> {
    static MISSING_WALLS_RECORD: crate::reported::ReportedKeys = crate::reported::ReportedKeys::new();
    let mut events = Vec::new();
    let Some(r) = model.world.regions.get(&region) else { return events };
    let Some(walls) = r.fortification.as_ref() else { return events };
    let Some(level) = model.rules.buildings.get(&walls.level_key).map(|b| b.level) else {
        if MISSING_WALLS_RECORD.first(&walls.level_key) {
            log::warn!("region {region:?}: walls {} have no building record; no siege wall damage (logged once per key)", walls.level_key);
        }
        return events;
    };
    let interval = level.wrapping_add(1);
    if interval <= 0 || r.siege.turns_besieged % interval as u32 != 0 {
        return events;
    }
    let Some(entry) = model.world.regions.get_mut(&region).and_then(|r| r.siege.walls_damage_mut()) else { return events };
    if entry.hits >= WALL_DAMAGE_MAX_HITS {
        return events;
    }
    entry.hits += 1;
    entry.factor = clamp_fortification_factor(entry.factor - WALL_DAMAGE_STEP);
    let health = entry.walls_health();
    model.set_building_health(region, super::SlotRef::Walls, health, &mut events);
    events
}

/// The original's round-start siege attrition for one region's settlement under siege
/// (`0x00B796B0`, see the module docs): the turns besieged go up, each garrison unit but the
/// commander's loses a drawn percent of its men, and the units left at or below minimum strength
/// go with their characters. Does nothing when the settlement is not under siege. The model calls
/// it through the [`siege.attrition`](super::seams::SIEGE_ATTRITION) seam.
pub fn siege_attrition(model: &mut CampaignModel, region: RegionId) -> Vec<CampaignEvent> {
    let mut events = Vec::new();
    let Some(r) = model.world.regions.get_mut(&region) else { return events };
    if !r.siege.under_siege() {
        return events;
    }
    r.siege.turns_besieged = r.siege.turns_besieged.wrapping_add(1);
    let percent = attrition_percent(r.siege.turns_besieged, r.siege.surrender_turns);
    let Some(garrison) = r.garrison else { return events };
    let Some(force) = model.world.forces.get(&garrison) else {
        log::warn!("region {region:?}: garrison force {garrison:?} is not in the world; no siege attrition");
        return events;
    };
    let commander = force.commander;
    let minimums: Vec<u32> = force.units.iter().map(|u| super::economy::unit_minimum_men(model, u.max_men)).collect();
    let mut gone = Vec::new();
    let force = model.world.forces.get_mut(&garrison).expect("checked above");
    for (i, u) in force.units.iter_mut().enumerate() {
        if commander.is_some() && u.character == commander {
            continue;
        }
        let drawn = model.rng.int_range(0, percent as i32) as u32;
        u.men -= men_lost(u.men, drawn);
        if u.men <= minimums[i] {
            gone.push(i);
        }
    }
    let mut fallen: Vec<CharacterId> = Vec::new();
    for &i in gone.iter().rev() {
        let u = force.units.remove(i);
        fallen.extend(u.character);
    }
    let emptied = force.units.is_empty();
    // The exe kills the characters in the units' order.
    for c in fallen.into_iter().rev() {
        model.character_falls(c);
    }
    if emptied {
        // Only a garrison with no commander can lose every unit (the commander's unit is never
        // drawn); the exe reads the commander unchecked (`0x00B79790`), so it never meets one. The
        // model keeps no empty force.
        log::warn!("region {region:?}: siege attrition removed every unit of garrison {garrison:?}; the force is destroyed");
        model.destroy_force(garrison, &mut events);
    }
    events
}

impl CampaignModel {
    /// `force` leaves the siege it holds, if any (`EndForceSiegeOfSettlement` `0x008BD550`, CONFIRMED,
    /// disassembly: the siege object at force +0x94 is destroyed, which unlinks the force from the
    /// settlement's besiegers; a force holds one siege at most). The settlement's turns stay, as in the
    /// residence. The one path every end of a siege takes (see the module docs). When a siege did end,
    /// the exe then runs, unless the campaign is being torn down (+0xFC5, which the model never is), the
    /// trade price update `0x00BCB020(0)` at `0x008BD65B` ([`Self::update_commodity_prices_with`] with
    /// flag 0: the prices, the commerce raid check and the trade income) and
    /// `RebuildRegionRecruitablesAndEffects` `0x00A6AE40` on the target (no port needed: the model derives
    /// a region's recruitables and building effects from its slots when read,
    /// [`Self::recruitable_units`], [`super::Region::effect_buildings`]). Its call `0x00B084F0` puts the
    /// residence character's path obstacle back (when `0x00B1D100` allows: he is the residence's own and it
    /// is no longer besieged), which the model's zones read live ([`Self::besieged_garrison`]); its event
    /// 0xDF on the notifier (model +0x9A8) has no listener (CONFIRMED, xrefs).
    pub(crate) fn end_force_siege(&mut self, force: ForceId) {
        let Some(r) = self.world.regions.values_mut().find(|r| r.siege.besiegers.contains(&force)) else { return };
        r.siege.besiegers.retain(|&b| b != force);
        // The siege object goes, and its +0x84 with it.
        self.world.siege_turn_started.remove(&force);
        self.update_commodity_prices_with(false);
    }

    /// The siege step of a besieging army's turn start (`0x008F2290`, the army's virtual +0x28, calls
    /// `0x008F2F30` when its siege object +0x94 is set; CONFIRMED, decompile), run at its commander's turn
    /// start ([`super::turn::TurnStep::CharactersStart`]):
    /// 1. the siege object's +0x84 is set ([`super::World::siege_turn_started`]): the army is asked at the
    ///    faction's turn-phase state 0 whether the settlement falls ([`Self::siege_falls_at_turn`]);
    /// 2. the wall damage, through the [`siege.wall_damage`](super::seams::SIEGE_WALL_DAMAGE) seam
    ///    ([`siege_wall_damage`] in the original): the walls' damage entry takes a hit every `L` turns
    ///    besieged, at most 4, and the walls' health follows it.
    ///
    /// The entries come from a loaded campaign's `FORTIFICATIONS_BLOCK` and from finished walls items
    /// ([`SiegeState::fortifications`]); the occupy, loot, post-battle and repair code only reads or overwrites
    /// them (CAMPAIGN_FIDELITY.md "Sieges"). The original has one more restore, for map forts: the map-fort
    /// repair (`0x00B4E4C0`, CCQ_REPAIR_FORT) queues an item that the faction's turn end (`0x008BD0F0` →
    /// `0x00B29940`) counts down, and on completion `0x00B01890` restores every entry of the fort
    /// (`0x00576850`). Not modelled: the model's map forts ([`super::world::Fort`]) carry no fortification
    /// entries and none is ever besieged (module docs).
    pub(crate) fn start_siege_turn(&mut self, army: ForceId, events: &mut Vec<CampaignEvent>) {
        let Some(region) = self.besieged_region(army) else { return };
        self.world.siege_turn_started.insert(army);
        let rules = std::sync::Arc::clone(&self.rules);
        let mut writes = ModWrites::default();
        events.extend(rules.seams.siege_wall_damage.rule()(self, army, region, &mut writes));
        self.mod_state.apply(&mut writes);
    }

    /// The settlement's owner-changed event (+0x24, fired by `0x00B449F0` at `0x00B44B31`) reaching
    /// each siege of `region`'s settlement (listener `0x008DB7D0`, CONFIRMED): a besieger of the new
    /// owner's faction, or not at war with it ([`Self::at_war`], `0x008CE9B0`), ends its siege
    /// ([`Self::end_force_siege`]); one at war with the new owner goes on besieging. A besieger id with
    /// no force in the world (the model never leaves one) is dropped with a warning.
    pub(crate) fn end_sieges_after_owner_change(&mut self, region: RegionId) {
        let Some(r) = self.world.regions.get(&region) else { return };
        let owner = r.owner;
        let ended: Vec<ForceId> = r
            .siege
            .besiegers
            .iter()
            .copied()
            .filter(|b| match self.world.forces.get(b) {
                Some(f) => f.faction == owner || !self.at_war(f.faction, owner),
                None => {
                    log::warn!("region {region:?}: besieger {b:?} is not in the world; its siege is dropped");
                    true
                }
            })
            .collect();
        for force in ended {
            self.end_force_siege(force);
        }
    }

    /// A peace's siege step for character `c` towards the other side `other` (`0x00B262C0` → `0x00A25570`,
    /// the call `0x00A255C5`, CONFIRMED): the army he commands (`0x009D3A40`, [`Self::force_of_kind`]: a navy
    /// is not looked at), if it besieges a settlement `other` owns (the target's virtual +0x14 equal to it,
    /// `0x00898B20`), ends its siege ([`Self::end_force_siege`]). The exe reaches a force only through its
    /// commander, so a besieger with none goes on besieging.
    pub(crate) fn end_siege_on_peace(&mut self, c: CharacterId, other: FactionId) {
        let Some(force) = self.force_of_kind(c, false) else { return };
        if self.world.regions.values().any(|r| r.owner == other && r.siege.besiegers.contains(&force)) {
            self.end_force_siege(force);
        }
    }

    /// The region whose settlement `force` besieges, if any (the force's siege object +0x94, whose target is
    /// at +0x7C; a force holds one siege at most).
    pub fn besieged_region(&self, force: ForceId) -> Option<RegionId> {
        self.world.regions.values().find(|r| r.siege.besiegers.contains(&force)).map(|r| r.id)
    }

    /// `force` lays siege to `region`'s settlement (LaySiege `0x008AF6A0`, CONFIRMED, disassembly; the exe
    /// calls it only for an army whose siege object +0x94 is null, at `0x0094B94B`):
    /// 1. the army's commander (army +0x74) is no longer hidden (character +0x4E0, `CHARACTER` #22, cleared
    ///    at `0x008AF6C2`);
    /// 2. the siege object is built (`0x0088A0D0` → AddBesieger `0x00B112F0`): the first besieger sets the
    ///    surrender turns ([`settlement_surrender_turns`] of the live population) and the turns besieged to 0,
    ///    and the force joins the end of the besieger list; the constructor also takes the residence
    ///    character's path obstacle off the map (`0x00B653A0`), which the zones read live
    ///    ([`Self::besieged_garrison`]);
    /// 3. the commander's offensive sieges go up by 1 (+0x4D8, `CHARACTER` #20, `0x008AF6FC`; the script's
    ///    `OffensiveSiegesFought`, `0x008A21F0`);
    /// 4. the residence character's defensive sieges go up by 1 (+0x4D0, `CHARACTER` #18, `0x008AF709`; the
    ///    script's `DefensiveSiegesFought`, `0x0089EC50`): the target's virtual +0x24 (`0x00B3F890` for a
    ///    settlement: the garrison army's commander, null without a garrison army). The exe increments it
    ///    unchecked; the model counts only a character it has. UNKNOWN whether the exe ever meets a null here
    ///    (a walk-in on a settlement with no garrison army lays siege, [`Self::walk_in_besieges`]);
    /// 5. with a commander, event 221 {commander} on the model's notifier +0x9A8 (`0x008AF73C`), which has no
    ///    listener (CONFIRMED, xrefs): not sent.
    ///
    /// The model's own guard: a force besieging another settlement leaves that siege first, so it holds one
    /// siege at most. The exe never needs it: its only caller lays siege when +0x94 is null, and the attack
    /// order's start has already ended any siege ([`Self::begin_order`]).
    pub(crate) fn start_siege(&mut self, force: ForceId, region: RegionId) {
        if self.besieged_region(force) == Some(region) {
            return;
        }
        self.end_force_siege(force);
        let Some(r) = self.world.regions.get_mut(&region) else {
            log::warn!("force {force:?}: no region {region:?} to besiege");
            return;
        };
        if r.siege.besiegers.is_empty() {
            r.siege.surrender_turns = settlement_surrender_turns(r.population);
            r.siege.turns_besieged = 0;
        }
        r.siege.besiegers.push(force);
        let garrison = r.garrison;
        let commander = self.world.forces.get(&force).and_then(|f| f.commander);
        // A character with no details record yet gets one, as the duel counters do: the exe's counters live
        // on the character itself.
        if let Some(c) = commander {
            let d = self.world.character_details.entry(c).or_default();
            d.hidden = false;
            d.offensive_sieges = d.offensive_sieges.wrapping_add(1);
        }
        if let Some(c) = garrison.and_then(|g| self.world.forces.get(&g)).and_then(|f| f.commander) {
            let d = self.world.character_details.entry(c).or_default();
            d.defensive_sieges = d.defensive_sieges.wrapping_add(1);
        }
    }

    /// Whether `force` has a unit that assaults walls (`ForceHasSiegeUnits` `0x008CE1B0`, CONFIRMED by
    /// decompile): a unit whose record's +0x1A0 flag is set ([`super::rules::UnitRules::siege_capable`],
    /// `unit_stats_land` #77) or whose category is artillery (code 1).
    pub fn has_siege_units(&self, force: ForceId) -> bool {
        self.world.forces.get(&force).is_some_and(|f| {
            f.units.iter().any(|u| {
                self.rules
                    .units
                    .get(&u.unit_key)
                    .is_some_and(|r| r.siege_capable || crate::unit_kind::category(&r.category) == crate::unit_kind::Category::Artillery)
            })
        })
    }

    /// Whether the besieging army `force` can assault `region`'s settlement now (`0x008B26C0`), through the
    /// [`siege.assault_allowed`](super::seams::SIEGE_ASSAULT_ALLOWED) seam ([`siege_assault_allowed`] in the
    /// original).
    pub fn can_assault_now(&self, force: ForceId, region: RegionId) -> bool {
        self.rules.seams.siege_assault_allowed.rule()(self, force, region)
    }

    /// `true` when `force` is the garrison of a settlement under siege: its commander is then no path
    /// obstacle (the siege object's constructor `0x0088A0D0` takes the residence character's obstacle off
    /// with `0x00B653A0`; `0x00B084F0` puts it back at the siege's end when `0x00B1D100` allows it).
    pub(crate) fn besieged_garrison(&self, force: ForceId) -> bool {
        self.world
            .forces
            .get(&force)
            .and_then(|f| f.commander)
            .and_then(|c| self.world.characters.get(&c)?.garrisoned_in)
            .and_then(|r| self.world.regions.get(&r))
            .is_some_and(|r| r.garrison == Some(force) && r.siege.under_siege())
    }

    /// The assault on `region`'s settlement by the army `force` (`0x008E1A90`, CONFIRMED by decompile: it
    /// builds the battle, `0x00883680`, with the type `0x008B5930` gives): a pending battle against the
    /// settlement's defenders ([`Self::defenders_of`]). INFERRED: with no defenders the exe's empty battle
    /// resumes into the capture with its report; the model takes the settlement at once
    /// ([`Self::settle_capture`]).
    pub(crate) fn assault(&mut self, force: ForceId, region: RegionId, events: &mut Vec<CampaignEvent>) {
        let defenders = self.defenders_of(region);
        if defenders.is_empty() {
            let Some(faction) = self.world.forces.get(&force).map(|f| f.faction) else { return };
            self.settle_capture(region, faction, Some(force), false, events);
        } else {
            self.pending_battle = Some(super::battles::PendingBattle { attacker: force, defenders, settlement: Some(region), resume: None, ambush: false });
            events.push(CampaignEvent::PreBattle { attacker: force });
        }
    }

    /// The assault command (handler `0x009365D0`, CONFIRMED by decompile): the siege the command names is
    /// assaulted when its army can assault now ([`Self::can_assault_now`], [`Self::assault`]). The exe does
    /// nothing otherwise; the model refuses the command with an error, as it does an army besieging nothing.
    /// The model's commands also check whose turn it is.
    pub(crate) fn assault_command(&mut self, force: ForceId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.commander_of(force)?;
        let region = self.besieged_region(force).ok_or(CommandError::Unsupported("the army besieges no settlement"))?;
        if !self.can_assault_now(force, region) {
            return Err(CommandError::Unsupported("the army cannot assault the walls: it has no siege unit"));
        }
        let mut events = Vec::new();
        self.assault(force, region, &mut events);
        Ok(events)
    }

    /// The end-siege command (handler `0x00932760`, CONFIRMED by decompile): the siege of the army `character`
    /// commands (`GetCharacterForceAsArmy` `0x009D3A40`, [`Self::force_of_kind`]) ends
    /// ([`Self::end_force_siege`]). The exe passes a character with no army to `0x008BD550` unchecked; the
    /// model refuses that command, and one for an army besieging nothing, with an error. The model's commands
    /// also check whose turn it is.
    pub(crate) fn end_siege_command(&mut self, character: CharacterId) -> Result<Vec<CampaignEvent>, CommandError> {
        let faction = self.world.characters.get(&character).ok_or(CommandError::UnknownCharacter(character))?.faction;
        self.check_turn(faction)?;
        let force = self.force_of_kind(character, false).ok_or(CommandError::Unsupported("the character commands no army"))?;
        if self.besieged_region(force).is_none() {
            return Err(CommandError::Unsupported("the army besieges no settlement"));
        }
        self.end_force_siege(force);
        Ok(Vec::new())
    }

    /// Every siege of `region`'s settlement ends, each besieger in list order (`0x00B29080`, CONFIRMED by
    /// decompile; reached only from a siege's fall).
    pub(crate) fn end_all_sieges(&mut self, region: RegionId) {
        // A copy, not `mem::take`: each end finds its siege by the besieger's place in this list
        // ([`Self::end_force_siege`], the one path every end takes) and removes it there.
        let besiegers = self.world.regions.get(&region).map(|r| r.siege.besiegers.clone()).unwrap_or_default();
        for b in besiegers {
            self.end_force_siege(b);
        }
    }

    /// The sieges `faction`'s armies hold, at the start of its turn (`0x008C2120`, called in turn-phase state 0
    /// of `0x0094A940` until it returns 0; CONFIRMED by decompile). Each call takes the first army of the
    /// faction's characters, in character-list order, whose siege object has +0x84 set (its siege turn started
    /// at its commander's turn start, [`Self::start_siege_turn`]) and asks whether the settlement falls
    /// (`0x008F9160`, which clears +0x84 either way; [`siege.fall`](super::seams::SIEGE_FALL), [`siege_falls`]),
    /// then returns 1; it returns 0 when no such army is left. So each flagged army is asked once, in list
    /// order, and a siege laid after its commander's turn start (by the reinforcement march, say) is not asked
    /// until the next turn. A fallen settlement's garrison army is destroyed and the army takes the settlement
    /// with the capture report (`0x00B58560`, settlement vtable +0xA0, [`Self::settle_capture`]; there the
    /// commander gets `0x00880950` and the move inside, `IssueMoveToResidenceOrder` `0x009549B0`), then every
    /// other siege of it ends ([`Self::end_all_sieges`]).
    ///
    /// The faction's flags no character reaches (a besieger whose commander left it since his turn start) stay
    /// set in the exe but are never read: the next ask comes after the next turn start, which sets them again
    /// for an army a character commands. The model drops them here.
    pub fn siege_falls_at_turn(&mut self, faction: FactionId, events: &mut Vec<CampaignEvent>) {
        let of_faction = |m: &CampaignModel, b: &ForceId| m.world.forces.get(b).is_none_or(|f| f.faction == faction);
        if !self.world.siege_turn_started.iter().any(|b| of_faction(self, b)) {
            return;
        }
        let rules = std::sync::Arc::clone(&self.rules);
        for c in self.world.faction_characters_in_list_order(faction) {
            let Some(force) = self.force_of_kind(c, false) else { continue };
            if !self.world.siege_turn_started.remove(&force) {
                continue;
            }
            // Only a flagged army gets here, so the regions are scanned once per siege, not per character; a
            // force → region index would be a second copy of the besieger lists.
            let Some(region) = self.besieged_region(force) else { continue };
            if !rules.seams.siege_fall.rule()(self, force, region) {
                continue;
            }
            if let Some(garrison) = self.world.regions.get(&region).and_then(|r| r.garrison) {
                self.destroy_force(garrison, events);
            }
            self.settle_capture(region, faction, Some(force), false, events);
            self.end_all_sieges(region);
        }
        let forces = &self.world.forces;
        self.world.siege_turn_started.retain(|b| forces.get(b).is_some_and(|f| f.faction != faction));
    }

    /// The round start's siege attrition (`0x00AAAF60` per region in the region manager's order,
    /// [`super::World::regions_in_manager_order`]): each region whose settlement is under siege runs the
    /// [`siege.attrition`](super::seams::SIEGE_ATTRITION) seam ([`siege_attrition`] in the original).
    pub fn siege_attrition_round_start(&mut self, events: &mut Vec<CampaignEvent>) {
        if !self.world.regions.values().any(|r| r.siege.under_siege()) {
            return;
        }
        let rules = std::sync::Arc::clone(&self.rules);
        // Each region's siege is tested when its turn comes, as `0x00AAAF60` does.
        for region in self.world.regions_in_manager_order() {
            if !self.world.regions.get(&region).is_some_and(|r| r.siege.under_siege()) {
                continue;
            }
            let mut writes = ModWrites::default();
            events.extend(rules.seams.siege_attrition.rule()(self, region, &mut writes));
            self.mod_state.apply(&mut writes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::tests::test_model;
    use crate::campaign::{CampaignUnit, TurnStep, UnitId};

    const BESIEGED: RegionId = RegionId(10); // test_model's region of faction A
    const GARRISON: ForceId = ForceId(1000); // A's army under general 100
    const BESIEGER: ForceId = ForceId(1001); // B's army

    fn unit(id: i32, men: u32, character: Option<CharacterId>) -> CampaignUnit {
        CampaignUnit { id: UnitId(id), unit_key: "test_unit".into(), men, max_men: 100, character, officer_name: Default::default() }
    }

    /// test_model with A's army 1000 garrisoned in region 10 (the general's own unit first, then two
    /// more, one of them led by character 102) and B's army 1001 besieging it, its siege turn started (its
    /// commander's turn start has run, `0x008F2F30`).
    fn besieged_model(surrender_turns: u32, turns_besieged: u32) -> CampaignModel {
        let mut m = test_model();
        let f = m.world.forces.get_mut(&GARRISON).unwrap();
        f.units = vec![unit(1, 60, Some(CharacterId(100))), unit(2, 80, None), unit(3, 90, Some(CharacterId(102)))];
        let r = m.world.regions.get_mut(&BESIEGED).unwrap();
        r.garrison = Some(GARRISON);
        r.siege = SiegeState { besiegers: vec![BESIEGER], surrender_turns, turns_besieged, ..Default::default() };
        m.world.siege_turn_started.insert(BESIEGER);
        m
    }

    /// The (defensive, offensive) sieges of character `c`.
    fn sieges_fought(m: &CampaignModel, c: i32) -> (u32, u32) {
        let d = &m.world.character_details[&CharacterId(c)];
        (d.defensive_sieges, d.offensive_sieges)
    }

    fn men(m: &CampaignModel) -> Vec<(i32, u32)> {
        m.world.forces[&GARRISON].units.iter().map(|u| (u.id.0, u.men)).collect()
    }

    /// `0x00B796E3`–`0x00B79735`: trunc((n² − 1) · 1.5), at least 1, at most 30, n = min(turns, cap).
    #[test]
    fn the_attrition_percent_grows_with_the_turns_up_to_the_surrender_turns() {
        let table: Vec<u32> = (1..=7).map(|n| attrition_percent(n, 10)).collect();
        assert_eq!(table, [1, 4, 12, 22, 30, 30, 30]);
        assert_eq!(attrition_percent(9, 3), 12, "n is capped at the surrender turns");
        assert_eq!(attrition_percent(0, 4), 30, "n = 0: n² − 1 wraps to u32::MAX, out of i32 range");
        assert_eq!(attrition_percent(5, 0), 30, "a siege loaded with no surrender turns");
        assert_eq!(attrition_percent(70_000, 70_000), 30, "n² wraps");
    }

    /// `0x00B5C0E0`: a percent of the unit's men, truncated; all of them from 100.
    #[test]
    fn a_unit_loses_a_truncated_percent_of_its_men() {
        assert_eq!(men_lost(80, 12), 9);
        assert_eq!(men_lost(99, 1), 0);
        assert_eq!(men_lost(80, 0), 0);
        assert_eq!(men_lost(80, 100), 80);
        assert_eq!(men_lost(u32::MAX, 30), u32::MAX.wrapping_mul(30) / 100, "the 32-bit IMUL wraps");
        assert_eq!(men_lost(143_165_577, 30), 0, "the product wraps to 14");
        assert_eq!(men_lost(143_165_576, 30), 42_949_672, "the largest unit that does not wrap");
    }

    /// One draw per unit but the commander's (no draw), in unit order, of `int_range(0, p)` on the
    /// campaign RNG; the counter goes up first.
    #[test]
    fn each_garrison_unit_but_the_commanders_draws_its_loss_in_order() {
        let mut m = besieged_model(4, 2);
        let mut rng = m.rng;
        let percent = attrition_percent(3, 4);
        assert_eq!(percent, 12);
        let (r2, r3) = (rng.int_range(0, 12) as u32, rng.int_range(0, 12) as u32);
        let events = siege_attrition(&mut m, BESIEGED);
        assert!(events.is_empty());
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 3);
        assert_eq!(men(&m), [(1, 60), (2, 80 - men_lost(80, r2)), (3, 90 - men_lost(90, r3))]);
        assert_eq!(m.rng, rng, "two draws, none for the commander's unit");
        assert_eq!(m.world.regions[&BESIEGED].siege.besiegers, [BESIEGER], "the siege goes on");
    }

    /// `0x00B797F9`–`0x00B79925`: a unit left at or below its minimum strength (4 men here) goes after
    /// the loop, and its character dies with it.
    #[test]
    fn a_unit_at_minimum_strength_goes_with_its_character() {
        let mut m = besieged_model(6, 10);
        {
            let f = m.world.forces.get_mut(&GARRISON).unwrap();
            f.units[2].men = 4; // already at the minimum: any draw removes it
        }
        let min = crate::campaign::economy::unit_minimum_men(&m, 100);
        assert_eq!(min, 5, "test_rules: ceil(0.05 · 100)");
        let mut rng = m.rng;
        let r2 = rng.int_range(0, 30) as u32;
        let _ = rng.int_range(0, 30);
        siege_attrition(&mut m, BESIEGED);
        assert_eq!(men(&m), [(1, 60), (2, 80 - men_lost(80, r2))]);
        assert!(!m.world.characters.contains_key(&CharacterId(102)), "the unit's character dies");
        assert!(m.world.characters.contains_key(&CharacterId(100)), "the general stays");
        assert_eq!(m.rng, rng, "the removed unit drew too");
    }

    /// The counter goes up with no garrison (`0x00B796D2` before the garrison test at `0x00B796DD`), and no
    /// draw is made.
    #[test]
    fn the_turns_besieged_go_up_without_a_garrison() {
        let mut m = besieged_model(3, 1);
        m.world.regions.get_mut(&BESIEGED).unwrap().garrison = None;
        let rng = m.rng;
        siege_attrition(&mut m, BESIEGED);
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 2);
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_until_surrender(), 1);
        assert_eq!(m.rng, rng);
    }

    #[test]
    fn a_region_not_under_siege_is_left_alone() {
        let mut m = besieged_model(3, 1);
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.clear();
        let before = m.clone();
        let mut events = Vec::new();
        m.siege_attrition_round_start(&mut events);
        assert!(events.is_empty());
        assert_eq!(m.state_hash(), before.state_hash());
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 1);
    }

    /// A destroyed besieger leaves the siege (`0x0088ECC0` unlinks force +0x7C); the turns stay.
    #[test]
    fn a_destroyed_besieger_ends_the_siege() {
        let mut m = besieged_model(4, 2);
        let mut events = Vec::new();
        m.destroy_force(BESIEGER, &mut events);
        let siege = &m.world.regions[&BESIEGED].siege;
        assert!(!siege.under_siege());
        assert_eq!((siege.surrender_turns, siege.turns_besieged), (4, 2));
        let before = men(&m);
        m.siege_attrition_round_start(&mut events);
        assert_eq!(men(&m), before);
    }

    /// The round-start step runs the attrition (`0x0096C0F2`, after the computed factors, before the
    /// trade update) with the campaign RNG as it stands there.
    #[test]
    fn the_round_start_runs_the_siege_attrition() {
        let mut m = besieged_model(4, 2);
        let mut direct = m.clone();
        siege_attrition(&mut direct, BESIEGED);
        m.turn.humans = vec![FactionId(1)];
        m.turn.queue.push_back(TurnStep::RoundStart);
        m.step().expect("the round-start step");
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 3);
        assert_eq!(men(&m), men(&direct));
    }

    /// The regions run in the region manager's order (`World::region_order`), not id order.
    #[test]
    fn the_regions_run_in_the_region_managers_order() {
        let mut m = besieged_model(5, 4);
        // A second besieged settlement: region 12 (A's), garrisoned by a new army of A under general 100's
        // colleague 102 (his unit leaves army 1000).
        m.world.forces.get_mut(&GARRISON).unwrap().units.truncate(2);
        let other = ForceId(1005);
        m.world.forces.insert(
            other,
            crate::campaign::MilitaryForce { id: other, faction: FactionId(1), commander: Some(CharacterId(102)), units: vec![unit(5, 70, Some(CharacterId(102))), unit(6, 90, None), unit(7, 50, None)], is_navy: false },
        );
        let r = m.world.regions.get_mut(&RegionId(12)).unwrap();
        r.garrison = Some(other);
        r.siege = SiegeState { besiegers: vec![BESIEGER], surrender_turns: 5, turns_besieged: 1, ..Default::default() };
        m.world.region_order = vec![RegionId(12), RegionId(11), RegionId(10)];
        let run = |m: &CampaignModel, order: [RegionId; 2]| {
            let mut m = m.clone();
            for r in order {
                siege_attrition(&mut m, r);
            }
            m
        };
        let file_order = run(&m, [RegionId(12), BESIEGED]);
        let id_order = run(&m, [BESIEGED, RegionId(12)]);
        let mut events = Vec::new();
        m.siege_attrition_round_start(&mut events);
        let all = |m: &CampaignModel| m.world.forces.values().flat_map(|f| f.units.iter().map(|u| u.men)).collect::<Vec<_>>();
        assert_eq!(all(&m), all(&file_order));
        assert_ne!(all(&m), all(&id_order), "the orders draw differently here");
    }

    /// A third army, of faction 3 and with no commander.
    fn third_army(m: &mut CampaignModel) -> ForceId {
        let third = ForceId(1010);
        m.world.forces.insert(
            third,
            crate::campaign::MilitaryForce { id: third, faction: FactionId(3), commander: None, units: vec![unit(8, 50, None)], is_navy: false },
        );
        third
    }

    /// A third army of faction 3 joins the siege of region 10.
    fn with_third_besieger(m: &mut CampaignModel) -> ForceId {
        let third = third_army(m);
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.push(third);
        third
    }

    /// The captor's capture ends its siege (`0x00B586E9`, and the owner-changed listener `0x008DB7D0`
    /// for the new owner's own force); a besieger at war with the new owner goes on (`0x008CE9B0`).
    #[test]
    fn a_capture_ends_the_captors_siege_and_a_besieger_at_war_with_the_captor_stays() {
        let mut m = besieged_model(4, 2);
        let third = with_third_besieger(&mut m);
        m.world.set_stance(FactionId(1), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.world.set_stance(FactionId(3), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.capture_by_force(BESIEGED, FactionId(2), Some(BESIEGER));
        let siege = &m.world.regions[&BESIEGED].siege;
        assert_eq!(m.world.regions[&BESIEGED].owner, FactionId(2));
        assert_eq!(siege.besiegers, [third]);
        assert_eq!((siege.surrender_turns, siege.turns_besieged), (4, 2), "the turns stay");
    }

    /// The owner-changed listener `0x008DB7D0` ends the siege of a besieger not at war with the new owner.
    #[test]
    fn an_owner_change_ends_the_siege_of_a_besieger_at_peace_with_the_new_owner() {
        let mut m = besieged_model(4, 2);
        with_third_besieger(&mut m);
        m.world.set_stance(FactionId(1), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.capture_by_force(BESIEGED, FactionId(2), Some(BESIEGER));
        assert!(!m.world.regions[&BESIEGED].siege.under_siege());
    }

    /// Peace between the owner and a besieger's faction ends that siege (`0x00A25570`, the call
    /// `0x00A255C5`); a besieger of a third faction goes on, and so does one with no commander or a commander
    /// in no character list (the exe walks each side's character list).
    #[test]
    fn peace_ends_the_siege_of_the_other_sides_besieger() {
        let mut m = besieged_model(4, 2);
        let third = with_third_besieger(&mut m);
        m.world.set_stance(FactionId(1), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.world.set_stance(FactionId(1), FactionId(3), crate::campaign::Stance::War).unwrap();
        m.make_peace_rules(FactionId(1), FactionId(2), true).expect("at war");
        assert_eq!(m.world.regions[&BESIEGED].siege.besiegers, [third]);
        let mut no_commander = m.clone();
        no_commander.make_peace_rules(FactionId(3), FactionId(1), true).expect("at war");
        assert_eq!(no_commander.world.regions[&BESIEGED].siege.besiegers, [third], "no commander: not reached");
        m.world.forces.get_mut(&third).unwrap().commander = Some(CharacterId(300));
        let mut unlisted = m.clone();
        unlisted.make_peace_rules(FactionId(3), FactionId(1), true).expect("at war");
        assert_eq!(unlisted.world.regions[&BESIEGED].siege.besiegers, [third], "a commander in no character list: not reached");
        let general = crate::campaign::Character {
            id: CharacterId(300),
            faction: FactionId(3),
            kind: crate::campaign::CharacterKind::General,
            position: Default::default(),
            movement_points: 0,
            max_movement_points: 0,
            base_movement_points: 0,
            garrisoned_in: None,
        };
        m.world.characters.insert(general.id, general);
        m.make_peace_rules(FactionId(3), FactionId(1), true).expect("at war");
        assert!(!m.world.regions[&BESIEGED].siege.under_siege(), "either side may propose");
    }

    /// The state hash covers the siege.
    #[test]
    fn the_state_hash_covers_the_siege() {
        let m = besieged_model(4, 2);
        let mut turns = m.clone();
        turns.world.regions.get_mut(&BESIEGED).unwrap().siege.turns_besieged = 3;
        let mut surrender = m.clone();
        surrender.world.regions.get_mut(&BESIEGED).unwrap().siege.surrender_turns = 5;
        let mut lifted = m.clone();
        lifted.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.clear();
        // `besieged_model` starts with the besieger's siege turn started, so the other case is the cleared flag.
        assert!(m.world.siege_turn_started.contains(&BESIEGER));
        let mut started = m.clone();
        started.world.siege_turn_started.clear();
        for other in [&turns, &surrender, &lifted, &started] {
            assert_ne!(m.state_hash(), other.state_hash());
        }
    }

    /// `0x00B57B10`: 2 turns below 25,000 people, one more at each step up to 6.
    #[test]
    fn the_surrender_turns_follow_the_population() {
        let cases = [(0, 2), (24_999, 2), (25_000, 3), (99_999, 3), (100_000, 4), (999_999, 4), (1_000_000, 5), (9_999_999, 5), (10_000_000, 6), (u32::MAX, 6)];
        for (population, turns) in cases {
            assert_eq!(settlement_surrender_turns(population), turns, "{population}");
        }
    }

    /// AddBesieger `0x00B112F0`: the first besieger sets the surrender turns from the live population and the
    /// turns besieged to 0; a later one joins the end of the list and sets neither. An army besieging elsewhere
    /// leaves that siege first; one already besieging the settlement changes nothing.
    #[test]
    fn the_first_besieger_sets_the_surrender_turns() {
        let mut m = besieged_model(6, 3);
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.clear();
        m.world.regions.get_mut(&BESIEGED).unwrap().population = 20_000;
        m.start_siege(BESIEGER, BESIEGED);
        assert_eq!(m.world.regions[&BESIEGED].siege, SiegeState { besiegers: vec![BESIEGER], surrender_turns: 2, turns_besieged: 0, ..Default::default() });
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.turns_besieged = 1;
        let third = third_army(&mut m);
        m.start_siege(third, BESIEGED);
        m.start_siege(BESIEGER, BESIEGED);
        assert_eq!(m.world.regions[&BESIEGED].siege, SiegeState { besiegers: vec![BESIEGER, third], surrender_turns: 2, turns_besieged: 1, ..Default::default() });
        assert_eq!(m.besieged_region(third), Some(BESIEGED));
        m.start_siege(third, RegionId(12));
        assert_eq!(m.world.regions[&BESIEGED].siege.besiegers, [BESIEGER]);
        assert_eq!(m.world.regions[&RegionId(12)].siege, SiegeState { besiegers: vec![third], surrender_turns: 4, turns_besieged: 0, ..Default::default() });
    }

    /// LaySiege `0x008AF6A0`: the besieging general is no longer hidden (`0x008AF6C2`), his offensive sieges
    /// go up (+0x4D8, `0x008AF6FC`) and so do the defensive sieges of the garrison army's commander (+0x4D0
    /// through the target's virtual +0x24 `0x00B3F890`, `0x008AF709`). With no garrison army only the
    /// besieger counts; the same army laying the same siege again counts nothing.
    #[test]
    fn laying_siege_unhides_the_general_and_counts_both_sides_sieges() {
        let mut m = besieged_model(6, 3);
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.clear();
        for c in [100, 101] {
            m.world.character_details.insert(CharacterId(c), Default::default());
        }
        m.world.character_details.get_mut(&CharacterId(101)).unwrap().hidden = true;
        m.start_siege(BESIEGER, BESIEGED);
        assert!(!m.world.character_details[&CharacterId(101)].hidden, "0x008AF6C2");
        assert_eq!((sieges_fought(&m, 101), sieges_fought(&m, 100)), ((0, 1), (1, 0)));
        m.start_siege(BESIEGER, BESIEGED);
        assert_eq!((sieges_fought(&m, 101), sieges_fought(&m, 100)), ((0, 1), (1, 0)), "already besieging it");
        m.end_force_siege(BESIEGER);
        m.world.regions.get_mut(&BESIEGED).unwrap().garrison = None;
        m.start_siege(BESIEGER, BESIEGED);
        assert_eq!((sieges_fought(&m, 101), sieges_fought(&m, 100)), ((0, 2), (1, 0)), "no garrison army");
    }

    /// `0x008C2120` asks only an army whose siege object has +0x84 set (`0x008F2F30`, at its commander's turn
    /// start, `0x00A24EB0` → `0x008F2290`), and the ask clears it whatever the answer (`0x008F9160`). So a
    /// siege laid after the commander's turn start is not asked at that turn's state 0, only at the next.
    #[test]
    fn only_a_siege_whose_turn_started_is_asked_whether_it_falls() {
        let mut m = besieged_model(4, 4);
        m.world.siege_turn_started.clear();
        m.turn.queue.push_back(TurnStep::Decide(FactionId(2)));
        m.step().expect("the decide step");
        assert_eq!(m.world.regions[&BESIEGED].owner, FactionId(1), "laid after the turn start: not asked");
        m.turn.queue.clear();
        m.turn.queue.push_back(TurnStep::CharactersStart(FactionId(2)));
        m.step().expect("the characters' turn start");
        assert!(m.world.siege_turn_started.contains(&BESIEGER), "0x008F2F30 sets +0x84");
        let mut holds = m.clone();
        holds.world.regions.get_mut(&BESIEGED).unwrap().siege.turns_besieged = 3;
        m.turn.queue.clear();
        m.turn.queue.push_back(TurnStep::Decide(FactionId(2)));
        m.step().expect("the decide step");
        assert_eq!(m.world.regions[&BESIEGED].owner, FactionId(2), "asked once its turn started");
        assert!(m.world.siege_turn_started.is_empty());
        let mut events = Vec::new();
        holds.siege_falls_at_turn(FactionId(2), &mut events);
        assert_eq!(holds.world.regions[&BESIEGED].owner, FactionId(1));
        assert!(holds.world.siege_turn_started.is_empty(), "0x008F9160 clears +0x84 when it holds too");
        // A siege's end drops the flag with the siege object; an army besieging nothing gets none.
        let mut ended = besieged_model(4, 4);
        ended.end_force_siege(BESIEGER);
        assert!(ended.world.siege_turn_started.is_empty());
        ended.start_siege_turn(BESIEGER, &mut Vec::new());
        assert!(ended.world.siege_turn_started.is_empty(), "no siege object");
    }

    /// `ForceHasSiegeUnits` `0x008CE1B0` and `0x008B26C0`: a unit flagged at record +0x1A0 or of the artillery
    /// category takes an army over the walls; with no walls any army can assault.
    #[test]
    fn siege_units_are_the_flagged_ones_and_the_artillery() {
        let mut m = test_model();
        let mut rules = (*m.rules).clone();
        let base = rules.units["test_unit"].clone();
        rules.units.insert("test_ladders".into(), super::super::UnitRules { siege_capable: true, ..base.clone() });
        rules.units.insert("test_guns".into(), super::super::UnitRules { category: "artillery".into(), ..base });
        m.rules = std::sync::Arc::new(rules);
        assert!(!m.has_siege_units(BESIEGER));
        assert!(m.can_assault_now(BESIEGER, BESIEGED), "no walls");
        m.world.regions.get_mut(&BESIEGED).unwrap().fortification = Some(super::super::BuildingRef { level_key: "test_building_level".into(), health: 100 });
        assert!(!m.can_assault_now(BESIEGER, BESIEGED), "walls, no siege unit");
        for key in ["test_ladders", "test_guns"] {
            let mut x = m.clone();
            x.world.forces.get_mut(&BESIEGER).unwrap().units[1].unit_key = key.into();
            assert!(x.has_siege_units(BESIEGER) && x.can_assault_now(BESIEGER, BESIEGED), "{key}");
        }
    }

    /// `0x008F9160`: the settlement falls with no garrison army or once the turns besieged reach the surrender
    /// turns.
    #[test]
    fn a_settlement_falls_without_a_garrison_or_when_its_turns_run_out() {
        let m = besieged_model(4, 3);
        assert!(!siege_falls(&m, BESIEGER, BESIEGED));
        let mut x = besieged_model(4, 4);
        assert!(siege_falls(&x, BESIEGER, BESIEGED));
        x = besieged_model(4, 0);
        x.world.regions.get_mut(&BESIEGED).unwrap().garrison = None;
        assert!(siege_falls(&x, BESIEGER, BESIEGED), "no garrison");
        x = besieged_model(4, 0);
        x.world.forces.remove(&GARRISON);
        assert!(siege_falls(&x, BESIEGER, BESIEGED), "a garrison not in the world");
    }

    /// `0x008C2120` at the besieger's turn start: the garrison army is destroyed, the besieger takes the
    /// settlement and every siege of it ends, one of a faction at war with the captor too (`0x00B29080`).
    #[test]
    fn a_fallen_settlement_is_taken_and_its_sieges_end() {
        let mut m = besieged_model(4, 4);
        let third = with_third_besieger(&mut m);
        m.world.set_stance(FactionId(3), FactionId(2), crate::campaign::Stance::War).unwrap();
        let mut events = Vec::new();
        m.siege_falls_at_turn(FactionId(1), &mut events);
        assert_eq!(m.world.regions[&BESIEGED].owner, FactionId(1), "only the besieger's faction runs its falls");
        m.siege_falls_at_turn(FactionId(2), &mut events);
        let r = &m.world.regions[&BESIEGED];
        assert_eq!(r.owner, FactionId(2));
        assert!(!m.world.forces.contains_key(&GARRISON), "the garrison is destroyed");
        assert!(!r.siege.under_siege(), "{:?}", r.siege.besiegers);
        assert_eq!(m.besieged_region(third), None);
    }

    /// The settlement holds while the turns besieged are below the surrender turns.
    #[test]
    fn a_settlement_holds_until_its_turns_run_out() {
        let mut m = besieged_model(4, 3);
        // Nothing changes but the siege turn's flag, which the decide step clears when the settlement holds
        // too (`0x008F9160`).
        let mut before = m.clone();
        before.world.siege_turn_started.clear();
        let mut events = Vec::new();
        m.siege_falls_at_turn(FactionId(2), &mut events);
        assert!(events.is_empty());
        assert_eq!(m.state_hash(), before.state_hash());
    }

    /// The faction's decide step runs its falls first (turn-phase state 0 of `0x0094A940`).
    #[test]
    fn the_decide_step_runs_the_siege_falls() {
        let mut m = besieged_model(4, 4);
        m.turn.queue.push_back(TurnStep::Decide(FactionId(2)));
        m.step().expect("the decide step");
        assert_eq!(m.world.regions[&BESIEGED].owner, FactionId(2));
    }

    /// A mod replaces the fall rule through the [`siege.fall`](super::super::seams::SIEGE_FALL) seam.
    #[test]
    fn the_fall_rule_is_a_seam() {
        use super::super::seams::SiegeFall;
        use std::sync::Arc;
        let with = |falls: bool| {
            let mut m = besieged_model(4, if falls { 0 } else { 4 });
            let mut rules = (*m.rules).clone();
            let maker: crate::seam::Maker<SiegeFall> = Arc::new(move |_: Arc<SiegeFall>| -> Arc<SiegeFall> { Arc::new(move |_: &CampaignModel, _: ForceId, _: RegionId| falls) });
            rules.seams.siege_fall.apply("test_fall", &maker);
            m.rules = Arc::new(rules);
            let mut events = Vec::new();
            m.siege_falls_at_turn(FactionId(2), &mut events);
            m.world.regions[&BESIEGED].owner
        };
        assert_eq!(with(true), FactionId(2), "falls at once");
        assert_eq!(with(false), FactionId(1), "never falls");
    }

    /// A mod replaces the assault test through the
    /// [`siege.assault_allowed`](super::super::seams::SIEGE_ASSAULT_ALLOWED) seam: the model's check (the
    /// attack's arrival and the assault command) answers with it.
    #[test]
    fn the_assault_test_is_a_seam() {
        use super::super::seams::SiegeAssaultAllowed;
        use std::sync::Arc;
        let with = |walls: bool, allowed: bool| {
            let mut m = besieged_model(4, 0);
            if walls {
                m.world.regions.get_mut(&BESIEGED).unwrap().fortification = Some(super::super::BuildingRef { level_key: "test_building_level".into(), health: 100 });
            }
            let original = m.can_assault_now(BESIEGER, BESIEGED);
            assert_eq!(original, siege_assault_allowed(&m, BESIEGER, BESIEGED));
            let maker: crate::seam::Maker<SiegeAssaultAllowed> =
                Arc::new(move |_| Arc::new(move |_: &CampaignModel, _: ForceId, _: RegionId| allowed));
            Arc::make_mut(&mut m.rules).seams.siege_assault_allowed.apply("test_assault", &maker);
            (original, m.can_assault_now(BESIEGER, BESIEGED))
        };
        assert_eq!(with(true, true), (false, true), "walls and no siege unit, the mod allows");
        assert_eq!(with(false, false), (true, false), "no walls, the mod forbids");
    }

    /// The garrison of a besieged settlement is no path obstacle (`0x0088A0D0` → `0x00B653A0`) until the siege
    /// ends (`0x00B084F0`).
    #[test]
    fn a_besieged_garrison_is_no_obstacle() {
        let mut m = besieged_model(4, 0);
        m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(BESIEGED);
        assert!(m.besieged_garrison(GARRISON));
        assert!(!m.besieged_garrison(BESIEGER), "the besieger");
        m.end_force_siege(BESIEGER);
        assert!(!m.besieged_garrison(GARRISON), "the siege ended");
    }

    /// [`besieged_model`] with walls of `level_key` and the given fortification damage entries.
    fn walled_model(level_key: &str, fortifications: Vec<FortificationDamage>) -> CampaignModel {
        let mut m = besieged_model(4, 0);
        let r = m.world.regions.get_mut(&BESIEGED).unwrap();
        r.fortification = Some(super::super::BuildingRef { level_key: level_key.into(), health: 100 });
        r.siege.fortifications = fortifications;
        m
    }

    fn entry(kind: u32, factor: f32) -> FortificationDamage {
        FortificationDamage { hit_points: vec![0.0; 3], factor, kind, ..Default::default() }
    }

    fn damaged() -> CampaignEvent {
        CampaignEvent::CampaignBuildingDamaged { region: BESIEGED, slot: super::super::SlotRef::Walls }
    }

    /// The walls' (hits, factor, health) after one turn start of the besieger at `turns` besieged.
    fn wall_turn(m: &mut CampaignModel, turns: u32) -> (Vec<CampaignEvent>, u32, f32, u32) {
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.turns_besieged = turns;
        let mut events = Vec::new();
        m.start_siege_turn(BESIEGER, &mut events);
        let r = &m.world.regions[&BESIEGED];
        let e = r.siege.walls_damage().unwrap();
        (events, e.hits, e.factor, r.fortification.as_ref().unwrap().health)
    }

    /// Step 2 of `0x008F2F30`: the walls take a hit when the turns besieged are a multiple of the walls' level
    /// + 1 (`0x00B3F7C0`, `0x008F2FAE`); a level-1 wall every second turn, turn 0 included.
    #[test]
    fn the_walls_take_a_hit_every_level_plus_one_turns() {
        let mut m = walled_model("test_building_level_2", vec![entry(0, 1.0)]);
        let hits: Vec<u32> = (0..6).map(|t| wall_turn(&mut m, t).1).collect();
        assert_eq!(hits, [1, 1, 2, 2, 3, 3]);
        // A level-0 wall every turn.
        let mut m = walled_model("test_building_level", vec![entry(0, 1.0)]);
        let hits: Vec<u32> = (0..3).map(|t| wall_turn(&mut m, t).1).collect();
        assert_eq!(hits, [1, 2, 3]);
    }

    /// Each hit takes 0.1 off the factor (`0x008F3027`) and sets the walls' health to trunc(factor × 100)
    /// (`0x008F30F4` → `SetBuildingHealth` `0x00B452A0`, which posts `CampaignBuildingDamaged`); after 4 hits
    /// (`0x008F301A`) the walls take no more.
    #[test]
    fn the_walls_take_at_most_four_hits() {
        let mut m = walled_model("test_building_level", vec![entry(0, 1.0)]);
        let mut factor = 1.0f32;
        for turn in 0..4 {
            factor -= WALL_DAMAGE_STEP;
            let (events, hits, f, health) = wall_turn(&mut m, turn);
            assert_eq!((hits, f), (turn + 1, factor), "turn {turn}");
            assert_eq!(health, crate::campaign::commands::cvttss2si(factor * 100.0) as u32, "turn {turn}");
            assert_eq!(events, [damaged()], "turn {turn}");
        }
        let health = m.world.regions[&BESIEGED].fortification.as_ref().unwrap().health;
        assert!((59..=60).contains(&health), "{health}");
        for turn in 4..7 {
            let (events, hits, f, h) = wall_turn(&mut m, turn);
            assert_eq!((events.len(), hits, f, h), (0, 4, factor, health), "turn {turn}");
        }
    }

    /// The factor is clamped to [0, 1] (`0x00B69AF0`'s caller at `0x008F3030`): 0.05 − 0.1 gives 0, health 0.
    #[test]
    fn the_wall_factor_is_clamped_at_zero() {
        let mut m = walled_model("test_building_level", vec![entry(0, 0.05)]);
        let (events, hits, factor, health) = wall_turn(&mut m, 0);
        assert_eq!((hits, factor, health), (1, 0.0, 0));
        assert_eq!(events, [damaged()]);
        assert_eq!(clamp_fortification_factor(1.5), 1.0);
        assert_eq!(clamp_fortification_factor(-0.5), 0.0);
        assert_eq!(clamp_fortification_factor(0.25), 0.25);
    }

    /// No walls entry (kind 0, `0x00B42AA0`), or no walls, and nothing happens; the siege turn still starts.
    #[test]
    fn without_a_walls_entry_or_walls_nothing_is_damaged() {
        let mut m = walled_model("test_building_level", vec![entry(1, 1.0), entry(2, 1.0)]);
        m.world.siege_turn_started.clear();
        let mut events = Vec::new();
        m.start_siege_turn(BESIEGER, &mut events);
        assert!(events.is_empty(), "{events:?}");
        assert!(m.world.siege_turn_started.contains(&BESIEGER));
        let r = &m.world.regions[&BESIEGED];
        assert_eq!(r.fortification.as_ref().unwrap().health, 100);
        assert!(r.siege.fortifications.iter().all(|e| e.hits == 0 && e.factor == 1.0));

        let mut m = walled_model("test_building_level", vec![entry(0, 1.0)]);
        m.world.regions.get_mut(&BESIEGED).unwrap().fortification = None;
        let mut events = Vec::new();
        m.start_siege_turn(BESIEGER, &mut events);
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(m.world.regions[&BESIEGED].siege.walls_damage().unwrap().hits, 0);
    }

    /// The first entry of kind 0 is the walls' (`0x00B42AA0` walks the block in order).
    #[test]
    fn the_first_walls_entry_takes_the_hit() {
        let mut m = walled_model("test_building_level", vec![entry(1, 1.0), entry(0, 1.0), entry(0, 1.0)]);
        wall_turn(&mut m, 0);
        let hits: Vec<u32> = m.world.regions[&BESIEGED].siege.fortifications.iter().map(|e| e.hits).collect();
        assert_eq!(hits, [0, 1, 0]);
    }

    /// A mod replaces the wall damage through the [`siege.wall_damage`](super::super::seams::SIEGE_WALL_DAMAGE)
    /// seam.
    #[test]
    fn the_wall_damage_is_a_seam() {
        use super::super::seams::SiegeWallDamage;
        use std::sync::Arc;
        let mut m = walled_model("test_building_level", vec![entry(0, 1.0)]);
        let mut rules = (*m.rules).clone();
        let maker: crate::seam::Maker<SiegeWallDamage> = Arc::new(|_: Arc<SiegeWallDamage>| -> Arc<SiegeWallDamage> {
            Arc::new(|m: &mut CampaignModel, _: ForceId, r: RegionId, _: &mut ModWrites| {
                m.world.regions.get_mut(&r).unwrap().siege.walls_damage_mut().unwrap().hits = 9;
                Vec::new()
            })
        });
        rules.seams.siege_wall_damage.apply("test_walls", &maker);
        m.rules = Arc::new(rules);
        let (events, hits, factor, health) = wall_turn(&mut m, 0);
        assert_eq!((events.len(), hits, factor, health), (0, 9, 1.0, 100));
    }
}

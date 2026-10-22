//! A faction's capital through owner changes, and a faction's end (CAMPAIGN_FIDELITY.md
//! "Broadcasters": faction destruction).
//!
//! - Every owner change moves the capitals ([`CampaignModel::capital_after_owner_change`]): the old
//!   owner that loses its capital picks the nearest region it still holds (`0x008BA580` →
//!   `0x008DABC0`), the new owner makes the region its capital when it has none or the region is
//!   its home (`AddRegionToFaction` `0x008AE270`).
//! - A capture, the campaign director's transfer or a deal (not a liberation) that leaves the old
//!   owner without a capital ends it ([`CampaignModel::end_if_capital_lost`], `0x00A64AC0`):
//!   [`CampaignModel::destroy_faction`] (`0x008AB6C0` with no killer).
//! - A faction destroyed by another (the script's `grant_faction_handover`, a mission's takeover
//!   reward) hands its regions to the killer ([`CampaignModel::destroy_faction_by`], `0x008AB6C0`
//!   with a killer; [`CampaignModel::grant_faction_handover`], [`CampaignModel::take_over_faction`]).

use std::collections::BTreeSet;

use super::details::Relationship;
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::world::{CampaignModel, Region, Stance};

/// The settlement building slots from which a region is preferred as the new capital (`0x008DABC0`
/// compares the settlement's slot count, settlement +0x1A8, with 4; CONFIRMED).
const CAPITAL_TOWN_SLOTS: usize = 4;

/// The settlement's own building slots (settlement +0x1A8: the slots `AttachSlotToSettlement`
/// `0x00B6D270` appends, road and walls apart, CONFIRMED; it is called from `0x00A4D952` for a
/// slot whose region-template record has its byte +0x40 set). INFERRED: those are the slots whose
/// key starts with `settlement:` (the byte +0x40 read as "the slot sits in the settlement").
fn settlement_slots(r: &Region) -> usize {
    r.slots.iter().filter(|s| s.key.starts_with("settlement:")).count()
}

/// The relationship reset of a faction leaving the game (`0x00B72900`; `0x00B2B920` does the same
/// after its trade break, CONFIRMED): no trade agreement, access, war ally, commitment, payments,
/// tribute or income (their running-record lines taken back, `0x00B72A04` / `0x00B72A20`, `0x00B2BA44` /
/// `0x00B2BA60`: the caller resets through [`CampaignModel::change_treaty_money`]), friendship,
/// war counters, access counters or embargo. The attitudes, the diplomacy options, the previous
/// stance, the start attitude and the region-return flag stay. The stance (neutral) is set by the
/// caller, which holds the stances.
fn reset_relationship(r: &mut Relationship) {
    r.trade_agreement = false;
    r.military_access_turns = 0;
    r.war_ally = None;
    r.alliance_commitment_turns = 0;
    r.payments.clear();
    r.protectorate_tribute = 0;
    r.protectorate_income = 0;
    r.friendship_turns = 0;
    r.unknown_16 = 0;
    r.allied_in_war_against.clear();
    r.war_momentum = 0;
    r.war_region_balance = 0;
    r.war_wealth_balance = 0;
    r.war_turns = 0;
    r.turns_since_battle = 0;
    r.military_access_streak = 0;
    r.military_access_granted = 0;
    r.military_access_elapsed = 0;
    r.access_cancel_grievance = 0;
    r.trade_embargo_turns = 0;
}

impl CampaignModel {
    /// The capitals after `region` passed from `old` to `new` (`TransferRegionToFaction` `0x00A64AC0`,
    /// CONFIRMED; neither step runs for the rebels, `0x008CEEF0`):
    /// - `old`'s capital was `region` (`0x008BA580`): it picks the nearest region it still holds to
    ///   the lost one ([`Self::nearest_capital`]); with none left it has no capital;
    /// - `new` gains it (`AddRegionToFaction` `0x008AE270`): it becomes the capital when `new` has
    ///   none or it is `new`'s home region ([`super::details::FactionDetails::capital_2`]), and then
    ///   also the home when `new` had none.
    pub(crate) fn capital_after_owner_change(&mut self, region: RegionId, old: FactionId, new: FactionId) {
        if !self.is_rebel_faction(old) && self.world.capital(old) == Some(region) {
            let next = self.nearest_capital(old, region);
            if let Some(d) = self.world.faction_details.get_mut(&old) {
                d.capital = next;
            }
        }
        if self.is_rebel_faction(new) {
            return;
        }
        if let Some(d) = self.world.faction_details.get_mut(&new)
            && (d.capital.is_none() || d.capital_2 == Some(region))
        {
            d.capital = Some(region);
            d.capital_2.get_or_insert(region);
        }
    }

    /// The new capital of `faction`, which lost its capital `lost` (`0x008DABC0`, CONFIRMED): over the
    /// regions it holds in its home theatre (every region: each map is one theatre), the squared
    /// distance (single precision, map units) from the lost capital's settlement to each settlement;
    /// the nearest region whose settlement has at least [`CAPITAL_TOWN_SLOTS`] building slots, else
    /// the nearest of the others, else none. A tie keeps the first region (strict `<`) in the
    /// faction's region list order ([`super::world::World::faction_regions_in_list_order`]: the exe
    /// walks faction +0x76C, count +0x778 / data +0x77C, at `0x008DAC2F`..`0x008DACF7`, CONFIRMED).
    fn nearest_capital(&self, faction: FactionId, lost: RegionId) -> Option<RegionId> {
        let (ox, oz) = self.world.regions.get(&lost)?.settlement.position;
        let mut town: Option<(f32, RegionId)> = None;
        let mut other: Option<(f32, RegionId)> = None;
        for r in self.world.faction_regions_in_list_order(faction).iter().filter_map(|id| self.world.regions.get(id)) {
            let (x, z) = r.settlement.position;
            let (dx, dz) = (x.to_f32() - ox.to_f32(), z.to_f32() - oz.to_f32());
            let d = dx * dx + dz * dz;
            let best = if settlement_slots(r) >= CAPITAL_TOWN_SLOTS { &mut town } else { &mut other };
            if best.is_none_or(|(b, _)| d < b) {
                *best = Some((d, r.id));
            }
        }
        town.or(other).map(|(_, r)| r)
    }

    /// After an owner change that can end the old owner (`0x00A64AC0`'s test at `0x00A651D5`..`0x00A65208`,
    /// CONFIRMED): the change was a capture, the campaign director's transfer or a deal (its first flag;
    /// a liberation clears it), `old` is not the rebels, has no capital left and is still in the game.
    /// Then `old` leaves the game ([`Self::destroy_faction`]). The human new owner's statistic
    /// (`0x008CA100(6, 1)`) is not modelled.
    pub(crate) fn end_if_capital_lost(&mut self, old: FactionId, events: &mut Vec<CampaignEvent>) {
        if self.is_rebel_faction(old) {
            return;
        }
        let Some(d) = self.world.faction_details.get(&old) else { return };
        if d.capital.is_some() || d.out_of_game == Some(true) {
            return;
        }
        self.destroy_faction(old, events);
    }

    /// `faction` leaves the game with no killer ([`Self::destroy_faction_by`] with `None`).
    pub(crate) fn destroy_faction(&mut self, faction: FactionId, events: &mut Vec<CampaignEvent>) {
        self.destroy_faction_by(faction, None, events);
    }

    /// `faction` leaves the game, destroyed by `killer` if any (`DestroyFactionAndResetRelations`
    /// `0x008AB6C0`, CONFIRMED). With a killer, the regions it holds are first listed (its region list,
    /// faction +0x76C, copied); then:
    /// 1. it is marked out of the game ([`super::details::FactionDetails::out_of_game`], faction +0x824);
    ///    its faction +0x754 is cleared too (read by `0x008CEEA0` / `0x008E2A80`; the notes read it as
    ///    the patron in the sight sources and as a forced war target in the negotiation options, and the
    ///    model holds neither as a field: a patron tie is the stance pair, which step 2 sets neutral;
    ///    which reading is right is not traced, BACKLOG);
    /// 2. each relationship between it and another faction, both ways, is reset (`0x00B2B8B0` →
    ///    `0x00B2B920` on its own record, `0x00B72900` on the other's; [`reset_relationship`]) and the
    ///    stance becomes neutral. Only its own record towards the killer, when it holds a trade
    ///    agreement (record +0x788), first breaks that trade both ways (`0x00B2B920` →
    ///    `BreakTradeBothWays` `0x00B73430`, [`Self::break_trade_both_ways`]); nothing else is posted or
    ///    broadcast;
    /// 3. an AI faction loses everything on the map: no character of it returns after death (character
    ///    +0x52C cleared), every force goes ([`Self::destroy_force`]: the character loop `0x009DAB40`
    ///    deletes each one's army or navy) and every character dies ([`Self::character_dies`]; no post is
    ///    refilled, see [`Self::refill_post`]); the town slots of other factions' regions it held go back
    ///    to their region's owner, each port slot running the trade price update with flag 0 (the exe
    ///    does this through its occupants' leave paths, CONFIRMED, priceupdate: the army destructor
    ///    `0x0088CE30` → `0x00A64560` for a slot garrison, and `0x00B794B0` from `0x008ABB2D`/`0x008ABB3D`,
    ///    which recomputes a slot's holder from its region owner, docked navy and garrison and calls
    ///    `SetSettlementOwner` `0x00B2B810` on a change; which slots go back in our model is INFERRED,
    ///    as slot garrisons are not modelled). With a killer, the killer's destroyed-a-faction flag is
    ///    set ([`super::details::FactionDetails::destroyed_faction`], killer +0x924 at `0x008ABC50`, even
    ///    with no region to hand over) and, while the faction holds a region, its first region goes to
    ///    the killer, or its second when the first is its capital (`0x00A8B5A0`) and it holds more than
    ///    one, so the capital goes last (`TransferSettlementWithoutCharacter` `0x00B58A10(killer, 0, 0)`:
    ///    [`Self::transfer_region`], which neither ends the old owner nor brings anyone back). The
    ///    exe's loop at `0x008ABD3E` that hands back held forts (`0x00B1B010`, region +0x240 list) has
    ///    no price effect, and our forts have no holder. A human faction keeps its forces, characters
    ///    and regions, and the campaign is over ([`Self::end_campaign`], `0x008C30A0` from `0x008AB6C0`
    ///    when the faction is human, CONFIRMED; the exe's multiplayer messages are not modelled);
    /// 4. [`CampaignEvent::FactionDestroyed`] (the exe's event +0x108), except for an AI faction with a
    ///    killer, which posts none; for a human faction its defeat movie follows
    ///    ([`CampaignEvent::OutcomeMovie`]: the listener `0x00A14E80` on that event with its flag 1,
    ///    CONFIRMED) when the data names one ([`Self::outcome_movie`]);
    /// 5. with a killer and a listed region, the director garrisons the handed-over regions
    ///    (`0x00BA80E0`, CONFIRMED): only for a killer in the CAI world ([`super::director::in_cai_world`],
    ///    `0x00C3C4E0`), the listed regions are sorted (the library sort `0x00B7FEE0`,
    ///    [`crate::msvc_sort::sort_by`]) by settlement building slots, most first, then by recruitable
    ///    units, most first (the comparator's tie test is `>=`), and the first gets a new garrison
    ///    ([`Self::spawn_transfer_garrison`], `0x00BA8400`), then the second when more than three were
    ///    listed (`0x00BA8090`). A region the killer does not own (a human faction keeps its regions)
    ///    gets none: the exe runs the pass for a human target too, and `0x00BA8400` then spawns
    ///    nothing, so ours runs it the same way. INFERRED: the sort key is our [`settlement_slots`]
    ///    (which slots count is INFERRED there), and the CAI settlement's count is taken to be that
    ///    plus a constant (see [`Self::garrison_handed_regions`]).
    ///
    /// The listed regions and the hand-over walk follow the faction's region list (faction +0x76C,
    /// count +0x778 / data +0x77C; [`super::world::World::faction_regions_in_list_order`]), CONFIRMED.
    ///
    /// With no killer, it is called only for a faction left with no region (see
    /// [`Self::end_if_capital_lost`]): the exe's handing out of regions it still holds in other
    /// theatres (a revolt, `0x00A79810`, else the faction scoring best, AI 5 / human 1, +2 for the same
    /// home theatre) is never reached on a map of one theatre.
    ///
    /// ORIGINAL BUG: `0x008AB6C0` hands the destroyed faction's regions to the killer with no
    /// killer ≠ destroyed check; when they are the same faction, `TransferRegionToFaction`
    /// `0x00A64AC0` returns early on a same-owner transfer (`0x00A64D13`), the region never leaves the
    /// list and the loop never ends. Ours refuses a faction destroying itself (logged, nothing
    /// changes) and returns false.
    pub(crate) fn destroy_faction_by(&mut self, faction: FactionId, killer: Option<FactionId>, events: &mut Vec<CampaignEvent>) -> bool {
        if killer == Some(faction) {
            log::warn!("{faction:?} cannot be destroyed by itself: refused");
            return false;
        }
        debug_assert!(killer.is_some() || !self.world.regions.values().any(|r| r.owner == faction), "a faction leaves the game holding a region");
        let listed: Vec<RegionId> = match killer {
            Some(_) => self.world.faction_regions_in_list_order(faction),
            None => Vec::new(),
        };
        if let Some(d) = self.world.faction_details.get_mut(&faction) {
            d.out_of_game = Some(true);
        }
        let mut others: BTreeSet<FactionId> = self.world.relationships.keys().filter(|(a, _)| *a == faction).map(|(_, b)| *b).collect();
        if let Some(f) = self.world.factions.get(&faction) {
            others.extend(f.diplomacy.keys().copied());
        }
        others.remove(&faction);
        for other in others {
            if Some(other) == killer && self.world.relationships.get(&(faction, other)).is_some_and(|r| r.trade_agreement) {
                self.break_trade_both_ways(faction, other);
            }
            for key in [(faction, other), (other, faction)] {
                self.change_treaty_money(key.0, key.1, reset_relationship);
                if let Some(stance) = self.world.factions.get_mut(&key.0).and_then(|f| f.diplomacy.get_mut(&key.1)) {
                    *stance = Stance::Neutral;
                }
            }
        }
        if !self.is_human(faction) {
            let characters: Vec<CharacterId> = self.world.characters.values().filter(|c| c.faction == faction).map(|c| c.id).collect();
            for c in &characters {
                if let Some(d) = self.world.character_details.get_mut(c) {
                    d.returns_after_death = false;
                }
            }
            let forces: Vec<ForceId> = self.world.forces.values().filter(|f| f.faction == faction).map(|f| f.id).collect();
            for f in forces {
                self.destroy_force(f, events);
            }
            let left: Vec<CharacterId> = self.world.characters.values().filter(|c| c.faction == faction).map(|c| c.id).collect();
            for c in left {
                self.character_dies(c);
            }
            let held: Vec<(RegionId, usize, bool)> = self
                .world
                .regions
                .iter()
                .flat_map(|(id, r)| r.slots.iter().enumerate().filter(|(_, s)| s.holder == Some(faction)).map(move |(i, s)| (*id, i, s.port)))
                .collect();
            for (region, slot, port) in held {
                if let Some(s) = self.world.regions.get_mut(&region).and_then(|r| r.slots.get_mut(slot)) {
                    s.holder = None;
                }
                if port {
                    self.update_commodity_prices_with(false);
                }
                // `SetSettlementOwner` rebuilds the region's demand (`0x00AB49F0(0)` at `0x00B2B85F`) and then
                // refreshes its town wealth (`0x00AB4410(0)` at `0x00B2B86F`, CONFIRMED).
                self.refresh_region_demand(region);
                self.refresh_region_town_wealth(region);
            }
            if let Some(killer) = killer {
                if let Some(d) = self.world.faction_details.get_mut(&killer) {
                    d.destroyed_faction = true;
                }
                self.hand_regions_to_killer(faction, killer, events);
            }
        } else {
            self.end_campaign();
        }
        if killer.is_none() || self.is_human(faction) {
            events.push(CampaignEvent::FactionDestroyed { faction });
        }
        if self.is_human(faction)
            && let Some(movie) = self.outcome_movie(faction, super::outcome::CampaignOutcome::Destroyed)
        {
            events.push(CampaignEvent::OutcomeMovie { faction, movie });
        }
        if let Some(killer) = killer
            && !listed.is_empty()
        {
            self.garrison_handed_regions(killer, &listed);
        }
        true
    }

    /// The script's `grant_faction_handover(killer, target, min_turn, max_turn)` (binding `0x00978950`,
    /// CONFIRMED): `target` still in the game (faction +0x824 clear) may be destroyed by `killer`
    /// ([`Self::destroy_faction_by`]) once per call between turns `min_turn` and `max_turn` (both
    /// included; a `max_turn` below `min_turn` is raised to it, `0x00978A55`), the turn being the
    /// calendar's elapsed turns. Inside the window one campaign RNG draw (`0..=100`,
    /// [`crate::rng::CaRng::percent_0_100`]) is made and the faction is destroyed when
    /// `100.0 / (max_turn − turn + 1)` (f32) is at least the draw, or on `max_turn` itself, so the
    /// chance rises to certain by the last turn. Outside the window, or with the target out of the
    /// game, nothing happens and no draw is made. Returns the destruction's events when the target was
    /// destroyed ([`Self::destroy_by_command`]: an AI target destroyed by a killer posts no
    /// `FactionDestroyed`).
    pub fn grant_faction_handover(&mut self, killer: FactionId, target: FactionId, min_turn: i32, max_turn: i32) -> Option<Vec<CampaignEvent>> {
        if self.world.faction_details.get(&target).is_some_and(|d| d.out_of_game == Some(true)) {
            return None;
        }
        let max_turn = max_turn.max(min_turn);
        let turn = self.calendar.turns_elapsed as i32;
        if turn < min_turn || turn > max_turn {
            return None;
        }
        // The exe's int arithmetic, wrapping (a max_turn near i32::MAX).
        let threshold = 100.0f32 / max_turn.wrapping_sub(turn).wrapping_add(1) as f32;
        let draw = self.rng.percent_0_100() as f32;
        if threshold < draw && turn < max_turn {
            return None;
        }
        self.destroy_by_command(target, killer)
    }

    /// A mission's takeover reward (`0x00A06050`, after its money; CONFIRMED): the mission's target
    /// faction (mission +0x68) is destroyed by the mission's owner ([`Self::destroy_by_command`]),
    /// with no other check. Returns the destruction's events, or `None` when it was refused (the owner
    /// is the target, logged by [`Self::destroy_faction_by`]). Mission completion is not modelled yet,
    /// so nothing calls this (the reward is only saved, [`super::details::CampaignMission::reward_takeover`]).
    pub fn take_over_faction(&mut self, target: FactionId, owner: FactionId) -> Option<Vec<CampaignEvent>> {
        self.destroy_by_command(target, owner)
    }

    /// `target` destroyed by `killer` outside the turn steps and commands (the script binding, a
    /// mission reward): [`Self::destroy_faction_by`], then `CharacterCreated` for each character it
    /// created (the killer's garrison colonel), as [`Self::apply`] reports a command's. `None` when
    /// the destruction was refused.
    fn destroy_by_command(&mut self, target: FactionId, killer: FactionId) -> Option<Vec<CampaignEvent>> {
        let known: BTreeSet<CharacterId> = self.world.characters.keys().copied().collect();
        let mut events = Vec::new();
        if !self.destroy_faction_by(target, Some(killer), &mut events) {
            return None;
        }
        events.extend(self.created_since(&known));
        Some(events)
    }

    /// Step 3's hand-over of every region `faction` holds to `killer` (`0x008AB6C0`'s loop over the
    /// faction's region list, `0x00B58A10(killer, 0, 0)` each): the first region of the list
    /// ([`super::world::World::faction_regions_in_list_order`]), or the second when the first is the
    /// faction's capital (`0x00A8B5A0`) and another is left.
    fn hand_regions_to_killer(&mut self, faction: FactionId, killer: FactionId, events: &mut Vec<CampaignEvent>) {
        loop {
            let list = self.world.faction_regions_in_list_order(faction);
            let mut held = list.iter().copied();
            let Some(first) = held.next() else { return };
            let region = match held.next() {
                Some(second) if self.world.is_capital(first) => second,
                _ => first,
            };
            if !self.transfer_region(region, killer, events) {
                // Not reached: the killer is another faction, so the owner always changes.
                log::error!("{faction:?}'s region {region:?} did not pass to {killer:?}: hand-over stopped");
                return;
            }
        }
    }

    /// Step 5 (`0x00BA80E0` → `0x00BA8090`): the killer's new garrisons in the sorted `listed` regions.
    fn garrison_handed_regions(&mut self, killer: FactionId, listed: &[RegionId]) {
        if !super::director::in_cai_world(self, killer) {
            return;
        }
        // (region, settlement slots, recruitable units): the exe's key is the CAI settlement's building
        // slot count (`0x00B7CF1A`); its tie key is the region's recruitable list count (region +0x1B0),
        // read after the hand-over. INFERRED: ours sorts by [`settlement_slots`] (itself INFERRED),
        // taking the CAI count to be the settlement's own slots plus two, as on every settlement of
        // three original saves, so the constant cancels out; the CAI settlement's slot list builder is
        // not traced (which two slots it adds, and whether every settlement gets them).
        let mut sorted: Vec<(RegionId, usize, usize)> = listed
            .iter()
            .filter_map(|id| Some((*id, settlement_slots(self.world.regions.get(id)?), self.recruitable_units(*id).len())))
            .collect();
        crate::msvc_sort::sort_by(&mut sorted, |a, b| if a.1 != b.1 { a.1 > b.1 } else { a.2 >= b.2 });
        let picks = if sorted.len() > 3 { 2 } else { 1 };
        for &(region, _, _) in sorted.iter().take(picks) {
            self.spawn_transfer_garrison(region, killer);
        }
    }
}

/// The two records of a pair a re-emergence resets (`0x00B2BB00`): the returning faction's own record
/// (`0x00B2BC10`) and the other faction's record towards it (`0x00B72AC0`), which differ only in the
/// stance a liberation gives and the attitude a recapture costs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RecordSide {
    /// The returning faction's record towards another faction (`0x00B2BC10`).
    Returning,
    /// Another faction's record towards the returning faction (`0x00B72AC0`).
    Other,
}

impl CampaignModel {
    /// A region passes to `faction` through a liberation (`TransferRegionToFaction` `0x00A64AC0` with its
    /// second flag set, from `0x00B58A10(faction, 0, 1)`, CONFIRMED): a faction out of the game first comes
    /// back ([`Self::reemerge_faction`], `0x00A64C98`..`0x00A64CB9`, the previous owner being the region's
    /// owner before the change, the liberator), then the owner change ([`Self::transfer_region`]); a
    /// liberation never ends the old owner (the first flag is clear). Returns false when the region does
    /// not exist or already belongs to `faction`.
    pub(crate) fn liberate_region(&mut self, region: RegionId, faction: FactionId, events: &mut Vec<CampaignEvent>) -> bool {
        let Some(prev) = self.world.regions.get(&region).map(|r| r.owner).filter(|&o| o != faction) else { return false };
        if !self.in_the_game(faction) {
            self.reemerge_faction(faction, prev, true, events);
        }
        self.transfer_region(region, faction, events)
    }

    /// `faction`, out of the game, comes back (`0x008AE6E0`, CONFIRMED): every relationship between it and
    /// another faction is set up again, both ways (`0x00B2BB00`: for each of its records, its own record
    /// then the other faction's record towards it; [`Self::reemergence_record`]), then it is marked in the
    /// game (faction +0x824 cleared). `prev` is the region's owner before the transfer that brings it back,
    /// `liberation` the transfer's second flag.
    ///
    /// The rebels hold no diplomacy record (the rebel faction has no `DIPLOMACY_MANAGER` in the save): no pair with them is set up.
    ///
    /// Not modelled: the effect-sum rebuild (`RebuildFactionEffectSum`; the model derives effects when read)
    /// and the campaign-specific step of `0x00B2BB00` for a transfer without the flag (the revolt path,
    /// not in the model: BACKLOG §0-B).
    ///
    /// INFERRED: the returning faction's posts stay empty (its destruction killed every holder and
    /// `0x008AE6E0` fills none); whether and when the exe gives them new holders (a later refill, the
    /// liberation's army `0x00B4F090` with its new general) is not traced (BACKLOG).
    pub(crate) fn reemerge_faction(&mut self, faction: FactionId, prev: FactionId, liberation: bool, events: &mut Vec<CampaignEvent>) {
        let others: Vec<FactionId> =
            self.world.factions.keys().copied().filter(|&o| o != faction && !self.is_rebel_faction(o)).collect();
        for other in others {
            self.reemergence_record(faction, other, prev, liberation, RecordSide::Returning, events);
            self.reemergence_record(other, faction, prev, liberation, RecordSide::Other, events);
        }
        if let Some(d) = self.world.faction_details.get_mut(&faction) {
            d.out_of_game = Some(false);
        }
    }

    /// One record of a re-emergence (`0x00B2BC10` / `0x00B72AC0`, CONFIRMED), `owner`'s record towards
    /// `target`:
    /// 1. the attitude starts from the record's start attitude (record +0x830) and the stance from neutral;
    /// 2. the pirates on either side (faction key `pirates`, `IsPiratesFaction` `0x008CEE20`: the director's
    ///    [`super::director::DirectorRules::no_rival_keys`]): war, attitude + the `war` event value; else when
    ///    the pair's other faction (the record's target on the returning side, its owner on the other side,
    ///    `0x00898B20`) is `prev`: with `liberation` protectorate (the returning faction's record) or patron
    ///    (the other's); without it, an AI `prev` costs 50 (returning) or 25 (other) attitude and a human
    ///    `prev` means war, attitude + the `war` event value; any other pair stays neutral;
    /// 3. the record is set up (`0x00B45A40`): every attitude factor cleared (`0x00B1C110`), then
    ///    `initial_modifier` = the attitude (`0x00B69620`), the caps of `declared_war_against_enemies` and
    ///    `declared_war_against_friends` from the `war_against_enemies` / `war_against_friends` event values
    ///    (`0x00B69B90`), the `war` factor from the `war` event for a war, the `alliance` factor from the
    ///    `alliance` event for an alliance or a protectorate, and the religion and government factors
    ///    ([`Self::setup_relationship_factors`], `0x00B1B540`);
    /// 4. what the stance brings (`0x00B45C20`): a protectorate goes back to neutral both ways and is made
    ///    through [`Self::make_protectorate`] (`0x00B105C0` with no peace step). A trade agreement the record
    ///    still holds is re-checked there; the faction's destruction cleared them all.
    ///
    /// PROVISIONAL: the attitude adjustment that follows (`0x00B30A50`: from the attitude category of the
    /// `initial_modifier` contribution, a drift on a further factor, not traced to its field) is not
    /// modelled; the exe's record has a second factor array (+0x3C8) that the setup points the first
    /// array's later writes at, and the model holds one array; a war (the pirates, a human `prev`) is set as the stance
    /// with its factor, while the exe resets it to neutral and declares it through `0x00B268B0`, whose own
    /// declaration step is not traced.
    fn reemergence_record(&mut self, owner: FactionId, target: FactionId, prev: FactionId, liberation: bool, side: RecordSide, events: &mut Vec<CampaignEvent>) {
        let war = super::treaties::event("war");
        let mut attitude = self.world.relationships.get(&(owner, target)).map_or(0, |r| r.start_attitude);
        let pirates = super::director::is_pirate(self, owner) || super::director::is_pirate(self, target);
        let stance = if pirates {
            attitude += war.value;
            Stance::War
        } else if prev != if side == RecordSide::Returning { target } else { owner } {
            Stance::Neutral
        } else if liberation {
            if side == RecordSide::Returning { Stance::Protectorate } else { Stance::Patron }
        } else if !self.is_human(prev) {
            attitude -= if side == RecordSide::Returning { 50 } else { 25 };
            Stance::Neutral
        } else {
            attitude += war.value;
            Stance::War
        };
        let r = self.relationship_mut(owner, target);
        r.attitudes = vec![Default::default(); super::details::ATTITUDE_FACTORS.len()];
        r.factor_mut("initial_modifier").reset(attitude);
        for (factor, cap) in [("declared_war_against_enemies", "war_against_enemies"), ("declared_war_against_friends", "war_against_friends")] {
            let f = r.factor_mut(factor);
            f.cap = super::treaties::event(cap).value;
            f.capped = true;
        }
        match stance {
            Stance::War => r.set_factor("war", war),
            Stance::Allied | Stance::Protectorate => r.set_factor("alliance", super::treaties::event("alliance")),
            Stance::Neutral | Stance::Patron => {}
        }
        if let Some(f) = self.world.factions.get_mut(&owner) {
            f.diplomacy.insert(target, stance);
        }
        self.setup_relationship_factors(owner, target);
        if stance == Stance::Protectorate {
            for (a, b) in [(owner, target), (target, owner)] {
                if let Some(f) = self.world.factions.get_mut(&a) {
                    f.diplomacy.insert(b, Stance::Neutral);
                }
            }
            match self.make_protectorate(owner, target) {
                Ok(e) => events.extend(e),
                Err(e) => log::warn!("{owner:?} came back as the protectorate of {target:?}, but the protectorate failed: {e:?}"),
            }
        }
    }
}

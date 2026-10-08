//! The turn loop: rounds, faction turns and their phases.
//!
//! **What is CONFIRMED:** the script event names (FactionRoundStart, FactionTurnStart/End,
//! RegionTurnStart/End, CharacterTurnStart/End, SlotTurnStart, UnitTurnEnd; W1 §4 "Campaign
//! engine", W3 §6.4), the end-turn command name `CCQ_END_TURN`, that factions take their turns
//! one after another, and (round 9, from the exe) the order of the faction turn start `0x008F2620`
//! and turn end `0x008BD0F0` and of the round-end economy `0x008BC650`. The order below is pinned by
//! tests so that it only changes on purpose.
//!
//! ## Order
//! Factions play in [`World::factions_in_turn_order`](super::World::factions_in_turn_order) (the
//! startpos `FACTION_ARRAY` order, INFERRED). A *round* is every faction once:
//! 1. Round start: `FactionRoundStart` for every faction, in turn order (posted by `0x008F1F60`).
//! 2. For each faction in turn order, the turn start `0x008F2620` (CONFIRMED order of its calls):
//!    1. Every character (`0x00A24EB0` per character): `CharacterTurnStart` (INFERRED place of the event)
//!       and the action points refill to the type's base, times [`force_action_point_factor`] for a
//!       force's commander (CONFIRMED factor; trait, ancillary and technology movement effects are not
//!       modelled).
//!    2. Every owned region, in id order (`0x00AAE820`): the slots (`SlotTurnStart` each, `0x00AAE8D0`,
//!       with a port's naval queue right after its slot), construction counts down (`BuildingCompleted`;
//!       `0x00A78670`, every slot in one pass, CONFIRMED, caller not traced: INFERRED here), the land
//!       queue counts down (`UnitTrained`; `0x00B71FB0`, CONFIRMED), then `RegionTurnStart` (posted at
//!       the end of the region update, CONFIRMED). An item takes part only after a round end has
//!       flagged it (`0x00A78620` from the round-end economy `0x008BC650`, CONFIRMED); the saves agree.
//!       Not modelled in the region update: building-driven spawns `0x00A249A0` (a campaign-RNG roll).
//!    3. The faction-level updates: research availability (`0x008F91F0`, CONFIRMED place), then the
//!       other calls of `0x008F2620` not modelled (character pools `0x00A24C90`, pending orders resumed
//!       `0x009653D0`, forces in transit), and last `FactionTurnStart` (posted at its end, CONFIRMED).
//!    4. The faction acts: a **human** faction stops here and waits for commands until
//!       `CCQ_END_TURN`; an AI faction is played by the campaign AI (`ntw_ai`), which the caller runs
//!       while `TurnStep::AiTurn` is the next step (without a driver the AI does nothing).
//!    5. The turn end `0x008BD0F0` (CONFIRMED order): `CharacterTurnEnd` for every character
//!       (`0x009DA210`), every region (`0x00A786C0`: `RegionTurnEnd`, then its bankruptcy offset −1),
//!       `UnitTurnEnd` for every unit of every force (forces in id order, units in array order;
//!       `0x008BD4B0`), the diplomacy manager's turn end (`0x00B29940`, not read), `FactionTurnEnd`.
//! 3. Round end (CONFIRMED order, 0x00948CF0: when the turn passes the last faction, the economy of
//!    **every** faction is settled at once, not at each faction's turn start): for every faction in
//!    turn order, [`TurnStep::Economy`]: income and upkeep are settled (0x00BABE30: treasury +=
//!    income − expenses, unless the faction cannot pay, see [`economy`](super::economy)) and every
//!    owned region's town wealth grows (0x00AB4410), then research takes one step
//!    ([`super::research`]; CONFIRMED place: `0x008BC650` runs the step `0x008DD450`), then the relationships'
//!    per-turn update ([`super::treaties`], `0x00B29100`, after the regions in `0x008BC650`). Then the calendar advances one half-month
//!    (that this comes after the economy is our PROVISIONAL choice).
//!
//! A start position is loaded *before* turn 1 has begun: [`CampaignModel::start_campaign`] runs
//! the first round start and the factions up to the first human one. A save is loaded *inside* the
//! human player's turn (see [`TurnState::in_turn_of`]).
//!
//! ## Stepping
//! The work is a queue of steps. [`CampaignModel::step`] runs one step and returns its events, so
//! a caller (the Lua host) can fire each step's script events before the next step runs — the
//! scripts then see the state of that moment and their changes affect the following phases.
//! [`CampaignModel::end_turn`] simply runs every step.

use std::collections::VecDeque;

use super::economy;
use super::events::CampaignEvent;
use super::ids::{FactionId, RegionId};
use super::world::{BuildingRef, CampaignModel};
use crate::fnv::Fnv64;

/// One unit of turn work. See the module docs for the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TurnStep {
    /// `FactionRoundStart` for every faction.
    RoundStart,
    /// The calendar advances.
    RoundEnd,
    /// Start of a faction's turn: queues its phases.
    FactionStart(FactionId),
    /// Round end: the faction's income and upkeep, then its regions' town wealth.
    Economy(FactionId),
    /// One region's turn.
    Region(RegionId),
    /// `CharacterTurnStart` and action point refill for the faction's characters.
    CharactersStart(FactionId),
    /// The end of the faction's turn start: research availability, then `FactionTurnStart`.
    FactionReady(FactionId),
    /// Either stop for a human player or play the AI's turn.
    Decide(FactionId),
    /// The AI's actions: a caller (`ntw_ai::campaign::driver`, or the script host's AI hook)
    /// plays the faction while this is the next step; the step itself does nothing.
    AiTurn(FactionId),
    /// End of a faction's turn.
    FactionEnd(FactionId),
    /// Move on to the next faction after this one (and to a new round after the last).
    Advance(FactionId),
}

/// Whose turn it is and the work still queued. Part of the state (hashed).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TurnState {
    /// Factions played by humans. Without any, every faction is played by the AI and
    /// [`CampaignModel::end_turn`] plays exactly one round.
    pub humans: Vec<FactionId>,
    /// True once [`CampaignModel::start_campaign`] has run (or a save was loaded).
    pub started: bool,
    /// The faction whose turn it is.
    pub current: Option<FactionId>,
    /// True while `current` is between its start phases and its end phases (a human is playing).
    pub in_turn: bool,
    /// Work not done yet.
    pub queue: VecDeque<TurnStep>,
}

impl TurnState {
    /// The state of a loaded save: inside `faction`'s turn (saves are made by the human during
    /// their own turn; INFERRED).
    pub fn in_turn_of(faction: FactionId, humans: Vec<FactionId>) -> Self {
        TurnState { humans, started: true, current: Some(faction), in_turn: true, queue: VecDeque::new() }
    }

    pub(crate) fn hash_into(&self, h: &mut Fnv64) {
        h.u32(self.humans.len() as u32);
        for f in &self.humans {
            h.i32(f.raw());
        }
        h.u32(self.started as u32);
        h.i32(self.current.map_or(i32::MIN, FactionId::raw));
        h.u32(self.in_turn as u32);
        h.u32(self.queue.len() as u32);
    }
}

impl CampaignModel {
    /// True if `faction` is played by a human, or (with no humans at all) if it is the first
    /// faction in turn order, where an all-AI game pauses after each round.
    fn stops_for(&self, faction: FactionId) -> bool {
        if self.turn.humans.is_empty() {
            return self.world.factions_in_turn_order().first() == Some(&faction);
        }
        self.turn.humans.contains(&faction)
    }

    /// Begins turn 1 of a freshly loaded start position: round start, then every faction up to
    /// and including the start phases of the first human one. Returns every event. Does nothing
    /// if the campaign has already started.
    pub fn start_campaign(&mut self) -> Vec<CampaignEvent> {
        self.begin_start_campaign();
        self.run_pending()
    }

    /// Queues the work of [`start_campaign`](Self::start_campaign) without running it (for
    /// callers that [`step`](Self::step) themselves).
    pub fn begin_start_campaign(&mut self) {
        if self.turn.started {
            return;
        }
        self.turn.started = true;
        super::effects::apply_start_handicaps(self);
        // A new campaign sets every character's hidden flag (`0x008B42F0`, forced).
        let all: Vec<_> = self.world.characters.keys().copied().collect();
        for c in all {
            self.update_hidden(c);
        }
        let Some(&first) = self.world.factions_in_turn_order().first() else {
            return;
        };
        self.turn.queue.push_back(TurnStep::RoundStart);
        self.turn.queue.push_back(TurnStep::FactionStart(first));
        self.turn.queue.push_back(TurnStep::Decide(first));
    }

    /// `CCQ_END_TURN`: ends the current faction's turn and plays the following factions until the
    /// next human faction's turn has started (with no humans: one full round). Starts the
    /// campaign first if needed. Returns every event in order.
    pub fn end_turn(&mut self) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        if !self.turn.started {
            events.extend(self.start_campaign());
        }
        self.begin_end_turn();
        events.extend(self.run_pending());
        events
    }

    /// Queues the end of the current turn (see [`end_turn`](Self::end_turn)) without running it.
    /// If the campaign has not started, this queues the start instead; call it again once that has
    /// run.
    pub fn begin_end_turn(&mut self) {
        if !self.turn.started {
            self.begin_start_campaign();
            return;
        }
        if let Some(f) = self.turn.current.filter(|_| self.turn.in_turn) {
            self.turn.in_turn = false;
            self.turn.queue.push_back(TurnStep::FactionEnd(f));
            self.turn.queue.push_back(TurnStep::Advance(f));
        }
    }

    /// True while turn work is queued.
    pub fn has_pending_steps(&self) -> bool {
        !self.turn.queue.is_empty()
    }

    /// True if `faction` may act now: the campaign has not started (tests and tools), it is
    /// that faction's turn, or the faction is an AI whose [`TurnStep::AiTurn`] is the next step
    /// (the campaign AI, `ntw_ai::campaign::driver`, gives its orders then).
    pub fn may_act(&self, faction: crate::campaign::FactionId) -> bool {
        !self.turn.started
            || (self.turn.in_turn && self.turn.current == Some(faction))
            || self.turn.queue.front() == Some(&TurnStep::AiTurn(faction))
    }

    /// Runs every queued step and returns all their events.
    pub fn run_pending(&mut self) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        while let Some(mut ev) = self.step() {
            events.append(&mut ev);
        }
        events
    }

    /// Runs the next queued step. `None` when nothing is queued.
    pub fn step(&mut self) -> Option<Vec<CampaignEvent>> {
        let step = self.turn.queue.pop_front()?;
        let mut events = Vec::new();
        let known: std::collections::BTreeSet<_> = self.world.characters.keys().copied().collect();
        match step {
            TurnStep::RoundStart => {
                for faction in self.world.factions_in_turn_order() {
                    events.push(CampaignEvent::FactionRoundStart { faction });
                }
                // The relationships' computed factors (0x0096C050 → 0x008BAF30 per faction, after the round-start events).
                self.refresh_computed_factors();
            }
            TurnStep::RoundEnd => {
                // The yearly character pass (natural deaths, expired ancillaries) of the year's last
                // round end, before the calendar moves on (`0x008A9920`, CONFIRMED order).
                super::characters::yearly_character_pass(self);
                // The trade update of the round end (0x00BCB020): commodity prices.
                self.update_commodity_prices();
                self.calendar.advance_turn()
            }
            TurnStep::FactionStart(f) => {
                self.turn.current = Some(f);
                // The faction turn start 0x008F2620 (CONFIRMED order): every character (0x00A24EB0), every region
                // (0x00AAE820), then the faction-level updates (research availability 0x008F91F0) and last the
                // `FactionTurnStart` event (posted at its end). Its phases run next, before whatever was queued
                // after this step.
                let mut phases = vec![TurnStep::CharactersStart(f)];
                phases.extend(self.world.regions.values().filter(|r| r.owner == f).map(|r| TurnStep::Region(r.id)));
                phases.push(TurnStep::FactionReady(f));
                for p in phases.into_iter().rev() {
                    self.turn.queue.push_front(p);
                }
            }
            TurnStep::Economy(f) => {
                // Treaty money: regular payments and protectorate tribute (PROVISIONAL place, see `treaties`).
                self.diplomacy_money(f);
                if economy::settle_round(self, f) == (economy::Settlement::CannotPay { first_turn: true }) {
                    events.push(CampaignEvent::PendingBankruptcy { faction: f });
                }
                // Each region's religion conversion (0x00A63FE0, in the region update 0x00AB42F0 of the same step).
                self.religion_round_end(f);
                // Research (0x008DD450 with (0, 0, 1), called from the round-end economy 0x008BC650: CONFIRMED
                // place; the saves agree: AI techs started in turn 1 hold one step in the turn-2 save).
                for technology in self.research_step(f) {
                    events.push(CampaignEvent::ResearchCompleted { faction: f, technology });
                }
                // The relationships' per-turn update (0x00B29100, from the same round-end step 0x008BC650,
                // after the regions; CONFIRMED place).
                self.diplomacy_round_end(f);
            }
            TurnStep::Region(r) => self.region_turn(r, &mut events),
            TurnStep::FactionReady(f) => {
                // After the faction's regions (buildings may have finished): new techs open up (0x008F2620 runs the
                // availability writer 0x008F91F0 after its region updates, CONFIRMED), then `FactionTurnStart`.
                self.update_tech_availability(f);
                events.push(CampaignEvent::FactionTurnStart { faction: f });
            }
            TurnStep::CharactersStart(f) => {
                let ids: Vec<_> = self.world.characters.values().filter(|c| c.faction == f).map(|c| c.id).collect();
                let fx = super::effects::Effects::compute_for(self, f);
                for character in ids {
                    self.world.agents_acted.remove(&character);
                    events.push(CampaignEvent::CharacterTurnStart { character });
                    let commands = self.world.forces.values().any(|x| x.commander == Some(character));
                    let factor = if commands {
                        force_action_point_factor(&self.rules, fx.character_total(character, "general_admiral_action_point_bonus"))
                    } else {
                        1.0
                    };
                    if let Some(c) = self.world.characters.get_mut(&character) {
                        // Refill to the type's base, scaled for a force's commander (see
                        // `force_action_point_factor`). PROVISIONAL: the movement effects of
                        // traits, ancillaries and technologies (0x008A86D0) are not modelled.
                        c.max_movement_points = (c.base_movement_points as f32 * factor).round_ties_even() as i32;
                        c.movement_points = c.max_movement_points;
                    }
                }
                // A sabotaged force cannot move this turn (`0x008F2290` → `0x00A2A140`: the commander's,
                // and an army it carries, action points to 0; INFERRED to come after the refill).
                let sabotaged: Vec<_> = self.world.sabotaged.iter().copied().filter(|x| self.world.forces.get(x).is_some_and(|fo| fo.faction == f)).collect();
                for force in sabotaged {
                    self.world.sabotaged.remove(&force);
                    let carried = self.world.embarked.iter().filter(|(_, n)| **n == force).map(|(a, _)| *a).collect::<Vec<_>>();
                    for x in std::iter::once(force).chain(carried) {
                        if let Some(c) = self.world.forces.get(&x).and_then(|fo| fo.commander).and_then(|c| self.world.characters.get_mut(&c)) {
                            c.movement_points = 0;
                        }
                    }
                }
                // Each character's hidden flag at his turn start (`0x00A24EB0` → `0x009D3000`); then, in the
                // faction turn start `0x008F2620` (CONFIRMED order): a human faction's spy-network step
                // (`0x008F2480`: last turn's network shapes revealed again, this turn's listed and revealed,
                // `CharacterBuildsSpyNetwork` at exactly 3 idle turns) and the spotting pass (`0x008B4B70`;
                // an AI faction runs only the spotting pass), then the recruitment pools' step
                // (`0x00A25310` on the manager at faction +0x940, both pools' slot 12 `0x00A252B0`);
                // CHARACTERS_FIDELITY.md §8, §10.
                let mine: Vec<_> = self.world.characters.values().filter(|c| c.faction == f).map(|c| c.id).collect();
                for c in mine {
                    self.update_hidden(c);
                }
                if self.turn.humans.contains(&f) {
                    for character in self.spy_network_step(f) {
                        events.push(CampaignEvent::CharacterBuildsSpyNetwork { character });
                    }
                }
                self.spotting_pass(f);
                self.pool_tick(f);
                // The faction's sight: a turn start only adds (the box updates OR into the visible tree); the
                // reset is the turn end's (`0x008BD0F0` → `0x00B66EF0`, see `FactionEnd`).
                self.refresh_shroud(f, false);
            }
            TurnStep::Decide(f) => {
                if self.stops_for(f) {
                    self.turn.current = Some(f);
                    self.turn.in_turn = true;
                } else {
                    // Repairs of damaged buildings (PROVISIONAL stand-in for the AI's repair decision, see `capture`).
                    self.ai_repairs(f);
                    for s in [TurnStep::Advance(f), TurnStep::FactionEnd(f), TurnStep::AiTurn(f)] {
                        self.turn.queue.push_front(s);
                    }
                }
            }
            // The campaign AI acts *before* this step runs, while it is the next step (see
            // `may_act`); the driver lives in `ntw_ai` (this crate cannot depend on it).
            TurnStep::AiTurn(_) => {}
            TurnStep::FactionEnd(f) => {
                // The faction turn end 0x008BD0F0 (CONFIRMED order): every character (`CharacterTurnEnd`, 0x009DA210),
                // every region (0x00A786C0: `RegionTurnEnd`, then the bankruptcy offset falls by 1), every force's
                // units (`UnitTurnEnd`, 0x008BD4B0), then `FactionTurnEnd`.
                let mine: Vec<crate::campaign::CharacterId> = self.world.characters.values().filter(|c| c.faction == f).map(|c| c.id).collect();
                for c in mine {
                    // The turn-end counters come first (`0x009DA210`, CharacterTurnEnd after them).
                    self.turn_end_counters(c);
                    events.push(CampaignEvent::CharacterTurnEnd { character: c });
                }
                let regions: Vec<RegionId> = self.world.regions.values().filter(|r| r.owner == f).map(|r| r.id).collect();
                for region in regions {
                    events.push(CampaignEvent::RegionTurnEnd { region });
                    if let Some(r) = self.world.regions.get_mut(&region).filter(|r| r.wealth_growth_offset != 0) {
                        r.wealth_growth_offset -= 1;
                    }
                }
                let forces: Vec<_> = self.world.forces.values().filter(|x| x.faction == f).map(|x| x.id).collect();
                for force in forces {
                    for unit in self.world.forces[&force].units.iter().map(|u| u.id) {
                        events.push(CampaignEvent::UnitTurnEnd { force, unit });
                    }
                }
                events.push(CampaignEvent::FactionTurnEnd { faction: f });
                // Last in 0x008BD0F0 (CONFIRMED): a faction with a shroud clears its visible tree and
                // rebuilds it from every sight source over the whole map (`0x00B66EF0(0)`; the box
                // updates during the turn only added). CHARACTERS_FIDELITY.md §10.
                self.refresh_shroud(f, true);
            }
            TurnStep::Advance(f) => {
                let order = self.world.factions_in_turn_order();
                let pos = order.iter().position(|&x| x == f).unwrap_or(order.len());
                match order.get(pos + 1) {
                    Some(&next) => {
                        self.turn.queue.push_front(TurnStep::Decide(next));
                        self.turn.queue.push_front(TurnStep::FactionStart(next));
                    }
                    None => {
                        if let Some(&first) = order.first() {
                            // Round end (every faction's economy, then the calendar), then a new
                            // round; pushed to the front in reverse order.
                            for s in [TurnStep::Decide(first), TurnStep::FactionStart(first), TurnStep::RoundStart, TurnStep::RoundEnd] {
                                self.turn.queue.push_front(s);
                            }
                            for &each in order.iter().rev() {
                                self.turn.queue.push_front(TurnStep::Economy(each));
                            }
                        }
                    }
                }
            }
        }
        events.extend(self.created_since(&known));
        Some(events)
    }

    fn region_turn(&mut self, region: RegionId, events: &mut Vec<CampaignEvent>) {
        let Some(reg) = self.world.regions.get(&region) else { return };
        // The region update 0x00AAE820 (CONFIRMED order): the slots (`SlotTurnStart` each, 0x00AAE8D0, a port's naval
        // queue right after it), the land queue (0x00B71FB0), then `RegionTurnStart` (posted at its end).
        for slot in 0..reg.slots.len() as u32 {
            events.push(CampaignEvent::SlotTurnStart { region, slot });
        }

        // Recruitment (`0x00B71FB0` with force 0, from the region update `0x00AAE820`, CONFIRMED): first
        // the items whose unit the region can no longer recruit are removed (`0x00B1A820`, the cancel
        // path; refunded like a cancel, INFERRED). The cancel path also adds campaign variable 37
        // (`recruitment_population_cost`) to the queue manager's +0x54 (round 12 step 1; meaning
        // UNKNOWN, the model has no population simulation).
        let recruitable = self.recruitable_units(region);
        let owner = reg.owner;
        let mut refund = 0i32;
        if let Some(r) = self.world.regions.get_mut(&region) {
            r.recruitment_queue.retain(|i| {
                let keep = recruitable.contains(&i.unit_key);
                if !keep {
                    refund = refund.saturating_add(i.cost);
                }
                keep
            });
        }
        if refund != 0
            && let Some(f) = self.world.factions.get_mut(&owner)
        {
            f.treasury = f.treasury.saturating_add(refund);
        }
        let reg = &self.world.regions[&region];
        // Capacity (the queue's method 1: `recruitment_points`), read before the queues change.
        let caps = (self.recruitment_points(region, false), self.recruitment_points(region, true));
        let naval: Vec<bool> = reg.recruitment_queue.iter().map(|i| self.rules.units.get(&i.unit_key).is_some_and(|u| u.is_naval)).collect();

        // Construction: every item counts down one turn; finished items replace the slot's
        // building. Every slot advances its own item in the same pass (CONFIRMED: `0x00A78670` runs
        // `0x00B78800` for each slot of the settlement container, which steps the slot's construction item
        // when the region's owner holds the slot; the saves agree: Brandenburg farm 3/4 and supply post 1/3).
        let reg = self.world.regions.get_mut(&region).expect("region exists");
        // An item whose slot does not exist could never finish and would leave the region
        // `constructing` for good, so it is dropped here, at its first turn (no refund). The
        // loader and the commands only make items for existing slots (`ConstructBuilding` checks
        // `construction_slot`), so only code that edits `World` directly can make one. Each drop
        // is reported once ([`CampaignEvent::ConstructionItemDropped`], logged by the app, not sent
        // to the scripts). The original cannot reach this state: its item lives in the slot's own
        // `BUILDING_MANAGER`, so it has no drop or refund path to match (PROVISIONAL handling).
        let (kept, dropped): (Vec<super::ConstructionItem>, Vec<super::ConstructionItem>) =
            std::mem::take(&mut reg.construction).into_iter().partition(|i| reg.construction_slot(i.slot).is_some());
        reg.construction = kept;
        events.extend(dropped.into_iter().map(|i| CampaignEvent::ConstructionItemDropped { region, slot: i.slot, level_key: i.level_key }));
        let mut done = Vec::new();
        let held: Vec<bool> = reg.construction.iter().map(|i| reg.slot_held(i.slot)).collect();
        for (item, held) in reg.construction.iter_mut().zip(held) {
            if !held {
                continue;
            }
            item.turns_remaining = item.turns_remaining.saturating_sub(1);
            if item.turns_remaining == 0 {
                done.push(item.clone());
            }
        }
        reg.construction.retain(|i| i.turns_remaining > 0);
        for item in done {
            let building = Some(BuildingRef { level_key: item.level_key.clone(), health: 100 });
            if let Some(b) = reg.building_mut(item.slot) {
                *b = building;
            }
            events.push(CampaignEvent::BuildingCompleted { region, slot: item.slot });
        }

        // Then, in queue order, items count down (item method +0x18) until `recruitment_points` of
        // them have (CONFIRMED, `0x00B71FB0`); the rest wait. The land and naval queues are separate
        // managers with their own capacity. An item counts only after a round end has passed since it
        // was queued (flag +0x54, set by `0x00A78620` at the faction's round-end economy), which the
        // turn order here gives already. Items reaching zero finish, in queue order.
        let mut finished = Vec::new();
        let mut waiting = Vec::with_capacity(reg.recruitment_queue.len());
        let (mut land_n, mut naval_n) = (0u32, 0u32);
        for (mut item, is_naval) in std::mem::take(&mut reg.recruitment_queue).into_iter().zip(naval) {
            let (n, cap) = if is_naval { (&mut naval_n, caps.1) } else { (&mut land_n, caps.0) };
            *n += 1;
            if *n <= cap {
                item.turns_remaining = item.turns_remaining.saturating_sub(1);
            }
            if item.turns_remaining == 0 {
                finished.push(item.unit_key);
            } else {
                waiting.push(item);
            }
        }
        reg.recruitment_queue = waiting;
        for unit_key in finished {
            let ev = self.spawn_recruited_unit(region, unit_key);
            events.push(ev);
        }
        events.push(CampaignEvent::RegionTurnStart { region });
    }
}

/// The action point factor of everything in an army or a navy (0x008C3250 → 0x008C3170,
/// CONFIRMED): `clamp(1 + general_admiral_action_point_bonus + max(0, effect)/100, 1.0, 1.6)`,
/// where the effect is the `general_admiral_action_point_bonus` effect (128; a faction / character
/// effect: the commander's faction sum + own set, [`super::effects::Effects::character_total`]). The original applies it to the movement points of every unit whose
/// military force is an army (type 0) or a navy (type 1; 0x00F9C6C0 / 0x00F9C690, the types the
/// script functions counting armies and navies use), so commanders of any kind qualify
/// (generals, colonels, admirals, captains), not agents. The model keeps action points on the
/// commanding character, so the factor scales the commander's refill (INFERRED mapping).
pub fn force_action_point_factor(rules: &super::rules::CampaignRules, effect: f32) -> f32 {
    let f = 1.0 + rules.var("general_admiral_action_point_bonus", 0.0) + effect.max(0.0) / 100.0;
    f.clamp(1.0, 1.6)
}

impl CampaignModel {
    /// Sets every force commander's [`max_movement_points`](super::Character::max_movement_points)
    /// to its base times [`force_action_point_factor`] and everyone else's to the base, without
    /// touching the points left. The loader calls it once the rules are known, so a loaded
    /// campaign shows the same maximum the refill will give.
    pub fn refresh_movement_maximums(&mut self) {
        let fx = super::effects::Effects::compute(self);
        let commanders: std::collections::BTreeSet<_> = self.world.forces.values().filter_map(|f| f.commander).collect();
        let rules = self.rules.clone();
        for c in self.world.characters.values_mut() {
            let f = if commanders.contains(&c.id) {
                force_action_point_factor(&rules, fx.character_total(c.id, "general_admiral_action_point_bonus"))
            } else {
                1.0
            };
            c.max_movement_points = (c.base_movement_points as f32 * f).round_ties_even() as i32;
        }
    }
}

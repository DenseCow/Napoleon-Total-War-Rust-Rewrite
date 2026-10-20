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
//! startpos `FACTION_ARRAY` order, loader `0x0090BEB0`, then the rebels last; CONFIRMED). A *round* is
//! every faction once, ended by the rebels' turn; the turn passes over a faction out of the game
//! (`0x00948CF0`, CONFIRMED, [`CampaignModel::takes_turns`]), which gets no turn start, turn end or
//! handover until it re-emerges:
//! 1. Round start (`0x0096C050`): `FactionRoundStart` for every faction, in turn order, out-of-game
//!    factions included and the rebels last (CONFIRMED: posted by `0x008F1F60` outside its +0x824 test;
//!    that function also refreshes the region population factors, `0x00AAE710`, of a faction in the game
//!    other than the rebels).
//! 2. For each faction in turn order, the turn start `0x008F2620` (CONFIRMED order of its calls):
//!    1. Every character, in the faction's character list order (faction +0x764 count / +0x768 data, front
//!       to back, `0x008F2AC4`, and `0x008F2B47` in the branch for the rebels; CONFIRMED;
//!       [`World::faction_characters_in_list_order`](super::World::faction_characters_in_list_order)),
//!       `0x00A24EB0` per character: `CharacterTurnStart` (INFERRED place of the event)
//!       and the action points refill to the type's base, times [`force_action_point_factor`] for a
//!       force's commander (CONFIRMED factor; trait, ancillary and technology movement effects are not
//!       modelled).
//!    2. Every owned region, in the faction's region list order (faction +0x778 count / +0x77C data, front
//!       to back, `0x008F2AF6`, and `0x008F2B7F` in the branch for the rebels; CONFIRMED, CAMPAIGN_FIDELITY.md
//!       "Turn order"; [`World::faction_regions_in_list_order`](super::World::faction_regions_in_list_order)),
//!       the region update `0x00AAE820`: the slots (`SlotTurnStart` each, `0x00AAE8D0`, with a port's naval
//!       queue right after its slot), the land queue counts down (`UnitTrained`; `0x00B71FB0`, CONFIRMED), then
//!       `RegionTurnStart` (posted at the end of the region update, CONFIRMED). An item takes part only
//!       after a round end has flagged it (`0x00A78620` from the round-end economy `0x008BC650`,
//!       CONFIRMED); the saves agree.
//!       Not modelled in the region update: building-driven spawns `0x00A249A0` (a campaign-RNG roll).
//!    3. The faction-level updates: the reinforcement manager (`0x00B71FA0` at `0x008F2B35`: the armies raised
//!       for a commander march to him, [`CampaignModel::march_reinforcements`]; CONFIRMED place), research
//!       availability (`0x008F91F0` through `0x008AE3B0` at `0x008F2B40`, CONFIRMED place); the rebels skip
//!       both, since their branch `0x008F28E1` → `0x008F2B47` runs only the character and region walks
//!       (CONFIRMED); then the other calls of `0x008F2620` not modelled (character pools `0x00A24C90`, pending orders resumed
//!       `0x009653D0`, forces in transit), and last `FactionTurnStart` (posted at its end, CONFIRMED).
//!    4. The faction acts: a **human** faction stops here and waits for commands until
//!       `CCQ_END_TURN`; an AI faction is played by the campaign AI (`ntw_ai`), which the caller runs
//!       while `TurnStep::AiTurn` is the next step (without a driver the AI does nothing).
//!    5. The turn end `0x008BD0F0` (CONFIRMED order): `CharacterTurnEnd` for every character in the
//!       faction's character list order (`0x008BD1AF` → `0x009DA210`, CONFIRMED), every region in the faction's region list order (`0x008BD1E1`, CONFIRMED;
//!       `0x00A786C0`: `RegionTurnEnd`, then its bankruptcy offset −1),
//!       `UnitTurnEnd` for every unit of every force (forces in id order, units in array order;
//!       `0x008BD4B0`), the diplomacy manager's turn end (`0x00B29940`, not read), `FactionTurnEnd`.
//!    6. The handover ([`TurnStep::Handover`], `0x008A9920`, CONFIRMED order): right after the next
//!       faction's turn start (after the round end and the next round start when the round wraps),
//!       the campaign director's queued region transfers and, from a human to an AI faction, its
//!       step ([`super::director`]).
//! 3. Round end (CONFIRMED order, 0x00948CF0: when the turn passes the last faction, the economy of
//!    **every** faction is settled at once, not at each faction's turn start): first the
//!    population factors of the regions of every faction in the game but the rebels are refreshed
//!    ([`TurnStep::RoundEndRefresh`], `0x008E1030` per listed faction, which also sets the expense
//!    trackers), then for every faction in turn order, [`TurnStep::Economy`]: income and upkeep are settled
//!    (0x00BABE30: treasury += income − expenses, unless the faction cannot pay, see
//!    [`economy`](super::economy); not for the rebels or a faction out of the game), every owned region's
//!    town wealth grows (0x00AB4410) and its population grows and converts ([`super::population`],
//!    0x00AB4070), then (not for the rebels or a faction out of the game) every owned
//!    region's construction steps (`BuildingCompleted`, `CampaignBuildingDamaged`; `0x00AB3DA0` → `0x00A78670`,
//!    every slot in one pass; regions in the faction's region list order, faction +0x778 / +0x77C,
//!    CONFIRMED: CAMPAIGN_FIDELITY.md "Faction region list order", test
//!    `vanilla_save_region_order_is_the_manager_order` with 31 regions and 12 descents from id order;
//!    see `construction_round_end`), then the
//!    relationships' per-turn update ([`super::treaties`], `0x00B29100`), then research takes one step
//!    ([`super::research`], `0x008F3C40` → `0x008DD450`); CONFIRMED order of `0x008BC650`. So the queue
//!    step of the next turn start reads the list with the buildings finished here. Then the calendar
//!    advances one half-month (that this comes after the economy is our PROVISIONAL choice).
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
use super::mod_state::ModWrites;
use super::world::{BuildingRef, CampaignModel, ConstructionItem, Region};
use crate::fnv::Fnv64;

/// One unit of turn work. See the module docs for the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TurnStep {
    /// `FactionRoundStart` for every faction (out-of-game ones included, the rebels last; `0x0096C050`).
    RoundStart,
    /// The calendar advances.
    RoundEnd,
    /// The round end's first pass (`0x00948CF0` → `0x008E1030` for every listed faction, before any economy):
    /// the population factors of the regions of every faction in the game but the rebels are refreshed, then their expense trackers set ([`CampaignModel::prepare_round_end`]).
    RoundEndRefresh,
    /// Start of a faction's turn: queues its phases.
    FactionStart(FactionId),
    /// Round end: the faction's income and upkeep, then its regions' town wealth.
    Economy(FactionId),
    /// One region's turn.
    Region(RegionId),
    /// `CharacterTurnStart` and action point refill for the faction's characters.
    CharactersStart(FactionId),
    /// The end of the faction's turn start: research availability (not for the rebels), then `FactionTurnStart`.
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
    /// The turn handover from the ending faction to the starting one, after the starting one's turn
    /// start (`0x008A9920`): the campaign director's queued region transfers and, from a human to an
    /// AI faction, its step ([`CampaignModel::director_handover`]).
    Handover(FactionId, FactionId),
}

/// Whose turn it is and the work still queued. Part of the state (hashed).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    /// The campaign is over (campaign +0xFE9; `CAMPAIGN_MODEL` #18 of the exe's start positions and
    /// saves). Set only by [`CampaignModel::end_campaign`], never cleared; from then on no turn step
    /// runs. A save of ours from before it was kept loads it as false.
    #[cfg_attr(feature = "serde", serde(default))]
    pub campaign_over: bool,
}

impl TurnState {
    /// The state of a loaded save: inside `faction`'s turn (saves are made by the human during
    /// their own turn; INFERRED). The loader sets [`Self::campaign_over`] from the save afterwards.
    pub fn in_turn_of(faction: FactionId, humans: Vec<FactionId>) -> Self {
        TurnState { humans, started: true, current: Some(faction), in_turn: true, queue: VecDeque::new(), campaign_over: false }
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
        h.u32(self.campaign_over as u32);
    }
}

impl CampaignModel {
    /// The campaign is over (`SetCampaignOverFlag` `0x008C30A0`, CONFIRMED: it sets campaign
    /// +0xFE9 to 1 and nothing else). The one path that sets [`TurnState::campaign_over`]; the
    /// queued turn work is dropped, as nothing will run it. From then on the campaign update
    /// `0x008F4120` still runs its queued-command loop (from `0x008F4158`) but returns at the flag
    /// test `0x008F419E`, before time, the CAI step loop (`0x008F4524`, the AI's turns), the victory
    /// check `0x0096CD10` and the rest, so here [`Self::step`] runs nothing and [`Self::end_turn`]
    /// does nothing. [`Self::may_act`] is unchanged: a human destroyed in their own turn may still
    /// give commands, but one destroyed in an AI's turn leaves nobody who may act (the AI's
    /// [`TurnStep::AiTurn`] went with the queue). The campaign AI therefore gives no more orders
    /// once the flag is set (`ntw_ai::campaign::apply_orders_with_events`; PROVISIONAL: that stops
    /// the AI's remaining orders of the frame, while the exe's CAI loop has no test of the flag and
    /// finishes the frame's steps; what those steps then do is not traced). Saving is refused too
    /// (`0x008BE4A0` returns false while it is set; the save button's check is the UI's, BACKLOG).
    ///
    /// Callers: a human faction destroyed ([`Self::destroy_faction_by`], `0x008AB6C0`). Not modelled
    /// (BACKLOG): the multiplayer victory check (`0x0096CD10` sets it when a human has won and another
    /// human is still in the game; in single player winning or losing by the victory conditions does
    /// not set it), and the end of a human faction's +0x814 state (`0x008BEAA0` from a settlement
    /// transfer, `0x008DB1A0` from the faction's listener; a state the model does not have). What the
    /// exe shows next (the defeat or victory screen) is not traced.
    pub fn end_campaign(&mut self) {
        self.turn.campaign_over = true;
        self.turn.queue.clear();
    }

    /// True if `faction` is played by a human, or (with no humans) if it is the [first faction to
    /// take turns](Self::first_turn_taker), where an all-AI game pauses after each round. A human
    /// out of the game gets no `Decide` step ([`Self::takes_turns`]), and a destroyed human ends the
    /// campaign ([`Self::end_campaign`]). With humans listed but none of them taking turns (a state
    /// the exe never reaches, see [`Self::no_human_left`]) this stops for nobody; the loop is still
    /// bounded, as the next round start ends the campaign (`TurnStep::RoundStart`), whatever the
    /// order of the checks here.
    fn stops_for(&self, faction: FactionId) -> bool {
        if !self.turn.humans.is_empty() {
            return self.turn.humans.contains(&faction);
        }
        self.first_turn_taker() == Some(faction)
    }

    /// True if humans are listed but none of them [takes turns](Self::takes_turns) while the
    /// campaign is not over: the turn loop would never stop. The exe cannot reach it (a human
    /// leaves the game only through `0x008AB6C0`, which ends the campaign); a save of ours from
    /// before [`TurnState::campaign_over`] was kept, or a hand-built world, can. Tested once per
    /// round, at its start (`TurnStep::RoundStart`).
    fn no_human_left(&self) -> bool {
        !self.turn.humans.is_empty() && !self.turn.humans.iter().any(|&h| self.takes_turns(h))
    }

    /// The first faction in turn order that [takes turns](Self::takes_turns): where a campaign and
    /// each round begin, and where an all-AI game pauses.
    fn first_turn_taker(&self) -> Option<FactionId> {
        self.world.factions_in_turn_order().into_iter().find(|&f| self.takes_turns(f))
    }

    /// Whether `f` gets a turn when the turn moves on (`PassTurnToNextActiveFaction`
    /// `0x00948CF0`, CONFIRMED by disassembly): a listed faction out of the game (faction +0x824,
    /// [`Self::in_the_game`]) is passed over, so it gets no turn start, turn, turn end or handover
    /// until it comes back ([`Self::reemerge_faction`]); its round-end economy still runs (that loop
    /// walks every faction). The rebels (world +0x1C, after the list) are never tested. A save of
    /// ours from before the flag was kept has no +0x824: then `in_the_game` derives it by the exe's
    /// startpos rule ([`Self::out_of_game_by_startpos_rule`]). Not modelled: the exe also
    /// passes over a faction whose byte +0x93C is set, a flag no byte store in the exe writes
    /// (UNKNOWN meaning, never seen set).
    pub fn takes_turns(&self, f: FactionId) -> bool {
        self.is_rebel_faction(f) || self.in_the_game(f)
    }

    /// Begins turn 1 of a freshly loaded start position: round start, then every faction up to
    /// and including the start phases of the first human one. Returns every event. Does nothing
    /// if the campaign has already started.
    pub fn start_campaign(&mut self) -> Vec<CampaignEvent> {
        self.begin_start_campaign();
        self.run_pending()
    }

    /// Queues the work of [`start_campaign`](Self::start_campaign) without running it (for
    /// callers that [`step`](Self::step) themselves). Does nothing once the campaign is over.
    pub fn begin_start_campaign(&mut self) {
        if self.turn.started || self.turn.campaign_over {
            return;
        }
        self.turn.started = true;
        super::effects::apply_start_handicaps(self);
        // A new campaign sets every character's hidden flag (`0x008B42F0`, forced).
        let all: Vec<_> = self.world.characters.keys().copied().collect();
        for c in all {
            self.update_hidden(c);
        }
        let Some(first) = self.first_turn_taker() else {
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
    /// run. Does nothing once the campaign is over ([`Self::end_campaign`]).
    pub fn begin_end_turn(&mut self) {
        if self.turn.campaign_over {
            return;
        }
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

    /// Runs the next queued step. `None` when nothing is queued (always once the campaign is over:
    /// [`Self::end_campaign`] drops the queue, and a step that ends it drops what it queued), and
    /// also while a call to war waits for a human ally's answer
    /// ([`CampaignModel::pending_ally_war_calls`]): the queue is kept and the turn resumes with the
    /// next `step` after the answer. CONFIRMED: the campaign update (`0x008F4120`) does not advance the
    /// turn phases while the pending stack (+0xF70) holds an entry (`0x008F41F9`, `0x008F457A`); it only
    /// runs `0x008F7850`, which leaves an offer waiting for a human (counter 0) on the stack. An
    /// [`TurnStep::AiTurn`] at the front still runs (it does nothing: its faction's orders were given
    /// before it, and one of them asked), so a driver does not play that faction again on resuming.
    pub fn step(&mut self) -> Option<Vec<CampaignEvent>> {
        if !self.pending_ally_war_calls.is_empty() && !matches!(self.turn.queue.front(), Some(TurnStep::AiTurn(_))) {
            return None;
        }
        let step = self.turn.queue.pop_front()?;
        let mut events = Vec::new();
        let known: std::collections::BTreeSet<_> = self.world.characters.keys().copied().collect();
        match step {
            TurnStep::RoundStart if self.no_human_left() => {
                // Our own fix for a state the exe never reaches (see `no_human_left`), tested once per round:
                // without it the loop would play AI rounds forever. Logged once per campaign (the flag stops
                // the loop; the rest of the queue is dropped below).
                log::warn!("every human faction {:?} is out of the game but the campaign was not over; it ends now", self.turn.humans);
                self.end_campaign();
            }
            TurnStep::RoundStart => {
                // `0x0096C050` (CONFIRMED by disassembly): `0x008F1F60` for each listed faction (list +0x2C /
                // +0x30), then `0x008BAF30` for each listed faction, then `0x00AAAF60`, `0x00BCB020(0)` and
                // `0x008F5990`, and last a tail call of `0x008F1F60` on the rebels (world +0x1C). `0x008F1F60`
                // refreshes the faction's regions' population factors (`0x00AAE710`) unless the faction is the
                // rebels (`0x008CEEF0`) or out of the game (+0x824), and posts the event (`0x013563E4`, named by
                // `0x008BDE70`) either way. So every faction gets the event, the rebels' after the refreshes.
                let (rebels, listed): (Vec<_>, Vec<_>) =
                    self.world.factions_in_turn_order().into_iter().partition(|&f| self.world.is_rebel_faction(f));
                for &faction in &listed {
                    events.push(CampaignEvent::FactionRoundStart { faction });
                }
                // The listed factions' regions (not the rebels', not an out-of-game faction's), then the
                // relationships' computed factors (`0x008BAF30`, the same faction set).
                self.refresh_population_factors();
                // The same factions' running economics records: upkeep, taxes, category 6 and trade
                // (`0x008F1F60` step 1d, CONFIRMED; [`super::economy::refresh_running_record_round_start`]).
                let in_game: Vec<_> = listed.iter().copied().filter(|&f| self.in_the_game(f)).collect();
                for faction in in_game {
                    super::economy::refresh_running_record_round_start(self, faction);
                }
                self.refresh_computed_factors();
                // Siege attrition (`0x00AAAF60` per region of world +0x34, [`super::siege`]): each besieged
                // settlement's turns besieged go up and its garrison loses men by campaign RNG draws.
                self.siege_attrition_round_start(&mut events);
                // The trade update with flag 0 (`0x00BCB020(0)`): prices and trends, no history shift, then the
                // raid check (`0x00BD1020`) as at the round end.
                self.update_commodity_prices_with(false);
                // The round's historical events (`0x008F5990`, [`super::historical_events`]): the human factions'
                // event messages, with at most one campaign RNG draw per human faction.
                self.historical_events_round_start(&mut events);
                for faction in rebels {
                    events.push(CampaignEvent::FactionRoundStart { faction });
                }
            }
            TurnStep::RoundEndRefresh => self.prepare_round_end(),
            TurnStep::RoundEnd => {
                // The yearly character pass (natural deaths, expired ancillaries) of the year's last
                // round end, before the calendar moves on (`0x008A9920`, CONFIRMED order).
                super::characters::yearly_character_pass(self);
                // The trade update of the round end (0x00BCB020(1)): commodity prices, then the route rebuild
                // (0x00BD1020 at 0x00BCC5F8), whose check ends the raids no longer raidable.
                self.update_commodity_prices();
                // The calendar moves on (`0x008A98B0`), by the campaign's date step when its data names one.
                self.calendar.advance_turn(self.rules.features.date_step);
                // Then the deal inflation factor (`0x008A9920`, after the calendar, CONFIRMED order).
                let net = self.world_net_income();
                self.deal_inflation.round_end(net);
            }
            TurnStep::FactionStart(f) => {
                self.turn.current = Some(f);
                // The acting view on the path grid (`0x008ED860` at `0x008F28D3`), which every later
                // zone flood reads (`CampaignModel::store_acting_view`).
                self.store_acting_view();
                // The faction turn start 0x008F2620 (CONFIRMED order): every character (0x00A24EB0), every region
                // (0x00AAE820), then the faction-level updates (research availability 0x008F91F0) and last the
                // `FactionTurnStart` event (posted at its end). Its phases run next, before whatever was queued
                // after this step.
                let mut phases = vec![TurnStep::CharactersStart(f)];
                // The regions in the faction's region list order (+0x778 count / +0x77C data, walked front to
                // back at `0x008F2AF6`, and at `0x008F2B7F` for the rebels; CONFIRMED), the same list as the
                // round end's (see `construction_round_end`).
                phases.extend(self.world.faction_regions_in_list_order(f).into_iter().map(TurnStep::Region));
                phases.push(TurnStep::FactionReady(f));
                for p in phases.into_iter().rev() {
                    self.turn.queue.push_front(p);
                }
            }
            TurnStep::Economy(f) => {
                // The treaty money (regular payments, protectorate tribute and income) is in the running
                // record the settle pays (categories 8 / 10 / 22 / 24, booked by its writers,
                // `CampaignModel::change_treaty_money`). The rebels and a faction out of the game pay nothing
                // (`economy::Settlement::Skipped`).
                if economy::settle_round(self, f, &mut events) == (economy::Settlement::CannotPay { first_turn: true }) {
                    events.push(CampaignEvent::PendingBankruptcy { faction: f });
                }
                // The payments to `f` that an earlier payer's update finished this round go now that `f` has
                // been paid their last round (ORIGINAL BUG fix, `CampaignModel::diplomacy_round_end` step 8).
                self.drop_finished_payments_to(f);
                // Each region's population growth, then its religion conversion (0x00AB42F0 → 0x00AB4070 →
                // 0x00A63FE0, in the same step), for the rebels too: `0x008BC650` tests only faction +0x824 at
                // `0x008BC685` before that loop (CONFIRMED by listing).
                if self.in_the_game(f) {
                    self.population_round_end(f);
                }
                // Then, for a faction with a record still in the game (`Self::in_the_game_with_record`: the JNZs
                // at `0x008BC869` / `0x008BC871` jump past the whole block to `0x008BC8C4` for the rebels and a
                // faction out of the game, CONFIRMED by listing), `0x008BC650` steps each owned region's
                // construction (`0x00AB3DA0`, [`Self::construction_round_end`]) and flags its queued recruits
                // (`0x00A78620`, which the turn order gives already, see `region_turn`), then the relationships'
                // per-turn update (`0x00B29100`), then research (`0x008F3C40` → `0x008DD450` with (0, 0, 1));
                // CONFIRMED order from the listing. The same gate holds the family year (`0x008BC9C0`, in
                // `characters::yearly_character_pass`) and the last two steps `0x008AB200` / `0x008ABF50`
                // (over the faction's +0x7BC / +0x7C0 force list; not modelled).
                if self.in_the_game_with_record(f) {
                    self.construction_round_end(f, &mut events);
                    events.extend(self.diplomacy_round_end(f));
                    // The saves agree on research's place: AI techs started in turn 1 hold one step in the turn-2 save.
                    for technology in self.research_step(f) {
                        events.push(CampaignEvent::ResearchCompleted { faction: f, technology });
                    }
                }
            }
            TurnStep::Region(r) => self.region_turn(r, &mut events),
            TurnStep::FactionReady(f) => {
                // After the faction's regions (buildings may have finished): new techs open up (0x008F2620 runs the
                // availability writer 0x008F91F0, through the thunk 0x008AE3B0 at 0x008F2B40, after its region
                // updates, CONFIRMED), then `FactionTurnStart`. Before them the reinforcement manager (`0x00B71FA0`
                // at `0x008F2B35`, CONFIRMED order). The rebels skip both: for a faction without a record
                // (`IsFactionWithoutRecord` 0x008CEEF0) the JNZ at 0x008F28E1 takes the branch 0x008F2B47, which
                // walks the characters and regions only and joins at 0x008F2BB1, past the tax-rate log, the
                // reinforcement manager and research availability (CONFIRMED by listing).
                if !self.is_rebel_faction(f) {
                    events.extend(self.march_reinforcements(f));
                    self.update_tech_availability(f);
                }
                events.push(CampaignEvent::FactionTurnStart { faction: f });
            }
            TurnStep::CharactersStart(f) => {
                // The faction's character list front to back (+0x764 / +0x768 at `0x008F2AC4`, and at
                // `0x008F2B47` for the rebels → `0x00A24EB0`; CONFIRMED), not the ids.
                let ids = self.world.faction_characters_in_list_order(f);
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
                for c in self.world.faction_characters_in_list_order(f) {
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
                // units (`UnitTurnEnd`, 0x008BD4B0), then `FactionTurnEnd`. The regions go in the faction's region
                // list order, as the turn start's (+0x778 / +0x77C front to back at `0x008BD1E1`, CONFIRMED),
                // and the characters in the character list order (+0x764 / +0x768 at `0x008BD1AF`, CONFIRMED).
                for c in self.world.faction_characters_in_list_order(f) {
                    // The turn-end counters come first (`0x009DA210`, CharacterTurnEnd after them).
                    self.turn_end_counters(c);
                    events.push(CampaignEvent::CharacterTurnEnd { character: c });
                }
                for region in self.world.faction_regions_in_list_order(f) {
                    events.push(CampaignEvent::RegionTurnEnd { region });
                    if let Some(r) = self.world.regions.get_mut(&region).filter(|r| r.wealth_growth_offset != 0) {
                        r.wealth_growth_offset -= 1;
                    }
                }
                let forces: Vec<_> = self.world.forces.values().filter(|x| x.faction == f).map(|x| x.id).collect();
                for force in forces {
                    // A navy's turn end (`0x008BD430`) first repairs its ships ordered to repair (`super::ship_repair`).
                    self.repair_ships(force);
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
            TurnStep::Handover(ending, starting) => self.director_handover(ending, starting, &mut events),
            TurnStep::Advance(f) => {
                // The faction turn end clears its `WarStartedCharacterFaction` flag (+0x925, by the word
                // write at `0x008BD408`) after the `FactionTurnEnd` dispatch, before the shroud rebuild
                // (CONFIRMED). The scripts get a step's events only after the step, so clearing it in
                // `FactionEnd` would hide it from the `FactionTurnEnd` handlers; `Advance(f)` always runs
                // right after `FactionEnd(f)`, and nothing between them reads the flag. The same word write
                // clears the `FactionDestroyedByCharacterFaction` flag (+0x924).
                if let Some(d) = self.world.faction_details.get_mut(&f) {
                    d.war_started = false;
                    d.destroyed_faction = false;
                }
                let order = self.world.factions_in_turn_order();
                let pos = order.iter().position(|&x| x == f).unwrap_or(order.len());
                // The next faction that takes turns (`0x00948CF0`, [`Self::takes_turns`]). The rebels are
                // last in `order`, so their turn ends the round, as in the exe.
                match order.iter().skip(pos + 1).copied().find(|&x| self.takes_turns(x)) {
                    Some(next) => {
                        // The next faction's turn start, then the handover (`0x008A9920`, CONFIRMED order).
                        for s in [TurnStep::Decide(next), TurnStep::Handover(f, next), TurnStep::FactionStart(next)] {
                            self.turn.queue.push_front(s);
                        }
                    }
                    None => {
                        if let Some(first) = self.first_turn_taker() {
                            // Round end (every faction's economy, then the calendar), then a new
                            // round; pushed to the front in reverse order.
                            for s in [TurnStep::Decide(first), TurnStep::Handover(f, first), TurnStep::FactionStart(first), TurnStep::RoundStart, TurnStep::RoundEnd] {
                                self.turn.queue.push_front(s);
                            }
                            for &each in order.iter().rev() {
                                self.turn.queue.push_front(TurnStep::Economy(each));
                            }
                            self.turn.queue.push_front(TurnStep::RoundEndRefresh);
                        }
                    }
                }
            }
        }
        if self.turn.campaign_over {
            self.turn.queue.clear();
        }
        events.extend(self.created_since(&known));
        Some(events)
    }

    /// The construction step of `faction`'s round-end economy (`0x008BC650` → `0x00AB3DA0` per owned region,
    /// CONFIRMED place and order). The exe walks the faction's region list (faction +0x778 count / +0x77C data,
    /// `0x008BC873`: appended as the faction gains a region,`AddRegionToFaction` `0x008AE270`, and erased in place as it loses one,
    /// `0x008BA580` → `0x008B0210`, both from `0x00A64AC0`), and so does the model
    /// ([`super::world::World::faction_regions_in_list_order`]; the list's order after a load and in play
    /// is CONFIRMED, CAMPAIGN_FIDELITY.md "Faction region list order"), so the events (`BuildingCompleted`,
    /// `CampaignBuildingDamaged`) come out in the original's order. A region's step reads and changes only that
    /// region, its owner's treasury is not touched and a demolition clears only the researcher of its own school,
    /// so only the order of the events hangs on the list's order. Each region steps its slots in one
    /// pass (`0x00A78670`: the town / port slots in index order, then the road, then the walls, each through
    /// `0x00B78800`), one item at a time through the [`construction.step`](super::seams::CONSTRUCTION_STEP) seam
    /// ([`construction_step`] in the original game), each result applied before the next slot steps
    /// ([`Self::apply_construction_step`]). The rebel faction steps nothing: the caller, `TurnStep::Economy`, runs
    /// it only for [`Self::in_the_game_with_record`] (`0x008BC650` skips the whole block for a faction with no
    /// record, `0x008CEEF0`, or out of the game). The saves agree: Brandenburg farm 3/4 and supply post 1/3.
    fn construction_round_end(&mut self, faction: FactionId, events: &mut Vec<CampaignEvent>) {
        // `0x00A78670`'s order: the town / port slots, the road (+0x20), the walls (+0x24).
        let order = |s: super::SlotRef| match s {
            super::SlotRef::Slot(i) => (0, i),
            super::SlotRef::Road => (1, 0),
            super::SlotRef::Walls => (2, 0),
        };
        // The faction's region list order (+0x778 / +0x77C; see above).
        let mut regions = self.world.faction_regions_in_list_order(faction);
        regions.retain(|id| self.world.regions.get(id).is_some_and(|r| !r.construction.is_empty()));
        let mut writes = ModWrites::default();
        for region in regions {
            let Some(reg) = self.world.regions.get_mut(&region) else { continue };
            // An item whose slot does not exist could never finish and would leave the region
            // `constructing` for good, so it is dropped here, at its first step (no refund). The
            // loader and the commands only make items for existing slots (`ConstructBuilding` checks
            // `construction_slot`), so only code that edits `World` directly can make one. Each drop
            // is reported once ([`CampaignEvent::ConstructionItemDropped`], logged by the app, not sent
            // to the scripts). The original cannot reach this state: its item lives in the slot's own
            // `BUILDING_MANAGER`, so it has no drop or refund path to match (PROVISIONAL handling).
            let (kept, dropped): (Vec<super::ConstructionItem>, Vec<super::ConstructionItem>) =
                std::mem::take(&mut reg.construction).into_iter().partition(|i| reg.construction_slot(i.slot).is_some());
            events.extend(dropped.into_iter().map(|i| CampaignEvent::ConstructionItemDropped { region, slot: i.slot, level_key: i.level_key }));
            reg.construction = kept;
            // Stepped in slot order, one item per slot (a slot holds one, `BUILDING_MANAGER`); the list keeps its
            // own order.
            let mut slots: Vec<super::SlotRef> = reg.construction.iter().map(|i| i.slot).collect();
            slots.sort_by_key(|&s| order(s));
            slots.dedup();
            for slot in slots {
                let reg = &self.world.regions[&region];
                let Some(item) = reg.construction.iter().find(|i| i.slot == slot) else { continue };
                let step = self.rules.seams.construction_step.rule()(self, reg, item, &mut writes);
                self.mod_state.apply(&mut writes);
                self.apply_construction_step(region, slot, step, events);
            }
        }
    }

    /// Applies one item's [`ConstructionStep`] to the region (`0x00B78800`, CONFIRMED): a waiting item is left
    /// as it is; a step stores the turns left and, for a repair, the building's new health
    /// ([`Self::set_building_health`], which fires `CampaignBuildingDamaged` below 100); a finished item leaves
    /// the slot (`0x00B78800` frees it) and its result stands: a build's level (`BuildingCompleted`, posted by
    /// the build steps `0x00B78230` / `0x00B78560`), a repair's health ([`Self::set_building_health`]: 100,
    /// no event), or a demolition's empty slot, whose school stops researching first (`0x00B781B0` →
    /// `0x00AA4BF0`: the technology's researcher cleared, its progress kept, as an owner change does).
    fn apply_construction_step(&mut self, region: RegionId, slot: super::SlotRef, step: ConstructionStep, events: &mut Vec<CampaignEvent>) {
        let Some(reg) = self.world.regions.get_mut(&region) else { return };
        let Some(index) = reg.construction.iter().position(|i| i.slot == slot) else { return };
        match step {
            ConstructionStep::Waits => {}
            ConstructionStep::Steps { turns_remaining, health } => {
                reg.construction[index].turns_remaining = turns_remaining.max(1);
                if let Some(h) = health {
                    self.set_building_health(region, slot, h, events);
                }
            }
            ConstructionStep::Finishes(result) => {
                reg.construction.remove(index);
                match result {
                    ConstructionResult::Built(level_key) => {
                        if let Some(b) = reg.building_mut(slot) {
                            *b = Some(BuildingRef { level_key, health: 100 });
                        }
                        events.push(CampaignEvent::BuildingCompleted { region, slot });
                    }
                    ConstructionResult::Repaired(health) => self.set_building_health(region, slot, health, events),
                    ConstructionResult::Demolished => {
                        let (holder, id) = match slot {
                            super::SlotRef::Slot(i) => reg.slots.get(i).map_or((None, 0), |s| (s.holder, s.id)),
                            _ => (None, 0),
                        };
                        let faction = holder.unwrap_or(reg.owner);
                        if let Some(b) = reg.building_mut(slot) {
                            *b = None;
                        }
                        if let Some(d) = self.world.faction_details.get_mut(&faction).filter(|_| id != 0) {
                            for t in d.research.values_mut().filter(|t| t.researcher == id) {
                                t.researcher = 0;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Sets the health of the building in `slot` (`SetBuildingHealth` `0x00B452A0`, CONFIRMED): below 100 it posts
    /// `CampaignBuildingDamaged` for the slot (vtable `0x0137CE30`). A slot with no building is left as it is.
    pub(crate) fn set_building_health(&mut self, region: RegionId, slot: super::SlotRef, health: u32, events: &mut Vec<CampaignEvent>) {
        let Some(Some(b)) = self.world.regions.get_mut(&region).and_then(|r| r.building_mut(slot)) else { return };
        b.health = health;
        if health < 100 {
            events.push(CampaignEvent::CampaignBuildingDamaged { region, slot });
        }
    }

    fn region_turn(&mut self, region: RegionId, events: &mut Vec<CampaignEvent>) {
        if !self.world.regions.contains_key(&region) {
            return;
        }
        let reg = &self.world.regions[&region];
        // The region update 0x00AAE820 (CONFIRMED order): the slots (`SlotTurnStart` each, 0x00AAE8D0, a port's naval
        // queue right after it), the land queue (0x00B71FB0), then `RegionTurnStart` (posted at its end).
        for slot in 0..reg.slots.len() as u32 {
            events.push(CampaignEvent::SlotTurnStart { region, slot });
        }

        // Recruitment (`0x00B71FB0` with force 0, from the region update `0x00AAE820`, CONFIRMED): first
        // the items whose unit the region can no longer recruit are removed through the cancel path
        // `0x00B1A820` with 1, so each is refunded like a cancel (CONFIRMED: the cancel command's call,
        // crediting the item's cost; [`super::treasury::credit`], saturating where the exe wraps). The cancel path
        // also gives the region its population back, per item (`0x00A61AA0`,
        // [`super::population::RecruitmentPopulation::credited`]).
        let recruitable = self.recruitable_units(region);
        // The items no longer recruitable, in queue order (no allocation when there are none).
        let removed: Vec<_> = match self.world.regions.get_mut(&region) {
            Some(r) => r.recruitment_queue.extract_if(.., |i| !recruitable.iter().any(|e| e.unit_key == i.unit_key)).collect(),
            None => Vec::new(),
        };
        self.cancelled_recruitment_items(region, &removed, |_, _| true);
        let reg = &self.world.regions[&region];
        // Capacity (the queue's method 1: `recruitment_points`), read before the queues change.
        let caps = (self.recruitment_points(region, false), self.recruitment_points(region, true));
        let naval: Vec<bool> = reg.recruitment_queue.iter().map(|i| self.rules.is_naval_unit(&i.unit_key)).collect();
        // An item whose recruitable entry carries building flags is held back (`0x00B5AD90`, CONFIRMED: the
        // entry of the item's unit in the region's unpriced list, vtable +0x14 = region +0x1A8, has a flag). The
        // list is the one the removal above read: the exe's step reads the same cache, which nothing inside
        // `0x00B71FB0` rebuilds, and which already holds this turn's finished buildings (the construction of the round end before).
        let blocked: Vec<bool> = reg
            .recruitment_queue
            .iter()
            .map(|i| recruitable.iter().find(|e| e.unit_key == i.unit_key).is_none_or(|e| e.flags != 0))
            .collect();

        // Then, in queue order, items count down (item method +0x18) until `recruitment_points` of
        // them have (CONFIRMED, `0x00B71FB0`); the rest wait. The land and naval queues are separate
        // managers with their own capacity. An item counts only after a round end has passed since it
        // was queued (flag +0x54, set by `0x00A78620` at the faction's round-end economy), which the
        // turn order here gives already. A held-back item neither counts down nor takes one of the
        // `recruitment_points`. Items reaching zero finish, in queue order.
        let reg = self.world.regions.get_mut(&region).expect("region exists");
        let mut finished = Vec::new();
        let mut waiting = Vec::with_capacity(reg.recruitment_queue.len());
        let (mut land_n, mut naval_n) = (0u32, 0u32);
        for ((mut item, is_naval), blocked) in std::mem::take(&mut reg.recruitment_queue).into_iter().zip(naval).zip(blocked) {
            let (n, cap) = if is_naval { (&mut naval_n, caps.1) } else { (&mut land_n, caps.0) };
            if !blocked && *n < cap {
                *n += 1;
                item.turns_remaining = item.turns_remaining.saturating_sub(1);
            }
            if item.turns_remaining == 0 {
                finished.push((item.unit_key, item.target, is_naval));
            } else {
                waiting.push(item);
            }
        }
        reg.recruitment_queue = waiting;
        // The finished items spawn through the queue manager's vtable +0x20 (land `0x00B1E500`, CONFIRMED):
        // first the land items recruited through a commander (item +0x18) who leads an army, one new army
        // per commander that marches to him ([`Self::spawn_reinforcement`]), then every other item
        // ([`Self::spawn_recruited_unit`]). The exe groups by (+0x18, +0x50); +0x50 is 0 for a player's
        // item (`HandleRecruitUnitCommand` passes 0 at `0x00936C55`) and an AI handle only for AI items
        // queued with no target (`0x00A9F126`), so grouping by commander is the exe's for every item with one.
        // PROVISIONAL: a commander with no army: the exe hands the item to the holder at his +0x2B0 when
        // that holder's vfunc +0x38 says so (`0x00B1E500` phase 1), ours spawns it as a settlement recruit
        // (the exe's other branch, +0x18 cleared). PROVISIONAL: the exe also spawns a group as settlement
        // recruits when its path query from the settlement to the commander fails (`0x00B6D8D0` /
        // `0x00B6D760`); ours always raises the army. PROVISIONAL: naval items recruited through an
        // admiral spawn as port recruits here. The naval spawn `0x00B1D7E0` groups its items by (+0x18,
        // +0x50) too and, when the target has a navy (his +0xE0) and the sea path query `0x00B6D760`
        // succeeds, raises one new fleet per group (`SpawnRecruitmentGroupAsNewArmy`) and returns the
        // (item, fleet) pairs to the queue step `0x00B71FB0`; that fleet, its spot and its march to the
        // admiral are not ported.
        let mut groups: Vec<(super::CharacterId, Vec<String>)> = Vec::new();
        let mut others = Vec::with_capacity(finished.len());
        // The commanders leading an army, looked up once for the pass (and only when a land item has a target).
        let army_leaders: std::collections::BTreeSet<super::CharacterId> = if finished.iter().any(|(_, t, naval)| t.is_some() && !naval) {
            self.world.forces.values().filter(|f| !f.is_navy).filter_map(|f| f.commander).collect()
        } else {
            std::collections::BTreeSet::new()
        };
        for (unit_key, target, is_naval) in finished {
            match target.filter(|t| !is_naval && army_leaders.contains(t)) {
                Some(t) => match groups.iter_mut().find(|g| g.0 == t) {
                    Some(g) => g.1.push(unit_key),
                    None => groups.push((t, vec![unit_key])),
                },
                None => others.push(unit_key),
            }
        }
        for (target, unit_keys) in groups {
            let ev = self.spawn_reinforcement(region, target, unit_keys);
            events.extend(ev);
        }
        for unit_key in others {
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

/// What one round does to a construction item ([`construction_step`], seam
/// [`construction.step`](super::seams::CONSTRUCTION_STEP)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructionStep {
    /// The item does not step this round: an inactive item (`0x00B7880E`), or a build or upgrade in a slot another
    /// faction holds (`0x00B78800`).
    Waits,
    /// The item steps and stays.
    Steps {
        /// The item's turns left after this step.
        turns_remaining: u32,
        /// For a repair, the standing building's new health (set through `0x00B452A0`).
        health: Option<u32>,
    },
    /// The item is finished and leaves the slot with this result.
    Finishes(ConstructionResult),
}

/// What a finished construction item leaves in its slot ([`ConstructionStep::Finishes`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructionResult {
    /// A new building or an upgrade: this level at full health.
    Built(String),
    /// A repair: the standing building at this health.
    Repaired(u32),
    /// A demolition: the slot is empty.
    Demolished,
}

/// The original game's step of one construction item at its region's round end (`StepSlotConstructionItem`
/// `0x00B78800`, CONFIRMED). An inactive item ([`ConstructionItem::active`] false) does not step
/// (`0x00B7880E`). A build or upgrade (ESF types 0 / 1) steps only when the region's owner holds the
/// slot; a repair and a demolition (types 2 / 3) step whoever holds it. Each step counts one turn
/// (`0x00B781A0`: elapsed + 1, done once the total is reached):
/// - a build finishes as its level at full health (`0x00B78230` / `0x00B78560`);
/// - a repair (`0x00B78490`) raises the standing building's health by its step each turn and finishes at 100
///   (for the walls the exe also restores the fortification's damage object, vtable +0xC0, not in the model);
/// - a demolition (`0x00B781B0`, one turn) finishes as an empty slot.
pub fn construction_step(_model: &CampaignModel, reg: &Region, item: &ConstructionItem) -> ConstructionStep {
    use super::ConstructionKind::{Build, Demolish, Repair};
    if !item.active || (item.kind == Build && !reg.slot_held(item.slot)) {
        return ConstructionStep::Waits;
    }
    let left = item.turns_remaining.saturating_sub(1);
    if left > 0 {
        let health = match item.kind {
            Repair { health_per_turn, .. } => Some(reg.building_at(item.slot).map_or(0, |b| b.health).saturating_add(health_per_turn)),
            Build | Demolish => None,
        };
        return ConstructionStep::Steps { turns_remaining: left, health };
    }
    ConstructionStep::Finishes(match item.kind {
        Build => ConstructionResult::Built(item.level_key.clone()),
        Repair { .. } => ConstructionResult::Repaired(100),
        Demolish => ConstructionResult::Demolished,
    })
}

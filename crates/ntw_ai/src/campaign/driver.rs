//! Plugs the campaign AI into the campaign turn loop (`ntw_sim::campaign::turn`).
//!
//! The turn loop queues a [`TurnStep::AiTurn`] for every non-human faction (after its economy,
//! region and character start phases, before its end phases). While that step is the next one,
//! the model accepts the faction's commands (`CampaignModel::may_act`), and this driver:
//! 1. builds the [`TurnContext`] (humans, script hints from `ntw_script::ScriptState`);
//! 2. calls [`take_turn`] with the campaign's own RNG (`CampaignModel::rng`, so the AI is part of
//!    the saved, deterministic state). The CAI has its own stream, `CampaignModel::cai_rng` (CAI
//!    interface +0x71C, CONFIRMED: `0x00A3CE70` / `0x00A5B5B0`), on which the model's AI capture
//!    choice (`0x00AAABA0`, `ntw_sim::campaign::capture_ai`) draws. PROVISIONAL: which stream
//!    `take_turn`'s draws (the BDI pool jitter, the recruitment draws) use in the original is
//!    untraced, so they stay on `CampaignModel::rng` (BACKLOG §6 "CAI turn draws' stream");
//! 3. applies the orders through the model's validated commands ([`apply_orders_with_events`]);
//!    a battle an order starts is autoresolved at once (PROVISIONAL, see there).
//!
//! Two ways in: [`end_turn`] for a bare `CampaignModel` (tests, tools; [`end_turn_declining_calls`]
//! when nobody plays the humans, [`end_host_turn_declining_calls`] on a script host), and [`install`], which
//! sets the `ntw_script::ScriptHost` AI hook so the original scripts see every event in order.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ntw_script::{FireReport, ScriptHost, ScriptState};
use ntw_sim::campaign::{CampaignCommand, CampaignEvent, CampaignModel, CommandError, FactionId, TurnStep};

use super::{AiOrder, CampaignAiData, TurnContext, apply_orders_with_events, take_turn};

/// What one AI faction did in one turn.
#[derive(Debug, Clone, PartialEq)]
pub struct AiTurnReport {
    /// The faction.
    pub faction: FactionId,
    /// Turn number (`Calendar::turn_number`) when it played.
    pub turn: u32,
    /// Every order the AI gave.
    pub orders: Vec<AiOrder>,
    /// How many the model accepted.
    pub accepted: usize,
    /// The orders the model refused, with why (the app logs each kind once).
    pub rejected: Vec<(AiOrder, CommandError)>,
}

/// A context for a bare model: the model's human factions, no script hints (the difficulty is in the model's
/// faction effects, see [`TurnContext::new`]).
pub fn context_for(model: &CampaignModel, campaign_key: &str) -> TurnContext {
    let mut ctx = TurnContext::new(campaign_key);
    ctx.humans.extend(model.turn.humans.iter().copied());
    ctx
}

/// A context from the scripts' state: campaign key, humans (the model's, plus the local faction)
/// and what the scripts told the AI (the invasion switch; `force_diplomacy` permissions and the
/// restricted building levels and units are the model's own).
pub fn context_from_script(state: &ScriptState) -> TurnContext {
    let mut ctx = context_for(&state.model, &state.campaign);
    ctx.humans.extend(state.faction_id(&state.local_faction));
    // `set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour(bool)` is
    // a logging stub in ntw_script; its last call in the log wins (INFERRED: a global switch).
    const INVADE: &str = "set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour(";
    if let Some(line) = state.log.iter().rev().find(|l| l.contains(INVADE)) {
        ctx.hints.invade_humans = line.contains(&format!("{INVADE}true"));
    }
    ctx
}

/// Plays AI faction `faction` now: decides and applies its orders. Returns the report and every
/// event the orders caused. Human factions are skipped (empty report), and so is every faction once the
/// campaign is over; an order that ends it drops the rest quietly (see [`apply_orders_with_events`],
/// which cites the exe).
pub fn play_faction(model: &mut CampaignModel, data: &CampaignAiData, ctx: &TurnContext, faction: FactionId) -> (AiTurnReport, Vec<CampaignEvent>) {
    let mut report = AiTurnReport { faction, turn: model.calendar.turn_number(), orders: Vec::new(), accepted: 0, rejected: Vec::new() };
    let mut events = Vec::new();
    // The campaign-over test also skips `take_turn` (no RNG draw, no orders), which the apply guard cannot.
    if ctx.humans.contains(&faction) || model.turn.campaign_over {
        return (report, events);
    }
    let mut rng = model.rng;
    report.orders = take_turn(model, data, ctx, faction, &mut rng);
    model.rng = rng;
    report.accepted = apply_orders_with_events(model, &report.orders, &mut events, &mut report.rejected);
    (report, events)
}

/// The AI faction whose turn step is next, if any.
pub fn next_ai_turn(model: &CampaignModel) -> Option<FactionId> {
    match model.turn.queue.front() {
        Some(TurnStep::AiTurn(f)) => Some(*f),
        _ => None,
    }
}

/// `CCQ_END_TURN` on a bare model with the campaign AI playing every AI faction (the model's own
/// `end_turn` leaves them idle). Returns every event in order and one report per AI turn. A call to war
/// waiting for a human ally (`CampaignModel::pending_ally_war_calls`) pauses the steps
/// (`CampaignModel::step`): the turn stops there (logged), and a later call does not end the turn
/// again until it is answered, through [`answer_ally_war_call`] (or a script host's `apply` of the
/// answer), which resumes the steps. On a model that has not started, a call raised during the
/// start-campaign steps stops it before the turn's end is queued: once that call is answered the
/// human is in their turn, and the next `end_turn` ends it. [`end_turn_declining_calls`] always
/// ends one turn.
pub fn end_turn(model: &mut CampaignModel, data: &CampaignAiData, ctx: &TurnContext) -> (Vec<CampaignEvent>, Vec<AiTurnReport>) {
    let mut play = |m: &mut CampaignModel, f| play_faction(m, data, ctx, f);
    let mut out = Steps::default();
    if !model.turn.started {
        model.begin_start_campaign();
        out.run(model, &mut play);
    }
    if !stopped_for_human(model) {
        model.begin_end_turn();
        out.run(model, &mut play);
        stopped_for_human(model);
    }
    (out.events, out.reports)
}

/// The human ally's answer to the top waiting call to war on a bare model (`RespondToAllyWarCall`,
/// `CampaignModel::respond_to_ally_war_call`), then the turn steps the call paused resume with the AI
/// playing, as in [`end_turn`] (a script host's `apply` of the answer does the same). Returns the
/// events and AI turn reports of the answer and the resumed steps.
pub fn answer_ally_war_call(
    model: &mut CampaignModel,
    data: &CampaignAiData,
    ctx: &TurnContext,
    side: Option<FactionId>,
    accept: bool,
) -> Result<(Vec<CampaignEvent>, Vec<AiTurnReport>), CommandError> {
    let mut out = Steps { events: model.respond_to_ally_war_call(side, accept)?, reports: Vec::new() };
    out.run(model, &mut |m: &mut CampaignModel, f| play_faction(m, data, ctx, f));
    stopped_for_human(model);
    Ok((out.events, out.reports))
}

/// [`end_turn`] for a tool or test with nobody playing the human factions: every call to war that
/// waits for a human ally is declined (as [`answer_ally_war_call`] with no side) and the steps it
/// paused resume, so one call always ends exactly one turn, on a model that has not started too (the
/// campaign starts, then its first turn ends). Calls left waiting by an earlier [`end_turn`] are
/// declined first, so the steps they paused finish before this turn's end is queued. Declining a
/// call never logs the "turn stops" warning; a refused decline (logged) leaves the turn paused there.
pub fn end_turn_declining_calls(model: &mut CampaignModel, data: &CampaignAiData, ctx: &TurnContext) -> (Vec<CampaignEvent>, Vec<AiTurnReport>) {
    end_turn_declining_with(model, &mut |m: &mut CampaignModel, f| play_faction(m, data, ctx, f))
}

/// [`end_turn_declining_calls`] with `play` giving each AI faction's turn (tests drive it with their own).
fn end_turn_declining_with(model: &mut CampaignModel, play: &mut PlayFaction<'_>) -> (Vec<CampaignEvent>, Vec<AiTurnReport>) {
    let mut out = Steps::default();
    let mut declined = out.decline_calls(model, play);
    if declined && !model.turn.started {
        model.begin_start_campaign();
        out.run(model, play);
        declined = out.decline_calls(model, play);
    }
    if declined {
        model.begin_end_turn();
        out.run(model, play);
        out.decline_calls(model, play);
    }
    (out.events, out.reports)
}

/// [`end_turn_declining_calls`] on a script host (its own AI hook, [`install`], plays the AI
/// factions and the scripts see every event): `EndTurn` through [`ScriptHost::apply`], declining each
/// waiting call to war with `RespondToAllyWarCall { side: None, accept: false }` (whose `apply`
/// resumes the paused steps), in the same order, so one call ends exactly one turn. Returns every
/// event with its script report, or the first command the host refused (a pending battle refuses
/// `EndTurn`: autoresolve it first).
pub fn end_host_turn_declining_calls(host: &mut ScriptHost) -> Result<Vec<(CampaignEvent, FireReport)>, CommandError> {
    fn decline(host: &mut ScriptHost, out: &mut Vec<(CampaignEvent, FireReport)>) -> Result<(), CommandError> {
        while !host.model().pending_ally_war_calls.is_empty() {
            out.extend(host.apply(CampaignCommand::RespondToAllyWarCall { side: None, accept: false })?);
        }
        Ok(())
    }
    let mut out = Vec::new();
    decline(host, &mut out)?;
    if !host.model().turn.started {
        out.extend(host.start_campaign());
        decline(host, &mut out)?;
    }
    out.extend(host.apply(CampaignCommand::EndTurn)?);
    decline(host, &mut out)?;
    Ok(out)
}

/// True (and logged) while a call to war waits for a human ally's answer, which pauses the turn steps.
fn stopped_for_human(model: &CampaignModel) -> bool {
    let Some(call) = model.pending_ally_war_calls.last() else { return false };
    log::warn!(
        "AI driver: the turn stops until {:?} answers the call to {:?}'s war on {:?} (RespondToAllyWarCall)",
        call.ally,
        call.declarer,
        call.target
    );
    true
}

/// Plays one AI faction's turn step: [`play_faction`] with the driver's data and context.
type PlayFaction<'a> = dyn FnMut(&mut CampaignModel, FactionId) -> (AiTurnReport, Vec<CampaignEvent>) + 'a;

/// What the turn steps run so far gave: every event in order and one report per AI turn.
#[derive(Default)]
struct Steps {
    events: Vec<CampaignEvent>,
    reports: Vec<AiTurnReport>,
}

impl Steps {
    /// Runs the queued turn steps, `play` giving each AI faction's turn when its step is next, until
    /// none is left or a waiting call to war pauses them.
    fn run(&mut self, model: &mut CampaignModel, play: &mut PlayFaction<'_>) {
        loop {
            if let Some(f) = next_ai_turn(model) {
                let (r, ev) = play(model, f);
                self.events.extend(ev);
                self.reports.push(r);
            }
            match model.step() {
                Some(ev) => self.events.extend(ev),
                None => break,
            }
        }
    }

    /// Declines every call to war waiting for a human ally, top first, running the steps each answer
    /// resumes. False if the model refused a decline (logged once here; the call keeps waiting).
    fn decline_calls(&mut self, model: &mut CampaignModel, play: &mut PlayFaction<'_>) -> bool {
        while let Some(call) = model.pending_ally_war_calls.last() {
            let (ally, declarer, target) = (call.ally, call.declarer, call.target);
            match model.respond_to_ally_war_call(None, false) {
                Ok(ev) => {
                    self.events.extend(ev);
                    self.run(model, play);
                }
                Err(e) => {
                    log::error!(
                        "AI driver: declining {ally:?}'s call to {declarer:?}'s war on {target:?} was refused ({e:?}); the turn stays paused there"
                    );
                    return false;
                }
            }
        }
        true
    }
}

/// Installs the campaign AI on a script host: from now on every AI faction's turn in the host's
/// turn loop is played by [`play_faction`], with the context rebuilt from the script state each
/// time. The factions' stored manager / personality keys and the regions' stored base values come
/// from the model (`World::ai_keys`, `World::region_base_values`).
/// Returns a shared log of the reports (newest last) for displays and tests.
pub fn install(host: &mut ScriptHost, data: Arc<CampaignAiData>) -> Rc<RefCell<Vec<AiTurnReport>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let out = log.clone();
    host.set_ai_turn_hook(Box::new(move |state: &mut ScriptState, faction| {
        let ctx = context_from_script(state);
        let (report, events) = play_faction(&mut state.model, &data, &ctx, faction);
        log.borrow_mut().push(report);
        events
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::calendar::{Calendar, Date, HALF_EARLY};
    use ntw_sim::campaign::{Faction, GovernmentType, World};
    use std::collections::BTreeMap;

    const A: FactionId = FactionId(1);
    const B: FactionId = FactionId(2);
    const C: FactionId = FactionId(3);

    /// A and B played by the AI, C by a human and allied to B; turn order A, B, C.
    fn model() -> CampaignModel {
        let mut w = World::default();
        for (id, key) in [(A, "test_faction_a"), (B, "test_faction_b"), (C, "test_faction_c")] {
            let government = GovernmentType::AbsoluteMonarchy;
            let tax = || "tax_normal".to_owned();
            w.factions.insert(
                id,
                Faction { id, key: key.into(), treasury: 0, government, government_key: String::new(), tax_lower: tax(), tax_upper: tax(), diplomacy: BTreeMap::new() },
            );
            let d = w.faction_details.entry(id).or_default();
            d.major = Some(true);
            d.out_of_game = Some(false);
        }
        w.turn_order = vec![A, B, C];
        let mut m = CampaignModel::new(Calendar::new(Date { year: 1805, season: 1, month: 0, half: HALF_EARLY }, 0), ntw_sim::rng::CaRng::new(1), w);
        m.turn.humans = vec![C];
        m.form_alliance(C, B).unwrap();
        m
    }

    /// An AI that does nothing, except that A declares war on B (which asks the human ally C) in its
    /// first turn when `declare` is set.
    fn play(declare: bool) -> impl FnMut(&mut CampaignModel, FactionId) -> (AiTurnReport, Vec<CampaignEvent>) {
        let mut declared = !declare;
        move |m, f| {
            let report = AiTurnReport { faction: f, turn: m.calendar.turn_number(), orders: Vec::new(), accepted: 0, rejected: Vec::new() };
            let mut events = Vec::new();
            if f == A && !declared {
                declared = true;
                events = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
                assert!(!m.pending_ally_war_calls.is_empty(), "the human ally is asked");
            }
            (report, events)
        }
    }

    /// Where a model stands: the turn number, whose turn it is, whether it is in that turn, and whether
    /// steps are left.
    fn state(m: &CampaignModel) -> (u32, Option<FactionId>, bool, bool) {
        (m.calendar.turn_number(), m.turn.current, m.turn.in_turn, m.has_pending_steps())
    }

    /// Review (allyattacked): on a model that had not started, a call raised during the start-campaign
    /// steps made the turn's end never queued, so the turn did not end. One call ends exactly one turn.
    #[test]
    fn declining_on_an_unstarted_model_ends_one_turn() {
        let mut quiet = model();
        end_turn_declining_with(&mut quiet, &mut play(false));
        let mut m = model();
        let (events, reports) = end_turn_declining_with(&mut m, &mut play(true));
        assert!(m.pending_ally_war_calls.is_empty(), "the call was declined");
        assert!(events.iter().any(|e| matches!(e, CampaignEvent::AllyWarCallPending { ally: C, .. })));
        assert_eq!(state(&m), state(&quiet), "the same turn as without the call");
        assert_eq!(state(&m), (2, Some(C), true, false), "the campaign started and its first turn ended");
        assert_eq!(reports.len(), 4, "A and B played in the start steps and again after the turn's end");
    }

    /// The same on a started model: the call is raised in the turn's own steps.
    #[test]
    fn declining_on_a_started_model_ends_one_turn() {
        let mut quiet = model();
        let mut m = model();
        for m in [&mut quiet, &mut m] {
            m.begin_start_campaign();
            Steps::default().run(m, &mut play(false));
        }
        assert_eq!(state(&m), (1, Some(C), true, false));
        end_turn_declining_with(&mut quiet, &mut play(false));
        let (events, _) = end_turn_declining_with(&mut m, &mut play(true));
        assert!(events.iter().any(|e| matches!(e, CampaignEvent::AllyWarCallPending { ally: C, .. })));
        assert!(m.pending_ally_war_calls.is_empty());
        assert_eq!(state(&m), state(&quiet));
        assert_eq!(state(&m), (2, Some(C), true, false));
        // The next call ends the next turn (A and B are already at war, so no new call).
        end_turn_declining_with(&mut m, &mut play(false));
        assert_eq!(state(&m), (3, Some(C), true, false));
    }
}

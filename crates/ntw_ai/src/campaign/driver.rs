//! Plugs the campaign AI into the campaign turn loop (`ntw_sim::campaign::turn`).
//!
//! The turn loop queues a [`TurnStep::AiTurn`] for every non-human faction (after its economy,
//! region and character start phases, before its end phases). While that step is the next one,
//! the model accepts the faction's commands (`CampaignModel::may_act`), and this driver:
//! 1. builds the [`TurnContext`] (humans, script hints from `ntw_script::ScriptState`);
//! 2. calls [`take_turn`] with the campaign's own RNG (`CampaignModel::rng`, so the AI is part of
//!    the saved, deterministic state; PROVISIONAL: which RNG the original's CAI draws from is
//!    UNKNOWN);
//! 3. applies the orders through the model's validated commands ([`apply_orders_with_events`]);
//!    a battle an order starts is autoresolved at once (PROVISIONAL, see there).
//!
//! Two ways in: [`end_turn`] for a bare `CampaignModel` (tests, tools), and [`install`], which
//! sets the `ntw_script::ScriptHost` AI hook so the original scripts see every event in order.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ntw_script::{ScriptHost, ScriptState};
use ntw_sim::campaign::{CampaignEvent, CampaignModel, CommandError, FactionId, TurnStep};

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
/// and what the scripts told the AI (`add_restricted_unit_record` and the invasion switch;
/// `force_diplomacy` permissions and the restricted building levels are the model's own).
pub fn context_from_script(state: &ScriptState) -> TurnContext {
    let mut ctx = context_for(&state.model, &state.campaign);
    ctx.humans.extend(state.faction_id(&state.local_faction));
    ctx.hints.restricted_units = state.restricted_units.clone();
    // `set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour(bool)` is
    // a logging stub in ntw_script; its last call in the log wins (INFERRED: a global switch).
    const INVADE: &str = "set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour(";
    if let Some(line) = state.log.iter().rev().find(|l| l.contains(INVADE)) {
        ctx.hints.invade_humans = line.contains(&format!("{INVADE}true"));
    }
    ctx
}

/// Plays AI faction `faction` now: decides and applies its orders. Returns the report and every
/// event the orders caused. Human factions are skipped (empty report).
pub fn play_faction(model: &mut CampaignModel, data: &CampaignAiData, ctx: &TurnContext, faction: FactionId) -> (AiTurnReport, Vec<CampaignEvent>) {
    let mut report = AiTurnReport { faction, turn: model.calendar.turn_number(), orders: Vec::new(), accepted: 0, rejected: Vec::new() };
    let mut events = Vec::new();
    if ctx.humans.contains(&faction) {
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
/// `end_turn` leaves them idle). Returns every event in order and one report per AI turn.
pub fn end_turn(model: &mut CampaignModel, data: &CampaignAiData, ctx: &TurnContext) -> (Vec<CampaignEvent>, Vec<AiTurnReport>) {
    let mut events = Vec::new();
    let mut reports = Vec::new();
    if !model.turn.started {
        model.begin_start_campaign();
        run_pending(model, data, ctx, &mut events, &mut reports);
    }
    model.begin_end_turn();
    run_pending(model, data, ctx, &mut events, &mut reports);
    (events, reports)
}

fn run_pending(model: &mut CampaignModel, data: &CampaignAiData, ctx: &TurnContext, events: &mut Vec<CampaignEvent>, reports: &mut Vec<AiTurnReport>) {
    loop {
        if let Some(f) = next_ai_turn(model) {
            let (r, ev) = play_faction(model, data, ctx, f);
            events.extend(ev);
            reports.push(r);
        }
        match model.step() {
            Some(ev) => events.extend(ev),
            None => break,
        }
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

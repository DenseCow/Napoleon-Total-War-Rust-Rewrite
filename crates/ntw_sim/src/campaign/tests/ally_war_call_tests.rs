//! Campaign model tests: a human ally's call to war (`DeclareWarAndCallAllies` `0x00B268B0` → the
//! `pending_diplomacy` offer, answered by `CCQ_RESPOND_TO_ALLY_WAR_INVITATION` `0x009370F0` → `0x008BFA80`).

use super::*;
use std::collections::VecDeque;

const D: FactionId = FactionId(4);

/// A, B, C and D in the game, every one a major power; `humans` plays.
fn model(humans: &[FactionId]) -> CampaignModel {
    let mut m = test_model();
    m.world.factions.insert(D, super::faction(D, "test_faction_d", 0, GovernmentType::AbsoluteMonarchy));
    for f in [A, B, C, D] {
        let d = m.world.faction_details.entry(f).or_default();
        d.major = Some(true);
        d.out_of_game = Some(false);
    }
    m.world.turn_order = vec![B, A, C, D];
    m.turn.humans = humans.to_vec();
    m
}

fn pending(ev: &[CampaignEvent]) -> Vec<CampaignEvent> {
    ev.iter().filter(|e| matches!(e, CampaignEvent::AllyWarCallPending { .. })).cloned().collect()
}

fn answer(m: &mut CampaignModel, side: Option<FactionId>, accept: bool) -> Vec<CampaignEvent> {
    m.apply(CampaignCommand::RespondToAllyWarCall { side, accept }).unwrap()
}

/// The attacked faction's human ally not yet at war is asked (offer built at `0x00B26C06`, pushed at
/// `0x00B26C1F`, the UI event at `0x00B26C6A`) and the declaration stops there: no war, no notices yet.
#[test]
fn the_attacked_factions_human_ally_is_asked_and_the_war_waits() {
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    let ev = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(
        pending(&ev),
        [CampaignEvent::AllyWarCallPending { ally: C, declarer: A, target: B, allied_to_declarer: false, asked_by_target_side: true }]
    );
    assert_ne!(m.world.stance(A, B), Stance::War, "the war waits for the answer");
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::Message { message: CampaignMessage::WarDeclared { .. }, .. })));
    let call = &m.pending_ally_war_calls[0];
    assert_eq!((call.ally, call.allied_to_target, call.allied_to_declarer), (C, true, false));
    // The script name list leaves the project event out.
    assert_eq!(ev.iter().find(|e| matches!(e, CampaignEvent::AllyWarCallPending { .. })).unwrap().script_name(), None);
}

/// Accepted for the attacked faction (`0x008BFA80`): the ally joins (its war on the declarer, the access
/// saved for the war, `0x00B26700` / `0x00B0CCE0`), then the declaration resumes and the war is declared.
#[test]
fn an_accepted_call_joins_then_the_war_is_declared() {
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let ev = answer(&mut m, Some(B), true);
    assert!(m.pending_ally_war_calls.is_empty());
    assert_eq!(m.world.stance(C, A), Stance::War);
    assert_eq!(m.world.stance(A, B), Stance::War);
    assert!(matches!(m.world.stance(C, B), Stance::Allied));
    for (x, y) in [(B, C), (C, B)] {
        assert_eq!(m.world.relationships[&(x, y)].military_access_turns, -1, "{x:?}->{y:?} access saved for the war");
    }
    let wars: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::StanceChanged { a, b, stance: Stance::War } => Some((*a, *b)),
            _ => None,
        })
        .collect();
    assert_eq!(wars, [(C, A), (A, B)], "the ally's war comes first, as for an AI ally");
    // C is in the lists, so it gets no `war_declared` notice; D does.
    let notices: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::Message { recipient, message: CampaignMessage::WarDeclared { .. } } => Some(*recipient),
            _ => None,
        })
        .collect();
    assert_eq!(notices, [D]);
}

/// Refused as the attacked faction's ally: listed as broken with it (`0x00B27500`) and the alliance broken
/// (`0x00B13840`); the war is declared without it.
#[test]
fn a_refused_call_from_the_attacked_side_breaks_the_alliance() {
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    answer(&mut m, None, false);
    assert!(m.pending_ally_war_calls.is_empty());
    assert!(!matches!(m.world.stance(C, B), Stance::Allied | Stance::Patron | Stance::Protectorate));
    assert_ne!(m.world.stance(C, A), Stance::War);
    assert_eq!(m.world.stance(A, B), Stance::War);
}

/// The flags the `ally_attacked` message shows (`0x00A08B90`): `InvitedByAggressor` / `InvitedByDefender`
/// from the +0xB10 signal (`0x00B26B80` / `0x00B26D79`), `AlliedWithAggressor` / `AlliedWithDefender` from
/// the ally's stances (`0x00B0CDF0`). An ally of the target alone; of the declarer alone; of both (asked by
/// the target's loop, invited by both).
#[test]
fn the_message_flags_follow_the_loop_that_asks() {
    let flags = |m: &CampaignModel| {
        let c = m.pending_ally_war_calls.last().unwrap();
        (c.invited_by_declarer(), c.invited_by_target(), m.allied_like_with(c.ally, c.declarer), m.allied_like_with(c.ally, c.target))
    };
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(flags(&m), (false, true, false, true), "the target's ally");
    let mut m = model(&[C]);
    m.form_alliance(C, A).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(flags(&m), (true, false, true, false), "the declarer's ally");
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    m.form_alliance(C, A).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(flags(&m), (true, true, true, true), "an ally of both");
}

/// The declarer's human ally is asked by the declarer's loop (`0x00B26DFE`, UI event flag B = 0); a refusal
/// only lists it (`0x00B0DB40`) and it keeps its alliance.
#[test]
fn the_declarers_human_ally_keeps_its_alliance_when_it_refuses() {
    let mut m = model(&[C]);
    m.form_alliance(C, A).unwrap();
    let ev = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(
        pending(&ev),
        [CampaignEvent::AllyWarCallPending { ally: C, declarer: A, target: B, allied_to_declarer: true, asked_by_target_side: false }]
    );
    answer(&mut m, None, false);
    assert!(matches!(m.world.stance(C, A), Stance::Allied));
    assert_ne!(m.world.stance(C, B), Stance::War);
    assert_eq!(m.world.stance(A, B), Stance::War);
}

/// An ally of both sides is asked once, by the attacked side's loop, for both (`0x008CE390` sets +0x61):
/// an acceptance must name a side, one it is not asked for is refused and the call keeps waiting; joining
/// the declarer breaks the alliance with the target and lists it for both, so the declarer's loop does not
/// ask again (`0x00B67380`).
#[test]
fn an_ally_of_both_sides_is_asked_once() {
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    m.form_alliance(C, A).unwrap();
    let ev = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(
        pending(&ev),
        [CampaignEvent::AllyWarCallPending { ally: C, declarer: A, target: B, allied_to_declarer: true, asked_by_target_side: true }]
    );
    for side in [None, Some(D), Some(C)] {
        assert!(m.apply(CampaignCommand::RespondToAllyWarCall { side, accept: true }).is_err(), "{side:?}");
        assert_eq!(m.pending_ally_war_calls.len(), 1, "the call keeps waiting");
    }
    let ev = answer(&mut m, Some(A), true);
    assert!(pending(&ev).is_empty(), "not asked again by the declarer's loop");
    assert!(m.pending_ally_war_calls.is_empty());
    assert_eq!(m.world.stance(C, B), Stance::War);
    assert!(matches!(m.world.stance(C, A), Stance::Allied));
    assert_eq!(m.world.stance(A, B), Stance::War);
}

/// Each human ally is asked in turn: the answer resumes the declaration (`0x008BFA80` → `0x00B268B0` with
/// the offer's lists), which asks the next one; the war comes after the last answer.
#[test]
fn the_next_human_ally_is_asked_after_the_answer() {
    let mut m = model(&[C, D]);
    m.form_alliance(C, B).unwrap();
    m.form_alliance(D, B).unwrap();
    let ev = m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert!(matches!(pending(&ev)[..], [CampaignEvent::AllyWarCallPending { ally: C, .. }]));
    let ev = answer(&mut m, Some(B), true);
    assert!(matches!(pending(&ev)[..], [CampaignEvent::AllyWarCallPending { ally: D, .. }]));
    assert_ne!(m.world.stance(A, B), Stance::War);
    assert_eq!(m.pending_ally_war_calls[0].called.joined_b, [C], "the lists so far go with the call");
    answer(&mut m, None, false);
    assert!(m.pending_ally_war_calls.is_empty());
    assert_eq!(m.world.stance(A, B), Stance::War);
    assert_eq!(m.world.stance(C, A), Stance::War);
    assert!(!matches!(m.world.stance(D, B), Stance::Allied));
}

/// An answer with no call waiting is refused.
#[test]
fn an_answer_with_no_call_is_refused() {
    let mut m = model(&[C]);
    assert!(m.apply(CampaignCommand::RespondToAllyWarCall { side: Some(B), accept: true }).is_err());
}

/// A waiting call pauses the turn steps (`0x008F4120` does not advance while the pending stack is not
/// empty, `0x008F41F9` / `0x008F457A`): the queue is kept and resumes after the answer. An AI faction's
/// turn step, whose orders asked the human, still finishes (it is not played again).
#[test]
fn a_waiting_call_pauses_the_turn_steps() {
    let mut m = model(&[A, C]);
    m.form_alliance(C, B).unwrap();
    m.start_campaign();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.begin_end_turn();
    let queued = m.turn.queue.clone();
    assert!(m.step().is_none());
    assert_eq!(m.turn.queue, queued, "the queue is kept");
    // An AI turn's step runs while a call waits.
    let mut ai = m.clone();
    ai.turn.queue = VecDeque::from([TurnStep::AiTurn(B)]);
    assert!(ai.step().is_some());
    answer(&mut m, Some(B), true);
    assert!(m.step().is_some(), "the steps go on after the answer");
}

/// The waiting call is part of the state hash.
#[test]
fn a_waiting_call_changes_the_state_hash() {
    let mut m = model(&[C]);
    m.form_alliance(C, B).unwrap();
    let before = m.state_hash();
    let mut asked = m.clone();
    asked.declare_war_rules(A, B).unwrap();
    let mut no_call = asked.clone();
    no_call.pending_ally_war_calls.clear();
    assert_ne!(asked.state_hash(), no_call.state_hash());
    assert_ne!(before, asked.state_hash());
}

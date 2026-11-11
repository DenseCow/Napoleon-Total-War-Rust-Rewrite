//! Campaign model tests: the victory check (`0x0096CD10`, [`CampaignModel::victory_check`]), the capture
//! choice it holds back behind the victory screen (the report's +0x9C) and `ContinueGameSelected`
//! (`0x00932B40`), on the MADE-UP world with MADE-UP movie rows and MADE-UP victory states (a test rule on
//! the `campaign.victory_state` seam: the original's is PROVISIONAL and never decides).

use super::*;
use super::capture::CaptureChoice;
use super::outcome::{MovieEventRow, MovieExit, VictoryState};

/// Replaces the victory-state rule of `m`: the listed factions' states, every other faction undecided.
fn with_states(m: &mut CampaignModel, states: &[(FactionId, VictoryState)]) {
    let states = states.to_vec();
    let maker: crate::seam::Maker<super::seams::VictoryStateRule> = Arc::new(move |_original| {
        let states = states.clone();
        Arc::new(move |_: &CampaignModel, f: FactionId| states.iter().find(|(s, _)| *s == f).map_or(VictoryState::Undecided, |&(_, s)| s))
    });
    let mut rules = (*m.rules).clone();
    rules.seams.victory_state.apply("test_mod.victory_state", &maker);
    m.rules = Arc::new(rules);
}

/// [`capture_model`] with a victory movie row and a defeat movie row for A (id 7) and for B (id 8).
fn model(humans: Vec<FactionId>) -> CampaignModel {
    let mut m = capture_model(humans);
    let row = |event: &str, movie: &str, id: &str| MovieEventRow { event: event.into(), culture: "european".into(), movie: movie.into(), id: id.into() };
    let mut rules = (*m.rules).clone();
    rules.movie_events = vec![row("game_victory_absolute_monachy", "won.bik", ""), row("game_lost_a", "a_lost.bik", "7"), row("game_lost_b", "b_lost.bik", "8")];
    rules.faction_movie_ids.insert("test_faction_a".into(), "7".into());
    rules.faction_movie_ids.insert("test_faction_b".into(), "8".into());
    m.rules = Arc::new(rules);
    m
}

/// One update with no work of its own: the events its end ([`CampaignModel::end_update`]) posts.
fn update(m: &mut CampaignModel) -> Vec<CampaignEvent> {
    let mut events = Vec::new();
    m.end_update(&mut events);
    events
}

/// The outcome movies in `events`: (faction, movie file, exit).
fn movies(events: &[CampaignEvent]) -> Vec<(FactionId, &str, MovieExit)> {
    events
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::OutcomeMovie { faction, movie } => Some((*faction, movie.movie.as_str(), movie.exit)),
            _ => None,
        })
        .collect()
}

fn lost(events: &[CampaignEvent]) -> Vec<FactionId> {
    events
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::CampaignLost { faction } => Some(*faction),
            _ => None,
        })
        .collect()
}

/// The original's rule is PROVISIONAL (the victory test `0x0096FCE0` is not ported): it never decides,
/// so the check posts nothing and gives no verdict.
#[test]
fn the_original_rule_never_decides() {
    let mut m = model(vec![A]);
    assert_eq!(m.victory_state(A), VictoryState::Undecided);
    assert_eq!(update(&mut m), Vec::new());
    assert!(!m.turn.victory_checked && !m.turn.campaign_over);
}

/// A single player's win: its victory movie with the victory popup as exit (event +0x120 with flag 0),
/// the campaign goes on (no `SetCampaignOverFlag`), and the verdict is given once (the check's +0x38).
#[test]
fn a_single_players_win_plays_its_victory_movie_once() {
    let mut m = model(vec![A]);
    with_states(&mut m, &[(A, VictoryState::Won), (B, VictoryState::Lost)]);
    let ev = m.apply(CampaignCommand::ContinueGameSelected).unwrap();
    assert_eq!(movies(&ev), vec![(A, "won.bik", MovieExit::ShowVictoryOptionsPopup)]);
    assert_eq!(lost(&ev), Vec::new(), "B is no human: the check never asks its state");
    assert!(m.turn.victory_checked && !m.turn.campaign_over);
    assert_eq!(update(&mut m), Vec::new(), "the verdict is given once");
    assert_eq!(m.apply(CampaignCommand::ContinueGameSelected).unwrap(), Vec::new());
}

/// With the notification turned off (`set_ui_notification_of_victory_disabled`, +0x125 read at `0x0096CE9C`) the
/// winner gets no event, but the verdict is given and the capture report is still held.
#[test]
fn a_win_with_the_notification_off_posts_nothing_but_is_the_verdict() {
    let mut m = model(vec![A]);
    m.world.victory_notification_disabled = true;
    with_states(&mut m, &[(A, VictoryState::Won)]);
    let ev = m.capture_by_force(RegionId(11), A, Some(ForceId(1000)));
    assert_eq!(movies(&ev), Vec::new());
    assert!(m.turn.victory_checked && !m.turn.campaign_over);
    assert!(m.pending_capture.as_ref().is_some_and(|p| p.held), "the hold comes before the notification test (0x0096CE7C)");
}

/// With another human in the game the win ends the campaign: the first other human of the faction list
/// gets the defeat event first (+0x108 with flag 1: its defeat movie), then the winner its victory with
/// the "campaign over" flag (the movie exits to the main menu), and the campaign is over.
#[test]
fn a_win_against_another_human_ends_the_campaign() {
    let mut m = model(vec![A, B]);
    with_states(&mut m, &[(A, VictoryState::Won)]);
    let ev = update(&mut m);
    assert_eq!(movies(&ev), vec![(B, "b_lost.bik", MovieExit::ExitGame), (A, "won.bik", MovieExit::ExitGame)]);
    assert_eq!(lost(&ev), Vec::new(), "B is undecided: no lost event");
    assert!(m.turn.victory_checked && m.turn.campaign_over);
    assert_eq!(update(&mut m), Vec::new());
}

/// A winner and a loser in one check: the loser's lost event (+0x138) comes after the winners' events.
#[test]
fn a_loser_is_told_after_the_winner() {
    let mut m = model(vec![A, B]);
    with_states(&mut m, &[(A, VictoryState::Lost), (B, VictoryState::Won)]);
    let ev = update(&mut m);
    assert_eq!(movies(&ev), vec![(A, "a_lost.bik", MovieExit::ExitGame), (B, "won.bik", MovieExit::ExitGame)]);
    assert_eq!(ev.last(), Some(&CampaignEvent::CampaignLost { faction: A }));
    assert_eq!(ev.len(), 3);
}

/// A single player who failed the conditions is told once (event +0x138) and the verdict is given; the
/// campaign over flag is not set by the check.
#[test]
fn a_single_players_loss_is_told_once() {
    let mut m = model(vec![A]);
    with_states(&mut m, &[(A, VictoryState::Lost)]);
    let ev = update(&mut m);
    assert_eq!(ev, vec![CampaignEvent::CampaignLost { faction: A }]);
    assert!(m.turn.victory_checked && !m.turn.campaign_over);
    assert_eq!(update(&mut m), Vec::new());
}

/// With no winner, while another human is undecided the check posts nothing and gives no verdict (the
/// exe leaves for its clean-up at `0x0096CE4F` / `0x0096CE57`, past the loser loop and the done byte);
/// once nobody is undecided the losers are told once, in faction-list order (B, A, C), with the verdict.
#[test]
fn a_loser_is_not_told_while_another_human_is_undecided() {
    let mut m = model(vec![A, B]);
    with_states(&mut m, &[(A, VictoryState::Lost)]);
    for _ in 0..2 {
        assert_eq!(update(&mut m), Vec::new());
        assert!(!m.turn.victory_checked);
    }
    with_states(&mut m, &[(A, VictoryState::Lost), (B, VictoryState::Lost)]);
    assert_eq!(lost(&update(&mut m)), vec![B, A]);
    assert!(m.turn.victory_checked && !m.turn.campaign_over);
    assert_eq!(update(&mut m), Vec::new(), "told once");
}

/// A human id with no faction in the world is in no faction list: the check never asks it, so it
/// neither wins, loses nor keeps the others' verdict open.
#[test]
fn a_human_without_a_faction_is_never_asked() {
    let ghost = FactionId(9_999);
    let mut m = model(vec![A, ghost]);
    assert!(!m.world.factions.contains_key(&ghost));
    with_states(&mut m, &[(A, VictoryState::Lost), (ghost, VictoryState::Won)]);
    let ev = update(&mut m);
    assert_eq!(ev, vec![CampaignEvent::CampaignLost { faction: A }]);
    assert!(m.turn.victory_checked && !m.turn.campaign_over);
}

/// The capture that wins the campaign: its choice is held back behind the victory screen (the report's
/// +0x9C, set at `0x0096CE7C`), refused while held, and offered when the player continues
/// (`0x00932B40` clears the flag).
#[test]
fn the_winning_captures_choice_waits_for_continue() {
    let mut m = model(vec![A]);
    with_states(&mut m, &[(A, VictoryState::Won)]);
    let ev = m.capture_by_force(RegionId(11), A, Some(ForceId(1000)));
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CaptureChoicePending { .. })), "{ev:?}");
    assert_eq!(movies(&ev), vec![(A, "won.bik", MovieExit::ShowVictoryOptionsPopup)]);
    assert!(m.pending_capture.as_ref().is_some_and(|p| p.held));
    let before = m.state_hash();
    assert!(m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).is_err(), "no choice while held");
    assert_eq!(m.state_hash(), before, "a refused choice changes nothing");
    assert_eq!(update(&mut m), Vec::new(), "the hold stays until the player continues");
    let ev = m.apply(CampaignCommand::ContinueGameSelected).unwrap();
    assert_eq!(ev, vec![CampaignEvent::CaptureChoicePending { region: RegionId(11), faction: A }]);
    assert!(m.pending_capture.as_ref().is_some_and(|p| !p.held));
    assert_ne!(m.state_hash(), before, "the hold is state");
    assert_eq!(m.apply(CampaignCommand::ContinueGameSelected).unwrap(), Vec::new(), "nothing held: nothing happens");
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CaptureResolved { choice: CaptureChoice::Occupy, .. })), "{ev:?}");
}

/// A choice already offered when the campaign is won is not held: the report's step returns at once
/// when its pending byte +0x98 is clear (`0x008F8D00`), whatever +0x9C says.
#[test]
fn a_choice_already_offered_is_not_held() {
    let mut m = model(vec![A]);
    let ev = m.capture_by_force(RegionId(11), A, Some(ForceId(1000)));
    assert!(ev.contains(&CampaignEvent::CaptureChoicePending { region: RegionId(11), faction: A }));
    with_states(&mut m, &[(A, VictoryState::Won)]);
    let ev = update(&mut m);
    assert_eq!(movies(&ev), vec![(A, "won.bik", MovieExit::ShowVictoryOptionsPopup)]);
    assert!(m.pending_capture.as_ref().is_some_and(|p| !p.held));
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Loot }).unwrap();
}

/// The scripts' looting switch (+0x114) overrides the hold: the report's step tests +0x9C only while
/// the switch is clear (`0x008F8D32`), so a held capture is occupied at the next update.
#[test]
fn a_held_capture_is_occupied_once_looting_is_disabled() {
    let mut m = model(vec![A]);
    with_states(&mut m, &[(A, VictoryState::Won)]);
    m.capture_by_force(RegionId(11), A, Some(ForceId(1000)));
    assert!(m.pending_capture.as_ref().is_some_and(|p| p.held));
    m.world.looting_options_disabled_for_human = true;
    let ev = update(&mut m);
    assert!(m.pending_capture.is_none());
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CaptureResolved { region: RegionId(11), faction: A, choice: CaptureChoice::Occupy, .. })), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CaptureChoicePending { .. })));
}

/// The verdict is state a save keeps and the lockstep hash covers; unset, it leaves the hash as it was.
#[test]
fn the_verdict_is_hashed_only_when_given() {
    let mut m = model(vec![A]);
    let before = m.state_hash();
    assert_eq!(update(&mut m), Vec::new());
    assert_eq!(m.state_hash(), before);
    m.turn.victory_checked = true;
    assert_ne!(m.state_hash(), before);
}

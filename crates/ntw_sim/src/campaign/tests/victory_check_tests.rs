//! Campaign model tests: the victory check (`0x0096CD10`, [`CampaignModel::victory_check`]), the capture
//! choice it holds back behind the victory screen (the report's +0x9C) and `ContinueGameSelected`
//! (`0x00932B40`), on the MADE-UP world with MADE-UP movie rows; the check's tests use MADE-UP victory
//! states (a test rule on the `campaign.victory_state` seam), the victory test's (`0x0096FCE0`, the
//! seam's original rule) MADE-UP conditions records.

use super::*;
use super::capture::CaptureChoice;
use super::events::CampaignMessage;
use super::features::VictoryRival;
use super::outcome::{MovieEventRow, MovieExit, VictoryConditions, VictoryState, KIND_NONE, NO_DATE};

/// Replaces the victory-state rule of `m`: the listed factions' states, every other faction undecided.
fn with_states(m: &mut CampaignModel, states: &[(FactionId, VictoryState)]) {
    let states = states.to_vec();
    let maker: crate::seam::Maker<super::seams::VictoryStateRule> = Arc::new(move |_original| {
        let states = states.clone();
        Arc::new(move |_: &mut CampaignModel, f: FactionId, _: &mut Vec<CampaignEvent>, _: &mut super::mod_state::ModWrites| {
            states.iter().find(|(s, _)| *s == f).map_or(VictoryState::Undecided, |&(_, s)| s)
        })
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

/// A MADE-UP conditions record of type 1: hold `regions` and reach `count`, no deadline, no hold.
fn conditions(regions: &[u32], count: u32) -> VictoryConditions {
    VictoryConditions { regions: regions.iter().map(|&r| RegionId(r)).collect(), count, kind: 1, unknown_2: 24, ..Default::default() }
}

/// One run of the original rule for `f`: its state and the events it posts.
fn state(m: &mut CampaignModel, f: FactionId) -> (VictoryState, Vec<CampaignEvent>) {
    let mut events = Vec::new();
    let s = m.victory_state(f, &mut events);
    (s, events)
}

/// The events of conditions met with the notification on: the met event, then the `campaign_won` message.
fn met_events(f: FactionId) -> Vec<CampaignEvent> {
    vec![CampaignEvent::VictoryConditionMet { faction: f }, CampaignEvent::Message { recipient: f, message: CampaignMessage::CampaignWon }]
}

fn set_features(m: &mut CampaignModel, edit: impl FnOnce(&mut super::features::CampaignFeatures)) {
    let mut rules = (*m.rules).clone();
    edit(&mut rules.features);
    m.rules = Arc::new(rules);
}

/// No record, or one of type 6 (`0x008F3DF0` skips it): undecided and untested, so the check posts
/// nothing and gives no verdict. A type-1 record with the same (empty) data is met at once.
#[test]
fn a_faction_without_conditions_is_never_decided() {
    let mut m = model(vec![A]);
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()));
    m.world.victory_conditions.insert(A, VictoryConditions::default());
    assert_eq!(m.world.victory_conditions[&A].kind, KIND_NONE);
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()));
    assert_eq!(update(&mut m), Vec::new());
    assert!(!m.turn.victory_checked && !m.turn.campaign_over);
    m.world.victory_conditions.get_mut(&A).expect("inserted").kind = 1;
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)));
}

/// A deadline before the turn's date (`0x00898E90`, year then month then half) fails the conditions
/// once (`0x0096FCE0`'s +0x32 and its failed event); the deadline's own turn does not; no date never
/// fails. Through the check the single player is then told he lost.
#[test]
fn a_passed_deadline_fails_the_conditions() {
    let mut m = model(vec![A]);
    let now = m.calendar.date;
    let unmet = conditions(&[11], 0);
    m.world.victory_conditions.insert(A, VictoryConditions { deadline: NO_DATE, ..unmet.clone() });
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "no date: no deadline");
    m.world.victory_conditions.insert(A, VictoryConditions { deadline: now, ..unmet.clone() });
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "the deadline's own turn is not past it");
    let day_before = Date { year: now.year - 1, month: 11, half: HALF_LATE, ..now };
    m.world.victory_conditions.insert(A, VictoryConditions { deadline: day_before, ..unmet.clone() });
    assert_eq!(update(&mut m), vec![CampaignEvent::VictoryConditionFailed { faction: A }, CampaignEvent::CampaignLost { faction: A }]);
    assert!(m.world.victory_conditions[&A].failed && m.turn.victory_checked);
    assert_eq!(state(&mut m, A), (VictoryState::Lost, Vec::new()), "failed once: the test does nothing more");
    // A failed deadline wins over regions that are all held (the deadline is tested first).
    let mut m = model(vec![A]);
    m.world.victory_conditions.insert(A, VictoryConditions { deadline: day_before, ..conditions(&[10], 0) });
    assert_eq!(state(&mut m, A), (VictoryState::Lost, vec![CampaignEvent::VictoryConditionFailed { faction: A }]));
}

/// A listed region is held when the faction owns it, or when its owner is the faction's protectorate
/// (`0x00B64C50` on the owner's stance towards the faction) and not the rebels (`0x008CEEF0`). A region
/// missing from the world is never held.
#[test]
fn listed_regions_are_held_by_the_faction_or_its_protectorates() {
    let mut m = model(vec![A]);
    m.world.victory_conditions.insert(A, conditions(&[10, 11], 0));
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "B holds region 11");
    m.world.set_stance(B, A, Stance::Protectorate).unwrap();
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)));
    assert!(m.world.victory_conditions[&A].met);
    assert_eq!(state(&mut m, A), (VictoryState::Won, Vec::new()), "met once: the test does nothing more");

    // The rebels never hold a region for anyone, whatever their stance.
    let mut m = model(vec![A]);
    let rebels = FactionId(4);
    let mut rebel_faction = m.world.factions[&C].clone();
    rebel_faction.id = rebels;
    rebel_faction.key = String::new();
    m.world.factions.insert(rebels, rebel_faction);
    m.world.regions.get_mut(&RegionId(11)).expect("region 11").owner = rebels;
    m.world.factions.get_mut(&rebels).expect("rebels").diplomacy.insert(A, Stance::Protectorate);
    m.world.victory_conditions.insert(A, conditions(&[11], 0));
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()));

    let mut m = model(vec![A]);
    m.world.victory_conditions.insert(A, conditions(&[10, 99], 0));
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "region 99 is not in the world");
}

/// The count: the faction's regions plus those of every faction it is the patron of (`0x00B64CB0`).
#[test]
fn the_count_takes_the_regions_of_the_factions_protectorates() {
    let mut m = model(vec![A]);
    m.world.victory_conditions.insert(A, conditions(&[], 3));
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "A holds 2");
    m.world.set_stance(A, B, Stance::Patron).unwrap();
    assert_eq!(m.world.stance(B, A), Stance::Protectorate);
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)), "2 + B's 1");
}

/// The rival rule (`0x00955B70`, the campaign data's [`VictoryRival`]): a record of its type is met while
/// the rival holds at most the count, or does not exist; the faction's own regions do not count. A record
/// of another type keeps the count rule.
#[test]
fn the_rival_rule_counts_the_rivals_regions() {
    let mut m = model(vec![A]);
    set_features(&mut m, |f| f.victory_rival = Some(VictoryRival { kind: 5, faction: "test_faction_b".into() }));
    m.world.victory_conditions.insert(A, VictoryConditions { kind: 5, ..conditions(&[], 0) });
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "B holds 1 > 0");
    m.world.victory_conditions.get_mut(&A).expect("inserted").count = 1;
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)), "B holds 1 <= 1");

    let mut m = model(vec![A]);
    set_features(&mut m, |f| f.victory_rival = Some(VictoryRival { kind: 5, faction: "no_such_faction".into() }));
    m.world.victory_conditions.insert(A, VictoryConditions { kind: 5, ..conditions(&[], 0) });
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)), "no rival: met");

    let mut m = model(vec![A]);
    set_features(&mut m, |f| f.victory_rival = Some(VictoryRival { kind: 5, faction: "test_faction_b".into() }));
    m.world.victory_conditions.insert(A, conditions(&[], 3));
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "type 1: A's 2 regions against 3");
}

/// With a hold (`0x00951590`): met starts it at the turn's date, and the conditions count only once they
/// have been met for `hold_turns` turns (`0x008EB610`); a turn not met stops the hold, and the next met
/// turn starts it again.
#[test]
fn a_hold_counts_the_turns_since_it_started() {
    let mut m = model(vec![A]);
    m.world.victory_conditions.insert(A, VictoryConditions { hold_turns: 2, ..conditions(&[10], 0) });
    let start = m.calendar.date;
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()));
    assert!(m.world.victory_conditions[&A].holding && m.world.victory_conditions[&A].since == start);
    m.calendar.advance_turn(None);
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "one turn held");
    // Lost for a turn: the hold stops (its date stays), and starts again when met.
    m.world.regions.get_mut(&RegionId(10)).expect("region 10").owner = B;
    m.calendar.advance_turn(None);
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()));
    assert!(!m.world.victory_conditions[&A].holding && m.world.victory_conditions[&A].since == start);
    m.world.regions.get_mut(&RegionId(10)).expect("region 10").owner = A;
    m.calendar.advance_turn(None);
    let restart = m.calendar.date;
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()));
    assert!(m.world.victory_conditions[&A].holding && m.world.victory_conditions[&A].since == restart);
    m.calendar.advance_turn(None);
    assert_eq!(state(&mut m, A), (VictoryState::Undecided, Vec::new()), "one turn since the restart");
    m.calendar.advance_turn(None);
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)), "two turns since the restart");
    assert!(m.world.victory_conditions[&A].holding);
}

/// The prestige test (`0x00953660`: no faction's prestige above the faction's). PROVISIONAL with
/// [`CampaignModel::prestige`]: no faction scores prestige yet, so it always passes.
#[test]
fn the_prestige_test_passes_while_no_prestige_is_scored() {
    let mut m = model(vec![A]);
    m.world.victory_conditions.insert(A, VictoryConditions { prestige_test: true, ..conditions(&[10], 0) });
    assert_eq!(state(&mut m, A), (VictoryState::Won, met_events(A)));
}

/// The scripts' notification switch (+0x125) drops the `campaign_won` message; the met event stays.
#[test]
fn the_notification_switch_drops_the_campaign_won_message() {
    let mut m = model(vec![A]);
    m.world.victory_notification_disabled = true;
    m.world.victory_conditions.insert(A, conditions(&[10], 0));
    assert_eq!(state(&mut m, A), (VictoryState::Won, vec![CampaignEvent::VictoryConditionMet { faction: A }]));
}

/// The shared win (`0x0096CF69`..`0x0096CF84`, the campaign data's `shared_victory_kind`): when the other
/// human is allied with the winner and both records are of the shared type, the other human gets the
/// victory (its victory movie, the campaign over) instead of the defeat. Not allied, another type or a
/// campaign without a shared win: the defeat.
#[test]
fn a_shared_win_lets_the_allied_human_win_too() {
    let run = |shared: Option<u32>, allied: bool, other_kind: u32| {
        let mut m = model(vec![A, B]);
        m.world.multiplayer_campaign = true;
        set_features(&mut m, |f| f.shared_victory_kind = shared);
        if allied {
            m.world.set_stance(A, B, Stance::Allied).unwrap();
        }
        m.world.victory_conditions.insert(A, VictoryConditions { kind: 5, ..conditions(&[10], 0) });
        m.world.victory_conditions.insert(B, VictoryConditions { kind: other_kind, ..conditions(&[99], 0) });
        let ev = update(&mut m);
        assert!(m.turn.victory_checked && m.turn.campaign_over);
        movies(&ev).into_iter().map(|(f, movie, exit)| (f, movie.to_string(), exit)).collect::<Vec<_>>()
    };
    let won_together = vec![(B, "won.bik".to_string(), MovieExit::ExitGame), (A, "won.bik".to_string(), MovieExit::ExitGame)];
    let b_lost = vec![(B, "b_lost.bik".to_string(), MovieExit::ExitGame), (A, "won.bik".to_string(), MovieExit::ExitGame)];
    assert_eq!(run(Some(5), true, 5), won_together);
    assert_eq!(run(Some(5), false, 5), b_lost, "not allied");
    assert_eq!(run(Some(5), true, 1), b_lost, "the other's record is of another type");
    assert_eq!(run(None, true, 5), b_lost, "the campaign shares no win");
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

/// In a multiplayer campaign (+0x1034, tested at `0x0096CEA9`) the win ends the campaign: the first
/// other human of the faction list gets the defeat event first (+0x108 with flag 1: its defeat movie),
/// then the winner its victory with the "campaign over" flag (the movie exits to the main menu), and the
/// campaign is over. Without the flag a second human is not looked for: only the winner's victory, with
/// the flag clear, and the campaign goes on.
#[test]
fn a_win_against_another_human_ends_the_campaign() {
    let mut m = model(vec![A, B]);
    m.world.multiplayer_campaign = true;
    with_states(&mut m, &[(A, VictoryState::Won)]);
    let ev = update(&mut m);
    assert_eq!(movies(&ev), vec![(B, "b_lost.bik", MovieExit::ExitGame), (A, "won.bik", MovieExit::ExitGame)]);
    assert_eq!(lost(&ev), Vec::new(), "B is undecided: no lost event");
    assert!(m.turn.victory_checked && m.turn.campaign_over);
    assert_eq!(update(&mut m), Vec::new());

    let mut m = model(vec![A, B]);
    with_states(&mut m, &[(A, VictoryState::Won)]);
    let ev = update(&mut m);
    assert_eq!(movies(&ev), vec![(A, "won.bik", MovieExit::ShowVictoryOptionsPopup)], "not a multiplayer campaign");
    assert!(m.turn.victory_checked && !m.turn.campaign_over);
}

/// A winner and a loser in one check: the loser's lost event (+0x138) comes after the winners' events.
#[test]
fn a_loser_is_told_after_the_winner() {
    let mut m = model(vec![A, B]);
    m.world.multiplayer_campaign = true;
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

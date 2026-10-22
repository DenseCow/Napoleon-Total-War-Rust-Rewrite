//! Campaign model tests: whether the local player may save ([`CampaignModel::can_save`], the exe's
//! `0x008BE4A0`), one test per modelled condition in the exe's order (conditions 3 and 6 are PROVISIONAL and untested).

use super::*;

/// A's turn under way, A the only human.
fn in_a_turn() -> CampaignModel {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m
}

fn battle(attacker: ForceId, defenders: Vec<ForceId>) -> Option<super::battles::PendingBattle> {
    Some(super::battles::PendingBattle { attacker, defenders, settlement: None, resume: None })
}

/// Condition 7 alone: the player's own turn, under way.
#[test]
fn the_player_may_save_in_its_own_turn() {
    let m = in_a_turn();
    assert!(m.can_save(A));
    assert!(!m.can_save(B), "it is not B's turn");
}

/// Condition 1, +0xFE9 (`0x008BE4A6`): the campaign is over.
#[test]
fn no_save_once_the_campaign_is_over() {
    let mut m = in_a_turn();
    m.end_campaign();
    assert!(!m.can_save(A));
    // Before even a battle the player takes part in (condition 4 comes later).
    m.pending_battle = battle(ForceId(1000), vec![ForceId(1001)]);
    assert!(!m.can_save(A));
}

/// Condition 2, `[+0xFA8]+0x115` (`0x008BE4B9`): the scripts switched saving off.
#[test]
fn no_save_while_the_scripts_disable_saving() {
    let mut m = in_a_turn();
    m.world.saving_disabled = true;
    assert!(!m.can_save(A));
    m.pending_battle = battle(ForceId(1000), vec![ForceId(1001)]);
    assert!(!m.can_save(A));
}

/// Condition 4, battle state 1 (`0x008CEE40`): a player on either side of the pending battle may save
/// at once, outside its turn and over a waiting pending action; one not in it goes on to the rest.
#[test]
fn a_pending_battle_lets_its_sides_save_and_others_go_on() {
    let mut m = in_a_turn();
    m.turn.current = Some(B);
    m.pending_battle = battle(ForceId(1000), vec![ForceId(1001)]);
    assert!(m.can_save(A), "A attacks");
    m.pending_battle = battle(ForceId(1001), vec![ForceId(1000)]);
    assert!(m.can_save(A), "A defends");
    m.pending_capture = Some(capture_waiting());
    assert!(m.can_save(A), "the battle answers before the pending-action stack (condition 5)");

    // C is in neither side: the battle does not decide, the turn test does.
    let mut m = in_a_turn();
    m.pending_battle = battle(ForceId(1001), vec![ForceId(1000)]);
    assert!(!m.can_save(C));
    m.turn.current = Some(C);
    assert!(m.can_save(C));
}

/// A capture's report as `ChooseCapture` waits for it.
fn capture_waiting() -> super::capture::CapturePreview {
    let mut m = capture_model(vec![A]);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    m.pending_capture.expect("A took the settlement by force")
}

/// Condition 5, `[+0xF70]` (`0x005865E0`): a capture waiting for its occupy / loot / liberate choice.
#[test]
fn no_save_while_a_capture_waits_for_its_choice() {
    let mut m = capture_model(vec![A]);
    assert!(m.can_save(A));
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(m.pending_capture.is_some());
    assert!(!m.can_save(A));
    m.apply(CampaignCommand::ChooseCapture { choice: super::capture::CaptureChoice::Occupy }).unwrap();
    assert!(m.can_save(A));
}

/// Condition 5, `[+0xF70]`: a call to war waiting for the human ally's answer.
#[test]
fn no_save_while_a_call_to_war_waits_for_the_answer() {
    let mut m = in_a_turn();
    m.turn.humans = vec![A, C];
    // Major powers in the game, as `ally_war_call_tests` sets them up.
    for f in [A, B, C] {
        let d = m.world.faction_details.entry(f).or_default();
        d.major = Some(true);
        d.out_of_game = Some(false);
    }
    m.form_alliance(C, B).unwrap();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!(m.pending_ally_war_calls.len(), 1);
    assert!(!m.can_save(A));
    m.apply(CampaignCommand::RespondToAllyWarCall { side: Some(B), accept: false }).unwrap();
    assert!(m.pending_ally_war_calls.is_empty());
    assert!(m.can_save(A));
}

/// Condition 7, `[+0xF5C]+0x18` / `+0x3C` == 1: not before the campaign's first turn, not in another
/// faction's turn, and not once the player has ended its turn (`0x0096E200` moves the state on to 2).
#[test]
fn no_save_outside_the_players_turn_under_way() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    assert!(!m.can_save(A), "the campaign has not started");
    let mut m = in_a_turn();
    m.turn.current = Some(B);
    assert!(!m.can_save(A), "B's turn");
    let mut m = in_a_turn();
    m.begin_end_turn();
    assert!(!m.turn.in_turn);
    assert!(!m.can_save(A), "A ended its turn");
}

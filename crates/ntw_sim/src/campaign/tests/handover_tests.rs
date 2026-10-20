//! Campaign model tests: a faction destroyed by a killer (`0x008AB6C0` with a killer), the script's
//! `grant_faction_handover` (`0x00978950`) and the mission takeover reward (`0x00A06050`).

use super::trade::TradePath;
use super::*;

/// The test model with capitals (A: 10, B: 11) and every faction in the game.
fn handover_model() -> CampaignModel {
    let mut m = test_model();
    set_details(&mut m, A, Some(10), false);
    set_details(&mut m, B, Some(11), false);
    set_details(&mut m, C, None, false);
    m
}

fn out_of_game(m: &CampaignModel, f: FactionId) -> bool {
    m.world.faction_details.get(&f).is_some_and(|d| d.out_of_game == Some(true))
}

fn destroyed_flag(m: &CampaignModel, f: FactionId) -> bool {
    m.world.faction_details.get(&f).is_some_and(|d| d.destroyed_faction)
}

/// An AI faction destroyed by another: its regions (the capital among them) all pass to the killer, its
/// characters and forces go, the killer's destroyed-a-faction flag (+0x924) is set and no
/// `FactionDestroyed` is posted (the exe posts event +0x108 only without a killer or for a human).
#[test]
fn an_ai_faction_destroyed_by_a_killer_hands_it_every_region() {
    let mut m = handover_model();
    let mut events = Vec::new();
    assert!(m.destroy_faction_by(A, Some(B), &mut events));
    assert!(out_of_game(&m, A));
    for r in [10, 11, 12] {
        assert_eq!(m.world.regions[&RegionId(r)].owner, B, "region {r}");
    }
    assert!(!m.world.characters.values().any(|c| c.faction == A));
    assert!(!m.world.forces.values().any(|f| f.faction == A));
    assert!(destroyed_flag(&m, B));
    assert!(!destroyed_flag(&m, C), "only the killer's flag");
    assert!(!events.iter().any(|e| matches!(e, CampaignEvent::FactionDestroyed { .. })), "{events:?}");
}

/// Only the relationship towards the killer breaks its trade first (`0x00B2B920` → `0x00B73430`): the
/// first route each way goes; a third faction's route with the destroyed one is left to the reset.
#[test]
fn the_trade_with_the_killer_is_broken_both_ways() {
    let mut m = handover_model();
    for (x, y) in [(A, B), (B, A), (A, C), (C, A)] {
        m.relationship_mut(x, y).trade_agreement = true;
        m.world.trade_paths.insert((x, y), vec![TradePath::default()]);
    }
    assert!(m.destroy_faction_by(A, Some(B), &mut Vec::new()));
    for key in [(A, B), (B, A), (A, C), (C, A)] {
        assert!(!m.world.relationships[&key].trade_agreement, "{key:?}: reset");
    }
    assert!(!m.world.trade_paths.contains_key(&(A, B)) && !m.world.trade_paths.contains_key(&(B, A)));
    assert!(m.world.trade_paths.contains_key(&(A, C)) && m.world.trade_paths.contains_key(&(C, A)), "no break towards C");
}

/// The killer's flag is set even when nothing is handed over, and its own turn end clears it (the word
/// write at `0x008BD408` that clears +0x925 too), after its `FactionTurnEnd`.
#[test]
fn the_destroyed_a_faction_flag_lasts_until_the_killers_turn_end() {
    let mut m = handover_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    let killer = m.turn.current.unwrap();
    let target = if killer == C { B } else { C };
    m.world.regions.retain(|_, r| r.owner != target);
    assert!(m.destroy_faction_by(target, Some(killer), &mut Vec::new()));
    assert!(destroyed_flag(&m, killer));
    m.begin_end_turn();
    let end = m.step().unwrap();
    assert!(end.contains(&CampaignEvent::FactionTurnEnd { faction: killer }));
    assert!(destroyed_flag(&m, killer), "still set when the FactionTurnEnd handlers run");
    m.step();
    assert!(!destroyed_flag(&m, killer), "cleared by the killer's turn end");
}

/// A human faction destroyed by a killer keeps its regions and gets `FactionDestroyed`.
#[test]
fn a_human_faction_destroyed_by_a_killer_keeps_its_regions() {
    let mut m = handover_model();
    m.turn.humans = vec![A];
    let mut events = Vec::new();
    assert!(m.destroy_faction_by(A, Some(B), &mut events));
    assert!(out_of_game(&m, A));
    assert_eq!((m.world.regions[&RegionId(10)].owner, m.world.regions[&RegionId(12)].owner), (A, A));
    assert!(events.contains(&CampaignEvent::FactionDestroyed { faction: A }));
}

/// ORIGINAL BUG (`0x008AB6C0` → `0x00A64AC0`'s same-owner return `0x00A64D13`): a faction destroying
/// itself hangs the exe; ours refuses it and changes nothing.
#[test]
fn a_faction_cannot_destroy_itself() {
    let mut m = handover_model();
    let before = m.clone();
    let mut events = Vec::new();
    assert!(!m.destroy_faction_by(A, Some(A), &mut events));
    assert!(events.is_empty());
    assert_eq!(m.state_hash(), before.state_hash());
    assert!(!out_of_game(&m, A));
}

/// `grant_faction_handover`'s window: outside `min_turn..=max_turn`, or with the target out of the
/// game, nothing happens and no draw is made.
#[test]
fn a_handover_outside_its_window_draws_nothing() {
    let mut m = handover_model();
    m.calendar.turns_elapsed = 5;
    let rng = m.rng;
    assert_eq!(m.grant_faction_handover(B, A, 6, 9), None);
    assert_eq!(m.grant_faction_handover(B, A, 1, 4), None);
    // max below min is raised to min (0x00978A55): 6..=6 does not hold turn 5.
    assert_eq!(m.grant_faction_handover(B, A, 6, 3), None);
    assert_eq!(m.rng, rng);
    set_details(&mut m, A, Some(10), true);
    assert_eq!(m.grant_faction_handover(B, A, 0, 9), None);
    assert_eq!(m.rng, rng, "no draw for a target out of the game");
}

/// Inside the window one draw (`0..=100`) is made; the target is destroyed when
/// `100 / (max_turn − turn + 1)` is at least it, and always on `max_turn` (a raised max included).
#[test]
fn a_handover_inside_its_window_rolls_once() {
    let mut seen = [false; 2];
    for seed in 0..40 {
        let mut m = handover_model();
        m.calendar.turns_elapsed = 5;
        m.rng = crate::rng::CaRng::new(seed);
        let mut want = m.rng;
        let draw = want.percent_0_100() as f32;
        let destroyed = m.grant_faction_handover(B, A, 0, 6).is_some();
        assert_eq!(m.rng, want, "seed {seed}: one draw");
        assert_eq!(destroyed, 50.0 >= draw, "seed {seed}: draw {draw}");
        assert_eq!(out_of_game(&m, A), destroyed);
        seen[usize::from(destroyed)] = true;
    }
    assert_eq!(seen, [true, true], "both outcomes seen");
    for (min, max) in [(0, 5), (5, 3)] {
        let mut m = handover_model();
        m.calendar.turns_elapsed = 5;
        let events = m.grant_faction_handover(B, A, min, max).expect("the last turn is certain");
        assert!(!events.iter().any(|e| matches!(e, CampaignEvent::FactionDestroyed { .. })), "{min}..{max}: {events:?}");
        assert_eq!(m.world.regions[&RegionId(10)].owner, B);
    }
}

/// The binding passes the killer through unchecked: a target granted to itself is refused after the
/// draw (ours; the exe hangs) and nothing else changes.
#[test]
fn a_handover_to_the_target_itself_is_refused() {
    let mut m = handover_model();
    m.calendar.turns_elapsed = 5;
    let before = m.clone();
    assert_eq!(m.grant_faction_handover(A, A, 0, 5), None);
    m.rng = before.rng;
    assert_eq!(m.state_hash(), before.state_hash());
}

/// A mission's takeover reward destroys its target by the mission's owner (`0x00A06050`).
#[test]
fn a_takeover_reward_destroys_the_target_by_the_owner() {
    let mut m = handover_model();
    let events = m.take_over_faction(B, A).expect("not refused");
    assert!(events.contains(&CampaignEvent::ForceDestroyed { force: ForceId(1001) }), "{events:?}");
    assert!(out_of_game(&m, B));
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert!(destroyed_flag(&m, A));
}

/// The hand-over walks the faction's region list (+0x76C), not the region ids: the first region each
/// time, the second while the first is the capital (`0x00A8B5A0`), so the capital goes last. Each
/// region joins the end of the killer's list as it passes (`0x00A64AC0`).
#[test]
fn the_hand_over_follows_the_region_list_order() {
    let mut m = handover_model();
    let mut r13 = m.world.regions[&RegionId(12)].clone();
    (r13.id, r13.key) = (RegionId(13), "test_region_13".into());
    m.world.regions.insert(r13.id, r13);
    // A's list: 13, 10 (its capital), 12; in id order it would be 10, 12, 13.
    m.world.region_order = vec![RegionId(13), RegionId(10), RegionId(12), RegionId(11)];
    assert_eq!(m.world.faction_regions_in_list_order(A), vec![RegionId(13), RegionId(10), RegionId(12)]);
    assert!(m.destroy_faction_by(A, Some(B), &mut Vec::new()));
    assert_eq!(m.world.region_gains, vec![RegionId(13), RegionId(12), RegionId(10)], "13, then 12 past the capital, the capital last");
    assert_eq!(m.world.faction_regions_in_list_order(B), vec![RegionId(11), RegionId(13), RegionId(12), RegionId(10)]);
}

/// A new capital tie (two settlements the same distance from the lost one) keeps the first region of
/// the faction's list (`0x008DABC0` walks +0x76C with a strict `<`), not the lowest id.
#[test]
fn a_capital_tie_keeps_the_first_region_of_the_list() {
    let mut m = handover_model();
    // Region 8 lies as far from the capital (10) as region 12 does; neither has four settlement slots.
    let mut r8 = m.world.regions[&RegionId(12)].clone();
    (r8.id, r8.key) = (RegionId(8), "test_region_8".into());
    r8.settlement.position = pos(8, 0);
    m.world.regions.insert(r8.id, r8);
    m.world.region_order = vec![RegionId(12), RegionId(8), RegionId(10), RegionId(11)];
    assert!(m.transfer_region(RegionId(10), B));
    assert_eq!(m.world.capital(A), Some(RegionId(12)), "12 is first in A's list");
}

/// A takeover by the target itself is refused and says so (`None`), changing nothing.
#[test]
fn a_takeover_by_the_target_itself_is_refused() {
    let mut m = handover_model();
    let before = m.state_hash();
    assert_eq!(m.take_over_faction(A, A), None);
    assert_eq!(m.state_hash(), before);
}

//! Port blockades ([`crate::campaign::port_blockade`]; analysis/fidelity/CAMPAIGN_FIDELITY.md "Port
//! blockades"): the one lift `0x00A784E0` and its price update `0x00BCB020(0)`, from a peace (`0x00A25570`),
//! an owner change's slot pass (`0x00B1B300`) and the blockading navy's destructor (`0x0088EF90`).

use super::turn_order_tests::{make_rebels, price_model, price_state};
use super::*;

const R12: RegionId = RegionId(12);
/// B's navy, blockading region 12's port from the open sea.
const NAVY: ForceId = ForceId(1005);

/// The price state after an update with flag 0 ([`price_model`]: 140, the history kept, trend 0), and before any.
fn updated() -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    (vec![140], vec![100], vec![120], vec![0])
}
fn unchanged() -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    (vec![10], vec![100], vec![120], vec![9])
}

/// Adds navy `force` of `faction` (admiral `admiral`) at `at`.
fn add_navy(m: &mut CampaignModel, force: ForceId, admiral: i32, faction: FactionId, at: (Fixed20, Fixed20)) {
    let c = Character { id: CharacterId(admiral), faction, kind: CharacterKind::Admiral, position: at, movement_points: 0, max_movement_points: 0, base_movement_points: 0, garrisoned_in: None };
    m.world.characters.insert(c.id, c);
    m.world.forces.insert(force, MilitaryForce { id: force, faction, commander: Some(CharacterId(admiral)), units: vec![unit(admiral)], is_navy: true });
}

/// [`price_model`] with a port slot at (5, 5) in C's region 12, held by `holder` (`None` = C), and B's navy
/// [`NAVY`] at sea (50, 50), blockading it when `blockaded`.
fn blockade_model(holder: Option<FactionId>, blockaded: bool) -> CampaignModel {
    let mut m = price_model();
    let mut slot = m.world.regions[&RegionId(10)].slots[1].clone();
    (slot.port, slot.position, slot.holder) = (true, Some(pos(5, 5)), holder);
    slot.blockade = blockaded.then_some(PortBlockade { navy: NAVY, docked_navy_attacked: true });
    m.world.regions.get_mut(&R12).unwrap().slots.push(slot);
    add_navy(&mut m, NAVY, 109, B, pos(50, 50));
    m
}

fn blockade(m: &CampaignModel) -> Option<PortBlockade> {
    m.world.regions[&R12].slots.last().unwrap().blockade
}

/// A peace lifts a blockade by either side of a port the other side holds (`0x00A25570` → `0x00A784E0`, the
/// call at `0x00A25627`), and the lift runs the price update with flag 0. A blockade of a third faction's
/// port stays.
#[test]
fn a_peace_lifts_a_blockade_of_the_other_sides_port_and_updates_the_prices() {
    let peace = |blockaded: bool, a: FactionId, b: FactionId| {
        let mut m = blockade_model(None, blockaded);
        m.world.set_stance(a, b, Stance::War).unwrap();
        m.make_peace_rules(a, b, true).unwrap();
        (blockade(&m), price_state(&m))
    };
    assert_eq!(peace(true, B, C), (None, updated()), "B's navy, C's port");
    assert_eq!(peace(true, C, B), (None, updated()), "either side proposing");
    assert_eq!(peace(false, B, C), (None, unchanged()), "no blockade: the peace updates no prices");
    let kept = peace(true, A, B);
    assert_eq!(kept.0.map(|b| b.navy), Some(NAVY), "C's port is a third party's");
    assert_eq!(kept.1, unchanged());
    // The port's holder is what counts, not the region's owner.
    let mut m = blockade_model(Some(A), true);
    m.world.set_stance(A, B, Stance::War).unwrap();
    m.make_peace_rules(A, B, true).unwrap();
    assert_eq!(blockade(&m), None, "A holds the port in C's region");
}

/// An owner change's slot pass (`0x00B1B300`, the lift at `0x00B1B3EF`) lifts the blockade of a port the new
/// owner now holds when the blockading navy is the new owner's or not at war with it (`0x008CE9B0`); at war,
/// or with the port's holder kept by a docked navy, the blockade stays. The rebels lose the region, so no
/// route loss updates the prices: here a port that does not pass (a navy is docked) updates them only
/// through the lift.
#[test]
fn an_owner_change_lifts_a_blockade_of_a_port_the_new_owner_holds_unless_at_war() {
    let transfer = |holder: Option<FactionId>, war: bool, docked: Option<FactionId>| {
        let mut m = blockade_model(holder, true);
        make_rebels(&mut m, C);
        if war {
            m.world.set_stance(A, B, Stance::War).unwrap();
        }
        if let Some(f) = docked {
            add_navy(&mut m, ForceId(1006), 110, f, pos(5, 5));
        }
        assert!(m.transfer_region(R12, A), "the rebels lose region 12");
        (blockade(&m).is_some(), price_state(&m))
    };
    assert_eq!(transfer(None, false, None), (false, updated()), "the port passes to A and the blockade lifts");
    assert!(transfer(None, true, None).0, "A at war with B: the blockade stays");
    assert_eq!(transfer(None, false, Some(B)), (true, unchanged()), "B's docked navy keeps the port: no pass, no lift");
    assert_eq!(transfer(Some(A), false, Some(A)), (false, updated()), "A's docked port already A's: only the lift updates");
    assert_eq!(transfer(Some(A), true, Some(A)), (true, unchanged()), "the same at war: nothing");
    // The new owner's own navy blockading: never at war with itself.
    let mut m = blockade_model(None, true);
    make_rebels(&mut m, C);
    m.world.forces.get_mut(&NAVY).unwrap().faction = A;
    m.world.characters.get_mut(&CharacterId(109)).unwrap().faction = A;
    assert!(m.transfer_region(R12, A));
    assert_eq!(blockade(&m), None);
}

/// A handover to the region's own owner still passes the slots (`0x00B1B300` at `0x00B44B69`), so a blockade by a
/// navy at peace with the owner lifts there too.
#[test]
fn a_same_owner_handover_lifts_a_blockade_at_peace() {
    let mut m = blockade_model(None, true);
    assert!(!m.transfer_region(R12, C), "C already owns region 12");
    assert_eq!(blockade(&m), None);
    let mut m = blockade_model(None, true);
    m.world.set_stance(B, C, Stance::War).unwrap();
    m.transfer_region(R12, C);
    assert!(blockade(&m).is_some(), "B at war with C");
}

/// The blockading navy's destructor lifts its blockade (`0x0088EF90` → `0x00A784E0` on the navy's +0xE8 port), and
/// the lift updates the prices; another navy's blockade stays.
#[test]
fn a_destroyed_blockading_navy_lifts_its_blockade_and_updates_the_prices() {
    let mut m = blockade_model(None, true);
    m.destroy_force(NAVY, &mut Vec::new());
    assert_eq!((blockade(&m), price_state(&m)), (None, updated()));
    let mut m = blockade_model(None, true);
    add_navy(&mut m, ForceId(1007), 111, B, pos(60, 60));
    m.destroy_force(ForceId(1007), &mut Vec::new());
    assert_eq!((blockade(&m).map(|b| b.navy), price_state(&m)), (Some(NAVY), unchanged()));
}

/// The lift itself (`0x00A784E0`): the link and the +0x74 byte go, the price update runs; a slot that is not
/// blockaded is left alone and runs none.
#[test]
fn the_lift_clears_the_blockade_and_updates_the_prices_once() {
    let mut m = blockade_model(None, true);
    assert!(m.lift_port_blockade(R12, 0));
    assert_eq!((blockade(&m), price_state(&m)), (None, updated()));
    let mut m = blockade_model(None, false);
    assert!(!m.lift_port_blockade(R12, 0));
    assert_eq!(price_state(&m), unchanged());
}

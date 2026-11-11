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
/// call at `0x00A25627`), and the lift runs the price update with flag 0, before the peace's own
/// (`0x00B2643F`). A blockade of a third faction's port stays.
#[test]
fn a_peace_lifts_a_blockade_of_the_other_sides_port_and_updates_the_prices() {
    let peace = |blockaded: bool, a: FactionId, b: FactionId| {
        let mut m = blockade_model(None, blockaded);
        m.world.set_stance(a, b, Stance::War).unwrap();
        let updates = price_updates(|| drop(m.make_peace_rules(a, b, true).unwrap()));
        (blockade(&m), price_state(&m), updates.len())
    };
    assert_eq!(peace(true, B, C), (None, updated(), 2), "B's navy, C's port: the lift's update, then the peace's");
    assert_eq!(peace(true, C, B), (None, updated(), 2), "either side proposing");
    assert_eq!(peace(false, B, C), (None, updated(), 1), "no blockade: only the peace's own update");
    let kept = peace(true, A, B);
    assert_eq!(kept.0.map(|b| b.navy), Some(NAVY), "C's port is a third party's");
    assert_eq!((kept.1, kept.2), (updated(), 1));
    // The port's holder is what counts, not the region's owner.
    let mut m = blockade_model(Some(A), true);
    m.world.set_stance(A, B, Stance::War).unwrap();
    m.make_peace_rules(A, B, true).unwrap();
    assert_eq!(blockade(&m), None, "A holds the port in C's region");
}

/// The peace reaches a blockading navy only through its commander's step (`0x00A25570` per listed
/// character, `0x009D3A80`): a navy with no commander, or whose commander is in neither side's list, keeps
/// blockading; and an army is not a navy (`0x009D3A80` returns null for it).
#[test]
fn a_peace_keeps_a_blockade_its_character_walk_does_not_reach() {
    let peace = |edit: &dyn Fn(&mut CampaignModel)| {
        let mut m = blockade_model(None, true);
        m.world.set_stance(B, C, Stance::War).unwrap();
        edit(&mut m);
        let updates = price_updates(|| drop(m.make_peace_rules(B, C, true).unwrap()));
        (blockade(&m).map(|b| b.navy), updates.len())
    };
    assert_eq!(peace(&|_| {}), (None, 2), "commanded: lifted");
    assert_eq!(peace(&|m| m.world.forces.get_mut(&NAVY).unwrap().commander = None), (Some(NAVY), 1), "no commander");
    assert_eq!(peace(&|m| {
        m.world.characters.remove(&CharacterId(109));
    }), (Some(NAVY), 1), "a commander in no list");
    assert_eq!(peace(&|m| m.world.forces.get_mut(&NAVY).unwrap().is_navy = false), (Some(NAVY), 1), "not a navy");
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
        assert!(m.transfer_region(R12, A, &mut Vec::new()), "the rebels lose region 12");
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
    assert!(m.transfer_region(R12, A, &mut Vec::new()));
    assert_eq!(blockade(&m), None);
}

/// A handover to the region's own owner still passes the slots (`0x00B1B300` at `0x00B44B69`), so a blockade by a
/// navy at peace with the owner lifts there too.
#[test]
fn a_same_owner_handover_lifts_a_blockade_at_peace() {
    let mut m = blockade_model(None, true);
    assert!(!m.transfer_region(R12, C, &mut Vec::new()), "C already owns region 12");
    assert_eq!(blockade(&m), None);
    let mut m = blockade_model(None, true);
    m.world.set_stance(B, C, Stance::War).unwrap();
    m.transfer_region(R12, C, &mut Vec::new());
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

/// C's navy, docked in region 12's port.
const DOCKED: ForceId = ForceId(1006);

/// [`blockade_model`] with B at war with C and B's admiral able to sail anywhere; `docked` adds C's navy
/// [`DOCKED`] in the port.
fn order_model(blockaded: bool, docked: bool) -> CampaignModel {
    let mut m = blockade_model(None, blockaded);
    m.world.set_stance(B, C, Stance::War).unwrap();
    m.world.characters.get_mut(&CharacterId(109)).unwrap().movement_points = 1_000_000;
    if docked {
        add_navy(&mut m, DOCKED, 110, C, pos(5, 5));
    }
    m
}

fn sail(m: &mut CampaignModel, navy: ForceId, to: (Fixed20, Fixed20)) -> Result<Vec<CampaignEvent>, CommandError> {
    m.apply(CampaignCommand::MoveForce { force: navy, to })
}

/// The port order (`0x0095D6A0`) and its arrival (`0x0094B9B0`): onto a hostile port with no docked navy the
/// navy sails in and docks; with one docked there it stops outside and the blockade starts
/// (`StartPortBlockade` `0x00A66F80`), the +0x74 byte clear, with one price update with flag 0.
#[test]
fn a_port_order_docks_in_an_empty_hostile_port_and_blockades_a_held_one() {
    let mut m = order_model(false, false);
    sail(&mut m, NAVY, pos(5, 5)).unwrap();
    assert_eq!((m.docked_port(NAVY), blockade(&m)), (Some((R12, 0)), None), "no docked navy: it docks");

    let mut m = order_model(false, true);
    let updates = price_updates(|| drop(sail(&mut m, NAVY, pos(5, 5)).unwrap()));
    assert_eq!(blockade(&m), Some(PortBlockade { navy: NAVY, docked_navy_attacked: false }));
    assert_eq!(m.docked_port(NAVY), None, "the blockader stays outside the port");
    assert_eq!((updates, price_state(&m)), (vec![false], updated()));
}

/// The arrival goes through the `port.arrival` seam: a mod's rule that always blockades makes the navy stop
/// outside an empty port and blockade it, and the seam's chain records the mod.
#[test]
fn the_port_arrival_seam_decides_dock_or_blockade() {
    use super::port_blockade::PortArrival;
    use super::seams::PortArrivalRule;
    let mut m = order_model(false, false);
    let always: crate::seam::Maker<PortArrivalRule> =
        Arc::new(|_| Arc::new(|_: &CampaignModel, _: ForceId, _: RegionId, _: usize| PortArrival::Blockade));
    Arc::make_mut(&mut m.rules).seams.port_arrival.apply("test.always", &always);
    assert_eq!(m.rules.seams.port_arrival.chain(), ["original", "test.always"]);
    sail(&mut m, NAVY, pos(5, 5)).unwrap();
    assert_eq!((m.docked_port(NAVY), blockade(&m).map(|b| b.navy)), (None, Some(NAVY)));
}

/// A port that already has a blockader refuses the order (`0x009243A0`) before anything changes: the other
/// navy stays where it was.
#[test]
fn a_port_order_onto_a_blockaded_port_is_refused() {
    let mut m = order_model(true, true);
    add_navy(&mut m, ForceId(1007), 111, B, pos(60, 60));
    m.world.characters.get_mut(&CharacterId(111)).unwrap().movement_points = 1_000_000;
    assert!(sail(&mut m, ForceId(1007), pos(5, 5)).is_err());
    assert_eq!(m.force_position(ForceId(1007)), Some(pos(60, 60)));
    assert_eq!(blockade(&m).map(|b| b.navy), Some(NAVY));
}

/// The start of the blockading admiral's next order lifts his blockade (`0x008FF470` → `0x00A0B680` →
/// `0x00A784E0`), with its price update, even when the order then goes nowhere.
#[test]
fn the_blockaders_next_order_lifts_the_blockade() {
    let mut m = order_model(true, false);
    sail(&mut m, NAVY, pos(60, 60)).unwrap();
    assert_eq!((blockade(&m), price_state(&m)), (None, updated()));
    let mut m = order_model(true, false);
    m.world.characters.get_mut(&CharacterId(109)).unwrap().movement_points = 0;
    let _ = sail(&mut m, NAVY, pos(60, 60));
    assert_eq!(blockade(&m), None, "no action points: the blockade is still lifted");
}

/// A blockading navy that sails off to land the army it carries lifts its blockade, with the lift's one price
/// update. ORIGINAL BUG: the exe's landing order is the general's, and its order helper (`0x008FF470` at
/// `0x0095C2C0`) lifts a blockade only for a navy's character (`0x00A0B680`), so the navy sailed off still
/// blockading the port ([`CampaignModel::disembark`]).
#[test]
fn a_blockading_navy_that_sails_off_to_land_its_army_lifts_the_blockade() {
    let mut m = order_model(true, false);
    // Sea in x 0..9 (the port at (5, 5) lies in it), the coast at x 9..10, land from x 10.
    let mut grid = super::pathing::PathGrid::new((0.0, 0.0), 2.0, 10, 6);
    grid.poly = Some(super::embark::tests::coast_map());
    m.attach_terrain(super::world::Terrain(Arc::new(grid)));
    // The navy waits at sea off the port, B's army 1001 aboard.
    let off_port = pos(3, 5);
    let admiral = m.world.characters.get_mut(&CharacterId(109)).unwrap();
    (admiral.position, admiral.movement_points, admiral.max_movement_points) = (off_port, 20, 20);
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = off_port;
    m.world.embarked.insert(ForceId(1001), NAVY);
    let mut events = Vec::new();
    let updates = price_updates(|| events = m.apply(CampaignCommand::Disembark { force: ForceId(1001), to: pos(15, 5) }).unwrap());
    assert!(events.iter().any(|e| matches!(e, CampaignEvent::CharacterDisembarksNavy { character: CharacterId(101), navy: NAVY, .. })), "{events:?}");
    assert_ne!(m.world.characters[&CharacterId(109)].position, off_port, "the navy sailed");
    assert_eq!((blockade(&m), price_state(&m)), (None, updated()));
    assert_eq!(updates, vec![false], "the lift's one update");
}

/// The end-blockade command (`0x00932710`) lifts the blockade of the navy its character commands, with the
/// lift's price update. A character with no navy, or a navy that blockades nothing, is refused (ORIGINAL BUG:
/// the exe reads through a null pointer there).
#[test]
fn the_end_blockade_command_lifts_the_admirals_blockade() {
    let mut m = order_model(true, false);
    m.apply(CampaignCommand::EndBlockade { character: CharacterId(109) }).unwrap();
    assert_eq!((blockade(&m), price_state(&m)), (None, updated()));
    let mut m = order_model(false, false);
    assert!(m.apply(CampaignCommand::EndBlockade { character: CharacterId(109) }).is_err(), "nothing blockaded");
    let mut m = order_model(true, false);
    assert!(m.apply(CampaignCommand::EndBlockade { character: CharacterId(100) }).is_err(), "a general");
    assert!(m.apply(CampaignCommand::EndBlockade { character: CharacterId(999) }).is_err(), "no such character");
    assert_eq!(blockade(&m).map(|b| b.navy), Some(NAVY));
}

/// The docked navy attacks its blockader where it lies, at once (`0x0095B370`), once per blockade: the +0x74
/// byte is set and a second attack is refused until a lift clears it.
#[test]
fn the_docked_navy_attacks_its_blockader_once_per_blockade() {
    let mut m = order_model(false, true);
    m.world.regions.get_mut(&R12).unwrap().slots[0].blockade = Some(PortBlockade { navy: NAVY, docked_navy_attacked: false });
    let events = m.apply(CampaignCommand::AttackForce { force: DOCKED, target: NAVY }).unwrap();
    assert_eq!(events, vec![CampaignEvent::PreBattle { attacker: DOCKED }]);
    assert_eq!(m.pending_battle.as_ref().map(|b| (b.attacker, b.defenders.clone())), Some((DOCKED, vec![NAVY])));
    assert_eq!((m.force_position(DOCKED), blockade(&m).map(|b| b.docked_navy_attacked)), (Some(pos(5, 5)), Some(true)));
    m.pending_battle = None;
    assert!(m.apply(CampaignCommand::AttackForce { force: DOCKED, target: NAVY }).is_err(), "once per blockade");
    assert_eq!(m.sally_port(DOCKED, NAVY), Some((R12, 0)));
    assert_eq!(m.sally_port(NAVY, DOCKED), None);
}

/// The post-battle blockade step (`0x008F7880`, the lift at `0x008F8981`): when the blockader won, the
/// blockade is lifted if no foreign navy is left docked in the port (the beaten attacker was destroyed); a
/// surviving attacker, or a docked navy of the blockader's own faction, decides as the exe's +0x60 / +0x7C
/// tests do.
#[test]
fn after_the_docked_navys_attack_the_blockade_lifts_only_when_no_foreign_navy_is_left_docked() {
    let after = |docked: Option<FactionId>| {
        let mut m = order_model(true, false);
        if let Some(f) = docked {
            add_navy(&mut m, DOCKED, 110, f, pos(5, 5));
        }
        m.lift_after_sally((R12, 0), NAVY, false);
        blockade(&m).is_some()
    };
    assert!(!after(None), "the attacker destroyed: lifted");
    assert!(after(Some(C)), "the beaten attacker still docked: kept");
    assert!(!after(Some(B)), "only the blockader's own faction docked: lifted");
}

/// A blockader beaten by the docked navy's attack that survives loses its blockade: the post-battle sequence
/// sends him into the retreat `0x00A18550` (`0x008F8421`), whose order runs the order helper (`0x0095D096`),
/// or, when it cannot run, deletes his force (`0x008CF310`), whose destructor lifts it.
#[test]
fn a_beaten_blockader_that_survives_the_docked_navys_attack_loses_its_blockade() {
    let mut m = order_model(true, false);
    add_navy(&mut m, DOCKED, 110, C, pos(5, 5));
    m.lift_after_sally((R12, 0), NAVY, true);
    assert!(m.world.forces.contains_key(&NAVY), "the blockader survives");
    assert!(blockade(&m).is_none(), "its blockade is lifted");
}

/// The post-battle placement undocks a force only from a port nobody blockades (`0x008B29E0`), and never the
/// docked navy's attack's attacker.
#[test]
fn the_post_battle_placement_keeps_a_navy_docked_in_a_blockaded_port() {
    use super::battles::PendingBattle;
    let battle = PendingBattle { attacker: DOCKED, defenders: vec![NAVY], settlement: None, resume: None, ambush: false };
    for (blockaded, sally) in [(true, false), (false, true)] {
        let mut m = order_model(blockaded, true);
        m.undock_after_battle(&battle, sally);
        assert_eq!(m.docked_port(DOCKED), Some((R12, 0)), "blockaded: {blockaded}, sally: {sally}");
    }
}

/// A navy docked in a blockaded port is no obstacle while the blockade lasts (`0x00B653A0` at the start,
/// `0x00B1D100` in the re-add `0x00B084F0`), and is one again once it is lifted.
#[test]
fn a_navy_docked_in_a_blockaded_port_is_no_obstacle() {
    let mut m = order_model(true, true);
    assert_eq!(m.navies_docked_in_blockaded_ports(), vec![DOCKED]);
    // C's own view, for another faction's character, so that C sees and knows its docked admiral.
    let seen = |m: &CampaignModel| m.obstacles_seen_by(C, CharacterId(100)).iter().any(|o| o.pos == (5.0, 5.0));
    assert!(!seen(&m));
    assert!(m.lift_port_blockade(R12, 0));
    assert!(m.navies_docked_in_blockaded_ports().is_empty());
    assert!(seen(&m));
}

/// A blockaded port's naval queue keeps its finished ships, and a besieged settlement's land queue its units
/// (`0x00B71FB0`: the byte +0x58 set from `0x00B61E70` / `0x00B61E90` skips the spawn); they spawn at the first
/// turn the queue is free again.
#[test]
fn a_blockaded_port_holds_its_ships_and_a_siege_its_land_units() {
    let left = |blockaded: bool, besieged: bool| {
        let mut m = test_model();
        let mut rules = (*m.rules).clone();
        let mut ship = rules.units["test_unit"].clone();
        ship.is_naval = true;
        rules.units.insert("test_ship".into(), ship);
        rules.allow_unit_for_all("test_ship");
        rules.buildings.get_mut("test_building_level").unwrap().units_allowed.push("test_ship".into());
        m.rules = Arc::new(rules);
        add_navy(&mut m, NAVY, 109, B, pos(50, 50));
        m.world.set_stance(A, B, Stance::War).unwrap();
        let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
        (r.slots[1].port, r.slots[1].position) = (true, Some(pos(5, 5)));
        r.slots[1].blockade = blockaded.then_some(PortBlockade { navy: NAVY, docked_navy_attacked: false });
        if besieged {
            // Garrisoned, with turns to hold, so that B's turn (before A's) does not take it (`0x008F9160`).
            r.garrison = Some(ForceId(1000));
            r.siege = SiegeState { besiegers: vec![ForceId(1001)], surrender_turns: 4, turns_besieged: 0, ..Default::default() };
        }
        for (id, unit_key) in [(9001, "test_unit"), (9002, "test_ship")] {
            let item = RecruitmentItem { id: RecruitmentItemId(id), unit_key: unit_key.into(), turns_remaining: 0, cost: 0, target: None, target_lost: false };
            r.recruitment_queue.push(item);
        }
        m.turn.humans = vec![A];
        m.start_campaign();
        let queued = |m: &CampaignModel| -> Vec<String> { m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.unit_key.clone()).collect() };
        let held = queued(&m);
        m.world.regions.get_mut(&RegionId(10)).unwrap().siege.besiegers.clear();
        m.lift_port_blockade(RegionId(10), 1);
        m.end_turn();
        (held, queued(&m).is_empty())
    };
    assert_eq!(left(false, false), (vec![], true));
    assert_eq!(left(true, false), (vec!["test_ship".to_owned()], true), "the blockaded port holds its ship");
    assert_eq!(left(false, true), (vec!["test_unit".to_owned()], true), "the besieged settlement holds its unit");
}

/// The besieging general's next order ends his siege (`0x008FF470` → `0x00A0B8C0`), except the settlement
/// order onto the besieged settlement itself (`0x0095D140` runs the helper with its flag clear).
#[test]
fn the_besiegers_next_order_ends_his_siege_unless_he_enters_the_besieged_settlement() {
    let besieged = |m: &CampaignModel| m.world.regions[&RegionId(10)].siege.under_siege();
    let mut m = test_model();
    m.world.set_stance(A, B, Stance::War).unwrap();
    m.world.regions.get_mut(&RegionId(10)).unwrap().siege.besiegers = vec![ForceId(1001)];
    // No action points: neither order goes anywhere, and neither starts a battle.
    m.world.characters.get_mut(&CharacterId(101)).unwrap().movement_points = 0;
    let _ = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1001), region: RegionId(10) });
    assert!(besieged(&m), "the settlement order onto the besieged settlement keeps the siege");
    let _ = m.apply(CampaignCommand::MoveForce { force: ForceId(1001), to: pos(60, 60) });
    assert!(!besieged(&m), "any other order ends it");
}

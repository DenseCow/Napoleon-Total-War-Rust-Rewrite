//! Campaign model tests: buildings.

use super::*;

#[test]
fn capture_rolls_the_fortification_after_the_town_buildings() {
    use super::capture::{damage_roll, CaptureChoice, LOOT_DAMAGE, OCCUPY_DAMAGE};
    let mut m = capture_model(vec![A]);
    m.world.regions.get_mut(&RegionId(11)).unwrap().fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    let mut rng = m.rng;
    damage_roll(&mut rng, 100, 300, LOOT_DAMAGE, (4, 15_000));
    let (fort_loot, _) = damage_roll(&mut rng, 100, 300, LOOT_DAMAGE, (4, 15_000));
    damage_roll(&mut rng, 100, 300, OCCUPY_DAMAGE, (4, 15_000));
    let (fort_occ, _) = damage_roll(&mut rng, 100, 300, OCCUPY_DAMAGE, (4, 15_000));
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(m.rng, rng);
    let p = m.pending_capture.clone().unwrap();
    assert_eq!((p.loot.fortification, p.occupy.fortification), (Some(fort_loot), Some(fort_occ)));
    m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].fortification.as_ref().unwrap().health, fort_occ);
}

#[test]
fn allies_are_called_into_the_war() {
    use super::treaties::{slot, DiplomaticAction as D};
    let mut m = test_model();
    // C needs land to be in the game.
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    let act = |m: &mut CampaignModel, a, b, action| m.apply(CampaignCommand::Diplomacy { a, b, action }).unwrap();
    act(&mut m, B, C, D::Alliance);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    // C (AI, allied to B) joins B's war against A, dragged in by its ally.
    assert_eq!(m.world.stance(C, A), Stance::War);
    let r = &m.world.relationships[&(C, A)];
    assert_eq!((r.war_ally, r.attitudes[slot("war")].value), (Some(B), -70));
    assert_eq!(m.world.relationships[&(C, B)].allied_in_war_against.len(), 1);
    assert_eq!(m.world.relationships[&(C, B)].military_access_turns, -1);
    // An ally of both sides refuses the attacked side (breaking that alliance), then is called by the attacker.
    let mut m = test_model();
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    act(&mut m, B, C, D::Alliance);
    act(&mut m, A, C, D::Alliance);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    assert_eq!((m.world.stance(C, B), m.world.stance(C, A)), (Stance::War, Stance::Allied));
    assert!(m.world.relationships[&(B, C)].attitudes[slot("alliance_broken")].value < 0);
}

#[test]
fn fortifications_are_built_and_repaired() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 1000, turns: 2, upgrades_to: vec!["test_keep".into()], ..Default::default() });
    rules.buildings.insert("test_keep".into(), BuildingRules { chain: "test_fort".into(), level: 1, cost: 3000, turns: 3, ..Default::default() });
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    let t = m.world.factions[&A].treasury;
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 4000);
    let _ = t;
    m.end_turn();
    m.end_turn();
    assert_eq!(m.world.regions[&RegionId(10)].fortification.as_ref().map(|b| b.level_key.as_str()), Some("test_walls"));
    assert!(m.can_build(RegionId(10), SlotRef::Walls, "test_keep").is_ok(), "intact walls can be upgraded");
    // Damaged to 40: repair costs round(1000 × 0.6) and takes floor(0.6 × 2) = 1 turn.
    m.world.regions.get_mut(&RegionId(10)).unwrap().fortification.as_mut().unwrap().health = 40;
    // ... and the command refuses to upgrade it: one rule for the panel and the command.
    assert!(m.can_build(RegionId(10), SlotRef::Walls, "test_keep").is_err(), "no upgrade of damaged walls");
    // An AI owner repairs its damaged walls at its turn start (`ai_repairs`), so they can be
    // upgraded again afterwards.
    {
        let mut ai = m.clone();
        ai.turn.humans = Vec::new();
        ai.world.factions.get_mut(&A).unwrap().treasury = 5000;
        ai.ai_repairs(A);
        assert_eq!(ai.world.regions[&RegionId(10)].construction.last().map(|c| c.slot), Some(SlotRef::Walls), "the AI repairs its walls");
    }
    assert!(m.can_repair(RegionId(10), SlotRef::Walls));
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Walls), 600);
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.turn.humans = vec![A];
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Walls }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 4400);
    assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| (c.slot, c.turns_remaining)), Some((SlotRef::Walls, 1)));
    // The repair's length is the model's one rule, also while it runs (the health stays until it ends).
    assert_eq!(m.repair_turns(RegionId(10), SlotRef::Walls), 1);
    m.world.regions.get_mut(&RegionId(10)).unwrap().fortification.as_mut().unwrap().health = 0;
    assert_eq!(m.repair_turns(RegionId(10), SlotRef::Walls), 2);
}

/// A damaged road is never repaired (PROVISIONAL: the exe's road repair is not traced, nothing
/// damages a road, and the original's infrastructure panel hides the repair button).
#[test]
fn a_road_is_not_repaired() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    m.world.regions.get_mut(&RegionId(10)).unwrap().road = Some(BuildingRef { level_key: "test_building_level".into(), health: 40 });
    assert!(!m.can_repair(RegionId(10), SlotRef::Road));
    assert!(matches!(m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Road }), Err(CommandError::CannotBuild(_))));
    assert!(m.world.regions[&RegionId(10)].construction.is_empty());
    assert!(!m.can_demolish(RegionId(10), SlotRef::Road), "nor demolished");
}

/// A construction item for a slot the region does not have (review: it never progressed, kept the
/// region `constructing` for good and the AI skipped the region) is dropped at the owner's construction step,
/// while the items of real slots go on. The drop is reported once (a diagnostic event the app logs, never
/// sent to the scripts; review: it was silent).
#[test]
fn a_construction_item_for_a_missing_slot_is_dropped() {
    let mut m = test_model();
    let item = |slot| ConstructionItem { slot, level_key: "test_building_level".into(), turns_remaining: 3, cost: 0, kind: ConstructionKind::Build, active: true };
    m.world.regions.get_mut(&RegionId(10)).unwrap().construction = vec![item(SlotRef::Slot(99)), item(SlotRef::Slot(1))];
    let events = m.end_turn();
    let left: Vec<(SlotRef, u32)> = m.world.regions[&RegionId(10)].construction.iter().map(|c| (c.slot, c.turns_remaining)).collect();
    assert_eq!(left, vec![(SlotRef::Slot(1), 2)]);
    let dropped: Vec<&CampaignEvent> = events.iter().filter(|e| e.is_diagnostic()).collect();
    assert_eq!(dropped, vec![&CampaignEvent::ConstructionItemDropped { region: RegionId(10), slot: SlotRef::Slot(99), level_key: "test_building_level".into() }]);
    assert_eq!(dropped[0].script_name(), None);
    assert!(m.end_turn().iter().all(|e| !e.is_diagnostic()), "reported once");
}

/// Cancelling names a queued item by its id, so two cancels issued before the queue is shown
/// again remove exactly those two items (review: by queue position, cancelling A then B removed C).
#[test]
fn recruitment_cancels_name_their_item() {
    let mut m = test_model();
    m.world.factions.get_mut(&A).unwrap().treasury = 10_000;
    for _ in 0..3 {
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    }
    let ids: Vec<RecruitmentItemId> = m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.id).collect();
    assert!(ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2], "{ids:?}");
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: ids[0] }).unwrap();
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: ids[1] }).unwrap();
    let left: Vec<RecruitmentItemId> = m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.id).collect();
    assert_eq!(left, vec![ids[2]]);
    assert_eq!(
        m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: ids[0] }),
        Err(CommandError::UnknownRecruitmentItem(ids[0])),
        "a cancelled item is gone"
    );
}

/// The state hash tells construction slots apart without sentinel indices (review: `Slot(i)` was
/// hashed as `i as u32`, with `u32::MAX` / `u32::MAX - 1` standing for the road and the walls).
#[test]
fn the_state_hash_tells_construction_slots_apart() {
    let m = test_model();
    let hash = |slot| {
        let mut m = m.clone();
        let item = ConstructionItem { slot, level_key: "test_building_level".into(), turns_remaining: 1, cost: 0, kind: ConstructionKind::Build, active: true };
        m.world.regions.get_mut(&RegionId(10)).unwrap().construction = vec![item];
        m.state_hash()
    };
    let hashes = [hash(SlotRef::Slot(0)), hash(SlotRef::Slot(u32::MAX as usize)), hash(SlotRef::Slot(u32::MAX as usize - 1)), hash(SlotRef::Walls), hash(SlotRef::Road)];
    for (i, a) in hashes.iter().enumerate() {
        assert!(hashes[i + 1..].iter().all(|b| a != b), "slot {i} collides: {hashes:?}");
    }
    // The walls building counts too.
    let mut walled = m.clone();
    walled.world.regions.get_mut(&RegionId(10)).unwrap().fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    assert_ne!(walled.state_hash(), m.state_hash());
    // So does the item's active byte (review: it was left out of the hash).
    let active = |on| {
        let mut m = m.clone();
        let item = ConstructionItem { slot: SlotRef::Walls, level_key: "test_building_level".into(), turns_remaining: 1, cost: 0, kind: ConstructionKind::Build, active: on };
        m.world.regions.get_mut(&RegionId(10)).unwrap().construction = vec![item];
        m.state_hash()
    };
    assert_ne!(active(true), active(false));
}

/// construction2: `DemolishBuilding` / `DemolishFort` (exe queue ids `0x84` / `0x89`) queue a demolition
/// (`0x00B286C0` → `0x00AE6ED0`: one turn, cost 0), after cancelling the slot's item with its refund; the guard
/// (`0x00B16540`) wants a standing building in a slot the owner holds, not the road. At the round end the
/// demolition (`0x00B781B0`) empties the slot and stops its school's research.
#[test]
fn buildings_and_forts_are_demolished() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    let r10 = RegionId(10);
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        r.slots[0].building.as_mut().unwrap().health = 40;
        r.slots[0].id = 77;
        r.fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
        r.road = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    }
    m.world.faction_details.entry(A).or_default().research.insert("made_up_tech".into(), super::details::TechResearch { progress: 5.0, researcher: 77, traded: 0 });
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    // A repair is queued and paid; the demolition cancels it with its refund, then queues itself.
    m.apply(CampaignCommand::RepairBuilding { region: r10, slot: SlotRef::Slot(0) }).unwrap();
    assert!(m.world.factions[&A].treasury < 5000);
    assert!(m.can_demolish(r10, SlotRef::Slot(0)), "an item in the slot does not block it");
    assert_eq!(m.apply(CampaignCommand::DemolishBuilding { region: r10, slot: SlotRef::Slot(0) }).unwrap(), Vec::new());
    assert_eq!(m.world.factions[&A].treasury, 5000);
    m.apply(CampaignCommand::DemolishBuilding { region: r10, slot: SlotRef::Walls }).unwrap();
    let demolish = |slot| ConstructionItem { slot, level_key: String::new(), turns_remaining: 1, cost: 0, kind: ConstructionKind::Demolish, active: true };
    assert_eq!(m.world.regions[&r10].construction, vec![demolish(SlotRef::Slot(0)), demolish(SlotRef::Walls)]);
    assert!(m.world.regions[&r10].slots[0].building.is_some(), "queued, not done");
    // The road, an empty slot, another faction's region, an unknown region.
    assert!(!m.can_demolish(r10, SlotRef::Road));
    m.world.regions.get_mut(&r10).unwrap().slots[1].building = None;
    assert_eq!(
        m.apply(CampaignCommand::DemolishBuilding { region: r10, slot: SlotRef::Slot(1) }),
        Err(CommandError::CannotBuild("nothing to demolish in that slot".into()))
    );
    assert_eq!(m.apply(CampaignCommand::DemolishBuilding { region: RegionId(11), slot: SlotRef::Slot(0) }), Err(CommandError::NotYourTurn(B)));
    assert_eq!(m.apply(CampaignCommand::DemolishBuilding { region: RegionId(99), slot: SlotRef::Slot(0) }), Err(CommandError::UnknownRegion(RegionId(99))));
    // The round end: both slots empty, the school's research stopped with its progress kept.
    let events = m.end_turn();
    let r = &m.world.regions[&r10];
    assert!(r.slots[0].building.is_none() && r.fortification.is_none() && r.construction.is_empty());
    assert!(r.road.is_some());
    assert_eq!(m.world.faction_details[&A].research["made_up_tech"], super::details::TechResearch { progress: 5.0, researcher: 0, traded: 0 });
    assert!(!events.iter().any(|e| matches!(e, CampaignEvent::BuildingCompleted { .. })));
}

/// The walls card's options: the land option rule (`0x00B43300`) on the fortification slot, an
/// ordinary building slot in the exe (CONFIRMED, `0x00A01F50` lists it with the other slots).
#[test]
fn the_walls_slot_options_follow_the_land_rule() {
    let walls = |m: &CampaignModel| -> Vec<String> { m.construction_options(RegionId(10), SlotRef::Walls).into_iter().map(|o| o.level_key).collect() };
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 1000, turns: 2, ..Default::default() });
    rules.buildings.insert("test_keep".into(), BuildingRules { chain: "test_fort".into(), level: 1, cost: 3000, turns: 3, ..Default::default() });
    rules.buildings.get_mut("test_walls").unwrap().upgrades_to = vec!["test_keep".into()];
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    m.turn = TurnState::in_turn_of(A, vec![A]);
    // An empty fortification slot: the level 0 of the fort chain, nothing else. `test_building_level`
    // is level 0 of `test_chain`, whose slots are `test_slot`, so it is not offered here.
    assert_eq!(walls(&m), vec!["test_walls".to_string()]);
    assert_eq!(
        m.construction_options(RegionId(10), SlotRef::Walls),
        vec![ConstructionOption { level_key: "test_walls".into(), cost: 1000, turns: 2, affordable: true, tech: true }]
    );
    // `affordable` follows the treasury: the owner starts with 1000, so 999 is out of reach.
    m.world.factions.get_mut(&A).unwrap().treasury = 999;
    assert!(!m.construction_options(RegionId(10), SlotRef::Walls)[0].affordable, "999 in the treasury");
    m.world.factions.get_mut(&A).unwrap().treasury = 5000;
    assert!(m.construction_options(RegionId(10), SlotRef::Walls)[0].affordable);
    assert!(m.construction_options(RegionId(99), SlotRef::Walls).is_empty());
    // While the slot has a construction item there are no options at all.
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).unwrap();
    assert!(walls(&m).is_empty());
    for _ in 0..8 {
        if m.world.regions[&RegionId(10)].fortification.is_some() {
            break;
        }
        m.end_turn();
    }
    // A standing undamaged fort offers only its upgrades, and no new fort.
    assert_eq!(m.world.regions[&RegionId(10)].fortification.as_ref().map(|b| b.level_key.as_str()), Some("test_walls"));
    assert_eq!(walls(&m), vec!["test_keep".to_string()]);
    // A damaged fort has no options at all (the construction panel lists upgrades only at full health).
    m.world.regions.get_mut(&RegionId(10)).unwrap().fortification.as_mut().unwrap().health = 40;
    assert!(walls(&m).is_empty());
    assert!(m.construction_options(RegionId(10), SlotRef::Walls).is_empty());
}

/// The scripts' restricted levels are the model's (`World::restricted_buildings`), and the
/// permission test `0x008BE7F0` rejects them for the options and the construct command alike
/// (`CCQ_BUILDING_CONSTRUCT` `0x00931C80` → `0x00B13D50` starts only a level among the slot's
/// options; CONFIRMED). Review: `BeginConstruction` with a restricted key used to build it.
#[test]
fn a_restricted_level_is_neither_offered_nor_built() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_walls".into(), BuildingRules { chain: "test_fort".into(), level: 0, cost: 100, turns: 2, ..Default::default() });
    rules.chain_slots.insert("test_fort".into(), vec!["settlement_fortification".into()]);
    m.rules = Arc::new(rules);
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.restricted_buildings.insert("test_walls".into());
    assert!(m.construction_options(RegionId(10), SlotRef::Walls).is_empty());
    assert!(!m.building_permitted(A, "test_walls"));
    let t = m.world.factions[&A].treasury;
    let r = m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() });
    assert_eq!(r, Err(CommandError::CannotBuild("test_walls".into())));
    assert!(m.world.regions[&RegionId(10)].construction.is_empty());
    assert_eq!(m.world.factions[&A].treasury, t);
    // `remove_restricted_building_level_record` lifts it.
    m.world.restricted_buildings.remove("test_walls");
    assert!(m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Walls, level_key: "test_walls".into() }).is_ok());
}

/// `FindNextFortUpgradeLevel` (`0x00B430C0`, CONFIRMED) works from the fort's own level index
/// (`+0x188`), never a record lookup of the standing level: the next level is the first record of
/// index + 1, none above the top or when that record is restricted. Review: a standing key missing
/// from the rules used to give no upgrade; the index now comes with the standing level.
#[test]
fn the_map_fort_upgrade_follows_the_level_index() {
    let mut m = test_model();
    let levels = vec![(0, "f0".to_string()), (1, "f1".to_string()), (1, "g1".to_string()), (2, "f2".to_string())];
    assert_eq!(m.map_fort_next_level(0, &levels), Some("f1"));
    assert_eq!(m.map_fort_next_level(1, &levels), Some("f2"));
    assert_eq!(m.map_fort_next_level(2, &levels), None);
    m.world.restricted_buildings.insert("f1".into());
    assert_eq!(m.map_fort_next_level(0, &levels), None, "a restricted record is not passed over for g1");
    // No fort record: the standing level is index 0 of the list, with its index.
    assert_eq!(m.map_fort_standing(RegionId(10), &levels), Some((0, "f0".to_string())));
}
#[test]
fn naval_engagement_and_battle() {
    use super::naval::{engagement, resolve, NavalPair, NavalUnit, NavalVars, ShipRules, ShipState};
    use super::autoresolve::PairResult;
    // Equal ships fire at the same rate: B (second to fire in each step's crew update) never breaks first.
    let p = NavalPair { crew_a: 200.0, crew_b: 200.0, hull_a: 3000.0, hull_b: 3000.0, dmg_a: 0.0, dmg_b: 0.0, sink_a: 0.6, sink_b: 0.6 };
    let s = engagement(&p, 50.0, 80.0, 50.0, 80.0, 0);
    assert_eq!(s.result, PairResult::Draw);
    // A much stronger A wins: B's hull reaches its sink point (1 − 0.6) × 3000 first.
    let s = engagement(&p, 200.0, 80.0, 20.0, 80.0, 0);
    assert_eq!((s.result, s.hull_b), (PairResult::AWins, 1.0));
    let ship = |guns: u32| NavalUnit {
        state: ShipState { crews: [30, 30, 144], max_crews: [30, 30, 144], guns, max_guns: guns, ..Default::default() },
        rules: ShipRules { crews: [30, 30, 144], hull: 3300.0, sink_weight: 0.45, morale: 10.0, fire: [40, 15], guns: 74, range: 3, ..Default::default() },
        human: false,
        force: 0,
    };
    let mut rules = CampaignRules::test_rules();
    for (k, v) in [
        ("autoresolve_gaussian_boundary", 3.0), ("autoresolve_gaussian_standard_deviation", 1.0), ("autoresolve_min_combat_potential_only_win_chance", 0.5),
        ("autoresolve_minimum_win_chance_to_win", 0.225), ("autoresolve_advantage_over_enemy_wipeout_threshold", 0.6), ("autoresolve_stat_massacre_chance", 0.2),
        ("autoresolve_normal_naval_victory_win_percent", 0.16), ("autoresolve_normal_naval_victory_lose_percent", 0.2), ("autoresolve_ship_damage_fuzziness", 0.3),
        ("autoresolve_ship_damage_required_for_sink", 0.875),
    ] {
        rules.variables.insert(k.into(), v);
    }
    let v = NavalVars::from_rules(&rules);
    // Three 74s against one: the big side wins and the lone ship is wiped out (advantage 2/3 > 0.6).
    let mut a = vec![ship(74), ship(74), ship(74)];
    let mut b = vec![ship(74)];
    let mut rng = CaRng::new(7);
    let o = resolve(&mut a, &mut b, &v, &mut rng);
    assert!(o.a_won && o.probabilities[0] >= 0.5 && o.probabilities[0] > o.probabilities[1], "{o:?}");
    assert!(b[0].state.sunk && b[0].state.crew() == 0);
    assert!(a.iter().all(|u| !u.state.sunk));
    // Same inputs, same result.
    let (mut a2, mut b2) = (vec![ship(74), ship(74), ship(74)], vec![ship(74)]);
    assert_eq!(resolve(&mut a2, &mut b2, &v, &mut CaRng::new(7)), o);
    assert_eq!(a2, a);
}

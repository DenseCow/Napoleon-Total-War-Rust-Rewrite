//! Campaign model tests: slots exit.

use super::*;

/// `0x00B73030`: the wait for a training place, from the queue's turns left.
#[test]
fn the_queue_wait_counts_down_the_training_places() {
    use super::commander_recruitment::queue_wait;
    // Below capacity: no wait; at capacity: the shortest item.
    assert_eq!(queue_wait(2, &[]), 0);
    assert_eq!(queue_wait(2, &[5]), 0);
    assert_eq!(queue_wait(2, &[5, 3]), 3);
    // Above: the first two count down; after 2 turns the 2 leaves and the queue is at capacity (4 - 2, 1).
    assert_eq!(queue_wait(2, &[4, 2, 1]), 2 + 1);
    // Several leave at once and the queue drops below capacity.
    assert_eq!(queue_wait(2, &[1, 1, 3]), 1);
    // Only the first `capacity` items count down: the 9 behind does not until it is first.
    assert_eq!(queue_wait(1, &[2, 9]), 2 + 9);
}

/// A commander's options (`0x00B72DF0`): each unit once, from the source with the shortest training plus
/// march; the march is the path cost over `units` #9; a farther source only competes when it is the first
/// (the search for the next ones is cut at the first one's path cost).
#[test]
fn a_commander_recruits_each_unit_from_its_quickest_source() {
    let mut m = commander_recruitment_model();
    let speed = m.rules.units["test_recruit"].travel_speed as f32;
    let r = m.commander_recruitment(A, CharacterId(100));
    assert_eq!(r.options.len(), 2, "{r:?}");
    let o = option(&r, "test_recruit");
    // Region 10 comes first (merge order: equal flags and cost, lower region); 13 is nearer and as quick.
    assert_eq!(o.region, RegionId(13));
    assert_eq!(o.training_turns, m.rules.units["test_recruit"].turns as i32);
    assert!((o.travel_turns - 7.0 / speed).abs() < 1e-6, "{o:?}");
    assert!(o.available());
    // A busy queue in 13 (both places taken for 5 turns): 10's 2 + 10/30 turns beat 13's 5 + 2 + 7/30.
    for id in [9100, 9101] {
        m.world.regions.get_mut(&RegionId(13)).unwrap().recruitment_queue.push(RecruitmentItem {
            id: RecruitmentItemId(id),
            unit_key: "test_unit".into(),
            turns_remaining: 5,
            cost: 0,
            target: None,
            target_lost: false,
        });
    }
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.training_turns), (RegionId(10), 2), "{o:?}");
    assert!((o.travel_turns - 10.0 / speed).abs() < 1e-6, "{o:?}");
}

/// ORIGINAL BUG (`0x00B68FE0` over `0x00B61D50`): a source whose queue trains nothing (no recruitment
/// points) has no training estimate (-1), which the exe takes as the shortest time, so its queue would get
/// the item and never train it. Ours ranks it after every source that can train.
#[test]
fn a_source_without_recruitment_points_is_not_chosen() {
    let mut m = commander_recruitment_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_no_points".into(), BuildingRules { chain: "test_chain_2".into(), units_allowed: vec!["test_recruit".into()], ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(13)).unwrap().slots[0].building = Some(BuildingRef { level_key: "test_no_points".into(), health: 100 });
    assert_eq!(m.recruitment_points(RegionId(13), false), 0);
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.training_turns), (RegionId(10), 2), "{o:?}");
}

/// The commander's queue is the items queued for him (item +0x18, `0x00B26020`); ten of them refuse every
/// option as queue-full (`0x00B1BBB0`), whatever the source queues hold. The recruit command keeps the
/// target and the panel names the source region as the card's manager.
#[test]
fn items_queued_through_a_commander_are_his_queue() {
    use super::commands::ENTRY_QUEUE_FULL;
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    m.apply(CampaignCommand::Recruit { region: RegionId(13), unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    let item = m.world.regions[&RegionId(13)].recruitment_queue[0].clone();
    assert_eq!(item.target, Some(general));
    let r = m.commander_recruitment(A, general);
    assert_eq!(r.queue, vec![(RegionId(13), item.id)]);
    assert!(r.options.iter().all(|o| o.flags & ENTRY_QUEUE_FULL == 0));
    // Ten for him, spread over both regions (neither queue is full on its own).
    for i in 0..9 {
        let region = if i % 2 == 0 { RegionId(10) } else { RegionId(13) };
        m.apply(CampaignCommand::Recruit { region, unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    }
    let r = m.commander_recruitment(A, general);
    assert_eq!(r.queue.len(), 10);
    assert!(r.options.iter().all(|o| o.flags & ENTRY_QUEUE_FULL != 0 && !o.available()), "{r:?}");
}

/// When the commander an item was queued through dies, the item stays queued and unrefunded and only loses
/// its target (item listener `0x00B57F40`, notified from the character destructor `0x0099D2D0`).
#[test]
fn a_dead_commanders_queued_items_stay_and_lose_their_target() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    m.apply(CampaignCommand::Recruit { region: RegionId(13), unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    let treasury = m.world.factions[&A].treasury;
    let own = m.world.regions[&RegionId(10)].recruitment_queue.clone();
    let mut item = m.world.regions[&RegionId(13)].recruitment_queue[0].clone();
    m.character_dies(general);
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue, own, "an untargeted item is untouched");
    assert!(!item.target_lost);
    // Both +0x18 and +0x24 are cleared (`0x00B57F40`).
    item.target = None;
    item.target_lost = true;
    assert_eq!(m.world.regions[&RegionId(13)].recruitment_queue, vec![item]);
    assert_eq!(m.world.factions[&A].treasury, treasury, "no refund");
}

/// ORIGINAL BUG (`0x00B0F2B0` refuses a path whose start is its goal): a general inside a settlement
/// recruits its units with no march; the exe flagged them as having no path.
#[test]
fn a_garrisoned_general_recruits_his_settlements_units_without_a_march() {
    let mut m = commander_recruitment_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(10));
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.travel_turns, o.flags), (RegionId(10), 0.0, 0), "{o:?}");
}

/// The same ORIGINAL BUG in the other order: the untrainable source is the first and the nearer one.
/// The exe cuts the next source's search at the first one's path cost and so refuses the farther,
/// trainable region; ours searches without the cut while the best cannot train.
#[test]
fn a_nearer_first_source_without_recruitment_points_loses_to_a_farther_one() {
    let mut m = commander_recruitment_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(0, 0);
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("test_no_points".into(), BuildingRules { chain: "test_chain_2".into(), units_allowed: vec!["test_recruit".into()], ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building = Some(BuildingRef { level_key: "test_no_points".into(), health: 100 });
    assert_eq!(m.recruitment_points(RegionId(10), false), 0);
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert_eq!((o.region, o.training_turns), (RegionId(13), 2), "{o:?}");
}

/// `units` #9 divides the march (`0x00B41F60`, unguarded in the exe); a mod's 0 is read as 1, not a
/// division by zero.
#[test]
fn a_march_speed_of_zero_is_read_as_one() {
    let mut m = commander_recruitment_model();
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_recruit").unwrap().travel_speed = 0;
    m.rules = Arc::new(rules);
    let r = m.commander_recruitment(A, CharacterId(100));
    let o = option(&r, "test_recruit");
    assert!((o.travel_turns - 7.0).abs() < 1e-6 && o.travel_turns_rounded() == 7, "{o:?}");
}

#[test]
fn model_equality_ignores_the_unsaved_negotiation_slot() {
    let a = test_model();
    let mut b = a.clone();
    b.negotiations.begun = 3;
    b.negotiations.ended = 1;
    assert_eq!(a, b, "the negotiation slot is neither saved nor hashed");
    assert_eq!(a.state_hash(), b.state_hash());
    b.deal_inflation.first += 1;
    assert_ne!(a, b, "saved state still counts");
}

/// An army's options hold no ships and a navy's no land units: each queue's list is cut to its kind
/// (`0x00AE5110`, the land list by `0x00B64E00`, the port's by `0x00B64DD0`), even where one building
/// allows both.
#[test]
fn an_army_is_offered_no_ships() {
    let mut m = commander_recruitment_model();
    let mut rules = (*m.rules).clone();
    let mut ship = rules.units["test_recruit"].clone();
    ship.is_naval = true;
    rules.units.insert("test_ship".into(), ship);
    rules.allow_unit_for_all("test_ship");
    rules.buildings.get_mut("test_building_level").unwrap().units_allowed.push("test_ship".into());
    m.rules = Arc::new(rules);
    assert!(m.recruitable_units(RegionId(13)).iter().any(|e| e.unit_key == "test_ship"));
    let r = m.commander_recruitment(A, CharacterId(100));
    let mut units: Vec<&str> = r.options.iter().map(|o| o.unit_key.as_str()).collect();
    units.sort_unstable();
    assert_eq!(units, ["test_recruit", "test_unit"], "{r:?}");
}

/// A faction's unit set (`0x00F74CB0`): its grouping's units plus its `allowed` rows, each denied row taking
/// out one copy; a unit no row names is not in it, and a faction without a set recruits nothing.
#[test]
fn a_factions_unit_set_is_its_grouping_and_rows_minus_one_copy_per_denial() {
    let set = super::rules::faction_unit_set(
        ["grouped", "grouped_denied", "twice"],
        [("own", true), ("twice", true), ("twice", false), ("grouped_denied", false), ("never", false)],
    );
    let got: Vec<&str> = set.iter().map(String::as_str).collect();
    assert_eq!(got, ["grouped", "own", "twice"]);
    let mut rules = CampaignRules::default();
    rules.faction_units.insert("f".into(), set);
    assert!(rules.faction_may_recruit("f", "own"));
    assert!(!rules.faction_may_recruit("f", "unnamed"));
    assert!(!rules.faction_may_recruit("other", "own"));
}

/// recruit-followups: the road and the walls are slots like the others (`0x00A4DCB0` builds them with the
/// same constructor, holder and all), merged after the ordinary slots by `0x00A6AE40` through `0x00A62380`:
/// their effects count and their units are listed only while the region's owner holds them, a held-by-other
/// one flagging its units occupied (`0x00B43CA0`). An owner change passes the old owner's holds to the new
/// one, theirs included.
#[test]
fn the_road_and_walls_count_only_while_the_owner_holds_them() {
    use super::commands::{RecruitableUnit, ENTRY_OCCUPIED};
    use super::world::SlotRef;
    let mut m = model_with_a_depot();
    let r10 = RegionId(10);
    let entry = |k: &str, flags: u32| RecruitableUnit { unit_key: k.into(), flags };
    let walls = BuildingRef { level_key: "test_depot".into(), health: 100 };
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        r.slots[1].building = None;
        r.fortification = Some(walls.clone());
        r.road = Some(BuildingRef { level_key: "test_road".into(), health: 100 });
    }
    let effect_keys = |m: &CampaignModel| m.world.regions[&r10].effect_buildings().map(|b| b.level_key.clone()).collect::<Vec<_>>();
    // Slots first, then the road, then the walls (the order `0x00A6AE40` merges them in).
    assert_eq!(effect_keys(&m), ["test_building_level", "test_road", "test_depot"]);
    assert_eq!(m.recruitable_units(r10), vec![entry("test_guard", 0), entry("test_recruit", 0), entry("test_unit", 0)]);
    // B holds the walls and the road: neither counts, and the walls' own unit is flagged.
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        (r.road_holder, r.fortification_holder) = (Some(B), Some(B));
        assert!(!r.slot_held(SlotRef::Road) && !r.slot_held(SlotRef::Walls));
    }
    assert_eq!(effect_keys(&m), ["test_building_level"]);
    assert_eq!(m.recruitable_units(r10), vec![entry("test_guard", ENTRY_OCCUPIED), entry("test_recruit", 0), entry("test_unit", 0)]);
    // Holding one's own road and walls is no occupation.
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        (r.road_holder, r.fortification_holder) = (Some(A), Some(A));
    }
    assert_eq!(effect_keys(&m), ["test_building_level", "test_road", "test_depot"]);
    // The region passes to C: the road and the walls always pass to the new owner, B's hold too (`0x00B1B300`).
    m.world.regions.get_mut(&r10).unwrap().fortification_holder = Some(B);
    assert!(m.transfer_region(r10, C, &mut Vec::new()));
    let r = &m.world.regions[&r10];
    assert_eq!((r.road_holder, r.fortification_holder), (None, None));
    assert!(r.slot_held(SlotRef::Road) && r.slot_held(SlotRef::Walls));
}

/// recruit-followups: walls another faction holds (`0x00A91FC0`) can be neither repaired (`0x00B1A6B0`) nor
/// demolished (`0x009B7920`), and a construction item in the road or the walls stands still while another
/// faction holds that slot (`0x00B78800` steps an item only in a slot the region's owner holds). The owner's
/// own hold changes nothing.
#[test]
fn walls_and_road_held_by_another_faction_are_not_repaired_demolished_or_built() {
    let mut m = test_model();
    let r10 = RegionId(10);
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        r.fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 50 });
        r.fortification_holder = Some(B);
    }
    assert!(!m.can_repair(r10, SlotRef::Walls) && !m.can_demolish(r10, SlotRef::Walls));
    m.world.regions.get_mut(&r10).unwrap().fortification_holder = Some(A);
    assert!(m.can_repair(r10, SlotRef::Walls) && m.can_demolish(r10, SlotRef::Walls));

    // Builds, not repairs (a repair, an item of the standing level, steps whoever holds the slot).
    let item = |slot| ConstructionItem { slot, level_key: "test_building_level_2".into(), turns_remaining: 3, cost: 0, kind: ConstructionKind::Build, active: true };
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        (r.road_holder, r.fortification_holder) = (Some(B), Some(B));
        r.construction = vec![item(SlotRef::Road), item(SlotRef::Walls), item(SlotRef::Slot(1))];
    }
    let left = |m: &CampaignModel| m.world.regions[&r10].construction.iter().map(|c| (c.slot, c.turns_remaining)).collect::<Vec<_>>();
    m.end_turn();
    assert_eq!(left(&m), vec![(SlotRef::Road, 3), (SlotRef::Walls, 3), (SlotRef::Slot(1), 2)]);
    // Back in the owner's hands (its own faction named as holder): both advance again.
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        (r.road_holder, r.fortification_holder) = (Some(A), None);
    }
    m.end_turn();
    assert_eq!(left(&m), vec![(SlotRef::Road, 2), (SlotRef::Walls, 2), (SlotRef::Slot(1), 1)]);
}

/// construction: a repair (item type 2) counts down whoever holds its slot,
/// while a build in a slot another faction holds stands still (`0x00B78800` steps item types 2 and 3 in any
/// slot, the others only when the region's owner holds it). Both run at the round end (`0x008BC650` →
/// `0x00AB3DA0`), not at the owner's turn start.
#[test]
fn a_repair_counts_down_in_a_slot_another_faction_holds() {
    let mut m = test_model();
    let r10 = RegionId(10);
    m.turn.humans = vec![A];
    m.start_campaign();
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        r.slots[0].building.as_mut().unwrap().health = 50;
        (r.slots[0].holder, r.slots[1].holder) = (Some(B), Some(B));
        r.construction = vec![
            ConstructionItem { slot: SlotRef::Slot(0), level_key: "test_building_level".into(), turns_remaining: 2, cost: 0, kind: ConstructionKind::Repair { health_per_turn: 25, turns: 2 }, active: true },
            ConstructionItem { slot: SlotRef::Slot(1), level_key: "test_building_level".into(), turns_remaining: 2, cost: 0, kind: ConstructionKind::Build, active: true },
        ];
    }
    let left = |m: &CampaignModel| m.world.regions[&r10].construction.iter().map(|c| (c.slot, c.turns_remaining)).collect::<Vec<_>>();
    // Nothing moves before the round end: A's turn end and C's turn pass first (turn order B, A, C).
    m.begin_end_turn();
    while m.turn.queue.front().is_some_and(|s| !matches!(s, super::turn::TurnStep::Economy(_))) {
        m.step();
    }
    assert_eq!(left(&m), vec![(SlotRef::Slot(0), 2), (SlotRef::Slot(1), 2)]);
    // A step that does not finish adds the repair's step to the health (`0x00B78490`), and `0x00B452A0` posts
    // `CampaignBuildingDamaged` while it is below 100.
    let ev = m.run_pending();
    assert_eq!(left(&m), vec![(SlotRef::Slot(0), 1), (SlotRef::Slot(1), 2)]);
    assert_eq!(m.world.regions[&r10].slots[0].building.as_ref().map(|b| b.health), Some(75));
    assert!(ev.contains(&CampaignEvent::CampaignBuildingDamaged { region: r10, slot: SlotRef::Slot(0) }));
    // The last step sets 100: no `BuildingCompleted` (only the build steps post it), no damage event.
    let ev = m.end_turn();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::BuildingCompleted { .. } | CampaignEvent::CampaignBuildingDamaged { .. })));
    assert_eq!(m.world.regions[&r10].slots[0].building.as_ref().map(|b| b.health), Some(100));
    assert_eq!(left(&m), vec![(SlotRef::Slot(1), 2)]);
}

/// construction: the rebels' regions build nothing (`0x008BC650` skips the construction step for a faction with
/// no record, `0x008CEEF0`), while every other faction's items count down at the round end.
#[test]
fn the_rebels_construction_does_not_count_down() {
    let mut m = test_model();
    m.world.factions.get_mut(&C).unwrap().key = String::new();
    assert!(m.is_rebel_faction(C));
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    let item = |slot| ConstructionItem { slot, level_key: "test_building_level_2".into(), turns_remaining: 3, cost: 0, kind: ConstructionKind::Build, active: true };
    m.world.regions.get_mut(&RegionId(12)).unwrap().construction = vec![item(SlotRef::Walls)];
    m.world.regions.get_mut(&RegionId(10)).unwrap().construction = vec![item(SlotRef::Walls)];
    m.end_turn();
    assert_eq!(m.world.regions[&RegionId(12)].construction[0].turns_remaining, 3);
    assert_eq!(m.world.regions[&RegionId(10)].construction[0].turns_remaining, 2);
}

/// construction2: an inactive item (`ConstructionItem::active` false, ESF #1, +0x8) is not stepped
/// (`0x00B78800` tests the byte at `0x00B7880E`): it keeps its turns and its result never stands, while an
/// active item in the same region counts down and finishes.
#[test]
fn an_inactive_construction_item_is_not_stepped() {
    let mut m = test_model();
    let r10 = RegionId(10);
    let item = |slot, kind, active| ConstructionItem { slot, level_key: "test_building_level_2".into(), turns_remaining: 1, cost: 0, kind, active };
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        r.slots[0].building.as_mut().unwrap().health = 50;
        r.construction = vec![
            item(SlotRef::Slot(0), ConstructionKind::Repair { health_per_turn: 25, turns: 2 }, false),
            item(SlotRef::Slot(1), ConstructionKind::Build, false),
            item(SlotRef::Walls, ConstructionKind::Build, true),
        ];
    }
    let slot1 = m.world.regions[&r10].slots[1].building.clone();
    let ev = m.end_turn();
    let r = &m.world.regions[&r10];
    assert_eq!(r.construction.iter().map(|c| (c.slot, c.turns_remaining)).collect::<Vec<_>>(), vec![(SlotRef::Slot(0), 1), (SlotRef::Slot(1), 1)]);
    assert_eq!(r.slots[0].building.as_ref().map(|b| b.health), Some(50));
    assert_eq!(r.slots[1].building, slot1);
    assert_eq!(r.fortification.as_ref().map(|b| b.level_key.as_str()), Some("test_building_level_2"));
    let completed: Vec<_> = ev.iter().filter(|e| matches!(e, CampaignEvent::BuildingCompleted { .. })).collect();
    assert_eq!(completed, vec![&CampaignEvent::BuildingCompleted { region: r10, slot: SlotRef::Walls }]);
}

/// accessloss2: the round-end construction steps the faction's regions in its region list order
/// (faction +0x778, `0x008BC873`; `World::regions_in_list_order`), so the events come out in that order.
#[test]
fn construction_steps_the_faction_regions_in_list_order() {
    let completed = |order: [u32; 3]| {
        let mut m = test_model();
        m.world.region_order = order.map(RegionId).to_vec();
        let item = ConstructionItem { slot: SlotRef::Walls, level_key: "test_building_level_2".into(), turns_remaining: 1, cost: 0, kind: ConstructionKind::Build, active: true };
        for r in [10, 12] {
            m.world.regions.get_mut(&RegionId(r)).unwrap().construction = vec![item.clone()];
        }
        let ev = m.end_turn();
        ev.iter().filter_map(|e| match e { CampaignEvent::BuildingCompleted { region, .. } => Some(region.raw()), _ => None }).collect::<Vec<_>>()
    };
    assert_eq!(completed([10, 11, 12]), vec![10, 12]);
    assert_eq!(completed([12, 11, 10]), vec![12, 10], "not id order");
}

/// construction: a slot another faction holds offers no construction option and refuses the command
/// (`0x00B43300` exits at `0x00B4333B` when `0x00A91FC0` says the slot is held by another faction).
#[test]
fn a_slot_another_faction_holds_offers_no_construction() {
    let mut m = test_model();
    let r10 = RegionId(10);
    assert!(!m.construction_options(r10, SlotRef::Slot(1)).is_empty());
    m.world.regions.get_mut(&r10).unwrap().slots[1].holder = Some(B);
    assert!(m.construction_options(r10, SlotRef::Slot(1)).is_empty());
    let cmd = CampaignCommand::ConstructBuilding { region: r10, slot: SlotRef::Slot(1), level_key: "test_building_level".into() };
    assert!(matches!(m.apply(cmd.clone()), Err(CommandError::CannotBuild(_))));
    m.world.regions.get_mut(&r10).unwrap().slots[1].holder = Some(A);
    assert!(m.apply(cmd).is_ok());
}

/// construction: on an owner change every town / port slot passes to the new owner, a third faction's hold too,
/// except a port a navy is docked in, whose holder stays (`0x00B1B300`: `0x00B2B810(new, 0)` per slot unless
/// garrisoned or docked).
#[test]
fn an_owner_change_hands_every_slot_over_but_a_port_with_a_navy_docked() {
    let mut m = test_model();
    let r10 = RegionId(10);
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        (r.slots[0].port, r.slots[0].position) = (true, Some(pos(5, 5)));
        r.slots[1].holder = Some(B);
    }
    // B's navy docked at the port.
    let admiral = Character { id: CharacterId(109), faction: B, kind: CharacterKind::Admiral, position: pos(5, 5), movement_points: 0, max_movement_points: 0, base_movement_points: 0, garrisoned_in: None };
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: B, commander: Some(CharacterId(109)), units: vec![unit(9)], is_navy: true });
    assert!(m.transfer_region(r10, C, &mut Vec::new()));
    let r = &m.world.regions[&r10];
    assert_eq!((r.slots[0].holder, r.slots[1].holder), (Some(A), None), "the docked port stays the old owner's; B's slot passes");
    // Without the navy, a later change hands the port over too.
    m.world.forces.remove(&ForceId(1005));
    assert!(m.transfer_region(r10, B, &mut Vec::new()));
    let r = &m.world.regions[&r10];
    assert_eq!((r.slots[0].holder, r.slots[1].holder), (None, None));
}

/// The capital through owner changes (`0x008BA580` → `0x008DABC0`, `0x008AE270`): the loser picks the
/// nearest region whose settlement has 4 building slots before any nearer smaller one; regaining the
/// home region makes it the capital again; a gainer with a capital keeps it.
#[test]
fn a_lost_capital_moves_to_the_nearest_town_and_back_home() {
    let mut m = test_model();
    let mut town = region(13, A, 1000, 4);
    for (i, s) in town.slots.iter_mut().enumerate() {
        s.key = format!("settlement:test_region_13:town:test_slot:{i}");
    }
    town.settlement.position = pos(100, 0);
    m.world.regions.insert(town.id, town);
    set_details(&mut m, A, Some(10), false);
    set_details(&mut m, B, Some(11), false);
    assert!(m.transfer_region(RegionId(10), B, &mut Vec::new()));
    // Region 12 (2 away, no settlement slot) is nearer than region 13 (90 away, 4 slots).
    assert_eq!(m.world.capital(A), Some(RegionId(13)));
    assert_eq!(m.world.capital(B), Some(RegionId(11)));
    assert!(m.transfer_region(RegionId(10), A, &mut Vec::new()));
    assert_eq!(m.world.capital(A), Some(RegionId(10)), "the home region is the capital again");
    // Without a town of 4 slots the nearest region is taken.
    m.world.regions.remove(&RegionId(13));
    assert!(m.transfer_region(RegionId(10), B, &mut Vec::new()));
    assert_eq!(m.world.capital(A), Some(RegionId(12)));
}

/// A capture that takes an AI faction's last region ends it (`0x00A64AC0` → `0x008AB6C0`): out of the
/// game, its forces and characters gone, its relationships reset both ways and neutral, and the
/// event; it no longer counts as in the game.
#[test]
fn capturing_the_last_region_ends_an_ai_faction() {
    let mut m = capture_model(Vec::new());
    set_details(&mut m, A, Some(10), false);
    set_details(&mut m, B, Some(11), false);
    m.relationship_mut(A, B).trade_agreement = true;
    m.relationship_mut(B, C).military_access_turns = 3;
    m.world.set_stance(B, C, Stance::Allied).unwrap();
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::FactionDestroyed { faction: B }), "{ev:?}");
    assert_eq!(m.world.faction_details[&B].out_of_game, Some(true));
    assert!(!m.in_the_game(B));
    assert!(!m.world.forces.contains_key(&ForceId(1001)));
    assert!(!m.world.characters.contains_key(&CharacterId(101)));
    assert_eq!((m.world.stance(A, B), m.world.stance(B, C), m.world.stance(C, B)), (Stance::Neutral, Stance::Neutral, Stance::Neutral));
    assert!(!m.world.relationships[&(A, B)].trade_agreement);
    assert_eq!(m.world.relationships[&(B, C)].military_access_turns, 0);
    assert_eq!(m.world.capital(B), None);
    // The capturer's capital is kept.
    assert_eq!(m.world.capital(A), Some(RegionId(10)));
}

/// A human faction that loses its last region is out of the game but keeps its forces and
/// characters (`0x008AB6C0` deletes them for an AI faction only).
#[test]
fn a_human_faction_out_of_the_game_keeps_its_forces() {
    use super::capture::CaptureChoice;
    let mut m = capture_model(vec![A, B]);
    set_details(&mut m, B, Some(11), false);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Occupy }).unwrap();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::FactionDestroyed { .. })), "ended at the capture, before the choice");
    assert_eq!(m.world.faction_details[&B].out_of_game, Some(true));
    assert!(m.world.forces.contains_key(&ForceId(1001)));
    assert!(m.world.characters.contains_key(&CharacterId(101)));
}

/// A human faction destroyed ends the campaign (`0x008AB6C0` → `SetCampaignOverFlag` `0x008C30A0`):
/// from then on the turn loop runs nothing (`0x008F4120` returns at its flag test `0x008F419E`,
/// before the CAI step loop), so ending the turn plays no faction.
#[test]
fn a_destroyed_human_ends_the_campaign_and_the_turn_loop_stops() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    set_details(&mut m, A, Some(10), false);
    m.start_campaign();
    assert_eq!(m.turn.current, Some(A));
    for r in [10, 12] {
        m.world.regions.get_mut(&RegionId(r)).unwrap().owner = B;
    }
    m.destroy_faction(A, &mut Vec::new());
    assert!(m.turn.campaign_over && m.turn.queue.is_empty());
    let mut not_over = m.clone();
    not_over.turn.campaign_over = false;
    assert_ne!(m.state_hash(), not_over.state_hash(), "the flag is part of the state");
    for _ in 0..2 {
        assert_eq!(m.apply(CampaignCommand::EndTurn).unwrap(), Vec::new());
        assert_eq!((m.turn.current, m.turn.in_turn), (Some(A), true));
        assert!(!m.has_pending_steps());
    }
}

/// A human destroyed while an AI plays its turn (between steps, as the AI's capture of the human's
/// last region is) stops the loop at once: the queued rest of the round is dropped.
#[test]
fn a_human_destroyed_in_an_ai_turn_drops_the_rest_of_the_round() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    set_details(&mut m, A, Some(10), false);
    m.start_campaign();
    m.begin_end_turn();
    // Turn order B, A, C (C holds nothing): after A's turn end comes the round end, then B's (an AI's) turn.
    while m.turn.queue.front().is_some_and(|s| !matches!(s, super::turn::TurnStep::AiTurn(_))) {
        m.step();
    }
    assert!(m.has_pending_steps());
    for r in [10, 12] {
        m.world.regions.get_mut(&RegionId(r)).unwrap().owner = B;
    }
    m.destroy_faction(A, &mut Vec::new());
    assert!(m.turn.campaign_over);
    assert_eq!(m.step(), None);
    assert!(m.run_pending().is_empty());
}

/// Every listed human out of the game with the campaign not over (a save of ours from before the
/// flag was kept; the exe cannot reach it): the loop plays the AI factions left in the round, then
/// ends the campaign at the next round start (tested once per round) instead of playing AI rounds
/// forever.
#[test]
fn every_human_out_of_the_game_without_the_flag_ends_the_campaign() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    set_details(&mut m, A, Some(10), false);
    m.start_campaign();
    // Turn order B, A, C: C (now holding region 12) still plays after A this round; B does not start again.
    m.world.regions.get_mut(&RegionId(10)).unwrap().owner = B;
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.world.faction_details.get_mut(&A).unwrap().out_of_game = Some(true);
    assert!(!m.turn.campaign_over);
    let events = m.apply(CampaignCommand::EndTurn).unwrap();
    let started = |f: FactionId| events.iter().any(|e| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == f));
    assert!(started(C) && !started(B), "{events:?}");
    assert!(m.turn.campaign_over && !m.has_pending_steps());
    assert_eq!(m.apply(CampaignCommand::EndTurn).unwrap(), Vec::new());
}

/// A deal that hands over the giver's last region ends it (`0x00C18BF0` → `0x00B58A10(faction, 1, 0)`).
#[test]
fn a_deal_giving_away_the_last_region_ends_the_giver() {
    let mut m = test_model();
    // Both human: the recipient answers for itself (no AI refusal).
    m.turn.humans = vec![A, B];
    set_details(&mut m, B, Some(11), false);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    let ev = m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert!(ev.contains(&CampaignEvent::FactionDestroyed { faction: B }), "{ev:?}");
    assert_eq!(m.world.faction_details[&B].out_of_game, Some(true));
}

/// A liberation never ends the old owner and brings an out-of-game faction back
/// (`0x00A64AC0` with its second flag → `0x008AE6E0`): in the game again, the liberator's
/// protectorate, neutral to the others, each record's `initial_modifier` from its start attitude,
/// the alliance factor on its side only (the liberator's record is reset after the protectorate
/// is made, `0x00B2BB00`), the war caps set, and the region its capital.
#[test]
fn a_liberation_brings_the_faction_back_as_the_liberators_protectorate() {
    use super::capture::CaptureChoice;
    let mut m = capture_model(vec![A]);
    set_details(&mut m, A, Some(10), false);
    set_details(&mut m, C, None, true);
    m.world.region_rebel_factions.insert(RegionId(11), "test_faction_c".into());
    m.relationship_mut(C, A).start_attitude = 20;
    m.relationship_mut(A, C).start_attitude = 10;
    m.relationship_mut(C, B).start_attitude = 5;
    m.relationship_mut(C, A).factor_mut("trade").reset(7);
    m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert_eq!(m.pending_capture.as_ref().unwrap().liberate, Some(C));
    let ev = m.apply(CampaignCommand::ChooseCapture { choice: CaptureChoice::Liberate }).unwrap();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::FactionDestroyed { faction } if *faction == A)));
    assert_eq!(m.world.regions[&RegionId(11)].owner, C);
    assert_eq!(m.world.faction_details[&C].out_of_game, Some(false));
    assert!(m.in_the_game(C));
    assert_eq!((m.world.stance(C, A), m.world.stance(A, C)), (Stance::Protectorate, Stance::Patron));
    assert_eq!(m.world.stance(C, B), Stance::Neutral);
    let factor = |m: &CampaignModel, a, b, k| m.world.relationships[&(a, b)].factor(k).copied().unwrap_or_default();
    assert_eq!(factor(&m, C, A, "initial_modifier").value, 20);
    assert_eq!(factor(&m, A, C, "initial_modifier").value, 10);
    assert_eq!(factor(&m, C, B, "initial_modifier").value, 5);
    assert_eq!(factor(&m, C, A, "trade").value, 0, "every factor cleared first");
    assert_eq!(factor(&m, C, A, "alliance").value, 30);
    assert_eq!(factor(&m, A, C, "alliance").value, 0);
    let cap = factor(&m, C, B, "declared_war_against_enemies");
    assert_eq!((cap.cap, cap.capped), (15, true));
    let cap = factor(&m, A, C, "declared_war_against_friends");
    assert_eq!((cap.cap, cap.capped), (-15, true));
    assert_eq!(m.world.capital(C), Some(RegionId(11)));
}

/// The pirates on either side of a returning faction's record mean war (`0x00B2BC10`, `0x00B72AC0`).
#[test]
fn a_returning_faction_is_at_war_with_the_pirates() {
    let mut m = test_model();
    Arc::make_mut(&mut m.rules).director.no_rival_keys = vec!["test_faction_b".into()];
    set_details(&mut m, C, None, true);
    m.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
    let mut ev = Vec::new();
    assert!(m.liberate_region(RegionId(11), C, &mut ev));
    assert_eq!((m.world.stance(C, B), m.world.stance(B, C)), (Stance::War, Stance::War));
    let f = m.world.relationships[&(C, B)].factor("war").copied().unwrap();
    assert_eq!((f.value, f.drift, f.limit), (-140, -2, -200));
    assert_eq!(m.world.relationships[&(C, B)].factor("initial_modifier").unwrap().value, -140);
}

/// `in_the_game` follows the out-of-game flag (faction +0x824) when it is known, and the exe's startpos
/// rule (`0x008B4330`) for an own save from before the flag was kept.
#[test]
fn in_the_game_follows_the_out_of_game_flag() {
    let mut m = test_model();
    assert!(m.in_the_game(A) && !m.in_the_game(C));
    set_details(&mut m, A, Some(10), true);
    set_details(&mut m, C, None, false);
    assert!(!m.in_the_game(A) && m.in_the_game(C));
}

/// charflag: the exe's rule for faction +0x824 (`0x008B4330`): out of the game when the faction holds
/// no region and every character of it holds a government post (character +0x2B8); no character, out;
/// the rebels never. A save without the flag falls back on it (it was "a region or a force").
#[test]
fn the_out_of_game_rule_needs_no_region_and_only_post_holders() {
    use super::details::GovernmentPost;
    let mut m = test_model();
    assert!(!m.out_of_game_by_startpos_rule(A), "holds a region");
    assert!(m.out_of_game_by_startpos_rule(C) && !m.in_the_game(C), "no region, no character");
    m.world.characters.insert(CharacterId(300), character(300, C, CharacterKind::Minister, 0));
    assert!(!m.out_of_game_by_startpos_rule(C) && m.in_the_game(C), "a character in no post");
    m.world.faction_details.entry(C).or_default().posts =
        vec![GovernmentPost { id: 1, key: "faction_leader".into(), holder: Some(CharacterId(300)), governorship: None }];
    assert!(m.out_of_game_by_startpos_rule(C) && !m.in_the_game(C), "every character in a post");
    // A force without a character keeps no faction in the game (the old fallback's force).
    m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: C, commander: None, units: Vec::new(), is_navy: false });
    assert!(!m.in_the_game(C));
    m.world.regions.get_mut(&RegionId(11)).unwrap().owner = C;
    assert!(!m.out_of_game_by_startpos_rule(C), "a region");
    // The rebels are never out by the rule.
    m.world.factions.get_mut(&B).unwrap().key.clear();
    m.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
    m.world.characters.remove(&CharacterId(101));
    assert!(m.is_rebel_faction(B) && !m.out_of_game_by_startpos_rule(B));
}

/// A return without the liberation flag (`0x00B2BC10` / `0x00B72AC0`): an AI previous owner costs 50
/// attitude on the returning faction's record and 25 on its own; a human previous owner means war.
#[test]
fn a_return_from_an_ai_owner_costs_attitude_and_from_a_human_means_war() {
    let mut m = test_model();
    set_details(&mut m, C, None, true);
    m.relationship_mut(C, A).start_attitude = 20;
    m.relationship_mut(A, C).start_attitude = 10;
    m.reemerge_faction(C, A, false, &mut Vec::new());
    let value = |m: &CampaignModel, a, b| m.world.relationships[&(a, b)].factor("initial_modifier").unwrap().value;
    assert_eq!((value(&m, C, A), value(&m, A, C)), (-30, -15));
    assert_eq!((m.world.stance(C, A), m.world.stance(A, C)), (Stance::Neutral, Stance::Neutral));
    assert_eq!(value(&m, C, B), 0, "only the previous owner's pair");
    assert!(m.in_the_game(C));
    m.turn.humans = vec![A];
    m.reemerge_faction(C, A, false, &mut Vec::new());
    assert_eq!((m.world.stance(C, A), m.world.stance(A, C)), (Stance::War, Stance::War));
    assert_eq!((value(&m, C, A), value(&m, A, C)), (20 - 140, 10 - 140));
}

/// A returning faction sets up no pair with the rebels (`0x00B2BB00` walks its diplomacy records; the
/// rebel faction has no `DIPLOMACY_MANAGER`, so it holds none): no record and no stance either way.
#[test]
fn a_returning_faction_sets_up_nothing_with_the_rebels() {
    let mut m = test_model();
    m.world.factions.get_mut(&B).unwrap().key.clear();
    m.world.relationships.retain(|(a, b), _| *a != B && *b != B);
    for f in m.world.factions.values_mut() {
        f.diplomacy.remove(&B);
    }
    m.world.factions.get_mut(&B).unwrap().diplomacy.clear();
    set_details(&mut m, C, None, true);
    m.reemerge_faction(C, A, true, &mut Vec::new());
    assert!(m.in_the_game(C));
    assert!(!m.world.relationships.contains_key(&(C, B)) && !m.world.relationships.contains_key(&(B, C)));
    assert!(!m.world.factions[&C].diplomacy.contains_key(&B) && !m.world.factions[&B].diplomacy.contains_key(&C));
    assert_eq!(m.world.stance(C, A), Stance::Protectorate, "the other pairs are set up");
}

/// construction2: a repair an older save stored as a build of the standing level becomes a repair item of the
/// length and step `queue_repair` gives from the building's health (unchanged while such a repair ran), with
/// the steps already run added to the health; a build of another level is left alone.
#[test]
fn a_repair_stored_as_a_build_of_the_standing_level_converts() {
    let mut m = test_model();
    Arc::make_mut(&mut m.rules).buildings.get_mut("test_building_level").unwrap().turns = 10;
    let r10 = RegionId(10);
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        r.slots[0].building.as_mut().unwrap().health = 40;
        r.slots[1].building = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
        let item = |slot, level_key: &str| ConstructionItem { slot, level_key: level_key.into(), turns_remaining: 4, cost: 0, kind: ConstructionKind::Build, active: true };
        r.construction = vec![item(SlotRef::Slot(0), "test_building_level"), item(SlotRef::Slot(1), "test_building_level_2")];
    }
    // floor(60 × 0.01 × 10) in f32 (5.9999…) = 5 turns of (100 − 40) / 5 = 12; 4 left, so 1 ran: health 52.
    assert_eq!(m.convert_legacy_repairs(), 1);
    let r = &m.world.regions[&r10];
    assert_eq!(r.construction[0].kind, ConstructionKind::Repair { health_per_turn: 12, turns: 5 });
    assert_eq!((r.construction[0].turns_remaining, r.slots[0].building.as_ref().unwrap().health), (4, 52));
    assert_eq!(r.construction[1].kind, ConstructionKind::Build);
    assert_eq!(m.convert_legacy_repairs(), 0, "converted once");
}

/// construction2: the predicted slot values (`0x00A6AFC0` with its predicted flag) count the level being
/// built, nothing for a slot being demolished, the standing building while it is repaired, and otherwise
/// the standing building only at full health.
#[test]
fn the_predicted_slot_level_follows_the_slot_item() {
    let mut m = test_model();
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    r.slots[0].building.as_mut().unwrap().health = 40;
    let s0 = SlotRef::Slot(0);
    assert_eq!(r.predicted_slot_level(s0), None, "damaged");
    let item = |level_key: &str, kind| ConstructionItem { slot: s0, level_key: level_key.into(), turns_remaining: 1, cost: 0, kind, active: true };
    r.construction = vec![item("test_building_level", ConstructionKind::Repair { health_per_turn: 60, turns: 1 })];
    assert_eq!(r.predicted_slot_level(s0), Some("test_building_level"));
    r.construction = vec![item("", ConstructionKind::Demolish)];
    assert_eq!(r.predicted_slot_level(s0), None);
    r.construction = vec![item("test_building_level_2", ConstructionKind::Build)];
    assert_eq!(r.predicted_slot_level(s0), Some("test_building_level_2"));
    assert_eq!(r.construction_changes().count(), 1);
    r.construction = vec![item("", ConstructionKind::Demolish)];
    assert_eq!(r.construction_changes().count(), 0, "a demolition has no level");
}

/// polish21: a docked navy leaving its port, by sailing (the undock `0x00A8C0B0`) or by going (its destructor
/// `0x0088EF90`), hands the port slot back to the region's owner (`0x00A8D830`); a navy elsewhere changes nothing.
#[test]
fn a_navy_leaving_its_port_hands_the_slot_back_to_the_owner() {
    let setup = || {
        let mut m = test_model();
        let r10 = RegionId(10);
        {
            let r = m.world.regions.get_mut(&r10).unwrap();
            (r.slots[0].port, r.slots[0].position) = (true, Some(pos(5, 5)));
        }
        let admiral = Character { id: CharacterId(109), faction: B, kind: CharacterKind::Admiral, position: pos(5, 5), movement_points: 0, max_movement_points: 0, base_movement_points: 0, garrisoned_in: None };
        m.world.characters.insert(admiral.id, admiral);
        m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: B, commander: Some(CharacterId(109)), units: vec![unit(9)], is_navy: true });
        // The owner changes while the navy lies there: the port stays A's.
        assert!(m.transfer_region(r10, C, &mut Vec::new()));
        assert_eq!(m.world.regions[&r10].slots[0].holder, Some(A));
        m
    };
    // Sailing away.
    let mut m = setup();
    m.move_character(CharacterId(109), pos(9, 9));
    assert_eq!(m.world.regions[&RegionId(10)].slots[0].holder, None, "the port is C's again");
    // A move at sea leaves a held port alone.
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].holder = Some(A);
    m.move_character(CharacterId(109), pos(9, 12));
    assert_eq!(m.world.regions[&RegionId(10)].slots[0].holder, Some(A));
    // Destroyed in port.
    let mut m = setup();
    m.destroy_force(ForceId(1005), &mut Vec::new());
    assert_eq!(m.world.regions[&RegionId(10)].slots[0].holder, None);
    // Its commander dies and no unit is left: the empty navy goes where he stood.
    let mut m = setup();
    m.world.forces.get_mut(&ForceId(1005)).unwrap().units.clear();
    m.character_dies(CharacterId(109));
    assert!(!m.world.forces.contains_key(&ForceId(1005)));
    assert_eq!(m.world.regions[&RegionId(10)].slots[0].holder, None);
}

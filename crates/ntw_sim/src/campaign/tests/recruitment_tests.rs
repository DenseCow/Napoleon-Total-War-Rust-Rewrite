//! Campaign model tests: recruitment.

use super::*;

/// A raised unit's full size (`world::recruited_unit_size`, CONFIRMED from the vanilla saves in
/// `ntw_campaign`'s `economy_fidelity`): `num_men` for land, the crew triple's sum for a ship, even
/// when no unit of that ship type exists yet.
#[test]
fn recruited_units_are_sized_by_num_men_or_the_crew_sum() {
    use super::naval::ShipRules;
    let mut rules = CampaignRules::test_rules();
    rules.units.insert("test_ship".into(), UnitRules { is_naval: true, men: 0, ..rules.units["test_unit"].clone() });
    rules.ships.insert("test_ship".into(), ShipRules { crews: [20, 20, 100], ..Default::default() });
    assert_eq!(world::recruited_unit_size(&rules, "test_unit"), 100);
    assert_eq!(world::recruited_unit_size(&rules, "test_ship"), 140);
    assert_eq!(world::recruited_unit_size(&rules, "no_such_unit"), 1);
    // Through the spawn: a ship type no force holds yet still gets its crew sum.
    let mut m = test_model();
    m.rules = Arc::new(rules);
    let Some(CampaignEvent::UnitTrained { force, unit }) = m.spawn_recruited_unit(RegionId(10), "test_ship".into()) else {
        panic!("a unit is trained")
    };
    let u = m.world.forces[&force].units.iter().find(|u| u.id == unit).expect("the new unit");
    assert_eq!((u.men, u.max_men), (140, 140));
}

#[test]
fn recruitment_completes_after_n_turns() {
    let mut m = test_model();
    // MADE-UP rules: campaign cost (#7) 400, 2 turns, 100 men; region 10 has 2 recruitment points.
    let ev = m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    assert_eq!(ev, vec![CampaignEvent::RecruitmentItemIssuedByPlayer { region: RegionId(10) }]);
    assert_eq!(m.world.factions[&A].treasury, 600);
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 200);
    m.world.factions.get_mut(&A).unwrap().treasury = 10_000;
    // A third item queues too (a queue holds 10, 0x00B62040); it would wait while both points train.
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 9_600);
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 2) }).unwrap();
    // Cancelling refunds.
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 1) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 10_400);
    m.turn.humans = vec![A];
    let ev1 = m.start_campaign();
    assert!(!ev1.iter().any(|e| matches!(e, CampaignEvent::UnitTrained { .. })));
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue[0].turns_remaining, 1);
    let ids_before: Vec<u32> = m.world.characters.keys().map(|c| c.0 as u32).chain(m.world.forces.keys().map(|f| f.0)).collect();
    let ev2 = m.end_turn();
    let i_trained = ev2.iter().position(|e| matches!(e, CampaignEvent::UnitTrained { .. })).unwrap();
    let CampaignEvent::UnitTrained { force, unit } = ev2[i_trained] else { unreachable!() };
    let i_end = ev2.iter().position(|e| *e == CampaignEvent::RegionTurnStart { region: RegionId(10) }).unwrap();
    assert!(i_trained < i_end);
    assert!(m.world.regions[&RegionId(10)].recruitment_queue.is_empty());
    // The settlement had no garrison: a new army led by a new colonel, garrisoned there, with the
    // colonel attached to the unit (SAVE_COMPAT.md §4).
    assert_eq!(m.world.regions[&RegionId(10)].garrison, Some(force));
    let garrison = m.world.forces[&force].clone();
    let colonel = garrison.commander.expect("every force has a commander");
    assert_eq!(garrison.faction, A);
    assert_eq!(garrison.units[0].id, unit);
    assert_eq!(garrison.units[0].unit_key, "test_recruit");
    assert_eq!(garrison.units[0].men, 100);
    assert_eq!(garrison.units[0].character, Some(colonel));
    let ch = &m.world.characters[&colonel];
    assert_eq!((ch.kind, ch.faction, ch.garrisoned_in), (CharacterKind::Colonel, A, Some(RegionId(10))));
    assert_eq!(ch.position, m.world.regions[&RegionId(10)].settlement.position);
    // New ids: unique, above every id loaded, steps of 8.
    for id in [unit.0 as u32, force.0, colonel.0 as u32] {
        assert!(ids_before.iter().all(|&b| b < id), "{id}");
        assert_eq!(id % 8, 0);
    }
    // The next unit raised there joins the garrison, with no colonel of its own.
    let n = m.world.forces.len();
    m.spawn_recruited_unit(RegionId(10), "test_unit".into());
    assert_eq!(m.world.forces.len(), n);
    assert_eq!(m.world.forces[&force].units.len(), 2);
    assert_eq!(m.world.forces[&force].units[1].character, None);
}

/// `0x00B0D220`, the recruitable entry's cost (CONFIRMED): `units` #7 (not the #4 cost) scaled by
/// `max(−100, mod) + 100` percent in f32 and rounded by FISTP (half to even), `mod` being the integer
/// `recruitment_mod_cost_land_all` (`_naval_all` for a ship) plus the unit category's and class's
/// `cost_mod`, and the campaign's unit-key suffix cost mod (spa_napoleon's guerrilla / auxiliary).
#[test]
fn the_recruitment_cost_is_units_7_scaled_by_the_cost_effects() {
    use super::effects::EffectSet;
    use super::features::UnitKeyEffects;
    let mut rules = CampaignRules::test_rules();
    let land = UnitRules { cost: 710, campaign_cost: 580, category: "infantry".into(), unit_class: "infantry_line".into(), ..Default::default() };
    let ship = UnitRules { is_naval: true, category: "naval_frigate".into(), unit_class: "frigate".into(), ..land.clone() };
    let mut set = EffectSet::default();
    // No effects: the #7 cost exactly.
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &land), 580);
    // −8 (a real vanilla figure: Sweden's line infantry was queued at 534, its hussars at 432).
    set.add(EffectKey::basic("recruitment_mod_cost_land_all"), -8.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &land), 534);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &UnitRules { campaign_cost: 470, ..land.clone() }), 432);
    // A ship reads the naval key only.
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &ship), 580);
    set.add(EffectKey::basic("recruitment_mod_cost_naval_all"), -1.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &UnitRules { campaign_cost: 500, ..ship.clone() }), 495);
    // The category's and the class's cost_mod add in.
    set.add(EffectKey { kind: BonusKind::UnitCategory, bonus: "cost_mod".into(), qualifier: "infantry".into() }, -2.0);
    set.add(EffectKey { kind: BonusKind::UnitClass, bonus: "cost_mod".into(), qualifier: "infantry_line".into() }, -10.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &set, "u", &land), 464); // 580 × 0.80
    // Each effect is read as an integer, half to even: −0.5 reads 0, −1.5 reads −2.
    let mut half = EffectSet::default();
    half.add(EffectKey::basic("recruitment_mod_cost_land_all"), -0.5);
    assert_eq!(economy::recruitment_cost_in(&rules, &half, "u", &land), 580);
    half.add(EffectKey { kind: BonusKind::UnitCategory, bonus: "cost_mod".into(), qualifier: "infantry".into() }, -1.5);
    assert_eq!(economy::recruitment_cost_in(&rules, &half, "u", &land), 568); // 580 × 0.98 = 568.4
    // The product is rounded half to even: 25 × 0.5 = 12.5 → 12, 35 × 0.5 = 17.5 → 18.
    let mut fifty = EffectSet::default();
    fifty.add(EffectKey::basic("recruitment_mod_cost_land_all"), -50.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &fifty, "u", &UnitRules { campaign_cost: 25, ..land.clone() }), 12);
    assert_eq!(economy::recruitment_cost_in(&rules, &fifty, "u", &UnitRules { campaign_cost: 35, ..land.clone() }), 18);
    // The sum is clamped at −100: never a negative price.
    fifty.add(EffectKey::basic("recruitment_mod_cost_land_all"), -500.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &fifty, "u", &land), 0);
    // The guerrilla / auxiliary mods count only with the campaign's suffix table (spa_napoleon's), by the end of
    // the unit key, the first match only.
    let mut spa = EffectSet::default();
    spa.add(EffectKey::basic("guerrilla_cost_mod"), -20.0);
    spa.add(EffectKey::basic("auxiliary_cost_mod"), 10.0);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_Guerrilla", &land), 580);
    rules.features.unit_key_effects = vec![UnitKeyEffects::new("_Guerrilla", "guerrilla_cost_mod", "guerrilla_upkeep_mod"), UnitKeyEffects::new("_Auxiliary", "auxiliary_cost_mod", "auxiliary_upkeep_mod")];
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_Guerrilla", &land), 464);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_British_Auxiliary", &land), 638);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Guerrilla_Line", &land), 580);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_guerrilla", &land), 580);
    // Another campaign's own suffix (a Shogun-style roster): any key, any effect.
    rules.features.unit_key_effects = vec![UnitKeyEffects::new("_Ashigaru", "auxiliary_cost_mod", "auxiliary_upkeep_mod")];
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Yari_Ashigaru", &land), 638);
    assert_eq!(economy::recruitment_cost_in(&rules, &spa, "Inf_Spanish_Guerrilla", &land), 580);
}

/// A unit's upkeep adds the campaign's unit-key upkeep effect (land `0x008F9B5F`, naval `0x008F9D4F`; spa: `_Guerrilla` →
/// `guerrilla_upkeep_mod`, else `_Auxiliary` → `auxiliary_upkeep_mod`; review: missing). A campaign
/// without the feature is unchanged.
#[test]
fn upkeep_adds_the_unit_key_effect() {
    use super::effects::{EffectKey, EffectSet, Effects, ForceEffects};
    use super::features::{CampaignFeatures, UnitKeyEffects};
    let mut set = EffectSet::default();
    set.add(EffectKey::basic("guerrilla_upkeep_mod"), -50.0);
    set.add(EffectKey::basic("auxiliary_upkeep_mod"), 20.0);
    let mut fx = Effects::default();
    fx.faction.insert(A, set);
    let land = UnitRules { upkeep: 100, ..Default::default() };
    let none = CampaignFeatures::default();
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &none, "Inf_Spanish_Guerrilla", &land), 100);
    let spa = CampaignFeatures {
        unit_key_effects: vec![UnitKeyEffects::new("_Guerrilla", "guerrilla_cost_mod", "guerrilla_upkeep_mod"), UnitKeyEffects::new("_Auxiliary", "auxiliary_cost_mod", "auxiliary_upkeep_mod")],
        ..Default::default()
    };
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &spa, "Inf_Spanish_Guerrilla", &land), 50);
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &spa, "Inf_British_Auxiliary", &land), 120);
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &spa, "Inf_Line", &land), 100);
    let ship = UnitRules { upkeep: 100, is_naval: true, ..Default::default() };
    // Ships too (`0x008F9D00`, review round 2), and a ship's negative mod costs 0 where a land unit's goes negative.
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &spa, "Ship_Guerrilla", &ship), 50);
    let mut cheap = EffectSet::default();
    cheap.add(EffectKey::basic("upkeep_cost_mod_naval_all"), -150.0);
    cheap.add(EffectKey::basic("upkeep_cost_mod_land_all"), -150.0);
    fx.faction.insert(A, cheap);
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &spa, "Ship", &ship), 0);
    assert_eq!(economy::unit_upkeep(ForceEffects::faction_only(&fx.faction[&A]), &spa, "Inf_Line", &land), -50);
}

/// The recruit command charges the region's entry cost (#7 with the region's effects, here a faction
/// `recruitment_mod_cost_land_all` of −8 from its saved bonuses), the item keeps it, and a cancel
/// credits exactly that back (`0x00B5C060`: item +0x20 as income category 3).
#[test]
fn recruiting_charges_the_entry_cost_and_cancelling_refunds_it() {
    let mut m = test_model();
    let unit = m.rules.units["test_recruit"].clone();
    assert_ne!(unit.cost, unit.campaign_cost, "the fixture tells the two costs apart");
    m.world.faction_details.entry(A).or_default().bonus_base = vec![SavedBonus { kind: 1, bonus: 30, value: -8.0, qualifier: String::new(), ..Default::default() }];
    let before = m.world.factions[&A].treasury;
    let cost = (unit.campaign_cost as f32 * 0.92).round_ties_even() as i32;
    assert_eq!(economy::recruitment_cost(&m, &m.world.regions[&RegionId(10)], "test_recruit", &unit), cost);
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, before - cost);
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue[0].cost, cost);
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, before);
}

/// The entry is "too dear" only when its cost is above the treasury compared unsigned (`0x00B69BA0`,
/// flag 2), and the command has no other money test: a faction in debt may recruit, going deeper into
/// debt; one with a positive treasury below the cost may not.
#[test]
fn a_faction_in_debt_may_recruit() {
    let mut m = test_model();
    let cost = m.rules.units["test_recruit"].campaign_cost;
    m.world.factions.get_mut(&A).unwrap().treasury = cost - 1;
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }),
        Err(CommandError::InsufficientFunds { needed: cost, available: cost - 1 })
    );
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, -5 - cost);
}

/// The recruit command (`0x00936B90` → `0x00B58DD0`) does not look at the recruitment points: a region
/// with none still queues, and its items then wait (`0x00B71FB0` counts down only the first
/// `recruitment_points` items); only a full queue (10) refuses.
#[test]
fn a_region_without_recruitment_points_still_queues() {
    let mut m = test_model();
    // The region's building gives 2 points; a faction effect of −2 takes them away.
    m.world.faction_details.entry(A).or_default().bonus_base = vec![SavedBonus { kind: 1, bonus: 32, value: -2.0, qualifier: String::new(), ..Default::default() }];
    assert_eq!(m.recruitment_points(RegionId(10), false), 0);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    let turns = m.world.regions[&RegionId(10)].recruitment_queue[0].turns_remaining;
    m.turn.humans = vec![A];
    m.start_campaign();
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue[0].turns_remaining, turns);
    while m.world.regions[&RegionId(10)].recruitment_queue.len() < super::commands::MAX_QUEUE as usize {
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }).unwrap();
    }
    assert_eq!(
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None }),
        Err(CommandError::NoRecruitmentCapacity)
    );
}

/// `units` #15 caps what a faction may hold and have queued of one unit type (`0x008F68B0`: the faction's
/// live units of the type plus its queued items of it, against the cap; flag 0x40 of `0x00B69BA0`). Other
/// factions' units and queues do not count, a cancel or a lost unit frees a place, and 0 means no cap.
#[test]
fn the_unit_cap_counts_the_factions_units_and_queued_items() {
    use super::commands::{ENTRY_QUEUE_FULL, ENTRY_TOO_DEAR, ENTRY_UNIT_CAP};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_recruit").unwrap().unit_cap = 3;
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    let recruit = |k: &str| CampaignUnit { unit_key: k.into(), ..unit(50) };
    // A holds one; B holds two and has one queued, which are B's own.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.push(recruit("test_recruit"));
    m.world.forces.get_mut(&ForceId(1001)).unwrap().units.extend([recruit("test_recruit"), recruit("test_recruit")]);
    m.world.regions.get_mut(&RegionId(11)).unwrap().recruitment_queue.push(RecruitmentItem {
        id: RecruitmentItemId(9000),
        unit_key: "test_recruit".into(),
        turns_remaining: 2,
        cost: 400,
        target: None,
        target_lost: false,
    });
    let order = || CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: None };
    // 1 held + 0 and then 1 queued are below 3; at 1 + 2 the entry is flagged.
    m.apply(order()).unwrap();
    m.apply(order()).unwrap();
    assert_eq!(m.apply(order()), Err(CommandError::UnitCapReached("test_recruit".into())));
    let unit = m.rules.units["test_recruit"].clone();
    let flags = |m: &CampaignModel| m.recruitable_entry_flags(&m.world.regions[&RegionId(10)], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags(&m), ENTRY_UNIT_CAP);
    // A cancel frees a place.
    m.apply(CampaignCommand::CancelRecruitment { region: RegionId(10), item: queued(&m, RegionId(10), 1) }).unwrap();
    assert_eq!(flags(&m), 0);
    m.apply(order()).unwrap();
    // So does a unit the faction no longer holds.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.retain(|u| u.unit_key != "test_recruit");
    assert_eq!(flags(&m), 0);
    m.apply(order()).unwrap();
    assert_eq!(flags(&m), ENTRY_UNIT_CAP);
    // The flags add up: also too dear and, once the queue holds 10, full.
    m.world.factions.get_mut(&A).unwrap().treasury = 399;
    assert_eq!(flags(&m), ENTRY_UNIT_CAP | ENTRY_TOO_DEAR);
    let mut rules = (*m.rules).clone();
    rules.units.get_mut("test_recruit").unwrap().unit_cap = 0;
    m.rules = Arc::new(rules);
    let unit = m.rules.units["test_recruit"].clone();
    let flags = |m: &CampaignModel| m.recruitable_entry_flags(&m.world.regions[&RegionId(10)], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags(&m), ENTRY_TOO_DEAR, "a cap of 0 is no cap");
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    while m.world.regions[&RegionId(10)].recruitment_queue.len() < super::commands::MAX_QUEUE as usize {
        m.apply(order()).unwrap();
    }
    assert_eq!(flags(&m), ENTRY_QUEUE_FULL);
}

/// The recruitable population (`0x00A89550` / `0x00AAF190` / `0x00A61AA0`): with the shipped variables (both
/// 0) a recruit needs and takes nothing; with a mod's `recruitment_population_cost` (rounded half to even, as
/// `0x008B25F0` reads it) and `minimum_population_after_recruitment`, the queue command takes the cost from
/// the region's population while that leaves the minimum, flags the entry (4) and refuses below it without
/// charging anything, and every untrained exit (cancel, the turn start's removal, a capture) gives the cost
/// back per item.
#[test]
fn recruiting_takes_population_and_cancelling_gives_it_back() {
    use super::commands::ENTRY_NO_POPULATION;
    let mut m = test_model();
    let r10 = RegionId(10);
    let order = || CampaignCommand::Recruit { region: r10, unit_key: "test_recruit".into(), target: None };
    let pop = |m: &CampaignModel| m.world.regions[&r10].population;
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m.world.regions.get_mut(&r10).unwrap().population = 5;
    m.apply(order()).unwrap();
    assert_eq!(pop(&m), 5, "the shipped variables are 0");

    let mut rules = (*m.rules).clone();
    rules.variables.insert("recruitment_population_cost".into(), 300.5);
    rules.variables.insert("minimum_population_after_recruitment".into(), 1000.0);
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&r10).unwrap().population = 1600;
    m.apply(order()).unwrap();
    assert_eq!(pop(&m), 1300, "300.5 reads as 300");
    m.apply(order()).unwrap();
    assert_eq!(pop(&m), 1000, "exactly the minimum is left");
    let unit = m.rules.units["test_recruit"].clone();
    let flags = |m: &CampaignModel| m.recruitable_entry_flags(&m.world.regions[&r10], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A));
    assert_eq!(flags(&m), ENTRY_NO_POPULATION);
    let treasury = m.world.factions[&A].treasury;
    assert_eq!(m.apply(order()), Err(CommandError::NotEnoughPopulation));
    assert_eq!((pop(&m), m.world.factions[&A].treasury, m.world.regions[&r10].recruitment_queue.len()), (1000, treasury, 3));

    // A cancel gives the cost back.
    m.apply(CampaignCommand::CancelRecruitment { region: r10, item: queued(&m, r10, 2) }).unwrap();
    assert_eq!(pop(&m), 1300);
    assert_eq!(flags(&m), 0);

    // So does the turn start's removal of an item the region can no longer recruit, per item.
    for id in [9001, 9002] {
        let item = RecruitmentItem { id: RecruitmentItemId(id), unit_key: "gone_unit".into(), turns_remaining: 5, cost: 0, target: None, target_lost: false };
        m.world.regions.get_mut(&r10).unwrap().recruitment_queue.push(item);
    }
    assert!(!m.recruitable_units(r10).iter().any(|e| e.unit_key == "gone_unit"));
    m.turn.humans = vec![A];
    m.start_campaign();
    assert_eq!((pop(&m), m.world.regions[&r10].recruitment_queue.len()), (1900, 2));

    // And a capture, for every item it clears.
    let mut events = Vec::new();
    m.occupy(r10, B, None, &mut events);
    assert_eq!((pop(&m), m.world.regions[&r10].recruitment_queue.len()), (2500, 0));
}

/// A capture (every capture variant, e.g. `0x00B58A30` / `0x00B58560`) empties the land queue with no refund
/// (`0x00B1A760(0)` on region +0x124), each port's naval queue with the refund (`0x00B1A760(1)` on slot
/// +0x1E8: `0x00B5C0A0` credits item +0x20 to the region's owner, still the old one) and every construction
/// with none (`0x00A6CBE0(0)`), whoever the old owner is.
#[test]
fn a_capture_refunds_the_old_owners_queued_ships_only() {
    for human in [false, true] {
        let mut m = model_with_queued_items();
        if human {
            m.turn.humans = vec![A];
        }
        let (a, b) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
        let mut events = Vec::new();
        m.occupy(RegionId(10), B, None, &mut events);
        let r = &m.world.regions[&RegionId(10)];
        assert!(r.recruitment_queue.is_empty() && r.construction.is_empty());
        assert_eq!(m.world.factions[&A].treasury, a + 70 + 110, "human old owner: {human}");
        assert_eq!(m.world.factions[&B].treasury, b);
    }
}

/// A region handed over without a capture (a deal's region item, a liberation, `grant_faction_handover`:
/// `0x00B58A10` → `0x00B449F0` with the report flag clear) runs only `0x00A64AC0`'s drain, whose flag is set
/// when the old owner is not human (faction +0x6E0): an AI old owner gets every queued unit's cost back, land
/// and naval; a human one gets nothing. Constructions are never refunded.
#[test]
fn a_transfer_refunds_an_ai_old_owners_queued_units_and_a_humans_none() {
    for (human, refund) in [(false, 300 + 70 + 500 + 110), (true, 0)] {
        let mut m = model_with_queued_items();
        if human {
            m.turn.humans = vec![A];
        }
        let (a, b) = (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
        assert!(m.transfer_region(RegionId(10), B, &mut Vec::new()));
        let r = &m.world.regions[&RegionId(10)];
        assert!(r.recruitment_queue.is_empty() && r.construction.is_empty());
        assert_eq!(m.world.factions[&A].treasury, a + refund, "human old owner: {human}");
        assert_eq!(m.world.factions[&B].treasury, b);
    }
}

/// The population rules' 32-bit arithmetic (`0x00AAF190`): a charge that would leave less than the minimum
/// sets the population to the minimum (unreachable through the queue command, which refuses that entry
/// first); the gate's sum and compare are signed, so a mod's negative cost passes any population.
#[test]
fn the_recruitment_population_rules_are_signed_32_bit() {
    use super::population::RecruitmentPopulation;
    let p = RecruitmentPopulation { cost: 300, minimum: 1000 };
    assert_eq!((p.available(1299), p.charged(1299), p.charged(1300)), (false, 1000, 1000));
    assert_eq!(p.credited(1000, 3), 1900);
    let negative = RecruitmentPopulation { cost: -50, minimum: 0 };
    assert!(negative.available(0));
    assert_eq!((negative.charged(0), negative.credited(100, 2)), (50, 0));
    let mut rules = super::rules::CampaignRules::default();
    rules.variables.insert("recruitment_population_cost".into(), 2.5);
    rules.variables.insert("minimum_population_after_recruitment".into(), 3.5);
    assert_eq!(RecruitmentPopulation::of(&rules), RecruitmentPopulation { cost: 2, minimum: 4 });
    assert_eq!(RecruitmentPopulation::of(&super::rules::CampaignRules::default()), RecruitmentPopulation { cost: 0, minimum: 0 });
}

/// A mod's negative `minimum_population_after_recruitment` lets recruiting take the population below 0. The exe
/// keeps the field's 32 bits: the gate keeps reading them signed (so −200 still passes a need of −700 and −800
/// fails it), while the growth reads them unsigned (`0x00AB4227`), as the `u32` holding the same bits does here.
#[test]
fn a_population_charged_below_zero_keeps_the_exes_bits() {
    use super::commands::ENTRY_NO_POPULATION;
    let mut m = test_model();
    let r10 = RegionId(10);
    let order = || CampaignCommand::Recruit { region: r10, unit_key: "test_recruit".into(), target: None };
    let mut rules = (*m.rules).clone();
    rules.variables.insert("recruitment_population_cost".into(), 300.0);
    rules.variables.insert("minimum_population_after_recruitment".into(), -1000.0);
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m.world.regions.get_mut(&r10).unwrap().population = 100;
    m.apply(order()).unwrap();
    assert_eq!(m.world.regions[&r10].population, (-200i32) as u32);
    m.apply(order()).unwrap();
    m.apply(order()).unwrap();
    assert_eq!(m.world.regions[&r10].population, (-800i32) as u32);
    let unit = m.rules.units["test_recruit"].clone();
    assert_eq!(m.recruitable_entry_flags(&m.world.regions[&r10], &super::commands::RecruitableUnit { unit_key: "test_recruit".into(), flags: 0 }, &unit, 400, &m.unit_type_counts(A)), ENTRY_NO_POPULATION);
    assert_eq!(m.apply(order()), Err(CommandError::NotEnoughPopulation));
    // Growth of 1% on the bits of −200, read unsigned: f32(4294967096) = 4294967040, × 0.01 → 42949668 (FISTP),
    // plus −200 (a signed read would give −202).
    let r = &m.world.regions[&r10];
    let state = super::population::PopulationState { factors: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
    assert_eq!(super::population::grow(&m, r, (-200i32) as u32, &state).0, 42_949_468);
}

/// Two items recruited through a general who leads an army (item +0x18): when they finish, their units
/// raise ONE new army led by a new colonel outside the settlement (`0x00B1E500` → `0x00B1D3F0`), which
/// the faction's turn start then marches to him and merges into his army (`0x00B71FA0` at `0x008F2B35`
/// → `0x00B5C4A0`), before `FactionTurnStart`. An untargeted item of the same turn joins the garrison.
#[test]
fn items_recruited_through_a_commander_march_to_his_army() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    for _ in 0..2 {
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
    }
    m.world.characters.get_mut(&general).unwrap().position = pos(10, 1);
    m.turn.humans = vec![A];
    m.start_campaign();
    let ev = m.end_turn();
    let trained: Vec<_> = ev.iter().filter_map(|e| if let CampaignEvent::UnitTrained { force, unit } = e { Some((*force, *unit)) } else { None }).collect();
    assert_eq!(trained.len(), 2, "{ev:?}");
    let new_force = trained[0].0;
    assert!(trained.iter().all(|&(f, _)| f == new_force), "one army for the group: {trained:?}");
    assert_ne!(new_force, ForceId(1000));
    let merged = ev.iter().position(|e| *e == CampaignEvent::CampaignArmiesMerge { force: new_force, into: ForceId(1000) }).expect("the new army merges");
    let started = ev.iter().position(|e| *e == CampaignEvent::FactionTurnStart { faction: A }).unwrap();
    assert!(merged < started);
    // Both units are in the general's army now; the new army is gone and nothing was garrisoned.
    let army = &m.world.forces[&ForceId(1000)];
    assert_eq!(army.units.len(), 3);
    assert!(trained.iter().all(|&(_, u)| army.units.iter().any(|x| x.id == u)));
    assert!(!m.world.forces.contains_key(&new_force));
    assert_eq!(m.world.regions[&RegionId(10)].garrison, None);
    assert!(m.world.reinforcements.is_empty());
}

/// The army raised for a commander (`spawn_reinforcement`): every unit of the group in one new army led by
/// a new colonel attached to one of them, standing outside the settlement, with a reinforcement entry
/// (`0x00B0A270` → `0x00AE8B10`: source, target, settlement). When the target is gone by the step
/// (`0x00B5C830`; his death listener `0x00B57DA0` clears the entry) the entry is dropped and the army stays.
#[test]
fn a_reinforcement_whose_commander_is_gone_stays_where_it_was_raised() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    let ev = m.spawn_reinforcement(RegionId(10), general, vec!["test_recruit".into(), "test_unit".into()]);
    assert_eq!(ev.len(), 2);
    let CampaignEvent::UnitTrained { force, .. } = ev[0] else { panic!("{ev:?}") };
    let f = m.world.forces[&force].clone();
    let colonel = f.commander.unwrap();
    assert_eq!(f.units.iter().map(|u| u.unit_key.as_str()).collect::<Vec<_>>(), ["test_recruit", "test_unit"]);
    assert_eq!(f.units.iter().filter(|u| u.character == Some(colonel)).count(), 1);
    let ch = &m.world.characters[&colonel];
    assert_eq!((ch.kind, ch.faction, ch.garrisoned_in), (CharacterKind::Colonel, A, None));
    assert_eq!(ch.position, m.world.regions[&RegionId(10)].settlement.position);
    assert_eq!(m.world.regions[&RegionId(10)].garrison, None);
    assert_eq!(m.world.reinforcements, vec![Reinforcement { source: colonel, target: general, settlement: RegionId(10) }]);
    m.character_dies(general);
    assert!(m.march_reinforcements(A).is_empty());
    assert!(m.world.reinforcements.is_empty());
    assert_eq!(m.world.forces[&force].units.len(), 2);
}

/// The army raised for general 100 at region 10, its colonel, and the colonel's action points set to `mp`.
fn raised_reinforcement(m: &mut CampaignModel, mp: i32) -> (ForceId, CharacterId) {
    let ev = m.spawn_reinforcement(RegionId(10), CharacterId(100), vec!["test_recruit".into(), "test_unit".into()]);
    let CampaignEvent::UnitTrained { force, .. } = ev[0] else { panic!("{ev:?}") };
    let colonel = m.world.forces[&force].commander.unwrap();
    m.world.characters.get_mut(&colonel).unwrap().movement_points = mp;
    (force, colonel)
}

/// The reinforcement order is a multi-turn one (`0x009195A0`, order object `0x00905460`): given even when
/// the new army cannot move this turn (the execute `0x0095BA40` commits the path whatever the points), it
/// replaces the entry, stays on the colonel (character +0x9C) and is resumed at each later turn start
/// (`0x009653D0`), following the target, until the merge completes it (`0x0094E840`).
#[test]
fn a_reinforcement_out_of_reach_marches_on_at_later_turn_starts() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    let (force, colonel) = raised_reinforcement(&mut m, 0);
    assert!(m.march_reinforcements(A).is_empty());
    assert!(m.world.reinforcements.is_empty(), "the entry is deleted once the order is given");
    assert_eq!(m.world.standing_orders.get(&colonel), Some(&StandingOrder::JoinForce { into: ForceId(1000) }));
    // Still no points: the order waits.
    assert!(m.resume_standing_orders(A).is_empty());
    assert!(m.world.standing_orders.contains_key(&colonel));
    // The general comes closer and the colonel has his points back: the march reaches him and merges.
    m.world.characters.get_mut(&general).unwrap().position = pos(10, 1);
    m.world.characters.get_mut(&colonel).unwrap().movement_points = 25;
    let ev = m.resume_standing_orders(A);
    assert!(ev.contains(&CampaignEvent::CampaignArmiesMerge { force, into: ForceId(1000) }), "{ev:?}");
    assert!(!m.world.forces.contains_key(&force));
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 3);
    assert!(m.world.standing_orders.is_empty(), "the merge completes the order");
}

/// The turn start resumes the order (`0x008F2C5B` → `0x009653D0`, after the reinforcement manager): an
/// order kept from an earlier turn merges once the general is in reach.
#[test]
fn the_turn_start_resumes_the_reinforcement_order() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    m.turn.humans = vec![A];
    m.start_campaign();
    // Given in A's turn without points (the order waits); A's next turn start refills them first.
    let (force, colonel) = raised_reinforcement(&mut m, 0);
    m.world.characters.get_mut(&general).unwrap().position = pos(11, 1);
    m.world.reinforcements.clear();
    m.world.standing_orders.insert(colonel, StandingOrder::JoinForce { into: ForceId(1000) });
    let ev = m.end_turn();
    let merged = ev.iter().position(|e| *e == CampaignEvent::CampaignArmiesMerge { force, into: ForceId(1000) }).expect("the army merges");
    let started = ev.iter().position(|e| *e == CampaignEvent::FactionTurnStart { faction: A }).unwrap();
    assert!(merged < started);
    assert!(m.world.standing_orders.is_empty());
}

/// When the order cannot be given (`0x00968B00` returns 0: here the gate `0x008BE360` refuses while a battle
/// is pending), the entry is kept and tried again at the next turn start (`0x00B5C710` deletes only on
/// success).
#[test]
fn a_reinforcement_whose_order_cannot_be_given_keeps_its_entry() {
    let mut m = commander_recruitment_model();
    let (_, colonel) = raised_reinforcement(&mut m, 0);
    let entry = m.world.reinforcements.clone();
    m.pending_battle = Some(PendingBattle { attacker: ForceId(1001), defenders: vec![ForceId(1000)], settlement: None, resume: None, ambush: false });
    assert!(m.march_reinforcements(A).is_empty());
    assert_eq!(m.world.reinforcements, entry);
    assert!(m.world.standing_orders.is_empty());
    m.pending_battle = None;
    m.march_reinforcements(A);
    assert!(m.world.reinforcements.is_empty());
    assert!(m.world.standing_orders.contains_key(&colonel));
}

/// While a battle is pending (an ambush sprung by an earlier march of the same turn start), a standing
/// order's army does not walk and keeps its order (`0x00973B90` steps no mover then, `0x00973C20`); once
/// the battle is over the next resume merges. An invalid order is still cancelled then.
#[test]
fn a_standing_order_waits_while_a_battle_is_pending() {
    let mut m = commander_recruitment_model();
    let general = CharacterId(100);
    let (force, colonel) = raised_reinforcement(&mut m, 0);
    m.march_reinforcements(A);
    m.world.characters.get_mut(&general).unwrap().position = pos(10, 1);
    m.world.characters.get_mut(&colonel).unwrap().movement_points = 25;
    let before = m.world.characters[&colonel].position;
    m.pending_battle = Some(PendingBattle { attacker: ForceId(1001), defenders: vec![ForceId(1000)], settlement: None, resume: None, ambush: true });
    assert!(m.resume_standing_orders(A).is_empty());
    assert_eq!(m.world.characters[&colonel].position, before, "no walk while the battle is pending");
    assert_eq!((m.world.characters[&colonel].movement_points, m.world.forces.contains_key(&force)), (25, true));
    assert_eq!(m.world.standing_orders.get(&colonel), Some(&StandingOrder::JoinForce { into: ForceId(1000) }));
    m.pending_battle = None;
    let ev = m.resume_standing_orders(A);
    assert!(ev.contains(&CampaignEvent::CampaignArmiesMerge { force, into: ForceId(1000) }), "{ev:?}");

    // The still-valid check still runs while a battle is pending: an order whose target force is
    // gone is cancelled, not kept (`0x009653D0`, review).
    let mut m = commander_recruitment_model();
    let (_, colonel) = raised_reinforcement(&mut m, 0);
    m.march_reinforcements(A);
    assert!(m.world.standing_orders.contains_key(&colonel));
    m.pending_battle = Some(PendingBattle { attacker: ForceId(1001), defenders: vec![ForceId(1002)], settlement: None, resume: None, ambush: true });
    m.world.forces.remove(&ForceId(1000));
    assert!(m.resume_standing_orders(A).is_empty());
    assert!(m.world.standing_orders.is_empty(), "an invalid order is cancelled while a battle is pending");
}

/// A new order ends the march (`0x00924D30` cancels the holder's old order), and so does the target's
/// force going away (the still-valid check, vtable +0x10): the next resume cancels it.
#[test]
fn a_new_order_or_a_lost_target_ends_the_reinforcement_march() {
    let mut m = commander_recruitment_model();
    let (_, colonel) = raised_reinforcement(&mut m, 0);
    m.march_reinforcements(A);
    m.world.characters.get_mut(&colonel).unwrap().movement_points = 25;
    m.apply(CampaignCommand::MoveCharacter { character: colonel, to: pos(10, 1) }).unwrap();
    assert!(m.world.standing_orders.is_empty(), "the move is a new order");

    let mut m = commander_recruitment_model();
    raised_reinforcement(&mut m, 0);
    m.march_reinforcements(A);
    assert_eq!(m.world.standing_orders.len(), 1);
    m.world.forces.remove(&ForceId(1000));
    assert!(m.resume_standing_orders(A).is_empty());
    assert!(m.world.standing_orders.is_empty());
}

/// A new order the army has no action points for still ends the march: RunCharacterOrder `0x009651A0`
/// cancels the old order (`0x00924D30`) before the new one's execute finds the path and the points. One
/// refused at the gate (`0x008BE360`, here a pending battle) never reaches it, and the march stays.
#[test]
fn a_new_order_refused_for_its_points_still_ends_the_reinforcement_march() {
    let mut m = commander_recruitment_model();
    let (force, colonel) = raised_reinforcement(&mut m, 0);
    m.march_reinforcements(A);
    let march = m.world.standing_orders.clone();
    assert_eq!(march.len(), 1);
    let order = CampaignCommand::MoveForce { force, to: pos(10, 1) };
    m.pending_battle = Some(PendingBattle { attacker: ForceId(1001), defenders: vec![ForceId(1000)], settlement: None, resume: None, ambush: false });
    assert_eq!(m.apply(order.clone()), Err(CommandError::BattlePending));
    assert_eq!(m.world.standing_orders, march, "refused at the gate");
    m.pending_battle = None;
    assert!(matches!(m.apply(order), Err(CommandError::NotEnoughMovementPoints { available: 0, .. })));
    assert!(m.world.standing_orders.is_empty(), "the new order replaced the march");
    m.world.characters.get_mut(&colonel).unwrap().movement_points = 25;
    assert!(m.resume_standing_orders(A).is_empty(), "no march to resume");
}

/// An army's move that finds no path keeps the march: the move's pre-check (`0x00924790` / `0x009246C0`) asks
/// the path query `0x00B17A10` with no cost limit for a character who passes the army test `0x009D0F50`, and
/// stops before RunCharacterOrder `0x009651A0` reaches the cancel (`0x00924D30`) when there is none, so
/// also before the execute's order start (`0x008FF470`): the army's siege goes on.
#[test]
fn an_army_move_without_a_path_keeps_the_reinforcement_march() {
    use crate::campaign::pathing::PathGrid;
    let mut m = commander_recruitment_model();
    let (force, colonel) = raised_reinforcement(&mut m, 0);
    m.march_reinforcements(A);
    m.world.characters.get_mut(&colonel).unwrap().movement_points = 25;
    let march = m.world.standing_orders.clone();
    assert_eq!(march.get(&colonel), Some(&StandingOrder::JoinForce { into: ForceId(1000) }));
    // A map with no passable cell: no path anywhere.
    m.attach_terrain(crate::campaign::world::Terrain(Arc::new(PathGrid::new((0.0, 0.0), 1.0, 30, 4))));
    m.world.regions.get_mut(&RegionId(11)).unwrap().siege.besiegers.push(force);
    let besieging = |m: &CampaignModel| m.world.regions[&RegionId(11)].siege.besiegers.contains(&force);
    assert_eq!(m.apply(CampaignCommand::MoveForce { force, to: pos(15, 1) }), Err(CommandError::NoPath));
    assert_eq!(m.world.standing_orders, march, "the move's pre-check refused it before the cancel");
    assert!(besieging(&m), "and before the order start");
    assert_eq!(m.apply(CampaignCommand::MoveCharacter { character: colonel, to: pos(15, 1) }), Err(CommandError::NoPath));
    assert_eq!(m.world.standing_orders, march);
    assert!(besieging(&m));
}

/// `0x009D0D00`, the entry's check that its army may join the target's force: two armies always; an army
/// to a navy carrying nothing; never a navy carrying nothing to an army; a navy carrying an army when that
/// army fits; an agent without a force while the target's army has room; never a type that does not count
/// against a residence's room (`agents` #4).
#[test]
fn who_may_join_an_own_force() {
    let mut m = commander_recruitment_model();
    let (_, colonel) = raised_reinforcement(&mut m, 0);
    let general = CharacterId(100);
    assert!(m.may_join_own_force(colonel, general));
    let admiral = character(103, A, CharacterKind::Admiral, 60);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1002),
        MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(103)), units: vec![unit(4)], is_navy: true },
    );
    assert!(m.may_join_own_force(colonel, CharacterId(103)), "an army to a navy carrying nothing");
    assert!(!m.may_join_own_force(CharacterId(103), general), "a navy carrying nothing to an army");
    // A third army of A aboard the navy: an army joins the navy when it fits the carried army.
    let mut other = character(105, A, CharacterKind::General, 30);
    other.position = pos(3, 0);
    m.world.characters.insert(other.id, other);
    m.world.forces.insert(
        ForceId(1005),
        MilitaryForce { id: ForceId(1005), faction: A, commander: Some(CharacterId(105)), units: vec![unit(6)], is_navy: false },
    );
    m.world.embarked.insert(ForceId(1005), ForceId(1002));
    assert!(m.may_join_own_force(colonel, CharacterId(103)));
    assert!(m.may_join_own_force(CharacterId(103), general), "the carried army fits the target's");
    m.world.forces.get_mut(&ForceId(1005)).unwrap().units = (10..30).map(unit).collect();
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = (30..50).map(unit).collect();
    assert!(!m.may_join_own_force(CharacterId(103), general), "a full target army");
    assert!(!m.may_join_own_force(colonel, CharacterId(103)), "a full carried army");
    // An agent without a force, while the target's army has room.
    let rake = character(104, A, CharacterKind::Rake, 10);
    m.world.characters.insert(rake.id, rake);
    assert!(m.may_join_own_force(CharacterId(104), general));
    // ... and to a navy: the branch needs the target to lead any force (`0x009D0F10`, his +0x2A8).
    assert!(m.may_join_own_force(CharacterId(104), CharacterId(103)), "an agent without a force to an admiral");
    // Not to a character who leads no force.
    let spy = character(106, A, CharacterKind::Rake, 10);
    m.world.characters.insert(spy.id, spy);
    assert!(!m.may_join_own_force(CharacterId(104), CharacterId(106)));
    let mut rules = (*m.rules).clone();
    rules.limits.max_residence_characters = 0;
    m.rules = std::sync::Arc::new(rules);
    assert!(!m.may_join_own_force(CharacterId(104), general));
    // A type that does not count never joins.
    let mut rules = (*m.rules).clone();
    rules.residence_exempt_agents.insert("colonel".into());
    m.rules = std::sync::Arc::new(rules);
    assert!(!m.may_join_own_force(colonel, general));
}

/// `0x00B1E500` phase 1: an item raised through a general who leads no army but rides in another's army
/// (attached to one of its units, his holder +0x2B0) marches to that army's commander. Riding in a navy,
/// or an admiral attached to his own ship, the exe hands it to the admiral, and the retarget's relink
/// into the admiral's army list crashes (ORIGINAL BUG, `0x00B1B480` → `0x008CF82A`); ours spawns it in
/// the settlement and clears its target.
/// The exe reaches phase 1 with such a target only when he lost his army after the queueing: queueing
/// through a character with no army crashes in the item constructor (ORIGINAL BUG, `0x00AECEE0` →
/// `0x008CF82A`); ours queues it, as the recruits below through the rider and the admiral show.
#[test]
fn an_item_through_a_rider_marches_to_the_army_he_rides_in() {
    let general = CharacterId(100);
    let rider = CharacterId(105);
    for in_navy in [false, true] {
        let mut m = commander_recruitment_model();
        m.world.characters.insert(rider, character(105, A, CharacterKind::General, 30));
        m.world.characters.get_mut(&general).unwrap().position = pos(10, 1);
        let admiral = character(103, A, CharacterKind::Admiral, 60);
        m.world.characters.insert(admiral.id, admiral);
        let mut ship = unit(4);
        ship.character = Some(CharacterId(103));
        m.world.forces.insert(
            ForceId(1002),
            MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(103)), units: vec![ship], is_navy: true },
        );
        let mut mount = unit(5);
        mount.character = Some(rider);
        m.world.forces.get_mut(&ForceId(if in_navy { 1002 } else { 1000 })).unwrap().units.push(mount);
        let mut targets = vec![rider];
        if in_navy {
            targets.push(CharacterId(103));
        }
        for &t in &targets {
            m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: Some(t) }).unwrap();
        }
        let leaders = [general].into_iter().collect();
        assert_eq!(m.reinforcement_commander(rider, &leaders), (!in_navy).then_some(general));
        assert_eq!(m.reinforcement_commander(general, &leaders), Some(general));
        m.turn.humans = vec![A];
        m.start_campaign();
        let ev = m.end_turn();
        let trained: Vec<_> = ev.iter().filter_map(|e| if let CampaignEvent::UnitTrained { force, .. } = e { Some(*force) } else { None }).collect();
        assert_eq!(trained.len(), targets.len(), "{ev:?}");
        if in_navy {
            assert!(trained.iter().all(|f| m.world.regions[&RegionId(10)].garrison == Some(*f)), "{ev:?}");
            assert!(m.world.reinforcements.is_empty() && m.world.standing_orders.is_empty());
        } else {
            assert!(ev.contains(&CampaignEvent::CampaignArmiesMerge { force: trained[0], into: ForceId(1000) }), "{ev:?}");
            assert_eq!(m.world.regions[&RegionId(10)].garrison, None);
        }
    }
}

/// ORIGINAL BUG (`0x00B1E500` phase 1): the exe looks an item's (target, AI key) pair up before the
/// retarget to the rider's commander (`0x00B1E596`-`0x00B1E5AF`) and pushes it after it (`0x00B1E677`), so
/// an item through the general and one through a rider in his army, or two items through the rider, push
/// the general's pair twice; the group spawn `0x00B1D3F0` runs once per pair and gathers every finished
/// item of the general each time (`0x00B5C110` does not read the spawned flag +0x48): two armies, each
/// with both units. Ours raises one army with the two units.
#[test]
fn items_through_a_general_and_his_rider_raise_one_army() {
    let general = CharacterId(100);
    let rider = CharacterId(105);
    for targets in [[general, rider], [rider, rider]] {
        let mut m = commander_recruitment_model();
        m.world.characters.insert(rider, character(105, A, CharacterKind::General, 30));
        m.world.characters.get_mut(&general).unwrap().position = pos(10, 1);
        let mut mount = unit(5);
        mount.character = Some(rider);
        m.world.forces.get_mut(&ForceId(1000)).unwrap().units.push(mount);
        for t in targets {
            m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: Some(t) }).unwrap();
        }
        m.turn.humans = vec![A];
        m.start_campaign();
        let ev = m.end_turn();
        let trained: Vec<_> = ev.iter().filter_map(|e| if let CampaignEvent::UnitTrained { force, .. } = e { Some(*force) } else { None }).collect();
        assert_eq!(trained.len(), 2, "{targets:?}: {ev:?}");
        assert!(trained.iter().all(|f| *f == trained[0]), "one army: {targets:?}: {ev:?}");
        assert_eq!(m.world.regions[&RegionId(10)].garrison, None);
    }
}

/// The recruit command takes any character as the item's target: neither the handler (`0x00936B90`, its
/// target read by `0x008FDE50`) nor the queueing (`0x00B58DD0`, the item constructor `0x00AECEE0`) checks
/// his faction, so another faction's general is queued as the target.
#[test]
fn a_recruit_through_another_factions_general_is_queued() {
    let mut m = commander_recruitment_model();
    let foreign = CharacterId(101);
    assert_ne!(m.world.characters[&foreign].faction, A);
    m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: Some(foreign) }).unwrap();
    assert_eq!(m.world.regions[&RegionId(10)].recruitment_queue.last().unwrap().target, Some(foreign));
}

/// An item whose commander died before it finished has lost its target (`0x00B57F40`) and spawns as a
/// settlement recruit; so does one whose commander leads no army (`0x00B1E500` phase 1: +0x18 cleared).
#[test]
fn an_item_whose_commander_is_gone_or_leads_no_army_spawns_in_the_settlement() {
    for dies in [true, false] {
        let mut m = commander_recruitment_model();
        let general = CharacterId(100);
        m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_recruit".into(), target: Some(general) }).unwrap();
        if dies {
            m.character_dies(general);
        } else {
            m.world.forces.remove(&ForceId(1000));
        }
        m.turn.humans = vec![A];
        m.start_campaign();
        let ev = m.end_turn();
        let Some(&CampaignEvent::UnitTrained { force, .. }) = ev.iter().find(|e| matches!(e, CampaignEvent::UnitTrained { .. })) else { panic!("{ev:?}") };
        assert_eq!(m.world.regions[&RegionId(10)].garrison, Some(force), "dies: {dies}");
        assert!(m.world.reinforcements.is_empty());
    }
}

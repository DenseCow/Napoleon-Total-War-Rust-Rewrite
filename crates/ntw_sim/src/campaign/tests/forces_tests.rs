//! Campaign model tests: forces.

use super::*;

/// The ranking's power (`0x008B2150` with flag 0) sums each unit's raw upkeep: no effects, whatever
/// the unit's strength (A: one test_unit, B: two; 80 of 100 men each). A unit with no record counts
/// 0 and is reported. A navy's units go to the naval total, an army's to the land total.
#[test]
fn faction_power_is_the_raw_upkeep_of_the_forces() {
    use super::negotiation::FactionPower;
    let mut m = test_model();
    let mut unknown = std::collections::BTreeSet::new();
    let p = m.faction_powers(&mut unknown);
    let land = |land| Some(FactionPower { land, naval: 0 });
    assert_eq!((p.get(&A).copied(), p.get(&B).copied(), p.get(&C).copied()), (land(10), land(20), None));
    assert_eq!(p[&B].total(), 20);
    assert!(unknown.is_empty());
    m.world.forces.get_mut(&ForceId(1000)).unwrap().is_navy = true;
    assert_eq!(m.faction_powers(&mut unknown)[&A], FactionPower { land: 0, naval: 10 });
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].unit_key = "no_such_unit".into();
    let r = m.faction_rankings();
    assert_eq!(r.unknown_units.iter().map(String::as_str).collect::<Vec<_>>(), ["no_such_unit"]);
    assert!(r.ranks.contains_key(&A));
}

/// A force without a commander counts 0 in both the upkeep and the ranking power: `0x008F9C50`
/// returns 0 for the whole force when its +0x74 commander handle resolves to null (`0x008F9C62`),
/// before the upkeep / power flag is read.
#[test]
fn a_force_without_a_commander_costs_and_counts_nothing() {
    let mut m = test_model();
    let fx = super::effects::Effects::compute_for(&m, B);
    assert_eq!(super::economy::faction_upkeep_split_with(&m, &fx, B), (20, 0));
    m.world.forces.get_mut(&ForceId(1001)).unwrap().commander = None;
    assert_eq!(super::economy::faction_upkeep_split_with(&m, &fx, B), (0, 0));
    let p = m.faction_powers(&mut std::collections::BTreeSet::new());
    assert_eq!(p[&B].total(), 0);
    assert_eq!(p[&A].total(), 10);
}

/// The negotiation's constructor (`0x00BF5A60`, run by the `BeginNegotiation` command) picks the
/// human proposer's greeting (the recipient's `receive` line) and the human recipient's `approach`
/// line; an override row costs one RNG draw, no override row none, and reading the negotiation
/// draws nothing. `EndNegotiation` ends it once.
#[test]
fn a_negotiation_picks_its_greetings_once() {
    use super::negotiation::Greeting;
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.factions.get_mut(&B).unwrap().government_key = "gov_republic".into();
    let mut rules = (*m.rules).clone();
    rules.faction_cultures.insert("test_faction_b".into(), "european".into());
    for attitude in ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"] {
        let key = (format!("receive_{attitude}"), "european".to_owned(), "gov_republic".to_owned());
        rules.negotiation_strings.insert(key.clone(), "generic".into());
        rules.negotiation_overrides.insert((key.0, key.1, key.2, "test_faction_b".into()), "own".into());
    }
    m.rules = Arc::new(rules);
    // A (human) proposes to B: B's receive line, one draw for its override row.
    let before = m.rng;
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let mut once = before;
    let pick = if once.unit_float() < 0.5 { "own" } else { "generic" };
    assert_eq!(m.rng, once);
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!((n.serial, n.proposer, n.recipient), (1, A, B));
    assert_eq!(n.greeting, Some(Greeting::Line(pick.into())));
    assert_eq!(n.approach, None, "B is not human");
    let _ = m.faction_rankings();
    assert_eq!(m.rng, once, "reading draws nothing");
    // B proposes to A (human), over the open one: no greeting; B's approach line has no row:
    // Missing, no draw. Building over it ends nothing.
    m.apply(CampaignCommand::BeginNegotiation { proposer: B, recipient: A }).unwrap();
    assert_eq!(m.rng, once);
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!((n.serial, n.greeting), (2, None));
    let key = ("approach_neutral".to_owned(), "european".to_owned(), "gov_republic".to_owned());
    assert_eq!(n.approach, Some(Greeting::Missing { faction: "test_faction_b".into(), key }), "the log names the faction and the row");
    assert_eq!(m.negotiations.ended, 0);
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    assert_eq!((m.negotiations.current.is_none(), m.negotiations.begun, m.negotiations.ended), (true, 2, 1), "ended once");
    assert_eq!(
        m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: FactionId(999) }),
        Err(CommandError::UnknownFaction(FactionId(999)))
    );
    assert_eq!(m.negotiations.begun, 2, "a rejected begin leaves no negotiation");
}

#[test]
fn merging_takes_the_commander_along_and_destroying_kills_attached_characters() {
    let mut m = test_model();
    // Force 1000 (commander 100) has unit 1; give the commander his bodyguard link.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].character = Some(CharacterId(100));
    let mut events = Vec::new();
    // Merge force 1000 into a second force of faction A.
    m.world.characters.insert(CharacterId(105), character(105, A, CharacterKind::General, 26));
    m.world.forces.insert(
        ForceId(1010),
        MilitaryForce { id: ForceId(1010), faction: A, commander: Some(CharacterId(105)), units: vec![unit(20)], is_navy: false },
    );
    m.merge_units(ForceId(1000), ForceId(1010), &mut events);
    assert!(!m.world.forces.contains_key(&ForceId(1000)));
    let into = &m.world.forces[&ForceId(1010)];
    assert_eq!(into.units.len(), 2);
    assert_eq!(into.units[1].character, Some(CharacterId(100)));
    // The former commander lives on, attached to his unit, at the receiving army's spot.
    assert_eq!(m.world.characters[&CharacterId(100)].position, m.world.characters[&CharacterId(105)].position);
    assert!(m.force_of(CharacterId(100)).is_none());
    // Destroying the receiving force kills its commander and the attached general.
    m.destroy_force(ForceId(1010), &mut events);
    assert!(!m.world.characters.contains_key(&CharacterId(105)));
    assert!(!m.world.characters.contains_key(&CharacterId(100)));
}

#[test]
fn construction_completes_and_unlocks_upgrades() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    let t0 = m.world.factions[&A].treasury;
    // Slot 1 is empty: only level 0 of a chain fits.
    let ev = m
        .apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level".into() })
        .unwrap();
    assert_eq!(ev, vec![CampaignEvent::BuildingConstructionIssuedByPlayer { region: RegionId(10) }]);
    assert_eq!(m.world.factions[&A].treasury, t0 - 300);
    assert_eq!(
        m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level".into() }),
        Err(CommandError::SlotBusy)
    );
    // Slot 0 holds level 0: its upgrade is allowed (3 turns).
    m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level_2".into() })
        .unwrap();
    m.end_turn();
    assert!(m.world.regions[&RegionId(10)].slots[1].building.is_none());
    let ev = m.end_turn();
    assert!(ev.contains(&CampaignEvent::BuildingCompleted { region: RegionId(10), slot: SlotRef::Slot(1) }));
    assert_eq!(m.world.regions[&RegionId(10)].slots[1].building.as_ref().unwrap().level_key, "test_building_level");
    m.end_turn();
    assert_eq!(m.world.regions[&RegionId(10)].slots[0].building.as_ref().unwrap().level_key, "test_building_level_2");
    assert!(m.world.regions[&RegionId(10)].construction.is_empty());
}

#[test]
fn move_force_spends_and_end_turn_refills_movement_points() {
    let mut m = test_model();
    let ev = m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(18, 24) }).unwrap();
    assert_eq!(ev.len(), 2);
    assert!(matches!(&ev[0], CampaignEvent::CharacterMoved { character: CharacterId(100), path } if path.len() == 2));
    assert_eq!(ev[1], CampaignEvent::MovementPointsExhausted { character: CharacterId(100) });
    let general = &m.world.characters[&CharacterId(100)];
    assert_eq!(general.movement_points, 0);
    assert_eq!(general.position, pos(18, 24));
    assert!(m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(19, 24) }).is_err());
    m.end_turn();
    assert_eq!(m.world.characters[&CharacterId(100)].movement_points, 30);
}

#[test]
fn attack_and_autoresolve() {
    fn fight() -> (CampaignModel, Vec<CampaignEvent>) {
        let mut m = test_model();
        m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(5, 0);
        m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
        let ev = m.apply(CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }).unwrap();
        assert!(ev.contains(&CampaignEvent::PreBattle { attacker: ForceId(1000) }));
        assert!(m.pending_battle.is_some());
        // Nothing else may happen until the battle is fought.
        assert_eq!(m.apply(CampaignCommand::EndTurn), Err(CommandError::BattlePending));
        let ev = m.apply(CampaignCommand::Autoresolve).unwrap();
        (m, ev)
    }
    let (m, ev) = fight();
    // 80 men against 160 of the same quality: B wins.
    assert!(matches!(ev[0], CampaignEvent::BattleCompleted { attacker: ForceId(1000), attacker_won: false, .. }));
    assert!(m.pending_battle.is_none());
    let men_b: u32 = m.world.forces[&ForceId(1001)].units.iter().map(|u| u.men).sum();
    assert!(men_b < 160);
    // Deterministic.
    assert_eq!(m.state_hash(), fight().0.state_hash());
}

#[test]
fn enter_and_occupy_settlements_and_merge() {
    let mut m = test_model();
    // Own settlement (region 12 at x = 12): enter it.
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(10, 0);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(12) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEntersGarrison { character: CharacterId(100), region: RegionId(12) }));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, Some(RegionId(12)));
    // An undefended enemy settlement (region 11 at x = 11) is occupied at once.
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(-20, 0);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::SettlementOccupied { region: RegionId(11), faction: A }));
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    // Merge: a second army of A joins the first.
    let mut g = character(103, A, CharacterKind::General, 30);
    g.position = pos(11, 1);
    m.world.characters.insert(CharacterId(103), g);
    m.world.forces.insert(
        ForceId(1005),
        MilitaryForce { id: ForceId(1005), faction: A, commander: Some(CharacterId(103)), units: vec![unit(9)], is_navy: false },
    );
    let ev = m.apply(CampaignCommand::MergeForces { force: ForceId(1005), into: ForceId(1000) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CampaignArmiesMerge { force: ForceId(1005), into: ForceId(1000) }));
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 2);
    assert!(!m.world.forces.contains_key(&ForceId(1005)));
}

#[test]
fn units_per_army_come_from_the_campaign_and_a_mod() {
    // A full 20-unit army (the original's cap) takes no more; the campaign's own value and a new
    // campaign under a mod's `max_land_units` row both lift it.
    let setup = |extra: usize| {
        let mut m = test_model();
        m.world.forces.get_mut(&ForceId(1000)).unwrap().units = (0..20).map(|i| unit(500 + i)).collect();
        let mut g = character(103, A, CharacterKind::General, 30);
        g.position = m.world.characters[&CharacterId(100)].position;
        m.world.characters.insert(CharacterId(103), g);
        let units = (0..extra as i32).map(|i| unit(600 + i)).collect();
        m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: A, commander: Some(CharacterId(103)), units, is_navy: false });
        m
    };
    let merge = CampaignCommand::MergeForces { force: ForceId(1005), into: ForceId(1000) };
    let mut m = setup(1);
    assert_eq!((m.max_units(false), m.max_units(true)), (20, 20));
    assert_eq!(m.apply(merge.clone()), Err(CommandError::Unsupported("the receiving force is full")));
    // The campaign's own value (CAMPAIGN_MODEL #23).
    m.force_caps.army = 21;
    m.apply(merge.clone()).unwrap();
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 21);
    // A new campaign: the original's caps at each `campaign_unit_multiplier` step (20 and 6 / 8 /
    // 10 / 20; the default 0.75 gives 10 ships)...
    let l = crate::limits::GameLimits::default();
    let caps = |m: Option<f32>| super::rules::ForceCaps::new_campaign(&l, m);
    assert_eq!([0.25, 0.5, 0.75, 1.0].map(|s| caps(Some(s))).map(|c| (c.army, c.navy)), [(20, 6), (20, 8), (20, 10), (20, 20)]);
    assert_eq!(caps(None), caps(Some(0.75)));
    // ...and under a mod's 40-unit armies: 25 units arrive, 20 fit.
    let mut m = setup(25);
    Arc::make_mut(&mut m.rules).limits.max_land_units = 40;
    m.force_caps = super::rules::ForceCaps::new_campaign(&m.rules.limits, Some(1.0));
    assert_eq!((m.max_units(false), m.max_units(true)), (40, 20));
    m.apply(merge).unwrap();
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 40);
    assert_eq!(m.world.forces[&ForceId(1005)].units.len(), 5);
}

#[test]
fn command_queue_applies_in_order() {
    let mut m = test_model();
    let mut q = CommandQueue::new();
    q.push(CampaignCommand::DeclareWar { a: A, b: B });
    q.push(CampaignCommand::DeclareWar { a: A, b: B });
    q.push(CampaignCommand::EndTurn);
    assert_eq!(q.len(), 3);
    let results = m.apply_queue(&mut q);
    assert!(q.is_empty());
    assert!(results[0].is_ok());
    assert_eq!(results[1], Err(CommandError::AlreadyAtWar(A, B)));
    assert!(results[2].is_ok());
    assert_eq!(m.calendar.turns_elapsed, 1);
}

#[test]
fn identical_models_stay_identical_for_50_turns() {
    let mut a = test_model();
    let mut b = test_model();
    play(&mut a, 50);
    play(&mut b, 50);
    assert_eq!(a, b);
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.world.forces.len() > 2);
    assert_ne!(a.state_hash(), test_model().state_hash());
    let mut c = test_model();
    c.apply(CampaignCommand::SetTaxLevel { faction: C, class: TaxClass::Upper, level: "tax_low".into() }).unwrap();
    play(&mut c, 50);
    assert_ne!(a.state_hash(), c.state_hash());
}

/// Embarking and landing without terrain (the straight-line stand-in of `embark`): the army walks
/// to the navy and boards it, the navy carries it, a move order lands it and landing ends its turn.
#[test]
fn armies_board_navies_sail_and_land() {
    let mut m = test_model();
    let mut admiral = character(103, A, CharacterKind::Admiral, 60);
    admiral.position = pos(2, 0);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1002),
        MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(103)), units: vec![unit(4)], is_navy: true },
    );
    // Only armies board navies of their own faction.
    assert!(m.apply(CampaignCommand::Embark { force: ForceId(1001), navy: ForceId(1002) }).is_err());
    assert!(m.apply(CampaignCommand::Embark { force: ForceId(1002), navy: ForceId(1002) }).is_err());
    let ev = m.apply(CampaignCommand::Embark { force: ForceId(1000), navy: ForceId(1002) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEmbarksNavy { character: CharacterId(100), navy: ForceId(1002) }));
    assert_eq!(m.carrier_of(ForceId(1000)), Some(ForceId(1002)));
    assert_eq!(m.passengers_of(ForceId(1002)), vec![ForceId(1000)]);
    // Boarding spends nothing itself; the walk to the fleet did.
    let general = &m.world.characters[&CharacterId(100)];
    assert_eq!(general.position, pos(2, 0));
    assert!(general.movement_points > 0 && general.movement_points < 30);
    // An embarked army does not board again and may not attack.
    assert!(m.apply(CampaignCommand::Embark { force: ForceId(1000), navy: ForceId(1002) }).is_err());
    assert!(m.apply(CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }).is_err());
    // A second army joins the carried one when both fit into one army (20 units).
    let mut colonel = character(104, A, CharacterKind::Colonel, 25);
    colonel.position = pos(2, 0);
    m.world.characters.insert(colonel.id, colonel);
    m.world.forces.insert(
        ForceId(1003),
        MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(104)), units: vec![unit(5), unit(6)], is_navy: false },
    );
    m.apply(CampaignCommand::Embark { force: ForceId(1003), navy: ForceId(1002) }).unwrap();
    assert!(!m.world.forces.contains_key(&ForceId(1003)), "merged into the carried army");
    assert_eq!(m.world.forces[&ForceId(1000)].units.len(), 3);
    assert_eq!(m.passengers_of(ForceId(1002)), vec![ForceId(1000)]);
    // The navy sails and carries the army.
    m.apply(CampaignCommand::MoveForce { force: ForceId(1002), to: pos(10, 0) }).unwrap();
    assert_eq!(m.world.characters[&CharacterId(100)].position, pos(10, 0));
    // Next turn (refilled by hand: after end_turn only the current faction may act): a move order
    // to the embarked army lands it; landing ends its turn.
    for c in m.world.characters.values_mut() {
        c.movement_points = c.max_movement_points;
    }
    let ev = m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(14, 0) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterDisembarksNavy { character: CharacterId(100), navy: ForceId(1002) }), "{ev:?}");
    assert_eq!(m.carrier_of(ForceId(1000)), None);
    let general = &m.world.characters[&CharacterId(100)];
    assert_eq!((general.position, general.movement_points), (pos(14, 0), 0));
    // The embark state is part of the state hash.
    let mut a = test_model();
    let b = a.clone();
    a.world.embarked.insert(ForceId(1000), ForceId(1001));
    assert_ne!(a.state_hash(), b.state_hash());
}

/// An army that has already moved this turn cannot land before next turn (CONFIRMED rule:
/// landing costs `ceil(max(g, 1)) + the army's spent fraction`).
#[test]
fn an_army_that_moved_lands_next_turn() {
    let mut m = test_model();
    let mut admiral = character(103, A, CharacterKind::Admiral, 60);
    admiral.position = pos(0, 0);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1002),
        MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(103)), units: vec![unit(4)], is_navy: true },
    );
    m.world.embarked.insert(ForceId(1000), ForceId(1002));
    m.world.characters.get_mut(&CharacterId(100)).unwrap().movement_points = 20; // spent a third
    let ev = m.apply(CampaignCommand::Disembark { force: ForceId(1000), to: pos(3, 0) });
    assert!(matches!(ev, Err(CommandError::NotEnoughMovementPoints { .. })), "{ev:?}");
    assert_eq!(m.carrier_of(ForceId(1000)), Some(ForceId(1002)));
}

#[test]
fn garrison_repression_and_automated_policing() {
    // The shipped caps: policing_garrison_cap 15, policing_automated_cap 4.
    let mut rules = CampaignRules::test_rules();
    rules.variables.insert("policing_garrison_cap".into(), 15.0);
    rules.variables.insert("policing_automated_cap".into(), 4.0);
    // Population factors of 0x008B18F0.
    assert_eq!(economy::garrison_population_factor(5_000), 1.5);
    assert_eq!(economy::garrison_population_factor(9_999), 1.25);
    assert_eq!(economy::garrison_population_factor(100_000), 1.0);
    assert_eq!(economy::garrison_population_factor(500_000), 0.75);
    assert_eq!(economy::garrison_population_factor(2_000_000), 0.5);
    assert_eq!(economy::garrison_population_factor(17_386_000), 0.3);
    // 5 units in a town of 2 million: 2.5 -> 2 (ties to even); 20 units in a village: capped at 15.
    assert_eq!(economy::garrison_repression(&rules, 5, 2_000_000), 2.0);
    assert_eq!(economy::garrison_repression(&rules, 20, 5_000), 15.0);
    assert_eq!(economy::garrison_repression(&rules, 0, 5_000), 0.0);
    // Policing only covers unrest, up to the cap.
    assert_eq!(economy::automated_policing(&rules, 3.0), 0.0);
    assert_eq!(economy::automated_policing(&rules, -3.0), 3.0);
    assert_eq!(economy::automated_policing(&rules, -10.0), 4.0);

    // In the model: region 12 (A, tax_high -> lower -10) gets a 2-unit garrison.
    let mut m = test_model();
    let mut r = (*m.rules).clone();
    r.variables.insert("policing_garrison_cap".into(), 15.0);
    r.variables.insert("policing_automated_cap".into(), 4.0);
    m.rules = Arc::new(r);
    m.apply(CampaignCommand::SetTaxLevel { faction: A, class: TaxClass::Lower, level: "tax_high".into() }).unwrap();
    m.world.regions.get_mut(&RegionId(12)).unwrap().garrison = Some(ForceId(1000));
    // population 200 000 (test helper: gdp × 100): factor 1.0, 1 unit -> 1; -10 + 1 = -9, policing 4.
    assert_eq!(economy::public_order(&m, RegionId(12)).lower, -10.0 + 1.0 + 4.0);
    assert_eq!(economy::public_order(&m, RegionId(12)).upper, 1.0);
}

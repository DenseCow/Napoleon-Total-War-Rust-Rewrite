//! Campaign model tests: construction.

use super::*;

/// The tax-level bundles are compiled when set (through the rules' fixed mapping), not on every region
/// effect set; the region's set merges the compiled bundle of its owner's level.
#[test]
fn tax_bundles_are_compiled_once_and_merged_into_the_region_set() {
    let mut m = test_model();
    let rules = CampaignRules::test_rules();
    assert_eq!(rules.effects.tax_set("lower_classes", "tax_high").unwrap().get_int("happiness_active_lower_tax"), -10);
    assert_eq!(rules.effects.tax_rows("lower_classes", "tax_high"), Some(&vec![("happy_active_lower_tax".to_string(), -10.0)]));
    m.rules = Arc::new(rules);
    m.world.factions.get_mut(&A).unwrap().tax_lower = "tax_high".into();
    let r = m.world.regions[&RegionId(10)].clone();
    assert_eq!(economy::region_effect_set(&m, &r).get_int("happiness_active_lower_tax"), -10);
}

#[test]
fn a_negative_cost_shows_unaffordable_and_is_paid_out_only_from_a_negative_treasury() {
    let mut m = test_model_with([map_chain_cost("building_cost_mod_all", "test_chain")]);
    m.turn.humans = vec![A];
    m.start_campaign();
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_all", -150.0)]));
    m.rules = Arc::new(rules);
    // Slot 0 holds the test building; its upgrade (600) now costs 600 × −50 × 0.01 = −300.
    let upgrade = |m: &CampaignModel| m.construction_options(RegionId(10), SlotRef::Slot(0)).into_iter().find(|o| o.level_key == "test_building_level_2").unwrap();
    let cmd = CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level_2".into() };
    m.world.factions.get_mut(&A).unwrap().treasury = 1000;
    let o = upgrade(&m);
    assert_eq!((o.cost, o.affordable), (-300, false), "unsigned: 0xFFFFFED4 > 1000");
    // The command's list drops the option (`0x00B43300` with flag 1 set): refused, nothing paid.
    assert_eq!(m.apply(cmd.clone()), Err(CommandError::NegativeCost { cost: -300, available: 1000 }));
    assert_eq!(m.world.factions[&A].treasury, 1000);
    // A treasury of −5 is 0xFFFFFFFB unsigned, not below the cost: listed, and −5 ≥ −300 signed, so it pays out.
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    assert!(upgrade(&m).affordable);
    m.apply(cmd).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 295);
}

/// Review: `NegativeCost` is only the unsigned test's refusal of a negative cost. A positive cost passes the
/// unsigned test against a negative treasury (0xFFFFFFFB) and is refused by the signed one: a plain shortfall.
#[test]
fn the_signed_treasury_refusal_is_insufficient_funds() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    let cmd = CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(0), level_key: "test_building_level_2".into() };
    let cost = m.can_build(RegionId(10), SlotRef::Slot(0), "test_building_level_2").unwrap();
    assert!(cost > 0 && super::treasury::construction_affordable(-5, cost));
    assert_eq!(m.apply(cmd), Err(CommandError::InsufficientFunds { needed: cost, available: -5 }));
    assert_eq!(m.world.factions[&A].treasury, -5);
}

#[test]
fn an_undamaged_building_costs_nothing_to_repair_whatever_the_modifier() {
    use super::effects::SavedBonus;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().bonus_base =
        vec![SavedBonus { kind: 2, bonus: 0, value: 50.0, qualifier: "test_chain".into(), ..Default::default() }];
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Slot(0)), 0);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
    // 300 × 50 × 0.01 = 150, × 150 × 0.01 = 225.
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Slot(0)), 225);
    // The level cost is read unsigned (0xFFFFFFFF) and an out-of-range product is FISTP's 0x80000000, as in the
    // construction cost.
    let mut rules = (*m.rules).clone();
    rules.buildings.get_mut("test_building_level").unwrap().cost = -1;
    m.rules = Arc::new(rules);
    assert_eq!(m.repair_cost_uncapped(RegionId(10), SlotRef::Slot(0)), i32::MIN);
}

#[test]
fn a_repair_costing_i32_min_is_queued_and_charges_nothing() {
    // Cost 0xFFFFFFFF, health 50: the repair cost is FISTP's 0x80000000. `0x00B16430` lets it through (cost ≤
    // treasury, signed) and `0x00B66260` charges only a cost above 0, so the repair is queued for free.
    let setup = |human: bool| {
        let mut m = test_model();
        let humans = if human { vec![A] } else { Vec::new() };
        m.turn = TurnState::in_turn_of(A, humans);
        let mut rules = (*m.rules).clone();
        rules.buildings.get_mut("test_building_level").unwrap().cost = -1;
        m.rules = Arc::new(rules);
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
        m.world.factions.get_mut(&A).unwrap().treasury = 1000;
        m
    };
    let mut m = setup(true);
    assert_eq!(m.repair_cost(RegionId(10), SlotRef::Slot(0)), i32::MIN);
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 1000);
    assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| (c.slot, c.cost)), Some((SlotRef::Slot(0), i32::MIN)));
    // Cancelling credits the stored cost back whatever was charged (`0x00B1A790`), wrapping as the exe's add.
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 1000i32.wrapping_add(i32::MIN));
    // The AI's repair goes the same way.
    let mut ai = setup(false);
    ai.ai_repairs(A);
    assert_eq!(ai.world.factions[&A].treasury, 1000);
    assert_eq!(ai.world.regions[&RegionId(10)].construction.last().map(|c| c.slot), Some(SlotRef::Slot(0)));
}

#[test]
fn an_ai_repair_is_capped_at_the_treasury_and_free_in_debt() {
    let ai = |treasury: i32| {
        let mut m = test_model();
        m.turn.humans = Vec::new();
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
        m.world.factions.get_mut(&A).unwrap().treasury = treasury;
        m.ai_repairs(A);
        assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| c.slot), Some(SlotRef::Slot(0)));
        m.world.factions[&A].treasury
    };
    // Full cost 150; the AI's cost is capped at its treasury (`0x00B66410`), so it always passes `0x00B16430`.
    assert_eq!(ai(100), 0);
    assert_eq!(ai(1000), 850);
    // In debt the capped cost is negative and `0x00B66260` charges nothing.
    assert_eq!(ai(-20), -20);
}

/// `0x00AA4910` skips a repair where the AI may not shop (`0x00A8B2D0`): at the slot's position, at the
/// settlement's for the walls.
#[test]
fn the_ai_does_not_repair_a_building_under_the_scripted_shroud() {
    use super::visibility::{CellSet, Shroud, SightGrid};
    let mut m = test_model();
    m.turn.humans = vec![B];
    let slot_at = pos(3, 3);
    let town = {
        let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
        r.slots[0].position = Some(slot_at);
        r.slots[0].building.as_mut().unwrap().health = 50;
        let mut walls = r.slots[0].building.clone();
        walls.as_mut().unwrap().health = 50;
        r.fortification = walls;
        r.settlement.position
    };
    let grid = SightGrid::centred(64, 64, 64);
    let cell = |p: (Fixed20, Fixed20)| grid.cell_of((p.0.to_f32(), p.1.to_f32())).unwrap();
    let mut hidden = CellSet::new(64, 64);
    let (x, z) = cell(slot_at);
    hidden.set(x, z);
    assert_ne!(cell(slot_at), cell(town));
    m.world.sight_grid = Some(grid);
    m.world.shrouds.insert(B, Shroud { explored: CellSet::new(64, 64), visible: CellSet::new(64, 64), hidden, active: true });
    m.set_ai_shopping_shroud(true);
    let queued = |m: &CampaignModel| m.world.regions[&RegionId(10)].construction.iter().map(|c| c.slot).collect::<Vec<_>>();
    m.ai_repairs(A);
    assert_eq!(queued(&m), vec![SlotRef::Walls], "the hidden slot is not repaired");
    // Hiding the settlement holds the walls back too; the script's off lets the slot through.
    let mut m2 = m.clone();
    m2.world.regions.get_mut(&RegionId(10)).unwrap().construction.clear();
    let (x, z) = cell(town);
    m2.world.shrouds.get_mut(&B).unwrap().hidden.set(x, z);
    m2.ai_repairs(A);
    assert!(queued(&m2).is_empty());
    m.set_ai_shopping_shroud(false);
    m.ai_repairs(A);
    assert_eq!(queued(&m), vec![SlotRef::Walls, SlotRef::Slot(0)]);
}

#[test]
fn cancelling_a_repair_credits_its_stored_cost() {
    let mut m = test_model();
    m.turn = TurnState::in_turn_of(A, vec![A]);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 50;
    // A human repair with the treasury below its cost: `0x00B66260` tests no funds and charges 150.
    m.world.factions.get_mut(&A).unwrap().treasury = 100;
    m.apply(CampaignCommand::RepairBuilding { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, -50);
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, 100);
    // An AI repair queued in debt charged nothing but stores its capped cost −20; a cancel credits −20 (the exe's
    // `0x00B1A790` credits the stored cost, not what was charged).
    m.turn = TurnState::in_turn_of(A, Vec::new());
    m.world.factions.get_mut(&A).unwrap().treasury = -20;
    m.ai_repairs(A);
    assert_eq!(m.world.factions[&A].treasury, -20);
    assert_eq!(m.world.regions[&RegionId(10)].construction.last().map(|c| c.cost), Some(-20));
    m.apply(CampaignCommand::CancelConstruction { region: RegionId(10), slot: SlotRef::Slot(0) }).unwrap();
    assert_eq!(m.world.factions[&A].treasury, -40);
}

#[test]
fn region_options_build_the_same_lists_as_each_slot() {
    let m = test_model();
    let new_levels = m.new_levels_by_slot_type();
    let all = m.region_construction_options(RegionId(10), Some(&new_levels));
    assert!(all.iter().any(|(_, o)| !o.is_empty()));
    for (slot, options) in all {
        assert_eq!(options, m.construction_options_in(RegionId(10), slot, Some(&new_levels)), "{slot:?}");
    }
}

#[test]
fn a_treasury_below_zero_shows_every_card_affordable_but_the_command_refuses() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.start_campaign();
    m.world.factions.get_mut(&A).unwrap().treasury = -5;
    // The panel's flag compares unsigned (`0x00B43300`).
    let opts = m.construction_options(RegionId(10), SlotRef::Slot(1));
    assert!(!opts.is_empty() && opts.iter().all(|o| o.affordable));
    // The construct command compares signed (`0x00B13DD0`).
    assert!(matches!(
        m.apply(CampaignCommand::ConstructBuilding { region: RegionId(10), slot: SlotRef::Slot(1), level_key: "test_building_level".into() }),
        Err(CommandError::InsufficientFunds { .. })
    ));
}

#[test]
fn a_dead_commanders_force_goes_to_a_new_colonel() {
    let mut m = test_model();
    // Force 1001 (B, two units) loses its commander 101; force 1000's commander 100 dies with an
    // empty force after its only unit is removed.
    let pos101 = m.world.characters[&CharacterId(101)].position;
    m.character_dies(CharacterId(101));
    assert!(!m.world.characters.contains_key(&CharacterId(101)));
    let f = &m.world.forces[&ForceId(1001)];
    let colonel = f.commander.expect("a new commander");
    assert_ne!(colonel, CharacterId(101));
    assert_eq!(f.units[0].character, Some(colonel));
    let c = &m.world.characters[&colonel];
    assert_eq!((c.kind, c.faction, c.position), (CharacterKind::Colonel, B, pos101));
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.clear();
    m.character_dies(CharacterId(100));
    assert!(!m.world.forces.contains_key(&ForceId(1000)));
    assert!(m.world.forces.values().all(|f| f.commander.is_some_and(|c| m.world.characters.contains_key(&c))));
}

#[test]
fn schools_research_technologies_and_unlock_the_next() {
    use super::research::{state, ResearchError};
    use super::rules::TechRules;
    let mut m = test_model_with([map_basic("research_points", "research_points")]);
    let mut rules = (*m.rules).clone();
    // The test building is a school: research_points 10.
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("research_points", 10.0)]));
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    rules.technologies.insert("admin2_b".into(), TechRules { cost: 30, building_level: "test_building_level".into(), requires: vec!["admin1_a".into()], factions: vec![], thread: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin1_a".into(), state::AVAILABLE), ("admin2_b".into(), state::UNAVAILABLE)];
    let school_pos = pos(10, 2);
    {
        let s = &mut m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0];
        s.id = 77;
        s.position = Some(school_pos);
    }
    // A gentleman with research 2 in the school adds 2 + 1.
    let mut g = character(120, A, CharacterKind::Gentleman, 10);
    g.position = school_pos;
    m.world.characters.insert(g.id, g);
    m.world.character_details.entry(CharacterId(120)).or_default().attributes = vec![("research".into(), 2)];
    assert_eq!(m.research_rate(RegionId(10), 0, "admin1_a"), 13.0);
    // Not a school, not available yet.
    assert_eq!(m.start_research(RegionId(10), 1, "admin1_a"), Err(ResearchError::NotASchool));
    assert_eq!(m.start_research(RegionId(10), 0, "admin2_b"), Err(ResearchError::NotAvailable));
    m.start_research(RegionId(10), 0, "admin1_a").unwrap();
    assert_eq!(m.researching_at(RegionId(10), 0).as_deref(), Some("admin1_a"));
    assert!(m.research_step(A).is_empty());
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].progress, 13.0);
    // The second step reaches the cost: researched, progress = cost, school free, the next tech opens.
    assert_eq!(m.research_step(A), vec!["admin1_a".to_string()]);
    let d = &m.world.faction_details[&A];
    assert_eq!(d.research["admin1_a"].progress, 25.0);
    assert_eq!(d.research["admin1_a"].researcher, 0);
    assert_eq!(m.tech_state(A, "admin1_a"), Some(state::RESEARCHED));
    assert_eq!(m.tech_state(A, "admin2_b"), Some(state::AVAILABLE));
    assert!(m.researching_at(RegionId(10), 0).is_none());
    // A finished technology cannot be researched again.
    assert_eq!(m.start_research(RegionId(10), 0, "admin1_a"), Err(ResearchError::NotAvailable));
}

/// A building level's technology list (`0x008C7620`): the faction tree's technologies of that exact
/// level whose faction list passes (`0x00F1EFA0`), in tree order; the building info drops it while the
/// script's technology switch is set (`0x009B73AF`), the effects table also for no owner, the caller's
/// snapshot or an `episodic` campaign (`0x009B2BB0`).
#[test]
fn building_level_technologies_follow_the_tree_the_faction_list_and_the_switches() {
    use super::rules::TechRules;
    let mut m = test_model();
    let key_a = m.world.factions[&A].key.clone();
    let mut rules = (*m.rules).clone();
    let tech = |level: &str, factions: Vec<String>| TechRules { cost: 10, building_level: level.into(), requires: vec![], factions, thread: 2, ..Default::default() };
    rules.technologies.insert("t_mine".into(), tech("test_building_level", vec![key_a.clone(), "other".into()]));
    rules.technologies.insert("t_all".into(), tech("test_building_level", vec![]));
    rules.technologies.insert("t_theirs".into(), tech("test_building_level", vec!["other".into()]));
    rules.technologies.insert("t_upgrade".into(), tech("test_building_level_2", vec![]));
    m.rules = Arc::new(rules);
    // The tree order wins over the table's (its key order); any state counts.
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("t_upgrade".into(), 0), ("t_mine".into(), 4), ("t_theirs".into(), 0), ("t_all".into(), 0), ("t_unknown".into(), 0)];
    let level = "test_building_level";
    assert_eq!(m.building_level_technologies(A, level), ["t_mine", "t_all"]);
    assert_eq!(m.building_level_technologies(A, "test_building_level_2"), ["t_upgrade"], "exact level only");
    assert!(m.building_level_technologies(B, level).is_empty(), "no tree, no list");
    assert_eq!(m.building_info_technologies(A, level), ["t_mine", "t_all"]);
    assert_eq!(m.effects_table_technologies(Some(A), level, false), ["t_mine", "t_all"]);
    assert!(m.effects_table_technologies(None, level, false).is_empty());
    assert!(m.effects_table_technologies(Some(A), level, true).is_empty(), "a message's snapshot of the switch");
    // The script switch empties the building info, not the ungated list.
    m.world.technology_research_disabled = true;
    assert!(m.building_info_technologies(A, level).is_empty());
    assert_eq!(m.building_level_technologies(A, level), ["t_mine", "t_all"]);
    m.world.technology_research_disabled = false;
    let mut rules = (*m.rules).clone();
    rules.features.effects_technologies_hidden = true;
    m.rules = Arc::new(rules);
    assert!(m.effects_table_technologies(Some(A), level, false).is_empty());
    assert_eq!(m.building_info_technologies(A, level), ["t_mine", "t_all"], "the episodic gate is the effects table's only");
}

//! Campaign model tests: trade order.

use super::*;

#[test]
fn gdp_and_town_wealth_growth_are_recomputed() {
    // Made-up building effects; the formula of 0x00A6AFC0.
    let mut m = test_model_with(tax_growth_mapping().into_iter().chain([map_basic("gdp_mod_all", "gdp_mod_all")]));
    let mut rules = (*m.rules).clone();
    {
        let b = rules.buildings.get_mut("test_building_level").unwrap();
        b.effects.push(("gdp_mine".into(), 300.0));
        b.effects.push(("gdp_farm".into(), 50.0));
        b.effects.push(("tw_growth_industry".into(), 10.0));
        b.effects.push(("tw_growth_port".into(), 4.0));
    }
    // The upper classes' tax level slows growth: modifier -0.3, fixed -3.
    rules.effects.set_tax_bundle(
        "upper_classes",
        "tax_normal",
        mapped(&rules, vec![("tw_growth_taxes_modifier".into(), -0.3), ("tw_growth_taxes_fixed".into(), -3.0)]),
    );
    m.rules = Arc::new(rules);
    let r = m.world.regions[&RegionId(10)].clone();
    // GDP: base 4000 + 300 + 50. Growth: 14 -> trunc(-0.3 × 14) = -4 -> 14 - 4 - 3 = 7.
    assert_eq!(economy::recompute_region(&m, &r), (4350, 7));
    // A damaged building adds nothing; a negative growth is not scaled by the modifier.
    let mut d = r.clone();
    d.slots[0].building.as_mut().unwrap().health = 50;
    assert_eq!(economy::recompute_region(&m, &d), (4000, -3));
    // gdp_mod_all and tw_growth_mod_all scale each building effect (then truncate).
    let mut rules = (*m.rules).clone();
    rules.government_effects.insert(String::new(), vec![("gdp_mod_all".into(), 10.0), ("tw_growth_mod_all".into(), 50.0)]);
    m.rules = Arc::new(rules);
    // 300 × 1.1 = 330, 50 × 1.1 = 55; growth 10 × 1.5 = 15, 4 × 1.5 = 6 -> 21 - 6 - 3 = 12.
    assert_eq!(economy::recompute_region(&m, &r), (4385, 12));
    // A faction-wide effect (a researched technology with gdp_mod_all +20) counts at the round end
    // (recompute_region_with), not in the start-position form: 300 × 1.3 = 390, 50 × 1.3 = 65.
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("gdp_mod_all", 20.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("test_tech".into(), 0)];
    let fx = super::effects::Effects::compute(&m);
    assert_eq!(economy::recompute_region(&m, &r), (4385, 12));
    assert_eq!(economy::recompute_region_with(&m, Some(&fx), &r), (4455, 12));
}

#[test]
fn trade_routes_from_agreements() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_value_accumulator_proportion".into(), 0.005);
    m.rules = Arc::new(rules);
    assert_eq!(economy::trade_routes_value(&m, A), 0);
    // A (GDP 100 + 4000 + 2000) and B (100 + 8000) agree to trade: each earns
    // trunc(2.2 × sqrt(14200)) = 262 a turn.
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    assert_eq!(economy::faction_gdp(&m, A), 6100);
    assert_eq!(economy::trade_routes_value(&m, A), 262);
    assert_eq!(economy::trade_routes_value(&m, B), 262);
    assert_eq!(economy::faction_income(&m, A).trade, 262);
    // The round end accumulates trunc(0.005 × 262) = 1 per route.
    economy::accumulate_trade(&mut m, A);
    assert_eq!(economy::trade_routes_value(&m, A), 263);
    // War ends the route.
    m.world.set_stance(A, B, Stance::War).unwrap();
    assert_eq!(economy::trade_routes_value(&m, A), 0);
}

#[test]
fn trade_commodities_and_blockades() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_value_accumulator_proportion".into(), 0.005);
    m.rules = Arc::new(rules);
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    // A ships 2 of commodity 0 (price 10) and 1 of commodity 1 (price 20) to B by sea, from its
    // port at (20, 0): commodity part 40 on top of the GDP part 262.
    m.world.commodity_prices = vec![10, 20];
    let port = pos(20, 0);
    let path = TradePath {
        waypoints: vec![
            TradeWaypoint { region: RegionId(10), from: 1, to: 2, sea: true, from_pos: Some(port), to_pos: Some(pos(90, 0)) },
            TradeWaypoint { region: RegionId(11), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None },
        ],
        volumes: vec![2, 1],
        capital_port: false,
        accumulated: 0,
    };
    m.world.trade_paths.insert((A, B), vec![path.clone()]);
    assert_eq!(economy::trade_routes_value(&m, A), 302);
    // B has no path loaded: GDP part only.
    assert_eq!(economy::trade_routes_value(&m, B), 262);
    // Accumulation counts the commodity part: trunc(0.005 × 302) = 1.
    economy::accumulate_trade(&mut m, A);
    assert_eq!(m.world.trade_paths[&(A, B)][0].accumulated, 1);
    // A navy of C (at war with A) at the port blockades the route: nothing, not even the
    // accumulated value, and nothing accumulates.
    let mut admiral = character(104, C, CharacterKind::Admiral, 60);
    admiral.position = pos(20, 0);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(
        ForceId(1003),
        MilitaryForce { id: ForceId(1003), faction: C, commander: Some(CharacterId(104)), units: vec![unit(5)], is_navy: true },
    );
    assert_eq!(economy::trade_routes_value(&m, A), 303, "not at war: no blockade");
    m.world.set_stance(A, C, Stance::War).unwrap();
    assert!(m.trade_path_blockaded(A, &path));
    assert_eq!(economy::trade_routes_value(&m, A), 0);
    economy::accumulate_trade(&mut m, A);
    assert_eq!(m.world.trade_paths[&(A, B)][0].accumulated, 1);
    // Far from the port it no longer blocks.
    m.world.characters.get_mut(&CharacterId(104)).unwrap().position = pos(50, 30);
    assert_eq!(economy::trade_routes_value(&m, A), 303);
    // With a domestic route the supply is split (`0x00BC26D0`): B has no demand in this world, so it takes no
    // share and the route carries nothing (the loaded volumes are replaced); a blockaded domestic route
    // cuts the supply to 0 as well.
    let home = TradePath {
        waypoints: vec![TradeWaypoint { region: RegionId(0), from: 120, to: 1, sea: true, from_pos: Some(pos(60, 30)), to_pos: Some(port) }],
        volumes: vec![2, 1],
        capital_port: false,
        accumulated: 0,
    };
    m.world.domestic_trade.insert(A, vec![home]);
    assert_eq!(economy::trade_routes_value(&m, A), 263);
    m.world.characters.get_mut(&CharacterId(104)).unwrap().position = pos(60, 30);
    assert_eq!(m.trade_supply(A), Some(vec![0, 0]));
    assert_eq!(economy::trade_routes_value(&m, A), 263);
}

#[test]
fn commanders_get_the_force_action_point_factor() {
    let mut rules = CampaignRules::test_rules();
    assert_eq!(turn::force_action_point_factor(&rules, 0.0), 1.0);
    rules.variables.insert("general_admiral_action_point_bonus".into(), 0.2);
    assert_eq!(turn::force_action_point_factor(&rules, 0.0), 1.2);
    rules.variables.insert("general_admiral_action_point_bonus".into(), 0.9);
    assert_eq!(turn::force_action_point_factor(&rules, 0.0), 1.6);
    rules.variables.insert("general_admiral_action_point_bonus".into(), 0.2);
    let mut m = test_model();
    m.rules = Arc::new(rules);
    m.turn.humans = vec![A];
    m.start_campaign();
    // Character 100 commands force 1000: 30 × 1.2 = 36. Character 102 (a minister) does not.
    assert_eq!(m.world.characters[&CharacterId(100)].movement_points, 36);
    assert_eq!(m.world.characters[&CharacterId(102)].movement_points, 0);
}

#[test]
fn new_trade_agreements_get_a_route_over_the_network() {
    use super::trade::TradeLeg;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    // The test building gives a sea route cap (`trade_routes_mod_max_sea`, 0x008DC150).
    m.world.commodity_prices = vec![10];
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_land_sea_bias".into(), 0.5);
    m.rules = Arc::new(rules);
    edit_test_building_effects(&mut m, |e| e.push(("trade_routes_mod_max_sea".into(), 3.0)));
    m.world.commodity_prices = vec![10];
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    // Nodes: 1 = settlement of region 10 (A), 2 = A's port in region 10, 3 = B's port in region 11,
    // 4 = settlement of region 11 (B); 5 = a settlement of C in between.
    m.world.trade_node_regions =
        [(1, (RegionId(10), false)), (2, (RegionId(10), true)), (3, (RegionId(11), true)), (4, (RegionId(11), false))].into_iter().collect();
    // Land 1-4 costs 100; land 1-2 (5) + sea 2-3 (100 × 0.5) + land 3-4 (5) costs 60.
    m.world.trade_network = vec![
        TradeLeg { from: 1, to: 4, length: 100.0, splines: Vec::new() },
        TradeLeg { from: 1, to: 2, length: 5.0, splines: Vec::new() },
        TradeLeg { from: 2, to: 3, length: 100.0, splines: Vec::new() },
        TradeLeg { from: 3, to: 4, length: 5.0, splines: Vec::new() },
    ];
    let route = m.build_trade_route(A, B).expect("route");
    let hops: Vec<(u32, u32, bool)> = route.waypoints.iter().map(|w| (w.from, w.to, w.sea)).collect();
    // A's port is a source itself, so the route starts there and ends at B's port.
    assert_eq!(hops, vec![(2, 3, true), (u32::MAX, u32::MAX, false)]);
    assert_eq!(route.waypoints.last().unwrap().region, RegionId(11));
    assert_eq!(economy::trade_routes_value(&m, A), 262);
    // With a sea length limit below 100 only the land route is left.
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_internat_sea_length_limit".into(), 50.0);
    m.rules = Arc::new(rules);
    let hops: Vec<(u32, u32)> = m.build_trade_route(A, B).unwrap().waypoints.iter().map(|w| (w.from, w.to)).collect();
    assert_eq!(hops[0], (1, 4));
    // Without a sea cap no sea leg is allowed either.
    let mut rules = (*m.rules).clone();
    rules.variables.remove("trade_route_internat_sea_length_limit");
    m.rules = Arc::new(rules);
    edit_test_building_effects(&mut m, |e| e.retain(|(k, _)| k != "trade_routes_mod_max_sea"));
    let hops: Vec<(u32, u32)> = m.build_trade_route(A, B).unwrap().waypoints.iter().map(|w| (w.from, w.to)).collect();
    assert_eq!(hops[0], (1, 4));
    // No path at all: no route, no income from the pair.
    m.world.trade_network.clear();
    m.world.trade_network.push(TradeLeg { from: 7, to: 8, length: 1.0, splines: Vec::new() });
    assert!(m.build_trade_route(A, B).is_none());
    assert_eq!(economy::trade_routes_value(&m, A), 0);
}

#[test]
fn trade_fleets_gather_commodities_at_nodes() {
    use super::trade::{node_volume, TradeNode, TradeNodeInfo};
    let info = TradeNodeInfo { commodity: 0, base: 20, per_ship: 0.85, cap: 50.0 };
    // 0x00BC9930: trunc((min((n − 1) × 0.85, 50) + 1) × 20).
    assert_eq!([0, 1, 2, 3, 4].map(|n| node_volume(&info, n)), [0, 20, 37, 54, 71]);
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    let mut ship = rules.units["test_unit"].clone();
    ship.category = "naval_merchant".into();
    ship.is_naval = true;
    rules.units.insert("trade_ship".into(), ship);
    m.rules = Arc::new(rules);
    m.world.commodity_prices = vec![10];
    m.world.trade_nodes = vec![TradeNode { node: 120, pos: (40.0, 0.0), info: Some(info) }];
    let mut admiral = character(105, A, CharacterKind::Admiral, 60);
    admiral.position = pos(40, 0);
    m.world.characters.insert(admiral.id, admiral);
    let mut ships = vec![unit(6), unit(7)];
    for u in &mut ships {
        u.unit_key = "trade_ship".into();
    }
    m.world.forces.insert(ForceId(1004), MilitaryForce { id: ForceId(1004), faction: A, commander: Some(CharacterId(105)), units: ships, is_navy: true });
    assert_eq!(m.trade_supply(A), Some(vec![37]));
    // Sailing away ends the gathering.
    m.world.characters.get_mut(&CharacterId(105)).unwrap().position = pos(42, 0);
    assert_eq!(m.trade_supply(A), Some(vec![0]));
}

#[test]
fn religion_gentlemen_war_and_education_in_public_order() {
    let mut m = test_model_with([map_basic("gentleman_happiness_bonus", "gentleman_happiness_bonus")]);
    let base = economy::public_order(&m, RegionId(10));
    // Religion: a protestant faction ruling a region 70 % catholic, unrest factor 0.2 for (catholic,
    // protestant): −round(0.7 × 100 × 0.2) = −14.
    let mut rules = (*m.rules).clone();
    rules.religion_relations.insert(("rel_catholic".into(), "rel_protestant".into()), 0.2);
    rules.variables.insert("gentleman_happiness_divisor".into(), 3.0);
    rules.variables.insert("gentleman_happiness_positive_limit".into(), 6.0);
    rules.variables.insert("gentleman_happiness_negative_limit".into(), -8.0);
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().religion = "rel_protestant".into();
    m.world.regions.get_mut(&RegionId(10)).unwrap().religions = vec![("rel_catholic".into(), 0.7), ("rel_protestant".into(), 0.3)];
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower - 14.0);
    m.world.regions.get_mut(&RegionId(10)).unwrap().religions.clear();
    // Gentlemen (`0x008C55D0`): each resident in the settlement (region +0xFC → +0xC8) gives
    // ceil(rank / 3) plus his own set's gentleman_happiness_bonus
    // (character +0x48C, `0x008C569E`), own ones positive, foreign ones negative. Without a research
    // attribute the rank is −1: ceil(−1 / 3) = 0. A faction-level bonus does not count.
    let mut rules = (*m.rules).clone();
    for (key, v) in [("anc_gent_a", 1.0), ("anc_gent_c", 2.0)] {
        rules.effects.insert_ancillary(key.into(), mapped(&rules, vec![("gentleman_happiness_bonus", v)]));
    }
    rules.effects.insert_technology("tech_gent".into(), mapped(&rules, vec![("gentleman_happiness_bonus", 5.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("tech_gent".into(), 0)];
    m.world.character_details.entry(CharacterId(106)).or_default().ancillaries = vec!["anc_gent_a".into()];
    m.world.character_details.entry(CharacterId(107)).or_default().ancillaries = vec!["anc_gent_c".into()];
    let mut g = character(106, A, CharacterKind::Gentleman, 10);
    g.position = pos(10, 0);
    m.world.characters.insert(g.id, g);
    // Standing in the region outside the settlement, he does not count.
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower);
    m.world.characters.get_mut(&CharacterId(106)).unwrap().garrisoned_in = Some(RegionId(10));
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower + 1.0);
    let mut g = character(107, C, CharacterKind::Gentleman, 10);
    (g.position, g.garrisoned_in) = (pos(11, 0), Some(RegionId(10)));
    m.world.characters.insert(g.id, g);
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower + 1.0 - 2.0);
    // An ally's gentleman is on the owner's side (`0x008CE390`, called at `0x008C56CC`).
    m.world.set_stance(A, C, Stance::Allied).unwrap();
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower + 1.0 + 2.0);
    // Taken for C instead of the owner A (`0x008C55D0(region, C)`): allied, both are C's side; at peace,
    // C's own +2 and A's −1.
    assert_eq!(economy::gentlemen_factor_for(&m, &m.world.regions[&RegionId(10)], C), 3.0);
    m.world.set_stance(A, C, Stance::Neutral).unwrap();
    assert_eq!(economy::gentlemen_factor_for(&m, &m.world.regions[&RegionId(10)], C), 1.0);
    // A's gentleman at rank 4 (`0x00A198D0`, his research): ceil(4 / 3) + 1 = 3.
    m.world.character_details.entry(CharacterId(106)).or_default().attributes = vec![("research".into(), 4)];
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower + 3.0 - 2.0);
    assert_eq!(economy::gentlemen_factor_for(&m, &m.world.regions[&RegionId(10)], C), 2.0 - 3.0);
    for id in [CharacterId(106), CharacterId(107)] {
        m.world.characters.remove(&id);
        m.world.character_details.remove(&id);
    }
    m.world.faction_details.entry(A).or_default().technologies.clear();
    // War results: a stored base counts as it is; with base 0 an enemy army in the region adds 12.
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = vec![PopulationClassState::new("lower"), PopulationClassState { resistance: 3, ..PopulationClassState::new("upper") }];
    assert_eq!(economy::public_order(&m, RegionId(10)).upper, base.upper + 3.0);
    m.world.set_stance(A, C, Stance::War).unwrap();
    let mut col = character(108, C, CharacterKind::Colonel, 10);
    col.position = pos(12, 0);
    m.world.characters.insert(col.id, col);
    // A character without a force does not count.
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower);
    let army = MilitaryForce { id: ForceId(1008), faction: C, commander: Some(CharacterId(108)), units: vec![unit(8)], is_navy: true };
    m.world.forces.insert(army.id, army);
    // Nor does a fleet.
    assert_eq!(economy::public_order(&m, RegionId(10)).lower, base.lower);
    m.world.forces.get_mut(&ForceId(1008)).unwrap().is_navy = false;
    let po = economy::public_order(&m, RegionId(10));
    assert_eq!((po.lower, po.upper), (base.lower + 12.0, base.upper + 3.0));
}

#[test]
fn the_winner_goes_on_into_the_settlement_after_beating_the_army_outside() {
    let mut m = test_model();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    // B's army stands outside its settlement (region 11 at (11, 0)); A's strong army comes to take it.
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(12, 0);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(15, 0);
    let mut units = Vec::new();
    for i in 0..6 {
        let mut u = unit(20 + i);
        u.men = 100;
        units.push(u);
    }
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = units;
    for u in &mut m.world.forces.get_mut(&ForceId(1001)).unwrap().units {
        u.men = 20;
    }
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(11) }).unwrap();
    assert!(ev.contains(&CampaignEvent::PreBattle { attacker: ForceId(1000) }));
    let pb = m.pending_battle.clone().unwrap();
    assert_eq!((pb.defenders.clone(), pb.settlement, pb.resume), (vec![ForceId(1001)], None, Some(RegionId(11))));
    let ev = m.apply(CampaignCommand::Autoresolve).unwrap();
    assert!(matches!(ev[0], CampaignEvent::BattleCompleted { attacker_won: true, .. }));
    // The settlement had no garrison: the winner walks in and takes it.
    assert!(ev.contains(&CampaignEvent::SettlementOccupied { region: RegionId(11), faction: A }));
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
}

#[test]
fn faction_wide_effects_reach_public_order() {
    let mut m = test_model_with([
        ("happy_character_lower".to_string(), vec![EffectKey { kind: BonusKind::PopClass, bonus: "happiness_character".into(), qualifier: "lower".into() }]),
        map_basic("repression_ministers", "repression_ministers"),
    ]);
    let base = economy::public_order(&m, RegionId(10));
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("happy_character_lower", 3.0), ("repression_ministers", 2.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("test_tech".into(), 0)];
    let po = economy::public_order(&m, RegionId(10));
    assert_eq!((po.lower, po.upper), (base.lower + 5.0, base.upper + 2.0));
}

#[test]
fn sea_route_cap_walks_owned_land_neighbours_from_the_capital() {
    let mut m = test_model();
    edit_test_building_effects(&mut m, |e| e.push(("trade_routes_mod_max_sea".into(), 3.0)));
    // A owns regions 10 (capital) and 12, each with one port-like building (cap 3); B owns 11.
    let mut slot = m.world.regions[&RegionId(10)].slots[0].clone();
    slot.key = "test_slot_12".into();
    m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(slot);
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    // Without the map's neighbours every owned region but the capital counts.
    assert_eq!(m.sea_route_cap(A), 3);
    // 12 is reached only through B's region 11: the walk stops there. The capital itself never counts
    // (`0x008DC150`): its ports give the capital port routes.
    m.world.region_neighbours =
        [(RegionId(10), vec![RegionId(11)]), (RegionId(11), vec![RegionId(10), RegionId(12)]), (RegionId(12), vec![RegionId(11)])].into_iter().collect();
    assert_eq!(m.sea_route_cap(A), 0);
    // A land link from the capital to 12.
    m.world.region_neighbours.get_mut(&RegionId(10)).unwrap().push(RegionId(12));
    m.world.region_neighbours.get_mut(&RegionId(12)).unwrap().push(RegionId(10));
    assert_eq!(m.sea_route_cap(A), 3);
    // The capital port cap (`0x008DC360`): the capital's own cap, only with an export port.
    assert_eq!(m.capital_port_route_cap(A), 0);
    edit_test_building_effects(&mut m, |e| e.push(("export_capacity_port".into(), 30.0)));
    assert_eq!(m.capital_port_route_cap(A), 3);
    // No capital: no cap.
    m.world.faction_details.entry(A).or_default().capital = None;
    assert_eq!((m.sea_route_cap(A), m.capital_port_route_cap(A)), (0, 0));
}

#[test]
fn compute_for_gives_the_same_answers_for_its_faction() {
    use super::effects::Effects;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("recruitment_points", 2.0)]));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("test_tech".into(), 0)];
    let full = Effects::compute(&m);
    for f in [A, B] {
        let part = Effects::compute_for(&m, f);
        assert_eq!(part.faction.get(&f), full.faction.get(&f));
        for r in m.world.regions.values().filter(|r| r.owner == f) {
            assert_eq!(part.region_local.get(&r.id), full.region_local.get(&r.id));
            for key in ["recruitment_points", "happiness_active_lower_tax"] {
                assert_eq!(part.region(r.id, key), full.region(r.id, key));
            }
            assert_eq!(m.recruitment_points_with(&part, r.id, false), m.recruitment_points_with(&full, r.id, false));
        }
        for c in m.world.characters.values().filter(|c| c.faction == f) {
            assert_eq!(part.character_total(c.id, "recruitment_points"), full.character_total(c.id, "recruitment_points"));
        }
        assert_eq!(economy::faction_income_with(&m, &part, f), economy::faction_income_with(&m, &full, f));
    }
}

/// `0x00BC26D0` (CONFIRMED): per commodity the partners, sorted by net demand, take
/// `max(1, demand × rest / total)` from the smallest demand up while there are at least as many units
/// left as partners.
#[test]
fn supply_is_split_over_the_partners_by_their_net_demand() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = split_model(&["c0"], vec![33]);
    // The smaller demand first: C trunc(33 × 2000 / 10000) = 6, then B the rest, 27.
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C].clone(), split[&B].clone()), (vec![6], vec![27]));
    // A partner's own supply comes off its demand when it has a capital: B brings home 6000 itself,
    // so both partners have 2000. Equal demands keep the faction list order (B before C: turn order
    // B, A, C) and are handed out from the end: C 33 × 2000 / 4000 = 16 first, then B the rest, 17.
    let w = TradeWaypoint { region: RegionId(11), from: 121, to: 2, sea: true, from_pos: None, to_pos: None };
    m.world.domestic_trade.insert(B, vec![TradePath { waypoints: vec![w], volumes: vec![6000], capital_port: false, accumulated: 0 }]);
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C].clone(), split[&B].clone()), (vec![16], vec![17]));
    // Without its capital B's own supply does not count.
    m.world.faction_details.entry(B).or_default().capital = None;
    assert_eq!(m.trade_split(A).unwrap()[&B], vec![27]);
    // A second route of the pair carries the same share (the exe adds it to every route of the pair).
    let path = m.world.trade_paths[&(A, B)][0].clone();
    m.world.trade_paths.get_mut(&(A, B)).unwrap().push(path.clone());
    let split = m.trade_split(A).unwrap();
    for p in &m.world.trade_paths[&(A, B)] {
        assert_eq!(m.trade_path_volumes(Some(&split), A, B, p), vec![27]);
    }
    // An exporter without a capital exports nothing.
    m.world.faction_details.entry(A).or_default().capital = None;
    assert_eq!(m.trade_split(A).unwrap().values().cloned().collect::<Vec<_>>(), vec![vec![0], vec![0]]);
}

#[test]
fn partners_without_net_demand_take_nothing_and_the_rest_may_stay_home() {
    // No demand at all: no partner takes a share, the supply is not exported.
    let mut m = split_model(&["c0"], vec![33]);
    let mut rules = (*m.rules).clone();
    rules.commodity_demand.clear();
    m.rules = Arc::new(rules);
    assert_eq!(m.trade_split(A).unwrap().values().cloned().collect::<Vec<_>>(), vec![vec![0], vec![0]]);

    // C has no demand for c0 (GDP 0) but some for c1 (town wealth 100 → 10), so it counts with demand
    // 0 for c0: the total is 8000 + max(0, 1) = 8001. Of 5 units C gets max(1, 0) = 1, then B
    // trunc(8000 × 4 / 8001) = 3; the last unit is not exported.
    let mut m = split_model(&["c0", "c1"], vec![5, 0]);
    let r = m.world.regions.get_mut(&RegionId(12)).unwrap();
    (r.gdp, r.town_wealth) = (0, 100);
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C][0], split[&B][0]), (1, 3));
    // One unit for two partners: C (first, two partners left) gets nothing, B takes max(1, 0) = 1.
    m.world.domestic_trade.get_mut(&A).unwrap()[0].volumes = vec![1, 0];
    let split = m.trade_split(A).unwrap();
    assert_eq!((split[&C][0], split[&B][0]), (0, 1));
}

#[test]
fn construction_cost_takes_the_local_and_chain_cost_effects() {
    use super::effects::SavedBonus;
    let mut m = test_model_with([map_chain_cost("building_cost_mod_all", "test_chain")]);
    // Region 10 holds a test building (level 0) at full health; the upgrade costs 600.
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 600);
    // A timber-like local effect, mapped to the chain through the chain junction: −10 %.
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_all", -10.0)]));
    m.rules = Arc::new(rules);
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 540);
    // The owner's chain-keyed entry (saved type 2, id 0) adds up: −10 − 50 = −60 %.
    m.world.faction_details.entry(A).or_default().bonus_base =
        vec![SavedBonus { kind: 2, bonus: 0, value: -50.0, qualifier: "test_chain".into(), ..Default::default() }];
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 240);
    // A slot held by another faction drops the local effect.
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].holder = Some(B);
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 300);
    // That slot itself offers nothing to build (`0x00B4333B`).
    assert!(matches!(m.can_build(RegionId(10), SlotRef::Slot(0), "test_building_level_2"), Err(CommandError::CannotBuild(_))));
}

#[test]
fn construction_cost_counts_only_the_chains_an_effect_maps_to() {
    // `building_cost_mod_all` maps to 19 vanilla chains, not to every chain (e.g. not `rHorse`): a local effect
    // mapped to another chain leaves this one at full cost.
    let mut m = test_model_with([map_chain_cost("building_cost_mod_other_chain", "other_chain")]);
    let mut rules = (*m.rules).clone();
    rules.effects.insert_building_local("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_other_chain", -10.0)]));
    m.rules = Arc::new(rules);
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 600);
}

#[test]
fn construction_cost_takes_the_faction_wide_and_technology_entries() {
    use super::effects::TECH_RESEARCHED;
    let mut m = test_model_with([map_chain_cost("building_cost_mod_all_global", "test_chain"), map_chain_cost("building_cost_mod_industry", "test_chain")]);
    let mut rules = (*m.rules).clone();
    // A faction-wide building effect (the steam sawmill's −2 %) reaches every region of the owner.
    rules.effects.insert_building_factionwide("test_building_level".into(), mapped(&rules, vec![("building_cost_mod_all_global", -2.0)]));
    // A researched technology's chain cost effect (`economy2_joint_stock_company`: −10 % on factories).
    rules.effects.insert_technology("test_tech".into(), mapped(&rules, vec![("building_cost_mod_industry", -10.0)]));
    m.rules = Arc::new(rules);
    // 600 × 98 × 0.01 = 588.
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 588);
    m.world.faction_details.entry(A).or_default().technologies.push(("test_tech".into(), TECH_RESEARCHED));
    // 600 × 88 × 0.01 = 528.
    assert_eq!(m.construction_cost(RegionId(10), "test_building_level_2"), 528);
    // The construction options and the repair read the same modifier.
    assert_eq!(m.building_cost_modifier(RegionId(10), "test_building_level_2"), -12.0);
    let opts = m.construction_options(RegionId(10), SlotRef::Slot(0));
    assert_eq!(opts.iter().find(|o| o.level_key == "test_building_level_2").map(|o| o.cost), Some(528));
}

#[test]
fn construction_cost_rounds_half_to_even_in_f32() {
    // `cost × (modifier + 100) × 0.01` in f32, then FISTP: 5 × 50 × 0.01 = 2.5 → 2, 3 × 50 × 0.01 = 1.5 → 2.
    assert_eq!(super::commands::building_cost(5, -50.0), 2);
    assert_eq!(super::commands::building_cost(3, -50.0), 2);
    assert_eq!(super::commands::building_cost(8000, -58.0), 3360);
    // No clamp: below −100 the cost is negative.
    assert_eq!(super::commands::building_cost(1000, -110.0), -100);
    // Out of the int range FISTP stores the integer indefinite 0x80000000.
    assert_eq!(super::commands::building_cost(2_000_000_000, 100.0), i32::MIN);
    assert_eq!(super::commands::building_cost(1000, f32::NAN), i32::MIN);
}

/// leftroutes: a broken agreement removes only the first route each way (`0x00BC0120`); the pair's second
/// route stays and still earns (`0x00BB3490` sums every route of the exporter, no agreement check): its GDP
/// part and accumulated value, with no commodity part (the split clears every route and shares only to
/// partners, `0x00B1C1A0` / `0x00BC26D0`). The `spa_france` home term subtracts this turn's volumes over the
/// same routes (it needs the faction's domestic map entry, `0x00BB35C4`), so the leftover route takes off nothing.
#[test]
fn a_route_left_after_a_broken_agreement_keeps_earning() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_value_accumulator_proportion".into(), 0.005);
    rules.features.home_trade_faction = Some(m.world.factions[&A].key.clone());
    m.rules = Arc::new(rules);
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    // Two routes A -> B, each with 2 of commodity 0 (price 10) and 1 of commodity 1 (price 20): commodity
    // part 40 each on top of the GDP part 262. No domestic route: the supply is unknown and the loaded
    // volumes stand.
    m.world.commodity_prices = vec![10, 20];
    let path = |from: u32| TradePath {
        waypoints: vec![
            TradeWaypoint { region: RegionId(10), from, to: 2, sea: true, from_pos: Some(pos(20, 0)), to_pos: Some(pos(90, 0)) },
            TradeWaypoint { region: RegionId(11), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None },
        ],
        volumes: vec![2, 1],
        capital_port: false,
        accumulated: 0,
    };
    m.world.trade_paths.insert((A, B), vec![path(1), path(3)]);
    // Home term: with a domestic map entry, no domestic income minus 2 × 40 exported; without one, none
    // (`0x00BB35C4`): A has no entry here.
    let with_entry = |m: &CampaignModel| {
        let mut m = m.clone();
        m.world.domestic_trade.insert(A, Vec::new());
        m.trade_home_value(A, None, &economy::trade_route_importer_paths(&m, A))
    };
    assert_eq!(with_entry(&m), -80);
    assert_eq!(m.trade_home_value(A, None, &economy::trade_route_importer_paths(&m, A)), 0);
    assert_eq!(economy::trade_routes_value(&m, A), 2 * 302);
    assert_eq!(economy::trade_routes_value(&m, B), 262, "B has no route loaded: GDP part only");

    m.break_trade(A, B).unwrap();
    assert_eq!(m.world.trade_paths[&(A, B)], vec![path(3)]);
    assert!(economy::trade_partners(&m, A).is_empty());
    assert_eq!(economy::trade_route_importers(&m, A), vec![B]);
    assert_eq!(m.trade_path_volumes(None, A, B, &path(3)), vec![0, 0]);
    // The leftover route: GDP part 262, no commodity part, and the home term subtracts nothing.
    assert_eq!(with_entry(&m), 0);
    assert_eq!(economy::trade_routes_value(&m, A), 262);
    assert_eq!(economy::faction_income(&m, A).trade, 262);
    // B's only route was its built one: nothing left.
    assert_eq!(economy::trade_route_importers(&m, B), Vec::<FactionId>::new());
    assert_eq!(economy::trade_routes_value(&m, B), 0);
    // Turn after turn the route accumulates trunc(0.005 × 262) = 1 and earns it.
    for turn in 1..=3 {
        economy::accumulate_trade(&mut m, A);
        assert_eq!(m.world.trade_paths[&(A, B)][0].accumulated, turn);
        assert_eq!(economy::trade_routes_value(&m, A), 262 + turn, "turn {turn}");
    }
    // A second break removes it: nothing earns any more.
    m.break_trade(A, B).unwrap();
    assert!(!m.world.trade_paths.contains_key(&(A, B)));
    assert_eq!(economy::trade_routes_value(&m, A), 0);
}

/// polish41: the `spa_france` home term of `0x00BB3490` runs only when the domestic map find (`0x00BB0A10`)
/// returns the faction's entry; at its end `0x00BB35C4` / `0x00BB35CC` jump to the plain route sum, so
/// without an entry nothing is subtracted either. With an entry it adds the domestic routes ending in the
/// capital and subtracts every international route's volumes, also when the entry holds no route.
#[test]
fn the_home_term_needs_a_domestic_map_entry() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.features.home_trade_faction = Some(m.world.factions[&A].key.clone());
    m.rules = Arc::new(rules);
    m.world.relationships.insert((A, B), details::Relationship { trade_agreement: true, ..Default::default() });
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    m.world.commodity_prices = vec![10, 20];
    let path = |end: u32, volumes: Vec<u32>| TradePath {
        waypoints: vec![
            TradeWaypoint { region: RegionId(10), from: 1, to: 2, sea: true, from_pos: Some(pos(20, 0)), to_pos: Some(pos(90, 0)) },
            TradeWaypoint { region: RegionId(end), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None },
        ],
        volumes,
        capital_port: false,
        accumulated: 0,
    };
    // One international route A -> B worth 2 × 10 + 1 × 20 = 40 at its loaded volumes (split `None`).
    m.world.trade_paths.insert((A, B), vec![path(11, vec![2, 1])]);
    let home = |m: &CampaignModel| m.trade_home_value(A, None, &economy::trade_route_importer_paths(m, A));
    assert_eq!(home(&m), 0, "no domestic map entry: no home term");
    m.world.domestic_trade.insert(A, Vec::new());
    assert_eq!(home(&m), -40, "an entry with no route still subtracts the exports");
    // A route home to the capital (3 × 10 + 1 × 20 = 50) counts; one ending elsewhere does not.
    m.world.domestic_trade.insert(A, vec![path(10, vec![3, 1]), path(11, vec![5, 5])]);
    assert_eq!(home(&m), 50 - 40);
    // Only the configured faction has the term.
    m.world.domestic_trade.insert(B, vec![path(11, vec![3, 1])]);
    assert_eq!(m.trade_home_value(B, None, &economy::trade_route_importer_paths(&m, B)), 0);
}

/// routeacc: each route accumulates its own value (+0x3C, `AccumulateTradeRouteValue` 0x00B05CC0) and earns
/// it (`0x00B15B50` adds it to the route's total); the erase of a broken agreement (`0x00BC0120` →
/// `0x00BAC2A0`) drops only the removed route's value. A pair with no stored route keeps its built route's
/// value, dropped with that route.
#[test]
fn each_trade_route_keeps_its_own_accumulated_value() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    rules.variables.insert("trade_route_value_accumulator_proportion".into(), 0.5);
    m.rules = Arc::new(rules);
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    // Two routes A -> B: commodity parts 40 and 100 on top of the GDP part 262 (no domestic route: the
    // loaded volumes stand).
    m.world.commodity_prices = vec![10, 20];
    let path = |from: u32, volumes: Vec<u32>| TradePath {
        waypoints: vec![
            TradeWaypoint { region: RegionId(10), from, to: 2, sea: true, from_pos: Some(pos(20, 0)), to_pos: Some(pos(90, 0)) },
            TradeWaypoint { region: RegionId(11), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None },
        ],
        volumes,
        capital_port: false,
        accumulated: 0,
    };
    m.world.trade_paths.insert((A, B), vec![path(1, vec![2, 1]), path(3, vec![10, 0])]);
    assert_eq!(economy::trade_pair_value(&m, A, B), 302 + 362);
    economy::accumulate_trade(&mut m, A);
    // trunc(0.5 × 302) = 151 and trunc(0.5 × 362) = 181, each on its own route, each earned once.
    assert_eq!(m.world.trade_paths[&(A, B)].iter().map(|p| p.accumulated).collect::<Vec<_>>(), [151, 181]);
    assert_eq!(economy::trade_pair_value(&m, A, B), 302 + 151 + 362 + 181);
    assert!(m.world.trade_accumulated.is_empty(), "stored routes keep their values on the routes");
    // B has no stored route (no network here: the pair stands for its built route): trunc(0.5 × 262) = 131.
    economy::accumulate_trade(&mut m, B);
    assert_eq!(m.world.trade_accumulated[&(B, A)], 131);
    assert_eq!(economy::trade_pair_value(&m, B, A), 262 + 131);
    // The break removes A's first route with its 151; the second keeps its 181 (and loses its volumes, A and B
    // no longer being partners). B's built route goes with its value.
    m.break_trade(A, B).unwrap();
    assert_eq!(m.world.trade_paths[&(A, B)].iter().map(|p| p.accumulated).collect::<Vec<_>>(), [181]);
    assert_eq!(economy::trade_pair_value(&m, A, B), 262 + 181);
    assert!(!m.world.trade_accumulated.contains_key(&(B, A)));
}

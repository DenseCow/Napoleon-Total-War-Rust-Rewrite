//! Campaign model tests: commerce raids (`super::commerce_raid`), on a MADE-UP trade network.

use super::*;
use crate::campaign::commerce_raid::{curve_point, segment_distance, CommerceRaid, OwnedRoute, RouteKind, RAID_RADIUS};
use crate::campaign::trade::{TradeLeg, TradePath, TradeWaypoint};
use crate::campaign::visibility::TradeSegmentSight;

/// A straight curve from `a` to `b` (control points at the thirds, so `t` runs linearly).
fn line(a: (f32, f32), b: (f32, f32)) -> [(f32, f32); 4] {
    let at = |t: f32| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    [a, at(1.0 / 3.0), at(2.0 / 3.0), b]
}

fn sight(curves: Vec<[(f32, f32); 4]>) -> TradeSegmentSight {
    TradeSegmentSight { cells: Vec::new(), domestic: Vec::new(), international: Vec::new(), curves }
}

fn waypoint(region: u32, from: u32, to: u32) -> TradeWaypoint {
    TradeWaypoint { region: RegionId(region), from, to, sea: false, from_pos: None, to_pos: None }
}

/// The test model with a network of one leg (nodes 1–2) running along segments 0 (x 50..60 at z 50)
/// and 1 (far away); B owns a domestic route over it carrying 30 of commodity 0 (price 10) and 20 of
/// commodity 1 (price 20): worth 700. A and B are at war; A's general 100 (force 1000, one unit of
/// travel speed 30: share 30 × 0.05 / 30 = 0.05) stands at the origin.
fn raid_model() -> CampaignModel {
    let mut m = test_model();
    m.world.commodity_prices = vec![10, 20];
    m.world.trade_network = vec![TradeLeg { from: 2, to: 1, length: 10.0, splines: vec![0, 3] }];
    m.world.trade_sight = vec![sight(vec![line((50.0, 50.0), (60.0, 50.0))]), sight(vec![line((-90.0, -90.0), (-80.0, -90.0))])];
    let path = TradePath { waypoints: vec![waypoint(11, 1, 2), waypoint(11, u32::MAX, u32::MAX)], volumes: vec![30, 20], capital_port: false, accumulated: 0 };
    m.world.domestic_trade.insert(B, vec![path]);
    m.world.set_stance(A, B, Stance::War).unwrap();
    m
}

fn at(x: f32, z: f32) -> (Fixed20, Fixed20) {
    (Fixed20::from_f64(f64::from(x)), Fixed20::from_f64(f64::from(z)))
}

#[test]
fn segment_distance_splits_each_curve_into_32_chords_overshooting_the_end() {
    // 0x00BA8E90: chord i runs from t = i/31 to (i+1)/31, so the last ends at t = 32/31: a straight
    // curve of length 31 reaches x = 32.
    let c = [line((0.0, 0.0), (31.0, 0.0))];
    assert!(segment_distance(&c, (31.9, 0.0)) < 1e-4, "inside the overshoot");
    assert!((segment_distance(&c, (32.4, 0.0)) - 0.4).abs() < 1e-4);
    assert!((segment_distance(&c, (5.0, 0.4)) - 0.4).abs() < 1e-4);
    assert!((segment_distance(&c, (-0.3, 0.0)) - 0.3).abs() < 1e-4, "no overshoot at the start");
    assert_eq!(segment_distance(&[], (0.0, 0.0)), f32::MAX);
    // 0x007CD7E0: step 0 weights the first control point by exactly 1, the others by 0.
    let s = [(1.5, -2.25), (7.0, 3.0), (9.0, 1.0), (31.0, 0.0)];
    assert_eq!(curve_point(&s, 0), (1.5, -2.25));
    let end = curve_point(&s, 31);
    assert!((end.0 - 31.0).abs() < 1e-4 && end.1.abs() < 1e-4, "{end:?}");
}

#[test]
fn raid_target_and_raidable() {
    let mut m = test_model();
    m.world.set_stance(A, B, Stance::War).unwrap();
    m.world.set_stance(A, C, Stance::War).unwrap();
    // 0x00BA7DF0: another faction's route, at war with the raider.
    assert!(m.segment_raidable(&[(B, 0)], A));
    assert!(!m.segment_raidable(&[(A, 500)], A), "the raider's own route");
    assert!(!m.segment_raidable(&[(C, 100)], B), "not at war");
    // 0x00BAFB20: the largest summed value, the raider's own and peaceful owners left out.
    assert_eq!(m.raid_target(&[(B, 100), (C, 60), (C, 60), (A, 1000)], A), Some(C));
    // A tie goes to the lower faction id, whatever the list order.
    assert_eq!(m.raid_target(&[(C, 100), (B, 100)], A), Some(B));
    assert_eq!(m.raid_target(&[(C, 100)], B), None);
}

#[test]
fn a_raid_takes_category_9_income_from_the_target() {
    let mut m = raid_model();
    let g = CharacterId(100);
    // Nothing near the origin: no raid (the original cancels the order).
    assert!(!m.raid_trade_route(g, at(0.0, 0.0)));
    assert!(m.world.commerce_raids.is_empty());
    // Next to segment 0 (within 0.5): one raid on B's route there.
    assert!(m.segment_routes(&[0].into()).get(&0).is_some_and(|l| l == &[(B, 700)]));
    assert!(m.raid_trade_route(g, at(55.0, 50.0 + RAID_RADIUS * 0.8)));
    assert_eq!(m.world.commerce_raids, vec![CommerceRaid { character: g, segment: 0, target: B }]);
    // The domestic take (0x00B609B0): Σ price × trunc(volume × 0.05) = 10 × 1 + 20 × 1.
    assert_eq!(m.raid_income(A), 30);
    assert_eq!(economy::faction_income(&m, A).raiding, 30);
    assert_eq!(m.raid_income(B), 0);
    // A second raid on the same route (the other segment of the leg) counts the character once.
    m.world.commerce_raids.push(CommerceRaid { character: g, segment: 1, target: B });
    assert_eq!(m.raid_income(A), 30);
}

#[test]
fn shares_above_one_are_normalised() {
    let mut m = raid_model();
    // 30 units of travel speed 30: share 1.5, divided by the sum to 1: the whole route.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = (0..30).map(unit).collect();
    assert!(m.raid_trade_route(CharacterId(100), at(52.0, 50.0)));
    assert_eq!(m.raid_income(A), 700);
    // With C raiding too (one unit, 0.05), the 1.55 is shared out: A 1.5 / 1.55, C 0.05 / 1.55.
    m.world.set_stance(B, C, Stance::War).unwrap();
    let mut c = character(104, C, CharacterKind::General, 30);
    c.position = at(58.0, 50.0);
    m.world.characters.insert(c.id, c);
    m.world.forces.insert(ForceId(1004), MilitaryForce { id: ForceId(1004), faction: C, commander: Some(CharacterId(104)), units: vec![unit(40)], is_navy: false });
    assert!(m.raid_trade_route(CharacterId(104), at(58.0, 50.0)));
    let (sa, sc) = (1.5f32 / 1.55, 0.05f32 / 1.55);
    let take = |s: f32| ((30.0 * s) as i32) * 10 + ((20.0 * s) as i32) * 20;
    assert_eq!(m.raid_income(A), take(sa));
    assert_eq!(m.raid_income(C), take(sc));
}

#[test]
fn an_international_take_truncates_three_terms_and_nets_the_owner() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("trade_route_value_combined_gdp_proportion".into(), 2.2);
    m.rules = Arc::new(rules);
    for (x, y) in [(A, B), (B, A)] {
        m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
    }
    m.world.commodity_prices = vec![10, 20];
    m.world.trade_network = vec![TradeLeg { from: 1, to: 2, length: 10.0, splines: vec![0] }];
    m.world.trade_sight = vec![sight(vec![line((50.0, 50.0), (60.0, 50.0))])];
    let path = TradePath { waypoints: vec![waypoint(10, 1, 2), waypoint(11, u32::MAX, u32::MAX)], volumes: vec![2, 1], capital_port: false, accumulated: 7 };
    m.world.trade_paths.insert((A, B), vec![path.clone()]);
    // GDP part trunc(2.2 × sqrt(14200)) = 262; commodities 40; accumulated 7.
    assert_eq!(economy::trade_routes_value(&m, A), 309);
    // C (at war with A) raids it with one unit: share 0.05.
    m.world.set_stance(A, C, Stance::War).unwrap();
    let mut c = character(104, C, CharacterKind::General, 30);
    c.position = at(55.0, 50.0);
    m.world.characters.insert(c.id, c);
    m.world.forces.insert(ForceId(1004), MilitaryForce { id: ForceId(1004), faction: C, commander: Some(CharacterId(104)), units: vec![unit(40)], is_navy: false });
    assert!(m.raid_trade_route(CharacterId(104), at(55.0, 50.0)));
    // 0x00B60A40: trunc(20 × 0.05) + trunc(20 × 0.05) + trunc(262 × 0.05) + trunc(7 × 0.05) = 1 + 1 + 13 + 0.
    let route = OwnedRoute { owner: A, kind: RouteKind::International { importer: B }, path: std::borrow::Cow::Owned(path), volumes: vec![2, 1] };
    assert_eq!(super::commerce_raid::raid_take(&m, &route, 0.05), 15);
    assert_eq!(m.raid_income(C), 15);
    // 0x00B79B20: the owner earns the net value.
    assert_eq!(economy::trade_routes_value(&m, A), 309 - 15);
    // The take goes through the seam: a mod's rule replaces it.
    let maker: crate::seam::Maker<super::seams::RaidTake> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: &OwnedRoute, _: f32| 100));
    Arc::make_mut(&mut m.rules).seams.raid_take.apply("test.flat", &maker);
    assert_eq!(m.raid_income(C), 100);
    assert_eq!(economy::trade_routes_value(&m, A), 209);
}

#[test]
fn raids_end_on_peace_death_and_the_round_end_check() {
    let mut m = raid_model();
    let g = CharacterId(100);
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    // A move ends his raids (0x0095D940 → 0x009DA140), even along the same trade: no re-raid.
    m.move_character(g, at(56.0, 50.0));
    assert!(!m.is_raiding(g));
    // A character not raiding is left alone by a move.
    m.move_character(g, at(55.0, 50.0));
    assert!(!m.is_raiding(g));
    // Peace (0x00A25570) ends his raids on the other side.
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    m.make_peace_rules(A, B, false).unwrap();
    assert!(m.world.commerce_raids.is_empty());
    // The round-end check (0x008B3690): a segment no longer raidable ends all his raids.
    m.world.set_stance(A, B, Stance::War).unwrap();
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    m.validate_commerce_raids();
    assert!(m.is_raiding(g), "still at war");
    m.world.set_stance(A, B, Stance::Neutral).unwrap();
    m.validate_commerce_raids();
    assert!(!m.is_raiding(g));
    // Death (0x0099D62E): his raids end with him, but as the force's commander he hands them on first
    // (the force's observer 0x008DB3E0): the colonel who takes over raids where he stood.
    m.world.set_stance(A, B, Stance::War).unwrap();
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    let before = m.world.commerce_raids.len();
    m.character_dies(g);
    assert!(!m.is_raiding(g));
    let next = m.world.forces[&ForceId(1000)].commander.expect("a colonel takes over");
    assert_eq!(m.world.characters[&next].position, at(55.0, 50.0));
    assert_eq!(m.world.commerce_raids.len(), before);
    assert!(m.world.commerce_raids.iter().all(|r| r.character == next));
}

#[test]
fn a_change_of_commander_hands_the_raids_on() {
    let mut m = raid_model();
    let g = CharacterId(100);
    m.move_character(g, at(55.0, 50.0));
    // A second unit led by its own general (101): he takes command when 100 dies (0x008B8E50).
    let mut h = character(101, A, CharacterKind::General, 30);
    h.position = at(0.0, 0.0);
    m.world.characters.insert(h.id, h);
    let mut u = unit(41);
    u.character = Some(CharacterId(101));
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.insert(0, u);
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    m.character_dies(g);
    assert_eq!(m.world.forces[&ForceId(1000)].commander, Some(CharacterId(101)));
    // 0x008DB3E0: the new commander, placed where the old one stood, raids from there (0x009D3AC0).
    assert!(m.is_raiding(CharacterId(101)));
    assert!(m.world.commerce_raids.iter().all(|r| r.character == CharacterId(101) && r.segment == 0 && r.target == B));
    // A commander who was not raiding hands nothing on.
    m.clear_character_raids(CharacterId(101));
    m.character_dies(CharacterId(101));
    let next = m.world.forces[&ForceId(1000)].commander.expect("a colonel takes over");
    assert!(!m.is_raiding(next));
    assert!(m.world.commerce_raids.is_empty());
}

#[test]
fn the_round_end_books_raids_as_category_9() {
    let mut m = raid_model();
    assert!(m.raid_trade_route(CharacterId(100), at(55.0, 50.0)));
    let income = economy::faction_income(&m, A);
    assert_eq!(income.raiding, 30);
    assert_eq!(income.revenue(), income.taxes + income.trade + 30 + income.other);
    economy::settle_round(&mut m, A);
    assert_eq!(m.world.economy_history[&A].last().map(|r| r[9]), Some(30));
}

#[test]
fn the_raid_keeps_his_raids_and_the_order_start_ends_them() {
    let mut m = raid_model();
    let g = CharacterId(100);
    // In range of B's route but at peace with B: nothing raidable, yet a segment is in range, so the
    // order is not cancelled (0x009D3AC0 returns any-in-range).
    m.world.set_stance(A, B, Stance::Neutral).unwrap();
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    assert!(m.world.commerce_raids.is_empty());
    // 0x009D3AC0 itself ends no raid: raiding twice holds two entries.
    m.world.set_stance(A, B, Stance::War).unwrap();
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    assert_eq!(m.world.commerce_raids.len(), 2);
    // The order's start ends them first, also where he already stands: one raid.
    m.world.characters.get_mut(&g).unwrap().position = at(55.0, 50.0);
    m.apply(CampaignCommand::RaidTradeRoute { force: ForceId(1000), to: at(55.0, 50.0) }).unwrap();
    assert_eq!(m.world.commerce_raids, vec![CommerceRaid { character: g, segment: 0, target: B }]);
}

/// The round start's trade update `0x00BCB020(0)` runs the raid check `0x00BD1020` too (the flag test at
/// `0x00BCC534` rejoins at the call `0x00BCC5F8`): a raid no longer raidable ends there, before the round end.
#[test]
fn the_round_start_checks_the_raids() {
    let mut m = raid_model();
    let g = CharacterId(100);
    assert!(m.raid_trade_route(g, at(55.0, 50.0)));
    m.turn.humans = vec![A];
    m.turn.queue.push_back(TurnStep::RoundStart);
    m.step().expect("the round-start step");
    assert!(m.is_raiding(g), "still at war: the raids stay");
    m.world.set_stance(A, B, Stance::Neutral).unwrap();
    m.turn.queue.push_back(TurnStep::RoundStart);
    m.step().expect("the round-start step");
    assert!(!m.is_raiding(g), "no longer raidable: the round start ends his raids");
}

/// [`raid_model`] with the pools' rules (the hire tests' `pool_model`): A's army 1002 under colonel 107
/// and fleet 1003 under captain 108 (on its one ship), both standing on segment 0.
fn raid_pool_model() -> CampaignModel {
    let mut m = raid_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let rules = Arc::make_mut(&mut m.rules);
    rules.general_units.insert(String::new(), "test_unit".into());
    rules.agent_caps.insert("General".into(), (4, 0));
    rules.agent_caps.insert("admiral".into(), (3, 0));
    for (id, kind) in [(107, CharacterKind::Colonel), (108, CharacterKind::Captain)] {
        let mut c = character(id, A, kind, 25);
        c.position = at(55.0, 50.0);
        m.world.characters.insert(c.id, c);
    }
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![unit(7)], is_navy: false });
    let mut ship = unit(8);
    ship.character = Some(CharacterId(108));
    m.world.forces.insert(ForceId(1003), MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(108)), units: vec![ship], is_navy: true });
    m
}

/// A candidate drawn into A's admiral (`admiral`) or General pool, with the campaign started for human A.
fn candidate(m: &mut CampaignModel, admiral: bool) -> CharacterId {
    let elapsed = m.calendar.turns_elapsed;
    let d = m.world.faction_details.get_mut(&A).unwrap();
    d.general_pool = (Vec::new(), if admiral { elapsed + 100 } else { elapsed });
    d.admiral_pool = (Vec::new(), if admiral { elapsed } else { elapsed + 100 });
    let c = m.pool_tick(A)[0];
    m.turn.humans = vec![A];
    m.start_campaign();
    c
}

/// Every raid is `c`'s, on segment 0 against B.
fn only_raider(m: &CampaignModel, c: CharacterId) -> bool {
    !m.world.commerce_raids.is_empty() && m.world.commerce_raids.iter().all(|r| r.character == c && r.segment == 0 && r.target == B)
}

#[test]
fn a_general_hired_into_a_raiding_army_takes_its_raids() {
    let mut m = raid_pool_model();
    let c = candidate(&mut m, false);
    assert!(m.raid_trade_route(CharacterId(107), at(55.0, 50.0)));
    m.apply(CampaignCommand::HireGeneral { character: c, into: Some(ForceId(1002)) }).unwrap();
    // 0x00A164C0: the colonel's raids end (0x00A165D0) and the General raids where he stood (0x00A165E9).
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(c));
    assert!(only_raider(&m, c));
    assert!(!m.is_raiding(CharacterId(107)));
    // A commander who was not raiding hands nothing on.
    let mut m = raid_pool_model();
    let c = candidate(&mut m, false);
    m.apply(CampaignCommand::HireGeneral { character: c, into: Some(ForceId(1002)) }).unwrap();
    assert!(m.world.commerce_raids.is_empty());
}

#[test]
fn a_hired_admiral_takes_the_raids_of_the_captain_he_replaces() {
    let mut m = raid_pool_model();
    let c = candidate(&mut m, true);
    assert!(m.raid_trade_route(CharacterId(108), at(55.0, 50.0)));
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    // 0x00A16110: the raiding captain is the one replaced (flag at 0x00A161CB) and destroyed
    // (0x00A0C6F0); the admiral raids at the position noted on entry (0x00A163DF).
    assert!(!m.world.characters.contains_key(&CharacterId(108)));
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(c));
    assert!(only_raider(&m, c));
}

#[test]
fn a_hired_admiral_on_an_empty_ship_takes_the_raids_of_the_commander_he_outranks() {
    let mut m = raid_pool_model();
    // An empty ship first: the admiral boards it (0x008EDE20) and the captain stays aboard his own.
    let f = m.world.forces.get_mut(&ForceId(1003)).unwrap();
    let mut empty = unit(9);
    empty.character = None;
    f.units.insert(0, empty);
    let c = candidate(&mut m, true);
    assert!(m.raid_trade_route(CharacterId(108), at(55.0, 50.0)));
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    // The command passes to the admiral: the captain's raids end (0x00A163CC), the admiral's start.
    assert!(m.world.characters.contains_key(&CharacterId(108)));
    assert_eq!(m.world.forces[&ForceId(1003)].units[0].character, Some(c));
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(c));
    assert!(!m.is_raiding(CharacterId(108)));
    assert!(only_raider(&m, c));
}

/// Grants A both field promotion bonuses (`promote_general_in_field` 64, `promote_admiral_at_sea` 65).
fn allow_promotion(m: &mut CampaignModel) {
    use crate::campaign::effects::SavedBonus;
    let d = m.world.faction_details.get_mut(&A).unwrap();
    d.bonus_base = [64, 65].map(|bonus| SavedBonus { kind: 1, bonus, value: 1.0, qualifier: String::new(), ..Default::default() }).to_vec();
    d.bonus_with_difficulty = Vec::new();
}

#[test]
fn a_promoted_colonel_takes_the_raids_of_the_army_commander() {
    let mut m = raid_pool_model();
    // A second colonel (110) in the army, standing elsewhere.
    m.world.characters.insert(CharacterId(110), character(110, A, CharacterKind::Colonel, 25));
    let mut u = unit(10);
    u.character = Some(CharacterId(110));
    m.world.forces.get_mut(&ForceId(1002)).unwrap().units.push(u);
    m.turn.humans = vec![A];
    m.start_campaign();
    allow_promotion(&mut m);
    assert!(m.raid_trade_route(CharacterId(107), at(55.0, 50.0)));
    m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 1 }).unwrap();
    // 0x008E1C20: he stands where the commander stood (0x008E1E4F) and takes his raids (0x008E2170 / 0x008E2190).
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(110)));
    assert_eq!(m.world.characters[&CharacterId(110)].position, at(55.0, 50.0));
    assert!(!m.is_raiding(CharacterId(107)));
    assert!(only_raider(&m, CharacterId(110)));
}

#[test]
fn a_ship_promoted_at_sea_takes_the_raids_of_the_fleet_commander() {
    let mut m = raid_pool_model();
    // A second ship without a character: a new officer is made for it (0x00990EF0).
    let mut empty = unit(9);
    empty.character = None;
    m.world.forces.get_mut(&ForceId(1003)).unwrap().units.push(empty);
    m.turn.humans = vec![A];
    m.start_campaign();
    allow_promotion(&mut m);
    assert!(m.raid_trade_route(CharacterId(108), at(55.0, 50.0)));
    let ev = m.apply(CampaignCommand::PromoteUnit { force: ForceId(1003), unit: 1 }).unwrap();
    let CampaignEvent::CharacterPromoted { character: new } = ev[0] else { panic!("promoted") };
    // 0x008E2260: the captain's raids end (0x008E2650) and the new admiral raids at his position (0x008E2670).
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(new));
    assert!(!m.is_raiding(CharacterId(108)));
    assert!(only_raider(&m, new));
}

#[test]
fn a_hired_admiral_leaves_the_raids_of_an_admiral_already_in_command() {
    let mut m = raid_pool_model();
    // An admiral already aboard the first ship commands the fleet and raids; the captain (108) sails the second.
    let mut adm = character(120, A, CharacterKind::Admiral, 25);
    adm.position = at(55.0, 50.0);
    m.world.characters.insert(adm.id, adm);
    let f = m.world.forces.get_mut(&ForceId(1003)).unwrap();
    let mut flag = unit(9);
    flag.character = Some(CharacterId(120));
    f.units.insert(0, flag);
    f.commander = Some(CharacterId(120));
    let c = candidate(&mut m, true);
    assert!(m.raid_trade_route(CharacterId(120), at(55.0, 50.0)));
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    // The commander after the pick is the one before, so nothing is handed over (0x00A163CC is not reached).
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(CharacterId(120)));
    assert!(only_raider(&m, CharacterId(120)));
    assert!(!m.is_raiding(c));
}

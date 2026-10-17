//! Campaign model tests on a small MADE-UP world (no game data).

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use super::effects::{BonusKind, EffectKey, Effects, SavedBonus, TECH_RESEARCHED};
use crate::calendar::{Calendar, Date, HALF_EARLY, HALF_LATE};
use crate::fixed::Fixed20;
use crate::rng::CaRng;

mod agents_tests;
mod buildings_tests;
mod capture_diplomacy_tests;
mod commerce_raid_tests;
mod construction_tests;
mod deals_tests;
mod economy_tests;
mod fog_turn_end_tests;
mod forces_tests;
mod hiring_tests;
mod recruitment_tests;
mod slots_exit_tests;
mod trade_order_tests;
mod transfer;
mod turn_order_tests;

const A: FactionId = FactionId(1);
const B: FactionId = FactionId(2);
const C: FactionId = FactionId(3);

/// `rows`, after asserting that the test mapping ([`CampaignRules::test_rules_with`]) maps every effect
/// key in them: an unmapped key compiles to nothing (as an unmapped DB key does), so a test inserting
/// one would silently check nothing. Every effect set a test inserts goes through it.
fn mapped<K: AsRef<str>>(rules: &CampaignRules, rows: Vec<(K, f32)>) -> Vec<(K, f32)> {
    for (k, _) in &rows {
        assert!(rules.effects.maps(k.as_ref()), "effect key {:?} is not in the test mapping (CampaignRules::test_rules_with)", k.as_ref());
    }
    rows
}

/// One test mapping row: DB effect key `db` → the basic bonus `bonus`.
fn map_basic(db: &str, bonus: &str) -> (String, Vec<EffectKey>) {
    (db.into(), vec![EffectKey::basic(bonus)])
}

/// One test mapping row: DB effect key `db` → the chain cost bonus (saved kind 2) of `chain`.
fn map_chain_cost(db: &str, chain: &str) -> (String, Vec<EffectKey>) {
    (db.into(), vec![EffectKey { kind: BonusKind::Saved(2), bonus: "0".into(), qualifier: chain.into() }])
}

/// The tax bundles' growth terms of [`tax_normal_growth_bundles`], under the engine names the growth
/// reads (`recompute_region_with`).
fn tax_growth_mapping() -> [(String, Vec<EffectKey>); 2] {
    [map_basic("tw_growth_taxes_modifier", "tw_growth_tax_modifier"), map_basic("tw_growth_taxes_fixed", "tw_growth_tax_modifier_fixed")]
}

/// [`test_model`] on [`CampaignRules::test_rules_with`] `extra`: the effect keys this test compiles.
fn test_model_with(extra: impl IntoIterator<Item = (String, Vec<EffectKey>)>) -> CampaignModel {
    let mut m = test_model();
    m.rules = Arc::new(CampaignRules::test_rules_with(extra));
    m
}

/// Edits `test_building_level`'s DB effect rows with `edit` and compiles them again (as the fixture
/// does): for effects read through the compiled set ([`CampaignModel::building_local_bonus`]).
pub(super) fn edit_test_building_effects(m: &mut CampaignModel, edit: impl FnOnce(&mut Vec<(String, f32)>)) {
    let mut rules = (*m.rules).clone();
    let level = rules.buildings.get_mut("test_building_level").expect("fixture building");
    edit(&mut level.effects);
    let rows: Vec<(String, f32)> = level.effects.clone();
    rules.effects.insert_building_local("test_building_level".into(), rows.iter().map(|(k, v)| (k.as_str(), *v)));
    m.rules = Arc::new(rules);
}

fn pos(x: i32, z: i32) -> (Fixed20, Fixed20) {
    (Fixed20::from_int(x), Fixed20::from_int(z))
}

fn faction(id: FactionId, key: &str, treasury: i32, government: GovernmentType) -> Faction {
    Faction {
        id,
        key: key.into(),
        treasury,
        government,
        government_key: String::new(),
        tax_lower: "tax_normal".into(),
        tax_upper: "tax_normal".into(),
        diplomacy: BTreeMap::new(),
    }
}

fn region(id: u32, owner: FactionId, gdp: u32, slots: usize) -> Region {
    let mut s: Vec<RegionSlot> = (0..slots)
        .map(|i| RegionSlot { key: format!("test_slot_{i}"), slot_type: "test_slot".into(), building: None, position: None, port: false, holder: None, id: 0 })
        .collect();
    if slots > 0 {
        s[0].building = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
    }
    Region {
        id: RegionId(id),
        key: format!("test_region_{id}"),
        owner,
        settlement: Settlement { key: format!("settlement:test_region_{id}:town"), position: pos(id as i32, 0) },
        slots: s,
        road: None,
        fortification: None,
        road_holder: None,
        fortification_holder: None,
        population: gdp * 100,
        base_gdp: gdp,
        gdp,
        wealth_growth_offset: 0,
        discontent_growth: 0,
        town_wealth: 0,
        town_wealth_growth: 0,
        tax_exempt: false,
        religions: Vec::new(),
        class_bases: Vec::new(),
        unit_resources: Vec::new(),
        // The shipped start positions give every region a base capacity of twice its population.
        population_state: super::population::PopulationState { base_capacity: gdp * 200, trend: 2, ..Default::default() },
        recruitment_queue: Vec::new(),
        construction: Vec::new(),
        garrison: None,
        fleet: None,
    }
}

/// The id of the `i`-th item of `region`'s recruitment queue.
fn queued(m: &CampaignModel, region: RegionId, i: usize) -> super::RecruitmentItemId {
    m.world.regions[&region].recruitment_queue[i].id
}

fn character(id: i32, faction: FactionId, kind: CharacterKind, mp: i32) -> Character {
    Character {
        id: CharacterId(id),
        faction,
        kind,
        position: pos(0, 0),
        movement_points: mp,
        max_movement_points: mp,
        base_movement_points: mp,
        garrisoned_in: None,
    }
}

fn unit(id: i32) -> CampaignUnit {
    CampaignUnit { id: UnitId(id), unit_key: "test_unit".into(), men: 80, max_men: 100, character: None, officer_name: Default::default() }
}

/// A small MADE-UP world: 3 factions (turn order B, A, C), 3 regions, 3 characters, 2 forces,
/// with the made-up [`CampaignRules::test_rules`].
pub(super) fn test_model() -> CampaignModel {
    let mut w = World::default();
    for f in [
        faction(A, "test_faction_a", 1000, GovernmentType::AbsoluteMonarchy),
        faction(B, "test_faction_b", 1000, GovernmentType::Republic),
        faction(C, "test_faction_c", 0, GovernmentType::ConstitutionalMonarchy),
    ] {
        w.factions.insert(f.id, f);
    }
    // Turn order B, A, C: deliberately not id order.
    w.turn_order = vec![B, A, C];
    for r in [region(10, A, 4000, 2), region(11, B, 8000, 1), region(12, A, 2000, 0)] {
        w.regions.insert(r.id, r);
    }
    for c in [
        character(100, A, CharacterKind::General, 30),
        character(101, B, CharacterKind::Colonel, 25),
        character(102, A, CharacterKind::Minister, 0),
    ] {
        w.characters.insert(c.id, c);
    }
    w.forces.insert(
        ForceId(1000),
        MilitaryForce { id: ForceId(1000), faction: A, commander: Some(CharacterId(100)), units: vec![unit(1)], is_navy: false },
    );
    w.forces.insert(
        ForceId(1001),
        MilitaryForce { id: ForceId(1001), faction: B, commander: Some(CharacterId(101)), units: vec![unit(2), unit(3)], is_navy: false },
    );
    let start = Date { year: 1805, season: 1, month: 0, half: HALF_EARLY };
    let mut m = CampaignModel::new(Calendar::new(start, 0), CaRng::new(12345), w);
    m.rules = Arc::new(CampaignRules::test_rules());
    m
}


/// The three region-growth misses 0-B round 14 closed, one per region, with the `tax_normal`
/// bundles of the shipped DB (`tw_growth_taxes_modifier` -0.55 over both classes,
/// `tw_growth_taxes_fixed` -5 over both classes) and the growth terms the vanilla saves carry.
///
/// * `eur_moravia` (Austria, tax-exempt): the technology term survives, the tax term does not
///   (5 stored, -2 before the fix).
/// * `eur_wallachia` (the Ottomans, tax-exempt, no term at all): 0 stays 0 (it was -5).
/// * `eur_bavaria` (Austria owns it, the landless Bavaria governs it): the buildings in its slots
///   count, the faction-wide part is Bavaria's (nothing), not Austria's, so the raw 4 (roads only)
///   is taxed to -3 and the GDP carries no `gdp_mod_all` factor.
fn tax_normal_growth_bundles(rules: &mut CampaignRules) {
    for class in ["lower_classes", "upper_classes"] {
        rules.effects.set_tax_bundle(
            class,
            "tax_normal",
            mapped(rules, vec![("tw_growth_taxes_modifier".into(), -0.275), ("tw_growth_taxes_fixed".into(), -2.5)]),
        );
    }
}

/// One made-up researched technology of `effects` on A's researched list (keys the test mapping maps to
/// themselves, as `effect_bonus_value_basic_junction` does in the shipped DB).
fn with_technology(m: &mut CampaignModel, effects: &[(&str, f32)]) {
    let mut rules = (*m.rules).clone();
    rules.effects.insert_technology("test_growth_tech".into(), mapped(&rules, effects.to_vec()));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies.push(("test_growth_tech".into(), TECH_RESEARCHED));
}

/// Region 10 (owner A) with two land items (costs 300, 500), two ship items (70, 110) and a construction
/// (600), each cost as stored when it was paid.
fn model_with_queued_items() -> CampaignModel {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.units.insert("test_ship".into(), UnitRules { is_naval: true, ..rules.units["test_unit"].clone() });
    m.rules = Arc::new(rules);
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    for (id, unit_key, cost) in [(9001, "test_unit", 300), (9002, "test_ship", 70), (9003, "test_unit", 500), (9004, "test_ship", 110)] {
        r.recruitment_queue.push(RecruitmentItem { id: RecruitmentItemId(id), unit_key: unit_key.into(), turns_remaining: 2, cost, target: None, target_lost: false });
    }
    r.construction.push(ConstructionItem { slot: SlotRef::Slot(0), level_key: "test_building_level".into(), turns_remaining: 2, cost: 600, kind: ConstructionKind::Build, active: true });
    m
}

/// A fixed, made-up script of commands for the determinism test (A and B are human; each acts
/// in its own turn).
fn play(m: &mut CampaignModel, turns: u32) {
    m.turn.humans = vec![A, B];
    m.start_campaign();
    for t in 0..turns {
        let cur = m.turn.current.unwrap();
        if cur == A && t % 7 == 0 {
            let _ = m.apply(CampaignCommand::Recruit { region: RegionId(10), unit_key: "test_unit".into(), target: None });
        }
        if cur == A && t % 5 == 0 {
            let _ = m.apply(CampaignCommand::DeclareWar { a: A, b: B });
        }
        if cur == B && t % 5 == 3 {
            let _ = m.apply(CampaignCommand::MakePeace { a: B, b: A });
        }
        if cur == B {
            let _ = m.apply(CampaignCommand::MoveForce { force: ForceId(1001), to: pos((t % 10) as i32, (t % 3) as i32) });
        }
        m.apply(CampaignCommand::EndTurn).unwrap();
    }
}

/// A test model for the trade split: A trades with B (region 11, GDP 8000) and C (region 12, given to
/// C, GDP 2000); A's capital is region 10 and its trade ships bring home `home` of each commodity.
fn split_model(commodities: &[&str], home: Vec<u32>) -> CampaignModel {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    // Demand of `c0` = the region's GDP; of `c1` = trunc(sqrt(town wealth)).
    rules.commodity_demand.push(("c0".into(), "ddr_GDP".into(), 1.0, 1.0));
    rules.commodity_demand.push(("c1".into(), "ddr_TW".into(), 1.0, 1.0));
    m.rules = Arc::new(rules);
    m.world.commodity_keys = commodities.iter().map(|c| c.to_string()).collect();
    m.world.commodity_prices = vec![10; commodities.len()];
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    for b in [B, C] {
        for (x, y) in [(A, b), (b, A)] {
            m.world.relationships.insert((x, y), details::Relationship { trade_agreement: true, ..Default::default() });
        }
        let end = TradeWaypoint { region: RegionId(11), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None };
        m.world.trade_paths.insert((A, b), vec![TradePath { waypoints: vec![end], volumes: vec![0; commodities.len()], capital_port: false, accumulated: 0 }]);
    }
    let w = TradeWaypoint { region: RegionId(10), from: 120, to: 1, sea: true, from_pos: None, to_pos: None };
    m.world.domestic_trade.insert(A, vec![TradePath { waypoints: vec![w], volumes: home, capital_port: false, accumulated: 0 }]);
    m
}

/// Unit rules for the commander pick tests (MADE-UP keys; categories are the DB's).
fn with_pick_units(m: &mut CampaignModel) {
    let mut rules = (*m.rules).clone();
    for (k, category, campaign_cost, flag_21, is_naval) in [
        ("t_cav", "cavalry", 0, false, false),
        ("t_art", "artillery", 0, false, false),
        ("t_inf_rich", "infantry", 900, false, false),
        ("t_inf_flag", "infantry", 900, true, false),
        ("t_line", "naval_line_of_battle", 0, false, true),
        ("t_frig", "naval_frigate", 0, false, true),
    ] {
        rules.units.insert(k.into(), UnitRules { category: category.into(), campaign_cost, flag_21, is_naval, ..Default::default() });
    }
    m.rules = Arc::new(rules);
}

fn keyed(id: i32, key: &str) -> CampaignUnit {
    CampaignUnit { unit_key: key.into(), ..unit(id) }
}

fn command_land(m: &mut CampaignModel, c: i32, rank: i32) {
    m.world.character_details.entry(CharacterId(c)).or_default().attributes = vec![("command_land".into(), rank), ("command_sea".into(), -1)];
}

fn agent_details(m: &mut CampaignModel, c: i32, abilities: &[(&str, i32, &str)], attributes: &[(&str, i32)]) {
    let d = m.world.character_details.entry(CharacterId(c)).or_default();
    d.abilities = abilities.iter().map(|(k, l, a)| (k.to_string(), *l, a.to_string())).collect();
    d.attributes = attributes.iter().map(|(k, v)| (k.to_string(), *v)).collect();
}

/// A rake of A (103) and gentlemen of A (104) and B (105), all at (0, 0), with the abilities and
/// attributes the saves give them (rake: can_assassinate 1 + subterfuge 2; gentleman: can_duel 1).
fn agents_model() -> CampaignModel {
    let mut m = test_model();
    for (id, f, kind) in [(103, A, CharacterKind::Rake), (104, A, CharacterKind::Gentleman), (105, B, CharacterKind::Gentleman)] {
        m.world.characters.insert(CharacterId(id), character(id, f, kind, 20));
    }
    agent_details(&mut m, 103, &[("can_assassinate", 1, "subterfuge"), ("can_receive_duel", 1, "duelling_pistols")], &[("subterfuge", 2), ("duelling_pistols", 0), ("duelling_swords", 0)]);
    for g in [104, 105] {
        agent_details(&mut m, g, &[("can_duel", 1, "duelling_pistols"), ("can_receive_duel", 1, "duelling_pistols")], &[("duelling_pistols", 3), ("duelling_swords", 1), ("research", 2)]);
    }
    agent_details(&mut m, 101, &[], &[("command_land", 0)]);
    m
}

fn with_relationships(m: &mut CampaignModel, totals: &[((FactionId, FactionId), i32)]) {
    for a in [A, B, C] {
        for b in [A, B, C] {
            if a != b {
                let mut r = details::Relationship { attitudes: vec![details::AttitudeFactor::default(); 24], ..Default::default() };
                if let Some((_, v)) = totals.iter().find(|(k, _)| *k == (a, b)) {
                    r.attitudes[17].value = *v;
                }
                m.world.relationships.insert((a, b), r);
            }
        }
    }
}

/// The test model plus a `test_depot` level that allows only `test_guard` (a copy of `test_recruit`) and has no
/// effects, standing in region 10's second slot.
fn model_with_a_depot() -> CampaignModel {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    let guard = rules.units["test_recruit"].clone();
    rules.units.insert("test_guard".into(), guard);
    rules.allow_unit_for_all("test_guard");
    rules.buildings.insert(
        "test_depot".into(),
        super::rules::BuildingRules { chain: "test_chain".into(), units_allowed: vec!["test_guard".into(), "test_recruit".into()], ..Default::default() },
    );
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[1].building = Some(BuildingRef { level_key: "test_depot".into(), health: 100 });
    m.world.factions.get_mut(&A).unwrap().treasury = 10_000;
    m
}

/// Region 11 (B's) with a town building, war between A and B, A's army next to it.
fn capture_model(humans: Vec<FactionId>) -> CampaignModel {
    let mut m = test_model();
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(-20, 0);
    let r = m.world.regions.get_mut(&RegionId(11)).unwrap();
    r.slots[0].key = "settlement:test_region_11:town:test_slot:0".into();
    r.town_wealth = 1000;
    if !humans.is_empty() {
        m.turn = TurnState::in_turn_of(A, humans);
    }
    m
}

/// A made-up world for the commander pool: A's army 1002 (a colonel, one unit) and fleet 1003 (a
/// captain on its only ship), A's capital at region 10, a General unit for every culture, the
/// vanilla `agents` #11 of the General (4) and the admiral (3).
fn pool_model() -> CampaignModel {
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let mut rules = (*m.rules).clone();
    rules.general_units.insert(String::new(), "test_unit".into());
    rules.agent_caps.insert("General".into(), (4, 0));
    rules.agent_caps.insert("admiral".into(), (3, 0));
    m.rules = Arc::new(rules);
    m.world.characters.insert(CharacterId(107), character(107, A, CharacterKind::Colonel, 25));
    m.world.forces.insert(ForceId(1002), MilitaryForce { id: ForceId(1002), faction: A, commander: Some(CharacterId(107)), units: vec![unit(7)], is_navy: false });
    m.world.characters.insert(CharacterId(108), character(108, A, CharacterKind::Captain, 25));
    let mut ship = unit(8);
    ship.character = Some(CharacterId(108));
    m.world.forces.insert(ForceId(1003), MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(108)), units: vec![ship], is_navy: true });
    m
}

/// A world for the government-change tests: A is an absolute monarchy, B a republic, C a constitutional
/// monarchy, every pair has a relationship record with the setup's `government_type` values, and
/// `government_relations` holds a MADE-UP row per pair (the shipped `absolute vs republic` is #2 = -100,
/// #3 = -30: the setup writes #3, a government change shocks it to #2 and lets it recover to #3).
fn government_test_model() -> CampaignModel {
    let mut m = test_model();
    // C holds region 12, so all three are in the game.
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    for (f, key) in [(A, "gov_absolute_monarchy"), (B, "gov_republic"), (C, "gov_constitutional_monarchy")] {
        m.world.factions.get_mut(&f).unwrap().government_key = key.into();
    }
    let mut rules = (*m.rules).clone();
    for (pair, row) in [
        // (government, other government) -> (#2, #3).
        (("gov_absolute_monarchy", "gov_republic"), (-100, -30)),
        (("gov_republic", "gov_absolute_monarchy"), (-40, 10)),
        (("gov_absolute_monarchy", "gov_constitutional_monarchy"), (-50, 30)),
        (("gov_constitutional_monarchy", "gov_absolute_monarchy"), (70, 20)),
        (("gov_republic", "gov_constitutional_monarchy"), (-60, 10)),
        // The one row with #3 below #2: the drift sign follows the pair.
        (("gov_constitutional_monarchy", "gov_republic"), (70, 15)),
        (("gov_republic", "gov_republic"), (-90, -30)),
    ] {
        rules.government_relations.insert((pair.0.into(), pair.1.into()), row);
    }
    m.rules = Arc::new(rules);
    for (a, b) in [(A, B), (B, A), (A, C), (C, A), (B, C), (C, B)] {
        m.setup_relationship_factors(a, b);
    }
    m
}

/// The world for the commander recruitment tests: A's general 100 (army 1000) stands at (20, 0); A owns
/// region 10 (settlement at (10, 0), the fixture building: `test_unit` / `test_recruit`, 2 recruitment
/// points) and a new region 13 at (13, 0) with the same building, so 13 is the nearer source. Without a
/// map the path cost is the straight line at the off-road cost.
fn commander_recruitment_model() -> CampaignModel {
    let mut m = test_model();
    let r = region(13, A, 1000, 1);
    m.world.regions.insert(r.id, r);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(20, 0);
    m.world.factions.get_mut(&A).unwrap().treasury = 1_000_000;
    m
}

fn option<'a>(r: &'a super::CommanderRecruitment, unit: &str) -> &'a super::CommanderOption {
    r.options.iter().find(|o| o.unit_key == unit).unwrap_or_else(|| panic!("no {unit} option: {r:?}"))
}

/// Faction facts for the destruction tests: `f`'s capital (and home) and whether it is out of the game.
fn set_details(m: &mut CampaignModel, f: FactionId, capital: Option<u32>, out_of_game: bool) {
    let d = m.world.faction_details.entry(f).or_default();
    d.capital = capital.map(RegionId);
    d.capital_2 = capital.map(RegionId);
    d.out_of_game = Some(out_of_game);
}

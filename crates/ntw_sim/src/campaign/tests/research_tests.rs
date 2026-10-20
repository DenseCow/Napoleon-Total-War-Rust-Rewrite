//! Campaign model tests: technology availability and grants.

use super::*;
use super::research::state;
use super::rules::{BuildingRules, TechRules, NO_THREAD};

/// Building levels of four chains: `sAdmin` and its variant `sAdminSpain` share the pair (0, 0),
/// `tFactory` is (0, 2), `tFactorySpain` (no group) is (−1, 0).
fn tech_model() -> CampaignModel {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    let level = |chain: &str, level: i32, thread: i32, class: i32| BuildingRules {
        chain: chain.into(),
        level,
        chain_thread: thread,
        chain_class: class,
        ..Default::default()
    };
    rules.buildings.insert("admin_1".into(), level("sAdmin", 1, 2, 0));
    rules.buildings.insert("admin_spain_2".into(), level("sAdminSpain", 2, 2, 0));
    rules.buildings.insert("admin_0".into(), level("sAdmin", 0, 2, 0));
    rules.buildings.insert("factory_1".into(), level("tFactory", 1, 1, 2));
    rules.buildings.insert("factory_spain_3".into(), level("tFactorySpain", 3, NO_THREAD, 0));
    let tech = |level: &str| TechRules { cost: 10, building_level: level.into(), ..Default::default() };
    rules.technologies.insert("admin_t".into(), tech("admin_1"));
    rules.technologies.insert("eco_t".into(), tech("factory_1"));
    rules.technologies.insert("lost_t".into(), tech("nowhere"));
    m.rules = Arc::new(rules);
    m
}

fn build(m: &mut CampaignModel, slot: usize, level: &str) {
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[slot].building = Some(BuildingRef { level_key: level.into(), health: 100 });
}

/// The building check of `0x008F91F0` (`0x008B13E0`): an owned building whose chain has the tech level's
/// chain pair (record +0x10 / +0x14) at that level or higher; a variant of another group does not serve.
#[test]
fn availability_matches_the_chain_pair_at_the_level_or_higher() {
    let mut m = tech_model();
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("admin_t".into(), state::UNAVAILABLE), ("eco_t".into(), state::UNAVAILABLE), ("lost_t".into(), state::UNAVAILABLE)];
    build(&mut m, 0, "admin_0");
    build(&mut m, 1, "factory_spain_3");
    m.update_tech_availability(A);
    assert_eq!(m.world.faction_details[&A].technologies.iter().map(|(_, s)| *s).collect::<Vec<_>>(), [4, 4, 4], "level 0 too low; (-1, 0) is not (0, 2)");
    build(&mut m, 0, "admin_spain_2");
    build(&mut m, 1, "factory_1");
    // A building in another faction's region counts for nobody else.
    m.world.regions.get_mut(&RegionId(11)).unwrap().slots[0].building = Some(BuildingRef { level_key: "admin_1".into(), health: 0 });
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::AVAILABLE), "sAdminSpain 2 serves sAdmin 1");
    assert_eq!(m.tech_state(A, "eco_t"), Some(state::AVAILABLE));
    assert_eq!(m.tech_state(A, "lost_t"), Some(state::UNAVAILABLE), "an unknown level never opens");
    // Losing the building never closes it again.
    build(&mut m, 0, "admin_0");
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::AVAILABLE));
}

/// The list +0x7C (`technology_required_building_levels_junctions`, `0x008B13C0` → `0x008B1330`): with
/// the requirements met but a level missing (exact chain, that level or higher) the state is 3, which
/// is re-checked like 4 and becomes 2 once the level is owned.
#[test]
fn a_missing_required_building_level_holds_state_3_until_it_is_built() {
    let mut m = tech_model();
    let mut rules = (*m.rules).clone();
    rules.technologies.get_mut("admin_t").unwrap().required_buildings = vec!["factory_1".into()];
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin_t".into(), state::UNAVAILABLE)];
    build(&mut m, 0, "admin_1");
    build(&mut m, 1, "factory_spain_3");
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::BUILDING_MISSING), "a variant chain does not count here");
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::BUILDING_MISSING));
    build(&mut m, 1, "factory_1");
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::AVAILABLE));
}

/// ORIGINAL BUG (`0x00F21A30`): the exe keeps only the last same-tree requirement in +0x60, so the
/// earlier ones are never checked by `0x008F91F0`; ours checks every requirement.
#[test]
fn every_requirement_is_checked_even_two_of_the_same_tree() {
    let mut m = tech_model();
    let mut rules = (*m.rules).clone();
    for k in ["admin_r1", "admin_r2"] {
        rules.technologies.insert(k.into(), TechRules { cost: 10, building_level: "admin_1".into(), ..Default::default() });
    }
    let t = rules.technologies.get_mut("admin_t").unwrap();
    t.requires = vec!["admin_r1".into(), "admin_r2".into()];
    t.parent = Some("admin_r2".into());
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("admin_r1".into(), state::AVAILABLE), ("admin_r2".into(), state::RESEARCHED), ("admin_t".into(), state::UNAVAILABLE)];
    build(&mut m, 0, "admin_1");
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::UNAVAILABLE), "admin_r1 is not researched");
    m.world.faction_details.get_mut(&A).unwrap().technologies[0].1 = state::RESEARCHED;
    m.update_tech_availability(A);
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::AVAILABLE));
}

/// `GrantFactionTechnology` (`0x008CDCB0`): researches the technology and walks its parents (+0x60)
/// while each is at state 1..4, then always updates availability.
#[test]
fn a_grant_walks_the_parents_and_always_updates_availability() {
    let mut m = tech_model();
    let mut rules = (*m.rules).clone();
    let tech = |parent: Option<&str>| TechRules { cost: 10, building_level: "admin_1".into(), parent: parent.map(Into::into), requires: parent.into_iter().map(Into::into).collect(), ..Default::default() };
    rules.technologies.insert("g1".into(), tech(None));
    rules.technologies.insert("g2".into(), tech(Some("g1")));
    rules.technologies.insert("g3".into(), tech(Some("g2")));
    rules.technologies.insert("g4".into(), tech(Some("g3")));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![
        ("g1".into(), state::UNAVAILABLE),
        ("g2".into(), state::RESEARCHED),
        ("g3".into(), state::UNAVAILABLE),
        ("g4".into(), state::AVAILABLE),
        ("admin_t".into(), state::UNAVAILABLE),
    ];
    build(&mut m, 0, "admin_1");
    assert!(m.grant_technology(A, "g4"));
    let states: Vec<u32> = m.world.faction_details[&A].technologies.iter().map(|(_, s)| *s).collect();
    // g4 and g3 researched; the walk stops at the researched g2, so g1 is not researched (it and
    // admin_t only open in the update).
    assert_eq!(states, [state::AVAILABLE, 0, 0, 0, state::AVAILABLE]);
    assert_eq!(m.world.faction_details[&A].research["g3"].progress, 10.0);
    // A technology already researched grants nothing, but availability is still updated.
    m.world.faction_details.get_mut(&A).unwrap().technologies[4].1 = state::UNAVAILABLE;
    assert!(!m.grant_technology(A, "g4"));
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::AVAILABLE));
}

/// The school rate adds the technology's thread effect (record +0x8C, `0x008EA9F0`); thread 3 adds none.
#[test]
fn the_research_rate_reads_the_technology_thread() {
    let mut m = test_model_with([map_basic("research_points", "research_points"), map_basic("research_points_industry", "research_points_industry")]);
    let mut rules = (*m.rules).clone();
    let rows = mapped(&rules, vec![("research_points", 10.0), ("research_points_industry", 5.0)]);
    rules.effects.insert_building_local("test_building_level".into(), rows);
    let tech = |thread: usize| TechRules { cost: 10, building_level: "test_building_level".into(), thread, ..Default::default() };
    rules.technologies.insert("eco_t".into(), tech(1));
    rules.technologies.insert("admin_t".into(), tech(2));
    rules.technologies.insert("none_t".into(), tech(NO_THREAD as usize));
    m.rules = Arc::new(rules);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].id = 77;
    let rate = |t: &str| m.research_rate(RegionId(10), 0, t);
    assert_eq!((rate("eco_t"), rate("admin_t"), rate("none_t")), (15.0, 10.0, 10.0));
}

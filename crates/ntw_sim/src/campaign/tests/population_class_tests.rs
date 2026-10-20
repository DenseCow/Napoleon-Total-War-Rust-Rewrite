//! The population classes' war factor (class +0x20, `0x008A94D0`) and their round-end passes
//! (`0x008F9690` from `GrowRegionPopulation` `0x00AB4070`).

use super::*;
use super::super::population::{class_round, class_round_step, class_war_change, ClassWarEvent, BATTLE_WAR_CHANGE};

/// A's government ranks 3 and B's 1 (`government_types` #3), both with the classes `lower` and `upper`.
fn ranked_model() -> CampaignModel {
    let mut m = test_model();
    m.world.factions.get_mut(&A).unwrap().government_key = "gov_a".into();
    m.world.factions.get_mut(&B).unwrap().government_key = "gov_b".into();
    let rules = Arc::make_mut(&mut m.rules);
    for (g, rank) in [("gov_a", 3), ("gov_b", 1)] {
        rules.government_classes.insert(g.into(), ("upper".into(), "lower".into()));
        rules.government_ranks.insert(g.into(), rank);
    }
    m
}

fn war(m: &CampaignModel, region: u32) -> Vec<(String, i32)> {
    m.world.regions[&RegionId(region)].class_bases.iter().map(|c| (c.class.clone(), c.war)).collect()
}

/// Kinds 0..=5 add the table `0x014579F0`; every change is clamped to [−15, 6], even a value already outside.
#[test]
fn battle_kinds_add_the_table_and_clamp() {
    let m = ranked_model();
    for (kind, change) in BATTLE_WAR_CHANGE.iter().enumerate() {
        assert_eq!(class_war_change(&m, "lower", ClassWarEvent::Battle(kind as u8), A, B, 0), *change, "kind {kind}");
    }
    assert_eq!(BATTLE_WAR_CHANGE, [-1, -2, 1, 2, -4, 4]);
    assert_eq!(class_war_change(&m, "lower", ClassWarEvent::Battle(5), A, B, 4), 6);
    assert_eq!(class_war_change(&m, "lower", ClassWarEvent::Battle(4), A, B, -13), -15);
    assert_eq!(class_war_change(&m, "lower", ClassWarEvent::Battle(0), A, B, 10), 6, "clamped after the change");
    assert_eq!(class_war_change(&m, "lower", ClassWarEvent::Battle(3), A, B, -20), -15);
}

/// Kind 6 by the attitude category of the owner to the target and the two ranks (own 3, theirs 1).
#[test]
fn a_war_declaration_moves_the_classes_by_attitude_and_rank() {
    let mut m = ranked_model();
    let change = |m: &CampaignModel, class: &str, owner, other| class_war_change(m, class, ClassWarEvent::WarDeclared, owner, other, 0);
    // Hostile (0): +4 for the lower class if own ≤ theirs, for the upper if theirs ≤ own.
    with_relationships(&mut m, &[((A, B), -100), ((B, A), -100)]);
    assert_eq!(m.attitude_category(A, B), 0);
    assert_eq!((change(&m, "lower", A, B), change(&m, "upper", A, B)), (0, 4));
    assert_eq!((change(&m, "lower", B, A), change(&m, "upper", B, A)), (4, 0));
    // Unfriendly (1), neutral (2): nothing.
    for total in [-40, 0] {
        with_relationships(&mut m, &[((A, B), total), ((B, A), total)]);
        assert_eq!((change(&m, "lower", A, B), change(&m, "upper", A, B)), (0, 0), "{total}");
    }
    // Friendly (3): −3 for the upper class if own ≤ theirs, for the lower if theirs ≤ own.
    with_relationships(&mut m, &[((A, B), 50), ((B, A), 50)]);
    assert_eq!(m.attitude_category(A, B), 3);
    assert_eq!((change(&m, "lower", A, B), change(&m, "upper", A, B)), (-3, 0));
    assert_eq!((change(&m, "lower", B, A), change(&m, "upper", B, A)), (0, -3));
    // Very friendly (4): −6, same test.
    with_relationships(&mut m, &[((A, B), 100), ((B, A), 100)]);
    assert_eq!(m.attitude_category(A, B), 4);
    assert_eq!((change(&m, "lower", A, B), change(&m, "upper", A, B)), (-6, 0));
    // Equal ranks: both classes; a class of neither kind: nothing.
    Arc::make_mut(&mut m.rules).government_ranks.insert("gov_b".into(), 3);
    assert_eq!((change(&m, "lower", A, B), change(&m, "upper", A, B), change(&m, "middle", A, B)), (-6, -6, 0));
}

/// Nothing changes when either side is the rebels (`0x008CEEF0`), whatever the kind.
#[test]
fn the_rebels_move_no_class() {
    let mut m = ranked_model();
    m.world.factions.get_mut(&C).unwrap().key.clear();
    for event in [ClassWarEvent::Battle(5), ClassWarEvent::WarDeclared] {
        assert_eq!(class_war_change(&m, "lower", event, A, C, 2), 2);
        assert_eq!(class_war_change(&m, "lower", event, C, A, 2), 2);
    }
}

/// `0x00B27070`: the declarer's capital's classes move, before the war factor is set; a region that is not
/// the capital, the target's capital and a joining war's do not; a declarer without a capital declares
/// (ORIGINAL BUG: the exe crashes at `0x00B270D0`).
#[test]
fn only_the_declarers_capital_moves_and_only_for_its_own_war() {
    let mut m = ranked_model();
    with_relationships(&mut m, &[((A, B), -100), ((B, A), -100), ((C, B), -100), ((A, C), -100)]);
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    m.declare_war_rules(A, B).unwrap();
    assert_eq!(war(&m, 10), [("lower".to_owned(), 0), ("upper".to_owned(), 4)], "A ranks above B: the upper class");
    assert!(war(&m, 12).is_empty(), "not the capital");
    assert!(war(&m, 11).is_empty(), "the target's capital");
    // A joining war (on an ally's behalf) moves nothing.
    let mut m = ranked_model();
    with_relationships(&mut m, &[((C, B), -100)]);
    m.world.faction_details.entry(C).or_default().capital = Some(RegionId(12));
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.join_war(C, B, A).unwrap();
    assert!(war(&m, 12).is_empty());
    // No capital: the war is declared and nothing moves.
    let mut m = ranked_model();
    with_relationships(&mut m, &[((A, B), -100)]);
    m.declare_war_rules(A, B).unwrap();
    assert_eq!(m.world.stance(A, B), Stance::War);
    assert!(war(&m, 10).is_empty());
}

/// One pass: the war factor a step toward 0, a negative resistance base +1, a positive education base −2
/// (1 or 2 become 0).
#[test]
fn a_round_pass_decays_the_stored_factors() {
    let step = |war, education, resistance| {
        let mut c = PopulationClassState { class: "lower".into(), education, resistance, war };
        class_round_step(&mut c);
        (c.war, c.education, c.resistance)
    };
    assert_eq!(step(4, 5, -3), (3, 3, -2));
    assert_eq!(step(-4, 2, 3), (-3, 0, 3));
    assert_eq!(step(0, 1, 0), (0, 0, 0));
    assert_eq!(step(-15, -2, -1), (-14, -2, 0));
}

/// `0x00AB4070`: a rebel owner gives each class one pass; otherwise the first lower, the first upper and the
/// first other class take one each, a missing one falling back to the first class.
#[test]
fn the_round_passes_pick_the_classes_as_the_original() {
    let mut m = ranked_model();
    let classes = |keys: &[&str]| keys.iter().map(|k| PopulationClassState { war: 6, ..PopulationClassState::new(*k) }).collect::<Vec<_>>();
    let wars = |m: &CampaignModel| class_round(m, &m.world.regions[&RegionId(10)]).iter().map(|c| c.war).collect::<Vec<_>>();
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = classes(&["lower", "upper", "middle"]);
    assert_eq!(wars(&m), [5, 5, 5]);
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = classes(&["upper", "lower"]);
    assert_eq!(wars(&m), [4, 5], "no third class: the first class takes its pass");
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = classes(&["middle", "lower", "lower"]);
    assert_eq!(wars(&m), [4, 5, 6], "no upper class: the first; only the first lower");
    m.world.factions.get_mut(&A).unwrap().key.clear();
    assert_eq!(wars(&m), [5, 5, 5], "the rebels: one pass each");
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases.clear();
    assert!(wars(&m).is_empty());
}

/// The round end runs the passes on the faction's regions (before the growth), and the war factor is
/// happiness slot 7 of the region's public order.
#[test]
fn the_round_end_decays_and_public_order_counts_the_war_factor() {
    let mut m = ranked_model();
    let base = economy::public_order(&m, RegionId(10));
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases =
        vec![PopulationClassState { war: 5, ..PopulationClassState::new("lower") }, PopulationClassState { war: -3, ..PopulationClassState::new("upper") }];
    m.world.regions.get_mut(&RegionId(11)).unwrap().class_bases = vec![PopulationClassState { war: 5, ..PopulationClassState::new("lower") }];
    let order = economy::public_order(&m, RegionId(10));
    assert_eq!((order.lower, order.upper), (base.lower + 5.0, base.upper - 3.0));
    m.population_round_end(A);
    // Two classes: the lower class takes its pass and the "neither" pass, the upper one.
    assert_eq!(war(&m, 10), [("lower".to_owned(), 3), ("upper".to_owned(), -2)]);
    assert_eq!(war(&m, 11), [("lower".to_owned(), 5)], "B's region waits for B's round end");
}

/// The panel's projection runs the round's class passes on its copy (`0x00A727D0` → `0x00AB4070` with the
/// copy): its classes are the ones the next real round end gives, and its predicted public order reads them.
#[test]
fn the_projection_runs_the_class_passes_on_its_copy() {
    let mut m = ranked_model();
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = vec![
        PopulationClassState { war: 5, education: 4, resistance: -3, ..PopulationClassState::new("lower") },
        PopulationClassState { war: -3, education: 2, resistance: -1, ..PopulationClassState::new("upper") },
    ];
    let before = m.world.regions[&RegionId(10)].class_bases.clone();
    let projection = super::super::population::project(&m, RegionId(10)).unwrap();
    assert_eq!(m.world.regions[&RegionId(10)].class_bases, before, "the projection leaves the region alone");
    assert_ne!(projection.classes, before);
    let reg = &m.world.regions[&RegionId(10)];
    let garrison = economy::garrison_units(&m, reg);
    let war = |classes: &[PopulationClassState]| -> Vec<i32> {
        economy::public_order_factors_with(&m, reg, &projection.set, &projection.religions, classes, garrison).iter().map(|c| c.happiness[7]).collect()
    };
    assert_eq!(war(&projection.classes), [3, -2], "the predicted public order reads the copy's classes");
    assert_eq!(war(&before), [5, -3]);
    let projected = projection.classes.clone();
    m.population_round_end(A);
    assert_eq!(m.world.regions[&RegionId(10)].class_bases, projected, "the next round's classes");
}

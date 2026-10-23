//! The population classes' war factor (class +0x20, `0x008A94D0`), bankruptcy factor (+0x28, `0x008A8A70`)
//! and their round-end passes with the riot checks (`0x008F9690` from `GrowRegionPopulation` `0x00AB4070`).

use super::*;
use super::super::population::{
    bankruptcy_change, class_pass, class_round, class_war_change, order_share, riot_chance, unrest_cause, ClassWarEvent, UnrestFlags,
    BATTLE_WAR_CHANGE, PROJECTION_SEED, UNREST_REBELLION,
};
use crate::rng::CaRng;
use std::collections::BTreeSet;

const ALL: UnrestFlags = UnrestFlags { rebellion: true, riots: true, strikes: true, demands: true };

/// A class of `key` whose stored order is −10 (negative sum), so its riot check runs.
fn unhappy(key: &str) -> PopulationClassState {
    PopulationClassState { negative: -10, ..PopulationClassState::new(key) }
}

/// `m` with `lower` a striking class and `upper` a demanding one (`population_classes` #1 / #2), and the
/// demand of cause 1 (`taxes`) for each kind made up as 10 + kind.
fn unrest_model() -> CampaignModel {
    let mut m = ranked_model();
    let rules = Arc::make_mut(&mut m.rules);
    rules.class_unrest_paths.insert("lower".into(), (true, false));
    rules.class_unrest_paths.insert("upper".into(), (false, true));
    for kind in 1..=4 {
        rules.unrest_demands.insert((1, kind), 10 + kind);
    }
    m
}

/// One pass of `class` in A's region 10 with `flags`, from a fixed stream: (kind returned, the class after).
fn pass(m: &CampaignModel, mut class: PopulationClassState, flags: UnrestFlags) -> (u32, PopulationClassState) {
    let kind = class_pass(m, &m.world.regions[&RegionId(10)], &mut class, flags, &mut CaRng::new(1));
    (kind, class)
}

/// A's government ranks 3 and B's 1 (`government_types` #3), both with the classes `lower` and `upper`.
pub(super) fn ranked_model() -> CampaignModel {
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

pub(super) fn war(m: &CampaignModel, region: u32) -> Vec<(String, i32)> {
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

/// One pass (`0x008F9690`): the war factor a step toward 0, a negative bankruptcy factor and a negative
/// resistance base +1, a positive crackdown −1, a positive education base −2 (1 or 2 become 0).
#[test]
fn a_round_pass_decays_the_stored_factors() {
    let m = ranked_model();
    let step = |war, bankruptcy, crackdown, education, resistance| {
        let c = PopulationClassState { war, bankruptcy, crackdown, education, resistance, ..PopulationClassState::new("lower") };
        let (_, c) = pass(&m, c, UnrestFlags::default());
        (c.war, c.bankruptcy, c.crackdown, c.education, c.resistance)
    };
    assert_eq!(step(4, -16, 6, 5, -3), (3, -15, 5, 3, -2));
    assert_eq!(step(-4, 0, 0, 2, 3), (-3, 0, 0, 0, 3));
    assert_eq!(step(0, -1, 1, 1, 0), (0, 0, 0, 0, 0));
    assert_eq!(step(-15, 3, -2, -2, -1), (-14, 3, -2, -2, 0));
}

/// The bankruptcy factor (`0x008A8A70`): −4, clamped to [−16, 0] even from a value outside.
#[test]
fn bankruptcy_takes_four_and_clamps() {
    assert_eq!(bankruptcy_change(0), -4);
    assert_eq!(bankruptcy_change(-14), -16);
    assert_eq!(bankruptcy_change(-20), -16);
    assert_eq!(bankruptcy_change(9), 0);
}

/// A faction that cannot pay takes 4 from every class's bankruptcy factor in its capital
/// (`0x00A65DC0` → `0x00A61570` → `0x00A61580`), and only there; the round end then decays it by 1.
#[test]
fn a_bankrupt_faction_lowers_its_capitals_classes() {
    let mut m = ranked_model();
    for r in [11, 12] {
        m.world.regions.get_mut(&RegionId(r)).unwrap().owner = B;
        m.world.regions.get_mut(&RegionId(r)).unwrap().class_bases = ["lower", "upper", "middle"].map(PopulationClassState::new).to_vec();
    }
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    economy::refresh_running_record_round_start(&mut m, B);
    m.world.factions.get_mut(&B).unwrap().treasury = -1_000_000;
    assert!(matches!(economy::settle_round(&mut m, B, &mut Vec::new()), economy::Settlement::CannotPay { .. }));
    let bankruptcy = |m: &CampaignModel, r| m.world.regions[&RegionId(r)].class_bases.iter().map(|c| c.bankruptcy).collect::<Vec<_>>();
    assert_eq!(bankruptcy(&m, 11), [-3, -3, -3], "−4, then the region's round end decays it by 1");
    assert_eq!(bankruptcy(&m, 12), [0, 0, 0], "not the capital");
}

/// The share of order (`0x008C9120`) and the riot chance (`0x008F9690`): 50 with both sums 0; else
/// |positive| / (|positive| + |negative|) × 100; the chance (50 − trunc(share)) × 2 within 0..=100.
#[test]
fn the_riot_chance_follows_the_share_of_order() {
    assert_eq!(order_share(0, 0), 50.0);
    assert_eq!(order_share(10, -30), 25.0);
    assert_eq!(order_share(-10, 30), 25.0, "absolute values");
    assert_eq!((riot_chance(0, 0), riot_chance(10, -30), riot_chance(0, -5), riot_chance(10, 0)), (0, 50, 100, 0));
    assert_eq!(riot_chance(2, -1), 0, "share 66 is above 50: no chance");
    assert_eq!(riot_chance(1, -2), 34, "share 33.3 truncates to 33");
}

/// The cause (`0x008B37D0`): the slots below 0, most negative first; one of the first two from three on, the
/// first with two (still one draw), the only one without a draw; none: 1 (ORIGINAL BUG: the exe reads a null
/// list, `0x008B37D0`). Equal slots keep their order (the exe's stable insertion sort).
#[test]
fn the_cause_is_one_of_the_two_unhappiest_slots() {
    let mut slots = [0; 13];
    let mut rng = CaRng::new(5);
    assert_eq!(unrest_cause(&slots, &mut rng), 1, "no slot below 0");
    assert_eq!(rng, CaRng::new(5), "no draw");
    slots[4] = -3;
    assert_eq!(unrest_cause(&slots, &mut rng), 4);
    assert_eq!(rng, CaRng::new(5), "one slot: no draw");
    slots[9] = -8;
    assert_eq!(unrest_cause(&slots, &mut rng), 9, "the most negative");
    let mut once = CaRng::new(5);
    once.uniform_below(1);
    assert_eq!(rng, once, "two slots: one draw below 1");
    slots[2] = -5;
    slots[0] = 3;
    let picks: BTreeSet<u32> = (0..64u32).map(|seed| unrest_cause(&slots, &mut CaRng::new(seed))).collect();
    assert_eq!(picks, BTreeSet::from([9, 2]), "one of the two unhappiest, never the third");
    let mut rng = CaRng::new(77);
    let mut draw = CaRng::new(77);
    let expected = [9, 2][draw.uniform_below(2) as usize];
    assert_eq!(unrest_cause(&slots, &mut rng), expected);
    assert_eq!(rng, draw);
    let mut ties = [0; 13];
    (ties[8], ties[3], ties[6]) = (-2, -2, -2);
    let picks: BTreeSet<u32> = (0..64u32).map(|seed| unrest_cause(&ties, &mut CaRng::new(seed))).collect();
    assert_eq!(picks, BTreeSet::from([3, 6]), "a tie keeps slot order");
    (ties[1], ties[11]) = (-1, -9);
    let picks: BTreeSet<u32> = (0..64u32).map(|seed| unrest_cause(&ties, &mut CaRng::new(seed))).collect();
    assert_eq!(picks, BTreeSet::from([11, 3]), "a later, lower slot goes first");
}

/// The gate: a rebel owner returns 0 and keeps its unrest; a class of neither government kind, a stored order
/// of 0 or more, or no flag: stage, kind and demand become 0. A rebellion (kind 5) is cleared by the decay.
#[test]
fn the_riot_check_needs_a_governed_unhappy_class_and_a_flag() {
    let mut m = unrest_model();
    let restless = |key: &str| PopulationClassState { stage: 1, unrest: 1, demand: 11, ..unhappy(key) };
    let cleared = |c: &PopulationClassState| (c.stage, c.unrest, c.demand);
    assert_eq!(cleared(&pass(&m, restless("middle"), ALL).1), (0, 0, 0), "neither class");
    assert_eq!(cleared(&pass(&m, PopulationClassState { negative: 0, ..restless("lower") }, ALL).1), (0, 0, 0), "order 0");
    assert_eq!(cleared(&pass(&m, PopulationClassState { repression: 10, ..restless("lower") }, ALL).1), (0, 0, 0), "repressed");
    assert_eq!(cleared(&pass(&m, restless("lower"), UnrestFlags::default()).1), (0, 0, 0), "no flag");
    let (kind, rebel) = pass(&m, PopulationClassState { stage: 3, unrest: UNREST_REBELLION, demand: 4, ..unhappy("lower") }, ALL);
    assert_eq!((kind, rebel.stage, rebel.demand), (1, 1, 11), "a rebellion clears to stage 0, then the class strikes again");
    m.world.factions.get_mut(&A).unwrap().key.clear();
    let (kind, c) = pass(&m, restless("lower"), ALL);
    assert_eq!((kind, cleared(&c)), (0, (1, 1, 11)), "the rebels: 0, unrest kept");
    assert_eq!(c.riot_chance, 0, "no check");
}

/// The stages: a striking class strikes (1) at stage 0 and riots (2) at stage 1; a demanding class sends a
/// letter of demand (4) at stage 0 and an ultimatum (3) at stage 1; from stage 2 it rebels (5). Each step
/// grows the stage and takes the cause's demand for the new kind; a step not allowed changes nothing.
#[test]
fn the_unrest_stages_step_by_the_class_paths() {
    let m = unrest_model();
    let run = |class: PopulationClassState, flags| {
        let (kind, c) = pass(&m, class, flags);
        (kind, c.stage, c.demand, c.cause, c.riot_chance)
    };
    let lower = unhappy("lower");
    assert_eq!(run(lower.clone(), ALL), (1, 1, 11, 1, 100), "strike; cause 1 (no slot below 0); chance 100");
    assert_eq!(run(PopulationClassState { stage: 1, unrest: 1, ..lower.clone() }, ALL), (2, 2, 12, 1, 100), "riot");
    assert_eq!(run(PopulationClassState { stage: 2, unrest: 2, ..lower.clone() }, ALL), (UNREST_REBELLION, 3, 0, 1, 100), "rebellion, no demand");
    let no_rebellion = UnrestFlags { rebellion: false, ..ALL };
    assert_eq!(run(PopulationClassState { stage: 2, unrest: 2, demand: 12, ..lower.clone() }, no_rebellion), (2, 2, 12, 1, 100), "kept");
    assert_eq!(run(lower.clone(), UnrestFlags { strikes: false, ..ALL }), (0, 0, 0, 1, 100), "no strike allowed");
    let upper = unhappy("upper");
    assert_eq!(run(upper.clone(), ALL), (4, 1, 14, 1, 100), "letter of demand");
    assert_eq!(run(PopulationClassState { stage: 1, unrest: 4, ..upper.clone() }, ALL), (3, 2, 13, 1, 100), "ultimatum");
    assert_eq!(run(upper.clone(), UnrestFlags { demands: false, ..ALL }), (0, 0, 0, 1, 100));
    let mut slots = PopulationClassState { negative: -10, ..unhappy("upper") };
    slots.happiness[3] = -2;
    assert_eq!(run(slots, ALL).3, 3, "the cause is the unhappy slot");
}

/// `0x00AB4070`'s flags: the lower class gets every flag while no rebel army has risen in the region, rebellion
/// only while rebellions are not switched off (+0xEB); the upper class none after a lower rebellion, only
/// rebellion and demands after a strike... and the lower class's kind and demand clear when the upper class
/// rebels or riots while the lower strikes.
#[test]
fn the_round_flags_follow_the_region_and_the_lower_class() {
    let mut m = unrest_model();
    Arc::make_mut(&mut m.rules).class_unrest_paths.insert("upper".into(), (true, false));
    // A third class takes the "neither" pass, so the first class does not take a second one.
    let round = |m: &CampaignModel, classes: &[PopulationClassState]| {
        let classes = [classes, &[PopulationClassState::new("middle")]].concat();
        class_round(m, &m.world.regions[&RegionId(10)], &classes, &mut CaRng::new(1))
            .iter()
            .take(2)
            .map(|c| (c.unrest, c.stage, c.demand))
            .collect::<Vec<_>>()
    };
    let at = |key: &str, stage, unrest| PopulationClassState { stage, unrest, ..unhappy(key) };
    // Both strike: the upper class may not strike after the lower one did.
    assert_eq!(round(&m, &[at("lower", 0, 0), at("upper", 0, 0)]), [(1, 1, 11), (0, 0, 0)]);
    // The lower class riots: the upper class may not riot or strike either.
    assert_eq!(round(&m, &[at("lower", 1, 1), at("upper", 1, 1)]), [(2, 2, 12), (1, 1, 0)]);
    // The lower strikes and the upper riots: the lower class's kind and demand clear, its stage stays.
    assert_eq!(round(&m, &[at("lower", 0, 0), at("upper", 1, 1)]), [(0, 1, 0), (2, 2, 12)]);
    // The lower class rebels: the upper class gets no flag and resets.
    assert_eq!(round(&m, &[at("lower", 2, 2), at("upper", 1, 1)]), [(UNREST_REBELLION, 3, 0), (0, 0, 0)]);
    // The upper class rebels: the lower class's kind and demand clear.
    assert_eq!(round(&m, &[at("lower", 0, 0), at("upper", 2, 2)]), [(0, 1, 0), (UNREST_REBELLION, 3, 0)]);
    // Rebellions switched off: stage 2 keeps its kind.
    m.world.rebellions_disabled = true;
    assert_eq!(round(&m, &[at("lower", 2, 2), at("upper", 0, 0)]), [(2, 2, 0), (0, 0, 0)]);
    // A risen rebel army: no flag at all, so both reset.
    m.world.rebellions_disabled = false;
    m.world.region_risen_armies.insert(RegionId(10), vec![ForceId(1000)]);
    assert_eq!(round(&m, &[at("lower", 1, 1), at("upper", 1, 1)]), [(0, 0, 0), (0, 0, 0)]);
}

/// `0x00AB4070`: a rebel owner gives each class one pass; otherwise the first lower, the first upper and the
/// first other class take one each, a missing one falling back to the first class.
#[test]
fn the_round_passes_pick_the_classes_as_the_original() {
    let mut m = ranked_model();
    let classes = |keys: &[&str]| keys.iter().map(|k| PopulationClassState { war: 6, ..PopulationClassState::new(*k) }).collect::<Vec<_>>();
    let wars = |m: &CampaignModel| {
        let reg = &m.world.regions[&RegionId(10)];
        class_round(m, reg, &reg.class_bases, &mut CaRng::new(1)).iter().map(|c| c.war).collect::<Vec<_>>()
    };
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
    m.refresh_population_factors();
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

/// The projection's riot checks draw from their own stream seeded 0x61266 (`0x00A727D0`), on the copy's
/// refreshed sums, and leave the campaign's stream alone.
#[test]
fn the_projection_draws_from_its_own_stream() {
    let mut m = unrest_model();
    let mut lower = unhappy("lower");
    (lower.happiness[1], lower.happiness[2], lower.happiness[3]) = (-1, -2, -3);
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = vec![lower, unhappy("upper")];
    let rng = m.rng;
    let projection = super::super::population::project(&m, RegionId(10)).unwrap();
    assert_eq!(m.rng, rng);
    let reg = &m.world.regions[&RegionId(10)];
    let set = economy::region_effect_set(&m, reg);
    let input = super::super::population::FactorInputs::of(&m, reg, &set);
    let refreshed = super::super::population::refresh_class_sums(&m, reg, &input);
    assert_eq!(projection.classes, class_round(&m, reg, &refreshed, &mut CaRng::new(PROJECTION_SEED)));
}

/// The round end draws the riot checks from the campaign's stream (model +0xFB8).
#[test]
fn the_round_end_draws_from_the_campaign_stream() {
    let mut m = unrest_model();
    let mut lower = unhappy("lower");
    (lower.happiness[1], lower.happiness[2], lower.happiness[3]) = (-1, -2, -3);
    m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = vec![lower];
    let before = m.rng;
    let mut expected = m.rng;
    let reg = &m.world.regions[&RegionId(10)];
    let classes = class_round(&m, reg, &reg.class_bases, &mut expected);
    m.population_region_round_end(RegionId(10));
    assert_ne!(m.rng, before, "the riot check drew");
    assert_eq!(m.rng, expected);
    assert_eq!(m.world.regions[&RegionId(10)].class_bases, classes);
}

/// The region's discontent growth (#18, `0x00AB42F0`): −trunc(var 107) while a rebel army has risen there or
/// its upper or lower class has unrest; else 0.
#[test]
fn discontent_follows_unrest_and_risen_armies() {
    let mut m = ranked_model();
    Arc::make_mut(&mut m.rules).variables.insert("town_wealth_growth_discontent_reduction".into(), 12.7);
    let d = |m: &CampaignModel| economy::discontent_growth(m, &m.world.regions[&RegionId(10)]);
    let set = |m: &mut CampaignModel, classes: Vec<PopulationClassState>| m.world.regions.get_mut(&RegionId(10)).unwrap().class_bases = classes;
    assert_eq!(d(&m), 0, "no classes");
    set(&mut m, vec![PopulationClassState::new("lower"), PopulationClassState::new("upper")]);
    assert_eq!(d(&m), 0);
    set(&mut m, vec![PopulationClassState::new("lower"), PopulationClassState { unrest: 4, ..PopulationClassState::new("upper") }]);
    assert_eq!(d(&m), -12, "truncated");
    set(&mut m, vec![PopulationClassState { unrest: 1, ..PopulationClassState::new("lower") }]);
    assert_eq!(d(&m), -12);
    set(&mut m, vec![PopulationClassState::new("lower"), PopulationClassState::new("upper"), PopulationClassState { unrest: 1, ..PopulationClassState::new("middle") }]);
    assert_eq!(d(&m), 0, "the class of neither kind does not count");
    m.world.region_risen_armies.insert(RegionId(10), vec![ForceId(1000)]);
    assert_eq!(d(&m), -12, "a risen army");
}

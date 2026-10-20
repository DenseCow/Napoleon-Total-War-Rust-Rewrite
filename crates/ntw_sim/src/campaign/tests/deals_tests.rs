//! Campaign model tests: deals.

use super::*;

#[test]
fn population_factors_round_to_hundredths_as_the_exe_stores_them() {
    use super::effects::EffectSet;
    use super::population::{growth_factors, hundredths, FactorInputs};
    // The vanilla saves' stored values: 0.3 → 0.29999998, −0.37 → −0.37, eight hostile units × −0.05 →
    // −0.39999998 (the product of 0.01f and 40 lies exactly between two floats and rounds to even).
    assert_eq!(hundredths(0.3).to_bits(), 0.29999998f32.to_bits());
    assert_eq!(hundredths(-0.37).to_bits(), (-0.37f32).to_bits());
    assert_eq!(hundredths(8.0 * -0.05).to_bits(), (-0.39999998f32).to_bits());
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("baseline_pop_growth".into(), 0.3);
    m.rules = Arc::new(rules);
    let r = m.world.regions[&RegionId(10)].clone();
    let set = EffectSet::default();
    let s = growth_factors(&m, &r, r.population, &r.population_state, &FactorInputs { hostile_units: 8, ..FactorInputs::of(&m, &r, &set) });
    assert_eq!(s.factors[0].to_bits(), 0.29999998f32.to_bits());
    assert_eq!(s.factors[3].to_bits(), (-0.39999998f32).to_bits());
    assert_eq!(s.capacity, r.population_state.base_capacity);
    assert!(!s.overcrowded);
    // Above 90% of the capacity: food shortages (0.9 − r) × 10 × (base + buildings + ports).
    let crowded = growth_factors(&m, &r, r.population_state.base_capacity / 100 * 95, &r.population_state, &FactorInputs { hostile_units: 0, ..FactorInputs::of(&m, &r, &set) });
    assert!(crowded.overcrowded);
    assert_eq!(crowded.factors[4], hundredths((0.9 - 0.95f32) * 10.0 * 0.29999998));
}

#[test]
fn population_grows_by_its_rounded_share_and_keeps_the_minimum() {
    use super::population::{grow, PopulationState};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("minimum_population".into(), 1000.0);
    m.rules = Arc::new(rules);
    let r = m.world.regions[&RegionId(10)].clone();
    // `auto_nr4_t4` → `orig_over_nr4_0252`, eur_east_prussia: 1296529 at −0.07% → 1295621, down.
    let state = PopulationState { factors: [0.29999998, 0.0, -0.37, 0.0, 0.0, 0.0, 0.0], trend: 3, ..Default::default() };
    let (pop, next) = grow(&m, &r, 1_296_529, &state);
    assert_eq!((pop, next.trend), (1_295_621, 3));
    // Never below `minimum_population`; unchanged reads as trend 2.
    assert_eq!(grow(&m, &r, 1000, &state).0, 1000);
    assert_eq!(grow(&m, &r, 1000, &state).1.trend, 2);
    let up = PopulationState { factors: [0.29999998, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], ..Default::default() };
    assert_eq!(grow(&m, &r, 100_000, &up), (100_300, PopulationState { growth: up.factors[0], trend: 1, ..up.clone() }));
}

#[test]
fn religion_normalisation_drops_dust_and_leaves_an_empty_breakdown_alone() {
    use super::population::normalise_religions;
    let mut shares = vec![("a".to_string(), 0.6f32), ("b".to_string(), 0.3), ("c".to_string(), 0.0004)];
    normalise_religions(&mut shares);
    assert_eq!(shares[2].1, 0.0);
    assert!((shares[0].1 - 0.6 / 0.9).abs() < 1e-6 && (shares[1].1 - 0.3 / 0.9).abs() < 1e-6);
    // ORIGINAL BUG (0x00AA4860): the exe writes the first share of an empty breakdown through a null list.
    let mut empty: Vec<(String, f32)> = Vec::new();
    normalise_religions(&mut empty);
    assert!(empty.is_empty());
    let mut zero = vec![("a".to_string(), 0.0f32), ("b".to_string(), 0.0)];
    normalise_religions(&mut zero);
    assert_eq!((zero[0].1, zero[1].1), (1.0, 0.0));
    // A NaN share fails the minimum and is zeroed like dust.
    let mut nan = vec![("a".to_string(), f32::NAN), ("b".to_string(), 0.5)];
    normalise_religions(&mut nan);
    assert_eq!((nan[0].1, nan[1].1), (0.0, 1.0));
}

/// One share read without copying the breakdown gives what [`normalise_religions`] writes there.
#[test]
fn a_normalised_share_matches_the_normalised_breakdown() {
    use super::population::{normalise_religions, normalised_share};
    let cases: [&[f32]; 5] = [&[0.6, 0.3, 0.0004], &[0.0, 0.0], &[0.25, 0.75], &[0.0001, 0.2], &[f32::NAN, 0.5]];
    for shares in cases {
        let original: Vec<(String, f32)> = shares.iter().enumerate().map(|(i, s)| (i.to_string(), *s)).collect();
        let mut normalised = original.clone();
        normalise_religions(&mut normalised);
        for (i, (_, s)) in normalised.iter().enumerate() {
            assert_eq!(normalised_share(&original, i).to_bits(), s.to_bits(), "{shares:?} [{i}]");
        }
    }
}

#[test]
fn the_round_end_refreshes_then_grows_every_region() {
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("baseline_pop_growth".into(), 0.3);
    m.rules = Arc::new(rules);
    let before = m.world.regions[&RegionId(10)].population;
    m.end_turn();
    m.end_turn();
    m.end_turn();
    let r = &m.world.regions[&RegionId(10)];
    assert!(r.population > before, "{} -> {}", before, r.population);
    assert_eq!(r.population_state.trend, 1);
    assert_eq!(r.population_state.factors[0].to_bits(), 0.29999998f32.to_bits());
}

#[test]
fn town_wealth_growth_breaks_down_by_factor_and_predicts_constructions() {
    use super::economy::{region_wealth, wealth_trend};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.buildings.get_mut("test_building_level").unwrap().effects.push(("tw_growth_education".into(), 7.0));
    rules.buildings.get_mut("test_building_level_2").unwrap().effects.push(("tw_growth_roads".into(), 30.0));
    m.rules = Arc::new(rules);
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    r.discontent_growth = -4;
    let reg = r.clone();
    let now = region_wealth(&m, None, &reg, false);
    assert_eq!(now.factors[0], 7, "{now:?}");
    assert_eq!(now.factors[9], -4);
    assert_eq!(now.growth, super::economy::recompute_region(&m, &reg).1);
    // Predicted: the level under construction in the slot's place (the education building upgraded to roads).
    let r = m.world.regions.get_mut(&RegionId(10)).unwrap();
    r.construction.push(ConstructionItem { slot: SlotRef::Slot(0), level_key: "test_building_level_2".into(), turns_remaining: 2, cost: 600, kind: ConstructionKind::Build, active: true });
    let reg = r.clone();
    let next = region_wealth(&m, None, &reg, true);
    assert_eq!((next.factors[0], next.factors[4]), (0, 30), "{next:?}");
    assert_eq!(region_wealth(&m, None, &reg, false), now);
    // The trend codes of 0x00AB4410.
    assert_eq!([21, 20, 1, 0, -1, -20, -21].map(wealth_trend), [0, 1, 1, 2, 3, 3, 4]);
}

/// `0x00C5C040`: a faction offers every region it owns but its capital.
#[test]
fn tradeable_regions_leave_out_the_capital() {
    let mut m = test_model();
    let owned: Vec<RegionId> = m.world.regions.values().filter(|r| r.owner == A).map(|r| r.id).collect();
    assert!(owned.contains(&RegionId(10)) && owned.len() > 1, "{owned:?}");
    m.world.faction_details.entry(A).or_default().capital = None;
    assert_eq!(m.tradeable_regions(A).collect::<Vec<_>>(), owned);
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    let without: Vec<RegionId> = owned.iter().copied().filter(|r| *r != RegionId(10)).collect();
    assert_eq!(m.tradeable_regions(A).collect::<Vec<_>>(), without);
    // In the faction's region list order (faction +0x778): the manager's order, a gain last.
    m.world.faction_details.entry(A).or_default().capital = None;
    let mut listed = owned.clone();
    listed.reverse();
    m.world.region_order = listed.clone();
    assert_eq!(m.tradeable_regions(A).collect::<Vec<_>>(), listed);
    m.world.note_region_gain(listed[0]);
    listed.rotate_left(1);
    assert_eq!(m.tradeable_regions(A).collect::<Vec<_>>(), listed, "a gain goes last");
}

/// The deal's region and technology records (UI_FIDELITY.md §4.9): `ProposeRegions` replaces the
/// regions record (`0x00933CC0` → `0x00C4B0A0`), two empty lists leave it, `clear` empties it;
/// `ProposeTechnologies` (`0x009340A0`) clears on two empty lists too; an unknown region or
/// technology aborts; nothing without a negotiation.
#[test]
fn deal_records_follow_the_propose_executors() {
    use super::rules::TechRules;
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    let regions = |m: &CampaignModel| m.negotiations.current.as_ref().unwrap().regions.clone();
    let techs = |m: &CampaignModel| m.negotiations.current.as_ref().unwrap().technologies.clone();
    let propose = |clear, demanded: &[u32], offered: &[u32]| CampaignCommand::ProposeRegions {
        clear,
        demanded: demanded.iter().map(|&r| RegionId(r)).collect(),
        offered: offered.iter().map(|&r| RegionId(r)).collect(),
    };
    assert_eq!(m.apply(propose(false, &[11], &[])), Err(CommandError::NoNegotiation));
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(propose(false, &[11], &[12])).unwrap();
    assert_eq!((regions(&m).demanded, regions(&m).offered), (vec![RegionId(11)], vec![RegionId(12)]));
    // Owners are not checked (0x00933CC0, 0x00C4B0A0): a demanded region the proposer A owns
    // and an offered one of the recipient B are kept as given.
    m.apply(propose(false, &[12], &[11])).unwrap();
    assert_eq!((regions(&m).demanded, regions(&m).offered), (vec![RegionId(12)], vec![RegionId(11)]));
    // Replaced, not added to.
    m.apply(propose(false, &[], &[10])).unwrap();
    assert_eq!((regions(&m).demanded, regions(&m).offered), (vec![], vec![RegionId(10)]));
    // Two empty lists leave the regions record; an unknown region aborts.
    m.apply(propose(false, &[], &[])).unwrap();
    assert_eq!(regions(&m).offered, vec![RegionId(10)]);
    assert_eq!(m.apply(propose(false, &[99], &[])), Err(CommandError::UnknownRegion(RegionId(99))));
    assert_eq!(regions(&m).offered, vec![RegionId(10)]);
    m.apply(propose(true, &[11], &[])).unwrap();
    assert!(regions(&m).is_empty());
    // Technologies: an unknown key aborts; two empty lists clear.
    let t = |demanded: &[&str], offered: &[&str]| CampaignCommand::ProposeTechnologies {
        clear: false,
        demanded: demanded.iter().map(|s| s.to_string()).collect(),
        offered: offered.iter().map(|s| s.to_string()).collect(),
    };
    m.apply(t(&["admin1_a"], &[])).unwrap();
    assert_eq!(techs(&m).demanded, vec!["admin1_a".to_string()]);
    assert_eq!(m.apply(t(&["nope"], &[])), Err(CommandError::UnknownTechnology("nope".into())));
    assert_eq!(techs(&m).demanded, vec!["admin1_a".to_string()]);
    m.apply(t(&[], &[])).unwrap();
    assert!(techs(&m).is_empty());
    // Clear empties both records; a new negotiation starts empty.
    m.apply(propose(false, &[11], &[])).unwrap();
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert!(regions(&m).is_empty());
    m.apply(propose(false, &[11], &[])).unwrap();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    assert!(regions(&m).is_empty());
}

/// Accepting the deal (`0x00C114B0`): the regions record's demanded regions go to the proposer and
/// the offered ones to the recipient (`0x00C18BF0` → `0x00B449F0`): queues cleared, research at
/// the region's schools stopped. The old owner's garrison leaves the settlement (`0x008D9DC0` for
/// the proposer on the offered regions; more in tests/transfer.rs).
#[test]
fn accepted_regions_change_hands_and_put_the_old_garrison_out() {
    use super::details::TechResearch;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    // Region 12 (A) holds A's army and A researches at its school (slot id 55).
    m.world.regions.get_mut(&RegionId(12)).unwrap().garrison = Some(ForceId(1000));
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(12));
    m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(RegionSlot {
        key: "school".into(),
        slot_type: "test_slot".into(),
        building: None,
        position: None,
        port: false,
        holder: None,
        id: 55,
        blockade: None,
    });
    m.world.faction_details.entry(A).or_default().research.insert("t".into(), TechResearch { progress: 4.0, researcher: 55, traded: 0 });
    m.world.regions.get_mut(&RegionId(11)).unwrap().recruitment_queue.push(RecruitmentItem {
        id: RecruitmentItemId(9),
        unit_key: "u".into(),
        turns_remaining: 2,
        cost: 0,
        target: None,
        target_lost: false,
    });
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![RegionId(12)] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert_eq!(m.world.regions[&RegionId(12)].owner, B);
    assert!(m.world.regions[&RegionId(11)].recruitment_queue.is_empty());
    assert_eq!(m.world.faction_details[&A].research["t"], TechResearch { progress: 4.0, researcher: 0, traded: 0 });
    // A's army leaves B's settlement for A's nearest region, 10 (A has no capital: the deal's own
    // pass, not the owner change's).
    assert_eq!(m.world.regions[&RegionId(12)].garrison, None);
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.characters[&CharacterId(100)].position, pos(10, 0));
    // Accepting again changes nothing (the regions are already theirs: 0x00A64AC0 returns).
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!((m.world.regions[&RegionId(11)].owner, m.world.regions[&RegionId(12)].owner), (A, B));
    // No negotiation, no deal.
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::NoNegotiation));
}

/// A threat (`ProposeDeal(true)`, negotiation +0x2B8): whether accepted (`0x00C114B0`, at
/// `0x00C11671`) or declined (`0x00C1F210`), the recipient gets the `threatened` event (−30, drift 3,
/// limit 0) added towards the proposer (`0x00B72F00` → `0x00B72EC0` → `0x00B02CB0`); the
/// proposer's record is untouched, a plain offer leaves both alone, and a decline without a
/// negotiation is refused.
#[test]
fn a_threat_leaves_the_threatened_factor_on_accept_and_decline() {
    let threatened = |m: &CampaignModel, a, b| m.world.relationships.get(&(a, b)).and_then(|r| r.factor("threatened").copied());
    let answer = |threat: bool, decline: bool| {
        let mut m = test_model();
        m.turn.humans = vec![A, B];
        m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
        m.apply(CampaignCommand::ProposeDeal { threat }).unwrap();
        let before = (threatened(&m, B, A), threatened(&m, A, B));
        m.apply(if decline { CampaignCommand::DeclineDeal } else { CampaignCommand::AcceptDeal }).unwrap();
        (m, before)
    };
    for decline in [false, true] {
        let (m, (before, proposer_before)) = answer(true, decline);
        let after = threatened(&m, B, A).unwrap();
        let old = before.map_or(0, |f| f.value);
        assert_eq!((after.value, after.drift, after.limit, after.limited), ((old - 30).min(0), 3, 0, true), "decline {decline}");
        assert_eq!(threatened(&m, A, B), proposer_before, "decline {decline}");
        let (m, (before, proposer_before)) = answer(false, decline);
        assert_eq!((threatened(&m, B, A), threatened(&m, A, B)), (before, proposer_before), "decline {decline}");
    }
    let mut m = test_model();
    assert_eq!(m.apply(CampaignCommand::DeclineDeal), Err(CommandError::NoNegotiation));
}

/// Declining a threat twice adds the `threatened` event twice: the command (`0x00932F00`) and
/// `0x00C1F210` keep no record of an earlier answer, and `0x00B02CB0` caps the value only from
/// above (limit 0, drift 3), so −30 becomes −60.
#[test]
fn a_repeated_decline_adds_the_threat_again() {
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeDeal { threat: true }).unwrap();
    let old = m.world.relationships.get(&(B, A)).and_then(|r| r.factor("threatened").copied()).map_or(0, |f| f.value);
    m.apply(CampaignCommand::DeclineDeal).unwrap();
    m.apply(CampaignCommand::DeclineDeal).unwrap();
    let f = *m.world.relationships[&(B, A)].factor("threatened").unwrap();
    assert_eq!(f.value, (old - 30).min(0) - 30);
}

/// Accepted technologies (`0x00C18CF0`): the receiver researches each one it has at state 1..4
/// (`0x008CDCB0` → `0x008EED20`: progress = cost, no school, then availability), the giver's
/// traded count (+0x28, `0x008F3DD0`) goes up by one per technology handed over, granted or not.
#[test]
fn accepted_technologies_are_granted_and_counted() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    rules.technologies.insert("mil1".into(), TechRules { cost: 40, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin1_a".into(), state::RESEARCHED), ("mil1".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies = vec![("admin1_a".into(), state::AVAILABLE), ("mil1".into(), state::RESEARCHED)];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["mil1".into()], offered: vec!["admin1_a".into()] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.tech_state(B, "admin1_a"), Some(state::RESEARCHED));
    assert_eq!(m.world.faction_details[&B].research["admin1_a"].progress, 25.0);
    assert_eq!(m.tech_state(A, "mil1"), Some(state::RESEARCHED));
    assert_eq!(m.world.faction_details[&A].research["mil1"].progress, 40.0);
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].traded, 1);
    assert_eq!(m.world.faction_details[&B].research["mil1"].traded, 1);
    // A deal applies once: accepted again (ProposeDeal then AcceptOffer), nothing changes; after
    // the deal is proposed anew it applies again (a granted technology is not granted twice, but
    // the giver's count goes up per hand-over, as 0x008F3DD0).
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].traded, 1);
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec![], offered: vec!["admin1_a".into()] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.faction_details[&A].research["admin1_a"].traded, 2);
    // A technology the receiver has no entry for is not granted.
    assert!(!m.grant_technology(C, "admin1_a"));
}

/// The technology record's value for the AI recipient (`0x00C13360` → `0x00A36B20`): each item
/// from the model's costs, holders and the proposer's traded counts, summed, × trunc(inflation).
#[test]
fn technology_deal_value_reads_the_model() {
    use super::deal_value::{technology_value, DealEvaluation};
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A];
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("admin1_a".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    rules.technologies.insert("mil1".into(), TechRules { cost: 40, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("admin1_a".into(), state::RESEARCHED), ("mil1".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies = vec![("admin1_a".into(), state::AVAILABLE), ("mil1".into(), state::RESEARCHED)];
    m.world.faction_details.entry(A).or_default().research.entry("admin1_a".into()).or_default().traded = 1;
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["mil1".into()], offered: vec!["admin1_a".into()] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    // One holder each (doubled); the offered one divided by (1 + 1)²; inflation 2.5 counts as 2.
    let v = m.technology_deal_value(&n, 2.5);
    let gain = technology_value(25, 1, 1, true, true).gain;
    let cost = technology_value(40, 1, 0, false, true).cost;
    assert_eq!((gain, cost), ((500 + 344) * 2 / 4, (500 + 578) * 2));
    assert_eq!((v.gain, v.cost, v.given), (gain * 2, cost * 2, cost * 2));
    // A dear technology demanded for a cheap one: not fair.
    let mut e = DealEvaluation::default();
    e.add(5, v, 0.0, 1.0);
    assert!(!e.fair());
}

/// The payment record (`CCQ_DIPLOMACY_PROPOSE_PAYMENT`, executor `0x00933AD0`): an offered one-off
/// payment is capped at the proposer's treasury when it is proposed (`0x00C4AB20`), not again when the
/// deal is accepted; the applier (`0x00C18A70`) has the recipient pay a demand and the proposer an
/// offer, once over 1 turn, else as a regular payment.
#[test]
fn a_proposed_payment_is_capped_when_proposed_and_paid_on_accept() {
    use super::details::RegularPayment;
    use super::negotiation::DealPayment;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    let propose = |m: &mut CampaignModel, amount, turns| m.apply(CampaignCommand::ProposePayment { clear: false, amount, turns }).unwrap();
    let treasuries = |m: &CampaignModel| (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
    assert_eq!(m.apply(CampaignCommand::ProposePayment { clear: false, amount: 10, turns: 1 }), Err(CommandError::NoNegotiation));
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    // A offers 5000 once with 1000: the record holds −1000.
    propose(&mut m, -5000, 1);
    assert_eq!(m.negotiations.current.as_ref().unwrap().payment, DealPayment { amount: -1000, turns: 1 });
    // The treasury falls before the accept: the stored amount is paid as it is.
    m.world.factions.get_mut(&A).unwrap().treasury = 200;
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(treasuries(&m), (-800, 2000));
    // A demands 300 once: B pays A.
    propose(&mut m, 300, 1);
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(treasuries(&m), (-500, 1700));
    // A demands 50 a turn for 5 turns: a regular payment of B to A; nothing moves now.
    propose(&mut m, 50, 5);
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(treasuries(&m), (-500, 1700));
    assert_eq!(m.world.relationships[&(B, A)].payments, vec![RegularPayment { amount: 50, turns: 5 }]);
    // Clearing empties the record; i32::MIN is refused.
    m.apply(CampaignCommand::ProposePayment { clear: true, amount: 0, turns: 0 }).unwrap();
    assert!(!m.negotiations.current.as_ref().unwrap().payment.has_items());
    propose(&mut m, 70, 1);
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert!(!m.negotiations.current.as_ref().unwrap().payment.has_items());
    assert!(m.apply(CampaignCommand::ProposePayment { clear: false, amount: i32::MIN, turns: 1 }).is_err());
}

/// A deal whose payment fails is not applied at all: no region moves, no money, and the deal stays
/// unapplied (a later accept of a valid deal still applies it).
#[test]
fn a_deal_with_a_failing_payment_applies_nothing() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("mil1".into(), TechRules { cost: 40, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("mil1".into(), state::AVAILABLE)];
    let owner = m.world.regions[&RegionId(11)].owner;
    assert_ne!(owner, A);
    let treasury = m.world.factions[&A].treasury;
    // A negotiation of A with itself: the payment's faction pair is refused.
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: A }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec![], offered: vec!["mil1".into()] }).unwrap();
    m.apply(CampaignCommand::ProposePayment { clear: false, amount: 300, turns: 1 }).unwrap();
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::SameFaction(A)));
    assert_eq!(m.world.regions[&RegionId(11)].owner, owner);
    assert_eq!(m.tech_state(A, "mil1"), Some(state::AVAILABLE));
    assert!(!m.world.faction_details[&A].research.contains_key("mil1"));
    assert_eq!(m.world.factions[&A].treasury, treasury);
    assert!(!m.negotiations.current.as_ref().unwrap().applied);
    // Without the payment the same regions and technology records apply.
    m.apply(CampaignCommand::ProposePayment { clear: true, amount: 0, turns: 0 }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert_eq!(m.tech_state(A, "mil1"), Some(state::RESEARCHED));
    assert_eq!(m.world.faction_details[&A].research["mil1"].traded, 1);
}

/// The payment record's value for the AI recipient (`0x00C12EE0`) and its part in the answer
/// (`0x00AA5ED0`: weight 0, slot 2 is the payment the accept tests read).
#[test]
fn the_ai_values_a_payment_as_the_exe() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let value = |m: &mut CampaignModel, amount, turns| {
        m.apply(CampaignCommand::ProposePayment { clear: false, amount, turns }).unwrap();
        let v = m.payment_deal_value(m.negotiations.current.as_ref().unwrap());
        (v.gain, v.cost, v.given)
    };
    // Once: the AI gains an offer, gives a demand.
    assert_eq!(value(&mut m, -700, 1), (700, 0, 0));
    assert_eq!(value(&mut m, 400, 1), (0, 400, 400));
    // Regular: worth (0.9^t − 1) × −10 payments, at most 0.8 × the payer's treasury and future net
    // income. B (1000, no history) pays 105 for 2 turns: 1.9 × 105 = 199.5 → 199; 1000 for 3 turns:
    // 2710 capped at 800; given = amount × turns.
    assert_eq!(value(&mut m, 105, 2), (0, 199, 210));
    assert_eq!(value(&mut m, 1000, 3), (0, 800, 3000));
    // An offer is capped by the proposer's means: A has 0 and a net income of 60 a turn.
    m.world.factions.get_mut(&A).unwrap().treasury = 0;
    let mut record = [0; 25];
    (record[5], record[18]) = (100, 40);
    m.world.economy_history.insert(A, vec![record]);
    assert_eq!(value(&mut m, -1000, 3), (144, 0, 0));
    // A one-off offer from an empty treasury is capped to nothing: no record, no AI answer.
    value(&mut m, -100, 1);
    assert_eq!(m.ai_accepts_deal(), None);
    // The answer: an offered gift is accepted; a demand for nothing is not.
    m.world.factions.get_mut(&A).unwrap().treasury = 1000;
    value(&mut m, -100, 1);
    assert_eq!(m.ai_accepts_deal(), Some(true));
    value(&mut m, 500, 1);
    assert_eq!(m.ai_accepts_deal(), Some(false));
    assert!(m.apply(CampaignCommand::AcceptDeal).is_err());
}

/// The technology goals of the AI's two goal lists (`0x008F4F10`): the giver has it researched and
/// the receiver has it in state 1, 2 or 3; the weight of one the AI gives follows its spread.
#[test]
fn technology_goals_follow_the_research_states() {
    use super::research::state;
    let mut m = test_model();
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("t1".into(), state::RESEARCHED), ("t2".into(), state::RESEARCHED), ("t3".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies =
        vec![("t1".into(), state::AVAILABLE), ("t2".into(), state::UNAVAILABLE), ("t3".into(), state::RESEARCHED)];
    assert!(m.is_technology_goal("t1", A, B), "researched by the giver, available to the receiver");
    assert!(!m.is_technology_goal("t2", A, B), "not yet available to the receiver (state 4)");
    assert!(!m.is_technology_goal("t1", B, A), "the giver must have it researched");
    assert!(m.is_technology_goal("t3", B, A));
    // Fewer than 8 factions: t = 1, so any holder makes the spread 1 and the weight −0.5.
    assert_eq!(m.given_technology_goal_weight("t1"), -0.5);
    assert_eq!(m.given_technology_goal_weight("nobody_has_it"), -1.0);
}

/// The AI evaluates a deal of technologies as the exe's `0x00AA5ED0` (AI_RESEARCH.md §4): value,
/// goal weights, `fair` / `good`, and the `diplomacy_options` refusal.
#[test]
fn the_ai_evaluates_technology_deals() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A];
    let mut rules = (*m.rules).clone();
    let tech = |cost| TechRules { cost, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() };
    rules.technologies.insert("admin1_a".into(), tech(1000));
    rules.technologies.insert("military1_b".into(), tech(1000));
    rules.technologies.insert("economy1_c".into(), tech(100));
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies =
        vec![("admin1_a".into(), state::RESEARCHED), ("military1_b".into(), state::AVAILABLE), ("economy1_c".into(), state::RESEARCHED)];
    m.world.faction_details.entry(B).or_default().technologies =
        vec![("admin1_a".into(), state::AVAILABLE), ("military1_b".into(), state::RESEARCHED), ("economy1_c".into(), state::AVAILABLE)];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    // Nothing proposed: nothing to answer.
    assert_eq!(m.ai_accepts_deal(), None);
    // An even trade (both worth 20452 × 2, one holder each): the AI's need weight for the admin
    // technology (≤ 0.5 × 500) and the spread weight of the one it gives (−0.5 × 500, one holder of
    // two factions) leave it fair.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["military1_b".into()], offered: vec!["admin1_a".into()] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(true));
    assert!(!m.ai_refuses_deal());
    // A cheap technology for a dear one: refused.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["military1_b".into()], offered: vec!["economy1_c".into()] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(false));
    assert!(m.ai_refuses_deal());
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::DealRefused));
    // A gift is accepted.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec![], offered: vec!["economy1_c".into()] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(true));
    // The AI's diplomacy options forbid accepting technology deals from A (1 or 3): refused.
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[5] = 1;
    assert_eq!(m.ai_accepts_deal(), Some(false));
    // A human recipient answers for itself.
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[5] = 0;
    m.turn.humans = vec![A, B];
    assert_eq!(m.ai_accepts_deal(), None);
}

/// `force_diplomacy` permissions have one home, the model's relationship (`0x00B28670`): set for
/// one direction only, refused for the same faction (the original's `0x00B64C50` miss bug), an
/// unknown faction or option, or a pair with no relationship (never created there); the
/// negotiation panel and the deal rules read them back.
#[test]
fn diplomacy_permissions_live_in_the_model() {
    use super::negotiation::NegotiationAction as N;
    let mut m = test_model();
    let (peace, war) = (N::Peace.option(), N::War.option());
    m.relationship_mut(A, B);
    m.relationship_mut(B, A);
    assert!(!m.world.relationships.contains_key(&(A, C)));
    assert!(!m.set_diplomacy_option(A, C, war, false, false), "no relationship: refused");
    assert!(!m.world.relationships.contains_key(&(A, C)), "and none created");
    assert!(m.may_propose(A, B, war) && m.may_accept(A, B, war), "nothing stored: allowed");
    assert!(m.set_diplomacy_option(A, B, war, false, true));
    assert_eq!((m.diplomacy_option(A, B, war), m.diplomacy_option(B, A, war)), (2, 0));
    assert!(!m.may_propose(A, B, war) && m.may_accept(A, B, war));
    assert!(m.negotiation_actions(A, B).iter().any(|r| r.action == N::War && r.forbidden));
    assert!(m.negotiation_actions(B, A).iter().all(|r| !r.forbidden), "the other direction is untouched");
    assert!(!m.deal_goal_allowed(A, B, war) && m.deal_goal_allowed(B, A, war));
    assert!(m.set_diplomacy_option(B, A, peace, true, false));
    assert!(!m.may_accept(B, A, peace) && !m.deal_goal_allowed(A, B, peace));
    assert!(!m.set_diplomacy_option(A, A, war, false, false));
    assert!(!m.set_diplomacy_option(A, FactionId(999), war, false, false));
    assert!(!m.set_diplomacy_option(A, B, 14, false, false));
    assert!(!m.world.relationships.contains_key(&(A, A)));
    assert!(m.may_propose(A, B, 14), "an index past the table is allowed");
}

/// The research need from the model: enemies / allies, forts and forces (`0x00ABB340`).
#[test]
fn the_research_need_reads_the_model() {
    let mut m = test_model();
    // No history (16), no enemies or allies (M = 1, D = 1), p1 / p2 from B's forces.
    let v = m.research_need(B);
    let land = m.world.forces.values().filter(|f| f.faction == B && !f.is_navy && !f.units.is_empty()).count() as u32;
    let naval = m.world.forces.values().filter(|f| f.faction == B && f.is_navy && !f.units.is_empty()).count() as u32;
    let ports = m.world.regions.values().filter(|r| r.owner == B).map(|r| r.slots.iter().filter(|s| s.port).count() as u32).sum::<u32>();
    let p1 = deal_value::need_power(land >> 2);
    let p2 = deal_value::need_power((ports >> 1) + (naval >> 2));
    assert_eq!(v, deal_value::research_need_values(1, 1, 16, p1, p2));
    // At war with A: one enemy → M = 2.
    m.world.set_stance(A, B, Stance::War).unwrap();
    let v = m.research_need(B);
    assert_eq!(v, deal_value::research_need_values(2, 1, 16, p1, p2));
}

/// `0x00ABAFB0`: the rebels are on every enemy list, and the rebels' own list holds every faction,
/// even one out of the game (the rebel test comes before the out-of-game skip); the ally list holds
/// neither. The research need counts these lists (`0x00ABE100` / `0x00A98460`).
#[test]
fn the_rebels_are_on_every_enemy_list() {
    use super::director::{believed_ally_count, believed_enemy_count};
    let r = FactionId(9);
    let mut m = test_model();
    m.world.set_stance(A, B, Stance::Allied).unwrap();
    let (enemies, allies) = (believed_enemy_count(&m, B), believed_ally_count(&m, B));
    assert_eq!(allies, 1);
    m.world.factions.insert(r, faction(r, "", 0, GovernmentType::Republic));
    assert!(m.is_rebel_faction(r));
    assert_eq!(believed_enemy_count(&m, B), enemies + 1, "the rebels are B's enemy at any stance");
    assert_eq!(believed_ally_count(&m, B), allies);
    let others = m.world.factions.len() as u32 - 1;
    assert_eq!(believed_enemy_count(&m, r), others, "every faction is the rebels' enemy");
    m.world.faction_details.entry(C).or_default().out_of_game = Some(true);
    assert!(!m.in_the_game(C));
    assert_eq!(believed_enemy_count(&m, r), others, "even one out of the game");
    assert_eq!(believed_ally_count(&m, r), 0);
    m.world.set_stance(A, C, Stance::War).unwrap();
    assert_eq!(believed_enemy_count(&m, A), believed_enemy_count(&m, B), "C out of the game is no enemy of A");
}

/// The AI evaluates a deal of regions as the exe's `0x00C131C0` → `0x00A364B0` → `0x00AA1E90`
/// (AI_RESEARCH.md §4 "Region value"): the region goals (−1 × 2500 each), the worth of each
/// region to each side, the human-demand factor and its counter, and the `diplomacy_options`
/// refusal. Bases from the formula: region 10 (gdp 4000) 35000, 11 (8000) 55000, 12 (2000)
/// 25000.
#[test]
fn the_ai_evaluates_region_deals() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.region_neighbours =
        [(RegionId(10), vec![RegionId(11)]), (RegionId(11), vec![RegionId(10), RegionId(12)]), (RegionId(12), vec![RegionId(11)])].into();
    // Regions 10 and 12 hold only B's state religion: B's attitude to them is their class order
    // (1 and 0, `0x00A79050`), so an offer of either counts.
    share_religion(&mut m, B, &[RegionId(10), RegionId(12)]);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    assert_eq!(m.ai_accepts_deal(), None, "an empty deal");
    // A gift bordering the AI: worth 25000 to B (no slots, B holds a region), against the region
    // goal's −2500: accepted.
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![], offered: vec![RegionId(12)] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 25000, cost: 0, given: 0 });
    assert_eq!(m.ai_accepts_deal(), Some(true));
    // A gift that borders none of the AI's regions counts too (attitude ≥ 0 needs no border).
    let neighbours = std::mem::take(&mut m.world.region_neighbours);
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 25000, cost: 0, given: 0 });
    assert_eq!(m.ai_accepts_deal(), Some(true));
    m.world.region_neighbours = neighbours;
    // A demand for B's only region: 55000 to A (A has another region with slots); to B ×2 (its
    // only region in the theatre) and ×1.5 (its last with slots) = 165000. Refused.
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 0, cost: 55000, given: 165000 });
    assert_eq!(m.region_deal_value(&n, 2.5), deal_value::DealValue { gain: 0, cost: 110000, given: 330000 }, "inflation ×2");
    assert!(m.ai_refuses_deal());
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::DealRefused));
    assert_eq!(m.world.regions[&RegionId(11)].owner, B);
    // A swap that pays: region 10 stored at 200000, region 11 at 10000.
    m.world.region_base_values = [(RegionId(10), 200_000), (RegionId(11), 10_000)].into();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![RegionId(10)] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 200_000, cost: 10_000, given: 30_000 });
    // The AI's diplomacy options forbid region deals from A (1 or 3): refused.
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[4] = 3;
    assert_eq!(m.ai_accepts_deal(), Some(false));
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[4] = 0;
    assert_eq!(m.ai_accepts_deal(), Some(true));
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!((m.world.regions[&RegionId(10)].owner, m.world.regions[&RegionId(11)].owner), (B, A));
    // The human receiver counts its deal region (faction +0x938); the AI receiver does not.
    assert_eq!(m.world.deal_regions_received.get(&A), Some(&1));
    assert_eq!(m.world.deal_regions_received.get(&B), None);
    // So A's next demand counts m = 1 + 1: B's worth of region 10 (200000 × 2 × 1.5) × 1.5.
    m.apply(CampaignCommand::EndNegotiation).unwrap();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(10)], offered: vec![] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0), deal_value::DealValue { gain: 0, cost: 200_000, given: 900_000 });
    // An AI proposer's demands are not scaled.
    m.turn.humans = vec![];
    assert_eq!(m.region_deal_value(&n, 1.0).given, 600_000);
    // A human recipient answers for itself.
    m.turn.humans = vec![A, B];
    assert_eq!(m.ai_accepts_deal(), None);
    assert!(!m.ai_refuses_deal());
}

/// A deal of regions and technologies is evaluated as one sum (records 4 then 5).
#[test]
fn the_ai_evaluates_regions_and_technologies_together() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.region_neighbours = [(RegionId(12), vec![RegionId(11)]), (RegionId(11), vec![RegionId(12)])].into();
    share_religion(&mut m, B, &[RegionId(12)]);
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("military1_b".into(), TechRules { cost: 1000, building_level: "test_building_level".into(), requires: vec![], factions: vec![], ..Default::default() });
    m.rules = Arc::new(rules);
    m.world.faction_details.entry(A).or_default().technologies = vec![("military1_b".into(), state::AVAILABLE)];
    m.world.faction_details.entry(B).or_default().technologies = vec![("military1_b".into(), state::RESEARCHED)];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    // A technology worth 20452 × 2 (one holder) for nothing: refused.
    m.apply(CampaignCommand::ProposeTechnologies { clear: false, demanded: vec!["military1_b".into()], offered: vec![] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(false));
    // With region 12 (25000 to B, −2500 region goal, −250 technology goal) added, still short.
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![], offered: vec![RegionId(12)] }).unwrap();
    assert_eq!(m.ai_accepts_deal(), Some(false));
    // Region 12 stored at 60000: 1.05 × (60000 − 2500 − 250) against 40904 → accepted.
    m.world.region_base_values.insert(RegionId(12), 60_000);
    assert_eq!(m.ai_accepts_deal(), Some(true));
}

/// `test_model` with capitals for A (12, so its region 10 counts towards its sea cap) and B (11) and the route check answering `codes` per (from, to)
/// pair (0 for any other), for the trade agreement explanation (`0x00BC7910`).
fn trade_tooltip_model(codes: Vec<(FactionId, FactionId, u8)>) -> CampaignModel {
    let mut m = test_model();
    for (f, cap) in [(A, 12), (B, 11)] {
        m.world.faction_details.entry(f).or_default().capital = Some(RegionId(cap));
    }
    for (x, y) in [(A, B), (B, A)] {
        m.relationship_mut(x, y);
    }
    let mut rules = (*m.rules).clone();
    let maker: crate::seam::Maker<super::seams::TradeRouteCheck> = Arc::new(move |_| {
        let codes = codes.clone();
        Arc::new(move |_, a, b| codes.iter().find(|(x, y, _)| (*x, *y) == (a, b)).map_or(0, |c| c.2))
    });
    rules.seams.trade_route_check.apply("test_mod", &maker);
    m.rules = Arc::new(rules);
    m
}

/// `0x00BC7910`'s reason (`0x00BC7BB1`, CONFIRMED disassembly): an embargo either way, then the other
/// side's route back, then the asking side's code; no capital on either side gives no text.
#[test]
fn trade_agreement_tooltip_reasons_follow_the_route_codes() {
    use super::negotiation::*;
    let reason = |m: &CampaignModel| m.trade_agreement_tooltip(A, B, false).map(|t| (t.reason, t.route_shifted));
    // B cannot reach A: "no route them" whatever A's code.
    assert_eq!(reason(&trade_tooltip_model(vec![(A, B, 3)])), Some((TRADE_TIP_NO_ROUTE_THEM, false)));
    for (code, want) in [
        (0, (TRADE_TIP_NO_ROUTE_YOU, false)),
        (1, (TRADE_TIP_NEIGHBOUR, false)),
        (2, (TRADE_TIP_SEA_ROUTES, false)),
        (3, (TRADE_TIP_SEA_ROUTES, false)),
        (4, (TRADE_TIP_SEA_ROUTES, true)),
        (5, (TRADE_TIP_SEA_ROUTES, false)),
        (6, (TRADE_TIP_SEA_ROUTES, true)),
        (7, (TRADE_TIP_SEA_ROUTES, false)),
        (9, (TRADE_TIP_NO_ROUTE_THEM, false)),
    ] {
        assert_eq!(reason(&trade_tooltip_model(vec![(B, A, 1), (A, B, code)])), Some(want), "code {code}");
    }
    let mut m = trade_tooltip_model(vec![(B, A, 1), (A, B, 1)]);
    m.relationship_mut(B, A).trade_embargo_turns = 2;
    assert_eq!(reason(&m), Some((TRADE_TIP_EMBARGOED, false)), "an embargo either way comes first");
    m.relationship_mut(B, A).trade_embargo_turns = 0;
    m.world.faction_details.get_mut(&B).unwrap().capital = None;
    assert_eq!(m.trade_agreement_tooltip(A, B, true), None, "no capital: the empty text");
}

/// The counts (`0x00BC7910` with the flag): each side's sea route cap less its export routes with a sea
/// hop, then the cap; a land-only route is not counted.
#[test]
fn trade_agreement_tooltip_counts_sea_routes_left() {
    use super::trade::{TradePath, TradeWaypoint};
    let mut m = trade_tooltip_model(vec![(B, A, 1), (A, B, 1)]);
    edit_test_building_effects(&mut m, |e| e.push(("trade_routes_mod_max_sea".into(), 2.0)));
    let wp = |region: u32, sea: bool| TradeWaypoint { region: RegionId(region), from: 0, to: 0, sea, from_pos: None, to_pos: None };
    let path = |sea: bool| TradePath { waypoints: vec![wp(10, sea), wp(11, false)], ..Default::default() };
    m.world.trade_paths.insert((A, B), vec![path(true), path(false)]);
    let cap = |f| m.sea_route_cap(f) + m.capital_port_route_cap(f);
    let (ca, cb) = (cap(A), cap(B));
    assert!(ca > 0, "the test effect gives A a sea cap");
    let t = m.trade_agreement_tooltip(A, B, true).expect("both capitals");
    assert_eq!(t.sea_routes, Some([ca - 1, ca, cb, cb]));
    assert_eq!(m.trade_agreement_tooltip(A, B, false).unwrap().sea_routes, None);
}

/// The diplomacy screen's explanation (`0x009F35CC`, CONFIRMED): only for a new agreement the scripts
/// allow, never at war or while one is held; the negotiation panel's trade button always carries it
/// with the counts (`0x009B55FF`).
#[test]
fn diplomacy_trade_tooltip_only_for_an_allowed_new_agreement() {
    use super::negotiation::*;
    let mut m = trade_tooltip_model(vec![(B, A, 1), (A, B, 1)]);
    let neighbour = Some(TradeAgreementTooltip { reason: TRADE_TIP_NEIGHBOUR, sea_routes: None, route_shifted: false });
    assert_eq!(m.diplomacy_trade_tooltip(A, B), neighbour);
    let trade = NegotiationAction::Trade.option();
    m.relationship_mut(A, B).diplomacy_options[trade] = 2;
    assert_eq!(m.diplomacy_trade_tooltip(A, B), None, "the player may not propose");
    m.relationship_mut(A, B).diplomacy_options[trade] = 0;
    m.relationship_mut(B, A).diplomacy_options[trade] = 3;
    assert_eq!(m.diplomacy_trade_tooltip(A, B), None, "the other side neither proposes nor accepts");
    m.relationship_mut(B, A).diplomacy_options[trade] = 2;
    assert_eq!(m.diplomacy_trade_tooltip(A, B), neighbour, "the other side's 2 still allows it");
    m.relationship_mut(A, B).trade_agreement = true;
    assert_eq!(m.diplomacy_trade_tooltip(A, B), None, "held");
    m.relationship_mut(A, B).trade_agreement = false;
    let button = m.possible_actions(A, A, B).unilaterals.into_iter().find(|a| a.action == NegotiationAction::Trade);
    let with_counts = m.trade_agreement_tooltip(A, B, true);
    assert!(with_counts.is_some_and(|t| t.sea_routes.is_some()));
    assert_eq!(button.map(|a| a.tooltip), Some(Some(ActionTooltip::TradeAgreement(with_counts))));
    m.world.set_stance(A, B, Stance::War).unwrap();
    assert_eq!(m.diplomacy_trade_tooltip(A, B), None, "at war");
}

/// The join-war record (11): `CCQ_DIPLOMACY_PROPOSE_JOIN_WAR` (`0x009336C0`) replaces its two lists,
/// leaves it on two empty ones and empties it on `clear`, as the regions record; an unknown faction
/// aborts; `ClearNegotiation` empties it too.
#[test]
fn the_join_war_record_is_proposed_like_the_regions_record() {
    let mut m = test_model();
    let jw = |clear, demanded: &[FactionId], offered: &[FactionId]| CampaignCommand::ProposeJoinWar { clear, demanded: demanded.to_vec(), offered: offered.to_vec() };
    assert_eq!(m.apply(jw(false, &[C], &[])), Err(CommandError::NoNegotiation));
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    let items = |m: &CampaignModel| {
        let j = &m.negotiations.current.as_ref().unwrap().join_war;
        (j.demanded.clone(), j.offered.clone())
    };
    m.apply(jw(false, &[C], &[])).unwrap();
    assert_eq!(items(&m), (vec![C], vec![]));
    m.apply(jw(false, &[], &[])).unwrap();
    assert_eq!(items(&m), (vec![C], vec![]), "two empty lists leave the record");
    m.apply(jw(false, &[], &[C])).unwrap();
    assert_eq!(items(&m), (vec![], vec![C]), "replaced, both lists");
    assert_eq!(m.apply(jw(false, &[FactionId(77)], &[])), Err(CommandError::UnknownFaction(FactionId(77))));
    assert_eq!(items(&m), (vec![], vec![C]));
    m.apply(jw(true, &[C], &[C])).unwrap();
    assert_eq!(items(&m), (vec![], vec![]));
    m.apply(jw(false, &[C], &[C])).unwrap();
    m.apply(CampaignCommand::ClearNegotiation).unwrap();
    assert_eq!(items(&m), (vec![], vec![]));
}

/// Accepting a join-war item (`0x00C188F0`): the proposer declares war on each offered faction on the
/// recipient's behalf, then the recipient on each demanded one on the proposer's, each through
/// `0x00B26700` with its refresh: the war (`war_dragged_by_ally` both ways, the ally as #5, both war
/// flags), then `campmap_faction_joins_your_enemies` (0x14D, `0x009DAE40`) to the faction war is
/// declared on, before the next war. No allies called, no access saved for the war, no
/// third-party factors (B, hostile to C, gets no `war_against_enemies` towards A).
#[test]
fn a_join_war_deal_declares_each_war_on_the_partners_behalf() {
    use super::treaties::{event, slot};
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    m.relationship_mut(B, C).factor_mut("war").value = -200;
    assert_eq!(m.attitude_category(B, C), 0, "B hostile to C");
    for f in [A, B, C] {
        m.world.faction_details.entry(f).or_default();
    }
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![C] }).unwrap();
    let ev = m.apply(CampaignCommand::AcceptDeal).unwrap();
    let seen: Vec<String> = ev
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::StanceChanged { a, b, stance: Stance::War } => Some(format!("war {a:?} {b:?}")),
            CampaignEvent::Message { recipient, message: CampaignMessage::FactionJoinsYourEnemies { joiner } } => Some(format!("notice {recipient:?} {joiner:?}")),
            _ => None,
        })
        .collect();
    assert_eq!(seen, [format!("war {A:?} {C:?}"), format!("notice {C:?} {A:?}"), format!("war {B:?} {C:?}"), format!("notice {C:?} {B:?}")]);
    let dragged = event("war_dragged_by_ally");
    for (x, y) in [(A, C), (C, A), (B, C), (C, B)] {
        assert_eq!(m.world.relationships[&(x, y)].attitudes[slot("war")].value, dragged.value, "{x:?}->{y:?}");
    }
    assert_eq!((m.world.relationships[&(A, C)].war_ally, m.world.relationships[&(B, C)].war_ally), (Some(B), Some(A)));
    assert!(m.world.relationships.get(&(B, A)).is_none_or(|r| r.attitudes[slot("declared_war_against_enemies")].value == 0));
    for (x, y) in [(A, B), (B, A)] {
        assert!(m.world.relationships.get(&(x, y)).is_none_or(|r| r.allied_in_war_against.is_empty()));
    }
    for f in [A, B, C] {
        assert!(m.world.faction_details[&f].war_started, "{f:?}");
    }
    assert_eq!(CampaignMessage::FactionJoinsYourEnemies { joiner: A }.type_name(), "campmap_faction_joins_your_enemies");
    // Applied once.
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Ok(Vec::new()));
}

/// `0x00B26700` has no at-war test: a join-war item against a faction already at war with the joiner
/// applies the war's records again (the factor becomes `war_dragged_by_ally`, the ally #5) and posts
/// the notice, with no new stance change. An item naming its own joiner fails the whole deal before
/// anything is applied.
#[test]
fn a_join_war_item_against_an_existing_enemy_applies_again() {
    use super::treaties::{event, slot};
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    m.apply(CampaignCommand::DeclareWar { a: A, b: C }).unwrap();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![], offered: vec![C] }).unwrap();
    let ev = m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::StanceChanged { .. })));
    assert!(ev.contains(&CampaignEvent::Message { recipient: C, message: CampaignMessage::FactionJoinsYourEnemies { joiner: A } }));
    assert_eq!(m.world.relationships[&(A, C)].attitudes[slot("war")].value, event("war_dragged_by_ally").value);
    assert_eq!(m.world.relationships[&(A, C)].war_ally, Some(B));
    // B demanded to declare war on itself, after A's item on C: nothing applied.
    m.apply(CampaignCommand::ProposePayment { clear: false, amount: 100, turns: 1 }).unwrap();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![B], offered: vec![C] }).unwrap();
    let treasury = m.world.factions[&B].treasury;
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::SameFaction(B)));
    assert_eq!(m.world.factions[&B].treasury, treasury);
    assert!(!m.negotiations.current.as_ref().unwrap().applied);
}

/// A (human) proposes to B (AI): allied, A at war with C, B neutral with C (not at war, so C is
/// a war B could join).
fn join_war_model() -> CampaignModel {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.set_stance(A, B, Stance::Allied).unwrap();
    m.world.set_stance(A, C, Stance::War).unwrap();
    m.world.set_stance(B, C, Stance::Neutral).unwrap();
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m
}

/// The join-war record's goal weight as the evaluator reads it: B asked to join A's war on C.
fn asked_to_join_weight(m: &CampaignModel) -> f32 {
    let n = m.negotiations.current.as_ref().unwrap();
    m.join_war_record_weight(n, m.deal_goal_allowed(B, A, 11))
}

/// The join-war goals need record 11 allowed both ways (`0x00CCB150`), an alliance (`0x008CE390`)
/// and a war joinable either way (`0x00C1A6A0`): without them the record weighs nothing.
#[test]
fn join_war_goals_need_an_alliance_and_a_joinable_war() {
    let mut m = join_war_model();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![] }).unwrap();
    // B asked to join A's war on C: C is a list-2 target throughout the gate's closed cases, so the
    // zero comes from the gate, not from a missing goal.
    let targets = |m: &CampaignModel| (m.join_war_targets(B, A), m.join_war_targets(A, B));
    assert_eq!(targets(&m), (vec![], vec![C]));
    assert_eq!(asked_to_join_weight(&m), -0.25);
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[11] = 2;
    assert_eq!(targets(&m), (vec![], vec![C]));
    assert_eq!(asked_to_join_weight(&m), 0.0, "the AI may not propose join-war deals to A");
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[11] = 0;
    m.world.set_stance(A, B, Stance::Neutral).unwrap();
    assert_eq!(targets(&m), (vec![], vec![C]));
    assert_eq!(asked_to_join_weight(&m), 0.0, "not allied");
    m.world.set_stance(A, B, Stance::Allied).unwrap();
    m.world.set_stance(B, C, Stance::War).unwrap();
    assert!(!m.joinable_war(A, B) && !m.joinable_war(B, A), "the gate is closed");
    assert_eq!(targets(&m), (vec![], vec![]), "and no target is left");
    assert_eq!(asked_to_join_weight(&m), 0.0, "both at war with C: no war left to join");
}

/// `0x00C1E590`: the joiner must be free to fight the enemy: no patron, or its patron is the
/// enemy itself or at war with it.
#[test]
fn a_joinable_war_needs_the_joiners_patron_to_allow_it() {
    let d = FactionId(4);
    let mut m = join_war_model();
    m.world.factions.insert(d, faction(d, "test_faction_d", 0, GovernmentType::Republic));
    assert!(m.joinable_war(A, B), "B has no patron");
    m.world.set_stance(B, d, Stance::Protectorate).unwrap();
    assert_eq!(m.patron(B), Some(d));
    assert!(!m.joinable_war(A, B), "B's patron D is not at war with C");
    m.world.set_stance(d, C, Stance::War).unwrap();
    assert!(m.joinable_war(A, B), "D is at war with C");
    m.world.set_stance(d, C, Stance::Neutral).unwrap();
    m.world.set_stance(B, d, Stance::Neutral).unwrap();
    m.world.set_stance(B, C, Stance::Protectorate).unwrap();
    assert!(m.joinable_war(A, B), "B's patron is C itself (a plain equality)");
    assert!(!m.joinable_war(B, A), "B has no war");
}

/// `0x008FA020`: the first side's wars, not on the second side, that the second side is not
/// already at war with (its record's stance, `0x008FA0BE..0x008FA0C8`).
#[test]
fn join_war_targets_are_wars_the_other_side_is_not_in() {
    let mut m = join_war_model();
    assert_eq!(m.join_war_targets(A, B), [C]);
    assert_eq!(m.join_war_targets(B, A), []);
    m.world.set_stance(B, C, Stance::War).unwrap();
    assert_eq!(m.join_war_targets(A, B), [], "B is already at war with C");
    assert_eq!(m.join_war_targets(B, A), [], "and A is at war with C too");
    m.world.set_stance(B, C, Stance::Allied).unwrap();
    assert_eq!(m.join_war_targets(A, B), [C], "an ally of C may still join A's war on it");
    m.world.factions.get_mut(&B).unwrap().diplomacy.remove(&C);
    // The exe would test B's first, unrelated record (ORIGINAL BUG of 0x00B64C50, see
    // `join_war_targets`).
    assert_eq!(m.join_war_targets(A, B), [C], "a missing record reads as not at war");
}

/// `0x00CCB210` case 11 with `0x00D62DA0`: the attitude is the relation difference (the
/// stance-belief offsets read 0), capped by what the proposer is to the AI: patron −0.1, ally
/// −0.25, else (here: B is A's patron) −0.5; below −1 it is −1; a target without a CAI faction
/// is −1. Read through the record's goal weight, as the evaluator reads it.
#[test]
fn the_given_join_war_weight_is_capped_by_what_the_proposer_is() {
    use super::director::relation_belief_value;
    let mut m = join_war_model();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![] }).unwrap();
    let attitude = |m: &CampaignModel| relation_belief_value(m, B, A) - relation_belief_value(m, B, C);
    let w = attitude(&m);
    assert_eq!(m.join_war_attitude(B, A, C), w);
    assert!(w > -0.25, "an ally is related more closely than a neutral");
    assert!(m.believes_allied(B, A) && m.believed_patron(B).is_none());
    assert_eq!(asked_to_join_weight(&m), -0.25);
    m.world.set_stance(B, A, Stance::Protectorate).unwrap();
    assert_eq!(m.believed_patron(B), Some(A));
    assert_eq!(asked_to_join_weight(&m), deal_value::given_join_war_weight(attitude(&m), -0.1));
    m.world.set_stance(B, A, Stance::Patron).unwrap();
    assert!(!m.believes_allied(B, A) && m.believed_patron(B).is_none());
    assert_eq!(asked_to_join_weight(&m), deal_value::given_join_war_weight(attitude(&m), -0.5));
    // C has no CAI faction: −1, and the attitude is the offsets only.
    m.world.ai_factions = vec![A, B];
    assert_eq!(asked_to_join_weight(&m), -1.0);
    assert_eq!(m.join_war_attitude(B, A, C), 0.0);
}

/// The ally list is read from the stored relation belief while it is clean, like the enemy list:
/// both counts come from one snapshot.
#[test]
fn the_ally_list_is_read_from_the_stored_belief() {
    use super::director::{believed_ally_count, believed_enemy_count, build_relation_belief};
    let mut m = join_war_model();
    let belief = build_relation_belief(&m, B);
    assert_eq!(belief.allies, [A]);
    m.director.beliefs.insert(B, belief);
    // A change that does not mark the belief dirty: the stored lists still answer.
    m.world.factions.get_mut(&A).unwrap().diplomacy.insert(B, Stance::War);
    assert!(m.believes_allied(B, A));
    let stored_enemies = m.director.beliefs[&B].enemies.len() as u32;
    assert_eq!((believed_enemy_count(&m, B), believed_ally_count(&m, B)), (stored_enemies, 1));
    m.director.beliefs.remove(&B);
    assert!(!m.believes_allied(B, A));
    assert_eq!((believed_enemy_count(&m, B), believed_ally_count(&m, B)), (stored_enemies + 1, 0));
}

/// `0x00AB5400`: a land force near the target counts in full, any other a tenth, a navy halved
/// first; forces without units count nothing.
#[test]
fn the_war_strength_counts_near_land_forces_in_full() {
    let mut m = join_war_model();
    let force = m.world.forces[&ForceId(1000)].clone();
    let v = m.force_power(&force, &mut Default::default()) as u32;
    assert!(v > 0);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(11));
    assert_eq!(m.war_strength(A, B), v, "in B's region");
    assert_eq!(m.war_strength(A, C), v / 10, "C owns nothing");
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(10));
    assert_eq!(m.war_strength(A, B), v / 10, "no neighbours");
    m.world.region_neighbours = [(RegionId(10), vec![RegionId(11)])].into();
    assert_eq!(m.war_strength(A, B), v, "next to B's region");
    m.world.forces.get_mut(&ForceId(1000)).unwrap().is_navy = true;
    assert_eq!(m.war_strength(A, B), (v >> 1) / 10, "a navy is never near");
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.clear();
    assert_eq!(m.war_strength(A, B), 0);
}

/// `0x00C12D40`: an offered war is worth twice the proposer's scaled strength, an asked one costs
/// the AI's scaled strength and gives the enemy's strength + 1000; the factor is the AI's attitude
/// to the proposer (`0x00A8D250`: 2 at −100, 1 at 0, 0.5 at +100). ORIGINAL BUG fixed: the exe
/// truncates the factor before multiplying, so any attitude above 0 zeroed the value; ours keeps
/// the fraction.
#[test]
fn the_join_war_value_follows_the_war_strengths() {
    let mut m = join_war_model();
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = (1..=60).map(unit).collect();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![C] }).unwrap();
    let (sa, sb) = (m.war_strength(A, C), m.war_strength(B, C));
    assert!(sa > 0 && sb > 0);
    let value = |m: &CampaignModel| m.join_war_deal_value(m.negotiations.current.as_ref().unwrap(), 1.0);
    assert_eq!(value(&m), deal_value::DealValue { gain: 2 * sa, cost: sb, given: 1000 }, "attitude 0: factor 1");
    m.relationship_mut(B, A).factor_mut("war").value = -100;
    assert_eq!(m.world.relationships[&(B, A)].attitude_total(), -100);
    assert_eq!(value(&m), deal_value::DealValue { gain: 4 * sa, cost: 2 * sb, given: 1000 });
    m.relationship_mut(B, A).factor_mut("war").value = 50;
    // Factor 0.75 at +50 (the exe's truncation made it 0).
    let three_quarters = |v: u32| (f64::from(v) * 0.75).trunc() as u32;
    let expected = deal_value::DealValue { gain: 2 * three_quarters(sa), cost: three_quarters(sb), given: 1000 };
    assert!(expected.gain > 0, "A's strength is large enough to keep a fraction");
    assert_eq!(value(&m), expected);
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.join_war_deal_value(&n, 2.0), expected.inflated(2.0), "inflated");
}

/// The join-war record in the evaluator (`0x00AA5ED0`, after the payments): declined on
/// `diplomacy_options` 1 or 3, else weighed and valued like any record.
#[test]
fn the_ai_answers_a_join_war_deal_from_its_goals_and_value() {
    let mut m = join_war_model();
    // Asked to join A's war on C: a list-2 goal (−0.25 × 1000) and 1000 given: refused.
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.join_war_record_weight(&n, m.deal_goal_allowed(B, A, 11)), -0.25);
    assert_eq!(m.ai_accepts_deal(), Some(false));
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::DealRefused));
    assert_ne!(m.world.stance(B, C), Stance::War);
    // B's war on C, which A offers to join: a list-1 goal (−1 × 1000) against twice A's strength.
    m.world.set_stance(A, C, Stance::Neutral).unwrap();
    m.world.set_stance(B, C, Stance::War).unwrap();
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.world.characters.get_mut(&CharacterId(100)).unwrap().garrisoned_in = Some(RegionId(12));
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![], offered: vec![C] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.join_war_record_weight(&n, m.deal_goal_allowed(B, A, 11)), -1.0);
    let expected = |m: &CampaignModel| {
        let n = m.negotiations.current.as_ref().unwrap();
        let mut e = deal_value::DealEvaluation::default();
        e.add(11, m.join_war_deal_value(n, 1.0), m.join_war_record_weight(n, m.deal_goal_allowed(B, A, 11)), 1.0);
        e.fair() || e.good()
    };
    assert!(2 * m.war_strength(A, C) < 1000);
    assert!(!expected(&m));
    assert_eq!(m.ai_accepts_deal(), Some(false), "A is too weak to be worth the goal");
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = (1..=60).map(unit).collect();
    assert!(2 * m.war_strength(A, C) >= 1000);
    assert!(expected(&m));
    assert_eq!(m.ai_accepts_deal(), Some(true));
    // diplomacy_options 1 or 3: declined; 2 only takes the goal away (weight 0), still accepted.
    for (option, answer) in [(1, false), (3, false), (2, true)] {
        m.world.relationships.entry((B, A)).or_default().diplomacy_options[11] = option;
        assert_eq!(m.ai_accepts_deal(), Some(answer), "option {option}");
    }
    m.world.relationships.entry((B, A)).or_default().diplomacy_options[11] = 0;
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.stance(A, C), Stance::War, "A joined B's war");
}

/// A deal applies whole or not at all: with a region, a payment and join-war items, one item naming
/// its own joiner (`SameFaction`, checked before any record) leaves every record unapplied (no
/// region moves, nothing is paid, no war); once the item is valid, every record applies, the
/// join-war wars after the region and the payment, none of them failing part-way.
#[test]
fn a_deal_with_an_invalid_join_war_item_applies_none_of_its_records() {
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    m.apply(CampaignCommand::ProposePayment { clear: false, amount: -100, turns: 1 }).unwrap();
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![B], offered: vec![C] }).unwrap();
    let treasuries = |m: &CampaignModel| (m.world.factions[&A].treasury, m.world.factions[&B].treasury);
    let before = treasuries(&m);
    assert_eq!(m.apply(CampaignCommand::AcceptDeal), Err(CommandError::SameFaction(B)));
    assert_eq!(m.world.regions[&RegionId(11)].owner, B);
    assert_eq!(treasuries(&m), before);
    assert_ne!(m.world.stance(A, C), Stance::War, "the valid item before the bad one is not applied either");
    assert!(!m.negotiations.current.as_ref().unwrap().applied);
    m.apply(CampaignCommand::ProposeJoinWar { clear: false, demanded: vec![C], offered: vec![C] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert_ne!(treasuries(&m), before);
    assert_eq!((m.world.stance(A, C), m.world.stance(B, C)), (Stance::War, Stance::War));
    assert!(m.negotiations.current.as_ref().unwrap().applied);
}

/// Gives `faction` the state religion `rel_b` and `regions` that religion as their whole breakdown,
/// so `faction`'s hypothetical-order attitude to them (`0x00A79050`) is their class order.
fn share_religion(m: &mut CampaignModel, faction: FactionId, regions: &[RegionId]) {
    m.world.faction_details.entry(faction).or_default().religion = "rel_b".into();
    for r in regions {
        m.world.regions.get_mut(r).unwrap().religions = vec![("rel_b".into(), 1.0)];
    }
}

/// A population class's order under another faction (`0x008BDB90`): the total, its negative
/// resistance (slot 10) again, without its negative tax and its garrison, the gentlemen taken for the new side.
#[test]
fn a_class_order_under_another_faction() {
    use super::economy::{hypothetical_class_order, ClassPublicOrder};
    let mut c = ClassPublicOrder { class: "lower".into(), happiness: [0; 13], repression: [0; 6] };
    // Government 5, tax −3, resistance −2, the owner's gentlemen 1; government buildings 2, garrison 4.
    (c.happiness[0], c.happiness[1], c.happiness[10], c.happiness[11]) = (5, -3, -2, 1);
    (c.repression[1], c.repression[4]) = (2, 4);
    assert_eq!(c.total(), 7);
    // 5 + 2 − 2 × 2, the owner's gentleman gone: 3.
    assert_eq!(hypothetical_class_order(&c, 1, 0), 3);
    // Two gentlemen on the new side's terms: 5.
    assert_eq!(hypothetical_class_order(&c, 1, 2), 5);
    // Positive tax and resistance stay as they are.
    let mut p = ClassPublicOrder { class: "lower".into(), happiness: [0; 13], repression: [0; 6] };
    (p.happiness[1], p.happiness[10]) = (2, 3);
    assert_eq!(hypothetical_class_order(&p, 0, 0), 5);
}

/// The AI's attitude to holding an offered region (`0x00A79050`): the lower class order less
/// `floor((1 − share) × 6)` of its state religion's (normalised) share, or 6 without one; the deal
/// valuation (`0x00A364B0`) counts the region at 0 or more, or at −5 or more when it borders the AI.
#[test]
fn the_ai_counts_an_offered_region_by_its_attitude() {
    use super::economy::hypothetical_order_attitude;
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.region_neighbours = [(RegionId(12), vec![RegionId(11)]), (RegionId(11), vec![RegionId(12)])].into();
    let attitude = |m: &CampaignModel, r: u32| hypothetical_order_attitude(m, &m.world.regions[&RegionId(r)], B);
    // B has no state religion: −6.
    assert_eq!(attitude(&m, 12), -6);
    share_religion(&mut m, B, &[RegionId(10), RegionId(12)]);
    // Region 10's classes have culture +1, region 12's nothing.
    assert_eq!((attitude(&m, 10), attitude(&m, 12)), (1, 0));
    let set = |m: &mut CampaignModel, shares: &[(&str, f32)]| {
        m.world.regions.get_mut(&RegionId(12)).unwrap().religions = shares.iter().map(|&(k, s)| (k.to_string(), s)).collect();
    };
    set(&mut m, &[("rel_a", 0.3), ("rel_b", 0.7)]);
    assert_eq!(attitude(&m, 12), -1, "floor(0.3 × 6)");
    set(&mut m, &[("rel_a", 0.25), ("rel_b", 0.25)]);
    assert_eq!(attitude(&m, 12), -3, "normalised to a half");
    set(&mut m, &[("rel_a", 1.0), ("rel_b", 0.0004)]);
    assert_eq!(attitude(&m, 12), -6, "a share under 0.0005 is dropped");
    set(&mut m, &[("rel_a", 1.0)]);
    assert_eq!(attitude(&m, 12), -6, "not held");
    // At −3 the offer counts only while region 12 borders B's region 11.
    set(&mut m, &[("rel_a", 0.5), ("rel_b", 0.5)]);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![], offered: vec![RegionId(12)] }).unwrap();
    let n = m.negotiations.current.clone().unwrap();
    assert_eq!(m.region_deal_value(&n, 1.0).gain, 25000);
    let neighbours = std::mem::take(&mut m.world.region_neighbours);
    assert_eq!(m.region_deal_value(&n, 1.0).gain, 0);
    m.world.region_neighbours = neighbours;
    // At −6 it never counts.
    set(&mut m, &[("rel_a", 1.0)]);
    assert_eq!(m.region_deal_value(&n, 1.0).gain, 0);
    assert_eq!(m.ai_accepts_deal(), Some(false));
}

/// `0x008BDB90` subtracts the owner's gentlemen with its own `0x008C55D0(region, owner)` call
/// (`0x008BDBB4`), not the class's gentlemen slot: a region whose first class its government does
/// not govern (the fallback, e.g. a monarchy's region a republic took) still loses them.
#[test]
fn the_fallback_class_still_loses_the_owners_gentlemen() {
    use super::economy::{gentlemen_factor, gentlemen_factor_for, hypothetical_order_attitude};
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.variables.insert("gentleman_happiness_positive_limit".into(), 6.0);
    rules.variables.insert("gentleman_happiness_negative_limit".into(), -8.0);
    m.rules = Arc::new(rules);
    let r = RegionId(10);
    let owner = m.world.regions[&r].owner;
    assert!(owner != B && !m.allied_with(owner, B));
    share_religion(&mut m, B, &[r]);
    m.world.regions.get_mut(&r).unwrap().class_bases = vec![PopulationClassState::new("clergy")];
    // The ungoverned class has every slot 0.
    assert_eq!(hypothetical_order_attitude(&m, &m.world.regions[&r], B), 0);
    // The owner's gentleman resident in the settlement, rank 2: +2 for the owner, −2 for B.
    let mut g = character(106, owner, CharacterKind::Gentleman, 10);
    g.garrisoned_in = Some(r);
    m.world.characters.insert(g.id, g);
    m.world.character_details.entry(CharacterId(106)).or_default().attributes = vec![("research".into(), 2)];
    let reg = &m.world.regions[&r];
    assert_eq!((gentlemen_factor(&m, reg), gentlemen_factor_for(&m, reg, B)), (2.0, -2.0));
    // 0 − 2 (owner's) + (−2) (B's).
    assert_eq!(hypothetical_order_attitude(&m, reg, B), -4);
}

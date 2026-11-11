//! Campaign model tests: the AI's region groups and their change states ([`super::region_groups`],
//! flood `0x00A68120`, owner state `0x00A734B0` / split test `0x00A73210`, other faction
//! `0x00A73660`) and the evaluating AI's personality multipliers in a deal's region worth
//! (`0x00AA1E90`), on MADE-UP regions.

use super::*;
use super::personalities::AiPersonalities;
use super::region_value::{compound, GroupChange};

/// [`test_model`] with only regions `owners` (id, owner) and the two-way links `links`.
fn model(owners: &[(u32, FactionId)], links: &[(u32, u32)]) -> CampaignModel {
    let mut m = test_model();
    m.world.regions = owners.iter().map(|&(id, f)| (RegionId(id), region(id, f, 1000, 0))).collect();
    let mut n: BTreeMap<RegionId, Vec<RegionId>> = BTreeMap::new();
    for &(a, b) in links {
        n.entry(RegionId(a)).or_default().push(RegionId(b));
        n.entry(RegionId(b)).or_default().push(RegionId(a));
    }
    m.world.region_neighbours = n;
    m
}

fn change(m: &CampaignModel, r: u32, f: FactionId) -> GroupChange {
    m.region_groups().change(RegionId(r), f)
}

#[test]
fn the_owner_reads_lost_reduced_or_split() {
    // A: 1 alone; 2-3; the line 4-5-6; the triangle 7-8-9. B: 10 between 3 and 4.
    let m = model(
        &[(1, A), (2, A), (3, A), (4, A), (5, A), (6, A), (7, A), (8, A), (9, A), (10, B)],
        &[(2, 3), (3, 10), (10, 4), (4, 5), (5, 6), (7, 8), (8, 9), (9, 7)],
    );
    assert_eq!(change(&m, 1, A), GroupChange::Lost);
    assert_eq!(change(&m, 2, A), GroupChange::Reduced, "a group of two");
    assert_eq!(change(&m, 5, A), GroupChange::Split, "the line's middle cuts it in two");
    assert_eq!(change(&m, 4, A), GroupChange::Reduced, "the line's end");
    assert_eq!(change(&m, 8, A), GroupChange::Reduced, "the triangle stays joined");
    assert_eq!(change(&m, 10, B), GroupChange::Lost);
}

#[test]
fn another_faction_reads_new_increased_or_merged() {
    // B's 10 borders A's groups {1} and {3, 4} (twice: both its members); B's 11 borders {3, 4}
    // only; B's 12 borders none of A's; 13 is B's, beside 12 in the same group.
    let m = model(
        &[(1, A), (3, A), (4, A), (10, B), (11, B), (12, B), (13, B)],
        &[(1, 10), (10, 3), (10, 4), (3, 4), (4, 11), (12, 13)],
    );
    assert_eq!(change(&m, 10, A), GroupChange::Merged);
    assert_eq!(change(&m, 11, A), GroupChange::Increased);
    assert_eq!(change(&m, 12, A), GroupChange::New);
    assert_eq!(change(&m, 12, C), GroupChange::New, "a faction with no region");
    assert_eq!(change(&m, 99, A), GroupChange::New, "an unknown region");
}

#[test]
fn rebels_and_land_masses_split_groups() {
    let mut m = model(&[(1, A), (2, A), (3, A), (4, C), (5, C)], &[(1, 2), (2, 3), (4, 5)]);
    let rebels = FactionId(9);
    m.world.factions.insert(rebels, faction(rebels, "", 0, GovernmentType::Republic));
    assert!(m.is_rebel_faction(rebels));
    for r in [4, 5] {
        m.world.regions.get_mut(&RegionId(r)).unwrap().owner = rebels;
    }
    assert_eq!(change(&m, 4, rebels), GroupChange::Lost, "every rebel region is a group of its own");
    assert_eq!(change(&m, 1, A), GroupChange::Reduced, "one land mass: 1-2-3");
    // 1 on land mass 7 only, 2 and 3 on 8 (2 on both): {1, 2, 3} still joins (1-2 share 7).
    m.world.region_land_masses = [(RegionId(1), vec![7]), (RegionId(2), vec![7, 8]), (RegionId(3), vec![8]), (RegionId(4), vec![]), (RegionId(5), vec![])].into();
    assert_eq!(change(&m, 2, A), GroupChange::Split);
    // 2 leaves land mass 7: {1} and {2, 3}.
    m.world.region_land_masses.insert(RegionId(2), vec![8]);
    assert_eq!(change(&m, 1, A), GroupChange::Lost);
    assert_eq!(change(&m, 2, A), GroupChange::Reduced);
    // A region without land masses (or missing from the list) joins no one.
    m.world.region_land_masses.remove(&RegionId(3));
    assert_eq!(change(&m, 2, A), GroupChange::Lost);
}

/// `0x00A73210` floods from the first link of the region its owner holds, even one outside the
/// group (another land mass): nothing of the group is removed and it reads SPLIT.
#[test]
fn the_split_test_starts_at_the_first_owned_link() {
    let mut m = model(&[(1, A), (2, A), (3, A), (4, A)], &[(2, 1), (2, 3), (3, 4)]);
    m.world.region_land_masses = [(RegionId(1), vec![5]), (RegionId(2), vec![6]), (RegionId(3), vec![6]), (RegionId(4), vec![6])].into();
    assert_eq!(change(&m, 2, A), GroupChange::Split, "the flood starts at 1, outside {{2, 3, 4}}");
    // Starting inside the group, the same shape reads REDUCED.
    m.world.region_neighbours.get_mut(&RegionId(2)).unwrap().reverse();
    assert_eq!(change(&m, 2, A), GroupChange::Reduced);
}

/// The multipliers are the evaluating AI's personality's (`0x00B2B7B0` → `0x0047B710`), its
/// default's where it sets none, the shipped value where neither does (PROVISIONAL fallback,
/// `Multipliers::from_tunables`).
#[test]
fn a_region_worth_takes_the_evaluators_personality() {
    let mut m = model(&[(1, B), (2, B), (3, B)], &[(1, 2), (2, 3)]);
    m.world.region_base_values = [(RegionId(2), 100_000)].into();
    let split = |v: f32| (compound(v, v, 2) * 100_000.0) as u32;
    assert_eq!(m.region_deal_worth(RegionId(2), B, B), split(1.4), "no personalities: the shipped 1.4");
    let mut rules = (*m.rules).clone();
    let mut p = AiPersonalities { default: "default".into(), ..Default::default() };
    let key = "COMPOSITE_VALUE_ANALYSER_REGION_GROUP_SPLIT_MULTIPLIER";
    p.by_key.entry("default".into()).or_default().insert(key.into(), 1.5);
    p.by_key.entry("made_up_a".into()).or_default().insert(key.into(), 1.8);
    rules.ai_personalities = p;
    m.rules = Arc::new(rules);
    m.world.ai_keys.insert(A, FactionAiKeys { personality: "made_up_a".into(), ..Default::default() });
    assert_eq!(m.region_deal_worth(RegionId(2), B, B), split(1.5), "B runs the default");
    assert_eq!(m.region_deal_worth(RegionId(2), B, A), split(1.8), "A evaluates with its own");
}

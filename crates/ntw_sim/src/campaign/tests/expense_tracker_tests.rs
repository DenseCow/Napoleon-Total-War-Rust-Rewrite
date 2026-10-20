//! Campaign model tests: the running record's expense trackers and the script's other income entries
//! (CAMPAIGN_FIDELITY.md "Expense trackers and the record's payment").

use super::*;

/// The upkeep trackers book the change, new − booked, into categories 19 / 20 (`0x0088F950` /
/// `0x0088F990`) and keep the new amount.
#[test]
fn upkeep_trackers_book_the_change() {
    let mut m = test_model();
    economy::set_upkeep_trackers(&mut m, A, 100, 30);
    let r = *m.world.last_economy_record(A).unwrap();
    assert_eq!((r[19], r[20]), (100, 30));
    economy::set_upkeep_trackers(&mut m, A, 120, 10);
    let r = *m.world.last_economy_record(A).unwrap();
    assert_eq!((r[19], r[20]), (120, 10));
    assert_eq!(m.world.upkeep_booked[&A], [120, 10]);
    // Something else booked into 19 stays: only the change is added.
    m.world.economy_history.get_mut(&A).unwrap().last_mut().unwrap()[19] += 7;
    economy::set_upkeep_trackers(&mut m, A, 125, 10);
    assert_eq!(m.world.last_economy_record(A).unwrap()[19], 132);
    // An unknown faction: nothing.
    economy::set_upkeep_trackers(&mut m, FactionId(999), 5, 5);
    assert!(!m.world.upkeep_booked.contains_key(&FactionId(999)));
    assert!(!m.world.economy_history.contains_key(&FactionId(999)));
}

/// A region's policing tracker books its change into the owner's category 21 (`0x00A51230`) and moves
/// with the region on an owner change (`0x00AAF220` in `0x00A64AC0`).
#[test]
fn the_policing_tracker_books_and_follows_the_owner() {
    let mut m = test_model();
    economy::set_policing_tracker(&mut m, RegionId(10), 50);
    assert_eq!(m.world.last_economy_record(A).unwrap()[21], 50);
    economy::set_policing_tracker(&mut m, RegionId(10), 30);
    assert_eq!(m.world.last_economy_record(A).unwrap()[21], 30);
    assert_eq!(m.world.regions[&RegionId(10)].population_state.policing_booked, 30);
    assert!(m.transfer_region(RegionId(10), B));
    assert_eq!(m.world.last_economy_record(A).unwrap()[21], 0);
    assert_eq!(m.world.last_economy_record(B).unwrap()[21], 30);
    assert_eq!(m.world.regions[&RegionId(10)].population_state.policing_booked, 30);
}

/// A deal handing a region to the faction that already owns it (nothing checks a deal's region owners,
/// `0x00C18BF0`): `0x00B449F0` still runs, but `0x00A64AC0` only stops the research at the region's
/// schools (`0x008B4AB0` at `0x00A64CE5`) before it returns for the same owner (`0x00A64D11` /
/// `0x00A64D13`), and then `0x00B1B300` hands the slots back (a third faction's hold too). The queues,
/// the policing tracker and the running taxes are left alone; the region still counts as received.
#[test]
fn a_same_owner_deal_handover_stops_research_and_passes_slots_only() {
    use super::details::TechResearch;
    let mut m = test_model();
    m.turn.humans = vec![A, B];
    let r10 = RegionId(10);
    {
        let r = m.world.regions.get_mut(&r10).unwrap();
        (r.slots[1].holder, r.slots[1].id) = (Some(C), 55);
        r.road_holder = Some(C);
        r.recruitment_queue.push(RecruitmentItem { id: RecruitmentItemId(9), unit_key: "u".into(), turns_remaining: 2, cost: 0, target: None, target_lost: false });
    }
    m.world.faction_details.entry(A).or_default().research.insert("t".into(), TechResearch { progress: 4.0, researcher: 55, traded: 0 });
    economy::set_policing_tracker(&mut m, r10, 50);
    economy::refresh_running_record_round_start(&mut m, A);
    let before = *m.world.last_economy_record(A).unwrap();
    m.world.regions.get_mut(&r10).unwrap().gdp += 5000;
    assert_ne!(economy::faction_income(&m, A).taxes, before[5], "a refresh would change the taxes");
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![r10], offered: vec![] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    let r = &m.world.regions[&r10];
    assert_eq!(r.owner, A);
    assert_eq!((r.slots[1].holder, r.road_holder), (None, None), "C's holds pass back to the owner");
    assert_eq!(r.recruitment_queue.len(), 1, "the queues are not cleared");
    assert_eq!(r.population_state.policing_booked, 50);
    assert_eq!(m.world.faction_details[&A].research["t"], TechResearch { progress: 4.0, researcher: 0, traded: 0 });
    let after = m.world.last_economy_record(A).unwrap();
    assert_eq!((after[5], after[21]), (before[5], before[21]), "neither the taxes nor the policing tracker refresh");
    assert_eq!(m.world.deal_regions_received.get(&A), Some(&1));
    assert!(!m.transfer_region(r10, A), "the region did not change hands");
}

/// The round end's first pass sets every listed faction's upkeep trackers to its upkeep and each of its
/// regions' policing trackers to the policing cost the refresh just stored (`0x008E1030`); the rebels and
/// factions out of the game are skipped.
#[test]
fn the_round_end_pass_sets_the_trackers() {
    let mut m = test_model();
    economy::set_policing_tracker(&mut m, RegionId(12), 40);
    m.prepare_round_end();
    for f in [A, B] {
        let income = economy::faction_income(&m, f);
        let r = *m.world.last_economy_record(f).unwrap();
        assert_eq!(r[19] + r[20], income.upkeep, "{f:?}");
        let booked = m.world.upkeep_booked[&f];
        assert_eq!(booked[0] + booked[1], income.upkeep);
        let policing: i32 = m.world.regions.values().filter(|x| x.owner == f).map(|x| x.population_state.policing_cost).sum();
        assert_eq!(r[21], policing, "{f:?}");
    }
    // AI owners pay no policing (`0x00AA9C10`): region 12's 40 is booked out again.
    assert_eq!(m.world.regions[&RegionId(12)].population_state.policing_booked, 0);
}

/// The script's `other_income_mod` (`0x0097BB70` → `0x00A15B30`): a positive amount (FISTP-rounded) adds
/// to category 11 and carries into the next records; a negative one adds `−amount` to category 18 for a
/// faction no human plays, and nothing for a human's; 0 does nothing.
#[test]
fn script_other_income_entries_book_the_running_record() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    let other = economy::other_income(&m, B);
    economy::script_other_income_mod(&mut m, B, 2500.4);
    assert_eq!(m.world.last_economy_record(B).unwrap()[11], other + 2500);
    // Ties round to even.
    economy::script_other_income_mod(&mut m, B, 2.5);
    economy::script_other_income_mod(&mut m, B, 3.5);
    assert_eq!(m.world.last_economy_record(B).unwrap()[11], other + 2506);
    economy::script_other_income_mod(&mut m, B, -250.0);
    economy::script_other_income_mod(&mut m, B, 0.0);
    let r = *m.world.last_economy_record(B).unwrap();
    assert_eq!((r[11], r[18]), (other + 2506, 250));
    // Both are carried into the next running record.
    economy::settle_round(&mut m, B);
    let r = *m.world.last_economy_record(B).unwrap();
    assert_eq!((r[11], r[18]), (other + 2506, 250));
    // A human faction's negative amount is dropped (`0x0097BBE3`).
    economy::script_other_income_mod(&mut m, A, -100.0);
    assert_eq!(m.world.last_economy_record(A).map_or(0, |r| r[18]), 0);
    // A load drops the entries (they are not saved): the record is booked again without them.
    economy::rebuild_running_record(&mut m, B);
    let r = *m.world.last_economy_record(B).unwrap();
    assert_eq!((r[11], r[18]), (other, 0));
}

/// A faction without an economy history gets its running record from the rebuild, holding the full
/// tracker amounts, so a later tracker change adds only its difference to a complete record.
#[test]
fn rebuilding_a_faction_without_history_creates_the_record_with_the_trackers() {
    let mut m = test_model();
    m.world.economy_history.remove(&B);
    m.world.upkeep_booked.insert(B, [30, 12]);
    economy::rebuild_running_record(&mut m, B);
    let r = *m.world.last_economy_record(B).unwrap();
    assert_eq!((r[19], r[20]), (30, 12));
    economy::set_upkeep_trackers(&mut m, B, 35, 12);
    let r = *m.world.last_economy_record(B).unwrap();
    assert_eq!((r[19], r[20]), (35, 12));
}

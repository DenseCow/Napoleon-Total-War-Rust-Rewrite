//! The region-transfer expulsion (`0x008D9DC0`; analysis/fidelity/CAMPAIGN_FIDELITY.md, director
//! section "Transfer and the old owner's forces"): who leaves a region at an owner change, a deal and
//! the end of a protectorate tie, the loss of military access ("Access-loss expulsions"), and where
//! they go.

use super::*;
use crate::campaign::expulsion::{expel_faction_from_region, ExpelledForce};
use crate::campaign::pathing::{CellKind, PathGrid};
use crate::campaign::seams::RegionExpulsion;
use crate::seam::Maker;

/// [`test_model`] with the settlements 100 apart (10 at x 0, 11 at 100, 12 at 200), a MADE-UP
/// region 13 of C's at x 400, capitals A 10, B 11, C 13, and B's army at its own settlement.
fn spread_model() -> CampaignModel {
    let mut m = test_model();
    let mut r13 = region(13, C, 1000, 0);
    r13.settlement.position = pos(400, 0);
    m.world.regions.insert(r13.id, r13);
    for (id, x) in [(10, 0), (11, 100), (12, 200)] {
        m.world.regions.get_mut(&RegionId(id)).unwrap().settlement.position = pos(x, 0);
    }
    for (f, capital) in [(A, 10), (B, 11), (C, 13)] {
        set_details(&mut m, f, Some(capital), false);
    }
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(100, 0);
    m
}

/// Adds a general `id` of `faction` leading army `id + 900` at `at`.
fn add_army(m: &mut CampaignModel, id: i32, faction: FactionId, at: (Fixed20, Fixed20)) -> ForceId {
    let mut c = character(id, faction, CharacterKind::General, 20);
    c.position = at;
    m.world.characters.insert(c.id, c);
    let force = ForceId(id as u32 + 900);
    m.world.forces.insert(force, MilitaryForce { id: force, faction, commander: Some(CharacterId(id)), units: vec![unit(id)], is_navy: false });
    force
}

fn garrison(m: &mut CampaignModel, force: ForceId, region: u32) {
    let c = m.world.forces[&force].commander.unwrap();
    m.world.characters.get_mut(&c).unwrap().garrisoned_in = Some(RegionId(region));
    m.world.regions.get_mut(&RegionId(region)).unwrap().garrison = Some(force);
}

fn at(m: &CampaignModel, c: i32) -> (Fixed20, Fixed20) {
    m.world.characters[&CharacterId(c)].position
}

fn fpos(x: f64, z: f64) -> (Fixed20, Fixed20) {
    (Fixed20::from_f64(x), Fixed20::from_f64(z))
}

/// A deal's handover: the old owner's garrison leaves the settlement for its nearest own region
/// (`0x008DCB30` sorts its regions by distance), the garrison link cleared; its agent stays.
#[test]
fn a_deal_handover_puts_the_old_garrison_out_to_its_nearest_region() {
    let mut m = spread_model();
    m.turn.humans = vec![A, B];
    let mut r14 = region(14, A, 1000, 0);
    r14.settlement.position = pos(260, 0);
    m.world.regions.insert(r14.id, r14);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(200, 0);
    garrison(&mut m, ForceId(1000), 12);
    m.world.characters.get_mut(&CharacterId(102)).unwrap().position = pos(200, 0);
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![], offered: vec![RegionId(12)] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(12)].owner, B);
    assert_eq!(at(&m, 100), pos(260, 0), "region 14 (60 away) before region 10 (200 away)");
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(12)].garrison, None);
    assert_eq!(at(&m, 102), pos(200, 0), "agents are never moved");
    assert_eq!(at(&m, 101), pos(100, 0), "the new owner's army is not touched");
}

/// A third party standing in the region at an owner change stays where it is when the new owner
/// gives it military access, is at war with it, or is its protectorate (`0x00B16570`, the "may stay"
/// flag); otherwise it goes to its own region.
#[test]
fn a_third_party_stays_with_access_war_or_a_protectorate() {
    let base = || {
        let mut m = spread_model();
        add_army(&mut m, 103, C, pos(205, 0));
        m
    };
    let mut m = base();
    assert!(m.transfer_region(RegionId(12), B, &mut Vec::new()));
    assert_eq!(at(&m, 103), pos(400, 0), "no tie: to its own region 13");

    let mut m = base();
    m.world.relationships.insert((B, C), Relationship { military_access_turns: 2, ..Default::default() });
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 103), pos(205, 0), "B gives C access");

    let mut m = base();
    m.apply(CampaignCommand::DeclareWar { a: B, b: C }).unwrap();
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 103), pos(205, 0), "at war with B");

    let mut m = base();
    m.world.set_stance(B, C, Stance::Protectorate).unwrap();
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 103), pos(205, 0), "B is C's protectorate");

    // Access from the old owner A is no reason to stay in B's region now.
    let mut m = base();
    m.world.relationships.insert((A, C), Relationship { military_access_turns: 2, ..Default::default() });
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 103), pos(400, 0));
}

/// The candidates' order (`0x008DCB30`): the faction's own regions come before a third party's,
/// however near; among a third party's regions (those whose owner lets it stand) the nearest wins.
#[test]
fn own_regions_come_before_a_nearer_third_partys() {
    let mut m = spread_model();
    add_army(&mut m, 103, C, pos(195, 0));
    m.world.relationships.insert((A, C), Relationship { military_access_turns: 2, ..Default::default() });
    let mut own = m.clone();
    own.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&own, 103), pos(400, 0), "C's region 13 (205 away), not A's 10 (195 away)");
    // C holds no region (its capital record still names 13): A's regions, nearest first.
    m.world.regions.get_mut(&RegionId(13)).unwrap().owner = A;
    m.world.characters.get_mut(&CharacterId(103)).unwrap().position = pos(210, 0);
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 103), pos(400, 0), "A's 13 (190 away) before A's 10 (210 away)");
}

/// The third parties' regions go in in the region manager's order (world +0x34 → +0x10 / +0x14,
/// `World::regions_in_manager_order`) before the sort, so of two as near the first in that order wins
/// (our sort is stable).
#[test]
fn a_third_partys_regions_go_in_in_the_manager_order() {
    let army_goes_to = |order: Option<[u32; 4]>| {
        let mut m = spread_model();
        add_army(&mut m, 103, C, pos(200, 0));
        m.world.relationships.insert((A, C), Relationship { military_access_turns: 2, ..Default::default() });
        // C holds no region: A's 10 and 13 are both 200 away.
        m.world.regions.get_mut(&RegionId(13)).unwrap().owner = A;
        if let Some(order) = order {
            m.world.region_order = order.map(RegionId).to_vec();
        }
        m.transfer_region(RegionId(12), B, &mut Vec::new());
        at(&m, 103)
    };
    assert_eq!(army_goes_to(None), pos(0, 0), "no order listed: id order, 10 first");
    assert_eq!(army_goes_to(Some([13, 12, 11, 10])), pos(400, 0), "13 first in the manager's order");
    assert_eq!(army_goes_to(Some([10, 11, 12, 13])), pos(0, 0));
}

/// An army aboard a navy is never expelled (INFERRED: it stands where its carrier is, `0x00A17D60`,
/// on a sea polygon; see `expel_faction_from_region`).
#[test]
fn an_embarked_army_is_not_expelled() {
    let mut m = spread_model();
    let army = add_army(&mut m, 103, C, pos(205, 0));
    m.world.embarked.insert(army, ForceId(9999));
    assert!(expel_faction_from_region(&m, C, RegionId(12)).is_empty());
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 103), pos(205, 0));
}

/// The map: region 13 (A's) in columns 0-4, region 12 (A's) in 5-9, the sea in 10-19; region 12's
/// port at (9.5, 1.5), its harbour's dock at (12.5, 1.5). A's army garrisons 12 standing at
/// (7.5, 1.5) and A's navy is docked in the port.
fn terrain_model(with_13: bool) -> CampaignModel {
    let mut m = test_model();
    set_details(&mut m, A, Some(10), false);
    m.world.regions.insert(RegionId(13), region(13, A, 1000, 0));
    let mut grid = PathGrid::new((0.0, 0.0), 1.0, 20, 4);
    grid.region_keys = vec!["test_region_12".into(), "test_region_13".into(), "sea".into()];
    for i in 0..grid.kind.len() {
        let col = i % 20;
        let (kind, region) = match col {
            0..=4 if with_13 => (CellKind::Land, 1),
            0..=4 => (CellKind::Blocked, u32::MAX),
            5..=9 => (CellKind::Land, 0),
            _ => (CellKind::Sea, 2),
        };
        grid.kind[i] = kind as u8;
        grid.region[i] = region;
    }
    grid.harbours.push(crate::campaign::embark::Harbour { port: (9.5, 1.5), dock: (12.5, 1.5), outline: Vec::new() });
    m.attach_terrain(crate::campaign::world::Terrain(Arc::new(grid)));
    m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(RegionSlot {
        key: "port:test_region_12".into(),
        slot_type: "port".into(),
        building: None,
        position: Some(fpos(9.5, 1.5)),
        port: true,
        holder: None,
        id: 0,
        blockade: None,
    });
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = fpos(7.5, 1.5);
    garrison(&mut m, ForceId(1000), 12);
    let mut admiral = character(103, A, CharacterKind::Admiral, 20);
    admiral.position = fpos(9.5, 1.5);
    m.world.characters.insert(admiral.id, admiral);
    m.world.forces.insert(ForceId(1003), MilitaryForce { id: ForceId(1003), faction: A, commander: Some(CharacterId(103)), units: vec![unit(7)], is_navy: true });
    m
}

/// With the map: the army goes to the nearest cell of its nearest own region (the flood
/// `0x00B35640` within 100 of the candidate's point); the navy leaves its port through the
/// harbour's dock to the sea, and the port slot it held goes back to the new owner.
#[test]
fn on_the_map_the_army_goes_to_its_region_and_the_navy_puts_to_sea() {
    let mut m = terrain_model(true);
    assert_eq!(m.docked_port(ForceId(1003)), Some((RegionId(12), 0)));
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 100), fpos(4.5, 1.5), "region 13's cell nearest to the army");
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(12)].garrison, None);
    assert_eq!(at(&m, 103), fpos(12.5, 1.5), "out through the dock");
    assert_eq!(m.docked_port(ForceId(1003)), None);
    assert_eq!(m.world.regions[&RegionId(12)].slots[0].holder, None);

    // At war with the new owner: the army may stay (its own position first) but leaves the
    // settlement; the navy stays docked, the port still held by A.
    let mut m = terrain_model(true);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 100), fpos(7.5, 1.5));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(at(&m, 103), fpos(9.5, 1.5));
    assert_eq!(m.world.regions[&RegionId(12)].slots[0].holder, Some(A));
}

/// The expelled are placed by `0x0096C8D0` (`0x008DA072`, `0x008DA1B2`), whose last call to
/// `0x00A285E0` passes 1 (`0x0096C9C4`): set down there, the army's hidden flag is recomputed (out
/// of the settlement, every unit able to hide: hidden).
#[test]
fn the_expelled_army_is_set_down_with_its_hidden_flag() {
    let mut m = terrain_model(true);
    let mut rules = (*m.rules).clone();
    for r in rules.units.values_mut() {
        r.campaign_stealth = true;
    }
    m.rules = Arc::new(rules);
    m.world.character_details.entry(CharacterId(100)).or_default();
    assert!(!m.stealthy(CharacterId(100)), "inside the settlement");
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 100), fpos(4.5, 1.5));
    assert!(m.world.character_details[&CharacterId(100)].hidden);
}

/// No candidate places the army (its regions are not on the map): it stays where it stands, out of
/// the settlement.
#[test]
fn an_army_no_region_takes_stays_in_place_out_of_the_settlement() {
    let mut m = terrain_model(false);
    let mut handed = m.clone();
    handed.world.regions.get_mut(&RegionId(12)).unwrap().owner = B;
    let plan = expel_faction_from_region(&handed, A, RegionId(12));
    assert!(plan.contains(&ExpelledForce { force: ForceId(1000), to: None, notice: false }), "{plan:?}");
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 100), fpos(7.5, 1.5));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(12)].garrison, None);
}

/// The forces leave in the order of the faction's character list (+0x768, `0x008D9E7A`), each
/// commander's army or navy in turn, not armies first and then navies by id.
#[test]
fn the_expulsion_walks_the_faction_list_in_its_order() {
    let mut m = terrain_model(true);
    let extra = add_army(&mut m, 104, A, fpos(6.5, 1.5));
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = B;
    let forces = |m: &CampaignModel| expel_faction_from_region(m, A, RegionId(12)).into_iter().map(|e| e.force).collect::<Vec<_>>();
    assert_eq!(forces(&m), vec![ForceId(1000), ForceId(1003), extra], "no order listed: id order");
    m.world.character_order = vec![CharacterId(104), CharacterId(103), CharacterId(100)];
    assert_eq!(forces(&m), vec![extra, ForceId(1003), ForceId(1000)]);
}

/// The end of a protectorate tie (`0x00B64B10`): the patron's forces leave each of the
/// protectorate's regions, which they could stand in while the tie held.
#[test]
fn the_end_of_a_protectorate_tie_puts_the_patron_out() {
    let mut m = spread_model();
    m.make_protectorate(B, A).unwrap();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(95, 0);
    assert_eq!(expel_faction_from_region(&m, A, RegionId(11)), vec![ExpelledForce { force: ForceId(1000), to: Some(pos(95, 0)), notice: false }], "may stay");
    m.end_protectorate_tie(B, A).unwrap();
    assert_eq!(at(&m, 100), pos(0, 0), "to A's nearest region, 10 (95 away; 12 is 105)");
}

/// The deal's region lists (`0x00C114B0`) expel the recipient from the regions it gives up even when
/// it has no capital, which the owner change's own pass skips (`0x00A64AC0`).
#[test]
fn the_deal_expels_a_recipient_without_a_capital() {
    let mut m = spread_model();
    m.turn.humans = vec![A, B];
    m.world.faction_details.remove(&B);
    let mut r15 = region(15, B, 1000, 0);
    r15.settlement.position = pos(150, 0);
    m.world.regions.insert(r15.id, r15);
    garrison(&mut m, ForceId(1001), 11);
    let mut handover = m.clone();
    handover.transfer_region(RegionId(11), A, &mut Vec::new());
    assert_eq!(handover.world.characters[&CharacterId(101)].garrisoned_in, Some(RegionId(11)), "no capital: not in the owner change's pass");
    m.apply(CampaignCommand::BeginNegotiation { proposer: A, recipient: B }).unwrap();
    m.apply(CampaignCommand::ProposeRegions { clear: false, demanded: vec![RegionId(11)], offered: vec![] }).unwrap();
    m.apply(CampaignCommand::AcceptDeal).unwrap();
    assert_eq!(m.world.regions[&RegionId(11)].owner, A);
    assert_eq!(at(&m, 101), pos(150, 0));
    assert_eq!(m.world.characters[&CharacterId(101)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(11)].garrison, None);
}

/// The rule runs through its seam: a mod's rule that expels no one keeps everyone in place.
#[test]
fn a_mod_rule_replaces_the_expulsion() {
    let mut m = spread_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(200, 0);
    garrison(&mut m, ForceId(1000), 12);
    let keep: Maker<RegionExpulsion> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: FactionId, _: RegionId, _: &mut _| Vec::new()));
    Arc::make_mut(&mut m.rules).seams.region_expulsion.apply("test.keep", &keep);
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 100), pos(200, 0));
    assert_eq!(m.world.regions[&RegionId(12)].garrison, Some(ForceId(1000)));
}

/// A faction out of the game (`+0x824`) is never expelled, whichever caller asks: the rule returns
/// at once (`0x008D9DDC`), so the end of a protectorate tie leaves the patron's army in place too.
#[test]
fn a_faction_out_of_the_game_is_not_expelled() {
    let mut m = spread_model();
    m.make_protectorate(B, A).unwrap();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(95, 0);
    set_details(&mut m, A, Some(10), true);
    assert!(expel_faction_from_region(&m, A, RegionId(11)).is_empty());
    m.end_protectorate_tie(B, A).unwrap();
    assert_eq!(at(&m, 100), pos(95, 0));
}

/// A garrison that may stay (the new owner gives its faction military access) still leaves the
/// settlement: the residence's vt+0x90 runs whatever the "may stay" flag (`0x008D9DC0`); the flag
/// only puts the region itself first, so the army is placed where it stood.
#[test]
fn a_garrison_that_may_stay_still_leaves_the_settlement() {
    let mut m = terrain_model(true);
    m.world.relationships.insert((B, A), Relationship { military_access_turns: 2, ..Default::default() });
    m.transfer_region(RegionId(12), B, &mut Vec::new());
    assert_eq!(at(&m, 100), fpos(7.5, 1.5));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    assert_eq!(m.world.regions[&RegionId(12)].garrison, None);
}

/// `owner`'s record towards `target` as a war of `enemy` left it (`0x00B0CCE0`): access -1, and one
/// `allied_in_war_against` item saving `saved` access turns.
fn suspend_access(m: &mut CampaignModel, owner: FactionId, target: FactionId, enemy: FactionId, saved: i32) {
    let r = m.relationship_mut(owner, target);
    r.military_access_turns = -1;
    r.allied_in_war_against = vec![crate::campaign::details::AlliedWar { enemy, saved_access_turns: saved }];
}

/// Cancelling military access (`0x00B67BD0`) sends the target's forces out of each of the giver's
/// regions. The war declaration's abuse cancel (`0x00B0DAF0`) comes after the stance turned to war,
/// so there they may stay.
#[test]
fn cancelled_access_expels_the_target_but_not_at_war() {
    let base = || {
        let mut m = spread_model();
        add_army(&mut m, 103, C, pos(205, 0));
        m.relationship_mut(A, C).military_access_turns = 5;
        m
    };
    let mut m = base();
    m.apply_diplomatic_action(A, C, crate::campaign::treaties::DiplomaticAction::CancelMilitaryAccess).unwrap();
    assert_eq!(at(&m, 103), pos(400, 0), "out of A's region 12 to C's own 13");

    let mut m = base();
    m.declare_war_rules(C, A).unwrap();
    assert_eq!(m.world.relationships[&(A, C)].military_access_turns, 0, "the abuse cancelled it");
    assert_eq!(at(&m, 103), pos(205, 0), "at war: may stay");
}

/// Timed access running out at the giver's round end (`0x00B29170`) sends the target out.
#[test]
fn timed_access_running_out_expels_the_target() {
    let mut m = spread_model();
    add_army(&mut m, 103, C, pos(205, 0));
    m.relationship_mut(A, C).military_access_turns = 2;
    m.diplomacy_round_end(A);
    assert_eq!(at(&m, 103), pos(205, 0), "one turn left");
    m.diplomacy_round_end(A);
    assert_eq!(m.world.relationships[&(A, C)].military_access_turns, 0);
    assert_eq!(at(&m, 103), pos(400, 0));

    // Indefinite access never runs out.
    let mut m = spread_model();
    add_army(&mut m, 103, C, pos(205, 0));
    m.relationship_mut(A, C).military_access_turns = -1;
    m.diplomacy_round_end(A);
    assert_eq!(at(&m, 103), pos(205, 0));
}

/// Breaking an alliance (`0x00B13A30` on the breaker's record, `0x00B0CA20` on the other's): a side
/// whose access the alliance had suspended for a war, and that gets none back, leaves the other's
/// regions; access given back lets it stay; the war declaration's break (its war flag) moves no one,
/// not even out of a garrison.
#[test]
fn a_broken_alliance_expels_a_side_given_back_no_access() {
    let base = |saved| {
        let mut m = spread_model();
        add_army(&mut m, 103, C, pos(205, 0));
        m.form_alliance(A, C).unwrap();
        suspend_access(&mut m, A, C, B, saved);
        m
    };
    let mut m = base(0);
    m.break_alliance(A, C).unwrap();
    assert_eq!(at(&m, 103), pos(400, 0), "A breaks: its own record");
    let mut m = base(0);
    m.break_alliance(C, A).unwrap();
    assert_eq!(at(&m, 103), pos(400, 0), "C breaks: A's record is the other's");

    let mut m = base(3);
    m.break_alliance(A, C).unwrap();
    assert_eq!(m.world.relationships[&(A, C)].military_access_turns, 3);
    assert_eq!(at(&m, 103), pos(205, 0), "access given back");

    let mut m = base(0);
    let army = ForceId(103 + 900);
    garrison(&mut m, army, 12);
    m.declare_war_rules(A, C).unwrap();
    assert_eq!(at(&m, 103), pos(205, 0));
    assert_eq!(m.world.regions[&RegionId(12)].garrison, Some(army), "not expelled at all");
}

/// Peace (`0x00B0D0C0` from `0x00B262C0`): the access both sides' alliances suspended against the
/// enemy comes back, and a side given back none leaves the other's regions. ORIGINAL BUG (0x00B262C0):
/// the exe runs that loop on the proposer's manager alone, so the target's allies kept their indefinite
/// access and their item; ours gives both sides' back, whoever proposes.
#[test]
fn peace_gives_back_both_sides_allies_access() {
    let base = || {
        let mut m = spread_model();
        add_army(&mut m, 104, A, pos(405, 0));
        m.world.declare_war(A, B).unwrap();
        m.form_alliance(A, C).unwrap();
        suspend_access(&mut m, C, A, B, 0);
        m
    };
    let mut m = base();
    m.make_peace_rules(A, B, false).unwrap();
    assert!(m.world.relationships[&(C, A)].allied_in_war_against.is_empty());
    assert_eq!(m.world.relationships[&(C, A)].military_access_turns, 0);
    assert_eq!(at(&m, 104), pos(200, 0), "out of C's region 13 to A's nearest, 12");

    let mut m = base();
    m.make_peace_rules(B, A, false).unwrap();
    assert!(
        m.world.relationships[&(C, A)].allied_in_war_against.is_empty(),
        "the target's ally's item is cleared too (the exe kept it)"
    );
    assert_eq!(m.world.relationships[&(C, A)].military_access_turns, 0);
    assert_eq!(at(&m, 104), pos(200, 0), "expelled as when A proposes");

    // Saved timed access comes back to the target's ally's record as well.
    let mut m = spread_model();
    m.world.declare_war(A, B).unwrap();
    m.form_alliance(A, C).unwrap();
    suspend_access(&mut m, C, A, B, 4);
    m.make_peace_rules(B, A, false).unwrap();
    assert_eq!(m.world.relationships[&(C, A)].military_access_turns, 4);
    assert!(m.world.relationships[&(C, A)].allied_in_war_against.is_empty());
}

/// Peace's step order (`0x00B262C0`): the `0x00B0D0C0` loop's expulsion runs before the character walk
/// (`0x00A25570`), so a raider it moves out has lost his raids (the move ends them, with no price update)
/// and his walk step finds none to end: the peace runs only its own update. Given back access, he stays,
/// and his walk step ends the raid with its own update first.
#[test]
fn a_peace_expels_before_its_character_walk_ends_raids() {
    use crate::campaign::trade::{TradeLeg, TradePath, TradeWaypoint};
    let run = |saved: i32| {
        let mut m = spread_model();
        add_army(&mut m, 104, A, pos(405, 0));
        m.world.declare_war(A, B).unwrap();
        m.form_alliance(A, C).unwrap();
        suspend_access(&mut m, C, A, B, saved);
        // A MADE-UP leg (segment 0) through C's region 13, carrying B's route.
        m.world.commodity_prices = vec![10];
        m.world.trade_network = vec![TradeLeg { from: 2, to: 1, length: 10.0, splines: vec![0] }];
        let line = [(400.0, 0.0), (403.0, 0.0), (406.0, 0.0), (410.0, 0.0)];
        m.world.trade_sight = vec![crate::campaign::visibility::TradeSegmentSight { cells: Vec::new(), domestic: Vec::new(), international: Vec::new(), curves: vec![line] }];
        let w = |from, to| TradeWaypoint { region: RegionId(13), from, to, sea: false, from_pos: None, to_pos: None };
        let path = TradePath { waypoints: vec![w(1, 2), w(u32::MAX, u32::MAX)], volumes: vec![30], capital_port: false, accumulated: 0 };
        m.world.domestic_trade.insert(B, vec![path]);
        assert!(m.raid_trade_route(CharacterId(104), pos(405, 0)));
        assert!(m.is_raiding(CharacterId(104)));
        let updates = price_updates(|| drop(m.make_peace_rules(A, B, true).unwrap()));
        assert!(m.world.commerce_raids.is_empty());
        (at(&m, 104), updates.len())
    };
    assert_eq!(run(0), (pos(200, 0), 1), "expelled to A's region 12 first: the peace's own update only");
    assert_eq!(run(3), (pos(405, 0), 2), "kept access: his raid end's update, then the peace's");
}

/// A patron breaking with its protectorate ends the tie first (`0x00B0CA20` → `0x00B64B10`): the
/// patron leaves the protectorate's regions, unless the break comes from its war declaration (the
/// war flag is the tie's skip flag). The protectorate breaking away moves no one: neither record
/// runs the tie's end.
#[test]
fn a_patron_breaking_with_its_protectorate_leaves_its_regions() {
    let base = || {
        let mut m = spread_model();
        add_army(&mut m, 104, A, pos(405, 0));
        m.make_protectorate(C, A).unwrap();
        m
    };
    let mut m = base();
    m.break_alliance(A, C).unwrap();
    assert_eq!(m.world.stance(C, A), Stance::Neutral);
    assert_eq!(at(&m, 104), pos(200, 0));

    let mut m = base();
    m.declare_war_rules(A, C).unwrap();
    assert_eq!(at(&m, 104), pos(405, 0));

    let mut m = base();
    m.break_alliance(C, A).unwrap();
    assert_eq!(m.world.stance(C, A), Stance::Neutral);
    assert_eq!(at(&m, 104), pos(405, 0));
}

/// Each faction's region list (faction +0x76C) follows the region manager's order after a load
/// (`0x00A4CE20`, `0x00A45950`; `World::region_order`); a gain appends the region at the end of the
/// new owner's list (`0x008AE270` → `0x008CF7D0`) and the loss keeps the old owner's others in order
/// (`0x008B0210`). A transfer to the owner moves nothing: `0x00A64AC0` returns before the lists
/// (`0x00A64D13`).
#[test]
fn a_region_list_keeps_the_manager_order_and_a_gain_goes_last() {
    let mut m = spread_model();
    let ids = |m: &CampaignModel| m.world.regions_in_list_order().into_iter().map(RegionId::raw).collect::<Vec<_>>();
    assert_eq!(ids(&m), vec![10, 11, 12, 13], "no order listed: id order");
    m.world.region_order = vec![RegionId(12), RegionId(10), RegionId(13)];
    assert_eq!(ids(&m), vec![12, 10, 13, 11], "the unlisted after the listed");
    assert!(m.transfer_region(RegionId(10), B, &mut Vec::new()));
    assert_eq!(ids(&m), vec![12, 13, 11, 10], "the gain at the end, the rest in their order");
    assert!(m.transfer_region(RegionId(11), A, &mut Vec::new()));
    assert_eq!(ids(&m), vec![12, 13, 10, 11]);
    assert!(m.transfer_region(RegionId(10), A, &mut Vec::new()));
    assert_eq!(ids(&m), vec![12, 13, 11, 10], "gained again: moved to the end, listed once");
    assert!(!m.transfer_region(RegionId(10), A, &mut Vec::new()), "already A's");
    assert_eq!(ids(&m), vec![12, 13, 11, 10], "no owner change: no move");
}

/// The access-loss loops walk the giver's region list in its order (`0x00B67BD0` and the other
/// callers of `expel_from_each_region_of`), a gained region last: here the order the expulsion seam
/// is asked in.
#[test]
fn an_access_loss_walks_the_givers_region_list_in_its_order() {
    let mut m = spread_model();
    m.world.regions.insert(RegionId(14), region(14, B, 1000, 0));
    m.world.region_order = [12, 14, 10, 11, 13].map(RegionId).to_vec();
    let walked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = walked.clone();
    let record: Maker<RegionExpulsion> = Arc::new(move |_| {
        let log = log.clone();
        Arc::new(move |_: &CampaignModel, f: FactionId, r: RegionId, _: &mut _| {
            log.lock().unwrap().push((f, r));
            Vec::new()
        })
    });
    Arc::make_mut(&mut m.rules).seams.region_expulsion.apply("test.record", &record);
    assert!(m.transfer_region(RegionId(14), A, &mut Vec::new()));
    walked.lock().unwrap().clear();
    m.relationship_mut(A, C).military_access_turns = 5;
    m.apply_diplomatic_action(A, C, crate::campaign::treaties::DiplomaticAction::CancelMilitaryAccess).unwrap();
    assert_eq!(*walked.lock().unwrap(), vec![(C, RegionId(12)), (C, RegionId(10)), (C, RegionId(14))]);
}

/// The expulsion's move notices in `events`: (recipient, commander, position).
fn retreat_notices(events: &[CampaignEvent]) -> Vec<(FactionId, CharacterId, (Fixed20, Fixed20))> {
    events
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::Message { recipient, message: CampaignMessage::YourArmyRetreatsRegion { character, at } } => {
                Some((*recipient, *character, *at))
            }
            _ => None,
        })
        .collect()
}

/// The move notice (`0x009FB680`, from `0x008D9DC0` at `0x008DA091`): a placed army that may not stay
/// tells its own faction where it went, after the move; the navy put to sea tells no one, and at war
/// (the army may stay, the navy stays docked) nothing is sent.
#[test]
fn a_placed_army_that_may_not_stay_sends_its_faction_the_move_notice() {
    let mut m = terrain_model(true);
    let mut events = Vec::new();
    m.transfer_region(RegionId(12), B, &mut events);
    assert_eq!(at(&m, 103), fpos(12.5, 1.5), "the navy moved too");
    assert_eq!(retreat_notices(&events), vec![(A, CharacterId(100), fpos(4.5, 1.5))], "{events:?}");
    let message = CampaignMessage::YourArmyRetreatsRegion { character: CharacterId(100), at: fpos(4.5, 1.5) };
    assert_eq!(message.type_name(), "your_army_retreats_region", "message type 0x119");

    let mut m = terrain_model(true);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    let mut events = Vec::new();
    m.transfer_region(RegionId(12), B, &mut events);
    assert_eq!(retreat_notices(&events), vec![], "at war: may stay");
}

/// No notice for an army that may stay (military access; `0x008DA077`) though it leaves the
/// settlement, nor for one no candidate placed (the call sits in the placed branch).
#[test]
fn no_move_notice_when_the_army_may_stay_or_is_not_placed() {
    let mut m = terrain_model(true);
    m.world.relationships.insert((B, A), Relationship { military_access_turns: 2, ..Default::default() });
    let mut events = Vec::new();
    m.transfer_region(RegionId(12), B, &mut events);
    assert_eq!(m.world.regions[&RegionId(12)].garrison, None);
    assert_eq!(retreat_notices(&events), vec![]);

    let mut m = terrain_model(false);
    let mut events = Vec::new();
    m.transfer_region(RegionId(12), B, &mut events);
    assert_eq!(at(&m, 100), fpos(7.5, 1.5), "not placed");
    assert_eq!(retreat_notices(&events), vec![]);
}

/// Every expulsion caller passes its notices on: the cancelled access (`0x00B67BD0`, before the command's
/// `alliance_broken`), the end of a protectorate tie (before its stance change) and the
/// `CCQ_MOVE_ARMIES_OUT_OF_REGIONS` command.
#[test]
fn the_expulsion_callers_return_the_move_notices() {
    let mut m = spread_model();
    add_army(&mut m, 103, C, pos(205, 0));
    m.relationship_mut(A, C).military_access_turns = 5;
    let events = m.apply_diplomatic_action(A, C, crate::campaign::treaties::DiplomaticAction::CancelMilitaryAccess).unwrap();
    assert!(
        matches!(events.as_slice(), [
            CampaignEvent::Message { recipient: C, message: CampaignMessage::YourArmyRetreatsRegion { character: CharacterId(103), .. } },
            CampaignEvent::Message { recipient: C, message: CampaignMessage::AllianceBroken { .. } },
        ]),
        "{events:?}"
    );
    assert_eq!(retreat_notices(&events), vec![(C, CharacterId(103), pos(400, 0))]);

    let mut m = spread_model();
    m.make_protectorate(B, A).unwrap();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(95, 0);
    let events = m.end_protectorate_tie(B, A).unwrap();
    assert_eq!(retreat_notices(&events), vec![(A, CharacterId(100), pos(0, 0))]);
    assert!(matches!(events.last(), Some(CampaignEvent::StanceChanged { .. })), "{events:?}");

    let mut m = spread_model();
    add_army(&mut m, 103, C, pos(205, 0));
    let events = m.apply(CampaignCommand::MoveArmiesOutOfRegions { a: A, b: C }).unwrap();
    assert_eq!(retreat_notices(&events), vec![(C, CharacterId(103), pos(400, 0))]);
}

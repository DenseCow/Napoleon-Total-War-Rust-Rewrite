//! Campaign model tests: the turn order (`PassTurnToNextActiveFaction` `0x00948CF0`, round start `0x0096C050`).

use super::*;

/// The faction-level turn events of `ev`, in order: (event name, faction).
fn faction_events(ev: &[CampaignEvent]) -> Vec<(&'static str, FactionId)> {
    ev.iter()
        .filter_map(|e| match *e {
            CampaignEvent::FactionRoundStart { faction } => Some(("round", faction)),
            CampaignEvent::FactionTurnStart { faction } => Some(("start", faction)),
            CampaignEvent::FactionTurnEnd { faction } => Some(("end", faction)),
            _ => None,
        })
        .collect()
}

/// Makes `f` the rebel faction (no key, no diplomacy records), as in the startpos `REBEL_FACTION`.
fn make_rebels(m: &mut CampaignModel, f: FactionId) {
    m.world.factions.get_mut(&f).unwrap().key.clear();
    m.world.relationships.retain(|(a, b), _| *a != f && *b != f);
    for x in m.world.factions.values_mut() {
        x.diplomacy.remove(&f);
    }
}

/// The rebels play last wherever they are listed: the exe keeps them outside its turn list (world
/// +0x1C, `0x0090BEB0`) and gives them the turn past the list's end (`0x00948CF0`). An unlisted
/// faction still comes after the listed ones, before the rebels.
#[test]
fn the_rebels_are_last_in_the_turn_order() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    assert_eq!(m.world.factions_in_turn_order(), vec![A, C, B], "listed B, A, C");
    m.world.turn_order = vec![B, C];
    assert_eq!(m.world.factions_in_turn_order(), vec![C, A, B], "A unlisted, after C, before the rebels");
}

/// A round with the rebels listed first: they still play after every other faction, and their turn
/// ends the round (`0x00948CF0` returns true after the rebels' turn); their round-start event
/// (`0x0096C050`'s tail call of `0x008F1F60` on +0x1C, after the round-start refreshes) and round-end
/// economy (`0x008BC650` on +0x1C) come last.
#[test]
fn the_rebels_turn_ends_the_round() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    m.turn.humans = vec![A];
    set_details(&mut m, C, None, false);
    let start = m.start_campaign();
    assert_eq!(faction_events(&start), vec![("round", A), ("round", C), ("round", B), ("start", A)]);
    assert_eq!(m.calendar.turns_elapsed, 0);
    let end = m.end_turn();
    assert_eq!(
        faction_events(&end),
        vec![
            ("end", A),
            ("start", C),
            ("end", C),
            ("start", B),
            ("end", B),
            ("round", A),
            ("round", C),
            ("round", B),
            ("start", A),
        ]
    );
    assert_eq!(m.calendar.turns_elapsed, 1, "one round end, after the rebels' turn");
}

/// `FactionRoundStart` is posted for a faction out of the game too (`0x008F1F60` posts it outside its
/// +0x824 test), which still gets no turn (`0x00948CF0` passes it over).
#[test]
fn an_out_of_game_faction_gets_the_round_start_event_but_no_turn() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    set_details(&mut m, C, None, true);
    let start = m.start_campaign();
    assert_eq!(faction_events(&start), vec![("round", B), ("round", A), ("round", C), ("start", B), ("end", B), ("start", A)]);
    let end = m.end_turn();
    assert_eq!(
        faction_events(&end),
        vec![("end", A), ("round", B), ("round", A), ("round", C), ("start", B), ("end", B), ("start", A)]
    );
}

/// Runs one round-start step (`0x0096C050`) on its own.
fn run_round_start(m: &mut CampaignModel) {
    m.turn.humans = vec![A];
    m.turn.queue.push_back(TurnStep::RoundStart);
    m.step().expect("the round-start step");
}

/// The round start refreshes the population factors of the listed, in-game factions' regions only
/// (`0x008F1F60` skips step 1 for the rebels, `0x008CEEF0`, and an out-of-game faction, +0x824), and so does
/// the round end's first pass (`0x00948CF0` → `0x008E1030` → `0x00AB3E70`: listed factions only, +0x824 clear).
#[test]
fn the_round_start_and_end_refresh_only_listed_in_game_factions_regions() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    set_details(&mut m, C, None, true);
    const STALE: f32 = 999.0;
    for r in m.world.regions.values_mut() {
        r.population_state.factors[0] = STALE;
    }
    run_round_start(&mut m);
    let factor = |m: &CampaignModel, id: u32| m.world.regions[&RegionId(id)].population_state.factors[0];
    assert_ne!(factor(&m, 10), STALE, "A's region is refreshed");
    assert_eq!(factor(&m, 11), STALE, "the rebels' region keeps its factors");
    assert_eq!(factor(&m, 12), STALE, "the out-of-game faction's region keeps its factors");
    for r in m.world.regions.values_mut() {
        r.population_state.factors[0] = STALE;
    }
    m.turn.queue.push_front(TurnStep::RoundEndRefresh);
    m.step().expect("the round end's refresh step");
    assert_ne!(factor(&m, 10), STALE, "A's region is refreshed at the round end");
    assert_eq!(factor(&m, 11), STALE, "the rebels' region keeps its factors at the round end");
    assert_eq!(factor(&m, 12), STALE, "the out-of-game faction's region keeps its factors at the round end");
}

/// The computed factors (`0x008BAF30` → `0x00B71E90`) are reset only on relationships owned by a listed
/// faction in the game: the rebels' (`IsFactionWithoutRecord` `0x008CEEF0`) and an out-of-game faction's
/// (+0x824) keep theirs.
#[test]
fn the_computed_factors_skip_the_rebels_and_out_of_game_owners() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    set_details(&mut m, C, None, true);
    for (o, t) in [(A, C), (B, A), (C, A)] {
        m.relationship_mut(o, t).factor_mut("faction_leader").reset(77);
    }
    run_round_start(&mut m);
    let leader = |m: &CampaignModel, o, t| m.world.relationships[&(o, t)].attitudes[super::super::treaties::slot("faction_leader")].value;
    assert_eq!(leader(&m, A, C), 0, "A's record is reset from C's effects (none)");
    assert_eq!(leader(&m, B, A), 77, "the rebels' record is left alone");
    assert_eq!(leader(&m, C, A), 77, "the out-of-game faction's record is left alone");
}

/// The round start's trade update `0x00BCB020(0)` recomputes the prices and trends without moving the price
/// history; the round end's `0x00BCB020(1)` (`PUSH 0x1` at `0x00948E0E`) moves it.
#[test]
fn the_round_start_price_update_keeps_the_price_history() {
    // D = 4000 + 8000 + 2000 (c0 = GDP), S = A's domestic supply 100, f = 1: price 140.
    let mut m = split_model(&["c0"], vec![100]);
    m.world.commodity_market = super::super::trade::CommodityMarket {
        initial: vec![10],
        factors: vec![1.0],
        previous: vec![100],
        previous2: vec![120],
        trend: vec![9],
    };
    run_round_start(&mut m);
    let mk = &m.world.commodity_market;
    assert_eq!(m.world.commodity_prices, vec![140]);
    assert_eq!((mk.previous.clone(), mk.previous2.clone(), mk.trend.clone()), (vec![100], vec![120], vec![0]), "140 > 1.2 × 100");
    m.update_commodity_prices();
    let mk = &m.world.commodity_market;
    assert_eq!((mk.previous.clone(), mk.previous2.clone()), (vec![120], vec![140]), "the round end shifts the history");
}

/// [`split_model`] with the price history of [`the_round_start_price_update_keeps_the_price_history`] (the
/// update gives 140, trend 0) and the prices still at 10.
fn price_model() -> CampaignModel {
    let mut m = split_model(&["c0"], vec![100]);
    m.world.commodity_market = super::super::trade::CommodityMarket {
        initial: vec![10],
        factors: vec![1.0],
        previous: vec![100],
        previous2: vec![120],
        trend: vec![9],
    };
    m
}

fn price_state(m: &CampaignModel) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    let mk = &m.world.commodity_market;
    (m.world.commodity_prices.clone(), mk.previous.clone(), mk.previous2.clone(), mk.trend.clone())
}

/// A region lost mid-turn by a faction other than the rebels runs the price update with flag 0 at the end of
/// the route loss (`0x00BCB020(0)` at `0x00BBE7C4`): prices and trends recomputed, the history kept.
#[test]
fn a_region_lost_mid_turn_updates_the_prices_keeping_the_history() {
    let mut m = price_model();
    assert!(m.transfer_region(RegionId(12), A), "C hands region 12 over");
    assert_eq!(price_state(&m), (vec![140], vec![100], vec![120], vec![0]));
}

/// A region the rebels lose runs no route loss (`0x00A64E87` is skipped), so only a port slot passing to the
/// new owner runs the price update (`SetSettlementOwner` `0x00B2B810` → `0x00BCB020(0)` at `0x00B2B84F`); a
/// port a navy is docked in does not pass (`0x00B1B300`), so it runs none.
#[test]
fn a_port_slot_passing_updates_the_prices() {
    let r12 = RegionId(12);
    let rebel_held = |port: bool, docked: bool| {
        let mut m = price_model();
        make_rebels(&mut m, C);
        let mut slot = m.world.regions[&RegionId(10)].slots[1].clone();
        (slot.port, slot.position) = (port, Some(pos(5, 5)));
        m.world.regions.get_mut(&r12).unwrap().slots.push(slot);
        if docked {
            let admiral = Character { id: CharacterId(109), faction: B, kind: CharacterKind::Admiral, position: pos(5, 5), movement_points: 0, max_movement_points: 0, base_movement_points: 0, garrisoned_in: None };
            m.world.characters.insert(admiral.id, admiral);
            m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: B, commander: Some(CharacterId(109)), units: vec![unit(9)], is_navy: true });
        }
        assert!(m.transfer_region(r12, A), "the rebels lose region 12");
        price_state(&m)
    };
    let unchanged = (vec![10], vec![100], vec![120], vec![9]);
    assert_eq!(rebel_held(false, false), unchanged, "no port slot: no update");
    assert_eq!(rebel_held(true, false), (vec![140], vec![100], vec![120], vec![0]), "the port passes: flag 0");
    assert_eq!(rebel_held(true, true), unchanged, "the docked port stays: no update");
}

/// A handover to the region's own owner (a deal naming it, `0x00B58A10` → `0x00B449F0`) loses no route
/// (`0x00A64AC0` returns at `0x00A64D13` first) but still passes the slots (`0x00B1B300` at `0x00B44B69`):
/// a port slot runs the price update with flag 0, a region without one none.
#[test]
fn a_same_owner_handover_updates_the_prices_only_through_a_port() {
    let r12 = RegionId(12);
    let same_owner = |port: bool| {
        let mut m = price_model();
        let mut slot = m.world.regions[&RegionId(10)].slots[1].clone();
        slot.port = port;
        m.world.regions.get_mut(&r12).unwrap().slots.push(slot);
        assert!(!m.transfer_region(r12, C), "C already owns region 12");
        assert_eq!(m.world.regions[&r12].owner, C);
        price_state(&m)
    };
    assert_eq!(same_owner(false), (vec![10], vec![100], vec![120], vec![9]), "no port slot: no update");
    assert_eq!(same_owner(true), (vec![140], vec![100], vec![120], vec![0]), "the port passes: flag 0");
}

/// [`price_model`] with a slot (a port when `port`) at (5, 5) in C's region 12, held by B.
fn price_model_with_held_slot(port: bool) -> CampaignModel {
    let mut m = price_model();
    let mut slot = m.world.regions[&RegionId(10)].slots[1].clone();
    (slot.port, slot.position, slot.holder) = (port, Some(pos(5, 5)), Some(B));
    m.world.regions.get_mut(&RegionId(12)).unwrap().slots.push(slot);
    m
}

/// A docked navy leaving its port, by undocking or by dying, hands the slot back to the region's owner through
/// `RestorePortSlotHolderToRegionOwner` (`0x00A8D830`, the call `0x00A8D859`), whose `SetSettlementOwner`
/// (`0x00B2B810`) runs the price update with flag 0 for a port slot (`0x00BCB020(0)` at `0x00B2B84F`). A navy
/// standing at a slot that is no port is docked nowhere: no update.
#[test]
fn a_navy_leaving_its_port_updates_the_prices() {
    let leaves = |port: bool, dies: bool| {
        let mut m = price_model_with_held_slot(port);
        let admiral = Character { id: CharacterId(109), faction: B, kind: CharacterKind::Admiral, position: pos(5, 5), movement_points: 0, max_movement_points: 0, base_movement_points: 0, garrisoned_in: None };
        m.world.characters.insert(admiral.id, admiral);
        m.world.forces.insert(ForceId(1005), MilitaryForce { id: ForceId(1005), faction: B, commander: Some(CharacterId(109)), units: vec![unit(9)], is_navy: true });
        if dies {
            m.destroy_force(ForceId(1005), &mut Vec::new());
        } else {
            m.undock(ForceId(1005));
        }
        if port {
            assert_eq!(m.world.regions[&RegionId(12)].slots.last().unwrap().holder, None, "the port goes back to C");
        }
        price_state(&m)
    };
    let (updated, unchanged) = ((vec![140], vec![100], vec![120], vec![0]), (vec![10], vec![100], vec![120], vec![9]));
    assert_eq!(leaves(true, false), updated, "undocking: flag 0");
    assert_eq!(leaves(true, true), updated, "the navy's destructor: flag 0");
    assert_eq!(leaves(false, false), unchanged, "no port: no update");
}

/// An AI faction leaving the game hands back the town slots it held in other factions' regions; a port slot
/// runs the price update with flag 0 (`0x00B794B0` from `0x008ABB2D`/`0x008ABB3D` → `SetSettlementOwner`
/// `0x00B2B810` → `0x00BCB020(0)` at `0x00B2B84F`); a slot that is no port runs none.
#[test]
fn a_destroyed_faction_handing_back_a_port_updates_the_prices() {
    let destroyed = |port: bool| {
        let mut m = price_model_with_held_slot(port);
        m.world.regions.get_mut(&RegionId(11)).unwrap().owner = A;
        m.destroy_faction(B, &mut Vec::new());
        assert_eq!(m.world.regions[&RegionId(12)].slots.last().unwrap().holder, None, "the slot goes back to C");
        price_state(&m)
    };
    assert_eq!(destroyed(true), (vec![140], vec![100], vec![120], vec![0]), "the port passes: flag 0");
    assert_eq!(destroyed(false), (vec![10], vec![100], vec![120], vec![9]), "no port: no update");
}

/// The faction registry (`FindCampaignFactionByKey` 0x00955B70) holds every faction of the turn
/// list, dead or out of the game included (0x008AB6C0 only flags it, the list keeps it), and never
/// the rebels (world +0x1C, outside the list): `registry_faction` finds a destroyed faction and an
/// unlisted one, but not the rebels under their empty key nor an unknown key.
#[test]
fn the_faction_registry_holds_every_faction_but_the_rebels() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    m.world.turn_order = vec![B, A];
    m.world.faction_details.entry(A).or_default().out_of_game = Some(true);
    assert_eq!(m.registry_faction("test_faction_a").map(|f| f.id), Some(A), "out of the game");
    assert_eq!(m.registry_faction("test_faction_c").map(|f| f.id), Some(C), "not in turn_order");
    assert_eq!(m.faction_by_key("").map(|f| f.id), Some(B), "the rebels have no key");
    assert!(m.registry_faction("").is_none(), "the rebels");
    assert!(m.registry_faction("rebels").is_none(), "no faction has the key \"rebels\"");
    assert!(m.registry_faction("nobody").is_none(), "an unknown key");
}

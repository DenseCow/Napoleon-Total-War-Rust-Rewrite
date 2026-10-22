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
pub(super) fn make_rebels(m: &mut CampaignModel, f: FactionId) {
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
pub(super) fn price_model() -> CampaignModel {
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

pub(super) fn price_state(m: &CampaignModel) -> (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>) {
    let mk = &m.world.commodity_market;
    (m.world.commodity_prices.clone(), mk.previous.clone(), mk.previous2.clone(), mk.trend.clone())
}

/// Ending a siege runs the price update with flag 0 (`0x008BD550` → `0x00BCB020(0)` at `0x008BD65B`), from
/// every path (here the call itself and a destroyed besieger, `0x0088CE30` → `0x008BD550`); a force that
/// holds no siege runs none.
#[test]
fn ending_a_siege_updates_the_prices_keeping_the_history() {
    let besieged = || {
        let mut m = price_model();
        m.world.regions.get_mut(&RegionId(10)).unwrap().siege.besiegers = vec![ForceId(1001)];
        m
    };
    let updated = (vec![140], vec![100], vec![120], vec![0]);
    let mut m = besieged();
    m.end_force_siege(ForceId(1000));
    assert_eq!(price_state(&m), (vec![10], vec![100], vec![120], vec![9]), "no siege held: no update");
    m.end_force_siege(ForceId(1001));
    assert!(!m.world.regions[&RegionId(10)].siege.under_siege());
    assert_eq!(price_state(&m), updated);
    let mut m = besieged();
    m.destroy_force(ForceId(1001), &mut Vec::new());
    assert!(!m.world.regions[&RegionId(10)].siege.under_siege());
    assert_eq!(price_state(&m), updated, "a destroyed besieger");
}

/// A region lost mid-turn by a faction other than the rebels runs the price update with flag 0 at the end of
/// the route loss (`0x00BCB020(0)` at `0x00BBE7C4`): prices and trends recomputed, the history kept.
#[test]
fn a_region_lost_mid_turn_updates_the_prices_keeping_the_history() {
    let mut m = price_model();
    assert!(m.transfer_region(RegionId(12), A, &mut Vec::new()), "C hands region 12 over");
    assert_eq!(price_state(&m), (vec![140], vec![100], vec![120], vec![0]));
}

/// The route loss's price update reads the demand the settlement's `SetSettlementOwner` rebuilt before
/// `TransferRegionToFaction` (`0x00B44ACC` → `0x00AB49F0(0)` at `0x00B2B85F`), not a demand stored earlier.
#[test]
fn the_route_loss_reads_the_demand_rebuilt_before_the_transfer() {
    let mut m = price_model();
    m.world.regions.get_mut(&RegionId(12)).unwrap().commodity_demand = vec![99_999];
    assert!(m.transfer_region(RegionId(12), A, &mut Vec::new()), "C hands region 12 over");
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
        assert!(m.transfer_region(r12, A, &mut Vec::new()), "the rebels lose region 12");
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
        assert!(!m.transfer_region(r12, C, &mut Vec::new()), "C already owns region 12");
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

/// regionwalk: the faction turn start updates the faction's regions in its region list order (faction
/// +0x778 count / +0x77C data, walked front to back at `0x008F2AF6` → `0x00AAE820`), not by id: A owns
/// regions 10 and 12, listed 12 first.
#[test]
fn the_turn_start_updates_the_regions_in_the_faction_list_order() {
    let region_starts = |order: [u32; 3]| {
        let mut m = test_model();
        m.turn.humans = vec![A];
        m.world.region_order = order.map(RegionId).to_vec();
        let ev = m.start_campaign();
        // A's turn start comes last in the start events (B plays first as AI).
        let a_start = ev.iter().rposition(|e| matches!(e, CampaignEvent::CharacterTurnStart { .. })).unwrap();
        ev[a_start..]
            .iter()
            .filter_map(|e| match e {
                CampaignEvent::RegionTurnStart { region } => Some(region.raw()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(region_starts([10, 11, 12]), vec![10, 12]);
    assert_eq!(region_starts([12, 11, 10]), vec![12, 10], "not id order");
}

/// regionwalk fix 1: the faction turn end walks the same region list (`0x008BD1E1` → `0x00A786C0`), not
/// the ids. A region gained in play joins the list's end (`0x008CF7D0`); a lost one leaves it and the
/// rest keep their order (`0x008B0210`).
#[test]
fn the_turn_end_walks_the_faction_list_with_gains_last_and_losses_dropped() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.regions.insert(RegionId(13), region(13, A, 1000, 0));
    // A's list: 13, 12, 10 (id order would be 10, 12, 13).
    m.world.region_order = [13, 12, 11, 10].map(RegionId).to_vec();
    m.start_campaign();
    assert!(m.transfer_region(RegionId(11), A, &mut Vec::new()), "gained: to the end");
    assert_eq!(m.world.faction_regions_in_list_order(A), [13, 12, 10, 11].map(RegionId));
    assert!(m.transfer_region(RegionId(12), B, &mut Vec::new()), "lost: removed");
    assert_eq!(m.world.faction_regions_in_list_order(A), [13, 10, 11].map(RegionId));
    let region_ends: Vec<u32> = a_turn_end(&mut m)
        .iter()
        .filter_map(|e| match e {
            CampaignEvent::RegionTurnEnd { region } => Some(region.raw()),
            _ => None,
        })
        .collect();
    assert_eq!(region_ends, vec![13, 10, 11]);
}

/// rebelresearch: the faction turn start opens the technologies whose building the faction owns
/// (research availability `0x008F91F0`, through `0x008AE3B0` at `0x008F2B40`), but not for the rebels:
/// their branch (`IsFactionWithoutRecord` `0x008CEEF0`, the JNZ at `0x008F28E1` → `0x008F2B47`) walks
/// only the characters and the regions and joins past it at `0x008F2BB1`. A and the rebels (B) each own
/// a region with the test building and hold one technology that needs it, unavailable.
#[test]
fn the_rebels_turn_start_skips_research_availability() {
    use super::research::state;
    use super::rules::TechRules;
    let mut m = test_model();
    make_rebels(&mut m, B);
    m.turn.humans = vec![C];
    set_details(&mut m, C, None, false);
    let mut rules = (*m.rules).clone();
    rules.technologies.insert("t1".into(), TechRules { cost: 25, building_level: "test_building_level".into(), requires: vec![], factions: vec![], thread: 2, ..Default::default() });
    m.rules = Arc::new(rules);
    for f in [A, B] {
        m.world.faction_details.entry(f).or_default().technologies = vec![("t1".into(), state::UNAVAILABLE)];
    }
    // Turn order A, C (human), then the rebels B: A's turn start runs before C stops.
    m.start_campaign();
    assert_eq!(m.tech_state(A, "t1"), Some(state::AVAILABLE), "a normal faction's turn start opens it");
    assert_eq!(m.tech_state(B, "t1"), Some(state::UNAVAILABLE));
    // Step C's turn end and the rebels' turn start, up to their FactionTurnStart (before the round end).
    m.begin_end_turn();
    let mut rebels_started = false;
    while let Some(ev) = m.step() {
        if ev.iter().any(|e| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == B)) {
            rebels_started = true;
            break;
        }
    }
    assert!(rebels_started, "the rebels' turn start ran");
    assert_eq!(m.tech_state(B, "t1"), Some(state::UNAVAILABLE), "the rebels' turn start leaves it");
}

/// rebelresearch: the rebels' turn start skips the reinforcement manager too (`0x00B71FA0` at `0x008F2B35`
/// sits on the branch their JNZ at `0x008F28E1` jumps past, to `0x008F2B47`): a rebel army raised for a
/// rebel commander one step away neither marches to him nor loses its entry.
#[test]
fn the_rebels_turn_start_skips_reinforcement_march() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    m.turn.humans = vec![C];
    set_details(&mut m, C, None, false);
    // B's colonel 101 leads army 1001 at (0, 0); B's general 104 leads army 1004 at (1, 0).
    let mut general = character(104, B, CharacterKind::General, 30);
    general.position = pos(1, 0);
    m.world.characters.insert(general.id, general);
    m.world.forces.insert(
        ForceId(1004),
        MilitaryForce { id: ForceId(1004), faction: B, commander: Some(CharacterId(104)), units: vec![unit(5)], is_navy: false },
    );
    let entry = Reinforcement { source: CharacterId(101), target: CharacterId(104), settlement: RegionId(11) };
    m.world.reinforcements = vec![entry];
    // Turn order A, C (human), then the rebels B.
    m.start_campaign();
    m.begin_end_turn();
    let mut rebels_started = false;
    while let Some(ev) = m.step() {
        assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CampaignArmiesMerge { .. })), "nothing merges: {ev:?}");
        if ev.iter().any(|e| matches!(e, CampaignEvent::FactionTurnStart { faction } if *faction == B)) {
            rebels_started = true;
            break;
        }
    }
    assert!(rebels_started, "the rebels' turn start ran");
    assert_eq!(m.world.reinforcements, vec![entry], "the entry is kept");
    assert_eq!(m.world.forces[&ForceId(1001)].units.len(), 2, "the army stays where it was");
    assert_eq!(m.world.forces[&ForceId(1004)].units.len(), 1);
    assert_eq!(m.world.characters[&CharacterId(101)].position, pos(0, 0));
}

/// rebelroundend: the round-end economy (`0x008BC650`) settles only a faction with a record still in the game
/// (`0x008CEEF0` at `0x008BC65D`, faction +0x824 at `0x008BC666`, both before `0x00BABE30`): the rebels and a
/// faction out of the game, each with a running record worth 50, keep their treasury and record ring, while
/// the rebels' regions still take their town wealth step (`0x008BC685` tests only +0x824 before `0x00AB42F0`).
#[test]
fn the_round_end_economy_skips_the_rebels_settle() {
    let mut m = test_model();
    make_rebels(&mut m, B);
    m.turn.humans = vec![A];
    assert!(!m.in_the_game(C) && m.in_the_game(B) && m.world.regions[&RegionId(11)].owner == B);
    m.start_campaign();
    m.begin_end_turn();
    // Up to the first faction's economy: the round end's first pass has run.
    while m.turn.queue.front().is_some_and(|s| !matches!(s, TurnStep::Economy(_))) {
        m.step();
    }
    for f in [A, B, C] {
        let mut record = [0; 25];
        record[11] = 50;
        m.world.economy_history.insert(f, vec![record]);
        m.world.factions.get_mut(&f).unwrap().treasury = 1000;
    }
    m.world.regions.get_mut(&RegionId(11)).unwrap().town_wealth_growth = i32::MIN;
    while let Some(TurnStep::Economy(_)) = m.turn.queue.front() {
        m.step();
    }
    assert_eq!(m.world.economy_history[&A].len(), 2, "A settled and rolled its record");
    assert_ne!(m.world.factions[&A].treasury, 1000, "A was paid");
    for f in [B, C] {
        assert_eq!(m.world.factions[&f].treasury, 1000, "{f:?} pays nothing");
        assert_eq!(m.world.economy_history[&f].len(), 1, "{f:?} keeps its record ring");
    }
    assert_ne!(m.world.regions[&RegionId(11)].town_wealth_growth, i32::MIN, "the rebels' region growth was recomputed");
}

/// rebelroundend: the round-end economy steps the relationships' per-turn update (`0x00B29100`) and research
/// (`0x008F3C40`, whose end refreshes the technology availability) only for a faction with a record still in
/// the game (the JNZs at `0x008BC869` / `0x008BC871`): A's alliance commitment counts down and its technology
/// opens, the rebels' keep theirs.
#[test]
fn the_round_end_economy_skips_the_rebels_relations_and_research() {
    use super::research::state;
    use super::rules::{BuildingRules, TechRules};
    let mut m = test_model();
    make_rebels(&mut m, B);
    let mut rules = (*m.rules).clone();
    rules.buildings.insert("admin_1".into(), BuildingRules { chain: "sAdmin".into(), level: 1, chain_thread: 2, ..Default::default() });
    rules.technologies.insert("admin_t".into(), TechRules { cost: 10, building_level: "admin_1".into(), ..Default::default() });
    m.rules = Arc::new(rules);
    for (f, region) in [(A, 10), (B, 11)] {
        m.world.regions.get_mut(&RegionId(region)).unwrap().slots[0].building = Some(BuildingRef { level_key: "admin_1".into(), health: 100 });
        m.world.faction_details.entry(f).or_default().technologies = vec![("admin_t".into(), state::UNAVAILABLE)];
    }
    // The rebels hold a record of their own here (the startpos has none), so a wrongly stepped one shows.
    m.relationship_mut(A, B).alliance_commitment_turns = 5;
    m.relationship_mut(B, A).alliance_commitment_turns = 5;
    m.turn.humans = vec![A];
    m.start_campaign();
    m.begin_end_turn();
    while m.turn.queue.front().is_some_and(|s| !matches!(s, TurnStep::Economy(_))) {
        m.step();
    }
    while let Some(TurnStep::Economy(_)) = m.turn.queue.front() {
        m.step();
    }
    assert_eq!(m.world.relationships[&(A, B)].alliance_commitment_turns, 4, "A's record stepped");
    assert_eq!(m.world.relationships[&(B, A)].alliance_commitment_turns, 5, "the rebels' record did not");
    assert_eq!(m.tech_state(A, "admin_t"), Some(state::AVAILABLE), "A's research step refreshed its availability");
    assert_eq!(m.tech_state(B, "admin_t"), Some(state::UNAVAILABLE), "the rebels' did not");
}

/// The events of the human A's turn end (`0x008BD0F0`) and nothing else: ends A's turn and runs only
/// its first step (`TurnStep::FactionEnd`), which closes with `FactionTurnEnd`.
fn a_turn_end(m: &mut CampaignModel) -> Vec<CampaignEvent> {
    assert_eq!((m.turn.current, m.turn.in_turn, m.has_pending_steps()), (Some(A), true, false), "A's turn, nothing queued");
    m.begin_end_turn();
    let end = m.step().expect("A's turn end is queued");
    assert_eq!(end.last(), Some(&CampaignEvent::FactionTurnEnd { faction: A }), "the step is A's whole turn end");
    end
}

/// charwalk: the faction turn start (`0x008F2AC4` → `0x00A24EB0`) and turn end (`0x008BD1AF` →
/// `0x009DA210`) walk the faction's character list (+0x764 / +0x768) front to back, not the ids. A
/// character made in play joins the list's end (`0x008CF6B0`); a removed one leaves it and the rest keep
/// their order (`0x008B0150`).
#[test]
fn the_turn_walks_the_faction_character_list_with_gains_last_and_losses_dropped() {
    let mut m = test_model();
    m.turn.humans = vec![A];
    m.world.characters.insert(CharacterId(103), character(103, A, CharacterKind::General, 30));
    // A's list: 103, 102, 100 (id order would be 100, 102, 103); 101 is B's.
    m.world.character_order = [103, 101, 102, 100].map(CharacterId).to_vec();
    m.start_campaign();
    let made = CharacterId(m.world.alloc_id() as i32);
    m.world.characters.insert(made, character(made.0, A, CharacterKind::Colonel, 20));
    assert!(m.world.remove_character(CharacterId(102)).is_some(), "lost: removed");
    let expected = vec![CharacterId(103), CharacterId(100), made];
    assert_eq!(m.world.faction_characters_in_list_order(A), expected, "made in play: at the end");
    let of_a = |m: &CampaignModel, ev: &[CampaignEvent], end: bool| -> Vec<CharacterId> {
        ev.iter()
            .filter_map(|e| match *e {
                CampaignEvent::CharacterTurnEnd { character } if end => Some(character),
                CampaignEvent::CharacterTurnStart { character } if !end => Some(character),
                _ => None,
            })
            .filter(|c| m.world.characters.get(c).is_some_and(|x| x.faction == A))
            .collect()
    };
    let end = a_turn_end(&mut m);
    assert_eq!(of_a(&m, &end, true), expected, "turn end");
    // The rest of the round, then A's next turn start.
    let next = m.run_pending();
    assert!(next.contains(&CampaignEvent::FactionTurnStart { faction: A }), "A's next turn started");
    assert_eq!(of_a(&m, &next, false), expected, "turn start");
}

/// idwalks: the round end's population step walks the faction region list too (`0x008BC68E`–`0x008BC6C3` →
/// `UpdateRegionRoundEnd` `0x00AB42F0`), not the ids: a MADE-UP mod growth rule that logs each region it
/// grows in mod state sees a region gained in play last.
#[test]
fn the_population_round_end_walks_the_faction_list_with_gains_last() {
    use super::super::mod_state::{ModKey, ModScope, ModValue, ModWrites};
    use super::super::population::PopulationState;
    use super::super::seams::{CampaignRuleRegistry, PopulationGrow, POPULATION_GROW};
    const OWNER: &str = "test_mod.walk_log";
    let mut reg = CampaignRuleRegistry::default();
    let maker = |prev: Arc<PopulationGrow>| {
        Arc::new(move |m: &CampaignModel, r: &Region, pop: u32, s: &PopulationState, w: &mut ModWrites| {
            let key = ModKey::new(OWNER, ModScope::Campaign, "order");
            let mut order = match m.mod_state.get(&key) {
                Some(ModValue::List(v)) => v.clone(),
                _ => Vec::new(),
            };
            order.push(ModValue::Int(i64::from(r.id.raw())));
            w.set(key, ModValue::List(order));
            prev(m, r, pop, s, w)
        }) as Arc<PopulationGrow>
    };
    assert!(reg.population_grow.register(OWNER, Arc::new(maker)));
    let mut warnings = Vec::new();
    let seams = reg.seams(&[(POPULATION_GROW.to_owned(), OWNER.to_owned())], &mut warnings);
    assert!(warnings.is_empty());
    let mut m = test_model();
    let mut rules = (*m.rules).clone();
    rules.seams = seams;
    m.rules = Arc::new(rules);
    m.world.regions.insert(RegionId(13), region(13, A, 1000, 0));
    // A's list: 13, 12, 10 (id order would be 10, 12, 13), then 11 gained in play.
    m.world.region_order = [13, 12, 11, 10].map(RegionId).to_vec();
    assert!(m.transfer_region(RegionId(11), A, &mut Vec::new()), "gained: to the end");
    m.population_round_end(A);
    let order = m.mod_state.get(&ModKey::new(OWNER, ModScope::Campaign, "order")).cloned();
    assert_eq!(order, Some(ModValue::List([13, 12, 10, 11].map(ModValue::Int).to_vec())));
}

/// charloops: the faction turn start runs the spotting manager (a human's spy-network step `0x008F2480`)
/// after its regions and the faction-level updates, at `0x008F2C3C` past the join `0x008F2BB1`, then the
/// standing orders (`0x008F2C5B` → `0x009653D0`) and the pools' step (`0x008F2C99` → `0x00A25310`), and
/// `FactionTurnStart` last; not with the characters' turn starts (`0x008F2AC4`).
#[test]
fn the_spy_network_step_comes_after_the_regions_and_before_the_faction_turn_start() {
    let mut m = agents_model();
    m.world.character_details.get_mut(&CharacterId(103)).unwrap().idle_turns = 3;
    m.turn.humans = vec![A];
    let ev = m.start_campaign();
    // A's turn start comes last in the start events (B plays first as AI).
    let a_chars = ev.iter().position(|e| matches!(e, CampaignEvent::CharacterTurnStart { character } if *character == CharacterId(103))).unwrap();
    let network = ev.iter().position(|e| *e == CampaignEvent::CharacterBuildsSpyNetwork { character: CharacterId(103) }).unwrap();
    for r in [10, 12] {
        assert_eq!(m.world.regions[&RegionId(r)].owner, A, "region {r} belongs to A");
    }
    let last_region = ev.iter().rposition(|e| matches!(e, CampaignEvent::RegionTurnStart { region } if [10, 12].contains(&region.raw()))).unwrap();
    let a_start = ev.iter().position(|e| *e == CampaignEvent::FactionTurnStart { faction: A }).unwrap();
    assert!(a_chars < last_region, "A's regions come after its characters");
    assert!(last_region < network, "the spy network after A's regions");
    assert!(network < a_start, "and before FactionTurnStart");
}

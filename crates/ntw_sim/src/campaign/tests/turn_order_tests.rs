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
/// (`0x008F1F60` skips step 1 for the rebels, `0x008CEEF0`, and an out-of-game faction, +0x824); the round
/// end's first pass (`0x008E1030` → `0x00AB3E70`) refreshes every region.
#[test]
fn the_round_start_refreshes_only_listed_in_game_factions_regions() {
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
    m.refresh_population_factors();
    let after = [10, 11, 12].map(|id| factor(&m, id));
    assert!(after.iter().all(|&g| g != STALE), "the round end's pass refreshes every region: {after:?}");
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

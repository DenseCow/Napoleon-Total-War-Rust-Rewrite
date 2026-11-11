//! Campaign model tests: another faction's garrison in a settlement (one an owner change does not
//! expel: the rebels, a faction out of the game or one without a capital). The owner's army
//! arriving, the settlement's defence and the owner's recruits (analysis/fidelity/CAMPAIGN_FIDELITY.md,
//! "Foreign garrisons").

use super::*;
use crate::campaign::commands::{straight_distance, LandRecruitPlace, CONTACT_DISTANCE};

/// [`test_model`] with B's army 1001 (2 units) as the garrison of A's region 10 (settlement at
/// x 10) and A's army 1000 out of reach at x 30.
fn foreign_garrison_model() -> CampaignModel {
    let mut m = test_model();
    let g = m.world.characters.get_mut(&CharacterId(101)).unwrap();
    g.position = pos(10, 0);
    g.garrisoned_in = Some(RegionId(10));
    m.world.regions.get_mut(&RegionId(10)).unwrap().garrison = Some(ForceId(1001));
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(30, 0);
    m
}

/// Adds a general `id` of `faction` at `at` leading force `id + 900` of `units` units.
fn add_force(m: &mut CampaignModel, id: i32, faction: FactionId, at: (Fixed20, Fixed20), units: i32, is_navy: bool) -> ForceId {
    let mut c = character(id, faction, CharacterKind::General, 20);
    c.position = at;
    m.world.characters.insert(c.id, c);
    let force = ForceId(id as u32 + 900);
    let units = (0..units).map(|i| unit(id * 100 + i)).collect();
    m.world.forces.insert(force, MilitaryForce { id: force, faction, commander: Some(CharacterId(id)), units, is_navy });
    force
}

/// The land recruit with another faction's garrison inside: the exe's room check (`0x00B44ED0` →
/// residence +0x78) ignores the garrison's faction and puts the owner's unit into it (ORIGINAL
/// BUG); ours treats it as no room, and with no owner army near the unit raises a new army of
/// the owner outside the settlement (`0x00B43A60` → `0x008D25F0`).
#[test]
fn a_recruit_never_joins_a_foreign_garrison() {
    let mut m = foreign_garrison_model();
    let Some(CampaignEvent::UnitTrained { force, .. }) = m.spawn_recruited_unit(RegionId(10), "test_unit".into()) else { panic!() };
    assert_ne!(force, ForceId(1001));
    assert_eq!(m.world.forces[&ForceId(1001)].units.len(), 2);
    let f = &m.world.forces[&force];
    assert_eq!((f.faction, f.units.len()), (A, 1));
    let colonel = &m.world.characters[&f.commander.unwrap()];
    assert_eq!((colonel.garrisoned_in, colonel.position), (None, pos(10, 0)));
    assert_eq!(m.world.regions[&RegionId(10)].garrison, Some(ForceId(1001)));
}

/// With no room in the garrison (foreign, or the owner's and full) the recruit joins the owner's
/// army with the most units that has room and whose commander stands within 5.0 map units either
/// side of the settlement, edges included (`0x00B43A60`); equal sizes take the lowest force id.
#[test]
fn a_recruit_joins_the_fullest_owner_army_near_the_settlement() {
    for own_full_garrison in [false, true] {
        let mut m = foreign_garrison_model();
        if own_full_garrison {
            let g = add_force(&mut m, 120, A, pos(10, 0), 20, false);
            m.world.characters.get_mut(&CharacterId(120)).unwrap().garrisoned_in = Some(RegionId(10));
            m.world.regions.get_mut(&RegionId(10)).unwrap().garrison = Some(g);
        }
        let edge = add_force(&mut m, 103, A, pos(15, 0), 3, false);
        let tie = add_force(&mut m, 104, A, (Fixed20::from_int(10), Fixed20::from_int(-5)), 3, false);
        add_force(&mut m, 105, A, (Fixed20::from_f64(15.01), Fixed20::from_int(0)), 10, false);
        add_force(&mut m, 106, A, pos(11, 0), 20, false);
        add_force(&mut m, 107, B, pos(10, 1), 5, false);
        add_force(&mut m, 108, A, pos(10, 1), 5, true);
        let trained = |m: &mut CampaignModel| match m.spawn_recruited_unit(RegionId(10), "test_unit".into()) {
            Some(CampaignEvent::UnitTrained { force, .. }) => force,
            e => panic!("{e:?}"),
        };
        assert_eq!(trained(&mut m), edge, "the tie of 3 goes to the lower id; out of the box, full, B's and a navy are skipped");
        assert_eq!(m.world.forces[&edge].units.len(), 4);
        assert_eq!(trained(&mut m), edge, "now the fullest");
        m.world.forces.get_mut(&tie).unwrap().units.extend((0..3).map(|i| unit(90_000 + i)));
        assert_eq!(trained(&mut m), tie);
        assert_eq!(m.world.forces[&ForceId(1001)].units.len(), 2);
    }
}

/// The owner's army ordered into its settlement while another faction's garrison holds it: the exe
/// moves its units into that army (`0x0094D660` → `0x008D7310`, no faction check; ORIGINAL BUG);
/// ours stops outside and nothing changes hands.
#[test]
fn the_owners_army_waits_outside_a_foreign_garrison() {
    let mut m = foreign_garrison_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(5, 0);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(10) }).unwrap();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CampaignArmiesMerge { .. } | CampaignEvent::CharacterEntersGarrison { .. })), "{ev:?}");
    assert_eq!((m.world.forces[&ForceId(1000)].units.len(), m.world.forces[&ForceId(1001)].units.len()), (1, 2));
    let ch = &m.world.characters[&CharacterId(100)];
    assert_eq!(ch.garrisoned_in, None);
    assert!(ch.position != pos(5, 0) && straight_distance(ch.position, pos(10, 0)) <= CONTACT_DISTANCE, "walked up, not in");
    assert_eq!(m.world.regions[&RegionId(10)].garrison, Some(ForceId(1001)));
}

/// An attack on the settlement needs war with its owner alone (`0x00917D20` → `0x009D0980`), and the
/// foreign garrison defends it (`0x0094B600` → `0x008E1A90` → `0x00883680`, the defending side led by
/// the garrison's commander, residence +0x24 `0x00B3F890`).
#[test]
fn a_foreign_garrison_defends_the_settlement() {
    let mut m = foreign_garrison_model();
    let attacker = add_force(&mut m, 110, C, pos(9, 0), 1, false);
    m.apply(CampaignCommand::DeclareWar { a: C, b: A }).unwrap();
    assert_ne!(m.world.stance(C, B), Stance::War);
    assert_eq!(m.defenders_of(RegionId(10)), vec![ForceId(1001)]);
    let ev = m.apply(CampaignCommand::EnterSettlement { force: attacker, region: RegionId(10) }).unwrap();
    assert!(ev.contains(&CampaignEvent::PreBattle { attacker }), "{ev:?}");
    let battle = m.pending_battle.as_ref().expect("a battle");
    assert_eq!((battle.defenders.clone(), battle.settlement), (vec![ForceId(1001)], Some(RegionId(10))));
}

/// [`test_model`] with A's army `id + 900` of `units` units as the garrison of A's region 10 and A's
/// army 1000 (1 unit) next to the settlement.
fn own_garrison_model(id: i32, units: i32) -> (CampaignModel, ForceId) {
    let mut m = test_model();
    let g = add_force(&mut m, id, A, pos(10, 0), units, false);
    m.world.characters.get_mut(&CharacterId(id)).unwrap().garrisoned_in = Some(RegionId(10));
    m.world.regions.get_mut(&RegionId(10)).unwrap().garrison = Some(g);
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(9, 0);
    (m, g)
}

/// The script's `join_garrison` (`0x0097B520` → `0x009549B0`, enter flag set) sends another
/// faction's army into a settlement with no garrison: the force fit `0x00B16700` passes with no
/// garrison, and the arrival `0x0094CFF0` enters it without a capture. The map's own order
/// (`EnterSettlement`) still needs war with the owner.
#[test]
fn an_allied_army_joins_an_empty_settlement() {
    let mut m = test_model();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(9, 0);
    assert_ne!(m.world.stance(B, A), Stance::War);
    assert_eq!(m.apply(CampaignCommand::EnterSettlement { force: ForceId(1001), region: RegionId(10) }), Err(CommandError::NotAtWar(B, A)));
    assert!(m.army_may_enter(ForceId(1001), RegionId(10)));
    let ev = m.apply(CampaignCommand::JoinGarrison { character: CharacterId(101), region: RegionId(10) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEntersGarrison { character: CharacterId(101), region: RegionId(10) }), "{ev:?}");
    let r = &m.world.regions[&RegionId(10)];
    assert_eq!((r.owner, r.garrison), (A, Some(ForceId(1001))));
    let ch = &m.world.characters[&CharacterId(101)];
    assert_eq!((ch.garrisoned_in, ch.position), (Some(RegionId(10)), pos(10, 0)));
    // A forceless character takes the agent's entry.
    m.world.characters.get_mut(&CharacterId(102)).unwrap().position = pos(12, 0);
    let ev = m.apply(CampaignCommand::JoinGarrison { character: CharacterId(102), region: RegionId(12) }).unwrap();
    assert!(ev.contains(&CampaignEvent::CharacterEntersGarrison { character: CharacterId(102), region: RegionId(12) }), "{ev:?}");
}

/// With a garrison inside, `0x00B16700` lets in only the owner's army: another faction's order is
/// refused before it walks.
#[test]
fn a_held_settlement_refuses_another_factions_army() {
    let (mut m, _) = own_garrison_model(120, 2);
    let start = pos(20, 0);
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = start;
    assert!(!m.army_may_enter(ForceId(1001), RegionId(10)));
    assert!(m.army_may_enter(ForceId(1000), RegionId(10)));
    assert_eq!(
        m.apply(CampaignCommand::JoinGarrison { character: CharacterId(101), region: RegionId(10) }),
        Err(CommandError::NoResidenceRoom(RegionId(10)))
    );
    assert_eq!(m.world.characters[&CharacterId(101)].position, start);
}

/// An army aboard a navy is not ordered into a settlement, to a merge or to an attack from aboard
/// (PROVISIONAL, `CampaignModel::refuse_aboard`: the exe lands it inside the order): every such
/// order gets the one refusal, the script's as the map's.
#[test]
fn an_army_aboard_a_navy_gets_one_refusal_from_every_order() {
    let mut m = test_model();
    let navy = add_force(&mut m, 130, B, pos(9, 0), 1, true);
    let other = add_force(&mut m, 131, B, pos(9, 1), 1, false);
    m.world.embarked.insert(ForceId(1001), navy);
    let start = m.world.characters[&CharacterId(101)].position;
    let aboard = Err(CommandError::Unsupported("the army is aboard a navy: land it first"));
    assert_eq!(m.apply(CampaignCommand::JoinGarrison { character: CharacterId(101), region: RegionId(10) }), aboard);
    assert_eq!(m.apply(CampaignCommand::EnterSettlement { force: ForceId(1001), region: RegionId(10) }), aboard);
    assert_eq!(m.apply(CampaignCommand::MergeForces { force: ForceId(1001), into: other }), aboard);
    assert_eq!(m.apply(CampaignCommand::AttackForce { force: ForceId(1001), target: ForceId(1000) }), aboard);
    assert_eq!((m.world.regions[&RegionId(10)].garrison, m.world.characters[&CharacterId(101)].position), (None, start));
}

/// The move-to-residence order `0x0091BE30` refuses every character while the settlement is under
/// siege (the residence's virtual +0x98). For an army the residence's force fit is tested first
/// (`0x0091BE4F`, then the siege at `0x0091BE63`), so one the fit turns away gets that refusal.
#[test]
fn nobody_enters_a_besieged_settlement() {
    let mut m = test_model();
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(9, 0);
    m.world.characters.get_mut(&CharacterId(102)).unwrap().position = pos(9, 1);
    m.world.regions.get_mut(&RegionId(10)).unwrap().siege.besiegers = vec![ForceId(1001)];
    let besieged = Err(CommandError::UnderSiege(RegionId(10)));
    assert_eq!(m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(10) }), besieged);
    assert_eq!(m.apply(CampaignCommand::AgentEnterSettlement { agent: CharacterId(102), region: RegionId(10) }), besieged);
    assert_eq!(m.apply(CampaignCommand::JoinGarrison { character: CharacterId(102), region: RegionId(10) }), besieged);
    assert_eq!(m.world.regions[&RegionId(10)].garrison, None);
    // Besieged and held by A's garrison: B's army fails the fit before the siege is looked at.
    let (mut m, _) = own_garrison_model(120, 2);
    m.world.regions.get_mut(&RegionId(10)).unwrap().siege.besiegers = vec![ForceId(1001)];
    assert_eq!(
        m.apply(CampaignCommand::JoinGarrison { character: CharacterId(101), region: RegionId(10) }),
        Err(CommandError::NoResidenceRoom(RegionId(10)))
    );
}

/// The exchange arrival `0x0094D660` with the fit check `0x008B2720(army, garrison, 1)`: a human's
/// two armies must fit whole, else the exchange waits for him (the pending exchange `0x00885F30`)
/// and nothing moves; an AI's garrison needs room, and what fits moves; a full one takes nothing
/// and the AI's pending exchange does nothing (`0x008EA7E0`).
#[test]
fn the_garrison_exchange_fit_differs_for_humans_and_the_ai() {
    let units = |m: &CampaignModel, f: ForceId| m.world.forces.get(&f).map_or(0, |f| f.units.len());
    let enter = |m: &mut CampaignModel| m.apply(CampaignCommand::EnterSettlement { force: ForceId(1000), region: RegionId(10) }).unwrap();
    // A human whose army fits whole: it joins, the source army is gone.
    let (mut m, g) = own_garrison_model(120, 19);
    m.turn.humans = vec![A];
    let ev = enter(&mut m);
    assert!(ev.iter().any(|e| matches!(e, CampaignEvent::CampaignArmiesMerge { .. })), "{ev:?}");
    assert_eq!((units(&m, ForceId(1000)), units(&m, g)), (0, 20));
    // A human's two armies over the cap: the exchange waits, nothing moves.
    let (mut m, g) = own_garrison_model(120, 18);
    m.turn.humans = vec![A];
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.extend((0..2).map(|i| unit(80_000 + i)));
    let ev = enter(&mut m);
    assert!(ev.contains(&CampaignEvent::ExchangePending { force: ForceId(1000), into: g }), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CampaignArmiesMerge { .. })), "{ev:?}");
    assert_eq!((units(&m, ForceId(1000)), units(&m, g)), (3, 18));
    assert_eq!(m.world.characters[&CharacterId(100)].garrisoned_in, None);
    // The same for the AI: the garrison has room, so two units move and one stays outside.
    let (mut m, g) = own_garrison_model(120, 18);
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units.extend((0..2).map(|i| unit(80_000 + i)));
    let ev = enter(&mut m);
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::ExchangePending { .. })), "{ev:?}");
    assert_eq!((units(&m, ForceId(1000)), units(&m, g)), (1, 20));
    // A full garrison takes nothing from the AI, and nothing is queued.
    let (mut m, g) = own_garrison_model(120, 20);
    let ev = enter(&mut m);
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::ExchangePending { .. } | CampaignEvent::CampaignArmiesMerge { .. })), "{ev:?}");
    assert_eq!((units(&m, ForceId(1000)), units(&m, g)), (1, 20));
}

/// The merge order ends in the same exchange step as the garrison's (`0x0091CED0` pushes
/// `0x009081F0`; arrival `0x0094D660`), so `MergeForces` follows the one fit rule: a human's 18 + 3
/// exchange nothing and wait for him, a full receiver does not refuse his order, and what fits
/// whole merges.
#[test]
fn a_merge_order_uses_the_garrison_exchange_fit() {
    let units = |m: &CampaignModel, f: ForceId| m.world.forces.get(&f).map_or(0, |f| f.units.len());
    let setup = |receiver: i32, giver: i32| {
        let mut m = test_model();
        m.turn.humans = vec![A];
        let at = m.world.characters[&CharacterId(100)].position;
        let into = add_force(&mut m, 121, A, at, receiver, false);
        m.world.forces.get_mut(&ForceId(1000)).unwrap().units.extend((1..giver).map(|i| unit(80_000 + i)));
        (m, into)
    };
    for (receiver, giver) in [(18, 3), (20, 1)] {
        let (mut m, into) = setup(receiver, giver);
        let ev = m.apply(CampaignCommand::MergeForces { force: ForceId(1000), into }).unwrap();
        assert!(ev.contains(&CampaignEvent::ExchangePending { force: ForceId(1000), into }), "{ev:?}");
        assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::CampaignArmiesMerge { .. })), "{ev:?}");
        assert_eq!((units(&m, ForceId(1000)), units(&m, into)), (giver as usize, receiver as usize));
    }
    let (mut m, into) = setup(18, 2);
    let ev = m.apply(CampaignCommand::MergeForces { force: ForceId(1000), into }).unwrap();
    assert!(ev.contains(&CampaignEvent::CampaignArmiesMerge { force: ForceId(1000), into }), "{ev:?}");
    assert_eq!((units(&m, ForceId(1000)), units(&m, into)), (0, 20));
}

/// [`foreign_garrison_model`] on a polygon map of 2-unit cells along z 0 (origin (-1, -1), so the
/// settlement at x 10 is in the middle of cell 5), one square polygon per cell with `(kind, region
/// id)` from `cell(c)`; region id 1 holds A's region 10, id 2 B's region 11.
fn free_spot_model(cell: impl Fn(u32) -> (u8, u32)) -> CampaignModel {
    free_spot_model_with(cell, vec![vec![], vec![0], vec![1]], 0)
}

/// [`free_spot_model`] with the map's region sets (one per region id, by region index) and its
/// count of single-region slots given.
fn free_spot_model_with(cell: impl Fn(u32) -> (u8, u32), region_sets: Vec<Vec<u32>>, single_region_slots: usize) -> CampaignModel {
    use crate::campaign::pathing::PathGrid;
    use crate::campaign::polypath::{CellInput, PolyInput, PolyMap};
    let mut m = foreign_garrison_model();
    let u = Fixed20::from_int(1).raw();
    let cells: Vec<CellInput> = (0..9)
        .map(|c| {
            let x0 = (c as i32 * 2 - 1) * u;
            let (kind, region_id) = cell(c);
            CellInput { header: [0; 8], polys: vec![PolyInput { kind, region_id, outline: vec![(x0, -u), (x0 + 2 * u, -u), (x0 + 2 * u, u), (x0, u)] }] }
        })
        .collect();
    let mut grid = PathGrid::new((-1.0, -1.0), 2.0, 9, 1);
    grid.region_keys = vec!["test_region_10".into(), "test_region_11".into()];
    let mut map = PolyMap::build((-u, -u), 2 * u, 9, 1, &cells, region_sets);
    map.single_region_slots = single_region_slots;
    grid.poly = Some(map);
    m.attach_terrain(crate::campaign::world::Terrain(Arc::new(grid)));
    m
}

/// The region test of the free-spot search for the rebels (`0x00B16C00` → `0x00B622E0`, which
/// answers false at its entry for a faction without a record): every single region reads closed and
/// so does a group that holds one; only region id 0 and an empty group pass. The same on a map
/// without the original's region table (no single slots).
#[test]
fn the_region_test_closes_every_region_to_the_rebels() {
    use crate::campaign::polypath::kind::LAND;
    let rebels = FactionId(9);
    // Region ids: 0 none, 1 A's region 10, 2 B's region 11, 3 an empty group, 4 a group of both.
    let sets = || vec![vec![], vec![0], vec![1], vec![], vec![0, 1]];
    for single in [3, 0] {
        let mut m = free_spot_model_with(|_| (LAND, 1), sets(), single);
        m.world.factions.insert(rebels, faction(rebels, "", 0, GovernmentType::Republic));
        assert!(m.world.is_rebel_faction(rebels));
        assert_eq!(m.region_test_flags(rebels), Some(vec![true, false, false, true, false]), "single slots {single}");
        // The owner: its own region and the empty group, not B's region nor a group holding it.
        assert_eq!(m.region_test_flags(A), Some(vec![true, true, false, true, false]), "single slots {single}");
    }
    assert_eq!(test_model().region_test_flags(rebels), None, "no polygon map");
}

/// The transfer garrison when a mod's army cap is below the drawn size and a unit finds neither an
/// army near nor a free spot (`spawn_transfer_garrison`): the garrison keeps what fitted, the
/// units left over are not raised, and nothing else is created.
#[test]
fn a_transfer_garrison_stops_at_the_cap_with_no_free_spot() {
    use crate::campaign::polypath::kind::{RIVER, SHARED};
    let mut m = free_spot_model(|c| if c == 5 { (SHARED, 1) } else { (RIVER, 1) });
    // The settlement empty, B's army away.
    m.world.regions.get_mut(&RegionId(10)).unwrap().garrison = None;
    let b = m.world.characters.get_mut(&CharacterId(101)).unwrap();
    (b.garrisoned_in, b.position) = (None, pos(30, 5));
    m.force_caps.army = 2;
    let (forces, characters) = (m.world.forces.len(), m.world.characters.len());
    let mut drawn = m.director.rng;
    assert!(drawn.int_range(4, 10) > 2, "more units drawn than the cap");
    m.spawn_transfer_garrison(RegionId(10), A);
    assert_eq!(m.director.rng, drawn, "one draw");
    let g = m.world.regions[&RegionId(10)].garrison.expect("a garrison");
    assert_eq!((m.world.forces[&g].faction, m.world.forces[&g].units.len()), (A, 2));
    assert_eq!((m.world.forces.len(), m.world.characters.len()), (forces + 1, characters + 1), "the garrison and its colonel only");
}

/// With a foreign garrison inside and no owner army near, the new army stands at the free spot
/// (`0x00B39FE0`, called at `0x00B1EB78`): the nearest position (`0x00B1CA50`: outline point moved
/// inside by the row search) of a land or road polygon of a region open to the owner, reached from
/// the building footprint (kind 7) the settlement stands on.
#[test]
fn a_recruit_outside_stands_at_the_nearest_free_spot() {
    use crate::campaign::polypath::kind::{LAND, RIVER, ROAD, SHARED};
    let army_at = |m: &mut CampaignModel| {
        let Some(CampaignEvent::UnitTrained { force, .. }) = m.spawn_recruited_unit(RegionId(10), "test_unit".into()) else { panic!() };
        let f = &m.world.forces[&force];
        m.world.characters[&f.commander.unwrap()].position
    };
    let x = |x: f64| (Fixed20::from_f64(x), Fixed20::from_int(0));
    // The settlement on its footprint (no candidate), a river west, A's land east: its west edge.
    assert_eq!(army_at(&mut free_spot_model(|c| (match c { 0..=4 => RIVER, 5 => SHARED, _ => LAND }, 1))), x(11.0));
    // Land west, a river east: the land's east edge, which the land does not hold, so one raw unit
    // inside it.
    let east = army_at(&mut free_spot_model(|c| (match c { 0..=4 => LAND, 5 => SHARED, _ => RIVER }, 1)));
    assert_eq!(east, (Fixed20::from_raw(Fixed20::from_f64(9.0).raw() - 1), Fixed20::from_int(0)));
    // A settlement that stands on no footprint (here A's own road) starts no search (the seed
    // `0x00AF1590` takes kind-7 polygons only): nothing is raised.
    let mut on_road = free_spot_model(|c| (match c { 0..=4 => RIVER, 5 => ROAD, _ => LAND }, 1));
    assert_eq!(on_road.spawn_recruited_unit(RegionId(10), "test_unit".into()), None);
}

/// No free spot (`0x00B39FE0` fails): the item is not spawned and nothing is created (`0x00B1EAE6`
/// leaves the loop); it stays queued, and so do the land items after it, while the countdown keeps
/// taking a recruitment point for it every turn (`0x00B71FB0`).
#[test]
fn with_no_free_spot_the_recruit_stays_queued() {
    use crate::campaign::polypath::kind::{RIVER, SHARED};
    let mut m = free_spot_model(|c| if c == 5 { (SHARED, 1) } else { (RIVER, 1) });
    let (forces, characters) = (m.world.forces.len(), m.world.characters.len());
    assert_eq!(m.spawn_recruited_unit(RegionId(10), "test_unit".into()), None);
    assert_eq!((m.world.forces.len(), m.world.characters.len()), (forces, characters));
    for (id, turns) in [(9001, 1), (9002, 1)] {
        let item = RecruitmentItem { id: RecruitmentItemId(id), unit_key: "test_recruit".into(), turns_remaining: turns, cost: 400, target: None, target_lost: false };
        m.world.regions.get_mut(&RegionId(10)).unwrap().recruitment_queue.push(item);
    }
    m.turn.humans = vec![A];
    let queue = |m: &CampaignModel| m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| (i.id.0, i.turns_remaining)).collect::<Vec<_>>();
    m.start_campaign();
    assert_eq!(queue(&m), vec![(9001, 0), (9002, 0)]);
    assert_eq!((m.world.forces.len(), m.world.characters.len()), (forces, characters));
    m.end_turn();
    assert_eq!(queue(&m), vec![(9001, 0), (9002, 0)], "still waiting a turn later");
}

/// With no place for a recruit near the settlement the whole land queue is held (`0x00B71FB0` sets
/// byte +0x58 from `0x00B6A8D0`, the no-place flag of `0x00B43A60`, and skips the spawn): an item
/// recruited through a commander who leads an army raises no army either, though its own placement
/// would not need the free spot. Once a place exists it spawns.
#[test]
fn with_no_place_for_a_recruit_the_commander_s_recruits_wait_too() {
    use crate::campaign::polypath::kind::{RIVER, SHARED};
    let mut m = free_spot_model(|c| if c == 5 { (SHARED, 1) } else { (RIVER, 1) });
    assert_eq!(m.land_recruit_place(RegionId(10)), None);
    let (forces, characters) = (m.world.forces.len(), m.world.characters.len());
    let item = RecruitmentItem { id: RecruitmentItemId(9001), unit_key: "test_recruit".into(), turns_remaining: 1, cost: 400, target: Some(CharacterId(100)), target_lost: false };
    m.world.regions.get_mut(&RegionId(10)).unwrap().recruitment_queue.push(item);
    m.turn.humans = vec![A];
    let queue = |m: &CampaignModel| m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| (i.id.0, i.turns_remaining)).collect::<Vec<_>>();
    m.start_campaign();
    assert_eq!(queue(&m), vec![(9001, 0)], "held at zero turns");
    assert_eq!((m.world.forces.len(), m.world.characters.len()), (forces, characters), "no army raised");
    // A's army 1000 comes within 5.0 of the settlement: the recruit has a place, the item spawns.
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(12, 0);
    assert_eq!(m.land_recruit_place(RegionId(10)), Some(LandRecruitPlace::Join(ForceId(1000))));
    m.end_turn();
    assert!(queue(&m).is_empty(), "spawned once a place exists");
}

/// The `log` lines this test thread writes while `f` runs (a capturing logger, installed once for
/// the test binary; other threads' lines are dropped).
fn logged_lines<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
    use std::cell::RefCell;
    thread_local! { static LINES: RefCell<Option<Vec<String>>> = const { RefCell::new(None) }; }
    struct Capture;
    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            LINES.with(|l| {
                if let Some(lines) = l.borrow_mut().as_mut() {
                    lines.push(record.args().to_string());
                }
            });
        }
        fn flush(&self) {}
    }
    static CAPTURE: Capture = Capture;
    static INSTALLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let installed = *INSTALLED.get_or_init(|| {
        let ok = log::set_logger(&CAPTURE).is_ok();
        if ok {
            log::set_max_level(log::LevelFilter::Info);
        }
        ok
    });
    assert!(installed, "another logger is installed in this test binary");
    LINES.with(|l| *l.borrow_mut() = Some(Vec::new()));
    let r = f();
    (r, LINES.with(|l| l.borrow_mut().take().unwrap_or_default()))
}

/// The no-place hold is reported in the turn a land item counts down into it, not on the turns it
/// stays: here with one more item at 1 turn than the region's recruitment points, so the last one
/// never counts down (the held items at zero take every point, `0x00B71FB0`) and stays at 1.
#[test]
fn the_no_place_hold_is_logged_once() {
    use crate::campaign::polypath::kind::{RIVER, SHARED};
    let mut m = free_spot_model(|c| if c == 5 { (SHARED, 1) } else { (RIVER, 1) });
    m.turn.humans = vec![A];
    let points = m.recruitment_points(RegionId(10), false);
    assert!(points >= 1, "the test needs a recruitment point");
    for id in 0..=points {
        let item = RecruitmentItem { id: RecruitmentItemId(9001 + id as i32), unit_key: "test_recruit".into(), turns_remaining: 1, cost: 400, target: None, target_lost: false };
        m.world.regions.get_mut(&RegionId(10)).unwrap().recruitment_queue.push(item);
    }
    let held = |l: &String| l.starts_with("no place for a recruit near RegionId(10)");
    let ((), lines) = logged_lines(|| {
        m.start_campaign();
        m.end_turn();
        m.end_turn();
    });
    let turns: Vec<u32> = m.world.regions[&RegionId(10)].recruitment_queue.iter().map(|i| i.turns_remaining).collect();
    assert_eq!(turns.last(), Some(&1), "the last item never counts down");
    assert_eq!(turns.iter().filter(|&&t| t == 0).count() as u32, points, "the others wait at zero");
    assert_eq!(lines.iter().filter(|l| held(l)).count(), 1, "logged once: {lines:?}");
}

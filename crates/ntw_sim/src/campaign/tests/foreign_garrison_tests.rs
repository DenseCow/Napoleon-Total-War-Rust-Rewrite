//! Campaign model tests: another faction's garrison in a settlement (one an owner change does not
//! expel: the rebels, a faction out of the game or one without a capital). The owner's army
//! arriving, the settlement's defence and the owner's recruits (analysis/fidelity/CAMPAIGN_FIDELITY.md,
//! "Foreign garrisons").

use super::*;
use crate::campaign::commands::{straight_distance, CONTACT_DISTANCE};

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
    let CampaignEvent::UnitTrained { force, .. } = m.spawn_recruited_unit(RegionId(10), "test_unit".into()) else { panic!() };
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
            CampaignEvent::UnitTrained { force, .. } => force,
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

//! Campaign ambushes ([`crate::campaign::ambush`], CAMPAIGN_FIDELITY.md "Ambush"): a hidden enemy springs
//! on a force that walks into its zone.

use super::*;
use crate::campaign::pathing::PathGrid;
use crate::campaign::seams::AmbushSpring;
use crate::campaign::zoc::tests::open;
use crate::campaign::{BattleType, Terrain};
use std::sync::Arc;

const A: FactionId = FactionId(1);
const B: FactionId = FactionId(2);
/// A's general (force 1000) and B's colonel (force 1001).
const GA: CharacterId = CharacterId(100);
const GB: CharacterId = CharacterId(101);

/// A's general at (41, 21) and B's colonel hidden 8 units east at (49, 21), on an open 80 x 40 map, at
/// war when `war`; A's turn, and A's general has the action points to walk 20 units.
fn model(war: bool) -> CampaignModel {
    let mut m = test_model();
    m.turn.current = Some(A);
    m.world.characters.get_mut(&GA).unwrap().position = pos(41, 21);
    m.world.characters.get_mut(&GB).unwrap().position = pos(49, 21);
    let ga = m.world.characters.get_mut(&GA).unwrap();
    (ga.movement_points, ga.max_movement_points) = (100, 100);
    if war {
        m.world.set_stance(A, B, Stance::War).unwrap();
    }
    let mut grid = PathGrid::new((0.0, 0.0), 2.0, 40, 20);
    grid.poly = Some(open(40, 20));
    m.attach_terrain(Terrain(Arc::new(grid)));
    m.world.character_details.entry(GB).or_default().hidden = true;
    m
}

fn march(m: &mut CampaignModel) -> Vec<CampaignEvent> {
    m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(61, 21) }).unwrap()
}

fn at(m: &CampaignModel, c: CharacterId) -> (f32, f32) {
    let p = m.world.characters[&c].position;
    (p.0.to_f32(), p.1.to_f32())
}

/// `0x00B6A270` → `0x00A15610` → `0x008E18D0`: the hidden colonel springs on the general who walks into
/// his zone; the walk stops there, the colonel attacks in a land ambush and is no longer hidden.
#[test]
fn a_hidden_enemy_springs_on_a_walk_into_its_zone() {
    let mut m = model(true);
    let events = march(&mut m);
    let battle = m.pending_battle.clone().expect("an ambush");
    assert_eq!((battle.attacker, battle.defenders.as_slice(), battle.ambush), (ForceId(1001), &[ForceId(1000)][..], true));
    assert_eq!(m.battle_type(&battle), BattleType::LandAmbush);
    assert!(events.contains(&CampaignEvent::PreBattle { attacker: ForceId(1001) }));
    assert!(!m.world.character_details[&GB].hidden, "the setup clears the ambusher's flag");
    let stop = at(&m, GA);
    assert!(m.in_stored_zone(GB, stop) && stop.0 < 49.0, "stopped inside the zone, short of the colonel: {stop:?}");
    // Its zone's first step: one step back is outside it.
    assert!(!m.in_stored_zone(GB, (stop.0 - 0.2, stop.1)), "{stop:?}");
}

/// The state hash tells an ambush and a resumed order apart: both are saved and change the battle.
#[test]
fn the_state_hash_covers_the_ambush_and_the_resumed_order() {
    let mut m = model(true);
    march(&mut m);
    let base = m.state_hash();
    let mut plain = m.clone();
    plain.pending_battle.as_mut().unwrap().ambush = false;
    let mut resumes = m.clone();
    resumes.pending_battle.as_mut().unwrap().resume = Some(RegionId(1));
    assert_ne!(base, plain.state_hash());
    assert_ne!(base, resumes.state_hash());
}

/// An ordinary battle is no ambush, whatever the forces.
#[test]
fn an_attack_is_no_ambush() {
    let mut m = model(true);
    m.world.character_details.get_mut(&GB).unwrap().hidden = false;
    m.apply(CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }).unwrap();
    let battle = m.pending_battle.clone().expect("a battle");
    assert!(!battle.ambush);
    assert_eq!(m.battle_type(&battle), BattleType::LandNormal);
}

/// No ambush from an enemy the mover's faction knows (`0x008CE880`), one it is not at war with, or one
/// who is not hidden: the walk goes on (into the zone of a visible enemy the planner stops it at).
#[test]
fn no_ambush_from_a_known_friendly_or_visible_force() {
    let exposed = |m: &mut CampaignModel| m.world.faction_details.entry(A).or_default().exposed.push(GB);
    let peace = |m: &mut CampaignModel| m.world.set_stance(A, B, Stance::Neutral).unwrap();
    let visible = |m: &mut CampaignModel| m.world.character_details.get_mut(&GB).unwrap().hidden = false;
    for (name, edit) in [("exposed", &exposed as &dyn Fn(&mut CampaignModel)), ("not at war", &peace), ("not hidden", &visible)] {
        let mut m = model(true);
        edit(&mut m);
        m.apply(CampaignCommand::MoveForce { force: ForceId(1000), to: pos(45, 21) }).unwrap();
        assert_eq!(m.pending_battle, None, "{name}");
        assert_eq!(at(&m, GA), (45.0, 21.0), "{name}");
    }
}

/// A navy is not ambushed by an army, nor an army by a navy (`0x009D3A40` / `0x009D3A60`).
#[test]
fn an_army_is_not_ambushed_by_a_navy() {
    let mut m = model(true);
    m.world.forces.get_mut(&ForceId(1001)).unwrap().is_navy = true;
    march(&mut m);
    assert_eq!(m.pending_battle, None);
}

/// A hidden army inside a besieged settlement holds still (`0x00A15610`, its residence's +0x9C).
#[test]
fn a_besieged_army_does_not_spring() {
    let mut m = model(true);
    let r = *m.world.regions.keys().next().expect("a region");
    m.world.characters.get_mut(&GB).unwrap().garrisoned_in = Some(r);
    // A made-up besieger: the walker's own move would lift a siege he held.
    m.world.regions.get_mut(&r).unwrap().siege.besiegers.push(ForceId(4242));
    assert!(m.world.regions[&r].siege.under_siege());
    march(&mut m);
    assert_eq!(m.pending_battle, None);
}

/// The decision is the `ambush.spring` seam: a mod rule that never springs lets the walk go on.
#[test]
fn the_spring_decision_is_a_seam() {
    let mut m = model(true);
    let maker: crate::seam::Maker<AmbushSpring> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: CharacterId| false));
    Arc::make_mut(&mut m.rules).seams.ambush_spring.apply("test.never", &maker);
    assert_eq!(m.rules.seams.ambush_spring.chain(), ["original".to_string(), "test.never".to_string()]);
    march(&mut m);
    assert_eq!(m.pending_battle, None);
    assert!(at(&m, GA).0 > 49.0, "walked on past the colonel");
    assert!(m.world.character_details[&GB].hidden);
}

/// Each hidden character is asked once per walk (the move tracker's +124 list, `0x00959EA0`): anyone
/// already asked is skipped at the walk's later steps inside the same zone.
#[test]
fn each_hidden_enemy_is_asked_once_per_walk() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut m = model(true);
    let asked = Arc::new(AtomicUsize::new(0));
    let count = asked.clone();
    let maker: crate::seam::Maker<AmbushSpring> = Arc::new(move |_| {
        let count = count.clone();
        Arc::new(move |_: &CampaignModel, _: CharacterId, _: CharacterId| {
            count.fetch_add(1, Ordering::Relaxed);
            false
        })
    });
    Arc::make_mut(&mut m.rules).seams.ambush_spring.apply("test.count", &maker);
    march(&mut m);
    assert_eq!(asked.load(Ordering::Relaxed), 1);
}

/// Two standing orders resumed in one turn start (`0x009653D0`), the first one's march sprung on: that
/// army stops in the zone and keeps its order (`keep_joining`: not merged), and the second, still valid,
/// keeps its order without walking while the ambush battle is pending (`0x00973C20`). The control, with an
/// ambush rule that never springs, shows the second army's march is otherwise walked.
#[test]
fn an_ambush_sprung_by_the_first_standing_order_holds_the_second() {
    let setup = |springs: bool| {
        let mut m = model(true);
        if !springs {
            let maker: crate::seam::Maker<AmbushSpring> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: CharacterId| false));
            Arc::make_mut(&mut m.rules).seams.ambush_spring.apply("test.never", &maker);
        }
        // A's general (force 1000) waits east of the colonel; two A colonels march to join him.
        m.world.characters.get_mut(&GA).unwrap().position = pos(61, 21);
        for (id, force, p) in [(200, 2000, pos(41, 21)), (201, 2001, pos(61, 31))] {
            let mut c = character(id, A, CharacterKind::Colonel, 100);
            c.position = p;
            m.world.characters.insert(c.id, c);
            let force = MilitaryForce { id: ForceId(force), faction: A, commander: Some(CharacterId(id)), units: vec![unit(id)], is_navy: false };
            m.world.forces.insert(force.id, force);
            m.world.standing_orders.insert(CharacterId(id), StandingOrder::JoinForce { into: ForceId(1000) });
        }
        m
    };
    let mut control = setup(false);
    let second = control.world.characters[&CharacterId(201)].position;
    control.resume_standing_orders(A);
    assert_eq!(control.pending_battle, None, "control: no ambush");
    // Walked (or, arrived, merged into the general's army).
    assert_ne!(control.world.characters.get(&CharacterId(201)).map(|c| c.position), Some(second), "control: the second army walks");

    let mut m = setup(true);
    m.resume_standing_orders(A);
    let battle = m.pending_battle.clone().expect("the first march is sprung on");
    assert_eq!((battle.attacker, battle.defenders.as_slice(), battle.ambush), (ForceId(1001), &[ForceId(2000)][..], true));
    let first = at(&m, CharacterId(200));
    assert!(first.0 < 49.0, "stopped in the zone: {first:?}");
    assert_eq!(m.world.characters[&CharacterId(201)].position, second, "the second army does not walk");
    for c in [200, 201] {
        assert_eq!(m.world.standing_orders.get(&CharacterId(c)), Some(&StandingOrder::JoinForce { into: ForceId(1000) }), "{c}");
    }
}

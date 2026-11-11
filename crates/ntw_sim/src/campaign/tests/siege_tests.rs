//! Campaign model tests: sieges on the campaign map (the arrival's siege path `0x0094B600`, the assault and
//! the siege commands).

use super::*;
use super::commands::{RecruitableUnit, ENTRY_BESIEGED};

const R11: RegionId = RegionId(11);
/// A's army under general 100 ([`capture_model`]).
const ARMY: ForceId = ForceId(1000);
/// B's army under colonel 101.
const GARRISON: ForceId = ForceId(1001);

fn walls(m: &mut CampaignModel) {
    m.world.regions.get_mut(&R11).unwrap().fortification = Some(BuildingRef { level_key: "test_building_level".into(), health: 100 });
}

/// B's army 1001 inside its region 11 as the garrison.
fn garrisoned(m: &mut CampaignModel) {
    let c = m.world.characters.get_mut(&CharacterId(101)).unwrap();
    c.position = pos(11, 0);
    c.garrisoned_in = Some(R11);
    m.world.regions.get_mut(&R11).unwrap().garrison = Some(GARRISON);
}

/// A's army's unit becomes `test_siege_unit`, a copy of `test_unit` changed by `f`.
fn with_unit(m: &mut CampaignModel, f: impl FnOnce(&mut UnitRules)) {
    let mut rules = (*m.rules).clone();
    let mut u = rules.units["test_unit"].clone();
    f(&mut u);
    rules.units.insert("test_siege_unit".into(), u);
    m.rules = Arc::new(rules);
    m.world.forces.get_mut(&ARMY).unwrap().units[0].unit_key = "test_siege_unit".into();
}

fn siege(m: &CampaignModel) -> &SiegeState {
    &m.world.regions[&R11].siege
}

fn order(m: &mut CampaignModel, cmd: CampaignCommand) -> Vec<CampaignEvent> {
    m.apply(cmd).unwrap()
}

fn enter(force: ForceId) -> CampaignCommand {
    CampaignCommand::EnterSettlement { force, region: R11 }
}

fn assaulted() -> Option<PendingBattle> {
    Some(PendingBattle { attacker: ARMY, defenders: vec![GARRISON], settlement: Some(R11), resume: None, ambush: false })
}

/// Behind walls, an army without a siege unit lays siege and waits (`0x008B26C0` false); its first besieger set
/// the surrender turns from the population (800,000: 4) and the turns besieged to 0 (`0x00B112F0`).
#[test]
fn an_army_without_siege_units_only_besieges_a_walled_settlement() {
    let mut m = capture_model(vec![A]);
    walls(&mut m);
    garrisoned(&mut m);
    m.world.regions.get_mut(&R11).unwrap().siege.turns_besieged = 3;
    let ev = order(&mut m, enter(ARMY));
    assert!(!ev.contains(&CampaignEvent::PreBattle { attacker: ARMY }), "{ev:?}");
    assert_eq!(m.pending_battle, None);
    assert_eq!(*siege(&m), SiegeState { besiegers: vec![ARMY], surrender_turns: 4, turns_besieged: 0, ..Default::default() });
    assert_eq!(m.world.regions[&R11].owner, B);
}

/// A siege unit (the `unit_stats_land` #77 flag, record +0x1A0) or an artillery unit takes the army over the
/// walls (`ForceHasSiegeUnits` `0x008CE1B0`): it lays siege and assaults (`0x008E1A90`).
#[test]
fn an_army_with_siege_units_assaults_the_walls() {
    let flagged = |u: &mut UnitRules| u.siege_capable = true;
    let guns = |u: &mut UnitRules| u.category = "artillery".into();
    for (name, f) in [("flagged", &flagged as &dyn Fn(&mut UnitRules)), ("artillery", &guns)] {
        let mut m = capture_model(vec![A]);
        walls(&mut m);
        garrisoned(&mut m);
        with_unit(&mut m, f);
        let ev = order(&mut m, enter(ARMY));
        assert!(ev.contains(&CampaignEvent::PreBattle { attacker: ARMY }), "{name}: {ev:?}");
        assert_eq!(m.pending_battle, assaulted(), "{name}");
        assert_eq!(siege(&m).besiegers, [ARMY], "{name}: the assault comes after the siege is laid");
    }
}

/// With no walls (fortification level below 0) any army assaults at once.
#[test]
fn an_unwalled_settlement_is_assaulted_at_once() {
    let mut m = capture_model(vec![A]);
    garrisoned(&mut m);
    let ev = order(&mut m, enter(ARMY));
    assert!(ev.contains(&CampaignEvent::PreBattle { attacker: ARMY }), "{ev:?}");
    assert_eq!(m.pending_battle, assaulted());
}

/// The besiege order sets the attack command's hold byte (step +0x5C): the army lays siege and does not assault
/// even when it could; the assault order then assaults (`0x009365D0`).
#[test]
fn the_besiege_order_holds_and_the_assault_order_assaults() {
    let mut m = capture_model(vec![A]);
    garrisoned(&mut m);
    let ev = order(&mut m, CampaignCommand::BesiegeSettlement { force: ARMY, region: R11 });
    assert!(!ev.contains(&CampaignEvent::PreBattle { attacker: ARMY }), "{ev:?}");
    assert_eq!(siege(&m).besiegers, [ARMY]);
    // Ordered again, the siege goes on: the turns are not set again.
    m.world.regions.get_mut(&R11).unwrap().siege.turns_besieged = 2;
    order(&mut m, CampaignCommand::BesiegeSettlement { force: ARMY, region: R11 });
    assert_eq!((siege(&m).besiegers.as_slice(), siege(&m).turns_besieged), (&[ARMY][..], 2));
    let ev = order(&mut m, CampaignCommand::AssaultSettlement { force: ARMY });
    assert_eq!(ev, [CampaignEvent::PreBattle { attacker: ARMY }]);
    assert_eq!(m.pending_battle, assaulted());
}

/// The assault order needs a siege and an army that can assault now (`0x008B26C0`).
#[test]
fn the_assault_order_needs_a_siege_and_a_way_over_the_walls() {
    let mut m = capture_model(vec![A]);
    garrisoned(&mut m);
    walls(&mut m);
    assert!(m.apply(CampaignCommand::AssaultSettlement { force: ARMY }).is_err(), "no siege");
    order(&mut m, enter(ARMY));
    assert_eq!(siege(&m).besiegers, [ARMY]);
    assert!(m.apply(CampaignCommand::AssaultSettlement { force: ARMY }).is_err(), "no siege unit");
    assert_eq!(m.pending_battle, None);
    with_unit(&mut m, |u| u.siege_capable = true);
    order(&mut m, CampaignCommand::AssaultSettlement { force: ARMY });
    assert_eq!(m.pending_battle, assaulted());
}

/// A settlement another army besieges first is left to that siege (`0x0094B600`): the second army neither joins
/// nor assaults, though it could.
#[test]
fn a_second_army_leaves_the_settlement_to_its_first_besieger() {
    let mut m = capture_model(vec![A]);
    garrisoned(&mut m);
    m.world.characters.insert(CharacterId(107), character(107, A, CharacterKind::General, 30));
    let second = ForceId(1002);
    m.world.forces.insert(second, MilitaryForce { id: second, faction: A, commander: Some(CharacterId(107)), units: vec![unit(7)], is_navy: false });
    order(&mut m, CampaignCommand::BesiegeSettlement { force: ARMY, region: R11 });
    let ev = order(&mut m, enter(second));
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::PreBattle { .. })), "{ev:?}");
    assert_eq!(m.pending_battle, None);
    assert_eq!(siege(&m).besiegers, [ARMY]);
}

/// The end-siege order (`0x00932760`) ends the siege of the army the character commands; the turns stay.
#[test]
fn the_end_siege_order_ends_the_commanders_siege() {
    let mut m = capture_model(vec![A]);
    garrisoned(&mut m);
    order(&mut m, CampaignCommand::BesiegeSettlement { force: ARMY, region: R11 });
    m.world.regions.get_mut(&R11).unwrap().siege.turns_besieged = 1;
    assert!(m.apply(CampaignCommand::EndSiege { character: CharacterId(102) }).is_err(), "a minister commands no army");
    order(&mut m, CampaignCommand::EndSiege { character: CharacterId(100) });
    assert_eq!(*siege(&m), SiegeState { besiegers: Vec::new(), surrender_turns: 4, turns_besieged: 1, ..Default::default() });
    assert!(m.apply(CampaignCommand::EndSiege { character: CharacterId(100) }).is_err(), "no siege left");
}

/// A building's entries carry [`ENTRY_BESIEGED`] while its settlement is under siege (`IsUnderSiege`, virtual
/// +0x98, the region's siege state).
#[test]
fn a_besieged_settlements_entries_are_flagged() {
    let mut m = capture_model(vec![A]);
    let before = m.recruitable_units(R11);
    assert!(!before.is_empty() && before.iter().all(|e| e.flags & ENTRY_BESIEGED == 0), "{before:?}");
    m.world.regions.get_mut(&R11).unwrap().siege.besiegers.push(ARMY);
    let flagged: Vec<RecruitableUnit> = before.iter().map(|e| RecruitableUnit { unit_key: e.unit_key.clone(), flags: e.flags | ENTRY_BESIEGED }).collect();
    assert_eq!(m.recruitable_units(R11), flagged);
}

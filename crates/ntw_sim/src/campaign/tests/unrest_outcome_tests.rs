//! classslots2: what the population classes' unrest does at the owner's turn start (`0x008B46A0`), the riot
//! (`0x00AAD7D0`), the crackdown after a region's last risen army (`0x008DBEA0`) and the bankruptcy cleared
//! after a revolution (`0x008B69B0`).

use super::*;
use super::super::population::{
    riot_health, unrest_outcome, DEMANDS_DEFAULT_MESSAGE, RIOTS_DEFAULT_MESSAGE, RISING_CRACKDOWN, STRIKES_DEFAULT_MESSAGE,
};

const R10: RegionId = RegionId(10);

/// `test_model` with region 10 (population 400 000) holding one class `lower` at unrest `kind`, cause 1 and
/// riot chance `riot`, and both its slots attached to the settlement, each with a building at health 100.
fn unrest(kind: u32, riot: i32) -> CampaignModel {
    let mut m = test_model();
    let r = m.world.regions.get_mut(&R10).unwrap();
    r.class_bases = vec![PopulationClassState { unrest: kind, cause: 1, riot_chance: riot, ..PopulationClassState::new("lower") }];
    for (i, s) in r.slots.iter_mut().enumerate() {
        s.key = format!("settlement:test_region_10:town:test_slot:{i}");
        s.building = Some(BuildingRef { level_key: format!("test_building_{i}"), health: 100 });
    }
    m
}

fn to_a(message: CampaignMessage) -> CampaignEvent {
    CampaignEvent::Message { recipient: A, message }
}

fn health(m: &CampaignModel, slot: usize) -> u32 {
    m.world.regions[&R10].slots[slot].building.as_ref().unwrap().health
}

/// A riot (kind 2) draws its crowd (`int_range(0, 150)`), kills (draw + √(pop / 10) + 1000) × riot chance %
/// (here (draw + 200 + 1000) × 0.5), draws one settlement building and takes the riot chance off its health,
/// then posts `RegionRiots` and the riot notice with the deaths and the building.
#[test]
fn a_riot_kills_and_damages_one_settlement_building() {
    let mut m = unrest(2, 50);
    let mut rng = m.rng;
    let draw = rng.int_range(0, 150);
    let pick = rng.int_range(0, 1) as usize;
    let deaths = ((draw + 1200) as f32 * 0.5) as u32;
    let ev = unrest_outcome(&mut m, R10);
    assert_eq!(
        ev,
        vec![
            CampaignEvent::CampaignBuildingDamaged { region: R10, slot: SlotRef::Slot(pick) },
            CampaignEvent::RegionRiots { region: R10 },
            to_a(CampaignMessage::RegionRiot {
                message_type: RIOTS_DEFAULT_MESSAGE.into(),
                region: R10,
                deaths,
                building: Some(format!("test_building_{pick}")),
            }),
        ]
    );
    assert_eq!(m.world.regions[&R10].population, 400_000 - deaths);
    assert_eq!((health(&m, pick), health(&m, 1 - pick)), (50, 100));
    assert_eq!(m.rng, rng, "two draws from the campaign stream");
}

/// The deaths stop at `minimum_population`; a population already below it is raised to it (`0x00A8BDF0`).
#[test]
fn a_riot_leaves_the_minimum_population() {
    let mut m = unrest(2, 50);
    Arc::make_mut(&mut m.rules).variables.insert("minimum_population".into(), 1000.0);
    m.world.regions.get_mut(&R10).unwrap().population = 1500;
    let ev = unrest_outcome(&mut m, R10);
    assert_eq!(m.world.regions[&R10].population, 1000);
    assert!(matches!(&ev[2], CampaignEvent::Message { message: CampaignMessage::RegionRiot { deaths: 500, .. }, .. }), "{ev:?}");
    m.world.regions.get_mut(&R10).unwrap().population = 800;
    unrest_outcome(&mut m, R10);
    assert_eq!(m.world.regions[&R10].population, 1000);
}

/// The building's health less the riot chance, at least 0, clamped to 10..=90.
#[test]
fn a_riot_leaves_a_building_between_10_and_90() {
    assert_eq!(riot_health(100, 0), 90);
    assert_eq!(riot_health(100, 95), 10);
    assert_eq!(riot_health(50, 200), 10);
    assert_eq!(riot_health(60, 20), 40);
}

/// Only the slots attached to the settlement and holding a building are drawn from (the exe's second list,
/// `0x00AAA790`); one candidate still costs a draw, none costs none.
#[test]
fn a_riot_draws_only_settlement_buildings() {
    let mut m = unrest(2, 50);
    m.world.regions.get_mut(&R10).unwrap().slots[1].key = "test_slot_1".into();
    let mut rng = m.rng;
    rng.int_range(0, 150);
    rng.int_range(0, 0);
    unrest_outcome(&mut m, R10);
    assert_eq!((health(&m, 0), health(&m, 1)), (50, 100));
    assert_eq!(m.rng, rng);

    let mut m = unrest(2, 50);
    m.world.regions.get_mut(&R10).unwrap().slots[0].building = None;
    m.world.regions.get_mut(&R10).unwrap().slots[1].key = "test_slot_1".into();
    let mut rng = m.rng;
    rng.int_range(0, 150);
    let ev = unrest_outcome(&mut m, R10);
    assert!(matches!(&ev[..], [CampaignEvent::RegionRiots { .. }, CampaignEvent::Message { message: CampaignMessage::RegionRiot { building: None, .. }, .. }]), "{ev:?}");
    assert_eq!(health(&m, 1), 100);
    assert_eq!(m.rng, rng, "no building, no second draw");
}

/// A strike (kind 1) posts its notice, then `RegionStrikes`; a row whose type contains `riot` posts the riot
/// notice with no deaths and no building instead. Nobody dies and nothing is drawn.
#[test]
fn a_strike_notices_then_posts_the_event() {
    let strike = |row: Option<&str>| {
        let mut m = unrest(1, 50);
        if let Some(row) = row {
            Arc::make_mut(&mut m.rules).unrest_messages.insert((1, 1), row.into());
        }
        let rng = m.rng;
        let ev = unrest_outcome(&mut m, R10);
        assert_eq!((m.rng, m.world.regions[&R10].population), (rng, 400_000));
        ev
    };
    let strikes = CampaignEvent::RegionStrikes { region: R10 };
    let unrest_notice = |t: &str| to_a(CampaignMessage::RegionUnrest { message_type: t.into(), region: R10 });
    assert_eq!(strike(None), vec![unrest_notice(STRIKES_DEFAULT_MESSAGE), strikes.clone()]);
    assert_eq!(strike(Some("")), vec![unrest_notice(STRIKES_DEFAULT_MESSAGE), strikes.clone()], "an empty row is the default");
    assert_eq!(strike(Some("strikes_taxes")), vec![unrest_notice("strikes_taxes"), strikes.clone()]);
    let riot_row = CampaignMessage::RegionRiot { message_type: "riots_taxes".into(), region: R10, deaths: 0, building: None };
    assert_eq!(riot_row.type_name(), "riots_taxes");
    assert_eq!(strike(Some("riots_taxes")), vec![to_a(riot_row), strikes]);
}

/// An ultimatum or a letter of demand (kinds 3, 4) posts `RegionIssuesDemands`, then the notice by its own
/// row; a rebellion (5) posts `RegionRebels`; a content class (0) does nothing.
#[test]
fn demands_and_rebellions_post_their_events() {
    for kind in [3, 4] {
        let mut m = unrest(kind, 50);
        let demands = CampaignEvent::RegionIssuesDemands { region: R10 };
        let notice = |t: &str| to_a(CampaignMessage::RegionUnrest { message_type: t.into(), region: R10 });
        assert_eq!(unrest_outcome(&mut m, R10), vec![demands.clone(), notice(DEMANDS_DEFAULT_MESSAGE)]);
        Arc::make_mut(&mut m.rules).unrest_messages.insert((1, kind), format!("demands_{kind}"));
        assert_eq!(unrest_outcome(&mut m, R10), vec![demands, notice(&format!("demands_{kind}"))]);
    }
    assert_eq!(unrest_outcome(&mut unrest(5, 50), R10), vec![CampaignEvent::RegionRebels { region: R10 }]);
    assert_eq!(unrest_outcome(&mut unrest(0, 50), R10), vec![]);
}

/// The owner's turn start runs the outcome for each of its regions (`0x008F2C04`), before its
/// `FactionTurnStart`; another faction's turn start leaves the region alone.
#[test]
fn the_owners_turn_start_acts_on_the_unrest() {
    let mut m = unrest(3, 50);
    m.turn.humans = vec![A];
    let ev = m.start_campaign();
    let demands: Vec<usize> = ev.iter().enumerate().filter(|(_, e)| matches!(e, CampaignEvent::RegionIssuesDemands { .. })).map(|(i, _)| i).collect();
    let a_start = ev.iter().position(|e| *e == CampaignEvent::FactionTurnStart { faction: A }).unwrap();
    assert_eq!(demands.len(), 1, "{ev:?}");
    assert!(demands[0] < a_start);
    assert_eq!(ev[demands[0]], CampaignEvent::RegionIssuesDemands { region: R10 });
}

/// MADE-UP mod: a rule registered on `population.unrest_outcome` replaces the original's.
#[test]
fn a_mod_can_replace_the_unrest_outcome() {
    use super::super::mod_state::ModWrites;
    use super::super::seams::{CampaignRuleRegistry, PopulationUnrestOutcome, POPULATION_UNREST_OUTCOME};
    const OWNER: &str = "test_mod.quiet";
    let mut m = unrest(3, 50);
    m.turn.humans = vec![A];
    let mut reg = CampaignRuleRegistry::default();
    let maker = |_: Arc<PopulationUnrestOutcome>| Arc::new(|_: &mut CampaignModel, _: RegionId, _: &mut ModWrites| Vec::new()) as Arc<PopulationUnrestOutcome>;
    assert!(reg.population_unrest_outcome.register(OWNER, Arc::new(maker)));
    let mut warnings = Vec::new();
    Arc::make_mut(&mut m.rules).seams = reg.seams(&[(POPULATION_UNREST_OUTCOME.to_owned(), OWNER.to_owned())], &mut warnings);
    assert!(warnings.is_empty());
    let ev = m.start_campaign();
    assert!(!ev.iter().any(|e| matches!(e, CampaignEvent::RegionIssuesDemands { .. })));
}

/// The last risen army of a region going sets every class's crackdown to 6 (`0x0088CF08` → `0x008DBEA0`);
/// while another stays, nothing changes.
#[test]
fn the_last_risen_army_going_cracks_down() {
    let crackdowns = |armies: Vec<ForceId>| {
        let mut m = test_model();
        m.world.regions.get_mut(&R10).unwrap().class_bases = vec![PopulationClassState::new("lower"), PopulationClassState::new("upper")];
        m.world.region_risen_armies.insert(R10, armies);
        m.remove_force(ForceId(1000), None);
        m.world.regions[&R10].class_bases.iter().map(|c| c.crackdown).collect::<Vec<_>>()
    };
    assert_eq!(crackdowns(vec![ForceId(1000)]), vec![RISING_CRACKDOWN; 2]);
    assert_eq!(crackdowns(vec![ForceId(1000), ForceId(1001)]), vec![0; 2]);
    assert_eq!(crackdowns(vec![ForceId(1001)]), vec![0; 2], "another army goes");
}

/// A revolution's end clears the wealth growth offset and every class's bankruptcy factor of the faction's
/// regions only (`0x008B69B0`).
#[test]
fn a_revolutions_end_clears_the_factions_bankruptcy() {
    let mut m = test_model();
    for r in m.world.regions.values_mut() {
        r.wealth_growth_offset = -4;
        r.class_bases = vec![PopulationClassState { bankruptcy: -4, ..PopulationClassState::new("lower") }];
    }
    m.clear_bankruptcy_after_revolution(A);
    for (id, left) in [(10, 0), (11, -4), (12, 0)] {
        let r = &m.world.regions[&RegionId(id)];
        assert_eq!((r.wealth_growth_offset, r.class_bases[0].bankruptcy), (left, left), "region {id}");
    }
}

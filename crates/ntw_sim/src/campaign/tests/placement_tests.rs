//! Campaign model tests: the placement `0x0096C8D0` ([`CampaignModel::place_character`]) and its
//! callers in the model (analysis/campaign/PATHFINDING.md §8): the region expulsion, the General
//! hire, the admiral hire, the field promotion and the unit transfers. A placement ends the placed
//! character's raids, even in place, and sets him down: his sight is added to his faction's visible
//! set.

use super::commerce_raid_tests::{allow_promotion, at, candidate, raid_pool_model};
use super::*;
use crate::campaign::expulsion::ExpelledForce;
use crate::campaign::seams::RegionExpulsion;
use crate::campaign::visibility::{CellSet, Shroud, SightGrid};
use crate::seam::Maker;

/// Where the pool model's army 1002 and fleet 1003 stand.
const SPOT: (f32, f32) = (55.0, 50.0);

/// Gives A a shroud on a grid covering [`SPOT`], with nothing visible yet, and Generals and admirals
/// a sight radius by type (colonels and captains have none); radii stored so far are dropped.
fn watch(m: &mut CampaignModel) {
    let grid = SightGrid::centred(128, 128, 128);
    m.world.sight_grid = Some(grid);
    let empty = CellSet::new(128, 128);
    m.world.shrouds.insert(A, Shroud { explored: empty.clone(), visible: empty.clone(), hidden: empty, active: true });
    let rules = Arc::make_mut(&mut m.rules);
    rules.agent_sight.insert("General".into(), 3);
    rules.agent_sight.insert("admiral".into(), 3);
    m.world.sight_radius.clear();
}

/// Whether A's visible set holds [`SPOT`]: someone was set down there.
fn spot_seen(m: &CampaignModel) -> bool {
    let (x, z) = m.world.sight_grid.unwrap().cell_of(SPOT).unwrap();
    m.world.shrouds[&A].visible.get(x, z)
}

#[test]
fn an_expulsion_placing_a_raider_where_he_stands_ends_his_raids_and_sets_him_down() {
    let mut m = raid_pool_model();
    watch(&mut m);
    let colonel = CharacterId(107);
    m.world.sight_radius.insert(colonel, 3.0);
    assert!(m.raid_trade_route(colonel, at(SPOT.0, SPOT.1)));
    // A rule that puts army 1002 down where it stands.
    let stay: Maker<RegionExpulsion> =
        Arc::new(|_| Arc::new(|_: &CampaignModel, _: FactionId, _: RegionId, _: &mut _| vec![ExpelledForce { force: ForceId(1002), to: Some(at(SPOT.0, SPOT.1)), notice: false }]));
    Arc::make_mut(&mut m.rules).seams.region_expulsion.apply("test.stay", &stay);
    m.expel_from_region(A, RegionId(10), &mut Vec::new());
    // `0x0096C930` → `0x00A28605`: the raids end though the position did not change; `0x0096C9C4`
    // sets him down.
    assert_eq!(m.world.characters[&colonel].position, at(SPOT.0, SPOT.1));
    assert!(!m.is_raiding(colonel));
    assert!(spot_seen(&m));
}

#[test]
fn a_general_hired_into_an_army_is_placed_before_he_takes_its_raids() {
    let mut m = raid_pool_model();
    let c = candidate(&mut m, false);
    watch(&mut m);
    assert!(m.raid_trade_route(CharacterId(107), at(SPOT.0, SPOT.1)));
    m.apply(CampaignCommand::HireGeneral { character: c, into: Some(ForceId(1002)) }).unwrap();
    // `0x00A165B9`: placed where the colonel stood, so his sight is added; the hand-over after it
    // leaves him the colonel's raids.
    assert!(spot_seen(&m));
    assert!(m.is_raiding(c));
    assert!(!m.is_raiding(CharacterId(107)));
}

#[test]
fn an_admiral_hired_onto_a_replaced_ship_is_placed_and_one_on_an_empty_ship_is_not() {
    // He replaces the captain commanding the fleet's one ship: placed (`0x00A162E9`), then the raids.
    let mut m = raid_pool_model();
    let c = candidate(&mut m, true);
    watch(&mut m);
    assert!(m.raid_trade_route(CharacterId(108), at(SPOT.0, SPOT.1)));
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(c));
    assert!(spot_seen(&m));
    assert!(m.is_raiding(c));
    // He boards an empty ship and takes the command: no placement (`0x00A162F3`), but the pick's
    // `0x008F9400` floods the zones and adds the sight, leaving his hidden flag as it is; the raids
    // still pass to him.
    let mut m = raid_pool_model();
    let f = m.world.forces.get_mut(&ForceId(1003)).unwrap();
    let mut empty = unit(9);
    empty.character = None;
    f.units.insert(0, empty);
    let c = candidate(&mut m, true);
    watch(&mut m);
    m.world.character_details.entry(c).or_default().hidden = true;
    assert!(m.raid_trade_route(CharacterId(108), at(SPOT.0, SPOT.1)));
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(c));
    assert!(spot_seen(&m));
    assert!(m.world.character_details[&c].hidden);
    assert!(m.is_raiding(c));
}

/// The pool model with fleet 1003 led by admiral 120 (`command_sea` 9) on its first ship, where
/// [`SPOT`] is, and `second` aboard its second ship.
fn led_fleet(second: Option<CharacterId>) -> CampaignModel {
    let mut m = raid_pool_model();
    let mut lead = character(120, A, CharacterKind::Admiral, 25);
    lead.position = at(SPOT.0, SPOT.1);
    m.world.characters.insert(lead.id, lead);
    m.world.character_details.entry(CharacterId(120)).or_default().attributes = vec![("command_sea".into(), 9)];
    let f = m.world.forces.get_mut(&ForceId(1003)).unwrap();
    let mut ship = unit(9);
    ship.character = Some(CharacterId(120));
    f.units[0].character = second;
    f.units.insert(0, ship);
    f.commander = Some(CharacterId(120));
    m
}

#[test]
fn an_admiral_hired_under_an_abler_admiral_adds_sight_only_when_he_replaced_a_ship_character() {
    // He replaces the captain of the second ship; the admiral keeps command, so no placement
    // (`0x00A162A6`), but the pick after the replacement (`0x008B3B00` → `0x008B8E50`) runs
    // `0x008F9400`: the zones and the sight.
    let mut m = led_fleet(Some(CharacterId(108)));
    // The new admiral ranks below 120 (the test rules start every attribute at -1).
    let c = candidate(&mut m, true);
    m.world.character_details.get_mut(&c).unwrap().attributes = vec![("command_sea".into(), 1)];
    watch(&mut m);
    m.world.sight_radius.insert(CharacterId(120), 3.0);
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(CharacterId(120)));
    assert!(spot_seen(&m));
    // He boards the empty second ship and the command stays: the pick `0x008D2FA0` returns at
    // `0x008D2FEF`, nothing is flooded or seen.
    let mut m = led_fleet(None);
    let c = candidate(&mut m, true);
    m.world.character_details.get_mut(&c).unwrap().attributes = vec![("command_sea".into(), 1)];
    watch(&mut m);
    m.world.sight_radius.insert(CharacterId(120), 3.0);
    m.apply(CampaignCommand::HireAdmiral { character: c, fleet: ForceId(1003) }).unwrap();
    assert_eq!(m.world.forces[&ForceId(1003)].commander, Some(CharacterId(120)));
    assert!(!spot_seen(&m));
}

#[test]
fn a_field_promotion_places_a_new_commander_but_not_the_one_already_in_command() {
    // A second colonel (110) is promoted over the commander: placed (`0x008E2147`), then the raids.
    let mut m = raid_pool_model();
    m.world.characters.insert(CharacterId(110), character(110, A, CharacterKind::Colonel, 25));
    let mut u = unit(10);
    u.character = Some(CharacterId(110));
    m.world.forces.get_mut(&ForceId(1002)).unwrap().units.push(u);
    m.turn.humans = vec![A];
    m.start_campaign();
    allow_promotion(&mut m);
    watch(&mut m);
    assert!(m.raid_trade_route(CharacterId(107), at(SPOT.0, SPOT.1)));
    m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 1 }).unwrap();
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(110)));
    assert!(spot_seen(&m));
    assert!(m.is_raiding(CharacterId(110)));
    // The commanding colonel's own unit is promoted: the command stays, no placement
    // (`0x008E1E96`), and his raids stay his.
    let mut m = raid_pool_model();
    m.world.forces.get_mut(&ForceId(1002)).unwrap().units[0].character = Some(CharacterId(107));
    m.turn.humans = vec![A];
    m.start_campaign();
    allow_promotion(&mut m);
    watch(&mut m);
    assert!(m.raid_trade_route(CharacterId(107), at(SPOT.0, SPOT.1)));
    m.apply(CampaignCommand::PromoteUnit { force: ForceId(1002), unit: 0 }).unwrap();
    assert_eq!(m.world.characters[&CharacterId(107)].kind, CharacterKind::General);
    assert!(!spot_seen(&m));
    assert!(m.is_raiding(CharacterId(107)));
}

/// For the unit transfers `0x008D47B0` / `0x008D5F10` ([`CampaignModel::merge_units`]): army `force`
/// of A under `kind` `c` at `p`, riding the first of `units`.
fn army(m: &mut CampaignModel, force: u32, c: i32, kind: CharacterKind, p: (f32, f32), mut units: Vec<CampaignUnit>) {
    let mut ch = character(c, A, kind, 25);
    ch.position = at(p.0, p.1);
    m.world.characters.insert(ch.id, ch);
    units[0].character = Some(CharacterId(c));
    m.world.forces.insert(ForceId(force), MilitaryForce { id: ForceId(force), faction: A, commander: Some(CharacterId(c)), units, is_navy: false });
}

/// The pool model with its colonel 107 riding his unit 7 (an infantry unit) and raiding, A watching,
/// and colonels seeing as far as Generals when `colonels_see`.
fn transfer_model(colonels_see: bool) -> CampaignModel {
    let mut m = raid_pool_model();
    with_pick_units(&mut m);
    let u = &mut m.world.forces.get_mut(&ForceId(1002)).unwrap().units[0];
    u.character = Some(CharacterId(107));
    u.unit_key = "t_inf_rich".into();
    watch(&mut m);
    if colonels_see {
        Arc::make_mut(&mut m.rules).agent_sight.insert("colonel".into(), 3);
    }
    assert!(m.raid_trade_route(CharacterId(107), at(SPOT.0, SPOT.1)));
    m
}

/// Whether A's visible set holds `p`.
fn seen(m: &CampaignModel, p: (f32, f32)) -> bool {
    let (x, z) = m.world.sight_grid.unwrap().cell_of(p).unwrap();
    m.world.shrouds[&A].visible.get(x, z)
}

#[test]
fn a_general_merged_into_a_colonels_army_takes_command_is_placed_and_takes_the_raids() {
    let mut m = transfer_model(false);
    army(&mut m, 1004, 109, CharacterKind::General, SPOT, vec![keyed(9, "t_inf_rich")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    // `0x008D4F49`: the General comes first; `0x008D5356` places him; `0x008D5DDD` hands the raids on.
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(109)));
    assert!(spot_seen(&m));
    assert!(m.is_raiding(CharacterId(109)));
    assert!(!m.is_raiding(CharacterId(107)));
}

#[test]
fn a_merge_that_keeps_the_commander_places_no_one() {
    let mut m = transfer_model(true);
    army(&mut m, 1004, 109, CharacterKind::Colonel, SPOT, vec![keyed(9, "t_inf_rich")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    // Equal keys down to the unit's place: 107's unit stays first.
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(107)));
    // No placement (it would end his raids), but the transfer's `0x008F9400` adds the sight.
    assert!(m.is_raiding(CharacterId(107)));
    assert!(spot_seen(&m));
}

#[test]
fn a_colonel_the_pick_makes_in_a_plain_transfer_is_not_placed() {
    let mut m = transfer_model(true);
    // Every unit can hide, so a placement (its `0x009D3000`) would hide the made colonel.
    for r in Arc::make_mut(&mut m.rules).units.values_mut() {
        r.campaign_stealth = true;
    }
    army(&mut m, 1004, 109, CharacterKind::Colonel, SPOT, vec![keyed(9, "t_inf_rich"), keyed(12, "t_cav")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    // The characterless cavalry comes first (category 0): a colonel is made for it, and
    // `0x008D47B0` places none it made (`0x008D52B0`); 107's raids pass to him. Its `0x008F9400`
    // still adds the sight and leaves the hidden flag as it is.
    let new = m.world.forces[&ForceId(1002)].commander.unwrap();
    assert!(![CharacterId(107), CharacterId(109)].contains(&new));
    assert_eq!(m.world.characters[&new].kind, CharacterKind::Colonel);
    assert_eq!(m.world.characters[&new].position, at(SPOT.0, SPOT.1));
    assert!(spot_seen(&m));
    assert!(!m.world.character_details[&new].hidden);
    assert!(m.is_raiding(new));
    // The control: placing him would have hidden him.
    m.place_character(new, at(SPOT.0, SPOT.1));
    assert!(m.world.character_details[&new].hidden);
}

#[test]
fn a_merge_into_a_garrison_that_keeps_its_commander_adds_no_sight() {
    // `0x008D5F10` runs `0x008F9400` on the source alone and not when it emptied (`0x008D6F43`):
    // with 107 keeping command and no one placed, nothing adds the sight (in a plain transfer the
    // target's `0x008F9400` does, `a_merge_that_keeps_the_commander_places_no_one`).
    let away = (-40.0, 50.0);
    let mut m = transfer_model(true);
    m.world.characters.get_mut(&CharacterId(107)).unwrap().garrisoned_in = Some(RegionId(10));
    army(&mut m, 1004, 109, CharacterKind::Colonel, away, vec![keyed(9, "t_inf_rich")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    assert!(!m.world.forces.contains_key(&ForceId(1004)));
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(107)));
    assert!(!spot_seen(&m));
    assert!(!seen(&m, away));
}

#[test]
fn joining_a_garrison_places_a_new_commander_and_passes_the_residence_on() {
    // A General joins the colonel's garrison: placed (`0x008D68EC`), and the residence passes to him.
    let mut m = transfer_model(false);
    m.world.characters.get_mut(&CharacterId(107)).unwrap().garrisoned_in = Some(RegionId(10));
    m.world.regions.get_mut(&RegionId(10)).unwrap().garrison = Some(ForceId(1002));
    army(&mut m, 1004, 109, CharacterKind::General, SPOT, vec![keyed(9, "t_inf_rich")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(109)));
    assert!(spot_seen(&m));
    assert_eq!(m.world.characters[&CharacterId(109)].garrisoned_in, Some(RegionId(10)));
    assert_eq!(m.world.characters[&CharacterId(107)].garrisoned_in, None);
    assert!(m.is_raiding(CharacterId(109)));
    // A colonel made for the garrison is not placed: `0x009D3B60` puts him on the map only after the
    // placement test (`0x008D6933`). Inside a settlement he cannot hide and his raids pass to him
    // either way, so that shows only in the code. The source emptied, so `0x008D5F10` runs no
    // `0x008F9400`: the sight is his own, from `0x009D3B60`.
    let mut m = transfer_model(true);
    m.world.characters.get_mut(&CharacterId(107)).unwrap().garrisoned_in = Some(RegionId(10));
    army(&mut m, 1004, 109, CharacterKind::Colonel, SPOT, vec![keyed(9, "t_inf_rich"), keyed(12, "t_cav")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    let new = m.world.forces[&ForceId(1002)].commander.unwrap();
    assert_ne!(new, CharacterId(107));
    assert!(spot_seen(&m));
    assert_eq!(m.world.characters[&new].garrisoned_in, Some(RegionId(10)));
    // One made for the force left outside is: `0x008D6689` puts him on the map before `0x008D6812`.
    // The garrison has room for one unit, so the source keeps its commander's unit and a cavalry
    // unit, which gets the new colonel.
    let away = (-40.0, 50.0);
    let mut m = transfer_model(true);
    m.force_caps.army = 2;
    m.world.characters.get_mut(&CharacterId(107)).unwrap().garrisoned_in = Some(RegionId(10));
    army(&mut m, 1004, 109, CharacterKind::Colonel, away, vec![keyed(9, "t_inf_rich"), keyed(12, "t_inf_rich"), keyed(13, "t_cav")]);
    m.merge_units(ForceId(1004), ForceId(1002), &mut Vec::new());
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(107)));
    let new = m.world.forces[&ForceId(1004)].commander.unwrap();
    assert_ne!(new, CharacterId(109));
    assert_eq!(m.world.characters[&new].position, at(away.0, away.1));
    assert!(seen(&m, away));
}

#[test]
fn a_partial_transfer_re_picks_the_source_and_its_new_commander_keeps_the_raids() {
    // The source keeps its colonel's unit and a General's: the General takes command, is placed
    // where the colonel stands, and the colonel's raids pass to him. ORIGINAL BUG: `0x008D47B0`
    // hands them over (`0x008D5078`) before the placement (`0x008D5299`) ends them.
    let mut m = transfer_model(false);
    m.force_caps.army = 2;
    let f = m.world.forces.get_mut(&ForceId(1002)).unwrap();
    f.units.push(keyed(10, "t_inf_rich"));
    f.units.push(CampaignUnit { character: Some(CharacterId(109)), ..keyed(9, "t_inf_rich") });
    let mut g = character(109, A, CharacterKind::General, 25);
    g.position = at(-40.0, 50.0);
    m.world.characters.insert(g.id, g);
    army(&mut m, 1005, 111, CharacterKind::Colonel, (0.0, 50.0), vec![keyed(11, "t_inf_rich")]);
    m.merge_units(ForceId(1002), ForceId(1005), &mut Vec::new());
    // Unit 10 moved; the target keeps its colonel.
    assert_eq!(m.world.forces[&ForceId(1005)].units.len(), 2);
    assert_eq!(m.world.forces[&ForceId(1005)].commander, Some(CharacterId(111)));
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(109)));
    assert_eq!(m.world.characters[&CharacterId(109)].position, at(SPOT.0, SPOT.1));
    assert!(spot_seen(&m));
    assert!(m.is_raiding(CharacterId(109)));
    assert!(!m.is_raiding(CharacterId(107)));
}

#[test]
fn an_army_boarding_onto_a_carried_one_re_picks_its_commander() {
    let mut m = raid_pool_model();
    m.world.forces.get_mut(&ForceId(1002)).unwrap().units[0].character = Some(CharacterId(107));
    m.world.embarked.insert(ForceId(1002), ForceId(1003));
    army(&mut m, 1004, 109, CharacterKind::General, SPOT, vec![unit(9)]);
    m.turn.humans = vec![A];
    m.start_campaign();
    watch(&mut m);
    m.apply(CampaignCommand::Embark { force: ForceId(1004), navy: ForceId(1003) }).unwrap();
    assert!(!m.world.forces.contains_key(&ForceId(1004)));
    assert_eq!(m.world.forces[&ForceId(1002)].commander, Some(CharacterId(109)));
    assert!(spot_seen(&m));
}

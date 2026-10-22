//! Campaign model tests: a character's attribute levels and his rank (the command stars,
//! CHARACTERS_FIDELITY.md §16).

use super::*;
use super::agents::{rank, ATTRIBUTES};
use super::characters::{add_ancillary, add_trait_points, promoted_attributes, remove_ancillary, remove_trait, AncillaryRule, TraitOutcome, TraitRule};
use super::details::{CharacterTrait, GovernmentPost};

/// The test mapping of every rank bonus: DB key `t_<bonus>` → the basic bonus `<bonus>`.
const RANK_BONUSES: [&str; 10] = [
    "command_land_infantry",
    "command_land_cavalry",
    "command_land_artillery",
    "command_naval_frigates",
    "command_naval_ships_of_the_line",
    "management_army",
    "management_navy",
    "management_finance",
    "management_justice",
    "zeal_europe",
];

/// [`test_model`] whose mapping maps every [`RANK_BONUSES`] key, with the pick units, and a hidden
/// trait `R` (one level, `R_1` at 1 point) whose effects are `bonuses` (bonus, value).
fn rank_model(bonuses: &[(&str, f32)]) -> CampaignModel {
    let mut m = test_model_with(RANK_BONUSES.iter().map(|b| map_basic(&format!("t_{b}"), b)));
    with_pick_units(&mut m);
    let rules = Arc::make_mut(&mut m.rules);
    rules.effects.trait_levels.insert("R".into(), vec![(1, 1, "R_1".into())]);
    let rows: Vec<(String, f32)> = bonuses.iter().map(|(b, v)| (format!("t_{b}"), *v)).collect();
    rules.effects.insert_trait_level("R_1".into(), rows.iter().map(|(k, v)| (k.as_str(), *v)));
    m
}

/// Gives character `c` trait `R` (so its [`rank_model`] bonuses) and the attribute `attr` at `level`.
fn with_rank_trait(m: &mut CampaignModel, c: i32, attr: &str, level: i32) {
    let d = m.world.character_details.entry(CharacterId(c)).or_default();
    d.traits = vec![CharacterTrait { key: "R".into(), points: 1, level: 1 }];
    d.attributes = vec![(attr.into(), level)];
}

fn army(m: &mut CampaignModel, units: &[&str]) {
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units = units.iter().enumerate().map(|(i, k)| keyed(50 + i as i32, k)).collect();
}

const ARMY_BONUSES: [(&str, f32); 3] = [("command_land_infantry", 1.0), ("command_land_cavalry", 2.0), ("command_land_artillery", 4.0)];

#[test]
fn a_new_character_starts_at_minus_one_then_his_types_rows() {
    let mut rules = CampaignRules::test_rules();
    rules.characters.agent_attributes.insert("General".into(), vec![("command_land".into(), 0), ("made_up".into(), 2)]);
    let start = rules.characters.starting_attributes("General");
    assert_eq!(start.len(), ATTRIBUTES.len() + 1);
    for (i, a) in ATTRIBUTES.iter().enumerate() {
        assert_eq!(start[i], (a.to_string(), if *a == "command_land" { 0 } else { -1 }));
    }
    // A row naming an attribute outside the list is appended.
    assert_eq!(start[ATTRIBUTES.len()], ("made_up".to_string(), 2));
    // A type without rows has every attribute at -1.
    assert!(rules.characters.starting_attributes("rake").iter().all(|(_, l)| *l == -1));
    let d = rules.characters.new_character_details("General");
    assert_eq!(d.attributes, start);
    // The situation slot (#14) starts at one `dummy` 0, as the maker clears details +0x13C
    // (`0x00992E60` at `0x00992F5D`) and the writer writes one entry per key of `0x0145DA24`.
    assert_eq!(d.attribute_bonuses, vec![("dummy".to_string(), 0)]);
}

#[test]
fn the_colonel_who_takes_over_starts_with_his_types_levels() {
    let mut m = test_model();
    Arc::make_mut(&mut m.rules).characters.agent_attributes.insert("colonel".into(), vec![("command_land".into(), 0)]);
    let year = m.calendar.date.year;
    m.world.character_details.entry(CharacterId(100)).or_default().birth = Some(Date { year: year - 120, season: 0, month: 0, half: HALF_EARLY });
    m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
    assert!(super::characters::yearly_character_pass(&mut m).died.contains(&CharacterId(100)));
    let c = m.world.forces[&ForceId(1000)].commander.expect("commander");
    assert_eq!(m.world.character_details[&c].attributes, m.rules.characters.starting_attributes("colonel"));
}

/// A model with trait `T` (levels 1 at 2 points, 2 at 5) whose levels raise `command_land` by 1 and
/// 3, antitrait `U` (one level at 1 point, `command_land` −2), and General 100 at `command_land` 2.
fn trait_model() -> CampaignModel {
    let mut m = test_model();
    let rules = Arc::make_mut(&mut m.rules);
    let cr = &mut rules.characters;
    cr.traits.insert("T".into(), TraitRule { thresholds: vec![(1, 2), (2, 5)], antitraits: vec!["U".into()], ..Default::default() });
    cr.traits.insert("U".into(), TraitRule { thresholds: vec![(1, 1)], antitraits: vec!["T".into()], ..Default::default() });
    cr.max_traits = 10;
    let fx = &mut rules.effects;
    fx.trait_levels.insert("T".into(), vec![(2, 1, "T_1".into()), (5, 2, "T_2".into())]);
    fx.trait_levels.insert("U".into(), vec![(1, 1, "U_1".into())]);
    fx.trait_attribute.insert("T_1".into(), vec![("command_land".into(), 1)]);
    fx.trait_attribute.insert("T_2".into(), vec![("command_land".into(), 3)]);
    fx.trait_attribute.insert("U_1".into(), vec![("command_land".into(), -2)]);
    command_land(&mut m, 100, 2);
    m
}

fn land(m: &CampaignModel) -> i32 {
    super::effects::Effects::character_attribute(m, CharacterId(100), "command_land").unwrap_or(-1)
}

#[test]
fn trait_levels_move_the_stored_attribute() {
    let mut m = trait_model();
    // Below the first threshold: no level, no effect.
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "T", 1), TraitOutcome::Added { level: 0, evicted: None });
    assert_eq!(land(&m), 2);
    // Level 1 (+1), then level 2 (+3 instead of +1).
    add_trait_points(&mut m, CharacterId(100), "T", 1);
    assert_eq!(land(&m), 3);
    add_trait_points(&mut m, CharacterId(100), "T", 3);
    assert_eq!(land(&m), 5);
    // The antitrait takes 4 points off T (5 → 1, below its first level): it keeps level 2 (0x008B5380).
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "U", 4), TraitOutcome::Absorbed);
    assert_eq!(land(&m), 5);
    // Two more: T (1 point) is removed with its level 2 effect, and the overflow (1) adds U (−2).
    assert!(matches!(add_trait_points(&mut m, CharacterId(100), "U", 2), TraitOutcome::Added { level: 1, .. }));
    assert!(m.world.character_details[&CharacterId(100)].traits.iter().all(|t| t.key != "T"));
    assert_eq!(land(&m), 0);
}

#[test]
fn an_antitrait_removing_a_trait_takes_its_effect_off() {
    // ORIGINAL BUG 0x008BDB20: the exe keeps the removed trait's attribute effects.
    let mut m = trait_model();
    add_trait_points(&mut m, CharacterId(100), "T", 2);
    assert_eq!(land(&m), 3);
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "U", 2), TraitOutcome::Absorbed);
    assert!(m.world.character_details[&CharacterId(100)].traits.is_empty());
    assert_eq!(land(&m), 2);
}

#[test]
fn an_evicted_trait_takes_its_effect_off() {
    let mut m = trait_model();
    {
        let cr = &mut Arc::make_mut(&mut m.rules).characters;
        cr.max_traits = 1;
        cr.traits.insert("V".into(), TraitRule { thresholds: vec![(1, 1)], priority: 5, ..Default::default() });
    }
    add_trait_points(&mut m, CharacterId(100), "T", 2);
    assert_eq!(land(&m), 3);
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "V", 1), TraitOutcome::Added { level: 1, evicted: Some("T".into()) });
    assert_eq!(land(&m), 2);
}

#[test]
fn remove_trait_keeps_a_trait_at_its_no_going_back_level() {
    let mut m = trait_model();
    Arc::make_mut(&mut m.rules).characters.traits.get_mut("T").unwrap().no_going_back_level = 2;
    // Level 1 (< 2): removed, its effect with it.
    add_trait_points(&mut m, CharacterId(100), "T", 2);
    assert!(remove_trait(&mut m, CharacterId(100), "T"));
    assert_eq!(land(&m), 2);
    // Level 2: kept.
    add_trait_points(&mut m, CharacterId(100), "T", 5);
    assert!(!remove_trait(&mut m, CharacterId(100), "T"));
    assert_eq!(land(&m), 5);
    // No-going-back level 0: never removed (level 0 ≥ 0), even below the first threshold (where the
    // exe crashes, ORIGINAL BUG 0x008E98A0).
    let mut m = trait_model();
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().traits.push(CharacterTrait { key: "T".into(), points: 1, level: 0 });
    assert!(!remove_trait(&mut m, CharacterId(100), "T"));
    // A trait the rules do not know is removed; one not held is not.
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().traits.push(CharacterTrait { key: "gone".into(), points: 3, level: 0 });
    assert!(remove_trait(&mut m, CharacterId(100), "gone"));
    assert!(!remove_trait(&mut m, CharacterId(100), "gone"));
}

/// [`trait_model`] with General ancillaries `A1` (`command_land` +2, priority 1) and `A2` (+1,
/// priority 2), valid until `end`, for any subculture "" of A, at most one held.
fn ancillary_model(end: i32) -> CampaignModel {
    let mut m = trait_model();
    let rules = Arc::make_mut(&mut m.rules);
    for (k, v, p) in [("A1", 2, 1), ("A2", 1, 2)] {
        rules.characters.ancillaries.insert(
            k.into(),
            AncillaryRule { character: true, priority: p, start_year: 0, end_year: end, agents: vec!["General".into()], subcultures: vec![String::new()], ..Default::default() },
        );
        rules.effects.ancillary_attribute.insert(k.into(), vec![("command_land".into(), v)]);
    }
    rules.characters.max_ancillaries = 1;
    m
}

#[test]
fn ancillaries_move_the_stored_attribute() {
    let mut m = ancillary_model(9999);
    assert_eq!(add_ancillary(&mut m, CharacterId(100), "A1"), Ok(None));
    assert_eq!(land(&m), 4);
    // A2 outranks A1 and evicts it: A1's +2 off, A2's +1 on.
    assert_eq!(add_ancillary(&mut m, CharacterId(100), "A2"), Ok(Some("A1".into())));
    assert_eq!(land(&m), 3);
    assert!(remove_ancillary(&mut m, CharacterId(100), "A2"));
    assert_eq!(land(&m), 2);
    assert!(!remove_ancillary(&mut m, CharacterId(100), "A2"));
}

#[test]
fn an_expired_ancillary_takes_its_effect_off() {
    let year = test_model().calendar.date.year as i32;
    let mut m = ancillary_model(year + 1);
    add_ancillary(&mut m, CharacterId(100), "A1").unwrap();
    assert_eq!(land(&m), 4);
    m.calendar.date.year += 1;
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().birth = Some(Date { year: m.calendar.date.year - 30, season: 0, month: 0, half: HALF_EARLY });
    m.calendar.turn_in_year = m.calendar.turns_per_year - 1;
    let pass = super::characters::yearly_character_pass(&mut m);
    assert!(pass.expired.contains(&(CharacterId(100), "A1".to_string())));
    assert_eq!(land(&m), 2);
}

#[test]
fn a_promotion_keeps_the_trait_and_ancillary_effects() {
    // ORIGINAL BUG 0x00A1A300: the exe copies the new type's levels and drops these effects.
    let mut m = ancillary_model(9999);
    Arc::make_mut(&mut m.rules).characters.agent_attributes.insert("General".into(), vec![("command_land".into(), 1)]);
    add_trait_points(&mut m, CharacterId(100), "T", 5);
    add_ancillary(&mut m, CharacterId(100), "A1").unwrap();
    let d = &m.world.character_details[&CharacterId(100)];
    let promoted = promoted_attributes(&m.rules, "General", d);
    // 1 (the type) + 3 (T level 2) + 2 (A1); every other attribute back to -1.
    assert_eq!(promoted.iter().find(|(k, _)| k == "command_land").map(|(_, l)| *l), Some(6));
    assert!(promoted.iter().filter(|(k, _)| k != "command_land").all(|(_, l)| *l == -1));
}

#[test]
fn the_army_make_up_gives_the_command_bonuses() {
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    // n = 3: infantry 2 ≥ 2, cavalry 1 ≥ 1, artillery 0 < 1.
    army(&mut m, &["t_inf_rich", "t_inf_flag", "t_cav"]);
    assert_eq!(rank(&m, CharacterId(100)), 2 + 1 + 2);
    // n = 4: infantry 2 < 3, cavalry 2 ≥ 2.
    army(&mut m, &["t_inf_rich", "t_inf_flag", "t_cav", "t_cav"]);
    assert_eq!(rank(&m, CharacterId(100)), 2 + 2);
    // A unit the rules do not know counts in n only.
    army(&mut m, &["t_inf_rich", "t_inf_flag", "unknown"]);
    assert_eq!(rank(&m, CharacterId(100)), 2 + 1);
    // An empty army meets every share; the sum (5 + 7) is clamped at 9.
    army(&mut m, &[]);
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().attributes = vec![("command_land".into(), 5)];
    assert_eq!(rank(&m, CharacterId(100)), 9);
    // Without an army, no bonus.
    m.world.forces.remove(&ForceId(1000));
    assert_eq!(rank(&m, CharacterId(100)), 5);
}

/// The saved situation slot (`CHARACTER_DETAILS` #14, details +0x13C) and the situation tables give no
/// rank: `GetCharacterRank` `0x00A198D0` never reads them (CHARACTERS_FIDELITY.md §17), so neither a
/// `dummy` value nor a situation key there moves the stars.
#[test]
fn the_situation_slot_gives_no_rank() {
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    m.world.forces.remove(&ForceId(1000));
    assert_eq!(rank(&m, CharacterId(100)), 2);
    let d = m.world.character_details.get_mut(&CharacterId(100)).unwrap();
    d.attribute_bonuses = vec![("dummy".into(), 3), ("Command_Land_Attack".into(), 4)];
    assert_eq!(rank(&m, CharacterId(100)), 2);
}

#[test]
fn the_rank_clamp_is_data() {
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 5);
    army(&mut m, &[]);
    Arc::make_mut(&mut m.rules).characters.rank_limits = (-1, 20);
    assert_eq!(rank(&m, CharacterId(100)), 12);
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().attributes = vec![("command_land".into(), -5)];
    m.world.forces.remove(&ForceId(1000));
    assert_eq!(rank(&m, CharacterId(100)), -1);
}

#[test]
fn the_fleet_make_up_gives_the_naval_bonuses() {
    let mut m = rank_model(&[("command_naval_frigates", 1.0), ("command_naval_ships_of_the_line", 2.0), ("command_land_infantry", 4.0)]);
    m.world.characters.insert(CharacterId(200), character(200, A, CharacterKind::Admiral, 30));
    let ships = ["t_frig", "t_frig", "t_line"].iter().enumerate().map(|(i, k)| keyed(60 + i as i32, k)).collect();
    m.world.forces.insert(ForceId(2000), MilitaryForce { id: ForceId(2000), faction: A, commander: Some(CharacterId(200)), units: ships, is_navy: true });
    with_rank_trait(&mut m, 200, "command_sea", 3);
    // Frigates 2 ≥ 2, ships of the line 1 < 2.
    assert_eq!(rank(&m, CharacterId(200)), 3 + 1);
    // A General commanding a navy gets no land bonus from it.
    with_rank_trait(&mut m, 100, "command_land", 2);
    m.world.forces.get_mut(&ForceId(1000)).unwrap().is_navy = true;
    assert_eq!(rank(&m, CharacterId(100)), 2);
}

#[test]
fn a_ministers_post_gives_its_management_bonus() {
    let mut m = rank_model(&[("management_finance", 2.0), ("management_army", 5.0)]);
    with_rank_trait(&mut m, 102, "management", 3);
    assert_eq!(rank(&m, CharacterId(102)), 3);
    let post = |key: &str, holder: i32| GovernmentPost { id: 1, key: key.into(), holder: Some(CharacterId(holder)), governorship: None };
    m.world.faction_details.entry(A).or_default().posts = vec![post("finance", 102)];
    assert_eq!(rank(&m, CharacterId(102)), 5);
    m.world.faction_details.entry(A).or_default().posts = vec![post("army", 102)];
    assert_eq!(rank(&m, CharacterId(102)), 8);
    // A General holding a post gets no management bonus (his main attribute is command_land).
    with_rank_trait(&mut m, 100, "command_land", 2);
    m.world.forces.remove(&ForceId(1000));
    m.world.faction_details.entry(A).or_default().posts = vec![post("army", 100)];
    assert_eq!(rank(&m, CharacterId(100)), 2);
}

#[test]
fn a_type_without_a_main_attribute_has_rank_minus_one() {
    let mut m = rank_model(&[]);
    m.world.characters.insert(CharacterId(103), character(103, A, CharacterKind::Rake, 20));
    agent_details(&mut m, 103, &[], &[("subterfuge", 5)]);
    assert_eq!(rank(&m, CharacterId(103)), 5);
    Arc::make_mut(&mut m.rules).agent_primary_attributes.remove("rake");
    assert_eq!(rank(&m, CharacterId(103)), -1);
    assert_eq!(rank(&m, CharacterId(9999)), -1);
}

#[test]
fn zeal_counts_the_own_and_faction_bonus_only_where_the_feature_is_on() {
    let mut m = rank_model(&[("zeal_europe", 2.0)]);
    m.world.characters.insert(CharacterId(103), character(103, A, CharacterKind::CatholicMissionary, 20));
    with_rank_trait(&mut m, 103, "zeal", 1);
    with_technology(&mut m, &[("t_zeal_europe", 1.0)]);
    assert_eq!(rank(&m, CharacterId(103)), 1);
    Arc::make_mut(&mut m.rules).features.faction_zeal = true;
    // 1 + his own 2 + the faction's 1.
    assert_eq!(rank(&m, CharacterId(103)), 4);
    // The faction set's call leaves the faction term out.
    let own = super::effects::Effects::character_effects(&m, CharacterId(103));
    assert_eq!(super::agents::rank_original(&m, CharacterId(103), &own, false), 3);
}

#[test]
fn a_mod_rule_replaces_the_rank() {
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    let maker: crate::seam::Maker<super::seams::CharacterRank> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: &super::effects::EffectSet, _: bool| 7));
    Arc::make_mut(&mut m.rules).seams.character_rank.apply("test.seven", &maker);
    assert_eq!(rank(&m, CharacterId(100)), 7);
}

/// The battle card of unit `i` of force `f`.
fn card(m: &CampaignModel, f: u32, i: usize) -> super::battles::BattleGeneralCard {
    let force = &m.world.forces[&ForceId(f)];
    m.battle_general_card(force, &force.units[i])
}

#[test]
fn the_battle_card_carries_the_rank_and_the_command_role() {
    use super::battles::BattleCommandRole as Role;
    let mut m = test_model();
    // Army 1000: General 100 (its commander) on unit 1, General 103 and colonel 104 under him, a unit
    // without a character. Army 1001: colonel 101 commands it from unit 2.
    m.world.characters.insert(CharacterId(103), character(103, A, CharacterKind::General, 30));
    m.world.characters.insert(CharacterId(104), character(104, A, CharacterKind::Colonel, 25));
    command_land(&mut m, 100, 3);
    command_land(&mut m, 103, 5);
    command_land(&mut m, 104, 1);
    command_land(&mut m, 101, 2);
    let f = m.world.forces.get_mut(&ForceId(1000)).unwrap();
    f.units = vec![CampaignUnit { character: Some(CharacterId(100)), ..unit(1) }, CampaignUnit { character: Some(CharacterId(103)), ..unit(4) }, CampaignUnit { character: Some(CharacterId(104)), ..unit(5) }, unit(6)];
    m.world.forces.get_mut(&ForceId(1001)).unwrap().units[0].character = Some(CharacterId(101));

    let c = card(&m, 1000, 0);
    assert_eq!((c.role, c.rank, c.role.card_value(false), c.army_general_rank()), (Role::Commander, 3, 1, Some(3)));
    // A General under another one keeps his own rank on the card but is not the army's general.
    let c = card(&m, 1000, 1);
    assert_eq!((c.role, c.rank, c.role.card_value(false), c.army_general_rank()), (Role::Subordinate, 5, 3, None));
    // A colonel in someone else's army gets no role.
    let c = card(&m, 1000, 2);
    assert_eq!((c.role, c.rank, c.role.card_value(false), c.army_general_rank()), (Role::None, 1, 0, None));
    // No character: the default record.
    let c = card(&m, 1000, 3);
    assert_eq!((c.role, c.rank, c.army_general_rank()), (Role::None, 0, None));
    // A colonel commanding his own unit is its army's general (card 2).
    let c = card(&m, 1001, 0);
    assert_eq!((c.role, c.rank, c.role.card_value(false), c.army_general_rank()), (Role::OtherCommander, 2, 2, Some(2)));

    // A force without a (living) commander: the General on it counts as commanding it.
    m.world.forces.get_mut(&ForceId(1000)).unwrap().commander = Some(CharacterId(999));
    assert_eq!(card(&m, 1000, 1).role, Role::Commander);
    m.world.forces.get_mut(&ForceId(1000)).unwrap().commander = None;
    assert_eq!(card(&m, 1000, 2).role, Role::OtherCommander);

    // The naval builder's values.
    assert_eq!([Role::None, Role::Commander, Role::OtherCommander, Role::Subordinate].map(|r| r.card_value(true)), [0, 4, 5, 6]);
}

#[test]
fn the_battle_card_reads_the_rank_through_its_seam() {
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    m.world.forces.get_mut(&ForceId(1000)).unwrap().units[0].character = Some(CharacterId(100));
    assert_eq!(card(&m, 1000, 0).rank, rank(&m, CharacterId(100)));
    let maker: crate::seam::Maker<super::seams::CharacterRank> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: &super::effects::EffectSet, _: bool| 7));
    Arc::make_mut(&mut m.rules).seams.character_rank.apply("test.seven", &maker);
    assert_eq!(card(&m, 1000, 0).army_general_rank(), Some(7));
}

#[test]
fn the_autoresolve_star_ratings_are_the_battle_cards_ranks() {
    // 0x007CA330: land card +0xBC / naval card +0xE8 → resolver unit +0xAC, for every unit with a character.
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    m.world.characters.insert(CharacterId(104), character(104, A, CharacterKind::Colonel, 25));
    command_land(&mut m, 104, 1);
    let f = m.world.forces.get_mut(&ForceId(1000)).unwrap();
    f.units = vec![CampaignUnit { character: Some(CharacterId(100)), ..unit(1) }, CampaignUnit { character: Some(CharacterId(104)), ..unit(2) }, unit(3)];
    let ranks = |m: &CampaignModel| -> (Vec<i32>, Vec<i32>) {
        let land = m.autoresolve_units(&[ForceId(1000)]).iter().map(|x| x.2.star_rating).collect();
        let naval = m.naval_autoresolve_units(&[ForceId(1000)]).iter().map(|x| x.2.star_rating).collect();
        (land, naval)
    };
    let expected = vec![card(&m, 1000, 0).rank, 1, 0];
    assert_eq!(expected[0], rank(&m, CharacterId(100)));
    assert_eq!(ranks(&m), (expected.clone(), expected));
    // Through the rank seam (the one rank rule), for both resolvers.
    let maker: crate::seam::Maker<super::seams::CharacterRank> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: &super::effects::EffectSet, _: bool| 7));
    Arc::make_mut(&mut m.rules).seams.character_rank.apply("test.seven", &maker);
    assert_eq!(ranks(&m), (vec![7, 7, 0], vec![7, 7, 0]));
}

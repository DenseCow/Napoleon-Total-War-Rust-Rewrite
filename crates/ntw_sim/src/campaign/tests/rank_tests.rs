//! Campaign model tests: a character's attribute levels and his rank (the command stars,
//! CHARACTERS_FIDELITY.md §16).

use super::*;
use super::agents::{rank, ATTRIBUTES};
use super::characters::{add_ancillary, add_trait_points, promoted_attributes, remove_ancillary, remove_trait, AncillaryRule, TraitOutcome, TraitRule};
use super::details::{CharacterTrait, GovernmentPost};

/// The test mapping of every rank bonus: DB key `t_<bonus>` → the basic bonus `<bonus>`.
const RANK_BONUSES: [&str; 21] = [
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
    "land_command_attack",
    "land_command_defence",
    "land_command_ambush",
    "land_command_siege_attack",
    "land_command_siege_defence",
    "sea_command_attack",
    "sea_command_defence",
    "hates_ottomans",
    "hates_europeans",
    "hates_french",
    "hates_british",
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

/// Character 100's level of ability `key` as the action code reads it (`0x009C7610`).
fn ability_level(m: &CampaignModel, key: &str) -> i32 {
    super::agents::ability(m, CharacterId(100), key)
}

#[test]
fn trait_and_ancillary_ability_effects_move_the_ability_level() {
    // 0x00A0ECB0 / 0x00A0EC10: the second loop adds each row to the ability slot's level.
    let mut m = ancillary_model(9999);
    {
        let rules = Arc::make_mut(&mut m.rules);
        rules.effects.trait_ability.insert("T_1".into(), vec![("can_spy".into(), 1)]);
        rules.effects.trait_ability.insert("T_2".into(), vec![("can_spy".into(), 2)]);
        rules.effects.ancillary_ability.insert("A1".into(), vec![("can_spy".into(), 3), ("can_duel".into(), 1)]);
        rules.characters.traits.get_mut("T").unwrap().no_going_back_level = 9;
    }
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().abilities = vec![("can_spy".into(), 0, String::new())];
    // T level 1 (+1), then level 2 (+2 instead of +1).
    add_trait_points(&mut m, CharacterId(100), "T", 2);
    assert_eq!(ability_level(&m, "can_spy"), 1);
    add_trait_points(&mut m, CharacterId(100), "T", 3);
    assert_eq!(ability_level(&m, "can_spy"), 2);
    // A1: can_spy +3; can_duel, which he lacks, is a reset slot (−1) plus 1.
    add_ancillary(&mut m, CharacterId(100), "A1").unwrap();
    assert_eq!((ability_level(&m, "can_spy"), ability_level(&m, "can_duel")), (5, 0));
    assert!(remove_ancillary(&mut m, CharacterId(100), "A1"));
    assert_eq!((ability_level(&m, "can_spy"), ability_level(&m, "can_duel")), (2, -1));
    assert!(remove_trait(&mut m, CharacterId(100), "T"));
    assert_eq!(ability_level(&m, "can_spy"), 0);
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
    assert_eq!(super::agents::rank_original(&m, CharacterId(103), &super::effects::OwnEffects::held(&own), false), 3);
}

/// polish72: the rank reads one bonus of the faction's sum without building it
/// (`Effects::faction_bonus`); it is the built sum's value for every source: saved base entries
/// (basic, an id outside the basic names, another type), technologies and a minister's own set.
#[test]
fn one_faction_bonus_is_the_built_sums_value() {
    use super::effects::{Effects, SavedBonus};
    let mut m = rank_model(&[("land_command_attack", 2.0), ("zeal_europe", 0.25)]);
    with_rank_trait(&mut m, 102, "management", 3);
    with_technology(&mut m, &[("t_land_command_attack", 3.0), ("t_zeal_europe", 1.5)]);
    let basic = |name: &str| super::effects::BONUS_NAMES.iter().position(|n| *n == name).unwrap() as i32;
    let d = m.world.faction_details.entry(A).or_default();
    d.posts = vec![GovernmentPost { id: 1, key: "army".into(), holder: Some(CharacterId(102)), governorship: None }];
    d.bonus_base = vec![
        SavedBonus { kind: 1, bonus: basic("land_command_attack"), value: 0.5, ..Default::default() },
        SavedBonus { kind: 1, bonus: basic("land_command_attack"), value: 0.25, ..Default::default() },
        SavedBonus { kind: 1, bonus: 0x98, value: 9.0, ..Default::default() },
        SavedBonus { kind: 6, bonus: basic("land_command_attack"), value: 7.0, qualifier: "upper".into(), ..Default::default() },
    ];
    let sum = Effects::faction_sum(&m, A);
    for bonus in RANK_BONUSES.iter().copied().chain(["conversion", "not_a_bonus"]) {
        assert_eq!(Effects::faction_bonus(&m, A, bonus), sum.get(bonus), "{bonus}");
    }
    assert_eq!(Effects::faction_bonus(&m, A, "land_command_attack"), 0.5 + 0.25 + 3.0 + 2.0);
    assert_eq!(Effects::faction_bonus(&m, FactionId(9999), "land_command_attack"), 0.0);
}

#[test]
fn a_mod_rule_replaces_the_rank() {
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    let maker: crate::seam::Maker<super::seams::CharacterRank> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: &super::effects::OwnEffects<'_>, _: bool| 7));
    Arc::make_mut(&mut m.rules).seams.character_rank.apply("test.seven", &maker);
    assert_eq!(rank(&m, CharacterId(100)), 7);
}

/// The rank was built from the character's whole effect set on every call; the set is now built only
/// by a rank that reads one of its bonuses, and gives the same rank as a set held beforehand.
#[test]
fn the_rank_builds_the_own_set_only_when_it_reads_it() {
    use super::effects::{Effects, OwnEffects};
    let mut m = rank_model(&ARMY_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    army(&mut m, &["t_inf_rich", "t_inf_flag", "t_cav"]);
    let c = CharacterId(100);
    // Commanding an army whose make-up earns a bonus: read, so built.
    let own = OwnEffects::of(&m, c);
    assert!(!own.is_built());
    assert_eq!(super::agents::rank_original(&m, c, &own, true), 2 + 1 + 2);
    assert!(own.is_built());
    let held = Effects::character_effects(&m, c);
    assert_eq!(super::agents::rank_original(&m, c, &OwnEffects::held(&held), true), 2 + 1 + 2);
    // Without an army no bonus is read: the same trait, no set.
    m.world.forces.remove(&ForceId(1000));
    let own = OwnEffects::of(&m, c);
    assert_eq!(super::agents::rank_original(&m, c, &own, true), 2);
    assert!(!own.is_built());
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
    let maker: crate::seam::Maker<super::seams::CharacterRank> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: &super::effects::OwnEffects<'_>, _: bool| 7));
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
    let maker: crate::seam::Maker<super::seams::CharacterRank> = Arc::new(|_| Arc::new(|_: &CampaignModel, _: CharacterId, _: &super::effects::OwnEffects<'_>, _: bool| 7));
    Arc::make_mut(&mut m.rules).seams.character_rank.apply("test.seven", &maker);
    assert_eq!(ranks(&m), (vec![7, 7, 0], vec![7, 7, 0]));
}

/// The stance bonuses of the pending-battle tests, one bit each.
const STANCE_BONUSES: [(&str, f32); 6] = [
    ("land_command_attack", 1.0),
    ("land_command_defence", 2.0),
    ("land_command_siege_attack", 4.0),
    ("land_command_siege_defence", 8.0),
    ("sea_command_attack", 16.0),
    ("sea_command_defence", 32.0),
];

/// Force `attacker` attacks force `defender` (and the settlement of region `settlement`); returns the
/// battle's type.
fn pend(m: &mut CampaignModel, attacker: u32, defender: u32, settlement: Option<u32>) -> BattleType {
    let battle = PendingBattle { attacker: ForceId(attacker), defenders: vec![ForceId(defender)], settlement: settlement.map(RegionId), resume: None, ambush: false };
    let t = m.battle_type(&battle);
    m.pending_battle = Some(battle);
    t
}

/// Character `c`'s pending-battle bonus (the faction set's call).
fn battle_bonus(m: &CampaignModel, c: i32) -> i32 {
    super::agents::pending_battle_bonus(m, CharacterId(c), &super::effects::OwnEffects::of(m, CharacterId(c)), true)
}

/// `0x00A198D0` at `0x00A19B32`..`0x00A19C7F`: the stance bonus by the battle's type and the side he
/// commands, for the two commanders and nobody else, only while the battle is pending.
#[test]
fn the_pending_battle_gives_the_stance_bonus_by_type_and_side() {
    let mut m = rank_model(&STANCE_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    with_rank_trait(&mut m, 101, "command_land", 1);
    with_rank_trait(&mut m, 102, "command_land", 1);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101), rank(&m, CharacterId(100))), (0, 0, 2));
    // A field battle: attack / defence, in the rank too; the bystander gets nothing.
    assert_eq!(pend(&mut m, 1000, 1001, None), BattleType::LandNormal);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101), battle_bonus(&m, 102)), (1, 2, 0));
    assert_eq!(rank(&m, CharacterId(100)), 2 + 1);
    pend(&mut m, 1001, 1000, None);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101)), (2, 1));
    // An unwalled settlement (9) as a field battle; a walled one (6) gives the siege bonuses.
    assert_eq!(pend(&mut m, 1000, 1001, Some(11)), BattleType::SettlementUnfortified);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101)), (1, 2));
    m.world.regions.get_mut(&RegionId(11)).unwrap().fortification = Some(BuildingRef { level_key: "walls".into(), health: 100 });
    assert_eq!(pend(&mut m, 1000, 1001, Some(11)), BattleType::SettlementStandard);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101)), (4, 8));
    // Two navies at sea (11), the attacker docked (12): the sea bonuses.
    for f in [1000, 1001] {
        m.world.forces.get_mut(&ForceId(f)).unwrap().is_navy = true;
    }
    assert_eq!(pend(&mut m, 1000, 1001, None), BattleType::NavalNormal);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101)), (16, 32));
    let slot = &mut m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0];
    (slot.port, slot.position) = (true, Some(pos(0, 0)));
    assert_eq!(pend(&mut m, 1000, 1001, None), BattleType::NavalBlockade);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101)), (16, 32));
    // An attack on a navy blockading a port, from outside it (13): nothing.
    m.world.characters.get_mut(&CharacterId(100)).unwrap().position = pos(50, 50);
    m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].blockade = Some(PortBlockade { navy: ForceId(1001), docked_navy_attacked: false });
    assert_eq!(pend(&mut m, 1000, 1001, None), BattleType::NavalBreakout);
    assert_eq!((battle_bonus(&m, 100), battle_bonus(&m, 101)), (0, 0));
    // Settled: no bonus.
    m.pending_battle = None;
    assert_eq!(battle_bonus(&m, 100), 0);
}

/// The attacker in a land battle also gets his faction's `land_command_attack` (faction +0x6FC through
/// character +0x2A0), unless the caller is the faction set's own (the minister level).
#[test]
fn the_land_attacker_adds_his_factions_attack_bonus() {
    let mut m = rank_model(&STANCE_BONUSES);
    with_rank_trait(&mut m, 100, "command_land", 2);
    with_rank_trait(&mut m, 101, "command_land", 1);
    with_technology(&mut m, &[("t_land_command_attack", 3.0)]);
    pend(&mut m, 1000, 1001, None);
    assert_eq!(battle_bonus(&m, 100), 1 + 3);
    let own = super::effects::Effects::character_effects(&m, CharacterId(100));
    assert_eq!(super::agents::pending_battle_bonus(&m, CharacterId(100), &super::effects::OwnEffects::held(&own), false), 1);
    // As the defender he gets only his own defence.
    pend(&mut m, 1001, 1000, None);
    assert_eq!(battle_bonus(&m, 100), 2);
}

/// The vanilla enemy bonuses (`ntw_campaign::features`): ottomans by key, european by culture,
/// spa_france by key, spa_britain by key (the exe's culture test at `0x00A19D35` is the ORIGINAL BUG),
/// with three made-up european factions and the rebels' "" in the culture too.
fn with_enemy_bonuses(m: &mut CampaignModel) {
    use super::features::{EnemyKey, EnemyRankBonus};
    let rules = Arc::make_mut(&mut m.rules);
    let b = |bonus: &str, enemy| EnemyRankBonus { bonus: bonus.into(), enemy };
    rules.features.enemy_rank_bonuses = vec![
        b("hates_ottomans", EnemyKey::Faction("ottomans".into())),
        b("hates_europeans", EnemyKey::Culture("european".into())),
        b("hates_french", EnemyKey::Faction("spa_france".into())),
        b("hates_british", EnemyKey::Faction("spa_britain".into())),
    ];
    for key in ["spa_france", "spa_britain", "europe_test", ""] {
        rules.characters.faction_subculture.insert(key.into(), "sc_european_west".into());
    }
    rules.characters.subculture_culture.insert("sc_european_west".into(), "european".into());
}

/// `0x00A198D0` at `0x00A19C80`..`0x00A19D5F`: the bonuses against the other side's faction, by key or
/// culture, for both sides, none against the rebels.
#[test]
fn the_enemy_faction_gives_the_hates_bonuses() {
    let mut m = rank_model(&[("hates_ottomans", 1.0), ("hates_europeans", 2.0), ("hates_french", 4.0), ("hates_british", 8.0)]);
    with_rank_trait(&mut m, 100, "command_land", 2);
    with_rank_trait(&mut m, 101, "command_land", 1);
    with_enemy_bonuses(&mut m);
    pend(&mut m, 1000, 1001, None);
    let mut against = |key: &str| {
        m.world.factions.get_mut(&B).unwrap().key = key.into();
        battle_bonus(&m, 100)
    };
    assert_eq!(against("test_faction_b"), 0);
    assert_eq!(against("ottomans"), 1);
    assert_eq!(against("europe_test"), 2);
    assert_eq!(against("spa_france"), 2 + 4);
    // ORIGINAL BUG fixed: the exe never gives `hates_british`.
    assert_eq!(against("spa_britain"), 2 + 8);
    // The rebels (no faction record) give nothing, though their key "" has a culture here.
    assert_eq!(against(""), 0);
    // The defender's enemy is the attacker's faction.
    m.world.factions.get_mut(&A).unwrap().key = "ottomans".into();
    assert_eq!(battle_bonus(&m, 101), 1);
}

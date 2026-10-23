//! Campaign model tests: a held trait's level (its level record, `0x008B5380`,
//! CHARACTERS_FIDELITY.md §1): kept below the first threshold, the no-going-back drop, and the load.

use super::*;
use super::characters::{add_trait_points, load_trait_levels, remove_trait, CharacterRules, TraitOutcome, TraitRule};
use super::details::CharacterTrait;

/// Trait `S`: levels 1 at 2 points, 2 at 5, 3 at 9, no-going-back level `ngb`, losing points to
/// antitrait `U` (one level at 1 point, no effects).
fn rules(ngb: i32) -> CharacterRules {
    let mut cr = CampaignRules::test_rules().characters;
    cr.traits.insert(
        "S".into(),
        TraitRule { no_going_back_level: ngb, thresholds: vec![(1, 2), (2, 5), (3, 9)], antitraits: vec!["U".into()], ..Default::default() },
    );
    cr.traits.insert("U".into(), TraitRule { thresholds: vec![(1, 1)], ..Default::default() });
    cr.max_traits = 10;
    cr
}

fn held(points: i32, level: i32) -> CharacterTrait {
    CharacterTrait { key: "S".into(), points, level }
}

/// `rules(ngb)` in a model whose `S` levels raise `command_land` by 1, 2 and 4 (General 100 at 2) and
/// whose `S_2` level gives basic bonus `b` 3.
fn model(ngb: i32) -> CampaignModel {
    let mut m = test_model_with([map_basic("t_b", "b")]);
    let r = Arc::make_mut(&mut m.rules);
    r.characters = rules(ngb);
    r.effects.trait_levels.insert("S".into(), vec![(2, 1, "S_1".into()), (5, 2, "S_2".into()), (9, 3, "S_3".into())]);
    r.effects.trait_levels.insert("U".into(), vec![(1, 1, "U_1".into())]);
    for (k, v) in [("S_1", 1), ("S_2", 2), ("S_3", 4)] {
        r.effects.trait_attribute.insert(k.into(), vec![("command_land".into(), v)]);
    }
    r.effects.insert_trait_level("S_2".into(), [("t_b", 3.0)]);
    command_land(&mut m, 100, 2);
    m
}

fn land(m: &CampaignModel) -> i32 {
    super::effects::Effects::character_attribute(m, CharacterId(100), "command_land").unwrap_or(-1)
}

fn s(m: &CampaignModel) -> CharacterTrait {
    m.world.character_details[&CharacterId(100)].traits.iter().find(|t| t.key == "S").cloned().expect("S held")
}

#[test]
fn below_the_first_threshold_the_level_is_kept() {
    let cr = rules(0);
    for gaining in [true, false] {
        let mut t = held(1, 2);
        cr.update_trait_level(&mut t, gaining);
        assert_eq!(t, held(1, 2), "no level reached: nothing changes (gaining {gaining})");
    }
    let mut t = held(-3, 0);
    cr.update_trait_level(&mut t, true);
    assert_eq!(t, held(-3, 0));
    // Reaching a level moves it, either way.
    let mut t = held(9, 1);
    cr.update_trait_level(&mut t, true);
    assert_eq!(t.level, 3);
    let mut t = held(5, 3);
    cr.update_trait_level(&mut t, false);
    assert_eq!(t, held(5, 2));
}

#[test]
fn losing_points_from_the_no_going_back_level_drops_to_it_with_its_threshold() {
    // From level 3 to the points of level 1: level 2, points 5.
    let mut t = held(3, 3);
    rules(2).update_trait_level(&mut t, false);
    assert_eq!(t, held(5, 2));
    // Points reaching the no-going-back level itself still reset to its threshold.
    let mut t = held(8, 3);
    rules(2).update_trait_level(&mut t, false);
    assert_eq!(t, held(5, 2));
    // From level 3, no-going-back level 1: straight to level 1, though the points reach level 2.
    let mut t = held(6, 3);
    rules(1).update_trait_level(&mut t, false);
    assert_eq!(t, held(2, 1));
    // Held at the no-going-back level and falling: stays on it, points reset.
    let mut t = held(2, 2);
    rules(2).update_trait_level(&mut t, false);
    assert_eq!(t, held(5, 2));
}

#[test]
fn the_no_going_back_drop_needs_a_loss_below_a_reached_level() {
    // No no-going-back level.
    let mut t = held(6, 3);
    rules(0).update_trait_level(&mut t, false);
    assert_eq!(t, held(6, 2));
    // The held level below it.
    let mut t = held(2, 2);
    rules(3).update_trait_level(&mut t, false);
    assert_eq!(t, held(2, 1));
    // Gaining.
    let mut t = held(6, 3);
    rules(2).update_trait_level(&mut t, true);
    assert_eq!(t, held(6, 2));
    // No level reached: nothing changes, not even the points.
    let mut t = held(1, 3);
    rules(2).update_trait_level(&mut t, false);
    assert_eq!(t, held(1, 3));
}

#[test]
fn an_antitrait_keeps_the_level_and_its_effects_below_the_first_threshold() {
    let mut m = model(0);
    add_trait_points(&mut m, CharacterId(100), "S", 5);
    assert_eq!((s(&m).level, land(&m)), (2, 4));
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "U", 4), TraitOutcome::Absorbed);
    assert_eq!(s(&m), held(1, 2));
    assert_eq!(land(&m), 4, "level 2's attribute effect stays");
    let fx = super::effects::Effects::compute(&m);
    assert_eq!(fx.character[&CharacterId(100)].get("b"), 3.0, "and its effects");
}

#[test]
fn an_antitrait_drop_to_the_no_going_back_level_absorbs_the_points() {
    let mut m = model(2);
    add_trait_points(&mut m, CharacterId(100), "S", 9);
    assert_eq!((s(&m).level, land(&m)), (3, 6));
    // 9 − 6 = 3 points (level 1): dropped to level 2 at 5 points; no overflow, so U is not added.
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "U", 6), TraitOutcome::Absorbed);
    assert_eq!(s(&m), held(5, 2));
    assert_eq!(land(&m), 4);
    assert!(m.world.character_details[&CharacterId(100)].traits.iter().all(|t| t.key != "U"));
}

#[test]
fn a_gain_from_a_kept_level_into_a_lower_one_moves_the_effects() {
    // ORIGINAL BUG 0x008B5380 / 0x008C2D30: the exe moves the level record down with no slot.
    let mut m = model(0);
    add_trait_points(&mut m, CharacterId(100), "S", 5);
    add_trait_points(&mut m, CharacterId(100), "U", 4);
    assert_eq!(s(&m), held(1, 2));
    assert_eq!(add_trait_points(&mut m, CharacterId(100), "S", 1), TraitOutcome::Raised { from_level: 2, to_level: 1 });
    assert_eq!(s(&m), held(2, 1));
    assert_eq!(land(&m), 3, "level 2's +2 off, level 1's +1 on");
}

#[test]
fn remove_trait_reads_the_kept_level() {
    // Held at level 2 (no-going-back level 2) with 1 point: kept, though the points reach no level.
    let mut m = model(2);
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().traits.push(held(1, 2));
    assert!(!remove_trait(&mut m, CharacterId(100), "S"));
    // At level 1: removed, with level 1's effect.
    let mut m = model(2);
    add_trait_points(&mut m, CharacterId(100), "S", 2);
    assert_eq!(land(&m), 3);
    assert!(remove_trait(&mut m, CharacterId(100), "S"));
    assert_eq!(land(&m), 2);
}

#[test]
fn a_load_sets_unset_levels_from_the_points() {
    let mut m = model(2);
    let before = land(&m);
    m.world.character_details.get_mut(&CharacterId(100)).unwrap().traits =
        vec![held(6, 0), CharacterTrait { key: "S".into(), points: -2, level: 0 }, held(1, 3), held(1, 0)];
    load_trait_levels(&mut m);
    let traits = &m.world.character_details[&CharacterId(100)].traits;
    // 0x008EF070: points floored at 0, level from them; a stored level kept; none below the first.
    assert_eq!(traits, &vec![held(6, 2), held(0, 0), held(1, 3), held(1, 0)]);
    assert_eq!(land(&m), before, "the saved attribute levels already hold the effects");
}

/// A region's traits load as a character's do (the same container class, `0x0088AFF0`).
#[test]
fn a_load_sets_region_trait_levels_too() {
    let mut m = model(2);
    m.world.regions.get_mut(&RegionId(10)).unwrap().traits = vec![held(6, 0), CharacterTrait { key: "S".into(), points: -2, level: 0 }, held(1, 3)];
    load_trait_levels(&mut m);
    assert_eq!(m.world.regions[&RegionId(10)].traits, vec![held(6, 2), held(0, 0), held(1, 3)]);
}

/// A region's effect list (region +0x1CC, `0x008EDAE0` → `0x00F84220`) sums one effect's values over its held
/// traits' current levels in first-appearance order, a trait with no level adding none; its set (+0x1DC) is part
/// of the region's effects (`0x00A67530`). MADE-UP traits `S` (level 2: `t_z` 2, `t_b` 3) and `T` (level 1:
/// `t_c` 1, `t_b` 0.5).
#[test]
fn a_regions_effect_list_sums_its_traits_level_effects_in_first_appearance_order() {
    use super::effects::{Effects, region_trait_rows, region_trait_set};
    let mut m = model(2);
    let r = Arc::make_mut(&mut m.rules);
    r.effects.insert_trait_level("S_2".into(), [("t_z", 2.0), ("t_b", 3.0)]);
    r.effects.trait_levels.insert("T".into(), vec![(1, 1, "T_1".into())]);
    r.effects.insert_trait_level("T_1".into(), [("t_c", 1.0), ("t_b", 0.5)]);
    let region = RegionId(10);
    let before = (Effects::compute(&m).region(region, "b"), economy::region_effect_set(&m, &m.world.regions[&region]).get("b"));
    m.world.regions.get_mut(&region).unwrap().traits = vec![held(6, 2), held(1, 0), CharacterTrait { key: "T".into(), points: 1, level: 1 }];
    let reg = &m.world.regions[&region];
    let rows = region_trait_rows(&m.rules.effects, reg);
    assert_eq!(rows, [("t_z".to_string(), 2.0), ("t_b".to_string(), 3.5), ("t_c".to_string(), 1.0)]);
    assert_eq!(region_trait_set(&m.rules.effects, reg).get("b"), 3.5, "only the mapped effect has a bonus");
    assert_eq!(Effects::compute(&m).region(region, "b"), before.0 + 3.5);
    assert_eq!(Effects::compute_for(&m, reg.owner).region(region, "b"), before.0 + 3.5);
    assert_eq!(economy::region_effect_set(&m, reg).get("b"), before.1 + 3.5);
    assert_eq!(Effects::compute(&m).region_local(region, "b"), 0.0, "the buildings' part alone");
}

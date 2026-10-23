//! The battle sides' result codes (`0x0054AE90`) and the class war step of the post-battle sequence
//! (`0x008ACCF0`, called from `0x008F7880` at `0x008F7B7C`).

use super::population_class_tests::{ranked_model, war};
use super::*;
use crate::campaign::{battle_grade, battle_side_results, BattleResultCode as Code};

/// Every branch of the grading `0x0054AE90`: the ratio thresholds 0.1 / 0.3 / 1.1 and the sizes (1000, 3000)
/// with land units, (400, 1500) without.
#[test]
fn the_grade_follows_the_ratio_and_the_battle_size() {
    let g = |p: (f32, f32), l: (u32, u32), land| battle_grade(p, l, land);
    // Ratio below 0.1 (here 0: the winner lost nobody).
    assert_eq!(g((10.0, 1.0), (0, 999), true), 2);
    assert_eq!(g((10.0, 1.0), (0, 1000), true), 5);
    assert_eq!(g((10.0, 1.0), (0, 399), false), 2);
    assert_eq!(g((10.0, 1.0), (0, 400), false), 5);
    // Below 0.3.
    assert_eq!(g((1.0, 1.0), (20, 100), true), 1);
    assert_eq!(g((1.0, 1.0), (200, 1000), true), 4);
    // Below 1.1; exactly 0.1 and 0.3 are not below them.
    assert_eq!(g((1.0, 1.0), (10, 100), true), 1);
    assert_eq!(g((1.0, 1.0), (30, 100), true), 0);
    assert_eq!(g((1.0, 1.0), (50, 100), true), 0);
    assert_eq!(g((1.0, 1.0), (500, 1000), true), 3);
    // 1.1 and above: 6 only past the large size.
    assert_eq!(g((1.0, 1.0), (120, 100), true), 0);
    assert_eq!(g((1.0, 1.0), (1200, 1000), true), 3);
    assert_eq!(g((1.0, 1.0), (1600, 1400), true), 3, "3000 is not past 3000");
    assert_eq!(g((1.0, 1.0), (1700, 1400), true), 6);
    assert_eq!(g((1.0, 1.0), (900, 700), false), 6);
    // The strength term: a loser without potential makes the ratio 0, and so does a loser without losses.
    assert_eq!(g((5.0, 0.0), (500, 100), true), 2);
    assert_eq!(g((5.0, 1.0), (500, 0), true), 2);
    // Both sides' zero losses (an unresolved battle) grade 2.
    assert_eq!(g((0.0, 0.0), (0, 0), true), 2);
}

/// The jump tables `0x0054B088` / `0x0054B0A4` and the codes the class war step switches on.
#[test]
fn grades_map_to_the_result_codes() {
    let win: Vec<u8> = (0..=6).map(|g| Code::from_grade(g, true).code()).collect();
    let lose: Vec<u8> = (0..=6).map(|g| Code::from_grade(g, false).code()).collect();
    assert_eq!(win, [8, 9, 10, 8, 9, 10, 11]);
    assert_eq!(lose, [13, 14, 15, 13, 14, 15, 16]);
}

/// `0x007B49A0` orients both records' input to the winner; each record grades with its own land flag.
#[test]
fn side_results_are_graded_from_the_winner() {
    // B won: the winner's potential is 1 against 10 and it lost 10 men against 500, so the ratio is tiny.
    assert_eq!(battle_side_results(false, (10.0, 1.0), (500, 10), (true, true)), (Code::CrushingDefeat, Code::HeroicVictory));
    // A large even battle: the side with land units grades 3, the one without 6.
    assert_eq!(battle_side_results(true, (2.0, 1.0), (1000, 1000), (true, false)), (Code::CloseVictory, Code::ValiantDefeat));
    assert_eq!(battle_side_results(false, (1.0, 2.0), (1000, 1000), (false, true)), (Code::ValiantDefeat, Code::CloseVictory));
    assert_eq!(battle_side_results(true, (2.0, 1.0), (1000, 1000), (false, false)), (Code::PyrrhicVictory, Code::ValiantDefeat));
}

fn capitals() -> CampaignModel {
    let mut m = ranked_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    m.world.faction_details.entry(B).or_default().capital = Some(RegionId(11));
    m
}

fn field_battle() -> PendingBattle {
    PendingBattle { attacker: ForceId(1000), defenders: vec![ForceId(1001)], settlement: None, resume: None }
}

fn result(attacker_won: bool, codes: (Code, Code)) -> BattleResult {
    BattleResult { attacker_won, attacker_casualties: 0.0, defender_casualties: 0.0, codes: Some(codes) }
}

fn both(w: i32) -> Vec<(String, i32)> {
    vec![("lower".to_owned(), w), ("upper".to_owned(), w)]
}

/// Outside a walled settlement: the winner's capital +1 (close, decisive) or +2 (heroic, pyrrhic), the loser's
/// −1 (close, decisive) or −2 (crushing, valiant), every class alike.
#[test]
fn a_field_battle_moves_both_capitals_by_the_codes() {
    for (codes, win, lose) in [
        ((Code::CloseVictory, Code::CloseDefeat), 1, -1),
        ((Code::DecisiveVictory, Code::DecisiveDefeat), 1, -1),
        ((Code::HeroicVictory, Code::CrushingDefeat), 2, -2),
        ((Code::PyrrhicVictory, Code::ValiantDefeat), 2, -2),
        ((Code::HeroicVictory, Code::CloseDefeat), 2, -1),
    ] {
        let mut m = capitals();
        m.battle_class_war(&field_battle(), &result(true, codes), (Some(A), Some(B)));
        assert_eq!(war(&m, 10), both(win), "{codes:?}");
        assert_eq!(war(&m, 11), both(lose), "{codes:?}");
        assert!(war(&m, 12).is_empty(), "not a capital");
    }
    // The defender won: the codes are read from its side.
    let mut m = capitals();
    m.battle_class_war(&field_battle(), &result(false, (Code::ValiantDefeat, Code::DecisiveVictory)), (Some(A), Some(B)));
    assert_eq!(war(&m, 11), both(1));
    assert_eq!(war(&m, 10), both(-2));
}

/// An assault on a walled settlement (types 6 and 7): the winner's capital +4, the loser's −4, whatever the codes.
/// ORIGINAL BUG (`0x008ACF02`): the exe moves the second side's capital by −4, so a defender who won took +4 and
/// −4 and the attacker's capital nothing; ours moves the loser's.
#[test]
fn a_walled_settlement_assault_moves_the_capitals_by_four() {
    let walled = |m: &mut CampaignModel| {
        m.world.regions.get_mut(&RegionId(12)).unwrap().fortification = Some(BuildingRef { level_key: "test_walls".into(), health: 100 });
    };
    let assault = PendingBattle { settlement: Some(RegionId(12)), ..field_battle() };
    let mut m = capitals();
    walled(&mut m);
    m.battle_class_war(&assault, &result(true, (Code::CloseVictory, Code::CloseDefeat)), (Some(A), Some(B)));
    assert_eq!(war(&m, 10), both(4));
    assert_eq!(war(&m, 11), both(-4));
    let mut m = capitals();
    walled(&mut m);
    m.battle_class_war(&assault, &result(false, (Code::CrushingDefeat, Code::HeroicVictory)), (Some(A), Some(B)));
    assert_eq!(war(&m, 11), both(4), "the defender won");
    assert_eq!(war(&m, 10), both(-4), "the attacker lost");
    // A settlement without walls is fought as a field battle (type 9).
    let mut m = capitals();
    m.battle_class_war(&assault, &result(true, (Code::CloseVictory, Code::CrushingDefeat)), (Some(A), Some(B)));
    assert_eq!(war(&m, 10), both(1));
    assert_eq!(war(&m, 11), both(-2));
}

/// Nothing moves without codes or a side's faction; a side without a capital is skipped; a battle with the rebels
/// moves nobody.
#[test]
fn the_step_skips_what_it_cannot_apply() {
    let codes = (Code::CloseVictory, Code::CloseDefeat);
    let mut m = capitals();
    let none = BattleResult { codes: None, ..result(true, codes) };
    m.battle_class_war(&field_battle(), &none, (Some(A), Some(B)));
    m.battle_class_war(&field_battle(), &result(true, codes), (Some(A), None));
    assert!(war(&m, 10).is_empty() && war(&m, 11).is_empty());
    // B has no capital: A's still moves.
    let mut m = ranked_model();
    m.world.faction_details.entry(A).or_default().capital = Some(RegionId(10));
    m.battle_class_war(&field_battle(), &result(true, codes), (Some(A), Some(B)));
    assert_eq!(war(&m, 10), both(1));
    assert!(war(&m, 11).is_empty());
    // A battle against the rebels (C) moves neither capital: the rule leaves a rebel owner or other alone.
    let mut m = capitals();
    m.world.regions.get_mut(&RegionId(12)).unwrap().owner = C;
    m.world.faction_details.entry(C).or_default().capital = Some(RegionId(12));
    m.world.factions.get_mut(&C).unwrap().key.clear();
    m.battle_class_war(&field_battle(), &result(true, codes), (Some(A), Some(C)));
    assert_eq!(war(&m, 10), both(0));
    assert!(war(&m, 12).iter().all(|(_, w)| *w == 0));
}

/// End to end: the land autoresolve grades both sides from the resolver's sums and the post-battle step moves
/// both capitals by them.
#[test]
fn an_autoresolved_battle_moves_both_capitals() {
    let mut m = capitals();
    m.world.characters.get_mut(&CharacterId(101)).unwrap().position = pos(5, 0);
    m.apply(CampaignCommand::DeclareWar { a: A, b: B }).unwrap();
    m.apply(CampaignCommand::AttackForce { force: ForceId(1000), target: ForceId(1001) }).unwrap();
    // The declaration moved A's capital; a capital without classes yet gets them at 0.
    let base = |v: Vec<(String, i32)>| if v.is_empty() { both(0) } else { v };
    let war_before = (base(war(&m, 10)), base(war(&m, 11)));
    m.apply(CampaignCommand::Autoresolve).unwrap();
    let o = m.last_autoresolve.clone().unwrap();
    let losses = (o.losses_a.iter().sum::<u32>(), o.losses_b.iter().sum::<u32>());
    let (code_a, code_b) = battle_side_results(o.a_won, o.potentials, losses, (true, true));
    let step = |c: Code| match c {
        Code::CloseVictory | Code::DecisiveVictory => 1,
        Code::HeroicVictory | Code::PyrrhicVictory => 2,
        Code::CloseDefeat | Code::DecisiveDefeat => -1,
        Code::CrushingDefeat | Code::ValiantDefeat => -2,
    };
    let moved = |before: &[(String, i32)], c: Code| before.iter().map(|(k, w)| (k.clone(), (w + step(c)).clamp(-15, 6))).collect::<Vec<_>>();
    assert!(!o.a_won, "80 men against 160 of the same quality");
    assert_eq!(war(&m, 10), moved(&war_before.0, code_a));
    assert_eq!(war(&m, 11), moved(&war_before.1, code_b));
    assert!(!war(&m, 10).is_empty());
}

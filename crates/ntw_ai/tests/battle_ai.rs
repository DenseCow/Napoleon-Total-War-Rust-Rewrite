//! Headless battle-AI tests on MADE-UP data (no install needed), plus one test on the real
//! install's units that skips itself when the game is not installed.

use ntw_ai::battle::{AttackState, BattleAi, DefendState, Efficiency, LineTactic, ShootState, UnitTask, run_headless};
use ntw_ai::battle::melee_manager::ObjectiveKind;
use ntw_sim::battle::fatigue::KvFatigue;
use ntw_sim::battle::model::{Battle, BattleResult, LandUnit};
use ntw_sim::battle::morale::KvMorale;
use ntw_sim::battle::rules::KvRules;
use ntw_sim::battle::shooting::MissileWeapon;

/// OBVIOUSLY MADE-UP morale data (not game data), like the model's own tests.
fn kv_morale() -> KvMorale {
    KvMorale {
        morale_base: 40,
        ums_impetuous_threshold_lower: 90,
        ums_eager_threshold_upper: 95,
        ums_eager_threshold_lower: 70,
        ums_confident_threshold_upper: 75,
        ums_confident_threshold_lower: 50,
        ums_steady_threshold_upper: 55,
        ums_steady_threshold_lower: 30,
        ums_shaken_threshold_upper: 35,
        ums_shaken_threshold_lower: 10,
        ums_wavering_threshold_upper: 15,
        ums_wavering_threshold_lower: -10,
        ums_broken_threshold_upper: -5,
        ums_broken_threshold_lower: -30,
        waver_base_timeout: 20,
        broken_finish_base_timeout: 50,
        was_attacked_in_front: -2,
        was_attacked_in_flank: -10,
        was_attacked_in_rear: -20,
        recent_casualties_shock_threshold: 100,
        total_casualties_penalty_20: -10,
        total_casualties_penalty_40: -25,
        total_casualties_penalty_60: -45,
        total_casualties_penalty_80: -60,
        total_casualties_penalty_90: -80,
        ..KvMorale::default()
    }
}

/// OBVIOUSLY MADE-UP fatigue data.
fn kv_fatigue() -> KvFatigue {
    KvFatigue {
        idle: -1,
        walking: 1,
        running: 3,
        combat: 4,
        threshold_fresh: 0,
        threshold_active: 100,
        threshold_winded: 200,
        threshold_tired: 300,
        threshold_very_tired: 400,
        threshold_exhausted: 500,
        threshold_max: 600,
        ..KvFatigue::default()
    }
}

/// OBVIOUSLY MADE-UP rules data.
fn kv_rules() -> KvRules {
    KvRules {
        armour_melee_piercing_divisor: 2,
        defense_melee_piercing_divisor: 2,
        melee_height_delta_min: -1.0,
        melee_height_delta_max: 1.0,
        relative_melee_height_delta_divisor: 1.0,
        factor_attackdir_flankleft: 3,
        factor_attackdir_flankright: 3,
        factor_attackdir_rear: 6,
        melee_hn_to_xholds_0_max: -10,
        melee_hn_to_xholds_1_max: 0,
        melee_hn_to_xholds_2_max: 10,
        melee_hn_to_xholds_3_max: 20,
        melee_xholds_knockdown_2: 50,
        melee_xholds_knockback_2: 100,
        melee_xholds_stepback_2: 150,
        missile_distance_for_half_chance_hit: 40,
        ..KvRules::default()
    }
}

/// A made-up musket.
fn musket() -> MissileWeapon {
    MissileWeapon {
        range: 80,
        accuracy: 30.0,
        reload_skill: 50,
        reload_time_s: 2,
        damage: 1.0,
        projectiles_per_shot: 1,
        is_artillery: false,
        guns: 0,
        ballistics: Default::default(),
    }
}

fn infantry(id: u32, side: u8, pos: (f32, f32), facing: f32) -> LandUnit {
    let mut u = LandUnit::new(id, side, 120, pos);
    u.facing = facing;
    u.melee_attack = 6;
    u.melee_defence = 6;
    u.armour = 3;
    u.charge_bonus = 6;
    u.walk_speed = 1.5;
    u.run_speed = 4.0;
    u.missile = Some(musket());
    u.ammunition = 30;
    u
}

fn cavalry(id: u32, side: u8, pos: (f32, f32), facing: f32) -> LandUnit {
    let mut u = LandUnit::new(id, side, 60, pos);
    u.facing = facing;
    u.is_cavalry = true;
    u.melee_attack = 10;
    u.melee_defence = 4;
    u.armour = 4;
    u.charge_bonus = 20;
    u.walk_speed = 3.0;
    u.run_speed = 9.0;
    u
}

/// Side 0 at y = -150 facing +y, side 1 at y = +150 facing -y: three musket units and one
/// cavalry each (ids 1..4 and 11..14).
fn two_armies(seed: u32) -> Battle {
    let mut b = Battle::with_rules(seed, kv_morale(), kv_fatigue(), kv_rules());
    let up = std::f32::consts::FRAC_PI_2;
    for (k, x) in [-70.0f32, 0.0, 70.0].into_iter().enumerate() {
        b.add_unit(infantry(1 + k as u32, 0, (x, -150.0), up));
        b.add_unit(infantry(11 + k as u32, 1, (x, 150.0), -up));
    }
    b.add_unit(cavalry(4, 0, (140.0, -150.0), up));
    b.add_unit(cavalry(14, 1, (140.0, 150.0), -up));
    b
}

/// Makes side 0 completely passive: holds its ground and never fires.
fn make_passive(b: &mut Battle, side: u8) {
    let ids: Vec<u32> = b.units.iter().filter(|u| u.side == side).map(|u| u.id).collect();
    for id in ids {
        b.order_halt(id);
        b.order_fire_at_will(id, false);
    }
}

#[test]
fn ai_beats_a_passive_army() {
    let mut b = two_armies(1805);
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    let ticks = run_headless(&mut b, &mut ai, 60_000);
    let fired: u32 = b.units.iter().filter(|u| u.side == 1).map(|u| u.volleys_fired).sum();
    eprintln!("decided after {ticks} ticks, AI volleys {fired}, {}", ai.alliances[0].status());
    assert_eq!(b.battle_result(), BattleResult::Won { side: 1 });
    assert!(fired > 0, "the AI's muskets fired");
}

#[test]
fn same_seed_same_battle() {
    let run = |seed| {
        let mut b = two_armies(seed);
        let mut ai = BattleAi::new(&[0, 1]);
        let mut hashes = Vec::new();
        for _ in 0..3000 {
            ai.update(&mut b);
            b.step();
            if b.tick.is_multiple_of(500) {
                hashes.push(b.state_hash());
            }
        }
        (b.state_hash(), hashes, ai)
    };
    let (h1, s1, a1) = run(42);
    let (h2, s2, a2) = run(42);
    assert_eq!(h1, h2);
    assert_eq!(s1, s2);
    assert_eq!(a1, a2, "the AI's own state is deterministic too");
    let (h3, _, _) = run(43);
    assert_ne!(h1, h3, "a different seed gives a different battle");
}

#[test]
fn deploys_then_advances_and_stops_to_shoot() {
    let mut b = two_armies(7);
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    // Deployment: the first think forms the line.
    ai.update(&mut b);
    assert_eq!(ai.alliances[0].line, LineTactic::Attack(AttackState::Reform));
    let mut saw_move_to_target = false;
    let mut saw_stop_and_shoot = false;
    for _ in 0..3000 {
        ai.update(&mut b);
        b.step();
        let a = &ai.alliances[0];
        saw_move_to_target |= a.line == LineTactic::Attack(AttackState::MoveToTarget);
        saw_stop_and_shoot |= a.tasks.values().any(|t| {
            matches!(t, UnitTask::StopAndShoot)
                || matches!(t, UnitTask::Objective(c) if c.kind == ObjectiveKind::Missile)
        });
    }
    assert!(saw_move_to_target, "{}", ai.alliances[0].status());
    assert!(saw_stop_and_shoot);
    // The AI muskets halted within range of the enemy and fired.
    let fired: u32 = b.units.iter().filter(|u| u.side == 1).map(|u| u.volleys_fired).sum();
    assert!(fired > 0);
}

#[test]
fn cavalry_charges_an_engaged_enemy_in_the_flank() {
    // Our (side 1) infantry is locked in melee with an enemy infantry unit; our cavalry stands to
    // the enemy's side. The melee manager should send the cavalry at that enemy.
    let mut b = Battle::with_rules(3, kv_morale(), kv_fatigue(), kv_rules());
    let up = std::f32::consts::FRAC_PI_2;
    b.add_unit(infantry(1, 0, (0.0, 0.0), up)); // enemy, facing +y
    b.add_unit(infantry(11, 1, (0.0, 6.0), -up)); // ours, in contact in front of it
    b.add_unit(cavalry(14, 1, (60.0, 0.0), std::f32::consts::PI)); // ours, on its right flank
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    ai.update(&mut b);
    let a = &ai.alliances[0];
    let c = a.assignments.get(&14).expect("cavalry got an objective");
    assert_eq!(c.kind, ObjectiveKind::Melee);
    assert_eq!(c.target, Some(1));
    // Within 60 m the order is a charge.
    assert!(b.units[b.unit_index(14).unwrap()].destination.is_some());
}

#[test]
fn ai_against_ai_ends() {
    let mut b = two_armies(99);
    let mut ai = BattleAi::new(&[0, 1]);
    let ticks = run_headless(&mut b, &mut ai, 60_000);
    eprintln!("AI v AI: {:?} after {ticks} ticks", b.battle_result());
    eprintln!("{}\n{}", ai.alliances[0].status(), ai.alliances[1].status());
    assert_ne!(b.battle_result(), BattleResult::Ongoing);
}

#[test]
#[ignore]
fn trace_ai_v_ai() {
    let mut b = two_armies(99);
    let passive = std::env::var("TRACE_PASSIVE").is_ok();
    if passive {
        make_passive(&mut b, 0);
    }
    let mut ai = if passive { BattleAi::new(&[1]) } else { BattleAi::new(&[0, 1]) };
    for t in 0..20_000u32 {
        ai.update(&mut b);
        b.step();
        if t % 1000 == 0 {
            eprintln!("t={t} {:?}", ai.alliances.iter().map(|a| a.status()).collect::<Vec<_>>());
            for u in &b.units {
                eprintln!(
                    "  u{} s{} men {} pos ({:.0},{:.0}) dest {:?} fire {:?} melee {} morale {:?} ammo {} task {:?}",
                    u.id, u.side, u.men, u.position.0, u.position.1, u.destination.map(|d| (d.0 as i32, d.1 as i32)),
                    u.fire_target, u.in_melee, u.morale.state, u.ammunition,
                    ai.alliance(u.side).and_then(|a| a.tasks.get(&u.id))
                );
            }
        }
    }
}

/// The ATTACK_BATTLEGROUP outflank (`0x007A5C80`): a weaker attacker, not engaged, takes the
/// decision once and marches to a point turned off the line to the enemy; a stronger one does not.
#[test]
fn weaker_attacker_outflanks_once_stronger_does_not() {
    let up = std::f32::consts::FRAC_PI_2;
    let run = |n_enemy: u32| {
        let mut b = Battle::with_rules(3, kv_morale(), kv_fatigue(), kv_rules());
        for k in 0..2u32 {
            b.add_unit(infantry(1 + k, 1, (-35.0 + 70.0 * k as f32, 400.0), -up));
        }
        for k in 0..n_enemy {
            b.add_unit(infantry(11 + k, 0, (-70.0 * (n_enemy as f32 - 1.0) / 2.0 + 70.0 * k as f32, -150.0), up));
        }
        make_passive(&mut b, 0);
        let mut ai = BattleAi::new(&[1]);
        let mut states = Vec::new();
        for _ in 0..4000 {
            ai.update(&mut b);
            b.step();
            let l = ai.alliances[0].line;
            if states.last() != Some(&l) {
                states.push(l);
            }
        }
        (states, ai.alliances[0].outflank_point, ai.alliances[0].outflank_checked)
    };
    let (weak, point, checked) = run(4);
    eprintln!("weaker: {weak:?} point {point:?}");
    assert!(checked, "the decision was taken");
    assert!(weak.contains(&LineTactic::Attack(AttackState::Outflank)), "outflanks: {weak:?}");
    let p = point.expect("an outflank point");
    assert!(p.0.abs() > 20.0, "the point is off to one side: {p:?}");
    assert_eq!(weak.iter().filter(|s| **s == LineTactic::Attack(AttackState::Outflank)).count(), 1, "only once");
    let (strong, _, _) = run(1);
    eprintln!("stronger: {strong:?}");
    assert!(!strong.contains(&LineTactic::Attack(AttackState::Outflank)));
}

/// The STOP_AND_SHOOT efficiency rules (`0x007D9190` sampler, `0x00796DF0` test), CONFIRMED.
#[test]
fn stop_and_shoot_efficiency_rules() {
    let mut e = Efficiency::default();
    e.reset(5, 1000);
    // Same tick on both samples: efficient.
    assert!(e.efficient(1000, 200, 0.01));
    // No rise for 200 ticks is still fine, 201 is stale.
    assert!(e.efficient(1200, 200, 0.01));
    assert!(!e.efficient(1201, 200, 0.01));
    // A rise 50 ticks later: current moves, the baseline (50 ticks old) stays.
    e.sample(6, 1050, 100);
    assert_eq!(e.current, (6, 1050));
    assert_eq!(e.previous, (5, 1000));
    assert!(e.efficient(1060, 200, 0.01)); // 1 kill / 50 ticks = 0.02
    // No rise: nothing changes.
    e.sample(6, 1100, 100);
    assert_eq!(e.current, (6, 1050));
    // A rise when the baseline is more than 100 ticks old: both move up.
    e.sample(7, 1101, 100);
    assert_eq!(e.previous, (7, 1101));
    assert_eq!(e.current, (7, 1101));
}

/// STOP_AND_SHOOT runs ADVANCING_TOWARDS_LINE → FORM_ON_LINE → HOLD_THE_LINE, and a hold that
/// kills nobody for more than 200 ticks creeps forward (a weaker line) to put its shortest range
/// 3 m inside the target.
#[test]
fn stop_and_shoot_holds_then_creeps_forward() {
    let up = std::f32::consts::FRAC_PI_2;
    let mut b = Battle::with_rules(5, kv_morale(), kv_fatigue(), kv_rules());
    // Ours (side 1): two musket units that cannot hit anything (accuracy 0).
    for k in 0..2u32 {
        let mut u = infantry(1 + k, 1, (-35.0 + 70.0 * k as f32, 300.0), -up);
        let mut w = musket();
        w.accuracy = 0.0;
        w.damage = 0.0;
        u.missile = Some(w);
        b.add_unit(u);
    }
    // The enemy: three passive units at y = 0.
    for k in 0..3u32 {
        // Big units (weaker AI line, so a failed hold creeps rather than assaults).
        // No muskets: our mean range beats theirs, so the firing line is at our longest range.
        let mut e = infantry(11 + k, 0, (-70.0 + 70.0 * k as f32, 0.0), up);
        e.men = 400;
        e.max_men = 400;
        e.missile = None;
        b.add_unit(e);
    }
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    ai.update(&mut b);
    // The planner would defend (weaker); put the line into STOP_AND_SHOOT the way MOVE_TO_TARGET
    // hands it over.
    ai.alliances[0].line = LineTactic::Attack(AttackState::MoveToTarget);
    let mut states = Vec::new();
    let mut hold_anchor = None;
    let mut creep_anchor = None;
    for _ in 0..3000 {
        ai.update(&mut b);
        b.step();
        let a = &ai.alliances[0];
        if states.last() != Some(&a.line) {
            states.push(a.line);
            eprintln!("tick {} {:?}", b.tick, a.line);
            match a.line {
                LineTactic::StopAndShoot(ShootState::HoldTheLine) if hold_anchor.is_none() => hold_anchor = Some(a.anchor),
                LineTactic::StopAndShoot(ShootState::CreepForward) if creep_anchor.is_none() => creep_anchor = Some(a.anchor),
                _ => {}
            }
        }
    }
    eprintln!("{states:?}");
    let pos = |s| states.iter().position(|x| *x == LineTactic::StopAndShoot(s));
    let (adv, form, hold, creep) = (
        pos(ShootState::AdvancingTowardsLine).expect("advancing"),
        pos(ShootState::FormOnLine).expect("form on line"),
        pos(ShootState::HoldTheLine).expect("hold"),
        pos(ShootState::CreepForward).expect("creep"),
    );
    assert!(adv < form && form < hold && hold < creep, "{states:?}");
    // The creep line is closer to the enemy (y = 0) than the hold line.
    let (h, c) = (hold_anchor.unwrap(), creep_anchor.unwrap());
    assert!(c.1 < h.1, "creeps forward: hold {h:?} creep {c:?}");
}

/// DEFEND_ABSTRACT: REFORM → DEFEND_LINE once fully formed; a unit leaving the line sends it back
/// to REFORM (`0x007B5C00` / `0x0076D870`, CONFIRMED).
#[test]
fn defend_line_reforms_when_a_unit_leaves() {
    let up = std::f32::consts::FRAC_PI_2;
    let mut b = Battle::with_rules(9, kv_morale(), kv_fatigue(), kv_rules());
    for k in 0..3u32 {
        b.add_unit(infantry(1 + k, 1, (-70.0 + 70.0 * k as f32, 600.0), -up));
    }
    for k in 0..6u32 {
        b.add_unit(infantry(11 + k, 0, (-175.0 + 70.0 * k as f32, 0.0), up));
    }
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    // A defender (alliance `+0x70`) facing twice its strength defends (plan 2 → 4, mode 3).
    ai.set_defender(1, true);
    for _ in 0..1000 {
        ai.update(&mut b);
        b.step();
    }
    assert_eq!(ai.alliances[0].line, LineTactic::Defend(DefendState::DefendLine), "{}", ai.alliances[0].status());
    // Unit 2 is wiped out.
    let i = b.unit_index(2).unwrap();
    b.units[i].men = 0;
    for _ in 0..20 {
        ai.update(&mut b);
        b.step();
    }
    let a = &ai.alliances[0];
    assert!(
        matches!(a.line, LineTactic::Defend(DefendState::Reform) | LineTactic::Defend(DefendState::DefendLine)),
        "{}",
        a.status()
    );
    assert!(!a.line_members.contains(&2), "the reform took the new group: {:?}", a.line_members);
}

/// The planner's STOP_AND_SHOOT score and keep tests (`0x007B1BD0`, `0x0075ECA0`): a line that is
/// outgunned more than 2:1 in missile strength never takes up STOP_AND_SHOOT, it goes in with the
/// bayonet instead.
#[test]
fn outgunned_line_does_not_stop_to_shoot() {
    let up = std::f32::consts::FRAC_PI_2;
    let mut b = Battle::with_rules(5, kv_morale(), kv_fatigue(), kv_rules());
    let mut u = infantry(1, 1, (0.0, 300.0), -up);
    u.missile = Some(musket());
    b.add_unit(u);
    for k in 0..3u32 {
        let mut e = infantry(11 + k, 0, (-70.0 + 70.0 * k as f32, 0.0), up);
        e.missile = Some(musket());
        b.add_unit(e);
    }
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    ai.update(&mut b);
    ai.alliances[0].line = LineTactic::Attack(AttackState::MoveToTarget);
    for _ in 0..1500 {
        ai.update(&mut b);
        b.step();
        assert!(!matches!(ai.alliances[0].line, LineTactic::StopAndShoot(_)), "outgunned line stopped to shoot");
    }
}

/// The battlegroup's tactic auction (`0x00751800`): with five cavalry beside a strong line, a
/// wing tactic (OUTFLANK or DOUBLE_ENVELOPMENT) wins units once the battle closes, the cavalry it
/// claims leaves the ATTACK_BATTLEGROUP group, and STOP_AND_SHOOT never runs while a wing tactic
/// does (the exclusion pairs at `0x01453450`).
#[test]
fn auction_gives_the_wings_their_units_and_excludes_stop_and_shoot() {
    use ntw_ai::battle::auction::Kind;
    let up = std::f32::consts::FRAC_PI_2;
    let mut b = Battle::with_rules(21, kv_morale(), kv_fatigue(), kv_rules());
    for k in 0..4u32 {
        b.add_unit(infantry(1 + k, 1, (-105.0 + 70.0 * k as f32, 400.0), -up));
    }
    for k in 0..5u32 {
        b.add_unit(cavalry(5 + k, 1, (180.0 + 40.0 * k as f32, 400.0), -up));
    }
    for k in 0..5u32 {
        b.add_unit(infantry(11 + k, 0, (-140.0 + 70.0 * k as f32, 0.0), up));
    }
    make_passive(&mut b, 0);
    let mut ai = BattleAi::new(&[1]);
    let mut claimed = false;
    for _ in 0..3000 {
        ai.update(&mut b);
        b.step();
        let a = &ai.alliances[0];
        let wing_active = a.tactics.iter().any(|t| t.active && matches!(t.kind, Kind::Outflank | Kind::DoubleEnvelopment));
        assert!(!(wing_active && matches!(a.line, LineTactic::StopAndShoot(_))), "{}", a.status());
        if wing_active {
            claimed = true;
            let attack = a.tactics.iter().find(|t| t.kind == Kind::AttackBattlegroup).unwrap();
            for id in a.wing_side.keys() {
                assert!(!attack.units.contains(id), "unit {id} is on a wing and in the line group");
            }
        }
    }
    assert!(claimed, "a wing tactic won units: {}", ai.alliances[0].status());
}

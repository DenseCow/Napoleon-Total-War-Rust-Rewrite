//! The **high-level plan** of an alliance (`analysis/ai/AI_RESEARCH.md` §3.1c), CONFIRMED.
//!
//! Every 10th alliance-AI update (and every update while deploying) `0x007D7020` runs the plan
//! vote `0x007B1FB0` and maps the winning plan to the alliance **mode** (`+0x61C`). The mode picks
//! the high-level objectives: attack every enemy battlegroup (mode 2), defend a terrain feature
//! or a line (mode 3), look for an enemy that is not seen (mode 8), withdraw (mode 9).
//!
//! The vote reads two strength blocks per side, one taken at the first update ("initial") and
//! one refreshed every update ("current"), from the alliance's two unit trackers (`+0x330` ours,
//! `+0x3E8` the enemy's; `0x007CE410`): men, Σ melee ratings, Σ missile ratings, (a stub 0),
//! Σ infantry strength, Σ mounted strength, total strength (`0x007CCD60`). All integers.

/// One side's strength block (`0x007CE410`, CONFIRMED fields; ratings rounded to integers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Block {
    /// Field 0: men of every unit in the tracker (`+0x90`).
    pub men: i32,
    /// Field 1: Σ melee ratings (`0x0079C1E0`, INFERRED: `+0xBE8`).
    pub melee: i32,
    /// Field 2: Σ missile ratings (`0x0079C940`, `+0xBEC`, halved for units with `+0x278D`).
    pub missile: i32,
    /// Field 4: Σ (melee + missile) of the infantry (category 2, `0x00794B40`).
    pub infantry: i32,
    /// Field 5: Σ (melee + missile) of the mounted units (`0x007A4EB0`).
    pub mounted: i32,
    /// Field 6: total strength (`0x007CCD60`: Σ melee + missile, halved for `+0x278D`, plus ships).
    pub total: i32,
}

/// Facts about the alliance the vote reads besides the strength blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// Our battle alliance's `+0x70` (INFERRED: the battle's **defender**; the deployment step
    /// `0x0076A580` places units in ambush/cover only when it is set).
    pub defender: bool,
    /// `0x0055AE90` (enemy) / `0x00796710` (ours): the alliance's object list `+0x4C` holds an
    /// entry for it (`0x00557240`; INFERRED: a settlement-capture victory condition, i.e. the
    /// alliance assaults a settlement).
    pub enemy_capture: bool,
    /// See [`Flags::enemy_capture`]; ours.
    pub our_capture: bool,
    /// `alliance+0x64 != 0` (UNKNOWN meaning).
    pub pending: bool,
    /// More than one army in our alliance and the second army's `+0x220` set (UNKNOWN meaning).
    pub second_army_flag: bool,
    /// `0x00796B80`: the enemy's emplaced artillery (pieces passing `0x0053EE30`, INFERRED in a
    /// building or fixed) brings more than **1.5 ×** the missile power of all our artillery
    /// pieces (`0x006B05B0` sums).
    pub enemy_artillery_wins: bool,
    /// `0x007536C0` (see [`composition_defends`]).
    pub composition_defends: bool,
    /// The withdraw test's own flags (`0x0076A230`): deployment over (`+0x648`), every army's
    /// `+0xD4→+0xCC` set (UNKNOWN), `+0x2D4 == 0` (UNKNOWN). All three must hold.
    pub withdraw_allowed: bool,
    /// The first army's `+0xD4→+0xB8 == 0.0` (UNKNOWN meaning; turns plan 7 into 4 for a
    /// defender).
    pub first_army_b8_zero: bool,
}

/// The alliance's plan state (`+0x4A0` object: `+4` plan, `+8` loss counter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanState {
    /// The winning plan of the previous vote (0..11; it gets +3 votes next time unless 0).
    pub plan: u32,
    /// Updates on which our side had lost men (`+8`; nothing reads it here).
    pub loss_updates: u32,
}

/// Alliance modes (`+0x61C`) the plans map to (`0x007D7020`, CONFIRMED values; names are ours,
/// from what each mode's handler does in `0x007C8BD0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Mode {
    /// 0: before the first plan.
    #[default]
    None,
    /// 1: attack one chosen battlegroup (`+0x630`) plus every enemy battlegroup.
    AttackTarget,
    /// 2: attack every enemy battlegroup (`0x007C8D80`: one ATTACK objective per enemy
    /// battlegroup, priority 70 + its strength).
    Attack,
    /// 3: defend (`0x007C8E20`: a defensive terrain feature within 250 m, else a defence line).
    Defend,
    /// 8: no enemy strength seen: move to the map centre, after 60 s to the enemy's last centre
    /// (radius 400 m), after 240 s radius 500 m (`0x007C8E40`).
    Search,
    /// 9: withdraw (`0x007CA2B0`).
    Withdraw,
    /// 10: (plan 9, INFERRED settlement assault; handler not decoded).
    Assault,
    /// 11 (plan 5; unreachable: its trigger `0x0075BC80` is always false).
    Mode11,
    /// 13 (plan 11; unreachable: its trigger `0x00462CB0` returns 0).
    Mode13,
}

/// Integer percentage lost: `100 − now × 100 / initial` (0 when `initial` is 0).
fn loss_percent(initial: i32, now: i32) -> i32 {
    if initial == 0 { 0 } else { 100 - now.wrapping_mul(100) / initial }
}

/// `0x0076A230`: a defender withdraws (plan 1) when the withdraw flags hold, the enemy has no
/// capture goal, there is no time limit (always), the vote chose plan 2, our total strength is
/// at most half its initial value and the AI has run more than 1200 updates (120 s).
pub fn withdraws(plan: u32, flags: &Flags, ours_initial: &Block, ours: &Block, updates: u32) -> bool {
    flags.withdraw_allowed
        && !flags.enemy_capture
        && plan == 2
        && ours.total.wrapping_mul(2) <= ours_initial.total
        && updates > 1200
}

/// `0x007536C0` (CONFIRMED): with no defender in the battle and more than one unit of ours,
/// every unit of ours is the general's, melee infantry (class 0x13) or fixed artillery (class
/// 0) and every enemy unit is mounted (when the enemy has units), and some unit on either side
/// can fire. `ours` / `theirs` give `(class, can fire)` and `(mounted, can fire)`.
pub fn composition_defends(any_defender: bool, ours: &[(u8, bool, bool)], theirs: &[(bool, bool)]) -> bool {
    if any_defender || ours.len() <= 1 {
        return false;
    }
    // (class, is the general's unit, can fire)
    if ours.iter().any(|&(class, general, _)| !general && class != 0x13 && class != 0) {
        return false;
    }
    let ours_fire = ours.iter().any(|x| x.2);
    if theirs.is_empty() {
        return false;
    }
    if theirs.iter().any(|&(mounted, _)| !mounted) {
        return false;
    }
    ours_fire || theirs.iter().any(|x| x.1)
}

/// The plan vote `0x007B1FB0` (CONFIRMED). Returns the new plan (0..11) and updates `state`.
/// `updates` is the alliance-AI update counter (`+0x634`).
pub fn vote(state: &mut PlanState, flags: &Flags, initial: (&Block, &Block), now: (&Block, &Block), updates: u32) -> u32 {
    let (ours_initial, enemy_initial) = initial;
    let (ours, enemy) = now;
    let mut v = [0i32; 12];
    v[0] = 1;
    if state.plan != 0 && (state.plan as usize) < 12 {
        v[state.plan as usize] += 3;
    }
    // bVar25: hold-ground posture.
    let hold = (flags.defender || flags.enemy_capture)
        && !flags.pending
        && !flags.second_army_flag
        && !flags.enemy_artillery_wins;
    // Constant inputs: `0x004613B0` returns −1 (no time limit), `0x0075BC80` is always false,
    // `0x00462CB0` returns 0.
    let time_limit = false;
    let c12 = false;
    let c9 = false;

    let e = if enemy.total == 0 { -1 } else { enemy.total };
    let o = if ours.total == 0 { -1 } else { ours.total };
    let u14 = o.wrapping_sub(e).wrapping_mul(10) / e;
    let i15 = e.wrapping_sub(o).wrapping_mul(10) / o;
    let enemy_loss = if enemy_initial.men > 0 { loss_percent(enemy_initial.men, enemy.men) } else { 0 };
    // bVar5: we are losing the exchange.
    let losing = if ours_initial.melee < 1 {
        false
    } else {
        let our_loss = loss_percent(ours_initial.men, ours.men);
        if our_loss > 0 {
            state.loss_updates += 1;
        }
        (our_loss >= 31 && our_loss > enemy_loss) || (our_loss >= 15 && our_loss > enemy_loss.wrapping_mul(10))
    };
    // bVar4: outgunned at range.
    let outgunned = ours.missile.wrapping_mul(2) < enemy.missile
        && ours.melee < enemy.melee.wrapping_add(enemy.missile)
        && !time_limit
        && !flags.enemy_capture
        && !c12;
    // Logical shift of the (unsigned) own total, as in the exe.
    let half_o = ((o as u32) >> 1) as i32;
    if e < 1 {
        if hold {
            v[4] += 10;
        } else if !flags.our_capture {
            v[0] += 100;
        } else {
            v[6] += 100;
            v[7] += 3;
        }
    } else if e < half_o {
        v[8] = v[8].wrapping_add(u14);
        v[7] = v[7].wrapping_add(((u14 as u32) >> 1) as i32);
        if ours.missile < enemy.missile.wrapping_mul(2) {
            v[8] += 9;
        }
    } else if e < o {
        let h = ((u14 as u32) >> 1) as i32;
        let v6 = v[6];
        v[7] = v[7].wrapping_add(3).wrapping_add(u14);
        v[6] = v6.wrapping_add(3).wrapping_add(h);
        if hold && u14 < 10 {
            v[4] = v[4].wrapping_add(8 - h);
        }
        if outgunned {
            v[6] = v6.wrapping_add(6).wrapping_add(h);
            if losing {
                v[6] = v6.wrapping_add(15).wrapping_add(h);
            }
        }
    } else if (e.wrapping_mul(3) as u32) < ((o as u32) << 2) {
        let (v4, v6) = (v[4], v[6]);
        v[6] += 3;
        v[4] += 3;
        if hold {
            v[4] = v4 + 6;
        } else {
            v[6] = v6 + 6;
        }
        let base = v[6];
        if outgunned {
            v[6] += 3;
            if losing {
                v[6] = base + 12;
            }
        }
    } else if e < o.wrapping_mul(2) {
        let v6 = v[6];
        v[4] = v[4].wrapping_add(3).wrapping_add(i15);
        if outgunned {
            v[6] += 3;
            if losing {
                v[6] = v6 + 12;
            }
        }
    } else {
        v[2] = v[2].wrapping_add(i15);
        v[4] += 12;
        if outgunned {
            v[2] += 5;
        }
    }
    // Highest vote; ties keep the earlier option.
    let mut p = 0usize;
    for i in 1..12 {
        if v[p] < v[i] {
            p = i;
        }
    }
    let mut p = p as u32;
    if p == 8 && c12 {
        p = if hold { 5 } else { 6 };
    }
    if hold && time_limit && matches!(p, 0 | 2 | 6 | 7) {
        p = 4;
    }
    if flags.enemy_capture {
        p = 3;
    }
    if flags.our_capture {
        p = 9;
    }
    if o <= 0 {
        p = 0;
    }
    let mut to_250f = false;
    if hold && withdraws(p, flags, ours_initial, ours, updates) {
        p = 1;
        to_250f = true;
    }
    if !to_250f {
        // 0x007B24B8
        if p == 2 {
            p = 4;
        } else if p == 1 {
            to_250f = true;
        }
        if !to_250f {
            if !hold && matches!(p, 2 | 4 | 5) {
                p = 6;
                if flags.our_capture {
                    p = 9;
                }
            }
            if flags.enemy_capture {
                p = 3;
            }
            if flags.our_capture {
                p = 9;
            }
            if c9 {
                p = 11;
            }
        }
    }
    // 0x007B250F
    if !hold && matches!(p, 6..=8) && flags.composition_defends {
        p = 4;
    }
    // 0x007B2533
    if !flags.pending && flags.defender && !flags.enemy_capture && p == 7 && flags.first_army_b8_zero {
        p = 4;
    }
    state.plan = p;
    p
}

/// `0x007D7020` (CONFIRMED): the alliance mode for a plan. Plan 3 keeps the current mode; plan 7
/// keeps mode 11 (outside deployment) and is otherwise 2. Plan 10's function `0x007AE7B0` is not
/// ported (no vote ever goes to 10).
pub fn mode_for(plan: u32, current: Mode, deploying: bool) -> Mode {
    match plan {
        0 => Mode::Search,
        1 | 2 => Mode::Withdraw,
        4 => Mode::Defend,
        5 => Mode::Mode11,
        6 | 8 => Mode::Attack,
        7 => {
            if !deploying && current == Mode::Mode11 {
                Mode::Mode11
            } else {
                Mode::Attack
            }
        }
        9 => Mode::Assault,
        11 => Mode::Mode13,
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(men: i32, melee: i32, missile: i32) -> Block {
        Block { men, melee, missile, infantry: 0, mounted: 0, total: melee + missile }
    }

    fn run(flags: Flags, ours: Block, enemy: Block) -> u32 {
        let mut s = PlanState::default();
        vote(&mut s, &flags, (&ours, &enemy), (&ours, &enemy), 20)
    }

    #[test]
    fn attacker_always_attacks() {
        // Not holding ground: every defend plan (2/4/5) becomes 6 (attack).
        let f = Flags::default();
        for (o, e) in [(1000, 3000), (1000, 1500), (1000, 1100), (1000, 900), (1000, 300)] {
            let p = run(f, block(1000, o / 2, o / 2), block(1000, e / 2, e / 2));
            assert_eq!(mode_for(p, Mode::None, true), Mode::Attack, "{o} v {e}: plan {p}");
        }
    }

    #[test]
    fn defender_holds_unless_stronger() {
        let f = Flags { defender: true, ..Flags::default() };
        // Enemy twice as strong or more: plan 2 → 4 (defend).
        assert_eq!(run(f, block(1000, 500, 500), block(1000, 1500, 1500)), 4);
        // Even: the defender's +6 on plan 4 beats plan 6.
        assert_eq!(run(f, block(1000, 500, 500), block(1000, 500, 500)), 4);
        // Enemy weaker than us (1000 v 700): u14 = 4; votes 7 = 7, 6 = 5, 4 = 8 − 2 = 6 → 7.
        assert_eq!(run(f, block(1000, 500, 500), block(1000, 350, 350)), 7);
        // Far stronger: plan 8 (attack).
        assert_eq!(run(f, block(1000, 500, 500), block(1000, 100, 100)), 8);
        assert_eq!(mode_for(8, Mode::Defend, false), Mode::Attack);
    }

    #[test]
    fn hysteresis_and_ties() {
        let f = Flags { defender: true, ..Flags::default() };
        // 1000 v 800: u14 = 2 → v7 = 5, v6 = 4, v4 = 7: plan 4. With the previous plan 7 it gets
        // +3 → 8 > 7: plan 7 stays.
        let (o, e) = (block(1000, 500, 500), block(1000, 400, 400));
        let mut s = PlanState::default();
        assert_eq!(vote(&mut s, &f, (&o, &e), (&o, &e), 20), 4);
        let mut s = PlanState { plan: 7, ..PlanState::default() };
        assert_eq!(vote(&mut s, &f, (&o, &e), (&o, &e), 20), 7);
    }

    #[test]
    fn no_strength_and_capture_overrides() {
        // No enemy strength seen: search (plan 0 → mode 8); no own strength: plan 0.
        let f = Flags::default();
        assert_eq!(run(f, block(1000, 500, 500), block(0, 0, 0)), 0);
        assert_eq!(mode_for(0, Mode::Attack, false), Mode::Search);
        assert_eq!(run(f, block(0, 0, 0), block(1000, 500, 500)), 0);
        // A capture goal of ours → plan 9 (mode 10); the enemy's → plan 3 (mode kept).
        assert_eq!(run(Flags { our_capture: true, ..f }, block(1000, 500, 500), block(1000, 500, 500)), 9);
        assert_eq!(run(Flags { enemy_capture: true, ..f }, block(1000, 500, 500), block(1000, 500, 500)), 3);
        assert_eq!(mode_for(3, Mode::Defend, false), Mode::Defend);
    }

    #[test]
    fn defender_withdraws_when_beaten() {
        let f = Flags { defender: true, withdraw_allowed: true, ..Flags::default() };
        let initial = block(1000, 500, 500);
        let now = block(400, 200, 200);
        let enemy = block(1000, 1500, 1500);
        let mut s = PlanState::default();
        // Before 1200 updates: defend.
        assert_eq!(vote(&mut s, &f, (&initial, &enemy), (&now, &enemy), 1200), 4);
        let mut s = PlanState::default();
        assert_eq!(vote(&mut s, &f, (&initial, &enemy), (&now, &enemy), 1201), 1);
        assert_eq!(mode_for(1, Mode::Defend, false), Mode::Withdraw);
    }

    #[test]
    fn outgunned_and_losing_favours_the_attack() {
        // Defender, slightly stronger (1000 v 900) but outgunned at range and losing men.
        let f = Flags { defender: true, ..Flags::default() };
        let ours_initial = Block { men: 1000, melee: 800, missile: 200, total: 1000, ..Block::default() };
        let ours = Block { men: 600, melee: 800, missile: 200, total: 1000, ..Block::default() };
        let enemy = Block { men: 1000, melee: 400, missile: 500, total: 900, ..Block::default() };
        let mut s = PlanState::default();
        // u14 = 1: v7 = 4, v4 = 8, v6 = 0 + 15 + 0 = 15 → plan 6.
        assert_eq!(vote(&mut s, &f, (&ours_initial, &enemy), (&ours, &enemy), 20), 6);
        assert_eq!(s.loss_updates, 1);
    }

    #[test]
    fn composition_rule() {
        // Two melee-infantry units of ours, all-mounted enemy, someone can fire.
        assert!(composition_defends(false, &[(0x13, false, false), (0, false, true)], &[(true, false)]));
        assert!(!composition_defends(true, &[(0x13, false, false), (0, false, true)], &[(true, false)]));
        assert!(!composition_defends(false, &[(0x12, false, true), (0x13, false, false)], &[(true, false)]));
        assert!(!composition_defends(false, &[(0x13, false, false), (0x13, false, false)], &[(true, false)]));
    }
}

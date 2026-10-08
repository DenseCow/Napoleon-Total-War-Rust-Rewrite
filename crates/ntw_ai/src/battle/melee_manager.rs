//! The melee manager: per-unit objectives (MELEE / MISSILE / RETREAT) and their allocation.
//!
//! Structure from the exe (`analysis/ai/AI_RESEARCH.md` §3.3):
//! - Analysers (`AI_MELEE_ATTACK_ANALYSER` `0x00749E90`, `AI_MELEE_MISSILE_ANALYSER` `0x0074A090`)
//!   walk the enemy alliance's units. Each valid target (unit active state not 0/2 and with
//!   soldiers, `0x0055CBD0`, CONFIRMED) gets an objective with a base priority, then one
//!   sub-objective per possible attacker of ours (CONFIRMED).
//! - Objective type codes: 0 = MELEE, 3 = MISSILE, 4 = RETREAT (CONFIRMED from the log strings).
//! - A melee attacker within **50 m** of the enemy (`< 2500` squared, CONFIRMED in `0x00791FD0`)
//!   is force-assigned ("Force Melee Attack ... Force Assign Override Level").
//! - The missile analyser drops targets out of the attacker's range (`is_viable_target`, CONFIRMED
//!   log text).
//! - Allocation `0x007496E0` (CONFIRMED algorithm, [`allocate`]): repeatedly, for every unassigned
//!   unit, find its best and second-best objective among those that meet their minimum priority;
//!   assign the unit with the **largest best-minus-second difference** to its best objective; repeat
//!   until no unit can be assigned.
//!
//! PROVISIONAL: how a sub-objective's priority combines base priority, potential and distance
//! (a virtual call we have not decoded) and the minimum-priority rule; see [`Candidate`].

use std::collections::BTreeMap;

/// Objective kind, with the original's type codes (CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ObjectiveKind {
    /// 0: attack the target in melee (charge).
    Melee = 0,
    /// 3: shoot the target.
    Missile = 3,
    /// 4: fall back out of the fight.
    Retreat = 4,
}

impl ObjectiveKind {
    /// The tag the original writes in `battle_ai_melee_log.txt` (CONFIRMED strings).
    pub fn log_tag(self) -> &'static str {
        match self {
            ObjectiveKind::Melee => "(MELEE)",
            ObjectiveKind::Missile => "(MISSILE)",
            ObjectiveKind::Retreat => "(RETREAT)",
        }
    }
}

/// One possible objective for one of our units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    /// What to do.
    pub kind: ObjectiveKind,
    /// The enemy unit id (none for RETREAT).
    pub target: Option<u32>,
    /// Allocation priority (higher is better), before the objective's cap rule.
    pub priority: f32,
    /// The minimum-priority test (objective virtual `+0x18`, `0x0079C730`).
    pub meets_minimum: bool,
    /// Force-assigned (melee within 50 m, CONFIRMED distance).
    pub forced: bool,
    /// The sub-objective's potential (`+0x38`): what an assignment adds to the objective's
    /// accumulated potential (objective virtual `+0x1C`, `0x007CF5A0` / `0x007CF7C0`, CONFIRMED).
    pub potential: f32,
}

/// [`allocate`]'s choice for one unit.
pub type Assignments = BTreeMap<u32, Candidate>;

/// CONFIRMED (`0x00755BF0`): a melee objective's potential cap `+0x3C` (2.5). Missile objectives
/// use 1000 (CONFIRMED, effectively no cap) and their priority ignores it.
pub const MELEE_POTENTIAL_CAP: f32 = 2.5;

/// The melee priority after the objective's cap rule (`0x007D0D10`, CONFIRMED): 0 once the
/// accumulated potential of the attackers already assigned reaches the cap; ×0.3 when some is
/// assigned and the room left is smaller than this attacker's potential.
pub fn capped_priority(c: &Candidate, accumulated: f32) -> f32 {
    if c.kind != ObjectiveKind::Melee {
        return c.priority;
    }
    let room = MELEE_POTENTIAL_CAP - accumulated;
    if room <= 0.0 {
        0.0
    } else if accumulated != 0.0 && room < c.potential {
        c.priority * 0.3
    } else {
        c.priority
    }
}

/// The CONFIRMED allocation loop of `0x007496E0` (see the module docs). `candidates` maps our unit
/// id to its candidate objectives. Units and candidates are visited in id / list order, and ties
/// keep the earlier one, so the result is deterministic.
///
/// A forced candidate always wins its unit (the "Force Assign Override"). Every assignment adds the
/// attacker's potential to its objective; melee priorities are re-evaluated against the cap on
/// each round (the original re-asks the sub-objective's priority virtual), so attackers spread over
/// targets once one is saturated.
pub fn allocate(candidates: &BTreeMap<u32, Vec<Candidate>>) -> Assignments {
    let mut out = Assignments::new();
    // Accumulated potential per (kind, target).
    let mut acc: BTreeMap<(ObjectiveKind, u32), f32> = BTreeMap::new();
    let key = |c: &Candidate| c.target.map(|t| (c.kind, t));
    // Forced assignments first.
    for (&unit, list) in candidates {
        let mut best: Option<Candidate> = None;
        for c in list.iter().filter(|c| c.forced) {
            if best.is_none_or(|b| c.priority > b.priority) {
                best = Some(*c);
            }
        }
        if let Some(c) = best {
            out.insert(unit, c);
            if let Some(k) = key(&c) {
                *acc.entry(k).or_insert(0.0) += c.potential;
            }
        }
    }
    loop {
        // (unit, best, difference)
        let mut pick: Option<(u32, Candidate, f32)> = None;
        for (&unit, list) in candidates {
            if out.contains_key(&unit) {
                continue;
            }
            let mut best: Option<Candidate> = None;
            let mut second = 0.0f32;
            for c in list.iter().filter(|c| c.meets_minimum) {
                let a = key(c).and_then(|k| acc.get(&k).copied()).unwrap_or(0.0);
                let p = capped_priority(c, a);
                if p <= 0.0 && c.kind == ObjectiveKind::Melee {
                    continue; // saturated objective
                }
                let c = Candidate { priority: p, ..*c };
                match best {
                    Some(b) if c.priority <= b.priority => {
                        if c.priority > second {
                            second = c.priority; // "Assigned as 2nd Priority"
                        }
                    }
                    _ => {
                        if let Some(b) = best {
                            second = b.priority;
                        }
                        best = Some(c); // "Assigned as Best Objective"
                    }
                }
            }
            let Some(b) = best else { continue };
            let diff = b.priority - second;
            // "This is the Unit with the highest priority difference so far"
            if pick.is_none_or(|(_, _, d)| diff > d) {
                pick = Some((unit, b, diff));
            }
        }
        let Some((unit, chosen, _)) = pick else { break };
        out.insert(unit, chosen);
        if let Some(k) = key(&chosen) {
            *acc.entry(k).or_insert(0.0) += chosen.potential;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(kind: ObjectiveKind, target: u32, priority: f32) -> Candidate {
        Candidate { kind, target: Some(target), priority, meets_minimum: true, forced: false, potential: 1.0 }
    }

    #[test]
    fn largest_regret_goes_first() {
        // Unit 1: 10 vs 9 (difference 1). Unit 2: 8 vs 0 (difference 8) -> unit 2 is assigned
        // first; both still get their best objective.
        let mut m = BTreeMap::new();
        m.insert(1, vec![c(ObjectiveKind::Missile, 7, 10.0), c(ObjectiveKind::Missile, 8, 9.0)]);
        m.insert(2, vec![c(ObjectiveKind::Missile, 7, 8.0)]);
        let a = allocate(&m);
        assert_eq!(a[&1].target, Some(7));
        assert_eq!(a[&2].target, Some(7));
    }

    #[test]
    fn minimum_and_force() {
        let mut low = c(ObjectiveKind::Melee, 7, 100.0);
        low.meets_minimum = false;
        let mut forced = c(ObjectiveKind::Melee, 8, 1.0);
        forced.forced = true;
        let mut m = BTreeMap::new();
        m.insert(1, vec![low]);
        m.insert(2, vec![low, forced]);
        let a = allocate(&m);
        assert!(!a.contains_key(&1), "nothing meets the minimum");
        assert_eq!(a[&2].target, Some(8));
    }

    #[test]
    fn melee_targets_spread_once_the_cap_is_reached() {
        // Potential 2.0 each: after one attacker the room left (0.5) is less than the next one's
        // potential, so its priority there drops to x0.3 (3.0 < 9.0) and it takes the other target.
        let strong = |t, p| Candidate { potential: 2.0, ..c(ObjectiveKind::Melee, t, p) };
        let mut m = BTreeMap::new();
        m.insert(1, vec![strong(7, 10.0), strong(8, 9.0)]);
        m.insert(2, vec![strong(7, 10.0), strong(8, 9.0)]);
        let a = allocate(&m);
        assert_ne!(a[&1].target, a[&2].target);
        // Weak attackers (potential 1.0) share a target until it is saturated (2.5).
        let mut m = BTreeMap::new();
        m.insert(1, vec![c(ObjectiveKind::Melee, 7, 10.0), c(ObjectiveKind::Melee, 8, 9.0)]);
        m.insert(2, vec![c(ObjectiveKind::Melee, 7, 10.0), c(ObjectiveKind::Melee, 8, 9.0)]);
        m.insert(3, vec![c(ObjectiveKind::Melee, 7, 10.0), c(ObjectiveKind::Melee, 8, 9.0)]);
        let a = allocate(&m);
        assert_eq!(a[&1].target, Some(7));
        assert_eq!(a[&2].target, Some(7));
        assert_eq!(a[&3].target, Some(8), "2.0 accumulated, 0.5 room < 1.0: x0.3");
    }

    #[test]
    fn capped_priority_rule() {
        let x = Candidate { potential: 1.0, ..c(ObjectiveKind::Melee, 7, 10.0) };
        assert_eq!(capped_priority(&x, 0.0), 10.0);
        assert_eq!(capped_priority(&x, 1.5), 10.0);
        assert!((capped_priority(&x, 2.0) - 3.0).abs() < 1e-6);
        assert_eq!(capped_priority(&x, 2.5), 0.0);
        let m = c(ObjectiveKind::Missile, 7, 10.0);
        assert_eq!(capped_priority(&m, 100.0), 10.0);
    }
}

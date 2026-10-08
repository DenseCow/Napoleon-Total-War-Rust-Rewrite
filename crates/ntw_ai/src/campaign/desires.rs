//! The BDI desire priorities of the behaviours (`analysis/ai/AI_RESEARCH.md` §4 "Desires" and
//! "BDI pool processing"; how the pool uses them is [`super::bdi`]).
//!
//! In the original every manager behaviour is thin: once per deliberation its step (vtable slot
//! 12) creates one **desire** or goal per target and gives it a priority through
//! `0x00CB2560(desire, add, mult, slot)`. That call adds a *link* to the desire; the desire's
//! total is its own base plus, for every link, `source priority × mult + add` (`0x00CEFC50`),
//! and its final priority (`+0xEC`) is `(1 + r) × total` with the per-component jitter `r`
//! (`+0xE8`, drawn raw from ±`PRIORITY_RANDOMIZATION_DESIRE` when the component joins the pool,
//! `0x00CB5CF0`). All CONFIRMED unless marked.
//!
//! The multipliers each behaviour passes (CONFIRMED; round 6 corrected which behaviour owns
//! which step, see AI_RESEARCH §4 "CORRECTION"):
//! * `REGION_GROUP_DEFENCE` (`0x00D65650`, one `CAI_BDI_GOAL_REGION_GROUP_DEFENCE` per own
//!   region group) and `REGION_DEFENCE` (`0x00CD4AE0`, one desire per own region): multiplier
//!   `value / total × (1 − k) × N + k` ([`expansion_multipliers`]), `N` = number of desires,
//!   `total` = the sum of the values (integer, 1 when 0), `k` =
//!   `BASIC_DESIRES_DEFEND_REGION_GROUPS_GOAL_BASE_COMPONENT_PROPORTION` resp.
//!   `BASIC_DESIRES_DEFEND_REGION_GOAL_BASE_COMPONENT_PROPORTION` (both 25 in the shipped
//!   `default`, so the multiplier is `25 − 24 × value / mean value`: the most valuable targets
//!   get the *lowest*, even negative, multipliers; in the pool that only orders them later).
//! * `REGION_COAST_DEFENCE` (`0x00CD4F90`): candidate regions sorted by value, highest first
//!   (stable), multipliers `1, 0.5, 0.25, …` ([`region_defence_multipliers`]; not used yet: the
//!   model has no coast flag).
//! * `MERGE_UNITS` (`0x00CD6260`): multiplier 1.0 for each desire it makes.

/// `0x00CB2560` links: the desire priority from a behaviour priority and a multiplier (`add` is 0
/// in every behaviour decoded so far).
pub fn link_priority(behaviour_priority: f32, mult: f32, add: f32) -> f32 {
    behaviour_priority * mult + add
}

/// `REGION_GROUP_DEFENCE` multipliers for `(target, value)` candidates, in the given order.
/// Values are the analyser's integers (`0x00A63FD0`); the total is accumulated as an integer
/// (each step truncates `total as f32 + value`), and 1 when it ends at 0 (CONFIRMED,
/// `0x00D90A60`).
pub fn expansion_multipliers<T: Copy>(candidates: &[(T, i32)], k: f32) -> Vec<(T, f32)> {
    let mut total: i32 = 0;
    for &(_, v) in candidates {
        total = (total as f32 + v as f32) as i32;
    }
    if total as f32 == 0.0 {
        total = 1;
    }
    let n = candidates.len() as f32;
    let scale = (1.0 - k) * n;
    candidates.iter().map(|&(t, v)| (t, v as f32 / total as f32 * scale + k)).collect()
}

/// `REGION_DEFENCE` multipliers (`0x00CD4AE0`, CONFIRMED): the same blend as
/// [`expansion_multipliers`], evaluated as `(value × (1 − k) × N) / total + k` in `f32`, with the
/// total built the same way (`0x00D0E090`).
pub fn defend_region_multipliers<T: Copy>(candidates: &[(T, i32)], k: f32) -> Vec<(T, f32)> {
    let mut total: i32 = 0;
    for &(_, v) in candidates {
        total = (total as f32 + v as f32) as i32;
    }
    if total as f32 == 0.0 {
        total = 1;
    }
    let n = candidates.len() as f32;
    candidates.iter().map(|&(t, v)| (t, v as f32 * (1.0 - k) * n / total as f32 + k)).collect()
}

/// `REGION_COAST_DEFENCE` multipliers: candidates sorted by value, highest first (ties keep their
/// order), then `1, 0.5, 0.25, …` (CONFIRMED, `0x00CD4F90`).
pub fn region_defence_multipliers<T: Copy>(candidates: &[(T, i32)]) -> Vec<(T, f32)> {
    let mut sorted: Vec<(T, i32)> = Vec::with_capacity(candidates.len());
    for &(t, v) in candidates {
        let at = sorted.iter().position(|&(_, w)| v > w).unwrap_or(sorted.len());
        sorted.insert(at, (t, v));
    }
    let mut m = 1.0f32;
    sorted
        .into_iter()
        .map(|(t, _)| {
            let out = (t, m);
            m *= 0.5;
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expansion_blends_relative_value_with_k() {
        // k = 0: the multiplier is value / mean value.
        let m = expansion_multipliers(&[(1, 30), (2, 10)], 0.0);
        assert_eq!(m, vec![(1, 1.5), (2, 0.5)]);
        // k = 1: constant.
        let m = expansion_multipliers(&[(1, 30), (2, 10)], 1.0);
        assert_eq!(m, vec![(1, 1.0), (2, 1.0)]);
        // The shipped default (25): 25 - 24 x value / mean.
        let m = expansion_multipliers(&[(1, 30), (2, 10)], 25.0);
        assert_eq!(m, vec![(1, 25.0 - 36.0), (2, 25.0 - 12.0)]);
        // All zero: total 1.
        let m = expansion_multipliers(&[(1, 0)], 0.5);
        assert_eq!(m, vec![(1, 0.5)]);
    }

    #[test]
    fn defend_region_blend() {
        let m = defend_region_multipliers(&[(1, 30), (2, 10)], 25.0);
        assert_eq!(m, vec![(1, 30.0 * -24.0 * 2.0 / 40.0 + 25.0), (2, 10.0 * -24.0 * 2.0 / 40.0 + 25.0)]);
    }

    #[test]
    fn region_defence_halves_by_rank() {
        let m = region_defence_multipliers(&[('a', 5), ('b', 9), ('c', 5), ('d', 1)]);
        assert_eq!(m, vec![('b', 1.0), ('a', 0.5), ('c', 0.25), ('d', 0.125)]);
        assert_eq!(link_priority(2000.0, 0.5, 0.0), 1000.0);
    }
}

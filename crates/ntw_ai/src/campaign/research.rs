//! `RESEARCH_TECHNOLOGY`: the campaign AI's technology research (`analysis/ai/AI_RESEARCH.md` §4
//! "RESEARCH_TECHNOLOGY", CONFIRMED round 7).
//!
//! The behaviour is one `campaign_ai_manager_behaviour_junctions` row, priority **500** in every
//! shipped manager that has one (CONFIRMED by the shipped table: `nap_eur_full`,
//! `nap_eur_france`, `nap_eur_britain`, `nap_eur_maintainance`, `nap_spa_*`, `nap_egy_full`; the
//! `nap_ita_*` managers have no navy and no research row).
//!
//! Shape (CONFIRMED):
//! * the step `0x00C2AD60` sorts its candidates by the exe's key descending (skipping −1), makes
//!   one `CAI_BDI_GOAL_RESEARCH_TECHNOLOGY` goal per candidate (vtable `0x013825B4`, type 0x110)
//!   and links each with **mult `1.0`, then `× 0.95` per entry in list order**;
//! * a goal's refresh (`0x00C5F2C0` → `0x00C3FF10`) asks an analyser for **one** technology and
//!   leaves the goal finished when there is none;
//! * a goal's deliberation (`0x00C2E350`) reserves a school for it (`0x00A74B50` → belief type
//!   `0x69` `0x00A3B080` + `0x00A614A0`, one per gentleman: a later one replaces an earlier school)
//!   and, **only if a school was reserved**, makes one `CAI_BDI_RESEARCH`-type intention (id 0xE5,
//!   vtable `0x013828A0`) linked with mult 1.0;
//! * that intention acts (`0x00CE4720`): `0x00AA4B80(tech, gentleman, school)` → the research start
//!   `0x008EEC90`; a failed start drops the intention (`0x00D0A1D0`).
//!
//! **PROVISIONAL / UNKNOWN:** the candidate list is an analyser over each gentleman's own
//! technology list (`0x00C40310`, `0x00AAF1F0`: his list is non-empty and an effect `0x27` is
//! positive), then the gentlemen of the factions whose relation entry `+0x160` is 0 and `+0x188`
//! is 0, sorted by character attribute 8; the sort key `0x00C288B0` is a pointer chase through
//! `+0xFC → +0x1E0 → +8` that is not decoded, and the analyser behind the refresh is not read. Our
//! stand-in therefore uses the faction's **available technologies in research-cost order** (the
//! cheapest, which open the technology tree, first) and, for the school, the free school with the
//! best rate for the technology's thread. Both are deterministic and seed-free.

/// A technology the AI may start now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchCand {
    /// Technology key.
    pub key: String,
    /// `technologies` #3 research cost in points.
    pub cost: i32,
    /// Thread of the school rate that researches it: 0 military, 1 industry, 2 enlightenment
    /// (`ntw_sim::campaign::research::thread_of`).
    pub thread: usize,
}

/// The goal link multipliers of `0x00C2AD60`: `1.0`, `×0.95`, `×0.9025`, … in candidate list order
/// (CONFIRMED: `FUN_00CB2560(goal, 0, m, 0)` with `m` starting at `0x3F800000` and multiplied by
/// `0.95` after each entry).
pub fn goal_multipliers(candidates: &[ResearchCand]) -> Vec<(String, f32)> {
    let mut m = 1.0f32;
    candidates
        .iter()
        .map(|c| {
            let out = (c.key.clone(), m);
            m *= 0.95;
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(k: &str, cost: i32) -> ResearchCand {
        ResearchCand { key: k.to_string(), cost, thread: 0 }
    }

    #[test]
    fn goal_multipliers_decay_by_five_percent() {
        let m = goal_multipliers(&[cand("a", 100), cand("b", 200), cand("c", 300)]);
        assert_eq!(m, vec![("a".to_string(), 1.0), ("b".to_string(), 0.95), ("c".to_string(), 0.9025)]);
        // 0x00C2AD60 starts at 0x3F800000 and multiplies by 0.95 after every link.
        assert_eq!(m[2].1, 1.0f32 * 0.95 * 0.95);
    }

    #[test]
    fn no_candidates_means_no_goals() {
        assert!(goal_multipliers(&[]).is_empty());
    }
}

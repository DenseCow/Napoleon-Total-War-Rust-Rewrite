//! The campaign's rule seams (DESIGN.md §3.3.1, [`crate::seam`]): every campaign rule a mod may
//! replace or extend, by its seam key, and the registry of implementations data picks from.
//!
//! [`CampaignSeams`] lives in [`super::CampaignRules::seams`] (game data, rebuilt on load, not
//! saved); [`CampaignSeams::default`] is the original game. The model, the UI and the AI call a
//! rule only through its seam, so a mod's rule applies everywhere the original's did.
//!
//! Seams so far (one per rule function; the systems still to be moved behind seams are BACKLOG
//! §11 "Replaceable systems" lines):
//! - [`POPULATION_GROW`]: a region's round-end growth ([`super::population::grow`]).

use std::sync::Arc;

use super::mod_state::ModWrites;
use super::population::PopulationState;
use super::world::{CampaignModel, Region};
use crate::seam::{Seam, SeamRegistry};

/// Seam key of a region's population growth for one round ([`PopulationGrow`]).
pub const POPULATION_GROW: &str = "population.grow";

/// A region's population after one round of growth: given the model, the region, its population
/// and its population state, the new population and state. The original's is
/// [`super::population::grow`]. Its mod-state changes go to the [`ModWrites`].
pub type PopulationGrow = dyn Fn(&CampaignModel, &Region, u32, &PopulationState, &mut ModWrites) -> (u32, PopulationState) + Send + Sync;

/// The campaign rules in use, one field per seam (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct CampaignSeams {
    /// [`POPULATION_GROW`].
    pub population_grow: Seam<PopulationGrow>,
}

impl Default for CampaignSeams {
    /// The original game's rules.
    fn default() -> Self {
        CampaignSeams { population_grow: Seam::original(Arc::new(|m: &CampaignModel, r: &Region, pop: u32, s: &PopulationState, _: &mut ModWrites| super::population::grow(m, r, pop, s))) }
    }
}

impl CampaignSeams {
    /// Every seam's key and implementation chain, in a fixed order (for the multiplayer handshake
    /// and the log).
    pub fn chains(&self) -> [(&'static str, &[String]); 1] {
        [(POPULATION_GROW, self.population_grow.chain())]
    }
}

/// The campaign rule implementations a game knows, per seam (see the module docs). The engine
/// registers its own; a fork registers more in Rust, the extended Lua API (later) one per rule
/// script.
#[derive(Default)]
pub struct CampaignRuleRegistry {
    /// [`POPULATION_GROW`] implementations.
    pub population_grow: SeamRegistry<PopulationGrow>,
}

impl CampaignRuleRegistry {
    /// The rules data selects: the original's, then each `(seam key, implementation key)` of
    /// `selection` applied in order (the mods' load order). An unknown seam or implementation is
    /// skipped and described in `warnings` (the caller logs them once).
    pub fn seams(&self, selection: &[(String, String)], warnings: &mut Vec<String>) -> CampaignSeams {
        let mut seams = CampaignSeams::default();
        for (seam, key) in selection {
            let applied = match seam.as_str() {
                POPULATION_GROW => self.population_grow.apply(&mut seams.population_grow, key),
                _ => {
                    warnings.push(format!("rule seam {seam}: no such seam; {key} is ignored"));
                    continue;
                }
            };
            if !applied {
                warnings.push(format!("rule seam {seam}: no implementation {key}; it is ignored"));
            }
        }
        seams
    }
}

#[cfg(test)]
mod tests {
    use super::super::ids::{FactionId, RegionId};
    use super::super::mod_state::{ModKey, ModScope, ModState, ModValue};
    use super::super::tests::test_model;
    use super::*;

    const OWNER: &str = "test_mod.lean_years";

    /// A MADE-UP mod rule that extends the original: it counts each region's rounds in mod state
    /// and takes one more person away per round counted.
    fn lean_years() -> CampaignRuleRegistry {
        let mut reg = CampaignRuleRegistry::default();
        let maker = |prev: Arc<PopulationGrow>| {
            Arc::new(move |m: &CampaignModel, r: &Region, pop: u32, s: &PopulationState, w: &mut ModWrites| {
                let (grown, state) = prev(m, r, pop, s, w);
                let key = ModKey::new(OWNER, ModScope::Region(r.id), "rounds");
                let rounds = match m.mod_state.get(&key) {
                    Some(ModValue::Int(n)) => *n + 1,
                    _ => 1,
                };
                w.set(key, ModValue::Int(rounds));
                (grown.saturating_sub(rounds as u32), state)
            }) as Arc<PopulationGrow>
        };
        assert!(reg.population_grow.register(OWNER, Arc::new(maker)));
        reg
    }

    fn selection(key: &str) -> Vec<(String, String)> {
        vec![(POPULATION_GROW.to_owned(), key.to_owned())]
    }

    /// The model, its factors refreshed, after `rounds` round ends of faction A with `seams`.
    fn run(seams: CampaignSeams, rounds: u32) -> CampaignModel {
        let mut m = test_model();
        let mut rules = (*m.rules).clone();
        rules.variables.insert("baseline_pop_growth".into(), 0.3);
        rules.seams = seams;
        m.rules = Arc::new(rules);
        m.refresh_population_factors();
        for _ in 0..rounds {
            m.population_round_end(FactionId(1)); // test_model's faction A
        }
        m
    }

    #[test]
    fn vanilla_runs_the_original_growth_unchanged() {
        let mut warnings = Vec::new();
        let seams = CampaignRuleRegistry::default().seams(&[], &mut warnings);
        assert!(warnings.is_empty());
        assert_eq!(seams, CampaignSeams::default());
        assert!(seams.population_grow.is_original());
        assert_eq!(seams.chains(), [(POPULATION_GROW, &["original".to_owned()][..])]);
        // The round end through the seam gives exactly what the original's function gives.
        let before = run(CampaignSeams::default(), 0);
        let after = run(CampaignSeams::default(), 1);
        for (id, r) in &before.world.regions {
            let expected = if r.owner == FactionId(1) { super::super::population::grow(&before, r, r.population, &r.population_state) } else { (r.population, r.population_state.clone()) };
            let got = &after.world.regions[id];
            assert_eq!((got.population, &got.population_state), (expected.0, &expected.1), "region {id:?}");
        }
        assert_ne!(after.world.regions[&RegionId(10)].population, before.world.regions[&RegionId(10)].population);
        assert!(after.mod_state.is_empty());
    }

    #[test]
    fn unknown_seams_and_implementations_keep_the_original_and_warn() {
        let mut warnings = Vec::new();
        let sel = vec![("population.shrink".to_owned(), OWNER.to_owned()), (POPULATION_GROW.to_owned(), "nobody".to_owned())];
        assert!(lean_years().seams(&sel, &mut warnings).population_grow.is_original());
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn a_mod_rule_extends_the_original_and_keeps_its_state_in_the_model() {
        let mut warnings = Vec::new();
        let seams = lean_years().seams(&selection(OWNER), &mut warnings);
        assert!(warnings.is_empty());
        assert_eq!(seams.population_grow.chain(), ["original", OWNER]);
        let vanilla = run(CampaignSeams::default(), 3);
        let modded = run(seams.clone(), 3);
        // 1 + 2 + 3 fewer people after three rounds, in each of A's regions only.
        for (id, r) in &vanilla.world.regions {
            let lost = if r.owner == FactionId(1) { 6 } else { 0 };
            assert_eq!(modded.world.regions[id].population + lost, r.population, "region {id:?}");
        }
        let rounds = |id| modded.mod_state.get(&ModKey::new(OWNER, ModScope::Region(RegionId(id)), "rounds")).cloned();
        assert_eq!((rounds(10), rounds(11), rounds(12)), (Some(ModValue::Int(3)), None, Some(ModValue::Int(3))));
        assert_ne!(modded.state_hash(), vanilla.state_hash());
        // The panel's projection runs the same rule (the fourth round: 4 fewer) and changes nothing.
        let project = |m: &CampaignModel| super::super::population::project(m, RegionId(10)).expect("projection").population;
        let mut original = modded.clone();
        let mut rules = (*original.rules).clone();
        rules.seams = CampaignSeams::default();
        original.rules = Arc::new(rules);
        assert_eq!(project(&modded) + 4, project(&original));
        assert_eq!(rounds(10), Some(ModValue::Int(3)));
    }

    #[test]
    fn mod_state_is_deterministic() {
        // Two runs from the same start agree bit for bit.
        let seams = lean_years().seams(&selection(OWNER), &mut Vec::new());
        assert_eq!(run(seams.clone(), 4).state_hash(), run(seams, 4).state_hash());
        // Values written in any order are kept, iterated and hashed in key order.
        let keys = [
            ModKey::new("b_mod", ModScope::Campaign, "x"),
            ModKey::new("a_mod", ModScope::Region(RegionId(12)), "x"),
            ModKey::new("a_mod", ModScope::Faction(FactionId(-1)), "y"),
            ModKey::new("a_mod", ModScope::Region(RegionId(10)), "x"),
        ];
        let value = |i: usize| ModValue::List(vec![ModValue::Int(i as i64), ModValue::Float(0.1), ModValue::Text("t".into()), ModValue::Bool(true)]);
        let (mut one, mut two) = (test_model(), test_model());
        for (i, k) in keys.iter().enumerate() {
            one.mod_state.set(k.clone(), value(i));
        }
        let mut writes = ModWrites::default();
        for (i, k) in keys.iter().enumerate().rev() {
            writes.set(k.clone(), value(i));
        }
        two.mod_state.apply(&mut writes);
        assert!(writes.is_empty());
        assert_eq!(one.mod_state, two.mod_state);
        assert_eq!(one.state_hash(), two.state_hash());
        let order: Vec<&ModKey> = one.mod_state.iter().map(|(k, _)| k).collect();
        assert_eq!(order, [&keys[2], &keys[3], &keys[1], &keys[0]]);
        // The hash covers the values, and a removal through the writes applies.
        let mut w = ModWrites::default();
        w.remove(keys[0].clone());
        two.mod_state.apply(&mut w);
        assert_eq!(two.mod_state.len(), 3);
        assert_ne!(one.state_hash(), two.state_hash());
        assert_ne!(test_model().state_hash(), one.state_hash());
        assert_eq!(test_model().mod_state, ModState::default());
    }
}

//! Replaceable rules: the rule seam a mod replaces or extends (DESIGN.md §3.3.1; BACKLOG §11
//! "Replaceable systems").
//!
//! A game rule the model runs (one rule function: a region's growth, a unit's upkeep, a battle
//! tick's morale step, ...) is called only through its [`Seam`]. A seam holds the rule in use and
//! the chain of implementation keys that built it: [`ORIGINAL`] (the original game's 1:1 rule) and
//! then each mod's implementation in load order. A mod's implementation is made by a [`Maker`]
//! from the rule before it, so it either replaces that rule (ignores it) or extends it (calls it and
//! changes the result). The makers a game knows are kept by name in a [`SeamRegistry`]; data picks
//! them by name (the campaign's [`crate::campaign::seams::CampaignRuleRegistry::seams`]).
//!
//! The rule is an `Arc<dyn Fn ...>`: one indirect call per use, no allocation, no lookup by name
//! on the hot path (the names are resolved once when the rules are built). A seam is game data,
//! not state: it is rebuilt from the same data on load and never saved; the state a mod's rule
//! keeps is [`crate::campaign::mod_state::ModState`], which is.

use std::collections::BTreeMap;
use std::sync::Arc;

/// The implementation key of the original game's rule: the first link of every chain.
pub const ORIGINAL: &str = "original";

/// Makes a mod's rule from the rule before it in the chain (to replace it, ignore the argument; to
/// extend it, keep it and call it). Run once, when the rules are built.
pub type Maker<F> = Arc<dyn Fn(Arc<F>) -> Arc<F> + Send + Sync>;

/// One rule in use and how it was built (see the module docs). `F` is the rule's
/// `dyn Fn(...) + Send + Sync` type.
pub struct Seam<F: ?Sized> {
    rule: Arc<F>,
    chain: Vec<String>,
}

impl<F: ?Sized> Seam<F> {
    /// The original game's rule.
    pub fn original(rule: Arc<F>) -> Self {
        Seam { rule, chain: vec![ORIGINAL.to_owned()] }
    }

    /// The rule in use: call it.
    pub fn rule(&self) -> &F {
        &self.rule
    }

    /// The implementation keys that built the rule, [`ORIGINAL`] first, then each mod's in the
    /// order they were applied. Two seams with the same chain run the same rule; multiplayer peers
    /// compare it.
    pub fn chain(&self) -> &[String] {
        &self.chain
    }

    /// The original game's rule alone, as vanilla runs.
    pub fn is_original(&self) -> bool {
        self.chain.len() == 1
    }

    /// Puts the rule `maker` makes from the current one in its place, recorded as `key`.
    pub fn apply(&mut self, key: &str, maker: &Maker<F>) {
        self.rule = maker(Arc::clone(&self.rule));
        self.chain.push(key.to_owned());
    }
}

impl<F: ?Sized> Clone for Seam<F> {
    fn clone(&self) -> Self {
        Seam { rule: Arc::clone(&self.rule), chain: self.chain.clone() }
    }
}

impl<F: ?Sized> std::fmt::Debug for Seam<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Seam").field(&self.chain).finish()
    }
}

impl<F: ?Sized> PartialEq for Seam<F> {
    fn eq(&self, other: &Self) -> bool {
        self.chain == other.chain
    }
}

/// The makers of one seam's implementations, by key (see the module docs).
pub struct SeamRegistry<F: ?Sized> {
    makers: BTreeMap<String, Maker<F>>,
}

impl<F: ?Sized> Default for SeamRegistry<F> {
    fn default() -> Self {
        SeamRegistry { makers: BTreeMap::new() }
    }
}

impl<F: ?Sized> SeamRegistry<F> {
    /// Adds the implementation `key`. `false` (and nothing changes) when the key is [`ORIGINAL`] or
    /// already taken: an implementation is never replaced by name, so two mods cannot silently
    /// swap each other's rule.
    pub fn register(&mut self, key: &str, maker: Maker<F>) -> bool {
        if key == ORIGINAL || self.makers.contains_key(key) {
            return false;
        }
        self.makers.insert(key.to_owned(), maker);
        true
    }

    /// The maker of implementation `key`.
    pub fn get(&self, key: &str) -> Option<&Maker<F>> {
        self.makers.get(key)
    }

    /// Applies implementation `key` on top of `seam`; `false` (and `seam` unchanged) when no such
    /// implementation is registered.
    pub fn apply(&self, seam: &mut Seam<F>, key: &str) -> bool {
        match self.get(key) {
            Some(maker) => {
                seam.apply(key, maker);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Add = dyn Fn(i32) -> i32 + Send + Sync;

    #[test]
    fn makers_replace_or_extend_in_order() {
        let mut reg: SeamRegistry<Add> = SeamRegistry::default();
        assert!(reg.register("double", Arc::new(|prev: Arc<Add>| Arc::new(move |x| prev(x) * 2) as Arc<Add>)));
        assert!(reg.register("seven", Arc::new(|_| Arc::new(|_| 7) as Arc<Add>)));
        // The original's key and a taken key are refused.
        assert!(!reg.register(ORIGINAL, Arc::new(|p| p)));
        assert!(!reg.register("double", Arc::new(|p| p)));
        let mut seam: Seam<Add> = Seam::original(Arc::new(|x| x + 1));
        assert!(seam.is_original());
        assert_eq!(seam.rule()(1), 2);
        assert!(reg.apply(&mut seam, "double"));
        assert_eq!(seam.rule()(1), 4);
        assert!(!reg.apply(&mut seam, "missing"));
        assert!(reg.apply(&mut seam, "seven"));
        assert!(reg.apply(&mut seam, "double"));
        assert_eq!(seam.rule()(1), 14);
        assert_eq!(seam.chain(), ["original", "double", "seven", "double"]);
        assert!(!seam.is_original());
        assert_eq!(seam.clone(), seam);
    }
}

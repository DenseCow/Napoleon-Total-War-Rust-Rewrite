//! The pack precedence graph of `set_pack_file_precedence` / `set_pack_file_dependency`
//! (`g_PackPrecedenceGraph` 0x01768250 in `Napoleon.exe`; CONFIRMED by static trace except how a script line's pack name is matched to a pack, INFERRED;
//! `analysis/mods/MOD_LOADING.md` §2.1).
//!
//! Both commands add the ordered pair `(lhs, rhs)` (0x0109AA10 / 0x0109A910 → 0x0108E8F0); a
//! dependency is also kept in its own list. After every added pair the graph's **node order** is
//! rebuilt by a topological sort of all pairs (0x0105E8E0), so `lhs` always comes before `rhs`. A
//! pair that would close a cycle is dropped. When two packs that are both graph nodes hold the same
//! file (or DB row key), the one later in the node order wins (`VFS_ComparePackPrecedenceGraph`
//! 0x0108EBB0, used by `VFS_ShouldPackOverride` 0x0108ED00): the header types are not looked at.

/// What [`PackGraph::add_precedence`] / [`PackGraph::add_dependency`] did with a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairResult {
    /// The pair was added and the node order rebuilt.
    Added,
    /// The same pair was already there: nothing changes.
    ///
    /// ORIGINAL BUG (fixed): adding a pair that is already in the list erases it instead
    /// (0x0108E8F0 calls the erase 0x0106BBB0 on a found pair even when adding), so a repeated
    /// line silently cancels the pair at the next rebuild of the node order.
    AlreadyThere,
    /// The pair would close a cycle; it was dropped and the node order kept (0x0108E8F0: the
    /// sort 0x0105E8E0 returns no nodes, the pair is not stored, as in the original).
    WouldCloseCycle,
}

/// The precedence graph: pairs in the order the script added them, the dependency pairs and the
/// node order. Names are lowercase pack file names without folders (as the `mod` lines are).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackGraph {
    /// Every pair of both commands, `(lhs, rhs)`: lhs before rhs (graph+0x00).
    pairs: Vec<(String, String)>,
    /// The `set_pack_file_dependency` pairs (graph+0x10): `lhs` is mounted before `rhs`.
    dependencies: Vec<(String, String)>,
    /// The node order (graph+0x30), rebuilt after every added pair.
    order: Vec<String>,
}

impl PackGraph {
    /// An empty graph (no command in the script): the header types decide everywhere.
    pub fn new() -> Self {
        Self::default()
    }

    /// True when no pair was added.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// The node order, first to last (a later node wins over an earlier one).
    pub fn order(&self) -> &[String] {
        &self.order
    }

    /// `set_pack_file_precedence lhs rhs` (0x0109AA10 → 0x0108E8F0).
    pub fn add_precedence(&mut self, lhs: &str, rhs: &str) -> PairResult {
        let pair = (lhs.to_owned(), rhs.to_owned());
        if self.pairs.contains(&pair) {
            return PairResult::AlreadyThere;
        }
        let mut pairs = self.pairs.clone();
        pairs.push(pair);
        match topological_order(&pairs) {
            Some(order) => {
                self.pairs = pairs;
                self.order = order;
                PairResult::Added
            }
            None => PairResult::WouldCloseCycle,
        }
    }

    /// `set_pack_file_dependency lhs rhs` (0x0109A910 → 0x01064D00): the pair as with
    /// [`add_precedence`](Self::add_precedence), and, unless it was dropped, a dependency
    /// (`lhs` is mounted before `rhs`).
    pub fn add_dependency(&mut self, lhs: &str, rhs: &str) -> PairResult {
        let result = self.add_precedence(lhs, rhs);
        let pair = (lhs.to_owned(), rhs.to_owned());
        if result != PairResult::WouldCloseCycle && !self.dependencies.contains(&pair) {
            self.dependencies.push(pair);
        }
        result
    }

    /// The position of a pack (lowercase file name) in the node order, if it is a node.
    pub fn position(&self, name: &str) -> Option<usize> {
        self.order.iter().position(|n| n == name)
    }

    /// The precedence rule on two node positions (see [`position`](Self::position)): `Some(true)`
    /// if a copy from the `new` pack replaces one from the `old` pack, `Some(false)` if not, `None`
    /// when either pack is not a node (then the header types decide). The later node wins; the
    /// same pack replaces itself (0x0108EBB0 walks `[old, new]` sorted by node order and answers
    /// "replace" when it meets `old` first, which it does when both are one pack).
    pub fn replaces(old: Option<usize>, new: Option<usize>) -> Option<bool> {
        Some(old? <= new?)
    }

    /// The packs to mount for `name` in mount order (0x0109ECF0 as `VFS_MountPack` 0x01082690
    /// uses it): what `name` depends on through `set_pack_file_dependency`, transitively, in node
    /// order, then `name`. `Err(dependency)` when a pack it needs is in `excludes` (the original
    /// then refuses the pack: `VFS_MountPack` returns 6).
    pub fn mount_order(&self, name: &str, excludes: &[String]) -> Result<Vec<String>, String> {
        // The closure: every dependency of a listed pack is appended; the scan restarts from the
        // first entry after each append (0x0109EDE0 loop).
        let mut needed = vec![name.to_owned()];
        let mut i = 0;
        'scan: while i < needed.len() {
            for (lhs, rhs) in &self.dependencies {
                if *rhs == needed[i] && !needed.contains(lhs) {
                    if excludes.contains(lhs) {
                        return Err(lhs.clone());
                    }
                    needed.push(lhs.clone());
                    i = 0;
                    continue 'scan;
                }
            }
            i += 1;
        }
        // Graph nodes in node order, then the rest in closure order (0x0109EF85 on).
        let mut out: Vec<String> = self.order.iter().filter(|n| needed.contains(n)).cloned().collect();
        out.extend(needed.into_iter().filter(|n| !self.order.contains(n)));
        Ok(out)
    }
}

/// The node order of a list of pairs (0x0105E8E0, CONFIRMED): repeatedly take the first pair (in
/// list order) whose `lhs` is no pair's `rhs`, emit that `lhs` and remove all of its pairs,
/// remembering their `rhs` in first-seen order; when no pair is left, the remembered nodes not yet
/// emitted follow in that order. `None` if some pairs are left but every `lhs` is a `rhs` (a cycle).
fn topological_order(pairs: &[(String, String)]) -> Option<Vec<String>> {
    let mut pairs = pairs.to_vec();
    let (mut out, mut pending): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    while !pairs.is_empty() {
        let source = pairs.iter().find(|(lhs, _)| !pairs.iter().any(|(_, rhs)| rhs == lhs))?.0.clone();
        pending.retain(|n| *n != source);
        pairs.retain(|(lhs, rhs)| {
            if *lhs != source {
                return true;
            }
            if !pending.contains(rhs) {
                pending.push(rhs.clone());
            }
            false
        });
        out.push(source);
    }
    out.extend(pending);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    /// 0x0105E8E0: sources in pair order, the pairs' targets after them in first-seen order.
    #[test]
    fn node_order_follows_the_original_sort() {
        let mut g = PackGraph::new();
        assert_eq!(g.add_precedence("a", "b"), PairResult::Added);
        assert_eq!(g.add_precedence("c", "d"), PairResult::Added);
        assert_eq!(names(g.order()), ["a", "c", "b", "d"]);
        assert_eq!(g.add_precedence("b", "c"), PairResult::Added);
        assert_eq!(names(g.order()), ["a", "b", "c", "d"]);
        // Two unrelated nodes are still ordered: b and c compare by the graph, not by header type.
        assert_eq!(PackGraph::replaces(g.position("c"), g.position("b")), Some(false));
        assert_eq!(PackGraph::replaces(g.position("b"), g.position("c")), Some(true));
        assert_eq!(PackGraph::replaces(g.position("a"), None), None);
        assert_eq!(PackGraph::replaces(g.position("a"), g.position("a")), Some(true), "same pack");
    }

    /// 0x0108E8F0: a pair that closes a cycle is dropped and the order kept.
    #[test]
    fn a_pair_closing_a_cycle_is_dropped() {
        let mut g = PackGraph::new();
        g.add_precedence("a", "b");
        g.add_precedence("b", "c");
        let before = g.clone();
        assert_eq!(g.add_precedence("c", "a"), PairResult::WouldCloseCycle);
        assert_eq!(g.add_dependency("c", "a"), PairResult::WouldCloseCycle);
        assert_eq!(g.add_precedence("x", "x"), PairResult::WouldCloseCycle, "a pack before itself");
        assert_eq!(g, before);
    }

    /// ORIGINAL BUG fixed: a repeated pair stays (the original erases it).
    #[test]
    fn a_repeated_pair_is_kept() {
        let mut g = PackGraph::new();
        g.add_precedence("a", "b");
        assert_eq!(g.add_precedence("a", "b"), PairResult::AlreadyThere);
        g.add_precedence("c", "d");
        assert_eq!(names(g.order()), ["a", "c", "b", "d"], "a and b are still nodes");
    }

    /// 0x0109ECF0 for one pack: dependencies (transitively) in node order, then the pack.
    #[test]
    fn mount_order_pulls_dependencies_first() {
        let mut g = PackGraph::new();
        g.add_dependency("base", "mid");
        g.add_dependency("mid", "top");
        g.add_dependency("extra", "top");
        g.add_precedence("unrelated", "top");
        assert_eq!(names(g.order()), ["base", "mid", "extra", "unrelated", "top"]);
        assert_eq!(g.mount_order("top", &[]).unwrap(), ["base", "mid", "extra", "top"]);
        assert_eq!(g.mount_order("alone", &[]).unwrap(), ["alone"]);
        assert_eq!(g.mount_order("top", &["base".to_owned()]), Err("base".to_owned()));
    }
}

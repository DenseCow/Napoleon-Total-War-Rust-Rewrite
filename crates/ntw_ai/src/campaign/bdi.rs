//! The original's BDI pool: how behaviours, desires, goals and intentions get their priorities and
//! in which order they are worked on (`analysis/ai/AI_RESEARCH.md` §4 "BDI pool processing",
//! all CONFIRMED from `Napoleon.exe` unless tagged).
//!
//! * Every behaviour, desire, goal and intention is a **component** with a base priority, a
//!   jitter `r`, a total and a final priority `(1 + r) × total`. Components start in state 1
//!   with their dirty flag set (`0x00C7BC90`).
//! * A **link** `source → target` (`0x00CB2560`) adds `source final × mult + add` to the
//!   target's total (`0x00CEFC50`, `0x00D0CB60`); a changed total propagates to the targets of
//!   the component's own links (`0x00D0C760`). Links also keep transitive ancestor / descendant
//!   sets per link slot.
//! * The **jitter** is drawn when a component joins the pool (`0x00CB5CF0` / `0x00CB6AD0`):
//!   `r = rng.float_range(−j, j)` with `j` = `PRIORITY_RANDOMIZATION_DESIRE` for the desire list
//!   and `PRIORITY_RANDOMIZATION_INTENTION` for the intention list, **used raw** (10 and 0 in the
//!   shipped `default` personality); no jitter while the pool is loading.
//! * A **run** (`0x00D01910`) repeatedly picks the best desire ([`Pool::select`], `0x00D01B10`)
//!   and refreshes it (dirty) or deliberates it (state 0); only when no desire needs work does it
//!   pick and act on the best intention (`0x00D024D0`, same rule). At most 6000 steps per run.
//!   Priorities therefore only **order** the work: ancestors go before descendants, then state 0
//!   before 1, then the higher priority.
//!
//! The pool does not know what a component means; the caller's [`Deliberate`] implementation
//! does (the virtual slots 9 and 12 of the original's classes).

use ntw_sim::rng::CaRng;

/// Index of a component in its [`Pool`].
pub type CompId = usize;

/// Which pool list a component is on (`0x00CB2980` adds to the desire list, `0x00CB2C10` to the
/// intention list; behaviours, desires and goals are all on the desire list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    Desire,
    Intention,
}

/// Component state (`+0xF0`): 0 = to deliberate, 1 = deliberated, 2 = finished / failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Deliberate = 0,
    Active = 1,
    Finished = 2,
}

/// A priority link (`0x00C87B90`; the startpos record `CAI_BDI_COMPONENT_BLOCK_OWNS`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    pub source: CompId,
    pub add: f32,
    pub mult: f32,
    pub slot: usize,
}

/// One component.
#[derive(Debug, Clone)]
pub struct Component<P> {
    pub list: List,
    pub payload: P,
    base: f32,
    total: f32,
    r: f32,
    priority: f32,
    /// Incoming links (`+0x94/+0x98`), in creation order.
    links: Vec<Link>,
    /// Targets of this component's links (`+0xA8/+0xAC`), in creation order.
    targets: Vec<CompId>,
    /// Transitive ancestors / descendants per link slot (`+0x24 + 0x14k`, `+0x4C + 0x14k`).
    ancestors: [Vec<CompId>; 2],
    descendants: [Vec<CompId>; 2],
    pub state: State,
    pub dirty: bool,
    /// Removed from its list (dead).
    removed: bool,
}

impl<P> Component<P> {
    /// Final priority (`+0xEC`, getter slot 6).
    pub fn priority(&self) -> f32 {
        self.priority
    }
    /// Base + Σ link values (`+0x20`).
    pub fn total(&self) -> f32 {
        self.total
    }
    /// The jitter `r` (`+0xE8`).
    pub fn jitter(&self) -> f32 {
        self.r
    }
    pub fn links(&self) -> &[Link] {
        &self.links
    }
}

/// What the caller's classes do (the original's virtual slots).
pub trait Deliberate<P> {
    /// Virtual `+0x28`: false removes the component from its list.
    fn alive(&self, _pool: &Pool<P>, _id: CompId) -> bool {
        true
    }
    /// Slot 9 (`+0x24`), run for a dirty component: refresh its inputs. The default (and most
    /// classes) sets state 0 unless it is finished (e.g. `0x00D916B0`).
    fn refresh(&mut self, pool: &mut Pool<P>, id: CompId) {
        if pool.get(id).state != State::Finished {
            pool.set_state(id, State::Deliberate);
        }
    }
    /// Slot 12 (`+0x30`), run for a component in state 0: deliberate (desires, goals: create and
    /// link children) or act (intentions). Must leave the state at 1 or 2 (the originals call
    /// `0x00D0A0F0(1)` at the end); a component left in state 0 is deliberated again.
    fn deliberate(&mut self, pool: &mut Pool<P>, id: CompId);
}

/// Steps per run before the pool gives up (`0x00CB42A0`: `+0x74 < 6000`).
pub const MAX_STEPS: u32 = 6000;

/// A faction's BDI pool (vtable `0x013890BC`).
#[derive(Debug, Clone)]
pub struct Pool<P> {
    comps: Vec<Component<P>>,
    /// The desire and intention lists in insertion order (`+0x40/+0x44`, `+0x54/+0x58`).
    desires: Vec<CompId>,
    intentions: Vec<CompId>,
    /// `PRIORITY_RANDOMIZATION_DESIRE` / `_INTENTION` (raw personality values).
    pub jitter_desire: f32,
    pub jitter_intention: f32,
    /// `+0x80`: no jitter while set (pool construction and loading).
    pub loading: bool,
    /// Steps of the last run (`+0x74`).
    pub steps: u32,
}

impl<P> Pool<P> {
    pub fn new(jitter_desire: f32, jitter_intention: f32) -> Self {
        Pool { comps: Vec::new(), desires: Vec::new(), intentions: Vec::new(), jitter_desire, jitter_intention, loading: false, steps: 0 }
    }

    pub fn get(&self, id: CompId) -> &Component<P> {
        &self.comps[id]
    }

    pub fn payload(&self, id: CompId) -> &P {
        &self.comps[id].payload
    }

    pub fn len(&self) -> usize {
        self.comps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.comps.is_empty()
    }

    /// Every component in creation order.
    pub fn iter(&self) -> impl Iterator<Item = (CompId, &Component<P>)> {
        self.comps.iter().enumerate()
    }

    /// Adds a component (`0x00CB2980` / `0x00CB2C10`): the base ctor's state 1 + dirty, then the
    /// jitter draw (`0x00CB5CF0`: one `float_range(−j, j)` on `rng`, also when `j` is 0, unless
    /// loading) and `final = (1 + r) × total`.
    pub fn add(&mut self, list: List, base: f32, payload: P, rng: &mut CaRng) -> CompId {
        let id = self.comps.len();
        let j = if self.loading {
            0.0
        } else {
            match list {
                List::Desire => self.jitter_desire,
                List::Intention => self.jitter_intention,
            }
        };
        let r = if self.loading { 0.0 } else { rng.float_range(-j, j) };
        self.comps.push(Component {
            list,
            payload,
            base,
            total: base,
            r,
            priority: (r + 1.0) * base,
            links: Vec::new(),
            targets: Vec::new(),
            ancestors: [Vec::new(), Vec::new()],
            descendants: [Vec::new(), Vec::new()],
            state: State::Active,
            dirty: true,
            removed: false,
        });
        match list {
            List::Desire => self.desires.push(id),
            List::Intention => self.intentions.push(id),
        }
        id
    }

    /// Re-draws every desire's and intention's jitter (pool slot 11 `0x00D03D50`: desires first,
    /// then intentions, in list order; `final = (1 + r) × total`, no propagation).
    pub fn rejitter(&mut self, rng: &mut CaRng) {
        for list in [List::Desire, List::Intention] {
            let ids: Vec<CompId> = match list {
                List::Desire => self.desires.clone(),
                List::Intention => self.intentions.clone(),
            };
            let j = match list {
                List::Desire => self.jitter_desire,
                List::Intention => self.jitter_intention,
            };
            for id in ids {
                let j = if self.loading { 0.0 } else { j };
                let r = rng.float_range(-j, j);
                let c = &mut self.comps[id];
                c.r = r;
                c.priority = (r + 1.0) * c.total;
            }
        }
    }

    /// Is `x` a descendant of `of` through slot-`slot` links (`0x00CF1000(x, slot)` on `of`)?
    pub fn is_descendant(&self, x: CompId, of: CompId, slot: usize) -> bool {
        self.comps[of].descendants[slot].contains(&x)
    }

    /// `source` links to `target` (`0x00CB2560` on the source): a new link, or the existing link
    /// from the same source updated (`0x00D09C40`; the total is only recomputed when the link
    /// changed).
    pub fn link(&mut self, source: CompId, target: CompId, add: f32, mult: f32, slot: usize) {
        assert!(slot < 2);
        if let Some(l) = self.comps[target].links.iter_mut().find(|l| l.source == source) {
            if l.add != add || l.mult != mult {
                l.add = add;
                l.mult = mult;
                self.recompute(target);
            }
            return;
        }
        self.comps[target].links.push(Link { source, add, mult, slot });
        self.comps[source].targets.push(target);
        // Ancestor sets of the target and its descendants (`0x00CB26A0` / `0x00CB2440`).
        let mut new_anc = vec![source];
        new_anc.extend(self.comps[source].ancestors[slot].iter().copied());
        let mut downs = vec![target];
        downs.extend(self.comps[target].descendants[slot].iter().copied());
        for d in &downs {
            for &a in &new_anc {
                if !self.comps[*d].ancestors[slot].contains(&a) {
                    self.comps[*d].ancestors[slot].push(a);
                }
            }
        }
        // Descendant sets of the source and its ancestors (`0x00CB2320`).
        let mut ups = vec![source];
        ups.extend(self.comps[source].ancestors[slot].iter().copied());
        for u in &ups {
            for &d in &downs {
                if !self.comps[*u].descendants[slot].contains(&d) {
                    self.comps[*u].descendants[slot].push(d);
                }
            }
        }
        // `0x00CB26A0`: recompute when the new link's value is not 0.
        if self.link_value(Link { source, add, mult, slot }) != 0.0 {
            self.recompute(target);
        }
    }

    /// Removes `source`'s link to `target` (`0x00D047C0`) and recomputes the target. The
    /// ancestor sets are left as they are (PROVISIONAL: the original's list upkeep on removal is
    /// not decoded).
    pub fn unlink(&mut self, source: CompId, target: CompId) {
        let before = self.comps[target].links.len();
        self.comps[target].links.retain(|l| l.source != source);
        self.comps[source].targets.retain(|&t| t != target);
        if self.comps[target].links.len() != before {
            self.recompute(target);
        }
    }

    fn link_value(&self, l: Link) -> f32 {
        self.comps[l.source].priority * l.mult + l.add
    }

    /// `0x00D0CB60`: total = base + Σ link values (in link order, `f32`); on change, the final
    /// priority and every target of this component's links.
    fn recompute(&mut self, id: CompId) {
        let mut stack = vec![id];
        let mut guard = 0usize;
        while let Some(id) = stack.pop() {
            guard += 1;
            if guard > 100_000 {
                break; // link cycles: the original would recurse until the stack runs out
            }
            let mut t = self.comps[id].base;
            for l in &self.comps[id].links {
                t += self.link_value(*l);
            }
            let c = &mut self.comps[id];
            if t == c.total {
                continue;
            }
            c.total = t;
            c.priority = (c.r + 1.0) * t;
            for &tg in c.targets.iter().rev() {
                stack.push(tg);
            }
        }
    }

    /// Sets the state (`0x00D0A0F0`). State 2 drops the component's outgoing links (its
    /// children lose this source).
    pub fn set_state(&mut self, id: CompId, state: State) {
        if self.comps[id].state == state {
            return;
        }
        self.comps[id].state = state;
        if state == State::Finished {
            for t in self.comps[id].targets.clone() {
                self.unlink(id, t);
            }
        }
    }

    /// Marks a component for re-deliberation (`+0xF8`).
    pub fn mark_dirty(&mut self, id: CompId) {
        self.comps[id].dirty = true;
    }

    fn list_ids(&self, list: List) -> &Vec<CompId> {
        match list {
            List::Desire => &self.desires,
            List::Intention => &self.intentions,
        }
    }

    /// Does candidate `c` replace the best so far `b`? (`0x00D01B10`, CONFIRMED order of tests.)
    pub fn beats(&self, c: CompId, b: CompId) -> bool {
        let (cc, bb) = (&self.comps[c], &self.comps[b]);
        let c_refresh = cc.state == State::Active && cc.dirty;
        let b_refresh = bb.state == State::Active && bb.dirty;
        if c_refresh {
            if !b_refresh || self.is_descendant(b, c, 0) {
                return true;
            }
            if self.is_descendant(c, b, 0) {
                return false;
            }
            return cc.priority > bb.priority;
        }
        if b_refresh {
            return false;
        }
        if cc.state != bb.state {
            return cc.state < bb.state;
        }
        if cc.dirty != bb.dirty {
            return cc.dirty;
        }
        if self.is_descendant(b, c, 0) {
            return true;
        }
        if self.is_descendant(c, b, 0) {
            return false;
        }
        cc.priority > bb.priority
    }

    /// The best living component of `list` (dead ones are removed first, as the pass does).
    pub fn select<D: Deliberate<P> + ?Sized>(&mut self, list: List, d: &D) -> Option<CompId> {
        let ids = self.list_ids(list).clone();
        let mut keep = Vec::with_capacity(ids.len());
        let mut best: Option<CompId> = None;
        for id in ids {
            if !d.alive(self, id) {
                self.comps[id].removed = true;
                continue;
            }
            keep.push(id);
            best = match best {
                None => Some(id),
                Some(b) if self.beats(id, b) => Some(id),
                keep_b => keep_b,
            };
        }
        match list {
            List::Desire => self.desires = keep,
            List::Intention => self.intentions = keep,
        }
        best
    }

    /// One pass over `list` (`0x00D01B10` / `0x00D024D0`): refresh dirty winners until a
    /// winner in state 0 is deliberated (returns true) or nothing is left to do (false).
    fn pass<D: Deliberate<P> + ?Sized>(&mut self, list: List, d: &mut D) -> bool {
        loop {
            let Some(best) = self.select(list, d) else { return false };
            let c = &self.comps[best];
            if c.state == State::Finished {
                return false;
            }
            if c.dirty {
                // `0x00CCCF70`: dirty children first, then the component itself.
                let mut order: Vec<CompId> = self.comps[best].descendants[0]
                    .iter()
                    .copied()
                    .filter(|&k| self.comps[k].dirty && self.comps[k].state != State::Finished && !self.comps[k].removed)
                    .collect();
                order.push(best);
                for k in order {
                    self.comps[k].dirty = false;
                    d.refresh(self, k);
                }
                if !self.step() {
                    return false;
                }
                continue;
            }
            if c.state == State::Deliberate {
                d.deliberate(self, best);
                return true;
            }
            return false;
        }
    }

    /// Slot 16 (`0x00CB42A0`): count a step; false once [`MAX_STEPS`] is reached.
    fn step(&mut self) -> bool {
        self.steps += 1;
        self.steps < MAX_STEPS
    }

    /// A run (`0x00D01910`): desires until none needs work, then one intention, and again.
    pub fn run<D: Deliberate<P> + ?Sized>(&mut self, d: &mut D) {
        self.steps = 0;
        while self.step() {
            if self.pass(List::Desire, d) {
                continue;
            }
            if !self.step() {
                break;
            }
            if !self.pass(List::Intention, d) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_propagate_priorities() {
        let mut rng = CaRng::new(1);
        let mut p: Pool<&str> = Pool::new(0.0, 0.0);
        let b = p.add(List::Desire, 3000.0, "behaviour", &mut rng);
        let d = p.add(List::Desire, 0.0, "desire", &mut rng);
        p.link(b, d, 0.0, 0.5, 0);
        assert_eq!(p.get(d).priority(), 1500.0);
        let g = p.add(List::Desire, 0.0, "goal", &mut rng);
        p.link(d, g, 10.0, 2.0, 0);
        assert_eq!(p.get(g).priority(), 3010.0);
        // Updating the link re-propagates down the chain.
        p.link(b, d, 0.0, 1.0, 0);
        assert_eq!(p.get(g).priority(), 6010.0);
        assert!(p.is_descendant(g, b, 0) && !p.is_descendant(b, g, 0));
        p.set_state(d, State::Finished);
        assert_eq!(p.get(g).priority(), 0.0);
    }

    #[test]
    fn jitter_is_raw_and_drawn_on_add() {
        let mut rng = CaRng::new(7);
        let mut p: Pool<&str> = Pool::new(10.0, 0.0);
        let a = p.add(List::Desire, 100.0, "a", &mut rng);
        let mut check = CaRng::new(7);
        let r = check.float_range(-10.0, 10.0);
        assert_eq!(p.get(a).jitter(), r);
        assert_eq!(p.get(a).priority(), (r + 1.0) * 100.0);
        // Intentions draw too (with j = 0), so the RNG advances.
        let i = p.add(List::Intention, 5.0, "i", &mut rng);
        check.float_range(0.0, 0.0);
        assert_eq!(p.get(i).priority(), 5.0);
        assert_eq!(rng.state, check.state);
    }

    #[test]
    fn run_orders_by_priority_children_after_parents() {
        struct Spawn(Vec<&'static str>);
        impl Deliberate<&'static str> for Spawn {
            fn deliberate(&mut self, pool: &mut Pool<&'static str>, id: CompId) {
                let name = *pool.payload(id);
                self.0.push(name);
                if name == "low" {
                    let mut rng = CaRng::new(3);
                    let child = pool.add(List::Desire, 0.0, "child of low", &mut rng);
                    pool.link(id, child, 100.0, 1.0, 0);
                }
                pool.set_state(id, State::Active);
            }
        }
        let mut rng = CaRng::new(1);
        let mut p: Pool<&str> = Pool::new(0.0, 0.0);
        p.add(List::Intention, 1.0, "intention", &mut rng);
        p.add(List::Desire, 10.0, "low", &mut rng);
        p.add(List::Desire, 50.0, "high", &mut rng);
        let mut log = Spawn(Vec::new());
        p.run(&mut log);
        // Higher priority first; the child (110) is deliberated as soon as it exists; every
        // desire before the intention.
        assert_eq!(log.0, vec!["high", "low", "child of low", "intention"]);
        // The ancestry rule beats priority in the pairwise test.
        let child = 3;
        p.mark_dirty(child);
        p.mark_dirty(1);
        assert!(p.beats(1, child), "a dirty ancestor goes before its dirty descendant");
        assert!(!p.beats(child, 1));
    }
}

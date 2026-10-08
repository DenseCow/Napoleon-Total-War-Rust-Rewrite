//! The battlegroup's **tactic auction** (`0x00751800`, battlegroup vtable slot 4; CONFIRMED,
//! `analysis/ai/AI_RESEARCH.md` §3.1b / §3.1d).
//!
//! A battlegroup owns one instance of each tactic, created in this order (`0x0070A980`):
//! ATTACK_BATTLEGROUP, OUTFLANK, DOUBLE_ENVELOPMENT, STOP_AND_SHOOT, LIMBERED_ARTILLERY,
//! GENERAL_SUPPORT. Its units sit in a list of `{unit, owning tactic, used}` entries. Each planner
//! step:
//! 1. every tactic is reset; an active tactic whose **keep** test fails gives its units back and
//!    is deactivated;
//! 2. while some inactive tactic is still in the running: every inactive tactic the battlegroup
//!    **accepts** is scored on the free units; the highest score wins (ties: the earlier tactic);
//!    a zero score drops that tactic out; the winner **claims** units and is activated when its
//!    activation test passes;
//! 3. tactics that were already active top up from the free units (their own claim rule);
//! 4. the default tactic takes what is left;
//! 5. a tactic left without units is deactivated.
//!
//! The battlegroup accepts a tactic only while no tactic it excludes is active (type pairs at
//! `0x01453450`: OUTFLANK 14 / DOUBLE_ENVELOPMENT 15 / STOP_AND_SHOOT 16 exclude each other,
//! `0x0075DD00`).

use std::collections::BTreeSet;

/// The tactics of a battlegroup, in creation order (ties in the auction go to the earlier one).
/// The `u32` is the tactic's type id (virtual `+0x54`, CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// ATTACK_BATTLEGROUP (type 6).
    AttackBattlegroup,
    /// OUTFLANK (type 14).
    Outflank,
    /// DOUBLE_ENVELOPMENT (type 15).
    DoubleEnvelopment,
    /// STOP_AND_SHOOT (type 16).
    StopAndShoot,
    /// LIMBERED_ARTILLERY (type 18).
    LimberedArtillery,
}

impl Kind {
    /// Creation order (`0x0070A980`). GENERAL_SUPPORT (last) is not modelled.
    pub const ORDER: [Kind; 5] =
        [Kind::AttackBattlegroup, Kind::Outflank, Kind::DoubleEnvelopment, Kind::StopAndShoot, Kind::LimberedArtillery];

    /// The type id (`+0x54` getters `0x004CDC90` / `0x007CF2A0` / `0x007CF300` / `0x007A9120` /
    /// `0x007CF310`).
    pub fn type_id(self) -> u32 {
        match self {
            Kind::AttackBattlegroup => 6,
            Kind::Outflank => 14,
            Kind::DoubleEnvelopment => 15,
            Kind::StopAndShoot => 16,
            Kind::LimberedArtillery => 18,
        }
    }
}

/// `0x0075DD00`: `a` is not accepted while `b` is active (pairs (14, 15), (14, 16), (15, 16)).
pub fn excludes(a: Kind, b: Kind) -> bool {
    const PAIRS: [(u32, u32); 3] = [(14, 15), (14, 16), (15, 16)];
    let (x, y) = (a.type_id(), b.type_id());
    x != y && PAIRS.iter().any(|&(p, q)| (p == x && q == y) || (p == y && q == x))
}

/// The commit rules of the accept test (`0x007CD4B0`, CONFIRMED round 6): a tactic that started
/// (`+0x4A`) in the current phase (`+0x44` = the battlegroup's `+0x48`, `0x00794830`) is refused
/// unless its slot 14 holds (ATTACK_BATTLEGROUP, LIMBERED_ARTILLERY); OUTFLANK is refused while
/// a DOUBLE_ENVELOPMENT has started and vice versa (`0x0075DD90`; the pair table `0x01453450`
/// yields only (14, 15), the other entry reads past the table into non-type values).
pub fn commit_allows(tactics: &[Tactic], i: usize, phase: u8) -> bool {
    let t = &tactics[i];
    let slot14 = matches!(t.kind, Kind::AttackBattlegroup | Kind::LimberedArtillery);
    if t.committed == Some(phase) && !slot14 {
        return false;
    }
    let partner = match t.kind {
        Kind::Outflank => Some(Kind::DoubleEnvelopment),
        Kind::DoubleEnvelopment => Some(Kind::Outflank),
        _ => None,
    };
    !tactics.iter().enumerate().any(|(j, o)| j != i && Some(o.kind) == partner && o.committed.is_some())
}

/// How a tactic takes units when it wins (slot 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// Takes no units (STOP_AND_SHOOT only raises its activation flag, `0x00752270`).
    None,
    /// Every free unit its filter passes (ATTACK_BATTLEGROUP `0x00752230`, LIMBERED_ARTILLERY
    /// `0x00752350`).
    All,
    /// `n` units by filter preference (`0x0075CCC0`: the free units whose filter gives 2,
    /// scanning the list from the back, then those giving 1).
    Count(usize),
}

/// One tactic of the battlegroup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tactic {
    /// Which tactic.
    pub kind: Kind,
    /// `+0x4B`.
    pub active: bool,
    /// The units it owns.
    pub units: BTreeSet<u32>,
    /// `+0x4A` with `+0x44`: the encounter phase in which the tactic started (`0x00754780`, run
    /// by OUTFLANK's and DOUBLE_ENVELOPMENT's start). No writer clears it (INFERRED: it stays).
    pub committed: Option<u8>,
}

/// The tactic-specific virtuals the auction calls.
pub trait Bidder {
    /// Slot 1, the unit filter: 0 = not usable, 1 = usable, 2 = preferred.
    fn filter(&self, kind: Kind, unit: u32) -> u8;
    /// Slot 15, the score on the free units (0 = not wanted).
    fn score(&mut self, kind: Kind, free: &[u32]) -> f32;
    /// Slot 13, the keep test of an active tactic on its units.
    fn keeps(&mut self, kind: Kind, owned: &BTreeSet<u32>) -> bool;
    /// Slot 16, the claim rule for an inactive (`active == false`) or already active tactic.
    fn claim(&mut self, kind: Kind, active: bool, free: &[u32]) -> Claim;
    /// Slot 9, the activation test after a claim (default: it owns units, `0x007CB3F0`).
    fn activates(&mut self, kind: Kind, owned: &BTreeSet<u32>) -> bool {
        let _ = kind;
        !owned.is_empty()
    }
    /// Slot 10 then 12 (`0x007CB840`): deactivated after the round (default: no units left and
    /// the tactic claims units).
    fn drops(&mut self, kind: Kind, owned: &BTreeSet<u32>) -> bool {
        kind != Kind::StopAndShoot && owned.is_empty()
    }
    /// The battlegroup's encounter phase (`+0x48`) for the commit rules.
    fn phase(&self) -> u8 {
        0
    }
}

/// Picks `n` of the free units by filter preference (`0x0075CCC0`, CONFIRMED order).
fn pick(free: &[u32], n: usize, filter: impl Fn(u32) -> u8) -> Vec<u32> {
    let n = n.min(free.len());
    let mut out = Vec::new();
    for want in [2u8, 1] {
        for &u in free.iter().rev() {
            if out.len() == n {
                return out;
            }
            if !out.contains(&u) && filter(u) == want {
                out.push(u);
            }
        }
    }
    out
}

fn take(tactics: &mut [Tactic], i: usize, claim: Claim, free: &mut Vec<u32>, bidder: &dyn Fn(Kind, u32) -> u8) {
    let kind = tactics[i].kind;
    let got: Vec<u32> = match claim {
        Claim::None => Vec::new(),
        Claim::All => free.iter().copied().filter(|&u| bidder(kind, u) != 0).collect(),
        Claim::Count(n) => pick(free, n, |u| bidder(kind, u)),
    };
    free.retain(|u| !got.contains(u));
    tactics[i].units.extend(got);
}

/// Runs one auction (`0x00751800`) over `units` (the battlegroup's unit list, in order). Units
/// gone from `units` are dropped from their tactics first. `default` is the battlegroup's default
/// tactic, which takes the units left over. Returns the tactics newly activated this round.
pub fn run(tactics: &mut [Tactic], units: &[u32], default: Kind, bidder: &mut dyn Bidder) -> Vec<Kind> {
    for t in tactics.iter_mut() {
        t.units.retain(|u| units.contains(u));
    }
    // The default tactic takes the leftovers without marking them used (INFERRED: otherwise it
    // would hold every unit for good and no other tactic could ever bid), so they are free.
    for t in tactics.iter_mut().filter(|t| t.kind == default) {
        t.units.clear();
    }
    // 1. Keep tests.
    for t in tactics.iter_mut() {
        if t.active && !bidder.keeps(t.kind, &t.units) {
            t.units.clear();
            t.active = false;
        }
    }
    let mut free: Vec<u32> = units.iter().copied().filter(|u| !tactics.iter().any(|t| t.units.contains(u))).collect();
    let active_at_start: Vec<usize> = (0..tactics.len()).filter(|&i| tactics[i].active).collect();
    let mut newly = Vec::new();
    // 2. Competition by score.
    let mut running = tactics.iter().filter(|t| !t.active).count() as i32;
    while running > 0 {
        let mut best: Option<(usize, f32)> = None;
        for i in 0..tactics.len() {
            let t = &tactics[i];
            if t.active {
                continue;
            }
            let blocked = tactics.iter().any(|o| o.active && excludes(t.kind, o.kind));
            if blocked || !commit_allows(tactics, i, bidder.phase()) {
                continue;
            }
            let s = bidder.score(t.kind, &free);
            if s > best.map_or(0.0, |b| b.1) {
                best = Some((i, s));
            } else if s == 0.0 {
                running -= 1;
            }
        }
        let Some((i, _)) = best else { break };
        let kind = tactics[i].kind;
        let claim = bidder.claim(kind, false, &free);
        {
            let bf = |k: Kind, u: u32| bidder.filter(k, u);
            take(tactics, i, claim, &mut free, &bf);
        }
        running -= 1;
        if bidder.activates(kind, &tactics[i].units) {
            tactics[i].active = true;
            if matches!(kind, Kind::Outflank | Kind::DoubleEnvelopment) {
                tactics[i].committed = Some(bidder.phase());
            }
            newly.push(kind);
        }
    }
    // 3. Tactics active before the round top up.
    for i in active_at_start {
        let kind = tactics[i].kind;
        let claim = bidder.claim(kind, true, &free);
        let bf = |k: Kind, u: u32| bidder.filter(k, u);
        take(tactics, i, claim, &mut free, &bf);
    }
    // 4. The default tactic takes the rest.
    if let Some(i) = tactics.iter().position(|t| t.kind == default) {
        let kind = tactics[i].kind;
        let active = tactics[i].active;
        let claim = bidder.claim(kind, active, &free);
        let bf = |k: Kind, u: u32| bidder.filter(k, u);
        take(tactics, i, claim, &mut free, &bf);
        if !tactics[i].units.is_empty() {
            tactics[i].active = true;
        }
    }
    // 5. Tactics without units drop out.
    for t in tactics.iter_mut() {
        if t.active && bidder.drops(t.kind, &t.units) {
            t.active = false;
        }
    }
    newly
}

/// A fresh set of the modelled tactics, all inactive.
pub fn new_tactics() -> Vec<Tactic> {
    Kind::ORDER.iter().map(|&kind| Tactic { kind, active: false, units: BTreeSet::new(), committed: None }).collect()
}

/// OUTFLANK's claim (`0x007523F0`): `n = free / 2`; when `n > 1`, `seed0 % n + 1` units.
pub fn outflank_claim(free: usize, seed0: u32) -> Claim {
    let n = free / 2;
    if n > 1 { Claim::Count((seed0 % n as u32) as usize + 1) } else { Claim::None }
}

/// DOUBLE_ENVELOPMENT's claim (`0x007522A0`): `n = min(free / 2 + 1, free units its filter
/// passes)`; when `n > 1`, `seed0 % (n − 1) + 2` units.
pub fn double_envelopment_claim(free: usize, eligible: usize, seed0: u32) -> Claim {
    let n = (free / 2 + 1).min(eligible);
    if n > 1 { Claim::Count((seed0 % (n as u32 - 1)) as usize + 2) } else { Claim::None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A bidder with fixed scores, keep answers and filters.
    struct Fixed {
        scores: BTreeMap<Kind, f32>,
        keep: BTreeMap<Kind, bool>,
        filters: BTreeMap<(Kind, u32), u8>,
        claims: BTreeMap<Kind, Claim>,
    }

    impl Bidder for Fixed {
        fn filter(&self, kind: Kind, unit: u32) -> u8 {
            *self.filters.get(&(kind, unit)).unwrap_or(&1)
        }
        fn score(&mut self, kind: Kind, free: &[u32]) -> f32 {
            if free.is_empty() && kind != Kind::StopAndShoot { 0.0 } else { *self.scores.get(&kind).unwrap_or(&0.0) }
        }
        fn keeps(&mut self, kind: Kind, _owned: &BTreeSet<u32>) -> bool {
            *self.keep.get(&kind).unwrap_or(&true)
        }
        fn claim(&mut self, kind: Kind, active: bool, _free: &[u32]) -> Claim {
            match kind {
                Kind::AttackBattlegroup | Kind::LimberedArtillery => Claim::All,
                _ if active => Claim::None,
                _ => *self.claims.get(&kind).unwrap_or(&Claim::None),
            }
        }
        fn activates(&mut self, kind: Kind, owned: &BTreeSet<u32>) -> bool {
            kind == Kind::StopAndShoot || !owned.is_empty()
        }
    }

    fn fixed() -> Fixed {
        Fixed { scores: BTreeMap::new(), keep: BTreeMap::new(), filters: BTreeMap::new(), claims: BTreeMap::new() }
    }

    #[test]
    fn highest_score_claims_first_and_default_takes_the_rest() {
        let mut b = fixed();
        b.scores.insert(Kind::AttackBattlegroup, 1.0);
        b.scores.insert(Kind::Outflank, 120.0);
        b.claims.insert(Kind::Outflank, Claim::Count(2));
        // Units 3 and 5 are preferred by OUTFLANK; it scans from the back.
        b.filters.insert((Kind::Outflank, 3), 2);
        b.filters.insert((Kind::Outflank, 5), 2);
        b.filters.insert((Kind::Outflank, 1), 0);
        let mut t = new_tactics();
        let newly = run(&mut t, &[1, 2, 3, 4, 5, 6], Kind::AttackBattlegroup, &mut b);
        assert_eq!(newly, vec![Kind::Outflank, Kind::AttackBattlegroup]);
        assert_eq!(t[1].units, BTreeSet::from([3, 5]));
        assert_eq!(t[0].units, BTreeSet::from([1, 2, 4, 6]));
    }

    #[test]
    fn exclusions_and_keep_failure() {
        let mut b = fixed();
        b.scores.insert(Kind::StopAndShoot, 164.0);
        b.scores.insert(Kind::Outflank, 150.0);
        b.scores.insert(Kind::DoubleEnvelopment, 100.0);
        b.claims.insert(Kind::Outflank, Claim::Count(1));
        let mut t = new_tactics();
        run(&mut t, &[1, 2, 3], Kind::AttackBattlegroup, &mut b);
        // STOP_AND_SHOOT wins (164) and then shuts out OUTFLANK and DOUBLE_ENVELOPMENT.
        assert!(t[3].active && !t[1].active && !t[2].active);
        assert_eq!(t[0].units.len(), 3);
        // Its keep test fails: OUTFLANK can bid again.
        b.keep.insert(Kind::StopAndShoot, false);
        b.scores.insert(Kind::StopAndShoot, 0.0);
        run(&mut t, &[1, 2, 3], Kind::AttackBattlegroup, &mut b);
        assert!(!t[3].active);
        assert!(t[1].active, "{t:?}");
        assert_eq!(t[1].units, BTreeSet::from([3]));
    }

    #[test]
    fn committed_tactics_wait_for_a_new_phase_and_shut_out_their_partner() {
        let mut b = fixed();
        b.scores.insert(Kind::Outflank, 150.0);
        b.scores.insert(Kind::DoubleEnvelopment, 100.0);
        b.claims.insert(Kind::Outflank, Claim::Count(1));
        b.claims.insert(Kind::DoubleEnvelopment, Claim::Count(2));
        let mut t = new_tactics();
        run(&mut t, &[1, 2, 3], Kind::AttackBattlegroup, &mut b);
        assert!(t[1].active);
        assert_eq!(t[1].committed, Some(0));
        // OUTFLANK lets go; in the same phase it may not start again, and DOUBLE_ENVELOPMENT is
        // shut out by OUTFLANK's commit.
        b.keep.insert(Kind::Outflank, false);
        run(&mut t, &[1, 2, 3], Kind::AttackBattlegroup, &mut b);
        assert!(!t[1].active && !t[2].active, "{t:?}");
        assert!(!commit_allows(&t, 1, 0) && commit_allows(&t, 1, 4) && !commit_allows(&t, 2, 4));
    }

    #[test]
    fn ties_go_to_the_earlier_tactic() {
        let mut b = fixed();
        b.scores.insert(Kind::Outflank, 164.0);
        b.scores.insert(Kind::StopAndShoot, 164.0);
        b.claims.insert(Kind::Outflank, Claim::Count(1));
        let mut t = new_tactics();
        run(&mut t, &[1, 2, 3], Kind::AttackBattlegroup, &mut b);
        assert!(t[1].active && !t[3].active);
    }

    #[test]
    fn claim_counts() {
        assert_eq!(outflank_claim(3, 7), Claim::None);
        assert_eq!(outflank_claim(4, 7), Claim::Count(2)); // n = 2: 7 % 2 + 1
        assert_eq!(outflank_claim(9, 10), Claim::Count(3)); // n = 4: 10 % 4 + 1
        assert_eq!(double_envelopment_claim(6, 6, 5), Claim::Count(4)); // n = 4: 5 % 3 + 2
        assert_eq!(double_envelopment_claim(6, 1, 5), Claim::None);
        assert_eq!(double_envelopment_claim(2, 2, 9), Claim::Count(2)); // n = 2: 9 % 1 + 2
    }

    #[test]
    fn exclusion_pairs() {
        assert!(excludes(Kind::Outflank, Kind::DoubleEnvelopment));
        assert!(excludes(Kind::StopAndShoot, Kind::Outflank));
        assert!(excludes(Kind::DoubleEnvelopment, Kind::StopAndShoot));
        assert!(!excludes(Kind::AttackBattlegroup, Kind::Outflank));
        assert!(!excludes(Kind::LimberedArtillery, Kind::StopAndShoot));
    }
}

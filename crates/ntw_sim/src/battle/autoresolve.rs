//! Autoresolve kill rates and the engagement loop.
//!
//! - [`kill_rates`] follows W1 §12.7, `0x0078D200` (CONFIRMED formula).
//! - [`AutoresolveTweaks::default`] holds the CONFIRMED tweaker defaults from the executable.
//! - [`engage`] / [`engagement`] are the exact port of `0x00759860` (CONFIRMED, shared with the
//!   campaign's stat-based resolver).

/// Autoresolve tweakers. These numbers are CONFIRMED defaults compiled into the executable
/// (W1 §12.7 / `tweakers.tsv`), not table data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoresolveTweaks {
    /// `Kmel`: melee kill-rate constant.
    pub k_melee: f32,
    /// `Kmis`: missile kill-rate constant.
    pub k_missile: f32,
    /// `Mmel`: melee outnumbering multiplier.
    pub m_melee: f32,
    /// `Mmis`: missile outnumbering multiplier.
    pub m_missile: f32,
    /// `Mnav`: naval multiplier (not used by the land formula).
    pub m_naval: f32,
    /// `landKillMult`: scales per-step land losses.
    pub land_kill_mult: f32,
    /// Random fuzz amount.
    pub fuzz: f32,
    /// Base rout point.
    pub rout: f32,
    /// Shaken point.
    pub shaken: f32,
}

impl Default for AutoresolveTweaks {
    /// W1 §12.7 (CONFIRMED): Kmel 0.2, Kmis 0.2, Mmel 0, Mmis 0, Mnav 2, landKillMult 0.1,
    /// fuzz 0.2, rout 0.4, shaken 0.9.
    fn default() -> Self {
        AutoresolveTweaks {
            k_melee: 0.2,
            k_missile: 0.2,
            m_melee: 0.0,
            m_missile: 0.0,
            m_naval: 2.0,
            land_kill_mult: 0.1,
            fuzz: 0.2,
            rout: 0.4,
            shaken: 0.9,
        }
    }
}

/// The autoresolve query record (`0x0078D200`'s argument). W1 §8 "Autoresolve query".
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AutoresolveQuery {
    /// Unit A starting men (`+0x08 → +0xC8`).
    pub start_men_a: f32,
    /// Unit B starting men (`+0x0C → +0xC8`).
    pub start_men_b: f32,
    /// Current men A (`+0x18`).
    pub men_a: f32,
    /// Current men B (`+0x1C`).
    pub men_b: f32,
    /// Signed outnumbering ratio `r` (`+0x24`).
    pub r: f32,
    /// Signed missile modifier `r2` (`+0x28`).
    pub r2: f32,
    /// Melee potential A (`+0x30`).
    pub melee_a: f32,
    /// Melee potential B (`+0x34`).
    pub melee_b: f32,
    /// Missile potential A (`+0x38`).
    pub missile_a: f32,
    /// Missile potential B (`+0x3C`).
    pub missile_b: f32,
}

/// The four kill rates produced by [`kill_rates`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct KillRates {
    /// Melee kill rate of A (against B).
    pub melee_a: f32,
    /// Melee kill rate of B (against A).
    pub melee_b: f32,
    /// Missile kill rate of A.
    pub missile_a: f32,
    /// Missile kill rate of B.
    pub missile_b: f32,
}

/// `pot(P, mult, men, start) = P*mult*men/(start*start)` (without the zero check). W1 §12.7.
fn pot_raw(p: f32, mult: f32, men: f32, start: f32) -> f32 {
    p * mult * men / (start * start)
}

/// The zero check: a potential of exactly 0 becomes 1. W1 §12.7 (CONFIRMED).
fn nonzero(p: f32) -> f32 {
    if p == 0.0 { 1.0 } else { p }
}

/// Kill rates, `0x0078D200`. W1 §12.7 (CONFIRMED):
/// ```text
/// multA = r<0 ? 1+M*|r| : 1 ;  multB = r>0 ? 1+M*r : 1
/// pA = pot(PA, multA, menA, startA) ; pB = pot(PB, multB, menB, startB)
/// killA = K*(pA/pB)*(r<=0 ? 1 : 1-|r|) ; killB = K*(pB/pA)*(r>=0 ? 1 : 1-|r|)
/// ```
/// Missile uses the same with `Mmis`/`Kmis`, and before the zero check:
/// `r2<0 → pA *= 1-|r2|`, `r2>0 → pB *= 1-r2`.
pub fn kill_rates(q: &AutoresolveQuery, t: &AutoresolveTweaks) -> KillRates {
    let r = q.r;
    let a_factor = if r <= 0.0 { 1.0 } else { 1.0 - r.abs() };
    let b_factor = if r >= 0.0 { 1.0 } else { 1.0 - r.abs() };

    // Melee.
    let mult_a = if r < 0.0 {
        1.0 + t.m_melee * r.abs()
    } else {
        1.0
    };
    let mult_b = if r > 0.0 { 1.0 + t.m_melee * r } else { 1.0 };
    let pa = nonzero(pot_raw(q.melee_a, mult_a, q.men_a, q.start_men_a));
    let pb = nonzero(pot_raw(q.melee_b, mult_b, q.men_b, q.start_men_b));
    let melee_a = t.k_melee * (pa / pb) * a_factor;
    let melee_b = t.k_melee * (pb / pa) * b_factor;

    // Missile.
    let mult_a = if r < 0.0 {
        1.0 + t.m_missile * r.abs()
    } else {
        1.0
    };
    let mult_b = if r > 0.0 { 1.0 + t.m_missile * r } else { 1.0 };
    let mut pa = pot_raw(q.missile_a, mult_a, q.men_a, q.start_men_a);
    let mut pb = pot_raw(q.missile_b, mult_b, q.men_b, q.start_men_b);
    if q.r2 < 0.0 {
        pa *= 1.0 - q.r2.abs();
    }
    if q.r2 > 0.0 {
        pb *= 1.0 - q.r2;
    }
    let pa = nonzero(pa);
    let pb = nonzero(pb);
    let missile_a = t.k_missile * (pa / pb) * a_factor;
    let missile_b = t.k_missile * (pb / pa) * b_factor;

    KillRates {
        melee_a,
        melee_b,
        missile_a,
        missile_b,
    }
}

/// The engagement loop `0x00759860` (CONFIRMED). There is one port of it, 0-B's
/// [`crate::campaign::autoresolve::engagement`]; these names re-export it for the battle side.
/// It replaced our earlier simplified loop, which differed from the exe in four ways: it fuzzed
/// the rates itself (the original fuzzes the 64 pair queries before the loop), it summed melee and
/// missile (the original uses one of the two), it had no range pre-phase and no shaken rule.
pub use crate::campaign::autoresolve::{engagement, PairResult, PairSample};

/// [`engagement`] with the four rates of a [`KillRates`]. `rout_a` / `rout_b` are the men left
/// when each side breaks; `level` > 0 lets A fire first, < 0 lets B fire first.
pub fn engage(
    rates: &KillRates,
    rout_a: f32,
    rout_b: f32,
    men_a: u32,
    men_b: u32,
    level: i32,
) -> PairSample {
    engagement(
        rates.melee_a,
        rates.missile_a,
        rout_a,
        rates.melee_b,
        rates.missile_b,
        rout_b,
        men_a,
        men_b,
        level,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tweak_defaults_are_the_confirmed_numbers() {
        let t = AutoresolveTweaks::default();
        assert_eq!(
            (t.k_melee, t.k_missile, t.m_melee, t.m_missile, t.m_naval),
            (0.2, 0.2, 0.0, 0.0, 2.0)
        );
        assert_eq!(
            (t.land_kill_mult, t.fuzz, t.rout, t.shaken),
            (0.1, 0.2, 0.4, 0.9)
        );
    }

    /// Made-up query (NOT game data).
    fn query() -> AutoresolveQuery {
        AutoresolveQuery {
            start_men_a: 100.0,
            start_men_b: 100.0,
            men_a: 100.0,
            men_b: 100.0,
            r: 0.0,
            r2: 0.0,
            melee_a: 200.0,
            melee_b: 100.0,
            missile_a: 0.0,
            missile_b: 0.0,
        }
    }

    #[test]
    fn equal_r_melee() {
        let k = kill_rates(&query(), &AutoresolveTweaks::default());
        // pA = 200*1*100/10000 = 2, pB = 1 -> killA = 0.2*2 = 0.4, killB = 0.2*0.5 = 0.1
        assert!((k.melee_a - 0.4).abs() < 1e-6);
        assert!((k.melee_b - 0.1).abs() < 1e-6);
        // Both missile potentials are 0 -> replaced by 1 -> 0.2 each.
        assert!((k.missile_a - 0.2).abs() < 1e-6);
        assert!((k.missile_b - 0.2).abs() < 1e-6);
    }

    #[test]
    fn outnumbering_ratio_sign() {
        let t = AutoresolveTweaks {
            m_melee: 1.0,
            ..Default::default()
        };
        // r > 0: B gets the multiplier, A's kill rate gets (1-|r|).
        let q = AutoresolveQuery { r: 0.5, ..query() };
        let k = kill_rates(&q, &t);
        // pA = 2, pB = 1 * 1.5 = 1.5; killA = 0.2*(2/1.5)*0.5 ; killB = 0.2*(1.5/2)*1
        assert!((k.melee_a - 0.2 * (2.0 / 1.5) * 0.5).abs() < 1e-6);
        assert!((k.melee_b - 0.2 * 0.75).abs() < 1e-6);
        // r < 0: mirror image.
        let q = AutoresolveQuery { r: -0.5, ..query() };
        let k = kill_rates(&q, &t);
        // pA = 2*1.5 = 3, pB = 1; killA = 0.2*3 ; killB = 0.2*(1/3)*0.5
        assert!((k.melee_a - 0.6).abs() < 1e-6);
        assert!((k.melee_b - 0.2 / 3.0 * 0.5).abs() < 1e-6);
    }

    #[test]
    fn missile_r2_applies_before_zero_check() {
        let q = AutoresolveQuery {
            missile_a: 100.0,
            missile_b: 100.0,
            r2: -0.5,
            ..query()
        };
        let k = kill_rates(&q, &AutoresolveTweaks::default());
        // pA = 1 * 0.5 = 0.5, pB = 1 -> missile_a = 0.1, missile_b = 0.4
        assert!((k.missile_a - 0.1).abs() < 1e-6);
        assert!((k.missile_b - 0.4).abs() < 1e-6);
        // r2 = 1 makes pB exactly 0, which the zero check turns into 1.
        let q = AutoresolveQuery {
            missile_a: 100.0,
            missile_b: 100.0,
            r2: 1.0,
            ..query()
        };
        let k = kill_rates(&q, &AutoresolveTweaks::default());
        assert!((k.missile_a - 0.2).abs() < 1e-6);
    }

    #[test]
    fn engage_is_the_exe_loop() {
        let t = AutoresolveTweaks::default();
        let k = kill_rates(&query(), &t);
        // Made-up numbers: A's rates are higher, so B breaks; rout points are 40 % of the men.
        let e = engage(&k, 40.0, 40.0, 100, 100, 0);
        assert_eq!(
            e,
            engagement(k.melee_a, k.missile_a, 40.0, k.melee_b, k.missile_b, 40.0, 100, 100, 0)
        );
        assert_eq!(e.result, PairResult::AWins);
        assert!(e.cas_b >= 0.6 && e.cas_a < e.cas_b);
    }
}

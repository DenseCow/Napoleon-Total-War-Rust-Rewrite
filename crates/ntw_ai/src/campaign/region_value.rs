//! The campaign AI's **region value** (`analysis/ai/AI_RESEARCH.md` §4 "Region value"), CONFIRMED
//! formulas.
//!
//! * The base is belief 0x4D (`0x00A75560`, ctor `0x00A40DF0`, update `0x00ABD5E0`): from three
//!   integer fields `a = +0xE8`, `b = +0xBC`, `c = +0xCC` of the region's `+0x1A0` object (INFERRED
//!   the settlement; the fields' meanings are UNKNOWN):
//!   `15000 + 25 × (floor(0.12 × max(0, a − b)) + trunc(0.2 × (c + b)))`.
//! * The composite value analyser 0x56 (`0x00AB8BC0`) scales it by the
//!   `COMPOSITE_VALUE_ANALYSER_*` personality multipliers by the region group's change state and
//!   the region's loss likelihood, ×5 for the faction's capital, ×3 for an UNKNOWN test and
//!   +5000 for another (see [`Composite`]).

/// `0x00ABD5E0`: the base value from the three settlement fields.
pub fn base(a: i32, b: i32, c: i32) -> i32 {
    let surplus = (a - b).max(0);
    let first = (surplus as f32 * 0.12).floor() as i32;
    let second = ((c + b) as f32 * 0.2) as i32;
    (first + second) * 25 + 15000
}

/// A region group's change state (`0x00A634D0`: looked up per faction in the group belief's list
/// `+0x124/+0x128`; **5 when the faction has no entry**).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupChange {
    /// 0 (own group).
    Reduced,
    /// 1 (own group).
    Split,
    /// 2 (own group).
    Lost,
    /// 3 (other group).
    Increased,
    /// 4 (other group).
    Merged,
    /// 5 (default).
    New,
}

/// The personality's `COMPOSITE_VALUE_ANALYSER_*` multipliers (personality slots `+0x218..+0x240`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Multipliers {
    pub lost: f32,
    pub reduced: f32,
    pub split: f32,
    pub new: f32,
    pub increased: f32,
    pub merged: f32,
    pub loss_certain: f32,
    pub loss_very_likely: f32,
    pub loss_likely: f32,
    pub can_win: f32,
}

impl Multipliers {
    /// Reads them through a tunable getter (`key → value`, with the shipped `default` values as
    /// fallbacks).
    pub fn from_tunables(t: impl Fn(&str, f32) -> f32) -> Self {
        let k = |name: &str, d: f32| t(&format!("COMPOSITE_VALUE_ANALYSER_{name}_MULTIPLIER"), d);
        Multipliers {
            lost: k("REGION_GROUP_LOST", 1.0),
            reduced: k("REGION_GROUP_REDUCED", 1.3),
            split: k("REGION_GROUP_SPLIT", 1.4),
            new: k("REGION_GROUP_NEW", 1.0),
            increased: k("REGION_GROUP_INCREASED", 1.3),
            merged: k("REGION_GROUP_MERGED", 1.4),
            loss_certain: k("REGION_LOSS_CERTAIN", 0.5),
            loss_very_likely: k("REGION_LOSS_VERY_LIKELY", 0.8),
            loss_likely: k("REGION_LOSS_LIKELY", 0.9),
            can_win: k("REGION_CAN_WIN", 1.1),
        }
    }
}

/// What the composite analyser reads about one region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Composite {
    /// The region lies in a region group of ours (`0x008CF5F0` and the group belief's virtual
    /// `+8`): the "own" branch.
    pub own_group: bool,
    /// The group's change state for us.
    pub change: GroupChange,
    /// `n` of the compounding factor: regions of the group's list passing `0x00898B20` (UNKNOWN
    /// test, `0x00A2C170`).
    pub group_regions: i32,
    /// The region's counts `+0x104→+0x44 / +0x40 / +0x48` (`0x00D7A2F0 / 0x00D7A200 /
    /// 0x00D7A1E0`; UNKNOWN meanings: a level 0..4 and two counts).
    pub level: i32,
    pub count_a: i32,
    pub count_b: i32,
    /// The region is the faction's capital (`0x00A8B5A0`: faction `+0x72C`, INFERRED).
    pub capital: bool,
    /// `faction+0x724` and `0x009749B0(settlement)` (UNKNOWN): ×3.
    pub triple: bool,
    /// The settlement's list `0x0047B490()+8` is not empty and so is that of some region of the
    /// faction's list `+0x778` (UNKNOWN): +5000.
    pub bonus: bool,
}

/// `1 + ((m − 1) − (m − 1)^(n+1)) / (2 − d)`: the compounding factor of the reduced / split /
/// increased / merged states (`powf` at `0x01285310`; `d` is `m` for the own states and the
/// REDUCED multiplier for the other ones, as the exe reads it).
fn compound(m: f32, d: f32, n: i32) -> f32 {
    let x = m - 1.0;
    (x - x.powf((n + 1) as f32)) / (2.0 - d) + 1.0
}

/// `0x00AB8BC0` (CONFIRMED): the composite value of a region from its base value.
pub fn composite(base: i32, c: &Composite, m: &Multipliers) -> i32 {
    let mut v = base;
    let scale = |v: i32, f: f32| (f * v as f32) as i32;
    if c.own_group {
        match c.change {
            GroupChange::Reduced => v = scale(v, compound(m.reduced, m.reduced, c.group_regions)),
            GroupChange::Split => v = scale(v, compound(m.split, m.split, c.group_regions)),
            GroupChange::Lost => v = scale(v, m.lost),
            _ => {}
        }
        // Loss likelihood (the exe's branches, flattened).
        let (l, a, b) = (c.level, c.count_a, c.count_b);
        let likely = |v: i32| if a < 2 && b > 0 { scale(v, m.loss_likely) } else { v };
        if l == 4 {
            if a == 0 {
                if b == 0 {
                    v = scale(v, m.loss_certain);
                } else if b >= 1 {
                    v = scale(v, m.loss_very_likely);
                } else {
                    v = likely(v);
                }
            } else {
                v = likely(v);
            }
        } else if l > 2 {
            if a == 0 && b >= 1 {
                v = scale(v, m.loss_very_likely);
            } else {
                v = likely(v);
            }
        }
        if c.capital {
            v *= 5;
        }
    } else {
        match c.change {
            GroupChange::Increased => v = scale(v, compound(m.increased, m.reduced, c.group_regions)),
            GroupChange::Merged => v = scale(v, compound(m.merged, m.reduced, c.group_regions)),
            GroupChange::New => v = scale(v, m.new),
            _ => {}
        }
        if c.level < 2 {
            v = scale(v, m.can_win);
        }
    }
    if c.triple {
        v *= 3;
    }
    if c.bonus {
        v += 5000;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Multipliers {
        Multipliers::from_tunables(|_, d| d)
    }

    fn plain() -> Composite {
        Composite {
            own_group: false,
            change: GroupChange::New,
            group_regions: 0,
            level: 0,
            count_a: 0,
            count_b: 0,
            capital: false,
            triple: false,
            bonus: false,
        }
    }

    #[test]
    fn base_formula() {
        assert_eq!(base(0, 0, 0), 15000);
        // floor(0.12 × 1000) = 120; trunc(0.2 × 500) = 100 → 15000 + 25 × 220.
        assert_eq!(base(1000, 0, 500), 20500);
        // a − b clamps at 0; c + b = 600 → 120.
        assert_eq!(base(100, 300, 300), 15000 + 25 * 120);
    }

    #[test]
    fn composite_branches() {
        let m = defaults();
        // Other group, NEW (×1.0), level 0 → CAN_WIN ×1.1.
        assert_eq!(composite(20000, &plain(), &m), 22000);
        // Own group, LOST ×1.0, level 4 with no counts → LOSS_CERTAIN ×0.5, capital ×5.
        let c = Composite { own_group: true, change: GroupChange::Lost, level: 4, capital: true, ..plain() };
        assert_eq!(composite(20000, &c, &m), 50000);
        // Own group, REDUCED with n = 2: 1 + (0.3 − 0.027) / 0.7 = 1.39.
        let c = Composite { own_group: true, change: GroupChange::Reduced, group_regions: 2, ..plain() };
        assert!((composite(10000, &c, &m) - 13900).abs() <= 1);
        // Level 3 with count_a 0 and count_b 2 → VERY_LIKELY ×0.8; +5000 bonus.
        let c = Composite { own_group: true, change: GroupChange::New, level: 3, count_b: 2, bonus: true, ..plain() };
        assert_eq!(composite(10000, &c, &m), 13000);
    }
}

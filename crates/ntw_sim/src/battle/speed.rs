//! Battle speed (time multiplier). W1 §12.2, 0x005D02A0 (CONFIRMED):
//! the multipliers are {0, 0.4, 1, 2, 4} and the "cycle speed" order is 0 → 0.4 → 1 → 2 → 4 → 0.
//!
//! The speed only changes how many 0.1 s ticks are run per real second; it never changes the
//! result of a tick, which keeps the simulation deterministic.

/// One of the five battle speeds. W1 §12.2 (CONFIRMED values and order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BattleSpeed {
    /// ×0 (paused).
    Paused,
    /// ×0.4 (slow motion).
    Slow,
    /// ×1.
    #[default]
    Normal,
    /// ×2.
    Fast,
    /// ×4.
    VeryFast,
}

impl BattleSpeed {
    /// All speeds in cycle order.
    pub const ALL: [BattleSpeed; 5] = [
        BattleSpeed::Paused,
        BattleSpeed::Slow,
        BattleSpeed::Normal,
        BattleSpeed::Fast,
        BattleSpeed::VeryFast,
    ];

    /// The time multiplier: 0, 0.4, 1, 2 or 4. W1 §12.2 (CONFIRMED).
    pub fn multiplier(self) -> f32 {
        match self {
            BattleSpeed::Paused => 0.0,
            BattleSpeed::Slow => 0.4,
            BattleSpeed::Normal => 1.0,
            BattleSpeed::Fast => 2.0,
            BattleSpeed::VeryFast => 4.0,
        }
    }

    /// The next speed in the cycle 0 → 0.4 → 1 → 2 → 4 → 0. W1 §12.2, 0x005D02A0 (CONFIRMED).
    pub fn cycle(self) -> BattleSpeed {
        match self {
            BattleSpeed::Paused => BattleSpeed::Slow,
            BattleSpeed::Slow => BattleSpeed::Normal,
            BattleSpeed::Normal => BattleSpeed::Fast,
            BattleSpeed::Fast => BattleSpeed::VeryFast,
            BattleSpeed::VeryFast => BattleSpeed::Paused,
        }
    }

    /// Model ticks per real second at this speed (10 ticks/s at ×1).
    pub fn ticks_per_second(self) -> f32 {
        self.multiplier() / super::TICK_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipliers() {
        let m: Vec<f32> = BattleSpeed::ALL.iter().map(|s| s.multiplier()).collect();
        assert_eq!(m, vec![0.0, 0.4, 1.0, 2.0, 4.0]);
    }

    #[test]
    fn cycle_order() {
        let mut s = BattleSpeed::Paused;
        let mut seen = vec![s.multiplier()];
        for _ in 0..5 {
            s = s.cycle();
            seen.push(s.multiplier());
        }
        assert_eq!(seen, vec![0.0, 0.4, 1.0, 2.0, 4.0, 0.0]);
    }

    #[test]
    fn ticks_per_second() {
        assert!((BattleSpeed::Normal.ticks_per_second() - 10.0).abs() < 1e-4);
        assert_eq!(BattleSpeed::Paused.ticks_per_second(), 0.0);
    }
}

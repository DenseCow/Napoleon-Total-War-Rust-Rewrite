//! A unit's casualty and kill log: the recent and extended casualty ratios, the kill ratio and the
//! fighting "category" that the morale code reads (unit `+0xCBC`, `+0xCC0`, `+0xCC4`, `+0xCC8`,
//! `+0xD08`).
//!
//! CONFIRMED (`0x005828A0`, the unit update's slot 4, on the unit's `+0xC78` object built by
//! `0x0051DF60`):
//! - Two ring buffers: 40 entries (`0x28`) and 60 entries (`0x3C`), each entry
//!   `(kills, deaths, men before the deaths)` (`0x00582B60`).
//! - Every tick the recent ring gets this tick's kills and deaths (object `+0x54` / `+0x58`, counted
//!   by `0x00566EB0` / `0x00566D20`) and the men alive plus those deaths (unit `+0x204`).
//!   - recent casualty ratio `+0xCBC` = Σ deaths / max men over the ring (`0x005692F0`, `0x00569200`);
//!   - kill ratio `+0xCC0` = Σ kills / max men (`0x005695D0`);
//!   - with max men 0 the recent ratio is 1.0.
//! - Every 10th battle tick (battle `+0x58 % 10 == 0`) the extended ring gets the kills and deaths
//!   summed since the last such push (`+0x5C` / `+0x60`, then cleared) and the men plus those deaths.
//!   - extended casualty ratio `+0xCC4` = Σ deaths / max men;
//!   - extended kill ratio `+0xCC8` = Σ kills / max men;
//!   - both 0 when max men is 0.
//! - The category `+0xD08` from the extended ratios (d = deaths, k = kills):
//!   - 0 if both are below 0.01;
//!   - 1, 2, 3 when the unit is killing more than it loses (k − d > 0.5 → 1, > 0.3 → 2, else 3);
//!   - 7, 6, 5 when it loses more (d − k > 0.5 → 7, > 0.3 → 6, else 5);
//!   - 4 otherwise.
//!
//! CONFIRMED (round 12): slot 4 itself clears the per-tick counters `+0x54` / `+0x58` right after
//! the recent push, every tick, and the 10-tick counters `+0x5C` / `+0x60` right after the
//! extended push. The model counts deaths as the drop in men since the unit's last update, which
//! is the same count.

/// Recent ring size (ticks), CONFIRMED.
pub const RECENT_TICKS: usize = 40;
/// Extended ring size (pushes, one per 10 ticks), CONFIRMED.
pub const EXTENDED_PUSHES: usize = 60;

#[derive(Debug, Clone, PartialEq, Default)]
struct Ring {
    next: usize,
    entries: Vec<(u32, u32, u32)>,
}

impl Ring {
    fn new(n: usize) -> Self {
        Ring { next: 0, entries: vec![(0, 0, 0); n] }
    }

    fn push(&mut self, e: (u32, u32, u32)) {
        if self.entries.is_empty() {
            return;
        }
        self.entries[self.next] = e;
        self.next = (self.next + 1) % self.entries.len();
    }

    fn max_men(&self) -> u32 {
        self.entries.iter().map(|e| e.2).max().unwrap_or(0)
    }

    fn sums(&self) -> (u32, u32) {
        self.entries.iter().fold((0, 0), |a, e| (a.0 + e.0, a.1 + e.1))
    }
}

/// The unit's log (`+0xC78` object) and the ratios it gives.
#[derive(Debug, Clone, PartialEq)]
pub struct CasualtyLog {
    recent: Ring,
    extended: Ring,
    /// Kills and deaths since the last extended push (`+0x5C` / `+0x60`).
    ten: (u32, u32),
    /// Men and kills at the last update (the model's way to count this tick's deaths and kills).
    last: Option<(u32, u32)>,
    /// `+0xCBC`.
    pub recent_ratio: f32,
    /// `+0xCC0`.
    pub kill_ratio: f32,
    /// `+0xCC4`.
    pub extended_ratio: f32,
    /// `+0xCC8`.
    pub extended_kill_ratio: f32,
    /// `+0xD08`.
    pub category: i32,
}

impl Default for CasualtyLog {
    fn default() -> Self {
        CasualtyLog {
            recent: Ring::new(RECENT_TICKS),
            extended: Ring::new(EXTENDED_PUSHES),
            ten: (0, 0),
            last: None,
            recent_ratio: 0.0,
            kill_ratio: 0.0,
            extended_ratio: 0.0,
            extended_kill_ratio: 0.0,
            category: 0,
        }
    }
}

impl CasualtyLog {
    /// One update of `0x005828A0` for a unit with `men` alive and `kills` made so far, at battle
    /// tick `tick`.
    pub fn update(&mut self, men: u32, kills: u32, tick: u32) {
        let (last_men, last_kills) = self.last.unwrap_or((men, kills));
        let deaths = last_men.saturating_sub(men);
        let killed = kills.saturating_sub(last_kills);
        self.last = Some((men, kills));
        self.recent.push((killed, deaths, men + deaths));
        let m = self.recent.max_men();
        let (k, d) = self.recent.sums();
        if m == 0 {
            self.recent_ratio = 1.0;
        } else {
            let inv = 1.0 / m as f32;
            self.recent_ratio = inv * d as f32;
            self.kill_ratio = k as f32 * inv;
        }
        self.ten.0 += killed;
        self.ten.1 += deaths;
        if tick.is_multiple_of(10) {
            self.extended.push((self.ten.0, self.ten.1, men + self.ten.1));
            let m = self.extended.max_men();
            let (k, d) = self.extended.sums();
            if m == 0 {
                self.extended_ratio = 0.0;
            } else {
                let inv = 1.0 / m as f32;
                self.extended_ratio = inv * d as f32;
                self.extended_kill_ratio = k as f32 * inv;
            }
            self.ten = (0, 0);
        }
        let (d, k) = (self.extended_ratio, self.extended_kill_ratio);
        self.category = if d < 0.01 && k < 0.01 {
            0
        } else if k > d && k > 0.01 {
            if k - d > 0.5 {
                1
            } else if k - d > 0.3 {
                2
            } else {
                3
            }
        } else if d > k && d > 0.01 {
            if d - k > 0.5 {
                7
            } else if d - k > 0.3 {
                6
            } else {
                5
            }
        } else {
            4
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_window_is_forty_ticks() {
        let mut log = CasualtyLog::default();
        log.update(100, 0, 1);
        // Lose 20 men in one tick: 20 / 100 over the window.
        log.update(80, 0, 2);
        assert!((log.recent_ratio - 0.2).abs() < 1e-6);
        // 40 quiet ticks later the loss has left the window.
        for t in 3..43 {
            log.update(80, 0, t);
        }
        assert!(log.recent_ratio.abs() < 1e-6);
    }

    #[test]
    fn extended_ratio_and_category() {
        let mut log = CasualtyLog::default();
        log.update(100, 0, 1);
        // Lose 60 men, kill 5, within the first 10 ticks.
        log.update(40, 5, 2);
        for t in 3..=10 {
            log.update(40, 5, t);
        }
        assert!((log.extended_ratio - 0.6).abs() < 1e-6, "{}", log.extended_ratio);
        assert!((log.extended_kill_ratio - 0.05).abs() < 1e-6);
        assert_eq!(log.category, 7);
    }
}

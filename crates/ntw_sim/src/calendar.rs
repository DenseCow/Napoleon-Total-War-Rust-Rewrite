//! The campaign calendar: dates and turn counting.
//!
//! W3 §3.1 (CONFIRMED by save filenames):
//! - `DATE` = `{ u32 year, u32 season, u32 month (0 = January), u32 half (0 = "Early", 2 = "Late") }`.
//! - `CAMPAIGN_CALENDAR` = `{ u32 turns_per_year = 24, u32 turn_in_year = month*2 + (half==2), DATE, u32 turns_elapsed }`.
//! - The save header's `turn_number` = `turns_elapsed + 1`.
//!
//! Napoleon has two turns per month ("Early April", "Late April", ...).

/// Value of `Date::half` for the first turn of a month ("Early"). W3 §3.1 (CONFIRMED).
pub const HALF_EARLY: u32 = 0;
/// Value of `Date::half` for the second turn of a month ("Late"). W3 §3.1 (CONFIRMED).
pub const HALF_LATE: u32 = 2;
/// Turns per year stored in every observed save. W3 §3.1 (CONFIRMED).
pub const TURNS_PER_YEAR: u32 = 24;

/// Season codes as stored in `DATE`. W3 §3.1 (CONFIRMED from the save header season string).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Season {
    /// Code 0.
    Summer = 0,
    /// Code 1.
    Winter = 1,
    /// Code 2.
    Spring = 2,
    /// Code 3.
    Autumn = 3,
}

impl Season {
    /// Converts a stored code to a `Season` (`None` for unknown codes).
    pub fn from_code(code: u32) -> Option<Season> {
        match code {
            0 => Some(Season::Summer),
            1 => Some(Season::Winter),
            2 => Some(Season::Spring),
            3 => Some(Season::Autumn),
            _ => None,
        }
    }
}

/// A campaign date, laid out like the original's `DATE` record. W3 §3.1 (CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Date {
    /// Calendar year, e.g. 1806.
    pub year: u32,
    /// Season code (see [`Season`]). Stored, never derived: the rule is UNKNOWN (W3 §3.1).
    pub season: u32,
    /// Month, 0 = January .. 11 = December.
    pub month: u32,
    /// 0 = "Early", 2 = "Late".
    pub half: u32,
}

impl Date {
    /// Index of this date within its year: `month*2 + (half == 2)`. W3 §3.1 (CONFIRMED).
    pub fn turn_in_year(&self) -> u32 {
        self.month * 2 + u32::from(self.half == HALF_LATE)
    }
}

/// The campaign calendar record. W3 §3.1 (CONFIRMED layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Calendar {
    /// Always 24 in the observed saves.
    pub turns_per_year: u32,
    /// `date.turn_in_year()`, stored redundantly like the original.
    pub turn_in_year: u32,
    /// The current date.
    pub date: Date,
    /// Number of turns already played. The save header shows `turns_elapsed + 1`.
    pub turns_elapsed: u32,
}

impl Calendar {
    /// Builds a calendar at `date` with `turns_elapsed` turns played.
    pub fn new(date: Date, turns_elapsed: u32) -> Self {
        Calendar {
            turns_per_year: TURNS_PER_YEAR,
            turn_in_year: date.turn_in_year(),
            date,
            turns_elapsed,
        }
    }

    /// The 1-based turn number shown in the save header. W3 §3.1 (CONFIRMED).
    pub fn turn_number(&self) -> u32 {
        self.turns_elapsed + 1
    }

    /// Moves to the next half-month turn.
    ///
    /// INFERRED from the CONFIRMED date encoding: Early → Late of the same month, Late → Early of
    /// the next month, December → January of the next year.
    ///
    /// The season is **kept as stored**.
    // TODO(W3 §3.1): the season derivation rule is UNKNOWN ("Early September 1805" is stored as
    // Spring). Do not invent one; update `season` here once the real rule is found.
    pub fn advance_turn(&mut self) {
        if self.date.half == HALF_EARLY {
            self.date.half = HALF_LATE;
        } else {
            self.date.half = HALF_EARLY;
            self.date.month += 1;
            if self.date.month >= 12 {
                self.date.month = 0;
                self.date.year += 1;
            }
        }
        self.turn_in_year = self.date.turn_in_year();
        self.turns_elapsed += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_april_1806() {
        // W3 §3.1: "Early April 1806" is stored as 1806/2/3/0 and has index 6.
        let d = Date {
            year: 1806,
            season: 2,
            month: 3,
            half: 0,
        };
        assert_eq!(d.turn_in_year(), 6);
        assert_eq!(Season::from_code(d.season), Some(Season::Spring));
    }

    #[test]
    fn late_december_1805() {
        // W3 §3.1: "Late December 1805" is stored as 1805/1/11/2 -> index 23.
        let d = Date {
            year: 1805,
            season: 1,
            month: 11,
            half: 2,
        };
        assert_eq!(d.turn_in_year(), 23);
    }

    #[test]
    fn advance_early_to_late() {
        let mut c = Calendar::new(
            Date {
                year: 1806,
                season: 2,
                month: 3,
                half: 0,
            },
            10,
        );
        assert_eq!(c.turn_number(), 11);
        c.advance_turn();
        assert_eq!(
            c.date,
            Date {
                year: 1806,
                season: 2,
                month: 3,
                half: 2
            }
        );
        assert_eq!(c.turn_in_year, 7);
        assert_eq!(c.turns_elapsed, 11);
    }

    #[test]
    fn advance_over_new_year_keeps_season() {
        let mut c = Calendar::new(
            Date {
                year: 1805,
                season: 1,
                month: 11,
                half: 2,
            },
            0,
        );
        c.advance_turn();
        assert_eq!(c.date.year, 1806);
        assert_eq!(c.date.month, 0);
        assert_eq!(c.date.half, HALF_EARLY);
        assert_eq!(c.turn_in_year, 0);
        // Season is not derived (UNKNOWN rule), so it stays as stored.
        assert_eq!(c.date.season, 1);
        assert_eq!(c.turns_per_year, 24);
    }

    #[test]
    fn full_year_is_24_turns() {
        let start = Date {
            year: 1805,
            season: 0,
            month: 0,
            half: 0,
        };
        let mut c = Calendar::new(start, 0);
        for _ in 0..24 {
            c.advance_turn();
        }
        assert_eq!(
            c.date,
            Date {
                year: 1806,
                ..start
            }
        );
        assert_eq!(c.turns_elapsed, 24);
    }
}

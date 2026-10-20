//! The campaign calendar: dates and turn counting.
//!
//! W3 §3.1 (CONFIRMED by save filenames):
//! - `DATE` = `{ u32 year, u32 season, u32 month (0 = January), u32 half (0 = "Early", 2 = "Late") }`.
//! - `CAMPAIGN_CALENDAR` = `{ u32 turns_per_year = 24, u32 turn_in_year = month*2 + (half==2), DATE, u32 turns_elapsed }`.
//! - The save header's `turn_number` = `turns_elapsed + 1`.
//!
//! Napoleon has two turns per month ("Early April", "Late April", ...): `turns_per_year` 24. The
//! calendar follows the stored `turns_per_year` as the exe does (`0x008A98B0`, CONFIRMED; see
//! [`Calendar::advance_turn`]): the exe's own engine also knows 12 turns a year (one per month) and 1
//! or 2 (the summer / winter seasons of Empire), and a campaign's data can name the date step of any
//! other count ([`Calendar::advance_turn`]'s `date_step`).

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

    /// The season's `seasons` table key, by code: the exe's key table `0x01457a74` (CONFIRMED:
    /// `season_summer`, `season_winter`, `season_spring`, `season_autumn` for codes 0..3), which
    /// `0x008ED110` indexes with the calendar's season to find the record that names it on screen.
    pub fn key(self) -> &'static str {
        match self {
            Season::Summer => "season_summer",
            Season::Winter => "season_winter",
            Season::Spring => "season_spring",
            Season::Autumn => "season_autumn",
        }
    }

    /// The loc key of the season's on-screen name, `seasons_onscreen_<key>` (`0x008ED110`; the one
    /// builder for the HUD's `season_string` and the save header's season).
    pub fn onscreen_key(self) -> String {
        format!("seasons_onscreen_{}", self.key())
    }
}

/// A campaign date, laid out like the original's `DATE` record. W3 §3.1 (CONFIRMED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Date {
    /// Calendar year, e.g. 1806.
    pub year: u32,
    /// Season code (see [`Season`]). A start position's is stored as it is (W3 §3.1: "Early September
    /// 1805" is stored as Spring); every date the calendar moves to gets its month's season
    /// ([`Date::set_quarter`]).
    pub season: u32,
    /// Month, 0 = January .. 11 = December.
    pub month: u32,
    /// 0 = "Early", 2 = "Late".
    pub half: u32,
}

/// Quarter-months in a year: the exe's date position runs 0..48 (`0x008EDC80`, CONFIRMED).
pub const QUARTERS_PER_YEAR: u32 = 48;

impl Date {
    /// Index of this date within its year: `month*2 + (half == 2)`. W3 §3.1 (CONFIRMED).
    pub fn turn_in_year(&self) -> u32 {
        self.month * 2 + u32::from(self.half == HALF_LATE)
    }

    /// The exe's date setter `0x008EDC80` (CONFIRMED): `x` quarter-months into `year` (an `x` of 48 or
    /// more is 0): month `x / 4`, half `x % 4`, and the month's season: December to February winter,
    /// March to May spring, June to August summer, September to November autumn.
    pub fn set_quarter(&mut self, year: u32, x: u32) {
        let x = if x < QUARTERS_PER_YEAR { x } else { 0 };
        self.year = year;
        self.month = x / 4;
        self.half = x % 4;
        self.season = match self.month {
            0 | 1 | 11 => Season::Winter,
            2..=4 => Season::Spring,
            5..=7 => Season::Summer,
            _ => Season::Autumn,
        } as u32;
    }
}

/// The date step per turn, in quarter-months, the exe gives a calendar of `turns_per_year`
/// (`0x008A98B0`, CONFIRMED): 24 turns → 2 (half a month), 12 → 4 (a month); any other count keeps
/// the month (`None`).
pub fn original_date_step(turns_per_year: u32) -> Option<u32> {
    match turns_per_year {
        24 => Some(2),
        12 => Some(4),
        _ => None,
    }
}

/// The `random_localisation_strings` keys naming the turns of a 24-turn year, in turn order
/// (string ids 0x191..0x1A8, static table `0x0164E7E0`; CONFIRMED, UI_FIDELITY.md §5.1).
const HALF_MONTH_ROUND_IDS: [&str; 24] = [
    "round_id_early_january", "round_id_late_january", "round_id_early_february", "round_id_late_february",
    "round_id_early_march", "round_id_late_march", "round_id_early_april", "round_id_late_april",
    "round_id_early_may", "round_id_late_may", "round_id_early_june", "round_id_late_june",
    "round_id_early_july", "round_id_late_july", "round_id_early_august", "round_id_late_august",
    "round_id_early_september", "round_id_late_september", "round_id_early_october", "round_id_late_october",
    "round_id_early_november", "round_id_late_november", "round_id_early_december", "round_id_late_december",
];

/// The `random_localisation_strings` keys naming the turns of a 12-turn year (string ids
/// 0x1A9..0x1B4; CONFIRMED, UI_FIDELITY.md §5.1).
const MONTH_ROUND_IDS: [&str; 12] = [
    "round_id_january", "round_id_february", "round_id_march", "round_id_april", "round_id_may", "round_id_june",
    "round_id_july", "round_id_august", "round_id_september", "round_id_october", "round_id_november",
    "round_id_december",
];

/// The campaign calendar record. W3 §3.1 (CONFIRMED layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Calendar {
    /// Turns in a year: 24 in every shipped file; the calendar follows it ([`Self::advance_turn`]).
    pub turns_per_year: u32,
    /// The turn's place in its year (0 .. `turns_per_year`), stored like the original.
    pub turn_in_year: u32,
    /// The current date.
    pub date: Date,
    /// Number of turns already played. The save header shows `turns_elapsed + 1`.
    pub turns_elapsed: u32,
}

impl Calendar {
    /// Builds a 24-turn calendar at `date` with `turns_elapsed` turns played.
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

    /// The `random_localisation_strings` key of the current turn's name, the "round description" of
    /// the HUD date and the default save name (CONFIRMED, UI_FIDELITY.md §5.1): when the calendar is
    /// read, `0x008EF0C0` fills its turn-name list from `turns_per_year`: 24 → the half-month names
    /// ("Early January" ...), 12 → the month names, any other count → that many empty names; the
    /// round description is the entry at `turn_in_year` (`0x008D1E20`). `None` is the exe's empty
    /// name (also for a `turn_in_year` past the list, which the exe does not check).
    pub fn round_id(&self) -> Option<&'static str> {
        let ids: &[&'static str] = match self.turns_per_year {
            24 => &HALF_MONTH_ROUND_IDS,
            12 => &MONTH_ROUND_IDS,
            _ => &[],
        };
        ids.get(self.turn_in_year as usize).copied()
    }

    /// Moves to the next turn: the exe's `0x008A98B0` (CONFIRMED). One more turn played; the turn in
    /// the year goes up; in a calendar of fewer than 3 turns a year the season flips between summer
    /// and winter at half and at the end of the year; at the end of the year (`turn_in_year` reaches
    /// `turns_per_year`) the year goes up and the turn in the year is 0; then the date moves to
    /// `turn_in_year × step` quarter-months ([`Date::set_quarter`], which also sets the season).
    ///
    /// `date_step` is the campaign's own step in quarter-months (its data); `None` takes the exe's for
    /// the calendar's `turns_per_year` ([`original_date_step`]: 24 turns → half a month, 12 → a month,
    /// any other count keeps the month as the exe does).
    pub fn advance_turn(&mut self, date_step: Option<u32>) {
        let n = self.turns_per_year;
        self.turns_elapsed += 1;
        let mut t = self.turn_in_year + 1;
        if n < 3 && (t == n / 2 || t == n) {
            self.date.season = u32::from(self.date.season == Season::Summer as u32);
        }
        if t == n {
            self.date.year += 1;
            t = 0;
        }
        self.turn_in_year = t;
        if let Some(step) = date_step.or_else(|| original_date_step(n)) {
            self.date.set_quarter(self.date.year, t.saturating_mul(step));
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onscreen_key_prefixes_the_season_key() {
        assert_eq!(Season::Summer.onscreen_key(), "seasons_onscreen_season_summer");
        assert_eq!(Season::Autumn.onscreen_key(), "seasons_onscreen_season_autumn");
    }

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
        c.advance_turn(None);
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
    fn advance_over_new_year_is_winter() {
        let mut c = Calendar::new(
            Date {
                year: 1805,
                season: 1,
                month: 11,
                half: 2,
            },
            0,
        );
        c.advance_turn(None);
        assert_eq!(c.date.year, 1806);
        assert_eq!(c.date.month, 0);
        assert_eq!(c.date.half, HALF_EARLY);
        assert_eq!(c.turn_in_year, 0);
        // January: winter (`0x008EDC80`).
        assert_eq!(c.date.season, Season::Winter as u32);
        assert_eq!(c.turns_per_year, 24);
    }

    #[test]
    fn full_year_is_24_turns() {
        let start = Date {
            year: 1805,
            season: 1,
            month: 0,
            half: 0,
        };
        let mut c = Calendar::new(start, 0);
        for _ in 0..24 {
            c.advance_turn(None);
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

    /// Every date the calendar moves to has its month's season (`0x008EDC80`): a start position's
    /// "Early September" stored as spring becomes autumn at Late September.
    #[test]
    fn the_season_follows_the_month() {
        let mut c = Calendar::new(Date { year: 1805, season: Season::Spring as u32, month: 8, half: HALF_EARLY }, 0);
        c.advance_turn(None);
        assert_eq!((c.date.month, c.date.half, c.date.season), (8, HALF_LATE, Season::Autumn as u32));
        let seasons: Vec<u32> = (0..12).map(|m| {
            let mut d = c.date;
            d.set_quarter(1806, m * 4);
            d.season
        }).collect();
        assert_eq!(seasons, [1, 1, 2, 2, 2, 0, 0, 0, 3, 3, 3, 1]);
        let mut d = c.date;
        d.set_quarter(1806, 48);
        assert_eq!((d.month, d.half), (0, 0), "48 or more is the year's start");
    }

    /// The exe's other calendars: 12 turns a year move a month per turn; 2 a year (Empire's summer and
    /// winter) flip the season and keep the month.
    #[test]
    fn other_turns_per_year_follow_the_exe() {
        let mut c = Calendar { turns_per_year: 12, turn_in_year: 0, date: Date { year: 1700, season: 1, month: 0, half: 0 }, turns_elapsed: 0 };
        c.advance_turn(None);
        assert_eq!((c.date.month, c.date.half, c.turn_in_year), (1, 0, 1));
        for _ in 0..11 {
            c.advance_turn(None);
        }
        assert_eq!((c.date.year, c.date.month, c.turn_in_year, c.turns_elapsed), (1701, 0, 0, 12));
        let mut e = Calendar { turns_per_year: 2, turn_in_year: 0, date: Date { year: 1700, season: Season::Summer as u32, month: 3, half: 0 }, turns_elapsed: 0 };
        e.advance_turn(None);
        assert_eq!((e.date.year, e.date.season, e.date.month, e.turn_in_year), (1700, Season::Winter as u32, 3, 1));
        e.advance_turn(None);
        assert_eq!((e.date.year, e.date.season, e.date.month, e.turn_in_year), (1701, Season::Summer as u32, 3, 0));
    }

    /// A campaign's own calendar: 4 turns a year with its data's step of a quarter year (12
    /// quarter-months) walks the seasons; without a step the exe keeps the month.
    #[test]
    fn a_campaign_names_its_date_step() {
        let start = Date { year: 1550, season: Season::Winter as u32, month: 0, half: 0 };
        let mut c = Calendar { turns_per_year: 4, turn_in_year: 0, date: start, turns_elapsed: 0 };
        let mut months = Vec::new();
        for _ in 0..4 {
            c.advance_turn(Some(12));
            months.push((c.date.month, c.date.season));
        }
        assert_eq!(months, [(3, 2), (6, 0), (9, 3), (0, 1)]);
        assert_eq!(c.date.year, 1551);
        let mut plain = Calendar { turns_per_year: 4, turn_in_year: 0, date: start, turns_elapsed: 0 };
        plain.advance_turn(None);
        assert_eq!(plain.date, start, "no step: the exe keeps the date");
    }

    #[test]
    fn round_id_follows_turns_per_year_and_turn_in_year() {
        let date = Date { year: 1805, season: Season::Spring as u32, month: 3, half: HALF_LATE };
        let c = Calendar::new(date, 0);
        assert_eq!(c.round_id(), Some("round_id_late_april"));
        let first = Calendar::new(Date { year: 1805, season: Season::Winter as u32, month: 0, half: HALF_EARLY }, 0);
        assert_eq!(first.round_id(), Some("round_id_early_january"));
        let last = Calendar { turn_in_year: 23, ..c };
        assert_eq!(last.round_id(), Some("round_id_late_december"));
        let monthly = Calendar { turns_per_year: 12, turn_in_year: 4, ..c };
        assert_eq!(monthly.round_id(), Some("round_id_may"));
        // Any other count: the exe's empty names.
        assert_eq!(Calendar { turns_per_year: 2, turn_in_year: 0, ..c }.round_id(), None);
        assert_eq!(Calendar { turns_per_year: 24, turn_in_year: 24, ..c }.round_id(), None);
    }
}

//! When a land battle ends: the victory conditions and time limit of a battle specification
//! (`victory_condition` per alliance, `battle_description/duration` and
//! `timeout_winning_alliance_index`, see `analysis/battle/BATTLE_FLOW.md` §4).
//!
//! - `kill_or_rout_enemy` (CONFIRMED name, every land battle file): an alliance with this condition
//!   wins when no enemy unit is still fighting, i.e. every enemy unit is routing, shattered or
//!   destroyed ([`LandUnit::is_out_of_fight`]). INFERRED: the same test as
//!   [`Battle::battle_result`]; whether the original waits for routing units to leave the field
//!   or counts units that may still rally is UNKNOWN.
//! - Time limit (CONFIRMED, BATTLE_FIDELITY.md §9): the battle file's `duration` is parsed as an
//!   `f32` (default −1 = no limit) and compared with the battle clock, which adds 0.1 s per tick
//!   only while the battle runs (states 2..5, so not during deployment). The battle times out
//!   when `0 <= duration < clock` (strict), and then `timeout_winning_alliance_index` wins. The
//!   time-out test runs after the per-alliance test and overrides it in the same tick.
//! - Both sides out of the fight at once: a draw.

use super::model::{Battle, LandUnit};

/// What a battle file asks for.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VictoryRules {
    /// Time limit in seconds of battle time, if any.
    pub time_limit_s: Option<f32>,
    /// The side that wins when the time runs out.
    pub timeout_winner: Option<u8>,
}

/// How a battle ended (or not yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Still running.
    Ongoing,
    /// One side routed or destroyed every enemy unit.
    Won {
        /// The winning side.
        side: u8,
    },
    /// The time limit ran out; `side` is the file's timeout winner.
    TimeOut {
        /// The side the battle file names for a timeout.
        side: u8,
    },
    /// Nobody is left fighting (or the time ran out without a timeout winner).
    Draw,
}

impl Outcome {
    /// The winning side, if any.
    pub fn winner(self) -> Option<u8> {
        match self {
            Outcome::Won { side } | Outcome::TimeOut { side } => Some(side),
            _ => None,
        }
    }

    /// True once the battle is decided.
    pub fn is_over(self) -> bool {
        self != Outcome::Ongoing
    }
}

/// Checks the victory conditions after a tick. Sides are the `LandUnit::side` values present.
pub fn check(battle: &Battle, rules: &VictoryRules) -> Outcome {
    // The time-out runs after the alliance test in `0x00582FB0` and overrides it (CONFIRMED).
    if let Some(limit) = rules.time_limit_s
        && limit >= 0.0
        && limit < battle.time_seconds()
    {
        return rules.timeout_winner.map_or(Outcome::Draw, |side| Outcome::TimeOut { side });
    }
    let mut sides: Vec<u8> = battle.units.iter().map(|u| u.side).collect();
    sides.sort_unstable();
    sides.dedup();
    let fighting = |s: u8| battle.units.iter().any(|u| u.side == s && !u.is_out_of_fight());
    let alive: Vec<u8> = sides.iter().copied().filter(|&s| fighting(s)).collect();
    match alive.as_slice() {
        [] if !sides.is_empty() => return Outcome::Draw,
        [s] if sides.len() > 1 => return Outcome::Won { side: *s },
        _ => {}
    }
    Outcome::Ongoing
}

/// Men a side started with, has left, and lost; units at the start and still fighting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SideTotals {
    /// Men at the start.
    pub men_start: u32,
    /// Men alive now.
    pub men_alive: u32,
    /// Enemy men killed by this side.
    pub kills: u32,
    /// Units at the start.
    pub units_start: u32,
    /// Units still fighting (not routing, shattered or destroyed).
    pub units_fighting: u32,
}

/// Totals of one side (for the results screen).
pub fn totals(battle: &Battle, side: u8) -> SideTotals {
    let mine = || battle.units.iter().filter(move |u| u.side == side);
    SideTotals {
        men_start: mine().map(|u| u.max_men).sum(),
        men_alive: mine().map(|u| u.men).sum(),
        kills: mine().map(|u| u.kills).sum(),
        units_start: mine().count() as u32,
        units_fighting: mine().filter(|u| !LandUnit::is_out_of_fight(u)).count() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::morale::MoraleBehaviour;

    fn battle() -> Battle {
        let mut b = Battle::new(1, Default::default(), Default::default());
        b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 100, (0.0, 100.0)));
        b
    }

    #[test]
    fn rout_of_every_enemy_unit_wins() {
        let mut b = battle();
        assert_eq!(check(&b, &VictoryRules::default()), Outcome::Ongoing);
        b.units[1].morale.behaviour = MoraleBehaviour::Routing;
        assert_eq!(check(&b, &VictoryRules::default()), Outcome::Won { side: 0 });
    }

    #[test]
    fn time_limit_gives_the_timeout_winner() {
        let mut b = battle();
        b.tick = 21_001;
        let rules = VictoryRules { time_limit_s: Some(2100.0), timeout_winner: Some(1) };
        assert_eq!(check(&b, &rules), Outcome::TimeOut { side: 1 });
        let rules = VictoryRules { time_limit_s: Some(2100.0), timeout_winner: None };
        assert_eq!(check(&b, &rules), Outcome::Draw);
        // Strict: exactly at the limit the battle goes on (`duration < clock`, 0x00582FB0).
        b.tick = 21_000;
        let rules = VictoryRules { time_limit_s: Some(2100.0), timeout_winner: Some(1) };
        assert_eq!(check(&b, &rules), Outcome::Ongoing);
        // A negative duration (the default −1) means no limit.
        let rules = VictoryRules { time_limit_s: Some(-1.0), timeout_winner: Some(1) };
        assert_eq!(check(&b, &rules), Outcome::Ongoing);
        // The time-out overrides a rout in the same tick.
        b.tick = 21_001;
        b.units[1].morale.behaviour = MoraleBehaviour::Routing;
        let rules = VictoryRules { time_limit_s: Some(2100.0), timeout_winner: Some(1) };
        assert_eq!(check(&b, &rules), Outcome::TimeOut { side: 1 });
    }

    #[test]
    fn totals_count_men_and_units() {
        let mut b = battle();
        b.units[0].men = 60;
        b.units[0].kills = 30;
        let t = totals(&b, 0);
        assert_eq!((t.men_start, t.men_alive, t.kills, t.units_start, t.units_fighting), (100, 60, 30, 1, 1));
    }
}

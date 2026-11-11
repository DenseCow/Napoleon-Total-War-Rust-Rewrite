//! The movie a faction's campaign outcome plays, and what its panel's OK does after it
//! (CAMPAIGN_FIDELITY.md "Campaign over" → "What is shown next", CONFIRMED by disassembly).
//!
//! The exe's movie listener (`0x0099B720`, built by the campaign loader) answers two campaign events
//! with a movie: a faction destroyed (event +0x108; only a human's, whose flag is 1) through
//! `0x00A14E80`, and a faction that won (event +0x120) through `0x00A14E00`. Each queues the
//! `movie_event_strings` row it finds with the panel data the UI script `event_movie_panel` reads
//! (`ExitFunction`, `show_everyone`, `only_call_function_for_local_faction`); no row queues nothing.
//! [`outcome_movie`] is that choice, called only through the [`OUTCOME_MOVIE`](super::seams::OUTCOME_MOVIE)
//! seam ([`CampaignModel::outcome_movie`]).
//!
//! The victory check (`0x0096CD10`, CONFIRMED by disassembly; CAMPAIGN_FIDELITY.md "Campaign over" →
//! "The victory check") is [`CampaignModel::victory_check`]: it asks each human faction's state by its
//! victory conditions (the [`VICTORY_STATE`](super::seams::VICTORY_STATE) seam), posts the winners'
//! and the losers' events and holds a capture choice back until the player continues after the
//! victory screen ([`CampaignModel::continue_game_selected`]).

use super::capture::CaptureChoice;
use super::events::CampaignEvent;
use super::ids::FactionId;
use super::world::CampaignModel;

/// A human faction's state by its victory conditions, as the victory check reads it (`0x008F3DF0`,
/// CONFIRMED by disassembly: after the victory test `0x0096FCE0` it reads the conditions record's
/// met byte +0x31 through `0x0055C390` and its failed byte +0x32 through `0x00949C40`; the check
/// `0x0096CD10` tests the met one first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VictoryState {
    /// Neither met nor failed. Also a faction with no conditions record (faction +0x724) or one of
    /// type 6 (record +0x7C): `0x008F3DF0` runs no test for those and reports both flags clear.
    Undecided,
    /// The conditions are met (record +0x31).
    Won,
    /// The conditions failed (record +0x32; the victory test sets it once the deadline has passed).
    Lost,
}

/// The original's state of `faction` by its victory conditions (the
/// [`VICTORY_STATE`](super::seams::VICTORY_STATE) seam's original rule). A query.
///
/// PROVISIONAL: always [`VictoryState::Undecided`]. The model keeps no conditions record and the
/// victory test `0x0096FCE0` is not ported (BACKLOG §6 "Victory conditions for each campaign";
/// its traced shape is in CAMPAIGN_FIDELITY.md "Campaign over" → "The victory test"), so no
/// vanilla campaign is won or lost by its conditions yet; a mod's rule on the seam is.
pub fn victory_state_original(_m: &CampaignModel, _faction: FactionId) -> VictoryState {
    VictoryState::Undecided
}

/// One `movie_event_strings` row (the DB record `MovieEventStringRecord`), as the lookups read it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MovieEventRow {
    /// #0: the movie event key (the exe resolves it against its event names, table `0x0145CAE8`).
    pub event: String,
    /// #1: the culture the row is for.
    pub culture: String,
    /// #2: the movie file under `movies\`.
    pub movie: String,
    /// #3: the movie id a faction names (`factions` #39); empty when the row has none (the exe compares
    /// the empty string).
    pub id: String,
}

/// How a faction's campaign ended, as the exe's events tell it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignOutcome {
    /// A human faction was destroyed (event +0x108 with its flag 1, from `0x008AB6C0`).
    Destroyed,
    /// The faction won by the victory conditions (event +0x120, from the victory check `0x0096CD10`).
    Won {
        /// The event's second field: set when the win ends a multiplayer campaign.
        multiplayer_over: bool,
    },
}

/// What the movie panel's OK runs after the movie (the data's `ExitFunction`, read by
/// `event_movie_panel.AssignExitFunction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieExit {
    /// `"ExitGame"`: `CampaignUI.QuitToMainScreen` (fill `0x00A07790`; the victory's `0x00A07760` when
    /// the win ends a multiplayer campaign).
    ExitGame,
    /// `"ShowVictoryOptionsPopup"`: the root's `ShowCampaignResultPopup(true)`, the victory popup
    /// with Continue / Exit (fill `0x00A07760`).
    ShowVictoryOptionsPopup,
}

impl MovieExit {
    /// The `ExitFunction` string the panel script compares.
    pub fn script_name(self) -> &'static str {
        match self {
            MovieExit::ExitGame => "ExitGame",
            MovieExit::ShowVictoryOptionsPopup => "ShowVictoryOptionsPopup",
        }
    }
}

/// A movie an outcome plays and the panel data that goes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeMovie {
    /// The row's movie event key, e.g. `game_lost_france`.
    pub event: String,
    /// The row's culture (#1), the second half of the row's key.
    pub culture: String,
    /// The row's movie file under `movies\`, e.g. `EUR_Lose_FR.bik`.
    pub movie: String,
    /// What OK runs after the movie.
    pub exit: MovieExit,
    /// The data's `show_everyone` (the data's byte +4): clear, the panel opens only on the machine of
    /// the faction the movie is for (`0x00A13EA0`, CONFIRMED by disassembly).
    pub show_everyone: bool,
    /// The data's `only_call_function_for_local_faction` (byte +5): the exit runs only on the machine
    /// of the faction the movie is for.
    pub only_local_faction: bool,
}

impl OutcomeMovie {
    /// The loc key of the panel's title, `movie_event_strings_title_<event><culture>` (CONFIRMED by
    /// disassembly: the record builder `0x00F57D60` formats `"movie_event_strings_title_%S"`
    /// (`0x013D43E4`) with the row's event key followed by its culture and keeps the looked-up text at
    /// record +0x14; the queued event `0x0099A4F0` copies it to its +0x18, which the panel
    /// `0x00998E20` passes as the data's `title`). Shipped e.g.
    /// `movie_event_strings_title_game_lost_franceeuropean` = "Faction Destroyed".
    pub fn title_key(&self) -> String {
        format!("movie_event_strings_title_{}{}", self.event, self.culture)
    }
}

/// The movie events of a win, by the winner's government (`0x00A14E00`): event 0x1D unless the
/// government type index (faction +0x70C → +0xC8) is 1 (0x1E) or 2 (0x1F). The index is the one
/// `0x008C8D10` gives a `government_types` key (0 `gov_absolute_monarchy`, 1
/// `gov_constitutional_monarchy`, 2 `gov_republic`, 3 any other, table `0x01457878`), which
/// `0x008C73A0` / `0x008F3610` compare +0xC8 with. The keys keep the exe's spelling.
const VICTORY_ABSOLUTE: &str = "game_victory_absolute_monachy";
/// Event 0x1E.
const VICTORY_CONSTITUTIONAL: &str = "game_victory_constitutional_monachy";
/// Event 0x1F.
const VICTORY_REPUBLIC: &str = "game_victory_republic";

/// The original's outcome movie for `faction` (the [`OUTCOME_MOVIE`](super::seams::OUTCOME_MOVIE)
/// seam's original rule), or `None` when no row matches (the exe then queues nothing). A query.
///
/// - [`CampaignOutcome::Destroyed`] (`0x00A14E80` → `0x009FBA20` → `0x00F55030`): the **last** row whose
///   #3 equals the faction's movie id (`factions` #39), compared exactly (`CompareUniStrings` through
///   `0x004EFB20`), so a faction without an id (or without a `factions` row) gets the last row without one; exit `ExitGame`, `show_everyone` and
///   `only_call_function_for_local_faction` set (data word +4 = 0x0101).
/// - [`CampaignOutcome::Won`] (`0x00A14E00` → `0x009FBA60` → `0x00F54F80`): the government's victory event
///   (see [`VICTORY_ABSOLUTE`]); of its rows, the one
///   [`CampaignRules::culture_row`](super::rules::CampaignRules::culture_row) picks for the faction's culture; exit `ExitGame` when the
///   win ends a multiplayer campaign, else `ShowVictoryOptionsPopup`; both flags clear (data word +4 = 0).
pub fn outcome_movie(m: &CampaignModel, faction: FactionId, outcome: CampaignOutcome) -> Option<OutcomeMovie> {
    let f = m.world.factions.get(&faction)?;
    let rows = &m.rules.movie_events;
    match outcome {
        CampaignOutcome::Destroyed => {
            // `0x00A14E80` reads the id from the faction's record (+0x514 → +0xD8) with no null check:
            // every exe faction has a `factions` row, whose empty #39 is the empty string. A faction with
            // no row (a custom map's) compares the empty string the same way.
            let id = m.rules.faction_movie_ids.get(&f.key).map_or("", String::as_str);
            let row = rows.iter().rev().find(|r| r.id == id)?;
            Some(OutcomeMovie { event: row.event.clone(), culture: row.culture.clone(), movie: row.movie.clone(), exit: MovieExit::ExitGame, show_everyone: true, only_local_faction: true })
        }
        CampaignOutcome::Won { multiplayer_over } => {
            let event = match f.government_key.as_str() {
                "gov_constitutional_monarchy" => VICTORY_CONSTITUTIONAL,
                "gov_republic" => VICTORY_REPUBLIC,
                _ => VICTORY_ABSOLUTE,
            };
            let culture = m.rules.faction_cultures.get(&f.key).map(String::as_str);
            let row = m.rules.culture_row(rows.iter().filter(|r| r.event == event), culture, |r| &r.culture)?;
            let exit = if multiplayer_over { MovieExit::ExitGame } else { MovieExit::ShowVictoryOptionsPopup };
            Some(OutcomeMovie { event: row.event.clone(), culture: row.culture.clone(), movie: row.movie.clone(), exit, show_everyone: false, only_local_faction: false })
        }
    }
}

impl CampaignModel {
    /// The movie `faction`'s campaign outcome plays, through the [`OUTCOME_MOVIE`](super::seams::OUTCOME_MOVIE)
    /// seam ([`outcome_movie`] in vanilla). A query.
    pub fn outcome_movie(&self, faction: FactionId, outcome: CampaignOutcome) -> Option<OutcomeMovie> {
        (self.rules.seams.outcome_movie.rule())(self, faction, outcome)
    }

    /// `faction`'s state by its victory conditions, through the
    /// [`VICTORY_STATE`](super::seams::VICTORY_STATE) seam ([`victory_state_original`] in vanilla).
    /// A query.
    pub fn victory_state(&self, faction: FactionId) -> VictoryState {
        (self.rules.seams.victory_state.rule())(self, faction)
    }

    /// The end of one campaign update, after the work of a command, a turn step or a battle result:
    /// a held capture choice whose hold no longer applies is settled, then the victory check runs.
    /// The exe runs both from its campaign update `0x008F4120` every frame (the pending-action step
    /// `0x008F7850`, the check `0x0096CD10` at `0x008F487A` after the step loop); the model's state
    /// changes only in those three places, so checking there gives the same verdicts. The one path
    /// that runs the check.
    pub(crate) fn end_update(&mut self, events: &mut Vec<CampaignEvent>) {
        // The report's step ignores the hold while the scripts' looting switch is set (`0x008F8D32`
        // tests +0x9C only when +0x114 is clear, CONFIRMED) and then occupies.
        if self.world.looting_options_disabled_for_human && self.pending_capture.as_ref().is_some_and(|p| p.held) {
            self.release_held_capture(events);
        }
        self.victory_check(events);
    }

    /// The victory check (`0x0096CD10`, CONFIRMED by disassembly unless tagged). It gives its verdict
    /// once ([`TurnState::victory_checked`](super::turn::TurnState::victory_checked), the check's byte
    /// +0x38) and never runs once the campaign is over (campaign +0xFE9).
    ///
    /// Each human faction of the faction list is a winner, a loser or undecided
    /// ([`Self::victory_state`]). With a winner:
    /// 1. The capture report on top of the pending-action stack is held (+0x9C = 1, `0x0096CE7C`):
    ///    its choice is not offered until the player continues ([`Self::continue_game_selected`]).
    ///    A report already offered ignores the flag (its step `0x008F8D00` returns at once when its
    ///    pending byte +0x98 is clear), so ours holds only a choice raised in this update. INFERRED:
    ///    that the check runs before the report's first step when both come in one update (after a
    ///    battle the update's poll of the stack at `0x008F4699` is skipped while the battle sequence
    ///    state is not 0 or 8; the case of a capture without a battle is not traced). The model
    ///    keeps one waiting capture, not the exe's stack of pending actions: a hold while another
    ///    kind of action is on top is not modelled.
    /// 2. Per winner, unless the scripts turned the notification off
    ///    ([`World::victory_notification_disabled`](super::World::victory_notification_disabled),
    ///    +0x125 read at `0x0096CE9C`): in multiplayer (campaign +0x1034) the first other human of
    ///    the faction list gets the defeat event (+0x108 with its flag 1, here its defeat movie) and
    ///    the campaign is over ([`Self::end_campaign`]); then the winner's victory event (+0x120,
    ///    here its movie) with the flag "this ended a multiplayer campaign". INFERRED: the model
    ///    keeps no multiplayer flag, so another human in the list stands for it. PROVISIONAL: the
    ///    exe's shared win (campaign key `spa_napoleon`, `0x008CE390(winner, other)` and both
    ///    factions' conditions of type 5: the other human gets the victory event with flag 1 instead
    ///    of the defeat) is not modelled: it needs the conditions record (BACKLOG §6) and must become
    ///    campaign data, not a key test. Not modelled, no equivalent in ours (no Steam integration):
    ///    for every winner, also with the notification off, the exe calls the campaign achievement
    ///    tracker (`0x009C7660`'s vfunc +0x50, `0x004AFD40`, called at `0x0096D0B1`) with the
    ///    winner's key, the campaign key and campaign numbers. It only sets the tracked faction's
    ///    campaign-won achievement flags on the tracker itself (its bytes +0x9C..+0xC6) and runs the
    ///    tracker's own update `0x0049E5D0` (CONFIRMED by disassembly).
    /// 3. Every loser gets the lost event (+0x138, [`CampaignEvent::CampaignLost`]) and the verdict
    ///    is given, also while another human is undecided.
    ///
    /// With no winner the exe tests the loser count (`0x0096CE47`) and then the undecided count
    /// (`0x0096CE4F` / `0x0096CE57`): with no loser, or while any human is undecided (multiplayer
    /// only), it goes straight to its clean-up `0x0096D115`, past the loser loop `0x0096D0E5` and the
    /// done byte `0x0096D111`: nothing is posted and no verdict is given. Only when every human has
    /// lost do the losers get the lost event, with the verdict. So a loser is told exactly once.
    pub(crate) fn victory_check(&mut self, events: &mut Vec<CampaignEvent>) {
        if self.turn.victory_checked || self.turn.campaign_over {
            return;
        }
        // Nothing is allocated until a faction has won or lost. The exe asks the humans of its
        // faction list: a human id with no faction in the world is in no list and is never asked.
        let mut decided: Vec<(FactionId, VictoryState)> = Vec::new();
        let mut undecided = false;
        for &faction in self.turn.humans.iter().filter(|f| self.world.factions.contains_key(*f)) {
            match self.victory_state(faction) {
                VictoryState::Undecided => undecided = true,
                state => decided.push((faction, state)),
            }
        }
        if decided.is_empty() {
            return;
        }
        let won = decided.iter().any(|(_, s)| *s == VictoryState::Won);
        if !won && undecided {
            return;
        }
        // The exe walks the faction list: winners and losers are told in that order (the list holds
        // every faction of the world, so each decided faction has a place).
        let order = self.world.factions_in_turn_order();
        decided.sort_by_key(|&(f, _)| order.iter().position(|&o| o == f));
        let lost = |events: &mut Vec<CampaignEvent>| {
            events.extend(decided.iter().filter(|(_, s)| *s == VictoryState::Lost).map(|&(faction, _)| CampaignEvent::CampaignLost { faction }));
        };
        if !won {
            lost(events);
            self.turn.victory_checked = true;
            return;
        }
        if let Some(p) = self.pending_capture.as_mut().filter(|p| !p.held) {
            let raised = events.iter().rposition(|e| matches!(e, CampaignEvent::CaptureChoicePending { region, faction } if *region == p.region && *faction == p.faction));
            if let Some(at) = raised {
                events.remove(at);
                p.held = true;
            }
        }
        for &(winner, _) in decided.iter().filter(|(_, s)| *s == VictoryState::Won) {
            if self.world.victory_notification_disabled {
                continue;
            }
            let other = order.iter().copied().find(|&f| f != winner && self.is_human(f));
            if let Some(other) = other {
                // Event +0x108 {other, 1}: the movie listener answers it as it answers a destroyed
                // human's (`0x00A14E80`). UNKNOWN: what that event's other listeners do when the
                // check posts it for a faction still in the game.
                if let Some(movie) = self.outcome_movie(other, CampaignOutcome::Destroyed) {
                    events.push(CampaignEvent::OutcomeMovie { faction: other, movie });
                }
                self.end_campaign();
            }
            if let Some(movie) = self.outcome_movie(winner, CampaignOutcome::Won { multiplayer_over: other.is_some() }) {
                events.push(CampaignEvent::OutcomeMovie { faction: winner, movie });
            }
        }
        lost(events);
        self.turn.victory_checked = true;
    }

    /// The player continues the campaign after the victory screen (`CCQ_CONTINUE_GAME_SELECTED`
    /// `0x00932B40`, CONFIRMED by disassembly: it takes the top of the pending-action stack, asks it
    /// for its capture report (vfunc +0x1C, an object only for a `pending_loot_options`) and clears
    /// the report's hold +0x9C; nothing else). The held capture choice is then offered
    /// ([`CampaignEvent::CaptureChoicePending`]), as the report's next step does. With nothing held it
    /// does nothing.
    pub(crate) fn continue_game_selected(&mut self) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        if self.pending_capture.as_ref().is_some_and(|p| p.held) {
            self.release_held_capture(&mut events);
        }
        events
    }

    /// The held capture report's step once its hold no longer stops it (`0x008F8D00`): a human's
    /// choice is offered, or the settlement is occupied at once while the scripts' looting switch is
    /// set (`0x008F8DBF` → `0x008F8EB3`). The caller has checked that a held capture waits.
    fn release_held_capture(&mut self, events: &mut Vec<CampaignEvent>) {
        let Some(mut p) = self.pending_capture.take() else { return };
        p.held = false;
        if self.world.looting_options_disabled_for_human {
            self.resolve_capture(p, CaptureChoice::Occupy, events);
        } else {
            events.push(CampaignEvent::CaptureChoicePending { region: p.region, faction: p.faction });
            self.pending_capture = Some(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::events::CampaignEvent;
    use super::super::ids::RegionId;
    use super::super::tests::test_model;
    use super::*;

    const A: FactionId = FactionId(1); // test_model's faction A

    fn row(event: &str, culture: &str, movie: &str, id: &str) -> MovieEventRow {
        MovieEventRow { event: event.into(), culture: culture.into(), movie: movie.into(), id: id.into() }
    }

    /// test_model with MADE-UP movie rows and faction A's movie id, culture and government key.
    fn model(rows: Vec<MovieEventRow>, id: &str, culture: &str, government: &str) -> CampaignModel {
        let mut m = test_model();
        let mut rules = (*m.rules).clone();
        rules.movie_events = rows;
        rules.faction_movie_ids.insert("test_faction_a".into(), id.into());
        rules.faction_cultures.insert("test_faction_a".into(), culture.into());
        m.rules = Arc::new(rules);
        m.world.factions.get_mut(&A).expect("faction A").government_key = government.into();
        m
    }

    fn movie(m: &CampaignModel, outcome: CampaignOutcome) -> Option<String> {
        m.outcome_movie(A, outcome).map(|o| o.movie)
    }

    /// The defeat movie is the last row naming the faction's movie id (`0x00F55030` keeps overwriting its
    /// match), whatever its event or culture; it exits to the main menu and shows for the local faction only.
    #[test]
    fn the_defeat_movie_is_the_last_row_with_the_factions_id() {
        let rows = vec![row("game_lost_a", "european", "first.bik", "7"), row("game_lost_b", "middle_east", "other.bik", "8"), row("game_lost_c", "european", "last.bik", "7")];
        let m = model(rows.clone(), "7", "european", "gov_republic");
        let got = m.outcome_movie(A, CampaignOutcome::Destroyed).expect("a row names 7");
        assert_eq!(got, OutcomeMovie { event: "game_lost_c".into(), culture: "european".into(), movie: "last.bik".into(), exit: MovieExit::ExitGame, show_everyone: true, only_local_faction: true });
        assert_eq!(got.exit.script_name(), "ExitGame");
        // No row with the id: no movie (`0x009FBA20` queues nothing); the comparison is case-sensitive.
        assert_eq!(movie(&model(rows.clone(), "9", "european", ""), CampaignOutcome::Destroyed), None);
        // A faction without an id matches the last row without one, as the exe's string comparison does.
        let mut with_blank = rows;
        with_blank.push(row("game_victory_republic", "european", "blank.bik", ""));
        assert_eq!(movie(&model(with_blank.clone(), "", "european", ""), CampaignOutcome::Destroyed).as_deref(), Some("blank.bik"));
        // A faction with no `factions` row (a custom map's) compares the empty string too (`0x00A14E80`).
        let mut unlisted = model(with_blank, "7", "european", "");
        let mut rules = (*unlisted.rules).clone();
        rules.faction_movie_ids.remove("test_faction_a");
        unlisted.rules = Arc::new(rules);
        assert_eq!(movie(&unlisted, CampaignOutcome::Destroyed).as_deref(), Some("blank.bik"));
        // An unknown faction plays nothing.
        assert_eq!(test_model().outcome_movie(FactionId(99), CampaignOutcome::Destroyed), None);
    }

    /// The victory movie (`0x00A14E00` → `0x00F54F80`): the government picks the event (constitutional
    /// monarchy, republic, else absolute monarchy, `gov_empire` included); of its rows, the first of the
    /// faction's culture, else the first `european`, else the first. Single player exits to the victory
    /// popup, a multiplayer win to the main menu; both flags clear.
    #[test]
    fn the_victory_movie_follows_the_government_and_the_culture() {
        let rows = vec![
            row(VICTORY_ABSOLUTE, "middle_east", "abs_me.bik", ""),
            row(VICTORY_ABSOLUTE, "european", "abs_eu.bik", ""),
            row(VICTORY_ABSOLUTE, "european", "abs_eu2.bik", ""),
            row(VICTORY_CONSTITUTIONAL, "middle_east", "con_me.bik", ""),
            row(VICTORY_REPUBLIC, "native", "rep_na.bik", ""),
            row(VICTORY_REPUBLIC, "middle_east", "rep_me.bik", ""),
        ];
        let won = CampaignOutcome::Won { multiplayer_over: false };
        let case = |culture: &str, government: &str| movie(&model(rows.clone(), "", culture, government), won);
        assert_eq!(case("middle_east", "gov_absolute_monarchy").as_deref(), Some("abs_me.bik"), "its culture's row");
        assert_eq!(case("european", "gov_empire").as_deref(), Some("abs_eu.bik"), "an empire plays the absolute event, the first match");
        assert_eq!(case("native", "gov_absolute_monarchy").as_deref(), Some("abs_eu.bik"), "no row of its culture: european");
        assert_eq!(case("european", "gov_constitutional_monarchy").as_deref(), Some("con_me.bik"), "no european row: the first");
        assert_eq!(case("middle_east", "gov_republic").as_deref(), Some("rep_me.bik"));
        assert_eq!(case("european", "gov_republic").as_deref(), Some("rep_na.bik"));
        let single = model(rows.clone(), "", "european", "").outcome_movie(A, won).expect("absolute rows");
        assert_eq!((single.exit, single.show_everyone, single.only_local_faction), (MovieExit::ShowVictoryOptionsPopup, false, false));
        assert_eq!(single.exit.script_name(), "ShowVictoryOptionsPopup");
        let mp = model(rows, "", "european", "").outcome_movie(A, CampaignOutcome::Won { multiplayer_over: true }).expect("absolute rows");
        assert_eq!(mp.exit, MovieExit::ExitGame);
        assert_eq!(movie(&model(Vec::new(), "", "european", ""), won), None, "no row: no movie");
    }

    /// `0x00F54F80`'s `cultures` #2 step: with no row of its own culture, a faction takes the last row
    /// whose culture names its culture in #2, and once one is seen a `european` row no longer replaces the
    /// pick; a row of its own culture still wins at once, wherever it is. MADE-UP rows; the #2 values are
    /// vanilla's (indian → middle_east, egy_middle_east → middle_east, european empty).
    #[test]
    fn the_victory_movie_takes_a_culture_standing_in_for_the_factions() {
        let won = CampaignOutcome::Won { multiplayer_over: false };
        let case = |rows: Vec<MovieEventRow>, culture: &str| {
            let mut m = model(rows, "", culture, "gov_absolute_monarchy");
            let mut rules = (*m.rules).clone();
            for (c, alt) in [("indian", "middle_east"), ("egy_middle_east", "middle_east"), ("european", "")] {
                rules.culture_alternates.insert(c.into(), alt.into());
            }
            m.rules = Arc::new(rules);
            movie(&m, won)
        };
        let r = |culture: &str, movie: &str| row(VICTORY_ABSOLUTE, culture, movie, "");
        let stand_ins = vec![r("native", "na.bik"), r("indian", "ind.bik"), r("european", "eu.bik"), r("egy_middle_east", "egy.bik")];
        assert_eq!(case(stand_ins.clone(), "middle_east").as_deref(), Some("egy.bik"), "the last stand-in wins; european no longer replaces");
        assert_eq!(case(vec![r("native", "na.bik"), r("indian", "ind.bik"), r("european", "eu.bik")], "middle_east").as_deref(), Some("ind.bik"));
        let mut own = stand_ins.clone();
        own.push(r("middle_east", "me.bik"));
        assert_eq!(case(own, "middle_east").as_deref(), Some("me.bik"), "its own culture's row, even after stand-ins");
        assert_eq!(case(stand_ins.clone(), "native").as_deref(), Some("na.bik"), "its own culture first");
        assert_eq!(case(stand_ins, "tribal").as_deref(), Some("eu.bik"), "no own row, no stand-in: european");
        // An empty #2 never stands in for a culture with a key.
        assert_eq!(case(vec![r("native", "na.bik"), r("european", "eu.bik")], "indian").as_deref(), Some("eu.bik"));
    }

    /// A destroyed human gets its defeat movie right after its `FactionDestroyed`; an AI faction none.
    #[test]
    fn a_destroyed_human_plays_its_defeat_movie() {
        let mut m = model(vec![row("game_lost_a", "european", "lost.bik", "7")], "7", "european", "");
        let mut ai = m.clone();
        m.turn.humans = vec![A];
        for r in [10, 12] {
            m.world.regions.get_mut(&RegionId(r)).expect("A's region").owner = FactionId(2);
            ai.world.regions.get_mut(&RegionId(r)).expect("A's region").owner = FactionId(2);
        }
        let mut events = Vec::new();
        m.destroy_faction(A, &mut events);
        let at = events.iter().position(|e| matches!(e, CampaignEvent::FactionDestroyed { faction } if *faction == A)).expect("destroyed");
        match events.get(at + 1) {
            Some(CampaignEvent::OutcomeMovie { faction, movie }) => assert_eq!((*faction, movie.movie.as_str()), (A, "lost.bik")),
            other => panic!("expected the defeat movie, got {other:?}"),
        }
        let mut events = Vec::new();
        ai.destroy_faction(A, &mut events);
        assert!(!events.iter().any(|e| matches!(e, CampaignEvent::OutcomeMovie { .. })));
    }
}

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

use super::ids::FactionId;
use super::world::CampaignModel;

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

/// The culture the victory lookup falls back to (`0x00F54F80`, string `0x0132EE78`).
const FALLBACK_CULTURE: &str = "european";

/// The original's outcome movie for `faction` (the [`OUTCOME_MOVIE`](super::seams::OUTCOME_MOVIE)
/// seam's original rule), or `None` when no row matches (the exe then queues nothing). A query.
///
/// - [`CampaignOutcome::Destroyed`] (`0x00A14E80` → `0x009FBA20` → `0x00F55030`): the **last** row whose
///   #3 equals the faction's movie id (`factions` #39), compared exactly (`CompareUniStrings` through
///   `0x004EFB20`), so a faction without an id (or without a `factions` row) gets the last row without one; exit `ExitGame`, `show_everyone` and
///   `only_call_function_for_local_faction` set (data word +4 = 0x0101).
/// - [`CampaignOutcome::Won`] (`0x00A14E00` → `0x009FBA60` → `0x00F54F80`): the government's victory event
///   (see [`VICTORY_ABSOLUTE`]); of its rows, the one [`victory_row`] picks; exit `ExitGame` when the
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
            let row = victory_row(m, rows.iter().filter(|r| r.event == event), culture)?;
            let exit = if multiplayer_over { MovieExit::ExitGame } else { MovieExit::ShowVictoryOptionsPopup };
            Some(OutcomeMovie { event: row.event.clone(), culture: row.culture.clone(), movie: row.movie.clone(), exit, show_everyone: false, only_local_faction: false })
        }
    }
}

/// The victory event's row for a faction of `culture` (`0x00F54F80`, CONFIRMED by disassembly), walking
/// `rows` in table order: a row of the faction's own culture is taken at once; a row whose culture's
/// `cultures` #2 ([`CampaignRules::culture_alternates`](super::rules::CampaignRules::culture_alternates))
/// is the faction's culture becomes the pick, the last such row winning; otherwise the first row is the
/// pick, which, until such a #2 match is seen, a `european` row replaces unless the pick is already
/// `european`. A faction whose culture is unknown takes only the first / `european` steps.
fn victory_row<'a>(m: &CampaignModel, rows: impl Iterator<Item = &'a MovieEventRow>, culture: Option<&str>) -> Option<&'a MovieEventRow> {
    let mut pick: Option<&MovieEventRow> = None;
    let mut alternate_seen = false;
    for row in rows {
        if culture == Some(row.culture.as_str()) {
            return Some(row);
        }
        let alternate = m.rules.culture_alternates.get(&row.culture).map(String::as_str);
        if culture.is_some() && alternate == culture {
            alternate_seen = true;
            pick = Some(row);
        } else if pick.is_none_or(|p| !alternate_seen && p.culture != FALLBACK_CULTURE && row.culture == FALLBACK_CULTURE) {
            pick = Some(row);
        }
    }
    pick
}

impl CampaignModel {
    /// The movie `faction`'s campaign outcome plays, through the [`OUTCOME_MOVIE`](super::seams::OUTCOME_MOVIE)
    /// seam ([`outcome_movie`] in vanilla). A query.
    pub fn outcome_movie(&self, faction: FactionId, outcome: CampaignOutcome) -> Option<OutcomeMovie> {
        (self.rules.seams.outcome_movie.rule())(self, faction, outcome)
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

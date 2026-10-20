//! Tests of the defeat / victory movie panel's engine side ([`UiScriptHost::campaign_outcome_movie`],
//! `AttachCustomControl`, `CampaignUI.QuitToMainScreen` / `MovieDismissed` / `ContinueGameSelected`).

use ntw_formats::loc::{LocEntry, LocFile, Localisation};
use ntw_sim::campaign::outcome::{MovieExit, OutcomeMovie};

use super::tests::{A, HudSetup, TestHud, test_hud, test_hud_setup};
use super::*;

/// The root records the panel data the engine hands `InitialiseMoviePanel` (a stand-in for
/// layout.root.lua's, which opens `event_movie_panel` with it).
fn record_movie_panel(hud: &TestHud) -> Table {
    let env = hud.host.script_env(hud.root).unwrap();
    hud.host.lua().load("panels = {}\nfunction InitialiseMoviePanel(data) table.insert(panels, data) end").set_environment(env.clone()).exec().unwrap();
    env.get("panels").unwrap()
}

fn defeat() -> OutcomeMovie {
    OutcomeMovie { event: "game_lost_france".into(), culture: "european".into(), movie: "EUR_Lose_FR.bik".into(), exit: MovieExit::ExitGame, show_everyone: true, only_local_faction: true }
}

fn victory() -> OutcomeMovie {
    OutcomeMovie {
        event: "game_victory_absolute_monachy".into(),
        culture: "european".into(),
        movie: "EUR_MON_VIC.bik".into(),
        exit: MovieExit::ShowVictoryOptionsPopup,
        show_everyone: false,
        only_local_faction: false,
    }
}

#[test]
fn an_outcome_movie_opens_the_movie_panel_with_the_exes_data() {
    let hud = test_hud();
    let panels = record_movie_panel(&hud);
    assert!(hud.host.campaign_outcome_movie(A, &defeat()));
    let data: Table = panels.get(1).unwrap();
    assert_eq!(data.get::<String>("bink_control_ptr").unwrap(), "EUR_Lose_FR.bik");
    // No loc string: an empty title, logged once.
    assert_eq!(data.get::<String>("title").unwrap(), "");
    let d: Table = data.get("Data").unwrap();
    assert_eq!(d.get::<String>("ExitFunction").unwrap(), "ExitGame");
    assert!(d.get::<bool>("show_everyone").unwrap());
    assert!(d.get::<bool>("only_call_function_for_local_faction").unwrap());
    assert!(d.get::<bool>("local_faction_involved").unwrap());

    assert!(hud.host.campaign_outcome_movie(A, &victory()));
    let d: Table = panels.get::<Table>(2).unwrap().get("Data").unwrap();
    assert_eq!(d.get::<String>("ExitFunction").unwrap(), "ShowVictoryOptionsPopup");
    assert!(!d.get::<bool>("show_everyone").unwrap());
    let log = hud.host.take_log();
    // The missing title string logs "UNKNOWN no loc string"; any other ERROR / UNKNOWN CampaignUI line fails.
    assert!(!log.iter().any(|l| l.starts_with("ERROR") || l.starts_with("UNKNOWN CampaignUI")), "{log:?}");
    assert_eq!(log.iter().filter(|l| l.contains("movie_event_strings_title_game_lost_franceeuropean")).count(), 1, "{log:?}");
}

/// The title is the row's loc text, `movie_event_strings_title_<event><culture>` (`0x00F57D60`).
#[test]
fn the_movie_panel_title_is_the_rows_loc_text() {
    let mut loc = Localisation::new();
    let entry = |k: &str, t: &str| LocEntry { key: k.into(), text: t.into(), flag: false };
    loc.add(&LocFile {
        version: 1,
        entries: vec![
            entry("movie_event_strings_title_game_lost_franceeuropean", "Faction Destroyed"),
            entry("movie_event_strings_title_game_victory_absolute_monachyeuropean", "Supreme Victory"),
        ],
    });
    let hud = test_hud_setup(GameDatabase::test_fixture(), HudSetup { loc, ..HudSetup::default() });
    let panels = record_movie_panel(&hud);
    assert!(hud.host.campaign_outcome_movie(A, &defeat()));
    assert!(hud.host.campaign_outcome_movie(A, &victory()));
    let title = |i: usize| panels.get::<Table>(i).unwrap().get::<String>("title").unwrap();
    assert_eq!((title(1), title(2)), ("Faction Destroyed".to_owned(), "Supreme Victory".to_owned()));
    assert!(!hud.host.take_log().iter().any(|l| l.contains("movie_event_strings_title")));
}

#[test]
fn another_factions_movie_opens_only_when_shown_to_everyone() {
    let hud = test_hud();
    let panels = record_movie_panel(&hud);
    let other = FactionId(2);
    assert!(!hud.host.campaign_outcome_movie(other, &victory()));
    assert_eq!(panels.raw_len(), 0);
    assert!(hud.host.campaign_outcome_movie(other, &defeat()));
    let d: Table = panels.get::<Table>(1).unwrap().get("Data").unwrap();
    assert!(!d.get::<bool>("local_faction_involved").unwrap());
}

#[test]
fn a_root_without_the_movie_panel_logs_the_movie_it_dropped() {
    let hud = test_hud();
    assert!(!hud.host.campaign_outcome_movie(A, &defeat()));
    assert!(hud.errors().iter().any(|e| e.contains("game_lost_france")), "{:?}", hud.errors());
}

#[test]
fn an_attached_movie_shows_until_the_panel_dismisses_it() {
    let hud = test_hud();
    let env = hud.host.script_env(hud.root).unwrap();
    let run = |s: &str| hud.host.lua().load(s).set_environment(env.clone()).exec().unwrap();
    assert_eq!(hud.host.attached_movie(), None);
    run("UIComponent(Address):AttachCustomControl('EUR_Lose_FR.bik')");
    let (node, movie) = hud.host.attached_movie().unwrap();
    assert_eq!((node, &*movie), (hud.root, "EUR_Lose_FR.bik"));
    run("CampaignUI.MovieDismissed()");
    assert_eq!(hud.host.attached_movie(), None);
    // Anything but a movie is not attached, and says so once.
    run("UIComponent(Address):AttachCustomControl(5) UIComponent(Address):AttachCustomControl({})");
    assert_eq!(hud.host.attached_movie(), None);
    assert_eq!(hud.host.take_log().iter().filter(|l| l.contains("AttachCustomControl")).count(), 1);
}

#[test]
fn quit_to_main_screen_asks_the_game_to_leave_and_continue_does_nothing_yet() {
    let hud = test_hud();
    hud.host.take_campaign_requests();
    hud.host.lua().load("CampaignUI.QuitToMainScreen() CampaignUI.ContinueGameSelected() CampaignUI.ContinueGameSelected()").exec().unwrap();
    assert_eq!(hud.host.take_campaign_requests(), vec![CampaignRequest::QuitToMainScreen]);
    assert_eq!(hud.host.take_log().iter().filter(|l| l.contains("ContinueGameSelected")).count(), 1);
}

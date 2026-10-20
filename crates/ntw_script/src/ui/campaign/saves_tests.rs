//! Tests of the campaign's save functions ([`super::saves`]).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mlua::{Table, Value};
use ntw_formats::loc::{LocEntry, LocFile, Localisation};

use super::super::host::UiRequest;
use super::tests::{A, HudSetup, TestHud, test_hud_setup};
use super::*;

/// A made-up save callback: counts its calls and writes a minimal own-format save (header only, with
/// the season name the HUD gives), so the listing and `latest` see a valid file.
struct FakeSaves {
    calls: Cell<u32>,
    paths: RefCell<Vec<PathBuf>>,
}

fn info(season_name: &str) -> ntw_campaign::CampaignInfo {
    ntw_campaign::CampaignInfo {
        kind: ntw_campaign::FileKind::Save,
        timestamp: 0,
        build_id: String::new(),
        build_version: String::new(),
        header: ntw_campaign::SaveHeader {
            faction_key: "test_faction_a".into(),
            portrait: "ui/portraits/a.tga".into(),
            turn_number: 7,
            year: 1806,
            season_name: season_name.into(),
            flag_path: "ui/flags/a".into(),
            date: None,
            maps: Vec::new(),
        },
        campaign_key: "test_campaign".into(),
        map_key: "test_map".into(),
        players: Vec::new(),
    }
}

impl CampaignSaves for FakeSaves {
    fn current_info(&self, season_name: &str) -> ntw_campaign::CampaignInfo {
        info(season_name)
    }
    fn save(&self, path: &Path, season_name: &str) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        self.paths.borrow_mut().push(path.to_path_buf());
        let bytes = ntw_campaign::own_save::write(&info(season_name), &()).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

/// A fresh, empty user folder for one test (removed when dropped).
struct TempDir(PathBuf);

impl TempDir {
    fn new(test: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("ntw_saves_{}_{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn loc_of(entries: &[(&str, &str)]) -> Localisation {
    let mut loc = Localisation::new();
    loc.add(&LocFile { version: 1, entries: entries.iter().map(|(k, t)| LocEntry { key: (*k).into(), text: (*t).into(), flag: false }).collect() });
    loc
}

/// A HUD of campaign `campaign` saving into `dir`'s `save_games` through a [`FakeSaves`].
fn hud_saving(dir: &TempDir, campaign: &'static str, loc: Localisation) -> (TestHud, Rc<FakeSaves>) {
    let saves = Rc::new(FakeSaves { calls: Cell::new(0), paths: RefCell::new(Vec::new()) });
    let mut setup = HudSetup { campaign, loc, saves: Some(saves.clone() as Rc<dyn CampaignSaves>), ..HudSetup::default() };
    setup.facts.user_dir = Some(dir.0.clone());
    (test_hud_setup(GameDatabase::test_fixture(), setup), saves)
}

fn eval<R: mlua::FromLuaMulti>(hud: &TestHud, code: &str) -> R {
    hud.host.lua().load(code).eval().unwrap()
}

#[test]
fn save_campaign_answers_the_three_shapes_and_asks_before_overwriting() {
    let dir = TempDir::new("shapes");
    let (hud, saves) = hud_saving(&dir, "test_campaign", Localisation::new());
    // A new name: two values, false (no question) and the save's success.
    let (ask, ok): (bool, bool) = eval(&hud, "return CampaignUI.SaveCampaign('first', false)");
    assert!(!ask && ok);
    assert_eq!(saves.paths.borrow().last().unwrap(), &dir.0.join("save_games").join("first.save"));
    // The same name again without overwrite: one value, true, and nothing saved.
    let again: mlua::MultiValue = eval(&hud, "return CampaignUI.SaveCampaign('first', false)");
    assert_eq!(again.len(), 1);
    assert!(matches!(again.front(), Some(Value::Boolean(true))));
    assert_eq!(saves.calls.get(), 1);
    // With overwrite: one value, the save's success, no exists check.
    let over: mlua::MultiValue = eval(&hud, "return CampaignUI.SaveCampaign('first', true)");
    assert_eq!(over.len(), 1);
    assert!(matches!(over.front(), Some(Value::Boolean(true))));
    assert_eq!(saves.calls.get(), 2);
    // A name already ending in .save keeps it as is.
    let _: mlua::MultiValue = eval(&hud, "return CampaignUI.SaveCampaign('second.save', true)");
    assert_eq!(saves.paths.borrow().last().unwrap(), &dir.0.join("save_games").join("second.save"));
    assert!(hud.errors().is_empty());
}

#[test]
fn a_name_the_panel_cannot_type_is_refused_and_logged() {
    let dir = TempDir::new("refused");
    let (hud, saves) = hud_saving(&dir, "test_campaign", Localisation::new());
    let (ask, ok): (bool, bool) = eval(&hud, "return CampaignUI.SaveCampaign('..\\\\out', false)");
    assert!(!ask && !ok);
    let (ask, ok): (bool, bool) = eval(&hud, "return CampaignUI.SaveCampaign('', false)");
    assert!(!ask && !ok);
    assert_eq!(saves.calls.get(), 0);
    assert_eq!(hud.host.take_log().iter().filter(|l| l.contains("refused save name")).count(), 2);
}

#[test]
fn disable_saving_game_closes_can_save_and_every_save() {
    let dir = TempDir::new("disabled");
    let (hud, saves) = hud_saving(&dir, "test_campaign", Localisation::new());
    assert!(eval::<bool>(&hud, "return CampaignUI.CanSave()"));
    hud._scripts.state_mut().model.world.saving_disabled = true;
    assert!(!eval::<bool>(&hud, "return CampaignUI.CanSave()"));
    let (ask, ok): (bool, bool) = eval(&hud, "return CampaignUI.SaveCampaign('x', false)");
    assert!(!ask && !ok);
    let _: () = eval(&hud, "CampaignUI.QuickSave()");
    assert_eq!(saves.calls.get(), 0);
    assert!(hud.host.take_log().iter().any(|l| l.contains("saving is disabled")));
}

#[test]
fn the_default_save_name_is_campaign_or_faction_then_round_and_year() {
    let dir = TempDir::new("default_name");
    let loc = loc_of(&[
        ("campaigns_onscreen_name_eur_napoleon", "Europe"),
        ("random_localisation_strings_string_round_id_early_may", "Early May"),
    ]);
    let (hud, _) = hud_saving(&dir, "eur_napoleon", loc.clone());
    let testland = || ntw_sim::campaign::details::FactionDetails { display_name: "Testland".into(), ..Default::default() };
    hud._scripts.state_mut().model.world.faction_details.insert(A, testland());
    let early_may = ntw_sim::calendar::Date { year: 1805, season: ntw_sim::calendar::Season::Spring as u32, month: 4, half: 0 };
    hud._scripts.state_mut().model.calendar = ntw_sim::calendar::Calendar::new(early_may, 0);
    let set_switch = |hud: &TestHud, on: bool| {
        std::sync::Arc::make_mut(&mut hud._scripts.state_mut().model.rules).features.save_name_uses_campaign_name = on;
    };
    // The campaign's rules switch the on-screen name on (the importer does for the exe's four).
    // The round is the calendar's turn name, read when asked (no HUD update needed).
    set_switch(&hud, true);
    assert_eq!(eval::<String>(&hud, "return CampaignUI.DefaultSaveName()"), "Europe Early May 1805");
    // The same key without the switch: the faction's name, the key itself decides nothing.
    set_switch(&hud, false);
    assert_eq!(eval::<String>(&hud, "return CampaignUI.DefaultSaveName()"), "Testland Early May 1805");

    // Another campaign: the human faction's display name (the save's FACTION record), also when the
    // switch is on but the loc has no on-screen name for it.
    let (hud, _) = hud_saving(&dir, "test_campaign", loc);
    hud._scripts.state_mut().model.world.faction_details.insert(A, testland());
    set_switch(&hud, true);
    hud._scripts.state_mut().model.calendar = ntw_sim::calendar::Calendar::new(early_may, 0);
    assert_eq!(eval::<String>(&hud, "return CampaignUI.DefaultSaveName()"), "Testland Early May 1805");
    // A calendar the exe gives no turn names (here 2 turns a year): the empty round keeps both
    // spaces, as the exe's two formats do.
    hud._scripts.state_mut().model.calendar.turns_per_year = 2;
    assert_eq!(eval::<String>(&hud, "return CampaignUI.DefaultSaveName()"), "Testland  1805");
}

#[test]
fn a_missing_round_name_is_empty_and_logged_once() {
    let dir = TempDir::new("missing_round");
    let (hud, _) = hud_saving(&dir, "test_campaign", Localisation::new());
    let early_may = ntw_sim::calendar::Date { year: 1805, season: ntw_sim::calendar::Season::Spring as u32, month: 4, half: 0 };
    hud._scripts.state_mut().model.calendar = ntw_sim::calendar::Calendar::new(early_may, 0);
    let _ = hud.host.take_log();
    let _: String = eval(&hud, "return CampaignUI.DefaultSaveName()");
    let _: String = eval(&hud, "return CampaignUI.DefaultSaveName()");
    let missing: Vec<String> = hud.host.take_log().into_iter().filter(|l| l.contains("round_id_early_may")).collect();
    assert_eq!(missing.len(), 1, "{missing:?}");
}

#[test]
fn a_name_listed_from_the_original_folder_asks_before_saving() {
    let ours = TempDir::new("overwrite_ours");
    let original = TempDir::new("overwrite_original");
    let saves = Rc::new(FakeSaves { calls: Cell::new(0), paths: RefCell::new(Vec::new()) });
    let mut setup = HudSetup { saves: Some(saves.clone() as Rc<dyn CampaignSaves>), ..HudSetup::default() };
    setup.facts.user_dir = Some(ours.0.clone());
    setup.facts.original_user_dir = Some(original.0.clone());
    let hud = test_hud_setup(GameDatabase::test_fixture(), setup);
    std::fs::create_dir_all(original.0.join("save_games")).unwrap();
    std::fs::write(original.0.join("save_games").join("theirs.save"), b"x").unwrap();
    // The panel lists the original's save, so saving under its name asks first.
    let listed: mlua::MultiValue = eval(&hud, "return CampaignUI.SaveCampaign('theirs', false)");
    assert_eq!(listed.len(), 1);
    assert!(matches!(listed.front(), Some(Value::Boolean(true))));
    assert_eq!(saves.calls.get(), 0);
    // Confirmed: written to our folder; the original's file is left alone.
    assert!(eval::<bool>(&hud, "return CampaignUI.SaveCampaign('theirs', true)"));
    assert_eq!(saves.paths.borrow().as_slice(), [ours.0.join("save_games").join("theirs.save")]);
    assert_eq!(std::fs::read(original.0.join("save_games").join("theirs.save")).unwrap(), b"x");
}

#[test]
fn get_current_game_info_is_the_running_campaigns_header() {
    let dir = TempDir::new("current_info");
    let loc = loc_of(&[("seasons_onscreen_season_winter", "Hiver")]);
    let (hud, _) = hud_saving(&dir, "test_campaign", loc);
    hud._scripts.state_mut().model.calendar.date.season = ntw_sim::calendar::Season::Winter as u32;
    let t: Table = eval(&hud, "return CampaignUI.GetCurrentGameInfo()");
    assert_eq!(t.get::<String>("Faction").unwrap(), "test_faction_a");
    assert_eq!(t.get::<String>("LeaderPortrait").unwrap(), "ui/portraits/a.tga");
    assert_eq!(t.get::<String>("FlagPath").unwrap(), "ui/flags/a");
    assert_eq!(t.get::<u32>("TimePlayed").unwrap(), 7);
    assert_eq!(t.get::<u32>("Year").unwrap(), 1806);
    // The header's season is the calendar season's on-screen name (`0x00888320` → `0x008ED110`).
    assert_eq!(t.get::<String>("Season").unwrap(), "Hiver");
}

/// The save header's season is the loc on-screen name the HUD sends (`0x00888320` stores
/// `0x008ED110`'s result), and the details table shows a header's season as it is stored
/// (`0x008982C0` copies it; review: ours looked up `seasons_onscreen_<stored text>`, a key the loc
/// never has, and showed the stored text only by falling back to it).
#[test]
fn a_save_keeps_the_seasons_on_screen_name_and_shows_it_as_stored() {
    let dir = TempDir::new("season_name");
    let loc = loc_of(&[("seasons_onscreen_season_spring", "Printemps"), ("seasons_onscreen_printemps", "not looked up")]);
    let (hud, saves) = hud_saving(&dir, "test_campaign", loc);
    hud._scripts.state_mut().model.calendar.date.season = ntw_sim::calendar::Season::Spring as u32;
    assert!(eval::<bool>(&hud, "return CampaignUI.SaveCampaign('spring', true)"));
    let path = saves.paths.borrow().last().unwrap().clone();
    let read = ntw_campaign::read_info(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(read.header.season_name, "Printemps");
    let t: Table = hud.host.lua().load("return CampaignUI.GetExtendedSaveGameInfo(...)").call(path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(t.get::<String>("Season").unwrap(), "Printemps");
    assert!(hud.errors().is_empty());
}

#[test]
fn quick_save_writes_the_loc_named_save_and_quick_load_loads_the_newest() {
    let dir = TempDir::new("quick");
    let loc = loc_of(&[("random_localisation_strings_string_quick_save", "quick_save")]);
    let (hud, saves) = hud_saving(&dir, "test_campaign", loc);
    hud.host.take_requests();
    let _: () = eval(&hud, "CampaignUI.QuickLoad()");
    assert!(hud.host.take_requests().is_empty(), "no save yet");
    assert!(hud.host.take_log().iter().any(|l| l.contains("QuickLoad: no save")));

    let _: () = eval(&hud, "CampaignUI.QuickSave()");
    let quick = dir.0.join("save_games").join("quick_save.save");
    assert_eq!(saves.paths.borrow().as_slice(), std::slice::from_ref(&quick));
    let _: () = eval(&hud, "CampaignUI.QuickLoad()");
    assert_eq!(hud.host.take_requests(), vec![UiRequest::LoadCampaign(quick)]);
}

#[test]
fn a_single_player_save_has_no_multiplayer_side() {
    let dir = TempDir::new("mp");
    let (hud, _) = hud_saving(&dir, "test_campaign", Localisation::new());
    assert!(eval::<bool>(&hud, "return CampaignUI.MultiplayerPlayersName() == nil"));
    assert!(!eval::<bool>(&hud, "return CampaignUI.MultiplayerPlayerOutOfTurn()"));
    let _: () = eval(&hud, "CampaignUI.MultiplayerSaveConfirm()");
    assert!(hud.errors().is_empty());
}

#[test]
fn without_a_game_to_save_the_calls_log_and_fail() {
    let mut setup = HudSetup::default();
    let dir = TempDir::new("no_saver");
    setup.facts.user_dir = Some(dir.0.clone());
    let hud = test_hud_setup(GameDatabase::test_fixture(), setup);
    let (ask, ok): (bool, bool) = eval(&hud, "return CampaignUI.SaveCampaign('x', false)");
    assert!(!ask && !ok);
    assert!(eval::<bool>(&hud, "return CampaignUI.GetCurrentGameInfo() == nil"));
    let log = hud.host.take_log();
    assert!(log.iter().any(|l| l.contains("no game to save")));
    assert!(log.iter().any(|l| l.contains("no game to describe")));
}

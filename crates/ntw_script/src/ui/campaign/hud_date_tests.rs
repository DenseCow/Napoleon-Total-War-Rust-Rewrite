//! Tests of the HUD treasury and date update ([`CampaignUi`]'s `campaign_update_funds`, the exe's
//! `0x009C1AD0`) and the date getters that share its rules.

use mlua::Table;
use ntw_formats::loc::{LocEntry, LocFile, Localisation};

use super::tests::{HudSetup, TestHud, test_hud_setup};
use super::*;

fn loc_of(entries: &[(&str, &str)]) -> Localisation {
    let mut loc = Localisation::new();
    loc.add(&LocFile { version: 1, entries: entries.iter().map(|(k, t)| LocEntry { key: (*k).into(), text: (*t).into(), flag: false }).collect() });
    loc
}

/// A HUD at Early May 1805 (spring), 6 turns played, with the season and round names in its loc;
/// both HUD updates are recorded into the root's global `sent` as `name` plus their table.
fn hud() -> TestHud {
    let loc = loc_of(&[("seasons_onscreen_season_spring", "Spring"), ("random_localisation_strings_string_round_id_early_may", "Early May")]);
    let hud = test_hud_setup(GameDatabase::test_fixture(), HudSetup { loc, ..HudSetup::default() });
    let early_may = ntw_sim::calendar::Date { year: 1805, season: ntw_sim::calendar::Season::Spring as u32, month: 4, half: 0 };
    hud._scripts.state_mut().model.calendar = ntw_sim::calendar::Calendar::new(early_may, 6);
    hud.host
        .lua()
        .load(
            "function UpdateFactionFundsAndDate(t) sent = { name = 'date', t = t } end\n\
             function UpdateFactionFundsAndTurn(t) sent = { name = 'turn', t = t } end",
        )
        .set_environment(root_env(&hud))
        .exec()
        .unwrap();
    hud
}

/// The root script's environment, where the engine looks its globals up.
fn root_env(hud: &TestHud) -> Table {
    hud.host.script_env(hud.root).expect("the root has scripts")
}

/// The last update's name and table.
fn sent(hud: &TestHud) -> (String, Table) {
    let sent: Table = root_env(hud).get("sent").unwrap();
    (sent.get("name").unwrap(), sent.get("t").unwrap())
}

/// The table's keys, sorted.
fn keys(t: &Table) -> Vec<String> {
    let mut k: Vec<String> = t.pairs::<String, mlua::Value>().map(|p| p.unwrap().0).collect();
    k.sort();
    k
}

#[test]
fn the_date_update_sends_numbers_the_season_name_and_the_round() {
    let hud = hud();
    hud.host.campaign_update_funds();
    let (name, t) = sent(&hud);
    assert_eq!(name, "date");
    assert_eq!(keys(&t), ["funds", "round_description", "season", "season_string", "year"]);
    // 0x0044DE40 pushes funds, season and year as numbers.
    assert!(matches!(t.get::<mlua::Value>("funds").unwrap(), mlua::Value::Number(_) | mlua::Value::Integer(_)));
    assert_eq!(t.get::<i64>("funds").unwrap(), 1000);
    assert_eq!(t.get::<u32>("season").unwrap(), 2);
    assert!(matches!(t.get::<mlua::Value>("year").unwrap(), mlua::Value::Number(_) | mlua::Value::Integer(_)));
    assert_eq!(t.get::<u32>("year").unwrap(), 1805);
    assert_eq!(t.get::<String>("season_string").unwrap(), "Spring");
    assert_eq!(t.get::<String>("round_description").unwrap(), "Early May");
    assert!(!hud.host.lua().load("return CampaignUI.DisplayingTurns()").eval::<bool>().unwrap());
    assert_eq!(hud.host.lua().load("return CampaignUI.CurrentYear()").eval::<(u32, bool)>().unwrap(), (1805, false));
}

/// The turns display switch (`display_turns`, +0xD1) takes the other branch: the turn number
/// (turns elapsed + 1), no year and no round description; `CurrentYear` answers the turn number.
#[test]
fn the_turns_display_switch_sends_the_turn_number_instead() {
    let hud = hud();
    hud._scripts.state_mut().model.world.display_turns = true;
    hud.host.campaign_update_funds();
    let (name, t) = sent(&hud);
    assert_eq!(name, "turn");
    assert_eq!(keys(&t), ["funds", "round", "season", "season_string"]);
    assert_eq!(t.get::<i64>("funds").unwrap(), 1000);
    assert_eq!(t.get::<u32>("season").unwrap(), 2);
    assert_eq!(t.get::<String>("season_string").unwrap(), "Spring");
    assert_eq!(t.get::<u32>("round").unwrap(), 7);
    assert!(hud.host.lua().load("return CampaignUI.DisplayingTurns()").eval::<bool>().unwrap());
    assert_eq!(hud.host.lua().load("return CampaignUI.CurrentYear()").eval::<(u32, bool)>().unwrap(), (7, true));
}

/// A season name missing from the loc is "" and logged once (in the update and in
/// `CurrentSeasonString` alike: one rule); an unknown season code too.
#[test]
fn a_missing_season_name_is_empty_and_logged_once() {
    let hud = hud();
    hud._scripts.state_mut().model.calendar.date.season = ntw_sim::calendar::Season::Winter as u32;
    let _ = hud.host.take_log();
    hud.host.campaign_update_funds();
    assert_eq!(sent(&hud).1.get::<String>("season_string").unwrap(), "");
    assert_eq!(hud.host.lua().load("return CampaignUI.CurrentSeasonString()").eval::<String>().unwrap(), "");
    let missing: Vec<String> = hud.host.take_log().into_iter().filter(|l| l.contains("seasons_onscreen_season_winter")).collect();
    assert_eq!(missing.len(), 1, "{missing:?}");
    hud._scripts.state_mut().model.calendar.date.season = 9;
    hud.host.campaign_update_funds();
    hud.host.campaign_update_funds();
    let unknown: Vec<String> = hud.host.take_log().into_iter().filter(|l| l.contains("season code 9")).collect();
    assert_eq!(unknown.len(), 1, "{unknown:?}");
}

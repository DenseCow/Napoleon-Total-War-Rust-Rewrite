//! Run the ORIGINAL front-end UI scripts (root.lua, main.main.lua, the button templates) from the
//! player's install (read-only) against our UI tree. Skipped when the install is not there.
//! See the script log with `cargo test -p ntw_script --test frontend_ui -- --nocapture`.

use std::path::PathBuf;

use ntw_formats::loc::Localisation;
use ntw_script::ScriptSource;
use ntw_script::ui::{FrontEndFacts, PointerEvent, UiScriptHost};
use ntw_script::ui::test_support::no_errors;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

#[test]
fn frontend_scripts_build_the_main_menu() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { campaign_saves_exist: true, spanish_campaign: false, game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").expect("layout loads");
    let log = host.take_log();
    for l in &log {
        println!("{l}");
    }
    assert!(log.iter().all(|l| !l.starts_with("ERROR")), "script errors, see log above");
    // root.lua's InitState → TransitionTo("main") built and adopted the main page ...
    let world = host.world();
    let main = world.find(root, "main").expect("main page adopted by root.lua");
    // ... and main.main.lua showed the default button group only.
    let visible = |id: &str| world.get(world.find(main, id).unwrap()).unwrap().visible;
    assert!(visible("button_group_default"));
    assert!(!visible("single_player_expanded"));
    assert!(!visible("multiplayer_expanded"));
    let version = world.find(main, "version_number").unwrap();
    assert_eq!(world.get(version).unwrap().current().unwrap().text, "test");
    drop(world);

    // Clicking "Single Player" in the default group runs its g_click_callback → ShowButtons("singleplayer").
    let sp = {
        let w = host.world();
        let group = w.find(main, "button_group_default").unwrap();
        w.find(group, "single_player").unwrap()
    };
    host.pointer(sp, PointerEvent::Enter);
    host.pointer(sp, PointerEvent::LeftDown);
    host.pointer(sp, PointerEvent::LeftUp);
    for l in host.take_log() {
        println!("{l}");
    }
    let world = host.world();
    let visible = |id: &str| world.get(world.find(main, id).unwrap()).unwrap().visible;
    assert!(visible("single_player_expanded"));
    assert!(!visible("button_group_default"));
}

#[test]
fn quit_button_binding_reaches_frontend_quit() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let facts = FrontEndFacts { campaign_saves_exist: false, spanish_campaign: false, game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, Localisation::new(), facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    // The Quit button's layout binding is "call Root.LuaCall, Quit" → root.lua Quit() → FrontEnd.Quit().
    let quit = host.world().find(root, "quit").unwrap();
    host.pointer(quit, PointerEvent::LeftUp);
    assert_eq!(host.take_requests(), vec![ntw_script::ui::UiRequest::Quit]);
}

#[test]
fn hovering_never_changes_page() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        return;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let facts = FrontEndFacts { campaign_saves_exist: true, spanish_campaign: false, game_version: "test".into(), ..Default::default() };
    let host = UiScriptHost::new(source, Localisation::new(), facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    let ids: Vec<_> = host.world().ids().collect();
    for id in ids {
        for e in [PointerEvent::Enter, PointerEvent::Leave] {
            host.pointer(id, e);
        }
    }
    let w = host.world();
    let pages: Vec<String> = w.get(root).unwrap().children.iter().map(|&c| w.get(c).unwrap().data.id.clone()).collect();
    assert_eq!(pages, vec!["layout".to_string(), "main".to_string()]);
}

/// A host on the real front end with clicks by component id (first visible match).
fn page_host() -> Option<(UiScriptHost, usize)> {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let original = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("The Creative Assembly").join("Napoleon"));
    // user_dir None: preference changes stay in memory (never written by tests).
    let facts = FrontEndFacts { campaign_saves_exist: true, game_version: "test".into(), original_user_dir: original, user_dir: None, nap_unlock: 1, ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    host.take_log();
    Some((host, root))
}

fn click(host: &UiScriptHost, root: usize, id: &str) {
    let mut target = None;
    host.world().visit_visible(root, &mut |n, node| {
        if target.is_none() && node.data.id == id {
            target = Some(n);
        }
    });
    let t = target.unwrap_or_else(|| panic!("no visible component {id}"));
    for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
        host.pointer(t, e);
    }
}

/// Clicks `group` first unless `id` is already shown (main.main.lua toggles its button groups,
/// and after Back the main page keeps the group it had open).
fn click_in_group(host: &UiScriptHost, root: usize, group: &str, id: &str) {
    let mut shown = false;
    host.world().visit_visible(root, &mut |_, node| shown |= node.data.id == id);
    if !shown {
        click(host, root, group);
    }
    click(host, root, id);
}

#[test]
fn single_player_pages_open_and_back_returns_to_main() {
    let Some((host, root)) = page_host() else { return };
    for page in ["load_game", "episodic_campaign", "napoleon_battles", "grand_campaign"] {
        click_in_group(&host, root, "single_player", page);
        no_errors(&host);
        assert!(host.world().find(root, "main").and_then(|m| host.world().get(m).map(|n| n.parent.is_none())).unwrap_or(true), "main page left the tree on {page}");
        // ESCAPE = root.lua's TransitionBack.
        assert!(host.key("ESCAPE"));
        no_errors(&host);
        let main = host.world().find(root, "main").expect("back on main");
        assert_eq!(host.world().get(main).unwrap().parent, Some(root));
    }
}

#[test]
fn historical_battle_start_requests_the_battle_map() {
    let Some((host, root)) = page_host() else { return };
    for id in ["single_player", "napoleon_battles", "NHB_Arcole", "button_start"] {
        click(&host, root, id);
    }
    no_errors(&host);
    assert_eq!(
        host.take_requests(),
        vec![ntw_script::ui::UiRequest::StartBattle { battle: "NHB_Arcole".into(), map: Some("hb_arcole".into()) }]
    );
}

#[test]
fn coalition_campaign_start_requests_the_chosen_faction() {
    let Some((host, root)) = page_host() else { return };
    for id in ["single_player", "grand_campaign", "britain", "button_forward"] {
        click(&host, root, id);
    }
    no_errors(&host);
    assert_eq!(
        host.take_requests(),
        vec![ntw_script::ui::UiRequest::StartCampaign { campaign: "mp_eur_napoleon".into(), faction: "britain".into() }]
    );
}

#[test]
fn options_tabs_open_without_script_errors() {
    let Some((host, root)) = page_host() else { return };
    for tab in ["graphics", "sound", "controls", "ui"] {
        click_in_group(&host, root, "options", tab);
        no_errors(&host);
        assert!(host.key("ESCAPE"));
    }
}

/// The load-game page (sp_load_game.lua) on two made-up save folders under `target`: ours
/// (NapoleonRust's) and a stand-in for the original's. Ours are listed first and win on equal
/// names; selecting a save shows its header (year, season, the theatre map); Load hands the
/// chosen save's full path to the game although the page passes only its name.
#[test]
fn load_game_page_lists_both_folders_and_loads_by_name() {
    let dir = data_dir();
    let startpos = dir.join(r"campaigns\eur_napoleon\startpos.esf");
    if !startpos.is_file() {
        eprintln!("skipped: no install");
        return;
    }
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("load_game_page");
    let _ = std::fs::remove_dir_all(&base);
    let (ours, orig) = (base.join("ours"), base.join("original"));
    for (d, names) in [(&ours, &["A game.save", "B game.save"][..]), (&orig, &["A game.save", "C original.save", "notes.txt"][..])] {
        std::fs::create_dir_all(d.join("save_games")).unwrap();
        for n in names {
            std::fs::copy(&startpos, d.join("save_games").join(n)).unwrap();
        }
    }
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { campaign_saves_exist: true, game_version: "test".into(), original_user_dir: Some(orig.clone()), user_dir: Some(ours.clone()), nap_unlock: 1, ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    host.take_log();
    click(&host, root, "single_player");
    click(&host, root, "load_game");
    no_errors(&host);
    let mut rows = Vec::new();
    host.world().visit_visible(root, &mut |_, n| {
        if n.data.id == "name" && n.current().is_some_and(|s| s.text.contains("game") || s.text.contains("original")) {
            rows.push(n.current().unwrap().text.clone());
        }
    });
    rows.sort();
    assert_eq!(rows, vec!["A game", "B game", "C original"]);
    // The newest is selected on entry; pick the original's save and load it.
    let mut entry = None;
    host.world().visit_visible(root, &mut |id, n| {
        if n.data.id.starts_with("entry") && entry.is_none() {
            let mut name = String::new();
            for &c in &n.children {
                if let Some(ch) = host.world().get(c).filter(|c| c.data.id == "name") {
                    name = ch.current().map(|s| s.text.clone()).unwrap_or_default();
                }
            }
            if name == "C original" {
                entry = Some(id);
            }
        }
    });
    let entry = entry.expect("row of the original's save");
    for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
        host.pointer(entry, e);
    }
    no_errors(&host);
    // The header's details: year and the theatre picture as a run-time image.
    let date = host.world().find(root, "dy_date").and_then(|d| host.world().get(d).and_then(|n| n.current().map(|s| s.text.clone())));
    assert_eq!(date.as_deref(), Some("1805"));
    assert!(host.world().runtime_images.get("save_map_europe_main").is_some_and(|i| i.width == 605 && i.height == 300 && i.rgba().len() == 605 * 300 * 4));
    click(&host, root, "button_load");
    no_errors(&host);
    assert_eq!(host.take_requests(), vec![ntw_script::ui::UiRequest::LoadCampaign(orig.join("save_games").join("C original.save"))]);
    let _ = std::fs::remove_dir_all(&base);
}

/// Resting the pointer on a component with tooltip text shows root.lua's "Tooltip" template with
/// the text's first part (template.tooltip.lua cycles the `||` parts), fitted to the text by its
/// InitState after SetState (`0x01035B30` calls InitState on every SetState), its frame edges
/// following the new size; leaving hides it.
#[test]
fn front_end_tooltips_show_and_fit_the_text() {
    let Some((host, root)) = page_host() else { return };
    click(&host, root, "single_player");
    click(&host, root, "napoleon_battles");
    let mut back = None;
    host.world().visit_visible(root, &mut |n, node| {
        if back.is_none() && node.data.id == "button_back" {
            back = Some(n);
        }
    });
    let back = back.expect("back button");
    host.set_cursor_position(40.0, 900.0);
    host.hover(Some(back));
    no_errors(&host);
    let w = host.world();
    let tip = w.find(root, "Tooltip").and_then(|t| w.get(t)).expect("tooltip created");
    assert!(tip.visible);
    let text = tip.current().map(|s| s.text.clone()).unwrap_or_default();
    assert_eq!(Some(text.as_str()), host.tooltip_text(back).as_deref().map(|t| t.split("||").next().unwrap_or("")));
    // Fitted (the template starts at 573 x 387 and SetText makes it 200 x 200 first).
    assert!(tip.rect.w < 573.0 && tip.rect.h < 200.0 && tip.rect.h > 16.0, "{:?}", tip.rect);
    // The top edge spans the box between the corners.
    let t = tip.children.iter().filter_map(|&c| w.get(c)).find(|c| c.data.id == "t").unwrap();
    assert_eq!((t.rect.x, t.rect.w), (tip.rect.x + 8.0, tip.rect.w - 16.0));
    drop(w);
    host.hover(None);
    let w = host.world();
    assert!(!w.find(root, "Tooltip").and_then(|t| w.get(t)).unwrap().visible);
}

/// Options → Show Credits: `FrontEnd.BuildCredits` builds one page per `<page>` of
/// `text/credits.xml` under `credits_list`; credits.lua shows the first, centred.
#[test]
fn credits_pages_are_built_from_the_xml() {
    let Some((host, root)) = page_host() else { return };
    click(&host, root, "options");
    click(&host, root, "credits");
    no_errors(&host);
    let w = host.world();
    let list = w.find(root, "credits_list").expect("credits page");
    let pages: Vec<_> = w.get(list).unwrap().children.iter().filter_map(|&c| w.get(c)).collect();
    assert_eq!(pages.len(), 21, "one container per <page>");
    assert!(pages[0].visible && pages[1..].iter().all(|p| !p.visible));
    let first: Vec<String> = pages[0].children.iter().filter_map(|&c| w.get(c)).filter_map(|n| n.current().map(|s| s.text.clone())).collect();
    assert_eq!(&first[..2], ["Studio Director", "Tim Heaton"]);
    // Centred vertically in the list (credits.lua: y = list y + (list height - page height) / 2).
    let (l, p) = (w.get(list).unwrap().rect, pages[0].rect);
    assert!((p.y - (l.y + (l.h - p.h) / 2.0)).abs() < 1.0, "{l:?} {p:?}");
}

/// Play Battle → Land: the map and settings page (sp_battle2) fills its dropdowns from the DB
/// (weather and time of day from the map's sky types, wind levels) and its map list with the
/// players column.
#[test]
fn custom_battle_settings_page_fills_from_the_db() {
    let Some((host, root)) = page_host() else { return };
    for id in ["single_player", "sp_battle", "button_classic_battle"] {
        click(&host, root, id);
    }
    no_errors(&host);
    let log = host.take_log();
    assert!(!log.iter().any(|l| l.starts_with("UNKNOWN FrontEnd")), "{log:?}");
    let text = |id: &str| {
        let w = host.world();
        let d = w.find(root, id)?;
        let s = w.find(d, "dy_selected_txt")?;
        w.get(s).and_then(|n| n.current().map(|s| s.text.clone()))
    };
    assert_eq!(text("dropdown_weather").as_deref(), Some("Dry"));
    assert_eq!(text("dropdown_time_of_day").as_deref(), Some("Morning"));
    let w = host.world();
    let mut players = 0;
    w.visit_visible(root, &mut |_, n| players += usize::from(n.data.id == "players" && n.current().is_some_and(|s| s.text.contains(" v "))));
    assert!(players > 10, "{players}");
}

/// The army page's engine data (FRONTEND_PAGES.md "Custom battle"): a faction's recruitable land
/// units by category, the default army within the army size's funds, experience costs.
#[test]
fn custom_battle_army_data() {
    let Some((host, _root)) = page_host() else { return };
    let lua = host.lua();
    let n: i64 = lua.load("local u = FrontEnd.RecruitableUnits('austria', false, 1, 4, 5000, 1, false); return #u").eval().unwrap();
    assert!(n > 3, "infantry units: {n}");
    let first: String = lua.load("return FrontEnd.RecruitableUnits('austria', false, 1, 4, 5000, 1, false)[1].Key").eval().unwrap();
    assert!(first.starts_with("Inf_"), "{first}");
    let cav: bool = lua.load("for _, u in ipairs(FrontEnd.RecruitableUnits('austria', false, 1, 2, 5000, 1, false)) do if not string.find(u.Key, 'Cav_') and not string.find(u.Key, 'Gen_') then return false end end return true").eval().unwrap();
    assert!(cav);
    let (count, cost): (i64, i64) = lua.load("local p = FrontEnd.RetrieveArmyPresets('austria', 'classic', 0, 2, 1.0).balanced; return #p.Units, p.Cost").eval().unwrap();
    assert!(count > 3 && cost > 0 && cost < 5000, "{count} units, cost {cost}");
    let xp: i64 = lua.load("return FrontEnd.MPExperienceTables(false)[2].FixedCost").eval().unwrap();
    assert_eq!(xp, 20);
}

/// Play Battle > Custom Battle > Host > OK with the default setup: StartBattle asks for the chosen
/// map with both armies (the human's first), each army's units and experience as the army
/// pages hold them.
#[test]
fn custom_battle_start_requests_both_armies() {
    let Some((host, root)) = page_host() else { return };
    for id in ["single_player", "sp_battle", "button_classic_battle", "button_host", "button_ok"] {
        click(&host, root, id);
    }
    no_errors(&host);
    let requests = host.take_requests();
    let [ntw_script::ui::UiRequest::StartCustomBattle { battle, map, armies }] = requests.as_slice() else {
        panic!("{requests:?}")
    };
    assert!(!battle.is_empty() && map.is_some(), "{battle} {map:?}");
    assert_eq!(armies.len(), 2);
    assert!(armies[0].human && !armies[1].human);
    assert_eq!((armies[0].alliance, armies[1].alliance), (0, 1));
    for a in armies {
        assert!(!a.faction.is_empty() && !a.units.is_empty(), "{a:?}");
        assert!(a.units.iter().all(|(k, xp)| !k.is_empty() && *xp <= 9), "{a:?}");
    }
}

/// Army setup files (FRONTEND_PAGES.md "Custom battle"): the default army is saved, refused
/// without overwrite once the file exists, loaded back with its cards and cost, validated, and
/// listed by `EnumerateArmySetups` only for its era and army size. Written in a test folder
/// standing in for NapoleonRust's user folder; a path outside it is refused.
#[test]
fn army_setups_save_load_validate_and_list() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let user = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("army_setup_user");
    let _ = std::fs::remove_dir_all(&user);
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { game_version: "test".into(), user_dir: Some(user.clone()), nap_unlock: 1, ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let lua = host.lua();
    lua.globals().set("outside", user.parent().unwrap().join("outside.army_setup").display().to_string()).unwrap();
    let script = r#"
        local ext, folder = FrontEnd.FileExtenstionAndPathForWriteClass("army_setup")
        local path = folder .. "test army" .. ext
        local setup = FrontEnd.RetrieveArmyPresets("austria", "classic", 1, 2, 1.0).balanced
        setup.Era, setup.ArmySize, setup.IsHuman, setup.Name = 2, 10000, true, "test army"
        setup.Units[1].IsGeneral = true
        local r = {}
        r.first = {FrontEnd.SaveArmySetup(setup, path, false)}
        r.again = {FrontEnd.SaveArmySetup(setup, path, false)}
        r.over = {FrontEnd.SaveArmySetup(setup, path, true)}
        r.outside = {FrontEnd.SaveArmySetup(setup, outside, false)}
        local loaded = FrontEnd.LoadArmySetup(path)
        r.units, r.cost, r.saved_cost, r.faction = #loaded.Units, loaded.Cost, setup.Cost, loaded.Faction
        r.general, r.tag, r.total = loaded.Units[1].IsGeneral, loaded[1].Tag, loaded.TotalCards
        r.keys_match = true
        for i, u in ipairs(setup.Units) do
            if loaded.Units[i].Key ~= u.Key or loaded.Units[i].Experience ~= u.Experience then r.keys_match = false end
        end
        local _, vcost, verr = FrontEnd.ValidateArmySetup(loaded, false, 2)
        r.vcost, r.verr = vcost, verr
        local _, _, naval_err = FrontEnd.ValidateArmySetup(FrontEnd.LoadArmySetup(path), true, 2)
        r.naval_err = naval_err
        local factions = FrontEnd.FactionListForBattles(false, 2)
        r.listed = #FrontEnd.EnumerateArmySetups(folder, "*" .. ext, 2, 10000, 20, factions)
        r.other_size = #FrontEnd.EnumerateArmySetups(folder, "*" .. ext, 2, 5000, 20, factions)
        r.other_era = #FrontEnd.EnumerateArmySetups(folder, "*" .. ext, 0, 10000, 20, factions)
        r.dir = #DirectoryUtils.EnumerateDirectory(folder, "*" .. ext)
        r.missing = FrontEnd.LoadArmySetup(folder .. "nothing" .. ext) == nil
        return r
    "#;
    let r: mlua::Table = lua.load(script).eval().unwrap();
    let list = |k: &str| r.get::<Vec<bool>>(k).unwrap();
    assert_eq!(list("first"), [false, true]);
    assert_eq!(list("again"), [true]);
    assert_eq!(list("over"), [true]);
    assert_eq!(list("outside"), [false, false]);
    assert!(user.join("army_setups").join("test army.army_setup").is_file());
    let n = |k: &str| r.get::<i64>(k).unwrap();
    assert!(n("units") > 0);
    assert_eq!(n("cost"), n("saved_cost"));
    assert_eq!(n("vcost"), n("cost"));
    assert_eq!(n("total"), n("units"));
    assert!(r.get::<Option<String>>("verr").unwrap().is_none());
    assert_eq!(r.get::<String>("naval_err").unwrap(), "Invalid unit type found in setup");
    assert_eq!(r.get::<String>("faction").unwrap(), "austria");
    assert!(r.get::<bool>("general").unwrap() && r.get::<bool>("keys_match").unwrap());
    assert_eq!(r.get::<String>("tag").unwrap(), "inf");
    assert_eq!((n("listed"), n("other_size"), n("other_era"), n("dir")), (1, 0, 0, 1));
    assert!(r.get::<bool>("missing").unwrap());
    let _ = std::fs::remove_dir_all(&user);
}

/// The battle settings page keeps its last settings (FRONTEND_PAGES.md "Custom battle"): leaving
/// it for the armies page saves `.sp_default.battle_preferences` (SaveBattleSetup), which reads
/// back with the chosen map; coming back to the page loads it (LoadBattleSetup) without script
/// errors. Written in a test folder standing in for NapoleonRust's user folder.
#[test]
fn battle_settings_are_kept_in_the_default_preferences_file() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let user = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("battle_prefs_user");
    let _ = std::fs::remove_dir_all(&user);
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { campaign_saves_exist: true, game_version: "test".into(), user_dir: Some(user.clone()), nap_unlock: 1, ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    host.take_log();
    for id in ["single_player", "sp_battle", "button_classic_battle", "button_host"] {
        click(&host, root, id);
    }
    no_errors(&host);
    let path = user.join("battle_preferences").join(".sp_default.battle_preferences");
    let f = ntw_script::ui::army_file::BattlePrefsFile::read(&std::fs::read(&path).expect("default settings saved")).unwrap();
    // Hidden, as the exe leaves it (attribute 2), and so not listed by the requesters.
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        assert_ne!(std::fs::metadata(&path).unwrap().file_attributes() & 2, 0, "the default settings file is hidden");
        let listed: i64 = host.lua().load(format!("return #DirectoryUtils.EnumerateDirectory([[{}]], \"*.battle_preferences\")", path.parent().unwrap().display())).eval().unwrap();
        assert_eq!(listed, 0);
    }
    eprintln!("{f:?}");
    assert_eq!(f.kind, "classic");
    assert!(!f.key.is_empty() && f.allowed_funds > 0, "{f:?}");
    let lua = host.lua();
    lua.globals().set("path", path.display().to_string()).unwrap();
    let (key, funds, teams): (String, i64, i64) = lua.load("local s = FrontEnd.LoadBattleSetup(path) return s.map.Key, s.allowed_funds, #s.map.Teams").eval().unwrap();
    assert_eq!((key.as_str(), funds, teams), (f.key.as_str(), i64::from(f.allowed_funds), 2));
    // Back to the settings page: it loads the defaults.
    assert!(host.key("ESCAPE"));
    no_errors(&host);
    let _ = std::fs::remove_dir_all(&user);
}

/// Play Battle > Load Battle (`button_capture_point`): a saved battle setup (the default settings
/// plus an army from the presets for each team, written by SaveBattleSetup) is listed by the
/// file requester; choosing it and Accept loads it (LoadBattleSetup) and opens the armies page.
#[test]
fn saved_battle_setup_loads_from_the_requester() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let user = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("battle_load_user");
    let _ = std::fs::remove_dir_all(&user);
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { campaign_saves_exist: true, game_version: "test".into(), user_dir: Some(user.clone()), nap_unlock: 1, ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    host.take_log();
    // The default settings file, then a named setup with armies.
    for id in ["single_player", "sp_battle", "button_classic_battle", "button_host"] {
        click(&host, root, id);
    }
    assert!(host.key("ESCAPE"));
    assert!(host.key("ESCAPE"));
    no_errors(&host);
    let script = r#"
        local ext, folder = FrontEnd.FileExtenstionAndPathForWriteClass("battle_prefs")
        local s = FrontEnd.LoadBattleSetup(folder .. ".sp_default" .. ext)
        local prefs = s
        prefs.Armies = {}
        for team, faction in ipairs({"france", "austria"}) do
            local a = FrontEnd.RetrieveArmyPresets(faction, "classic", 0, 2, 1.0).balanced
            a.Era, a.ArmySize, a.IsHuman = 2, 5000, team == 1
            prefs.Armies[team] = {a}
        end
        return FrontEnd.SaveBattleSetup(prefs, folder .. "my battle" .. ext, false)
    "#;
    let (exists, ok): (bool, bool) = host.lua().load(script).eval().unwrap();
    assert!(!exists && ok);
    let f = ntw_script::ui::army_file::BattlePrefsFile::read(&std::fs::read(user.join("battle_preferences").join("my battle.battle_preferences")).unwrap()).unwrap();
    assert_eq!(f.teams.iter().map(|t| t.armies.len()).collect::<Vec<_>>(), [1, 1]);
    assert_eq!(f.teams[1].armies[0].faction, "austria");
    click(&host, root, "button_capture_point");
    no_errors(&host);
    // The requester lists "my battle"; its row is the text's parent.
    let mut row = None;
    host.world().visit_visible(root, &mut |id, n| {
        if row.is_none() && n.current().is_some_and(|s| s.text == "my battle") {
            row = Some(id);
        }
    });
    let text = row.expect("my battle is listed");
    let row = host.world().get(text).and_then(|n| n.parent).unwrap();
    for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
        host.pointer(row, e);
    }
    no_errors(&host);
    click(&host, root, "button_ok");
    no_errors(&host);
    // The armies page with the loaded armies: France's and Austria's army boxes.
    let mut texts = Vec::new();
    host.world().visit_visible(root, &mut |_, n| {
        if let Some(s) = n.current() {
            texts.push(s.text.clone());
        }
    });
    assert!(texts.iter().any(|t| t == "Army Setup"), "armies page shown");
    // Starting the battle fights the loaded armies.
    click(&host, root, "button_ok");
    no_errors(&host);
    let requests = host.take_requests();
    let [ntw_script::ui::UiRequest::StartCustomBattle { armies, .. }] = requests.as_slice() else { panic!("{requests:?}") };
    let factions: Vec<&str> = armies.iter().map(|a| a.faction.as_str()).collect();
    assert_eq!(factions, ["france", "austria"]);
    let _ = std::fs::remove_dir_all(&user);
}

/// Text entry (template.text_input.lua on the engine's focus, OnKey and CharacterInput): Save
/// army opens the requester; clicking its name field gives it the focus, typing (with a
/// Backspace and a Latin-1 letter) edits it with the caret, and RETURN saves the setup under the
/// typed name.
#[test]
fn typed_file_name_saves_the_army() {
    let dir = data_dir();
    if !dir.join("data.pack").is_file() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let user = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("text_entry_user");
    let _ = std::fs::remove_dir_all(&user);
    let source = ScriptSource::from_install(&dir).expect("open packs");
    let vfs = ntw_formats::pack::Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let facts = FrontEndFacts { campaign_saves_exist: true, game_version: "test".into(), user_dir: Some(user.clone()), nap_unlock: 1, ..Default::default() };
    let host = UiScriptHost::new(source, loc, facts, (1280.0, 960.0), ntw_sim::limits::GameLimits::default()).unwrap();
    let root = host.load_root_layout("data/ui/frontend ui/layout").unwrap();
    host.take_log();
    for id in ["single_player", "sp_battle", "button_classic_battle", "button_host", "button_save", "input_name"] {
        click(&host, root, id);
    }
    no_errors(&host);
    let input = host.world().find(root, "input_name").unwrap();
    assert_eq!(host.focus(), Some(input), "the name field has the focus");
    assert!(host.text_input("Armee"));
    assert!(host.key("BACK"));
    assert!(host.text_input("é 1"));
    no_errors(&host);
    let shown = host.world().get(input).and_then(|n| n.current().map(|s| s.text.clone())).unwrap();
    assert_eq!(shown, "Armeé 1|", "typed text with the caret");
    assert!(host.key("RETURN"));
    no_errors(&host);
    assert_eq!(host.focus(), None);
    let saved = user.join("army_setups").join("Armeé 1.army_setup");
    assert!(saved.is_file(), "{:?}", std::fs::read_dir(user.join("army_setups")).map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>()));
    let _ = std::fs::remove_dir_all(&user);
}

/// GenerateShipName(faction, class): names from the faction's `ship_names` group, a different
/// one each time; an unknown faction gets the exe's "Invalid faction".
#[test]
fn ship_names_come_from_the_faction_group() {
    let Some((host, _root)) = page_host() else { return };
    let (a, b, bad): (String, String, String) = host
        .lua()
        .load(r#"return FrontEnd.GenerateShipName("france", 23), FrontEnd.GenerateShipName("france", 23), FrontEnd.GenerateShipName("nobody", 23)"#)
        .eval()
        .unwrap();
    eprintln!("{a} / {b}");
    assert!(!a.is_empty() && a != b);
    assert_eq!(bad, "Invalid faction");
}

/// The option sliders' end buttons step the slider through their "ClickDown" state's exit
/// function `UpdateSlider` (inline script: `Parent.Value` +/- `stepSize`, then the parent's
/// `Update`), so the front end runs layout state functions too, as the exe does for every UI
/// (`0x01035620` tests no mode, CONFIRMED).
#[test]
fn option_slider_buttons_step_through_their_exit_function() {
    let Some((host, root)) = page_host() else { return };
    click_in_group(&host, root, "options", "controls");
    no_errors(&host);
    let mut right = None;
    host.world().visit_visible(root, &mut |n, node| {
        if right.is_none() && node.data.id == "slider_right" {
            right = Some(n);
        }
    });
    let right = right.expect("a visible slider_right on the controls tab (camera speeds)");
    // The slider's Value, read the way the end button's own script reads it (run-time properties
    // live with the scripts).
    let env = host.script_env(right).expect("the end button's scripts");
    let value = |host: &UiScriptHost| -> f64 {
        host.lua().load("return Component.GetProperty('Parent.Value')").set_environment(env.clone()).eval::<f64>().unwrap()
    };
    let step: f64 = host.world().get(right).unwrap().data.properties.iter().find(|(k, _)| k == "stepSize").map(|(_, v)| v.parse().unwrap()).unwrap();
    let before = value(&host);
    for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
        host.pointer(right, e);
    }
    no_errors(&host);
    let after = value(&host);
    assert!(after > before && after <= before + step, "Value {before} -> {after} (step {step})");
}

/// A click's handler runs BEFORE the click's state transition, so `CurrentState()` inside it is
/// the state the click starts from (CONFIRMED: `0x0102E340` fires `OnMouseLClickUp` and only then
/// calls `0x01035620`, which sets the new state; `CurrentState` (`0x01014300`) reads the current
/// state). The shipped checkboxes rely on it: template.checkbox.lua's `NotifySelected`, bound to
/// `OnMouseLClickUp`, reports "selected" when the box is in "down" (pressed while unticked) and
/// the transition then ticks it. Each click must report what the box becomes.
#[test]
fn option_checkboxes_report_the_state_their_click_leads_to() {
    let Some((host, root)) = page_host() else { return };
    click_in_group(&host, root, "options", "graphics");
    no_errors(&host);
    let mut boxes = Vec::new();
    host.world().visit_visible(root, &mut |n, node| {
        if node.data.id == "checkbox_windowed" {
            boxes.push(n);
        }
    });
    let cb = *boxes.first().expect("the windowed checkbox on the graphics tab");
    // Record what NotifySelected reports (its g_notify_func hook, template.checkbox.lua).
    let env = host.script_env(cb).expect("the checkbox's scripts");
    host.lua().load("reported = {}\ng_notify_func = function(a, selected) reported[#reported + 1] = selected end").set_environment(env.clone()).exec().unwrap();
    for _ in 0..2 {
        for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
            host.pointer(cb, e);
        }
        no_errors(&host);
        let reported: mlua::Table = env.get("reported").unwrap();
        let last: bool = reported.get(reported.raw_len()).unwrap();
        let state = host.world().get(cb).unwrap().state_name().to_owned();
        assert_eq!(last, state == "selected", "reported {last}, the box is now {state}");
    }
}

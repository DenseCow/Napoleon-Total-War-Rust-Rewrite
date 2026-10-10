//! NapoleonRust: the game program.
//!
//! How the program is organised (see `docs/DESIGN.md` §1, "model vs display"):
//! - `ntw_sim` (another crate) is the **model**. It holds the game rules and owns the
//!   authoritative battle state. It knows nothing about Bevy.
//! - This crate is the **display**. It opens the window, draws what the model says,
//!   and turns mouse and keyboard input into orders for the model.
//!
//! Modules:
//! - `config`:   where the original game is installed.
//! - `data`:     loads the game database (unit stats, morale and fatigue tables).
//! - `frontend`: the main menu, built from the original UI layout files (the default mode).
//! - `battle`:   the Phase 5 vertical-slice battle, a test harness: `--battle`.
//! - `battle_ai`: the battle AI hook (`ntw_ai`) for the non-player army: `--ai on|off`.
//! - `model_viewer`: `--view-model <name>` shows original 3D models from the install.
//! - `campaign`: the campaign map (`--campaign <name>`): terrain, borders, settlements, armies.
//! - `campaign_ai`: the campaign AI hook (`ntw_ai`) for the non-human factions: `--campaign-ai on|off`.
//! - `audio`:    music and sounds from the original sound tables (hooks: `audio::PlaySound`, `UiSound`, ...).
//! - `terrain`:  the battle map (`--battle --battle-map <name>`): terrain mesh, ground heights, placed objects.
//! - `video`:    Bink movies (our own decoder): intro (`--intro`), front-end background (`--frontend-movie`), cutscenes (hook `video::PlayMovie`).
//!
//! Game modes mirror the original's mode handlers (`0x00485B90`, DESIGN §3.4) as a Bevy state.

mod audio;
mod battle;
mod battle_ai;
mod campaign;
mod campaign_ai;
mod config;
mod data;
mod frontend;
mod model_viewer;
mod soldiers;
mod terrain;
mod video;

use bevy::prelude::*;

/// The top-level game mode, like the original's MVC mode handlers.
/// `CampaignLoad` and `Campaign` are not implemented yet.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameMode {
    /// The front end / main menu (the original starts here).
    #[default]
    FrontEnd,
    /// Loading a campaign (planned).
    #[allow(dead_code)]
    CampaignLoad,
    /// The campaign map (planned).
    #[allow(dead_code)]
    Campaign,
    /// A land battle. Today only reachable through the `--battle` test harness.
    Battle,
    /// The start-up movies (`--intro`), then the front end.
    Intro,
}

fn main() {
    // `cargo run -p napoleon -- --view-model american_church` opens the model viewer instead.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Text language: `--language fr`, else our own setting, else the install's language.txt.
    config::apply_language_setting(&args);
    // Mods: `--no-mods`, `--mods <dir>`, `--user-script <file>`; `--list-mods` prints them and exits.
    if config::apply_mod_setting(&args) {
        return;
    }
    if let Some(viewer_args) = model_viewer::ViewerArgs::parse(&args) {
        model_viewer::run(viewer_args);
        return;
    }
    // `--battle` starts the battle slice directly (test harness, not part of the original flow).
    let campaign = campaign::start_from_args(&args);
    // `--battle-key <KEY>` (a `battles` key, e.g. NHB_Arcole) fights that historical battle on its
    // own map with its own armies (test harness; implies `--battle`).
    let battle_start = args.iter().position(|a| a == "--battle-key").and_then(|i| args.get(i + 1)).and_then(|k| {
        let found = battle::BattleStart::from_battles_table(k);
        if found.is_none() {
            eprintln!("--battle-key {k}: not in the battles table");
        }
        found
    });
    let start = if args.iter().any(|a| a == "--battle") || battle_start.is_some() {
        GameMode::Battle
    } else if campaign.is_some() {
        // `--campaign <name>` opens the campaign map directly (test harness).
        GameMode::Campaign
    } else if video::intro_enabled(&args) {
        // The start-up movies first, as the original (`--no-intro` to skip them; harness runs skip them).
        GameMode::Intro
    } else {
        GameMode::FrontEnd
    };
    // The front-end layouts are authored at 1280x960 (their root state size).
    let screenshot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    // The battle HUD layouts are authored at 1280x960 too.
    let resolution = if start == GameMode::Campaign { (1280, 720) } else { (1280, 960) };
    let mut app = App::new();
    app
        // `DefaultPlugins` gives us a window, rendering, input, time and more.
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            // Harness runs (--screenshot) use a fixed-size window so every capture has the same layout.
            primary_window: Some(Window {
                title: "NapoleonRust".into(),
                resolution: resolution.into(),
                resizable: screenshot.is_none(),
                ..default()
            }),
            ..default()
        }))
        .insert_state(start)
        // Background colour: sky behind the 3D battlefield, black behind the menus.
        // Bevy colour values run from 0.0 to 1.0.
        .insert_resource(ClearColor(if start == GameMode::Battle { Color::srgb(0.55, 0.65, 0.78) } else { Color::BLACK }))
        // Our own plugins. A "plugin" is a bundle of resources and systems.
        .add_plugins((data::DataPlugin, frontend::FrontEndPlugin, battle::BattlePlugin, campaign::CampaignPlugin, audio::GameAudioPlugin))
        // `--ai on|off` (default on): the battle AI controls the non-player army.
        .add_plugins(battle_ai::BattleAiPlugin::from_args(&args))
        // `--campaign-ai on|off` (default on): the campaign AI plays the non-human factions.
        .add_plugins(campaign_ai::CampaignAiPlugin)
        // Movies: `--intro`, `--frontend-movie`, and the `video::PlayMovie` hook.
        .add_plugins(video::VideoPlugin::from_args(&args));
    if let Some(c) = campaign {
        app.insert_resource(c);
    }
    // The battle map: with `--battle`, `--battle-map <name>` (default `terrain::DEFAULT_MAP`) is
    // loaded at start; from the front end, the chosen battle's map is loaded when it starts.
    let mut terrain_plugin = terrain::TerrainPlugin::from_args(&args);
    terrain_plugin.load_at_start = start == GameMode::Battle;
    if let Some(b) = battle_start {
        if let Some(m) = b.map.clone().filter(|_| terrain::map_arg(&args).is_none()) {
            terrain_plugin.map = m;
        }
        app.insert_resource(b);
    }
    app.add_plugins(terrain_plugin);
    if let Some(ids) = args.iter().position(|a| a == "--ui-click").and_then(|i| args.get(i + 1)) {
        app.insert_resource(frontend::UiClicks { ids: ids.split(',').map(str::to_owned).collect(), secs: 0.0 });
    }
    if let Some(path) = screenshot {
        app.insert_resource(frontend::AutoScreenshot { path, secs: 0.0, taken: false });
    }
    app.run();
}

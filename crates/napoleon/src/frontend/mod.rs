//! The front end (main menu and its pages), recreated from the original UI layout files and run
//! by the original UI scripts.
//!
//! What happens (evidence in `analysis/frontend/FRONTEND_FLOW.md`, `UI_SCRIPTING.md` and
//! `FRONTEND_PAGES.md`):
//! 1. Like the original's front-end UI object (`0x004581B0`), we load the layout
//!    `data/UI/FrontEnd UI/layout` as the root of the UI tree.
//! 2. `ntw_script::ui::UiScriptHost` runs the components' original Lua: `layout_scripts/root.lua`
//!    whose `InitState` calls `TransitionTo("main")`. Every page change (Single Player pages,
//!    Options, Back) is root.lua's own `TransitionTo` / `TransitionBack`, page stack included.
//! 3. Mouse input drives each state's transition map and the Lua click handlers; ESCAPE/RETURN go
//!    to the component that stole the key (root.lua's ESCAPE = Back); every frame is an
//!    `OnUpdatePulse` (headings slide in, ...).
//! 4. Requests from the scripts: Quit; StartBattle (switches to the battle on the chosen map, the
//!    same maps as `--battle-map`); StartCampaign (opens the campaign map of the chosen campaign); LoadCampaign (no save loading yet: logged).
//!
//! The background movie `Frontend2.bik` plays in `movie_bg` with `--frontend-movie` (`crate::video`);
//! by default the layout's own still image shows (PLACEHOLDER until the movie is checked against
//! the original). Not done yet: tooltips, sounds, list clipping, and the `FrontEnd.*` engine
//! functions of the multiplayer pages (they log `UNKNOWN`).

pub(crate) mod render;

use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow, SystemCursorIcon};
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;
use ntw_script::ScriptSource;
use ntw_script::ui::{FrontEndFacts, NodeId, PointerEvent, UiRequest, UiScriptHost};

use crate::GameMode;
use crate::config;
use render::{UiAssets, UiSprite, spawn_world};

/// The UI script host plus pointer bookkeeping. Lua is single-threaded, so this is a
/// non-`Send` resource used only from the main thread.
pub struct FrontEndUi {
    host: UiScriptHost,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    drawn_generation: u64,
    screen: Vec2,
    /// Milliseconds since the front end started (the `OnUpdatePulse` clock).
    clock_ms: f64,
    cursor: Option<String>,
}

/// Front-end state and systems.
pub struct FrontEndPlugin;

impl Plugin for FrontEndPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameMode::FrontEnd), enter)
            .add_systems(OnExit(GameMode::FrontEnd), leave)
            .add_systems(
                Update,
                (resize, scripted_clicks, pointer, keys, pulse, script_output, cursor, redraw)
                    .chain()
                    .run_if(in_state(GameMode::FrontEnd)),
            )
            .add_systems(Update, auto_screenshot);
    }
}

/// Our or the original's `save_games` folder holds at least one `.save` (W3 §1; the load-game
/// page lists both, `ntw_script::ui::frontend::SaveFolders`).
fn campaign_saves_exist() -> bool {
    [config::user_dir(), config::original_user_dir()].into_iter().flatten().any(|d| {
        std::fs::read_dir(d.join("save_games"))
            .map(|it| it.flatten().any(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("save"))))
            .unwrap_or(false)
    })
}

fn enter(world: &mut World) {
    world.insert_resource(ClearColor(Color::BLACK));
    world.spawn((Camera2d, DespawnOnExit(GameMode::FrontEnd)));
    let dir = config::game_data_dir();
    let (vfs, source) = match (Vfs::open_install(&dir), ScriptSource::from_install(&dir)) {
        (Ok(v), Ok(s)) => (v, s),
        (Err(e), _) | (_, Err(e)) => {
            error!("The front end needs the original game files, but {} could not be opened: {e}", dir.display());
            return;
        }
    };
    let loc = Localisation::from_vfs(&vfs).unwrap_or_else(|e| {
        warn!("Localisation not loaded: {e}");
        Localisation::new()
    });
    let facts = FrontEndFacts {
        campaign_saves_exist: campaign_saves_exist(),
        // PROVISIONAL: DLC ownership comes from Steam in the original. The install has the
        // `spain_main` page, but without Steam we cannot tell whether the player owns it.
        spanish_campaign: false,
        // PROVISIONAL: the exact text of `FrontEnd.GameVersion()` is not known yet.
        game_version: "1.3.0".into(),
        original_user_dir: config::original_user_dir(),
        user_dir: config::user_dir(),
        nap_unlock: config::nap_unlock(),
    };
    let screen = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(world)
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(1280.0, 960.0));
    let host = match UiScriptHost::new(source, loc, facts, (screen.x, screen.y)) {
        Ok(h) => h,
        Err(e) => {
            error!("UI script host failed to start: {e}");
            return;
        }
    };
    if let Err(e) = host.load_root_layout("data/ui/frontend ui/layout") {
        error!("Could not load the front-end layout: {e}");
        return;
    }
    world.insert_resource(UiAssets::new(vfs).with_loose_files(&dir));
    world.insert_non_send(FrontEndUi { host, hovered: None, pressed: None, drawn_generation: u64::MAX, screen, clock_ms: 0.0, cursor: None });
}

/// Leaving the front end (a battle starts): drop the UI and put the normal cursor back.
fn leave(world: &mut World) {
    world.remove_non_send::<FrontEndUi>();
    world.remove_resource::<UiAssets>();
    if let Ok(e) = world.query_filtered::<Entity, With<PrimaryWindow>>().single(world) {
        world.entity_mut(e).insert(CursorIcon::System(SystemCursorIcon::Default));
    }
}

/// Tells the scripts' world about the window size.
fn resize(ui: Option<NonSendMut<FrontEndUi>>, window: Query<&Window, With<PrimaryWindow>>) {
    let (Some(mut ui), Ok(window)) = (ui, window.single()) else { return };
    let size = Vec2::new(window.width(), window.height());
    if ui.screen != size {
        ui.screen = size;
        ui.host.set_screen(size.x, size.y);
    }
}

/// Mouse → transition-map events and Lua click handlers.
///
/// In test-harness runs (`--screenshot`, `--ui-click`) the real mouse is ignored: the front end
/// only changes page on a click (startup and hovering never do, see ntw_script's frontend_ui
/// tests), so a stray OS click or the cursor resting where the new window opens must not change
/// what the harness captures.
fn pointer(
    ui: Option<NonSendMut<FrontEndUi>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    harness: Option<Res<AutoScreenshot>>,
    clicks: Option<Res<UiClicks>>,
    mut sounds: MessageWriter<crate::audio::UiSound>,
) {
    if harness.is_some() || clicks.is_some() {
        return;
    }
    let (Some(mut ui), Ok(window)) = (ui, window.single()) else { return };
    if let Some(p) = window.cursor_position() {
        ui.host.set_cursor_position(p.x, p.y);
    }
    let hit = window.cursor_position().and_then(|p| ui.host.hit(p.x, p.y));
    if hit != ui.hovered {
        if let Some(old) = ui.hovered {
            ui.host.pointer(old, PointerEvent::Leave);
        }
        if let Some(new) = hit {
            ui.host.pointer(new, PointerEvent::Enter);
            ui_event_sound(&ui.host, new, crate::audio::UiEvent::MouseOn, &mut sounds);
        }
        ui.hovered = hit;
        // The tooltip of the component under the pointer (root.lua's SetTooltipText).
        ui.host.hover(hit);
    }
    if mouse.just_released(MouseButton::Right)
        && let Some(h) = hit
    {
        ui_event_sound(&ui.host, h, crate::audio::UiEvent::RClickUp, &mut sounds);
    }
    if mouse.just_pressed(MouseButton::Left) {
        ui.pressed = hit;
        if let Some(h) = hit {
            ui.host.pointer(h, PointerEvent::LeftDown);
        }
    }
    if mouse.just_released(MouseButton::Left)
        && let Some(p) = ui.pressed.take()
    {
        if hit == Some(p) {
            click_sound(&ui.host, p, &mut sounds);
            ui.host.pointer(p, PointerEvent::LeftUp);
        } else {
            ui.host.pointer(p, PointerEvent::LeftUpElsewhere);
        }
    }
}

/// The engine's key names (CONFIRMED in the scripts: ESCAPE and RETURN, which the pages steal;
/// BACK, DELETE, LEFT, RIGHT, HOME, END, which template.text_input.lua edits with).
fn key_name(k: KeyCode) -> Option<&'static str> {
    Some(match k {
        KeyCode::Escape => "ESCAPE",
        KeyCode::Enter | KeyCode::NumpadEnter => "RETURN",
        KeyCode::Backspace => "BACK",
        KeyCode::Delete => "DELETE",
        KeyCode::ArrowLeft => "LEFT",
        KeyCode::ArrowRight => "RIGHT",
        KeyCode::Home => "HOME",
        KeyCode::End => "END",
        _ => return None,
    })
}

/// Keyboard → the focused text field (typed characters and editing keys, with key repeat) and
/// `OnKey` of the component that stole the key (root.lua: ESCAPE = Back).
fn keys(ui: Option<NonSend<FrontEndUi>>, mut input: MessageReader<bevy::input::keyboard::KeyboardInput>, clicks: Option<Res<UiClicks>>) {
    let Some(ui) = ui else { return };
    if clicks.is_some() {
        input.clear();
        return;
    }
    for ev in input.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        if let Some(name) = key_name(ev.key_code) {
            // ESCAPE / RETURN act once per press (no repeat); editing keys repeat.
            if !(ev.repeat && matches!(name, "ESCAPE" | "RETURN")) {
                ui.host.key(name);
            }
        } else if let Some(text) = &ev.text {
            ui.host.text_input(text);
        }
    }
}

/// One `OnUpdatePulse` per frame.
fn pulse(ui: Option<NonSendMut<FrontEndUi>>, time: Res<Time>) {
    let Some(mut ui) = ui else { return };
    ui.clock_ms += f64::from(time.delta_secs()) * 1000.0;
    let t = ui.clock_ms;
    ui.host.pulse(t);
}

/// Forwards the scripts' log lines and requests (quit, battle start, ...).
fn script_output(ui: Option<NonSend<FrontEndUi>>, mut exit: MessageWriter<AppExit>, mut commands: Commands) {
    let Some(ui) = ui else { return };
    for line in ui.host.take_log() {
        if line.starts_with("ERROR") {
            warn!("UI script: {line}");
        } else {
            debug!("UI script: {line}");
        }
    }
    for r in ui.host.take_requests() {
        match r {
            UiRequest::Quit => {
                info!("Front end: Quit");
                exit.write(AppExit::Success);
            }
            UiRequest::StartBattle { battle, map } => {
                // The chosen battle's armies come from its specification file (`battle::BattleStart`,
                // read when the battle starts); the map is its terrain preset, loaded the same way
                // as `--battle-map`.
                let map = map.unwrap_or_else(|| crate::terrain::DEFAULT_MAP.to_owned());
                info!("Front end: start battle {battle} on map {map}");
                let start = crate::battle::BattleStart::from_battles_table(&battle);
                commands.queue(move |world: &mut World| {
                    match start {
                        Some(s) => world.insert_resource(s),
                        None => warn!("Battle {battle} is not in the battles table; the test armies fight"),
                    }
                    crate::terrain::load_battle_map(world, &map);
                    world.resource_mut::<NextState<GameMode>>().set(GameMode::Battle);
                });
            }
            UiRequest::StartCustomBattle { battle, map, armies } => {
                // A custom battle (Play Battle > Custom Battle): the map is the chosen battle's
                // terrain, the armies are the army pages' (`battle::BattleStart::custom`); the
                // record's specification file is not read (it holds no armies for these maps).
                let map = map.unwrap_or_else(|| crate::terrain::DEFAULT_MAP.to_owned());
                info!("Front end: start custom battle {battle} on map {map}, {} armies", armies.len());
                let start = crate::battle::BattleStart { key: battle, spec: None, map: Some(map.clone()), technologies: Vec::new(), custom: armies };
                commands.queue(move |world: &mut World| {
                    world.insert_resource(start);
                    crate::terrain::load_battle_map(world, &map);
                    world.resource_mut::<NextState<GameMode>>().set(GameMode::Battle);
                });
            }
            UiRequest::LoadCampaign(path) => {
                info!("Front end: load campaign {}", path.display());
                commands.insert_resource(crate::campaign::CampaignStart { campaign: String::new(), faction: None, save: Some(path) });
                commands.queue(|world: &mut World| world.resource_mut::<NextState<GameMode>>().set(GameMode::Campaign));
            }
            UiRequest::StartCampaign { campaign, faction } => {
                // The campaign mode's entry point (crate::campaign): CampaignStart + GameMode::Campaign.
                // PROVISIONAL: the campaign options are not passed on yet (the faction is).
                info!("Front end: start campaign {campaign} as {faction}");
                commands.insert_resource(crate::campaign::CampaignStart { campaign, faction: Some(faction), save: None });
                commands.queue(|world: &mut World| world.resource_mut::<NextState<GameMode>>().set(GameMode::Campaign));
            }
        }
    }
}

/// The scripts' cursor (`Cursor("busy")` during a page change). PROVISIONAL: the system's wait
/// cursor stands in for the original's animated cursor art (`template.cursor`).
fn cursor(mut commands: Commands, ui: Option<NonSendMut<FrontEndUi>>, window: Query<Entity, With<PrimaryWindow>>) {
    let (Some(mut ui), Ok(win)) = (ui, window.single()) else { return };
    let now = ui.host.cursor();
    if now != ui.cursor {
        let icon = if now.as_deref() == Some("busy") { SystemCursorIcon::Wait } else { SystemCursorIcon::Default };
        commands.entity(win).insert(CursorIcon::System(icon));
        ui.cursor = now;
    }
}

/// Rebuilds all sprites when the UI tree changed.
fn redraw(
    mut commands: Commands,
    ui: Option<NonSendMut<FrontEndUi>>,
    assets: Option<ResMut<UiAssets>>,
    mut images: ResMut<Assets<Image>>,
    sprites: Query<Entity, With<UiSprite>>,
    movie: Option<Res<crate::video::FrontEndMovieTexture>>,
) {
    let (Some(mut ui), Some(mut assets)) = (ui, assets) else { return };
    // `--frontend-movie`: the background movie plays in `movie_bg` (crate::video).
    let want = movie.map(|m| m.0.clone());
    if assets.movies.get("movie_bg") != want.as_ref() {
        match want {
            Some(h) => assets.movies.insert("movie_bg".into(), h),
            None => assets.movies.remove("movie_bg"),
        };
        ui.drawn_generation = u64::MAX;
    }
    let generation = ui.host.world().generation;
    if generation == ui.drawn_generation {
        return;
    }
    for e in &sprites {
        commands.entity(e).despawn();
    }
    if let Some(root) = ui.host.root() {
        spawn_world(&ui.host.world(), root, &mut commands, &mut assets, &mut images, ui.screen, 1.0);
    }
    ui.drawn_generation = generation;
}

/// `--screenshot <file.png>`: save one picture once the scene has settled (4 s after start, or
/// 2 s after the last `--ui-click`), then quit (used to check the menu against the original
/// without opening the window by hand).
#[derive(Resource)]
pub struct AutoScreenshot {
    pub path: std::path::PathBuf,
    pub secs: f32,
    pub taken: bool,
}

fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    shot: Option<ResMut<AutoScreenshot>>,
    clicks: Option<Res<UiClicks>>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    let Some(mut shot) = shot else { return };
    shot.secs += time.delta_secs();
    let settle = match &clicks {
        Some(c) if !c.ids.is_empty() => {
            shot.secs = 0.0;
            return;
        }
        Some(_) => 2.0,
        None => 4.0,
    };
    if !shot.taken && shot.secs >= settle {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(shot.path.clone()));
        shot.taken = true;
    }
    if shot.taken && shot.secs >= settle + 1.0 {
        exit.write(AppExit::Success);
    }
}

/// `--ui-click a,b,...`: clicks the first visible component with each id, one per second, as a
/// test harness for screenshots (e.g. `--ui-click single_player --screenshot sp.png`).
/// `key:ESCAPE` presses a key instead (e.g. `--ui-click single_player,load_game,key:ESCAPE`).
#[derive(Resource)]
pub struct UiClicks {
    pub ids: Vec<String>,
    pub secs: f32,
}

fn scripted_clicks(
    ui: Option<NonSendMut<FrontEndUi>>,
    clicks: Option<ResMut<UiClicks>>,
    time: Res<Time>,
    mut sounds: MessageWriter<crate::audio::UiSound>,
) {
    let (Some(ui), Some(mut clicks)) = (ui, clicks) else { return };
    clicks.secs += time.delta_secs();
    if clicks.ids.is_empty() || clicks.secs < 1.0 {
        return;
    }
    clicks.secs = 0.0;
    let id = clicks.ids.remove(0);
    // `type:<text>`: typed into the focused text field (`_` stands for a space).
    if let Some(text) = id.strip_prefix("type:") {
        info!("Test typing {text}");
        if !ui.host.text_input(&text.replace('_', " ")) {
            warn!("--ui-click: no text field has the focus");
        }
        return;
    }
    if let Some(key) = id.strip_prefix("key:") {
        info!("Test key {key}");
        if !ui.host.key(key) {
            warn!("--ui-click: no component takes key {key}");
        }
        return;
    }
    let Some(root) = ui.host.root() else { return };
    let mut target = None;
    ui.host.world().visit_visible(root, &mut |n, node| {
        if target.is_none() && node.data.id == id {
            target = Some(n);
        }
    });
    // "hover:<id>": rest the pointer on the component (its tooltip) instead of clicking it.
    if let Some(hover) = id.strip_prefix("hover:") {
        let mut found = None;
        ui.host.world().visit_visible(root, &mut |n, node| {
            if found.is_none() && node.data.id == hover {
                found = Some(n);
            }
        });
        match found {
            Some(n) => {
                let r = ui.host.world().get(n).map(|c| c.rect);
                if let Some(r) = r {
                    ui.host.set_cursor_position(r.x + r.w / 2.0, r.y + r.h / 2.0);
                }
                info!("Test hover on {hover}: {:?}", ui.host.tooltip_text(n));
                ui.host.pointer(n, PointerEvent::Enter);
                ui.host.hover(Some(n));
            }
            None => warn!("--ui-click: no visible component {hover}"),
        }
        return;
    }
    match target {
        Some(t) => {
            info!("Test click on {id}");
            click_sound(&ui.host, t, &mut sounds);
            for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
                ui.host.pointer(t, e);
            }
        }
        None => warn!("--ui-click: no visible component {id}"),
    }
}

/// Audio hook: a completed click on an enabled component plays the `ui` sound named after the
/// component id (see `audio::UiSound`). Sent before the click runs, as the click may destroy it.
/// Audio hook for the other UI events that make a sound (MIDDLEWARE_VERIFY.md §2): pointer entered
/// (`MouseOn`) and right click released (`RClickUp`) on a component. The sound is the `ui` event the
/// original's handler would look up; most components have none, and then nothing plays.
pub(crate) fn ui_event_sound(host: &UiScriptHost, id: NodeId, kind: crate::audio::UiEvent, sounds: &mut MessageWriter<crate::audio::UiSound>) {
    let world = host.world();
    let Some(node) = world.get(id) else { return };
    sounds.write(crate::audio::UiSound { component: node.data.id.clone(), kind });
}

pub(crate) fn click_sound(host: &UiScriptHost, id: NodeId, sounds: &mut MessageWriter<crate::audio::UiSound>) {
    let world = host.world();
    let Some(node) = world.get(id) else { return };
    if node.current().is_some_and(|s| s.disabled) {
        return;
    }
    sounds.write(crate::audio::UiSound::click(node.data.id.clone()));
}

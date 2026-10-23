//! The campaign HUD from the original layout `ui\campaign ui\layout` and its UI scripts, run by
//! the front end's `UiScriptHost` and drawn by its renderer (`frontend::render::spawn_world`).
//!
//! The engine side (the `CampaignUI.*` functions and the calls into the HUD's Lua: funds and
//! date, selection, review-panel tabs) is `ntw_script::ui::campaign`; evidence and tags are in
//! `analysis/campaign/CAMPAIGN_UI.md`. This module connects it to the running campaign:
//! - the HUD reads the model live through the campaign script host's shared state;
//! - what the HUD asks for (`CampaignRequest`: end turn, model commands, selection) is applied
//!   here through `CampaignSim`, so the campaign scripts see every event;
//! - the map selection (`CampaignSim::selected` / `selected_region` / `selected_fort`) is sent to the HUD, which
//!   fills the selection bar and the review panel (unit cards, ...);
//! - clicks on HUD components send `audio::UiSound` like the front end;
//! - keys, in the exe's order: a focused text field takes them all; else the campaign's bindings
//!   of `text/default_keys.xml` (the root's save, load, escape menu, the end turn button's Return,
//!   the camera's steps...) through the shortcut handler's state machine, a stolen key going to
//!   its stealer ([`keys`]);
//! - the scripts' load requests (`UiRequest::LoadCampaign`, from the save panel or quick load)
//!   reload the campaign ([`requests`]).
//! - a defeat / victory movie the model queued opens the original movie panel ([`update`]), its
//!   movie plays in the panel ([`panel_movie`]), and `CampaignUI.QuitToMainScreen` goes back to
//!   the main menu ([`requests`]).

use std::rc::Rc;

use bevy::camera::ClearColorConfig;
use bevy::camera::primitives::{Aabb, Frustum};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_formats::default_keys::{self, Shortcuts};
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;
use ntw_script::ScriptSource;
use ntw_script::ui::{CampaignKey, CampaignKeys, CampaignLink, CampaignRequest, CampaignSelection, FrontEndFacts, NodeId, PointerEvent, ShortcutMachine, UiScriptHost};

use super::play::CampaignSim;
use crate::GameMode;
use crate::data::GameData;
use crate::frontend::render::{UiAssets, UiSprite, spawn_world, ui_rect_to_window, ui_scale, ui_to_window, ui_virtual_screen, window_to_ui};

/// The HUD's script host and pointer state. Non-`Send` (Lua).
pub struct CampaignHud {
    host: UiScriptHost,
    screen: Vec2,
    drawn_generation: u64,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    sim_generation: u64,
    /// The selection last sent to the HUD.
    selection: CampaignSelection,
    clock_ms: f64,
    /// Screen rectangles of the HUD panels (clicks there do not reach the map).
    panels: Vec<Rect>,
    /// A camera move the HUD asked for (`CampaignUI.SetCameraTarget`, e.g. a radar click), in
    /// map units, applied by `labels`.
    camera_to: Option<Vec2>,
    /// The campaign's key bindings from `text/default_keys.xml` and their state ([`keys`]).
    shortcuts: ShortcutMachine,
    /// The camera bindings pressed this frame, in order: [`keys`] queues them,
    /// [`super::camera::control`] moves the camera and empties the queue.
    pub camera_steps: Vec<super::camera::CameraKeyPress>,
    /// `CampaignUI.QuitToMainScreen` ran: [`requests`] leaves for the main menu.
    leave: bool,
    /// The movie a HUD panel shows (`AttachCustomControl`), once [`panel_movie`] started it.
    panel_movie: Option<PanelMovie>,
}

/// A movie playing in a HUD component (the movie panel's `movie_window`).
struct PanelMovie {
    node: NodeId,
    /// The movie's file name, as the scripts attached it.
    movie: Rc<str>,
    /// The component's id, the key the renderer draws the movie under (`UiAssets::movies`).
    component: String,
    /// The movie's picture is in `UiAssets::movies`.
    shown: bool,
}

impl CampaignHud {
    /// True if a screen point is over a visible HUD panel.
    pub fn covers(&self, p: Vec2) -> bool {
        panels_cover(&self.panels, p)
    }
}

/// True if a screen point is inside one of the panel rectangles.
fn panels_cover(panels: &[Rect], p: Vec2) -> bool {
    panels.iter().any(|r| r.contains(p))
}

/// The pointer as the map sees it: none while it is over a HUD panel (a label hidden under a
/// panel is listed but cannot be the one under the pointer).
fn map_pointer(panels: &[Rect], cursor: Option<Vec2>) -> Option<Vec2> {
    cursor.filter(|&c| !panels_cover(panels, c))
}

/// Called from `scene::enter` once the campaign is running.
pub fn enter(world: &mut World) {
    let dir = crate::config::game_data_dir();
    let (vfs, source) = match (Vfs::open_install(&dir), ScriptSource::from_install(&dir)) {
        (Ok(v), Ok(s)) => (v, s),
        _ => {
            warn!("Campaign HUD: install not readable");
            return;
        }
    };
    let loc = Localisation::from_vfs(&vfs).unwrap_or_else(|e| {
        warn!("Campaign HUD: localisation not loaded: {e}");
        Localisation::new()
    });
    let screen = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(world)
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(1280.0, 720.0));
    let db = Rc::new(world.resource::<GameData>().db.clone());
    let Some(link) = world.get_non_send::<CampaignSim>().map(|sim| CampaignLink {
        state: sim.host.shared_state(),
        human: sim.human().to_owned(),
        campaign: sim.campaign().to_owned(),
        map: sim.saver.info.map_key.clone(),
        theatres: sim.saver.info.theatres().map(str::to_owned).collect(),
        db,
        saves: Some(sim.saver.clone() as Rc<dyn ntw_script::ui::CampaignSaves>),
    }) else {
        warn!("Campaign HUD: no campaign running");
        return;
    };
    // Both user folders: the save panel lists our saves and the original's (`frontend::SaveFolders`).
    let facts = FrontEndFacts {
        game_version: "1.3.0".into(),
        original_user_dir: crate::config::original_user_dir(),
        user_dir: crate::config::user_dir(),
        ..Default::default()
    };
    // The scripts lay the HUD out in the virtual screen of the original UI scale (see `ui_scale`).
    let virt = ui_virtual_screen(screen);
    let host = match UiScriptHost::new(source, loc, facts, (virt.x, virt.y), world.resource::<GameData>().db.limits.clone()) {
        Ok(h) => h,
        Err(e) => {
            warn!("Campaign HUD: script host failed: {e}");
            return;
        }
    };
    if let Err(e) = host.install_campaign(link) {
        warn!("Campaign HUD: setup failed: {e}");
        return;
    }
    if let Err(e) = host.load_root_layout("data/ui/campaign ui/layout") {
        warn!("Campaign HUD: layout not loaded: {e}");
        return;
    }
    // Nothing is selected at the start: the engine's ClearHud empties the selection bar (whose
    // layout text is the designers' sample "ygT").
    host.campaign_ready();
    log_errors(&host);
    // The campaign scripts hear that the campaign UI exists: `UICreated` with context.string
    // "Campaign UI" (CONFIRMED test in EpisodicScripting.lua's OnUICreated, which then runs
    // InitialiseCampaign: on a new game it restricts the campaign's building list, e.g. the
    // Peninsular and tutorial chains in eur_napoleon). INFERRED order: after NewCampaignStarted
    // (eur_napoleon's scripting.lua sets its new-game flag there and reads it in OnUICreated).
    if let Some(sim) = world.get_non_send::<CampaignSim>() {
        let ctx = ntw_script::ScriptContext { string: Some("Campaign UI".into()), component: Some("root".into()), ..Default::default() };
        let r = sim.host.fire("UICreated", ctx);
        for e in r.errors {
            warn!("Campaign script UICreated: {e}");
        }
        info!("Campaign: UICreated ({} handlers), {} restricted building levels", r.handlers, sim.host.state().model.world.restricted_buildings.len());
    }
    // A 2D camera over the 3D map for the HUD sprites.
    world.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }, DespawnOnExit(GameMode::Campaign)));
    let shortcuts = campaign_shortcuts(&vfs);
    let mut assets = UiAssets::new(vfs);
    assets.add_embedded(host.template_images());
    world.insert_resource(assets);
    world.insert_non_send(CampaignHud {
        host,
        screen,
        drawn_generation: u64::MAX,
        hovered: None,
        pressed: None,
        sim_generation: u64::MAX,
        selection: CampaignSelection::None,
        clock_ms: 0.0,
        panels: Vec::new(),
        camera_to: None,
        shortcuts,
        camera_steps: Vec::new(),
        leave: false,
        panel_movie: None,
    });
}

/// The scripts' log lines, each once (the host's log is drained): errors as warnings, the save
/// functions' own results (`QuickLoad` with no save, a `SaveCampaign` the campaign script switched
/// off) as info, the rest at debug level, as the front end does.
fn log_errors(host: &UiScriptHost) {
    for l in host.take_log() {
        if l.starts_with("ERROR") {
            warn!("Campaign HUD script: {l}");
        } else if l.starts_with("QuickLoad") || l.starts_with("SaveCampaign") {
            info!("Campaign HUD script: {l}");
        } else {
            debug!("Campaign HUD script: {l}");
        }
    }
}

/// Advances the campaign UI clock (ms) by one frame of real time and returns it. The clock is what
/// `OnUpdatePulse` gets and what `CampaignUI.Time()` reads (see `UiScriptHost::pulse`).
/// PROVISIONAL: real time, not `Time<Virtual>`, because the battle's pause and x2/x4 speed (and
/// `NAPOLEON_AI_SPEED`) change the virtual clock and nothing resets it when the campaign returns;
/// the campaign map has no pause of its own. What advances the exe's counter (`0x00A0EB80` reads
/// `+0x90` of the object at manager `+0x9FC`) is UNKNOWN. There is no per-frame cap: after a
/// stall (an end turn, the return from a battle) one frame carries the whole gap, so scripted UI
/// transitions running then (template.BuildingFrame.lua's drop-down) jump to their end; whether the
/// exe's counter caps a long frame belongs to that open question.
fn advance_ui_clock(clock_ms: &mut f64, real: &Time<Real>) -> f64 {
    *clock_ms += real.delta_secs_f64() * 1000.0;
    *clock_ms
}

/// The campaign's key bindings, read once from `text/default_keys.xml` through the install's file
/// system (mods included): the default keyset's funcs with a campaign or "all" mapping, each to
/// its component (`Shortcuts::mode_bindings`; the exe's loader `0x00EDB080`). A missing or
/// unreadable file is logged once and leaves no bindings.
fn campaign_shortcuts(vfs: &Vfs) -> ShortcutMachine {
    match vfs.read(default_keys::PATH).map_err(|e| e.to_string()).and_then(|b| Shortcuts::parse(&b).map_err(|e| e.to_string())) {
        Ok(s) => ShortcutMachine::new(s.mode_bindings("campaign")),
        Err(e) => {
            warn!("Campaign HUD: {} not read, no keyboard shortcuts: {e}", default_keys::PATH);
            ShortcutMachine::default()
        }
    }
}

/// The `keys` system's buffers, kept between frames so they are not allocated again once they
/// have grown: the frame's key events, their (key, pressed) changes, the keys down replayed from
/// the frame's start and the exe's names of those keys.
#[derive(Default)]
pub struct KeyBuffers {
    events: Vec<bevy::input::keyboard::KeyboardInput>,
    changes: Vec<(KeyCode, bool)>,
    down: crate::shortcut_keys::KeysDown,
    names: Vec<&'static str>,
}

/// Keyboard → the HUD in the exe's order: first each key event of the frame (press, auto-repeat,
/// release) to the focused component ([`UiScriptHost::campaign_key`]: while a component holds the
/// input focus the editing keys and the typed text go to it), then once per frame the shortcut
/// poll ([`UiScriptHost::campaign_keys`]), as the campaign's listener runs it from its per-frame
/// slot whether or not a key event came (`0x00A27EB0`, vtable `0x0136B40C` +0x20, called by
/// `0x0048BD40` after the frame's events). The poll runs the campaign's bindings of
/// `text/default_keys.xml` ([`campaign_shortcuts`]) on the keys down at the frame's end, through
/// the shortcut handler's state machine: in vanilla ESCAPE (any modifier) is the root's
/// `escape_menu`, CTRL+S `quick_save`, CTRL+L `quick_load` (the root's script decides, quick save
/// only when `CanSave()`), Return `end_turn` on `button_end_turn` (its script ends the turn),
/// W/S/A/D the camera's steps, pressed every frame they are held with the frame's time
/// ([`CampaignHud::camera_steps`], [`super::camera::key_step`]); a key a component stole goes to
/// its `OnKey`. The frame time is the real one capped at 0.3 s (`0x0048A650`). Keys are matched by
/// the exe's name of the physical key against the keys down at the frame's end; the replay of the
/// frame's events gives only each event's modifiers ([`crate::shortcut_keys::KeysDown`]). Harness
/// runs ignore the keyboard.
pub fn keys(
    hud: Option<NonSendMut<CampaignHud>>,
    mut input: MessageReader<bevy::input::keyboard::KeyboardInput>,
    held: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Real>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    mut buf: Local<KeyBuffers>,
) {
    let Some(mut hud) = hud else { return };
    if harness.is_some() {
        input.clear();
        return;
    }
    let KeyBuffers { events, changes, down, names } = &mut *buf;
    events.clear();
    events.extend(input.read().cloned());
    changes.clear();
    changes.extend(events.iter().map(|ev| (ev.key_code, ev.state.is_pressed())));
    down.rewind(held.get_pressed().copied(), changes);
    let hud = &mut *hud;
    for (ev, &(key, pressed)) in events.iter().zip(changes.iter()) {
        down.apply(key, pressed);
        hud.host.campaign_key(CampaignKey {
            up: !pressed,
            edit: if pressed { crate::frontend::key_name(key) } else { None },
            text: if pressed { ev.text.as_deref() } else { None },
            repeat: ev.repeat,
            held: down.modifiers(),
        });
    }
    // After the replay `down` is the keys held now.
    down.names_into(names);
    let held = down.modifiers();
    let keys = CampaignKeys { down: names.as_slice(), held, time: time.delta_secs().min(MAX_FRAME_TIME) };
    let steps = &mut hud.camera_steps;
    hud.host.campaign_keys(&mut hud.shortcuts, keys, &mut |func, time| {
        if let Some(func) = super::camera::CameraFunc::from_binding(func) {
            steps.push(super::camera::CameraKeyPress { func, time, shift: held.shift });
        }
    });
    log_errors(&hud.host);
}

/// The longest frame time the frame tick passes on, 300 ms (CONFIRMED, `0x0048A650`: the delta
/// from `0x010A1E10` is capped at 300 ms unless the frame-rate test is on).
const MAX_FRAME_TIME: f32 = 0.3;

/// The scripts' requests outside the HUD (`UiRequest`): a load (`CampaignUI.LoadCampaign` from the
/// save panel, `QuickLoad`) reloads the campaign from that save, as the front end's Load Game does
/// (`CampaignStart` + `GameMode::Campaign`, which leaves and re-enters the campaign). The other
/// requests are the front end's and have no meaning here: logged. `CampaignUI.QuitToMainScreen`
/// (the OK of the defeat movie, Exit on the campaign result) goes to the main menu.
pub fn requests(mut commands: Commands, hud: Option<NonSendMut<CampaignHud>>) {
    let Some(mut hud) = hud else { return };
    if std::mem::take(&mut hud.leave) {
        info!("Campaign: quit to the main menu");
        commands.queue(|world: &mut World| world.resource_mut::<NextState<GameMode>>().set(GameMode::FrontEnd));
    }
    for r in hud.host.take_requests() {
        match r {
            ntw_script::ui::UiRequest::LoadCampaign(path) => {
                info!("Campaign: load {}", path.display());
                commands.insert_resource(super::CampaignStart { campaign: String::new(), faction: None, save: Some(path) });
                commands.queue(|world: &mut World| world.resource_mut::<NextState<GameMode>>().set(GameMode::Campaign));
            }
            other => warn!("Campaign HUD: request {other:?} is not handled in the campaign"),
        }
    }
}

/// Mouse → HUD components (like the front end's pointer system). Harness runs ignore the mouse.
#[allow(clippy::too_many_arguments)]
pub fn pointer(
    hud: Option<NonSendMut<CampaignHud>>,
    sim: Option<NonSendMut<CampaignSim>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    time: Res<Time<Real>>,
    mut sounds: MessageWriter<crate::audio::UiSound>,
) {
    let (Some(mut hud), Ok(window)) = (hud, window.single()) else { return };
    let size = Vec2::new(window.width(), window.height());
    if hud.screen != size {
        hud.screen = size;
        let virt = ui_virtual_screen(size);
        hud.host.set_screen(virt.x, virt.y);
    }
    let t = advance_ui_clock(&mut hud.clock_ms, &time);
    hud.host.pulse(t);
    if harness.is_none() {
        let cursor = window.cursor_position().map(|p| window_to_ui(p, hud.screen));
        if let Some(p) = cursor {
            hud.host.set_cursor_position(p.x, p.y);
        }
        let hit = cursor.and_then(|p| hud.host.hit(p.x, p.y));
        if hit != hud.hovered {
            if let Some(old) = hud.hovered {
                hud.host.pointer(old, PointerEvent::Leave);
            }
            if let Some(new) = hit {
                hud.host.pointer(new, PointerEvent::Enter);
                crate::frontend::ui_event_sound(&hud.host, new, crate::audio::UiEvent::MouseOn, &mut sounds);
            }
            hud.hovered = hit;
        }
        if mouse.just_released(MouseButton::Right)
            && let Some(h) = hit
        {
            crate::frontend::ui_event_sound(&hud.host, h, crate::audio::UiEvent::RClickUp, &mut sounds);
        }
        if mouse.just_pressed(MouseButton::Left) {
            hud.pressed = hit;
            if let Some(h) = hit {
                hud.host.pointer(h, PointerEvent::LeftDown);
            }
        }
        if mouse.just_released(MouseButton::Left)
            && let Some(p) = hud.pressed.take()
        {
            if hit == Some(p) {
                crate::frontend::click_sound(&hud.host, p, &mut sounds);
                hud.host.pointer(p, PointerEvent::LeftUp);
            } else {
                hud.host.pointer(p, PointerEvent::LeftUpElsewhere);
            }
        }
    }
    log_errors(&hud.host);
    if let Some(mut sim) = sim {
        apply_requests(&mut hud, &mut sim);
    }
}

/// Applies what the HUD asked for.
fn apply_requests(hud: &mut CampaignHud, sim: &mut CampaignSim) {
    for r in hud.host.take_campaign_requests() {
        match r {
            CampaignRequest::EndTurn => sim.end_turn(),
            CampaignRequest::Command(cmd) => sim.command(cmd),
            CampaignRequest::Select(sel) => {
                let (c, r, f) = match sel {
                    CampaignSelection::Character(c) => (Some(c), None, None),
                    CampaignSelection::Settlement(r) => (None, Some(r), None),
                    CampaignSelection::Fort(r) => (None, None, Some(r)),
                    CampaignSelection::None => (None, None, None),
                };
                sim.selected = c;
                sim.selected_region = r;
                sim.selected_fort = f;
                sim.generation += 1;
            }
            CampaignRequest::CameraTo(x, y) => hud.camera_to = Some(Vec2::new(x, y)),
            CampaignRequest::QuitToMainScreen => hud.leave = true,
        }
    }
}

/// Feeds the model's treasury and date (`UpdateFactionFundsAndDate`) and the map selection to the
/// layout whenever the campaign changed, and opens the movie panel for each defeat / victory movie
/// the model queued (`UiScriptHost::campaign_outcome_movie`), and shows the human a waiting
/// capture's screen and a waiting call to war's `ally_attacked` message.
pub fn update(hud: Option<NonSendMut<CampaignHud>>, sim: Option<NonSendMut<CampaignSim>>) {
    let (Some(mut hud), Some(mut sim)) = (hud, sim) else { return };
    if hud.sim_generation == sim.generation {
        return;
    }
    for (faction, movie) in std::mem::take(&mut sim.outcome_movies) {
        hud.host.campaign_outcome_movie(faction, &movie);
    }
    hud.sim_generation = sim.generation;
    hud.host.campaign_update_funds();
    let sel = match (sim.selected, sim.selected_region, sim.selected_fort) {
        (Some(c), ..) => CampaignSelection::Character(c),
        (None, Some(r), _) => CampaignSelection::Settlement(r),
        (None, None, Some(r)) => CampaignSelection::Fort(r),
        (None, None, None) => CampaignSelection::None,
    };
    // A new selection, or the model changed under the current one: the engine regenerates the
    // review panel (INFERRED; the original refreshes it softly with SoftRefreshReviewPanel).
    if sel != hud.selection || sel != CampaignSelection::None {
        hud.selection = sel;
        hud.host.campaign_select(sel);
    }
    // A capture the human must answer opens the capture screen (see `campaign_capture_screen`).
    hud.host.campaign_capture_screen();
    // A call to war waiting for the human's answer shows its message (see `campaign_ally_war_call`).
    hud.host.campaign_ally_war_call();
    log_errors(&hud.host);
}

/// Whether the movie the scripts attached (`UiScriptHost::attached_movie`) is the one playing.
fn same_panel_movie(want: Option<&(NodeId, Rc<str>)>, playing: Option<&PanelMovie>) -> bool {
    match (want, playing) {
        (Some((node, movie)), Some(p)) => *node == p.node && *movie == p.movie,
        (None, None) => true,
        _ => false,
    }
}

/// The movie a HUD panel shows: the scripts attach it with `AttachCustomControl` (the movie panel's
/// `movie_window`, CONFIRMED event_movie_panel.luac) and `CampaignUI.MovieDismissed` drops it.
/// Starts it into a texture, draws it in that component and stops it when it is dropped or the
/// component is gone. INFERRED: the movie plays once with its sound track in the player's language
/// (what the exe does with the sound volume it hands the bink control is traced only up to the
/// player: which two settings feed it and how the player applies it are not, see the PROVISIONAL
/// note in the body).
pub fn panel_movie(
    hud: Option<NonSendMut<CampaignHud>>,
    assets: Option<ResMut<UiAssets>>,
    started: Query<&crate::video::Movie, Added<crate::video::Movie>>,
    mut play: MessageWriter<crate::video::PlayMovie>,
    mut stop: MessageWriter<crate::video::StopMovie>,
) {
    let (Some(mut hud), Some(mut assets)) = (hud, assets) else { return };
    let want = hud.host.attached_movie();
    if !same_panel_movie(want.as_ref(), hud.panel_movie.as_ref()) {
        if let Some(old) = hud.panel_movie.take() {
            stop.write(crate::video::StopMovie { path: Some(old.movie.to_string()) });
            assets.movies.remove(&old.component);
            hud.drawn_generation = u64::MAX;
        }
        if let Some((node, movie)) = want {
            let component = hud.host.world().get(node).map(|n| n.data.id.clone()).unwrap_or_default();
            // PROVISIONAL: the movie's sound plays like every movie's in ours (`audio::movie_audio`):
            // the `sound_events` movie-table multiplier, in the movie group (`MOVIE_VOLUME`), under
            // master. The exe: `0x00A13EA0` hands the panel constructor `0x00998E20` int(setting 4 ×
            // 0.01 × setting 5) (CVTTSS2SI at ~0x00A13FC0), stored at panel +0x24; `0x00A23330` reads
            // +0x24 and passes it to the bink control's open `0x00D9F600`, which multiplies it by 0x102
            // (0 when the control's +0xC vfunc +0xD4(5) returns false) and hands it to the player's
            // vfunc +0xC. Not traced: which two settings, and how the player applies that value.
            play.write(crate::video::PlayMovie { path: movie.to_string(), mode: crate::video::MovieMode::Texture, track: None, looped: false, sound: true });
            hud.panel_movie = Some(PanelMovie { node, movie, component, shown: false });
        }
    }
    let mut shown = false;
    if let Some(p) = hud.panel_movie.as_mut().filter(|p| !p.shown)
        && let Some(m) = started.iter().find(|m| m.plays(&p.movie))
    {
        assets.movies.insert(p.component.clone(), m.image.clone());
        p.shown = true;
        shown = true;
    }
    if shown {
        hud.drawn_generation = u64::MAX;
    }
}

impl CampaignHud {
    /// The stop for the panel's movie, when one plays (leaving the campaign).
    pub(super) fn stop_panel_movie(&mut self) -> Option<crate::video::StopMovie> {
        self.panel_movie.take().map(|p| crate::video::StopMovie { path: Some(p.movie.to_string()) })
    }
}

/// Rebuilds the HUD sprites when the UI tree changed, and records the panel rectangles.
pub fn redraw(
    mut commands: Commands,
    hud: Option<NonSendMut<CampaignHud>>,
    assets: Option<ResMut<UiAssets>>,
    mut images: ResMut<Assets<Image>>,
    sprites: Query<Entity, With<UiSprite>>,
    new_sprites: Query<Entity, Added<UiSprite>>,
) {
    // Sprites spawned last frame by `spawn_world` belong to this mode.
    for e in &new_sprites {
        commands.entity(e).insert(DespawnOnExit(GameMode::Campaign));
    }
    let (Some(mut hud), Some(mut assets)) = (hud, assets) else { return };
    let generation = hud.host.world().generation;
    if generation == hud.drawn_generation {
        return;
    }
    for e in &sprites {
        commands.entity(e).despawn();
    }
    if let Some(root) = hud.host.root() {
        let world = hud.host.world();
        spawn_world(&world, root, &mut commands, &mut assets, &mut images, hud.screen, ui_scale(hud.screen));
        let mut panels = Vec::new();
        world.visit_visible(root, &mut |_, node| {
            if matches!(node.data.id.as_str(), "hud_left" | "hud_center" | "hud_right" | "frame") {
                panels.push(ui_rect_to_window(node.rect, hud.screen));
            }
        });
        drop(world);
        hud.panels = panels;
    }
    hud.drawn_generation = generation;
}

/// The harness's state.
pub struct Harness {
    end_turns: u32,
    demo_move: bool,
    embark: Option<Option<super::play::EmbarkDemo>>,
    region: Option<String>,
    clicks: Vec<String>,
    quick_save: bool,
    secs: f32,
    /// Where `shot:<name>` steps write their pictures (`--campaign-ui-proof [dir]`, default
    /// `target/tmp/ui_proof`).
    proof_dir: std::path::PathBuf,
}

/// Screenshot harness: `--campaign-end-turn <n>` clicks the original End Turn button `n` times
/// (one click per second, starting after one second), `--campaign-demo-move` gives the
/// `--campaign-demo` army its move order once, `--campaign-demo-embark` runs the embark demo
/// (a fleet sails into a port, an army boards it and is landed on another shore, see
/// `play::embark_demo_step`), `--campaign-select-region <key>` selects a
/// settlement and `--campaign-ui-click id,...` clicks HUD components (one per second of real time,
/// the clock the HUD's UI clock runs on, see `advance_ui_clock`; a
/// `selectfort:<region key>` step selects that region's fort instead, `selectchar:<name>` one of the
/// player's characters; a `shot:<name>` step writes a screenshot to the `--campaign-ui-proof`
/// folder, default `target/tmp/ui_proof`). `--campaign-quick-save`
/// presses the CTRL+S quick save after all that (for testing our saves in the original game). All only
/// run with `--screenshot`.
pub fn harness(
    mut commands: Commands,
    hud: Option<NonSendMut<CampaignHud>>,
    sim: Option<NonSendMut<CampaignSim>>,
    shot: Option<ResMut<crate::frontend::AutoScreenshot>>,
    time: Res<Time<Real>>,
    mut state: Local<Option<Harness>>,
    mut sounds: MessageWriter<crate::audio::UiSound>,
) {
    let (Some(mut hud), Some(mut sim), Some(mut shot)) = (hud, sim, shot) else { return };
    let args: Vec<String> = std::env::args().collect();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let st = state.get_or_insert_with(|| {
        let proof = args.iter().any(|a| a == "--campaign-ui-proof");
        let proof_dir = opt("--campaign-ui-proof").filter(|d| !d.starts_with("--")).unwrap_or_else(|| "target/tmp/ui_proof".into());
        let mut clicks: Vec<String> = opt("--campaign-ui-click").map(|s| s.split(',').map(str::to_owned).collect()).unwrap_or_default();
        if proof {
            clicks = PROOF_STEPS.iter().map(|s| (*s).to_owned()).collect();
        }
        if args.iter().any(|a| a == "--campaign-browser-proof") {
            clicks.extend(BROWSER_PROOF_STEPS.iter().map(|s| (*s).to_owned()));
        }
        if args.iter().any(|a| a == "--campaign-capture-proof") {
            clicks.extend(CAPTURE_PROOF_STEPS.iter().map(|s| (*s).to_owned()));
        }
        // Any `shot:` step (also from `--campaign-ui-click`) writes into the proof folder.
        if clicks.iter().any(|c| c.starts_with("shot:"))
            && let Err(e) = std::fs::create_dir_all(&proof_dir)
        {
            warn!("Harness: cannot create {proof_dir}: {e}");
        }
        Harness {
            end_turns: opt("--campaign-end-turn").and_then(|s| s.parse().ok()).unwrap_or(0),
            demo_move: args.iter().any(|a| a == "--campaign-demo-move"),
            embark: args.iter().any(|a| a == "--campaign-demo-embark").then_some(None),
            region: opt("--campaign-select-region").or_else(|| proof.then(|| "eur_france".into())),
            clicks,
            quick_save: args.iter().any(|a| a == "--campaign-quick-save"),
            secs: 0.0,
            proof_dir: proof_dir.into(),
        }
    });
    st.secs += time.delta_secs();
    if st.secs < 1.0 {
        return;
    }
    st.secs = 0.0;
    if let Some(key) = st.region.take() {
        let r = sim.model().world.regions.values().find(|r| r.key == key).map(|r| r.id);
        match r {
            Some(r) => {
                sim.selected = None;
                sim.selected_region = Some(r);
                sim.selected_fort = None;
                sim.generation += 1;
                info!("Harness: selected region {key}");
            }
            None => warn!("Harness: no region {key}"),
        }
        shot.secs = 0.0;
        return;
    }
    if st.end_turns > 0 {
        click(&mut hud, "button_end_turn", &mut sounds);
        st.end_turns -= 1;
        shot.secs = 0.0; // keep the screenshot waiting until the clicks are done
        return;
    }
    if let Some(demo) = st.embark.as_mut() {
        if demo.is_none() {
            let human = sim.human_id();
            *demo = human.and_then(|h| super::play::embark_demo_pick(&sim.model(), h));
            if demo.is_none() {
                warn!("Embark demo: no fleet, port and army found");
                st.embark = None;
            }
        } else if super::play::embark_demo_step(&mut sim, demo.as_mut().expect("checked")) {
            st.embark = None;
        }
        shot.secs = 0.0;
        return;
    }
    if st.demo_move {
        st.demo_move = false;
        if let Some((x, z)) = sim.demo_target.take() {
            super::play::right_click(&mut sim, x, z);
            info!("Harness: {}", sim.last_message);
        }
        shot.secs = 0.0;
        return;
    }
    if !st.clicks.is_empty() {
        let id = st.clicks.remove(0);
        if let Some(name) = id.strip_prefix("shot:") {
            // A picture of this step (the HUD was redrawn in the second since the last step).
            use bevy::render::view::screenshot::{Screenshot, save_to_disk};
            let path = st.proof_dir.join(format!("{name}.png"));
            info!("Harness: screenshot {}", path.display());
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        } else if id == "noop" {
            // One more second for animations to settle.
        } else if id == "queues" {
            log_queues(&sim);
        } else if let Some(key) = id.strip_prefix("stagecapture:") {
            stage_capture(&mut sim, key);
        } else if let Some(key) = id.strip_prefix("selectfort:") {
            select_fort(&mut sim, key);
        } else if let Some(name) = id.strip_prefix("selectchar:") {
            select_character(&mut sim, name);
        } else if let Some(key) = id.strip_prefix("owner:") {
            let m = sim.model();
            let owner = m.world.regions.values().find(|r| r.key == key).and_then(|r| m.world.factions.get(&r.owner)).map(|f| f.key.clone());
            info!("Proof: {key} is owned by {owner:?}, capture waiting: {}, treasury {}", m.pending_capture.is_some(), m.faction_by_key(sim.human()).map_or(0, |f| f.treasury));
        } else {
            click(&mut hud, &id, &mut sounds);
            apply_requests(&mut hud, &mut sim);
        }
        shot.secs = 0.0;
    }
    if st.quick_save {
        st.quick_save = false;
        // The CTRL+S shortcut, as a player would press it (the root decides, `CanSave()`).
        if !hud.host.campaign_shortcut("quick_save") {
            warn!("Harness: the campaign root has no OnShortcut");
        }
        log_errors(&hud.host);
        shot.secs = 0.0;
    }
}

/// `--campaign-ui-proof`: the settlement panel checked end to end through the same UI events a
/// player's clicks send (Paris, France): pictures of every step in the proof folder, the queues in
/// the log before and after End Turns ("Proof:" lines).
const PROOF_STEPS: &[&str] = &[
    "shot:01_paris_selected",
    "queues",
    // Resting the pointer on the third slot (Ordnance Factory) shows its upgrade.
    "hover:Building3",
    "shot:02_slot_upgrade_shown",
    "Building3_Upgrade1",
    "shot:03_upgrade_queued",
    "recruitment_tab",
    "shot:04_recruitment_tab",
    "Inf_Line_French_Fusiliers!recruitable!3",
    "Inf_Line_French_Fusiliers!recruitable!3",
    "shot:05_two_units_queued",
    // Clicking a queued card cancels it (refund).
    "Inf_Line_French_Fusiliers!enqueued!1",
    "shot:06_one_unit_cancelled",
    "queues",
    "button_end_turn",
    "queues",
    "shot:07_after_end_turn_recruitment",
    "construction_tab",
    "shot:08_after_end_turn_construction",
    "button_end_turn",
    "queues",
    "shot:09_after_second_end_turn",
];

/// `--campaign-browser-proof`: the building browser (Build Browser button), then the tree view
/// of its third entry (Paris's Ordnance Factory slot; the click shows the slot's building tree).
const BROWSER_PROOF_STEPS: &[&str] = &[
    "build_browser",
    "shot:b1_building_browser",
    "building_browser_entry3",
    "noop",
    "shot:b2_building_tree",
    "hover:sCannon3_great_arsenal",
    "shot:b3_tree_node_tooltip",
];

/// `--campaign-capture-proof`: the capture screen. The capture itself is staged (harness
/// `stagecapture:<region>` puts the model's own capture preview for the human in
/// `pending_capture`, as a won assault does; marching an army there takes several turns); from
/// there on everything is the real path: the HUD opens the original `settlement_captured` panel,
/// the Loot button's click answers it and the model resolves the capture.
const CAPTURE_PROOF_STEPS: &[&str] = &[
    "owner:eur_bavaria",
    "stagecapture:eur_bavaria",
    "shot:c1_capture_screen",
    "panel_loot/text_button",
    "shot:c2_after_loot",
    "owner:eur_bavaria",
];

/// Harness `stagecapture:<region>`: the human faction takes that region by force without the
/// battle (`CampaignModel::capture_by_force`: the settlement changes hands as after a won assault,
/// then the capture report waits for the player's answer in `pending_capture`).
fn stage_capture(sim: &mut CampaignSim, key: &str) {
    let human = sim.human_id();
    {
        let mut st = sim.host.state_mut();
        let m = &mut st.model;
        let r = m.world.regions.values().find(|r| r.key == key).map(|r| r.id);
        let (Some(f), Some(r)) = (human, r) else {
            warn!("Harness: cannot stage a capture of {key}");
            return;
        };
        let events = m.capture_by_force(r, f, None);
        info!("Harness: staged a capture of {key} ({} events)", events.len());
    }
    sim.generation += 1;
}

/// Harness `selectfort:<region>`: selects the region's fort (`CampaignSelection::Fort`). A map click
/// on a fort selects it too now (`play::map_pick`), but no shipped file has a fort, so this step is
/// still the only way to reach one (see `CampaignSim::selected_fort`).
fn select_fort(sim: &mut CampaignSim, key: &str) {
    let r = sim.model().world.regions.values().find(|r| r.key == key).map(|r| r.id);
    match r {
        Some(r) => {
            sim.selected = None;
            sim.selected_region = None;
            sim.selected_fort = Some(r);
            sim.generation += 1;
            info!("Harness: selected the fort of {key}");
        }
        None => warn!("Harness: no region {key}"),
    }
}

/// Harness `selectchar:<name>`: selects the human faction's character whose forename or surname
/// key ends in `name` (case-insensitive; the keys end in the spelled name, e.g. `Wellesley`), else
/// one whose key contains it; of several, the one with the lowest id, with a warning naming how
/// many matched. As a map click on him would, it also drops the `--campaign-demo` path target, so
/// the path preview follows the cursor again.
fn select_character(sim: &mut CampaignSim, name: &str) {
    let needle = name.to_ascii_lowercase();
    let (found, matches) = {
        let m = sim.model();
        let human = m.faction_by_key(sim.human()).map(|f| f.id);
        let keys = |c: &ntw_sim::campaign::Character| {
            m.world.character_details.get(&c.id).map(|d| [d.forename.to_ascii_lowercase(), d.surname.to_ascii_lowercase()])
        };
        let mine = || m.world.characters.values().filter(|c| Some(c.faction) == human);
        // The characters map is ordered by id, so the first match has the lowest id.
        let exact: Vec<_> = mine().filter(|c| keys(c).is_some_and(|k| k.iter().any(|k| k.ends_with(&needle)))).map(|c| c.id).collect();
        let found = if exact.is_empty() {
            mine().filter(|c| keys(c).is_some_and(|k| k.iter().any(|k| k.contains(&needle)))).map(|c| c.id).collect()
        } else {
            exact
        };
        (found.first().copied(), found.len())
    };
    match found {
        Some(c) => {
            if matches > 1 {
                warn!("Harness: {matches} characters match {name}; selecting the first (id {})", c.0);
            }
            sim.selected = Some(c);
            sim.selected_region = None;
            sim.selected_fort = None;
            sim.demo_target = None;
            sim.generation += 1;
            info!("Harness: selected character {name}");
        }
        None => warn!("Harness: no character named {name}"),
    }
}

/// Logs the selected settlement's treasury and queues (harness `queues`).
fn log_queues(sim: &CampaignSim) {
    let m = sim.model();
    let treasury = m.faction_by_key(sim.human()).map_or(0, |f| f.treasury);
    let Some(r) = sim.selected_region.and_then(|r| m.world.regions.get(&r)) else {
        info!("Proof: turn {}, treasury {treasury}, no settlement selected", m.calendar.turn_number());
        return;
    };
    let garrison = r.garrison.and_then(|g| m.world.forces.get(&g)).map_or(0, |f| f.units.len());
    info!(
        "Proof: turn {}, treasury {treasury}, {}: construction [{}], recruitment [{}], garrison {garrison} units",
        m.calendar.turn_number(),
        r.key,
        r.construction.iter().map(|c| format!("{} {} turns left", c.level_key, c.turns_remaining)).collect::<Vec<_>>().join(", "),
        r.recruitment_queue.iter().map(|q| format!("{} {} turns left", q.unit_key, q.turns_remaining)).collect::<Vec<_>>().join(", "),
    );
}

/// Clicks the first visible HUD component with this id (harness).
fn click(hud: &mut CampaignHud, id: &str, sounds: &mut MessageWriter<crate::audio::UiSound>) {
    let Some(root) = hud.host.root() else { return };
    // "a/b": the first visible `b` inside the first visible `a` (ids that repeat, such as the
    // capture screen's three `text_button`s).
    let mut target = Some(root);
    for part in id.split('/') {
        let Some(start) = target.take() else { break };
        hud.host.world().visit_visible(start, &mut |n, node| {
            if target.is_none() && node.data.id == part {
                target = Some(n);
            }
        });
    }
    // "hover:<id>": rest the pointer on the component (tooltip) instead of clicking it.
    if let Some(hover) = id.strip_prefix("hover:") {
        let mut t = None;
        hud.host.world().visit_visible(root, &mut |n, node| {
            if t.is_none() && node.data.id == hover {
                t = Some((n, node.rect));
            }
        });
        if let Some((n, r)) = t {
            hud.host.set_cursor_position(r.x + r.w / 2.0, r.y + r.h / 2.0);
            hud.host.pointer(n, PointerEvent::Enter);
            info!("Harness: hovering {hover}");
        }
        return;
    }
    match target {
        Some(b) => {
            crate::frontend::click_sound(&hud.host, b, sounds);
            for e in [PointerEvent::Enter, PointerEvent::LeftDown, PointerEvent::LeftUp, PointerEvent::Leave] {
                hud.host.pointer(b, e);
            }
            info!("Harness: clicked {id}");
        }
        None => warn!("Harness: no visible {id}"),
    }
}

/// Settlement labels: each frame the HUD gets the camera position and the settlements on screen
/// (`RetrieveVisibleEnitityDetails`, `0x009F4520` → `0x009B0400` per settlement, CONFIRMED): a
/// settlement is listed when a 6-unit cube around it at height 0 is not wholly outside the view
/// frustum (`0x009BAA90` → `0x010F6AA0`), whether or not a HUD panel covers it, and its screen
/// position is the point at height 0 two units south of it, projected and rounded (`0x010EE3B0`,
/// camera virtual +0x270). The original Labels.lua (driven by the root layout's
/// pulse) creates and moves the `city_info_bar` labels ("Settlement, Region" in the owner's
/// colours). The labels draw under every HUD panel: they are root children of priority -1 and
/// the root's `OnAdoptChild` sorts its children by priority (CONFIRMED, see the host's
/// `adopted`). PROVISIONAL: the one under the pointer is the nearest within 24 px (none while the
/// pointer is over a HUD panel, [`map_pointer`]).
/// Also reports the camera target and theatre to the HUD (`CameraTarget`, which the radar
/// follows) and applies a camera move the HUD asked for (a radar click).
pub fn labels(
    hud: Option<NonSendMut<CampaignHud>>,
    sim: Option<NonSend<CampaignSim>>,
    camera: Query<(&Camera, &GlobalTransform, &Frustum), With<super::camera::CampaignCamera>>,
    mut rig: Query<&mut super::camera::CampaignCamera>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let (Some(mut hud), Some(sim), Ok((camera, cam_tf, frustum))) = (hud, sim, camera.single()) else { return };
    if let Ok(mut cam) = rig.single_mut() {
        // The camera works in Bevy x / z, which is map x / -y.
        if let Some(to) = hud.camera_to.take() {
            cam.target = Vec2::new(to.x, -to.y).clamp(cam.min, cam.max);
        }
        hud.host.campaign_set_theatre_bounds((cam.min.x, -cam.max.y), (cam.max.x, -cam.min.y));
        hud.host.campaign_set_camera_target(cam.target.x, -cam.target.y, cam.distance);
    }
    let cursor = map_pointer(&hud.panels, window.single().ok().and_then(|w| w.cursor_position()));
    let mut list = Vec::new();
    let mut over: Option<(ntw_sim::campaign::RegionId, f32)> = None;
    for r in sim.model().world.regions.values() {
        let (x, y) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        // A 6-unit cube around the settlement at height 0 that is not wholly outside the view
        // frustum (`0x009BAA90` → `0x010F6AA0`, all six planes); the HUD panels play no part.
        if !frustum.intersects_obb_identity(&Aabb::from_min_max(Vec3::new(x - 3.0, -3.0, -y - 3.0), Vec3::new(x + 3.0, 3.0, -y + 3.0))) {
            continue;
        }
        // The point projected is at height 0, 2 units south of the settlement (`0x009B0400`).
        let Ok(p) = camera.world_to_viewport(cam_tf, Vec3::new(x, 0.0, -(y - 2.0))) else { continue };
        if let Some(c) = cursor {
            let d = c.distance(p);
            if d < 24.0 && over.is_none_or(|(_, best)| d < best) {
                over = Some((r.id, d));
            }
        }
        let q = window_to_ui(p, hud.screen);
        list.push((r.id, q.x.round_ties_even(), q.y.round_ties_even()));
    }
    let t = cam_tf.translation();
    hud.host.campaign_set_view((t.x, t.y, t.z), list, over.map(|o| o.0));
}

/// Marks the radar's camera outline sprites (redrawn every frame).
#[derive(Component)]
pub struct RadarOutline;

/// The camera's view outline on the radar: the ground points under the window's four corners
/// (the original projects the screen corners (-1, -1) .. (1, 1) onto the ground, 0x00A27D50,
/// CONFIRMED) placed on the radar by `UiScriptHost::campaign_radar_outline`, drawn as four lines
/// clipped to the radar. PROVISIONAL look: 1.5-pixel white lines (the original's line art is
/// UNKNOWN); corners whose ray misses the ground use the far clip distance.
pub fn radar_outline(
    hud: Option<NonSend<CampaignHud>>,
    mut commands: Commands,
    old: Query<Entity, With<RadarOutline>>,
    camera: Query<(&Camera, &GlobalTransform), With<super::camera::CampaignCamera>>,
    rig: Query<&super::camera::CampaignCamera>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let (Some(hud), Ok((camera, tf)), Ok(cam)) = (hud, camera.single(), rig.single()) else { return };
    let size = hud.screen;
    let corners = [Vec2::ZERO, Vec2::new(size.x, 0.0), size, Vec2::new(0.0, size.y)];
    let mut ground = [(0.0, 0.0); 4];
    for (i, c) in corners.iter().enumerate() {
        let Ok(ray) = camera.viewport_to_world(tf, *c) else { return };
        let d = ray.direction.as_vec3();
        let t = if d.y < -1e-4 { (cam.ground - ray.origin.y) / d.y } else { 1000.0 };
        let p = ray.origin + d * t.min(1000.0);
        ground[i] = (p.x, -p.z);
    }
    let Some((pts, clip)) = hud.host.campaign_radar_outline(ground) else { return };
    for i in 0..4 {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        let Some((a, b)) = clip_segment(a, b, (clip.x, clip.y, clip.x + clip.w, clip.y + clip.h)) else { continue };
        let (a, b) = (ui_to_window(Vec2::new(a.0, a.1), size), ui_to_window(Vec2::new(b.0, b.1), size));
        let mid = (a + b) / 2.0;
        let len = a.distance(b);
        if len < 0.5 {
            continue;
        }
        let angle = -(b.y - a.y).atan2(b.x - a.x);
        commands.spawn((
            Sprite { color: Color::srgba(1.0, 1.0, 1.0, 0.85), custom_size: Some(Vec2::new(len, 1.5)), ..default() },
            Transform::from_xyz(mid.x - size.x / 2.0, size.y / 2.0 - mid.y, 900.0).with_rotation(Quat::from_rotation_z(angle)),
            RadarOutline,
            DespawnOnExit(GameMode::Campaign),
        ));
    }
}

/// A segment cut to a rectangle (x0, y0, x1, y1) (Liang-Barsky), None if outside.
fn clip_segment(a: (f32, f32), b: (f32, f32), r: (f32, f32, f32, f32)) -> Option<((f32, f32), (f32, f32))> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [(-dx, a.0 - r.0), (dx, r.2 - a.0), (-dy, a.1 - r.1), (dy, r.3 - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t0 <= t1).then_some(((a.0 + t0 * dx, a.1 + t0 * dy), (a.0 + t1 * dx, a.1 + t1 * dy)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::{TimePlugin, TimeUpdateStrategy};
    use std::time::Duration;

    /// Settlements behind a HUD panel are listed (`0x009F4520` has no panel test), so the label
    /// under the pointer must ignore a pointer that is over a panel. Bug: hovering a panel within
    /// 24 px of a hidden settlement made `ShouldShowLabelBottomRow` report it.
    #[test]
    fn a_pointer_over_a_panel_is_not_on_the_map() {
        let panels = [Rect::new(0.0, 0.0, 100.0, 50.0)];
        assert_eq!(map_pointer(&panels, Some(Vec2::new(10.0, 10.0))), None);
        assert_eq!(map_pointer(&panels, Some(Vec2::new(10.0, 80.0))), Some(Vec2::new(10.0, 80.0)));
        assert_eq!(map_pointer(&panels, None), None);
    }

    #[derive(Resource, Default)]
    struct Clock(f64);

    fn tick(mut clock: ResMut<Clock>, real: Res<Time<Real>>) {
        advance_ui_clock(&mut clock.0, &real);
    }

    /// The campaign UI clock follows real time: a battle left paused or sped up (both change
    /// `Time<Virtual>`) neither freezes nor scales the campaign's scripted transitions.
    #[test]
    fn the_ui_clock_ignores_virtual_pause_and_speed() {
        let mut app = App::new();
        app.add_plugins(TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
            .init_resource::<Clock>()
            .add_systems(Update, tick);
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        for _ in 0..5 {
            app.update();
        }
        let real_ms = |app: &App| app.world().resource::<Time<Real>>().elapsed_secs_f64() * 1000.0;
        assert_eq!(app.world().resource::<Time<Virtual>>().elapsed_secs_f64(), 0.0, "virtual time paused");
        assert!(real_ms(&app) >= 400.0);
        assert!((app.world().resource::<Clock>().0 - real_ms(&app)).abs() < 1e-6, "paused: the clock still runs");
        app.world_mut().resource_mut::<Time<Virtual>>().unpause();
        app.world_mut().resource_mut::<Time<Virtual>>().set_relative_speed(4.0);
        for _ in 0..5 {
            app.update();
        }
        assert!((app.world().resource::<Clock>().0 - real_ms(&app)).abs() < 1e-6, "x4: the clock runs at 1x");
    }
}

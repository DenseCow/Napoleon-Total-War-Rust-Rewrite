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
//! - clicks on HUD components send `audio::UiSound` like the front end.

use std::rc::Rc;

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;
use ntw_script::ScriptSource;
use ntw_script::ui::{CampaignLink, CampaignRequest, CampaignSelection, FrontEndFacts, NodeId, PointerEvent, UiScriptHost};

use super::play::{CampaignSim, month_name};
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
}

impl CampaignHud {
    /// True if a screen point is over a visible HUD panel.
    pub fn covers(&self, p: Vec2) -> bool {
        self.panels.iter().any(|r| r.contains(p))
    }
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
        human: sim.human.clone(),
        campaign: sim.campaign.clone(),
        db,
    }) else {
        warn!("Campaign HUD: no campaign running");
        return;
    };
    let facts = FrontEndFacts { game_version: "1.3.0".into(), user_dir: crate::config::user_dir(), ..Default::default() };
    // The scripts lay the HUD out in the virtual screen of the original UI scale (see `ui_scale`).
    let virt = ui_virtual_screen(screen);
    let host = match UiScriptHost::new(source, loc, facts, (virt.x, virt.y)) {
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
    let mut assets = UiAssets::new(vfs).with_loose_files(&dir);
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
    });
}

fn log_errors(host: &UiScriptHost) {
    for l in host.take_log().into_iter().filter(|l| l.starts_with("ERROR")) {
        warn!("Campaign HUD script: {l}");
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
            hud.host.campaign_hover(hit);
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
        }
    }
}

/// Feeds the model's treasury and date (`UpdateFactionFundsAndDate`) and the map selection to the
/// layout whenever the campaign changed.
pub fn update(hud: Option<NonSendMut<CampaignHud>>, sim: Option<NonSend<CampaignSim>>) {
    let (Some(mut hud), Some(sim)) = (hud, sim) else { return };
    if hud.sim_generation == sim.generation {
        return;
    }
    hud.sim_generation = sim.generation;
    let (month, half) = {
        let d = sim.model().calendar.date;
        (d.month, d.half)
    };
    // PROVISIONAL round text "Early/Late <month>" (the original's loc key is not found yet).
    let round = format!("{} {}", if half == 0 { "Early" } else { "Late" }, month_name(month));
    hud.host.campaign_update_funds(&round);
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
    log_errors(&hud.host);
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
/// player's characters). `--campaign-quick-save`
/// writes the F5 quick save after all that (for testing our saves in the original game). All only
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
            let _ = std::fs::create_dir_all(&proof_dir);
        }
        if args.iter().any(|a| a == "--campaign-browser-proof") {
            clicks.extend(BROWSER_PROOF_STEPS.iter().map(|s| (*s).to_owned()));
            let _ = std::fs::create_dir_all(&proof_dir);
        }
        if args.iter().any(|a| a == "--campaign-capture-proof") {
            clicks.extend(CAPTURE_PROOF_STEPS.iter().map(|s| (*s).to_owned()));
            let _ = std::fs::create_dir_all(&proof_dir);
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
            info!("Proof: {key} is owned by {owner:?}, capture waiting: {}, treasury {}", m.pending_capture.is_some(), m.faction_by_key(&sim.human).map_or(0, |f| f.treasury));
        } else {
            click(&mut hud, &id, &mut sounds);
            apply_requests(&mut hud, &mut sim);
        }
        shot.secs = 0.0;
    }
    if st.quick_save {
        st.quick_save = false;
        super::play::quick_save(&mut sim);
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
        let human = m.faction_by_key(&sim.human).map(|f| f.id);
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
    let treasury = m.faction_by_key(&sim.human).map_or(0, |f| f.treasury);
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
            hud.host.campaign_hover(Some(n));
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
/// (projected from their map position), and the original Labels.lua (driven by the root layout's
/// pulse) creates and moves the `city_info_bar` labels ("Settlement, Region" in the owner's
/// colours). PROVISIONAL: the label sits at the settlement's projected ground point; settlements
/// behind the HUD panels get no label; the one under the pointer is the nearest within 24 px.
/// Also reports the camera target and theatre to the HUD (`CameraTarget`, which the radar
/// follows) and applies a camera move the HUD asked for (a radar click).
pub fn labels(
    hud: Option<NonSendMut<CampaignHud>>,
    sim: Option<NonSend<CampaignSim>>,
    ground: Option<Res<super::scene::CampaignGround>>,
    camera: Query<(&Camera, &GlobalTransform), With<super::camera::CampaignCamera>>,
    mut rig: Query<&mut super::camera::CampaignCamera>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let (Some(mut hud), Some(sim), Some(ground), Ok((camera, cam_tf))) = (hud, sim, ground, camera.single()) else { return };
    if let Ok(mut cam) = rig.single_mut() {
        // The camera works in Bevy x / z, which is map x / -y.
        if let Some(to) = hud.camera_to.take() {
            cam.target = Vec2::new(to.x, -to.y).clamp(cam.min, cam.max);
        }
        hud.host.campaign_set_theatre_bounds((cam.min.x, -cam.max.y), (cam.max.x, -cam.min.y));
        hud.host.campaign_set_camera_target(cam.target.x, -cam.target.y, cam.distance);
    }
    let cursor = window.single().ok().and_then(|w| w.cursor_position());
    let mut list = Vec::new();
    let mut over: Option<(ntw_sim::campaign::RegionId, f32)> = None;
    for r in sim.model().world.regions.values() {
        let (x, z) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        let pos = Vec3::new(x, ground.0.height_at(x, z).max(0.0), -z);
        let Ok(p) = camera.world_to_viewport(cam_tf, pos) else { continue };
        if p.x < 0.0 || p.y < 0.0 || p.x > hud.screen.x || p.y > hud.screen.y || hud.covers(p) {
            continue;
        }
        if let Some(c) = cursor {
            let d = c.distance(p);
            if d < 24.0 && over.is_none_or(|(_, best)| d < best) {
                over = Some((r.id, d));
            }
        }
        let q = window_to_ui(p, hud.screen);
        list.push((r.id, q.x.round(), q.y.round()));
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

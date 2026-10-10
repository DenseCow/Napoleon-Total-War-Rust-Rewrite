//! The original battle HUD: `ui\battle ui\layout` and its UI scripts (root.lua, the cards panel,
//! the orders bar, the speed controls and timer, the deployment panel, the end-of-battle popups
//! and the post-battle results screen), run by the front end's `UiScriptHost` with the battle
//! engine side of `ntw_script::ui::battle`, drawn by `frontend::render::spawn_world`.
//!
//! Each frame the model is described to the scripts (`BattleHudFacts`: the player's units for the
//! cards, time, speed, phase, results) and their requests are applied to the model: speed buttons,
//! card selection, order buttons, Start Battle, end battle / exit. Clicks send `audio::UiSound`.
//! The engine calls the root script's event functions at the phase changes (CONFIRMED names in
//! root.lua: ShowDeploymentPopup, SetDeploymentPopupAsDeploymentStart, ClearDeploymentPanels,
//! ShowSinglePlayerEndPhasePopup, ShowBattleSummaryPopup); when exactly the original calls them is
//! INFERRED (see `analysis/battle/BATTLE_FLOW.md` §3).

use std::collections::HashMap;

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;
use ntw_script::ScriptSource;
use ntw_script::ui::battle::{self as ui_battle, BattleHudFacts, BattleUiRequest, HudPhase, HudSideResult, HudUnit};
use ntw_script::ui::{FrontEndFacts, NodeId, PointerEvent, UiScriptHost};
use ntw_sim::battle::morale::{MoraleBehaviour, MoraleState};
use ntw_sim::battle::orders::MoveSpeed;
use ntw_sim::battle::speed::BattleSpeed;
use ntw_sim::battle::victory;

use super::{BattlePhase, BattleSim};
use crate::GameMode;
use crate::data::GameData;
use crate::frontend::render::{UiAssets, UiSprite, spawn_world, ui_rect_to_window, ui_scale, ui_virtual_screen, window_to_ui};

/// The HUD's script host and pointer state. Non-`Send` (Lua).
pub struct BattleHud {
    host: UiScriptHost,
    screen: Vec2,
    drawn_generation: u64,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    /// The UI frame clock (ms, [`advance_ui_clock`]): what the root's `OnUpdatePulse` gets and
    /// `BattleUI.Time` reads.
    clock_ms: u32,
    /// Card portrait per faction, then unit key (without `.tga`).
    portraits: HashMap<String, HashMap<String, String>>,
    /// Flag folder per faction ([`flag_of`]).
    flags: HashMap<String, String>,
    /// The facts the last successful write left in `__battle` (`None`: unknown, the next write
    /// writes all), and a spare snapshot refreshed in place for the next frame; the two swap after
    /// each write, so the snapshots' strings and lists are reused.
    written_facts: Option<BattleHudFacts>,
    spare_facts: Option<BattleHudFacts>,
    /// The phase whose popups were opened last.
    shown: Option<BattlePhase>,
    /// Screen rectangles of the HUD panels (clicks there do not reach the battlefield).
    panels: Vec<Rect>,
    vfs: Vfs,
    /// Battle name (loc) for the results.
    battle_name: String,
    loc: Localisation,
    /// The per-frame steps whose error was logged.
    frame_errors: FrameErrors,
}

/// The per-frame HUD steps (model → HUD). A step that fails fails every frame, so each logs its
/// first error only.
#[derive(Default)]
struct FrameErrors {
    facts: bool,
    cards: bool,
    orders: bool,
}

/// Logs `result`'s error for the per-frame step `step` once: only when `logged` is not set yet,
/// which it then sets. Returns whether it logged.
fn log_once(logged: &mut bool, step: &str, result: mlua::Result<()>) -> bool {
    let Err(e) = result else { return false };
    if std::mem::replace(logged, true) {
        return false;
    }
    warn!("Battle HUD: {step}: {e} (logged once)");
    true
}

/// The longest frame the UI clock counts, in ms (the main loop's cap, `0x0048A906`).
const UI_FRAME_CAP_MS: u32 = 300;

/// One frame of the battle UI clock (ms): the clock the root's `OnUpdatePulse` gets and
/// `BattleUI.Time` reads. CONFIRMED from the main loop `0x0048A650`: each frame adds its real
/// elapsed time, `(float)µs * 0.001f` truncated to whole ms (`0x010A1E10`; the fraction is dropped
/// every frame, as the frame timer restarts), capped at 300 ms, to a u32 counter (MVC manager
/// `+0xB8`) that battle pause and speed never touch; the battle UI keeps it at `+0x28218` and its
/// seconds at `+0x2821C` (`0x005F4CF0`). Not modelled: the `frame_rate_test_fps` preference
/// (`0x0149B8C0`, a fixed 1000/fps step without the cap) and the `root_time` debug factor
/// (`0x0149F5E8`, 1.0).
fn advance_ui_clock(clock_ms: u32, frame: std::time::Duration) -> u32 {
    let ms = (frame.as_micros() as f64 as f32 * 0.001_f32) as u32;
    clock_ms.wrapping_add(ms.min(UI_FRAME_CAP_MS))
}

/// Whether the mouse is over the HUD this frame (read by `input::mouse`).
#[derive(Resource, Default)]
pub struct HudPointer {
    pub over: bool,
    /// Screen rectangles of the visible HUD panels (the unit labels keep clear of them).
    pub panels: Vec<Rect>,
}

/// Layout ids of the HUD panels that take clicks away from the battlefield.
const PANELS: [&str; 9] = [
    // land_battle_orders' first panel, named "orders" by battle_hud.lua's CreateFromLayout.
    "orders",
    "cards_panel",
    "stopwatch",
    "kill_ratio_PH",
    "deployment_end_sp",
    "SP_victory_options",
    "in_battle_results_popup",
    "mp_postbattle",
    "esc_menu_battle",
];

/// OnEnter(Battle), after the battle is built: loads the HUD.
pub fn enter(world: &mut World) {
    world.init_resource::<HudPointer>();
    let dir = crate::config::game_data_dir();
    let (Ok(vfs), Ok(assets_vfs), Ok(source), Ok(source2)) =
        (Vfs::open_install(&dir), Vfs::open_install(&dir), ScriptSource::from_install(&dir), ScriptSource::from_install(&dir))
    else {
        warn!("Battle HUD: install not readable");
        return;
    };
    let loc = Localisation::from_vfs(&vfs).unwrap_or_else(|e| {
        warn!("Battle HUD: localisation not loaded: {e}");
        Localisation::new()
    });
    let screen = world
        .query_filtered::<&Window, With<PrimaryWindow>>()
        .single(world)
        .map(|w| Vec2::new(w.width(), w.height()))
        .unwrap_or(Vec2::new(1280.0, 960.0));
    let facts = FrontEndFacts { game_version: "1.3.0".into(), user_dir: crate::config::user_dir(), ..Default::default() };
    // Laid out in the virtual screen of the original UI scale, like the campaign HUD (the device
    // applies it to every HUD; see `ui_scale`).
    let virt = ui_virtual_screen(screen);
    let limits = world.resource::<crate::data::GameData>().db.limits.clone();
    let host = match UiScriptHost::new(source, loc.clone(), facts, (virt.x, virt.y), limits) {
        Ok(h) => h,
        Err(e) => {
            warn!("Battle HUD: script host failed: {e}");
            return;
        }
    };
    if let Err(e) = ui_battle::install(&host, source2) {
        warn!("Battle HUD: engine functions not installed: {e}");
        return;
    }
    let battle_name = world
        .get_resource::<BattleSim>()
        .and_then(|s| s.battle_key.clone())
        .and_then(|k| loc.get(&format!("battles_localised_name_{k}")).map(str::to_owned))
        .unwrap_or_default();
    let mut hud = BattleHud {
        host,
        screen,
        drawn_generation: u64::MAX,
        hovered: None,
        pressed: None,
        clock_ms: 0,
        portraits: HashMap::new(),
        flags: HashMap::new(),
        written_facts: None,
        spare_facts: None,
        shown: None,
        panels: Vec::new(),
        vfs,
        battle_name,
        loc,
        frame_errors: FrameErrors::default(),
    };
    if let Some(sim) = world.get_resource::<BattleSim>()
        && let Err(e) = write_facts(&mut hud, sim, &world.resource::<GameData>().db)
    {
        warn!("Battle HUD: {e}");
    }
    if let Err(e) = ui_battle::load_hud(&hud.host) {
        warn!("Battle HUD: layout not loaded: {e}");
        return;
    }
    if let Err(e) = ui_battle::create_cards(&hud.host) {
        warn!("Battle HUD: unit cards: {e}");
    }
    // PLACEHOLDER: the radar (minimap) is not drawn yet, so its empty frame is hidden.
    let _ = hud
        .host
        .lua()
        .load("local r = UIComponent(__ntw_battle_root_env.Address); local c = r:Find('radar_group'); if c then UIComponent(c):SetVisible(false) end")
        .exec();
    log_errors(&hud.host);
    world.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }, DespawnOnExit(GameMode::Battle)));
    world.insert_resource(UiAssets::new(assets_vfs));
    world.insert_non_send(hud);
}

/// A battle script's `ui_component(name):set_visible(b)`: shows or hides that HUD component.
/// The radar stays hidden (PLACEHOLDER: it is not drawn yet).
pub fn set_component_visible(world: &mut World, name: &str, visible: bool) {
    if name == "radar_group" {
        return;
    }
    let Some(hud) = world.get_non_send::<BattleHud>() else { return };
    let quoted: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let code = format!(
        "local r = UIComponent(__ntw_battle_root_env.Address); local c = r:Find('{quoted}'); if c then UIComponent(c):SetVisible({visible}) end"
    );
    let _ = hud.host.lua().load(code).exec();
    log_errors(&hud.host);
}

fn log_errors(host: &UiScriptHost) {
    for l in host.take_log() {
        if l.starts_with("ERROR") {
            warn!("Battle HUD script: {l}");
        }
    }
}

/// The card portrait of a unit: `ui/units/icons/<faction icon folder>_<key>_icon.tga` (CONFIRMED
/// file names, e.g. `french_rep_ita_inf_line_french_fusiliers_icon.tga`), with the faction's
/// `unit_icon_path` as the prefix (INFERRED), else any icon of that unit. Worked out once per
/// faction and unit key and kept in `portraits` (faction → key → path), looked up by `&str`.
fn portrait<'a>(portraits: &'a mut HashMap<String, HashMap<String, String>>, vfs: &Vfs, db: &ntw_data::GameDatabase, faction: &str, key: &str) -> &'a str {
    if !portraits.get(faction).is_some_and(|m| m.contains_key(key)) {
        let path = find_portrait(vfs, db, faction, key);
        portraits.entry(faction.to_owned()).or_default().insert(key.to_owned(), path);
    }
    portraits.get(faction).and_then(|m| m.get(key)).map_or("", String::as_str)
}

/// [`portrait`]'s search, without the cache.
fn find_portrait(vfs: &Vfs, db: &ntw_data::GameDatabase, faction: &str, key: &str) -> String {
    let lower = key.to_ascii_lowercase();
    let mut prefixes: Vec<String> = Vec::new();
    if let Some(f) = db.faction(faction) {
        let p = f.unit_icon_path.replace('\\', "/");
        prefixes.push(p.trim_matches('/').rsplit('/').next().unwrap_or_default().to_ascii_lowercase());
    }
    prefixes.push(faction.to_ascii_lowercase());
    let mut found = prefixes
        .iter()
        .filter(|p| !p.is_empty())
        .map(|p| format!("ui/units/icons/{p}_{lower}_icon"))
        .find(|p| vfs.read(&format!("{p}.tga")).is_ok());
    if found.is_none() {
        let suffix = format!("_{lower}_icon.tga");
        found = vfs.list("ui/units/icons/").into_iter().find(|p| p.to_ascii_lowercase().ends_with(&suffix)).map(|p| {
            let p = p.replace('\\', "/");
            p[..p.len() - 4].to_owned()
        });
    }
    found.map(|p| format!("data/{p}")).unwrap_or_default()
}

/// A faction's flag folder as the HUD scripts use it (`FlagPath` .. `/HUD_right.tga`): the
/// `factions` flag folder (#13) with `data/` in front (INFERRED form). Made once per faction and
/// kept in `flags`, looked up by `&str`.
fn flag_of<'a>(flags: &'a mut HashMap<String, String>, db: &ntw_data::GameDatabase, faction: &str) -> &'a str {
    if !flags.contains_key(faction) {
        let flag = db.faction(faction).map(|f| format!("data/{}", f.flag_path.replace('\\', "/").trim_matches('/'))).unwrap_or_default();
        flags.insert(faction.to_owned(), flag);
    }
    flags.get(faction).map_or("", String::as_str)
}

/// Sets `s` to `v` when they differ, keeping its buffer (an unchanged fact allocates nothing).
fn set_str(s: &mut String, v: &str) {
    if s != v {
        s.clear();
        s.push_str(v);
    }
}

/// The model, as the HUD scripts see it, written into `out` in place (each frame: its strings and
/// lists keep their buffers, so a frame allocates nothing when no name or unit changed). Every
/// field is set: the destructuring patterns name them all, so a new fact does not compile until it
/// is filled here.
fn refresh_facts(out: &mut BattleHudFacts, hud: &mut BattleHud, sim: &BattleSim, db: &ntw_data::GameDatabase) {
    let b = &sim.battle;
    let BattleHudFacts { phase, elapsed_s, total_s, speed, units, naval, player_won, results, battle_name, player_faction, player_flag, balance } = out;
    let mut n = 0;
    for (u, info) in sim.units_with_info() {
        if !info.controllable {
            continue;
        }
        if n == units.len() {
            units.push(HudUnit::default());
        }
        let HudUnit {
            id,
            key,
            name,
            kills,
            portrait: portrait_path,
            men,
            max_men,
            guns,
            max_guns,
            is_artillery,
            has_ammo,
            ammo_percent,
            experience,
            wavering,
            routing,
            walking,
            running,
            firing,
            melee,
            under_fire,
            selected,
            category,
            fire_at_will,
        } = &mut units[n];
        n += 1;
        let stats = db.unit_stats(&info.key);
        let max_ammo = stats.map_or(0, |s| s.ammunition.max(0) as u32);
        *id = u.id;
        set_str(key, &info.key);
        set_str(name, info.general.as_deref().unwrap_or(&info.name));
        *kills = u.kills;
        set_str(portrait_path, portrait(&mut hud.portraits, &hud.vfs, db, &info.faction, &info.key));
        *men = u.men;
        *max_men = u.max_men;
        *guns = stats.map_or(0, |s| s.num_guns.max(0) as u32);
        *max_guns = *guns;
        *is_artillery = stats.is_some_and(|s| s.is_artillery);
        *has_ammo = u.missile.is_some() && u.ammunition > 0;
        // Despite its name, the card script's `AmmoRemainingAsPercent` is a 0..1 fraction: it scales the bar
        // as RoundToInt(bar_height * value). INFERRED: with 0..100 every bar was drawn 100x too tall
        // (green lines up the whole screen); with 0..1 it fits the card (template.battleunitcard.luac).
        *ammo_percent = if max_ammo > 0 { u.ammunition as f32 / max_ammo as f32 } else { 0.0 };
        *experience = info.experience;
        *wavering = u.morale.state == MoraleState::Wavering;
        *routing = matches!(u.morale.behaviour, MoraleBehaviour::Routing | MoraleBehaviour::Shattered);
        *walking = u.moved && !u.charging;
        *running = u.running || u.charging;
        *firing = u.fired_this_tick || (u.reload_ticks_left > 0 && u.fire_target.is_some());
        *melee = u.in_melee;
        *under_fire = u.under_fire_ticks > 0;
        *selected = sim.selected == Some(u.id);
        set_str(category, &info.category);
        *fire_at_will = u.fire_at_will;
    }
    units.truncate(n);
    results.resize_with(2, HudSideResult::default);
    for (side, r) in results.iter_mut().enumerate() {
        let t = victory::totals(b, side as u8);
        let HudSideResult { name, faction, flag, men_start, men_alive, kills, units_start, units_left } = r;
        set_str(name, &sim.side_names[side]);
        set_str(faction, &sim.side_factions[side]);
        set_str(flag, flag_of(&mut hud.flags, db, &sim.side_factions[side]));
        *men_start = t.men_start;
        *men_alive = t.men_alive;
        *kills = t.kills;
        *units_start = t.units_start;
        *units_left = t.units_fighting;
    }
    let (mine, theirs) = (results[0].men_alive as f32, results[1].men_alive as f32);
    *phase = match sim.phase {
        BattlePhase::Deployment => HudPhase::Deployment,
        BattlePhase::Conflict => HudPhase::Conflict,
        BattlePhase::Finished => HudPhase::Finished,
    };
    *elapsed_s = b.time_seconds();
    *total_s = sim.victory.time_limit_s.unwrap_or(0.0);
    *speed = sim.speed.multiplier();
    *naval = false;
    *player_won = sim.outcome.is_over().then(|| sim.outcome.winner() == Some(0));
    set_str(battle_name, &hud.battle_name);
    set_str(player_faction, &sim.side_factions[0]);
    set_str(player_flag, flag_of(&mut hud.flags, db, &sim.side_factions[0]));
    *balance = if mine + theirs > 0.0 { mine / (mine + theirs) } else { 0.5 };
}

/// Model → `__battle`: refreshes the spare snapshot in place and writes what changed since the
/// last successful write; after a failed write the next one writes all.
fn write_facts(hud: &mut BattleHud, sim: &BattleSim, db: &ntw_data::GameDatabase) -> mlua::Result<()> {
    let mut facts = hud.spare_facts.take().unwrap_or_default();
    refresh_facts(&mut facts, hud, sim, db);
    let result = ui_battle::update_facts(&hud.host, &facts, hud.written_facts.as_ref());
    if result.is_ok() {
        hud.spare_facts = hud.written_facts.replace(facts);
    } else {
        hud.written_facts = None;
        hud.spare_facts = Some(facts);
    }
    result
}

/// Every frame: model → HUD, phase popups, pointer → HUD, HUD requests → model.
#[allow(clippy::too_many_arguments)]
pub fn frame(
    hud: Option<NonSendMut<BattleHud>>,
    sim: Option<ResMut<BattleSim>>,
    data: Res<GameData>,
    window: Query<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    time: Res<Time<Real>>,
    mut pointer: ResMut<HudPointer>,
    mut sounds: MessageWriter<crate::audio::UiSound>,
    mut music: MessageWriter<crate::audio::SetMusic>,
    mut next: ResMut<NextState<GameMode>>,
) {
    let (Some(mut hud), Some(mut sim), Ok(window)) = (hud, sim, window.single()) else { return };
    let size = Vec2::new(window.width(), window.height());
    if hud.screen != size {
        hud.screen = size;
        let virt = ui_virtual_screen(size);
        hud.host.set_screen(virt.x, virt.y);
    }
    // Model → HUD.
    let player_won = {
        let hud = &mut *hud;
        let written = write_facts(hud, &sim, &data.db);
        log_once(&mut hud.frame_errors.facts, "facts", written);
        log_once(&mut hud.frame_errors.cards, "unit cards", ui_battle::update_cards(&hud.host));
        log_once(&mut hud.frame_errors.orders, "order buttons", ui_battle::update_orders(&hud.host));
        hud.written_facts.as_ref().or(hud.spare_facts.as_ref()).and_then(|f| f.player_won)
    };
    hud.clock_ms = advance_ui_clock(hud.clock_ms, time.delta());
    let t = hud.clock_ms;
    hud.host.pulse(f64::from(t));
    // Phase popups (the engine's calls into root.lua) and the music of each phase.
    if hud.shown != Some(sim.phase) {
        match sim.phase {
            BattlePhase::Deployment => {
                let _ = ui_battle::call_event(&hud.host, "ShowDeploymentPopup");
                let _ = ui_battle::call_event(&hud.host, "SetDeploymentPopupAsDeploymentStart");
                music.write(music_for(&sim, &data.db, "music_land_deployment"));
            }
            BattlePhase::Conflict => {
                if hud.shown == Some(BattlePhase::Deployment) {
                    let _ = ui_battle::call_event(&hud.host, "ClearDeploymentPanels");
                }
                if hud.shown != Some(BattlePhase::Finished) {
                    music.write(music_for(&sim, &data.db, "music_land_battle"));
                }
            }
            BattlePhase::Finished => {
                // INFERRED: a single-player battle first offers "continue / end battle"
                // (SP_victory_options); End leads to the summary popup, then the results screen.
                let _ = hud.host.lua().globals().set("is_winner", player_won == Some(true));
                let _ = ui_battle::call_event(&hud.host, "ShowSinglePlayerEndPhasePopup");
                music.write(music_for(&sim, &data.db, "music_land_battle_results"));
            }
        }
        hud.shown = Some(sim.phase);
    }
    // Pointer → HUD (harness runs ignore the live mouse).
    let mut over = false;
    if harness.is_none() && !ai_shot_run() {
        let cursor = window.cursor_position();
        let hit = cursor.and_then(|p| {
            let q = window_to_ui(p, hud.screen);
            hud.host.hit(q.x, q.y)
        });
        over = hit.is_some() || cursor.is_some_and(|p| hud.panels.iter().any(|r| r.contains(p)));
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
                click(&hud.host, p, keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight), &mut sounds);
            } else {
                hud.host.pointer(p, PointerEvent::LeftUpElsewhere);
            }
        }
    }
    pointer.over = over;
    pointer.panels.clone_from(&hud.panels);
    // Keys the scripts stole (ESCAPE opens the battle's escape menu, RETURN confirms).
    for (k, name) in [(KeyCode::Escape, "ESCAPE"), (KeyCode::Enter, "RETURN")] {
        if keys.just_released(k) {
            hud.host.key(name);
        }
    }
    log_errors(&hud.host);
    // HUD → model.
    let requests = ui_battle::take_requests(&hud.host).unwrap_or_else(|e| {
        warn!("Battle HUD: {e}");
        Vec::new()
    });
    for r in requests {
        apply(&mut hud, &mut sim, r, &mut next);
    }
}

/// A completed click on a HUD component: the click sound, the component's own handlers, then the
/// card selection.
fn click(host: &UiScriptHost, node: NodeId, add: bool, sounds: &mut MessageWriter<crate::audio::UiSound>) {
    let id = host.world().get(node).filter(|n| !n.current().is_some_and(|s| s.disabled)).map(|n| n.data.id.clone());
    if let Some(component) = id {
        sounds.write(crate::audio::UiSound::click(component));
    }
    host.pointer(node, PointerEvent::LeftUp);
    let _ = ui_battle::click(host, node, add);
}

/// The music state for a battle phase with the player's subculture (`factions` #2), e.g.
/// `music_land_battle` + `sc_european_west`.
fn music_for(sim: &BattleSim, db: &ntw_data::GameDatabase, state: &str) -> crate::audio::SetMusic {
    let subculture = db.faction(&sim.side_factions[0]).map(|f| f.subculture.clone()).filter(|s| !s.is_empty());
    crate::audio::SetMusic { state: Some(state.to_owned()), subculture }
}

/// Applies one HUD request to the model.
fn apply(hud: &mut BattleHud, sim: &mut BattleSim, r: BattleUiRequest, next: &mut NextState<GameMode>) {
    match r {
        BattleUiRequest::SetSpeed(m) => {
            let s = BattleSpeed::ALL.into_iter().min_by(|a, b| (a.multiplier() - m).abs().total_cmp(&(b.multiplier() - m).abs())).unwrap_or_default();
            if s == BattleSpeed::Paused && sim.speed != BattleSpeed::Paused {
                sim.speed_before_pause = sim.speed;
            }
            sim.speed = s;
        }
        BattleUiRequest::CycleSpeed => sim.speed = sim.speed.cycle(),
        BattleUiRequest::DeploymentFinished => {
            if sim.phase == BattlePhase::Deployment {
                info!("Battle: deployment finished, the battle starts");
                sim.phase = BattlePhase::Conflict;
            }
        }
        BattleUiRequest::Select { ids, add: _ } => {
            // PROVISIONAL: the model's player selection holds one unit; the first card wins.
            sim.selected = ids.into_iter().find(|id| sim.info_of(*id).is_some_and(|i| i.controllable));
        }
        BattleUiRequest::Order { name, arg } => order(sim, &name, arg),
        BattleUiRequest::ExitBattle => {
            info!("Battle: exit to the front end");
            next.set(GameMode::FrontEnd);
        }
        BattleUiRequest::ContinueBattle => {
            // Keep fighting after the result (PostBattleDismissContinueBattle).
            info!("Battle: continue after the result");
            sim.continued = true;
            sim.phase = BattlePhase::Conflict;
            hud.shown = Some(BattlePhase::Conflict);
        }
        BattleUiRequest::EndBattle => {
            // FinishBattle → the battle summary popup (in_battle_results_popup); its Close leads to
            // root.lua's DismissBattleResult → the post-battle results screen (mp_postbattle).
            let text = summary_text(hud, sim);
            info!("Battle: end battle: {text}");
            let r = ui_battle::call_global(&hud.host, "ShowBattleSummaryPopup", |lua| {
                Ok(mlua::MultiValue::from_vec(vec![mlua::Value::String(lua.create_string(&text)?)]))
            });
            if let Err(e) = r {
                warn!("Battle HUD: ShowBattleSummaryPopup: {e}");
            }
        }
        // The results screen's Exit (root.lua ClosePopup → InformOfBattleSummaryDismiss; for a
        // campaign battle DismissBattleResult calls it directly): the battle is over, leave it.
        // PROVISIONAL: always to the front end (the campaign return through CampaignModel's
        // pending_battle / apply_battle_result is not wired yet).
        BattleUiRequest::SummaryDismissed => {
            if sim.phase == BattlePhase::Finished || sim.continued {
                info!("Battle: results dismissed, back to the front end");
                next.set(GameMode::FrontEnd);
            }
        }
        BattleUiRequest::ZoomTo(_) => {}
    }
}

/// The battle summary's main text: the historical battle script's own Won/Lost line
/// (`battle_script_strings_string_CreativeAssembly.HB_<name>_Battle_Won|Lost`, CONFIRMED loc keys;
/// that the popup shows them is INFERRED), else the outcome name from `random_localisation_strings`
/// (`battle_victory_minor_decisive`, ...; the grading rule is PROVISIONAL: decisive when the
/// winner lost under a quarter of its men, else close).
fn summary_text(hud: &BattleHud, sim: &BattleSim) -> String {
    let won = sim.outcome.winner() == Some(0);
    if let Some(script) = sim.battle_script.as_deref() {
        let name = script.trim_end_matches("_Battle").trim_end_matches("_battle");
        let key = format!("battle_script_strings_string_CreativeAssembly.HB_{name}_Battle_{}", if won { "Won" } else { "Lost" });
        if let Some(t) = hud.loc.get(&key) {
            return t.to_owned();
        }
    }
    let grade = |side: u8| {
        let t = victory::totals(&sim.battle, side);
        if t.men_start > 0 && (t.men_start - t.men_alive) * 4 < t.men_start { "decisive" } else { "close" }
    };
    let key = match sim.outcome.winner() {
        None => "battle_draw".to_owned(),
        Some(0) => format!("battle_victory_minor_{}", grade(0)),
        Some(w) => format!("battle_defeat_minor_{}", grade(w)),
    };
    hud.loc.get(&format!("random_localisation_strings_string_{key}")).unwrap_or_default().to_owned()
}

/// An order button for the selected unit, by its `BattleUI` binding name. Implemented with the
/// existing model orders; PROVISIONAL where noted.
fn order(sim: &mut BattleSim, name: &str, arg: Option<bool>) {
    let Some(id) = sim.selected else { return };
    let Some(i) = sim.battle.unit_index(id) else { return };
    match name {
        "Current_Selection_Halt" | "CurrentSelectionHalt" | "CancelOrderForSelection" => {
            sim.battle.order_halt(id);
            sim.script_events.push(super::ScriptEvent::Command { name: "Halt", unit: Some(id), bool1: false });
            return;
        }
        "Fire_At_Will" | "Current_Selection_Enable_Fire_At_Will" => {
            let on = arg.unwrap_or(!sim.battle.units[i].fire_at_will);
            sim.battle.order_fire_at_will(id, on);
            sim.script_events.push(super::ScriptEvent::Command { name: "Fire At Will", unit: Some(id), bool1: on });
            return;
        }
        "Current_Selection_Start_Firing_At_Will" | "Current_Selection_Stop_Firing_At_Will" => {
            sim.battle.order_fire_at_will(id, name.contains("Start"));
            sim.script_events.push(super::ScriptEvent::Command { name: "Fire At Will", unit: Some(id), bool1: name.contains("Start") });
            return;
        }
        // The run/walk toggle (land_hud_move_speed.lua: Walks when the button is selected, else
        // Runs): a change of speed of the unit's current move (`Battle::order_move_speed`).
        "Current_Selection_Walks" | "CurrentSelectionWalks" | "Current_Selection_Runs" | "CurrentSelectionRuns" => {
            let run = name.ends_with("Runs");
            sim.battle.order_move_speed(id, if run { MoveSpeed::Run } else { MoveSpeed::Walk });
            sim.script_events.push(super::ScriptEvent::Command { name: "Change Speed", unit: Some(id), bool1: run });
            return;
        }
        _ => {}
    }
    let u = &mut sim.battle.units[i];
    match name {
        // PROVISIONAL: 10 m per click and 15° per turn (the original's step sizes are UNKNOWN).
        "Current_Selection_Move_Forwards" | "Current_Selection_Move_Backwards" => {
            let d = if name.ends_with("Forwards") { 10.0 } else { -10.0 };
            u.set_destination(Some((u.position.0 + u.facing.cos() * d, u.position.1 + u.facing.sin() * d)));
        }
        "Current_Selection_Rotate_Left" => u.facing += 15f32.to_radians(),
        "Current_Selection_Rotate_Right" => u.facing -= 15f32.to_radians(),
        // Not in the model yet: melee mode, withdraw, formations, groups, special abilities.
        other => info!("Battle HUD: order {other} is not done yet (PROVISIONAL)"),
    }
}

/// Rebuilds the HUD sprites when the UI tree changed, and records the panel rectangles.
pub fn redraw(
    mut commands: Commands,
    hud: Option<NonSendMut<BattleHud>>,
    assets: Option<ResMut<UiAssets>>,
    mut images: ResMut<Assets<Image>>,
    sprites: Query<Entity, With<UiSprite>>,
    new_sprites: Query<Entity, Added<UiSprite>>,
) {
    for e in &new_sprites {
        commands.entity(e).insert(DespawnOnExit(GameMode::Battle));
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
            if PANELS.contains(&node.data.id.as_str()) {
                panels.push(ui_rect_to_window(node.rect, hud.screen));
            }
        });
        drop(world);
        hud.panels = panels;
    }
    hud.drawn_generation = generation;
}

/// Leaving the battle: drop the HUD.
pub fn leave(world: &mut World) {
    world.remove_non_send::<BattleHud>();
    world.remove_resource::<UiAssets>();
}

/// True in `NAPOLEON_AI_SHOT` harness runs (battle_ai.rs): they must not depend on the live mouse.
fn ai_shot_run() -> bool {
    std::env::var_os("NAPOLEON_AI_SHOT").is_some()
}

/// The HUD clicks a harness run makes: `--battle-ui-click id,...` as given; otherwise, in an
/// `NAPOLEON_AI_SHOT` run that starts in deployment (no `--skip-deployment`), wait a second and
/// click the deployment panel's Start Battle button (`button_battle_start`) like a player.
///
/// Without this, such a run stayed in deployment for ever: nothing ends deployment by itself
/// (CONFIRMED by repeated runs at 12ced63 and 01c5155), and the occasional runs that did start
/// had picked up a live mouse click on the window (the harness used to read the live mouse).
pub fn harness_clicks(args: &[String], ai_shot: bool) -> Vec<String> {
    if let Some(ids) = args.iter().position(|a| a == "--battle-ui-click").and_then(|i| args.get(i + 1)) {
        return ids.split(',').map(str::to_owned).collect();
    }
    if ai_shot && !args.iter().any(|a| a == "--skip-deployment") {
        return vec!["wait".to_owned(), "button_battle_start".to_owned()];
    }
    Vec::new()
}

/// Test harness: `--battle-ui-click id,...` clicks HUD components (one per second, after 2 s;
/// `end:win` / `end:lose` decide the battle, `wait` skips a second).
/// With `--screenshot`, the picture waits until the clicks are done.
pub fn harness(
    hud: Option<NonSend<BattleHud>>,
    shot: Option<ResMut<crate::frontend::AutoScreenshot>>,
    time: Res<Time<Real>>,
    mut state: Local<Option<(Vec<String>, f32)>>,
    mut sounds: MessageWriter<crate::audio::UiSound>,
    sim: Option<ResMut<BattleSim>>,
) {
    let Some(hud) = hud else { return };
    let st = state.get_or_insert_with(|| {
        let args: Vec<String> = std::env::args().collect();
        (harness_clicks(&args, ai_shot_run()), -1.0)
    });
    if st.0.is_empty() {
        return;
    }
    st.1 += time.delta_secs();
    if let Some(mut shot) = shot {
        shot.secs = 0.0;
    }
    if st.1 < 1.0 {
        return;
    }
    st.1 = 0.0;
    let id = st.0.remove(0);
    // `end:win` / `end:lose` decide the battle at once (to reach the end-of-battle screens);
    // `wait` does nothing for a second.
    if let Some(r) = id.strip_prefix("end:") {
        if let Some(mut sim) = sim {
            sim.outcome = victory::Outcome::Won { side: if r == "win" { 0 } else { 1 } };
            sim.phase = BattlePhase::Finished;
            info!("Harness: battle decided ({r})");
        }
        return;
    }
    if id == "wait" {
        return;
    }
    let Some(root) = hud.host.root() else { return };
    let mut target = None;
    hud.host.world().visit_visible(root, &mut |n, node| {
        if target.is_none() && node.data.id == id {
            target = Some(n);
        }
    });
    match target {
        Some(t) => {
            info!("Harness: battle HUD click on {id}");
            hud.host.pointer(t, PointerEvent::Enter);
            hud.host.pointer(t, PointerEvent::LeftDown);
            click(&hud.host, t, false, &mut sounds);
            hud.host.pointer(t, PointerEvent::Leave);
        }
        None => warn!("Harness: no visible HUD component {id}"),
    }
}

#[cfg(test)]
mod harness_tests {
    use super::{UI_FRAME_CAP_MS, advance_ui_clock, harness_clicks, log_once};
    use std::time::Duration;

    /// The battle UI clock follows the exe's main loop (`0x0048A650`): each frame's real time in
    /// whole ms, the fraction dropped, at most 300 ms a frame (it used to add the exact delta with
    /// no cap, so a stall jumped scripted transitions by the whole gap).
    #[test]
    fn the_ui_clock_counts_whole_ms_per_frame_capped_at_300() {
        // 60 fps: 16.666 ms counts 16 each frame, so 60 frames make 960 ms, not 1000.
        let mut t = 0;
        for _ in 0..60 {
            t = advance_ui_clock(t, Duration::from_micros(16_666));
        }
        assert_eq!(t, 960);
        assert_eq!(advance_ui_clock(5, Duration::from_millis(2_000)), 5 + UI_FRAME_CAP_MS, "a stall counts 300 ms");
        assert_eq!(advance_ui_clock(7, Duration::from_micros(999)), 7, "under a ms counts nothing");
        assert_eq!(advance_ui_clock(u32::MAX, Duration::from_millis(2)), 1, "a u32 counter");
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// Regression: an `NAPOLEON_AI_SHOT` run without `--skip-deployment` must click Start Battle
    /// (it used to wait for a live mouse click and stayed in deployment for ever).
    #[test]
    fn ai_shot_runs_click_start_battle() {
        assert_eq!(harness_clicks(&args(&["napoleon", "--battle"]), true), vec!["wait", "button_battle_start"]);
        assert!(harness_clicks(&args(&["napoleon", "--battle", "--skip-deployment"]), true).is_empty());
        assert!(harness_clicks(&args(&["napoleon", "--battle"]), false).is_empty());
        // Explicit clicks win.
        assert_eq!(harness_clicks(&args(&["napoleon", "--battle-ui-click", "a,b"]), true), vec!["a", "b"]);
    }

    /// The per-frame card and order updates dropped their errors (`let _ =`); a broken step now
    /// logs its first error only, not one line per frame.
    #[test]
    fn a_failing_frame_step_logs_once() {
        let mut logged = false;
        assert!(!log_once(&mut logged, "unit cards", Ok(())));
        assert!(!logged);
        assert!(log_once(&mut logged, "unit cards", Err(mlua::Error::runtime("broken"))));
        for _ in 0..3 {
            assert!(!log_once(&mut logged, "unit cards", Err(mlua::Error::runtime("broken"))));
        }
    }
}

#[cfg(test)]
mod order_tests {
    use super::{BattleSim, order};
    use ntw_sim::battle::TICK_SECONDS;
    use ntw_sim::battle::fatigue::KvFatigue;
    use ntw_sim::battle::model::{Battle, LandUnit};
    use super::super::UnitInfo;
    use ntw_sim::battle::morale::KvMorale;

    fn sim() -> BattleSim {
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        let mut me = LandUnit::new(1, 0, 100, (0.0, 0.0));
        (me.walk_speed, me.run_speed) = (1.4, 3.6);
        sim.add_unit(me, UnitInfo { id: 1, ..UnitInfo::default() });
        let mut enemy = LandUnit::new(2, 1, 100, (500.0, 0.0));
        enemy.hold_position = true;
        sim.add_unit(enemy, UnitInfo { id: 2, ..UnitInfo::default() });
        sim.selected = Some(1);
        sim
    }

    fn step_x(sim: &mut BattleSim) -> f32 {
        let x = sim.battle.units[0].position.0;
        sim.battle.step();
        sim.battle.units[0].position.0 - x
    }

    /// The user's report (2026-10-08): the HUD Run button did nothing. The button set the unit's
    /// run flag, but a unit moving without a move order of its own (the advance on the enemy every
    /// unit makes at the start of a battle) kept walking: only a destination read the flag. The
    /// original's change of speed edits whatever move the unit is making (`0x005600C0`).
    #[test]
    fn the_run_button_makes_a_moving_unit_run() {
        let mut sim = sim();
        assert!((step_x(&mut sim) - 1.4 * TICK_SECONDS).abs() < 1e-4, "walks before");
        order(&mut sim, "Current_Selection_Runs", None);
        assert!((step_x(&mut sim) - 3.6 * TICK_SECONDS).abs() < 1e-4, "runs after the Run button");
        order(&mut sim, "Current_Selection_Walks", None);
        assert!((step_x(&mut sim) - 1.4 * TICK_SECONDS).abs() < 1e-4, "walks after the Walk button");
        assert!(sim.script_events.iter().any(|e| matches!(e, super::super::ScriptEvent::Command { name: "Change Speed", bool1: true, .. })));
    }

    /// A unit that is not moving keeps no run option (it belongs to an order), so the button
    /// changes nothing on it and the HUD shows it walking.
    #[test]
    fn the_run_button_keeps_nothing_on_a_standing_unit() {
        let mut sim = sim();
        sim.battle.units[0].hold_position = true;
        order(&mut sim, "Current_Selection_Runs", None);
        assert!(!sim.battle.units[0].running);
    }
}

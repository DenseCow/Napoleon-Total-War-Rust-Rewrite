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
use ntw_sim::battle::speed::BattleSpeed;
use ntw_sim::battle::victory;

use super::{BattlePhase, BattleSim};
use crate::GameMode;
use crate::data::GameData;
use crate::frontend::render::{UiAssets, UiSprite, spawn_world};

/// The HUD's script host and pointer state. Non-`Send` (Lua).
pub struct BattleHud {
    host: UiScriptHost,
    screen: Vec2,
    drawn_generation: u64,
    hovered: Option<NodeId>,
    pressed: Option<NodeId>,
    clock_ms: f64,
    /// Card portrait per faction/unit key (without `.tga`).
    portraits: HashMap<String, String>,
    /// The phase whose popups were opened last.
    shown: Option<BattlePhase>,
    /// Screen rectangles of the HUD panels (clicks there do not reach the battlefield).
    panels: Vec<Rect>,
    vfs: Vfs,
    /// Battle name (loc) for the results.
    battle_name: String,
    loc: Localisation,
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
    "land_battle_orders",
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
    let host = match UiScriptHost::new(source, loc.clone(), facts, (screen.x, screen.y)) {
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
        clock_ms: 0.0,
        portraits: HashMap::new(),
        shown: None,
        panels: Vec::new(),
        vfs,
        battle_name,
        loc,
    };
    let facts = world.get_resource::<BattleSim>().map(|sim| facts_of(&mut hud, sim, &world.resource::<GameData>().db));
    if let Some(f) = &facts
        && let Err(e) = ui_battle::set_facts(&hud.host, f)
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
/// `unit_icon_path` as the prefix (INFERRED), else any icon of that unit.
fn portrait(hud: &mut BattleHud, db: &ntw_data::GameDatabase, faction: &str, key: &str) -> String {
    let cache_key = format!("{faction}/{key}");
    if let Some(p) = hud.portraits.get(&cache_key) {
        return p.clone();
    }
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
        .find(|p| hud.vfs.read(&format!("{p}.tga")).is_ok());
    if found.is_none() {
        let suffix = format!("_{lower}_icon.tga");
        found = hud.vfs.list("ui/units/icons/").into_iter().find(|p| p.to_ascii_lowercase().ends_with(&suffix)).map(|p| {
            let p = p.replace('\\', "/");
            p[..p.len() - 4].to_owned()
        });
    }
    let path = found.map(|p| format!("data/{p}")).unwrap_or_default();
    hud.portraits.insert(cache_key, path.clone());
    path
}

/// A faction's flag folder as the HUD scripts use it (`FlagPath` .. `/HUD_right.tga`): the
/// `factions` flag folder (#13) with `data/` in front (INFERRED form).
fn flag_of(db: &ntw_data::GameDatabase, faction: &str) -> String {
    db.faction(faction).map(|f| format!("data/{}", f.flag_path.replace('\\', "/").trim_matches('/'))).unwrap_or_default()
}

/// The model, as the HUD scripts see it.
fn facts_of(hud: &mut BattleHud, sim: &BattleSim, db: &ntw_data::GameDatabase) -> BattleHudFacts {
    let b = &sim.battle;
    let mut units = Vec::new();
    for (u, info) in b.units.iter().zip(&sim.info) {
        if !info.controllable {
            continue;
        }
        let stats = db.unit_stats(&info.key);
        let is_artillery = stats.is_some_and(|s| s.is_artillery);
        let guns = stats.map_or(0, |s| s.num_guns.max(0) as u32);
        let max_ammo = stats.map_or(0, |s| s.ammunition.max(0) as u32);
        units.push(HudUnit {
            id: u.id,
            key: info.key.clone(),
            name: info.general.clone().unwrap_or_else(|| info.name.clone()),
            kills: u.kills,
            portrait: portrait(hud, db, &info.faction, &info.key),
            men: u.men,
            max_men: u.max_men,
            guns,
            max_guns: guns,
            is_artillery,
            has_ammo: u.missile.is_some() && u.ammunition > 0,
            // Despite its name, the card script's `AmmoRemainingAsPercent` is a 0..1 fraction: it scales the bar
            // as RoundToInt(bar_height * value). INFERRED: with 0..100 every bar was drawn 100x too tall
            // (green lines up the whole screen); with 0..1 it fits the card (template.battleunitcard.luac).
            ammo_percent: if max_ammo > 0 { u.ammunition as f32 / max_ammo as f32 } else { 0.0 },
            experience: info.experience,
            wavering: u.morale.state == MoraleState::Wavering,
            routing: matches!(u.morale.behaviour, MoraleBehaviour::Routing | MoraleBehaviour::Shattered),
            walking: u.moved && !u.charging,
            running: u.running || u.charging,
            firing: u.fired_this_tick || (u.reload_ticks_left > 0 && u.fire_target.is_some()),
            melee: u.in_melee,
            under_fire: u.under_fire_ticks > 0,
            selected: sim.selected == Some(u.id),
            category: info.category.clone(),
            fire_at_will: u.fire_at_will,
        });
    }
    let results = [0u8, 1]
        .into_iter()
        .map(|side| {
            let t = victory::totals(b, side);
            HudSideResult {
                name: sim.side_names[side as usize].clone(),
                faction: sim.side_factions[side as usize].clone(),
                flag: flag_of(db, &sim.side_factions[side as usize]),
                men_start: t.men_start,
                men_alive: t.men_alive,
                kills: t.kills,
                units_start: t.units_start,
                units_left: t.units_fighting,
            }
        })
        .collect();
    let mine = victory::totals(b, 0).men_alive as f32;
    let theirs = victory::totals(b, 1).men_alive as f32;
    let flag = flag_of(db, &sim.side_factions[0]);
    BattleHudFacts {
        phase: match sim.phase {
            BattlePhase::Deployment => HudPhase::Deployment,
            BattlePhase::Conflict => HudPhase::Conflict,
            BattlePhase::Finished => HudPhase::Finished,
        },
        elapsed_s: b.time_seconds(),
        total_s: sim.victory.time_limit_s.unwrap_or(0.0),
        speed: sim.speed.multiplier(),
        units,
        naval: false,
        player_won: sim.outcome.is_over().then(|| sim.outcome.winner() == Some(0)),
        results,
        battle_name: hud.battle_name.clone(),
        player_faction: sim.side_factions[0].clone(),
        player_flag: flag,
        balance: if mine + theirs > 0.0 { mine / (mine + theirs) } else { 0.5 },
    }
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
        hud.host.set_screen(size.x, size.y);
    }
    // Model → HUD.
    let facts = facts_of(&mut hud, &sim, &data.db);
    if let Err(e) = ui_battle::set_facts(&hud.host, &facts) {
        warn!("Battle HUD: {e}");
    }
    let _ = ui_battle::update_cards(&hud.host);
    let _ = ui_battle::update_orders(&hud.host);
    hud.clock_ms += f64::from(time.delta_secs()) * 1000.0;
    let t = hud.clock_ms;
    hud.host.pulse(t);
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
                let _ = hud.host.lua().globals().set("is_winner", facts.player_won == Some(true));
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
        let hit = cursor.and_then(|p| hud.host.hit(p.x, p.y));
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
        // The run/walk toggle (land_hud_move_speed.lua): the model's `running` move option.
        "Current_Selection_Walks" | "CurrentSelectionWalks" | "Current_Selection_Runs" | "CurrentSelectionRuns" => {
            sim.battle.units[i].running = name.ends_with("Runs");
            sim.script_events.push(super::ScriptEvent::Command { name: "Change Speed", unit: Some(id), bool1: name.ends_with("Runs") });
            return;
        }
        _ => {}
    }
    let u = &mut sim.battle.units[i];
    match name {
        // PROVISIONAL: 10 m per click and 15° per turn (the original's step sizes are UNKNOWN).
        "Current_Selection_Move_Forwards" | "Current_Selection_Move_Backwards" => {
            let d = if name.ends_with("Forwards") { 10.0 } else { -10.0 };
            u.destination = Some((u.position.0 + u.facing.cos() * d, u.position.1 + u.facing.sin() * d));
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
        spawn_world(&world, root, &mut commands, &mut assets, &mut images, hud.screen, 1.0);
        let mut panels = Vec::new();
        world.visit_visible(root, &mut |_, node| {
            if PANELS.contains(&node.data.id.as_str()) {
                let r = node.rect;
                panels.push(Rect::new(r.x, r.y, r.x + r.w, r.y + r.h));
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
    use super::harness_clicks;

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
}

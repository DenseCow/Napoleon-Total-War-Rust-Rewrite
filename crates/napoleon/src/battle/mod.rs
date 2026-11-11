//! The Phase 5 vertical-slice battle:
//! world → factions → armies → units → movement → combat → morale → AI → battle result.
//!
//! How the parts communicate:
//! ```text
//!  setup.rs ──creates──► BattleSim (resource: the ntw_sim model + display info)
//!                            ▲            │
//!  input.rs ──orders────────┘            │ read every frame
//!  (FixedUpdate, 10 Hz) tick_battle ─────┤
//!                                         ▼
//!                     view.rs (draws units)   hud.rs (text overlay)
//! ```
//! Shooting: after each tick, `tick_battle` copies the model's volley log
//! (`Battle::volleys`) into the display-only [`VolleyFx`] resource, and `view::draw_volleys`
//! draws a short-lived line + flash for each volley. The model never reads `VolleyFx`.
//! Battle effects: `fx` is the deterministic particle world and `fx_draw` the Bevy draw layer
//! (muzzle flashes, smoke, dust, blood, air bursts and ground scorches), both driven from
//! `effects\landbattle.xml` through `fx_draw::tick_fx`. Neither is read by the model.
//! The model ticks at the original's **0.1 s (10 Hz)** (W1 §12.2) in Bevy's `FixedUpdate`
//! schedule. Battle speed {pause, 0.4, 1, 2, 4} (CONFIRMED) is applied by scaling Bevy's
//! *virtual* clock, so a tick's RESULT never depends on speed or frame rate (determinism).

mod actions;
mod camera;
pub(crate) mod flag;
pub(crate) mod fx;
pub(crate) mod fx_draw;
pub(crate) mod fx_probe;
mod hud;
mod input;
mod labels;
mod markers;
mod scripts;
mod sim;
pub(crate) mod setup;
mod skin;
mod view;

use bevy::prelude::*;
#[cfg(test)]
use ntw_sim::battle::model::{Battle, LandUnit};
use ntw_sim::battle::victory;
use ntw_sim::battle::shooting::VolleyEvent;

pub use setup::BattleStart;
pub use sim::BattleSim;
#[cfg(test)]
use sim::next_build;

use crate::GameMode;

/// How long a volley stays visible, in seconds of battle time.
pub const VOLLEY_FX_SECONDS: f32 = 0.6;

/// Display-only list of recent volleys, each with the battle time it was fired at.
#[derive(Resource, Default)]
pub struct VolleyFx {
    /// `(battle time in seconds, volley)`, oldest first.
    pub recent: Vec<(f32, VolleyEvent)>,
}

/// Display-side information about one unit (things the model doesn't need).
pub struct UnitInfo {
    /// Model unit id (`LandUnit::id`).
    pub id: u32,
    /// Readable name from the `units` table, e.g. "German Fusiliers".
    pub name: String,
    /// Drawn width and depth of the unit block, in metres.
    pub size_m: Vec2,
    /// `units` key (for the soldier models).
    pub key: String,
    /// Faction key of the unit's army (uniforms, unit-card folder), e.g. `ita_french_republic`.
    pub faction: String,
    /// True for the human player's units (selectable, take orders).
    pub controllable: bool,
    /// (alliance, army) indices in the battle file (the test slice: (side, 0)).
    pub army: (usize, usize),
    /// The general's name if this is a general's unit.
    pub general: Option<String>,
    /// Experience level (battle file `unit_experience`; 0 for the test slice).
    pub experience: u32,
    /// Category: the battle file's `unit_category`, else the `units` category.
    pub category: String,
    /// The battle file's `script_name` (the battle scripts find units by it).
    pub script_name: Option<String>,
}

impl Default for UnitInfo {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            size_m: Vec2::new(5.0, 2.0),
            key: String::new(),
            faction: String::new(),
            controllable: false,
            army: (0, 0),
            general: None,
            experience: 0,
            category: String::new(),
            script_name: None,
        }
    }
}

/// The stages of a battle (the original's deployment and conflict modes, `BattleUI.IsDeploymentOrConflict`
/// / `HasEnteredDeployment` / `IsConflict`, CONFIRMED binding names), then the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BattlePhase {
    /// Units are placed inside the player's deployment area; battle time does not run.
    #[default]
    Deployment,
    /// The battle runs.
    Conflict,
    /// Decided (victory conditions or time limit); the results are shown.
    Finished,
}


/// A player action a battle script may listen for.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptEvent {
    /// A command: the original's event name ("Move", "Attack Unit", "Fire At Will", "Halt", "Change
    /// Speed", ...), the unit it is about, and its bool.
    Command { name: &'static str, unit: Option<u32>, bool1: bool },
    /// A camera input action (`ntw_script::battle_script::INPUT_EVENT_NAMES`).
    Input(&'static str),
}



/// Registers everything the battle needs.
pub struct BattlePlugin;

impl Plugin for BattlePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "soldier_skin.wgsl");
        bevy::asset::embedded_asset!(app, "soldier_skin_prepass.wgsl");
        bevy::asset::embedded_asset!(app, "particle.wgsl");
        skin::add_fade_upload(app);
        // Test harness: the per-pass times `fps_log` prints when it quits, and frames drawn at
        // full rate while the window is not focused (Bevy's default holds an unfocused window
        // at 60 updates a second, which would cap the measurement).
        if std::env::var_os("NAPOLEON_FPS_LOG").is_some() {
            app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin);
            app.insert_resource(bevy::winit::WinitSettings::continuous());
        }
        app
            // Run `FixedUpdate` 10 times per (virtual) second = one model tick each time.
            .add_plugins(MaterialPlugin::<skin::SkinMaterial>::default())
            .add_plugins(MaterialPlugin::<fx_draw::ParticleMaterial>::default())
            .insert_resource(Time::<Fixed>::from_hz(10.0))
            .init_resource::<VolleyFx>()
            .init_resource::<fx_draw::DrawnFx>()
            .init_resource::<fx_draw::FxMeshes>()
            .init_resource::<view::ScriptCamera>()
            .init_resource::<view::CameraKeys>()
            .add_systems(
                OnEnter(GameMode::Battle),
                (
                    battle_background,
                    setup::start_battle,
                    fx_draw::enter,
                    fx_draw::load_assets,
                    fx_draw::spawn_buckets,
                    scripts::enter,
                    view::spawn_camera,
                    flag::load,
                    hud::enter,
                )
                    .chain(),
            )
            .add_systems(OnExit(GameMode::Battle), (hud::leave, scripts::leave, markers::leave, fx_draw::leave))
            .add_systems(
                Update,
                (hud::harness, hud::keys, hud::frame, hud::redraw).chain().before(input::keyboard).run_if(in_state(GameMode::Battle)),
            )
            .add_systems(FixedUpdate, (tick_battle, fx_draw::tick_fx, scripts::tick, trace_battle, view::observe_ticks).chain().run_if(in_state(GameMode::Battle)))
            .add_systems(
                Update,
                (
                    input::keyboard,
                    input::mouse,
                    view::apply_script_camera,
                    markers::sync_markers,
                    markers::sync_defences,
                    view::spawn_missing_views,
                    flag::spawn_flags,
                    view::sync_views,
                    // After the views: a tracking camera follows its unit's view this frame.
                    view::move_camera,
                    flag::sync_flags,
                    labels::update_labels,
                    view::draw_volleys,
                    fx_draw::draw_fx,
                    view::screenshot_from_env,
                    fps_log,
                )
                    .chain()
                    .run_if(in_state(GameMode::Battle)),
            )
            // The particle log is not in the chain: it only reads the world and writes the log.
            .add_systems(
                Update,
                fx_draw::fx_log.run_if(in_state(GameMode::Battle)),
            );
    }
}

/// Grass-green battlefield background. Bevy colour values run from 0.0 to 1.0.
fn battle_background(mut commands: Commands) {
    // A loaded battle map has its own sky colour (its `.environment`); also when the front end
    // loaded the map at run time.
    let sky = crate::terrain::current_map().and_then(|m| m.lighting.as_ref().and_then(|l| l.sky_colour));
    let colour = sky.map_or(Color::srgb(0.22, 0.32, 0.18), |s| Color::srgb(s[0], s[1], s[2]));
    commands.insert_resource(ClearColor(colour));
}

/// Advances the model by exactly one 0.1 s tick, unless the battle is already decided, then
/// copies that tick's volleys into the display-only [`VolleyFx`].
fn tick_battle(mut sim: ResMut<BattleSim>, mut fx: ResMut<VolleyFx>) {
    // Only the conflict phase runs the model (not deployment; not once decided, unless the player
    // chose to continue). The victory conditions are checked after each tick. The selected
    // deployables were built when deployment ended (`BattleSim::end_deployment`).
    if sim.phase() == BattlePhase::Conflict {
        let fired_at = sim.battle.time_seconds();
        sim.battle.step();
        fx.recent.extend(sim.battle.volleys.iter().map(|v| (fired_at, *v)));
        if !sim.continued {
            let outcome = victory::check(&sim.battle, &sim.victory);
            if outcome.is_over() {
                info!("Battle over at {:.1} s: {outcome:?}", sim.battle.time_seconds());
                sim.finish(outcome);
            }
        }
    }
    // Forget volleys that have faded (or that belong to a battle that was restarted with the test key F8).
    let now = sim.battle.time_seconds();
    fx.recent.retain(|(t, _)| *t <= now && now - *t <= VOLLEY_FX_SECONDS);
}

/// Test harness: with `NAPOLEON_FPS_LOG=<seconds>`, logs the frame rate every 2 s (mean FPS,
/// worst frame) and quits after that many seconds of real time. On quitting it logs, once, the
/// mean CPU and GPU time of every render pass over the last frames (`PASS` lines, from the
/// `RenderDiagnosticsPlugin` that `BattlePlugin` adds under the same variable).
/// `NAPOLEON_FPS_SIZE=<width>x<height>` asks for that window size (physical pixels) for the run;
/// the `PASS` block starts with the size the window really has and the number of entities that
/// carry a mesh. `NAPOLEON_FPS_SHADOWS=0` (`view::spawn_camera`) runs without sun shadows, whose
/// passes Bevy 0.19 does not time. Used for the performance numbers
/// in `analysis/units/ANIM_FORMAT.md` and the frame budget of `docs/DESIGN.md` §3.7.2.
fn fps_log(
    time: Res<Time<Real>>,
    sim: Res<BattleSim>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut state: Local<FpsLog>,
    mut exit: MessageWriter<AppExit>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
    drawn: Query<(), With<Mesh3d>>,
) {
    let Some(limit) = std::env::var("NAPOLEON_FPS_LOG").ok().and_then(|s| s.parse::<f32>().ok()) else { return };
    let FpsLog { window, frames, worst, total, sized, done } = &mut *state;
    if *done {
        return;
    }
    // Measure without vsync, so the numbers are not capped at the display rate.
    for mut w in &mut windows {
        if w.present_mode != bevy::window::PresentMode::AutoNoVsync {
            w.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
        if !*sized {
            match std::env::var("NAPOLEON_FPS_SIZE").ok().map(|s| fps_size(&s).ok_or(s)) {
                Some(Ok((width, height))) => w.resolution.set_physical_resolution(width, height),
                Some(Err(s)) => warn!("NAPOLEON_FPS_SIZE={s:?} is not <width>x<height>; the window keeps its size"),
                None => {}
            }
        }
    }
    *sized = true;
    let dt = time.delta_secs();
    *window += dt;
    *frames += 1;
    *worst = worst.max(dt);
    *total += dt;
    if *window >= 2.0 {
        let men: u32 = sim.battle.units.iter().map(|u| u.men).sum();
        info!(
            "FPS {:.1} (worst frame {:.1} ms) with {men} men, {} units, t = {:.0} s",
            *frames as f32 / *window,
            *worst * 1000.0,
            sim.battle.units.len(),
            *total
        );
        (*window, *frames, *worst) = (0.0, 0, 0.0);
    }
    if *total >= limit {
        for w in &windows {
            info!("PASS window {}x{} physical pixels", w.resolution.physical_width(), w.resolution.physical_height());
        }
        info!("PASS mesh entities {}", drawn.iter().count());
        let mut passes: Vec<(&str, f64, &str)> = diagnostics
            .iter()
            .filter(|d| d.path().as_str().starts_with("render/"))
            .filter_map(|d| Some((d.path().as_str(), d.average()?, &*d.suffix)))
            .collect();
        passes.sort_unstable_by_key(|pass| pass.0);
        for (path, mean, unit) in passes {
            info!("PASS {path} {mean:.3}{unit}");
        }
        *done = true;
        exit.write(AppExit::Success);
    }
}

/// What `fps_log` keeps between frames.
#[derive(Default)]
struct FpsLog {
    /// Seconds in the 2 s window being measured.
    window: f32,
    /// Frames in that window.
    frames: u32,
    /// The longest frame of that window, in seconds.
    worst: f32,
    /// Seconds since the run began.
    total: f32,
    /// The window size was asked for.
    sized: bool,
    /// The pass times were logged and the exit sent.
    done: bool,
}

/// `NAPOLEON_FPS_SIZE`'s value, `<width>x<height>` in physical pixels (both above 0).
fn fps_size(text: &str) -> Option<(u32, u32)> {
    let (width, height) = text.split_once(['x', 'X'])?;
    let size = (width.trim().parse::<u32>().ok()?, height.trim().parse::<u32>().ok()?);
    (size.0 > 0 && size.1 > 0).then_some(size)
}

/// Comparison harness (`docs/COMPARE_WITH_ORIGINAL.md`): with `NAPOLEON_BATTLE_TRACE=<file.csv>`,
/// writes one row per unit every 10 s of battle time: time, side, unit name, men, max men,
/// morale value and behaviour, fatigue, position, and 1 once the unit has left the field. The
/// same numbers can be read off the original's unit cards and tooltips at the same moments.
fn trace_battle(sim: Res<BattleSim>, mut out: Local<Option<std::io::BufWriter<std::fs::File>>>, mut next: Local<f32>) {
    use std::io::Write;
    let Some(path) = std::env::var_os("NAPOLEON_BATTLE_TRACE") else { return };
    if out.is_none() {
        match std::fs::File::create(&path) {
            Ok(f) => {
                let mut w = std::io::BufWriter::new(f);
                let _ = writeln!(w, "seconds,side,unit,men,max_men,morale,behaviour,fatigue,x,y,left");
                *out = Some(w);
            }
            Err(e) => {
                warn!("NAPOLEON_BATTLE_TRACE: {e}");
                return;
            }
        }
    }
    let t = sim.battle.time_seconds();
    if t + 10.0 < *next {
        *next = 0.0; // the battle was restarted (the test key F8)
    }
    if t < *next {
        return;
    }
    *next = (t / 10.0).floor() * 10.0 + 10.0;
    let Some(w) = out.as_mut() else { return };
    for (u, info) in sim.units_with_info() {
        let name = info.name.as_str();
        let _ = writeln!(
            w,
            "{t:.1},{},{name},{},{},{},{:?},{},{:.1},{:.1},{}",
            u.side, u.men, u.max_men, u.morale.morale, u.morale.behaviour, u.fatigue, u.position.0, u.position.1, u.left_field as u8
        );
    }
    let _ = w.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::fatigue::KvFatigue;
    use ntw_sim::battle::morale::KvMorale;

    fn info(id: u32) -> UnitInfo {
        UnitInfo { id, name: format!("unit {id}"), ..UnitInfo::default() }
    }

    /// The harness window size is `<width>x<height>`; anything else is refused, not guessed.
    #[test]
    fn the_harness_window_size_parses() {
        assert_eq!(fps_size("1920x1080"), Some((1920, 1080)));
        assert_eq!(fps_size(" 2560 X 1440 "), Some((2560, 1440)));
        for bad in ["", "1920", "1920x", "0x1080", "1920x-1", "wide x tall"] {
            assert_eq!(fps_size(bad), None, "{bad:?}");
        }
    }

    /// Regression (review of the polish-hotpaths branch): the HUD cards (`hud::refresh_facts`)
    /// and the volley sounds (`audio::battle::bridge_volleys`) paired `battle.units` with `info`
    /// by position, but the model inserts a unit in id order while `info` was appended, so a
    /// reinforcement with a lower id gave every later unit the next unit's info. Both now pair by
    /// id (`units_with_info`, `unit_with_info`), and `add_unit` keeps the two in one order.
    #[test]
    fn a_lower_id_reinforcement_keeps_every_unit_with_its_own_info() {
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        sim.add_unit(LandUnit::new(2, 0, 100, (0.0, 0.0)), info(2));
        sim.add_unit(LandUnit::new(3, 1, 100, (0.0, 900.0)), info(3));
        // The reinforcement has the lowest id, so it lands in front of both.
        sim.add_unit(LandUnit::new(1, 0, 100, (0.0, -50.0)), info(1));
        let pairs: Vec<(u32, u32)> = sim.units_with_info().map(|(u, i)| (u.id, i.id)).collect();
        assert_eq!(pairs, [(1, 1), (2, 2), (3, 3)], "HUD cards");
        for id in 1..=3 {
            let (u, i) = sim.unit_with_info(id).unwrap();
            assert_eq!((u.id, i.id), (id, id), "volley sound of unit {id}");
        }
        assert!(sim.unit_with_info(9).is_none());
        // `add_unit` keeps `info` in the model's order, so the slot lookups stay O(1).
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        for id in [2, 4, 1, 3] {
            sim.add_unit(LandUnit::new(id, 0, 100, (0.0, 0.0)), info(id));
        }
        assert_eq!(sim.battle.units.iter().map(|u| u.id).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(sim.infos().iter().map(|i| i.id).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(sim.info_of(3).map(|i| i.id), Some(3));
        assert!(sim.info_of(9).is_none() && sim.unit(9).is_none());
    }

    /// A unit put into the model without its info is caught at the next `add_unit`.
    #[test]
    #[should_panic(expected = "without its info")]
    fn a_bare_model_add_unit_is_caught() {
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        sim.battle.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        sim.add_unit(LandUnit::new(2, 0, 100, (0.0, 0.0)), info(2));
    }

    /// Regression (abilitygate review): the deployment phase was kept twice, `BattleSim::phase` and
    /// the model's `Battle::deploying`, set and cleared at different times. The phase now reads
    /// the model's flag, so the two cannot disagree.
    #[test]
    fn the_deployment_phase_is_the_models_own_flag() {
        use ntw_sim::battle::victory::Outcome;
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        sim.battle.deploying = true;
        assert_eq!(sim.phase(), BattlePhase::Deployment);
        sim.end_deployment();
        assert!(!sim.battle.deploying);
        assert_eq!(sim.phase(), BattlePhase::Conflict);
        sim.finish(Outcome::Won { side: 0 });
        assert_eq!((sim.phase(), sim.outcome), (BattlePhase::Finished, Outcome::Won { side: 0 }));
        sim.continue_battle();
        assert_eq!(sim.phase(), BattlePhase::Conflict);
        assert!(sim.continued);
        // A battle a harness decides during deployment shows its result; continuing ends the deployment.
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        sim.battle.deploying = true;
        sim.finish(Outcome::Won { side: 1 });
        assert_eq!(sim.phase(), BattlePhase::Finished);
        sim.continue_battle();
        assert_eq!(sim.phase(), BattlePhase::Conflict);
        assert!(!sim.battle.deploying);
    }
}

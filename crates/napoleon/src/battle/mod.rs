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
pub(crate) mod flag;
pub(crate) mod fx;
pub(crate) mod fx_draw;
pub(crate) mod fx_probe;
mod hud;
mod input;
mod labels;
mod markers;
mod scripts;
pub(crate) mod setup;
mod skin;
mod view;

use bevy::prelude::*;
use ntw_sim::battle::model::{Battle, LandUnit};
use ntw_sim::battle::victory::{self, Outcome, VictoryRules};
use ntw_sim::battle::shooting::VolleyEvent;
use ntw_sim::battle::speed::BattleSpeed;

pub use setup::BattleStart;

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
    /// Ranks in the drawn formation (`unit_stats_land` #43, or from the battle file's frontage;
    /// INFERRED, see setup).
    pub ranks: u32,
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
            ranks: 1,
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

/// The battle, as a Bevy resource: the authoritative `ntw_sim` model plus display info.
#[derive(Resource)]
pub struct BattleSim {
    /// The model. Only `tick_battle` advances it; `input` only sets orders.
    pub battle: Battle,
    /// Current battle speed (CONFIRMED set and cycle order).
    pub speed: BattleSpeed,
    /// Speed to return to when un-pausing.
    pub speed_before_pause: BattleSpeed,
    /// Display info for every unit, in the same order as `battle.units` (kept by [`Self::add_unit`]).
    pub info: Vec<UnitInfo>,
    /// Names of the two sides, e.g. ["France", "Austria"].
    pub side_names: [String; 2],
    /// RNG seed this battle started from (shown in the HUD; R restarts with seed + 1).
    pub seed: u32,
    /// The currently selected player (side 0) unit id, if any.
    pub selected: Option<u32>,
    /// Faction key of each side's first army (e.g. `france`), for uniforms and music.
    pub side_factions: [String; 2],
    /// The `battles` key of the historical battle being fought (None = the test slice).
    pub battle_key: Option<String>,
    /// The player's army's `camera_start_position` and `camera_target_position` from the battle
    /// file, as (x, height, y) map coordinates.
    pub camera_start: Option<([f32; 3], [f32; 3])>,
    /// The player's deployment area (map metres), if the battle has one.
    pub deployment_area: Option<ntw_formats::battle_terrain::DeploymentArea>,
    /// Deployment, conflict or finished.
    pub phase: BattlePhase,
    /// The battle file's time limit and timeout winner (none for the test slice).
    pub victory: VictoryRules,
    /// How the battle ended (Ongoing until then).
    pub outcome: Outcome,
    /// The player chose to keep fighting after the result.
    pub continued: bool,
    /// `battle_description/battle_script` of the battle file (e.g. `Arcole_Battle`).
    pub battle_script: Option<String>,
    /// The player's commands and camera inputs since the last script tick, for the battle script's
    /// command and input handlers (see `scripts`).
    pub script_events: Vec<ScriptEvent>,
    /// The battle file's `weather/prevailing_wind` as a world-space wind velocity in m/s
    /// (battle `(x, y)` -> world `(x, 0, -y)`). Used by the flag cloth (`flag`). **INFERRED**: the
    /// shipped vectors are `0`, `(0, 5)`, `(0, 9)` and `(10, 0)`, which read as metres per second,
    /// but the exe normalises the vector separately for the wind audio levels, so its own unit is
    /// not the metre. PROVISIONAL target: the exe-side wind speed the verlet item is fed.
    pub wind: [f32; 3],
    /// A number no other `BattleSim` of this run has: a restart (R) or a new battle gets a new one,
    /// even with the same unit ids, so the views of one battle are never reused for another.
    pub build: u32,
}

/// The next [`BattleSim::build`] number.
fn next_build() -> u32 {
    static BUILDS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

impl BattleSim {
    /// A battle with no units yet, at normal speed.
    pub fn new(battle: Battle, seed: u32) -> Self {
        Self {
            battle,
            speed: BattleSpeed::Normal,
            speed_before_pause: BattleSpeed::Normal,
            info: Vec::new(),
            side_names: [String::new(), String::new()],
            seed,
            selected: None,
            side_factions: [String::new(), String::new()],
            battle_key: None,
            camera_start: None,
            deployment_area: None,
            phase: BattlePhase::Conflict,
            victory: VictoryRules::default(),
            outcome: Outcome::Ongoing,
            continued: false,
            battle_script: None,
            script_events: Vec::new(),
            wind: [0.0; 3],
            build: next_build(),
        }
    }

    /// Display info of a unit by id.
    pub fn info_of(&self, id: u32) -> Option<&UnitInfo> {
        self.info.iter().find(|i| i.id == id)
    }

    /// Adds a unit and its display info: `info` goes to the index the model's
    /// [`Battle::add_unit`] gives the unit (sorted by id), so `info` keeps `battle.units`' order
    /// whatever order units are added in (a reinforcement with a lower id included).
    pub fn add_unit(&mut self, unit: LandUnit, info: UnitInfo) {
        debug_assert_eq!(unit.id, info.id);
        let at = self.battle.units.partition_point(|u| u.id < unit.id).min(self.info.len());
        self.battle.add_unit(unit);
        self.info.insert(at, info);
    }

    /// Display info of unit `id`, expected at `slot` (its index in `battle.units`; [`add_unit`]
    /// keeps `info` in that order): O(1) while that slot holds it, else found by id.
    ///
    /// [`add_unit`]: Self::add_unit
    pub fn info_at(&self, slot: usize, id: u32) -> Option<&UnitInfo> {
        self.info.get(slot).filter(|i| i.id == id).or_else(|| self.info_of(id))
    }

    /// Every model unit with its display info, paired by id (a unit with no info is skipped).
    pub fn units_with_info(&self) -> impl Iterator<Item = (&LandUnit, &UnitInfo)> {
        self.battle.units.iter().enumerate().filter_map(|(i, u)| Some((u, self.info_at(i, u.id)?)))
    }

    /// Model unit `id` with its display info.
    pub fn unit_with_info(&self, id: u32) -> Option<(&LandUnit, &UnitInfo)> {
        let i = self.battle.units.iter().position(|u| u.id == id)?;
        Some((&self.battle.units[i], self.info_at(i, id)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::fatigue::KvFatigue;
    use ntw_sim::battle::morale::KvMorale;

    fn info(id: u32) -> UnitInfo {
        UnitInfo { id, name: format!("unit {id}"), ..UnitInfo::default() }
    }

    /// Regression (review of the polish-hotpaths branch): the HUD cards (`hud::refresh_facts`)
    /// and the volley sounds (`audio::battle::bridge_volleys`) paired `battle.units` with `info`
    /// by position, but the model inserts a unit in id order while `info` was appended, so a
    /// reinforcement with a lower id gave every later unit the next unit's info. Both now pair by
    /// id (`units_with_info`, `unit_with_info`), and `add_unit` keeps the two in one order.
    #[test]
    fn a_lower_id_reinforcement_keeps_every_unit_with_its_own_info() {
        let mut battle = Battle::new(1, KvMorale::default(), KvFatigue::default());
        battle.add_unit(LandUnit::new(2, 0, 100, (0.0, 0.0)));
        battle.add_unit(LandUnit::new(3, 1, 100, (0.0, 900.0)));
        let mut sim = BattleSim::new(battle, 1);
        sim.info = vec![info(2), info(3)];
        // Appended out of the model's order (as a bare `info.push` would).
        sim.battle.add_unit(LandUnit::new(1, 0, 100, (0.0, -50.0)));
        sim.info.push(info(1));
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
        assert_eq!(sim.info.iter().map(|i| i.id).collect::<Vec<_>>(), [1, 2, 3, 4]);
    }
}

/// Registers everything the battle needs.
pub struct BattlePlugin;

impl Plugin for BattlePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "soldier_skin.wgsl");
        bevy::asset::embedded_asset!(app, "soldier_skin_prepass.wgsl");
        bevy::asset::embedded_asset!(app, "particle.wgsl");
        skin::add_fade_upload(app);
        app
            // Run `FixedUpdate` 10 times per (virtual) second = one model tick each time.
            .add_plugins(MaterialPlugin::<skin::SkinMaterial>::default())
            .add_plugins(MaterialPlugin::<fx_draw::ParticleMaterial>::default())
            .insert_resource(Time::<Fixed>::from_hz(10.0))
            .init_resource::<VolleyFx>()
            .init_resource::<fx_draw::DrawnFx>()
            .init_resource::<fx_draw::FxMeshes>()
            .init_resource::<view::ScriptCamera>()
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
                (hud::harness, hud::frame, hud::redraw).chain().before(input::keyboard).run_if(in_state(GameMode::Battle)),
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
                    view::move_camera,
                    view::spawn_missing_views,
                    flag::spawn_flags,
                    view::sync_views,
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
    // chose to continue). The victory conditions are checked after each tick.
    if sim.phase == BattlePhase::Conflict {
        if sim.battle.tick == 0 {
            // Deployment is over (0x00551BA0): the selected deployable defences are built.
            sim.battle.end_deployment();
            if !sim.battle.defences.is_empty() {
                info!("Battle: {} deployable defences built", sim.battle.defences.len());
            }
        }
        let fired_at = sim.battle.time_seconds();
        sim.battle.step();
        fx.recent.extend(sim.battle.volleys.iter().map(|v| (fired_at, *v)));
        if !sim.continued {
            let outcome = victory::check(&sim.battle, &sim.victory);
            if outcome.is_over() {
                info!("Battle over at {:.1} s: {outcome:?}", sim.battle.time_seconds());
                sim.outcome = outcome;
                sim.phase = BattlePhase::Finished;
            }
        }
    }
    // Forget volleys that have faded (or that belong to a battle that was restarted with R).
    let now = sim.battle.time_seconds();
    fx.recent.retain(|(t, _)| *t <= now && now - *t <= VOLLEY_FX_SECONDS);
}

/// Test harness: with `NAPOLEON_FPS_LOG=<seconds>`, logs the frame rate every 2 s (mean FPS,
/// worst frame) and quits after that many seconds of real time. Used for the performance
/// numbers in `analysis/units/ANIM_FORMAT.md`.
fn fps_log(
    time: Res<Time<Real>>,
    sim: Res<BattleSim>,
    mut acc: Local<(f32, u32, f32, f32)>,
    mut exit: MessageWriter<AppExit>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(limit) = std::env::var("NAPOLEON_FPS_LOG").ok().and_then(|s| s.parse::<f32>().ok()) else { return };
    // Measure without vsync, so the numbers are not capped at the display rate.
    for mut w in &mut windows {
        if w.present_mode != bevy::window::PresentMode::AutoNoVsync {
            w.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
    }
    let dt = time.delta_secs();
    let (window, frames, worst, total) = &mut *acc;
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
        exit.write(AppExit::Success);
    }
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
        *next = 0.0; // the battle was restarted (R)
    }
    if t < *next {
        return;
    }
    *next = (t / 10.0).floor() * 10.0 + 10.0;
    let Some(w) = out.as_mut() else { return };
    for (i, u) in sim.battle.units.iter().enumerate() {
        let name = sim.info.get(i).map_or("?", |n| n.name.as_str());
        let _ = writeln!(
            w,
            "{t:.1},{},{name},{},{},{},{:?},{},{:.1},{:.1},{}",
            u.side, u.men, u.max_men, u.morale.morale, u.morale.behaviour, u.fatigue, u.position.0, u.position.1, u.left_field as u8
        );
    }
    let _ = w.flush();
}

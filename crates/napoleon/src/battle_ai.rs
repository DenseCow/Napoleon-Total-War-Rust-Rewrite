//! The battle AI hook: `ntw_ai` controls the non-player army (side 1) in the battle.
//!
//! `--ai on|off` (default on). With `off`, side 1 uses the model's own PLACEHOLDER default
//! behaviour (advance on the nearest enemy and fire at will).
//!
//! The AI runs in `FixedPreUpdate`, i.e. right before each 0.1 s model tick in `FixedUpdate`,
//! and only gives orders through the model's public order functions (deterministic).

use bevy::prelude::*;
use ntw_ai::battle::BattleAi;

use crate::GameMode;
use crate::battle::BattleSim;

/// The side the AI plays (the player is side 0 in the battle slice).
const AI_SIDE: u8 = 1;

/// Whether the AI is on, from `--ai on|off`.
#[derive(Resource, Clone, Copy)]
pub struct AiSettings {
    /// True: the AI controls side 1.
    pub enabled: bool,
}

/// The AI state for the current battle, plus which battle it belongs to.
#[derive(Resource)]
struct AiState {
    ai: BattleAi,
    seed: u32,
    last_tick: u32,
}

/// Registers the AI system.
pub struct BattleAiPlugin {
    enabled: bool,
}

impl BattleAiPlugin {
    /// Reads `--ai on|off` (default on).
    pub fn from_args(args: &[String]) -> Self {
        let off = args
            .iter()
            .position(|a| a == "--ai")
            .and_then(|i| args.get(i + 1))
            .is_some_and(|v| v.eq_ignore_ascii_case("off"));
        BattleAiPlugin { enabled: !off }
    }
}

impl Plugin for BattleAiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AiSettings { enabled: self.enabled })
            .add_systems(FixedPreUpdate, run_ai.run_if(in_state(GameMode::Battle)))
            .add_systems(PostUpdate, ai_harness.run_if(in_state(GameMode::Battle)));
    }
}

/// Gives the AI side its orders for the coming tick. A new AI is made when the battle is
/// restarted (new seed, or the tick counter went back).
fn run_ai(
    mut commands: Commands,
    settings: Res<AiSettings>,
    sim: Option<ResMut<BattleSim>>,
    state: Option<ResMut<AiState>>,
) {
    if !settings.enabled {
        return;
    }
    let Some(mut sim) = sim else { return };
    // The AI gives orders only while the battle runs (not during deployment, not once decided).
    if sim.phase != crate::battle::BattlePhase::Conflict {
        return;
    }
    let tick = sim.battle.tick;
    let seed = sim.seed;
    match state {
        Some(mut s) if s.seed == seed && s.last_tick <= tick => {
            s.ai.update(&mut sim.battle);
            s.last_tick = tick;
        }
        _ => {
            let mut ai = BattleAi::new(&[AI_SIDE]);
            ai.update(&mut sim.battle);
            commands.insert_resource(AiState { ai, seed, last_tick: tick });
        }
    }
}

/// Test harness for checking the AI without playing:
/// - `NAPOLEON_AI_SPEED=<x>` runs the battle clock x times faster (Bevy caps a frame at 0.25 s);
/// - `NAPOLEON_AI_SHOT=<file.png>@<seconds>` saves a screenshot at that battle time and quits;
/// - `NAPOLEON_AI_TRACE=<seconds>` logs, every that many battle seconds, each side's centre,
///   the closest enemy distance, units firing / in melee / running, men left and the AI's state.
#[allow(clippy::too_many_arguments)] // Bevy system parameters
fn ai_harness(
    sim: Option<Res<BattleSim>>,
    state: Option<Res<AiState>>,
    mut time: ResMut<Time<Virtual>>,
    mut last: Local<Option<f32>>,
    mut shot: Local<Option<f32>>,
    real: Res<Time<Real>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(sim) = sim else { return };
    // Runs after the battle input system (Update) has set the clock speed for this frame.
    if let Some(x) = std::env::var("NAPOLEON_AI_SPEED").ok().and_then(|s| s.parse::<f32>().ok()) {
        time.set_relative_speed(x.max(0.1));
    }
    // `NAPOLEON_AI_SHOT=<file.png>@<battle seconds>`: one screenshot at that battle time, then quit.
    if let Ok(spec) = std::env::var("NAPOLEON_AI_SHOT")
        && let Some((file, secs)) = spec.rsplit_once('@')
        && let Ok(secs) = secs.parse::<f32>()
    {
        match *shot {
            None if sim.battle.time_seconds() >= secs => {
                commands
                    .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                    .observe(bevy::render::view::screenshot::save_to_disk(std::path::PathBuf::from(file)));
                *shot = Some(real.elapsed_secs());
            }
            Some(t) if real.elapsed_secs() > t + 1.5 => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
    let Some(every) = std::env::var("NAPOLEON_AI_TRACE").ok().and_then(|s| s.parse::<f32>().ok()) else { return };
    let t = sim.battle.time_seconds();
    if last.is_some_and(|l| t < l + every && t >= l) {
        return;
    }
    *last = Some(t);
    let b = &sim.battle;
    let mut parts = Vec::new();
    for side in 0..2u8 {
        let us: Vec<_> = b.units.iter().filter(|u| u.side == side && u.men > 0).collect();
        if us.is_empty() {
            parts.push(format!("side {side}: none"));
            continue;
        }
        let n = us.len() as f32;
        let c = us.iter().fold((0.0, 0.0), |a, u| (a.0 + u.position.0 / n, a.1 + u.position.1 / n));
        let closest = us
            .iter()
            .flat_map(|u| b.units.iter().filter(move |e| e.side != side && e.men > 0).map(move |e| ((u.position.0 - e.position.0).powi(2) + (u.position.1 - e.position.1).powi(2)).sqrt()))
            .fold(f32::INFINITY, f32::min);
        let volleys: u32 = us.iter().map(|u| u.volleys_fired).sum();
        parts.push(format!(
            "side {side}: centre ({:.0}, {:.0}) closest enemy {closest:.0} m, men {}, volleys {volleys}, melee {}, running {}",
            c.0,
            c.1,
            us.iter().map(|u| u.men).sum::<u32>(),
            us.iter().filter(|u| u.in_melee).count(),
            us.iter().filter(|u| u.running).count(),
        ));
    }
    let ai = state.and_then(|s| s.ai.alliance(AI_SIDE).map(|a| a.status())).unwrap_or_default();
    info!("AI trace t={t:.0}s: {} | {ai}", parts.join(" | "));
}

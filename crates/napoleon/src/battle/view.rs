//! Drawing, in 3D: every model unit is a block of real soldier figures (assembled from the
//! install by `crate::soldiers`), an outline on the ground in its side's colour, and a
//! screen-space text label. Volleys are drawn as lines.
//!
//! These Bevy entities are only a *picture* of the model. They are read from `BattleSim` every
//! frame and never feed back into the rules (the original's model/display split).
//!
//! Battlefield metres map to Bevy world units 1:1: model `(x, y)` -> world `(x, 0, -y)`.
//! If the install's soldier data cannot be read (e.g. running on the test fixture), units are
//! drawn as outlines only.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::transform::commands::BuildChildrenTransformExt;
use bevy::render::storage::ShaderBuffer;
use ntw_formats::unit_model::{VariantRole, unit_lod};
use ntw_sim::battle::model::LandUnit;
use ntw_sim::battle::morale::MoraleState;

use super::skin::{self, BoneAtlas, SkinExt, SkinMaterial};
use super::{BattleSim, VOLLEY_FX_SECONDS, VolleyFx};
use crate::data::GameData;
use crate::soldiers::{FigureKit, KitAssets, SoldierLibrary, animation_keys};
use ntw_formats::unit_animation::{self, DeathCause, Gait, SelectionRng, alternative, family_pick, pick_level};

use super::actions::{self, Mode};

/// Different men per unit (PROVISIONAL: the game varies parts per man; we build this many
/// kits per unit and share them round-robin; the men of a kit share its meshes and are drawn
/// instanced). Clips do not depend on the kit: each man plays his own alternative clips,
/// picked by his selection number, with his own phase (see `skin`).
const KITS_PER_UNIT: usize = 3;
/// Most figures drawn per unit.
const MAX_FIGURES: usize = 240;
/// The figure generators start from the battle's own seed, as the original's battle generator
/// does (CONFIRMED source: the battle set-up `0x00511210` gets `timeGetTime()`, or the
/// `constant_random_seed` option, from `0x004847F0`; our battles use their deterministic seed). Which
/// draws come before the men are made is UNKNOWN, so the state stays PROVISIONAL. The actions
/// generator is a second stream (ours) so deaths do not shift the men's selection numbers.
fn selection_seed(battle_seed: u32) -> u32 {
    battle_seed
}

/// Marks the entity (parent of the figures) that shows model unit `id`.
#[derive(Component)]
pub struct UnitView {
    pub id: u32,
}

/// Marks the text label entity for model unit `id`.
#[derive(Component)]
pub struct UnitLabel {
    pub id: u32,
    /// The text last drawn (redrawn only when it changes).
    pub text: String,
}

/// The soldier figures of one unit view.
#[derive(Component)]
pub struct UnitFigures {
    kits: Vec<FigureKit>,
    men: Vec<Entity>,
    /// Figure slot of man 0 in [`SkinState::figures`]; man `i` uses slots `2 (first + i)` (man)
    /// and `2 (first + i) + 1` (mount).
    first_slot: usize,
    /// Facing at the last frame, and whether the men were placed on the terrain yet.
    last_facing: f32,
    seated: bool,
    /// The unit's ground speed, which picks the gait and the speed level; its `pos` is the
    /// model position at the last frame.
    ground: GroundSpeed,
    /// Each man's selection number: which alternative clip of every slot he plays
    /// (`unit_animation::alternative`).
    selections: Vec<u32>,
    /// The unit's animation clock (s), advanced by the frame time × the playback rate of
    /// the speed level its men play.
    clock: f32,
    /// Per-man action state (fire, reload, melee, death; see `actions`).
    acts: Vec<actions::ManAct>,
    /// Model state at the last frame, to see events: men alive, volleys fired, in melee.
    last_men: u32,
    last_volleys: u32,
    last_melee: bool,
    /// The kit's `COMBAT_IDLE_n` / `ATTACK_n` slots (with `RIDER_` for riders).
    melee_idle: Vec<String>,
    melee_attack: Vec<String>,
    /// Each man's current part LOD (`unit_model::unit_lod`).
    lods: Vec<u8>,
    /// The unit's standard bearer, when `unit_stats_land` names one. He is not one of `unit.men`:
    /// he does not fire, and he does not die with the strength count.
    bearer: Option<Bearer>,
}

/// Seconds without moving before a unit's men stop walking and stand. PROVISIONAL (ours): the
/// exe's stand rule is not traced.
const STILL_SECONDS: f32 = 0.3;

/// A unit's ground speed, which picks its gait and speed level (`unit_animation::pick_level`).
///
/// The model moves a unit only on its 0.1 s ticks, so the speed is the distance moved over the
/// model ticks since the last observation, not over the frame: the frames between two ticks see no
/// movement, and a per-frame estimate swings every tick (the walking jitter of 2026-10-04..07:
/// the men switched speed level and playback rate several times a second). The exe picks a
/// speed-matched clip from the entity's own speed (`+0x14C`; `0x006611E0`, `0x005B7B20`,
/// UNITS_TERRAIN_FIDELITY.md §1.4), which is steady while it marches; the distance per tick is the
/// model's equivalent. Between ticks, and on a tick without movement, the last speed is kept.
#[derive(Debug, Clone, Copy)]
struct GroundSpeed {
    /// The model position and tick last seen.
    pos: Vec2,
    tick: u32,
    /// m/s over the last model tick(s) in which the unit moved.
    speed: f32,
    /// Seconds since the last model tick in which the unit moved.
    still_for: f32,
}

impl GroundSpeed {
    fn new(pos: Vec2, tick: u32) -> Self {
        GroundSpeed { pos, tick, speed: 0.0, still_for: 1.0 }
    }

    /// Once per frame, with the unit's model position and the model's tick counter.
    fn observe(&mut self, pos: Vec2, tick: u32, dt: f32) {
        let Some(ticks) = tick.checked_sub(self.tick) else {
            // The tick went back: the battle was restarted (R) under the same views. Start over,
            // standing.
            *self = GroundSpeed::new(pos, tick);
            return;
        };
        let moved = pos.distance(self.pos);
        if ticks > 0 && moved > 1e-4 {
            self.speed = moved / (ticks as f32 * ntw_sim::battle::TICK_SECONDS);
            self.still_for = 0.0;
        } else {
            // No tick, or a tick without movement. A move with no tick (a deployment drag) is
            // not walking.
            self.still_for += dt;
        }
        self.pos = pos;
        self.tick = tick;
    }

    /// The gait: stand once still for [`STILL_SECONDS`], run above the walk/run midpoint
    /// (PROVISIONAL, ours: the exe's walk/run choice is not traced).
    fn gait(&self, walk_speed: f32, run_speed: f32) -> Gait {
        if self.still_for >= STILL_SECONDS {
            Gait::Stand
        } else if self.speed > (walk_speed + run_speed) * 0.5 {
            Gait::Run
        } else {
            Gait::Walk
        }
    }
}

/// A fallen man: detached from his unit so he stays where he fell (see `actions`).
#[derive(Component)]
pub struct Corpse;

/// The standard bearer of a unit that has one: his own kit, because his variant
/// (`*.standard_bearer.unit_variant`) and his skeleton (`Animations/MEN/FLAG_BEARER/...`) are not
/// his unit's, and the entity he is drawn on.
#[derive(Clone)]
pub struct Bearer {
    /// His figure, drawn like any other man.
    pub entity: Entity,
    /// His kit. Kept whole so the view picks his clip per frame exactly as it does for a man.
    pub kit: FigureKit,
}

/// The bearer's pole bone, in the bearer's own model space, for the frame the view posed him on.
/// The flag cloth is placed from it, so the cloth and the body always agree on the frame.
#[derive(Component)]
pub struct PoleBone {
    /// Column-major 4x4, exactly as `ntw_formats::anim::world_matrices` gives it. No Bevy mirror:
    /// `flag::sync_flags` applies that.
    pub model: [f32; 16],
}

/// The battle's generator for random clip families (deaths, knock-downs; `actions`).
#[derive(Resource)]
pub struct ActionRng(pub SelectionRng);

/// The soldier data, if the install has it.
#[derive(Resource)]
pub struct BattleSoldiers {
    lib: Option<SoldierLibrary>,
}

/// GPU skinning state of the current battle's figures (see `skin`).
#[derive(Resource)]
pub struct SkinState {
    atlas: BoneAtlas,
    figures: Handle<ShaderBuffer>,
    /// One entry per figure slot, rewritten every frame.
    data: Vec<[u32; 4]>,
}

/// Orbit-style battle camera: looks at `focus` from `distance`, `yaw` around the vertical.
#[derive(Component)]
pub struct BattleCamera {
    focus: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

/// Model battlefield metres -> Bevy world position on the ground (the battle map's terrain
/// height, see `crate::terrain`; y = 0 when no map is loaded).
pub fn world_of(p: Vec2) -> Vec3 {
    crate::terrain::ground_point(p)
}

/// Startup system: 3D camera, sun, ground, and the soldier library.
pub fn spawn_camera(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sim: Option<Res<BattleSim>>,
) {
    let mut cam = BattleCamera { focus: Vec3::new(30.0, 0.0, 90.0), distance: 170.0, yaw: 0.0, pitch: 0.4 };
    // Start behind the player's army (side 0), looking the way it faces.
    if let Some(sim) = sim.as_ref() {
        let mine: Vec<&LandUnit> = sim.battle.units.iter().filter(|u| u.side == 0).collect();
        if let Some(first) = mine.first() {
            let n = mine.len() as f32;
            let centre = mine.iter().fold(Vec2::ZERO, |a, u| a + Vec2::new(u.position.0, u.position.1)) / n;
            let facing = Vec2::new(first.facing.cos(), first.facing.sin());
            let focus = centre + facing * 60.0;
            cam.focus = Vec3::new(focus.x, 0.0, -focus.y);
            cam.yaw = (-first.facing.cos()).atan2(first.facing.sin());
            // Test harness: `NAPOLEON_BATTLE_ZOOM=<metres>[,<k>]` looks at the player's k-th
            // unit (default the first) from close by.
            let zoom = std::env::var("NAPOLEON_BATTLE_ZOOM").unwrap_or_default();
            let mut it = zoom.split(',').map(|s| s.trim().parse::<f32>().ok());
            if let Some(Some(d)) = it.next() {
                let k = it.next().flatten().unwrap_or(0.0) as usize;
                let u = mine.get(k).copied().unwrap_or(*first);
                cam.focus = Vec3::new(u.position.0, 0.0, -u.position.1);
                (cam.distance, cam.pitch) = (d, 0.2);
            }
        }
    }
    // A battle file's own start view: camera_start/target_position are (x, height, map y)
    // (INFERRED). Heights are taken as given (the camera floor keeps it above the ground).
    if let Some((s, t)) = sim.as_ref().and_then(|s| s.camera_start).filter(|_| std::env::var_os("NAPOLEON_BATTLE_ZOOM").is_none()) {
        let (start, target) = (Vec3::new(s[0], s[1], -s[2]), Vec3::new(t[0], t[1], -t[2]));
        let off = start - target;
        let d = off.length().max(1.0);
        cam = BattleCamera { focus: target, distance: d, yaw: off.x.atan2(off.z), pitch: (off.y / d).clamp(-1.0, 1.0).asin().max(0.05) };
    }
    // Test harness: `NAPOLEON_BATTLE_CAMERA=x,z,distance,yaw,pitch` overrides the start view.
    if let Ok(s) = std::env::var("NAPOLEON_BATTLE_CAMERA") {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, z, d, yaw, pitch] = v[..] {
            cam = BattleCamera { focus: Vec3::new(x, 0.0, z), distance: d, yaw, pitch };
        }
    }
    commands.spawn((Camera3d::default(), Transform::default(), cam));
    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-60.0, 120.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y),
        // PROVISIONAL: 2 shadow cascades out to 150 m (Bevy's default is 4); every cascade
        // re-draws every skinned figure, so this halves the cost of soldier shadows.
        bevy::light::CascadeShadowConfigBuilder { num_cascades: 2, maximum_distance: 150.0, ..default() }.build(),
    ));
    commands.insert_resource(GlobalAmbientLight { brightness: 500.0, ..default() });
    // Flat ground only when no battle map loaded (`crate::terrain` draws the real terrain).
    if !crate::terrain::loaded() {
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(2_000.0, 2_000.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.30, 0.40, 0.22),
                perceptual_roughness: 1.0,
                ..default()
            })),
        ));
    }
    let lib = match SoldierLibrary::open(&crate::config::game_data_dir()) {
        Ok(lib) => Some(lib),
        Err(e) => {
            warn!("Soldier models unavailable ({e}); drawing unit outlines only");
            None
        }
    };
    commands.insert_resource(super::labels::LabelFont::load(lib.as_ref().map(|l| &l.vfs)));
    commands.insert_resource(BattleSoldiers { lib });
}

/// Battle harness runs (a fixed camera, a screenshot, an AI shot, or `--screenshot`): the live keyboard
/// and mouse must not move the camera or give orders (`input.rs`), so captures can be repeated.
pub fn camera_harness_run() -> bool {
    static RUN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RUN.get_or_init(|| {
        ["NAPOLEON_BATTLE_CAMERA", "NAPOLEON_BATTLE_ZOOM", "NAPOLEON_BATTLE_SCREENSHOT", "NAPOLEON_AI_SHOT"].iter().any(|v| std::env::var_os(v).is_some())
            || std::env::args().any(|a| a == "--screenshot")
    })
}

/// Arrow keys pan, mouse wheel zooms, Q / E turn the camera (not in harness runs, see
/// [`camera_harness_run`]).
/// A camera position requested by a battle script (`camera:move_to`): (camera position, look-at
/// target) in engine coordinates (x, height, map y). Applied at once (PROVISIONAL: the script's
/// move time is not used).
#[derive(Resource, Default)]
pub struct ScriptCamera(pub Option<([f32; 3], [f32; 3])>);

/// Applies a pending [`ScriptCamera`] to the battle camera (same conversion as a battle file's
/// start view).
pub fn apply_script_camera(mut pending: ResMut<ScriptCamera>, mut cams: Query<&mut BattleCamera>) {
    let Some((s, t)) = pending.0.take() else { return };
    let (start, target) = (Vec3::new(s[0], s[1], -s[2]), Vec3::new(t[0], t[1], -t[2]));
    let off = start - target;
    let d = off.length().max(1.0);
    for mut cam in &mut cams {
        *cam = BattleCamera { focus: target, distance: d, yaw: off.x.atan2(off.z), pitch: (off.y / d).clamp(-1.0, 1.0).asin().max(0.05) };
    }
}

pub fn move_camera(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time<Real>>,
    mut cams: Query<(&mut BattleCamera, &mut Transform)>,
    sim: Option<ResMut<BattleSim>>,
) {
    let fixed = camera_harness_run();
    // The camera actions a battle script's input handler hears (the original's action names; keys:
    // arrows or W/A/S/D move, Q/E rotate, X/Z up/down, INFERRED from the tutorial's texts).
    if let Some(mut sim) = sim
        && !fixed
    {
        for (k, alt, name) in [
            (KeyCode::ArrowUp, KeyCode::KeyW, "move forward"),
            (KeyCode::ArrowDown, KeyCode::KeyS, "move backward"),
            (KeyCode::ArrowLeft, KeyCode::KeyA, "move left"),
            (KeyCode::ArrowRight, KeyCode::KeyD, "move right"),
            (KeyCode::KeyQ, KeyCode::KeyQ, "rotate left"),
            (KeyCode::KeyE, KeyCode::KeyE, "rotate right"),
            (KeyCode::KeyX, KeyCode::KeyX, "move up"),
            (KeyCode::KeyZ, KeyCode::KeyZ, "move down"),
        ] {
            if keys.just_pressed(k) || keys.just_pressed(alt) {
                sim.script_events.push(super::ScriptEvent::Input(name));
            }
        }
    }
    let dt = if fixed { 0.0 } else { time.delta_secs() };
    for (mut cam, mut tf) in &mut cams {
        let fwd = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
        let right = Vec3::new(cam.yaw.cos(), 0.0, -cam.yaw.sin());
        let speed = cam.distance * 0.8 * dt;
        let mut pan = Vec3::ZERO;
        if keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
            pan += fwd;
        }
        if keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
            pan -= fwd;
        }
        if keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
            pan += right;
        }
        if keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
            pan -= right;
        }
        cam.focus += pan * speed;
        if keys.pressed(KeyCode::KeyQ) {
            cam.yaw += dt;
        }
        if keys.pressed(KeyCode::KeyE) {
            cam.yaw -= dt;
        }
        // X / Z: camera up / down (closer / farther along the view).
        if keys.pressed(KeyCode::KeyX) {
            cam.distance = (cam.distance * (1.0 + dt)).min(1_500.0);
        }
        if keys.pressed(KeyCode::KeyZ) {
            cam.distance = (cam.distance * (1.0 - dt)).max(4.0);
        }
        if scroll.delta.y != 0.0 && !fixed {
            cam.distance = (cam.distance * (1.0 - scroll.delta.y * 0.1)).clamp(4.0, 1_500.0);
            cam.pitch = (0.15 + cam.distance / 600.0).clamp(0.15, 1.2);
        }
        // Follow the terrain: the focus sits on the ground, or on the sea where the ground is the
        // sea bed; the camera never goes below either.
        cam.focus.y = crate::terrain::surface_height(Vec2::new(cam.focus.x, -cam.focus.z));
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        tf.translation = cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance);
        let floor = crate::terrain::surface_height(Vec2::new(tf.translation.x, -tf.translation.z)) + 2.0;
        tf.translation.y = tf.translation.y.max(floor);
        tf.look_at(cam.focus, Vec3::Y);
    }
}

/// Creates the view and label for any model unit that doesn't have one yet. This covers
/// the first frame and every restart (R), which replaces the units.
///
/// Figures are GPU-skinned (`skin`): all kits are built first, then their clips go into one
/// bone atlas, then every man's parts are spawned with static skinned meshes and his figure
/// slot as `MeshTag`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_missing_views(
    mut commands: Commands,
    sim: Res<BattleSim>,
    data: Res<GameData>,
    mut soldiers: ResMut<BattleSoldiers>,
    views: Query<(Entity, &UnitView)>,
    labels: Query<(Entity, &UnitLabel)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut skin_materials: ResMut<Assets<SkinMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
    label_font: Option<Res<super::labels::LabelFont>>,
    corpses: Query<Entity, With<Corpse>>,
) {
    if !sim.is_changed() {
        return;
    }
    // On a restart, throw away the old pictures first.
    let have_all = sim.info.iter().all(|i| views.iter().any(|(_, v)| v.id == i.id));
    if have_all && views.iter().count() == sim.info.len() {
        return;
    }
    for (e, _) in &views {
        commands.entity(e).despawn();
    }
    for e in &corpses {
        commands.entity(e).despawn();
    }
    commands.insert_resource(ActionRng(SelectionRng(selection_seed(sim.seed) ^ 0x0BAD_F00D)));
    for (e, _) in &labels {
        commands.entity(e).despawn();
    }
    // 1. Kits per unit, plus each unit's standard bearer when it has one.
    let mut unit_kits: Vec<Vec<FigureKit>> = Vec::new();
    let mut bearer_kits: Vec<Option<FigureKit>> = Vec::new();
    for info in &sim.info {
        let mut kits = Vec::new();
        let mut bearer = None;
        if let (Some(lib), Some(unit)) = (soldiers.lib.as_mut(), sim.battle.units.iter().find(|u| u.id == info.id)) {
            let faction = if info.faction.is_empty() { sim.side_factions[usize::from(unit.side).min(1)].clone() } else { info.faction.clone() };
            let keys = data.db.unit_stats(&info.key).map(animation_keys);
            let mut assets = KitAssets { meshes: &mut meshes, materials: &mut materials, images: &mut images };
            for seed in 0..KITS_PER_UNIT as u64 {
                let Some(keys) = keys.as_ref() else { break };
                match lib.build_figure(&info.key, Some(&faction), keys, VariantRole::Soldier, seed, &mut assets) {
                    Ok(kit) => kits.push(kit),
                    Err(e) => {
                        warn!("{}: {e}", info.key);
                        break;
                    }
                }
            }
            // The standard bearer (CONFIRMED: `unit_stats_land` #4/5/6 officer / musician /
            // standard bearer feed `battle_personalities`, and `plan_figure` reads his own
            // animation table and equipment theme from it). He is a figure of his own because his
            // variant and skeleton are not his unit's, and the flag pole is his personal equipment.
            if let Some(keys) = keys.as_ref()
                && keys.standard_bearer.as_deref().is_some_and(|s| !s.is_empty())
            {
                match lib.build_figure(&info.key, Some(&faction), keys, VariantRole::StandardBearer, 0, &mut assets) {
                    Ok(kit) => bearer = Some(kit),
                    Err(e) => warn!("{} standard bearer: {e}", info.key),
                }
            }
        }
        unit_kits.push(kits);
        bearer_kits.push(bearer);
    }
    // 2. The bone atlas of every clip in use, and the figure slots.
    let mut atlas = BoneAtlas::default();
    for kit in unit_kits.iter().flatten().chain(bearer_kits.iter().flatten()) {
        for level in kit.levels.iter().flat_map(|(_, l)| l).chain(kit.actions.values()) {
            for clip in level.man.iter().chain(&level.mount) {
                atlas.add(clip);
            }
        }
    }
    let total_men: usize = sim
        .info
        .iter()
        .zip(&unit_kits)
        .filter(|(_, k)| !k.is_empty())
        .filter_map(|(i, _)| sim.battle.units.iter().find(|u| u.id == i.id))
        .map(|u| (u.men as usize).min(MAX_FIGURES))
        .sum::<usize>()
        // One figure slot each for the standard bearers.
        + bearer_kits.iter().filter(|k| k.is_some()).count();
    let figures_data = vec![[0u32; 4]; (2 * total_men).max(1)];
    info!("Bone atlas: {} matrices ({:.1} MB)", atlas.matrices.len(), atlas.matrices.len() as f64 * 64.0 / 1e6);
    let bones = buffers.add(ShaderBuffer::new(&atlas.bytes(), RenderAssetUsages::RENDER_WORLD));
    let figures = buffers.add(ShaderBuffer::new(&skin::figure_bytes(&figures_data), RenderAssetUsages::default()));
    // 3. Static skinned meshes and skin materials, shared by every man of a kit.
    let mut part_meshes: HashMap<usize, Vec<Handle<Mesh>>> = HashMap::new();
    let mut skin_mats: HashMap<AssetId<StandardMaterial>, Handle<SkinMaterial>> = HashMap::new();
    let mut skin_mat = |h: &Handle<StandardMaterial>| -> Handle<SkinMaterial> {
        skin_mats
            .entry(h.id())
            .or_insert_with(|| {
                let base = materials.get(h).cloned().unwrap_or_default();
                skin_materials.add(SkinMaterial { base, extension: SkinExt { bones: bones.clone(), figures: figures.clone() } })
            })
            .clone()
    };
    // Per unit, per kit: its parts as (meshes per LOD, material, is_mount).
    let mut kit_parts: Vec<Vec<Vec<KitPart>>> = Vec::new();
    for kits in &unit_kits {
        let mut per_kit = Vec::new();
        for kit in kits {
            let mut parts = Vec::new();
            for kp in &kit.man.parts {
                let mesh = part_meshes
                    .entry(std::sync::Arc::as_ptr(&kp.part) as usize)
                    .or_insert_with(|| (0..skin::part_lod_count(&kp.part)).map(|l| meshes.add(skin::part_mesh_lod(&kp.part, l))).collect())
                    .clone();
                parts.push((mesh, skin_mat(&kp.material), false));
            }
            if let Some(m) = &kit.mount {
                for (pi, (piece, _)) in m.pieces.iter().enumerate() {
                    // LOD 0 and the horse's lower LODs (same piece in each; `mount::animated_lod`).
                    let mesh = part_meshes
                        .entry(std::sync::Arc::as_ptr(piece) as usize)
                        .or_insert_with(|| {
                            std::iter::once(piece).chain(m.lower_lods.iter().filter_map(|l| l.get(pi))).map(|p| meshes.add(skin::piece_mesh(p))).collect()
                        })
                        .clone();
                    parts.push((mesh, skin_mat(&m.material), true));
                }
            }
            per_kit.push(parts);
        }
        kit_parts.push(per_kit);
    }
    // The standard bearers' parts, with the same skin materials and the same shared meshes.
    let mut bearer_parts: Vec<Option<Vec<KitPart>>> = Vec::new();
    for kit in bearer_kits.iter() {
        bearer_parts.push(kit.as_ref().map(|kit| {
            let mut parts = Vec::new();
            for kp in &kit.man.parts {
                let mesh = part_meshes
                    .entry(std::sync::Arc::as_ptr(&kp.part) as usize)
                    .or_insert_with(|| (0..skin::part_lod_count(&kp.part)).map(|l| meshes.add(skin::part_mesh_lod(&kp.part, l))).collect())
                    .clone();
                parts.push((mesh, skin_mat(&kp.material), false));
            }
            if let Some(m) = &kit.mount {
                for (pi, (piece, _)) in m.pieces.iter().enumerate() {
                    let mesh = part_meshes
                        .entry(std::sync::Arc::as_ptr(piece) as usize)
                        .or_insert_with(|| {
                            std::iter::once(piece).chain(m.lower_lods.iter().filter_map(|l| l.get(pi))).map(|p| meshes.add(skin::piece_mesh(p))).collect()
                        })
                        .clone();
                    parts.push((mesh, skin_mat(&m.material), true));
                }
            }
            parts
        }));
    }
    // 4. Units, men and labels. Each man draws his selection number in spawn order.
    let mut next_slot = 0usize;
    let mut selection_rng = SelectionRng(selection_seed(sim.seed));
    let spawns: Vec<_> = sim
        .info
        .iter()
        .zip(unit_kits)
        .zip(kit_parts)
        .zip(bearer_kits)
        .zip(bearer_parts)
        .collect();
    for ((((info, kits), parts), bkit), bparts) in spawns {
        let Some(unit) = sim.battle.units.iter().find(|u| u.id == info.id) else { continue };
        let pos = Vec2::new(unit.position.0, unit.position.1);
        let parent = commands
            .spawn((Transform::from_translation(world_of(pos)), Visibility::default(), UnitView { id: info.id }))
            .id();
        if soldiers.lib.is_some() {
            let mut men = Vec::new();
            let mut selections = Vec::new();
            let first_slot = next_slot;
            if !kits.is_empty() {
                let count = (unit.men as usize).min(MAX_FIGURES);
                let ranks = info.ranks.max(1) as usize;
                let files = count.div_ceil(ranks).max(1);
                let (dx, dz) = (info.size_m.x / files as f32, info.size_m.y / ranks as f32);
                for i in 0..count {
                    let (file, rank) = (i % files, i / files);
                    let local = Vec3::new(
                        (file as f32 - (files - 1) as f32 * 0.5) * dx,
                        0.0,
                        (rank as f32 - (ranks - 1) as f32 * 0.5) * dz,
                    );
                    let slot = (first_slot + i) as u32;
                    let man = commands
                        .spawn((Transform::from_translation(local), Visibility::default()))
                        .with_children(|c| {
                            for (mesh, material, is_mount) in &parts[i % parts.len()] {
                                let mut part = c.spawn((
                                    Mesh3d(mesh[0].clone()),
                                    MeshMaterial3d(material.clone()),
                                    MeshTag(2 * slot + u32::from(*is_mount)),
                                    // The vertices are bone-local, so the mesh bounds mean nothing: a
                                    // fixed box around a man (or a horse) lets cameras and shadow
                                    // cascades cull him (PROVISIONAL sizes).
                                    figure_bounds(*is_mount),
                                    bevy::camera::visibility::NoAutoAabb,
                                ));
                                if mesh.len() > 1 {
                                    part.insert(PartLods(mesh.clone(), *is_mount));
                                }
                            }
                        })
                        .id();
                    commands.entity(parent).add_child(man);
                    men.push(man);
                    selections.push(selection_rng.next_selection());
                }
                next_slot += count;
            }
            // The standard bearer, when the unit has one: he stands in the middle of the front
            // rank (PROVISIONAL -- the exe places him in the formation and we do not know where;
            // target: the exe's standard bearer slot in the unit's formation). He gets his own
            // figure slot because he plays a clip of his own skeleton.
            let bearer = match (bkit, bparts) {
                (Some(kit), Some(bparts)) if !bparts.is_empty() => {
                    let ranks = info.ranks.max(1) as usize;
                    let dz = info.size_m.y / ranks as f32;
                    let slot = next_slot as u32;
                    next_slot += 1;
                    let local = Vec3::new(0.0, 0.0, -(ranks as f32 - 1.0) * 0.5 * dz);
                    let entity = commands
                        .spawn((
                            Transform::from_translation(local),
                            Visibility::default(),
                            // The pole bone starts at its frame-0 rest matrix; `sync_views` writes
                            // the real one every frame.
                            PoleBone { model: ntw_formats::cloth::bone3_at_ground() },
                        ))
                        .with_children(|c| {
                            for (mesh, material, is_mount) in &bparts {
                                c.spawn((
                                    Mesh3d(mesh[0].clone()),
                                    MeshMaterial3d(material.clone()),
                                    MeshTag(2 * slot + u32::from(*is_mount)),
                                    figure_bounds(*is_mount),
                                    bevy::camera::visibility::NoAutoAabb,
                                ));
                            }
                        })
                        .id();
                    commands.entity(parent).add_child(entity);
                    Some(Bearer { entity, kit })
                }
                _ => None,
            };
            commands.entity(parent).insert(UnitFigures {
                first_slot,
                last_facing: unit.facing,
                seated: false,
                ground: GroundSpeed::new(pos, sim.battle.tick),
                acts: vec![actions::ManAct::default(); men.len()],
                lods: vec![0; men.len()],
                last_men: unit.men,
                last_volleys: unit.volleys_fired,
                last_melee: unit.in_melee,
                melee_idle: kits.first().map(|k| actions::present(k, unit_animation::COMBAT_IDLE, k.mount.is_some())).unwrap_or_default(),
                melee_attack: kits.first().map(|k| actions::present(k, unit_animation::ATTACK, k.mount.is_some())).unwrap_or_default(),
                kits,
                bearer,
                men,
                selections,
                clock: 0.0,
            });
        }
        let mut label = commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, UnitLabel { id: info.id, text: String::new() }));
        if label_font.as_ref().is_some_and(|f| f.0.is_some()) {
            label.insert(ImageNode::default());
        } else {
            label.insert((Text::new(""), TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() }, TextColor(Color::WHITE)));
        }
    }
    commands.insert_resource(SkinState { atlas, figures, data: figures_data });
}

/// Every frame: copy each model unit's position, facing, men and morale into its picture,
/// and animate the figures (walk while moving, idle otherwise).
#[allow(clippy::too_many_arguments)]
pub fn sync_views(
    sim: Res<BattleSim>,
    time: Res<Time>,
    mut commands: Commands,
    mut action_rng: Option<ResMut<ActionRng>>,
    mut skin: Option<ResMut<SkinState>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut views: Query<(&UnitView, &mut Transform, Option<&mut UnitFigures>)>,
    mut men_tf: Query<&mut Transform, Without<UnitView>>,
    mut visibility: Query<&mut Visibility>,
    mut gizmos: Gizmos,
    cams: Query<&GlobalTransform, With<Camera3d>>,
    children: Query<&Children>,
    mut part_lods: Query<(&mut Mesh3d, &PartLods)>,
    mut pole_bones: Query<&mut PoleBone>,
) {
    // Test harness: `NAPOLEON_UNIT_LOD=0` keeps every man at LOD 0 (for frame-rate comparisons).
    let cam_pos = cams.iter().next().map(|g| g.translation()).filter(|_| std::env::var("NAPOLEON_UNIT_LOD").map_or(true, |v| v != "0"));
    let dt = time.delta_secs();
    let map = crate::terrain::current_map();
    let ground = |p: Vec2| map.as_ref().map_or(0.0, |m| m.height_at(p.x, p.y));
    for (view, mut transform, figures) in &mut views {
        let Some(unit) = find(&sim, view.id) else { continue };
        let pos = Vec2::new(unit.position.0, unit.position.1);
        transform.translation = world_of(pos);
        // A figure faces local -Z; model facing 0 = +x. See `world_of` for the axes.
        transform.rotation = Quat::from_rotation_y(unit.facing - std::f32::consts::FRAC_PI_2);
        if let Some(info) = sim.info.iter().find(|i| i.id == view.id).filter(|_| !unit.off_field()) {
            let half = info.size_m * 0.5;
            let corners = [(-half.x, -half.y), (half.x, -half.y), (half.x, half.y), (-half.x, half.y)]
                .map(|(x, z)| {
                    let p = transform.transform_point(Vec3::new(x, 0.0, z));
                    Vec3::new(p.x, ground(Vec2::new(p.x, -p.z)) + 0.15, p.z)
                });
            let color = unit_color(unit, sim.selected == Some(unit.id));
            for k in 0..4 {
                gizmos.line(corners[k], corners[(k + 1) % 4], color);
            }
        }
        let Some(mut figures) = figures else { continue };
        // The ground speed picks the gait (walk below the walk/run midpoint).
        let moved = pos.distance(figures.ground.pos);
        figures.ground.observe(pos, sim.battle.tick, dt);
        // Re-seat the men on the terrain only when the unit moved or turned (writing 2,000
        // transforms every frame would make Bevy re-propagate them all).
        let reseat = moved > 1e-4 || figures.last_facing != unit.facing || !figures.seated;
        figures.last_facing = unit.facing;
        figures.seated = true;
        let gait = figures.ground.gait(unit.walk_speed, unit.run_speed);
        // What the unit is doing (display only, read from the model; see `actions`).
        let mode = if unit.in_melee {
            Mode::Melee
        } else if gait != Gait::Stand {
            Mode::Gait(gait)
        } else if unit.missile.is_some() && (unit.fire_target.is_some() || unit.reload_ticks_left > 0 || unit.aiming) {
            Mode::Ready { aiming: unit.aiming }
        } else {
            Mode::Gait(Gait::Stand)
        };
        let now = time.elapsed_secs();
        let Some(skin) = skin.as_mut() else { continue };
        let SkinState { atlas, data: skin_data, .. } = &mut **skin;
        let mut fallback_rng = ActionRng(SelectionRng(1));
        let action_rng: &mut ActionRng = match action_rng.as_deref_mut() {
            Some(r) => r,
            None => &mut fallback_rng,
        };
        let length = |a: &std::sync::Arc<ntw_formats::anim::Anim>| atlas.slot(a).map_or(0.0, |s| s.play_length());
        let files = {
            let ranks = sim.info.iter().find(|i| i.id == view.id).map_or(1, |i| i.ranks.max(1)) as usize;
            figures.men.len().div_ceil(ranks).max(1)
        };
        let figures = &mut *figures;
        // Events: men who fell, a volley, contact with charging cavalry.
        if unit.men < figures.last_men {
            let cause = match mode {
                Mode::Melee => DeathCause::Melee,
                Mode::Ready { .. } if unit.reload_ticks_left > 0 => DeathCause::Reloading,
                Mode::Ready { .. } => DeathCause::Poised,
                Mode::Gait(Gait::Walk) => DeathCause::Walking,
                Mode::Gait(Gait::Run) if unit.charging => DeathCause::Charging,
                Mode::Gait(Gait::Run) => DeathCause::Running,
                Mode::Gait(Gait::Stand) => DeathCause::Standing,
            };
            let speed = if matches!(mode, Mode::Gait(Gait::Walk | Gait::Run)) { figures.ground.speed } else { 0.0 };
            for i in (unit.men as usize)..(figures.last_men as usize).min(figures.men.len()) {
                if figures.acts[i].dead {
                    continue;
                }
                let kit = &figures.kits[i % figures.kits.len()];
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                if let Some(clips) = actions::death(kit, cause, speed, sel, &mut action_rng.0) {
                    figures.acts[i].play(now, vec![clips], true);
                    figures.acts[i].dead = true;
                    commands.entity(figures.men[i]).remove_parent_in_place().insert(Corpse);
                }
            }
        }
        figures.last_men = unit.men;
        let alive = (unit.men as usize).min(figures.men.len());
        if unit.volleys_fired > figures.last_volleys && mode != Mode::Melee {
            for i in 0..alive {
                let kit = &figures.kits[i % figures.kits.len()];
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                let reload = format!("RELOAD_{}", 1 + sel % 2);
                let chain: Vec<_> = [unit_animation::FIRE, reload.as_str()].iter().filter_map(|s| actions::pick(kit, s, sel)).collect();
                figures.acts[i].play(now + actions::volley_delay(sel), chain, false);
            }
        }
        figures.last_volleys = unit.volleys_fired;
        if unit.in_melee && !figures.last_melee {
            let charged = sim.battle.units.iter().any(|e| {
                e.side != unit.side && e.is_cavalry && e.charging && e.in_melee && {
                    let (dx, dy) = (e.position.0 - unit.position.0, e.position.1 - unit.position.1);
                    dx * dx + dy * dy < 80.0 * 80.0
                }
            });
            let knockdowns = figures.kits.first().map(|k| actions::present(k, unit_animation::KNOCKDOWN, false)).unwrap_or_default();
            for i in 0..alive {
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                figures.acts[i].next_attack = now + (sel % 10) as f32 * 0.2;
                // PROVISIONAL: front-rank men knocked down by a charge, about 3 in 10.
                if charged && i < files && !knockdowns.is_empty() && family_pick(&mut action_rng.0, 10) < 3 {
                    let kit = &figures.kits[i % figures.kits.len()];
                    let k = family_pick(&mut action_rng.0, knockdowns.len() as u32) as usize;
                    let chain: Vec<_> = [knockdowns[k].as_str(), unit_animation::FACE_DOWN_GET_UP].iter().filter_map(|s| actions::pick(kit, s, sel)).collect();
                    figures.acts[i].play(now, chain, false);
                }
            }
        }
        figures.last_melee = unit.in_melee;
        for (i, man) in figures.men.iter().enumerate() {
            let dead = figures.acts[i].dead;
            // Each man stands on the terrain under him (the unit's own origin is on the ground
            // at its centre; the parent only turns about Y, so a local y offset = a world one).
            // The fallen are detached and stay where they fell.
            if reseat && !dead && let Ok(mut man_tf) = men_tf.get_mut(*man) {
                let p = transform.transform_point(Vec3::new(man_tf.translation.x, 0.0, man_tf.translation.z));
                man_tf.translation.y = ground(Vec2::new(p.x, -p.z)) - transform.translation.y;
            }
            // Part LODs by camera distance (CONFIRMED switch distances; only on a change).
            if !dead && let (Some(cam), Ok(man_tf)) = (cam_pos, men_tf.get(*man)) {
                let d2 = transform.transform_point(man_tf.translation).distance_squared(cam);
                let soldier = unit_lod(d2, 4) as u8;
                // Horses: the `warscape_animated_lod` ranges (INFERRED metres).
                let horse = figures.kits[i % figures.kits.len()].mount.as_ref().map_or(0, |m| ntw_formats::mount::animated_lod(&m.lod_distances, d2.sqrt())) as u8;
                let want = soldier | (horse << 4);
                if figures.lods[i] != want {
                    figures.lods[i] = want;
                    for c in children.get(*man).into_iter().flatten() {
                        if let Ok((mut mesh, lods)) = part_lods.get_mut(*c) {
                            let level = if lods.1 { horse } else { soldier } as usize;
                            let h = &lods.0[level.min(lods.0.len() - 1)];
                            if mesh.0 != *h {
                                mesh.0 = h.clone();
                            }
                        }
                    }
                }
            }
            if let Ok(mut v) = visibility.get_mut(*man) {
                // Reinforcements still off the field (waiting or held) are not drawn, nor a garrison
                // inside its building (PROVISIONAL: the original shows men at the windows).
                let off_field = unit.off_field() || unit.garrison.is_some();
                let want = if (i < unit.men as usize || dead) && !off_field { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
        }
        // The speed level whose clip speed is closest to the unit's, played at
        // `speed / clip speed` (`unit_animation::pick_level`). All kits of a unit have the
        // same clips, so the first kit's levels pick for every man.
        let Some(levels) = figures.kits.first().and_then(|k| k.gait_levels(gait)) else { continue };
        let ground_speed = if gait == Gait::Stand { 0.0 } else { figures.ground.speed };
        let Some((li, rate)) = pick_level(levels.iter().map(|l| l.speed), ground_speed) else { continue };
        // PROVISIONAL clamp (ours): a unit placed by a script or a reinforcement arrival during a
        // tick shows a huge "speed" until its next moving tick, or for up to `STILL_SECONDS` if it
        // then stands still.
        figures.clock += dt * rate.clamp(0.25, 4.0);
        let clock = figures.clock;
        // Every man plays his own alternative clips, with his own phase (GPU skinning): a
        // one-shot if one runs, else the loop of the unit's mode.
        for i in 0..figures.men.len() {
            let kit = &figures.kits[i % figures.kits.len()];
            let sel = figures.selections.get(i).copied().unwrap_or(0);
            let act = &mut figures.acts[i];
            act.advance(now, length);
            if mode == Mode::Melee && !act.dead && i < alive && act.shot.is_none() && now >= act.next_attack && !figures.melee_attack.is_empty() {
                let k = family_pick(&mut action_rng.0, figures.melee_attack.len() as u32) as usize;
                if let Some(clips) = actions::pick(kit, &figures.melee_attack[k], sel) {
                    act.next_attack = now + length(&clips.0) + actions::swing_gap(sel);
                    act.play(now, vec![clips], false);
                }
            }
            // The loop: gait level, or the mode's action loop (stand level if the table has none).
            let Some(level) = kit.gait_levels(gait).and_then(|l| l.get(li)) else { continue };
            let gait_pair = (level.man[alternative(sel, level.man.len())].clone(), (!level.mount.is_empty()).then(|| level.mount[alternative(sel, level.mount.len())].clone()));
            let loop_slot = match mode {
                Mode::Ready { aiming } if aiming && kit.actions.contains_key(unit_animation::AIM) => Some(unit_animation::AIM.to_string()),
                Mode::Ready { .. } => Some(unit_animation::COMBAT_READY.to_string()),
                Mode::Melee => figures.melee_idle.get(alternative(sel, figures.melee_idle.len().max(1))).cloned(),
                Mode::Gait(_) => None,
            };
            let (loop_man, loop_mount) = match loop_slot.as_deref().and_then(|s| actions::pick(kit, s, sel)) {
                Some((m, mount)) => (m, mount.or(gait_pair.1.clone())),
                None => gait_pair.clone(),
            };
            let slot = figures.first_slot + i;
            let shot = act.shot.as_ref().filter(|s| now >= s.start);
            let (man_fig, mount_fig) = match shot {
                Some(s) => {
                    let t = now - s.start;
                    let m = atlas.slot(&s.man).map(|c| c.figure_once(t));
                    let h = s.mount.as_ref().and_then(|a| atlas.slot(a)).map(|c| c.figure_once(t));
                    (m, h)
                }
                None => (None, None),
            };
            let Some(man_clip) = atlas.slot(&loop_man) else { continue };
            let t = clock + skin::phase(view.id, i) * man_clip.duration();
            let man_fig = man_fig.unwrap_or_else(|| man_clip.figure(t));
            skin_data[2 * slot] = man_fig;
            // A paired mount clip has the same frame count: same frame, same blend.
            skin_data[2 * slot + 1] = mount_fig.or_else(|| loop_mount.as_ref().and_then(|m| atlas.slot(m)).map(|m| m.figure(t))).unwrap_or(man_fig);
        }
        // The standard bearer: the same clip choice as a man, on his own figure slot, and the
        // pole bone's matrix for the frame he is on so `flag::sync_flags` can hang the cloth from
        // it. He is not one of `unit.men`, so he never fires and never dies with the count.
        if let Some(bearer) = figures.bearer.as_ref() {
            // A stable selection number of his own, from the unit's first man (deterministic, and
            // it only picks which alternative clip of the gait he plays).
            let sel = figures.selections.first().copied().unwrap_or(0);
            let level = bearer.kit.gait_levels(gait).and_then(|l| l.get(li));
            let clip = level
                .map(|l| l.man[alternative(sel, l.man.len())].clone())
                .or_else(|| bearer.kit.anims(gait).map(|a| a.man.clone()));
            if let Some(clip) = clip
                && let Some(slot_atlas) = atlas.slot(&clip)
            {
                let t = clock + skin::phase(view.id, usize::MAX) * slot_atlas.duration();
                let fig = slot_atlas.figure(t);
                let bslot = figures.first_slot + figures.men.len();
                if bslot * 2 + 1 < skin_data.len() {
                    skin_data[2 * bslot] = fig;
                    skin_data[2 * bslot + 1] = fig;
                    if let Some(bone) = bone_at(atlas, &fig, ntw_formats::cloth::POLE_BONE as usize)
                        && let Ok(mut p) = pole_bones.get_mut(bearer.entity)
                    {
                        p.model = bone;
                    }
                }
            }
            // He stands on the terrain under him, and is hidden with the rest of his unit.
            if reseat
                && let (Ok(mut b_tf), Ok(mut v)) = (men_tf.get_mut(bearer.entity), visibility.get_mut(bearer.entity))
            {
                let p = transform.transform_point(Vec3::new(b_tf.translation.x, 0.0, b_tf.translation.z));
                b_tf.translation.y = ground(Vec2::new(p.x, -p.z)) - transform.translation.y;
                let off_field = unit.off_field() || unit.garrison.is_some();
                let want = if off_field { Visibility::Hidden } else { Visibility::Inherited };
                if *v != want {
                    *v = want;
                }
            }
        }
    }
    if let Some(skin) = skin.as_ref()
        && let Some(mut buffer) = buffers.get_mut(&skin.figures)
    {
        buffer.data = Some(skin::figure_bytes(&skin.data));
    }
}

/// One bone's matrix, in the figure's own model space, from the four numbers a figure slot holds
/// (`skin::ClipSlot::figure`): the two frames' starts in the bone storage and the blend between
/// them. The bone's own index offsets into the frame.
fn bone_at(atlas: &BoneAtlas, fig: &[u32; 4], bone: usize) -> Option<[f32; 16]> {
    let a = (*fig.first()? as usize).checked_add(bone)?;
    let b = (*fig.get(1)? as usize).checked_add(bone)?;
    let t = f32::from_bits(*fig.get(2)?);
    let ma = atlas.matrices.get(a)?;
    let mb = atlas.matrices.get(b).unwrap_or(ma);
    Some(std::array::from_fn(|k| ma[k] + (mb[k] - ma[k]) * t))
}

fn find(sim: &BattleSim, id: u32) -> Option<&LandUnit> {
    sim.battle.units.iter().find(|u| u.id == id)
}

/// Blue for France (side 0), red for Austria (side 1). Paler as morale drops; grey when
/// shattered or destroyed; yellow when selected.
fn unit_color(unit: &LandUnit, selected: bool) -> Color {
    if selected {
        return Color::srgb(1.0, 0.9, 0.2);
    }
    let base = if unit.side == 0 {
        Vec3::new(0.15, 0.3, 0.85)
    } else {
        Vec3::new(0.85, 0.15, 0.15)
    };
    let fade = match unit.morale.state {
        MoraleState::Shattered => return Color::srgb(0.45, 0.45, 0.45),
        _ if unit.men == 0 => return Color::srgb(0.45, 0.45, 0.45),
        MoraleState::Broken => 0.65,
        MoraleState::Wavering => 0.45,
        MoraleState::Shaken => 0.25,
        _ => 0.0,
    };
    let c = base.lerp(Vec3::ONE, fade);
    Color::srgb(c.x, c.y, c.z)
}

/// Every frame: draws recent volleys (display only, from [`VolleyFx`]) with Bevy "gizmos"
/// (simple lines and circles drawn fresh every frame).
/// - A volley is a pale-yellow line from the shooter to the target plus a "flash" circle at the
///   shooter; both fade out over `VOLLEY_FX_SECONDS`. The more men it killed, the bigger the
///   flash at the target.
/// - The selected unit, if it can shoot, gets a faint ring showing its missile range, and a red
///   line to the enemy it was ordered to shoot at.
pub fn draw_volleys(sim: Res<BattleSim>, fx: Res<VolleyFx>, mut gizmos: Gizmos) {
    let now = sim.battle.time_seconds();
    let at = |id: u32| find(&sim, id).map(|u| world_of(Vec2::new(u.position.0, u.position.1)) + Vec3::Y * 1.4);
    let flat = |p: Vec3| Isometry3d::new(p, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    for (fired_at, v) in &fx.recent {
        let (Some(from), Some(to)) = (at(v.shooter), at(v.target)) else { continue };
        let fade = (1.0 - (now - fired_at) / VOLLEY_FX_SECONDS).clamp(0.0, 1.0);
        gizmos.line(from, to, Color::srgba(1.0, 0.95, 0.6, 0.8 * fade));
        gizmos.circle(flat(from), 3.0, Color::srgba(1.0, 1.0, 0.8, fade));
        if v.kills > 0 {
            let r = 2.0 + v.kills as f32 * 0.5;
            gizmos.circle(flat(to), r, Color::srgba(1.0, 0.3, 0.2, fade));
        }
    }
    if let Some(unit) = sim.selected.and_then(|id| find(&sim, id))
        && unit.missile.is_some()
    {
        let pos = world_of(Vec2::new(unit.position.0, unit.position.1)) + Vec3::Y * 0.2;
        let range = unit.missile_range(sim.battle.kv_rules.fire_on_walls_range_modifier);
        gizmos.circle(flat(pos), range, Color::srgba(1.0, 1.0, 1.0, 0.25)).resolution(64);
        if let Some(target) = unit.fire_target.and_then(at) {
            gizmos.line(pos, target, Color::srgba(1.0, 0.2, 0.2, 0.7));
        }
    }
}

/// Test harness: with `NAPOLEON_BATTLE_SCREENSHOT=<file.png>` set, saves one screenshot
/// after a few seconds and quits (for checking the 3D view without clicking).
pub fn screenshot_from_env(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut done: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = std::env::var_os("NAPOLEON_BATTLE_SCREENSHOT") else { return };
    let t = time.elapsed_secs();
    if !*done && t > 8.0 {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(std::path::PathBuf::from(path)));
        *done = true;
    }
    if *done && t > 9.5 {
        exit.write(AppExit::Success);
    }
}

/// A box around one standing figure in its own space: a man ~2 m tall, a horse ~3 m long.
fn figure_bounds(mount: bool) -> bevy::camera::primitives::Aabb {
    let (centre, half) = if mount { (Vec3::new(0.0, 1.4, 0.0), Vec3::new(1.2, 1.8, 2.2)) } else { (Vec3::new(0.0, 1.1, 0.0), Vec3::new(1.2, 1.4, 1.2)) };
    bevy::camera::primitives::Aabb::from_min_max(centre - half, centre + half)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::TICK_SECONDS;

    /// Line infantry's walk and run clip speeds (`MUS_T_Walk_100/127/173`, `Jog_219/313`,
    /// analysis/units/CAVALRY.md §7) and a line battalion's `battle_entities` speeds.
    const WALK_LEVELS: [f32; 3] = [1.0, 1.27, 1.73];
    const RUN_LEVELS: [f32; 2] = [2.19, 3.13];
    const WALK: f32 = 1.4;
    const RUN: f32 = 3.6;

    /// Plays the app's frame loop: the model ticks at 10 Hz on the fixed clock (`FixedUpdate`
    /// before `Update`), moving the unit `speed × 0.1 s` per tick, and every frame the view
    /// observes it. Returns each frame's (gait, speed level, playback rate) from the second
    /// tick on.
    fn frames(speed: f32, frame_dts: &[f32], count: usize) -> Vec<(Gait, usize, f32)> {
        let (mut tick, mut x, mut fixed) = (0u32, 0.0f32, 0.0f32);
        let mut g = GroundSpeed::new(Vec2::ZERO, 0);
        let mut out = Vec::new();
        for f in 0..count {
            let dt = frame_dts[f % frame_dts.len()];
            fixed += dt;
            while fixed >= TICK_SECONDS {
                fixed -= TICK_SECONDS;
                tick += 1;
                x += speed * TICK_SECONDS;
            }
            g.observe(Vec2::new(x, 0.0), tick, dt);
            let gait = g.gait(WALK, RUN);
            let levels: &[f32] = if gait == Gait::Run { &RUN_LEVELS } else { &WALK_LEVELS };
            let (level, rate) = pick_level(levels.iter().copied(), g.speed).unwrap();
            if tick >= 2 {
                out.push((gait, level, rate));
            }
        }
        out
    }

    /// The walking jitter (2026-10-04 to 10-07): the view took the unit's speed from its
    /// per-frame displacement, but the model only moves it on 10 Hz ticks, so the smoothed speed
    /// swung by about ±20 % every tick and the men switched speed level (and clip) and playback
    /// rate several times a second. A unit marching at a steady speed must play one level at one
    /// rate, whatever the frame rate.
    #[test]
    fn steady_march_keeps_one_gait_level_and_rate() {
        for (speed, gait) in [(WALK, Gait::Walk), (RUN, Gait::Run)] {
            for dts in [&[1.0 / 60.0][..], &[1.0 / 144.0], &[1.0 / 30.0, 1.0 / 90.0, 1.0 / 45.0], &[1.0 / 20.0]] {
                let out = frames(speed, dts, 600);
                let (_, level0, rate0) = out[0];
                for (i, &(g, level, rate)) in out.iter().enumerate() {
                    assert_eq!(g, gait, "speed {speed}, frames {dts:?}: frame {i} gait");
                    assert_eq!(level, level0, "speed {speed}, frames {dts:?}: frame {i} speed level");
                    assert!((rate - rate0).abs() < 1e-3, "speed {speed}, frames {dts:?}: frame {i} rate {rate} vs {rate0}");
                }
            }
        }
    }

    /// A unit that stops stands after [`STILL_SECONDS`]; frames between two ticks are not "still".
    #[test]
    fn stops_then_stands() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0);
        let dt = 1.0 / 60.0;
        let mut x = 0.0;
        for f in 1..=60u32 {
            let tick = f / 6;
            if f % 6 == 0 {
                x += WALK * TICK_SECONDS;
            }
            g.observe(Vec2::new(x, 0.0), tick, dt);
            if f >= 6 {
                assert_eq!(g.gait(WALK, RUN), Gait::Walk, "frame {f}");
            }
        }
        for f in 61..=120u32 {
            g.observe(Vec2::new(x, 0.0), f / 6, dt);
        }
        assert_eq!(g.gait(WALK, RUN), Gait::Stand);
    }

    /// A restart (R) sends the tick back under the same views: the speed starts over instead of
    /// spreading the next step over ~4 billion ticks.
    #[test]
    fn restart_starts_over() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0);
        g.observe(Vec2::new(0.14, 0.0), 1, 0.1);
        g.observe(Vec2::new(50.0, 0.0), 0, 0.1);
        assert_eq!(g.gait(WALK, RUN), Gait::Stand);
        g.observe(Vec2::new(50.14, 0.0), 1, 0.1);
        assert!((g.speed - WALK).abs() < 1e-3, "speed {}", g.speed);
    }
}

/// One drawable piece of a figure kit: GPU mesh, skin material, and whether it belongs to the mount.
type KitPart = (Vec<Handle<Mesh>>, Handle<SkinMaterial>, bool);

/// The LOD meshes of a figure part entity (most detailed first) and whether it belongs to the
/// mount (horses switch by `mount::animated_lod`, men by `unit_model::unit_lod`).
#[derive(Component)]
pub struct PartLods(pub Vec<Handle<Mesh>>, pub bool);

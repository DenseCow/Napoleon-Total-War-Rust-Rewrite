//! `--view-model <unit key>`: shows a unit's soldier, officer, musician and standard bearer
//! side by side, assembled from the install and animated with the clips the DB animation
//! tables give them. Cavalry figures ride their horses (rider and horse clips paired).
//!
//! Controls: mouse orbit/pan/zoom as in the model viewer, N / P = next / previous unit,
//! Space = stand / walk / run, Esc = quit. With `--screenshot <folder>` it saves
//! `<folder>\<unit key>.png` per unit and quits.

use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use ntw_formats::unit_animation::{Gait, UnitAnimationKeys};
use ntw_formats::unit_model::VariantRole;

use super::{OrbitCamera, orbit_camera};
use crate::soldiers::{FigureKit, KitAssets, SoldierLibrary, animation_keys};

/// True if `query` is a `units` key with at least one uniform.
pub fn is_unit_key(lib: &SoldierLibrary, query: &str) -> bool {
    !lib.index.uniforms_for_unit(query, None).is_empty()
}

#[derive(Resource)]
struct UnitViewer {
    units: Vec<String>,
    /// `unit_stats_land` animation columns per unit, if the DB has the unit.
    keys: Vec<Option<UnitAnimationKeys>>,
    current: usize,
    loaded: Option<usize>,
    gait: Gait,
    secs_since_load: f32,
    screenshot_dir: Option<PathBuf>,
    shot_taken: bool,
}

#[derive(Resource, Default)]
struct Shown {
    figures: Vec<FigureKit>,
}

#[derive(Component)]
struct ShownPart;

/// Runs the soldier viewer. `units` are `units` keys.
pub fn run(lib: SoldierLibrary, units: Vec<String>, screenshot_dir: Option<PathBuf>) {
    let db = ntw_data::GameDatabase::from_vfs(&lib.vfs).ok();
    for w in db.iter().flat_map(|d| &d.load_warnings) {
        warn!("Game data: {w}");
    }
    let keys = units.iter().map(|u| db.as_ref().and_then(|db| db.unit_stats(u)).map(animation_keys)).collect();
    if let Some(dir) = &screenshot_dir {
        let _ = std::fs::create_dir_all(dir);
    }
    // Test harness: `NAPOLEON_VIEW_GAIT=walk|run` starts in that gait (for screenshots).
    let gait = match std::env::var("NAPOLEON_VIEW_GAIT").unwrap_or_default().as_str() {
        "walk" => Gait::Walk,
        "run" => Gait::Run,
        _ => Gait::Stand,
    };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "NapoleonRust unit viewer".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.45, 0.55, 0.65)))
        .insert_resource(GlobalAmbientLight { brightness: 700.0, ..default() })
        .insert_resource(lib)
        .insert_resource(Shown::default())
        .insert_resource(UnitViewer {
            units,
            keys,
            current: 0,
            loaded: None,
            gait,
            secs_since_load: 0.0,
            screenshot_dir,
            shot_taken: false,
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (key_input, load_unit, animate, orbit_camera, auto_screenshot).chain())
        .run();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        Camera3d::default(),
        Transform::default(),
        OrbitCamera { focus: Vec3::new(2.0, 1.2, 0.0), distance: 7.5, yaw: 0.6 + std::f32::consts::PI, pitch: 0.15 },
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(3.0, 8.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.38, 0.25), perceptual_roughness: 1.0, ..default() })),
    ));
}

fn key_input(input: Res<ButtonInput<KeyCode>>, mut viewer: ResMut<UnitViewer>, mut exit: MessageWriter<AppExit>) {
    let n = viewer.units.len();
    if input.just_pressed(KeyCode::KeyN) {
        viewer.current = (viewer.current + 1) % n;
    }
    if input.just_pressed(KeyCode::KeyP) {
        viewer.current = (viewer.current + n - 1) % n;
    }
    if input.just_pressed(KeyCode::Space) {
        viewer.gait = match viewer.gait {
            Gait::Stand => Gait::Walk,
            Gait::Walk => Gait::Run,
            Gait::Run => Gait::Stand,
        };
        info!("gait {:?}", viewer.gait);
    }
    if input.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

#[allow(clippy::too_many_arguments)]
fn load_unit(
    mut commands: Commands,
    mut lib: ResMut<SoldierLibrary>,
    mut viewer: ResMut<UnitViewer>,
    mut shown: ResMut<Shown>,
    old: Query<Entity, With<ShownPart>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut window: Query<&mut Window>,
) {
    if viewer.loaded == Some(viewer.current) {
        return;
    }
    for e in &old {
        commands.entity(e).despawn();
    }
    let index = viewer.current;
    viewer.loaded = Some(index);
    viewer.secs_since_load = 0.0;
    viewer.shot_taken = false;
    let unit = viewer.units[index].clone();
    for mut w in &mut window {
        w.title = format!("NapoleonRust unit viewer - {unit}");
    }
    shown.figures.clear();
    let Some(keys) = viewer.keys[index].clone() else {
        warn!("{unit}: not in unit_stats_land; nothing to animate");
        return;
    };
    let mut assets = KitAssets { meshes: &mut meshes, materials: &mut materials, images: &mut images };
    let roles = [VariantRole::Soldier, VariantRole::Officer, VariantRole::Musician, VariantRole::StandardBearer];
    let mut x = 0.0;
    for (i, role) in roles.into_iter().enumerate() {
        if role != VariantRole::Soldier && keys.personality(role).is_none() {
            continue;
        }
        match lib.build_figure(&unit, None, &keys, role, i as u64, &mut assets) {
            Ok(fig) => {
                let parent = commands.spawn((Transform::from_xyz(x, 0.0, 0.0), Visibility::default(), ShownPart)).id();
                fig.spawn_parts(&mut commands, parent);
                info!(
                    "{unit} {role:?}: table {} theme {:?}, {} parts, mount {:?}",
                    fig.plan.animation_table,
                    fig.plan.equipment_theme,
                    fig.man.parts.len(),
                    fig.mount.as_ref().map(|m| (&m.model_key, &m.mesh_path, m.pieces.len())),
                );
                for p in &fig.man.parts {
                    info!("    {:<26} {} ({} vertices)", p.part.category, p.part.source, p.part.vertices.len());
                }
                for (gait, a) in &fig.gaits {
                    info!("    {gait:?}: {} / {:?}", a.slots.0, a.slots.1);
                }
                x += if fig.mount.is_some() { 1.6 } else { 1.0 };
                shown.figures.push(fig);
            }
            Err(e) => warn!("{unit} {role:?}: {e}"),
        }
    }
}

fn animate(time: Res<Time>, viewer: Res<UnitViewer>, mut shown: ResMut<Shown>, mut meshes: ResMut<Assets<Mesh>>) {
    for fig in &mut shown.figures {
        fig.pose(viewer.gait, time.elapsed_secs(), &mut meshes);
    }
}

fn auto_screenshot(mut commands: Commands, time: Res<Time>, mut viewer: ResMut<UnitViewer>, mut exit: MessageWriter<AppExit>) {
    let Some(dir) = viewer.screenshot_dir.clone() else { return };
    viewer.secs_since_load += time.delta_secs();
    let wait = if viewer.current == 0 { 4.0 } else { 2.0 };
    if !viewer.shot_taken && viewer.secs_since_load >= wait {
        let name = format!("{}.png", viewer.units[viewer.current]);
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join(name)));
        viewer.shot_taken = true;
    }
    if viewer.shot_taken && viewer.secs_since_load >= wait + 0.75 {
        if viewer.current + 1 < viewer.units.len() {
            viewer.current += 1;
        } else {
            exit.write(AppExit::Success);
        }
    }
}

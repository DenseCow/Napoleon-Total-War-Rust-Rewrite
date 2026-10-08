//! Model viewer: shows original `.rigid_model` files from your install in 3D.
//!
//! ```text
//! cargo run -p napoleon -- --view-model <model> [<model> ...] [--screenshot <folder>]
//! cargo run -p napoleon -- --list-models <text>
//! ```
//! `<model>` can also be a `units` key (`Inf_Line_French_Fusiliers`): then the unit's
//! soldier, officer, musician and standard bearer are shown, animated (see `soldier.rs`).
//! Otherwise `<model>` is a pack path such as
//! `rigidmodels\buildings\american_church\american_church_piece01_destruct01_lod01.rigid_model`,
//! or just part of one (`american_church`): the first matching model is shown,
//! preferring LOD 1 (the most detailed).
//!
//! Controls: left mouse drag = orbit, right drag = pan, wheel = zoom,
//! N / P = next / previous model, W = wireframe-ish view of the bounding box, Esc = quit.
//!
//! With `--screenshot <folder>` the viewer saves `<folder>\<model name>.png` for each
//! model and quits after the last one (handy for checking without clicking).
//!
//! Everything is read from the install through the [`ntw_formats::pack::Vfs`]
//! (read-only). Textures are decoded on the CPU by [`ntw_formats::dds`].

mod soldier;
pub(crate) mod source;

use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use ntw_formats::dds::Dds;
use ntw_formats::rigid_model::RigidModel;

use crate::config;
use source::{ModelSource, NameFlags, convert_mesh, describe_material};

/// What the command line asked the viewer to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerArgs {
    /// Show these models (queries), optionally saving a screenshot of each into a folder.
    View { models: Vec<String>, screenshot_dir: Option<PathBuf> },
    /// Print every model path containing this text.
    List(String),
}

impl ViewerArgs {
    /// Reads `--view-model` / `--list-models` / `--screenshot` from the arguments
    /// (without the program name). `None` if the viewer was not requested.
    pub fn parse(args: &[String]) -> Option<Self> {
        if let Some(i) = args.iter().position(|a| a == "--list-models") {
            return Some(Self::List(args.get(i + 1).cloned().unwrap_or_default()));
        }
        let start = args.iter().position(|a| a == "--view-model")?;
        let mut models = Vec::new();
        let mut screenshot_dir = None;
        let mut i = start + 1;
        while i < args.len() {
            if args[i] == "--screenshot" {
                screenshot_dir = args.get(i + 1).map(PathBuf::from);
                i += 2;
            } else {
                models.push(args[i].clone());
                i += 1;
            }
        }
        Some(Self::View { models, screenshot_dir })
    }
}

/// Runs the viewer instead of the battle. Returns when the window closes.
pub fn run(args: ViewerArgs) {
    let data_dir = config::game_data_dir();
    let source = match ModelSource::open(&data_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Model viewer needs the original game: {e}");
            eprintln!("Set {} to your install folder if it is not the default Steam one.", config::INSTALL_DIR_ENV);
            return;
        }
    };
    let (queries, screenshot_dir) = match args {
        ViewerArgs::List(q) => {
            let found = source.list_models(&q);
            for p in &found {
                println!("{p}");
            }
            println!("{} models match {q:?}", found.len());
            return;
        }
        ViewerArgs::View { models, screenshot_dir } => (models, screenshot_dir),
    };
    // Unit keys (e.g. `Inf_Line_French_Fusiliers`) open the soldier viewer instead.
    if let Some(first) = queries.first() {
        match crate::soldiers::SoldierLibrary::open(&data_dir) {
            Ok(lib) if soldier::is_unit_key(&lib, first) => {
                let units: Vec<String> = queries.iter().filter(|q| soldier::is_unit_key(&lib, q)).cloned().collect();
                soldier::run(lib, units, screenshot_dir);
                return;
            }
            Ok(_) => {}
            Err(e) => eprintln!("Soldier data unavailable: {e}"),
        }
    }
    let mut models = Vec::new();
    for q in &queries {
        match source.find_model(q) {
            Some(p) => models.push(p),
            None => eprintln!("No .rigid_model matches {q:?} (try --list-models {q})"),
        }
    }
    if models.is_empty() {
        eprintln!("Nothing to show. Example: --view-model american_church");
        return;
    }
    if let Some(dir) = &screenshot_dir {
        let _ = std::fs::create_dir_all(dir);
    }
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "NapoleonRust model viewer".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.45, 0.55, 0.65)))
        .insert_resource(GlobalAmbientLight { brightness: 600.0, ..default() })
        .insert_non_send(SourceRes(source))
        .insert_resource(Viewer { models, current: 0, loaded: None, frames_since_load: 0, secs_since_load: 0.0, screenshot_dir, shot_taken: false })
        .add_systems(Startup, setup)
        .add_systems(Update, (switch_model, load_model, orbit_camera, draw_bbox, auto_screenshot).chain())
        .run();
}

/// The install + lookup tables. Not `Send`-required, so it is a non-send resource.
struct SourceRes(ModelSource);

#[derive(Resource)]
struct Viewer {
    models: Vec<String>,
    current: usize,
    /// Index of the model currently in the scene, if any.
    loaded: Option<usize>,
    frames_since_load: u32,
    /// Seconds since the current model was loaded (for --screenshot timing).
    secs_since_load: f32,
    screenshot_dir: Option<PathBuf>,
    shot_taken: bool,
}

/// Marks entities that belong to the shown model (removed when switching).
#[derive(Component)]
struct ModelPart;

/// The model's bounding box (Bevy coordinates), for framing and the debug box.
#[derive(Resource, Default)]
struct ModelBounds {
    min: Vec3,
    max: Vec3,
    show: bool,
}

/// Orbit camera state: it looks at `focus` from `distance` away.
#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

fn setup(mut commands: Commands) {
    commands.insert_resource(ModelBounds::default());
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(10.0, 10.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera { focus: Vec3::ZERO, distance: 20.0, yaw: 0.8, pitch: 0.45 },
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn switch_model(keys: Res<ButtonInput<KeyCode>>, mut viewer: ResMut<Viewer>, mut bounds: ResMut<ModelBounds>, mut exit: MessageWriter<AppExit>) {
    let n = viewer.models.len();
    if keys.just_pressed(KeyCode::KeyN) {
        viewer.current = (viewer.current + 1) % n;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        viewer.current = (viewer.current + n - 1) % n;
    }
    if keys.just_pressed(KeyCode::KeyW) {
        bounds.show = !bounds.show;
    }
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

#[allow(clippy::too_many_arguments)]
fn load_model(
    mut commands: Commands,
    source: NonSend<SourceRes>,
    mut viewer: ResMut<Viewer>,
    mut bounds: ResMut<ModelBounds>,
    old: Query<Entity, With<ModelPart>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut camera: Query<&mut OrbitCamera>,
    mut window: Query<&mut Window>,
) {
    if viewer.loaded == Some(viewer.current) {
        viewer.frames_since_load += 1;
        return;
    }
    for e in &old {
        commands.entity(e).despawn();
    }
    let index = viewer.current;
    viewer.loaded = Some(index);
    viewer.frames_since_load = 0;
    viewer.secs_since_load = 0.0;
    viewer.shot_taken = false;
    let path = viewer.models[index].clone();
    let src = &source.0;
    let model = match src.vfs.read(&path).map_err(|e| e.to_string()).and_then(|b| RigidModel::read(&b).map_err(|e| e.to_string())) {
        Ok(m) => m,
        Err(e) => {
            error!("{path}: {e}");
            return;
        }
    };
    let flags = NameFlags::from_path(&path);
    info!(
        "[{}/{}] {path}: {} meshes, {} vertices, {} triangles, texture folder from DB: {:?}",
        index + 1,
        viewer.models.len(),
        model.meshes.len(),
        model.vertex_count(),
        model.index_count() / 3,
        src.db_texture_folder(&path)
    );
    for mut w in &mut window {
        w.title = format!("NapoleonRust model viewer - {path}");
    }
    let mut texture_cache: Vec<(String, Handle<Image>)> = Vec::new();
    for (i, rm) in model.meshes.iter().enumerate() {
        let arrays = convert_mesh(rm);
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, arrays.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, arrays.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, arrays.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, arrays.tangents);
        mesh.insert_indices(Indices::U32(arrays.indices));

        let mut material = StandardMaterial {
            perceptual_roughness: 0.85,
            reflectance: 0.2,
            double_sided: flags.two_sided,
            cull_mode: if flags.two_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
            alpha_mode: if flags.alpha_blend {
                AlphaMode::Blend
            } else if flags.alpha_test {
                AlphaMode::Mask(0.5)
            } else {
                AlphaMode::Opaque
            },
            ..default()
        };
        let diffuse = rm.material.diffuse_name();
        match diffuse.as_deref().and_then(|n| src.find_texture(&path, n)) {
            Some(hit) => {
                let tex_path = hit.path().to_owned();
                let handle = match texture_cache.iter().find(|(p, _)| *p == tex_path) {
                    Some((_, h)) => Some(h.clone()),
                    None => match load_dds(src, &tex_path) {
                        Ok(img) => {
                            let h = images.add(img);
                            texture_cache.push((tex_path.clone(), h.clone()));
                            Some(h)
                        }
                        Err(e) => {
                            warn!("  mesh {i}: {tex_path}: {e}");
                            None
                        }
                    },
                };
                info!("  mesh {i}: v{} {} -> {:?}", rm.version, describe_material(&rm.material), hit);
                material.base_color_texture = handle;
            }
            None => {
                warn!("  mesh {i}: diffuse texture {diffuse:?} not found; drawing it grey");
                material.base_color = Color::srgb(0.6, 0.6, 0.6);
            }
        }
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(materials.add(material)), ModelPart));
    }

    // Frame the model: flip the box like the vertices.
    let (a, b) = (model.bbox_min, model.bbox_max);
    bounds.min = Vec3::new(a[0], a[1], -b[2]);
    bounds.max = Vec3::new(b[0], b[1], -a[2]);
    let size = (bounds.max - bounds.min).length().max(0.5);
    for mut cam in &mut camera {
        cam.focus = (bounds.min + bounds.max) * 0.5;
        cam.distance = size * 1.1;
        cam.yaw = 0.8;
        cam.pitch = 0.4;
    }
    // A ground plane just under the model so the shadows have something to land on.
    let ground = meshes.add(Plane3d::default().mesh().size(size * 3.0, size * 3.0));
    commands.spawn((
        Mesh3d(ground),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.32, 0.38, 0.25), perceptual_roughness: 1.0, ..default() })),
        Transform::from_xyz(cam_center_x(&bounds), bounds.min.y - 0.01, cam_center_z(&bounds)),
        ModelPart,
    ));
}

fn cam_center_x(b: &ModelBounds) -> f32 {
    (b.min.x + b.max.x) * 0.5
}

fn cam_center_z(b: &ModelBounds) -> f32 {
    (b.min.z + b.max.z) * 0.5
}

/// Reads a `.dds` from the packs and decodes every mip level to RGBA8 (sRGB colour).
pub(crate) fn load_dds(src: &ModelSource, path: &str) -> Result<Image, String> {
    let bytes = src.vfs.read(path).map_err(|e| e.to_string())?;
    let dds = Dds::parse(&bytes).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    for level in 0..dds.mip_count {
        data.extend(dds.decode_rgba8(level));
    }
    let size = Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 };
    let first = dds.decode_rgba8(0);
    let mut image = Image::new(size, TextureDimension::D2, first, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = dds.mip_count;
    // UVs outside 0..1 tile in the original (D3D wrap addressing).
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    Ok(image)
}

fn orbit_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut cams: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    for (mut cam, mut tf) in &mut cams {
        let d = motion.delta;
        if mouse.pressed(MouseButton::Left) {
            cam.yaw -= d.x * 0.005;
            cam.pitch = (cam.pitch + d.y * 0.005).clamp(-1.5, 1.5);
        }
        if mouse.pressed(MouseButton::Right) {
            let right = tf.right();
            let up = tf.up();
            let scale = cam.distance * 0.0015;
            cam.focus += (-right * d.x + up * d.y) * scale;
        }
        if scroll.delta.y != 0.0 {
            cam.distance = (cam.distance * (1.0 - scroll.delta.y * 0.1)).clamp(0.2, 5_000.0);
        }
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        tf.translation = cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance);
        tf.look_at(cam.focus, Vec3::Y);
    }
}

fn draw_bbox(bounds: Res<ModelBounds>, mut gizmos: Gizmos) {
    if bounds.show {
        let center = (bounds.min + bounds.max) * 0.5;
        let size = bounds.max - bounds.min;
        gizmos.cube(Transform::from_translation(center).with_scale(size), Color::srgb(1.0, 0.9, 0.2));
    }
}

/// With `--screenshot`, waits a moment after each load, saves a PNG, moves on, and quits at the end.
fn auto_screenshot(mut commands: Commands, time: Res<Time>, mut viewer: ResMut<Viewer>, mut exit: MessageWriter<AppExit>) {
    let Some(dir) = viewer.screenshot_dir.clone() else { return };
    if viewer.loaded != Some(viewer.current) {
        return;
    }
    viewer.secs_since_load += time.delta_secs();
    // Bevy compiles shaders in the background, so the first frames can be empty:
    // wait 4 s for the first model and 1.5 s for the others.
    let wait = if viewer.current == 0 { 4.0 } else { 1.5 };
    if !viewer.shot_taken && viewer.frames_since_load > 5 && viewer.secs_since_load >= wait {
        let name = viewer.models[viewer.current].rsplit('\\').next().unwrap_or("model").replace(".rigid_model", ".png");
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join(name)));
        viewer.shot_taken = true;
    }
    if viewer.shot_taken && viewer.secs_since_load >= wait + 0.75 {
        if viewer.current + 1 < viewer.models.len() {
            viewer.current += 1;
        } else {
            exit.write(AppExit::Success);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn parses_command_line() {
        assert_eq!(ViewerArgs::parse(&args(&[])), None);
        assert_eq!(ViewerArgs::parse(&args(&["--list-models", "church"])), Some(ViewerArgs::List("church".into())));
        assert_eq!(
            ViewerArgs::parse(&args(&["--view-model", "a", "b", "--screenshot", "out"])),
            Some(ViewerArgs::View { models: vec!["a".into(), "b".into()], screenshot_dir: Some("out".into()) })
        );
    }
}

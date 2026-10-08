//! The battle terrain: reads a preset battle map from the install (`ntw_formats::battle_terrain`)
//! and draws it in the 3D battle view; also answers "how high is the ground here?" for the
//! soldiers, the camera and mouse picking.
//!
//! - `--battle-map <name>` picks the preset (folder under `battleterrain\presets\`, e.g.
//!   `hb_waterloo`, `nap_mp_prussian_hills`). Default: [`DEFAULT_MAP`]. `--battle-map list`
//!   prints the names. If the map cannot be read, the battle falls back to the flat plane.
//! - Coordinates: an `ntw_sim` battle position `(x, y)` in metres IS a map position (both are
//!   metres from the map centre), and Bevy draws it at `(x, height_at(x, y), -y)` (the same axes
//!   as `battle::view::world_of`). See `ntw_formats::battle_terrain` for the grid layout.
//! - Drawing: level 0 (the 2,048 m playable area) at full resolution (1025² samples, 2 m apart);
//!   levels 1..3 (4, 8, 16 km, the distant landscape) as rings at 1/4 resolution with a hole
//!   where the inner level is. Each level uses its own `colour_map_N.jpg` (+`_alpha`) and the
//!   map's tiled ground texture (`textures.xml tiled_detail_map`, a DDS) in `terrain.wgsl`.
//!
//! PROVISIONAL (to be replaced by the original's terrain renderer once found in the exe):
//! the lighting (plain sun + ambient from the map's `.environment`), no detail/blend/cliff/rock
//! maps, no grass, no lightmap, no fog, and the seams between levels.

pub mod objects;
mod speedtree;
mod trees;
pub mod water;

use std::sync::{Arc, RwLock};

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::image::{CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor, ImageType};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use ntw_formats::battle_terrain::{self, BattleMap, Heightfield, texture_dds_path};
use ntw_formats::dds::Dds;
use ntw_formats::pack::Vfs;

/// The map used when no `--battle-map` is given: Austerlitz suits the France vs Austria slice.
pub const DEFAULT_MAP: &str = "hb_austerlitz";

/// Resolution divisor of the distant levels 1..3.
const FAR_STRIDE: u32 = 4;

/// The loaded map, shared with code that only needs heights. Set at start-up (`--battle`) or when
/// the front end starts a battle ([`load_battle_map`]).
static GROUND: RwLock<Option<Arc<BattleMap>>> = RwLock::new(None);

/// The map in use, if one loaded.
pub fn current_map() -> Option<Arc<BattleMap>> {
    GROUND.read().ok().and_then(|g| g.clone())
}

/// True if a battle map is loaded (the flat placeholder plane is then not needed).
pub fn loaded() -> bool {
    current_map().is_some()
}

/// Ground height in metres at battlefield position `p` (= `ntw_sim` position, metres from the
/// map centre); 0 when no map is loaded. On a naval map this is the **sea bed**, 100 m down.
pub fn ground_height(p: Vec2) -> f32 {
    current_map().map_or(0.0, |m| m.height_at(p.x, p.y))
}

/// The height of the surface you see at `p`: the ground, or the sea surface where the ground lies
/// below [`battle_terrain::SEA_LEVEL`]. The battle camera follows this, not the bare ground, or on a
/// naval map (`hb_nile`, `hb_trafalgar`, `caribbean`, `hb_naval`, all a flat −100 m bed) it would
/// spend the whole battle 100 m under water looking at the sand.
pub fn surface_height(p: Vec2) -> f32 {
    let ground = ground_height(p);
    match current_map() {
        Some(m) if ground < battle_terrain::SEA_LEVEL && m.sea_coverage() > 0.0 => battle_terrain::SEA_LEVEL,
        _ => ground,
    }
}

/// Battlefield metres → Bevy world position on the ground.
pub fn ground_point(p: Vec2) -> Vec3 {
    Vec3::new(p.x, ground_height(p), -p.y)
}

/// Where a ray (e.g. from the mouse cursor) first hits the terrain, by marching along the ray
/// and refining by bisection. Falls back to the plane y = 0 when no map is loaded.
pub fn pick_ground(ray: Ray3d) -> Option<Vec3> {
    if !loaded() {
        let t = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
        return Some(ray.get_point(t));
    }
    let above = |t: f32| {
        let p = ray.get_point(t);
        p.y - ground_height(Vec2::new(p.x, -p.z))
    };
    if above(0.0) < 0.0 {
        return None;
    }
    let (mut t0, step) = (0.0f32, 2.0f32);
    while t0 < 20_000.0 {
        let t1 = t0 + step;
        if above(t1) < 0.0 {
            let (mut lo, mut hi) = (t0, t1);
            for _ in 0..24 {
                let mid = 0.5 * (lo + hi);
                if above(mid) < 0.0 { hi = mid } else { lo = mid }
            }
            return Some(ray.get_point(hi));
        }
        t0 = t1;
    }
    None
}

/// Reads `--battle-map <name>` from the command line. `None` = not given.
pub fn map_arg(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a == "--battle-map")?;
    args.get(i + 1).cloned()
}

/// Draws the battle map and provides heights.
pub struct TerrainPlugin {
    /// Preset folder name.
    pub map: String,
    /// Load `map` while building the app (the `--battle` start). Otherwise a map is loaded when
    /// the front end starts a battle ([`load_battle_map`]).
    pub load_at_start: bool,
}

impl TerrainPlugin {
    /// Builds the plugin from the command line (`--battle-map <name>`, default [`DEFAULT_MAP`]).
    /// `--battle-map list` prints the preset names and exits.
    pub fn from_args(args: &[String]) -> Self {
        let map = map_arg(args).unwrap_or_else(|| DEFAULT_MAP.to_owned());
        if map == "list" {
            match Vfs::open_install(crate::config::game_data_dir()) {
                Ok(vfs) => battle_terrain::list_presets(&vfs).iter().for_each(|n| println!("{n}")),
                Err(e) => eprintln!("cannot open the install: {e}"),
            }
            std::process::exit(0);
        }
        Self { map, load_at_start: true }
    }
}

/// The open packs, kept for loading textures and objects when the battle starts.
#[derive(Resource)]
pub(crate) struct TerrainVfs(pub(crate) Arc<Vfs>);

/// Reads a battle map (preset folder name, e.g. `hb_arcole`) and makes it the current map.
/// Returns the packs on success.
fn read_map(name: &str) -> Result<Vfs, String> {
    let dir = crate::config::game_data_dir();
    let vfs = Vfs::open_install(&dir).map_err(|e| e.to_string())?;
    let map = BattleMap::load(&vfs, name).map_err(|e| e.to_string())?;
    info!(
        "Battle map {}: {} heightfields, {} buildings, {} tree lists",
        map.name,
        map.heightfields.len(),
        map.buildings_near.len() + map.buildings_far.len(),
        map.trees.len()
    );
    if let Ok(mut g) = GROUND.write() {
        *g = Some(Arc::new(map));
    }
    Ok(vfs)
}

/// Loads `name` as the battle map before the game switches to [`crate::GameMode::Battle`] (used by
/// the front end's battle start, the same maps `--battle-map` takes). On failure the battle uses
/// the flat ground.
pub fn load_battle_map(world: &mut World, name: &str) {
    match read_map(name) {
        Ok(vfs) => {
            world.insert_resource(TerrainVfs(Arc::new(vfs)));
        }
        Err(e) => {
            warn!("Battle map '{name}' not loaded ({e}); using the flat ground");
            if let Ok(mut g) = GROUND.write() {
                *g = None;
            }
        }
    }
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain.wgsl");
        embedded_asset!(app, "trees.wgsl");
        embedded_asset!(app, "speedtree.wgsl");
        embedded_asset!(app, "water.wgsl");
        app.add_plugins((
            MaterialPlugin::<TerrainMaterial>::default(),
            MaterialPlugin::<trees::TreeMaterial>::default(),
            MaterialPlugin::<speedtree::TreeGeomMaterial>::default(),
            MaterialPlugin::<water::SeaMaterial>::default(),
        ))
            .add_systems(Update, extend_camera_far_plane)
            .add_systems(
                OnEnter(crate::GameMode::Battle),
                (
                    spawn_terrain,
                    water::spawn_sea,
                    objects::spawn_buildings,
                    trees::spawn_trees,
                    speedtree::setup_near_trees,
                    speedtree::setup_wind,
                ),
            )
            .add_systems(
                Update,
                (
                    speedtree::update_near_trees,
                    speedtree::update_wind,
                    water::update_sea,
                )
                    .run_if(in_state(crate::GameMode::Battle)),
            )
            .add_systems(OnExit(crate::GameMode::Battle), speedtree::clear_near_trees);
        if !self.load_at_start {
            return;
        }
        match read_map(&self.map) {
            Ok(vfs) => {
                if let Some(sky) = current_map().and_then(|m| m.lighting.as_ref().and_then(|l| l.sky_colour)) {
                    app.insert_resource(ClearColor(Color::srgb(sky[0], sky[1], sky[2])));
                }
                app.insert_resource(TerrainVfs(Arc::new(vfs)));
            }
            Err(e) => warn!("Battle map '{}' not loaded ({e}); using the flat ground", self.map),
        }
    }
}

/// The terrain material (see `terrain.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TerrainMaterial {
    #[uniform(0)]
    params: TerrainParams,
    #[texture(1)]
    #[sampler(2)]
    colour_map: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    tile_map: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct TerrainParams {
    sun_dir: Vec4,
    sun_colour: Vec4,
    ambient: Vec4,
    tile: Vec4,
}

impl Material for TerrainMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/terrain/terrain.wgsl".into()
    }
}

/// The default camera stops drawing at 1 km; the map's distant rings reach 8 km.
fn extend_camera_far_plane(mut cams: Query<&mut Projection, Added<Camera3d>>) {
    for mut p in &mut cams {
        if let Projection::Perspective(persp) = &mut *p {
            persp.far = 20_000.0;
        }
    }
}

/// Builds `Image` RGBA8 (UNORM: values are used as stored, like the original's
/// `SRGBTEXTURE = false`) with a full box-filtered mip chain.
fn image_with_mips(width: u32, height: u32, rgba: Vec<u8>, repeat: bool) -> Image {
    let mut data = rgba.clone();
    let (mut w, mut h, mut level) = (width as usize, height as usize, rgba);
    let mut mips = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for ch in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((2 * x + dx).min(w - 1), (2 * y + dy).min(h - 1));
                        s += level[(sy * w + sx) * 4 + ch] as u32;
                    }
                    next[(y * nw + x) * 4 + ch] = (s / 4) as u8;
                }
            }
        }
        data.extend_from_slice(&next);
        (w, h, level) = (nw, nh, next);
        mips += 1;
    }
    let size = Extent3d { width, height, depth_or_array_layers: 1 };
    let first = data[..(width * height * 4) as usize].to_vec();
    let mut image = Image::new(size, TextureDimension::D2, first, TextureFormat::Rgba8Unorm, RenderAssetUsages::default());
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mips;
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Decodes a JPEG from the packs to (width, height, pixels as RGBA8 / as luminance).
fn read_jpeg(vfs: &Vfs, path: &str) -> Result<(u32, u32, Vec<u8>), String> {
    let bytes = vfs.read(path).map_err(|e| e.to_string())?;
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension("jpg"),
        CompressedImageFormats::NONE,
        false,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .map_err(|e| format!("{path}: {e}"))?;
    // Bevy's JPEG loader returns RGBA8 (UNORM since `is_srgb` is false).
    let (w, h) = (image.width(), image.height());
    let data = image.data.ok_or_else(|| format!("{path}: no pixels"))?;
    if data.len() != (w * h * 4) as usize {
        return Err(format!("{path}: unexpected pixel format {:?}", image.texture_descriptor.format));
    }
    Ok((w, h, data))
}

/// `colour_map_<level>.jpg` with the red channel of `colour_map_<level>_alpha.jpg` as alpha
/// (INFERRED: the `_alpha` JPEG is greyscale alpha, since JPEG has no alpha channel).
fn colour_map(vfs: &Vfs, map: &BattleMap, level: usize) -> Option<Image> {
    let path = map.colour_map_path(level, false)?;
    let (w, h, mut rgba) = read_jpeg(vfs, &path).map_err(|e| warn!("{e}")).ok()?;
    match map.colour_map_path(level, true).map(|p| read_jpeg(vfs, &p)) {
        Some(Ok((aw, ah, alpha))) if (aw, ah) == (w, h) => {
            for (px, a) in rgba.chunks_exact_mut(4).zip(alpha.chunks_exact(4)) {
                px[3] = a[0];
            }
        }
        _ => rgba.chunks_exact_mut(4).for_each(|px| px[3] = 0),
    }
    Some(image_with_mips(w, h, rgba, false))
}

/// The tiled ground texture (`tiled_detail_map`), every DDS mip level decoded.
fn tile_map(vfs: &Vfs, map: &BattleMap) -> Option<Image> {
    let name = &map.textures.as_ref()?.tiled_detail_map;
    let path = texture_dds_path(name);
    let bytes = vfs.read(&path).map_err(|e| warn!("{path}: {e}")).ok()?;
    let dds = Dds::parse(&bytes).map_err(|e| warn!("{path}: {e}")).ok()?;
    let mut image = image_with_mips(dds.width, dds.height, dds.decode_rgba8(0), true);
    // Replace the box-filtered chain with the file's own mips.
    if dds.mip_count > 1 {
        let mut data = Vec::new();
        for level in 0..dds.mip_count {
            data.extend(dds.decode_rgba8(level));
        }
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = dds.mip_count;
    }
    Some(image)
}

/// The triangle mesh of one heightfield level. `stride` skips samples; cells that lie wholly
/// inside `hole` (half-size in metres, the inner level) are left out.
fn level_mesh(hf: &Heightfield, stride: u32, hole: f32) -> Mesh {
    let cols: Vec<u32> = (0..hf.width).step_by(stride as usize).collect();
    let rows: Vec<u32> = (0..hf.height).step_by(stride as usize).collect();
    let (dx, dy) = hf.spacing();
    let mut positions = Vec::with_capacity(cols.len() * rows.len());
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for &r in &rows {
        for &c in &cols {
            let (x, y) = hf.sample_position(c, r);
            let (ci, ri) = (c as i64, r as i64);
            let h = hf.sample_m(ci, ri);
            // Slopes in map space: dh/dx along columns, dh/dy against rows (row 0 = +y).
            let gx = (hf.sample_m(ci + 1, ri) - hf.sample_m(ci - 1, ri)) / (2.0 * dx);
            let gy = (hf.sample_m(ci, ri - 1) - hf.sample_m(ci, ri + 1)) / (2.0 * dy);
            positions.push([x, h, -y]);
            // Map normal (-gx, -gy, 1) with Bevy axes (x, up, -y).
            normals.push(Vec3::new(-gx, 1.0, gy).normalize().to_array());
            uvs.push([c as f32 / (hf.width - 1) as f32, r as f32 / (hf.height - 1) as f32]);
        }
    }
    let n = cols.len() as u32;
    let mut indices = Vec::new();
    for j in 0..rows.len() as u32 - 1 {
        for i in 0..n - 1 {
            if hole > 0.0 {
                let p0 = positions[(j * n + i) as usize];
                let p1 = positions[((j + 1) * n + i + 1) as usize];
                let inside = |a: f32, b: f32| a.min(b) >= -hole - 0.01 && a.max(b) <= hole + 0.01;
                if inside(p0[0], p1[0]) && inside(p0[2], p1[2]) {
                    continue;
                }
            }
            let (v00, v01, v10, v11) = (j * n + i, j * n + i + 1, (j + 1) * n + i, (j + 1) * n + i + 1);
            // Counter-clockwise seen from above (+Y) = front face in Bevy.
            indices.extend_from_slice(&[v00, v10, v01, v01, v10, v11]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Sun direction and colours from the map's `.environment` (PROVISIONAL reading of
/// `light_direction='euler(a,b,c)'` as yaw a, pitch b of the direction the light travels, in the
/// game's left-handed axes; the colour scales are tuned by eye).
fn lighting_params(map: &BattleMap) -> (Vec3, Vec3, Vec3) {
    let Some(l) = &map.lighting else {
        return (Vec3::new(0.4, 0.8, 0.3).normalize(), Vec3::splat(0.8), Vec3::splat(0.45));
    };
    let [yaw, pitch, _] = l.light_direction_euler;
    let travel_d3d = Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos());
    let mut to_sun = -Vec3::new(travel_d3d.x, travel_d3d.y, -travel_d3d.z);
    if to_sun.y < 0.1 {
        to_sun.y = 0.1;
    }
    let sun = Vec3::from(l.light_colour) * l.light_colour_scale * 0.55;
    let ambient = Vec3::from(l.ambient_top) * l.ambient_cube_scale * 0.45;
    (to_sun.normalize(), sun, ambient)
}

/// Startup: one mesh + material per heightfield level.
fn spawn_terrain(
    mut commands: Commands,
    vfs: Option<Res<TerrainVfs>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    let (Some(map), Some(vfs)) = (current_map(), vfs) else { return };
    let map = &map;
    let (sun_dir, sun, ambient) = lighting_params(map);
    let tile = match tile_map(&vfs.0, map) {
        Some(img) => images.add(img),
        // Neutral grey tile (alpha 0.6 = no change in the second blend) if the map names none.
        None => images.add(image_with_mips(1, 1, vec![128, 128, 128, 153], true)),
    };
    let mut hole = 0.0;
    for (level, hf) in map.heightfields.iter().enumerate() {
        let colour = colour_map(&vfs.0, map, level)
            .unwrap_or_else(|| image_with_mips(1, 1, vec![90, 110, 70, 0], false));
        let stride = if level == 0 { 1 } else { FAR_STRIDE };
        let material = TerrainMaterial {
            params: TerrainParams {
                sun_dir: sun_dir.extend(0.0),
                sun_colour: sun.extend(1.0),
                ambient: ambient.extend(1.0),
                // `tiled_texcoord` = 8 · (0.5 + xz / 2048): 8 repeats per 2,048 m.
                tile: Vec4::new(8.0 * hf.settings.world_width / 2048.0, 0.0, 0.0, 0.0),
            },
            colour_map: images.add(colour),
            tile_map: tile.clone(),
        };
        commands.spawn((
            Mesh3d(meshes.add(level_mesh(hf, stride, hole))),
            MeshMaterial3d(materials.add(material)),
            Transform::default(),
            Name::new(format!("terrain level {level}")),
        ));
        hole = hf.settings.world_width / 2.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Research check (needs the install): is `colour_map_0.jpg` laid out like the ground-type
    /// map (whose layout is confirmed)? For each of 4 flips, the colour variance inside each
    /// ground type is computed; the right layout makes the types most uniform (lowest variance).
    #[test]
    #[ignore]
    fn colour_map_layout_matches_ground_types() {
        let vfs = Vfs::open_install(crate::config::game_data_dir()).expect("install");
        // Maps with strong colour differences between ground types (forts are too uniform to tell).
        for name in ["hb_waterloo", "nap_mp_mountain"] {
            let map = BattleMap::load(&vfs, name).unwrap();
            let gt = map.ground_types.as_ref().unwrap();
            let (w, h, rgba) = read_jpeg(&vfs, &map.colour_map_path(0, false).unwrap()).unwrap();
            let mut scores = Vec::new();
            for flip in 0..4u8 {
                let mut sum = [[0f64; 3]; 256];
                let mut sq = [0f64; 256];
                let mut n = [0f64; 256];
                for r in 0..gt.height {
                    for c in 0..gt.width {
                        let t = gt.cells[(r * gt.width + c) as usize] as usize;
                        let mut u = (c as f32 + 0.5) / gt.width as f32;
                        let mut v = (r as f32 + 0.5) / gt.height as f32;
                        if flip & 1 != 0 { u = 1.0 - u; }
                        if flip & 2 != 0 { v = 1.0 - v; }
                        let (x, y) = ((u * w as f32) as usize, (v * h as f32) as usize);
                        let p = &rgba[(y * w as usize + x) * 4..][..3];
                        for k in 0..3 {
                            sum[t][k] += p[k] as f64;
                            sq[t] += (p[k] as f64).powi(2);
                        }
                        n[t] += 1.0;
                    }
                }
                let mut var = 0.0;
                let total: f64 = n.iter().sum();
                for t in 0..256 {
                    if n[t] > 0.0 {
                        let m2: f64 = sum[t].iter().map(|s| (s / n[t]).powi(2)).sum();
                        var += (sq[t] / n[t] - m2) * n[t] / total;
                    }
                }
                scores.push(var);
            }
            println!("{name}: within-type colour variance [none, flip u, flip v, both] = {scores:.0?}");
            let best = scores.iter().enumerate().min_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
            assert_eq!(best, 0, "{name}: colour map not laid out like the ground-type map");
        }
    }
}

//! The battle map's sea: the surface itself, drawn at the sea level over the ground that lies
//! below it. `fx\ocean.fx` (shipped as HLSL text in the install's `fx\` folder) is the original's
//! sea shader; `water.wgsl` is our own port of its non-ultra-sea maths. Research notes and every
//! exe address: `analysis/graphics/WATER.md`.
//!
//! - **Where the water is** (`ntw_formats::battle_terrain::SEA_LEVEL`): the sea surface is at y = 0
//!   in map metres, CONFIRMED on all 60 presets; a position is sea when the ground there is below
//!   that. The original draws one plane and lets the terrain occlude it, so this module does the
//!   same: no mask, no stencil.
//! - **The mesh** is a camera-centred set of concentric square rings with geometrically growing
//!   cell size (ours; the entity follows the camera, so the mesh is built once). The original
//!   builds its own grid and displaces it on the CPU with the containers' height and displacement
//!   images (`0x01123820`); we draw it flat, **PROVISIONAL** (`WATER.md` §3).
//! - **The wave textures** are the install's `sea\sea` (the chop) and `sea\swell` (the swell),
//!   decoded by `ntw_formats::sea_waves` (CONFIRMED container, loader `0x010D25C0`). The original
//!   rebuilds one texture per frame from two neighbouring frames (`0x010E21F0`); we upload every
//!   frame once as a texture array and blend the same two frames in the shader, which gives the
//!   same texels (the blend and the dequantisation are both linear). Mip levels of each frame are
//!   a box filter of ours (**PROVISIONAL**: the original's mip source is not traced).
//! - **The constants** come from where the exe takes them (per-draw setter `0x0119E830`): the
//!   battle's `sea_surfaces` row (froth, shininess, decay; key from the battle XML, CONFIRMED
//!   parser `0x00511210`) and its climate's `sea_climate_details` row (the deep colour), both read
//!   through the merged database; `g_fresnel_R0` is the exe's own constant.
//! - **`sea_attenuation`**, the per-vertex attribute that damps the short chop near the shore, comes
//!   from a sea-bed depth texture of the level-0 heightfield (INFERRED: the original's own source
//!   for the attribute is not known).
//! - **Not here**: the sky reflection cube (the map's `SKYGEN` sky model stands in, see
//!   `SkyModel`), shoreline foam passes, planar reflections, refraction, rain ripples, wakes,
//!   fog and HDR post-processing, and inland water at its own level. All listed in `WATER.md`.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::RenderDevice;
use bevy::shader::ShaderRef;
use ntw_data::{DbRecord, SeaClimateDetail, SeaSurface, Table};
use ntw_formats::battle_terrain::{BattleMap, SEA_LEVEL, Skygen};
use ntw_formats::pack::Vfs;
use ntw_formats::sea_waves::SeaWaves;

use super::{TerrainVfs, lighting_params};

/// The cell size of the innermost ring, metres (ours).
const FIRST_CELL: f32 = 0.5 * 4.0;
/// How many doublings of the cell size. 13 rings from 2 m reach a 16,384 m cell, so the outermost
/// ring is far past the camera's 20 km far plane and the sea always runs to the horizon.
const RINGS: u32 = 13;
/// Cells along one side of a ring's outer edge. Each ring is a `2 * EDGE_CELLS + 1` vertex square, so
/// the cost does not grow with the world: 13 rings × 33² vertices = 13,533 and 13 × 2·32² = 26,624
/// triangles.
const EDGE_CELLS: u32 = 16;
/// Sea-bed depth (metres) at which the short chop reaches full strength. PROVISIONAL: ours, with
/// the INFERRED depth source of `sea_attenuation` (module comment); the original's value is UNKNOWN.
const FULL_CHOP_DEPTH: f32 = 12.0;
/// Texels per side of the sea-bed depth texture (ours, a resolution choice; no original value).
const DEPTH_TEX: u32 = 512;
/// `g_fresnel_R0`: the float constant `0x3CA6D903` the per-draw setter `0x0119E830` writes in
/// place of `ocean.fx`'s own 0.5 (CONFIRMED, `WATER.md` §4).
const FRESNEL_R0: f32 = 0.020_37;
/// `ocean.fx`: `blinn_phong(..., g_sea_shininess * 0.7)` (CONFIRMED, shader text).
const SHININESS_TO_EXPONENT: f32 = 0.7;
/// `ocean.fx g_foam_uv_scale` (CONFIRMED, shader text; no setter writes it).
const FOAM_UV_SCALE: f32 = 15.0;
/// `ocean.fx MAX_FOAM_DIST`, metres of view distance (CONFIRMED, shader text).
const MAX_FOAM_DIST: f32 = 300.0;
/// Below this share of water the map is a land battle with a pond, not a sea. INFERRED: the naval
/// presets are 100 % water and `nap_mp_italian_grassland` has 0.01 %, so any small threshold
/// separates them.
const MIN_SEA_COVERAGE: f32 = 0.005;
/// The chop and swell containers and the foam texture in the install.
const SEA_PATH: &str = "sea\\sea";
const SWELL_PATH: &str = "sea\\swell";
const FOAM_PATH: &str = "sea\\combined_foam.tga";

/// Foam on or off. `ocean.fx` compiles the foam into the pixel shader but the engine picks
/// **per draw** between `sm_render_sm3` (no foam), `sm_render_foam_sm3` (foam) and the four
/// `sm_render_sea_stenciling_*` variants, which clip it to where the sea meets something
/// (CONFIRMED techniques, UNKNOWN which pass each region gets). One surface, one pass, so there is
/// nothing here to choose by: **PROVISIONAL**, foam off by default; `NAPOLEON_SEA_FOAM=1` draws the
/// foam pass over the whole surface.
fn foam_on() -> bool {
    std::env::var("NAPOLEON_SEA_FOAM").is_ok_and(|v| v == "1")
}

/// The sea surface material (see `water.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SeaMaterial {
    #[uniform(0)]
    params: SeaParams,
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    sea_surface: Handle<Image>,
    #[texture(3, dimension = "2d_array")]
    #[sampler(4)]
    swell_surface: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    foam: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    depth: Handle<Image>,
}

/// `water.wgsl`'s `WaterParams`; each field's layout is described there.
#[derive(ShaderType, Debug, Clone, Copy)]
struct SeaParams {
    sun_dir: Vec4,
    sun_colour: Vec4,
    sky: Vec4,
    circumsolar: Vec4,
    horizon: Vec4,
    horizon_opposite: Vec4,
    backscatter: Vec4,
    waves: Vec4,
    deep: Vec4,
    spec: Vec4,
    foam: Vec4,
    froth: Vec4,
    depth: Vec4,
    depth_rect: Vec4,
    sea_frames: Vec4,
    swell_frames: Vec4,
    sea_range: Vec4,
    swell_range: Vec4,
}

impl Material for SeaMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://napoleon/terrain/water.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/terrain/water.wgsl".into()
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

/// The sea surface of the battle in progress, so the per-frame system can follow the camera and
/// advance the animation. Inserted by [`spawn_sea`], removed by [`remove_sea`] when the battle ends.
#[derive(Resource)]
pub struct Sea {
    /// The mesh entity.
    pub entity: Entity,
    /// Its material.
    material: Handle<SeaMaterial>,
    /// The chop and swell animations.
    waves: Arc<WaveHeaders>,
}

/// The chop and swell containers' headers (`angles` emptied: the frames are on the GPU).
#[derive(Debug)]
struct WaveHeaders {
    sea: SeaWaves,
    swell: SeaWaves,
}

/// The install's chop, swell and foam textures, decoded and uploaded on the first sea battle and
/// reused by every later one. Invalidation: none, the install's files do not change while the game
/// runs (and the images are render-world only, so after the upload the main world keeps no copy).
/// Decoded on the main thread: the first sea battle needs them in the frame it starts, so a task
/// would only be waited on (21 ms in release, `WATER.md` §3).
#[derive(Resource, Clone)]
pub struct SeaTextures {
    waves: Arc<WaveHeaders>,
    sea: Handle<Image>,
    swell: Handle<Image>,
    foam: Handle<Image>,
}

impl SeaTextures {
    /// Decodes the three files; `max_layers` is the GPU's texture-array limit.
    fn load(vfs: &Vfs, max_layers: u32, images: &mut Assets<Image>) -> Self {
        let started = std::time::Instant::now();
        let (mut sea, mut swell) = (wave_container(vfs, SEA_PATH, max_layers), wave_container(vfs, SWELL_PATH, max_layers));
        let textures = Self {
            sea: images.add(angle_array(&sea)),
            swell: images.add(angle_array(&swell)),
            foam: images.add(foam_texture(vfs)),
            waves: {
                sea.angles = Vec::new();
                swell.angles = Vec::new();
                Arc::new(WaveHeaders { sea, swell })
            },
        };
        info!("Battle sea: wave and foam textures decoded in {:.0} ms", started.elapsed().as_secs_f64() * 1000.0);
        textures
    }
}

/// Ends the battle's sea: the per-frame system stops and the material (with the battle's depth
/// texture) is freed. The mesh entity goes with `DespawnOnExit`; the [`SeaTextures`] stay.
pub fn remove_sea(mut commands: Commands) {
    commands.remove_resource::<Sea>();
}

/// The sea's values from the database: the battle's `sea_surfaces` row and its climate's
/// `sea_climate_details` row, as the per-draw setter `0x0119E830` hands them to `ocean.fx`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SeaSettings {
    /// `g_sea_deep_colour`: the climate row's first colour.
    deep: Vec3,
    /// `g_froth_value`, `g_froth_distortion_speed`, `g_froth_distortion_amount`.
    froth: [f32; 3],
    /// `g_sea_shininess`.
    shininess: f32,
    /// `g_sea_decay`, per metre of view depth.
    decay: f32,
}

/// Picks the rows the exe's lookup `0x006AB800` picks: the row with the given key, or the first
/// row when the key is missing or not in the table (CONFIRMED fallback). `None` only when a table
/// is empty. Logs once per call (once per battle) when it falls back.
///
/// A battle with **no** sea surface key is a campaign battle: its XML names none, and the original
/// rolls a wind level for it instead (`0x00535B30`, called from `0x008C4E50`, four thresholds over
/// one random draw; `WATER.md` §4). That roll is not ported, so such a battle gets the first row
/// (`wind_level_0`): **PROVISIONAL**. Where a battle without a climate key takes its climate from is
/// not traced (UNKNOWN); every shipped `sea_climate_details` row has the same deep colour.
fn sea_settings(
    surfaces: &Table<SeaSurface>,
    climates: &Table<SeaClimateDetail>,
    surface_key: Option<&str>,
    climate_key: Option<&str>,
) -> Option<SeaSettings> {
    fn pick<'a, T: DbRecord>(table: &'a Table<T>, key: Option<&str>, what: &str, unnamed: &str) -> Option<&'a T> {
        let first = table.rows().first()?;
        Some(match key.map(|k| (k, table.get(k))) {
            Some((_, Some(row))) => row,
            Some((k, None)) => {
                warn!("Battle sea: {what} `{k}` is not in the table, using its first row (as the original's lookup 0x006AB800)");
                first
            }
            None => {
                info!("Battle sea: no {what} named ({unnamed}), using the table's first row");
                first
            }
        })
    }
    let surface = pick(
        surfaces,
        surface_key,
        "sea surface",
        "a campaign battle: the original rolls its wind level at 0x00535B30, not ported, PROVISIONAL",
    )?;
    let climate = pick(climates, climate_key, "sea climate", "the original's source for it is not traced")?;
    Some(SeaSettings {
        deep: Vec3::new(climate.deep_r, climate.deep_g, climate.deep_b),
        froth: [surface.froth_value, surface.froth_distortion_speed, surface.froth_distortion_amount],
        shininess: surface.shininess,
        decay: surface.decay,
    })
}

/// Spawns the sea surface for the loaded battle map. Does nothing when the map has no water (the
/// common case: most historical battle maps lie entirely above the sea level).
#[allow(clippy::too_many_arguments)]
pub fn spawn_sea(
    mut commands: Commands,
    vfs: Option<Res<TerrainVfs>>,
    data: Option<Res<crate::data::GameData>>,
    setup: Option<Res<crate::battle::setup::SetupData>>,
    cache: Option<Res<SeaTextures>>,
    device: Option<Res<RenderDevice>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let (Some(map), Some(vfs)) = (super::current_map(), vfs) else { return };
    let coverage = map.sea_coverage();
    if coverage <= MIN_SEA_COVERAGE {
        return;
    }
    let spec = setup.as_ref().and_then(|s| s.battle.as_ref()).map(|(_, spec)| spec.clone());
    let settings = data.as_ref().and_then(|d| {
        sea_settings(
            &d.db.sea_surfaces,
            &d.db.sea_climate_details,
            spec.as_ref().and_then(|s| s.sea_surface.as_deref()),
            spec.as_ref().and_then(|s| s.climate.as_deref()),
        )
    });
    let Some(settings) = settings else {
        warn!("Battle sea: no sea_surfaces / sea_climate_details rows in the database, the sea is not drawn");
        return;
    };
    info!("Battle sea: {:.2}% of {} is below the sea level, drawing the surface ({settings:?})", 100.0 * coverage, map.name);
    let (sun_dir, sun, ambient) = lighting_params(&map);
    let sky = SkyModel::of(&map, ambient);
    let textures = match cache {
        Some(t) => t.clone(),
        None => {
            // wgpu's default when there is no device (nothing is drawn then anyway).
            let max_layers = device.map_or(256, |d| d.limits().max_texture_array_layers);
            let t = SeaTextures::load(&vfs.0, max_layers, &mut images);
            commands.insert_resource(t.clone());
            t
        }
    };
    let (sea, swell) = (&textures.waves.sea, &textures.waves.swell);
    let range = |w: &SeaWaves| Vec4::new(w.lo[0] as f32, w.lo[1] as f32, w.hi[0] as f32, w.hi[1] as f32) / 65535.0;
    let material = materials.add(SeaMaterial {
        params: SeaParams {
            sun_dir: sun_dir.extend(0.0),
            sun_colour: sun.extend(1.0),
            sky: sky.sky.extend(sky.circumsolar_decay),
            circumsolar: sky.circumsolar.extend(sky.horizon_decay),
            horizon: sky.horizon.extend(sky.backscatter_decay),
            horizon_opposite: sky.horizon_opposite.extend(0.0),
            backscatter: sky.backscatter.extend(0.0),
            // g_sea_uv_scale / g_swell_uv_scale = 1 / the containers' tile metres (CONFIRMED 0x0119E830).
            waves: Vec4::new(1.0 / sea.tile_metres, 1.0 / swell.tile_metres, settings.decay, FRESNEL_R0),
            deep: settings.deep.extend(1.0),
            // g_time starts at 0 and is advanced per frame.
            spec: Vec4::new(settings.shininess * SHININESS_TO_EXPONENT, 0.0, 0.0, 0.0),
            foam: Vec4::new(if foam_on() { 1.0 } else { 0.0 }, settings.froth[0], FOAM_UV_SCALE, MAX_FOAM_DIST),
            froth: Vec4::new(settings.froth[1], settings.froth[2], 0.0, 0.0),
            depth: Vec4::new(FULL_CHOP_DEPTH, SEA_LEVEL, 0.0, 0.0),
            depth_rect: depth_rect(&map),
            sea_frames: frames_at(sea, 0.0),
            swell_frames: frames_at(swell, 0.0),
            sea_range: range(sea),
            swell_range: range(swell),
        },
        sea_surface: textures.sea.clone(),
        swell_surface: textures.swell.clone(),
        foam: textures.foam.clone(),
        depth: images.add(depth_texture(&map)),
    });
    let entity = commands
        .spawn((
            Mesh3d(meshes.add(sea_mesh(SEA_LEVEL))),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Name::new("sea surface"),
            DespawnOnExit(crate::GameMode::Battle),
        ))
        .id();
    commands.insert_resource(Sea { entity, material, waves: textures.waves.clone() });
}

/// Reads and decodes one wave container. A missing or broken file is logged once and replaced by
/// a single flat frame (every texel the zero angle), so the sea still draws.
///
/// `max_layers` is the GPU's `max_texture_array_layers`: every
/// frame is one layer of one texture array. The original has no frame limit (it rebuilds one 2D
/// texture per frame from two frames, `0x010E21F0`); the shipped files have 100 and 60 frames.
/// A mod container with more frames than the GPU holds is drawn flat, logged once (decoded once
/// per session).
fn wave_container(vfs: &Vfs, path: &str, max_layers: u32) -> SeaWaves {
    let decoded = vfs.read(path).map_err(|e| e.to_string()).and_then(|b| SeaWaves::parse(&b).map_err(|e| e.to_string()));
    match decoded {
        Ok(w) if w.frames <= max_layers => w,
        Ok(w) => {
            warn!(
                "Battle sea: {path} has {} frames, more than this GPU's texture-array limit ({max_layers}); drawing it flat",
                w.frames
            );
            flat_waves()
        }
        Err(e) => {
            warn!("Battle sea: {path}: {e}; drawing it flat");
            flat_waves()
        }
    }
}

/// One 1×1 frame whose range is the middle value, i.e. a zero angle (`convert_to_angle(0.5)`).
fn flat_waves() -> SeaWaves {
    SeaWaves { size: 1, tile_metres: 1.0, period: 1.0, frames: 1, lo: [32768; 2], hi: [32768; 2], angles: vec![0; 2] }
}

/// All frames of a container as one `Rg8Unorm` texture array, one layer per frame with its own mip
/// chain (box filter, PROVISIONAL), sampled with wrap and trilinear filtering like `ocean.fx`'s
/// `sea_surface_sampler` / `swell_surface_sampler` (CONFIRMED sampler states, `SRGBTEXTURE` false).
fn angle_array(waves: &SeaWaves) -> Image {
    let size = waves.size as usize;
    let mut data = Vec::with_capacity(waves.angles.len() / 3 * 4 + 2 * waves.frames as usize);
    let mut mips = 1;
    for layer in 0..waves.frames {
        let mut level = waves.frame(layer).to_vec();
        data.extend_from_slice(&level);
        let (mut s, mut count) = (size, 1);
        while s > 1 {
            let n = s / 2;
            let mut next = vec![0u8; n * n * 2];
            for y in 0..n {
                for x in 0..n {
                    for ch in 0..2 {
                        let at = |dx: usize, dy: usize| level[((2 * y + dy) * s + 2 * x + dx) * 2 + ch] as u32;
                        next[(y * n + x) * 2 + ch] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                    }
                }
            }
            data.extend_from_slice(&next);
            (level, s) = (next, n);
            count += 1;
        }
        mips = count;
    }
    let extent = Extent3d { width: waves.size, height: waves.size, depth_or_array_layers: waves.frames };
    let mut image = Image::new_uninit(extent, TextureDimension::D2, TextureFormat::Rg8Unorm, RenderAssetUsages::RENDER_WORLD);
    // Layer-major (each layer's whole mip chain in turn), which is Bevy's default data order.
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mips;
    image.texture_view_descriptor =
        Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::D2Array), ..default() });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// What stands in for the original's `get_sky_reflection_color`, a sky reflection cube map we do
/// not render (**PROVISIONAL**): the map's `SKYGEN` sky model, evaluated per reflection vector in
/// `water.wgsl` exactly as `fx\sky.fx` `skygen_get_sky_color` / `CalcSkyColour` paint the sky.
///
/// The shipped `.environment` files store the circumsolar and horizon decays as **positive**
/// numbers (`HB_Nile`: 0.3495, 0.1956) while `sky.fx`'s defaults are negative (-0.84, -0.07);
/// `exp(+decay · angle)` saturates to 1 everywhere and paints the whole sky the circumsolar
/// colour, so the engine must negate them when it loads them. INFERRED: the setter is not traced
/// (the name at `0x0142512C` is only referenced from the name table at `0x01469884`), so the two
/// negations are **PROVISIONAL**. The backscatter decay is used as written (its default is
/// positive, 1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
struct SkyModel {
    sky: Vec3,
    circumsolar: Vec3,
    circumsolar_decay: f32,
    horizon: Vec3,
    horizon_opposite: Vec3,
    horizon_decay: f32,
    backscatter: Vec3,
    backscatter_decay: f32,
}

impl SkyModel {
    /// The map's `SKYGEN`, or (a map without one) a flat sky of the given colour, logged once.
    fn of(map: &BattleMap, fallback: Vec3) -> Self {
        match map.lighting.as_ref().and_then(|l| l.skygen.map(|s| (l, s))) {
            // The sky colour is `Lighting`'s (the one place it is stored), the rest `Skygen`'s.
            Some((l, s)) => Self::from_skygen(Vec3::from(l.sky_colour.unwrap_or_default()) * l.sky_colour_scale, &s),
            None => {
                warn!("Battle sea: {} has no SKYGEN, the sea reflects a flat sky", map.name);
                Self {
                    sky: fallback,
                    circumsolar: fallback,
                    circumsolar_decay: -1.0,
                    horizon: fallback,
                    horizon_opposite: fallback,
                    horizon_decay: -1.0,
                    backscatter: Vec3::ZERO,
                    backscatter_decay: 1.0,
                }
            }
        }
    }

    /// `sky` is `sky_colour` × `sky_colour_scale`.
    fn from_skygen(sky: Vec3, s: &Skygen) -> Self {
        Self {
            sky,
            circumsolar: Vec3::from(s.circumsolar),
            circumsolar_decay: -s.circumsolar_decay.abs(),
            horizon: Vec3::from(s.horizon),
            horizon_opposite: Vec3::from(s.horizon_opposite),
            horizon_decay: -s.horizon_decay.abs(),
            backscatter: Vec3::from(s.backscatter),
            backscatter_decay: s.backscatter_decay,
        }
    }
}

/// The sea-bed depth texture's world rectangle: `xy` = its (0, 0) texel's world x and z, `zw` = size.
fn depth_rect(map: &BattleMap) -> Vec4 {
    match map.ground() {
        Some(hf) => {
            let (w, h) = (hf.settings.world_width, hf.settings.world_height);
            Vec4::new(-w / 2.0, -h / 2.0, w, h)
        }
        None => Vec4::new(-1.0, -1.0, 2.0, 2.0),
    }
}

/// The camera-centred ring grid (see [`RINGS`]). Positions are local to the entity, which is moved
/// to the camera each frame, so the sea is always centred on the viewer.
///
/// Every ring is the full `2·EDGE_CELLS + 1` square centred on the origin, but a ring only emits the
/// cells **outside** the square the previous ring already covers (half-extent `EDGE_CELLS / 2` cells
/// of the previous ring's cell size). That is what makes the rings an annulus: two centred squares
/// of 32 cells each always overlap in the middle, and coplanar overlap on a flat plane z-fights.
fn sea_mesh(level: f32) -> Mesh {
    let n = (2 * EDGE_CELLS + 1) as usize;
    let (mut positions, mut indices) = (Vec::with_capacity(n * n * RINGS as usize), Vec::new());
    for ring in 0..RINGS {
        let cell = FIRST_CELL * 2f32.powi(ring as i32);
        // Cell (i, j) has its centre at `(i - EDGE_CELLS + 0.5) * cell`, so it lies inside the
        // previous ring's square when both `i` and `j` are in the middle `EDGE_CELLS / 2` cells.
        let skip = ring > 0;
        let from = EDGE_CELLS / 2;
        let to = EDGE_CELLS + EDGE_CELLS / 2;
        let base = positions.len() as u32;
        for j in 0..n {
            for i in 0..n {
                positions.push([(i as f32 - EDGE_CELLS as f32) * cell, level, (j as f32 - EDGE_CELLS as f32) * cell]);
            }
        }
        for j in 0..n as u32 - 1 {
            for i in 0..n as u32 - 1 {
                if skip && i >= from && i < to && j >= from && j < to {
                    continue;
                }
                let (a, b, c, d) = (
                    base + j * n as u32 + i,
                    base + j * n as u32 + i + 1,
                    base + (j + 1) * n as u32 + i,
                    base + (j + 1) * n as u32 + i + 1,
                );
                // Counter-clockwise seen from above (+Y) = front face in Bevy.
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; n * n * RINGS as usize]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; n * n * RINGS as usize]);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// `sea\combined_foam.tga` from the install: a 32-bit uncompressed 256² TGA. The original samples
/// its red / green / blue / alpha as foam / froth / tendril / distortion, without gamma
/// (`SRGBTEXTURE = false` in the non-ultra path) and **without mip maps** (`MipFilter = NONE`,
/// CONFIRMED sampler state), so this is one level, linear, wrapping.
fn foam_texture(vfs: &Vfs) -> Image {
    let decoded = vfs
        .read(FOAM_PATH)
        .map_err(|e| e.to_string())
        .and_then(|bytes| ntw_formats::tga::Tga::parse(&bytes).map_err(|e| e.to_string()));
    let (w, h, rgba) = match decoded {
        Ok(tga) if tga.width > 0 && tga.height > 0 => (tga.width, tga.height, tga.rgba),
        // Black: no foam pattern and no colour shift (the shader's `foam_value` is the red channel).
        Ok(_) => {
            warn!("Battle sea: {FOAM_PATH} is empty, no foam");
            (1, 1, vec![0u8; 4])
        }
        Err(e) => {
            warn!("Battle sea: {FOAM_PATH}: {e}, no foam");
            (1, 1, vec![0u8; 4])
        }
    };
    let extent = Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let mut image = Image::new(extent, TextureDimension::D2, rgba, TextureFormat::Rgba8Unorm, RenderAssetUsages::RENDER_WORLD);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Nearest,
        ..default()
    });
    image
}

/// The sea bed's depth below the sea level over the level-0 heightfield, as 0..1 over
/// 0..[`FULL_CHOP_DEPTH`] metres (R8, clamp to edge, so outside the map the border's depth applies).
fn depth_texture(map: &BattleMap) -> Image {
    let size = DEPTH_TEX;
    let mut data = vec![0u8; (size * size) as usize];
    if let Some(hf) = map.ground() {
        let (w, h) = (hf.settings.world_width, hf.settings.world_height);
        for j in 0..size {
            // Texel row 0 is the map's −z edge, matching `depth_rect`.
            let z = -h / 2.0 + (j as f32 + 0.5) / size as f32 * h;
            for i in 0..size {
                let x = -w / 2.0 + (i as f32 + 0.5) / size as f32 * w;
                let depth = (SEA_LEVEL - hf.height_at(x, z)).max(0.0);
                data[(j * size + i) as usize] = (depth / FULL_CHOP_DEPTH * 255.0).min(255.0) as u8;
            }
        }
    }
    let extent = Extent3d { width: size, height: size, depth_or_array_layers: 1 };
    let mut image = Image::new(extent, TextureDimension::D2, data, TextureFormat::R8Unorm, Default::default());
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Nearest,
        ..default()
    });
    image
}

/// The two frames and blend weight `water.wgsl` samples at `time` (`0x010E21F0`'s choice, the
/// weight in its 1/256 steps).
fn frames_at(waves: &SeaWaves, time: f32) -> Vec4 {
    let (a, b, w) = waves.frame_blend(time);
    Vec4::new(a as f32, b as f32, w as f32 / 256.0, 0.0)
}

/// Moves the sea surface to the camera and advances `g_time` and the wave animation.
/// PROVISIONAL: the clock is Bevy's elapsed time; which clock the original's sea runs on (and
/// whether it stops with the battle's pause) is not traced.
pub fn update_sea(
    time: Res<Time>,
    sea: Option<Res<Sea>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut transforms: Query<&mut Transform>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let Some(sea) = sea else { return };
    if let Some(cam) = camera.iter().next() {
        let p = cam.translation();
        if let Ok(mut t) = transforms.get_mut(sea.entity) {
            // Snap to the innermost cell so the ring pattern does not crawl under the camera.
            t.translation = Vec3::new((p.x / FIRST_CELL).round() * FIRST_CELL, SEA_LEVEL, (p.z / FIRST_CELL).round() * FIRST_CELL);
        }
    }
    let now = time.elapsed_secs();
    if let Some(mut mat) = materials.get_mut(&sea.material) {
        mat.params.spec.y = now;
        mat.params.sea_frames = frames_at(&sea.waves.sea, now);
        mat.params.swell_frames = frames_at(&sea.waves.swell, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rings must tile the plane with **no hole and no overlap**: ring `r` covers everything
    /// outside the square ring `r - 1` already filled, and ring 0 covers its whole square. Checked
    /// on the mesh itself: every quad is assigned the ring whose cell size its own corner fits, and
    /// no ring may draw a cell another ring drew.
    #[test]
    fn rings_cover_the_plane_without_gaps_or_overlap() {
        let mesh = sea_mesh(SEA_LEVEL);
        let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
        let side = 2 * EDGE_CELLS as usize;
        let mut drawn = std::collections::BTreeSet::new();
        let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
        for quad in indices.chunks(6) {
            let corners: Vec<[f32; 3]> = quad.iter().map(|&t| pos[t]).collect();
            // A quad is exactly one cell, so its width gives the ring's cell size outright.
            let (min_x, max_x) = (corners.iter().map(|c| c[0]).fold(f32::MAX, f32::min), corners.iter().map(|c| c[0]).fold(f32::MIN, f32::max));
            let (min_z, max_z) = (corners.iter().map(|c| c[2]).fold(f32::MAX, f32::min), corners.iter().map(|c| c[2]).fold(f32::MIN, f32::max));
            let cell = max_x - min_x;
            assert!((cell - (max_z - min_z)).abs() < 1e-4, "quad is not square: {cell} x {}", max_z - min_z);
            let ring = (cell / FIRST_CELL).log2().round() as u32;
            assert!((FIRST_CELL * 2f32.powi(ring as i32) - cell).abs() < 1e-4, "cell size {cell} is not a ring size");
            let i = (min_x / cell).round() as i32 + EDGE_CELLS as i32;
            let j = (min_z / cell).round() as i32 + EDGE_CELLS as i32;
            assert!((0..side as i32).contains(&i) && (0..side as i32).contains(&j), "cell index out of the grid");
            assert!(drawn.insert((ring, i, j)), "cell {ring}/{i},{j} drawn twice");
        }
        // Ring 0 fills its whole 32×32; each later ring fills that minus the previous ring's square,
        // which is `EDGE_CELLS` cells a side in this ring's units (the previous ring's half-extent
        // is 16 of *its* cells = 8 of this ring's = the middle 16 cells a side).
        let skipped = EDGE_CELLS as usize * EDGE_CELLS as usize;
        let expected = 32 * 32 + (RINGS as usize - 1) * (32 * 32 - skipped);
        assert_eq!(drawn.len(), expected, "unexpected number of sea cells");
        // The outermost ring reaches far past the camera's far plane.
        assert!(FIRST_CELL * 2f32.powi(RINGS as i32 - 1) * EDGE_CELLS as f32 > 100_000.0);
    }

    /// The sea mesh is the size the constants promise, and every vertex is at the sea level.
    #[test]
    fn sea_mesh_size_and_level() {
        let mesh = sea_mesh(SEA_LEVEL);
        let pos = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
        assert_eq!(pos.len(), (2 * EDGE_CELLS + 1).pow(2) as usize * RINGS as usize);
        for v in pos.as_float3().unwrap() {
            assert_eq!(v[1], SEA_LEVEL);
        }
        // Ring 0 is a full 32x32 grid; every later ring is an annulus of 32² - 16² cells.
        assert_eq!(mesh.indices().unwrap().len(), (32 * 32 + 12 * (32 * 32 - 16 * 16)) * 6);
    }

    /// Every frame goes up as one layer with its whole mip chain, layer after layer, and the mips
    /// are the box average of the level above.
    #[test]
    fn angle_array_uploads_every_frame_with_its_mips() {
        let (size, frames) = (4u32, 3u32);
        let angles: Vec<u8> = (0..frames * size * size * 2).map(|i| (i * 7 % 251) as u8).collect();
        let waves = SeaWaves { size, tile_metres: 70.0, period: 10.0, frames, lo: [0; 2], hi: [65535; 2], angles };
        let image = angle_array(&waves);
        assert_eq!(image.texture_descriptor.size.depth_or_array_layers, frames);
        assert_eq!(image.texture_descriptor.mip_level_count, 3);
        let per_layer = (16 + 4 + 1) * 2;
        let data = image.data.as_ref().unwrap();
        assert_eq!(data.len(), per_layer * frames as usize);
        // Layer 1 starts with frame 1, and its 2x2 mip's first texel averages the 2x2 corner.
        let layer = &data[per_layer..2 * per_layer];
        assert_eq!(&layer[..32], waves.frame(1));
        let corner = [0usize, 1, 4, 5].map(|t| waves.frame(1)[t * 2] as u32);
        assert_eq!(layer[32] as u32, (corner.iter().sum::<u32>() + 2) / 4);
    }

    /// An unknown key falls back to the first row, as `0x006AB800` does, and so does a missing one
    /// (PROVISIONAL for the sea surface, see `sea_settings`); a named key picks its row. Empty
    /// tables give nothing to draw with.
    #[test]
    fn sea_settings_pick_the_named_row_or_the_first() {
        let surface = |key: &str, shininess: f32| SeaSurface {
            key: key.into(),
            froth_value: 0.7,
            froth_distortion_speed: 0.35,
            froth_distortion_amount: 0.05,
            shininess,
            decay: 0.0006,
            ..default()
        };
        let surfaces = Table::from_rows(0, vec![surface("wind_level_0", 200.0), surface("wind_level_2", 378.0)]);
        let climate = |key: &str, r: f32| SeaClimateDetail { climate: key.into(), deep_r: r, ..default() };
        let climates = Table::from_rows(0, vec![climate("lc_a", 0.02), climate("lc_b", 0.5)]);
        let named = sea_settings(&surfaces, &climates, Some("wind_level_2"), Some("lc_b")).unwrap();
        assert_eq!((named.shininess, named.deep.x), (378.0, 0.5));
        assert_eq!((named.froth, named.decay), ([0.7, 0.35, 0.05], 0.0006));
        let unknown = sea_settings(&surfaces, &climates, Some("wind_level_9"), Some("lc_z")).unwrap();
        assert_eq!((unknown.shininess, unknown.deep.x), (200.0, 0.02));
        let missing = sea_settings(&surfaces, &climates, None, None).unwrap();
        assert_eq!(missing, unknown);
        assert!(sea_settings(&Table::from_rows(0, Vec::new()), &climates, None, None).is_none());
    }

    /// Leaving a battle drops the `Sea` (and with it the last handle to its material), so the next
    /// battle does not update a stale material; the session's `SeaTextures` stay for the next one.
    #[test]
    fn leaving_the_battle_removes_the_sea() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let waves = Arc::new(WaveHeaders { sea: flat_waves(), swell: flat_waves() });
        world.insert_resource(Sea { entity: Entity::PLACEHOLDER, material: Handle::default(), waves: waves.clone() });
        world.insert_resource(SeaTextures { waves, sea: Handle::default(), swell: Handle::default(), foam: Handle::default() });
        world.run_system_once(remove_sea).unwrap();
        assert!(!world.contains_resource::<Sea>());
        assert!(world.contains_resource::<SeaTextures>());
    }

    /// The shipped files' positive circumsolar / horizon decays become the falling exponentials
    /// `sky.fx`'s negative defaults describe; the backscatter decay keeps its sign.
    #[test]
    fn sky_model_negates_the_shipped_decays() {
        let s = Skygen { circumsolar_decay: 0.3495, horizon_decay: 0.1956, backscatter_decay: 0.6893, ..default() };
        let m = SkyModel::from_skygen(Vec3::ONE, &s);
        assert_eq!((m.circumsolar_decay, m.horizon_decay, m.backscatter_decay), (-0.3495, -0.1956, 0.6893));
        let d = Skygen { circumsolar_decay: -0.84, horizon_decay: -0.07, backscatter_decay: 1.0, ..default() };
        let m = SkyModel::from_skygen(Vec3::ONE, &d);
        assert_eq!((m.circumsolar_decay, m.horizon_decay), (-0.84, -0.07));
    }
}

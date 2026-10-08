//! The battle map's sea: the surface itself, drawn at the sea level over the ground that lies
//! below it. `fx\ocean.fx` (shipped as HLSL text in the install's `fx\` folder) is the original's
//! sea shader and is the source of every constant and every step here; `water.wgsl` is our own
//! port of its maths. Research notes: `analysis/graphics/WATER.md`.
//!
//! - **Where the water is** (`ntw_formats::battle_terrain::SEA_LEVEL`): the sea surface is at y = 0
//!   in map metres, CONFIRMED on all 60 presets; a position is sea when the ground there is below
//!   that. The original draws one flat plane and lets the terrain occlude it, so this module does
//!   the same: no mask, no stencil.
//! - **The mesh** is a camera-centred set of concentric square rings with geometrically growing
//!   cell size, which is what `ocean.fx`'s `g_band_size` 64 / `g_grid_increment` 0.5 /
//!   `g_start_band` 4 describe (INFERRED — the ring builder is in the exe, which we have not
//!   disassembled). The entity's transform follows the camera, so the mesh is built once.
//! - **`sea_attenuation`**, the per-vertex attribute that damps the short chop near the shore, comes
//!   from a sea-bed depth texture of the level-0 heightfield (INFERRED: the original's own source
//!   for the attribute is not known).
//! - **Textures.** `sea\combined_foam.tga` is a real shipped asset and is read from the install. The
//!   two wave-angle textures, `sea\sea` and `sea\swell`, are in the engine's own texture container
//!   (magic `0x12345678`, version 1), which is **UNKNOWN** — see `WATER.md` §2 — so we generate
//!   tiling slope maps with the same layout instead (**PLACEHOLDER**).
//! - **Not here**: shoreline foam blending, planar reflections, refraction/sea-bed visibility, rain
//!   ripples, wakes, and inland water bodies at their own level (a lake at +105 m is not at the sea
//!   level). All listed in `WATER.md` §3.

use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use ntw_formats::battle_terrain::{BattleMap, SEA_LEVEL};
use ntw_formats::pack::Vfs;

use super::{TerrainVfs, image_with_mips, lighting_params};

/// `ocean.fx`: `g_grid_increment` × `g_start_band` = 0.5 m × 4 = the cell size of the innermost ring.
const FIRST_CELL: f32 = 0.5 * 4.0;
/// How many doublings of the cell size. 13 rings from 2 m reach a 16,384 m cell, so the outermost
/// ring is far past the camera's 20 km far plane and the sea always runs to the horizon.
const RINGS: u32 = 13;
/// Cells along one side of a ring's outer edge. Each ring is a `2 * EDGE_CELLS + 1` vertex square, so
/// the cost does not grow with the world: 13 rings × 33² vertices = 13,533 and 13 × 2·32² = 26,624
/// triangles. (`g_band_size` 64 in `ocean.fx` is a band's width in cells; our rings are 32 cells of
/// their own size, which reaches the same order of detail — INFERRED.)
const EDGE_CELLS: u32 = 16;
/// The wave-slope texture that covers `sea\sea`'s header tile size, in metres (INFERRED, `WATER.md` §2).
const SEA_TILE_METRES: f32 = 70.0;
/// The same for `sea\swell`.
const SWELL_TILE_METRES: f32 = 800.0;
/// Sea-bed depth (metres) at which the short chop reaches full strength. PROVISIONAL: ours, with
/// the INFERRED depth source of `sea_attenuation` (module comment); the original's value is UNKNOWN.
const FULL_CHOP_DEPTH: f32 = 12.0;
/// Texels per side of the sea-bed depth texture (ours, a resolution choice; no original value).
const DEPTH_TEX: u32 = 512;
/// `ocean.fx g_sea_decay`: how fast the short chop dies with view depth, in `1 / metres`. The
/// shipped file says `0.2f`, which multiplied by the clip w **in metres** would put the chop out
/// within 5 m of the camera — a flat mirror (CONFIRMED by measurement) — so like `g_time = 1.0f` and
/// `g_sea_shininess` it is INFERRED to be a default the engine replaces (no setter traced; **UNKNOWN**
/// what it sets). **PROVISIONAL** here: 1/400 m.
const SEA_DECAY: f32 = 1.0 / 400.0;
/// `ocean.fx g_sea_shininess`: the Blinn-Phong exponent is `g_sea_shininess * 0.7`. The shipped
/// file says `1.0f`, i.e. an exponent of 0.7 — **below** Lambert, so `pow(saturate(dot(n, halfway)),
/// 0.7)` is ≈ 1 across the whole sea and the highlight drowns everything in the sun colour (measured
/// in-game: a white screen on `hb_nile`). INFERRED engine-overridden default, like `g_sea_decay`.
/// **PROVISIONAL** here: the exponent itself is 8, chosen so the lobe is *wider* than the wave tilt
/// (`SPECULAR_EXPONENT`); at 28 the lobe is narrower than the tilt and the glitter breaks into a
/// hard speckle (also measured). Set `g_sea_shininess = 8 / 0.7 = 11.43` to match the shader's name.
const SEA_SHININESS: f32 = 8.0 / 0.7;
/// The exponent `water.wgsl` actually uses, `g_sea_shininess * 0.7`.
const SPECULAR_EXPONENT: f32 = SEA_SHININESS * 0.7;
/// Below this share of water the map is a land battle with a pond, not a sea. INFERRED: the naval
/// presets are 100 % water and `nap_mp_italian_grassland` has 0.01 %, so any small threshold
/// separates them.
const MIN_SEA_COVERAGE: f32 = 0.005;
/// The foam texture in the install (`ocean.fx g_combined_foam_texture`).
const FOAM_PATH: &str = "sea\\combined_foam.tga";
/// Foam on or off. `ocean.fx` compiles the foam into the pixel shader but the engine picks
/// **per draw** between `sm_render_sm3` (no foam), `sm_render_foam_sm3` (foam) and the four
/// `sm_render_sea_stenciling_*` variants, which clip it to where the sea meets something
/// (CONFIRMED techniques, UNKNOWN which pass each region gets). One surface, one pass, so there is
/// nothing here to choose by: **PROVISIONAL**, foam off by default — on by default it covers the
/// whole visible sea in whitecaps (measured on hb_toulon), and it is what the shoreline work
/// (`WATER.md` §4) has to drive properly. `NAPOLEON_SEA_FOAM=1` turns it on.
fn foam_on() -> bool {
    std::env::var("NAPOLEON_SEA_FOAM").is_ok_and(|v| v == "1")
}

/// The sea surface material (see `water.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct SeaMaterial {
    #[uniform(0)]
    params: SeaParams,
    #[texture(1)]
    #[sampler(2)]
    sea_surface: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    swell_surface: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    foam: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    depth: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct SeaParams {
    sun_dir: Vec4,
    sun_colour: Vec4,
    sky: Vec4,
    waves: Vec4,
    deep: Vec4,
    spec: Vec4,
    foam: Vec4,
    depth: Vec4,
    depth_rect: Vec4,
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

/// The sea surface, so the per-frame system can follow the camera.
#[derive(Resource)]
pub struct Sea {
    /// The mesh entity.
    pub entity: Entity,
}

/// Spawns the sea surface for the loaded battle map. Does nothing when the map has no water (the
/// common case: most historical battle maps lie entirely above the sea level).
pub fn spawn_sea(
    mut commands: Commands,
    vfs: Option<Res<TerrainVfs>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SeaMaterial>>,
) {
    let (Some(map), Some(vfs)) = (super::current_map(), vfs) else { return };
    let coverage = map.sea_coverage();
    if coverage <= MIN_SEA_COVERAGE {
        return;
    }
    info!("Battle sea: {:.2}% of {} is below the sea level, drawing the surface", 100.0 * coverage, map.name);
    let (sun_dir, sun, ambient) = lighting_params(&map);
    let sky = sky_colour(&map, ambient);
    let material = materials.add(SeaMaterial {
        params: SeaParams {
            sun_dir: sun_dir.extend(0.0),
            sun_colour: sun.extend(1.0),
            sky: sky.extend(1.0),
            // g_sea_uv_scale / g_swell_uv_scale as 1 / tile size in metres (INFERRED);
            // g_fresnel_R0 0.5 is the shader's own constant and is used as written.
            waves: Vec4::new(1.0 / SEA_TILE_METRES, 1.0 / SWELL_TILE_METRES, SEA_DECAY, 0.5),
            // g_sea_deep_colour, the shader's own constant (WATER.md §4).
            deep: Vec4::new(0.0, 0.1, 0.5, 1.0),
            // g_sea_shininess * 0.7; g_time starts at 0 and is advanced per frame.
            spec: Vec4::new(SPECULAR_EXPONENT, 0.0, 0.0, 0.0),
            // Foam off by default, PROVISIONAL (see `foam_on`); g_froth_value 0.75, g_foam_uv_scale 15
            // and MAX_FOAM_DIST 300 are the shader's own constants (WATER.md §4).
            foam: Vec4::new(
                if foam_on() { 1.0 } else { 0.0 },
                0.75,
                15.0,
                300.0,
            ),
            depth: Vec4::new(FULL_CHOP_DEPTH, SEA_LEVEL, 0.0, 0.0),
            depth_rect: depth_rect(&map),
        },
        sea_surface: images.add(wave_map(256, CHOP_WAVES, CHOP_SLOPE)),
        swell_surface: images.add(wave_map(128, SWELL_WAVES, SWELL_SLOPE)),
        foam: images.add(foam_texture(&vfs.0)),
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
    commands.insert_resource(Sea { entity });
}

/// What the original's `get_sky_reflection_color` would return. It samples a rendered sky cube map;
/// we have none, so the map's `SKYGEN sky_colour` × `sky_colour_scale` stands in (PROVISIONAL).
fn sky_colour(map: &BattleMap, fallback: Vec3) -> Vec3 {
    map.lighting
        .as_ref()
        .filter(|l| l.sky_colour.is_some())
        .map(|l| Vec3::from(l.sky_colour.unwrap()) * l.sky_colour_scale)
        .unwrap_or(fallback)
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

/// One wave of a generated slope map: `(kx, ky)` in whole cycles per tile, so the map is seamless.
type Wave = (i32, i32);

/// PLACEHOLDER (with [`SWELL_WAVES`], [`CHOP_SLOPE`] and [`SWELL_SLOPE`]): ours, standing in for the
/// undecoded `sea\sea` / `sea\swell` textures (`wave_map`). The short chop: wavelengths from about 3 to 25 m in a 70 m tile (`|k|` 3…25). Amplitudes are
/// `1 / |k|` (see [`wave_map`]), the total is [`CHOP_SLOPE`] radians of surface angle.
const CHOP_WAVES: &[Wave] = &[(13, 5), (-9, 17), (19, -7), (-17, -13), (7, -21), (23, 3), (11, -25)];

/// The longest waves: wavelengths from about 100 to 800 m in an 800 m tile (`|k|` 3…8). A sea mostly
/// reads as these; the chop is the texture on top.
const SWELL_WAVES: &[Wave] = &[(5, 2), (-3, 7), (7, -4), (2, -9)];

/// Total surface angle of [`CHOP_WAVES`], radians (0.38 = 22°).
const CHOP_SLOPE: f32 = 0.38;
/// Total surface angle of [`SWELL_WAVES`], radians (0.22 = 13°). The shader adds the two, so the
/// worst case is 0.60 rad = 34° of tilt.
const SWELL_SLOPE: f32 = 0.22;

/// A tiling wave-slope map, in the layout `ocean.fx` samples: the red and green channels hold the
/// two slope angles as `0.5 + slope / 2PI` (`convert_to_angle` turns them back into radians), and
/// blue holds the slope magnitude for later passes. **PLACEHOLDER** for the original's `sea\sea`
/// and `sea\swell` (unknown container, `WATER.md` §2).
///
/// Each wave's angle amplitude is `total · (1/|k|) / Σ(1/|k|)`, so short waves stay small, the sum
/// of the amplitudes is exactly `total` (a hard bound on the slope, which is what keeps the 8-bit
/// angle channels from clipping) and the result is reproducible from the table alone — no RNG and
/// no dependence on anything outside this file.
fn wave_map(size: u32, waves: &[Wave], total: f32) -> Image {
    let weight: f32 = waves.iter().map(|(kx, ky)| 1.0 / ((kx * kx + ky * ky) as f32).sqrt()).sum();
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for j in 0..size {
        for i in 0..size {
            let (sx, sy) = wave_slope(i as f32 / size as f32, j as f32 / size as f32, waves, total, weight);
            let p = ((j * size + i) * 4) as usize;
            rgba[p] = angle_texel(sx);
            rgba[p + 1] = angle_texel(sy);
            rgba[p + 2] = ((sx * sx + sy * sy).sqrt() * 255.0).min(255.0) as u8;
            rgba[p + 3] = 255;
        }
    }
    let mut image = image_with_mips(size, size, rgba, true);
    // The original samples these without gamma (`SRGBTEXTURE = false`), so UNORM.
    image.texture_descriptor.format = TextureFormat::Rgba8Unorm;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    image
}

/// The two slope angles at `(u, v)` in tile units: the gradient of the sum of sines [`Wave`] lists,
/// weighted so the amplitudes add up to `total`. Exact, and periodic with period 1 in each axis
/// because every `k` is a whole number of cycles per tile.
fn wave_slope(u: f32, v: f32, waves: &[Wave], total: f32, weight: f32) -> (f32, f32) {
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    for &(kx, ky) in waves {
        // Height `h · sin(2PI(k·uv))` along the wave's direction, so its gradient is
        // `a · cos(2PI(k·uv))` along that direction.
        let phase = std::f32::consts::TAU * (kx as f32 * u + ky as f32 * v);
        let length = ((kx * kx + ky * ky) as f32).sqrt();
        let a = total / weight / length;
        sx += a * kx as f32 / length * phase.cos();
        sy += a * ky as f32 / length * phase.cos();
    }
    (sx, sy)
}

/// One channel of a slope map: `convert_to_angle` inverts `0.5 + slope / 2PI`.
fn angle_texel(slope: f32) -> u8 {
    (0.5 + slope / std::f32::consts::TAU).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8
}

/// `sea\combined_foam.tga` from the install: a 32-bit uncompressed 256² TGA (a 26-byte TGA
/// extension area follows the pixels). The original samples its red/green/blue as
/// foam / froth / tendril, without gamma (`SRGBTEXTURE = GAMMA_CORRECT_TEXTURE` there; here UNORM
/// either way, PROVISIONAL).
fn foam_texture(vfs: &Vfs) -> Image {
    let decoded = vfs
        .read(FOAM_PATH)
        .map_err(|e| e.to_string())
        .and_then(|bytes| ntw_formats::tga::Tga::parse(&bytes).map_err(|e| e.to_string()));
    let (w, h, rgba) = match decoded {
        Ok(tga) if tga.width > 0 && tga.height > 0 => (tga.width, tga.height, tga.rgba),
        // Black: no foam pattern and no colour shift (the shader's `foam_value` is the red channel).
        _ => {
            if let Err(e) = vfs.read(FOAM_PATH) {
                warn!("Sea foam texture {FOAM_PATH}: {e}");
            }
            (1, 1, vec![0u8; 4])
        }
    };
    let mut image = image_with_mips(w, h, rgba, true);
    image.texture_descriptor.format = TextureFormat::Rgba8Unorm;
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

/// Moves the sea surface to the camera and advances `g_time` (the wave and foam textures scroll
/// with it in the original).
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
    // There is one sea material per battle (one map, one surface), so every asset is the sea's.
    let now = time.elapsed_secs();
    for (_, mat) in materials.iter_mut() {
        mat.params.spec.y = now;
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

    /// `wave_slope` is the gradient of the sine sum and repeats exactly every tile, which is what
    /// makes the generated maps seamless, and its magnitude never exceeds the total the tables ask
    /// for (so the 8-bit angle channels cannot clip).
    #[test]
    fn wave_slope_tiles_and_stays_in_range() {
        for (waves, total) in [(CHOP_WAVES, CHOP_SLOPE), (SWELL_WAVES, SWELL_SLOPE)] {
            let weight: f32 = waves.iter().map(|(kx, ky)| 1.0 / ((kx * kx + ky * ky) as f32).sqrt()).sum();
            for &(u, v) in &[(0.0f32, 0.0f32), (0.31, 0.77), (0.5, 0.125), (0.999, 0.001)] {
                let (x, y) = wave_slope(u, v, waves, total, weight);
                assert!((wave_slope(u + 1.0, v, waves, total, weight).0 - x).abs() < 1e-5, "does not wrap in u");
                assert!((wave_slope(u, v + 1.0, waves, total, weight).1 - y).abs() < 1e-5, "does not wrap in v");
                assert!(x.hypot(y) <= total + 1e-4, "slope {x},{y} exceeds the total {total}");
                // The texture stores 0.5 + slope / 2PI, so it must stay inside 0..1.
                assert!((x / std::f32::consts::TAU).abs() <= 0.5 && (y / std::f32::consts::TAU).abs() <= 0.5);
            }
            // The shader adds the swell and the chop, so their sum is the real worst-case tilt: the
            // sea must not be so steep it reads as rubble.
            let peak = peak_slope();
            assert!(peak < 0.7, "the sea's peak tilt is {peak} rad, too rough to read as water");
            // The sun highlight is `pow(cos(θ), e)`; the angle where it falls to half is
            // `acos(e^(-1/e))`. It has to be a broad band, not a single-pixel glint, or the sea
            // sparkles (measured: exponent 28, half-width 0.10 rad, hard speckle).
            let half_width = SPECULAR_EXPONENT.powf(-1.0 / SPECULAR_EXPONENT).acos();
            assert!(half_width > 0.15, "the highlight's half-width is only {half_width} rad");
        }
    }

    /// The largest surface angle the two wave tables can add up to, sampled on a fine grid (the
    /// bound is `CHOP_SLOPE + SWELL_SLOPE`, but the sum of sines only reaches it in phase).
    fn peak_slope() -> f32 {
        let weight = |waves: &[Wave]| waves.iter().map(|(kx, ky)| 1.0 / ((kx * kx + ky * ky) as f32).sqrt()).sum::<f32>();
        let (cw, sw) = (weight(CHOP_WAVES), weight(SWELL_WAVES));
        let mut peak = 0.0f32;
        for j in 0..64 {
            for i in 0..64 {
                let (u, v) = (i as f32 / 64.0, j as f32 / 64.0);
                let (cx, cy) = wave_slope(u, v, CHOP_WAVES, CHOP_SLOPE, cw);
                let (sx, sy) = wave_slope(u, v, SWELL_WAVES, SWELL_SLOPE, sw);
                peak = peak.max((cx + sx).hypot(cy + sy));
            }
        }
        peak
    }

    /// `convert_to_angle` must invert `angle_texel`: a zero slope is the middle of the range.
    #[test]
    fn angle_texel_round_trips() {
        assert_eq!(angle_texel(0.0), 128);
        assert_eq!(angle_texel(std::f32::consts::TAU * 0.5), 255);
        assert_eq!(angle_texel(-std::f32::consts::TAU * 0.5), 0);
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
}

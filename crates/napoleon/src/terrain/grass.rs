//! Battle-map **grass**, drawn our own way from the original's placement (`ntw_formats::grass`).
//!
//! Data: the map's grass map (`<preset>\grassmap` in the original loader's extension order, a
//! `.tga` or on six presets a `.jpg` with its `_alpha.jpg`, or our open override `grassmap.png` in
//! the same folder: RGBA = the densities of the kinds g0..g3; [`find_grass_map`]), its grass type's picture atlas
//! (`battleterrain\grass\<textures.xml grass_type>.tai` + its DDS pages), the ground's own colour
//! map and tiled texture (the blades take their colour from the ground under them), and
//! `commontextures\jitter.dds` (the dissolve pattern at the fade's edge). A map with no grass map,
//! no grass type or an empty atlas has no grass.
//!
//! Drawing: the map is cut into the original's cells ([`ntw_formats::grass::cell_window`]); the
//! cells around the camera are built when they come into the window (a cell is the same every
//! time it is built) and dropped when they leave it. Each cell is one mesh per atlas page, 4
//! vertices per blade at its foot, and `grass.wgsl` spreads them into a quad facing the view,
//! fades it with distance and sways it (grass.fx's rules, numbers from the material description
//! `materials/vanilla/grass.toml`). Nothing is rebuilt per frame: only the gust uniform changes
//! while the wind's gust is still settling.
//!
//! The detail setting `gfx_grass_quality` picks the cell size, radius and blades per cell
//! (CONFIRMED table `0x01419AB4`); `NAPOLEON_GRASS_QUALITY=<0..4>` overrides it (test harness).
//!
//! PROVISIONAL: only grass level 0 is drawn (the exe loops `num_grass_levels` levels of doubling
//! cell size, whose default is UNKNOWN); the gust's two weather inputs; the sway's time input
//! (grass.fx names it `g_sin_elapsed`; what the exe puts there is not traced, we give it the elapsed
//! seconds); the lighting (the ground normal's sun + ambient, as `terrain.wgsl`, without the
//! original's lightmap, shadows or fog). See `analysis/graphics/GRASS.md`.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexFormat};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TextureFormat,
};
use bevy::shader::ShaderRef;
use ntw_formats::battle_terrain::BattleMap;
use ntw_formats::pack::GameFiles;
use ntw_formats::grass::{
    ATLAS_FOLDER, BLADE_WIDTH, Blade, GrassAtlas, GrassLook, GrassMap, GrassQuality, ImageKind, Placement, advance_gust,
    atlas_path, blade_look, cell_window, corner_uv, find_grass_map, image_kind, place_cell, quality_for_option,
};
use ntw_formats::material::{GRASS_MATERIAL, MaterialLibrary};
use ntw_formats::pack::Vfs;
use ntw_formats::speedtree::wind::WIND_LADDER;

use super::{TerrainTextures, TerrainVfs, current_map, lighting_params};

/// Per vertex: the corner weights (x across, y up), the blade's phase (0..1) and 0
/// (grass.fx `COLOR0`).
pub const ATTRIBUTE_WEIGHT: MeshVertexAttribute = MeshVertexAttribute::new("Grass_Weight", 988_540_930, VertexFormat::Float32x4);

/// The dissolve pattern (CONFIRMED string `CommonTextures/jitter.dds` in the exe).
const JITTER_MAP: &str = r"commontextures\jitter.dds";
/// Most cells built in one frame, nearest first, so entering a battle or a jump of the camera
/// spreads its building over a few frames instead of one long one.
const CELLS_PER_FRAME: usize = 24;

/// The grass material (see `grass.wgsl`); one per atlas page.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GrassMaterial {
    #[uniform(0)]
    params: GrassParams,
    #[texture(1)]
    #[sampler(2)]
    atlas: Handle<Image>,
    #[texture(3)]
    #[sampler(4)]
    colour_map: Handle<Image>,
    #[texture(5)]
    #[sampler(6)]
    tile_map: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    jitter: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct GrassParams {
    sun_dir: Vec4,
    sun_colour: Vec4,
    ambient: Vec4,
    /// spread, rise, lift, view_lean
    shape: Vec4,
    /// fade_distance, fade_spread, fade_start, fade_width
    fade: Vec4,
    /// fade_height_weight, dissolve_start, alpha_gain, alpha-test cutoff
    fade2: Vec4,
    /// sway_amplitude, sway_speed, sway_speed_spread, sway_phase
    sway: Vec4,
    /// the wind (Bevy x, z) in m/s, the gust, gust_scale × grass_wind_scale
    gust: Vec4,
    /// level 0's width and height in metres, the tiled texture's repeats per map, 0
    map: Vec4,
}

impl Material for GrassMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://napoleon/terrain/grass.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/terrain/grass.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(self.params.fade2.w)
    }
    // The quads are built in our own vertex shader, which the default prepass and shadow shaders
    // would not do.
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(3),
            ATTRIBUTE_WEIGHT.at_shader_location(4),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // grass.fx: CULLMODE = NONE.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// The battle's grass: what the cells are built from and the cells built so far.
#[derive(Resource)]
pub struct GrassField {
    map: std::sync::Arc<BattleMap>,
    grass_map: GrassMap,
    atlas: GrassAtlas,
    quality: GrassQuality,
    size: [f32; 2],
    look: GrassLook,
    /// Per atlas entry, its page (an index into `materials`).
    entry_page: Vec<usize>,
    materials: Vec<Handle<GrassMaterial>>,
    /// The window of cells last asked for.
    window: Option<([u32; 2], [u32; 2])>,
    /// Built cells → their entities (one per page that has blades there).
    cells: HashMap<[u32; 2], Vec<Entity>>,
    /// Cells of the window still to build, farthest first (built from the back).
    queue: Vec<[u32; 2]>,
    /// Reused for every cell's blades.
    scratch: Vec<Blade>,
    gust: f32,
    applied_gust: f32,
}

impl GrassField {
    fn placement(&self) -> Placement<'_> {
        Placement {
            map_width: self.size[0],
            map_height: self.size[1],
            quality: self.quality,
            edge_margin: 5.0 * BLADE_WIDTH,
            grass_map: &self.grass_map,
            atlas: &self.atlas,
        }
    }
}

/// The grass detail setting in use: `NAPOLEON_GRASS_QUALITY` if set, else the preferences file's
/// `gfx_grass_quality`, else the value the game registers it with (2).
fn grass_option() -> i64 {
    if let Some(v) = std::env::var("NAPOLEON_GRASS_QUALITY").ok().and_then(|s| s.trim().parse::<i64>().ok()) {
        return v;
    }
    let user = crate::config::user_dir();
    let prefs = ntw_script::ui::frontend::load_preferences(user.as_deref(), crate::config::original_user_dir().as_deref());
    option_from(&prefs)
}

/// `gfx_grass_quality` from the preferences, else the default the game registers it with
/// (`Preferences::get_f64`; the original's is 2, `0x00403D20`). A game that registers no such
/// setting gets 0: no grass.
fn option_from(prefs: &ntw_formats::preferences::Preferences) -> i64 {
    prefs.get_f64("gfx_grass_quality").unwrap_or_default() as i64
}

/// Decodes a JPEG or PNG to RGBA8 ([`super::decode_rgba8`]: a JPEG's alpha is 255; a grey one
/// gives R = G = B).
fn decode_bevy(bytes: &[u8], ext: &str) -> Result<GrassMap, String> {
    let (w, h, rgba) = super::decode_rgba8(bytes, ext)?;
    GrassMap::new(w, h, rgba).ok_or_else(|| "an empty image".to_string())
}

/// Reads the image at `path` as the original's loader decodes it ([`image_kind`]).
fn read_image(vfs: &Vfs, path: &str) -> Result<GrassMap, String> {
    let bytes = vfs.read(path).map_err(|e| format!("{path}: {e}"))?;
    match image_kind(path, &bytes) {
        ImageKind::Jpeg => decode_bevy(&bytes, "jpg"),
        ImageKind::Png => decode_bevy(&bytes, "png"),
        ImageKind::Dds => GrassMap::from_dds(&bytes),
        ImageKind::Tga => GrassMap::from_tga(&bytes),
    }
    .map_err(|e| format!("{path}: {e}"))
}

/// The map's grass map ([`find_grass_map`]: our `grassmap.png` override, else `grassmap` in the
/// original loader's extension order, a `.jpg` with its `_alpha.jpg`). `None` (logged once: a
/// warning when a file is there but unreadable) when there is none. An `_alpha.jpg` that does not
/// decode is warned about and the map kept with its own alpha.
fn load_grass_map(vfs: &Vfs, folder: &str) -> Option<GrassMap> {
    let Some(file) = find_grass_map(folder, |p| vfs.contains(p)) else {
        info!("Battle-map grass: {folder} has no grass map; no grass");
        return None;
    };
    let mut map = match read_image(vfs, &file.path) {
        Ok(m) => m,
        Err(e) => {
            warn!("Battle-map grass not drawn: {e}");
            return None;
        }
    };
    if let Some(alpha) = &file.alpha {
        match read_image(vfs, alpha) {
            Ok(a) => map.apply_alpha(&a),
            Err(e) => warn!("Battle-map grass: its alpha not read, the densities of g3 stay full: {e}"),
        }
    }
    Some(map)
}

/// A texture sampled as stored (grass.fx: `SRGBTEXTURE = false`), linear, with the address mode.
fn load_unorm(vfs: &Vfs, path: &str, mode: ImageAddressMode) -> Result<Image, String> {
    let mut image = crate::soldiers::load_tinted(vfs, path, None, None).map_err(|e| format!("{path}: {e}"))?;
    image.texture_descriptor.format = TextureFormat::Rgba8Unorm;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        ..ImageSamplerDescriptor::linear()
    });
    Ok(image)
}

/// On entering a battle, after the terrain: reads the grass's data and makes its materials. The
/// cells themselves are built by [`update_grass`].
pub fn setup_grass(
    mut commands: Commands,
    vfs: Option<Res<TerrainVfs>>,
    ground: Option<Res<TerrainTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    let (Some(map), Some(vfs), Some(ground)) = (current_map(), vfs, ground) else { return };
    let vfs = &vfs.0;
    let Some(quality) = quality_for_option(grass_option()) else {
        info!("Battle-map grass off (gfx_grass_quality 0)");
        return;
    };
    let Some(level0) = map.heightfields.first() else { return };
    let size = [level0.settings.world_width, level0.settings.world_height];
    let grass_type = map.textures.as_ref().map(|t| t.grass_type.clone()).unwrap_or_default();
    if grass_type.is_empty() {
        info!("Battle-map grass: the map names no grass type; no grass");
        return;
    }
    let Some(grass_map) = load_grass_map(vfs, &map.folder) else { return };
    let path = atlas_path(&grass_type);
    let tai = vfs
        .read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| ntw_formats::texture_atlas::TaiAtlas::read(&String::from_utf8_lossy(&b)).map_err(|e| e.to_string()));
    let atlas = match tai {
        Ok(t) => GrassAtlas::from_tai(&t),
        Err(e) => {
            warn!("Battle-map grass not drawn: {path}: {e}");
            return;
        }
    };
    if atlas.is_empty() {
        warn!("Battle-map grass not drawn: {path} has no pictures");
        return;
    }
    let library = MaterialLibrary::load(&GameFiles { vfs });
    let look = GrassLook::from_material(library.get(GRASS_MATERIAL));
    let jitter = match load_unorm(vfs, JITTER_MAP, ImageAddressMode::Repeat) {
        Ok(i) => images.add(i),
        Err(e) => {
            // A flat 0.5: the fade loses its dissolve pattern but keeps its distance.
            warn!("Battle-map grass: {e}; fading without the jitter pattern");
            images.add(super::image_with_mips(1, 1, vec![128; 4], true))
        }
    };
    let (sun_dir, sun, ambient) = lighting_params(&map);
    let params = GrassParams {
        sun_dir: sun_dir.extend(0.0),
        sun_colour: sun.extend(1.0),
        ambient: ambient.extend(1.0),
        shape: Vec4::new(look.spread, look.rise, look.lift, look.view_lean),
        fade: Vec4::new(look.fade_distance, look.fade_spread, look.fade_start, look.fade_width),
        fade2: Vec4::new(look.fade_height_weight, look.dissolve_start, look.alpha_gain, look.cutoff),
        sway: Vec4::new(look.sway_amplitude, look.sway_speed, look.sway_speed_spread, look.sway_phase),
        gust: Vec4::new(0.0, 0.0, 0.0, look.gust_scale * look.grass_wind_scale),
        map: Vec4::new(size[0], size[1], ground.tile_repeat, 0.0),
    };
    // One material per page; an entry whose page does not load is never drawn.
    let mut pages: Vec<String> = Vec::new();
    let mut entry_page = Vec::with_capacity(atlas.entries.len());
    let mut handles = Vec::new();
    let mut page_ok = Vec::new();
    for e in &atlas.entries {
        let key = e.page.to_ascii_lowercase();
        let i = match pages.iter().position(|p| *p == key) {
            Some(i) => i,
            None => {
                let page_path = format!("{ATLAS_FOLDER}{key}");
                let material = load_unorm(vfs, &page_path, ImageAddressMode::ClampToEdge).map_err(|e| warn!("Battle-map grass: {e}")).ok().map(|img| {
                    materials.add(GrassMaterial {
                        params,
                        atlas: images.add(img),
                        colour_map: ground.colour0.clone(),
                        tile_map: ground.tile.clone(),
                        jitter: jitter.clone(),
                    })
                });
                page_ok.push(material.is_some());
                handles.push(material.unwrap_or_default());
                pages.push(key);
                pages.len() - 1
            }
        };
        entry_page.push(if page_ok[i] { i } else { usize::MAX });
    }
    info!(
        "Battle-map grass: {} pictures of {grass_type} on {} page(s), grass map {}x{}, {:?}",
        atlas.entries.len(),
        pages.len(),
        grass_map.width,
        grass_map.height,
        quality
    );
    commands.insert_resource(GrassField {
        map,
        grass_map,
        atlas,
        quality,
        size,
        look,
        entry_page,
        materials: handles,
        window: None,
        cells: HashMap::new(),
        queue: Vec::new(),
        scratch: Vec::new(),
        gust: 0.0,
        applied_gust: 0.0,
    });
}

/// The ground's normal at a battle position (Bevy axes), from the heights 1 m either side.
fn ground_normal(map: &BattleMap, x: f32, y: f32) -> [f32; 3] {
    let gx = (map.height_at(x + 1.0, y) - map.height_at(x - 1.0, y)) * 0.5;
    let gy = (map.height_at(x, y + 1.0) - map.height_at(x, y - 1.0)) * 0.5;
    Vec3::new(-gx, 1.0, gy).normalize().to_array()
}

/// One page's blades of a cell as a mesh, and its bounds (the quads are spread in the shader, so
/// the box is grown by the largest blade on every side).
fn cell_mesh(field: &GrassField, page: usize) -> Option<(Mesh, Aabb)> {
    let blades = field.scratch.iter().filter(|b| field.entry_page[b.entry] == page);
    let n = blades.clone().count();
    if n == 0 {
        return None;
    }
    let mut positions = Vec::with_capacity(n * 4);
    let mut normals = Vec::with_capacity(n * 4);
    let mut uvs = Vec::with_capacity(n * 4);
    let mut sizes = Vec::with_capacity(n * 4);
    let mut weights = Vec::with_capacity(n * 4);
    let mut indices = Vec::with_capacity(n * 6);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut reach = 0.0f32;
    for b in blades {
        let foot = Vec3::new(b.x, b.ground, -b.y);
        lo = lo.min(foot);
        hi = hi.max(foot);
        reach = reach.max(b.height).max(b.width);
        let normal = ground_normal(&field.map, b.x, b.y);
        let (mirrored, phase) = blade_look(b.rand);
        let entry = &field.atlas.entries[b.entry];
        let v = positions.len() as u32;
        for k in 0..4 {
            let w = field.look.corner_weights(k);
            positions.push(foot.to_array());
            normals.push(normal);
            uvs.push(corner_uv(entry, k, mirrored));
            sizes.push([b.width, b.height]);
            weights.push([w[0], w[1], phase as f32 / 255.0, 0.0]);
        }
        indices.extend_from_slice(&[v, v + 1, v + 3, v, v + 3, v + 2]);
    }
    // The furthest a corner can move from its foot: the spread quad, the lift and the sway.
    let look = &field.look;
    let grow = reach * (look.spread * 0.5 + look.rise) + look.lift + 1.0;
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, sizes);
    mesh.insert_attribute(ATTRIBUTE_WEIGHT, weights);
    mesh.insert_indices(Indices::U32(indices));
    Some((mesh, Aabb::from_min_max(lo - Vec3::splat(grow), hi + Vec3::splat(grow))))
}

/// Each frame: keeps the cells of the camera's window built (a few per frame, nearest first) and
/// drops those that left it; advances the gust.
pub fn update_grass(
    mut commands: Commands,
    field: Option<ResMut<GrassField>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    time: Res<Time>,
    sim: Option<Res<crate::battle::BattleSim>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    let Some(mut field) = field else { return };
    let field = &mut *field;
    if let Some(cam) = camera.iter().next() {
        let c = cam.translation();
        let window = cell_window(&field.placement(), c.x, -c.z, 0);
        if window != field.window {
            field.window = window;
            let inside = |k: &[u32; 2]| window.is_some_and(|(a, b)| (a[0]..=b[0]).contains(&k[0]) && (a[1]..=b[1]).contains(&k[1]));
            field.cells.retain(|k, es| {
                let keep = inside(k);
                if !keep {
                    es.iter().for_each(|e| commands.entity(*e).despawn());
                }
                keep
            });
            field.queue.clear();
            if let Some((a, b)) = window {
                for cz in a[1]..=b[1] {
                    for cx in a[0]..=b[0] {
                        if !field.cells.contains_key(&[cx, cz]) {
                            field.queue.push([cx, cz]);
                        }
                    }
                }
            }
            // Nearest last: the queue is built from its back.
            let s = field.quality.cell_size;
            let (w, h) = (field.size[0], field.size[1]);
            let at = |k: &[u32; 2]| Vec2::new((k[0] as f32 + 0.5) * s - w / 2.0, (k[1] as f32 + 0.5) * s - h / 2.0);
            let cam = Vec2::new(c.x, c.z);
            field.queue.sort_by(|p, q| at(q).distance_squared(cam).total_cmp(&at(p).distance_squared(cam)));
        }
    }
    for _ in 0..CELLS_PER_FRAME {
        let Some(k) = field.queue.pop() else { break };
        let mut blades = std::mem::take(&mut field.scratch);
        blades.clear();
        let map = field.map.clone();
        place_cell(&field.placement(), k[0], k[1], 0, |x, y| map.height_at(x, y), &mut blades);
        field.scratch = blades;
        let mut es = Vec::new();
        for page in 0..field.materials.len() {
            let Some((mesh, aabb)) = cell_mesh(field, page) else { continue };
            es.push(
                commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(field.materials[page].clone()),
                        Transform::default(),
                        aabb,
                        NoAutoAabb,
                    ))
                    .id(),
            );
        }
        field.cells.insert(k, es);
    }
    // The gust: grows with the wind and decays (CONFIRMED form, 0x01247E90). PROVISIONAL inputs:
    // the push is the battle's wind speed, the level its strength on the trees' 1..100 scale.
    let (strength, dir) = sim.as_deref().map_or((0.0, [0.0; 3]), |s| super::speedtree::battle_wind_target(s.wind));
    let speed = strength * WIND_LADDER[WIND_LADDER.len() - 1];
    field.gust = advance_gust(field.gust, speed, 1.0 + 99.0 * strength, time.delta_secs(), &field.look);
    if (field.gust - field.applied_gust).abs() > 1e-3 * field.applied_gust.max(1.0) {
        field.applied_gust = field.gust;
        // The shader's wind is the wind's unit direction (+x when there is none), CONFIRMED
        // 0x01248580; its grass_wind_scale factor is in `gust.w`. PROVISIONAL: the axes.
        let horizontal = Vec2::new(dir[0], -dir[1]).try_normalize().unwrap_or(Vec2::X);
        for h in &field.materials {
            if let Some(mut m) = materials.get_mut(h) {
                m.params.gust = Vec4::new(horizontal.x, horizontal.y, field.gust, m.params.gust.w);
            }
        }
    }
}

/// Removes the grass when the battle ends.
pub fn clear_grass(mut commands: Commands, field: Option<ResMut<GrassField>>) {
    if let Some(mut field) = field {
        for (_, es) in field.cells.drain() {
            for e in es {
                commands.entity(e).despawn();
            }
        }
    }
    commands.remove_resource::<GrassField>();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no preferences file the grass draws at the setting the options page shows (the
    /// registered default 2: 24² blades per 16 m cell); the file's value wins; 0 turns it off.
    #[test]
    fn the_grass_follows_the_setting_the_options_page_shows() {
        use ntw_formats::preferences::Preferences;
        let none = Preferences::default();
        assert_eq!(option_from(&none), 2);
        assert_eq!(quality_for_option(option_from(&none)).map(|q| q.blades_per_side), Some(24));
        assert_eq!(option_from(&Preferences::parse("gfx_grass_quality 4; # #\r\n")), 4);
        assert_eq!(quality_for_option(option_from(&Preferences::parse("gfx_grass_quality 0; # #\r\n"))), None);
    }

    /// Needs the install. The presets whose grass map is a `.jpg` (the original's loader tries it
    /// before `.tga`, 0x008D1340) load it with its `_alpha.jpg`: hb_nile among them; each alpha is
    /// the colour image's size (so the original's unchecked alpha copy, an ORIGINAL BUG we bound,
    /// changes no vanilla map) and gives the map a real alpha (g3), not the JPEG's 255.
    #[test]
    #[ignore]
    fn every_jpeg_grass_map_loads() {
        let vfs = Vfs::open_install(crate::config::game_data_dir()).expect("install");
        let nile = BattleMap::load(&vfs, "hb_nile").unwrap();
        let file = find_grass_map(&nile.folder, |p| vfs.contains(p)).expect("hb_nile has a grass map");
        assert!(file.path.ends_with(r"grassmap.jpg"), "{}", file.path);
        assert!(file.alpha.as_deref().is_some_and(|a| a.ends_with(r"grassmap_alpha.jpg")), "{file:?}");
        let mut jpgs = 0;
        for name in ntw_formats::battle_terrain::list_presets(&vfs) {
            let map = BattleMap::load(&vfs, &name).unwrap();
            let Some(file) = find_grass_map(&map.folder, |p| vfs.contains(p)) else { continue };
            if !file.path.ends_with(".jpg") {
                continue;
            }
            jpgs += 1;
            let colour = read_image(&vfs, &file.path).unwrap();
            let alpha = read_image(&vfs, file.alpha.as_deref().expect("an _alpha.jpg")).unwrap();
            assert_eq!((alpha.width, alpha.height), (colour.width, colour.height), "{name}");
            let loaded = load_grass_map(&vfs, &map.folder).unwrap_or_else(|| panic!("{name}: not loaded"));
            assert_eq!((loaded.width, loaded.height), (colour.width, colour.height), "{name}");
            let alphas: std::collections::BTreeSet<u8> = loaded.rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
            println!("{name:32} {}x{}, {} alpha values", loaded.width, loaded.height, alphas.len());
            assert!(alphas.iter().any(|&a| a != 255), "{name}: the alpha was not applied");
        }
        assert_eq!(jpgs, 6);
    }
}

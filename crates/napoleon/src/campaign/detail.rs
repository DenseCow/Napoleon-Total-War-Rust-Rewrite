//! Finer supertexture near the camera. The terrain is drawn with one supertexture level for the
//! whole map (at most 4,096 px wide, 3.2 px per logic unit on Europe). When the camera comes close,
//! a patch of the same terrain grid around the point looked at is drawn over it with a finer level:
//! level 1 (12.8 px per unit) within distance 50 and level 0 (25.6 px per unit) within 25. The
//! tiles are read from `supertexture.stpd` one by one (seek and read, then inflate and DXT5-decode)
//! on the async compute pool, and the patch is swapped in when its window is complete.
//!
//! The original streams its "virtual texture" by the same tiles (the `.stpi` index has every level);
//! its own switching distances are not decoded (PROVISIONAL: the distances above).


use std::io::{Read, Seek, SeekFrom};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ntw_formats::campaign_map::{CampaignMap, HEIGHT_SCALE, StpTile, SuperTexture};

use super::camera::CampaignCamera;
use super::scene::{CampaignGround, TERRAIN_STRIDE, bevy_pos, image_with_mips};
use crate::GameMode;




/// The level and window size (tiles per side) for a camera distance, or `None` beyond the last.
fn level_for(distance: f32) -> Option<(usize, u32)> {
    if distance < 25.0 {
        Some((0, 4))
    } else if distance < 50.0 {
        Some((1, 3))
    } else {
        None
    }
}

/// A requested window: level, first tile column and row, tiles per side.
type Window = (usize, u32, u32, u32);

/// A built window: its image and its patch mesh.
type Built = (Image, Mesh);

/// The patch currently drawn and the one being built.
#[derive(Default)]
pub struct DetailState {
    shown: Option<(Window, Entity)>,
    building: Option<(Window, Task<Option<Built>>)>,
}

/// Keeps the finer patch under the camera (see the module docs). The window is read, decoded and
/// assembled on the async compute pool, so moving the camera does not stall a frame.
pub fn update(
    mut state: Local<DetailState>,
    ground: Option<Res<CampaignGround>>,
    cams: Query<&CampaignCamera>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(ground), Ok(cam)) = (ground, cams.single()) else { return };
    let map = &ground.0;
    let Some(st) = map.supertexture.as_ref() else { return };
    let want = level_for(cam.distance).and_then(|(level, n)| {
        let lv = st.levels.get(level)?;
        let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
        let (x, z) = (cam.target.x, -cam.target.y);
        let tx = ((x - mn.0) / (mx.0 - mn.0) * lv.tiles_x as f32).floor() as i64 - (n as i64 - 1) / 2;
        let ty = ((mx.1 - z) / (mx.1 - mn.1) * lv.tiles_y as f32).floor() as i64 - (n as i64 - 1) / 2;
        let tx = tx.clamp(0, (lv.tiles_x as i64 - n as i64).max(0)) as u32;
        let ty = ty.clamp(0, (lv.tiles_y as i64 - n as i64).max(0)) as u32;
        Some((level, tx, ty, n))
    });
    let Some(want) = want else {
        if let Some((_, e)) = state.shown.take() {
            commands.entity(e).despawn();
        }
        state.building = None;
        return;
    };
    if state.shown.map(|s| s.0) == Some(want) {
        state.building = None;
        return;
    }
    // Start (or restart) building the wanted window.
    if state.building.as_ref().map(|b| b.0) != Some(want) {
        let map = ground.0.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move { build_window(&map, want) });
        state.building = Some((want, task));
    }
    let Some(result) = state.building.as_mut().and_then(|b| check_ready(&mut b.1)) else { return };
    state.building = None;
    let Some((image, mesh)) = result else { return };
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(images.add(image)),
        perceptual_roughness: 1.0,
        reflectance: 0.05,
        depth_bias: 10.0,
        ..default()
    });
    let entity = commands
        .spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material), Transform::from_xyz(0.0, 0.004, 0.0), Name::new("campaign terrain detail"), DespawnOnExit(GameMode::Campaign)))
        .id();
    if let Some((_, old)) = state.shown.replace((want, entity)) {
        commands.entity(old).despawn();
    }
}

/// Reads and decodes a window's tiles and builds its image and mesh (runs on the task pool).
fn build_window(map: &CampaignMap, (level, tx0, ty0, n): Window) -> Option<Built> {
    let st = map.supertexture.as_ref()?;
    let lv = st.levels.get(level)?;
    let mut file = None;
    let ts = st.tile_size as usize;
    let side = n as usize * ts;
    let mut rgba = vec![0u8; side * side * 4];
    for ty in ty0..ty0 + n {
        for tx in tx0..tx0 + n {
            let px = read_tile(&mut file, map, st, level, tx, ty).unwrap_or_else(|e| {
                warn!("Campaign supertexture tile {level}/{tx},{ty}: {e}");
                vec![128; ts * ts * 4]
            });
            let (ox, oy) = ((tx - tx0) as usize * ts, (ty - ty0) as usize * ts);
            for row in 0..ts {
                let dst = ((oy + row) * side + ox) * 4;
                rgba[dst..dst + ts * 4].copy_from_slice(&px[row * ts * 4..(row + 1) * ts * 4]);
            }
        }
    }
    rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
    let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
    let (tw, th) = ((mx.0 - mn.0) / lv.tiles_x as f32, (mx.1 - mn.1) / lv.tiles_y as f32);
    let (x0, z_top) = (mn.0 + tx0 as f32 * tw, mx.1 - ty0 as f32 * th);
    let (x1, z_bottom) = (x0 + n as f32 * tw, z_top - n as f32 * th);
    Some((image_with_mips(side as u32, side as u32, rgba), patch_mesh(map, x0, x1, z_bottom, z_top)))
}

/// Reads one tile of a level from `supertexture.stpd` (loose file; seek and read only its bytes) and
/// decodes it to RGBA8.
fn read_tile(file: &mut Option<std::fs::File>, map: &CampaignMap, st: &SuperTexture, level: usize, tx: u32, ty: u32) -> Result<Vec<u8>, String> {
    let lv = st.levels.get(level).ok_or("no such level")?;
    let index = (ty * lv.tiles_x + tx) as usize;
    let tile = lv.tiles.get(index).ok_or("no such tile")?;
    if file.is_none() {
        let path = crate::config::game_data_dir().join(format!("campaign_maps\\{}\\display\\supertexture\\supertexture.stpd", map.name));
        *file = Some(std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?);
    }
    let f = file.as_mut().expect("opened above");
    let mut buf = vec![0u8; tile.size as usize];
    f.seek(SeekFrom::Start(u64::from(tile.offset))).map_err(|e| e.to_string())?;
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    let raw = SuperTexture::inflate_tile(&buf, &StpTile { offset: 0, ..*tile }, index).map_err(|e| e.to_string())?;
    Ok(st.decode_tile_rgba(&raw))
}

/// The terrain grid (the same vertices as `scene::spawn_terrain`, so the patch lies exactly on the
/// terrain) between logic x0..x1 and z_bottom..z_top, with UVs over that window.
fn patch_mesh(map: &CampaignMap, x0: f32, x1: f32, z_bottom: f32, z_top: f32) -> Mesh {
    let hm = &map.heightmap;
    let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
    let (cols, rows) = (hm.width / TERRAIN_STRIDE + 1, hm.height / TERRAIN_STRIDE + 1);
    let (sx, sz) = ((mx.0 - mn.0) / (cols - 1) as f32, (mx.1 - mn.1) / (rows - 1) as f32);
    let height = |c: i64, r: i64| {
        let (c, r) = (c.clamp(0, cols as i64 - 1), r.clamp(0, rows as i64 - 1));
        hm.sample(c as f32 / (cols - 1) as f32 * hm.width as f32, r as f32 / (rows - 1) as f32 * hm.height as f32) * HEIGHT_SCALE
    };
    let c0 = (((x0 - mn.0) / sx).floor() as i64).max(0);
    let c1 = (((x1 - mn.0) / sx).ceil() as i64).min(cols as i64 - 1);
    let r0 = (((mx.1 - z_top) / sz).floor() as i64).max(0);
    let r1 = (((mx.1 - z_bottom) / sz).ceil() as i64).min(rows as i64 - 1);
    let (w, h) = ((c1 - c0 + 1) as u32, (r1 - r0 + 1) as u32);
    let (mut pos, mut nor, mut uv) = (Vec::new(), Vec::new(), Vec::new());
    for r in r0..=r1 {
        for c in c0..=c1 {
            let (u, v) = (c as f32 / (cols - 1) as f32, r as f32 / (rows - 1) as f32);
            let x = mn.0 + u * (mx.0 - mn.0);
            let z = mx.1 - v * (mx.1 - mn.1);
            pos.push(bevy_pos(x, height(c, r), z).to_array());
            let gx = (height(c + 1, r) - height(c - 1, r)) / (2.0 * sx);
            let gz = (height(c, r + 1) - height(c, r - 1)) / (2.0 * sz);
            nor.push(Vec3::new(-gx, 1.0, -gz).normalize().to_array());
            uv.push([(x - x0) / (x1 - x0), (z_top - z) / (z_top - z_bottom)]);
        }
    }
    let mut idx = Vec::new();
    for r in 0..h - 1 {
        for c in 0..w - 1 {
            let (a, b, d, e) = (r * w + c, r * w + c + 1, (r + 1) * w + c, (r + 1) * w + c + 1);
            idx.extend_from_slice(&[a, d, b, b, d, e]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_indices(Indices::U32(idx));
    mesh
}

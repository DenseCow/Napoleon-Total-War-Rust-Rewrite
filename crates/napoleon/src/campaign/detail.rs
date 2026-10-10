//! Finer supertexture near the camera. The terrain is drawn with one supertexture level for the
//! whole map (at most 4,096 px wide, 3.2 px per logic unit on Europe). When the camera comes close,
//! a patch of the same terrain grid around the point looked at is drawn over it with a finer level:
//! level 1 (12.8 px per unit) within distance 50 and level 0 (25.6 px per unit) within 25. The
//! window's tiles are read from the map's ground texture (`MapDisplay::ground`; for the original's
//! maps the `supertexture.stpd` tiles: seek and read, then inflate and DXT5-decode) on the async
//! compute pool, and the patch is swapped in when its window is complete. A window that cannot be
//! read is logged once, and the finer patch then stays off for that map.
//!
//! The original streams its "virtual texture" by the same tiles (the `.stpi` index has every level);
//! its own switching distances are not decoded (PROVISIONAL: the distances above).

use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ntw_campaign::map_display::MapDisplay;

use super::camera::CampaignCamera;
use super::scene::{CampaignGround, TerrainGrid, image_with_mips};
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
    building: Option<(Window, Task<Result<Built, String>>)>,
    /// The map whose ground texture could not be read: its patch stays off (logged once).
    failed: Option<String>,
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
    if state.failed.as_deref() == Some(map.key.as_str()) {
        return;
    }
    let Some(tex) = map.ground.as_deref() else { return };
    let want = level_for(cam.distance).and_then(|(level, n)| {
        let &(tiles_x, tiles_y) = tex.levels().get(level)?;
        let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
        let (x, z) = (cam.target.x, -cam.target.y);
        let tx = ((x - mn.0) / (mx.0 - mn.0) * tiles_x as f32).floor() as i64 - (n as i64 - 1) / 2;
        let ty = ((mx.1 - z) / (mx.1 - mn.1) * tiles_y as f32).floor() as i64 - (n as i64 - 1) / 2;
        let tx = tx.clamp(0, (tiles_x as i64 - n as i64).max(0)) as u32;
        let ty = ty.clamp(0, (tiles_y as i64 - n as i64).max(0)) as u32;
        Some((level, tx, ty, n.min(tiles_x).min(tiles_y)))
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
    let (image, mesh) = match result {
        Ok(built) => built,
        Err(e) => {
            warn!("Campaign ground texture: {e}; the finer patch near the camera stays off");
            state.failed = Some(ground.0.key.clone());
            return;
        }
    };
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
fn build_window(map: &MapDisplay, (level, tx0, ty0, n): Window) -> Result<Built, String> {
    let tex = map.ground.as_deref().ok_or("the map has no ground texture")?;
    let &(tiles_x, tiles_y) = tex.levels().get(level).ok_or_else(|| format!("no ground texture level {level}"))?;
    let mut rgba = tex.window_rgba(level, tx0, ty0, n, n)?;
    rgba.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p[3] = 255);
    let side = n * tex.tile_size();
    let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
    let (tw, th) = ((mx.0 - mn.0) / tiles_x as f32, (mx.1 - mn.1) / tiles_y as f32);
    let (x0, z_top) = (mn.0 + tx0 as f32 * tw, mx.1 - ty0 as f32 * th);
    let (x1, z_bottom) = (x0 + n as f32 * tw, z_top - n as f32 * th);
    Ok((image_with_mips(side, side, rgba), patch_mesh(map, x0, x1, z_bottom, z_top)))
}

/// The terrain grid's vertices ([`TerrainGrid`], the terrain mesh's own, so the patch lies exactly on
/// the terrain) between logic x0..x1 and z_bottom..z_top, with UVs over that window.
fn patch_mesh(map: &MapDisplay, x0: f32, x1: f32, z_bottom: f32, z_top: f32) -> Mesh {
    let grid = TerrainGrid::new(map);
    let (mn, mx) = (map.regions.bounds_min, map.regions.bounds_max);
    let c0 = (((x0 - mn.0) / grid.sx).floor() as i64).max(0);
    let c1 = (((x1 - mn.0) / grid.sx).ceil() as i64).min(grid.cols as i64 - 1);
    let r0 = (((mx.1 - z_top) / grid.sz).floor() as i64).max(0);
    let r1 = (((mx.1 - z_bottom) / grid.sz).ceil() as i64).min(grid.rows as i64 - 1);
    grid.mesh(c0..=c1, r0..=r1, |v| [(v.x - x0) / (x1 - x0), (z_top - v.z) / (z_top - z_bottom)])
}

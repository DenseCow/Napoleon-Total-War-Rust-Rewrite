//! Placed battle-map buildings and props (`bmd_near_buildings.building_list`,
//! `bmd_far_buildings.building_list`), drawn with the original `.rigid_model` files.
//!
//! Key → model: INFERRED from the pack layout, `rigidmodels\buildings\<key>\<key>_pieceNN_
//! destruct01_lod01.rigid_model` (every piece, intact state, most detailed LOD). The game's own
//! route probably goes through the `battlefield_buildings` DB table (not decoded here).
//! Rotation: `Angle` u16 → −θ about Bevy's Y (mirroring the D3D Z flip); checked visually on
//! hb_waterloo: rotated houses sit exactly on their footprints baked into the colour map
//! (a 180° turn would look the same, so that part is UNKNOWN).
//! PROVISIONAL:
//! no LOD switching, no destruction states, no trees yet (see `BattleMap::trees`).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::render_resource::Face;
use ntw_formats::rigid_model::RigidModel;

use super::current_map;
use crate::model_viewer::load_dds;
use crate::model_viewer::source::{ModelSource, NameFlags, convert_mesh};

/// A model's meshes with their materials.
pub type Parts = Vec<(Handle<Mesh>, Handle<StandardMaterial>)>;

/// The `.rigid_model` files of one building key (all pieces, intact, LOD 1).
fn model_paths(src: &ModelSource, key: &str) -> Vec<String> {
    let dir = format!("rigidmodels\\buildings\\{}\\", key.to_ascii_lowercase());
    let mut paths: Vec<String> = src
        .list_models(&dir)
        .into_iter()
        .filter(|p| p.contains("_destruct01_lod01"))
        .map(str::to_owned)
        .collect();
    if paths.is_empty() {
        paths.extend(src.find_model(&dir));
    }
    paths
}

fn load_parts(
    src: &ModelSource,
    key: &str,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    textures: &mut HashMap<String, Handle<Image>>,
) -> Parts {
    load_model_parts(src, &model_paths(src, key), meshes, materials, images, textures)
}

/// The meshes and materials of the given `.rigid_model` files (textures shared through `textures`).
pub fn load_model_parts(
    src: &ModelSource,
    paths: &[String],
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    textures: &mut HashMap<String, Handle<Image>>,
) -> Parts {
    let mut parts = Vec::new();
    for path in paths {
        let model = match src.vfs.read(path).map_err(|e| e.to_string()).and_then(|b| RigidModel::read(&b).map_err(|e| e.to_string())) {
            Ok(m) => m,
            Err(e) => {
                warn!("{path}: {e}");
                continue;
            }
        };
        let flags = NameFlags::from_path(path);
        for rm in &model.meshes {
            let arrays = convert_mesh(rm);
            let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, arrays.positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, arrays.normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, arrays.uvs);
            mesh.insert_indices(bevy::mesh::Indices::U32(arrays.indices));
            let texture = rm.material.diffuse_name().and_then(|n| src.find_texture(path, &n)).and_then(|hit| {
                let p = hit.path().to_owned();
                if let Some(h) = textures.get(&p) {
                    return Some(h.clone());
                }
                let h = images.add(load_dds(src, &p).map_err(|e| warn!("{p}: {e}")).ok()?);
                textures.insert(p, h.clone());
                Some(h)
            });
            let material = StandardMaterial {
                base_color: if texture.is_some() { Color::WHITE } else { Color::srgb(0.6, 0.6, 0.6) },
                base_color_texture: texture,
                perceptual_roughness: 0.9,
                reflectance: 0.15,
                double_sided: flags.two_sided,
                cull_mode: if flags.two_sided { None } else { Some(Face::Back) },
                alpha_mode: if flags.alpha_blend {
                    AlphaMode::Blend
                } else if flags.alpha_test {
                    AlphaMode::Mask(0.5)
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            };
            parts.push((meshes.add(mesh), materials.add(material)));
        }
    }
    parts
}

/// Startup: one entity per placed building, with its model parts as children.
pub fn spawn_buildings(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(map) = current_map() else { return };
    let map = &map;
    if map.buildings_near.is_empty() && map.buildings_far.is_empty() {
        return;
    }
    let src = match ModelSource::open(&crate::config::game_data_dir()) {
        Ok(s) => s,
        Err(e) => {
            warn!("Battle-map buildings unavailable: {e}");
            return;
        }
    };
    let mut by_key: HashMap<String, Parts> = HashMap::new();
    let mut textures = HashMap::new();
    let mut missing = Vec::new();
    let mut count = 0;
    for b in map.buildings_near.iter().chain(&map.buildings_far) {
        let key = b.key.to_ascii_lowercase();
        let parts = by_key
            .entry(key.clone())
            .or_insert_with(|| load_parts(&src, &key, &mut meshes, &mut materials, &mut images, &mut textures));
        if parts.is_empty() {
            missing.push(key);
            continue;
        }
        let origin = super::ground_point(Vec2::new(b.position.0, b.position.1));
        let transform = Transform::from_translation(origin)
            .with_rotation(Quat::from_rotation_y(-b.angle_radians()));
        commands
            .spawn((transform, Visibility::default(), Name::new(format!("building {}", b.key))))
            .with_children(|c| {
                for (mesh, material) in parts.iter() {
                    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
                }
            });
        count += 1;
    }
    missing.sort();
    missing.dedup();
    info!("Battle map: {count} buildings placed ({} models)", by_key.len());
    if !missing.is_empty() {
        warn!("Battle map: no model found for {} building keys: {:?}", missing.len(), missing);
    }
}

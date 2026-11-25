//! The original's markers under the men of a selected or hovered unit (BATTLE_FIDELITY.md §59.9).
//!
//! CONFIRMED static (`0x00789BD0`, the soldier display's per-frame update): for every living,
//! shown man (entity `+0xCC` = 0, `+0xBA` = 1) the decal manager (`0x00794B10`, vfunc `+0x60`)
//! lays a flat textured square at his position (entity `+0xA8..+0xB0`), turned to his heading
//! (`+0xB8`):
//! - his unit selected (the display's `+0x88`): the `entity_selection_marker`, 1 × 1 m, height
//!   argument 0.03;
//! - his unit under the mouse (`+0x89`): 1.5 × 1.5 m, height argument 0.045; the
//!   `entity_mouse_over_marker` for a unit of the player's alliance (army `+0x44` → `+0x10` equal
//!   to the battle's `+0x281A4`), else the `entity_mouse_over_enemy_marker`. Both can show at once.
//!
//! The markers' textures (`0x0118CE90`): `Indicators/selector_units_selected_diffuse`,
//! `selector_units_mouse_over_diffuse`, `selector_units_mouse_over_enemy_diffuse` (a disc with a
//! point at the man's front; the selected one filled, the mouse-over one an outline, the enemy one
//! red). The marker handles are made once per battle (`0x007159D0`).
//!
//! Ours: the view (`super::view::sync_views`) hands every marked man's drawn position and facing
//! to [`MarkerQuads`]; [`draw`] writes them into one mesh per marker kind when they changed (a
//! standing unit's stay as they are), reusing its buffers. Not ported: the original clamps the entity's height to at least 0.1
//! (`0x00789BD0`), in a frame not traced against our terrain heights (ours can be below 0). PROVISIONAL: the height argument is taken as the square's lift above the ground under
//! the man (INFERRED from the sizes of the two); the decal is a flat square, not projected on
//! the slope; the unit under the mouse is the one whose centre is nearest the ground point
//! (`input::unit_near`, the original picks a soldier).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use ntw_formats::pack::Vfs;

use crate::GameMode;

/// The three markers: texture, side of the square (m), lift above the ground (m).
const KINDS: [(&str, f32, f32); 3] = [
    ("indicators/selector_units_selected_diffuse.dds", 1.0, 0.03),
    ("indicators/selector_units_mouse_over_diffuse.dds", 1.5, 0.045),
    ("indicators/selector_units_mouse_over_enemy_diffuse.dds", 1.5, 0.045),
];

/// Which marker a man gets ([`MarkerQuads::push`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Selected = 0,
    MouseOver = 1,
    MouseOverEnemy = 2,
}

/// This frame's marked men, per marker: the ground point under the man (world) and his facing
/// (the model's radians). Filled by the view, emptied by [`draw`].
#[derive(Resource, Default)]
pub struct MarkerQuads {
    marks: [Vec<(Vec3, f32)>; 3],
    /// The marks the meshes hold now (written by [`draw`]).
    written: [Vec<(Vec3, f32)>; 3],
}

impl MarkerQuads {
    pub fn push(&mut self, marker: Marker, ground: Vec3, facing: f32) {
        self.marks[marker as usize].push((ground, facing));
    }
}

/// The marker meshes, one per kind (`None` where the texture did not load).
#[derive(Resource, Default)]
pub struct MarkerMeshes {
    meshes: [Option<(Entity, Handle<Mesh>)>; 3],
}

/// Loads the three textures and makes one empty mesh per marker. A texture that does not load is
/// logged once and its marker is not drawn.
pub fn enter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(MarkerQuads::default());
    let mut made = MarkerMeshes::default();
    let vfs = match Vfs::open_install(crate::config::game_data_dir()) {
        Ok(v) => v,
        Err(e) => {
            warn!("Unit markers: install not readable ({e}); no selection markers");
            commands.insert_resource(made);
            return;
        }
    };
    for (k, (path, _, _)) in KINDS.iter().enumerate() {
        let image = match vfs.read(path).map_err(|e| e.to_string()).and_then(|b| crate::campaign::scene::dds_image(&b)) {
            Ok(i) => i,
            Err(e) => {
                warn!("Unit markers: {path}: {e}; that marker is not drawn");
                continue;
            }
        };
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(image)),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        });
        // One square to start with: the renderer allocates no empty mesh (it is hidden until used).
        let handle = meshes.add(quad_mesh(&[(Vec3::ZERO, 0.0)], 1.0, 0.0));
        let entity = commands
            .spawn((
                Mesh3d(handle.clone()),
                MeshMaterial3d(material),
                Transform::IDENTITY,
                Visibility::Hidden,
                // The squares are rewritten in world space whenever the marks change: bounds from the
                // first write would go stale and cull them.
                bevy::camera::visibility::NoFrustumCulling,
                bevy::light::NotShadowCaster,
                DespawnOnExit(GameMode::Battle),
            ))
            .id();
        made.meshes[k] = Some((entity, handle));
    }
    commands.insert_resource(made);
}

/// The four corners of a marker square of side `size` at `ground` turned to `facing` (the
/// model's radians: 0 = +x, the world's x; the model's y is the world's −z), in the order
/// front-left, front-right, back-right, back-left: the texture's top (its point) at the front.
pub fn corners(ground: Vec3, facing: f32, size: f32) -> [Vec3; 4] {
    let front = Vec3::new(facing.cos(), 0.0, -facing.sin()) * (size * 0.5);
    // The man's right: the model's (sin, −cos), the world's (sin, 0, cos).
    let right = Vec3::new(facing.sin(), 0.0, facing.cos()) * (size * 0.5);
    [ground + front - right, ground + front + right, ground - front + right, ground - front - right]
}

/// Writes this frame's marked men into the marker meshes (in place, reusing the buffers) and
/// shows the meshes that have any.
pub fn draw(mut quads: ResMut<MarkerQuads>, made: Option<Res<MarkerMeshes>>, mut meshes: ResMut<Assets<Mesh>>, mut visibility: Query<&mut Visibility>) {
    let Some(made) = made else { return };
    let MarkerQuads { marks, written } = &mut *quads;
    for (k, slot) in made.meshes.iter().enumerate() {
        if let Some((entity, handle)) = slot {
            if let Ok(mut v) = visibility.get_mut(*entity) {
                let wanted = if marks[k].is_empty() { Visibility::Hidden } else { Visibility::Visible };
                v.set_if_neq(wanted);
            }
            // An empty frame only hides the mesh (the renderer allocates no empty mesh), and the
            // same marks as last frame's (a standing unit) leave the mesh as it is.
            if !marks[k].is_empty()
                && marks[k] != written[k]
                && let Some(mut mesh) = meshes.get_mut(handle)
            {
                let (_, size, lift) = KINDS[k];
                write_quads(&mut mesh, &marks[k], size, lift);
                // This frame's marks become the written ones; the old buffer is reused next frame.
                std::mem::swap(&mut marks[k], &mut written[k]);
            }
        }
        marks[k].clear();
    }
}

/// A marker mesh holding one square per mark ([`write_quads`]).
fn quad_mesh(marks: &[(Vec3, f32)], size: f32, lift: f32) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));
    write_quads(&mut mesh, marks, size, lift);
    mesh
}

/// Fills `mesh` with one square per mark, its buffers cleared and refilled in place.
fn write_quads(mesh: &mut Mesh, marks: &[(Vec3, f32)], size: f32, lift: f32) {
    if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        p.clear();
        for &(ground, facing) in marks {
            p.extend(corners(ground + Vec3::Y * lift, facing, size).map(|c| c.to_array()));
        }
    }
    if let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        n.clear();
        n.resize(marks.len() * 4, [0.0, 1.0, 0.0]);
    }
    if let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        uv.clear();
        for _ in marks {
            uv.extend([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        }
    }
    if let Some(Indices::U32(ix)) = mesh.indices_mut() {
        ix.clear();
        for q in 0..marks.len() as u32 {
            let b = q * 4;
            ix.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the texture (its top edge) lies at the man's front, and the square's side is
    /// the marker's size.
    #[test]
    fn a_marker_square_points_to_the_mans_front() {
        // Facing the model's +y (north): the world's −z.
        let c = corners(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 1.0);
        let top = (c[0] + c[1]) * 0.5;
        assert!((top - Vec3::new(0.0, 0.0, -0.5)).length() < 1e-6, "{top}");
        assert!((c[0].distance(c[1]) - 1.0).abs() < 1e-6);
        // Front-left is to the man's left: facing north, his left is west (−x).
        assert!(c[0].x < 0.0 && c[1].x > 0.0);
    }

    /// The meshes are rewritten in place: one square (4 corners, 6 indices) per mark.
    #[test]
    fn the_quads_are_written_per_mark() {
        let mut mesh = quad_mesh(&[(Vec3::ZERO, 0.0)], 1.0, 0.0);
        assert_eq!(mesh.count_vertices(), 4);
        write_quads(&mut mesh, &[(Vec3::ZERO, 0.0), (Vec3::X, 1.0)], 1.5, 0.045);
        assert_eq!((mesh.count_vertices(), mesh.indices().map_or(0, |i| i.len())), (8, 12));
    }
}

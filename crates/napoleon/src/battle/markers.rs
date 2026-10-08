//! Script markers (`battle:marker(name)`): models a battle script places, moves, turns, shows and
//! hides, such as the land tutorial's waypoint flags.
//!
//! The marker object (`0x0064BC10`, CONFIRMED layout) holds a position, a rotation (radians, from
//! the script's degrees), a scale, a visible flag (off when made) and a model from its name. The
//! model file is `rigidmodels\waypointmarkers\<name>\<name>.rigid_model` (INFERRED from the pack
//! layout: the only marker model shipped, `tutorial_flag`, sits there; the name lookup
//! `0x00745320` was not followed). Engine position (x, height, z) → Bevy (x, height, −z), as for
//! the script camera; rotation about the vertical axis (INFERRED sign).

use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;

use crate::model_viewer::source::ModelSource;
use crate::terrain::objects::{Parts, load_model_parts};

/// One marker's state, as the script last set it.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerState {
    pub name: String,
    /// Engine coordinates (x, height, z).
    pub position: [f32; 3],
    pub rotation_deg: f32,
    pub scale: f32,
    pub visible: bool,
}

/// The battle script's markers by id.
#[derive(Resource, Default)]
pub struct ScriptMarkers {
    states: BTreeMap<u32, MarkerState>,
    changed: bool,
}

impl ScriptMarkers {
    /// Sets marker `id`'s state.
    pub fn set(&mut self, id: u32, state: MarkerState) {
        if self.states.get(&id) != Some(&state) {
            self.states.insert(id, state);
            self.changed = true;
        }
    }
}

/// The entity drawing a marker.
#[derive(Component)]
pub struct MarkerView(pub u32);

/// Loaded marker models by name.
#[derive(Default)]
pub struct MarkerModels(HashMap<String, Parts>);

/// Spawns, moves, shows and hides the marker entities.
pub fn sync_markers(
    mut commands: Commands,
    markers: Option<ResMut<ScriptMarkers>>,
    mut views: Query<(&MarkerView, &mut Transform, &mut Visibility)>,
    mut models: Local<MarkerModels>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut markers) = markers else { return };
    if !markers.changed {
        return;
    }
    markers.changed = false;
    let transform = |s: &MarkerState| {
        Transform::from_translation(Vec3::new(s.position[0], s.position[1], -s.position[2]))
            .with_rotation(Quat::from_rotation_y(-s.rotation_deg.to_radians()))
            .with_scale(Vec3::splat(s.scale))
    };
    let visibility = |s: &MarkerState| if s.visible { Visibility::Inherited } else { Visibility::Hidden };
    let mut seen = Vec::new();
    for (view, mut tf, mut vis) in &mut views {
        if let Some(s) = markers.states.get(&view.0) {
            *tf = transform(s);
            *vis = visibility(s);
            seen.push(view.0);
        }
    }
    for (id, s) in &markers.states {
        if seen.contains(id) {
            continue;
        }
        let name = s.name.to_ascii_lowercase();
        if !models.0.contains_key(&name) {
            let parts = match ModelSource::open(&crate::config::game_data_dir()) {
                Ok(src) => {
                    let path = format!(r"rigidmodels\waypointmarkers\{name}\{name}.rigid_model");
                    let mut textures = HashMap::new();
                    load_model_parts(&src, &[path], &mut meshes, &mut materials, &mut images, &mut textures)
                }
                Err(e) => {
                    warn!("Script marker {name}: {e}");
                    Vec::new()
                }
            };
            if parts.is_empty() {
                warn!("Script marker {name}: no model found");
            }
            models.0.insert(name.clone(), parts);
        }
        let parts = &models.0[&name];
        commands
            .spawn((MarkerView(*id), transform(s), visibility(s), DespawnOnExit(crate::GameMode::Battle)))
            .with_children(|c| {
                for (mesh, material) in parts {
                    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
                }
            });
    }
}

/// Leaving the battle: forget the markers (their entities go with the battle's).
pub fn leave(mut commands: Commands) {
    commands.remove_resource::<ScriptMarkers>();
}

/// The entity drawing one deployable defence piece (`ntw_sim::battle::abilities::Defence`).
#[derive(Component)]
pub struct DefenceView;

/// The model file of a defence kind: `rigidmodels\deployableitems\…` (INFERRED from the pack
/// layout; the exe's model names, e.g. `deployable_item_chevaux_de_frise`, were not followed).
fn defence_model(kind: u8) -> Option<&'static str> {
    use ntw_sim::battle::abilities::ability_ids as a;
    Some(match kind {
        a::CHEVAUX_DE_FRISE => r"rigidmodels\deployableitems\cheval_de_frise\cheval_de_frise_piece01_destruct01_lod01.rigid_model",
        a::EARTHWORKS => r"rigidmodels\deployableitems\earthworks\infantry_earthworks_piece01_destruct01_lod01.rigid_model",
        a::GABIONADE => r"rigidmodels\deployableitems\gabionade\gabionade_artillery_center_emplacement_piece01_destruct01_lod01.rigid_model",
        a::WOODEN_STAKES => r"rigidmodels\deployableitems\wooden_stake\ground_stake_lod1.rigid_model",
        a::FOUGASSE_BASIC | a::FOUGASSE_IMPROVED => r"rigidmodels\deployableitems\fougasse\fougasse_piece01_destruct01_lod01.rigid_model",
        _ => return None,
    })
}

/// Draws the model's defence pieces once they exist (built at the end of deployment). Position on
/// the ground; rotation as the map buildings (engine angle `a` → −a about Bevy's Y, with
/// `a = π/2 − facing`; INFERRED).
#[allow(clippy::too_many_arguments)]
pub fn sync_defences(
    mut commands: Commands,
    sim: Res<super::BattleSim>,
    views: Query<Entity, With<DefenceView>>,
    mut drawn: Local<usize>,
    mut models: Local<MarkerModels>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let defences = &sim.battle.defences;
    if defences.len() == *drawn {
        return;
    }
    // Rebuilt from scratch when the list changes (it only grows, or empties on a restart).
    for e in &views {
        commands.entity(e).despawn();
    }
    *drawn = defences.len();
    let src = match ModelSource::open(&crate::config::game_data_dir()) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("Defence models: {e}");
            None
        }
    };
    for d in defences {
        let Some(path) = defence_model(d.kind) else { continue };
        if !models.0.contains_key(path) {
            let parts = src.as_ref().map_or_else(Vec::new, |src| {
                let mut textures = HashMap::new();
                load_model_parts(src, &[path.to_owned()], &mut meshes, &mut materials, &mut images, &mut textures)
            });
            if parts.is_empty() {
                warn!("Defence model {path}: not found");
            }
            models.0.insert(path.to_owned(), parts);
        }
        let h = crate::terrain::ground_height(Vec2::new(d.position.0, d.position.1));
        let tf = Transform::from_translation(Vec3::new(d.position.0, h, -d.position.1))
            .with_rotation(Quat::from_rotation_y(d.facing - std::f32::consts::FRAC_PI_2));
        commands
            .spawn((DefenceView, tf, Visibility::Inherited, DespawnOnExit(crate::GameMode::Battle)))
            .with_children(|c| {
                for (mesh, material) in &models.0[path] {
                    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
                }
            });
    }
    info!("Battle: {} defence pieces drawn", defences.len());
}

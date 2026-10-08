//! Battle-map **trees** (`bmd.tree_list`), drawn with the trees' own 360° billboard pictures.
//!
//! Data route (see `ntw_formats::vegetation` and `analysis/worker5/BATTLE_TERRAIN.md` §9):
//! species → `warscape_trees` → `.spt` → `data.tree_model` (size) + `*_compositemap.txt`
//! (8 billboard rectangles in `textures\<map>_diffuse_billboards.dds`).
//!
//! Drawing: every tree is one camera-facing quad that turns about the vertical (a cylindrical
//! billboard), showing the picture whose view direction is nearest to the camera's. All trees
//! that share a billboard texture are baked into ONE static mesh (4 vertices per tree, the quad
//! is expanded in the vertex shader `trees.wgsl`), so a map's 180,000 trees cost one draw call per
//! texture. The leaves are alpha-tested (no blending, no sorting).
//!
//! # What this pass does not draw, and why (round 11)
//!
//! **Only trees.** A species with an empty `Billboards` section is a **shrub** — CONFIRMED on every
//! shipped preset (`battle_terrain_install::every_tree_species_resolves`: every tree has exactly 8
//! pictures, every shrub has none) — and a shrub has no picture to draw, so it is skipped here.
//! **It is not skipped by the battle map**, though: `speedtree.rs` draws every shrub from its own
//! `.spt` geometry out to [`speedtree::SHRUB_FAR_DISTANCE`], which is the only representation the
//! shipped data offers them. See that module for the survey behind the decision.
//!
//! PROVISIONAL:
//! - the original draws near trees as SpeedTree geometry generated at run time from the `.spt`
//!   (branches, fronds, leaf cards), and the billboards only far away. That geometry generator is
//!   the SpeedTree library's and is UNKNOWN to us, so we show the billboards at every distance.
//! - size: height = the vertical extent of the `data.tree_model` box, width = height × the
//!   picture's aspect ratio; × the near list's instance scale byte (`u8 / 128`, clamped to
//!   0.5 ..= 1.4).
//! - which direction picture 0 faces, and the flat lighting.
//!
//! **The alpha-test reference is now measured rather than guessed** (round 11): the atlas's alpha is
//! a soft ramp, not a two-level cut-out (install test
//! `speedtree_install::billboard_atlas_alpha_is_a_soft_ramp`), and 0.33 loses 0.4% of the picture
//! against a true cut-out, so it is a safe default — but the game's own value is still UNKNOWN and
//! 0.5 would visibly thin the canopy.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use ntw_formats::vegetation::{TreeAppearance, VegetationIndex};

use super::{TerrainVfs, current_map, lighting_params};

/// Most species per billboard texture (the uniform array holds `MAX_SPECIES × 8` rectangles).
const MAX_SPECIES: usize = 32;
/// Billboard pictures per tree (CONFIRMED: 8 on every tree of the shipped composite maps).
const PICTURES: usize = 8;

/// The billboard material (see `trees.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TreeMaterial {
    #[uniform(0)]
    params: TreeParams,
    #[texture(1)]
    #[sampler(2)]
    texture: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone)]
struct TreeParams {
    sun_dir: Vec4,
    sun_colour: Vec4,
    ambient: Vec4,
    /// x: alpha-test threshold, y: tree far distance, z: tree near distance (the near 3D trees
    /// fade out and the billboards fade in over the last tenth of near..far).
    misc: Vec4,
    /// `[u_min, v_min, u_max, v_max]` of picture `k` of species slot `s` at `s * 8 + k`.
    rects: [Vec4; MAX_SPECIES * PICTURES],
}

impl Material for TreeMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://napoleon/terrain/trees.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/terrain/trees.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(self.params.misc.x)
    }
    // The quads are built in our own vertex shader, which the default prepass and shadow
    // shaders would not do.
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
            Mesh::ATTRIBUTE_UV_0.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_1.at_shader_location(2),
            Mesh::ATTRIBUTE_COLOR.at_shader_location(3),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// One species as drawn: its texture, slot in that texture's rectangle table, and size.
struct Drawn {
    texture: String,
    slot: usize,
    height: f32,
    width: f32,
}

/// Vertices of one billboard texture's mesh.
#[derive(Default)]
struct Batch {
    positions: Vec<[f32; 3]>,
    corners: Vec<[f32; 2]>,
    sizes: Vec<[f32; 2]>,
    info: Vec<[f32; 4]>,
    /// species slot → its 8 rectangles
    rects: Vec<[Vec4; PICTURES]>,
}

impl Batch {
    fn push(&mut self, base: Vec3, w: f32, h: f32, slot: usize) {
        for corner in [[-0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [-0.5, 1.0]] {
            self.positions.push(base.to_array());
            self.corners.push(corner);
            self.sizes.push([w, h]);
            self.info.push([slot as f32, PICTURES as f32, 0.0, 0.0]);
        }
    }

    fn mesh(self) -> Mesh {
        let quads = self.positions.len() as u32 / 4;
        let mut indices = Vec::with_capacity(quads as usize * 6);
        for q in 0..quads {
            let v = q * 4;
            indices.extend_from_slice(&[v, v + 1, v + 2, v, v + 2, v + 3]);
        }
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.corners);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.sizes);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.info);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

/// Size and picture aspect of a resolved tree. `None` for a **shrub**, which has no billboard
/// pictures at all (CONFIRMED) and is drawn as geometry by [`super::speedtree`] instead.
fn billboard_size(a: &TreeAppearance, texture_size: (u32, u32)) -> Option<(f32, f32)> {
    let first = a.composite.billboards.first()?;
    let model = a.model.as_ref()?;
    let (tw, th) = (texture_size.0 as f32, texture_size.1 as f32);
    let aspect = ((first[2] - first[0]) * tw) / ((first[3] - first[1]) * th).max(1e-6);
    let height = model.height().max(0.5);
    Some((aspect * height, height))
}

/// Billboard texture with clamped edges (the pictures sit in an atlas).
fn load_billboard_texture(vfs: &ntw_formats::pack::Vfs, path: &str) -> Option<(Image, (u32, u32))> {
    let mut image = crate::soldiers::load_tinted(vfs, path, None, None).map_err(|e| warn!("{path}: {e}")).ok()?;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });
    let size = (image.width(), image.height());
    Some((image, size))
}

/// Startup: one mesh + material per billboard texture holding every tree of the map.
pub fn spawn_trees(
    mut commands: Commands,
    vfs: Option<Res<TerrainVfs>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TreeMaterial>>,
) {
    let (Some(map), Some(vfs)) = (current_map(), vfs) else { return };
    let map = map.as_ref();
    if map.trees.is_empty() {
        return;
    }
    let vfs = &vfs.0;
    let mut index = match VegetationIndex::from_vfs(vfs) {
        Ok(i) => i,
        Err(e) => {
            warn!("Battle-map trees unavailable: {e}");
            return;
        }
    };
    let season = map.definition.season.clone();
    // species (lower case) → how it is drawn (None = not drawable)
    let mut species: HashMap<String, Option<Drawn>> = HashMap::new();
    let mut textures: HashMap<String, (Handle<Image>, (u32, u32))> = HashMap::new();
    let mut batches: HashMap<String, Batch> = HashMap::new();
    let (mut drawn, mut skipped) = (0usize, HashMap::<String, usize>::new());
    for list in &map.trees {
        for group in &list.groups {
            let key = group.species.to_ascii_lowercase();
            let entry = species.entry(key.clone()).or_insert_with(|| {
                let a = index.resolve(vfs, &group.species, &season)?;
                if a.composite.billboards.is_empty() {
                    return None;
                }
                if !textures.contains_key(&a.billboard_texture) {
                    let (img, size) = load_billboard_texture(vfs, &a.billboard_texture)?;
                    textures.insert(a.billboard_texture.clone(), (images.add(img), size));
                }
                let size = textures[&a.billboard_texture].1;
                let (width, height) = billboard_size(&a, size)?;
                let batch = batches.entry(a.billboard_texture.clone()).or_default();
                if batch.rects.len() >= MAX_SPECIES {
                    warn!("{}: more than {MAX_SPECIES} species on one texture", a.billboard_texture);
                    return None;
                }
                let mut rects = [Vec4::ZERO; PICTURES];
                for (k, r) in a.composite.billboards.iter().take(PICTURES).enumerate() {
                    rects[k] = Vec4::from_array(*r);
                }
                batch.rects.push(rects);
                Some(Drawn { texture: a.billboard_texture.clone(), slot: batch.rects.len() - 1, height, width })
            });
            let Some(d) = entry.as_ref() else {
                *skipped.entry(key).or_default() += group.instances.len();
                continue;
            };
            let batch = batches.get_mut(&d.texture).expect("batch made with the species");
            for t in &group.instances {
                // The near list's U8 is a relative scale (`u8 / 128`, clamped to the picker's 0.5 ..= 1.4;
                // see `TreeInstance::scale`); an unscaled list has no byte and draws at 1.0.
                let scale = t.scale(list.flag);
                let base = super::ground_point(Vec2::new(t.position.0, t.position.1));
                batch.push(base, d.width * scale, d.height * scale, d.slot);
                drawn += 1;
            }
        }
    }
    let (sun_dir, sun, ambient) = lighting_params(map);
    for (texture, mut batch) in batches {
        if batch.positions.is_empty() {
            continue;
        }
        let mut rects = [Vec4::ZERO; MAX_SPECIES * PICTURES];
        for (s, r) in batch.rects.drain(..).enumerate() {
            rects[s * PICTURES..(s + 1) * PICTURES].copy_from_slice(&r);
        }
        let material = TreeMaterial {
            params: TreeParams {
                sun_dir: sun_dir.extend(0.0),
                sun_colour: sun.extend(1.0),
                ambient: ambient.extend(1.0),
                misc: Vec4::new(0.33, super::speedtree::far_distance_from_prefs(), super::speedtree::TREE_NEAR_DISTANCE, 0.0),
                rects,
            },
            texture: textures[&texture].0.clone(),
        };
        commands.spawn((
            Mesh3d(meshes.add(batch.mesh())),
            MeshMaterial3d(materials.add(material)),
            Transform::default(),
            NoFrustumCulling,
            Name::new(format!("trees {texture}")),
        ));
    }
    info!("Battle map: {drawn} trees drawn as billboards");
    if !skipped.is_empty() {
        let mut s: Vec<_> = skipped.into_iter().collect();
        s.sort();
        // Shrubs land here too, and that is expected: they have no billboards, so they are drawn
        // from their `.spt` geometry by `speedtree.rs` instead. Only a species with no files at all
        // is genuinely missing from the battle.
        info!("Battle map: {drawn} more not drawn as billboards (shrubs are drawn as geometry): {s:?}");
    }
}

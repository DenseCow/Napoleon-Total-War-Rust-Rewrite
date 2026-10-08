//! Battle-map **trees and shrubs** as real SpeedTree geometry (bark, fronds, leaf cards), generated
//! at load time from each species' `.spt` by our recreation of SpeedTreeRT
//! (`ntw_formats::speedtree`), with the original LOD distances.
//!
//! Notes: `analysis/speedtree/SPEEDTREE.md` §6.
//! - Distances (CONFIRMED in the exe): `tree_near_distance` = 30 m, `tree_far_distance` set from
//!   the tree quality option: 1 → 100 m, 2 → 150 m, 3 → 200 m, other → 50 m (option INFERRED to be
//!   `gfx_tree_quality`).
//! - Instancing: trees are bucketed into 64 m cells; entities exist only for cells within the far
//!   distance of the camera, so a 180,000-tree map keeps a few thousand near trees. Every tree of a
//!   species shares one mesh + material per part, which Bevy draws instanced.
//! - Per-instance transform: position on the ground, scale = the near list's U8 / 128 clamped to
//!   0.5 ..= 1.4 (the picker's own clamps; INFERRED decode, as for the billboards).
//!   PROVISIONAL: no per-tree rotation or slope tilt — **the data has none to give**
//!   (`UNITS_TERRAIN_FIDELITY.md` §4.1: `TREE_ITEM` is closed at `Coord2d, U8, I32`, and the I32 is
//!   0 on 3,843,976 of 3,844,367 instances, the other 391 all on one preset's two outer lists, so
//!   it cannot be a yaw); and simplified lighting.
//!
//! # Shrubs (round 11)
//!
//! **A shrub is drawn from its own `.spt` geometry, at every distance, with no billboard.** That is
//! forced by the shipped data rather than chosen:
//!
//! - Shrubs have **no** `Billboards` block in their `*_compositemap.txt` entry (CONFIRMED on every
//!   preset, install test `battle_terrain_install::every_tree_species_resolves`), while every tree
//!   has exactly 8. So there is nothing to cross-fade a shrub into, and `trees.rs` skips them.
//! - Their geometry is complete enough to draw (**CONFIRMED**, install test
//!   `speedtree_install::every_shrub_species_generates_drawable_geometry`): across the presets
//!   **34 shrub species** covering **524,295 instances**, and **every one** produces leaf cards
//!   and/or fronds with a composite-map rectangle to texture them and a diffuse texture that
//!   exists. 9 have no bark (fronds and cards only), which is correct — a shrub has no trunk worth
//!   texturing. The heaviest is `lc_boreal-lowbrush_SHRUB` at ~1,165 leaf cards + frond triangles
//!   per instance.
//! - There is no cheaper far representation to fall back on: shrub `.spt` files ship **2 or 3**
//!   branch LOD levels, the same as trees (install test `shrubs_ship_no_extra_branch_lods`), and
//!   the shipped `.spt` files carry **no leaf-cluster LODs at all** (`SptFile::leaf_lods` is `None`
//!   throughout). So the original most likely draws far shrubs as geometry too -- INFERRED from
//!   the absence of any other representation, not read out of the exe's vegetation renderer.
//!
//! [`SHRUB_FAR_DISTANCE`] is therefore the radius the shrub layer is streamed to, PROVISIONAL: the
//! data bounds the cost but not the distance. The census (`shrub_instances_within_a_draw_radius`)
//! gives the worst case per radius of the map centre — 957 shrubs within 200 m (`hb_arcole`),
//! 5,139 within 400 m, 19,358 within 800 m — which is what the constant is chosen against. Shrubs
//! beyond it are simply not drawn; they have no billboards to stand in. What bounds them in the
//! original (cell streaming, a distance cull) is UNKNOWN.
//!
//! Wind sway: driven by the battle's own `prevailing_wind`, see [`TreeWind`].

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexFormat};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use ntw_formats::pack::Vfs;
use ntw_formats::speedtree::SptFile;
use ntw_formats::speedtree::generate::{TreeGeometry, compute_tree};
use ntw_formats::speedtree::params::TreeParams;
use ntw_formats::speedtree::wind::{SpeedWind, WIND_LADDER};
use ntw_formats::vegetation::{TreeAppearance, VegetationIndex};

use super::{TerrainVfs, current_map, lighting_params};

/// `tree_near_distance` (CONFIRMED default).
pub const TREE_NEAR_DISTANCE: f32 = 30.0;
/// How far the **shrub** layer is streamed, in metres.
///
/// **PROVISIONAL.** Shrubs have no billboard pictures (CONFIRMED), so unlike a tree they cannot
/// hand over to `trees.rs` at the far distance — the original draws them as geometry at every
/// distance, and this is the radius we do the same over. Chosen against the install census
/// (`speedtree_install::shrub_instances_within_a_draw_radius`): the densest preset has 957 shrubs
/// within 200 m, 5,139 within 400 m and 19,358 within 800 m, and each carries up to ~1,165 leaf
/// cards plus fronds, so 400 m is the last radius that is plainly affordable and 800 m is the point
/// where the shrub layer starts to cost real frame time. **The data does not name this number**;
/// the targets are the exe's own vegetation LOD distances (which are per-species in
/// `.spt`, `branch_lods` 9007/9008/9012-9014, and which we do not yet select by distance) and the
/// cell streaming radius of `VegetationMeshManager`. Override with `NAPOLEON_SHRUB_DISTANCE=<m>`.
pub const SHRUB_FAR_DISTANCE: f32 = 400.0;
/// Cell size for spawning near trees.
const CELL: f32 = 64.0;

/// The shrub layer's draw radius, from [`SHRUB_FAR_DISTANCE`] or `NAPOLEON_SHRUB_DISTANCE`.
pub fn shrub_far_distance() -> f32 {
    std::env::var("NAPOLEON_SHRUB_DISTANCE").ok().and_then(|s| s.parse().ok()).unwrap_or(SHRUB_FAR_DISTANCE)
}

/// Is a resolved species a shrub? **CONFIRMED by the data's own marker**: a `*_compositemap.txt`
/// entry with an empty `Billboards` section is a shrub's, and every tree's has exactly 8 pictures
/// (`battle_terrain_install::every_tree_species_resolves` checks both on every preset). The file
/// names agree (`_SHRUB.spt`), so this is the same test either way; the composite map is the one
/// that decides how we draw it.
pub fn is_shrub(a: &TreeAppearance) -> bool {
    a.composite.billboards.is_empty()
}

/// `tree_far_distance` for a tree quality setting (CONFIRMED mapping, `FUN_011e1220`).
pub fn tree_far_distance(quality: i64) -> f32 {
    match quality {
        1 => 100.0,
        2 => 150.0,
        3 => 200.0,
        _ => 50.0,
    }
}

/// The far distance from the user's preferences (`gfx_tree_quality`, default 3).
pub fn far_distance_from_prefs() -> f32 {
    let user = crate::config::user_dir();
    let prefs = ntw_script::ui::frontend::load_preferences(user.as_deref(), crate::config::original_user_dir().as_deref());
    tree_far_distance(prefs.get_f64("gfx_tree_quality").map_or(3, |v| v as i64))
}

/// Per-vertex extra data (leaf cards: corner offset right/up in metres, dimming).
pub const ATTRIBUTE_EXTRA: MeshVertexAttribute = MeshVertexAttribute::new("SpeedTree_Extra", 988_540_917, VertexFormat::Float32x4);
/// Per-vertex wind: two levels, each `matrix group + weight` (see [`pack_wind`]).
pub const ATTRIBUTE_WIND: MeshVertexAttribute = MeshVertexAttribute::new("SpeedTree_Wind", 988_540_918, VertexFormat::Float32x2);

/// Bark, frond or leaf-card material (see `speedtree.wgsl`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TreeGeomMaterial {
    #[uniform(0)]
    params: TreeGeomParams,
    #[texture(1)]
    #[sampler(2)]
    texture: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone)]
struct TreeGeomParams {
    sun_dir: Vec4,
    sun_colour: Vec4,
    ambient: Vec4,
    /// x: alpha-test reference, y: near distance, z: far distance, w: kind (0 bark, 1 frond, 2 leaf)
    misc: Vec4,
    /// The 6 SpeedWind matrices in Bevy axes, 3 rows each (`p' = M · p`, tree-local).
    wind: [Vec4; 18],
}

const IDENTITY_WIND: [Vec4; 18] = {
    let mut m = [Vec4::ZERO; 18];
    let mut i = 0;
    while i < 6 {
        m[3 * i] = Vec4::X;
        m[3 * i + 1] = Vec4::Y;
        m[3 * i + 2] = Vec4::Z;
        i += 1;
    }
    m
};

impl Material for TreeGeomMaterial {
    fn vertex_shader() -> ShaderRef {
        "embedded://napoleon/terrain/speedtree.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/terrain/speedtree.wgsl".into()
    }
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
            ATTRIBUTE_EXTRA.at_shader_location(3),
            ATTRIBUTE_WIND.at_shader_location(4),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        // both sides: fronds and leaves are single cards, and the Z flip mirrors the winding
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// One drawable part of a species.
#[derive(Clone)]
struct Part {
    mesh: Handle<Mesh>,
    material: Handle<TreeGeomMaterial>,
    cards: bool,
    /// Drawn by the shrub layer rather than the tree layer (see the module docs). Only affects the
    /// material's distance fade, which trees have and shrubs must not.
    shrub: bool,
}

/// One placed tree or shrub: (species, position, scale).
type TreeInstance = (u16, Vec3, f32);

/// One streaming layer: instances bucketed into cells, spawned within `far` of the camera.
///
/// Trees and shrubs are two of these because they reach differently — a tree hands over to its
/// billboard at [`tree_far_distance`], a shrub has none to hand over to and is geometry all the way
/// to [`SHRUB_FAR_DISTANCE`].
#[derive(Default)]
struct Layer {
    far: f32,
    /// cell → instances
    cells: HashMap<(i32, i32), Vec<TreeInstance>>,
    spawned: HashMap<(i32, i32), Vec<Entity>>,
}

/// The near trees and shrubs of the current map.
#[derive(Resource, Default)]
pub struct NearTrees {
    species: Vec<Vec<Part>>,
    trees: Layer,
    shrubs: Layer,
}

/// Game space → Bevy (flip Z).
fn bevy(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[1], -v[2]]
}

/// Strips → triangle list (alternating winding; degenerates dropped).
fn strip_triangles(strips: &[Vec<u32>], out: &mut Vec<u32>) {
    for s in strips {
        for k in 2..s.len() {
            let (a, b, c) = (s[k - 2], s[k - 1], s[k]);
            if a == b || b == c || a == c {
                continue;
            }
            if k % 2 == 0 { out.extend([a, b, c]) } else { out.extend([b, a, c]) }
        }
    }
}

fn new_mesh(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, extra: Vec<[f32; 4]>, wind: Vec<[f32; 2]>, indices: Vec<u32>) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(ATTRIBUTE_EXTRA, extra);
    mesh.insert_attribute(ATTRIBUTE_WIND, wind);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Packs a vertex's two wind levels as `group + weight` (the original packs `group·10/6 +
/// weight`; we keep the group whole). Weights are the exe's clamped `1 − w`.
fn pack_wind(weights: [f32; 2], groups: [u8; 2]) -> [f32; 2] {
    [f32::from(groups[0] % 6) + weights[0].min(0.999), f32::from(groups[1] % 6) + weights[1].min(0.999)]
}

/// Map a 0..1 coordinate into a composite-map rectangle (`[u0, v0, u1, v1]`, v0 = top).
fn in_rect(r: &[f32; 4], u: f32, v: f32) -> [f32; 2] {
    [r[0] + (r[2] - r[0]) * u, r[3] + (r[1] - r[3]) * v]
}

/// The meshes of a species (bark, fronds, leaf cards, mesh leaves), in Bevy axes.
fn species_meshes(g: &TreeGeometry, t: &TreeParams, a: &TreeAppearance) -> [Option<Mesh>; 4] {
    // bark
    let m = &g.mesh;
    let mut idx = Vec::new();
    strip_triangles(m.lod_strips.first().map(Vec::as_slice).unwrap_or(&[]), &mut idx);
    let bark = (!idx.is_empty()).then(|| {
        new_mesh(
            m.positions.iter().map(|p| bevy(*p)).collect(),
            m.normals.iter().map(|n| bevy(*n)).collect(),
            m.uvs.iter().map(|uv| [uv[0], -uv[1]]).collect(),
            vec![[0.0; 4]; m.positions.len()],
            m.wind_weights.iter().zip(&m.wind_groups).map(|(w, g)| pack_wind(*w, *g)).collect(),
            idx,
        )
    });
    // fronds
    let f = &g.frond_mesh;
    let mut idx = Vec::new();
    strip_triangles(&f.strips, &mut idx);
    let fronds = (!idx.is_empty() && !a.composite.fronds.is_empty()).then(|| {
        let uvs = f
            .uvs
            .iter()
            .zip(&f.textures)
            .map(|(uv, &tx)| {
                let r = a.composite.fronds.get(tx as usize).unwrap_or(&a.composite.fronds[0]);
                in_rect(r, uv[0], uv[1])
            })
            .collect();
        new_mesh(
            f.positions.iter().map(|p| bevy(*p)).collect(),
            f.normals.iter().map(|n| bevy(*n)).collect(),
            uvs,
            vec![[0.0; 4]; f.positions.len()],
            f.wind_weights.iter().zip(&f.wind_groups).map(|(w, g)| pack_wind(*w, *g)).collect(),
            idx,
        )
    });
    let leaf_wind = |lf: &ntw_formats::speedtree::generate::PlacedLeaf| {
        let ww = |w: f32| (1.0 - w).clamp(0.0, 1.0);
        pack_wind([ww(lf.wind[0]), ww(lf.wind[1])], [lf.wind_groups[0] as u8, lf.wind_groups[1] as u8])
    };
    // leaf cards: 4 vertices per leaf at its position, expanded in the shader
    let leaves = (!g.leaves.is_empty() && !a.composite.leaves.is_empty()).then(|| {
        let n = g.leaves.len();
        let (mut pos, mut nor, mut uvs, mut extra, mut wind, mut idx) =
            (Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(6 * n));
        for lf in &g.leaves {
            let tex = &t.leaf_textures[lf.texture.min(t.leaf_textures.len() - 1)];
            if tex.mesh.is_some() {
                continue;
            }
            let r = a.composite.leaves.get(lf.texture).unwrap_or(&a.composite.leaves[0]);
            // even slots are the mirrored variant (CONFIRMED: FUN_012cfa30 flips the origin x)
            let mirror = lf.slot % 2 == 0;
            let ox = if mirror { 1.0 - tex.origin[0] } else { tex.origin[0] };
            // PROVISIONAL pivot convention: the card is centred on the leaf, offset by origin − 0.5
            let (px, py) = (0.5 - ox, tex.origin[1] - 0.5);
            let (w, h) = (tex.size[0], tex.size[1]);
            let base = pos.len() as u32;
            for (sx, sy) in [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                pos.push(bevy(lf.pos));
                nor.push([0.0, 1.0, 0.0]);
                let u = if mirror { 0.5 - sx } else { sx + 0.5 };
                uvs.push(in_rect(r, u, sy + 0.5));
                extra.push([(sx + px) * w, (sy + py) * h, lf.dimming, 0.0]);
                wind.push(leaf_wind(lf));
            }
            idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        (!idx.is_empty()).then(|| new_mesh(pos, nor, uvs, extra, wind, idx))
    })
    .flatten();
    // mesh leaves (72001): the 71000 mesh scaled by the card's larger side, placed with the leaf
    // basis. PROVISIONAL: mesh x/y/z taken along R/U/D (the game's exact axis packing is UNKNOWN).
    let (mut pos, mut nor, mut uvs, mut wind, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for lf in &g.leaves {
        let tex = &t.leaf_textures[lf.texture.min(t.leaf_textures.len() - 1)];
        let Some(mesh) = tex.mesh.and_then(|i| t.leaf_meshes.get(i.max(0) as usize)) else { continue };
        let Some(r) = a.composite.leaves.get(lf.texture).or(a.composite.leaves.first()) else { continue };
        let s = tex.size[0].max(tex.size[1]);
        let [br, bu, bd] = lf.basis;
        let along = |v: [f32; 3], k: f32| {
            [
                br[0] * v[0] * k + bu[0] * v[1] * k + bd[0] * v[2] * k,
                br[1] * v[0] * k + bu[1] * v[1] * k + bd[1] * v[2] * k,
                br[2] * v[0] * k + bu[2] * v[1] * k + bd[2] * v[2] * k,
            ]
        };
        let base = pos.len() as u32;
        for v in &mesh.vertices {
            let o = along(v.v71006, s);
            pos.push(bevy([lf.pos[0] + o[0], lf.pos[1] + o[1], lf.pos[2] + o[2]]));
            nor.push(bevy(along(v.v71007, 1.0)));
            uvs.push(in_rect(r, v.uv[0], v.uv[1]));
            wind.push(leaf_wind(lf));
        }
        idx.extend(mesh.indices.iter().filter(|&&i| (i as usize) < mesh.vertices.len()).map(|&i| base + i as u32));
    }
    let leaf_meshes = (!idx.is_empty()).then(|| {
        let n = pos.len();
        new_mesh(pos, nor, uvs, vec![[0.0; 4]; n], wind, idx)
    });
    [bark, fronds, leaves, leaf_meshes]
}

fn load_texture(vfs: &Vfs, path: &str, repeat: bool) -> Option<Image> {
    let mut image = crate::soldiers::load_tinted(vfs, path, None, None).map_err(|e| warn!("{path}: {e}")).ok()?;
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        ..ImageSamplerDescriptor::linear()
    });
    Some(image)
}

/// Builds every species' meshes and buckets the map's trees into cells.
pub fn setup_near_trees(
    mut commands: Commands,
    vfs: Option<Res<TerrainVfs>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<TreeGeomMaterial>>,
) {
    let (Some(map), Some(vfs)) = (current_map(), vfs) else { return };
    let map = map.as_ref();
    let vfs = &vfs.0;
    let mut near = NearTrees {
        trees: Layer { far: far_distance_from_prefs(), ..Default::default() },
        shrubs: Layer { far: shrub_far_distance(), ..Default::default() },
        ..Default::default()
    };
    if map.trees.is_empty() {
        commands.insert_resource(near);
        return;
    }
    let Ok(mut index) = VegetationIndex::from_vfs(vfs) else {
        commands.insert_resource(near);
        return;
    };
    let (tree_far, shrub_far) = (near.trees.far, near.shrubs.far);
    let season = map.definition.season.clone();
    let (sun_dir, sun, ambient) = lighting_params(map);
    let mut textures: HashMap<String, Option<Handle<Image>>> = HashMap::new();
    let mut mats: HashMap<(String, u8, bool), Handle<TreeGeomMaterial>> = HashMap::new();
    let mut species_index: HashMap<String, Option<u16>> = HashMap::new();
    let t0 = std::time::Instant::now();
    let (mut generated, mut total) = (0usize, 0usize);
    let (mut shrub_species, mut shrub_instances) = (0usize, 0usize);
    for list in &map.trees {
        for group in &list.groups {
            let key = group.species.to_ascii_lowercase();
            let slot = *species_index.entry(key).or_insert_with(|| {
                let a = index.resolve(vfs, &group.species, &season)?;
                // A shrub has no billboard pictures, so it is drawn from this geometry at every
                // distance (see the module docs). The material's distances follow: a tree fizzles
                // into its billboard, a shrub must not fizzle at all.
                let shrub = is_shrub(&a);
                let f = SptFile::read(&vfs.read(&a.spt_path).ok()?).map_err(|e| warn!("{}: {e}", a.spt_path)).ok()?;
                let params = TreeParams::from_spt(&f);
                let g = compute_tree(&params);
                generated += 1;
                let dir = a.spt_path.rsplit_once('\\').map(|(d, _)| d.to_owned())?;
                let bark_path = format!("{dir}\\textures\\{}", f.tree.branch_texture.to_ascii_lowercase());
                let mut parts = Vec::new();
                let [bark, fronds, leaves, leaf_meshes] = species_meshes(&g, &params, &a);
                // Test harness: `NAPOLEON_TREE_PARTS=bark,frond,leaf` draws only the listed parts.
                let only = std::env::var("NAPOLEON_TREE_PARTS").ok();
                for (mesh, tex_path, kind) in [(bark, &bark_path, 0u8), (fronds, &a.diffuse_texture, 1), (leaves, &a.diffuse_texture, 2), (leaf_meshes, &a.diffuse_texture, 1)] {
                    let Some(mesh) = mesh else { continue };
                    if kind == 0 && f.tree.branch_texture.is_empty() {
                        continue;
                    }
                    if only.as_ref().is_some_and(|o| !o.contains(["bark", "frond", "leaf"][kind as usize])) {
                        continue;
                    }
                    let tex = textures
                        .entry(tex_path.clone())
                        .or_insert_with(|| load_texture(vfs, tex_path, kind == 0).map(|i| images.add(i)))
                        .clone();
                    let Some(tex) = tex else { continue };
                    // A tree's geometry fizzles out over the near→far band because a billboard
                    // takes over there; a shrub has none, so its two distances are the same and the
                    // fizzle band collapses to the last metre (the shader's `FADE_BAND` floor).
                    let (near_d, far_d) = if shrub { (shrub_far, shrub_far) } else { (TREE_NEAR_DISTANCE, tree_far) };
                    let material = mats
                        .entry((tex_path.clone(), kind, shrub))
                        .or_insert_with(|| {
                            materials.add(TreeGeomMaterial {
                                params: TreeGeomParams {
                                    sun_dir: sun_dir.extend(0.0),
                                    sun_colour: sun.extend(1.0),
                                    ambient: ambient.extend(1.0),
                                    misc: Vec4::new(0.33, near_d, far_d, f32::from(kind)),
                                    wind: IDENTITY_WIND,
                                },
                                texture: tex,
                            })
                        })
                        .clone();
                    parts.push(Part { mesh: meshes.add(mesh), material, cards: kind == 2, shrub });
                }
                if parts.is_empty() {
                    return None;
                }
                if shrub {
                    shrub_species += 1;
                }
                near.species.push(parts);
                Some((near.species.len() - 1) as u16)
            });
            let Some(slot) = slot else { continue };
            // A species is wholly trees or wholly shrubs, so which layer an instance belongs to is
            // decided once for the group, by its species' billboard block.
            let shrub = near.species[slot as usize].first().is_some_and(|p| p.shrub);
            let layer = if shrub { &mut near.shrubs } else { &mut near.trees };
            for t in &group.instances {
                let scale = t.scale(list.flag);
                let p = super::ground_point(Vec2::new(t.position.0, t.position.1));
                let cell = ((p.x / CELL).floor() as i32, (p.z / CELL).floor() as i32);
                layer.cells.entry(cell).or_default().push((slot, p, scale));
                total += 1;
                if shrub {
                    shrub_instances += 1;
                }
            }
        }
    }
    info!(
        "SpeedTree: {generated} species generated in {:?} ({shrub_species} of them shrubs); {total} instances in {} tree and {} shrub cells; tree far {tree_far} m, shrub far {shrub_far} m",
        t0.elapsed(),
        near.trees.cells.len(),
        near.shrubs.cells.len()
    );
    info!("SpeedTree: {shrub_instances} shrub instances drawn as geometry (they have no billboards)");
    commands.insert_resource(near);
}

/// Spawns the instances of cells that came within their layer's far distance, despawns those that
/// left, and returns how many cells each layer still has live.
pub fn update_near_trees(
    mut commands: Commands,
    near: Option<ResMut<NearTrees>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
) {
    let Some(mut near) = near else { return };
    let Some(cam) = camera.iter().next() else { return };
    let c = cam.translation();
    let (cx, cz) = ((c.x / CELL).floor() as i32, (c.z / CELL).floor() as i32);
    let dist = |cell: (i32, i32)| {
        let centre = Vec2::new((cell.0 as f32 + 0.5) * CELL, (cell.1 as f32 + 0.5) * CELL);
        // horizontal distance to the cell's nearest point
        let d = (Vec2::new(c.x, c.z) - centre).abs() - Vec2::splat(CELL * 0.5);
        Vec2::new(d.x.max(0.0), d.y.max(0.0)).length()
    };
    // The two layers are streamed independently, each with its own radius and its own per-frame
    // cell budget, so a dense shrub field cannot starve the trees (or the reverse).
    let NearTrees { species, trees, shrubs } = &mut *near;
    update_layer(&mut commands, trees, species, (cx, cz), &dist);
    update_layer(&mut commands, shrubs, species, (cx, cz), &dist);
}

/// One layer's streaming step: spawn the cells that came into reach, despawn those that left.
fn update_layer(
    commands: &mut Commands,
    layer: &mut Layer,
    species: &[Vec<Part>],
    (cx, cz): (i32, i32),
    dist: &impl Fn((i32, i32)) -> f32,
) {
    let reach = layer.far + CELL;
    let r = (reach / CELL).ceil() as i32 + 1;
    let mut wanted = HashSet::new();
    for z in cz - r..=cz + r {
        for x in cx - r..=cx + r {
            if layer.cells.contains_key(&(x, z)) && dist((x, z)) < layer.far {
                wanted.insert((x, z));
            }
        }
    }
    let gone: Vec<(i32, i32)> = layer.spawned.keys().filter(|k| !wanted.contains(k) && dist(**k) > layer.far + CELL * 0.5).copied().collect();
    for k in gone {
        if let Some(es) = layer.spawned.remove(&k) {
            for e in es {
                commands.entity(e).despawn();
            }
        }
    }
    let mut budget = 24;
    for k in wanted {
        if layer.spawned.contains_key(&k) || budget == 0 {
            continue;
        }
        budget -= 1;
        let mut es = Vec::new();
        for &(s, p, scale) in &layer.cells[&k] {
            for part in &species[s as usize] {
                let mut e = commands.spawn((
                    Mesh3d(part.mesh.clone()),
                    MeshMaterial3d(part.material.clone()),
                    Transform::from_translation(p).with_scale(Vec3::splat(scale)),
                ));
                if part.cards {
                    e.insert(NoFrustumCulling);
                }
                es.push(e.id());
            }
        }
        layer.spawned.insert(k, es);
    }
}

/// Removes the near trees and shrubs when the battle ends.
pub fn clear_near_trees(mut commands: Commands, near: Option<ResMut<NearTrees>>) {
    if let Some(mut near) = near {
        let NearTrees { trees, shrubs, .. } = &mut *near;
        for (_, es) in trees.spawned.drain().chain(shrubs.spawned.drain()) {
            for e in es {
                commands.entity(e).despawn();
            }
        }
    }
    commands.remove_resource::<NearTrees>();
}

/// SpeedWind (`ntw_formats::speedtree::wind`) driving the trees' and shrubs' sway.
///
/// **The wind is the battle's own, not an invented one** (item 4, round 11). The battle file's
/// `weather/prevailing_wind` is the only wind any battle ships, and it is the same vector the flag
/// cloth already uses ([`crate::battle::BattleSim::wind`]), read as a velocity in m/s. The **scale**
/// comes from the game's own table: [`WIND_LADDER`] = 3, 6, 9, 12, 16 m/s, read off the ten rows of
/// `db\wind_levels_tables\wind_levels`, every one of which is either westerly (+x) or northerly (−y)
/// with exactly one of those five magnitudes — **CONFIRMED** (data), so **16 m/s is the strongest
/// wind the table carries**. Using it as the divisor is our choice (INFERRED/PROVISIONAL).
///
/// **A negative, worth more than the divisor.** Four of the five distinct speeds the 29 shipped
/// battle files declare — `(0, 9)`, `(0, 5)`, `(0, 0)`, `(10, 0)`, `(−5, 5)` — are **not** on that
/// five-rung ladder. So the vegetation's wind is **not** a lookup of a `wind_levels` row: the battle
/// file's vector is its own thing and the ladder only fixes the scale. (The `wind_levels` audio column
/// cannot be recovered from the exe's `|v|` thresholds 0.583 / 0.687 / 0.820 / 0.916 either — dividing
/// by 16 bands the five speeds 0, 0, 0, 2, 4, not the 0..4 the table assigns them.)
///
/// So: **strength = speed / 16**, PROVISIONAL (16 is the table's maximum, CONFIRMED; that the exe
/// normalises the vegetation wind by it is not read out of the exe -- target `FUN_01200be0`);
/// **direction = the battle vector**, which is why a battle shipping `(0, 0)` now has still trees to
/// match its hanging flags — five of the 29 do. Still PROVISIONAL that the game multiplies the base
/// strength by a time-varying factor at all (`FUN_01200be0`, not decoded). Override with
/// `NAPOLEON_TREE_WIND=<0..1>`.
#[derive(Resource)]
pub struct TreeWind {
    wind: SpeedWind,
    /// The `(strength, direction)` last handed to [`SpeedWind::set_wind`], so the battle's vector is
    /// re-read every frame but only applied when it actually changes.
    applied: Option<(f32, [f32; 3])>,
}

/// Loads `SpeedWind.ini` through the Vfs.
///
/// The battle's wind is applied in [`update_wind`] rather than here, because `BattleSim` is inserted
/// by the battle plugin's own `OnEnter` hook and the two plugins' order is not fixed.
pub fn setup_wind(mut commands: Commands, vfs: Option<Res<TerrainVfs>>) {
    let Some(vfs) = vfs else { return };
    let path = r"rigidmodels\vegetation\wind\speedwind.ini";
    let Ok(bytes) = vfs.0.read(path) else {
        warn!("{path} missing: trees will not sway");
        return;
    };
    commands.insert_resource(TreeWind { wind: SpeedWind::parse(&String::from_utf8_lossy(&bytes)), applied: None });
}

/// The battle's wind as SpeedTree wants it: `(strength, direction)`.
///
/// `BattleSim::wind` is world axes `(x, 0, −y)`; the game hands SpeedTree `(x, y, z)`, so the two
/// flips cancel and the vector comes back out as `(x, y, 0)`, which then maps to SpeedTree's
/// `(x, z, y)`.
///
/// **The strength is the battle's own wind speed over [`WIND_LADDER`]'s strongest rung, 16 m/s**
/// (the table's maximum is CONFIRMED from the ten `wind_levels` rows; dividing by it is
/// PROVISIONAL). It is a divisor, not a level lookup: four of the five distinct speeds the shipped battle files declare
/// (`(0, 9)`, `(0, 5)`, `(0, 0)`, `(10, 0)`, `(−5, 5)`) are **not** on the five-rung ladder, and an
/// exact-match lookup would hand three real winds no sway at all. Austerlitz's `(0, 9)` therefore
/// gives 9/16 = 0.5625. Override with `NAPOLEON_TREE_WIND=<0..1>`.
///
/// **The direction** is the battle vector's horizontal pair, in SpeedTree's triple. That is the only
/// form the ported [`SpeedWind`] can use: its `advance` reads the wind's horizontal components as
/// `d[0], d[1]` (`theta = d[0].atan2(d[1])`), with its `z` the vertical — SpeedTree is Z-up. So a
/// battle wind of `(0, 9)` becomes SpeedTree `(0, 9, 0)` and bends the trees about the x axis, i.e.
/// along ±y, which is right. The full game→SpeedTree axis permutation is still **PROVISIONAL**
/// (`FUN_01240980`); before this round the direction was hard-coded along SpeedTree +x, which is
/// this reading with the battle's y component zero.
pub fn battle_wind_target(wind: [f32; 3]) -> (f32, [f32; 3]) {
    // `BattleSim::wind` is world axes `(x, 0, −y)`; the game's own horizontal pair is `(x, y)`.
    let dir = [wind[0], -wind[2], 0.0];
    let speed = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    let strongest = WIND_LADDER[WIND_LADDER.len() - 1];
    // `"nan".parse::<f32>()` succeeds, and a NaN strength would poison every wind matrix; only a
    // finite override counts. A non-finite battle vector is still air.
    let speed = if speed.is_finite() { speed } else { 0.0 };
    let dir = if speed > 0.0 { dir } else { [0.0; 3] };
    let strength = std::env::var("NAPOLEON_TREE_WIND")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|v| v.is_finite())
        .map_or_else(|| (speed / strongest).clamp(0.0, 1.0), |v| v.clamp(0.0, 1.0));
    (strength, dir)
}

/// Advances the wind and uploads its matrices (in Bevy axes) to every tree material.
pub fn update_wind(
    time: Res<Time>,
    wind: Option<ResMut<TreeWind>>,
    sim: Option<Res<crate::battle::BattleSim>>,
    mut materials: ResMut<Assets<TreeGeomMaterial>>,
) {
    let Some(mut wind) = wind else { return };
    // The battle's vector is the sway's source, so re-read it each frame (cheap) and only push it
    // into SpeedWind when it differs from what is already applied.
    if let Some(sim) = sim.as_deref() {
        let (strength, dir) = battle_wind_target(sim.wind);
        if wind.applied != Some((strength, dir)) {
            if wind.applied.is_none() {
                info!(
                    "Tree wind: the battle's own prevailing_wind {:?} -> strength {strength:.3} (over the game's strongest shipped wind, 16 m/s), direction {dir:?}",
                    sim.wind
                );
            }
            wind.wind.set_wind(strength, dir);
            wind.applied = Some((strength, dir));
        }
    }
    wind.wind.advance(time.elapsed_secs());
    // SpeedTree axes s from Bevy axes b: s = C·b with s.x = −b.x, s.y = −b.z, s.z = b.y.
    let c = Mat3::from_cols(Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0));
    let mut rows = [Vec4::ZERO; 18];
    for (i, m) in wind.wind.matrices.iter().take(6).enumerate() {
        // the shaders use p' = p·M (row vector): as a column operation that is Mᵀ
        let mt = Mat3::from_cols(Vec3::new(m[0], m[1], m[2]), Vec3::new(m[3], m[4], m[5]), Vec3::new(m[6], m[7], m[8]));
        let n = c.transpose() * mt * c;
        for r in 0..3 {
            rows[3 * i + r] = n.row(r).extend(0.0);
        }
    }
    for (_, mat) in materials.iter_mut() {
        mat.params.wind = rows;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sway's source is the battle's own `prevailing_wind`, read off the game's speed ladder.
    /// Austerlitz ships `(0, 9)` — world `(0, 0, −9)` — so the strength is 9/16 and the wind blows
    /// along SpeedTree +y. The five battle files shipping `(0, 0)` must give **no** wind at all,
    /// which is the point: their flags hang, so their trees should stand still.
    #[test]
    fn the_sway_follows_the_battle_files_own_wind() {
        // (battle vector) -> (expected strength, expected SpeedTree direction)
        // `battle_wind_target` hands `SpeedWind::set_wind` the horizontal game pair; `set_wind` normalises.
        for (world, strength, dir) in [
            ([0.0, 0.0, -9.0], 9.0 / 16.0, [0.0, 9.0, 0.0]), // Austerlitz (0, 9): bends along ±y
            ([0.0, 0.0, 0.0], 0.0, [0.0, 0.0, 0.0]),         // still air: no sway at all
            ([0.0, 0.0, -5.0], 5.0 / 16.0, [0.0, 5.0, 0.0]),  // Nile / Trafalgar MP (0, 5)
            ([10.0, 0.0, 0.0], 10.0 / 16.0, [10.0, 0.0, 0.0]), // Trafalgar (10, 0): along ±x
            ([-5.0, 0.0, -5.0], 7.071_068 / 16.0, [-5.0, 5.0, 0.0]), // Nile (-5, 5)
        ] {
            let (got, got_dir) = battle_wind_target(world);
            assert!((got - strength).abs() < 1e-6, "{world:?}: strength {got} != {strength}");
            for (a, b) in got_dir.iter().zip(dir) {
                assert!((a - b).abs() < 1e-6, "{world:?}: direction {got_dir:?} != {dir:?}");
            }
        }
    }

    /// The strength is the battle's own wind speed over the game's strongest shipped wind
    /// (16 m/s), so each of the game's five speeds lands on 3/16, 6/16 … 1.0, still air gives 0,
    /// and a speed over the strongest clamps at full strength.
    #[test]
    fn the_strength_is_the_speed_over_the_games_strongest_wind() {
        for (speed, want) in [(0.0, 0.0), (3.0, 3.0 / 16.0), (6.0, 6.0 / 16.0), (9.0, 9.0 / 16.0), (12.0, 12.0 / 16.0), (16.0, 1.0), (40.0, 1.0)] {
            assert!((battle_wind_target([speed, 0.0, 0.0]).0 - want).abs() < 1e-6, "{speed} m/s");
        }
    }
}

//! GPU skinning for the battle's soldiers and mounts, with a separate animation phase per man.
//!
//! The CPU path (`crate::soldiers`, still used by the model viewer) re-poses one shared mesh per
//! kit, so every man of a unit moves in step. Here each part keeps one static mesh whose
//! vertices carry, per bone influence, the bone-local position and normal (the formats have no
//! bind pose, see `analysis/units/ANIM_FORMAT.md` §4); `soldier_skin.wgsl` poses them with:
//! - [`BoneAtlas`]: every frame of every clip in use, as model-space bone matrices, in one
//!   storage buffer (uploaded once per battle);
//! - a small per-figure storage buffer, rewritten every frame: which two frames a man shows
//!   now and the blend between them. Each part entity's `MeshTag` is its figure's slot (man and
//!   mount have separate slots, as they play different, paired clips);
//! - a fade buffer: for the figures cross-fading out of a clip change only, the frozen poses
//!   and their weights ([`Fade`], [`FadeUpload`]).
//!
//! Men sharing a kit share its meshes and materials, so Bevy draws them instanced.
//! PROVISIONAL: the per-man phase is a hash of the unit id and the man's index (the original's
//! rule is UNKNOWN); frames are blended linearly (matrix lerp); more than 4 influences per
//! vertex (none seen on soldiers) keep the 4 heaviest.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexFormat};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use ntw_formats::anim::Anim;
use ntw_formats::unit_model::{PartBinding, SoldierPart};
use ntw_formats::weighted_mesh::WeightedPiece;

/// Bone-local positions of influences 1..3 (influence 0 uses `ATTRIBUTE_POSITION`).
pub const ATTRIBUTE_P1: MeshVertexAttribute = MeshVertexAttribute::new("Skin_P1", 0x4E52_5301, VertexFormat::Float32x3);
pub const ATTRIBUTE_P2: MeshVertexAttribute = MeshVertexAttribute::new("Skin_P2", 0x4E52_5302, VertexFormat::Float32x3);
pub const ATTRIBUTE_P3: MeshVertexAttribute = MeshVertexAttribute::new("Skin_P3", 0x4E52_5303, VertexFormat::Float32x3);
/// Bone-local normals of influences 1..3 (influence 0 uses `ATTRIBUTE_NORMAL`).
pub const ATTRIBUTE_N1: MeshVertexAttribute = MeshVertexAttribute::new("Skin_N1", 0x4E52_5304, VertexFormat::Float32x3);
pub const ATTRIBUTE_N2: MeshVertexAttribute = MeshVertexAttribute::new("Skin_N2", 0x4E52_5305, VertexFormat::Float32x3);
pub const ATTRIBUTE_N3: MeshVertexAttribute = MeshVertexAttribute::new("Skin_N3", 0x4E52_5306, VertexFormat::Float32x3);
/// Bone indices of the 4 influences (`NO_BONE` = identity).
pub const ATTRIBUTE_JOINTS: MeshVertexAttribute = MeshVertexAttribute::new("Skin_Joints", 0x4E52_5307, VertexFormat::Uint32x4);
/// Weights of the 4 influences (0 = unused).
pub const ATTRIBUTE_WEIGHTS: MeshVertexAttribute = MeshVertexAttribute::new("Skin_Weights", 0x4E52_5308, VertexFormat::Float32x4);

/// Bone index meaning "no bone" (the vertex is already in model space).
const NO_BONE: u32 = 65535;

/// The skinning extension of `StandardMaterial`: three shared storage buffers (bone matrices,
/// figure slots, cross-fades).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct SkinExt {
    #[storage(100, read_only)]
    pub bones: Handle<ShaderBuffer>,
    #[storage(101, read_only)]
    pub figures: Handle<ShaderBuffer>,
    #[storage(102, read_only)]
    pub fades: Handle<ShaderBuffer>,
}

/// A soldier material: Bevy's PBR fragment with our skinning vertex stage.
pub type SkinMaterial = ExtendedMaterial<StandardMaterial, SkinExt>;

impl MaterialExtension for SkinExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://napoleon/battle/soldier_skin.wgsl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://napoleon/battle/soldier_skin_prepass.wgsl".into()
    }
    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            Mesh::ATTRIBUTE_NORMAL.at_shader_location(1),
            Mesh::ATTRIBUTE_UV_0.at_shader_location(2),
            ATTRIBUTE_P1.at_shader_location(8),
            ATTRIBUTE_P2.at_shader_location(9),
            ATTRIBUTE_P3.at_shader_location(10),
            ATTRIBUTE_N1.at_shader_location(11),
            ATTRIBUTE_N2.at_shader_location(12),
            ATTRIBUTE_N3.at_shader_location(13),
            ATTRIBUTE_JOINTS.at_shader_location(14),
            ATTRIBUTE_WEIGHTS.at_shader_location(15),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

/// One vertex with up to 4 influences (bone, bone-local position, bone-local normal, weight).
#[derive(Default)]
struct SkinVerts {
    p: [Vec<[f32; 3]>; 4],
    n: [Vec<[f32; 3]>; 4],
    uv: Vec<[f32; 2]>,
    joints: Vec<[u32; 4]>,
    weights: Vec<[f32; 4]>,
}

impl SkinVerts {
    fn push(&mut self, uv: [f32; 2], mut inf: Vec<(u32, [f32; 3], [f32; 3], f32)>) {
        inf.sort_by(|a, b| b.3.total_cmp(&a.3));
        inf.truncate(4);
        let total: f32 = inf.iter().map(|i| i.3).sum::<f32>().max(1e-6);
        let mut joints = [NO_BONE; 4];
        let mut weights = [0.0; 4];
        for k in 0..4 {
            let (b, p, n, w) = inf.get(k).copied().unwrap_or((NO_BONE, [0.0; 3], [0.0, 1.0, 0.0], 0.0));
            self.p[k].push(p);
            self.n[k].push(n);
            joints[k] = b;
            weights[k] = w / total;
        }
        self.uv.push(uv);
        self.joints.push(joints);
        self.weights.push(weights);
    }

    fn mesh(self, indices: Vec<u32>) -> Mesh {
        let [p0, p1, p2, p3] = self.p;
        let [n0, n1, n2, n3] = self.n;
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, p0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, n0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uv);
        mesh.insert_attribute(ATTRIBUTE_P1, p1);
        mesh.insert_attribute(ATTRIBUTE_P2, p2);
        mesh.insert_attribute(ATTRIBUTE_P3, p3);
        mesh.insert_attribute(ATTRIBUTE_N1, n1);
        mesh.insert_attribute(ATTRIBUTE_N2, n2);
        mesh.insert_attribute(ATTRIBUTE_N3, n3);
        mesh.insert_attribute(ATTRIBUTE_JOINTS, self.joints);
        mesh.insert_attribute(ATTRIBUTE_WEIGHTS, self.weights);
        // Mirroring Z in the shader flips handedness, so swap two indices of every triangle
        // (as the CPU path does).
        let mut idx = Vec::with_capacity(indices.len());
        for t in indices.as_chunks::<3>().0 {
            idx.extend([t[0], t[2], t[1]]);
        }
        mesh.insert_indices(Indices::U32(idx));
        mesh
    }
}

fn mat_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    ntw_formats::anim::transform_point(m, p)
}

fn mat_vector(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
    ntw_formats::anim::transform_vector(m, v)
}

/// Number of LODs of a part (1 + its lower LODs).
pub fn part_lod_count(part: &SoldierPart) -> usize {
    1 + part.lower_lods.len()
}

/// The static GPU-skinned mesh of LOD `lod` of a soldier part (`.variant_part_mesh` or
/// equipment; 0 = most detailed; clamped).
pub fn part_mesh_lod(part: &SoldierPart, lod: usize) -> Mesh {
    let (vertices, indices) = match lod.checked_sub(1).and_then(|k| part.lower_lods.get(k)) {
        Some((v, i)) => (v.as_slice(), i.as_slice()),
        None => (part.vertices.as_slice(), part.indices.as_slice()),
    };
    let mut v = SkinVerts::default();
    for x in vertices {
        let inf = match (&part.binding, x.bones) {
            (PartBinding::Bone(b), _) => vec![(*b, x.positions[0], x.normals[0], 1.0)],
            (PartBinding::Attachment { bone, matrix }, _) => {
                vec![(*bone, mat_point(matrix, x.positions[0]), mat_vector(matrix, x.normals[0]), 1.0)]
            }
            (PartBinding::Skinned, None) => vec![(NO_BONE, x.positions[0], x.normals[0], 1.0)],
            (PartBinding::Skinned, Some([a, b])) => vec![
                (u32::from(a), x.positions[0], x.normals[0], x.weight),
                (u32::from(b), x.positions[1], x.normals[1], 1.0 - x.weight),
            ],
        };
        v.push(x.uv, inf.into_iter().filter(|i| i.3 > 0.0).collect());
    }
    v.mesh(indices.iter().map(|&i| u32::from(i)).collect())
}

/// The static GPU-skinned mesh of a mount piece (`.variant_weighted_mesh`).
pub fn piece_mesh(piece: &WeightedPiece) -> Mesh {
    let mut v = SkinVerts::default();
    for x in &piece.vertices {
        v.push(x.uv, x.influences.iter().map(|i| (i.bone, i.position, i.normal, i.weight)).collect());
    }
    v.mesh(piece.indices.clone())
}

/// Where a clip's frames start in the bone storage buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClipSlot {
    /// Index of frame 0, bone 0.
    pub base: u32,
    pub frames: u32,
    pub bones: u32,
    pub rate: f32,
}

impl ClipSlot {
    /// `[frame A offset, frame B offset, blend bits, 0]` for `time` seconds into the loop.
    pub fn figure(&self, time: f32) -> [u32; 4] {
        let n = self.frames.max(1);
        let f = (time * self.rate).rem_euclid(n as f32);
        let a = (f.floor() as u32).min(n - 1);
        let b = (a + 1) % n;
        let t = f - a as f32;
        [self.base + a * self.bones, self.base + b * self.bones, t.to_bits(), 0]
    }

    /// Loop length in seconds.
    pub fn duration(&self) -> f32 {
        self.frames as f32 / self.rate.max(1e-3)
    }

    /// Like [`Self::figure`] for a one-shot: holds the first frame before 0 and the last after
    /// the end.
    pub fn figure_once(&self, time: f32) -> [u32; 4] {
        let n = self.frames.max(1);
        let f = (time * self.rate).clamp(0.0, (n - 1) as f32);
        let a = (f.floor() as u32).min(n - 1);
        let b = (a + 1).min(n - 1);
        let t = f - a as f32;
        [self.base + a * self.bones, self.base + b * self.bones, t.to_bits(), 0]
    }

    /// One-shot length in seconds (first to last frame).
    pub fn play_length(&self) -> f32 {
        self.frames.saturating_sub(1) as f32 / self.rate.max(1e-3)
    }
}

/// Every frame of every clip in use, as model-space bone matrices.
#[derive(Default)]
pub struct BoneAtlas {
    pub matrices: Vec<[f32; 16]>,
    slots: HashMap<usize, ClipSlot>,
}

impl BoneAtlas {
    /// Adds a clip (once; keyed by its `Arc`) and returns its slot.
    pub fn add(&mut self, anim: &Arc<Anim>) -> ClipSlot {
        let key = Arc::as_ptr(anim) as usize;
        if let Some(s) = self.slots.get(&key) {
            return *s;
        }
        let bones = anim.bones.len().max(1) as u32;
        let base = self.matrices.len() as u32;
        let frames = anim.frames.len().max(1) as u32;
        for f in 0..frames as usize {
            let mut m = anim.world_matrices(f);
            m.resize(bones as usize, [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]);
            self.matrices.extend(m);
        }
        let rate = if anim.frame_rate > 0.0 { anim.frame_rate } else { 20.0 };
        let slot = ClipSlot { base, frames, bones, rate };
        self.slots.insert(key, slot);
        slot
    }

    /// The slot of a clip added before.
    pub fn slot(&self, anim: &Arc<Anim>) -> Option<ClipSlot> {
        self.slots.get(&(Arc::as_ptr(anim) as usize)).copied()
    }

    /// The matrices as bytes for the storage buffer.
    pub fn bytes(&self) -> Vec<u8> {
        self.matrices.iter().flatten().flat_map(|f| f.to_le_bytes()).collect()
    }
}

/// One figure slot: a [`ClipSlot::figure`] `[frame A, frame B, blend A->B bits, fade]`, where
/// `fade` is 0, or 1 + the index of the figure's [`Fade`] in the frame's fade entries.
pub type Figure = [u32; 4];

/// A figure's cross-fade out of the poses frozen at its last loop-clip changes (`view::ClipBlend`,
/// UNITS_TERRAIN_FIDELITY.md §1.9): up to two clip frames, newest first, each
/// `[frame A, frame B, blend A->B bits, weight bits]` (weight 0 = unused); the figure's own frame
/// has the rest of the weight. Only fading figures have one, so the per-frame upload of the
/// figure slots stays at four words per slot.
pub type Fade = [[u32; 4]; 2];

/// The [`Fade`] of frozen frames `frames` (each a [`ClipSlot::figure`]) at weights `keeps`.
pub fn fade(frames: [[u32; 4]; 2], keeps: [f32; 2]) -> Fade {
    std::array::from_fn(|k| {
        let [a, b, t, _] = frames[k];
        [a, b, t, keeps[k].to_bits()]
    })
}

/// Writes figure slots as bytes for the storage buffer into `out`, reusing its allocation (the
/// buffer is rewritten every frame).
pub fn write_figure_bytes(figures: &[Figure], out: &mut Vec<u8>) {
    out.clear();
    out.extend(figures.iter().flatten().flat_map(|u| u.to_le_bytes()));
}

/// The frame's [`Fade`] entries (main world, filled by `view::sync_views`) and the fade buffer
/// they go to. Only the entries in use are copied to the GPU: the render world writes them into
/// the start of that buffer (sized at one entry per figure slot) with `write_buffer`, so a frame
/// without a cross-fade uploads nothing.
#[derive(Resource, Default)]
pub struct FadeUpload {
    pub buffer: Option<AssetId<ShaderBuffer>>,
    pub entries: Vec<Fade>,
}

/// The render world's copy of [`FadeUpload`], as bytes, until it is written.
#[derive(Resource, Default)]
struct RenderFades {
    buffer: Option<AssetId<ShaderBuffer>>,
    bytes: Vec<u8>,
    dirty: bool,
}

/// Registers the fade upload: [`FadeUpload`] in the main world, its extraction and the write (in
/// the render app; an app without one, e.g. a headless test, draws nothing to upload to).
pub fn add_fade_upload(app: &mut App) {
    use bevy::render::{ExtractSchedule, Render, RenderApp, RenderSystems};
    app.init_resource::<FadeUpload>();
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<RenderFades>()
            .add_systems(ExtractSchedule, extract_fades)
            .add_systems(Render, write_fades.in_set(RenderSystems::PrepareResources));
    }
}

fn extract_fades(main: bevy::render::Extract<Res<FadeUpload>>, mut ours: ResMut<RenderFades>) {
    if !main.is_changed() {
        return;
    }
    ours.buffer = main.buffer;
    ours.bytes.clear();
    ours.bytes.extend(main.entries.iter().flatten().flatten().flat_map(|u| u.to_le_bytes()));
    ours.dirty = !ours.bytes.is_empty();
}

fn write_fades(
    mut ours: ResMut<RenderFades>,
    gpu: Res<bevy::render::render_asset::RenderAssets<bevy::render::storage::GpuShaderBuffer>>,
    queue: Res<bevy::render::renderer::RenderQueue>,
) {
    if !ours.dirty {
        return;
    }
    ours.dirty = false;
    // Before the buffer's first upload (the battle's first frame) its fades read as weight 0.
    let Some(buffer) = ours.buffer.and_then(|id| gpu.get(id)) else { return };
    let len = ours.bytes.len().min(buffer.buffer.size() as usize);
    queue.write_buffer(&buffer.buffer, 0, &ours.bytes[..len]);
}

/// PROVISIONAL per-man phase in `[0, 1)`: a hash of the unit id and the man's index.
pub fn phase(unit: u32, man: usize) -> f32 {
    let mut h = (unit as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (man as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_slot_frames_loop_and_blend() {
        let s = ClipSlot { base: 100, frames: 4, bones: 10, rate: 20.0 };
        assert_eq!(s.figure(0.0)[..2], [100, 110]);
        let f = s.figure(0.175); // frame 3.5: 3 -> 0, blend 0.5
        assert_eq!(f[..2], [130, 100]);
        assert!((f32::from_bits(f[2]) - 0.5).abs() < 1e-4);
        assert!((s.duration() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn phases_differ_and_stay_in_range() {
        let p: Vec<f32> = (0..100).map(|i| phase(3, i)).collect();
        assert!(p.iter().all(|x| (0.0..1.0).contains(x)));
        let mut sorted = p.clone();
        sorted.sort_by(f32::total_cmp);
        sorted.dedup();
        assert!(sorted.len() > 95, "phases should be (almost) all different");
        assert_eq!(phase(3, 7), phase(3, 7));
    }
}

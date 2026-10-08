// GPU skinning of soldiers and mounts (NapoleonRust, our own shader; see battle/skin.rs).
//
// The original formats store, per vertex and per bone influence, the position and normal in
// that bone's frame (no bind pose): posed = sum_k w_k * (M_k * p_k). Up to 4 influences here
// (soldiers use 2, rigid equipment 1). M_k come from `bones` (every clip frame's model-space bone
// matrices); `figures[tag]` says which frames this man shows now (his own phase) and how far
// between them. The file space is left-handed (a man faces +Z); like the CPU path we negate Z.
// The fragment stage is Bevy's StandardMaterial.

#import bevy_pbr::{
    mesh_functions,
    forward_io::VertexOutput,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> bones: array<mat4x4<f32>>;
// x: first matrix of frame A, y: of frame B, z: bitcast blend A->B, w: unused
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> figures: array<vec4<u32>>;

struct SkinVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) p0: vec3<f32>,
    @location(1) n0: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(8) p1: vec3<f32>,
    @location(9) p2: vec3<f32>,
    @location(10) p3: vec3<f32>,
    @location(11) n1: vec3<f32>,
    @location(12) n2: vec3<f32>,
    @location(13) n3: vec3<f32>,
    @location(14) joints: vec4<u32>,
    @location(15) weights: vec4<f32>,
}

const NO_BONE: u32 = 65535u;

fn bone(fig: vec4<u32>, j: u32) -> mat4x4<f32> {
    if j == NO_BONE {
        return mat4x4<f32>(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0));
    }
    let t = bitcast<f32>(fig.z);
    return bones[fig.x + j] * (1.0 - t) + bones[fig.y + j] * t;
}

struct Posed {
    p: vec3<f32>,
    n: vec3<f32>,
}

fn pose(v: SkinVertex) -> Posed {
    let fig = figures[mesh_functions::get_tag(v.instance_index)];
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    let ps = array<vec3<f32>, 4>(v.p0, v.p1, v.p2, v.p3);
    let ns = array<vec3<f32>, 4>(v.n0, v.n1, v.n2, v.n3);
    for (var k = 0u; k < 4u; k++) {
        let w = v.weights[k];
        if w > 0.0 {
            let m = bone(fig, v.joints[k]);
            p += w * (m * vec4<f32>(ps[k], 1.0)).xyz;
            n += w * (m * vec4<f32>(ns[k], 0.0)).xyz;
        }
    }
    var out: Posed;
    out.p = vec3<f32>(p.x, p.y, -p.z);
    out.n = normalize(vec3<f32>(n.x, n.y, -n.z) + vec3<f32>(0.0, 1e-6, 0.0));
    return out;
}

@vertex
fn vertex(v: SkinVertex) -> VertexOutput {
    let posed = pose(v);
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    var out: VertexOutput;
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(posed.p, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(posed.n, v.instance_index);
#ifdef VERTEX_UVS_A
    out.uv = v.uv;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, world_from_local[3]);
#endif
    return out;
}

// Prepass / shadow-pass version of soldier_skin.wgsl (same skinning, prepass outputs).

#import bevy_pbr::{
    mesh_functions,
    prepass_io::VertexOutput,
    view_transformations::position_world_to_clip,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<storage, read> bones: array<mat4x4<f32>>;
// x, y: first matrix of frames A and B, z: bitcast blend A->B, w: 0 or 1 + the index of the
// figure's cross-fade in `fades` (battle/skin.rs `Figure`).
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var<storage, read> figures: array<vec4<u32>>;
// A figure's cross-fade out of a clip change: up to two frozen poses, newest first (x, y, z as a
// figure; w: bitcast weight, 0 = unused); the figure's own frame has the rest of the weight
// (battle/skin.rs `Fade`, view.rs `ClipBlend`).
struct Fade {
    a: vec4<u32>,
    b: vec4<u32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var<storage, read> fades: array<Fade>;

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

fn frame(f: vec4<u32>, j: u32) -> mat4x4<f32> {
    let t = bitcast<f32>(f.z);
    return bones[f.x + j] * (1.0 - t) + bones[f.y + j] * t;
}

fn bone(fig: vec4<u32>, fade: Fade, j: u32) -> mat4x4<f32> {
    if j == NO_BONE {
        return mat4x4<f32>(vec4(1.0, 0.0, 0.0, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(0.0, 0.0, 1.0, 0.0), vec4(0.0, 0.0, 0.0, 1.0));
    }
    let m = frame(fig, j);
    if fig.w == 0u {
        return m;
    }
    let ka = bitcast<f32>(fade.a.w);
    let kb = bitcast<f32>(fade.b.w);
    var out = m * (1.0 - ka - kb) + frame(fade.a, j) * ka;
    if kb > 0.0 {
        out += frame(fade.b, j) * kb;
    }
    return out;
}

@vertex
fn vertex(v: SkinVertex) -> VertexOutput {
    let fig = figures[mesh_functions::get_tag(v.instance_index)];
    var fade: Fade;
    if fig.w != 0u {
        fade = fades[fig.w - 1u];
    }
    var p = vec3<f32>(0.0);
    var n = vec3<f32>(0.0);
    let ps = array<vec3<f32>, 4>(v.p0, v.p1, v.p2, v.p3);
    let ns = array<vec3<f32>, 4>(v.n0, v.n1, v.n2, v.n3);
    for (var k = 0u; k < 4u; k++) {
        let w = v.weights[k];
        if w > 0.0 {
            let m = bone(fig, fade, v.joints[k]);
            p += w * (m * vec4<f32>(ps[k], 1.0)).xyz;
            n += w * (m * vec4<f32>(ns[k], 0.0)).xyz;
        }
    }
    let world_from_local = mesh_functions::get_world_from_local(v.instance_index);
    var out: VertexOutput;
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(p.x, p.y, -p.z, 1.0));
    out.position = position_world_to_clip(out.world_position.xyz);
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef VERTEX_UVS_A
    out.uv = v.uv;
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(normalize(vec3<f32>(n.x, n.y, -n.z) + vec3<f32>(0.0, 1e-6, 0.0)), v.instance_index);
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = out.world_position;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = v.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(v.instance_index, world_from_local[3]);
#endif
    return out;
}

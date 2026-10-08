// Battle-map tree billboards (NapoleonRust, our own shader; see trees.rs).
//
// Each tree is 4 vertices at its base point; the vertex shader spreads them into a quad that
// faces the camera and turns only about the vertical, and picks which of the tree's 8 pictures
// to show from the horizontal direction it is seen from. The fragment shader alpha-tests.
// PROVISIONAL: picture 0's direction and the turning sense, the 0.33 threshold, flat lighting.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_functions::get_world_from_local

struct TreeParams {
    sun_dir: vec4<f32>,
    sun_colour: vec4<f32>,
    ambient: vec4<f32>,
    // x: alpha-test threshold
    misc: vec4<f32>,
    rects: array<vec4<f32>, 256>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: TreeParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var smp: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    // tree base point (on the ground)
    @location(0) position: vec3<f32>,
    // x: -0.5 .. 0.5 across, y: 0 (ground) .. 1 (top)
    @location(1) corner: vec2<f32>,
    // width, height in metres
    @location(2) size: vec2<f32>,
    // x: species slot, y: pictures per tree
    @location(3) info: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    // the two pictures either side of the view direction
    @location(0) uv: vec2<f32>,
    @location(2) uv2: vec2<f32>,
    // 0 = hidden (the near 3D tree is shown), 1 = fully shown
    @location(1) fade: f32,
    // blend from picture k0 (0) to k1 (1)
    @location(3) blend: f32,
}

const TAU: f32 = 6.28318530718;

@vertex
fn vertex(v: Vertex) -> VertexOut {
    let base = (get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0)).xyz;
    var to_cam = view.world_position.xz - base.xz;
    if dot(to_cam, to_cam) < 1e-6 {
        to_cam = vec2<f32>(0.0, 1.0);
    }
    let d = normalize(to_cam);
    // Right-hand side of the quad as seen from the camera.
    let right = vec3<f32>(d.y, 0.0, -d.x);
    let p = base + right * (v.corner.x * v.size.x) + vec3<f32>(0.0, v.corner.y * v.size.y, 0.0);

    // Picture k shows the view azimuth k/n turns (PROVISIONAL direction of picture 0); like the
    // original we blend the two pictures either side of the camera's azimuth.
    let n = max(v.info.y, 1.0);
    let a = fract(atan2(d.x, d.y) / TAU) * n;
    let k0 = u32(floor(a)) % u32(n);
    let k1 = (k0 + 1u) % u32(n);
    let r0 = params.rects[u32(v.info.x) * 8u + k0];
    let r1 = params.rects[u32(v.info.x) * 8u + k1];
    // Picture rows: v_min at the top of the tree, v_max at its foot.
    let s = v.corner.x + 0.5;

    var out: VertexOut;
    out.clip = view.clip_from_world * vec4<f32>(p, 1.0);
    out.uv = vec2<f32>(mix(r0.x, r0.z, s), mix(r0.w, r0.y, v.corner.y));
    out.uv2 = vec2<f32>(mix(r1.x, r1.z, s), mix(r1.w, r1.y, v.corner.y));
    out.blend = fract(a);
    // Fade in over the band where the near 3D tree fades out (speedtree.wgsl).
    let far = params.misc.y;
    let band = max((far - params.misc.z) * 0.1, 1.0);
    out.fade = clamp((distance(view.world_position, base) - (far - band)) / band, 0.0, 1.0);
    return out;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let c = mix(textureSample(tex, smp, in.uv), textureSample(tex, smp, in.uv2), in.blend);
    if c.a < mix(1.01, params.misc.x, in.fade) {
        discard;
    }
    // Flat light: half the sun (a billboard has no single normal) plus the ambient.
    let light = params.sun_colour.rgb * 0.5 * max(params.sun_dir.y, 0.2) + params.ambient.rgb;
    return vec4<f32>(c.rgb * light, 1.0);
}

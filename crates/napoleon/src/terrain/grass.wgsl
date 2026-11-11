// Battle-map grass (NapoleonRust, our own shader; see grass.rs). The rules are those of the
// original's fx\grass.fx (shipped as HLSL text, CONFIRMED by the file), with its numbers taken
// from the material description materials/vanilla/grass.toml:
// - each blade is 4 vertices at its foot; a corner's weights w (0..1) spread it into a quad of the
//   blade's size facing the view direction (leaned towards the blade), lifted a little;
// - it fades over the last part of a distance that differs per blade (its phase), with heights
//   counted three times, the edge dissolved by the jitter pattern;
// - it sways on a sine of the time, its position and phase, and leans with the wind's gust;
// - its colour is the picture added to the ground's colour under it (the colour map at the foot,
//   over the tiled ground texture), turning into the plain ground colour as it fades.
// The original samples with SRGBTEXTURE = false, so the maths runs on the stored values (our
// textures are UNORM), then converts to linear. PROVISIONAL: the time input of the sway (grass.fx
// `g_sin_elapsed`, given the elapsed seconds), the lighting (sun + ambient on the ground normal,
// no lightmap, shadows or fog).

#import bevy_pbr::mesh_view_bindings::{view, globals}

struct GrassParams {
    sun_dir: vec4<f32>,
    sun_colour: vec4<f32>,
    ambient: vec4<f32>,
    // spread, rise, lift, view_lean
    shape: vec4<f32>,
    // fade_distance, fade_spread, fade_start, fade_width
    fade: vec4<f32>,
    // fade_height_weight, dissolve_start, alpha_gain, alpha-test cutoff
    fade2: vec4<f32>,
    // sway_amplitude, sway_speed, sway_speed_spread, sway_phase
    sway: vec4<f32>,
    // wind (x, z), gust, gust_scale * grass_wind_scale
    gust: vec4<f32>,
    // level 0 width, height (m), tiled texture repeats across it
    map: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: GrassParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var atlas_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var atlas_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var colour_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var colour_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var tile_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var tile_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var jitter_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var jitter_smp: sampler;

struct Vertex {
    // the blade's foot (world)
    @location(0) position: vec3<f32>,
    // the ground's normal there
    @location(1) normal: vec3<f32>,
    // the corner's place in the atlas page
    @location(2) uv: vec2<f32>,
    // the blade's width and height in metres
    @location(3) size: vec2<f32>,
    // corner weights (x across, y up), phase 0..1
    @location(4) weight: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) weight: vec2<f32>,
    // the colour map at the foot (grass.fx's "near" colour)
    @location(2) near: vec4<f32>,
    @location(3) world_xz: vec2<f32>,
    @location(4) normal: vec3<f32>,
    // 1 shown .. 0 faded
    @location(5) lod: f32,
}

// A battle position (Bevy x, z) as a place on level 0's colour map.
fn map_uv(xz: vec2<f32>) -> vec2<f32> {
    return xz / params.map.xy + 0.5;
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    let phase = v.weight.z;
    let w = params.shape.x * (v.weight.xy - 0.5) + vec2<f32>(0.0, params.shape.y);
    let cam = view.world_position;
    let forward = -normalize(view.world_from_view[2].xyz);
    let dir = normalize(mix(forward, normalize(v.position - cam), params.shape.w));
    let right = normalize(vec3<f32>(-dir.z, 0.0, dir.x));
    let up = normalize(cross(right, dir));
    var pos = v.position + up * (v.size.y * w.y) + right * (v.size.x * w.x) + vec3<f32>(0.0, params.shape.z, 0.0);

    var diff = pos - cam;
    diff.y = diff.y * params.fade2.x;
    let distance = params.fade.x * (1.0 - params.fade.y * phase);
    let a = saturate((length(diff) / distance - params.fade.z) / params.fade.w);

    let theta = globals.time * (params.sway.y + params.sway.z * phase) + pos.x + pos.y + phase * params.sway.w;
    pos.x = pos.x + params.sway.x * v.weight.y * sin(theta);
    pos.z = pos.z + params.sway.x * v.weight.y * cos(theta);
    pos = pos + vec3<f32>(params.gust.x, 0.0, params.gust.y) * (v.weight.y * params.gust.z * params.gust.w);

    var out: VertexOut;
    out.clip = view.clip_from_world * vec4<f32>(pos, 1.0);
    out.uv = v.uv;
    out.weight = v.weight.xy;
    out.near = textureSampleLevel(colour_tex, colour_smp, map_uv(v.position.xz), 0.0);
    out.world_xz = pos.xz;
    out.normal = v.normal;
    out.lod = 1.0 - a;
    return out;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas_tex, atlas_smp, in.uv);
    let tile = textureSample(tile_tex, tile_smp, map_uv(in.world_xz) * params.map.z);
    let jitter = saturate(textureSample(jitter_tex, jitter_smp, in.weight).r);
    if tex.a < 0.002 {
        discard;
    }
    let terrain = mix(tile.rgb, in.near.rgb, in.near.a);
    let lod = saturate(2.0 * in.lod + jitter - 0.5);
    var res = saturate(terrain + tex.rgb - 0.5);
    res = mix(terrain, res, saturate(2.0 * (lod - params.fade2.y)));
    let alpha = min(tex.a * params.fade2.z, lod);
    if alpha < params.fade2.w {
        discard;
    }
    let n = normalize(in.normal);
    let light = params.sun_colour.rgb * max(dot(n, params.sun_dir.xyz), 0.0) + params.ambient.rgb;
    return vec4<f32>(srgb_to_linear(res) * light, 1.0);
}

// Battle sea surface (NapoleonRust, our own shader), a port of the non-ultra-sea path of the
// original's `fx\ocean.fx` (shipped as HLSL text in data.pack; read to learn the maths, never
// copied). Every value in `WaterParams` and where the exe takes it from: `terrain::water` and
// `analysis/graphics/WATER.md`.
//
// What the original does, and what is here:
// - Two wave textures hold the surface **slopes as angles**: `convert_to_angle(texel) = (texel -
//   0.5) * 2PI`, sampled at `position.xz * g_sea_uv_scale` (the chop, `sea\sea`) and `position.xz *
//   g_swell_uv_scale` (the swell, `sea\swell`). The engine rebuilds each texture every frame as the
//   blend of two neighbouring animation frames, dequantised to 16 bits (`0x010E21F0`,
//   `0x010E3140`); here every frame is a layer of a texture array and the same blend and
//   dequantisation run per texel (both linear, so the result is the same).
// - `sea_scale = max(1 - g_sea_decay * position.w, 0)` fades the chop out with view depth (the
//   clip w), and `surface_angle = swell_angle + sea_angle * sea_attenuation` keeps the chop off the
//   shore. `sea_attenuation` is a per-vertex attribute in the original (source UNKNOWN); here it
//   comes from the sea-bed depth texture (INFERRED).
// - The normal is rebuilt with `sincos`: `n.xz = sin(angle)`, `n.y = cos(x)cos(y)`, then normalised.
// - Colour: `water = g_sea_deep_colour * (dot(n, -light_direction) + 0.5)`; the reflection vector is
//   `reflect(view, n)` with `y = abs(y)`, looked up in the sky reflection cube (we have none: the
//   map's SKYGEN sky model, `sky.fx CalcSkyColour`, is evaluated for it instead, PROVISIONAL); the
//   two are mixed with Schlick `fresnel(view, n)` at `g_fresnel_R0`; then the Blinn-Phong highlight
//   `blinn_phong(-view, n, g_sea_shininess * 0.7)` lerps towards the sun colour.
// - Foam (the `foam_enabled` branch, `sea\combined_foam.tga`): `foam_value` from red, `foam_froth`
//   from green × 0.7 at the `distorted_uv` coordinate, `foam_tendril` from blue, white foam,
//   `MAX_FOAM_DIST` over the view distance. The original runs it only on the `*_foam_*` passes
//   (UNKNOWN which region gets which); here it is off unless `NAPOLEON_SEA_FOAM=1` (PROVISIONAL).
//
// Not ported (deliberately; they must not be added piecemeal):
// - `USE_ULTRA_SEA` / `ENABLE_REFLECTION` / `ENABLE_REFRACTION` (the shallow/deep blend, the
//   wind-offset second chop sample, the reflection/refraction render targets);
// - `apply_fog` / `hdr_encode`: `terrain.wgsl` applies neither and the water must match the
//   terrain, so fog and the HDR post are one item covering both; the colour is written as it is
//   computed (PROVISIONAL colour space, with that item);
// - shadow-map occlusion (the original multiplies `light_frac` and the specular by it);
// - the colour overlay decal and the rain ripples.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_functions::get_world_from_local

struct WaterParams {
    // xyz: direction TOWARDS the sun (Bevy world space, as `terrain.wgsl`; `g_sun_direction` of sky.fx).
    sun_dir: vec4<f32>,
    // rgb: the sun colour (`light_colour` in the original).
    sun_colour: vec4<f32>,
    // The SKYGEN sky model (colours already × their scale). rgb: sky colour, w: circumsolar decay.
    sky: vec4<f32>,
    // rgb: circumsolar colour, w: horizon decay.
    circumsolar: vec4<f32>,
    // rgb: horizon colour, w: backscatter decay.
    horizon: vec4<f32>,
    // rgb: horizon opposite colour.
    horizon_opposite: vec4<f32>,
    // rgb: backscatter colour.
    backscatter: vec4<f32>,
    // x: g_sea_uv_scale, y: g_swell_uv_scale, z: g_sea_decay, w: g_fresnel_R0.
    waves: vec4<f32>,
    // rgb: g_sea_deep_colour.
    deep: vec4<f32>,
    // x: g_sea_shininess * 0.7, y: g_time.
    spec: vec4<f32>,
    // x: foam on (0/1), y: g_froth_value, z: g_foam_uv_scale, w: MAX_FOAM_DIST.
    foam: vec4<f32>,
    // x: g_froth_distortion_speed, y: g_froth_distortion_amount.
    froth: vec4<f32>,
    // x: sea depth in metres that reaches full sea_attenuation, y: sea level.
    depth: vec4<f32>,
    // xy: world position of the sea-bed depth texture's (0, 0) texel, zw: its size in metres.
    depth_rect: vec4<f32>,
    // x, y: the two chop frames (layers) shown now, z: the blend weight towards y.
    sea_frames: vec4<f32>,
    // The same for the swell.
    swell_frames: vec4<f32>,
    // xy: the chop's x / z angle value of byte 0, zw: of byte 255, both over 65535.
    sea_range: vec4<f32>,
    // The same for the swell.
    swell_range: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: WaterParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sea_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sea_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var swell_tex: texture_2d_array<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var swell_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var foam_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var foam_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var depth_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var depth_smp: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // The vertex's clip w (`r.position.w` in the original), for `sea_scale`.
    @location(1) clip_w: f32,
}

/// `ocean.fx convert_to_angle`: the texture stores a slope as an angle over 0..1.
fn convert_to_angle(texel: vec2<f32>) -> vec2<f32> {
    return (texel - vec2<f32>(0.5)) * 2.0 * 3.14159265358979323846;
}

/// One wave texture's angles from its two current frames' samples `a`, `b`: blended, then
/// dequantised from the stored byte range to the 16-bit value the original's texture holds.
fn wave_angle(a: vec2<f32>, b: vec2<f32>, frames: vec4<f32>, range: vec4<f32>) -> vec2<f32> {
    let t = mix(a, b, frames.z);
    return convert_to_angle(range.xy + t * (range.zw - range.xy));
}

/// `sky.fx skygen_get_sky_color` + `CalcSkyColour` for a direction (the stand-in for the sky
/// reflection cube). `vdir.y >= 0` here: the sea's reflection vector is folded up.
fn sky_reflection(dir: vec3<f32>) -> vec3<f32> {
    var vdir = dir;
    let flat_dir = vec3<f32>(vdir.x, 0.0, vdir.z);
    // Straight up has no horizon direction (HLSL's normalize of zero); any one gives the same sky.
    let horizon_dir = select(vec3<f32>(1.0, 0.0, 0.0), normalize(flat_dir), dot(flat_dir, flat_dir) > 1e-12);
    if vdir.y < 0.0 {
        vdir = horizon_dir;
    }
    let view_angle = acos(clamp(vdir.y, -1.0, 1.0));
    let sun = params.sun_dir.xyz;
    let sun_to_view = acos(clamp(dot(vdir, sun), -1.0, 1.0));
    let flat_sun = vec3<f32>(sun.x, 0.0, sun.z);
    let horizon_sun = select(vec3<f32>(1.0, 0.0, 0.0), normalize(flat_sun), dot(flat_sun, flat_sun) > 1e-12);
    let fhorizonsun = dot(horizon_dir, horizon_sun);

    let cos_sun_to_view = cos(sun_to_view * params.horizon.w);
    let circumsolar = mix(params.sky.rgb, params.circumsolar.rgb, saturate(exp(params.sky.w * sun_to_view)));
    let horizon_colour = mix(params.horizon_opposite.rgb, params.horizon.rgb, (fhorizonsun + 1.0) / 2.0);
    // cos(view_angle) = vdir.y, 0 on the horizon: the exponent runs to -inf there (decay < 0).
    let horizon = mix(horizon_colour, circumsolar, saturate(exp(params.circumsolar.w / max(cos(view_angle), 1e-6))));
    let back_scatter = params.backscatter.rgb * cos_sun_to_view * cos_sun_to_view;
    return saturate(horizon + back_scatter);
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    let world = (get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0)).xyz;
    var out: VertexOut;
    out.clip = view.clip_from_world * vec4<f32>(world, 1.0);
    out.world = world;
    out.clip_w = out.clip.w;
    return out;
}

/// The sea bed's depth under a world position in metres: the texture holds 0..1 over
/// 0..`params.depth.x` (outside the map it clamps, and clamp-to-edge makes the border the map's
/// edge depth, which is the deep open sea).
fn sea_depth(world_xz: vec2<f32>) -> f32 {
    let uv = (world_xz - params.depth_rect.xy) / params.depth_rect.zw;
    return textureSampleLevel(depth_tex, depth_smp, uv, 0.0).r * params.depth.x;
}

/// `ocean.fx distorted_uv`: the foam alpha channel as a signed offset, scrolled with time.
fn distorted_uv(uv: vec2<f32>) -> vec2<f32> {
    let distortion_uv = uv + vec2<f32>(0.0, params.spec.y * params.froth.x);
    let distortion = textureSample(foam_tex, foam_smp, distortion_uv).a * 2.0 - 1.0;
    return uv + distortion * params.froth.y;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    // `sea_scale.x`: the chop dies with view depth, so the sea reads flat far off.
    let sea_scale = max(1.0 - params.waves.z * in.clip_w, 0.0);

    let uv_sea = in.world.xz * params.waves.x;
    let uv_swell = in.world.xz * params.waves.y;
    let sf = params.sea_frames;
    let wf = params.swell_frames;
    let sea_angle = wave_angle(
        textureSample(sea_tex, sea_smp, uv_sea, i32(sf.x)).xy,
        textureSample(sea_tex, sea_smp, uv_sea, i32(sf.y)).xy,
        sf,
        params.sea_range,
    ) * sea_scale;
    let swell_angle = wave_angle(
        textureSample(swell_tex, swell_smp, uv_swell, i32(wf.x)).xy,
        textureSample(swell_tex, swell_smp, uv_swell, i32(wf.y)).xy,
        wf,
        params.swell_range,
    );

    let attenuation = clamp(sea_depth(in.world.xz) / max(params.depth.x, 1e-3), 0.0, 1.0);
    let surface_angle = swell_angle + sea_angle * attenuation;

    // sincos(angle): n.xz = sin(angle), n.y = cos(x) * cos(y).
    let s = sin(surface_angle);
    let c = cos(surface_angle);
    let surface_normal = normalize(vec3<f32>(s.x, c.x * c.y, s.y));

    // `view_vec`: the unit vector from the camera to the surface, and its length.
    let to_surface = in.world - view.world_position;
    let view_dist = length(to_surface);
    let view_vec = to_surface / max(view_dist, 1e-6);

    let water_colour = params.deep.rgb * (dot(surface_normal, params.sun_dir.xyz) + 0.5);

    // `reflect(view_vec, n)` with `y = abs(y)`, so the sea never looks below the horizon.
    var reflection = reflect(view_vec, surface_normal);
    reflection.y = abs(reflection.y);
    let env = sky_reflection(reflection);

    // `fresnel` = Schlick with R0 = g_fresnel_R0.
    let f = saturate(params.waves.w + (1.0 - params.waves.w) * pow(1.0 - dot(view_vec, -surface_normal), 5.0));
    var colour = mix(water_colour, env, f);

    // `blinn_phong(-view_vec, n, g_sea_shininess * 0.7)`: the halfway vector of the unit vector to
    // the camera and the direction to the sun.
    let halfway = normalize(-view_vec + params.sun_dir.xyz);
    let specular = pow(saturate(dot(surface_normal, halfway)), params.spec.x);
    colour = mix(colour, params.sun_colour.rgb, specular);

    if params.foam.x > 0.5 {
        // The `foam_enabled` branch of `render_pixel_ps3`, the non-ultra-sea path. `uv_sea` is the
        // chop's own coordinate (`position.xz * g_sea_uv_scale`).
        let max_foam = params.foam.w;
        let foam = textureSample(foam_tex, foam_smp, uv_sea).r * (1.0 - min(view_dist, max_foam) / max_foam);
        let froth = textureSample(foam_tex, foam_smp, distorted_uv(uv_sea * params.foam.z)).g * 0.7;
        let tendril = textureSample(foam_tex, foam_smp, uv_sea * params.foam.z).b;

        var alpha: f32;
        if foam >= params.foam.y {
            alpha = foam * froth;
        } else {
            // Scale f to 0..1 and ramp down quickly between froth and loose foam: `pow(t, 3)`.
            let t = pow(foam / params.foam.y, 3.0);
            alpha = mix(tendril, froth, t) * foam;
        }
        // foam_colour = white in the non-ultra-sea path.
        colour = mix(colour, vec3<f32>(1.0), alpha);
    }

    return vec4<f32>(colour, 1.0);
}

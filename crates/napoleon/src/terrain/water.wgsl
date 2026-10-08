// Battle sea surface (NapoleonRust, our own shader), a port of the original's `fx\ocean.fx`
// (shipped as HLSL text in data.pack; read to learn the maths, never copied).
//
// What the original does, and what is here:
// - The mesh is a **camera-centred banded grid** (`ocean.fx`: `g_band_size` 64, `g_grid_increment`
//   0.5, `g_start_band` 4) of concentric square rings at the sea level, built by the Rust side
//   (`terrain::water`); this shader only needs the vertex position, so the ring shape is invisible
//   from here.
// - Two tiling textures hold the wave **slopes as angles**: `convert_to_angle(texel) = (texel - 0.5)
//   * 2PI`, sampled with `world.xz * g_sea_uv_scale` (the short chop) and `world.xz *
//   g_swell_uv_scale` (the long swell). Ported as-is.
// - `r.sea_scale.x = max(1 - g_sea_decay * position.w, 0)` fades the chop out with view depth (the
//   clip w, which is what the original multiplies), and
//   `surface_angle = swell_angle + sea_angle * sea_attenuation` keeps the chop off the shore.
//   `sea_attenuation` is a per-vertex attribute in the original (source UNKNOWN); here it comes
//   from the sea-bed depth texture (INFERRED).
// - The normal is rebuilt with `sincos`: `n.xz = sin(angle)`, `n.y = cos(x)cos(y)`, then normalised.
// - Colour: `water = g_sea_deep_colour * (dot(n, -light_direction) + 0.5)`; the reflection vector is
//   `reflect(view, n)` with `y = abs(y)`, looked up in the sky reflection cube (we have no cube
//   map: the map's sky colour stands in, PROVISIONAL); the two are mixed with Schlick
//   `fresnel(view, n)` at `g_fresnel_R0 = 0.5`; then the Blinn-Phong highlight
//   `blinn_phong(-view, n, g_sea_shininess * 0.7)` lerps towards the sun colour.
// - Foam (the `foam_enabled` branch, from `sea\combined_foam.tga`): `foam_value` from red,
//   `foam_froth` from green × 0.7, `foam_tendril` from blue, `g_foam_uv_scale` 15,
//   `g_froth_value` 0.75, the same `pow(t, 3)` ramp, `MAX_FOAM_DIST` 300. The original only enables
//   it on the `sm_render_*_foam_*` passes, which it drives where the sea meets the land and where
//   ships make a wake (UNKNOWN which pass each region gets); here it is **off by default** and, when
//   `NAPOLEON_SEA_FOAM=1` turns it on, covers the whole surface (PROVISIONAL until the shoreline
//   pass, `analysis/graphics/WATER.md` §8, replaces it).
//
// Not ported (deliberately; they must not be added piecemeal):
// - `ENABLE_REFLECTION` / `ENABLE_REFRACTION` (`USE_ULTRA_SEA`: the shallow/deep colour blend and
//   the reflection/refraction render targets) — those need render targets we do not draw yet;
// - `apply_fog` / `hdr_encode`: `terrain.wgsl` applies neither and the water must match the
//   terrain, so distance fog is one item covering both (`WATER.md` §6);
// - shadow-map occlusion (the original multiplies `light_frac` and the specular by it);
// - the colour overlay decal and the rain ripples.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_functions::get_world_from_local

struct WaterParams {
    // xyz: direction TOWARDS the sun (Bevy world space, as `terrain.wgsl`).
    sun_dir: vec4<f32>,
    // rgb: the sun colour (`light_colour` in the original).
    sun_colour: vec4<f32>,
    // rgb: what `get_sky_reflection_color` returns; the map's sky colour (PROVISIONAL).
    sky: vec4<f32>,
    // x: g_sea_uv_scale, y: g_swell_uv_scale, z: g_sea_decay, w: g_fresnel_R0.
    waves: vec4<f32>,
    // rgb: g_sea_deep_colour.
    deep: vec4<f32>,
    // x: g_sea_shininess * 0.7, y: g_time.
    spec: vec4<f32>,
    // x: foam on (0/1), y: g_froth_value, z: g_foam_uv_scale, w: MAX_FOAM_DIST.
    foam: vec4<f32>,
    // x: sea depth in metres that reaches full sea_attenuation, y: sea level.
    depth: vec4<f32>,
    // xy: world position of the sea-bed depth texture's (0, 0) texel, zw: its size in metres.
    depth_rect: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: WaterParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var sea_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var sea_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var swell_tex: texture_2d<f32>;
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
    @location(1) uv: vec2<f32>,
}

/// `ocean.fx convert_to_angle`: the texture stores a slope as an angle over 0..1.
fn convert_to_angle(texel: vec2<f32>) -> vec2<f32> {
    return (texel - vec2<f32>(0.5)) * 2.0 * 3.14159265358979323846;
}

@vertex
fn vertex(v: Vertex) -> VertexOut {
    let world = (get_world_from_local(v.instance_index) * vec4<f32>(v.position, 1.0)).xyz;
    var out: VertexOut;
    out.clip = view.clip_from_world * vec4<f32>(world, 1.0);
    out.world = world;
    out.uv = v.uv;
    return out;
}

/// The sea bed's depth under a world position in metres: the texture holds 0..1 over
/// 0..`params.depth.x` (outside the map it clamps, and clamp-to-edge makes the border the map's
/// edge depth, which is the deep open sea).
fn sea_depth(world_xz: vec2<f32>) -> f32 {
    let uv = (world_xz - params.depth_rect.xy) / params.depth_rect.zw;
    return textureSampleLevel(depth_tex, depth_smp, uv, 0.0).r * params.depth.x;
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    // `sea_scale.x`: the chop dies with view depth, so the sea reads flat far off. The original
    // multiplies the clip w of the vertex; the fragment's `@builtin(position).w` is its reciprocal.
    let clip_w = 1.0 / max(in.clip.w, 1e-6);
    let sea_scale = max(1.0 - params.waves.z * clip_w, 0.0);

    var sea_angle = convert_to_angle(textureSample(sea_tex, sea_smp, in.world.xz * params.waves.x).xy);
    sea_angle = sea_angle * sea_scale;
    let swell_angle = convert_to_angle(textureSample(swell_tex, swell_smp, in.world.xz * params.waves.y).xy);

    let attenuation = clamp(sea_depth(in.world.xz) / max(params.depth.x, 1e-3), 0.0, 1.0);
    let surface_angle = swell_angle + sea_angle * attenuation;

    // sincos(angle): n.xz = sin(angle), n.y = cos(x) * cos(y).
    let s = sin(surface_angle);
    let c = cos(surface_angle);
    let surface_normal = normalize(vec3<f32>(s.x, c.x * c.y, s.y));

    // `view_vec`: the unit vector from the camera to the surface.
    let view_vec = normalize(in.world - view.world_position);

    let water_colour = params.deep.rgb * (dot(surface_normal, params.sun_dir.xyz) + 0.5);

    // `reflect(view_vec, n)` with `y = abs(y)`, so the sea never looks below the horizon. The
    // original samples a sky cube map; ours is the map's sky colour (PROVISIONAL).
    let reflection = reflect(view_vec, surface_normal);
    let env = params.sky.rgb;

    // `fresnel` = Schlick with R0 = g_fresnel_R0.
    let f = clamp(params.waves.w + (1.0 - params.waves.w) * pow(1.0 - dot(view_vec, -surface_normal), 5.0), 0.0, 1.0);
    var colour = mix(water_colour, env, f);

    // `blinn_phong(-view_vec, n, g_sea_shininess * 0.7)`: the halfway vector of the unit vector to
    // the camera and `light_direction` (the direction the light travels), so `-view_vec + to_sun`.
    let halfway = normalize(-view_vec + params.sun_dir.xyz);
    let specular = pow(clamp(dot(surface_normal, halfway), 0.0, 1.0), params.spec.x);
    colour = mix(colour, params.sun_colour.rgb, specular);

    if params.foam.x > 0.5 {
        // The `foam_enabled` branch of `render_pixel_ps3`, the non-ultra-sea path. `uv_sea` is the
        // same scaled coordinate the wave angles use (`uv_sea_swell.xy = position.xz *
        // g_sea_uv_scale`), not the raw world position: with 1.0 as the scale the foam would repeat
        // every metre (measured in-game: a fine dot grid over the whole sea).
        let uv_sea = in.world.xz * params.waves.x;
        let time = params.spec.y;
        let max_foam = params.foam.w;
        // `foam_value` and the MAX_FOAM_DIST fade, over the clip w like `sea_scale` (the original's
        // `position.w`, not the fragment builtin's reciprocal of it).
        let foam = textureSample(foam_tex, foam_smp, uv_sea).r * (1.0 - min(clip_w, max_foam) / max_foam);

        // `distorted_uv`: the alpha channel as a signed distortion, scrolling with time.
        let distortion = textureSample(foam_tex, foam_smp, uv_sea * params.foam.z + vec2<f32>(0.0, time)).a * 2.0 - 1.0;
        let froth = textureSample(foam_tex, foam_smp, uv_sea * params.foam.z + distortion).g * 0.7;

        var alpha: f32;
        if foam >= params.foam.y {
            alpha = foam * froth;
        } else {
            let tendril = textureSample(foam_tex, foam_smp, uv_sea * params.foam.z).b;
            // Scale f to 0..1 and ramp down quickly between froth and loose foam: `pow(t, 3)`.
            let t = pow(foam / params.foam.y, 3.0);
            alpha = mix(tendril, froth, t) * foam;
        }
        // foam_colour = white in the non-ultra-sea path.
        colour = mix(colour, vec3<f32>(1.0), alpha);
    }

    return vec4<f32>(colour, 1.0);
}

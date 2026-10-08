// Battle terrain material (NapoleonRust, our own shader).
//
// Colour: `colour_map_near` from the game's `fx\terrain_shared.fx_fragment` (shipped as HLSL
// text in data.pack): the per-map colour map ("near", RGB + alpha from the `_alpha` JPEG) is
// mixed with the tiled ground texture ("tile", `textures.xml tiled_detail_map`):
//     res = lerp(near, tile, 0.5);
//     res = lerp(res, saturate(near*0.8 + (tile.a-0.6)*0.5), near.a);
// The original samples with SRGBTEXTURE = false, i.e. this maths runs on gamma-space values;
// our textures are uploaded as UNORM (not sRGB) so we do the same, then convert to linear.
// Lighting: `terrain_lighting` = directional * occlusion + ambient (same file); here a plain
// Lambert sun + flat ambient. PROVISIONAL: no lightmap/occlusion, no ambient cube, no fog.

#import bevy_pbr::forward_io::VertexOutput

struct TerrainParams {
    // xyz: direction TOWARDS the sun (Bevy world space).
    sun_dir: vec4<f32>,
    // rgb: sun colour * scale.
    sun_colour: vec4<f32>,
    // rgb: ambient colour * scale.
    ambient: vec4<f32>,
    // x: tile repeats per colour-map UV unit (8 on the 2048 m level, as in the shader).
    tile: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var colour_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var colour_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var tile_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var tile_smp: sampler;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let near = textureSample(colour_tex, colour_smp, in.uv);
    let tile = textureSample(tile_tex, tile_smp, in.uv * params.tile.x);
    var res = mix(near, tile, 0.5);
    res = mix(res, saturate(near * 0.8 + (tile.a - 0.6) * 0.5), near.a);
    let n = normalize(in.world_normal);
    let light = params.sun_colour.rgb * max(dot(n, params.sun_dir.xyz), 0.0) + params.ambient.rgb;
    return vec4<f32>(srgb_to_linear(saturate(res.rgb)) * light, 1.0);
}

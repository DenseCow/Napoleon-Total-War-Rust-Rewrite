// Battle particles (NapoleonRust, our own shader).
//
// The original draws its particles with the shipped `fx\particle.fx` / `particle2.fx` /
// `particle_distortion.fx` effects (see `analysis/graphics/SHADERS.md` §1). Their fragment stage
// does three things; this shader reproduces two and a stand-in for the third:
//   * samples the emitter's diffuse texture (`SCRIPTED_EFFECT_RENDERING_VARS/texture_1`) and
//     multiplies it by the per-particle colour the CPU side ramped through `COLOUR_INFO`;
//   * PROVISIONAL: multiplies by one scene light for the whole battle. The emitter's own `lighting`
//     weight is NOT applied (it is not in the vertex data), so an unlit flash is lit like smoke
//     (BATTLE_EFFECTS.md §5 row 8);
//   * lets `render_method` choose the blend: `RENDER_METHOD_ALPHA` is ordinary alpha,
//     `RENDER_METHOD_ADDITIVE` adds, which is what makes a muzzle flash read as light rather than
//     paint. The distortion method needs the scene's normal buffer and is not drawn yet.
#import bevy_pbr::forward_io::VertexOutput

struct FxParams {
    // rgb: scene light the particles take (sun colour * ambient, the `.environment` lighting).
    // a: 1.0 for a RENDER_METHOD_ADDITIVE bucket, 0.0 for an alpha one.
    light: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: FxParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var albedo_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var albedo_smp: sampler;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // The original samples with SRGBTEXTURE = false, i.e. the maths runs on the stored values;
    // our particle textures are uploaded as UNORM for the same reason, so we undo the sRGB
    // encode ourselves (same as terrain.wgsl).
    let texel = textureSample(albedo_tex, albedo_smp, in.uv);
    let lo = texel.rgb / 12.92;
    let hi = pow((texel.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    let stored = select(hi, lo, texel.rgb <= vec3<f32>(0.04045));
    let rgb = stored * in.color.rgb * params.light.rgb;
    let a = texel.a * in.color.a;
    if params.light.a > 0.5 {
        // Bevy's `AlphaMode::Add` blend state is premultiplied (`src + (1 - src_a) * dst`) and leaves
        // the premultiply to the shader, as `pbr_functions::premultiply_alpha` does for the standard
        // material. Without it an additive flash darkened what was behind it by `1 - a` and ignored
        // its own fade. Premultiplied with a zero alpha, the blend is `a * src + dst`: a pure add.
        return vec4<f32>(rgb * a, 0.0);
    }
    return vec4<f32>(rgb, a);
}

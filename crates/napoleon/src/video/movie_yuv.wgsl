// Movie pictures: Bink's 8-bit Y, Cb, Cr planes to RGB with the game's own movie-shader maths
// (`fx\sprite.fx`, `pixel_yuv`, technique `normal_yuv_t0`; analysis/video/BINK.md §6), drawn into
// the movie's RGBA texture. Same maths as `ntw_formats::bink::Frame::to_rgba_with` on the CPU.
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct MovieYuv {
    // xy: visible size in pixels; z: 2 / GAMMA_VALUE; w: g_brightness / 1.2.
    view: vec4<f32>,
    // xy: Y plane size, zw: chroma plane size (texels, padded to 8).
    sizes: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> m: MovieYuv;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var y_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var y_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var cb_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var cb_smp: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var cr_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var cr_smp: sampler;

// The original writes these values straight to an 8-bit target; ours is sRGB, so undo its encode.
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3(0.055)) / 1.055, vec3(2.4));
    return select(hi, lo, c <= vec3(0.04045));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // Position in luma pixels; bilinear samplers on all three planes, chroma at half resolution.
    let px = in.uv * m.view.xy;
    let y = textureSample(y_tex, y_smp, px / m.sizes.xy).r;
    let cuv = px * 0.5 / m.sizes.zw;
    let cb = textureSample(cb_tex, cb_smp, cuv).r;
    let cr = textureSample(cr_tex, cr_smp, cuv).r;
    var rgb = 1.164123535 * y
        + vec3(1.595794678 * cr, -0.813476563 * cr - 0.391448975 * cb, 2.017822266 * cb)
        + vec3(-0.87065506, 0.529705048, -1.081668854);
    // The gamma step is a no-op at the default gamma 2 (as on the CPU path).
    if (m.view.z != 1.0) {
        rgb = pow(abs(rgb), vec3(m.view.z));
    }
    rgb = clamp(rgb * m.view.w, vec3(0.0), vec3(1.0));
    return vec4(srgb_to_linear(rgb), 1.0);
}

// Near SpeedTree trees: bark, fronds and leaf cards (NapoleonRust, our own shader; see speedtree.rs).
//
// One material kind per pass (params.misc.w): 0 bark, 1 frond, 2 leaf card.
// - Leaf cards are expanded here to face the camera (like the original's leaf card shader, which
//   turns each card by the camera azimuth and pitch); their lighting normal is the direction from
//   the tree centre with the height divided by 5 (as the original).
// - LOD: geometry is shown up to the tree far distance and fizzles out over the last part of the
//   near→far range by raising the alpha-test reference (the original's SpeedTree alpha fizzle);
//   bark has no alpha, so it dithers. The billboards fade in over the same band (trees.wgsl).
//   A shrub's material is given near == far (speedtree.rs), which collapses that band to the last
//   metre: a shrub has no billboard to hand over to, so it must not fizzle.
// PROVISIONAL: the fade band width, the bark/frond lighting terms.

#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_functions::get_world_from_local

struct TreeGeomParams {
    sun_dir: vec4<f32>,
    sun_colour: vec4<f32>,
    ambient: vec4<f32>,
    // x: alpha-test reference, y: near distance, z: far distance, w: kind
    misc: vec4<f32>,
    // the 6 SpeedWind matrices, 3 rows each, tree-local Bevy axes
    wind: array<vec4<f32>, 18>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: TreeGeomParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var smp: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // leaf cards: xy = card corner offset (right, up) in metres, z = dimming
    @location(3) extra: vec4<f32>,
    // two wind levels: matrix group + weight
    @location(4) wind: vec2<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
    // x: fade (1 shown .. 0 gone), y: dimming
    @location(2) info: vec2<f32>,
}

/// Fraction of the near→far range over which the geometry fizzles out (PROVISIONAL).
const FADE_BAND: f32 = 0.1;

@vertex
fn vertex(v: Vertex) -> VertexOut {
    let m = get_world_from_local(v.instance_index);
    let origin = m[3].xyz;
    let scale = length(m[0].xyz);
    // Wind (the original's WindEffect): blend toward the level-1 matrix, then the level-2 one,
    // fading out with the distance to the camera over 200 m (WindFade). The matrix group is offset
    // per tree (PROVISIONAL: from its position; the original passes a per-instance offset).
    let off = u32(abs(origin.x * 0.37 + origin.z * 0.61));
    let wa = fract(v.wind.x);
    let wb = fract(v.wind.y);
    let ia = (u32(v.wind.x) + off) % 6u;
    let ib = (u32(v.wind.y) + off) % 6u;
    var p = v.position;
    p = mix(p, wind_apply(ia, p), wa);
    p = mix(p, wind_apply(ib, p), wb);
    let lerp_value = clamp(length((m * vec4<f32>(p, 1.0)).xyz - view.world_position) / 200.0, 0.0, 1.0);
    p = mix(p, v.position, lerp_value);
    var world = (m * vec4<f32>(p, 1.0)).xyz;
    var n = (m * vec4<f32>(v.normal, 0.0)).xyz;
    var dim = 1.0;
    if params.misc.w > 1.5 {
        let right = normalize(view.world_from_view[0].xyz);
        let up = normalize(view.world_from_view[1].xyz);
        world = world + (right * v.extra.x + up * v.extra.y) * scale;
        var lp = v.position;
        lp.y = lp.y / 5.0;
        n = (m * vec4<f32>(lp, 0.0)).xyz;
        dim = v.extra.z;
    }
    let d = distance(view.world_position, origin);
    let near = params.misc.y;
    let far = params.misc.z;
    let band = max((far - near) * FADE_BAND, 1.0);
    var out: VertexOut;
    out.clip = view.clip_from_world * vec4<f32>(world, 1.0);
    out.uv = v.uv;
    out.normal = n;
    out.info = vec2<f32>(clamp((far - d) / band, 0.0, 1.0), dim);
    return out;
}

fn dither(p: vec2<f32>) -> f32 {
    return fract(sin(dot(floor(p), vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, smp, in.uv);
    let kind = params.misc.w;
    let fade = in.info.x;
    if kind < 0.5 {
        if dither(in.clip.xy) >= fade {
            discard;
        }
    } else {
        // raise the alpha reference toward 1 as the tree fades (alpha fizzle)
        let reference = mix(1.01, params.misc.x, fade);
        if c.a < reference {
            discard;
        }
    }
    var n = normalize(in.normal);
    let l = normalize(params.sun_dir.xyz);
    var diffuse: f32;
    if kind < 0.5 {
        diffuse = max(dot(n, l), 0.0);
    } else if kind < 1.5 {
        diffuse = abs(dot(n, l)) * 0.5 + 0.25;
    } else {
        diffuse = in.info.y * (dot(l, n) + 1.0) * 0.5;
    }
    let light = params.ambient.rgb + params.sun_colour.rgb * diffuse;
    return vec4<f32>(c.rgb * light, 1.0);
}

fn wind_apply(i: u32, p: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        dot(params.wind[3u * i].xyz, p),
        dot(params.wind[3u * i + 1u].xyz, p),
        dot(params.wind[3u * i + 2u].xyz, p),
    );
}

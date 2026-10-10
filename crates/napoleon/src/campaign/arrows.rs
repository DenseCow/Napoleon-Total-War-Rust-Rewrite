//! The campaign map's movement arrow: the original's textured arrows laid along the ordered path,
//! in place of the plain gizmo line `play::preview` used to draw.
//!
//! The original's own assets are used, not stand-ins (CONFIRMED, `analysis/fidelity/UI_FIDELITY.md`
//! §7):
//! - the **texture** is `rigidmodels\campaignpieces\textures\arrow.dds` — one white arrow with an
//!   alpha channel, 512x256 (2:1), its head at the *left* of the picture. The exe's path prefix is
//!   the string `RigidModels/CampaignPieces/Textures/arrow`, referenced twice from `FUN_00986430`
//!   (`0x00986562` and `0x0098656f`), the function that builds the arrow pool;
//! - the **mesh** is the campaign map's own `campaign_maps\<map>\display\arrows\arrows.rigid_model`
//!   — one flat mesh lying in the XZ plane (vertex y 0.0032..0.0049 display units), 12.157 x 7.336
//!   display units, 432 vertices in 576 triangles, subdivided along its length so it can be bent
//!   round a path, and taking a 0.4549 x 1.0 slice of a 2:1 texture (its own diffuse slot,
//!   `transitmarkers_diffuse`, is a placeholder). Our strip is generated from the same texture and
//!   the same path rule with the mesh's proportions, rather than bending the mesh itself: the
//!   mapping the original uses to bend it is UNKNOWN.
//!
//! HOW the original lays the arrows out, attributed to `FUN_009CC5F0` — **INFERRED**: the sandbox
//! read it in Ghidra but kept no decompile (the only trace on record is a few stack-access lines and
//! one call site), so every point below needs the decompile redone in Ghidra to be CONFIRMED:
//! - the path's points are walked and one is taken every `spacing` units of walking distance,
//!   interpolated linearly inside its segment at `(spacing − walked) / segment length`;
//! - the sampled points go into **two buckets** (the loop's bound is the float value of `2`): the
//!   first from the path's start, the second from the remainder. The two buckets are the two
//!   colours the original draws; INFERRED: they are the part of the path the character can still
//!   reach this turn and the part beyond it;
//! - each bucket is smoothed before it is drawn: the first and last legs give up their points at
//!   1/3 and 2/3 along, and every interior point `p[i]` becomes the midpoint of itself and its
//!   predecessor `lerp(p[i-1], p[i], 0.5)`, so the chain bends round a corner instead of kinking at
//!   it. The original then sweeps a strip through those points with `FUN_005AFED0`, the same spline
//!   builder the campaign's borders, roads and rivers use (`CAMPAIGN_MAP.md` §10.3);
//! - the pool holds **five chains per bucket**; a bucket with fewer than two points draws nothing,
//!   and a path with fewer than two points draws nothing at all.
//!
//! PROVISIONAL: the **spacing** and the two **colours** are arguments of the virtual setter
//! `FUN_00A27C30(this, colour, flag, path, spacing)`, which Ghidra resolves to no direct call and no
//! vtable entry, so their values could not be read (§7 "Open"). The strip's width follows from the
//! original mesh's proportions (12.157 x 7.336 display units, so 0.60 of its length).

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use ntw_campaign::map_display::MapDisplay;
use ntw_formats::campaign_map::GameFiles;

use crate::GameMode;

/// The arrow texture in the install (the exe's own path prefix,
/// `RigidModels/CampaignPieces/Textures/arrow`, plus the extension the loader appends).
pub const ARROW_TEXTURE: &str = "rigidmodels\\campaignpieces\\textures\\arrow.dds";

/// Distance between two arrow heads along the path, in logic map units — one arrow's length.
/// PROVISIONAL: the original passes this in and its caller is unresolved (module comment).
pub const SPACING: f32 = 12.0;
/// How wide an arrow is, as a fraction of [`SPACING`], when the map's arrow mesh cannot be read.
/// The mesh's own proportion is 0.603 (12.157 x 7.336), and [`ArrowAssets::width`] prefers that.
pub const ARROW_WIDTH: f32 = 0.6;
/// The part of the path the character can still reach this turn. PROVISIONAL colour.
pub const REACHABLE_COLOUR: [f32; 4] = [0.94, 0.96, 0.90, 0.95];
/// The part beyond the character's movement for this turn. PROVISIONAL colour.
pub const UNREACHABLE_COLOUR: [f32; 4] = [0.95, 0.42, 0.30, 0.85];
/// How far the arrows float above the ground, so they do not z-fight with the terrain. PLACEHOLDER
/// (ours, like `scene::LINE_LIFT`; the original's draw offset is UNKNOWN).
pub const ARROW_LIFT: f32 = 0.12;
/// The pool's chains per bucket. INFERRED: `FUN_00986430` and `FUN_009CC5F0` were noted as looping five
/// times, but no decompile of either was kept; redo in Ghidra to confirm.
pub const CHAINS_PER_BUCKET: usize = 5;

/// The path the movement arrow is drawn along, published by `play::preview` when it plans one.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct ArrowPath {
    /// `CampaignSim::generation` the path was planned at, so a redraw happens when it changes.
    pub generation: u64,
    /// The ordered path's logic map (x, z) points.
    pub points: Vec<(f32, f32)>,
    /// How many of the path's points the character can still reach this turn; the arrow changes
    /// colour there.
    pub reachable: usize,
}

/// One drawn arrow chain. The pool is [`CHAINS_PER_BUCKET`] per bucket, so at most
/// `2 * CHAINS_PER_BUCKET` of these exist and the spare ones are hidden.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArrowChain;

/// The two arrow materials, built once from the install's arrow texture.
#[derive(Resource, Debug, Clone)]
pub struct ArrowAssets {
    /// The part of the path the character can still reach.
    pub reachable: Handle<StandardMaterial>,
    /// The part beyond its movement for this turn.
    pub unreachable: Handle<StandardMaterial>,
    /// The arrow mesh's bounding box in the units it is authored in (display units, as read from the
    /// install's `display\arrows\arrows.rigid_model`): (length, width).
    ///
    /// The mesh is a **template**, not a placed model, so its absolute size means nothing — the
    /// original's placement code supplies the scale, and that scale is UNKNOWN. Only the
    /// proportion is used, and it is scale-invariant ([`ArrowAssets::width`]).
    pub mesh_size: (f32, f32),
}

impl ArrowAssets {
    /// How wide one arrow is, in logic units: [`SPACING`] x the install mesh's own width-to-length
    /// proportion (0.603 for 12.157 x 7.336 display units), or [`ARROW_WIDTH`] when the map's model
    /// could not be read.
    pub fn width(&self) -> f32 {
        let (length, width) = self.mesh_size;
        let ratio = if length > 0.0 && width > 0.0 { width / length } else { ARROW_WIDTH };
        SPACING * ratio.clamp(0.05, 1.0)
    }
}

impl ArrowAssets {
    /// The material of a bucket: `false` for the part beyond the movement.
    pub fn material(&self, reachable: bool) -> Handle<StandardMaterial> {
        if reachable { self.reachable.clone() } else { self.unreachable.clone() }
    }
}

/// `OnEnter(GameMode::Campaign)`: reads the install's arrow texture and the map's arrow mesh, builds
/// the two materials and lays out the chain pool. `NAPOLEON_CAMPAIGN_ARROWS=0` leaves them out (for
/// comparisons with the gizmo line).
pub fn load(
    commands: &mut Commands,
    files: &GameFiles<'_>,
    map: &MapDisplay,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    if std::env::var("NAPOLEON_CAMPAIGN_ARROWS").is_ok_and(|v| v == "0") {
        return;
    }
    let bytes = match files.read(ARROW_TEXTURE) {
        Ok(b) => b,
        Err(e) => {
            warn!("Campaign arrows: {ARROW_TEXTURE}: {e}");
            return;
        }
    };
    let image = match arrow_image(&bytes) {
        Ok(i) => i,
        Err(e) => {
            warn!("Campaign arrows: {ARROW_TEXTURE}: {e}");
            return;
        }
    };
    let (w, h) = (image.width(), image.height());
    let texture = images.add(image);
    // The map's own arrow mesh, read for the proportion its width comes from (see `ArrowAssets`).
    let mesh_path = format!("{}'s arrow model", map.key);
    let mesh_size = match &map.arrow_model {
        Ok(b) => match ntw_formats::rigid_model::RigidModel::read(b) {
            Ok(m) => {
                let (length, width) = (m.bbox_max[0] - m.bbox_min[0], m.bbox_max[2] - m.bbox_min[2]);
                info!("Campaign arrows: the map's {mesh_path} is {length:.3} x {width:.3} in its own units ({} bytes)", b.len());
                (length, width)
            }
            Err(e) => {
                warn!("Campaign arrows: {mesh_path}: {e:?}");
                (0.0, 0.0)
            }
        },
        Err(e) => {
            warn!("Campaign arrows: {mesh_path}: {e}");
            (0.0, 0.0)
        }
    };
    let asset = ArrowAssets {
        reachable: arrow_material(materials, texture.clone(), REACHABLE_COLOUR),
        unreachable: arrow_material(materials, texture, UNREACHABLE_COLOUR),
        mesh_size,
    };
    // The pool: five chains per bucket, as the original's, all hidden until a path is planned.
    // Each is spawned with the components `draw` queries, so the query matches from the start.
    for i in 0..2 * CHAINS_PER_BUCKET {
        commands.spawn((
            Name::new(format!("campaign arrow chain {i}")),
            Mesh3d::default(),
            MeshMaterial3d::<StandardMaterial>::default(),
            Transform::default(),
            Visibility::Hidden,
            ArrowChain,
            DespawnOnExit(GameMode::Campaign),
        ));
    }
    info!(
        "Campaign arrows: {ARROW_TEXTURE} ({w}x{h}), one arrow every {SPACING} map units, {:.3} wide, \
         two colours (spacing and colours PROVISIONAL)",
        asset.width()
    );
    commands.insert_resource(asset);
}

/// The arrow texture: RGBA8 with every mip, repeating along the path (one arrow per [`SPACING`]).
fn arrow_image(bytes: &[u8]) -> Result<Image, String> {
    let dds = ntw_formats::dds::Dds::parse(bytes).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    for level in 0..dds.mip_count {
        data.extend(dds.decode_rgba8(level));
    }
    let size = Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, dds.decode_rgba8(0), TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = dds.mip_count;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        // Repeat along the path: one arrow every `spacing` (see [`strip`]).
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    Ok(image)
}

/// One arrow material: the install's texture in the bucket's colour, unlit, alpha blended and drawn
/// from both sides (the original's shaders use `CULLMODE = NONE` for their flat map overlays).
fn arrow_material(materials: &mut Assets<StandardMaterial>, texture: Handle<Image>, colour: [f32; 4]) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        base_color: Color::srgba(colour[0], colour[1], colour[2], colour[3]),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    })
}

/// Distance between two map points.
pub fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// `a` moved `t` of the way to `b`.
fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// The points an arrow is drawn at: one every [`SPACING`] units of walking distance along `path`,
/// each interpolated linearly inside its segment (INFERRED from the sandbox's notes on
/// `FUN_009CC5F0`, decompile not kept, needs redoing in Ghidra: the walked distance is compared
/// against the spacing and the point placed at `(spacing − walked) / length` of the segment). The spacing carries across segments, so a bend does not restart it; the path's start
/// and end are always kept, so the first arrow sits on the character and the last reaches the
/// destination. A spacing of zero or less keeps every point — what the original's other entry point
/// (`FUN_00A27CB0`) passes.
pub fn sample_path(path: &[(f32, f32)], spacing: f32) -> Vec<(f32, f32)> {
    sample_path_along(path, spacing).into_iter().map(|(p, _)| p).collect()
}

/// [`sample_path`], with each sampled point's walking distance from the path's start.
fn sample_path_along(path: &[(f32, f32)], spacing: f32) -> Vec<((f32, f32), f32)> {
    if path.len() < 2 {
        return path.iter().map(|&p| (p, 0.0)).collect();
    }
    let mut out: Vec<((f32, f32), f32)> = vec![(path[0], 0.0)];
    let mut push = |p: (f32, f32), along: f32| {
        if out.last().is_none_or(|(l, _)| dist(*l, p) > 1e-4) {
            out.push((p, along));
        }
    };
    let mut walked = 0.0f32;
    if spacing <= 0.0 {
        for w in path.windows(2) {
            walked += dist(w[0], w[1]);
            push(w[1], walked);
        }
        return out;
    }
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        if len <= f32::EPSILON {
            continue;
        }
        // The first multiple of `spacing` still ahead of us, measured from the path's start.
        let mut s = spacing - walked.rem_euclid(spacing);
        if s <= 1e-4 {
            s += spacing;
        }
        while s <= len {
            push(lerp(a, b, s / len), walked + s);
            s += spacing;
        }
        walked += len;
    }
    push(*path.last().unwrap(), walked);
    out
}

/// How many of the [`sample_path`] points lie within the reachable part of `path`, where
/// `reachable` is the index of the last path point the character can reach this turn (the model's
/// `PathPlan::reachable`). The split is by walking distance, since the sampled points are not the
/// path's points. INFERRED, like the reachable/beyond reading of the two buckets.
pub fn reachable_samples(path: &[(f32, f32)], reachable: usize, spacing: f32) -> usize {
    if path.is_empty() {
        return 0;
    }
    let end = reachable.min(path.len() - 1);
    let reach: f32 = path[..=end].windows(2).map(|w| dist(w[0], w[1])).sum();
    sample_path_along(path, spacing).iter().filter(|(_, along)| *along <= reach + 1e-3).count()
}

/// One bucket's smoothed centre-line, the way the original smooths it (INFERRED from the sandbox's
/// notes on `FUN_009CC5F0` in `UI_FIDELITY.md` §7; the decompile was not kept and needs redoing in
/// Ghidra to confirm): every
/// interior point `p[i]` becomes the midpoint `m[i]` of itself and its predecessor, and the two end
/// legs (`p[0]` to `m[1]`, `m[n-1]` to `p[n-1]`) add their points at 1/3 and 2/3 along. Two
/// points give the straight segment's thirds. `None` when there is nothing to draw (fewer than two
/// sampled points, as the original's pool does).
pub fn smooth_chain(samples: &[(f32, f32)]) -> Option<Vec<(f32, f32)>> {
    match samples.len() {
        0 | 1 => None,
        2 => {
            let (a, b) = (samples[0], samples[1]);
            Some(vec![a, lerp(a, b, 1.0 / 3.0), lerp(a, b, 2.0 / 3.0), b])
        }
        n => {
            let first = lerp(samples[0], samples[1], 0.5);
            let last = lerp(samples[n - 2], samples[n - 1], 0.5);
            let mut out = vec![samples[0], lerp(samples[0], first, 1.0 / 3.0), lerp(samples[0], first, 2.0 / 3.0)];
            for w in samples.windows(2).take(n - 1) {
                out.push(lerp(w[0], w[1], 0.5));
            }
            let end = samples[n - 1];
            out.push(lerp(last, end, 1.0 / 3.0));
            out.push(lerp(last, end, 2.0 / 3.0));
            out.push(end);
            Some(out)
        }
    }
}

/// The two buckets the original splits a path into (INFERRED: the sandbox noted that `FUN_009CC5F0`
/// builds two lists, the second starting where the first ended; no decompile kept, redo in Ghidra). `reachable` is the number of *sampled* points the
/// character can still reach this turn ([`reachable_samples`]); splitting there is INFERRED (the
/// original is told the split by the same caller that sets the colour). A first bucket too short to
/// draw takes the whole path.
pub fn buckets(samples: &[(f32, f32)], reachable: usize) -> [(&[(f32, f32)], bool); 2] {
    let split = reachable.clamp(0, samples.len());
    let (head, tail) = samples.split_at(split);
    if head.len() < 2 { [(samples, true), (&[], false)] } else { [(head, true), (tail, false)] }
}

/// One arrow chain's strip: two vertices per smoothed point, `width` across the path, at `lift`
/// above a draped ground. The texture's `u` runs backwards along the path from the destination, so
/// the arrow's head (the left of the picture, CONFIRMED) is at the far end and each arrow's tail
/// meets the next arrow's head: one arrow every `arrow_length` of path. Returns
/// `(positions, uvs, indices)` in a Bevy mesh's layout.
pub fn strip(chain: &[(f32, f32)], width: f32, arrow_length: f32, lift: f32, drape: &dyn Fn(f32, f32) -> f32) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>) {
    let n = chain.len();
    let total: f32 = chain.windows(2).map(|w| dist(w[0], w[1])).sum();
    let mut positions = Vec::with_capacity(n * 2);
    let mut uvs = Vec::with_capacity(n * 2);
    let mut walked = 0.0f32;
    for (i, &p) in chain.iter().enumerate() {
        if i > 0 {
            walked += dist(chain[i - 1], p);
        }
        // The direction of travel at p: back along the chain, else forward.
        let (dx, dz) = chain
            .get(i.wrapping_sub(1))
            .copied()
            .filter(|a| dist(*a, p) > 1e-6)
            .or_else(|| chain.get(i + 1).copied().filter(|b| dist(*b, p) > 1e-6))
            .map_or((1.0, 0.0), |other| {
                let (dx, dz) = (p.0 - other.0, p.1 - other.1);
                let l = (dx * dx + dz * dz).sqrt();
                (dx / l, dz / l)
            });
        // The left-hand normal in map (x, z).
        let (nx, nz) = (-dz * width * 0.5, dx * width * 0.5);
        let u = if arrow_length > 0.0 { (total - walked) / arrow_length } else { 0.0 };
        for (side, v) in [(1.0f32, 0.0f32), (-1.0f32, 1.0f32)] {
            let (x, z) = (p.0 + nx * side, p.1 + nz * side);
            positions.push([x, lift + drape(x, z), -z]);
            uvs.push([u, v]);
        }
    }
    let indices: Vec<u32> = (0..n.saturating_sub(1) as u32)
        .flat_map(|i| [2 * i, 2 * i + 3, 2 * i + 2, 2 * i, 2 * i + 2, 2 * i + 1])
        .collect();
    (positions, uvs, indices)
}

/// One chain's mesh, draped on the terrain. `None` when there is nothing to draw.
fn chain_mesh(map: &MapDisplay, chain: &[(f32, f32)], width: f32) -> Option<Mesh> {
    let drape = |x: f32, z: f32| map.height_at(x, z).max(0.0);
    let (positions, uvs, indices) = strip(chain, width, SPACING, ARROW_LIFT, &drape);
    if indices.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; uvs.len()]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

/// Draws the movement arrow for the path `play::preview` published: the path's points sampled and
/// smoothed the way the original does, as two chains (the part still reachable this turn and the
/// part beyond it). The pool of chain entities is refilled in place, never respawned.
pub fn draw(
    path: Option<Res<ArrowPath>>,
    ground: Option<Res<super::scene::CampaignGround>>,
    assets: Option<Res<ArrowAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chains: Query<(&mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, &mut Visibility), With<ArrowChain>>,
    mut shown: Local<bool>,
) {
    let (Some(ground), Some(assets)) = (ground, assets) else { return };
    // `play::preview` withdraws the path when nothing is selected or hovered: hide the pool once.
    let Some(path) = path else {
        if *shown {
            for (_, _, mut vis) in &mut chains {
                *vis = Visibility::Hidden;
            }
            *shown = false;
        }
        return;
    };
    // Redraw only when `preview` published a different path (it inserts only on a change).
    if *shown && !path.is_changed() && !assets.is_changed() {
        return;
    }
    *shown = true;
    let map = &ground.0;
    let width = assets.width();
    let samples = sample_path(&path.points, SPACING);
    let reachable = reachable_samples(&path.points, path.reachable, SPACING);
    let mut built: Vec<(Mesh, bool)> = Vec::new();
    for (bucket, reachable) in buckets(&samples, reachable) {
        let Some(chain) = smooth_chain(bucket) else { continue };
        if let Some(mesh) = chain_mesh(map, &chain, width) {
            built.push((mesh, reachable));
        }
    }
    if built.len() > 2 * CHAINS_PER_BUCKET {
        warn!("Campaign arrows: {} chains, the pool holds {}", built.len(), 2 * CHAINS_PER_BUCKET);
    }
    let mut slot = 0usize;
    for (mut mesh, mut mat, mut vis) in &mut chains {
        if slot < built.len() {
            *mesh = Mesh3d(meshes.add(built[slot].0.clone()));
            *mat = MeshMaterial3d(assets.material(built[slot].1));
            *vis = Visibility::Visible;
            slot += 1;
        } else {
            *vis = Visibility::Hidden;
        }
    }
    if path.points.is_empty() {
        return;
    }
    let bounds = samples.iter().fold(None, |m: Option<(f32, f32, f32, f32)>, p| {
        let (x, z) = *p;
        Some(match m {
            None => (x, z, x, z),
            Some((x0, z0, x1, z1)) => (x0.min(x), z0.min(z), x1.max(x), z1.max(z)),
        })
    });
    if let Some((x0, z0, x1, z1)) = bounds {
        info!(
            "Campaign arrows: {} path points -> {} sampled over {:.1} x {:.1} map units from ({:.1}, {:.1}), \
             {} chains ({} of {} reachable), one arrow {} long and {:.2} wide",
            path.points.len(),
            samples.len(),
            x1 - x0,
            z1 - z0,
            x0,
            z0,
            built.len(),
            reachable,
            samples.len(),
            SPACING,
            width
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points land one `spacing` apart along the path, as walking distance.
    #[test]
    fn samples_are_spacing_apart() {
        let s = sample_path(&[(0.0, 0.0), (100.0, 0.0)], 10.0);
        assert_eq!(s.first(), Some(&(0.0, 0.0)), "the start is kept");
        assert_eq!(s.last(), Some(&(100.0, 0.0)), "the destination is kept");
        for w in s.windows(2) {
            assert!((dist(w[0], w[1]) - 10.0).abs() < 1e-3, "{:?} to {:?} is not one spacing", w[0], w[1]);
        }
        assert_eq!(s.len(), 11, "100 units at 10 gives 11 points");
    }

    /// A path whose length is not a whole number of arrows keeps a short last gap.
    #[test]
    fn a_short_last_gap_is_kept() {
        let s = sample_path(&[(0.0, 0.0), (25.0, 0.0)], 10.0);
        assert_eq!(s, vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (25.0, 0.0)]);
    }

    /// The walked distance carries across segments, so a bend does not restart the spacing.
    #[test]
    fn spacing_carries_over_a_bend() {
        // Two legs of 6 and 8, so the path is 14 long: arrows at 0, 10 and 14. The one at 10 is
        // 4 up the second leg, so the spacing carried across the bend rather than restarting (a
        // per-segment spacing would have put one on each leg, 6 and 8 along).
        let s = sample_path(&[(0.0, 0.0), (6.0, 0.0), (6.0, 8.0)], 10.0);
        assert_eq!(s, vec![(0.0, 0.0), (6.0, 4.0), (6.0, 8.0)]);
        // The distance along the path from the start to the middle arrow is the spacing.
        let along = dist((0.0, 0.0), (6.0, 0.0)) + dist((6.0, 0.0), (6.0, 4.0));
        assert!((along - 10.0).abs() < 1e-4, "the middle arrow is 10 along the path: {along}");
        assert!((dist(s[1], s[2]) - 4.0).abs() < 1e-4, "and the last gap is the short one: {}", dist(s[1], s[2]));
    }

    /// More than one arrow fits inside a segment.
    #[test]
    fn several_arrows_fit_in_one_segment() {
        let s = sample_path(&[(0.0, 0.0), (55.0, 0.0)], 10.0);
        assert_eq!(s.len(), 7, "55 units at 10: 0, 10, 20, 30, 40, 50, 55");
    }

    /// A zero spacing is the original's other entry point: a point at every path point.
    #[test]
    fn a_zero_spacing_keeps_every_point() {
        let path = [(0.0, 0.0), (3.0, 4.0), (3.0, 9.0)];
        assert_eq!(sample_path(&path, 0.0), path);
    }

    /// Two points make a straight chain with the 1/3 and 2/3 samples the original takes.
    #[test]
    fn a_straight_chain_is_thirds() {
        let c = smooth_chain(&[(0.0, 0.0), (30.0, 0.0)]).expect("two points draw");
        assert_eq!(c.len(), 4);
        assert!((c[1].0 - 10.0).abs() < 1e-4 && (c[2].0 - 20.0).abs() < 1e-4, "{c:?}");
    }

    /// An interior point becomes the midpoint of itself and its predecessor, so the chain bends round
    /// the corner instead of kinking at it.
    #[test]
    fn a_corner_becomes_a_midpoint() {
        let c = smooth_chain(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]).expect("three points draw");
        // start, 1/3 and 2/3 of the first leg, the two midpoints, 1/3 and 2/3 of the last, the end.
        assert_eq!(c.len(), 8, "{c:?}");
        assert_eq!(c[3], (5.0, 0.0), "the midpoint of p[0] and p[1]");
        assert_eq!(c[4], (10.0, 5.0), "the midpoint of p[1] and p[2]");
        assert!((c[5].1 - 5.0 - 5.0 / 3.0).abs() < 1e-4 && (c[5].0 - 10.0).abs() < 1e-4, "1/3 along the last leg: {:?}", c[5]);
    }

    /// Fewer than two sampled points draws nothing, as the original's pool does.
    #[test]
    fn a_single_point_draws_nothing() {
        assert!(smooth_chain(&[(1.0, 1.0)]).is_none());
        assert!(smooth_chain(&[]).is_none());
    }

    /// The path is split once, into the reachable part and the rest, and an empty first bucket does
    /// not swallow the path.
    #[test]
    fn the_path_is_split_once() {
        let s = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)];
        let [(a, ra), (b, rb)] = buckets(&s, 2);
        assert!(ra && !rb);
        assert_eq!(a, &s[..2]);
        assert_eq!(b, &s[2..]);
        let [(a, _), (b, _)] = buckets(&s, 0);
        assert_eq!(a, &s[..], "nothing reachable: the whole path is still drawn");
        assert!(b.is_empty());
        let [_, (b, _)] = buckets(&s, 4);
        assert!(b.is_empty(), "everything reachable: no second chain");
    }

    /// The colour split is by walking distance: the model's `reachable` indexes the path's own
    /// points, which are not the sampled ones.
    #[test]
    fn the_reachable_split_follows_the_walked_distance() {
        // Path points at 0, 5, 25 and 40 along x; the character can reach the point at 25.
        let path = [(0.0, 0.0), (5.0, 0.0), (25.0, 0.0), (40.0, 0.0)];
        // Samples at 0, 10, 20, 30, 40: the first three are within 25.
        assert_eq!(sample_path(&path, 10.0).len(), 5);
        assert_eq!(reachable_samples(&path, 2, 10.0), 3);
        // Everything reachable, or only the start.
        assert_eq!(reachable_samples(&path, 3, 10.0), 5);
        assert_eq!(reachable_samples(&path, 0, 10.0), 1);
        // An index past the end is the whole path, and a zero spacing counts path points.
        assert_eq!(reachable_samples(&path, 99, 10.0), 5);
        assert_eq!(reachable_samples(&path, 2, 0.0), 3);
        assert_eq!(reachable_samples(&[], 0, 10.0), 0);
    }

    /// The strip's texture coordinate puts the arrow's head at the destination and repeats one arrow
    /// every `arrow_length` of path.
    #[test]
    fn the_head_is_at_the_destination() {
        let chain = smooth_chain(&[(0.0, 0.0), (30.0, 0.0)]).expect("chain");
        let (pos, uv, idx) = strip(&chain, 10.0, 10.0, 0.0, &|_, _| 0.0);
        assert_eq!(pos.len(), chain.len() * 2);
        // u falls from the start of the parameterisation to 0 at the destination, where the head is.
        let last = uv.len() - 2;
        assert!(uv[0][0] > uv[last][0], "u runs backwards along the path: {:?} then {:?}", uv[0], uv[last]);
        assert!(uv[last][0].abs() < 1e-4, "u = 0 at the destination: the head points there");
        assert!((uv[0][0] - 3.0).abs() < 1e-3, "30 units at one arrow per 10 is three arrows: {:?}", uv[0]);
        for t in &uv {
            assert!((0.0..=3.0).contains(&t[0]), "u stays in range: {t:?}");
            assert!(t[1] == 0.0 || t[1] == 1.0, "v is the texture's height: {t:?}");
        }
        assert_eq!(idx.len(), (chain.len() - 1) * 6, "two triangles per segment");
    }

    /// The width comes from the install mesh's own proportion, at whatever scale the arrows are
    /// drawn, and falls back to [`ARROW_WIDTH`] when the mesh is not there.
    #[test]
    fn the_width_follows_the_mesh_proportion() {
        let with_mesh = ArrowAssets {
            reachable: Handle::default(),
            unreachable: Handle::default(),
            mesh_size: (12.157, 7.336),
        };
        assert!((with_mesh.width() - SPACING * 0.603).abs() < 0.01, "{}", with_mesh.width());
        let without = ArrowAssets { mesh_size: (0.0, 0.0), ..with_mesh.clone() };
        assert!((without.width() - SPACING * ARROW_WIDTH).abs() < 1e-4, "{}", without.width());
        // A mesh with no size does not make the strip vanish or swallow the map.
        assert!(without.width() > 0.0 && without.width() < SPACING);
    }

    /// The strip is `width` across the path, and its vertices sit at the chain's points.
    #[test]
    fn the_strip_is_the_arrow_width() {
        let chain = [(0.0, 0.0), (30.0, 0.0)];
        let (pos, _, _) = strip(&chain, 12.0, 12.0, 0.0, &|_, _| 0.0);
        let across = dist((pos[0][0], -pos[0][2]), (pos[1][0], -pos[1][2]));
        assert!((across - 12.0).abs() < 1e-3, "the strip is {across} across, not 12");
        // The centre of the first pair is the chain's first point.
        let mid = ((pos[0][0] + pos[1][0]) / 2.0, -(pos[0][2] + pos[1][2]) / 2.0);
        assert!(dist(mid, chain[0]) < 1e-4, "{mid:?}");
    }

    /// The strip is lifted and draped onto the ground.
    #[test]
    fn the_strip_lies_on_the_ground() {
        let chain = [(0.0, 0.0), (30.0, 0.0)];
        let (pos, _, _) = strip(&chain, 12.0, 12.0, 0.12, &|_, _| 1.5);
        assert!(pos.iter().all(|v| (v[1] - 1.62).abs() < 1e-5), "lift plus the drape: {pos:?}");
    }

    /// The real install: the arrow texture is there and is a 2:1 arrow with its head at the left, and
    /// the map's arrow model is a flat strip of the proportions the width comes from.
    #[test]
    fn the_install_has_the_arrow_assets() {
        let dir = crate::config::game_data_dir();
        let Ok(vfs) = ntw_formats::pack::Vfs::open_install(&dir) else { return };
        let files = GameFiles { vfs: &vfs };
        let bytes = files.read(ARROW_TEXTURE).expect("the campaign arrow texture");
        let img = arrow_image(&bytes).expect("arrow.dds decodes");
        assert_eq!((img.width(), img.height()), (512, 256), "arrow.dds is one 2:1 arrow");
        let rgba = img.data.clone().expect("the mip chain");
        let alpha = |x: u32, y: u32| rgba[((y * 512 + x) * 4 + 3) as usize];
        // The arrow's opaque height per column, measured: a point at mid-height on the left, the head
        // flaring to 162 of 256 rows by x = 80, a constant 64-row shaft to x = 288, then a tail
        // tapering back to a point at the right. So the head is at the LEFT — the arrow points along
        // -u, which is what [`strip`] assumes when it runs u backwards from the destination.
        let rows = |x: u32| (0..256).filter(|&y| alpha(x, y) > 24).count();
        assert!(rows(0) <= 4, "the tip is a point at the left edge: {} rows", rows(0));
        assert!(rows(80) > 150, "the head is at its widest by x = 80: {} rows", rows(80));
        assert_eq!(rows(200), 64, "the shaft is a constant 64 rows: {}", rows(200));
        assert!(rows(496) <= 12, "and the tail tapers to a point at the right: {} rows", rows(496));
        // The head is about 2.5x the shaft's height, so the strip's width should cover the head.
        assert!((rows(80) as f32 / rows(200) as f32 - 2.53).abs() < 0.05);
        let Ok(b) = files.read("campaign_maps/nap_europe/display/arrows/arrows.rigid_model") else { return };
        let m = ntw_formats::rigid_model::RigidModel::read(&b).expect("arrows.rigid_model");
        assert_eq!(m.meshes.len(), 1, "one mesh");
        let hi = m.meshes[0].vertices.iter().fold(f32::MIN, |a, v| a.max(v.position[1]));
        assert!(hi < 0.01, "the arrow lies flat on the ground (y up to {hi})");
        let (w, d) = (m.bbox_max[0] - m.bbox_min[0], m.bbox_max[2] - m.bbox_min[2]);
        assert!((w - 12.157).abs() < 0.01 && (d - 7.336).abs() < 0.01, "12.157 x 7.336 in its own units: {w} x {d}");
        // The width follows from the mesh's own proportion, whatever scale it is placed at.
        assert!((d / w - 0.603).abs() < 0.002, "the mesh's proportion: {}", d / w);
        assert!((d / w - ARROW_WIDTH).abs() < 0.005, "which is the fallback constant");
    }
}

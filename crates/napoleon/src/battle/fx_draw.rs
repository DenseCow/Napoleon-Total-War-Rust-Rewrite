//! Drawing the particle world: quads with the original's own effect textures, turned the way each
//! emitter's shipped `sprite_facing_mode` says — camera-facing, or upright about the vertical.
//!
//! One mesh entity per **(effect texture, render method)** pair. Every frame each live particle
//! becomes a camera-facing quad coloured by its emitter's colour ramp and faded by
//! [`super::fx::Particle::opacity`], and the meshes are rebuilt. That keeps a massed volley in a
//! handful of draw calls instead of one entity per puff.
//!
//! The `RENDER_METHOD_*` names in `landbattle.xml` map onto blend modes like this:
//!
//! | file | ours | state |
//! |---|---|---|
//! | `RENDER_METHOD_ALPHA` | `AlphaMode::Blend` | smoke, dust |
//! | `RENDER_METHOD_ADDITIVE` | `AlphaMode::Add` | muzzle flashes, glows, sparks |
//! | `RENDER_METHOD_DISTORTION` | `AlphaMode::Blend` | **PROVISIONAL**: the original's
//!   `particle_distortion.fx` bends the scene through a normal map; we draw it flat until the
//!   distortion pass exists (`BATTLE_EFFECTS.md` §2) |
//! | `RENDER_METHOD_OPAQUE` | `AlphaMode::Blend` | no shipped row uses it (clean negative) |
//!
//! The shader itself is `particle.wgsl`, ours, written from what the shipped `fx\particle.fx`
//! does; `analysis/graphics/SHADERS.md` §1 has the survey.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

use ntw_formats::effects::{DustParameters, Effect, EffectLibrary, RenderMethod};

use super::fx::{self, FxWorld, Particle};
use super::view::world_of;
use super::{BattleSim, VolleyFx};
use crate::data::GameData;

/// The groups this slot plays. Every name is a real group in `landbattle.xml` (CONFIRMED by
/// `effects_install::land_battle_effects_parse`); the list is deliberately small so we only
/// upload the textures those groups need — the other groups wait for their own backlog item.
///
/// **CONFIRMED as the complete land list**: these are the eleven `fire_effect` values
/// `db\projectiles` column 31 takes over the 144 shipped rows (`CannonFire` 56,
/// `LandGunFire_canister` 13, `LandGunFire_large` 9, `MusketFire` 8, `LandGunFire` 7,
/// `LandGunFire_howitzer` 7, `LandGunFire_mortar` 8, `LandGunFire_small` 7, `fougasse_default` 2,
/// `rifleFire` 2, `pistolFire` 1) — see `ntw_data/tests/effects_data.rs`. The twelfth value,
/// `ship_explosion`, is naval and left out. `LandGunFire_congreve` was in the round-1 list on the
/// strength of its name; no `projectiles` row uses it, so it is gone.
pub const FIRE_GROUPS: &[&str] = &[
    "MusketFire",
    "rifleFire",
    "pistolFire",
    "LandGunFire",
    "LandGunFire_small",
    "LandGunFire_large",
    "LandGunFire_howitzer",
    "LandGunFire_mortar",
    "LandGunFire_canister",
    "fougasse_default",
    "CannonFire",
];
/// The dust groups this slot plays (CONFIRMED group names in `landbattle.xml`).
pub const DUST_GROUPS: &[&str] = &["infantry_walk_dust", "cavalry_walk_dust", "infantry_combat_dust"];
/// The impact groups this slot plays where a shot lands (CONFIRMED group names in
/// `landbattle.xml`: `effects_install::projectile_effect_names_are_land_battle_groups` and
/// `effects_data::explosion_rows_name_the_effect_groups_they_play` check them against the shipped
/// `projectiles_explosions` / `projectile_impacts` bytes).
///
/// The first five are what the shot's own `projectiles_explosions` row names (read by
/// [`fx::impact::air_burst`] / [`fx::impact::ground_scorch`]); the last two are the PROVISIONAL
/// calibre-sized fallback for a row with no ground scorch of its own.
pub const IMPACT_GROUPS: &[&str] = &[
    "blood_gen",
    "AirExplosion_sml",
    "AirExplosion_med",
    "AirExplosion_lrg",
    "Cannon_Groundimpact_explosive",
    "Cannon_Groundimpact_gen_sml",
    "Cannon_Groundimpact_gen_med",
];

/// The battle's effect data, read once from the install: `effects\landbattle.xml` and the
/// per-entity dust table. Split from the live particles so a system can hold the data immutably
/// while it releases particles.
#[derive(Resource)]
pub struct BattleFx {
    /// The parsed `effects\landbattle.xml`.
    pub lib: EffectLibrary,
    /// The parsed `effects\unit_dust_parameters.txt`.
    pub dust: DustParameters,
}

/// The live particle world and the per-unit dust timers.
#[derive(Resource)]
pub struct FxWorldRes {
    /// The particles.
    pub world: FxWorld,
    /// Per-unit seconds of battle time since that unit last puffed dust.
    pub dust_timers: HashMap<u32, f32>,
    /// Battle time at the last step, so the world advances once per model tick.
    last: Option<f32>,
}

impl FxWorldRes {
    /// A world whose generator starts from the battle's seed.
    pub fn new(seed: u32) -> Self {
        Self { world: FxWorld::new(seed), dust_timers: HashMap::new(), last: None }
    }

    /// The number of live particles (the log and the tests read this).
    pub fn live(&self) -> usize {
        self.world.particles.len()
    }
}

/// One draw bucket: an effect texture and how the effect blends.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BucketKey {
    /// The emitter's `texture_1`, as a pack path.
    pub texture: String,
    /// The emitter's `render_method`.
    pub method: RenderMethod,
}

/// One particle material per bucket: the emitter's diffuse texture and its blend mode.
///
/// The light the particles take is a uniform, not a Bevy light, because the original multiplies
/// each particle by the scene's own lighting (`lighting` x `light_colour`) rather than running
/// the standard PBR path (`analysis/graphics/SHADERS.md` §2).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ParticleMaterial {
    #[uniform(0)]
    params: FxParams,
    #[texture(1)]
    #[sampler(2)]
    albedo: Handle<Image>,
    /// True for `RENDER_METHOD_ADDITIVE` (see [`Material::alpha_mode`]).
    additive: bool,
}

#[derive(ShaderType, Debug, Clone, Copy)]
struct FxParams {
    /// rgb: the scene light the particles take. a: 1.0 for an additive bucket, 0.0 otherwise —
    /// `particle.wgsl` premultiplies an additive particle by its alpha, because Bevy's
    /// `AlphaMode::Add` is a premultiplied blend state (`src + (1 - src_a) * dst`) that expects the
    /// shader to do it.
    light: Vec4,
}

impl Material for ParticleMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://napoleon/battle/particle.wgsl".into()
    }

    /// `RENDER_METHOD_ADDITIVE` is the original's additive pass (`explosion_particle.fx` /
    /// `particle2.fx` add their colour); everything else is ordinary alpha. See the module table.
    fn alpha_mode(&self) -> AlphaMode {
        if self.additive { AlphaMode::Add } else { AlphaMode::Blend }
    }
}

/// The particle textures and materials of one battle.
#[derive(Resource, Default)]
pub struct FxAssets {
    /// Bucket key -> material.
    pub materials: HashMap<BucketKey, Handle<ParticleMaterial>>,
    /// The buckets, in a fixed order, so the mesh entities are reused frame to frame.
    pub keys: Vec<BucketKey>,
    /// Every effect texture we uploaded, so [`leave`] can drop them out of `Assets<Image>`.
    pub textures: Vec<Handle<Image>>,
}

impl FxAssets {
    /// The material of a bucket, if its texture was found.
    pub fn material(&self, key: &BucketKey) -> Option<Handle<ParticleMaterial>> {
        self.materials.get(key).cloned()
    }
}

/// The mesh entity drawing one bucket.
#[derive(Component)]
pub struct FxBucket(pub BucketKey);

/// The buckets a world needs this frame, with their particles grouped, in the key's sorted order.
///
/// **Looked up by key, never by position in the list.** A `Query`'s iteration order is not part of
/// its contract, so matching bucket N of the query against bucket N of this map silently drew
/// nothing as soon as the order changed. (`buckets_group_by_texture_and_blend` only checked that
/// the *map* was sorted; it could not catch this.)
fn buckets<'a>(world: &'a FxWorld, lib: &'a EffectLibrary) -> BTreeMap<BucketKey, Vec<&'a Particle>> {
    let mut out: BTreeMap<BucketKey, Vec<&Particle>> = BTreeMap::new();
    for p in &world.particles {
        let Some(effect) = lib.effects.get(&p.effect) else { continue };
        let key = BucketKey { texture: effect.texture().to_owned(), method: p.render_method };
        out.entry(key).or_default().push(p);
    }
    out
}

/// The mesh for one bucket: one quad per particle, four vertices each.
///
/// The quad's corners are the particle's centre plus/minus half its size along the sprite's right
/// and up axes, rotated in plane by the particle's `rotation`. Which axes those are comes from the
/// emitter's `sprite_facing_mode`, CONFIRMED as four shipped names:
///
/// - `CAMERA_FACING` (401 of the 435 shipped emitters) and `BILLBOARD` (18) both point the quad's
///   normal at the camera. The two are drawn alike because **the shipped data and the shipped
///   shader leave no channel for a difference** — see [`FacingMode::is_y_axis`] and
///   `ntw_formats/tests/effects_install.rs::billboard_is_indistinguishable_from_camera_facing_in_the_shipped_shader`.
///   No basis-side attribute separates them, and `fx\particle_volumetric.fx_fragment` (which defines
///   the `particle_vertex_30` both `particle.fx` and `particle_distortion.fx` compile) is handed only
///   `g_camera_aligned_x/y/z_axis` and has **no facing parameter at all**, so the choice is the
///   exe's. What the original does differently stays UNKNOWN, with that named as the target.
/// - `LOCAL_Y_AXIS` (14) and `WORLD_Y_AXIS` (2) stand the quad **upright** and turn it about the
///   vertical only — a cylindrical billboard. These are the ground-impact distortion sheets, the
///   water ripples and `shockwave_large`. INFERRED from the mode names: the shipped shader is handed
///   one camera-aligned basis only, so how the exe builds an upright quad is UNKNOWN. (The earth and
///   debris sprites are `BILLBOARD`, drawn camera-facing as above, not upright.)
/// - `VELOCITY_FACING` is a **clean negative**: no shipped effect file writes it, so nothing draws
///   with it.
pub fn quad_mesh(particles: &[&Particle], eye: Vec3, cam_right: Vec3, cam_up: Vec3) -> Mesh {
    // The camera's own right and up axes: the shipped `particle_vertex_30` builds every quad from
    // `g_camera_aligned_x_axis * xy.x + g_camera_aligned_y_axis * xy.y` (CONFIRMED from the shipped
    // HLSL, `billboard_is_indistinguishable_from_camera_facing_in_the_shipped_shader`), i.e. one
    // screen-aligned basis for every quad. **Review fix:** this used to derive the basis from
    // `eye.normalize()` — the direction from the *world origin* to the camera — so every quad faced
    // the origin's view line rather than the screen, and was visibly skewed whenever the camera was
    // not looking at (0, 0, 0), which is almost always.
    let right = cam_right.normalize_or(Vec3::X);
    let up = cam_up.normalize_or(Vec3::Y);
    // The upright basis, for the `*_Y_AXIS` modes: the camera's right flattened onto the ground, with
    // world up as the quad's own up. A rolled-over camera whose right is vertical has no bearing, so it
    // falls back to the camera's own right.
    let flat_right = Vec3::new(right.x, 0.0, right.z);
    let y_right = if flat_right.length_squared() > 1e-6 { flat_right.normalize() } else { right };
    let y_up = Vec3::Y;
    let n = particles.len();
    let mut positions = Vec::with_capacity(n * 4);
    let mut normals = Vec::with_capacity(n * 4);
    let mut uvs = Vec::with_capacity(n * 4);
    let mut colors = Vec::with_capacity(n * 4);
    let mut indices = Vec::with_capacity(n * 6);
    for (i, p) in particles.iter().enumerate() {
        let [w, h] = p.size();
        let c = Vec3::new(p.position[0], p.position[1], p.position[2]);
        let (s, co) = p.rotation.sin_cos();
        // In-plane rotation, then the two half-axes. The `*_Y_AXIS` sprites keep the upright basis,
        // so they stand vertical and only their bearing turns with the camera.
        let (base_r, base_u) = if p.facing.is_y_axis() { (y_right, y_up) } else { (right, up) };
        let (rx, ry) = (base_r * co + base_u * s, base_u * co - base_r * s);
        let hw = rx * (w * 0.5);
        let hh = ry * (h * 0.5);
        let corners = [c - hw - hh, c + hw - hh, c + hw + hh, c - hw + hh];
        let (u0, v0, du, dv) = frame_uv(p);
        let uv = [[u0, v0], [u0 + du, v0], [u0 + du, v0 + dv], [u0, v0 + dv]];
        let alpha = p.opacity();
        let colour = p.colour();
        // The normal points from the quad at the camera, measured from the particle itself: with
        // one normal for a whole battlefield the quads far from the origin shaded as if they were
        // at it, which the previous `-normalize(eye)` did. An upright quad's normal is horizontal,
        // because its face is a vertical plane.
        let to_eye = (eye - c).normalize_or_zero();
        let normal = if p.facing.is_y_axis() {
            Vec3::new(to_eye.x, 0.0, to_eye.z).normalize_or_zero()
        } else {
            to_eye
        };
        for k in 0..4 {
            positions.push(corners[k].to_array());
            normals.push(normal.to_array());
            uvs.push(uv[k]);
            colors.push([colour[0], colour[1], colour[2], alpha]);
        }
        let b = (i * 4) as u32;
        indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// The UV rectangle of a particle's sprite-sheet frame.
///
/// `ANIMATION_INFO` gives `cell_width`, `cell_height` and `total_frames` (CONFIRMED names), so the
/// sheet is a grid of that many frames. **INFERRED**: the grid is the squarest one (columns =
/// `ceil(sqrt(frames))`) and the frames run left to right, top to bottom — the file does not say,
/// and nothing in it distinguishes that from a single row.
pub fn frame_uv(p: &Particle) -> (f32, f32, f32, f32) {
    let frames = p.frames.max(1);
    let cols = (frames as f32).sqrt().ceil().max(1.0) as u32;
    let rows = frames.div_ceil(cols).max(1);
    let index = if p.frames > 1 {
        p.frame % frames
    } else {
        0
    };
    let (c, r) = (index % cols, index / cols);
    let du = 1.0 / cols as f32;
    let dv = 1.0 / rows as f32;
    (c as f32 * du, r as f32 * dv, du, dv)
}

// -------------------------------------------------------------------------------------------
// Bevy systems
// -------------------------------------------------------------------------------------------

/// Reads the two effect files once, when the battle starts.
pub fn enter(mut commands: Commands, sim: Option<Res<BattleSim>>) {
    let seed = sim.as_ref().map_or(0, |s| s.seed);
    let vfs = match ntw_formats::pack::Vfs::open_install(crate::config::game_data_dir()) {
        Ok(v) => v,
        Err(e) => {
            warn!("Battle effects: cannot open the install: {e}");
            return;
        }
    };
    let lib = match EffectLibrary::from_vfs(&vfs) {
        Ok(l) => l,
        Err(e) => {
            warn!("Battle effects: effects\\landbattle.xml: {e}");
            return;
        }
    };
    let dust = EffectLibrary::dust_from_vfs(&vfs).unwrap_or_default();
    info!(
        "Battle effects: {} emitters, {} groups, {} dust frequencies, seed {seed}",
        lib.effects.len(),
        lib.groups.len(),
        dust.frequency.len()
    );
    commands.insert_resource(BattleFx { lib, dust });
    commands.insert_resource(FxWorldRes::new(seed));
}

/// Leaving the battle: the particles go with everything else, and so do the assets we uploaded.
///
/// Nothing references the meshes, the effect textures or the particle materials once the bucket
/// entities are despawned, so without this they stay in `Assets<…>` for the rest of the session.
pub fn leave(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ParticleMaterial>>,
    handles: Option<Res<FxMeshes>>,
    assets: Option<Res<FxAssets>>,
) {
    if let Some(handles) = handles {
        for h in handles.0.values() {
            meshes.remove(h);
        }
    }
    if let Some(assets) = assets {
        for h in assets.materials.values() {
            materials.remove(h);
        }
        for h in &assets.textures {
            images.remove(h);
        }
    }
    commands.remove_resource::<BattleFx>();
    commands.remove_resource::<FxWorldRes>();
    commands.remove_resource::<FxAssets>();
    commands.remove_resource::<FxMeshes>();
}

/// Uploads the effect textures of the groups this slot plays and builds one material per bucket.
pub fn load_assets(
    mut commands: Commands,
    fx: Option<Res<BattleFx>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ParticleMaterial>>,
) {
    let Some(fx) = fx.as_ref() else { return };
    let vfs = match ntw_formats::pack::Vfs::open_install(crate::config::game_data_dir()) {
        Ok(v) => v,
        Err(e) => {
            warn!("Battle effects: cannot open the install: {e}");
            return;
        }
    };
    // The scene light the particles take: the battle map's `.environment` sun plus a flat ambient.
    // PROVISIONAL: the original multiplies by its own scene lighting per particle (the emitter's
    // `lighting` weight x the light), and we do the same with one light for the whole battle.
    let light = scene_light();

    // Every emitter of every playable group gets its own bucket, and one texture per bucket: the
    // emitters of one group share a texture often but not always (`cannon_fire_anim_smoke` and
    // `landgun_med_2_whispy` do not), so we key on (texture, blend) and let `images` deduplicate
    // the identical handles.
    let wanted: BTreeSet<BucketKey> = FIRE_GROUPS
        .iter()
        .copied()
        .chain(IMPACT_GROUPS.iter().copied())
        .chain(DUST_GROUPS.iter().copied())
        .flat_map(|group| fx.lib.group_effects(group))
        .map(key_of)
        .collect();
    let keys: Vec<BucketKey> = wanted.into_iter().collect();
    let mut map: HashMap<BucketKey, Handle<ParticleMaterial>> = HashMap::new();
    // One image handle per distinct texture path, shared by the buckets that use it.
    let mut handles: HashMap<String, Handle<Image>> = HashMap::new();
    let mut missing: Vec<String> = Vec::new();
    for key in &keys {
        let handle = match handles.get(&key.texture) {
            Some(h) => h.clone(),
            None => match texture_image(&vfs, &key.texture) {
                Ok(image) => {
                    let h = images.add(image);
                    handles.insert(key.texture.clone(), h.clone());
                    h
                }
                Err(e) => {
                    missing.push(format!("{}: {e}", key.texture));
                    continue;
                }
            },
        };
        map.insert(
            key.clone(),
            materials.add(ParticleMaterial {
                params: FxParams {
                    light: Vec4::new(light.x, light.y, light.z, if matches!(key.method, RenderMethod::Additive) { 1.0 } else { 0.0 }),
                },
                albedo: handle,
                additive: matches!(key.method, RenderMethod::Additive),
            }),
        );
    }
    if !missing.is_empty() {
        warn!("Battle effects: {} textures missing: {}", missing.len(), missing.join(", "));
    }
    info!("Battle effects: {} buckets, {} distinct textures", map.len(), handles.len());
    let textures = handles.into_values().collect();
    commands.insert_resource(FxAssets { materials: map, keys, textures });
}

/// Spawns one mesh entity per bucket.
pub fn spawn_buckets(mut commands: Commands, assets: Res<FxAssets>) {
    for key in &assets.keys {
        let Some(material) = assets.material(key) else { continue };
        commands.spawn((
            FxBucket(key.clone()),
            Mesh3d::default(),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Hidden,
            DespawnOnExit(crate::GameMode::Battle),
        ));
    }
    info!("Battle effects: {} draw buckets", assets.keys.len());
}

/// Every model tick: release the effects the battle owes, then step the particles.
///
/// **Battle time, not frame time.** The model ticks at a fixed 0.1 s (CONFIRMED W1 §12.2) and so
/// do the particles, so the picture of a battle does not depend on the frame rate or the speed
/// setting.
pub fn tick_fx(
    data_fx: Option<Res<BattleFx>>,
    mut world: Option<ResMut<FxWorldRes>>,
    sim: Option<Res<BattleSim>>,
    volleys: Option<Res<VolleyFx>>,
    data: Option<Res<GameData>>,
    mut seen: Local<Option<(u32, usize)>>,
) {
    let (Some(fx), Some(world), Some(sim)) = (data_fx.as_ref(), world.as_mut(), sim.as_ref()) else { return };
    let now = sim.battle.time_seconds();
    let dt = match world.last {
        None => {
            world.last = Some(now);
            // A fresh world is a fresh battle. `seen` is a system `Local`, so it outlives the battle
            // and still holds the last battle's (tick, count); left alone it would skip this battle's
            // volleys until its tick counter passed the old one.
            *seen = None;
            return;
        }
        Some(t) if now > t => now - t,
        Some(_) => return,
    };
    world.last = Some(now);
    let world = &mut *world;

    // Muzzle flashes and smoke: the firing group of every new volley, at the firing unit's muzzle.
    if let (Some(volleys), Some(data)) = (volleys.as_ref(), data.as_ref()) {
        for v in new_volleys(volleys, &mut seen) {
            fire_muzzle(&mut world.world, &fx.lib, sim, &data.db, v);
        }
    }

    // Foot and wheel dust: one puff per unit per its `entity frequency` seconds. A unit counts as
    // artillery when its own `unit_stats_land` row names a gun type (`gun_type` col 24), which is
    // the same test `setup::missile_weapon` uses to find its guns.
    let artillery: Vec<u32> = data
        .as_ref()
        .map(|d| {
            sim.battle
                .units
                .iter()
                .enumerate()
                .filter(|(i, _)| {
                    sim.info
                        .get(*i)
                        .and_then(|info| d.db.land_unit(&info.key))
                        .is_some_and(|v| v.stats.gun_type.is_some())
                })
                .map(|(_, u)| u.id)
                .collect()
        })
        .unwrap_or_default();
    for u in &sim.battle.units {
        if !u.moved || u.men == 0 || u.left_field {
            continue;
        }
        let behaviour = behaviour_of(u);
        let Some(entity) = super::fx::dust_entity_name(u.is_cavalry, artillery.contains(&u.id), behaviour) else { continue };
        let ground = world_of(Vec2::new(u.position.0, u.position.1));
        let facing = [u.facing.cos(), 0.2, -u.facing.sin()];
        let FxWorldRes { world: particles, dust_timers, .. } = &mut **world;
        particles.dust(
            &fx.lib,
            &fx.dust,
            u.id,
            &entity,
            behaviour,
            [ground.x, ground.y + 0.15, ground.z],
            facing,
            dust_timers,
            dt,
        );
    }

    world.world.advance(dt);
}

/// How many live particles `NAPOLEON_FX_SHOT` waits for, so the shot catches a volley rather than
/// one stray dust puff.
const SHOT_MIN_PARTICLES: usize = 40;

/// How many seconds of real time `NAPOLEON_FX_SHOT` waits before it takes the picture, so the
/// window has been drawn at least once.
const SHOT_MIN_AGE: f32 = 3.0;

/// Test harness, with three independent switches:
///
/// - `NAPOLEON_FX_LOG` logs the particle count, how many have ever been released and how many the
///   cap dropped, every second of battle time, and quits once that many particles have been
///   released. Used to check the effect system against the original in
///   `analysis/graphics/BATTLE_EFFECTS.md`.
/// - `NAPOLEON_FX_SHOT=<file.png>` saves a screenshot at the first moment a volley is in the air
///   (at least [`SHOT_MIN_PARTICLES`] particles, some of them at least 0.25 s old, so the smoke
///   has risen but not gone) and quits 1.5 s later. It proves the effect is drawn where a real
///   volley put it, not at a scripted position. `NAPOLEON_FX_SHOT_AT=<battle seconds>` holds the
///   shot back until that point of the battle clock, which is what a check at deployment distance
///   needs: a volley there is a few pixels wide.
/// - `NAPOLEON_FX_CHECK=<substring>` prints the measured numbers for every queued check whose name
///   contains it (all of them for `all`), one `FX check ... PASS/FAIL` line each, and quits. This is
///   the diagnostic that answers a *question* instead of leaving it to a picture: see
///   [`crate::battle::fx_probe`]. It needs no camera and no battle, but it runs here so the same
///   switch works inside a live battle too.
#[allow(clippy::too_many_arguments, reason = "a Bevy system's parameters; one per resource plus the harness's own state")]
pub fn fx_log(
    time: Res<Time<Real>>,
    world: Option<Res<FxWorldRes>>,
    sim: Option<Res<BattleSim>>,
    drawn: Option<Res<DrawnFx>>,
    fx: Option<Res<BattleFx>>,
    data: Option<Res<GameData>>,
    mut commands: Commands,
    mut acc: Local<f32>,
    mut shot_at: Local<f32>,
    mut reported: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let check = std::env::var("NAPOLEON_FX_CHECK").ok().filter(|p| !p.is_empty());
    if let (Some(needle), Some(lib), Some(db), false) = (&check, fx.as_ref(), data.as_ref(), *reported)
        && !lib.lib.groups.is_empty()
    {
        // `all` (or an empty needle) prints the whole table through `report_everything`, which is the
        // same text the install test prints, so a run and a test cannot disagree about a verdict. A
        // name substring prints just those claims, each with the check line it stands for so the log is
        // self-explaining.
        if needle.is_empty() || needle.eq_ignore_ascii_case("all") {
            for line in super::fx_probe::report_everything(&lib.lib, &db.db).lines() {
                info!("{line}");
            }
        } else {
            for claim in super::fx_probe::claims_matching(needle) {
                match super::fx_probe::check(&lib.lib, claim) {
                    Some(v) => info!("{v}\n  asked: {}", v.question),
                    None => warn!(
                        "FX check {}: {} or {} is not a group in the installed landbattle.xml",
                        claim.name, claim.a, claim.b
                    ),
                }
            }
            for claim in super::fx_probe::shot_claims_matching(needle) {
                match super::fx_probe::check_shot(&lib.lib, &db.db, claim) {
                    Some(v) => info!("{v}\n  asked: {}", v.question),
                    None => warn!(
                        "FX check {}: {} or {} is not a `projectiles` row in the installed table",
                        claim.name, claim.a, claim.b
                    ),
                }
            }
        }
        if !*reported {
            // The measurement table, once: every group this draw slot can play, so a claim's numbers
            // can be traced back to the group they came from. A group still alive at the end of the
            // watch is marked, because its peak size was then measured mid-growth.
            for group in [FIRE_GROUPS, IMPACT_GROUPS, DUST_GROUPS].concat() {
                if let Some(p) = super::fx_probe::probe(&lib.lib, group) {
                    let note = if p.finished() { "" } else { "  (STILL ALIVE AT THE END OF THE WATCH)" };
                    info!("{}{}", super::fx_probe::report(&p), note);
                }
            }
            info!("FX check: {}", super::fx_probe::summary(&lib.lib, &db.db));
            *reported = true;
        }
        exit.write(AppExit::Success);
        return;
    }
    let Some(world) = world else { return };
    let log = std::env::var("NAPOLEON_FX_LOG").ok().and_then(|s| s.parse::<f32>().ok());
    let shot = std::env::var("NAPOLEON_FX_SHOT").ok().filter(|p| !p.is_empty());
    if log.is_none() && shot.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let shot_at_battle =
        std::env::var("NAPOLEON_FX_SHOT_AT").ok().and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);

    // The screenshot: once, at the first volley in the air. It waits [`SHOT_MIN_AGE`] of real time
    // first, because a shot taken in the first frames of a run captures a window that has not been
    // composited yet and comes out blank.
    //
    // `NAPOLEON_FX_SHOT_AT=<battle seconds>` waits for the lines to close instead of taking the first
    // volley: at Austerlitz's deployment distance a volley is a few pixels wide, so a shot taken at
    // the first volley shows almost nothing. The gate is **battle** time, not real time, because the
    // battle clock is what "let the lines fight for a minute" means and it is unaffected by the
    // frame rate or the speed setting.
    let battle_now = sim.as_ref().map_or(0.0, |s| s.battle.time_seconds());
    if let Some(path) = shot.as_deref()
        && *shot_at == 0.0
        && now > SHOT_MIN_AGE
        && battle_now >= shot_at_battle
        && world.live() >= SHOT_MIN_PARTICLES
        && world.world.particles.iter().any(|p| p.age > 0.25)
    {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(std::path::PathBuf::from(path)));
        info!("FX: screenshot {path} with {} particles live", world.live());
        *shot_at = now;
    }
    if *shot_at > 0.0 && now > *shot_at + 1.5 {
        exit.write(AppExit::Success);
        return;
    }

    let Some(limit) = log else { return };
    *acc += time.delta_secs();
    if *acc < 1.0 {
        return;
    }
    *acc = 0.0;
    let (phase, t) =
        sim.as_ref().map_or(("-".into(), 0.0), |s| (format!("{:?}", s.phase), s.battle.time_seconds()));
    let (buckets, quads) = drawn.as_ref().map_or((0, 0), |d| (d.buckets, d.quads));
    info!(
        "FX {phase} t={t:.1} s: {} particles live in {buckets} buckets ({quads} quads), {} released, {} dropped at the cap",
        world.live(),
        world.world.released,
        world.world.dropped
    );
    if world.world.released as f32 >= limit {
        exit.write(AppExit::Success);
    }
}

/// The mesh handles of the draw buckets, one per bucket for the whole battle.
///
/// The buckets' meshes are replaced in place every frame, so this holds at most one `Handle<Mesh>`
/// per bucket instead of one per frame. Cleared by [`leave`].
#[derive(Resource, Default)]
pub struct FxMeshes(HashMap<BucketKey, Handle<Mesh>>);

/// What the draw layer put on screen last frame, so a run can tell "released but drew nothing" from
/// "drew it" (`NAPOLEON_FX_LOG` reads it).
#[derive(Resource, Default)]
pub struct DrawnFx {
    /// Live buckets, i.e. distinct (texture, blend) pairs in use.
    pub buckets: usize,
    /// Camera-facing quads in their meshes.
    pub quads: usize,
}

/// Every frame: rebuild the bucket meshes from the live particles.
///
/// Three things this has to get right, each of them a bug the first version had:
/// - **the camera.** The battle scene has a 2D HUD camera as well as the 3D one, and a
///   `Query<&GlobalTransform>` with no filter hands back whichever comes first, so every quad was
///   billboarded around the HUD camera's position. `With<Camera3d>` picks the right one.
/// - **the bucket order.** Looked up by key (see [`buckets`]).
/// - **the meshes.** `meshes.add(...)` every frame leaves the previous frame's meshes in
///   `Assets<Mesh>` for ever: at 60 frames a second a massed battle added hundreds of megabytes a
///   minute. The handles are created once per bucket and the asset is *replaced* in place, so the
///   count stays at one mesh per bucket.
pub fn draw_fx(
    fx: Option<Res<BattleFx>>,
    world: Option<Res<FxWorldRes>>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut views: Query<(&FxBucket, &mut Mesh3d, &mut Visibility)>,
    mut handles: ResMut<FxMeshes>,
    mut drawn: ResMut<DrawnFx>,
) {
    let (Some(fx), Some(world)) = (fx.as_ref(), world.as_ref()) else { return };
    let Some(camera) = cameras.iter().next() else { return };
    let eye = camera.translation();
    let (cam_right, cam_up) = (camera.right().as_vec3(), camera.up().as_vec3());

    let groups = buckets(&world.world, &fx.lib);
    let mut quads = 0usize;
    let mut live = 0usize;
    for (bucket, mut mesh, mut vis) in &mut views {
        let Some(particles) = groups.get(&bucket.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        quads += particles.len();
        live += 1;
        // One mesh handle per bucket, created on first use and then replaced in place.
        let id = match handles.0.get(&bucket.0) {
            Some(h) => h.clone(),
            None => {
                // `Mesh` has no `Default`, so an empty placeholder needs the same constructor
                // `quad_mesh` uses. It is replaced in place on the very next line.
                let h = meshes.add(Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default()));
                handles.0.insert(bucket.0.clone(), h.clone());
                h
            }
        };
        let _ = meshes.insert(&id, quad_mesh(particles, eye, cam_right, cam_up));
        // Only when the handle actually changes: writing the same handle through `Mut<Mesh3d>`
        // flags the component changed and the renderer would re-extract it every frame anyway.
        if mesh.0 != id {
            *mesh = Mesh3d(id);
        }
        *vis = Visibility::Inherited;
    }
    drawn.buckets = live;
    drawn.quads = quads;
    if live < groups.len() {
        warn!("Battle effects: {} live buckets but only {} bucket entities", groups.len(), live);
    }
}

// -------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------

/// The bucket a single emitter draws into.
fn key_of(effect: &Effect) -> BucketKey {
    BucketKey { texture: effect.texture().to_owned(), method: effect.render_method }
}

/// The scene light the particles take, from the loaded battle map's `.environment`.
/// PROVISIONAL: `LIGHTING/light_colour` x `light_colour_scale` plus a flat half ambient.
fn scene_light() -> Vec3 {
    let Some(map) = crate::terrain::current_map() else { return Vec3::ONE };
    let Some(l) = map.lighting.as_ref() else { return Vec3::ONE };
    let sun = Vec3::from(l.light_colour) * l.light_colour_scale;
    (sun + Vec3::splat(0.5)).max(Vec3::ZERO)
}

/// An RGBA8 (UNORM, mipped, clamped) image from one of the install's `.dds` effect textures.
fn texture_image(vfs: &ntw_formats::pack::Vfs, path: &str) -> Result<Image, String> {
    let bytes = vfs.read(path).map_err(|e| e.to_string())?;
    let dds = ntw_formats::dds::Dds::parse(&bytes).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    for level in 0..dds.mip_count {
        data.extend(dds.decode_rgba8(level));
    }
    let size = Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 };
    let mut image = Image::new(
        size,
        TextureDimension::D2,
        dds.decode_rgba8(0),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = dds.mip_count;
    // Clamped: a particle sprite never tiles (the sprite grid is in its UVs instead), and the
    // original's particle sampler is a plain clamp in every technique.
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 4,
        ..default()
    });
    Ok(image)
}

/// The volleys that have not been handled yet, by (tick, position within that tick), so a restart
/// with R does not replay the old ones.
fn new_volleys<'a>(volleys: &'a VolleyFx, seen: &mut Option<(u32, usize)>) -> Vec<&'a ntw_sim::battle::shooting::VolleyEvent> {
    let mut out = Vec::new();
    for (_, v) in &volleys.recent {
        let at = volleys.recent.iter().filter(|(_, w)| w.tick == v.tick).position(|(_, w)| std::ptr::eq(w, v)).unwrap_or(0);
        if let Some((t, n)) = *seen
            && (v.tick < t || (v.tick == t && at < n))
        {
            continue;
        }
        out.push(v);
    }
    if let Some(t) = volleys.recent.last().map(|(_, v)| v.tick) {
        let n = volleys.recent.iter().filter(|(_, w)| w.tick == t).count();
        *seen = Some((t, n));
    } else {
        *seen = None;
    }
    out
}

/// The movement state a unit's dust entity name comes from.
fn behaviour_of(unit: &ntw_sim::battle::model::LandUnit) -> &'static str {
    if unit.in_melee {
        "melee"
    } else if unit.charging {
        "charging"
    } else if unit.running {
        "running"
    } else {
        "walking"
    }
}

/// How high above the ground a unit's muzzle is drawn, in metres.
///
/// **PROVISIONAL**, and round 3 established that it has to stay that way from data. The original
/// puts the muzzle flash at the gun's own muzzle, but **no shipped file says where that is** — not
/// the gun models, not the gun table, not anywhere in the pack. Three exhaustive negatives, kept as
/// `ntw_formats/tests/effects_install.rs::no_shipped_gun_model_carries_a_muzzle_attachment_point`:
///
/// 1. `muzzle` is a substring of only **5** of the 86,977 pack entries, and none is a gun model:
///    `gun_type_to_projectiles`, `warscape_rigid`, `warscape_rigid_lod`,
///    `rigidmodels\projectile\cannon_muzzle01.rigid_model` (the shot's own impact model) and the
///    localisation file.
/// 2. All **158** gun model files (`enginemodels\*cannon*`, `*howitzer*`, `*mortar*`,
///    `*carronade*`, `*rocket*`) hold 8,415 strings between them and every one is a texture name, a
///    texture path, a `building_NNN` destruction clip or one of 15 named shader constants. **No node
///    name, so no attachment point.** This is the method 0-D used on
///    `rigid_equip_euro_flagpole01` ("attachment 1 of 134" in `euro_equipment`) applied to the guns:
///    it does not work here, because equipment attachments are swappable meshes carried by a
///    *soldier* model and a gun has none.
/// 3. `db\models_artilleries_tables\models_artillery` has one row per gun with a 683-byte numeric
///    block after it, and the block is **byte-identical across all 34 rows** — the shared
///    field-cannon rig, not a per-gun muzzle.
///
/// So the original computes the muzzle; finding how is an exe-side question, not a data one. 1.35 m
/// is a standing man's musket height and about eye height, which is where the smoke reads best.
pub const MUZZLE_HEIGHT: f32 = 1.35;

/// The muzzle a firing unit's effect comes out of: the front of its drawn block, at
/// [`MUZZLE_HEIGHT`], plus the direction the smoke blows (along the barrel, mostly forward).
pub fn muzzle_of(unit: &ntw_sim::battle::model::LandUnit, size_m: Vec2) -> ([f32; 3], [f32; 3]) {
    let half = size_m.x * 0.5;
    let ground = world_of(Vec2::new(
        unit.position.0 + unit.facing.cos() * half,
        unit.position.1 + unit.facing.sin() * half,
    ));
    ([ground.x, ground.y + MUZZLE_HEIGHT, ground.z], [unit.facing.cos(), 0.25, -unit.facing.sin()])
}

/// The scale a gun's group is played at, so a 12-pounder throws a bigger puff than a 6-pounder.
/// INFERRED: the shipped group has no per-gun scale of its own, so we use the projectile's muzzle
/// velocity against the 150 m/s the small-arms and light-gun rows carry. Small arms are always
/// 1.0, because a musket's group is tuned for a musket.
pub fn gun_strength(muzzle_velocity: f32, is_artillery: bool) -> f32 {
    if is_artillery { (muzzle_velocity / 150.0).clamp(0.6, 2.0) } else { 1.0 }
}

/// Releases the firing group for one volley at the shooter's muzzle.
///
/// The group's name is the shooter's own projectile row's `fire_effect` (`projectiles` column 31,
/// CONFIRMED; see [`super::fx::fire_group`]); the weapon-family rule is only the fallback for a row
/// with none. The muzzle position ([`muzzle_of`]) and the velocity scale ([`gun_strength`]) are
/// PROVISIONAL.
fn fire_muzzle(
    world: &mut FxWorld,
    lib: &EffectLibrary,
    sim: &BattleSim,
    db: &ntw_data::GameDatabase,
    v: &ntw_sim::battle::shooting::VolleyEvent,
) {
    let Some((i, unit)) = sim.battle.units.iter().enumerate().find(|(_, u)| u.id == v.shooter) else {
        warn!("FX: volley {} from unknown unit {}", v.tick, v.shooter);
        return;
    };
    let Some(info) = sim.info.get(i) else { return };
    let Some(view) = db.land_unit(&info.key) else {
        warn!("FX: unit {} has no `units` row for {}", v.shooter, info.key);
        return;
    };
    let projectile = db.primary_projectile(view.stats);
    let Some(p) = projectile else {
        warn!("FX: unit {} ({}) fires no projectile in the data", v.shooter, info.key);
        return;
    };
    let group = super::fx::fire_group(db, p);
    if !lib.has_group(group) {
        warn!("FX: no group {group} for {} ({})", p.key, info.key);
        return;
    }
    let is_artillery = view.stats.gun_type.is_some();
    let (position, facing) = muzzle_of(unit, info.size_m);
    let n = world.spawn_group(lib, group, position, facing, gun_strength(p.muzzle_velocity, is_artillery));
    debug!("FX: {group} x{n} at {position:?} for {} ({})", v.shooter, p.key);
    impact_at_target(world, lib, sim, db, v, p, is_artillery);
}

/// What a shot plays where it lands: blood on a man it killed, and for a gun the air burst plus
/// the scorch on the ground.
///
/// The **groups** come from the shot's own `projectiles.explosion` row in `db\projectiles_explosions`
/// (CONFIRMED — see [`super::fx::impact`]), so a 12-pounder shell gets `AirExplosion_med` and a
/// 12-pounder *shrapnel* shell gets `AirExplosion_sml`, which is what the shipped rows say. Rows
/// with no ground scorch (a carcass, a grenade) fall back to the generic ground impact.
///
/// **INFERRED**: the blood group ([`fx::BLOOD_GROUP`]) — `projectile_impacts`' surface column order
/// is still unknown. **PROVISIONAL**: the burst's sprite scale ([`burst_scale`]), the velocity scale
/// (a calibre ratio), the heights the burst and scorch are played at, and the fact that the scorch
/// is drawn on the ground at all, since the row names a group but not a place.
///
/// Our model resolves a volley at once, so the impact is played at the target's own position the
/// tick the volley happened, with no flight time (`BATTLE_EFFECTS.md` §5 item 14).
fn impact_at_target(
    world: &mut FxWorld,
    lib: &EffectLibrary,
    sim: &BattleSim,
    db: &ntw_data::GameDatabase,
    v: &ntw_sim::battle::shooting::VolleyEvent,
    projectile: &ntw_data::schemas::Projectile,
    is_artillery: bool,
) {
    let Some(target) = sim.battle.units.iter().find(|u| u.id == v.target) else { return };
    let ground = world_of(Vec2::new(target.position.0, target.position.1));
    // A shot into a man: blood at chest height.
    if v.kills > 0 {
        world.spawn_group(lib, fx::BLOOD_GROUP, [ground.x, ground.y + 1.1, ground.z], [0.0, 0.6, 0.0], 1.0);
    }
    if !is_artillery {
        return;
    }
    let pounder = fx::gun_pounder(&projectile.calibre);
    // The air burst a little above the ground and the scorch on it, both blown along the shot.
    let along = [target.facing.cos(), 0.4, -target.facing.sin()];
    // Velocity scale only: the PROVISIONAL calibre ratio, which is what it was before round 5. The
    // *size* scale is the shot's own burst radius, applied below.
    let strength = (0.6 + pounder / 24.0).clamp(0.6, 2.0);
    // CONFIRMED: the burst is the air group of this shot's own `projectiles_explosions` row. The
    // PROVISIONAL calibre rule only fills in for a row that has no air group.
    let burst = fx::air_burst(db, projectile).unwrap_or_else(|| fx::air_explosion(pounder));
    if lib.has_group(burst) {
        // The sprite size is PROVISIONAL: the same row's fourth number over 10 m (see
        // [`burst_scale`]); `strength` is the velocity and stays the calibre ratio.
        world.size_scale = burst_scale(db, projectile, pounder);
        world.spawn_group(lib, burst, [ground.x, ground.y + 0.6, ground.z], along, strength);
        world.size_scale = 1.0;
    } else {
        warn!("FX: no air burst group {burst} for {}", projectile.key);
    }
    let scorch = match fx::ground_scorch(db, projectile) {
        Some(named) => named.to_string(),
        // No scorch named on this row: a carcass or a grenade. The round-1 PROVISIONAL
        // calibre-sized generic ground impact.
        None => fx::ground_impact(pounder).to_string(),
    };
    if lib.has_group(&scorch) {
        world.spawn_group(lib, &scorch, [ground.x, ground.y + 0.1, ground.z], along, 1.0);
    } else {
        warn!("FX: no ground scorch group {scorch} for {}", projectile.key);
    }
    debug!("FX: {burst} + {scorch} at {ground:?} for {} ({})", v.shooter, projectile.key);
}

/// The sprite-size multiplier for a shot's air burst: the **fourth number of its own
/// `projectiles_explosions` row**, over the reference radius.
///
/// **PROVISIONAL, a stand-in, not the original's rule.** What is CONFIRMED is the premise:
/// `AirExplosion_sml`, `AirExplosion_med` and `AirExplosion_lrg` are **byte-identical groups** in
/// `effects\landbattle.xml` — the same eight entries in the same order, and the same `spawn_interval`
/// 100 / `cell_size` 10 / `cell_count` 1
/// (`fx_probe::tests::the_three_air_burst_groups_are_one_group`, install). So the group name cannot
/// make a 64 lb shell's burst bigger than a 3 lb one's. What is **not** known is whether the
/// original draws them at different sizes at all, and if it does, from what: that the fourth number
/// is a radius in metres is INFERRED, that the exe feeds it to the sprite sizes is a guess (it may
/// be the gameplay blast radius only), and the linear scale about a 10 m reference is ours. Target:
/// the exe's read of `projectiles_explosions`' numeric block and its air-burst spawn call.
///
/// The shipped values (CONFIRMED by `the_burst_rises_with_the_pound_count_over_the_shipped_rows`,
/// install); the scale column is ours:
///
/// | row | radius (m) | scale | | row | radius (m) | scale |
/// |---|---|---|---|---|---|---|
/// | `shrapnel_3lb` | 2 | 0.20 | | `shell_12lb` | 10 | 1.00 |
/// | `shrapnel_12lb` | 6 | 0.60 | | `shell_24lb` | 15 | 1.50 |
/// | `shrapnel_32lb` | 12 | 1.20 | | `shell_64lb` | 25 | 2.50 |
/// | `mortar_4_shell` | 15 | 1.50 | | `mortar_8_shell` | 25 | 2.50 |
///
/// **Where it is INFERRED:** that the unit is metres and that the number is a radius. It is read
/// only for the rows whose air group is one of the three `AirExplosion_*` groups — the shell, shrapnel
/// and mortar families, where it rises monotonically with the pound count and lands in 2..25 m. For
/// every other family the fourth number is plainly a different quantity and **is not used**: a
/// `grenade` row's is 0, a `quicklime` row's 0.4, a `rocket` row's 0.99 and a `carcass` row's 0.99,
/// which are not blast radii in metres for those shot types. Those rows keep scale 1.0 rather than
/// being shrunk to the clamp by a number read from the wrong column, and `the_burst_rises_with_the_
/// pound_count_over_the_shipped_rows` pins the family boundary.
///
/// **A row that names no air group at all** — which is most field guns, see
/// [`burst_scale_falls_back_on_calibre`] — gets the PROVISIONAL calibre ratio, because there is no
/// number to read.
pub fn burst_scale(db: &ntw_data::GameDatabase, projectile: &ntw_data::schemas::Projectile, pounder: f32) -> f32 {
    let row = db.explosion_effects(projectile);
    let is_air_explosion = row
        .and_then(|r| r.air.as_deref())
        .is_some_and(|g| super::fx::impact::AIR_EXPLOSIONS.contains(&g));
    match row.and_then(|r| r.numbers.get(BURST_RADIUS_COLUMN).copied()) {
        // Only the three air-explosion families, and only a value that is credible as a radius in
        // metres there. See [`MIN_BURST_RADIUS`] for the two exceptions that reading is not.
        Some(r) if is_air_explosion && r >= MIN_BURST_RADIUS => (r / REFERENCE_RADIUS).clamp(0.2, 2.5),
        _ => (0.6 + pounder / 24.0).clamp(0.6, 2.0),
    }
}

/// The smallest fourth number this is read at: 2 m, which is `shrapnel_3lb`'s.
///
/// **Two shipped rows sit below it and are left on the fallback**, because reading them would be
/// wrong rather than merely uncertain:
///
/// - the **rocket** rows, `rocket` and `rocket_naval`, whose air group *is* an `AirExplosion_*` group
///   but whose fourth number is **0.99**. Divided by the 10 m reference that is a 0.099 scale, which
///   would draw a Congreve rocket's air burst at a tenth of the sprite sizes — smaller than anything
///   else in the file. Whether the column means something else for a rocket (a fuse time, a
///   fragment count) or the rocket really does get a tiny air pop is **UNKNOWN**; the target is the
///   exe's own read of `projectiles_explosions`' numeric block, which is the same question the
///   column layout is. Until then they get the PROVISIONAL calibre ratio, as they did before.
/// - the `grenade`, `quicklime` and `carcass` rows, whose air group is *not* an `AirExplosion_*`
///   group and whose numbers are 0, 0.4 and 0.99 — filtered by the family test above.
///
/// INFERRED as metres; CONFIRMED that it is monotone over the shell, shrapnel and mortar families and
/// that these two families break it. The floor itself is PROVISIONAL (ours).
pub const MIN_BURST_RADIUS: f32 = 2.0;

/// Which of a `projectiles_explosions` row's numbers is the burst radius: the fourth (index 3).
///
/// Kept as a named constant because it is **INFERRED**, not decoded — the table's column layout is
/// still UNKNOWN (`BATTLE_EFFECTS.md` §2), so this is "the fourth number", which the shipped values
/// then make coherent as a radius over one family. The alternative readings were checked and are
/// worse: the first number is a damage-ish scalar that is not monotone across families (`grenade` is
/// 0.001, `carcass_12lb` is 50), the third is 10 or 20 throughout, and the last number of every row
/// is a denormal (`6e-45`, `2.8e-44`) — the last element of a float array, not a value.
pub const BURST_RADIUS_COLUMN: usize = 3;

/// The radius that plays at the shipped sprite sizes: a 12-pounder round shell's 10 m.
/// PROVISIONAL (ours): no shipped value says which row the authored sizes belong to.
pub const REFERENCE_RADIUS: f32 = 10.0;

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::effects::EffectLibrary;

    /// A stand-in for `effects\landbattle.xml` with one alpha emitter and one additive emitter,
    /// so the bucket test can check that the two land in different buckets.
    const SAMPLE_DOC: &str = r#"
<EFFECTS_MANAGER><SCRIPTED_EFFECT_INFO_LIST>
<SCRIPTED_EFFECT_INFO name='puff' num_particles_per_point='1' gravity_range='variance(-9.81,0.0)'>
 <SCRIPTED_EFFECT_EMISSION_CONTROL emission_type='EMISSION_TYPE_POINT'>
  <RELEASE_INFO release_type='RELEASE_TYPE_NONE'/>
  <EMISSION_MODIFIERS><EMISSION_MODIFIER_VELOCITY value='1.0'/><EMISSION_MODIFIER_LIFETIME value='1.0'/><EMISSION_MODIFIER_OPACITY value='1.0'/></EMISSION_MODIFIERS>
 </SCRIPTED_EFFECT_EMISSION_CONTROL>
 <SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES life_range='variance(2.0,0.0)'>
  <MOVEMENT_INFO velocity_range='variance(1.0,0.0)' dir_range_XYZ='vector(0.0,1.0,0.0)'/>
  <ANIMATION_INFO total_frames='variance(1,0)' cell_width='128' cell_height='128'/>
  <COLOUR_INFO start_colour_range_r='variance(255,0)' start_colour_range_g='variance(255,0)' start_colour_range_b='variance(255,0)'
   end_colour_range_r='variance(255,0)' end_colour_range_g='variance(255,0)' end_colour_range_b='variance(255,0)' colour_range_a='variance(255,0)'/>
  <FADE_INFO fadein_range_life_unary='variance(0.0,0.0)' fadeout_range_life_unary='variance(1.0,0.0)'/>
  <SCALE_INFO initial_scale_range_metres='vector(variance(1.0,0.0),variance(1.0,0.0))'
   primary_scale_life_unary_range='variance(0.0,0.0)' primary_target_scale_range_metres='vector(variance(1.0,0.0),variance(1.0,0.0))'
   secondary_scale_life_unary_range='variance(1.0,0.0)' secondary_target_scale_range_metres='vector(variance(1.0,0.0),variance(1.0,0.0))'/>
  <ROTATION_INFO rotation_range='variance(0.0,0.0)' rotations_per_second_range='variance(0.0,0.0)'/>
 </SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES>
 <SCRIPTED_EFFECT_RENDERING_VARS texture_1='effects\textures\emp_smoke_diffuse.dds' fx='particle.fx'
  render_method='RENDER_METHOD_ALPHA' sprite_facing_mode='CAMERA_FACING'/>
</SCRIPTED_EFFECT_INFO>
<SCRIPTED_EFFECT_INFO name='flash' num_particles_per_point='1' gravity_range='variance(0.0,0.0)'>
 <SCRIPTED_EFFECT_EMISSION_CONTROL emission_type='EMISSION_TYPE_POINT'>
  <RELEASE_INFO release_type='RELEASE_TYPE_NONE'/>
  <EMISSION_MODIFIERS><EMISSION_MODIFIER_VELOCITY value='1.0'/><EMISSION_MODIFIER_LIFETIME value='1.0'/><EMISSION_MODIFIER_OPACITY value='1.0'/></EMISSION_MODIFIERS>
 </SCRIPTED_EFFECT_EMISSION_CONTROL>
 <SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES life_range='variance(0.2,0.0)'>
  <MOVEMENT_INFO velocity_range='variance(0.0,0.0)' dir_range_XYZ='vector(0.0,1.0,0.0)'/>
  <ANIMATION_INFO total_frames='variance(1,0)' cell_width='32' cell_height='32'/>
  <COLOUR_INFO start_colour_range_r='variance(255,0)' start_colour_range_g='variance(255,0)' start_colour_range_b='variance(255,0)'
   end_colour_range_r='variance(255,0)' end_colour_range_g='variance(255,0)' end_colour_range_b='variance(255,0)' colour_range_a='variance(255,0)'/>
  <FADE_INFO fadein_range_life_unary='variance(0.0,0.0)' fadeout_range_life_unary='variance(1.0,0.0)'/>
  <SCALE_INFO initial_scale_range_metres='vector(variance(0.5,0.0),variance(0.5,0.0))'
   primary_scale_life_unary_range='variance(0.0,0.0)' primary_target_scale_range_metres='vector(variance(0.5,0.0),variance(0.5,0.0))'
   secondary_scale_life_unary_range='variance(1.0,0.0)' secondary_target_scale_range_metres='vector(variance(0.5,0.0),variance(0.5,0.0))'/>
  <ROTATION_INFO rotation_range='variance(0.0,0.0)' rotations_per_second_range='variance(0.0,0.0)'/>
 </SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES>
 <SCRIPTED_EFFECT_RENDERING_VARS texture_1='effects\textures\emp_flare_diffuse.dds' fx='particle2.fx'
  render_method='RENDER_METHOD_ADDITIVE' sprite_facing_mode='CAMERA_FACING'/>
</SCRIPTED_EFFECT_INFO>
</SCRIPTED_EFFECT_INFO_LIST>
<SCRIPTED_EFFECT_GROUP_INFO>
<SCRIPTED_EFFECT_GROUP name='Fire'><EFFECT_GROUP_AMBIENT_SETTINGS spawn_interval='100.0'/>
<SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/><SCRIPTED_EFFECT_GROUP_ENTRY name='flash' effect='flash'/>
</SCRIPTED_EFFECT_GROUP>
<SCRIPTED_EFFECT_GROUP name='infantry_walk_dust'><EFFECT_GROUP_AMBIENT_SETTINGS spawn_interval='100.0'/>
<SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/></SCRIPTED_EFFECT_GROUP>
<SCRIPTED_EFFECT_GROUP name='cavalry_walk_dust'><EFFECT_GROUP_AMBIENT_SETTINGS spawn_interval='100.0'/>
<SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/></SCRIPTED_EFFECT_GROUP>
<SCRIPTED_EFFECT_GROUP name='infantry_combat_dust'><EFFECT_GROUP_AMBIENT_SETTINGS spawn_interval='100.0'/>
<SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/></SCRIPTED_EFFECT_GROUP>
</SCRIPTED_EFFECT_GROUP_INFO></EFFECTS_MANAGER>"#;

    /// A mesh's position (or any other `[f32; 3]`) attribute as plain vectors.
    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.iter().copied().map(Vec3::from).collect(),
            other => panic!("expected Float32x3 positions, got {}", other.is_some()),
        }
    }

    /// A camera at `eye` looking at `target`, as `draw_fx` reads it: its position, right and up.
    fn camera_at(eye: Vec3, target: Vec3) -> (Vec3, Vec3, Vec3) {
        let t = Transform::from_translation(eye).looking_at(target, Vec3::Y);
        (eye, t.right().as_vec3(), t.up().as_vec3())
    }

    /// **Review fix.** The quad lies in the camera's own screen plane wherever the camera is. The old
    /// basis was built from `eye.normalize()` (origin to camera), which only matched the screen when
    /// the camera looked at the world origin; this camera is 2 km from it and looking sideways, so
    /// the old quads were turned well off the screen plane.
    #[test]
    fn the_quad_lies_in_the_screen_plane_far_from_the_origin() {
        let mut p = particle("puff", 0, 1);
        p.position = [2000.0, 5.0, -1500.0];
        let target = Vec3::new(2100.0, 0.0, -1500.0);
        let (eye, r, u) = camera_at(Vec3::new(1900.0, 60.0, -1400.0), target);
        let forward = Transform::from_translation(eye).looking_at(target, Vec3::Y).forward();
        let pos = positions(&quad_mesh(&[&p], eye, r, u));
        for edge in [pos[1] - pos[0], pos[3] - pos[0]] {
            assert!(edge.normalize().dot(forward.as_vec3()).abs() < 1e-4, "edge {edge:?} leaves the screen plane");
        }
        assert!((pos[1] - pos[0]).normalize().dot(r) > 0.9999, "width runs along the camera's right");
    }

    fn particle(effect: &str, frame: u32, frames: u32) -> Particle {
        Particle {
            effect: effect.into(),
            position: [1.0, 2.0, 3.0],
            velocity: [0.0; 3],
            life: 1.0,
            age: 0.0,
            start_colour: [1.0, 1.0, 1.0],
            end_colour: [0.0, 0.0, 0.0],
            alpha: 1.0,
            fade_in: 0.0,
            fade_out: 1.0,
            start_scale: [2.0, 4.0],
            mid_scale: [2.0, 4.0],
            end_scale: [2.0, 4.0],
            primary_at: 0.0,
            secondary_at: 1.0,
            rotation: 0.0,
            rotation_speed: 0.0,
            frame,
            frames,
            slot: 0,
            render_method: RenderMethod::Alpha,
            facing: ntw_formats::effects::FacingMode::Camera,
            gravity: 0.0,
            wind: 0.0,
            dampening: 0.0,
            serial: 0,
        }
    }

    #[test]
    fn one_quad_per_particle_four_vertices_six_indices() {
        let ps = [particle("puff", 0, 4), particle("puff", 1, 4)];
        let refs: Vec<&Particle> = ps.iter().collect();
        let (eye, r, u) = camera_at(Vec3::new(0.0, 50.0, 100.0), Vec3::ZERO);
        let mesh = quad_mesh(&refs, eye, r, u);
        assert_eq!(mesh.count_vertices(), 8);
        assert_eq!(mesh.indices().map_or(0, |i| i.len()), 12);
        let uv = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap();
        assert_eq!(uv.len(), 8);
        let colour = mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap();
        assert_eq!(colour.len(), 8);
    }

    #[test]
    fn the_quad_is_camera_facing_and_the_right_size() {
        let p = particle("puff", 0, 1);
        let (eye, r, u) = camera_at(Vec3::new(0.0, 50.0, 100.0), Vec3::new(1.0, 2.0, 3.0));
        let mesh = quad_mesh(&[&p], eye, r, u);
        let pos = positions(&mesh);
        // The centre of the quad is the particle's position.
        let centre = (0..4).fold(Vec3::ZERO, |a, i| a + pos[i]) / 4.0;
        assert!((centre - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5, "centre {centre:?}");
        // Its width is 2 m and its height 4 m along the camera's right and up.
        let across = pos[1] - pos[0];
        let up = pos[3] - pos[0];
        assert!((across.length() - 2.0).abs() < 1e-5, "width {}", across.length());
        assert!((up.length() - 4.0).abs() < 1e-5, "height {}", up.length());
        // The normal points from the quad back at the camera. (This read the position attribute
        // by mistake until round 3, so the check below is the first one to really run.)
        let normals: Vec<Vec3> = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.iter().copied().map(Vec3::from).collect(),
            other => panic!("expected Float32x3 normals, got {}", other.is_some()),
        };
        let to_eye = (eye - centre).normalize();
        assert!(normals[0].dot(to_eye) > 0.99, "normal {:?} vs to-eye {to_eye:?}", normals[0]);
    }

    /// The shipped `LOCAL_Y_AXIS` / `WORLD_Y_AXIS` emitters — the ground-impact distortion and the
/// water ripples — must stand upright and turn about the vertical, while `CAMERA_FACING` and
/// `BILLBOARD` keep pointing at the camera. The two differ most when the camera is low: from a low
/// angle a camera-facing ground card tips to face the eye and disappears, an upright one does not.
#[test]
fn a_y_axis_sprite_stands_upright_while_a_camera_one_tips_to_the_eye() {
    use ntw_formats::effects::FacingMode;
    // A low camera, off to one side, so "upright" and "facing the eye" cannot be confused.
    let eye = Vec3::new(60.0, 12.0, 100.0);

    let mut camera = particle("ground_impact_distortion", 0, 1);
    camera.facing = FacingMode::Camera;
    let mut yaxis = particle("ground_impact_distortion", 0, 1);
    yaxis.facing = FacingMode::LocalYAxis;
    let mut billboard = particle("cannon_hit_smoke", 0, 1);
    billboard.facing = FacingMode::Billboard;

    let (eye, r, u) = camera_at(eye, Vec3::new(1.0, 2.0, 3.0));
    let m = quad_mesh(&[&camera, &yaxis, &billboard], eye, r, u);
    let pos = positions(&m);
    let quad = |i: usize| [pos[i * 4], pos[i * 4 + 1], pos[i * 4 + 2], pos[i * 4 + 3]];

    // Every quad is still the right size and centred on its particle.
    for (i, q) in [quad(0), quad(1), quad(2)].into_iter().enumerate() {
        let centre = q.iter().fold(Vec3::ZERO, |a, v| a + *v) / 4.0;
        assert!((centre - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-5, "quad {i} centre {centre:?}");
        assert!(((q[1] - q[0]).length() - 2.0).abs() < 1e-5, "quad {i} width");
        assert!(((q[3] - q[0]).length() - 4.0).abs() < 1e-5, "quad {i} height");
    }

    // The upright quad's height axis is world up, whatever the camera's roll.
    let upright = quad(1)[3] - quad(1)[0];
    assert!((upright.normalize().dot(Vec3::Y) - 1.0).abs() < 1e-5, "upright axis {upright:?}");
    // The camera-facing one is not: it follows the camera's own up, which is tilted.
    let tipped = quad(0)[3] - quad(0)[0];
    assert!(tipped.normalize().dot(Vec3::Y) < 0.999, "camera quad was already upright: {tipped:?}");
    // BILLBOARD is drawn like CAMERA_FACING, which is a settled negative rather than a guess: no
    // shipped attribute separates the two and the shader is handed one camera-aligned basis only.
    let b = quad(2)[3] - quad(2)[0];
    assert!((b - tipped).length() < 1e-5, "BILLBOARD should match CAMERA_FACING");

    // The upright quad's normal is horizontal; the camera-facing one points straight at the eye.
    let normals: Vec<Vec3> = match m.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(v)) => v.iter().copied().map(Vec3::from).collect(),
        other => panic!("expected Float32x3 normals, got {}", other.is_some()),
    };
    // All three quads are at the same place, so they share one to-eye direction.
    let to_eye = (eye - Vec3::new(1.0, 2.0, 3.0)).normalize();
    assert!(normals[0].dot(to_eye) > 0.99, "camera-facing normal {:?} vs to-eye {to_eye:?}", normals[0]);
    // The upright quad's normal is the horizontal part of the eye direction, so it has no y at all.
    let upright_normal = normals[4];
    assert!(upright_normal.y.abs() < 1e-5, "upright normal has a y: {upright_normal:?}");
    let want = Vec3::new(to_eye.x, 0.0, to_eye.z).normalize();
    assert!((upright_normal - want).length() < 1e-5, "upright normal {upright_normal:?} vs {want:?}");
    // BILLBOARD's normal matches CAMERA_FACING's: the settled negative, kept in the unit suite as well
    // as on the install so the drawing cannot drift without one of the two noticing.
    assert!((normals[8] - normals[0]).length() < 1e-5);
}

#[test]
fn the_sprite_frame_is_a_squarest_grid() {
    // 16 frames -> 4x4, so frame 5 is column 1, row 1.
        let (u, v, du, dv) = frame_uv(&particle("puff", 5, 16));
        assert!((u - 0.25).abs() < 1e-6 && (v - 0.25).abs() < 1e-6, "u {u} v {v}");
        assert!((du - 0.25).abs() < 1e-6 && (dv - 0.25).abs() < 1e-6);
        // One frame fills the sheet.
        let (u, v, du, dv) = frame_uv(&particle("puff", 3, 1));
        assert_eq!((u, v, du, dv), (0.0, 0.0, 1.0, 1.0));
        // Nine frames -> 3x3 exactly; the last frame's row is 2.
        let (_, v, _, dv) = frame_uv(&particle("puff", 8, 9));
        assert!((v - 2.0 / 3.0).abs() < 1e-6 && (dv - 1.0 / 3.0).abs() < 1e-6);
        // Six frames -> 3 columns, 2 rows, so frame 5 is column 2 of row 1 (the last cell).
        let (u, v, du, dv) = frame_uv(&particle("puff", 5, 6));
        assert!((u - 2.0 / 3.0).abs() < 1e-6 && (v - 0.5).abs() < 1e-6, "u {u} v {v}");
        assert!((du - 1.0 / 3.0).abs() < 1e-6 && (dv - 0.5).abs() < 1e-6);
        // A frame index past the end wraps rather than pointing off the sheet.
        let (u, _, _, _) = frame_uv(&particle("puff", 7, 6));
        assert!((u - 1.0 / 3.0).abs() < 1e-6, "u {u}");
    }

    #[test]
    fn buckets_group_by_texture_and_blend() {
        let lib = EffectLibrary::read(SAMPLE_DOC.as_bytes()).expect("the sample document parses");
        let mut world = FxWorld::new(1);
        world.spawn_group(&lib, "Fire", [0.0; 3], [0.0, 1.0, 0.0], 1.0);
        let groups = buckets(&world, &lib);
        // Two emitters with two different textures and blend modes: two buckets, each holding
        // one particle, and every particle in exactly one bucket.
        assert_eq!(groups.len(), 2, "{:?}", groups.keys().map(|k| &k.texture).collect::<Vec<_>>());
        assert_eq!(groups.values().map(Vec::len).sum::<usize>(), world.particles.len());
        assert!(groups.values().all(|particles| particles.len() == 1));
        // The smoke is alpha-blended and the flash additive, each under its own texture.
        let find = |name: &str, method: RenderMethod| {
            groups
                .iter()
                .find(|(k, _)| k.method == method && k.texture.ends_with(name))
                .map(|(k, _)| k.texture.clone())
        };
        assert_eq!(
            find("emp_smoke_diffuse.dds", RenderMethod::Alpha).as_deref(),
            Some(r"effects\textures\emp_smoke_diffuse.dds")
        );
        assert_eq!(
            find("emp_flare_diffuse.dds", RenderMethod::Additive).as_deref(),
            Some(r"effects\textures\emp_flare_diffuse.dds")
        );
        // The map is sorted by key, so the bucket list it feeds is in a stable order.
        let keys: Vec<&BucketKey> = groups.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        // And the draw layer looks its bucket up by key, not by position: a `Query` gives no order
        // guarantee, which is why `draw_fx` may not index into this map.
        let smoke = find("emp_smoke_diffuse.dds", RenderMethod::Alpha).unwrap();
        assert_eq!(groups.get(&BucketKey { texture: smoke, method: RenderMethod::Alpha }).map(Vec::len), Some(1));
    }

    #[test]
    fn a_particle_of_an_unknown_emitter_is_skipped_not_drawn() {
        // A mod may leave a group entry pointing at an effect another mod removed; the draw layer
        // must not panic or draw it in the wrong bucket.
        let lib = EffectLibrary::read(SAMPLE_DOC.as_bytes()).expect("parse");
        let mut world = FxWorld::new(1);
        world.particles.push(Particle {
            effect: "gone_with_the_mod".into(),
            position: [0.0; 3],
            velocity: [0.0; 3],
            life: 1.0,
            age: 0.0,
            start_colour: [1.0; 3],
            end_colour: [1.0; 3],
            alpha: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            start_scale: [1.0, 1.0],
            mid_scale: [1.0, 1.0],
            end_scale: [1.0, 1.0],
            primary_at: 0.0,
            secondary_at: 1.0,
            rotation: 0.0,
            rotation_speed: 0.0,
            frame: 0,
            frames: 1,
            slot: 0,
            render_method: RenderMethod::Alpha,
            facing: ntw_formats::effects::FacingMode::Camera,
            gravity: 0.0,
            wind: 0.0,
            dampening: 0.0,
            serial: 0,
        });
        assert!(buckets(&world, &lib).is_empty());
    }

    /// **Round 4.** This used to loop `DUST_GROUPS` and ask `lib.group_effects(g)` of `SAMPLE_DOC`
    /// — a two-emitter sample document this module's own test wrote. So it could only ever fail if
    /// someone edited that sample, and it said nothing about the shipped library, which is where a
    /// wrong group name would actually cost us a puff. The real check is the **join**: a group
    /// [`fx::dust_puff`] can return is a group whose texture gets uploaded, or the dust draws
    /// nothing. Nothing tested that, so it is what is tested now. Whether the three names exist in
    /// the shipped `landbattle.xml` is an install fact and is checked there instead:
    /// `ntw_formats/tests/effects_install.rs::every_dust_group_is_a_shipped_group`.
    #[test]
    fn every_dust_group_a_puff_can_ask_for_is_uploaded() {
        let params = DustParameters::parse(b"infantry_walking\t0.05\r\ncavalry_running\t0.15\r\n");
        assert!(!DUST_GROUPS.is_empty());
        // No repeat, or a texture is fetched twice.
        let mut uploaded = DUST_GROUPS.to_vec();
        let before = uploaded.len();
        uploaded.sort_unstable();
        uploaded.dedup();
        assert_eq!(uploaded.len(), before, "DUST_GROUPS lists a group twice");
        assert!(uploaded.iter().all(|g| !g.is_empty()), "an empty dust group name");

        // Every entity/behaviour pair the shipped table knows must land on an uploaded group, and
        // every one of the 15 `// type` names must resolve to something at all.
        let mut asked: Vec<&str> = Vec::new();
        for entity in [
            "infantry_walking", "infantry_running", "infantry_charging", "infantry_melee",
            "cavalry_walking", "cavalry_running", "cavalry_charging", "cavalry_melee",
            "elephants_walking", "elephants_running", "elephants_charging", "elephants_melee",
            "artillery_walking", "artillery_running", "artillery_melee",
        ] {
            for behaviour in ["walking", "running", "charging", "melee"] {
                if let Some((group, freq)) = fx::dust_puff(&params, entity, behaviour) {
                    assert!(DUST_GROUPS.contains(&group), "{entity}/{behaviour} asks for {group}, which is never uploaded");
                    assert!(freq > 0.0, "{entity}/{behaviour} puffs at {freq}");
                    if !asked.contains(&group) {
                        asked.push(group);
                    }
                }
            }
        }
        // Every uploaded group is reachable, so the list cannot grow a name nothing plays either.
        for g in DUST_GROUPS {
            assert!(asked.contains(g), "{g} is uploaded but no entity ever puffs it");
        }
    }

    /// Every group the impact code can ask for is uploaded, and every ask lands in one of them.
    /// A group missing from [`IMPACT_GROUPS`] would draw nothing, because only the listed groups
    /// get their textures uploaded.
    #[test]
    fn every_impact_group_is_uploaded_and_unique() {
        // What the impact code can actually ask for. These deliberately overlap
        // `IMPACT_GROUPS` — that overlap is the point of the first assertion, so
        // they must not be pooled with it before the duplicate check.
        let mut asked: Vec<&str> = vec![fx::BLOOD_GROUP];
        asked.extend(fx::impact::AIR_EXPLOSIONS);
        asked.extend(fx::impact::GROUND_IMPACTS);
        let before = asked.len();
        asked.sort_unstable();
        asked.dedup();
        assert_eq!(asked.len(), before, "the impact module asks for the same group twice");
        for g in &asked {
            assert!(IMPACT_GROUPS.contains(g), "{g} is asked for but never uploaded");
            assert!(!g.is_empty(), "an empty group name");
        }
        // The upload list itself must not repeat a group, or a texture is fetched twice.
        let mut uploaded = IMPACT_GROUPS.to_vec();
        let count = uploaded.len();
        uploaded.sort_unstable();
        uploaded.dedup();
        assert_eq!(uploaded.len(), count, "IMPACT_GROUPS lists a group twice");
        // The fallback size rules only ever return groups from the uploaded list, and so does the
        // one ground-scorch name the shipped `projectiles_explosions` rows can ask for.
        for p in [0.0, 3.0, 6.0, 9.0, 12.0, 24.0, 64.0] {
            assert!(IMPACT_GROUPS.contains(&fx::air_explosion(p)), "air_explosion({p})");
            assert!(IMPACT_GROUPS.contains(&fx::ground_impact(p)), "ground_impact({p})");
        }
        assert!(IMPACT_GROUPS.contains(&"Cannon_Groundimpact_explosive"));
    }

    #[test]
    fn fire_group_names_are_unique() {
        // A duplicate entry would upload a group's textures twice; this catches a copy-paste.
        let mut names: Vec<&str> = FIRE_GROUPS.to_vec();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "FIRE_GROUPS has a duplicate");
        assert!(names.iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn every_weapon_family_maps_to_a_listed_fire_group() {
        // The shipped group names are CONFIRMED; a name not in FIRE_GROUPS would silently draw
        // nothing, because only the listed groups get their textures uploaded. This exercises the
        // PROVISIONAL fallback, which is the path a `projectiles` row with no `fire_effect` takes.
        for family in ["musket_flintlock", "musket_breech_loader", "pistol", "rifle", "cannon", "howitzer", "mortar", "carronade", "none"] {
            let g = fx::fire_group_by_name(family, "cannon_12_pounder");
            assert!(FIRE_GROUPS.contains(&g), "{family} -> {g} is not in FIRE_GROUPS");
        }
    }

    /// Both halves of [`fx::fire_group`], which the fixture cannot supply on its own.
///
/// Round 4: this test used to assert only that a fixture row with **no** `fire_effect` falls back,
/// while its name promised "reads the projectiles column **then** falls back". Mutating the data
/// read out of `fire_group` left all 95 tests in the crate green, so the CONFIRMED path — the one
/// round 2 actually shipped — had no unit test at all. Both branches are now driven here by
/// mutating a copy of the fixture row, so the data read is load-bearing.
///
/// The values are the shipped ones (`shell_12lb` plays `AirExplosion_med`, `shrapnel_12lb`
/// `_sml`), and the column itself is pinned on the install by
/// `ntw_data/tests/effects_data.rs::explosion_rows_name_the_effect_groups_they_play`.
#[test]
fn fire_group_reads_the_projectiles_column_then_falls_back() {
    use ntw_data::schemas::Projectile;
    let db = ntw_data::GameDatabase::test_fixture();
    let ball = db.projectile("fixture_ball").expect("the fixture's projectile");

    // The fallback end: no `fire_effect`, so the weapon family decides.
    assert_eq!(ball.fire_effect, None, "the fixture carries no fire group, so it must fall back");
    assert_eq!(ball.weapon_family, None, "the fixture row names no family either");
    assert_eq!(fx::fire_group(&db, ball), "MusketFire", "an unknown family falls back to musket");

    // The data end: the same row, with the column set. Every one of these asserts that the returned
    // group is the column's value and NOT what the family would have produced, so a `fire_group`
    // that ignored the column cannot pass them.
    for (column, family, calibre, not_the_fallback) in [
        ("LandGunFire_canister", "cannon", "cannon_12_pounder", "LandGunFire_large"),
        ("CannonFire", "cannon", "cannon_6_pounder", "LandGunFire_small"),
        ("LandGunFire_howitzer", "howitzer", "mortar_8", "LandGunFire_mortar"),
        ("rifleFire", "rifle", "musket_ball", "MusketFire"),
        ("MusketFire", "cannon", "cannon_24_pounder", "LandGunFire_large"),
    ] {
        let mut row: Projectile = ball.clone();
        row.fire_effect = Some(column.to_string());
        row.weapon_family = Some(family.to_string());
        row.calibre = calibre.to_string();
        let got = fx::fire_group(&db, &row);
        assert_eq!(got, column, "fire_effect {column} over family {family}");
        assert_ne!(got, not_the_fallback, "fire_effect {column} lost to the family fallback");
    }

    // And the fixture really does ship no effect tables, so the air-burst and scorch lookups below
    // are testing the *absence* path rather than passing by accident.
    assert!(db.projectile_explosions.is_empty() && db.projectile_impacts.is_empty(), "the fixture ships no effect tables");
    assert_eq!(fx::air_burst(&db, ball), None);
    assert_eq!(fx::ground_scorch(&db, ball), None);
    // A row that *does* name an explosion still resolves to nothing, because the fixture has no
    // `projectiles_explosions` table to resolve it against. That is the mod-with-a-stale-key case.
    let mut stale = ball.clone();
    stale.explosion = Some("shell_12lb".into());
    assert_eq!(fx::air_burst(&db, &stale), None, "the fixture has no explosion table");
    assert_eq!(fx::ground_scorch(&db, &stale), None);
    }

    /// A minimal `LandUnit` for the muzzle maths. Only the fields [`muzzle_of`] reads are set.
    fn unit_at(x: f32, y: f32, facing: f32) -> ntw_sim::battle::model::LandUnit {
        let mut u = ntw_sim::battle::model::LandUnit::new(1, 0, 100, (x, y));
        u.facing = facing;
        u
    }

    #[test]
    fn the_muzzle_is_half_a_unit_block_in_front_at_musket_height() {
        // Facing 0 is +x, and world z is -y, so the muzzle moves along +x and z is -20.
        let (pos, facing) = muzzle_of(&unit_at(10.0, 20.0, 0.0), Vec2::new(4.0, 2.0));
        assert!((pos[0] - 12.0).abs() < 1e-5, "x {pos:?}");
        assert!((pos[1] - MUZZLE_HEIGHT).abs() < 1e-5, "y {pos:?}");
        assert!((pos[2] + 20.0).abs() < 1e-5, "z {pos:?}");
        assert!((facing[0] - 1.0).abs() < 1e-6 && facing[1] > 0.0 && facing[2].abs() < 1e-6, "{facing:?}");
        // Facing pi/2 is +y in map space, which is -z in world space: the muzzle moves 3 m along -z
        // and, with no battle map loaded, stays at the muzzle height above y = 0.
        let (pos, facing) = muzzle_of(&unit_at(0.0, 0.0, std::f32::consts::FRAC_PI_2), Vec2::new(6.0, 2.0));
        assert!((pos[2] + 3.0).abs() < 1e-5 && pos[0].abs() < 1e-5, "{pos:?}");
        assert!((pos[1] - MUZZLE_HEIGHT).abs() < 1e-5, "height {pos:?}");
        assert!((facing[2] + 1.0).abs() < 1e-6, "the smoke blows along -z: {facing:?}");
    }

    #[test]
    fn only_gun_strength_varies_and_it_stays_in_its_band() {
        // Small arms are always 1.0 whatever their muzzle velocity.
        for v in [50.0, 150.0, 400.0] {
            assert_eq!(gun_strength(v, false), 1.0, "{v} m/s small arms");
        }
        // A gun scales with its muzzle velocity, clamped to 0.6..2.0.
        assert!((gun_strength(150.0, true) - 1.0).abs() < 1e-6);
        assert!(gun_strength(250.0, true) > 1.6);
        assert_eq!(gun_strength(1000.0, true), 2.0);
        assert_eq!(gun_strength(1.0, true), 0.6);
    }

}

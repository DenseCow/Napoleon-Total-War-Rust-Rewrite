//! Battle visual effects: the CPU side of the particle system.
//!
//! **The data is the original's.** [`ntw_formats::effects`] reads `effects\landbattle.xml`, which
//! holds 283 `SCRIPTED_EFFECT_INFO` emitters and 152 `SCRIPTED_EFFECT_GROUP` composites; this
//! module turns a *group* into live particles and steps them. The Bevy draw layer is
//! [`crate::battle::draw_fx`] at the bottom of the same file.
//!
//! Everything here is **display-only**: the battle model never reads [`FxWorld`]. It exists
//! because the original's effects have to be reproducible — a replay of the same seed has to look
//! the same — so the RNG is [`ntw_sim::rng::CaRng`] (the MSVC LCG, CONFIRMED W1 §12.1), driven from
//! the battle's own seed and advanced only by the fixed 0.1 s model tick, never by frame time.
//!
//! Tags: the emitter attribute names and their meaning as the file's own documentation are
//! CONFIRMED; how the engine spends `conic_range_fan`/`conic_range_depth` on a direction, the
//! order in which a group's emitters release, and the engine's own particle RNG seed are UNKNOWN
//! (see `analysis/graphics/BATTLE_EFFECTS.md` §2).

use std::collections::HashMap;

use ntw_formats::effects::{ChannelLink, DustParameters, Effect, EffectLibrary, FacingMode, RenderMethod, VarVector, Variance};
use ntw_sim::rng::CaRng;

/// Hard cap on live particles. The original bounds each emitter by `max_num_quads`
/// (`SCRIPTED_EFFECT_RENDERING_VARS/@max_num_quads`, 1000..2500 in the shipped file) and each group
/// by its entries; we keep one global cap so a massed volley cannot run the frame time away.
/// When the cap is hit the **oldest** particles are dropped (PROVISIONAL: the original recycles a
/// fixed pool instead).
pub const MAX_PARTICLES: usize = 24_000;

/// One live particle. Positions and velocities are metres and metres per second in the model's
/// frame (x, y up, z toward the viewer, i.e. **Bevy's** convention: the caller converts, since the
/// effect data itself is frame-agnostic).
#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    /// Which emitter made this, by name (for the draw layer's texture lookup).
    pub effect: String,
    /// World position, metres.
    pub position: [f32; 3],
    /// World velocity, m/s.
    pub velocity: [f32; 3],
    /// Seconds this particle lives.
    pub life: f32,
    /// Seconds since it was released.
    pub age: f32,
    /// Start colour, 0..1 (the file's 0..255 divided by 255).
    pub start_colour: [f32; 3],
    /// End colour, 0..1.
    pub end_colour: [f32; 3],
    /// Start alpha, 0..1.
    pub alpha: f32,
    /// Fade-in fraction of life, then fade-out fraction (CONFIRMED `FADE_INFO`).
    pub fade_in: f32,
    pub fade_out: f32,
    /// Size ramp: initial size (metres), size at `primary_at` of life, size at `secondary_at`.
    pub start_scale: [f32; 2],
    pub mid_scale: [f32; 2],
    pub end_scale: [f32; 2],
    pub primary_at: f32,
    pub secondary_at: f32,
    /// In-plane rotation in radians and its rate (rad/s).
    pub rotation: f32,
    pub rotation_speed: f32,
    /// Sprite-sheet frame index and the frame count (CONFIRMED `ANIMATION_INFO`).
    pub frame: u32,
    pub frames: u32,
    /// How the sprite is drawn: which of the emitter's eight texture slots, and how.
    pub slot: usize,
    pub render_method: RenderMethod,
    pub facing: FacingMode,
    /// Per-particle copy of the emitter's gravity, wind strength and thickness (the draw layer
    /// uses `thickness` for nothing yet; it is kept so nothing from the file is dropped).
    pub gravity: f32,
    pub wind: f32,
    /// Share of the speed lost per second (`MOVEMENT_INFO/@dampening_range`; INFERRED units, the
    /// attribute is only named in the file).
    pub dampening: f32,
    /// Release order, for the age tie-break.
    pub serial: u32,
}

impl Particle {
    /// Age as a fraction of life, clamped to `0..=1`.
    pub fn life_fraction(&self) -> f32 {
        if self.life <= 0.0 { 1.0 } else { (self.age / self.life).clamp(0.0, 1.0) }
    }

    /// The particle's opacity at its current age: `fade_in` and `fade_out` are fractions of life
    /// (CONFIRMED `FADE_INFO/@fadein_range_life_unary`, `@fadeout_range_life_unary`), so the
    /// sprite is invisible for the first `fade_in` and the last `fade_out` of its life.
    pub fn opacity(&self) -> f32 {
        let t = self.life_fraction();
        let up = if self.fade_in > 0.0 { (t / self.fade_in).min(1.0) } else { 1.0 };
        let down = if self.fade_out > 0.0 { ((1.0 - t) / self.fade_out).min(1.0) } else { 1.0 };
        self.alpha * up.max(0.0) * down.max(0.0)
    }

    /// The particle's colour at its current age, interpolated along the emitter's colour ramp.
    pub fn colour(&self) -> [f32; 3] {
        let t = self.life_fraction();
        [lerp(self.start_colour[0], self.end_colour[0], t),
         lerp(self.start_colour[1], self.end_colour[1], t),
         lerp(self.start_colour[2], self.end_colour[2], t)]
    }

    /// The particle's size in metres at its current age: width and height, following the
    /// two-segment ramp `SCALE_INFO` describes (an initial size and two target sizes reached at
    /// `primary_at` and `secondary_at` of the particle's life). So `1 m -> 2 m at half life ->
    /// 4 m at the end` is the ramp an expanding smoke puff uses. CONFIRMED attribute names; the
    /// linear interpolation between the waypoints is INFERRED (the file does not name a curve).
    pub fn size(&self) -> [f32; 2] {
        let t = self.life_fraction();
        let at1 = self.primary_at.clamp(0.0, 1.0);
        let at2 = self.secondary_at.clamp(0.0, 1.0);
        let waypoint = |i: usize| -> f32 {
            if t < at1 && at1 > f32::EPSILON {
                lerp(self.start_scale[i], self.mid_scale[i], t / at1)
            } else if t < at2 && at2 > at1 {
                lerp(self.mid_scale[i], self.end_scale[i], (t - at1) / (at2 - at1))
            } else if t < at2 {
                self.mid_scale[i]
            } else {
                self.end_scale[i]
            }
        };
        [waypoint(0).abs(), waypoint(1).abs()]
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The live particle world plus the deterministic generator that fills it.
#[derive(Debug)]
pub struct FxWorld {
    /// Live particles, oldest first (release order).
    pub particles: Vec<Particle>,
    /// The generator. Seeded from the battle seed, advanced only by [`Self::spawn`].
    pub rng: CaRng,
    /// Next release serial number.
    pub serial: u32,
    /// The scene wind, m/s. The `.environment` `WIND_SETTINGS` feeds this (see the notes: how the
    /// engine scales it by each emitter's `wind_range` is UNKNOWN, we multiply).
    pub wind: [f32; 3],
    /// The multiplier on every sprite's `SCALE_INFO` sizes for the next release. 1.0 is the shipped
    /// size; see [`crate::battle::fx_draw::burst_scale`] for where a real value comes from.
    ///
    /// For an air burst it is set from the fourth number of the shot's `projectiles_explosions` row
    /// (`shell_12lb` 10, `shell_32lb` 20, `shell_64lb` 25, `shrapnel_12lb` 6) over 10 m.
    ///
    /// **PROVISIONAL.** CONFIRMED: `AirExplosion_sml`, `_med` and `_lrg` are byte-identical groups in
    /// `effects\landbattle.xml` (`fx_probe::tests::the_three_air_burst_groups_are_one_group`,
    /// install), so the group name cannot size a burst. INFERRED: the fourth number is a radius in
    /// metres. **Not known at all:** whether the original sizes its air-burst sprites from that
    /// number, or from anything — it may be the gameplay blast radius only, and the identical groups
    /// may simply mean every burst looks the same. Using it here is our stand-in; the target is the
    /// exe's air-burst spawn call.
    pub size_scale: f32,
    /// How many particles have ever been released, for the HUD and the notes.
    pub released: u64,
    /// How many particles were dropped because of [`MAX_PARTICLES`].
    pub dropped: u64,
}

impl Default for FxWorld {
    fn default() -> Self {
        Self::new(0)
    }
}

impl FxWorld {
    /// An empty world whose generator starts from the battle's seed.
    pub fn new(seed: u32) -> Self {
        Self {
            particles: Vec::new(),
            rng: CaRng::new(seed),
            serial: 0,
            wind: [0.0; 3],
            size_scale: 1.0,
            released: 0,
            dropped: 0,
        }
    }

    /// Releases one [`Effect`] at `position` facing `direction`.
    ///
    /// Every random draw goes through [`FxWorld::rng`], so the same seed and the same call order
    /// give the same particles — replays match. The sampling rules themselves (how a `variance` is
    /// turned into a draw, how the release cone becomes a position) are PROVISIONAL; the ranges and
    /// their meanings are CONFIRMED.
    ///
    /// `strength` scales the emitter's **velocity only**. Sprite size is [`FxWorld::size_scale`], a
    /// separate multiplier, because the two come from different columns of different tables — and
    /// until round 5 `strength` was the *only* scale there was, which meant a 24-pounder's muzzle
    /// flash was drawn at exactly the size of a 6-pounder's. `size_scale_follows_the_explosion_row`
    /// is the kept test for that.
    pub fn spawn(
        &mut self,
        fx: &Effect,
        position: [f32; 3],
        direction: [f32; 3],
        strength: f32,
    ) -> usize {
        let before = self.particles.len();
        // A `conic_range_fan`/`conic_range_depth` pair is the emitter's cone half-angles
        // (CONFIRMED names; the units are INFERRED radians, capped so a zero-fan emitter cannot
        // be pushed sideways by a stray draw).
        let fan = fx.conic_fan.base.abs().min(std::f32::consts::PI);
        let depth = fx.conic_depth.base.abs().min(std::f32::consts::PI);
        // Make room for the whole release in one go. Dropping the oldest particle one at a time
        // (`Vec::remove(0)`) memmoves the entire pool per particle, which is gigabytes a tick once
        // a massed volley is over the cap.
        let want = self.particles.len() + fx.num_particles_per_point as usize;
        if want > MAX_PARTICLES {
            let excess = want - MAX_PARTICLES;
            self.particles.drain(..excess.min(self.particles.len()));
            self.dropped += excess as u64;
        }
        for _ in 0..fx.num_particles_per_point {
            let Some(mut p) = self.spawn_one(fx, position, direction, strength, fan, depth) else { continue };
            p.serial = self.serial;
            self.serial = self.serial.wrapping_add(1);
            self.released += 1;
            self.particles.push(p);
        }
        self.particles.len() - before
    }

    /// Releases a whole `SCRIPTED_EFFECT_GROUP`: every entry in the order the file lists them.
    pub fn spawn_group(
        &mut self,
        lib: &EffectLibrary,
        group: &str,
        position: [f32; 3],
        direction: [f32; 3],
        strength: f32,
    ) -> usize {
        let mut n = 0;
        for e in lib.group_effects(group) {
            n += self.spawn(e, position, direction, strength);
        }
        n
    }

    /// One particle from one emitter.
    fn spawn_one(
        &mut self,
        fx: &Effect,
        position: [f32; 3],
        direction: [f32; 3],
        strength: f32,
        fan: f32,
        depth: f32,
    ) -> Option<Particle> {
        // Life, velocity and opacity go through their emission modifiers, which are `NONE` in almost
        // every shipped row and therefore leave the value alone (CONFIRMED: 277 of the 283
        // lifetime modifiers are `NONE` with value 0.0 -- read as a multiplier they would give
        // every particle no life at all).
        let life = draw(&mut self.rng, Variance { base: fx.life_seconds(fx.life_range.base), var: fx.life_range.var });
        // A NaN life (a malformed row) is dropped like a zero one.
        if life.is_nan() || life <= 0.0 {
            return None;
        }
        // Release position: the emitter's `release_position_variation_range`, a box in its own
        // (x, y, z). INFERRED: x is across, y along and z up, all in metres; a component the file
        // did not write is not drawn at all.
        let off = fx.release_position_variation;
        let axis = |i: usize, rng: &mut CaRng| if (i as u8) < off.len { draw(rng, off.c[i]) } else { 0.0 };
        let px = position[0] + axis(0, &mut self.rng);
        let py = position[1] + axis(1, &mut self.rng);
        let pz = position[2] + axis(2, &mut self.rng);

        // Direction: the emitter's `dir_range_XYZ` turned towards the caller, plus the cone spread.
        let (speed, dir) = self.cone_direction(fx, direction, strength, fan, depth);

        // Colour ramp. `*_CHANNELS_LINKED` makes one draw drive all three channels (CONFIRMED).
        let colour = |c: [Variance; 3], linked: ChannelLink, rng: &mut CaRng| match linked {
            ChannelLink::Rgb => [draw(rng, c[0]) / 255.0; 3],
            ChannelLink::None => [draw(rng, c[0]) / 255.0, draw(rng, c[1]) / 255.0, draw(rng, c[2]) / 255.0],
        };
        let (start_ramp, end_ramp) = fx.colour_ramp(fx.start_colour, fx.end_colour);
        let start_colour = colour(start_ramp, fx.start_channels_linked, &mut self.rng);
        let end_colour = colour(end_ramp, fx.end_channels_linked, &mut self.rng);
        let alpha = draw(&mut self.rng, Variance { base: fx.opacity(fx.colour_alpha.base), var: fx.colour_alpha.var }) / 255.0;

        // The three `SCALE_INFO` sizes, each scaled by the world's `size_scale` — the shot's own burst
        // radius. Round 5: these were the raw file values, so every shell's air burst drew at the
        // same size whatever it weighed.
        let size = |v: VarVector, rng: &mut CaRng| -> [f32; 2] {
            let w = if v.len >= 2 { draw(rng, v.c[0]) } else { 1.0 };
            let h = if v.len >= 2 { draw(rng, v.c[1]) } else { w };
            [w.abs() * self.size_scale, h.abs() * self.size_scale]
        };
        let (initial, mid, end) = fx.scales(fx.initial_scale, fx.primary_target_scale, fx.secondary_target_scale);
        let (rotation_range, rotation_speed_range) = fx.rotation(fx.rotation, fx.rotations_per_second);
        let frames = fx.frames();
        Some(Particle {
            effect: fx.name.clone(),
            position: [px, py, pz],
            velocity: [dir[0] * speed, dir[1] * speed, dir[2] * speed],
            life,
            age: 0.0,
            start_colour,
            end_colour,
            alpha: alpha.clamp(0.0, 1.0),
            fade_in: fx.fade_in.base.clamp(0.0, 1.0),
            fade_out: fx.fade_out.base.clamp(0.0, 1.0),
            start_scale: size(initial, &mut self.rng),
            mid_scale: size(mid, &mut self.rng),
            end_scale: size(end, &mut self.rng),
            primary_at: fx.primary_scale_life.base.clamp(0.0, 1.0),
            secondary_at: fx.secondary_scale_life.base.clamp(0.0, 1.0),
            rotation: draw(&mut self.rng, rotation_range),
            rotation_speed: draw(&mut self.rng, rotation_speed_range),
            frame: (draw(&mut self.rng, fx.start_frame_variation).round().max(0.0) as u32) % frames.max(1),
            frames,
            slot: 0,
            render_method: fx.render_method,
            facing: fx.sprite_facing,
            gravity: fx.gravity.base,
            wind: fx.wind.base,
            dampening: fx.dampening.base.abs(),
            serial: self.serial,
        })
    }

    /// The initial velocity direction and speed, spread inside the emitter's cone and scaled by
    /// `strength` (the caller's scale, e.g. a bigger effect for a bigger gun).
    fn cone_direction(
        &mut self,
        fx: &Effect,
        direction: [f32; 3],
        strength: f32,
        fan: f32,
        depth: f32,
    ) -> (f32, [f32; 3]) {
        // The emitter's own axis, from `dir_range_XYZ`: each component is 0 or 1 in the shipped
        // file, so the vector is a signed axis choice (INFERRED).
        let axis = [fx.direction[0], fx.direction[1], fx.direction[2]];
        let speed = draw(&mut self.rng, Variance { base: fx.velocity(fx.velocity.base), var: fx.velocity.var }).abs() * strength.max(0.0);
        let base = normalise(axis);
        // Two independent spreads: one across the direction, one along it.
        let a = self.rng.float_range(-fan, fan);
        let b = self.rng.float_range(-depth, depth);
        let dir = spread(base, direction, a, b);
        (speed, dir)
    }

    /// Advances every particle by `dt` seconds of **battle** time and drops the finished ones.
    ///
    /// Battle time, not frame time: the model ticks at a fixed 0.1 s (W1 §12.2) and so do we, so
    /// the picture of a battle does not depend on the frame rate or the speed setting.
    pub fn advance(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.age += dt;
            if p.rotation_speed != 0.0 {
                p.rotation += p.rotation_speed * std::f32::consts::TAU * dt;
            }
            // `gravity_range` is signed: the shipped puff writes -9.81 for earth gravity, so it is
            // added straight to the vertical acceleration (CONFIRMED sign; the value's unit is
            // INFERRED m/s², which is the only one that makes the shipped numbers earth-like).
            p.velocity[1] += p.gravity * dt;
            // Wind: the scene wind scaled by the emitter's own `wind_range` (CONFIRMED the two
            // values exist; how the engine combines them is UNKNOWN, we multiply).
            p.velocity[0] += self.wind[0] * p.wind * dt;
            p.velocity[2] += self.wind[2] * p.wind * dt;
            // `dampening_range` is the share of speed lost per second (INFERRED units). **Review
            // fix:** the share used to be taken off once per *call*, i.e. per 0.1 s tick, which is ten
            // times the stated per-second rate and would change with the step. Compounded over `dt`
            // it is a per-second share whatever the step.
            let keep = (1.0 - p.dampening).max(0.0).powf(dt);
            p.velocity[0] *= keep;
            p.velocity[1] *= keep;
            p.velocity[2] *= keep;
            for i in 0..3 {
                p.position[i] += p.velocity[i] * dt;
            }
        }
        self.particles.retain(|p| p.age < p.life);
    }

    /// Releases the dust puff a moving unit owes, at most once per its `entity frequency`
    /// seconds of battle time.
    ///
    /// CONFIRMED: `effects\unit_dust_parameters.txt` gives one `entity frequency` per entity
    /// state (0.05 for infantry walking, 0.15 for infantry charging, ...). INFERRED: the number is
    /// the **interval between puffs** in seconds, which is the only reading that makes every
    /// shipped value (0.05..0.45) a sensible period for a footfall.
    ///
    /// `timers` is per-unit, keyed by unit id, and holds the seconds of battle time since that
    /// unit last puffed.
    ///
    /// **Review fix:** a step longer than the interval owes more than one puff — `infantry_walking`
    /// is 0.05 s against the 0.1 s model tick, so two. This released one and took one interval off
    /// the timer, so the timer grew by 0.05 s every tick for as long as the unit walked, without
    /// bound, and the shipped rate was halved. Every puff owed is now released (at most
    /// [`MAX_PUFFS_PER_STEP`], so a huge `dt` cannot loop away the frame), and what is left is less
    /// than one interval.
    #[allow(clippy::too_many_arguments)]
    pub fn dust(
        &mut self,
        lib: &EffectLibrary,
        params: &DustParameters,
        unit_id: u32,
        entity: &str,
        behaviour: &str,
        position: [f32; 3],
        facing: [f32; 3],
        timers: &mut HashMap<u32, f32>,
        dt: f32,
    ) -> bool {
        let Some((group, frequency)) = dust_puff(params, entity, behaviour) else { return false };
        if frequency <= 0.0 {
            return false;
        }
        // Time since the last puff. The comparison carries a small epsilon so that a frequency
        // that is an exact multiple of the tick (0.05 s at a 0.05 s step) is not lost to
        // `f32` rounding and puffs only every other tick.
        let elapsed = timers.entry(unit_id).or_insert(0.0);
        *elapsed += dt;
        let mut owed = 0usize;
        while *elapsed + TIMER_EPSILON >= frequency && owed < MAX_PUFFS_PER_STEP {
            *elapsed -= frequency;
            owed += 1;
        }
        // Past the cap the rest is dropped rather than carried, so the timer stays bounded.
        if *elapsed + TIMER_EPSILON >= frequency {
            *elapsed %= frequency;
        }
        let mut released = 0;
        for _ in 0..owed {
            released += self.spawn_group(lib, group, position, facing, 1.0);
        }
        released > 0
    }
}

/// The most dust puffs one unit releases in one step. Two cover the shipped table at the 0.1 s tick
/// (the shortest interval is 0.05 s); the cap only stops a pathological `dt` or frequency from
/// looping. Ours, not the original's.
const MAX_PUFFS_PER_STEP: usize = 4;

/// Slack in the dust timer comparison, so an interval that is an exact multiple of the tick is not
/// lost to `f32` rounding and puffs only every other tick. Well under one 0.1 s tick.
const TIMER_EPSILON: f32 = 1e-4;

/// One draw from a `variance(base,var)` range: the base plus a spread of ±`var` (CONFIRMED the
/// file's shape; INFERRED that the spread is uniform and symmetric, and that the engine draws
/// every attribute of every particle fresh — the file does not say).
pub fn draw(rng: &mut CaRng, v: Variance) -> f32 {
    if v.var <= 0.0 { v.base } else { v.base + rng.float_range(-v.var, v.var) }
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l <= f32::EPSILON { [0.0, 1.0, 0.0] } else { [v[0] / l, v[1] / l, v[2] / l] }
}

/// Rotates `base` towards `facing` by the two cone spreads (PROVISIONAL: two independent small
/// rotations about two perpendicular axes, built from the normalised inputs).
fn spread(base: [f32; 3], facing: [f32; 3], across: f32, along: f32) -> [f32; 3] {
    let f = normalise(facing);
    // Any perpendicular pair around the base direction.
    let up = if base[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let right = normalise(cross(up, base));
    let up2 = cross(base, right);
    let (sa, ca) = across.sin_cos();
    let (sb, cb) = along.sin_cos();
    let r = [
        base[0] * cb + (right[0] * sa + up2[0] * ca * sb),
        base[1] * cb + (right[1] * sa + up2[1] * ca * sb),
        base[2] * cb + (right[2] * sa + up2[2] * ca * sb),
    ];
    // Bias towards the caller's facing so a group played at a unit's muzzle blows the right way.
    normalise([r[0] * 0.5 + f[0] * 0.5, r[1] * 0.5 + f[1] * 0.5, r[2] * 0.5 + f[2] * 0.5])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Which effect group a shot's firing muzzle plays.
///
/// **CONFIRMED, from the shipped data.** `db\projectiles` column 31 (`fire_effect`, read by
/// [`ntw_data::schemas::Projectile`]) is the group itself: 120 of the 144 shipped rows set it, and
/// every set value is a `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml`. The install test
/// `ntw_data/tests/effects_data.rs::projectiles_name_their_own_fire_group_and_trail` re-reads them
/// and prints the tally — `CannonFire` 56, `LandGunFire_canister` 13, `MusketFire` 8,
/// `LandGunFire` 7, `LandGunFire_howitzer` 7, `LandGunFire_large` 9, `LandGunFire_mortar` 8,
/// `LandGunFire_small` 7, `fougasse_default` 2, `rifleFire` 2, `pistolFire` 1.
///
/// This replaces the round-1 PROVISIONAL name-matching table, which guessed from
/// `projectiles.weapon_family` and the calibre and got several guns wrong (`cannon_18_pounder` is
/// plain `LandGunFire`, not `_large`, and a 4-pounder howitzer is `_howitzer`, not `_small`).
///
/// The remaining UNKNOWN is the **position**: the group is played at a muzzle node, and the
/// per-gun `gun_type_to_projectiles.muzzle_flash` names (`cannon_12_pounder_muzzle_flash`) are not
/// in any shipped model or animation file — a whole-pack negative in the install test — so the node
/// lookup is exe-side. We still place it ourselves; see [`crate::battle::fx_draw::MUZZLE_HEIGHT`].
///
/// `weapon_family` and `calibre` are kept as the **fallback** for a row with no `fire_effect`
/// (the 24 rows without one are arrows, grenades and fragments, which do not fire a group) and for
/// a modded table that leaves the column empty.
pub fn fire_group<'a>(db: &'a ntw_data::GameDatabase, projectile: &'a ntw_data::schemas::Projectile) -> &'a str {
    // The group name from the data, borrowed. The caller checks it against the loaded library, so
    // a mod that adds the group still works and a stale name still falls back.
    if let Some(group) = db.fire_effect(projectile)
        && !group.trim().is_empty()
    {
        return group;
    }
    fire_group_by_name(projectile.weapon_family.as_deref().unwrap_or("none"), &projectile.calibre)
}

/// The round-1 fallback: guess the group from the weapon family and calibre.
///
/// **PROVISIONAL, kept only as a fallback** for a `projectiles` row with no `fire_effect` (see
/// [`fire_group`] for the shipped column). Note it disagrees with the data in several places, which
/// is why it is no longer the primary path: `cannon_18_pounder` is plain `LandGunFire` in the
/// table, and `LandGunFire_canister` is a group of its own that no calibre rule reaches.
pub fn fire_group_by_name(weapon_family: &str, calibre: &str) -> &'static str {
    let family = weapon_family.to_ascii_lowercase();
    if family.contains("pistol") {
        return "pistolFire";
    }
    if family.contains("rifle") {
        return "rifleFire";
    }
    if family.contains("howitzer") {
        return "LandGunFire_howitzer";
    }
    if family.contains("mortar") {
        return "LandGunFire_mortar";
    }
    if family.contains("carronade") {
        return "LandGunFire_small";
    }
    if family.contains("cannon") || family.contains("gun") {
        // PROVISIONAL: the data has no calibre rule at all (see [`fire_group`]), so this split is
        // only a guess for a row with no group of its own.
        return if gun_pounder(calibre) <= 9.0 { "LandGunFire_small" } else { "LandGunFire_large" };
    }
    "MusketFire"
}

/// The calibre's number of pounds of shot, from a `projectiles.calibre` name such as
/// `cannon_12_pounder` or `fort_9_pounder`. 0 when the name does not say.
pub fn gun_pounder(calibre: &str) -> f32 {
    let lower = calibre.to_ascii_lowercase();
    let Some(at) = lower.find("_pounder") else { return 0.0 };
    let digits: String = lower[..at].chars().rev().take_while(char::is_ascii_digit).collect();
    digits.chars().rev().collect::<String>().parse().unwrap_or(0.0)
}

/// What a shot plays where it lands, read out of the shipped tables.
///
/// All of these names are `SCRIPTED_EFFECT_GROUP` names of `effects\landbattle.xml`, CONFIRMED
/// twice on the install: `ntw_formats/tests/effects_install.rs`
/// (`projectile_effect_names_are_land_battle_groups`) and
/// `ntw_data/tests/effects_data.rs` (`explosion_rows_name_the_effect_groups_they_play`).
///
/// **Which** group is now **CONFIRMED for the air burst and the ground scorch**: they are
/// `projectiles.explosion`'s row in `db\projectiles_explosions`, read by
/// [`ntw_formats::projectile_fx::ExplosionTable`] and reached through
/// [`ntw_data::GameDatabase::explosion_effects`]. The table chooses the size **per row**, which no
/// calibre rule reproduces — `shell_12lb` plays `AirExplosion_med` while `shrapnel_12lb` plays
/// `AirExplosion_sml`, and `shell_percussive_12lb` (a 12-pounder) plays `AirExplosion_sml`. So the
/// round-1 6/12-pound thresholds are gone as the primary path.
///
/// Still **UNKNOWN**: which of `projectile_impacts`' group columns is which surface (that table's
/// 8 rows hold 15-18 group columns each and their order is not decoded), so [`impact_groups`]
/// returns them in file order and the caller picks. That is also why [`BLOOD_GROUP`] is INFERRED:
/// `projectile_impacts`' `musket_ball` row is the one that names `blood_gen`, but which of its
/// columns is "on a man" is not known.
pub mod impact {
    use ntw_data::schemas::Projectile;
    use ntw_data::GameDatabase;

    /// The group a musket ball (or any small-arm shot) plays on a man it kills.
    ///
    /// **INFERRED**: `projectile_impacts`' `musket_ball` row is the only one that names
    /// `blood_gen` (the shell rows name `blood_spray`), and a musket ball is what kills men. Target:
    /// the surface column of that row which means "a man".
    pub const BLOOD_GROUP: &str = "blood_gen";
    /// The three air bursts, smallest first. All CONFIRMED group names.
    pub const AIR_EXPLOSIONS: [&str; 3] = ["AirExplosion_sml", "AirExplosion_med", "AirExplosion_lrg"];
    /// The two generic ground impacts, smallest first. All CONFIRMED group names.
    pub const GROUND_IMPACTS: [&str; 2] = ["Cannon_Groundimpact_gen_sml", "Cannon_Groundimpact_gen_med"];
    /// The burst a shot plays **in the air**, straight from its `projectiles_explosions` row.
    ///
    /// **CONFIRMED** that this is where the group comes from (`effects_data.rs::
    /// explosion_rows_name_the_effect_groups_they_play`). The three `AirExplosion_*` groups are
    /// byte-identical, so the name does not size the burst; the sprite size we apply is the
    /// PROVISIONAL [`crate::battle::fx_draw::burst_scale`]. Falls back to [`air_explosion`] for a
    /// row with no air group.
    pub fn air_burst<'a>(db: &'a GameDatabase, projectile: &'a Projectile) -> Option<&'a str> {
        db.explosion_effects(projectile)?.air.as_deref()
    }
    /// The scorch a shot leaves on the ground, straight from its `projectiles_explosions` row.
    /// `None` on the 25 of 35 rows that have no ground scorch (the carcass, grenade and quicklime
    /// families). Falls back to [`ground_impact`] when the row itself is missing.
    pub fn ground_scorch<'a>(db: &'a GameDatabase, projectile: &'a Projectile) -> Option<&'a str> {
        db.explosion_effects(projectile)?.ground.as_deref()
    }
    /// Every impact group the shot's ball class plays, in the `projectile_impacts` file order.
    /// **UNKNOWN** which surface each column is, so this is the whole row, not a pick — the
    /// water, ship, sail, building, tree and blood groups of a `default_ball` all come back.
    /// Not called by the draw layer yet (that is what the column order is blocking); it is here so
    /// the row is reachable and testable.
    #[allow(dead_code)]
    pub fn impact_groups<'a>(db: &'a GameDatabase, projectile: &'a Projectile) -> Vec<&'a str> {
        db.impact_effects(projectile).map(|r| r.groups.iter().flatten().map(String::as_str).collect()).unwrap_or_default()
    }

    /// The air burst of a shot of `pounder` pounds of shot, when the data has no row.
    ///
    /// **PROVISIONAL**: thresholds from the shipped rows, 6 / 12 pounds. Small arms pass `0.0` and
    /// get the smallest burst. Prefer [`air_burst`], which reads the shipped row.
    pub fn air_explosion(pounder: f32) -> &'static str {
        if pounder > 12.0 {
            AIR_EXPLOSIONS[2]
        } else if pounder > 6.0 {
            AIR_EXPLOSIONS[1]
        } else {
            AIR_EXPLOSIONS[0]
        }
    }
    /// The scorch a shot leaves on hard ground, when the data has no row. **PROVISIONAL**: the same
    /// thresholds. Prefer [`ground_scorch`].
    pub fn ground_impact(pounder: f32) -> &'static str {
        if pounder > 6.0 { GROUND_IMPACTS[1] } else { GROUND_IMPACTS[0] }
    }
}

pub use impact::{air_burst, ground_scorch, BLOOD_GROUP};
pub use impact::{air_explosion, ground_impact};

/// The `// type` name of an entity in `effects\unit_dust_parameters.txt`, from the model's unit.
/// **CONFIRMED** the file's 15 names (four families x walking/running/charging/melee, artillery
/// without charging) and that the family is decided by whether the unit rides or drives; the
/// family's exact test in the original is INFERRED.
pub fn dust_entity_name(is_cavalry: bool, is_artillery: bool, behaviour: &str) -> Option<String> {
    let family = if is_cavalry { "cavalry" } else if is_artillery { "artillery" } else { "infantry" };
    let state = match behaviour {
        "melee" => "melee",
        "charging" => "charging",
        "running" => "running",
        _ => "walking",
    };
    Some(format!("{family}_{state}"))
}

/// Which dust group an entity puffs while moving, and how often it puffs.
///
/// The group names are CONFIRMED group names in `landbattle.xml`: `infantry_walk_dust`,
/// `cavalry_walk_dust`, `infantry_combat_dust`, plus `move_foot_*` / `move_fieldgun_*` / `move_*`
/// per ground type. **Which** group the engine picks for a given entity and ground type is
/// INFERRED from the group's own name (the file names them `generic_dust_sml`, `move_foot_mud_small`
/// and so on), so this is the stand-in mapping and is tagged PROVISIONAL. The **frequency** is
/// CONFIRMED: it is the entity's own `entity frequency` from the shipped table.
pub fn dust_puff(params: &DustParameters, entity: &str, behaviour: &str) -> Option<(&'static str, f32)> {
    let group = match dust_family(entity) {
        Some("cavalry") => match behaviour {
            "melee" => "infantry_combat_dust",
            _ => "cavalry_walk_dust",
        },
        Some(_) => match behaviour {
            "melee" => "infantry_combat_dust",
            _ => "infantry_walk_dust",
        },
        None => return None,
    };
    // CONFIRMED: the frequency is the entity's own `entity frequency` from the shipped table.
    Some((group, params.frequency(entity)?))
}

/// Which family prefix an `entity frequency` name belongs to, or `None` for a name the shipped
/// table does not have (a clean negative for the naval families: there is no `ship_*` dust row).
fn dust_family(entity: &str) -> Option<&'static str> {
    ["infantry", "cavalry", "elephants", "artillery"].into_iter().find(|f| entity.starts_with(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::effects::{EffectLibrary, RenderMethod};

    /// A library with one emitter and one group, built from the module's own sample document.
    fn lib() -> EffectLibrary {
        EffectLibrary::read(TEST_DOC.as_bytes()).expect("the fx test document parses")
    }

    const TEST_DOC: &str = r#"
<EFFECTS_MANAGER><SCRIPTED_EFFECT_INFO_LIST>
<SCRIPTED_EFFECT_INFO name='puff' num_particles_per_point='3' gravity_range='variance(-9.810000,0.000000)'
  wind_range='variance(1.000000,0.000000)'>
 <SCRIPTED_EFFECT_EMISSION_CONTROL emission_type='EMISSION_TYPE_POINT' emission_time_range_seconds='variance(1.0,0.0)'>
  <RELEASE_INFO release_type='RELEASE_TYPE_NONE' release_position_variation_range='vector(variance(0.0,1.0),variance(0.0,2.0))'/>
  <EMISSION_MODIFIERS>
   <EMISSION_MODIFIER_VELOCITY value='1.000000'/><EMISSION_MODIFIER_LIFETIME value='1.000000'/>
   <EMISSION_MODIFIER_OPACITY value='1.000000'/>
  </EMISSION_MODIFIERS>
 </SCRIPTED_EFFECT_EMISSION_CONTROL>
 <SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES life_range='variance(2.000000,0.000000)'>
  <MOVEMENT_INFO velocity_range='variance(5.000000,0.000000)' dampening_range='variance(0.100000,0.000000)'
   dir_range_XYZ='vector(0.000000,1.000000,0.000000)' conic_range_fan='variance(0.000000,0.000000)' conic_range_depth='variance(0.000000,0.000000)'/>
  <ANIMATION_INFO total_frames='variance(4,0)' cell_width='128' cell_height='128' start_frame_variation='variance(0,0)'/>
  <COLOUR_INFO start_linked_channels='NO_CHANNELS_LINKED' start_colour_range_r='variance(255,0)' start_colour_range_g='variance(128,0)'
   start_colour_range_b='variance(0,0)' end_linked_channels='NO_CHANNELS_LINKED' end_colour_range_r='variance(0,0)'
   end_colour_range_g='variance(0,0)' end_colour_range_b='variance(0,0)' colour_range_a='variance(255,0)'/>
  <FADE_INFO fadein_range_life_unary='variance(0.000000,0.000000)' fadeout_range_life_unary='variance(1.000000,0.000000)'/>
  <SCALE_INFO initial_scale_range_metres='vector(variance(1.000000,0.000000),variance(1.000000,0.000000))'
   primary_scale_life_unary_range='variance(0.500000,0.000000)' primary_target_scale_range_metres='vector(variance(2.000000,0.000000),variance(2.000000,0.000000))'
   secondary_scale_life_unary_range='variance(1.000000,0.000000)' secondary_target_scale_range_metres='vector(variance(4.000000,0.000000),variance(4.000000,0.000000))'/>
  <ROTATION_INFO rotation_range='variance(0.000000,0.000000)' rotations_per_second_range='variance(0.000000,0.000000)'/>
 </SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES>
 <SCRIPTED_EFFECT_RENDERING_VARS texture_1='effects\textures\emp_smoke_diffuse.dds' texture_2='x.dds' fx='particle.fx'
  render_method='RENDER_METHOD_ALPHA' sprite_facing_mode='CAMERA_FACING' max_effects='50' max_num_quads='1000'/>
</SCRIPTED_EFFECT_INFO>
<SCRIPTED_EFFECT_INFO name='flash' num_particles_per_point='1' gravity_range='variance(0.0,0.0)'>
 <SCRIPTED_EFFECT_EMISSION_CONTROL emission_type='EMISSION_TYPE_POINT'>
  <RELEASE_INFO release_type='RELEASE_TYPE_NONE'/>
  <EMISSION_MODIFIERS><EMISSION_MODIFIER_VELOCITY value='1.0'/><EMISSION_MODIFIER_LIFETIME value='1.0'/><EMISSION_MODIFIER_OPACITY value='1.0'/></EMISSION_MODIFIERS>
 </SCRIPTED_EFFECT_EMISSION_CONTROL>
 <SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES life_range='variance(0.200000,0.000000)'>
  <MOVEMENT_INFO velocity_range='variance(0.0,0.0)' dir_range_XYZ='vector(0.0,1.0,0.0)'/>
  <ANIMATION_INFO total_frames='variance(1,0)' cell_width='32' cell_height='32'/>
  <COLOUR_INFO start_linked_channels='RED_GREEN_BLUE_CHANNELS_LINKED' start_colour_range_r='variance(255,0)'
   start_colour_range_g='variance(0,0)' start_colour_range_b='variance(0,0)' end_linked_channels='RED_GREEN_BLUE_CHANNELS_LINKED'
   end_colour_range_r='variance(255,0)' end_colour_range_g='variance(0,0)' end_colour_range_b='variance(0,0)' colour_range_a='variance(255,0)'/>
  <FADE_INFO fadein_range_life_unary='variance(0.0,0.0)' fadeout_range_life_unary='variance(1.0,0.0)'/>
  <SCALE_INFO initial_scale_range_metres='vector(variance(0.5,0.0),variance(0.5,0.0))'
   primary_scale_life_unary_range='variance(0.0,0.0)' primary_target_scale_range_metres='vector(variance(0.5,0.0),variance(0.5,0.0))'
   secondary_scale_life_unary_range='variance(1.0,0.0)' secondary_target_scale_range_metres='vector(variance(0.5,0.0),variance(0.5,0.0))'/>
  <ROTATION_INFO rotation_range='variance(0.0,0.0)' rotations_per_second_range='variance(0.0,0.0)'/>
 </SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES>
 <SCRIPTED_EFFECT_RENDERING_VARS texture_1='effects\textures\flash_diffuse.dds' fx='particle.fx'
  render_method='RENDER_METHOD_ADDITIVE' sprite_facing_mode='CAMERA_FACING' max_effects='50' max_num_quads='1000'/>
</SCRIPTED_EFFECT_INFO>
</SCRIPTED_EFFECT_INFO_LIST>
<SCRIPTED_EFFECT_GROUP_INFO>
<SCRIPTED_EFFECT_GROUP name='Fire'><EFFECT_GROUP_AMBIENT_SETTINGS spawn_interval='100.0' cell_size='10.0' cell_count='1.0' wind_offset='0.0'/>
<SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/><SCRIPTED_EFFECT_GROUP_ENTRY name='flash' effect='flash'/>
</SCRIPTED_EFFECT_GROUP>
</SCRIPTED_EFFECT_GROUP_INFO></EFFECTS_MANAGER>"#;

    #[test]
    fn spawn_releases_num_particles_per_point() {
        let lib = lib();
        let mut w = FxWorld::new(7);
        let n = w.spawn_group(&lib, "Fire", [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], 1.0);
        assert_eq!(n, 4, "3 puffs + 1 flash");
        assert_eq!(w.particles.len(), 4);
        assert_eq!(w.released, 4);
        assert_eq!(w.serial, 4, "every release gets its own serial");
        // Every release has its own serial, oldest first.
        let serials: Vec<u32> = w.particles.iter().map(|p| p.serial).collect();
        assert_eq!(serials, vec![0, 1, 2, 3]);
        assert_eq!(w.particles.iter().filter(|p| p.effect == "puff").count(), 3);
        assert_eq!(w.particles.iter().filter(|p| p.effect == "flash").count(), 1);
    }

    #[test]
    fn same_seed_same_particles() {
        let lib = lib();
        let mut a = FxWorld::new(1234);
        let mut b = FxWorld::new(1234);
        a.spawn_group(&lib, "Fire", [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], 1.0);
        b.spawn_group(&lib, "Fire", [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], 1.0);
        assert_eq!(a.particles, b.particles);
        // A different seed must give a different picture.
        let mut c = FxWorld::new(1235);
        c.spawn_group(&lib, "Fire", [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], 1.0);
        assert_ne!(a.particles, c.particles);
    }

    #[test]
    fn gravity_and_dampening_change_the_motion_and_things_expire() {
        let lib = lib();
        let mut w = FxWorld::new(3);
        w.spawn_group(&lib, "Fire", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let before: Vec<[f32; 3]> = w.particles.iter().map(|p| p.velocity).collect();
        let released: Vec<(u32, [f32; 3])> = w.particles.iter().map(|p| (p.serial, p.position)).collect();
        for _ in 0..5 {
            w.advance(0.1);
        }
        // The puff leaves upwards (its `dir_range_XYZ` is +y), so gravity has to show up as a
        // *reduced* upward speed, and dampening as a further reduction.
        let puff = w.particles.iter().find(|p| p.effect == "puff").unwrap();
        assert_eq!(puff.gravity, -9.81);
        assert!((puff.dampening - 0.1).abs() < 1e-6);
        assert!(puff.velocity[1] < before[0][1], "upward speed fell from {} to {}", before[0][1], puff.velocity[1]);
        // Dampening is a share per **second**, whatever the step: ten 0.1 s steps and one 1.0 s step
        // of a gravity-free, wind-free particle keep the same speed (0.9 of it for `dampening` 0.1).
        let mut fine = puff.clone();
        let mut coarse = puff.clone();
        for p in [&mut fine, &mut coarse] {
            p.gravity = 0.0;
            p.velocity = [0.0, 10.0, 0.0];
            p.age = 0.0;
            p.life = 100.0;
        }
        let mut a = FxWorld::new(0);
        a.particles.push(fine);
        for _ in 0..10 {
            a.advance(0.1);
        }
        let mut b = FxWorld::new(0);
        b.particles.push(coarse);
        b.advance(1.0);
        assert!((a.particles[0].velocity[1] - 9.0).abs() < 1e-3, "10 x 0.1 s: {}", a.particles[0].velocity[1]);
        assert!((b.particles[0].velocity[1] - 9.0).abs() < 1e-3, "1 x 1.0 s: {}", b.particles[0].velocity[1]);
        // Every particle moved. **Round 4:** this was `iter().zip(&before).any(..)`, and the
        // predicate `p.position != [0.0; 3] && p.velocity != *v` is true of any particle that moved
        // by any amount, so `any` was satisfied by the one short-lived flash alone — the puff, which
        // is what the rest of this test measures, could have been frozen and it still passed. Now
        // every particle that is still alive has to have moved from where it was released. Keyed on
        // the serial, not zipped by index, because the flash dies partway through and would shift
        // every later particle by one.
        for p in &w.particles {
            let start = released.iter().find(|(s, _)| *s == p.serial).map(|(_, v)| *v).expect("a live particle was never released");
            assert!(p.position != start, "particle {} never moved", p.serial);
        }
        assert_eq!(w.particles.len(), 3, "the three puffs are the ones left, and all of them moved");
        // The flash lives 0.2 s, so five 0.1 s steps kill it.
        assert!(w.particles.iter().all(|p| p.effect != "flash"));
    }

    #[test]
    fn wind_pushes_particles_downwind() {
        let lib = lib();
        let mut still = FxWorld::new(21);
        let mut windy = FxWorld::new(21);
        still.spawn(&lib.effects["puff"], [0.0; 3], [0.0, 1.0, 0.0], 1.0);
        windy.spawn(&lib.effects["puff"], [0.0; 3], [0.0, 1.0, 0.0], 1.0);
        windy.wind = [4.0, 0.0, 0.0];
        for _ in 0..10 {
            still.advance(0.1);
            windy.advance(0.1);
        }
        assert!(windy.particles[0].position[0] > still.particles[0].position[0] + 1.0);
    }

    #[test]
    fn everything_expires_eventually() {
        let lib = lib();
        // **Round 4.** This used to advance 4 s and assert the world was empty, which is also what
        // an `advance` that deleted every particle on the first tick — or a `life` read as 0 —
        // would produce. So it pinned "they die", never "they lived". The puff's life is 2.0 s from
        // `life_range='variance(2.000000,0.000000)'`, so it must still be there at 1 s and gone by 3.
        let mut w = FxWorld::new(9);
        w.spawn_group(&lib, "Fire", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        assert_eq!(w.particles.len(), 4, "3 puffs + 1 flash");
        for _ in 0..10 {
            w.advance(0.1);
        }
        assert_eq!(w.particles.len(), 3, "the flash (0.2 s) is gone but the puffs (2.0 s) are not");
        assert!(w.particles.iter().all(|p| p.effect == "puff"), "only puffs are left at 1 s");
        for _ in 0..20 {
            w.advance(0.1);
        }
        assert!(w.particles.is_empty(), "left {} particles", w.particles.len());
    }

    #[test]
    fn fade_and_size_follow_the_file_ramp() {
        let lib = lib();
        let mut w = FxWorld::new(5);
        w.spawn_group(&lib, "Fire", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let puff = w.particles.iter().find(|p| p.effect == "puff").unwrap().clone();
        assert_eq!(puff.life, 2.0);
        assert_eq!(puff.fade_out, 1.0, "a 1.0 fade-out means invisible over the whole life");
        assert!((puff.opacity() - 1.0).abs() < 1e-6, "a fade-out covering the whole life starts opaque");
        // start 1 -> 2 at half life -> 4 at the end.
        assert_eq!(puff.size(), [1.0, 1.0]);
        let mut p = puff.clone();
        p.age = 1.0;
        assert_eq!(p.size()[0], 2.0, "mid-size at half life");
        p.age = 2.0;
        assert_eq!(p.size()[0], 4.0, "end size at full life");
        // Colour ramps red (1,0,0) -> black.
        let mut p = puff.clone();
        p.age = 0.0;
        assert_eq!(p.colour(), [1.0, 128.0 / 255.0, 0.0]);
        p.age = 1.0;
        assert_eq!(p.colour()[0], 0.5);
    }

    #[test]
    fn linked_channels_carry_one_value() {
        let lib = lib();
        let mut w = FxWorld::new(11);
        w.spawn_group(&lib, "Fire", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        let flash = w.particles.iter().find(|p| p.effect == "flash").unwrap();
        assert_eq!(flash.start_colour, [1.0, 1.0, 1.0], "RGB linked: one draw, three channels");
        assert_eq!(flash.render_method, RenderMethod::Additive);
        assert_eq!(flash.frames, 1);
    }

    #[test]
    fn strength_scales_the_speed() {
        let lib = lib();
        let mut a = FxWorld::new(2);
        let mut b = FxWorld::new(2);
        a.spawn(&lib.effects["puff"], [0.0; 3], [0.0, 1.0, 0.0], 1.0);
        b.spawn(&lib.effects["puff"], [0.0; 3], [0.0, 1.0, 0.0], 2.0);
        let speed = |w: &FxWorld| w.particles[0].velocity[1];
        assert!((speed(&b) - 2.0 * speed(&a)).abs() < 1e-4, "{} vs {}", speed(&b), speed(&a));
    }

    #[test]
    fn the_cap_drops_the_oldest_and_never_exceeds_it() {
        let lib = lib();
        let mut w = FxWorld::new(1);
        // Fill past the cap in small batches so the recycle path runs.
        for _ in 0..(MAX_PARTICLES / 3 + 10) {
            w.spawn(&lib.effects["puff"], [0.0; 3], [0.0, 1.0, 0.0], 1.0);
            assert!(w.particles.len() <= MAX_PARTICLES, "{} after one release", w.particles.len());
        }
        assert_eq!(w.particles.len(), MAX_PARTICLES);
        assert!(w.dropped > 0);
        // The oldest go, not the newest: the pool still holds the last particles released, and the
        // serials stay in release order.
        let last = w.particles.last().map(|p| p.serial).unwrap_or(0);
        assert_eq!(last, w.serial - 1, "the newest particle is still in the pool");
        assert!(w.particles.first().is_some_and(|p| p.serial > 0), "the first releases were dropped");
        assert!(w.particles.windows(2).all(|w| w[0].serial < w[1].serial), "serials stay in order");
    }

    /// The shipped `unit_dust_parameters.txt`, verbatim, so the frequency tests use real numbers.
    fn dust_params() -> DustParameters {
        DustParameters::parse(
            b"version 1.0\r\n\r\n// type\t\t\t\tentity frequency\r\ninfantry_walking\t0.05\r\ninfantry_running\t0.10\r\n\
              infantry_charging\t0.15\r\ninfantry_melee\t\t0.10\r\ncavalry_walking\t\t0.05\r\ncavalry_running\t\t0.15\r\n\
              cavalry_charging\t0.30\r\ncavalry_melee\t\t0.10\r\nelephants_walking\t0.15\r\nelephants_running\t0.30\r\n\
              elephants_charging\t0.45\r\nelephants_melee\t\t0.10\r\nartillery_walking\t0.1\r\nartillery_running\t0.2\r\n\
              artillery_melee\t\t0.2",
        )
    }

    #[test]
    fn dust_entity_names_follow_the_shipped_table() {
        let p = dust_params();
        assert_eq!(dust_entity_name(false, false, "running").as_deref(), Some("infantry_running"));
        assert_eq!(dust_entity_name(true, false, "charging").as_deref(), Some("cavalry_charging"));
        assert_eq!(dust_entity_name(false, true, "walking").as_deref(), Some("artillery_walking"));
        assert_eq!(p.frequency("infantry_running"), Some(0.10));
        // Clean negative: the shipped table has no `artillery_charging` row.
        assert_eq!(p.frequency("artillery_charging"), None);
        assert_eq!(dust_puff(&p, "artillery_charging", "charging"), None);
    }

    #[test]
    fn dust_puffs_carry_the_shipped_frequency() {
        let p = dust_params();
        assert_eq!(dust_puff(&p, "infantry_walking", "walking"), Some(("infantry_walk_dust", 0.05)));
        assert_eq!(dust_puff(&p, "cavalry_running", "running"), Some(("cavalry_walk_dust", 0.15)));
        assert_eq!(dust_puff(&p, "infantry_melee", "melee"), Some(("infantry_combat_dust", 0.10)));
        assert_eq!(dust_puff(&p, "artillery_running", "running"), Some(("infantry_walk_dust", 0.2)));
        // An entity the shipped table does not know puffs nothing.
        assert_eq!(dust_puff(&p, "ship_sailing", "running"), None);
    }

    /// The fallback only. The real mapping is `projectiles` column 31, read by
    /// [`fire_group`] from the shipped data; this is for a row with no group of its own.
    #[test]
    fn fire_groups_fall_back_to_the_weapon_family() {
        assert_eq!(fire_group_by_name("musket_flintlock", "musket_ball"), "MusketFire");
        assert_eq!(fire_group_by_name("musket_breech_loader", "musket_ball"), "MusketFire");
        assert_eq!(fire_group_by_name("pistol", "pistol_ball"), "pistolFire");
        assert_eq!(fire_group_by_name("rifle", "rifle_ball"), "rifleFire");
        assert_eq!(fire_group_by_name("cannon", "cannon_12_pounder"), "LandGunFire_large");
        assert_eq!(fire_group_by_name("cannon", "cannon_6_pounder"), "LandGunFire_small");
        assert_eq!(fire_group_by_name("howitzer", "howitzer_5_In"), "LandGunFire_howitzer");
        assert_eq!(fire_group_by_name("mortar", "mortar_8"), "LandGunFire_mortar");
        assert_eq!(fire_group_by_name("carronade", "naval_carronade_32_pounder"), "LandGunFire_small");
        // A family we do not know falls back to the musket group rather than drawing nothing.
        assert_eq!(fire_group_by_name("none", ""), "MusketFire");
    }

    #[test]
    fn the_calibre_parser_reads_pounder_names() {
        assert_eq!(gun_pounder("cannon_12_pounder"), 12.0);
        assert_eq!(gun_pounder("naval_carronade_64_pounder"), 64.0);
        assert_eq!(gun_pounder("musket_ball"), 0.0);
        assert_eq!(gun_pounder(""), 0.0);
    }

    /// The fallback impact rules, which are only used when `projectiles_explosions` has no row for
    /// the shot. The shipped rows themselves are read by [`impact::air_burst`] /
    /// [`impact::ground_scorch`] and pinned by `ntw_data/tests/effects_data.rs` on the install;
    /// what is left here is the PROVISIONAL stand-in and the group-name list.
    #[test]
    fn impact_fallbacks_and_group_names() {
        assert_eq!(BLOOD_GROUP, "blood_gen");
        assert_eq!(impact::AIR_EXPLOSIONS, ["AirExplosion_sml", "AirExplosion_med", "AirExplosion_lrg"]);
        assert_eq!(impact::GROUND_IMPACTS, ["Cannon_Groundimpact_gen_sml", "Cannon_Groundimpact_gen_med"]);
        // Small arms and light shot get the smallest burst; a 12-pounder the middle; a 24-pounder
        // the largest. A musket's calibre says no pounds, so it is the smallest.
        assert_eq!(air_explosion(gun_pounder("musket_ball")), "AirExplosion_sml");
        assert_eq!(air_explosion(gun_pounder("cannon_6_pounder")), "AirExplosion_sml");
        assert_eq!(air_explosion(gun_pounder("cannon_12_pounder")), "AirExplosion_med");
        assert_eq!(air_explosion(gun_pounder("cannon_24_pounder")), "AirExplosion_lrg");
        assert_eq!(air_explosion(gun_pounder("naval_carronade_64_pounder")), "AirExplosion_lrg");
        // The ground scorch is two sizes only, split at 6 pounds.
        assert_eq!(ground_impact(gun_pounder("musket_ball")), "Cannon_Groundimpact_gen_sml");
        assert_eq!(ground_impact(gun_pounder("cannon_6_pounder")), "Cannon_Groundimpact_gen_sml");
        assert_eq!(ground_impact(gun_pounder("cannon_12_pounder")), "Cannon_Groundimpact_gen_med");

        // **Round 4.** This test used to end with two assertions that could not fail: a
        // `has_group` check against `dust_lib()`, a document the test itself had just written with
        // the group name in it, and a `contains('_')` shape test over the two `const` arrays that
        // the `assert_eq!`s above had already pinned by value. Neither mentioned the impact code,
        // so `air_explosion` could be rewritten as `fn air_explosion(_: f32) -> &'static str { "AirExplosion_sml" }`
        // and this test still passed. What replaced them is the thing actually at risk in a
        // threshold table — the boundaries either side of each split, over a sweep wide enough to
        // walk off the end of the arrays.
        for p in [0.0, 1.0, 5.9, 6.0, 6.1, 11.9, 12.0, 12.1, 23.9, 24.0, 64.0, 1000.0] {
            assert!(
                impact::AIR_EXPLOSIONS.contains(&air_explosion(p)),
                "air_explosion({p}) = {} is not one of the three air bursts",
                air_explosion(p)
            );
            assert!(
                impact::GROUND_IMPACTS.contains(&ground_impact(p)),
                "ground_impact({p}) = {} is not one of the two ground impacts",
                ground_impact(p)
            );
        }
        // The split is exclusive at both ends, so a 6-pounder is the small burst and a 12-pounder
        // the middle one — the neighbouring values, which a `>=` instead of a `>` would survive.
        assert_eq!(air_explosion(6.0), "AirExplosion_sml", "6 lb is not above the 6 lb split");
        assert_eq!(air_explosion(6.1), "AirExplosion_med");
        assert_eq!(air_explosion(12.0), "AirExplosion_med", "12 lb is not above the 12 lb split");
        assert_eq!(air_explosion(12.1), "AirExplosion_lrg");
        assert_eq!(ground_impact(6.0), "Cannon_Groundimpact_gen_sml", "6 lb is not above the split");
        assert_eq!(ground_impact(6.1), "Cannon_Groundimpact_gen_med");
        // And the size never goes **down** as the shot gets heavier, so the pound count cannot be
        // walking the arrays backwards.
        let mut air = 0usize;
        let mut ground = 0usize;
        for n in 0..=200u32 {
            let p = n as f32;
            let a = impact::AIR_EXPLOSIONS.iter().position(|g| *g == air_explosion(p)).unwrap();
            let g = impact::GROUND_IMPACTS.iter().position(|x| *x == ground_impact(p)).unwrap();
            assert!(a >= air, "air_explosion({p}) went back down the size list");
            assert!(g >= ground, "ground_impact({p}) went back down the size list");
            air = a;
            ground = g;
        }
        assert_eq!((air, ground), (2, 1), "a 200-pounder is the largest of both");
    }

    /// A stand-in library with the two dust groups, so the puff timing can be tested without the
    /// install. The group entries name emitters this library actually has.
    fn dust_lib() -> EffectLibrary {
        // The `puff` emitter from TEST_DOC, verbatim, reused so the puff tests need no install.
        let emitter = TEST_DOC
            .split_once("<SCRIPTED_EFFECT_INFO name='puff'")
            .and_then(|(_, rest)| rest.split_once("</SCRIPTED_EFFECT_INFO>"))
            .map(|(body, _)| format!("<SCRIPTED_EFFECT_INFO name='puff'{body}</SCRIPTED_EFFECT_INFO>"))
            .expect("the puff emitter in TEST_DOC");
        let doc = format!(
            "<EFFECTS_MANAGER><SCRIPTED_EFFECT_INFO_LIST>{emitter}</SCRIPTED_EFFECT_INFO_LIST>\
             <SCRIPTED_EFFECT_GROUP_INFO>\
             <SCRIPTED_EFFECT_GROUP name='infantry_walk_dust'><EFFECT_GROUP_AMBIENT_SETTINGS/>\
             <SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/></SCRIPTED_EFFECT_GROUP>\
             <SCRIPTED_EFFECT_GROUP name='cavalry_walk_dust'><EFFECT_GROUP_AMBIENT_SETTINGS/>\
             <SCRIPTED_EFFECT_GROUP_ENTRY name='puff' effect='puff'/></SCRIPTED_EFFECT_GROUP>\
             </SCRIPTED_EFFECT_GROUP_INFO></EFFECTS_MANAGER>"
        );
        EffectLibrary::read(doc.as_bytes()).expect("dust library")
    }

    #[test]
    fn dust_puffs_at_the_shipped_frequency() {
        let lib = dust_lib();
        let params = dust_params();
        let mut w = FxWorld::new(31);
        let mut timers = HashMap::new();
        let puff = |w: &mut FxWorld, t: &mut HashMap<u32, f32>| {
            w.dust(&lib, &params, 7, "infantry_walking", "walking", [0.0; 3], [1.0, 0.0, 0.0], t, 0.1)
        };
        // infantry_walking frequency is 0.05 s, so a 0.1 s step owes two puffs: two groups of three.
        assert!(puff(&mut w, &mut timers));
        assert_eq!(w.particles.len(), 6, "two groups of three particles");
        // Cavalry walking is also 0.05 s; a second unit has its own timer.
        assert!(w.dust(&lib, &params, 8, "cavalry_walking", "walking", [0.0; 3], [1.0, 0.0, 0.0], &mut timers, 0.1));
        assert_eq!(w.particles.len(), 12);
        // The timer does not creep: after many ticks of walking it still holds less than one
        // interval. (It used to grow by 0.05 s every tick.)
        for _ in 0..1000 {
            puff(&mut w, &mut timers);
            w.particles.clear();
        }
        assert!(timers[&7] < 0.05, "timer {} crept past one interval", timers[&7]);
        // And a pathological step is capped rather than looped.
        let mut w = FxWorld::new(31);
        let mut capped = HashMap::new();
        w.dust(&lib, &params, 9, "infantry_walking", "walking", [0.0; 3], [1.0, 0.0, 0.0], &mut capped, 1000.0);
        assert_eq!(w.particles.len(), 3 * MAX_PUFFS_PER_STEP);
        assert!(capped[&9] < 0.05, "the excess is dropped, not carried: {}", capped[&9]);
        // The frequency is the interval between puffs (INFERRED: it is the only reading that
        // makes every value 0.05..0.45 a sensible period), so infantry_running puffs once every
        // 0.10 s of battle time: exactly once per 0.1 s model tick, ten times a second.
        let mut w = FxWorld::new(31);
        let mut timers = HashMap::new();
        let run = |w: &mut FxWorld, t: &mut HashMap<u32, f32>| {
            w.dust(&lib, &params, 1, "infantry_running", "running", [0.0; 3], [1.0, 0.0, 0.0], t, 0.1)
        };
        let mut hits = 0;
        for _ in 0..10 {
            hits += usize::from(run(&mut w, &mut timers));
        }
        assert_eq!(hits, 10, "one puff per 0.1 s tick");
        // Charging is rarer than walking, and the shipped table says so: infantry_charging is 0.15 s
        // (a puff every third 0.05 s step) and cavalry_charging 0.30 s.
        let mut w = FxWorld::new(31);
        let mut timers = HashMap::new();
        let mut hits = 0;
        for _ in 0..20 {
            hits += usize::from(w.dust(&lib, &params, 1, "infantry_charging", "charging", [0.0; 3], [1.0, 0.0, 0.0], &mut timers, 0.05));
        }
        assert_eq!(hits, 6, "0.15 s interval over 1 s of battle time");
        let mut w = FxWorld::new(31);
        let mut timers = HashMap::new();
        let mut hits = 0;
        for _ in 0..20 {
            hits += usize::from(w.dust(&lib, &params, 1, "cavalry_charging", "charging", [0.0; 3], [1.0, 0.0, 0.0], &mut timers, 0.05));
        }
        assert_eq!(hits, 3, "0.30 s interval over 1 s");
    }

    #[test]
    fn dust_is_not_emitted_for_an_entity_the_table_lacks() {
        let lib = dust_lib();
        let params = dust_params();
        let mut w = FxWorld::new(1);
        let mut timers = HashMap::new();
        for _ in 0..10 {
            assert!(!w.dust(&lib, &params, 1, "ship_sailing", "running", [0.0; 3], [0.0, 1.0, 0.0], &mut timers, 0.1));
        }
        assert!(w.particles.is_empty());
    }
}

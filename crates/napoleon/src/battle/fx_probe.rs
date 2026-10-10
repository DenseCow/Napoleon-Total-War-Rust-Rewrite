//! A headless measurement of an effect group, and the **claims** the queued in-game checks are
//! really asking.
//!
//! The problem this solves: every battle-effects check in `docs/HANDOFF.md` that is not a clean
//! negative is a *comparative* sentence — "a 12-pounder shrapnel burst should be smaller than a
//! round shell's", "a canister flash should differ from a plain gun's", "the ground scorch should
//! stay visible from a low camera". Asking a person to judge those from pixels is unreliable in
//! both directions: a difference too small to see reads as "no difference", and a difference that
//! is merely *different* reads as "matches".
//!
//! So the comparison is done here instead, on the numbers, with no window and no battle:
//! [`probe`] releases one group into a fresh [`FxWorld`] at a fixed seed, steps it through the
//! model's own 0.1 s tick and reports what came out — how many particles, how big a quad gets, how
//! long it lives, how far it drifts, how it blends, whether its sprites stand upright. [`run`] then
//! evaluates [`CLAIMS`], each of which is one of the queued checks written as a relation between
//! named groups, and prints a verdict with the numbers that produced it.
//!
//! **What this is and is not.** It measures *our* particle world driven by the shipped
//! `effects\landbattle.xml`, so a claim that holds here is a statement about what we draw from the
//! original's data — it is **not** a comparison against the original's picture, which is what the
//! in-game check is still for. What it removes is the need to eyeball: a check line becomes "the
//! numbers say PASS", and a human only has to look at what is genuinely visual (does the smoke read
//! as smoke).
//!
//! Deterministic by construction: the seed is [`PROBE_SEED`], the release point is the origin, the
//! direction is fixed and the step is the model's own 0.1 s, so the same install always gives the
//! same verdict. `probe_is_reproducible` holds it to that.
//!
//! Tags: the metrics are measured facts about our draw, not claims about the original; the *claims*
//! are the ones `docs/HANDOFF.md` is waiting on, INFERRED until checked in game. Every group name is
//! checked against the shipped file by `every_claim_names_a_shipped_group` (install), so a claim can
//! only fail because of what we do with a group, never because a name went stale.
//!
//! **A PASS here is a statement about our drawing of the shipped data, not about the original.**

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ntw_formats::effects::{EffectLibrary, FacingMode, RenderMethod};

use super::fx::FxWorld;

/// The seed every probe runs at, so a verdict is reproducible.
pub const PROBE_SEED: u32 = 0x5EED;

/// The tick the probe steps by: the model's own fixed step (CONFIRMED W1 §12.2), the same one
/// `FxWorld::advance` is driven with in a battle, so the measured life and drift are the ones the
/// battle would show.
pub const PROBE_STEP: f32 = 0.1;

/// How many ticks a probe watches: **75 s of battle time**.
///
/// The watch has to outlast the longest-lived emitter any claim names. Round 5 reported
/// `explode_smoke_airburst` at `life_range variance(15, 5)` and `Uber_smoke_brown` at
/// `variance(60, 5)` — INFERRED until `every_probe_watches_its_group_outlive_the_clock` (install),
/// which now asserts both values, has run on an install; the round-5 run that read them kept no
/// record. That test also holds this constant against every claimed group, so a too-short watch
/// cannot pass silently.
pub const PROBE_TICKS: usize = 750;

/// What one release of one group put into the world, measured.
///
/// Every field is a mean or an extreme over the particles the release made, so a group is summarised
/// by numbers rather than by a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupProbe {
    /// The group that was released.
    pub group: String,
    /// Its emitters, in the order the file lists them.
    pub emitters: Vec<String>,
    /// The distinct `texture_1` values it draws, sorted. Two groups with different sets cannot look
    /// the same, whatever their numbers say.
    pub textures: Vec<String>,
    /// How many particles one release of the group makes.
    pub particles: usize,
    /// Of those, how many are `RENDER_METHOD_ADDITIVE` (the flash) rather than alpha (the smoke).
    pub additive: usize,
    /// Of those, how many are *not* camera-facing, i.e. the `*_Y_AXIS` sprites that stand upright.
    pub upright: usize,
    /// How many of the group's **emitters** the shipped file marks not `CAMERA_FACING`: the
    /// `BILLBOARD` and `*_Y_AXIS` ones. The shipped `Cannon_Groundimpact_explosive` has four of
    /// eighteen. **Not** what stands up in our draw: `BILLBOARD` is drawn identically to
    /// `CAMERA_FACING` (round 4, from the shipped shader), so only `ground_impact_distortion` of those
    /// four is upright — see [`Self::drawn_upright_emitters`], which the scorch claim uses.
    pub upright_emitters: usize,
    /// How many of the group's emitters we actually **draw** upright: the `LOCAL_Y_AXIS` and
    /// `WORLD_Y_AXIS` ones. `BILLBOARD` is not among them (it is drawn camera-facing). This is the
    /// number the ground-scorch claim uses, because a claim about what stands up in our draw must not
    /// count sprites we draw lying towards the camera.
    pub drawn_upright_emitters: usize,
    /// Mean seconds a particle lives.
    pub mean_life: f32,
    /// The longest-lived particle, seconds. Compared against [`PROBE_TICKS`] x [`PROBE_STEP`] so a
    /// truncated measurement is visible rather than silent.
    pub max_life: f32,
    /// Mean initial speed, m/s.
    pub mean_speed: f32,
    /// Mean alpha at release, 0..1 (`colour_range_a`).
    pub mean_alpha: f32,
    /// Mean quad area at release, m² (`initial_scale_range_metres`).
    pub start_area: f32,
    /// The largest area any one quad reaches, m² — the single biggest sprite in the group.
    ///
    /// Measured as the peak of the particle's own size ramp, not by sampling frames. Sampling alone
    /// would systematically miss the end of the ramp: a particle is dropped the instant
    /// `age == life`, which is exactly where `SCALE_INFO`'s second waypoint sits, so the sample at
    /// one tick before death is one step short of the ramp's own maximum. The three waypoints are
    /// therefore included, which makes `max_area` the ramp's true peak and not a sampling artefact.
    pub max_area: f32,
    /// The most ground the group covers at any one moment, m²: the sum of its live quads' areas at
    /// their largest. This, not `max_area`, is what "a bigger burst" means — two groups can share
    /// their single biggest sprite and differ only in how many of them there are.
    ///
    /// **Read it as "how much of the screen the effect fills at its busiest", not as a size.** It
    /// counts every particle the group ever releases and every one of them keeps counting for its
    /// whole life, so a group holding one enormous slow smoke puff can cover more than a group of
    /// fast sharp debris. That is a true statement about both and not a defect, but it does mean a
    /// `COVER` claim is a claim about *coverage*, which is why the size claims that want "more/bigger
    /// sprites" are written against [`Self::particles`] and [`Self::max_area`] instead.
    pub cover_area: f32,
    /// Mean distance from the release point, metres: how far the puff drifts. A smoke that barely
    /// moves and one that blows across the line are not the same effect.
    pub travel: f32,
}

impl GroupProbe {
    /// The share of the group's particles that blend additively, 0..1.
    pub fn additive_share(&self) -> f32 {
        share(self.additive, self.particles)
    }
    /// The share of the group's particles whose sprites stand upright rather than face the camera.
    ///
    /// **Weighted by particle count, which is usually not what a visual claim wants** — see
    /// [`Self::upright_emitters`]. Kept because "a quarter of what this group draws stands up" is a
    /// real question, just not the one the ground scorch asks.
    pub fn upright_share(&self) -> f32 {
        share(self.upright, self.particles)
    }
    /// The share of the group's *emitters* that are not `CAMERA_FACING`, 0..1. See
    /// [`Self::upright_emitters`] for why this and not the particle share is the scorch's number.
    pub fn upright_emitter_share(&self) -> f32 {
        share(self.upright_emitters, self.emitters.len())
    }
    /// Whether every particle of this release expired inside the watch, i.e. the measurement saw the
    /// whole thing. `false` means `max_area` and `travel` may still have been growing.
    pub fn finished(&self) -> bool {
        self.max_life <= PROBE_TICKS as f32 * PROBE_STEP
    }
}

fn share(part: usize, all: usize) -> f32 {
    if all == 0 { 0.0 } else { part as f32 / all as f32 }
}

/// Releases `group` into an empty world at [`PROBE_SEED`] and measures what came out.
///
/// `None` when the library has no such group, or the group is empty, so a caller can tell "no group"
/// from "a group that produced nothing" — different bugs, and a claim naming a missing group must be
/// reported as missing rather than as a relation between zeros.
pub fn probe(lib: &EffectLibrary, group: &str) -> Option<GroupProbe> {
    if lib.group_effects(group).is_empty() {
        return None;
    }
    let mut world = FxWorld::new(PROBE_SEED);
    if world.spawn_group(lib, group, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0) == 0 {
        return None;
    }
    Some(measure(lib, group, world))
}

/// The largest area one particle's own size ramp ever reaches, m²: the peak of `start_scale`,
/// `mid_scale` and `end_scale`.
///
/// Sampling `Particle::size` frame by frame would miss `end_scale` — `FxWorld::advance` drops a
/// particle the moment `age == life`, and `secondary_scale_life` is 1.0 in the shipped file, so the
/// ramp's own maximum exists only at the frame that is never drawn. Folding the waypoints in makes
/// `max_area` the ramp's true peak.
fn quad_area(p: &super::fx::Particle) -> f32 {
    [p.start_scale, p.mid_scale, p.end_scale].iter().map(|[w, h]| w * h).fold(0.0f32, f32::max)
}

fn distance(from: &[f32; 3], to: &[f32; 3]) -> f32 {
    (0..3).map(|i| to[i] - from[i]).map(|d| d * d).sum::<f32>().sqrt()
}

/// How two probes are related: the ways a check line actually phrases a claim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Relation {
    /// `a`'s metric is smaller than `b`'s.
    Less,
    /// `a`'s metric is larger than `b`'s.
    Greater,
    /// The two groups draw different textures or different emitters, so they cannot look alike.
    Distinct,
    /// The two groups are the same one. Used for a check that turned out to be about the **shipped
    /// data** rather than about our drawing — see `the-three-air-bursts-are-one-group`, which records
    /// that `AirExplosion_sml` / `_med` / `_lrg` are byte-identical in `landbattle.xml`, so the size
    /// comes from the shot's own row instead.
    Same,
    /// One group's metric is at least this much, for a claim about a single group ("the ground scorch
    /// has sprites that stand upright", "a musket leaves smoke hanging").
    AtLeast(f32),
}

/// One queued in-game check, written as a relation between named groups.
#[derive(Debug, Clone, Copy)]
pub struct Claim {
    /// A stable short name, so a run can ask for one claim by name.
    pub name: &'static str,
    /// The check line it stands for, in the user's terms.
    pub question: &'static str,
    pub a: &'static str,
    /// The other group. The same as `a` for a one-sided claim, which reads oddly but keeps every
    /// claim one shape.
    pub b: &'static str,
    pub relation: Relation,
    /// The metric [`Relation::Less`] / [`Relation::Greater`] / [`Relation::AtLeast`] compare.
    pub metric: fn(&GroupProbe) -> f32,
    /// How the metric is printed beside the verdict.
    pub unit: &'static str,
}

/// The metrics a claim can be about, so a claim reads as a sentence about a number.
pub const COVER: fn(&GroupProbe) -> f32 = |p| p.cover_area;
/// The share of emitters the shipped file marks not camera-facing (`BILLBOARD` included). Not a
/// claim's metric: we draw `BILLBOARD` camera-facing, so this overstates what stands up.
#[allow(dead_code, reason = "printed by the reports' readers; no claim may use it, see above")]
pub const UPRIGHT_EMITTER_SHARE: fn(&GroupProbe) -> f32 = |p| p.upright_emitter_share();
/// How many of a group's emitters we draw upright (`*_Y_AXIS` only).
pub const DRAWN_UPRIGHT_EMITTERS: fn(&GroupProbe) -> f32 = |p| p.drawn_upright_emitters as f32;
/// Not used by any claim — every size claim wants `COVER` — but kept because it is the honest measure
/// of "how big is the biggest sprite", which is the other half of a size question, and the tests read
/// it.
#[allow(dead_code)]
pub const PEAK_AREA: fn(&GroupProbe) -> f32 = |p| p.max_area;
pub const PARTICLES: fn(&GroupProbe) -> f32 = |p| p.particles as f32;
pub const MEAN_LIFE: fn(&GroupProbe) -> f32 = |p| p.mean_life;
/// Mean drift in metres. Not a claim's metric — "how far does it blow" is a question about a scene,
/// not about two groups — but it is the number that answers it, and two tests read it.
#[allow(dead_code, reason = "read by this module's tests; a claim may want it next")]
pub const TRAVEL: fn(&GroupProbe) -> f32 = |p| p.travel;
/// The particle-weighted upright share. **Not used by any claim**: the ground scorch's single upright
/// emitter releases one particle of 429, so the share is 0.2 % and cannot express "the scorch has an
/// upright sprite" — [`DRAWN_UPRIGHT_EMITTERS`] can. Kept as the honest weighted number, and one test
/// reads it.
#[allow(dead_code, reason = "the wrong measure for the scorch claim, kept because it is the right one elsewhere")]
pub const UPRIGHT_SHARE: fn(&GroupProbe) -> f32 = |p| p.upright_share();
pub const ADDITIVE_SHARE: fn(&GroupProbe) -> f32 = |p| p.additive_share();

/// Every claim the queued battle-effects checks are waiting on.
///
/// Group names are CONFIRMED `SCRIPTED_EFFECT_GROUP` names of `effects\landbattle.xml`, and the
/// install test [`every_claim_names_a_shipped_group`] in this module's tests holds that over the real
/// file, so a claim cannot fail for want of a group.
pub const CLAIMS: &[Claim] = &[
    Claim {
        name: "the-three-air-bursts-are-one-group",
        question: "the _sml / _med / _lrg air bursts should be the same group, with the size coming from the shot's own row",
        a: "AirExplosion_sml",
        b: "AirExplosion_lrg",
        relation: Relation::Same,
        metric: COVER,
        unit: "m^2 covered",
    },
    Claim {
        name: "shell-scorch-is-the-explosive-one-not-the-generic",
        question: "the scorch under a shell hit should be the orange explosive scorch, not the old grey generic one",
        a: "Cannon_Groundimpact_explosive",
        b: "Cannon_Groundimpact_gen_sml",
        relation: Relation::Distinct,
        metric: COVER,
        unit: "m^2 covered",
    },
    Claim {
        name: "ground-scorch-sprites-stand-upright",
        question: "the ground scorch should have a sprite that stands upright (so something of it shows from a low camera)",
        a: "Cannon_Groundimpact_explosive",
        b: "Cannon_Groundimpact_explosive",
        // **Review fix.** This was `AtLeast(0.2)` on the share of emitters the *file* marks
        // non-camera-facing (4/18 = 0.222), three of which are `BILLBOARD` — which we draw facing the
        // camera. The claim passed on sprites we do not draw upright. It now counts only what we draw
        // upright; the shipped scorch has exactly one (`ground_impact_distortion`, pinned by
        // `every_non_camera_facing_emitter_of_the_shipped_file`). Whether that one sheet keeps the
        // scorch visible from a low camera is still the in-game question.
        relation: Relation::AtLeast(1.0),
        metric: DRAWN_UPRIGHT_EMITTERS,
        unit: "emitters drawn upright",
    },
    Claim {
        name: "generic-ground-impacts-grow-with-calibre",
        question: "the PROVISIONAL calibre fallback should pick more sprites, not fewer, for the bigger gun",
        a: "Cannon_Groundimpact_gen_med",
        b: "Cannon_Groundimpact_gen_sml",
        relation: Relation::Greater,
        metric: PARTICLES,
        unit: "particles per release",
    },
    Claim {
        name: "canister-fire-differs-from-a-plain-gun",
        question: "a canister shot should burst differently from a plain gun's report",
        a: "LandGunFire_canister",
        b: "LandGunFire",
        relation: Relation::Distinct,
        metric: COVER,
        unit: "m^2 covered",
    },
    Claim {
        name: "small-and-med-gun-reports-differ",
        question: "a light gun's report should differ from a plain gun's",
        a: "LandGunFire_small",
        b: "LandGunFire",
        relation: Relation::Distinct,
        metric: COVER,
        unit: "m^2 covered",
    },
    Claim {
        name: "cannon-smoke-is-bigger-than-musket-smoke",
        question: "a gun's smoke should be a bigger puff than a musket's",
        a: "CannonFire",
        b: "MusketFire",
        relation: Relation::Greater,
        metric: COVER,
        unit: "m^2 covered",
    },
    Claim {
        name: "foot-dust-is-smaller-than-musket-smoke",
        question: "dust at a unit's feet should be a smaller puff than the smoke at its musket",
        a: "infantry_walk_dust",
        b: "MusketFire",
        relation: Relation::Less,
        metric: COVER,
        unit: "m^2 covered",
    },
    Claim {
        name: "musket-smoke-lingers",
        question: "a musket volley should leave white smoke hanging for about a second",
        a: "MusketFire",
        b: "MusketFire",
        relation: Relation::AtLeast(1.0),
        metric: MEAN_LIFE,
        unit: "s mean life",
    },
    Claim {
        name: "a-firing-unit-gets-a-flash-not-only-smoke",
        question: "a firing unit should show a bright muzzle flash as well as smoke",
        a: "MusketFire",
        b: "MusketFire",
        relation: Relation::AtLeast(0.01),
        metric: ADDITIVE_SHARE,
        unit: "additive share",
    },
    Claim {
        name: "blood-is-not-a-gun-burst",
        question: "a man killed by a musket ball should get blood, not an air explosion",
        a: "blood_gen",
        b: "AirExplosion_sml",
        relation: Relation::Distinct,
        metric: COVER,
        unit: "m^2 covered",
    },
];

/// One queued in-game check about a **shot** rather than a group, because the shot's own row is what
/// sizes its burst (see [`probe_shot`]).
#[derive(Debug, Clone, Copy)]
pub struct ShotClaim {
    /// A stable short name, so a run can ask for one claim by name.
    pub name: &'static str,
    /// The check line it stands for, in the user's terms.
    pub question: &'static str,
    /// The `projectiles` key of the shot on the left of the relation.
    pub a: &'static str,
    /// The `projectiles` key of the shot on the right.
    pub b: &'static str,
    pub relation: ShotRelation,
    /// The metric compared.
    pub metric: fn(&GroupProbe) -> f32,
    /// How the metric is printed beside the verdict.
    pub unit: &'static str,
}

/// The relations a shot claim can hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShotRelation {
    /// `a`'s burst covers less ground than `b`'s.
    Smaller,
    /// `a`'s burst covers more ground than `b`'s.
    Bigger,
}

/// The queued checks that are about a shot, not a group.
///
/// **These are the ones round 5 added, and they exist because the three air-burst groups are one
/// group.** A group-vs-group size claim on `AirExplosion_sml` against `_med` is vacuous — they are
/// the same eight emitters — so the size questions are asked of the shots, whose
/// `projectiles_explosions` rows carry the burst radius that actually varies (2 m for
/// `shrapnel_3lb` to 25 m for `shell_64lb`). The projectile keys are CONFIRMED `projectiles` keys of
/// the shipped table; `every_shot_claim_names_a_shell_row` on the install holds that.
pub const SHOT_CLAIMS: &[ShotClaim] = &[
    ShotClaim {
        name: "shrapnel_12lb_bursts-smaller-than-a_12lb_shell",
        question: "a 12-pounder shrapnel burst should be noticeably smaller than a 12-pounder round shell's",
        // **`howitzer_5_In_shell`, not `fort_12_pounder_shot`.** A field gun's own `*_pounder_shot`
        // row names **no** explosion row in the shipped table — only 25 of the 144 `projectiles`
        // rows carry an `explosion` key at all, and the field guns' round shots are not among them,
        // so their burst is the PROVISIONAL calibre fallback and their burst radius does not exist.
        // The howitzer's shell row does name `shell_12lb`, which is the same 12-pounder round shell
        // (`unicorn_10_pounder_shell` names it too), so the comparison is the real one.
        a: "fort_12_pounder_shrapnel",
        b: "howitzer_5_In_shell",
        relation: ShotRelation::Smaller,
        metric: COVER,
        unit: "m^2 covered",
    },
    ShotClaim {
        name: "a_bigger_mortars_shell_bursts_bigger",
        question: "an 8-inch mortar's burst should be bigger than a 4-inch one's",
        a: "mortar_8_shell",
        b: "mortar_4_shell",
        relation: ShotRelation::Bigger,
        metric: COVER,
        unit: "m^2 covered",
    },
    ShotClaim {
        name: "heavier_shrapnel_bursts_bigger",
        question: "a 32-pounder shrapnel's burst should be bigger than a 12-pounder shrapnel's",
        a: "fort_32_pounder_shrapnel",
        b: "fort_12_pounder_shrapnel",
        relation: ShotRelation::Bigger,
        metric: COVER,
        unit: "m^2 covered",
    },
    ShotClaim {
        name: "a_shell_and_its_shrapnel_burst_differently_at_the_same_weight",
        question: "at the same calibre a round shell's burst should be bigger than its shrapnel's",
        // `shrapnel_24lb` (10 m) against `shell_24lb` (15 m): same pound count, different air group AND
        // a different radius, so this is the clearest single statement of the round-5 change.
        a: "fort_24_pounder_shrapnel",
        b: "unicorn_20_pounder_shell",
        relation: ShotRelation::Smaller,
        metric: COVER,
        unit: "m^2 covered",
    },
];

/// A projectile's air burst, measured the way the draw plays it: the group its own
/// `projectiles_explosions` row names, at the sprite scale that row's fourth number gives.
///
/// This is the only way to answer the size questions, because the three air-burst groups are one
/// group (see [`CLAIMS`]). `None` when the projectile has no explosion row, or the row names no air
/// group that the library has.
pub fn probe_shot(
    lib: &EffectLibrary,
    db: &ntw_data::GameDatabase,
    key: &str,
) -> Option<(GroupProbe, f32)> {
    let projectile = db.projectile(key)?;
    let pounder = super::fx::gun_pounder(&projectile.calibre);
    let group = super::fx::air_burst(db, projectile).or_else(|| Some(super::fx::air_explosion(pounder)))?;
    if !lib.has_group(group) {
        return None;
    }
    let scale = super::fx_draw::burst_scale(db, projectile, pounder);
    let mut world = FxWorld::new(PROBE_SEED);
    world.size_scale = scale;
    if world.spawn_group(lib, group, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0) == 0 {
        return None;
    }
    Some((measure(lib, group, world), scale))
}

/// Measures a world that has already been released into, at whatever `size_scale` it was released
/// with. Split out of [`probe`] so [`probe_shot`] can supply the scale without duplicating the walk.
fn measure(lib: &EffectLibrary, group: &str, mut world: FxWorld) -> GroupProbe {
    let effects = lib.group_effects(group);
    let emitters: Vec<String> = effects.iter().map(|e| e.name.clone()).collect();
    let textures: BTreeSet<String> = effects.iter().map(|e| e.texture().to_string()).collect();
    // "Not camera-facing" rather than "strictly y-axis": `BILLBOARD` is in this count because the
    // shipped file says it is not camera-facing, and round 4 settled that we draw it the same way —
    // a documented limitation of the number, not of the count.
    let upright_emitters = effects.iter().filter(|e| e.sprite_facing != FacingMode::Camera).count();
    let drawn_upright_emitters = effects.iter().filter(|e| e.sprite_facing.is_y_axis()).count();

    let released = world.particles.len();
    let n = released as f32;
    let mut mean_life = 0.0;
    let mut max_life = 0.0f32;
    let mut mean_speed = 0.0;
    let mut mean_alpha = 0.0;
    let mut start_area = 0.0;
    let mut max_area = 0.0f32;
    let mut cover_area = 0.0f32;
    let mut additive = 0usize;
    let mut upright = 0usize;
    for p in &world.particles {
        mean_life += p.life;
        max_life = max_life.max(p.life);
        mean_speed += (p.velocity[0].powi(2) + p.velocity[1].powi(2) + p.velocity[2].powi(2)).sqrt();
        mean_alpha += p.alpha;
        let [w, h] = p.size();
        start_area += w * h;
        max_area = max_area.max(quad_area(p));
        cover_area += quad_area(p);
        if p.render_method == RenderMethod::Additive {
            additive += 1;
        }
        if p.facing.is_y_axis() {
            upright += 1;
        }
    }

    let start: BTreeMap<u32, [f32; 3]> = world.particles.iter().map(|p| (p.serial, p.position)).collect();
    let mut travel: BTreeMap<u32, f32> = start.keys().map(|s| (*s, 0.0)).collect();
    for _ in 0..PROBE_TICKS {
        world.advance(PROBE_STEP);
        let mut covered = 0.0;
        for p in &world.particles {
            max_area = max_area.max(quad_area(p));
            covered += quad_area(p);
            if let Some(from) = start.get(&p.serial) {
                travel.insert(p.serial, distance(from, &p.position));
            }
        }
        cover_area = cover_area.max(covered);
    }

    let travelled: f32 = travel.values().sum();
    GroupProbe {
        group: group.to_string(),
        emitters,
        textures: textures.into_iter().collect(),
        particles: released,
        additive,
        upright,
        upright_emitters,
        drawn_upright_emitters,
        mean_life: mean_life / n,
        max_life,
        mean_speed: mean_speed / n,
        mean_alpha: mean_alpha / n,
        start_area: start_area / n,
        max_area,
        cover_area,
        travel: travelled / n,
    }
}

/// One claim's answer: does it hold, and on what numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    /// The claim's short name.
    pub name: &'static str,
    /// The check line it stands for.
    pub question: &'static str,
    /// Whether the relation held.
    pub holds: bool,
    /// `a`'s metric, printed.
    pub a_value: String,
    /// `b`'s metric, printed, and empty for a one-sided claim.
    pub b_value: String,
    /// How the two were compared, so a PASS is legible without the notes.
    pub detail: String,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.b_value.is_empty() {
            write!(f, "FX check {:<44} {}  {} {}", self.name, pass(self.holds), self.a_value, self.detail)
        } else {
            write!(
                f,
                "FX check {:<44} {}  {} {} {}",
                self.name,
                pass(self.holds),
                self.a_value,
                self.detail,
                self.b_value
            )
        }
    }
}

fn pass(holds: bool) -> &'static str {
    if holds { "PASS" } else { "FAIL" }
}

/// One line per probe, for a run that wants the numbers rather than the verdicts.
pub fn report(p: &GroupProbe) -> String {
    format!(
        "  {:<34} {:2} emitters {:4} particles  peak {:8.3} m^2  life {:5.2} s  travel {:6.2} m  \
         additive {:.2}  upright {:.2}  textures {}",
        p.group,
        p.emitters.len(),
        p.particles,
        p.max_area,
        p.mean_life,
        p.travel,
        p.additive_share(),
        p.upright_share(),
        p.textures.len(),
    )
}

/// One shot claim's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotVerdict {
    pub name: &'static str,
    pub question: &'static str,
    pub holds: bool,
    /// `a`'s metric, then the sprite scale the row gave (PROVISIONAL, see `burst_scale`).
    pub a_value: String,
    /// `b`'s, likewise.
    pub b_value: String,
    /// The two group names, so a PASS says *what* was compared and not just two numbers.
    pub detail: String,
    /// The relation as written, `<` or `>`.
    pub op: &'static str,
}

impl fmt::Display for ShotVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FX check {:<44} {}  {} ({}) {} {}",
            self.name,
            pass(self.holds),
            self.a_value,
            self.detail,
            if self.holds { self.op } else { "NOT" },
            self.b_value
        )
    }
}

/// Evaluates one shot claim against the install. `None` when a projectile key is not in the database
/// or its row names no air group the library has.
pub fn check_shot(
    lib: &EffectLibrary,
    db: &ntw_data::GameDatabase,
    claim: &ShotClaim,
) -> Option<ShotVerdict> {
    let (pa, sa) = probe_shot(lib, db, claim.a)?;
    let (pb, sb) = probe_shot(lib, db, claim.b)?;
    let holds = match claim.relation {
        ShotRelation::Smaller => (claim.metric)(&pa) < (claim.metric)(&pb),
        ShotRelation::Bigger => (claim.metric)(&pa) > (claim.metric)(&pb),
    };
    Some(ShotVerdict {
        name: claim.name,
        question: claim.question,
        holds,
        // The number printed is the sprite **scale** `burst_scale` applied (it used to be labelled
        // "radius ... m", which it is not: 0.6 printed as "radius 1 m").
        a_value: format!("{:>9.4} {} at scale {sa:.2}x", (claim.metric)(&pa), claim.unit),
        b_value: format!("{:>9.4} {} at scale {sb:.2}x", (claim.metric)(&pb), claim.unit),
        detail: format!("{} vs {}", pa.group, pb.group),
        op: match claim.relation {
            ShotRelation::Smaller => "<",
            ShotRelation::Bigger => ">",
        },
    })
}

/// Every shot claim, evaluated.
pub fn run_shots(lib: &EffectLibrary, db: &ntw_data::GameDatabase) -> Vec<Option<ShotVerdict>> {
    SHOT_CLAIMS.iter().map(|c| check_shot(lib, db, c)).collect()
}

/// Every shot claim's outcome as one string.
pub fn report_shots(lib: &EffectLibrary, db: &ntw_data::GameDatabase) -> String {
    let mut out = String::new();
    for v in run_shots(lib, db) {
        match v {
            Some(v) => out.push_str(&format!("{v}\n")),
            None => out.push_str("FX check MISSING SHOT: a claim names a `projectiles` row this install does not have\n"),
        }
    }
    out
}

/// Evaluates one claim against a library. `None` when a named group is missing.
pub fn check(lib: &EffectLibrary, claim: &Claim) -> Option<Verdict> {
    let a = probe(lib, claim.a)?;
    let b = probe(lib, claim.b)?;
    let one_sided = claim.a == claim.b;
    let held = match claim.relation {
        Relation::Less => (claim.metric)(&a) < (claim.metric)(&b),
        Relation::Greater => (claim.metric)(&a) > (claim.metric)(&b),
        Relation::Distinct => a.textures != b.textures || a.emitters != b.emitters,
        Relation::Same => a.textures == b.textures && a.emitters == b.emitters,
        Relation::AtLeast(at_least) => (claim.metric)(&a) >= at_least,
    };
    let (detail, b_value) = match claim.relation {
        Relation::Less => (format!("< {}", claim.b), value(&b, claim)),
        Relation::Greater => (format!("> {}", claim.b), value(&b, claim)),
        Relation::Distinct => {
            let same = a.textures == b.textures && a.emitters == b.emitters;
            (
                format!("vs {}", claim.b),
                if same { "same emitters and textures".to_string() } else { "different".to_string() },
            )
        }
        Relation::Same => {
            let same = a.textures == b.textures && a.emitters == b.emitters;
            (
                format!("is {}", claim.b),
                if same { "the same group".to_string() } else { "a different group".to_string() },
            )
        }
        Relation::AtLeast(at_least) => (format!(">= {at_least}"), String::new()),
    };
    Some(Verdict {
        name: claim.name,
        question: claim.question,
        holds: held,
        a_value: value(&a, claim),
        b_value: if one_sided { String::new() } else { b_value },
        detail,
    })
}

fn value(p: &GroupProbe, claim: &Claim) -> String {
    format!("{:>9.4} {}", (claim.metric)(p), claim.unit)
}

/// Every claim, evaluated. `None` for a claim naming a group the library does not have.
pub fn run(lib: &EffectLibrary) -> Vec<Option<Verdict>> {
    CLAIMS.iter().map(|c| check(lib, c)).collect()
}

/// Every group claim, or those whose name contains `needle`. An empty needle — or `all` — is
/// everything, which is what `NAPOLEON_FX_CHECK=all` means.
pub fn claims_matching(needle: &str) -> Vec<&'static Claim> {
    if needle.is_empty() || needle.eq_ignore_ascii_case("all") {
        return CLAIMS.iter().collect();
    }
    CLAIMS.iter().filter(|c| c.name.contains(needle)).collect()
}

/// The same, for [`SHOT_CLAIMS`].
pub fn shot_claims_matching(needle: &str) -> Vec<&'static ShotClaim> {
    if needle.is_empty() || needle.eq_ignore_ascii_case("all") {
        return SHOT_CLAIMS.iter().collect();
    }
    SHOT_CLAIMS.iter().filter(|c| c.name.contains(needle)).collect()
}

/// Every group claim's outcome as one string, for the log and the install test.
pub fn report_all(lib: &EffectLibrary) -> String {
    let mut out = String::new();
    for verdict in run(lib) {
        match verdict {
            Some(v) => out.push_str(&format!("{v}\n")),
            None => out.push_str("FX check MISSING GROUP: a claim names a group this library does not have\n"),
        }
    }
    out
}

/// Every claim, group and shot alike, as one string. What `NAPOLEON_FX_CHECK` prints and what the
/// install test prints, so the two cannot disagree about a verdict.
pub fn report_everything(lib: &EffectLibrary, db: &ntw_data::GameDatabase) -> String {
    let mut out = report_all(lib);
    out.push_str(&report_shots(lib, db));
    out
}

/// How a run went: how many claims there were, how many failed, how many could not be answered, and
/// whether the whole thing held.
///
/// [`Self::holds`] requires `missing == 0` as well as `failed == 0`. A claim that named a group the
/// install does not have has **not** passed — it has not run — and counting it as a pass would make a
/// stale name look like a clean bill of health, which is the failure mode the whole module exists to
/// prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Claims evaluated, group and shot alike.
    pub total: usize,
    /// Claims whose relation did not hold.
    pub failed: usize,
    /// Claims that named a group or a `projectiles` row this install does not have.
    pub missing: usize,
}

impl Summary {
    /// Every claim held and every claim ran.
    pub fn holds(&self) -> bool {
        self.failed == 0 && self.missing == 0
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} of {} claims FAIL, {} could not be answered, {} held overall",
            self.failed,
            self.total,
            self.missing,
            if self.holds() { "everything" } else { "NOT everything" }
        )
    }
}

/// Counts the run's outcome. What `NAPOLEON_FX_CHECK` prints as its last line and what the install test
/// asserts on.
pub fn summary(lib: &EffectLibrary, db: &ntw_data::GameDatabase) -> Summary {
    // The two lists carry different verdict types (`Verdict` and `ShotVerdict`), so each is mapped to
    // its `Option<bool>` first and the two are concatenated — the only thing `Summary` needs is whether
    // each claim held, and whether it ran at all.
    let holds: Vec<Option<bool>> = run(lib)
        .into_iter()
        .map(|v| v.map(|v| v.holds))
        .chain(run_shots(lib, db).into_iter().map(|v| v.map(|v| v.holds)))
        .collect();
    Summary {
        total: holds.len(),
        failed: holds.iter().filter(|h| **h == Some(false)).count(),
        missing: holds.iter().filter(|h| h.is_none()).count(),
    }
}

/// True when every **group** claim holds and none names a missing group. The group half of
/// [`summary`], for a caller that only has the library — which is what the round-5 `CLAIMS` half of
/// `every_claim_in_the_list_passes_against_the_test_library` asserts.
#[allow(dead_code, reason = "read by this module's tests and by callers holding only a library")]
pub fn all_hold(lib: &EffectLibrary) -> bool {
    let mut all = true;
    for claim in CLAIMS {
        match check(lib, claim) {
            Some(v) => all &= v.holds,
            None => all = false,
        }
    }
    all
}

/// True when every **shot** claim holds and none names a missing `projectiles` row. The shot half of
/// [`summary`], for the install test that has both the library and the database.
#[allow(dead_code, reason = "read by this module's tests and by callers holding only a library")]
pub fn all_shots_hold(lib: &EffectLibrary, db: &ntw_data::GameDatabase) -> bool {
    let mut all = true;
    for claim in SHOT_CLAIMS {
        match check_shot(lib, db, claim) {
            Some(v) => all &= v.holds,
            None => all = false,
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::effects::{FacingMode, RenderMethod};

    /// Three emitters and four groups, chosen so the claims have something to be right or wrong
    /// about: `big` is a big slow alpha puff, `small` a small fast one, `flash` an additive speck, and
    /// `earth` a `LOCAL_Y_AXIS` sprite so the upright share is neither 0 nor 1.
    ///
    /// Built by string rather than as a fixture constant because the scale ramp is the thing the
    /// peak-area claims measure, and a ramp that is mis-read has to be writable as a number.
    fn emitter(name: &str, num: u32, life: f32, size: f32, method: RenderMethod, facing: FacingMode) -> String {
        let render = match method {
            RenderMethod::Alpha => "RENDER_METHOD_ALPHA",
            RenderMethod::Additive => "RENDER_METHOD_ADDITIVE",
            RenderMethod::Distortion => "RENDER_METHOD_DISTORTION",
            RenderMethod::Opaque => "RENDER_METHOD_OPAQUE",
        };
        let facing = facing.as_shipped();
        format!(
            "<SCRIPTED_EFFECT_INFO name='{name}' num_particles_per_point='{num}' gravity_range='variance(0.0,0.0)'>
 <SCRIPTED_EFFECT_EMISSION_CONTROL emission_type='EMISSION_TYPE_POINT'>
  <RELEASE_INFO release_type='RELEASE_TYPE_NONE'/>
  <EMISSION_MODIFIERS><EMISSION_MODIFIER_VELOCITY value='1.0'/><EMISSION_MODIFIER_LIFETIME value='1.0'/>
   <EMISSION_MODIFIER_OPACITY value='1.0'/></EMISSION_MODIFIERS>
 </SCRIPTED_EFFECT_EMISSION_CONTROL>
 <SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES life_range='variance({life:.1},0.0)'>
  <MOVEMENT_INFO velocity_range='variance(2.0,0.0)' dir_range_XYZ='vector(0.0,1.0,0.0)'/>
  <ANIMATION_INFO total_frames='variance(1,0)' cell_width='32' cell_height='32'/>
  <COLOUR_INFO start_colour_range_r='variance(255,0)' start_colour_range_g='variance(255,0)'
   start_colour_range_b='variance(255,0)' end_colour_range_r='variance(255,0)' end_colour_range_g='variance(255,0)'
   end_colour_range_b='variance(255,0)' colour_range_a='variance(255,0)'/>
  <FADE_INFO fadein_range_life_unary='variance(0.0,0.0)' fadeout_range_life_unary='variance(1.0,0.0)'/>
  <SCALE_INFO initial_scale_range_metres='vector(variance({size:.1},0.0),variance({size:.1},0.0))'
   primary_scale_life_unary_range='variance(0.5,0.0)' primary_target_scale_range_metres='vector(variance({:.1},0.0),variance({:.1},0.0))'
   secondary_scale_life_unary_range='variance(1.0,0.0)' secondary_target_scale_range_metres='vector(variance({:.1},0.0),variance({:.1},0.0))'/>
  <ROTATION_INFO rotation_range='variance(0.0,0.0)' rotations_per_second_range='variance(0.0,0.0)'/>
 </SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES>
 <SCRIPTED_EFFECT_RENDERING_VARS texture_1='effects\\textures\\{name}_diffuse.dds' fx='particle.fx'
  render_method='{render}' sprite_facing_mode='{facing}'/>
</SCRIPTED_EFFECT_INFO>",
            size * 2.0,
            size * 2.0,
            size * 3.0,
            size * 3.0,
        )
    }

    fn group(name: &str, entries: &[&str]) -> String {
        let body: String =
            entries.iter().map(|e| format!("<SCRIPTED_EFFECT_GROUP_ENTRY name='{e}' effect='{e}'/>")).collect();
        format!("<SCRIPTED_EFFECT_GROUP name='{name}'><EFFECT_GROUP_AMBIENT_SETTINGS/>{body}</SCRIPTED_EFFECT_GROUP>")
    }

    fn build(groups: &[(&str, &[&str])]) -> EffectLibrary {
        let effects: String = [
            ("big", 4u32, 2.0f32, 2.0f32, RenderMethod::Alpha, FacingMode::Camera),
            ("small", 1, 0.4, 0.5, RenderMethod::Alpha, FacingMode::Camera),
            ("flash", 1, 0.1, 0.3, RenderMethod::Additive, FacingMode::Camera),
            ("earth", 2, 1.5, 1.0, RenderMethod::Alpha, FacingMode::LocalYAxis),
            // A long-lived mid-sized puff, so "smoke lingers for a second" has something to hold.
            ("smoke", 3, 3.0, 0.6, RenderMethod::Alpha, FacingMode::Camera),
        ]
        .iter()
        .map(|(n, num, life, size, m, f)| emitter(n, *num, *life, *size, *m, *f))
        .collect();
        let groups: String = groups.iter().map(|(n, e)| group(n, e)).collect();
        EffectLibrary::read(
            format!("<EFFECTS_MANAGER><SCRIPTED_EFFECT_INFO_LIST>{effects}</SCRIPTED_EFFECT_INFO_LIST>\
                      <SCRIPTED_EFFECT_GROUP_INFO>{groups}</SCRIPTED_EFFECT_GROUP_INFO></EFFECTS_MANAGER>")
                .as_bytes(),
        )
        .expect("the probe test document parses")
    }

    /// The library every claim in this module's own `CLAIMS` list names, so the claim-checking code
    /// can be driven end to end without the install. Same shape as the shipped names.
    fn claim_library() -> EffectLibrary {
        build(&[
            // **All three air bursts are the same group**, which is what the shipped file says — so
            // this fixture gives them the same entries too, and the test library matches the install
            // rather than papering over the finding. The size questions are asked of the *shots*
            // instead, which is what `SHOT_CLAIMS` is for.
            ("AirExplosion_sml", &["big", "flash"]),
            ("AirExplosion_med", &["big", "flash"]),
            ("AirExplosion_lrg", &["big", "flash"]),
            ("Cannon_Groundimpact_explosive", &["earth", "flash"]),
            ("Cannon_Groundimpact_gen_sml", &["small", "earth"]),
            ("Cannon_Groundimpact_gen_med", &["big", "earth"]),
            ("LandGunFire_canister", &["flash", "flash", "small"]),
            ("LandGunFire", &["flash", "small"]),
            ("LandGunFire_small", &["flash"]),
            ("CannonFire", &["big", "big"]),
            ("MusketFire", &["smoke", "flash"]),
            ("infantry_walk_dust", &["small"]),
            ("blood_gen", &["small"]),
        ])
    }

    fn claim(name: &str) -> &'static Claim {
        CLAIMS.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no claim named {name}"))
    }

    #[test]
    fn a_group_with_no_emitters_is_not_a_probe() {
        // The distinction a claim depends on: a missing group and a group that made nothing are
        // different bugs, and `None` says "cannot answer" rather than "answered zero".
        let lib = build(&[("empty", &["not_an_emitter"])]);
        assert_eq!(lib.groups["empty"].entries, vec!["not_an_emitter"], "the entry really is unresolved");
        assert!(probe(&lib, "empty").is_none(), "an all-unresolved group must not measure as zero");
        assert!(probe(&lib, "no_such_group").is_none());
    }

    #[test]
    fn the_probe_counts_what_the_group_released() {
        let lib = build(&[("two", &["big", "flash"])]);
        let p = probe(&lib, "two").expect("two exists");
        // 4 puffs + 1 flash, all released by one `spawn_group`.
        assert_eq!(p.particles, 5);
        assert_eq!(p.emitters, vec!["big".to_string(), "flash".to_string()], "in the file's order");
        assert_eq!(p.additive, 1);
        assert_eq!(p.additive_share(), 0.2);
        assert_eq!(p.upright, 0, "no sprite in this group stands upright");
        // Distinct `texture_1`s: `big_diffuse.dds` and `flash_diffuse.dds`.
        assert_eq!(p.textures, vec!["effects\\textures\\big_diffuse.dds", "effects\\textures\\flash_diffuse.dds"]);
    }

    #[test]
    fn an_upright_sprite_is_counted_and_a_camera_one_is_not() {
        let lib = build(&[("earth", &["earth"]), ("smoke", &["small"])]);
        let e = probe(&lib, "earth").expect("earth");
        // `earth` releases 2 particles, both `LOCAL_Y_AXIS`.
        assert_eq!(e.upright, e.particles);
        assert_eq!(e.upright_share(), 1.0);
        let s = probe(&lib, "smoke").expect("smoke");
        assert_eq!(s.upright_share(), 0.0, "a CAMERA_FACING sprite is not upright");
    }

    #[test]
    fn peak_area_follows_the_scale_ramp_not_the_initial_scale() {
        let lib = build(&[("big", &["big"])]);
        let p = probe(&lib, "big").expect("big");
        // The emitter ramps 2 m -> 4 m at half life -> 6 m at full life, so the peak quad is 6x6 and
        // NOT the 2x2 it starts at. A probe that read `initial_scale` would report 4.0 here, which is
        // the specific mistake this pins: "a bigger burst" is about the biggest the puff gets.
        assert!((p.start_area - 4.0).abs() < 1e-4, "start area {}", p.start_area);
        assert!((p.max_area - 36.0).abs() < 1e-3, "peak area {} (expected 6x6)", p.max_area);
        assert!(p.max_area > p.start_area);
    }

    /// **`end_scale` is only ever reached on the frame the particle is dropped**, so a probe that
    /// samples `Particle::size` frame by frame systematically under-reports the peak of any ramp
    /// whose last waypoint sits at the end of its life — which is every shipped one, since
    /// `secondary_scale_life` is 1.0. That is a silent bias, and it would make a "smaller burst"
    /// claim pass for the wrong reason, so the waypoints are folded into `max_area` directly. The
    /// test pins the size of the bias rather than only asserting the fixed behaviour.
    #[test]
    fn the_end_of_a_size_ramp_is_measured_not_sampled_past() {
        let lib = build(&[("big", &["big"])]);
        let p = probe(&lib, "big").expect("big");
        let mut w = FxWorld::new(PROBE_SEED);
        w.spawn_group(&lib, "big", [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0);
        // Sample the world exactly the way a frame-by-frame probe would and watch what it sees.
        let mut sampled = 0.0f32;
        while !w.particles.is_empty() {
            for q in &w.particles {
                let [a, b] = q.size();
                sampled = sampled.max(a * b);
            }
            w.advance(PROBE_STEP);
        }
        // It never sees the 6x6: the last frame it gets is one tick short of the ramp's end.
        assert!(sampled < p.max_area, "sampling saw {sampled}, which is not short of {}", p.max_area);
        // And the shortfall is small, so this is a real bias and not the probe being wildly wrong.
        assert!(sampled > p.max_area * 0.9, "sampled {sampled} vs peak {}", p.max_area);
        // The ratio is bounded by the step: one 0.1 s tick short of a linear ramp from 4 m to 6 m.
        assert!(p.max_area - sampled < 6.0, "shortfall {}", p.max_area - sampled);
    }

    #[test]
    fn cover_area_is_the_sum_of_the_live_quads_and_not_the_single_biggest() {
        let lib = build(&[("one", &["big"]), ("four", &["big", "big", "big", "big"])]);
        let one = probe(&lib, "one").expect("one");
        let four = probe(&lib, "four").expect("four");
        // Both hold the *same* sprite four times over, so the per-quad peak cannot tell them apart —
        // which is exactly why `air-bursts-grow-with-size` is a COVER claim and not a PEAK one.
        assert_eq!(one.max_area, four.max_area, "the same emitter gives the same biggest quad");
        assert!((four.cover_area - 4.0 * one.cover_area).abs() < 1e-3, "{} vs {}", four.cover_area, one.cover_area);
        assert!(four.cover_area > one.cover_area);
        // And cover is at least the single biggest quad: a group covers at least what its largest
        // member covers.
        assert!(four.cover_area >= four.max_area - 1e-4);
    }

    #[test]
    fn the_watch_outlives_every_particle_it_measures() {
        // **Round 5.** Without this a probe that stopped early would report a smaller `max_area` for
        // a long-lived group and a claim like "shrapnel is smaller than shell" would pass for the
        // wrong reason. `finished()` is the guard, and the install test
        // `every_probe_watches_its_group_outlive_the_clock` holds it over the real file.
        let lib = build(&[("quick", &["small"]), ("long", &["big"])]);
        let q = probe(&lib, "quick").expect("quick");
        let l = probe(&lib, "long").expect("long");
        assert!(q.finished(), "a 0.4 s emitter is inside a {:.0} s watch", PROBE_TICKS as f32 * PROBE_STEP);
        assert!(l.finished(), "a 2.0 s emitter is inside a {:.0} s watch", PROBE_TICKS as f32 * PROBE_STEP);
        // And the assertion has teeth: an emitter outliving the watch must report as unfinished, or
        // `finished` would be a constant `true` that never fails.
        let mut too_long = probe(&lib, "long").expect("long");
        too_long.max_life = PROBE_TICKS as f32 * PROBE_STEP + 1.0;
        assert!(!too_long.finished());
    }

    #[test]
    fn probe_is_reproducible() {
        // A verdict that moved between two runs of the same install would be worse than no verdict,
        // so the seed is the only thing that varies and nothing else may.
        let lib = claim_library();
        for name in ["AirExplosion_sml", "Cannon_Groundimpact_explosive", "MusketFire"] {
            let a = probe(&lib, name).expect(name);
            let b = probe(&lib, name).expect(name);
            assert_eq!(a, b, "{name} measured differently twice");
        }
    }

    #[test]
    fn less_and_greater_read_the_metric_they_name() {
        let lib = claim_library();
        let sml = probe(&lib, "AirExplosion_sml").unwrap();
        let med = probe(&lib, "AirExplosion_med").unwrap();
        let lrg = probe(&lib, "AirExplosion_lrg").unwrap();
        // The finding, made to fail loudly: all three measure the same, because they *are* the same
        // group. Before round 5 the fixture gave them different entries, so a size claim between them
        // passed here and could not possibly pass in game.
        assert_eq!(sml.emitters, med.emitters, "the fixture makes _sml and _med one group");
        assert_eq!(med.emitters, lrg.emitters, "the fixture makes _med and _lrg one group");
        assert_eq!(sml.cover_area, med.cover_area);
        assert_eq!(med.cover_area, lrg.cover_area);
        // MEAN_LIFE: `MusketFire` is three 3.0 s puffs and one 0.1 s flash, so its mean is 2.275 s —
        // above the 1.0 s `musket-smoke-lingers` claim and below the puff's own 3.0 s.
        let fire = probe(&lib, "MusketFire").unwrap();
        assert!((MEAN_LIFE)(&fire) > 1.0, "mean life {}", fire.mean_life);
        assert!((MEAN_LIFE)(&fire) < 3.0, "the flash drags it below its longest member");
        assert!((MEAN_LIFE)(&probe(&lib, "LandGunFire").unwrap()) < (MEAN_LIFE)(&fire), "a plain gun's report is briefer");
        // PARTICLES: `AirExplosion_med` releases four `big`s plus a flash.
        assert_eq!((PARTICLES)(&med), 5.0);
        // TRAVEL: every emitter here blows +y at 2 m/s, so travel is positive and ordered by life.
        let quick = probe(&lib, "infantry_walk_dust").unwrap();
        assert!((TRAVEL)(&fire) > 0.0, "travel {}", fire.travel);
        assert!((TRAVEL)(&fire) > (TRAVEL)(&quick), "a 3 s puff drifts further than a 0.4 s one");
    }

    /// Travel must not be measured over the survivors only. Every emitter in the test fixture lives
    /// at most 2 s inside a 15 s watch, so a survivor-only measurement would report 0 m for all of
    /// them and any claim about drift would be comparing zeros.
    #[test]
    fn travel_is_kept_for_a_particle_that_expired() {
        let lib = build(&[("quick", &["small"])]);
        let p = probe(&lib, "quick").expect("quick");
        // The `small` emitter: 2 m/s up, life 0.4 s, gravity and dampening zero, so it should get
        // about 0.8 m up. A survivor-only measurement would have reported exactly 0.
        assert!(p.travel > 0.5, "travel {} m over a 0.4 s life at 2 m/s", p.travel);
        assert!(p.travel < 1.0, "travel {} m", p.travel);
        assert!(p.finished(), "and nothing was alive at the end of the watch");
    }

    #[test]
    fn distinct_is_textures_or_emitters_not_the_metric() {
        // The point of `Distinct`: two groups can measure the same and still not look the same, and
        // can hold different numbers and look the same. Here `LandGunFire` and `LandGunFire_small`
        // share the `flash` emitter but differ in the second one, so they are distinct; and the
        // relation must not be reachable by making the numbers differ.
        let lib = claim_library();
        let plain = probe(&lib, "LandGunFire").unwrap();
        let small = probe(&lib, "LandGunFire_small").unwrap();
        assert_ne!(plain.emitters, small.emitters);
        let v = check(&lib, claim("small-and-med-gun-reports-differ")).expect("both groups exist");
        assert!(v.holds, "{v}");
        // Now make two groups that ARE identical and the same relation must fail, which is what stops
        // `Distinct` passing by accident.
        let twin = build(&[("LandGunFire", &["flash", "small"]), ("LandGunFire_small", &["flash", "small"])]);
        let failed = check(&twin, claim("small-and-med-gun-reports-differ")).expect("both exist");
        assert!(!failed.holds, "{failed}");
        assert_eq!(failed.b_value, "same emitters and textures");
    }

    #[test]
    fn a_missing_group_is_reported_missing_not_as_a_passing_relation() {
        // The dangerous failure: `probe` returning a zeroed struct would make `Less` and `Greater`
        // compare 0 against 0 and every claim would report FAIL, or worse, a padded struct would let
        // one pass. `check` must return `None`.
        let lib = build(&[("only_this", &["big"])]);
        assert!(check(&lib, claim("the-three-air-bursts-are-one-group")).is_none());
        assert!(!all_hold(&lib), "a claim whose group is missing is not a held claim");
    }

    #[test]
    fn at_least_is_one_sided_and_prints_no_second_value() {
        let lib = claim_library();
        let v = check(&lib, claim("ground-scorch-sprites-stand-upright")).expect("the group exists");
        assert!(v.holds, "{v}");
        assert!(v.b_value.is_empty(), "a one-sided claim has nothing to compare against: {v:?}");
        assert!(v.detail.starts_with(">="), "{}", v.detail);
        // And it must fail when the share is below the threshold: the `earth`-only group has a share
        // of 1.0, so build one with no upright sprites at all under the same name.
        let flat = build(&[("Cannon_Groundimpact_explosive", &["small"])]);
        let w = check(&flat, claim("ground-scorch-sprites-stand-upright")).expect("exists");
        assert!(!w.holds, "{w}");
    }

    #[test]
    fn the_claim_list_is_well_formed() {
        // Names are how a run asks for one claim, so they must be unique and non-empty, every claim
        // must carry a question (that is the check line it stands for), and no claim may silently
        // compare a group with itself under `Less`/`Greater` — that is always false and always a bug.
        let mut names: Vec<&str> = CLAIMS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(unique, names.len(), "two claims share a name: {names:?}");
        for c in CLAIMS {
            assert!(!c.name.is_empty() && !c.name.contains(' '), "{c:?}: name");
            assert!(!c.question.is_empty(), "{} has no question", c.name);
            assert!(!c.unit.is_empty(), "{} has no unit", c.name);
            if matches!(c.relation, Relation::Less | Relation::Greater) {
                assert_ne!(c.a, c.b, "{} compares a group with itself", c.name);
            }
            if c.relation == Relation::Same {
                assert_ne!(c.a, c.b, "{} is trivially the same as itself", c.name);
            }
            if let Relation::AtLeast(at_least) = c.relation {
                assert!(at_least > 0.0, "{}: a threshold of {at_least} passes vacuously", c.name);
            }
        }
        // `claims_matching` is the switch `NAPOLEON_FX_CHECK` uses, so it must find claims by
        // substring, match an empty needle as "all of them" (that is what `NAPOLEON_FX_CHECK=all`
        // relies on), and not match something absent.
        assert_eq!(claims_matching("").len(), CLAIMS.len(), "an empty needle means every claim");
        assert_eq!(claims_matching("all").len(), CLAIMS.len(), "and `all` does too");
        assert_eq!(claims_matching("air-bursts").len(), 1);
        // **Review.** `shrapnel-burst-smaller-than-shell-burst` was a `Same` relation under a
        // "smaller" name: it passed because the groups are identical, i.e. it could never fail and
        // said the opposite of its name. The size question is the shot claim of the same wording.
        assert!(claims_matching("shrapnel-burst-smaller").is_empty());
        assert!(claims_matching("no-such-claim").is_empty());
        // The shot-claim twin of the same switch, so `NAPOLEON_FX_CHECK` can reach both lists.
        assert_eq!(shot_claims_matching("").len(), SHOT_CLAIMS.len());
        assert_eq!(shot_claims_matching("all").len(), SHOT_CLAIMS.len());
        assert_eq!(shot_claims_matching("mortar").len(), 1);
        assert!(shot_claims_matching("no-such-claim").is_empty());
        assert_eq!(
            SHOT_CLAIMS.iter().map(|c| c.name).collect::<Vec<_>>(),
            vec![
                "shrapnel_12lb_bursts-smaller-than-a_12lb_shell",
                "a_bigger_mortars_shell_bursts_bigger",
                "heavier_shrapnel_bursts_bigger",
                "a_shell_and_its_shrapnel_burst_differently_at_the_same_weight",
            ],
            "the shot claims, in order"
        );
        for c in SHOT_CLAIMS {
            assert!(!c.name.is_empty() && !c.name.contains(' '), "{c:?}: name");
            assert!(!c.question.is_empty(), "{} has no question", c.name);
            assert_ne!(c.a, c.b, "{} compares a shot with itself", c.name);
        }
        // The scorch claims, named by the substring a user would actually type.
        assert_eq!(
            claims_matching("scorch").iter().map(|c| c.name).collect::<Vec<_>>(),
            vec![
                "shell-scorch-is-the-explosive-one-not-the-generic",
                "ground-scorch-sprites-stand-upright",
            ]
        );
    }

    #[test]
    fn every_claim_in_the_list_passes_against_the_test_library() {
        // The claim machinery is exercised end to end: every claim in `CLAIMS`, against a library
        // built to satisfy each of them, has to come back PASS. If this ever fails, either a claim was
        // added without a matching test group or a relation was written backwards.
        let lib = claim_library();
        for v in run(&lib).into_iter().flatten() {
            assert!(v.holds, "{v}\n  asked: {}", v.question);
        }
        assert!(all_hold(&lib));
    }

    /// **Needs the install** (it was not `#[ignore]`d, so `cargo test` failed on any machine
    /// without one).
    #[test]
    #[ignore]
    fn every_claim_names_a_shipped_group() {
        // The kept evidence that no queued check is waiting on a group name that does not exist, so
        // a claim can only fail because of what we do with a group and never because a name is stale.
        let (lib, _db) = install();
        let mut missing = Vec::new();
        for c in CLAIMS {
            for g in [c.a, c.b] {
                if !lib.has_group(g) {
                    missing.push(format!("{}: {g}", c.name));
                }
            }
        }
        assert!(missing.is_empty(), "claims name groups the shipped file does not have: {missing:?}");
    }

    /// **Needs the install.** Every claim PASSes against the real `effects\landbattle.xml` and the
    /// real `projectiles` table, and the measured numbers are printed so a check line can quote them.
    ///
    /// This is the evidence that replaces eyeballing a picture for the comparative claims in
    /// `docs/HANDOFF.md`. It is a statement about what we draw from the original's data; the
    /// remaining in-game question is only whether it *looks* like the original.
    #[test]
    #[ignore]
    fn every_queued_check_passes_on_the_shipped_effects() {
        let (lib, db) = install();
        for g in ["AirExplosion_sml", "AirExplosion_med", "AirExplosion_lrg"] {
            println!("{}", report(&probe(&lib, g).unwrap()));
        }
        println!("{}", report_everything(&lib, &db));
        let mut failed = Vec::new();
        for v in run(&lib).into_iter().flatten() {
            if !v.holds {
                failed.push(format!("{v}\n  asked: {}", v.question));
            }
        }
        for v in run_shots(&lib, &db).into_iter().flatten() {
            if !v.holds {
                failed.push(format!("{v}\n  asked: {}", v.question));
            }
        }
        assert!(failed.is_empty(), "{} claims failed:\n{}", failed.len(), failed.join("\n"));
        let s = summary(&lib, &db);
        assert_eq!((s.total, s.failed, s.missing), (CLAIMS.len() + SHOT_CLAIMS.len(), 0, 0));
        assert!(s.holds(), "{s}");
        assert!(all_hold(&lib));
        assert!(all_shots_hold(&lib, &db));
    }

    /// The `Summary` a run prints is the *same* text the install test asserts on, so a run and a test
    /// cannot disagree about a verdict. It also has to keep `Display` honest: three bare numbers, and a
    /// reader who only sees `0` in the failure column cannot tell "nothing failed" from "nothing ran".
    #[test]
    #[ignore]
    fn the_summary_a_run_prints_matches_the_one_the_test_asserts() {
        let (lib, db) = install();
        let s = summary(&lib, &db);
        let text = s.to_string();
        println!("{text}");
        assert_eq!(s.total, CLAIMS.len() + SHOT_CLAIMS.len());
        assert!(text.contains(&format!("{} of {} claims FAIL", s.failed, s.total)), "{text}");
        assert!(text.contains(&format!("{} could not be answered", s.missing)), "{text}");
        assert_eq!(text.contains("everything held overall"), s.holds(), "{text}");
        // And the run's exit status is the same predicate, not a separate one.
        assert_eq!(s.holds(), all_hold(&lib) && all_shots_hold(&lib, &db), "two spellings of one question");
    }

    /// **Needs the install.** Every group any claim names finishes inside the probe's watch, so no
    /// measurement is taken half-way up a size ramp — which would make a "smaller burst" claim pass
    /// for the wrong reason.
    ///
    /// This is the test that caught the shipped file's own answer: `explode_smoke_airburst` is
    /// `life_range variance(15, 5)` and `Uber_smoke_brown` is `variance(60, 5)`, so a 15 s watch was
    /// truncating a minute-long ground-dust puff mid-growth.
    #[test]
    #[ignore]
    fn every_probe_watches_its_group_outlive_the_clock() {
        let (lib, _db) = install();
        // **Review.** The two lifetimes round 5 quoted as shipped values were never asserted
        // anywhere; they are now, so the notes' "a gun's ground dust lives a minute" has evidence once
        // this has run on an install (INFERRED until then).
        for (emitter, base, var) in [("Uber_smoke_brown", 60.0, 5.0), ("explode_smoke_airburst", 15.0, 5.0)] {
            let e = lib.effects.get(emitter).unwrap_or_else(|| panic!("no emitter {emitter}"));
            println!("{emitter}: life_range {:?}", e.life_range);
            assert_eq!((e.life_range.base, e.life_range.var), (base, var), "{emitter} life_range");
        }
        let mut names: Vec<&str> = CLAIMS.iter().flat_map(|c| [c.a, c.b]).collect();
        names.sort_unstable();
        names.dedup();
        let mut unfinished = Vec::new();
        for g in names {
            let p = probe(&lib, g).unwrap_or_else(|| panic!("{g} is not a group"));
            println!("{}", report(&p));
            if !p.finished() {
                unfinished.push(format!(
                    "{g}: max life {:.2} s > watch {:.1} s",
                    p.max_life,
                    PROBE_TICKS as f32 * PROBE_STEP
                ));
            }
        }
        assert!(unfinished.is_empty(), "{}", unfinished.join("\n"));
    }

    /// **The round-5 finding, kept as an install test.** `AirExplosion_sml`, `_med` and `_lrg` are
    /// byte-identical groups in the shipped `landbattle.xml`.
    ///
    /// This is why the burst size has to come from the shot's own `projectiles_explosions` row and
    /// not from which group the row names, and it is the evidence for the round-5 change to
    /// [`super::fx_draw::burst_scale`]. Round 2 had already noticed the row's fourth number rises
    /// with the pound count and concluded it "does not pick the group"; the reason it does not is
    /// that there is nothing for it to pick between.
    #[test]
    #[ignore]
    fn the_three_air_burst_groups_are_one_group() {
        let (lib, _db) = install();
        let want = [
            "black_burn",
            "sparks_airburst",
            "sparks_airburst",
            "shock_distortion_airburst_med",
            "explode_flare_airburst",
            "explode_smoke_airburst",
            "ribbon_360",
            "ribbon_additive_360",
        ];
        for g in ["AirExplosion_sml", "AirExplosion_med", "AirExplosion_lrg"] {
            let group = &lib.groups[g];
            assert_eq!(group.entries, want, "{g} entries");
            assert_eq!(group.spawn_interval, 100.0, "{g} spawn_interval");
            assert_eq!(group.cell_size, 10.0, "{g} cell_size");
            assert_eq!(group.cell_count, 1.0, "{g} cell_count");
        }
        // And the two ground-impact pairs, which DO differ — so this is a finding about the air
        // bursts, not a parser that has stopped reading group entries.
        assert_ne!(
            lib.groups["Cannon_Groundimpact_gen_sml"].entries,
            lib.groups["Cannon_Groundimpact_gen_med"].entries,
            "the generic ground impacts do differ"
        );
        assert_eq!(lib.groups["Cannon_Groundimpact_gen_sml"].entries.len(), 9);
        assert_eq!(lib.groups["Cannon_Groundimpact_gen_med"].entries.len(), 11);
    }

    /// **The other half of the round-5 finding, and the reason `burst_scale` exists.** The fourth
    /// number of every shell and shrapnel row is a radius that rises with the calibre, and it is the
    /// only per-shot size the table carries.
    #[test]
    #[ignore]
    fn the_burst_rises_with_the_pound_count_over_the_shipped_rows() {
        let (_lib, db) = install();
        let table = &db.projectile_explosions;
        let radius = |key: &str| {
            table.get(key).unwrap_or_else(|| panic!("no row {key}")).numbers[crate::battle::fx_draw::BURST_RADIUS_COLUMN]
        };
        // Which `projectiles` rows point at which explosion row. A `projectiles` row's own
        // `explosion` foreign key is what selects the table row, and several shots share one — so the
        // fort 12-pounder's *round* shot and its *shrapnel* shot are different `projectiles` rows with
        // different `explosion` keys, and that is the whole of the difference the draw sees.
        let row_of = |key: &str| {
            let p = db.projectile(key).unwrap_or_else(|| panic!("no projectile {key}"));
            p.explosion.clone().unwrap_or_else(|| panic!("{key} names no explosion row"))
        };
        assert_eq!(row_of("fort_12_pounder_shrapnel"), "shrapnel_12lb");
        assert_eq!(row_of("howitzer_5_In_shell"), "shell_12lb");
        assert_eq!(row_of("fort_24_pounder_shrapnel"), "shrapnel_24lb");
        assert_eq!(row_of("unicorn_20_pounder_shell"), "shell_24lb");

        // **The negative that decides which shots the claims can name.** Only 25 of the 144
        // `projectiles` rows carry an `explosion` key, and a *field* gun's own `*_pounder_shot` row is
        // not one of them — so `fort_12_pounder_shot`, `cannon_12_pounder_shot` and
        // `siege_cannon_64_pounder_shot` name no explosion row, and their air burst is the PROVISIONAL
        // calibre fallback with no burst radius behind it. Only the *shrapnel* variant of a field gun
        // and the howitzer / mortar / unicorn / experimental shells do.
        //
        // This is a genuine limit on what can be checked in game, not on what we draw: the Austerlitz
        // gun line is mostly field guns, so **most of what the user sees there is the fallback path**,
        // and a shot-relative check has to name a howitzer or a mortar. Recorded as
        // [`burst_scale_falls_back_on_calibre`].
        let with_row: Vec<&str> = db
            .projectiles
            .iter()
            .filter(|p| db.explosion_effects(p).is_some())
            .map(|p| p.key.as_str())
            .collect();
        assert_eq!(with_row.len(), 25, "the 25 projectiles that resolve an explosion row: {with_row:?}");
        for key in [
            "fort_12_pounder_shot",
            "fort_18_pounder_shot",
            "fort_24_pounder_shot",
            "fort_32_pounder_shot",
            "cannon_6_pounder_shot",
            "cannon_9_pounder_shot",
            "cannon_12_pounder_shot",
            "cannon_18_pounder_shot",
            "siege_cannon_64_pounder_shot",
            "fort_carronade_64_pounder_shot",
        ] {
            let p = db.projectile(key).unwrap_or_else(|| panic!("no projectile {key}"));
            assert!(p.explosion.is_none(), "{key} names an explosion row after all");
            // ...and so its scale is the calibre fallback, not the table's radius.
            let scale = crate::battle::fx_draw::burst_scale(&db, p, crate::battle::fx::gun_pounder(&p.calibre));
            assert!(
                (0.6..=2.0).contains(&scale),
                "{key}: fallback scale {scale} is outside the calibre band"
            );
        }
        // The shrapnel series: 2, 3, 4, 6, 8, 10, 12, 14 m.
        assert_eq!(
            [
                "shrapnel_3lb", "shrapnel_6lb", "shrapnel_9lb", "shrapnel_12lb",
                "shrapnel_18lb", "shrapnel_24lb", "shrapnel_32lb", "shrapnel_64lb"
            ]
            .map(radius),
            [2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0],
            "the shrapnel radii, ascending"
        );
        // The round shells: 10, 10, 15, 20, 25 m — and a percussive shell, which is a shell that does
        // not burst, gets 20 at 12 lb against a round shell's 10. That is the table saying
        // something about the *effect*, not the weight, which is why this column cannot be read as
        // calibre alone.
        assert_eq!(["shell_12lb", "shell_18lb", "shell_24lb", "shell_32lb", "shell_64lb"].map(radius), [10.0, 10.0, 15.0, 20.0, 25.0]);
        assert_eq!(radius("shell_percussive_12lb"), 20.0);
        assert!(radius("shrapnel_12lb") < radius("shell_12lb"), "a 12 lb shrapnel bursts smaller than a 12 lb shell");
        assert!(radius("shell_64lb") > radius("shell_12lb"));
        // The scale a draw actually applies, read through the `projectiles` rows.
        let got = |key: &str| {
            let p = db.projectile(key).unwrap_or_else(|| panic!("no projectile {key}"));
            (
                crate::battle::fx_draw::burst_scale(&db, p, crate::battle::fx::gun_pounder(&p.calibre)),
                radius(&row_of(key)),
            )
        };
        assert_eq!(got("howitzer_5_In_shell"), (1.0, 10.0), "the reference shot plays at the shipped sizes");
        assert_eq!(got("fort_12_pounder_shrapnel"), (0.6, 6.0), "a 12 lb shrapnel draws at 0.6 of them");
        assert_eq!(got("fort_32_pounder_shrapnel"), (1.2, 12.0), "a 32-pounder shrapnel at 1.2x");
        assert_eq!(got("mortar_8_shell").0, 2.5, "an 8-inch mortar, the largest shipped radius, unclamped");
        // And the orderings the in-game check rests on, on the scales themselves.
        assert!(got("fort_12_pounder_shrapnel").0 < got("howitzer_5_In_shell").0, "shrapnel < shell at 12 lb");
        assert!(got("fort_12_pounder_shrapnel").0 < got("fort_32_pounder_shrapnel").0, "12 lb < 32 lb shrapnel");
        assert!(got("mortar_4_shell").0 < got("mortar_8_shell").0, "4-inch < 8-inch mortar");
        assert!(got("fort_24_pounder_shrapnel").0 < got("unicorn_20_pounder_shell").0, "shrapnel < shell at 24 lb");
    }

    /// **The family boundary on `burst_scale`, which is what keeps it honest.** The fourth number is
    /// a burst radius for the shell / shrapnel / mortar rows and is *not* one everywhere else, so it is
    /// read only where the row's air group is one of the three `AirExplosion_*` groups.
    ///
    /// Without this guard a `rocket` row's 0.99 would shrink its burst to 0.2x and a `grenade` row's
    /// 0.0 would do the same — a number read from the wrong column, silently.
    #[test]
    #[ignore]
    fn burst_scale_falls_back_on_calibre() {
        use crate::battle::fx_draw::burst_scale;
        let (_lib, db) = install();
        let scale = |key: &str| {
            let p = db.projectile(key).unwrap_or_else(|| panic!("no projectile {key}"));
            burst_scale(&db, p, crate::battle::fx::gun_pounder(&p.calibre))
        };
        // The families that do carry a radius, scaled by it.
        assert_eq!(scale("fort_12_pounder_shrapnel"), 0.6);
        assert_eq!(scale("mortar_8_shell"), 2.5);
        // The families that do not, left on the PROVISIONAL calibre band rather than shrunk.
        //
        // The **rockets** are the interesting ones: their air group *is* an `AirExplosion_*` group, so
        // the family test does not exclude them, and only the radius floor does. Their fourth number
        // is 0.99, which over the 10 m reference is a 0.099 scale — a Congreve rocket's burst drawn at
        // a tenth of the sprite sizes, smaller than anything else in the file. Reading it would be
        // wrong, not merely uncertain, so `MIN_BURST_RADIUS` leaves them on the fallback.
        for key in [
            "rockets",
            "rockets_naval",
            "grenade",
            "grenade_improved",
            "mortar_4_quicklime",
            "howitzer_experimental_quicklime",
            "howitzer_experimental_carcass",
        ] {
            let s = scale(key);
            assert!(
                (0.6..=2.0).contains(&s),
                "{key}: {s} is outside the fallback band, so the radius column leaked"
            );
        }
        // And the rockets are excluded by the floor specifically, which is the part that could rot:
        // their air group is `AirExplosion_med` / `_lrg`, so if the family test were the only guard
        // they would read 0.99.
        for key in ["rockets", "rockets_naval"] {
            let p = db.projectile(key).unwrap();
            let air = db.explosion_effects(p).and_then(|r| r.air.clone()).unwrap_or_default();
            assert!(
                crate::battle::fx::impact::AIR_EXPLOSIONS.contains(&air.as_str()),
                "{key}: its air group {air} is an AirExplosion group, so only the floor excludes it"
            );
            let radius = db.explosion_effects(p).unwrap().numbers[crate::battle::fx_draw::BURST_RADIUS_COLUMN];
            assert!(radius < crate::battle::fx_draw::MIN_BURST_RADIUS, "{key}: radius {radius} is above the floor");
        }
        // A field gun's round shot names no row at all, so it is the fallback too — see
        // `the_burst_rises_with_the_pound_count_over_the_shipped_rows` for the kept negative.
        for key in ["fort_12_pounder_shot", "cannon_12_pounder_shot", "siege_cannon_64_pounder_shot"] {
            assert!((0.6..=2.0).contains(&scale(key)), "{key}");
        }
    }

    /// **Needs the install.** Every shot claim resolves: both projectile keys exist, their rows name
    /// air groups the library has, and the comparison holds.
    #[test]
    #[ignore]
    fn every_shot_claim_names_a_shell_row_and_holds() {
        let (lib, db) = install();
        let mut missing = Vec::new();
        for c in SHOT_CLAIMS {
            for k in [c.a, c.b] {
                if db.projectile(k).is_none() {
                    missing.push(format!("{}: {k}", c.name));
                }
            }
        }
        assert!(missing.is_empty(), "shot claims name rows the shipped table does not have: {missing:?}");
        println!("{}", report_shots(&lib, &db));
        let mut failed = Vec::new();
        for v in run_shots(&lib, &db).into_iter().flatten() {
            if !v.holds {
                failed.push(format!("{v}\n  asked: {}", v.question));
            }
        }
        assert!(failed.is_empty(), "{} shot claims failed:\n{}", failed.len(), failed.join("\n"));
        assert!(all_shots_hold(&lib, &db));
    }

    /// [`summary`] must not report a clean bill of health for a claim that never ran. This is the
    /// distinction the whole module turns on: "the burst is the right size" and "there is no burst to
    /// measure" are different answers, and a stale group name must read as the second.
    #[test]
    fn a_missing_group_is_a_failure_not_a_pass() {
        let lib = build(&[("only_this", &["big"])]);
        let db = ntw_data::GameDatabase::test_fixture();
        let s = summary(&lib, &db);
        assert_eq!(s.total, CLAIMS.len() + SHOT_CLAIMS.len(), "every claim is counted");
        // Nothing in this library is a group any claim names, so every claim is missing rather than
        // failed — and `holds()` must be false.
        assert_eq!(s.failed, 0, "a missing claim is not a failed claim");
        assert_eq!(s.missing, s.total);
        assert!(!s.holds(), "a claim that did not run has not passed");
        // And `Display` says so in words rather than leaving it to be inferred from three numbers.
        let text = s.to_string();
        assert!(text.contains("could not be answered"), "{text}");
        assert!(text.contains("NOT everything"), "{text}");
        // A summary with a genuine failure also reads correctly.
        let mut failed = Summary { total: 4, failed: 1, missing: 0 };
        assert!(!failed.holds());
        failed.failed = 0;
        assert!(failed.holds(), "four claims, none failed, none missing");
        assert!(failed.to_string().contains("everything held"), "{failed}");
    }

    /// The installed effect library and database, for the `#[ignore]`d tests above.
    fn install() -> (EffectLibrary, ntw_data::GameDatabase) {
        let dir = crate::config::game_data_dir();
        let vfs = ntw_formats::pack::Vfs::open_install(dir.clone()).expect("open install");
        let lib = EffectLibrary::from_vfs(&vfs).expect("landbattle.xml");
        let db = ntw_data::GameDatabase::from_install(dir).expect("database");
        (lib, db)
    }

    /// **The kept evidence for the round-3 facing claim, made measurable.** Every non-camera-facing
    /// emitter of the shipped land-battle file, and which group plays it.
    ///
    /// This is the survey that showed the scorch's upright sprite is a single
    /// `ground_impact_distortion` sheet — `scale 0.0 -> 20.0 -> 30.0` metres, one particle, 0.7 s — and
    /// that the rest of the scorch's non-camera-facing entries are `BILLBOARD` earth and smoke, which
    /// round 4 closed as indistinguishable from `CAMERA_FACING`. So exactly **one** sprite of the
    /// eighteen stands up in our draw, and it is the big one.
    #[test]
    #[ignore]
    fn every_non_camera_facing_emitter_of_the_shipped_file() {
        let (lib, _db) = install();
        let mut modes: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut entries = 0usize;
        for g in lib.groups.values() {
            for name in g.entries.iter() {
                let Some(e) = lib.effects.get(name) else { continue };
                if e.sprite_facing == FacingMode::Camera {
                    continue;
                }
                entries += 1;
                *modes.entry(e.sprite_facing.as_shipped()).or_default() += 1;
                let s = e.scales(e.initial_scale, e.primary_target_scale, e.secondary_target_scale);
                println!(
                    "  {:<14} {:<32} in {:<34} n {:>4} life {:>5.2}  scale {:.1} -> {:.1} -> {:.1}",
                    e.sprite_facing.as_shipped(),
                    e.name,
                    g.name,
                    e.num_particles_per_point,
                    e.life_seconds(e.life_range.base),
                    s.0.c[0].base,
                    s.1.c[0].base,
                    s.2.c[0].base,
                );
            }
        }
        println!("{entries} non-camera-facing group entries: {modes:?}");
        // The three modes the shipped land-battle file writes and no fourth — in particular no
        // `VELOCITY_FACING`, the clean negative
        // `ntw_formats/tests/effects_install.rs::every_sprite_facing_mode_is_a_named_one` holds over
        // all three effect files.
        assert_eq!(
            modes.keys().copied().collect::<Vec<_>>(),
            vec!["BILLBOARD", "LOCAL_Y_AXIS", "WORLD_Y_AXIS"]
        );
        // 46 group entries across the file's 152 groups; `WORLD_Y_AXIS` is the single `shockwave_large`
        // in `ShipExplosion`, which is why it appears once.
        assert_eq!(modes["WORLD_Y_AXIS"], 1);
        assert_eq!(entries, 46);
        // The scorch's single upright sprite, named. If this ever changes, the round-3 in-game check
        // line is describing something we no longer draw.
        let scorch = &lib.groups["Cannon_Groundimpact_explosive"];
        assert_eq!(scorch.entries.len(), 18, "the scorch's 18 emitters");
        let non_camera: Vec<&str> = scorch
            .entries
            .iter()
            .filter_map(|n| lib.effects.get(n))
            .filter(|e| e.sprite_facing != FacingMode::Camera)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            non_camera,
            vec!["impact_cannon_earth_lrg1", "ground_impact_distortion", "explosion", "cannon_hit_smoke"],
            "the scorch's four non-camera-facing emitters, in file order"
        );
        // ...and only the second of those four stands up in our draw, at 30 m across.
        let standing: Vec<(&str, f32)> = scorch
            .entries
            .iter()
            .filter_map(|n| lib.effects.get(n))
            .filter(|e| e.sprite_facing.is_y_axis())
            .map(|e| {
                (
                    e.name.as_str(),
                    e.scales(e.initial_scale, e.primary_target_scale, e.secondary_target_scale).2.c[0].base,
                )
            })
            .collect();
        assert_eq!(standing, vec![("ground_impact_distortion", 30.0)], "one upright sheet, 30 m across");
    }
}

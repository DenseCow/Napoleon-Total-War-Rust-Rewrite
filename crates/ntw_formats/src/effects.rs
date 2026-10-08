//! The game's particle-effect database: `effects\landbattle.xml` (and its naval and
//! campaign-map siblings) plus `effects\unit_dust_parameters.txt`.
//!
//! **The whole visual effect system is data, not code.** The original ships 283
//! `SCRIPTED_EFFECT_INFO` blocks (one particle emitter each) and 152 `SCRIPTED_EFFECT_GROUP`
//! blocks (named composites of emitters) in `effects\landbattle.xml`; the engine plays a *group*,
//! never a single emitter, and the DB refers to groups by name
//! (`db\particle_effects_tables\particle_effects` is one string column of 104 group names,
//! CONFIRMED). See `analysis/graphics/BATTLE_EFFECTS.md`.
//!
//! Three value literal forms appear in the file (CONFIRMED, all 283 shipped rows):
//! - a bare number: `0.500000`
//! - `variance(base,var)`: a value with a symmetric spread, e.g. `variance(0.010000,0.005000)`
//! - `vector(...)` / `vector(a,b,c)`: 2- or 3-component, each component in either of the above
//!
//! Tags: attribute and element names are CONFIRMED (read from the shipped file). What the engine
//! *does* with the emission modifiers is INFERRED (3906 of the 3962 shipped modifier elements are
//! `EMISSION_MODIFIER_TYPE_NONE`, 37 `ADD`, 19 `MUL`: `land_battle_effects_parse`, install; see
//! [`ModifierType`]), and the engine's particle RNG is UNKNOWN.

use std::collections::BTreeMap;

use crate::xml::{self, XmlElement};

/// One `variance(base,var)` value: the number the engine draws around, and its spread.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Variance {
    /// The centre value.
    pub base: f32,
    /// The spread. The shipped file always writes `var >= 0`; we take the absolute value so a
    /// negative spread cannot invert a draw.
    pub var: f32,
}

impl Variance {
    /// A value with no spread.
    pub const fn exact(base: f32) -> Self {
        Self { base, var: 0.0 }
    }

    /// Parses `0.5`, `variance(0.5,0.1)` or a bare `variance(...)` fragment.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if let Some(inner) = t.strip_prefix("variance(").and_then(|s| s.strip_suffix(')')) {
            let (a, b) = inner.split_once(',')?;
            return Some(Self { base: a.trim().parse().ok()?, var: b.trim().parse::<f32>().ok()?.abs() });
        }
        Some(Self::exact(t.parse().ok()?))
    }
}

/// A `vector(...)` of 2 or 3 [`Variance`]s. Missing components are zero.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VarVector {
    /// The components, in the file's order (x, y[, z]).
    pub c: [Variance; 3],
    /// How many components the file actually wrote (2 or 3).
    pub len: u8,
}

impl VarVector {
    /// A vector with no spread.
    pub fn exact(v: [f32; 3]) -> Self {
        Self { c: [Variance::exact(v[0]), Variance::exact(v[1]), Variance::exact(v[2])], len: 3 }
    }

    /// Parses `vector(a,b)` / `vector(a,b,c)`, where each component is a number or `variance(...)`.
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        let inner = t.strip_prefix("vector(").and_then(|s| s.strip_suffix(')'))?;
        // Split on the commas that are not inside a `variance(...)`.
        let mut parts = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;
        for (i, c) in inner.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    parts.push(&inner[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        parts.push(&inner[start..]);
        let mut c = [Variance::default(); 3];
        for (i, p) in parts.iter().enumerate().take(3) {
            c[i] = Variance::parse(p)?;
        }
        Some(Self { c, len: parts.len() as u8 })
    }
}

/// What an `EMISSION_MODIFIER_*` does to its attribute.
///
/// **This is load-bearing and it was nearly got wrong.** In all 283 shipped emitters the
/// `NONE`-typed modifiers carry the value `0.000000` — 207 of the 283 velocity modifiers, 277 of
/// the 283 lifetime modifiers, 265 of the 283 opacity modifiers. A `NONE` modifier with value 0
/// therefore means **"leave this attribute alone"**, not "scale it by 0": read as a multiplier it
/// would give every particle a life of zero seconds and the whole effect system would emit
/// nothing. Only `ADD` (37 rows) and `MUL` (19 rows) apply, and they are the only rows whose value
/// is not the neutral 0.
///
/// CONFIRMED the enum names and the row counts; INFERRED that `NONE` means "no change" (it is the
/// only reading that leaves the shipped effects alive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ModifierType {
    /// `EMISSION_MODIFIER_TYPE_NONE`: the value is inert.
    #[default]
    None,
    /// `EMISSION_MODIFIER_TYPE_ADD`: the value is added to the attribute.
    Add,
    /// `EMISSION_MODIFIER_TYPE_MUL`: the value multiplies it.
    Mul,
}

impl ModifierType {
    fn parse(text: &str) -> Option<Self> {
        Some(match text.trim() {
            "EMISSION_MODIFIER_TYPE_NONE" => Self::None,
            "EMISSION_MODIFIER_TYPE_ADD" => Self::Add,
            "EMISSION_MODIFIER_TYPE_MUL" => Self::Mul,
            _ => return None,
        })
    }

    /// Applies `value` to `base`: nothing for [`Self::None`], `base + value` for [`Self::Add`],
    /// `base * value` for [`Self::Mul`].
    pub fn apply(self, base: f32, value: f32) -> f32 {
        match self {
            Self::None => base,
            Self::Add => base + value,
            Self::Mul => base * value,
        }
    }
}

/// How an emitter releases its particles (CONFIRMED enum names).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EmissionType {
    /// `EMISSION_TYPE_POINT` — a single point (277 of the 283 shipped rows).
    #[default]
    Point,
    /// `EMISSION_TYPE_RADIAL` — a cone/disc around the direction (5 shipped rows).
    Radial,
    /// `EMISSION_TYPE_SPHERICAL` — a full sphere (1 shipped row).
    Spherical,
}

impl EmissionType {
    fn parse(text: &str) -> Option<Self> {
        Some(match text.trim() {
            "EMISSION_TYPE_POINT" => Self::Point,
            "EMISSION_TYPE_RADIAL" => Self::Radial,
            "EMISSION_TYPE_SPHERICAL" => Self::Spherical,
            _ => return None,
        })
    }
}

/// How an emitter's particles are drawn (CONFIRMED enum names; unknown names read as `Alpha`).
/// Ordered so a caller can bucket particles by blend mode in a stable order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum RenderMethod {
    /// `RENDER_METHOD_ALPHA`: ordinary alpha blending (smoke, dust).
    #[default]
    Alpha,
    /// `RENDER_METHOD_ADDITIVE`: additive (flashes, glows, sparks).
    Additive,
    /// `RENDER_METHOD_DISTORTION`: the screen-distortion shader (heat haze, blast distortion).
    Distortion,
    /// `RENDER_METHOD_OPAQUE`: no blending.
    Opaque,
}

impl RenderMethod {
    fn parse(text: &str) -> Option<Self> {
        Some(match text.trim() {
            "RENDER_METHOD_ALPHA" => Self::Alpha,
            "RENDER_METHOD_ADDITIVE" | "RENDER_METHOD_ADD" | "RENDER_METHOD_ADDITIVE_BRIGHT" => Self::Additive,
            "RENDER_METHOD_DISTORTION" => Self::Distortion,
            "RENDER_METHOD_OPAQUE" => Self::Opaque,
            _ => return None,
        })
    }
}

/// How a particle sprite is turned to face the camera (CONFIRMED enum names).
///
/// The three shipped effect files write exactly four values between them, and every one is named
/// here. CONFIRMED by the whole-pack install test
/// `ntw_formats/tests/effects_install.rs::every_sprite_facing_mode_is_a_named_one`:
///
/// | value | land battle | naval | campaign |
/// |---|---|---|---|
/// | `CAMERA_FACING` | 261 | 121 | 19 |
/// | `BILLBOARD` | 13 | 5 | 0 |
/// | `LOCAL_Y_AXIS` | 8 | 6 | 0 |
/// | `WORLD_Y_AXIS` | 1 | 1 | 0 |
///
/// **No shipped file ever writes `VELOCITY_FACING`** (a clean negative over all 435 emitters), so
/// the variant is kept for completeness and nothing draws with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FacingMode {
    /// `CAMERA_FACING`: a billboard whose normal points at the camera. 401 of the 435 emitters.
    #[default]
    Camera,
    /// `BILLBOARD`: 18 emitters, earth, debris, dust and water sprites. Drawn exactly like
    /// [`FacingMode::Camera`]: no shipped attribute separates the two and the shipped particle
    /// shader is handed one camera-aligned basis and no facing parameter
    /// (`effects_install.rs::billboard_is_indistinguishable_from_camera_facing_in_the_shipped_shader`).
    /// What the exe does differently, if anything, is UNKNOWN.
    Billboard,
    /// `LOCAL_Y_AXIS`: 14 emitters, the ground-impact distortion and the water ripples. INFERRED
    /// from the name: the quad stays vertical and turns about **its emitter's local Y**, which for
    /// us is world up.
    LocalYAxis,
    /// `WORLD_Y_AXIS`: 2 emitters (`shockwave_large` in both battle files). As
    /// [`FacingMode::LocalYAxis`] but about the world Y rather than the emitter's.
    WorldYAxis,
    /// `VELOCITY_FACING`: turned along its own movement. **Never written by any shipped file.**
    Velocity,
    /// A name the three effect files do not use. Kept so an unknown value is visible rather than
    /// silently drawn as something else.
    Other,
}

impl FacingMode {
    /// True when the quad stands upright and turns about the vertical, rather than pointing its
    /// normal at the camera: the shipped `LOCAL_Y_AXIS` and `WORLD_Y_AXIS` sprites.
    pub fn is_y_axis(self) -> bool {
        matches!(self, Self::LocalYAxis | Self::WorldYAxis)
    }

    /// The shipped string this mode came from, for the notes and the logs.
    pub fn as_shipped(self) -> &'static str {
        match self {
            Self::Camera => "CAMERA_FACING",
            Self::Billboard => "BILLBOARD",
            Self::LocalYAxis => "LOCAL_Y_AXIS",
            Self::WorldYAxis => "WORLD_Y_AXIS",
            Self::Velocity => "VELOCITY_FACING",
            Self::Other => "OTHER",
        }
    }
}

/// Which RGB channels share one draw (CONFIRMED: `NO_CHANNELS_LINKED`,
/// `RED_GREEN_BLUE_CHANNELS_LINKED`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelLink {
    /// `NO_CHANNELS_LINKED`: r, g and b are drawn independently.
    #[default]
    None,
    /// `RED_GREEN_BLUE_CHANNELS_LINKED`: r drives g and b too.
    Rgb,
}

/// One `SCRIPTED_EFFECT_INFO`: a single particle emitter.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// `name` attribute.
    pub name: String,
    /// `num_particles_per_point`: how many particles each release point makes.
    pub num_particles_per_point: u32,
    /// `gravity_range`: downward acceleration in m/s².
    pub gravity: Variance,
    /// `min_lod_range` / `max_lod_range`: the effect is not emitted inside / beyond these
    /// distances from the camera (metres).
    pub min_lod_range: Variance,
    pub max_lod_range: Variance,
    /// `wind_range`: how strongly the scene wind pushes these particles.
    pub wind: Variance,
    /// `thickness`: the emitter's depth along its direction (ribbon emitters).
    pub thickness: Variance,
    /// `lighting`: how much scene light the particle takes (see the open item in the notes).
    pub lighting: Variance,
    /// `adjust_direction_by_offset_position`: the release offset steers the direction too.
    pub adjust_direction_by_offset: bool,
    /// `clamp_sea_level`: particles are kept above sea level.
    pub clamp_sea_level: bool,
    /// `quality_level` (0..3 in the shipped file).
    pub quality_level: u8,
    /// `SCRIPTED_EFFECT_EMISSION_CONTROL/@emission_type`.
    pub emission_type: EmissionType,
    /// `RELEASE_INFO/@release_type` kept verbatim (UNKNOWN; every shipped row is
    /// `RELEASE_TYPE_NONE`).
    pub release_type: String,
    /// `release_interval_range`: seconds between releases.
    pub release_interval: Variance,
    /// `release_position_variation_range`: the cone of positions a particle may start in.
    pub release_position_variation: VarVector,
    /// `emission_time_range_seconds`: how long the emitter keeps releasing.
    pub emission_time: Variance,
    /// `EMISSION_MODIFIER_VELOCITY`: how it changes `MOVEMENT_INFO/velocity_range`. Read with
    /// [`Effect::velocity_scale`]; the raw value alone is meaningless without the type.
    pub velocity_modifier: Modifier,
    /// `EMISSION_MODIFIER_LIFETIME`: how it changes `life_range`.
    pub lifetime_modifier: Modifier,
    /// `EMISSION_MODIFIER_OPACITY`: how it changes `colour_range_a`.
    pub opacity_modifier: Modifier,
    /// `EMISSION_MODIFIER_INITIAL_SCALE`, `TARGET_SCALE1`, `TARGET_SCALE2`: how they change the
    /// three `SCALE_INFO` sizes.
    pub scale_modifiers: [Modifier; 3],
    /// `EMISSION_MODIFIER_INITIAL_ROTATION`, `ROTATION_SPEED`: how they change `ROTATION_INFO`.
    pub rotation_modifiers: [Modifier; 2],
    /// `EMISSION_MODIFIER_START_*` and `END_*`: how they change the colour ramp.
    pub colour_modifiers: [[Modifier; 3]; 2],
    /// `MOVEMENT_INFO/@velocity_range`: the initial speed along the direction (m/s).
    pub velocity: Variance,
    /// `MOVEMENT_INFO/@dampening_range`: speed lost per second (fraction).
    pub dampening: Variance,
    /// `MOVEMENT_INFO/@dir_range_XYZ`: the base direction, x/y/z in [0,1] per axis with the
    /// signed value in the component itself (the shipped values are 0 or 1).
    pub direction: [f32; 3],
    /// `conic_range_fan` / `conic_range_depth`: the cone of directions around `direction`.
    pub conic_fan: Variance,
    pub conic_depth: Variance,
    /// `ANIMATION_INFO`: the sprite sheet's frame grid.
    pub start_frame_variation: Variance,
    pub animation_length: Variance,
    pub loop_count: Variance,
    pub total_frames: Variance,
    pub cell_width: u32,
    pub cell_height: u32,
    /// `COLOUR_INFO`: the colour ramp, 0..255.
    pub start_colour: [Variance; 3],
    pub end_colour: [Variance; 3],
    pub colour_alpha: Variance,
    pub start_channels_linked: ChannelLink,
    pub end_channels_linked: ChannelLink,
    /// `SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES/@life_range`: seconds a particle lives,
    /// before the lifetime emission modifier.
    pub life_range: Variance,
    /// `ribbon_life_range`: how long a particle lives in a ribbon emitter.
    pub ribbon_life_range: Variance,
    /// `FADE_INFO`: fade-in and fade-out as fractions of the particle's life.
    pub fade_in: Variance,
    pub fade_out: Variance,
    /// `SCALE_INFO`: the size ramp, in metres.
    pub initial_scale: VarVector,
    pub primary_scale_life: Variance,
    pub primary_target_scale: VarVector,
    pub secondary_scale_life: Variance,
    pub secondary_target_scale: VarVector,
    /// `radial_blur_strength` / the two distances around it (UNKNOWN; every shipped row is 0).
    pub radial_blur_strength: f32,
    pub radial_blur_min_distance: f32,
    pub radial_blur_max_distance: f32,
    /// `ROTATION_INFO`: radians.
    pub rotation: Variance,
    pub rotations_per_second: Variance,
    /// `SCRIPTED_EFFECT_RENDERING_VARS`: the eight texture slots, exactly as written (pack paths;
    /// the leading `\` some rows carry is kept so the path can be used verbatim).
    pub textures: [String; 8],
    /// `@fx`: the shipped shader this emitter uses (`particle.fx`, `ribbon.fx`, ...).
    pub fx: String,
    /// `@render_method`.
    pub render_method: RenderMethod,
    /// `@align_to_velocity`.
    pub align_to_velocity: bool,
    /// `@sprite_facing_mode`.
    pub sprite_facing: FacingMode,
    /// `@max_effects` / `@max_num_quads`: the engine's own budget for one emitter instance.
    pub max_effects: u32,
    pub max_num_quads: u32,
}

/// One `EMISSION_MODIFIER_*` element: what it does and by how much.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Modifier {
    /// The `@type` (see [`ModifierType`]).
    pub kind: ModifierType,
    /// The `@value`, a number or a `vector(...)`.
    pub value: VarVector,
}

/// An `EMISSION_MODIFIER_*/@value`: either a `vector(variance(...),variance(...))` (the three
/// `INITIAL_SCALE` / `TARGET_SCALE*` rows) or a bare number or `variance(...)` (all the others).
/// A bare number fills the x component only, leaving y and z at zero.
pub fn modifier_value(text: &str) -> VarVector {
    let t = text.trim();
    if t.starts_with("vector(") {
        return VarVector::parse(t).unwrap_or_default();
    }
    let one = Variance::parse(t).unwrap_or_default();
    VarVector { c: [one, Variance::default(), Variance::default()], len: 1 }
}

impl Modifier {
    /// The modifier's effect on one scalar attribute, with `base` the attribute's own value.
    pub fn scalar(&self, base: f32) -> f32 {
        self.kind.apply(base, self.value.c[0].base)
    }

    /// The modifier's effect on a two-component size. Only the components the modifier writes
    /// move; the rest keep their own value.
    pub fn size(&self, base: VarVector) -> VarVector {
        let moved = |i: usize| Variance { base: self.kind.apply(base.c[i].base, self.value.c[i].base), var: base.c[i].var };
        VarVector { c: [moved(0), moved(1), base.c[2]], len: base.len.max(1) }
    }

    /// The modifier's effect on one colour channel, in the file's 0..255 scale.
    pub fn channel(&self, base: Variance) -> Variance {
        Variance { base: self.kind.apply(base.base, self.value.c[0].base), var: base.var }
    }
}

impl Effect {
    /// The primary sprite texture: slot 1, with the leading `\` some rows carry removed.
    pub fn texture(&self) -> &str {
        self.textures[0].trim_start_matches('\\')
    }

    /// The particle's initial speed, after `EMISSION_MODIFIER_VELOCITY`.
    pub fn velocity(&self, base: f32) -> f32 {
        self.velocity_modifier.scalar(base)
    }

    /// `life_range` after `EMISSION_MODIFIER_LIFETIME`, in seconds.
    pub fn life_seconds(&self, base: f32) -> f32 {
        self.lifetime_modifier.scalar(base)
    }

    /// `colour_range_a` after `EMISSION_MODIFIER_OPACITY`, in the file's 0..255 scale.
    pub fn opacity(&self, base: f32) -> f32 {
        self.opacity_modifier.scalar(base)
    }

    /// The three `SCALE_INFO` sizes after their modifiers.
    pub fn scales(&self, start: VarVector, mid: VarVector, end: VarVector) -> (VarVector, VarVector, VarVector) {
        (
            self.scale_modifiers[0].size(start),
            self.scale_modifiers[1].size(mid),
            self.scale_modifiers[2].size(end),
        )
    }

    /// `ROTATION_INFO/rotation_range` and `rotations_per_second_range` after their modifiers.
    pub fn rotation(&self, base: Variance, speed: Variance) -> (Variance, Variance) {
        (self.rotation_modifiers[0].channel(base), self.rotation_modifiers[1].channel(speed))
    }

    /// The colour ramp after its modifiers, 0..255.
    pub fn colour_ramp(&self, start: [Variance; 3], end: [Variance; 3]) -> ([Variance; 3], [Variance; 3]) {
        let one = |mods: [Modifier; 3], base: [Variance; 3]| {
            [mods[0].channel(base[0]), mods[1].channel(base[1]), mods[2].channel(base[2])]
        };
        (one(self.colour_modifiers[0], start), one(self.colour_modifiers[1], end))
    }

    /// Frames in the sprite sheet: `total_frames` as written, at least 1.
    pub fn frames(&self) -> u32 {
        self.total_frames.base.round().max(1.0) as u32
    }

    /// Seconds a particle lives, before the lifetime emission modifier: the file's `life_range`.
    pub fn life_range(&self) -> Variance {
        self.life_range
    }

    /// Reads one `SCRIPTED_EFFECT_INFO` element.
    pub fn from_xml(e: &XmlElement) -> Option<Self> {
        let a = |n: &str| e.attr(n).unwrap_or_default();
        let v = |n: &str| Variance::parse(a(n)).unwrap_or_default();
        let control = e.child("SCRIPTED_EFFECT_EMISSION_CONTROL");
        let release = control.and_then(|c| c.child("RELEASE_INFO"));
        let emission = control
            .map(|c| c.child("EMISSION_MODIFIERS"))
            .unwrap_or_else(|| e.find("EMISSION_MODIFIERS"));
        let mods = emission?;
        // An `EMISSION_MODIFIER_*` element: its `@type` and its `@value`. A missing element is a
        // `NONE` modifier with value 0, which leaves the attribute alone (see [`ModifierType`]).
        let modifier = |n: &str| {
            mods.child(n).map_or(Modifier::default(), |m| Modifier {
                kind: m.attr("type").and_then(ModifierType::parse).unwrap_or_default(),
                value: modifier_value(m.attr("value").unwrap_or_default()),
            })
        };
        let attrs = e.child("SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES")?;
        let mov = attrs.child("MOVEMENT_INFO");
        let anim = attrs.child("ANIMATION_INFO");
        let col = attrs.child("COLOUR_INFO");
        let fade = attrs.child("FADE_INFO");
        let scale = attrs.child("SCALE_INFO");
        let rot = attrs.child("ROTATION_INFO");
        let rend = e.child("SCRIPTED_EFFECT_RENDERING_VARS")?;
        fn get<'a>(p: Option<&'a XmlElement>, n: &str) -> &'a str {
            p.and_then(|x| x.attr(n)).unwrap_or_default()
        }
        let dir_s = get(mov, "dir_range_XYZ");
        let dir: [f32; 3] = {
            let parts: Vec<f32> = dir_s
                .trim_start_matches("vector(")
                .trim_end_matches(')')
                .split(',')
                .filter_map(|p| p.trim().parse().ok())
                .collect();
            [parts.first().copied().unwrap_or(0.0), parts.get(1).copied().unwrap_or(1.0), parts.get(2).copied().unwrap_or(0.0)]
        };
        let colour = |p: Option<&XmlElement>, stem: &str| -> [Variance; 3] {
            [Variance::parse(get(p, &format!("{stem}_colour_range_r"))).unwrap_or_default(),
             Variance::parse(get(p, &format!("{stem}_colour_range_g"))).unwrap_or_default(),
             Variance::parse(get(p, &format!("{stem}_colour_range_b"))).unwrap_or_default()]
        };
        let link = |p: Option<&XmlElement>, stem: &str| match get(p, &format!("{stem}_linked_channels")) {
            "RED_GREEN_BLUE_CHANNELS_LINKED" => ChannelLink::Rgb,
            _ => ChannelLink::None,
        };
        Some(Self {
            name: a("name").to_owned(),
            num_particles_per_point: a("num_particles_per_point").parse().unwrap_or(1),
            gravity: v("gravity_range"),
            min_lod_range: v("min_lod_range"),
            max_lod_range: v("max_lod_range"),
            wind: v("wind_range"),
            thickness: v("thickness"),
            lighting: v("lighting"),
            adjust_direction_by_offset: a("adjust_direction_by_offset_position").eq_ignore_ascii_case("true"),
            clamp_sea_level: a("clamp_sea_level").eq_ignore_ascii_case("true"),
            quality_level: a("quality_level").parse().unwrap_or(0),
            emission_type: control
                .and_then(|c| c.attr("emission_type"))
                .and_then(EmissionType::parse)
                .unwrap_or_default(),
            release_type: release.map_or(String::new(), |r| r.attr("release_type").unwrap_or_default().to_owned()),
            release_interval: Variance::parse(get(release, "release_interval_range")).unwrap_or_default(),
            release_position_variation: VarVector::parse(get(release, "release_position_variation_range")).unwrap_or_default(),
            emission_time: Variance::parse(control.and_then(|c| c.attr("emission_time_range_seconds")).unwrap_or_default()).unwrap_or_default(),
            velocity_modifier: modifier("EMISSION_MODIFIER_VELOCITY"),
            lifetime_modifier: modifier("EMISSION_MODIFIER_LIFETIME"),
            opacity_modifier: modifier("EMISSION_MODIFIER_OPACITY"),
            scale_modifiers: [
                modifier("EMISSION_MODIFIER_INITIAL_SCALE"),
                modifier("EMISSION_MODIFIER_TARGET_SCALE1"),
                modifier("EMISSION_MODIFIER_TARGET_SCALE2"),
            ],
            rotation_modifiers: [
                modifier("EMISSION_MODIFIER_INITIAL_ROTATION"),
                modifier("EMISSION_MODIFIER_ROTATION_SPEED"),
            ],
            colour_modifiers: [
                [
                    modifier("EMISSION_MODIFIER_START_RED"),
                    modifier("EMISSION_MODIFIER_START_GREEN"),
                    modifier("EMISSION_MODIFIER_START_BLUE"),
                ],
                [
                    modifier("EMISSION_MODIFIER_END_RED"),
                    modifier("EMISSION_MODIFIER_END_GREEN"),
                    modifier("EMISSION_MODIFIER_END_BLUE"),
                ],
            ],
            velocity: Variance::parse(get(mov, "velocity_range")).unwrap_or_default(),
            dampening: Variance::parse(get(mov, "dampening_range")).unwrap_or_default(),
            direction: dir,
            conic_fan: Variance::parse(get(mov, "conic_range_fan")).unwrap_or_default(),
            conic_depth: Variance::parse(get(mov, "conic_range_depth")).unwrap_or_default(),
            start_frame_variation: Variance::parse(get(anim, "start_frame_variation")).unwrap_or_default(),
            animation_length: Variance::parse(get(anim, "animation_length")).unwrap_or_default(),
            loop_count: Variance::parse(get(anim, "loop_count_range")).unwrap_or_default(),
            total_frames: Variance::parse(get(anim, "total_frames")).unwrap_or_default(),
            cell_width: get(anim, "cell_width").parse().unwrap_or(0),
            cell_height: get(anim, "cell_height").parse().unwrap_or(0),
            start_colour: colour(col, "start"),
            end_colour: colour(col, "end"),
            colour_alpha: Variance::parse(get(col, "colour_range_a")).unwrap_or_default(),
            start_channels_linked: link(col, "start"),
            end_channels_linked: link(col, "end"),
            fade_in: Variance::parse(get(fade, "fadein_range_life_unary")).unwrap_or_default(),
            fade_out: Variance::parse(get(fade, "fadeout_range_life_unary")).unwrap_or_default(),
            initial_scale: VarVector::parse(get(scale, "initial_scale_range_metres")).unwrap_or_default(),
            primary_scale_life: Variance::parse(get(scale, "primary_scale_life_unary_range")).unwrap_or_default(),
            primary_target_scale: VarVector::parse(get(scale, "primary_target_scale_range_metres")).unwrap_or_default(),
            secondary_scale_life: Variance::parse(get(scale, "secondary_scale_life_unary_range")).unwrap_or_default(),
            secondary_target_scale: VarVector::parse(get(scale, "secondary_target_scale_range_metres")).unwrap_or_default(),
            rotation: Variance::parse(get(rot, "rotation_range")).unwrap_or_default(),
            rotations_per_second: Variance::parse(get(rot, "rotations_per_second_range")).unwrap_or_default(),
            life_range: Variance::parse(attrs.attr("life_range").unwrap_or_default()).unwrap_or_default(),
            ribbon_life_range: Variance::parse(attrs.attr("ribbon_life_range").unwrap_or_default()).unwrap_or_default(),
            radial_blur_strength: attrs.attr_f32("radial_blur_strength").unwrap_or(0.0),
            radial_blur_min_distance: attrs.attr_f32("radial_blur_min_distance").unwrap_or(0.0),
            radial_blur_max_distance: attrs.attr_f32("radial_blur_max_distance").unwrap_or(0.0),
            textures: std::array::from_fn(|i| get(Some(rend), &format!("texture_{}", i + 1)).to_owned()),
            fx: get(Some(rend), "fx").to_owned(),
            render_method: rend.attr("render_method").and_then(RenderMethod::parse).unwrap_or_default(),
            align_to_velocity: rend.attr("align_to_velocity").is_some_and(|s| s.eq_ignore_ascii_case("true")),
            sprite_facing: match rend.attr("sprite_facing_mode").unwrap_or_default() {
                "CAMERA_FACING" => FacingMode::Camera,
                "BILLBOARD" => FacingMode::Billboard,
                "LOCAL_Y_AXIS" => FacingMode::LocalYAxis,
                "WORLD_Y_AXIS" => FacingMode::WorldYAxis,
                "VELOCITY_FACING" => FacingMode::Velocity,
                _ => FacingMode::Other,
            },
            max_effects: get(Some(rend), "max_effects").parse().unwrap_or(0),
            max_num_quads: get(Some(rend), "max_num_quads").parse().unwrap_or(0),
        })
    }
}

/// One `SCRIPTED_EFFECT_GROUP`: the unit the engine plays. Its entries name single emitters, in
/// file order (INFERRED to be play order); a name may repeat (CONFIRMED: `AirExplosion_lrg` lists
/// `sparks_airburst` twice, `fx_probe::tests::the_three_air_burst_groups_are_one_group`, install).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectGroup {
    /// `name` attribute.
    pub name: String,
    /// `EFFECT_GROUP_AMBIENT_SETTINGS/@spawn_interval` (seconds).
    pub spawn_interval: f32,
    /// `@cell_size` / `@cell_count` (the grid the group spreads its emitters over).
    pub cell_size: f32,
    pub cell_count: f32,
    /// `@wind_offset`.
    pub wind_offset: f32,
    /// The emitter names, in order (duplicates kept).
    pub entries: Vec<String>,
}

impl EffectGroup {
    fn from_xml(e: &XmlElement) -> Option<Self> {
        let amb = e.child("EFFECT_GROUP_AMBIENT_SETTINGS");
        let g = |n: &str| amb.and_then(|a| a.attr_f32(n)).unwrap_or(0.0);
        Some(Self {
            name: e.attr("name").unwrap_or_default().to_owned(),
            spawn_interval: g("spawn_interval"),
            cell_size: g("cell_size"),
            cell_count: g("cell_count"),
            wind_offset: g("wind_offset"),
            entries: e
                .children_named("SCRIPTED_EFFECT_GROUP_ENTRY")
                .filter_map(|c| c.attr("effect"))
                .map(str::to_owned)
                .collect(),
        })
    }
}

/// The whole effect database of one file (`effects\landbattle.xml` and its siblings).
#[derive(Debug, Clone, Default)]
pub struct EffectLibrary {
    /// Every `SCRIPTED_EFFECT_INFO`, by name.
    pub effects: BTreeMap<String, Effect>,
    /// Every `SCRIPTED_EFFECT_GROUP`, by name.
    pub groups: BTreeMap<String, EffectGroup>,
}

/// Pack path of the land-battle effect definitions.
pub const LAND_BATTLE_EFFECTS: &str = r"effects\landbattle.xml";
/// Pack path of the naval effect definitions.
pub const NAVAL_BATTLE_EFFECTS: &str = r"effects\navalbattle.xml";
/// Pack path of the campaign-map effect definitions.
pub const CAMPAIGN_MAP_EFFECTS: &str = r"effects\campaignmap.xml";
/// Pack path of the per-entity dust frequency table.
pub const UNIT_DUST_PARAMETERS: &str = r"effects\unit_dust_parameters.txt";

impl EffectLibrary {
    /// Parses an `EFFECTS_MANAGER` document (`effects\landbattle.xml`).
    pub fn read(bytes: &[u8]) -> Result<Self, xml::XmlError> {
        Self::from_root(&xml::parse_bytes(bytes)?)
    }

    /// Reads one of the three shipped effect files by its pack path.
    pub fn from_vfs_path(vfs: &crate::pack::Vfs, path: &str) -> Result<Self, String> {
        Self::read(&vfs.read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    /// Reads the land-battle effect file from a pack Vfs.
    pub fn from_vfs(vfs: &crate::pack::Vfs) -> Result<Self, String> {
        Self::from_vfs_path(vfs, LAND_BATTLE_EFFECTS)
    }

    /// Reads `effects\unit_dust_parameters.txt` from a pack Vfs.
    pub fn dust_from_vfs(vfs: &crate::pack::Vfs) -> Option<DustParameters> {
        vfs.read(UNIT_DUST_PARAMETERS).ok().map(|b| DustParameters::parse(&b))
    }

    /// Collects the emitters and groups out of a parsed document.
    pub fn from_root(root: &XmlElement) -> Result<Self, xml::XmlError> {
        let mut effects = BTreeMap::new();
        let mut groups = BTreeMap::new();
        let mut skipped = 0usize;
        if let Some(list) = root.find("SCRIPTED_EFFECT_INFO_LIST") {
            for e in list.children_named("SCRIPTED_EFFECT_INFO") {
                match Effect::from_xml(e) {
                    Some(fx) => {
                        effects.insert(fx.name.clone(), fx);
                    }
                    None => skipped += 1,
                }
            }
        }
        if let Some(gl) = root.find("SCRIPTED_EFFECT_GROUP_INFO") {
            for g in gl.children_named("SCRIPTED_EFFECT_GROUP") {
                if let Some(g) = EffectGroup::from_xml(g) {
                    groups.insert(g.name.clone(), g);
                }
            }
        }
        if effects.is_empty() && groups.is_empty() {
            return Err(xml::XmlError("no SCRIPTED_EFFECT_INFO or SCRIPTED_EFFECT_GROUP elements".into()));
        }
        let _ = skipped; // counted for the notes; no shipped row fails to parse.
        Ok(Self { effects, groups })
    }

    /// The emitters a group plays, in order. Names not in `effects` are skipped (a mod may add
    /// groups that reference effects another mod removed).
    pub fn group_effects(&self, group: &str) -> Vec<&Effect> {
        self.groups
            .get(group)
            .map(|g| g.entries.iter().filter_map(|n| self.effects.get(n)).collect())
            .unwrap_or_default()
    }

    /// True when `name` is a group in this library.
    pub fn has_group(&self, name: &str) -> bool {
        self.groups.contains_key(name)
    }
}

/// `effects\unit_dust_parameters.txt`: how often each entity type puffs dust, keyed by
/// `// type` name (CONFIRMED, whole shipped file).
#[derive(Debug, Clone, Default)]
pub struct DustParameters {
    /// `entity` name -> `entity frequency`. The file is tab-separated with `\r\n` line endings.
    pub frequency: BTreeMap<String, f32>,
}

impl DustParameters {
    /// Parses the file's bytes.
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut frequency = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.trim_start().starts_with("//") || line.starts_with("version") {
                continue;
            }
            let mut parts = line.split('\t').filter(|p| !p.trim().is_empty());
            let (Some(name), Some(freq)) = (parts.next(), parts.next()) else { continue };
            if let Ok(f) = freq.trim().parse::<f32>() {
                frequency.insert(name.trim().to_ascii_lowercase(), f);
            }
        }
        Self { frequency }
    }

    /// The dust frequency of an entity type, e.g. `infantry_running` -> 0.10.
    pub fn frequency(&self, entity: &str) -> Option<f32> {
        self.frequency.get(&entity.to_ascii_lowercase()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `SCRIPTED_EFFECT_INFO` with every attribute the parser reads, in the file's own shape.
    const SAMPLE_EFFECT: &str = r#"
<SCRIPTED_EFFECT_INFO name='sample_smoke' num_particles_per_point='2'
    gravity_range='variance(0.000000,0.000000)' min_lod_range='variance(50.000000,0.000000)'
    max_lod_range='variance(1000.000000,0.000000)' wind_range='variance(0.500000,0.400000)'
    thickness='variance(1.000000,0.000000)' lighting='variance(1.000000,0.000000)'
    adjust_direction_by_offset_position='false' clamp_sea_level='false' quality_level='2'>
  <SCRIPTED_EFFECT_EMISSION_CONTROL name='Emission Control' emission_type='EMISSION_TYPE_POINT'
      emission_time_range_seconds='variance(5.000000,0.000000)'>
    <RELEASE_INFO name='Release Info' release_type='RELEASE_TYPE_NONE'
        release_interval_range='variance(0.000000,0.000000)'
        release_position_variation_range='vector(variance(0.000000,5.000000),variance(0.000000,25.000000))' />
    <EMISSION_MODIFIERS name='Emission Modifiers'>
      <EMISSION_MODIFIER_VELOCITY name='velocityModifier' type='EMISSION_MODIFIER_TYPE_NONE' value='0.800000' />
      <EMISSION_MODIFIER_LIFETIME name='lifetimeModifier' type='EMISSION_MODIFIER_TYPE_NONE' value='1.000000' />
      <EMISSION_MODIFIER_OPACITY name='opacityModifier' type='EMISSION_MODIFIER_TYPE_NONE' value='0.000000' />
    </EMISSION_MODIFIERS>
  </SCRIPTED_EFFECT_EMISSION_CONTROL>
  <SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES name='Attribute Ranges' ribbon_life_range='variance(2.000000,0.000000)'
      radial_blur_strength='0.000000' life_range='variance(2.000000,0.500000)'>
    <MOVEMENT_INFO name='Movement' velocity_range='variance(1.500000,0.250000)' dampening_range='variance(0.200000,0.000000)'
        dir_range_XYZ='vector(0.000000,1.000000,0.000000)' conic_range_fan='variance(0.000000,0.300000)'
        conic_range_depth='variance(0.000000,0.100000)' />
    <ANIMATION_INFO name='Animation' start_frame_variation='variance(0,16)' animation_length='variance(0,0)'
        loop_count_range='variance(1,0)' total_frames='variance(16,0)' cell_width='128' cell_height='128' />
    <COLOUR_INFO name='Colour' start_linked_channels='NO_CHANNELS_LINKED' start_colour_range_r='variance(60,0)'
        start_colour_range_g='variance(40,0)' start_colour_range_b='variance(20,0)' end_linked_channels='NO_CHANNELS_LINKED'
        end_colour_range_r='variance(60,0)' end_colour_range_g='variance(40,0)' end_colour_range_b='variance(20,0)'
        colour_range_a='variance(60,0)' />
    <FADE_INFO name='Fade' fadein_range_life_unary='variance(0.400000,0.000000)' fadeout_range_life_unary='variance(0.600000,0.000000)' />
    <SCALE_INFO name='Scale' initial_scale_range_metres='vector(variance(10.000000,20.000000),variance(2.000000,2.000000))'
        primary_scale_life_unary_range='variance(0.500000,0.000000)'
        primary_target_scale_range_metres='vector(variance(10.000000,20.000000),variance(2.000000,2.000000))'
        secondary_scale_life_unary_range='variance(1.000000,0.000000)' />
    <ROTATION_INFO name='Rotation' rotation_range='variance(2.500000,0.000000)' rotations_per_second_range='variance(0.000000,0.000000)' />
  </SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES>
  <SCRIPTED_EFFECT_RENDERING_VARS name='Rendering Vars' texture_1='effects\textures\emp_animsmoke1_diffuse.dds'
      texture_2='effects\textures\emp_animsmoke1_normal.dds' fx='particle.fx' render_method='RENDER_METHOD_ALPHA'
      align_to_velocity='false' sprite_facing_mode='CAMERA_FACING' max_effects='50' max_num_quads='1000' />
</SCRIPTED_EFFECT_INFO>"#;

    #[test]
    fn a_none_modifier_leaves_its_attribute_alone() {
        // This is the load-bearing rule: 277 of the 283 shipped lifetime modifiers are
        // `NONE` with value 0.0, so read as a multiplier every particle would get no life.
        let none = Modifier { kind: ModifierType::None, value: VarVector::exact([0.0; 3]) };
        assert_eq!(none.scalar(2.0), 2.0);
        let add = Modifier { kind: ModifierType::Add, value: VarVector::exact([1.5, 0.0, 0.0]) };
        assert!((add.scalar(2.0) - 3.5).abs() < 1e-6);
        let mul = Modifier { kind: ModifierType::Mul, value: VarVector::exact([0.8, 0.0, 0.0]) };
        assert!((mul.scalar(2.0) - 1.6).abs() < 1e-6);
        // A default modifier is inert, so a missing element is harmless.
        assert_eq!(Modifier::default().scalar(7.0), 7.0);
        // Sizes and colours go through the same rule.
        let size = VarVector::exact([1.0, 2.0, 0.0]);
        assert_eq!(none.size(size), size);
        let scaled = add.size(size);
        assert!((scaled.c[0].base - 2.5).abs() < 1e-6 && (scaled.c[1].base - 2.0).abs() < 1e-6, "only x is written");
        assert_eq!(none.channel(Variance { base: 255.0, var: 3.0 }), Variance { base: 255.0, var: 3.0 });
    }

    #[test]
    fn modifier_values_are_vectors_or_a_single_number() {
        // The three scale modifiers write a vector; everything else writes a bare number.
        let vec = modifier_value("vector(variance(0.100000,0.100000),variance(0.000000,0.000000))");
        assert_eq!(vec.len, 2);
        assert_eq!(vec.c[0], Variance { base: 0.1, var: 0.1 });
        let one = modifier_value("0.800000");
        assert_eq!(one.len, 1);
        assert_eq!(one.c[0], Variance::exact(0.8));
        assert_eq!(one.c[1], Variance::default(), "only x is written");
        assert_eq!(modifier_value("variance(0.5,0.1)").c[0], Variance { base: 0.5, var: 0.1 });
        // An absent value is inert, not "one component of zero".
        let empty = modifier_value("");
        assert_eq!(empty.c, [Variance::default(); 3]);
    }

    #[test]
    fn modifier_types_parse() {
        assert_eq!(ModifierType::parse("EMISSION_MODIFIER_TYPE_NONE"), Some(ModifierType::None));
        assert_eq!(ModifierType::parse("EMISSION_MODIFIER_TYPE_ADD"), Some(ModifierType::Add));
        assert_eq!(ModifierType::parse("EMISSION_MODIFIER_TYPE_MUL"), Some(ModifierType::Mul));
        assert_eq!(ModifierType::parse("EMISSION_MODIFIER_TYPE_SOMETHING"), None);
    }

    #[test]
    fn variance_forms() {
        assert_eq!(Variance::parse("0.5").unwrap(), Variance::exact(0.5));
        assert_eq!(Variance::parse("variance(0.010000,0.005000)").unwrap(), Variance { base: 0.01, var: 0.005 });
        // A negative spread is taken absolute, so a draw can never invert.
        assert_eq!(Variance::parse("variance(2.0,-1.0)").unwrap(), Variance { base: 2.0, var: 1.0 });
        assert!(Variance::parse("variance(1.0)").is_none());
        assert!(Variance::parse("nonsense").is_none());
    }

    #[test]
    fn vector_forms() {
        let v = VarVector::parse("vector(1.0,2.0)").unwrap();
        assert_eq!(v.len, 2);
        assert_eq!(v.c[0], Variance::exact(1.0));
        assert_eq!(v.c[1], Variance::exact(2.0));
        assert_eq!(v.c[2], Variance::default());
        let w = VarVector::parse("vector(variance(0.000000,5.000000),variance(0.000000,25.000000),variance(2.000000,25.000000))").unwrap();
        assert_eq!(w.len, 3);
        assert_eq!(w.c[0], Variance { base: 0.0, var: 5.0 });
        assert_eq!(w.c[2], Variance { base: 2.0, var: 25.0 });
        // The commas inside variance(...) do not split the vector.
        assert_eq!(VarVector::parse("vector(1.0,variance(2.0,3.0))").unwrap().c[1], Variance { base: 2.0, var: 3.0 });
    }

    #[test]
    fn reads_one_effect() {
        let fx = Effect::from_xml(&xml::parse_bytes(SAMPLE_EFFECT.as_bytes()).unwrap()).unwrap();
        assert_eq!(fx.name, "sample_smoke");
        assert_eq!(fx.num_particles_per_point, 2);
        assert_eq!(fx.gravity, Variance::exact(0.0));
        assert_eq!(fx.wind, Variance { base: 0.5, var: 0.4 });
        assert_eq!(fx.quality_level, 2);
        assert_eq!(fx.emission_type, EmissionType::Point);
        assert_eq!(fx.release_type, "RELEASE_TYPE_NONE");
        assert_eq!(fx.release_position_variation.c[1], Variance { base: 0.0, var: 25.0 });
        assert_eq!(fx.velocity_modifier.kind, ModifierType::None);
        assert_eq!(fx.velocity_modifier.value.c[0].base, 0.8, "the value is kept but inert");
        assert!((fx.velocity(5.0) - 5.0).abs() < 1e-6, "a NONE modifier leaves the speed alone");
        assert_eq!(fx.lifetime_modifier.kind, ModifierType::None);
        assert!((fx.life_seconds(2.0) - 2.0).abs() < 1e-6);
        assert_eq!(fx.velocity, Variance { base: 1.5, var: 0.25 });
        assert_eq!(fx.direction, [0.0, 1.0, 0.0]);
        assert_eq!(fx.conic_fan, Variance { base: 0.0, var: 0.3 });
        assert_eq!(fx.total_frames, Variance::exact(16.0));
        assert_eq!(fx.frames(), 16);
        assert_eq!(fx.cell_width, 128);
        assert_eq!(fx.start_colour[0], Variance::exact(60.0));
        assert_eq!(fx.end_channels_linked, ChannelLink::None);
        assert_eq!(fx.fade_in.base, 0.4);
        assert_eq!(fx.fade_out.base, 0.6);
        assert_eq!(fx.initial_scale.len, 2);
        assert_eq!(fx.initial_scale.c[1], Variance { base: 2.0, var: 2.0 });
        assert_eq!(fx.rotation.base, 2.5);
        assert_eq!(fx.texture(), r"effects\textures\emp_animsmoke1_diffuse.dds");
        assert_eq!(fx.fx, "particle.fx");
        assert_eq!(fx.render_method, RenderMethod::Alpha);
        assert_eq!(fx.sprite_facing, FacingMode::Camera);
        assert_eq!(fx.max_num_quads, 1000);
        // The lifetime modifier is NONE in the sample, so life_range is the life.
        assert_eq!(fx.life_range().base, 2.0);
        assert!((fx.life_seconds(fx.life_range().base) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn reads_a_whole_document_with_groups() {
        let flash = SAMPLE_EFFECT.replace("sample_smoke", "flash");
        let doc = format!(
            "<EFFECTS_MANAGER name='Effects Manager'><SCRIPTED_EFFECT_INFO_LIST name='Parsed Datasets'>\
             {SAMPLE_EFFECT}{flash}</SCRIPTED_EFFECT_INFO_LIST><SCRIPTED_EFFECT_GROUP_INFO name='Scripted effect groups'>\
             <SCRIPTED_EFFECT_GROUP name='Fire'><EFFECT_GROUP_AMBIENT_SETTINGS spawn_interval='100.000000' cell_size='10.000000' \
             cell_count='1.000000' wind_offset='0.000000' />\
             <SCRIPTED_EFFECT_GROUP_ENTRY name='sample_smoke' effect='sample_smoke' />\
             <SCRIPTED_EFFECT_GROUP_ENTRY name='flash' effect='flash' /></SCRIPTED_EFFECT_GROUP>\
             </SCRIPTED_EFFECT_GROUP_INFO></EFFECTS_MANAGER>",
        );
        let lib = EffectLibrary::read(doc.as_bytes()).unwrap();
        assert_eq!(lib.effects.len(), 2);
        assert_eq!(lib.groups.len(), 1);
        let g = &lib.groups["Fire"];
        assert_eq!(g.entries, vec!["sample_smoke".to_string(), "flash".to_string()]);
        assert_eq!(g.cell_size, 10.0);
        assert_eq!(lib.group_effects("Fire").len(), 2);
        assert!(lib.has_group("Fire"));
        assert!(lib.group_effects("nope").is_empty());
    }

    #[test]
    fn rejects_a_document_without_effects() {
        assert!(EffectLibrary::read(b"<EFFECTS_MANAGER></EFFECTS_MANAGER>").is_err());
    }

    #[test]
    fn dust_parameters() {
        let p = DustParameters::parse(
            b"version 1.0\n\r\n// type\t\t\t\tentity frequency\r\ninfantry_walking\t0.05\r\ncavalry_running\t\t0.15\r\n",
        );
        assert_eq!(p.frequency("infantry_walking"), Some(0.05));
        assert_eq!(p.frequency("cavalry_running"), Some(0.15));
        assert_eq!(p.frequency("INFANTRY_WALKING"), Some(0.05));
        assert_eq!(p.frequency("elephants_melee"), None);
        assert_eq!(p.frequency.len(), 2);
    }
}

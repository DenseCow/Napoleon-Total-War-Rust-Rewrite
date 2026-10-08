//! Battle maps from `battleterrain.pack`: heightfields, map metadata, texture layers,
//! deployment zones and placed objects.
//!
//! Research notes: `analysis/worker5/BATTLE_TERRAIN.md` (file inventory) and
//! `analysis/worker5/BATTLE_TERRAIN.md` §8 (this reader, coordinate mapping, open questions).
//! Tags: **CONFIRMED** = checked on the bytes of every shipped preset (see the `#[ignore]`
//! install test), **INFERRED** = consistent with the data but the meaning is our reading,
//! **UNKNOWN** = not worked out.
//!
//! # A preset folder: `battleterrain\presets\<map>\`
//! | file | reader |
//! |---|---|
//! | `definition.xml` (`BATTLE_MAP_DEFINITION`) | [`MapDefinition`] |
//! | `textures.xml` (`BMD_TEXTURES`) | [`MapTextures`] |
//! | `weather.xml` (`WEATHER`) + the `*.environment` it names (`SCENE/LIGHTING`) | [`Weather`], [`Lighting`] |
//! | `height_map_N.dds` (L16) + `height_map_N_settings.xml` | [`Heightfield`] |
//! | `deployment_areas.xml` | [`DeploymentSetup`] |
//! | `bmd_near_buildings.building_list`, `bmd_far_buildings.building_list` (ESF) | [`BuildingPlacement`] |
//! | `bmd.tree_list` (ESF) | [`TreeList`] |
//! | `non_terrain_outlines.xml` | [`BattleMap::outlines`] |
//! | `ground_type_map_0.tga` | [`GroundTypeMap`] |
//! | `colour_map_N.jpg` (+`_alpha`), `blendmap.jpg` (+`_alpha`) | paths only ([`BattleMap::colour_map_path`]); JPEG decoding is left to the renderer |
//!
//! # Coordinates (see [`Heightfield::height_at`])
//! Every position in these files is a **map position `(x, y)` in metres, origin at the map
//! centre** (CONFIRMED: deployment centres, building and tree positions all fall inside
//! ±`world_width / 2`). The game's 3D world is Direct3D style, Y up, and the shaders
//! (`fx\terrain_shared.fx_fragment`, CONFIRMED as text) look terrain textures up with
//! `uv = 0.5 + world.xz / 2048` (so texture column grows with world x).
//!
//! Grid layout (**CONFIRMED statistically** on 4 maps with `examples/terrain_orient.rs`: of the 8
//! possible grid orientations only this one puts buildings on flat ground and makes the
//! ground types under trees stand out):
//! - column `c` ↔ `x = -W/2 + c·W/(w-1)` (x grows to the right of the picture);
//! - row `r` ↔ `y = +H/2 - r·H/(h-1)` (row 0, the top of the picture, is the `+y` edge:
//!   "north up" like a map);
//! - the same holds for `ground_type_map_0.tga` (after the decoder puts its rows top-down).
//!
//! How map `y` relates to the D3D world `z` axis (the shader works in world `xz`) is UNKNOWN:
//! either `z = y` and the engine flips the textures when loading, or `z = -y`. For Bevy we use
//! map `(x, y)` → Bevy `(x, height, -y)`, which keeps `+y` "north" pointing away from a default camera.

use std::fmt;

use crate::dds::{Dds, DdsFormat};
use crate::esf::{EsfFile, EsfNode, EsfRecord};
use crate::pack::Vfs;
use crate::tga::Tga;
use crate::xml::{self, XmlElement};

/// Folder holding every preset battle map (normalized form).
pub const PRESETS_DIR: &str = r"battleterrain\presets\";

/// Why a battle map could not be read.
#[derive(Debug)]
pub enum TerrainError {
    /// A required file is missing.
    Missing(String),
    /// A file exists but could not be parsed.
    Bad(String, String),
}

impl fmt::Display for TerrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(p) => write!(f, "missing battle-map file {p}"),
            Self::Bad(p, why) => write!(f, "bad battle-map file {p}: {why}"),
        }
    }
}

impl std::error::Error for TerrainError {}

fn bad(path: &str, why: impl fmt::Display) -> TerrainError {
    TerrainError::Bad(path.to_owned(), why.to_string())
}

/// The sea surface height in map metres (**CONFIRMED**, see [`SEA_LEVEL_NOTE`]).
pub const SEA_LEVEL: f32 = 0.0;

/// Why the sea surface sits at [`SEA_LEVEL`], and what is not known about it.
///
/// **CONFIRMED** that the sea surface is at y = 0 in map metres, from two independent sources on
/// every shipped preset (`examples/water_probe.rs`, all 60 maps):
/// - the four all-sea naval presets (`caribbean`, `hb_naval`, `hb_nile`, `hb_trafalgar`) have a
///   **flat** level-0 heightfield at exactly −100 m (`scale` 100, `bias` −100, every sample 0), so
///   the surface cannot be at the sea bed, and the only other round number in range is 0;
/// - on every preset that marks water in `ground_type_map_0.tga` (`water_deep` 17,
///   `water_shallow` 20, `water_medium_ford` 19) the **highest** water cell is at ≈ 0 m
///   (hb_toulon −0.00, hb_pyramids −0.03, hb_arcole +0.11, rti_fort_1…9 −0.03…+4.02), and the share
///   of cells under y = 0 matches the share of typed water to within 0.5–2 % (hb_toulon 47.66 % vs
///   47.50 %, hb_pyramids 15.90 % vs 15.50 %, rti_fort_5 38.52 % vs 38.01 %, hb_waterloo 0 % vs 0 %).
/// - the maps with no water in the ground-type map but ground below 0 (`nap_mp_gorge_[sa]` 26 %,
///   `nap_mp_twin_hills_[sa]` 11 %, `nap_mp_coastal_[sa]` 23 %) share **one identical**
///   ground-type map (the same histogram on all 16 `[sa]` maps), i.e. it is a placeholder, so those
///   basins read as sea on the height rule alone. INFERRED that the original floods them the same
///   way (it draws one flat sea plane).
///
/// **UNKNOWN / deliberately not decided here:**
/// - inland water bodies at their own level (the lake of `nap_mp_lakeside_[sa]` reaches +105.8 m
///   and its typed water sits at +22.9 m on average, `nap_mp_valley_[sa]` +21.97 m, the Adda at
///   hb_lodi +24.37 m). Those are rivers and lakes, not the sea, and the original's per-body level
///   is in no map file (no `sea_level` in `definition.xml`, `weather.xml` or `*.environment`;
///   searched every preset).
/// - how the original clips the sea to the map (the `ocean.fx` stencil techniques
///   `sm_render_sea_stenciling_*` suggest a stencil pass against the terrain).
pub const SEA_LEVEL_NOTE: &str = "see analysis/graphics/WATER.md";

// ---------------------------------------------------------------------------------------------
// definition.xml / textures.xml / weather.xml / *.environment
// ---------------------------------------------------------------------------------------------

/// `definition.xml`: root `BATTLE_MAP_DEFINITION` (CONFIRMED attribute names).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapDefinition {
    /// `bmd_type` (UNKNOWN enum; 0 on land presets).
    pub bmd_type: i64,
    /// `additive_type` (UNKNOWN enum).
    pub additive_type: i64,
    /// `terrain_type` (UNKNOWN enum).
    pub terrain_type: i64,
    /// Climate key, e.g. `lc_tundra` (matches `battle_terrain_set_climates_jcts`, INFERRED).
    pub climate: String,
    /// Season key, e.g. `season_winter`.
    pub season: String,
    /// Playable terrain size in metres, e.g. 2048 × 2048.
    pub base_terrain_width: f32,
    /// See `base_terrain_width`.
    pub base_terrain_height: f32,
    /// Shrub density multiplier.
    pub shrub_density: f32,
    /// Tree density multiplier.
    pub tree_density: f32,
    /// Subculture key, e.g. `sc_european_east`.
    pub subculture: String,
    /// `simple_colour_blending`.
    pub simple_colour_blending: bool,
}

impl MapDefinition {
    /// Reads the `BATTLE_MAP_DEFINITION` element.
    pub fn from_xml(e: &XmlElement) -> Self {
        let s = |k: &str| e.attr(k).unwrap_or_default().to_owned();
        Self {
            bmd_type: e.attr_i64("bmd_type").unwrap_or(0),
            additive_type: e.attr_i64("additive_type").unwrap_or(0),
            terrain_type: e.attr_i64("terrain_type").unwrap_or(0),
            climate: s("climate"),
            season: s("season"),
            base_terrain_width: e.attr_f32("base_terrain_width").unwrap_or(0.0),
            base_terrain_height: e.attr_f32("base_terrain_height").unwrap_or(0.0),
            shrub_density: e.attr_f32("shrub_density").unwrap_or(1.0),
            tree_density: e.attr_f32("tree_density").unwrap_or(1.0),
            subculture: s("subculture"),
            simple_colour_blending: e.attr_bool("simple_colour_blending").unwrap_or(false),
        }
    }
}

/// `textures.xml`: root `BMD_TEXTURES`. Paths are pack paths **without** the `.dds` extension,
/// e.g. `BattleTerrain/tiled_maps/snow1` (CONFIRMED). Detail maps resolve to
/// `<name>_diffuse.dds` (CONFIRMED: `detail_maps\cold_l0_diffuse.dds` exists, `cold_l0.dds` does not).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MapTextures {
    /// Grass type key, e.g. `Alpine_winter` (INFERRED: a `battleterrain\grass` set).
    pub grass_type: String,
    /// The tiled ground texture (`t_detail_colour_map` in the shaders, INFERRED).
    pub tiled_detail_map: String,
    /// Tiled texture used under farm fields.
    pub farm_tiled_detail_map: String,
    /// Cliff texture path.
    pub cliff_map: String,
    /// Rock texture path.
    pub rock_map: String,
    /// Forest underlay texture path.
    pub forest_underlay: String,
    /// `DETAIL_MAP id name` entries (4 in every preset: one per heightfield level, INFERRED).
    pub detail_maps: Vec<(u32, String)>,
}

impl MapTextures {
    /// Reads the `BMD_TEXTURES` element.
    pub fn from_xml(e: &XmlElement) -> Self {
        let s = |k: &str| e.attr(k).unwrap_or_default().to_owned();
        Self {
            grass_type: s("grass_type"),
            tiled_detail_map: s("tiled_detail_map"),
            farm_tiled_detail_map: s("farm_tiled_detail_map"),
            cliff_map: s("cliff_map"),
            rock_map: s("rock_map"),
            forest_underlay: s("forest_underlay"),
            detail_maps: e
                .children_named("DETAIL_MAP")
                .map(|d| (d.attr_i64("id").unwrap_or(0) as u32, d.attr("name").unwrap_or_default().to_owned()))
                .collect(),
        }
    }
}

/// A texture path from `textures.xml` (no extension) → the `.dds` pack path.
pub fn texture_dds_path(name: &str) -> String {
    format!("{name}.dds")
}

/// `weather.xml`: root `WEATHER`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Weather {
    /// `has_ambient_fog`.
    pub has_ambient_fog: bool,
    /// `heat_fatigue` (INFERRED: extra fatigue in hot weather).
    pub heat_fatigue: f32,
    /// `cold_fatigue`.
    pub cold_fatigue: f32,
    /// `max_weather_type_key`, e.g. `dry`.
    pub max_weather_type_key: String,
    /// Pack path of the `*.environment` file (lighting, fog, sky).
    pub environment_key: String,
}

impl Weather {
    /// Reads the `WEATHER` element.
    pub fn from_xml(e: &XmlElement) -> Self {
        Self {
            has_ambient_fog: e.attr_bool("has_ambient_fog").unwrap_or(false),
            heat_fatigue: e.attr_f32("heat_fatigue").unwrap_or(0.0),
            cold_fatigue: e.attr_f32("cold_fatigue").unwrap_or(0.0),
            max_weather_type_key: e.attr("max_weather_type_key").unwrap_or_default().to_owned(),
            environment_key: e.attr("environment_key").unwrap_or_default().to_owned(),
        }
    }
}

/// The parts of an `*.environment` file's `SCENE/LIGHTING` (and sky colour) a renderer needs.
/// Values are written like `euler(0.87,-1.24,0)` and `rgb(0.9,0.78,0.61)` (CONFIRMED syntax).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lighting {
    /// `light_direction` euler angles (radians). Axis order UNKNOWN (INFERRED: yaw, pitch, roll).
    pub light_direction_euler: [f32; 3],
    /// `light_colour` × `light_colour_scale` is the sun colour (INFERRED).
    pub light_colour: [f32; 3],
    /// `light_colour_scale`.
    pub light_colour_scale: f32,
    /// `ambient_cube_top` (the other 5 faces are not kept yet).
    pub ambient_top: [f32; 3],
    /// `ambient_cube_scale`.
    pub ambient_cube_scale: f32,
    /// `SKYGEN sky_colour`, if present.
    pub sky_colour: Option<[f32; 3]>,
    /// `SKYGEN sky_colour_scale` (the sky's own brightness, separate from `ambient_cube_scale`).
    pub sky_colour_scale: f32,
}

/// Parses `name(a,b,c[,d])` into its numbers.
pub fn parse_tuple(s: &str) -> Vec<f32> {
    let inner = s.split_once('(').map_or(s, |(_, r)| r).trim_end_matches(')');
    inner.split(',').filter_map(|v| v.trim().parse().ok()).collect()
}

fn tuple3(s: Option<&str>) -> Option<[f32; 3]> {
    let v = parse_tuple(s?);
    (v.len() >= 3).then(|| [v[0], v[1], v[2]])
}

impl Lighting {
    /// Reads `SCENE` (the root of an `.environment` file).
    pub fn from_scene(scene: &XmlElement) -> Option<Self> {
        let l = if scene.name.eq_ignore_ascii_case("LIGHTING") { scene } else { scene.find("LIGHTING")? };
        Some(Self {
            light_direction_euler: tuple3(l.attr("light_direction")).unwrap_or_default(),
            light_colour: tuple3(l.attr("light_colour")).unwrap_or([1.0; 3]),
            light_colour_scale: l.attr_f32("light_colour_scale").unwrap_or(1.0),
            ambient_top: tuple3(l.attr("ambient_cube_top")).unwrap_or([0.5; 3]),
            ambient_cube_scale: l.attr_f32("ambient_cube_scale").unwrap_or(1.0),
            sky_colour: scene.find("SKYGEN").and_then(|s| tuple3(s.attr("sky_colour"))),
            sky_colour_scale: scene.find("SKYGEN").and_then(|s| s.attr_f32("sky_colour_scale")).unwrap_or(1.0),
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Heightfields
// ---------------------------------------------------------------------------------------------

/// `HEIGHTFIELD_SETTINGS`, as XML attributes (presets) or an ESF record (tiles).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeightfieldSettings {
    /// Size along x in metres covered by the whole heightfield (first to last sample).
    pub world_width: f32,
    /// Size along y (world z) in metres.
    pub world_height: f32,
    /// `normalize`: true on all presets (samples use the full 0..65535 range).
    pub normalize: bool,
    /// `simple_colour_blending`.
    pub simple_colour_blending: bool,
    /// Height range in metres (see [`Heightfield::sample_m`]).
    pub scale: f32,
    /// Height offset in metres.
    pub bias: f32,
}

impl HeightfieldSettings {
    /// Reads the `HEIGHTFIELD_SETTINGS` XML element.
    pub fn from_xml(e: &XmlElement) -> Self {
        Self {
            world_width: e.attr_f32("world_width").unwrap_or(0.0),
            world_height: e.attr_f32("world_height").unwrap_or(0.0),
            normalize: e.attr_bool("normalize").unwrap_or(true),
            simple_colour_blending: e.attr_bool("simple_colour_blending").unwrap_or(false),
            scale: e.attr_f32("scale").unwrap_or(1.0),
            bias: e.attr_f32("bias").unwrap_or(0.0),
        }
    }

    /// Reads the ESF record `HEIGHTFIELD_SETTINGS` v2 of a tile's `height_map_0.settings`.
    /// Child order (CONFIRMED types): f32, f32, bool, f32, f32, bool. Field meaning INFERRED
    /// from the XML attribute order: world_width, world_height, normalize, scale, bias,
    /// simple_colour_blending.
    pub fn from_esf(r: &EsfRecord) -> Option<Self> {
        Some(Self {
            world_width: r.get_f32(0)?,
            world_height: r.get_f32(1)?,
            normalize: r.get_bool(2)?,
            scale: r.get_f32(3)?,
            bias: r.get_f32(4)?,
            simple_colour_blending: r.get_bool(5).unwrap_or(false),
        })
    }
}

/// One L16 heightfield: `width × height` samples on a regular grid with samples on the cell
/// corners (1025 = 2¹⁰ + 1 per side on presets, INFERRED).
#[derive(Debug, Clone, PartialEq)]
pub struct Heightfield {
    /// Samples per row.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Raw 16-bit samples, row-major as stored in the DDS (row 0 first).
    pub samples: Vec<u16>,
    /// Size, scale and bias.
    pub settings: HeightfieldSettings,
    /// Raw-sample span that `scale` covers: the level's own `max - min` when `normalize`
    /// (the exe's rescale, see [`Heightfield::sample_m`]), else 65535.
    pub span: f32,
}

/// See [`Heightfield::span`]. A flat normalised level keeps 65535 (the exe then divides by 1
/// in its float units, i.e. by the full range).
pub fn sample_span(samples: &[u16], normalize: bool) -> f32 {
    if !normalize {
        return 65535.0;
    }
    let (lo, hi) = samples.iter().fold((u16::MAX, 0u16), |(a, b), &s| (a.min(s), b.max(s)));
    if hi > lo { f32::from(hi - lo) } else { 65535.0 }
}

impl Heightfield {
    /// Builds a heightfield from an L16 `.dds` (16-bit luminance, no compression, CONFIRMED).
    pub fn from_dds(bytes: &[u8], settings: HeightfieldSettings) -> Result<Self, String> {
        let dds = Dds::parse(bytes).map_err(|e| e.to_string())?;
        if !matches!(dds.format, DdsFormat::Rgb { bits: 16, .. }) {
            return Err(format!("not a 16-bit heightfield: {:?}", dds.format));
        }
        let data = dds.level_data(0);
        let samples: Vec<u16> = data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let span = sample_span(&samples, settings.normalize);
        Ok(Self { width: dds.width, height: dds.height, samples, settings, span })
    }

    /// One raw sample, clamped to the grid.
    pub fn raw(&self, col: i64, row: i64) -> u16 {
        let c = col.clamp(0, self.width as i64 - 1) as usize;
        let r = row.clamp(0, self.height as i64 - 1) as usize;
        self.samples[r * self.width as usize + c]
    }

    /// Height in metres of one sample (clamped to the grid):
    /// `sample / span · scale + bias`.
    ///
    /// From the exe's loader (`0x00ED11F0`, `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md`
    /// §5.1): a `normalize` level is rescaled by its own sample range, `scale / (max - min)`,
    /// offset by `bias + min` (CONFIRMED structure; the per-sample `h · s + b` step is
    /// INFERRED). Every shipped level has `min = 0` and `max` 65534 or 65535 (install test
    /// `every_battle_map_parses`), so the min term vanishes and only the divisor differs
    /// from a plain `/ 65535` (by at most one sample step).
    pub fn sample_m(&self, col: i64, row: i64) -> f32 {
        self.raw(col, row) as f32 / self.span * self.settings.scale + self.settings.bias
    }

    /// Metres between neighbouring samples along x and y.
    pub fn spacing(&self) -> (f32, f32) {
        (
            self.settings.world_width / (self.width.max(2) - 1) as f32,
            self.settings.world_height / (self.height.max(2) - 1) as f32,
        )
    }

    /// The map position `(x, y)` in metres of sample `(col, row)`.
    pub fn sample_position(&self, col: u32, row: u32) -> (f32, f32) {
        let (dx, dy) = self.spacing();
        (
            col as f32 * dx - self.settings.world_width / 2.0,
            self.settings.world_height / 2.0 - row as f32 * dy,
        )
    }

    /// True if map position `(x, y)` lies inside this heightfield.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x.abs() <= self.settings.world_width / 2.0 && y.abs() <= self.settings.world_height / 2.0
    }

    /// Ground height in metres at map position `(x, y)` (metres from the map centre), by
    /// bilinear interpolation of the four surrounding samples. Outside the grid the edge is
    /// extended.
    ///
    /// Mapping (CONFIRMED statistically, see module docs): `x` grows with the column, `y` shrinks
    /// as the row grows (row 0 = `+y` edge).
    /// PROVISIONAL: the game may split each cell into two triangles instead of interpolating
    /// bilinearly; the difference is below a few centimetres on these smooth maps.
    pub fn height_at(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = self.spacing();
        let fc = (x + self.settings.world_width / 2.0) / dx;
        let fr = (self.settings.world_height / 2.0 - y) / dy;
        let (c0, r0) = (fc.floor(), fr.floor());
        let (tx, ty) = (fc - c0, fr - r0);
        let (c0, r0) = (c0 as i64, r0 as i64);
        let h00 = self.sample_m(c0, r0);
        let h10 = self.sample_m(c0 + 1, r0);
        let h01 = self.sample_m(c0, r0 + 1);
        let h11 = self.sample_m(c0 + 1, r0 + 1);
        let top = h00 + (h10 - h00) * tx;
        let bottom = h01 + (h11 - h01) * tx;
        top + (bottom - top) * ty
    }

    /// Lowest and highest sample in metres.
    pub fn min_max_m(&self) -> (f32, f32) {
        let lo = self.samples.iter().copied().min().unwrap_or(0);
        let hi = self.samples.iter().copied().max().unwrap_or(0);
        let m = |s: u16| s as f32 / 65535.0 * self.settings.scale + self.settings.bias;
        (m(lo), m(hi))
    }
}

// ---------------------------------------------------------------------------------------------
// Deployment, outlines
// ---------------------------------------------------------------------------------------------

/// One `deployment_area`: a rotated rectangle in map metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeploymentArea {
    /// `id` within its alliance.
    pub id: u32,
    /// Centre `(x, y)` in map metres.
    pub centre: (f32, f32),
    /// `width metres`: the long side, across the facing direction (INFERRED).
    pub width: f32,
    /// `height metres`: the depth, along the facing direction (INFERRED).
    pub height: f32,
    /// `orientation radians`. INFERRED: the direction the army faces, 0 = +y, π/2 = +x
    /// (clockwise seen from above). Evidence: on hb_austerlitz the west army (x = −409) has 1.57
    /// and the east army (x = +252) has 4.71, so each faces the other.
    pub orientation: f32,
}

impl DeploymentArea {
    /// The facing direction as a unit vector in map `(x, y)`.
    pub fn facing_vector(&self) -> (f32, f32) {
        (self.orientation.sin(), self.orientation.cos())
    }

    /// The facing as an `ntw_sim` battle angle (0 = +x, π/2 = +y, counter-clockwise).
    pub fn sim_facing(&self) -> f32 {
        std::f32::consts::FRAC_PI_2 - self.orientation
    }
}

/// One `ALLIANCE` in a deployment setup.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeploymentAlliance {
    /// `ALLIANCE id`.
    pub id: u32,
    /// Its areas (one per army in that alliance, INFERRED).
    pub areas: Vec<DeploymentArea>,
}

/// One `BATTLE_DEPLOYMENT_AREAS` block. The blocks come in file order; the comments in the
/// files label them "1v1 Setup", "1v2 Setup", ... (CONFIRMED on hb_austerlitz).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeploymentSetup {
    /// Alliances in file order.
    pub alliances: Vec<DeploymentAlliance>,
}

/// The second form of `deployment_area` (e.g. hb_waterloo): no `centre`/`width`/`height`/
/// `orientation`, but a closed outline of `position x y` corners (5 points, the last repeating
/// the first; CONFIRMED form). INFERRED reading, checked on every preset by the install test
/// `deployment_areas_face_each_other`: the outline is a rectangle whose first edge runs across
/// the front (width) and whose second edge runs towards the rear (depth), so the army faces
/// against the second edge.
fn area_from_outline(id: u32, corners: &[(f32, f32)]) -> DeploymentArea {
    let unique = if corners.len() > 4 && corners.first() == corners.last() { &corners[..corners.len() - 1] } else { corners };
    let n = unique.len() as f32;
    let centre = (unique.iter().map(|p| p.0).sum::<f32>() / n, unique.iter().map(|p| p.1).sum::<f32>() / n);
    let (a, b, c) = (corners[0], corners[1], corners[2]);
    let width = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let (bx, by) = (c.0 - b.0, c.1 - b.1);
    let height = (bx * bx + by * by).sqrt();
    // Facing = −(second edge); orientation measured from +y clockwise: (sin θ, cos θ) = facing.
    let orientation = (-bx).atan2(-by);
    DeploymentArea { id, centre, width, height, orientation }
}

fn read_deployment(root: &XmlElement) -> Vec<DeploymentSetup> {
    let num = |e: &XmlElement, child: &str, attr: &str| e.child(child).and_then(|c| c.attr_f32(attr));
    root.children_named("BATTLE_DEPLOYMENT_AREAS")
        .map(|set| DeploymentSetup {
            alliances: set
                .children_named("ALLIANCE")
                .map(|a| DeploymentAlliance {
                    id: a.attr_i64("id").unwrap_or(0) as u32,
                    areas: a
                        .children_named("deployment_area")
                        .map(|d| {
                            let id = d.attr_i64("id").unwrap_or(0) as u32;
                            let corners: Vec<(f32, f32)> = d
                                .children_named("position")
                                .map(|p| (p.attr_f32("x").unwrap_or(0.0), p.attr_f32("y").unwrap_or(0.0)))
                                .collect();
                            if d.child("centre").is_none() && corners.len() >= 4 {
                                return area_from_outline(id, &corners);
                            }
                            DeploymentArea {
                                id,
                                centre: (num(d, "centre", "x").unwrap_or(0.0), num(d, "centre", "y").unwrap_or(0.0)),
                                width: num(d, "width", "metres").unwrap_or(0.0),
                                height: num(d, "height", "metres").unwrap_or(0.0),
                                orientation: num(d, "orientation", "radians").unwrap_or(0.0),
                            }
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Placed objects (ESF)
// ---------------------------------------------------------------------------------------------

/// One item of `BATTLEFIELD_BUILDING_LIST_BLOCK` (CONFIRMED layout:
/// `Utf16String key, Coord2d position, Angle, F32`).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildingPlacement {
    /// Building key, e.g. `reeds_1` (INFERRED: a `battlefield_buildings` row → model).
    pub key: String,
    /// Map position in metres.
    pub position: (f32, f32),
    /// Raw `Angle` value (u16).
    pub angle_raw: u16,
    /// The trailing F32. UNKNOWN meaning: 1.0 on hb_austerlitz reeds, 0.0 on hb_waterloo houses,
    /// so it is NOT a scale (models drawn at scale 1 match their baked footprints in the colour map).
    pub unknown_f32: f32,
}

impl BuildingPlacement {
    /// The angle in radians, INFERRED as `raw / 65536 · 2π`. Direction of rotation UNKNOWN.
    pub fn angle_radians(&self) -> f32 {
        self.angle_raw as f32 / 65536.0 * std::f32::consts::TAU
    }
}

fn read_building_list(root: &EsfRecord) -> Vec<BuildingPlacement> {
    let mut out = Vec::new();
    for child in &root.children {
        let Some(arr) = child.as_record_array() else { continue };
        for item in &arr.items {
            let mut key = None;
            let mut pos = None;
            let mut angle = 0u16;
            let mut unknown_f32 = 0.0;
            for n in item {
                match n {
                    EsfNode::Utf16String(s) | EsfNode::AsciiString(s) if key.is_none() => key = Some(s.clone()),
                    EsfNode::Coord2d(x, y) if pos.is_none() => pos = Some((*x, *y)),
                    EsfNode::Angle(a) => angle = *a,
                    EsfNode::F32(f) => unknown_f32 = *f,
                    _ => {}
                }
            }
            if let (Some(key), Some(position)) = (key, pos) {
                out.push(BuildingPlacement { key, position, angle_raw: angle, unknown_f32 });
            }
        }
    }
    out
}

/// `MIN_TREE_SCALE` 0.5 and `MAX_TREE_SCALE` 1.4, the `VegetationPicker.cpp` tweaks the exe's
/// vegetation picker copies into `+0x4C` / `+0x48` at battle set-up (`0x00EC33A0`). The values
/// are an exe reading (tweak objects `0x0164CD90` / `0x0164CD28`) whose decompile is not kept;
/// the kept sandbox evidence has only `0x00EC33A0`'s address and instruction list. INFERRED.
pub const MIN_TREE_SCALE: f32 = 0.5;
/// See [`MIN_TREE_SCALE`].
pub const MAX_TREE_SCALE: f32 = 1.4;

/// One placed tree or shrub: `Coord2d, U8, I32` (CONFIRMED types).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeInstance {
    /// Map position in metres.
    pub position: (f32, f32),
    /// The U8: read as a **relative scale** (data, see [`TreeInstance::scale`]); 0 in every
    /// unscaled list.
    pub variation: u8,
    /// The I32 (UNKNOWN). 0 on all but 391 of 3,844,367 shipped instances; 1 on 256 and 2 on 135,
    /// all in unscaled lists of one preset (install test `the_tree_instance_i32_is_not_a_rotation`).
    pub flags: i32,
}

impl TreeInstance {
    /// The multiplier this instance's `variation` byte means, or `1.0` in an unscaled list.
    ///
    /// **The decode is `u8 / 128`, clamped to [`MIN_TREE_SCALE`] .. [`MAX_TREE_SCALE`] --
    /// PROVISIONAL.** What the data shows over every shipped map (install test
    /// `every_scaled_tree_byte_lands_on_its_own_scale_band`; `UNITS_TERRAIN_FIDELITY.md` §4.1),
    /// which makes "relative scale" the reading but not a proven decode:
    /// - the byte is a scale and not an absolute size -- shrubs and trees of the same map share
    ///   one byte range, although their `.spt` bounding boxes differ by 10x (`data.tree_model`);
    /// - within one species group the bytes are spread ~uniformly over the group's own
    ///   `[min, max]`, so each instance gets a random scale in an artist-chosen band;
    /// - the shipped values are exactly `63 ..= 253` over 3,844,367 instances in 57 maps, with
    ///   **0 bytes in 1..=62** and none in 254..=255 -- the picker never sees the byte's ends.
    ///
    /// INFERRED, not read out of the exe: the division itself. The only vegetation code with the
    /// `MAX - MIN` float is unrelated (`0x00EC5E20` builds a 0.9-wide rectangle), and no `1/255`
    /// or `1/128` constant appears in `0x00E00000..0x00F00000`, so the exe computes the divisor
    /// from the two tweak floats at run time. The alternative
    /// `MIN_TREE_SCALE + u8 * (MAX_TREE_SCALE - MIN_TREE_SCALE) / 255` maps the same bytes onto
    /// 0.72 ..= 1.39 and cannot be told apart without the reader. Under `/ 128` the 1.82% of
    /// bytes above 179 are clamped to 1.4; under the rival none are. Target: the exe's reader of
    /// the byte (the tree list item reader `0x00EDA990`'s consumer).
    pub fn scale(&self, list_is_scaled: bool) -> f32 {
        if !list_is_scaled {
            return 1.0;
        }
        (f32::from(self.variation) / 128.0).clamp(MIN_TREE_SCALE, MAX_TREE_SCALE)
    }
}

/// All instances of one species in a [`TreeList`].
#[derive(Debug, Clone, PartialEq)]
pub struct TreeGroup {
    /// Species key, e.g. `lc_tundra-lowbrush_shrub` (INFERRED: `warscape_trees`).
    pub species: String,
    /// The instances.
    pub instances: Vec<TreeInstance>,
}

/// One `TREE_LIST` record: v3 = `Bool, U32 group count, groups...`; v2 = no Bool
/// (CONFIRMED structure). A group is `Utf16String species, U32 n, n × instance`.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeList {
    /// The leading Bool of v3 lists (false for v2). INFERRED: "instances carry a U8
    /// variation" (CONFIRMED: the U8 is present exactly when it is true, on the files checked).
    pub flag: bool,
    /// Species groups.
    pub groups: Vec<TreeGroup>,
}

fn read_tree_lists(root: &EsfRecord) -> Result<Vec<TreeList>, String> {
    let mut lists = Vec::new();
    for list in root.children_named("TREE_LIST") {
        let c = &list.children;
        let what = |i: usize| c.get(i).map_or("end", EsfNode::type_name);
        // v3 starts with a Bool; v2 (some MP maps) has no Bool (CONFIRMED).
        let mut i = 0;
        let flag = match c.first() {
            Some(EsfNode::Bool(b)) => {
                i = 1;
                *b
            }
            _ => false,
        };
        let groups_n = c.get(i).and_then(EsfNode::as_u32).ok_or_else(|| format!("no group count ({})", what(i)))?;
        i += 1;
        let mut groups = Vec::with_capacity(groups_n as usize);
        for _ in 0..groups_n {
            let species =
                c.get(i).and_then(EsfNode::as_str).ok_or_else(|| format!("no species at {i} ({})", what(i)))?.to_owned();
            let n = c.get(i + 1).and_then(EsfNode::as_u32).ok_or_else(|| format!("no count at {}", i + 1))? as usize;
            i += 2;
            let mut instances = Vec::with_capacity(n);
            for _ in 0..n {
                // Coord2d, then a U8 only in lists whose Bool is true (CONFIRMED), then an I32.
                let Some(EsfNode::Coord2d(x, y)) = c.get(i) else {
                    return Err(format!("expected Coord2d at child {i}, found {}", what(i)));
                };
                i += 1;
                let mut variation = 0;
                if let Some(EsfNode::U8(v)) = c.get(i) {
                    variation = *v;
                    i += 1;
                }
                let Some(EsfNode::I32(f)) = c.get(i) else {
                    return Err(format!("expected I32 at child {i}, found {}", what(i)));
                };
                i += 1;
                instances.push(TreeInstance { position: (*x, *y), variation, flags: *f });
            }
            groups.push(TreeGroup { species, instances });
        }
        if i != c.len() {
            return Err(format!("TREE_LIST: {} children left over", c.len() - i));
        }
        lists.push(TreeList { flag, groups });
    }
    Ok(lists)
}

// ---------------------------------------------------------------------------------------------
// Ground types
// ---------------------------------------------------------------------------------------------

/// `ground_type_map_0.tga`: an 8-bit palette image whose **index** is the ground type of each
/// cell (CONFIRMED: colour-mapped TGA, 25-entry palette, 512 × 512 on hb_austerlitz). The names
/// of the indices are UNKNOWN (no DB table lists them; likely an enum in the exe).
#[derive(Debug, Clone, PartialEq)]
pub struct GroundTypeMap {
    /// Cells per row.
    pub width: u32,
    /// Rows; row 0 = top of the picture (TGA rows are re-ordered top-down by the decoder).
    pub height: u32,
    /// Ground-type index per cell.
    pub cells: Vec<u8>,
    /// The palette (editor display colours).
    pub palette: Vec<[u8; 4]>,
}

impl GroundTypeMap {
    /// The ground type index at map position `(x, y)` on a map `world` metres wide, using the
    /// same layout as the heightfield (column ↔ x, row 0 = `+y` edge; CONFIRMED statistically).
    pub fn at(&self, x: f32, y: f32, world_width: f32, world_height: f32) -> u8 {
        let c = ((x / world_width + 0.5) * self.width as f32).floor() as i64;
        let r = ((0.5 - y / world_height) * self.height as f32).floor() as i64;
        let c = c.clamp(0, self.width as i64 - 1) as usize;
        let r = r.clamp(0, self.height as i64 - 1) as usize;
        self.cells[r * self.width as usize + c]
    }
}

// ---------------------------------------------------------------------------------------------
// The whole map
// ---------------------------------------------------------------------------------------------

/// Everything this reader understands about one preset battle map.
#[derive(Debug, Clone)]
pub struct BattleMap {
    /// Folder name, e.g. `hb_austerlitz` (lower case, as the VFS lists it).
    pub name: String,
    /// Normalized folder path with a trailing `\`.
    pub folder: String,
    /// `definition.xml`.
    pub definition: MapDefinition,
    /// `textures.xml`, if present.
    pub textures: Option<MapTextures>,
    /// `weather.xml`, if present.
    pub weather: Option<Weather>,
    /// Lighting from the environment file `weather.xml` names, if present.
    pub lighting: Option<Lighting>,
    /// `height_map_0..3`: level 0 is the playable area, each next level covers twice the size
    /// at the same resolution (INFERRED: the distant landscape).
    pub heightfields: Vec<Heightfield>,
    /// `deployment_areas.xml`.
    pub deployment: Vec<DeploymentSetup>,
    /// `bmd_near_buildings.building_list`.
    pub buildings_near: Vec<BuildingPlacement>,
    /// `bmd_far_buildings.building_list`.
    pub buildings_far: Vec<BuildingPlacement>,
    /// `bmd.tree_list`.
    pub trees: Vec<TreeList>,
    /// `non_terrain_outlines.xml`: polygons in map metres (INFERRED: footprints where no
    /// terrain is drawn or walked, e.g. under buildings).
    pub outlines: Vec<Vec<(f32, f32)>>,
    /// `ground_type_map_0.tga`, if present.
    pub ground_types: Option<GroundTypeMap>,
    /// Every file in the folder (normalized paths).
    pub files: Vec<String>,
}

/// Names of all preset maps (folders under `battleterrain\presets\` that have a `definition.xml`).
pub fn list_presets(vfs: &Vfs) -> Vec<String> {
    let mut names: Vec<String> = vfs
        .list(PRESETS_DIR)
        .into_iter()
        .filter_map(|p| p.strip_prefix(PRESETS_DIR)?.strip_suffix(r"\definition.xml").map(str::to_owned))
        .filter(|n| !n.contains('\\'))
        .collect();
    names.sort();
    names
}

fn read_xml(vfs: &Vfs, path: &str) -> Result<Option<XmlElement>, TerrainError> {
    if !vfs.contains(path) {
        return Ok(None);
    }
    let bytes = vfs.read(path).map_err(|e| bad(path, e))?;
    xml::parse_bytes(&bytes).map(Some).map_err(|e| bad(path, e))
}

fn read_esf(vfs: &Vfs, path: &str) -> Result<Option<EsfRecord>, TerrainError> {
    if !vfs.contains(path) {
        return Ok(None);
    }
    let bytes = vfs.read(path).map_err(|e| bad(path, e))?;
    EsfFile::from_bytes(&bytes).map(|f| Some(f.root)).map_err(|e| bad(path, e))
}

impl BattleMap {
    /// Loads preset `name` (case-insensitive folder name under `battleterrain\presets\`).
    pub fn load(vfs: &Vfs, name: &str) -> Result<Self, TerrainError> {
        let name = name.to_ascii_lowercase();
        let folder = format!("{PRESETS_DIR}{name}\\");
        let path = |f: &str| format!("{folder}{f}");
        let def_path = path("definition.xml");
        let definition = read_xml(vfs, &def_path)?.ok_or_else(|| TerrainError::Missing(def_path.clone()))?;
        let definition = MapDefinition::from_xml(&definition);
        let textures = read_xml(vfs, &path("textures.xml"))?.map(|e| MapTextures::from_xml(&e));
        let weather = read_xml(vfs, &path("weather.xml"))?.map(|e| Weather::from_xml(&e));
        let lighting = match &weather {
            Some(w) if !w.environment_key.is_empty() => {
                read_xml(vfs, &w.environment_key)?.and_then(|scene| Lighting::from_scene(&scene))
            }
            _ => None,
        };

        let mut heightfields = Vec::new();
        for level in 0.. {
            let dds_path = path(&format!("height_map_{level}.dds"));
            if !vfs.contains(&dds_path) {
                break;
            }
            let set_path = path(&format!("height_map_{level}_settings.xml"));
            let settings = read_xml(vfs, &set_path)?.ok_or_else(|| TerrainError::Missing(set_path.clone()))?;
            let settings = HeightfieldSettings::from_xml(&settings);
            let bytes = vfs.read(&dds_path).map_err(|e| bad(&dds_path, e))?;
            heightfields.push(Heightfield::from_dds(&bytes, settings).map_err(|e| bad(&dds_path, e))?);
        }

        let deployment = read_xml(vfs, &path("deployment_areas.xml"))?.map(|r| read_deployment(&r)).unwrap_or_default();
        let buildings_near = read_esf(vfs, &path("bmd_near_buildings.building_list"))?
            .map(|r| read_building_list(&r))
            .unwrap_or_default();
        let buildings_far = read_esf(vfs, &path("bmd_far_buildings.building_list"))?
            .map(|r| read_building_list(&r))
            .unwrap_or_default();
        let tree_path = path("bmd.tree_list");
        let trees = match read_esf(vfs, &tree_path)? {
            Some(r) => read_tree_lists(&r).map_err(|e| bad(&tree_path, e))?,
            None => Vec::new(),
        };
        let outlines = read_xml(vfs, &path("non_terrain_outlines.xml"))?
            .map(|r| {
                r.children_named("OUTLINE")
                    .map(|o| {
                        o.children_named("position")
                            .map(|p| (p.attr_f32("x").unwrap_or(0.0), p.attr_f32("y").unwrap_or(0.0)))
                            .collect()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let gt_path = path("ground_type_map_0.tga");
        let ground_types = if vfs.contains(&gt_path) {
            let bytes = vfs.read(&gt_path).map_err(|e| bad(&gt_path, e))?;
            let tga = Tga::parse(&bytes).map_err(|e| bad(&gt_path, e))?;
            (!tga.indices.is_empty()).then_some(GroundTypeMap {
                width: tga.width,
                height: tga.height,
                cells: tga.indices,
                palette: tga.palette,
            })
        } else {
            None
        };
        let files = vfs.list(&folder).into_iter().map(str::to_owned).collect();
        Ok(Self {
            name,
            folder,
            definition,
            textures,
            weather,
            lighting,
            heightfields,
            deployment,
            buildings_near,
            buildings_far,
            trees,
            outlines,
            ground_types,
            files,
        })
    }

    /// The playable heightfield (level 0), if the map has one.
    pub fn ground(&self) -> Option<&Heightfield> {
        self.heightfields.first()
    }

    /// Ground height in metres at map position `(x, y)`: level 0 where it covers the point,
    /// else the first larger level that does, else 0. This is the query battle units use to
    /// stand on the terrain (map `(x, y)` = `ntw_sim` battle position, see module docs).
    pub fn height_at(&self, x: f32, y: f32) -> f32 {
        self.heightfields
            .iter()
            .find(|h| h.contains(x, y))
            .or(self.heightfields.last())
            .map_or(0.0, |h| h.height_at(x, y))
    }

    /// Path of `colour_map_<level>.jpg` (or its `_alpha` companion) if it exists.
    pub fn colour_map_path(&self, level: usize, alpha: bool) -> Option<String> {
        let suffix = if alpha { "_alpha" } else { "" };
        let p = format!("{}colour_map_{level}{suffix}.jpg", self.folder);
        self.files.contains(&p).then_some(p)
    }

    /// The first deployment setup (the 1v1 layout on the files checked), if any.
    pub fn one_v_one(&self) -> Option<&DeploymentSetup> {
        self.deployment.first()
    }

    /// True when map position `(x, y)` is under the sea, i.e. the ground there is below
    /// [`SEA_LEVEL`]. This is the rule the renderer uses to clip the sea plane; the ground-type map
    /// is the cross-check that confirmed the level (see [`SEA_LEVEL_NOTE`]).
    pub fn is_sea(&self, x: f32, y: f32) -> bool {
        self.height_at(x, y) < SEA_LEVEL
    }

    /// The share of the playable (level-0) area that is under the sea, 0..1. Used to decide
    /// whether a map needs the sea surface at all.
    pub fn sea_coverage(&self) -> f32 {
        let Some(hf) = self.ground() else { return 0.0 };
        if hf.width == 0 || hf.height == 0 {
            return 0.0;
        }
        let (mut below, mut total) = (0u32, 0u32);
        // Every 8th sample: the coverage figure only decides "draw it or not".
        let step = 8;
        for r in (0..hf.height as i64).step_by(step) {
            for c in (0..hf.width as i64).step_by(step) {
                total += 1;
                if hf.sample_m(c, r) < SEA_LEVEL {
                    below += 1;
                }
            }
        }
        if total == 0 { 0.0 } else { below as f32 / total as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The (PROVISIONAL) decode at its landmarks: the shipped floor byte 63 and byte 64 both give
    /// the 0.5 clamp, 128 is unit scale, 179 is just under the 1.4 clamp and 180 is on it; an
    /// unscaled list ignores the byte.
    #[test]
    fn tree_scale_byte_landmarks() {
        let s = |v: u8, scaled: bool| TreeInstance { position: (0.0, 0.0), variation: v, flags: 0 }.scale(scaled);
        assert_eq!(s(63, true), 0.5);
        assert_eq!(s(64, true), 0.5);
        assert_eq!(s(128, true), 1.0);
        assert!((s(179, true) - 1.3984375).abs() < 1e-7);
        assert_eq!(s(180, true), 1.4);
        assert_eq!(s(253, true), 1.4);
        assert_eq!(s(200, false), 1.0);
        assert_eq!(s(0, false), 1.0);
    }

    fn flat(settings_scale: f32, samples: Vec<u16>, w: u32, h: u32) -> Heightfield {
        Heightfield {
            width: w,
            height: h,
            samples,
            settings: HeightfieldSettings {
                world_width: 100.0,
                world_height: 100.0,
                normalize: true,
                simple_colour_blending: false,
                scale: settings_scale,
                bias: -10.0,
            },
            span: 65535.0,
        }
    }

    #[test]
    fn normalised_levels_scale_by_their_own_range() {
        assert_eq!(sample_span(&[0, 100, 65534], true), 65534.0);
        assert_eq!(sample_span(&[0, 100, 65534], false), 65535.0);
        assert_eq!(sample_span(&[7, 7], true), 65535.0, "flat level");
        let mut h = flat(100.0, vec![0, 65534], 2, 1);
        h.span = sample_span(&h.samples, true);
        assert!((h.sample_m(1, 0) - 90.0).abs() < 1e-4, "top sample = bias + scale");
        assert!((h.sample_m(0, 0) + 10.0).abs() < 1e-6);
    }

    #[test]
    fn height_formula_and_bilinear() {
        // 3x3 grid over 100 m: samples at -50, 0, +50. Only the centre is raised.
        let mut s = vec![0u16; 9];
        s[4] = 65535;
        let h = flat(20.0, s, 3, 3);
        assert_eq!(h.height_at(-50.0, -50.0), -10.0);
        assert!((h.height_at(0.0, 0.0) - 10.0).abs() < 1e-4);
        assert!((h.height_at(25.0, 0.0) - 0.0).abs() < 1e-4);
        assert!((h.height_at(25.0, 25.0) - -5.0).abs() < 1e-4);
        // Outside: edge extended.
        assert_eq!(h.height_at(-500.0, 0.0), -10.0);
        assert_eq!(h.sample_position(2, 0), (50.0, 50.0));
        assert_eq!(h.min_max_m(), (-10.0, 10.0));
    }

    #[test]
    fn row_zero_is_plus_y() {
        // 2x2: row 1 (y = -50) is high.
        let h = flat(10.0, vec![0, 0, 65535, 65535], 2, 2);
        assert!(h.height_at(0.0, -50.0) > h.height_at(0.0, 50.0));
    }

    #[test]
    fn deployment_xml() {
        let x = xml::parse(
            "<BATTLE_DEPLOYMENT_AREA_HASH_TABLE><BATTLE_DEPLOYMENT_AREAS><!-- 1v1 -->\
             <ALLIANCE id='1'><deployment_area id='0'><centre x=\"1.5\" y=\"-2\"/><width metres=\"1000\"/>\
             <height metres=\"300\"/><orientation radians=\"1.5707964\"/></deployment_area></ALLIANCE>\
             </BATTLE_DEPLOYMENT_AREAS></BATTLE_DEPLOYMENT_AREA_HASH_TABLE>",
        )
        .unwrap();
        let d = read_deployment(&x);
        let a = d[0].alliances[0].areas[0];
        assert_eq!(d[0].alliances[0].id, 1);
        assert_eq!(a.centre, (1.5, -2.0));
        assert_eq!((a.width, a.height), (1000.0, 300.0));
        let (fx, fy) = a.facing_vector();
        assert!((fx - 1.0).abs() < 1e-5 && fy.abs() < 1e-5);
        assert!(a.sim_facing().abs() < 1e-5);
    }

    #[test]
    fn deployment_outline_form() {
        // hb_waterloo's alliance 0 shape: front edge along +x at y = -453, rear at y = -653.
        let x = xml::parse(
            "<T><BATTLE_DEPLOYMENT_AREAS><ALLIANCE id='0'><deployment_area id='0'>\
             <position x='-269' y='-453'/><position x='531' y='-453'/><position x='531' y='-653'/>\
             <position x='-269' y='-653'/><position x='-269' y='-453'/></deployment_area></ALLIANCE>\
             </BATTLE_DEPLOYMENT_AREAS></T>",
        )
        .unwrap();
        let a = read_deployment(&x)[0].alliances[0].areas[0];
        assert_eq!(a.centre, (131.0, -553.0));
        assert_eq!((a.width, a.height), (800.0, 200.0));
        let (fx, fy) = a.facing_vector();
        assert!(fx.abs() < 1e-5 && (fy - 1.0).abs() < 1e-5, "faces +y");
    }

    #[test]
    fn tree_list_layout() {
        let mut list = EsfRecord::new("TREE_LIST", 3);
        list.children = vec![
            EsfNode::Bool(true),
            EsfNode::U32(1),
            EsfNode::Utf16String("oak".into()),
            EsfNode::U32(2),
            EsfNode::Coord2d(1.0, 2.0),
            EsfNode::U8(128),
            EsfNode::I32(0),
            EsfNode::Coord2d(3.0, 4.0),
            EsfNode::U8(200),
            EsfNode::I32(0),
        ];
        let mut root = EsfRecord::new("TREE_LOD_LIST", 1);
        root.children = vec![EsfNode::U32(1), EsfNode::Record(Box::new(list))];
        let lists = read_tree_lists(&root).unwrap();
        assert_eq!(lists[0].groups[0].species, "oak");
        assert_eq!(lists[0].groups[0].instances[1].position, (3.0, 4.0));
    }

    #[test]
    fn lighting_tuples() {
        let scene = xml::parse(
            "<SCENE><LIGHTING light_direction='euler(0.5,-1.0,0)' light_colour='rgb(1,0.5,0.25)' light_colour_scale='2'/>\
             <X><SKYGEN sky_colour='rgb(0.1,0.2,0.3)' sky_colour_scale='2'/></X></SCENE>",
        )
        .unwrap();
        let l = Lighting::from_scene(&scene).unwrap();
        assert_eq!(l.light_direction_euler, [0.5, -1.0, 0.0]);
        assert_eq!(l.light_colour, [1.0, 0.5, 0.25]);
        assert_eq!(l.sky_colour, Some([0.1, 0.2, 0.3]));
        assert_eq!(l.sky_colour_scale, 2.0);
    }

    /// A flat heightfield entirely below the sea is all sea; one above it is none (`SEA_LEVEL`).
    #[test]
    fn sea_level_splits_the_heightfield() {
        let mut below = flat(100.0, vec![0, 0], 2, 1);
        below.settings.scale = 100.0;
        below.settings.bias = -100.0;
        below.span = sample_span(&below.samples, true);
        let map = BattleMap {
            name: "t".into(),
            folder: "t\\".into(),
            heightfields: vec![below],
            ..test_map()
        };
        assert!(map.is_sea(0.0, 0.0));
        assert_eq!(map.sea_coverage(), 1.0);

        let mut above = flat(10.0, vec![0, 0], 2, 1);
        above.settings.bias = 6.0;
        above.span = sample_span(&above.samples, true);
        let map = BattleMap { heightfields: vec![above], ..test_map() };
        assert!(!map.is_sea(0.0, 0.0));
        assert_eq!(map.sea_coverage(), 0.0);
    }

    /// `BattleMap`'s fields have grown; this keeps the tests above from listing them all.
    fn test_map() -> BattleMap {
        BattleMap {
            name: String::new(),
            folder: String::new(),
            definition: MapDefinition::default(),
            textures: None,
            weather: None,
            lighting: None,
            heightfields: Vec::new(),
            deployment: Vec::new(),
            buildings_near: Vec::new(),
            buildings_far: Vec::new(),
            trees: Vec::new(),
            outlines: Vec::new(),
            ground_types: None,
            files: Vec::new(),
        }
    }
}

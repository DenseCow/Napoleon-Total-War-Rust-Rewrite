//! The battlefield under the units: ground types (movement speed) and heights (slope fatigue).
//!
//! Sources (see `analysis/worker5/BATTLE_TERRAIN.md` §10):
//! - The ground-type map is `ground_type_map_0.tga` of the battle preset: one palette index per
//!   cell, laid out like the heightfield (column ↔ +x, row 0 = the +y edge; CONFIRMED
//!   statistically by the terrain worker).
//! - Index → name: the exe holds a pointer table of 25 names + `end_marker` +
//!   `invalid_ground_type` (CONFIRMED in the bytes of `Napoleon.exe`, read as data, no
//!   disassembly), in the order of [`GROUND_TYPE_NAMES`]. CONFIRMED (`0x0061E430`, the "Ground
//!   State Grid"): the exe looks up one `unit_movement_modifiers` record per name in this table
//!   order and indexes that list with each grid cell's type byte; the grid is 512 × 512 cells,
//!   filled from the first byte of each 4-byte pixel of the battle's ground image (INFERRED: the
//!   ground-type map). See BATTLE_FIDELITY.md §7.
//! - Speed: `unit_movement_modifiers` (28 rows `name, f×4`) by name; the column per unit type is
//!   [`MovementClass`] (CONFIRMED, `0x006543D0`).
//! - Slope: the gradient `+0x1A0` is the height change over the tick's step ahead divided by its
//!   length, and it scales the speed (uphill `1 / (1 + 3g)`, downhill `min(1 − g, 1.5)`; CONFIRMED,
//!   `0x00819770`, applied in the movement). The `kv_fatigue` gradient multipliers read it (W1 §12.5).
//!
//! Everything is plain `f32` arithmetic in a fixed order, so the model stays deterministic.

/// Ground-type names by map index (CONFIRMED table order in the exe; index mapping INFERRED).
pub const GROUND_TYPE_NAMES: [&str; 25] = [
    "field_ploughed",
    "field_ploughed_wet",
    "field_forest",
    "grassland",
    "mud",
    "mud_wet",
    "road",
    "road_frozen",
    "rock",
    "sand",
    "sand_wet",
    "scree",
    "snow",
    "stone_masonry",
    "vegetation_dense_forest",
    "vegetation_light_scrub",
    "vegetation_medium_woodland",
    "water_deep",
    "water_frozen",
    "water_medium_ford",
    "water_shallow",
    "wood",
    "stone",
    "glass",
    "none",
];

/// Which `unit_movement_modifiers` column a soldier uses: the soldier speed update `0x006543D0`
/// (CONFIRMED) takes the ground record of its cell (`+0x19C`, set by `0x0081AC80`) and reads:
/// - artillery with guns (`0x0055AB90`: category 1 and the guns object `+0x1F4 → +0x10`): column 0
///   (`+0xC`) for class `artillery_foot` (1), column 1 (`+0x10`) for `artillery_horse` (2), else no
///   modifier (1.0, e.g. `artillery_fixed`);
/// - mounted (`0x0055ABF0`: category cavalry 0, camels 5, or dragoons 3 not dismounted): column 2
///   (`+0x14`), or column 3 when dismounted (`0x0055C2A0`);
/// - infantry, gun crews without guns and dismounted units (`0x0055C1C0`): column 3 (`+0x18`);
/// - anything else, or a ground index ≥ 25: 1.0.
///
/// The soldier's speed `+0x1A4` is multiplied by it (and by the unit type's fatigue speed factor
/// `+0xE4 + fatigue state × 4`, `0x00649520`, while `+0x220 > 0`; not modelled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MovementClass {
    /// `artillery_foot` with its guns: column 0.
    FootArtillery,
    /// `artillery_horse` with its guns: column 1.
    HorseArtillery,
    /// Cavalry, camels, mounted dragoons: column 2.
    Mounted,
    /// Infantry (and dismounted units, crews without guns): column 3.
    #[default]
    Infantry,
    /// Other artillery with guns (`artillery_fixed`): no modifier.
    Fixed,
}

impl MovementClass {
    /// The table column, or `None` for no modifier.
    pub fn column(self) -> Option<usize> {
        match self {
            MovementClass::FootArtillery => Some(0),
            MovementClass::HorseArtillery => Some(1),
            MovementClass::Mounted => Some(2),
            MovementClass::Infantry => Some(3),
            MovementClass::Fixed => None,
        }
    }
}

/// A grid laid over the map, `cols × rows` cells covering `width × height` metres centred on the
/// origin; row 0 is the `+y` edge.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSpec {
    /// Cells across (x).
    pub cols: u32,
    /// Cells down (−y).
    pub rows: u32,
    /// World width in metres.
    pub width: f32,
    /// World height in metres.
    pub height: f32,
}

/// Ground types per cell (`ground_type_map_0.tga`).
#[derive(Clone, PartialEq)]
pub struct GroundTypeGrid {
    /// Layout.
    pub spec: GridSpec,
    /// Index into [`GROUND_TYPE_NAMES`] per cell, row-major.
    pub cells: Vec<u8>,
}

impl std::fmt::Debug for GroundTypeGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GroundTypeGrid({:?})", self.spec)
    }
}

impl std::fmt::Debug for HeightGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HeightGrid({:?})", self.spec)
    }
}

impl GroundTypeGrid {
    /// The ground type at map position `(x, y)` (cell containing the point; edges clamp).
    pub fn at(&self, x: f32, y: f32) -> u8 {
        let s = &self.spec;
        let c = ((x / s.width + 0.5) * s.cols as f32).floor() as i64;
        let r = ((0.5 - y / s.height) * s.rows as f32).floor() as i64;
        let c = c.clamp(0, s.cols as i64 - 1) as usize;
        let r = r.clamp(0, s.rows as i64 - 1) as usize;
        self.cells[r * s.cols as usize + c]
    }
}

/// Heights at the grid's sample points (corners: `cols` samples span `width`), bilinear in
/// between, like the renderer's `Heightfield::height_at`.
#[derive(Clone, PartialEq)]
pub struct HeightGrid {
    /// Layout: `cols × rows` samples, the first and last on the edges.
    pub spec: GridSpec,
    /// Metres per sample, row-major.
    pub heights: Vec<f32>,
}

impl HeightGrid {
    /// Ground height in metres at `(x, y)`; outside the grid the edge is extended.
    pub fn at(&self, x: f32, y: f32) -> f32 {
        let s = &self.spec;
        let fx = ((x / s.width + 0.5) * (s.cols - 1) as f32).clamp(0.0, (s.cols - 1) as f32);
        let fy = ((0.5 - y / s.height) * (s.rows - 1) as f32).clamp(0.0, (s.rows - 1) as f32);
        let (c0, r0) = (fx.floor() as usize, fy.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(s.cols as usize - 1), (r0 + 1).min(s.rows as usize - 1));
        let (tx, ty) = (fx - c0 as f32, fy - r0 as f32);
        let w = s.cols as usize;
        let h = |c: usize, r: usize| self.heights[r * w + c];
        let top = h(c0, r0) + (h(c1, r0) - h(c0, r0)) * tx;
        let bottom = h(c0, r1) + (h(c1, r1) - h(c0, r1)) * tx;
        top + (bottom - top) * ty
    }
}

/// What the model knows about the ground. `Default` = flat grassland everywhere.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleGround {
    /// Ground types, if the map has a ground-type map.
    pub types: Option<GroundTypeGrid>,
    /// Heights, if the map has a heightfield.
    pub heights: Option<HeightGrid>,
    /// `unit_movement_modifiers` per ground-type index (4 columns); 1.0 where the table has no
    /// row for the name.
    pub speed_modifiers: [[f32; 4]; 25],
}

impl Default for BattleGround {
    fn default() -> Self {
        Self { types: None, heights: None, speed_modifiers: [[1.0; 4]; 25] }
    }
}

/// Builds the per-index speed table from `unit_movement_modifiers` rows (name, 4 floats).
/// Names not in the table keep 1.0.
pub fn speed_table<'a>(rows: impl IntoIterator<Item = (&'a str, [f32; 4])>) -> [[f32; 4]; 25] {
    let mut out = [[1.0f32; 4]; 25];
    for (name, values) in rows {
        if let Some(i) = GROUND_TYPE_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            out[i] = values;
        }
    }
    out
}

impl BattleGround {
    /// The ground-type index at `(x, y)`; `grassland` without a map.
    pub fn ground_type(&self, x: f32, y: f32) -> u8 {
        self.types.as_ref().map_or(3, |t| t.at(x, y))
    }

    /// Speed multiplier for a unit of `class` standing at `(x, y)`.
    pub fn speed_modifier(&self, x: f32, y: f32, class: MovementClass) -> f32 {
        let t = self.ground_type(x, y) as usize;
        match (self.speed_modifiers.get(t), class.column()) {
            (Some(row), Some(c)) => row[c],
            _ => 1.0,
        }
    }

    /// Ground height at `(x, y)` (0 without a heightfield).
    pub fn height(&self, x: f32, y: f32) -> f32 {
        self.heights.as_ref().map_or(0.0, |h| h.at(x, y))
    }

    /// Gradient of a move from `a` to `b`: height gained / horizontal distance (negative
    /// downhill, 0 for no move).
    pub fn gradient(&self, a: (f32, f32), b: (f32, f32)) -> f32 {
        let d = ((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1)).sqrt();
        if d <= 0.0 {
            return 0.0;
        }
        (self.height(b.0, b.1) - self.height(a.0, a.1)) / d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(cols: u32, rows: u32) -> GridSpec {
        GridSpec { cols, rows, width: 100.0, height: 100.0 }
    }

    #[test]
    fn names_have_the_exe_order() {
        assert_eq!(GROUND_TYPE_NAMES[3], "grassland");
        assert_eq!(GROUND_TYPE_NAMES[6], "road");
        assert_eq!(GROUND_TYPE_NAMES[24], "none");
    }

    #[test]
    fn ground_type_layout_row_zero_is_plus_y() {
        // 2 × 2: top row (y > 0) road, bottom row mud.
        let g = GroundTypeGrid { spec: spec(2, 2), cells: vec![6, 6, 4, 4] };
        assert_eq!(g.at(-10.0, 25.0), 6);
        assert_eq!(g.at(10.0, -25.0), 4);
        assert_eq!(g.at(1000.0, -1000.0), 4); // clamped
    }

    #[test]
    fn speed_modifier_by_ground_and_class() {
        let ground = BattleGround {
            types: Some(GroundTypeGrid { spec: spec(2, 1), cells: vec![6, 14] }),
            heights: None,
            speed_modifiers: speed_table([("road", [1.5; 4]), ("vegetation_dense_forest", [0.4, 0.4, 0.45, 0.7])]),
        };
        assert_eq!(ground.speed_modifier(-20.0, 0.0, MovementClass::Infantry), 1.5);
        assert_eq!(ground.speed_modifier(20.0, 0.0, MovementClass::Infantry), 0.7);
        assert_eq!(ground.speed_modifier(20.0, 0.0, MovementClass::Mounted), 0.45);
        assert_eq!(ground.speed_modifier(20.0, 0.0, MovementClass::FootArtillery), 0.4);
        assert_eq!(ground.speed_modifier(20.0, 0.0, MovementClass::Fixed), 1.0);
        // Names missing from the table keep 1.0; no map = grassland = 1.0.
        assert_eq!(BattleGround::default().speed_modifier(0.0, 0.0, MovementClass::FootArtillery), 1.0);
    }

    #[test]
    fn height_bilinear_and_gradient() {
        // 3 × 3 samples over 100 m (x = -50, 0, 50); heights rise with x by 10 m per 50 m.
        let heights = (0..3).flat_map(|_| [0.0, 10.0, 20.0]).collect();
        let ground = BattleGround { heights: Some(HeightGrid { spec: spec(3, 3), heights }), ..Default::default() };
        assert!((ground.height(25.0, 7.0) - 15.0).abs() < 1e-5);
        assert!((ground.gradient((0.0, 0.0), (10.0, 0.0)) - 0.2).abs() < 1e-5);
        assert!((ground.gradient((10.0, 0.0), (0.0, 0.0)) + 0.2).abs() < 1e-5);
        assert_eq!(ground.gradient((5.0, 5.0), (5.0, 5.0)), 0.0);
    }
}

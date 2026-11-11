//! Battle-map **grass**: where the blades stand, which picture each one shows and how big it is.
//! The drawing is the renderer's own (`napoleon::terrain::grass`); this module is the original's
//! placement rule, traced in `Napoleon.exe` (notes: `analysis/graphics/GRASS.md`).
//!
//! The data a map gives the grass:
//! - **the grass map** (`<preset>\grassmap` in the image loader's extension order, so
//!   `grassmap.tga`, or `grassmap.jpg` + `grassmap_alpha.jpg` on six vanilla presets;
//!   [`find_grass_map`]): one texel per patch of ground, its four channels the density (0..255) of
//!   the four grass kinds `g0`..`g3`. Row 0 is the map's +y edge, like the ground-type map
//!   ([`GrassMap`]);
//! - **the grass type** (`textures.xml grass_type`, e.g. `Temperate`): the picture atlas
//!   `battleterrain\grass\<grass_type>.tai` ([`atlas_path`], CONFIRMED `0x011D0A10`). Each atlas
//!   entry is one blade picture; its name says its kind and size ([`GrassAtlas`]).
//!
//! Placement (CONFIRMED `0x01248590`): the map is cut into square cells of
//! [`GrassQuality::cell_size`] metres; a cell's blades come from its own seeded random sequence,
//! so a cell is the same every time it is built and only the cells near the camera need to exist
//! ([`cell_window`]). [`place_cell`] is that rule, step for step.

use crate::texture_atlas::TaiAtlas;

/// The grass kinds: the grass map's four channels, and the atlas names' `g0`..`g3` prefixes.
pub const KINDS: usize = 4;

/// The folder of the grass picture atlases (CONFIRMED `0x011D0A10`: `"BattleTerrain/Grass/"`).
pub const ATLAS_FOLDER: &str = r"battleterrain\grass\";

/// The game-file path of a grass type's atlas: `battleterrain\grass\<grass_type>.tai`
/// (CONFIRMED `0x011D0A10`), normalized like every VFS path (lower case, `\`).
pub fn atlas_path(grass_type: &str) -> String {
    crate::pack::normalize_path(&format!("{ATLAS_FOLDER}{grass_type}.tai"))
}

/// One step of the original's random sequence (the C runtime's `rand` constants), CONFIRMED
/// `0x01248590`, `0x0124E070`, `0x011EFBD0`.
fn lcg(r: u32) -> u32 {
    r.wrapping_mul(214_013).wrapping_add(2_531_011)
}

/// The 16-bit draw of a step: its upper half (CONFIRMED, same functions).
fn hi(r: u32) -> u32 {
    r >> 16
}

/// `1/65535` as the exe stores it (CONFIRMED `g_flInv65535`, `0x0131A7AC`).
const INV_65535: f32 = 1.0 / 65535.0;
/// `1/257`, the accept test's factor (CONFIRMED `0x01248590`, the constant 0.0038910506).
const INV_257: f32 = 1.0 / 257.0;
/// `0.15/65535`, the blade scale's spread per draw (CONFIRMED `0x01248590`, 2.2888535e-6).
const SCALE_SPREAD: f32 = 2.288_853_5e-6;

/// A 16-bit draw as 0..=1 (`× 1/65535`, CONFIRMED).
fn unit(r: u32) -> f32 {
    hi(r) as f32 * INV_65535
}

/// One row of the grass detail setting's table (CONFIRMED: the five rows at `0x01419AB4`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassQuality {
    /// How many cells either side of the camera's cell have grass.
    pub radius_cells: u32,
    /// A cell's side in metres.
    pub cell_size: f32,
    /// A cell tries `blades_per_side²` blades.
    pub blades_per_side: u32,
}

/// The grass detail setting (`gfx_grass_quality`, 0..=4) → its row, CONFIRMED `0x01419AB4`.
/// Row 0 is never drawn: the setting 0 turns the grass off.
pub const QUALITY_TABLE: [GrassQuality; 5] = [
    GrassQuality { radius_cells: 5, cell_size: 1.0, blades_per_side: 1 },
    GrassQuality { radius_cells: 5, cell_size: 16.0, blades_per_side: 16 },
    GrassQuality { radius_cells: 5, cell_size: 16.0, blades_per_side: 24 },
    GrassQuality { radius_cells: 5, cell_size: 16.0, blades_per_side: 28 },
    GrassQuality { radius_cells: 5, cell_size: 16.0, blades_per_side: 32 },
];

/// The row a `gfx_grass_quality` value selects, or `None` when the grass is off (0, or below).
/// A value over the table's last row takes the last row.
///
/// The original also lowers the setting at start-up while its grass would not fit a fixed memory
/// budget (`0x0117B330`); we have no such budget, so the setting is always used as chosen.
pub fn quality_for_option(option: i64) -> Option<GrassQuality> {
    let row = option.clamp(0, QUALITY_TABLE.len() as i64 - 1) as usize;
    (row > 0).then(|| QUALITY_TABLE[row])
}

/// One blade picture of an atlas.
#[derive(Debug, Clone, PartialEq)]
pub struct GrassEntry {
    /// The picture's name in the `.tai` (`g2_hay3_800_400.tga`).
    pub name: String,
    /// Its kind, 0..[`KINDS`] ([`kind_of`]).
    pub kind: usize,
    /// Width and height in metres before the blade's random scale ([`size_of`]).
    pub size: [f32; 2],
    /// The atlas page that holds it, as the `.tai` writes it (`Temperate0.dds`).
    pub page: String,
    /// The picture's corners in the page's texture coordinates (top-left, bottom-right).
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
}

/// A kind's pictures, by the name's prefix (CONFIRMED `0x0121E190`): `g3…` → 3, `g2…` → 2,
/// `g1…` → 1, anything else → 0. The prefix is matched ignoring ASCII case (CONFIRMED: the compare
/// `0x004F4540` lowers `A`..`Z` on both sides).
pub fn kind_of(name: &str) -> usize {
    let prefix = |p: &str| name.get(..p.len()).is_some_and(|n| n.eq_ignore_ascii_case(p));
    if prefix("g3") {
        3
    } else if prefix("g2") {
        2
    } else if prefix("g1") {
        1
    } else {
        0
    }
}

/// A picture's size in metres from its name (CONFIRMED `0x011D0A10`): the name without its
/// extension ends in `_<width>_<height>` in millimetres (`g2_hay3_800_400.tga` → 0.8 × 0.4 m).
/// A missing height reads as 512, a missing width as 1024, and a number that does not read (or is
/// not above 0) as 512.
///
/// The first number is the width (CONFIRMED: `0x011D0A10` stores the next-to-last token at the
/// size pair's `+0` and the last at `+4`; `0x01248590` passes them in that order to `0x011EFBD0`,
/// which writes them as the vertex's size, whose first value grass.fx spreads across the blade).
pub fn size_of(name: &str) -> [f32; 2] {
    let mut stem = match name.rfind('.') {
        Some(p) if p > 0 => &name[..p],
        _ => name,
    };
    let mut token = |default: &'static str| match stem.rfind('_') {
        Some(p) if p > 0 => {
            let t = &stem[p + 1..];
            stem = &stem[..p];
            t
        }
        _ => default,
    };
    let height = token("512");
    let width = token("1024");
    [millimetres(width), millimetres(height)]
}

/// A size token in metres: its leading number (the rest ignored, as the C runtime reads it), 512
/// when there is none or it is not above 0.
fn millimetres(token: &str) -> f32 {
    let end = token.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '+' | '-'))).unwrap_or(token.len());
    let v = token[..end].parse::<f32>().ok().filter(|v| *v > 0.0).unwrap_or(512.0);
    v * 0.001
}

/// A grass type's pictures, sorted by kind.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GrassAtlas {
    /// Every picture, in the `.tai`'s order.
    pub entries: Vec<GrassEntry>,
    /// Per kind, the indices into `entries` of its pictures, in the `.tai`'s order (the order a
    /// blade's random draw indexes, CONFIRMED `0x0121E190` / `0x01223060`).
    pub kinds: [Vec<usize>; KINDS],
}

impl GrassAtlas {
    /// The pictures of a `.tai`, in file order.
    pub fn from_tai(tai: &TaiAtlas) -> Self {
        let mut out = Self::default();
        for name in &tai.order {
            let Some(e) = tai.entries.get(name) else { continue };
            let (uv_min, uv_max) = e.rect();
            let kind = kind_of(name);
            out.kinds[kind].push(out.entries.len());
            out.entries.push(GrassEntry { name: name.clone(), kind, size: size_of(name), page: e.page.clone(), uv_min, uv_max });
        }
        out
    }

    /// True when no kind has a picture: such a map draws no grass.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The grass map's name in its preset's folder, with no extension: the original asks its image
/// loader for `<preset folder>grassmap` (CONFIRMED `0x00ECAC80`).
pub const GRASS_MAP_STEM: &str = "grassmap";

/// The extensions the original's image loader tries after a name, in order; the first file that
/// exists is read (CONFIRMED `0x008D1340`, the five strings of the table at `0x015C1438`).
pub const IMAGE_EXTENSIONS: [&str; 5] = ["", ".jpg", ".tga", ".dds", ".png"];

/// Our open-format override of a preset's grass map. Unlike the original, where `.png` comes last
/// in [`IMAGE_EXTENSIONS`], it is looked for first, so a mod's PNG beside a shipped `.tga` wins.
pub const GRASS_MAP_OVERRIDE: &str = "grassmap.png";

/// A grass map file found in a preset's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrassMapFile {
    /// The file the colour (the four densities) is read from.
    pub path: String,
    /// For a `.jpg`, its `_alpha.jpg` when that exists: its luminance replaces the alpha
    /// ([`GrassMap::apply_alpha`]).
    pub alpha: Option<String>,
}

/// The `_alpha.jpg` the original's loader looks for beside a `.jpg` (CONFIRMED `0x008D1340`: only
/// when the extension it tried is `.jpg`, the name less its last 4 characters + `_alpha.jpg`).
pub fn alpha_jpg_path(path: &str) -> Option<String> {
    path.ends_with(".jpg").then(|| format!("{}_alpha.jpg", &path[..path.len() - 4]))
}

/// The grass map of the preset in `folder` (a VFS folder ending in `\`): our override
/// [`GRASS_MAP_OVERRIDE`] if present, else the first of [`GRASS_MAP_STEM`] + each of
/// [`IMAGE_EXTENSIONS`] that `exists`, with its `_alpha.jpg` when that exists. `None` when there is
/// none: the map has no grass.
pub fn find_grass_map(folder: &str, exists: impl Fn(&str) -> bool) -> Option<GrassMapFile> {
    let png = format!("{folder}{GRASS_MAP_OVERRIDE}");
    if exists(&png) {
        return Some(GrassMapFile { path: png, alpha: None });
    }
    let path = IMAGE_EXTENSIONS.iter().map(|ext| format!("{folder}{GRASS_MAP_STEM}{ext}")).find(|p| exists(p))?;
    let alpha = alpha_jpg_path(&path).filter(|a| exists(a));
    Some(GrassMapFile { path, alpha })
}

/// How an image file is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    Dds,
    Tga,
}

/// How the original's loader decodes `path` (CONFIRMED `0x008D1340`): a `.tga` with its own TGA
/// reader (`0x0086F5B0`); any other name through D3DX (`0x008D1710` → `0x010A6890`), which goes by
/// the file's content. INFERRED: which content D3DX takes for which format; we read the
/// signatures of the formats we decode (JPEG `FF D8`, PNG, `DDS `) and take anything else as TGA.
pub fn image_kind(path: &str, bytes: &[u8]) -> ImageKind {
    if path.ends_with(".tga") {
        ImageKind::Tga
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        ImageKind::Jpeg
    } else if bytes.starts_with(b"\x89PNG") {
        ImageKind::Png
    } else if bytes.starts_with(b"DDS ") {
        ImageKind::Dds
    } else {
        ImageKind::Tga
    }
}

/// The 8-bit value an `_alpha.jpg` pixel gives (CONFIRMED `0x004D6D60`, the 32-bit → 8-bit copy
/// `0x008D1970` decodes it with): `(76·R + 150·G + 28·B + 127) / 254`, integer. A grey pixel gives
/// its own value.
pub fn luminance(rgba: [u8; 4]) -> u8 {
    ((76 * rgba[0] as u32 + 150 * rgba[1] as u32 + 28 * rgba[2] as u32 + 127) / 254) as u8
}

/// A map's grass densities: `rgba[(row · width + col) · 4 + k]` is kind `k`'s density, row 0 the
/// map's +y edge and column 0 its −x edge (the ground-type map's layout).
///
/// The channels: R, G, B, A are `g0`, `g1`, `g2`, `g3` (CONFIRMED). `0x01248590` reads the image's
/// bytes +2, +1, +0, +3 for kinds 0..3 (the table `0x0141AE04`); the image is B, G, R, A in memory:
/// the TGA reader `0x0086F5B0` → `0x004E2680` copies a 32-bit, 8-alpha-bit TGA's stored B, G, R, A
/// pixels as they are (`0x004CDCA0` gives format 5 for it, `0x004D6210` the verbatim copy), and the
/// D3DX path decodes to `A8R8G8B8` (`0x010A6890`) and copies it the same way (`0x004D6210`).
///
/// CONFIRMED by data that a `grassmap.tga` read top-down has this layout: across the 51 shipped
/// maps with grass, its densities are most uniform within each ground type with no flip on 48
/// (`tests/battle_terrain_install.rs`, `every_grass_map_and_atlas_reads`). The TGA reader flips by
/// the header's descriptor (rows by bit 5, columns by bit 4: `0x004E2680`); D3DX images are
/// not flipped (`0x008D1710` passes no flip).
#[derive(Debug, Clone, PartialEq)]
pub struct GrassMap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl GrassMap {
    /// A map from RGBA8 pixels, row 0 first. `None` when the sizes do not match.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        (width > 0 && height > 0 && rgba.len() == width as usize * height as usize * 4).then_some(Self { width, height, rgba })
    }

    /// A grass map from a TGA file, laid out as the original's TGA reader lays it out: rows by the
    /// descriptor's bit 5 and columns right to left when bit 4 is set (CONFIRMED `0x004E2680`),
    /// both done by [`crate::tga::Tga::parse`].
    pub fn from_tga(bytes: &[u8]) -> Result<Self, String> {
        let tga = crate::tga::Tga::parse(bytes).map_err(|e| e.to_string())?;
        Self::new(tga.width, tga.height, tga.rgba).ok_or_else(|| "an empty image".to_string())
    }

    /// A grass map from a DDS file's first level.
    pub fn from_dds(bytes: &[u8]) -> Result<Self, String> {
        let dds = crate::dds::Dds::parse(bytes).map_err(|e| e.to_string())?;
        Self::new(dds.width, dds.height, dds.decode_rgba8(0)).ok_or_else(|| "an empty image".to_string())
    }

    /// Writes an `_alpha.jpg`'s [`luminance`] into the alpha of the texels it covers (CONFIRMED
    /// `0x008D1340`: alpha row `i`, column `j` → texel `(i, j)`); texels it does not cover keep
    /// theirs (a JPEG's alpha is opaque, 255).
    ///
    /// ORIGINAL BUG (`0x008D1340`): the original loops over the alpha image's own width and height
    /// and writes texel `j + i × colour width` with no bounds check, so an alpha image wider than
    /// the colour one spills into the next rows and a taller one writes past the image's pixels.
    /// We write only the texels both images have. Every vanilla pair is the same size (the
    /// `napoleon` install test `every_jpeg_grass_map_loads`), so vanilla is unchanged.
    pub fn apply_alpha(&mut self, alpha: &GrassMap) {
        let w = self.width.min(alpha.width) as usize;
        for i in 0..self.height.min(alpha.height) as usize {
            for j in 0..w {
                let a = (i * alpha.width as usize + j) * 4;
                let px = [alpha.rgba[a], alpha.rgba[a + 1], alpha.rgba[a + 2], alpha.rgba[a + 3]];
                self.rgba[(i * self.width as usize + j) * 4 + 3] = luminance(px);
            }
        }
    }

    /// The densities of the texel at `(u, v)` (0..1 across the map from its −x and +y edges),
    /// read as the original does: the texel `floor(u · width)`, `floor(v · height)`, kept on the map.
    pub fn densities(&self, u: f32, v: f32) -> [u8; KINDS] {
        let col = ((u * self.width as f32).floor().max(0.0) as u32).min(self.width - 1);
        let row = ((v * self.height as f32).floor().max(0.0) as u32).min(self.height - 1);
        let i = (row as usize * self.width as usize + col as usize) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

/// What [`place_cell`] needs about the map.
#[derive(Debug, Clone, Copy)]
pub struct Placement<'a> {
    /// The playable area's size in metres (level 0's `world_width`, `world_height`).
    pub map_width: f32,
    pub map_height: f32,
    pub quality: GrassQuality,
    /// No blade stands closer than half this to the map's edge: `5 × grass_blade_width`
    /// (CONFIRMED `0x01248590`; the variable's default 2.0 at `0x01839820`).
    pub edge_margin: f32,
    pub grass_map: &'a GrassMap,
    pub atlas: &'a GrassAtlas,
}

/// The original's `grass_blade_width` (CONFIRMED default 2.0, `0x01839820`).
pub const BLADE_WIDTH: f32 = 2.0;

/// A blade only grows where the ground is above this height in metres (CONFIRMED `0x01248590`):
/// none under the sea.
pub const MIN_GROUND_HEIGHT: f32 = 0.25;

/// One placed blade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blade {
    /// Its foot, in battle metres from the map centre (`ntw_sim` axes).
    pub x: f32,
    pub y: f32,
    /// The ground height there.
    pub ground: f32,
    /// Its picture, an index into [`GrassAtlas::entries`].
    pub entry: usize,
    /// Width and height in metres (the picture's size × the blade's random scale).
    pub width: f32,
    pub height: f32,
    /// The blade's own 16-bit random draw: [`blade_look`] turns it into its mirroring and phase.
    pub rand: u32,
}

/// The kind a blade draws, CONFIRMED `0x0124E070`: a random start kind and a random target in
/// 0..sum of the densities; walking the kinds from the start (wrapping), the first whose running
/// sum reaches the target. With no density at all, kind 0.
fn choose_kind(state: &mut u32, c: [u8; KINDS]) -> usize {
    let a = lcg(*state);
    let b = lcg(a);
    *state = b;
    let start = (hi(b) & 3) as usize;
    let sum: f32 = c.iter().map(|&v| v as f32).sum();
    if sum <= 0.0 {
        return 0;
    }
    let target = unit(a) * sum;
    let mut acc = 0.0;
    for k in (start..KINDS).chain(0..start) {
        acc += c[k] as f32;
        if acc >= target {
            return k;
        }
    }
    0
}

/// The blades of cell `(cx, cz)` of grass level `level` (CONFIRMED `0x01248590`), appended to
/// `out`. `cx` counts cells from the map's −x edge, `cz` from its +y edge; `ground(x, y)` is the
/// ground height at a battle position.
///
/// ORIGINAL BUG (`0x01248590`): the grass-map row is read at `z / map width` instead of
/// `z / map height` (the same function bounds z by the height; the row is not clamped, so a map
/// taller than wide reads past the image); we divide by the height. The two agree on every vanilla
/// preset (all square, `tests/battle_terrain_install.rs`), so vanilla is unchanged; a modded oblong
/// map gets its grass where its grass map puts it.
///
/// The arithmetic is the exe's, operation for operation in `f32` (so a spot on a texel's or the
/// margin's boundary falls the same way): the cell is `(c + 0.5)·s ± 0.5·s` and a spot is a draw ×
/// that cell's extent from its low side; the margin test is made from the map's centre; the
/// jitter is `draw × (spacing × 1/65535) − spacing × 0.5`; the grass-map texel is
/// `floor((1/width) × x × image width)`; the scale is `v − v × 0.3`.
///
/// `ground` is the caller's: the original reads level 0's heights bilinearly itself (origin
/// `+0x1C`/`+0x20`, scale `+0x2C`/`+0x30`, samples `+0x34`, row stride `+0x4` of its level
/// record); ours is `BattleMap::height_at`, the same bilinear rule on level 0's samples. INFERRED:
/// that the record's origin and scale are half the map and 1 / sample spacing (the fill of those
/// fields is not traced), and the order of the interpolation's float operations differs.
pub fn place_cell(p: &Placement<'_>, cx: u32, cz: u32, level: u32, ground: impl Fn(f32, f32) -> f32, out: &mut Vec<Blade>) {
    let q = p.quality;
    let s = (1u32 << level.min(31)) as f32 * q.cell_size;
    let n = q.blades_per_side;
    let sp = s / n.max(1) as f32;
    let (w, h) = (p.map_width, p.map_height);
    let half_s = s * 0.5;
    let x_mid = (cx as f32 + 0.5) * s;
    let x_low = x_mid - half_s;
    let z_low = (cz as f32 + 0.5) * s - half_s;
    let extent = (half_s + x_mid) - x_low;
    let m = p.edge_margin;
    let (x_min, x_max) = ((w - m) * -0.5, (w - m) * 0.5);
    let (z_min, z_max) = ((h - m) * -0.5, (h - m) * 0.5);
    let (half_w, half_h) = (w * 0.5, h * 0.5);
    let (jitter_step, jitter_half) = (sp * INV_65535, sp * 0.5);
    let mut seed = cx.wrapping_add(cz.wrapping_mul(level.wrapping_add(32)));
    for _ in 0..n * n {
        let r1 = lcg(seed);
        seed = lcg(r1);
        let pz = unit(r1) * extent + z_low;
        let px = unit(seed) * extent + x_low;
        let (from_x, from_z) = (px - half_w, pz - half_h);
        if !(from_x >= x_min && from_z >= z_min && x_max > from_x && z_max > from_z) {
            continue;
        }
        let r3 = lcg(seed);
        let r4 = lcg(r3);
        let r5 = lcg(r4);
        let mut state = r5;
        let jx = (hi(r4) as f32 * jitter_step - jitter_half) + px;
        let jz = (hi(r3) as f32 * jitter_step - jitter_half) + pz;
        let c = p.grass_map.densities((1.0 / w) * jx, (1.0 / h) * jz);
        let kind = choose_kind(&mut state, c);
        let accepted = c[kind] as f32 >= hi(r5) as f32 * INV_257;
        let q1 = lcg(state);
        let q2 = lcg(q1);
        let q3 = lcg(q2);
        seed = lcg(q3);
        if !accepted {
            continue;
        }
        let list = &p.atlas.kinds[kind];
        if list.is_empty() {
            continue;
        }
        let entry = list[hi(q1) as usize % list.len()];
        let base = if unit(q3) < 0.1 { 1.05 } else { 0.75 };
        let v = hi(q2) as f32 * SCALE_SPREAD + base;
        let scale = v - v * 0.3;
        let (x, y) = (jx - half_w, -(jz - half_h));
        let g = ground(x, y);
        if g > MIN_GROUND_HEIGHT {
            let size = p.atlas.entries[entry].size;
            out.push(Blade { x, y, ground: g, entry, width: scale * size[0], height: scale * size[1], rand: hi(seed) });
        }
    }
}

/// A blade's look from its random draw (CONFIRMED `0x011EFBD0`): whether its picture is mirrored,
/// and its phase 0..=254 (the sway's offset and the fade distance's spread).
pub fn blade_look(rand: u32) -> (bool, u8) {
    let t = lcg(rand);
    let mirrored = unit(t) < 0.5;
    let mut v = lcg(t);
    while hi(v) == 0 {
        v = lcg(v);
    }
    (mirrored, (hi(v) % 255) as u8)
}

/// The texture coordinate of corner `k` (0 and 1 the foot, 2 and 3 the top; even `k` the left
/// side) of a picture, CONFIRMED `0x011EFBD0`. A mirrored blade swaps the picture's two sides.
pub fn corner_uv(e: &GrassEntry, k: usize, mirrored: bool) -> [f32; 2] {
    let left = k.is_multiple_of(2);
    let u = if left == mirrored { e.uv_min[0] } else { e.uv_max[0] };
    let v = if k < 2 { e.uv_max[1] } else { e.uv_min[1] };
    [u, v]
}

/// The cells `(first, last)` (inclusive, per axis) that have grass around a camera at battle
/// position `(x, y)`: the camera's cell ± the radius, kept on the map (CONFIRMED `0x01247E90`).
/// `None` when the camera's window misses the map entirely.
pub fn cell_window(p: &Placement<'_>, x: f32, y: f32, level: u32) -> Option<([u32; 2], [u32; 2])> {
    let s = (1u32 << level.min(31)) as f32 * p.quality.cell_size;
    let cells = [(p.map_width / s).ceil() as i64, (p.map_height / s).ceil() as i64];
    let at = [((x + p.map_width / 2.0) / s).floor() as i64, ((p.map_height / 2.0 - y) / s).floor() as i64];
    let r = p.quality.radius_cells as i64;
    let mut first = [0u32; 2];
    let mut last = [0u32; 2];
    for k in 0..2 {
        let lo = (at[k] - r).max(0);
        let hi = (at[k] + r).min(cells[k] - 1);
        if lo > hi {
            return None;
        }
        first[k] = lo as u32;
        last[k] = hi as u32;
    }
    Some((first, last))
}

/// The gust the grass leans with, advanced by `dt` seconds (CONFIRMED `0x01247E90`): it grows by
/// `push × ramp × dt`, where `ramp = low × (1 − t) + high × t` with `t = (level − 1) × 0.01010101`
/// runs from [`GrassLook::gust_rampup_low`] to [`GrassLook::gust_rampup_high`] as the weather's
/// wind level runs from 1 to 100; then a positive gust loses `gust / falloff × dt`
/// ([`GrassLook::gust_falloff`]), never going below 0.
/// PROVISIONAL: what the two weather values (`push`, `level`) are; see the notes.
pub fn advance_gust(gust: f32, push: f32, level: f32, dt: f32, look: &GrassLook) -> f32 {
    let t = (level - 1.0) * 0.010_101_01;
    let ramp = look.gust_rampup_high * t + look.gust_rampup_low * (1.0 - t);
    let mut g = push * ramp * dt + gust;
    if g > 0.0 {
        let loss = g / look.gust_falloff * dt;
        g = if g - loss >= 0.0 { g - loss } else { 0.0 };
    }
    g
}

/// The numbers of the grass's material description ([`crate::material::GRASS_MATERIAL`],
/// `materials/vanilla/grass.toml`): how a blade's quad is built, fades and sways. The file says
/// what each one means and where it comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrassLook {
    /// The alpha test: a pixel whose alpha is below this is not drawn.
    pub cutoff: f32,
    pub spread: f32,
    pub rise: f32,
    pub lift: f32,
    pub view_lean: f32,
    /// The corner weights: left, right, foot, top.
    pub corners: [f32; 4],
    pub fade_distance: f32,
    pub fade_spread: f32,
    pub fade_start: f32,
    pub fade_width: f32,
    pub fade_height_weight: f32,
    pub dissolve_start: f32,
    pub alpha_gain: f32,
    pub sway_amplitude: f32,
    pub sway_speed: f32,
    pub sway_speed_spread: f32,
    pub sway_phase: f32,
    pub gust_scale: f32,
    pub grass_wind_scale: f32,
    /// The gust's growth factor at wind level 1 and 100, and its decay time ([`advance_gust`]).
    pub gust_rampup_low: f32,
    pub gust_rampup_high: f32,
    pub gust_falloff: f32,
}

impl GrassLook {
    /// The numbers of `desc` (the game's grass description, a mod's when it ships one); a number
    /// it leaves out, or no description at all, keeps the built-in file's value.
    pub fn from_material(desc: Option<&crate::material::MaterialDescription>) -> Self {
        use crate::material::{BUILT_IN, GRASS_MATERIAL, parse_material};
        let text = BUILT_IN.iter().find(|(k, _)| *k == GRASS_MATERIAL).map(|(_, t)| *t).expect("the grass description is built in");
        let built_in = parse_material(GRASS_MATERIAL, text).expect("the built-in grass description parses (tested)");
        let get = |name: &str| {
            let find = |d: &crate::material::MaterialDescription| d.params.iter().find(|(n, _)| n == name).map(|(_, v)| *v);
            desc.and_then(find).or_else(|| find(&built_in)).unwrap_or_else(|| panic!("the built-in grass description has no `{name}`"))
        };
        Self {
            cutoff: desc.unwrap_or(&built_in).alpha.cutoff,
            spread: get("spread"),
            rise: get("rise"),
            lift: get("lift"),
            view_lean: get("view_lean"),
            corners: [get("corner_left"), get("corner_right"), get("corner_foot"), get("corner_top")],
            fade_distance: get("fade_distance"),
            fade_spread: get("fade_spread"),
            fade_start: get("fade_start"),
            fade_width: get("fade_width"),
            fade_height_weight: get("fade_height_weight"),
            dissolve_start: get("dissolve_start"),
            alpha_gain: get("alpha_gain"),
            sway_amplitude: get("sway_amplitude"),
            sway_speed: get("sway_speed"),
            sway_speed_spread: get("sway_speed_spread"),
            sway_phase: get("sway_phase"),
            gust_scale: get("gust_scale"),
            grass_wind_scale: get("grass_wind_scale"),
            gust_rampup_low: get("gust_rampup_low"),
            gust_rampup_high: get("gust_rampup_high"),
            gust_falloff: positive_falloff(get("gust_falloff"), &built_in),
        }
    }

    /// The weights of corner `k` (the [`corner_uv`] numbering: even `k` one side, `k < 2` the foot).
    pub fn corner_weights(&self, k: usize) -> [f32; 2] {
        [self.corners[k % 2], self.corners[2 + usize::from(k >= 2)]]
    }

    /// Where corner weights `w` put the corner, as (across, up) fractions of the blade's width and
    /// height: `spread × (w − 0.5) + (0, rise)` (grass.fx `calculate_common`).
    pub fn corner_offset(&self, w: [f32; 2]) -> [f32; 2] {
        [self.spread * (w[0] - 0.5), self.spread * (w[1] - 0.5) + self.rise]
    }
}

/// `falloff` when it is a positive number; otherwise (a mod's `gust_falloff = 0` would divide the
/// gust by zero in [`advance_gust`]) the built-in value, with a warning. [`GrassLook::from_material`]
/// runs once per battle load, so the warning is logged once per load.
fn positive_falloff(falloff: f32, built_in: &crate::material::MaterialDescription) -> f32 {
    if falloff > 0.0 && falloff.is_finite() {
        return falloff;
    }
    let fallback = built_in.params.iter().find(|(n, _)| n == "gust_falloff").map(|(_, v)| *v).expect("the built-in grass description has `gust_falloff`");
    log::warn!("Grass material: gust_falloff = {falloff} must be a positive number; using the built-in {fallback}");
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The built-in description gives grass.fx's numbers, and its corner weights build a quad the
    /// blade's width across, centred on its foot, from the ground to its height.
    #[test]
    fn the_built_in_look_is_grass_fx() {
        let look = GrassLook::from_material(None);
        assert_eq!((look.fade_distance, look.fade_start, look.alpha_gain, look.gust_scale), (78.0, 0.65, 1.5, 0.0004));
        assert!((look.cutoff - 128.0 / 255.0).abs() < 1e-4);
        let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5;
        assert!(near(look.corner_offset(look.corner_weights(0)), [-0.5, 0.0]));
        assert!(near(look.corner_offset(look.corner_weights(1)), [0.5, 0.0]));
        assert!(near(look.corner_offset(look.corner_weights(2)), [-0.5, 1.0]));
        assert!(near(look.corner_offset(look.corner_weights(3)), [0.5, 1.0]));
        // A mod's description replaces the numbers it gives and keeps the rest.
        let modded = crate::material::parse_material(crate::material::GRASS_MATERIAL, "[params]\nfade_distance = 150.0\n").unwrap();
        let m = GrassLook::from_material(Some(&modded));
        assert_eq!(m.fade_distance, 150.0);
        assert_eq!(m.sway_speed, look.sway_speed);
    }

    const TAI: &str = "g2_hay3_800_400.tga\t\tTemperate0.dds, 0, 2D, 0.000000, 0.000000, 0.000000, 0.250000, 0.250000\r\n\
g1_short3_1000_500.tga\t\tTemperate0.dds, 0, 2D, 0.000000, 0.250000, 0.000000, 0.250000, 0.250000\r\n\
g0_longgrass2_800_400.tga\t\tTemperate0.dds, 0, 2D, 0.750000, 0.250000, 0.000000, 0.250000, 0.250000\r\n\
g3_plant3_200_400.tga\t\tTemperate0.dds, 0, 2D, 0.250000, 0.500000, 0.000000, 0.125000, 0.500000\r\n\
g0_longgrass1_800_400.tga\t\tTemperate0.dds, 0, 2D, 0.000000, 0.500000, 0.000000, 0.250000, 0.250000\r\n";

    fn atlas() -> GrassAtlas {
        GrassAtlas::from_tai(&TaiAtlas::read(TAI).unwrap())
    }

    fn uniform_map(c: [u8; 4]) -> GrassMap {
        GrassMap::new(4, 4, c.repeat(16)).unwrap()
    }

    fn placement<'a>(map: &'a GrassMap, atlas: &'a GrassAtlas) -> Placement<'a> {
        Placement { map_width: 256.0, map_height: 256.0, quality: QUALITY_TABLE[2], edge_margin: 5.0 * BLADE_WIDTH, grass_map: map, atlas }
    }

    /// The constants are the exe's floats (as the decompiler prints them, 0x01248590).
    #[test]
    fn the_constants_are_the_exes() {
        assert_eq!(INV_257, 0.003_891_050_6_f32);
        // The exe's constant, not 0.15 / 65535 rounded once (that is a different f32).
        assert!((SCALE_SPREAD as f64 - 0.15 / 65535.0).abs() < 1e-12);
        assert_eq!(INV_65535, 1.525_902_2e-5_f32);
    }

    /// The loader's order: our PNG override, then `grassmap` + "", .jpg (with its `_alpha.jpg` when
    /// there is one), .tga, .dds, .png.
    #[test]
    fn the_grass_map_is_found_in_the_loaders_order() {
        let find = |files: &[&str]| find_grass_map(r"p\", |p: &str| files.iter().any(|f| format!(r"p\{f}") == p));
        let file = |path: &str, alpha: Option<&str>| Some(GrassMapFile { path: format!(r"p\{path}"), alpha: alpha.map(|a| format!(r"p\{a}")) });
        assert_eq!(find(&["grassmap.tga", "grassmap.png"]), file("grassmap.png", None));
        assert_eq!(find(&["grassmap.tga", "grassmap.jpg", "grassmap_alpha.jpg"]), file("grassmap.jpg", Some("grassmap_alpha.jpg")));
        assert_eq!(find(&["grassmap.tga", "grassmap.jpg"]), file("grassmap.jpg", None));
        assert_eq!(find(&["grassmap.dds", "grassmap.tga"]), file("grassmap.tga", None));
        assert_eq!(find(&["grassmap.dds"]), file("grassmap.dds", None));
        assert_eq!(find(&["grassmap", "grassmap.tga"]), file("grassmap", None));
        assert_eq!(find(&["grassmap_alpha.jpg", "other.tga"]), None);
        assert_eq!(alpha_jpg_path(r"a\grassmap.jpg").as_deref(), Some(r"a\grassmap_alpha.jpg"));
        assert_eq!(alpha_jpg_path(r"a\grassmap.tga"), None);
    }

    #[test]
    fn an_image_is_decoded_by_its_name_then_its_content() {
        assert_eq!(image_kind("x.tga", b"\x89PNG"), ImageKind::Tga);
        assert_eq!(image_kind("x.jpg", &[0xFF, 0xD8, 0xFF]), ImageKind::Jpeg);
        assert_eq!(image_kind("x", b"\x89PNG\r\n"), ImageKind::Png);
        assert_eq!(image_kind("x.dds", b"DDS |"), ImageKind::Dds);
        assert_eq!(image_kind("x", &[0, 0, 2]), ImageKind::Tga);
    }

    /// 0x004D6D60's weights: a grey pixel keeps its value, white is 255, black 0.
    #[test]
    fn the_alpha_is_the_luminance() {
        for v in [0u8, 1, 77, 128, 254, 255] {
            assert_eq!(luminance([v, v, v, 255]), v);
        }
        assert_eq!(luminance([255, 0, 0, 255]), 76);
        assert_eq!(luminance([0, 255, 0, 255]), 151);
        assert_eq!(luminance([0, 0, 255, 255]), 28);
    }

    /// The alpha image's luminance goes into the texels both images have; a larger alpha image
    /// writes nothing outside the colour one (the ORIGINAL BUG fixed), a smaller one leaves the
    /// rest at their alpha.
    #[test]
    fn the_alpha_jpg_writes_only_the_overlap() {
        let grey = |w: u32, h: u32, v: u8| GrassMap::new(w, h, [v, v, v, 255].repeat((w * h) as usize)).unwrap();
        let mut m = grey(2, 2, 9);
        m.apply_alpha(&grey(2, 2, 40));
        assert!(m.rgba.as_chunks::<4>().0.iter().all(|p| *p == [9, 9, 9, 40]));
        let mut m = grey(2, 2, 9);
        m.apply_alpha(&grey(3, 5, 40));
        assert!(m.rgba.as_chunks::<4>().0.iter().all(|p| *p == [9, 9, 9, 40]));
        assert_eq!(m.rgba.len(), 16);
        let mut m = grey(2, 2, 9);
        m.apply_alpha(&grey(1, 1, 40));
        let alphas: Vec<u8> = m.rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        assert_eq!(alphas, [40, 255, 255, 255]);
        // Row i of the alpha image is row i of the map, column j column j.
        let mut m = grey(2, 2, 9);
        let mut a = grey(2, 2, 0);
        a.rgba[(2 + 1) * 4..][..3].copy_from_slice(&[200; 3]);
        m.apply_alpha(&a);
        assert_eq!(m.rgba[(2 + 1) * 4 + 3], 200);
        assert_eq!(m.rgba[3], 0);
    }

    /// A TGA's descriptor bit 4 mirrors the columns (0x004E2680), bit 5 the rows.
    #[test]
    fn a_tga_is_laid_out_by_its_descriptor() {
        let tga = |desc: u8| {
            let mut b = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 32, desc];
            // Stored B, G, R, A: pixels 1, 2 then 3, 4 (their R).
            for r in [1u8, 2, 3, 4] {
                b.extend([0, 0, r, 255]);
            }
            let m = GrassMap::from_tga(&b).unwrap();
            m.rgba.as_chunks::<4>().0.iter().map(|p| p[0]).collect::<Vec<u8>>()
        };
        assert_eq!(tga(0x28), [1, 2, 3, 4]);
        assert_eq!(tga(0x28 | 0x10), [2, 1, 4, 3]);
        assert_eq!(tga(0x08), [3, 4, 1, 2]);
        assert_eq!(tga(0x08 | 0x10), [4, 3, 2, 1]);
    }

    #[test]
    fn the_atlas_path_is_the_grass_types_tai() {
        assert_eq!(atlas_path("Alpine_winter"), r"battleterrain\grass\alpine_winter.tai");
    }

    /// Kinds from the name's prefix, in the file's order (the order a blade's draw indexes).
    #[test]
    fn kinds_follow_the_names_in_file_order() {
        let a = atlas();
        assert_eq!(a.kinds, [vec![2, 4], vec![1], vec![0], vec![3]]);
        assert_eq!(kind_of("G3_upper"), 3, "the prefix is matched ignoring case");
        assert_eq!(kind_of("g"), 0);
        assert_eq!(kind_of("gé_x"), 0, "a prefix cut inside a character is no match");
        assert_eq!(kind_of("plant"), 0);
        let plant = &a.entries[3];
        assert_eq!((plant.uv_min, plant.uv_max), ([0.25, 0.5], [0.375, 1.0]));
        assert_eq!(plant.page, "Temperate0.dds");
    }

    #[test]
    fn sizes_come_from_the_names() {
        assert_eq!(size_of("g2_hay3_800_400.tga"), [0.8, 0.4]);
        assert_eq!(size_of("g1_short3_1000_500.tga"), [1.0, 0.5]);
        // One number: it is the height, the width is the default 1024.
        assert_eq!(size_of("plant_300.tga"), [1.024, 0.3]);
        // None: both defaults. A number that does not read, or is not above 0, is 512.
        assert_eq!(size_of("plant.tga"), [1.024, 0.512]);
        assert_eq!(size_of("g0_a_b_x.tga"), [0.512, 0.512]);
        assert_eq!(size_of("g0_a_0_-4.tga"), [0.512, 0.512]);
        // A leading '_' is not a separator, nor a leading '.'.
        assert_eq!(size_of("_400"), [1.024, 0.512]);
        assert_eq!(size_of("g0_a_600_300"), [0.6, 0.3]);
    }

    /// The setting the options page shows (`gfx_grass_quality`, registered default 2) picks its row;
    /// 0 turns the grass off; out-of-range values clamp.
    #[test]
    fn the_option_maps_to_its_row() {
        use crate::preferences::registered_default;
        let shown: i64 = registered_default("gfx_grass_quality").expect("registered").parse().expect("an int");
        assert_eq!(shown, 2);
        assert_eq!(quality_for_option(shown), Some(GrassQuality { radius_cells: 5, cell_size: 16.0, blades_per_side: 24 }));
        assert_eq!(quality_for_option(0), None);
        assert_eq!(quality_for_option(-3), None);
        assert_eq!(quality_for_option(1).unwrap().blades_per_side, 16);
        assert_eq!(quality_for_option(4).unwrap().blades_per_side, 32);
        assert_eq!(quality_for_option(9), quality_for_option(4));
    }

    /// The kind draw: the start kind and target come from two random steps; a lone density always
    /// wins; no density gives kind 0.
    #[test]
    fn the_kind_draw_follows_the_densities() {
        for seed in 0..200u32 {
            let mut s = seed;
            assert_eq!(choose_kind(&mut s, [0, 0, 200, 0]), 2);
            assert_eq!(s, lcg(lcg(seed)), "two steps used");
            let mut s = seed;
            assert_eq!(choose_kind(&mut s, [0; 4]), 0);
        }
        // Hand-worked: seed 0 → a = 2531011 (hi 38), b = lcg(a) = 505908858 (hi 7719 → start 3);
        // target = 38/65535 × 40 ≈ 0.023; from kind 3 (density 10) the sum reaches it at once.
        let mut s = 0;
        assert_eq!(hi(lcg(0)), 38);
        assert_eq!(lcg(lcg(0)), 505_908_858);
        assert_eq!(hi(lcg(lcg(0))) & 3, 3);
        assert_eq!(choose_kind(&mut s, [10, 10, 10, 10]), 3);
    }

    /// A cell is the same every time it is built, and the blades stay in the cell's square give or
    /// take half the jitter spacing, off the map's edge margin, on ground above 0.25 m.
    #[test]
    fn a_cell_is_placed_the_same_every_time() {
        let a = atlas();
        let m = uniform_map([255, 255, 255, 255]);
        let p = placement(&m, &a);
        let (mut one, mut two) = (Vec::new(), Vec::new());
        place_cell(&p, 3, 5, 0, |_, _| 10.0, &mut one);
        place_cell(&p, 3, 5, 0, |_, _| 10.0, &mut two);
        assert_eq!(one, two);
        // Full density on every kind: every candidate is kept (24² of them).
        assert_eq!(one.len(), 24 * 24);
        let sp = 16.0 / 24.0;
        for b in &one {
            let (jx, jz) = (b.x + 128.0, 128.0 - b.y);
            assert!(jx >= 48.0 - sp / 2.0 && jx < 64.0 + sp / 2.0, "{jx}");
            assert!(jz >= 80.0 - sp / 2.0 && jz < 96.0 + sp / 2.0, "{jz}");
            assert!(b.rand <= 0xFFFF);
            let size = a.entries[b.entry].size;
            let scale = b.width / size[0];
            assert!((b.height / size[1] - scale).abs() < 1e-5);
            assert!((0.7 * 0.75..=0.7 * 1.2).contains(&scale), "{scale}");
        }
        // Another cell draws other blades.
        let mut other = Vec::new();
        place_cell(&p, 4, 5, 0, |_, _| 10.0, &mut other);
        assert_ne!(one[0].x - 16.0, other[0].x);
    }

    #[test]
    fn no_grass_on_low_ground_off_the_edge_or_where_the_map_says_none() {
        let a = atlas();
        let full = uniform_map([255; 4]);
        let p = placement(&full, &a);
        let mut out = Vec::new();
        place_cell(&p, 3, 5, 0, |_, _| MIN_GROUND_HEIGHT, &mut out);
        assert!(out.is_empty(), "the ground must be above 0.25 m");
        // The corner cell loses the blades within 5 m of the edge.
        place_cell(&p, 0, 0, 0, |_, _| 1.0, &mut out);
        assert!(!out.is_empty() && out.len() < 24 * 24);
        let sp = 16.0 / 24.0;
        assert!(out.iter().all(|b| b.x + 128.0 >= 5.0 - sp / 2.0 && 128.0 - b.y >= 5.0 - sp / 2.0));
        // A cell past the map's far edge has none.
        out.clear();
        place_cell(&p, 16, 0, 0, |_, _| 1.0, &mut out);
        assert!(out.is_empty());
        // An empty grass map grows (almost) nothing: a blade needs a draw of 0 out of 65536.
        let empty = uniform_map([0; 4]);
        let p = placement(&empty, &a);
        let mut n = 0;
        for c in 0..16 {
            out.clear();
            place_cell(&p, c, c, 0, |_, _| 1.0, &mut out);
            n += out.len();
        }
        assert!(n <= 1, "{n}");
    }

    /// Half density keeps about half the candidates, and only the kinds the map grows appear.
    #[test]
    fn the_density_is_the_chance_a_blade_is_kept() {
        let a = atlas();
        let m = uniform_map([0, 128, 0, 0]);
        let p = placement(&m, &a);
        let mut out = Vec::new();
        for c in 1..9 {
            place_cell(&p, c, 4, 0, |_, _| 1.0, &mut out);
        }
        let share = out.len() as f32 / (8.0 * 576.0);
        assert!((share - 0.5).abs() < 0.05, "{share}");
        assert!(out.iter().all(|b| a.entries[b.entry].kind == 1));
        // A kind without pictures grows nothing.
        let none = GrassAtlas::from_tai(&TaiAtlas::read(&TAI.lines().take(1).collect::<Vec<_>>().join("\n")).unwrap());
        let m = uniform_map([255, 0, 0, 0]);
        let p = placement(&m, &none);
        out.clear();
        place_cell(&p, 4, 4, 0, |_, _| 1.0, &mut out);
        assert!(out.is_empty());
    }

    /// The ORIGINAL BUG fix: on an oblong map the grass-map row follows the map's height. A map twice
    /// as tall as wide with grass only in its top half grows grass only north of the centre.
    #[test]
    fn the_row_follows_the_maps_height() {
        let a = atlas();
        let mut rgba = [255u8; 4].repeat(2);
        rgba.extend([0u8; 4].repeat(2));
        let m = GrassMap::new(1, 4, rgba).unwrap();
        let p = Placement { map_width: 128.0, map_height: 256.0, ..placement(&m, &a) };
        let mut out = Vec::new();
        for cz in 0..16 {
            for cx in 0..8 {
                place_cell(&p, cx, cz, 0, |_, _| 1.0, &mut out);
            }
        }
        assert!(out.len() > 1000);
        // (An empty texel still keeps a candidate whose draw is exactly 0, 1 in 65536.)
        assert!(out.iter().filter(|b| b.y < 0.0).count() <= 1, "grass south of the centre");
    }

    #[test]
    fn a_blades_look_and_corners() {
        let e = &atlas().entries[0];
        let mut mirrored = 0;
        // A blade's draw is the top half of a random step (as `place_cell` stores it).
        let mut s = 1u32;
        for _ in 0..1000 {
            s = lcg(s);
            let (m, phase) = blade_look(hi(s));
            mirrored += m as u32;
            assert!(phase < 255);
        }
        assert!((400..600).contains(&mirrored), "{mirrored}");
        assert_eq!(corner_uv(e, 0, false), [0.25, 0.25]);
        assert_eq!(corner_uv(e, 1, false), [0.0, 0.25]);
        assert_eq!(corner_uv(e, 2, false), [0.25, 0.0]);
        assert_eq!(corner_uv(e, 3, true), [0.25, 0.0]);
        assert_eq!(corner_uv(e, 0, true), [0.0, 0.25]);
    }

    #[test]
    fn the_window_is_the_cameras_cell_and_its_radius_on_the_map() {
        let a = atlas();
        let m = uniform_map([0; 4]);
        let p = placement(&m, &a);
        // The map centre: cell 8 of 16 each way.
        assert_eq!(cell_window(&p, 0.0, 0.0, 0), Some(([3, 3], [13, 13])));
        // Near the −x, +y corner the window is cut at the edge.
        assert_eq!(cell_window(&p, -120.0, 120.0, 0), Some(([0, 0], [5, 5])));
        // Far off the map.
        assert_eq!(cell_window(&p, 1000.0, 0.0, 0), None);
    }

    #[test]
    fn the_gust_grows_with_the_push_and_decays() {
        let look = GrassLook::from_material(None);
        // The exe's defaults (0x01839030, 0x01838A30, 0x018383C0).
        assert_eq!((look.gust_rampup_low, look.gust_rampup_high, look.gust_falloff), (1.0, 5.0, 1.5));
        let mut g = 0.0;
        for _ in 0..1000 {
            g = advance_gust(g, 1.0, 1.0, 0.01, &look);
        }
        // Settles where push × ramp = gust / falloff (ramp = low at level 1).
        assert!((g - 1.5).abs() < 0.05, "{g}");
        // One step from 0: push × ramp × dt, then the decay of that.
        let g = advance_gust(0.0, 1.0, 100.0, 0.1, &look);
        let ramp = 5.0 * 0.999_999_9 + 1.0 * (1.0 - 0.999_999_9);
        let grown = ramp * 0.1;
        assert!((g - (grown - grown / 1.5 * 0.1)).abs() < 1e-5, "{g}");
        assert!(g > advance_gust(0.0, 1.0, 1.0, 0.1, &look));
        // A decay past zero stops at zero; a negative gust is left alone.
        assert_eq!(advance_gust(0.5, 0.0, 1.0, 10.0, &look), 0.0);
        assert_eq!(advance_gust(-0.5, 0.0, 1.0, 1.0, &look), -0.5);
        // A mod's numbers are used.
        let modded = GrassLook { gust_rampup_low: 2.0, gust_falloff: 3.0, ..look };
        assert!((advance_gust(0.0, 1.0, 1.0, 0.1, &modded) - (0.2 - 0.2 / 3.0 * 0.1)).abs() < 1e-6);
    }

    /// A mod's `gust_falloff` of zero or less would divide the gust by zero: the built-in value
    /// is kept instead, and the gust stays finite.
    #[test]
    fn a_falloff_that_is_not_positive_keeps_the_built_in_one() {
        for bad in ["0.0", "-2.0"] {
            let text = format!("[params]\ngust_falloff = {bad}\n");
            let modded = crate::material::parse_material(crate::material::GRASS_MATERIAL, &text).unwrap();
            let look = GrassLook::from_material(Some(&modded));
            assert_eq!(look.gust_falloff, 1.5, "{bad}");
            assert!(advance_gust(0.0, 1.0, 1.0, 0.1, &look).is_finite());
        }
        let modded = crate::material::parse_material(crate::material::GRASS_MATERIAL, "[params]\ngust_falloff = 3.0\n").unwrap();
        assert_eq!(GrassLook::from_material(Some(&modded)).gust_falloff, 3.0);
    }
}

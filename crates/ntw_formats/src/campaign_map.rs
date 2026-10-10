//! Campaign map display data: `data\campaign_maps\<map>\display\` (loose files, read through the Vfs).
//!
//! Formats from `analysis/worker3/CAMPAIGN_MAP_GRAPHICS.md` (structure CONFIRMED on all 5 maps)
//! plus this module's own findings, written up in `analysis/campaign/CAMPAIGN_MAP.md`:
//!
//! | file | reader |
//! |---|---|
//! | `display\heightmap\heightmap.tga` (8-bit grey) | [`Heightmap`] |
//! | `display\supertexture\supertexture.stpi` + `.stpd` | [`SuperTexture`] |
//! | `display\{borders,roads,rivers,traderoutes}\*.rigid_spline` ("SPLN") | [`SplineFile`] |
//! | `regions.esf` | [`RegionMap`] |
//!
//! # Coordinates
//! The logic map (startpos positions, `regions.esf`) uses map units, x = east, z = north
//! (W3 §2.4, CONFIRMED). The display files (splines, models) use **display units**; see
//! [`DISPLAY_TO_LOGIC`] for the transform between them.

use crate::bytes::{Cursor, ReadError};
use crate::esf::{EsfFile, EsfNode, EsfRecord};
use crate::tga::Tga;
use std::fmt;

/// Logic map units per display unit (splines, `.rigid_model` placement): `1 / 0.0254`
/// (display units are metres when logic units are inches). Measured, not read from the exe:
/// border splines scaled by 39.37 (no offset, no flip) land on the `regions.esf` outline
/// vertices on all four big maps (mean distance 0.18 to 0.6 units; `campaign_probe fit2`).
/// INFERRED (strong); see `analysis/campaign/CAMPAIGN_MAP.md` §2.
pub const DISPLAY_TO_LOGIC: f32 = 1.0 / 0.0254;

/// Errors from the campaign map readers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CampaignMapError {
    /// The data ended early.
    Truncated {
        /// What was being read.
        what: &'static str,
    },
    /// A magic number or fixed field did not match.
    BadMagic {
        /// What was expected.
        expected: &'static str,
    },
    /// Bytes remain after the last record.
    TrailingBytes {
        /// How many.
        count: usize,
    },
    /// A zlib stream did not inflate to its stated size.
    Inflate {
        /// Which tile.
        tile: usize,
    },
    /// The TGA or ESF reader failed.
    Inner(String),
    /// A required ESF record is missing.
    Missing(&'static str),
}

impl fmt::Display for CampaignMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { what } => write!(f, "data ends inside the {what}"),
            Self::BadMagic { expected } => write!(f, "expected {expected}"),
            Self::TrailingBytes { count } => write!(f, "{count} bytes left over"),
            Self::Inflate { tile } => write!(f, "supertexture tile {tile} does not inflate"),
            Self::Inner(e) => write!(f, "{e}"),
            Self::Missing(what) => write!(f, "missing {what}"),
        }
    }
}

impl std::error::Error for CampaignMapError {}

fn trunc(what: &'static str) -> impl Fn(ReadError) -> CampaignMapError {
    move |_| CampaignMapError::Truncated { what }
}

// ---------------------------------------------------------------------------------------------
// SPLN
// ---------------------------------------------------------------------------------------------

/// One named polyline.
#[derive(Debug, Clone, PartialEq)]
pub struct Spline {
    /// e.g. `"border:eur_austria:1"`, `"road:..."`, `"river:..."`.
    pub name: String,
    /// u32 after the name (1 in every shipped spline; meaning UNKNOWN).
    pub flag: u32,
    /// Points in display units: (x, y = height, z).
    pub points: Vec<[f32; 3]>,
}

impl Spline {
    /// The text before the first `:` of the name (`border`, `road`, `river`, `land`, `sea`, `none`), lowercase.
    pub fn kind(&self) -> String {
        self.name.split(':').next().unwrap_or("").to_ascii_lowercase()
    }
}

/// A `.rigid_spline` file: `"SPLN"`, u32 version (1), u32 count, then splines
/// `{u16 len + UTF-16 name, u32 flag, u32 n, n x f32[3]}`. CONFIRMED (W3, all 903 files).
#[derive(Debug, Clone, PartialEq)]
pub struct SplineFile {
    /// The version field (1).
    pub version: u32,
    /// The splines, in file order.
    pub splines: Vec<Spline>,
}

impl SplineFile {
    /// Parses a whole file; it must end exactly after the last spline.
    pub fn read(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let mut c = Cursor::new(bytes);
        if c.take(4).map_err(trunc("magic"))? != b"SPLN" {
            return Err(CampaignMapError::BadMagic { expected: "SPLN" });
        }
        let version = c.u32().map_err(trunc("header"))?;
        let count = c.u32().map_err(trunc("header"))?;
        let mut splines = Vec::with_capacity((count as usize).min(c.remaining() / 10));
        for _ in 0..count {
            let name = c.utf16().map_err(trunc("spline name"))?;
            let flag = c.u32().map_err(trunc("spline"))?;
            let n = c.u32().map_err(trunc("spline"))? as usize;
            if n.saturating_mul(12) > c.remaining() {
                return Err(CampaignMapError::Truncated { what: "spline points" });
            }
            let mut points = Vec::with_capacity(n);
            for _ in 0..n {
                let x = c.f32().map_err(trunc("point"))?;
                let y = c.f32().map_err(trunc("point"))?;
                let z = c.f32().map_err(trunc("point"))?;
                points.push([x, y, z]);
            }
            splines.push(Spline { name, flag, points });
        }
        if c.remaining() != 0 {
            return Err(CampaignMapError::TrailingBytes { count: c.remaining() });
        }
        Ok(Self { version, splines })
    }
}

// ---------------------------------------------------------------------------------------------
// Heightmap
// ---------------------------------------------------------------------------------------------

/// `display\heightmap\heightmap.tga`: an 8-bit grey image covering the whole map bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct Heightmap {
    /// Width in pixels (4096 for Europe).
    pub width: u32,
    /// Height in pixels (2048 for Europe).
    pub height: u32,
    /// Grey values, **top row first** (row 0 = north edge, INFERRED from the TGA origin).
    pub values: Vec<u8>,
}

impl Heightmap {
    /// Decodes the TGA.
    pub fn read(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let t = Tga::decode(bytes).map_err(|e| CampaignMapError::Inner(format!("heightmap: {e}")))?;
        if t.indices.len() != (t.width * t.height) as usize {
            return Err(CampaignMapError::BadMagic { expected: "an 8-bit greyscale TGA" });
        }
        Ok(Self { width: t.width, height: t.height, values: t.indices })
    }

    /// The raw value at pixel (`col`, `row`), clamped to the image.
    pub fn at(&self, col: i64, row: i64) -> u8 {
        let c = col.clamp(0, self.width as i64 - 1) as usize;
        let r = row.clamp(0, self.height as i64 - 1) as usize;
        self.values[r * self.width as usize + c]
    }

    /// Bilinear sample at fractional pixel coordinates (pixel centres at +0.5).
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let (x, y) = (u - 0.5, v - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let p = |dx, dy| f32::from(self.at(x0 + dx, y0 + dy));
        let top = p(0, 0) * (1.0 - fx) + p(1, 0) * fx;
        let bottom = p(0, 1) * (1.0 - fx) + p(1, 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}

// ---------------------------------------------------------------------------------------------
// Supertexture
// ---------------------------------------------------------------------------------------------

/// One tile record of the `.stpi` index (20 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StpTile {
    /// Byte offset of the tile in `.stpd`.
    pub offset: u32,
    /// Byte size of the tile in `.stpd`.
    pub size: u32,
    /// Decompressed size (262,144 = 512x512 at 1 byte per pixel).
    pub raw_size: u32,
    /// Tile id (UNKNOWN numbering).
    pub id: u32,
    /// UNKNOWN (perhaps an average colour).
    pub value: u32,
}

/// One mip level of the supertexture: a grid of tiles, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StpLevel {
    /// Tiles across.
    pub tiles_x: u32,
    /// Tiles down.
    pub tiles_y: u32,
    /// `tiles_x * tiles_y` tiles, row-major (row 0 first).
    pub tiles: Vec<StpTile>,
}

/// The supertexture index (`.stpi`). CONFIRMED layout (W3 §3):
/// `u32 levels, u32 width, u32 height, u32 tile_size, u32 tile_bytes, u32 levels_again`,
/// then per level `{u32 tiles_x, u32 tiles_y, tiles}` (finest first), then `u32 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperTexture {
    /// Full virtual width in pixels (32768 for Europe).
    pub width: u32,
    /// Full virtual height in pixels (16384 for Europe).
    pub height: u32,
    /// Tile edge in pixels (512).
    pub tile_size: u32,
    /// Decompressed bytes per tile.
    pub tile_bytes: u32,
    /// The repeated level count (meaning UNKNOWN).
    pub unknown_6: u32,
    /// Levels, finest first.
    pub levels: Vec<StpLevel>,
}

impl SuperTexture {
    /// Parses the `.stpi` index.
    pub fn read_index(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let mut c = Cursor::new(bytes);
        let h = |c: &mut Cursor<'_>| c.u32().map_err(trunc("stpi header"));
        let (levels, width, height, tile_size, tile_bytes, unknown_6) = (h(&mut c)?, h(&mut c)?, h(&mut c)?, h(&mut c)?, h(&mut c)?, h(&mut c)?);
        let mut out = Vec::new();
        for _ in 0..levels.min(32) {
            let tiles_x = c.u32().map_err(trunc("stpi level"))?;
            let tiles_y = c.u32().map_err(trunc("stpi level"))?;
            let n = tiles_x as usize * tiles_y as usize;
            if n.saturating_mul(20) > c.remaining() {
                return Err(CampaignMapError::Truncated { what: "stpi tiles" });
            }
            let mut tiles = Vec::with_capacity(n);
            for _ in 0..n {
                let t = |c: &mut Cursor<'_>| c.u32().map_err(trunc("stpi tile"));
                tiles.push(StpTile { offset: t(&mut c)?, size: t(&mut c)?, raw_size: t(&mut c)?, id: t(&mut c)?, value: t(&mut c)? });
            }
            out.push(StpLevel { tiles_x, tiles_y, tiles });
        }
        let trailer = c.u32().map_err(trunc("stpi trailer"))?;
        if trailer != 1 || c.remaining() != 0 {
            return Err(CampaignMapError::TrailingBytes { count: c.remaining() });
        }
        Ok(Self { width, height, tile_size, tile_bytes, unknown_6, levels: out })
    }

    /// Inflates one tile from the `.stpd` bytes: 8 chunks of `{u32 packed, u32 raw, zlib}`.
    /// The result is `raw_size` bytes of block-compressed pixels (see [`decode_tile_rgba`](Self::decode_tile_rgba)).
    pub fn inflate_tile(stpd: &[u8], tile: &StpTile, tile_index: usize) -> Result<Vec<u8>, CampaignMapError> {
        let start = tile.offset as usize;
        let end = start.checked_add(tile.size as usize).filter(|&e| e <= stpd.len()).ok_or(CampaignMapError::Truncated { what: "stpd tile" })?;
        let mut c = Cursor::new(&stpd[start..end]);
        let mut out = Vec::with_capacity(tile.raw_size as usize);
        while c.remaining() > 0 {
            let packed = c.u32().map_err(trunc("stpd chunk"))? as usize;
            let raw = c.u32().map_err(trunc("stpd chunk"))? as usize;
            let data = c.take(packed).map_err(trunc("stpd chunk"))?;
            let chunk = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, raw)
                .map_err(|_| CampaignMapError::Inflate { tile: tile_index })?;
            if chunk.len() != raw {
                return Err(CampaignMapError::Inflate { tile: tile_index });
            }
            out.extend_from_slice(&chunk);
        }
        if out.len() != tile.raw_size as usize {
            return Err(CampaignMapError::Inflate { tile: tile_index });
        }
        Ok(out)
    }

    /// Decodes an inflated tile (512x512 DXT5 blocks, see `CAMPAIGN_MAP.md` §3) to RGBA8, top row first.
    pub fn decode_tile_rgba(&self, raw: &[u8]) -> Vec<u8> {
        crate::dds::decode_blocks_rgba8(crate::dds::DdsFormat::Dxt5, self.tile_size, self.tile_size, raw)
    }

    /// The RGBA8 pixels (north row first, `nx · tile_size` wide) of the `nx` × `ny` tiles of
    /// `level` from tile (`tx0`, `ty0`). `tile_bytes` gives one tile's own `.stpd` bytes (its
    /// `size` bytes from its `offset`), so the caller reads the file whole or by range.
    pub fn window_rgba(
        &self,
        level: usize,
        (tx0, ty0, nx, ny): (u32, u32, u32, u32),
        mut tile_bytes: impl FnMut(&StpTile) -> Result<Vec<u8>, CampaignMapError>,
    ) -> Result<Vec<u8>, CampaignMapError> {
        let lv = self.levels.get(level).ok_or(CampaignMapError::Missing("supertexture level"))?;
        if tx0.checked_add(nx).is_none_or(|e| e > lv.tiles_x) || ty0.checked_add(ny).is_none_or(|e| e > lv.tiles_y) {
            return Err(CampaignMapError::Missing("supertexture tiles outside the level"));
        }
        let ts = self.tile_size as usize;
        let w = nx as usize * ts;
        let mut out = vec![0u8; w * ny as usize * ts * 4];
        for ty in ty0..ty0 + ny {
            for tx in tx0..tx0 + nx {
                let i = (ty * lv.tiles_x + tx) as usize;
                let tile = lv.tiles.get(i).ok_or(CampaignMapError::Truncated { what: "stpi tiles" })?;
                let raw = Self::inflate_tile(&tile_bytes(tile)?, &StpTile { offset: 0, ..*tile }, i)?;
                let px = self.decode_tile_rgba(&raw);
                let (ox, oy) = ((tx - tx0) as usize * ts, (ty - ty0) as usize * ts);
                for row in 0..ts {
                    let dst = ((oy + row) * w + ox) * 4;
                    out[dst..dst + ts * 4].copy_from_slice(&px[row * ts * 4..(row + 1) * ts * 4]);
                }
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------
// regions.esf
// ---------------------------------------------------------------------------------------------

/// One area (a connected piece) of a region: triangles over the shared vertex list.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionArea {
    /// Triangle vertex indices into [`RegionMap::vertices`], 3 per triangle.
    pub faces: Vec<u32>,
    /// Closed outlines (vertex index loops).
    pub outlines: Vec<Vec<u32>>,
}

/// One building slot of a settlement (`slot_descriptions` v1).
#[derive(Debug, Clone, PartialEq)]
pub struct MapSlot {
    /// e.g. `settlement:eur_france:paris:settlement_4_slot:0`.
    pub key: String,
    /// e.g. `settlement_4_slot`, `port`, `farm` (the slot template).
    pub slot_type: String,
    /// Logic position (x, y = height, z).
    pub position: (f32, f32, f32),
    /// Facing as a u16 angle (65536 = one turn, INFERRED).
    pub angle: u16,
    /// The Coord2d after the position (#3). Equal to the position for every slot type except
    /// `port`, where it lies about 1.3 units seawards inside the port's sea footprint: INFERRED the
    /// dock (CONFIRMED values on the 4 maps with ports).
    pub dock: (f32, f32),
    /// The Coord2d arrays after the slot's model key (ports: 3 outlines; the third, the union of
    /// the other two, always reaches into a sea polygon of `pathfinding.esf`, CONFIRMED on all 67
    /// ports; INFERRED the port's footprint).
    pub footprints: Vec<Vec<(f32, f32)>>,
    /// #6 the slot's template model key, e.g. `nap_eur_port`, `nap_eur_town_com` (CONFIRMED: the
    /// names of `rigidmodels\campaignbuildings\templates\<culture>\<key>.rigid_model` files);
    /// empty when the slot has none.
    pub model: String,
}

/// A settlement from `settlement_and_slots`.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionSettlement {
    /// Logic position (x, z).
    pub position: (f32, f32),
    /// The building slots (town slots, ports, farms, ...).
    pub slots: Vec<MapSlot>,
}

/// One `region_data/regions` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct MapRegion {
    /// e.g. `eur_france`, or `eur_map_west` for the land outside the theatre.
    pub key: String,
    /// True for `"sea"`, false for `"land"`.
    pub is_sea: bool,
    /// Bounding box min (x, z).
    pub min: (f32, f32),
    /// Bounding box max (x, z).
    pub max: (f32, f32),
    /// The region's pieces.
    pub areas: Vec<RegionArea>,
    /// The settlement, if the region has one.
    pub settlement: Option<RegionSettlement>,
    /// Indices (into [`RegionMap::regions`]) of the regions sharing an outline edge with this one
    /// (land, sea, river or lake), from the outlines' `connectivity` triples (CONFIRMED structure:
    /// e.g. `eur_france` touches Picardie, Normandie, Bretagne, the Bay of Biscay, ...).
    pub neighbours: Vec<usize>,
}

/// The parts of `regions.esf` the map view needs (W3 §5 structure, CONFIRMED by parsing).
#[derive(Debug, Clone, PartialEq)]
pub struct RegionMap {
    /// Map bounds min (x, z), e.g. (-640, -320).
    pub bounds_min: (f32, f32),
    /// Map bounds max (x, z), e.g. (640, 320).
    pub bounds_max: (f32, f32),
    /// Playable theatre bounds min/max (e.g. (-410, -190)..(340, 195)).
    pub theatre: ((f32, f32), (f32, f32)),
    /// Region label positions by key (`theatres_and_region_keys`).
    pub labels: Vec<(String, (f32, f32))>,
    /// Shared vertex list (x, z).
    pub vertices: Vec<(f32, f32)>,
    /// All regions (land and sea), in file order.
    pub regions: Vec<MapRegion>,
    /// Trade node positions.
    pub trade_nodes: Vec<(String, (f32, f32))>,
    /// The ground-type map (`groundtypes`), one entry per `campaign_ground_types` key present.
    pub ground_types: Vec<GroundTypeRegion>,
}

/// One area of a ground type: its (outer?, outline points) loops.
pub type GroundArea = Vec<(bool, Vec<(f32, f32)>)>;

/// One ground type's polygons in `regions.esf` `groundtypes/region_data/regions[]` (CONFIRMED
/// structure: {utf16 `campaign_ground_types` key, ascii `land`, bounds, [areas], i32}; an area is
/// {bool, bool, bool, bounds, u16, [outlines], u16, u16} without faces; an outline is {bool outer,
/// bounds, u32[] vertex indices into `groundtypes/region_data/vertices`, [connectivity]}). The
/// types partition the map: the base type (`grassland` on the European map) is one area covering
/// the map whose other outlines are holes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GroundTypeRegion {
    /// The `campaign_ground_types` key.
    pub key: String,
    /// Areas, each a list of (outer?, outline points) loops.
    pub areas: Vec<GroundArea>,
}

/// Even-odd point-in-polygon test.
pub fn point_in_loop(pts: &[(f32, f32)], x: f32, z: f32) -> bool {
    let mut inside = false;
    let mut j = pts.len().wrapping_sub(1);
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.1 > z) != (b.1 > z) && x < (b.0 - a.0) * (z - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl GroundTypeRegion {
    /// Whether (x, z) lies in one of the areas: inside an outer outline and in none of its holes.
    pub fn contains(&self, x: f32, z: f32) -> bool {
        self.areas.iter().any(|a| {
            a.iter().any(|(outer, p)| *outer && point_in_loop(p, x, z)) && !a.iter().any(|(outer, p)| !*outer && point_in_loop(p, x, z))
        })
    }
}

fn read_ground_types(root: &EsfRecord) -> Vec<GroundTypeRegion> {
    let Some(data) = root.child("groundtypes").and_then(|g| g.child("region_data")) else { return Vec::new() };
    let Some(verts) = data.child("vertices").and_then(|v| v.children.first()).and_then(EsfNode::as_coord2d_array) else { return Vec::new() };
    let Some(regions) = find_array(&data.children, "regions") else { return Vec::new() };
    let mut out = Vec::new();
    for item in &regions.items {
        let key = item.first().and_then(EsfNode::as_str).unwrap_or_default().to_owned();
        let mut areas = Vec::new();
        for a in find_array(item, "areas").map(|arr| arr.items.as_slice()).unwrap_or_default() {
            let loops = find_array(a, "outlines")
                .map(|o| o.items.as_slice())
                .unwrap_or_default()
                .iter()
                .filter_map(|o| {
                    let outer = o.first()?.as_bool()?;
                    let idx = o.iter().find_map(EsfNode::as_u32_array)?;
                    Some((outer, idx.iter().filter_map(|i| verts.get(*i as usize).copied()).collect()))
                })
                .collect();
            areas.push(loops);
        }
        out.push(GroundTypeRegion { key, areas });
    }
    out
}

fn rec_child<'a>(r: &'a EsfRecord, name: &'static str) -> Result<&'a EsfRecord, CampaignMapError> {
    r.child(name).ok_or(CampaignMapError::Missing(name))
}

fn coord(n: Option<&EsfNode>) -> Option<(f32, f32)> {
    match n? {
        EsfNode::Coord2d(x, z) => Some((*x, *z)),
        _ => None,
    }
}

fn find_array<'a>(nodes: &'a [EsfNode], name: &str) -> Option<&'a crate::esf::EsfRecordArray> {
    nodes.iter().find_map(|n| n.as_record_array().filter(|a| a.name == name))
}

impl RegionMap {
    /// Parses `campaign_maps\<map>\regions.esf`.
    pub fn read(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let esf = EsfFile::from_bytes(bytes).map_err(|e| CampaignMapError::Inner(format!("regions.esf: {e}")))?;
        let root = &esf.root;
        let data = rec_child(root, "region_data")?;
        let vertices = rec_child(data, "vertices")?
            .children
            .first()
            .and_then(EsfNode::as_coord2d_array)
            .ok_or(CampaignMapError::Missing("region_data/vertices"))?
            .to_vec();
        let bounds_min = coord(data.children.get(1)).ok_or(CampaignMapError::Missing("region_data bounds"))?;
        let bounds_max = coord(data.children.get(2)).ok_or(CampaignMapError::Missing("region_data bounds"))?;

        let mut theatre = (bounds_min, bounds_max);
        let mut labels = Vec::new();
        if let Some(arr) = find_array(&root.children, "theatres_and_region_keys") {
            for item in &arr.items {
                let Some(t) = item.first().and_then(EsfNode::as_record) else { continue };
                if let (Some(a), Some(b)) = (coord(t.children.get(1)), coord(t.children.get(2))) {
                    theatre = (a, b);
                }
                if let Some(keys) = find_array(&t.children, "region_keys") {
                    for k in &keys.items {
                        if let (Some(key), Some(p)) = (k.first().and_then(EsfNode::as_str), coord(k.get(1))) {
                            labels.push((key.to_owned(), p));
                        }
                    }
                }
            }
        }

        let regions_arr = find_array(&data.children, "regions").ok_or(CampaignMapError::Missing("region_data/regions"))?;
        let mut regions = Vec::with_capacity(regions_arr.items.len());
        for item in &regions_arr.items {
            let key = item.first().and_then(EsfNode::as_str).unwrap_or_default().to_owned();
            let is_sea = item.get(1).and_then(EsfNode::as_str).is_some_and(|s| s.eq_ignore_ascii_case("sea"));
            let min = coord(item.get(2)).unwrap_or_default();
            let max = coord(item.get(3)).unwrap_or_default();
            let mut areas = Vec::new();
            if let Some(arr) = find_array(item, "areas") {
                for a in &arr.items {
                    let faces = a
                        .iter()
                        .find_map(|n| n.as_record().filter(|r| r.name == "faces"))
                        .and_then(|r| r.children.first())
                        .and_then(EsfNode::as_u32_array)
                        .map(<[u32]>::to_vec)
                        .unwrap_or_default();
                    let outlines = find_array(a, "outlines")
                        .map(|o| {
                            o.items
                                .iter()
                                .filter_map(|it| it.iter().find_map(EsfNode::as_u32_array).map(<[u32]>::to_vec))
                                .collect()
                        })
                        .unwrap_or_default();
                    areas.push(RegionArea { faces, outlines });
                }
            }
            let settlement = item.iter().find_map(|n| n.as_record().filter(|r| r.name == "settlement_and_slots")).and_then(|r| {
                let position = coord(r.children.first())?;
                let slots = find_array(&r.children, "slot_descriptions")
                    .map(|a| {
                        a.items
                            .iter()
                            .filter_map(|s| {
                                let key = s.first()?.as_str()?.to_owned();
                                let slot_type = s.get(1)?.as_str()?.to_owned();
                                let position = match s.get(2)? {
                                    EsfNode::Coord3d(x, y, z) => (*x, *y, *z),
                                    _ => return None,
                                };
                                let angle = match s.get(4) {
                                    Some(EsfNode::Angle(a)) => *a,
                                    _ => 0,
                                };
                                let dock = match s.get(3) {
                                    Some(EsfNode::Coord2d(x, z)) => (*x, *z),
                                    _ => (position.0, position.2),
                                };
                                let footprints = s
                                    .iter()
                                    .filter_map(|n| if let EsfNode::Coord2dArray(v) = n { Some(v.clone()) } else { None })
                                    .collect();
                                let model = s.get(6).and_then(|n| n.as_str()).unwrap_or_default().to_owned();
                                Some(MapSlot { key, slot_type, position, angle, dock, footprints, model })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(RegionSettlement { position, slots })
            });
            // Neighbours: every outline edge run carries a `connectivity` triple {u32 (region index << 16 |
            // area index), first vertex, last vertex} naming the region on the other side (0xFFFF: none).
            let mut neighbours: Vec<usize> = Vec::new();
            for a in find_array(item, "areas").map(|arr| arr.items.as_slice()).unwrap_or_default() {
                for o in find_array(a, "outlines").map(|o| o.items.as_slice()).unwrap_or_default() {
                    for c in find_array(o, "connectivity").map(|c| c.items.as_slice()).unwrap_or_default() {
                        let Some(v) = c.first().and_then(EsfNode::as_u32) else { continue };
                        let n = (v >> 16) as usize;
                        if n != 0xFFFF && n != regions.len() && !neighbours.contains(&n) {
                            neighbours.push(n);
                        }
                    }
                }
            }
            regions.push(MapRegion { key, is_sea, min, max, areas, settlement, neighbours });
        }

        let mut trade_nodes = Vec::new();
        if let Some(tn) = root.child("trade_nodes").and_then(|r| find_array(&r.children, "trade_nodes")) {
            for it in &tn.items {
                if let (Some(k), Some(p)) = (it.first().and_then(EsfNode::as_str), coord(it.get(1))) {
                    trade_nodes.push((k.to_owned(), p));
                }
            }
        }
        let ground_types = read_ground_types(root);
        Ok(Self { bounds_min, bounds_max, theatre, labels, vertices, regions, trade_nodes, ground_types })
    }

    /// The ground type (`campaign_ground_types` key) at (x, z): the first type whose areas contain
    /// it (the types do not overlap).
    pub fn ground_type_at(&self, x: f32, z: f32) -> Option<&str> {
        self.ground_types.iter().find(|g| g.contains(x, z)).map(|g| g.key.as_str())
    }

    /// The region (land or sea) whose triangles contain the logic point (x, z), if any.
    pub fn region_at(&self, x: f32, z: f32) -> Option<&MapRegion> {
        let inside = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
            let s = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (z - p.1) - (q.1 - p.1) * (x - p.0);
            let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
            !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
        };
        self.regions.iter().find(|r| {
            x >= r.min.0 && x <= r.max.0 && z >= r.min.1 && z <= r.max.1 && r.areas.iter().any(|a| {
                a.faces.as_chunks::<3>().0.iter().any(|t| {
                    let v = |i: u32| self.vertices.get(i as usize).copied().unwrap_or_default();
                    inside(v(t[0]), v(t[1]), v(t[2]))
                })
            })
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Trees
// ---------------------------------------------------------------------------------------------

/// One tree instance: logic position and uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeInstance {
    /// Position (logic x, height, logic z).
    pub position: [f32; 3],
    /// Uniform scale of the model (display units → logic units: about 35..38.5, i.e. 39.37 × 0.9..0.98).
    pub scale: f32,
}

/// One group of instances of a model with its bounding box (logic units; `-1..1` for empty groups).
#[derive(Debug, Clone, PartialEq)]
pub struct TreeGroup {
    /// Box minimum.
    pub min: [f32; 3],
    /// Box maximum.
    pub max: [f32; 3],
    /// The instances.
    pub instances: Vec<TreeInstance>,
}

/// One tree model block: its model path and its instance groups.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeModel {
    /// Model path, e.g. `RigidModels/CampaignTrees/Campaign_tree_coniferous01.rigid_model`. The exe
    /// also loads the `_low.rigid_model` variant beside it when it exists.
    pub path: String,
    /// The instance groups.
    pub groups: Vec<TreeGroup>,
}

/// `display\trees\campaign.rigid_trees` (CONFIRMED by the exe's reader `0x00966500`, which every
/// shipped file matches to the byte):
/// ```text
/// u32 "G@M=" | u32 1 | u32 (1024.0) | u32 (6) | u32 model_count
/// model_count × { u16 n; n × UTF-16 path; u32 group_count;
///                  group_count × { u32 instance_count; vec3 min; vec3 max;
///                                  instance_count × { vec3 position; f32 scale } } }
/// ```
/// The two 4-byte header fields are read and discarded. Each instance goes to `0x00914A30` with its
/// position and the scale, which multiplies the model's bounding box (so it is a uniform scale from
/// the model's display units). The instance also gets a random value from the CRT `rand()`
/// (`0x0126E2B0`), INFERRED to be its rotation about the vertical.
#[derive(Debug, Clone, PartialEq)]
pub struct RigidTrees {
    /// The model blocks in file order.
    pub models: Vec<TreeModel>,
}

impl RigidTrees {
    /// Parses the file.
    pub fn read(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let mut c = Cursor::new(bytes);
        let t = trunc("rigid_trees");
        if c.u32().map_err(&t)? != 0x3D4D_4047 {
            return Err(CampaignMapError::BadMagic { expected: "G@M=" });
        }
        if c.u32().map_err(&t)? != 1 {
            return Err(CampaignMapError::BadMagic { expected: "rigid_trees version 1" });
        }
        c.u32().map_err(&t)?;
        c.u32().map_err(&t)?;
        let count = c.u32().map_err(&t)?;
        let mut models = Vec::with_capacity((count as usize).min(c.remaining() / 6));
        for _ in 0..count {
            let path = c.utf16().map_err(&t)?;
            let groups_n = c.u32().map_err(&t)?;
            let mut groups = Vec::new();
            for _ in 0..groups_n {
                let n = c.u32().map_err(&t)? as usize;
                let mut v3 = || -> Result<[f32; 3], CampaignMapError> { Ok([c.f32().map_err(&t)?, c.f32().map_err(&t)?, c.f32().map_err(&t)?]) };
                let (min, max) = (v3()?, v3()?);
                if n.saturating_mul(16) > c.remaining() {
                    return Err(CampaignMapError::Truncated { what: "tree instances" });
                }
                let mut instances = Vec::with_capacity(n);
                for _ in 0..n {
                    let position = [c.f32().map_err(&t)?, c.f32().map_err(&t)?, c.f32().map_err(&t)?];
                    instances.push(TreeInstance { position, scale: c.f32().map_err(&t)? });
                }
                groups.push(TreeGroup { min, max, instances });
            }
            models.push(TreeModel { path, groups });
        }
        if c.remaining() != 0 {
            return Err(CampaignMapError::TrailingBytes { count: c.remaining() });
        }
        Ok(Self { models })
    }
}

// ---------------------------------------------------------------------------------------------
// Coastline
// ---------------------------------------------------------------------------------------------

/// One coastline vertex: position (logic units) and the two texture coordinates the coast shader
/// reads (`fx\campaignterrain.fx` `VS_INPUT_COAST`: `tex` and `tex2`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoastVertex {
    /// Position (logic x, y, z; y is 0 on the shipped files and the shader puts it at 0.1).
    pub position: [f32; 3],
    /// `TEXCOORD0`.
    pub tex: [f32; 2],
    /// `TEXCOORD1`.
    pub tex2: [f32; 2],
}

/// `display\coastline\coastline_group<n>.rigid_mesh` (read by `0x011C51B0`; the scene loader
/// `0x011C73A0` loads groups 0, 1, … until one is missing). Layout, CONFIRMED against the shipped
/// files (they parse to the byte):
/// ```text
/// u32 0x12345678 | u32 5 | 19 descriptor bytes (vertex format, read by 0x011B8520) | u32 vertex_count
/// vertex_count × 80 bytes: f32 x, y, z @0, f32 tex u, v @24, f32 tex2 u, v @72 (the rest unused here)
/// u32 index_count | index_count × u32
/// ```
/// The 80-byte stride is the vertex reader `0x011B2AB0`'s 0x38 bytes plus its 0x10 and 8 optional
/// parts; the descriptor is not decoded (the files all use the same 19 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct CoastMesh {
    /// The vertices.
    pub vertices: Vec<CoastVertex>,
    /// Triangle list indices.
    pub indices: Vec<u32>,
}

impl CoastMesh {
    /// Parses the file.
    pub fn read(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let mut c = Cursor::new(bytes);
        let t = trunc("coastline mesh");
        if c.u32().map_err(&t)? != 0x1234_5678 {
            return Err(CampaignMapError::BadMagic { expected: "0x12345678" });
        }
        if c.u32().map_err(&t)? != 5 {
            return Err(CampaignMapError::BadMagic { expected: "rigid_mesh version 5" });
        }
        c.take(19).map_err(&t)?;
        let n = c.u32().map_err(&t)? as usize;
        if n.saturating_mul(80) > c.remaining() {
            return Err(CampaignMapError::Truncated { what: "coastline vertices" });
        }
        let mut vertices = Vec::with_capacity(n);
        for _ in 0..n {
            let r = c.take(80).map_err(&t)?;
            let f = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
            vertices.push(CoastVertex { position: [f(0), f(4), f(8)], tex: [f(24), f(28)], tex2: [f(72), f(76)] });
        }
        let m = c.u32().map_err(&t)? as usize;
        if m.saturating_mul(4) != c.remaining() {
            return Err(CampaignMapError::Truncated { what: "coastline indices" });
        }
        let mut indices = Vec::with_capacity(m);
        for _ in 0..m {
            indices.push(c.u32().map_err(&t)?);
        }
        if indices.iter().any(|&i| i as usize >= n) {
            return Err(CampaignMapError::Inner("coastline index out of range".into()));
        }
        Ok(Self { vertices, indices })
    }
}

// ---------------------------------------------------------------------------------------------
// Border ribbon
// ---------------------------------------------------------------------------------------------

/// One border-ribbon vertex, read from `testdata\westerneuborders.rigid_mesh`. The record is the
/// standard 56-byte version-0 rigid-model vertex (see [`crate::rigid_model::vertex_size`]), and this
/// file fills it as:
///
/// ```text
/// u16 flags | u32 vertex_count | count × 56-byte vertex | u32 index_count | index_count × u32
/// ```
///
/// CONFIRMED against the shipped file to the byte: `6 + 586×56 + 4 + 1758×4 = 39858`, the file's
/// exact length, the stored `vertex_count` is 586 and the stored `index_count` is 1758 -- and
/// 1758 = 3 × 586 = three indices per vertex, which is the six-per-segment unrolled list a two-vertex
/// ribbon needs (293 segments).
///
/// **What the file does NOT have, contrary to the round-3 note** (CAMPAIGN_MAP.md 10.3a-1, which
/// said "with UVs"): CONFIRMED by reading all 586 records -- `y` is 0.0, the normal is exactly
/// `(0, 1, 0)` and **both** texture coordinates are exactly `0.0` on every vertex, as are the
/// tangent and the binormal. So the ribbon carries its cross-section in geometry only. The `v`
/// mapping of `rigidmodels\campaignborders\textures\border_diffuse.dds` is therefore produced by the
/// ribbon *builder* at draw time, not stored here -- and the positions are not campaign map display
/// units either (they span about 5.3 × 4.0 units around `z ≈ 21.6`, not a map-sized range), so this
/// file cannot be dropped into the map scene as it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct BorderRibbon {
    /// The `u16` after nothing else; `0` in the shipped file. UNKNOWN.
    pub flags: u16,
    /// The vertices, in file order.
    pub vertices: Vec<CoastVertex>,
    /// Triangle list indices. The shipped file's list starts `0, 1, 2, 0, 3, 1, 0, 4, 3, 4, 5, 3`,
    /// so it is **not** a strip of (2k, 2k+1) pairs -- vertex 0 is the apex of a fan -- which is why
    /// no single constant width can be read off it.
    pub indices: Vec<u32>,
}

impl BorderRibbon {
    /// Parses the file. CONFIRMED: it must end exactly after the last index.
    pub fn read(bytes: &[u8]) -> Result<Self, CampaignMapError> {
        let mut c = Cursor::new(bytes);
        let t = trunc("border ribbon");
        let flags = c.u16().map_err(&t)?;
        let n = c.u32().map_err(&t)? as usize;
        if n.saturating_mul(56) > c.remaining() {
            return Err(CampaignMapError::Truncated { what: "border ribbon vertices" });
        }
        let mut vertices = Vec::with_capacity(n);
        for _ in 0..n {
            let r = c.take(56).map_err(&t)?;
            let f = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
            vertices.push(CoastVertex { position: [f(0), f(4), f(8)], tex: [f(24), f(28)], tex2: [0.0, 0.0] });
        }
        let m = c.u32().map_err(&t)? as usize;
        if m.saturating_mul(4) != c.remaining() {
            return Err(CampaignMapError::Truncated { what: "border ribbon indices" });
        }
        let mut indices = Vec::with_capacity(m);
        for _ in 0..m {
            indices.push(c.u32().map_err(&t)?);
        }
        if indices.iter().any(|&i| i as usize >= n) {
            return Err(CampaignMapError::Inner("border ribbon index out of range".into()));
        }
        Ok(Self { flags, vertices, indices })
    }

    /// CONFIRMED on the shipped file: every vertex has `y == 0` and both texture coordinates
    /// exactly `0.0` (and, read off the raw record, the normal is exactly `(0, 1, 0)` with a zero
    /// tangent and binormal). True means the file carries no UVs at all.
    pub fn is_geometry_only(&self) -> bool {
        self.vertices.iter().all(|v| v.position[1] == 0.0 && v.tex == [0.0, 0.0] && v.tex2 == [0.0, 0.0])
    }
}

// ---------------------------------------------------------------------------------------------
// Whole map
// ---------------------------------------------------------------------------------------------

/// Logic height units per heightmap step: 5/255 (value 255 = 5.0 units). INFERRED (strong) from the
/// tree list: the stored tree heights are exact multiples of 1/51 = 5/255 where a tree stands on a
/// pixel centre (0.0196, 0.137, 0.2745 …), and a least-squares fit of tree height against the
/// heightmap value under it gives 0.0191 (Europe, 8,903 trees) and 0.0187 (Italy). It also matches
/// the highest spline point (0.13 display units = 5.1 logic units).
pub const HEIGHT_SCALE: f32 = 5.0 / 255.0;

/// Everything the campaign view draws from one `campaign_maps\<map>\` folder.
#[derive(Debug, Clone)]
pub struct CampaignMap {
    /// The folder name, e.g. `nap_europe`.
    pub name: String,
    /// `regions.esf`.
    pub regions: RegionMap,
    /// `display\heightmap\heightmap.tga` (covers `regions.bounds_*`, north row first; CONFIRMED
    /// by sea regions sampling ~0 and land ~5.6 on average with that mapping, and not the other).
    pub heightmap: Heightmap,
    /// The supertexture index (the pixels are decoded on request, see [`CampaignMap::supertexture_rgba`]).
    pub supertexture: Option<SuperTexture>,
    /// Splines by folder: `borders`, `rivers`, `roads`, `traderoutes`.
    pub splines: Vec<(String, Spline)>,
    /// `pathfinding.esf` (decrypted when needed), if present and readable.
    pub pathfinding: Option<crate::campaign_pathfinding::PathfindingFile>,
    /// `sea_grids.esf` (the campaign AI's sea zones), if present and readable.
    pub sea_grid: Option<crate::campaign_pathfinding::SeaGrid>,
    /// `display\trees\campaign.rigid_trees`, if present and readable.
    pub trees: Option<RigidTrees>,
    /// `display\coastline\coastline_group<n>.rigid_mesh` for n = 0, 1, … while present and readable.
    pub coast: Vec<CoastMesh>,
}

/// Where campaign files are read from: the install's [`Vfs`](crate::pack::Vfs), which also
/// mounts the loose files under `data\` (the campaign maps and start positions ship as loose
/// files, `data\campaign_maps\`, `data\campaigns\`; the original reads loose files through its
/// VFS, "non_pack") and the mods, so a mod can replace a map file.
#[derive(Clone, Copy)]
pub struct GameFiles<'a> {
    /// The install's files (packs, loose `data\` files and mods).
    pub vfs: &'a crate::pack::Vfs,
}

/// A game file's bytes on disk: `len` bytes at `offset` in `file` (a pack, or the loose file at 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpan {
    /// The file holding the bytes.
    pub file: std::path::PathBuf,
    /// Where they start in it.
    pub offset: u64,
    /// How many there are.
    pub len: u64,
}

impl FileSpan {
    /// Reads `len` bytes starting `start` bytes into the span from `file` (this span's file, open);
    /// an error past the span's end.
    pub fn read_at(&self, file: &mut std::fs::File, start: u64, len: usize) -> Result<Vec<u8>, String> {
        use std::io::{Read, Seek, SeekFrom};
        if start.checked_add(len as u64).is_none_or(|end| end > self.len) {
            return Err(format!("{}: bytes {start}..+{len} past its end ({})", self.file.display(), self.len));
        }
        file.seek(SeekFrom::Start(self.offset + start)).map_err(|e| format!("{}: {e}", self.file.display()))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).map_err(|e| format!("{}: {e}", self.file.display()))?;
        Ok(buf)
    }
}

impl GameFiles<'_> {
    /// Reads a game path (case-insensitive, `/` or `\`), read-only.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        self.vfs.read(path).map_err(|e| format!("{path}: {e}"))
    }

    /// Where the winning copy of a game path lies on disk (the copy [`read`](Self::read) gives): a
    /// pack's file at its entry, or the loose file. For reading parts of a large file (the
    /// supertexture's tiles) by range without reading all of it, and without holding the Vfs.
    pub fn locate(&self, path: &str) -> Result<FileSpan, String> {
        if let Some((pack, entry)) = self.vfs.find(path) {
            return Ok(FileSpan { file: pack.path().to_owned(), offset: entry.offset, len: u64::from(entry.size) });
        }
        let file = self.vfs.loose_path(path).ok_or_else(|| format!("{path}: not found"))?;
        let len = self.vfs.file_size(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(FileSpan { file, offset: 0, len })
    }

    /// All files whose normalized path starts with `prefix` (a folder ending in `/` or `\`),
    /// normalized and sorted.
    pub fn list(&self, prefix: &str) -> Vec<String> {
        self.vfs.list(prefix).into_iter().map(str::to_owned).collect()
    }
}

/// An optional map file: `None` when it is not there; when it is there but does not parse, the
/// error is logged and the part left out.
fn optional<T, E: fmt::Display>(files: &GameFiles<'_>, path: &str, parse: impl FnOnce(&[u8]) -> Result<T, E>) -> Option<T> {
    let bytes = files.read(path).ok()?;
    parse(&bytes).map_err(|e| log::warn!("Campaign map {path}: {e}; left out")).ok()
}

impl CampaignMap {
    /// Loads a map folder (`map` = `nap_europe`, or the full `campaign_maps/nap_europe`).
    /// Missing optional parts (supertexture, a spline folder) are skipped; an optional part that
    /// is there but does not parse is logged (once per load) and left out.
    pub fn load(files: &GameFiles<'_>, map: &str) -> Result<Self, CampaignMapError> {
        let name = map.trim_start_matches("campaign_maps/").trim_start_matches("campaign_maps\\").to_owned();
        let base = format!("campaign_maps/{name}");
        let read = |p: &str| files.read(p).map_err(CampaignMapError::Inner);
        let regions = RegionMap::read(&read(&format!("{base}/regions.esf"))?)?;
        let heightmap = Heightmap::read(&read(&format!("{base}/display/heightmap/heightmap.tga"))?)?;
        let supertexture = optional(files, &format!("{base}/display/supertexture/supertexture.stpi"), SuperTexture::read_index);
        let mut splines = Vec::new();
        for folder in ["borders", "rivers", "roads", "traderoutes"] {
            let prefix = crate::pack::normalize_path(&format!("{base}/display/{folder}/"));
            for path in files.list(&prefix) {
                // Only files directly in the folder (roads\tut_napoleon\ belongs to the tutorial).
                if !path.ends_with(".rigid_spline") || path[prefix.len()..].contains('\\') {
                    continue;
                }
                match SplineFile::read(&read(&path)?) {
                    Ok(f) => splines.extend(f.splines.into_iter().map(|s| (folder.to_owned(), s))),
                    Err(e) => log::warn!("Campaign map {path}: {e}; left out"),
                }
            }
        }
        let pathfinding = optional(files, &format!("{base}/pathfinding.esf"), crate::campaign_pathfinding::PathfindingFile::read);
        let sea_grid = optional(files, &format!("{base}/sea_grids.esf"), crate::campaign_pathfinding::SeaGrid::read);
        let trees = optional(files, &format!("{base}/display/trees/campaign.rigid_trees"), RigidTrees::read);
        // `coastline_group<n>` for n = 0, 1, … up to the first one that is not there.
        let mut coast = Vec::new();
        for i in 0.. {
            let path = format!("{base}/display/coastline/coastline_group{i}.rigid_mesh");
            let Ok(bytes) = files.read(&path) else { break };
            match CoastMesh::read(&bytes) {
                Ok(m) => coast.push(m),
                Err(e) => log::warn!("Campaign map {path}: {e}; left out"),
            }
        }
        Ok(Self { name, regions, heightmap, supertexture, splines, pathfinding, sea_grid, trees, coast })
    }

    /// Terrain height (logic units) at logic (x, z), bilinear.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let (mn, mx) = (self.regions.bounds_min, self.regions.bounds_max);
        let u = (x - mn.0) / (mx.0 - mn.0) * self.heightmap.width as f32;
        let v = (mx.1 - z) / (mx.1 - mn.1) * self.heightmap.height as f32;
        self.heightmap.sample(u, v) * HEIGHT_SCALE
    }

    /// Decodes one supertexture level (0 = finest) into one RGBA8 image, north row first,
    /// covering the map bounds. Returns `(width, height, pixels)`.
    pub fn supertexture_rgba(&self, files: &GameFiles<'_>, level: usize) -> Result<(u32, u32, Vec<u8>), CampaignMapError> {
        let st = self.supertexture.as_ref().ok_or(CampaignMapError::Missing("supertexture"))?;
        let lv = st.levels.get(level).ok_or(CampaignMapError::Missing("supertexture level"))?;
        let stpd = files.read(&format!("campaign_maps/{}/display/supertexture/supertexture.stpd", self.name)).map_err(CampaignMapError::Inner)?;
        let out = st.window_rgba(level, (0, 0, lv.tiles_x, lv.tiles_y), |tile| {
            let start = tile.offset as usize;
            stpd.get(start..start + tile.size as usize).map(<[u8]>::to_vec).ok_or(CampaignMapError::Truncated { what: "stpd tile" })
        })?;
        Ok((lv.tiles_x * st.tile_size, lv.tiles_y * st.tile_size, out))
    }
}

/// Converts a display-unit position (splines, models) to logic map units.
pub fn display_to_logic(v: f32) -> f32 {
    v * DISPLAY_TO_LOGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spln(splines: &[(&str, &[[f32; 3]])]) -> Vec<u8> {
        let mut b = b"SPLN".to_vec();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(splines.len() as u32).to_le_bytes());
        for (name, pts) in splines {
            let u: Vec<u16> = name.encode_utf16().collect();
            b.extend_from_slice(&(u.len() as u16).to_le_bytes());
            u.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&(pts.len() as u32).to_le_bytes());
            for p in *pts {
                p.iter().for_each(|f| b.extend_from_slice(&f.to_le_bytes()));
            }
        }
        b
    }

    #[test]
    fn reads_splines() {
        let b = spln(&[("border:eur_x:1", &[[1.0, 0.0, 2.0], [3.0, 0.5, 4.0]]), ("River:r", &[])]);
        let f = SplineFile::read(&b).unwrap();
        assert_eq!(f.splines.len(), 2);
        assert_eq!(f.splines[0].kind(), "border");
        assert_eq!(f.splines[1].kind(), "river");
        assert_eq!(f.splines[0].points[1], [3.0, 0.5, 4.0]);
        for len in 0..b.len() {
            assert!(SplineFile::read(&b[..len]).is_err());
        }
        let mut extra = b.clone();
        extra.push(0);
        assert!(SplineFile::read(&extra).is_err());
    }

    #[test]
    fn reads_rigid_trees() {
        let mut b = Vec::new();
        for v in [0x3D4D_4047u32, 1, 1024f32.to_bits(), 6, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let model = |b: &mut Vec<u8>, name: &str, inst: &[[f32; 4]]| {
            let u: Vec<u16> = name.encode_utf16().collect();
            b.extend_from_slice(&(u.len() as u16).to_le_bytes());
            u.iter().for_each(|x| b.extend_from_slice(&x.to_le_bytes()));
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&(inst.len() as u32).to_le_bytes());
            for f in [-1f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
                b.extend_from_slice(&f.to_le_bytes());
            }
            for i in inst {
                i.iter().for_each(|f| b.extend_from_slice(&f.to_le_bytes()));
            }
        };
        model(&mut b, "RigidModels/CampaignTrees/a.rigid_model", &[[1.0, 0.5, 2.0, 36.0]]);
        model(&mut b, "RigidModels/CampaignTrees/b.rigid_model", &[]);
        let t = RigidTrees::read(&b).unwrap();
        assert_eq!(t.models.len(), 2);
        assert_eq!(t.models[0].path, "RigidModels/CampaignTrees/a.rigid_model");
        assert_eq!(t.models[0].groups[0].instances[0], TreeInstance { position: [1.0, 0.5, 2.0], scale: 36.0 });
        assert!(t.models[1].groups[0].instances.is_empty());
        for len in 0..b.len() {
            assert!(RigidTrees::read(&b[..len]).is_err());
        }
    }

    #[test]
    fn reads_stpi_and_inflates() {
        // One level, one tile of 16 raw bytes in two zlib chunks.
        let raw = [7u8; 8];
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let mut stpd = Vec::new();
        for _ in 0..2 {
            stpd.extend_from_slice(&(z.len() as u32).to_le_bytes());
            stpd.extend_from_slice(&8u32.to_le_bytes());
            stpd.extend_from_slice(&z);
        }
        let mut stpi = Vec::new();
        for v in [1u32, 4, 4, 4, 16, 1, 1, 1, 0, stpd.len() as u32, 16, 0, 0, 1] {
            stpi.extend_from_slice(&v.to_le_bytes());
        }
        let st = SuperTexture::read_index(&stpi).unwrap();
        assert_eq!((st.width, st.levels.len(), st.levels[0].tiles.len()), (4, 1, 1));
        let tile = SuperTexture::inflate_tile(&stpd, &st.levels[0].tiles[0], 0).unwrap();
        assert_eq!(tile, [7u8; 16]);
        assert!(SuperTexture::read_index(&stpi[..stpi.len() - 1]).is_err());
    }

    /// `locate` finds a file where `read` would (in the winning pack at its entry, else loose); a
    /// range read gives the same bytes as `read` and refuses to read past the file.
    #[test]
    fn a_located_file_reads_like_read() {
        use crate::pack::test_util::{build_pack, temp_dir};
        let dir = temp_dir("campaign_map_locate");
        std::fs::write(dir.join("a.pack"), build_pack(1, &[("x\\first", b"0123"), ("campaign_maps\\m\\big.bin", b"hello world")])).unwrap();
        std::fs::create_dir_all(dir.join("campaign_maps").join("m")).unwrap();
        std::fs::write(dir.join("campaign_maps").join("m").join("loose.bin"), b"loose bytes").unwrap();
        let mut vfs = crate::pack::Vfs::from_packs(vec![crate::pack::PackFile::open(dir.join("a.pack")).unwrap()]);
        vfs.mount_dir(&dir, crate::pack::LayerKind::Loose).unwrap();
        let files = GameFiles { vfs: &vfs };
        for path in ["campaign_maps/m/big.bin", "campaign_maps/m/loose.bin"] {
            let span = files.locate(path).unwrap();
            let whole = files.read(path).unwrap();
            assert_eq!(span.len, whole.len() as u64);
            let mut f = std::fs::File::open(&span.file).unwrap();
            assert_eq!(span.read_at(&mut f, 0, whole.len()).unwrap(), whole);
            assert_eq!(span.read_at(&mut f, 6, 5).unwrap(), &whole[6..11]);
            assert!(span.read_at(&mut f, 6, 6).is_err(), "past the end of {path}");
        }
        assert!(files.locate("campaign_maps/m/big.bin").unwrap().offset > 0, "inside the pack");
        assert!(files.locate("campaign_maps/m/missing.bin").is_err());
    }
}

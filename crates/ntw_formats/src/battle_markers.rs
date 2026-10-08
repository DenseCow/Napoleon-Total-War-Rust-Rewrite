//! Battle-map marker files: `battleterrain\presets\<map>\bmd.markers` and the tiles' `bmd.markers`
//! (56 files). Spec: `analysis/campaign/CAMPAIGN_DATA.md` §6 (from the files and the exe's
//! writer `FUN_00f9fde0`). Tags: CONFIRMED / INFERRED / UNKNOWN.
//!
//! # Layout (CONFIRMED: every shipped file reads to its last byte)
//! ```text
//! u16 n, n x UTF-16          "BASE_MARKER_REPOSITORY"
//! key16                      12 zero bytes + u32 id (0x0009D9BB)
//! u32 1                      version
//! u16 group count            6 (or 5)
//! per group:
//!   key16                    12 zero bytes + u32 class id
//!   u16 n, n x UTF-16        TREE_MARKER / BUILDING_MARKER / PROP_MARKER (x3) / DECAL_MARKER
//!   key16, u32 1             the class id again, version
//!   u32 item count
//!   if count > 0:
//!     count x item           by class, below
//!     u16 model groups
//!     per model group: u16 1, u16 n, n x UTF-16 model key, u32 k, k x u32 item index
//! ```
//! Item classes (by the class id):
//! - 0x9D9BE (second group, `PROP_MARKER`): a placed prop {f32 x, f32 y, u32 angle (UNKNOWN unit,
//!   values < 0xA000), f32 scale (0.36..1.0)}; the model groups name the props
//!   (`small_rock_4_piece01_destruct01`, ...) and list which items use them. Only the three
//!   `nap_mp_*` maps have them (1,532 each).
//! - 0x9D9C0 (third `PROP_MARKER`): a polygon {u32 n, n x (f32 x, f32 y)}, usually closed (97 of 119
//!   repeat the first point); the model group key is a single NUL character. INFERRED: areas kept clear
//!   of generated props (they sit on the tiles' flat ground).
//! - The other classes are empty in every shipped file (their item layout is UNKNOWN).
//!
//! Coordinates are battle-map metres in the map's own frame (INFERRED: same frame as the
//! building and tree lists).

use crate::bytes::{Cursor as Reader, ReadError};

/// The class id of placed-prop groups.
pub const CLASS_PROPS: u32 = 0x0009_D9BE;
/// The class id of polygon groups.
pub const CLASS_POLYGONS: u32 = 0x0009_D9C0;

/// A placed prop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropMarker {
    /// x in metres.
    pub x: f32,
    /// y (map north) in metres.
    pub y: f32,
    /// UNKNOWN u32 (INFERRED: an angle or a random seed).
    pub angle: u32,
    /// Scale (0.36..1.0 in the files).
    pub scale: f32,
}

/// A group's items.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkerItems {
    /// No items.
    Empty,
    /// Placed props.
    Props(Vec<PropMarker>),
    /// Closed polygons.
    Polygons(Vec<Vec<(f32, f32)>>),
}

impl MarkerItems {
    /// The number of items.
    pub fn len(&self) -> usize {
        match self {
            MarkerItems::Empty => 0,
            MarkerItems::Props(v) => v.len(),
            MarkerItems::Polygons(v) => v.len(),
        }
    }
    /// True when there are no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A model key and the items that use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelGroup {
    /// The u16 before the name (1 in every file; UNKNOWN).
    pub flag: u16,
    /// Model key (a `battlefield_buildings`-style key) or "\0" for polygon groups.
    pub model: String,
    /// Indices into the group's items.
    pub items: Vec<u32>,
}

/// One marker group.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerGroup {
    /// The class id (the u32 of the group's key).
    pub class: u32,
    /// The group name, e.g. `PROP_MARKER`.
    pub name: String,
    /// The version u32 (1).
    pub version: u32,
    /// The items.
    pub items: MarkerItems,
    /// The model groups (only present when there are items).
    pub models: Vec<ModelGroup>,
}

/// A whole `.markers` file.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerRepository {
    /// `BASE_MARKER_REPOSITORY`.
    pub name: String,
    /// The repository's id (the u32 of its key).
    pub id: u32,
    /// Version (1).
    pub version: u32,
    /// The groups in file order.
    pub groups: Vec<MarkerGroup>,
}

fn string(r: &mut Reader<'_>) -> Result<String, String> {
    let n = r.u16().map_err(|e| format!("{e:?}"))? as usize;
    let mut units = Vec::with_capacity(n);
    for _ in 0..n {
        units.push(r.u16().map_err(|e| format!("{e:?}"))?);
    }
    Ok(String::from_utf16_lossy(&units))
}

fn key(r: &mut Reader<'_>) -> Result<u32, String> {
    let zeros = r.take(12).map_err(|e| format!("{e:?}"))?;
    if zeros.iter().any(|&b| b != 0) {
        return Err(format!("key at {} does not start with 12 zero bytes", r.pos() - 12));
    }
    r.u32().map_err(|e| format!("{e:?}"))
}

impl MarkerRepository {
    /// Parses a `.markers` file; fails on anything unexpected (including trailing bytes).
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader::new(bytes);
        let e = |e: ReadError| format!("{e:?}");
        let name = string(&mut r)?;
        let id = key(&mut r)?;
        let version = r.u32().map_err(e)?;
        let count = r.u16().map_err(e)?;
        let mut groups = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let class = key(&mut r)?;
            let name = string(&mut r)?;
            let inner = key(&mut r)?;
            if inner != class {
                return Err(format!("group {name}: inner key {inner:x} differs from {class:x}"));
            }
            let version = r.u32().map_err(e)?;
            let n = r.u32().map_err(e)? as usize;
            if n > bytes.len() {
                return Err(format!("group {name}: {n} items"));
            }
            let mut models = Vec::new();
            let items = if n == 0 {
                MarkerItems::Empty
            } else {
                let items = match class {
                    CLASS_PROPS => {
                        let mut v = Vec::with_capacity(n);
                        for _ in 0..n {
                            v.push(PropMarker {
                                x: r.f32().map_err(e)?,
                                y: r.f32().map_err(e)?,
                                angle: r.u32().map_err(e)?,
                                scale: r.f32().map_err(e)?,
                            });
                        }
                        MarkerItems::Props(v)
                    }
                    CLASS_POLYGONS => {
                        let mut v = Vec::with_capacity(n);
                        for _ in 0..n {
                            let k = r.u32().map_err(e)? as usize;
                            if k > bytes.len() / 8 {
                                return Err(format!("polygon of {k} points"));
                            }
                            let mut p = Vec::with_capacity(k);
                            for _ in 0..k {
                                p.push((r.f32().map_err(e)?, r.f32().map_err(e)?));
                            }
                            v.push(p);
                        }
                        MarkerItems::Polygons(v)
                    }
                    other => return Err(format!("group {name}: items of UNKNOWN class {other:x}")),
                };
                let groups = r.u16().map_err(e)?;
                for _ in 0..groups {
                    let flag = r.u16().map_err(e)?;
                    let model = string(&mut r)?;
                    let k = r.u32().map_err(e)? as usize;
                    if k > bytes.len() / 4 {
                        return Err(format!("model group of {k} items"));
                    }
                    let items = (0..k).map(|_| r.u32()).collect::<Result<_, _>>().map_err(e)?;
                    models.push(ModelGroup { flag, model, items });
                }
                items
            };
            groups.push(MarkerGroup { class, name, version, items, models });
        }
        if r.remaining() != 0 {
            return Err(format!("{} bytes after the last group", r.remaining()));
        }
        Ok(Self { name, id, version, groups })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str, out: &mut Vec<u8>) {
        out.extend_from_slice(&(s.len() as u16).to_le_bytes());
        for c in s.encode_utf16() {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    fn key(id: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(&id.to_le_bytes());
    }

    #[test]
    fn reads_a_polygon_group() {
        let mut b = Vec::new();
        utf16("BASE_MARKER_REPOSITORY", &mut b);
        key(0x9d9bb, &mut b);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        key(0x9d9bc, &mut b);
        utf16("TREE_MARKER", &mut b);
        key(0x9d9bc, &mut b);
        b.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        key(CLASS_POLYGONS, &mut b);
        utf16("PROP_MARKER", &mut b);
        key(CLASS_POLYGONS, &mut b);
        b.extend_from_slice(&[1, 0, 0, 0, 1, 0, 0, 0]);
        b.extend_from_slice(&3u32.to_le_bytes());
        for v in [0.0f32, 0.0, 1.0, 0.0, 0.0, 0.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[1, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        let m = MarkerRepository::read(&b).unwrap();
        assert_eq!(m.groups.len(), 2);
        assert_eq!(m.groups[1].items, MarkerItems::Polygons(vec![vec![(0.0, 0.0), (1.0, 0.0), (0.0, 0.0)]]));
        assert_eq!(m.groups[1].models, vec![ModelGroup { flag: 1, model: "\0".into(), items: vec![0] }]);
        b.push(0);
        assert!(MarkerRepository::read(&b).is_err());
    }
}

/// `rigidmodels\campaignbridges\bridge.markers`: a different marker format, a plain list of
/// {u16 n, n x UTF-16 name (`bridge:france:1`), 16 x f32 transform} to the end of the file
/// (CONFIRMED: the file splits exactly). The matrix is row-major with the translation in the
/// last row (INFERRED from the 0/1 pattern: rows 0..2 a rotation, row 3 = x, y, z, 1).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedTransforms(pub Vec<(String, [f32; 16])>);

impl NamedTransforms {
    /// Parses the whole file.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader::new(bytes);
        let mut out = Vec::new();
        while r.remaining() > 0 {
            let name = string(&mut r)?;
            let mut m = [0f32; 16];
            for v in &mut m {
                *v = r.f32().map_err(|e| format!("{e:?}"))?;
            }
            out.push((name, m));
        }
        Ok(Self(out))
    }
}

/// `battleterrain\farm_templates\<kind>_map_templates\*.farm_fields_tile_texture` (131 files): the
/// texture tiles used to paint farms into generated battlefields (`AUTO_GENERATOR` strings in the
/// exe name the `_grass_`, `_blend_`, `_colour_` x `_road`, `_building`, `_field` variants).
///
/// Layout (CONFIRMED: every file's offsets end exactly at its length):
/// `u8 kind, u32 n, (n + 1) x u32 offsets` (absolute; the last = the file length), then n chunks.
/// - kind 1 (blend and colour maps): a chunk is `u32 a, u32 b`, then two JPEG files of `a` and `b`
///   bytes (CONFIRMED: both start with FF D8). INFERRED: colour and alpha of one tile.
/// - kind 0 (20 of the large grass maps): every chunk is a **TGA file**, uncompressed true colour
///   (image type 2, 32 bits, 8 alpha bits; CONFIRMED: the 18-byte header's width x height x 4 + 18 is
///   the chunk size, except that a few chunks carry trailing zero bytes, e.g. 4,096 after a 166 x 764
///   image). Decode with [`FarmTileTexture::tga`]. The 20 small fort grass maps are kind 1 (JPEG pairs).
///   Pixel meaning: like the presets' `grassmap.tga` (B, G, A channels used); the channel semantics
///   are UNKNOWN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarmTileTexture {
    /// The first byte (0 grass, 1 blend/colour).
    pub kind: u8,
    /// The chunks' raw bytes, in order.
    pub chunks: Vec<Vec<u8>>,
}

impl FarmTileTexture {
    /// Parses a file.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader::new(bytes);
        let e = |e: ReadError| format!("{e:?}");
        let kind = r.u8().map_err(e)?;
        let n = r.u32().map_err(e)? as usize;
        if n > bytes.len() / 4 {
            return Err(format!("{n} chunks"));
        }
        let offsets: Vec<usize> = (0..=n).map(|_| r.u32().map(|v| v as usize)).collect::<Result<_, _>>().map_err(e)?;
        if offsets.first() != Some(&r.pos()) || offsets.last() != Some(&bytes.len()) || offsets.windows(2).any(|w| w[0] > w[1]) {
            return Err("chunk offsets do not cover the file".into());
        }
        Ok(Self { kind, chunks: offsets.windows(2).map(|w| bytes[w[0]..w[1]].to_vec()).collect() })
    }

    /// For a kind-0 chunk: the TGA image it holds (trailing bytes after the pixels are ignored).
    pub fn tga(chunk: &[u8]) -> Option<crate::tga::Tga> {
        crate::tga::Tga::decode(chunk).ok()
    }

    /// For a kind-1 chunk: its two JPEG files (colour, then INFERRED alpha).
    pub fn jpegs(chunk: &[u8]) -> Option<(&[u8], &[u8])> {
        let a = u32::from_le_bytes(chunk.get(0..4)?.try_into().ok()?) as usize;
        let b = u32::from_le_bytes(chunk.get(4..8)?.try_into().ok()?) as usize;
        if 8 + a + b != chunk.len() {
            return None;
        }
        Some((&chunk[8..8 + a], &chunk[8 + a..]))
    }
}

/// One placed object of a `.prop_list` or a farm template.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedObject {
    /// Model key, e.g. `rock_3_dry_piece01_destruct01`.
    pub key: String,
    /// (x, y) in metres.
    pub position: (f32, f32),
    /// ESF angle (u16, 65536 = one turn, INFERRED).
    pub angle: u16,
    /// Scale: one f32 in `.prop_list`; (x, y) in farm templates.
    pub scale: (f32, f32),
}

/// `bmd.prop_list` (root `BATTLE_PROP_LIST` v1, 20 files): u32 count, then count x {utf16 key,
/// coord2d position, angle, f32 scale}, all as flat values (CONFIRMED: every file reads to the
/// end).
pub fn read_prop_list(bytes: &[u8]) -> Result<Vec<PlacedObject>, String> {
    use crate::esf::{EsfFile, EsfNode};
    let f = EsfFile::from_bytes(bytes).map_err(|e| e.to_string())?;
    if f.root.name != "BATTLE_PROP_LIST" {
        return Err(format!("root {}", f.root.name));
    }
    let c = &f.root.children;
    let n = c.first().and_then(EsfNode::as_u32).ok_or("no count")? as usize;
    if c.len() != 1 + 4 * n {
        return Err(format!("{} values for {n} props", c.len()));
    }
    c[1..]
        .as_chunks::<4>().0.iter()
        .map(|g| match g {
            [EsfNode::Utf16String(k), EsfNode::Coord2d(x, y), EsfNode::Angle(a), EsfNode::F32(s)] => {
                Ok(PlacedObject { key: k.clone(), position: (*x, *y), angle: *a, scale: (*s, *s) })
            }
            _ => Err("prop is not {utf16, coord2d, angle, f32}".to_string()),
        })
        .collect()
}

/// One `EF_LINE_LIST` item: {utf16, i32, coord3d x3}.
pub type EfLine = (String, i32, [(f32, f32, f32); 3]);

/// `farm_templates\*.farm_template_tile` (root `ROOT_FARM_TILE_TEMPLATE` v0, 32 files): the
/// layout of a farm tile for generated battlefields. Summarised from the ESF (the full tree stays
/// readable with [`crate::esf`]); CONFIRMED record names and child types.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmTileTemplate {
    /// `FARM_LIST` item names (`FARM_DATA_ITEM_OWNER` #0, e.g. `field13`).
    pub farms: Vec<String>,
    /// `WALL_LIST` item names.
    pub walls: Vec<String>,
    /// Every `FARM_DATA_ITEM` {utf16 key, coord2d position, angle, coord2d scale} anywhere in the
    /// template (buildings, props and decals of farms, walls and roads).
    pub objects: Vec<PlacedObject>,
    /// `FARM_TREE_LIST` items {utf16 species, coord2d position}, both lists.
    pub trees: Vec<(String, (f32, f32))>,
    /// `WALL_POST_LIST` items {coord2d, coord2d}.
    pub wall_posts: Vec<((f32, f32), (f32, f32))>,
    /// `EF_LINE_LIST` items {utf16, i32, coord3d x3} (INFERRED: edge/fence lines).
    pub ef_lines: Vec<EfLine>,
    /// Every `FARM_COLLISION` (the outline of each `FARM`, then of each `ROAD`, in file order).
    pub collisions: Vec<FarmCollision>,
}

impl FarmTileTemplate {
    /// Reads a `.farm_template_tile`.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        use crate::esf::{EsfFile, EsfNode};
        let f = EsfFile::from_bytes(bytes).map_err(|e| e.to_string())?;
        if f.root.name != "ROOT_FARM_TILE_TEMPLATE" {
            return Err(format!("root {}", f.root.name));
        }
        let mut t = FarmTileTemplate::default();
        let owner_name = |item: &[EsfNode]| item.first()?.as_record()?.get_str(0).map(str::to_owned);
        if let Some(a) = f.root.record_array("FARM_LIST") {
            t.farms = a.items.iter().filter_map(|i| owner_name(i)).collect();
        }
        if let Some(a) = f.root.record_array("WALL_LIST") {
            t.walls = a.items.iter().filter_map(|i| owner_name(i)).collect();
        }
        f.root.walk(&mut |r| match r.name.as_str() {
            "FARM_DATA_ITEM" => {
                if let [EsfNode::Utf16String(k), EsfNode::Coord2d(x, y), EsfNode::Angle(a), EsfNode::Coord2d(sx, sy)] = &r.children[..] {
                    t.objects.push(PlacedObject { key: k.clone(), position: (*x, *y), angle: *a, scale: (*sx, *sy) });
                }
            }
            "FARM_TREE" => {
                if let [EsfNode::Utf16String(k), EsfNode::Coord2d(x, y)] = &r.children[..] {
                    t.trees.push((k.clone(), (*x, *y)));
                }
            }
            "WALL_POST" => {
                if let [EsfNode::Coord2d(a, b), EsfNode::Coord2d(c, d)] = &r.children[..] {
                    t.wall_posts.push(((*a, *b), (*c, *d)));
                }
            }
            "FARM_COLLISION" => {
                if let Some(c) = FarmCollision::from_record(r) {
                    t.collisions.push(c);
                }
            }
            "EF_LINE" => {
                if let [EsfNode::Utf16String(k), EsfNode::I32(v), EsfNode::Coord3d(a, b, c), EsfNode::Coord3d(d, e, g), EsfNode::Coord3d(h, i, j)] = &r.children[..] {
                    t.ef_lines.push((k.clone(), *v, [(*a, *b, *c), (*d, *e, *g), (*h, *i, *j)]));
                }
            }
            _ => {}
        });
        Ok(t)
    }
}

/// `FARM_COLLISION` v2 (in every `FARM` and `ROAD` of a farm template; reader `0x00FD63B0`).
/// Field meanings CONFIRMED on all 387 records of the 32 templates (`farm_probe collision`),
/// unless tagged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmCollision {
    /// #0 coord2d reference point of the outline (INFERRED: the centre used for the radius tests;
    /// it is not the box centre or the area centroid in general).
    pub centre: (f32, f32),
    /// #1 coord2d[] the outline polygon.
    pub outline: Vec<(f32, f32)>,
    /// #2 f32 distance from `centre` to the nearest outline edge (inner radius). Records before v2
    /// stored the square (the reader takes the square root).
    pub inner_radius: f32,
    /// #3 f32 distance from `centre` to the farthest outline point (outer radius); squared before v2.
    pub outer_radius: f32,
    /// #4, #5 coord2d box min / max: the outline's box grown by 12 m and floored to whole metres
    /// (375 of 387; INFERRED a margin for the ground-type / grass masks).
    pub box_min: (f32, f32),
    /// See `box_min`.
    pub box_max: (f32, f32),
    /// #6 bool (v2 only; default true; true in every file): CONFIRMED "the inner circle is usable" (the
    /// game's point/rectangle tests accept within `inner_radius` only when set; its run-time builder sets it
    /// when the centre lies inside the outline, S1_LEFTOVERS.md §2).
    pub flag: bool,
}

impl FarmCollision {
    fn from_record(r: &crate::esf::EsfRecord) -> Option<Self> {
        use crate::esf::EsfNode;
        let c2 = |i: usize| match r.get(i) {
            Some(EsfNode::Coord2d(x, y)) => Some((*x, *y)),
            _ => None,
        };
        let r2 = |i: usize| r.get_f32(i).map(|v| if r.version < 2 { v.max(0.0).sqrt() } else { v });
        Some(Self {
            centre: c2(0)?,
            outline: r.get(1).and_then(EsfNode::as_coord2d_array).map(<[_]>::to_vec).unwrap_or_default(),
            inner_radius: r2(2)?,
            outer_radius: r2(3)?,
            box_min: c2(4)?,
            box_max: c2(5)?,
            flag: r.get_bool(6).unwrap_or(true),
        })
    }
}

/// An instance's placement (`FARM_DATA_ITEM_OWNER_INSTANCE` v1; writer `0x00E99130`; CONFIRMED layout).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OwnerInstance {
    /// u32 index of the owner (farm, wall or road) in the tile template's `FARM_LIST` /
    /// `WALL_LIST` / `ROAD_LIST` (INFERRED: values stay below those list sizes).
    pub owner: u32,
    /// u32: the index of the owner's tile template in the manager's template list
    /// ([`FarmManager::templates`]). CONFIRMED: the owner accessor `0x00EEE7A0` returns item `owner`
    /// (0xFC bytes each) of the object list of template `unknown` of the manager, and the generator
    /// copies the value from a farm to the buildings and walls it places. 0 in every file (no shipped
    /// manager has more than one template).
    pub unknown: u32,
    /// Three coord3d rows of a 2D affine transform (x' = r0 . (x, y, 1), y' = r1 . (x, y, 1),
    /// r2 = (0, 0, 1)); CONFIRMED form: identity rotation plus a translation in every file.
    pub transform: [(f32, f32, f32); 3],
}

/// One `FARM_INSTANCE` v2 (writer `0x00E992D0`, reader `0x00E88C80`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmInstance {
    /// The farm template item and where it is placed.
    pub placement: OwnerInstance,
    /// u32 count, then count x {u32, u32}: the walls on the farm's edge, as {wall instance index
    /// (into [`FarmManager::walls`]), side}. The side is the wall's slot for this farm: the third
    /// value of the wall's {farm, list, slot} entry, 0 or 1. CONFIRMED in the data: every wall-to-farm
    /// link of the 3 managers with farms has the matching pair (86 of 86), with side = slot each time.
    /// CONFIRMED in the exe: the copy made for a farm in both lists (`0x00E891C0`) takes the other
    /// side (1 - side) and leaves out walls with a single template (+0xD4 = 1) and +0xE4 = 0.
    pub pieces: Vec<(u32, u32)>,
    /// bool (false in every file; UNKNOWN).
    pub flag: bool,
    /// bool: set when another instance in the two farm lists has the same owner and transform
    /// (CONFIRMED: the reader computes it that way for files before v2); false in every file.
    pub duplicate: bool,
}

/// One `FARM_TILE_SET` cell (an item of the set; reader `0x00E89790`, writer `0x00E99A00`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmTileCell {
    /// i32, i32 the cell's (column, row) in the set's grid of template tiles (values -2..2;
    /// INFERRED).
    pub cell: (i32, i32),
    /// {u32 index, i32 list}: the farm instances in this cell, as an index into the manager's farm
    /// list 0 or 1 (CONFIRMED layout; the list reading INFERRED: every index is below the size of the
    /// list its second value names).
    pub farms: Vec<(u32, i32)>,
    /// u32[] (INFERRED: wall instance indices; the largest value is below the wall count).
    pub walls: Vec<u32>,
    /// u32[] only in v1 sets; the reader skips it and the v2 writer drops it (empty in every file).
    pub legacy: Vec<u32>,
    /// u32[] (empty in every file; UNKNOWN, INFERRED road or post instances).
    pub other: Vec<u32>,
}

/// One `FARM_TILE_SET` v1 (CONFIRMED layout).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmTileSet {
    /// 2D affine transform of the set (three coord3d rows, as [`OwnerInstance::transform`]).
    pub transform: [(f32, f32, f32); 3],
    /// u32 index of the template (into [`FarmManager::templates`]; CONFIRMED by the reader).
    pub template: u32,
    /// The cells.
    pub cells: Vec<FarmTileCell>,
}

/// One `FARM_TILE_TEMPLATE` v3/v4 entry of a farm manager (writer `0x00E99E60`) plus the u32 that
/// follows it (CONFIRMED order from the writer; meanings from the shipped values).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmTemplateRef {
    /// The `.farm_template_tile` path.
    pub tile_template: String,
    /// u32 count + paths of the `.farm_fields_tile_texture` maps: blend, colour and grass maps,
    /// each for field / building / road (9 in every file).
    pub textures: Vec<String>,
    /// The tiled underlay map (`BattleTerrain/tiled_maps/farm_underlay1a`).
    pub underlay: String,
    /// The wall texture folder (`BattleTerrain/walls/textures`).
    pub wall_textures: String,
    /// The wall model key (`Wall_Asia`) and its end piece.
    pub wall: (String, String),
    /// The wall spline (`BattleTerrain/walls/Wall01_XS.rigid_spline`).
    pub wall_spline: String,
    /// Fence pieces (lengths 1, 2, 4, 8) and the end piece.
    pub fences: Vec<String>,
    /// Hedge pieces (lengths 1, 2, 4, 8) and the end piece.
    pub hedges: Vec<String>,
    /// The u32 after the record: a checksum/hash the writer computes (`0x00EAB460`, a rotate-xor
    /// hash; INFERRED of the template contents).
    pub hash: u32,
}

/// `.farm_manager` (root `ROOT_FARM_MANAGER` v0, `FARM_MANAGER` v3 or v4; writer `0x00E99490`,
/// reader `0x00EC67B0`): the farms placed on a battle map. CONFIRMED layout (every shipped file
/// reads to its last value); field meanings as tagged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FarmManager {
    /// coord2d min, coord2d max: the map area (e.g. -1024..1024).
    pub bounds: ((f32, f32), (f32, f32)),
    /// The templates used.
    pub templates: Vec<FarmTemplateRef>,
    /// The tile sets.
    pub tile_sets: Vec<FarmTileSet>,
    /// The two farm instance lists: list 0 = farms whose box lies inside the map's playable rectangle,
    /// list 1 = farms reaching outside it (CONFIRMED, the generator's `0x00EA6780`; S1_LEFTOVERS.md §2).
    pub farms: [Vec<FarmInstance>; 2],
    /// `WALL_INSTANCE` placements (the rest of the record is kept raw: i32, u32 n, n x {u32, i32,
    /// u32}, u32 m, m x u32; UNKNOWN meanings).
    pub walls: Vec<(OwnerInstance, Vec<i64>)>,
    /// `ROAD_INSTANCE` placements {placement, u32 n, n x {u32, u32}} (none in the shipped files).
    pub roads: Vec<(OwnerInstance, Vec<(u32, u32)>)>,
    /// `POST_INSTANCE` records (u32, u32 n, n x u32, u32, 3 x coord3d), kept raw (none shipped).
    pub posts: Vec<Vec<i64>>,
    /// The names that follow: one per farm instance (both lists), wall and road (e.g. `field66`),
    /// then one coord2d per post.
    pub names: Vec<String>,
}

impl FarmManager {
    /// Reads a `.farm_manager`.
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        use crate::esf::{EsfFile, EsfNode, EsfRecord};
        let f = EsfFile::from_bytes(bytes).map_err(|e| e.to_string())?;
        if f.root.name != "ROOT_FARM_MANAGER" {
            return Err(format!("root {}", f.root.name));
        }
        let m = f.root.child("FARM_MANAGER").ok_or("no FARM_MANAGER")?;
        let mut c = m.children.iter().peekable();
        let mut out = FarmManager::default();
        let coord2 = |n: Option<&EsfNode>| match n {
            Some(EsfNode::Coord2d(x, y)) => Ok((*x, *y)),
            other => Err(format!("expected coord2d, got {other:?}")),
        };
        let int = |n: Option<&EsfNode>| n.and_then(EsfNode::as_int).ok_or_else(|| "expected an integer".to_string());
        let coord3 = |n: Option<&EsfNode>| match n {
            Some(EsfNode::Coord3d(x, y, z)) => Some((*x, *y, *z)),
            _ => None,
        };
        let owner_instance = |r: &EsfRecord| -> Result<OwnerInstance, String> {
            let t = |i: usize| coord3(r.get(i)).ok_or("bad transform");
            Ok(OwnerInstance {
                owner: r.get_u32(0).ok_or("bad owner")?,
                unknown: r.get_u32(1).ok_or("bad owner")?,
                transform: [t(2)?, t(3)?, t(4)?],
            })
        };
        out.bounds = (coord2(c.next())?, coord2(c.next())?);
        for _ in 0..int(c.next())? {
            let t = named_record(c.next(), "FARM_TILE_TEMPLATE")?;
            let s: Vec<String> = t.children.iter().filter_map(|n| n.as_str().map(str::to_owned)).collect();
            let n = t.get_u32(1).unwrap_or(0) as usize;
            let rest = s.get(1 + n..).unwrap_or_default();
            let at = |i: usize| rest.get(i).cloned().unwrap_or_default();
            let pieces = rest.get(5..).unwrap_or_default();
            let half = pieces.len() / 2;
            out.templates.push(FarmTemplateRef {
                tile_template: s.first().cloned().unwrap_or_default(),
                textures: s.get(1..1 + n).map(<[_]>::to_vec).unwrap_or_default(),
                underlay: at(0),
                wall_textures: at(1),
                wall: (at(2), at(3)),
                wall_spline: at(4),
                fences: pieces[..half].to_vec(),
                hedges: pieces[half..].to_vec(),
                hash: int(c.next())? as u32,
            });
        }
        for _ in 0..int(c.next())? {
            let s = named_record(c.next(), "FARM_TILE_SET")?;
            let t = |i: usize| coord3(s.get(i)).ok_or("bad tile set transform");
            let v: Vec<i64> = s.children.iter().skip(3).map(|n| n.as_int().ok_or("bad tile set value")).collect::<Result<_, _>>()?;
            let mut i = 0;
            let mut take = |k: usize| take_values(&v, &mut i, k);
            let template = take(1)?[0] as u32;
            let n = take(1)?[0] as usize;
            let mut cells = Vec::with_capacity(n.min(4096));
            for _ in 0..n {
                let ab = take(2)?;
                let cell = (ab[0] as i32, ab[1] as i32);
                let np = take(1)?[0] as usize;
                let farms = take(2 * np)?.as_chunks::<2>().0.iter().map(|&[i, l]| (i as u32, l as i32)).collect();
                let mut list = || -> Result<Vec<u32>, String> {
                    let k = take(1)?[0] as usize;
                    Ok(take(k)?.iter().map(|&x| x as u32).collect())
                };
                let walls = list()?;
                let legacy = if s.version < 2 { list()? } else { Vec::new() };
                let other = list()?;
                cells.push(FarmTileCell { cell, farms, walls, legacy, other });
            }
            if i != v.len() {
                return Err(format!("FARM_TILE_SET: {} values left", v.len() - i));
            }
            out.tile_sets.push(FarmTileSet { transform: [t(0)?, t(1)?, t(2)?], template, cells });
        }
        for list in 0..2 {
            for _ in 0..int(c.next())? {
                let r = named_record(c.next(), "FARM_INSTANCE")?;
                let placement = owner_instance(named_record(r.get(0), "FARM_DATA_ITEM_OWNER_INSTANCE")?)?;
                let n = r.get_u32(1).unwrap_or(0) as usize;
                let pieces = (0..n).map(|k| (r.get_u32(2 + 2 * k).unwrap_or(0), r.get_u32(3 + 2 * k).unwrap_or(0))).collect();
                out.farms[list].push(FarmInstance {
                    placement,
                    pieces,
                    flag: r.get_bool(2 + 2 * n).unwrap_or(false),
                    duplicate: r.get_bool(3 + 2 * n).unwrap_or(false),
                });
            }
        }
        for _ in 0..int(c.next())? {
            let r = named_record(c.next(), "WALL_INSTANCE")?;
            let p = owner_instance(named_record(r.get(0), "FARM_DATA_ITEM_OWNER_INSTANCE")?)?;
            out.walls.push((p, r.children.iter().skip(1).filter_map(EsfNode::as_int).collect()));
        }
        for _ in 0..int(c.next())? {
            let r = named_record(c.next(), "ROAD_INSTANCE")?;
            let p = owner_instance(named_record(r.get(0), "FARM_DATA_ITEM_OWNER_INSTANCE")?)?;
            let n = r.get_u32(1).unwrap_or(0) as usize;
            out.roads.push((p, (0..n).map(|k| (r.get_u32(2 + 2 * k).unwrap_or(0), r.get_u32(3 + 2 * k).unwrap_or(0))).collect()));
        }
        for _ in 0..int(c.next())? {
            let r = named_record(c.next(), "POST_INSTANCE")?;
            out.posts.push(r.children.iter().filter_map(EsfNode::as_int).collect());
        }
        for n in c {
            if let Some(s) = n.as_str() {
                out.names.push(s.to_owned());
            }
        }
        Ok(out)
    }
}

/// The next `k` values of a flattened record, advancing the cursor.
fn take_values<'a>(v: &'a [i64], i: &mut usize, k: usize) -> Result<&'a [i64], String> {
    let r = v.get(*i..*i + k).ok_or("record ends early")?;
    *i += k;
    Ok(r)
}

/// The record child `n` if it is a record with this name.
fn named_record<'a>(n: Option<&'a crate::esf::EsfNode>, name: &str) -> Result<&'a crate::esf::EsfRecord, String> {
    match n {
        Some(crate::esf::EsfNode::Record(r)) if r.name == name => Ok(r.as_ref()),
        other => Err(format!("expected {name}, got {:?}", other.map(crate::esf::EsfNode::type_name))),
    }
}

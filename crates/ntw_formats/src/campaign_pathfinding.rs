//! Campaign pathfinding data: `campaign_maps\<map>\pathfinding.esf` and `sea_grids.esf`.
//!
//! Spec: `analysis/campaign/CAMPAIGN_DATA.md` §1 (worker campaign-data, from the files and Ghidra).
//! Tags: CONFIRMED / INFERRED / UNKNOWN.
//!
//! # `pathfinding.esf` (root `root` v0, CONFIRMED on all 5 maps)
//! ```text
//! root
//!   {obstacles} { u8[1024] }             only nap_spain: the key of the value cipher, below
//!   pathfinding_areas[]                  1 item on every map
//!     vertices[] { i32 x, i32 z }        Fixed20 map units; items 0..3 are (-2^31+1, -2^31+1) sentinels
//!     u32[]                              outline lists: n, then n vertex indices, back to back
//!     {grid_data}
//!       i32 x0, i32 z0                   Fixed20 south-west corner of the grid (= theatre min)
//!       u16, u16                         UNKNOWN (115, 65 on Europe)
//!       i32 cell                         Fixed20 cell size (2.0 on every map)
//!       u32 cols, u32 rows               grid size (Europe 375 x 193 = the theatre / 2)
//!       u32                              UNKNOWN (103563 on Europe)
//!       u16 region_count, u16            pathfinding regions (72 on Europe = its land regions)
//!       i16[region_count]                pathfinding region -> `regions.esf` region index
//!       u16[]                            region table: region_count 1-based indices into the i16[],
//!                                        then region_count count-prefixed lists (INFERRED: neighbours)
//!       grid_cells[]                     the cells, see [`GridCellRecord`]
//!   barriers[]                           only nap_tut: { utf16 name, 9 x u32[] }
//! ```
//! The cell list expands to exactly `cols * rows` cells (CONFIRMED on all 5 maps).
//!
//! ## The value cipher (`obstacles`, CONFIRMED by decrypting nap_spain to sane data)
//! When `obstacles` exists and its first byte is 0x93, the payload bytes of the area's values are
//! XORed with a key stream, in file order, before use (the exe reads them through a decoding stream;
//! `FUN_00b51f80` builds it, `FUN_00b273f0` is the byte step). Per byte:
//! `seed = seed * 0x343FD + 0x269EC3` (wrapping), `byte ^= (seed >> 16) as u8 + key[pos]`,
//! `pos = (pos - step) & 0x3FF`; initially `seed` = the u32 at key bytes 1..5, `pos` = 0 and `step`
//! = the u32 at key bytes 0x237..0x23B. Encrypted: the vertices, the u32 list array, the grid header
//! and its two region arrays. The grid cells are read by the stream's own cell reader and are not
//! XORed (CONFIRMED: Spain's cells parse to valid list offsets without it).
//! The exe also checks a content flag (bit 0x200 of a game-settings word) before it builds the
//! stream; we always decode (the file is in the user's install).
//!
//! # `sea_grids.esf` (root `CAI_SEA_GRID_ROOT` v0, a flat list of values)
//! The campaign AI's coarse sea grid (`CAI_SEA_GRID_CELL*` strings in the exe), see [`SeaGrid`].

use crate::esf::{EsfFile, EsfNode, EsfRecord};

/// The sentinel coordinate stored in vertices 0..3 (`-2^31 + 1`).
pub const SENTINEL: i32 = -2_147_483_647;

/// A reader error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathfindingError(pub String);

impl std::fmt::Display for PathfindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pathfinding data: {}", self.0)
    }
}

impl std::error::Error for PathfindingError {}

fn err<T>(msg: impl Into<String>) -> Result<T, PathfindingError> {
    Err(PathfindingError(msg.into()))
}

/// One entry of a cell's boundary list: two u32 words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Boundary {
    /// Word 0. UNKNOWN bit field (values seen: 2, 0x0D110001, 0x8F110101, ...).
    pub flags: u32,
    /// Word 1: bits 0..22 = start of an outline list in the area's u32 array (CONFIRMED: always a
    /// list start), bits 22..32 = a pathfinding region id (0..region_count; 1023 = none).
    pub link: u32,
}

impl Boundary {
    /// Offset of the outline list (index of its count) in [`PathfindingArea::lists`].
    pub fn list_offset(self) -> usize {
        (self.link & 0x3F_FFFF) as usize
    }
    /// The 10-bit pathfinding region id (1023 = none).
    pub fn region(self) -> u16 {
        (self.link >> 22) as u16
    }
}

/// One item of `grid_cells[]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCellRecord {
    /// u8[8]: UNKNOWN per-cell bytes (values 0..32 and 255).
    pub header: [u8; 8],
    /// `boundaries[]`: the cell's boundary list (empty for a compact run).
    pub boundaries: Vec<Boundary>,
    /// A compact run (only when `boundaries` is empty): this cell and `sub.len()` more cells that
    /// each hold a single boundary {flags, region << 22} (INFERRED: list offset 0).
    pub run: Option<CompactRun>,
}

/// The compact form of a cell record: `u32 flags, u16 region, u8[12 * n]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactRun {
    /// The first cell's boundary flags.
    pub flags: u32,
    /// The pathfinding region id shared by the whole run.
    pub region: u16,
    /// The following cells: header and boundary flags (12 bytes each in the file).
    pub sub: Vec<([u8; 8], u32)>,
}

/// One expanded grid cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    /// The cell's 8 header bytes.
    pub header: [u8; 8],
    /// Its boundaries.
    pub boundaries: Vec<Boundary>,
}

/// `grid_data`.
#[derive(Debug, Clone, PartialEq)]
pub struct GridData {
    /// Fixed20 south-west corner (x, z).
    pub origin: (i32, i32),
    /// The two UNKNOWN u16 after the origin.
    pub unknown_u16: (u16, u16),
    /// Fixed20 cell size.
    pub cell_size: i32,
    /// Columns.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    /// UNKNOWN u32 after the size.
    pub unknown_7: u32,
    /// Number of pathfinding regions.
    pub region_count: u16,
    /// UNKNOWN u16 (equal to `region_count` on every map).
    pub unknown_9: u16,
    /// i16[]: pathfinding region -> index into `regions.esf` `region_data` regions.
    pub region_map: Vec<i16>,
    /// u16[]: the region table (see the module docs).
    pub region_table: Vec<u16>,
    /// The raw cell records in file order.
    pub records: Vec<GridCellRecord>,
    /// True when the records are stored last cell first. The encrypted stream's cell reader
    /// reverses the list after reading (`FUN_00b60b20`), and nap_spain's cells only match its
    /// regions that way (CONFIRMED: 7527/7527 single-region cells vs 29 in file order).
    pub stored_reversed: bool,
}

/// One item of `pathfinding_areas[]`.
#[derive(Debug, Clone, PartialEq)]
pub struct PathfindingArea {
    /// Fixed20 (x, z) vertices; 0..3 are sentinels.
    pub vertices: Vec<(i32, i32)>,
    /// The count-prefixed outline lists (raw u32 array).
    pub lists: Vec<u32>,
    /// The cell grid.
    pub grid: GridData,
}

/// One item of `barriers[]` (nap_tut only).
#[derive(Debug, Clone, PartialEq)]
pub struct Barrier {
    /// e.g. `shroud_map_1_3`.
    pub name: String,
    /// The nine u32 arrays. INFERRED: polylines of Fixed20 (x, z) pairs.
    pub lines: Vec<Vec<u32>>,
}

/// A whole `pathfinding.esf`.
#[derive(Debug, Clone, PartialEq)]
pub struct PathfindingFile {
    /// The `obstacles` key, if present (nap_spain).
    pub obstacles: Option<Vec<u8>>,
    /// True if the values were decrypted with that key.
    pub decrypted: bool,
    /// The areas (one per map).
    pub areas: Vec<PathfindingArea>,
    /// Shroud barriers (nap_tut).
    pub barriers: Vec<Barrier>,
}

/// The key stream of the value cipher (see the module docs).
#[derive(Debug, Clone)]
pub struct Cipher {
    key: Vec<u8>,
    seed: u32,
    pos: u32,
    step: u32,
}

impl Cipher {
    /// The marker byte the key must start with.
    pub const MARKER: u8 = 0x93;

    /// A stream for a 1024-byte key, or `None` if the key is not one (wrong size or marker).
    pub fn new(key: &[u8]) -> Option<Self> {
        if key.len() < 0x400 || key[0] != Self::MARKER {
            return None;
        }
        let u = |o: usize| u32::from_le_bytes([key[o], key[o + 1], key[o + 2], key[o + 3]]);
        Some(Self { key: key[..0x400].to_vec(), seed: u(1), pos: 0, step: u(0x237) })
    }

    /// The next key byte.
    pub fn next_byte(&mut self) -> u8 {
        self.seed = self.seed.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        let k = ((self.seed >> 16) as u8).wrapping_add(self.key[self.pos as usize]);
        self.pos = self.pos.wrapping_sub(self.step) & 0x3FF;
        k
    }

    /// XORs `bytes` in place.
    pub fn apply(&mut self, bytes: &mut [u8]) {
        for b in bytes {
            *b ^= self.next_byte();
        }
    }

    fn u32(&mut self, v: u32) -> u32 {
        let mut b = v.to_le_bytes();
        self.apply(&mut b);
        u32::from_le_bytes(b)
    }
    fn i32(&mut self, v: i32) -> i32 {
        self.u32(v as u32) as i32
    }
    fn u16(&mut self, v: u16) -> u16 {
        let mut b = v.to_le_bytes();
        self.apply(&mut b);
        u16::from_le_bytes(b)
    }
}

/// Optional decryption of a stream of values.
struct Dec(Option<Cipher>);
impl Dec {
    fn u32(&mut self, v: u32) -> u32 {
        self.0.as_mut().map_or(v, |c| c.u32(v))
    }
    fn i32(&mut self, v: i32) -> i32 {
        self.0.as_mut().map_or(v, |c| c.i32(v))
    }
    fn u16(&mut self, v: u16) -> u16 {
        self.0.as_mut().map_or(v, |c| c.u16(v))
    }
}

fn get<'a>(r: &'a EsfRecord, i: usize, what: &str) -> Result<&'a EsfNode, PathfindingError> {
    r.get(i).ok_or_else(|| PathfindingError(format!("{} #{i} ({what}) missing", r.name)))
}

fn as_u16(n: &EsfNode) -> Option<u16> {
    match n {
        EsfNode::U16(v) => Some(*v),
        _ => None,
    }
}

impl PathfindingFile {
    /// Parses `pathfinding.esf` bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, PathfindingError> {
        let esf = EsfFile::from_bytes(bytes).map_err(|e| PathfindingError(e.to_string()))?;
        Self::from_esf(&esf)
    }

    /// Reads an already parsed file.
    pub fn from_esf(esf: &EsfFile) -> Result<Self, PathfindingError> {
        let root = &esf.root;
        let obstacles = root.child("obstacles").and_then(|o| o.get(0)?.as_u8_array()).map(<[u8]>::to_vec);
        let Some(areas_arr) = root.record_array("pathfinding_areas") else {
            return err("no pathfinding_areas");
        };
        let mut decrypted = false;
        let mut areas = Vec::new();
        for item in &areas_arr.items {
            let cipher = obstacles.as_deref().and_then(Cipher::new);
            decrypted |= cipher.is_some();
            areas.push(read_area(item, Dec(cipher))?);
        }
        let mut barriers = Vec::new();
        if let Some(b) = root.record_array("barriers") {
            for item in &b.items {
                let name = item.first().and_then(EsfNode::as_str).unwrap_or_default().to_string();
                let lines = item[1..].iter().filter_map(|n| n.as_u32_array().map(<[u32]>::to_vec)).collect();
                barriers.push(Barrier { name, lines });
            }
        }
        Ok(Self { obstacles, decrypted, areas, barriers })
    }
}

fn read_area(item: &[EsfNode], mut dec: Dec) -> Result<PathfindingArea, PathfindingError> {
    let (Some(EsfNode::RecordArray(va)), Some(EsfNode::U32Array(lists)), Some(EsfNode::Record(grid))) =
        (item.first(), item.get(1), item.get(2))
    else {
        return err("pathfinding_areas item is not [vertices], u32[], {grid_data}");
    };
    let mut vertices = Vec::with_capacity(va.items.len());
    for v in &va.items {
        let (Some(x), Some(z)) = (v.first().and_then(EsfNode::as_i32), v.get(1).and_then(EsfNode::as_i32)) else {
            return err("vertex is not two i32");
        };
        vertices.push((dec.i32(x), dec.i32(z)));
    }
    let lists: Vec<u32> = lists.iter().map(|&v| dec.u32(v)).collect();

    let g = grid;
    let i32_at = |i| get(g, i, "i32").and_then(|n| n.as_i32().ok_or_else(|| PathfindingError(format!("grid_data #{i} not i32"))));
    let u32_at = |i| get(g, i, "u32").and_then(|n| n.as_u32().ok_or_else(|| PathfindingError(format!("grid_data #{i} not u32"))));
    let u16_at = |i| get(g, i, "u16").and_then(|n| as_u16(n).ok_or_else(|| PathfindingError(format!("grid_data #{i} not u16"))));
    let origin = (dec.i32(i32_at(0)?), dec.i32(i32_at(1)?));
    let unknown_u16 = (dec.u16(u16_at(2)?), dec.u16(u16_at(3)?));
    let cell_size = dec.i32(i32_at(4)?);
    let cols = dec.u32(u32_at(5)?);
    let rows = dec.u32(u32_at(6)?);
    let unknown_7 = dec.u32(u32_at(7)?);
    let region_count = dec.u16(u16_at(8)?);
    let unknown_9 = dec.u16(u16_at(9)?);
    let region_map: Vec<i16> = match get(g, 10, "i16[]")? {
        EsfNode::I16Array(v) => v.iter().map(|&x| dec.u16(x as u16) as i16).collect(),
        _ => return err("grid_data #10 is not i16[]"),
    };
    let region_table: Vec<u16> = match get(g, 11, "u16[]")? {
        EsfNode::U16Array(v) => v.iter().map(|&x| dec.u16(x)).collect(),
        _ => return err("grid_data #11 is not u16[]"),
    };
    let Some(EsfNode::RecordArray(cells)) = g.get(12) else {
        return err("grid_data #12 is not grid_cells[]");
    };
    let stored_reversed = dec.0.is_some();
    let mut records = Vec::with_capacity(cells.items.len());
    for c in &cells.items {
        records.push(read_cell(c)?);
    }
    Ok(PathfindingArea {
        vertices,
        lists,
        grid: GridData {
            origin,
            unknown_u16,
            cell_size,
            cols,
            rows,
            unknown_7,
            region_count,
            unknown_9,
            region_map,
            region_table,
            records,
            stored_reversed,
        },
    })
}

fn header8(n: Option<&EsfNode>) -> Result<[u8; 8], PathfindingError> {
    match n.and_then(EsfNode::as_u8_array) {
        Some(b) if b.len() == 8 => Ok(b.try_into().unwrap_or_default()),
        _ => err("cell header is not u8[8]"),
    }
}

fn read_cell(c: &[EsfNode]) -> Result<GridCellRecord, PathfindingError> {
    let header = header8(c.first())?;
    let Some(EsfNode::RecordArray(b)) = c.get(1) else {
        return err("cell #1 is not boundaries[]");
    };
    let mut boundaries = Vec::with_capacity(b.items.len());
    for it in &b.items {
        let (Some(flags), Some(link)) = (it.first().and_then(EsfNode::as_u32), it.get(1).and_then(EsfNode::as_u32)) else {
            return err("boundary is not two u32");
        };
        boundaries.push(Boundary { flags, link });
    }
    let run = if c.len() > 2 {
        let (Some(flags), Some(region), Some(tail)) =
            (c.get(2).and_then(EsfNode::as_u32), c.get(3).and_then(as_u16), c.get(4).and_then(EsfNode::as_u8_array))
        else {
            return err("compact cell is not u32, u16, u8[]");
        };
        if tail.len() % 12 != 0 {
            return err(format!("compact cell tail of {} bytes is not 12 x n", tail.len()));
        }
        let sub = tail
            .as_chunks::<12>().0.iter()
            .map(|s| (s[..8].try_into().unwrap_or_default(), u32::from_le_bytes([s[8], s[9], s[10], s[11]])))
            .collect();
        Some(CompactRun { flags, region, sub })
    } else {
        None
    };
    Ok(GridCellRecord { header, boundaries, run })
}

impl GridData {
    /// The cells in grid order (row-major from the south-west corner, CONFIRMED by matching every
    /// single-region cell of all 5 maps to `regions.esf`), with compact runs expanded
    /// (`cols * rows` cells on every shipped map).
    pub fn expand(&self) -> Vec<GridCell> {
        let mut out = Vec::with_capacity((self.cols * self.rows) as usize);
        for r in &self.records {
            match &r.run {
                None => out.push(GridCell { header: r.header, boundaries: r.boundaries.clone() }),
                Some(run) => {
                    let link = u32::from(run.region) << 22;
                    out.push(GridCell { header: r.header, boundaries: vec![Boundary { flags: run.flags, link }] });
                    for (h, flags) in &run.sub {
                        out.push(GridCell { header: *h, boundaries: vec![Boundary { flags: *flags, link }] });
                    }
                }
            }
        }
        if self.stored_reversed {
            out.reverse();
        }
        out
    }

    /// Cell size in map units.
    pub fn cell_units(&self) -> f32 {
        self.cell_size as f32 / (1 << 20) as f32
    }

    /// The origin in map units.
    pub fn origin_units(&self) -> (f32, f32) {
        (self.origin.0 as f32 / (1 << 20) as f32, self.origin.1 as f32 / (1 << 20) as f32)
    }
}

impl PathfindingArea {
    /// Splits the u32 array into its count-prefixed lists: (offset of the count, the indices).
    /// `None` if the array does not split exactly.
    pub fn outline_lists(&self) -> Option<Vec<(usize, &[u32])>> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.lists.len() {
            let n = self.lists[i] as usize;
            let end = i.checked_add(1 + n)?;
            if n == 0 || end > self.lists.len() {
                return None;
            }
            out.push((i, &self.lists[i + 1..end]));
            i = end;
        }
        Some(out)
    }

    /// A vertex in map units.
    pub fn vertex_units(&self, i: usize) -> Option<(f32, f32)> {
        let v = *self.vertices.get(i)?;
        (v.0 != SENTINEL).then(|| (v.0 as f32 / (1 << 20) as f32, v.1 as f32 / (1 << 20) as f32))
    }
}

/// `sea_grids.esf` (root `CAI_SEA_GRID_ROOT`): the campaign AI's sea zones.
#[derive(Debug, Clone, PartialEq)]
pub struct SeaGrid {
    /// #0 u32 (1 on every map; INFERRED version).
    pub version: u32,
    /// #1 utf16, a number as text (UNKNOWN: build stamp / id).
    pub id: String,
    /// #2, #3 bounds (x, z) min and max in map units (= the theatre).
    pub bounds: ((f32, f32), (f32, f32)),
    /// #4 cell size in map units (75 on Europe).
    pub cell_size: f32,
    /// #5, #6 columns and rows.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    /// The cells (`CAI_SEA_GRID_CELL`), row-major from the south-west.
    pub cells: Vec<SeaGridCell>,
    /// The zone records, one per cell, in cell order.
    pub zones: Vec<SeaZone>,
    /// Pairs of zones with a distance: (zone a, zone b, distance in map units; 0 = UNKNOWN).
    pub links: Vec<(u32, u32, f32)>,
}

/// A sea grid cell's rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeaGridCell {
    /// Column.
    pub col: u32,
    /// Row.
    pub row: u32,
    /// (x, z) minimum corner.
    pub min: (f32, f32),
    /// (x, z) maximum corner.
    pub max: (f32, f32),
}

/// What a sea grid cell touches.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeaZone {
    /// Cell index.
    pub index: u32,
    /// Land-side names: map areas (`eur_map_west`), land region keys and `all`.
    pub land: Vec<String>,
    /// Sea region keys.
    pub seas: Vec<String>,
    /// Port slot keys (`port:<region>:<town>`).
    pub ports: Vec<String>,
    /// u32 list (INFERRED: indices of cells or trade nodes it links to; UNKNOWN).
    pub ids: Vec<u32>,
}

/// A cursor over a flat value list.
struct Flat<'a> {
    v: &'a [EsfNode],
    i: usize,
}

impl<'a> Flat<'a> {
    fn next(&mut self) -> Result<&'a EsfNode, PathfindingError> {
        let n = self.v.get(self.i).ok_or_else(|| PathfindingError(format!("sea_grids ends early at #{}", self.i)))?;
        self.i += 1;
        Ok(n)
    }
    fn u32(&mut self) -> Result<u32, PathfindingError> {
        let at = self.i;
        self.next()?.as_u32().ok_or_else(|| PathfindingError(format!("sea_grids #{at} is not u32")))
    }
    fn f32(&mut self) -> Result<f32, PathfindingError> {
        let at = self.i;
        self.next()?.as_f32().ok_or_else(|| PathfindingError(format!("sea_grids #{at} is not f32")))
    }
    fn str(&mut self) -> Result<String, PathfindingError> {
        let at = self.i;
        Ok(self.next()?.as_str().ok_or_else(|| PathfindingError(format!("sea_grids #{at} is not a string")))?.to_string())
    }
    fn xy(&mut self) -> Result<(f32, f32), PathfindingError> {
        let at = self.i;
        match self.next()? {
            EsfNode::Coord2d(x, z) => Ok((*x, *z)),
            _ => err(format!("sea_grids #{at} is not coord2d")),
        }
    }
    fn strings(&mut self) -> Result<Vec<String>, PathfindingError> {
        let n = self.u32()?;
        (0..n).map(|_| self.str()).collect()
    }
}

impl SeaGrid {
    /// Parses `sea_grids.esf` bytes.
    pub fn read(bytes: &[u8]) -> Result<Self, PathfindingError> {
        let esf = EsfFile::from_bytes(bytes).map_err(|e| PathfindingError(e.to_string()))?;
        if esf.root.name != "CAI_SEA_GRID_ROOT" {
            return err(format!("root is {}, not CAI_SEA_GRID_ROOT", esf.root.name));
        }
        let mut f = Flat { v: &esf.root.children, i: 0 };
        let version = f.u32()?;
        let id = f.str()?;
        let bounds = (f.xy()?, f.xy()?);
        let cell_size = f.f32()?;
        let cols = f.u32()?;
        let rows = f.u32()?;
        let n = (cols as usize).checked_mul(rows as usize).filter(|&n| n <= 1 << 20).ok_or_else(|| PathfindingError("bad sea grid size".into()))?;
        let mut cells = Vec::with_capacity(n);
        for _ in 0..n {
            let col = f.u32()?;
            let row = f.u32()?;
            cells.push(SeaGridCell { col, row, min: f.xy()?, max: f.xy()? });
        }
        let mut zones = Vec::with_capacity(n);
        for _ in 0..n {
            let index = f.u32()?;
            let land = f.strings()?;
            let seas = f.strings()?;
            let ports = f.strings()?;
            let k = f.u32()?;
            let ids = (0..k).map(|_| f.u32()).collect::<Result<_, _>>()?;
            zones.push(SeaZone { index, land, seas, ports, ids });
        }
        let k = f.u32()?;
        let mut links = Vec::with_capacity(k as usize);
        for _ in 0..k {
            links.push((f.u32()?, f.u32()?, f.f32()?));
        }
        if f.i != f.v.len() {
            return err(format!("sea_grids has {} values after the links", f.v.len() - f.i));
        }
        Ok(Self { version, id, bounds, cell_size, cols, rows, cells, zones, links })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cipher_is_its_own_inverse_and_follows_the_spec() {
        let mut key = vec![0u8; 1024];
        key[0] = Cipher::MARKER;
        key[1..5].copy_from_slice(&7u32.to_le_bytes());
        key[0x237..0x23B].copy_from_slice(&3u32.to_le_bytes());
        key[0] = Cipher::MARKER;
        let mut c = Cipher::new(&key).unwrap();
        // First byte: seed 7 -> 7*0x343FD + 0x269EC3; key[0] = 0x93.
        let seed = 7u32.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        assert_eq!(c.next_byte(), ((seed >> 16) as u8).wrapping_add(0x93));
        // pos moves to (0 - 3) & 0x3FF = 1021.
        assert_eq!(c.pos, 1021);
        let data = b"napoleon".to_vec();
        let mut enc = data.clone();
        Cipher::new(&key).unwrap().apply(&mut enc);
        assert_ne!(enc, data);
        Cipher::new(&key).unwrap().apply(&mut enc);
        assert_eq!(enc, data);
        assert!(Cipher::new(&[0u8; 1024]).is_none());
    }

    #[test]
    fn boundary_bits() {
        let b = Boundary { flags: 2, link: 0xFFC0_0005 };
        assert_eq!((b.list_offset(), b.region()), (5, 1023));
        let b = Boundary { flags: 0, link: 0x1200_000C };
        assert_eq!((b.list_offset(), b.region()), (12, 72));
    }
}

impl GridData {
    /// The region-set table that a boundary's 10-bit id indexes: ids `0..region_count` are single
    /// regions, higher ids are the count-prefixed groups that follow in [`GridData::region_table`]
    /// (the first group is empty: INFERRED "no region", the sea). Each entry lists `regions.esf`
    /// region indices. Built as the exe does (`FUN_00af2050`): a table value `v` means
    /// `region_map[v - 1]`. `None` if the table is malformed.
    pub fn region_sets(&self) -> Option<Vec<Vec<i16>>> {
        let t = &self.region_table;
        let map = |v: u16| -> Option<i16> { self.region_map.get(usize::from(v).checked_sub(1)?).copied() };
        let n = usize::from(self.region_count);
        let mut out: Vec<Vec<i16>> = Vec::new();
        for &v in t.get(..n)? {
            out.push(vec![map(v)?]);
        }
        let mut k = n;
        while k < t.len() {
            let c = usize::from(t[k]);
            let items = t.get(k + 1..k + 1 + c)?;
            out.push(items.iter().map(|&v| map(v)).collect::<Option<_>>()?);
            k += 1 + c;
        }
        Some(out)
    }
}

/// What a boundary's flags say about its polygon: the low 4 bits of [`Boundary::flags`] (CONFIRMED: the exe's kind getter `0x00B77EC0` returns `flags & 0xF`; see CAMPAIGN_DATA.md §11).
/// Values seen: land 0, sea 1 (and 17), off-map 2, river 3, road 6, road over water 7.
/// INFERRED from where they lie (rendered over the map): 6 is the thin strip along every road
/// and the octagon round every settlement; 2/3 only occur with region id 1023; 3 is the thin strip
/// along every river (INFERRED: Europe has kind 3 polygons in nearly all of its 3138 cells crossed by a
/// river spline; enterable by neither armies nor fleets, so rivers are crossed only at road (6) and
/// kind 7 polygons, see `analysis/campaign/PATHFINDING.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolygonKind {
    /// Open land (0).
    Land,
    /// Water (1).
    Water,
    /// Not enterable: outside the playable area (2) or a river strip (3).
    OffMap,
    /// Road or settlement (6), or a road over water (7, INFERRED bridge/ford).
    Road,
    /// Any other low-bit value (4, 5: never seen).
    Other,
}

impl Boundary {
    /// The polygon kind from the flags' low 4 bits (kinds 4 and 5 are rare and UNKNOWN: Other).
    pub fn kind(self) -> PolygonKind {
        match self.flags & 0xF {
            0 => PolygonKind::Land,
            1 => PolygonKind::Water,
            2 | 3 => PolygonKind::OffMap,
            6 | 7 => PolygonKind::Road,
            _ => PolygonKind::Other,
        }
    }
}

impl PathfindingArea {
    /// The polygons that partition grid cell `index` (grid order, see [`GridData::expand`]), in
    /// map units, each with its boundary. A cell with a single boundary is one square.
    ///
    /// Outline vertex indices 0..3 stand for the cell's own corners: 0 south-west, 1 north-west,
    /// 2 south-east, 3 north-east (CONFIRMED: with this mapping the polygons of every split cell
    /// of nap_italy add up to exactly the cell's area, and all but one wind counter-clockwise).
    pub fn cell_polygons(&self, index: usize, cell: &GridCell) -> Vec<(Boundary, Vec<(f32, f32)>)> {
        let g = &self.grid;
        let cs = g.cell_units();
        let (ox, oz) = g.origin_units();
        let w = g.cols.max(1) as usize;
        let (cx, cz) = (ox + (index % w) as f32 * cs, oz + (index / w) as f32 * cs);
        let corner = [(cx, cz), (cx, cz + cs), (cx + cs, cz), (cx + cs, cz + cs)];
        if cell.boundaries.len() == 1 {
            return vec![(cell.boundaries[0], vec![corner[0], corner[2], corner[3], corner[1]])];
        }
        let mut out = Vec::with_capacity(cell.boundaries.len());
        for &b in &cell.boundaries {
            let off = b.list_offset();
            let Some(&n) = self.lists.get(off) else { continue };
            let Some(ids) = self.lists.get(off + 1..off + 1 + n as usize) else { continue };
            let pts = ids
                .iter()
                .filter_map(|&v| if v < 4 { Some(corner[v as usize]) } else { self.vertex_units(v as usize) })
                .collect();
            out.push((b, pts));
        }
        out
    }
}

/// Even-odd point-in-polygon test.
pub fn point_in_polygon(p: &[(f32, f32)], x: f32, z: f32) -> bool {
    let mut inside = false;
    let mut j = p.len().wrapping_sub(1);
    for i in 0..p.len() {
        let (a, b) = (p[i], p[j]);
        if (a.1 > z) != (b.1 > z) && x < (b.0 - a.0) * (z - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

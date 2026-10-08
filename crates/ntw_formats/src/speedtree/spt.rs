//! The `.spt` binary layout (SpeedTree 4 "`__IdvSpt_02_`").
//!
//! A stream of little-endian `u32` tokens, each followed by a payload whose type is fixed by the
//! token (no type tags, no lengths except for strings). Sections open with a "begin" token and end
//! with an "end" token one higher (`1002`..`1003`, `8000`..`8001`, ...). The grammar below is the
//! exe's own SpeedTreeRT parser (`CSpeedTreeRT::LoadTree(Memory block)` at `FUN_012b2e10` and the
//! `CTreeEngine` parser `FUN_012ca110`), read in Ghidra — CONFIRMED token by token, see
//! `analysis/speedtree/SPEEDTREE.md` §2 for the function addresses and payload table.
//!
//! Payload types: `i32`, `f32`, `u8` (bool), `str` (`u32` length + bytes, no terminator),
//! `vec3` (3 × `f32`), `spline` (a `str` holding a [`BezierSpline`]).
//!
//! Field names: where the meaning comes from the exe's error strings or from how SpeedTreeRT
//! uses the value, the name says so; where it is still UNKNOWN the field is named after its token
//! (`t2001`) and documented as such. Field docs give the token and, in brackets, the offset the
//! exe stores it at.

use super::spline::BezierSpline;

/// The magic string after the begin-file token (CONFIRMED, `PTR_DAT_0146ce34`).
pub const SPT_MAGIC: &str = "__IdvSpt_02_";

/// Reading error: what went wrong and the byte offset.
#[derive(Debug, Clone, PartialEq)]
pub struct SptError {
    pub offset: usize,
    pub message: String,
}

impl std::fmt::Display for SptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "spt @{}: {}", self.offset, self.message)
    }
}

impl std::error::Error for SptError {}

type R<T> = Result<T, SptError>;

struct Cur<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Cur<'a> {
    fn err<T>(&self, m: impl Into<String>) -> R<T> {
        Err(SptError { offset: self.o, message: m.into() })
    }
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if self.o + n > self.b.len() {
            return self.err(format!("{n} bytes past end"));
        }
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }
    fn i32(&mut self) -> R<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn tok(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn peek_tok(&self) -> Option<u32> {
        self.b.get(self.o..self.o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    }
    fn f32(&mut self) -> R<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> R<bool> {
        Ok(self.u8()? != 0)
    }
    fn vec3(&mut self) -> R<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    fn str(&mut self) -> R<String> {
        let n = self.i32()?;
        if n < 0 {
            return self.err("negative string length");
        }
        Ok(String::from_utf8_lossy(self.take(n as usize)?).into_owned())
    }
    fn spline(&mut self) -> R<BezierSpline> {
        Ok(BezierSpline::parse(&self.str()?))
    }
    fn expect(&mut self, t: u32, what: &str) -> R<()> {
        let got = self.tok()?;
        if got != t {
            self.o -= 4;
            return self.err(format!("{what}: expected token {t}, got {got}"));
        }
        Ok(())
    }
    fn at_end(&self) -> bool {
        self.o >= self.b.len()
    }
}

/// A whole `.spt` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SptFile {
    /// Section 1002..1003: general tree information and the branch levels.
    pub tree: TreeInfo,
    /// Section 1004..1005: general leaf information and the leaf textures.
    pub leaves: LeafInfo,
    /// Section 1011..1012: general wind information (none in some files).
    pub wind: Option<WindInfo>,
    /// Section 7000..7001 right after the end-file token: leaf cluster LODs (none in the shipped files).
    pub leaf_lods: Option<Vec<LeafLod>>,
    /// The optional sections after the tree block, in the order of the exe's `LoadTree` switch.
    pub extra: ExtraSections,
    /// Bytes the exe would not read: the parser stops at an unknown section token (0 on every shipped file).
    pub trailing: usize,
}

/// 1002 section (`FUN_012cc4f0`; error string "malformed general tree information").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeInfo {
    /// 2000: the branch (bark) texture file name (+0x24).
    pub branch_texture: String,
    /// 2001 (+0x3c): f32. INFERRED: the tree size (SpeedTree units).
    pub size: f32,
    /// 2002: a bool the exe skips.
    pub t2002: Option<bool>,
    /// 2003 (+0x40): f32. INFERRED: the size variance.
    pub size_variance: f32,
    /// 2004: an i32 the exe skips.
    pub t2004: Option<i32>,
    /// 2005: the random seed (`FUN_012cc8f0`: 0 = a fresh random seed, 1 = keep the default,
    /// otherwise the seed itself) (+0x44).
    pub seed: Option<i32>,
    /// 2006 (+0x48): f32, UNKNOWN.
    pub t2006: f32,
    /// 2007 (+0x4c): f32, UNKNOWN.
    pub t2007: f32,
    /// 1014..1015: the branch levels, trunk first.
    pub branch_levels: Vec<BranchLevel>,
}

/// One branch level (1016..1017, `FUN_012da3f0`; also the root level of section 40000).
/// Defaults from the constructor `FUN_012da1b0` (CONFIRMED).
#[derive(Debug, Clone, PartialEq)]
pub struct BranchLevel {
    /// 6000..6007: the eight profile splines (+0x74 + k × 0x528 dwords).
    pub splines: [BezierSpline; 8],
    /// 6017: a ninth spline (+0x29b4 dwords).
    pub spline_6017: BezierSpline,
    /// 6008 (+0): i32, default 6. INFERRED: cross-section segments (sides of the tube).
    pub cross_sections: i32,
    /// 6009 (+4): i32, default 3. INFERRED: segments along the branch.
    pub segments: i32,
    /// 6010 (+8): f32, default 0.3.
    pub t6010: f32,
    /// 6011 (+0xc): f32, default 1.0.
    pub t6011: f32,
    /// 6012 (+0x10): f32, default 0.3.
    pub t6012: f32,
    /// 6013 (+0x14): f32, default 1.0 (`FUN_012da390` sub-object, see notes).
    pub t6013: f32,
    /// 6014 (+0x18): f32.
    pub t6014: f32,
    /// 6015 (+0x1c): bool.
    pub t6015: bool,
    /// 6016 (+0x1d): bool.
    pub t6016: bool,
    /// Supplemental values from later sections (15000, 16000, 23000, 26000, 40007).
    pub supplemental: BranchSupplement,
}

impl Default for BranchLevel {
    fn default() -> Self {
        Self {
            splines: Default::default(),
            spline_6017: BezierSpline::default(),
            cross_sections: 6,
            segments: 3,
            t6010: 0.3,
            t6011: 1.0,
            t6012: 0.3,
            t6013: 0.0,
            t6014: 0.0,
            t6015: false,
            t6016: false,
            supplemental: BranchSupplement::default(),
        }
    }
}

/// Per-branch-level values from the supplemental sections. `None` = not in the file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BranchSupplement {
    /// 15002 (+0x1e): bool (section 15000 "texture controls").
    pub t15002: Option<bool>,
    /// 15003 (+0x20): f32.
    pub t15003: Option<f32>,
    /// 16002..16012 (+0x164..+0x18c): section 16000 "flare info": f32, i32 (16003), then 9 f32.
    pub flare: Option<FlareInfo>,
    /// 23002 (+400), 23003 (+0x194): section 23000 "light seam reduction".
    pub light_seam: Option<[f32; 2]>,
    /// 26004.. tokens of section 26000 "supplemental branch info", in file order.
    pub values: Vec<(u32, SptValue)>,
}

/// Section 16000 values for one level (`FUN_012ca490`, "malformed flare info").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlareInfo {
    /// 16002 (+0x164).
    pub t16002: f32,
    /// 16003 (+0x168): i32.
    pub t16003: i32,
    /// 16004..16012 (+0x16c..+0x18c).
    pub rest: [f32; 9],
}

/// A token payload kept generically (supplemental sections whose meanings are still open).
#[derive(Debug, Clone, PartialEq)]
pub enum SptValue {
    I32(i32),
    F32(f32),
    Bool(bool),
    Str(String),
    Vec3([f32; 3]),
    Spline(Box<BezierSpline>),
}

/// Token id and payload pairs, in file order.
pub type SptValueList = Vec<(u32, SptValue)>;

/// 1004 section (`FUN_012c2310`, "malformed general leaf information").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LeafInfo {
    /// 3000 (+0x24): f32.
    pub t3000: f32,
    /// 3001 (+0x2c and +0x50): i32.
    pub t3001: i32,
    /// 3002 (+0x28): f32.
    pub t3002: f32,
    /// 3003, 3006: bools the exe skips.
    pub skipped_bools: Vec<(u32, bool)>,
    /// 3004, 3005: floats the exe skips.
    pub skipped_floats: Vec<(u32, f32)>,
    /// 3007 (+0x18): f32 (−1 or less = unused; otherwise passed to `FUN_012cc940` after loading).
    pub t3007: Option<f32>,
    /// 3008 (+8): i32.
    pub t3008: i32,
    /// 3009 (+0): bool.
    pub t3009: bool,
    /// 3010 (+4): f32.
    pub t3010: f32,
    /// 1009: the two i32s around the texture list (the first is skipped, the count follows) and
    /// the one after it.
    pub texture_list_tokens: Option<[i32; 2]>,
    /// The leaf textures (0x60-byte records at +0xc).
    pub textures: Vec<LeafTexture>,
}

/// One leaf texture (`"malformed single leaf information"`). Defaults CONFIRMED from the parser.
#[derive(Debug, Clone, PartialEq)]
pub struct LeafTexture {
    /// The i32 before the record's tokens (skipped by the exe; INFERRED a begin token 1006).
    pub begin: i32,
    /// 4000 (+0): bool, default false. INFERRED: blossom flag.
    pub t4000: bool,
    /// 4001 (+4): vec3, default (0.8, 0.8, 0.8). INFERRED: colour.
    pub t4001: [f32; 3],
    /// 4002 (+0x10): f32, default 0.2. INFERRED: colour variance.
    pub t4002: f32,
    /// 4003 (+0x14): texture file name (trailing characters trimmed by the exe).
    pub texture: String,
    /// 4004 (+0x2c): vec3, default (0.5, 1, 0).
    pub t4004: [f32; 3],
    /// 4005 (+0x38): vec3, default (0.12, 0.12, 0).
    pub t4005: [f32; 3],
    /// 4006 (+0x44): vec3, default (10, 10, 0).
    pub t4006: [f32; 3],
    /// 4007: an f32 the exe skips.
    pub t4007: Option<f32>,
}

impl Default for LeafTexture {
    fn default() -> Self {
        Self {
            begin: 0,
            t4000: false,
            t4001: [0.8; 3],
            t4002: 0.2,
            texture: String::new(),
            t4004: [0.5, 1.0, 0.0],
            t4005: [0.12, 0.12, 0.0],
            t4006: [10.0, 10.0, 0.0],
            t4007: None,
        }
    }
}

/// 1011 section (`FUN_012dad20`, "malformed general wind information").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindInfo {
    /// 5000, 5001, 5003: vec3s the exe reads and drops.
    pub dropped: Vec<(u32, [f32; 3])>,
    /// 5002 (+0xc): vec3.
    pub t5002: Option<[f32; 3]>,
    /// 5004 (+0): vec3.
    pub t5004: Option<[f32; 3]>,
    /// 5005 (+0x18): f32.
    pub t5005: Option<f32>,
    /// 5006: a bool the exe skips.
    pub t5006: Option<bool>,
}

/// 7002..7003: one leaf cluster LOD level (`CTreeEngine::ParseLeafCluster`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LeafLod {
    /// 7004..7005 billboard leaves.
    pub leaves: Vec<Vec<(u32, SptValue)>>,
}

/// The optional sections read by `LoadTree` after the tree block. `None` = absent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtraSections {
    /// 8000 "lighting information".
    pub lighting: Option<Vec<(u32, SptValue)>>,
    /// 9000 "lod info" (with the 9005 "engine lod data" sub-block flattened in).
    pub lod: Option<Vec<(u32, SptValue)>>,
    /// 10000 "texture coord info".
    pub texcoords: Option<TexCoordInfo>,
    /// 11000 "new wind info": the 11002 values (+0xe8 of the tree object).
    pub new_wind: Option<Vec<i32>>,
    /// 12000 "collision object info".
    pub collision: Option<Vec<CollisionObject>>,
    /// 13000 "frond info".
    pub fronds: Option<FrondInfo>,
    /// 16013 (+0x50): i32.
    pub t16013: Option<i32>,
    /// 16014 (+0x30): f32.
    pub t16014: Option<f32>,
    /// 18000: three vec3s (stored with x negated and y/z swapped) and a string.
    pub t18000: Option<Vec<(u32, SptValue)>>,
    /// 19000: the 19002 strings.
    pub t19000: Option<Vec<String>>,
    /// 20000: texture coord extras (only read when 10000 was present).
    pub t20000: Option<Vec<(u32, SptValue)>>,
    /// 21000, 21001: f32s (+0x3c / +0x40 of the leaf object).
    pub t21000: Option<f32>,
    pub t21001: Option<f32>,
    /// 22000: bool (+0x108 of the tree engine).
    pub t22000: Option<bool>,
    /// 24000 "supplemental leaf placement info": (24002 f32, 24003 i32).
    pub leaf_placement: Option<(f32, i32)>,
    /// 25000 "supplemental frond info".
    pub frond_supplement: Option<Vec<(u32, SptValue)>>,
    /// 27000 "floor info".
    pub floor: Option<Vec<(u32, SptValue)>>,
    /// 28000 "leaf normal smoothing info".
    pub leaf_normal_smoothing: Option<Vec<(u32, SptValue)>>,
    /// 29000 "cluster info": the 29002 values.
    pub cluster: Option<Vec<i32>>,
    /// 30000 "standard shader support info".
    pub standard_shader: Option<Vec<(u32, SptValue)>>,
    /// 40000 "root support info".
    pub root: Option<RootSupport>,
    /// 50000 "tex coord controls": per branch level and the frond level, 7 layers each.
    pub texcoord_controls: Option<Vec<[SptValueList; 7]>>,
    /// 60000 "map bank".
    pub map_bank: Option<MapBank>,
    /// 71000 "mesh info".
    pub meshes: Option<MeshInfo>,
    /// 72000 "leaf mesh info": per leaf texture, the 7000..7001 values.
    pub leaf_meshes: Option<Vec<Vec<(u32, SptValue)>>>,
    /// 73000 "supplemental collision object info": per object, its (token, f32) values.
    pub collision_supplement: Option<Vec<Vec<(u32, f32)>>>,
    /// 74000 "supplemental global info": the 74002 values.
    pub global_supplement: Option<Vec<f32>>,
    /// 75000: (token, value) pairs.
    pub t75000: Option<Vec<(u32, SptValue)>>,
    /// Section tokens in file order.
    pub order: Vec<u32>,
}

/// Section 10000.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TexCoordInfo {
    /// 10002: 8 floats per entry (four `(u, v)` corners).
    pub t10002: Vec<[f32; 8]>,
    /// 10003: 8 floats per entry; the exe negates the odd ones (v) when a global flag is set.
    pub t10003: Vec<[f32; 8]>,
    /// 10004: 8 floats per entry.
    pub t10004: Vec<[f32; 8]>,
    /// 10005: string (+0x18).
    pub t10005: Option<String>,
    /// 10006 (+0x71), 10007 (+0x70): bools.
    pub t10006: Option<bool>,
    pub t10007: Option<bool>,
}

/// A collision object (12002 sphere, 12003 capsule, 12004 box; "unknown collision object type").
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionObject {
    /// 0 sphere, 1 capsule, 2 box (the exe's type numbers).
    pub kind: u32,
    /// Position as written (the exe stores `(-x, z, y)`).
    pub position: [f32; 3],
    /// 1, 2 or 3 dimensions.
    pub dims: Vec<f32>,
}

/// Section 13000 (`FUN_012c7c50`, "malformed frond info").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrondInfo {
    /// Scalar values: 13002 (+0x30), 13003 (+0x20), 13004 (+0x24), 13006 (+0x2c), 13009 (+0x44),
    /// 14007 (+0x58), 14008 (+0x5c) as i32; 13007 (+0x34) bool; 13010..13013 (+0x48..+0x54) f32.
    pub values: Vec<(u32, SptValue)>,
    /// 13005: a spline (+0x?? via `FUN_012c1710`).
    pub spline: Option<BezierSpline>,
    /// 13008: the frond textures (0x28-byte records at +0x38).
    pub textures: Vec<FrondTexture>,
}

/// One frond texture (14001 record; "malformed frond texture information").
#[derive(Debug, Clone, PartialEq)]
pub struct FrondTexture {
    /// The skipped i32 before the record.
    pub begin: i32,
    /// 14002: file name.
    pub texture: String,
    /// 14003..14006 (+0x18..+0x24): defaults 0.5, 1.0, 0, 0.
    pub values: [f32; 4],
}

impl Default for FrondTexture {
    fn default() -> Self {
        Self { begin: 0, texture: String::new(), values: [0.5, 1.0, 0.0, 0.0] }
    }
}

/// Section 40000.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RootSupport {
    /// 40002 i32, 40003..40005 f32.
    pub values: Vec<(u32, SptValue)>,
    /// 40006: the root branch level.
    pub level: Option<BranchLevel>,
}

/// Section 60000 (`FUN_012d21f0`, "malformed map bank").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapBank {
    /// 60002: branch maps.
    pub branch: Option<MapCollection>,
    /// 60003: leaf maps.
    pub leaves: Vec<MapCollection>,
    /// 60004: frond maps.
    pub fronds: Vec<MapCollection>,
    /// 60005: composite maps.
    pub composite: Option<MapCollection>,
    /// 60006: a string (+0x210).
    pub t60006: Option<String>,
    /// 60009: billboard maps (+0x168).
    pub billboard: Option<MapCollection>,
}

/// 70000..70001: up to 7 texture layers (70002..70008).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MapCollection {
    pub layers: [String; 7],
}

/// Section 71000 (`FUN_012cb330`, "malformed mesh info").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshInfo {
    /// 71001: reserve count.
    pub reserve: Option<i32>,
    pub meshes: Vec<SptMesh>,
}

/// One 71002 mesh.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SptMesh {
    /// 71003.
    pub name: String,
    /// 71004 / 71012: reserve counts.
    pub reserves: Vec<(u32, i32)>,
    /// 71005 vertices: 71006..71009 vec3s, 71010 two floats.
    pub vertices: Vec<MeshVertex>,
    /// 71013 indices.
    pub indices: Vec<i32>,
}

/// A 71005 vertex.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshVertex {
    pub v71006: [f32; 3],
    pub v71007: [f32; 3],
    pub v71008: [f32; 3],
    pub v71009: [f32; 3],
    pub uv: [f32; 2],
}

fn branch_level(c: &mut Cur) -> R<BranchLevel> {
    c.expect(1016, "malformed branch data")?;
    let mut l = BranchLevel::default();
    loop {
        let t = c.tok()?;
        match t {
            6000..=6007 => l.splines[(t - 6000) as usize] = c.spline()?,
            6008 => l.cross_sections = c.i32()?,
            6009 => l.segments = c.i32()?,
            6010 => l.t6010 = c.f32()?,
            6011 => l.t6011 = c.f32()?,
            6012 => l.t6012 = c.f32()?,
            6013 => l.t6013 = c.f32()?,
            6014 => l.t6014 = c.f32()?,
            6015 => l.t6015 = c.bool()?,
            6016 => l.t6016 = c.bool()?,
            6017 => l.spline_6017 = c.spline()?,
            1017 => return Ok(l),
            _ => return c.err(format!("malformed general branch information (token {t})")),
        }
    }
}

fn tree_info(c: &mut Cur) -> R<TreeInfo> {
    let mut g = TreeInfo::default();
    loop {
        let t = c.tok()?;
        match t {
            2000 => g.branch_texture = c.str()?,
            2001 => g.size = c.f32()?,
            2002 => g.t2002 = Some(c.bool()?),
            2003 => g.size_variance = c.f32()?,
            2004 => g.t2004 = Some(c.i32()?),
            2005 => g.seed = Some(c.i32()?),
            2006 => g.t2006 = c.f32()?,
            2007 => g.t2007 = c.f32()?,
            1014 => {
                let n = c.i32()?;
                for _ in 0..n.max(0) {
                    g.branch_levels.push(branch_level(c)?);
                }
                c.expect(1015, "malformed branch data")?;
            }
            1003 => return Ok(g),
            _ => return c.err(format!("malformed general tree information (token {t})")),
        }
    }
}

fn leaf_info(c: &mut Cur) -> R<LeafInfo> {
    let mut l = LeafInfo { t3007: None, ..Default::default() };
    loop {
        let t = c.tok()?;
        match t {
            3000 => l.t3000 = c.f32()?,
            3001 => l.t3001 = c.i32()?,
            3002 => l.t3002 = c.f32()?,
            3003 | 3006 => l.skipped_bools.push((t, c.bool()?)),
            3004 | 3005 => l.skipped_floats.push((t, c.f32()?)),
            3007 => l.t3007 = Some(c.f32()?),
            3008 => l.t3008 = c.i32()?,
            3009 => l.t3009 = c.bool()?,
            3010 => l.t3010 = c.f32()?,
            1009 => {
                let first = c.i32()?;
                let n = c.i32()?;
                l.textures.clear();
                for _ in 0..n.max(0) {
                    let mut lt = LeafTexture { begin: c.i32()?, ..Default::default() };
                    loop {
                        let t = c.tok()?;
                        match t {
                            4000 => lt.t4000 = c.bool()?,
                            4001 => lt.t4001 = c.vec3()?,
                            4002 => lt.t4002 = c.f32()?,
                            4003 => lt.texture = c.str()?,
                            4004 => lt.t4004 = c.vec3()?,
                            4005 => lt.t4005 = c.vec3()?,
                            4006 => lt.t4006 = c.vec3()?,
                            4007 => lt.t4007 = Some(c.f32()?),
                            1008 => break,
                            _ => return c.err(format!("malformed single leaf information (token {t})")),
                        }
                    }
                    l.textures.push(lt);
                }
                let after = c.i32()?;
                l.texture_list_tokens = Some([first, after]);
            }
            1005 => return Ok(l),
            _ => return c.err(format!("malformed general leaf information (token {t})")),
        }
    }
}

fn wind_info(c: &mut Cur) -> R<WindInfo> {
    let mut w = WindInfo::default();
    let mut t = c.tok()?;
    loop {
        match t {
            5000 | 5001 | 5003 => {
                let v = c.vec3()?;
                w.dropped.push((t, v));
            }
            5002 => w.t5002 = Some(c.vec3()?),
            5004 => w.t5004 = Some(c.vec3()?),
            5005 => w.t5005 = Some(c.f32()?),
            5006 => w.t5006 = Some(c.bool()?),
            _ => return c.err(format!("malformed general wind information (token {t})")),
        }
        t = c.tok()?;
        if t == 1012 {
            return Ok(w);
        }
    }
}

fn leaf_lods(c: &mut Cur) -> R<Vec<LeafLod>> {
    let count = c.i32()?;
    let mut lods = Vec::new();
    let mut t = c.tok()?;
    while t != 7001 {
        if lods.len() as i32 >= count {
            return c.err("too many leaf lod levels");
        }
        if t != 7002 {
            return c.err(format!("malformed leaf lod data (token {t})"));
        }
        let mut lod = LeafLod::default();
        t = c.tok()?;
        while t != 7003 {
            if t != 7004 {
                return c.err(format!("malformed leaf lod data (token {t})"));
            }
            let mut leaf = Vec::new();
            loop {
                let t = c.tok()?;
                let v = match t {
                    7005 => break,
                    7006 => {
                        let v = SptValue::I32(c.i32()?);
                        c.take(4)?; // the exe reads 8 bytes and keeps the low 4
                        v
                    }
                    7007 | 7008 | 7011 | 7012 => SptValue::I32(i32::from(c.u8()?)),
                    7010 | 7015 => SptValue::Vec3(c.vec3()?),
                    7013 => SptValue::I32(c.i32()?),
                    7016 => SptValue::F32(c.f32()?),
                    _ => return c.err(format!("malformed billboard leaf (token {t})")),
                };
                leaf.push((t, v));
            }
            lod.leaves.push(leaf);
            t = c.tok()?;
        }
        lods.push(lod);
        t = c.tok()?;
    }
    Ok(lods)
}

/// Reads `(token, value)` pairs until `end`, the payload type given by `kind`.
fn pairs(c: &mut Cur, end: u32, what: &str, kind: impl Fn(u32) -> Option<char>) -> R<Vec<(u32, SptValue)>> {
    let mut v = Vec::new();
    loop {
        let t = c.tok()?;
        if t == end {
            return Ok(v);
        }
        let val = match kind(t) {
            Some('i') => SptValue::I32(c.i32()?),
            Some('f') => SptValue::F32(c.f32()?),
            Some('b') => SptValue::Bool(c.bool()?),
            Some('s') => SptValue::Str(c.str()?),
            Some('v') => SptValue::Vec3(c.vec3()?),
            Some('p') => SptValue::Spline(Box::new(c.spline()?)),
            Some('-') => continue,
            _ => return c.err(format!("{what} (token {t})")),
        };
        v.push((t, val));
    }
}

fn supplemental_branch_kind(t: u32) -> Option<char> {
    match t {
        26004..=26007 | 26010 | 26011 | 26015..=26017 | 26021 => Some('f'),
        26008 | 26012 => Some('i'),
        26009 | 26023 => Some('b'),
        26013 | 26014 | 26018..=26020 | 26022 => Some('p'),
        _ => None,
    }
}

impl SptFile {
    /// Parses a whole `.spt` like `CSpeedTreeRT::LoadTree` does.
    pub fn read(bytes: &[u8]) -> Result<Self, SptError> {
        let c = &mut Cur { b: bytes, o: 0 };
        c.expect(1000, "missing begin_file token")?;
        if c.str()? != SPT_MAGIC {
            return c.err("not a valid SpeedTree SPT file");
        }
        let mut f = SptFile::default();
        loop {
            let t = c.tok()?;
            match t {
                1002 => f.tree = tree_info(c)?,
                1004 => f.leaves = leaf_info(c)?,
                1011 => f.wind = Some(wind_info(c)?),
                1001 => break,
                _ => return c.err(format!("malformed SpeedTree SPT file (token {t})")),
            }
        }
        if !c.at_end() && c.peek_tok() == Some(7000) {
            c.o += 4;
            f.leaf_lods = Some(leaf_lods(c)?);
        }
        let levels = f.tree.branch_levels.len();
        let x = &mut f.extra;
        while !c.at_end() {
            let t = c.tok()?;
            let known = Self::section(c, t, x, &mut f.tree.branch_levels, &f.leaves, levels)?;
            if !known {
                // The exe stops at an unknown section token.
                c.o -= 4;
                f.trailing = c.b.len() - c.o;
                break;
            }
            x.order.push(t);
        }
        Ok(f)
    }

    fn section(
        c: &mut Cur,
        t: u32,
        x: &mut ExtraSections,
        levels: &mut [BranchLevel],
        leaves: &LeafInfo,
        nlevels: usize,
    ) -> R<bool> {
        match t {
            8000 => {
                let mut v = Vec::new();
                loop {
                    let t = c.tok()?;
                    match t {
                        8001 => break,
                        8002 | 8004 | 8007 | 8008 => v.push((t, SptValue::I32(c.i32()?))),
                        8006 => v.push((t, SptValue::F32(c.f32()?))),
                        8003 | 8005 | 8009 => {
                            for _ in 0..13 {
                                v.push((t, SptValue::F32(c.f32()?)));
                            }
                        }
                        _ => return c.err(format!("malformed lighting information (token {t})")),
                    }
                }
                x.lighting = Some(v);
            }
            9000 => {
                let mut v = Vec::new();
                let mut t = c.tok()?;
                loop {
                    match t {
                        9002 => v.push((t, SptValue::I32(c.i32()?))),
                        9003 | 9004 => v.push((t, SptValue::F32(c.f32()?))),
                        9005 => {
                            let sub = pairs(c, 9006, "malformed engine lod data", |t| match t {
                                9007 | 9011 => Some('i'),
                                9008 | 9010 | 9012..=9014 => Some('f'),
                                _ => None,
                            })?;
                            v.push((9005, SptValue::I32(sub.len() as i32)));
                            v.extend(sub);
                        }
                        // 9009 and (with an error message) any other token: an f32.
                        _ => v.push((t, SptValue::F32(c.f32()?))),
                    }
                    if c.at_end() {
                        return c.err("premature end of file reached parsing new lod info");
                    }
                    t = c.tok()?;
                    if t == 9001 {
                        break;
                    }
                }
                x.lod = Some(v);
            }
            10000 => {
                let mut tc = TexCoordInfo::default();
                let mut t = c.tok()?;
                loop {
                    match t {
                        10002..=10004 => {
                            let n = c.i32()?;
                            let mut list = Vec::new();
                            for _ in 0..n.max(0) {
                                let mut e = [0f32; 8];
                                for v in &mut e {
                                    *v = c.f32()?;
                                }
                                list.push(e);
                            }
                            match t {
                                10002 => tc.t10002 = list,
                                10003 => tc.t10003 = list,
                                _ => tc.t10004 = list,
                            }
                        }
                        10005 => tc.t10005 = Some(c.str()?),
                        10006 => tc.t10006 = Some(c.bool()?),
                        10007 => tc.t10007 = Some(c.bool()?),
                        _ => return c.err(format!("malformed texture coord info (token {t})")),
                    }
                    if c.at_end() {
                        return c.err("premature end of file reached parsing texture coordinate info");
                    }
                    t = c.tok()?;
                    if t == 10001 {
                        break;
                    }
                }
                x.texcoords = Some(tc);
            }
            11000 => {
                let mut v = Vec::new();
                let mut t = c.tok()?;
                loop {
                    if t != 11002 {
                        return c.err(format!("malformed new wind info (token {t})"));
                    }
                    v.push(c.i32()?);
                    if c.at_end() {
                        return c.err("premature end of file reached parsing new wind info");
                    }
                    t = c.tok()?;
                    if t == 11001 {
                        break;
                    }
                }
                x.new_wind = Some(v);
            }
            12000 => {
                let mut v = Vec::new();
                let mut t = c.tok()?;
                loop {
                    let kind = match t {
                        12002 => 0,
                        12003 => 1,
                        12004 => 2,
                        _ => return c.err(format!("malformed collision object info (token {t})")),
                    };
                    let position = c.vec3()?;
                    let mut dims = Vec::new();
                    for _ in 0..=kind {
                        dims.push(c.f32()?);
                    }
                    v.push(CollisionObject { kind, position, dims });
                    if c.at_end() {
                        return c.err("premature end of file reached parsing collision object info");
                    }
                    t = c.tok()?;
                    if t == 12001 {
                        break;
                    }
                }
                x.collision = Some(v);
            }
            13000 => {
                let mut fr = FrondInfo::default();
                let mut t = c.tok()?;
                loop {
                    match t {
                        13002..=13004 | 13006 | 13009 | 14007 | 14008 => fr.values.push((t, SptValue::I32(c.i32()?))),
                        13007 => fr.values.push((t, SptValue::Bool(c.bool()?))),
                        13010..=13013 => fr.values.push((t, SptValue::F32(c.f32()?))),
                        13005 => fr.spline = Some(c.spline()?),
                        13008 => {
                            let n = c.i32()?;
                            fr.textures.clear();
                            for _ in 0..n.max(0) {
                                let mut ft = FrondTexture { begin: c.i32()?, ..Default::default() };
                                loop {
                                    let t = c.tok()?;
                                    match t {
                                        14001 => break,
                                        14002 => ft.texture = c.str()?,
                                        14003..=14006 => ft.values[(t - 14003) as usize] = c.f32()?,
                                        _ => return c.err(format!("malformed frond texture information (token {t})")),
                                    }
                                }
                                fr.textures.push(ft);
                            }
                        }
                        _ => return c.err(format!("malformed frond info (token {t})")),
                    }
                    t = c.tok()?;
                    if t == 13001 {
                        break;
                    }
                }
                x.fronds = Some(fr);
            }
            15000 => {
                for l in levels.iter_mut().take(nlevels) {
                    c.expect(15002, "malformed texture controls")?;
                    l.supplemental.t15002 = Some(c.bool()?);
                    c.expect(15003, "malformed texture controls")?;
                    l.supplemental.t15003 = Some(c.f32()?);
                }
                c.expect(15001, "malformed texture controls")?;
            }
            16000 => {
                for l in levels.iter_mut().take(nlevels) {
                    let mut fl = FlareInfo::default();
                    c.expect(16002, "malformed flare info")?;
                    fl.t16002 = c.f32()?;
                    c.expect(16003, "malformed flare info")?;
                    fl.t16003 = c.i32()?;
                    for k in 0..9 {
                        c.expect(16004 + k as u32, "malformed flare info")?;
                        fl.rest[k] = c.f32()?;
                    }
                    l.supplemental.flare = Some(fl);
                }
                c.expect(16001, "malformed flare info")?;
            }
            16013 => x.t16013 = Some(c.i32()?),
            16014 => x.t16014 = Some(c.f32()?),
            18000 => {
                x.t18000 = Some(pairs(c, 18001, "malformed frond info", |t| match t {
                    18002..=18004 => Some('v'),
                    18005 => Some('s'),
                    _ => None,
                })?);
            }
            19000 => {
                let mut v = Vec::new();
                loop {
                    let t = c.tok()?;
                    if t == 19001 {
                        break;
                    }
                    if t == 19002 {
                        v.push(c.str()?);
                    }
                }
                x.t19000 = Some(v);
            }
            20000 => {
                if x.texcoords.is_some() {
                    let mut v = Vec::new();
                    let mut t = c.tok()?;
                    while t != 20001 {
                        match t {
                            20002 => v.push((t, SptValue::Str(c.str()?))),
                            20003 | 20004 => v.push((t, SptValue::Bool(c.bool()?))),
                            20005 => {
                                for _ in 0..8 {
                                    v.push((t, SptValue::F32(c.f32()?)));
                                }
                            }
                            _ => return c.err(format!("malformed texture coord info (token {t})")),
                        }
                        t = c.tok()?;
                    }
                    x.t20000 = Some(v);
                }
            }
            21000 => x.t21000 = Some(c.f32()?),
            21001 => x.t21001 = Some(c.f32()?),
            22000 => x.t22000 = Some(c.bool()?),
            23000 => {
                for l in levels.iter_mut().take(nlevels) {
                    c.expect(23002, "malformed light seam reduction info")?;
                    let a = c.f32()?;
                    c.expect(23003, "malformed light seam reduction info")?;
                    l.supplemental.light_seam = Some([a, c.f32()?]);
                }
                c.expect(23001, "malformed light seam reduction info")?;
            }
            24000 => {
                c.expect(24002, "malformed supplemental leaf placement info")?;
                let a = c.f32()?;
                c.expect(24003, "malformed supplemental leaf placement info")?;
                let b = c.i32()?;
                c.expect(24001, "malformed supplemental leaf placement info")?;
                x.leaf_placement = Some((a, b));
            }
            25000 => {
                x.frond_supplement = Some(pairs(c, 25001, "malformed supplemental frond info", |t| match t {
                    25002 => Some('f'),
                    25003..=25006 => Some('i'),
                    25007 => Some('b'),
                    _ => None,
                })?);
            }
            26000 => {
                for l in levels.iter_mut().take(nlevels) {
                    c.expect(26002, "malformed supplemental branch info")?;
                    l.supplemental.values = pairs(c, 26003, "malformed supplemental branch info", supplemental_branch_kind)?;
                }
                c.expect(26001, "malformed supplemental branch info")?;
            }
            27000 => {
                x.floor = Some(pairs(c, 27001, "malformed floor info", |t| match t {
                    27002 => Some('b'),
                    27003 | 27005 | 27006 => Some('f'),
                    27004 => Some('i'),
                    _ => None,
                })?);
            }
            28000 => {
                x.leaf_normal_smoothing = Some(pairs(c, 28001, "malformed leaf normal smoothing info", |t| match t {
                    28002 => Some('b'),
                    28003 => Some('f'),
                    28004 => Some('i'),
                    _ => None,
                })?);
            }
            29000 => {
                let mut v = Vec::new();
                loop {
                    let t = c.tok()?;
                    match t {
                        29001 => break,
                        29002 => v.push(c.i32()?),
                        _ => return c.err(format!("malformed cluster info (token {t})")),
                    }
                }
                x.cluster = Some(v);
            }
            30000 => {
                let mut v = Vec::new();
                let mut t = c.tok()?;
                loop {
                    match t {
                        30002..=30009 => v.push((t, SptValue::F32(c.f32()?))),
                        _ => return c.err(format!("malformed standard shader support info (token {t})")),
                    }
                    if c.at_end() {
                        return c.err("premature end of file reached parsing standard shader support info");
                    }
                    t = c.tok()?;
                    if t == 30001 {
                        break;
                    }
                }
                x.standard_shader = Some(v);
            }
            40000 => {
                let mut r = RootSupport::default();
                loop {
                    let t = c.tok()?;
                    match t {
                        40001 => break,
                        40002 => r.values.push((t, SptValue::I32(c.i32()?))),
                        40003..=40005 => r.values.push((t, SptValue::F32(c.f32()?))),
                        40006 => r.level = Some(branch_level(c)?),
                        40007 => {
                            let vals = pairs(c, 40008, "malformed supplemental branch information", |t| match t {
                                15002 => Some('b'),
                                15003 | 16002 | 16004..=16012 | 23002 | 23003 => Some('f'),
                                16003 => Some('i'),
                                t => supplemental_branch_kind(t),
                            })?;
                            r.level.get_or_insert_with(BranchLevel::default).supplemental.values.extend(vals);
                        }
                        _ => return c.err(format!("malformed root support info (token {t})")),
                    }
                }
                x.root = Some(r);
            }
            50000 => {
                let mut all = Vec::new();
                for _ in 0..=nlevels {
                    let mut layers: [Vec<(u32, SptValue)>; 7] = Default::default();
                    for layer in &mut layers {
                        c.expect(50002, "malformed tex coord controls")?;
                        *layer = pairs(c, 50003, "malformed tex coord controls", |t| match t {
                            50004 | 50005 | 50008 | 50010 | 50013..=50017 => Some('f'),
                            50006 | 50007 | 50009 | 50011 | 50012 | 50018 => Some('b'),
                            _ => None,
                        })?;
                    }
                    all.push(layers);
                }
                c.expect(50001, "malformed tex coord controls")?;
                x.texcoord_controls = Some(all);
            }
            60000 => {
                let mut mb = MapBank::default();
                loop {
                    let t = c.tok()?;
                    match t {
                        60001 => break,
                        60002 => mb.branch = Some(map_collection(c)?),
                        60003 | 60004 => {
                            c.expect(t + 4, "malformed map bank (expected num maps)")?;
                            let n = c.i32()?;
                            let mut v = Vec::new();
                            for _ in 0..n.max(0) {
                                v.push(map_collection(c)?);
                            }
                            if t == 60003 { mb.leaves = v } else { mb.fronds = v }
                        }
                        60005 => mb.composite = Some(map_collection(c)?),
                        60006 => mb.t60006 = Some(c.str()?),
                        60009 => mb.billboard = Some(map_collection(c)?),
                        _ => return c.err(format!("malformed map bank (token {t})")),
                    }
                }
                x.map_bank = Some(mb);
            }
            71000 => x.meshes = Some(mesh_info(c)?),
            72000 => {
                let mut all = Vec::new();
                let mut t = c.tok()?;
                loop {
                    if t == 7000 {
                        all.push(pairs(c, 7001, "malformed leaf mesh info", |t| match t {
                            72001 => Some('b'),
                            72002 | 72003 => Some('i'),
                            _ => Some('f'),
                        })?);
                    }
                    t = c.tok()?;
                    if t == 72004 {
                        break;
                    }
                }
                let _ = leaves;
                x.leaf_meshes = Some(all);
            }
            73000 => {
                let mut all = Vec::new();
                if x.collision.is_some() {
                    let mut t = c.tok()?;
                    loop {
                        if t != 73002 {
                            return c.err(format!("malformed supplemental collision object info (token {t})"));
                        }
                        let mut v = Vec::new();
                        loop {
                            let t = c.tok()?;
                            match t {
                                73003 => break,
                                73004..=73006 => v.push((t, c.f32()?)),
                                _ => {}
                            }
                        }
                        all.push(v);
                        t = c.tok()?;
                        if t == 73001 {
                            break;
                        }
                    }
                }
                x.collision_supplement = Some(all);
            }
            74000 => {
                let mut v = Vec::new();
                loop {
                    let t = c.tok()?;
                    match t {
                        74001 => break,
                        74002 => v.push(c.f32()?),
                        _ => return c.err(format!("malformed supplemental global info (token {t})")),
                    }
                }
                x.global_supplement = Some(v);
            }
            75000 => {
                x.t75000 = Some(pairs(c, 75001, "", |t| match t {
                    75002 | 75003 | 75005 => Some('f'),
                    75004 => Some('b'),
                    _ => Some('-'),
                })?);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn map_collection(c: &mut Cur) -> R<MapCollection> {
    c.expect(70000, "malformed map collection")?;
    let mut m = MapCollection::default();
    loop {
        let t = c.tok()?;
        match t {
            70001 => return Ok(m),
            70002..=70008 => m.layers[(t - 70002) as usize] = c.str()?,
            _ => return c.err(format!("malformed map collection (token {t})")),
        }
    }
}

fn mesh_info(c: &mut Cur) -> R<MeshInfo> {
    let mut mi = MeshInfo::default();
    let mut t = c.tok()?;
    loop {
        match t {
            71001 => mi.reserve = Some(c.i32()?),
            71002 => {
                let mut m = SptMesh::default();
                loop {
                    let t = c.tok()?;
                    match t {
                        71014 => break,
                        71003 => m.name = c.str()?,
                        71004 | 71012 => m.reserves.push((t, c.i32()?)),
                        71013 => m.indices.push(c.i32()?),
                        71005 => {
                            let mut v = MeshVertex::default();
                            loop {
                                let t = c.tok()?;
                                match t {
                                    71011 => break,
                                    71006 => v.v71006 = c.vec3()?,
                                    71007 => v.v71007 = c.vec3()?,
                                    71008 => v.v71008 = c.vec3()?,
                                    71009 => v.v71009 = c.vec3()?,
                                    71010 => v.uv = [c.f32()?, c.f32()?],
                                    _ => return c.err(format!("malformed mesh info (token {t})")),
                                }
                            }
                            m.vertices.push(v);
                        }
                        _ => return c.err(format!("malformed mesh info (token {t})")),
                    }
                }
                mi.meshes.push(m);
            }
            _ => return c.err(format!("malformed mesh info (token {t})")),
        }
        t = c.tok()?;
        if t == 71015 {
            return Ok(mi);
        }
    }
}

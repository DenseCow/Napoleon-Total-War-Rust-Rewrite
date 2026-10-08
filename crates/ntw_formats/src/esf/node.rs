//! The in-memory ESF tree: [`EsfNode`], [`EsfRecord`], [`EsfRecordArray`],
//! plus helpers for finding things in it.

/// The one-byte type codes that start every node (W3 §2.2).
///
/// A packed array of primitive type `t` uses the code `ARRAY_FLAG | t`, e.g. `0x44` is `i32[]`.
pub mod codes {
    /// `bool`: one byte, 0 or 1.
    pub const BOOL: u8 = 0x01;
    /// `i8`.
    pub const I8: u8 = 0x02;
    /// `i16`.
    pub const I16: u8 = 0x03;
    /// `i32`.
    pub const I32: u8 = 0x04;
    /// `i64`.
    pub const I64: u8 = 0x05;
    /// `u8`.
    pub const U8: u8 = 0x06;
    /// `u16`.
    pub const U16: u8 = 0x07;
    /// `u32`.
    pub const U32: u8 = 0x08;
    /// `u64`.
    pub const U64: u8 = 0x09;
    /// `f32`.
    pub const F32: u8 = 0x0A;
    /// `f64`.
    pub const F64: u8 = 0x0B;
    /// Two `f32`s.
    pub const COORD2D: u8 = 0x0C;
    /// Three `f32`s.
    pub const COORD3D: u8 = 0x0D;
    /// u16 character count, then UTF-16LE units.
    pub const UTF16: u8 = 0x0E;
    /// u16 byte count, then single-byte characters.
    pub const ASCII: u8 = 0x0F;
    /// u16 angle.
    pub const ANGLE: u8 = 0x10;
    /// Added to a primitive code to make a packed-array code.
    pub const ARRAY_FLAG: u8 = 0x40;
    /// A record: name, version, end offset, children.
    pub const RECORD: u8 = 0x80;
    /// A record array: name, version, end offset, count, items.
    pub const RECORD_ARRAY: u8 = 0x81;
}

/// One node of an ESF tree.
///
/// An ESF file is a tree. Its leaves are plain values (numbers, strings, packed
/// arrays) and its branches are *records*: named groups of child nodes.
/// The fields of a record have **no names**. Only records are named, so you
/// find a value by its *position* among the record's children.
///
/// The variant names follow the type codes in [`codes`]. Each `...Array` variant is
/// a packed array of that primitive (type code `0x40 + t`).
#[derive(Debug, Clone, PartialEq)]
pub enum EsfNode {
    /// 0x01.
    Bool(bool),
    /// 0x02.
    I8(i8),
    /// 0x03.
    I16(i16),
    /// 0x04. Map positions are often stored this way as fixed-point; see [`Fixed20`].
    I32(i32),
    /// 0x05.
    I64(i64),
    /// 0x06.
    U8(u8),
    /// 0x07.
    U16(u16),
    /// 0x08.
    U32(u32),
    /// 0x09.
    U64(u64),
    /// 0x0A.
    F32(f32),
    /// 0x0B.
    F64(f64),
    /// 0x0C: a 2D point. On the campaign map this is (x, z): east and north.
    Coord2d(f32, f32),
    /// 0x0D: a 3D point (x, y = height, z).
    Coord3d(f32, f32, f32),
    /// 0x0E: a UTF-16 string (most text in startpos and saves).
    Utf16String(String),
    /// 0x0F: a single-byte string. Each byte is one character (Latin-1), so it round-trips exactly.
    AsciiString(String),
    /// 0x10: an angle stored as u16 (INFERRED: 65536 = one full turn).
    Angle(u16),

    /// 0x41.
    BoolArray(Vec<bool>),
    /// 0x42.
    I8Array(Vec<i8>),
    /// 0x43.
    I16Array(Vec<i16>),
    /// 0x44.
    I32Array(Vec<i32>),
    /// 0x45 (never seen in shipped files).
    I64Array(Vec<i64>),
    /// 0x46: often raw images or grids.
    U8Array(Vec<u8>),
    /// 0x47.
    U16Array(Vec<u16>),
    /// 0x48.
    U32Array(Vec<u32>),
    /// 0x49 (never seen).
    U64Array(Vec<u64>),
    /// 0x4A.
    F32Array(Vec<f32>),
    /// 0x4B (never seen).
    F64Array(Vec<f64>),
    /// 0x4C.
    Coord2dArray(Vec<(f32, f32)>),
    /// 0x4D.
    Coord3dArray(Vec<(f32, f32, f32)>),
    /// 0x4E (never seen): repeated length-prefixed UTF-16 strings.
    Utf16Array(Vec<String>),
    /// 0x4F (never seen): repeated length-prefixed single-byte strings.
    AsciiArray(Vec<String>),
    /// 0x50 (never seen).
    AngleArray(Vec<u16>),

    /// 0x80: a named record with children. Boxed to keep `EsfNode` small.
    Record(Box<EsfRecord>),
    /// 0x81: a named list of items, where each item is a list of children.
    RecordArray(Box<EsfRecordArray>),
}

/// A record (type 0x80): a name, a version byte and an ordered list of children.
///
/// The `version` matters: the same record name can have a different layout in a
/// different version (e.g. `CHARACTER` is v12 in startpos files and v14 in 1.3
/// saves). Code that reads fields must check it.
#[derive(Debug, Clone, PartialEq)]
pub struct EsfRecord {
    /// The record's name, e.g. `"CAMPAIGN_MODEL"`.
    pub name: String,
    /// The per-record layout version.
    pub version: u8,
    /// Child nodes, in file order.
    pub children: Vec<EsfNode>,
}

/// A record array (type 0x81): a name, a version byte and a list of items.
///
/// Each item is its own list of child nodes. In practice an item usually holds
/// a single record, e.g. every item of `FACTION_ARRAY` contains one `FACTION`.
#[derive(Debug, Clone, PartialEq)]
pub struct EsfRecordArray {
    /// The array's name, e.g. `"FACTION_ARRAY"`.
    pub name: String,
    /// The per-record layout version.
    pub version: u8,
    /// The items, in file order. Each item is a list of child nodes.
    pub items: Vec<Vec<EsfNode>>,
}

/// What a path given to [`EsfRecord::lookup`] points at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EsfPathTarget<'a> {
    /// A record.
    Record(&'a EsfRecord),
    /// A whole record array.
    RecordArray(&'a EsfRecordArray),
    /// One item of a record array (a list of child nodes).
    Item(&'a [EsfNode]),
}

/// A map coordinate stored as an `i32` with 20 fractional bits (W3 §2.4, CONFIRMED).
///
/// The world value is `raw / 2^20`. For example Paris's settlement x is stored as
/// `-222517056`, which is `-212.2088` map units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed20(pub i32);

impl Fixed20 {
    /// `2^20`: one map unit in raw fixed-point steps.
    pub const ONE: i32 = 1 << 20;

    /// Convert to a float in map units.
    pub fn to_f32(self) -> f32 {
        (f64::from(self.0) / f64::from(Self::ONE)) as f32
    }

    /// Convert to a double in map units (exact: every `Fixed20` fits in an `f64`).
    pub fn to_f64(self) -> f64 {
        f64::from(self.0) / f64::from(Self::ONE)
    }

    /// Build from map units, rounding to the nearest step and saturating at the `i32` range.
    pub fn from_f64(units: f64) -> Self {
        // `as` from f64 to i32 saturates and maps NaN to 0, so this never panics.
        Self((units * f64::from(Self::ONE)).round() as i32)
    }
}

impl From<i32> for Fixed20 {
    fn from(raw: i32) -> Self {
        Self(raw)
    }
}

impl EsfNode {
    /// The type code this node is written with (see [`codes`]).
    pub fn type_code(&self) -> u8 {
        use codes::*;
        match self {
            Self::Bool(_) => BOOL,
            Self::I8(_) => I8,
            Self::I16(_) => I16,
            Self::I32(_) => I32,
            Self::I64(_) => I64,
            Self::U8(_) => U8,
            Self::U16(_) => U16,
            Self::U32(_) => U32,
            Self::U64(_) => U64,
            Self::F32(_) => F32,
            Self::F64(_) => F64,
            Self::Coord2d(..) => COORD2D,
            Self::Coord3d(..) => COORD3D,
            Self::Utf16String(_) => UTF16,
            Self::AsciiString(_) => ASCII,
            Self::Angle(_) => ANGLE,
            Self::BoolArray(_) => ARRAY_FLAG | BOOL,
            Self::I8Array(_) => ARRAY_FLAG | I8,
            Self::I16Array(_) => ARRAY_FLAG | I16,
            Self::I32Array(_) => ARRAY_FLAG | I32,
            Self::I64Array(_) => ARRAY_FLAG | I64,
            Self::U8Array(_) => ARRAY_FLAG | U8,
            Self::U16Array(_) => ARRAY_FLAG | U16,
            Self::U32Array(_) => ARRAY_FLAG | U32,
            Self::U64Array(_) => ARRAY_FLAG | U64,
            Self::F32Array(_) => ARRAY_FLAG | F32,
            Self::F64Array(_) => ARRAY_FLAG | F64,
            Self::Coord2dArray(_) => ARRAY_FLAG | COORD2D,
            Self::Coord3dArray(_) => ARRAY_FLAG | COORD3D,
            Self::Utf16Array(_) => ARRAY_FLAG | UTF16,
            Self::AsciiArray(_) => ARRAY_FLAG | ASCII,
            Self::AngleArray(_) => ARRAY_FLAG | ANGLE,
            Self::Record(_) => RECORD,
            Self::RecordArray(_) => RECORD_ARRAY,
        }
    }

    /// A short type name for debugging, e.g. `"u32"`, `"i32[]"`, `"record"`.
    pub fn type_name(&self) -> &'static str {
        type_name(self.type_code())
    }

    /// The value if this is a `Bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// The value if this is an `I32`.
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Self::I32(v) => Some(*v),
            _ => None,
        }
    }

    /// The value if this is a `U32`.
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::U32(v) => Some(*v),
            _ => None,
        }
    }

    /// The value if this is an `F32`.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// The value as a fixed-point map coordinate if this is an `I32`.
    pub fn as_fixed20(&self) -> Option<Fixed20> {
        self.as_i32().map(Fixed20)
    }

    /// Any integer node widened to `i64` (bool, signed, unsigned and angle; not u64 above i64::MAX).
    pub fn as_int(&self) -> Option<i64> {
        Some(match self {
            Self::Bool(v) => i64::from(*v),
            Self::I8(v) => i64::from(*v),
            Self::I16(v) => i64::from(*v),
            Self::I32(v) => i64::from(*v),
            Self::I64(v) => *v,
            Self::U8(v) => i64::from(*v),
            Self::U16(v) | Self::Angle(v) => i64::from(*v),
            Self::U32(v) => i64::from(*v),
            Self::U64(v) => i64::try_from(*v).ok()?,
            _ => return None,
        })
    }

    /// The text if this is a UTF-16 or single-byte string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Utf16String(s) | Self::AsciiString(s) => Some(s),
            _ => None,
        }
    }

    /// The record if this node is one.
    pub fn as_record(&self) -> Option<&EsfRecord> {
        match self {
            Self::Record(r) => Some(r),
            _ => None,
        }
    }

    /// The record array if this node is one.
    pub fn as_record_array(&self) -> Option<&EsfRecordArray> {
        match self {
            Self::RecordArray(r) => Some(r),
            _ => None,
        }
    }

    /// The elements if this is a `U32Array`.
    pub fn as_u32_array(&self) -> Option<&[u32]> {
        match self {
            Self::U32Array(v) => Some(v),
            _ => None,
        }
    }

    /// The elements if this is an `I32Array`.
    pub fn as_i32_array(&self) -> Option<&[i32]> {
        match self {
            Self::I32Array(v) => Some(v),
            _ => None,
        }
    }

    /// The elements if this is a `U8Array`.
    pub fn as_u8_array(&self) -> Option<&[u8]> {
        match self {
            Self::U8Array(v) => Some(v),
            _ => None,
        }
    }

    /// The elements if this is an `F32Array`.
    pub fn as_f32_array(&self) -> Option<&[f32]> {
        match self {
            Self::F32Array(v) => Some(v),
            _ => None,
        }
    }

    /// The elements if this is a `Coord2dArray`.
    pub fn as_coord2d_array(&self) -> Option<&[(f32, f32)]> {
        match self {
            Self::Coord2dArray(v) => Some(v),
            _ => None,
        }
    }

    /// The name if this node is a record or a record array.
    pub fn record_name(&self) -> Option<&str> {
        match self {
            Self::Record(r) => Some(&r.name),
            Self::RecordArray(r) => Some(&r.name),
            _ => None,
        }
    }
}

/// A short type name for a type code, e.g. `type_name(0x44) == "i32[]"`.
pub fn type_name(code: u8) -> &'static str {
    match code {
        0x01 => "bool",
        0x02 => "i8",
        0x03 => "i16",
        0x04 => "i32",
        0x05 => "i64",
        0x06 => "u8",
        0x07 => "u16",
        0x08 => "u32",
        0x09 => "u64",
        0x0A => "f32",
        0x0B => "f64",
        0x0C => "coord2d",
        0x0D => "coord3d",
        0x0E => "utf16",
        0x0F => "ascii",
        0x10 => "angle",
        0x41 => "bool[]",
        0x42 => "i8[]",
        0x43 => "i16[]",
        0x44 => "i32[]",
        0x45 => "i64[]",
        0x46 => "u8[]",
        0x47 => "u16[]",
        0x48 => "u32[]",
        0x49 => "u64[]",
        0x4A => "f32[]",
        0x4B => "f64[]",
        0x4C => "coord2d[]",
        0x4D => "coord3d[]",
        0x4E => "utf16[]",
        0x4F => "ascii[]",
        0x50 => "angle[]",
        0x80 => "record",
        0x81 => "record_array",
        _ => "unknown",
    }
}

/// Splits `"NAME[3]"` into `("NAME", Some(3))` and `"NAME"` into `("NAME", None)`.
fn parse_segment(seg: &str) -> Option<(&str, Option<usize>)> {
    match seg.strip_suffix(']') {
        Some(rest) => {
            let (name, idx) = rest.split_once('[')?;
            Some((name, Some(idx.parse().ok()?)))
        }
        None => Some((seg, None)),
    }
}

/// Resolves one path segment inside a list of child nodes.
fn step<'a>(children: &'a [EsfNode], seg: &str) -> Option<EsfPathTarget<'a>> {
    let (name, index) = parse_segment(seg)?;
    match index {
        None => children.iter().find_map(|n| match n {
            EsfNode::Record(r) if r.name == name => Some(EsfPathTarget::Record(r)),
            EsfNode::RecordArray(a) if a.name == name => Some(EsfPathTarget::RecordArray(a)),
            _ => None,
        }),
        Some(i) => {
            // Prefer "item i of the record array NAME"; fall back to "the i-th record named NAME".
            let array = children.iter().find_map(|n| match n {
                EsfNode::RecordArray(a) if a.name == name => Some(a),
                _ => None,
            });
            match array {
                Some(a) => a.items.get(i).map(|item| EsfPathTarget::Item(item)),
                None => children
                    .iter()
                    .filter_map(EsfNode::as_record)
                    .filter(|r| r.name == name)
                    .nth(i)
                    .map(EsfPathTarget::Record),
            }
        }
    }
}

impl<'a> EsfPathTarget<'a> {
    /// The child list to search for the next path segment.
    fn children(self) -> Option<&'a [EsfNode]> {
        match self {
            Self::Record(r) => Some(&r.children),
            Self::Item(i) => Some(i),
            Self::RecordArray(_) => None,
        }
    }
}

impl EsfRecord {
    /// Make a record with no children.
    pub fn new(name: impl Into<String>, version: u8) -> Self {
        Self { name: name.into(), version, children: Vec::new() }
    }

    /// The child at `index`, whatever its type.
    pub fn get(&self, index: usize) -> Option<&EsfNode> {
        self.children.get(index)
    }

    /// The first child *record* with this name.
    pub fn child(&self, name: &str) -> Option<&EsfRecord> {
        self.children.iter().filter_map(EsfNode::as_record).find(|r| r.name == name)
    }

    /// All child records with this name, in order.
    pub fn children_named<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s EsfRecord> + 's {
        self.children.iter().filter_map(EsfNode::as_record).filter(move |r| r.name == name)
    }

    /// The first child *record array* with this name.
    pub fn record_array(&self, name: &str) -> Option<&EsfRecordArray> {
        self.children.iter().filter_map(EsfNode::as_record_array).find(|r| r.name == name)
    }

    /// The child values that are not records or record arrays, in order.
    ///
    /// Handy because fields are positional: `values().nth(2)` is "the third plain value".
    pub fn values(&self) -> impl Iterator<Item = &EsfNode> {
        self.children
            .iter()
            .filter(|n| !matches!(n, EsfNode::Record(_) | EsfNode::RecordArray(_)))
    }

    /// The `i32` child at `index`.
    pub fn get_i32(&self, index: usize) -> Option<i32> {
        self.get(index)?.as_i32()
    }

    /// The `u32` child at `index`.
    pub fn get_u32(&self, index: usize) -> Option<u32> {
        self.get(index)?.as_u32()
    }

    /// The `f32` child at `index`.
    pub fn get_f32(&self, index: usize) -> Option<f32> {
        self.get(index)?.as_f32()
    }

    /// The `bool` child at `index`.
    pub fn get_bool(&self, index: usize) -> Option<bool> {
        self.get(index)?.as_bool()
    }

    /// The string child at `index` (UTF-16 or single-byte).
    pub fn get_str(&self, index: usize) -> Option<&str> {
        self.get(index)?.as_str()
    }

    /// The `i32` child at `index`, read as a fixed-point map coordinate.
    pub fn get_fixed20(&self, index: usize) -> Option<Fixed20> {
        self.get(index)?.as_fixed20()
    }

    /// Follows a slash-separated path of record names, starting *below* this record.
    ///
    /// Each segment is either `NAME` (the first child record or record array with that name)
    /// or `NAME[i]`: item `i` of the record array `NAME`, or if there is no such array,
    /// the `i`-th child record named `NAME`.
    ///
    /// Example, on a startpos root: `"CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY[0]/FACTION"`.
    pub fn lookup(&self, path: &str) -> Option<EsfPathTarget<'_>> {
        let mut current = EsfPathTarget::Record(self);
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            current = step(current.children()?, seg)?;
        }
        Some(current)
    }

    /// Like [`lookup`](Self::lookup), but only succeeds if the path ends at a record.
    pub fn find_path(&self, path: &str) -> Option<&EsfRecord> {
        match self.lookup(path)? {
            EsfPathTarget::Record(r) => Some(r),
            _ => None,
        }
    }

    /// Like [`lookup`](Self::lookup), but only succeeds if the path ends at a record array.
    pub fn find_record_array(&self, path: &str) -> Option<&EsfRecordArray> {
        match self.lookup(path)? {
            EsfPathTarget::RecordArray(r) => Some(r),
            _ => None,
        }
    }

    /// Like [`lookup`](Self::lookup), but only succeeds if the path ends at a record-array item (`NAME[i]`).
    pub fn find_item(&self, path: &str) -> Option<&[EsfNode]> {
        match self.lookup(path)? {
            EsfPathTarget::Item(i) => Some(i),
            _ => None,
        }
    }

    /// Calls `f` on this record and every record below it, depth first, in file order.
    /// Records inside record-array items are visited too.
    pub fn walk<'s>(&'s self, f: &mut impl FnMut(&'s EsfRecord)) {
        f(self);
        walk_children(&self.children, f);
    }
}

fn walk_children<'s>(children: &'s [EsfNode], f: &mut impl FnMut(&'s EsfRecord)) {
    for node in children {
        match node {
            EsfNode::Record(r) => r.walk(f),
            EsfNode::RecordArray(a) => {
                for item in &a.items {
                    walk_children(item, f);
                }
            }
            _ => {}
        }
    }
}

impl EsfRecordArray {
    /// Make a record array with no items.
    pub fn new(name: impl Into<String>, version: u8) -> Self {
        Self { name: name.into(), version, items: Vec::new() }
    }

    /// For the common "one record per item" shape: the record in each item, in order.
    /// Items whose first child is not a record are skipped.
    pub fn records(&self) -> impl Iterator<Item = &EsfRecord> {
        self.items.iter().filter_map(|item| item.first()?.as_record())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> EsfRecord {
        let mut faction = EsfRecord::new("FACTION", 18);
        faction.children.push(EsfNode::I32(749_327_284));
        faction.children.push(EsfNode::Utf16String("france".into()));
        let mut arr = EsfRecordArray::new("FACTION_ARRAY", 0);
        arr.items.push(vec![EsfNode::Record(Box::new(faction))]);
        let mut world = EsfRecord::new("WORLD", 1);
        world.children.push(EsfNode::RecordArray(Box::new(arr)));
        let mut root = EsfRecord::new("ROOT", 0);
        root.children.push(EsfNode::Bool(true));
        root.children.push(EsfNode::Record(Box::new(world)));
        root
    }

    #[test]
    fn path_lookup() {
        let root = sample();
        assert_eq!(root.find_path("WORLD").unwrap().version, 1);
        assert!(root.find_record_array("WORLD/FACTION_ARRAY").is_some());
        assert_eq!(root.find_item("WORLD/FACTION_ARRAY[0]").unwrap().len(), 1);
        let f = root.find_path("WORLD/FACTION_ARRAY[0]/FACTION").unwrap();
        assert_eq!(f.get_i32(0), Some(749_327_284));
        assert_eq!(f.get_str(1), Some("france"));
        assert!(root.find_path("WORLD/FACTION_ARRAY[1]/FACTION").is_none());
        assert!(root.find_path("NOPE").is_none());
        assert!(root.find_path("WORLD/FACTION_ARRAY[x]").is_none());
        // "" resolves to the record itself.
        assert_eq!(root.find_path("").unwrap().name, "ROOT");
    }

    #[test]
    fn walk_visits_all_records() {
        let mut names = Vec::new();
        sample().walk(&mut |r| names.push(r.name.clone()));
        assert_eq!(names, ["ROOT", "WORLD", "FACTION"]);
    }

    #[test]
    fn fixed20_conversion() {
        let paris_x = Fixed20(-222_517_056);
        assert!((paris_x.to_f32() - -212.2088).abs() < 1e-3);
        assert_eq!(Fixed20(Fixed20::ONE).to_f32(), 1.0);
        assert_eq!(Fixed20::from_f64(-410.0), Fixed20(-429_916_160));
        assert_eq!(Fixed20::from_f64(f64::NAN), Fixed20(0));
    }

    #[test]
    fn type_codes_and_getters() {
        assert_eq!(EsfNode::I32Array(vec![]).type_code(), 0x44);
        assert_eq!(EsfNode::I32Array(vec![]).type_name(), "i32[]");
        assert_eq!(EsfNode::U16(7).as_int(), Some(7));
        assert_eq!(EsfNode::U64(u64::MAX).as_int(), None);
        assert_eq!(EsfNode::I32(5).as_fixed20(), Some(Fixed20(5)));
        assert_eq!(EsfNode::AsciiString("land".into()).as_str(), Some("land"));
        assert_eq!(EsfNode::F32(1.0).as_i32(), None);
    }
}

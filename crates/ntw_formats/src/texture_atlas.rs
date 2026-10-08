//! Texture-atlas lists: `*.tai` (Atlas Creation Tool), a **plain-text** index from an image
//! name to a rectangle inside one of the atlas pages.
//!
//! CONFIRMED by the file itself: every shipped `.tai` opens with the tool's own header
//! (`AtlasCreationTool.exe -width 2048 -height 2048 -o flags`) and documents its own schema.
//! `rigidmodels\flags\textures\flags.tai` (6841 bytes) reads:
//!
//! ```text
//! # <filename>, <atlas filename>, <atlas idx>, <atlas type>, <woffset>,
//! #   <hoffset>, <depth offset>, <width>, <height>
//! # ... A = (<woffset>, <hoffset>); B = A + (<width>, <height>)
//! flag_austria.tga   flags0.dds, 0, 2D, 0.250000, 0.000000, 0.000000, 0.250000, 0.125000
//! ```
//!
//! (the real file separates the name from the fields with two tab characters.)
//!
//! so **A and B are the texture coordinates of the image's top-left and bottom-right corners in
//! the atlas** (the header says so in as many words), and `atlas type` is `2D` for a plain atlas
//! and a volume type for a 3D one (only then is `<depth offset>` the w coordinate).
//!
//! Used for the flag atlas: `standard_bearer_flag.logic` names the **prefix** `flag_` and the exe
//! completes it with the flag key, looking the result up here and falling back to
//! `flag_default.tga` (CONFIRMED in the exe at `0x01227BD0`).

use std::collections::BTreeMap;
use std::fmt;

/// One image's rectangle in an atlas page. `a` and `b` are its texture coordinates,
/// the two corners the header names.
#[derive(Debug, Clone, PartialEq)]
pub struct TaiEntry {
    /// The atlas page, as written (`flags0.dds`).
    pub page: String,
    /// `<atlas idx>`: the page's index when the atlas is a set.
    pub page_index: u32,
    /// `<atlas type>` (`2D`, or a volume type in a 3D atlas).
    pub kind: String,
    /// `A = (woffset, hoffset)`, the image's first corner.
    pub a: [f32; 2],
    /// `B = A + (width, height)`, the image's other corner.
    pub b: [f32; 2],
    /// `<depth offset>`: the w coordinate of a volume atlas, 0 for a `2D` one.
    pub depth_offset: f32,
    /// The width and height as written, before `B` is derived.
    pub width: f32,
    pub height: f32,
}

impl TaiEntry {
    /// The rectangle as `(min, max)` per axis, from `A` and `B` in either order.
    pub fn rect(&self) -> ([f32; 2], [f32; 2]) {
        let lo = [self.a[0].min(self.b[0]), self.a[1].min(self.b[1])];
        let hi = [self.a[0].max(self.b[0]), self.a[1].max(self.b[1])];
        (lo, hi)
    }
}

/// A whole `.tai` file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TaiAtlas {
    /// Every distinct page file, in first-seen order.
    pub pages: Vec<String>,
    /// Image name (as written, e.g. `flag_france.tga`) -> its rectangle.
    pub entries: BTreeMap<String, TaiEntry>,
}

impl TaiAtlas {
    /// The entry for `name`, matched case-insensitively (the header's names are lower case but
    /// the `.logic` prefix is built from a DB key that is not always).
    pub fn find(&self, name: &str) -> Option<&TaiEntry> {
        self.entries.get(name).or_else(|| {
            self.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
        })
    }

    /// Parse a `.tai` file's text.
    pub fn read(text: &str) -> Result<Self, TaiError> {
        let mut out = Self::default();
        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            // The file is DOS text with CRLF and a UTF-8 BOM.
            let l = raw.trim().trim_start_matches('\u{feff}').trim_end_matches('\u{feff}').trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            // `<filename>` then two tabs then the comma-separated fields.
            let (name, rest) = l
                .split_once('\t')
                .ok_or_else(|| TaiError::Fields { line, text: l.to_owned() })?;
            let name = name.trim();
            let f: Vec<&str> = rest.split(',').map(str::trim).collect();
            if f.len() < 8 {
                return Err(TaiError::Fields { line, text: l.to_owned() });
            }
            let num = |k: usize| -> Result<f32, TaiError> {
                f.get(k)
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| TaiError::Number { line, text: l.to_owned() })
            };
            let entry = TaiEntry {
                page: f[0].to_owned(),
                page_index: f[1]
                    .parse()
                    .map_err(|_| TaiError::Number { line, text: l.to_owned() })?,
                kind: f[2].to_owned(),
                a: [num(3)?, num(4)?],
                width: num(6)?,
                height: num(7)?,
                b: [num(3)? + num(6)?, num(4)? + num(7)?],
                depth_offset: num(5)?,
            };
            if !out.pages.iter().any(|p| p == &entry.page) {
                out.pages.push(entry.page.clone());
            }
            out.entries.insert(name.to_owned(), entry);
        }
        Ok(out)
    }
}

/// Why a `.tai` file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaiError {
    /// A line that is not `<filename><TAB>eight comma-separated fields`.
    Fields { line: usize, text: String },
    /// A field that is not a number.
    Number { line: usize, text: String },
}

impl fmt::Display for TaiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fields { line, text } => write!(f, "line {line}: not a filename and 8 fields: {text:?}"),
            Self::Number { line, text } => write!(f, "line {line}: bad number in {text:?}"),
        }
    }
}

impl std::error::Error for TaiError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header and three entries of the shipped `flags.tai`, verbatim.
    const SHIPPED: &str = "# \x20flags.tai\r\n\
# \x20AtlasCreationTool.exe -width 2048 -height 2048 -o flags\r\n\
#\r\n\
# \x20<filename>\t\t<atlas filename>, <atlas idx>, <atlas type>, <woffset>, <hoffset>, <depth offset>, <width>, <height>\r\n\
\r\n\
flag_austria.tga\t\tflags0.dds, 0, 2D, 0.250000, 0.000000, 0.000000, 0.250000, 0.125000\r\n\
flag_france.tga\t\tflags0.dds, 0, 2D, 0.500000, 0.000000, 0.000000, 0.250000, 0.125000\r\n\
flag_default.tga\t\tflags0.dds, 0, 2D, 0.250000, 0.125000, 0.000000, 0.250000, 0.125000\r\n";

    #[test]
    fn a_shipped_entry_reads_as_its_two_corners() {
        let a = TaiAtlas::read(SHIPPED).expect("parse");
        assert_eq!(a.pages, vec!["flags0.dds".to_string()]);
        assert_eq!(a.entries.len(), 3);
        let f = a.find("flag_france.tga").expect("france");
        assert_eq!(f.page, "flags0.dds");
        assert_eq!(f.page_index, 0);
        assert_eq!(f.kind, "2D");
        assert_eq!(f.depth_offset, 0.0);
        // A = (woffset, hoffset); B = A + (width, height).
        assert_eq!(f.a, [0.5, 0.0]);
        assert_eq!(f.b, [0.75, 0.125]);
        assert_eq!(f.rect(), ([0.5, 0.0], [0.75, 0.125]));
        // The default flag sits on the row below the French one.
        let d = a.find("flag_default.tga").expect("default");
        assert_eq!(d.a, [0.25, 0.125]);
    }

    #[test]
    fn a_volume_atlas_keeps_its_depth_offset() {
        let vol = "flag_default.tga\t\tnaval_id0.dds, 1, 3D, 0.0, 0.0, 0.25, 1.0, 1.0\n";
        let a = TaiAtlas::read(vol).expect("parse");
        let d = a.find("flag_default.tga").expect("entry");
        assert_eq!(d.kind, "3D");
        assert_eq!(d.depth_offset, 0.25);
        assert_eq!(d.b, [1.0, 1.0]);
    }

    #[test]
    fn a_short_line_is_an_error() {
        assert!(matches!(TaiAtlas::read("flag_x.tga\t\tflags0.dds, 0, 2D\n"), Err(TaiError::Fields { line: 1, .. })));
        let bad = "flag_x.tga\t\tflags0.dds, 0, 2D, a, 0, 0, 1, 1\n";
        assert!(matches!(TaiAtlas::read(bad), Err(TaiError::Number { line: 1, .. })));
    }

    #[test]
    fn names_match_case_insensitively() {
        let a = TaiAtlas::read(SHIPPED).unwrap();
        assert_eq!(a.find("FLAG_FRANCE.TGA").expect("case").a, [0.5, 0.0]);
        assert!(a.find("flag_spain.tga").is_none());
    }
}
//! `.cuf` bitmap fonts (`font\<family>_<size>.cuf` in `local_<lang>.pack`) and `ui\fontcategories.fc`.
//!
//! Spec and evidence: `analysis/frontend/FONT_FORMAT.md`. The layout of a `.cuf` file is CONFIRMED
//! (all 74 shipped fonts parse to their last byte, and the glyph pixel offsets add up); the meaning
//! of some header values and the exact pen-advance rule are INFERRED (see each field).
//!
//! ```text
//! "CUF0"
//! 12 × i16   header (see `CufFont::header`)            [11] = glyph count
//! u32        pixel data size in bytes
//! 65536 × u16  character → glyph index (0xFFFF = no glyph)
//! glyph count × 4 bytes  glyph metrics: i8 top, u8 advance, u8 width, u8 height
//! glyph count × u32      offset of each glyph's pixels inside the pixel data
//! pixel data             8-bit coverage (alpha), width × height per glyph, row-major
//! u16 n, u16 first       pair table for characters first .. first+n
//! n × n bytes            pen advance for (left, right) character pairs (kerning)
//! ```

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// Metrics of one glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CufGlyph {
    /// Pixels from the baseline up to the glyph's top row (INFERRED; -128 = empty glyph).
    pub top: i8,
    /// Default pen advance in pixels (INFERRED: width + 1 for most glyphs; space uses header[7]).
    pub advance: u8,
    /// Bitmap width.
    pub width: u8,
    /// Bitmap height.
    pub height: u8,
    /// Offset of the bitmap inside [`CufFont::pixels`].
    pub offset: u32,
}

/// A parsed `.cuf` font.
#[derive(Debug, Clone, PartialEq)]
pub struct CufFont {
    /// The 12 header values. INFERRED meanings: `[0]` line height, `[1]` ascent (baseline from the top),
    /// `[2]` and `[9]` nominal size, `[5]` cap height, `[6]` descent (negative), `[7]` space advance,
    /// `[10]` UNKNOWN, `[11]` glyph count (CONFIRMED), `[3]`, `[4]`, `[8]` UNKNOWN.
    pub header: [i16; 12],
    /// Character (UTF-16 code unit) → glyph index; `u16::MAX` = none.
    char_map: Vec<u16>,
    /// One entry per glyph.
    pub glyphs: Vec<CufGlyph>,
    /// All glyph bitmaps (8-bit coverage).
    pub pixels: Vec<u8>,
    /// First character of the pair table.
    pub pair_first: u16,
    /// Side length of the pair table.
    pub pair_count: u16,
    pairs: Vec<u8>,
}

/// Errors from [`CufFont::read`] and [`FontCategories::read`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// Wrong magic / header.
    BadMagic,
    /// Data ended early.
    UnexpectedEof {
        /// Where.
        offset: usize,
        /// Bytes needed.
        needed: usize,
    },
    /// Bytes remain after the last table.
    TrailingBytes {
        /// Where they start.
        offset: usize,
    },
    /// A glyph's pixels lie outside the pixel data.
    BadGlyph {
        /// Glyph index.
        index: usize,
    },
}

impl From<ReadError> for FontError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::UnexpectedEof { offset, needed: 0 },
        }
    }
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "bad font magic"),
            Self::UnexpectedEof { offset, needed } => write!(f, "font data ended at 0x{offset:x} (needed {needed})"),
            Self::TrailingBytes { offset } => write!(f, "unexpected bytes at 0x{offset:x}"),
            Self::BadGlyph { index } => write!(f, "glyph {index} lies outside the pixel data"),
        }
    }
}

impl std::error::Error for FontError {}

impl CufFont {
    /// Parses a whole `.cuf` file.
    pub fn read(bytes: &[u8]) -> Result<Self, FontError> {
        let mut c = Cursor::new(bytes);
        if c.take(4)? != b"CUF0" {
            return Err(FontError::BadMagic);
        }
        let mut header = [0i16; 12];
        for h in &mut header {
            *h = c.u16()? as i16;
        }
        let n = header[11] as u16 as usize;
        let pix_len = c.u32()? as usize;
        let mut char_map = Vec::with_capacity(65536);
        for _ in 0..65536 {
            char_map.push(c.u16()?);
        }
        let mut glyphs = Vec::with_capacity(n);
        for _ in 0..n {
            let b = c.take(4)?;
            glyphs.push(CufGlyph { top: b[0] as i8, advance: b[1], width: b[2], height: b[3], offset: 0 });
        }
        for g in &mut glyphs {
            g.offset = c.u32()?;
        }
        let pixels = c.take(pix_len)?.to_vec();
        for (i, g) in glyphs.iter().enumerate() {
            let end = g.offset as usize + usize::from(g.width) * usize::from(g.height);
            if end > pixels.len() {
                return Err(FontError::BadGlyph { index: i });
            }
        }
        let pair_count = c.u16()?;
        let pair_first = c.u16()?;
        let pairs = c.take(usize::from(pair_count) * usize::from(pair_count))?.to_vec();
        if c.remaining() != 0 {
            return Err(FontError::TrailingBytes { offset: c.pos() });
        }
        Ok(Self { header, char_map, glyphs, pixels, pair_first, pair_count, pairs })
    }

    /// Line height in pixels (header[0], INFERRED).
    pub fn line_height(&self) -> i32 {
        i32::from(self.header[0])
    }

    /// Distance from the top of a line to the baseline (header[1], INFERRED).
    pub fn ascent(&self) -> i32 {
        i32::from(self.header[1])
    }

    /// The glyph for a character, if the font has one.
    pub fn glyph(&self, ch: char) -> Option<(usize, &CufGlyph)> {
        let code = u32::from(ch);
        if code > 0xFFFF {
            return None;
        }
        let g = *self.char_map.get(code as usize)?;
        if g == u16::MAX {
            return None;
        }
        self.glyphs.get(usize::from(g)).map(|x| (usize::from(g), x))
    }

    /// The bitmap (width × height coverage bytes) of a glyph.
    pub fn bitmap(&self, glyph: &CufGlyph) -> &[u8] {
        let start = glyph.offset as usize;
        &self.pixels[start..start + usize::from(glyph.width) * usize::from(glyph.height)]
    }

    /// The pair-table value for `left` followed by `right`, if both are inside the table.
    pub fn pair(&self, left: char, right: char) -> Option<u8> {
        let (l, r) = (u32::from(left), u32::from(right));
        let first = u32::from(self.pair_first);
        let n = u32::from(self.pair_count);
        if l < first || r < first || l >= first + n || r >= first + n {
            return None;
        }
        Some(self.pairs[((l - first) * n + (r - first)) as usize])
    }

    /// Pen advance after `ch` when followed by `next`.
    ///
    /// PROVISIONAL rule (not yet confirmed in the exe): use the pair table value + 1 when both
    /// characters are in it (this reproduces `advance` = pair + 1 for unkerned pairs), otherwise
    /// the glyph's own `advance`; a missing glyph advances by the space width (header[7]).
    pub fn advance(&self, ch: char, next: Option<char>) -> i32 {
        if let Some(p) = next.and_then(|n| self.pair(ch, n)) {
            return i32::from(p) + 1;
        }
        match self.glyph(ch) {
            Some((_, g)) => i32::from(g.advance),
            None => i32::from(self.header[7]),
        }
    }

    /// Width in pixels of one line of text (no wrapping).
    pub fn text_width(&self, text: &str) -> i32 {
        let chars: Vec<char> = text.chars().collect();
        (0..chars.len()).map(|i| self.advance(chars[i], chars.get(i + 1).copied())).sum()
    }

    /// One line of `text` drawn as glyph coverage: `(width, height, bytes)`, one byte per pixel,
    /// row-major, no colour and no shadow. The box is `text_width + 1` wide and
    /// `max(line_height, ascent + 4) + 1` high, the same geometry as the battle unit labels'
    /// rasteriser, so a caller can add its own outline inside it. Glyphs are placed with
    /// [`Self::advance`] (PROVISIONAL kerning rule) and their top row at `ascent - top`.
    pub fn coverage(&self, text: &str) -> (u32, u32, Vec<u8>) {
        let line_h = self.line_height().max(self.ascent() + 4).max(1);
        let w = (self.text_width(text) + 1).max(1) as usize;
        let h = (line_h + 1).max(1) as usize;
        let mut cov = vec![0u8; w * h];
        let chars: Vec<char> = text.chars().collect();
        let mut pen = 0i32;
        for (i, &ch) in chars.iter().enumerate() {
            if let Some((_, g)) = self.glyph(ch)
                && g.top != i8::MIN
            {
                let bmp = self.bitmap(g);
                let top = self.ascent() - i32::from(g.top);
                for gy in 0..i32::from(g.height) {
                    for gx in 0..i32::from(g.width) {
                        let (x, y) = (pen + gx, top + gy);
                        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                            let o = y as usize * w + x as usize;
                            cov[o] = cov[o].max(bmp[(gy * i32::from(g.width) + gx) as usize]);
                        }
                    }
                }
            }
            pen += self.advance(ch, chars.get(i + 1).copied());
        }
        (w as u32, h as u32, cov)
    }

    /// Splits `text` into lines no wider than `max_width` pixels: breaks at `\n` and between
    /// words (a word wider than the line stays on its own line). `max_width <= 0` = no wrapping.
    /// PROVISIONAL: the original's text layout (tracking, leading, hyphenation) is UNKNOWN; this is
    /// the plain greedy rule.
    pub fn wrap_lines(&self, text: &str, max_width: i32) -> Vec<String> {
        let mut out = Vec::new();
        for para in text.split('\n') {
            let para = para.strip_suffix('\r').unwrap_or(para);
            if max_width <= 0 {
                out.push(para.to_owned());
                continue;
            }
            let mut line = String::new();
            for word in para.split(' ') {
                let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
                if !line.is_empty() && self.text_width(&candidate) > max_width {
                    out.push(std::mem::take(&mut line));
                    line = word.to_owned();
                } else {
                    line = candidate;
                }
            }
            out.push(line);
        }
        out
    }
}

/// Maps a layout font name such as `"Frontend 22, Normal"` or `"Ingame 14, Bold"` to its file,
/// `font/frontend_22.cuf` / `font/ingame_b_14.cuf`. INFERRED from the shipped file names.
pub fn font_file_for(name: &str) -> Option<String> {
    let (left, style) = name.split_once(',').unwrap_or((name, "Normal"));
    let mut it = left.split_whitespace();
    let family = it.next()?.to_ascii_lowercase();
    let size: u32 = it.next()?.parse().ok()?;
    let bold = style.trim().eq_ignore_ascii_case("bold");
    Some(if bold { format!("font/{family}_b_{size}.cuf") } else { format!("font/{family}_{size}.cuf") })
}

/// One entry of `ui\fontcategories.fc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontCategory {
    /// Category name, e.g. `"sp_napoleon_battles.twui 2"` (layout + number).
    pub name: String,
    /// Category index.
    pub index: u32,
    /// Font name, e.g. `"Ingame 12, Normal"`.
    pub font: String,
    /// INFERRED leading (same position as a layout state's font_leading).
    pub leading: u32,
    /// INFERRED tracking.
    pub tracking: u32,
    /// Colour (ARGB).
    pub colour: u32,
}

/// `ui\fontcategories.fc`: `"Version044"` then records until the end of the file
/// (str name, u32 index, str font, u32 leading, u32 tracking, u32 colour). CONFIRMED by parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontCategories {
    /// Header version (44).
    pub version: u32,
    /// The entries in file order.
    pub entries: Vec<FontCategory>,
}

impl FontCategories {
    /// Parses the whole file.
    pub fn read(bytes: &[u8]) -> Result<Self, FontError> {
        if bytes.len() < 10 || &bytes[..7] != b"Version" {
            return Err(FontError::BadMagic);
        }
        let version = std::str::from_utf8(&bytes[7..10]).ok().and_then(|s| s.parse().ok()).ok_or(FontError::BadMagic)?;
        let mut c = Cursor::new(bytes);
        c.take(10)?;
        let mut entries = Vec::new();
        while c.remaining() > 0 {
            entries.push(FontCategory {
                name: c.ascii()?,
                index: c.u32()?,
                font: c.ascii()?,
                leading: c.u32()?,
                tracking: c.u32()?,
                colour: c.u32()?,
            });
        }
        Ok(Self { version, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_font() -> Vec<u8> {
        let mut b = b"CUF0".to_vec();
        let header: [i16; 12] = [10, 8, 10, 0, 0, 7, -2, 3, 1, 10, 12, 2];
        for h in header {
            b.extend_from_slice(&h.to_le_bytes());
        }
        b.extend_from_slice(&6u32.to_le_bytes()); // pixels: glyph0 2x2, glyph1 1x2
        for c in 0..65536u32 {
            let g: u16 = match c {
                0x41 => 0,
                0x42 => 1,
                _ => u16::MAX,
            };
            b.extend_from_slice(&g.to_le_bytes());
        }
        b.extend_from_slice(&[8, 3, 2, 2, 8, 2, 1, 2]);
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&[255, 0, 0, 255, 128, 128]);
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&0x41u16.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3, 4]);
        b
    }

    #[test]
    fn parses_tiny_font() {
        let f = CufFont::read(&tiny_font()).unwrap();
        assert_eq!(f.glyphs.len(), 2);
        let (_, g) = f.glyph('B').unwrap();
        assert_eq!(f.bitmap(g), &[128, 128]);
        assert_eq!(f.pair('A', 'B'), Some(2));
        assert_eq!(f.advance('A', Some('B')), 3);
        assert_eq!(f.advance('B', None), 2);
        assert_eq!(f.advance('Z', None), 3);
        assert!(f.glyph('C').is_none());
        let mut bad = tiny_font();
        bad.push(0);
        assert!(CufFont::read(&bad).is_err());
    }

    #[test]
    fn coverage_places_glyphs_at_the_pen() {
        let f = CufFont::read(&tiny_font()).unwrap();
        // "AB": A advances by the pair value + 1 = 3, B by its own advance 2, so the box is 5 + 1
        // wide; the height is max(line 10, ascent 8 + 4) + 1.
        let (w, h, cov) = f.coverage("AB");
        assert_eq!((w, h), (6, 13));
        assert_eq!(cov.len(), 6 * 13);
        // A's 2x2 diagonal at the pen's start, its top row at ascent - top = 0.
        assert_eq!((cov[0], cov[1], cov[6], cov[7]), (255, 0, 0, 255));
        // B's 1x2 column at x = 3.
        assert_eq!((cov[3], cov[6 + 3]), (128, 128));
        // Nothing else is covered, and an unknown glyph only advances the pen.
        assert_eq!(cov.iter().filter(|&&c| c > 0).count(), 4);
        let (w, _, cov) = f.coverage("Z");
        assert_eq!(w, 4);
        assert!(cov.iter().all(|&c| c == 0));
    }

    #[test]
    fn font_names_map_to_files() {
        assert_eq!(font_file_for("Frontend 22, Normal").unwrap(), "font/frontend_22.cuf");
        assert_eq!(font_file_for("Ingame 14, Bold").unwrap(), "font/ingame_b_14.cuf");
    }

    #[test]
    fn parses_font_categories() {
        let mut b = b"Version044".to_vec();
        for (s, n) in [("a.twui 1", 1u32)] {
            b.extend_from_slice(&(s.len() as u16).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
            b.extend_from_slice(&n.to_le_bytes());
            b.extend_from_slice(&7u16.to_le_bytes());
            b.extend_from_slice(b"Ingame ");
            for x in [2u32, 1, 0xFF00_0000] {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
        let fc = FontCategories::read(&b).unwrap();
        assert_eq!(fc.version, 44);
        assert_eq!(fc.entries[0].colour, 0xFF00_0000);
    }
}

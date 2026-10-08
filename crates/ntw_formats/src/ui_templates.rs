//! The UIEd template library `data/UI/Templates/uied.templates` (a loose file).
//!
//! The engine reads it at UI start-up (the exe builds the path `data/UI/templates/` +
//! `uied.templates` in `FUN_00DA6A60`, CONFIRMED string) and `Component.CreateComponentFromTemplate`
//! builds components from it when `ui/templates/<name>` is not a layout file (INFERRED: e.g. the
//! campaign HUD's `ReviewPanelTab` exists only here).
//!
//! Layout (CONFIRMED by parsing all entries; field meanings INFERRED unless said):
//! - u32 entry count (126);
//! - each entry: a 256-byte name block (NUL-terminated ASCII, the rest is editor memory: 0xFD fill,
//!   and in newer entries u32 `1`, u32 layout version at +248/+252), u32 payload size, u32 the
//!   entry's own absolute file offset (CONFIRMED on all 126 entries: 4 for the first, then each
//!   previous offset + 264 + size; INFERRED: written by the editor as an index),
//!   then the payload:
//!   - u32 embedded image count, each {u16 length + ASCII path (the artist's original file path),
//!     the image file itself: a TGA whose length follows from its header};
//!   - one component tree in the layout format of the entry's version (`ui_layout::read_component`).
//! - Entries without a version at +252 (only `InputWindow`, the oldest: layout v1/v2, which read
//!   the same) are read with the first
//!   version that reads the component to exactly the payload end (INFERRED).

use crate::ui_layout::{UiComponent, read_template_component};

/// One template.
#[derive(Debug, Clone)]
pub struct UiTemplate {
    /// Template name, e.g. `ReviewPanelTab`.
    pub name: String,
    /// The entry's absolute offset in the file (the u32 after the payload size).
    pub offset: u32,
    /// Layout version of its component data.
    pub version: u32,
    /// Embedded image files: (path as stored in the component's images, file bytes).
    pub images: Vec<(String, Vec<u8>)>,
    /// The component tree.
    pub component: UiComponent,
}

/// The whole library.
#[derive(Debug, Clone, Default)]
pub struct UiTemplateLibrary {
    /// Templates in file order. Names may repeat (the engine's choice between duplicates is
    /// UNKNOWN; [`get`](Self::get) returns the last, as a later definition would replace an
    /// earlier one in a name map).
    pub templates: Vec<UiTemplate>,
    /// Entries whose component data could not be read (their framing is still skipped
    /// correctly). Empty for the shipped file since the v<6 layout fix (all 126 read).
    pub unreadable: Vec<String>,
}

/// Why the library could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTemplateError(pub String);

impl std::fmt::Display for UiTemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "uied.templates: {}", self.0)
    }
}

impl std::error::Error for UiTemplateError {}

fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    b.get(p..p + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

fn u16_at(b: &[u8], p: usize) -> Option<u16> {
    b.get(p..p + 2).map(|s| u16::from_le_bytes(s.try_into().unwrap()))
}

/// The byte length of a TGA file at the start of `b`, from its header (uncompressed, or RLE
/// walked packet by packet). No footer is stored (CONFIRMED: the next field follows directly).
pub fn tga_len(b: &[u8]) -> Option<usize> {
    let id_len = *b.first()? as usize;
    let cmap_type = *b.get(1)?;
    let image_type = *b.get(2)?;
    let cmap_len = u16_at(b, 5)? as usize;
    let cmap_bits = *b.get(7)? as usize;
    let (w, h) = (u16_at(b, 12)? as usize, u16_at(b, 14)? as usize);
    let px = (*b.get(16)? as usize).div_ceil(8);
    let mut pos = 18 + id_len;
    if cmap_type == 1 {
        pos += cmap_len * cmap_bits.div_ceil(8);
    }
    let pixels = w * h;
    if image_type < 9 {
        pos += pixels * px;
    } else {
        let mut done = 0;
        while done < pixels {
            let head = *b.get(pos)? as usize;
            pos += 1;
            let n = (head & 0x7F) + 1;
            pos += if head & 0x80 != 0 { px } else { n * px };
            done += n;
        }
    }
    (pos <= b.len()).then_some(pos)
}

impl UiTemplateLibrary {
    /// Parses the whole file.
    pub fn read(b: &[u8]) -> Result<Self, UiTemplateError> {
        let err = UiTemplateError;
        let count = u32_at(b, 0).ok_or_else(|| err("truncated".into()))? as usize;
        let mut p = 4;
        let mut templates = Vec::with_capacity(count);
        let mut unreadable = Vec::new();
        for i in 0..count {
            let block = b.get(p..p + 256).ok_or_else(|| err(format!("entry {i}: truncated name")))?;
            let n = block.iter().position(|&c| c == 0).unwrap_or(256);
            let name = String::from_utf8_lossy(&block[..n]).into_owned();
            let size = u32_at(b, p + 256).ok_or_else(|| err(format!("{name}: truncated")))? as usize;
            let stored = u32_at(b, p + 252).unwrap_or(0);
            let start = p + 264;
            // `size` counts the payload from the offset u32's end (CONFIRMED on ReviewPanelTab:
            // 30711 bytes up to the next entry's name).
            let end = start + size;
            let payload = b.get(start..end).ok_or_else(|| err(format!("{name}: payload past the end")))?;
            let mut q = 0;
            let images_n = u32_at(payload, q).ok_or_else(|| err(format!("{name}: no image count")))? as usize;
            q += 4;
            let mut images = Vec::with_capacity(images_n);
            for _ in 0..images_n {
                let len = u16_at(payload, q).ok_or_else(|| err(format!("{name}: image path")))? as usize;
                let path_bytes = payload.get(q + 2..q + 2 + len).ok_or_else(|| err(format!("{name}: image path")))?;
                let path = String::from_utf8_lossy(path_bytes).into_owned();
                q += 2 + len;
                let tl = tga_len(&payload[q..]).ok_or_else(|| err(format!("{name}: image {path} unreadable")))?;
                images.push((path, payload[q..q + tl].to_vec()));
                q += tl;
            }
            let rest = &payload[q..];
            let fits = |v: u32| read_template_component(rest, v).ok().filter(|(_, used, _)| *used == rest.len()).map(|(c, _, imgs)| (c, imgs));
            let found = if (1..=99).contains(&stored) { fits(stored).map(|c| (stored, c)) } else { None };
            match found.or_else(|| (1..=39).find_map(|v| fits(v).map(|c| (v, c)))) {
                Some((version, (component, more))) => {
                    images.extend(more);
                    templates.push(UiTemplate { name, offset: u32_at(b, p + 260).unwrap_or(0), version, images, component })
                }
                None => unreadable.push(name),
            }
            p = end;
        }
        Ok(Self { templates, unreadable })
    }

    /// A template by name (case-insensitive), the last one if the name repeats.
    pub fn get(&self, name: &str) -> Option<&UiTemplate> {
        self.templates.iter().rev().find(|t| t.name.eq_ignore_ascii_case(name))
    }
}

//! `.tga` (Truevision TARGA) images: a small decoder for the variants the game ships (battle
//! terrain maps and the UI art in `uirontend uiskins*.tga`).
//!
//! TGA is a public format (Truevision TGA 2.0 specification). battleterrain.pack holds 764 of
//! them (ground-type maps, grass maps, radar images, tile colour maps) and data.pack holds the
//! battlefield template masks. Variants seen in the shipped files (CONFIRMED from the headers):
//!
//! | image type | meaning | example |
//! |---|---|---|
//! | 1 | colour-mapped (palette), 8-bit indices | `ground_type_map_0.tga` (25-entry palette) |
//! | 2 | true colour, 24/32-bit BGR(A) | `grassmap.tga`, `loading_screen_radar.tga` |
//! | 3 | greyscale, 8-bit | masks |
//! | 9 / 10 / 11 | run-length encoded versions of 1 / 2 / 3 | |
//!
//! # Header (18 bytes, little-endian)
//! ```text
//! 0 u8 id length        1 u8 colour-map type   2 u8 image type
//! 3 u16 cmap first      5 u16 cmap length      7 u8 cmap entry bits
//! 8 u16 x origin        10 u16 y origin        12 u16 width   14 u16 height
//! 16 u8 bits per pixel  17 u8 descriptor (bit 5 set = rows stored top-down; bits 0-3 = alpha bits)
//! then: id field, colour map, pixel data
//! ```
//! This decoder always returns rows **top-down** (row 0 = the top of the picture) and, for
//! palette images, keeps the raw indices as well (the ground-type map's index IS the data).

use std::fmt;

/// A decoded TGA image.
#[derive(Debug, Clone, PartialEq)]
pub struct Tga {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8 pixels, row 0 = top of the picture.
    pub rgba: Vec<u8>,
    /// For colour-mapped (type 1/9) and greyscale (3/11) images: the raw 8-bit value of each
    /// pixel (palette index or grey level), top-down like `rgba`. Empty for true-colour images.
    pub indices: Vec<u8>,
    /// The palette as RGBA8 (colour-mapped images only).
    pub palette: Vec<[u8; 4]>,
}

/// Why a TGA could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TgaError {
    /// Shorter than its header, colour map or pixel data says.
    Truncated,
    /// An image type or bit depth this decoder does not handle.
    Unsupported(String),
}

impl fmt::Display for TgaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "TGA data is truncated"),
            Self::Unsupported(s) => write!(f, "unsupported TGA: {s}"),
        }
    }
}

impl std::error::Error for TgaError {}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// Converts one stored pixel (`bytes`, little-endian BGR(A) or 16-bit 5-5-5) to RGBA.
fn to_rgba(bytes: &[u8], alpha_bits: u8) -> [u8; 4] {
    match bytes.len() {
        1 => [bytes[0], bytes[0], bytes[0], 255],
        2 => {
            let v = u16::from_le_bytes([bytes[0], bytes[1]]);
            let c = |s: u16| (((v >> s) & 31) as u32 * 255 / 31) as u8;
            let a = if alpha_bits > 0 && v & 0x8000 == 0 { 0 } else { 255 };
            [c(10), c(5), c(0), a]
        }
        3 => [bytes[2], bytes[1], bytes[0], 255],
        _ => [bytes[2], bytes[1], bytes[0], if alpha_bits > 0 { bytes[3] } else { 255 }],
    }
}

impl Tga {
    /// Same as [`Tga::parse`] (the name the frontend code uses).
    pub fn decode(b: &[u8]) -> Result<Self, TgaError> {
        Self::parse(b)
    }

    /// Decodes a whole TGA file.
    pub fn parse(b: &[u8]) -> Result<Self, TgaError> {
        if b.len() < 18 {
            return Err(TgaError::Truncated);
        }
        let id_len = b[0] as usize;
        let cmap_type = b[1];
        let image_type = b[2];
        let (cmap_first, cmap_len, cmap_bits) = (u16_at(b, 3) as usize, u16_at(b, 5) as usize, b[7]);
        let (width, height) = (u16_at(b, 12) as u32, u16_at(b, 14) as u32);
        let bpp = b[16];
        let descriptor = b[17];
        let alpha_bits = descriptor & 0x0F;
        let top_down = descriptor & 0x20 != 0;
        let rle = image_type >= 9;
        let base_type = if rle { image_type - 8 } else { image_type };
        if !matches!(base_type, 1..=3) {
            return Err(TgaError::Unsupported(format!("image type {image_type}")));
        }
        if !matches!(bpp, 8 | 15 | 16 | 24 | 32) || (base_type != 2 && bpp != 8) {
            return Err(TgaError::Unsupported(format!("{bpp} bits per pixel, type {image_type}")));
        }
        let mut pos = 18 + id_len;
        // Colour map (present whenever cmap_type == 1, even if the image does not use it).
        let mut palette = Vec::new();
        if cmap_type == 1 {
            let entry = (cmap_bits as usize).div_ceil(8);
            let size = cmap_len * entry;
            let raw = b.get(pos..pos + size).ok_or(TgaError::Truncated)?;
            palette = vec![[0, 0, 0, 255]; cmap_first];
            palette.extend(raw.chunks_exact(entry).map(|e| to_rgba(e, if cmap_bits == 32 { 8 } else { 0 })));
            pos += size;
        }
        let px_bytes = (bpp as usize).div_ceil(8);
        let count = width as usize * height as usize;
        // Raw stored pixels in file order, `px_bytes` each.
        let stored: Vec<u8> = if rle {
            let mut out = Vec::with_capacity(count * px_bytes);
            while out.len() < count * px_bytes {
                let head = *b.get(pos).ok_or(TgaError::Truncated)?;
                pos += 1;
                let n = (head & 0x7F) as usize + 1;
                if head & 0x80 != 0 {
                    let px = b.get(pos..pos + px_bytes).ok_or(TgaError::Truncated)?;
                    for _ in 0..n {
                        out.extend_from_slice(px);
                    }
                    pos += px_bytes;
                } else {
                    out.extend_from_slice(b.get(pos..pos + n * px_bytes).ok_or(TgaError::Truncated)?);
                    pos += n * px_bytes;
                }
            }
            out.truncate(count * px_bytes);
            out
        } else {
            b.get(pos..pos + count * px_bytes).ok_or(TgaError::Truncated)?.to_vec()
        };
        let (w, h) = (width as usize, height as usize);
        let mut rgba = vec![0u8; count * 4];
        let mut indices = if base_type == 2 { Vec::new() } else { vec![0u8; count] };
        for file_row in 0..h {
            let row = if top_down { file_row } else { h - 1 - file_row };
            for x in 0..w {
                let src = &stored[(file_row * w + x) * px_bytes..][..px_bytes];
                let dst = row * w + x;
                let colour = match base_type {
                    1 => {
                        indices[dst] = src[0];
                        palette.get(src[0] as usize).copied().unwrap_or([0, 0, 0, 255])
                    }
                    3 => {
                        indices[dst] = src[0];
                        to_rgba(src, 0)
                    }
                    _ => to_rgba(src, alpha_bits),
                };
                rgba[dst * 4..dst * 4 + 4].copy_from_slice(&colour);
            }
        }
        Ok(Self { width, height, rgba, indices, palette })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // one argument per TGA header field
    fn header(cmap: u8, kind: u8, cmap_len: u16, cmap_bits: u8, w: u16, h: u16, bpp: u8, desc: u8) -> Vec<u8> {
        let mut v = vec![0, cmap, kind, 0, 0];
        v.extend(cmap_len.to_le_bytes());
        v.push(cmap_bits);
        v.extend([0, 0, 0, 0]);
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.extend([bpp, desc]);
        v
    }

    #[test]
    fn palette_bottom_up_is_flipped() {
        // 2x2, palette of 2 BGR entries, rows stored bottom-up.
        let mut b = header(1, 1, 2, 24, 2, 2, 8, 0);
        b.extend([0, 0, 255, /* red */ 255, 0, 0 /* blue */]);
        b.extend([0, 0, /* bottom row */ 1, 1 /* top row */]);
        let t = Tga::parse(&b).unwrap();
        assert_eq!(t.indices, vec![1, 1, 0, 0]);
        assert_eq!(&t.rgba[0..4], &[0, 0, 255, 255]);
        assert_eq!(&t.rgba[8..12], &[255, 0, 0, 255]);
    }

    #[test]
    fn rle_true_colour_top_down() {
        let mut b = header(0, 10, 0, 0, 3, 1, 32, 0x28);
        b.extend([0x81, 1, 2, 3, 4]); // run of 2 pixels BGRA (1,2,3,4)
        b.extend([0x00, 9, 8, 7, 6]); // 1 raw pixel
        let t = Tga::parse(&b).unwrap();
        assert_eq!(t.rgba, vec![3, 2, 1, 4, 3, 2, 1, 4, 7, 8, 9, 6]);
        assert!(t.indices.is_empty());
    }

    #[test]
    fn truncated_is_an_error() {
        let b = header(0, 2, 0, 0, 4, 4, 32, 0x20);
        assert_eq!(Tga::parse(&b), Err(TgaError::Truncated));
    }
}

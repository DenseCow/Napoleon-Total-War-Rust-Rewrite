//! `.dds` (DirectDraw Surface) textures: header parsing and a small CPU decoder.
//!
//! Napoleon's textures are almost all DXT1/DXT3/DXT5 (block-compressed) with full
//! mip chains. A survey of all 8,239 `.dds` in the install's packs (2026-10, see
//! `analysis/worker1/GRAPHICS_EXE.md` §5) found:
//!
//! | format | files | used for |
//! |---|---|---|
//! | DXT5 | 4,850 | normal maps, diffuse with alpha |
//! | DXT1 | 2,406 | most diffuse and gloss maps |
//! | DXT3 (incl. 1 DXT2) | 146 | some alpha-tested diffuse |
//! | 32-bit BGRA/BGRX | 101 | UI, BRDF lookup tables |
//! | 16-bit (R5G6B5, L16) | 708 | colour masks, battle heightmaps |
//! | 24-bit BGR, 8-bit L/A | 26 | UI, jitter, flare |
//!
//! [`Dds::decode_rgba8`] turns any of these into plain RGBA bytes, so a renderer
//! does not need GPU block-compression support. Cube maps and volume textures are
//! read as their first face / slice only. 2 files fail (fourCC 36 and 63, a test
//! file and one UI icon); no model uses them.
//!
//! # Header (Microsoft's documented `DDS_HEADER`, CONFIRMED on the shipped files)
//! ```text
//! 0x00 "DDS "            0x0C u32 height      0x10 u32 width
//! 0x1C u32 mip count     0x4C pixel format (32 bytes): 0x50 flags, 0x54 fourCC,
//! 0x58 RGB bit count, 0x5C..0x6C R/G/B/A masks   0x70 caps2 (cube map = 0x200)
//! 0x80 pixel data: mip 0, mip 1, ... (each level is max(1, size >> level))
//! ```

use std::fmt;

/// Pixel formats found in the game's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdsFormat {
    /// BC1: 8 bytes per 4x4 block, 1-bit alpha.
    Dxt1,
    /// BC2: 16 bytes per block, explicit 4-bit alpha. DXT2 (premultiplied) is read as DXT3.
    Dxt3,
    /// BC3: 16 bytes per block, interpolated alpha.
    Dxt5,
    /// Uncompressed, described by its bit count and channel masks.
    Rgb {
        /// Bits per pixel: 8, 16, 24 or 32.
        bits: u32,
        /// Red mask (luminance mask for L8/L16).
        r: u32,
        /// Green mask.
        g: u32,
        /// Blue mask.
        b: u32,
        /// Alpha mask (0 = opaque).
        a: u32,
    },
    /// D3DFMT_A16B16G16R16 (fourCC as the number 36): 16 bits per channel, R in the low word
    /// (`testdata\gadgets.dds`). Decoded by keeping the high byte of each channel.
    A16B16G16R16,
    /// D3DFMT_Q8W8V8U8 (fourCC as the number 63): four signed bytes per pixel, U V W Q
    /// (`ui\cinematicicons4.dds`). Decoded as unsigned bytes (INFERRED: the file is a plain
    /// 8-bit RGBA image stored under that code; see `decode_rgba8`).
    Q8W8V8U8,
}

impl DdsFormat {
    /// Bytes needed for one mip level of `w` x `h` pixels.
    pub fn level_size(self, w: u32, h: u32) -> usize {
        let blocks = |n: u32| n.div_ceil(4).max(1) as usize;
        match self {
            Self::Dxt1 => blocks(w) * blocks(h) * 8,
            Self::Dxt3 | Self::Dxt5 => blocks(w) * blocks(h) * 16,
            Self::Rgb { bits, .. } => w as usize * h as usize * (bits as usize / 8),
            Self::A16B16G16R16 => w as usize * h as usize * 8,
            Self::Q8W8V8U8 => w as usize * h as usize * 4,
        }
    }
}

/// A parsed DDS file (borrowing the bytes).
#[derive(Debug, Clone)]
pub struct Dds<'a> {
    /// Width of mip 0 in pixels.
    pub width: u32,
    /// Height of mip 0 in pixels.
    pub height: u32,
    /// Number of mip levels actually present in the data (at least 1).
    pub mip_count: u32,
    /// Pixel format.
    pub format: DdsFormat,
    /// True if the file is a cube map (only face 0 is used here).
    pub cube_map: bool,
    data: &'a [u8],
}

/// Why a DDS could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdsError {
    /// Missing `"DDS "` or a header shorter than 128 bytes.
    BadHeader,
    /// A fourCC or bit layout this reader does not handle.
    Unsupported(String),
    /// The pixel data is shorter than mip 0 needs.
    Truncated,
}

impl fmt::Display for DdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadHeader => write!(f, "not a DDS file"),
            Self::Unsupported(s) => write!(f, "unsupported DDS format {s}"),
            Self::Truncated => write!(f, "DDS pixel data is truncated"),
        }
    }
}

impl std::error::Error for DdsError {}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl<'a> Dds<'a> {
    /// Parses the header. Mip levels missing from the end of the data are dropped.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, DdsError> {
        if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
            return Err(DdsError::BadHeader);
        }
        let height = u32_at(bytes, 0x0C);
        let width = u32_at(bytes, 0x10);
        let pf_flags = u32_at(bytes, 0x50);
        let format = if pf_flags & 0x4 != 0 {
            match &bytes[0x54..0x58] {
                b"DXT1" => DdsFormat::Dxt1,
                b"DXT2" | b"DXT3" => DdsFormat::Dxt3,
                b"DXT4" | b"DXT5" => DdsFormat::Dxt5,
                [36, 0, 0, 0] => DdsFormat::A16B16G16R16,
                [63, 0, 0, 0] => DdsFormat::Q8W8V8U8,
                other => return Err(DdsError::Unsupported(format!("fourCC {other:?}"))),
            }
        } else {
            let bits = u32_at(bytes, 0x58);
            if ![8, 16, 24, 32].contains(&bits) {
                return Err(DdsError::Unsupported(format!("{bits}-bit")));
            }
            let mut r = u32_at(bytes, 0x5C);
            let a = if pf_flags & 0x3 != 0 { u32_at(bytes, 0x68) } else { 0 };
            if r == 0 && a == 0 {
                r = if bits == 8 { 0xFF } else { 0xFFFF };
            }
            DdsFormat::Rgb { bits, r, g: u32_at(bytes, 0x60), b: u32_at(bytes, 0x64), a }
        };
        let cube_map = u32_at(bytes, 0x70) & 0x200 != 0;
        let data = &bytes[128..];
        if width == 0 || height == 0 || format.level_size(width, height) > data.len() {
            return Err(DdsError::Truncated);
        }
        let wanted = u32_at(bytes, 0x1C).max(1);
        let (mut mips, mut used) = (0, 0usize);
        while mips < wanted && mips < 16 {
            let size = format.level_size((width >> mips).max(1), (height >> mips).max(1));
            if used + size > data.len() {
                break;
            }
            used += size;
            mips += 1;
        }
        Ok(Self { width, height, mip_count: mips, format, cube_map, data })
    }

    /// The raw bytes of one mip level (as stored, still compressed for DXT).
    pub fn level_data(&self, level: u32) -> &'a [u8] {
        let mut off = 0;
        for l in 0..level {
            off += self.format.level_size((self.width >> l).max(1), (self.height >> l).max(1));
        }
        let size = self.format.level_size((self.width >> level).max(1), (self.height >> level).max(1));
        &self.data[off..off + size]
    }

    /// Size in pixels of a mip level.
    pub fn level_dims(&self, level: u32) -> (u32, u32) {
        ((self.width >> level).max(1), (self.height >> level).max(1))
    }

    /// Decodes one mip level to RGBA8 (4 bytes per pixel, rows top to bottom).
    pub fn decode_rgba8(&self, level: u32) -> Vec<u8> {
        let (w, h) = self.level_dims(level);
        let src = self.level_data(level);
        let mut out = vec![0u8; w as usize * h as usize * 4];
        match self.format {
            DdsFormat::Dxt1 | DdsFormat::Dxt3 | DdsFormat::Dxt5 => decode_blocks(self.format, src, w, h, &mut out),
            DdsFormat::A16B16G16R16 => {
                for (px, o) in src.chunks_exact(8).zip(out.chunks_exact_mut(4)) {
                    // R, G, B, A little-endian u16s; keep the high byte of each.
                    o.copy_from_slice(&[px[1], px[3], px[5], px[7]]);
                }
            }
            DdsFormat::Q8W8V8U8 => {
                let n = out.len();
                out.copy_from_slice(&src[..n]);
            }
            DdsFormat::Rgb { bits, r, g, b, a } => {
                let bpp = bits as usize / 8;
                for (i, px) in src.chunks_exact(bpp).enumerate() {
                    let mut v = 0u32;
                    for (k, byte) in px.iter().enumerate() {
                        v |= u32::from(*byte) << (8 * k);
                    }
                    let lum_only = g == 0 && b == 0 && r != 0;
                    let rr = channel(v, r);
                    let o = &mut out[i * 4..i * 4 + 4];
                    o[0] = rr;
                    o[1] = if lum_only { rr } else { channel(v, g) };
                    o[2] = if lum_only { rr } else { channel(v, b) };
                    o[3] = if a == 0 { 255 } else { channel(v, a) };
                }
            }
        }
        out
    }
}

/// Extracts a masked channel and scales it to 0..=255.
fn channel(v: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    let x = (v & mask) >> shift;
    ((u64::from(x) * 255 + u64::from(max) / 2) / u64::from(max)) as u8
}

fn rgb565(c: u16) -> [u8; 3] {
    let r = (c >> 11) & 31;
    let g = (c >> 5) & 63;
    let b = c & 31;
    [((r * 527 + 23) >> 6) as u8, ((g * 259 + 33) >> 6) as u8, ((b * 527 + 23) >> 6) as u8]
}

/// Decodes the 4x4 colour part of a BC1/BC2/BC3 block into `px` (RGBA, alpha 255 / 0).
fn color_block(b: &[u8], allow_1bit_alpha: bool, px: &mut [[u8; 4]; 16]) {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |a: u8, b: u8, wa: u16, wb: u16| ((u16::from(a) * wa + u16::from(b) * wb) / (wa + wb)) as u8;
    let mut pal = [[0u8; 4]; 4];
    pal[0] = [p0[0], p0[1], p0[2], 255];
    pal[1] = [p1[0], p1[1], p1[2], 255];
    if c0 > c1 || !allow_1bit_alpha {
        pal[2] = [mix(p0[0], p1[0], 2, 1), mix(p0[1], p1[1], 2, 1), mix(p0[2], p1[2], 2, 1), 255];
        pal[3] = [mix(p0[0], p1[0], 1, 2), mix(p0[1], p1[1], 1, 2), mix(p0[2], p1[2], 1, 2), 255];
    } else {
        pal[2] = [mix(p0[0], p1[0], 1, 1), mix(p0[1], p1[1], 1, 1), mix(p0[2], p1[2], 1, 1), 255];
        pal[3] = [0, 0, 0, 0];
    }
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    for (i, p) in px.iter_mut().enumerate() {
        *p = pal[((bits >> (2 * i)) & 3) as usize];
    }
}

/// Decodes raw block-compressed pixels (no DDS header) of `w` x `h` to RGBA8, top row first.
/// Used for data that stores DXT blocks without a DDS header (the campaign supertexture).
/// Short input leaves the missing pixels transparent black.
pub fn decode_blocks_rgba8(format: DdsFormat, w: u32, h: u32, src: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; w as usize * h as usize * 4];
    let need = format.level_size(w, h).min(src.len());
    decode_blocks(format, &src[..need], w, h, &mut out);
    out
}

fn decode_blocks(format: DdsFormat, src: &[u8], w: u32, h: u32, out: &mut [u8]) {
    let bw = w.div_ceil(4) as usize;
    let block_bytes = if format == DdsFormat::Dxt1 { 8 } else { 16 };
    for (bi, b) in src.chunks_exact(block_bytes).enumerate() {
        let (bx, by) = (bi % bw, bi / bw);
        let mut px = [[0u8; 4]; 16];
        match format {
            DdsFormat::Dxt1 => color_block(b, true, &mut px),
            DdsFormat::Dxt3 => {
                color_block(&b[8..], false, &mut px);
                for (i, p) in px.iter_mut().enumerate() {
                    let nib = (b[i / 2] >> (4 * (i % 2))) & 0xF;
                    p[3] = nib * 17;
                }
            }
            _ => {
                color_block(&b[8..], false, &mut px);
                let (a0, a1) = (u16::from(b[0]), u16::from(b[1]));
                let mut pal = [0u8; 8];
                pal[0] = a0 as u8;
                pal[1] = a1 as u8;
                for k in 1..7u16 {
                    pal[k as usize + 1] = if a0 > a1 {
                        (((7 - k) * a0 + k * a1) / 7) as u8
                    } else if k < 5 {
                        (((5 - k) * a0 + k * a1) / 5) as u8
                    } else if k == 5 {
                        0
                    } else {
                        255
                    };
                }
                let mut bits = 0u64;
                for k in 0..6 {
                    bits |= u64::from(b[2 + k]) << (8 * k);
                }
                for (i, p) in px.iter_mut().enumerate() {
                    p[3] = pal[((bits >> (3 * i)) & 7) as usize];
                }
            }
        }
        for (i, p) in px.iter().enumerate() {
            let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
            if x < w as usize && y < h as usize {
                let o = (y * w as usize + x) * 4;
                out[o..o + 4].copy_from_slice(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: u32, h: u32, mips: u32, pf_flags: u32, fourcc: &[u8; 4], bits: u32, masks: [u32; 4]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[0..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[0x0C..0x10].copy_from_slice(&h.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&w.to_le_bytes());
        b[0x1C..0x20].copy_from_slice(&mips.to_le_bytes());
        b[0x50..0x54].copy_from_slice(&pf_flags.to_le_bytes());
        b[0x54..0x58].copy_from_slice(fourcc);
        b[0x58..0x5C].copy_from_slice(&bits.to_le_bytes());
        for (k, m) in masks.iter().enumerate() {
            b[0x5C + 4 * k..0x60 + 4 * k].copy_from_slice(&m.to_le_bytes());
        }
        b
    }

    #[test]
    fn dxt1_solid_red_with_mips() {
        // 8x8 DXT1, 4 mips claimed but only 8x8 + 4x4 present -> 2 kept.
        let mut b = header(8, 8, 4, 4, b"DXT1", 0, [0; 4]);
        let block = [0x00, 0xF8, 0x00, 0xF8, 0, 0, 0, 0]; // c0 = c1 = pure red, all index 0
        for _ in 0..5 {
            b.extend_from_slice(&block);
        }
        let d = Dds::parse(&b).unwrap();
        assert_eq!((d.width, d.height, d.mip_count, d.format), (8, 8, 2, DdsFormat::Dxt1));
        let px = d.decode_rgba8(0);
        assert_eq!(px.len(), 8 * 8 * 4);
        assert!(px.chunks(4).all(|p| p == [255, 0, 0, 255]));
        assert_eq!(d.decode_rgba8(1).len(), 4 * 4 * 4);
    }

    #[test]
    fn dxt5_alpha_and_bgra() {
        let mut b = header(4, 4, 1, 4, b"DXT5", 0, [0; 4]);
        b.extend_from_slice(&[128, 128, 0, 0, 0, 0, 0, 0]); // alpha 128 everywhere
        b.extend_from_slice(&[0x1F, 0x00, 0x1F, 0x00, 0, 0, 0, 0]); // blue
        let px = Dds::parse(&b).unwrap().decode_rgba8(0);
        assert_eq!(&px[0..4], &[0, 0, 255, 128]);

        let mut b = header(1, 1, 1, 0x41, b"\0\0\0\0", 32, [0xFF0000, 0xFF00, 0xFF, 0xFF00_0000]);
        b.extend_from_slice(&[1, 2, 3, 4]); // stored B, G, R, A
        assert_eq!(Dds::parse(&b).unwrap().decode_rgba8(0), [3, 2, 1, 4]);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Dds::parse(b"nope").unwrap_err(), DdsError::BadHeader);
        let b = header(64, 64, 1, 4, b"DXT1", 0, [0; 4]);
        assert_eq!(Dds::parse(&b).unwrap_err(), DdsError::Truncated);
        let b = header(4, 4, 1, 4, b"ATI2", 0, [0; 4]);
        assert!(matches!(Dds::parse(&b), Err(DdsError::Unsupported(_))));
    }
}

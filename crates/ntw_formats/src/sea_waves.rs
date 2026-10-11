//! The battle sea's animated wave textures: `sea\sea` (the short chop) and `sea\swell` (the long
//! swell), the engine's own container (magic `0x12345678`, version 1, no file extension).
//!
//! Layout (CONFIRMED: loader `0x010D25C0`, and the two shipped files decode to their exact
//! lengths, 32,771,262 and 2,951,102 bytes; `analysis/graphics/WATER.md` §3):
//! ```text
//! u32 magic 0x12345678, u32 version 1
//! u32 size (256 sea, 128 swell)        f32 tile metres (70, 800)
//! f32 loop period in seconds (10, 30)  u32 frame count (100, 60)    u8, u8 (0, 1)
//! f32 height min, max
//! u16 angle lo x, lo y, hi x, hi y     the dequantisation range of the two angle channels
//! f32 displacement min x, y, max x, y  (the swell's is empty: FLT_MAX, -FLT_MAX)
//! u32 frame count
//! per frame:
//!   u32 w, h, w*h u8                   heights (the CPU mesh displacement)
//!   u32 w, h, w*h*2 u8                 the two slope angles, interleaved per texel (x, z)
//!   u32 w, h, w*h*2 u8                 horizontal displacement (none in the swell: w*h = 0)
//!   u32 n, n * 16 bytes, u32 m, m * 8 bytes   (both 0 in the shipped files)
//! ```
//! Only the angle images are kept: they are what `ocean.fx` samples as `s_sea_texture` /
//! `s_swell_texture`. The engine rebuilds that texture every frame (`0x010E21F0`) from two
//! neighbouring frames, see [`SeaWaves::frame_blend`], and expands each byte to 16 bits with
//! `byte * (hi - lo) / 255 + lo` (`0x010E3140`), see [`SeaWaves::dequantise`]. The heights and the
//! horizontal displacement drive the original's CPU-displaced sea mesh, which we do not build yet.

use std::fmt;

use crate::bytes::Cursor;

/// The container's magic number.
pub const MAGIC: u32 = 0x1234_5678;

/// A decode failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeaWavesError(pub String);

impl fmt::Display for SeaWavesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sea wave container: {}", self.0)
    }
}

impl std::error::Error for SeaWavesError {}

/// The decoded angle animation of one sea wave container.
#[derive(Debug, Clone, PartialEq)]
pub struct SeaWaves {
    /// Texture width and height of the angle images in texels.
    pub size: u32,
    /// World metres one texture repeat covers (`g_sea_uv_scale` / `g_swell_uv_scale` = 1 / this,
    /// CONFIRMED per-draw setter `0x0119E830`).
    pub tile_metres: f32,
    /// Seconds one pass through all frames takes; the animation loops.
    pub period: f32,
    /// Number of frames.
    pub frames: u32,
    /// Per channel (x, z): the 16-bit value byte 0 stands for.
    pub lo: [u16; 2],
    /// Per channel (x, z): the 16-bit value byte 255 stands for.
    pub hi: [u16; 2],
    /// `frames` images of `size * size` texels, two bytes each (x angle, z angle), row-major.
    pub angles: Vec<u8>,
}

impl SeaWaves {
    /// Decodes a whole container.
    pub fn parse(bytes: &[u8]) -> Result<Self, SeaWavesError> {
        let err = |e| SeaWavesError(format!("{e:?}"));
        let mut c = Cursor::new(bytes);
        let magic = c.u32().map_err(err)?;
        let version = c.u32().map_err(err)?;
        if magic != MAGIC || version != 1 {
            return Err(SeaWavesError(format!("magic 0x{magic:08x} version {version}, expected 0x{MAGIC:08x} 1")));
        }
        let size = c.u32().map_err(err)?;
        let tile_metres = c.f32().map_err(err)?;
        let period = c.f32().map_err(err)?;
        let frames = c.u32().map_err(err)?;
        c.take(2).map_err(err)?; // two flag bytes (0, 1), not used by the angle path
        c.take(8).map_err(err)?; // height min, max
        let lo = [c.u16().map_err(err)?, c.u16().map_err(err)?];
        let hi = [c.u16().map_err(err)?, c.u16().map_err(err)?];
        c.take(16).map_err(err)?; // displacement min, max
        let listed = c.u32().map_err(err)?;
        // A period or tile size that is not a positive finite number (NaN included) cannot animate.
        let positive = |v: f32| v.is_finite() && v > 0.0;
        if listed != frames || frames == 0 || size == 0 || !positive(period) || !positive(tile_metres) {
            return Err(SeaWavesError(format!(
                "size {size}, {frames} frames in the header, {listed} listed, period {period}, tile {tile_metres}"
            )));
        }
        let texels = (size as usize).checked_mul(size as usize).ok_or_else(|| SeaWavesError(format!("size {size}")))?;
        let mut angles = Vec::with_capacity(texels.saturating_mul(2).saturating_mul(frames as usize).min(bytes.len()));
        for frame in 0..frames {
            let image = |c: &mut Cursor, per_texel: usize| -> Result<(u32, u32, usize), SeaWavesError> {
                let w = c.u32().map_err(err)?;
                let h = c.u32().map_err(err)?;
                let n = (w as usize).checked_mul(h as usize).and_then(|n| n.checked_mul(per_texel));
                n.map(|n| (w, h, n)).ok_or_else(|| SeaWavesError(format!("frame {frame}: image {w}x{h}")))
            };
            let (_, _, heights) = image(&mut c, 1)?;
            c.take(heights).map_err(err)?;
            let (w, h, n) = image(&mut c, 2)?;
            if w != size || h != size {
                return Err(SeaWavesError(format!("frame {frame}: angle image {w}x{h}, header says {size}")));
            }
            angles.extend_from_slice(c.take(n).map_err(err)?);
            let (_, _, displacement) = image(&mut c, 2)?;
            c.take(displacement).map_err(err)?;
            for item in [16usize, 8] {
                let count = c.u32().map_err(err)? as usize;
                let n = count.checked_mul(item).ok_or_else(|| SeaWavesError(format!("frame {frame}: {count} items")))?;
                c.take(n).map_err(err)?;
            }
        }
        if c.remaining() != 0 {
            return Err(SeaWavesError(format!("{} bytes left after {frames} frames", c.remaining())));
        }
        Ok(Self { size, tile_metres, period, frames, lo, hi, angles })
    }

    /// The two frames shown at `time` seconds and the blend weight towards the second, in 1/256
    /// steps (0..=256), as `0x010E21F0` computes them: `f = fmod(time, period) * frames / period`,
    /// `a = floor(f)`, `b = (a + 1) % frames`, `w = round((f - a) * 256)`.
    pub fn frame_blend(&self, time: f32) -> (u32, u32, u32) {
        let f = time.rem_euclid(self.period) * self.frames as f32 / self.period;
        let a = (f.floor() as u32).min(self.frames - 1);
        let b = (a + 1) % self.frames;
        let w = ((f - a as f32) * 256.0).round().clamp(0.0, 256.0) as u32;
        (a, b, w)
    }

    /// The 16-bit value a stored byte of channel `ch` (0 = x, 1 = z) stands for:
    /// `trunc(byte * (hi - lo) / 255 + lo)` (`0x010E3140`).
    pub fn dequantise(&self, ch: usize, byte: u8) -> u16 {
        let (lo, hi) = (self.lo[ch] as f32, self.hi[ch] as f32);
        (byte as f32 * (hi - lo) / 255.0 + lo) as u16
    }

    /// One frame's angle bytes.
    pub fn frame(&self, i: u32) -> &[u8] {
        let n = self.size as usize * self.size as usize * 2;
        &self.angles[i as usize * n..(i as usize + 1) * n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A container with `frames` frames of a 2x2 angle image, built the way the shipped files are.
    fn container(frames: u32) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [MAGIC, 1, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&70f32.to_le_bytes());
        b.extend_from_slice(&10f32.to_le_bytes());
        b.extend_from_slice(&frames.to_le_bytes());
        b.extend_from_slice(&[0, 1]);
        b.extend_from_slice(&[0; 8]);
        for v in [100u16, 200, 300, 400] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(&frames.to_le_bytes());
        for f in 0..frames {
            b.extend_from_slice(&2u32.to_le_bytes());
            b.extend_from_slice(&2u32.to_le_bytes());
            b.extend_from_slice(&[9; 4]);
            b.extend_from_slice(&2u32.to_le_bytes());
            b.extend_from_slice(&2u32.to_le_bytes());
            b.extend((0..8).map(|i| (f * 8 + i) as u8));
            b.extend_from_slice(&[0; 8]); // an empty displacement image
            b.extend_from_slice(&[0; 8]); // no items
        }
        b
    }

    #[test]
    fn decodes_only_the_angle_frames() {
        let w = SeaWaves::parse(&container(3)).unwrap();
        assert_eq!((w.size, w.tile_metres, w.period, w.frames), (2, 70.0, 10.0, 3));
        assert_eq!((w.lo, w.hi), ([100, 200], [300, 400]));
        assert_eq!(w.frame(1), &[8, 9, 10, 11, 12, 13, 14, 15]);
        let mut short = container(3);
        short.pop();
        assert!(SeaWaves::parse(&short).is_err());
        let mut long = container(3);
        long.push(0);
        assert!(SeaWaves::parse(&long).is_err());
    }

    #[test]
    fn frame_blend_loops_like_the_exe() {
        let w = SeaWaves::parse(&container(100)).unwrap();
        assert_eq!(w.frame_blend(0.0), (0, 1, 0));
        // 0.05 s of a 10 s, 100-frame loop is half way from frame 0 to 1.
        assert_eq!(w.frame_blend(0.05), (0, 1, 128));
        assert_eq!(w.frame_blend(9.95), (99, 0, 128));
        assert_eq!(w.frame_blend(10.0), (0, 1, 0));
        assert_eq!(w.dequantise(0, 0), 100);
        assert_eq!(w.dequantise(0, 255), 300);
        assert_eq!(w.dequantise(1, 128), 300);
    }
}

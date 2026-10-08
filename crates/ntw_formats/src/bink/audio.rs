//! Bink audio, our own decoder for both variants (DCT and RDFT), from `analysis/video/BINK.md` §5.
//!
//! Every shipped track is the DCT variant, stereo, 16-bit, at 44,100 or 48,000 Hz. One audio
//! packet (per frame and track) is `u32 decoded_bytes` followed by 32-bit-aligned blocks; each
//! block gives `frame_len - frame_len/16` samples per channel, and its first `frame_len/16`
//! samples are cross-faded with the tail of the previous block.

use super::bits::BitReader;
use super::dct::{Dct3, Irdft};
use super::{AudioTrackInfo, BinkError};

/// Band edges in Hz (index 0 = 0 Hz). CONFIRMED (DLL data).
pub const CRITICAL_FREQS: [u32; 25] = [
    0, 100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700, 4400, 5300,
    6400, 7700, 9500, 12000, 15500,
];

/// Coefficient run lengths in units of 8 (CONFIRMED).
pub const RUN_LENGTHS: [u8; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

/// Band quantiser values as f32 bits, `exp(i * 0.152891648)` (CONFIRMED, DLL data).
pub const QUANT_BITS: [u32; 96] = [
    0x3f800000, 0x3f95253b, 0x3fadc8b9, 0x3fca7e34, 0x3febf1cf, 0x4009760e, 0x40202b68, 0x403aa11e, 0x405975d6,
    0x407d626c, 0x40939f23, 0x40ac022f, 0x40c86c93, 0x40e988af, 0x41080e85, 0x411e887a, 0x4138b8fb, 0x41573d0f,
    0x417acbaf, 0x41921d06, 0x41aa404a, 0x41c6605b, 0x41e725de, 0x4206aaa8, 0x421ce9d4, 0x4236d5d5, 0x42550a18,
    0x42783bb8, 0x42909edc, 0x42a882fd, 0x42c4597f, 0x42e4c94a, 0x43054a6e, 0x431b4f6a, 0x4334f79e, 0x4352dce1,
    0x4375b274, 0x438f2499, 0x43a6ca3d, 0x43c257ef, 0x43e272e4, 0x4403edce, 0x4419b931, 0x44331e4a, 0x4450b55c,
    0x44732fd3, 0x448dae34, 0x44a515fe, 0x44c05b9f, 0x44e0229b, 0x450294bd, 0x4518271f, 0x453149cc, 0x454e937a,
    0x4570b3c2, 0x458c3ba2, 0x45a36634, 0x45be6480, 0x45ddd85f, 0x46013f33, 0x46169929, 0x462f7a18, 0x464c772b,
    0x466e3e31, 0x468accd9, 0x46a1bad4, 0x46bc7285, 0x46db9420, 0x46ffda4c, 0x47150f44, 0x472daf20, 0x474a6061,
    0x476bcf0e, 0x478961cf, 0x47a013d1, 0x47ba85a1, 0x47d955cf, 0x47fd3d1a, 0x48138965, 0x482be8d9, 0x48484f0e,
    0x4869664a, 0x4887fa7b, 0x489e7121, 0x48b89dc6, 0x48d71d5c, 0x48faa6bf, 0x49120781, 0x492a2736, 0x49464323,
    0x496703d3, 0x498696d3, 0x499cd2b7, 0x49b6bae7, 0x49d4eab7, 0x49f81728,
];

enum Transform {
    Dct(Dct3),
    Rdft(Irdft),
}

/// Decodes one audio track to interleaved 16-bit PCM.
pub struct AudioDecoder {
    /// Output channels (1 or 2).
    channels: usize,
    sample_rate: u32,
    /// Channels coded separately (DCT: the real count; RDFT: 1, interleaved inside the block).
    coded_channels: usize,
    frame_len: usize,
    /// Band edges in coefficient pairs (`band[k] * 2` = first coefficient of band k + 1).
    bands: Vec<usize>,
    /// Output scale `2 / sqrt(frame_len)`, as f32 like the original.
    scale: f32,
    /// Cross-fade length in interleaved samples, and its log2 shift (0 if not a power of two
    /// the original knows).
    overlap: usize,
    shift: u32,
    first: bool,
    previous: Vec<i16>,
    transform: Transform,
    coeffs: Vec<f64>,
    block: Vec<i16>,
}

impl AudioDecoder {
    pub fn new(track: &AudioTrackInfo) -> Result<Self, BinkError> {
        let channels = track.channels() as usize;
        let rate = track.sample_rate as u32;
        if rate == 0 {
            return Err(BinkError::BadHeader("audio sample rate 0"));
        }
        let dct = track.uses_dct();
        // Frame length from the sample rate (CONFIRMED, BINK.md §5.1).
        let mut frame_len = if rate < 22050 {
            512
        } else if rate < 44100 {
            1024
        } else {
            2048
        };
        let (mut coded_rate, mut coded_channels) = (rate, channels);
        if !dct {
            // RDFT: the channels are interleaved inside one longer transform.
            frame_len *= channels;
            coded_rate *= channels as u32;
            coded_channels = 1;
        }
        let half = frame_len / 2;
        let half_rate = coded_rate.div_ceil(2);
        let num_bands = CRITICAL_FREQS.iter().position(|&f| half_rate <= f).unwrap_or(CRITICAL_FREQS.len());
        let mut bands: Vec<usize> = CRITICAL_FREQS[..num_bands]
            .iter()
            .map(|&f| ((f as u64 * half as u64 / half_rate as u64) as usize).max(1))
            .collect();
        bands.push(half);
        let overlap = frame_len * coded_channels / 16;
        let shift = match overlap {
            32 => 5,
            64 => 6,
            128 => 7,
            256 => 8,
            _ => 0,
        };
        Ok(AudioDecoder {
            channels,
            sample_rate: rate,
            coded_channels,
            frame_len,
            bands,
            scale: (2.0 / (frame_len as f32).sqrt()),
            overlap,
            shift,
            first: true,
            previous: vec![0; overlap],
            transform: if dct { Transform::Dct(Dct3::new(frame_len)) } else { Transform::Rdft(Irdft::new(frame_len)) },
            coeffs: vec![0.0; frame_len],
            block: vec![0; frame_len * coded_channels],
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Interleaved samples one block produces.
    pub fn block_samples(&self) -> usize {
        self.frame_len * self.coded_channels - self.overlap
    }

    /// Back to the start state (after a seek).
    pub fn reset(&mut self) {
        self.first = true;
        self.previous.fill(0);
    }

    /// Decodes one audio packet (as split from a frame, including its leading byte count) and
    /// appends interleaved samples to `out`. Returns the bytes of block data used (whole 32-bit
    /// words), which equals the packet length minus 4 for a well-formed packet.
    pub fn decode_packet(&mut self, packet: &[u8], out: &mut Vec<i16>) -> Result<usize, BinkError> {
        if packet.len() < 4 {
            return Ok(0);
        }
        let mut want = u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize / 2;
        let data = &packet[4..];
        let mut br = BitReader::new(data);
        while want > 0 && br.position() < br.len_bits() {
            self.decode_block(&mut br)?;
            br.align32();
            let n = self.block_samples().min(want);
            out.extend_from_slice(&self.block[..n]);
            want -= n;
        }
        Ok(br.position() / 8)
    }

    fn decode_block(&mut self, br: &mut BitReader) -> Result<(), BinkError> {
        let dct = matches!(self.transform, Transform::Dct(_));
        if dct {
            br.skip(2);
        }
        let fl = self.frame_len;
        let nb = self.bands.len() - 1;
        let mut quant = [0f32; 25];
        for ch in 0..self.coded_channels {
            let c = &mut self.coeffs;
            c[0] = read_float(br) as f64;
            c[1] = read_float(br) as f64;
            for q in quant.iter_mut().take(nb) {
                let v = (br.read(8) as usize).min(95);
                *q = f32::from_bits(QUANT_BITS[v]);
            }
            // Coefficients 2.. in runs of equal bit width; band k's quantiser starts at
            // coefficient bands[k] * 2 (CONFIRMED order, BINK.md §5.2).
            let mut k = 0usize;
            let mut q = 0f32;
            let mut i = 2usize;
            while i < fl {
                let j = if br.bit() { i + RUN_LENGTHS[br.read(4) as usize] as usize * 8 } else { i + 8 };
                let j = j.min(fl);
                let width = br.read(4);
                if width == 0 {
                    c[i..j].fill(0.0);
                    i = j;
                    while k < nb && self.bands[k] * 2 < i {
                        q = quant[k];
                        k += 1;
                    }
                } else {
                    while i < j {
                        if k < nb && self.bands[k] * 2 == i {
                            q = quant[k];
                            k += 1;
                        }
                        let v = br.read(width);
                        c[i] = if v == 0 {
                            0.0
                        } else if br.bit() {
                            (-q * v as f32) as f64
                        } else {
                            (q * v as f32) as f64
                        };
                        i += 1;
                    }
                }
            }
            match &mut self.transform {
                Transform::Dct(t) => t.run(c),
                Transform::Rdft(t) => t.run(c),
            }
            // Convert to 16-bit with round-half-even and saturation, interleaving channels.
            let step = self.coded_channels;
            let scale = self.scale as f64;
            for (n, &x) in c.iter().enumerate() {
                let v = (x * scale).round_ties_even();
                self.block[n * step + ch] = v.clamp(-32768.0, 32767.0) as i16;
            }
        }
        if br.overrun() {
            return Err(BinkError::BadFrame { frame: 0, what: "audio block reads past its packet".into() });
        }
        // Cross-fade with the previous block's tail (SSE2-path semantics, BINK.md §5.4).
        if !self.first {
            for i in 0..self.overlap {
                let p = self.previous[i];
                let d = self.block[i].wrapping_sub(p) as i32;
                let v = ((d * i as i32) >> self.shift) + p as i32;
                self.block[i] = v.clamp(-32768, 32767) as i16;
            }
        }
        self.first = false;
        let total = self.block.len();
        self.previous.copy_from_slice(&self.block[total - self.overlap..]);
        Ok(())
    }
}

/// A 29-bit float: 5-bit power, 23-bit mantissa, sign. Value = mantissa * POW[power], where
/// `POW[p]` = 2^(p - 23) for p <= 23; the original's table holds NaN, 0 and tiny denormals
/// for 24..31 (CONFIRMED, DLL data; never used by the shipped files).
fn read_float(br: &mut BitReader) -> f32 {
    const HIGH: [u32; 8] = [0xffff_ffff, 0, 0, 1, 2, 3, 4, 5];
    let power = br.read(5) as i32;
    let mantissa = br.read(23) as f32;
    let p = if power <= 23 { (2f32).powi(power - 23) } else { f32::from_bits(HIGH[(power - 24) as usize]) };
    let v = mantissa * p;
    if br.bit() { -v } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quant_table_is_exponential() {
        for (i, &b) in QUANT_BITS.iter().enumerate() {
            let want = (i as f64 * 0.152_891_647_872_219_54).exp();
            let got = f32::from_bits(b) as f64;
            assert!((got / want - 1.0).abs() < 1e-5, "{i}");
        }
    }

    #[test]
    fn layout_for_shipped_tracks() {
        let t = AudioTrackInfo { max_decoded_size: 0, sample_rate: 44100, flags: 0x7000, id: 0 };
        let d = AudioDecoder::new(&t).unwrap();
        assert_eq!((d.frame_len, d.coded_channels, d.overlap, d.shift), (2048, 2, 256, 8));
        assert_eq!(d.block_samples() * 2, 7680);
        assert_eq!(d.bands.len(), 26);
        assert_eq!(d.bands[0], 1);
        assert_eq!(*d.bands.last().unwrap(), 1024);
    }
}

//! Microsoft ADPCM (WAV format tag 2), our own decoder in place of symphonia 0.5.5's.
//!
//! Same algorithm and output as symphonia's MS-ADPCM decoder (fixed coefficient table indexed by
//! the block's predictor byte, sample clamped to `i16`, step size `delta × table / 256`, at least
//! 16), except that the step size saturates instead of overflowing `i32`: on extreme data
//! (large nibbles block after block) symphonia's step overflows, which panics where overflow
//! checks are on (debug builds) and wraps in release. Saturating keeps every sample within the
//! format's `i16` clamp in both builds. No shipped sound is MS-ADPCM (AUDIO_FORMAT.md §2), so
//! only mods reach this. PROVISIONAL: what Miles does once the step size grows that large is
//! unknown.

use symphonia::core::audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Signal, SignalSpec};
use symphonia::core::codecs::{CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult, CODEC_TYPE_ADPCM_MS};
use symphonia::core::errors::{unsupported_error, Error, Result};
use symphonia::core::formats::Packet;

const ADAPTATION: [i32; 16] = [230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230];
const COEFF1: [i32; 7] = [256, 512, 0, 192, 240, 460, 392];
const COEFF2: [i32; 7] = [0, -256, 0, 64, 0, -208, -232];
const DELTA_MIN: i32 = 16;

/// One channel's predictor state within a block.
#[derive(Clone, Copy)]
struct Channel {
    c1: i32,
    c2: i32,
    delta: i32,
    s1: i32,
    s2: i32,
}

impl Channel {
    /// Decodes one 4-bit code. Bounds: `|s| ≤ 32768` and `|c| ≤ 512`, so the prediction is within
    /// ±131 072; `delta` saturates at `i32::MAX / 256` after the division, so `nibble × delta`
    /// (|nibble| ≤ 8) stays within `i32`. Only the step product can overflow, hence saturating.
    fn expand(&mut self, nibble: u8) -> i16 {
        let signed = i32::from(((nibble << 4) as i8) >> 4);
        let predicted = (self.s1 * self.c1 + self.s2 * self.c2) / 256 + signed * self.delta;
        let s = predicted.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        self.s2 = self.s1;
        self.s1 = s;
        self.delta = (ADAPTATION[usize::from(nibble & 0xF)].saturating_mul(self.delta) / 256).max(DELTA_MIN);
        s as i16
    }
}

/// Reads little-endian fields from a block.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u8(&mut self) -> Result<u8> {
        let (&b, rest) = self.0.split_first().ok_or(Error::DecodeError("adpcm: block truncated"))?;
        self.0 = rest;
        Ok(b)
    }

    fn i16(&mut self) -> Result<i32> {
        Ok(i32::from(i16::from_le_bytes([self.u8()?, self.u8()?])))
    }

    fn predictor(&mut self) -> Result<usize> {
        let p = usize::from(self.u8()?);
        if p >= COEFF1.len() {
            return Err(Error::DecodeError("adpcm: block predictor exceeds range"));
        }
        Ok(p)
    }
}

/// Bytes of one block of `fpb` frames: a 7-byte header per channel (predictor, delta, two
/// samples) and one nibble per channel for every frame after the first two, rounded up to bytes.
fn block_bytes(fpb: usize, channels: usize) -> usize {
    7 * channels + ((fpb - 2) * channels).div_ceil(2)
}

pub struct MsAdpcmDecoder {
    params: CodecParameters,
    frames_per_block: usize,
    buf: AudioBuffer<i16>,
}

impl MsAdpcmDecoder {
    fn decode_inner(&mut self, packet: &Packet) -> Result<()> {
        let fpb = self.frames_per_block;
        let blocks = packet.block_dur() as usize / fpb;
        self.buf.clear();
        self.buf.render_reserved(Some(blocks * fpb));
        // Each block starts at its own offset, `block_align` bytes apart. The codec parameters
        // carry no `block_align`, but the WAV reader derives the frames per block from it as
        // `(block_align − 7·ch) × 2 / ch + 2` (4-bit samples), which this inverts exactly; bytes
        // after the last whole block are ignored.
        let stereo = self.buf.spec().channels.count() == 2;
        let block_bytes = block_bytes(fpb, if stereo { 2 } else { 1 });
        let data = packet.buf();
        if data.len() < blocks * block_bytes {
            return Err(Error::DecodeError("adpcm: block truncated"));
        }
        for block in 0..blocks {
            let mut r = Reader(&data[block * block_bytes..(block + 1) * block_bytes]);
            let at = block * fpb;
            if stereo {
                let (pl, pr) = (r.predictor()?, r.predictor()?);
                let (dl, dr) = (r.i16()?, r.i16()?);
                let (s1l, s1r) = (r.i16()?, r.i16()?);
                let (s2l, s2r) = (r.i16()?, r.i16()?);
                let mut ch = [
                    Channel { c1: COEFF1[pl], c2: COEFF2[pl], delta: dl, s1: s1l, s2: s2l },
                    Channel { c1: COEFF1[pr], c2: COEFF2[pr], delta: dr, s1: s1r, s2: s2r },
                ];
                let (left, right) = self.buf.chan_pair_mut(0, 1);
                (left[at], left[at + 1]) = (ch[0].s2 as i16, ch[0].s1 as i16);
                (right[at], right[at + 1]) = (ch[1].s2 as i16, ch[1].s1 as i16);
                for f in 2..fpb {
                    let b = r.u8()?;
                    left[at + f] = ch[0].expand(b >> 4);
                    right[at + f] = ch[1].expand(b & 0xF);
                }
            } else {
                let p = r.predictor()?;
                let delta = r.i16()?;
                let (s1, s2) = (r.i16()?, r.i16()?);
                let mut ch = Channel { c1: COEFF1[p], c2: COEFF2[p], delta, s1, s2 };
                let out = self.buf.chan_mut(0);
                out[at] = s2 as i16;
                out[at + 1] = s1 as i16;
                // Two frames per byte, high nibble first; with an odd frame count the last byte's
                // low nibble is padding (frame by frame, so the last frame is decoded too).
                let mut b = 0;
                for f in 2..fpb {
                    let nibble = if f % 2 == 0 {
                        b = r.u8()?;
                        b >> 4
                    } else {
                        b & 0xF
                    };
                    out[at + f] = ch.expand(nibble);
                }
            }
        }
        Ok(())
    }
}

impl Decoder for MsAdpcmDecoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self> {
        if params.codec != CODEC_TYPE_ADPCM_MS {
            return unsupported_error("adpcm: invalid codec type");
        }
        // The block layout ([`block_bytes`]) is for 4-bit codes, the only size MS ADPCM defines.
        // The WAV reader refuses any other `bits_per_sample` before this decoder is made and
        // passes none on; a source that passes one must say 4.
        if params.bits_per_coded_sample.is_some_and(|b| b != 4) {
            return unsupported_error("adpcm: only 4-bit samples are supported");
        }
        let Some(max_frames) = params.max_frames_per_packet else {
            return unsupported_error("adpcm: maximum frames per packet is required");
        };
        let fpb = match params.frames_per_block {
            Some(n) if n >= 2 => n as usize,
            _ => return unsupported_error("adpcm: valid frames per block is required"),
        };
        let Some(rate) = params.sample_rate else {
            return unsupported_error("adpcm: sample rate is required");
        };
        let spec = match (params.channels, params.channel_layout) {
            (Some(c), _) => SignalSpec::new(rate, c),
            (None, Some(l)) => SignalSpec::new_with_layout(rate, l),
            (None, None) => return unsupported_error("adpcm: channels or channel_layout is required"),
        };
        if !(1..=2).contains(&spec.channels.count()) {
            return unsupported_error("adpcm: only mono and stereo are supported");
        }
        Ok(Self { params: params.clone(), frames_per_block: fpb, buf: AudioBuffer::new(max_frames, spec) })
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[CodecDescriptor {
            codec: CODEC_TYPE_ADPCM_MS,
            short_name: "adpcm_ms",
            long_name: "Microsoft ADPCM (saturating step size)",
            inst_func: |params, opts| Ok(Box::new(MsAdpcmDecoder::try_new(params, opts)?)),
        }]
    }

    fn reset(&mut self) {}

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        if let Err(e) = self.decode_inner(packet) {
            self.buf.clear();
            return Err(e);
        }
        Ok(self.buf.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.buf.as_audio_buffer_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_size_saturates_and_samples_stay_clamped() {
        let mut c = Channel { c1: 256, c2: 0, delta: 16, s1: 0, s2: 0 };
        // Code 8 (-8, table 768) triples the step every time: it overflowed i32 after ~20 steps.
        for _ in 0..1000 {
            c.expand(0x8);
        }
        assert_eq!(c.delta, i32::MAX / 256);
        assert_eq!(c.s1, i32::from(i16::MIN));
        assert_eq!(c.expand(0x7), i16::MAX);
    }

    /// Decodes one packet of `blocks` blocks with `fpb` frames per block.
    fn decode_blocks(channels: symphonia::core::audio::Channels, fpb: u64, blocks: u64, data: &[u8]) -> Vec<Vec<i16>> {
        let params = CodecParameters::new()
            .for_codec(CODEC_TYPE_ADPCM_MS)
            .with_sample_rate(22_050)
            .with_channels(channels)
            .with_frames_per_block(fpb)
            .with_max_frames_per_packet(fpb * blocks)
            .clone();
        let mut d = MsAdpcmDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
        let packet = Packet::new_from_slice(0, 0, fpb * blocks, data);
        let AudioBufferRef::S16(buf) = d.decode(&packet).unwrap() else { panic!("not i16") };
        (0..buf.spec().channels.count()).map(|c| buf.chan(c).to_vec()).collect()
    }

    #[test]
    fn odd_frames_per_block_decodes_the_last_frame_of_every_mono_block() {
        use symphonia::core::audio::Channels;
        // 5 frames per block: 2 header samples + 3 nibbles in 2 bytes (the last low nibble is
        // padding). Predictor 0 (c1 256, c2 0), delta 16, s1 100, s2 50; nibbles 1, 2, 3.
        let block = [0u8, 16, 0, 100, 0, 50, 0, 0x12, 0x30];
        let data = [block, block].concat();
        let out = decode_blocks(Channels::FRONT_LEFT, 5, 2, &data);
        let mut c = Channel { c1: 256, c2: 0, delta: 16, s1: 100, s2: 50 };
        let want = [50, 100, c.expand(1), c.expand(2), c.expand(3)];
        // The last frame was left at zero (a click every block); the second block starts at its
        // own offset.
        assert_ne!(want[4], 0);
        assert_eq!(out[0], [want, want].concat());
    }

    #[test]
    fn block_size_inverts_the_wav_readers_frames_per_block() {
        // symphonia: fpb = (block_align − 7·ch) × 8 / (4·ch) + 2.
        for ch in 1..=2usize {
            for align in 7 * ch + 1..600 {
                let fpb = (align - 7 * ch) * 8 / (4 * ch) + 2;
                assert_eq!(block_bytes(fpb, ch), align, "ch {ch} block_align {align}");
            }
        }
    }

    #[test]
    fn trailing_bytes_do_not_shift_later_blocks() {
        use symphonia::core::audio::Channels;
        // Two 9-byte mono blocks (5 frames) and 3 stray bytes: `len / blocks` = 10 put the second
        // block one byte late.
        let a = [0u8, 16, 0, 100, 0, 50, 0, 0x12, 0x30];
        let b = [1u8, 32, 0, 200, 0, 150, 0, 0x45, 0x60];
        let data = [&a[..], &b[..], &[0xAA, 0xBB, 0xCC]].concat();
        let out = decode_blocks(Channels::FRONT_LEFT, 5, 2, &data);
        let whole = decode_blocks(Channels::FRONT_LEFT, 5, 2, &[a, b].concat());
        assert_eq!(out, whole);
        assert_eq!(out[0][5..7], [150, 200]);
        // Stereo: 14 + 3 bytes per 5-frame block.
        let s = [0u8, 0, 16, 0, 16, 0, 100, 0, 100, 0, 50, 0, 50, 0, 0x12, 0x34, 0x56];
        let t = [0u8, 0, 16, 0, 16, 0, 7, 0, 8, 0, 9, 0, 10, 0, 0x21, 0x43, 0x65];
        let out = decode_blocks(Channels::FRONT_LEFT | Channels::FRONT_RIGHT, 5, 2, &[&s[..], &t[..], &[1]].concat());
        assert_eq!(out, decode_blocks(Channels::FRONT_LEFT | Channels::FRONT_RIGHT, 5, 2, &[s, t].concat()));
        assert_eq!((out[0][5], out[0][6], out[1][5], out[1][6]), (9, 7, 10, 8));
    }

    #[test]
    fn normal_steps_follow_the_ms_adpcm_formula() {
        let mut c = Channel { c1: 512, c2: -256, delta: 100, s1: 1000, s2: 900 };
        // (1000×512 − 900×256) / 256 = 1100; + 4 × 100 = 1500; delta = 100 × 307 / 256 = 119.
        assert_eq!(c.expand(4), 1500);
        assert_eq!((c.s1, c.s2, c.delta), (1500, 1000, 119));
        // Code 0xE = −2: (1500×512 − 1000×256) / 256 = 2000; − 2 × 119 = 1762; delta = 119 × 230 / 256 = 106.
        assert_eq!(c.expand(0xE), 1762);
        assert_eq!(c.delta, 106);
    }

    /// Only 4-bit codes: a source that says another size is refused, and the WAV reader refuses a
    /// file whose `bits_per_sample` is not 4 before the decoder is made.
    #[test]
    fn only_4_bit_samples_are_decoded() {
        use symphonia::core::audio::Channels;
        let params = |bits: Option<u32>| {
            let mut p = CodecParameters::new();
            p.for_codec(CODEC_TYPE_ADPCM_MS).with_sample_rate(22_050).with_channels(Channels::FRONT_LEFT).with_frames_per_block(5).with_max_frames_per_packet(5);
            if let Some(b) = bits {
                p.with_bits_per_coded_sample(b);
            }
            p
        };
        let made = |bits| MsAdpcmDecoder::try_new(&params(bits), &DecoderOptions::default()).is_ok();
        assert_eq!((made(None), made(Some(4)), made(Some(3)), made(Some(8))), (true, true, false, false));
        let mut wav = crate::sound::test_files::ms_adpcm_wav(2, 1, 0x11);
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 4, "bits_per_sample offset");
        assert!(crate::sound::decode(wav.clone(), Some("wav")).is_ok());
        wav[34] = 3;
        assert!(crate::sound::decode(wav, Some("wav")).is_err());
    }
}

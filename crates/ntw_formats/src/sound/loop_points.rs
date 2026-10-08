//! Loop points: the byte offsets of `loop_start_block` / `loop_end_block` mapped to sample
//! frames of the decoded sound.
//!
//! The exe hands the two event parameters straight to Miles' `AIL_set_sample_loop_block` /
//! `AIL_set_stream_loop_block` (`0x01004430`), whose arguments are byte offsets into the sound
//! file's data, not sample frames. CONFIRMED by the shipped data: every looped music file's
//! `loop_end_block` is 92-99.98 % of its file size in bytes but only 42-91 % of its decoded
//! length in frames, and every in-range offset is exactly the byte position of an MPEG frame
//! header (the common `loop_start_block` 1044 is the second frame of the 320 kbit/s files).
//! See `analysis/audio/AUDIO_FORMAT.md` §8.
//!
//! - MP3: offsets count from the first byte of the file; WAV: from the first byte of the
//!   `data` chunk (Miles' sample buffer). A frame there is one MPEG frame, or one WAV block
//!   (`block_align` bytes; an IMA-ADPCM block is `samples_per_block` frames).
//! - An offset inside a frame or block maps to the start of that frame or block, for both
//!   formats. PROVISIONAL: what Miles does with a mid-frame offset is unknown; every shipped
//!   offset is a frame start, so vanilla data never depends on it (no shipped WAV has a
//!   usable loop block).
//!
//! [`LoopIndex`] is built once per file (one frame walk) and maps any number of regions.

use std::fmt;

/// One MPEG audio frame of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp3Frame {
    /// Byte offset of the frame header in the file.
    pub offset: u64,
    /// Frame length in bytes (header included).
    pub len: u32,
    /// PCM frames the frame decodes to (1152, 576 or 384).
    pub samples: u32,
    /// The frame is a Xing / Info / VBRI tag frame, which decoders skip (no samples out).
    pub tag: bool,
}

/// The frames of an MP3 file, in order, found the way our decoder (symphonia 0.5.5) finds
/// them: after a leading ID3v2 tag, the first frame is the first sync word whose header
/// parses and whose next header (if any) has the same version, layer, rate and channel count;
/// after that, junk between frames is skipped by scanning for the next sync word with a
/// plausible header (a header that passes the quick check but does not parse, e.g. free
/// bitrate, is skipped whole). The walk ends where a frame would run past the end of the file,
/// even a false sync word's: the decoder's frame read hits the end of the file there and stops
/// too (test `false_sync_near_the_end_stops_walk_and_decoder_alike`).
pub fn mp3_frames(bytes: &[u8]) -> Vec<Mp3Frame> {
    let mut out = Vec::new();
    let mut pos = id3v2_len(bytes);
    // First frame (the decoder's strict read).
    loop {
        let Some((at, h)) = next_header(bytes, pos) else { return out };
        let end = at + h.len as usize;
        if end > bytes.len() {
            return out;
        }
        let next = bytes.get(end..end + 4).map(|w| u32::from_be_bytes([w[0], w[1], w[2], w[3]]));
        if next.is_none_or(|w| is_similar(&h, w)) {
            push_frame(&mut out, bytes, at, &h);
            pos = end;
            break;
        }
        pos = at + 1;
    }
    // The rest (the decoder's lenient read).
    while let Some((at, h)) = next_header(bytes, pos) {
        let end = at + h.len as usize;
        if end > bytes.len() {
            break;
        }
        push_frame(&mut out, bytes, at, &h);
        pos = end;
    }
    out
}

fn push_frame(out: &mut Vec<Mp3Frame>, bytes: &[u8], at: usize, h: &Header) {
    // The decoder drops a Xing / Info / VBRI frame wherever it appears (joined files).
    let tag = is_tag_frame(&bytes[at..at + h.len as usize], h);
    out.push(Mp3Frame { offset: at as u64, len: h.len, samples: h.samples, tag });
}

/// The next frame header at or after `pos`: a sync word passing the decoder's quick check
/// (version, layer, bitrate, rate not reserved) that also parses; a word that passes the
/// check but does not parse is skipped with its 4 bytes, as the decoder does.
fn next_header(bytes: &[u8], mut pos: usize) -> Option<(usize, Header)> {
    while pos + 4 <= bytes.len() {
        let w = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        if !quick_check(w) {
            pos += 1;
            continue;
        }
        match parse_header(&bytes[pos..]) {
            Some(h) => return Some((pos, h)),
            None => pos += 4,
        }
    }
    None
}

fn quick_check(w: u32) -> bool {
    w & 0xFFE0_0000 == 0xFFE0_0000 && (w >> 19) & 3 != 1 && (w >> 17) & 3 != 0 && (w >> 12) & 0xF != 0xF && (w >> 10) & 3 != 3
}

fn is_similar(h: &Header, w: u32) -> bool {
    quick_check(w)
        && parse_header(&w.to_be_bytes()).is_some_and(|c| c.version == h.version && c.layer == h.layer && c.rate == h.rate && c.mono == h.mono)
}

/// Whether the exe sets a loop block for these event parameters: both FLOATS non-zero
/// (`0x01004824` / `0x01004838`: `UCOMISS` against 0.0 before any truncation; the values are
/// only truncated, `CVTTSS2SI`, when passed to Miles). So (0.5, x) does set a block, with start
/// 0. The end must also lie inside the data, see [`LoopIndex::region`]. CONFIRMED.
pub fn has_loop_block(start_byte: f32, end_byte: f32) -> bool {
    start_byte != 0.0 && end_byte != 0.0
}

/// Why a loop block the exe would set could not be mapped to sample frames. We loop the whole
/// file instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopError {
    /// Neither a RIFF WAV with a `data` chunk nor an MP3 with at least one frame.
    UnknownFormat,
    /// The offset lies past the last whole MPEG frame (trailing tag or a cut-off file).
    PastLastFrame { offset: u64, walked_end: u64 },
    /// A negative start. PROVISIONAL: the exe passes it to Miles; Miles' handling is unknown
    /// and no shipped event has one.
    NegativeStart { start: i64 },
    /// The end maps to a frame at or before the start. PROVISIONAL, as for `NegativeStart`.
    EndNotAfterStart { start: u64, end: u64 },
}

impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFormat => write!(f, "not a WAV or MP3 file the frame walk understands"),
            Self::PastLastFrame { offset, walked_end } => {
                write!(f, "offset {offset} is past the last whole MPEG frame (ends at byte {walked_end})")
            }
            Self::NegativeStart { start } => write!(f, "negative start offset {start}"),
            Self::EndNotAfterStart { start, end } => write!(f, "end frame {end} is not after start frame {start}"),
        }
    }
}

impl std::error::Error for LoopError {}

/// A file's frame layout for mapping loop byte offsets: built once (one frame walk), then any
/// number of [`LoopIndex::region`] calls, each a binary search.
#[derive(Debug, Clone)]
pub struct LoopIndex {
    kind: IndexKind,
}

#[derive(Debug, Clone)]
enum IndexKind {
    Mp3 {
        /// Byte offset just past each frame, ascending.
        frame_ends: Vec<u64>,
        /// PCM frames decoded before each frame (`frame_ends.len() + 1` entries; the last is
        /// the file's total).
        pcm_before: Vec<u64>,
        file_len: u64,
    },
    Wav(WavLayout),
    Unknown,
}

impl LoopIndex {
    pub fn new(bytes: &[u8]) -> Self {
        let kind = if bytes.starts_with(b"RIFF") {
            wav_layout(bytes).map_or(IndexKind::Unknown, IndexKind::Wav)
        } else {
            let frames = mp3_frames(bytes);
            if frames.is_empty() {
                IndexKind::Unknown
            } else {
                let frame_ends = frames.iter().map(|f| f.offset + u64::from(f.len)).collect();
                let mut pcm_before = Vec::with_capacity(frames.len() + 1);
                let mut pcm = 0u64;
                pcm_before.push(0);
                for f in &frames {
                    if !f.tag {
                        pcm += u64::from(f.samples);
                    }
                    pcm_before.push(pcm);
                }
                IndexKind::Mp3 { frame_ends, pcm_before, file_len: bytes.len() as u64 }
            }
        };
        Self { kind }
    }

    /// Length of Miles' sample buffer in bytes: the whole file for MP3, the `data` chunk for WAV.
    pub fn data_len(&self) -> u64 {
        match &self.kind {
            IndexKind::Mp3 { file_len, .. } => *file_len,
            IndexKind::Wav(w) => w.data_len,
            IndexKind::Unknown => 0,
        }
    }

    /// The decoded length in sample frames when the walk knows it: an MP3's frames (the timed
    /// decode's length, equal on every vanilla looped file, see the install test). `None` for a
    /// WAV, whose header gives the decoder its length.
    pub fn decoded_frames(&self) -> Option<u64> {
        match &self.kind {
            IndexKind::Mp3 { pcm_before, .. } => pcm_before.last().copied(),
            IndexKind::Wav(_) | IndexKind::Unknown => None,
        }
    }

    /// Maps a loop byte offset to a sample frame of the decoded file (`decode_timed`
    /// output: every audio frame, tag frames skipped): the first sample of the frame or block
    /// holding the offset (junk between frames belongs to the frame after it).
    pub fn byte_to_frame(&self, offset: u64) -> Result<u64, LoopError> {
        match &self.kind {
            IndexKind::Wav(w) => Ok(offset.min(w.data_len) / w.block_align * w.frames_per_block),
            IndexKind::Unknown => Err(LoopError::UnknownFormat),
            IndexKind::Mp3 { frame_ends, pcm_before, .. } => {
                let walked_end = frame_ends.last().copied().unwrap_or(0);
                if offset > walked_end {
                    return Err(LoopError::PastLastFrame { offset, walked_end });
                }
                // The first frame ending after the offset holds it (or starts at it).
                Ok(pcm_before[frame_ends.partition_point(|&e| e <= offset)])
            }
        }
    }

    /// The loop region in decoded sample frames, following the exe's order (`0x01004430`):
    /// 1. both float parameters non-zero ([`has_loop_block`]), else no block;
    /// 2. `end < buffer length`, compared as floats with the length converted from unsigned
    ///    (`0x0100484B`..`0x01004869`), else no block;
    /// 3. both truncated to int and passed to Miles.
    ///
    /// `Ok(None)`: the exe sets no block and the whole file loops. `Ok(Some((s, 0)))`: loop from
    /// `s` at the end of the file. A negative end passes step 2 (it is below any length) and
    /// reaches Miles, whose loop-block convention is that -1 means the end of the sample, so a
    /// negative end loops at the file end (INFERRED from the Miles API convention; no shipped
    /// event has one). `Err`: a block the exe would set that we cannot map.
    pub fn region(&self, start_byte: f32, end_byte: f32) -> Result<Option<(u64, u64)>, LoopError> {
        if !has_loop_block(start_byte, end_byte) {
            return Ok(None);
        }
        if matches!(self.kind, IndexKind::Unknown) {
            return Err(LoopError::UnknownFormat);
        }
        if end_byte >= self.data_len() as f32 {
            return Ok(None);
        }
        let (start, end) = (start_byte as i64, end_byte as i64);
        if start < 0 {
            return Err(LoopError::NegativeStart { start });
        }
        let s = self.byte_to_frame(start as u64)?;
        if end < 0 {
            return Ok(Some((s, 0)));
        }
        let e = self.byte_to_frame(end as u64)?;
        if e <= s {
            return Err(LoopError::EndNotAfterStart { start: s, end: e });
        }
        Ok(Some((s, e)))
    }
}

#[derive(Clone, Copy)]
struct Header {
    version: u32,
    rate: u32,
    len: u32,
    samples: u32,
    mpeg1: bool,
    mono: bool,
    layer: u8,
}

fn parse_header(b: &[u8]) -> Option<Header> {
    let h = u32::from_be_bytes(b.get(..4)?.try_into().ok()?);
    if h >> 21 != 0x7FF {
        return None;
    }
    let version = (h >> 19) & 3; // 0 = 2.5, 2 = 2, 3 = 1
    let layer = match (h >> 17) & 3 {
        1 => 3u8,
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let br_idx = ((h >> 12) & 0xF) as usize;
    let sr_idx = ((h >> 10) & 3) as usize;
    if version == 1 || br_idx == 0 || br_idx == 15 || sr_idx == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    const BR_V1: [[u32; 15]; 3] = [
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
    ];
    const BR_V2: [[u32; 15]; 2] = [
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
    ];
    let kbps = if mpeg1 { BR_V1[layer as usize - 1][br_idx] } else { BR_V2[usize::from(layer != 1)][br_idx] };
    let rate = [44_100u32, 48_000, 32_000][sr_idx] >> match version {
        3 => 0,
        2 => 1,
        _ => 2,
    };
    let pad = (h >> 9) & 1;
    let bps = kbps * 1000;
    let (len, samples) = match layer {
        1 => ((12 * bps / rate + pad) * 4, 384),
        2 => (144 * bps / rate + pad, 1152),
        _ if mpeg1 => (144 * bps / rate + pad, 1152),
        _ => (72 * bps / rate + pad, 576),
    };
    let mono = (h >> 6) & 3 == 3;
    (len >= 4).then_some(Header { version, rate, len, samples, mpeg1, mono, layer })
}

/// A Xing / Info (after the side info) or VBRI (at byte 36) tag in a Layer III frame.
///
/// The CRC-protection bit is deliberately ignored: the walk must drop exactly the frames our
/// decoder (symphonia 0.5.5) drops, and it looks for the tag at header + side info whether or
/// not 2 CRC bytes follow the header (LAME writes the tag after them). A CRC-protected tag
/// frame is therefore decoded as audio and counted here too (test
/// `walk_matches_decoder_with_and_without_crc`). The loop offsets still land on the same audio
/// either way, since both sides use the same timeline.
fn is_tag_frame(frame: &[u8], h: &Header) -> bool {
    if h.layer != 3 {
        return false;
    }
    let side = match (h.mpeg1, h.mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    };
    let at = |o: usize, s: &[u8]| frame.get(o..o + 4) == Some(s);
    at(4 + side, b"Xing") || at(4 + side, b"Info") || at(36, b"VBRI")
}

fn id3v2_len(b: &[u8]) -> usize {
    if b.len() < 10 || &b[..3] != b"ID3" {
        return 0;
    }
    let size = b[6..10].iter().fold(0usize, |a, &x| (a << 7) | usize::from(x & 0x7F));
    let footer = if b[5] & 0x10 != 0 { 10 } else { 0 };
    10 + size + footer
}

#[derive(Debug, Clone, Copy)]
struct WavLayout {
    data_len: u64,
    block_align: u64,
    /// Frames per block (1 for PCM).
    frames_per_block: u64,
}

fn wav_layout(b: &[u8]) -> Option<WavLayout> {
    let u16_at = |o: usize| b.get(o..o + 2).map(|s| u64::from(u16::from_le_bytes([s[0], s[1]])));
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u64::from(u32::from_le_bytes([s[0], s[1], s[2], s[3]])));
    let mut pos = 12usize;
    let (mut format, mut channels, mut block_align) = (0, 1, 0);
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let len = u32_at(pos + 4)?;
        let body = pos + 8;
        if id == b"fmt " {
            format = u16_at(body)?;
            channels = u16_at(body + 2)?.max(1);
            block_align = u16_at(body + 12)?;
        } else if id == b"data" {
            let data_len = len.min((b.len() - body) as u64);
            // Frames per block as our decoder (symphonia 0.5.5) computes them from the block
            // size, so offsets land on its timeline (the fmt chunk's own samples-per-block field
            // is not used). 4-bit samples; header 4 bytes per channel (IMA, then 1 sample) or 7
            // (MS, then 2 samples).
            let frames_per_block = match format {
                0x11 => block_align.saturating_sub(4 * channels) * 8 / (4 * channels) + 1,
                0x02 => block_align.saturating_sub(7 * channels) * 8 / (4 * channels) + 2,
                _ => 1,
            };
            return (block_align > 0).then_some(WavLayout { data_len, block_align, frames_per_block });
        }
        pos = body + len as usize + (len as usize & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::decode;
    use crate::sound::test_files::{mp3, ms_adpcm_wav, wav};

    /// `n` audio frames, after an Info tag frame when `info`.
    fn mp3_info(n: usize, info: bool) -> Vec<u8> {
        mp3(n + usize::from(info), false, info.then_some(36), None)
    }

    fn frame_of(b: &[u8], offset: u64) -> Result<u64, LoopError> {
        LoopIndex::new(b).byte_to_frame(offset)
    }

    #[test]
    fn mp3_frames_walk_and_tag() {
        let b = mp3_info(5, true);
        let f = mp3_frames(&b);
        assert_eq!(f.len(), 6);
        assert!(f[0].tag && !f[1].tag);
        assert_eq!(f[0].len, 418);
        assert_eq!(f[1].offset, 418);
        assert_eq!(f[1].len, 417);
        assert!(f.iter().all(|x| x.samples == 1152));
    }

    #[test]
    fn byte_offsets_map_to_frames() {
        let b = mp3_info(5, true);
        // Right after the tag frame: the first audio sample.
        assert_eq!(frame_of(&b, 418), Ok(0));
        // The third audio frame's header.
        let f = mp3_frames(&b);
        assert_eq!(frame_of(&b, f[3].offset), Ok(2 * 1152));
        // Inside a frame: the start of that frame (PROVISIONAL rule, same as WAV blocks).
        assert_eq!(frame_of(&b, f[3].offset + 1), Ok(2 * 1152));
        // Region: only with both ends set and the end inside the file.
        let idx = LoopIndex::new(&b);
        assert_eq!(idx.region(418.0, f[4].offset as f32), Ok(Some((0, 3 * 1152))));
        assert_eq!(idx.region(0.0, f[4].offset as f32), Ok(None));
        assert_eq!(idx.region(418.0, 0.0), Ok(None));
        assert_eq!(idx.region(418.0, b.len() as f32), Ok(None));
        // The exe tests the floats before truncating: 0.5 is a block starting at byte 0.
        assert_eq!(idx.region(0.5, f[4].offset as f32), Ok(Some((0, 3 * 1152))));
        // A negative end passes the exe's length test; Miles' -1 = end of sample (INFERRED).
        assert_eq!(idx.region(418.0, -1.0), Ok(Some((0, 0))));
        // PROVISIONAL refusals, with their reason.
        assert!(matches!(idx.region(-5.0, 900.0), Err(LoopError::NegativeStart { .. })));
        assert!(matches!(idx.region(900.0, 418.0), Err(LoopError::EndNotAfterStart { .. })));
    }

    #[test]
    fn walk_resyncs_over_junk_like_the_decoder() {
        // Junk before the first frame and between frames.
        let mut b = vec![0x55u8; 37];
        b.extend_from_slice(&mp3_info(3, false));
        let junk_at = b.len() as u64;
        b.extend_from_slice(&[0u8; 50]);
        b.extend_from_slice(&mp3_info(3, false));
        let f = mp3_frames(&b);
        assert_eq!(f.len(), 6);
        assert_eq!(f[0].offset, 37);
        assert_eq!(f[3].offset, junk_at + 50);
        assert_eq!(frame_of(&b, junk_at + 50), Ok(3 * 1152));
        // Junk belongs to the frame after it.
        assert_eq!(frame_of(&b, junk_at + 10), Ok(3 * 1152));
        let decoded = decode(b.clone(), Some("mp3")).unwrap().frames() as u64;
        assert_eq!(decoded, 6 * 1152);
    }

    #[test]
    fn false_sync_near_the_end_stops_walk_and_decoder_alike() {
        // A sync word whose frame would run past the end: the decoder's frame read hits the end
        // of the file there and stops, so the walk stops too (it does not skip a byte and look
        // for later frames the decoder never reaches).
        let mut b = mp3_info(3, false);
        let walked = b.len() as u64;
        b.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        b.extend_from_slice(&[0u8; 20]);
        assert_eq!(mp3_frames(&b).len(), 3);
        assert_eq!(decode(b.clone(), Some("mp3")).unwrap().frames(), 3 * 1152);
        assert_eq!(frame_of(&b, walked), Ok(3 * 1152));
    }

    #[test]
    fn offsets_past_the_last_frame_are_errors() {
        let mut b = mp3_info(3, false);
        let walked = b.len() as u64;
        b.extend_from_slice(b"TAG");
        b.extend_from_slice(&[0u8; 125]);
        assert_eq!(frame_of(&b, walked), Ok(3 * 1152));
        assert_eq!(frame_of(&b, walked + 60), Err(LoopError::PastLastFrame { offset: walked + 60, walked_end: walked }));
        assert!(matches!(LoopIndex::new(&b).region(418.0, (walked + 60) as f32), Err(LoopError::PastLastFrame { .. })));
        assert_eq!(LoopIndex::new(b"not audio at all").region(1.0, 2.0), Err(LoopError::UnknownFormat));
    }

    #[test]
    fn ms_adpcm_blocks_match_the_decoder() {
        for ch in [1u16, 2] {
            let b = ms_adpcm_wav(6, ch, 0x11);
            let fpb = u64::from((256 - 7 * ch) * 2 / ch + 2);
            let idx = LoopIndex::new(&b);
            assert_eq!(idx.byte_to_frame(256), Ok(fpb), "{ch} ch");
            assert_eq!(idx.byte_to_frame(3 * 256 + 100), Ok(3 * fpb), "{ch} ch");
            let decoded = decode(b.clone(), Some("wav")).unwrap().frames() as u64;
            assert_eq!(idx.byte_to_frame(idx.data_len()), Ok(decoded), "{ch} ch");
        }
    }

    #[test]
    fn wav_offsets_are_data_relative_blocks() {
        // 16-bit stereo PCM: block_align 4, 400 data bytes.
        let b = wav(&[0; 200], 44_100, 2);
        let idx = LoopIndex::new(&b);
        assert_eq!(idx.byte_to_frame(40), Ok(10));
        assert_eq!(idx.region(4.0, 200.0), Ok(Some((1, 50))));
        // Inside a block: the start of that block.
        assert_eq!(idx.byte_to_frame(43), Ok(10));
        // End past the data chunk: the exe sets no loop block.
        assert_eq!(idx.region(4.0, 400.0), Ok(None));
    }

    /// The frame walk must reproduce the decoder's timeline exactly, tag frames included.
    /// Includes the CRC-protected cases: the decoder (symphonia 0.5.5) looks for the tag right
    /// after the side info and ignores the 2 CRC bytes, so "Info" after the CRC (byte 38) is
    /// decoded as an audio frame. The walk follows the decoder, not the LAME layout.
    #[test]
    fn walk_matches_decoder_with_and_without_crc() {
        for (crc, at, frames) in [(false, None, 10), (false, Some(36), 9), (true, None, 10), (true, Some(36), 9), (true, Some(38), 10)] {
            let b = mp3(10, crc, at, None);
            let decoded = decode(b.clone(), Some("mp3")).unwrap().frames() as u64;
            let walked: u64 = mp3_frames(&b).iter().filter(|f| !f.tag).map(|f| u64::from(f.samples)).sum();
            assert_eq!(decoded, frames * 1152, "crc={crc} at={at:?}");
            assert_eq!(walked, decoded, "crc={crc} at={at:?}");
        }
    }
}

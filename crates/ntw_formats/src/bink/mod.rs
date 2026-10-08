//! Bink 1 video (`.bik`), recreated as our own pure-Rust decoder.
//!
//! The original game plays its movies through RAD Game Tools' `binkw32.dll` (shipped in the
//! install). NapoleonRust never loads or calls that DLL. This module is our own decoder, written
//! from the format notes in `analysis/video/BINK.md` (public format descriptions plus Ghidra
//! analysis of the DLL for the details and constant tables).
//!
//! Every shipped movie is in `media.pack` under `movies\` and is read through the
//! [`Vfs`](crate::pack::Vfs), so mods that replace movies work. All 61 shipped files are revision
//! `'i'`, 1280x720 at 30 fps (CONFIRMED by the survey in `BINK.md` §1).
//!
//! | piece | module |
//! |---|---|
//! | container: header, audio track table, frame index, frame packets | this file |
//! | LSB-first bit reader | [`bits`] |
//! | video: bundles, Huffman trees, block types, DCT, motion compensation | [`video`] |
//! | audio: Bink audio (DCT and RDFT variants) | [`audio`] |
//! | constant tables (scan orders, quantisers, patterns, trees) | [`tables`] |

pub mod audio;
pub mod bits;
mod dct;
pub mod tables;
pub mod video;

pub use audio::AudioDecoder;
pub use video::{Frame, VideoDecoder, YuvToRgb};

/// Errors from the Bink reader and decoders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinkError {
    /// The file does not start with `BIK`.
    BadMagic([u8; 4]),
    /// A revision letter this decoder does not handle (Bink 2 `KB2`, or a very old Bink 1).
    UnsupportedRevision(u8),
    /// The header or frame index runs past the end of the data given.
    Truncated { what: &'static str },
    /// A header field is out of range.
    BadHeader(&'static str),
    /// A frame's bitstream is malformed (reads past its end, impossible values, ...).
    BadFrame { frame: u32, what: String },
}

impl std::fmt::Display for BinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinkError::BadMagic(m) => write!(f, "not a Bink file (magic {m:02x?})"),
            BinkError::UnsupportedRevision(r) => write!(f, "unsupported Bink revision {:?}", *r as char),
            BinkError::Truncated { what } => write!(f, "Bink data truncated in {what}"),
            BinkError::BadHeader(w) => write!(f, "bad Bink header: {w}"),
            BinkError::BadFrame { frame, what } => write!(f, "bad Bink frame {frame}: {what}"),
        }
    }
}

impl std::error::Error for BinkError {}

/// Header video flag: the file has an alpha plane (CONFIRMED meaning from format notes; no
/// shipped movie sets it).
pub const FLAG_ALPHA: u32 = 0x0010_0000;
/// Header video flag: grey-scale (no chroma planes). No shipped movie sets it.
pub const FLAG_GRAY: u32 = 0x0002_0000;

/// Audio track flag: the track uses the DCT transform (otherwise the RDFT one).
pub const AUDIO_FLAG_DCT: u16 = 0x1000;
/// Audio track flag: two channels.
pub const AUDIO_FLAG_STEREO: u16 = 0x2000;
/// Audio track flag: 16-bit output (always set in the shipped files).
pub const AUDIO_FLAG_16BIT: u16 = 0x4000;

/// One audio track's description from the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioTrackInfo {
    /// Largest decoded size of one packet in bytes (header field; used for buffer sizing).
    pub max_decoded_size: u32,
    /// Samples per second.
    pub sample_rate: u16,
    /// [`AUDIO_FLAG_DCT`], [`AUDIO_FLAG_STEREO`], [`AUDIO_FLAG_16BIT`].
    pub flags: u16,
    /// The track's id (the game picks a track by id, e.g. a language).
    pub id: u32,
}

impl AudioTrackInfo {
    /// 1 or 2.
    pub fn channels(&self) -> u16 {
        if self.flags & AUDIO_FLAG_STEREO != 0 { 2 } else { 1 }
    }
    /// True for the DCT variant of Bink audio.
    pub fn uses_dct(&self) -> bool {
        self.flags & AUDIO_FLAG_DCT != 0
    }
}

/// Where one frame's packet lives in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameEntry {
    /// Byte offset from the start of the file.
    pub offset: u32,
    /// Packet size in bytes.
    pub size: u32,
    /// The frame can be decoded without the previous one.
    pub keyframe: bool,
}

/// A parsed `.bik` header and frame index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinkHeader {
    /// Revision letter after `BIK` (`b'i'` in every shipped file).
    pub revision: u8,
    /// Total file size in bytes (the header stores size - 8).
    pub file_size: u32,
    /// Number of video frames.
    pub num_frames: u32,
    /// Largest frame packet in bytes.
    pub largest_frame: u32,
    pub width: u32,
    pub height: u32,
    /// Frame rate = `fps_num / fps_den` frames per second.
    pub fps_num: u32,
    pub fps_den: u32,
    /// Video flags ([`FLAG_ALPHA`], [`FLAG_GRAY`]).
    pub flags: u32,
    pub audio_tracks: Vec<AudioTrackInfo>,
    /// One entry per frame.
    pub frames: Vec<FrameEntry>,
}

fn rd32(b: &[u8], o: usize, what: &'static str) -> Result<u32, BinkError> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or(BinkError::Truncated { what })
}

fn rd16(b: &[u8], o: usize, what: &'static str) -> Result<u16, BinkError> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(BinkError::Truncated { what })
}

impl BinkHeader {
    /// Size of the fixed part of the header (before the audio tables).
    pub const FIXED_LEN: usize = 44;

    /// How many bytes [`BinkHeader::parse`] needs, from the first [`Self::FIXED_LEN`] bytes.
    pub fn needed_len(fixed: &[u8]) -> Result<usize, BinkError> {
        let frames = rd32(fixed, 8, "header")? as usize;
        let tracks = rd32(fixed, 40, "header")? as usize;
        if tracks > 256 || frames > 10_000_000 {
            return Err(BinkError::BadHeader("frame or track count"));
        }
        Ok(Self::FIXED_LEN + 12 * tracks + 4 * (frames + 1))
    }

    /// Parses the header and frame index (`bytes` may be the whole file or just its start).
    pub fn parse(bytes: &[u8]) -> Result<Self, BinkError> {
        let magic: [u8; 4] = bytes.get(..4).ok_or(BinkError::Truncated { what: "magic" })?.try_into().unwrap();
        if &magic[..3] != b"BIK" {
            return Err(BinkError::BadMagic(magic));
        }
        let revision = magic[3];
        // Bink 1 revisions b..k exist; this decoder follows the 'i' bitstream (the shipped one) and
        // the closely related 'f'..'k' variants.
        if !(b'f'..=b'k').contains(&revision) {
            return Err(BinkError::UnsupportedRevision(revision));
        }
        let file_size = rd32(bytes, 4, "header")?.wrapping_add(8);
        let num_frames = rd32(bytes, 8, "header")?;
        let largest_frame = rd32(bytes, 12, "header")?;
        let width = rd32(bytes, 20, "header")?;
        let height = rd32(bytes, 24, "header")?;
        let fps_num = rd32(bytes, 28, "header")?;
        let fps_den = rd32(bytes, 32, "header")?;
        let flags = rd32(bytes, 36, "header")?;
        let ntracks = rd32(bytes, 40, "header")? as usize;
        if width == 0 || height == 0 || width > 7680 || height > 4800 {
            return Err(BinkError::BadHeader("size"));
        }
        if fps_num == 0 || fps_den == 0 {
            return Err(BinkError::BadHeader("frame rate"));
        }
        let need = Self::needed_len(bytes)?;
        if bytes.len() < need {
            return Err(BinkError::Truncated { what: "frame index" });
        }
        let mut o = Self::FIXED_LEN;
        let mut audio_tracks = Vec::with_capacity(ntracks);
        for i in 0..ntracks {
            let max_decoded_size = rd32(bytes, o + 4 * i, "audio table")?;
            let sample_rate = rd16(bytes, o + 4 * ntracks + 4 * i, "audio table")?;
            let flags = rd16(bytes, o + 4 * ntracks + 4 * i + 2, "audio table")?;
            let id = rd32(bytes, o + 8 * ntracks + 4 * i, "audio table")?;
            audio_tracks.push(AudioTrackInfo { max_decoded_size, sample_rate, flags, id });
        }
        o += 12 * ntracks;
        let mut frames = Vec::with_capacity(num_frames as usize);
        let mut prev = rd32(bytes, o, "frame index")?;
        for i in 0..num_frames as usize {
            let next = rd32(bytes, o + 4 * (i + 1), "frame index")?;
            let (start, end) = (prev & !1, next & !1);
            if end < start || end > file_size {
                return Err(BinkError::BadHeader("frame index offsets"));
            }
            frames.push(FrameEntry { offset: start, size: end - start, keyframe: prev & 1 != 0 });
            prev = next;
        }
        Ok(Self {
            revision,
            file_size,
            num_frames,
            largest_frame,
            width,
            height,
            fps_num,
            fps_den,
            flags,
            audio_tracks,
            frames,
        })
    }

    /// Frames per second as a float.
    pub fn fps(&self) -> f64 {
        self.fps_num as f64 / self.fps_den as f64
    }

    /// Running time in seconds (`num_frames / fps`).
    pub fn duration_secs(&self) -> f64 {
        self.num_frames as f64 * self.fps_den as f64 / self.fps_num as f64
    }

    pub fn has_alpha(&self) -> bool {
        self.flags & FLAG_ALPHA != 0
    }
}

/// One frame packet split into its parts.
#[derive(Debug, Clone, Copy)]
pub struct FramePacket<'a> {
    /// Audio packet per header track (empty slice when the track has no data this frame).
    /// Each non-empty packet starts with a u32 decoded byte count, then the bitstream.
    pub audio: [&'a [u8]; 16],
    /// Number of valid entries in `audio`.
    pub num_audio: usize,
    /// The video bitstream.
    pub video: &'a [u8],
}

/// Splits a frame packet: one `u32 size; size bytes` block per audio track, then video.
pub fn split_packet<'a>(packet: &'a [u8], num_tracks: usize) -> Result<FramePacket<'a>, BinkError> {
    if num_tracks > 16 {
        return Err(BinkError::BadHeader("more than 16 audio tracks"));
    }
    let mut audio: [&[u8]; 16] = [&[]; 16];
    let mut o = 0usize;
    for slot in audio.iter_mut().take(num_tracks) {
        let n = rd32(packet, o, "audio packet size")? as usize;
        o += 4;
        let s = packet.get(o..o + n).ok_or(BinkError::Truncated { what: "audio packet" })?;
        *slot = s;
        o += n;
    }
    Ok(FramePacket { audio, num_audio: num_tracks, video: &packet[o..] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_header(frames: &[(u32, bool)], tracks: usize) -> Vec<u8> {
        let mut b = b"BIKi".to_vec();
        let n = frames.len() as u32;
        let hdr_len = BinkHeader::FIXED_LEN + 12 * tracks + 4 * (frames.len() + 1);
        let total: u32 = hdr_len as u32 + frames.iter().map(|f| f.0).sum::<u32>();
        for v in [total - 8, n, 100, n, 64, 32, 30, 1, 0, tracks as u32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..tracks {
            b.extend_from_slice(&1000u32.to_le_bytes());
        }
        for _ in 0..tracks {
            b.extend_from_slice(&48000u16.to_le_bytes());
            b.extend_from_slice(&0x7000u16.to_le_bytes());
        }
        for i in 0..tracks {
            b.extend_from_slice(&(i as u32).to_le_bytes());
        }
        let mut off = hdr_len as u32;
        for &(size, key) in frames {
            b.extend_from_slice(&(off | key as u32).to_le_bytes());
            off += size;
        }
        b.extend_from_slice(&off.to_le_bytes());
        b
    }

    #[test]
    fn parses_header_and_index() {
        let b = tiny_header(&[(40, true), (12, false)], 2);
        let h = BinkHeader::parse(&b).unwrap();
        assert_eq!((h.revision, h.width, h.height, h.num_frames), (b'i', 64, 32, 2));
        assert_eq!(h.audio_tracks.len(), 2);
        assert_eq!(h.audio_tracks[1].id, 1);
        assert!(h.audio_tracks[0].uses_dct() && h.audio_tracks[0].channels() == 2);
        assert!(h.frames[0].keyframe && !h.frames[1].keyframe);
        assert_eq!(h.frames[1].size, 12);
        assert_eq!(h.frames[1].offset, h.frames[0].offset + 40);
        assert_eq!(BinkHeader::needed_len(&b).unwrap(), b.len());
        assert!((h.duration_secs() - 2.0 / 30.0).abs() < 1e-9);
    }

    #[test]
    fn splits_packets() {
        let mut p = Vec::new();
        p.extend_from_slice(&8u32.to_le_bytes());
        p.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(&[9, 9]);
        let f = split_packet(&p, 2).unwrap();
        assert_eq!(f.audio[0].len(), 8);
        assert!(f.audio[1].is_empty());
        assert_eq!(f.video, &[9, 9]);
        assert!(split_packet(&p[..6], 1).is_err());
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(matches!(BinkHeader::parse(b"KB2a0000"), Err(BinkError::BadMagic(_))));
    }
}

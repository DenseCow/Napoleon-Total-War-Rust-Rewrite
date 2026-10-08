//! Decoding the game's sound files to PCM.
//!
//! The shipped sound files use three codecs (survey of every file, AUDIO_FORMAT.md §2):
//! - MPEG-1 Layer III `.mp3` (music, voices, advisor speech, many effects);
//! - RIFF `.wav` with PCM (format tag 1; 16-bit, one file 24-bit);
//! - RIFF `.wav` with IMA ADPCM (format tag 0x11), the `*_adpcm*.wav` files.
//!
//! All three are decoded by the pure-Rust `symphonia` crate. The container is
//! detected from the bytes, not the file name (a few `.mp3` files start with an
//! ID3 tag, the rest with an MPEG sync word). MS-ADPCM WAVs (format tag 2, none shipped; mods)
//! go through our own decoder ([`super::ms_adpcm`]).

use std::collections::VecDeque;
use std::fmt;
use std::io::Cursor;
use std::sync::LazyLock;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecRegistry, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::ms_adpcm::MsAdpcmDecoder;

/// Decoded audio: interleaved `f32` samples in -1..=1.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub channels: u16,
    pub sample_rate: u32,
    /// Interleaved samples (`frames * channels` values).
    pub samples: Vec<f32>,
}

impl Pcm {
    /// Number of sample frames (one value per channel each).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    /// Length in seconds.
    pub fn duration_secs(&self) -> f32 {
        self.frames() as f32 / self.sample_rate.max(1) as f32
    }
}

/// Why a sound file could not be decoded.
#[derive(Debug)]
pub enum DecodeError {
    /// The container was not recognised.
    UnknownFormat(String),
    /// The container has no audio track.
    NoTrack,
    /// The codec is not supported.
    Codec(String),
    /// Error inside the decoder.
    Decode(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFormat(e) => write!(f, "unknown audio format: {e}"),
            Self::NoTrack => write!(f, "no audio track"),
            Self::Codec(e) => write!(f, "unsupported codec: {e}"),
            Self::Decode(e) => write!(f, "decode error: {e}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Shared, immutable file bytes for a [`PcmStream`].
#[derive(Clone)]
pub struct SharedBytes(pub std::sync::Arc<[u8]>);

impl AsRef<[u8]> for SharedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// An incremental decoder: yields the file as chunks of interleaved `f32` samples.
pub struct PcmStream {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    channels: u16,
    sample_rate: u32,
    buf: Option<SampleBuffer<f32>>,
    /// Chunks ready before the next decode: the first chunk (decoded by [`PcmStream::open`] to
    /// learn the format) and audio held back behind silence for damaged frames.
    queue: VecDeque<Vec<f32>>,
    done: bool,
    /// Damaged frames play as silence of their length ([`PcmStream::open_timed`]) instead of
    /// being dropped.
    timed: bool,
    /// Frames of damaged packets since the last good one, still to be played as silence.
    owed_frames: u64,
    /// Silence for damaged frames, returned ahead of the audio held in `queue`.
    silence: Vec<f32>,
    /// The track's length in frames, when the container gives it (to size a full decode).
    n_frames: Option<u64>,
}

impl PcmStream {
    /// Opens a stream; decodes the first chunk so channels and rate are known. A damaged frame is
    /// skipped, as players do (later audio moves earlier by its length).
    pub fn open(bytes: SharedBytes, ext_hint: Option<&str>) -> Result<Self, DecodeError> {
        Self::open_with(bytes, ext_hint, false)
    }

    /// [`PcmStream::open`], but a damaged frame in mid-stream plays as silence of its length, so
    /// every later sample keeps its timestamp: for loop regions, whose frame numbers count the
    /// file's frames (`loop_points`). Damaged frames at the end of the file are still dropped.
    pub fn open_timed(bytes: SharedBytes, ext_hint: Option<&str>) -> Result<Self, DecodeError> {
        Self::open_with(bytes, ext_hint, true)
    }

    fn open_with(bytes: SharedBytes, ext_hint: Option<&str>, timed: bool) -> Result<Self, DecodeError> {
        // A header claiming more frames than the file can hold is damaged.
        let max_frames = max_frames(&bytes.0);
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        if let Some(e) = ext_hint {
            hint.with_extension(e);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| DecodeError::UnknownFormat(e.to_string()))?;
        let format = probed.format;
        let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or(DecodeError::NoTrack)?;
        let track_id = track.id;
        let channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(0);
        let sample_rate = track.codec_params.sample_rate.unwrap_or(0);
        let n_frames = track.codec_params.n_frames.map(|n| n.min(max_frames));
        let decoder = codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| DecodeError::Codec(e.to_string()))?;
        let mut s = Self {
            format,
            decoder,
            track_id,
            channels,
            sample_rate,
            buf: None,
            queue: VecDeque::new(),
            done: false,
            timed,
            owed_frames: 0,
            silence: Vec::new(),
            n_frames,
        };
        // Before anything `decode_chunk` may have queued behind it.
        if let Some(first) = s.decode_chunk()?.map(<[f32]>::to_vec) {
            s.queue.push_front(first);
        }
        if s.channels == 0 || s.sample_rate == 0 {
            return Err(DecodeError::Decode("no channel/rate information".into()));
        }
        Ok(s)
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The next chunk of interleaved samples, or `None` at the end of the file.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, DecodeError> {
        if let Some(c) = self.queue.pop_front() {
            return Ok(Some(c));
        }
        Ok(self.decode_chunk()?.map(<[f32]>::to_vec))
    }

    fn decode_chunk(&mut self) -> Result<Option<&[f32]>, DecodeError> {
        while !self.done {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.done = true;
                    break;
                }
                Err(SymError::ResetRequired) => {
                    self.done = true;
                    break;
                }
                Err(e) => return Err(DecodeError::Decode(e.to_string())),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let dur = packet.dur();
            match self.decoder.decode(&packet) {
                Ok(audio) => {
                    let spec = *audio.spec();
                    let n = audio.frames() as u64;
                    // Timed: a packet shorter than its duration (none in vanilla data) is padded
                    // with silence after it, so later frames keep their timestamps.
                    let short = timeline_gap(self.timed, dur, n);
                    if n == 0 {
                        self.owed_frames += short;
                        continue;
                    }
                    let ch = spec.channels.count();
                    self.channels = ch as u16;
                    self.sample_rate = spec.rate;
                    let need = audio.capacity() as u64 * ch as u64;
                    if self.buf.as_ref().is_none_or(|b| (b.capacity() as u64) < need) {
                        self.buf = Some(SampleBuffer::new(audio.capacity() as u64, spec));
                    }
                    let b = self.buf.as_mut().expect("just set");
                    b.copy_interleaved_ref(audio);
                    let samples = self.buf.as_ref().expect("just set").samples();
                    // Damaged frames before this one: their silence first, this audio after it.
                    if self.owed_frames > 0 {
                        let held = samples.to_vec();
                        self.queue.push_back(held);
                        self.silence.clear();
                        self.silence.resize(self.owed_frames as usize * ch, 0.0);
                        self.owed_frames = short;
                        return Ok(Some(&self.silence));
                    }
                    self.owed_frames = short;
                    return Ok(Some(samples));
                }
                // A damaged frame: skipped, or (timed streams) played as silence of its length
                // (symphonia's MP3 and WAV packets always carry it), so every later sample keeps
                // its timestamp. The silence is only owed until the next good frame: damaged
                // frames at the end of the file are dropped. PROVISIONAL: what Miles plays for a
                // frame it cannot decode is unknown.
                Err(SymError::DecodeError(_)) => {
                    self.owed_frames += timeline_gap(self.timed, dur, 0);
                    continue;
                }
                Err(e) => return Err(DecodeError::Decode(e.to_string())),
            }
        }
        Ok(None)
    }
}

/// The most sample frames `bytes` can hold: no codec here yields more than 8 frames per byte
/// (MPEG-2.5 Layer III at 8 kbit/s and 8 kHz: 576 frames in 72 bytes; ADPCM at most 2; PCM at most 1).
fn max_frames(bytes: &[u8]) -> u64 {
    (bytes.len() as u64).saturating_mul(8)
}

/// symphonia's codecs, with its MS-ADPCM decoder replaced by ours (`super::ms_adpcm`: the step size
/// saturates instead of overflowing).
fn codecs() -> &'static CodecRegistry {
    static REGISTRY: LazyLock<CodecRegistry> = LazyLock::new(|| {
        let mut r = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut r);
        r.register_all::<MsAdpcmDecoder>();
        r
    });
    &REGISTRY
}

/// Silence a timed stream owes after a packet of duration `dur` that decoded to `decoded` frames
/// (all of it for a damaged packet).
fn timeline_gap(timed: bool, dur: u64, decoded: u64) -> u64 {
    if timed { dur.saturating_sub(decoded) } else { 0 }
}

/// Decodes a whole sound file (`.mp3` or `.wav`) to PCM, skipping damaged frames.
///
/// `ext_hint` is the file extension (e.g. `"mp3"`), used only as a hint.
pub fn decode(bytes: Vec<u8>, ext_hint: Option<&str>) -> Result<Pcm, DecodeError> {
    let s = PcmStream::open(SharedBytes(bytes.into()), ext_hint)?;
    let (channels, sample_rate) = (s.channels, s.sample_rate);
    Ok(Pcm { channels, sample_rate, samples: drain(s, None, None, |x| x)? })
}

/// Decodes a sound file on its own timeline (damaged frames as silence, see
/// [`PcmStream::open_timed`]), for loop regions: up to sample frame `end` when given (a loop's
/// end; nothing after it ever plays), else the whole file.
pub fn decode_timed(bytes: Vec<u8>, ext_hint: Option<&str>, end: Option<u64>) -> Result<Pcm, DecodeError> {
    let s = PcmStream::open_timed(SharedBytes(bytes.into()), ext_hint)?;
    let (channels, sample_rate) = (s.channels, s.sample_rate);
    Ok(Pcm { channels, sample_rate, samples: drain(s, end, None, |x| x)? })
}

/// Decoded audio as interleaved 16-bit samples: half the memory of [`Pcm`], for sounds held
/// decoded for a long time (music). Miles mixes 16-bit samples too.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm16 {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

impl Pcm16 {
    /// Number of sample frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }
}

/// `f32` sample to 16-bit, rounded and clamped.
pub fn to_i16(x: f32) -> i16 {
    (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16
}

/// The whole file on its own timeline ([`decode_timed`]) as 16-bit samples, converted chunk by chunk
/// (no full `f32` copy): music, held decoded. `frames` is the decoded length when known
/// beforehand (`LoopIndex::decoded_frames`, the MP3 frame walk, equal to the decoder on every
/// vanilla file), so the buffer is allocated once at its final size; without it the container's
/// frame count sizes it (an MP3 has none unless it carries a Xing/VBRI tag).
pub fn decode_timed_i16(bytes: Vec<u8>, ext_hint: Option<&str>, frames: Option<u64>) -> Result<Pcm16, DecodeError> {
    let frames = frames.map(|n| n.min(max_frames(&bytes)));
    let s = PcmStream::open_timed(SharedBytes(bytes.into()), ext_hint)?;
    let (channels, sample_rate) = (s.channels, s.sample_rate);
    Ok(Pcm16 { channels, sample_rate, samples: drain(s, None, frames, to_i16)? })
}

/// Collects a stream's samples, up to frame `end` when given. The buffer is sized up front to `end`,
/// else `reserve` frames, else the container's frame count (all bounded by the file size).
fn drain<T: Copy>(mut s: PcmStream, end: Option<u64>, reserve: Option<u64>, convert: impl Fn(f32) -> T) -> Result<Vec<T>, DecodeError> {
    let ch = usize::from(s.channels.max(1));
    let limit = end.map(|e| (e as usize).saturating_mul(ch));
    let known = limit.or(reserve.or(s.n_frames).map(|n| (n as usize).saturating_mul(ch)));
    let mut samples = Vec::new();
    // A failed reservation just grows the buffer instead.
    let _ = samples.try_reserve_exact(known.unwrap_or(0));
    while let Some(chunk) = s.next_chunk()? {
        let take = limit.map_or(chunk.len(), |l| chunk.len().min(l - samples.len()));
        samples.extend(chunk[..take].iter().map(|&x| convert(x)));
        if limit.is_some_and(|l| samples.len() >= l) {
            break;
        }
    }
    // Unknown length, or a header that over-claimed it.
    if samples.capacity() > samples.len() + samples.len() / 8 {
        samples.shrink_to_fit();
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound::test_files::{ima_adpcm_wav, mp3, ms_adpcm_wav, wav};

    #[test]
    fn decodes_pcm_wav() {
        let pcm = decode(wav(&[0, 16384, -16384, 32767], 22050, 1), Some("wav")).unwrap();
        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.sample_rate, 22050);
        assert_eq!(pcm.samples.len(), 4);
        assert!((pcm.samples[1] - 0.5).abs() < 1e-3);
        assert!((pcm.samples[2] + 0.5).abs() < 1e-3);
    }

    #[test]
    fn decodes_ima_adpcm_wav() {
        assert_eq!(decode(ima_adpcm_wav(12), Some("wav")).unwrap().frames(), 12 * 505);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(vec![1, 2, 3, 4, 5], None).is_err());
    }

    fn timed(bytes: Vec<u8>, end: Option<u64>) -> Pcm {
        decode_timed(bytes, Some("mp3"), end).unwrap()
    }

    #[test]
    fn timed_decode_keeps_a_damaged_frame_as_silence() {
        for bad in [0, 20] {
            let got = timed(mp3(60, false, None, Some(bad)), None);
            assert_eq!(got.frames(), 60 * 1152, "bad frame {bad}");
            assert!(got.samples[bad * 1152 * 2..(bad + 1) * 1152 * 2].iter().all(|&x| x == 0.0));
        }
    }

    #[test]
    fn timed_decode_stops_at_the_loop_end() {
        let full = timed(mp3(60, false, None, Some(30)), None);
        let cut = timed(mp3(60, false, None, Some(30)), Some(40 * 1152 + 7));
        assert_eq!(cut.samples, full.samples[..(40 * 1152 + 7) * 2]);
        // An end past the audio: the whole file.
        assert_eq!(timed(mp3(60, false, None, None), Some(100 * 1152)).frames(), 60 * 1152);
    }

    #[test]
    fn full_decode_skips_a_damaged_frame() {
        // One-shot sounds: no silence for a damaged leading frame (the sound does not start late).
        for bad in [0, 20] {
            assert_eq!(decode(mp3(60, false, None, Some(bad)), Some("mp3")).unwrap().frames(), 59 * 1152, "bad frame {bad}");
        }
    }

    #[test]
    fn damaged_or_truncated_last_frame_adds_no_silence() {
        assert_eq!(timed(mp3(10, false, None, Some(9)), None).frames(), 9 * 1152);
        let mut cut = mp3(10, false, None, None);
        cut.truncate(cut.len() - 100);
        assert_eq!(timed(cut.clone(), None).frames(), 9 * 1152);
        assert_eq!(decode(cut, Some("mp3")).unwrap().frames(), 9 * 1152);
    }

    #[test]
    fn a_short_or_empty_timed_packet_owes_its_missing_frames() {
        assert_eq!(timeline_gap(true, 1152, 0), 1152);
        assert_eq!(timeline_gap(true, 1152, 1000), 152);
        assert_eq!(timeline_gap(true, 1152, 1152), 0);
        assert_eq!(timeline_gap(false, 1152, 0), 0);
    }

    #[test]
    fn i16_decode_matches_the_f32_decode() {
        let bytes = mp3(60, false, None, Some(30));
        let f = decode_timed(bytes.clone(), Some("mp3"), None).unwrap();
        let i = decode_timed_i16(bytes, Some("mp3"), None).unwrap();
        assert_eq!((i.channels, i.sample_rate, i.frames()), (f.channels, f.sample_rate, f.frames()));
        assert!(i.samples.iter().zip(&f.samples).all(|(&a, &b)| a == to_i16(b)));
        assert_eq!((to_i16(1.0), to_i16(-1.0), to_i16(0.5)), (32767, -32768, 16384));
    }

    #[test]
    fn i16_decode_is_allocated_once_at_its_length() {
        // An MP3 without a Xing tag has no frame count: the frame walk's total sizes the buffer
        // exactly (it grew by doubling before: up to 3x the final size at its peak).
        let bytes = mp3(60, false, None, Some(30));
        let walked = crate::sound::loop_points::LoopIndex::new(&bytes).decoded_frames();
        assert_eq!(walked, Some(60 * 1152));
        let i = decode_timed_i16(bytes, Some("mp3"), walked).unwrap();
        assert_eq!((i.frames(), i.samples.capacity()), (60 * 1152, i.samples.len()));
    }

    /// The same file through symphonia's own MS-ADPCM decoder.
    fn symphonia_ms_adpcm(bytes: Vec<u8>) -> Vec<f32> {
        use symphonia::core::audio::SampleBuffer;
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let mut format = symphonia::default::get_probe().format(&Hint::new(), mss, &FormatOptions::default(), &MetadataOptions::default()).unwrap().format;
        let mut dec = symphonia::default::get_codecs().make(&format.tracks()[0].codec_params, &DecoderOptions::default()).unwrap();
        let mut out = Vec::new();
        while let Ok(p) = format.next_packet() {
            let a = dec.decode(&p).unwrap();
            let mut b = SampleBuffer::<f32>::new(a.capacity() as u64, *a.spec());
            b.copy_interleaved_ref(a);
            out.extend_from_slice(b.samples());
        }
        out
    }

    #[test]
    fn ms_adpcm_matches_symphonia_on_normal_data() {
        for ch in [1u16, 2] {
            let bytes = ms_adpcm_wav(8, ch, 0x11);
            let ours = decode(bytes.clone(), Some("wav")).unwrap();
            assert_eq!(ours.samples, symphonia_ms_adpcm(bytes), "{ch} ch");
        }
    }

    #[test]
    fn extreme_ms_adpcm_decodes_every_block() {
        // Large nibbles overflowed symphonia's MS-ADPCM step size (a panic with overflow checks
        // on, garbage in release). Ours saturates: every block decodes, within the i16 clamp.
        for ch in [1u16, 2] {
            let pcm = decode(ms_adpcm_wav(8, ch, 0xFF), Some("wav")).unwrap();
            let spb = (256 - 7 * usize::from(ch)) * 2 / usize::from(ch) + 2;
            assert_eq!(pcm.frames(), 8 * spb, "{ch} ch");
            assert!(pcm.samples.iter().all(|x| (-1.0..1.0).contains(x)));
        }
    }
}

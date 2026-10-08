//! One playing sound ("voice"): a Bevy audio source that turns decoded or streamed PCM
//! into stereo, with gains the game can change while it plays.
//!
//! Bevy's own spatial audio uses a fixed 1/distance² law; the original uses Miles with
//! per-event min/max distances. So every voice is a plain stereo source whose left/right
//! gains are written each frame by [`super::update_voices`] through a shared
//! [`VoiceControl`]. Pitch is done by reporting a scaled sample rate (rodio resamples).

use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bevy::audio::Decodable;
use bevy::prelude::*;
use ntw_formats::sound::{Pcm, Pcm16, PcmStream, SharedBytes};

use super::feed::PcmFeed;
use super::mixer::LowPass;

/// Gains and the stop flag shared between the game and the audio thread.
#[derive(Debug, Default)]
pub struct VoiceControl {
    left: AtomicU32,
    right: AtomicU32,
    /// Low-pass cutoff as a fraction of the sample's Nyquist (f32 bits; 1.0 = off).
    cutoff: AtomicU32,
    stop: AtomicBool,
    /// Set by the audio thread when the sound has ended.
    finished: AtomicBool,
}

impl VoiceControl {
    pub fn new(left: f32, right: f32) -> Arc<Self> {
        let c = Self::default();
        c.set(left, right);
        c.set_cutoff(1.0);
        Arc::new(c)
    }

    pub fn set(&self, left: f32, right: f32) {
        self.left.store(left.to_bits(), Ordering::Relaxed);
        self.right.store(right.to_bits(), Ordering::Relaxed);
    }

    /// The low-pass cutoff (MIDDLEWARE_VERIFY.md §1.7); 1.0 (or more) = off.
    pub fn set_cutoff(&self, cutoff: f32) {
        self.cutoff.store(cutoff.to_bits(), Ordering::Relaxed);
    }

    fn cutoff(&self) -> f32 {
        f32::from_bits(self.cutoff.load(Ordering::Relaxed))
    }

    fn gains(&self) -> (f32, f32) {
        (f32::from_bits(self.left.load(Ordering::Relaxed)), f32::from_bits(self.right.load(Ordering::Relaxed)))
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }
}

/// Sample frame `i` of interleaved samples as (left, right) (mono on both sides); `None` past the
/// end. Generic, so each sample type gets its own monomorphic copy (no dynamic dispatch per frame
/// on the audio thread).
fn stereo<T: Copy>(samples: &[T], ch: usize, i: usize, to_f32: impl Fn(T) -> f32) -> Option<(f32, f32)> {
    let s = samples.get(i * ch..i * ch + ch)?;
    Some((to_f32(s[0]), to_f32(s[usize::from(ch > 1)])))
}

fn i16_to_f32(x: i16) -> f32 {
    f32::from(x) / 32768.0
}

/// Decoded samples in memory: a closed set of sample types, matched per frame (a predictable
/// branch, no virtual call).
#[derive(Clone)]
pub enum Samples {
    /// Short effects (f32, the clip cache).
    F32(Arc<Pcm>),
    /// Music (16-bit, decoded whole in the background, `super::MusicCache`).
    I16(Arc<Pcm16>),
}

impl Samples {
    fn channels(&self) -> usize {
        usize::from(match self {
            Self::F32(p) => p.channels,
            Self::I16(p) => p.channels,
        }
        .max(1))
    }

    fn sample_rate(&self) -> u32 {
        match self {
            Self::F32(p) => p.sample_rate,
            Self::I16(p) => p.sample_rate,
        }
        .max(1)
    }

    fn frames(&self) -> usize {
        match self {
            Self::F32(p) => p.frames(),
            Self::I16(p) => p.frames(),
        }
    }

    /// Sample frame `i` as (left, right); `None` past the end.
    fn frame(&self, ch: usize, i: usize) -> Option<(f32, f32)> {
        match self {
            Self::F32(p) => stereo(&p.samples, ch, i, |x| x),
            Self::I16(p) => stereo(&p.samples, ch, i, i16_to_f32),
        }
    }
}

/// Where the samples come from.
#[derive(Clone)]
pub enum ClipData {
    /// Decoded in memory: short effects (f32, cached), music (16-bit, decoded whole in the
    /// background, `super::MusicCache`).
    Decoded(Samples),
    /// Compressed bytes decoded while playing (music, long speech).
    Streamed { bytes: SharedBytes, ext: Option<String> },
    /// Samples pushed live by a producer (movie audio, `crate::video`).
    Live(Arc<PcmFeed>),
}

/// A Bevy audio asset for one voice. Each playing voice gets its own asset (they are cheap:
/// the PCM or bytes are shared).
#[derive(Asset, TypePath, Clone)]
pub struct VoiceClip {
    pub data: ClipData,
    pub control: Arc<VoiceControl>,
    /// Playback speed (pitch): 1.0 = original.
    pub speed: f32,
    /// Silence before the sound starts, in seconds.
    pub delay: f32,
    /// Loop forever (until stopped).
    pub looped: bool,
    /// Loop region in sample frames of the decoded file (`end == 0` = to the end of the file);
    /// set only for looping music, decoded whole (mapped from the event's byte offsets by
    /// `ntw_formats::sound::loop_points`).
    pub loop_start: u64,
    pub loop_end: u64,
}

impl Decodable for VoiceClip {
    type Decoder = VoiceSource;

    fn decoder(&self) -> VoiceSource {
        VoiceSource::new(self.clone())
    }
}

enum Feed {
    /// Read at the voice's frame index (`VoiceSource::frame`).
    Decoded(Samples),
    Streamed { stream: Option<PcmStream>, chunk: Vec<f32>, pos: usize },
    Live(Arc<PcmFeed>),
}

/// The rodio source: always stereo out.
pub struct VoiceSource {
    clip: VoiceClip,
    feed: Feed,
    in_channels: usize,
    rate: u32,
    /// Silence frames still to play (start delay).
    silence: u64,
    /// Frame index within the file (for loop regions).
    frame: u64,
    /// The right sample of the current output frame, when the left one was just returned.
    pending_right: Option<f32>,
    ended: bool,
    /// Miles' low-pass, one per output side, and the cutoff its coefficients were made for.
    filters: [LowPass; 2],
    filter_cutoff: f32,
}

impl VoiceSource {
    fn new(clip: VoiceClip) -> Self {
        let (feed, ch, rate) = match &clip.data {
            ClipData::Decoded(p) => (Feed::Decoded(p.clone()), p.channels(), p.sample_rate()),
            ClipData::Streamed { bytes, ext } => match PcmStream::open(bytes.clone(), ext.as_deref()) {
                Ok(s) => {
                    let (c, r) = (s.channels().max(1) as usize, s.sample_rate().max(1));
                    (Feed::Streamed { stream: Some(s), chunk: Vec::new(), pos: 0 }, c, r)
                }
                Err(e) => {
                    warn!("audio stream failed: {e}");
                    (Feed::Streamed { stream: None, chunk: Vec::new(), pos: 0 }, 1, 44_100)
                }
            },
            ClipData::Live(f) => (Feed::Live(f.clone()), f.channels() as usize, f.sample_rate()),
        };
        let silence = (clip.delay.max(0.0) * rate as f32) as u64;
        Self { clip, feed, in_channels: ch, rate, silence, frame: 0, pending_right: None, ended: false, filters: [LowPass::default(); 2], filter_cutoff: 1.0 }
    }

    /// The next input frame (up to 2 channels), handling loops. `None` at the end.
    fn next_frame(&mut self) -> Option<(f32, f32)> {
        let ch = self.in_channels;
        for _attempt in 0..2 {
            let at_loop_end = self.clip.looped && self.clip.loop_end > 0 && self.frame >= self.clip.loop_end;
            let got = if at_loop_end { None } else { self.read_frame(ch) };
            if let Some(f) = got {
                self.frame += 1;
                return Some(f);
            }
            if !self.clip.looped {
                return None;
            }
            self.rewind();
        }
        None
    }

    fn read_frame(&mut self, ch: usize) -> Option<(f32, f32)> {
        match &mut self.feed {
            Feed::Decoded(pcm) => pcm.frame(ch, self.frame as usize),
            Feed::Streamed { stream, chunk, pos } => {
                while *pos + ch > chunk.len() {
                    let next = stream.as_mut()?.next_chunk().ok().flatten()?;
                    *chunk = next;
                    *pos = 0;
                }
                let s = &chunk[*pos..*pos + ch];
                *pos += ch;
                Some((s[0], if ch > 1 { s[1] } else { s[0] }))
            }
            Feed::Live(f) => f.next_frame(),
        }
    }

    /// Back to the loop start.
    fn rewind(&mut self) {
        let start = self.clip.loop_start;
        match &mut self.feed {
            Feed::Decoded(pcm) => self.frame = start.min(pcm.frames() as u64),
            Feed::Streamed { stream, chunk, pos } => {
                if let ClipData::Streamed { bytes, ext } = &self.clip.data {
                    *stream = PcmStream::open(bytes.clone(), ext.as_deref()).ok();
                }
                chunk.clear();
                *pos = 0;
                self.frame = 0;
                // Skip to the loop start by decoding (streams are not seekable here).
                for _ in 0..start {
                    if self.read_frame(self.in_channels).is_none() {
                        break;
                    }
                    self.frame += 1;
                }
            }
            // A live feed cannot rewind (looping is the producer's job).
            Feed::Live(_) => {}
        }
    }
}

impl Iterator for VoiceSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(r) = self.pending_right.take() {
            return Some(r);
        }
        if self.ended || self.clip.control.stop.load(Ordering::Relaxed) {
            self.clip.control.finished.store(true, Ordering::Relaxed);
            return None;
        }
        let (gl, gr) = self.clip.control.gains();
        let (l, r) = if self.silence > 0 {
            self.silence -= 1;
            (0.0, 0.0)
        } else {
            match self.next_frame() {
                Some(f) => f,
                None => {
                    self.ended = true;
                    self.clip.control.finished.store(true, Ordering::Relaxed);
                    return None;
                }
            }
        };
        // Low-pass (3D voices; MIDDLEWARE_VERIFY.md §1.7). The filter runs here at the sample's
        // playback rate; Miles runs it at the output rate with `wc = cutoff × rate / output_rate`.
        // With output rate = playback rate (44.1 kHz files and output) both are the same
        // (INFERRED equivalent; PROVISIONAL for other rates).
        let cutoff = self.clip.control.cutoff();
        if cutoff != self.filter_cutoff {
            self.filter_cutoff = cutoff;
            let rate = self.rate as f32 * self.clip.speed.clamp(0.25, 4.0);
            for f in &mut self.filters {
                f.set(cutoff, rate, rate);
            }
        }
        let (l, r) = (self.filters[0].process(l), self.filters[1].process(r));
        // Mono files: the same sample to both sides, then panned by the gains.
        self.pending_right = Some(r * gr);
        Some(l * gl)
    }
}

impl rodio::Source for VoiceSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(2).expect("2")
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        let r = (self.rate as f32 * self.clip.speed.clamp(0.25, 4.0)).round() as u32;
        NonZero::new(r.max(1)).expect("non-zero")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(data: Samples, loop_start: u64, loop_end: u64, n: usize) -> Vec<f32> {
        let clip = VoiceClip { data: ClipData::Decoded(data), control: VoiceControl::new(1.0, 1.0), speed: 1.0, delay: 0.0, looped: true, loop_start, loop_end };
        // Left samples only (the output is interleaved stereo).
        clip.decoder().step_by(2).take(n).collect()
    }

    #[test]
    fn decoded_feed_loops_the_region_for_any_sample_type() {
        // Frames 0..5; the loop region 1..3 plays 0 1 2, then 1 2, 1 2, ...
        // Both sample types are variants of one enum (no `dyn` call per frame on the audio thread).
        let f = Samples::F32(Arc::new(Pcm { channels: 1, sample_rate: 100, samples: vec![0.0, 0.25, 0.5, 0.75, 1.0] }));
        let i = Samples::I16(Arc::new(Pcm16 { channels: 2, sample_rate: 100, samples: vec![0, 0, 8192, 0, 16384, 0, 24576, 0, 32767, 0] }));
        assert_eq!((f.channels(), i.channels(), f.frames(), i.frames(), i.sample_rate()), (1, 2, 5, 5, 100));
        let want = [0.0, 0.25, 0.5, 0.25, 0.5, 0.25, 0.5];
        assert_eq!(play(f.clone(), 1, 3, 7), want);
        assert_eq!(play(i, 1, 3, 7), want);
        // No end (0): the region runs to the end of the file.
        assert_eq!(play(f, 3, 0, 6), [0.0, 0.25, 0.5, 0.75, 1.0, 0.75]);
    }
}

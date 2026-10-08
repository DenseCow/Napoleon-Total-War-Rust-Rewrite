//! A live PCM feed: samples pushed by a producer thread (e.g. the movie decoder) and played by a
//! voice as they arrive. Used for Bink movie audio, which is decoded frame by frame together with
//! the pictures (`crate::video`).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Interleaved f32 samples in a queue shared between a producer and one playing voice.
#[derive(Debug)]
pub struct PcmFeed {
    channels: u16,
    sample_rate: u32,
    queue: Mutex<VecDeque<f32>>,
    /// The producer has pushed everything (the voice ends once the queue is empty).
    ended: AtomicBool,
    /// Stop now (skip): the voice ends at once.
    stopped: AtomicBool,
    /// Sample frames the voice has taken so far (playback position).
    played: AtomicU64,
}

impl PcmFeed {
    pub fn new(channels: u16, sample_rate: u32) -> Self {
        Self {
            channels: channels.max(1),
            sample_rate: sample_rate.max(1),
            queue: Mutex::new(VecDeque::new()),
            ended: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            played: AtomicU64::new(0),
        }
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Appends interleaved 16-bit samples.
    pub fn push_i16(&self, samples: &[i16]) {
        let mut q = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        q.extend(samples.iter().map(|&s| s as f32 / 32768.0));
    }

    /// No more samples will come.
    pub fn finish(&self) {
        self.ended.store(true, Ordering::Relaxed);
    }

    /// Stops playback at once (the queue is dropped).
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.queue.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    #[allow(dead_code)] // tests and diagnostics
    /// Interleaved samples waiting to be played.
    pub fn queued(&self) -> usize {
        self.queue.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    #[allow(dead_code)] // playback position (a hook for audio-clocked sync)
    /// Sample frames played so far.
    pub fn played_frames(&self) -> u64 {
        self.played.load(Ordering::Relaxed)
    }

    /// The next sample frame as (left, right) (mono is duplicated). `Some((0, 0))` while the
    /// producer is behind (an underrun plays silence); `None` once ended or stopped.
    pub fn next_frame(&self) -> Option<(f32, f32)> {
        if self.is_stopped() {
            return None;
        }
        let ch = self.channels as usize;
        let mut q = self.queue.lock().unwrap_or_else(|p| p.into_inner());
        if q.len() < ch {
            return if self.ended.load(Ordering::Relaxed) { None } else { Some((0.0, 0.0)) };
        }
        let l = q.pop_front().unwrap_or(0.0);
        let r = if ch > 1 { q.pop_front().unwrap_or(0.0) } else { l };
        for _ in 2..ch {
            q.pop_front();
        }
        drop(q);
        self.played.fetch_add(1, Ordering::Relaxed);
        Some((l, r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_plays_underruns_and_ends() {
        let f = PcmFeed::new(2, 48_000);
        assert_eq!(f.next_frame(), Some((0.0, 0.0)), "underrun = silence");
        f.push_i16(&[16384, -16384, 0, 32767]);
        assert_eq!(f.next_frame(), Some((0.5, -0.5)));
        f.finish();
        assert!(f.next_frame().is_some());
        assert_eq!(f.next_frame(), None);
        assert_eq!(f.played_frames(), 2);
        let m = PcmFeed::new(1, 44_100);
        m.push_i16(&[8192]);
        assert_eq!(m.next_frame(), Some((0.25, 0.25)));
        m.push_i16(&[1, 2, 3]);
        m.stop();
        assert_eq!(m.next_frame(), None);
    }
}

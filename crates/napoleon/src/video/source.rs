//! Where movie frames come from, and the decoder thread that runs ahead of the display.
//!
//! The player only needs "the next picture as RGBA plus this frame's sound" ([`MovieSource`]),
//! so it works with our Bink decoder ([`BinkSource`]) and with a generated test movie
//! ([`StubSource`]) alike.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};

use ntw_formats::bink::{split_packet, AudioDecoder, BinkHeader, VideoDecoder};
use ntw_formats::pack::Vfs;

use crate::audio::PcmFeed;

/// What a movie is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieInfo {
    pub width: u32,
    pub height: u32,
    /// Frame rate `fps_num / fps_den`.
    pub fps_num: u32,
    pub fps_den: u32,
    pub frames: u32,
    /// The chosen sound track: (sample rate, channels), if the movie has sound.
    pub audio: Option<(u32, u16)>,
    /// Plane sizes (width = stride): Y, and each of Cb/Cr (half size, padded to 8 like Bink's
    /// buffers).
    pub y_size: (u32, u32),
    pub c_size: (u32, u32),
}

impl MovieInfo {
    /// Seconds per frame.
    pub fn frame_secs(&self) -> f64 {
        self.fps_den as f64 / self.fps_num.max(1) as f64
    }
}

/// One picture as 8-bit planes: Y (`y_size`), Cb = `u` and Cr = `v` (`c_size`). The player
/// converts them to RGB on the GPU with the game's own movie-shader maths (`BINK.md` §6).
#[derive(Debug, Clone, Default)]
pub struct Planes {
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl Planes {
    fn fit(&mut self, info: &MovieInfo) {
        self.y.resize((info.y_size.0 * info.y_size.1) as usize, 0);
        self.u.resize((info.c_size.0 * info.c_size.1) as usize, 128);
        self.v.resize((info.c_size.0 * info.c_size.1) as usize, 128);
    }
}

/// A movie decoded frame by frame.
pub trait MovieSource: Send + 'static {
    fn info(&self) -> &MovieInfo;
    /// Decodes the next frame: the picture into `planes` (already sized from [`MovieInfo`]) and
    /// this frame's sound (interleaved 16-bit) appended to `audio`. `Ok(false)` after the last frame.
    fn next_frame(&mut self, planes: &mut Planes, audio: &mut Vec<i16>) -> Result<bool, String>;
    /// Back to the first frame (for looping movies).
    fn rewind(&mut self) -> Result<(), String>;
}

/// Bytes read from the pack in one go while streaming (several frames at a time).
const READ_WINDOW: usize = 4 << 20;

/// A `.bik` file from the Vfs, decoded with `ntw_formats::bink`, streamed in windows of frames
/// (a 169 MB movie is never loaded whole).
pub struct BinkSource {
    vfs: Arc<Vfs>,
    path: String,
    header: BinkHeader,
    info: MovieInfo,
    video: VideoDecoder,
    /// Header index of the chosen sound track and its decoder.
    audio: Option<(usize, AudioDecoder)>,
    next: usize,
    /// Bytes of the file from `window_start`.
    window: Vec<u8>,
    window_start: u64,
}

impl BinkSource {
    /// Opens `path` (e.g. `movies\frontend2.bik`). `track_id` picks the sound track by its id
    /// (`None` = the first track).
    pub fn open(vfs: Arc<Vfs>, path: &str, track_id: Option<u32>) -> Result<Self, String> {
        let fixed = vfs.read_range(path, 0, BinkHeader::FIXED_LEN).map_err(|e| format!("{path}: {e}"))?;
        let need = BinkHeader::needed_len(&fixed).map_err(|e| format!("{path}: {e}"))?;
        let header = BinkHeader::parse(&vfs.read_range(path, 0, need).map_err(|e| format!("{path}: {e}"))?)
            .map_err(|e| format!("{path}: {e}"))?;
        let video = VideoDecoder::new(&header).map_err(|e| format!("{path}: {e}"))?;
        let ti = match track_id {
            // A missing id falls back to track id 0, as the exe does (`BINK.md` §8).
            Some(id) => {
                let by_id = |want: u32| header.audio_tracks.iter().position(|t| t.id == want);
                by_id(id).or_else(|| by_id(0)).or((!header.audio_tracks.is_empty()).then_some(0))
            }
            None => (!header.audio_tracks.is_empty()).then_some(0),
        };
        let audio = match ti {
            Some(i) => Some((i, AudioDecoder::new(&header.audio_tracks[i]).map_err(|e| format!("{path}: {e}"))?)),
            None => None,
        };
        let info = MovieInfo {
            width: header.width,
            height: header.height,
            fps_num: header.fps_num,
            fps_den: header.fps_den,
            frames: header.num_frames,
            audio: audio.as_ref().map(|(_, d)| (d.sample_rate(), d.channels() as u16)),
            y_size: video.frame().y_size,
            c_size: video.frame().c_size,
        };
        Ok(Self { vfs, path: path.to_owned(), header, info, video, audio, next: 0, window: Vec::new(), window_start: 0 })
    }

    /// The bytes of frame `i`'s packet, reading a new window when needed.
    fn packet(&mut self, i: usize) -> Result<&[u8], String> {
        let f = self.header.frames[i];
        let (start, end) = (f.offset as u64, f.offset as u64 + f.size as u64);
        let have = start >= self.window_start && end <= self.window_start + self.window.len() as u64;
        if !have {
            let len = (f.size as usize).max(READ_WINDOW);
            self.window = self.vfs.read_range(&self.path, start, len).map_err(|e| format!("{}: {e}", self.path))?;
            self.window_start = start;
            if (self.window.len() as u64) < f.size as u64 {
                return Err(format!("{}: frame {i} truncated", self.path));
            }
        }
        let o = (start - self.window_start) as usize;
        Ok(&self.window[o..o + f.size as usize])
    }
}

impl MovieSource for BinkSource {
    fn info(&self) -> &MovieInfo {
        &self.info
    }

    fn next_frame(&mut self, planes: &mut Planes, audio: &mut Vec<i16>) -> Result<bool, String> {
        let i = self.next;
        if i >= self.header.frames.len() {
            return Ok(false);
        }
        let ntracks = self.header.audio_tracks.len();
        // Split borrows: the packet lives in `self.window`.
        self.packet(i)?;
        let f = self.header.frames[i];
        let o = (f.offset as u64 - self.window_start) as usize;
        let packet = &self.window[o..o + f.size as usize];
        let parts = split_packet(packet, ntracks).map_err(|e| format!("{}: frame {i}: {e}", self.path))?;
        if let Some((t, dec)) = &mut self.audio {
            dec.decode_packet(parts.audio[*t], audio).map_err(|e| format!("{}: frame {i}: {e}", self.path))?;
        }
        self.video.decode(parts.video).map_err(|e| format!("{}: {e}", self.path))?;
        let fr = self.video.frame();
        planes.y.copy_from_slice(&fr.y);
        planes.u.copy_from_slice(&fr.u);
        planes.v.copy_from_slice(&fr.v);
        self.next += 1;
        Ok(true)
    }

    fn rewind(&mut self) -> Result<(), String> {
        // Every shipped movie has a single keyframe (frame 0), so a rewind restarts the decoders.
        self.video = VideoDecoder::new(&self.header).map_err(|e| e.to_string())?;
        if let Some((_, d)) = &mut self.audio {
            d.reset();
        }
        self.next = 0;
        Ok(())
    }
}

/// A generated test movie: a moving white bar on a grey that changes each frame (Cb = Cr = 128),
/// and a 440 Hz tone.
/// Used by tests (and handy to check the player without the game files).
#[cfg_attr(not(test), allow(dead_code))]
pub struct StubSource {
    info: MovieInfo,
    next: u32,
    /// Sample frames produced so far (keeps the tone continuous).
    samples: u64,
}

#[cfg_attr(not(test), allow(dead_code))]
impl StubSource {
    pub fn new(width: u32, height: u32, frames: u32, audio: Option<(u32, u16)>) -> Self {
        let y_size = ((width + 7) & !7, (height + 7) & !7);
        let c_size = ((((width + 1) >> 1) + 7) & !7, (((height + 1) >> 1) + 7) & !7);
        Self { info: MovieInfo { width, height, fps_num: 30, fps_den: 1, frames, audio, y_size, c_size }, next: 0, samples: 0 }
    }

    /// The luma of frame `i`'s background (tests check what reaches the texture).
    pub fn luma(i: u32) -> u8 {
        (16 + i * 37 % 200) as u8
    }
}

impl MovieSource for StubSource {
    fn info(&self) -> &MovieInfo {
        &self.info
    }

    fn next_frame(&mut self, planes: &mut Planes, audio: &mut Vec<i16>) -> Result<bool, String> {
        let i = self.next;
        if i >= self.info.frames {
            return Ok(false);
        }
        let (w, h) = (self.info.width as usize, self.info.height as usize);
        let stride = self.info.y_size.0 as usize;
        let c = Self::luma(i);
        let bar = (i as usize * 4) % w.max(1);
        for y in 0..h {
            for x in 0..w {
                planes.y[y * stride + x] = if x == bar { 235 } else { c };
            }
        }
        planes.u.fill(128);
        planes.v.fill(128);
        if let Some((rate, ch)) = self.info.audio {
            // Exactly rate/fps sample frames per video frame, like the shipped movies.
            let end = (i as u64 + 1) * rate as u64 * self.info.fps_den as u64 / self.info.fps_num as u64;
            while self.samples < end {
                let t = self.samples as f64 / rate as f64;
                let v = ((t * 440.0 * std::f64::consts::TAU).sin() * 8000.0) as i16;
                audio.extend(std::iter::repeat_n(v, ch as usize));
                self.samples += 1;
            }
        }
        self.next += 1;
        Ok(true)
    }

    fn rewind(&mut self) -> Result<(), String> {
        self.next = 0;
        Ok(())
    }
}

/// One decoded picture with its frame number since the movie started (counting on through loops).
pub struct DecodedFrame {
    pub index: u64,
    pub planes: Planes,
}

/// Decoded frames queued ahead of the display. Small: each 1280x720 frame is 1.4 MB of planes, and the
/// decoder runs several times faster than real time.
const QUEUE_FRAMES: usize = 3;

/// A movie being decoded on its own thread.
pub struct MovieStream {
    pub info: MovieInfo,
    frames: Receiver<DecodedFrame>,
    recycle: SyncSender<Planes>,
    stop: Arc<AtomicBool>,
    /// The sound track (if the movie has one and sound was asked for).
    pub audio: Option<Arc<PcmFeed>>,
    error: Arc<Mutex<Option<String>>>,
    ended: bool,
}

/// The result of polling the stream.
pub enum Poll {
    Frame(DecodedFrame),
    /// Nothing ready yet.
    Pending,
    /// The movie (and any loop) is over, or it failed.
    Ended,
}

impl MovieStream {
    /// Starts decoding `source` on a new thread. `looped` restarts it at the end forever;
    /// `with_audio` routes its sound into a [`PcmFeed`].
    pub fn start(mut source: Box<dyn MovieSource>, looped: bool, with_audio: bool) -> Self {
        let info = source.info().clone();
        let (tx, frames) = std::sync::mpsc::sync_channel::<DecodedFrame>(QUEUE_FRAMES);
        let (recycle, spare) = std::sync::mpsc::sync_channel::<Planes>(QUEUE_FRAMES + 2);
        let stop = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));
        let audio = match (with_audio, info.audio) {
            (true, Some((rate, ch))) => Some(Arc::new(PcmFeed::new(ch, rate))),
            _ => None,
        };
        let (stop2, error2, feed, info2) = (stop.clone(), error.clone(), audio.clone(), info.clone());
        let spawned = std::thread::Builder::new().name("movie decoder".into()).spawn(move || {
            let mut index = 0u64;
            let mut pcm = Vec::new();
            loop {
                if stop2.load(Ordering::Relaxed) {
                    break;
                }
                let mut planes = spare.try_recv().unwrap_or_default();
                planes.fit(&info2);
                pcm.clear();
                match source.next_frame(&mut planes, &mut pcm) {
                    Ok(true) => {
                        if let Some(f) = &feed {
                            f.push_i16(&pcm);
                        }
                        if tx.send(DecodedFrame { index, planes }).is_err() {
                            break;
                        }
                        index += 1;
                    }
                    Ok(false) if looped && index > 0 => {
                        if let Err(e) = source.rewind() {
                            *error2.lock().unwrap_or_else(|p| p.into_inner()) = Some(e);
                            break;
                        }
                    }
                    Ok(false) => break,
                    Err(e) => {
                        *error2.lock().unwrap_or_else(|p| p.into_inner()) = Some(e);
                        break;
                    }
                }
            }
            if let Some(f) = &feed {
                f.finish();
            }
        });
        if let Err(e) = spawned {
            *error.lock().unwrap_or_else(|p| p.into_inner()) = Some(format!("could not start the decoder thread: {e}"));
        }
        Self { info, frames, recycle, stop, audio, error, ended: false }
    }

    /// The next decoded frame, if one is ready.
    pub fn poll(&mut self) -> Poll {
        if self.ended {
            return Poll::Ended;
        }
        match self.frames.try_recv() {
            Ok(f) => Poll::Frame(f),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => {
                self.ended = true;
                Poll::Ended
            }
        }
    }

    /// Gives a frame buffer back to the decoder for reuse.
    pub fn recycle(&self, planes: Planes) {
        let _ = self.recycle.try_send(planes);
    }

    /// The decoder's error, if it stopped on one.
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Stops decoding and the sound.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(f) = &self.audio {
            f.stop();
        }
        // Unblock a decoder waiting on a full queue.
        while self.frames.try_recv().is_ok() {}
        self.ended = true;
    }
}

impl Drop for MovieStream {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(s: &mut MovieStream) -> Vec<DecodedFrame> {
        let mut out = Vec::new();
        loop {
            match s.poll() {
                Poll::Frame(f) => out.push(f),
                Poll::Pending => std::thread::yield_now(),
                Poll::Ended => return out,
            }
        }
    }

    #[test]
    fn stream_delivers_every_frame_in_order_with_its_sound() {
        let mut s = MovieStream::start(Box::new(StubSource::new(8, 4, 10, Some((48_000, 2)))), false, true);
        let frames = drain(&mut s);
        assert_eq!(frames.len(), 10);
        for (i, f) in frames.iter().enumerate() {
            assert_eq!(f.index, i as u64);
            assert_eq!(f.planes.y[1], StubSource::luma(i as u32), "frame {i} pixel 1");
            assert_eq!(f.planes.y[i * 4 % 8], 235, "frame {i} bar");
        }
        assert!(s.error().is_none());
        // 10 frames at 30 fps = 1/3 s = 16,000 stereo sample frames.
        assert_eq!(s.audio.as_ref().unwrap().queued(), 16_000 * 2);
    }

    #[test]
    fn looped_stream_keeps_counting_until_stopped() {
        let mut s = MovieStream::start(Box::new(StubSource::new(16, 4, 3, None)), true, true);
        assert!(s.audio.is_none());
        let mut seen = Vec::new();
        while seen.len() < 8 {
            if let Poll::Frame(f) = s.poll() {
                seen.push((f.index, f.planes.y[15]));
                s.recycle(f.planes);
            }
        }
        s.stop();
        assert_eq!(seen.iter().map(|f| f.0).collect::<Vec<_>>(), (0..8).collect::<Vec<_>>());
        // Frame 3 is the first frame again.
        assert_eq!(seen[3].1, StubSource::luma(0));
        assert!(matches!(s.poll(), Poll::Ended));
    }
}

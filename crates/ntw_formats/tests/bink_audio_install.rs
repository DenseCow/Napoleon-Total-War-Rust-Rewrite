//! Bink movie audio against a real install. All `#[ignore]`d (they need the game). Run with:
//! ```text
//! cargo test -p ntw_formats --test bink_audio_install -- --ignored --nocapture
//! ```
//! Read-only: the movies are read through the Vfs, `binkw32.dll` is only read as bytes to compare
//! its constant tables with ours (it is never loaded or called). `NTW_DATA_DIR` overrides the
//! install's `data` folder. Nothing decoded is written anywhere.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use ntw_formats::bink::audio::{CRITICAL_FREQS, QUANT_BITS, RUN_LENGTHS};
use ntw_formats::bink::{split_packet, AudioDecoder, BinkHeader};
use ntw_formats::pack::Vfs;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

fn header(vfs: &Vfs, path: &str) -> BinkHeader {
    let fixed = vfs.read_range(path, 0, BinkHeader::FIXED_LEN).unwrap();
    let need = BinkHeader::needed_len(&fixed).unwrap();
    BinkHeader::parse(&vfs.read_range(path, 0, need).unwrap()).unwrap()
}

/// Per-track result of decoding a whole movie.
struct TrackResult {
    id: u32,
    rate: u32,
    channels: usize,
    /// Interleaved samples decoded.
    samples: usize,
    /// Sum of the packets' "decoded bytes" fields / 2.
    announced: usize,
    peak: i32,
}

/// Decodes every audio track of one movie in a single pass over its frames.
fn decode_movie(vfs: &Vfs, path: &str) -> Result<(BinkHeader, Vec<TrackResult>), String> {
    let h = header(vfs, path);
    let nt = h.audio_tracks.len();
    let mut decs: Vec<AudioDecoder> =
        h.audio_tracks.iter().map(|t| AudioDecoder::new(t).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    let mut res: Vec<TrackResult> = h
        .audio_tracks
        .iter()
        .map(|t| TrackResult { id: t.id, rate: t.sample_rate as u32, channels: t.channels() as usize, samples: 0, announced: 0, peak: 0 })
        .collect();
    let mut pcm = Vec::new();
    // Read the movie in windows of consecutive frames (frames are stored back to back).
    let mut i = 0usize;
    while i < h.frames.len() {
        let start = h.frames[i].offset as u64;
        let mut end = i;
        let mut bytes = 0u64;
        while end < h.frames.len() && (end == i || bytes + (h.frames[end].size as u64) < 16 << 20) {
            bytes += h.frames[end].size as u64;
            end += 1;
        }
        let win = vfs.read_range(path, start, bytes as usize).map_err(|e| e.to_string())?;
        for f in i..end {
            let fe = h.frames[f];
            let o = (fe.offset as u64 - start) as usize;
            let pk = split_packet(&win[o..o + fe.size as usize], nt).map_err(|e| format!("frame {f}: {e}"))?;
            for t in 0..nt {
                let a = pk.audio[t];
                if a.len() < 4 {
                    continue;
                }
                res[t].announced += u32::from_le_bytes(a[..4].try_into().unwrap()) as usize / 2;
                pcm.clear();
                let used = decs[t].decode_packet(a, &mut pcm).map_err(|e| format!("frame {f} track {t}: {e}"))?;
                if used != a.len() - 4 {
                    return Err(format!("frame {f} track {t}: used {used} of {} bytes", a.len() - 4));
                }
                res[t].samples += pcm.len();
                res[t].peak = res[t].peak.max(pcm.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0));
            }
        }
        i = end;
    }
    Ok((h, res))
}

/// Every audio track of every shipped movie decodes with no error, every packet's bitstream is
/// consumed exactly, the decoded sample count is what the packets announce, and the audio lasts as
/// long as the video: exactly `sample_rate / fps` sample frames per video frame.
#[test]
#[ignore = "needs the game installed"]
fn every_movie_audio_track_decodes() {
    let vfs = Vfs::open_install(data_dir()).expect("install");
    let movies: Vec<String> = vfs.list("movies\\").into_iter().filter(|p| p.ends_with(".bik")).map(String::from).collect();
    assert_eq!(movies.len(), 73, "shipped movie count (BINK.md §1)");
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::<String>::new());
    let totals = Mutex::new((0usize, 0usize, 0f64)); // tracks, movies with audio, seconds of audio
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(6);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = movies.get(k) else { break };
                match decode_movie(&vfs, path) {
                    Err(e) => failures.lock().unwrap().push(format!("{path}: {e}")),
                    Ok((h, res)) => {
                        let video = h.duration_secs();
                        for r in &res {
                            let secs = r.samples as f64 / r.channels as f64 / r.rate as f64;
                            // One block is 1,920 sample frames (2048 - 2048/16) at these rates.
                            let block = 1920.0 / r.rate as f64;
                            println!(
                                "{path:<34} id {} {} Hz {} ch {:>9} samples {:8.3}s (video {:8.3}s) peak {}",
                                r.id, r.rate, r.channels, r.samples, secs, video, r.peak
                            );
                            let mut f = failures.lock().unwrap();
                            if r.samples != r.announced {
                                f.push(format!("{path} id {}: {} samples, packets announce {}", r.id, r.samples, r.announced));
                            }
                            // CONFIRMED on every track: exactly rate / fps sample frames per video frame.
                            let want = h.num_frames as u64 * r.rate as u64 * h.fps_den as u64 / h.fps_num as u64 * r.channels as u64;
                            if r.samples as u64 != want || (secs - video).abs() > block {
                                f.push(format!("{path} id {}: {secs:.3}s of audio vs {video:.3}s of video ({} samples, want {want})", r.id, r.samples));
                            }
                            if r.peak == 0 {
                                f.push(format!("{path} id {}: silent", r.id));
                            }
                        }
                        let mut t = totals.lock().unwrap();
                        t.0 += res.len();
                        t.1 += (!res.is_empty()) as usize;
                        t.2 += res.iter().map(|r| r.samples as f64 / r.channels as f64 / r.rate as f64).sum::<f64>();
                    }
                }
            });
        }
    });
    let t = totals.into_inner().unwrap();
    println!("{} tracks in {} movies, {:.1}s of audio", t.0, t.1, t.2);
    let f = failures.into_inner().unwrap();
    assert!(f.is_empty(), "{} failures:\n{}", f.len(), f.join("\n"));
    assert_eq!(t.1, 70, "70 of the 73 movies have audio (BINK.md §1)");
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Our audio constant tables are byte for byte the ones in the shipped `binkw32.dll` (read as
/// bytes only).
#[test]
#[ignore = "needs the game installed"]
fn audio_tables_match_binkw32() {
    let dll = std::fs::read(data_dir().parent().unwrap().join("binkw32.dll")).expect("binkw32.dll");
    let quant: Vec<u8> = QUANT_BITS.iter().flat_map(|v| v.to_le_bytes()).collect();
    let q = find(&dll, &quant).expect("quantiser table (96 x f32) in binkw32.dll");
    // Band edges: stored as 25 x u32 (0 Hz first) or as u16; accept either.
    let f32s: Vec<u8> = CRITICAL_FREQS.iter().flat_map(|v| v.to_le_bytes()).collect();
    let f16s: Vec<u8> = CRITICAL_FREQS.iter().flat_map(|v| (*v as u16).to_le_bytes()).collect();
    let fpos = find(&dll, &f32s).map(|p| (p, "u32")).or_else(|| find(&dll, &f16s).map(|p| (p, "u16")));
    let fpos = fpos.or_else(|| find(&dll, &f32s[4..]).map(|p| (p, "u32 without the 0")));
    let (fp, fkind) = fpos.expect("band-edge table in binkw32.dll");
    // Run lengths: stored as bytes, as u32, or pre-multiplied by 8.
    let rl8: Vec<u8> = RUN_LENGTHS.to_vec();
    let rl32: Vec<u8> = RUN_LENGTHS.iter().flat_map(|v| (*v as u32).to_le_bytes()).collect();
    let rlx8: Vec<u8> = RUN_LENGTHS.iter().flat_map(|v| (*v as u32 * 8).to_le_bytes()).collect();
    let rpos = find(&dll, &rl32)
        .map(|p| (p, "u32"))
        .or_else(|| find(&dll, &rlx8).map(|p| (p, "u32 x 8")))
        .or_else(|| find(&dll, &rl8).map(|p| (p, "u8")));
    let (rp, rkind) = rpos.expect("run-length table in binkw32.dll");
    println!("binkw32.dll file offsets: quantisers 0x{q:x}, band edges 0x{fp:x} ({fkind}), run lengths 0x{rp:x} ({rkind})");
}

//! Bink decoder against a real install (read-only). All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test bink_install -- --ignored --nocapture
//! ```
//! The install path can be overridden with `NTW_DATA_DIR`.

use std::path::PathBuf;

use ntw_formats::bink::{split_packet, BinkHeader, VideoDecoder};
use ntw_formats::pack::Vfs;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

fn header(vfs: &Vfs, path: &str) -> BinkHeader {
    let fixed = vfs.read_range(path, 0, BinkHeader::FIXED_LEN).unwrap();
    let need = BinkHeader::needed_len(&fixed).unwrap();
    BinkHeader::parse(&vfs.read_range(path, 0, need).unwrap()).unwrap()
}

fn fnv(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100_0000_01b3);
    }
}

/// What decoding one movie produced.
struct Report {
    path: String,
    /// Hash of the Y, U and V planes of each requested frame.
    frame_hashes: Vec<(usize, u64)>,
    errors: Vec<String>,
}

fn decode_movie(vfs: &Vfs, path: &str, hash_frames: &[usize]) -> Report {
    let h = header(vfs, path);
    let mut r = Report { path: path.into(), frame_hashes: Vec::new(), errors: Vec::new() };
    if h.file_size as u64 != vfs.find(path).unwrap().1.size as u64 {
        r.errors.push(format!("header file size {} != pack entry size", h.file_size));
    }
    let mut video = VideoDecoder::new(&h).unwrap();
    for (i, f) in h.frames.iter().enumerate() {
        let b = vfs.read_range(path, f.offset as u64, f.size as usize).unwrap();
        let pk = split_packet(&b, h.audio_tracks.len()).unwrap();
        match video.decode(pk.video) {
            Ok(e) => {
                // Each plane must end exactly where the next one starts (32-bit aligned).
                if e.y_end.div_ceil(32) * 4 != e.chroma_offset || e.c2_end.div_ceil(32) * 4 != pk.video.len() {
                    r.errors.push(format!("frame {i}: plane ends {e:?}, packet {}", pk.video.len()));
                }
            }
            Err(e) => r.errors.push(format!("frame {i}: {e}")),
        }
        if hash_frames.contains(&i) {
            let fr = video.frame();
            let mut hh = 0xcbf2_9ce4_8422_2325u64;
            fnv(&mut hh, &fr.y);
            fnv(&mut hh, &fr.u);
            fnv(&mut hh, &fr.v);
            r.frame_hashes.push((i, hh));
        }
        // Audio packets are split off but not decoded here (the audio decoder has its own tests).
        if r.errors.len() > 5 {
            break;
        }
    }
    r
}

/// Every shipped movie decodes every video frame: each plane ends exactly where the stream says
/// the next one begins (Y at the stored chroma offset, the second chroma plane at the packet end),
/// and the header's file size matches the pack entry.
#[test]
#[ignore]
fn every_movie_decodes_completely() {
    let vfs = vfs();
    let movies: Vec<String> = vfs.list("movies\\").into_iter().filter(|p| p.ends_with(".bik")).map(String::from).collect();
    assert_eq!(movies.len(), 73, "shipped movie count");
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let reports = std::sync::Mutex::new(Vec::new());
    let t0 = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(p) = movies.get(i) else { break };
                let r = decode_movie(&vfs, p, &[]);
                reports.lock().unwrap().push(r);
            });
        }
    });
    let reports = reports.into_inner().unwrap();
    let mut failed = 0;
    for r in &reports {
        if !r.errors.is_empty() {
            failed += 1;
            println!("{}: {:?}", r.path, &r.errors[..r.errors.len().min(5)]);
        }
    }
    println!("{} movies decoded in {:.1}s, {failed} with errors", reports.len(), t0.elapsed().as_secs_f64());
    assert_eq!(failed, 0);
}

/// Golden hashes: decoding is deterministic and does not drift when the code changes. The values
/// were produced by this decoder (2026-10-03) after every movie passed the structural checks
/// above; there is no reference decoder output to compare with (see BINK.md §7).
#[test]
#[ignore]
fn golden_hashes() {
    let vfs = vfs();
    // (movie, frames to decode, (frame, hash) pairs)
    type Case<'a> = (&'a str, &'a [usize], &'a [(usize, u64)]);
    let cases: [Case; 3] = [
        ("movies\\sega_logo_sting_hd.bik", &[0, 37, 74], &[(0, 0xc0ccf54a899b0325), (37, 0x782380393a59ee21), (74, 0x95ff3a59861c0325)]),
        ("movies\\frontend2.bik", &[0, 1934, 3868], &[(0, 0x9aaad6303a428e68), (1934, 0x31c32319d3e49a88), (3868, 0x2e92e9f0f069da93)]),
        // Frames 0 and 899 are the same (black) picture.
        ("movies\\nhb_01_arcole.bik", &[0, 450, 899], &[(0, 0xfa4d915e8e306325), (450, 0x28043419b9645228), (899, 0xfa4d915e8e306325)]),
    ];
    let mut ok = true;
    for (path, frames, want_frames) in cases {
        let r = decode_movie(&vfs, path, frames);
        assert!(r.errors.is_empty(), "{path}: {:?}", r.errors);
        println!("{path}: frames {:x?}", r.frame_hashes);
        if r.frame_hashes != want_frames {
            ok = false;
        }
    }
    assert!(ok, "golden hashes changed");
}

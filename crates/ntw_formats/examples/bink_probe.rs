//! Research helper: survey the Bink movies in the install's packs (read-only).
//!   cargo run -p ntw_formats --example bink_probe -- survey
//!   cargo run -p ntw_formats --example bink_probe -- frames <movie path> [n]
use ntw_formats::bink::{split_packet, AudioDecoder, BinkHeader, VideoDecoder};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn header(vfs: &Vfs, path: &str) -> BinkHeader {
    let fixed = vfs.read_range(path, 0, BinkHeader::FIXED_LEN).unwrap();
    let need = BinkHeader::needed_len(&fixed).unwrap();
    BinkHeader::parse(&vfs.read_range(path, 0, need).unwrap()).unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let movies: Vec<String> = vfs.list("movies\\").into_iter().filter(|p| p.ends_with(".bik")).map(String::from).collect();
    match args.first().map(String::as_str) {
        Some("survey") => {
            println!("{:<34} {:>3} {:>9} {:>5}x{:<4} {:>6} {:>7} {:>5} {:>9} tracks(rate/flags/id/maxsz)", "movie", "rev", "bytes", "w", "h", "fps", "frames", "keys", "secs");
            for p in &movies {
                let h = header(&vfs, p);
                let keys = h.frames.iter().filter(|f| f.keyframe).count();
                let tr: Vec<String> = h.audio_tracks.iter().map(|t| format!("{}/{:04x}/{}/{}", t.sample_rate, t.flags, t.id, t.max_decoded_size)).collect();
                println!(
                    "{:<34} {:>3} {:>9} {:>5}x{:<4} {:>6.3} {:>7} {:>5} {:>9.2} {} [{}] flags={:x}",
                    p.trim_start_matches("movies\\"), h.revision as char, h.file_size, h.width, h.height, h.fps(), h.num_frames, keys,
                    h.duration_secs(), h.audio_tracks.len(), tr.join(" "), h.flags
                );
            }
        }
        Some("frames") => {
            let p = &args[1];
            let n: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(5);
            let h = header(&vfs, p);
            for (i, f) in h.frames.iter().take(n).enumerate() {
                let b = vfs.read_range(p, f.offset as u64, f.size as usize).unwrap();
                let pk = split_packet(&b, h.audio_tracks.len()).unwrap();
                let a: Vec<String> = pk.audio[..pk.num_audio].iter().map(|a| {
                    if a.len() >= 4 { format!("{}:{}", a.len(), u32::from_le_bytes(a[..4].try_into().unwrap())) } else { format!("{}", a.len()) }
                }).collect();
                let v = pk.video;
                let w: Vec<String> = v.chunks(4).take(4).map(|c| format!("{:08x}", u32::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0), c.get(2).copied().unwrap_or(0), c.get(3).copied().unwrap_or(0)]))).collect();
                println!("frame {i} key={} size={} audio=[{}] video={} first words {}", f.keyframe, f.size, a.join(" "), v.len(), w.join(" "));
            }
        }
        Some("decode") => {
            // decode <path> <frames> [png_every] [png_prefix]: decode frames, check plane ends, dump PNGs.
            let p = &args[1];
            let n: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);
            let every: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(0);
            let prefix = args.get(4).cloned().unwrap_or_default();
            let h = header(&vfs, p);
            let mut dec = VideoDecoder::new(&h).unwrap();
            let mut rgba = vec![0u8; (h.width * h.height * 4) as usize];
            let t0 = std::time::Instant::now();
            for (i, f) in h.frames.iter().take(n).enumerate() {
                let b = vfs.read_range(p, f.offset as u64, f.size as usize).unwrap();
                let pk = split_packet(&b, h.audio_tracks.len()).unwrap();
                match dec.decode(pk.video) {
                    Ok(e) => {
                        let yal = e.y_end.div_ceil(32) * 4;
                        let c2al = e.c2_end.div_ceil(32) * 4;
                        let ok = yal == e.chroma_offset && c2al == pk.video.len();
                        if !ok || i < 3 { println!("frame {i}: y_end {} (aligned {yal}) chroma_off {} c1_end {} c2_end {} (aligned {c2al}) len {} {}", e.y_end, e.chroma_offset, e.c1_end, e.c2_end, pk.video.len(), if ok { "OK" } else { "MISMATCH" }); }
                    }
                    Err(e) => { println!("frame {i}: ERROR {e}"); break; }
                }
                if every > 0 && i % every == 0 {
                    dec.frame().to_rgba(&mut rgba);
                    write_png(&format!("{prefix}{i:05}.png"), h.width, h.height, &rgba);
                }
            }
            println!("{n} frames in {:.2}s", t0.elapsed().as_secs_f64());
        }
        Some("bench") => {
            // bench <path> <frames>: decode + RGBA conversion speed (reads first, decodes after).
            let p = &args[1];
            let h = header(&vfs, p);
            let n: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(h.frames.len()).min(h.frames.len());
            let packets: Vec<Vec<u8>> = h.frames[..n].iter().map(|f| vfs.read_range(p, f.offset as u64, f.size as usize).unwrap()).collect();
            let mut dec = VideoDecoder::new(&h).unwrap();
            let mut rgba = vec![0u8; (h.width * h.height * 4) as usize];
            let (mut td, mut tc) = (0f64, 0f64);
            for b in &packets {
                let pk = split_packet(b, h.audio_tracks.len()).unwrap();
                let t = std::time::Instant::now();
                dec.decode(pk.video).unwrap();
                td += t.elapsed().as_secs_f64();
                let t = std::time::Instant::now();
                dec.frame().to_rgba(&mut rgba);
                tc += t.elapsed().as_secs_f64();
            }
            println!("{n} frames {}x{}: decode {:.2} ms/frame, to_rgba {:.2} ms/frame (budget {:.1} ms)", h.width, h.height, td * 1000.0 / n as f64, tc * 1000.0 / n as f64, 1000.0 / h.fps());
        }
        Some("audio") => {
            // audio <path> [track] [wav]: decode one track, check packet lengths, optional WAV.
            let p = &args[1];
            let ti: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(0);
            let h = header(&vfs, p);
            let t = h.audio_tracks[ti];
            let mut dec = AudioDecoder::new(&t).unwrap();
            let mut pcm = Vec::new();
            let mut bad = 0;
            let mut expected = 0usize;
            let t0 = std::time::Instant::now();
            for (i, f) in h.frames.iter().enumerate() {
                let b = vfs.read_range(p, f.offset as u64, f.size as usize).unwrap();
                let pk = split_packet(&b, h.audio_tracks.len()).unwrap();
                let a = pk.audio[ti];
                if a.len() >= 4 { expected += u32::from_le_bytes(a[..4].try_into().unwrap()) as usize / 2; }
                match dec.decode_packet(a, &mut pcm) {
                    Ok(used) => if a.len() >= 4 && used != a.len() - 4 { bad += 1; if bad < 5 { println!("frame {i}: used {used} of {}", a.len() - 4); } },
                    Err(e) => { println!("frame {i}: {e}"); break; }
                }
            }
            let peak = pcm.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
            let clipped = pcm.iter().filter(|s| **s == 32767 || **s == -32768).count();
            println!("track {ti} id {} {} Hz {} ch: {} samples (expected {expected}), {:.2}s audio vs {:.2}s video, peak {peak}, clipped {clipped}, {bad} packets with leftover bytes, {:.2}s", t.id, t.sample_rate, t.channels(), pcm.len(), pcm.len() as f64 / t.channels() as f64 / t.sample_rate as f64, h.duration_secs(), t0.elapsed().as_secs_f64());
            if let Some(w) = args.get(3) { write_wav(w, t.sample_rate as u32, t.channels(), &pcm); }
        }
        _ => eprintln!("usage: bink_probe survey | frames <path> [n] | decode <path> <n> [png_every] [prefix]"),
    }
}
/// Minimal PNG writer (RGBA8) for looking at decoded data. Research output only.
pub fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    let mut raw = Vec::with_capacity((w * h * 4 + h) as usize);
    for row in rgba.chunks_exact(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |ty: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = ty.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc(&c).to_be_bytes());
    };
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, out).unwrap();
}

/// Minimal 16-bit PCM WAV writer (scratch output only; never commit decoded audio).
pub fn write_wav(path: &str, rate: u32, channels: u16, pcm: &[i16]) {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

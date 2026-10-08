//! Sound data against a real install. All `#[ignore]`d (they need the game). Run with:
//! ```text
//! cargo test -p ntw_formats --test sound_install -- --ignored --nocapture
//! ```
//! Read-only. `NTW_DATA_DIR` overrides the install's `data` folder.
//! `decode_every_sound_file` decodes every `.wav`/`.mp3` in every pack (about 40,000 files)
//! on all cores; it takes a few minutes in a release build.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use ntw_formats::pack::Vfs;
use ntw_formats::sound::anim_events::AnimSoundEvents;
use ntw_formats::sound::names::normalize_sound_path;
use ntw_formats::sound::slots::SLOT_NAMES;
use ntw_formats::sound::{decode, SoundBankDatabase, SoundEvents, SoundLibrary};

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

fn vfs() -> Vfs {
    Vfs::open_install(data_dir()).expect("install")
}

#[test]
#[ignore = "needs the game installed"]
fn sound_events_parse_exactly() {
    let vfs = vfs();
    let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
    assert_eq!(se.categories.len(), 53);
    assert_eq!(se.params.len(), 962);
    assert_eq!(se.events.len(), 3471);
    assert_eq!(se.slots.len(), 401);
    assert_eq!(se.emitters.len(), 5);
    assert_eq!(se.movies.len(), 14);
    // Every slot is either empty (-1) or a valid event.
    assert!(se.slots.iter().all(|&s| s == u32::MAX || (s as usize) < se.events.len()));
    // The front-end music slot plays the original front-end theme.
    let slot = SLOT_NAMES.iter().position(|n| *n == "MUSIC_FRONTEND").unwrap();
    let ev = &se.events[se.slot_event(slot).unwrap()];
    assert_eq!(se.category_name(ev), "music");
    assert_eq!(ev.files, vec![r"front_end_music\ntw_mus01.mp3".to_string()]);
    // Every referenced sound file exists in the Vfs, except a few the shipped data names but
    // never shipped (reported, not fatal: the original plays nothing for them either).
    let mut missing = Vec::new();
    let mut total = 0;
    for e in &se.events {
        for f in &e.files {
            total += 1;
            if !vfs.contains(&normalize_sound_path(f)) {
                missing.push(f.clone());
            }
        }
    }
    missing.sort();
    missing.dedup();
    println!("{total} file references, {} distinct missing: {:?}", missing.len(), &missing[..missing.len().min(20)]);
    assert!(missing.len() * 50 < total, "too many missing sound files");
}

#[test]
#[ignore = "needs the game installed"]
fn sound_bank_database_parses_exactly() {
    let vfs = vfs();
    let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
    let db = SoundBankDatabase::read(&vfs.read(SoundBankDatabase::PATH).unwrap()).unwrap();
    assert_eq!(db.settings.len(), 154);
    assert_eq!(db.banks.len(), 28);
    let entries: usize = db.banks.iter().map(|b| b.entries.len()).sum();
    assert_eq!(entries, 1381);
    for b in &db.banks {
        for e in &b.entries {
            assert!(e.event == u32::MAX || (e.event as usize) < se.events.len(), "bank {} event {}", b.bank_type, e.event);
        }
    }
}

#[test]
#[ignore = "needs the game installed"]
fn library_names_and_vocabulary() {
    let lib = SoundLibrary::load(&vfs()).unwrap();
    println!("named events {}/{}, unmatched CSV rows {}", lib.names.named_count(), lib.events.events.len(), lib.names.unmatched_rows);
    assert!(lib.names.named_count() > 3400);
    assert_eq!(lib.setting("SPEED_OF_SOUND_IN_METRES_PER_SECOND"), Some(340.29));
    // Projectile fire: a flintlock musket firing a bullet, close by.
    let t = lib.vocabulary.bank_type_of("sound_bank_projectile_fire").unwrap();
    let q = lib.vocabulary.query(t, &[("gun_type", "musket_flintlock"), ("shot_type", "bullet"), ("audio_distance", "close")]).unwrap();
    let hit = lib.banks.bank(t).unwrap().best_match(&q).unwrap();
    let ev = &lib.events.events[hit.event as usize];
    assert!(ev.files[0].contains("arquebus"), "{:?}", ev.files);
    // Music states: the front end.
    let m = lib.vocabulary.bank_type_of("sound_bank_music_states").unwrap();
    let q = lib.vocabulary.query(m, &[("music_state", "music_front_end")]).unwrap();
    let first = lib.banks.bank(m).unwrap().best_match(&q).unwrap();
    assert_eq!(Some(first.event as usize), lib.slot_event("MUSIC_FRONTEND"));
    // UI: a front-end button's click sound is the event named after the component.
    let e = lib.event_by_name("grand_campaign").unwrap();
    assert_eq!(lib.events.category_name(&lib.events.events[e]), "ui");
}

#[test]
#[ignore = "needs the game installed"]
fn every_anim_sound_event_parses() {
    let vfs = vfs();
    let mut n = 0;
    for p in vfs.packs() {
        for e in p.entries() {
            if e.path.to_ascii_lowercase().ends_with(".anim_sound_event") {
                let a = AnimSoundEvents::read(&p.read_entry(e).unwrap()).unwrap_or_else(|| panic!("{}", e.path));
                assert_eq!(a.version, 1, "{}", e.path);
                n += 1;
            }
        }
    }
    println!("{n} .anim_sound_event files");
    assert!(n > 2600);
}

/// The silent placeholder files the events name (e.g. deployment music for subcultures without
/// any) decode to audio that is all silence. The player plays them like any other file (no name
/// rule, as in the exe), so one that decoded to no audio would be marked failed as music and the
/// old track would carry over instead. The data also names `uk\placeholder\...`, never shipped
/// (like the other missing files of `sound_events_parse_exactly`): reported, not fatal.
#[test]
#[ignore = "needs the game installed"]
fn silent_placeholder_files_decode_to_silence() {
    let vfs = vfs();
    let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
    let mut files: Vec<String> = se.events.iter().flat_map(|e| &e.files).map(|f| normalize_sound_path(f)).filter(|f| f.contains("placeholder")).collect();
    files.sort();
    files.dedup();
    let mut checked = 0;
    for f in &files {
        let Ok(bytes) = vfs.read(f) else {
            println!("{f}: not shipped");
            continue;
        };
        checked += 1;
        let pcm = decode(bytes, f.rsplit('.').next()).unwrap_or_else(|e| panic!("{f}: {e}"));
        println!("{f}: {} frames at {} Hz, {} ch", pcm.frames(), pcm.sample_rate, pcm.channels);
        assert!(pcm.frames() > 0, "{f}: no audio");
        assert!(pcm.samples.iter().all(|&s| s == 0.0), "{f}: not silent");
    }
    assert!(checked > 0, "no placeholder file shipped: {files:?}");
}

#[test]
#[ignore = "needs the game installed; slow (decodes ~40,000 files)"]
fn decode_every_sound_file() {
    let vfs = vfs();
    let mut jobs = Vec::new();
    for (pi, p) in vfs.packs().iter().enumerate() {
        for (ei, e) in p.entries().iter().enumerate() {
            let l = e.path.to_ascii_lowercase();
            if l.ends_with(".wav") || l.ends_with(".mp3") {
                jobs.push((pi, ei));
            }
        }
    }
    let next = AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(pi, ei)) = jobs.get(i) else { break };
                    let p = &vfs.packs()[pi];
                    let e = &p.entries()[ei];
                    let ext = e.path.rsplit('.').next().map(str::to_ascii_lowercase);
                    let bytes = p.read_entry(e).unwrap();
                    match decode(bytes, ext.as_deref()) {
                        Ok(pcm) if pcm.channels > 0 && pcm.sample_rate > 0 => {}
                        Ok(_) => failures.lock().unwrap().push(format!("{}: empty format", e.path)),
                        Err(err) => failures.lock().unwrap().push(format!("{}: {err}", e.path)),
                    }
                }
            });
        }
    });
    let f = failures.into_inner().unwrap();
    println!("{} sound files, {} failed: {:?}", jobs.len(), f.len(), &f[..f.len().min(30)]);
    assert!(f.is_empty());
}

/// `loop_start_block` / `loop_end_block` are byte offsets into the file (Miles'
/// `AIL_set_*_loop_block`, set by `0x01004430`), not sample frames. Treating them as frames
/// looped every `ntw\music` battle track at 42-45 % of its length ("music restarts partway").
/// Checks, for every looped event with a loop block:
/// - the offsets fall on MPEG frame boundaries (the designers' tool picked frame starts);
/// - the frame walk agrees with the decoder (same PCM frame count, on the timed decode);
/// - the mapped loop end is near the end of the track (> 75 % of its frames), the loop
///   start before it, and decoding up to the loop end gives the same samples.
#[test]
#[ignore = "needs the game installed"]
fn music_loop_blocks_are_byte_offsets() {
    use ntw_formats::sound::decode_timed;
    use ntw_formats::sound::events::Param;
    use ntw_formats::sound::loop_points::{has_loop_block, mp3_frames, LoopIndex};
    let vfs = vfs();
    let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
    let mut checked = 0;
    let mut done = std::collections::BTreeSet::new();
    for e in &se.events {
        let p = se.params_of(e);
        let (a, b) = (p.get(Param::LoopStartBlock), p.get(Param::LoopEndBlock));
        if !p.flag(Param::Looped) || !has_loop_block(a, b) {
            continue;
        }
        for f in &e.files {
            let f = normalize_sound_path(f);
            if !done.insert((f.clone(), a.to_bits(), b.to_bits())) {
                continue;
            }
            let Ok(bytes) = vfs.read(&f) else { continue };
            let index = LoopIndex::new(&bytes);
            let (s, end) = match index.region(a, b) {
                Ok(Some(r)) => r,
                // No loop block: the only exe reason left for a non-zero pair is an end at or past
                // the data (the silent placeholder WAV's data chunk, one mus076 event made for a
                // longer edit); the whole file loops.
                Ok(None) => {
                    assert!(b as u64 >= index.data_len(), "{f}: no region for {a}..{b} although the end is inside the {} data bytes", index.data_len());
                    println!("{f}: end {b} at or past the {} data bytes, whole file loops", index.data_len());
                    continue;
                }
                // Walk stopped early, unknown format, negative or reversed offsets.
                Err(e) => panic!("{f}: loop block {a}..{b} cannot be mapped: {e}"),
            };
            let frames = mp3_frames(&bytes);
            assert!(frames.iter().any(|x| x.offset == a as u64), "{f}: start {a} not a frame start");
            assert!(frames.iter().any(|x| x.offset == b as u64), "{f}: end {b} not a frame start");
            let walked: u64 = frames.iter().filter(|x| !x.tag).map(|x| u64::from(x.samples)).sum();
            // The timeline the game plays a loop region on (damaged frames kept as silence).
            let pcm = decode_timed(bytes.clone(), Some("mp3"), None).unwrap_or_else(|e| panic!("{f}: decode failed: {e}"));
            assert_eq!(walked, pcm.frames() as u64, "{f}: frame walk disagrees with the decoder");
            let (total, rate) = (pcm.frames() as u64, f64::from(pcm.sample_rate));
            println!("{f}: loop {s}..{end} of {total} frames ({:.1}..{:.1} s of {:.1} s)", s as f64 / rate, end as f64 / rate, total as f64 / rate);
            assert!(s < end && end <= total, "{f}: loop {s}..{end} outside 0..{total}");
            assert!(end as f64 > 0.75 * total as f64, "{f}: loop end {end} at {:.0} % of {total}", 100.0 * end as f64 / total as f64);
            // What the game decodes and holds: the file up to the loop end, sample-exact.
            let held = decode_timed(bytes, Some("mp3"), Some(end)).unwrap_or_else(|e| panic!("{f}: decode failed: {e}"));
            assert_eq!(held.samples, pcm.samples[..end as usize * usize::from(pcm.channels)], "{f}: decode up to the loop end differs");
            checked += 1;
        }
    }
    assert!(checked >= 20, "only {checked} looped files checked");
}

/// The start rules never refuse a music-state track in vanilla data: every event the music bank
/// names plays with probability 1, no reductions, no repeat delay and no at-once limit, so the
/// music state machine's "start rules refused the pick" branch is reached only by mods.
#[test]
#[ignore = "needs the game installed"]
fn music_state_tracks_pass_the_start_rules() {
    use ntw_formats::sound::events::Param;
    let lib = SoundLibrary::load(&vfs()).unwrap();
    let m = lib.vocabulary.bank_type_of("sound_bank_music_states").unwrap();
    let mut events: Vec<usize> = lib.banks.bank(m).unwrap().entries.iter().filter(|e| e.event != u32::MAX).map(|e| e.event as usize).collect();
    events.sort_unstable();
    events.dedup();
    for &e in &events {
        let p = lib.events.params_of(&lib.events.events[e]);
        let (prob, same, any) = (p.get(Param::Probability), p.get(Param::ProbabilityReductionSameEvents), p.get(Param::ProbabilityReductionAnyEvents));
        let (again, at_once) = (p.get(Param::DelayBeforeCanPlayAgain), p.get(Param::MaxNumberPlayingAtOnce));
        println!("event {e}: group {} looped {} prob {prob} -{same}/-{any} again {again} at once {at_once}", p.get(Param::Group), p.flag(Param::Looped));
        // Group 0 (music): the player decodes it whole before a voice starts, looped or not.
        assert_eq!(p.get(Param::Group), 0.0, "event {e}: not in the music group");
        assert!(prob >= 1.0 && same == 0.0 && any == 0.0, "event {e}: probability {prob} -{same}/-{any}");
        assert!(again <= 0.0, "event {e}: repeat delay {again}");
        assert!(at_once <= 0.0 || at_once >= 1000.0, "event {e}: at most {at_once} at once");
    }
    println!("{} music-state events", events.len());
    assert!(events.len() > 20);
}

/// The memory and background decode time of every music file (an event in group 0, looped or not;
/// and the biggest of all sounds) as the player loads music: the whole file decoded to 16-bit
/// samples (loop blocks are frame ranges into it), the buffer sized by the frame walk. Run in
/// release for real times:
/// `cargo test -p ntw_formats --release --test sound_install -- --ignored music_decoded_size --nocapture`.
#[test]
#[ignore = "needs the game installed"]
fn music_decoded_size() {
    use ntw_formats::sound::events::Param;
    use ntw_formats::sound::decode_timed_i16;
    use ntw_formats::sound::loop_points::LoopIndex;
    let vfs = vfs();
    let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
    let (mut biggest, mut slowest) = ((0usize, String::new()), (std::time::Duration::ZERO, String::new()));
    let mut biggest_any = (0usize, String::new());
    let mut done = std::collections::BTreeSet::new();
    for e in &se.events {
        let p = se.params_of(e);
        let music = p.get(Param::Group) == 0.0;
        for f in &e.files {
            let f = normalize_sound_path(f);
            if !done.insert(f.clone()) {
                continue;
            }
            let Ok(bytes) = vfs.read(&f) else { continue };
            let t = std::time::Instant::now();
            let frames = LoopIndex::new(&bytes).decoded_frames();
            let ext = f.rsplit('.').next().map(str::to_owned);
            let Ok(pcm) = decode_timed_i16(bytes, ext.as_deref(), frames) else { continue };
            let took = t.elapsed();
            let size = pcm.samples.len() * std::mem::size_of::<i16>();
            if size > biggest_any.0 {
                biggest_any = (size, f.clone());
            }
            if !music {
                continue;
            }
            // Music is held decoded: its buffer is allocated once, at its final size.
            if frames.is_some() {
                assert_eq!(pcm.samples.capacity(), pcm.samples.len(), "{f}: buffer not sized exactly");
            }
            if size > biggest.0 {
                biggest = (size, f.clone());
            }
            if took > slowest.0 {
                slowest = (took, f.clone());
            }
        }
    }
    println!("biggest music file decoded: {:.1} MB ({})", biggest.0 as f64 / (1024.0 * 1024.0), biggest.1);
    println!("biggest sound of any event decoded: {:.1} MB ({})", biggest_any.0 as f64 / (1024.0 * 1024.0), biggest_any.1);
    println!("slowest background decode: {:.0} ms ({})", slowest.0.as_secs_f64() * 1000.0, slowest.1);
    assert!(!biggest.1.is_empty());
}

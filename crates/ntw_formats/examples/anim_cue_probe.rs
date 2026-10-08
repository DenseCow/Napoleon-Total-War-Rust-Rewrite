//! Research helper for the `.anim_sound_event` cue numbers (MIDDLEWARE_VERIFY.md §3): for each cue,
//! how many files use it, the time range, and example animation paths. Read-only.
//!   cargo run -p ntw_formats --release --example anim_cue_probe [examples per cue]
use std::collections::BTreeMap;

use ntw_formats::pack::Vfs;
use ntw_formats::sound::anim_events::AnimSoundEvents;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let per: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let mut cues: BTreeMap<u32, (usize, f32, f32, Vec<String>)> = BTreeMap::new();
    for p in vfs.list("animations").into_iter().filter(|p| p.ends_with(".anim_sound_event")) {
        let Some(a) = vfs.read(p).ok().and_then(|b| AnimSoundEvents::read(&b)) else { continue };
        for c in &a.cues {
            let e = cues.entry(c.cue).or_insert((0, f32::MAX, 0.0, Vec::new()));
            e.0 += 1;
            e.1 = e.1.min(c.time);
            e.2 = e.2.max(c.time);
            if e.3.len() < per {
                e.3.push(p.rsplit(['\\', '/']).next().unwrap_or(p).to_owned());
            }
        }
    }
    for (cue, (n, lo, hi, ex)) in &cues {
        println!("{cue:4} x{n:<5} t {lo:.2}..{hi:.2}  {}", ex.join(" "));
    }
}

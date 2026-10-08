//! ASCII + UTF-16LE string extraction and keyword categorisation.
use crate::pe::Pe;
use std::fmt::Write as _;

pub struct Str { pub off: usize, pub va: u32, pub sec: String, pub enc: char, pub s: String }

fn printable(c: u8) -> bool { (0x20..=0x7e).contains(&c) || c == b'\t' }

pub fn extract(p: &Pe, min: usize) -> Vec<Str> {
    let d = &p.d;
    let mut out = vec![];
    // ASCII
    let mut i = 0;
    while i < d.len() {
        if printable(d[i]) {
            let st = i;
            while i < d.len() && printable(d[i]) { i += 1; }
            if i - st >= min { out.push((st, 'A', String::from_utf8_lossy(&d[st..i]).to_string())); }
        } else { i += 1; }
    }
    // UTF-16LE (both alignments)
    for start in 0..2 {
        let mut i = start;
        while i + 1 < d.len() {
            if printable(d[i]) && d[i + 1] == 0 {
                let st = i;
                let mut s = String::new();
                while i + 1 < d.len() && printable(d[i]) && d[i + 1] == 0 { s.push(d[i] as char); i += 2; }
                if s.len() >= min { out.push((st, 'U', s)); }
            } else { i += 2; }
        }
    }
    out.sort_by_key(|x| x.0);
    out.into_iter().filter_map(|(off, enc, s)| {
        let sec = p.section_of_off(off).to_string();
        if sec == ".reloc" { return None; }
        if sec == ".text" {
            // keep only wordy strings in code
            let mut run = 0; let mut ok = false;
            for c in s.chars() { if c.is_ascii_alphabetic() { run += 1; if run >= 5 { ok = true; break; } } else { run = 0; } }
            if !ok { return None; }
        }
        Some(Str { off, va: p.off2va(off).unwrap_or(0), sec, enc, s })
    }).collect()
}

fn has_any(s: &str, keys: &[&str]) -> bool { keys.iter().any(|k| s.contains(k)) }

pub fn categorize(s: &str) -> &'static str {
    let l = s.to_ascii_lowercase();
    if s.starts_with(".?AV") || s.starts_with(".?AU") { return "rtti"; }
    if [".cpp", ".h\"", ".hpp", ".inl", ".c\""].iter().any(|e| l.ends_with(&e.replace('"', ""))) && (l.contains('\\') || l.contains('/')) { return "source_paths"; }
    if s.starts_with("??") || (s.starts_with('?') && s.contains('@')) { return "mangled"; }
    if has_any(&l, &["assert", "failed", "unreachable", "!= null", "== null"]) { return "assert"; }
    if l.starts_with("db/") || l.starts_with("db\\") || l.contains("/db/") || l.ends_with("_tables") || l.ends_with("_table") { return "db_tables"; }
    if has_any(&l, &["lua", "__index", "__newindex", "require"]) { return "lua"; }
    if has_any(&l, &["steam", "gamespy", "lobby", "matchmak", "socket", "multiplayer", "leaderboard", "achievement", "p2p", "network"]) { return "network"; }
    let exts = [".pack", ".esf", ".lua", ".rigid_model", ".loc", ".bin", ".cs2", ".wsmodel", ".xml", ".txt", ".tga", ".dds", ".png", ".jpg", ".fx", ".bik", ".wav", ".mp3", ".ogg", ".anim", ".frg", ".fsm", ".parsed", ".variant", ".script", ".save", ".replay", ".dat", ".cfg", ".ini", ".log", ".csc", ".atlas", ".twui", ".world", ".unit_variant", ".tai"];
    if exts.iter().any(|e| l.contains(e)) { return "file_ext"; }
    if has_any(&l, &["d3d", "direct3d", "shader", "texture", "render", "vertex", "technique"]) { return "d3d_render"; }
    if has_any(&l, &["miles", "ail_", "bink", "sound", "music", "audio"]) { return "audio_video"; }
    // preference-like key: lower_snake with known prefix
    let pref_prefix = ["gfx_", "audio_", "camera_", "ui_", "battle_", "campaign_", "game_", "net_", "video_", "controls_", "scripting_", "graphics_", "sound_", "input_", "mod_", "default_", "porthole_", "voice_", "x_res", "y_res"];
    if pref_prefix.iter().any(|k| l.starts_with(k)) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') { return "preferences"; }
    if has_any(&l, &["error", "warning", "could not", "cannot", "unable", "invalid", "%s", "%d", "%i", "%f", "%x", "%u"]) { return "log_error"; }
    if s.len() >= 5 && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && s.chars().next().unwrap().is_ascii_uppercase() { return "class_like"; }
    "other"
}

pub fn run(p: &Pe, outdir: &str) {
    let v = extract(p, 6);
    let mut all = String::new();
    for x in &v { let _ = writeln!(all, "0x{:08x}\t0x{:08x}\t{}\t{}\t{}", x.off, x.va, x.sec, x.enc, x.s.replace('\t', " ")); }
    std::fs::write(format!("{}/strings_all_rs.tsv", outdir), all).unwrap();
    let cats = ["source_paths", "rtti", "mangled", "assert", "db_tables", "lua", "network", "file_ext", "d3d_render", "audio_video", "preferences", "log_error", "class_like", "other"];
    let mut buckets: Vec<Vec<&Str>> = vec![vec![]; cats.len()];
    for x in &v { let c = categorize(&x.s); let i = cats.iter().position(|k| *k == c).unwrap(); buckets[i].push(x); }
    let mut o = String::new();
    let _ = writeln!(o, "Total strings: {} (ASCII {}, UTF16 {})", v.len(), v.iter().filter(|x| x.enc == 'A').count(), v.iter().filter(|x| x.enc == 'U').count());
    for (i, c) in cats.iter().enumerate() { let _ = writeln!(o, "{:<14} {}", c, buckets[i].len()); }
    for (i, c) in cats.iter().enumerate() {
        let _ = writeln!(o, "\n######## {} ({})", c, buckets[i].len());
        for x in &buckets[i] { let s: String = x.s.chars().take(300).collect(); let _ = writeln!(o, "0x{:08x} 0x{:08x} {:<6} {} {}", x.off, x.va, x.sec, x.enc, s); }
    }
    std::fs::write(format!("{}/strings_categorized_rs.txt", outdir), &o).unwrap();
    for (i, c) in cats.iter().enumerate() { println!("{:<14} {}", c, buckets[i].len()); }
    println!("total {}", v.len());
}

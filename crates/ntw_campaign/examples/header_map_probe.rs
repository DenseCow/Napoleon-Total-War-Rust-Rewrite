//! Research helper: how the original builds the save header's territory picture
//! (`SAVE_GAME_HEADER/MAPS`, SAVE_COMPAT.md §25). Read-only.
//!
//! `header_map_probe <save>`: for each MAPS item, compares every pixel with the theatre's
//! `<x>_map.tga` and `<x>_lookup.tga` (data/campaign_maps/<map>/), groups the pixels by the region
//! under them (lookup palette colour = `regions` DB colour) and by the region owner's relation to
//! the save's faction, and prints the colour changes found.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_formats::tga::Tga;

/// Pixels, pixels unchanged, and (base, header) colour pairs with counts.
type Stats = (usize, usize, BTreeMap<([u8; 4], [u8; 4]), usize>);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let vfs = Vfs::open_install(&dir).expect("install");
    let db = GameDatabase::from_install(&dir).expect("db");
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    let bytes = std::fs::read(&args[0]).expect("read");
    let esf = EsfFile::from_bytes(&bytes).expect("esf");
    let l = ntw_campaign::read_esf(&esf, &db).expect("load");
    let human = l.info.header.faction_key.clone();
    let w = &l.model.world;
    let fkey: HashMap<_, _> = w.factions.values().map(|f| (f.id, f.key.clone())).collect();
    let map_folder = l.info.map_key.trim_start_matches("campaign_maps/").to_owned();
    // region colour -> (region key, owner key)
    let owner_of: HashMap<[u8; 3], (String, String)> = w
        .regions
        .values()
        .filter_map(|r| {
            let rec = db.region(&r.key)?;
            Some((rec.colour(), (r.key.clone(), fkey.get(&r.owner).cloned().unwrap_or_default())))
        })
        .collect();
    for m in &l.info.header.maps {
        let x = m.theatre.trim_end_matches("_main");
        let read = |name: &str| files.read(&format!("campaign_maps/{map_folder}/{name}")).ok().and_then(|b| Tga::decode(&b).ok());
        let (Some(base), Some(lookup)) = (read(&format!("{x}_map.tga")), read(&format!("{x}_lookup.tga"))) else {
            println!("{}: no {x}_map.tga / {x}_lookup.tga in {map_folder}", m.theatre);
            continue;
        };
        println!(
            "{} {}x{} pitch {}; {x}_map.tga {}x{}; {x}_lookup.tga {}x{} ({} palette entries); human {human}",
            m.theatre, m.width, m.height, m.pitch, base.width, base.height, lookup.width, lookup.height, lookup.palette.len()
        );
        let hdr = m.rgba();
        // relation -> (pixels, same as base, colour change histogram)
        let mut stats: BTreeMap<String, Stats> = BTreeMap::new();
        let mut per_region: BTreeMap<String, (String, usize, usize)> = BTreeMap::new();
        for y in 0..m.height as usize {
            for xx in 0..m.width as usize {
                let i = y * m.width as usize + xx;
                let h = [hdr[4 * i], hdr[4 * i + 1], hdr[4 * i + 2], hdr[4 * i + 3]];
                let scale = |v: usize, from: u32, to: u32| (v * to as usize / from as usize).min(to as usize - 1);
                let bi = scale(y, m.height, base.height) * base.width as usize + scale(xx, m.width, base.width);
                let b = [base.rgba[4 * bi], base.rgba[4 * bi + 1], base.rgba[4 * bi + 2], base.rgba[4 * bi + 3]];
                let li = scale(y, m.height, lookup.height) * lookup.width as usize + scale(xx, m.width, lookup.width);
                let lc = if lookup.indices.is_empty() {
                    [lookup.rgba[4 * li], lookup.rgba[4 * li + 1], lookup.rgba[4 * li + 2]]
                } else {
                    let p = lookup.palette[lookup.indices[li] as usize];
                    [p[0], p[1], p[2]]
                };
                let rel = match owner_of.get(&lc) {
                    Some((_, o)) if *o == human => "own".to_string(),
                    Some((_, o)) if o == "rebels" => "rebels".to_string(),
                    Some(_) => "other".to_string(),
                    None => "no region".to_string(),
                };
                let e = stats.entry(rel).or_default();
                e.0 += 1;
                e.1 += usize::from(h == b);
                *e.2.entry((b, h)).or_default() += 1;
                if let Some((k, o)) = owner_of.get(&lc) {
                    let r = per_region.entry(k.clone()).or_insert((o.clone(), 0, 0));
                    r.1 += 1;
                    r.2 += usize::from((0..3).any(|c| (h[c] as i32 - b[c] as i32).abs() > 2));
                }
            }
        }
        for (rel, (n, same, hist)) in &stats {
            let mut top: Vec<_> = hist.iter().collect();
            top.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
            println!("  {rel}: {n} pixels, {same} unchanged; top changes (base -> header):");
            for ((b, h), c) in top.iter().take(6) {
                println!("    {b:?} -> {h:?}: {c}");
            }
        }
        let changed: Vec<String> = per_region.iter().filter(|(_, v)| v.2 * 2 > v.1).map(|(k, v)| format!("{k}({})", v.0)).collect();
        println!("  regions mostly tinted: {changed:?}");
        let own: Vec<String> = per_region.iter().filter(|(_, v)| v.0 == human).map(|(k, v)| format!("{k} {}/{}", v.2, v.1)).collect();
        println!("  own regions (tinted/pixels): {own:?}");
        // Fits over all pixels: plain = the base scaled by 255/256 (floor); tint per channel h = k b + c.
        let mut plain = [0usize; 3];
        let mut sums = [[0f64; 5]; 3];
        let mut tinted = 0;
        for i in 0..(m.width * m.height) as usize {
            let h = [hdr[4 * i], hdr[4 * i + 1], hdr[4 * i + 2]];
            let b = [base.rgba[4 * i], base.rgba[4 * i + 1], base.rgba[4 * i + 2]];
            let t = (0..3).any(|c| (h[c] as i32 - b[c] as i32).abs() > 2);
            if !t {
                plain[0] += 1;
                plain[1] += usize::from((0..3).all(|c| h[c] as u32 == b[c] as u32 * 255 / 256));
                plain[2] += usize::from((0..3).all(|c| h[c] as i32 == (b[c] as i32 - 1).max(0)));
            } else {
                tinted += 1;
                for c in 0..3 {
                    let (x, y) = (b[c] as f64, h[c] as f64);
                    let s = &mut sums[c];
                    s[0] += 1.0; s[1] += x; s[2] += y; s[3] += x * x; s[4] += x * y;
                }
            }
        }
        // Brute force: h = floor((b*a + g*(256-a)) / 256 * 255 / 256) and variants, per channel.
        for c in 0..3 {
            let mut pairs: BTreeMap<(u8, u8), usize> = BTreeMap::new();
            for i in 0..(m.width * m.height) as usize {
                let h = [hdr[4 * i], hdr[4 * i + 1], hdr[4 * i + 2]];
                let b = [base.rgba[4 * i], base.rgba[4 * i + 1], base.rgba[4 * i + 2]];
                if (0..3).any(|k| (h[k] as i32 - b[k] as i32).abs() > 2) {
                    *pairs.entry((b[c], h[c])).or_default() += 1;
                }
            }
            if std::env::var_os("PAIRS").is_some() {
                let mut by_b: BTreeMap<u8, Vec<(u8, usize)>> = BTreeMap::new();
                for ((b, h), n) in &pairs { by_b.entry(*b).or_default().push((*h, *n)); }
                for (b, hs) in by_b.iter().filter(|(_, hs)| hs.iter().map(|x| x.1).sum::<usize>() > 40) { println!("    ch{c} b={b}: {hs:?}"); }
            }
            let total: usize = pairs.values().sum();
            let mut best = (0usize, 0u32, 0u32, 0u8);
            for a in 80..=100u32 {
                for g in 0..=255u32 {
                    for v in 0..5u8 {
                        let n: usize = pairs.iter().filter(|((b, h), _)| {
                            let mix = *b as u32 * a + g * (256 - a);
                            let p = match v { 0 => mix / 256 * 255 / 256, 1 => mix * 255 / 65536, 2 => (mix + 128) / 256 * 255 / 256, 4 => mix / 256, _ => ((mix as f64 / 256.0).round() as u32 * 255) / 256 };
                            p == *h as u32
                        }).map(|(_, n)| *n).sum();
                        if n > best.0 { best = (n, a, g, v); }
                    }
                }
            }
            println!("  channel {c}: best a={} g={} variant {} matches {}/{total}", best.1, best.2, best.3, best.0);
        }
        println!("  plain pixels {}: floor(b*255/256) {} ; b-1 {}", plain[0], plain[1], plain[2]);
        for (c, s) in sums.iter().enumerate() {
            let k = (s[0] * s[4] - s[1] * s[2]) / (s[0] * s[3] - s[1] * s[1]);
            let c0 = (s[2] - k * s[1]) / s[0];
            println!("  tinted {tinted}: channel {c}: h = {k:.4} b + {c0:.2}");
        }
    }
}

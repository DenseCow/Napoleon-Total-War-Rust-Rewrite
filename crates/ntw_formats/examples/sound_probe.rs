//! Research helper for the sound data (read-only). Usage:
//!   cargo run -p ntw_formats --example sound_probe -- strings <file> [min_len]
//!     lists u16-length-prefixed UTF-16LE strings with their offsets
//!   cargo run -p ntw_formats --example sound_probe -- formats
//!     histogram of .wav fmt chunks and .mp3 first bytes across every pack
use ntw_formats::pack::Vfs;
use std::collections::BTreeMap;
use std::env;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args[0].as_str() {
        "settings" => {
            // index, packed value, and the sound_settings.xml element (with the comment above it)
            use ntw_formats::sound::{library::SETTINGS_XML, SoundBankDatabase};
            let vfs = Vfs::open_install(DATA).unwrap();
            let db = SoundBankDatabase::read(&vfs.read(SoundBankDatabase::PATH).unwrap()).unwrap();
            let xml = ntw_formats::xml::decode_text(&vfs.read(SETTINGS_XML).unwrap());
            let mut i = 0;
            let mut comment = String::new();
            for line in xml.lines() {
                let l = line.trim();
                if l.starts_with("<!--") {
                    comment = l.to_string();
                    continue;
                }
                let Some(rest) = l.strip_prefix('<') else { continue };
                let Some((tag, after)) = rest.split_once('>') else { continue };
                let Some((text, _)) = after.split_once("</") else { continue };
                let v = db.settings.get(i).copied().unwrap_or(f32::NAN);
                println!("{i:3}\t{v}\t{tag}\t{}\t{comment}", text.trim());
                comment.clear();
                i += 1;
            }
        }
        "strings" => {
            let b = std::fs::read(&args[1]).unwrap();
            let min: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(3);
            let mut i = 0;
            while i + 2 <= b.len() {
                let n = u16::from_le_bytes([b[i], b[i + 1]]) as usize;
                if n >= min && i + 2 + n * 2 <= b.len() {
                    let s: Vec<u16> = (0..n).map(|k| u16::from_le_bytes([b[i + 2 + 2 * k], b[i + 3 + 2 * k]])).collect();
                    if s.iter().all(|&c| (32..127).contains(&c)) {
                        println!("{:08x} {}", i, String::from_utf16_lossy(&s));
                        i += 2 + n * 2;
                        continue;
                    }
                }
                i += 1;
            }
        }
        "formats" => {
            let vfs = Vfs::open_install(DATA).unwrap();
            let mut m: BTreeMap<String, (usize, String)> = BTreeMap::new();
            for p in vfs.packs() {
                let pn = p.path().file_name().unwrap().to_string_lossy().into_owned();
                for e in p.entries() {
                    let l = e.path.to_ascii_lowercase();
                    let key = if l.ends_with(".wav") {
                        let b = p.read_entry_prefix(e, 64).unwrap();
                        // find "fmt " chunk (assume it is first after RIFF/WAVE)
                        let pos = b.windows(4).position(|w| w == b"fmt ");
                        match pos {
                            Some(q) if q + 24 <= b.len() => {
                                let u16a = |o: usize| u16::from_le_bytes([b[q + o], b[q + o + 1]]);
                                let u32a = |o: usize| u32::from_le_bytes([b[q + o], b[q + o + 1], b[q + o + 2], b[q + o + 3]]);
                                format!("wav tag={:#x} ch={} rate={} bits={} align={}", u16a(8), u16a(10), u32a(12), u16a(22), u16a(20))
                            }
                            _ => format!("wav nofmt {:02x?}", &b[..b.len().min(16)]),
                        }
                    } else if l.ends_with(".mp3") {
                        let b = p.read_entry_prefix(e, 4).unwrap();
                        if b.starts_with(b"ID3") { "mp3 id3".into() } else if b.len() >= 2 && b[0] == 0xff && b[1] & 0xe0 == 0xe0 { format!("mp3 sync {:02x}{:02x}", b[1], b.get(2).copied().unwrap_or(0) & 0xf0) } else { format!("mp3 other {:02x?}", b) }
                    } else { continue };
                    let v = m.entry(format!("{key} [{pn}]")).or_insert((0, e.path.clone()));
                    v.0 += 1;
                }
            }
            for (k, v) in m { println!("{k:<70} {:>6} e.g. {}", v.0, v.1); }
        }
        "events" => {
            // events [filter]: parse sounds_packed\sound_events and print a summary + matching events
            use ntw_formats::sound::{slots::SLOT_NAMES, SoundEvents};
            let vfs = Vfs::open_install(DATA).unwrap();
            let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
            println!("header {:#x} categories {} params {} events {} keyed {} movies {}", se.header, se.categories.len(), se.params.len(), se.events.len(), se.emitters.len(), se.movies.len());
            println!("special {:?}", se.special_categories.iter().map(|&i| se.categories[i as usize].name.as_str()).collect::<Vec<_>>());
            let mut slot_of = vec![None; se.events.len()];
            for (s, &e) in se.slots.iter().enumerate() {
                if (e as usize) < slot_of.len() { slot_of[e as usize] = Some(s); }
            }
            let filt = args.get(1).map(|s| s.to_ascii_lowercase());
            for (i, e) in se.events.iter().enumerate() {
                let label = e.name.clone().or(slot_of[i].map(|s| format!("<{}>", SLOT_NAMES[s]))).unwrap_or_default();
                let cat = se.category_name(e);
                let line = format!("{i:5} {cat:<20} {label:<40} p{:<4} {:?}", e.params, e.files);
                if filt.as_ref().is_none_or(|f| line.to_ascii_lowercase().contains(f)) {
                    println!("{line}");
                    if filt.is_some() {
                        let p = se.params_of(e);
                        let v: Vec<String> = p.0.iter().map(|&x| { let f = f32::from_bits(x); if f.is_finite() && (f == 0.0 || f.abs() > 1e-6) { format!("{f}") } else { format!("#{x}") } }).collect();
                        println!("      {}", v.join(" "));
                    }
                }
            }
            if filt.as_deref() == Some("slots") {
                for (s, &e) in se.slots.iter().enumerate() {
                    let ev = se.events.get(e as usize);
                    println!("slot {s:3} {:<45} -> {e:5} {} {:?}", SLOT_NAMES[s], ev.map(|x| se.category_name(x)).unwrap_or("-"), ev.map(|x| x.files.first()));
                }
            }
            if filt.as_deref() == Some("keyed") {
                for (k, l) in &se.emitters { println!("emitters {k} {} {:?}", l.len(), &l[..l.len().min(2)]); }
                for m in &se.movies { println!("movie {m:?}"); }
            }
        }
        "banks" => {
            // banks [type]: parse sounds_packed\sound_bank_database; print settings and bank sizes, or one bank's entries
            use ntw_formats::sound::{SoundBankDatabase, SoundEvents, slots::SLOT_NAMES};
            let vfs = Vfs::open_install(DATA).unwrap();
            let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
            let db = SoundBankDatabase::read(&vfs.read(SoundBankDatabase::PATH).unwrap()).unwrap();
            let slot_of = |e: u32| se.slots.iter().position(|&s| s == e).map(|s| SLOT_NAMES[s]).unwrap_or("");
            match args.get(1) {
                None => {
                    println!("header {:#x} settings {:?}", db.header, db.settings);
                    for b in &db.banks {
                        println!("bank {:2} entries {:4}", b.bank_type, b.entries.len());
                    }
                }
                Some(t) => {
                    let b = db.bank(t.parse().unwrap()).unwrap();
                    for en in &b.entries {
                        let ev = se.events.get(en.event as usize);
                        println!("{:5} {:<12} {:<28} {:?} {:?}", en.event, ev.map(|e| se.category_name(e)).unwrap_or("?"), slot_of(en.event), en.conditions, ev.and_then(|e| e.files.first()));
                    }
                }
            }
        }
        "names" => {
            // names [filter]: event names from the CSV sources, checked against the packed events
            use ntw_formats::sound::{SoundEvents, names::{EventNames, CSV_DIR}};
            let vfs = Vfs::open_install(DATA).unwrap();
            let se = SoundEvents::read(&vfs.read(SoundEvents::PATH).unwrap()).unwrap();
            let mut csvs = Vec::new();
            for p in vfs.list(CSV_DIR) {
                if p.to_ascii_lowercase().ends_with(".csv") {
                    csvs.push((p.to_string(), String::from_utf8_lossy(&vfs.read(p).unwrap()).into_owned()));
                }
            }
            csvs.sort();
            {
                // debug: where does each CSV start to line up?
                use ntw_formats::sound::names::{parse_events_csv, normalize_sound_path};
                let mut i = 0usize;
                for (f, t) in &csvs {
                    let rows = parse_events_csv(t);
                    let mut bad = 0;
                    let mut first_bad = None;
                    for (k, r) in rows.iter().enumerate() {
                        let ok = se.events.get(i + k).is_some_and(|e| se.category_name(e).eq_ignore_ascii_case(&r.category) && e.files.len() == r.files.len() && e.files.iter().zip(&r.files).all(|(a, b)| normalize_sound_path(a) == normalize_sound_path(b)));
                        if !ok { bad += 1; if first_bad.is_none() { first_bad = Some(k); } }
                    }
                    println!("{f} start {i} rows {} bad {bad} first_bad {:?}", rows.len(), first_bad);
                    if let Some(k) = first_bad {
                        let r = &rows[k];
                        let e = &se.events[(i + k).min(se.events.len() - 1)];
                        println!("   csv {} {} {:?}\n   evt {} {:?} {:?}", r.name, r.category, r.files.first(), se.category_name(e), e.name, e.files.first());
                    }
                    i += rows.len();
                }
            }
            let n = EventNames::from_csvs(&se, &csvs);
            println!("csv files {} named {}/{} mismatched rows {}", csvs.len(), n.named_count(), se.events.len(), n.unmatched_rows);
            if let Some(f) = args.get(1) {
                for (i, nm) in n.names.iter().enumerate() {
                    if nm.as_deref().is_some_and(|x| x.to_ascii_lowercase().contains(&f.to_ascii_lowercase())) {
                        println!("{i:5} {} {}", se.category_name(&se.events[i]), nm.as_deref().unwrap());
                    }
                }
            }
        }
        "vocab" => {
            // vocab: which XML source each bank type was built from, and its condition names
            use ntw_formats::sound::SoundLibrary;
            let vfs = Vfs::open_install(DATA).unwrap();
            let lib = SoundLibrary::load(&vfs).unwrap();
            println!("named settings {}/154, named events {}/{}", lib.settings.len(), lib.names.named_count(), lib.events.events.len());
            for b in &lib.banks.banks {
                match lib.vocabulary.banks.get(&b.bank_type) {
                    Some(n) => {
                        let tags: Vec<String> = n.conditions.iter().map(|c| c.as_ref().map(|c| format!("{}({})", c.tag, c.values.len())).unwrap_or("?".into())).collect();
                        println!("bank {:2} n={:3} {:<45} {}", b.bank_type, b.entries.len(), n.source, tags.join(" "));
                    }
                    None => println!("bank {:2} n={:3} (no source)", b.bank_type, b.entries.len()),
                }
            }
            if let (Some(t), Some(src)) = (args.get(1), args.get(2)) {
                // vocab <type> <xml stem>: show how the XML entries line up with the packed ones
                use ntw_formats::sound::bank_xml::parse_bank_xml;
                let xml = parse_bank_xml(&ntw_formats::xml::decode_text(&vfs.read(&format!(r"sounds\banks\{src}.xml")).unwrap()));
                let bank = lib.banks.bank(t.parse().unwrap()).unwrap();
                let mut j = 0;
                for x in &xml {
                    let Some(b) = bank.entries.get(j) else { break };
                    let cands = lib.names.find(&x.event_name);
                    let vals: usize = b.conditions.iter().map(Vec::len).sum();
                    let ok = cands.contains(&(b.event as usize));
                    println!("{} xml {:<40} n={:2} cands {:?} | packed {:5} {:?} n={vals}", if ok { "OK " } else { "-- " }, x.event_name, x.conditions.len(), cands, b.event, lib.names.name(b.event as usize));
                    if ok || b.event == u32::MAX { j += 1; }
                }
                return;
            }
            if let Some(t) = args.get(1) {
                let n = &lib.vocabulary.banks[&t.parse::<u32>().unwrap()];
                for c in n.conditions.iter().flatten() {
                    println!("{}: {:?}", c.tag, c.values);
                }
            }
        }
        "params" => {
            // params: compare the 35 packed parameter values with the CSV columns of every named event
            use ntw_formats::sound::{SoundLibrary, names::{split_csv_line, CSV_DIR}};
            let vfs = Vfs::open_install(DATA).unwrap();
            let lib = SoundLibrary::load(&vfs).unwrap();
            let mut header: Vec<String> = Vec::new();
            let mut numeric_bad = vec![0usize; 35];
            let mut numeric_ok = vec![0usize; 35];
            let mut enums: BTreeMap<(usize, String), BTreeMap<u32, usize>> = BTreeMap::new();
            for p in vfs.list(CSV_DIR) {
                let text = String::from_utf8_lossy(&vfs.read(p).unwrap()).into_owned();
                let mut lines = text.lines();
                header = split_csv_line(lines.next().unwrap());
                for l in lines {
                    let f = split_csv_line(l);
                    if f[0].is_empty() || f.len() < 37 { continue; }
                    let Some(&ei) = lib.names.find(&f[0]).first() else { continue };
                    let ev = &lib.events.events[ei];
                    let pr = lib.events.params_of(ev).0;
                    for k in 0..35 {
                        let txt = &f[2 + k];
                        let v = pr[k];
                        match txt.parse::<f32>() {
                            Ok(x) => if (f32::from_bits(v) - x).abs() <= 1e-4 * x.abs().max(1.0) || (v as f32 - x).abs() < 1e-6 { numeric_ok[k] += 1 } else { numeric_bad[k] += 1 },
                            Err(_) => *enums.entry((k, txt.to_ascii_lowercase())).or_default().entry(v).or_default() += 1,
                        }
                    }
                }
            }
            for k in 0..35 {
                println!("{k:2} {:<40} ok {:5} bad {:5}", header.get(2 + k).map(String::as_str).unwrap_or("?"), numeric_ok[k], numeric_bad[k]);
            }
            for ((k, s), m) in &enums {
                let vals: Vec<String> = m.iter().map(|(v, n)| format!("{:#x}/{}({n})", v, f32::from_bits(*v))).collect();
                println!("enum col {k:2} {s:<20} -> {}", vals.join(" "));
            }
        }
        _ => eprintln!("unknown command"),
    }
}

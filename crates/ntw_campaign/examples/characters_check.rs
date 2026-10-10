//! Research helper for the characters and agents rules (slot 0-G,
//! `analysis/fidelity/CHARACTERS_FIDELITY.md`): what a start position or save holds per character
//! (type, age, traits with points and level, ancillaries) and overall counts (traits and ancillaries
//! per character, trait points against the level thresholds, ages by type). Read-only.
//!
//! ```text
//! cargo run -p ntw_campaign --release --example characters_check -- <data dir> <file|vfs:path>... [--faction key]
//! ```

use std::collections::BTreeMap;

use ntw_data::GameDatabase;
use ntw_formats::esf::EsfFile;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((data, rest)) = args.split_first() else {
        eprintln!("usage: characters_check <data dir> <file|vfs:path>... [--faction key]");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    let mut only: Option<String> = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--faction" {
            only = it.next().cloned();
        } else {
            files.push(a.clone());
        }
    }
    let db = GameDatabase::from_install(data).expect("game database");
    if std::env::var_os("ANC_FLAGS").is_some() {
        let mut combos: BTreeMap<(bool, bool, bool), Vec<String>> = BTreeMap::new();
        for a in db.campaign.characters.ancillaries.iter() {
            combos.entry((a.flag_3, a.world_unique, a.faction_unique)).or_default().push(a.key.clone());
        }
        for (k, v) in &combos {
            println!("flags {k:?}: {} e.g. {:?}", v.len(), &v[..v.len().min(8)]);
        }
    }
    let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
    for file in &files {
        let bytes = match file.strip_prefix("vfs:") {
            Some(p) => ntw_formats::campaign_map::GameFiles { vfs: &vfs }.read(p).expect("vfs file"),
            None => std::fs::read(file).expect("read"),
        };
        let esf = EsfFile::from_bytes(&bytes).expect("esf");
        let l = ntw_campaign::read_esf(&esf, &db).expect("load");
        if let Ok(fk) = std::env::var("FACTION_CHILDREN")
            && let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY")
        {
            {
                for item in &arr.items {
                    let Some(fr) = item.first().and_then(|n| n.as_record()) else { continue };
                    if fr.values().find_map(|n| n.as_str()) != Some(fk.as_str()) { continue; }
                    for (i, c) in fr.children.iter().enumerate() {
                        let d = match c { ntw_formats::esf::EsfNode::Record(r) => format!("REC {}", r.name), ntw_formats::esf::EsfNode::RecordArray(a) => format!("ARR {}[{}]", a.name, a.items.len()), other => format!("{other:?}").chars().take(60).collect() };
                        println!("  FACTION #{i}: {d}");
                    }
                }
            }
        }
        if std::env::var_os("PORTRAIT_PARENTS").is_some() {
            let mut parents: BTreeMap<String, usize> = BTreeMap::new();
            esf.root.walk(&mut |r: &ntw_formats::esf::EsfRecord| {
                let mut has = false;
                for c in &r.children {
                    if c.as_record().is_some_and(|x| x.name == "PORTRAIT_DETAILS") { has = true; }
                    if let Some(a) = c.as_record_array() { for it in &a.items { if it.iter().any(|n| n.as_record().is_some_and(|x| x.name == "PORTRAIT_DETAILS")) { *parents.entry(format!("{}[{}]", r.name, a.name)).or_default() += 1; } } }
                }
                if has { *parents.entry(r.name.clone()).or_default() += 1; }
            });
            println!("PORTRAIT_DETAILS parents: {parents:?}");
        }
        if std::env::var_os("CHARFIELDS").is_some() {
            let mut dist: BTreeMap<usize, BTreeMap<String, usize>> = BTreeMap::new();
            esf.root.walk(&mut |r: &ntw_formats::esf::EsfRecord| {
                if r.name == "CHARACTER" {
                    for (i, n) in r.children.iter().enumerate().filter(|(i, n)| *i != 2 && n.as_record().is_none() && n.as_record_array().is_none()) {
                        *dist.entry(i).or_default().entry(format!("{n:?}")).or_default() += 1;
                    }
                }
            });
            for (i, m) in &dist {
                let mut v: Vec<_> = m.iter().collect();
                v.sort_by(|a, b| b.1.cmp(a.1));
                println!("CHARACTER #{i}: {} values, top {:?}", v.len(), &v[..v.len().min(6)]);
            }
        }
        if let Ok(idx) = std::env::var("CHARTRUE") {
            let idx: usize = idx.parse().unwrap_or(0);
            esf.root.walk(&mut |r: &ntw_formats::esf::EsfRecord| {
                if r.name == "CHARACTER" && r.get(idx).and_then(|n| n.as_bool()) == Some(true) {
                    let d = r.child("CHARACTER_DETAILS");
                    let name = |i: usize| d.and_then(|d| d.get(i)).and_then(|n| n.as_record()).and_then(|x| x.get_str(0)).unwrap_or("").rsplit('_').next().unwrap_or("").to_string();
                    println!("  #{idx} true: {} {} {:?}", name(1), name(2), r.get(3));
                }
            });
        }
        if let Ok(who) = std::env::var("CHARREC") {
            esf.root.walk(&mut |r: &ntw_formats::esf::EsfRecord| {
                if r.name == "CHARACTER" {
                    let fore = r.child("CHARACTER_DETAILS").and_then(|d| d.get(1)).and_then(|n| n.as_record()).and_then(|x| x.get_str(0)).unwrap_or("").to_string();
                    if who.split(',').any(|w| fore.ends_with(w)) {
                        let vals: Vec<String> = r.children.iter().enumerate().filter(|(_, n)| n.as_record().is_none() && n.as_record_array().is_none()).map(|(i, n)| format!("#{i}={n:?}")).collect();
                        println!("  {fore}: {}", vals.join(" "));
                    }
                }
            });
        }
        if std::env::var_os("CD7").is_some() {
            let mut vals: BTreeMap<String, usize> = BTreeMap::new();
            esf.root.walk(&mut |r: &ntw_formats::esf::EsfRecord| {
                if r.name == "CHARACTER_DETAILS" {
                    let v7 = r.get(7).map(|n| format!("{n:?}")).unwrap_or_default();
                    let v15 = r.get(15).map(|n| format!("{n:?}")).unwrap_or_default();
                    let fore = r.get(1).and_then(|n| n.as_record()).and_then(|x| x.get_str(0)).unwrap_or("").rsplit('_').next().unwrap_or("").to_string();
                    if std::env::var("CD7").is_ok_and(|n| fore.contains(n.as_str()) && !n.is_empty()) {
                        println!("  {fore}: #7 {v7} #15 {v15}");
                    }
                    *vals.entry(format!("#7={v7} #15={v15}")).or_default() += 1;
                }
            });
            let mut v: Vec<_> = vals.into_iter().collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.1));
            println!("CHARACTER_DETAILS #7/#15: {:?}", &v[..v.len().min(15)]);
        }
        let m = &l.model;
        let year = m.calendar.date.year as i32;
        println!("== {file}: {:?}, turn {}", m.calendar.date, m.calendar.turn_number());
        let rules = &m.rules.effects;
        let mut n_traits: BTreeMap<usize, usize> = BTreeMap::new();
        let mut n_anc: BTreeMap<usize, usize> = BTreeMap::new();
        let mut ages: BTreeMap<String, Vec<i32>> = BTreeMap::new();
        let mut below_first = 0;
        let mut trait_total = 0;
        for (id, ch) in &m.world.characters {
            let Some(d) = m.world.character_details.get(id) else { continue };
            let fkey = m.world.factions.get(&ch.faction).map_or("?", |f| f.key.as_str());
            *n_traits.entry(d.traits.len()).or_default() += 1;
            *n_anc.entry(d.ancillaries.len()).or_default() += 1;
            let age = d.birth.map_or(-1, |b| year - b.year as i32);
            ages.entry(ch.kind.esf_name().to_string()).or_default().push(age);
            for t in &d.traits {
                trait_total += 1;
                if rules.trait_level_key(&t.key, t.points).is_none() {
                    below_first += 1;
                }
            }
            if only.as_deref().is_some_and(|k| k == fkey) {
                let traits: Vec<String> = d
                    .traits
                    .iter()
                    .map(|t| format!("{}:{}({})", t.key, t.points, rules.trait_level_key(&t.key, t.points).unwrap_or("-")))
                    .collect();
                println!(
                    "  {} {} | {:<10} age {:>3} post {} traits [{}] anc {:?}",
                    d.forename.rsplit('_').next().unwrap_or(""),
                    d.surname.rsplit('_').next().unwrap_or(""),
                    ch.kind.esf_name(),
                    age,
                    u8::from(d.post != 0),
                    traits.join(" "),
                    d.ancillaries
                );
            }
        }
        // Ancillaries held more than once (world-wide / within a faction), with the three bools #3..#5.
        let mut world: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (id, ch) in &m.world.characters {
            let fkey = m.world.factions.get(&ch.faction).map_or("?", |f| f.key.as_str());
            for a in m.world.character_details.get(id).map(|d| d.ancillaries.clone()).unwrap_or_default() {
                world.entry(a).or_default().push(fkey.to_string());
            }
        }
        for (a, fs) in world.iter().filter(|(_, v)| v.len() > 1) {
            let r = db.campaign.characters.ancillaries.get(a);
            let same_faction = fs.iter().any(|f| fs.iter().filter(|g| *g == f).count() > 1);
            println!("multi {a}: {} holders, same faction {same_faction}, flags {:?}", fs.len(), r.map(|r| (r.flag_3, r.world_unique, r.faction_unique)));
        }
        let mut d2: BTreeMap<String, usize> = BTreeMap::new();
        for d in m.world.character_details.values() {
            *d2.entry(format!("{:?}", d.date_2.map(|x| (x.year, x.month, x.half)))).or_default() += 1;
        }
        let mut d2v: Vec<_> = d2.into_iter().collect();
        d2v.sort_by_key(|x| std::cmp::Reverse(x.1));
        println!("date_2 values: {:?}", &d2v[..d2v.len().min(12)]);
        println!("traits per character: {n_traits:?}");
        println!("ancillaries per character: {n_anc:?}");
        println!("traits below their first level threshold: {below_first}/{trait_total}");
        for (k, v) in &ages {
            let known: Vec<i32> = v.iter().copied().filter(|a| *a >= 0).collect();
            let (lo, hi) = (known.iter().min().copied().unwrap_or(-1), known.iter().max().copied().unwrap_or(-1));
            println!("ages {k:<12} n {:>3} min {lo} max {hi}", v.len());
        }
    }
}

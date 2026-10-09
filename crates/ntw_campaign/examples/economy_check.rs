//! Read-only research helper for the campaign economy (CAMPAIGN_FIDELITY.md): prints, for each
//! faction of a save, the taxes the economy model computes next to the tax figures the original
//! stored in `FACTION_ECONOMICS` (`ECONOMICS_DATA` #1[0], one per turn of the 10-turn history).
//!
//! ```text
//! cargo run -p ntw_campaign --release --example economy_check -- <data dir> <file.save>...
//! ```
//! Files are only read.

use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_sim::campaign::economy;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("db") {
        // `db <data dir> <table> <codes> [filter]`: rows of a DB table (codes as in ntw_formats::db).
        let vfs = ntw_formats::pack::Vfs::open_install(&args[1]).expect("vfs");
        let schema = ntw_formats::db::Schema::from_codes(&args[3]).expect("codes");
        for path in vfs.list(&format!("db/{}_tables/", args[2])) {
            let t = ntw_formats::db::DbTable::read(&vfs.read(path).expect("read"), &schema).expect("decode");
            for r in &t.rows {
                let line = format!("{r:?}");
                if args.get(4).is_none_or(|f| line.contains(f.as_str())) {
                    println!("{line}");
                }
            }
        }
        return;
    }
    let Some((data, files)) = args.split_first() else {
        eprintln!("usage: economy_check <data dir> <file>...");
        std::process::exit(2);
    };
    let db = GameDatabase::from_install(data).expect("game database");
    if files.first().map(String::as_str) == Some("shipguns") {
        // Every ship type's full gun count (`SHIP_DAMAGE_INFO` #13) over the given files (`vfs:` start positions
        // or saves), against the naval unit keys of the DB.
        let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
        let gf = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) };
        let mut guns: std::collections::BTreeMap<String, std::collections::BTreeSet<u32>> = std::collections::BTreeMap::new();
        for file in &files[1..] {
            let bytes = match file.strip_prefix("vfs:") {
                Some(p) => gf.read(p).expect("vfs file"),
                None => std::fs::read(file).expect("read"),
            };
            let l = ntw_campaign::read_esf(&EsfFile::from_bytes(&bytes).expect("esf"), &db).expect("load");
            for f in l.model.world.forces.values().filter(|f| f.is_navy) {
                for u in &f.units {
                    if let Some(s) = l.model.world.ship_states.get(&u.id) {
                        guns.entry(u.unit_key.clone()).or_default().insert(s.max_guns);
                    }
                }
            }
        }
        let naval: Vec<&str> = db.units.iter().filter(|u| u.category.starts_with("naval")).map(|u| u.key.as_str()).collect();
        for k in &naval {
            println!("{k} {:?}", guns.get(*k));
        }
        println!("{} of {} naval types seen", naval.iter().filter(|k| guns.contains_key(**k)).count(), naval.len());
        return;
    }
    if files.first().map(String::as_str) == Some("naval") {
        // `unit_stats_naval` rows (raw), optionally only keys containing the second argument.
        for r in db.campaign.naval_stats.iter().filter(|r| files.get(1).is_none_or(|f| r.key.contains(f.as_str()))) {
            println!("{r:?}");
        }
        return;
    }
    if files.first().map(String::as_str) == Some("theatres") {
        // Each start position's map: the theatres of `regions.esf` `theatres_and_region_keys` and how many
        // region keys each lists, against the start position's regions.
        let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
        let gf = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) };
        for file in &files[1..] {
            let bytes = gf.read(file.trim_start_matches("vfs:")).expect("vfs file");
            let l = ntw_campaign::read_esf(&EsfFile::from_bytes(&bytes).expect("esf"), &db).expect("load");
            let rb = gf.read(&format!("campaign_maps/{}/regions.esf", l.info.map_key)).expect("regions.esf");
            let esf = EsfFile::from_bytes(&rb).expect("esf");
            let arr = esf.root.children.iter().find_map(|n| n.as_record_array().filter(|a| a.name == "theatres_and_region_keys"));
            let theatres: Vec<Vec<String>> = arr.map(|a| a.items.iter().map(|it| {
                let t = it.first().and_then(EsfNode::as_record);
                let keys = t.and_then(|t| t.children.iter().find_map(|n| n.as_record_array().filter(|a| a.name == "region_keys")));
                keys.map(|k| k.items.iter().filter_map(|x| x.first().and_then(EsfNode::as_str).map(str::to_string)).collect()).unwrap_or_default()
            }).collect()).unwrap_or_default();
            let game: Vec<&str> = l.model.world.regions.values().map(|r| r.key.as_str()).collect();
            let outside = game.iter().filter(|k| theatres.first().is_none_or(|t| !t.iter().any(|x| x == *k))).count();
            println!("{file} map {}: {} theatre(s), region keys {:?}; {} game regions, {} outside the first theatre", l.info.map_key, theatres.len(), theatres.iter().map(Vec::len).collect::<Vec<_>>(), game.len(), outside);
        }
        return;
    }
    if files.first().map(String::as_str) == Some("fleets") {
        // Start positions (`vfs:` paths or files): every pair of navies of factions at war, nearest first,
        // each with its nearest settlements, for planning a debugger session.
        let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
        for file in &files[1..] {
            let bytes = match file.strip_prefix("vfs:") {
                Some(p) => ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) }.read(p).expect("vfs file"),
                None => std::fs::read(file).expect("read"),
            };
            let esf = EsfFile::from_bytes(&bytes).expect("esf");
            let mut l = ntw_campaign::read_esf(&esf, &db).expect("load");
            {
                let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) };
                let map = ntw_formats::campaign_map::CampaignMap::load(&files, &l.info.map_key).expect("map");
                l.model.terrain = Some(ntw_sim::campaign::Terrain(std::sync::Arc::new(ntw_campaign::pathing::build_grid(&map))));
            }
            let m = &l.model;
            println!("== {file}");
            let pos = |f: &ntw_sim::campaign::MilitaryForce| f.commander.and_then(|c| m.world.characters.get(&c)).map(|c| (c.position.0.to_f64(), c.position.1.to_f64()));
            let near = |p: (f64, f64)| {
                let mut v: Vec<(f64, &str, &str)> = m.world.regions.values().map(|r| {
                    let (x, y) = (r.settlement.position.0.to_f64(), r.settlement.position.1.to_f64());
                    (((x - p.0).powi(2) + (y - p.1).powi(2)).sqrt(), r.key.as_str(), m.world.factions.get(&r.owner).map_or("?", |f| f.key.as_str()))
                }).collect();
                v.sort_by(|a, b| a.0.total_cmp(&b.0));
                v.iter().take(2).map(|(d, k, o)| format!("{k}({o}) {d:.0}")).collect::<Vec<_>>().join(", ")
            };
            let fkey = |id| m.world.factions.get(&id).map_or("?".to_string(), |f| f.key.clone());
            let navies: Vec<_> = m.world.forces.values().filter(|f| f.is_navy && !f.units.is_empty()).filter_map(|f| pos(f).map(|p| (f, p))).collect();
            let mut pairs = Vec::new();
            for (i, (a, pa)) in navies.iter().enumerate() {
                for (b, pb) in &navies[i + 1..] {
                    if m.at_war(a.faction, b.faction) && !fkey(a.faction).is_empty() && !fkey(b.faction).is_empty() {
                        pairs.push(((pa.0 - pb.0).hypot(pa.1 - pb.1), *a, *pa, *b, *pb));
                    }
                }
            }
            pairs.sort_by(|x, y| x.0.total_cmp(&y.0));
            let show = |f: &ntw_sim::campaign::MilitaryForce, p: (f64, f64)| {
                let cmd = f.commander.and_then(|c| m.world.characters.get(&c)).map_or(0, |c| c.id.0);
                format!("{} navy {} (admiral char {cmd}) at ({:.0},{:.0}) near {} [{}]", fkey(f.faction), f.id.0, p.0, p.1, near(p), f.units.iter().map(|u| u.unit_key.as_str()).collect::<Vec<_>>().join(" "))
            };
            for (d, a, pa, b, pb) in pairs.iter().take(12) {
                println!("  {d:.0}: {}\n        vs {}", show(a, *pa), show(b, *pb));
                for (x, y) in [(a, b), (b, a)] {
                    if let (Some(c), Some(t)) = (x.commander, y.commander.and_then(|c| m.world.characters.get(&c))) {
                        let ap = m.world.characters.get(&c).map_or(0, |c| c.movement_points);
                        match m.plan_path(c, t.position) {
                            Some(p) => println!("        {} -> {}: path cost {:.0}, action points {ap}, reach {}/{}", fkey(x.faction), fkey(y.faction), p.path.costs.last().copied().unwrap_or(0.0), p.reachable, p.path.points.len().saturating_sub(1)),
                            None => println!("        {} -> {}: no path", fkey(x.faction), fkey(y.faction)),
                        }
                    }
                }
            }
        }
        return;
    }
    if files.first().map(String::as_str) == Some("techs") {
        for t in db.technologies.iter() {
            println!("{} bl {} col {} cost {} u2c {} u30 {} u34 {} u38 {} b {} {} u4c {}", t.key, t.building_level, t.tree_column, t.research_cost, t.unknown_2c, t.unknown_30, t.unknown_34, t.unknown_38, t.unknown_3c, t.unknown_3d, t.unknown_4c);
        }
        return;
    }
    if files.first().map(String::as_str) == Some("unitflags") {
        let cat = files.get(1).map_or("infantry", String::as_str);
        for u in db.units.iter().filter(|u| u.category.starts_with(cat)) {
            println!("{} {} {} {} {} {} {} {} {} {:?}", u.key, u.unit_class, u.unknown_94, u.unknown_95, u.unknown_96, u.unknown_98, u.unknown_9c, u.unknown_a0, u.unknown_b0, u.ai_role);
        }
        return;
    }
    if files.first().map(String::as_str) == Some("units6") {
        for u in &db.units {
            println!("{} {} {}", u.key, u.unknown_38, u.category);
        }
        return;
    }
    if files.first().map(String::as_str) == Some("factions") {
        for k in &files[1..] {
            if let Some(f) = db.faction(k) {
                println!("{k} {} {} {} {}", f.category, f.unknown_64, f.unknown_65, f.unknown_66);
            }
        }
        return;
    }
    for file in files {
        let mut l = ntw_campaign::read_file(file, &db).expect("load");
        if std::env::var("ECON_MAP").is_ok() {
            let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
            let files = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) };
            let map = ntw_formats::campaign_map::CampaignMap::load(&files, &l.info.map_key).expect("map");
            l.model.terrain = Some(ntw_sim::campaign::Terrain(std::sync::Arc::new(ntw_campaign::pathing::build_grid(&map))));
            ntw_campaign::trade::attach_map(&mut l.model, &map.regions);
        }
        let esf = EsfFile::open(file).expect("open");
        println!("== {file}");
        let m = &l.model;
        if let Some(n) = std::env::var("ECON_TURNS").ok().and_then(|s| s.parse().ok()) {
            turns_detail(l.model.clone(), n);
            continue;
        }
        if std::env::var("ECON_RECRUITCOST").is_ok() {
            recruit_cost_detail(m, &db);
            continue;
        }
        if let Ok(next) = std::env::var("ECON_DIPLO") {
            // Attitude factors: the next file's values against this file's after k drift steps (k = turns between).
            let n = ntw_campaign::read_file(&next, &db).expect("load next");
            let k = n.model.calendar.turns_elapsed.saturating_sub(m.calendar.turns_elapsed);
            println!("  relationships {} / {} factors {:?}", m.world.relationships.len(), n.model.world.relationships.len(), m.world.relationships.values().next().map(|r| r.attitudes.len()));
            let (mut same, mut stepped, mut other, mut changed_stance, mut dead) = (0, 0, 0, 0, 0);
            let mut shown = 0;
            for ((a, b), r) in &m.world.relationships {
                let id2 = |x: &ntw_sim::campaign::FactionId| m.world.factions.get(x).and_then(|f| n.model.world.factions.values().find(|g| g.key == f.key)).map(|g| g.id);
                let (Some(a2), Some(b2)) = (id2(a), id2(b)) else { continue };
                let Some(r2) = n.model.world.relationships.get(&(a2, b2)) else { continue };
                if m.world.stance(*a, *b) != n.model.world.stance(a2, b2) {
                    changed_stance += 1;
                    continue;
                }
                let alive = |f: &ntw_sim::campaign::FactionId| m.world.regions.values().any(|x| x.owner == *f) || m.world.forces.values().any(|x| x.faction == *f);
                let alive = alive(a) && alive(b);
                if !alive {
                    dead += 1;
                    continue;
                }
                for (i, (f, f2)) in r.attitudes.iter().zip(&r2.attitudes).enumerate() {
                    if [15usize, 16, 21, 22].contains(&i) {
                        continue;
                    }
                    let mut g = *f;
                    for _ in 0..k {
                        g.step();
                    }
                    if g.value == f2.value && g.drift == f2.drift {
                        if f.drift == 0 { same += 1 } else { stepped += 1 }
                    } else {
                        other += 1;
                        if shown < 15 {
                            shown += 1;
                            let key = |x: &ntw_sim::campaign::FactionId| m.world.factions.get(x).map_or("?".to_string(), |f| f.key.clone());
                            println!("  {} -> {} {}: {:?} -> expected {} got {:?}", key(a), key(b), ntw_sim::campaign::details::ATTITUDE_FACTORS[i], (f.value, f.drift, f.limit, f.limited), g.value, (f2.value, f2.drift, f2.limit, f2.limited));
                        }
                    }
                }
            }
            println!("  turns {} -> {} (k {k}): unchanged {same}, drift steps matched {stepped}, different {other}, pairs whose stance changed {changed_stance}, dead targets {dead}", m.calendar.turns_elapsed, n.model.calendar.turns_elapsed);
            continue;
        }
        if let Ok(b) = std::env::var("ECON_BONUS") {
            let fx = ntw_sim::campaign::effects::Effects::compute(m);
            for f in m.world.factions.values().filter(|f| f.key == "france") {
                println!("  faction {}", fx.faction(f.id, &b));
                let d = &m.world.faction_details[&f.id];
                let saved = if d.bonus_with_difficulty.is_empty() { &d.bonus_base } else { &d.bonus_with_difficulty };
                for s in saved.iter().filter(|s| ntw_sim::campaign::effects::BONUS_NAMES.get(s.bonus as usize) == Some(&b.as_str())) { println!("  saved {s:?}"); }
                for c in m.world.characters.values().filter(|c| c.faction == f.id) {
                    let v = ntw_sim::campaign::effects::Effects::character_effects(m, c.id).get(&b);
                    if v != 0.0 { println!("  char {:?} {:?} {v}", c.id, c.kind); }
                }
                println!("  leader {:?}", d.leader());
            }
            continue;
        }
        if std::env::var("ECON_POSTS").is_ok() {
            for key in ["france", "spain", "prussia", "russia", "austria", "britain"] {
                let Some(f) = m.world.factions.values().find(|f| f.key == key) else { continue };
                let d = &m.world.faction_details[&f.id];
                for p in d.posts.iter().filter(|p| p.governorship.is_none() && (p.key == "faction_leader" || p.key == "head_of_government")) {
                    let Some(h) = p.holder else { continue };
                    let c = &m.world.characters[&h];
                    let attrs = m.world.character_details.get(&h).map(|x| x.attributes.clone()).unwrap_or_default();
                    let main = ntw_sim::campaign::effects::Effects::character_attribute(m, h, ntw_sim::campaign::agents::main_attribute(c.kind));
                    println!("  {key} {} holder {:?} {:?} gov {} main {main:?} attrs {attrs:?}", p.key, h, c.kind, f.government_key);
                }
            }
            continue;
        }
        if std::env::var("ECON_UCOLS").is_ok() {
            // 0-B round 14 item 3, continued: the distinct values of the `units` int columns that
            // could be the enum `unit + 0x20` is compared against, and which unit rows carry the
            // four codes (4, 0xB, 0xC, 0xE).
            for (name, vals) in [
                ("units #6 @0x38 (recruitment turns)", db.units.iter().map(|u| u.unknown_38).collect::<Vec<i32>>()),
                ("units #9 @0x44", db.units.iter().map(|u| u.unknown_44).collect::<Vec<i32>>()),
                ("units #20 @0x98", db.units.iter().map(|u| u.unknown_98).collect::<Vec<i32>>()),
                ("units #22 @0xA0", db.units.iter().map(|u| u.unknown_a0).collect::<Vec<i32>>()),
            ] {
                let set: std::collections::BTreeSet<i32> = vals.into_iter().collect();
                println!("{name}: {} distinct {:?}", set.len(), set);
            }
            for (label, pick) in [("#6", 0usize), ("#20", 1), ("#22", 2)] {
                let rows: Vec<&str> = db
                    .units
                    .iter()
                    .filter(|u| {
                        let v = match pick {
                            0 => u.unknown_38,
                            1 => u.unknown_98,
                            _ => u.unknown_a0,
                        };
                        [4, 0xB, 0xC, 0xE].contains(&v)
                    })
                    .map(|u| u.key.as_str())
                    .collect();
                println!("units {label} in {{4, 0xB, 0xC, 0xE}}: {} rows {:?}", rows.len(), &rows[..rows.len().min(12)]);
            }
            // And the four codes read as indices into the shipped class tables.
            let cls = ntw_formats::group_formation::UNIT_CLASSES;
            for c in [4u32, 0xB, 0xC, 0xE] {
                let land: Vec<&str> = db
                    .units
                    .iter()
                    .filter(|u| ntw_formats::group_formation::class_id(&u.unit_class) == c)
                    .map(|u| u.key.as_str())
                    .take(4)
                    .collect();
                println!("class id {c} (0x{c:02x}) = {:22} units {:?}", cls[c as usize], land);
            }
            continue;
        }
        if std::env::var("ECON_DESERT").is_ok() {
            // 0-B round 14 item 3: the four unit classes `bankrupt_desertion` skips.  The codes the
            // exe compares `unit + 0x20` against (4, 0xB, 0xC, 0xE) are ids in the alphabetical
            // class list `0x00EED3E0` (`ntw_formats::group_formation::UNIT_CLASSES`; CONFIRMED by
            // round 11's 0x18 / 0x26 = bomb ketch / rocket ship).  This lists the class ids that
            // actually occur in the units of forces OUTSIDE a settlement - the ones the skip can
            // change - over the vanilla saves and the shipped start positions.
            let garrisons: Vec<_> = m.world.regions.values().filter_map(|r| r.garrison).collect();
            let mut outside: std::collections::BTreeMap<u32, (String, usize)> = Default::default();
            let mut inside: std::collections::BTreeMap<u32, (String, usize)> = Default::default();
            for f in m.world.forces.values() {
                if f.units.is_empty() {
                    continue;
                }
                for u in &f.units {
                    let class = db.unit(&u.unit_key).map(|r| r.unit_class.as_str()).unwrap_or("");
                    let id = ntw_formats::group_formation::class_id(class);
                    let name = format!("{} ({class})", ntw_formats::group_formation::UNIT_CLASSES[id as usize]);
                    let map = if garrisons.contains(&f.id) { &mut inside } else { &mut outside };
                    map.entry(id).or_insert((name, 0)).1 += 1;
                }
            }
            println!("== {}: {} units outside a settlement in {} classes", m.rules.campaign.as_str(), outside.values().map(|(_, n)| n).sum::<usize>(), outside.len());
            for (id, (name, n)) in &outside {
                let mark = if [4u32, 0xB, 0xC, 0xE].contains(id) { "  <== one of the four codes" } else { "" };
                println!("   id {id:3} (0x{id:02x}) {name:24} {n} units{mark}");
            }
            for (id, (name, n)) in &inside {
                println!("   garrison id {id:3} {name:24} {n} units");
            }
            continue;
        }
        if std::env::var("ECON_GOVPAIR").is_ok() {
            // 0-B round 14 item 2: is `diplomatic_relations_government_type` keyed on the PAIR
            // (own government, target government) or on one government alone?  For every ordered
            // pair in the save, the row's #2 / #3 next to the stored `government_type` factor, and
            // the same read with the pair collapsed to each single column.
            let gov = |f: &ntw_sim::campaign::FactionId| m.world.factions.get(f).map_or(String::new(), |x| x.government_key.clone());
            let alive = |f: &ntw_sim::campaign::FactionId| m.world.regions.values().any(|x| x.owner == *f);
            let mut seen: std::collections::BTreeSet<(String, String)> = Default::default();
            let mut rows: std::collections::BTreeMap<(String, String), (i32, i32, i32, usize)> = Default::default();
            for ((a, b), r) in &m.world.relationships {
                if !alive(a) || !alive(b) {
                    continue;
                }
                let (ga, gb) = (gov(a), gov(b));
                let entry = rows.entry((ga.clone(), gb.clone())).or_insert((0, 0, 0, 0));
                entry.0 = r.attitudes[16].value;
                entry.3 += 1;
                if let Some(x) = m.rules.government_relations.get(&(ga.clone(), gb.clone())) {
                    entry.1 = x.0;
                    entry.2 = x.1;
                }
                seen.insert((ga, gb));
            }
            println!("== the {} shipped rows (own gov, target gov, #2 value, #3 limit)", m.rules.government_relations.len());
            for ((a, b), (v2, v3)) in &m.rules.government_relations {
                println!("   {a:26} {b:26} #2 {v2:5} #3 {v3:5}");
            }
            println!("== the ordered pairs the save holds, with the stored factor");
            for (ga, gb) in &seen {
                let (stored, v2, v3, n) = rows[&(ga.clone(), gb.clone())];
                println!("   {ga:26} {gb:26} stored {stored:5} #2 {v2:5} #3 {v3:5} n {n} {}", if stored == v3 { "= #3" } else { "!= #3" });
            }
            // The decisive test: collapse the pair to one column and see whether the stored factors
            // still match.  `own` = the government of the record's owner, `tgt` = its target's.
            let mut ok_pair = 0;
            let mut n = 0;
            for ((a, b), r) in &m.world.relationships {
                if !alive(a) || !alive(b) {
                    continue;
                }
                let stored = r.attitudes[16].value;
                ok_pair += usize::from(Some(stored) == m.rules.government_relations.get(&(gov(a), gov(b))).map(|x| x.1));
                n += 1;
            }
            println!("== {n} ordered pairs in the save");
            println!("   pair-keyed (#3 of (own, target))                {ok_pair}/{n}");
            // A single-column model must give ONE value per government.  Count the distinct stored
            // values per column: more than one means the stored factor is not f(that government).
            let mut by_own: std::collections::BTreeMap<String, std::collections::BTreeSet<i32>> = Default::default();
            let mut by_tgt: std::collections::BTreeMap<String, std::collections::BTreeSet<i32>> = Default::default();
            for ((a, b), r) in &m.world.relationships {
                if !alive(a) || !alive(b) {
                    continue;
                }
                by_own.entry(gov(a)).or_default().insert(r.attitudes[16].value);
                by_tgt.entry(gov(b)).or_default().insert(r.attitudes[16].value);
            }
            for (label, map) in [("owner", &by_own), ("target", &by_tgt)] {
                for (g, set) in map {
                    println!("   f({label} = {g:26}) would be one value, the save holds {}: {set:?}", set.len());
                }
            }
            // And the same over the shipped rows themselves: #2 and #3 as functions of each column.
            for (label, idx) in [("own", 0), ("target", 1)] {
                let mut col: std::collections::BTreeMap<String, (std::collections::BTreeSet<i32>, std::collections::BTreeSet<i32>)> = Default::default();
                for ((ga, gb), (v2, v3)) in &m.rules.government_relations {
                    let e = col.entry(if idx == 0 { ga.clone() } else { gb.clone() }).or_default();
                    e.0.insert(*v2);
                    e.1.insert(*v3);
                }
                for (g, (v2, v3)) in &col {
                    println!("   table f({label} = {g:26}): #2 takes {} value(s) {v2:?}, #3 takes {} {v3:?}", v2.len(), v3.len());
                }
            }
            continue;
        }
        if std::env::var("ECON_CFACT").is_ok() {
            // The computed attitude factors against predictions (religion 15, government 16, leader 21, enlightenment 22).
            let key = |f: &ntw_sim::campaign::FactionId| m.world.factions.get(f).map_or("?".to_string(), |x| x.key.clone());
            let fx = ntw_sim::campaign::effects::Effects::compute(m);
            let (mut ok, mut bad) = ([0; 4], [0; 4]);
            let mut shown = 0;
            for ((a, b), r) in &m.world.relationships {
                let rel = |f: &ntw_sim::campaign::FactionId| m.world.faction_details.get(f).map(|d| d.religion.clone()).unwrap_or_default();
                let gov = |f: &ntw_sim::campaign::FactionId| m.world.factions.get(f).map(|x| x.government_key.clone()).unwrap_or_default();
                let alive = |f: &ntw_sim::campaign::FactionId| m.world.regions.values().any(|x| x.owner == *f);
                if !alive(a) || !alive(b) { continue; }
                let preds = [
                    (15usize, m.rules.religion_attitudes.get(&(rel(a), rel(b))).copied()),
                    (16, m.rules.government_relations.get(&(gov(a), gov(b))).map(|x| x.1)),
                    (21, Some(fx.faction(*b, "diplomacy_bonus_faction_leader") as i32)),
                    (22, Some(fx.faction(*b, "diplomacy_bonus_enlightenment") as i32)),
                ];
                for (k, (i, p)) in preds.iter().enumerate() {
                    let f = r.attitudes[*i];
                    let good = Some(f.value) == *p;
                    if good { ok[k] += 1 } else {
                        bad[k] += 1;
                        if shown < 20 { shown += 1; println!("  {} -> {} [{i}] stored {:?} predicted {:?} rel {}/{} gov {}/{}", key(a), key(b), (f.value, f.drift, f.limit, f.limited), p, rel(a), rel(b), gov(a), gov(b)); }
                    }
                }
            }
            println!("  ok {ok:?} bad {bad:?} (religion, government limit, leader, enlightenment)");
            continue;
        }
        if std::env::var("ECON_NAVAL").is_ok() {
            // The two largest navies of two factions, autoresolved with the model (seed varied).
            let navies: Vec<_> = m.world.forces.values().filter(|f| f.is_navy && f.units.len() >= 2).map(|f| (f.id, f.faction, f.units.len())).collect();
            let Some(&(na, fa, _)) = navies.iter().max_by_key(|x| x.2) else { continue };
            let Some(&(nb, _, _)) = navies.iter().filter(|x| x.1 != fa).max_by_key(|x| x.2) else { continue };
            let show = |m: &ntw_sim::campaign::CampaignModel, f: ntw_sim::campaign::ForceId| m.world.forces.get(&f).map(|f| f.units.iter().map(|u| format!("{}:{}", u.unit_key, u.men)).collect::<Vec<_>>().join(" ")).unwrap_or("gone".into());
            println!("  A {}
  B {}", show(m, na), show(m, nb));
            for seed in [1u32, 2, 3] {
                let mut x = m.clone();
                x.rng = ntw_sim::rng::CaRng::new(seed);
                x.pending_battle = Some(ntw_sim::campaign::PendingBattle { attacker: na, defenders: vec![nb], settlement: None, resume: None });
                let _ = x.autoresolve();
                println!("  seed {seed}: A {}
           B {}", show(&x, na), show(&x, nb));
            }
            continue;
        }
        if std::env::var("ECON_SHIPS").is_ok() {
            // NAVAL_UNIT: key, UNIT men fields, SHIP_DAMAGE_INFO.
            let Some(fa) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { continue };
            let mut shown = 0;
            for f in fa.records() {
                let Some(armies) = f.record_array("ARMY_ARRAY") else { continue };
                for a in armies.records() {
                    let Some(units) = a.record_array("UNITS_ARRAY") else { continue };
                    for u in units.records().filter(|u| u.name == "NAVAL_UNIT") {
                        if shown >= 40 { break; }
                        shown += 1;
                        let key = u.child("NAVAL_RECORD_KEY").and_then(|k| k.get_str(0)).unwrap_or("?");
                        let unit = u.child("UNIT");
                        let ints: Vec<String> = unit.map(|x| (4..13).map(|i| format!("{:?}", x.get(i))).collect()).unwrap_or_default();
                        let dmg: Vec<String> = u.child("SHIP_DAMAGE_INFO").map(|d| d.children.iter().map(|c| format!("{c:?}")).collect()).unwrap_or_default();
                        println!("  {key} unit {} | {:?} {:?} {:?} | dmg {}", ints.join(" "), u.get(2), u.get(3), u.get(4), dmg.join(" "));
                    }
                }
            }
            continue;
        }
        if std::env::var("ECON_FORTS").is_ok() {
            let mut n = std::collections::BTreeMap::new();
            for r in m.world.regions.values() {
                *n.entry(r.fortification.as_ref().map_or("-".to_string(), |b| format!("{} {}", b.level_key, b.health))).or_insert(0) += 1;
            }
            println!("  {n:?}");
            continue;
        }
        if std::env::var("ECON_SAVED7").is_ok() {
            for (f, d) in &m.world.faction_details {
                let key = m.world.factions.get(f).map_or("?".to_string(), |x| x.key.clone());
                let saved = if d.bonus_with_difficulty.is_empty() { &d.bonus_base } else { &d.bonus_with_difficulty };
                for s in saved.iter().filter(|s| s.kind == 7 || s.qualifier.contains("align") || s.qualifier.contains("rel_")) {
                    println!("  {key} kind {} bonus {} value {} qualifier {}", s.kind, s.bonus, s.value, s.qualifier);
                }
            }
            continue;
        }
        if let Ok(keys) = std::env::var("ECON_RELSRC") {
            // Religion sources of the named regions: every character within 60 units (distance, in region or not,
            // type, faction, rank), the conversion entries of the faction sum and of the region's own buildings.
            let fx = ntw_sim::campaign::effects::Effects::compute(m);
            let vfs = ntw_formats::pack::Vfs::open_install(std::path::Path::new(data)).expect("vfs");
            let gf = ntw_formats::campaign_map::GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(data)) };
            let map = ntw_formats::campaign_map::CampaignMap::load(&gf, &l.info.map_key).expect("map");
            let rm = &map.regions;
            // The regions.esf region whose area outlines hold a point (even-odd over all its loops).
            let esf_region = |x: f32, z: f32| -> Vec<String> {
                rm.regions.iter().filter(|r| !r.is_sea && r.areas.iter().any(|a| {
                    let mut inside = false;
                    for o in &a.outlines {
                        let pts: Vec<(f32, f32)> = o.iter().map(|&i| rm.vertices[i as usize]).collect();
                        if ntw_formats::campaign_map::point_in_loop(&pts, x, z) { inside = !inside; }
                    }
                    inside
                })).map(|r| r.key.clone()).collect()
            };
            for k in keys.split(',') {
                let Some(r) = m.world.regions.values().find(|r| r.key == k) else { continue };
                let owner = m.world.factions.get(&r.owner).map_or("?", |f| f.key.as_str());
                println!("{k} owner {owner} shares {:?}", r.religions);
                let (sx, sy) = (r.settlement.position.0.to_f64(), r.settlement.position.1.to_f64());
                for c in m.world.characters.values() {
                    let (x, y) = (c.position.0.to_f64(), c.position.1.to_f64());
                    let d = (x - sx).hypot(y - sy);
                    if d < 60.0 {
                        let fac = m.world.factions.get(&c.faction).map_or("?", |f| f.key.as_str());
                        println!("  char {} {:?} {fac} dist {d:.0} in_region {} esf {:?} rank {} garrison {:?}", c.id.0, c.kind, economy::in_region(m, r, c), esf_region(x as f32, y as f32), ntw_sim::campaign::agents::rank(m, c.id), c.garrisoned_in);
                        if format!("{:?}", c.kind).contains("Missionary")
                            && let Some(det) = m.world.character_details.get(&c.id)
                        {
                            println!("    attrs {:?} bonuses {:?} traits {:?} anc {:?}", det.attributes, det.attribute_bonuses, det.traits.iter().map(|t| format!("{}:{}", t.key, t.points)).collect::<Vec<_>>(), det.ancillaries);
                        }
                    }
                }
                let _ = &fx;
                let fsum = ntw_sim::campaign::effects::Effects::faction_sum(m, r.owner);
                for (key, v) in &fsum.values {
                    if key.bonus == "conversion" {
                        println!("  faction sum {key:?} {v}");
                    }
                }
                for b in r.effect_buildings() {
                    if let Some(s) = m.rules.effects.building_local().get(&b.level_key) {
                        for (key, v) in &s.values {
                            if key.bonus == "conversion" {
                                println!("  building {} {key:?} {v}", b.level_key);
                            }
                        }
                    }
                }
                println!("  strengths {:?}", m.religion_strengths(r.id));
            }
            continue;
        }
        if let Ok(next) = std::env::var("ECON_RELIGION") {
            // Religion shares: this save after k conversion steps against the next save (k = turns between).
            let n = ntw_campaign::read_file(&next, &db).expect("load next");
            let k = n.model.calendar.turns_elapsed.saturating_sub(m.calendar.turns_elapsed);
            let mut sim = m.clone();
            let use_next_pop = std::env::var("ECON_RELIGION_POP").is_ok();
            // ECON_RELIGION_CHARS: the characters (positions, ranks) of the next save, i.e. where the missionaries stood
            // at the round end, when the conversion runs.
            if std::env::var("ECON_RELIGION_CHARS").is_ok() {
                sim.world.characters = n.model.world.characters.clone();
                sim.world.character_details = n.model.world.character_details.clone();
            }
            for _ in 0..k {
                let ids: Vec<_> = sim.world.regions.keys().copied().collect();
                for id in ids {
                    if use_next_pop {
                        let key = sim.world.regions[&id].key.clone();
                        if let Some(r2) = n.model.world.regions.values().find(|r| r.key == key) {
                            sim.world.regions.get_mut(&id).unwrap().population = r2.population;
                        }
                    }
                    sim.convert_region(id);
                }
            }
            let (mut same, mut close, mut far, mut changed) = (0, 0, 0, 0);
            for r in sim.world.regions.values() {
                let Some(r2) = n.model.world.regions.values().find(|x| x.key == r.key) else { continue };
                let r0 = &m.world.regions[&r.id];
                for ((rel, s), (_, s0)) in r.religions.iter().zip(&r0.religions) {
                    let Some((_, s2)) = r2.religions.iter().find(|(k2, _)| k2 == rel) else { continue };
                    if (s2 - s0).abs() > 1e-7 { changed += 1; }
                    let d = (s - s2).abs();
                    if d < 1e-6 { same += 1 } else if d < 1e-3 { close += 1 } else {
                        far += 1;
                        if far <= 12 {
                            println!("  {} {rel}: start {s0} model {s} next {s2} pop {} -> {} strengths {:?} owner {:?} -> {:?} buildings {:?}", r.key, r0.population, r2.population, m.religion_strengths(r.id), m.world.factions.get(&r0.owner).map(|f| f.key.clone()), n.model.world.factions.get(&r2.owner).map(|f| f.key.clone()), r0.buildings().map(|b| b.level_key.clone()).collect::<Vec<_>>());
                        }
                    }
                }
            }
            println!("  k {k}: shares equal {same}, within 1e-3 {close}, further {far}; shares the original changed {changed}");
            continue;
        }
        if std::env::var("ECON_DAMAGED").is_ok() {
            // Buildings below full health, and repairs under way.
            for r in m.world.regions.values() {
                for (i, s) in r.slots.iter().enumerate() {
                    if let Some(b) = s.building.as_ref().filter(|b| b.health < 100) {
                        let repair = r.construction.iter().any(|c| c.slot == ntw_sim::campaign::SlotRef::Slot(i));
                        println!("  {} {} {} health {} repairing {repair}", r.key, s.key, b.level_key, b.health);
                    }
                }
            }
            continue;
        }
        if std::env::var("ECON_REBELS").is_ok() {
            // REGION #24 (rebel faction key) of each region, and whether it names a campaign faction.
            for (r, k) in &m.world.region_rebel_factions {
                let f = m.world.factions.values().find(|f| &f.key == k);
                let regions = f.map_or(0, |f| m.world.regions.values().filter(|x| x.owner == f.id).count());
                println!("  {} owner {} rebel {k} faction {} regions {regions} lib {:?}", m.world.regions[r].key, m.world.factions.get(&m.world.regions[r].owner).map_or("?", |f| f.key.as_str()), f.is_some(), m.liberation_target(*r, m.world.regions[r].owner));
            }
            continue;
        }
        if std::env::var("ECON_AP").is_ok() {
            ap_detail(m);
            continue;
        }
        if let Ok(h) = std::env::var("ECON_FX") {
            fx_detail(l, &h);
            continue;
        }
        if std::env::var("ECON_GENT").is_ok() {
            for c in m.world.characters.values().filter(|c| c.kind.esf_name() == "gentleman").take(5) {
                println!("{:?} {:?}", c.id, m.world.character_details.get(&c.id).map(|d| d.attributes.clone()));
            }
            continue;
        }
        if std::env::var("ECON_FXGDP").is_ok() {
            fx_gdp_detail(m);
            continue;
        }
        if std::env::var("ECON_PRICES").is_ok() {
            prices_detail(m, &esf);
            continue;
        }
        if std::env::var("ECON_CAPS").is_ok() {
            caps_detail(m);
            continue;
        }
        if std::env::var("ECON_FXTIME").is_ok() {
            fx_time(m);
            continue;
        }
        if std::env::var("ECON_PO").is_ok() {
            po_detail(m, &esf);
            continue;
        }
        if let Ok(k) = std::env::var("ECON_BUILD") {
            if let Some(r) = m.world.regions.values().find(|r| r.key == k) {
                let owner = m.world.factions.get(&r.owner).map_or("?", |f| f.key.as_str());
                println!("{k} owner {owner}");
                for (i, s) in r.slots.iter().enumerate() {
                    if let Some(b) = &s.building {
                        for u in m.rules.buildings.get(&b.level_key).map(|x| x.upgrades_to.clone()).unwrap_or_default() {
                            println!("  CAN slot {i} {} -> {u}: {:?}", b.level_key, m.can_build(r.id, ntw_sim::campaign::SlotRef::Slot(i), &u));
                        }
                    }
                }
                for s in r.slots.iter().filter(|s| s.holder.is_some_and(|h| h != r.owner)) {
                    let h = s.holder.unwrap();
                    let hf = m.world.factions.get(&h);
                    println!("  slot {} held by {:?} gov {:?} tax {:?}/{:?} regions {}", s.key, hf.map(|x| x.key.clone()), hf.map(|x| x.government_key.clone()), hf.map(|x| x.tax_lower.clone()), hf.map(|x| x.tax_upper.clone()), m.world.regions.values().filter(|x| x.owner == h).count());
                    for (key, v) in ntw_sim::campaign::effects::Effects::faction_sum(m, h).values.iter().filter(|(key, _)| key.bonus.contains("happiness")) {
                        println!("    holder {key:?} {v}");
                    }
                }
                let fx = ntw_sim::campaign::effects::Effects::compute(m);
                for p in m.world.faction_details.get(&r.owner).map(|d| d.posts.clone()).unwrap_or_default() {
                    let hp: Vec<String> = p.holder.and_then(|h| fx.character.get(&h)).map(|s| s.values.iter().filter(|(k, _)| k.bonus.contains("happiness")).map(|(k, v)| format!("{}/{}={v}", k.bonus, k.qualifier)).collect()).unwrap_or_default();
                    println!("  post {} gov {:?} holder {:?} {:?}", p.key, p.governorship.as_ref().map(|g| (g.theatre_id, g.regions.contains(&r.id), g.regions.len())), p.holder, hp);
                }
                for (fid, d) in &m.world.faction_details {
                    for p in d.posts.iter().filter(|p| p.governorship.as_ref().is_some_and(|g| g.theatre_id == 721178928 || g.regions.contains(&r.id))) {
                        let g = p.governorship.as_ref().unwrap();
                        println!("  GOV of {:?}: {} theatre {} regions {:?} faction {:?}", m.world.factions.get(fid).map(|x| x.key.clone()), p.key, g.theatre_id, g.regions, g.faction);
                    }
                }
                let of = m.world.factions.get(&r.owner);
                println!("  owner tax {:?}/{:?}", of.map(|x| x.tax_lower.clone()), of.map(|x| x.tax_upper.clone()));
                let fs = ntw_sim::campaign::effects::Effects::faction_sum(m, r.owner);
                for (key, v) in fs.values.iter().filter(|(key, _)| key.bonus.contains("happiness")) {
                    println!("  faction {key:?} {v}");
                }
                for s in &r.slots {
                    if let Some(b) = &s.building {
                        let set = m.rules.effects.building_local().get(&b.level_key);
                        println!("  {} {} health {} local {:?}", s.key, b.level_key, b.health, set);
                    }
                }
            }
            continue;
        }
        if std::env::var("ECON_BUILDCOST").is_ok() {
            for f in m.world.factions.values() {
                let fs = ntw_sim::campaign::effects::Effects::faction_sum(m, f.id);
                let saved: Vec<String> = fs.values.iter().filter(|(k, _)| !matches!(k.kind, ntw_sim::campaign::effects::BonusKind::Basic)).filter(|(k, _)| k.qualifier.starts_with('r') || k.qualifier.starts_with('s') || k.qualifier.starts_with('t')).map(|(k, v)| format!("{:?}/{}/{}={v}", k.kind, k.bonus, k.qualifier)).collect();
                if !saved.is_empty() { println!("FX {} {}", f.key, saved.join(" ")); }
            }
            for (k, b) in m.rules.buildings.iter().filter(|(_, b)| b.effects.iter().any(|(e, _)| e.contains(std::env::var("BLD_FILTER").as_deref().unwrap_or("cost")))) {
                println!("BLD {k} {:?}", b.effects.iter().filter(|(e, _)| e.contains(std::env::var("BLD_FILTER").as_deref().unwrap_or("cost"))).collect::<Vec<_>>());
            }
            for r in m.world.regions.values() {
                for c in &r.construction {
                    let db = m.rules.buildings.get(&c.level_key).map(|b| (b.cost, b.chain.clone()));
                    let owner = m.world.factions.get(&r.owner).map_or("?", |f| f.key.as_str());
                    let timber: f32 = r.effect_buildings().map(|b| m.rules.buildings.get(&b.level_key).map_or(0.0, |x| x.effect("building_cost_mod_all"))).sum();
                    let chain_mod = db.as_ref().map_or(0.0, |d| ntw_sim::campaign::effects::Effects::faction_sum(m, r.owner).get_qualified(ntw_sim::campaign::effects::BonusKind::Saved(2), "0", &d.1));
                    let model = m.construction_cost(r.id, &c.level_key);
                    println!("{} model {model} chain {chain_mod}", if model == c.cost { "MATCH" } else { "MISS" });
                    println!("{owner:<20} {:<22} {:<32} stored {} db {:?} ratio {:.3} local_mod {timber}", r.key, c.level_key, c.cost, db, db.as_ref().map_or(0.0, |d| c.cost as f32 / d.0.max(1) as f32));
                }
            }
            continue;
        }
        if std::env::var("ECON_TECH").is_ok() {
            match std::env::var("ECON_TECH").as_deref() {
                Ok("avail") => tech_avail(m, &db),
                Ok("rate") => tech_rate(m),
                Ok("builds") => {
                    for r in m.world.regions.values().filter(|r| r.construction.len() > 1) {
                        let items: Vec<String> = r.construction.iter().map(|c| format!("{}:{}/{}", c.level_key, c.turns_remaining, m.rules.buildings.get(&c.level_key).map_or(0, |b| b.turns))).collect();
                        println!("{} {}", r.key, items.join(" "));
                    }
                }
                Ok("queues") => {
                    for r in m.world.regions.values() {
                        let land: Vec<String> = r.recruitment_queue.iter().filter(|q| !m.rules.units.get(&q.unit_key).is_some_and(|u| u.is_naval)).map(|q| format!("{}:{}/{}", q.unit_key.rsplit('_').next().unwrap_or(""), q.turns_remaining, m.rules.units.get(&q.unit_key).map_or(0, |u| u.turns))).collect();
                        if land.len() > 1 { println!("{} cap {} queue {}", r.key, m.recruitment_points(r.id, false), land.join(" ")); }
                    }
                }
                Ok("gates") => {
                    let (mut n, mut bad) = (0, 0);
                    for r in m.world.regions.values() {
                        for q in &r.recruitment_queue {
                            n += 1;
                            if !m.unit_tech_ok(r.owner, &q.unit_key) { bad += 1; println!("  unit {} in {} needs {:?}", q.unit_key, r.key, m.rules.unit_techs.get(&q.unit_key)); }
                        }
                        for c in &r.construction {
                            n += 1;
                            if !m.building_tech_ok(r.owner, &c.level_key) { bad += 1; println!("  building {} in {} needs {:?}", c.level_key, r.key, m.rules.building_techs.get(&c.level_key)); }
                        }
                    }
                    println!("queued items {n}, failing the tech gate {bad}");
                }
                Ok("update") => {
                    let mut mm = m.clone();
                    let fs: Vec<_> = mm.world.faction_details.keys().copied().collect();
                    let (mut n, mut changed) = (0, 0);
                    for f in fs {
                        let before = mm.world.faction_details[&f].technologies.clone();
                        mm.update_tech_availability(f);
                        n += before.len();
                        changed += before.iter().zip(&mm.world.faction_details[&f].technologies).filter(|(a, b)| a != b).count();
                    }
                    println!("techs {n}, changed by the availability rule {changed}");
                }
                _ => tech_detail(&esf),
            }
            continue;
        }
        if std::env::var("ECON_SPLIT").is_ok() {
            split_check(m);
            continue;
        }
        if std::env::var("ECON_FLOW").is_ok() {
            flow_detail(m);
            continue;
        }
        if std::env::var("ECON_GARALL").is_ok() {
            garrison_all(m, &esf);
            continue;
        }
        if std::env::var("ECON_POSUM").is_ok() {
            po_summary(m, &esf);
            continue;
        }
        if let Ok(k) = std::env::var("ECON_GARRISON") {
            garrison_detail(m, &k);
            continue;
        }
        if std::env::var("ECON_CLASSES").is_ok() {
            classes_detail(m);
            continue;
        }
        if std::env::var("ECON_ROUTES").is_ok() {
            routes_detail(&esf);
            continue;
        }
        if std::env::var("ECON_TRADE").is_ok() {
            trade_detail(m);
            continue;
        }
        if let Ok(filter) = std::env::var("ECON_RGROWTH") {
            // Every term of `economy::recompute_region_with` for the regions whose key contains the
            // filter (0-B round 14: the three region-growth misses, Wallachia / Moravia / Bavaria).
            rgrowth_detail(m, &filter);
            continue;
        }
        if std::env::var("ECON_MEN").is_ok() {
            // Open item 7, the last sub-item: what size a finished unit is created at. Every unit in
            // the file against its `unit_stats_land.num_men` (the rule `spawn_recruited_unit` uses),
            // so the rule can be checked on real data instead of staying PROVISIONAL.
            let mut n = 0;
            let mut exact = 0;
            let mut by_key = 0;
            let mut differs = 0;
            let mut naval_n = 0;
            let mut below_max = 0;
            for u in m.world.forces.values().flat_map(|f| f.units.iter()) {
                let db = m.rules.units.get(&u.unit_key);
                let is_naval = db.is_some_and(|r| r.is_naval);
                let want = db.map_or(0, |r| r.men);
                // Ships carry no `num_men`; their size is the sum of the `unit_stats_naval` crew
                // triple (c17/c18/c19), which `rules::ships` already holds.
                let want = if want == 0 { m.rules.ships.get(&u.unit_key).map_or(0, |s| s.crews.iter().map(|c| *c as u32).sum()) } else { want };
                let key_agrees = m
                    .world
                    .forces
                    .values()
                    .flat_map(|f| f.units.iter())
                    .any(|o| o.unit_key == u.unit_key && o.max_men == u.max_men);
                n += 1;
                if is_naval {
                    naval_n += 1;
                }
                if u.men != u.max_men {
                    below_max += 1;
                }
                if u.max_men == want {
                    exact += 1;
                } else if want == 0 && key_agrees {
                    by_key += 1;
                    if is_naval {
                        println!(
                            "  naval {} max_men {} men {} crew {:?} class {:?}",
                            u.unit_key,
                            u.max_men,
                            u.men,
                            m.rules.ships.get(&u.unit_key).map(|s| s.crews),
                            m.rules.units.get(&u.unit_key).map(|r| r.unit_class.as_str())
                        );
                    }
                } else {
                    differs += 1;
                    if differs < 12 {
                        println!("  {} max_men {} men {} db_num_men {} naval {is_naval}", u.unit_key, u.max_men, u.men, want);
                    }
                }
            }
            println!("  {n} units: {exact} max_men == num_men, {by_key} naval (no num_men, same key), {differs} differ, {naval_n} naval, {below_max} with men < max_men");
            continue;
        }
        if std::env::var("ECON_UCOLS2").is_ok() {
            // Open item 6: the four desertion-exempt unit codes. The test is `*(unit+0x48)+0x20`
            // (0x008BA244), which is the `unit_stats_land` record's column #1 offset (`num_men`), so
            // this lists which units take each code and every distinct value the column takes.
            let mut by_men: std::collections::BTreeMap<i32, Vec<String>> = Default::default();
            for (k, u) in m.rules.units.iter() {
                if u.men == 0 {
                    continue;
                }
                by_men.entry(u.men as i32).or_default().push(k.clone());
            }
            println!("  num_men distinct values: {:?}", by_men.keys().collect::<Vec<_>>());
            for code in [4, 11, 12, 14] {
                let keys = by_men.get(&code).cloned().unwrap_or_default();
                println!("  num_men == {code}: {} units {:?}", keys.len(), &keys[..keys.len().min(40)]);
            }
            // And the classes of those units, so the exemption set can be named.
            for code in [4, 11, 12, 14] {
                for k in by_men.get(&code).into_iter().flatten().take(40) {
                    let u = &m.rules.units[k];
                    println!("    {k}: class {:?} category {:?} naval {} turns {}", u.unit_class, u.category, u.is_naval, u.turns);
                }
            }
            continue;
        }
        if std::env::var("ECON_TAXEX").is_ok() {
            // The exposure of the two round-14 fixes: every region, its owner, its **governing**
            // faction and `REGION` #19, over the whole file. The two bugs were only caught because
            // eur has a tax-exempt region and a governed region; this says how much other data would.
            tax_exempt_detail(m);
            continue;
        }
        if std::env::var("ECON_RECOMP").is_ok() {
            // The round-end GDP / growth recompute with the faction effects against the stored values of a save.
            let fx = ntw_sim::campaign::effects::Effects::compute(m);
            let (mut ok_g, mut ok_t, mut n) = (0, 0, 0);
            for r in m.world.regions.values().filter(|r| m.world.factions.get(&r.owner).is_some_and(|f| !f.key.is_empty())) {
                let (g, t) = economy::recompute_region_with(m, Some(&fx), r);
                n += 1;
                ok_g += usize::from(g == r.gdp);
                ok_t += usize::from(t == r.town_wealth_growth);
                if g != r.gdp || t != r.town_wealth_growth {
                    println!("  {} gdp {} model {} | growth {} model {}", r.key, r.gdp, g, r.town_wealth_growth, t);
                }
            }
            println!("  gdp {ok_g}/{n} growth {ok_t}/{n}");
            continue;
        }
        if std::env::var("ECON_GDP").is_ok() {
            gdp_detail(m, &esf);
            continue;
        }
        let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { continue };
        for item in &arr.items {
            let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
            let key = f.values().find_map(EsfNode::as_str).unwrap_or_default();
            let Some(fac) = m.world.factions.values().find(|x| x.key == key) else { continue };
            let regions = m.world.regions.values().filter(|r| r.owner == fac.id).count();
            if regions == 0 {
                continue;
            }
            let inc = economy::faction_income(m, fac.id);
            let mut hist = Vec::new();
            if let Some(e) = f.child("FACTION_ECONOMICS") {
                if let Some(h) = e.get(0).and_then(EsfNode::as_record_array) {
                    for it in &h.items {
                        let Some(d) = it.first().and_then(EsfNode::as_record) else { continue };
                        let a = |i: usize| d.get(i).and_then(EsfNode::as_i32_array).map(|v| v.to_vec()).unwrap_or_default();
                        hist.push((a(1), a(2), a(5)));
                    }
                }
                println!(
                    "{} regions {regions} tax {}/{} rates {:?} eff {:.4} model taxes {} other {} upkeep {} | treasury {} cur {:?}",
                    fac.key,
                    fac.tax_lower,
                    fac.tax_upper,
                    m.world.faction_details.get(&fac.id).and_then(|d| d.governorship()).map(|g| (g.taxes.lower, g.taxes.upper, g.taxes.lower_rate, g.taxes.upper_rate)),
                    economy::tax_efficiency(&m.rules, regions as i32, 0),
                    inc.taxes,
                    inc.other,
                    inc.upkeep,
                    fac.treasury,
                    e.get(3).map(|n| format!("{n:?}")),
                );
                if std::env::var("ECON_DETAIL").is_ok() { regions_detail(m, &fac.key); }
                for (i, (a1, a2, a5)) in hist.iter().enumerate() {
                    println!("    hist[{i}] #1 {a1:?} #2 {a2:?} #5 {a5:?}");
                }
            }
        }
    }
}

/// `economy_check regions <data dir> <file> <faction>`: per-region tax inputs.
#[allow(dead_code)]
pub fn regions_detail(m: &ntw_sim::campaign::CampaignModel, faction: &str) {
    let Some(f) = m.world.factions.values().find(|f| f.key == faction) else { return };
    for r in m.world.regions.values().filter(|r| r.owner == f.id) {
        let bld: Vec<(String, f32, bool)> = r
            .buildings()
            .map(|b| (b.level_key.clone(), economy::building_effect(&m.rules, &b.level_key, "tax_bonus_building"), m.rules.buildings.contains_key(&b.level_key)))
            .collect();
        println!(
            "  {} gdp {} tw {} growth {} exempt {} taxes {} buildings {:?}",
            r.key, r.gdp, r.town_wealth, r.town_wealth_growth, r.tax_exempt, economy::region_taxes(m, r), bld
        );
    }
}

/// `ECON_GDP=1`: per region, the stored GDP (#10), base GDP (#9) and the buildings' `gdp_*` effects.
#[allow(dead_code)]
pub fn gdp_detail(m: &ntw_sim::campaign::CampaignModel, esf: &EsfFile) {
    let mut base = std::collections::BTreeMap::new();
    if let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY") {
        for it in &arr.items {
            if let Some(r) = it.first().and_then(EsfNode::as_record) {
                let key = r.values().find_map(EsfNode::as_str).unwrap_or_default().to_string();
                base.insert(key, (r.get_u32(9).unwrap_or(0), r.get_u32(11).unwrap_or(0)));
            }
        }
    }
    for r in m.world.regions.values() {
        let owner = m.world.factions.get(&r.owner).map_or("", |f| f.key.as_str());
        let mut sum = 0.0;
        let mut tw = 0.0;
        let mut parts = Vec::new();
        for b in r.buildings() {
            for k in ["gdp_farm", "gdp_industry", "gdp_mine", "gdp_port", "gdp_mod_all", "tw_growth_education", "tw_growth_government", "tw_growth_industry", "tw_growth_port", "tw_growth_roads", "tw_growth_mod_all"] {
                let v = economy::building_effect(&m.rules, &b.level_key, k);
                if v != 0.0 {
                    if k.starts_with("gdp_") && k != "gdp_mod_all" {
                        sum += v;
                    }
                    if k.starts_with("tw_") && k != "tw_growth_mod_all" {
                        tw += v;
                    }
                    parts.push(format!("{}:{k}={v}", b.level_key));
                }
            }
        }
        let (b9, c11) = base.get(&r.key).copied().unwrap_or_default();
        println!(
            "{} {owner} base {b9} gdp {} c0 {c11} diff {} bld {sum} twg {} twbld {tw} tw {} {}",
            r.key,
            r.gdp,
            r.gdp as i64 - b9 as i64,
            r.town_wealth_growth,
            r.town_wealth,
            parts.join(" ")
        );
    }
}

/// `ECON_TRADE=1`: per faction, its trade partners and the GDP part of each route value
/// (trunc(var3 × sqrt(GDP_a + GDP_b)), 0x00B3F8F0) next to the stored trade figure.
#[allow(dead_code)]
pub fn trade_detail(m: &ntw_sim::campaign::CampaignModel) {
    let gdp = |f: ntw_sim::campaign::FactionId| -> i64 {
        m.rules.var("faction_gdp_other", 0.0) as i64 + m.world.regions.values().filter(|r| r.owner == f).map(|r| r.gdp as i64).sum::<i64>()
    };
    let p = m.rules.var("trade_route_value_combined_gdp_proportion", 0.0);
    for f in m.world.factions.values() {
        let partners: Vec<_> = m
            .world
            .relationships
            .iter()
            .filter(|((a, _), r)| *a == f.id && r.trade_agreement)
            .map(|((_, b), _)| *b)
            .collect();
        let mut total = 0;
        let mut parts = Vec::new();
        for b in &partners {
            let v = (p * ((gdp(f.id) + gdp(*b)) as f32).sqrt()) as i64;
            total += v;
            parts.push(format!("{}:{v}", m.world.factions.get(b).map_or("?", |x| x.key.as_str())));
        }
        let stored = m.world.faction_details.get(&f.id).and_then(|d| d.stored_trade_income);
        let model = ntw_sim::campaign::economy::trade_routes_value(m, f.id);
        let tag = if stored == Some(model) { "OK" } else { "DIFF" };
        if stored != Some(model) {
            for b in ntw_sim::campaign::economy::trade_partners(m, f.id) {
                let key = m.world.factions.get(&b).map_or("?", |x| x.key.as_str());
                let loaded = m.world.trade_paths.get(&(f.id, b)).map(|p| p.iter().map(|x| (x.waypoints.iter().any(|w| w.sea), x.volumes.clone(), m.trade_path_blockaded(f.id, x))).collect::<Vec<_>>());
                println!("    {key}: pair {} loaded {:?} built {}", ntw_sim::campaign::economy::trade_pair_value(m, f.id, b), loaded, m.build_trade_route(f.id, b).is_some());
                for p in m.world.trade_paths.get(&(f.id, b)).into_iter().flatten() {
                    for w in p.waypoints.iter().filter(|w| w.sea) {
                        for pos in [w.from_pos, w.to_pos].into_iter().flatten() {
                            let (x, y) = (pos.0.to_f32(), pos.1.to_f32());
                            let d = m.world.forces.values().filter(|n| n.is_navy && m.world.stance(f.id, n.faction) == ntw_sim::campaign::Stance::War).filter_map(|n| m.force_position(n.id).map(|q| (((q.0.to_f32() - x).powi(2) + (q.1.to_f32() - y).powi(2)).sqrt(), n.faction))).fold((f32::MAX, ntw_sim::campaign::FactionId(0)), |a, b| if b.0 < a.0 { b } else { a });
                            println!("      node {} -> {} pos ({x:.1},{y:.1}) nearest hostile navy {:.2} ({:?})", w.from, w.to, d.0, m.world.factions.get(&d.1).map(|f| f.key.clone()));
                        }
                    }
                }
            }
        }
        println!("{tag} {} gdp {} stored {:?} model {model} gdp-part {total} {}", f.key, gdp(f.id), stored, parts.join(" "));
    }
}

/// `ECON_AP=1`: action points of generals / admirals by faction (base #8, left #9).
#[allow(dead_code)]
pub fn ap_detail(m: &ntw_sim::campaign::CampaignModel) {
    let mut by: std::collections::BTreeMap<(String, String, i32, i32, bool), u32> = Default::default();
    for c in m.world.characters.values() {
        let f = m.world.factions.get(&c.faction).map_or(String::new(), |f| f.key.clone());
        let commands = m.world.forces.values().any(|x| x.commander == Some(c.id));
        *by.entry((f, c.kind.esf_name().to_string(), c.max_movement_points, c.movement_points, commands)).or_default() += 1;
    }
    for ((f, k, b, l, cmd), n) in by {
        if k == "General" || k == "admiral" || k == "colonel" || k == "captain" {
            println!("{f} {k} base {b} left {l} commands {cmd} x{n}");
        }
    }
}

/// `ECON_TURNS=n`: plays `n` rounds (France human, the AI giving no orders) and prints the
/// treasury and income of a few factions after each.
#[allow(dead_code)]
pub fn turns_detail(mut m: ntw_sim::campaign::CampaignModel, n: u32) {
    let keys = ["france", "britain", "austria", "spain", "saxony", "hessen", "portugal", "naples"];
    if let Some(f) = m.faction_by_key("france").map(|f| f.id) {
        m.turn.humans = vec![f];
    }
    m.start_campaign();
    for t in 0..=n {
        let line: Vec<String> = keys
            .iter()
            .filter_map(|k| m.faction_by_key(k))
            .map(|f| {
                let i = economy::faction_income(&m, f.id);
                format!("{} {} ({:+})", f.key, f.treasury, i.net())
            })
            .collect();
        println!("round {t}: {}", line.join(" | "));
        if t < n {
            m.end_turn();
        }
    }
}

/// `ECON_ROUTES=1`: every international route's stored parts against `Σ volume × price` with the
/// manager's u32[8] arrays #2, #4, #5, #6 as prices, and how often the resource part is non-zero.
#[allow(dead_code)]
pub fn routes_detail(esf: &EsfFile) {
    let t = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER");
    let arr = |i: usize| t.and_then(|t| t.get(i)).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    let prices: Vec<(usize, Vec<u32>)> = [2, 4, 5, 6].iter().map(|&i| (i, arr(i))).collect();
    let parts = ntw_campaign::trade::stored_parts(esf);
    if let Ok(k) = std::env::var("ROUTE_ID") {
        for (a, b, p, v) in &parts {
            if format!("{a:?} {b:?}").contains(k.as_str()) { println!("ROUTE {a:?} -> {b:?} parts {p:?} vols {v:?}"); }
        }
    }
    let mut hits = [0usize; 4];
    let (mut resource, mut sum_ok) = (0, 0);
    for (_, _, p, vols) in &parts {
        for (k, (_, pr)) in prices.iter().enumerate() {
            let v: u64 = vols.iter().zip(pr).map(|(a, b)| u64::from(*a) * u64::from(*b)).sum();
            if v == u64::from(p[1]) {
                hits[k] += 1;
            }
        }
        resource += usize::from(p[2] != 0);
        sum_ok += usize::from(p[0] == p[1] + p[2] + p[3] + p[4]);
    }
    println!("routes {} price-array hits #2/#4/#5/#6 {:?} resource>0 {resource} total=sum {sum_ok}", parts.len(), hits);
}

/// `ECON_FX=<human>`: turn-1 land / naval upkeep and taxes with the effects, the human set and the
/// campaign-start handicaps applied (to compare with EFFECTS_FIDELITY.md §5.3).
#[allow(dead_code)]
pub fn fx_detail(mut l: ntw_campaign::LoadedCampaign, human: &str) {
    use ntw_sim::campaign::economy;
    use ntw_sim::campaign::effects::{apply_start_handicaps, Effects};
    l.set_human(human);
    if !l.model.turn.started {
        apply_start_handicaps(&mut l.model);
    }
    let m = &l.model;
    let fx = Effects::compute(m);
    for key in ["france", "austria", "britain"] {
        let Some(f) = m.faction_by_key(key).map(|f| f.id) else { continue };
        let (mut land, mut naval) = (0, 0);
        for force in m.world.forces.values().filter(|x| x.faction == f) {
            for u in &force.units {
                if let Some(r) = m.rules.units.get(&u.unit_key) {
                    let c = economy::unit_upkeep(&fx, f, r);
                    if r.is_naval { naval += c } else { land += c }
                }
            }
        }
        let inc = economy::faction_income(m, f);
        println!("{key}: land upkeep {land} naval {naval} taxes {} trade {} other {}", inc.taxes, inc.trade, inc.other);
    }
}

/// `ECON_TAXEX=1`: the exposure of the two round-14 fixes over the whole file — every region with
/// `REGION` #19 (tax exempt), its owner and the faction that actually **governs** it (the faction
/// whose governorship lists it, `World::governing_faction`). The two bugs were found through
/// `eur_moravia` / `eur_wallachia` (tax exempt) and `eur_bavaria` (owned by Austria, governed by
/// Bavaria); this lists every such region in the shipped data, so the rule can be checked where it
/// is not those three.
#[allow(dead_code)]
pub fn tax_exempt_detail(m: &ntw_sim::campaign::CampaignModel) {
    let gov_map = m.world.governing_factions();
    let (mut exempt, mut foreign_gov) = (0, 0);
    for r in m.world.regions.values() {
        if m.world.factions.get(&r.owner).is_none_or(|f| f.key.is_empty()) {
            continue;
        }
        let gov = gov_map.get(&r.id).copied().unwrap_or(r.owner);
        if r.tax_exempt {
            exempt += 1;
        }
        if gov != r.owner {
            foreign_gov += 1;
            println!(
                "  governed-by-other {} owner {} gov {} owner_gov_key {} gov_key {} exempt {}",
                r.key,
                m.world.factions.get(&r.owner).map_or("", |f| f.key.as_str()),
                m.world.factions.get(&gov).map_or("", |f| f.key.as_str()),
                m.world.factions.get(&r.owner).map_or("", |f| f.government_key.as_str()),
                m.world.factions.get(&gov).map_or("", |f| f.government_key.as_str()),
                r.tax_exempt,
            );
        }
        if r.tax_exempt {
            println!(
                "  tax-exempt {} owner {} gov {} gov_key {} taxes {:?}",
                r.key,
                m.world.factions.get(&r.owner).map_or("", |f| f.key.as_str()),
                m.world.factions.get(&gov).map_or("", |f| f.key.as_str()),
                m.world.factions.get(&gov).map_or("", |f| f.government_key.as_str()),
                m.world.factions.get(&gov).map(|f| (f.tax_lower.as_str(), f.tax_upper.as_str())),
            );
        }
    }
    println!("  {} tax-exempt regions, {} governed by a faction other than the owner", exempt, foreign_gov);
}

/// `ECON_RGROWTH=<key filter>`: every term `economy::recompute_region_with` adds, for the matching
/// regions: the region-local building effects, the owner's government and tax-level effects, the
/// faction-wide part and the final scalings (0-B round 14's growth misses).
#[allow(dead_code)]
pub fn rgrowth_detail(m: &ntw_sim::campaign::CampaignModel, filter: &str) {
    use ntw_sim::campaign::effects::Effects;
    let fx = Effects::compute(m);
    let gov_map = m.world.governing_factions();
    for r in m.world.regions.values().filter(|r| r.key.contains(filter)) {
        let f = m.world.factions.get(&r.owner);
        println!(
            "{} owner {} gov_key {} gov_faction {} gov_key {} exempt {} base_gdp {} gdp {} tw {} growth {} offset {} discontent {} taxes {:?}",
            r.key,
            f.map_or("", |f| f.key.as_str()),
            f.map_or("", |f| f.government_key.as_str()),
            gov_map.get(&r.id).and_then(|g| m.world.factions.get(g)).map_or("", |f| f.key.as_str()),
            gov_map.get(&r.id).and_then(|g| m.world.factions.get(g)).map_or("", |f| f.government_key.as_str()),
            r.tax_exempt,
            r.base_gdp,
            r.gdp,
            r.town_wealth,
            r.town_wealth_growth,
            r.wealth_growth_offset,
            r.discontent_growth,
            f.map(|f| (f.tax_lower.as_str(), f.tax_upper.as_str())),
        );
        let holders = r.slots.iter().map(|s| (s.building.as_ref(), s.holder)).chain(std::iter::once((r.road.as_ref(), None)));
        for (bld, holder) in holders {
            let level = bld.map_or("", |b| b.level_key.as_str());
            let health = bld.map_or(0, |b| b.health);
            let counted = r.effect_buildings().any(|e| e.level_key == bld.map_or(String::new(), |b| b.level_key.clone()));
            let g: Vec<String> = economy::SLOT_GDP_EFFECTS.iter().filter_map(|k| {
                let v = economy::building_effect(&m.rules, level, k);
                (v != 0.0).then(|| format!("{k}={v}"))
            }).collect();
            let t: Vec<String> = economy::SLOT_TW_EFFECTS.iter().filter_map(|k| {
                let v = economy::building_effect(&m.rules, level, k);
                (v != 0.0).then(|| format!("{k}={v}"))
            }).collect();
            if g.is_empty() && t.is_empty() && counted {
                continue;
            }
            println!(
                "   slot chain {:?} holder {:?} level {level:?} health {health} counted {counted} gdp {g:?} tw {t:?}",
                m.rules.buildings.get(level).map(|x| x.chain.as_str()),
                holder.and_then(|h| m.world.factions.get(&h)).map(|f| f.key.clone()),
            );
        }
        for k in ["tw_growth_industry_global", "tw_growth_technologies_fixed", "tw_growth_home_region", "tw_growth_taxes_modifier", "tw_growth_taxes_fixed", "gdp_mod_all", "tw_growth_mod_all"] {
            println!(
                "   region_effect {k} {} | fx.faction {} | fx.region {}",
                economy::region_effect(m, r, k),
                fx.faction(r.owner, k),
                fx.region(r.id, k),
            );
        }
        for k in ["tw_growth_factionwide", "tw_growth_technologies", "tw_growth_tax_modifier", "tw_growth_tax_modifier_fixed", "tw_growth_home_region"] {
            println!("   extra {k} {}", fx.faction(r.owner, k));
        }
        let (g, tw) = economy::recompute_region_with(m, Some(&fx), r);
        println!("   => model gdp {g} growth {tw} (stored {} {})", r.gdp, r.town_wealth_growth);
    }
}

/// `ECON_FXGDP=1`: per region, the faction-wide GDP / town wealth effects the effects store gives
/// (before the campaign start, so without difficulty handicaps) next to the stored GDP and growth.
#[allow(dead_code)]
pub fn fx_gdp_detail(m: &ntw_sim::campaign::CampaignModel) {
    use ntw_sim::campaign::economy;
    use ntw_sim::campaign::effects::Effects;
    let fx = Effects::compute(m);
    let keys = ["gdp_mod_all", "tw_growth_mod_all", "tw_growth_factionwide", "tw_growth_technologies", "tw_growth"];
    let (mut n, mut nonzero) = (0, 0);
    for r in m.world.regions.values().filter(|r| m.world.factions.contains_key(&r.owner)) {
        let vals: Vec<(String, f32, f32)> = keys.iter().map(|k| (k.to_string(), fx.region(r.id, k), fx.region_local(r.id, k))).collect();
        let (gdp, growth) = economy::recompute_region(m, r);
        n += 1;
        if vals.iter().any(|(_, t, l)| t != l) {
            nonzero += 1;
            println!("{} gdp stored {} model {} growth stored {} model {} | {:?}", r.key, r.gdp, gdp, r.town_wealth_growth, growth, vals);
        }
    }
    println!("regions {n}, with a faction-wide GDP/growth effect {nonzero}");
}

/// `ECON_PRICES=1`: the price formula of 0x00BCB020 against the stored prices:
/// `price = max(1, round(D × f / (background_commodity_supply + S)))`, D = Σ region #32, S = Σ region #31
/// + the fleets' node volumes, f = manager #3.
#[allow(dead_code)]
pub fn prices_detail(m: &ntw_sim::campaign::CampaignModel, esf: &EsfFile) {
    let t = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER").expect("trade");
    let u = |i: usize| t.get(i).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    let f: Vec<f32> = t.get(3).and_then(EsfNode::as_f32_array).map(<[f32]>::to_vec).unwrap_or_default();
    let regions = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY").expect("regions");
    let n = f.len();
    let (mut d, mut s) = (vec![0u64; n], vec![0u64; n]);
    for it in &regions.items {
        let Some(r) = it.first().and_then(EsfNode::as_record) else { continue };
        for c in 0..n {
            d[c] += u64::from(r.get(32).and_then(EsfNode::as_u32_array).and_then(|a| a.get(c).copied()).unwrap_or(0));
            s[c] += u64::from(r.get(31).and_then(EsfNode::as_u32_array).and_then(|a| a.get(c).copied()).unwrap_or(0));
        }
    }
    let mut fleets = vec![0u64; n];
    for fac in m.world.factions.values() {
        for (c, v) in m.trade_supply(fac.id).unwrap_or_default().into_iter().enumerate() {
            if c < n {
                fleets[c] += u64::from(v);
            }
        }
    }
    let bg = m.rules.var("background_commodity_supply", 0.0);
    let model: Vec<u32> = (0..n)
        .map(|c| ((d[c] as f32 * f[c] / (bg + (s[c] + fleets[c]) as f32)).round_ties_even() as u32).max(1))
        .collect();
    println!("initial #2 {:?}\nstored price #4 {:?}\nprev #5 {:?}\nprev2 #6 {:?}\ntrend #7 {:?}", u(2), u(4), u(5), u(6), u(7));
    println!("D {d:?}\nS {s:?} fleets {fleets:?} bg {bg}\nf {f:?}\nmodel price {model:?}");
}

/// `ECON_CAPS=1`: per faction, its international routes with / without a sea hop next to the sea
/// cap of its capital's buildings (`trade_routes_mod_max_sea`) and of all its regions.
#[allow(dead_code)]
pub fn caps_detail(m: &ntw_sim::campaign::CampaignModel) {
    use ntw_sim::campaign::economy::building_effect;
    for f in m.world.factions.values() {
        let routes: Vec<&ntw_sim::campaign::trade::TradePath> = m.world.trade_paths.iter().filter(|((a, _), _)| *a == f.id).flat_map(|(_, p)| p).collect();
        if routes.is_empty() {
            continue;
        }
        let sea = routes.iter().filter(|p| p.waypoints.iter().any(|w| w.sea)).count();
        let cap_of = |r: &ntw_sim::campaign::Region| -> f32 { r.buildings().map(|b| building_effect(&m.rules, &b.level_key, "trade_routes_mod_max_sea")).sum() };
        let capital = m.world.capital(f.id).and_then(|c| m.world.regions.get(&c)).map_or(0.0, cap_of);
        let all: f32 = m.world.regions.values().filter(|r| r.owner == f.id).map(cap_of).sum();
        println!("{:<24} routes {} sea {} land {} | sea cap capital {capital} all regions {all} bfs {} {}", f.key, routes.len(), sea, routes.len() - sea, m.sea_route_cap(f.id), if sea as i32 > m.sea_route_cap(f.id) { "OVER" } else { "" });
    }
}

/// `ECON_FXTIME=1`: how long one `Effects::compute` takes.
#[allow(dead_code)]
pub fn fx_time(m: &ntw_sim::campaign::CampaignModel) {
    let t = std::time::Instant::now();
    for _ in 0..20 {
        std::hint::black_box(ntw_sim::campaign::effects::Effects::compute(m));
    }
    println!("Effects::compute: {:.3} ms", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
}

/// `ECON_PO=1`: each region's stored per-class public-order factors (`POPULATION_CLASS` #1 i32[13]
/// happiness, #2 i32[6] repression, #3 positive, #4 negative, #5 repression total) next to the model.
#[allow(dead_code)]
pub fn po_detail(m: &ntw_sim::campaign::CampaignModel, esf: &EsfFile) {
    use ntw_sim::campaign::economy;
    let regions = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY").expect("regions");
    for it in &regions.items {
        let Some(r) = it.first().and_then(EsfNode::as_record) else { continue };
        let key = r.get_str(0).unwrap_or_default();
        let Some(reg) = m.world.regions.values().find(|x| x.key == key) else { continue };
        let owner = m.world.factions.get(&reg.owner).map_or("?", |f| f.key.as_str());
        let religion = m.world.faction_details.get(&reg.owner).map_or("", |d| d.religion.as_str());
        let po = economy::public_order(m, reg.id);
        let factors = economy::public_order_factors(m, reg.id);
        println!("{key} owner {owner} ({religion}) religions {:?} | model lower {} upper {}", reg.religions.iter().filter(|(_, s)| *s > 0.0).collect::<Vec<_>>(), po.lower, po.upper);
        let Some(classes) = r.child("POPULATION").and_then(|p| p.child("REGION_FACTORS")).and_then(|f| f.record_array("POPULATION CLASSES")) else { continue };
        for c in classes.records() {
            let name = c.get_str(0).unwrap_or_default();
            let h = c.get(1).and_then(EsfNode::as_i32_array).map(<[i32]>::to_vec).unwrap_or_default();
            let p = c.get(2).and_then(EsfNode::as_i32_array).map(<[i32]>::to_vec).unwrap_or_default();
            let model = factors.iter().find(|f| f.class == name);
            println!(
                "   {name:<6} stored happy {h:?} rep {p:?} pos {} neg {} rep {} | model happy {:?} rep {:?}",
                c.get_i32(3).unwrap_or(0),
                c.get_i32(4).unwrap_or(0),
                c.get_i32(5).unwrap_or(0),
                model.map(|f| f.happiness),
                model.map(|f| f.repression)
            );
        }
    }
}

/// `ECON_POSUM=1`: per slot, how many present population classes (any stored factor ≠ 0) the model
/// reproduces exactly; mismatching classes are printed.
#[allow(dead_code)]
pub fn po_summary(m: &ntw_sim::campaign::CampaignModel, esf: &EsfFile) {
    use ntw_sim::campaign::economy;
    let regions = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY").expect("regions");
    let (mut n, mut exact) = (0, 0);
    let mut slot_ok = [0usize; 19];
    for it in &regions.items {
        let Some(r) = it.first().and_then(EsfNode::as_record) else { continue };
        let key = r.get_str(0).unwrap_or_default();
        let Some(reg) = m.world.regions.values().find(|x| x.key == key) else { continue };
        let factors = economy::public_order_factors(m, reg.id);
        let Some(classes) = r.child("POPULATION").and_then(|p| p.child("REGION_FACTORS")).and_then(|f| f.record_array("POPULATION CLASSES")) else { continue };
        for c in classes.records() {
            let name = c.get_str(0).unwrap_or_default();
            let h = c.get(1).and_then(EsfNode::as_i32_array).map(<[i32]>::to_vec).unwrap_or_default();
            let p = c.get(2).and_then(EsfNode::as_i32_array).map(<[i32]>::to_vec).unwrap_or_default();
            let Some(f) = factors.iter().find(|f| f.class == name) else { continue };
            if h.iter().chain(&p).all(|v| *v == 0) && f.happiness.iter().chain(&f.repression).all(|v| *v == 0) {
                continue;
            }
            n += 1;
            let mut all = true;
            for (i, ok) in slot_ok.iter_mut().enumerate() {
                let (s, mv) = if i < 13 { (h.get(i).copied().unwrap_or(0), f.happiness[i]) } else { (p.get(i - 13).copied().unwrap_or(0), f.repression[i - 13]) };
                if s == mv {
                    *ok += 1;
                } else {
                    all = false;
                }
            }
            if all {
                exact += 1;
            } else {
                println!("{key} {name}: stored {h:?} {p:?} model {:?} {:?}", f.happiness, f.repression);
            }
        }
    }
    println!("classes {n}, exact {exact}, per slot {slot_ok:?}");
}

/// `ECON_GARRISON=<region>`: the forces inside a region's settlement and their units.
#[allow(dead_code)]
pub fn garrison_detail(m: &ntw_sim::campaign::CampaignModel, key: &str) {
    let Some(r) = m.world.regions.values().find(|r| r.key == key) else { return };
    println!("{key} population {} garrison {:?}", r.population, r.garrison);
    for fid in m.defenders_of(r.id) {
        let f = &m.world.forces[&fid];
        let units: Vec<String> = f.units.iter().map(|u| format!("{}({}/{})", u.unit_key, u.men, u.max_men)).collect();
        println!("  force {:?} commander {:?}: {}", fid, f.commander, units.join(" "));
    }
    for c in m.world.characters.values().filter(|c| c.garrisoned_in == Some(r.id)) {
        println!("  character {:?} {}", c.id, c.kind.esf_name());
    }
}

/// `ECON_CLASSES=1`: the unit class of every unit key containing "Militia" (research).
#[allow(dead_code)]
pub fn classes_detail(m: &ntw_sim::campaign::CampaignModel) {
    let mut seen = std::collections::BTreeMap::new();
    for (k, u) in &m.rules.units {
        *seen.entry((u.unit_class.clone(), k.contains("Militia"))).or_insert(0) += 1;
    }
    for ((c, mil), n) in seen {
        println!("{c} militia-key {mil} x{n}");
    }
}

/// `ECON_GARALL=1`: every region's stored garrison repression next to its settlement's forces and
/// their unit classes (research: which units count twice).
#[allow(dead_code)]
pub fn garrison_all(m: &ntw_sim::campaign::CampaignModel, esf: &EsfFile) {
    let regions = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER/REGIONS_ARRAY").expect("regions");
    for it in &regions.items {
        let Some(r) = it.first().and_then(EsfNode::as_record) else { continue };
        let key = r.get_str(0).unwrap_or_default();
        let Some(reg) = m.world.regions.values().find(|x| x.key == key) else { continue };
        let Some(classes) = r.child("POPULATION").and_then(|p| p.child("REGION_FACTORS")).and_then(|f| f.record_array("POPULATION CLASSES")) else { continue };
        let nonzero = |c: &&ntw_formats::esf::EsfRecord| [1, 2].iter().any(|i| c.get(*i).and_then(EsfNode::as_i32_array).is_some_and(|v| v.iter().any(|x| *x != 0)));
        let Some(c) = classes.records().find(nonzero) else { continue };
        let p = c.get(2).and_then(EsfNode::as_i32_array).map(<[i32]>::to_vec).unwrap_or_default();
        let stored = p.get(4).copied().unwrap_or(0);
        let mut parts = Vec::new();
        for fid in m.defenders_of(reg.id) {
            let f = &m.world.forces[&fid];
            let g = if reg.garrison == Some(fid) { "G" } else { "A" };
            let cls: Vec<String> = f
                .units
                .iter()
                .map(|u| m.rules.units.get(&u.unit_key).map_or("?".to_string(), |x| if x.unit_class == "infantry_militia" { format!("MIL:{}", u.unit_key) } else { format!("{}/{}", x.unit_class, x.category) }))
                .collect();
            parts.push(format!("{g}{}[{}]", f.units.len(), cls.join(",")));
        }
        let cn = c.get_str(0).unwrap_or_default();
        let model = ntw_sim::campaign::economy::public_order_factors(m, reg.id).iter().find(|x| x.class == cn).map_or(-1, |c| c.repression[4]);
        let cname = c.get_str(0).unwrap_or_default();
        println!("{key} {cname} pop {} stored {stored} model {model} | {}", reg.population, parts.join(" "));
    }
}

/// `ECON_FLOW=1`: per exporter, its domestic routes (trade node → own port, volumes) and its international
/// routes (importer, node hops, volumes): research for the supply assignment (0x00BC0DC0).
#[allow(dead_code)]
pub fn flow_detail(m: &ntw_sim::campaign::CampaignModel) {
    let name = |f: ntw_sim::campaign::FactionId| m.world.factions.get(&f).map_or("?".to_string(), |x| x.key.clone());
    for (f, doms) in &m.world.domestic_trade {
        if doms.is_empty() {
            continue;
        }
        println!("{} prices {:?}", name(*f), m.world.commodity_prices);
        for d in doms {
            let hops: Vec<String> = d.waypoints.iter().map(|w| format!("{}>{}{}", w.from, w.to, if w.sea { "s" } else { "" })).collect();
            println!("  dom {} vol {:?}", hops.join(" "), d.volumes);
        }
        for ((a, b), paths) in &m.world.trade_paths {
            if a != f {
                continue;
            }
            let mut dem = vec![0u64; m.world.commodity_prices.len()];
            for r in m.world.regions.values().filter(|r| r.owner == *b) {
                for (i, d) in m.commodity_demand(r).iter().enumerate() {
                    if let Some(x) = dem.get_mut(i) {
                        *x += u64::from(*d);
                    }
                }
            }
            let gdp: u64 = m.world.regions.values().filter(|r| r.owner == *b).map(|r| u64::from(r.gdp)).sum();
            println!("     {} demand {:?} gdp {gdp}", name(*b), dem);
            for p in paths {
                let hops: Vec<String> = p.waypoints.iter().map(|w| format!("{}>{}{}", w.from, w.to, if w.sea { "s" } else { "" })).collect();
                println!("  -> {:<20} {} vol {:?}", name(*b), hops.join(" "), p.volumes);
            }
        }
    }
}

/// `ECON_SPLIT=1`: the supply split (`CampaignModel::trade_split`) against every loaded route's volumes.
#[allow(dead_code)]
pub fn split_check(m: &ntw_sim::campaign::CampaignModel) {
    let name = |f: ntw_sim::campaign::FactionId| m.world.factions.get(&f).map_or("?".to_string(), |x| x.key.clone());
    let (mut n, mut ok) = (0, 0);
    let exporters: std::collections::BTreeSet<_> = m.world.trade_paths.keys().map(|(a, _)| *a).collect();
    for a in exporters {
        let split = m.trade_split(a);
        let fs = ntw_sim::campaign::effects::Effects::faction_sum(m, a);
        println!("{} supply {:?} trade_node_supply_mod {} mod_gdp {}", name(a), m.trade_supply(a), fs.get("trade_node_supply_mod"), fs.get("mod_gdp"));
        for ((x, b), paths) in &m.world.trade_paths {
            if *x != a {
                continue;
            }
            for (i, p) in paths.iter().enumerate() {
                n += 1;
                let model = m.trade_path_volumes(split.as_ref(), *b, p);
                if model == p.volumes {
                    ok += 1;
                } else {
                    println!("  {} -> {} [{i}] loaded {:?} model {:?}", name(a), name(*b), p.volumes, model);
                }
            }
        }
    }
    println!("routes {n}, volumes exact {ok}");
}

/// `ECON_TECH=1`: every faction's `FACTION_TECHNOLOGY_MANAGER` entries that are not plain state 4 / 2 with no progress.
#[allow(dead_code)]
pub fn tech_detail(esf: &EsfFile) {
    let Some(arr) = esf.root.find_record_array("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY") else { return };
    for item in &arr.items {
        let Some(f) = item.first().and_then(EsfNode::as_record) else { continue };
        let key = f.values().find_map(EsfNode::as_str).unwrap_or_default();
        let Some(tm) = f.child("FACTION_TECHNOLOGY_MANAGER") else { continue };
        let tail: Vec<String> = tm.children.iter().skip(1).map(|n| format!("{n:?}").chars().take(60).collect()).collect();
        let mut lines = Vec::new();
        if let Some(techs) = tm.record_array("techs") {
            for it in &techs.items {
                let k = it.first().and_then(EsfNode::as_str).unwrap_or_default();
                let st = it.get(1).and_then(EsfNode::as_u32).unwrap_or(99);
                let p = it.get(2).and_then(EsfNode::as_f32).unwrap_or(-1.0);
                let a = it.get(3).and_then(EsfNode::as_u32).unwrap_or(99);
                let l = it.get(4).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
                let b = it.get(5).and_then(EsfNode::as_u32).unwrap_or(99);
                if st == 4 && p == 0.0 && a == 0 && l.is_empty() && b == 0 {
                    continue;
                }
                lines.push(format!("    {k} st {st} prog {p} a {a} list {l:?} b {b}"));
            }
        }
        println!("{key} tail {tail:?}");
        for l in lines {
            println!("{l}");
        }
    }
}

/// `ECON_TECH=avail`: the stored state of every tech next to its prerequisites and building level.
#[allow(dead_code)]
pub fn tech_avail(m: &ntw_sim::campaign::CampaignModel, db: &GameDatabase) {
    let req: Vec<(String, String)> = db.campaign.tech_requirements.iter().map(|r| (r.technology.clone(), r.required.clone())).collect();
    let allowed: std::collections::BTreeSet<(String, String)> = db.campaign.tech_factions.iter().map(|r| (r.technology.clone(), r.faction.clone())).collect();
    let mut tally: std::collections::BTreeMap<String, usize> = Default::default();
    for (fid, d) in &m.world.faction_details {
        let Some(f) = m.world.factions.get(fid) else { continue };
        let state = |k: &str| d.technologies.iter().find(|(t, _)| t == k).map(|(_, s)| *s);
        let owned: std::collections::BTreeSet<&str> = m.world.regions.values().filter(|r| r.owner == *fid).flat_map(|r| r.buildings()).map(|b| b.level_key.as_str()).collect();
        for (k, s) in &d.technologies {
            let prereqs: Vec<&str> = req.iter().filter(|(t, _)| t == k).map(|(_, r)| r.as_str()).collect();
            let pre_ok = prereqs.iter().all(|p| state(p) == Some(0));
            let bl = db.technology(k).map(|t| t.building_level.clone()).unwrap_or_default();
            let need = m.rules.buildings.get(&bl).map(|b| (b.chain.clone(), b.level));
            let has_bl = need.as_ref().is_some_and(|(c, l)| owned.iter().any(|o| m.rules.buildings.get(*o).is_some_and(|b| b.chain.starts_with(c.as_str()) && b.level >= *l)));
            let fac_ok = allowed.contains(&(k.clone(), f.key.clone()));
            let key = format!("state {s} pre_ok {pre_ok} has_building {has_bl} faction_ok {fac_ok}");
            *tally.entry(key.clone()).or_default() += 1;
            if std::env::var("TECH_V").is_ok() {
                println!("{} {k} {key} bl {bl} pre {prereqs:?}", f.key);
            }
        }
    }
    for (k, n) in tally {
        println!("{n:5} {k}");
    }
}

/// `ECON_TECH=rate`: each technology under research, its school's rate and the stored progress.
#[allow(dead_code)]
pub fn tech_rate(m: &ntw_sim::campaign::CampaignModel) {
    for (fid, d) in &m.world.faction_details {
        let Some(f) = m.world.factions.get(fid) else { continue };
        for (k, t) in d.research.iter().filter(|(_, t)| t.researcher != 0) {
            let Some((r, s)) = m.school_slot(t.researcher) else {
                println!("{} {k}: school {} not found", f.key, t.researcher);
                continue;
            };
            let rate = m.research_rate(r, s, k);
            let parts = m.research_parts(r, s);
            let alt: Vec<String> = (0..3).map(|th| format!("{:.2}", parts.map_or(0.0, |p| p.rate(th)))).collect();
            let reg = &m.world.regions[&r];
            let b = reg.slots[s].building.as_ref().map(|b| b.level_key.clone()).unwrap_or_default();
            if std::env::var("SHOW_LOCAL").is_ok() { println!("    local {:?}", m.rules.effects.building_local().get(&b)); }
            let gents: Vec<i32> = m
                .gentlemen_in(r, s)
                .iter()
                .map(|c| ntw_sim::campaign::effects::Effects::character_attribute(m, *c, "research").unwrap_or(-9))
                .collect();
            for c in m.gentlemen_in(r, s) {
                let e = ntw_sim::campaign::effects::Effects::character_effects(m, c);
                let d = m.world.character_details.get(&c);
                println!("    gent {:?} traits {:?} anc {:?} set {:?}", c, d.map(|d| d.traits.iter().map(|t| (t.key.clone(), t.points)).collect::<Vec<_>>()), d.map(|d| d.ancillaries.clone()), e.values);
            }
            let fx = ntw_sim::campaign::effects::Effects::faction_sum(m, *fid);
            println!(
                "{:<14} {k:<34} {} {b:<22} gents {gents:?} fac rp {} rmod {} | rate {rate:.3} progress {:.3} ratio {:.3} alt {alt:?} parts {parts:?}",
                f.key,
                reg.key,
                fx.get("research_points"),
                fx.get("research_rate_mod"),
                t.progress,
                t.progress / rate.max(0.001)
            );
        }
    }
}

/// `ECON_RECRUITCOST=1`: every queued recruitment item of the file, the cost the original stored in it
/// (`RECRUITMENT_ITEM` #4, the item's `+0x20` = the recruitable entry's cost, `0x00AF3F80`) next to
/// the model's [`economy::recruitment_cost`], the plain `units` #4 / #7 columns, the region's and the
/// faction's `recruitment_mod_cost_*` sums and the turns left / the unit's turns.
#[allow(dead_code)]
pub fn recruit_cost_detail(m: &ntw_sim::campaign::CampaignModel, db: &GameDatabase) {
    let (mut items, mut exact) = (0, 0);
    for r in m.world.regions.values() {
        let owner = m.world.factions.get(&r.owner).map_or("?", |f| f.key.as_str());
        for it in &r.recruitment_queue {
            let Some(u) = db.units.iter().find(|u| u.key == it.unit_key) else { continue };
            let set = economy::region_effect_set(m, r);
            let model = m.rules.units.get(&it.unit_key).map(|ur| economy::recruitment_cost_in(&m.rules, &set, &it.unit_key, ur));
            items += 1;
            exact += usize::from(model == Some(it.cost));
            let fset = ntw_sim::campaign::effects::Effects::faction_sum(m, r.owner);
            let key = if u.category.starts_with("naval") { "recruitment_mod_cost_naval_all" } else { "recruitment_mod_cost_land_all" };
            println!("  {} {owner} {}: saved {} model {model:?} | #4 {} #7 {} | mod region {} faction {} | turns {}/{}", r.key, it.unit_key, it.cost, u.recruitment_cost, u.unknown_3c, set.get(key), fset.get(key), it.turns_remaining, u.unknown_38);
        }
    }
    println!("  RECRUITCOST {exact}/{items} exact");
}

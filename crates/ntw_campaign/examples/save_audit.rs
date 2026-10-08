//! Save compatibility research tool (read-only): lists the records that link characters, forces,
//! units and settlements, so a save written by NapoleonRust can be compared with one written by the
//! original (`analysis/campaign/SAVE_COMPAT.md`).
//!
//! ```text
//! cargo run -p ntw_campaign --example save_audit -- forces FILE [faction_key]
//! cargo run -p ntw_campaign --example save_audit -- new OLD NEW [faction_key]
//! ```
//! * `forces`: every character (id, type, #4 force, #5 unit, position) and every force (kind,
//!   id, commander, units with their ids and #10) of one faction or all.
//! * `new`: the characters, forces and units present in NEW but not in OLD, with their records.

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

fn factions(esf: &EsfFile) -> Vec<&EsfRecord> {
    let mut out = Vec::new();
    if let Some(w) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") {
        if let Some(a) = w.record_array("FACTION_ARRAY") {
            out.extend(a.records());
        }
        if let Some(r) = w.child("REBEL_FACTION").and_then(|r| r.child("FACTION")) {
            out.push(r);
        }
    }
    out
}

fn fkey(f: &EsfRecord) -> String {
    f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string()
}

fn short(n: &EsfNode) -> String {
    let s = format!("{n:?}");
    if s.len() > 90 {
        format!("{}...", &s[..s.char_indices().nth(90).map_or(s.len(), |c| c.0)])
    } else {
        s
    }
}

fn dump(r: &EsfRecord, depth: usize, max: usize) {
    let pad = "  ".repeat(depth);
    for (i, c) in r.children.iter().enumerate() {
        match c {
            EsfNode::Record(x) => {
                println!("{pad}#{i} {{{}}} v{}", x.name, x.version);
                if depth < max {
                    dump(x, depth + 1, max);
                }
            }
            EsfNode::RecordArray(a) => {
                println!("{pad}#{i} [{}] v{} ({} items)", a.name, a.version, a.items.len());
                if depth < max {
                    for (k, item) in a.items.iter().take(std::env::var("ITEMS").ok().and_then(|s| s.parse().ok()).unwrap_or(3)).enumerate() {
                        for n in item {
                            if let EsfNode::Record(x) = n {
                                println!("{pad}  [{k}] {{{}}} v{}", x.name, x.version);
                                dump(x, depth + 2, max);
                            } else {
                                println!("{pad}  [{k}] {}", short(n));
                            }
                        }
                    }
                }
            }
            other => println!("{pad}#{i} {}", short(other)),
        }
    }
}

struct Char<'a> {
    rec: &'a EsfRecord,
    id: i32,
}

fn chars(f: &EsfRecord) -> Vec<Char<'_>> {
    f.record_array("CHARACTER_ARRAY")
        .into_iter()
        .flat_map(|a| a.records())
        .filter(|r| r.name == "CHARACTER")
        .map(|rec| Char { rec, id: rec.get_i32(2).unwrap_or(-1) })
        .collect()
}

fn forces(f: &EsfRecord) -> Vec<&EsfRecord> {
    f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()).collect()
}

fn force_id(r: &EsfRecord) -> u32 {
    r.child("MILITARY_FORCE").and_then(|m| m.get_u32(0)).unwrap_or(0)
}

fn units(r: &EsfRecord) -> Vec<&EsfRecord> {
    r.record_array("UNITS_ARRAY")
        .into_iter()
        .flat_map(|a| a.records())
        .filter_map(|w| w.child("UNIT"))
        .collect()
}

fn print_forces(esf: &EsfFile, only: Option<&str>) {
    for f in factions(esf) {
        let key = fkey(f);
        if only.is_some_and(|o| o != key) {
            continue;
        }
        println!("== faction {key:?}");
        for c in chars(f) {
            let loco = c.rec.children.first().and_then(EsfNode::as_record);
            let pos = loco.map(|l| (l.get_i32(0).unwrap_or(0) as f64 / 1048576.0, l.get_i32(1).unwrap_or(0) as f64 / 1048576.0));
            println!(
                "  char {} {:?} #4={:?} #5={:?} pos={:?}",
                c.id,
                c.rec.get_str(3).unwrap_or(""),
                c.rec.get(4).map(short),
                c.rec.get(5).map(short),
                pos
            );
        }
        for r in forces(f) {
            let mf = r.child("MILITARY_FORCE");
            let us: Vec<String> = units(r)
                .iter()
                .map(|u| format!("{}:{}/{}", u.get_i32(4).unwrap_or(-1), u.child("UNIT_RECORD_KEY").and_then(|k| k.get_str(0)).unwrap_or(""), u.get_u32(10).unwrap_or(0)))
                .collect();
            println!(
                "  {} id={} cmd={:?} tail={:?}  units[{}] {}",
                r.name,
                force_id(r),
                mf.and_then(|m| m.get_u32(1)),
                r.children.iter().skip(2).map(short).collect::<Vec<_>>(),
                us.len(),
                us.join(" ")
            );
        }
    }
}

fn ids(esf: &EsfFile) -> (BTreeSet<i32>, BTreeSet<u32>, BTreeSet<i32>) {
    let (mut c, mut fo, mut u): (BTreeSet<i32>, BTreeSet<u32>, BTreeSet<i32>) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for f in factions(esf) {
        c.extend(chars(f).iter().map(|c| c.id));
        for r in forces(f) {
            fo.insert(force_id(r));
            u.extend(units(r).iter().filter_map(|u| u.get_i32(4)));
        }
    }
    (c, fo, u)
}

fn print_new(old: &EsfFile, new: &EsfFile, only: Option<&str>, depth: usize) {
    let (oc, of, ou) = ids(old);
    for f in factions(new) {
        let key = fkey(f);
        if only.is_some_and(|o| o != key) {
            continue;
        }
        for c in chars(f) {
            if !oc.contains(&c.id) {
                println!("== {key}: new CHARACTER {} v{}", c.id, c.rec.version);
                dump(c.rec, 1, depth);
            }
        }
        for r in forces(f) {
            if !of.contains(&force_id(r)) {
                println!("== {key}: new {} v{} {}", r.name, r.version, force_id(r));
                dump(r, 1, depth);
            } else {
                for u in units(r) {
                    if !u.get_i32(4).is_some_and(|i| ou.contains(&i)) {
                        println!("== {key}: new unit in force {}: {:?}", force_id(r), u.get_i32(4));
                    }
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let open = |p: &str| EsfFile::open(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    match args.first().map(String::as_str) {
        Some("forces") => print_forces(&open(&args[1]), args.get(2).map(String::as_str)),
        Some("new") => print_new(
            &open(&args[1]),
            &open(&args[2]),
            args.get(3).map(String::as_str).filter(|s| *s != "*"),
            args.get(4).and_then(|s| s.parse().ok()).unwrap_or(2),
        ),
        Some("orphans") => orphans(&open(&args[1]), &open(&args[2])),
        Some("faction_fields") => faction_fields(&open(&args[1])),
        Some("faction_layout") => faction_layout(&open(&args[1])),
        Some("obstacle_pos") => obstacle_pos(&open(&args[1])),
        Some("obstacle_dump") => obstacle_dump(&open(&args[1]), &args[2..].iter().map(|s| s.parse().unwrap()).collect::<Vec<u32>>()),
        Some("obstacle_slots") => obstacle_slots(&open(&args[1])),
        Some("force_units") => force_units(&open(&args[1]), args[2].parse().unwrap(), args[3].parse().unwrap(), args[4].parse().unwrap()),
        Some("unit_fields") => unit_fields(&open(&args[1])),
        Some("victory") => victory(&open(&args[1])),
        Some("victory_options") => victory_options(&open(&args[1]), args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6)),
        Some("region_owner_diff") => region_owner_diff(&open(&args[1]), &open(&args[2]), &args[3]),
        Some("region_owners") => { for p in &args[1..] { println!("== {p}"); region_owners(&open(p)); } }
        Some("obstacle_owner_refs") => obstacle_owner_refs(&open(&args[1]), args[2].parse().unwrap()),
        Some("rebel_mirrors") => { for p in &args[1..] { println!("== {p}"); rebel_mirrors(&open(p)); } }
        Some("obstacle_consistency") => { for p in &args[1..] { println!("== {p}"); obstacle_consistency(&open(p)); } }
        Some("boundary_indexes") => { for p in &args[1..] { println!("== {p}"); boundary_indexes(&open(p)); } }
        Some("grid_shape") => grid_shape(&open(&args[1]), args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2)),
        Some("grid_lists") => { for p in &args[1..] { println!("== {p}"); grid_lists(&open(p)); } }
        Some("grid_pairs") => grid_pairs(&open(&args[1]), args[2].parse().unwrap(), args[3].parse().unwrap()),
        Some("grid_pair_stats") => { for p in &args[1..] { println!("== {p}"); grid_pair_stats(&open(p)); } }
        Some("grid_items") => { for p in &args[1..] { println!("== {p}"); grid_items(&open(p)); } }
        Some("manager_lists") => { for p in &args[1..] { println!("== {p}"); manager_lists(&open(p)); } }
        Some("piece_refs") => { for p in &args[1..] { println!("== {p}"); piece_refs(&open(p)); } }
        Some("list_slots") => { for p in &args[1..] { println!("== {p}"); list_slots(&open(p)); } }
        Some("list_perm") => { for p in &args[1..] { println!("== {p}"); list_perm(&open(p)); } }
        Some("find_str") => { let e = open(&args[1]); for n in &args[2..] { println!("== {n}"); find_str(&e, n); } }
        Some("layout") => layout(&open(&args[1]), args.get(2).map_or("", String::as_str)),
        Some("commander_fields") => commander_fields(&open(&args[1])),
        Some("show") => show(&open(&args[1]), &args[2], args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3)),
        Some("findall") => findall(&open(&args[1]), args[2].parse().expect("number"), args.get(3).map_or("", String::as_str)),
        Some("items") => items(&open(&args[1]), &args[2], &args[3]),
        Some("item") => item(&open(&args[1]), &args[2], &args[3], args[4].parse().unwrap(), args.get(5).and_then(|s| s.parse().ok()).unwrap_or(2)),
        Some("trade_routes") => trade_routes(&open(&args[1]), args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5)),
        Some("tails") => tails(&open(&args[1]), &args[2], args[3].parse().unwrap()),
        Some("char39") => char39(&open(&args[1])),
        Some("refsites") => refsites(&open(&args[1])),
        Some("diff") => diff(&open(&args[1]), &open(&args[2]), args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3)),
        Some("paths") => paths(&open(&args[1]), &args[2], args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10)),
        Some("obstacles") => obstacles(&open(&args[1])),
        Some("obstacle_refs") => obstacle_refs(&open(&args[1])),
        Some("load_check") => {
            // The pathfinder loader replay (grid_load_check): faults = the original would crash.
            for p in &args[1..] {
                let e = open(p);
                let Some(grid) = e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|r| r.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { println!("{p}: no grid"); continue };
                let r = ntw_campaign::grid_load_check::check(grid);
                println!("{p}\n  versions {} rows {} nodes {} obstacles {}: {} faults, {} rule breaks", r.counts[0], r.counts[1], r.counts[2], r.counts[3], r.faults.len(), r.rule_breaks.len());
                for f in &r.faults { println!("  FAULT {f}"); }
                for b in &r.rule_breaks { println!("  rule  {b}"); }
            }
        }
        Some("u32s") => u32s(&open(&args[1]), &args[2], args[3].parse().unwrap(), args[4].parse().unwrap(), args[5].parse().unwrap()),
        Some("check") => {
            for p in &args[1..] {
                let r = ntw_campaign::save_check::check(&open(p));
                println!(
                    "== {p}: {} violations, {} informational (original's loader only), {} commanders without obstacle, AI block rebuilt: {}",
                    r.violations.len(),
                    r.informational.len(),
                    r.commanders_without_obstacle.len(),
                    r.ai_block_rebuilt
                );
                for v in r.violations.iter().take(15) {
                    println!("  {v}");
                }
                for v in r.informational.iter().take(15) {
                    println!("  (info) {v}");
                }
            }
        }
        Some("recruits") => recruits(&open(&args[1])),
        Some("records") => records(&open(&args[1]), &args[2], args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10)),
        Some("nan") => {
            let esf = open(&args[1]);
            let mut out = Vec::new();
            find_nan(&esf.root.children, &esf.root.name, &mut out);
            println!("{} NaN values", out.len());
            for p in out.iter().take(80) {
                println!("  {p}");
            }
        }
        Some("find") => {
            let esf = open(&args[1]);
            for v in &args[2..] {
                let mut out = Vec::new();
                find_value(&esf.root, v.parse().expect("number"), "", &mut out);
                println!("== {v}: {} hits", out.len());
                for p in out.iter().take(40) {
                    println!("  {p}");
                }
            }
        }
        _ => eprintln!("usage: save_audit find FILE N... | forces FILE [faction] | new OLD NEW [faction|*] [depth]"),
    }
}

/// Every place a u32/i32 value (or an element of a u32/i32 array) equals `v`, as paths.
pub fn find_value(r: &EsfRecord, v: i64, path: &str, out: &mut Vec<String>) {
    let here = format!("{path}/{}", r.name);
    find_in(&r.children, v, &here, out);
}

fn find_in(nodes: &[EsfNode], v: i64, path: &str, out: &mut Vec<String>) {
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(x) => find_value(x, v, path, out),
            EsfNode::RecordArray(a) => {
                for (k, item) in a.items.iter().enumerate() {
                    find_in(item, v, &format!("{path}/{}[{k}]", a.name), out);
                }
            }
            other => {
                if other.as_int() == Some(v) {
                    out.push(format!("{path} #{i}"));
                } else if let Some(a) = other.as_u32_array() {
                    if a.iter().any(|&x| i64::from(x) == v) {
                        out.push(format!("{path} #{i} (u32 array)"));
                    }
                } else if let Some(a) = other.as_i32_array()
                    && a.iter().any(|&x| i64::from(x) == v)
                {
                    out.push(format!("{path} #{i} (i32 array)"));
                }
            }
        }
    }
}

/// Paths of every NaN float (with its bits).
fn find_nan(nodes: &[EsfNode], path: &str, out: &mut Vec<String>) {
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(x) => find_nan(&x.children, &format!("{path}/{}", x.name), out),
            EsfNode::RecordArray(a) => {
                for (k, item) in a.items.iter().enumerate() {
                    find_nan(item, &format!("{path}/{}[{k}]", a.name), out);
                }
            }
            EsfNode::F32(f) if f.is_nan() => out.push(format!("{path} #{i} bits {:#010x}", f.to_bits())),
            EsfNode::Coord2d(a, b) if a.is_nan() || b.is_nan() => out.push(format!("{path} #{i} coord2d")),
            EsfNode::F32Array(v) if v.iter().any(|f| f.is_nan()) => out.push(format!("{path} #{i} f32 array")),
            _ => {}
        }
    }
}

/// Ids of characters, forces and units in OLD that NEW no longer has, and every place NEW still
/// holds one of those values (paths with indices removed, counted).
fn orphans(old: &EsfFile, new: &EsfFile) {
    let (oc, of, ou) = ids(old);
    let (nc, nf, nu) = ids(new);
    let mut gone: Vec<(String, i64)> = Vec::new();
    gone.extend(oc.difference(&nc).map(|&v| ("character".to_string(), i64::from(v))));
    gone.extend(of.difference(&nf).map(|&v| ("force".to_string(), i64::from(v))));
    gone.extend(ou.difference(&nu).map(|&v| ("unit".to_string(), i64::from(v))));
    println!("{} ids gone", gone.len());
    let set: std::collections::BTreeMap<i64, String> = gone.iter().map(|(k, v)| (*v, k.clone())).collect();
    let mut hits: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut walk_vals = Vec::new();
    collect_ints(&new.root.children, &new.root.name, &mut walk_vals);
    for (path, v) in walk_vals {
        if let Some(kind) = set.get(&v) {
            let p: String = path.split('[').map(|s| s.split_once(']').map_or(s, |x| x.1)).collect::<Vec<_>>().join("[]");
            *hits.entry(format!("{kind}: {p}")).or_default() += 1;
        }
    }
    for (k, n) in hits {
        println!("  {n:5}  {k}");
    }
}

fn collect_ints(nodes: &[EsfNode], path: &str, out: &mut Vec<(String, i64)>) {
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(x) => collect_ints(&x.children, &format!("{path}/{}", x.name), out),
            EsfNode::RecordArray(a) => {
                for (k, item) in a.items.iter().enumerate() {
                    collect_ints(item, &format!("{path}/{}[{k}]", a.name), out);
                }
            }
            other => {
                if let Some(v) = other.as_int() {
                    out.push((format!("{path} #{i}"), v));
                } else if let Some(a) = other.as_u32_array() {
                    out.extend(a.iter().map(|&x| (format!("{path} #{i}[u32]"), i64::from(x))));
                } else if let Some(a) = other.as_i32_array() {
                    out.extend(a.iter().map(|&x| (format!("{path} #{i}[i32]"), i64::from(x))));
                }
            }
        }
    }
}

/// Which characters have a pathfinder `CHARACTER_OBSTACLE`, by type and whether they command a
/// force or sit in a settlement.
fn obstacles(esf: &EsfFile) {
    let Some(ol) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()).and_then(|i| i.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))) else {
        println!("no OBSTACLE_LISTS");
        return;
    };
    let ids: Vec<u32> = ol.get(2).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    let mut garrisons = BTreeSet::new();
    esf.root.walk(&mut |r| {
        if r.name == "SIEGEABLE_GARRISON_RESIDENCE"
            && let Some(f) = r.get_u32(12).filter(|&f| f != 0)
        {
            garrisons.insert(f);
        }
    });
    let mut counts: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    for f in factions(esf) {
        for c in chars(f) {
            let force = c.rec.get_u32(4).unwrap_or(0);
            let state = if force == 0 { "no force" } else if garrisons.contains(&force) { "garrison" } else { "field" };
            let key = format!("{} / {state}", c.rec.get_str(3).unwrap_or(""));
            let e = counts.entry(key).or_default();
            e.0 += 1;
            if ids.contains(&(c.id as u32)) {
                e.1 += 1;
            }
        }
    }
    println!("{} obstacles", ids.len());
    for (k, (n, o)) in counts {
        println!("  {k:40} {n:4} characters, {o:4} with an obstacle");
    }
}

/// How the pathfinder's boundary lists refer to obstacles: every (u32, u32) pair in
/// `OBSTACLE_BOUNDARY_MANAGER` and `OBSTACLE_BASE_GRID_NODE`, split by low tag bits, and whether
/// the untagged value is a character obstacle's id.
fn obstacle_refs(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let rec = |name: &str| grid.iter().find_map(|n| n.as_record().filter(|r| r.name == name));
    let arr = |name: &str| grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == name));
    let ol = rec("OBSTACLE_LISTS").unwrap();
    let ids: BTreeSet<u32> = ol.get(2).and_then(EsfNode::as_u32_array).unwrap_or_default().iter().copied().collect();
    let mut stats: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut tally = |what: &str, a: &[u32]| {
        for p in a.chunks(2) {
            let tag = p[0] & 3;
            let known = ids.contains(&(p[0] & !3));
            *stats.entry(format!("{what}: tag {tag}, character obstacle {known}, second {}", p.get(1).map_or(9, |v| (*v).min(9)))).or_default() += 1;
        }
    };
    if let Some(m) = rec("OBSTACLE_BOUNDARY_MANAGER") {
        for b in m.record_array("OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()) {
            for n in b {
                if let Some(a) = n.as_u32_array() {
                    tally("boundary manager", a);
                }
            }
        }
    }
    if let Some(nodes) = arr("OBSTACLE_BASE_GRID_NODE") {
        for item in &nodes.items {
            for n in item {
                if let EsfNode::RecordArray(a) = n {
                    for mob in &a.items {
                        for x in mob {
                            if let EsfNode::Record(r) = x {
                                for v in &r.children {
                                    if let Some(a) = v.as_u32_array() {
                                        tally("grid node", a);
                                    }
                                }
                            } else if let Some(a) = x.as_u32_array() {
                                tally("grid node", a);
                            }
                        }
                    }
                }
            }
        }
    }
    for list in ol.children.iter().filter_map(EsfNode::as_record_array) {
        println!("OBSTACLE_LISTS/{}: {} items", list.name, list.items.len());
        for item in &list.items {
            for n in item {
                if let EsfNode::Record(o) = n
                    && let Some(m) = o.record_array("MANAGED_OBSTACLE_BOUNDARY")
                {
                    for mob in &m.items {
                        for x in mob {
                            if let Some(a) = x.as_u32_array() {
                                tally(&format!("{} own", list.name), a);
                            }
                        }
                    }
                }
            }
        }
    }
    for (k, n) in stats {
        println!("  {n:7}  {k}");
    }
}

/// Prints `count` values from `offset` of the `index`-th child (a u32 array) of the record at `path`.
fn u32s(esf: &EsfFile, path: &str, index: usize, offset: usize, count: usize) {
    let Some(r) = esf.root.find_path(path) else { return println!("no {path}") };
    let Some(a) = r.get(index).and_then(EsfNode::as_u32_array) else { return println!("#{index} is not a u32 array") };
    println!("{} values", a.len());
    for (i, v) in a.iter().enumerate().skip(offset).take(count) {
        println!("  [{i}] {v} ({v:#010x})");
    }
}

/// Every queued recruitment item: region, owner, the inner record's #0/#1/#8/#11, and the owner
/// faction's #38/#39 plain values.
fn recruits(esf: &EsfFile) {
    let Some(w) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return };
    let mut fac: std::collections::BTreeMap<u32, (String, Vec<String>)> = Default::default();
    for f in factions(esf) {
        let id = f.values().find_map(EsfNode::as_i32).unwrap_or(0) as u32;
        let tail: Vec<String> = [38usize, 39].iter().map(|&i| f.get(i).map(short).unwrap_or_default()).collect();
        fac.insert(id, (fkey(f), tail));
    }
    for r in w.child("REGION_MANAGER").and_then(|m| m.record_array("REGIONS_ARRAY")).into_iter().flat_map(|a| a.records()) {
        let rid = r.get_i32(4).unwrap_or(0);
        let owner = r.get_u32(20).unwrap_or(0);
        r.walk(&mut |x| {
            if x.name == "RECRUITMENT_ITEM" && x.get_i32(0).is_some() {
                println!(
                    "region {} {rid} owner {:?}: #0 {:?} #1 {:?} #8 {:?} #11 {:?} {:?}",
                    r.get_str(0).unwrap_or(""),
                    fac.get(&owner),
                    x.get_i32(0),
                    x.get_i32(1),
                    x.get_u32(8),
                    x.get_u32(11),
                    x.get_str(6)
                );
            }
        });
    }
}

/// The plain values of every record named `name` (first `max`).
fn records(esf: &EsfFile, name: &str, max: usize) {
    let mut n = 0;
    esf.root.walk(&mut |x| {
        if x.name == name {
            n += 1;
            if n <= max {
                let vals: Vec<String> = x.children.iter().map(|c| match c {
                    EsfNode::Record(r) => format!("{{{}}}", r.name),
                    EsfNode::RecordArray(a) => format!("[{}; {}]", a.name, a.items.len()),
                    other => short(other),
                }).collect();
                println!("  v{} {}", x.version, vals.join(", "));
            }
        }
    });
    println!("{n} {name} records");
}

/// Every place outside the defining records where a character, force or unit id of the file
/// appears (paths with array indexes stripped, with hit counts): the reference sites a writer
/// must keep consistent when objects appear or disappear.
fn refsites(esf: &EsfFile) {
    let (c, fo, u) = ids(esf);
    let mut set: std::collections::BTreeMap<i64, &str> = std::collections::BTreeMap::new();
    set.extend(c.iter().map(|&v| (i64::from(v), "character")));
    set.extend(fo.iter().map(|&v| (i64::from(v), "force")));
    set.extend(u.iter().map(|&v| (i64::from(v), "unit")));
    set.remove(&0);
    let mut vals = Vec::new();
    collect_ints(&esf.root.children, &esf.root.name, &mut vals);
    let mut hits: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (path, v) in vals {
        // Also ids tagged in their low bits (obstacle pairs: id | 2).
        let kind = set.get(&v).copied().or_else(|| set.get(&(v & !3)).filter(|_| v & 3 != 0).map(|_| "id|tag"));
        if let Some(kind) = kind {
            let p: String = path.split('[').map(|s| s.split_once(']').map_or(s, |x| x.1)).collect::<Vec<_>>().join("[]");
            *hits.entry(format!("{kind:9} {p}")).or_default() += 1;
        }
    }
    for (k, n) in hits {
        println!("{n:6}  {k}");
    }
}

/// A path with array indexes removed (`A/B[3]/C` -> `A/B[]/C`).
fn norm(path: &str) -> String {
    path.split('[').map(|s| s.split_once(']').map_or(s, |x| x.1)).collect::<Vec<_>>().join("[]")
}

/// Structural differences between two trees, summarised by normalised path: count and the first
/// `examples` instances (value differences, version / length differences, array lengths).
fn diff(a: &EsfFile, b: &EsfFile, examples: usize) {
    let mut hits: std::collections::BTreeMap<String, (usize, Vec<String>)> = std::collections::BTreeMap::new();
    let mut add = |key: String, ex: String| {
        let e = hits.entry(key).or_default();
        e.0 += 1;
        if e.1.len() < examples {
            e.1.push(ex);
        }
    };
    fn rec(a: &EsfRecord, b: &EsfRecord, path: &str, add: &mut dyn FnMut(String, String)) {
        let here = format!("{path}/{}", a.name);
        if a.name != b.name || a.version != b.version {
            add(format!("{} RECORD {} v{} -> {} v{}", norm(&here), a.name, a.version, b.name, b.version), here.clone());
        }
        if a.name != b.name {
            return;
        }
        if a.children.len() != b.children.len() {
            add(format!("{} CHILD COUNT", norm(&here)), format!("{here}: {} -> {}", a.children.len(), b.children.len()));
        }
        nodes(&a.children, &b.children, &here, add);
    }
    fn nodes(a: &[EsfNode], b: &[EsfNode], path: &str, add: &mut dyn FnMut(String, String)) {
        for (i, (x, y)) in a.iter().zip(b).enumerate() {
            match (x, y) {
                (EsfNode::Record(p), EsfNode::Record(q)) => rec(p, q, path, add),
                (EsfNode::RecordArray(p), EsfNode::RecordArray(q)) => {
                    let here = format!("{path}/{}", p.name);
                    if p.name != q.name || p.version != q.version {
                        add(format!("{} ARRAY {} v{} -> {} v{}", norm(&here), p.name, p.version, q.name, q.version), here.clone());
                    }
                    if p.items.len() != q.items.len() {
                        add(format!("{}[] LENGTH", norm(&here)), format!("{here}: {} -> {}", p.items.len(), q.items.len()));
                    }
                    for (k, (u, v)) in p.items.iter().zip(&q.items).enumerate() {
                        let ip = format!("{here}[{k}]");
                        if u.len() != v.len() {
                            add(format!("{} ITEM LENGTH", norm(&ip)), format!("{ip}: {} -> {}", u.len(), v.len()));
                        }
                        nodes(u, v, &ip, add);
                    }
                }
                (EsfNode::F32(a), EsfNode::F32(b)) if a.to_bits() == b.to_bits() => {}
                (p, q) => {
                    if p != q {
                        let (sp, sq) = (short(p), short(q));
                        add(format!("{} #{i}", norm(path)), format!("{path} #{i}: {sp} -> {sq}"));
                    }
                }
            }
        }
    }
    rec(&a.root, &b.root, "", &mut add);
    for (k, (n, ex)) in hits {
        println!("{n:7}  {k}");
        for e in ex {
            println!("           {e}");
        }
    }
}

/// Every record named `name` with its path and values (first `max`).
fn paths(esf: &EsfFile, name: &str, max: usize) {
    let mut n = 0;
    fn go(r: &EsfRecord, path: &str, name: &str, n: &mut usize, max: usize) {
        let here = format!("{path}/{}", r.name);
        if r.name == name {
            *n += 1;
            if *n <= max {
                let vals: Vec<String> = r.children.iter().map(|c| match c {
                    EsfNode::Record(x) => format!("{{{}}}", x.name),
                    EsfNode::RecordArray(a) => format!("[{}; {}]", a.name, a.items.len()),
                    other => short(other),
                }).collect();
                println!("{here} v{}: {}", r.version, vals.join(", "));
            }
        }
        for c in &r.children {
            match c {
                EsfNode::Record(x) => go(x, &here, name, n, max),
                EsfNode::RecordArray(a) => {
                    for (k, it) in a.items.iter().enumerate() {
                        for x in it.iter().filter_map(EsfNode::as_record) {
                            go(x, &format!("{here}/{}[{k}]", a.name), name, n, max);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    go(&esf.root, "", name, &mut n, max);
    println!("{n} {name} records");
}

/// For every scalar field of the `FACTION` records (and their direct child records, one level),
/// the factions whose value differs from the most common value: finds per-faction flags such as
/// "human".
fn faction_fields(esf: &EsfFile) {
    let fs = factions(esf);
    let mut table: std::collections::BTreeMap<String, Vec<(String, String)>> = std::collections::BTreeMap::new();
    for f in &fs {
        let key = fkey(f);
        let mut put = |path: String, n: &EsfNode| {
            if matches!(n, EsfNode::Bool(_) | EsfNode::U8(_) | EsfNode::I32(_) | EsfNode::U32(_) | EsfNode::Utf16String(_) | EsfNode::F32(_)) {
                table.entry(path).or_default().push((key.clone(), short(n)));
            }
        };
        for (i, c) in f.children.iter().enumerate() {
            match c {
                EsfNode::Record(r) => {
                    for (j, d) in r.children.iter().enumerate() {
                        put(format!("#{i} {}#{j}", r.name), d);
                    }
                }
                other => put(format!("#{i}"), other),
            }
        }
    }
    for (path, vals) in table {
        let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for (_, v) in &vals {
            *counts.entry(v.as_str()).or_default() += 1;
        }
        if counts.len() < 2 || counts.len() > 4 {
            continue;
        }
        let common = counts.iter().max_by_key(|x| x.1).map(|x| *x.0).unwrap_or("");
        let odd: Vec<String> = vals.iter().filter(|(_, v)| v != common).map(|(k, v)| format!("{k}={v}")).collect();
        if odd.len() <= 3 {
            println!("{path}: common {common}; {}", odd.join(", "));
        }
    }
}

/// The size of each child of `path` (serialised alone; approximate) and its cumulative share of
/// the whole file: where a byte-based loading bar stands while each block is read.
fn layout(esf: &EsfFile, path: &str) {
    let total = esf.to_bytes().map(|b| b.len()).unwrap_or(1) as f64;
    let size = |r: &EsfRecord| EsfFile::new(r.clone()).to_bytes().map(|b| b.len()).unwrap_or(0);
    let mut start = 0usize;
    fn offset_of(root: &EsfRecord, path: &str, size: &dyn Fn(&EsfRecord) -> usize) -> usize {
        // Bytes before `path` inside the root (sum of earlier siblings along the path).
        let mut off = 0;
        let mut cur = root;
        for part in path.split('/').filter(|s| !s.is_empty()) {
            let mut next = None;
            for c in &cur.children {
                match c {
                    EsfNode::Record(r) if r.name == part => {
                        next = Some(&**r);
                        break;
                    }
                    EsfNode::Record(r) => off += size(r),
                    EsfNode::RecordArray(a) => off += a.items.iter().flatten().filter_map(EsfNode::as_record).map(size).sum::<usize>(),
                    _ => off += 5,
                }
            }
            match next {
                Some(n) => cur = n,
                None => break,
            }
        }
        off
    }
    start += offset_of(&esf.root, path, &size);
    let Some(rec) = (if path.is_empty() { Some(&esf.root) } else { esf.root.find_path(path) }) else { return };
    for c in &rec.children {
        let (name, n) = match c {
            EsfNode::Record(r) => (r.name.clone(), size(r)),
            EsfNode::RecordArray(a) => (format!("{}[{}]", a.name, a.items.len()), a.items.iter().flatten().filter_map(EsfNode::as_record).map(size).sum()),
            _ => continue,
        };
        println!("{:6.2}% .. {:6.2}%  {:10}  {name}", start as f64 * 100.0 / total, (start + n) as f64 * 100.0 / total, n);
        start += n;
    }
}

/// The scalar fields of force commanders (`CHARACTER` and its `LOCOMOTABLE`), split by whether
/// their army is garrisoned (`ARMY` #5 != 0), field army or navy: value counts per field, to find
/// what marks a character as inside a settlement.
fn commander_fields(esf: &EsfFile) {
    let mut table: std::collections::BTreeMap<(String, String), std::collections::BTreeMap<String, usize>> = std::collections::BTreeMap::new();
    for f in factions(esf) {
        let chars = chars(f);
        for r in forces(f) {
            let kind = if r.name == "NAVY" { "navy" } else if r.get_u32(5).unwrap_or(0) != 0 { "garrison" } else { "field" };
            let cmd = r.child("MILITARY_FORCE").and_then(|m| m.get_u32(1)).unwrap_or(0);
            let Some(c) = chars.iter().find(|c| c.id as u32 == cmd) else { continue };
            let mut put = |field: String, n: &EsfNode| {
                if matches!(n, EsfNode::Bool(_) | EsfNode::U8(_) | EsfNode::I32(_) | EsfNode::U32(_) | EsfNode::F32(_)) {
                    let v = match n {
                        EsfNode::I32(x) if x.abs() > 100_000 => "big".to_string(),
                        EsfNode::U32(x) if *x > 100_000 => "big".to_string(),
                        other => short(other),
                    };
                    *table.entry((field, kind.to_string())).or_default().entry(v).or_default() += 1;
                }
            };
            for (i, n) in c.rec.children.iter().enumerate() {
                put(format!("CHARACTER #{i:02}"), n);
            }
            if let Some(l) = c.rec.children.first().and_then(EsfNode::as_record) {
                for (i, n) in l.children.iter().enumerate() {
                    put(format!("LOCOMOTABLE #{i:02}"), n);
                }
            }
        }
    }
    let mut last = String::new();
    for ((field, kind), vals) in table {
        if field != last {
            println!("{field}");
            last = field.clone();
        }
        let mut v: Vec<_> = vals.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1));
        let s: Vec<String> = v.iter().take(5).map(|(k, n)| format!("{k} x{n}")).collect();
        println!("    {kind:8} {}", s.join(", "));
    }
}

/// Dumps the record at `path` (`A/B[3]/C` syntax of `find_path`) to `depth` levels.
fn show(esf: &EsfFile, path: &str, depth: usize) {
    match esf.root.find_path(path) {
        Some(r) => {
            println!("{{{}}} v{}", r.name, r.version);
            dump(r, 1, depth);
        }
        None => println!("no {path}"),
    }
}

/// Every place `v` appears, counted by normalised path (array indexes removed).
fn findall(esf: &EsfFile, v: i64, under: &str) {
    let mut out = Vec::new();
    let root = if under.is_empty() { &esf.root } else { esf.root.find_path(under).expect("path") };
    find_value(root, v, "", &mut out);
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for p in out {
        *counts.entry(norm(&p)).or_default() += 1;
    }
    for (p, n) in counts {
        println!("{n:6}  {p}");
    }
}

/// The items of the record array `name` directly under `path`: for each, its records (name, version,
/// serialised size) and leading plain values.
fn items(esf: &EsfFile, path: &str, name: &str) {
    let Some(r) = esf.root.find_path(path) else { return println!("no {path}") };
    let Some(a) = r.record_array(name) else { return println!("no array {name}") };
    let size = |r: &EsfRecord| EsfFile::new(r.clone()).to_bytes().map(|b| b.len()).unwrap_or(0);
    for (k, it) in a.items.iter().enumerate() {
        let parts: Vec<String> = it
            .iter()
            .take(6)
            .map(|n| match n {
                EsfNode::Record(x) => format!("{{{} v{} {}b}}", x.name, x.version, size(x)),
                EsfNode::RecordArray(x) => format!("[{}; {}]", x.name, x.items.len()),
                other => short(other),
            })
            .collect();
        println!("[{k}] ({} nodes) {}", it.len(), parts.join(", "));
    }
}

/// Dumps item `k` of the record array `name` under `path`.
fn item(esf: &EsfFile, path: &str, name: &str, k: usize, depth: usize) {
    let Some(r) = esf.root.find_path(path) else { return println!("no {path}") };
    let Some(a) = r.record_array(name) else { return println!("no array {name}") };
    let Some(it) = a.items.get(k) else { return println!("no item {k}") };
    for (i, n) in it.iter().enumerate() {
        match n {
            EsfNode::Record(x) => {
                println!("#{i} {{{}}} v{}", x.name, x.version);
                dump(x, 1, depth);
            }
            other => println!("#{i} {}", short(other)),
        }
    }
}

/// The international trade routes: per owner faction, each route record's values.
fn trade_routes(esf: &EsfFile, max: usize) {
    let Some(t) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER") else { return };
    let mut n = 0;
    for it in t.record_array("INTERNATIONAL_TRADE_ROUTES").into_iter().flat_map(|a| a.items.iter()) {
        let owner = it.first().and_then(EsfNode::as_str).unwrap_or("");
        for a in it.iter().filter_map(EsfNode::as_record_array) {
            for r in a.records() {
                n += 1;
                if n <= max {
                    let vals: Vec<String> = r.children.iter().enumerate().map(|(i, c)| format!("#{i} {}", short(c))).collect();
                    println!("{owner}: {} v{}: {}", r.name, r.version, vals.join(" | "));
                }
            }
        }
    }
    println!("{n} routes");
}

/// Value counts of the trailing children (from `from`) of every record named `name`, by version.
fn tails(esf: &EsfFile, name: &str, from: usize) {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    esf.root.walk(&mut |r| {
        if r.name == name {
            let t: Vec<String> = r.children.iter().skip(from).map(|c| match c {
                EsfNode::Record(x) => format!("{{{}}}", x.name),
                other => short(other),
            }).collect();
            *counts.entry(format!("v{} {}", r.version, t.join(" | "))).or_default() += 1;
        }
    });
    for (k, n) in counts {
        println!("{n:6}  {k}");
    }
}

/// Characters whose v14 #39 is set: the key there, their type and the key of the unit they are
/// attached to (#5).
fn char39(esf: &EsfFile) {
    let mut unit_key: std::collections::BTreeMap<i32, String> = std::collections::BTreeMap::new();
    esf.root.walk(&mut |r| {
        if r.name == "UNIT"
            && let (Some(id), Some(k)) = (r.get_i32(4), r.child("UNIT_RECORD_KEY").and_then(|k| k.get_str(0)))
        {
            unit_key.insert(id, k.to_string());
        }
    });
    let (mut gen_units, mut set) = (0, 0);
    esf.root.walk(&mut |r| {
        if r.name != "CHARACTER" {
            return;
        }
        let k39 = r.get_str(39).unwrap_or("");
        let uk = unit_key.get(&(r.get_u32(5).unwrap_or(0) as i32)).cloned().unwrap_or_default();
        let unique = uk.starts_with("Gen_") && uk != "Gen_Generals_Staff" && !uk.contains("Generals");
        gen_units += usize::from(unique);
        set += usize::from(!k39.is_empty());
        if !k39.is_empty() || unique {
            println!("{:?} #39={k39:?} unit={uk:?}", r.get_str(3).unwrap_or(""));
        }
    });
    println!("{set} with #39, {gen_units} attached to a unique general unit");
}

/// Byte share of each faction in `WORLD/FACTION_ARRAY` and of its big children (approximate, as
/// `layout`): where a byte-based loading bar stands while each faction is read.
fn faction_layout(esf: &EsfFile) {
    let total = esf.to_bytes().map(|b| b.len()).unwrap_or(1) as f64;
    let size = |r: &EsfRecord| EsfFile::new(r.clone()).to_bytes().map(|b| b.len()).unwrap_or(0);
    let model = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL").unwrap();
    // Everything before WORLD: header, setup, the model's first children.
    let mut start = 0usize;
    for c in &esf.root.children {
        match c {
            EsfNode::Record(r) if r.name == "CAMPAIGN_ENV" => {
                for d in &r.children {
                    match d {
                        EsfNode::Record(m) if m.name == "CAMPAIGN_MODEL" => break,
                        EsfNode::Record(m) => start += size(m),
                        _ => {}
                    }
                }
                break;
            }
            EsfNode::Record(r) => start += size(r),
            _ => {}
        }
    }
    for c in &model.children {
        match c {
            EsfNode::Record(m) if m.name == "WORLD" => break,
            EsfNode::Record(m) => start += size(m),
            _ => {}
        }
    }
    let w = model.child("WORLD").unwrap();
    if let Some(a) = w.child("ANCILLARY_UNIQUENESS_MONITOR") {
        start += size(a);
    }
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        let n = size(f);
        println!("{:6.2}% .. {:6.2}%  {:10}  {key}", start as f64 * 100.0 / total, (start + n) as f64 * 100.0 / total, n);
        start += n;
    }
}

/// Does each character obstacle sit where its character stands? The core cell range (`OBSTACLE`
/// #15..#18) widened by one round the character's cell (eur grid: origin (-410, -190), 2 units;
/// INFERRED from the boxes).
fn obstacle_pos(esf: &EsfFile) {
    let Some(ol) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()).and_then(|i| i.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))) else {
        return;
    };
    let mut pos: BTreeMap<u32, (f64, f64, String)> = BTreeMap::new();
    for f in factions(esf) {
        for c in chars(f) {
            if let Some(EsfNode::Record(l)) = c.rec.children.first() {
                let x = f64::from(l.get_i32(0).unwrap_or(0)) / 1_048_576.0;
                let z = f64::from(l.get_i32(1).unwrap_or(0)) / 1_048_576.0;
                pos.insert(c.id as u32, (x, z, c.rec.get_str(3).unwrap_or("").to_string()));
            }
        }
    }
    let (mut ok, mut off) = (0, 0);
    for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
        let Some(o) = it.first().and_then(EsfNode::as_record) else { continue };
        let Some(id) = it.get(1).and_then(EsfNode::as_u32) else { continue };
        let u = |i: usize| match o.children.get(i) { Some(EsfNode::U16(v)) => i64::from(*v), _ => -1 };
        let Some(&(x, z, ref kind)) = pos.get(&id) else {
            println!("  obstacle of missing character {id}");
            continue;
        };
        let (col, row) = (((x + 410.0) / 2.0).floor() as i64, ((z + 190.0) / 2.0).floor() as i64);
        if col > u(15) && col < u(17) && row > u(16) && row < u(18) {
            ok += 1;
        } else {
            off += 1;
            println!("  {kind} {id} at ({x:.1}, {z:.1}) cell ({col}, {row}); core cells {}..{} x {}..{}", u(15), u(17), u(16), u(18));
        }
    }
    println!("{ok} obstacles at their character, {off} elsewhere");
}

/// The full tree of the character obstacles of `ids` (or the first of each mode state #4 when none).
fn obstacle_dump(esf: &EsfFile, ids: &[u32]) {
    let Some(ol) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()).and_then(|i| i.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS"))) else {
        return;
    };
    fn dump(n: &EsfNode, depth: usize) {
        let pad = "  ".repeat(depth);
        match n {
            EsfNode::Record(r) => {
                println!("{pad}{} v{}", r.name, r.version);
                for c in &r.children {
                    dump(c, depth + 1);
                }
            }
            EsfNode::RecordArray(a) => {
                println!("{pad}{}[{}]", a.name, a.items.len());
                for (i, it) in a.items.iter().enumerate() {
                    println!("{pad}  [{i}]");
                    for c in it {
                        dump(c, depth + 2);
                    }
                }
            }
            other => {
                let s = format!("{other:?}");
                println!("{pad}{}", if s.len() > 300 { format!("{}...", &s[..300]) } else { s });
            }
        }
    }
    let mut seen = BTreeSet::new();
    for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
        let Some(o) = it.first().and_then(EsfNode::as_record) else { continue };
        let Some(id) = it.get(1).and_then(EsfNode::as_u32) else { continue };
        let st = o.get_u32(4).unwrap_or(9);
        let want = if ids.is_empty() { seen.insert(st) } else { ids.contains(&id) };
        if want {
            println!("== obstacle of {id} (#4 = {st})");
            dump(&EsfNode::Record(Box::new(o.clone())), 1);
        }
    }
}

/// References to each obstacle boundary slot (pair second value) by the obstacle's mode state
/// (#4), in the boundary manager and the grid nodes; and the slot-1 BOUNDARIES lengths by state.
fn obstacle_slots(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let rec = |name: &str| grid.iter().find_map(|n| n.as_record().filter(|r| r.name == name));
    let ol = rec("OBSTACLE_LISTS").unwrap();
    let mut state: BTreeMap<u32, u32> = BTreeMap::new();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
        let (Some(o), Some(id)) = (it.first().and_then(EsfNode::as_record), it.get(1).and_then(EsfNode::as_u32)) else { continue };
        let cleared = (15..=18).all(|i| matches!(o.children.get(i), Some(EsfNode::U16(0))));
        let st = if cleared { 7 } else { o.get_u32(4).unwrap_or(9) };
        state.insert(id, st);
        if let Some(b) = o.record_array("BOUNDARIES") {
            for (k, item) in b.items.iter().enumerate().take(2) {
                let n = item.first().and_then(EsfNode::as_u32_array).map_or(0, <[u32]>::len);
                *tally.entry(format!("state {st}: BOUNDARIES slot {k} {}", if n == 0 { "empty" } else { "non-empty" })).or_default() += 1;
            }
        }
    }
    let count = |what: &str, a: &[u32], tally: &mut BTreeMap<String, usize>| {
        for p in a.chunks(2) {
            if p[0] & 3 != 2 {
                continue;
            }
            let st = state.get(&(p[0] & !3)).copied().unwrap_or(9);
            *tally.entry(format!("state {st}: {what} refs slot {}", p.get(1).copied().unwrap_or(99))).or_default() += 1;
        }
    };
    if let Some(m) = rec("OBSTACLE_BOUNDARY_MANAGER") {
        for b in m.record_array("OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()) {
            for n in b {
                if let Some(a) = n.as_u32_array() {
                    count("boundary manager", a, &mut tally);
                }
            }
        }
    }
    if let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) {
        for item in &nodes.items {
            for n in item {
                if let EsfNode::RecordArray(a) = n {
                    for mob in &a.items {
                        for x in mob {
                            if let EsfNode::Record(r) = x {
                                for v in &r.children {
                                    if let Some(a) = v.as_u32_array() {
                                        count("grid node", a, &mut tally);
                                    }
                                }
                            } else if let Some(a) = x.as_u32_array() {
                                count("grid node", a, &mut tally);
                            }
                        }
                    }
                }
            }
        }
    }
    for (k, n) in tally {
        println!("{n:7} {k}");
    }
}

/// Every unit item of army `k` of faction `f` (array indexes), dumped to `depth`.
fn force_units(esf: &EsfFile, f: usize, k: usize, depth: usize) {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let Some(fac) = w.record_array("FACTION_ARRAY").and_then(|a| a.records().nth(f)) else { return };
    let Some(army) = fac.record_array("ARMY_ARRAY").and_then(|a| a.items.get(k)) else { return };
    for n in army {
        if let EsfNode::Record(x) = n {
            println!("{{{}}} v{}", x.name, x.version);
            dump(x, 1, depth);
        }
    }
}

/// Every `UNIT`'s history date with its #5..#16 plain values (to compare fresh units with others).
fn unit_fields(esf: &EsfFile) {
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    esf.root.walk(&mut |r: &EsfRecord| {
        if r.name != "UNIT" {
            return;
        }
        let date = r.child("UNIT_HISTORY").and_then(|h| h.child("DATE")).map(|d| format!("{}-{}-{}-{}", d.get_u32(0).unwrap_or(0), d.get_u32(1).unwrap_or(0), d.get_u32(2).unwrap_or(0), d.get_u32(3).unwrap_or(0))).unwrap_or_default();
        let v = |i: usize| r.children.get(i).map(short).unwrap_or_default();
        let key = format!("date {date}: #7 {} #13 {} #15 {} #16 {}", if v(7) == "I32(0)" { "0" } else { "n" }, v(13), if v(15) == "I32(-1)" { "-1" } else { "n" }, v(16));
        *tally.entry(key).or_default() += 1;
    });
    for (k, n) in tally {
        println!("{n:6} {k}");
    }
}

/// Each faction's own `CAMPAIGN_VICTORY_CONDITIONS` (in its `CAMPAIGN_PLAYER_SETUP`) when it
/// differs from the default, and the same in `CAMPAIGN_SETUP`.
fn victory(esf: &EsfFile) {
    let show = |r: &EsfRecord| r.children.iter().map(short).collect::<Vec<_>>().join(", ");
    let default = "[REGION_KEYS; 0], Bool(false), U32(24)";
    for f in factions(esf) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        f.walk(&mut |r: &EsfRecord| {
            if r.name == "CAMPAIGN_VICTORY_CONDITIONS" && !show(r).starts_with(default) {
                println!("faction {key}: {}", show(r));
            }
        });
    }
    if let Some(s) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_SETUP") {
        s.walk(&mut |r: &EsfRecord| {
            if r.name == "CAMPAIGN_PLAYER_SETUP"
                && let Some(v) = r.child("CAMPAIGN_VICTORY_CONDITIONS")
                && !show(v).starts_with(default)
            {
                println!("setup {}: {}", r.get_str(2).unwrap_or(""), show(v));
            }
        });
    }
}

/// Every `VICTORY_CONDITION_OPTIONS` array (path and full tree to `depth`).
fn victory_options(esf: &EsfFile, depth: usize) {
    fn find(r: &EsfRecord, path: &str, depth: usize) {
        let here = format!("{path}/{}", r.name);
        for c in &r.children {
            match c {
                EsfNode::Record(x) => find(x, &here, depth),
                EsfNode::RecordArray(a) => {
                    if a.name == "VICTORY_CONDITION_OPTIONS" {
                        println!("{here}/{} ({} items)", a.name, a.items.len());
                        for (k, it) in a.items.iter().enumerate() {
                            println!("  [{k}]");
                            for n in it {
                                match n {
                                    EsfNode::Record(x) => {
                                        println!("    {{{}}} v{}", x.name, x.version);
                                        dump(x, 3, depth);
                                    }
                                    EsfNode::RecordArray(b) => {
                                        for (j, bi) in b.items.iter().enumerate() {
                                            println!("    {}[{j}]", b.name);
                                            for y in bi.iter().filter_map(EsfNode::as_record) {
                                                println!("      {{{}}} v{}", y.name, y.version);
                                                dump(y, 4, depth);
                                            }
                                        }
                                    }
                                    other => println!("    {}", short(other)),
                                }
                            }
                        }
                    }
                    for it in &a.items {
                        for n in it {
                            if let EsfNode::Record(x) = n {
                                find(x, &here, depth);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    find(&esf.root, "", depth);
}

/// For each region whose owner (`REGION` #20) differs between A and B: the region's full dump in
/// both, written to `<out>_<n>_a.txt` / `_b.txt` (to diff).
fn region_owner_diff(a: &EsfFile, b: &EsfFile, out: &str) {
    let regions = |e: &EsfFile| -> Vec<EsfRecord> {
        e.root
            .find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/REGION_MANAGER")
            .and_then(|m| m.record_array("REGIONS_ARRAY"))
            .map(|a| a.records().cloned().collect())
            .unwrap_or_default()
    };
    let keys = |e: &EsfFile| -> BTreeMap<u32, String> {
        let w = e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
        let mut m = BTreeMap::new();
        let mut fs: Vec<&EsfRecord> = w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
        fs.extend(w.child("REBEL_FACTION").and_then(|r| r.child("FACTION")));
        for f in fs {
            let id = f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32).unwrap_or(0) as u32;
            m.insert(id, f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string());
        }
        m
    };
    let (ka, kb) = (keys(a), keys(b));
    let owner = |r: &EsfRecord, k: &BTreeMap<u32, String>| r.get_u32(20).and_then(|i| k.get(&i).cloned()).unwrap_or_default();
    let (ra, rb) = (regions(a), regions(b));
    let mut n = 0;
    for (x, y) in ra.iter().zip(&rb) {
        if owner(x, &ka) != owner(y, &kb) {
            n += 1;
            println!("region {n}: owner {} -> {}", owner(x, &ka), owner(y, &kb));
            for (r, s) in [(x, "a"), (y, "b")] {
                let mut buf = Vec::new();
                dump_to(r, 0, 12, &mut buf);
                std::fs::write(format!("{out}_{n}_{s}.txt"), buf.join("\n")).unwrap();
            }
        }
    }
}

fn dump_to(r: &EsfRecord, depth: usize, max: usize, out: &mut Vec<String>) {
    let pad = "  ".repeat(depth);
    out.push(format!("{pad}{{{}}} v{}", r.name, r.version));
    if depth >= max {
        return;
    }
    for (i, c) in r.children.iter().enumerate() {
        match c {
            EsfNode::Record(x) => {
                out.push(format!("{pad}  #{i}"));
                dump_to(x, depth + 1, max, out);
            }
            EsfNode::RecordArray(a) => {
                out.push(format!("{pad}  #{i} [{}] ({} items)", a.name, a.items.len()));
                for (k, it) in a.items.iter().enumerate() {
                    out.push(format!("{pad}    [{k}]"));
                    for n in it {
                        if let EsfNode::Record(x) = n {
                            dump_to(x, depth + 3, max, out);
                        } else {
                            out.push(format!("{pad}      {}", short(n)));
                        }
                    }
                }
            }
            other => out.push(format!("{pad}  #{i} {}", short(other))),
        }
    }
}

/// Per region: does every `GARRISON_RESIDENCE` #0 inside it (settlement and slots) name the region's
/// owner (`REGION` #20)? And do its buildings' #2 faction keys name the owner?
fn region_owners(esf: &EsfFile) {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let mut keys = BTreeMap::new();
    let mut fs: Vec<&EsfRecord> = w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    fs.extend(w.child("REBEL_FACTION").and_then(|r| r.child("FACTION")));
    for f in fs {
        let id = f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32).unwrap_or(0) as u32;
        keys.insert(id, f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string());
    }
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for r in w.child("REGION_MANAGER").and_then(|m| m.record_array("REGIONS_ARRAY")).into_iter().flat_map(|a| a.records()) {
        let owner = r.get_u32(20).unwrap_or(0);
        let okey = keys.get(&owner).cloned().unwrap_or_default();
        let mut res = (0, 0);
        let mut bld = (0, 0);
        r.walk(&mut |x: &EsfRecord| {
            if x.name == "GARRISON_RESIDENCE" {
                res.0 += 1;
                if x.get_u32(0) == Some(owner) {
                    res.1 += 1;
                }
            }
            if x.name == "BUILDING" {
                bld.0 += 1;
                if x.get_str(2) == Some(okey.as_str()) {
                    bld.1 += 1;
                }
            }
        });
        let k = format!("residences {} / buildings {}", if res.0 == res.1 { "all owner" } else { "MIXED" }, if bld.0 == bld.1 { "all owner" } else { "mixed" });
        *tally.entry(k).or_default() += 1;
        if res.0 != res.1 {
            println!("  {}: owner {okey}, residences {}/{} owner's, buildings {}/{}", r.get_str(0).unwrap_or(""), res.1, res.0, bld.1, bld.0);
        }
    }
    for (k, n) in tally {
        println!("{n:4} {k}");
    }
}

/// The `OBSTACLE_BOUNDARY_MANAGER/OBSTACLE_BOUNDARY[]` items and grid nodes that name obstacle
/// `id` (pairs (id | 2, slot)), printed in full (arrays shortened).
fn obstacle_owner_refs(esf: &EsfFile, id: u32) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let has = |a: &[u32]| a.chunks(2).any(|p| p[0] == (id | 2));
    let show = |n: &EsfNode| {
        let s = short(n);
        if s.len() > 200 { format!("{}...", &s[..200]) } else { s }
    };
    if let Some(m) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARY_MANAGER")) {
        println!("manager children: {}", m.children.iter().map(|c| match c { EsfNode::RecordArray(a) => format!("[{}; {}]", a.name, a.items.len()), o => show(o) }).collect::<Vec<_>>().join(", "));
        for (k, b) in m.record_array("OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()).enumerate() {
            if b.iter().any(|n| n.as_u32_array().is_some_and(has)) {
                println!("OBSTACLE_BOUNDARY[{k}]: {}", b.iter().map(show).collect::<Vec<_>>().join(" | "));
            }
        }
    }
}

/// The rebels' characters, forces and units and which of them the AI block mirrors.
fn rebel_mirrors(esf: &EsfFile) {
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return };
    let Some(f) = world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")) else { return };
    let mut mirrored: BTreeSet<u32> = BTreeSet::new();
    if let Some(cai) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE/CAI_WORLD") {
        cai.walk(&mut |r: &EsfRecord| match r.name.as_str() {
            "CAI_CHARACTER" => {
                mirrored.extend(r.get_u32(3));
            }
            "CAI_RESOURCE_MOBILE" => {
                mirrored.extend(r.get_u32(10));
            }
            "CAI_UNIT" => {
                mirrored.extend(r.get_u32(1));
            }
            _ => {}
        });
    }
    for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
        let id = c.get_i32(2).unwrap_or(0) as u32;
        println!("character {id} {} force {} mirrored {}", c.get_str(3).unwrap_or(""), c.get_u32(4).unwrap_or(0), mirrored.contains(&id));
    }
    for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
        let Some(m) = a.child("MILITARY_FORCE") else { continue };
        let id = m.get_u32(0).unwrap_or(0);
        let units: Vec<u32> = a.record_array("UNITS_ARRAY").into_iter().flat_map(|x| x.records()).filter_map(|x| x.child("UNIT")).map(|u| u.get_i32(4).unwrap_or(0) as u32).collect();
        let mu = units.iter().filter(|u| mirrored.contains(u)).count();
        println!("force {id} ({}) commander {} mirrored {}, units {} mirrored {mu}", a.name, m.get_u32(1).unwrap_or(0), mirrored.contains(&id), units.len());
    }
}

/// Consistency of the obstacle structures: each obstacle's managed slots vs the boundary manager's
/// entries vs the grid nodes' pairs vs its BOUNDARIES lists.
fn obstacle_consistency(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let rec = |name: &str| grid.iter().find_map(|n| n.as_record().filter(|r| r.name == name));
    let ol = rec("OBSTACLE_LISTS").unwrap();
    let mut managed: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut nonempty: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut owners = BTreeSet::new();
    for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
        let (Some(o), Some(id)) = (it.first().and_then(EsfNode::as_record), it.get(1).and_then(EsfNode::as_u32)) else { continue };
        owners.insert(id);
        if let Some(m) = o.record_array("MANAGED_OBSTACLE_BOUNDARY") {
            for (k, item) in m.items.iter().enumerate() {
                if item.first().and_then(EsfNode::as_bool) == Some(true) {
                    managed.insert((id, k as u32));
                    let pair = item.get(1).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
                    if pair != vec![id | 2, k as u32] {
                        println!("  obstacle {id}: managed slot {k} holds {pair:?}");
                    }
                }
            }
        }
        if let Some(b) = o.record_array("BOUNDARIES") {
            for (k, item) in b.items.iter().enumerate() {
                if item.first().and_then(EsfNode::as_u32_array).is_some_and(|v| !v.is_empty()) {
                    nonempty.insert((id, k as u32));
                }
            }
        }
    }
    let mut manager: Vec<(u32, u32)> = Vec::new();
    if let Some(m) = rec("OBSTACLE_BOUNDARY_MANAGER") {
        for b in m.record_array("OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()) {
            for n in b {
                if let Some(a) = n.as_u32_array() {
                    for p in a.chunks(2) {
                        manager.push((p[0] & !3, p.get(1).copied().unwrap_or(99)));
                    }
                }
            }
        }
    }
    let mset: BTreeSet<(u32, u32)> = manager.iter().copied().collect();
    let mut grid_pairs: BTreeSet<(u32, u32)> = BTreeSet::new();
    if let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) {
        for item in &nodes.items {
            for n in item {
                if let EsfNode::RecordArray(a) = n {
                    for mob in &a.items {
                        for x in mob {
                            let arrs: Vec<&[u32]> = match x {
                                EsfNode::Record(r) => r.children.iter().filter_map(EsfNode::as_u32_array).collect(),
                                other => other.as_u32_array().into_iter().collect(),
                            };
                            for a in arrs {
                                for p in a.chunks(2) {
                                    if p[0] & 3 == 2 {
                                        grid_pairs.insert((p[0] & !3, p.get(1).copied().unwrap_or(99)));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!(
        "{} obstacles; managed slots {}, manager entries {} ({} distinct), grid pairs {}, non-empty boundary slots {}",
        owners.len(),
        managed.len(),
        manager.len(),
        mset.len(),
        grid_pairs.len(),
        nonempty.len()
    );
    println!("  managed but not in manager: {:?}", managed.difference(&mset).take(5).collect::<Vec<_>>());
    println!("  manager entries not managed: {:?} ({})", mset.difference(&managed).take(5).collect::<Vec<_>>(), mset.difference(&managed).count());
    println!("  grid pairs not managed: {} {:?}", grid_pairs.difference(&managed).count(), grid_pairs.difference(&managed).take(5).collect::<Vec<_>>());
    println!("  grid pairs on an empty boundary slot: {} {:?}", grid_pairs.difference(&nonempty).count(), grid_pairs.difference(&nonempty).take(5).collect::<Vec<_>>());
    println!("  non-empty boundary slots without grid pairs: {}", nonempty.difference(&grid_pairs).count());
    let owners_m: BTreeSet<u32> = mset.iter().map(|p| p.0).collect();
    println!("  manager owners not obstacles: {}", owners_m.difference(&owners).count());
}

/// The plain (non 0x80000000) values of the obstacles' BOUNDARIES lists against the boundary
/// manager's item count: are they indexes of its pieces (plus a base)?
fn boundary_indexes(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let rec = |name: &str| grid.iter().find_map(|n| n.as_record().filter(|r| r.name == name));
    let ol = rec("OBSTACLE_LISTS").unwrap();
    let mut plain: BTreeSet<u32> = BTreeSet::new();
    let mut high = 0usize;
    for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
        let Some(o) = it.first().and_then(EsfNode::as_record) else { continue };
        for b in o.record_array("BOUNDARIES").into_iter().flat_map(|a| a.items.iter()) {
            for v in b.iter().filter_map(EsfNode::as_u32_array).flatten() {
                if v & 0x8000_0000 == 0 {
                    plain.insert(*v);
                } else {
                    high += 1;
                }
            }
        }
    }
    let n = rec("OBSTACLE_BOUNDARY_MANAGER").and_then(|m| m.record_array("OBSTACLE_BOUNDARY")).map_or(0, |a| a.items.len());
    let (lo, hi) = (plain.first().copied().unwrap_or(0), plain.last().copied().unwrap_or(0));
    println!("manager items {n}; plain values {} distinct, {lo}..={hi} (span {}); map-boundary values {high}", plain.len(), hi - lo + 1);
    // Grid-node items: do they hold plain values too?
    let mut gplain = BTreeSet::new();
    if let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) {
        for item in &nodes.items {
            for n in item {
                match n {
                    EsfNode::U32(v) => {
                        gplain.insert(*v);
                    }
                    EsfNode::RecordArray(a) => {
                        for mob in &a.items {
                            for x in mob {
                                if let EsfNode::U32(v) = x {
                                    gplain.insert(*v);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    println!("grid-node plain u32s: {} distinct, {:?}..{:?}", gplain.len(), gplain.first(), gplain.last());
}

/// The `PATHFINDING_GRID[0]` item's children (shape), and the first grid nodes in full.
fn grid_shape(esf: &EsfFile, nodes: usize) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    for (i, n) in grid.iter().enumerate() {
        match n {
            EsfNode::RecordArray(a) => println!("#{i} [{}] v{} ({} items)", a.name, a.version, a.items.len()),
            EsfNode::Record(r) => println!("#{i} {{{}}} v{}", r.name, r.version),
            EsfNode::U32Array(v) => println!("#{i} U32Array len {} first {:?}", v.len(), &v[..v.len().min(12)]),
            other => println!("#{i} {}", short(other)),
        }
    }
    if let Some(a) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) {
        for (k, it) in a.items.iter().enumerate().filter(|(_, it)| it.iter().any(|n| matches!(n, EsfNode::RecordArray(x) if !x.items.is_empty()))).take(nodes) {
            println!("node [{k}]");
            for n in it {
                match n {
                    EsfNode::RecordArray(x) => {
                        println!("  [{}] ({} items)", x.name, x.items.len());
                        for m in x.items.iter().take(4) {
                            println!("    {}", m.iter().map(short).collect::<Vec<_>>().join(" | "));
                        }
                    }
                    other => println!("  {}", short(other)),
                }
            }
        }
    }
}

/// Grid nodes: the lengths of their two `MANAGED_OBSTACLE_BOUNDARY` lists (equal?) and the second
/// U32 against them.
fn grid_lists(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let Some(a) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) else { return };
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for it in &a.items {
        let lens: Vec<usize> = it.iter().filter_map(|n| match n {
            EsfNode::RecordArray(x) => Some(x.items.len()),
            _ => None,
        }).collect();
        let shape: Vec<String> = it.iter().map(|n| match n {
            EsfNode::RecordArray(_) => "L".into(),
            EsfNode::U32(_) => "u".into(),
            other => short(other).chars().take(4).collect(),
        }).collect();
        let hdr: Vec<u32> = it.iter().filter_map(EsfNode::as_u32).collect();
        let k = format!("shape {} lists equal {} empty {} second {}", shape.join(","), lens.windows(2).all(|w| w[0] == w[1]), lens.first().is_some_and(|&l| l == 0), hdr.get(1).map_or(99, |&v| v.min(9)));
        *tally.entry(k).or_default() += 1;
    }
    for (k, n) in tally {
        println!("{n:7} {k}");
    }
}

/// Grid nodes side by side with the U32Array that follows `OBSTACLE_BASE_GRID_NODE` (pairs).
fn grid_pairs(esf: &EsfFile, from: usize, n: usize) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let pos = grid.iter().position(|x| x.as_record_array().is_some_and(|r| r.name == "OBSTACLE_BASE_GRID_NODE")).unwrap();
    let nodes = grid[pos].as_record_array().unwrap();
    let pairs = grid[pos + 1].as_u32_array().unwrap();
    println!("nodes {}, pair array {} (= 2 x nodes: {})", nodes.items.len(), pairs.len(), pairs.len() == 2 * nodes.items.len());
    for k in from..(from + n).min(nodes.items.len()) {
        let hdr: Vec<u32> = nodes.items[k].iter().filter_map(EsfNode::as_u32).collect();
        let l = nodes.items[k].iter().find_map(EsfNode::as_record_array).map_or(0, |a| a.items.len());
        let p = (pairs[2 * k], pairs[2 * k + 1]);
        println!("  [{k}] node {hdr:?} items {l} | pair ({:#x} = ({}, {}), {})", p.0, p.0 >> 16, p.0 & 0xffff, p.1);
    }
}

/// Is the pair array's second value a permutation of the node indexes? And the first values.
fn grid_pair_stats(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let pos = grid.iter().position(|x| x.as_record_array().is_some_and(|r| r.name == "OBSTACLE_BASE_GRID_NODE")).unwrap();
    let nodes = grid[pos].as_record_array().unwrap();
    let pairs = grid[pos + 1].as_u32_array().unwrap();
    let seconds: BTreeSet<u32> = pairs.chunks(2).map(|p| p[1]).collect();
    let firsts: BTreeSet<u32> = pairs.chunks(2).map(|p| p[0]).collect();
    let hdr0: BTreeSet<u32> = nodes.items.iter().filter_map(|it| it.first().and_then(EsfNode::as_u32)).collect();
    println!(
        "nodes {}; pair seconds {} distinct, {:?}..{:?}; firsts (cells) {} distinct; node first u32 {} distinct, {:?}..{:?}; sorted by cell {}",
        nodes.items.len(),
        seconds.len(),
        seconds.first(),
        seconds.last(),
        firsts.len(),
        hdr0.len(),
        hdr0.first(),
        hdr0.last(),
        pairs.chunks(2).map(|p| p[0]).collect::<Vec<_>>().windows(2).all(|w| w[0] <= w[1])
    );
    let grid_cells = grid.first().and_then(EsfNode::as_u32);
    println!("grid #0 {grid_cells:?}");
}

/// Grid-node list-1 items {u32 piece, u32 x, bool, pairs}: x against the pair count, and the
/// row-wise owner overlap of list 1 and list 2 items.
fn grid_items(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) else { return };
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for it in &nodes.items {
        let lists: Vec<&ntw_formats::esf::EsfRecordArray> = it.iter().filter_map(EsfNode::as_record_array).collect();
        if lists.len() != 2 {
            continue;
        }
        for (a, b) in lists[0].items.iter().zip(&lists[1].items) {
            let x = a.get(1).and_then(EsfNode::as_u32).unwrap_or(99);
            let pa = a.iter().find_map(EsfNode::as_u32_array).map_or(0, |v| v.len() / 2);
            let pb = b.iter().find_map(EsfNode::as_u32_array).map_or(0, |v| v.len() / 2);
            let oa: BTreeSet<u32> = a.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().chunks(2).map(|p| p[0]).collect();
            let ob: BTreeSet<u32> = b.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().chunks(2).map(|p| p[0]).collect();
            *tally.entry(format!("x {} pairs1 {} pairs2 {} owners shared {}", x.min(9), pa.min(9), pb.min(9), !oa.is_disjoint(&ob))).or_default() += 1;
        }
    }
    for (k, n) in tally {
        println!("{n:7} {k}");
    }
}

/// The boundary manager as a set of pair lists, against the pair lists the grid-node rows name
/// (the loader looks every row's list up there: 0x00AEE2E0 -> 0x00AECFF0 -> 0x00B53800).
fn manager_lists(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let mut mgr: BTreeMap<Vec<u32>, usize> = BTreeMap::new();
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(m) = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_BOUNDARY_MANAGER")) {
        for b in m.record_array("OBSTACLE_BOUNDARY").into_iter().flat_map(|a| a.items.iter()) {
            *shapes.entry(b.iter().map(|n| short(n).chars().take(8).collect::<String>()).collect::<Vec<_>>().join(",")).or_default() += 1;
            for n in b {
                if let Some(a) = n.as_u32_array() {
                    *mgr.entry(a.to_vec()).or_default() += 1;
                }
            }
        }
    }
    println!("manager items: {} lists, {} distinct; duplicates {}; empty {}; shapes {shapes:?}", mgr.values().sum::<usize>(), mgr.len(), mgr.values().filter(|&&n| n > 1).count(), mgr.contains_key(&Vec::new()));
    let mut used: BTreeSet<Vec<u32>> = BTreeSet::new();
    let (mut rows, mut missing, mut empty, mut flag_false) = (0, 0, 0, 0);
    let mut ex = Vec::new();
    if let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) {
        for it in &nodes.items {
            for l in it.iter().filter_map(EsfNode::as_record_array) {
                for row in &l.items {
                    rows += 1;
                    let flag = row.iter().find_map(EsfNode::as_bool);
                    if flag == Some(false) {
                        flag_false += 1;
                    }
                    let Some(v) = row.iter().find_map(EsfNode::as_u32_array) else { continue };
                    if v.is_empty() {
                        empty += 1;
                    }
                    if mgr.contains_key(v) {
                        used.insert(v.to_vec());
                    } else {
                        missing += 1;
                        if ex.len() < 5 {
                            ex.push(v.to_vec());
                        }
                    }
                }
            }
        }
    }
    println!("grid rows {rows}: flag false {flag_false}, empty lists {empty}, lists not in the manager {missing} {ex:?}; manager lists unused {}", mgr.len() - used.len());
}

/// Obstacle BOUNDARIES values (plain, and 0x80000000 | n) against the grid rows' piece field, and
/// which owners' pairs a row holds against which obstacles list that piece.
fn piece_refs(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let ol = grid.iter().find_map(|n| n.as_record().filter(|r| r.name == "OBSTACLE_LISTS")).unwrap();
    // piece -> obstacles (owner, slot) listing it
    let mut listed: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let (mut plain, mut high) = (0, 0);
    for it in ol.record_array("CHARACTER_OBSTACLE").into_iter().flat_map(|a| a.items.iter()) {
        let (Some(o), Some(id)) = (it.first().and_then(EsfNode::as_record), it.get(1).and_then(EsfNode::as_u32)) else { continue };
        for (k, b) in o.record_array("BOUNDARIES").into_iter().flat_map(|a| a.items.iter()).enumerate() {
            for &v in b.iter().filter_map(EsfNode::as_u32_array).flatten() {
                if v & 0x8000_0000 == 0 { plain += 1 } else { high += 1 }
                listed.entry(v).or_default().insert((id, k as u32));
            }
        }
    }
    let mut row_pieces: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    let mut row_pieces2: BTreeMap<u32, BTreeSet<(u32, u32)>> = BTreeMap::new();
    if let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) {
        for it in &nodes.items {
            let lists: Vec<&ntw_formats::esf::EsfRecordArray> = it.iter().filter_map(EsfNode::as_record_array).collect();
            for (li, l) in lists.iter().enumerate() {
                for (ri, row) in l.items.iter().enumerate() {
                    let piece = lists[0].items.get(ri).and_then(|r| r.first()).and_then(EsfNode::as_u32).unwrap_or(0);
                    let pairs: BTreeSet<(u32, u32)> = row.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().chunks(2).map(|p| (p[0] & !3, p[1])).collect();
                    let m = if li == 0 { &mut row_pieces } else { &mut row_pieces2 };
                    m.entry(piece).or_default().extend(pairs);
                }
            }
        }
    }
    let lp: BTreeSet<u32> = listed.keys().copied().collect();
    let rp: BTreeSet<u32> = row_pieces.keys().copied().collect();
    println!("BOUNDARIES values: {plain} plain, {high} high; distinct {}; row pieces distinct {}", lp.len(), rp.len());
    println!("  listed values that are row pieces: {}; with the high bit cleared: {}", lp.intersection(&rp).count(), lp.iter().filter(|v| rp.contains(&(*v & 0x7fff_ffff))).count());
    // Does the row's owner set equal the obstacles listing that piece?
    let (mut same1, mut same2, mut n) = (0, 0, 0);
    for (p, owners) in &row_pieces {
        let Some(l) = listed.get(p) else { continue };
        n += 1;
        if l == owners { same1 += 1 }
        if row_pieces2.get(p) == Some(l) { same2 += 1 }
    }
    // List 2 against the obstacles listing the reversed piece (0x80000000 | P).
    let (mut rev_same, mut rev_n, mut l1_high_same, mut l1_high_n) = (0, 0, 0, 0);
    for (p, owners2) in &row_pieces2 {
        let rev = listed.get(&(p | 0x8000_0000)).cloned().unwrap_or_default();
        rev_n += 1;
        if &rev == owners2 { rev_same += 1 }
        let fwd = listed.get(p).cloned().unwrap_or_default();
        let l1 = row_pieces.get(p).cloned().unwrap_or_default();
        l1_high_n += 1;
        if l1 == fwd { l1_high_same += 1 }
    }
    println!("  list-2 owners = obstacles listing 0x80000000|P: {rev_same}/{rev_n}; list-1 = listing P (all rows): {l1_high_same}/{l1_high_n}");
    {
    }
    println!("  pieces in both: {n}; list-1 owners = listing obstacles {same1}; list-2 owners = listing {same2}");
}

/// Slot numbers of the pairs in grid list 1 and list 2, and how a row's list-2 owners relate to
/// its list-1 owners.
fn list_slots(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) else { return };
    let mut t: BTreeMap<String, usize> = BTreeMap::new();
    for it in &nodes.items {
        let lists: Vec<&ntw_formats::esf::EsfRecordArray> = it.iter().filter_map(EsfNode::as_record_array).collect();
        if lists.len() != 2 { continue }
        for (a, b) in lists[0].items.iter().zip(&lists[1].items) {
            let pa: Vec<(u32, u32)> = a.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().chunks(2).map(|p| (p[0], p[1])).collect();
            let pb: Vec<(u32, u32)> = b.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().chunks(2).map(|p| (p[0], p[1])).collect();
            for (_, s) in &pa { *t.entry(format!("list1 slot {s}")).or_default() += 1; }
            for (_, s) in &pb { *t.entry(format!("list2 slot {s}")).or_default() += 1; }
            let oa: BTreeSet<u32> = pa.iter().map(|p| p.0).collect();
            let ob: BTreeSet<u32> = pb.iter().map(|p| p.0).collect();
            let rel = if oa == ob { "same owners" } else if ob.is_subset(&oa) { "list2 owners subset" } else if oa.is_subset(&ob) { "list2 owners superset" } else { "other" };
            let x = a.get(1).and_then(EsfNode::as_u32).unwrap_or(9);
            *t.entry(format!("{rel}, x {x}")).or_default() += 1;
        }
    }
    for (k, n) in t { println!("{n:7} {k}"); }
}

/// Per grid node: is list 2 (its rows' pair lists) a permutation of list 1's pair lists? And how
/// is it ordered (by list-1 row order, by the pair lists, ...)?
fn list_perm(esf: &EsfFile) {
    let Some(grid) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_PATHFINDER").and_then(|p| p.record_array("PATHFINDING_GRID")).and_then(|a| a.items.first()) else { return };
    let Some(nodes) = grid.iter().find_map(|n| n.as_record_array().filter(|r| r.name == "OBSTACLE_BASE_GRID_NODE")) else { return };
    let mut t: BTreeMap<String, usize> = BTreeMap::new();
    for it in &nodes.items {
        let lists: Vec<&ntw_formats::esf::EsfRecordArray> = it.iter().filter_map(EsfNode::as_record_array).collect();
        if lists.len() != 2 { continue }
        let get = |l: &ntw_formats::esf::EsfRecordArray| -> Vec<Vec<u32>> { l.items.iter().map(|r| r.iter().find_map(EsfNode::as_u32_array).unwrap_or_default().to_vec()).collect() };
        let (a, b) = (get(lists[0]), get(lists[1]));
        let (mut sa, mut sb) = (a.clone(), b.clone());
        sa.sort();
        sb.sort();
        let perm = sa == sb;
        let same_order = a == b;
        let b_sorted = b.windows(2).all(|w| w[0] <= w[1]);
        *t.entry(format!("permutation {perm}, identical {same_order}, list2 sorted {b_sorted}")).or_default() += 1;
    }
    for (k, n) in t { println!("{n:7} {k}"); }
}

/// Every string value that contains `needle`, with its path.
fn find_str(esf: &EsfFile, needle: &str) {
    fn walk(r: &EsfRecord, path: &str, needle: &str) {
        let here = format!("{path}/{}", r.name);
        for (i, c) in r.children.iter().enumerate() {
            match c {
                EsfNode::Record(x) => walk(x, &here, needle),
                EsfNode::RecordArray(a) => {
                    for (k, it) in a.items.iter().enumerate() {
                        for n in it {
                            match n {
                                EsfNode::Record(x) => walk(x, &format!("{here}/{}[{k}]", a.name), needle),
                                other => {
                                    if let Some(s) = other.as_str().filter(|s| s.contains(needle)) {
                                        println!("  {here}/{}[{k}]: {s}", a.name);
                                    }
                                }
                            }
                        }
                    }
                }
                other => {
                    if let Some(s) = other.as_str().filter(|s| s.contains(needle)) {
                        println!("  {here} #{i}: {s}");
                    }
                }
            }
        }
    }
    walk(&esf.root, "", needle);
}

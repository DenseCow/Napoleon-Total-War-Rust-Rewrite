//! Research tool for the campaign AI block (`CAI_INTERFACE`) of saves (read-only;
//! analysis/campaign/SAVE_COMPAT.md §5, §10): how the original keeps the block in step with the
//! world when characters, forces and units appear or disappear.
//!
//! ```text
//! cargo run --release -p ntw_campaign --example cai_audit -- mirrors FILE
//! cargo run --release -p ntw_campaign --example cai_audit -- pair OLD NEW
//! cargo run --release -p ntw_campaign --example cai_audit -- comp FILE ID
//! ```
//! * `mirrors`: per dynamic `CAI_WORLD` list (characters, resource mobiles, units), how many items,
//!   whether every world object has exactly one mirror, component id range.
//! * `pair`: objects gone and new between two saves of one game; whether the gone objects' mirror
//!   components still appear anywhere in NEW's AI block; where the new objects' components appear.
//! * `comp`: every place a component id appears in the AI block, by record path.

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

const CAI: &str = "CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE";

/// (list name, record name, index of the game id in that record).
const LISTS: [(&str, &str, usize); 3] =
    [("CAI_WORLD_CHARACTERS", "CAI_CHARACTER", 3), ("CAI_WORLD_RESOURCE_MOBILES", "CAI_RESOURCE_MOBILE", 10), ("CAI_WORLD_UNITS", "CAI_UNIT", 1)];

/// A mirror: game object id, component id, the item.
struct Mirror {
    game: u32,
    component: u32,
}

fn component_of(item: &[EsfNode]) -> Option<u32> {
    item.iter().find(|n| !matches!(n, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_u32)
}

fn mirrors(esf: &EsfFile, list: &str, rec: &str, idx: usize) -> Vec<Mirror> {
    let Some(w) = esf.root.find_path(&format!("{CAI}/CAI_WORLD")) else { return Vec::new() };
    let Some(a) = w.record_array(list) else { return Vec::new() };
    a.items
        .iter()
        .filter_map(|it| {
            let r = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == rec)?;
            Some(Mirror { game: r.get_u32(idx).unwrap_or(0), component: component_of(it)? })
        })
        .collect()
}

/// Game objects: characters, forces, units (ids).
fn world_ids(esf: &EsfFile) -> [BTreeSet<u32>; 3] {
    let mut out: [BTreeSet<u32>; 3] = Default::default();
    let Some(w) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return out };
    let mut fs: Vec<&EsfRecord> = w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    fs.extend(w.child("REBEL_FACTION").and_then(|r| r.child("FACTION")));
    for f in fs {
        for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
            out[0].insert(c.get_i32(2).unwrap_or(0) as u32);
        }
        for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
            if let Some(m) = a.child("MILITARY_FORCE") {
                out[1].insert(m.get_u32(0).unwrap_or(0));
            }
            for u in a.record_array("UNITS_ARRAY").into_iter().flat_map(|x| x.records()).filter_map(|w| w.child("UNIT")) {
                out[2].insert(u.get_i32(4).unwrap_or(0) as u32);
            }
        }
    }
    out
}

fn norm(path: &str) -> String {
    path.split('[').map(|s| s.split_once(']').map_or(s, |x| x.1)).collect::<Vec<_>>().join("[]")
}

/// Every (normalised path, index) where one of `ids` appears as a u32 / i32 or array element.
fn sites(nodes: &[EsfNode], ids: &BTreeSet<u32>, path: &str, out: &mut BTreeMap<String, usize>, hits: &mut BTreeSet<u32>) {
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(r) => sites(&r.children, ids, &format!("{path}/{}", r.name), out, hits),
            EsfNode::RecordArray(a) => {
                for it in &a.items {
                    sites(it, ids, &format!("{path}/{}[]", a.name), out, hits);
                }
            }
            other => {
                let mut f = |v: u32| {
                    if ids.contains(&v) {
                        *out.entry(format!("{} #{i}", norm(path))).or_default() += 1;
                        hits.insert(v);
                    }
                };
                if let Some(v) = other.as_u32() {
                    f(v);
                } else if let Some(v) = other.as_i32() {
                    f(v as u32);
                } else if let Some(a) = other.as_u32_array() {
                    a.iter().for_each(|&v| f(v));
                }
            }
        }
    }
}

fn cmd_mirrors(esf: &EsfFile) {
    let world = world_ids(esf);
    for (k, (list, rec, idx)) in LISTS.iter().enumerate() {
        let m = mirrors(esf, list, rec, *idx);
        let games: BTreeSet<u32> = m.iter().map(|x| x.game).collect();
        let comps: Vec<u32> = m.iter().map(|x| x.component).collect();
        let missing = world[k].difference(&games).count();
        let extra = games.difference(&world[k]).count();
        println!(
            "{list}: {} items ({} distinct game ids), world {}, world objects without mirror {missing}, mirrors of no object {extra}, components {}..{}",
            m.len(),
            games.len(),
            world[k].len(),
            comps.iter().min().unwrap_or(&0),
            comps.iter().max().unwrap_or(&0)
        );
    }
    // The largest component id anywhere small (< 1e6) in CAI_WORLD.
    if let Some(w) = esf.root.find_path(&format!("{CAI}/CAI_WORLD")) {
        let mut max = 0u32;
        w.walk(&mut |r| {
            for c in &r.children {
                if let Some(v) = c.as_u32().filter(|&v| v < 1_000_000) {
                    max = max.max(v);
                }
            }
        });
        println!("largest small u32 in CAI_WORLD records: {max}");
    }
}

fn cmd_pair(a: &EsfFile, b: &EsfFile) {
    let (wa, wb) = (world_ids(a), world_ids(b));
    let cai_b = b.root.find_path(CAI).expect("CAI_INTERFACE");
    for (k, (list, rec, idx)) in LISTS.iter().enumerate() {
        let ma = mirrors(a, list, rec, *idx);
        let mb = mirrors(b, list, rec, *idx);
        let gone: BTreeSet<u32> = wa[k].difference(&wb[k]).copied().collect();
        let new: BTreeSet<u32> = wb[k].difference(&wa[k]).copied().collect();
        let comp_a: BTreeMap<u32, u32> = ma.iter().map(|m| (m.game, m.component)).collect();
        let comp_b: BTreeMap<u32, u32> = mb.iter().map(|m| (m.game, m.component)).collect();
        let kept_same = wa[k].intersection(&wb[k]).filter(|g| comp_a.get(g) == comp_b.get(g)).count();
        let kept = wa[k].intersection(&wb[k]).count();
        println!("== {list}: {} gone, {} new, {kept} kept ({kept_same} with the same component)", gone.len(), new.len());
        // Gone objects: does any component of theirs survive in B?
        let gone_comps: BTreeSet<u32> = gone.iter().filter_map(|g| comp_a.get(g).copied()).collect();
        let all_b: BTreeSet<u32> = mb.iter().map(|m| m.component).collect();
        let reused = gone_comps.intersection(&all_b).count();
        let mut out = BTreeMap::new();
        let mut hits = BTreeSet::new();
        sites(&cai_b.children, &gone_comps, "", &mut out, &mut hits);
        println!("   gone components: {} (reused by a mirror in NEW: {reused}); still named in NEW's AI block: {} of them", gone_comps.len(), hits.len());
        for (p, n) in out.iter().take(25) {
            println!("      {n:6} {p}");
        }
        // New objects: their mirrors, and where their components appear.
        let new_comps: BTreeSet<u32> = new.iter().filter_map(|g| comp_b.get(g).copied()).collect();
        println!("   new objects with a mirror: {} of {}", new_comps.len(), new.len());
        let mut out = BTreeMap::new();
        let mut hits = BTreeSet::new();
        sites(&cai_b.children, &new_comps, "", &mut out, &mut hits);
        for (p, n) in out.iter().take(40) {
            println!("      {n:6} {p}");
        }
        if let Some(g) = new.iter().find(|g| comp_b.contains_key(g)) {
            let c = comp_b[g];
            let comps_a: BTreeSet<u32> = ma.iter().map(|m| m.component).collect();
            println!("   example new object {g}: component {c} (was the component used in OLD's mirrors: {})", comps_a.contains(&c));
        }
    }
}

fn cmd_comp(esf: &EsfFile, id: u32) {
    let cai = esf.root.find_path(CAI).expect("CAI_INTERFACE");
    let mut out = BTreeMap::new();
    let mut hits = BTreeSet::new();
    sites(&cai.children, &BTreeSet::from([id]), "", &mut out, &mut hits);
    for (p, n) in out {
        println!("{n:6} {p}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let open = |p: &str| EsfFile::open(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    match args.first().map(String::as_str) {
        Some("mirrors") => cmd_mirrors(&open(&args[1])),
        Some("pair") => cmd_pair(&open(&args[1]), &open(&args[2])),
        Some("graph") => cmd_graph(&open(&args[1])),
        Some("show") => cmd_show(&open(&args[1]), args[2].parse().expect("id")),
        Some("ids") => cmd_ids(&open(&args[1]), &open(&args[2])),
        Some("refs") => cmd_refs(&args[1..].iter().map(|p| open(p)).collect::<Vec<_>>()),
        Some("invalid") => cmd_invalid(&open(&args[1]), &args[2]),
        Some("removal") => cmd_removal(&open(&args[1]), &open(&args[2])),
        Some("sites") => cmd_sites(&args[1..]),
        Some("simulate") => cmd_simulate(&open(&args[1]), &open(&args[2])),
        Some("a8") => cmd_a8(&open(&args[1])),
        Some("tas") => cmd_tas(&open(&args[1])),
        Some("mirror_rel") => cmd_mirror_rel(&open(&args[1])),
        Some("mirror_loc") => cmd_mirror_loc(&open(&args[1])),
        Some("newmirror") => cmd_newmirror(&open(&args[1]), &open(&args[2]), &args[3], args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0)),
        Some("counters") => cmd_counters(&open(&args[1])),
        Some("shapes") => cmd_shapes(&open(&args[1]), args[2].parse().unwrap()),
        Some("owns") => cmd_owns(&open(&args[1])),
        Some("links") => cmd_links(&open(&args[1])),
        Some("mcheck") => cmd_mirrors_check(&args[1..]),
        Some("owned") => cmd_owned(&open(&args[1])),
        Some("comp") => cmd_comp(&open(&args[1]), args[2].parse().expect("id")),
        _ => eprintln!("usage: cai_audit mirrors FILE | pair OLD NEW | comp FILE ID"),
    }
}

/// Component graph statistics: how many components, whether ids are unique, and whether every id
/// named in the link lists is a component.
pub fn cmd_graph(esf: &EsfFile) {
    let cai = esf.root.find_path(CAI).expect("CAI_INTERFACE");
    let comps = ntw_campaign::cai::components(cai);
    let ids: BTreeSet<u32> = comps.iter().map(|c| c.id).collect();
    println!("{} components, {} distinct ids, ids {}..{}", comps.len(), ids.len(), ids.iter().next().unwrap_or(&0), ids.iter().last().unwrap_or(&0));
    let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &comps {
        *classes.entry(c.class.as_str()).or_default() += 1;
    }
    println!("{} classes", classes.len());
    let names = ["a8", "a9", "a12", "a13", "b17", "b18", "b19", "b20"];
    for (k, name) in names.iter().enumerate() {
        let (mut n, mut bad, mut odd) = (0usize, 0usize, 0usize);
        for c in &comps {
            let l = if k < 4 { &c.lists_a[k] } else { &c.lists_b[k - 4] };
            n += l.len();
            bad += l.iter().filter(|v| !ids.contains(v)).count();
            odd += usize::from(l.len() % 2 == 1);
        }
        println!("list {name}: {n} values, {bad} not a component id, {odd} odd-length lists");
    }
    let (mut n, mut bad0, mut bad1, mut self0) = (0, 0, 0, 0);
    for c in &comps {
        for o in &c.owns {
            n += 1;
            bad0 += usize::from(!ids.contains(&o.0));
            bad1 += usize::from(!ids.contains(&o.1));
            self0 += usize::from(o.0 == c.id);
        }
    }
    println!("owns: {n} links, owner not a component {bad0}, owned not a component {bad1}, owner = this component {self0}");
}

/// One component, by id.
pub fn cmd_show(esf: &EsfFile, id: u32) {
    let cai = esf.root.find_path(CAI).expect("CAI_INTERFACE");
    for c in ntw_campaign::cai::components(cai).into_iter().filter(|c| c.id == id) {
        println!("{} class {} at {}", c.id, c.class, c.path);
        for (k, l) in c.lists_a.iter().enumerate() {
            println!("  a{} ({}) {:?}", [8, 9, 12, 13][k], l.len(), &l[..l.len().min(24)]);
        }
        println!("  owns ({}) {:?}", c.owns.len(), &c.owns[..c.owns.len().min(12)]);
        for (k, l) in c.lists_b.iter().enumerate() {
            println!("  b{} ({}) {:?}", 17 + k, l.len(), &l[..l.len().min(24)]);
        }
    }
}

/// Components of OLD and NEW by id: kept (same id, same class), class changed, gone, new; by class.
pub fn cmd_ids(a: &EsfFile, b: &EsfFile) {
    let ca = ntw_campaign::cai::components(a.root.find_path(CAI).expect("cai"));
    let cb = ntw_campaign::cai::components(b.root.find_path(CAI).expect("cai"));
    let ma: BTreeMap<u32, &str> = ca.iter().map(|c| (c.id, c.class.as_str())).collect();
    let mb: BTreeMap<u32, &str> = cb.iter().map(|c| (c.id, c.class.as_str())).collect();
    let mut per: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    for (id, cl) in &ma {
        match mb.get(id) {
            Some(x) if x == cl => per.entry(cl).or_default()[0] += 1,
            Some(_) => per.entry(cl).or_default()[1] += 1,
            None => per.entry(cl).or_default()[2] += 1,
        }
    }
    for (id, cl) in &mb {
        if !ma.contains_key(id) {
            per.entry(cl).or_default()[3] += 1;
        }
    }
    let t = per.values().fold([0; 4], |acc, v| [acc[0] + v[0], acc[1] + v[1], acc[2] + v[2], acc[3] + v[3]]);
    println!("total: kept {}, class changed {}, gone {}, new {}", t[0], t[1], t[2], t[3]);
    for (cl, v) in per {
        if v[2] + v[3] + v[1] > 0 {
            println!("  {cl:60} kept {:6} changed {:4} gone {:6} new {:6}", v[0], v[1], v[2], v[3]);
        }
    }
}

#[derive(Default, Clone)]
struct SiteStats {
    values: usize,
    zero: usize,
    valid: usize,
    max: u32,
    distinct: BTreeSet<u32>,
}

/// Walks the AI block and calls `f(site, value)` for every u32 scalar and u32-array element
/// (array sites get the suffixes `[all]`, `[even]`, `[odd]`).
fn walk_values(nodes: &[EsfNode], path: &str, f: &mut dyn FnMut(&str, u32)) {
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(r) => walk_values(&r.children, &format!("{path}/{}", r.name), f),
            EsfNode::RecordArray(a) => {
                let p = format!("{path}/{}[]", a.name);
                for it in &a.items {
                    walk_values(it, &p, f);
                }
            }
            EsfNode::U32(v) => f(&format!("{path} #{i}"), *v),
            EsfNode::U32Array(v) => {
                for (k, x) in v.iter().enumerate() {
                    f(&format!("{path} #{i}[all]"), *x);
                    f(&format!("{path} #{i}[{}]", if k % 2 == 0 { "even" } else { "odd" }), *x);
                }
            }
            _ => {}
        }
    }
}

/// Classifies every value site of the AI block over several saves: a site is a component
/// reference when every non-zero value in every file is a component id of that file, with at
/// least one value above 1000 and 3 distinct values overall.
pub fn cmd_refs(files: &[EsfFile]) {
    let mut stats: BTreeMap<String, SiteStats> = BTreeMap::new();
    for esf in files {
        let cai = esf.root.find_path(CAI).expect("cai");
        let ids: BTreeSet<u32> = ntw_campaign::cai::components(cai).iter().map(|c| c.id).collect();
        walk_values(&cai.children, "", &mut |site, v| {
            let s = stats.entry(site.to_string()).or_default();
            s.values += 1;
            if v == 0 {
                s.zero += 1;
                return;
            }
            if ids.contains(&v) {
                s.valid += 1;
            }
            s.max = s.max.max(v);
            if s.distinct.len() < 50 {
                s.distinct.insert(v);
            }
        });
    }
    let mut refs = 0;
    for (site, s) in &stats {
        let nonzero = s.values - s.zero;
        let is_ref = nonzero > 0 && s.valid == nonzero && s.max > 1000 && s.distinct.len() >= 3;
        let near = nonzero > 0 && s.valid * 100 >= nonzero * 90 && s.valid < nonzero && s.max > 1000;
        if is_ref {
            refs += 1;
            println!("REF  {:8} {site}", nonzero);
        } else if near {
            println!("NEAR {:8} {site} ({} of {} valid)", nonzero, s.valid, nonzero);
        }
    }
    println!("{refs} reference sites of {} sites", stats.len());
}

/// The values at one site (suffix as in `refs`) that are not component ids, with the file's
/// next-id counter, to tell dangling ids (below the counter) from another id space.
pub fn cmd_invalid(esf: &EsfFile, site: &str) {
    let cai = esf.root.find_path(CAI).expect("cai");
    let comps = ntw_campaign::cai::components(cai);
    let ids: BTreeSet<u32> = comps.iter().map(|c| c.id).collect();
    let next = cai.child("CAI_CENTRAL_BDI_POOL").and_then(|p| p.get_u32(0)).unwrap_or(0);
    let (mut n, mut bad) = (0, Vec::new());
    walk_values(&cai.children, "", &mut |s, v| {
        if s == site && v != 0 {
            n += 1;
            if !ids.contains(&v) {
                bad.push(v);
            }
        }
    });
    bad.sort();
    println!("{n} values, {} not a component (next id {next}); first: {:?}", bad.len(), &bad[..bad.len().min(30)]);
}

/// Like `walk_values`, also passing the id of the innermost component whose block shares a node
/// list with (or encloses) the value (0 = none).
fn walk_owned(nodes: &[EsfNode], path: &str, owner: u32, f: &mut dyn FnMut(u32, &str, u32)) {
    let here = (0..nodes.len())
        .find(|&p| matches!(&nodes[p], EsfNode::Record(r) if r.name == "CAI_BDI_COMPONENT_PROPERTY_SET") && matches!(nodes.get(p + 1), Some(EsfNode::U32(_))) && matches!(nodes.get(p + 14), Some(EsfNode::RecordArray(a)) if a.name == "CAI_BDI_COMPONENT_BLOCK_OWNS"))
        .and_then(|p| nodes[p + 1].as_u32())
        .unwrap_or(owner);
    for (i, n) in nodes.iter().enumerate() {
        match n {
            EsfNode::Record(r) => walk_owned(&r.children, &format!("{path}/{}", r.name), here, f),
            EsfNode::RecordArray(a) => {
                let p = format!("{path}/{}[]", a.name);
                for it in &a.items {
                    walk_owned(it, &p, here, f);
                }
            }
            EsfNode::U32(v) => f(here, &format!("{path} #{i}"), *v),
            EsfNode::U32Array(v) => {
                for (k, x) in v.iter().enumerate() {
                    f(here, &format!("{path} #{i}[{}]", if k % 2 == 0 { "even" } else { "odd" }), *x);
                }
            }
            _ => {}
        }
    }
}

/// How the original handled references to world mirrors that disappeared between OLD and NEW
/// (component ids are stable): for every (site, referencing class), whether the referencing
/// component was removed, or kept with the reference dropped, or kept with the reference still
/// there (dangling).
pub fn cmd_removal(a: &EsfFile, b: &EsfFile) {
    let (cai_a, cai_b) = (a.root.find_path(CAI).expect("cai"), b.root.find_path(CAI).expect("cai"));
    let ca = ntw_campaign::cai::components(cai_a);
    let cb = ntw_campaign::cai::components(cai_b);
    let class_a: BTreeMap<u32, String> = ca.iter().map(|c| (c.id, c.class.clone())).collect();
    let ids_b: BTreeSet<u32> = cb.iter().map(|c| c.id).collect();
    let gone: BTreeSet<u32> = ca
        .iter()
        .filter(|c| matches!(c.class.as_str(), "CAI_CHARACTER" | "CAI_UNIT" | "CAI_RESOURCE_MOBILE") && !ids_b.contains(&c.id))
        .map(|c| c.id)
        .collect();
    println!("{} world mirrors gone", gone.len());
    // References to gone mirrors in OLD: (owner, site) -> count.
    let mut refs_a: BTreeMap<(u32, String), usize> = BTreeMap::new();
    walk_owned(&cai_a.children, "", 0, &mut |owner, site, v| {
        if gone.contains(&v) && owner != v {
            *refs_a.entry((owner, site.to_string())).or_default() += 1;
        }
    });
    let mut refs_b: BTreeMap<(u32, String), usize> = BTreeMap::new();
    walk_owned(&cai_b.children, "", 0, &mut |owner, site, v| {
        if gone.contains(&v) && owner != v {
            *refs_b.entry((owner, site.to_string())).or_default() += 1;
        }
    });
    // Outcome per (site, class).
    let mut out: BTreeMap<(String, String), [usize; 3]> = BTreeMap::new();
    for ((owner, site), n) in &refs_a {
        let class = class_a.get(owner).cloned().unwrap_or_else(|| "-".into());
        let e = out.entry((site.clone(), class)).or_default();
        if !ids_b.contains(owner) {
            e[0] += n;
        } else {
            let left = refs_b.get(&(*owner, site.clone())).copied().unwrap_or(0);
            e[2] += left.min(*n);
            e[1] += n.saturating_sub(left);
        }
    }
    println!("{:>7} {:>7} {:>7}  site (referencing class)", "comp-", "ref-", "kept");
    for ((site, class), v) in out {
        println!("{:7} {:7} {:7}  {site} ({class})", v[0], v[1], v[2]);
    }
}

/// The site table for the writer (crates/ntw_campaign/src/cai_sites.txt): for every data site of
/// the AI block that holds component ids in the original's saves, what the original does with a
/// reference to a removed component there, learnt from consecutive saves of one game (component
/// ids are stable): `subject` (the referencing component is removed), `entry` (the reference is
/// dropped, the component kept), `tolerated` (the original leaves the dangling id). Link lists
/// (`cai::Place::Link`) are not listed: the writer drops removed ids from them structurally.
/// Arguments: the saves of one game in order, `--`, the next game, ...
pub fn cmd_sites(paths: &[String]) {
    use ntw_campaign::cai::Place;
    let open = |p: &str| EsfFile::open(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    // site -> (nonzero, valid, zero, max, distinct, place)
    #[allow(clippy::type_complexity)] // research tool: nonzero, valid, zero, max, distinct, place
    let mut valid: BTreeMap<String, (usize, usize, usize, u32, BTreeSet<u32>, Option<Place>)> = BTreeMap::new();
    let mut outcome: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    let mut link_bad = 0usize;
    let mut games: Vec<Vec<&str>> = vec![Vec::new()];
    for p in paths {
        if p == "--" {
            games.push(Vec::new());
        } else {
            games.last_mut().unwrap().push(p);
        }
    }
    type Refs = BTreeMap<(u32, String), usize>;
    let refs_to = |cai: &EsfRecord, gone: &BTreeSet<u32>| -> Refs {
        let mut r = Refs::new();
        ntw_campaign::cai::walk_values(&cai.children, "", &mut |o, s, place, v| {
            if place != Place::Link && gone.contains(&v) && o != v {
                *r.entry((o, s.to_string())).or_default() += 1;
            }
        });
        r
    };
    for game in &games {
        let mut prev: Option<EsfFile> = None;
        for p in game {
            let esf = open(p);
            let cai = esf.root.find_path(CAI).expect("cai");
            let ids: BTreeSet<u32> = ntw_campaign::cai::components(cai).iter().map(|c| c.id).collect();
            ntw_campaign::cai::walk_values(&cai.children, "", &mut |_, site, place, v| {
                if place == Place::Link {
                    if v != 0 && !ids.contains(&v) && !site.ends_with("[odd]") {
                        link_bad += 1;
                    }
                    return;
                }
                let s = valid.entry(site.to_string()).or_default();
                s.5 = Some(place);
                if v == 0 {
                    s.2 += 1;
                    return;
                }
                s.0 += 1;
                if ids.contains(&v) {
                    s.1 += 1;
                }
                s.3 = s.3.max(v);
                if s.4.len() < 50 {
                    s.4.insert(v);
                }
            });
            if let Some(a) = prev.take() {
                let cai_a = a.root.find_path(CAI).expect("cai");
                let ids_a: BTreeSet<u32> = ntw_campaign::cai::components(cai_a).iter().map(|c| c.id).collect();
                let gone: BTreeSet<u32> = ids_a.difference(&ids).copied().collect();
                let ra = refs_to(cai_a, &gone);
                let rb = refs_to(cai, &gone);
                for ((o, s), n) in &ra {
                    let e = outcome.entry(s.clone()).or_default();
                    if !ids.contains(o) {
                        e[0] += n;
                    } else {
                        let left = rb.get(&(*o, s.clone())).copied().unwrap_or(0).min(*n);
                        e[2] += left;
                        e[1] += n - left;
                    }
                }
            }
            prev = Some(esf);
        }
    }
    eprintln!("link-list values that are not component ids (pair flags excluded): {link_bad}");
    println!("# AI-block data reference sites (generated by `cai_audit sites` from the original's saves; SAVE_COMPAT.md §10).");
    println!("# site<TAB>kind<TAB>place<TAB>zero_seen<TAB>nonzero/component-removed/ref-dropped/kept");
    for (site, s) in &valid {
        let is_ref = s.0 > 0 && s.1 == s.0 && s.3 > 1000 && s.4.len() >= 3;
        let mostly = s.0 > 0 && s.1 * 100 >= s.0 * 90 && s.3 > 1000 && s.4.len() >= 3;
        if !is_ref && !mostly {
            continue;
        }
        let o = outcome.get(site).copied().unwrap_or_default();
        let kind = if !is_ref || o[2] > 0 {
            "tolerated"
        } else if o[0] > 0 && o[1] == 0 && !site.starts_with("/CAI_WORLD/") {
            "subject"
        } else {
            "entry"
        };
        let place = match s.5 {
            Some(Place::Scalar) => "scalar",
            Some(Place::ArrayElement) => "array",
            Some(Place::InDataItem) => "item",
            _ => "?",
        };
        println!("{site}\t{kind}\t{place}\t{}\t{}/{}/{}/{}", u8::from(s.2 > 0), s.0, o[0], o[1], o[2]);
    }
}

/// Simulates the writer's removal on OLD: removes the world mirrors whose components are gone in
/// NEW (and their dependants, `cai::remove_components`), then reports the block check and how the
/// removed set compares with what the original removed.
pub fn cmd_simulate(a: &EsfFile, b: &EsfFile) {
    let mut a = a.clone();
    let ids_b: BTreeSet<u32> = ntw_campaign::cai::components(b.root.find_path(CAI).expect("cai")).iter().map(|c| c.id).collect();
    let cai = a.root.find_path(CAI).expect("cai");
    println!("OLD block check: {:?}", ntw_campaign::cai::check_block(cai, 5));
    let comps = ntw_campaign::cai::components(cai);
    let ids_a: BTreeSet<u32> = comps.iter().map(|c| c.id).collect();
    let gone_mirrors: BTreeSet<u32> = comps
        .iter()
        .filter(|c| matches!(c.class.as_str(), "CAI_CHARACTER" | "CAI_UNIT" | "CAI_RESOURCE_MOBILE") && !ids_b.contains(&c.id))
        .map(|c| c.id)
        .collect();
    let class: BTreeMap<u32, String> = comps.iter().map(|c| (c.id, c.class.clone())).collect();
    let cai = find_mut(&mut a.root, &["CAMPAIGN_ENV", "CAMPAIGN_MODEL", "CAI_INTERFACE"]).expect("cai");
    let r = ntw_campaign::cai::remove_components(cai, &gone_mirrors);
    let gone_b: BTreeSet<u32> = ids_a.difference(&ids_b).copied().collect();
    let over: Vec<u32> = r.removed.difference(&gone_b).copied().collect();
    println!(
        "{} mirrors removed; {} components removed in all; the original removed {} components; we removed {} that it kept; structural kept {:?}; unknown sites {:?}",
        gone_mirrors.len(),
        r.removed.len(),
        gone_b.len(),
        over.len(),
        r.structural_kept,
        r.unknown_sites
    );
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    for id in &over {
        let why = r.reasons.get(id).map_or("requested", String::as_str);
        *by.entry(format!("{} via {why}", class.get(id).map_or("?", String::as_str))).or_default() += 1;
    }
    for (c, n) in by {
        println!("   over-removed {n:5} {c}");
    }
    let cai = a.root.find_path(CAI).expect("cai");
    let bad = ntw_campaign::cai::check_block(cai, 10);
    println!("block check after removal: {} lines", bad.len());
    for l in bad {
        println!("   {l}");
    }
}

fn find_mut<'a>(r: &'a mut EsfRecord, path: &[&str]) -> Option<&'a mut EsfRecord> {
    let Some((first, rest)) = path.split_first() else { return Some(r) };
    let next = r.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(x) if x.name == *first => Some(&mut **x),
        _ => None,
    })?;
    find_mut(next, rest)
}

/// Values of the block list at offset 8 that are not component ids, by component class.
pub fn cmd_a8(esf: &EsfFile) {
    let cai = esf.root.find_path(CAI).expect("cai");
    let comps = ntw_campaign::cai::components(cai);
    let ids: BTreeSet<u32> = comps.iter().map(|c| c.id).collect();
    let mut by: BTreeMap<&str, (usize, usize, Vec<u32>)> = BTreeMap::new();
    for c in &comps {
        let e = by.entry(c.class.as_str()).or_default();
        for v in &c.lists_a[0] {
            e.0 += 1;
            if !ids.contains(v) {
                e.1 += 1;
                if e.2.len() < 8 {
                    e.2.push(*v);
                }
            }
        }
    }
    for (k, (n, bad, ex)) in by {
        if bad > 0 {
            println!("{k:50} {n:7} values, {bad:6} not ids, e.g. {ex:?}");
        }
    }
    let ex = comps.iter().find(|c| c.lists_a[0].iter().any(|v| !ids.contains(v))).unwrap();
    println!("example {} {}: a8 {:?}", ex.id, ex.class, &ex.lists_a[0][..ex.lists_a[0].len().min(30)]);
}

/// Checks the `CAI_TAS_ANALYSIS` layout guess: [u32 subject, u32, u32, u32 n1, n1 x (u32, f32,
/// f32, i32), u32 n2, n2 x (u32, f32, f32, i32), rest].
pub fn cmd_tas(esf: &EsfFile) {
    let (mut ok, mut bad, mut n) = (0, 0, 0);
    esf.root.walk(&mut |r| {
        if r.name != "CAI_TAS_ANALYSIS" {
            return;
        }
        n += 1;
        let c = &r.children;
        let entry = |i: usize| {
            matches!((c.get(i), c.get(i + 1), c.get(i + 2), c.get(i + 3)), (Some(EsfNode::U32(_)), Some(EsfNode::F32(_)), Some(EsfNode::F32(_)), Some(EsfNode::I32(_))))
        };
        let mut i = 2;
        let mut good = true;
        for _ in 0..3 {
            let Some(cnt) = c.get(i).and_then(EsfNode::as_u32) else {
                good = false;
                break;
            };
            i += 1;
            for _ in 0..cnt {
                if !entry(i) {
                    good = false;
                }
                i += 4;
            }
        }
        if good && matches!(c.get(i), Some(EsfNode::I32(_))) {
            ok += 1;
        } else {
            bad += 1;
        }
    });
    println!("{n} TAS records: layout fits {ok}, not {bad}");
}

/// The world mirrors' fields against the game world: for each mirror field, how often it equals
/// the component of a candidate related object (SAVE_COMPAT.md §10).
pub fn cmd_mirror_rel(esf: &EsfFile) {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").expect("world");
    // Game world: character -> (force commanded, unit attached), force -> (commander, units, navy), unit -> (force, attached character).
    let mut char_force: BTreeMap<u32, (u32, u32, String)> = BTreeMap::new();
    let mut force_info: BTreeMap<u32, (u32, Vec<u32>, bool)> = BTreeMap::new();
    let mut unit_info: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut faction_of: BTreeMap<u32, u32> = BTreeMap::new();
    let mut fs: Vec<&EsfRecord> = w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()).collect();
    fs.extend(w.child("REBEL_FACTION").and_then(|r| r.child("FACTION")));
    for f in &fs {
        let fid = f.children.iter().find(|c| !matches!(c, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_i32).unwrap_or(0) as u32;
        for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
            let id = c.get_i32(2).unwrap_or(0) as u32;
            char_force.insert(id, (c.get_u32(4).unwrap_or(0), c.get_u32(5).unwrap_or(0), c.get_str(3).unwrap_or("").to_string()));
            faction_of.insert(id, fid);
        }
        for a in f.record_array("ARMY_ARRAY").into_iter().flat_map(|a| a.records()) {
            let Some(m) = a.child("MILITARY_FORCE") else { continue };
            let fid2 = m.get_u32(0).unwrap_or(0);
            let units: Vec<u32> = a.record_array("UNITS_ARRAY").into_iter().flat_map(|x| x.records()).filter_map(|w| w.child("UNIT")).map(|u| u.get_i32(4).unwrap_or(0) as u32).collect();
            for u in a.record_array("UNITS_ARRAY").into_iter().flat_map(|x| x.records()).filter_map(|w| w.child("UNIT")) {
                unit_info.insert(u.get_i32(4).unwrap_or(0) as u32, (fid2, u.get_u32(10).unwrap_or(0)));
            }
            force_info.insert(fid2, (m.get_u32(1).unwrap_or(0), units, a.name == "NAVY"));
            faction_of.insert(fid2, fid);
        }
    }
    // Mirrors: game id -> component, and the mirror records.
    let cai = esf.root.find_path(CAI).expect("cai");
    let wcai = cai.child("CAI_WORLD").expect("CAI_WORLD");
    let comp_of = |list: &str, rec: &str, idx: usize| -> BTreeMap<u32, u32> {
        wcai.record_array(list)
            .into_iter()
            .flat_map(|a| a.items.iter())
            .filter_map(|it| {
                let r = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == rec)?;
                Some((r.get_u32(idx)?, component_of(it)?))
            })
            .filter(|(g, _)| *g != 0)
            .collect()
    };
    let cc = comp_of("CAI_WORLD_CHARACTERS", "CAI_CHARACTER", 3);
    let fc = comp_of("CAI_WORLD_RESOURCE_MOBILES", "CAI_RESOURCE_MOBILE", 10);
    let uc = comp_of("CAI_WORLD_UNITS", "CAI_UNIT", 1);
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut t = |k: String, ok: bool| {
        let e = tally.entry(k).or_default();
        e.0 += 1;
        e.1 += usize::from(ok);
    };
    for it in wcai.record_array("CAI_WORLD_CHARACTERS").into_iter().flat_map(|a| a.items.iter()) {
        let Some(r) = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_CHARACTER") else { continue };
        let g = r.get_u32(3).unwrap_or(0);
        let Some(&(force, unit, ref kind)) = char_force.get(&g) else { continue };
        let force_comp = fc.get(&force).copied().unwrap_or(0);
        let unit_comp = uc.get(&unit).copied().unwrap_or(0);
        let unit_force_comp = unit_info.get(&unit).and_then(|u| fc.get(&u.0)).copied().unwrap_or(0);
        for k in 0..3 {
            let v = r.get_u32(k).unwrap_or(0);
            t(format!("CAI_CHARACTER #{k} == mobile of the force it commands"), v == force_comp);
            t(format!("CAI_CHARACTER #{k} == unit it is attached to"), v == unit_comp);
            t(format!("CAI_CHARACTER #{k} == mobile of its unit's force"), v == unit_force_comp);
            t(format!("CAI_CHARACTER #{k} == 0"), v == 0);
        }
        t(format!("CAI_CHARACTER #4/#5 zero ({kind})"), r.get_u32(4) == Some(0) && r.get_u32(5) == Some(0));
        // Mobiles led by / containing this character (from the mobiles' #0 and #4).
        let me = cc.get(&g).copied().unwrap_or(0);
        let mobiles: Vec<(u32, u32, Vec<u32>)> = wcai
            .record_array("CAI_WORLD_RESOURCE_MOBILES")
            .into_iter()
            .flat_map(|a| a.items.iter())
            .filter_map(|it| {
                let m = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_RESOURCE_MOBILE")?;
                Some((component_of(it)?, m.get_u32(0)?, m.get(4)?.as_u32_array()?.to_vec()))
            })
            .collect();
        let led = mobiles.iter().find(|m| m.1 == me).map_or(0, |m| m.0);
        let inside = mobiles.iter().find(|m| m.2.contains(&me)).map_or(0, |m| m.0);
        let class = if force != 0 { "commander" } else if unit != 0 { "attached" } else { "other" };
        t(format!("CAI_CHARACTER #0 == mobile it leads ({class})"), r.get_u32(0) == Some(led));
        t(format!("CAI_CHARACTER #2 == mobile whose #4 holds it ({class})"), r.get_u32(2) == Some(inside));
        let owner = it.first().and_then(EsfNode::as_record).map(|o| (o.name.clone(), o.get_u32(0).unwrap_or(0)));
        t(format!("CAI_CHARACTER item #0 = {} (any)", owner.as_ref().map_or("-", |o| o.0.as_str())), true);
    }
    for it in wcai.record_array("CAI_WORLD_UNITS").into_iter().flat_map(|a| a.items.iter()) {
        let Some(r) = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_UNIT") else { continue };
        let g = r.get_u32(1).unwrap_or(0);
        let Some(&(force, ch)) = unit_info.get(&g) else { continue };
        let force_comp = fc.get(&force).copied().unwrap_or(0);
        let ch_comp = cc.get(&ch).copied().unwrap_or(0);
        let cmd_comp = force_info.get(&force).and_then(|f| cc.get(&f.0)).copied().unwrap_or(0);
        t("CAI_UNIT #0 == its attached character".into(), r.get_u32(0) == Some(ch_comp));
        t("CAI_UNIT #0 == its force's commander".into(), r.get_u32(0) == Some(cmd_comp));
        t("CAI_UNIT #2 == its force's mobile".into(), r.get_u32(2) == Some(force_comp));
    }
    for it in wcai.record_array("CAI_WORLD_RESOURCE_MOBILES").into_iter().flat_map(|a| a.items.iter()) {
        let Some(r) = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_RESOURCE_MOBILE") else { continue };
        let g = r.get_u32(10).unwrap_or(0);
        if g == 0 {
            let first = r.get(4).and_then(EsfNode::as_u32_array).and_then(|a| a.first().copied()).unwrap_or(0);
            let is_char = cc.values().any(|&c| c == first);
            t("mobile without force: #4[0] is a character mirror".into(), is_char);
            t("mobile without force: #5 empty".into(), r.get(5).and_then(EsfNode::as_u32_array).is_some_and(|a| a.is_empty()));
            continue;
        }
        let Some((cmd, units, navy)) = force_info.get(&g) else { continue };
        let cmd_comp = cc.get(cmd).copied().unwrap_or(0);
        let unit_comps: Vec<u32> = units.iter().map(|u| uc.get(u).copied().unwrap_or(0)).collect();
        let l4: Vec<u32> = r.get(4).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
        let l5: Vec<u32> = r.get(5).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
        t(format!("mobile #0 == commander (navy {navy})"), r.get_u32(0) == Some(cmd_comp));
        t(format!("mobile #4 == [commander] (navy {navy})"), l4 == [cmd_comp]);
        t(format!("mobile #4[0] == commander (navy {navy})"), l4.first() == Some(&cmd_comp));
        t(format!("mobile #5 == units in order (navy {navy})"), l5 == unit_comps);
        let mut a = l5.clone();
        a.sort();
        let mut b = unit_comps.clone();
        b.sort();
        t(format!("mobile #5 == units as a set (navy {navy})"), a == b);
        // Characters attached to units of this force (other than the commander).
        let attached: Vec<u32> = units.iter().filter_map(|u| unit_info.get(u)).map(|u| u.1).filter(|&c| c != 0 && c != *cmd).filter_map(|c| cc.get(&c).copied()).collect();
        t(format!("mobile #4 == [commander] + attached characters (navy {navy})"), {
            let mut x = l4.clone();
            x.sort();
            let mut y = attached.clone();
            y.push(cmd_comp);
            y.sort();
            x == y
        });
    }
    for (k, (n, ok)) in tally {
        println!("{ok:6} / {n:6}  {k}");
    }
}

/// Location and ownership relations of the AI world mirrors (SAVE_COMPAT.md §10).
pub fn cmd_mirror_loc(esf: &EsfFile) {
    let cai = esf.root.find_path(CAI).expect("cai");
    let wcai = cai.child("CAI_WORLD").expect("CAI_WORLD");
    let items = |list: &str| -> Vec<&[EsfNode]> { wcai.record_array(list).into_iter().flat_map(|a| a.items.iter()).map(|v| v.as_slice()).collect() };
    let rec = |it: &[EsfNode], name: &str| -> Option<EsfRecord> { it.iter().filter_map(EsfNode::as_record).find(|r| r.name == name).cloned() };
    let arr = |r: &EsfRecord, i: usize| -> Vec<u32> { r.get(i).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default() };
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut t = |k: &str, ok: bool| {
        let e = tally.entry(k.to_string()).or_default();
        e.0 += 1;
        e.1 += usize::from(ok);
    };
    // Regions: comp -> mobiles list #9; theatres: comp -> #1.
    let regions: BTreeMap<u32, Vec<u32>> = items("CAI_WORLD_REGIONS").iter().filter_map(|it| Some((component_of(it)?, arr(&rec(it, "CAI_REGION")?, 9)))).collect();
    let theatres: BTreeMap<u32, Vec<u32>> = items("CAI_WORLD_THEATRES").iter().filter_map(|it| Some((component_of(it)?, arr(&rec(it, "THEATRE")?, 1)))).collect();
    let factions: BTreeMap<u32, (Vec<u32>, Vec<u32>)> = items("CAI_WORLD_FACTIONS")
        .iter()
        .filter_map(|it| {
            let f = rec(it, "CAI_FACTION")?;
            Some((component_of(it)?, (arr(&f, 3), arr(&f, 4))))
        })
        .collect();
    // Character positions by component.
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").expect("world");
    let mut pos: BTreeMap<u32, (i32, i32)> = BTreeMap::new();
    w.walk(&mut |r| {
        if r.name == "CHARACTER"
            && let (Some(id), Some(l)) = (r.get_i32(2), r.children.first().and_then(EsfNode::as_record))
        {
            pos.insert(id as u32, (l.get_i32(0).unwrap_or(0), l.get_i32(1).unwrap_or(0)));
        }
    });
    let char_game: BTreeMap<u32, u32> = items("CAI_WORLD_CHARACTERS").iter().filter_map(|it| Some((component_of(it)?, rec(it, "CAI_CHARACTER")?.get_u32(3)?))).collect();
    for it in items("CAI_WORLD_RESOURCE_MOBILES") {
        let Some(c) = component_of(it) else { continue };
        let Some(m) = rec(it, "CAI_RESOURCE_MOBILE") else { continue };
        let kind = if m.get_u32(10).unwrap_or(0) != 0 { "force" } else { "agent" };
        let owner = it.first().and_then(EsfNode::as_record).map(|o| (o.name.clone(), o.get_u32(0).unwrap_or(0)));
        if let Some((oname, of)) = &owner {
            t(&format!("{kind} mobile: item #0 is {oname}"), true);
            t(&format!("{kind} mobile: in CAI_FACTION #3 of its owner"), factions.get(of).is_some_and(|f| f.0.contains(&c)));
        }
        let Some(s) = rec(it, "CAI_SITUATED") else {
            t(&format!("{kind} mobile: has CAI_SITUATED"), false);
            continue;
        };
        t(&format!("{kind} mobile: has CAI_SITUATED"), true);
        let leader = m.get_u32(0).and_then(|l| char_game.get(&l)).and_then(|g| pos.get(g)).copied();
        let sp = (s.get_i32(0).unwrap_or(0), s.get_i32(1).unwrap_or(0));
        t(&format!("{kind} mobile: SITUATED #0/#1 == leader LOCOMOTABLE #0/#1"), leader == Some(sp));
        let r = s.get_u32(2).unwrap_or(0);
        t(&format!("{kind} mobile: SITUATED #2 region lists it in #9"), regions.get(&r).is_some_and(|l| l.contains(&c)));
        t(&format!("{kind} mobile: SITUATED #2 is a region"), regions.contains_key(&r));
        let th = arr(&s, 3);
        t(&format!("{kind} mobile: every SITUATED #3 theatre lists it in #1"), !th.is_empty() && th.iter().all(|x| theatres.get(x).is_some_and(|l| l.contains(&c))));
    }
    // Settlements: CAI_GARRISONABLE #0 against the residence's garrison (#12 force -> mobile).
    let mut residence_force: BTreeMap<u32, u32> = BTreeMap::new();
    w.walk(&mut |r| {
        if r.name == "SIEGEABLE_GARRISON_RESIDENCE" {
            residence_force.insert(r.get_u32(1).unwrap_or(0), r.get_u32(12).unwrap_or(0));
        }
    });
    let force_mobile: BTreeMap<u32, u32> = items("CAI_WORLD_RESOURCE_MOBILES")
        .iter()
        .filter_map(|it| Some((rec(it, "CAI_RESOURCE_MOBILE")?.get_u32(10)?, component_of(it)?)))
        .filter(|(g, _)| *g != 0)
        .collect();
    for it in items("CAI_WORLD_SETTLEMENTS") {
        let (Some(s), Some(g)) = (rec(it, "CAI_SETTLEMENT"), rec(it, "CAI_GARRISONABLE")) else { continue };
        let res = s.get_u32(2).unwrap_or(0);
        let want = residence_force.get(&res).and_then(|f| force_mobile.get(f)).copied().unwrap_or(0);
        t("settlement: GARRISONABLE #0 == mobile of the residence's garrison (0 = none)", g.get_u32(0) == Some(want));
        t("settlement: GARRISONABLE #1 == 0", g.get_u32(1) == Some(0));
    }
    // Slots: the set of CAI_GARRISONABLE #0 over the slot mirrors against the mobiles of the
    // forces garrisoned in slot residences (ARMY #5 = a REGION_SLOT residence).
    let mut slot_res: BTreeSet<u32> = BTreeSet::new();
    w.walk(&mut |r| {
        if r.name == "REGION_SLOT"
            && let Some(g) = r.child("SIEGEABLE_GARRISON_RESIDENCE")
        {
            slot_res.insert(g.get_u32(1).unwrap_or(0));
        }
    });
    let mut want: BTreeSet<u32> = BTreeSet::new();
    w.walk(&mut |r| {
        if r.name == "ARMY"
            && slot_res.contains(&r.get_u32(5).unwrap_or(0))
            && let Some(m) = r.child("MILITARY_FORCE").and_then(|m| m.get_u32(0)).and_then(|f| force_mobile.get(&f))
        {
            want.insert(*m);
        }
    });
    let have: BTreeSet<u32> = items("CAI_WORLD_REGION_SLOTS").iter().filter_map(|it| rec(it, "CAI_GARRISONABLE")?.get_u32(0)).filter(|&v| v != 0).collect();
    println!("slot garrisons: mirrors name {:?}, world {:?}", have, want);
    t("slots: set of GARRISONABLE #0 == mobiles of slot garrisons", have == want);
    let in_regions: usize = regions.values().map(Vec::len).sum();
    let in_theatres: usize = theatres.values().map(Vec::len).sum();
    println!("region #9 entries {in_regions}, theatre #1 entries {in_theatres}");
    for it in items("CAI_WORLD_CHARACTERS") {
        let Some(c) = component_of(it) else { continue };
        let owner = it.first().and_then(EsfNode::as_record).map(|o| o.get_u32(0).unwrap_or(0)).unwrap_or(0);
        t("character: in CAI_FACTION #4 of its OWNED_INDIRECT faction", factions.get(&owner).is_some_and(|f| f.1.contains(&c)));
    }
    for (k, (n, ok)) in tally {
        println!("{ok:6} / {n:6}  {k}");
    }
}

/// Dumps the first world mirror of `list` present in NEW but not in OLD (a mirror the original
/// created between the two saves).
pub fn cmd_newmirror(a: &EsfFile, b: &EsfFile, list: &str, nth: usize) {
    let ids_a: BTreeSet<u32> = ntw_campaign::cai::components(a.root.find_path(CAI).expect("cai")).iter().map(|c| c.id).collect();
    let w = b.root.find_path(&format!("{CAI}/CAI_WORLD")).expect("w");
    let Some(arr) = w.record_array(list) else { return };
    let fresh: Vec<&Vec<EsfNode>> = arr.items.iter().filter(|it| component_of(it).is_some_and(|c| !ids_a.contains(&c))).collect();
    println!("{} new of {}", fresh.len(), arr.items.len());
    if let Some(it) = fresh.get(nth) {
        for (i, n) in it.iter().enumerate() {
            match n {
                EsfNode::Record(r) => {
                    println!("#{i} {{{}}}", r.name);
                    for (k, c) in r.children.iter().enumerate() {
                        println!("    #{k} {c:?}");
                    }
                }
                other => println!("#{i} {other:?}"),
            }
        }
    }
}

/// How the block's scalar counters (offsets 10, 11, 15, 16) relate to its lists.
pub fn cmd_counters(esf: &EsfFile) {
    fn go(nodes: &[EsfNode], t: &mut BTreeMap<String, (usize, usize)>) {
        for p in ntw_campaign::cai::block_positions(nodes) {
            let u = |o: usize| nodes.get(p + o).and_then(EsfNode::as_u32).unwrap_or(u32::MAX) as usize;
            let l = |o: usize| nodes.get(p + o).and_then(EsfNode::as_u32_array).map_or(usize::MAX, <[u32]>::len);
            let owns = match nodes.get(p + 14) {
                Some(EsfNode::RecordArray(a)) => a.items.len(),
                _ => usize::MAX,
            };
            let mut f = |k: &str, ok: bool| {
                let e = t.entry(k.to_string()).or_default();
                e.0 += 1;
                e.1 += usize::from(ok);
            };
            f("off10 == 0", u(10) == 0);
            f("off11 == len(off9)/2", u(11) == l(9) / 2);
            f("off11 == owns", u(11) == owns);
            f("off11 == len(off13)", u(11) == l(13));
            f("off11 == len(off12)+len(off13)", u(11) == l(12) + l(13));
            f("off10 == len(off12)", u(10) == l(12));
            f("off10 == len(off8)", u(10) == l(8));
            f("off15 == len(off17)", u(15) == l(17));
            f("off16 == len(off18)", u(16) == l(18));
            f("off15 == 0", u(15) == 0);
            f("off16 == 0", u(16) == 0);
            f("len(off19) == len(off20)", l(19) == l(20));
            f("len(off9)/2 == owns", l(9) / 2 == owns);
        }
        for n in nodes {
            match n {
                EsfNode::Record(r) => go(&r.children, t),
                EsfNode::RecordArray(a) => a.items.iter().for_each(|it| go(it, t)),
                _ => {}
            }
        }
    }
    let mut t = BTreeMap::new();
    go(&esf.root.find_path(CAI).expect("cai").children, &mut t);
    for (k, (n, ok)) in t {
        println!("{ok:7} / {n:7}  {k}");
    }
}

/// Sample of block shapes: list lengths and counters.
pub fn cmd_shapes(esf: &EsfFile, every: usize) {
    let mut k = 0usize;
    fn go(nodes: &[EsfNode], k: &mut usize, every: usize) {
        for p in ntw_campaign::cai::block_positions(nodes) {
            *k += 1;
            if !(*k).is_multiple_of(every) {
                continue;
            }
            let u = |o: usize| nodes.get(p + o).and_then(EsfNode::as_u32).unwrap_or(u32::MAX);
            let l = |o: usize| nodes.get(p + o).and_then(EsfNode::as_u32_array).map_or(usize::MAX, <[u32]>::len);
            let owns = match nodes.get(p + 14) {
                Some(EsfNode::RecordArray(a)) => a.items.len(),
                _ => usize::MAX,
            };
            println!(
                "l8 {:3} l9 {:4} o10 {:4} o11 {:4} l12 {:3} l13 {:4} owns {:4} o15 {:3} o16 {:4} l17 {:3} l18 {:4} l19 {:4} l20 {:4}",
                l(8), l(9), u(10), u(11), l(12), l(13), owns, u(15), u(16), l(17), l(18), l(19), l(20)
            );
        }
        for n in nodes {
            match n {
                EsfNode::Record(r) => go(&r.children, k, every),
                EsfNode::RecordArray(a) => a.items.iter().for_each(|it| go(it, k, every)),
                _ => {}
            }
        }
    }
    go(&esf.root.find_path(CAI).expect("cai").children, &mut k, every);
}

/// How `BLOCK_OWNS` items relate to their component and to the pair lists of the components
/// they name.
pub fn cmd_owns(esf: &EsfFile) {
    let cai = esf.root.find_path(CAI).expect("cai");
    let comps = ntw_campaign::cai::components(cai);
    let by: BTreeMap<u32, &ntw_campaign::cai::Component> = comps.iter().map(|c| (c.id, c)).collect();
    let mut t: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut f = |k: &'static str, ok: bool| {
        let e = t.entry(k).or_default();
        e.0 += 1;
        e.1 += usize::from(ok);
    };
    let mut shown = 0;
    for c in &comps {
        for o in &c.owns {
            let pairs_of = |id: u32| by.get(&id).map(|x| x.lists_a[1].chunks(2).map(|p| p[0]).collect::<Vec<_>>()).unwrap_or_default();
            f("item #0 has C in its pair list", pairs_of(o.0).contains(&c.id));
            f("item #1 has C in its pair list", pairs_of(o.1).contains(&c.id));
            f("item #0 == item #1", o.0 == o.1);
            f("C's pair list has item #0", pairs_of(c.id).contains(&o.0));
            f("C's pair list has item #1", pairs_of(c.id).contains(&o.1));
            f("slot (#4) is 0 or 1", o.4 <= 1);
            if shown < 5 {
                shown += 1;
                println!("C {} ({}) owns {:?}; C pairs {:?}", c.id, c.class, o, &c.lists_a[1][..c.lists_a[1].len().min(10)]);
            }
        }
    }
    for (k, (n, ok)) in t {
        println!("{ok:7} / {n:7}  {k}");
    }
}

/// The link-counter check (`cai::check_link_counts`) on a save.
pub fn cmd_links(esf: &EsfFile) {
    let cai = esf.root.find_path(CAI).expect("cai");
    let r = ntw_campaign::cai::check_link_counts(cai, 10);
    println!("{} lines", r.len());
    for l in r {
        println!("  {l}");
    }
}

/// The mirror rules (`cai_world::check`) on saves.
pub fn cmd_mirrors_check(files: &[String]) {
    for p in files {
        let esf = EsfFile::open(p).expect("open");
        let r = ntw_campaign::cai_world::check(&esf, 10);
        println!("{p}: {} lines", r.len());
        for l in r {
            println!("   {l}");
        }
    }
}

/// The block lists at offsets 19 and 20: symmetry, and what owns the world mirrors.
pub fn cmd_owned(esf: &EsfFile) {
    let cai = esf.root.find_path(CAI).expect("cai");
    let comps = ntw_campaign::cai::components(cai);
    let by: BTreeMap<u32, &ntw_campaign::cai::Component> = comps.iter().map(|c| (c.id, c)).collect();
    let (mut n, mut sym) = (0, 0);
    for c in &comps {
        for &x in &c.lists_b[3] {
            n += 1;
            sym += usize::from(by.get(&x).is_some_and(|o| o.lists_b[2].contains(&c.id)));
        }
    }
    println!("offset 20 entries {n}, mirrored in the target's offset 19: {sym}");
    let (mut n2, mut sym2) = (0, 0);
    for c in &comps {
        for &x in &c.lists_b[2] {
            n2 += 1;
            sym2 += usize::from(by.get(&x).is_some_and(|o| o.lists_b[3].contains(&c.id)));
        }
    }
    println!("offset 19 entries {n2}, mirrored in the target's offset 20: {sym2}");
    for class in ["CAI_CHARACTER", "CAI_UNIT", "CAI_RESOURCE_MOBILE", "CAI_FACTION", "CAI_REGION", "CAI_BDI_NEW_TURN", "CAI_HISTORY"] {
        let mut owners: BTreeMap<String, usize> = BTreeMap::new();
        let mut cnt = 0;
        for c in comps.iter().filter(|c| c.class == class) {
            cnt += 1;
            let mut o: Vec<String> = c.lists_b[2].iter().map(|x| by.get(x).map_or("?".to_string(), |o| o.class.clone())).collect();
            o.sort();
            *owners.entry(format!("{o:?} / owns {}", c.lists_b[3].len())).or_default() += 1;
        }
        println!("{class} ({cnt}):");
        for (k, v) in owners.iter().take(8) {
            println!("    {v:6} offset19 = {k}");
        }
    }
}

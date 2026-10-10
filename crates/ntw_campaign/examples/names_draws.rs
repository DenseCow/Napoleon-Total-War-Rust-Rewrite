//! Research helper: the names each faction's allocators drew between two saves of one game
//! (deck entries gone, through a refill when the seed moved), mapped through `names::pool_rows`,
//! and where they turn up in the later save: a new character's names (`CHARACTER_DETAILS` #1/#2)
//! or a unit's officer names (`UNIT/COMMANDER_DETAILS` #0/#1).
use std::collections::BTreeMap;
use std::path::PathBuf;
use ntw_campaign::names::{self, read_allocator, Allocator};
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;

fn loc(r: Option<&EsfNode>) -> String {
    r.and_then(EsfNode::as_record).and_then(|r| r.get_str(0)).unwrap_or("").to_string()
}

struct Fac {
    allocs: Vec<Allocator>,
    /// (forename, surname) of characters and of unit officers, multiset.
    chars: BTreeMap<(String, String), i32>,
    officers: BTreeMap<(String, String), i32>,
}

fn factions(esf: &EsfFile) -> BTreeMap<String, Fac> {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let mut out = BTreeMap::new();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        let allocs = f.children_named("NAME_ALLOCATION_DETAILS").filter_map(read_allocator).collect();
        let mut chars = BTreeMap::new();
        for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
            if let Some(d) = c.children.get(1).and_then(EsfNode::as_record) {
                *chars.entry((loc(d.children.get(1)), loc(d.children.get(2)))).or_insert(0) += 1;
            }
        }
        let mut officers = BTreeMap::new();
        f.walk(&mut |r: &EsfRecord| {
            if r.name == "COMMANDER_DETAILS" {
                *officers.entry((loc(r.children.first()), loc(r.children.get(1)))).or_insert(0) += 1;
            }
        });
        out.insert(key, Fac { allocs, chars, officers });
    }
    out
}

fn drawn(a: &Allocator, b: &Allocator) -> Option<Vec<u32>> {
    if a.seed == b.seed {
        let gone: Vec<u32> = a.deck.iter().copied().filter(|x| !b.deck.contains(x)).collect();
        return Some(gone);
    }
    // Refilled (maybe more than once: only once is handled).
    let mut s = a.clone();
    s.deck.clear();
    s.size = b.size;
    let full = {
        let mut t = s.clone();
        t.draw()?;
        let mut d = vec![0u32];
        d.clear();
        let _ = &mut d;
        t
    };
    if full.seed != b.seed {
        return None;
    }
    let deck = Allocator::shuffled(b.size, b.seed);
    if !deck.ends_with(&b.deck) {
        return None;
    }
    let mut out = a.deck.clone();
    out.extend_from_slice(&deck[..deck.len() - b.deck.len()]);
    Some(out)
}

fn main() {
    let dir = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data");
    let db = GameDatabase::from_install(&dir).expect("db");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let rows = names::read_names(&vfs).expect("names");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = std::env::var("V").is_ok();
    if std::env::var("CONSTRAINTS").is_ok() {
        let pairs: Vec<(EsfFile, EsfFile)> = args.chunks(2).map(|c| (EsfFile::open(&c[0]).unwrap(), EsfFile::open(&c[1]).unwrap())).collect();
        constraints(&pairs, &rows, &db);
        return;
    }
    if let Ok(f) = std::env::var("POS") {
        positions(&EsfFile::open(&args[0]).unwrap(), &EsfFile::open(&args[1]).unwrap(), &rows, &db, &f);
        return;
    }
    if let Ok(f) = std::env::var("ALL") {
        all_pools(&EsfFile::open(&args[0]).unwrap(), &EsfFile::open(&args[1]).unwrap(), &f);
        return;
    }
    if let Ok(f) = std::env::var("SEARCH") {
        order_search(&EsfFile::open(&args[0]).unwrap(), &EsfFile::open(&args[1]).unwrap(), &rows, &db, &f);
        return;
    }
    if let Ok(f) = std::env::var("FACTION") {
        officers_dump(&EsfFile::open(&args[0]).unwrap(), &EsfFile::open(&args[1]).unwrap(), &rows, &db, &f);
        return;
    }
    if std::env::var("DUMP").is_ok() {
        pairs_dump(&EsfFile::open(&args[0]).unwrap(), &EsfFile::open(&args[1]).unwrap(), &rows, &db);
        return;
    }
    for w in args.windows(2) {
        println!("== {} -> {}", w[0], w[1]);
        let (x, y) = (factions(&EsfFile::open(&w[0]).unwrap()), factions(&EsfFile::open(&w[1]).unwrap()));
        let mut tally: BTreeMap<String, i32> = BTreeMap::new();
        for (key, fy) in &y {
            let Some(fx) = x.get(key) else { continue };
            let Some(group) = names::faction_group(&db, key) else { continue };
            // New (forename, surname) pairs in y.
            let new_of = |a: &BTreeMap<(String, String), i32>, b: &BTreeMap<(String, String), i32>| -> BTreeMap<(String, String), i32> {
                b.iter().filter_map(|(k, n)| { let d = n - a.get(k).copied().unwrap_or(0); (d > 0).then(|| (k.clone(), d)) }).collect()
            };
            let new_chars = new_of(&fx.chars, &fy.chars);
            let new_off = new_of(&fx.officers, &fy.officers);
            for p in [0usize, 4, 5] {
                let (Some(a), Some(b)) = (fx.allocs.get(p), fy.allocs.get(p)) else { continue };
                let variant = std::env::var("ORDER").unwrap_or_else(|_| "table".into());
                let Some(pool) = ordered(&rows, &group, p, &variant) else { continue };
                if pool.len() as u32 != b.size {
                    *tally.entry(format!("pool {p} size mismatch")).or_default() += 1;
                    continue;
                }
                let Some(d) = drawn(a, b) else {
                    *tally.entry(format!("pool {p} not explained")).or_default() += 1;
                    continue;
                };
                for i in d {
                    let Some(r) = pool.get(i as usize) else { continue };
                    let k = r.loc_key();
                    let side = |m: &BTreeMap<(String, String), i32>| m.keys().any(|(f, s)| if p == 0 { *f == k } else { *s == k });
                    let (c, o) = (side(&new_chars), side(&new_off));
                    let where_ = match (c, o) { (true, true) => "char+officer", (true, false) => "char", (false, true) => "officer", _ => "nowhere" };
                    *tally.entry(format!("pool {p} drawn -> {where_}")).or_default() += 1;
                    if verbose && where_ == "nowhere" {
                        println!("  {key} pool {p} drew {k} not found");
                    }
                }
            }
            *tally.entry("new characters".into()).or_default() += new_chars.values().sum::<i32>();
            *tally.entry("new officer pairs".into()).or_default() += new_off.values().sum::<i32>();
        }
        for (k, n) in tally {
            println!("{n:6} {k}");
        }
    }
}

/// The pool in one candidate order (research): `variant` = table | interleave | id | id_interleave
/// | name | rev | group_by_gender.
fn ordered<'a>(rows: &'a [names::NameRow], group: &str, p: usize, variant: &str) -> Option<Vec<&'a names::NameRow>> {
    let flat = names::pool_rows(rows, group, p)?;
    let mut uniq: Vec<&names::NameRow> = Vec::new();
    for r in &flat {
        if uniq.last().is_none_or(|u| !std::ptr::eq(*u, *r)) {
            uniq.push(r);
        }
    }
    let num = |r: &names::NameRow| r.id.parse::<i64>().unwrap_or(i64::MAX);
    let repeat = |v: &[&'a names::NameRow]| -> Vec<&'a names::NameRow> { v.iter().flat_map(|r| std::iter::repeat_n(*r, r.weight as usize)).collect() };
    let interleave = |v: &[&'a names::NameRow]| -> Vec<&'a names::NameRow> {
        let max = v.iter().map(|r| r.weight).max().unwrap_or(0);
        (1..=max).flat_map(|w| v.iter().filter(move |r| r.weight >= w).copied()).collect()
    };
    Some(match variant {
        "table" => flat,
        "interleave" => interleave(&uniq),
        "id" => {
            let mut u = uniq.clone();
            u.sort_by_key(|r| num(r));
            repeat(&u)
        }
        "id_interleave" => {
            let mut u = uniq.clone();
            u.sort_by_key(|r| num(r));
            interleave(&u)
        }
        "name" => {
            let mut u = uniq.clone();
            u.sort_by(|a, b| a.name.cmp(&b.name));
            repeat(&u)
        }
        "rev" => flat.into_iter().rev().collect(),
        "group_by_gender" => {
            let mut u = uniq.clone();
            u.sort_by_key(|r| r.gender);
            repeat(&u)
        }
        _ => return None,
    })
}

/// Research: per faction, the drawn deck indices (pools 0 and 4) and the new characters (by id,
/// with their forename / surname table positions in the flat pool) of one save pair.
pub fn pairs_dump(x: &EsfFile, y: &EsfFile, rows: &[names::NameRow], db: &GameDatabase) {
    let chars = |e: &EsfFile| -> BTreeMap<String, Vec<(i32, String, String, String)>> {
        let w = e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
        let mut out = BTreeMap::new();
        for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
            let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
            let mut v = Vec::new();
            for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
                let d = c.children.get(1).and_then(EsfNode::as_record).unwrap();
                v.push((c.get_i32(2).unwrap_or(0), c.get_str(3).unwrap_or("").to_string(), loc(d.children.get(1)), loc(d.children.get(2))));
            }
            out.insert(key, v);
        }
        out
    };
    let (cx, cy) = (chars(x), chars(y));
    let (fx, fy) = (factions(x), factions(y));
    for (key, list) in &cy {
        let old: std::collections::BTreeSet<i32> = cx.get(key).map(|v| v.iter().map(|c| c.0).collect()).unwrap_or_default();
        let new: Vec<_> = list.iter().filter(|c| !old.contains(&c.0)).collect();
        let Some(group) = names::faction_group(db, key) else { continue };
        let (Some(a), Some(b)) = (fx.get(key), fy.get(key)) else { continue };
        let d0 = a.allocs.first().zip(b.allocs.first()).and_then(|(p, q)| drawn(p, q));
        let d4 = a.allocs.get(4).zip(b.allocs.get(4)).and_then(|(p, q)| drawn(p, q));
        if new.is_empty() && d0.as_ref().is_none_or(|d| d.is_empty()) {
            continue;
        }
        let pool0 = names::pool_rows(rows, &group, 0).unwrap_or_default();
        let pool4 = names::pool_rows(rows, &group, 4).unwrap_or_default();
        let pos = |pool: &[&names::NameRow], k: &str| -> Vec<usize> { pool.iter().enumerate().filter(|(_, r)| r.loc_key() == k).map(|(i, _)| i).collect() };
        println!("{key} ({group}): pool sizes {} / {} stored {:?} / {:?}", pool0.len(), pool4.len(), b.allocs.first().map(|q| q.size), b.allocs.get(4).map(|q| q.size));
        println!("  drawn pool 0: {d0:?}");
        println!("  drawn pool 4: {d4:?}");
        for c in new {
            println!("  new {} {:10} {} {:?} / {} {:?}", c.0, c.1, c.2, pos(&pool0, &c.2), c.3, pos(&pool4, &c.3));
        }
    }
}

/// Research: new officer / character name pairs (multiset difference) of one faction, with the
/// positions of each name in pools 0 and 4, next to the drawn indices.
pub fn officers_dump(x: &EsfFile, y: &EsfFile, rows: &[names::NameRow], db: &GameDatabase, faction: &str) {
    let (fx, fy) = (factions(x), factions(y));
    let (Some(a), Some(b)) = (fx.get(faction), fy.get(faction)) else { return };
    let group = names::faction_group(db, faction).unwrap();
    let pool0 = names::pool_rows(rows, &group, 0).unwrap_or_default();
    let pool4 = names::pool_rows(rows, &group, 4).unwrap_or_default();
    let pos = |pool: &[&names::NameRow], k: &str| -> Vec<usize> { pool.iter().enumerate().filter(|(_, r)| r.loc_key() == k).map(|(i, _)| i).collect() };
    println!("drawn 0: {:?}", drawn(&a.allocs[0], &b.allocs[0]));
    println!("drawn 4: {:?}", drawn(&a.allocs[4], &b.allocs[4]));
    println!("deck 0 before (first 12): {:?}", &a.allocs[0].deck[..a.allocs[0].deck.len().min(12)]);
    println!("deck 4 before (first 12): {:?}", &a.allocs[4].deck[..a.allocs[4].deck.len().min(12)]);
    for (what, ma, mb) in [("officer", &a.officers, &b.officers), ("character", &a.chars, &b.chars)] {
        for ((f, s), n) in mb {
            let d = n - ma.get(&(f.clone(), s.clone())).copied().unwrap_or(0);
            if d > 0 {
                println!("  new {what} x{d}: {f} {:?} / {s} {:?}", pos(&pool0, f), pos(&pool4, s));
            }
        }
    }
}

/// Research: for many candidate orders, how many drawn indices land on a name that a new officer
/// or character of the faction carries (forenames: pool 0, surnames: pool 4).
pub fn order_search(x: &EsfFile, y: &EsfFile, rows: &[names::NameRow], db: &GameDatabase, faction: &str) {
    let (fx, fy) = (factions(x), factions(y));
    let (Some(a), Some(b)) = (fx.get(faction), fy.get(faction)) else { return };
    let group = names::faction_group(db, faction).unwrap();
    let mut new0 = std::collections::BTreeSet::new();
    let mut new4 = std::collections::BTreeSet::new();
    for (ma, mb) in [(&a.officers, &b.officers), (&a.chars, &b.chars)] {
        for ((f, s), n) in mb {
            if n - ma.get(&(f.clone(), s.clone())).copied().unwrap_or(0) > 0 {
                new0.insert(f.clone());
                new4.insert(s.clone());
            }
        }
    }
    let d0 = drawn(&a.allocs[0], &b.allocs[0]).unwrap_or_default();
    let d4 = drawn(&a.allocs[4], &b.allocs[4]).unwrap_or_default();
    for (p, d, new) in [(0usize, &d0, &new0), (4, &d4, &new4)] {
        let base = names::pool_rows(rows, &group, p).unwrap_or_default();
        let n = base.len();
        let mut best = Vec::new();
        // Orders: a sort key, then optionally reversed; and plain offsets / strides of the table.
        type Key = Box<dyn Fn(&names::NameRow) -> String>;
        let keys: Vec<(&str, Key)> = vec![
            ("table", Box::new(|_r| String::new())),
            ("name", Box::new(|r| r.name.clone())),
            ("id", Box::new(|r| format!("{:012}", r.id.parse::<i64>().unwrap_or(0)))),
            ("idstr", Box::new(|r| r.id.clone())),
            ("idu32", Box::new(|r| format!("{:010}", r.id.parse::<i64>().unwrap_or(0) as i32 as u32))),
            ("namelower_group", Box::new(|r| r.loc_key().to_lowercase())),
            ("gender", Box::new(|r| r.gender.to_string())),
            ("gender_rev", Box::new(|r| if r.gender == 'm' { "a".into() } else { "b".into() })),
            ("weight", Box::new(|r| format!("{:04}", 100 - r.weight))),
            ("namelen", Box::new(|r| format!("{:04}", r.name.len()))),
            ("lower", Box::new(|r| r.name.to_lowercase())),
            ("utf16", Box::new(|r| r.name.encode_utf16().map(|c| format!("{c:05}")).collect())),
        ];
        for (kname, key) in &keys {
            for rev in [false, true] {
                let mut uniq: Vec<&names::NameRow> = Vec::new();
                for r in &base {
                    if uniq.last().is_none_or(|u| !std::ptr::eq(*u, *r)) {
                        uniq.push(r);
                    }
                }
                uniq.sort_by_key(|r| key(r));
                if rev {
                    uniq.reverse();
                }
                let pool: Vec<&names::NameRow> = uniq.iter().flat_map(|r| std::iter::repeat_n(*r, r.weight as usize)).collect();
                let hits = d.iter().filter(|&&i| pool.get(i as usize).is_some_and(|r| new.contains(&r.loc_key()))).count();
                best.push((hits, format!("{kname}{}", if rev { " rev" } else { "" })));
            }
        }
        for off in 1..n {
            let hits = d.iter().filter(|&&i| base.get((i as usize + off) % n).is_some_and(|r| new.contains(&r.loc_key()))).count();
            if hits >= 3 {
                best.push((hits, format!("offset {off}")));
            }
        }
        best.sort_by_key(|b| std::cmp::Reverse(b.0));
        println!("pool {p}: {} draws, {} new names; best {:?}", d.len(), new.len(), &best[..best.len().min(6)]);
    }
}

/// Research: every allocator of a faction whose state changed, with its drawn indices.
pub fn all_pools(x: &EsfFile, y: &EsfFile, faction: &str) {
    let (fx, fy) = (factions(x), factions(y));
    let (Some(a), Some(b)) = (fx.get(faction), fy.get(faction)) else { return };
    for (p, (q, r)) in a.allocs.iter().zip(&b.allocs).enumerate() {
        if q != r {
            println!("pool {p}: size {} -> {}, seed {} -> {}, deck {} -> {}, drawn {:?}", q.size, r.size, q.seed, r.seed, q.deck.len(), r.deck.len(), drawn(q, r));
        }
    }
}

/// Research: the new names' positions in several candidate lists, next to the drawn set.
pub fn positions(x: &EsfFile, y: &EsfFile, rows: &[names::NameRow], db: &GameDatabase, faction: &str) {
    let (fx, fy) = (factions(x), factions(y));
    let (Some(a), Some(b)) = (fx.get(faction), fy.get(faction)) else { return };
    let group = names::faction_group(db, faction).unwrap();
    let mut new0 = Vec::new();
    for ((f, s), n) in &b.officers {
        if n - a.officers.get(&(f.clone(), s.clone())).copied().unwrap_or(0) > 0 {
            new0.push((f.clone(), s.clone()));
        }
    }
    let mut d0 = drawn(&a.allocs[0], &b.allocs[0]).unwrap_or_default();
    let mut d4 = drawn(&a.allocs[4], &b.allocs[4]).unwrap_or_default();
    d0.sort();
    d4.sort();
    println!("drawn 0 sorted {d0:?}\ndrawn 4 sorted {d4:?}");
    let g: Vec<&names::NameRow> = rows.iter().filter(|r| r.group == group).collect();
    let lists: Vec<(&str, Vec<&names::NameRow>)> = vec![
        ("group all rows", g.clone()),
        ("group forenames", g.iter().copied().filter(|r| r.forename).collect()),
        ("group surnames", g.iter().copied().filter(|r| !r.forename).collect()),
        ("group weighted>0", g.iter().copied().filter(|r| r.weight > 0).collect()),
        ("pool0 unique", g.iter().copied().filter(|r| r.forename && !r.noble && r.gender != 'f' && r.weight > 0).collect()),
        ("pool4 unique", g.iter().copied().filter(|r| !r.forename && !r.noble && r.weight > 0).collect()),
        ("forenames m/b all weights", g.iter().copied().filter(|r| r.forename && r.gender != 'f').collect()),
    ];
    for (name, l) in &lists {
        let mut p0: Vec<usize> = new0.iter().filter_map(|(f, _)| l.iter().position(|r| r.loc_key() == *f)).collect();
        let mut p4: Vec<usize> = new0.iter().filter_map(|(_, s)| l.iter().position(|r| r.loc_key() == *s)).collect();
        p0.sort();
        p4.sort();
        println!("{name:28} (len {:4}): forenames {p0:?} surnames {p4:?}", l.len());
    }
    let ids: Vec<String> = new0.iter().map(|(f, s)| {
        let id = |k: &str| rows.iter().find(|r| r.loc_key() == k).map(|r| r.id.clone()).unwrap_or_default();
        format!("{}={} {}={}", f.trim_start_matches("names_name_"), id(f), s.trim_start_matches("names_name_"), id(s))
    }).collect();
    println!("ids: {ids:?}");
}

/// Research: per names group and pool (0 forenames, 4 surnames), each drawn index with the
/// intersection of the new-name sets of every faction pair that drew it (all save pairs given).
pub fn constraints(pairs: &[(EsfFile, EsfFile)], rows: &[names::NameRow], db: &GameDatabase) {
    use std::collections::BTreeSet;
    // (group, pool, index) -> candidate names
    let mut cand: BTreeMap<(String, usize, u32), BTreeSet<String>> = BTreeMap::new();
    for (x, y) in pairs {
        let (fx, fy) = (factions(x), factions(y));
        for (key, b) in &fy {
            let Some(a) = fx.get(key) else { continue };
            let Some(group) = names::faction_group(db, key) else { continue };
            let mut new0 = BTreeSet::new();
            let mut new4 = BTreeSet::new();
            for (ma, mb) in [(&a.officers, &b.officers), (&a.chars, &b.chars)] {
                for ((f, s), n) in mb {
                    if n - ma.get(&(f.clone(), s.clone())).copied().unwrap_or(0) > 0 {
                        new0.insert(f.clone());
                        new4.insert(s.clone());
                    }
                }
            }
            for (p, new) in [(0usize, &new0), (4, &new4)] {
                let (Some(qa), Some(qb)) = (a.allocs.get(p), b.allocs.get(p)) else { continue };
                let size = names::pool_rows(rows, &group, p).map_or(0, |v| v.len());
                if qa.size as usize != size || qb.size as usize != size {
                    continue;
                }
                let Some(d) = drawn(qa, qb) else { continue };
                for i in d {
                    let e = cand.entry((group.clone(), p, i)).or_insert_with(|| new.clone());
                    *e = e.intersection(new).cloned().collect();
                }
            }
        }
    }
    let mut solved = 0;
    for ((g, p, i), c) in &cand {
        let table = names::pool_rows(rows, g, *p).and_then(|v| v.get(*i as usize).map(|r| r.loc_key())).unwrap_or_default();
        if c.len() == 1 {
            solved += 1;
            let name = c.iter().next().unwrap();
            let pos: Vec<usize> = names::pool_rows(rows, g, *p).unwrap_or_default().iter().enumerate().filter(|(_, r)| r.loc_key() == *name).map(|(k, _)| k).collect();
            println!("{g} pool {p} index {i} -> {} (table positions {pos:?}; table order gives {})", name.trim_start_matches("names_name_"), table.trim_start_matches("names_name_"));
        }
    }
    println!("{} drawn indices, {solved} pinned to one name", cand.len());
}

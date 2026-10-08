//! Research helper: the land / naval unit name allocators between two saves of one game: per
//! faction and class, names whose in-use flag changed, list growth, the trailing "next" name, and
//! the regiment names of new units.
use std::collections::{BTreeMap, BTreeSet};
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
type Lists = BTreeMap<(String, String), (Vec<(String, bool)>, String)>;
fn loc(r: &EsfRecord) -> String {
    r.get_str(0).unwrap_or("").to_string()
}
fn class_alloc(c: &EsfRecord) -> (Vec<(String, bool)>, String) {
    let list = c
        .record_array("UNIT_CLASS_NAMES_LIST")
        .into_iter()
        .flat_map(|a| a.items.iter())
        .map(|it| (it.first().and_then(EsfNode::as_record).map(loc).unwrap_or_default(), it.get(1).and_then(EsfNode::as_bool).unwrap_or(false)))
        .collect();
    let next = c.child("CAMPAIGN_LOCALISATION").map(loc).unwrap_or_default();
    (list, next)
}
fn lists(esf: &EsfFile) -> Lists {
    let mut out = Lists::new();
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        if let Some(l) = f.child("LAND_UNIT_NAME_ALLOCATOR") {
            for it in l.record_array("LAND_UNIT_NAMES_MAP").into_iter().flat_map(|a| a.items.iter()) {
                let class = it.first().and_then(EsfNode::as_u32).unwrap_or(0);
                if let Some(c) = it.iter().filter_map(EsfNode::as_record).next() {
                    out.insert((key.clone(), format!("land {class}")), class_alloc(c));
                }
            }
        }
        if let Some(c) = f.child("NAVAL_UNIT_NAME_ALLOCATOR").and_then(|n| n.child("UNIT_CLASS_NAME_ALLOCATOR")) {
            out.insert((key.clone(), "naval".into()), class_alloc(c));
        }
    }
    out
}
fn unit_names(esf: &EsfFile) -> BTreeMap<i32, (String, String, String)> {
    let mut out = BTreeMap::new();
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        f.walk(&mut |r| {
            if r.name == "UNIT"
                && let Some(id) = r.get_i32(4)
            {
                let name = r.child("CAMPAIGN_LOCALISATION").map(loc).unwrap_or_default();
                let k = r.child("UNIT_RECORD_KEY").map(loc).unwrap_or_default();
                out.insert(id, (key.clone(), k, name));
            }
        });
    }
    out
}
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a[0] == "flags" {
        for p in &a[1..] {
            print!("{p}: ");
            flag_consistency(&EsfFile::open(p).unwrap());
        }
        return;
    }
    if a[0] == "cd" {
        let e = EsfFile::open(&a[1]).unwrap();
        let w = e.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
        let mut n = 0;
        let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
        w.walk(&mut |r| {
            if r.name == "COMMANDER_DETAILS" {
                let s = format!("{:?}", r.children.iter().map(|c| match c { EsfNode::Record(x) => format!("{}{:?}", x.name, x.values().map(|v| format!("{v:?}")).collect::<Vec<_>>()), o => format!("{o:?}") }).collect::<Vec<_>>());
                let blank = !s.contains("names_name");
                *kinds.entry(format!("blank={blank}")).or_default() += 1;
                if n < 4 {
                    n += 1;
                    println!("{s}");
                }
            }
        });
        println!("{kinds:?}");
        return;
    }
    if a[0] == "growth" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let db = ntw_data::GameDatabase::from_install(std::path::Path::new(dir)).expect("db");
        for w in a[1..].windows(2) {
            println!("== {} -> {}", w[0], w[1]);
            growth(&EsfFile::open(&w[0]).unwrap(), &EsfFile::open(&w[1]).unwrap(), &db);
        }
        return;
    }
    if a[0] == "layout" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let vfs = ntw_formats::pack::Vfs::open_install(dir).expect("vfs");
        let db = ntw_data::GameDatabase::from_install(std::path::Path::new(dir)).expect("db");
        layout(&EsfFile::open(&a[1]).unwrap(), &db, &vfs);
        return;
    }
    if a[0] == "extract" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let vfs = ntw_formats::pack::Vfs::open_install(dir).expect("vfs");
        std::fs::write(&a[2], vfs.read(&a[1]).unwrap()).unwrap();
        return;
    }
    if a[0] == "table" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let vfs = ntw_formats::pack::Vfs::open_install(dir).expect("vfs");
        let b = vfs.read(r"db\unit_regiment_names_tables\unit_regiment_names").unwrap();
        let t = ntw_formats::db::DbTable::read(&b, &ntw_formats::db::Schema::from_codes("s,s,s,i").unwrap()).unwrap();
        let mut g: BTreeMap<(String, String), Vec<(String, i32)>> = BTreeMap::new();
        for r in &t.rows {
            let s = |i: usize| match &r[i] { ntw_formats::db::DbValue::Str(x) => x.clone(), _ => String::new() };
            let n = match r[3] { ntw_formats::db::DbValue::I32(v) => v, _ => 0 };
            g.entry((s(0), s(1))).or_default().push((s(2), n));
        }
        for ((grp, class), v) in &g {
            let orders: Vec<i32> = v.iter().map(|x| x.1).collect();
            let sorted = orders.windows(2).all(|w| w[0] < w[1]);
            println!("{grp:16} {class:22} {:4} first {} last {} orders {}..{} ascending={sorted}", v.len(), v[0].0, v[v.len() - 1].0, orders.iter().min().unwrap(), orders.iter().max().unwrap());
        }
        return;
    }
    if a[0] == "has" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let vfs = ntw_formats::pack::Vfs::open_install(dir).expect("vfs");
        let b = vfs.read(&a[1]).unwrap();
        for s in &a[2..] {
            let w: Vec<u8> = s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
            let n = b.windows(w.len()).filter(|x| *x == &w[..]).count();
            let n8 = b.windows(s.len()).filter(|x| *x == s.as_bytes()).count();
            println!("{s}: utf16 {n}, utf8 {n8}");
        }
        let db = ntw_data::GameDatabase::from_install(std::path::Path::new(dir)).expect("db");
        println!("db units rows {}", db.units.len());
        return;
    }
    if a[0] == "vfs" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let vfs = ntw_formats::pack::Vfs::open_install(dir).expect("vfs");
        for p in &a[1..] {
            for f in vfs.list(p) {
                let (pack, e) = vfs.find(f).unwrap();
                println!("{f}  {}  {}", pack.path().display(), e.size);
            }
        }
        return;
    }
    if a[0] == "rule" {
        for w in a[1..].windows(2) {
            println!("== {} -> {}", w[0], w[1]);
            rule(&EsfFile::open(&w[0]).unwrap(), &EsfFile::open(&w[1]).unwrap());
        }
        return;
    }
    if a[0] == "classes" {
        let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
        let db = ntw_data::GameDatabase::from_install(std::path::Path::new(dir)).expect("db");
        for p in &a[1..] {
            class_map(&EsfFile::open(p).unwrap(), &db);
            odd_units(&EsfFile::open(p).unwrap(), &db);
        }
        return;
    }
    let (x, y) = (EsfFile::open(&a[0]).unwrap(), EsfFile::open(&a[1]).unwrap());
    let (lx, ly) = (lists(&x), lists(&y));
    let mut shown = 0;
    for (k, (l2, n2)) in &ly {
        let Some((l1, n1)) = lx.get(k) else { continue };
        if l1 == l2 && n1 == n2 {
            continue;
        }
        shown += 1;
        if shown > 12 {
            continue;
        }
        let flipped: Vec<String> = l2.iter().enumerate().filter(|(i, (_, b))| l1.get(*i).is_some_and(|o| o.1 != *b)).map(|(i, (n, b))| format!("[{i}]{}->{b}", n.rsplit('_').next().unwrap_or(""))).collect();
        println!("{k:?}: list {} -> {}, next {} -> {}, flags changed {:?}", l1.len(), l2.len(), n1.rsplit('_').next().unwrap_or(""), n2.rsplit('_').next().unwrap_or(""), flipped);
    }
    println!("{shown} allocators changed");
    let (ux, uy) = (unit_names(&x), unit_names(&y));
    let old: BTreeSet<&i32> = ux.keys().collect();
    let mut n = 0;
    for (id, (f, k, name)) in &uy {
        if !old.contains(id) && n < 15 {
            n += 1;
            println!("new unit {f} {k}: {name}");
        }
    }
}

/// Unit class string → allocator class number, from which list holds each unit's name.
pub fn class_map(esf: &EsfFile, db: &ntw_data::GameDatabase) {
    let l = lists(esf);
    let mut tally: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (_, (f, k, name)) in unit_names(esf) {
        if name.is_empty() {
            continue;
        }
        let class = db.unit(&k).map(|u| u.unit_class.clone()).unwrap_or_else(|| "?".into());
        let hit = l.iter().find(|((ff, _), (list, _))| *ff == f && list.iter().any(|(n, b)| *n == name && *b)).map(|((_, c), _)| c.clone());
        *tally.entry((class, hit.unwrap_or_else(|| "none".into()))).or_default() += 1;
    }
    for ((c, h), n) in tally {
        println!("{n:5}  {c:24} -> {h}");
    }
}

/// Units whose key the db does not know, or whose name sits in no in-use list.
pub fn odd_units(esf: &EsfFile, db: &ntw_data::GameDatabase) {
    let l = lists(esf);
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (_, (f, k, name)) in unit_names(esf) {
        let known = db.unit(&k).is_some();
        let hit = l.iter().any(|((ff, _), (list, _))| *ff == f && list.iter().any(|(n, b)| *n == name && *b));
        let any = l.iter().any(|((ff, _), (list, _))| *ff == f && list.iter().any(|(n, _)| *n == name));
        if !known || !hit {
            let tag = format!("{} known={known} inuse={hit} listed={any} {k} {}", f, name.rsplit("lookup_").next().unwrap_or(&name));
            *seen.entry(tag).or_default() += 1;
        }
    }
    for (t, n) in seen.iter().take(60) {
        println!("{n:3} {t}");
    }
}

/// For each unit new in `y`: which kind of name it has and, for allocator names, whether the
/// index was the lowest free one of its list in `x` (counting other new units of that list too).
pub fn rule(x: &EsfFile, y: &EsfFile) {
    let (lx, ly) = (lists(x), lists(y));
    let (ux, uy) = (unit_names(x), unit_names(y));
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    // Per list: indices newly true in y.
    for (k, (l2, _)) in &ly {
        let Some((l1, _)) = lx.get(k) else { continue };
        let freed: Vec<usize> = (0..l1.len()).filter(|&i| l1[i].1 && !l2.get(i).is_some_and(|e| e.1)).collect();
        let taken: Vec<usize> = (0..l2.len()).filter(|&i| l2[i].1 && !l1.get(i).is_some_and(|e| e.1)).collect();
        if taken.is_empty() && freed.is_empty() {
            continue;
        }
        let free1: Vec<usize> = (0..l1.len()).filter(|&i| !l1[i].1).collect();
        let lowest = free1.iter().take(taken.len()).copied().collect::<Vec<_>>() == taken;
        // Free-then-retake inside the turn hides some; allow taken ⊆ lowest (free1 ∪ freed).
        let mut pool: Vec<usize> = free1.iter().chain(freed.iter()).copied().collect();
        pool.sort();
        let lowest2 = pool.iter().take(taken.len()).copied().collect::<Vec<_>>() == taken;
        let tag = format!("{} lowest={lowest} lowest_with_freed={lowest2}", k.1.split(' ').next().unwrap_or(""));
        *kinds.entry(tag.clone()).or_default() += 1;
        if !lowest2 {
            println!("  {k:?} taken {taken:?} freed {freed:?} first free {:?}", &free1[..free1.len().min(8)]);
        }
    }
    for (id, (f, key, name)) in &uy {
        if ux.contains_key(id) {
            continue;
        }
        let kind = if name.is_empty() {
            "empty"
        } else if name.contains("units_on_screen_name_") {
            "on_screen"
        } else if ly.iter().any(|((ff, _), (l, _))| ff == f && l.iter().any(|(n, b)| n == name && *b)) {
            "allocated"
        } else {
            "other"
        };
        let genl = if key.starts_with("Gen_") { " (general)" } else { "" };
        *kinds.entry(format!("new unit {kind}{genl}")).or_default() += 1;
    }
    for (k, n) in kinds {
        println!("{n:5} {k}");
    }
}

fn read_table(vfs: &ntw_formats::pack::Vfs, path: &str, codes: &str) -> Vec<Vec<String>> {
    let b = vfs.read(path).unwrap();
    let t = ntw_formats::db::DbTable::read(&b, &ntw_formats::db::Schema::from_codes(codes).unwrap()).unwrap();
    t.rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|v| match v {
                    ntw_formats::db::DbValue::Str(x) => x.clone(),
                    ntw_formats::db::DbValue::OptStr(x) => x.clone().unwrap_or_default(),
                    ntw_formats::db::DbValue::I32(x) => x.to_string(),
                    other => format!("{other:?}"),
                })
                .collect()
        })
        .collect()
}

/// Per faction: names group, its land lists (class, length, first, next) and naval list makeup.
pub fn layout(esf: &EsfFile, db: &ntw_data::GameDatabase, vfs: &ntw_formats::pack::Vfs) {
    let classes = read_table(vfs, r"db\unit_class_tables\unit_class", "s,s");
    let ships = read_table(vfs, r"db\ship_names_tables\ship_names", "s,s,s,o");
    let l = lists(esf);
    let mut last = String::new();
    for ((f, c), (list, next)) in &l {
        let grp = db.faction(f).map(|r| r.character_names_group.clone()).unwrap_or_default();
        if *f != last {
            last = f.clone();
            let fr = db.faction(f);
            println!("{f} ({grp}) cat={:?} sub={:?} model={:?} v8={:?}", fr.map(|r| r.category.clone()), fr.map(|r| r.subculture.clone()), fr.map(|r| r.model_faction.clone()), fr.map(|r| format!("{:?}", r)).map(|s| s.len()));
        }
        let short = |s: &str| s.rsplit("lookup_").next().unwrap_or(s).to_string();
        if c == "naval" {
            let ids: Vec<String> = list.iter().map(|(n, _)| n.rsplit('_').next().unwrap_or("").to_string()).collect();
            let rows: Vec<&Vec<String>> = ids.iter().filter_map(|i| ships.iter().find(|r| r[0] == *i)).collect();
            let mut by: BTreeMap<String, usize> = BTreeMap::new();
            for r in &rows {
                *by.entry(format!("{}/{}", r[1], r[3])).or_default() += 1;
            }
            let g = ships.iter().filter(|r| r[1] == grp).count();
            let fk = ships.iter().filter(|r| r[3] == *f).count();
            let gf = ships.iter().filter(|r| r[1] == grp && (r[3].is_empty() || r[3] == *f)).count();
            println!("  naval {} (rows of group {g}, of faction {fk}, group & (no faction or own) {gf}) {:?} first ids {:?}", list.len(), by, &ids[..ids.len().min(6)]);
        } else {
            let n: usize = c[5..].parse().unwrap();
            let cname = classes.get(n).map(|r| r[0].clone()).unwrap_or_default();
            println!("  {c:8} {cname:22} {:4} first {} next {}", list.len(), list.first().map(|x| short(&x.0)).unwrap_or_default(), short(next));
        }
    }
}

/// Per (faction, unit key, name kind): unit counts that grew from `x` to `y`.
pub fn growth(x: &EsfFile, y: &EsfFile, db: &ntw_data::GameDatabase) {
    let kind = |n: &str| if n.is_empty() { "empty" } else if n.contains("units_on_screen_name_") { "on_screen" } else if n.contains("ship_names") { "ship" } else { "regiment" };
    let count = |e: &EsfFile| {
        let mut m: BTreeMap<(String, String, &'static str), i32> = BTreeMap::new();
        for (_, (f, k, n)) in unit_names(e) {
            *m.entry((f, k, kind(&n))).or_default() += 1;
        }
        m
    };
    let (cx, cy) = (count(x), count(y));
    let mut tot: BTreeMap<(&str, bool), i32> = BTreeMap::new();
    for (k, n) in &cy {
        let d = n - cx.get(k).copied().unwrap_or(0);
        if d > 0 {
            let indb = db.unit(&k.1).is_some();
            *tot.entry((k.2, indb)).or_default() += d;
            if k.2 == "on_screen" || (k.2 == "empty" && !k.1.starts_with("Gen_")) {
                println!("  +{d} {} {} {} class={:?}", k.2, k.0, k.1, db.unit(&k.1).map(|u| u.unit_class.clone()));
            }
        }
    }
    println!("{tot:?}");
}

/// Per save: flagged list names against the names the faction's units carry.
pub fn flag_consistency(esf: &EsfFile) {
    let l = lists(esf);
    let units = unit_names(esf);
    let mut by_faction: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for (_, (f, _, n)) in units {
        *by_faction.entry(f).or_default().entry(n).or_default() += 1;
    }
    let (mut flagged, mut flagged_unused, mut used_unflagged, mut dup) = (0, 0, 0, 0);
    for ((f, _), (list, _)) in &l {
        let have = by_faction.get(f).cloned().unwrap_or_default();
        for (n, b) in list {
            let c = have.get(n).copied().unwrap_or(0);
            if *b {
                flagged += 1;
                if c == 0 {
                    flagged_unused += 1;
                }
                if c > 1 {
                    dup += 1;
                }
            } else if c > 0 {
                used_unflagged += 1;
            }
        }
    }
    println!("flagged {flagged}: without a unit {flagged_unused}; carried by >1 unit {dup}; unflagged but carried {used_unflagged}");
}

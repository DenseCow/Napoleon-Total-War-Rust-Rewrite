//! Research helper: (faction, unit key) pairs present in NEW but not in OLD, and whether the
//! install's `units_to_exclusive_faction_permissions` allows each.
use std::collections::BTreeSet;
use ntw_formats::db::{DbTable, DbValue, Schema};
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;

fn pairs(esf: &EsfFile) -> BTreeSet<(String, String)> {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let mut out = BTreeSet::new();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        f.walk(&mut |r: &EsfRecord| {
            if r.name == "UNIT_RECORD_KEY"
                && let Some(s) = r.get_str(0)
            {
                out.insert((key.clone(), s.to_string()));
            }
        });
    }
    out
}

fn main() {
    let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
    let vfs = Vfs::open_install(dir).expect("vfs");
    let b = vfs.read(r"db\units_to_exclusive_faction_permissions_tables\units_to_exclusive_faction_permissions").unwrap();
    let t = DbTable::read(&b, &Schema::from_codes("s,s,b").unwrap()).unwrap();
    let s = |v: &DbValue| match v { DbValue::Str(x) => x.clone(), _ => String::new() };
    let allowed: BTreeSet<(String, String)> = t.rows.iter().filter(|r| matches!(r[2], DbValue::Bool(true))).map(|r| (s(&r[1]), s(&r[0]))).collect();
    let listed: BTreeSet<String> = t.rows.iter().map(|r| s(&r[0])).collect();
    let a: Vec<String> = std::env::args().skip(1).collect();
    let old = pairs(&EsfFile::open(&a[0]).unwrap());
    for p in &a[1..] {
        let new = pairs(&EsfFile::open(p).unwrap());
        println!("== {p}");
        for (f, k) in new.difference(&old) {
            let ok = allowed.contains(&(f.clone(), k.clone()));
            println!("  {f:20} {k:40} allowed={ok} unit_listed={}", listed.contains(k));
        }
    }
}

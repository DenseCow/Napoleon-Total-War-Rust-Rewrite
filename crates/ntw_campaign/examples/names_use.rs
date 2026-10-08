//! Research helper: which allocator pool each new character's forename and surname comes from,
//! by character type, between two saves of one game (characters new in NEW).
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use ntw_campaign::names;
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;
fn chars(esf: &EsfFile) -> Vec<(String, EsfRecord)> {
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let mut out = Vec::new();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        for c in f.record_array("CHARACTER_ARRAY").into_iter().flat_map(|a| a.records()).filter(|c| c.name == "CHARACTER") {
            out.push((key.clone(), c.clone()));
        }
    }
    out
}
fn main() {
    let dir = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data");
    let db = GameDatabase::from_install(&dir).expect("db");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let rows = names::read_names(&vfs).expect("names");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let old = EsfFile::open(&args[0]).unwrap();
    let new = EsfFile::open(&args[1]).unwrap();
    let old_ids: BTreeSet<i32> = chars(&old).iter().map(|(_, c)| c.get_i32(2).unwrap_or(0)).collect();
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for (fkey, c) in chars(&new) {
        if old_ids.contains(&c.get_i32(2).unwrap_or(0)) {
            continue;
        }
        let kind = c.get_str(3).unwrap_or("").to_string();
        let Some(group) = names::faction_group(&db, &fkey) else { continue };
        let d = c.children.get(1).and_then(EsfNode::as_record).unwrap();
        let loc = |i: usize| d.children.get(i).and_then(EsfNode::as_record).and_then(|r| r.get_str(0)).unwrap_or("").to_string();
        let (fore, sur) = (loc(1), loc(2));
        let which = |key: &str| -> String {
            if key.is_empty() {
                return "-".into();
            }
            let mut ps = Vec::new();
            for p in 0..6 {
                if names::pool_rows(&rows, &group, p).unwrap_or_default().iter().any(|r| r.loc_key() == key) {
                    ps.push(p.to_string());
                }
            }
            if ps.is_empty() { format!("none({})", key.trim_start_matches("names_name_")) } else { ps.join("/") }
        };
        *tally.entry(format!("{kind:12} forename pool {} surname pool {}", which(&fore), which(&sur))).or_default() += 1;
    }
    for (k, n) in tally {
        println!("{n:5}  {k}");
    }
}

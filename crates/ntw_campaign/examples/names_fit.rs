//! Research helper: which `names` filter (type, genders, noble flag; entries weighted by the i32
//! column) reproduces each stored allocator pool size, for every faction of the given saves.
use std::collections::BTreeMap;
use std::path::PathBuf;
use ntw_campaign::names;
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::pack::Vfs;
type Filter = Box<dyn Fn(&names::NameRow) -> bool>;

fn main() {
    let dir = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data");
    let db = GameDatabase::from_install(&dir).expect("db");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let rows = names::read_names(&vfs).expect("names");
    // Candidate filters.
    let mut cands: Vec<(String, Filter)> = Vec::new();
    for fore in [true, false] {
        for gs in ["m", "f", "b", "mb", "fb", "mfb"] {
            for noble in [Some(true), Some(false), None] {
                let g = gs.to_string();
                cands.push((
                    format!("{} {gs} noble={noble:?}", if fore { "forename" } else { "surname" }),
                    Box::new(move |r: &names::NameRow| r.forename == fore && g.contains(r.gender) && noble.is_none_or(|n| r.noble == n)),
                ));
            }
        }
    }
    let mut score: BTreeMap<(usize, usize, &str), (usize, usize)> = BTreeMap::new();
    let groups = ["character", "secondary"];
    for p in std::env::args().skip(1) {
        let esf = EsfFile::open(&p).unwrap();
        let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
        for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
            let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
            let Some(fr) = db.faction(&key) else { continue };
            let sizes: Vec<u32> = f.children_named("NAME_ALLOCATION_DETAILS").filter_map(|r| r.get_u32(0)).collect();
            for (k, &size) in sizes.iter().enumerate() {
                for (gi, gname) in groups.iter().enumerate() {
                    let group = if gi == 0 { &fr.character_names_group } else { &fr.secondary_names_group };
                    for (ci, (_, c)) in cands.iter().enumerate() {
                        let n: i64 = rows.iter().filter(|r| &r.group == group && r.weight > 0 && c(r)).map(|r| i64::from(r.weight)).sum();
                        let e = score.entry((k, ci, gname)).or_default();
                        e.0 += 1;
                        e.1 += usize::from(n == i64::from(size));
                    }
                }
            }
        }
    }
    for k in 0..10 {
        let mut best: Vec<(usize, usize, String)> = score.iter().filter(|((kk, _, _), _)| *kk == k).map(|((_, ci, g), (n, ok))| (*ok, *n, format!("{g} {}", cands[*ci].0))).collect();
        best.sort_by_key(|b| std::cmp::Reverse(b.0));
        println!("pool {k}: {:?}", &best[..3.min(best.len())]);
    }
}

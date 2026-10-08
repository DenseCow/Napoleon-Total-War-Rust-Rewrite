//! Research helper: the ten stored allocator sizes (and deck lengths) of every faction in a save.
use ntw_formats::esf::{EsfFile, EsfNode};
fn main() {
    for p in std::env::args().skip(1) {
        let esf = EsfFile::open(&p).unwrap();
        let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
        for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
            let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
            let s: Vec<String> = f
                .children_named("NAME_ALLOCATION_DETAILS")
                .map(|r| format!("{}/{}", r.get_u32(0).unwrap_or(0), match r.get(2) { Some(EsfNode::U16Array(v)) => v.len(), _ => 0 }))
                .collect();
            println!("{key:22} {}", s.join(" "));
        }
    }
}

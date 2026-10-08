//! Research helper: per faction its names groups and the stored allocator sizes.
use std::path::PathBuf;
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode};
fn main() {
    let dir = PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data");
    let db = GameDatabase::from_install(&dir).expect("db");
    let esf = EsfFile::open(std::env::args().nth(1).unwrap()).unwrap();
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        let sizes: Vec<u32> = f.children_named("NAME_ALLOCATION_DETAILS").filter_map(|r| r.get_u32(0)).collect();
        let r = db.faction(&key);
        println!("{key:24} {:28} {:28} {sizes:?}", r.map_or("", |r| r.character_names_group.as_str()), r.map_or("", |r| r.secondary_names_group.as_str()));
    }
}

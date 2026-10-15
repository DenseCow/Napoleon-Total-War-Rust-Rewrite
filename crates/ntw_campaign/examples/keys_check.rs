//! Research helper: every unit key and building key a save uses, checked against the install's
//! own tables (`units`, `building_levels`, read through the merged database view with mods'
//! overrides): keys the database does not have.
use std::collections::BTreeSet;
use ntw_data::{BuildingLevel, UnitRecord, load_table};
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;

fn main() {
    let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
    let vfs = Vfs::open_install(dir).expect("vfs");
    let mut warnings = Vec::new();
    let units = load_table::<UnitRecord>(&vfs, &mut warnings).expect("units");
    let levels = load_table::<BuildingLevel>(&vfs, &mut warnings).expect("building_levels");
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    for p in std::env::args().skip(1) {
        let esf = EsfFile::open(&p).unwrap();
        let mut ukeys = BTreeSet::new();
        let mut bkeys = BTreeSet::new();
        esf.root.walk(&mut |r: &EsfRecord| {
            match r.name.as_str() {
                "UNIT_RECORD_KEY" | "LAND_RECORD_KEY" | "NAVAL_RECORD_KEY" => {
                    if let Some(s) = r.get_str(0) {
                        ukeys.insert(s.to_string());
                    }
                }
                "RECRUITMENT_ITEM" => {
                    if let Some(EsfNode::Utf16String(s)) = r.children.get(6) {
                        ukeys.insert(s.clone());
                    }
                }
                "BUILDING" => {
                    if let Some(s) = r.get_str(1) {
                        bkeys.insert(s.to_string());
                    }
                }
                "BUILDING_CONSTRUCTION_ITEM" => {
                    if let Some(EsfNode::Utf16String(s)) = r.children.get(5) {
                        bkeys.insert(s.clone());
                    }
                }
                _ => {}
            }
        });
        let bad_u: Vec<&String> = ukeys.iter().filter(|k| units.get(k).is_none()).collect();
        let bad_b: Vec<&String> = bkeys.iter().filter(|k| levels.get(k).is_none()).collect();
        println!("{p}: {} unit keys, {} missing {:?}; {} building keys, {} missing {:?}", ukeys.len(), bad_u.len(), bad_u, bkeys.len(), bad_b.len(), bad_b);
    }
}

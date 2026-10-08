//! Research helper: every unit key and building key a save uses, checked against the install's
//! own tables (`units`, `building_levels`): keys the vanilla DB does not have.
use std::collections::BTreeSet;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;

fn utf16_has(hay: &[u8], s: &str) -> bool {
    let mut w: Vec<u8> = (s.len() as u16).to_le_bytes().to_vec();
    w.extend(s.encode_utf16().flat_map(|c| c.to_le_bytes()));
    hay.windows(w.len()).any(|x| x == &w[..])
}

fn main() {
    let dir = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";
    let vfs = Vfs::open_install(dir).expect("vfs");
    let units = vfs.read(r"db\units_tables\units").unwrap();
    let levels = vfs.read(r"db\building_levels_tables\building_levels").unwrap();
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
        let bad_u: Vec<&String> = ukeys.iter().filter(|k| !utf16_has(&units, k)).collect();
        let bad_b: Vec<&String> = bkeys.iter().filter(|k| !utf16_has(&levels, k)).collect();
        println!("{p}: {} unit keys, {} missing {:?}; {} building keys, {} missing {:?}", ukeys.len(), bad_u.len(), bad_u, bkeys.len(), bad_b.len(), bad_b);
    }
}

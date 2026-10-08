//! `sight_probe <save>...`: the sight data of saves (SAVE_COMPAT.md, shroud and sight). Per save:
//! each character's `LINE_OF_SIGHT` against the disc of his position and radius (#17), #17 against
//! his type's radius, #22 (hidden), the `EXPOSED_CHARACTERS` layout, and each shroud's trees against
//! our encoder. Read-only.
use std::path::PathBuf;

use ntw_campaign::shroud;
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::CharacterId;

fn main() {
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let db = GameDatabase::from_install(&dir).expect("db");
    for path in std::env::args().skip(1) {
        let esf = EsfFile::from_bytes(&std::fs::read(&path).expect("read")).expect("esf");
        let l = ntw_campaign::read_esf(&esf, &db).expect("load");
        let m = &l.model;
        let Some(grid) = m.world.sight_grid else {
            println!("{path}: no sight grid");
            continue;
        };
        let world = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
        let (mut los_total, mut los_match, mut los_none, mut los_off, mut hidden) = (0, 0, 0, 0, 0);
        let mut shown = 0;
        let mut exposed_layout = None;
        let mut shroud_same = (0, 0);
        for fac in world.record_array("FACTION_ARRAY").unwrap().records() {
            if let Some(sh) = fac.child("CAMPAIGN_SHROUD") {
                for q in sh.children_named("QUAD_TREE_BIT_ARRAY") {
                    let set = shroud::decode(q).unwrap();
                    let again = shroud::encode(&set, q.get_u32(2).unwrap());
                    shroud_same.1 += 1;
                    if &again == q {
                        shroud_same.0 += 1;
                    }
                }
            }
            if exposed_layout.is_none()
                && let Some(a) = fac.record_array("EXPOSED_CHARACTERS")
                && let Some(it) = a.items.first()
            {
                exposed_layout = Some(format!("{it:?}"));
            }
            let Some(chars) = fac.record_array("CHARACTER_ARRAY") else { continue };
            for item in &chars.items {
                let Some(c) = item.first().and_then(EsfNode::as_record).filter(|r| r.name == "CHARACTER") else { continue };
                let Some(id) = c.get_i32(2) else { continue };
                if c.get(22).and_then(EsfNode::as_bool) == Some(true) {
                    hidden += 1;
                }
                let Some(los) = c.child("LINE_OF_SIGHT") else { continue };
                los_total += 1;
                if los.get(0).and_then(EsfNode::as_bool) != Some(true) {
                    los_off += 1;
                    continue;
                }
                let Some(q) = los.child("QUAD_TREE_BIT_ARRAY") else {
                    los_none += 1;
                    continue;
                };
                let saved: Vec<(u32, u32)> = shroud::decode(q).unwrap().cells().collect();
                let Some(ch) = m.world.characters.get(&CharacterId(id)) else { continue };
                let r = m.sight_radius(ch.id);
                let mut disc = grid.disc((ch.position.0.to_f32(), ch.position.1.to_f32()), r);
                disc.sort_by_key(|&(x, z)| (z, x));
                if disc == saved {
                    los_match += 1;
                } else if shown < 3 {
                    shown += 1;
                    println!("  char {id} {:?} r {r}: saved {} cells, disc {} cells; first saved {:?} disc {:?}", ch.kind, saved.len(), disc.len(), saved.first(), disc.first());
                }
            }
        }
        println!(
            "{path}: LINE_OF_SIGHT {los_total} (off {los_off}, no tree {los_none}, = disc {los_match}); #22 hidden {hidden}; shroud trees re-encoded the same {}/{}; EXPOSED item {}",
            shroud_same.0,
            shroud_same.1,
            exposed_layout.unwrap_or_else(|| "none".into())
        );
        let _ = EsfRecord::new("x", 0);
    }
}

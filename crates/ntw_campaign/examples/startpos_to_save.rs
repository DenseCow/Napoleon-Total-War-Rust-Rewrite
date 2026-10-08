//! Research helper: writes a start position as our writer's turn-1 save (no turn played), to
//! compare with the original's own turn-1 save of the same campaign.
//!   startpos_to_save STARTPOS HUMAN OUT
use ntw_formats::esf::EsfFile;
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::path::PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data");
    let db = ntw_data::GameDatabase::from_install(&dir).expect("db");
    let esf = EsfFile::open(&a[0]).unwrap();
    let mut l = ntw_campaign::read_esf(&esf, &db).unwrap();
    assert!(l.set_human(&a[1]));
    let out = ntw_campaign::save::write_save(&esf, &l.model, &a[1], 0).unwrap();
    std::fs::write(&a[2], out.to_bytes().unwrap()).unwrap();
}

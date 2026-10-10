//! `header_map_check <save>...`: rebuilds each save's header territory pictures from its own region
//! owners (`ntw_campaign::header_map`) and compares them with the stored ones (SAVE_COMPAT.md §25).
//! Read-only.
use std::collections::HashSet;
use std::path::PathBuf;

use ntw_campaign::header_map::TheatrePictures;
use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;

fn main() {
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let vfs = Vfs::open_install(&dir).expect("install");
    let db = GameDatabase::from_install(&dir).expect("db");
    let files = GameFiles { vfs: &vfs };
    for path in std::env::args().skip(1) {
        let esf = EsfFile::from_bytes(&std::fs::read(&path).expect("read")).expect("esf");
        let l = ntw_campaign::read_esf(&esf, &db).expect("load");
        let human = l.info.header.faction_key.clone();
        let hf = l.model.faction_by_key(&human).map(|f| f.id);
        let owned: HashSet<String> = l.model.world.regions.values().filter(|r| Some(r.owner) == hf).map(|r| r.key.clone()).collect();
        for m in &l.info.header.maps {
            let Some(p) = TheatrePictures::load(&files, &db, &l.info.map_key, &m.theatre) else {
                println!("{path}: {}: no pictures", m.theatre);
                continue;
            };
            let ours = p.render(&owned, Some(&m.pixels));
            let diff = ours.iter().zip(&m.pixels).filter(|(a, b)| a != b).count();
            println!("{path}: {} {}x{}: {} of {} pixels differ ({} regions owned by {human})", m.theatre, m.width, m.height, diff, m.pixels.len(), owned.len());
        }
    }
}

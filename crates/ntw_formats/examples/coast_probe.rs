//! Research helper (read-only): value ranges of a campaign map's coastline meshes.
//!   cargo run -p ntw_formats --example coast_probe -- [map, default nap_europe]
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let map = std::env::args().nth(1).unwrap_or_else(|| "nap_europe".into());
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs, data_dir: Some(std::path::Path::new(&dir)) };
    let m = CampaignMap::load(&files, &map).unwrap();
    for (g, c) in m.coast.iter().enumerate() {
        let mut mn = [f32::MAX; 7];
        let mut mx = [f32::MIN; 7];
        for v in &c.vertices {
            let a = [v.position[0], v.position[1], v.position[2], v.tex[0], v.tex[1], v.tex2[0], v.tex2[1]];
            for k in 0..7 {
                mn[k] = mn[k].min(a[k]);
                mx[k] = mx[k].max(a[k]);
            }
        }
        println!("group {g}: {} vertices; x y z u v u2 v2 min {mn:?} max {mx:?}", c.vertices.len());
        for v in c.vertices.iter().take(6) {
            println!("  {v:?}");
        }
    }
}

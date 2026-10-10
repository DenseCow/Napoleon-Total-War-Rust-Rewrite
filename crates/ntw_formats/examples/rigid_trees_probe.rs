//! Research helper (read-only): reads a campaign map's `display\trees\campaign.rigid_trees`,
//! prints per-model instance counts and scale ranges, and compares each tree's stored height with
//! the heightmap value under it (to calibrate the terrain's vertical scale).
//!   cargo run -p ntw_formats --example rigid_trees_probe -- [map, default nap_europe]
use ntw_formats::campaign_map::{CampaignMap, GameFiles, RigidTrees};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let map = std::env::args().nth(1).unwrap_or_else(|| "nap_europe".into());
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    let m = CampaignMap::load(&files, &map).unwrap();
    let t = RigidTrees::read(&files.read(&format!("campaign_maps/{map}/display/trees/campaign.rigid_trees")).unwrap()).unwrap();
    let (mut n, mut sxy, mut sxx, mut sy) = (0f64, 0f64, 0f64, 0f64);
    let (mut smin, mut smax) = (f32::MAX, f32::MIN);
    for model in &t.models {
        let count: usize = model.groups.iter().map(|g| g.instances.len()).sum();
        if count > 0 {
            println!("{:5} {}", count, model.path);
        }
        for i in model.groups.iter().flat_map(|g| &g.instances) {
            let [x, y, z] = i.position;
            let (mn, mx) = (m.regions.bounds_min, m.regions.bounds_max);
            let u = (x - mn.0) / (mx.0 - mn.0) * m.heightmap.width as f32;
            let v = (mx.1 - z) / (mx.1 - mn.1) * m.heightmap.height as f32;
            let raw = m.heightmap.sample(u, v) as f64;
            n += 1.0;
            sxy += raw * y as f64;
            sxx += raw * raw;
            sy += y as f64;
            smin = smin.min(i.scale);
            smax = smax.max(i.scale);
        }
    }
    println!("models {} instances {n} scale {smin}..{smax}", t.models.len());
    println!("height per heightmap step (least squares through 0): {}", sxy / sxx);
    println!("mean y {}", sy / n);
}

//! Research helper (read-only): compares each town/port slot's angle with the direction from its
//! position to its dock point in `regions.esf`.
//!   cargo run -p ntw_formats --example slot_facing_probe -- [map, default nap_europe]
use ntw_formats::campaign_map::{GameFiles, RegionMap};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let map = std::env::args().nth(1).unwrap_or_else(|| "nap_europe".into());
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    let rm = RegionMap::read(&files.read(&format!("campaign_maps/{map}/regions.esf")).unwrap()).unwrap();
    let (mut n, mut worst, mut far) = (0, 0f32, 0);
    for r in &rm.regions {
        let Some(s) = &r.settlement else { continue };
        for slot in s.slots.iter().filter(|s| !s.model.is_empty()) {
            let (dx, dz) = (slot.dock.0 - slot.position.0, slot.dock.1 - slot.position.2);
            if dx * dx + dz * dz < 1e-6 {
                continue;
            }
            let theta = slot.angle as f32 / 65536.0 * std::f32::consts::TAU;
            // Expected dock direction (logic x, z) = (-sin θ, -cos θ).
            let (ex, ez) = (-theta.sin(), -theta.cos());
            let len = (dx * dx + dz * dz).sqrt();
            let err = ((dx / len - ex).powi(2) + (dz / len - ez).powi(2)).sqrt();
            worst = worst.max(err);
            n += 1;
            if err > 0.1 {
                far += 1;
                println!("{} angle {} dock dir ({:.2}, {:.2}) expected ({ex:.2}, {ez:.2})", slot.key, slot.angle, dx / len, dz / len);
            }
        }
    }
    println!("{n} slots with a model and a dock; {far} off by more than 0.1; worst {worst:.3}");
    // Settlement slots: the footprints' edge directions (mod 90 degrees) against the slot angle.
    for r in rm.regions.iter().take(80) {
        let Some(s) = &r.settlement else { continue };
        for slot in s.slots.iter().filter(|s| s.slot_type.starts_with("settlement_") && s.slot_type.ends_with("_slot")).take(1) {
            let mut dirs = Vec::new();
            for fp in &slot.footprints {
                for w in fp.windows(2) {
                    let a = (w[1].1 - w[0].1).atan2(w[1].0 - w[0].0).to_degrees().rem_euclid(90.0);
                    dirs.push(a.round() as i32);
                }
            }
            dirs.sort();
            let theta = (slot.angle as f32 / 65536.0 * 360.0).rem_euclid(90.0);
            println!("{} angle {:.1} mod 90 = {theta:.1}; footprint edges mod 90: {:?}", slot.key, slot.angle as f32 / 65536.0 * 360.0, &dirs[..dirs.len().min(12)]);
        }
    }
}


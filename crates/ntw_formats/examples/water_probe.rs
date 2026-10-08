//! Research probe: where is the water on a battle map, and at what height?
//!
//! Reads every preset under `battleterrain\presets\` (read-only) and prints, per map: the level-0
//! heightfield's range, how much of the ground-type map is water (`water_deep` 17, `water_frozen`
//! 18, `water_medium_ford` 19, `water_shallow` 20) and the height of those cells. That is what the
//! sea surface height has to be.
//!
//! ```text
//! cargo run -p ntw_formats --example water_probe -- [name substring]
//! ```
use ntw_formats::battle_terrain::{self, BattleMap};
use ntw_formats::pack::Vfs;

/// The 25 ground-type names in the exe's table order (`ntw_sim::battle::ground::GROUND_TYPE_NAMES`,
/// copied here so the probe needs no dependency on the sim).
const NAMES: [&str; 25] = [
    "field_ploughed", "field_ploughed_wet", "field_forest", "grassland", "mud", "mud_wet", "road", "road_frozen", "rock", "sand",
    "sand_wet", "scree", "snow", "stone_masonry", "vegetation_dense_forest", "vegetation_light_scrub",
    "vegetation_medium_woodland", "water_deep", "water_frozen", "water_medium_ford", "water_shallow", "wood", "stone", "glass", "none",
];

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let filter = args.first().cloned().unwrap_or_default();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    println!("{:<26} {:>7} {:>7} {:>9} {:>9} {:>9} {:>9} {:>8} {:>8} {:>8}", "map", "scale", "bias", "h_min", "h_max", "typed%", "below0%", "w_min", "w_max", "w_mean");
    for name in battle_terrain::list_presets(&vfs) {
        if !filter.is_empty() && !name.contains(&filter) {
            continue;
        }
        let Ok(map) = BattleMap::load(&vfs, &name) else { continue };
        let Some(hf) = map.ground() else { continue };
        let (h_min, h_max) = hf.min_max_m();
        let (w, h) = (hf.settings.world_width, hf.settings.world_height);
        let (mut n_water, mut w_min, mut w_max, mut w_sum) = (0usize, f32::MAX, f32::MIN, 0.0f64);
        // Per-cell water coverage, sampled on the heightfield grid.
        let mut cells = 0usize;
        let mut below = 0usize;
        let mut hist = [0usize; 256];
        if let Some(gt) = &map.ground_types {
            for r in 0..gt.height {
                for c in 0..gt.width {
                    cells += 1;
                    let t = gt.cells[(r * gt.width + c) as usize];
                    hist[t as usize] += 1;
                    let x = (c as f32 + 0.5) / gt.width as f32 * w - w / 2.0;
                    let y = h / 2.0 - (r as f32 + 0.5) / gt.height as f32 * h;
                    let y_h = hf.height_at(x, y);
                    if y_h < 0.0 {
                        below += 1;
                    }
                    if !(17..=20).contains(&t) {
                        continue;
                    }
                    n_water += 1;
                    w_min = w_min.min(y_h);
                    w_max = w_max.max(y_h);
                    w_sum += y_h as f64;
                }
            }
        }
        let pct = if cells > 0 { 100.0 * n_water as f64 / cells as f64 } else { 0.0 };
        let pct_below = if cells > 0 { 100.0 * below as f64 / cells as f64 } else { 0.0 };
        println!(
            "{:<26} {:>7.1} {:>7.1} {:>9.2} {:>9.2} {:>8.2}% {:>8.2}% {:>8.2} {:>8.2} {:>8.2}",
            name,
            hf.settings.scale,
            hf.settings.bias,
            h_min,
            h_max,
            pct,
            pct_below,
            if n_water > 0 { w_min } else { 0.0 },
            if n_water > 0 { w_max } else { 0.0 },
            if n_water > 0 { (w_sum / n_water as f64) as f32 } else { 0.0 },
        );
        // Ground-type histogram with the editor palette colour of each index, so the water types
        // can be told apart from the palette alone (the index -> name order is INFERRED).
        let used: Vec<usize> = (0..256).filter(|&t| hist[t] > 0).collect();
        let parts: Vec<String> = used
            .iter()
            .map(|&t| {
                let p = map.ground_types.as_ref().and_then(|g| g.palette.get(t)).copied().unwrap_or([0; 4]);
                let nm = NAMES.get(t).copied().unwrap_or("?");
                format!("{t:>3} {nm:<24} #{:02x}{:02x}{:02x} {:>6.2}%", p[0], p[1], p[2], 100.0 * hist[t] as f64 / cells.max(1) as f64)
            })
            .collect();
        println!("      {}", parts.join("\n      "));
    }
}

//! Research probe: which way round do the heightfield and ground-type map lie relative to the
//! map `(x, y)` positions of trees and buildings? Read-only; prints statistics.
//!
//! For each of the 8 grid orientations (optional transpose, flip of columns, flip of rows) it
//! prints:
//! - KL divergence of the ground types under trees from the ground types of the whole map:
//!   trees sit in
//!   forest cells, so the right orientation concentrates them.
//! - `slope`: mean terrain slope under buildings (buildings stand on flat ground, so the right
//!   orientation gives the lowest value), against the mean slope of the whole map.
//!
//! `cargo run --release -p ntw_formats --example terrain_orient -- hb_austerlitz hb_waterloo`
use ntw_formats::battle_terrain::{BattleMap, Heightfield};
use ntw_formats::pack::Vfs;

const DEFAULT_DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

/// Maps normalized (u, v) in 0..1 (u grows with x, v with y) to a grid cell under orientation `o`.
fn cell(u: f32, v: f32, w: u32, h: u32, o: u8) -> (u32, u32) {
    let (mut a, mut b) = if o & 4 != 0 { (v, u) } else { (u, v) };
    if o & 1 != 0 {
        a = 1.0 - a;
    }
    if o & 2 != 0 {
        b = 1.0 - b;
    }
    let c = ((a * (w - 1) as f32).round() as i64).clamp(0, w as i64 - 1) as u32;
    let r = ((b * (h - 1) as f32).round() as i64).clamp(0, h as i64 - 1) as u32;
    (c, r)
}

fn slope(hf: &Heightfield, c: u32, r: u32) -> f32 {
    let (dx, dy) = hf.spacing();
    let (c, r) = (c as i64, r as i64);
    let gx = (hf.sample_m(c + 1, r) - hf.sample_m(c - 1, r)) / (2.0 * dx);
    let gy = (hf.sample_m(c, r + 1) - hf.sample_m(c, r - 1)) / (2.0 * dy);
    (gx * gx + gy * gy).sqrt()
}

fn main() {
    let vfs = Vfs::open_install(DEFAULT_DATA).expect("install");
    for name in std::env::args().skip(1) {
        let map = BattleMap::load(&vfs, &name).expect("map");
        let hf = map.ground().expect("heightfield");
        let world = hf.settings.world_width;
        let norm = |(x, y): (f32, f32)| (x / world + 0.5, y / world + 0.5);
        let trees: Vec<(f32, f32)> = map
            .trees
            .iter()
            .flat_map(|l| &l.groups)
            .flat_map(|g| &g.instances)
            .map(|t| t.position)
            .filter(|p| p.0.abs() < world / 2.0 && p.1.abs() < world / 2.0)
            .collect();
        let mut avg = 0.0;
        for r in (1..hf.height - 1).step_by(8) {
            for c in (1..hf.width - 1).step_by(8) {
                avg += slope(hf, c, r);
            }
        }
        avg /= (((hf.height - 2) / 8 + 1) * ((hf.width - 2) / 8 + 1)) as f32;
        println!("== {name}: {} trees inside, {} buildings, map mean slope {avg:.4}", trees.len(), map.buildings_near.len());
        for o in 0..8u8 {
            let mut gt_share = 0.0;
            if let Some(gt) = &map.ground_types {
                let mut hist = [0usize; 256];
                for &p in &trees {
                    let (u, v) = norm(p);
                    let (c, r) = cell(u, v, gt.width, gt.height, o);
                    hist[gt.cells[(r * gt.width + c) as usize] as usize] += 1;
                }
                let mut area = [0usize; 256];
                for &v in &gt.cells {
                    area[v as usize] += 1;
                }
                // KL divergence of the tree histogram from the area histogram (bits).
                let n = trees.len().max(1) as f32;
                let a = gt.cells.len() as f32;
                gt_share = (0..256)
                    .filter(|&i| hist[i] > 0 && area[i] > 0)
                    .map(|i| { let p = hist[i] as f32 / n; p * (p / (area[i] as f32 / a)).log2() })
                    .sum();
            }
            let mut s = 0.0;
            for b in &map.buildings_near {
                let (u, v) = norm(b.position);
                let (c, r) = cell(u, v, hf.width, hf.height, o);
                s += slope(hf, c, r);
            }
            s /= map.buildings_near.len().max(1) as f32;
            println!(
                "  orient {o} (transpose {}, flip cols {}, flip rows {}): tree-vs-area KL {:.3} bits, building slope {s:.4}",
                o & 4 != 0,
                o & 1 != 0,
                o & 2 != 0,
                gt_share
            );
        }
    }
}

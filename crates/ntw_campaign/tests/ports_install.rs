//! Ports, embarking and landing (`ntw_sim::campaign::embark`) on the real maps (read-only).
//! Each test skips (passes, printing a note) without an install.
//! Notes: `analysis/campaign/PATHFINDING_PORTS.md`.

use std::path::PathBuf;

use ntw_campaign::pathing::poly_map;
use ntw_formats::campaign_pathfinding::PathfindingFile;
use ntw_sim::campaign::polypath::{kind, PolyMap};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

const MAPS: [&str; 5] = ["nap_europe", "nap_italy", "nap_egypt", "nap_spain", "nap_tut"];

fn build(name: &str) -> Option<PolyMap> {
    let d = data_dir().join("campaign_maps").join(name);
    let pf = PathfindingFile::read(&std::fs::read(d.join("pathfinding.esf")).ok()?).expect("reads");
    let a = &pf.areas[0];
    let cells = a.grid.expand();
    let sets = a.grid.region_sets().expect("region table");
    Some(poly_map(a, &cells, &sets))
}

/// Sea polygons never share an edge with open land: a strip of kind 2 polygons lies between them,
/// so armies and fleets only meet through the port and landing nodes (PATHFINDING_PORTS.md §1).
#[test]
fn the_coast_is_a_kind_2_strip() {
    for name in MAPS {
        let Some(pm) = build(name) else {
            eprintln!("skipped: no install");
            return;
        };
        let (mut sea_land, mut sea_coast, mut coast_land) = (0, 0, 0);
        for p in 0..pm.len() {
            for &q in pm.neighbours(p) {
                match (pm.kind[p], pm.kind[q as usize]) {
                    (kind::SEA, kind::LAND) => sea_land += 1,
                    (kind::SEA, 2) => sea_coast += 1,
                    (2, kind::LAND) => coast_land += 1,
                    _ => {}
                }
            }
        }
        println!("{name}: sea-land {sea_land}, sea-coast {sea_coast}, coast-land {coast_land}");
        assert_eq!(sea_land, 0, "{name}");
        assert!(sea_coast > 0 && coast_land > 0, "{name}");
    }
}

//! `pathfinding.esf` and `sea_grids.esf` against the real install (read-only). Run with
//! `cargo test -p ntw_formats --test pathfinding_install -- --ignored --nocapture`.

use std::collections::HashSet;
use std::path::PathBuf;

use ntw_formats::campaign_map::RegionMap;
use ntw_formats::campaign_pathfinding::{PathfindingFile, SeaGrid, SENTINEL};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

const MAPS: [&str; 5] = ["nap_europe", "nap_italy", "nap_egypt", "nap_spain", "nap_tut"];

/// Every map's pathfinding file reads fully and is self-consistent (Spain only after decryption).
#[test]
#[ignore]
fn pathfinding_files_read_and_are_consistent() {
    for map in MAPS {
        let dir = data_dir().join("campaign_maps").join(map);
        let pf = PathfindingFile::read(&std::fs::read(dir.join("pathfinding.esf")).unwrap()).unwrap();
        let regions = RegionMap::read(&std::fs::read(dir.join("regions.esf")).unwrap()).unwrap();
        assert_eq!(pf.decrypted, map == "nap_spain", "{map}");
        assert_eq!(pf.areas.len(), 1, "{map}");
        let a = &pf.areas[0];
        let g = &a.grid;
        // Sentinels first, then real vertices inside the grid.
        assert!(a.vertices[..4].iter().all(|v| *v == (SENTINEL, SENTINEL)), "{map}");
        let (x0, z0) = g.origin_units();
        let (x1, z1) = (x0 + g.cols as f32 * g.cell_units(), z0 + g.rows as f32 * g.cell_units());
        for i in 4..a.vertices.len() {
            let (x, z) = a.vertex_units(i).unwrap();
            assert!(x >= x0 - 0.01 && x <= x1 + 0.01 && z >= z0 - 0.01 && z <= z1 + 0.01, "{map} vertex {i} ({x},{z})");
        }
        // The list array splits exactly; every index is a vertex.
        let lists = a.outline_lists().unwrap_or_else(|| panic!("{map}: lists do not split"));
        for (_, l) in &lists {
            assert!(l.iter().all(|&v| (v as usize) < a.vertices.len()), "{map}");
        }
        let starts: HashSet<usize> = lists.iter().map(|(o, _)| *o).collect();
        // The cells expand to the whole grid and every boundary points at a list start.
        let cells = g.expand();
        assert_eq!(cells.len(), (g.cols * g.rows) as usize, "{map}");
        assert_eq!(g.cell_units(), 2.0, "{map}");
        let mut nb = 0;
        let mut maxr = 0u16;
        for c in g.records.iter().filter(|c| c.run.is_none()) {
            for b in &c.boundaries {
                nb += 1;
                assert!(starts.contains(&b.list_offset()), "{map}: boundary offset {}", b.list_offset());
                maxr = maxr.max(if b.region() == 1023 { 0 } else { b.region() });
            }
        }
        // One boundary per list except the first list (INFERRED: the outer one).
        assert_eq!(nb + 1, lists.len(), "{map}");
        // Region map: valid regions.esf indices.
        assert_eq!(g.region_map.len(), g.region_count as usize, "{map}");
        assert!(g.region_map.iter().all(|&r| r >= 0 && (r as usize) < regions.regions.len()), "{map}");
        println!(
            "{map}: {} vertices, {} lists, grid {}x{} ({} records), {} regions (max boundary region {maxr}), {} barriers",
            a.vertices.len(), lists.len(), g.cols, g.rows, g.records.len(), g.region_count, pf.barriers.len()
        );
    }
}

/// Every map's sea grid reads to the last value.
#[test]
#[ignore]
fn sea_grids_read() {
    for map in MAPS {
        let path = data_dir().join("campaign_maps").join(map).join("sea_grids.esf");
        let sg = SeaGrid::read(&std::fs::read(path).unwrap()).unwrap_or_else(|e| panic!("{map}: {e}"));
        assert_eq!(sg.cells.len(), (sg.cols * sg.rows) as usize);
        assert_eq!(sg.zones.len(), sg.cells.len());
        for (i, z) in sg.zones.iter().enumerate() {
            if i < 14 && map == "nap_europe" { println!("zone {i}: index {} land {} seas {} ports {} ids {}", z.index, z.land.len(), z.seas.len(), z.ports.len(), z.ids.len()); }
        }
        println!("{map}: sea grid {}x{} of {} units, {} links", sg.cols, sg.rows, sg.cell_size, sg.links.len());
    }
}

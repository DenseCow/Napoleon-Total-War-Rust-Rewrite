//! Campaign map readers against the real install (read-only). Run with
//! `cargo test -p ntw_formats --test campaign_map_install -- --ignored --nocapture`.

use std::path::PathBuf;

use ntw_formats::campaign_map::{BorderRibbon, CampaignMap, DISPLAY_TO_LOGIC, GameFiles};
use ntw_formats::pack::Vfs;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// All five maps load: regions, heightmap, supertexture index and every spline file.
#[test]
#[ignore]
fn all_maps_load() {
    let dir = data_dir();
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    for name in ["nap_europe", "nap_italy", "nap_egypt", "nap_spain", "nap_tut"] {
        let m = CampaignMap::load(&files, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let settlements = m.regions.regions.iter().filter(|r| r.settlement.is_some()).count();
        println!(
            "{name}: {} regions ({settlements} settlements), heightmap {}x{}, {} splines, supertexture {:?}",
            m.regions.regions.len(),
            m.heightmap.width,
            m.heightmap.height,
            m.splines.len(),
            m.supertexture.as_ref().map(|s| (s.width, s.height, s.levels.len()))
        );
        assert!(m.supertexture.is_some(), "{name}");
        assert!(!m.splines.is_empty(), "{name}");
        // Every map has a tree list that parses to the byte.
        let trees = m.trees.as_ref().unwrap_or_else(|| panic!("{name}: trees"));
        let count: usize = trees.models.iter().flat_map(|t| &t.groups).map(|g| g.instances.len()).sum();
        println!("  {} tree models, {count} trees", trees.models.len());
        // Every map has at least one coastline group, and every file parses to the byte.
        let coast_groups = files.list(&format!("campaign_maps/{name}/display/coastline/")).iter().filter(|p| p.ends_with(".rigid_mesh")).count();
        assert_eq!(m.coast.len(), coast_groups, "{name}: every coastline group parses");
        println!("  {} coastline groups, {} triangles", m.coast.len(), m.coast.iter().map(|c| c.indices.len() / 3).sum::<usize>());
        if name == "nap_europe" {
            assert_eq!(count, 8903);
            assert_eq!((m.regions.regions.len(), settlements), (101, 72));
            assert_eq!((m.heightmap.width, m.heightmap.height), (4096, 2048));
            assert_eq!(m.regions.region_at(-212.2, 2.3).map(|r| r.key.as_str()), Some("eur_france"));
            // The coarsest supertexture level decodes (all tiles inflate to their stated size).
            let last = m.supertexture.as_ref().unwrap().levels.len() - 1;
            let (w, h, px) = m.supertexture_rgba(&files, last).unwrap();
            assert_eq!(px.len(), (w * h * 4) as usize);
        }
    }
}

/// Border splines scaled by DISPLAY_TO_LOGIC land on the regions.esf outline vertices.
#[test]
#[ignore]
fn display_scale_matches_region_outlines() {
    let dir = data_dir();
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    let m = CampaignMap::load(&files, "nap_europe").unwrap();
    let mut grid: std::collections::HashMap<(i32, i32), Vec<(f32, f32)>> = Default::default();
    for &v in &m.regions.vertices {
        grid.entry(((v.0 / 4.0).floor() as i32, (v.1 / 4.0).floor() as i32)).or_default().push(v);
    }
    let (mut sum, mut n) = (0.0f32, 0);
    for (_, s) in m.splines.iter().filter(|(f, _)| f == "borders") {
        for p in s.points.iter().step_by(5) {
            let (x, z) = (p[0] * DISPLAY_TO_LOGIC, p[2] * DISPLAY_TO_LOGIC);
            let (cx, cz) = ((x / 4.0).floor() as i32, (z / 4.0).floor() as i32);
            let mut best = 6.0f32;
            for dx in -1..=1 {
                for dz in -1..=1 {
                    for v in grid.get(&(cx + dx, cz + dz)).into_iter().flatten() {
                        best = best.min(((v.0 - x).powi(2) + (v.1 - z).powi(2)).sqrt());
                    }
                }
            }
            sum += best;
            n += 1;
        }
    }
    let mean = sum / n as f32;
    println!("{n} border points, mean distance to an outline vertex {mean:.3}");
    assert!(mean < 0.5, "mean distance {mean}");
}

/// The pre-generated border ribbon in the pack. This **corrects** CAMPAIGN_MAP.md 10.3a-1, which
/// said the file "has UVs": it has none. The layout is confirmed to the byte and every geometric
/// fact below is read off all 586 records.
#[test]
#[ignore]
fn the_shipped_border_ribbon_is_geometry_only_and_parses_to_the_byte() {
    let dir = data_dir();
    let vfs = Vfs::open_install(&dir).unwrap();
    let files = GameFiles { vfs: &vfs };
    let raw = files.read("testdata\\westerneuborders.rigid_mesh").expect("the ribbon ships");
    assert_eq!(raw.len(), 39_858, "the file's exact length");

    let r = BorderRibbon::read(&raw).expect("parses");
    // 6 + 586*56 + 4 + 1758*4 == 39858, so the layout closes on the byte.
    assert_eq!(r.flags, 0);
    assert_eq!(r.vertices.len(), 586);
    assert_eq!(r.indices.len(), 1758);
    assert_eq!(r.indices.len(), 3 * r.vertices.len(), "three indices per vertex: an unrolled ribbon list");
    assert_eq!(*r.indices.iter().max().unwrap(), 585, "the highest index is the last vertex");

    // CONFIRMED: no UVs at all, y is 0 everywhere, and the vertices are not map display units.
    assert!(r.is_geometry_only(), "every vertex: y == 0 and both texture coordinates 0.0");
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for v in &r.vertices {
        lo = lo.min(v.position[0]);
        hi = hi.max(v.position[0]);
    }
    println!(
        "ribbon: {} vertices, x [{lo:.4}, {hi:.4}] (span {:.4}), y 0, uv 0",
        r.vertices.len(),
        hi - lo
    );
    assert!(hi - lo < 10.0, "these are not campaign map display units, which run to about 1000");
    // The list is a fan from vertex 0, not a strip of (2k, 2k+1) pairs -- so no single constant
    // ribbon width can be read off this file, which is what stops it being a drop-in border mesh.
    assert_eq!(&r.indices[..6], &[0, 1, 2, 0, 3, 1], "the list is a fan from vertex 0");
}

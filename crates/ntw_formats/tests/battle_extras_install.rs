//! Battle-map extras against the real install (read-only): `.markers` and the farm files. Run with
//! `cargo test -p ntw_formats --test battle_extras_install -- --ignored --nocapture`.

use std::path::PathBuf;

use ntw_formats::battle_markers::{MarkerItems, MarkerRepository};
use ntw_formats::pack::Vfs;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// Every `.markers` file reads to its last byte.
#[test]
#[ignore]
fn all_markers_read() {
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let paths: Vec<String> = vfs
        .list("")
        .into_iter()
        .filter(|p| p.ends_with(".markers") && !p.contains("campaignbridges"))
        .map(str::to_owned)
        .collect();
    assert_eq!(paths.len(), 55);
    let bridges = ntw_formats::battle_markers::NamedTransforms::read(&vfs.read("rigidmodels/campaignbridges/bridge.markers").unwrap()).unwrap();
    assert!(bridges.0.iter().all(|(n, m)| n.starts_with("bridge:") && m[15] == 1.0));
    println!("bridge.markers: {} bridges", bridges.0.len());
    let (mut props, mut polys, mut closed) = (0, 0, 0);
    for p in &paths {
        let m = MarkerRepository::read(&vfs.read(p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert_eq!(m.name, "BASE_MARKER_REPOSITORY");
        for g in &m.groups {
            match &g.items {
                MarkerItems::Props(v) => props += v.len(),
                MarkerItems::Polygons(v) => {
                    polys += v.len();
                    closed += v.iter().filter(|poly| poly.first() == poly.last()).count();
                }
                MarkerItems::Empty => {}
            }
            // Every model group's indices point at items.
            for mg in &g.models {
                assert!(mg.items.iter().all(|&i| (i as usize) < g.items.len()), "{p}");
            }
        }
    }
    println!("{} marker files: {props} props, {polys} polygons ({closed} closed)", paths.len());
    assert!(props > 0 && polys > 0);
}

/// Every `.farm_fields_tile_texture` reads; kind-1 chunks are two JPEG files, kind-0
/// chunks are uncompressed TGA files.
#[test]
#[ignore]
fn all_farm_textures_read() {
    use ntw_formats::battle_markers::FarmTileTexture;
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let paths: Vec<String> = vfs.list("battleterrain").into_iter().filter(|p| p.ends_with(".farm_fields_tile_texture")).map(str::to_owned).collect();
    assert_eq!(paths.len(), 131);
    let mut kinds = std::collections::BTreeMap::new();
    let mut heads = std::collections::BTreeMap::new();
    for p in &paths {
        let f = FarmTileTexture::read(&vfs.read(p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
        *kinds.entry(f.kind).or_insert(0) += 1;
        for c in &f.chunks {
            if f.kind == 1 {
                let (a, b) = FarmTileTexture::jpegs(c).unwrap_or_else(|| panic!("{p}: chunk sizes"));
                assert!(a.starts_with(&[0xFF, 0xD8]) && b.starts_with(&[0xFF, 0xD8]), "{p}: not JPEG");
            } else {
                let img = FarmTileTexture::tga(c).unwrap_or_else(|| panic!("{p}: kind-0 chunk is not a TGA"));
                assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
                // The header's size accounts for the chunk, apart from trailing zero padding.
                let used = 18 + (img.width * img.height * 4) as usize;
                assert!(used <= c.len() && c[used..].iter().all(|&b| b == 0), "{p}: TGA size");
                *heads.entry("tga chunks").or_insert(0) += 1;
            }
        }
    }
    println!("farm textures by kind {kinds:?}; kind-0 TGA sizes {heads:?}");
}

/// The farm ESF files (`.farm_manager`, `.farm_template_tile`) and `.prop_list` parse as ESF and
/// round-trip byte for byte.
#[test]
#[ignore]
fn farm_esf_files_read() {
    use ntw_formats::esf::EsfFile;
    let vfs = Vfs::open_install(data_dir()).unwrap();
    for ext in [".farm_manager", ".farm_template_tile", ".prop_list"] {
        let paths: Vec<String> = vfs.list("").into_iter().filter(|p| p.ends_with(ext)).map(str::to_owned).collect();
        let mut roots = std::collections::BTreeMap::new();
        for p in &paths {
            let b = vfs.read(p).unwrap();
            let f = EsfFile::from_bytes(&b).unwrap_or_else(|e| panic!("{p}: {e}"));
            assert_eq!(f.to_bytes().unwrap(), b, "{p}");
            *roots.entry(format!("{} v{}", f.root.name, f.root.version)).or_insert(0) += 1;
        }
        if ext == ".prop_list" {
            let n: usize = paths.iter().map(|p| ntw_formats::battle_markers::read_prop_list(&vfs.read(p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}")).len()).sum();
            println!("  {n} props");
        }
        if ext == ".farm_template_tile" {
            for p in &paths {
                let t = ntw_formats::battle_markers::FarmTileTemplate::read(&vfs.read(p).unwrap()).unwrap();
                assert!(!t.farms.is_empty() || !t.trees.is_empty(), "{p}");
            }
        }
        println!("{ext}: {} files, roots {roots:?}", paths.len());
        assert!(!paths.is_empty());
    }
}

/// The typed farm readers (`FarmManager`, `FarmTileTemplate::collisions`) on every shipped file, with
/// the field relations documented in `analysis/campaign/S1_LEFTOVERS.md` §2.
#[test]
#[ignore]
fn farm_records_typed() {
    use ntw_formats::battle_markers::{FarmManager, FarmTileTemplate};
    let vfs = Vfs::open_install(data_dir()).unwrap();
    // Templates: the collision radii are the nearest-edge and farthest-point distances.
    let mut templates = std::collections::BTreeMap::new();
    let mut n_coll = 0;
    for p in vfs.list("").into_iter().filter(|p| p.ends_with(".farm_template_tile")) {
        let t = FarmTileTemplate::read(&vfs.read(p).unwrap()).unwrap();
        for c in &t.collisions {
            n_coll += 1;
            let far = c.outline.iter().map(|&(x, y)| (x - c.centre.0).hypot(y - c.centre.1)).fold(0f32, f32::max);
            assert!((far - c.outer_radius).abs() < 0.05 + far * 1e-4, "{p}: outer radius {} vs {far}", c.outer_radius);
            assert!(c.inner_radius <= c.outer_radius + 1e-3, "{p}");
            assert!(c.box_min.0 <= c.box_max.0 && c.box_min.1 <= c.box_max.1, "{p}");
        }
        templates.insert(p.to_owned(), (t.farms.len(), t.walls.len()));
    }
    assert_eq!(n_coll, 387);
    // Managers.
    let mut n_farms = 0;
    for p in vfs.list("").into_iter().filter(|p| p.ends_with(".farm_manager")) {
        let m = FarmManager::read(&vfs.read(p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
        let farms = m.farms[0].len() + m.farms[1].len();
        n_farms += farms;
        assert_eq!(m.names.len(), farms + m.walls.len() + m.roads.len(), "{p}: one name per instance");
        for t in &m.templates {
            assert_eq!((t.textures.len(), t.fences.len(), t.hedges.len()), (9, 5, 5), "{p}");
            let key = t.tile_template.replace('/', "\\").to_ascii_lowercase();
            let (tf, tw) = templates[&key];
            let owners = m.farms.iter().flatten().map(|f| f.placement.owner);
            assert!(owners.into_iter().all(|o| (o as usize) < tf), "{p}: farm owner index");
            assert!(m.walls.iter().all(|w| (w.0.owner as usize) < tw), "{p}: wall owner index");
        }
        for s in &m.tile_sets {
            assert!((s.template as usize) < m.templates.len(), "{p}");
            assert_eq!(s.transform[2], (0.0, 0.0, 1.0), "{p}");
            for c in &s.cells {
                for &(i, list) in &c.farms {
                    assert!((0..2).contains(&list) && (i as usize) < m.farms[list as usize].len(), "{p}: cell farm ({i}, {list})");
                }
                assert!(c.walls.iter().all(|&w| (w as usize) < m.walls.len()), "{p}: cell walls");
            }
        }
        for f in m.farms.iter().flatten() {
            assert_eq!(f.placement.transform[2], (0.0, 0.0, 1.0), "{p}");
        }
    }
    println!("{n_coll} farm collisions, {n_farms} farm instances");
}

/// `models_building`: the whole table reads with the exe's row layout, and the buildings Lodi's
/// script garrisons (town hall, farmhouse) have fire lines while a plain house has none.
#[test]
#[ignore]
fn models_building_fire_lines() {
    let vfs = Vfs::open_install(data_dir()).unwrap();
    let path = vfs.list("db/models_building_tables").into_iter().next().unwrap().to_owned();
    let rows = ntw_formats::models_building::read(&vfs.read(&path).unwrap()).unwrap();
    assert_eq!(rows.len(), 538);
    let lines = |k: &str| rows.iter().find(|r| r.key == k).map(|r| r.intact_fire_lines().count()).unwrap();
    assert!(lines("south_euro_townhall") > 0 && lines("south_euro_farmhouse") > 0);
    assert_eq!(lines("south_euro_house02"), 0);
    println!("town hall {}, farmhouse {}", lines("south_euro_townhall"), lines("south_euro_farmhouse"));
}

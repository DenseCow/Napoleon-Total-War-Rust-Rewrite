//! Battle-terrain reader against a real install (read-only). All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test battle_terrain_install -- --ignored --nocapture
//! ```
//! The install path can be overridden with `NTW_DATA_DIR`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ntw_formats::battle_terrain::{self, BattleMap, HeightfieldSettings};
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_formats::tga::Tga;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// Every preset battle map loads: definition, heightfields, deployment, object lists, ground types.
#[test]
#[ignore]
fn every_battle_map_parses() {
    let vfs = vfs();
    let names = battle_terrain::list_presets(&vfs);
    assert!(names.len() >= 50, "only {} presets", names.len());
    let mut failures = Vec::new();
    for name in &names {
        let map = match BattleMap::load(&vfs, name) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let trees: usize = map.trees.iter().flat_map(|l| &l.groups).map(|g| g.instances.len()).sum();
        let (lo, hi) = map.ground().map_or((0.0, 0.0), |h| h.min_max_m());
        println!(
            "{name:32} {}x{} m, {} hf ({}), h {lo:.1}..{hi:.1} m, {} deploy setups, {}+{} buildings, {trees} trees, {} outlines, gt {:?}",
            map.definition.base_terrain_width,
            map.definition.base_terrain_height,
            map.heightfields.len(),
            map.ground().map_or("-".to_string(), |h| format!("{}x{}", h.width, h.height)),
            map.deployment.len(),
            map.buildings_near.len(),
            map.buildings_far.len(),
            map.outlines.len(),
            map.ground_types.as_ref().map(|g| (g.width, g.height, g.palette.len())),
        );
        for h in &map.heightfields {
            assert_eq!(h.samples.len(), (h.width * h.height) as usize, "{name}");
            assert!(h.settings.world_width > 0.0 && h.settings.scale.is_finite(), "{name}");
            // The exe rescales a `normalize` level by its own min/max (UNITS_TERRAIN_FIDELITY §5.1).
            // Every shipped level starts at 0 (so the exe's `+ min` term vanishes) and ends at
            // 65534 (22 levels) or 65535; `span` follows the level's own top.
            let (lo, hi) = h.samples.iter().fold((u16::MAX, 0u16), |(a, b), &s| (a.min(s), b.max(s)));
            assert!(h.settings.normalize, "{name}: a level without normalize");
            if lo != hi {
                assert_eq!(lo, 0, "{name}: normalised level does not start at 0");
                assert!(hi >= 65534, "{name}: normalised level tops out at {hi}");
                assert_eq!(h.span, f32::from(hi), "{name}");
            }
        }
        // Level N+1 covers twice the area of level N (INFERRED nesting; CONFIRMED by this test).
        for w in map.heightfields.windows(2) {
            assert_eq!(w[1].settings.world_width, 2.0 * w[0].settings.world_width, "{name}");
        }
        // Every ground-type cell's palette colour is in the exe's colour table (0x0145BF60), so
        // no shipped cell falls back to "no modifier" (GROUND_TYPE_UNMATCHED).
        if let Some(g) = &map.ground_types {
            assert!(g.cells.iter().all(|&c| c < 25), "{name}: a ground-type colour outside the exe table");
        }
        if let Some(h) = map.ground() {
            assert_eq!(h.settings.world_width, map.definition.base_terrain_width, "{name}: level 0 = base terrain");
            assert!(map.height_at(0.0, 0.0).is_finite());
        }
        // Every placed object lies inside the outermost heightfield.
        let half = map.heightfields.last().map_or(f32::MAX, |h| h.settings.world_width / 2.0);
        for b in map.buildings_near.iter().chain(&map.buildings_far) {
            assert!(b.position.0.abs() <= half && b.position.1.abs() <= half, "{name}: {b:?}");
        }
    }
    println!("{} presets, {} failures", names.len(), failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Every `.tga` in battleterrain.pack decodes, and every tile `.settings` ESF reads as
/// `HEIGHTFIELD_SETTINGS`.
#[test]
#[ignore]
fn every_terrain_tga_and_tile_settings_parse() {
    let vfs = vfs();
    let (mut tga_ok, mut tga_bad) = (0, Vec::new());
    let (mut set_ok, mut set_bad) = (0, Vec::new());
    for path in vfs.list("battleterrain\\") {
        if path.ends_with(".tga") {
            match Tga::parse(&vfs.read(path).unwrap()) {
                Ok(_) => tga_ok += 1,
                Err(e) => tga_bad.push(format!("{path}: {e}")),
            }
        } else if path.ends_with(".settings") {
            let esf = EsfFile::from_bytes(&vfs.read(path).unwrap());
            match esf.ok().and_then(|f| HeightfieldSettings::from_esf(&f.root)) {
                Some(_) => set_ok += 1,
                None => set_bad.push(path.to_owned()),
            }
        }
    }
    println!("tga: {tga_ok} ok, {} failed; tile settings: {set_ok} ok, {} failed", tga_bad.len(), set_bad.len());
    assert!(tga_bad.is_empty(), "{tga_bad:#?}");
    assert!(set_bad.is_empty(), "{set_bad:#?}");
}

/// Every `data.tree_model` parses to its last byte (`ntw_formats::vegetation::TreeModelFile`).
#[test]
#[ignore]
fn every_tree_model_parses() {
    use ntw_formats::vegetation::TreeModelFile;
    let vfs = vfs();
    let mut n = 0;
    for p in vfs.list(r"rigidmodels\vegetation\battle\") {
        if p.ends_with("data.tree_model") {
            let f = TreeModelFile::read(&vfs.read(p).unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
            assert_eq!(f.version, 1, "{p}");
            // Survey (no assertion): how many boxes have a symmetric vertical extent.
            let sym = f.records.iter().filter(|r| (r.values[9] + r.values[12]).abs() < 1e-3).count();
            println!("{p}: {} records, {sym} with a symmetric vertical extent", f.records.len());
            n += 1;
        }
    }
    println!("{n} tree_model files");
    assert_eq!(n, 11);
}

/// Every species of every preset map's tree lists resolves through `warscape_trees` to a `.spt`
/// that exists, its `data.tree_model` record and a composite-map block; every tree (not shrub)
/// has 8 billboard pictures and both textures exist.
#[test]
#[ignore]
fn every_tree_species_resolves() {
    use ntw_formats::vegetation::VegetationIndex;
    let vfs = vfs();
    let mut index = VegetationIndex::from_vfs(&vfs).expect("warscape_trees");
    let mut failures = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for name in battle_terrain::list_presets(&vfs) {
        let map = BattleMap::load(&vfs, &name).unwrap();
        for g in map.trees.iter().flat_map(|l| &l.groups) {
            if !seen.insert((g.species.to_ascii_lowercase(), map.definition.season.clone())) {
                continue;
            }
            let Some(a) = index.resolve(&vfs, &g.species, &map.definition.season) else {
                // The American climates (Empire leftovers) ship no vegetation files at all.
                if g.species.to_ascii_lowercase().starts_with("lc_am_") {
                    println!("{name}: {} has no files (lc_am_*)", g.species);
                    continue;
                }
                failures.push(format!("{name}: {} ({}) does not resolve", g.species, map.definition.season));
                continue;
            };
            if !vfs.contains(&a.spt_path) {
                failures.push(format!("{}: missing", a.spt_path));
            }
            if a.model.is_none() {
                failures.push(format!("{}: no data.tree_model record", a.spt_path));
            }
            let shrub = a.spt_path.to_ascii_lowercase().contains("_shrub");
            if !shrub && (a.composite.billboards.len() != 8 || !vfs.contains(&a.billboard_texture)) {
                failures.push(format!("{}: {} billboards, {}", a.spt_path, a.composite.billboards.len(), a.billboard_texture));
            }
            if shrub && !a.composite.billboards.is_empty() {
                failures.push(format!("{}: shrub with billboards", a.spt_path));
            }
            if !vfs.contains(&a.diffuse_texture) {
                failures.push(format!("{}: missing", a.diffuse_texture));
            }
        }
    }
    println!("{} species/season pairs", seen.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The tree instance's unmapped `I32` column (0-D round 11, item 3): it cannot be a per-tree
/// rotation. A rotation would be present on **every** instance; this census says it is 0 on
/// 3,843,976 of 3,844,367 and takes only the values 1 and 2 on the other 391, all of them in
/// unscaled lists. A yaw encoded as an integer would also have to cover 360 degrees, which three
/// values cannot. So the per-tree angle, if the game has one, is generated by the renderer
/// (UNKNOWN), as §4.1 says.
#[test]
#[ignore]
fn the_tree_instance_i32_is_not_a_rotation() {
    let vfs = vfs();
    let mut values: BTreeMap<i32, usize> = BTreeMap::new();
    // (map, list index, list is scaled, species) -> instances with a non-zero I32
    let mut where_: BTreeMap<(String, usize, bool, String), usize> = BTreeMap::new();
    let mut total = 0usize;
    for name in battle_terrain::list_presets(&vfs) {
        let Ok(map) = BattleMap::load(&vfs, &name) else { continue };
        for (li, list) in map.trees.iter().enumerate() {
            for g in &list.groups {
                for i in &g.instances {
                    total += 1;
                    *values.entry(i.flags).or_default() += 1;
                    if i.flags != 0 {
                        *where_.entry((name.clone(), li, list.flag, g.species.clone())).or_default() += 1;
                    }
                }
            }
        }
    }
    println!("{total} instances, I32 census {values:?}");
    for ((map, li, scaled, species), n) in &where_ {
        println!("  {map:24} list {li} (scaled {scaled}) {species:44} {n:4} instances with a non-zero I32");
    }
    let nonzero: usize = values.iter().filter(|(v, _)| **v != 0).map(|(_, n)| n).sum();
    assert_eq!(values.len(), 3, "the I32 takes only three values: {values:?}");
    assert!(nonzero * 1_000 < total, "{nonzero} of {total} instances carry a non-zero I32");
    // none of them is in a scaled list, so it is not the scale byte's companion either, and they
    // all sit on one preset's two outer lists -- a per-list property, never a per-instance angle.
    assert!(where_.keys().all(|(_, _, scaled, _)| !scaled), "a scaled list holds a non-zero I32");
    let maps: BTreeSet<&str> = where_.keys().map(|(m, ..)| m.as_str()).collect();
    println!("the non-zero I32 appears on {} preset(s): {maps:?}", maps.len());
    // The notes say all 391 are on `ottoman_great_fortress`; assert exactly that rather than a
    // looser "at most two" (review 0-D).
    assert_eq!(maps.len(), 1, "the non-zero I32 is spread over {maps:?}");
    assert_eq!(nonzero, 391, "the non-zero I32 count moved");
}

/// In every preset's 1v1 setup (the first block), the first area of each of the two alliances
/// faces towards the other alliance's area: checks the INFERRED orientation rule of both area
/// forms (centre/width/orientation and outline).
#[test]
#[ignore]
fn deployment_areas_face_each_other() {
    let vfs = vfs();
    let (mut checked, mut failures) = (0, Vec::new());
    for name in battle_terrain::list_presets(&vfs) {
        let map = BattleMap::load(&vfs, &name).unwrap();
        let Some(setup) = map.one_v_one() else { continue };
        let [a, b] = [0, 1].map(|i| setup.alliances.get(i).and_then(|al| al.areas.first()).copied());
        let (Some(a), Some(b)) = (a, b) else { continue };
        for (me, other) in [(a, b), (b, a)] {
            let (fx, fy) = me.facing_vector();
            let (dx, dy) = (other.centre.0 - me.centre.0, other.centre.1 - me.centre.1);
            let cos = (fx * dx + fy * dy) / (dx * dx + dy * dy).sqrt().max(1e-6);
            if cos < 0.5 {
                failures.push(format!("{name}: area at {:?} faces {:?}, enemy at {:?} (cos {cos:.2})", me.centre, (fx, fy), other.centre));
            }
        }
        checked += 1;
    }
    println!("{checked} presets with a 1v1 setup");
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The tree-instance scale byte: the survey that settles what it means (0-D, round 7).
/// Run: `cargo test -p ntw_formats --test battle_terrain_install -- --ignored --nocapture`.
#[test]
#[ignore]
fn every_scaled_tree_byte_lands_on_its_own_scale_band() {
    use ntw_formats::battle_terrain::{MAX_TREE_SCALE, MIN_TREE_SCALE};
    let vfs = vfs();
    let mut hist = [0usize; 256];
    let (mut total, mut scaled, mut lists, mut unscaled_lists) = (0usize, 0usize, 0usize, 0usize);
    let mut species: BTreeMap<String, (usize, u32, u32)> = BTreeMap::new();
    let mut flag_values: BTreeMap<i32, usize> = BTreeMap::new();
    for name in battle_terrain::list_presets(&vfs) {
        let Ok(map) = BattleMap::load(&vfs, &name) else { continue };
        for list in &map.trees {
            let n: usize = list.groups.iter().map(|g| g.instances.len()).sum();
            if n == 0 { continue }
            total += n;
            // Non-zero bytes in this list, counted for **every** list, so the unscaled-list
            // assertion below is a real check. (Round 7 skipped unscaled lists before counting,
            // which made that assertion always pass; fixed in review 0-D.)
            let mut nz = 0;
            for g in &list.groups {
                for i in &g.instances {
                    if i.variation != 0 { nz += 1 }
                    *flag_values.entry(i.flags).or_default() += 1;
                }
                if !list.flag {
                    continue;
                }
                // Only a scaled list carries a byte.
                let e = species.entry(g.species.clone()).or_insert((0, 255, 0));
                for i in &g.instances {
                    hist[i.variation as usize] += 1;
                    e.0 += 1;
                    e.1 = e.1.min(i.variation as u32);
                    e.2 = e.2.max(i.variation as u32);
                }
            }
            if list.flag {
                scaled += n;
                lists += 1;
                // Every byte of a scaled list is a real scale; none is left at the default.
                assert_eq!(nz, n, "{name}: a scaled list has {} zero bytes", n - nz);
            } else {
                unscaled_lists += 1;
                assert_eq!(nz, 0, "{name}: an unscaled list has {nz} nonzero bytes");
            }
        }
    }
    println!("{total} instances, {scaled} in {lists} scaled lists, {unscaled_lists} unscaled");
    println!("instance I32 values: {flag_values:?}");
    assert!(total > 3_000_000, "only {total} instances");
    assert!(lists >= 30, "only {lists} scaled lists");
    // The byte never reaches its own ends.
    assert_eq!(hist[1..63].iter().sum::<usize>(), 0, "bytes 1..=62 are used");
    assert_eq!(hist[254] + hist[255], 0, "bytes 254..=255 are used");
    // and it never asks for a scale outside the picker's own clamps except just past them.
    let below = hist[..64].iter().sum::<usize>();
    let above = hist[180..].iter().sum::<usize>();
    println!(
        "bytes 0..=63: {below} ({:.2}%), 64..=179: {}, 180..=255: {above} ({:.2}%)",
        100.0 * below as f64 / scaled as f64,
        hist[64..180].iter().sum::<usize>(),
        100.0 * above as f64 / scaled as f64
    );
    assert!(below * 200 < scaled, "{below} bytes ask for a scale under {MIN_TREE_SCALE}");
    assert!(above * 50 < scaled, "{above} bytes ask for a scale over {MAX_TREE_SCALE}");
    // How many shipped instances the (PROVISIONAL) `/ 128` decode clamps. `scale` clamps by
    // construction, so asserting its output is in range would prove nothing (round 7 did; removed
    // in review 0-D). The clamped share is what the decode costs if it is wrong.
    let clamped: usize = hist
        .iter()
        .enumerate()
        .filter(|(v, n)| {
            let raw = *v as f32 / 128.0;
            **n > 0 && *v != 0 && !(MIN_TREE_SCALE..=MAX_TREE_SCALE).contains(&raw)
        })
        .map(|(_, n)| n)
        .sum();
    println!("the /128 decode clamps {clamped} of {scaled} scaled instances");
    // Shrubs and trees share one byte range, so the byte is a multiplier, not a size.
    let (shrub, tree) = species
        .iter()
        .filter(|(k, _)| k.contains("shrub") || k.contains("_tree"))
        .fold((0usize, 0usize), |(a, b), (k, v)| {
            if k.contains("shrub") { (a + v.0, b) } else { (a, b + v.0) }
        });
    println!("{shrub} shrub instances, {tree} tree instances");
    assert!(shrub > 1000 && tree > 1000);
    for (k, (_, lo, hi)) in &species {
        if k.contains("shrub") {
            assert!(*hi >= 150, "{k}: shrub byte span must reach into the tree span ({lo}..{hi})");
            break;
        }
    }
    let row: Vec<String> = (1..256).filter(|&v| hist[v] > 0).map(|v| format!("{v}:{}", hist[v])).collect();
    println!("non-zero bytes: {}", row.join(" "));
}

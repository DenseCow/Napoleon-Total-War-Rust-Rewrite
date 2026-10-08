//! SpeedTree `.spt` reader against a real install (read-only). All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test speedtree_install -- --ignored --nocapture
//! ```
//! The install path can be overridden with `NTW_DATA_DIR`.

use std::path::PathBuf;

use ntw_formats::pack::Vfs;
use ntw_formats::speedtree::SptFile;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

fn spt_paths(vfs: &Vfs) -> Vec<String> {
    let mut v: Vec<String> = vfs
        .packs()
        .iter()
        .flat_map(|p| p.entries().iter().map(|e| e.path.clone()))
        .filter(|p| p.to_ascii_lowercase().ends_with(".spt"))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Every shipped `.spt` parses with the exe's grammar to its last byte.
#[test]
#[ignore]
fn every_spt_parses_to_end() {
    let vfs = vfs();
    let paths = spt_paths(&vfs);
    assert!(paths.len() >= 229, "only {} .spt files", paths.len());
    let mut failures = Vec::new();
    for p in &paths {
        let b = vfs.read(p).expect("read");
        match SptFile::read(&b) {
            Ok(f) if f.trailing == 0 => {
                assert!(!f.tree.branch_levels.is_empty(), "{p}: no branch levels");
            }
            Ok(f) => failures.push(format!("{p}: {} bytes not read", f.trailing)),
            Err(e) => failures.push(format!("{p}: {e}")),
        }
    }
    assert!(failures.is_empty(), "{} of {} failed:\n{}", failures.len(), paths.len(), failures.join("\n"));
}

/// The shrub layer (0-D round 11): how many shrub species the shipped maps use, what their `.spt`
/// generates, and whether the composite map gives them anything to texture it with. This is the
/// survey behind drawing shrubs as geometry at **every** distance: they have no billboard pictures
/// at all (`Billboards` sections empty, CONFIRMED by `every_tree_species_resolves`), so there is
/// nothing to cross-fade them to.
///
/// Prints, per species: instances over the presets, the maps that use it, the generated branch /
/// bark / leaf-card / frond counts, the composite map's leaf and frond rectangles and whether the
/// diffuse texture exists; then the per-map shrub census, which is what bounds the draw cost.
#[test]
#[ignore]
fn every_shrub_species_generates_drawable_geometry() {
    use std::collections::{BTreeMap, BTreeSet};
    use ntw_formats::battle_terrain::{self, BattleMap};
    use ntw_formats::speedtree::generate::compute_tree;
    use ntw_formats::speedtree::params::TreeParams;
    use ntw_formats::vegetation::VegetationIndex;
    let vfs = vfs();
    let mut index = VegetationIndex::from_vfs(&vfs).expect("warscape_trees");
    // full `.spt` path -> (instances, the maps that use it, the appearance it resolved to)
    let mut by_spt: BTreeMap<String, (usize, BTreeSet<String>, ntw_formats::vegetation::TreeAppearance)> = BTreeMap::new();
    // map -> (shrub instances, tree instances), the census that bounds the draw
    let mut per_map: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for name in battle_terrain::list_presets(&vfs) {
        let Ok(map) = BattleMap::load(&vfs, &name) else { continue };
        for list in &map.trees {
            for g in &list.groups {
                let Some(a) = index.resolve(&vfs, &g.species, &map.definition.season) else { continue };
                let e = by_spt.entry(a.spt_path.clone()).or_insert_with(|| (0, BTreeSet::new(), a.clone()));
                e.0 += g.instances.len();
                e.1.insert(name.clone());
                let m = per_map.entry(name.clone()).or_default();
                if is_shrub_path(&a.spt_path) { m.0 += g.instances.len() } else { m.1 += g.instances.len() }
            }
        }
    }
    let mut failures = Vec::new();
    let (mut shrub_species, mut tree_species) = (0usize, 0usize);
    let (mut shrub_instances, mut tree_instances) = (0usize, 0usize);
    let (mut no_leaves, mut no_bark, mut worst) = (0usize, 0usize, (0usize, String::new()));
    for (spt, (n, maps, a)) in &by_spt {
        let shrub = is_shrub_path(spt);
        if shrub { shrub_species += 1; shrub_instances += n } else { tree_species += 1; tree_instances += n }
        let Ok(bytes) = vfs.read(spt) else {
            failures.push(format!("{spt}: cannot re-read"));
            continue;
        };
        let f = match SptFile::read(&bytes) {
            Ok(f) => f,
            Err(e) => {
                failures.push(format!("{spt}: {e}"));
                continue;
            }
        };
        let t = TreeParams::from_spt(&f);
        let g = compute_tree(&t);
        let cards = g
            .leaves
            .iter()
            .filter(|l| !t.leaf_textures.get(l.texture).is_some_and(|x| x.mesh.is_some()))
            .count();
        let frond_tris: usize = g.frond_mesh.strips.iter().map(|s| s.len().saturating_sub(2)).sum();
        println!(
            "{spt:78} {n:6} inst / {:2} maps | {} bark strips, {cards:4} leaf cards, {frond_tris:4} frond tris | composite: {} leaf rects, {} frond rects, {} billboards{}",
            maps.len(),
            g.mesh.lod_strips.first().map_or(0, Vec::len),
            a.composite.leaves.len(),
            a.composite.fronds.len(),
            a.composite.billboards.len(),
            if vfs.contains(&a.diffuse_texture) { "" } else { "  MISSING DIFFUSE" }
        );
        if shrub {
            if !vfs.contains(&a.diffuse_texture) {
                failures.push(format!("{spt}: no diffuse texture {}", a.diffuse_texture));
            }
            // A shrub is drawn only if it has something to draw: leaf cards with a composite
            // rectangle, or fronds with one. Bark alone would be a bare stick.
            let cards_ok = cards > 0 && !a.composite.leaves.is_empty();
            let fronds_ok = !g.frond_mesh.strips.is_empty() && !a.composite.fronds.is_empty();
            if !cards_ok && !fronds_ok {
                no_leaves += 1;
                failures.push(format!("{spt}: shrub with neither drawable leaf cards ({cards}) nor fronds"));
            }
            if g.mesh.lod_strips.first().is_none_or(Vec::is_empty) { no_bark += 1 }
            if cards + frond_tris > worst.0 { worst = (cards + frond_tris, spt.clone()) }
        }
    }
    println!(
        "\n{shrub_species} shrub species ({shrub_instances} instances) and {tree_species} tree species ({tree_instances} instances) over the presets"
    );
    println!("shrubs with no bark (fronds and cards only): {no_bark}; with nothing drawable: {no_leaves}");
    println!("the heaviest shrub is {worst:?} (leaf cards + frond triangles per instance)");
    let mut densest = per_map.iter().map(|(k, v)| (v.0, k.clone())).collect::<Vec<_>>();
    densest.sort_unstable_by(|a, b| b.cmp(a));
    println!("shrub instances per map, densest first:");
    for (_, name) in densest.iter().take(10) {
        let (s, t) = per_map[name.as_str()];
        println!("  {name:36} {s:6} shrubs, {t:8} trees");
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(shrub_species >= 30, "only {shrub_species} shrub species");
    assert!(worst.0 < 2000, "{} is too heavy to draw everywhere ({})", worst.1, worst.0);
}

/// The billboard atlas's alpha channel (item 2): is it a **bimodal cut-out** — every texel either
/// fully opaque or fully transparent — or a soft ramp? The answer decides how much the alpha-test
/// reference matters, and it **refutes the cut-out reading**: the channel carries a wide spread of
/// partial alphas, so the reference is a real choice that visibly changes how much of the canopy
/// survives. `trees.rs` uses 0.33, which this test measures against the alternatives.
///
/// The rectangle is much larger than the tree in it (most of a picture's texels are the atlas's
/// empty surround), which is why "fully opaque" is a small minority; the surviving counts below are
/// therefore shares of the **0.10 baseline**, which is the natural reading of a cut-out.
#[test]
#[ignore]
fn billboard_atlas_alpha_is_a_soft_ramp() {
    use ntw_formats::dds::Dds;
    use ntw_formats::vegetation::read_composite_map;
    let vfs = vfs();
    /// The alpha-test references worth comparing, including the one `trees.rs` uses.
    const THRESHOLDS: [f32; 5] = [0.1, 0.2, 0.33, 0.5, 0.8];
    let mut checked = 0usize;
    let mut opaque = 0usize;
    let mut clear = 0usize;
    let mut partial = 0usize;
    let mut lo = 255u8;
    let mut hi = 0u8;
    let mut survive = [0usize; THRESHOLDS.len()];
    for p in vfs.list(r"rigidmodels\vegetation\battle\") {
        if !p.ends_with("_compositemap_diffuse_billboards.dds") {
            continue;
        }
        // the atlas sits in `<climate>\textures\`, its composite map in `<climate>\`
        let mut parts: Vec<&str> = p.rsplit('\\').collect();
        let stem = parts.remove(0).trim_end_matches("_diffuse_billboards.dds");
        parts.reverse(); // rsplit gave them backwards
        assert_eq!(parts.last(), Some(&"textures"), "{p}");
        parts.pop(); // the atlas's own folder
        let map_path = format!("{}\\{stem}.txt", parts.join("\\"));
        let map = vfs.read(&map_path).unwrap_or_else(|e| panic!("{map_path}: {e}"));
        let bytes = vfs.read(p).unwrap();
        let d = Dds::parse(&bytes).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert!(matches!(d.format, ntw_formats::dds::DdsFormat::Dxt3 | ntw_formats::dds::DdsFormat::Dxt5), "{p}: {:?}", d.format);
        let px = d.decode_rgba8(0);
        // Only the texels inside one tree's billboard rectangles count, so the histogram is about
        // the pictures rather than the atlas's empty space.
        let entries = read_composite_map(&String::from_utf8_lossy(&map));
        let (w, h) = (d.width, d.height);
        for e in entries.iter().filter(|e| !e.billboards.is_empty()).take(2) {
            for r in e.billboards.iter().take(2) {
                // `read_composite_map` has already flipped v to top-down texture space.
                let x0 = (r[0] * w as f32) as u32;
                let y0 = (r[1] * h as f32) as u32;
                let x1 = (r[2] * w as f32) as u32;
                let y1 = (r[3] * h as f32) as u32;
                for y in y0..y1.min(h) {
                    for x in x0..x1.min(w) {
                        let a = px[((y * w + x) * 4 + 3) as usize];
                        lo = lo.min(a);
                        hi = hi.max(a);
                        match a {
                            0 => clear += 1,
                            255 => opaque += 1,
                            _ => partial += 1,
                        }
                        for (k, t) in THRESHOLDS.iter().enumerate() {
                            if (a as f32 / 255.0) >= *t {
                                survive[k] += 1;
                            }
                        }
                    }
                }
            }
        }
        checked += 1;
    }
    let total = (opaque + clear + partial).max(1);
    println!("{checked} billboard atlases; alpha min {lo} max {hi}");
    println!(
        "  opaque {opaque}, fully clear {clear}, PARTIAL {partial} ({:.2}% of the texels inside billboard rectangles)",
        100.0 * partial as f64 / total as f64
    );
    let base = survive[0].max(1);
    println!("texels surviving each alpha-test reference (share of the 0.10 baseline):");
    for (k, t) in THRESHOLDS.iter().enumerate() {
        println!(
            "  {t:.2}  {:8} ({:.2}% of the rectangles, {:.1}% lost vs 0.10)",
            survive[k],
            100.0 * survive[k] as f64 / total as f64,
            100.0 * (base - survive[k]) as f64 / base as f64
        );
    }
    assert!(checked >= 8, "only {checked} atlases");
    assert!(hi == 255 && lo == 0, "the pictures' alpha does not span the whole range ({lo}..{hi})");
    // A soft ramp, not a two-level cut-out, so the reference really does change the picture:
    // CONFIRMED negative for "any threshold in (0, 1) looks the same".
    assert!(partial * 4 > opaque, "only {partial} partial texels against {opaque} opaque");
    // 0.33 (what we draw with) loses almost nothing to a true cut-out, so it is a safe default.
    assert!(survive[2] * 100 > base * 99, "0.33 loses too much of the picture");
}

/// Do shrub `.spt` files ship **more branch LOD levels** than tree ones? If they did, that would be
/// the shipped answer to "shrubs have no billboards, so how is a far shrub drawn?" — a coarse mesh
/// instead. Prints the LOD count (the `9007` lod-info token) and the LOD parameters (`9008`,
/// `9012`, `9013`, `9014`) per species, split shrub / tree.
#[test]
#[ignore]
fn shrubs_ship_no_extra_branch_lods() {
    use ntw_formats::speedtree::params::TreeParams;
    let vfs = vfs();
    let mut shrubs: Vec<(String, i32, [f32; 4])> = Vec::new();
    let mut trees: Vec<(String, i32, [f32; 4])> = Vec::new();
    for p in spt_paths(&vfs) {
        let f = SptFile::read(&vfs.read(&p).unwrap()).unwrap();
        let t = TreeParams::from_spt(&f);
        let (n, a, b, c, d) = t.branch_lods;
        let e = if p.to_ascii_lowercase().contains("shrub") { &mut shrubs } else { &mut trees };
        e.push((p, n, [a, b, c, d]));
    }
    let tally = |v: &[(String, i32, [f32; 4])]| {
        let mut counts: std::collections::BTreeMap<i32, usize> = Default::default();
        let mut params: std::collections::BTreeMap<String, usize> = Default::default();
        for (_, n, p) in v {
            *counts.entry(*n).or_default() += 1;
            *params.entry(format!("{p:?}")).or_default() += 1;
        }
        (counts, params)
    };
    let (sc, sp) = tally(&shrubs);
    let (tc, tp) = tally(&trees);
    println!("{} shrub species: LOD counts {sc:?}", shrubs.len());
    for (p, n) in &sp { println!("    {n:3} x {p}") }
    println!("{} tree species: LOD counts {tc:?}", trees.len());
    for (p, n) in &tp { println!("    {n:3} x {p}") }
    assert!(sc.len() <= 2, "shrubs ship many LOD counts: {sc:?}");
}

/// How many shrub instances a camera can actually see: the census above counts whole maps, but the
/// draw is streamed per 64 m cell, so what matters is the count inside a radius of the map centre.
/// This is the number that decides whether shrubs can be drawn as geometry out to
/// `SHRUB_DISTANCE` (`speedtree.rs`) without a frame-time blow-up. Prints the worst map for each
/// radius of 200 m (the tree far distance at the top quality setting), 400 m and 800 m.
#[test]
#[ignore]
fn shrub_instances_within_a_draw_radius() {
    use std::collections::BTreeMap;
    use ntw_formats::battle_terrain::{self, BattleMap};
    use ntw_formats::vegetation::VegetationIndex;
    const RADII: [f32; 3] = [200.0, 400.0, 800.0];
    let vfs = vfs();
    let mut index = VegetationIndex::from_vfs(&vfs).expect("warscape_trees");
    // map -> shrubs inside each radius
    let mut census: BTreeMap<String, [usize; RADII.len()]> = BTreeMap::new();
    for name in battle_terrain::list_presets(&vfs) {
        let Ok(map) = BattleMap::load(&vfs, &name) else { continue };
        for list in &map.trees {
            for g in &list.groups {
                let Some(a) = index.resolve(&vfs, &g.species, &map.definition.season) else { continue };
                if !is_shrub_path(&a.spt_path) {
                    continue;
                }
                let e = census.entry(name.clone()).or_default();
                for i in &g.instances {
                    for (k, r) in RADII.iter().enumerate() {
                        if i.position.0 * i.position.0 + i.position.1 * i.position.1 <= r * r {
                            e[k] += 1;
                        }
                    }
                }
            }
        }
    }
    for (k, r) in RADII.iter().enumerate() {
        let mut v: Vec<(usize, &str)> = census.iter().map(|(m, c)| (c[k], m.as_str())).collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        println!("\n== shrubs within {r} m of the map centre, densest first:");
        for (n, name) in v.iter().take(6) {
            println!("  {name:36} {n:6}");
        }
        assert!(v[0].0 < 60_000, "{} has {} shrubs within {r} m", v[0].1, v[0].0);
    }
}

/// Is a `.spt` path a shrub's? The shipped trees name themselves `<climate>-<name>_TREE.spt` /
/// `_SHRUB.spt`, and the composite maps agree (a `_TREE` block has 8 billboard pictures, a
/// `_SHRUB` block none), so the file name is the classifier.
fn is_shrub_path(path: &str) -> bool {
    path.to_ascii_lowercase().contains("shrub")
}

/// Every tree generates the same geometry twice (deterministic RNG), and the generated heights
/// agree with the game's own `data.tree_model` boxes (median ratio within 5 %).
#[test]
#[ignore]
fn generator_is_deterministic_and_matches_tree_model() {
    use ntw_formats::speedtree::generate::compute_tree;
    use ntw_formats::speedtree::params::TreeParams;
    use ntw_formats::vegetation::TreeModelFile;
    let vfs = vfs();
    let mut ratios = Vec::new();
    for p in spt_paths(&vfs) {
        let f = SptFile::read(&vfs.read(&p).unwrap()).unwrap();
        let t = TreeParams::from_spt(&f);
        let (a, b) = (compute_tree(&t), compute_tree(&t));
        assert_eq!(a.mesh.positions, b.mesh.positions, "{p}: branches differ between runs");
        assert_eq!(a.leaves.iter().map(|l| l.pos).collect::<Vec<_>>(), b.leaves.iter().map(|l| l.pos).collect::<Vec<_>>(), "{p}");
        assert_eq!(a.frond_mesh.positions, b.frond_mesh.positions, "{p}: fronds differ");
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for v in a.mesh.positions.iter().chain(a.leaves.iter().map(|l| &l.pos)).chain(a.frond_mesh.positions.iter()) {
            lo = lo.min(v[1]);
            hi = hi.max(v[1]);
        }
        let dir = p.rsplit_once('\\').unwrap().0;
        let tm = TreeModelFile::read(&vfs.read(&format!("{dir}\\data.tree_model")).unwrap()).unwrap();
        let r = tm.record(&p).unwrap_or_else(|| panic!("{p}: no data.tree_model record"));
        ratios.push((hi - lo) / (r.values[12] - r.values[9]));
    }
    ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = ratios[ratios.len() / 2];
    assert!((0.95..1.05).contains(&median), "median height ratio {median}");
}

/// `WIND_LADDER` (`ntw_formats::speedtree::wind`) against the shipped table it is said to be read
/// off: every `wind_levels` row's vector (columns 3, 4) has one of the ladder's five lengths, lies
/// on one axis, and each rung occurs once per direction. Added in review 0-D: the round-11 note
/// called the ladder CONFIRMED with no real-file test behind it.
#[test]
#[ignore]
fn wind_ladder_is_the_shipped_wind_levels_table() {
    use ntw_formats::db::{DbTable, Schema};
    use ntw_formats::speedtree::wind::WIND_LADDER;
    let vfs = vfs();
    let bytes = vfs.read(r"db\wind_levels_tables\wind_levels").expect("wind_levels ships");
    let t = DbTable::read(&bytes, &Schema::from_codes("s,s,f,f,i").expect("schema")).expect("wind_levels reads");
    assert_eq!(t.rows.len(), 10, "ten wind_levels rows");
    let mut seen = [0usize; WIND_LADDER.len()];
    for r in &t.rows {
        let (x, y) = (r[2].as_f32().expect("x"), r[3].as_f32().expect("y"));
        let len = (x * x + y * y).sqrt();
        println!("{:24} {:16} ({x}, {y}) |v| {len}", r[0].as_str().unwrap_or("?"), r[1].as_str().unwrap_or("?"));
        assert!(x == 0.0 || y == 0.0, "{:?}: a diagonal wind ({x}, {y})", r[0].as_str());
        let rung = WIND_LADDER.iter().position(|s| (s - len).abs() < 1e-3);
        let rung = rung.unwrap_or_else(|| panic!("{:?}: |v| {len} is not on the ladder", r[0].as_str()));
        seen[rung] += 1;
    }
    assert!(seen.iter().all(|n| *n == 2), "each rung once per direction: {seen:?}");
}

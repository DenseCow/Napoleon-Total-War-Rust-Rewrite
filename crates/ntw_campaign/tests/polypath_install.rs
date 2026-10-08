//! The original's campaign path search (`ntw_sim::campaign::polypath`) on the real maps
//! (read-only). Each test skips (passes, printing a note) without an install.
//! Counts: `cargo test -p ntw_campaign --test polypath_install -- --nocapture`.

use std::path::PathBuf;

use ntw_campaign::pathing::poly_map;
use ntw_formats::campaign_map::RegionMap;
use ntw_formats::campaign_pathfinding::PathfindingFile;
use ntw_sim::campaign::polypath::{byte_cost, dir_index, kind, Mover, PolyMap};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

const MAPS: [&str; 5] = ["nap_europe", "nap_italy", "nap_egypt", "nap_spain", "nap_tut"];

struct Map {
    name: &'static str,
    pf: PathfindingFile,
    regions: RegionMap,
}

fn maps() -> Vec<Map> {
    let dir = data_dir().join("campaign_maps");
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return Vec::new();
    }
    MAPS.iter()
        .map(|&name| {
            let d = dir.join(name);
            let pf = PathfindingFile::read(&std::fs::read(d.join("pathfinding.esf")).expect("pathfinding.esf")).expect("reads");
            let regions = RegionMap::read(&std::fs::read(d.join("regions.esf")).expect("regions.esf")).expect("reads");
            Map { name, pf, regions }
        })
        .collect()
}

fn build(m: &Map) -> PolyMap {
    let a = &m.pf.areas[0];
    let cells = a.grid.expand();
    let sets = a.grid.region_sets().expect("region table");
    poly_map(a, &cells, &sets)
}

/// The cell header bytes as cost multipliers (`byte · 0.00995 + 0.796`, CONFIRMED `0x00B205D0`):
/// on open land (cells that are one land polygon) the median multiplier is 1.1 (Europe, Italy) to
/// 1.66 (Egypt), so one action point buys about one map unit off-road (a General: 26 AP). Also
/// prints how often a byte equals the neighbour's byte for the opposite direction (the field is
/// smooth but not symmetric: the costs depend on the direction of travel).
#[test]
fn header_bytes_as_costs() {
    for m in maps() {
        let pm = build(&m);
        let g = &m.pf.areas[0].grid;
        let (w, h) = (g.cols as i32, g.rows as i32);
        let mut land: Vec<f32> = Vec::new();
        let (mut same, mut all) = ([0u32; 8], [0u32; 8]);
        for r in 0..h {
            for c in 0..w {
                let ci = (r * w + c) as usize;
                let range = pm.cell_first[ci] as usize..pm.cell_first[ci + 1] as usize;
                let a = &pm.header[ci];
                if range.len() == 1 && pm.kind[range.start] == kind::LAND && pm.region_id[range.start] != 1023 {
                    land.extend(a.iter().map(|&b| byte_cost(b)));
                }
                for (dc, dr) in [(1, 0), (-1, 1), (0, 1), (1, 1), (-1, -1), (0, -1), (1, -1), (-1, 0)] {
                    let (c2, r2) = (c + dc, r + dr);
                    if c2 < 0 || r2 < 0 || c2 >= w || r2 >= h {
                        continue;
                    }
                    let d = dir_index(dc, dr);
                    let b = &pm.header[(r2 * w + c2) as usize];
                    if a[d] != 255 && b[7 - d] != 255 {
                        all[d] += 1;
                        same[d] += u32::from(a[d] == b[7 - d]);
                    }
                }
            }
        }
        land.sort_by(f32::total_cmp);
        let median = land[land.len() / 2];
        let pct: Vec<u32> = (0..8).map(|d| (100.0 * same[d] as f32 / all[d].max(1) as f32).round() as u32).collect();
        println!(
            "{}: {} polygons (grid_data u32 {}), open-land multiplier median {median:.3} (p10 {:.3}, p90 {:.3}); byte d == neighbour 7-d: {pct:?} %",
            m.name,
            pm.len(),
            g.unknown_7,
            land[land.len() / 10],
            land[land.len() * 9 / 10]
        );
        assert!((0.79..2.0).contains(&median), "{}: median {median}", m.name);
        // The grid header's u32 is the total polygon count (CONFIRMED on Europe: 103563).
        assert_eq!(g.unknown_7 as usize, pm.len(), "{}", m.name);
    }
}

/// Armies reach most settlements of a map by land, every path polygon is one an army may enter,
/// costs grow along the path and never undercut the heuristic's floor (0.33 per unit).
#[test]
fn settlements_connect_by_land() {
    for m in maps() {
        let pm = build(&m);
        let towns: Vec<(&str, (f32, f32))> = m
            .regions
            .regions
            .iter()
            .filter(|r| !r.is_sea)
            .filter_map(|r| r.settlement.as_ref().map(|s| (r.key.as_str(), s.position)))
            .collect();
        if towns.len() < 2 {
            continue;
        }
        let road = vec![0.5; pm.region_sets.len()];
        let t0 = std::time::Instant::now();
        // The best of the first few starting towns (islands reach nothing by land).
        let mut best = (0, "", 0);
        let mut best_missed = Vec::new();
        for &(from_key, from) in towns.iter().take(4) {
            let (mut reached, mut via_road) = (0, 0);
            let mut missed = Vec::new();
            for &(key, to) in towns.iter().filter(|t| t.0 != from_key) {
                let Some(p) = pm.find_path(from, to, Mover::Land, &road) else {
                    // Unreachable means another land component (an island or cut-off area).
                    let (a, b) = (pm.locate(from.0, from.1, Mover::Land, 2), pm.locate(to.0, to.1, Mover::Land, 2));
                    if let (Some(a), Some(b)) = (a, b) {
                        assert_ne!(pm.component[0][a], pm.component[0][b], "{} {key}: same component but no path", m.name);
                    }
                    missed.push(key);
                    continue;
                };
                reached += 1;
                assert_eq!(p.points.len(), p.costs.len(), "{} {key}", m.name);
                assert!(p.polys.iter().all(|&q| Mover::Land.may_enter(pm.kind[q as usize])), "{} {key}", m.name);
                assert!(p.costs.windows(2).all(|w| w[1] >= w[0]), "{} {key}: costs not monotone", m.name);
                let straight = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
                assert!(*p.costs.last().unwrap() >= straight * 0.33 - 1e-3, "{} {key}", m.name);
                via_road += usize::from(p.polys.iter().any(|&q| pm.kind[q as usize] == kind::ROAD));
            }
            if reached > best.0 {
                best = (reached, from_key, via_road);
                best_missed = missed;
            }
        }
        println!(
            "{}: {} adjacency entries; from {}: {}/{} settlements by land ({} via roads); {:.2} s",
            m.name,
            pm.adj.len(),
            best.1,
            best.0,
            towns.len() - 1,
            best.2,
            t0.elapsed().as_secs_f32()
        );
        println!("  not reached: {best_missed:?}");
        assert!(best.0 * 10 >= (towns.len() - 1) * 6, "{}: only {} of {} settlements reachable", m.name, best.0, towns.len() - 1);
    }
}

/// Fleets find paths between sea points and never enter land polygons.
#[test]
fn fleets_stay_at_sea() {
    for m in maps() {
        let pm = build(&m);
        // Sea points: the centres of sea cells that are a single sea polygon.
        let sea: Vec<(f32, f32)> = (0..pm.cols * pm.rows)
            .filter(|&c| {
                let r = pm.cell_first[c as usize] as usize..pm.cell_first[c as usize + 1] as usize;
                r.len() == 1 && pm.kind[r.start] == kind::SEA
            })
            .map(|c| pm.centre(c))
            .collect();
        if sea.len() < 2 {
            continue;
        }
        let (a, b) = (sea[sea.len() / 3], sea[2 * sea.len() / 3]);
        let p = pm.find_path(a, b, Mover::Sea, &[]);
        if let Some(p) = &p {
            assert!(p.polys.iter().all(|&q| Mover::Sea.may_enter(pm.kind[q as usize])), "{}", m.name);
        }
        println!("{}: sea path {:?} -> {:?}: {:?} points", m.name, a, b, p.as_ref().map(|p| p.points.len()));
    }
}

/// Kind 3 polygons are the river strips (INFERRED): nearly every one lies in a cell crossed by a
/// river spline. Neither armies nor fleets may enter them, so rivers are crossed only where a road
/// (kind 6) or kind 7 polygon bridges them. Prints where kinds 6 and 7 meet rivers.
#[test]
fn rivers_are_kind_3_strips() {
    use ntw_formats::campaign_map::{CampaignMap, GameFiles, DISPLAY_TO_LOGIC};
    use ntw_formats::pack::Vfs;
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return;
    }
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let files = GameFiles { vfs: &vfs, data_dir: Some(&dir) };
    for name in MAPS {
        let map = CampaignMap::load(&files, name).expect("map");
        let pm = ntw_campaign::pathing::build_grid(&map).poly.expect("pathfinding.esf");
        let mut river = vec![false; (pm.cols * pm.rows) as usize];
        for (folder, sp) in map.splines.iter().filter(|(f, _)| f == "rivers") {
            let _ = folder;
            let pts: Vec<(f32, f32)> = sp.points.iter().map(|p| (p[0] * DISPLAY_TO_LOGIC, p[2] * DISPLAY_TO_LOGIC)).collect();
            for w in pts.windows(2) {
                for k in 0..=20 {
                    let t = k as f32 / 20.0;
                    let (x, z) = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
                    // The cell and its 8 neighbours (the strip follows the spline loosely).
                    if let Some((c, r)) = pm.cell_of(x, z) {
                        for (dc, dr) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                            let (c2, r2) = (c as i32 + dc, r as i32 + dr);
                            if c2 >= 0 && r2 >= 0 && c2 < pm.cols as i32 && r2 < pm.rows as i32 {
                                river[(r2 as u32 * pm.cols + c2 as u32) as usize] = true;
                            }
                        }
                    }
                }
            }
        }
        let on_river = |k: u8| -> (usize, usize) {
            let all: Vec<usize> = (0..pm.len()).filter(|&p| pm.kind[p] == k).collect();
            (all.iter().filter(|&&p| river[pm.poly_cell[p] as usize]).count(), all.len())
        };
        let (r3, n3) = on_river(kind::RIVER);
        let (r6, n6) = on_river(kind::ROAD);
        let (r7, n7) = on_river(kind::SHARED);
        println!("{name}: kind 3 near rivers {r3}/{n3}; kind 6 {r6}/{n6}; kind 7 {r7}/{n7}");
        // Egypt has no river splines (its Nile is in the textures only).
        if n3 > 0 && river.iter().any(|&b| b) {
            assert!(r3 * 10 >= n3 * 6, "{name}: only {r3} of {n3} kind 3 polygons near a river");
        }
        assert!(!Mover::Land.may_enter(kind::RIVER) && !Mover::Sea.may_enter(kind::RIVER));
    }
}

/// Bits 24..31 of a boundary's flags: one bit per direction index d (0 E, 1 NW, 2 N, 3 NE, 4 SW,
/// 5 S, 6 SE, 7 W): set when the polygon reaches that neighbour, i.e. has an outline edge along
/// that side (orthogonal d) or holds that corner (diagonal d); 0 on off-map and river polygons.
/// Holds for every polygon of a split cell on all 5 maps (whole cells carry 0xFF).
#[test]
fn direction_mask_bits_24_31() {
    for m in maps() {
        let a = &m.pf.areas[0];
        let g = &a.grid;
        let cells = g.expand();
        let w = g.cols as usize;
        let cs = g.cell_size as i64;
        let (mut n, mut ok) = (0u32, 0u32);
        let mut bad = Vec::new();
        for (ci, cell) in cells.iter().enumerate() {
            if cell.boundaries.len() < 2 {
                continue;
            }
            let (x0, z0) = (g.origin.0 as i64 + (ci % w) as i64 * cs, g.origin.1 as i64 + (ci / w) as i64 * cs);
            let corner = [(x0, z0), (x0, z0 + cs), (x0 + cs, z0), (x0 + cs, z0 + cs)];
            for b in &cell.boundaries {
                let off = b.list_offset();
                let k = a.lists[off] as usize;
                let pts: Vec<(i64, i64)> = a.lists[off + 1..off + 1 + k]
                    .iter()
                    .map(|&v| if v < 4 { corner[v as usize] } else { (a.vertices[v as usize].0 as i64, a.vertices[v as usize].1 as i64) })
                    .collect();
                let mut mask = 0u32;
                let has = |c: (i64, i64)| pts.contains(&c);
                // Diagonals: 1 NW, 3 NE, 4 SW, 6 SE.
                for (d, c) in [(1, corner[1]), (3, corner[3]), (4, corner[0]), (6, corner[2])] {
                    if has(c) {
                        mask |= 1 << d;
                    }
                }
                // Sides: an edge with both ends on the side line and non-zero length.
                for i in 0..pts.len() {
                    let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                    if p == q {
                        continue;
                    }
                    if p.0 == x0 + cs && q.0 == x0 + cs { mask |= 1 << 0; }
                    if p.1 == z0 + cs && q.1 == z0 + cs { mask |= 1 << 2; }
                    if p.1 == z0 && q.1 == z0 { mask |= 1 << 5; }
                    if p.0 == x0 && q.0 == x0 { mask |= 1 << 7; }
                }
                let hi = b.flags >> 24;
                if matches!(b.flags & 0xF, 2 | 3) {
                    // Off-map and river polygons (enterable by nobody) carry 0.
                    assert_eq!(hi, 0, "{}", m.name);
                    continue;
                }
                n += 1;
                ok += u32::from(hi == mask);
                if hi != mask && bad.len() < 6 {
                    bad.push((format!("{hi:#04x}"), format!("{mask:#04x}"), b.flags & 0xF));
                }
            }
        }
        println!("{}: bits 24..31 == direction mask in {ok}/{n} split-cell polygons; e.g. {bad:?}", m.name);
        assert_eq!(ok, n, "{}", m.name);
    }
}

/// Smoothing (`ntw_sim::campaign::polysmooth`) on real paths: same ends and total cost, costs
/// grow, the line is no longer than the search's, every point lies in a polygon an army may
/// enter, and it rarely falls back to the unsmoothed path.
#[test]
fn smoothed_paths_stay_walkable() {
    use ntw_sim::campaign::polysmooth::smooth;
    let len = |p: &[(f32, f32)]| p.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum::<f32>();
    for m in maps() {
        let pm = build(&m);
        let towns: Vec<(f32, f32)> = m.regions.regions.iter().filter(|r| !r.is_sea).filter_map(|r| r.settlement.as_ref().map(|s| s.position)).collect();
        let road = vec![0.5; pm.region_sets.len()];
        let (mut n, mut same, mut pts_before, mut pts_after, mut off) = (0, 0, 0, 0, 0);
        let t0 = std::time::Instant::now();
        for (i, &from) in towns.iter().enumerate().take(6) {
            for &to in towns.iter().skip(i + 1).step_by(3) {
                let Some(p) = pm.find_path(from, to, Mover::Land, &road) else { continue };
                let s = smooth(&pm, &p);
                n += 1;
                same += usize::from(s == p);
                pts_before += p.points.len();
                pts_after += s.points.len();
                assert_eq!(s.points.first(), p.points.first());
                assert_eq!(s.points.last(), p.points.last());
                assert_eq!(s.costs.last(), p.costs.last());
                assert_eq!(s.points.len(), s.costs.len());
                assert!(s.costs.windows(2).all(|w| w[1] >= w[0]), "{}", m.name);
                assert!(len(&s.points) <= len(&p.points) + 1e-3, "{}", m.name);
                for q in &s.points[1..s.points.len() - 1] {
                    let poly = pm.polygon_at(q.0, q.1).expect("on the map");
                    off += usize::from(!Mover::Land.may_enter(pm.kind[poly]));
                }
            }
        }
        println!(
            "{}: {n} paths smoothed in {:.2} s, {same} unchanged; points {pts_before} -> {pts_after}; {off} turning points outside army polygons",
            m.name,
            t0.elapsed().as_secs_f32()
        );
        assert!(same * 10 <= n.max(1), "{}: {same} of {n} not smoothed", m.name);
        assert_eq!(off, 0, "{}", m.name);
    }
}

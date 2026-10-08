//! Research helper for the farm ESF files (`.farm_manager`, `.farm_template_tile`; BACKLOG §1, notes in
//! `analysis/campaign/S1_LEFTOVERS.md` §2). Read-only.
//!
//! ```text
//! cargo run -p ntw_formats --release --example farm_probe -- show <record name> [max] [path substring]
//! cargo run -p ntw_formats --release --example farm_probe -- collision
//! ```
//! * `show`: prints the children of the first `max` records with that name in every farm file
//!   (optionally only files whose pack path contains the substring).
//! * `collision`: checks the `FARM_COLLISION` fields against their polygon (centre, min/max
//!   vertex distance, bounding box) over every farm file.
use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn short(n: &EsfNode) -> String {
    match n {
        EsfNode::Record(r) => format!("{{{} v{}: {}}}", r.name, r.version, r.children.iter().map(short).collect::<Vec<_>>().join(", ")),
        EsfNode::RecordArray(a) => format!("[{} x{}]", a.name, a.items.len()),
        EsfNode::Coord2dArray(v) => format!("coord2d[{}] {:?}", v.len(), v.iter().take(4).collect::<Vec<_>>()),
        EsfNode::U32Array(v) => format!("u32[{}] {:?}", v.len(), v.iter().take(8).collect::<Vec<_>>()),
        EsfNode::I32Array(v) => format!("i32[{}] {:?}", v.len(), v.iter().take(8).collect::<Vec<_>>()),
        n => format!("{n:?}"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let vfs = Vfs::open_install(std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into())).unwrap();
    let files: Vec<String> = vfs
        .list("battleterrain")
        .into_iter()
        .filter(|p| p.ends_with(".farm_manager") || p.ends_with(".farm_template_tile"))
        .map(str::to_owned)
        .collect();
    match args.first().map(String::as_str) {
        Some("show") => {
            let name = &args[1];
            let max: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
            let filter = args.get(3).cloned().unwrap_or_default();
            for p in files.iter().filter(|p| p.contains(&filter)) {
                let f = EsfFile::from_bytes(&vfs.read(p).unwrap()).unwrap();
                let mut n = 0;
                f.root.walk(&mut |r: &EsfRecord| {
                    if r.name == *name && n < max {
                        n += 1;
                        println!("{p}: {}", r.children.iter().map(short).collect::<Vec<_>>().join(" | "));
                    }
                });
            }
        }
        Some("wallpieces") => {
            // Per manager: the farm instances' (a, b) pieces against the wall instances (a = wall
            // index? b = side?) and the owner's second u32.
            use ntw_formats::battle_markers::FarmManager;
            for p in files.iter().filter(|p| p.ends_with(".farm_manager")) {
                let Ok(m) = FarmManager::read(&vfs.read(p).unwrap()) else { continue };
                let all: Vec<(usize, usize, &ntw_formats::battle_markers::FarmInstance)> = (0..2).flat_map(|l| m.farms[l].iter().enumerate().map(move |(i, f)| (l, i, f))).collect();
                let pieces: Vec<(u32, u32)> = all.iter().flat_map(|x| x.2.pieces.iter().copied()).collect();
                let max_a = pieces.iter().map(|x| x.0).max();
                let b_vals: std::collections::BTreeSet<u32> = pieces.iter().map(|x| x.1).collect();
                let unknown: std::collections::BTreeSet<u32> = all.iter().map(|x| x.2.placement.unknown).chain(m.walls.iter().map(|w| w.0.unknown)).collect();
                // Wall w lists farms (index, list, slot): does that farm carry a piece (w, _)?
                let (mut links, mut hit) = (0, 0);
                let mut side = std::collections::BTreeMap::new();
                for (w, (_, raw)) in m.walls.iter().enumerate() {
                    let n = raw.get(1).copied().unwrap_or(0) as usize;
                    for k in 0..n {
                        let (fi, li, slot) = (raw[2 + 3 * k], raw[3 + 3 * k], raw[4 + 3 * k]);
                        links += 1;
                        if let Some(f) = m.farms.get(li as usize).and_then(|l| l.get(fi as usize))
                            && let Some(pc) = f.pieces.iter().find(|pc| pc.0 as usize == w)
                        {
                            hit += 1;
                            *side.entry((k, slot, pc.1)).or_insert(0) += 1;
                        }
                    }
                }
                println!(
                    "{p}: templates {}, tile set templates {:?}, farms {}+{}, walls {}, pieces {} (max a {max_a:?}, b values {b_vals:?}), owner #2 values {unknown:?}; wall->farm links {links}, with a piece naming the wall {hit}; (link k, slot, b) {side:?}",
                    m.templates.len(),
                    m.tile_sets.iter().map(|t| t.template).collect::<std::collections::BTreeSet<_>>(),
                    m.farms[0].len(),
                    m.farms[1].len(),
                    m.walls.len(),
                    pieces.len()
                );
            }
        }
        Some("tilesets") => {
            // Per manager: the template's list sizes, then per FARM_TILE_SET entry (a, b), the
            // pair count / largest index / flags, and the two u32 lists' sizes and largest values.
            for p in files.iter().filter(|p| p.ends_with(".farm_manager")) {
                let f = EsfFile::from_bytes(&vfs.read(p).unwrap()).unwrap();
                let Some(m) = f.root.child("FARM_MANAGER") else { continue };
                let Some(t) = m.child("FARM_TILE_TEMPLATE") else { continue };
                let tpath = t.get_str(0).unwrap_or_default().replace('/', "\\").to_ascii_lowercase();
                let sizes = vfs.read(&tpath).ok().and_then(|b| EsfFile::from_bytes(&b).ok()).map(|tf| {
                    let n = |name: &str| tf.root.record_array(name).map_or(0, |a| a.items.len());
                    let roads = tf.root.child("ROAD_LIST").and_then(|r| r.get_u32(0)).unwrap_or(0);
                    (n("FARM_LIST"), n("WALL_LIST"), roads)
                });
                println!("{p}: template {tpath} (farms, walls, roads) {sizes:?}");
                for set in m.children_named("FARM_TILE_SET") {
                    let v: Vec<i64> = set.children.iter().skip(3).filter_map(EsfNode::as_int).collect();
                    // v: template index, entry count, then entries.
                    let mut i = 2;
                    for _ in 0..v.get(1).copied().unwrap_or(0) {
                        let (a, b) = (v[i], v[i + 1]);
                        let np = v[i + 2] as usize;
                        let pairs: Vec<(i64, i64)> = (0..np).map(|k| (v[i + 3 + 2 * k], v[i + 4 + 2 * k])).collect();
                        i += 3 + 2 * np;
                        let nk = v[i] as usize;
                        let ks = &v[i + 1..i + 1 + nk];
                        i += 1 + nk;
                        // v1 sets carry one more u32 list here (the reader skips it; the v2 writer drops it).
                        let nx = if set.version < 2 { let n = v[i] as usize; i += 1 + n; n } else { 0 };
                        let nj = v[i] as usize;
                        let js = &v[i + 1..i + 1 + nj];
                        i += 1 + nj;
                        let off = pairs.iter().filter(|q| q.1 == 0).count();
                        println!(
                            "  ({a}, {b}) pairs {np} max {:?} off {off} | k {nk} max {:?} | skipped {nx} | j {nj} max {:?}",
                            pairs.iter().map(|q| q.0).max(),
                            ks.iter().max(),
                            js.iter().max()
                        );
                    }
                }
            }
        }
        Some("pieces") => {
            // Per farm instance: its piece pairs next to the owner farm's collection sizes
            // (per FARM_DATA_COLLECTION: ID_LIST sizes and item list sizes).
            use ntw_formats::battle_markers::FarmManager;
            let mut shown = 0;
            for p in files.iter().filter(|p| p.ends_with(".farm_manager")) {
                let m = FarmManager::read(&vfs.read(p).unwrap()).unwrap();
                let Some(t) = m.templates.first() else { continue };
                let tf = EsfFile::from_bytes(&vfs.read(&t.tile_template.replace('/', "\\").to_ascii_lowercase()).unwrap()).unwrap();
                let Some(list) = tf.root.record_array("FARM_LIST") else { continue };
                for f in m.farms.iter().flatten() {
                    let Some(owner) = list.items.get(f.placement.owner as usize).and_then(|it| it.first()).and_then(EsfNode::as_record) else { continue };
                    let sizes: Vec<String> = owner
                        .children_named("FARM_DATA_COLLECTION")
                        .map(|c| {
                            c.children
                                .iter()
                                .map(|n| match n {
                                    EsfNode::Record(r) => r.get(0).and_then(EsfNode::as_u32_array).map_or(0, <[u32]>::len).to_string(),
                                    EsfNode::RecordArray(a) => format!("[{}]", a.items.len()),
                                    _ => "?".into(),
                                })
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect();
                    if shown < 25 {
                        shown += 1;
                        println!("{} owner {} pieces {:?} | collections {:?}", p.rsplit('\\').nth(1).unwrap_or(""), f.placement.owner, f.pieces, sizes);
                    }
                }
            }
        }
        Some("collision") => {
            // Counts: [records, inner radius = nearest edge, outer radius = farthest vertex,
            // box = polygon box grown by 12 and floored, >= 2 vertices on the outer circle,
            // centre = polygon box centre]
            let mut k = [0usize; 6];
            for p in &files {
                let f = EsfFile::from_bytes(&vfs.read(p).unwrap()).unwrap();
                f.root.walk(&mut |r: &EsfRecord| {
                    if r.name != "FARM_COLLISION" {
                        return;
                    }
                    let c = &r.children;
                    let (Some(EsfNode::Coord2d(cx, cy)), Some(EsfNode::Coord2dArray(poly)), Some(EsfNode::F32(r0)), Some(EsfNode::F32(r1)), Some(EsfNode::Coord2d(x0, y0)), Some(EsfNode::Coord2d(x1, y1))) =
                        (c.first(), c.get(1), c.get(2), c.get(3), c.get(4), c.get(5))
                    else {
                        return;
                    };
                    let (cx, cy) = (*cx, *cy);
                    k[0] += 1;
                    let near = |a: f32, b: f32| (a - b).abs() < 0.05 + b.abs() * 1e-4;
                    let (mut bx0, mut by0, mut bx1, mut by1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                    let (mut dmax, mut emin) = (0f32, f32::MAX);
                    for (i, &(ax, ay)) in poly.iter().enumerate() {
                        bx0 = bx0.min(ax);
                        by0 = by0.min(ay);
                        bx1 = bx1.max(ax);
                        by1 = by1.max(ay);
                        dmax = dmax.max(((ax - cx).powi(2) + (ay - cy).powi(2)).sqrt());
                        let (bx, by) = poly[(i + 1) % poly.len()];
                        let (dx, dy) = (bx - ax, by - ay);
                        let l2 = dx * dx + dy * dy;
                        let t = if l2 > 0.0 { (((cx - ax) * dx + (cy - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                        emin = emin.min(((ax + t * dx - cx).powi(2) + (ay + t * dy - cy).powi(2)).sqrt());
                    }
                    k[1] += usize::from(near(*r0, emin));
                    k[2] += usize::from(near(*r1, dmax));
                    let grown = [(bx0 - 12.0).floor(), (by0 - 12.0).floor(), (bx1 + 12.0).floor(), (by1 + 12.0).floor()];
                    k[3] += usize::from(near(*x0, grown[0]) && near(*y0, grown[1]) && near(*x1, grown[2]) && near(*y1, grown[3]));
                    let on_circle = poly.iter().filter(|&&(x, y)| near(((x - cx).powi(2) + (y - cy).powi(2)).sqrt(), *r1)).count();
                    k[4] += usize::from(on_circle >= 2);
                    k[5] += usize::from(near(cx, (bx0 + bx1) / 2.0) && near(cy, (by0 + by1) / 2.0));
                });
            }
            println!(
                "{} FARM_COLLISION: inner radius = nearest edge {}, outer radius = farthest vertex {}, box = polygon box +-12 floored {}, >=2 vertices on the outer circle {}, centre = box centre {}",
                k[0], k[1], k[2], k[3], k[4], k[5]
            );
        }
        _ => eprintln!("usage: farm_probe show NAME [max] [filter] | collision"),
    }
}

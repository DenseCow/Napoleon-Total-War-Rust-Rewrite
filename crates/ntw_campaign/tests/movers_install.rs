//! The kind-7 entry rule (`ntw_sim::campaign::movers`) on the real maps (read-only).
//! Each test skips (passes, printing a note) without an install.
//! Notes: `analysis/campaign/PATHFINDING_PORTS.md` §9.

use std::path::PathBuf;

use ntw_campaign::pathing::poly_map;
use ntw_formats::campaign_map::{CampaignMap, GameFiles};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::movers::{kind7_closed, open_shared, shared_group, Family};
use ntw_sim::campaign::polypath::{kind, Mover, PolyMap};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

const MAPS: [&str; 5] = ["nap_europe", "nap_italy", "nap_egypt", "nap_spain", "nap_tut"];

/// A point inside polygon `q`: its cell centre, its vertex average or an edge midpoint nudged
/// inwards, whichever lies in it.
fn point_in(pm: &PolyMap, q: usize) -> Option<(f32, f32)> {
    let o = pm.outline(q);
    let n = o.len() as f32;
    let avg = (o.iter().map(|v| v.0).sum::<f32>() / n, o.iter().map(|v| v.1).sum::<f32>() / n);
    let mut cands = vec![pm.centre(pm.poly_cell[q]), avg];
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        let mid = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        cands.push((mid.0 + (avg.0 - mid.0) * 0.05, mid.1 + (avg.1 - mid.1) * 0.05));
    }
    cands.into_iter().find(|p| pm.polygon_at(p.0, p.1) == Some(q))
}

/// Every settlement lies on kind 7 (a building footprint), and a land mover outside any building
/// (family A) still reaches every settlement of its land area from another settlement's doorstep,
/// walking round every other footprint on the way.
#[test]
fn every_settlement_stays_reachable() {
    let dir = data_dir();
    let Ok(vfs) = Vfs::open_install(&dir) else {
        eprintln!("skipped: no install");
        return;
    };
    let files = GameFiles { vfs: &vfs };
    let (mut checked, mut alone) = (0, 0);
    for name in MAPS {
        let Ok(map) = CampaignMap::load(&files, name) else { continue };
        let Some(pf) = map.pathfinding.as_ref() else { continue };
        let a = &pf.areas[0];
        let pm = poly_map(a, &a.grid.expand(), &a.grid.region_sets().expect("region table"));
        // (settlement position, its polygon, a doorstep point outside its footprint)
        let mut towns = Vec::new();
        for r in &map.regions.regions {
            let Some(s) = r.settlement.as_ref() else { continue };
            let p = pm.locate(s.position.0, s.position.1, Mover::Land, 2).expect("settlement on land");
            assert_eq!(pm.kind[p], kind::SHARED, "{name} {}: settlement off kind 7", r.key);
            let group = shared_group(&pm, p);
            let door = group
                .iter()
                .flat_map(|&g| pm.neighbours(g as usize).iter().copied())
                .filter(|&q| matches!(pm.kind[q as usize], kind::LAND | kind::ROAD))
                .find_map(|q| point_in(&pm, q as usize));
            towns.push((s.position, p, door, r.key.clone()));
        }
        for (i, &(to, gp, _, ref key)) in towns.iter().enumerate() {
            // Start at the doorstep of the next settlement in the same land area.
            let comp = pm.component[0][gp];
            let Some(&(_, _, Some(from), _)) = towns
                .iter()
                .cycle()
                .skip(i + 1)
                .take(towns.len() - 1)
                .find(|t| pm.component[0][t.1] == comp && t.2.is_some())
            else {
                alone += 1; // alone in its land area (an island with one settlement)
                continue;
            };
            let sp = pm.locate(from.0, from.1, Mover::Land, 2);
            let open = open_shared(&pm, sp, Some(gp));
            let closed = |q: usize| kind7_closed(&pm.view(), Family::A, &open, q);
            let path = pm
                .find_path_avoiding(from, to, Mover::Land, &[], &closed)
                .unwrap_or_else(|| panic!("{name}: {key} unreachable for a family A land mover"));
            assert_eq!(*path.polys.last().expect("non-empty") as usize, gp, "{name} {key}");
            for &q in &path.polys {
                assert!(pm.kind[q as usize] != kind::SHARED || open.contains(&q), "{name} {key}: crossed another footprint");
            }
            checked += 1;
        }
    }
    eprintln!("{checked} settlements reached, {alone} alone in their land area");
    assert!(checked > 100);
}

/// Closing kind 7 splits neither the land nor the sea: the largest land and sea areas keep every
/// polygon that is not kind 7 (so the kind 7 rules never cut a route; PATHFINDING_PORTS.md §10.3).
#[test]
fn closing_kind_7_splits_nothing() {
    let dir = data_dir();
    let Ok(vfs) = Vfs::open_install(&dir) else {
        eprintln!("skipped: no install");
        return;
    };
    let files = GameFiles { vfs: &vfs };
    for name in MAPS {
        let Ok(map) = CampaignMap::load(&files, name) else { continue };
        let Some(pf) = map.pathfinding.as_ref() else { continue };
        let a = &pf.areas[0];
        let pm = poly_map(a, &a.grid.expand(), &a.grid.region_sets().expect("region table"));
        // Components over `ok` kinds; returns the number of components with more than 50 polygons.
        let big = |ok: &dyn Fn(u8) -> bool| {
            let mut c = vec![false; pm.len()];
            let mut n = 0;
            for s in 0..pm.len() {
                if c[s] || !ok(pm.kind[s]) {
                    continue;
                }
                let (mut st, mut size) = (vec![s], 0);
                c[s] = true;
                while let Some(p) = st.pop() {
                    size += 1;
                    for &q in pm.neighbours(p) {
                        if !c[q as usize] && ok(pm.kind[q as usize]) {
                            c[q as usize] = true;
                            st.push(q as usize);
                        }
                    }
                }
                n += usize::from(size > 50);
            }
            n
        };
        assert_eq!(big(&|k| matches!(k, 0 | 6)), big(&|k| matches!(k, 0 | 6 | 7)), "{name}: land");
        assert_eq!(big(&|k| matches!(k, 1 | 4)), big(&|k| matches!(k, 1 | 4 | 7)), "{name}: sea");
    }
}

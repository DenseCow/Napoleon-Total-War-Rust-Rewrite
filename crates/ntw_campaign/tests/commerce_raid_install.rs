//! Commerce raids against the vanilla saves (read-only; skips without the evidence saves).
//!
//! Every `TRADE_SEGMENTS[]` item lists the domestic (#18) and international (#19) routes that run along
//! it. The raids rebuild those lists from the routes themselves ([`CampaignModel::registered_segments`]:
//! each waypoint's network leg, plus the settlement connector legs of a sea waypoint, each spline id `s`
//! naming segment `s >> 1`); the two must agree for every saved route.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_sim::campaign::{CampaignModel, FactionId, RegionId};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

type Lists = BTreeMap<u32, (Vec<FactionId>, Vec<(FactionId, RegionId)>)>;

fn derived(m: &CampaignModel) -> Lists {
    let legs = m.trade_leg_index();
    let mut out = Lists::new();
    let on = |p: &ntw_sim::campaign::trade::TradePath| m.registered_segments(&legs, p);
    for (f, paths) in &m.world.domestic_trade {
        for p in paths {
            for s in on(p) {
                out.entry(s).or_default().0.push(*f);
            }
        }
    }
    for ((a, _), paths) in &m.world.trade_paths {
        for p in paths {
            let Some(last) = p.waypoints.last() else { continue };
            for s in on(p) {
                out.entry(s).or_default().1.push((*a, last.region));
            }
        }
    }
    for v in out.values_mut() {
        v.0.sort_unstable();
        v.1.sort_unstable();
    }
    out
}

#[test]
fn route_legs_give_the_saved_segment_lists() {
    let dir = data_dir();
    let ev = common::evidence_dir();
    if !dir.is_dir() || !ev.is_dir() {
        eprintln!("skipped: no install or no evidence saves");
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("DB");
    let mut checked = 0;
    for name in ["auto_nr4_t4", "auto_after_c8", "orig_fr_t1", "orig_fr_may1811", "auto_orig_spa_0245"] {
        let path = ev.join(format!("{name}.save"));
        if !path.is_file() {
            continue;
        }
        let l = ntw_campaign::read_file(&path, &db).expect("load");
        let m = &l.model;
        let got = derived(m);
        let mut saved = Lists::new();
        for (i, s) in m.world.trade_sight.iter().enumerate() {
            let mut d = s.domestic.clone();
            let mut n = s.international.clone();
            if d.is_empty() && n.is_empty() {
                continue;
            }
            d.sort_unstable();
            n.sort_unstable();
            saved.insert(i as u32, (d, n));
        }
        let mut bad = 0;
        for s in saved.keys().chain(got.keys()).collect::<BTreeSet<_>>() {
            let (a, b) = (saved.get(s), got.get(s));
            if a != b {
                if bad < 10 {
                    eprintln!("{name}: segment {s}: saved {a:?} derived {b:?}");
                }
                bad += 1;
            }
        }
        eprintln!("{name}: {} segments with routes saved, {} derived, {bad} differ", saved.len(), got.len());
        assert_eq!(bad, 0, "{name}");
        assert!(m.world.trade_sight.iter().filter(|s| !s.curves.is_empty()).count() > 0, "{name}: no curves read");
        checked += saved.len();
    }
    println!("checked {checked} segments");
}

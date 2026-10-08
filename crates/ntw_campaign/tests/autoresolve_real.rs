//! The autoresolver with the shipped unit data (read-only; skips without the install).

use std::path::PathBuf;

use ntw_campaign::rules_from_db;
use ntw_data::GameDatabase;
use ntw_sim::campaign::autoresolve::{resolve, ArUnit, ArVars};
use ntw_sim::rng::CaRng;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

#[test]
fn shipped_units_fight_sensible_battles() {
    let dir = data_dir();
    if !dir.is_dir() {
        println!("SKIP: no install at {}", dir.display());
        return;
    }
    let db = GameDatabase::from_install(&dir).expect("database");
    let rules = rules_from_db(&db, "eur_napoleon");
    let v = ArVars::from_rules(&rules);
    assert_eq!(v.wipeout_threshold, 0.6);
    // Every land unit has autoresolve data with a positive potential at full strength; units with
    // a firearm or a gun have a missile potential.
    let mut land = 0;
    for (key, u) in &rules.units {
        let Some(st) = db.unit_stats(key) else { continue };
        let a = u.autoresolve.unwrap_or_else(|| panic!("{key}: no autoresolve data"));
        assert!(a.melee(u.men) + a.missile(u.men) > 0.0, "{key}");
        if st.ammunition > 0 && (st.projectile.is_some() || st.gun_type.is_some()) {
            assert!(a.missile(u.men) > 0.0, "{key}: no missile potential");
        }
        land += 1;
    }
    assert!(land > 100, "{land}");
    // Pick a line infantry, a heavy cavalry and a foot artillery unit (first in key order).
    let first = |class: &str| {
        let (k, u) = rules
            .units
            .iter()
            .find(|(k, u)| u.autoresolve.is_some() && u.men > 0 && db.unit(k).is_some_and(|r| r.unit_class == class))
            .unwrap();
        (k.clone(), ArUnit { data: u.autoresolve.unwrap(), men: u.men, has_general: false, general_rank: 0, human: false })
    };
    let (ki, inf) = first("infantry_line");
    let (kc, cav) = first("cavalry_heavy");
    let (ka, art) = first("artillery_foot");
    println!("infantry {ki}, cavalry {kc}, artillery {ka}");
    // Four infantry and a gun against one infantry: the big army wins and the small one is wiped out.
    let big = vec![inf, inf, inf, inf, art];
    let small = vec![inf];
    let mut rng = CaRng::new(1234);
    let o = resolve(&big, &small, &v, &mut rng);
    println!("{o:?}");
    assert!(o.a_won);
    assert_eq!(o.losses_b, vec![inf.men]);
    assert!(o.losses_a.iter().sum::<u32>() < big.iter().map(|u| u.men).sum::<u32>() / 2);
    // Equal armies: both sides win some battles; losses never exceed the men.
    let army = vec![inf, inf, cav, art];
    let mut a_wins = 0;
    for seed in 0..30 {
        let mut rng = CaRng::new(seed);
        let o = resolve(&army, &army, &v, &mut rng);
        a_wins += o.a_won as u32;
        for (u, l) in army.iter().zip(&o.losses_a).chain(army.iter().zip(&o.losses_b)) {
            assert!(*l <= u.men);
        }
    }
    println!("equal armies: A won {a_wins} of 30");
    assert!(a_wins > 3 && a_wins < 27, "{a_wins}");
}

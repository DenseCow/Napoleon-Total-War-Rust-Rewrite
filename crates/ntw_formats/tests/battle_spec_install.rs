//! Historical battle files against a real install (read-only). All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test battle_spec_install -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use ntw_formats::battle_spec::BattleSpec;
use ntw_formats::pack::Vfs;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// Every `*historical_battles\*\*_battle.xml` parses, has two alliances with a victory condition,
/// a duration, a timeout winner and a terrain preset; land battles have units with types and
/// frontages.
#[test]
#[ignore]
fn every_battle_file_parses() {
    let vfs = vfs();
    let files: Vec<String> = vfs
        .list("")
        .into_iter()
        .filter(|p| {
            let l = p.to_ascii_lowercase();
            l.contains("historical_battles\\") && l.ends_with("_battle.xml")
        })
        .map(str::to_owned)
        .collect();
    assert!(files.len() >= 29, "only {} battle files", files.len());
    for f in &files {
        let spec = BattleSpec::parse(&vfs.read(f).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
        let units: usize = spec.alliances.iter().flat_map(|a| &a.armies).map(|a| a.units.len()).sum();
        let ships: usize = spec.alliances.iter().flat_map(|a| &a.armies).map(|a| a.ships).sum();
        println!(
            "{f:70} {} alliances, {units} units, {ships} ships, {}s, timeout -> {:?}, {:?}",
            spec.alliances.len(),
            spec.description.duration.unwrap_or(0.0),
            spec.description.timeout_winner,
            spec.map_definition
        );
        assert!(spec.alliances.len() >= 2, "{f}");
        assert!(spec.alliances.iter().all(|a| !a.victory_conditions.is_empty()), "{f}: victory conditions");
        assert!(spec.description.duration.is_some() && spec.description.timeout_winner.is_some(), "{f}");
        assert!(spec.map_definition.is_some(), "{f}");
        assert!(spec.player_army().is_some(), "{f}");
        if !spec.is_naval() {
            for u in spec.alliances.iter().flat_map(|a| a.armies.iter().chain(&a.reinforcements)).flat_map(|a| &a.units) {
                assert!(!u.unit_type.is_empty() && u.width.is_some_and(|w| w > 0.0), "{f}: {u:?}");
            }
        }
    }
}

/// **BATTLE_FIDELITY.md §58 (5):** how much `unit_experience` the *shipped* battle files actually
/// carry, and how high. This decides whether §57 (5) ("whoever reads the battle-file
/// `unit_experience` level after the parser stores it") can matter for a historical battle at all.
/// Over every `*_battle.xml`: the files that carry the element, how many units, and the values.
/// A general's own `<experience>` is counted separately: it is a *display* field (the name and the
/// star rating next to it), not the unit's chevrons.
/// ```text
/// cargo test -p ntw_formats --test battle_spec_install unit_experience -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn battle_file_experience_coverage() {
    let vfs = vfs();
    let files: Vec<String> = vfs
        .list("")
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with("_battle.xml"))
        .map(str::to_owned)
        .collect();
    assert!(files.len() >= 29, "only {} battle files", files.len());
    let (mut files_with, mut units_total, mut units_with, mut generals_with) = (0usize, 0usize, 0usize, 0usize);
    let mut seen: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for f in &files {
        let spec = BattleSpec::parse(&vfs.read(f).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
        let units: Vec<&ntw_formats::battle_spec::SpecUnit> =
            spec.alliances.iter().flat_map(|a| a.armies.iter().chain(&a.reinforcements)).flat_map(|a| &a.units).collect();
        units_total += units.len();
        let here: Vec<u32> = units.iter().filter_map(|u| u.experience).collect();
        units_with += here.len();
        for &e in &here {
            *seen.entry(e).or_default() += 1;
        }
        generals_with += units.iter().filter(|u| u.general.as_ref().is_some_and(|g| g.experience.is_some())).count();
        if !here.is_empty() {
            files_with += 1;
            let mut v = here.clone();
            v.sort_unstable();
            println!("{f:70} {} units, experience {:?}", units.len(), v);
        }
    }
    println!("{files_with}/{} files carry <unit_experience level=...>; {units_with}/{units_total} units; generals with <experience>: {generals_with}", files.len());
    println!("experience levels seen (level -> units): {seen:?}");
    // The chevron levels are 0..9 (BATTLE_FIDELITY.md §2.1: byte `unit+0xD48` indexed into the
    // 10-row `unit_stats_land_experience_bonuses` / `_naval_` tables), so nothing outside 0..=9
    // may appear: a level of 10+ would index past the table and prove a different field.
    for &e in seen.keys() {
        assert!(e <= 9, "unit_experience level {e} is outside the 10 experience rows");
    }
}

/// The unit orientations follow the deployment-area convention (0 = +y, π/2 = +x): over the land
/// battles, each alliance's units on average face towards the other alliance (mean cosine well
/// above zero, and higher than with the other candidate convention 0 = +x counter-clockwise).
/// Single armies may face elsewhere (Lodi's Austrians face the river crossing).
#[test]
#[ignore]
fn armies_face_each_other() {
    let vfs = vfs();
    let (mut ours, mut other, mut n) = (0.0, 0.0, 0);
    for f in vfs.list("").into_iter().filter(|p| p.to_ascii_lowercase().contains("napoleon_historical_battles\\") && p.ends_with(".xml")) {
        let spec = BattleSpec::parse(&vfs.read(f).unwrap()).unwrap();
        if spec.is_naval() {
            continue;
        }
        let centre = |ai: usize| {
            let us: Vec<_> = spec.alliances[ai].armies.iter().flat_map(|a| &a.units).collect();
            let n = us.len().max(1) as f32;
            (us.iter().map(|u| u.position.0).sum::<f32>() / n, us.iter().map(|u| u.position.1).sum::<f32>() / n)
        };
        for ai in 0..2 {
            let (me, them) = (centre(ai), centre(1 - ai));
            let (dx, dy) = (them.0 - me.0, them.1 - me.1);
            let len = dx.hypot(dy);
            let us: Vec<_> = spec.alliances[ai].armies.iter().flat_map(|a| &a.units).collect();
            let mean_cos = us.iter().map(|u| (u.orientation.sin() * dx + u.orientation.cos() * dy) / len).sum::<f32>() / us.len() as f32;
            let alt = us.iter().map(|u| (u.orientation.cos() * dx + u.orientation.sin() * dy) / len).sum::<f32>() / us.len() as f32;
            println!("{f:70} alliance {ai}: mean cos to the enemy {mean_cos:.2} (other convention {alt:.2})");
            assert!(mean_cos > -0.2, "{f}: alliance {ai} faces away ({mean_cos:.2})");
            (ours, other, n) = (ours + mean_cos, other + alt, n + 1);
        }
    }
    let (ours, other) = (ours / n as f32, other / n as f32);
    println!("mean over {n} armies: {ours:.2} (other convention {other:.2})");
    assert!(ours > 0.6 && ours > other + 0.3);
}

//! `groupformations.bin` against a real install (read-only). `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test group_formation_install -- --ignored --nocapture
//! ```
use std::path::PathBuf;

use ntw_formats::group_formation::{self, GroupUnit, PURPOSE_DEPLOYMENT, Placement, Role};
use ntw_formats::pack::Vfs;

/// The exe's class code of a class key.
fn class_id(key: &str) -> u32 {
    u32::from(ntw_sim::unit_kind::class_code(key))
}

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// Every template reads to the last byte, anchors and group members name elements of the same
/// template, and class ids are known (or the 46 filler).
#[test]
#[ignore]
fn every_template_parses() {
    let bytes = vfs().read("groupformations.bin").expect("groupformations.bin");
    let raw = group_formation::read(&bytes).expect("parse");
    assert_eq!(raw.len(), 26);
    let ts: Vec<_> = raw.iter().map(group_formation::Template::from_raw).collect();
    assert_eq!(ts[0].name, "Multiple Selection Drag Out Land");
    assert!(ts.iter().any(|t| t.name == "Single Line Standard" && t.purposes & PURPOSE_DEPLOYMENT != 0));
    for t in &ts {
        for e in &t.elements {
            match &e.placement {
                Placement::Relative { anchor, .. } => assert!(t.elements.iter().any(|x| x.id == *anchor), "{}: anchor {anchor}", t.name),
                Placement::Group { members } => assert!(members.iter().all(|m| t.elements.iter().any(|x| x.id == *m)), "{}", t.name),
                _ => {}
            }
            assert!(e.classes.iter().all(|(c, _)| *c <= 46), "{}: {:?}", t.name, e.classes);
        }
        println!(
            "{:40} prio {:4} purposes {:#x} min% {:?} units {}..{} factions {:?}",
            t.name, t.priority, t.purposes, t.min_percent, t.min_units, t.max_units as i64, t.factions
        );
    }
}

/// A typical French army gets a line template, and no two units overlap.
#[test]
#[ignore]
fn a_line_army_gets_a_line_template() {
    let bytes = vfs().read("groupformations.bin").expect("groupformations.bin");
    let ts = group_formation::read_templates(&bytes).expect("parse");
    let line = |c: &str| GroupUnit { class: class_id(c), role: Role::Infantry, width: 53.0, depth: 4.0 };
    let mut units: Vec<GroupUnit> = (0..6).map(|_| line("infantry_line")).collect();
    units.push(line("infantry_light"));
    units.push(GroupUnit { class: class_id("artillery_foot"), role: Role::Artillery, width: 20.0, depth: 12.0 });
    units.push(GroupUnit { class: class_id("cavalry_heavy"), role: Role::Cavalry, width: 40.0, depth: 8.0 });
    units.push(GroupUnit { class: class_id("general"), role: Role::Cavalry, width: 10.0, depth: 6.0 });
    let i = group_formation::choose(&ts, &units, "france", PURPOSE_DEPLOYMENT);
    let t = &ts[i];
    let (score, a) = group_formation::assign(t, &units).expect("assign");
    let (pos, bounds) = group_formation::layout(t, &units, &a);
    println!("{} score {score}: {:?}\nbounds {bounds:?}", t.name, pos);
    assert!(t.name.contains("Line"), "{}", t.name);
    // The general stands behind the front line.
    assert!(pos[9].1 < pos[0].1, "general {:?} behind the line {:?}", pos[9], pos[0]);
    for i in 0..units.len() {
        for j in i + 1..units.len() {
            let (dx, dy) = ((pos[i].0 - pos[j].0).abs(), (pos[i].1 - pos[j].1).abs());
            let overlap = dx < (units[i].width + units[j].width) * 0.5 - 0.01 && dy < (units[i].depth + units[j].depth) * 0.5 - 0.01;
            assert!(!overlap, "units {i} and {j} overlap: {:?} {:?}", pos[i], pos[j]);
        }
    }
}

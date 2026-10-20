//! Campaign model tests: the script's shroud switches (`show_shroud`, `unveil_black_shroud`).

use super::*;
use super::visibility::{CellSet, Shroud, SightGrid};

/// A 64 × 64 grid of 1.25 centred on the origin (cell (32, 32) has its low corner at (0, 0)), with
/// factions A and B each keeping a shroud that has explored everything and lies wholly under the
/// black shroud, and one outline: the square (0, 0) .. (5, 5) named "square".
fn shrouded_model() -> CampaignModel {
    let mut m = test_model();
    m.world.sight_grid = Some(SightGrid::centred(64, 64, 64));
    let mut all = CellSet::new(64, 64);
    for x in 0..64 {
        for z in 0..64 {
            all.set(x, z);
        }
    }
    for f in [A, B] {
        m.world.shrouds.insert(f, Shroud { explored: all.clone(), visible: CellSet::new(64, 64), hidden: all.clone(), active: true });
    }
    m.world.black_shroud_outlines = vec![("square".into(), vec![(0.0, 0.0), (5.0, 0.0), (5.0, 5.0), (0.0, 5.0)])];
    m
}

#[test]
fn show_shroud_switches_every_factions_shroud() {
    // 0x00979260 → 0x00B68500 for every faction with a shroud (+0x6F8): the on flag (+0x50).
    let mut m = shrouded_model();
    m.show_shroud(false);
    assert!(m.world.shrouds.values().all(|s| !s.active));
    m.show_shroud(true);
    assert!(m.world.shrouds.values().all(|s| s.active));
}

#[test]
fn unveil_black_shroud_lifts_the_named_outline_as_the_exe() {
    let mut m = shrouded_model();
    assert_eq!(m.unveil_black_shroud("other"), 0, "an unknown name matches nothing");
    assert_eq!(m.unveil_black_shroud("Square"), 0, "names match exactly (CompareUniStrings)");
    assert!(m.world.shrouds.values().all(|s| s.hidden.len() == 64 * 64 && s.explored.len() == 64 * 64));
    assert_eq!(m.unveil_black_shroud("square"), 1);
    for s in m.world.shrouds.values() {
        // 0x00B42670 on the box widened (-2, -2) .. (+4, +0): cells x 30..=39, z 30..=36.
        let in_box = |x: u32, z: u32| (30..=39).contains(&x) && (30..=36).contains(&z);
        // 0x00E29E60: explored cells still under the black shroud in the box are forgotten; the
        // rest of the map keeps its explored cells.
        assert!(s.explored.cells().all(|(x, z)| !in_box(x, z)));
        assert_eq!(s.explored.len(), 64 * 64 - 70);
        // 0x00E2A180: a cell leaves the black shroud when its low corner is inside the square or
        // nearer it than 1.25 × 0.707 + 0.5 = 1.384.
        assert!(!s.hidden.get(32, 32), "corner (0, 0) on the outline");
        assert!(!s.hidden.get(34, 34), "corner (2.5, 2.5) inside");
        assert!(!s.hidden.get(36, 32), "corner (5, 0) on the outline");
        assert!(!s.hidden.get(37, 33), "corner (6.25, 1.25): 1.25 away");
        assert!(!s.hidden.get(31, 33), "corner (-1.25, 1.25): 1.25 away");
        assert!(s.hidden.get(30, 30), "corner (-2.5, -2.5): 3.5 away");
        assert!(s.hidden.get(38, 32), "corner (7.5, 0): 2.5 away");
        // The high z is not widened: cell z 37 (corner z 6.25, 1.25 away) is outside the box.
        assert!(s.hidden.get(32, 37));
        // Nothing outside the box changes.
        assert!((0..64).all(|x| (0..64).all(|z| in_box(x, z) || s.hidden.get(x, z))));
        // Lifted: 5 × 5 inside/on, plus the near ring the reach takes in.
        let lifted = 64 * 64 - s.hidden.len();
        assert!(lifted > 25 && lifted < 70, "{lifted}");
    }
    // Ground lifted from the black shroud shows as never seen until a unit sees it.
    let s = &m.world.shrouds[&A];
    assert!(!s.explored.get(34, 34) && !s.hidden.get(34, 34));
}

#[test]
fn unveil_black_shroud_needs_three_points_and_a_grid() {
    // 0x00B65C50 skips an outline of fewer than 3 points; without a grid nothing is lifted.
    let mut m = shrouded_model();
    m.world.black_shroud_outlines.push(("line".into(), vec![(0.0, 0.0), (5.0, 5.0)]));
    assert_eq!(m.unveil_black_shroud("line"), 1);
    assert!(m.world.shrouds.values().all(|s| s.hidden.len() == 64 * 64 && s.explored.len() == 64 * 64));
    m.world.sight_grid = None;
    assert_eq!(m.unveil_black_shroud("square"), 0);
    assert!(m.world.shrouds.values().all(|s| s.hidden.len() == 64 * 64));
}

#[test]
fn unveil_black_shroud_lifts_a_box_reaching_below_cell_0() {
    // ORIGINAL BUG (0x00B42670): the exe turns a negative start cell back into its edge through an
    // unsigned conversion, so this box's low edge lands 2^32 cells away and nothing is lifted;
    // ours keeps the box's own edge. The square (-40, -40) .. (-35, -35) sits on the grid's low
    // corner (origin (-40, -40)): widened, cells x -2..=7, z -2..=4, of which x 0..=7, z 0..=4 are
    // on the grid.
    let mut m = shrouded_model();
    m.world.black_shroud_outlines = vec![("corner".into(), vec![(-40.0, -40.0), (-35.0, -40.0), (-35.0, -35.0), (-40.0, -35.0)])];
    assert_eq!(m.unveil_black_shroud("corner"), 1);
    for s in m.world.shrouds.values() {
        let in_box = |x: u32, z: u32| x <= 7 && z <= 4;
        // Step 2 forgets only the box's cells on the grid.
        assert!(s.explored.cells().all(|(x, z)| !in_box(x, z)));
        assert_eq!(s.explored.len(), 64 * 64 - 40);
        // Step 3 lifts the square's cells as anywhere else on the map.
        assert!(!s.hidden.get(0, 0), "corner (-40, -40) on the outline");
        assert!(!s.hidden.get(2, 2), "corner (-37.5, -37.5) inside");
        assert!(!s.hidden.get(4, 0), "corner (-35, -40) on the outline");
        assert!(!s.hidden.get(5, 1), "corner (-33.75, -38.75): 1.25 away");
        assert!(s.hidden.get(6, 0), "corner (-32.5, -40): 2.5 away");
        assert!(s.hidden.get(0, 5), "z 5 is outside the box (the high z is not widened)");
        assert!((0..64).all(|x| (0..64).all(|z| in_box(x, z) || s.hidden.get(x, z))));
    }
}

#[test]
fn a_full_cell_set_holds_every_cell_and_no_more() {
    // 0x00DF3190 with fill 1; the bits past cols × rows stay clear, so `len` counts real cells.
    let s = CellSet::full(5, 3);
    assert_eq!(s.len(), 15);
    assert!(s.get(4, 2) && !s.get(5, 0) && !s.get(0, 3));
    assert_eq!(CellSet::full(64, 64).len(), 64 * 64);
}

/// The campaign start (`0x008DD090`): human A has no shroud, B (not human) keeps the start
/// position's one, and the map has `outlines`.
fn starting_model(outlines: bool) -> CampaignModel {
    let mut m = shrouded_model();
    m.world.shrouds.remove(&A);
    if !outlines {
        m.world.black_shroud_outlines.clear();
    }
    m.turn.humans = vec![A];
    m
}

#[test]
fn a_new_human_shroud_starts_under_the_black_shroud_when_the_map_has_outlines() {
    // 0x00AE8910: hidden is built with fill (outline count != 0); explored and visible empty, on;
    // then 0x00B66EF0(1) rebuilds visible and sets explored to it. B is not human: dropped.
    let mut m = starting_model(true);
    m.start_shrouds();
    assert_eq!(m.world.shrouds.keys().copied().collect::<Vec<_>>(), [A]);
    let s = &m.world.shrouds[&A];
    assert_eq!((s.hidden.cols, s.hidden.rows), (64, 64));
    assert_eq!(s.hidden.len(), 64 * 64);
    assert!(s.active);
    assert_eq!(s.explored, s.visible);
    let mut expected = m.clone();
    expected.refresh_shroud(A, true);
    assert_eq!(expected.world.shrouds[&A], m.world.shrouds[&A], "the new shroud already sees what its sources see");
}

#[test]
fn a_new_human_shroud_has_no_black_shroud_on_a_map_without_outlines() {
    let mut m = starting_model(false);
    m.start_shrouds();
    let s = &m.world.shrouds[&A];
    assert!(s.hidden.is_empty() && s.active);
    assert_eq!((s.hidden.cols, s.hidden.rows), (64, 64));
}

#[test]
fn a_human_shroud_the_start_position_gave_is_rebuilt_to_what_is_seen() {
    // 0x008DD090 creates a shroud only when faction +0x6F8 is null, but runs 0x00B66EF0(1) on every
    // human's: visible cleared and rebuilt from the sources, explored a copy of it (0x00E02070);
    // the black shroud is kept.
    let mut m = shrouded_model();
    m.turn.humans = vec![A];
    let mut own = m.world.shrouds[&A].clone();
    own.hidden.unset(3, 3);
    own.visible.set(5, 5);
    m.world.shrouds.insert(A, own.clone());
    let seen = m.compute_visible(A).unwrap();
    assert!(seen.len() < 64 * 64 && !seen.get(5, 5), "the start position explored more than is seen");
    m.start_shrouds();
    let s = &m.world.shrouds[&A];
    assert_eq!(s.explored, seen, "explored replaced, not added to");
    assert_eq!(s.visible, seen);
    assert_eq!(s.hidden, own.hidden);
    assert!(s.active);
    assert!(!m.world.shrouds.contains_key(&B));
}

#[test]
fn the_rebels_keep_the_start_positions_shroud() {
    // 0x008DD090 skips the whole setup for a faction without a record (IsFactionWithoutRecord
    // 0x008CEEF0), so the rebels' shroud is neither dropped nor rebuilt.
    let mut m = shrouded_model();
    m.world.factions.get_mut(&B).unwrap().key.clear();
    m.turn.humans = vec![A];
    let theirs = m.world.shrouds[&B].clone();
    m.start_shrouds();
    assert_eq!(m.world.shrouds.get(&B), Some(&theirs));
    assert!(m.world.shrouds.contains_key(&A));
}

#[test]
fn without_a_sight_grid_no_shroud_is_made() {
    let mut m = starting_model(true);
    m.world.sight_grid = None;
    m.start_shrouds();
    assert!(m.world.shrouds.is_empty(), "B dropped, A not sized");
}

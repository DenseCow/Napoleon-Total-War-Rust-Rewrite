//! Mover families and the kind-7 entry rule of the original campaign pathfinder.
//!
//! **Mover types** (CONFIRMED, `analysis/campaign/PATHFINDING_PORTS.md` §9): the exe gives a
//! moving character one of four type triples, always ordered (army, other land character,
//! fleet): the army test is `0x009D0F50` (the character's type record has an army), "land" is
//! the mover's virtual `+0x24`, everything else is naval.
//! - `0x0095E3D0`: 0 / 1 / 2, or the next getter's 3 / 4 / 5 when the character's flag `+0x4E8` is
//!   set (set by `0x009DA300`, which also snaps the character onto a building slot of its region
//!   within 1 unit, cleared by `0x00A0C810`; INFERRED: "inside a building", i.e. garrisoned);
//! - `0x0095E4C0`: 3 / 4 / 5;  `0x0095E4F0`: 9 / 10 / 11;  `0x0095E520`: 6 / 7 / 8.
//!
//! The move-to-a-character query (`0x00B21510` → `0x00B18F40`) starts the mover with its
//! `0x0095E520` type (6 / 7 / 8) and gives the goal the target's `0x0095E4C0` type (3 / 4 / 5); the
//! other query builders take the mover's type from `0x0095E3D0`. Search nodes inherit the type of
//! the node they are expanded from (`0x00B2CC20` → `0x00AF3520`, which only turns 9 / 10 / 11 into
//! 3 / 4 / 5 off a kind 7 polygon). So a character moves as **family A** (types 0..2 / 6..8)
//! unless it starts inside a building, then as **family B** (3..5 / 9..11).
//!
//! **Kind 7 rules** (CONFIRMED, both checked on the current node L and the neighbour polygon N in
//! every neighbour expansion of every search variant, `0x00B2CC20` and its siblings):
//! - `0x00B16A30`: L of type 0, 1, 2, 6, 7, 8 (family A) outside kind 7 may not step into kind 7;
//! - `0x00B167B0`: L of type 3, 4, 5 inside kind 7 may not step out of it (types 9, 10, 11 may,
//!   becoming 3 / 4 / 5 outside; nothing turns them back into 9..11).
//!
//! So a family A mover never enters a building footprint (settlements, ports, forts; the static
//! grid has them baked in as kind 7) from outside, and a family B mover can enter one but not leave
//! it again: for both, the kind 7 polygons usable on a path are those of the group the mover starts
//! in and, for family B, one more group where the path ends. The data agree: kind 7 never joins two
//! land areas nor two seas (removing it leaves both connected as before), and every settlement lies
//! on kind 7 (`ntw_campaign` test `movers_install.rs`).
//!
//! How a family A mover enters the settlement it is ordered into is not decoded: no search can
//! take it there (UNKNOWN; INFERRED: the order code puts it in, `0x009DA300` snaps the character
//! onto the building slot and sets `+0x4E8`). Ours: the kind 7 group holding the goal stays open
//! for both families (CONFIRMED for family B, PROVISIONAL for family A), every other kind 7
//! polygon is closed, for armies, agents and fleets alike (fleets enter ports through
//! `super::embark`'s port nodes as the original does, `0x00B2CC20`, CONFIRMED).

use std::collections::HashSet;

use super::polypath::{kind, PolyMap, View};

/// The two type families of the original's movers (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Family {
    /// Types 0..2 / 6..8: may not step into kind 7 from outside.
    A,
    /// Types 3..5 / 9..11: may not step out of kind 7 (9..11 may, once).
    B,
}

impl Family {
    /// The family a character moves with: B inside a building (garrisoned; INFERRED meaning of
    /// the flag `+0x4E8` read by `0x0095E3D0`), else A.
    pub fn of(in_building: bool) -> Family {
        if in_building { Family::B } else { Family::A }
    }
}

/// The kind 7 group of polygon `p`: `p` and every kind 7 polygon reachable from it through kind
/// 7 neighbours. Empty when `p` is not kind 7.
pub fn shared_group(pm: &PolyMap, p: usize) -> Vec<u32> {
    if pm.kind.get(p) != Some(&kind::SHARED) {
        return Vec::new();
    }
    let mut seen = vec![p as u32];
    let mut stack = vec![p];
    while let Some(q) = stack.pop() {
        for &n in pm.neighbours(q) {
            if pm.kind[n as usize] == kind::SHARED && !seen.contains(&n) {
                seen.push(n);
                stack.push(n as usize);
            }
        }
    }
    seen
}

/// The kind 7 polygons a family A mover going from polygon `start` to polygon `goal` may use: the
/// group it starts in (CONFIRMED rule) and the goal's group (PROVISIONAL, module docs). Every
/// other kind 7 polygon is closed to it.
pub fn open_shared(pm: &PolyMap, start: Option<usize>, goal: Option<usize>) -> HashSet<u32> {
    start.into_iter().chain(goal).flat_map(|p| shared_group(pm, p)).collect()
}

/// Is polygon `p` closed to a mover of `family` by the kind 7 rules, given the `open` set of
/// [`open_shared`]? (The same answer for both families, module docs.)
pub fn kind7_closed(view: &View<'_>, family: Family, open: &HashSet<u32>, p: usize) -> bool {
    // Family A may not enter, family B may not leave: the same polygons stay usable.
    let _ = family;
    view.kind(p) == kind::SHARED && !open.contains(&(p as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::polypath::{CellInput, Mover, PolyInput};

    /// A 4 x 1 strip of 2-unit cells: land, footprint A (kind 7), land, footprint B (kind 7), each
    /// cell one polygon; plus a second row of land under it so movers can walk round.
    fn strip() -> PolyMap {
        let fx = |v: f32| (v * 1_048_576.0) as i32;
        let sq = |c: i32, r: i32| -> Vec<(i32, i32)> {
            let (x0, z0, x1, z1) = (fx(c as f32 * 2.0), fx(r as f32 * 2.0), fx(c as f32 * 2.0 + 2.0), fx(r as f32 * 2.0 + 2.0));
            vec![(x0, z0), (x1, z0), (x1, z1), (x0, z1)]
        };
        let mut cells = Vec::new();
        for r in 0..2 {
            for c in 0..5 {
                let k = if r == 1 && (c == 1 || c == 3) { kind::SHARED } else { kind::LAND };
                cells.push(CellInput { header: [0; 8], polys: vec![PolyInput { kind: k, region_id: 1023, outline: sq(c, r) }] });
            }
        }
        PolyMap::build((0, 0), fx(2.0), 5, 2, &cells, Vec::new())
    }

    #[test]
    fn family_a_walks_round_other_footprints() {
        let pm = strip();
        let at = |x: f32, z: f32| pm.polygon_at(x, z).unwrap();
        let (from, to) = ((1.0, 3.0), (9.0, 3.0));
        let (sp, gp) = (at(from.0, from.1), at(to.0, to.1));
        assert_eq!(pm.kind[at(3.0, 3.0)], kind::SHARED);
        let open = open_shared(&pm, Some(sp), Some(gp));
        let closed = |p: usize| kind7_closed(&pm.view(), Family::A, &open, p);
        let path = pm.find_path_avoiding(from, to, Mover::Land, &[], &closed).expect("path");
        // Footprint A at (3, 3) and footprint B at (7, 3) are closed: the path dips into row 0.
        assert!(path.polys.iter().all(|&p| pm.kind[p as usize] != kind::SHARED));
        // Sent into footprint B, the path ends in it (its group is open) but never crosses A.
        let gb = at(7.0, 3.0);
        let open = open_shared(&pm, Some(sp), Some(gb));
        let closed = |p: usize| kind7_closed(&pm.view(), Family::A, &open, p);
        let path = pm.find_path_avoiding(from, (7.0, 3.0), Mover::Land, &[], &closed).expect("path");
        assert_eq!(*path.polys.last().unwrap() as usize, gb);
        assert!(!path.polys.contains(&(at(3.0, 3.0) as u32)));
        // Family B: it could enter footprint A but not leave it, so A is closed to it too.
        assert!(kind7_closed(&pm.view(), Family::B, &HashSet::new(), at(3.0, 3.0)));
    }
}

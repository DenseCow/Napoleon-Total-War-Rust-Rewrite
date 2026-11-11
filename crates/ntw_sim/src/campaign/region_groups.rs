//! The campaign AI's **region groups** and their change state (belief 0x52, `analysis/fidelity/
//! CAMPAIGN_FIDELITY.md` "Region groups"), CONFIRMED but for the one INFERRED step under "When the
//! exe's state is refreshed": what the deal evaluation's region worth
//! ([`super::region_value::faction_value`], `0x00AA1E90`) reads through `0x00A634D0`.
//!
//! * **Groups** (builder `0x00A67A90` / `0x00A31680`, flood `0x00A68120`): the land regions joined
//!   across their CAI boundaries (`+0x174`, the other side by `0x00C1BCE0`) when the neighbour has
//!   the same owner (region virtual `+0xC`), that owner is not the rebels (`IsCaiFactionRebels`:
//!   a rebel region is a group of its own), it has the same area type (`+0x190 → +0xC`: seas and
//!   theatres never join a land region) and the two lie on one land mass (`0x00C3D6E0`,
//!   [`super::World::region_land_masses`]). Each group keeps its owner (`+0x138`, `0x00A88770`)
//!   and members (count `+0x108`, data `+0x10C`: `0x00A6FE10`, `0x00A71BC0`).
//! * **Change state** (belief update `0x00AB6ED0`, accessor `0x00A75460`): for the region's
//!   owner (`0x00A734B0`), LOST when its group is the region alone, REDUCED when the group has two
//!   regions, else SPLIT when the rest of the group falls apart without the region ([`split`]),
//!   REDUCED otherwise. For any other faction (`0x00A73660`), the groups of the region's
//!   neighbours outside its own group (distinct, `0x00A753D0`) of its area type that the faction
//!   owns: none NEW, one INCREASED, two or more MERGED. A faction with no entry reads NEW
//!   (`0x00A634D0`).
//!
//! **When the exe's state is refreshed.** Ours is computed from the current owners at every
//! evaluation; the exe keeps the groups and each region's entry as beliefs and refreshes a dirty one
//! before it is read. CONFIRMED: the accessor `0x00A75460` refreshes the region's entry when its
//! dirty byte is set (`+0xF8`, `0x00CF1170`; refresh `0x00CCCF60`: the dirty beliefs it depends on
//! first, then its own update, virtual `+0x24`), and so does the group lookup `0x00A753D0` for a
//! dirty group; marking a belief dirty (`0x00CF0AB0`) marks every belief that depends on it
//! (`0x00CF0B10`); the entry's update `0x00AB6ED0` makes the entry depend (`0x00CB22F0`) on the
//! region's own group and on each neighbour group, and the group's constructor `0x00A31680` makes
//! the group depend on each member's CAI region node (CAI region `+8`); the group's update
//! `0x00AB6CE0` compares each member's current owner with the group's and, when one differs, marks
//! the groups' registry belief dirty and refreshes it. INFERRED: a change of a region's owner
//! marks that CAI region's node dirty at once, so that an evaluation later in the same turn reads
//! the new groups as ours does. The call that marks the node was not found (the owner change
//! `0x00B449F0` → `0x00A64AC0` was not traced to it); until it is, a deal valued in the turn of a
//! capture or handover may read the old state in the exe (side-by-side check pending).
//!
//! The links are the CAI world's areas' ([`super::World::ai_areas`], the boundary order); a map
//! without them uses [`super::World::region_neighbours`] (the director's [`super::director`] rule).

use std::collections::BTreeMap;

use super::ids::{FactionId, RegionId};
use super::region_value::GroupChange;
use super::world::CampaignModel;

/// The region groups of the current owners, built once per evaluation ([`CampaignModel::region_groups`]).
pub struct RegionGroups {
    /// Each game region's links to other game regions, in boundary order (seas and theatres
    /// dropped: they have no owner and another area type, so no test here ever passes on them).
    links: BTreeMap<RegionId, Vec<RegionId>>,
    /// Each game region's owner.
    owner: BTreeMap<RegionId, FactionId>,
    /// Each game region's group, an index into `members`.
    group: BTreeMap<RegionId, usize>,
    /// Each group's regions.
    members: Vec<Vec<RegionId>>,
}

impl CampaignModel {
    /// The region groups of the current owners ([`RegionGroups`]). Cold path: one walk of the
    /// regions and their links, built once per deal evaluation.
    pub fn region_groups(&self) -> RegionGroups {
        let w = &self.world;
        let areas = super::director::areas(self);
        let mut links: BTreeMap<RegionId, Vec<RegionId>> = BTreeMap::new();
        for a in &areas {
            if let Some(r) = a.region {
                links.insert(r.id, a.links.iter().filter_map(|&i| areas.get(i).and_then(|n| n.region)).map(|n| n.id).collect());
            }
        }
        let owner: BTreeMap<RegionId, FactionId> = w.regions.values().map(|r| (r.id, r.owner)).collect();
        // `0x00C3D6E0`. An empty map is a campaign with no land-mass data (every region on one); a
        // filled one has an entry per game region, and a region with an empty or no entry lies on
        // no land mass and joins no neighbour (the map attach warns of each such gap).
        let same_land = |a: RegionId, b: RegionId| {
            if w.region_land_masses.is_empty() {
                return true;
            }
            let (Some(x), Some(y)) = (w.region_land_masses.get(&a), w.region_land_masses.get(&b)) else { return false };
            x.iter().any(|i| y.contains(i))
        };
        let mut group: BTreeMap<RegionId, usize> = BTreeMap::new();
        let mut members: Vec<Vec<RegionId>> = Vec::new();
        // One flood stack for every group (emptied by each flood).
        let mut stack: Vec<RegionId> = Vec::new();
        for (&seed, &f) in &owner {
            if group.contains_key(&seed) {
                continue;
            }
            let g = members.len();
            group.insert(seed, g);
            let mut found = vec![seed];
            // A rebel region is a group of its own: no flood.
            if !w.is_rebel_faction(f) {
                stack.push(seed);
            }
            while let Some(x) = stack.pop() {
                for &n in links.get(&x).map_or(&[][..], Vec::as_slice) {
                    if owner.get(&n) == Some(&f) && !group.contains_key(&n) && same_land(x, n) {
                        group.insert(n, g);
                        found.push(n);
                        stack.push(n);
                    }
                }
            }
            members.push(found);
        }
        RegionGroups { links, owner, group, members }
    }
}

impl RegionGroups {
    /// The change state of `r`'s group for `faction` (`0x00A634D0` on the belief `0x00AB6ED0`
    /// builds; see the module notes). NEW for an unknown region.
    pub fn change(&self, r: RegionId, faction: FactionId) -> GroupChange {
        let (Some(&g), Some(&owner)) = (self.group.get(&r), self.owner.get(&r)) else { return GroupChange::New };
        if faction == owner {
            return match self.members[g].len() {
                1 => GroupChange::Lost,
                2 => GroupChange::Reduced,
                _ if self.split(r) => GroupChange::Split,
                _ => GroupChange::Reduced,
            };
        }
        // The first of the faction's neighbour groups; a second distinct one makes it MERGED.
        let mut first = None;
        for n in self.links(r) {
            let Some(&h) = self.group.get(n).filter(|&&h| h != g && self.owner.get(n) == Some(&faction)) else { continue };
            match first {
                None => first = Some(h),
                Some(f) if f != h => return GroupChange::Merged,
                Some(_) => {}
            }
        }
        if first.is_some() { GroupChange::Increased } else { GroupChange::New }
    }

    fn links(&self, r: RegionId) -> &[RegionId] {
        self.links.get(&r).map_or(&[][..], Vec::as_slice)
    }

    /// `0x00A73210(r, group − r)`: from the first of `r`'s links its owner holds, `0x00AA3DC0`
    /// removes each region of the rest of the group it reaches over links between regions of one
    /// owner (only a removed region spreads further; it stops once the rest is empty). SPLIT when
    /// some of it is left. The start may lie outside the group (another land mass of the same
    /// owner): then nothing is removed and the group reads SPLIT, as in the exe.
    fn split(&self, r: RegionId) -> bool {
        let g = self.group[&r];
        let owner = self.owner[&r];
        let mut rest: Vec<RegionId> = self.members[g].iter().copied().filter(|&x| x != r).collect();
        let Some(&start) = self.links(r).iter().find(|n| self.owner.get(n) == Some(&owner)) else { return !rest.is_empty() };
        let mut stack = vec![start];
        while let Some(x) = stack.pop() {
            let Some(k) = rest.iter().position(|&y| y == x) else { continue };
            rest.swap_remove(k);
            if rest.is_empty() {
                break;
            }
            stack.extend(self.links(x).iter().copied().filter(|n| self.owner.get(n) == Some(&owner)));
        }
        !rest.is_empty()
    }
}

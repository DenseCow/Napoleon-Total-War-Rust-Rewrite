//! Recruiting through a commander: the recruitment tab of a general's army and the naval
//! recruitment tab of an admiral's navy (`analysis/fidelity/UI_FIDELITY.md` §4.10, CONFIRMED unless
//! tagged).
//!
//! The panel's info builder `0x009FE7B0` takes this path when its tab was made for a character
//! (`0x0098BEA0`, built by the army tab set `0x009855B0` and the navy tab set `0x009990B0`). Its options
//! are `0x00B72DF0`:
//! - every region of the faction in the theatre the commander stands in (`0x00B624C0` over the faction's
//!   region list; `0x00B1BC90` finds the theatre by the commander's position) gives its queue's priced
//!   recruitable list (the land queue's for an army, each port's for a navy; the list the settlement's
//!   own panel shows, [`CampaignModel::recruitable_units`] priced and flagged as the queue command does);
//! - the lists are merged by unit (`0x00B09DB0`, ordered by unit key, then flags, experience, cost and
//!   queue), and each unit keeps one source (`0x00B113A0` → `0x00B41F60`): the first; when it is flagged,
//!   the unit's flags are those of all its sources together; else the source with the shortest
//!   training plus travel time (see [`CampaignModel::commander_recruitment`]);
//! - all options are flagged queue-full ([`ENTRY_QUEUE_FULL`]) once ten items are queued for the commander
//!   (`0x00B1BBB0`), and the list is sorted (`0x00B70A00`).
//!
//! The commander's queue is every item of those queues whose target is he (`0x00B26020`); the panel
//! shows the first ten.
//!
//! The model has one theatre per map (as [`super::economy`] and [`super::population`] assume: every
//! shipped campaign has one), so the theatre is the faction's whole map.

use std::collections::HashMap;

use super::commands::{ENTRY_QUEUE_FULL, MAX_QUEUE};
use super::effects::Effects;
use super::ids::{CharacterId, FactionId, RecruitmentItemId, RegionId};
use super::polypath::{byte_cost, dir_index, Mover, PolyMap, PolyPath, View};
use super::rules::UnitRules;
use super::world::{CampaignModel, Region};
use crate::fixed::Fixed20;

/// Recruitable entry flag set by the commander's panel (`0x00B41F60`, CONFIRMED): no path from the
/// source settlement to the commander. The recruitment card's ninth reason ("path").
pub const ENTRY_UNREACHABLE: u32 = 0x100;

/// One option of a commander's recruitment panel: a unit, the region whose queue would train it and
/// the numbers the card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct CommanderOption {
    /// The unit (`units` key).
    pub unit_key: String,
    /// The region whose queue trains it (entry[7]): the card's `manager`, the queue the item goes into.
    pub region: RegionId,
    /// What the item costs (entry[0], [`super::economy::recruitment_cost`] in that region).
    pub cost: i32,
    /// Why the unit cannot be recruited ([`super::commands::ENTRY_QUEUE_FULL`] and the other entry flags,
    /// plus [`ENTRY_UNREACHABLE`]); 0 = recruitable.
    pub flags: u32,
    /// Turns until the unit is trained in that queue (`0x00B61D80`: the wait for a free training place
    /// plus `units` #6), -1 when the queue trains nothing (no recruitment points) or is full.
    pub training_turns: i32,
    /// Turns the unit then marches to the commander (entry[3]: the path cost / `units` #9); 0 for a
    /// flagged unit, whose path is never measured.
    pub travel_turns: f32,
}

impl CommanderOption {
    /// The card's status: "Available" when unflagged (`0x009FEA10`: flags 0 and travel not below 0).
    pub fn available(&self) -> bool {
        self.flags == 0 && self.travel_turns >= 0.0
    }

    /// The march as the card shows it, rounded up (`0x00B5AC60`: FIST to nearest, plus 1 when the rest,
    /// as a float's bits, is above 0).
    pub fn travel_turns_rounded(&self) -> i32 {
        let r = super::commands::fistp(self.travel_turns);
        if (self.travel_turns - r as f32).to_bits() as i32 > 0 { r.wrapping_add(1) } else { r }
    }
}

/// A commander's recruitment panel ([`CampaignModel::commander_recruitment`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommanderRecruitment {
    /// The options, in the panel's order.
    pub options: Vec<CommanderOption>,
    /// The items queued for him, in the order the panel lists them: the faction's regions in turn,
    /// each queue in its own order (all of them; the panel shows the first [`MAX_QUEUE`]).
    pub queue: Vec<(RegionId, RecruitmentItemId)>,
}

/// One source of a unit while the options are built: a priced entry of one region's queue plus what
/// the panel works out for it.
#[derive(Debug, Clone)]
struct Source<'a> {
    unit_key: &'a str,
    unit: &'a UnitRules,
    region: RegionId,
    cost: i32,
    flags: u32,
    training: i32,
    /// entry[2]: the path cost from the source to the commander, -1 = none (or not measured).
    travel: f32,
    /// entry[3]: `travel` / `units` #9.
    travel_turns: f32,
}

impl Source<'_> {
    /// `0x00B61D50`: training plus travel turns, -1 without a training estimate.
    ///
    /// ORIGINAL BUG: the exe's comparison (`0x00B68FE0`: the candidate wins when the best's total is
    /// above its own) takes that -1 as the shortest time, so a source whose queue cannot train (no
    /// recruitment points: `0x00B72FC0` returns -1) is chosen over every source that can, and is never
    /// replaced once it is the first; its item then never trains. Ours ranks a source without an
    /// estimate after every source with one, and, while the best is such a source, searches the next
    /// sources' paths without the cut at its cost (else a nearer untrainable first source would still
    /// win by refusing every farther one).
    fn total(&self) -> f32 {
        if self.training < 0 { f32::INFINITY } else { self.training as f32 + self.travel_turns }
    }
}

/// The queue a source's item goes into: the region's land queue, or its port queue for a ship (the
/// entry's queue, entry[7], is chosen by the unit's kind: `0x00B43CA0`). The key of the path cache.
type QueueKey = (RegionId, bool);

impl CampaignModel {
    /// The recruitment panel of `commander`'s army or navy as `faction` sees it (the panel's faction is
    /// the tab set's, the local player's: tab set +0x1C, INFERRED to be the player's faction).
    ///
    /// Per unit, the sources are taken in the merge order (flags, experience, cost, region; experience is
    /// not modelled, every entry has 0). The first is the unit's option. When it is flagged, the option
    /// carries the flags of every source of the unit. Else its path to the commander is measured
    /// ([`Self::recruit_travel_cost`]; none sets [`ENTRY_UNREACHABLE`]) and each further source replaces it
    /// when the source is unflagged, has a path no longer than the option's (the search is cut at that
    /// cost, unless the option cannot train; a path measured for an earlier unit is reused whatever its
    /// length, as the exe's cache does) and needs fewer training plus travel turns.
    pub fn commander_recruitment(&self, faction: FactionId, commander: CharacterId) -> CommanderRecruitment {
        let Some(ch) = self.world.characters.get(&commander) else { return CommanderRecruitment::default() };
        let naval = self.force_of(commander).and_then(|f| self.world.forces.get(&f)).is_some_and(|f| f.is_navy);
        // The commander's place: the settlement he is in (`0x009FB9D0`, its building's position), else his own.
        let goal = ch.garrisoned_in.and_then(|r| self.world.regions.get(&r)).map_or(ch.position, |r| r.settlement.position);
        let regions: Vec<&Region> = self.world.regions.values().filter(|r| r.owner == faction).filter(|r| !naval || r.slots.iter().any(|s| s.port)).collect();
        let queue: Vec<(RegionId, RecruitmentItemId)> = regions
            .iter()
            .flat_map(|r| r.recruitment_queue.iter().filter(|i| i.target == Some(commander) && self.unit_is_naval(&i.unit_key) == naval).map(|i| (r.id, i.id)))
            .collect();

        // The priced sources of every region, grouped by unit in the merge order.
        let fx = Effects::compute_for(self, faction);
        let counts = self.unit_type_counts(faction);
        let mut sources: Vec<Source<'_>> = Vec::new();
        for r in &regions {
            let set = super::economy::region_effect_set(self, r);
            let capacity = [self.recruitment_points_with(&fx, r.id, false), self.recruitment_points_with(&fx, r.id, true)];
            for e in self.recruitable_units(r.id) {
                let Some((unit_key, unit)) = self.rules.units.get_key_value(&e.unit_key) else { continue };
                if naval && !unit.is_naval {
                    continue;
                }
                let cost = super::economy::recruitment_cost_in(&self.rules, &set, unit_key, unit);
                let flags = self.recruitable_entry_flags(r, &e, unit, cost, &counts);
                let training = queue_training_turns(self, r, unit, capacity[usize::from(unit.is_naval)]);
                sources.push(Source { unit_key, unit, region: r.id, cost, flags, training, travel: -1.0, travel_turns: 0.0 });
            }
        }
        sources.sort_by(|a, b| a.unit_key.cmp(b.unit_key).then(a.flags.cmp(&b.flags)).then(a.cost.cmp(&b.cost)).then(a.region.cmp(&b.region)));

        // The paths measured so far, per queue: (cost, found) or (the limit the search failed at, false).
        let mut paths: HashMap<QueueKey, (f32, bool)> = HashMap::new();
        let mut travel = |s: &Source<'_>, limit: f32| -> f32 {
            let limit = if limit < 0.0 { f32::MAX } else { limit };
            let key = (s.region, s.unit.is_naval);
            let entry = paths.entry(key).or_insert((-1.0, false));
            if entry.1 {
                return entry.0;
            }
            if limit <= entry.0 {
                return -1.0;
            }
            let from = self.queue_position(s.region, s.unit.is_naval);
            let cost = self.recruit_travel_cost(from, goal, naval, limit);
            *entry = if cost < 0.0 { (limit, false) } else { (cost, true) };
            cost
        };
        let mut options = Vec::new();
        let mut i = 0;
        while i < sources.len() {
            let end = i + sources[i..].iter().take_while(|s| s.unit_key == sources[i].unit_key).count();
            let mut best = sources[i].clone();
            if best.flags != 0 {
                best.flags = sources[i..end].iter().fold(0, |f, s| f | s.flags);
            } else {
                best.travel = travel(&best, f32::MAX);
                if best.travel < 0.0 {
                    best.flags = ENTRY_UNREACHABLE;
                } else {
                    best.travel_turns = march_turns(best.travel, best.unit);
                }
                for s in &sources[i + 1..end] {
                    let mut cand = s.clone();
                    // The exe cuts the search at the best's path cost. A best that cannot train (the
                    // ORIGINAL BUG at `Source::total`) loses to any trainable source, so then the search is
                    // not cut: a farther source must not be refused for its distance.
                    cand.travel = travel(&cand, if best.training < 0 { f32::MAX } else { best.travel });
                    if cand.travel >= 0.0 {
                        cand.travel_turns = march_turns(cand.travel, cand.unit);
                    }
                    // `0x00B68FE0`: an unflagged candidate with a path wins over a best without one, else when
                    // its total is the smaller.
                    if cand.flags == 0 && cand.travel >= 0.0 && (best.travel < 0.0 || best.total() > cand.total()) {
                        best = cand;
                    }
                }
            }
            options.push(best);
            i = end;
        }
        // Ten items queued for him: every option is refused as queue-full (`0x00B1BBB0`, more than 9).
        if queue.len() >= MAX_QUEUE as usize {
            for o in &mut options {
                o.flags |= ENTRY_QUEUE_FULL;
            }
        }
        // `0x00B70A00`: the unit category (cavalry, artillery, infantry, ...), then flags, then the dearer,
        // then the more experienced (not modelled), then the shorter march. The exe's sort is an insertion
        // sort up to 32 options and an introsort above; ours is stable (equal keys keep the merge order).
        options.sort_by(|a, b| {
            super::characters::category_order(&a.unit.category)
                .cmp(&super::characters::category_order(&b.unit.category))
                .then(a.flags.cmp(&b.flags))
                .then(b.cost.cmp(&a.cost))
                .then(a.travel_turns.partial_cmp(&b.travel_turns).unwrap_or(std::cmp::Ordering::Equal))
        });
        CommanderRecruitment {
            options: options
                .into_iter()
                .map(|s| CommanderOption { unit_key: s.unit_key.to_owned(), region: s.region, cost: s.cost, flags: s.flags, training_turns: s.training, travel_turns: s.travel_turns })
                .collect(),
            queue,
        }
    }

    fn unit_is_naval(&self, unit_key: &str) -> bool {
        self.rules.units.get(unit_key).is_some_and(|u| u.is_naval)
    }

    /// Where a region's queue of that kind stands (its vtable +8): the settlement's position for the land
    /// queue (`0x00B62010`: region +0xFC, its virtual +0x3C), the port's for a naval one (`0x00B61FF0`: the
    /// port slot +0x5C, its virtual +0x40; the model's first port slot, as the spawn uses).
    fn queue_position(&self, region: RegionId, naval: bool) -> (Fixed20, Fixed20) {
        let Some(r) = self.world.regions.get(&region) else { return Default::default() };
        if naval {
            r.slots.iter().find(|s| s.port).and_then(|s| s.position).unwrap_or(r.settlement.position)
        } else {
            r.settlement.position
        }
    }

    /// The path cost from a recruit's source to its commander, -1 without a path no dearer than `limit`
    /// (`0x00B59340` → `0x00B0F180` / `0x00B0F1E0` → `0x00B0F2B0`). The search is the map's own
    /// ([`View::find_path_avoiding`], an army's or a fleet's), from the source inside its building to the
    /// commander; the cost is then measured along the path's cells ([`measure_path`]).
    ///
    /// INFERRED: the cut at `limit` compares the search's own cost (the exe hands the limit to its search
    /// `0x00AC54C0`, whose test is not traced); the search runs on the static map without the zones of
    /// control (which run-time cuts the exe's query sees is not traced). Without the map's polygons the
    /// cost is the grid path's, without a map the straight line's, both at the off-road cost
    /// (PROVISIONAL, as [`CampaignModel::plan_path`]).
    ///
    /// ORIGINAL BUG: `0x00B0F2B0` gives no path when the two points are the same (its start/goal test),
    /// so a general inside a settlement could not recruit that settlement's own units through his panel
    /// (flagged as having no path) although they need no march. Ours: cost 0.
    pub fn recruit_travel_cost(&self, from: (Fixed20, Fixed20), to: (Fixed20, Fixed20), naval: bool, limit: f32) -> f32 {
        if from == to {
            return 0.0;
        }
        let (a, b) = ((from.0.to_f32(), from.1.to_f32()), (to.0.to_f32(), to.1.to_f32()));
        let Some(t) = &self.terrain else {
            let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() * self.rules.road_cost(0);
            return if d > limit { -1.0 } else { d };
        };
        let grid = &t.0;
        let road: Vec<f32> = grid.region_keys.iter().map(|k| self.rules.road_cost(self.road_level(k))).collect();
        let Some(pm) = &grid.poly else {
            let min = road.iter().copied().fold(super::rules::OFF_ROAD_COST, f32::min);
            let domain = if naval { super::pathing::Domain::Sea } else { super::pathing::Domain::Land };
            let cost = grid.find_path(a, b, domain, min, |i| if grid.road[i] { road.get(grid.region[i] as usize).copied().unwrap_or(super::rules::OFF_ROAD_COST) } else { super::rules::OFF_ROAD_COST }).and_then(|p| p.costs.last().copied());
            return cost.filter(|&c| c <= limit).unwrap_or(-1.0);
        };
        let by_id = pm.road_costs(|r| road.get(r).copied().unwrap_or(super::rules::OFF_ROAD_COST), self.rules.road_cost(0));
        let mover = if naval { Mover::Sea } else { Mover::Land };
        let view = pm.view();
        let open7 = super::movers::open_shared(pm, pm.locate(a.0, a.1, mover, 2), pm.locate(b.0, b.1, mover, 2));
        let family = super::movers::Family::of(true);
        let blocked = |q: usize| super::movers::kind7_closed(&view, family, &open7, q);
        let Some(path) = view.find_path_avoiding(a, b, mover, &by_id, &blocked) else { return -1.0 };
        if path.costs.last().is_some_and(|&c| c > limit) {
            return -1.0;
        }
        measure_path(pm, &view, &path, &by_id)
    }
}

/// entry[3]: the march in turns, the path cost over `units` #9 (`0x00B41F60`). The exe divides by #9
/// unguarded; a mod's #9 below 1 is read as 1 here (the data loader reports it once per load).
fn march_turns(cost: f32, unit: &UnitRules) -> f32 {
    cost / unit.travel_speed.max(1) as f32
}

/// `0x00B72FC0`: the turns until a unit queued now in `region`'s queue of its kind is trained: the
/// wait for one of the queue's `capacity` training places ([`queue_wait`]) plus `units` #6; -1 when
/// the queue trains nothing (`capacity` 0) or is full ([`MAX_QUEUE`] items).
fn queue_training_turns(m: &CampaignModel, region: &Region, unit: &UnitRules, capacity: u32) -> i32 {
    let turns: Vec<u32> = region.recruitment_queue.iter().filter(|i| m.unit_is_naval(&i.unit_key) == unit.is_naval).map(|i| i.turns_remaining).collect();
    if capacity == 0 || turns.len() >= MAX_QUEUE as usize {
        return -1;
    }
    queue_wait(capacity, &turns) + unit.turns as i32
}

/// `0x00B73030`: how many turns until one of a queue's `capacity` training places is free, from its
/// items' turns left in queue order: 0 below capacity; at capacity the shortest item; above, the
/// first `capacity` items count down one turn at a time and finished ones leave, until the queue is
/// down to capacity (then its shortest item is added) or below it.
pub fn queue_wait(capacity: u32, turns: &[u32]) -> i32 {
    let cap = capacity as usize;
    let mut v = turns.to_vec();
    if v.len() < cap {
        return 0;
    }
    let mut wait = 0;
    loop {
        if v.len() == cap {
            return wait + v.iter().copied().min().unwrap_or(0) as i32;
        }
        if v.len() < cap {
            return wait;
        }
        wait += 1;
        for t in v.iter_mut().take(cap) {
            *t = t.saturating_sub(1);
        }
        v.retain(|&t| t != 0);
    }
}

/// `0x00B0F2B0`'s measure of a found path (CONFIRMED): the path's nodes in the start's cell are skipped
/// and so are those in the goal's cell; the cost is the start to the first other node's point, plus the
/// last node outside the goal's cell to the goal, plus, between those two, the distance between the
/// centres of each two consecutive nodes in different cells; each piece times the step's multiplier
/// ([`View::multiplier`] of the step's first node in its direction, `0x00B204C0`). A path that never
/// leaves the start's cell costs the straight line times the start cell's header byte picked by the
/// angle of the start seen from the goal ([`sector`], `0x00B0F690`); one that goes straight from
/// the start's cell into the goal's, the straight line times that step's multiplier. Below 0.001 it is 0.
fn measure_path(pm: &PolyMap, view: &View<'_>, path: &PolyPath, road: &[f32]) -> f32 {
    let (pts, polys) = (&path.points, &path.polys);
    let n = pts.len().min(polys.len());
    if n < 2 {
        return 0.0;
    }
    let cell = |i: usize| view.poly_cell(polys[i] as usize);
    let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)).sqrt();
    let step = |a: usize, b: usize| {
        let ((ca, ra), (cb, rb)) = (view.cell_rc(polys[a] as usize), view.cell_rc(polys[b] as usize));
        view.multiplier(polys[a] as usize, dir_index(cb - ca, rb - ra), road)
    };
    let (start, goal) = (pts[0], pts[n - 1]);
    // The pieces are summed as the exe does: extended precision, stored as a float after each piece.
    let add = |acc: f32, m: f32, d: f32| (f64::from(m) * f64::from(d) + f64::from(acc)) as f32;
    let cost = match (1..n).find(|&i| cell(i) != cell(0)) {
        None => {
            let theta = (start.0 - goal.0).atan2(start.1 - goal.1);
            add(0.0, byte_cost(pm.header[cell(0) as usize][sector(theta)]), dist(start, goal))
        }
        Some(f) => {
            let last = n - 1;
            let mut g = n - 2;
            while g != f - 1 && cell(g) == cell(last) {
                g -= 1;
            }
            if g == f - 1 {
                add(0.0, step(0, last), dist(start, goal))
            } else {
                let mut acc = add(add(0.0, step(0, f), dist(start, pts[f])), step(g, last), dist(pts[g], goal));
                for i in f + 1..=g {
                    if cell(i) != cell(i - 1) {
                        acc = add(acc, step(i - 1, i), dist(pm.centre(cell(i - 1)), pm.centre(cell(i))));
                    }
                }
                acc
            }
        }
    };
    if cost < 0.001 { 0.0 } else { cost }
}

/// The header byte index for the angle `atan2(dx, dz)` of the start seen from the goal (`0x00B0F692`..
/// `0x00B0F762`, CONFIRMED: eight 45° sectors at ±π/8, ±3π/8, ±5π/8, ±7π/8, mapped as the exe does; note
/// that the x axis maps mirrored against [`dir_index`]: an angle of π/2, the start east of the goal, is 7).
fn sector(theta: f32) -> usize {
    // The exe's constants, bit for bit (0x0137E920..0x0137E948, 0x01331B24).
    let (a, b, c, d) = (f32::from_bits(0x3EC9_0FDB), f32::from_bits(0x3F96_CBE4), f32::from_bits(0x3FFB_53D2), f32::from_bits(0x402F_EDE0));
    if theta <= -d || theta > d {
        5
    } else if theta <= -c {
        6
    } else if theta <= -b {
        0
    } else if theta <= -a {
        3
    } else if theta <= a {
        2
    } else if theta <= b {
        1
    } else if theta <= c {
        7
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::polypath::{kind, CellInput, PolyInput};

    const U: i32 = 1 << 20;

    /// A made-up `w` x `h` map of 2-unit land cells, one square polygon per cell, header byte `byte`.
    fn map(w: u32, h: u32, byte: u8) -> PolyMap {
        let mut cells = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let (x0, z0) = (c as i32 * 2 * U, r as i32 * 2 * U);
                let sq = vec![(x0, z0), (x0 + 2 * U, z0), (x0 + 2 * U, z0 + 2 * U), (x0, z0 + 2 * U)];
                cells.push(CellInput { header: [byte; 8], polys: vec![PolyInput { kind: kind::LAND, region_id: 0, outline: sq }] });
            }
        }
        PolyMap::build((0, 0), 2 * U, w, h, &cells, vec![vec![0]])
    }

    /// `0x00B0F2B0`'s measure: along a straight row it is the search's own cost (start to the first cell
    /// centre, centre to centre, the last centre to the goal); inside one cell, the straight line at the
    /// cell's byte for the sector the start lies in, seen from the goal.
    #[test]
    fn the_march_is_measured_along_the_paths_cells() {
        let m = map(10, 2, 20);
        let view = m.view();
        let p = view.find_path((1.0, 1.0), (19.0, 1.0), Mover::Land, &[1.0]).unwrap();
        assert!((measure_path(&m, &view, &p, &[1.0]) - 18.0 * byte_cost(20)).abs() < 1e-3);
        let p = view.find_path((0.5, 0.5), (1.5, 1.5), Mover::Land, &[1.0]).unwrap();
        assert!((measure_path(&m, &view, &p, &[1.0]) - std::f32::consts::SQRT_2 * byte_cost(20)).abs() < 1e-4);
        // The sectors, as the exe maps them: angle 0 is 2, π/2 is 7, π is 5, -π/2 is 0.
        assert_eq!([sector(0.0), sector(std::f32::consts::FRAC_PI_2), sector(std::f32::consts::PI), sector(-std::f32::consts::FRAC_PI_2)], [2, 7, 5, 0]);
        assert_eq!(sector(std::f32::consts::FRAC_PI_4), 1);
        assert_eq!(sector(-std::f32::consts::FRAC_PI_4), 3);
    }
}

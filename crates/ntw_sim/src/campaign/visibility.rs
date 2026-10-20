//! What each faction can see on the campaign map (the shroud / fog of war), read from the exe
//! (slot 0-G). Spec: `analysis/fidelity/CHARACTERS_FIDELITY.md` §10. Specs in our words; no
//! decompiled code.
//!
//! - The map is cut into sight cells of 1.25 map units (`SightGrid`; world +0xF58: origin, extent,
//!   columns and rows; the counts are the saved quad trees' header, 1024 × 512 for the European map,
//!   512 × 512 for the Peninsular one; the origin is the centre minus half the extent).
//! - A sight source sees a disc of cells (`0x00B696C0`, CONFIRMED against every settlement's saved
//!   `LINE_OF_SIGHT`): characters with their type's radius (`agents` #2, × (1 + their
//!   `line_of_sight_extension` / 100), `0x009CDF10`), settlements and slots with 5 (`0x00B40C00`),
//!   and an owned region its whole saved shape.
//! - A faction with a shroud (`FACTION` `CAMPAIGN_SHROUD`; faction +0x6F8; the playable factions in
//!   the vanilla saves) has three cell sets: explored, visible, and the black shroud (`Shroud::hidden`;
//!   empty in every save but the tutorial's). A point is visible when its cell is visible and not in the third set
//!   (`0x00B7A150`); a faction without a shroud sees everything (its AI has its own beliefs).

use std::collections::BTreeMap;

use super::ids::{CharacterId, FactionId, RegionId};
use super::world::{CampaignModel, CharacterKind};

/// The sight cell grid (world +0xF58).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SightGrid {
    /// Columns (x).
    pub cols: u32,
    /// Rows (z).
    pub rows: u32,
    /// The quad tree's root size (the larger of the two, a power of two).
    pub root: u32,
    /// The map position of cell (0, 0)'s corner.
    pub origin: (f32, f32),
    /// Cell size in map units (1.25, INFERRED from the saved boxes; the exe divides the extent by
    /// the counts).
    pub cell: f32,
}

impl SightGrid {
    /// A grid of `cols` × `rows` cells of 1.25 centred on the map origin.
    pub fn centred(cols: u32, rows: u32, root: u32) -> Self {
        let cell = 1.25;
        SightGrid { cols, rows, root, origin: (-(cols as f32) * cell / 2.0, -(rows as f32) * cell / 2.0), cell }
    }

    /// The cell holding a map position (`0x00B5B5A0`).
    pub fn cell_of(&self, p: (f32, f32)) -> Option<(u32, u32)> {
        let x = ((p.0 - self.origin.0) / self.cell).floor();
        let z = ((p.1 - self.origin.1) / self.cell).floor();
        (x >= 0.0 && z >= 0.0 && (x as u32) < self.cols && (z as u32) < self.rows).then_some((x as u32, z as u32))
    }

    /// The cells a source at `p` with radius `r` sees (`0x00B696C0`, CONFIRMED): the cell box
    /// x0..=x1 (x0 = floor((p.x − origin − r) / cell), x1 the same with + r), likewise in z; with
    /// w = x1 − x0 + 1 and h = z1 − z0 + 1 the cell at column c, row r of the box is seen when
    /// (r − h / 2)² + (c − w / 2)² ≤ w² / 4 (integer divisions).
    pub fn disc(&self, p: (f32, f32), radius: f32) -> Vec<(u32, u32)> {
        // As the exe evaluates it: ((p − origin) ∓ r) × count / extent, in f32.
        let (ex, ez) = (self.cols as f32 * self.cell, self.rows as f32 * self.cell);
        let (cx, cz) = (self.cols as f32, self.rows as f32);
        let x0 = (((p.0 - self.origin.0) - radius) * cx / ex).floor() as i64;
        let x1 = (((p.0 - self.origin.0) + radius) * cx / ex).floor() as i64;
        let z0 = (((p.1 - self.origin.1) - radius) * cz / ez).floor() as i64;
        let z1 = (((p.1 - self.origin.1) + radius) * cz / ez).floor() as i64;
        let (w, h) = (x1 - x0 + 1, z1 - z0 + 1);
        let lim = (w * w) / 4;
        let mut out = Vec::new();
        for c in 0..w {
            for r in 0..h {
                let (dc, dr) = (c - w / 2, r - h / 2);
                if dr * dr + dc * dc <= lim {
                    let (x, z) = (x0 + c, z0 + r);
                    if x >= 0 && z >= 0 && (x as u32) < self.cols && (z as u32) < self.rows {
                        out.push((x as u32, z as u32));
                    }
                }
            }
        }
        out
    }
}

/// A set of sight cells.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CellSet {
    /// Columns.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    bits: Vec<u64>,
}

impl CellSet {
    /// An empty set over `cols` × `rows`.
    pub fn new(cols: u32, rows: u32) -> Self {
        CellSet { cols, rows, bits: vec![0; (cols as usize * rows as usize).div_ceil(64)] }
    }
    fn index(&self, x: u32, z: u32) -> Option<usize> {
        (x < self.cols && z < self.rows).then(|| z as usize * self.cols as usize + x as usize)
    }
    /// Every cell of `cols` × `rows` (the quad tree's build with fill 1, `0x00DF3190`).
    pub fn full(cols: u32, rows: u32) -> Self {
        let n = cols as usize * rows as usize;
        let mut bits = vec![u64::MAX; n.div_ceil(64)];
        if let Some(last) = bits.last_mut().filter(|_| !n.is_multiple_of(64)) {
            *last = (1u64 << (n % 64)) - 1;
        }
        CellSet { cols, rows, bits }
    }
    /// Whether the cell is in the set.
    pub fn get(&self, x: u32, z: u32) -> bool {
        self.index(x, z).is_some_and(|i| self.bits[i / 64] >> (i % 64) & 1 == 1)
    }
    /// Adds a cell.
    pub fn set(&mut self, x: u32, z: u32) {
        if let Some(i) = self.index(x, z) {
            self.bits[i / 64] |= 1 << (i % 64);
        }
    }
    /// Removes a cell.
    pub fn unset(&mut self, x: u32, z: u32) {
        if let Some(i) = self.index(x, z) {
            self.bits[i / 64] &= !(1 << (i % 64));
        }
    }
    /// Removes every cell.
    pub fn clear(&mut self) {
        self.bits.iter_mut().for_each(|b| *b = 0);
    }
    /// Adds every cell of `other`.
    pub fn union_with(&mut self, other: &CellSet) {
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a |= *b;
        }
    }
    /// Number of cells.
    pub fn len(&self) -> usize {
        self.bits.iter().map(|b| b.count_ones() as usize).sum()
    }
    /// Whether no cell is set.
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|b| *b == 0)
    }
    /// Every cell, row by row.
    pub fn cells(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (0..self.rows).flat_map(move |z| (0..self.cols).map(move |x| (x, z))).filter(|&(x, z)| self.get(x, z))
    }
}

/// A faction's shroud (`CAMPAIGN_SHROUD` v1: three quad trees and a bool, CONFIRMED layout; read by
/// `0x00AE89A0`, written by `0x00AFBFC0` from the `FACTION` writer `0x00892A80`; object +4 / +0x1C
/// / +0x34 and +0x50; a new campaign's is built by `0x00AE8910`, [`CampaignModel::start_shrouds`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Shroud {
    /// #0: every cell ever seen (a superset of `visible` in every save).
    pub explored: CellSet,
    /// #1: the cells seen now.
    pub visible: CellSet,
    /// #2: the black shroud, cells that hide what is in them even when seen (`0x00B7A150` tests
    /// visible and not this). Empty in every vanilla save and campaign start position but the
    /// tutorial's, which fills all but 270 of its 524,288 cells (CONFIRMED by loading
    /// `campaigns/tut_napoleon/startpos.esf`); `unveil_black_shroud` lifts it outline by outline
    /// ([`CampaignModel::unveil_black_shroud`]). A shroud a new campaign creates
    /// starts with every cell here when the map has black shroud outlines (the ctor `0x00AE8910`,
    /// CONFIRMED; [`CampaignModel::start_shrouds`]).
    pub hidden: CellSet,
    /// #3: the shroud is on (true in every save); off, everything is visible.
    pub active: bool,
}

/// One black shroud outline turned into sight cells (`0x00B65C50`, the steps are listed at
/// [`CampaignModel::unveil_black_shroud`]).
struct BlackShroudLift {
    /// The cell box's low corner (it may reach off the grid, where no cell is touched; the exe's
    /// tree walks clip the same way, `0x00E29F90` / `0x00E2A1B0`).
    x0: i64,
    z0: i64,
    /// The cell box's size.
    w: i64,
    h: i64,
    /// Row by row over the box (index row × w + column, as the exe's byte mask): whether the cell
    /// leaves the black shroud.
    mask: Vec<bool>,
}

impl BlackShroudLift {
    fn of(grid: &SightGrid, outline: &[(f32, f32)]) -> Option<Self> {
        let &first = outline.first()?;
        if outline.len() < 3 {
            return None;
        }
        let (mut lo, mut hi) = (first, first);
        for &(x, z) in outline {
            lo = (lo.0.min(x), lo.1.min(z));
            hi = (hi.0.max(x), hi.1.max(z));
        }
        let (cx, cz) = (grid.cols as f32, grid.rows as f32);
        let (ex, ez) = (cx * grid.cell, cz * grid.cell);
        let (ox, oz) = grid.origin;
        // `0x00B42670`, in f32 as the exe: a corner's cell, and a cell edge's map position.
        // ORIGINAL BUG: the exe floors the corners to signed cells (start cells and counts stay
        // signed, the tree walks clip a box reaching off the grid) but turns a cell back into its
        // edge position through an unsigned conversion (`0x00B42670`: CVTDQ2PD then + 2^32 for a
        // negative value, table `0x01318130`), so a box reaching below cell 0 gets a low edge some
        // 2^32 cells away: every test point of that axis lands millions of units off the map and
        // the outline lifts nothing, while step 2 still forgets the box's explored cells. The high
        // edge uses the same formula and is right, so the intent is the box's own edge; ours
        // converts signed. Vanilla never reaches it (the tutorial's outlines lie far inside its
        // grid); a mod outline at the map's low x or z edge would.
        let cell = |v: f32, o: f32, n: f32, e: f32| ((v - o) * n / e).floor() as i64;
        let edge = |i: i64, o: f32, n: f32, e: f32| i as f32 * e * (1.0 / n) + o;
        // The widening the exe applies (constants 2.0 at `0x01325D14`, 4.0 at `0x01325D20`): the
        // high z gets none.
        let (x0, x1) = (cell(lo.0 - 2.0, ox, cx, ex), cell(hi.0 + 4.0, ox, cx, ex));
        let (z0, z1) = (cell(lo.1 - 2.0, oz, cz, ez), cell(hi.1, oz, cz, ez));
        let (w, h) = (x1 - x0 + 1, z1 - z0 + 1);
        let (sx0, sz0) = (edge(x0, ox, cx, ex), edge(z0, oz, cz, ez));
        let cw = (edge(x1 + 1, ox, cx, ex) - sx0) / w as f32;
        let ch = (edge(z1 + 1, oz, cz, ez) - sz0) / h as f32;
        // 0.707 at `0x0137E910`, 0.5 at `0x01318038`.
        let reach = ch * 0.707 + 0.5;
        let mut mask = vec![false; usize::try_from(w * h).ok()?];
        for c in 0..w {
            let px = c as f32 * cw + sx0;
            for r in 0..h {
                let p = (px, r as f32 * ch + sz0);
                mask[(r * w + c) as usize] = outline_contains(outline, p) || outline_distance(outline, p) < reach;
            }
        }
        Some(BlackShroudLift { x0, z0, w, h, mask })
    }

    /// Forgets the explored cells of the box still under the black shroud, then takes the mask's
    /// cells out of it (each step touches only its own cell, so one pass does both in order).
    fn apply(&self, s: &mut Shroud) {
        for r in 0..self.h {
            for c in 0..self.w {
                let (Ok(x), Ok(z)) = (u32::try_from(self.x0 + c), u32::try_from(self.z0 + r)) else { continue };
                if s.hidden.get(x, z) {
                    s.explored.unset(x, z);
                }
                if self.mask[(r * self.w + c) as usize] {
                    s.hidden.unset(x, z);
                }
            }
        }
    }
}

/// Whether a point is inside a closed outline (`0x010E7320`): even-odd crossings, an edge counting
/// when the point's z is in [low z, high z) and the point lies left of it.
fn outline_contains(outline: &[(f32, f32)], p: (f32, f32)) -> bool {
    let mut inside = false;
    for (i, &a) in outline.iter().enumerate() {
        let b = outline[(i + 1) % outline.len()];
        let (lo, hi) = if a.1 <= b.1 { (a, b) } else { (b, a) };
        if lo.1 <= p.1 && p.1 < hi.1 && (hi.0 - lo.0) * (p.1 - lo.1) > (hi.1 - lo.1) * (p.0 - lo.0) {
            inside = !inside;
        }
    }
    inside
}

/// A point's distance to a closed outline's edges (`0x00B287A0`: the nearest point of each edge,
/// the last point to the first included).
fn outline_distance(outline: &[(f32, f32)], p: (f32, f32)) -> f32 {
    let Some(&last) = outline.last() else { return f32::MAX };
    let mut best = f32::MAX;
    let mut a = last;
    for &b in outline {
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dz * dz).sqrt();
        let t = if len > 0.0 {
            let inv = 1.0 / len;
            let t = (p.1 - a.1) * inv * dz * inv + inv * (p.0 - a.0) * dx * inv;
            if t >= 1.0 { 1.0 } else if t >= 0.0 { t } else { 0.0 }
        } else {
            0.0
        };
        let (qx, qz) = (dx * t + a.0 - p.0, dz * t + a.1 - p.1);
        best = best.min(qx * qx + qz * qz);
        a = b;
    }
    best.sqrt()
}

impl CampaignModel {
    /// The sight radius of a character (character +0x2EC, saved as `CHARACTER` #17): the stored value
    /// ([`World::sight_radius`](super::world::World::sight_radius)), else his type's `agents` #2
    /// (record +0x10; General 15, colonel 10, admiral 20, captain 15, rake 18, gentleman 10,
    /// minister 0, …, CONFIRMED: every saved #17 of the vanilla saves is its type's value but one).
    pub fn sight_radius(&self, c: super::ids::CharacterId) -> f32 {
        if let Some(r) = self.world.sight_radius.get(&c) {
            return *r;
        }
        let Some(ch) = self.world.characters.get(&c) else { return 0.0 };
        self.rules.agent_sight.get(ch.kind.esf_name()).copied().unwrap_or(0) as f32
    }

    /// Re-sums a character's sight radius after his own effect set changed (`0x009CDF10` →
    /// `0x009CBC80`, CONFIRMED; run after a trait or ancillary change and at creation): his type's
    /// radius × (1 + his `line_of_sight_extension` / 100). The original skips it while a file loads
    /// (world +0xFC6, cleared at the end of `0x008742C0`), so a start position's characters keep
    /// their type's radius until their set changes: French generals with Grand Armee (+50) still
    /// see 15 in a turn-4 save, and one with 22.5 appears in a later save (CONFIRMED by #17).
    pub fn update_sight_radius(&mut self, c: super::ids::CharacterId) {
        let Some(ch) = self.world.characters.get(&c) else { return };
        let base = self.rules.agent_sight.get(ch.kind.esf_name()).copied().unwrap_or(0) as f32;
        let ext = super::effects::Effects::character_effects(self, c).get("line_of_sight_extension");
        self.world.sight_radius.insert(c, base * (ext * 0.01 + 1.0));
    }

    /// The factions whose sight counts for `faction` (`0x00B85040` / `0x00B85090`, CONFIRMED): itself
    /// and its protectorates (a protectorate whose patron it is). PROVISIONAL: the revolution case (the
    /// faction in a revolution on the revolutionaries' side, faction +0x81C) is not modelled.
    fn sight_factions(&self, faction: FactionId) -> Vec<FactionId> {
        let mut out = vec![faction];
        for (f, x) in &self.world.factions {
            if *f != faction && x.diplomacy.get(&faction) == Some(&super::world::Stance::Protectorate) {
                out.push(*f);
            }
        }
        out
    }

    /// The cells `faction` sees now (the union `0x00B617E0` builds from `0x00BB2A00`'s sources,
    /// CONFIRMED structure): its and its protectorates' characters on the map (each his disc), their
    /// settlements and the slots they hold (radius 5), and the saved shapes of the regions they own.
    /// PROVISIONAL: trade route segments (`TRADE_SEGMENTS` `LINE_OF_SIGHT`) and the other queried
    /// kinds (`0x00BB1D00`, `0x00BC6F00`, `0x00BCA310`) are not added; a garrisoned character sees
    /// from his settlement with his own radius.
    pub fn compute_visible(&self, faction: FactionId) -> Option<CellSet> {
        let grid = self.world.sight_grid?;
        let mut set = CellSet::new(grid.cols, grid.rows);
        let who = self.sight_factions(faction);
        for c in self.world.characters.values().filter(|c| who.contains(&c.faction)) {
            if c.kind == CharacterKind::Minister || (c.position.0.raw() == 0 && c.position.1.raw() == 0) {
                continue;
            }
            let r = self.sight_radius(c.id);
            if r <= 0.0 {
                continue;
            }
            for (x, z) in grid.disc((c.position.0.to_f32(), c.position.1.to_f32()), r) {
                set.set(x, z);
            }
        }
        // A human faction's spy-network discs (the sight object's +0x20 list, `0x00B617E0` reads it
        // through `0x00AE5F40`; CONFIRMED structure): the shapes listed at its last turn start.
        for g in &who {
            for &(p, r) in self.world.network_sight.get(g).into_iter().flatten() {
                for (x, z) in grid.disc(p, r) {
                    set.set(x, z);
                }
            }
        }
        for r in self.world.regions.values() {
            if who.contains(&r.owner) {
                for (x, z) in grid.disc((r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32()), 5.0) {
                    set.set(x, z);
                }
                if let Some(shape) = self.world.region_sight.get(&r.id) {
                    for &(x, z) in shape {
                        set.set(x, z);
                    }
                }
            }
            for s in &r.slots {
                let holder = s.holder.unwrap_or(r.owner);
                if who.contains(&holder)
                    && let Some(p) = s.position
                {
                    for (x, z) in grid.disc((p.0.to_f32(), p.1.to_f32()), 5.0) {
                        set.set(x, z);
                    }
                }
            }
        }
        // Trade route segments (`0x00BCA310` → `0x00BD2BC0`): a domestic route of the faction, or
        // an international one it exports along that still runs (its importer, the governor of the
        // last waypoint's region, is still a trade partner). PROVISIONAL: the route lists are those
        // of the loaded file; routes the model builds later add no segments.
        let mut partners: BTreeMap<FactionId, Vec<FactionId>> = BTreeMap::new();
        for g in &who {
            partners.insert(*g, super::economy::trade_partners(self, *g));
        }
        for s in &self.world.trade_sight {
            let seen = s.domestic.iter().any(|g| who.contains(g))
                || s.international.iter().any(|(g, r)| {
                    partners.get(g).is_some_and(|p| self.world.governing_faction(*r).is_some_and(|i| p.contains(&i)))
                });
            if seen {
                for &(x, z) in &s.cells {
                    set.set(x, z);
                }
            }
        }
        Some(set)
    }

    /// Updates `faction`'s visible cells and adds them to its explored ones (nothing for a faction
    /// without a shroud). `reset`: the visible set becomes what the sources see now; otherwise what
    /// they see now is added. Timing CONFIRMED (CHARACTERS_FIDELITY.md §10): the original's box
    /// updates after a source changes (`0x00A27F10` / `0x00B790F0` / the destructor `0x0099D2D0` →
    /// `0x00B1B8B0` → `0x00B617E0` with its clear flag 0) only OR the sources in the box into the
    /// visible tree (+0x1C) and the moved source's new shape into the explored tree (+4); the
    /// visible tree is cleared and rebuilt from every source over the whole map at the faction's
    /// turn END (`0x008BD0F0` → `0x00B66EF0(0)`, after the turn-end counters and `FactionTurnEnd`;
    /// skipped for a faction in a revolution on the revolutionaries' side) and at the setups `0x008DD090` /
    /// `0x008BD4F0` / `0x008EE3E0`. So `reset` belongs to the turn end, and a turn start adds only.
    pub fn refresh_shroud(&mut self, faction: FactionId, reset: bool) {
        if !self.world.shrouds.contains_key(&faction) {
            return;
        }
        let Some(visible) = self.compute_visible(faction) else { return };
        if let Some(s) = self.world.shrouds.get_mut(&faction) {
            s.explored.union_with(&visible);
            if reset {
                s.visible = visible;
            } else {
                s.visible.union_with(&visible);
            }
        }
    }

    /// Whether `faction` sees a map position (`0x00B7A150`, CONFIRMED): true without a shroud or with
    /// the shroud off; else its cell is visible and not hidden.
    pub fn sees(&self, faction: FactionId, p: (f32, f32)) -> bool {
        let Some(s) = self.world.shrouds.get(&faction) else { return true };
        if !s.active {
            return true;
        }
        let Some(grid) = self.world.sight_grid else { return true };
        let Some((x, z)) = grid.cell_of(p) else { return false };
        s.visible.get(x, z) && !s.hidden.get(x, z)
    }

    /// Whether `faction` sees a map position *and the four cells around it* (`0x00B7A1D0`, CONFIRMED):
    /// true without a shroud, with the shroud off or without a sight grid (as [`Self::sees`]); else
    /// the position's cell and its neighbours at x ± 1 and z ± 1 must each be visible and not hidden;
    /// a cell off the grid fails. The campaign director's "no human sees it" test uses it
    /// ([`super::director`]).
    pub fn sees_fully(&self, faction: FactionId, p: (f32, f32)) -> bool {
        let Some(s) = self.world.shrouds.get(&faction) else { return true };
        if !s.active {
            return true;
        }
        let Some(grid) = self.world.sight_grid else { return true };
        let Some((x, z)) = grid.cell_of(p) else { return false };
        let seen = |x: Option<u32>, z: Option<u32>| matches!((x, z), (Some(x), Some(z)) if s.visible.get(x, z) && !s.hidden.get(x, z));
        seen(Some(x), Some(z))
            && seen(x.checked_sub(1), Some(z))
            && seen(x.checked_add(1), Some(z))
            && seen(Some(x), z.checked_sub(1))
            && seen(Some(x), z.checked_add(1))
    }

    /// What `faction` knows about a map position, in the three states a renderer needs to tell
    /// apart: **never seen** (under the shroud, out of sight — the original paints it black),
    /// **explored** (seen at some point, not now — the original paints it dimmed), and **visible**
    /// (in sight now, painted in full).
    ///
    /// The three states are the exe's three shroud cell sets (`Shroud`: `explored` #0, `visible` #1,
    /// `hidden` #2) read the way `0x00B7A150` reads them: a cell is *visible* when it is in
    /// `visible` and not in `hidden`, and *explored* when it is in `explored` at all. A faction
    /// without a shroud, or with the shroud off, sees everything (`sees` answers true for it too).
    ///
    /// INFERRED that the original draws these as full / dimmed / black: the cell sets are
    /// CONFIRMED, but the exe's terrain shading for them is not read (the campaign scene's terrain
    /// material is 0-D's file, `crates/napoleon/src/campaign/scene.rs`). Callers that want to shade
    /// per terrain vertex or per pixel use this; callers that only need "can I see it" use
    /// [`sees`](Self::sees).
    ///
    /// The checks run in the same order as [`sees`](Self::sees) (no shroud, shroud off, no grid:
    /// visible; off the grid: never seen), so `fog_state(f, p) == Visible` is exactly
    /// `sees(f, p)`. The map labels use neither: [`hides_map_label`](Self::hides_map_label).
    pub fn fog_state(&self, faction: FactionId, p: (f32, f32)) -> FogState {
        let Some(s) = self.world.shrouds.get(&faction) else { return FogState::Visible };
        if !s.active {
            return FogState::Visible;
        }
        let Some(grid) = self.world.sight_grid else { return FogState::Visible };
        // Off the grid: `sees` answers false there, so it is never shaded as known ground.
        let Some((x, z)) = grid.cell_of(p) else { return FogState::NeverSeen };
        self.fog_state_at(faction, x, z)
    }

    /// The map position of a sight cell's **centre** — the inverse of
    /// [`SightGrid::cell_of`], for a renderer that walks the grid and needs to place each cell.
    /// `None` without a sight grid.
    ///
    /// A cell's corners are at `origin + (col, row) * cell`; its centre is half a cell in.
    /// `fog_states` is indexed by `row * cols + col`, so this is what turns a flat index back
    /// into a place on the map.
    pub fn cell_centre(&self, x: u32, z: u32) -> Option<(f32, f32)> {
        let g = self.world.sight_grid?;
        Some((g.origin.0 + (x as f32 + 0.5) * g.cell, g.origin.1 + (z as f32 + 0.5) * g.cell))
    }

    /// The fog state of one **sight cell**, for a caller walking the grid.
    ///
    /// This is the primitive a terrain renderer wants: given a cell it answers without the
    /// caller having to reconstruct a map position and call [`fog_state`](Self::fog_state),
    /// which is what makes the two paths able to disagree. Kept as the single definition of
    /// the three-way test — `fog_state` and `fog_states` both defer to it — so a renderer and
    /// the labels layer cannot shade a cell differently from the way it is filtered.
    ///
    /// A faction with no shroud, with the shroud off, or a model with no grid is **visible**; a
    /// cell outside the grid is **never seen** — both matching [`fog_state`](Self::fog_state)
    /// for a position off the map.
    pub fn fog_state_at(&self, faction: FactionId, x: u32, z: u32) -> FogState {
        let Some(s) = self.world.shrouds.get(&faction) else { return FogState::Visible };
        if !s.active {
            return FogState::Visible;
        }
        let Some(g) = self.world.sight_grid else { return FogState::Visible };
        if x >= g.cols || z >= g.rows {
            return FogState::NeverSeen;
        }
        if s.visible.get(x, z) && !s.hidden.get(x, z) {
            FogState::Visible
        } else if s.explored.get(x, z) {
            FogState::Explored
        } else {
            FogState::NeverSeen
        }
    }

    /// Whether `faction`'s shroud hides the map label at a position (CONFIRMED, the labels layer's
    /// only fog test): its cell is in the hide set (#2, shroud +0x34). The label handler
    /// (`0x009F4520` → `0x009B0400`) asks `0x009BAA90` per settlement, which skips the test when the
    /// faction has no shroud and drops the label only when `0x00B11E70` is true: the shroud is on
    /// (+0x50) and the cell, from `0x00B5B5A0`, is in the hide set (`0x00E25280`, false off the
    /// grid). Neither the visible (#1) nor the explored (#0) set is read, so a settlement never
    /// seen keeps its label, as one in sight does; the hide set (the black shroud) is empty
    /// in every vanilla save but the tutorial's.
    pub fn hides_map_label(&self, faction: FactionId, p: (f32, f32)) -> bool {
        let Some(s) = self.world.shrouds.get(&faction) else { return false };
        if !s.active {
            return false;
        }
        let Some((x, z)) = self.world.sight_grid.and_then(|g| g.cell_of(p)) else { return false };
        s.hidden.get(x, z)
    }

    /// The script's `disable_shopping_for_ai_under_shroud(on)` (CONFIRMED, `0x00AA9B40`; the
    /// binding is registered at `0x0145818C`): off clears the CAI interface's shroud (+0x618); on
    /// stores the shroud of the one human faction (faction +0x6F8), or none when no faction or more
    /// than one is human. The exe keeps the shroud object and saves only whether it is set
    /// (`CAI_INTERFACE`, version 6 and later), resolving it again on load (`0x00A3EF07`); ours keeps
    /// the faction ([`World::ai_shopping_shroud`](super::world::World::ai_shopping_shroud)).
    pub fn set_ai_shopping_shroud(&mut self, on: bool) {
        self.world.ai_shopping_shroud = match (on, self.turn.humans.as_slice()) {
            (true, [human]) => Some(*human),
            _ => None,
        };
    }

    /// Whether the AI may not shop at a position (CONFIRMED, `0x00A8B2D0`): a shroud is set
    /// ([`Self::set_ai_shopping_shroud`]) and it hides the position's cell (`0x00B11E70`, the test
    /// of [`Self::hides_map_label`]). The AI's ship repair asks it for the navy's position
    /// ([`Self::ai_repair_navy`]), its building repair for the slot's ([`Self::ai_repairs`]).
    pub fn ai_shopping_hidden(&self, p: (crate::fixed::Fixed20, crate::fixed::Fixed20)) -> bool {
        self.world.ai_shopping_shroud.is_some_and(|f| self.hides_map_label(f, (p.0.to_f32(), p.1.to_f32())))
    }

    /// The script's `show_shroud(on)` (CONFIRMED, binding `0x00979260`): every faction that keeps a
    /// shroud (the campaign's faction list +0xF5C, faction +0x6F8 not null) gets its on flag
    /// (shroud +0x50, [`Shroud::active`]) set to `on` by `0x00B68500`, which then refreshes the
    /// display (`0x008ED860` and a redraw of every map region through the game's +0x210 object;
    /// no model state).
    pub fn show_shroud(&mut self, on: bool) {
        for s in self.world.shrouds.values_mut() {
            s.active = on;
        }
    }

    /// The shroud part of a new campaign's per-faction setup (CONFIRMED, `0x008DD090`, run for
    /// every faction by `0x0095A810` from `0x00915E20`, which the model ctor `0x00872550` calls at
    /// 0x00874009 only with a campaign setup, so never on a save load: a loaded faction reads its
    /// whole shroud from the file, `0x0087A190` → `0x00AE89A0`, and keeps it). The rebels (no
    /// faction record, `IsFactionWithoutRecord` `0x008CEEF0`) skip the whole setup and keep whatever
    /// shroud the start position gave them. Any other faction that is not human loses its shroud
    /// (faction +0x6F8). A human without one gets a new shroud (`0x00AE8910`): the three sets sized
    /// to the sight grid (map data +0x3C / +0x40), explored and visible empty, the black shroud
    /// ([`Shroud::hidden`]) every cell when the map has black shroud outlines (+0xC0 not 0) and
    /// empty otherwise, and on. Then every human's shroud, new or the start position's, runs
    /// `0x00B66EF0(1)`: the visible tree (+0x1C) is cleared and rebuilt box by box
    /// (`0x00E143F0`, then `0x00B617E0` with clear 0 for each of the map data's boxes,
    /// `**(campaign+0xF58)` +0x6C, stride 0xBC, ORing in the sources each box sees), and with the
    /// flag 1 the explored tree (+4) becomes a copy of it (its header copied, `0x00E02070` copying
    /// the tree +0x28 over +0x10). So explored = visible = what the sources see now; the start
    /// position's explored and visible sets are replaced, its black shroud is kept.
    /// UNKNOWN: `0x008DD090` also keeps a non-human's shroud (and rebuilds it the same way) when
    /// the setup's +0x20 and the faction's +0x6E1 (the player slot's +0xA9) are both set; not
    /// ported (no such flag yet).
    /// PROVISIONAL: `0x00B66EF0` does nothing for a faction whose +0x814 flag (`0x008CA0B0`) is set
    /// and +0x81C clear (in a revolution, on the revolutionaries' side); neither flag is in our model, so
    /// the rebuild always runs.
    /// Without a sight grid no shroud can be sized or rebuilt: logged once per call when a human
    /// lacks one, and that human plays without a shroud (sees everything, as a faction without one).
    pub fn start_shrouds(&mut self) {
        let humans = self.turn.humans.clone();
        let dropped: Vec<FactionId> = self
            .world
            .shrouds
            .keys()
            .copied()
            .filter(|f| !humans.contains(f) && !self.is_rebel_faction(*f))
            .collect();
        for f in dropped {
            self.world.shrouds.remove(&f);
        }
        let humans: Vec<FactionId> = humans.into_iter().filter(|f| !self.is_rebel_faction(*f)).collect();
        let Some(grid) = self.world.sight_grid else {
            let missing: Vec<&FactionId> = humans.iter().filter(|f| !self.world.shrouds.contains_key(f)).collect();
            if !missing.is_empty() {
                log::warn!("campaign start: no sight grid, so {missing:?} start without a shroud");
            }
            return;
        };
        let black = !self.world.black_shroud_outlines.is_empty();
        for f in humans {
            let Some(seen) = self.compute_visible(f) else { continue };
            let shroud = self.world.shrouds.entry(f).or_insert_with(|| {
                let hidden = if black { CellSet::full(grid.cols, grid.rows) } else { CellSet::new(grid.cols, grid.rows) };
                Shroud { explored: CellSet::new(grid.cols, grid.rows), visible: CellSet::new(grid.cols, grid.rows), hidden, active: true }
            });
            shroud.explored = seen.clone();
            shroud.visible = seen;
        }
    }

    /// The script's `unveil_black_shroud(key)` (CONFIRMED, binding `0x00977A90` → `0x008F7760`):
    /// for every faction that keeps a shroud, `0x00B65BC0` lifts each black shroud outline named
    /// `key` ([`World::black_shroud_outlines`](super::world::World::black_shroud_outlines), exact
    /// name match, every match in file order) with `0x00B65C50`, then has the path grid rebuild
    /// its obstacle layers for the view it last stored (`CampaignPathGrid::ReapplyPathGridFactionView`
    /// `0x00B67490`, the refresh the diplomacy changes run too; that view is whichever faction the
    /// grid stored last, not the one whose shroud was lifted) and posts the key to the game's
    /// +0xD08 object (not traced; no model state found). The rebuild reads the view faction's
    /// shroud: an obstacle whose place it does not see, or sees under the black shroud, is hidden
    /// (mode 5, `0x00B3F4F0` → `0x00B6A920` → `0x00B7A150`). Ours decides that per search
    /// ([`Self::obstacles_seen_by`]) from the shroud as it stands, so nothing is refreshed here.
    /// Per outline of 3 points or more:
    /// 1. the box around the outline, widened by 2 at its low x and z and by 4 at its high x and
    ///    not at all at its high z (as the exe does), is turned into the sight cells x0..=x1,
    ///    z0..=z1 (`0x00B42670`: floor((v − origin) × count / extent) for each corner);
    /// 2. inside that cell box every explored cell (#0) still under the black shroud (#2) is
    ///    forgotten (`0x00E29E60`, explored &= !hidden), so ground lifted from the shroud shows as
    ///    never seen until something sees it;
    /// 3. a cell of the box leaves the black shroud (`0x00E2A180`, hidden &= !mask) when its low
    ///    corner is inside the outline (even-odd, `0x010E7320`) or nearer the closed outline than
    ///    0.707 × the cell height + 0.5 (`0x00B287A0`).
    ///
    /// Returns how many outlines matched; 0 also without a sight grid.
    pub fn unveil_black_shroud(&mut self, key: &str) -> usize {
        let Some(grid) = self.world.sight_grid else { return 0 };
        let w = &mut self.world;
        let mut matched = 0;
        for (name, outline) in &w.black_shroud_outlines {
            if name != key {
                continue;
            }
            matched += 1;
            let Some(lift) = BlackShroudLift::of(&grid, outline) else { continue };
            for s in w.shrouds.values_mut() {
                lift.apply(s);
            }
        }
        matched
    }

    /// The same three states as [`fog_state`](Self::fog_state), but per sight cell — for a caller
    /// that walks the grid once (a terrain mesh build, a fog texture) instead of per map position.
    ///
    /// **Covers the whole campaign map**, not the regions: the vector is `cols * rows` over
    /// `world.sight_grid`, the same whole-map grid the shroud's saved bit array uses
    /// (`QUAD_TREE_BIT_ARRAY`, `ntw_campaign::shroud`). A cell with no region in it is still
    /// present, and reads `NeverSeen` until something sees it.
    ///
    /// `None` means there is nothing to render — no shroud for this faction or no grid — and
    /// the caller should treat the map as fully visible, which is what `fog_state` does for the
    /// same two cases.
    pub fn fog_states(&self, faction: FactionId) -> Option<Vec<FogState>> {
        let s = self.world.shrouds.get(&faction)?;
        let grid = self.world.sight_grid?;
        if !s.active {
            return None;
        }
        let mut out = vec![FogState::NeverSeen; grid.cols as usize * grid.rows as usize];
        for z in 0..grid.rows {
            for x in 0..grid.cols {
                out[z as usize * grid.cols as usize + x as usize] = self.fog_state_at(faction, x, z);
            }
        }
        Some(out)
    }
}

/// How much of the map `faction` knows at a cell: the three shroud states a renderer tells apart.
/// See [`CampaignModel::fog_state`]. The states are CONFIRMED cell sets; how the original shades
/// each one (black / dimmed / full) is INFERRED, not read from the exe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FogState {
    /// Under the shroud and never in sight (INFERRED: painted black).
    #[default]
    NeverSeen,
    /// In sight at some point but not now (INFERRED: painted dimmed).
    Explored,
    /// In sight now: painted in full.
    Visible,
}


/// A missionary (agent types 6..10 of the type table, `0x00F9C710`: the catholic, orthodox,
/// Protestant, Indian and Middle East missionaries).
fn is_missionary(kind: CharacterKind) -> bool {
    matches!(kind, CharacterKind::CatholicMissionary | CharacterKind::OrthodoxMissionary | CharacterKind::ProtestantMissionary)
        || kind.esf_name().to_ascii_lowercase().contains("missionary")
}

impl CampaignModel {
    /// The stealth test (`0x009D1010`, CONFIRMED structure; CHARACTERS_FIDELITY.md §10): a character
    /// can hide when he is not inside a settlement (`0x009D3CB0`), commands a force that is not
    /// carried by a fleet (INFERRED for `0x00A0B8C0`, force +0x94), and either has
    /// `campaign_map_stealth` (bonus 80) > 0, or every unit of the force can hide on the campaign
    /// map (`unit_stats_land` #69), or he stands on hiding ground (`campaign_ground_types` #2 at his
    /// position, `0x00A88EB0` +0x14); and no character of another faction is in his force.
    /// Character +0x510 (`CHARACTER` #27, `CharacterDetails::fled`: he fled a duel) refuses him.
    /// PROVISIONAL: the world mode check (world +0xF6C, values 0 or 8) is taken as passed (hidden
    /// armies exist in both vanilla campaigns).
    pub fn stealthy(&self, c: CharacterId) -> bool {
        let Some(ch) = self.world.characters.get(&c) else { return false };
        if ch.garrisoned_in.is_some() {
            return false;
        }
        // Character +0x510 (`CHARACTER` #27): one who fled a duel cannot hide (CONFIRMED test).
        if self.world.character_details.get(&c).is_some_and(|d| d.fled) {
            return false;
        }
        let Some(fid) = self.force_of(c) else { return false };
        if self.world.embarked.contains_key(&fid) {
            return false;
        }
        let Some(force) = self.world.forces.get(&fid) else { return false };
        let bonus = super::effects::Effects::character_effects(self, c).get("campaign_map_stealth") > 0.0;
        if !bonus {
            let all_units = !force.units.is_empty()
                && force.units.iter().all(|u| self.rules.units.get(&u.unit_key).is_some_and(|r| r.campaign_stealth));
            if !all_units {
                let (x, z) = (ch.position.0.to_f32(), ch.position.1.to_f32());
                let ground = self.terrain.as_ref().and_then(|t| t.0.ground_type_at(x, z).map(str::to_string));
                if !ground.is_some_and(|g| self.rules.hiding_ground.contains(&g)) {
                    return false;
                }
            }
        }
        force.units.iter().filter_map(|u| u.character).all(|x| self.world.characters.get(&x).is_none_or(|y| y.faction == ch.faction))
    }

    /// Re-evaluates a character's hidden flag (`0x009D3000` → `0x009D0FB0` → [`Self::stealthy`];
    /// `CharacterDetails::hidden`). The original runs it for every character when a campaign is
    /// created (`0x008B42F0`), at the character's turn start (`0x00A24EB0`), after a move
    /// (`0x00A285E0`) and when his force's units change (`0x008B4260`). Rebel characters are left
    /// as they are (INFERRED: rebel armies on hiding ground are not hidden in the vanilla saves).
    pub fn update_hidden(&mut self, c: CharacterId) {
        let Some(ch) = self.world.characters.get(&c) else { return };
        // A character of a faction the world no longer holds is left as it is too.
        if !self.world.factions.contains_key(&ch.faction) || self.world.is_rebel_faction(ch.faction) {
            return;
        }
        let hidden = self.stealthy(c);
        if let Some(d) = self.world.character_details.get_mut(&c) {
            d.hidden = hidden;
        }
    }

    /// The spotting pass at the faction's turn start (`0x008B4B70`, from `0x008F2480`, CONFIRMED;
    /// CHARACTERS_FIDELITY.md §10). Spotters: the faction's characters not in a settlement with
    /// `subterfuge`, `research` or `zeal` above 0 that are missionaries (`0x00F9C710`; the other
    /// way in, character +0x4C5 / `CHARACTER` #14, is false in every vanilla save and not
    /// modelled). Each looks at the other factions' characters within his sight radius and, for
    /// each the faction does not know ([`super::agents::knows_character`]):
    /// - score = his rank, + 2 for a missionary whose `subterfuge` is exactly 0 (+ 1 for another
    ///   such character), + 4 when the target stands in a region the faction owns, then 1..=9;
    /// - threshold: against a target with `subterfuge` ≥ 1, (score − the target's rank + 9) × 5;
    ///   against a hidden one, 90 if the spotter has `subterfuge` > 0, else 25; others are skipped;
    /// - one `percent_0_100` draw on the campaign RNG: at or above the threshold the target, unless
    ///   inside a settlement, is spotted ([`Self::expose`]; message 246 and event 0x13D not modelled).
    ///
    /// PROVISIONAL: the spotter's sight is taken as the circle of his radius (the original uses his
    /// saved sight shape when he has one, the same disc up to its rim); spotters and targets are
    /// taken in id order. Returns the (spotter, spotted) pairs.
    pub fn spotting_pass(&mut self, faction: FactionId) -> Vec<(CharacterId, CharacterId)> {
        use super::agents::{attribute, knows_character, rank, SUBTERFUGE};
        let mut out = Vec::new();
        let spotters: Vec<CharacterId> = self
            .world
            .characters
            .values()
            .filter(|c| c.faction == faction && c.garrisoned_in.is_none() && is_missionary(c.kind))
            .map(|c| c.id)
            .collect();
        for s in spotters {
            if !(attribute(self, s, SUBTERFUGE) > 0 || attribute(self, s, 6) > 0 || attribute(self, s, 7) > 0) {
                continue;
            }
            let Some(sc) = self.world.characters.get(&s).cloned() else { continue };
            let r = self.sight_radius(s);
            let sp = (sc.position.0.to_f32(), sc.position.1.to_f32());
            let targets: Vec<CharacterId> = self
                .world
                .characters
                .values()
                .filter(|t| t.faction != faction)
                .filter(|t| {
                    let (dx, dz) = (t.position.0.to_f32() - sp.0, t.position.1.to_f32() - sp.1);
                    dx * dx + dz * dz <= r * r
                })
                .map(|t| t.id)
                .collect();
            let sub_s = attribute(self, s, SUBTERFUGE);
            for t in targets {
                let Some(tc) = self.world.characters.get(&t).cloned() else { continue };
                if knows_character(self, faction, t) {
                    continue;
                }
                let mut score = rank(self, s);
                if sub_s == 0 {
                    score += if is_missionary(sc.kind) { 2 } else { 1 };
                }
                if self.world.regions.values().any(|reg| reg.owner == faction && super::economy::in_region(self, reg, &tc)) {
                    score += 4;
                }
                let score = score.clamp(1, 9);
                let hidden = self.world.character_details.get(&t).is_some_and(|d| d.hidden);
                let threshold = if attribute(self, t, SUBTERFUGE) > 0 {
                    (score - rank(self, t) + 9) * 5
                } else if hidden {
                    if sub_s > 0 { 90 } else { 25 }
                } else {
                    continue;
                };
                let draw = self.rng.percent_0_100() as i32;
                if threshold <= draw && tc.garrisoned_in.is_none() {
                    self.expose(faction, t);
                    out.push((s, t));
                }
            }
        }
        out
    }
}

impl CampaignModel {
    /// A human faction's spy-network step at its turn start (`0x008F2480`, CONFIRMED structure;
    /// CHARACTERS_FIDELITY.md §10): the shapes listed at the last turn start are revealed again,
    /// the list is emptied and refilled from the faction's characters passing `0x008B4120`
    /// (`subterfuge` > 0 and idle for 3 turns or more, `CHARACTER` #15), each revealed at once
    /// (`0x008F9A00`); a character at exactly 3 idle turns gets message 250
    /// `spy_network_established` and the script event `CharacterBuildsSpyNetwork` (returned).
    /// The listed discs count as sight sources until the next turn start
    /// ([`World::network_sight`](super::world::World::network_sight)). Reveals here are the
    /// turn start's additive refresh ([`Self::refresh_shroud`]). PROVISIONAL: `0x008B4120` also
    /// asks the character's locomotable one thing (`0x00898B80`, not decoded); the model asks
    /// for a character on the map (not inside a settlement).
    pub fn spy_network_step(&mut self, faction: FactionId) -> Vec<CharacterId> {
        use super::agents::{attribute, SUBTERFUGE};
        let mut established = Vec::new();
        let mut shapes = Vec::new();
        let mine: Vec<CharacterId> = self.world.characters.values().filter(|c| c.faction == faction && c.garrisoned_in.is_none()).map(|c| c.id).collect();
        for c in mine {
            let idle = self.world.character_details.get(&c).map_or(0, |d| d.idle_turns);
            if idle < 3 || attribute(self, c, SUBTERFUGE) <= 0 {
                continue;
            }
            let Some(ch) = self.world.characters.get(&c) else { continue };
            shapes.push(((ch.position.0.to_f32(), ch.position.1.to_f32()), self.sight_radius(c)));
            if idle == 3 {
                established.push(c);
            }
        }
        self.world.network_sight.insert(faction, shapes);
        established
    }
}

/// One trade route segment's sight (`CAMPAIGN_TRADE_MANAGER` `TRADE_SEGMENTS[]`: its saved
/// `LINE_OF_SIGHT` shape and the routes that use it). `0x00BD2BC0` (CONFIRMED) makes a segment a
/// sight source for a faction when one of the segment's domestic routes (segment +0x44 list,
/// saved child `LINE_OF_SIGHT` − 4: route ids of `DOMESTIC_TRADE_ROUTES`) or international routes
/// (+0x54 list, `LINE_OF_SIGHT` − 3: ids of `INTERNATIONAL_TRADE_ROUTES`) belongs to it or to its
/// protectorate. Matched exactly: Russia's and Prussia's saved visible sets in a turn-4 save.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeSegmentSight {
    /// The cells the segment sees.
    pub cells: Vec<(u32, u32)>,
    /// Owners of the domestic routes on it.
    pub domestic: Vec<FactionId>,
    /// The international routes on it: (exporter, region of the route's last waypoint).
    pub international: Vec<(FactionId, RegionId)>,
    /// The segment's course (`TRADE_SEGMENTS[]` #1 n, then 4n coord2d): n cubic Bézier curves of four
    /// control points each, consecutive curves sharing their end points. Static data. The commerce
    /// raids measure a position's distance to it ([`super::commerce_raid::segment_distance`]).
    #[cfg_attr(feature = "serde", serde(default))]
    pub curves: Vec<[(f32, f32); 4]>,
}

/// Saved region sight shapes by region (`REGION` `LINE_OF_SIGHT`, cells).
pub type RegionSight = BTreeMap<RegionId, Vec<(u32, u32)>>;

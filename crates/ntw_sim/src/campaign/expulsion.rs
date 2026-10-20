//! A faction's forces leave a region they may not stand in (`ExpelFactionForcesFromRegion`
//! `0x008D9DC0`, CONFIRMED from the disassembly; analysis/fidelity/CAMPAIGN_FIDELITY.md, director
//! section "Transfer and the old owner's forces").
//!
//! Callers: every change of a region's owner (`TransferRegionToFaction` `0x00A64AC0` with its fourth
//! argument 0, which every settlement handover `0x00B449F0` passes: captures, deals, liberations, the
//! director's transfer; [`CampaignModel::expel_after_owner_change`]), an accepted deal's region lists
//! (`0x00C114B0`, [`CampaignModel::accept_deal`]) and the end of a protectorate tie (`0x00B64B10`,
//! [`CampaignModel::end_protectorate_tie`]) and every loss of military access, over all the giver's
//! regions ([`CampaignModel::expel_from_lands_of`]): timed access running out (`0x00B29170`,
//! [`CampaignModel::diplomacy_round_end`]), a cancel (`0x00B67BD0`,
//! [`CampaignModel::cancel_military_access`]), a broken alliance (`0x00B13A30` / `0x00B0CA20`,
//! [`CampaignModel::break_alliance`]) and a peace (`0x00B0D0C0`, [`CampaignModel::make_peace_rules`])
//! that give back no access. All but the cancel then flood both factions' zones of control again
//! ([`CampaignModel::expel_from_lands_of`]); the cancel loops alone ([`CampaignModel::expel_from_each_region_of`]).
//!
//! The rule ([`expel_faction_from_region`], behind the [`super::seams::REGION_EXPULSION`] seam) says
//! which forces go where; the model applies it ([`CampaignModel::expel_from_region`]). Agents are
//! never moved.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::mod_state::ModWrites;
use super::pathing::Domain;
use super::world::{CampaignModel, MilitaryForce, Region, Stance};
use crate::fixed::Fixed20;

/// How far from a candidate point a force may be placed, in map units: the 100.0 every placement of
/// `0x008D9DC0` passes (`0x00B37670`, `0x00B35570`, `0x00B377E0`; CONFIRMED). The flood `0x00B35640`
/// succeeds when the place it finds is closer than that.
pub const PLACEMENT_RADIUS: f32 = 100.0;

/// One force an expulsion moves.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpelledForce {
    /// The force. An army leaves its residence; a navy leaves its port.
    pub force: ForceId,
    /// Where its commander is put; `None`: it stays where it stands (no candidate placed it).
    pub to: Option<(Fixed20, Fixed20)>,
}

/// `0x00B16570` on `owner`'s relationship record towards `f`: the record lets `f` stand in `owner`'s
/// land. True when `owner` gives `f` military access (#3 not 0), the record's
/// `allied_in_war_against` list is not empty, `owner` is `f`'s protectorate, or they are at war
/// (stance index 0). CONFIRMED.
pub(super) fn record_lets_stand(m: &CampaignModel, owner: FactionId, f: FactionId) -> bool {
    m.world.relationships.get(&(owner, f)).is_some_and(|r| r.military_access_turns != 0 || !r.allied_in_war_against.is_empty())
        || matches!(m.world.stance(owner, f), Stance::Protectorate | Stance::War)
}

/// The "may stay" flag of `0x008D9DC0` (CONFIRMED): `n` (the region's owner) is `f`, either has no
/// diplomacy record (the rebels) or they are at war ([`CampaignModel::at_war`], `0x008CE9B0`), or
/// `n`'s record towards `f` lets it stand ([`record_lets_stand`]).
fn may_stay(m: &CampaignModel, n: FactionId, f: FactionId) -> bool {
    n == f || m.at_war(n, f) || record_lets_stand(m, n, f)
}

fn to_fixed(p: (f32, f32)) -> (Fixed20, Fixed20) {
    (Fixed20::from_f64(f64::from(p.0)), Fixed20::from_f64(f64::from(p.1)))
}

/// Where a land army standing at `here` may go when it leaves `region`, in the order they are tried
/// (`0x008C6010` builds the list, `0x008DCB30` orders it; CONFIRMED): the region itself at the
/// army's own position when it may stay; `f`'s other regions; the regions of factions not at war
/// with `f` whose record lets it stand ([`record_lets_stand`]), not the region. Each of the last two
/// lists is sorted by the squared distance from the army to the candidate's point.
///
/// PROVISIONAL: the model has no theatres (as `nearest_capital`), so every region counts and the
/// exe's fourth part, `f`'s capital in another theatre, never happens. That matches every shipped
/// map, each one theatre holding all its regions (CONFIRMED, `economy_check theatres`,
/// CAMPAIGN_FIDELITY.md "Theatres CONFIRMED a no-op"); a map with several theatres needs the exe's
/// theatre filter.
///
/// PROVISIONAL stand-ins: the point is the region's nearest outline point in the exe (`0x008AF750`);
/// ours is the centre of the region's path-grid cell nearest to the army, or its settlement without
/// the map. `f`'s regions are in its gain order there (faction `+0x778`), in region-id order here,
/// and the exe's sort (std::sort `0x00868A30`) is not stable where ours is.
fn army_candidates<'a>(m: &'a CampaignModel, f: FactionId, region: &'a Region, here: (f32, f32), stay: bool) -> Vec<(&'a str, (f32, f32))> {
    let cells = m.terrain.as_ref().map(|t| (&t.0, t.0.nearest_cell_per_region(here.0, here.1)));
    let point = |r: &Region| {
        let s = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        match &cells {
            Some((g, cells)) => g.region_index(&r.key).and_then(|i| cells.get(i as usize).copied().flatten()).map_or(s, |c| g.centre(c)),
            None => s,
        }
    };
    let by_distance = |list: &mut Vec<(&str, (f32, f32))>| {
        list.sort_by(|a, b| {
            let d = |p: (f32, f32)| (p.0 - here.0).powi(2) + (p.1 - here.1).powi(2);
            d(a.1).total_cmp(&d(b.1))
        });
    };
    let others = m.world.regions.values().filter(|r| r.id != region.id);
    let mut own: Vec<(&str, (f32, f32))> = others.clone().filter(|r| r.owner == f).map(|r| (r.key.as_str(), point(r))).collect();
    let mut open: Vec<(&str, (f32, f32))> =
        others.filter(|r| r.owner != f && !m.at_war(r.owner, f) && record_lets_stand(m, r.owner, f)).map(|r| (r.key.as_str(), point(r))).collect();
    by_distance(&mut own);
    by_distance(&mut open);
    let mut out = Vec::with_capacity(own.len() + open.len() + 1);
    if stay {
        out.push((region.key.as_str(), here));
    }
    out.extend(own);
    out.extend(open);
    out
}

/// Places a force near `point`: in the region `key` on land (`0x00B37670` for the region itself,
/// `0x00B35570` for another, both the flood `0x00B35640`), or at sea (`0x00B377E0`, mode 2) when
/// `key` is `None`. PROVISIONAL stand-in for the flood (a best-first search over the region's cells
/// from the point, avoiding other characters' footprints): the point itself when its path-grid cell
/// fits, else the nearest fitting cell's centre closer than [`PLACEMENT_RADIUS`]
/// ([`super::pathing::PathGrid::nearest_place_within`]); the model has no footprints. Without the
/// map, a land placement is the point itself and a sea one fails.
fn place(m: &CampaignModel, key: Option<&str>, point: (f32, f32)) -> Option<(f32, f32)> {
    let Some(t) = &m.terrain else { return key.map(|_| point) };
    let g = &t.0;
    match key {
        Some(k) => g.nearest_place_within(point.0, point.1, Some(g.region_index(k)?), Domain::Land, PLACEMENT_RADIUS),
        None => g.nearest_place_within(point.0, point.1, None, Domain::Sea, PLACEMENT_RADIUS),
    }
}

/// The original's expulsion of `f`'s forces from `region` (`0x008D9DC0`, CONFIRMED), as a plan the
/// model applies ([`CampaignModel::expel_from_region`]); the default [`super::seams::RegionExpulsion`].
///
/// Nothing moves when `f` is out of the game (faction `+0x824`, the early return at `0x008D9DDC`;
/// CONFIRMED), whichever caller asks. The forces come in the order of `f`'s character list (+0x768,
/// [`super::world::World::characters_in_list_order`]), each commander's army or navy in turn, as the
/// exe's one loop takes them (CONFIRMED, `0x008D9E7A`).
///
/// - **Land armies** of `f` standing in the region ([`super::economy::in_region`]): the candidates
///   ([`army_candidates`]) are tried in order until one places it ([`place`]). An army aboard a navy
///   never counts. INFERRED: the exe's test (`0x00A075B0` → `0x00B4D7B0` → `0x00B4D800`) is the
///   region of the path-grid polygon under the army's position (`0x00A17D50`), which for an army
///   aboard is its carrier's (at sea, or a docked carrier's port exit `0x00A99A60`), so a sea
///   polygon; whether the grid's shared-polygon lists (index past `+0x140`, lists at `+0x154`) can
///   name a land region there is not traced.
/// - Placed or not, an army leaves its residence (vt+0x90; for a settlement `0x00B0D610` unlinks
///   the garrison), whether or not it may stay (CONFIRMED, `0x008D9DC0`). The exe's one exception:
///   the owner at war with `f` and the residence's vt+0x54 returning 0 leaves the army untouched.
///   vt+0x54 returns the residence itself only for a settlement (vtable `0x0137C9A4`, `0x004470B0`);
///   for a fort (`0x0137D3E4`), a region slot (`0x0137695C`) and their base classes (`0x0137C7F0`,
///   `0x0137C8B4`) it returns 0 (`0x00461180`). The model garrisons armies only in settlements, so
///   the exception never applies here.
/// - **Navies** of `f` docked in one of the region's ports, when the owner is not at war with `f`:
///   undocked, then placed at sea from the port's way out (`0x00A99A60`; ours: the harbour's dock
///   point, else the port slot); unplaced, it stays where it is, undocked. PROVISIONAL: the model
///   links a navy to its port by position ([`CampaignModel::docked_port`]), so one left in place
///   still counts as docked there.
///
/// The move notice (`0x009FB680`, message 0x119, silenced when the army may stay) is not modelled
/// (BACKLOG).
pub fn expel_faction_from_region(m: &CampaignModel, f: FactionId, region: RegionId) -> Vec<ExpelledForce> {
    let Some(reg) = m.world.regions.get(&region) else { return Vec::new() };
    if !m.in_the_game(f) {
        return Vec::new();
    }
    let n = reg.owner;
    let stay = may_stay(m, n, f);
    let at_war = m.at_war(n, f);
    let harbours = m.terrain.as_ref().map_or(&[][..], |t| &t.0.harbours[..]);
    // The exe walks the faction's character list (+0x768, `0x008D9E7A`) in its order, each
    // commander's army or navy in turn (CONFIRMED), so the plan, and the moves the model applies
    // from it, follow that order ([`super::world::World::characters_in_list_order`]).
    let commanded: BTreeMap<CharacterId, &MilitaryForce> =
        m.world.forces.values().filter(|x| x.faction == f).filter_map(|x| Some((x.commander?, x))).collect();
    let mut out = Vec::new();
    for id in m.world.characters_in_list_order() {
        let (Some(force), Some(c)) = (commanded.get(&id), m.world.characters.get(&id)) else { continue };
        if !force.is_navy {
            if m.carrier_of(force.id).is_some() || !super::economy::in_region(m, reg, c) {
                continue;
            }
            let here = (c.position.0.to_f32(), c.position.1.to_f32());
            let to = army_candidates(m, f, reg, here, stay).into_iter().find_map(|(key, p)| place(m, Some(key), p));
            out.push(ExpelledForce { force: force.id, to: to.map(to_fixed) });
            continue;
        }
        if at_war {
            continue;
        }
        let Some((r, slot)) = m.docked_port(force.id).filter(|(r, _)| *r == region) else { continue };
        let Some(port) = m.world.regions.get(&r).and_then(|r| r.slots.get(slot)).and_then(|s| s.position) else { continue };
        let port = (port.0.to_f32(), port.1.to_f32());
        let exit = harbours.iter().find(|h| (h.port.0 - port.0).powi(2) + (h.port.1 - port.1).powi(2) < 1.0).map_or(port, |h| h.dock);
        out.push(ExpelledForce { force: force.id, to: place(m, None, exit).map(to_fixed) });
    }
    out
}

impl CampaignModel {
    /// `f`'s forces leave `region` (`0x008D9DC0`), as the [`super::seams::REGION_EXPULSION`] rule
    /// plans it ([`expel_faction_from_region`] in the original game). An army leaves its residence (its
    /// commander no longer garrisoned, the region's garrison link cleared); a navy leaves its port
    /// ([`Self::undock`]); then each placed force's commander moves ([`Self::move_character`]).
    pub(crate) fn expel_from_region(&mut self, f: FactionId, region: RegionId) {
        let mut writes = ModWrites::default();
        let rules = Arc::clone(&self.rules);
        let plan = rules.seams.region_expulsion.rule()(self, f, region, &mut writes);
        self.mod_state.apply(&mut writes);
        for e in plan {
            let Some(force) = self.world.forces.get(&e.force) else {
                log::warn!("region expulsion from {region:?}: the rule names force {:?}, which does not exist; skipped", e.force);
                continue;
            };
            let (navy, commander) = (force.is_navy, force.commander);
            if navy {
                self.undock(e.force);
            } else {
                if let Some(ch) = commander.and_then(|c| self.world.characters.get_mut(&c)) {
                    ch.garrisoned_in = None;
                }
                for r in self.world.regions.values_mut().filter(|r| r.garrison == Some(e.force)) {
                    r.garrison = None;
                }
            }
            if let (Some(c), Some(to)) = (commander, e.to) {
                self.move_character(c, to);
                self.rebuild_moved_zones(c);
            }
        }
    }

    /// The expulsion at a change of `region`'s owner (`TransferRegionToFaction` `0x00A64AC0` with its
    /// fourth argument 0; CONFIRMED): each faction of the world's list (campaign `+0xF5C`, which holds
    /// no rebels) other than the new owner, with a capital and still in the game, leaves the region
    /// ([`Self::expel_from_region`]). The model's faction ids stand in for the list's order (the
    /// placements do not depend on it: the model keeps no footprints).
    pub(crate) fn expel_after_owner_change(&mut self, region: RegionId) {
        let Some(owner) = self.world.regions.get(&region).map(|r| r.owner) else { return };
        let factions: Vec<FactionId> = self
            .world
            .factions
            .keys()
            .copied()
            .filter(|&f| f != owner && !self.is_rebel_faction(f) && self.world.capital(f).is_some() && self.in_the_game(f))
            .collect();
        for f in factions {
            self.expel_from_region(f, region);
        }
    }

    /// `f`'s forces leave each region `owner` holds ([`Self::expel_from_each_region_of`]), then both
    /// factions' zones of control are flooded again, `f`'s first ([`Self::refresh_faction_zones`],
    /// `RefreshFactionPathGridObstacles` `0x00B795A0`): the loss of standing in another's land that a
    /// broken alliance, a peace, timed access running out and the end of a protectorate tie run
    /// (`0x00B13A30`, `0x00B0CA20`, `0x00B0D0C0`, `0x00B29170`, `0x00B64B10`; CONFIRMED, disassembly: both
    /// refreshes sit inside the expel branch). A cancel (`0x00B67BD0`) runs the loop alone.
    pub(crate) fn expel_from_lands_of(&mut self, f: FactionId, owner: FactionId) {
        self.expel_from_each_region_of(f, owner);
        self.refresh_faction_zones(f);
        self.refresh_faction_zones(owner);
    }

    /// `f`'s forces leave each region `owner` holds ([`Self::expel_from_region`] per region): the loop over
    /// the owner's region list (faction `+0x778`) that every loss of standing in another's land runs
    /// (`0x00B64B10`, `0x00B13A30`, `0x00B0CA20`, `0x00B0D0C0`, `0x00B67BD0`, `0x00B29170`; CONFIRMED).
    /// PROVISIONAL: the exe walks the list in gain order, ours in region-id order (the model keeps no gain
    /// order). The order matters only when a force one step moves stands in a region a later step
    /// expels from: with no footprints in the model, that takes a force that may stay but cannot be
    /// placed where it stands and is sent to another of `owner`'s regions.
    pub(crate) fn expel_from_each_region_of(&mut self, f: FactionId, owner: FactionId) {
        let regions: Vec<RegionId> = self.world.regions.values().filter(|r| r.owner == owner).map(|r| r.id).collect();
        for r in regions {
            self.expel_from_region(f, r);
        }
    }
}

//! A faction's forces leave a region they may not stand in (`ExpelFactionForcesFromRegion`
//! `0x008D9DC0`, CONFIRMED from the disassembly; analysis/fidelity/CAMPAIGN_FIDELITY.md, director
//! section "Transfer and the old owner's forces").
//!
//! Callers: every change of a region's owner (`TransferRegionToFaction` `0x00A64AC0` with its fourth
//! argument 0, which every settlement handover `0x00B449F0` passes: captures, deals, liberations, the
//! director's transfer; [`CampaignModel::expel_after_owner_change`]), the liberation itself, which first expels the
//! capturer's army (`0x00B4F090` at `0x00B508C5`; [`CampaignModel::expel_from_region`]), an accepted deal's region lists
//! and, for a deal with a peace item, both sides' regions (`0x00C114B0`, [`CampaignModel::accept_deal`], the
//! peace pass looping alone as the `CCQ_MOVE_ARMIES_OUT_OF_REGIONS` command does, `0x00936AB0`) and the end of a protectorate tie (`0x00B64B10`,
//! [`CampaignModel::end_protectorate_tie`]) and every loss of military access, over all the giver's
//! regions ([`CampaignModel::expel_from_lands_of`]): timed access running out (`0x00B29170`,
//! [`CampaignModel::diplomacy_round_end`]), a cancel (`0x00B67BD0`,
//! [`CampaignModel::cancel_military_access`]), a broken alliance (`0x00B13A30` / `0x00B0CA20`,
//! [`CampaignModel::break_alliance`]) and a peace (`0x00B0D0C0`, [`CampaignModel::make_peace_rules`])
//! that give back no access. All but the cancel then flood both factions' zones of control again
//! ([`CampaignModel::expel_from_lands_of`]); the cancel loops alone ([`CampaignModel::expel_from_each_region_of`]).
//!
//! The original moves the forces one at a time: each is placed on the map as the moves before it
//! left it. So the rule ([`next_expelled_force`], behind the [`super::seams::REGION_EXPULSION`]
//! seam) names one force and where it goes, the model applies that
//! ([`CampaignModel::expel_from_region`]) and asks again until the rule has no more. Agents are
//! never moved.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::events::{CampaignEvent, CampaignMessage};
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::mod_state::ModWrites;
use super::pathing::{Domain, PathGrid};
use super::place_flood::{self, Point};
use super::polypath::PolyMap;
use super::rtcut::{self, Cut, Shape};
use super::world::{CampaignModel, Character, MilitaryForce, Region, Stance};
use super::zoc::{self, Obstacle};
use crate::fixed::Fixed20;
use crate::msvc_sort;

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
    /// Its faction gets the move notice after the move ([`super::CampaignMessage::YourArmyRetreatsRegion`],
    /// `0x009FB680`): in the original game a placed army that may not stay; never a navy. Ignored when
    /// `to` is `None` (nothing moved).
    pub notice: bool,
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

fn raw(p: (Fixed20, Fixed20)) -> Point {
    (p.0.raw(), p.1.raw())
}

fn from_raw(p: Point) -> (Fixed20, Fixed20) {
    (Fixed20::from_raw(p.0), Fixed20::from_raw(p.1))
}

/// A raw position in map units.
fn units(p: Point) -> (f32, f32) {
    (Fixed20::from_raw(p.0).to_f32(), Fixed20::from_raw(p.1).to_f32())
}

/// The map's path grid and its polygon grid, when the map has both: the ground the original places
/// forces on.
fn polygon_ground(m: &CampaignModel) -> Option<(&PathGrid, &PolyMap)> {
    let g = &*m.terrain.as_ref()?.0;
    Some((g, g.poly.as_ref()?))
}

/// One place a land army is tried at.
#[derive(Clone, Copy)]
struct Candidate<'a> {
    /// The region it would stand in.
    region: &'a Region,
    /// The point the placement starts from (raw Fixed20).
    point: Point,
    /// The region it leaves, at its own position ("may stay").
    stay: bool,
}

/// Where a land army standing at `here` may go when it leaves `region`, in the order they are tried
/// (`0x008C6010` picks the regions, `0x008DCB30` builds and orders the list; CONFIRMED):
/// 1. the region itself at the army's own position, as it is (no conversion), when it may stay;
/// 2. `f`'s other regions, in its list's order (faction `+0x778`,
///    [`super::world::World::regions_in_list_order`]);
/// 3. the regions of factions not at war with `f` whose record lets it stand
///    ([`record_lets_stand`]), not the region, in the region manager's order (world `+0x34` →
///    `+0x10` / `+0x14`, [`super::world::World::regions_in_manager_order`]).
///
/// Each region of 2 and 3 is entered at the point of its settlement area's outlines nearest to the
/// army (`0x008AF750`, [`place_flood::outline_point`]; the army's own position for a region without
/// such an outline). Each of the two lists is then sorted on its own by the squared distance from
/// the army to that point ([`place_flood::distance_key`]) with the library's sort (`0x00868A30`,
/// [`msvc_sort::sort_by`]: equal distances keep their order up to 32 regions and may not above).
///
/// PROVISIONAL: the model has no theatres (as `nearest_capital`), so every region counts and the
/// exe's fourth part, `f`'s capital in another theatre, never happens. That matches every shipped
/// map, each one theatre holding all its regions (CONFIRMED, `economy_check theatres`,
/// CAMPAIGN_FIDELITY.md "Theatres CONFIRMED a no-op"); a map with several theatres needs the exe's
/// theatre filter.
///
/// A map without the polygon grid (the original always has one; ours: a model without the map, or a
/// map given as cells only) has no outlines: there a region's point is the centre of its path-grid
/// cell nearest to the army, or its settlement without the map. Our own rule for such maps.
fn army_candidates<'a>(m: &'a CampaignModel, f: FactionId, region: &'a Region, here: Point, stay: bool) -> Vec<Candidate<'a>> {
    let outlines = polygon_ground(m).map(|_| &m.world.settlement_area_outlines);
    let cells = match (&m.terrain, outlines) {
        (Some(t), None) => {
            let p = units(here);
            Some((&*t.0, t.0.nearest_cell_per_region(p.0, p.1)))
        }
        _ => None,
    };
    let point = |r: &Region| -> Point {
        if let Some(outlines) = outlines {
            return place_flood::outline_point(outlines.get(&r.id).map_or(&[][..], |o| &o[..]), here);
        }
        let cell = cells.as_ref().and_then(|(g, cells)| Some(g.centre(cells.get(g.region_index(&r.key)? as usize).copied().flatten()?)));
        raw(cell.map_or(r.settlement.position, to_fixed))
    };
    let sorted = |ids: Vec<RegionId>, fits: &dyn Fn(&Region) -> bool| {
        let mut list: Vec<(&'a Region, Point, f32)> = ids
            .into_iter()
            .filter_map(|id| m.world.regions.get(&id))
            .filter(|r| r.id != region.id && fits(r))
            .map(|r| {
                let p = point(r);
                (r, p, place_flood::distance_key(here, p))
            })
            .collect();
        msvc_sort::sort_by(&mut list, |a, b| a.2 < b.2);
        list
    };
    let own = sorted(m.world.faction_regions_in_list_order(f), &|_| true);
    let open = sorted(m.world.regions_in_manager_order(), &|r| r.owner != f && !m.at_war(r.owner, f) && record_lets_stand(m, r.owner, f));
    let mut out = Vec::with_capacity(own.len() + open.len() + 1);
    if stay {
        out.push(Candidate { region, point: here, stay: true });
    }
    out.extend(own.into_iter().chain(open).map(|(region, point, _)| Candidate { region, point, stay: false }));
    out
}

/// Places the army `mover` commands at a candidate, on the polygon grid as its faction sees it: the
/// flood `0x00B35640` ([`place_flood::flood`]) from the candidate's point over the candidate
/// region's own land, within [`PLACEMENT_RADIUS`]. `others` are the obstacles the faction sees, the
/// mover left out ([`CampaignModel::obstacles_with_stored_zones`]).
///
/// CONFIRMED: the flood and what the view holds (below). Not covered by that: a position next to a
/// core or a zone depends on the pieces [`rtcut`] cuts the cell's polygons into, which that module
/// tags as not matched to the original's clipper (its module doc and its split), and the order of
/// the polygons inside a rebuilt cell, which decides between equally near positions, was not
/// compared with the original's.
///
/// The view ([`zoc::view_cuts`], each layer prepared for the faction by `0x00B06280`): the stored
/// zones of factions at war with it are no place (kind 8), nor is the core of any commander it sees
/// (kind 9). For another region the mover's own core counts too, where he still stands
/// (`0x00B35570`); for the region he leaves, his own footprint is lifted first (`0x00B37670`:
/// `0x00AF59B0` / `0x00B0C830`, put back by `0x00B66720`), so he may be put where he is.
///
/// Obstacles too far from the point to touch a cell the flood can scan are left out (that changes
/// nothing): it scans no cell at [`PLACEMENT_RADIUS`] or more from the point on either axis, and a
/// cell's polygons lie in the cell. A region the grid does not hold places nothing.
fn place_on_polygons(g: &PathGrid, map: &PolyMap, others: &[Obstacle], mover: &Character, cand: &Candidate<'_>) -> Option<Point> {
    let region = g.region_index(&cand.region.key)?;
    let at = units(cand.point);
    let reach = (PLACEMENT_RADIUS + map.cell) * std::f32::consts::SQRT_2;
    let mut cuts = zoc::view_cuts(map, others, |o, r| {
        let r = reach + r;
        (o.pos.0 - at.0).powi(2) + (o.pos.1 - at.1).powi(2) < r * r
    });
    if !cand.stay {
        let core = zoc::core_shape(units(raw(mover.position)), mover.garrisoned_in.is_some());
        cuts.push(Cut { shape: Shape::Convex(core), kind: zoc::CORE_KIND });
    }
    let overlay = rtcut::build(map, &cuts);
    place_flood::flood(&map.with(&overlay), cand.point, region, PLACEMENT_RADIUS)
}

/// Places a force near `point` on the path grid's cells: in the region `key` on land, or at sea when
/// `key` is `None`. The point itself when its cell fits, else the nearest fitting cell's centre
/// closer than [`PLACEMENT_RADIUS`] ([`super::pathing::PathGrid::nearest_place_within`]); without
/// the map, a land placement is the point itself and a sea one fails.
///
/// For a land army this is our own rule for a map without the polygon grid ([`army_candidates`]);
/// for a navy it stands in for the original's flood at sea ([`navy_step`]). Other characters' cores
/// do not count here.
fn place_on_cells(m: &CampaignModel, key: Option<&str>, point: Point) -> Option<Point> {
    let Some(t) = &m.terrain else { return key.map(|_| point) };
    let g = &t.0;
    let p = units(point);
    let q = match key {
        Some(k) => g.nearest_place_within(p.0, p.1, Some(g.region_index(k)?), Domain::Land, PLACEMENT_RADIUS),
        None => g.nearest_place_within(p.0, p.1, None, Domain::Sea, PLACEMENT_RADIUS),
    }?;
    Some(if q == p { point } else { raw(to_fixed(q)) })
}

/// The move of the land army `force` under commander `c`, if it stands in `reg` (the army branch of
/// `0x008D9DC0`, CONFIRMED).
///
/// - The army counts when the path-grid polygon under its commander's position is the region's
///   (`0x00A075B0` → `0x00B4D7B0` → `0x00B4D800`, [`place_flood::in_region`]). For a garrisoned
///   commander the original tests his residence's position record instead of his own (`0x00A17D60`,
///   the residence case); ours reads the model's garrison link, which gives the same region because
///   the model keeps a garrisoned commander at his settlement. On a map without the polygon grid:
///   [`super::economy::in_region`].
/// - An army aboard a navy never counts. INFERRED: the original tests it at the stored position of
///   the commander of the fleet carrying it (`0x00A17D60`, the case at `0x00A17E05`: army `+0xEC`,
///   the fleet, then its `+0x74` commander's position record), with the same polygon test. At sea
///   no polygon there is the land region's. Only a docked navy's own commander is given the port's
///   way out to sea by `0x00A17D60` (`0x00A99A60`); an army aboard is not. Not traced: where the
///   record of a fleet docked in one of the region's ports lies, and so whether the polygon under
///   it can name the region.
/// - The candidates ([`army_candidates`]) are tried in order, while the army still stands in its
///   residence, until one places it ([`place_on_polygons`]). `None` placed: it does not move.
/// - It gets the move notice when it was placed and may not stay (the test at `0x008DA077`).
fn army_step(m: &CampaignModel, f: FactionId, reg: &Region, stay: bool, force: &MilitaryForce, c: &Character) -> Option<ExpelledForce> {
    if m.carrier_of(force.id).is_some() {
        return None;
    }
    let here = raw(c.position);
    let ground = polygon_ground(m);
    let inside = match ground {
        Some((g, map)) => c.garrisoned_in == Some(reg.id) || g.region_index(&reg.key).is_some_and(|i| place_flood::in_region(map, here, i)),
        None => super::economy::in_region(m, reg, c),
    };
    if !inside {
        return None;
    }
    let candidates = army_candidates(m, f, reg, here, stay);
    let to = match ground {
        Some((g, map)) => {
            let others = m.obstacles_with_stored_zones(f, c.id);
            candidates.iter().find_map(|cand| place_on_polygons(g, map, &others, c, cand))
        }
        None => candidates.iter().find_map(|cand| place_on_cells(m, Some(&cand.region.key), cand.point)),
    };
    Some(ExpelledForce { force: force.id, to: to.map(from_raw), notice: to.is_some() && !stay })
}

/// The move of the navy `force`, if it is docked in one of `reg`'s ports (the navy branch of
/// `0x008D9DC0`, CONFIRMED): undocked, then placed at sea from the port's way out; unplaced, it stays
/// where it is, undocked. Never a notice (the branch has no call).
///
/// PROVISIONAL, the placement: the original takes the port's way out from `0x00A99A60` (a virtual
/// call on the port's `+0x70` object, not traced) and floods the polygon grid at sea from it
/// (`0x00B377E0`, the flood's mode 2, within [`PLACEMENT_RADIUS`]; not ported). Ours: the harbour's
/// dock point, else the port slot, placed on the path grid's cells ([`place_on_cells`]).
/// PROVISIONAL, the link: the model ties a navy to its port by position
/// ([`CampaignModel::docked_port`]), so one left in place still counts as docked there.
fn navy_step(m: &CampaignModel, reg: &Region, force: &MilitaryForce) -> Option<ExpelledForce> {
    let (r, slot) = m.docked_port(force.id).filter(|(r, _)| *r == reg.id)?;
    let port = m.world.regions.get(&r)?.slots.get(slot)?.position?;
    let port = (port.0.to_f32(), port.1.to_f32());
    let harbours = m.terrain.as_ref().map_or(&[][..], |t| &t.0.harbours[..]);
    let exit = harbours.iter().find(|h| (h.port.0 - port.0).powi(2) + (h.port.1 - port.1).powi(2) < 1.0).map_or(port, |h| h.dock);
    Some(ExpelledForce { force: force.id, to: place_on_cells(m, None, raw(to_fixed(exit))).map(from_raw), notice: false })
}

/// The original's expulsion of `f`'s forces from `region` (`0x008D9DC0`, CONFIRMED), one force at a
/// time: given the forces this expulsion has handled so far (`done`, in order), the next force to
/// move and where to, or `None` when it is over. The model applies each move before it asks again
/// ([`CampaignModel::expel_from_region`]), so each force is placed on the map the moves before it
/// left: the original places each (`0x0096C8D0`) before it tests the next. The default
/// [`super::seams::RegionExpulsion`].
///
/// Nothing moves when `f` is out of the game (faction `+0x824`, the early return at `0x008D9DDC`;
/// CONFIRMED), whichever caller asks. The forces come in the order of `f`'s character list (+0x768,
/// [`super::world::World::characters_in_list_order`]), each commander's army or navy in turn, as the
/// exe's one loop takes them (CONFIRMED, `0x008D9E7A`); the loop never goes back, so the walk goes
/// on behind the commander of the force handled last.
///
/// - **Land armies** of `f` standing in the region: [`army_step`].
/// - Placed or not, an army leaves its residence (vt+0x90; for a settlement `0x00B0D610` unlinks
///   the garrison), whether or not it may stay (CONFIRMED, `0x008D9DC0`). The exe's one exception:
///   the owner at war with `f` and the residence's vt+0x54 returning 0 leaves the army untouched.
///   vt+0x54 returns the residence itself only for a settlement (vtable `0x0137C9A4`, `0x004470B0`);
///   for a fort (`0x0137D3E4`), a region slot (`0x0137695C`) and their base classes (`0x0137C7F0`,
///   `0x0137C8B4`) it returns 0 (`0x00461180`). The model garrisons armies only in settlements, so
///   the exception never applies here.
/// - **Navies** of `f` docked in one of the region's ports, when the owner is not at war with `f`:
///   [`navy_step`].
/// - **The move notice** (`SendArmyRetreatsRegionMessage` `0x009FB680`, CONFIRMED from the
///   disassembly): a land army that a candidate placed gets it ([`ExpelledForce::notice`]) unless it
///   may stay (the test at `0x008DA077`, after the move `0x0096C8D0`); an unplaced army and a navy never do (the navy branch has no call).
pub fn next_expelled_force(m: &CampaignModel, f: FactionId, region: RegionId, done: &[ForceId]) -> Option<ExpelledForce> {
    let reg = m.world.regions.get(&region)?;
    if !m.in_the_game(f) {
        return None;
    }
    let n = reg.owner;
    let stay = may_stay(m, n, f);
    let at_war = m.at_war(n, f);
    let commanded: BTreeMap<CharacterId, &MilitaryForce> =
        m.world.forces.values().filter(|x| x.faction == f).filter_map(|x| Some((x.commander?, x))).collect();
    let order = m.world.characters_in_list_order();
    let last = done.last().and_then(|d| m.world.forces.get(d)).and_then(|x| x.commander);
    let start = last.and_then(|c| order.iter().position(|&id| id == c)).map_or(0, |i| i + 1);
    for id in &order[start..] {
        let (Some(force), Some(c)) = (commanded.get(id), m.world.characters.get(id)) else { continue };
        if done.contains(&force.id) {
            continue;
        }
        let step = if !force.is_navy {
            army_step(m, f, reg, stay, force, c)
        } else if at_war {
            None
        } else {
            navy_step(m, reg, force)
        };
        if step.is_some() {
            return step;
        }
    }
    None
}

impl CampaignModel {
    /// `f`'s forces leave `region` (`0x008D9DC0`), one at a time as the
    /// [`super::seams::REGION_EXPULSION`] rule names them ([`next_expelled_force`] in the original
    /// game): the rule is asked for the next force, its move is applied, and the rule is asked again
    /// with the forces handled so far, until it has none. An army leaves its residence (its
    /// commander no longer garrisoned, the region's garrison link cleared); a navy leaves its port
    /// ([`Self::undock`]); then a placed force's commander is placed ([`Self::place_character`], which
    /// rebuilds the zones the move touches) and, when the rule says so ([`ExpelledForce::notice`]),
    /// his faction (character `+0x2A0`) gets the move notice in `events` at once, before the next
    /// force moves (`0x009FB680` at `0x008DA091`, right after the move `0x0096C8D0`;
    /// [`CampaignMessage::YourArmyRetreatsRegion`]).
    ///
    /// A rule that names a force twice, or one that does not exist, ends the expulsion there (logged).
    pub(crate) fn expel_from_region(&mut self, f: FactionId, region: RegionId, events: &mut Vec<CampaignEvent>) {
        let rules = Arc::clone(&self.rules);
        let mut done: Vec<ForceId> = Vec::new();
        loop {
            let mut writes = ModWrites::default();
            let next = rules.seams.region_expulsion.rule()(self, f, region, &done, &mut writes);
            self.mod_state.apply(&mut writes);
            let Some(e) = next else { break };
            if done.contains(&e.force) {
                log::warn!("region expulsion from {region:?}: the rule names force {:?} a second time; the expulsion ends here", e.force);
                break;
            }
            let Some(force) = self.world.forces.get(&e.force) else {
                log::warn!("region expulsion from {region:?}: the rule names force {:?}, which does not exist; the expulsion ends here", e.force);
                break;
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
                // Placed by `0x0096C8D0` (`0x008DA072` army, `0x008DA1B2` navy, CONFIRMED by disassembly).
                self.place_character(c, to);
                if e.notice
                    && let Some(ch) = self.world.characters.get(&c)
                {
                    let message = CampaignMessage::YourArmyRetreatsRegion { character: c, at: ch.position };
                    events.push(CampaignEvent::Message { recipient: ch.faction, message });
                }
            }
            done.push(e.force);
        }
    }

    /// The expulsion at a change of `region`'s owner (`TransferRegionToFaction` `0x00A64AC0` with its
    /// fourth argument 0; CONFIRMED): each faction of the world's list (campaign `+0xF5C`, which holds
    /// no rebels) other than the new owner, with a capital and still in the game, leaves the region
    /// ([`Self::expel_from_region`]), in the list's order ([`super::world::World::factions_in_turn_order`]):
    /// it decides who is placed first when two factions' armies leave for the same spot.
    pub(crate) fn expel_after_owner_change(&mut self, region: RegionId, events: &mut Vec<CampaignEvent>) {
        let Some(owner) = self.world.regions.get(&region).map(|r| r.owner) else { return };
        let mut factions = self.world.factions_in_turn_order();
        factions.retain(|&f| f != owner && !self.is_rebel_faction(f) && self.world.capital(f).is_some() && self.in_the_game(f));
        for f in factions {
            self.expel_from_region(f, region, events);
        }
    }

    /// `f`'s forces leave each region `owner` holds ([`Self::expel_from_each_region_of`]), then both
    /// factions' zones of control are flooded again, `f`'s first ([`Self::refresh_faction_zones`],
    /// `RefreshFactionPathGridObstacles` `0x00B795A0`): the loss of standing in another's land that a
    /// broken alliance, a peace, timed access running out and the end of a protectorate tie run
    /// (`0x00B13A30`, `0x00B0CA20`, `0x00B0D0C0`, `0x00B29170`, `0x00B64B10`; CONFIRMED, disassembly: both
    /// refreshes sit inside the expel branch). A cancel (`0x00B67BD0`) runs the loop alone.
    pub(crate) fn expel_from_lands_of(&mut self, f: FactionId, owner: FactionId, events: &mut Vec<CampaignEvent>) {
        self.expel_from_each_region_of(f, owner, events);
        self.refresh_faction_zones(f);
        self.refresh_faction_zones(owner);
    }

    /// `f`'s forces leave each region `owner` holds ([`Self::expel_from_region`] per region): the loop over
    /// the owner's region list (faction `+0x778`) that every loss of standing in another's land runs
    /// (`0x00B64B10`, `0x00B13A30`, `0x00B0CA20`, `0x00B0D0C0`, `0x00B67BD0`, `0x00B29170`, and a peace deal's
    /// pass in `0x00C114B0`; CONFIRMED).
    /// The list is walked in its order ([`super::world::World::regions_in_list_order`]; CONFIRMED: the
    /// manager's order after a load, each gain appended at the end, a loss removed in place; see
    /// [`super::world::World::region_order`]). The order matters when a force one step moves stands in
    /// a region a later step expels from: a force that may stay there but finds its place taken (another
    /// commander's core, an enemy zone) is sent on to another of `owner`'s regions.
    pub(crate) fn expel_from_each_region_of(&mut self, f: FactionId, owner: FactionId, events: &mut Vec<CampaignEvent>) {
        for r in self.world.faction_regions_in_list_order(owner) {
            self.expel_from_region(f, r, events);
        }
    }

    /// The `CCQ_MOVE_ARMIES_OUT_OF_REGIONS` command (`HandleMoveArmiesOutOfRegionsCommand` `0x00936AB0`,
    /// CONFIRMED): both factions exist; `b`'s forces leave each of `a`'s regions, then `a`'s forces each of
    /// `b`'s ([`Self::expel_from_each_region_of`], `0x008D9DC0` per region of the owner's list). No stance,
    /// same-faction or turn test, and no zone flood after the loops (each move rebuilds the zones it
    /// touches, [`Self::expel_from_region`]); a faction out of the game moves nothing (`0x008D9DC0`).
    /// Returns the move notices, in the order the moves happened.
    pub fn move_armies_out_of_regions(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, super::CommandError> {
        if let Some(&f) = [a, b].iter().find(|f| !self.world.factions.contains_key(f)) {
            return Err(super::CommandError::UnknownFaction(f));
        }
        let mut events = Vec::new();
        self.expel_from_each_region_of(b, a, &mut events);
        self.expel_from_each_region_of(a, b, &mut events);
        Ok(events)
    }
}

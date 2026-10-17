//! Whether two factions can trade, and what a region changing hands does to the trade routes
//! through it (CAMPAIGN_FIDELITY.md §"Route loss"). CONFIRMED structure unless a line says otherwise.
//!
//! - [`trade_route_check`] (`0x00BA42E0`): the original's check that `a` can send a route to `b`,
//!   0 when it cannot, else a code 1..=7 naming the kind of route (land, sea, capital port, or a swap
//!   that frees a sea or capital port route). The model calls it through the
//!   [`trade.route_check`](super::seams::TRADE_ROUTE_CHECK) seam ([`CampaignModel::trade_route_check`]).
//! - [`CampaignModel::trade_path_exists`] (`FindTradePathBetweenCapitals 0x00BB1260` with the expansion `GetTradePathNeighbourNodes 0x00BB0240`): whether a
//!   path exists from `a`'s capital to `b`'s, by land only or with sea hops between export ports.
//! - [`CampaignModel::trade_routes_after_region_lost`] (`0x00BBE1A0`, from the owner change
//!   `0x00A64E87`): the routes through the region are dropped and each pair is checked again: a pair
//!   that can no longer trade has its agreement broken both ways ([`CampaignModel::break_trade_both_ways`]),
//!   the others get new routes.
//!
//! The original caches each pair's paths (the faction's pair map) and refreshes the entries as routes
//! change (`0x00BBDBA0`, step 2 of `0x00BBE1A0`); the model searches fresh each time. Existence of a
//! path does not depend on the cache's search costs, so the answers agree where the cache was refreshed
//! with `0x00BBDBA0` semantics; the swap codes can read pair entries that step 2 of `0x00BBE1A0` refreshed
//! with the cost-dependent path split (INFERRED to agree in practice; BACKLOG follow-up).

use std::collections::BTreeSet;

use super::ids::{FactionId, RegionId};
use super::world::{CampaignModel, Region};

/// A trade path search's network (`0x00BB1260`'s network argument).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeNetwork {
    /// Land only (the pair map's land entry, +0x1AF8 network).
    Land,
    /// Land and sea hops between export ports (`g_dwTradePathTypeLandLeg 0x015F1674` network).
    Sea,
    /// Land and sea hops, for a route out of the capital's port (`g_dwTradePathTypeSeaLeg 0x015F1670` network). It
    /// reaches exactly the regions [`TradeNetwork::Sea`] does: the two differ only in the start node's sea flag and
    /// in which of `a`'s two capital nodes is barred (`0x00BB0240`), the one that is not the start node either way.
    CapitalPort,
}

/// `a` can send a trade route to `b` (`0x00BA42E0`): 0 when it cannot, else the kind of route:
/// - 1: a land path ([`TradeNetwork::Land`]);
/// - 3: a capital port path while `a` has fewer capital port routes than its cap
///   ([`CampaignModel::capital_port_route_cap`]);
/// - 2: a sea path while `a` has fewer sea routes than its cap ([`CampaignModel::sea_route_cap`]);
/// - with routes at their caps, a swap: one of `a`'s open sea routes (not blockaded,
///   [`CampaignModel::trade_path_blockaded`]) whose importer `a` also reaches by land frees its slot:
///   4 (a sea route, and `a` reaches `b` by sea) or 6 (a capital port route, and `a` reaches `b`
///   through the capital port); else, with the capital port routes not full (or freeable) and the sea
///   routes full, 5: an open route not from the capital port whose importer the capital port reaches
///   while `b` is reached by sea; else, with the sea routes not full (or freeable) and the capital port
///   routes full, 7: an open capital port route whose importer is reached by sea while `b` is reached
///   through the capital port.
///
/// Either side without a capital: 0. The counts: capital port routes are `a`'s routes flagged
/// [`super::trade::TradePath::capital_port`]; sea routes the others with a sea hop.
///
/// Since [`TradeNetwork::Sea`] and [`TradeNetwork::CapitalPort`] reach the same regions here, 5 and 7 never
/// come back from the model (each needs one search to reach `b` where the other did not); in the original
/// they can, through cached pair entries whose cost-dependent paths differ (see the module docs).
pub fn trade_route_check(m: &CampaignModel, a: FactionId, b: FactionId) -> u8 {
    let w = &m.world;
    if w.capital(a).is_none() || w.capital(b).is_none() {
        return 0;
    }
    if m.trade_path_exists(a, b, TradeNetwork::Land) {
        return 1;
    }
    let (cap_count, sea_count) = m.trade_route_counts(a);
    // The original compares the counts with the caps unsigned (`JC`/`JNC` at `0x00BA4439`, `0x00BA4485`,
    // `0x00BA461F`, `0x00BA4718`): a negative cap (a mod's negative bonus) is never reached.
    let (cap_cap, sea_cap) = (m.capital_port_route_cap(a) as u32, m.sea_route_cap(a) as u32);
    let to_b = |net| m.trade_path_exists(a, b, net);
    if cap_count < cap_cap && to_b(TradeNetwork::CapitalPort) {
        return 3;
    }
    if sea_count < sea_cap && to_b(TradeNetwork::Sea) {
        return 2;
    }
    if cap_count + sea_count == 0 {
        return 0;
    }
    let routes = || w.trade_paths.iter().filter(|((x, _), _)| *x == a).flat_map(|((_, imp), ps)| ps.iter().map(move |p| (*imp, p))).filter(|(_, p)| !m.trade_path_blockaded(a, p));
    let (mut sea_freeable, mut cap_freeable) = (false, false);
    for (importer, p) in routes().filter(|(_, p)| p.uses_sea()) {
        if !m.trade_path_exists(a, importer, TradeNetwork::Land) {
            continue;
        }
        if !p.capital_port {
            if to_b(TradeNetwork::Sea) {
                return 4;
            }
            sea_freeable = true;
        } else {
            if to_b(TradeNetwork::CapitalPort) {
                return 6;
            }
            cap_freeable = true;
        }
    }
    if (cap_count < cap_cap || cap_freeable) && sea_count == sea_cap {
        for (importer, _) in routes().filter(|(_, p)| !p.capital_port) {
            if m.trade_path_exists(a, importer, TradeNetwork::CapitalPort) && to_b(TradeNetwork::Sea) {
                return 5;
            }
        }
    }
    if (sea_count < sea_cap || sea_freeable) && cap_count == cap_cap {
        for (importer, _) in routes().filter(|(_, p)| p.capital_port) {
            if m.trade_path_exists(a, importer, TradeNetwork::Sea) && to_b(TradeNetwork::CapitalPort) {
                return 7;
            }
        }
    }
    0
}

impl CampaignModel {
    /// [`trade_route_check`] through the [`trade.route_check`](super::seams::TRADE_ROUTE_CHECK) seam.
    pub fn trade_route_check(&self, a: FactionId, b: FactionId) -> u8 {
        self.rules.seams.trade_route_check.rule()(self, a, b)
    }

    /// The region has an export port (`HasRegionExportPort 0x00A6CA70`): one of its slots ([`Region::slots`]; not the road
    /// or the walls) has a building whose compiled `commodity_export_vol` bonus (effect 2, `0x00A797B0`;
    /// from `export_capacity_port` in vanilla) is not 0. The building level's own effect: the slot's holder
    /// and the building's health are not read.
    pub fn region_has_export_port(&self, region: &Region) -> bool {
        region.slots.iter().filter_map(|s| s.building.as_ref()).any(|b| self.building_local_bonus(&b.level_key, "commodity_export_vol") != 0.0)
    }

    /// The region's export port is full (`IsTradePortFull 0x00BB4F80`): its [`Self::region_trade_route_cap`], as an
    /// unsigned count, less one for each of the owner's routes whose first sea hop (the first waypoint
    /// with its sea flag set and a region) leaves from this region, is 0. The count wraps below 0 in the
    /// original (a port with more routes than its cap is not full): kept. An owner with no entry in the
    /// trade manager's exporter map (`+0x11C`, looked up with `0x00BB0980`) is never full, even with a cap
    /// of 0: here, an owner exporting no route (the rebels among them). PROVISIONAL (BACKLOG): the original
    /// never erases an entry once made (`0x00B9E480` inserts, no erase seen) and its supply split gives every
    /// world faction one (`0x00BC26D0`, operator[] at `0x00BC3699`), so there a 0-cap port of an owner with no
    /// routes reads full; here it does not. Vanilla's export ports have caps of 3-5, so only a mod sees it.
    pub fn trade_port_full(&self, region: &Region) -> bool {
        if !self.world.trade_paths.keys().any(|(x, _)| *x == region.owner) {
            return false;
        }
        let mut left = self.region_trade_route_cap(region) as u32;
        for p in self.world.trade_paths.iter().filter(|((x, _), _)| *x == region.owner).flat_map(|(_, ps)| ps) {
            if p.waypoints.iter().find(|w| w.sea && w.region != RegionId(0)).is_some_and(|w| w.region == region.id) {
                left = left.wrapping_sub(1);
            }
        }
        left == 0
    }

    /// The faction's (capital port, sea) route counts (`0x00BA42E0`): its routes flagged
    /// [`super::trade::TradePath::capital_port`], and its other routes with a sea hop.
    pub fn trade_route_counts(&self, faction: FactionId) -> (u32, u32) {
        let (mut cap, mut sea) = (0, 0);
        for p in self.world.trade_paths.iter().filter(|((x, _), _)| *x == faction).flat_map(|(_, ps)| ps) {
            if p.capital_port {
                cap += 1;
            } else if p.uses_sea() {
                sea += 1;
            }
        }
        (cap, sea)
    }

    /// A path exists from `a`'s capital to `b`'s on the network (`0x00BB1260`, its neighbours
    /// `0x00BB0240`). From each region reached (starting at `a`'s capital, which is never re-entered):
    /// - its land neighbours ([`super::World::region_neighbours`]) owned by `a` or `b`;
    /// - on [`TradeNetwork::Sea`] / [`TradeNetwork::CapitalPort`], from a region with an export port
    ///   ([`Self::region_has_export_port`]): every other region with an export port owned by `a`, or owned
    ///   by `b` and not `b`'s capital, whose port is not full ([`Self::trade_port_full`]) and that lies
    ///   closer than `trade_route_internat_sea_length_limit` (1e8 in vanilla: every port).
    ///
    /// The search reaches `b`'s capital only over land. The distance is between the regions' settlement
    /// positions (INFERRED: the original reads the region key's position, +0x3C / +0x40; moot with the
    /// vanilla limit). Either side without a capital: no path.
    pub fn trade_path_exists(&self, a: FactionId, b: FactionId, net: TradeNetwork) -> bool {
        let w = &self.world;
        let (Some(start), Some(goal)) = (w.capital(a), w.capital(b)) else { return false };
        if start == goal {
            return false;
        }
        // The sea hops' targets, the same from every region: the ports of a and b that have room.
        let ports: Vec<&Region> = if net == TradeNetwork::Land {
            Vec::new()
        } else {
            w.regions
                .values()
                .filter(|r| (r.owner == a || (r.owner == b && r.id != goal)) && self.region_has_export_port(r) && !self.trade_port_full(r))
                .collect()
        };
        let limit = f64::from(self.rules.var("trade_route_internat_sea_length_limit", f32::INFINITY));
        let pos = |r: &Region| (r.settlement.position.0.to_f64(), r.settlement.position.1.to_f64());
        let mut seen = BTreeSet::from([start]);
        let mut queue = vec![start];
        let mut i = 0;
        while i < queue.len() {
            let at = queue[i];
            i += 1;
            let Some(here) = w.regions.get(&at) else { continue };
            for n in w.region_neighbours.get(&at).map(Vec::as_slice).unwrap_or_default() {
                if w.regions.get(n).is_some_and(|r| r.owner == a || r.owner == b) && seen.insert(*n) {
                    if *n == goal {
                        return true;
                    }
                    queue.push(*n);
                }
            }
            if ports.is_empty() || !self.region_has_export_port(here) {
                continue;
            }
            let (x, y) = pos(here);
            for t in &ports {
                let (tx, ty) = pos(t);
                if t.id != at && ((tx - x).powi(2) + (ty - y).powi(2)).sqrt() < limit && seen.insert(t.id) {
                    queue.push(t.id);
                }
            }
        }
        false
    }

    /// The trade routes through `region` after it changed hands from `old` (`0x00BBE1A0`, called by
    /// the owner change `0x00A64E87` after the capital moved, when `old` is not the rebels):
    /// 1. while a route has a waypoint in `region` (exporters, then their routes, in order; INFERRED:
    ///    the original walks its exporter hash map, whose order the model cannot reproduce), its pair
    ///    loses one route each way ([`Self::remove_first_trade_route`] both ways): the partner is `old`
    ///    when the route ends in `region`, else the route's importer; the pair is remembered;
    /// 2. each remembered pair in order: when either side can no longer reach the other
    ///    ([`Self::trade_route_check`] 0 either way), the agreement is broken both ways
    ///    ([`Self::break_trade_both_ways`]) if both are in the game and `a` still trades with `b`;
    ///    otherwise each side gets a new route (`0x00BA0490`; the model builds it when the trade is valued,
    ///    [`Self::trade_routes_of`], PROVISIONAL).
    ///
    /// ORIGINAL BUG (`0x00BBE1A0`, the rescan loop): when neither direction has a route to remove (a route
    /// ending in `region` whose importer is not `old`, as with a route to a faction governing a region
    /// another faction owns), the route stays and the scan finds it again forever: the game hangs. Here
    /// the found route itself is removed and its own pair remembered.
    ///
    /// Not modelled: the original's listener when `old` has no capital, its pair-cache refresh
    /// (`0x00BBDBA0`, `0x00BAC0D0`; the model searches fresh) and the price update `0x00BCB020` (the
    /// model runs it only at the round start, flag 0, and the round end, flag 1, not on a region loss;
    /// BACKLOG follow-up).
    pub(crate) fn trade_routes_after_region_lost(&mut self, region: RegionId, old: FactionId) {
        if region == RegionId(0) {
            return;
        }
        let mut pairs: Vec<(FactionId, FactionId)> = Vec::new();
        loop {
            let found = self.world.trade_paths.iter().find_map(|(&(exporter, importer), ps)| {
                ps.iter().position(|p| p.waypoints.iter().any(|w| w.region == region)).map(|i| (exporter, importer, i, ps[i].waypoints.last().map(|w| w.region)))
            });
            let Some((exporter, importer, index, last)) = found else { break };
            let partner = if last == Some(region) { old } else { importer };
            let removed = self.remove_first_trade_route(exporter, partner);
            let removed_back = self.remove_first_trade_route(partner, exporter);
            if removed || removed_back {
                pairs.push((exporter, partner));
            } else {
                if let Some(ps) = self.world.trade_paths.get_mut(&(exporter, importer)) {
                    ps.remove(index);
                    if ps.is_empty() {
                        self.world.trade_paths.remove(&(exporter, importer));
                    }
                }
                log::debug!("trade route of {exporter:?} to {importer:?} through {region:?} removed directly: the original hangs here (0x00BBE1A0)");
                pairs.push((exporter, importer));
            }
        }
        for (a, b) in pairs {
            if self.trade_route_check(a, b) == 0 || self.trade_route_check(b, a) == 0 {
                let trading = self.world.relationships.get(&(a, b)).is_some_and(|r| r.trade_agreement);
                if self.in_the_game(a) && self.in_the_game(b) && trading {
                    self.break_trade_both_ways(a, b);
                }
            }
            // Else the original builds both routes again (`0x00BA0490(a, b, 0)` both ways). The model has no
            // port of that builder: a pair without a stored route gets one built when its trade is valued
            // ([`Self::trade_routes_of`], PROVISIONAL), so nothing is stored here.
        }
    }

    /// Removes the exporter's first route to `importer` (`0x00BC02B0` for the route loss, `0x00BC0120` for the
    /// agreement breaks; it then refreshes the exporter's pair cache, `0x00BBDBA0`, which the model does not
    /// keep). False when it has none. "First" is the oldest: both walk the exporter's route vector from index
    /// 0, routes are appended in load (`0x00B89270`) or build (`0x00BA0490`) order, and the erase
    /// (`0x00BAC2A0`) keeps the order of the rest, as the pair's `Vec` does. The exe's lookup (operator[]
    /// `0x00B9E480`) also inserts an empty exporter entry; not modelled. It is observable only through the
    /// [`Self::trade_port_full`] gap: an empty entry counts as no routes
    /// everywhere, the supply split gives every world faction an entry anyway (`0x00BC26D0`, operator[] at
    /// `0x00BC3699`; the price update `0x00BCB020` that `0x00B29BB0` calls right after its break reaches it on
    /// every path, at `0x00BCC52F`), and the only reader that tells "empty" from "none",
    /// [`Self::trade_port_full`], differs only for an export port with a route cap of 0 (vanilla's are 3-5).
    /// CONFIRMED. The removed route's accumulated value goes with it ([`TradePath::accumulated`](super::trade::TradePath::accumulated): the
    /// erase destroys that record, `0x00B981D0`); the pair's other routes keep theirs.
    ///
    /// A pair with no stored route has its one route built when valued ([`Self::trade_routes_of`]); the
    /// original stores that route (`0x00BA0490`) and this erase would drop it with its value, so the pair's
    /// built-route value ([`World::trade_accumulated`](super::World::trade_accumulated)) is dropped here
    /// too, and the next built route starts from 0 as a rebuilt one does. Still false then: no stored
    /// route was removed (the region-loss scan, which reads it, walks stored routes only).
    pub(crate) fn remove_first_trade_route(&mut self, exporter: FactionId, importer: FactionId) -> bool {
        let removed = match self.world.trade_paths.get_mut(&(exporter, importer)) {
            Some(ps) if !ps.is_empty() => {
                ps.remove(0);
                if ps.is_empty() {
                    self.world.trade_paths.remove(&(exporter, importer));
                }
                true
            }
            _ => false,
        };
        if self.world.trade_paths.get(&(exporter, importer)).is_none_or(Vec::is_empty) {
            self.world.trade_accumulated.remove(&(exporter, importer));
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::tests::{edit_test_building_effects, test_model};
    use super::super::trade::{TradePath, TradeWaypoint};
    use super::super::treaties::{event, slot};
    use super::*;

    const A: FactionId = FactionId(1);
    const B: FactionId = FactionId(2);
    const C: FactionId = FactionId(3);

    /// A MADE-UP map: A holds 10 (capital) and 13, B holds 11 (capital) and 12, C holds 14 (capital).
    /// Land links 10–13, 13–14, 11–12; every region has one building that is an export port with room
    /// for one route (so 10's capital port and 13's sea cap are 1 each).
    fn trade_map() -> CampaignModel {
        let mut m = test_model();
        edit_test_building_effects(&mut m, |e| e.extend([("trade_routes_mod_max_sea".into(), 1.0), ("export_capacity_port".into(), 30.0)]));
        let template = m.world.regions[&RegionId(10)].clone();
        for (id, owner) in [(12, B), (13, A), (14, C)] {
            let mut r = template.clone();
            r.id = RegionId(id);
            r.key = format!("test_region_{id}");
            r.owner = owner;
            m.world.regions.insert(r.id, r);
        }
        for (f, cap) in [(A, 10), (B, 11), (C, 14)] {
            m.world.faction_details.entry(f).or_default().capital = Some(RegionId(cap));
        }
        for (x, y) in [(10, 13), (13, 14), (11, 12)] {
            link(&mut m, x, y);
        }
        m
    }

    fn link(m: &mut CampaignModel, x: u32, y: u32) {
        m.world.region_neighbours.entry(RegionId(x)).or_default().push(RegionId(y));
        m.world.region_neighbours.entry(RegionId(y)).or_default().push(RegionId(x));
    }

    fn wp(region: u32, sea: bool) -> TradeWaypoint {
        TradeWaypoint { region: RegionId(region), from: 0, to: 0, sea, from_pos: None, to_pos: None }
    }

    /// A route through `regions`, a sea hop from the first.
    fn route(regions: &[u32], capital_port: bool) -> TradePath {
        TradePath { waypoints: regions.iter().enumerate().map(|(i, &r)| wp(r, i == 0)).collect(), capital_port, volumes: Vec::new(), accumulated: 0 }
    }

    fn add_route(m: &mut CampaignModel, exporter: FactionId, importer: FactionId, p: TradePath) {
        m.world.trade_paths.entry((exporter, importer)).or_default().push(p);
    }

    /// `0x00BA42E0`: the land, capital port and sea codes, then the swaps once both caps are used.
    #[test]
    fn route_check_codes() {
        let mut m = trade_map();
        // No land path to B; the capital port reaches B's port 12, then B's capital by land.
        assert!(!m.trade_path_exists(A, B, TradeNetwork::Land));
        assert_eq!((m.capital_port_route_cap(A), m.sea_route_cap(A)), (1, 1));
        assert_eq!(m.trade_route_check(A, B), 3);
        // C is reached by land.
        assert_eq!(m.trade_route_check(A, C), 1);
        // A sea route out of 13 and the capital port route, both to C.
        add_route(&mut m, A, C, route(&[13, 14], false));
        add_route(&mut m, A, C, route(&[10, 14], true));
        // Only the capital port route used: a sea route.
        let mut capital_full = m.clone();
        capital_full.world.trade_paths.get_mut(&(A, C)).unwrap().remove(0);
        assert_eq!(capital_full.trade_route_counts(A), (1, 0));
        assert_eq!(capital_full.trade_route_check(A, B), 2);
        // Both full: the first open route in A's list to C (reached by land too) frees its slot: the sea
        // route gives 4, the capital port route 6.
        assert_eq!(m.trade_route_counts(A), (1, 1));
        assert_eq!(m.trade_route_check(A, B), 4);
        m.world.trade_paths.get_mut(&(A, C)).unwrap().reverse();
        assert_eq!(m.trade_route_check(A, B), 6);
        // No capital on one side: 0 both ways.
        m.world.faction_details.get_mut(&B).unwrap().capital = None;
        assert_eq!((m.trade_route_check(A, B), m.trade_route_check(B, A)), (0, 0));
    }

    /// A full port on the way closes both sea searches: with the sea routes full and no route freed by land,
    /// nothing is left (0). The check runs through its seam.
    #[test]
    fn route_check_full_ports_and_the_seam() {
        let mut m = trade_map();
        // No land link to C: A's routes to C free nothing.
        m.world.region_neighbours.clear();
        link(&mut m, 10, 13);
        link(&mut m, 11, 12);
        add_route(&mut m, A, C, route(&[13, 14], false));
        assert_eq!(m.trade_route_counts(A), (0, 1));
        // The capital port has room: 3.
        assert_eq!(m.trade_route_check(A, B), 3);
        // B's own route fills its port 12: neither sea search reaches B.
        let mut cut = m.clone();
        add_route(&mut cut, B, A, route(&[12, 10], false));
        assert!(cut.trade_port_full(&cut.world.regions[&RegionId(12)]));
        assert_eq!(cut.trade_route_check(A, B), 0);
        // A mod's check replaces the original's.
        let mut rules = (*m.rules).clone();
        let maker: crate::seam::Maker<super::super::seams::TradeRouteCheck> = Arc::new(|_| Arc::new(|_, _, _| 9));
        rules.seams.trade_route_check.apply("test_mod", &maker);
        m.rules = Arc::new(rules);
        assert_eq!(m.trade_route_check(A, B), 9);
    }

    /// `0x00BA42E0` compares the counts with the caps unsigned: a negative cap (cast to a huge count) is
    /// never reached, so the capital port route is still offered.
    #[test]
    fn route_check_negative_cap_is_never_reached() {
        let mut m = trade_map();
        edit_test_building_effects(&mut m, |e| {
            e.iter_mut().filter(|(k, _)| k == "trade_routes_mod_max_sea").for_each(|(_, v)| *v = -1.0);
        });
        assert_eq!((m.capital_port_route_cap(A), m.sea_route_cap(A)), (-1, -1));
        assert_eq!(m.trade_route_counts(A), (0, 0));
        assert_eq!(m.trade_route_check(A, B), 3);
    }

    /// `0x00BB4F80`: an owner with no exporter entry (no route) is never full, even with a 0 cap; once it
    /// exports a route, the 0-cap port is full.
    #[test]
    fn port_full_needs_an_owner_route_list() {
        let mut m = trade_map();
        edit_test_building_effects(&mut m, |e| {
            e.iter_mut().filter(|(k, _)| k == "trade_routes_mod_max_sea").for_each(|(_, v)| *v = 0.0);
        });
        let full = |m: &CampaignModel| m.trade_port_full(&m.world.regions[&RegionId(13)]);
        assert_eq!(m.region_trade_route_cap(&m.world.regions[&RegionId(13)]), 0);
        assert!(!full(&m), "no route list: never full");
        // A route that does not leave from 13 still gives A a list: the 0-cap port is full.
        add_route(&mut m, A, C, route(&[10, 14], true));
        assert!(full(&m));
    }

    /// `0x00BB4F80`: the cap less one per route whose first sea hop leaves the port; an over-full port wraps
    /// and is not full.
    #[test]
    fn port_full_counts_first_sea_hops_and_wraps() {
        let mut m = trade_map();
        let full = |m: &CampaignModel| m.trade_port_full(&m.world.regions[&RegionId(13)]);
        assert!(!full(&m));
        // A route whose first waypoint is a land step: its first sea hop counts.
        add_route(&mut m, A, C, TradePath { waypoints: vec![wp(10, false), wp(13, true), wp(14, false)], capital_port: false, volumes: Vec::new(), accumulated: 0 });
        assert!(full(&m));
        add_route(&mut m, A, B, route(&[13, 12, 11], false));
        assert!(!full(&m), "the count wraps below 0");
        // Another faction's route from the port does not count.
        m.world.trade_paths.remove(&(A, B));
        add_route(&mut m, C, A, route(&[13, 10], false));
        assert!(full(&m));
    }

    fn trading(m: &mut CampaignModel, a: FactionId, b: FactionId) {
        m.relationship_mut(a, b).trade_agreement = true;
        m.relationship_mut(b, a).trade_agreement = true;
        add_route(m, a, b, route(&[10, 12, 11], true));
        add_route(m, b, a, route(&[12, 10], false));
    }

    /// `0x00BBE1A0` → `0x00B73430`: B's port 12 taken by C cuts A from B: the routes go and the agreement
    /// is broken both ways, each side getting the `break_trade` event.
    #[test]
    fn losing_the_route_region_breaks_the_trade_both_ways() {
        let mut m = trade_map();
        trading(&mut m, A, B);
        assert!(m.transfer_region(RegionId(12), C));
        let e = event("break_trade");
        for (x, y) in [(A, B), (B, A)] {
            let r = &m.world.relationships[&(x, y)];
            assert!(!r.trade_agreement);
            let f = r.attitudes[slot("trade_broken")];
            assert_eq!((f.value, f.drift, f.limit), (e.value, e.drift, e.limit));
            assert_eq!(r.attitudes[slot("trade")].value, 0);
            assert!(!m.world.trade_paths.contains_key(&(x, y)));
        }
    }

    /// A pair that still reaches each other keeps its agreement: only the routes through the region go.
    #[test]
    fn losing_a_route_region_keeps_trade_that_can_be_rerouted() {
        let mut m = trade_map();
        trading(&mut m, A, B);
        link(&mut m, 13, 11);
        assert!(m.transfer_region(RegionId(12), C));
        assert!(m.world.relationships[&(A, B)].trade_agreement && m.world.relationships[&(B, A)].trade_agreement);
        assert!(!m.world.trade_paths.contains_key(&(A, B)) && !m.world.trade_paths.contains_key(&(B, A)));
    }

    /// The rebels losing a region: no route change (`0x00A64E87` skips the call).
    #[test]
    fn a_rebel_region_lost_leaves_the_routes() {
        let mut m = trade_map();
        trading(&mut m, A, B);
        m.world.factions.get_mut(&B).unwrap().key.clear();
        assert!(m.transfer_region(RegionId(12), C));
        assert_eq!(m.world.trade_paths[&(A, B)].len(), 1);
        assert!(m.world.relationships[&(A, B)].trade_agreement);
    }

    /// ORIGINAL BUG (`0x00BBE1A0` hangs): a route ending in the region whose importer is not the old owner,
    /// with no route between the exporter and the old owner: the route itself is removed.
    #[test]
    fn a_route_the_rescan_cannot_remove_is_dropped_instead_of_hanging() {
        let mut m = trade_map();
        add_route(&mut m, A, C, route(&[10, 12], false));
        assert!(m.transfer_region(RegionId(12), A));
        assert!(!m.world.trade_paths.contains_key(&(A, C)));
    }

    /// Both agreement breaks: `break_trade` (`0x00B29BB0`) and `break_trade_both_ways` (`0x00B73430`).
    fn break_both(m: &mut CampaignModel, both_ways: bool) {
        if both_ways {
            m.break_trade_both_ways(A, B);
        } else {
            m.break_trade(A, B).unwrap();
        }
        assert!(!m.world.relationships[&(A, B)].trade_agreement && !m.world.relationships[&(B, A)].trade_agreement);
    }

    /// One route each way: each break removes it (`0x00BC0120` once per direction) and the pair's key goes.
    #[test]
    fn breaking_trade_removes_the_one_route_each_way() {
        for both_ways in [false, true] {
            let mut m = trade_map();
            trading(&mut m, A, B);
            break_both(&mut m, both_ways);
            assert!(!m.world.trade_paths.contains_key(&(A, B)) && !m.world.trade_paths.contains_key(&(B, A)), "both_ways {both_ways}");
        }
    }

    /// Two routes each way: only the oldest goes (`0x00BC0120` stops at the first match); the second stays
    /// and is the next break's first.
    #[test]
    fn breaking_trade_keeps_the_second_route_each_way() {
        for both_ways in [false, true] {
            let mut m = trade_map();
            trading(&mut m, A, B);
            let (ab, ba) = (route(&[13, 12, 11], false), route(&[11, 13], false));
            add_route(&mut m, A, B, ab.clone());
            add_route(&mut m, B, A, ba.clone());
            break_both(&mut m, both_ways);
            assert_eq!(m.world.trade_paths[&(A, B)], vec![ab], "both_ways {both_ways}");
            assert_eq!(m.world.trade_paths[&(B, A)], vec![ba], "both_ways {both_ways}");
            break_both(&mut m, both_ways);
            assert!(!m.world.trade_paths.contains_key(&(A, B)) && !m.world.trade_paths.contains_key(&(B, A)));
        }
    }

    /// A route one way only: it goes, the empty direction is a no-op, and other pairs' routes stay.
    #[test]
    fn breaking_trade_with_a_route_one_way_only() {
        for (exporter, importer) in [(A, B), (B, A)] {
            for both_ways in [false, true] {
                let mut m = trade_map();
                m.relationship_mut(A, B).trade_agreement = true;
                m.relationship_mut(B, A).trade_agreement = true;
                add_route(&mut m, exporter, importer, route(&[10, 12, 11], true));
                add_route(&mut m, A, C, route(&[13, 14], false));
                break_both(&mut m, both_ways);
                assert!(!m.world.trade_paths.contains_key(&(A, B)) && !m.world.trade_paths.contains_key(&(B, A)));
                assert_eq!(m.world.trade_paths[&(A, C)].len(), 1, "{exporter:?}->{importer:?} both_ways {both_ways}");
            }
        }
    }

    /// The diplomacy screen's trade status (`RetrieveFactionListForDiplomacy` `0x009F33E1`): the route
    /// check both ways, the player's permission to propose, the other side's value 3 only, embargoes.
    #[test]
    fn diplomacy_trade_status_reads_routes_permissions_and_embargoes() {
        use super::super::negotiation::{DiplomacyTradeStatus, NegotiationAction};
        use super::super::world::Stance;
        let new = |can_trade, land| Some(DiplomacyTradeStatus { held: false, can_trade, land });
        let mut m = trade_map();
        for (x, y) in [(A, B), (B, A), (A, C), (C, A)] {
            m.relationship_mut(x, y);
        }
        // C by land (route code 1), B by the capital port (3).
        assert_eq!(m.diplomacy_trade_status(A, C), new(true, true));
        assert_eq!(m.diplomacy_trade_status(A, B), new(true, false));
        let trade = NegotiationAction::Trade.option();
        let set = |m: &mut CampaignModel, x: FactionId, y: FactionId, v: u32| m.relationship_mut(x, y).diplomacy_options[trade] = v;
        // The player may not propose (2 or 3): cannot; it only declines (1): can.
        for (v, can) in [(1, true), (2, false), (3, false)] {
            set(&mut m, A, C, v);
            assert_eq!(m.diplomacy_trade_status(A, C), new(can, can), "player value {v}");
        }
        set(&mut m, A, C, 0);
        // The other side: only 3 (neither proposes nor accepts) closes it.
        for (v, can) in [(1, true), (2, true), (3, false)] {
            set(&mut m, C, A, v);
            assert_eq!(m.diplomacy_trade_status(A, C), new(can, can), "other value {v}");
        }
        set(&mut m, C, A, 0);
        // An embargo either way.
        m.relationship_mut(C, A).trade_embargo_turns = 5;
        assert_eq!(m.diplomacy_trade_status(A, C), new(false, false));
        m.relationship_mut(C, A).trade_embargo_turns = 0;
        // No capital on one side: no route either way.
        m.world.faction_details.get_mut(&B).unwrap().capital = None;
        assert_eq!(m.diplomacy_trade_status(A, B), new(false, false));
        assert!(!m.trade_possible(A, B, false) && !m.trade_possible(B, A, false));
        // At war: no entry.
        m.world.set_stance(A, C, Stance::War).unwrap();
        assert_eq!(m.diplomacy_trade_status(A, C), None);
    }

    /// A held agreement: "trading" unless either side embargoes; `LandTrade` from the first of the other
    /// side's routes to the player (`0x00B28620`: no sea hop), whatever the scripts say.
    #[test]
    fn diplomacy_trade_status_of_a_held_agreement() {
        use super::super::negotiation::{DiplomacyTradeStatus, NegotiationAction};
        let held = |can_trade, land| Some(DiplomacyTradeStatus { held: true, can_trade, land });
        let mut m = trade_map();
        trading(&mut m, A, B);
        // B's route to A has a sea hop.
        assert_eq!(m.diplomacy_trade_status(A, B), held(true, false));
        m.world.trade_paths.get_mut(&(B, A)).unwrap().insert(0, TradePath { waypoints: vec![wp(12, false), wp(10, false)], ..Default::default() });
        assert_eq!(m.diplomacy_trade_status(A, B), held(true, true));
        m.relationship_mut(A, B).diplomacy_options[NegotiationAction::Trade.option()] = 3;
        assert_eq!(m.diplomacy_trade_status(A, B), held(true, true));
        m.relationship_mut(A, B).trade_embargo_turns = 1;
        assert_eq!(m.diplomacy_trade_status(A, B), held(false, true));
        // `0x00C1A7D0` with cancel: the held agreement; a new one is not possible while held.
        assert!(m.trade_possible(A, B, true) && !m.trade_possible(A, B, false));
        m.world.trade_paths.remove(&(B, A));
        assert_eq!(m.diplomacy_trade_status(A, B), held(false, false));
    }
}

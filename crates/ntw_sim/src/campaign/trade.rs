//! International trade routes: their paths, commodity volumes, blockades and the commodity part
//! of their value (CAMPAIGN_FIDELITY.md §Trade).
//!
//! The original keeps one `INTERNATIONAL_TRADE_ROUTE` per exporter and path (route saver
//! `0x00AFD490`, CONFIRMED layout): `n` waypoints {region id, coord, network node from, node to,
//! sea hop}, the commodity volumes (u32[8], +0x24) and the value parts. Each round `0x00B15B50`
//! recomputes a route (CONFIRMED):
//! - blockaded (`0x00B12050`) → value 0;
//! - else commodity part `Σ volume[c] × price[c]` (prices: `CAMPAIGN_TRADE_MANAGER` u32[8] #4),
//!   resource part (0 in every shipped file), GDP part ([`economy::trade_route_gdp_value`]) and
//!   the accumulated value.
//!
//! Network nodes (`PORT_INDICES` / `SETTLEMENT_INDICES` / `TRADE_NODES`, CONFIRMED in the files):
//! 0..=38 ports, 39..=110 settlements (region capitals), 111.. the off-map trade nodes.
//!
//! [`economy::trade_route_gdp_value`]: super::economy::trade_route_gdp_value

use super::ids::{FactionId, RegionId};
use super::world::CampaignModel;
use crate::fixed::Fixed20;

/// One waypoint of a route (`0x00AFD490`, 0x18 bytes in the exe).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeWaypoint {
    /// The region of the waypoint (the record's i32).
    pub region: RegionId,
    /// Network node of the hop's start (u32::MAX on the last waypoint).
    pub from: u32,
    /// Network node of the hop's end (u32::MAX on the last waypoint).
    pub to: u32,
    /// The hop is a sea lane (the waypoint's byte `+0x14`).
    pub sea: bool,
    /// Map position of `from` (static network data, not state).
    pub from_pos: Option<(Fixed20, Fixed20)>,
    /// Map position of `to`.
    pub to_pos: Option<(Fixed20, Fixed20)>,
}

/// One international route of an (exporter, importer) pair.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradePath {
    /// The waypoints in order.
    pub waypoints: Vec<TradeWaypoint>,
    /// Commodity volumes in `COMMODITIES_ORDER` (+0x24).
    pub volumes: Vec<u32>,
}

/// How close a hostile navy has to be to a sea hop's port to block it (map units).
/// PROVISIONAL: the original tests whether the navy stands in the sea area of the hop's node
/// (`0x00BC4860` → `0x00A8BDE0`, area `+0x14` == node); the area shapes are not read, so the
/// value stands in for the area's size. 1.0 (the navy stands at the port): the vanilla eur save
/// `auto_nr4_t4` has a British navy 1.17 units off Rotterdam while the original still pays the
/// Dutch sea routes (their value is in the stored trade income), so the area is smaller than that.
pub const BLOCKADE_RADIUS: f32 = 1.0;

impl CampaignModel {
    /// `0x00B12050` (CONFIRMED structure): a route is blockaded when a sea hop has a hostile navy
    /// at its start or end node (`0x00BC4860`; hostile = at war with the exporter, INFERRED from
    /// `0x00A67430`). The waypoints' own "blockaded / besieged" test (vtable `+0x98`) is covered for
    /// ports by the same navy test; sieges are not modelled.
    pub fn trade_path_blockaded(&self, exporter: FactionId, path: &TradePath) -> bool {
        let navies: Vec<(f32, f32)> = self
            .world
            .forces
            .values()
            .filter(|f| f.is_navy && self.world.stance(exporter, f.faction) == super::Stance::War)
            .filter_map(|f| self.force_position(f.id))
            .map(|(x, y)| (x.to_f32(), y.to_f32()))
            .collect();
        if navies.is_empty() {
            return false;
        }
        let near = |p: Option<(Fixed20, Fixed20)>| {
            p.is_some_and(|(x, y)| {
                let (x, y) = (x.to_f32(), y.to_f32());
                navies.iter().any(|(nx, ny)| (nx - x).powi(2) + (ny - y).powi(2) <= BLOCKADE_RADIUS * BLOCKADE_RADIUS)
            })
        };
        path.waypoints.iter().any(|w| w.sea && (near(w.from_pos) || near(w.to_pos)))
    }

    /// The commodity part of a route: `Σ volume[c] × price[c]` (`0x00B15B50`, CONFIRMED).
    pub fn trade_commodity_value(&self, volumes: &[u32]) -> i32 {
        volumes.iter().zip(&self.world.commodity_prices).map(|(v, p)| (*v as i64) * (*p as i64)).sum::<i64>().clamp(0, i32::MAX as i64) as i32
    }
}

impl CampaignModel {
    /// A faction's commodity supply this turn, per commodity. With the trade nodes known (map
    /// attached): what its navies gather at the nodes ([`Self::node_supply`]), minus the nodes whose
    /// loaded domestic route home is blockaded. Otherwise the volumes its open domestic routes
    /// bring home. `None` when neither is available (then the international volumes stand).
    pub fn trade_supply(&self, faction: FactionId) -> Option<Vec<u32>> {
        let n = self.world.commodity_prices.len();
        let mut supply = vec![0u32; n];
        if let Some(list) = self.node_supply(faction) {
            let routes = self.world.domestic_trade.get(&faction);
            for (node, c, v) in list {
                let home = routes.and_then(|r| r.iter().find(|p| p.waypoints.first().is_some_and(|w| w.from == node)));
                if home.is_some_and(|p| self.trade_path_blockaded(faction, p)) {
                    continue;
                }
                if let Some(s) = supply.get_mut(c) {
                    *s = s.saturating_add(v);
                }
            }
            return Some(supply);
        }
        let routes = self.world.domestic_trade.get(&faction)?;
        for r in routes.iter().filter(|r| !self.trade_path_blockaded(faction, r)) {
            for (s, v) in supply.iter_mut().zip(&r.volumes) {
                *s = s.saturating_add(*v);
            }
        }
        Some(supply)
    }

    /// How the faction's supply ([`Self::trade_supply`]) is split over its trade partners this turn:
    /// importer → volumes per commodity. `None` when the supply is unknown (the loaded volumes stand).
    ///
    /// The original's split, `0x00BC26D0` (CONFIRMED; run by `0x00BBD870` after the builder `0x00BC0960`
    /// has gathered each faction's supply home). The builder has no importer-side limit (`0x00BB5730`
    /// caps a source only by its path's first stop: 300 per settlement by land, the port's
    /// `commodity_export_vol`); the importers' shares are decided here:
    /// - every international route's volumes are cleared first (`0x00B1C1A0`);
    /// - an exporter without a capital (faction `+0x72C`) has no supply entry and exports nothing;
    /// - a partner's demand is its **net** demand: Σ region demand over its regions (`0x008F4E50`,
    ///   [`Self::commodity_demand`]) minus its own supply when it has a capital ([`Self::trade_supply`];
    ///   region production, the other term, is 0 in every shipped region). A partner counts only when its
    ///   net demand is above 0 for at least one commodity; for the others its demand is 0;
    /// - the partners are the factions in the campaign's faction list order ([`World::factions_in_turn_order`])
    ///   with a trade agreement (relationship `+0x788`) and a route from the exporter;
    /// - per commodity with supply: the (partner, demand) pairs are sorted by demand, largest first, with the
    ///   exe's `std::sort` ([`crate::msvc_sort`]), and handed out from the end (smallest demand first). The
    ///   total starts at Σ max(demand, 1). While `k` partners are left and `k ≤ rest`, the next one gets
    ///   `max(1, (demand × rest) / total)` (32-bit product, unsigned division) and `rest` falls by it; with
    ///   fewer units left than partners it gets nothing. Either way the total then falls by its demand;
    /// - a partner's share goes onto every route of the pair (`0x00B08880`), see [`Self::trade_path_volumes`].
    ///
    /// [`World::factions_in_turn_order`]: super::world::World::factions_in_turn_order
    pub fn trade_split(&self, faction: FactionId) -> Option<std::collections::BTreeMap<FactionId, Vec<u32>>> {
        let supply = self.trade_supply(faction)?;
        let n = supply.len();
        let agreed = super::economy::trade_partners(self, faction);
        let partners: Vec<FactionId> = self
            .world
            .factions_in_turn_order()
            .into_iter()
            .filter(|b| *b != faction && agreed.contains(b) && self.trade_routes_of(faction, *b).is_some())
            .collect();
        let mut out: std::collections::BTreeMap<FactionId, Vec<u32>> = partners.iter().map(|b| (*b, vec![0; n])).collect();
        if self.world.capital(faction).is_none() {
            return Some(out);
        }
        let demand: Vec<(FactionId, Vec<u32>)> = partners.iter().filter_map(|b| self.trade_net_demand(*b, n).map(|d| (*b, d))).collect();
        for (c, s) in supply.iter().enumerate() {
            if *s == 0 {
                continue;
            }
            let mut pairs: Vec<(FactionId, u32)> = demand.iter().map(|(b, d)| (*b, d[c])).collect();
            let mut total = pairs.iter().fold(0u32, |t, (_, d)| t.wrapping_add((*d).max(1)));
            crate::msvc_sort::sort_by(&mut pairs, |x, y| x.1 > y.1);
            let mut rest = *s;
            for (left, (b, d)) in pairs.iter().enumerate().rev() {
                if (left as u32) < rest {
                    // `total` stays ≥ the remaining partners' Σ max(demand, 1) ≥ 1 (it only wraps past
                    // u32::MAX demand in all, where the exe's division would fault).
                    let share = ((*d as i32).wrapping_mul(rest as i32) as u32).checked_div(total).unwrap_or(0).max(1);
                    rest = rest.wrapping_sub(share);
                    if let Some(v) = out.get_mut(b) {
                        v[c] = v[c].wrapping_add(share);
                    }
                }
                total = total.wrapping_sub(*d);
            }
        }
        Some(out)
    }

    /// A faction's net demand per commodity for [`Self::trade_split`] (`0x00BC26D0`, CONFIRMED): its
    /// demand (`0x008F4E50`: Σ [`Self::commodity_demand`] over its regions; the exe counts the regions in
    /// its home theatre, and every shipped map is one theatre) minus its own supply when it has a capital,
    /// kept where it is above 0. `None` when no commodity is above 0 (the faction takes no share).
    fn trade_net_demand(&self, faction: FactionId, n: usize) -> Option<Vec<u32>> {
        let mut demand = vec![0i32; n];
        for r in self.world.regions.values().filter(|r| r.owner == faction) {
            for (x, v) in demand.iter_mut().zip(self.commodity_demand(r)) {
                *x = x.wrapping_add(v as i32);
            }
        }
        if self.world.capital(faction).is_some()
            && let Some(own) = self.trade_supply(faction)
        {
            for (x, v) in demand.iter_mut().zip(own) {
                *x = x.wrapping_sub(v as i32);
            }
        }
        demand.iter().any(|d| *d > 0).then(|| demand.into_iter().map(|d| d.max(0) as u32).collect())
    }

    /// The volumes a route of the pair (`faction`, `importer`) carries this turn: the
    /// importer's share of [`Self::trade_split`] (the exe adds it to every route of the pair, `0x00BC26D0`
    /// → `0x00B08880`, CONFIRMED), or the loaded volumes when the supply is unknown. `split` is
    /// `trade_split(faction)` (passed in to compute it once per faction).
    pub fn trade_path_volumes(
        &self,
        split: Option<&std::collections::BTreeMap<FactionId, Vec<u32>>>,
        importer: FactionId,
        path: &TradePath,
    ) -> Vec<u32> {
        match split {
            None => path.volumes.clone(),
            Some(s) => s.get(&importer).cloned().unwrap_or_else(|| vec![0; self.world.commodity_prices.len()]),
        }
    }

    /// The hard-coded extra of `0x00BB3490` (CONFIRMED; the campaign's [`home_trade_faction`](super::features::CampaignFeatures::home_trade_faction)): in `spa_napoleon` the faction
    /// `spa_france` also earns `Σ domestic volume × price` over its domestic routes (those whose last
    /// waypoint is in the faction's capital: `0x00A8B5A0`, region == faction `+0x72C`) minus
    /// `Σ volume × price` over its international routes. 0 for every other faction.
    pub fn trade_home_value(&self, faction: FactionId) -> i32 {
        let Some(home) = self.rules.features.home_trade_faction.as_deref() else { return 0 };
        if self.world.factions.get(&faction).is_none_or(|f| f.key != home) {
            return 0;
        }
        let value = |volumes: &[u32]| -> i64 {
            volumes.iter().zip(&self.world.commodity_prices).map(|(v, p)| i64::from(*v) * i64::from(*p)).sum()
        };
        let home: i64 = self
            .world
            .domestic_trade
            .get(&faction)
            .into_iter()
            .flatten()
            .filter(|r| r.waypoints.last().is_some_and(|w| Some(w.region) == self.world.capital(faction)))
            .map(|r| value(&r.volumes))
            .sum();
        let exported: i64 =
            self.world.trade_paths.iter().filter(|((a, _), _)| *a == faction).flat_map(|(_, p)| p).map(|p| value(&p.volumes)).sum();
        (home - exported).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

/// An off-map trade node (`CAMPAIGN_TRADE_MANAGER/TRADE_NODES`: position and network node; the
/// `trade_nodes` DB row found through the map's node key). Static data.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeNode {
    /// Network node index.
    pub node: u32,
    /// Map position (logic units).
    pub pos: (f32, f32),
    /// The DB row (`trade_nodes`), once the map's node key is known.
    pub info: Option<TradeNodeInfo>,
}

/// A `trade_nodes` row: #1 commodity, #2 base volume, #3 per extra ship, #4 cap (CONFIRMED order,
/// read by `0x00BC9930` at record +0x0C / +0x10 / +0x14 / +0x18).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeNodeInfo {
    /// Commodity index in `COMMODITIES_ORDER`.
    pub commodity: usize,
    /// #2 base volume.
    pub base: i32,
    /// #3 factor per extra ship.
    pub per_ship: f32,
    /// #4 cap of the factor.
    pub cap: f32,
}

/// The volume a fleet with `ships` trade ships gathers at a node (`0x00BC9930`, CONFIRMED):
/// `trunc((min((ships − 1) × per_ship, cap) + 1) × base)`; 0 without trade ships.
pub fn node_volume(info: &TradeNodeInfo, ships: u32) -> u32 {
    if ships == 0 {
        return 0;
    }
    let f = ((ships - 1) as f32 * info.per_ship).min(info.cap);
    ((f + 1.0) * info.base as f32).max(0.0) as u32
}

/// A trade fleet counts as working a node when it stands within this distance of it
/// (`0x00BC9930`: distance² < 1, CONFIRMED).
pub const NODE_RADIUS: f32 = 1.0;

/// One leg of the static trade network (`CAMPAIGN_TRADE_MANAGER/TRADE_ROUTES`: from, to, spline
/// ids, length).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TradeLeg {
    /// Start node.
    pub from: u32,
    /// End node.
    pub to: u32,
    /// Length in map units.
    pub length: f32,
}

impl CampaignModel {
    /// Trade ships in a force: units of category `naval_merchant` (INFERRED for the naval record
    /// flag +0x1F4 that `0x00BC7890` counts; the shipped trade ships are exactly that category).
    pub fn trade_ships(&self, force: super::ids::ForceId) -> u32 {
        self.world.forces.get(&force).map_or(0, |f| {
            f.units.iter().filter(|u| self.rules.units.get(&u.unit_key).is_some_and(|r| crate::unit_kind::category(&r.category) == crate::unit_kind::Category::NavalMerchant)).count() as u32
        })
    }

    /// The faction's commodity gathering per trade node this turn: (node, commodity, volume) for
    /// every navy of the faction standing at a node with trade ships. `None` when the nodes' DB rows
    /// are not known (no map attached): then the loaded domestic volumes stand.
    pub fn node_supply(&self, faction: FactionId) -> Option<Vec<(u32, usize, u32)>> {
        if self.world.trade_nodes.iter().all(|n| n.info.is_none()) {
            return None;
        }
        let mut out = Vec::new();
        // With the campaign's `node_supply_mod` (`spa_napoleon`) the volume is `trunc((1 + e/100) × v)` with e the faction's
        // `trade_node_supply_mod` (0x00BC9930, CONFIRMED: basic id 0x95 of faction +0x6FC; the vanilla spa
        // saves agree, spa_britain's 37 with +6 → 39).
        let supply_mod = self.rules.features.node_supply_mod
            .then(|| super::effects::Effects::faction_sum(self, faction).get("trade_node_supply_mod"));
        for f in self.world.forces.values().filter(|f| f.faction == faction && f.is_navy) {
            let Some((x, y)) = self.force_position(f.id).map(|(x, y)| (x.to_f32(), y.to_f32())) else { continue };
            let Some(n) = self.world.trade_nodes.iter().find(|n| (n.pos.0 - x).powi(2) + (n.pos.1 - y).powi(2) < NODE_RADIUS * NODE_RADIUS)
            else {
                continue;
            };
            let Some(info) = &n.info else { continue };
            let mut v = node_volume(info, self.trade_ships(f.id));
            if let Some(e) = supply_mod {
                v = ((e * 0.01 + 1.0) * v as f32) as u32;
            }
            if v > 0 {
                out.push((n.node, info.commodity, v));
            }
        }
        Some(out)
    }
}

impl CampaignModel {
    /// The faction's sea route cap (`0x008DC150`, CONFIRMED): a breadth-first walk from its capital over
    /// land neighbours ([`super::World::region_neighbours`]) that the faction owns; every region it
    /// reaches adds the `trade_routes_mod_max_sea` of its slots' buildings (`0x00AAF770`: any building,
    /// whole numbers). The capital counts too (INFERRED: read literally, the walk starts with faction
    /// +0x72C marked visited and never adds it, but every vanilla save needs the capital's ports:
    /// Britain keeps 7 sea routes with all its trading ports in England, so the start entry is likely
    /// not the region object itself). No capital: 0. Without the map's neighbours (not attached) every owned region counts
    /// (PROVISIONAL fallback).
    pub fn sea_route_cap(&self, faction: FactionId) -> i32 {
        let w = &self.world;
        let cap_of = |r: &super::Region| -> i32 {
            r.slots
                .iter()
                .filter_map(|s| s.building.as_ref())
                .map(|b| super::economy::building_effect(&self.rules, &b.level_key, "trade_routes_mod_max_sea") as i32)
                .sum()
        };
        if w.region_neighbours.is_empty() {
            return w.regions.values().filter(|r| r.owner == faction).map(cap_of).sum();
        }
        let Some(capital) = w.capital(faction) else { return 0 };
        let mut visited = vec![capital];
        let mut total = w.regions.get(&capital).filter(|r| r.owner == faction).map_or(0, cap_of);
        let mut i = 0;
        while i < visited.len() {
            for n in w.region_neighbours.get(&visited[i]).map(Vec::as_slice).unwrap_or_default() {
                let Some(r) = w.regions.get(n).filter(|r| r.owner == faction) else { continue };
                if !visited.contains(n) {
                    visited.push(*n);
                    total += cap_of(r);
                }
            }
            i += 1;
        }
        total
    }

    /// A route for a trade agreement that has no loaded route (made in play): the cheapest path over
    /// the static trade network from any port or settlement of `a` to any of `b`. Land legs (one end a
    /// settlement) cost their length, sea legs (ports and nodes) their length × `trade_route_land_sea_bias`
    /// (the cost terms of 0x00BC0DC0, CONFIRMED); trade nodes are not passed through; the sea legs
    /// together may not exceed `trade_route_internat_sea_length_limit` when it is set. PROVISIONAL:
    /// the original's candidate set, port capacities and route caps (0x00BC0960 / 0x00BC0DC0) are not
    /// ported. `None` when the network is not loaded or no path exists.
    pub fn build_trade_route(&self, a: FactionId, b: FactionId) -> Option<TradePath> {
        use std::cmp::Ordering;
        use std::collections::{BTreeMap, BinaryHeap};
        let w = &self.world;
        if w.trade_network.is_empty() {
            return None;
        }
        let owner = |node: u32| w.trade_node_regions.get(&node).and_then(|(r, _)| w.regions.get(r)).map(|r| r.owner);
        let settlement = |node: u32| w.trade_node_regions.get(&node).is_some_and(|(_, port)| !port);
        let is_node = |node: u32| !w.trade_node_regions.contains_key(&node);
        let bias = self.rules.var("trade_route_land_sea_bias", 1.0);
        // With the sea route cap reached, the new route may not use the sea.
        let cap = self.sea_route_cap(a) as f32;
        let sea_routes = w.trade_paths.iter().filter(|((x, _), _)| *x == a).flat_map(|(_, p)| p).filter(|p| p.waypoints.iter().any(|w| w.sea)).count();
        let sea_allowed = (sea_routes as f32) < cap;
        let sea_limit = self.rules.var("trade_route_internat_sea_length_limit", 0.0);
        let mut adj: BTreeMap<u32, Vec<(u32, f32, bool)>> = BTreeMap::new();
        for l in &w.trade_network {
            if is_node(l.from) || is_node(l.to) {
                continue;
            }
            let sea = !settlement(l.from) && !settlement(l.to);
            if sea && !sea_allowed {
                continue;
            }
            adj.entry(l.from).or_default().push((l.to, l.length, sea));
            adj.entry(l.to).or_default().push((l.from, l.length, sea));
        }
        #[derive(PartialEq)]
        struct St(f32, u32, f32);
        impl Eq for St {}
        impl PartialOrd for St {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for St {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
            }
        }
        // node → (cost, sea length so far, previous node and whether the leg is at sea)
        type Best = BTreeMap<u32, (f32, f32, Option<(u32, bool)>)>;
        let mut best: Best = BTreeMap::new();
        let mut heap = BinaryHeap::new();
        for (&node, _) in w.trade_node_regions.iter().filter(|(n, _)| owner(**n) == Some(a)) {
            best.insert(node, (0.0, 0.0, None));
            heap.push(St(0.0, node, 0.0));
        }
        let mut goal = None;
        while let Some(St(cost, node, sea_len)) = heap.pop() {
            if best.get(&node).is_some_and(|b| cost > b.0) {
                continue;
            }
            if owner(node) == Some(b) {
                goal = Some(node);
                break;
            }
            for &(next, len, sea) in adj.get(&node).into_iter().flatten() {
                let s = sea_len + if sea { len } else { 0.0 };
                if sea && sea_limit > 0.0 && s > sea_limit {
                    continue;
                }
                let c = cost + if sea { len * bias } else { len };
                if best.get(&next).is_none_or(|b| c < b.0) {
                    best.insert(next, (c, s, Some((node, sea))));
                    heap.push(St(c, next, s));
                }
            }
        }
        let goal = goal?;
        let mut legs = Vec::new();
        let mut at = goal;
        while let Some((prev, sea)) = best.get(&at).and_then(|b| b.2) {
            legs.push((prev, at, sea));
            at = prev;
        }
        if legs.is_empty() {
            return None;
        }
        legs.reverse();
        let region = |n: u32| w.trade_node_regions.get(&n).map_or(RegionId(0), |(r, _)| *r);
        let mut waypoints: Vec<TradeWaypoint> = legs
            .iter()
            .map(|&(from, to, sea)| TradeWaypoint { region: region(from), from, to, sea, from_pos: self.trade_node_pos(from), to_pos: self.trade_node_pos(to) })
            .collect();
        waypoints.push(TradeWaypoint { region: region(goal), from: u32::MAX, to: u32::MAX, sea: false, from_pos: None, to_pos: None });
        Some(TradePath { waypoints, volumes: vec![0; w.commodity_prices.len()] })
    }
}

impl CampaignModel {
    /// The map position of a network node: a port's slot, a settlement, or a trade node.
    pub fn trade_node_pos(&self, node: u32) -> Option<(Fixed20, Fixed20)> {
        if let Some((r, port)) = self.world.trade_node_regions.get(&node) {
            let r = self.world.regions.get(r)?;
            return if *port { r.slots.iter().find(|s| s.port).and_then(|s| s.position) } else { Some(r.settlement.position) };
        }
        self.world
            .trade_nodes
            .iter()
            .find(|n| n.node == node)
            .map(|n| (Fixed20::from_f64(f64::from(n.pos.0)), Fixed20::from_f64(f64::from(n.pos.1))))
    }

    /// The routes of an (exporter, importer) pair: the loaded ones, else a built one
    /// ([`Self::build_trade_route`]). `None` = no route known: the caller counts the pair with its
    /// GDP part only when the network is not loaded either (tests), else not at all.
    pub fn trade_routes_of(&self, a: FactionId, b: FactionId) -> Option<Vec<TradePath>> {
        if let Some(p) = self.world.trade_paths.get(&(a, b)).filter(|p| !p.is_empty()) {
            return Some(p.clone());
        }
        self.build_trade_route(a, b).map(|p| vec![p])
    }
}

/// The commodity market of `CAMPAIGN_TRADE_MANAGER` (CONFIRMED layout, see `0x00BCB020`).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommodityMarket {
    /// #2 initial prices.
    pub initial: Vec<u32>,
    /// #3 price factors (`f`; price = D × f / (background supply + S)).
    pub factors: Vec<f32>,
    /// #5 the price two rounds back.
    pub previous: Vec<u32>,
    /// #6 the previous price.
    pub previous2: Vec<u32>,
    /// #7 trend codes (0 far up .. 5 far down; UI arrows).
    pub trend: Vec<u32>,
}

impl CampaignModel {
    /// A region's demand per commodity (`0x00AB49F0`, CONFIRMED; checked against the stored region
    /// #32 of the eur start position and saves): for each `commodities_demand_junction` row,
    /// `round(weight × factor × driver)` with the drivers `ddr_GDP` = the region's GDP (+0xC0 = #11,
    /// equal to #10 in every file), `ddr_TW` = trunc(sqrt(town wealth)), the production drivers =
    /// trunc(sqrt(production of commodity 1 / 5 / 7)) and `ddr_textile_production` = trunc(sqrt(Σ
    /// levels of `industry-textile` buildings)), plus the commodity's `demand` effect (not modelled: 0).
    /// Production is not modelled (0 in every shipped European region), so the production and
    /// textile drivers give 0 here (PROVISIONAL for textile buildings).
    pub fn commodity_demand(&self, region: &super::world::Region) -> Vec<u32> {
        let n = self.world.commodity_keys.len();
        let mut out = vec![0i64; n];
        for (commodity, driver, factor, weight) in &self.rules.commodity_demand {
            let Some(c) = self.world.commodity_keys.iter().position(|k| k == commodity) else { continue };
            let value = match driver.as_str() {
                "ddr_GDP" => region.gdp as f32,
                "ddr_TW" => ((region.town_wealth as f32).sqrt() as i32) as f32,
                _ => 0.0,
            };
            out[c] += (weight * factor * value).round_ties_even() as i64;
        }
        out.into_iter().map(|v| v.clamp(0, i64::from(u32::MAX)) as u32).collect()
    }

    /// The round-end price update (`0x00BCB020`, CONFIRMED formula; reproduces the stored prices of
    /// the eur and spa start positions exactly): per commodity, D = Σ demand of every faction-owned
    /// region (the original counts the regions in each faction's home theatre; the shipped campaigns
    /// have one theatre per map: INFERRED), S = Σ region production (not modelled: 0) + Σ the factions'
    /// trade fleet supply; `price = max(1, round(D × f / (background_commodity_supply + S)))`; the trend
    /// compares the price with #5 (> 1.2× → 0, > 1.1× → 1, < 0.8× → 5, < 0.9× → 4, else 2, or 3 when
    /// lower); then #5 ← #6 and #6 ← price. With the campaign's `fixed_commodity_prices` (`spa_napoleon`) the prices stay (the exe skips the
    /// update there once the factors are set). The factors' first-time setup (`f = (bg + S) × f / D`)
    /// is not needed: every file holds set factors.
    pub fn update_commodity_prices(&mut self) {
        let n = self.world.commodity_prices.len();
        if n == 0 || self.world.commodity_market.factors.len() != n || self.rules.features.fixed_commodity_prices {
            return;
        }
        let mut d = vec![0u64; n];
        for r in self.world.regions.values().filter(|r| self.world.factions.contains_key(&r.owner)) {
            for (c, v) in self.commodity_demand(r).into_iter().enumerate().take(n) {
                d[c] += u64::from(v);
            }
        }
        let mut s = vec![0u64; n];
        for f in self.world.factions.keys() {
            for (c, v) in self.trade_supply(*f).unwrap_or_default().into_iter().enumerate().take(n) {
                s[c] += u64::from(v);
            }
        }
        let bg = self.rules.var("background_commodity_supply", 0.0);
        let m = &mut self.world.commodity_market;
        m.previous.resize(n, 0);
        m.previous2.resize(n, 0);
        m.trend.resize(n, 2);
        for c in 0..n {
            let price = ((d[c] as f32 * m.factors[c] / (bg + s[c] as f32)).round_ties_even() as i64).max(1) as u32;
            self.world.commodity_prices[c] = price;
            let (p, prev) = (f64::from(price), f64::from(m.previous[c]));
            m.trend[c] = if p > prev * 1.2 {
                0
            } else if p > prev * 1.1 {
                1
            } else if p < prev * 0.8 {
                5
            } else if p < prev * 0.9 {
                4
            } else {
                2 + u32::from(price < m.previous[c])
            };
            m.previous[c] = m.previous2[c];
            m.previous2[c] = price;
        }
    }
}

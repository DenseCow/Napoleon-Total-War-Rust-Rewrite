//! The saved international trade routes and their accumulated value (SAVE_COMPAT.md §12).
//!
//! `CAMPAIGN_TRADE_MANAGER/INTERNATIONAL_TRADE_ROUTES[]` = {utf16 exporter key,
//! [FACTION_INTERNATIONAL_TRADE_ROUTES_ARRAY] of `INTERNATIONAL_TRADE_ROUTE` v3}. A route record is
//! written by `0x00AFD490` (CONFIRMED field order): u32 `n` (+0xC) and `n` waypoints {i32 region
//! id, coord, u32, u32, bool}, bool (+0x14), u32[] (+0x24), seven u32 (+0x28 .. +0x40), u32[],
//! u32 count with its items, i32. +0x3C is the accumulated value: the round-end accumulator
//! `0x00B05CC0` adds to it (CONFIRMED), so it is child `5n + 8`. The importer is the owner of the
//! last waypoint's region (INFERRED from the layout; `trade_check` measures how often it is a model
//! trade partner).

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::trade::{TradeLeg, TradeNode, TradeNodeInfo, TradePath, TradeWaypoint};
use ntw_sim::campaign::{CampaignModel, FactionId, RegionId};
use ntw_sim::fixed::Fixed20;

const TRADE: &str = "CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER";

/// The index of the accumulated value in a route record with `n` waypoints.
fn accumulated_index(n: usize) -> usize {
    5 * n + 8
}

/// The waypoint count of a route record, if it has the expected layout.
fn waypoints(r: &EsfRecord) -> Option<usize> {
    let n = r.get_u32(0)? as usize;
    let ok = (0..n).all(|k| matches!(r.get(1 + 5 * k), Some(EsfNode::I32(_))))
        && (accumulated_index(n) - 5..=accumulated_index(n) + 1).all(|i| matches!(r.get(i), Some(EsfNode::U32(_))));
    ok.then_some(n)
}

/// One saved route.
#[derive(Debug, Clone)]
pub struct Route {
    /// The exporting faction.
    pub exporter: FactionId,
    /// The region of the last waypoint.
    pub last_region: RegionId,
    /// The accumulated value (+0x3C).
    pub accumulated: u32,
}

impl Route {
    /// The importer: the faction governing the last waypoint's region (normally its owner), as `read_paths`.
    pub fn importer(&self, m: &CampaignModel) -> Option<FactionId> {
        m.world.governing_faction(self.last_region)
    }
}

fn faction_by_key(m: Option<&CampaignModel>, key: &str, esf: &EsfFile) -> Option<FactionId> {
    if let Some(m) = m {
        return m.faction_by_key(key).map(|f| f.id);
    }
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD")?;
    w.record_array("FACTION_ARRAY")?.records().find(|f| f.values().filter_map(EsfNode::as_str).next() == Some(key)).and_then(|f| f.values().next()?.as_i32()).map(FactionId)
}

/// Every route with the expected layout, in file order.
pub fn routes(esf: &EsfFile) -> Vec<Route> {
    let mut out = Vec::new();
    let Some(t) = esf.root.find_path(TRADE) else { return out };
    for it in t.record_array("INTERNATIONAL_TRADE_ROUTES").into_iter().flat_map(|a| a.items.iter()) {
        let Some(key) = it.first().and_then(EsfNode::as_str) else { continue };
        let Some(exporter) = faction_by_key(None, key, esf) else { continue };
        for r in it.iter().filter_map(EsfNode::as_record_array).flat_map(|a| a.records()) {
            let Some(n) = waypoints(r).filter(|&n| n > 0) else { continue };
            let last = r.get_i32(1 + 5 * (n - 1)).unwrap_or(0) as u32;
            out.push(Route { exporter, last_region: RegionId(last), accumulated: r.get_u32(accumulated_index(n)).unwrap_or(0) });
        }
    }
    out
}

/// The model's `trade_accumulated` from a save: the routes' accumulated values summed per
/// (exporter, importer).
pub fn read_accumulated(esf: &EsfFile, m: &CampaignModel) -> BTreeMap<(FactionId, FactionId), i32> {
    let mut out: BTreeMap<(FactionId, FactionId), i32> = BTreeMap::new();
    for r in routes(esf) {
        if let Some(i) = r.importer(m) {
            let e = out.entry((r.exporter, i)).or_insert(0);
            *e = e.saturating_add(r.accumulated.min(i32::MAX as u32) as i32);
        }
    }
    out
}

/// A route in the tree: (item, array/record position, pair, waypoints, stored value).
type RouteAt = (usize, usize, (FactionId, FactionId), usize, u32);

/// Writes the commodity market back into `CAMPAIGN_TRADE_MANAGER` (CONFIRMED layout, `0x00BCB020`;
/// read by [`read_paths`]): #3 f32[] price factors, #4 u32[] current prices, #5 the price two rounds
/// back, #6 the previous price, #7 trend codes. #2 (initial prices) is left as stored. A slot is
/// written only when it holds an array of the same type and length (the model is never longer or
/// shorter than the file it was loaded from), so a loaded save is written back unchanged.
pub(crate) fn write_market(model_rec: &mut EsfRecord, m: &CampaignModel) {
    let Some(t) = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(r) if r.name == "CAMPAIGN_TRADE_MANAGER" => Some(&mut **r),
        _ => None,
    }) else {
        return;
    };
    let mk = &m.world.commodity_market;
    let put_u32 = |t: &mut EsfRecord, i: usize, v: &[u32]| {
        if let Some(EsfNode::U32Array(a)) = t.children.get_mut(i)
            && a.len() == v.len()
        {
            a.copy_from_slice(v);
        }
    };
    if let Some(EsfNode::F32Array(a)) = t.children.get_mut(3)
        && a.len() == mk.factors.len()
        && a.iter().zip(&mk.factors).any(|(x, y)| x.to_bits() != y.to_bits())
    {
        a.copy_from_slice(&mk.factors);
    }
    put_u32(t, PRICES, &m.world.commodity_prices);
    put_u32(t, 5, &mk.previous);
    put_u32(t, 6, &mk.previous2);
    put_u32(t, 7, &mk.trend);
}

/// Writes the model's `trade_accumulated` into the routes of a save tree (the CAMPAIGN_MODEL
/// record): for each (exporter, importer) the change from the stored sum goes to its first route,
/// so pairs with several routes keep their split and an unchanged model writes the file back
/// byte for byte.
pub(crate) fn write_accumulated(model_rec: &mut EsfRecord, m: &CampaignModel) {
    let Some(t) = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(r) if r.name == "CAMPAIGN_TRADE_MANAGER" => Some(&mut **r),
        _ => None,
    }) else {
        return;
    };
    // Stored sums per pair.
    let mut stored: BTreeMap<(FactionId, FactionId), i64> = BTreeMap::new();
    let mut first: BTreeMap<(FactionId, FactionId), (usize, usize)> = BTreeMap::new();
    let Some(EsfNode::RecordArray(routes)) = t.children.iter_mut().find(|c| matches!(c, EsfNode::RecordArray(a) if a.name == "INTERNATIONAL_TRADE_ROUTES")) else { return };
    // (item, array/record position, pair, waypoints, stored value)
    let mut pairs: Vec<RouteAt> = Vec::new();
    for (i, it) in routes.items.iter().enumerate() {
        let Some(key) = it.first().and_then(EsfNode::as_str) else { continue };
        let Some(exporter) = m.faction_by_key(key).map(|f| f.id) else { continue };
        for (j, r) in it.iter().enumerate().filter_map(|(j, n)| n.as_record_array().map(|a| (j, a))).flat_map(|(j, a)| a.records().enumerate().map(move |(k, r)| ((j, k), r))) {
            let Some(n) = waypoints(r).filter(|&n| n > 0) else { continue };
            let last = RegionId(r.get_i32(1 + 5 * (n - 1)).unwrap_or(0) as u32);
            let Some(importer) = m.world.governing_faction(last) else { continue };
            let acc = r.get_u32(accumulated_index(n)).unwrap_or(0);
            *stored.entry((exporter, importer)).or_default() += i64::from(acc);
            first.entry((exporter, importer)).or_insert((i, j.0 * 1_000_000 + j.1));
            pairs.push((i, j.0 * 1_000_000 + j.1, (exporter, importer), n, acc));
        }
    }
    for (i, jk, pair, n, acc) in pairs {
        if first.get(&pair) != Some(&(i, jk)) {
            continue;
        }
        let want = i64::from(m.world.trade_accumulated.get(&pair).copied().unwrap_or(0));
        let new = (i64::from(acc) + want - stored[&pair]).clamp(0, i64::from(u32::MAX)) as u32;
        if new == acc {
            continue;
        }
        let (j, k) = (jk / 1_000_000, jk % 1_000_000);
        if let Some(EsfNode::RecordArray(a)) = routes.items[i].get_mut(j)
            && let Some(EsfNode::Record(r)) = a.items.get_mut(k).and_then(|item| item.first_mut())
            && let Some(slot @ EsfNode::U32(_)) = r.children.get_mut(accumulated_index(n))
        {
            *slot = EsfNode::U32(new);
        }
    }
}

/// Map positions of the trade network nodes (CONFIRMED numbering in every file): ports from
/// `PORT_INDICES` (the region slot with the same key), settlements from `SETTLEMENT_INDICES`
/// (the region's settlement), off-map trade nodes from `TRADE_NODES` (coord, node).
fn node_positions(t: &EsfRecord, m: &CampaignModel) -> BTreeMap<u32, (Fixed20, Fixed20)> {
    let mut out = BTreeMap::new();
    let pairs = |name: &str| -> Vec<(String, u32)> {
        t.record_array(name)
            .into_iter()
            .flat_map(|a| a.items.iter())
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_u32()?)))
            .collect()
    };
    for (key, node) in pairs("PORT_INDICES") {
        if let Some(p) = m.world.regions.values().flat_map(|r| &r.slots).find(|s| s.key == key).and_then(|s| s.position) {
            out.insert(node, p);
        }
    }
    for (key, node) in pairs("SETTLEMENT_INDICES") {
        if let Some(r) = m.world.regions.values().find(|r| r.key == key) {
            out.insert(node, r.settlement.position);
        }
    }
    for it in t.record_array("TRADE_NODES").into_iter().flat_map(|a| a.items.iter()) {
        if let (Some(EsfNode::Coord2d(x, y)), Some(node)) = (it.first(), it.get(1).and_then(EsfNode::as_u32)) {
            out.insert(node, (Fixed20::from_f64(f64::from(*x)), Fixed20::from_f64(f64::from(*y))));
        }
    }
    out
}

/// Routes by (exporter, importer).
pub type PairPaths = BTreeMap<(FactionId, FactionId), Vec<TradePath>>;

/// The routes' paths and commodity volumes per (exporter, importer), in file order, and the
/// commodity prices (`CAMPAIGN_TRADE_MANAGER` #4, u32[8]; CONFIRMED: `Σ volume × price` equals
/// every route's stored commodity part, `trade_value_parts_match_the_files`).
pub fn read_paths(esf: &EsfFile, m: &CampaignModel) -> (PairPaths, Vec<u32>) {
    let mut out: BTreeMap<(FactionId, FactionId), Vec<TradePath>> = BTreeMap::new();
    let Some(t) = esf.root.find_path(TRADE) else { return (out, Vec::new()) };
    let prices = t.get(PRICES).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    let pos = node_positions(t, m);
    for it in t.record_array("INTERNATIONAL_TRADE_ROUTES").into_iter().flat_map(|a| a.items.iter()) {
        let Some(key) = it.first().and_then(EsfNode::as_str) else { continue };
        let Some(exporter) = m.faction_by_key(key).map(|f| f.id) else { continue };
        for r in it.iter().filter_map(EsfNode::as_record_array).flat_map(|a| a.records()) {
            let Some(n) = waypoints(r).filter(|&n| n > 0) else { continue };
            let last = RegionId(r.get_i32(1 + 5 * (n - 1)).unwrap_or(0) as u32);
            // The importer: the faction governing the last waypoint's region (normally its owner; INFERRED
            // from `auto_after_c8`, where Württemberg's route to its partner Bavaria ends in eur_bavaria,
            // owned by Austria but still in Bavaria's governorship).
            let Some(importer) = m.world.governing_faction(last) else { continue };
            let waypoints = (0..n)
                .map(|k| {
                    let b = 1 + 5 * k;
                    let from = r.get_u32(b + 2).unwrap_or(u32::MAX);
                    let to = r.get_u32(b + 3).unwrap_or(u32::MAX);
                    TradeWaypoint {
                        region: RegionId(r.get_i32(b).unwrap_or(0) as u32),
                        from,
                        to,
                        sea: r.get(b + 4).and_then(EsfNode::as_bool).unwrap_or(false),
                        from_pos: pos.get(&from).copied(),
                        to_pos: pos.get(&to).copied(),
                    }
                })
                .collect();
            let volumes = r.get(5 * n + 2).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
            out.entry((exporter, importer)).or_default().push(TradePath { waypoints, volumes });
        }
    }
    (out, prices)
}

/// A route's stored value parts (+0x28 total, +0x2C commodity, +0x30 resource, +0x34 GDP, +0x38
/// accumulated in the value, +0x3C accumulated), for checks.
pub fn stored_parts(esf: &EsfFile) -> Vec<(FactionId, RegionId, [u32; 6], Vec<u32>)> {
    let mut out = Vec::new();
    let Some(t) = esf.root.find_path(TRADE) else { return out };
    for it in t.record_array("INTERNATIONAL_TRADE_ROUTES").into_iter().flat_map(|a| a.items.iter()) {
        let Some(key) = it.first().and_then(EsfNode::as_str) else { continue };
        let Some(exporter) = faction_by_key(None, key, esf) else { continue };
        for r in it.iter().filter_map(EsfNode::as_record_array).flat_map(|a| a.records()) {
            let Some(n) = waypoints(r).filter(|&n| n > 0) else { continue };
            let last = RegionId(r.get_i32(1 + 5 * (n - 1)).unwrap_or(0) as u32);
            let v = |i: usize| r.get_u32(5 * n + i).unwrap_or(0);
            let volumes = r.get(5 * n + 2).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
            out.push((exporter, last, [v(3), v(4), v(5), v(6), v(7), v(8)], volumes));
        }
    }
    out
}

/// The `CAMPAIGN_TRADE_MANAGER` child holding the commodity prices.
pub const PRICES: usize = 4;

/// Each faction's domestic routes (`DOMESTIC_TRADE_ROUTES[]` = {utf16 faction key,
/// [FACTION_DOMESTIC_TRADE_ROUTES_ARRAY] of {`DOMESTIC_TRADE_ROUTE` v1, u32}}). A route record is
/// bool, utf16, coord2d, u32 `n`, `n` waypoints {i32 region id (0 at a trade node), coord, u32
/// from, u32 to, bool sea}, bool, u32[8] volumes, u32, i32 (layout read from the files; the
/// volumes add up to what the faction's international routes carry, e.g. Britain in eur).
pub fn read_domestic(esf: &EsfFile, m: &CampaignModel) -> BTreeMap<FactionId, Vec<TradePath>> {
    let mut out: BTreeMap<FactionId, Vec<TradePath>> = BTreeMap::new();
    let Some(t) = esf.root.find_path(TRADE) else { return out };
    let pos = node_positions(t, m);
    for it in t.record_array("DOMESTIC_TRADE_ROUTES").into_iter().flat_map(|a| a.items.iter()) {
        let Some(key) = it.first().and_then(EsfNode::as_str) else { continue };
        let Some(faction) = m.faction_by_key(key).map(|f| f.id) else { continue };
        let entry = out.entry(faction).or_default();
        for r in it.iter().filter_map(EsfNode::as_record_array).flat_map(|a| a.records()) {
            let Some(n) = r.get_u32(3).map(|n| n as usize) else { continue };
            let Some(volumes) = r.get(5 + 5 * n).and_then(EsfNode::as_u32_array) else { continue };
            let waypoints = (0..n)
                .map(|k| {
                    let b = 4 + 5 * k;
                    let from = r.get_u32(b + 2).unwrap_or(u32::MAX);
                    let to = r.get_u32(b + 3).unwrap_or(u32::MAX);
                    TradeWaypoint {
                        region: RegionId(r.get_i32(b).unwrap_or(0) as u32),
                        from,
                        to,
                        sea: r.get(b + 4).and_then(EsfNode::as_bool).unwrap_or(false),
                        from_pos: pos.get(&from).copied(),
                        to_pos: pos.get(&to).copied(),
                    }
                })
                .collect();
            entry.push(TradePath { waypoints, volumes: volumes.to_vec() });
        }
    }
    out
}

/// The static trade data of a file: commodity keys (`COMMODITIES_ORDER`), the network legs
/// (`TRADE_ROUTES`: u32 from, u32 to, u32[] splines, f32 length), the trade nodes (`TRADE_NODES`:
/// coord, node) and which region each port / settlement node lies in (`PORT_INDICES` key
/// `port:<region>:<town>`, `SETTLEMENT_INDICES` region key). The nodes' DB rows are attached later
/// by [`attach_trade_nodes`], which needs the map's node keys.
pub fn read_network(esf: &EsfFile, m: &mut CampaignModel) {
    let Some(t) = esf.root.find_path(TRADE) else { return };
    let keys: Vec<String> = t
        .record_array("COMMODITIES_ORDER")
        .into_iter()
        .flat_map(|a| a.items.iter())
        .filter_map(|it| it.first()?.as_str().map(str::to_string))
        .collect();
    let legs: Vec<TradeLeg> = t
        .record_array("TRADE_ROUTES")
        .into_iter()
        .flat_map(|a| a.items.iter())
        .filter_map(|it| {
            Some(TradeLeg { from: it.first()?.as_u32()?, to: it.get(1)?.as_u32()?, length: it.get(3).and_then(EsfNode::as_f32).unwrap_or(0.0) })
        })
        .collect();
    let nodes: Vec<TradeNode> = t
        .record_array("TRADE_NODES")
        .into_iter()
        .flat_map(|a| a.items.iter())
        .filter_map(|it| match (it.first(), it.get(1).and_then(EsfNode::as_u32)) {
            (Some(EsfNode::Coord2d(x, y)), Some(node)) => Some(TradeNode { node, pos: (*x, *y), info: None }),
            _ => None,
        })
        .collect();
    let mut regions = BTreeMap::new();
    let pairs = |name: &str| -> Vec<(String, u32)> {
        t.record_array(name)
            .into_iter()
            .flat_map(|a| a.items.iter())
            .filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_u32()?)))
            .collect()
    };
    for (key, node) in pairs("PORT_INDICES") {
        let region_key = key.split(':').nth(1).unwrap_or_default();
        if let Some(r) = m.world.regions.values().find(|r| r.key == region_key) {
            regions.insert(node, (r.id, true));
        }
    }
    for (key, node) in pairs("SETTLEMENT_INDICES") {
        if let Some(r) = m.world.regions.values().find(|r| r.key == key) {
            regions.insert(node, (r.id, false));
        }
    }
    m.world.commodity_keys = keys;
    let u = |i: usize| t.get(i).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    m.world.commodity_market = ntw_sim::campaign::trade::CommodityMarket {
        initial: u(2),
        factors: t.get(3).and_then(EsfNode::as_f32_array).map(<[f32]>::to_vec).unwrap_or_default(),
        previous: u(5),
        previous2: u(6),
        trend: u(7),
    };
    m.world.trade_network = legs;
    m.world.trade_nodes = nodes;
    m.world.trade_node_regions = regions;
}

/// Attaches each trade node's `trade_nodes` DB row, matching the file's node positions with the
/// campaign map's named nodes (`regions.esf` `trade_nodes`: key, position; nearest within 2 map
/// units). Without it the model keeps the loaded domestic volumes ([`CampaignModel::node_supply`]).
pub fn attach_trade_nodes(m: &mut CampaignModel, map_nodes: &[(String, (f32, f32))]) {
    let rules = m.rules.clone();
    let keys = m.world.commodity_keys.clone();
    for n in &mut m.world.trade_nodes {
        let best = map_nodes
            .iter()
            .map(|(k, p)| (k, (p.0 - n.pos.0).powi(2) + (p.1 - n.pos.1).powi(2)))
            .filter(|(_, d)| *d < 4.0)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        n.info = best.and_then(|(k, _)| rules.trade_nodes.get(k)).and_then(|(c, base, per_ship, cap)| {
            Some(TradeNodeInfo { commodity: keys.iter().position(|x| x == c)?, base: *base, per_ship: *per_ship, cap: *cap })
        });
    }
}

/// Attaches the campaign map's static data the model reads: the trade nodes' DB rows
/// ([`attach_trade_nodes`]) and the regions' land neighbours ([`attach_region_neighbours`]).
pub fn attach_map(m: &mut CampaignModel, map: &ntw_formats::campaign_map::RegionMap) {
    attach_trade_nodes(m, &map.trade_nodes);
    attach_region_neighbours(m, map);
}

/// Fills `World::region_neighbours` from the map's outline connectivity: two game regions are
/// neighbours when their map regions share an outline edge. Seas, rivers, lakes and the land outside
/// the theatre are not game regions and drop out.
pub fn attach_region_neighbours(m: &mut CampaignModel, map: &ntw_formats::campaign_map::RegionMap) {
    let by_key: BTreeMap<&str, RegionId> =
        m.world.regions.values().map(|r| (r.key.as_str(), r.id)).collect();
    let mut out = BTreeMap::new();
    for mr in &map.regions {
        let Some(&id) = by_key.get(mr.key.as_str()) else { continue };
        let list: Vec<_> = mr.neighbours.iter().filter_map(|&i| map.regions.get(i)).filter_map(|n| by_key.get(n.key.as_str()).copied()).collect();
        out.insert(id, list);
    }
    m.world.region_neighbours = out;
}

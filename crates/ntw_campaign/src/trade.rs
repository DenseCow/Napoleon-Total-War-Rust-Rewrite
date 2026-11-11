//! The saved international trade routes and their accumulated value (SAVE_COMPAT.md §12).
//!
//! `CAMPAIGN_TRADE_MANAGER/INTERNATIONAL_TRADE_ROUTES[]` = {utf16 exporter key,
//! [FACTION_INTERNATIONAL_TRADE_ROUTES_ARRAY] of `INTERNATIONAL_TRADE_ROUTE` v3}. A route record is
//! written by `0x00AFD490` (CONFIRMED field order): u32 `n` (+0xC) and `n` waypoints {i32 region
//! id, coord, u32, u32, bool}, bool (+0x14, the capital port flag: `TradePath::capital_port`), u32[] (+0x24), seven u32 (+0x28 .. +0x40), u32[],
//! u32 count with its items (the commerce-raid list +0x58..+0x64: per entry i32 amount, u32 raiding
//! faction's saved id; the reader `0x00AEC420` reads it for record versions above 1, CONFIRMED; not
//! modelled, see `economy::trade_pair_value`), i32. +0x3C is the accumulated value: the round-end
//! accumulator `0x00B05CC0` adds to it (CONFIRMED), so it is child `5n + 8`, kept per route
//! (`TradePath::accumulated`). The importer is the owner of the
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
    /// The accumulated value (+0x3C, an `int` stored as a u32 node: see [`accumulated_i32`]).
    pub accumulated: i32,
    /// The length of the commerce-raid list (+0x60, the u32 count after the u32[] at +0x4C), if the record has one.
    pub raids: Option<u32>,
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
            out.push(Route { exporter, last_region: RegionId(last), accumulated: accumulated_i32(r.get_u32(accumulated_index(n)).unwrap_or(0)), raids: r.get_u32(5 * n + 11) });
        }
    }
    out
}

/// A stored accumulated value as the model's `i32`. The route's +0x3C is an `int` (the accumulator
/// `0x00B05CC0` adds a truncated `int` to it) that the saver `0x00AFD490` writes as its 4 raw bytes
/// under the u32 node tag (8) and the reader `0x00AEC420` reads back the same way (CONFIRMED; the
/// record signature of the 12 evidence saves has a u32 there): the node holds the int's bits, so both ways are a bit
/// cast, every value fits and nothing is clamped ([`accumulated_u32`]).
fn accumulated_i32(v: u32) -> i32 {
    v as i32
}

/// The model's accumulated value as the record's u32 node: the bit cast back of [`accumulated_i32`].
fn accumulated_u32(v: i32) -> u32 {
    v as u32
}

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

/// Whether a stored route record has the waypoints of a model route (region, nodes, sea hop), as
/// [`read_paths`] reads them.
fn same_waypoints(r: &EsfRecord, n: usize, path: &TradePath) -> bool {
    path.waypoints.len() == n
        && path.waypoints.iter().enumerate().all(|(k, w)| {
            let b = 1 + 5 * k;
            r.get_i32(b) == Some(w.region.0 as i32)
                && r.get_u32(b + 2).unwrap_or(u32::MAX) == w.from
                && r.get_u32(b + 3).unwrap_or(u32::MAX) == w.to
                && r.get(b + 4).and_then(EsfNode::as_bool).unwrap_or(false) == w.sea
        })
}

/// Writes each model route's accumulated value ([`TradePath::accumulated`], +0x3C) into its record
/// in a save tree (the CAMPAIGN_MODEL record). A record is matched to the first model route of its
/// (exporter, importer) pair with the same waypoints not matched yet, so routes the model removed
/// (their records stay: this writer adds or removes no route) and repeated paths keep their own
/// values; an unchanged model writes the file back byte for byte. A pair's built route
/// (`World::trade_accumulated`) has no record and is not written.
pub(crate) fn write_accumulated(model_rec: &mut EsfRecord, m: &CampaignModel) {
    let Some(t) = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(r) if r.name == "CAMPAIGN_TRADE_MANAGER" => Some(&mut **r),
        _ => None,
    }) else {
        return;
    };
    let Some(EsfNode::RecordArray(routes)) = t.children.iter_mut().find(|c| matches!(c, EsfNode::RecordArray(a) if a.name == "INTERNATIONAL_TRADE_ROUTES")) else { return };
    // The model routes already matched, per pair.
    let mut used: BTreeMap<(FactionId, FactionId), Vec<bool>> = BTreeMap::new();
    for it in routes.items.iter_mut() {
        let Some(key) = it.first().and_then(EsfNode::as_str) else { continue };
        let Some(exporter) = m.faction_by_key(key).map(|f| f.id) else { continue };
        for a in it.iter_mut().filter_map(|c| match c {
            EsfNode::RecordArray(a) => Some(a),
            _ => None,
        }) {
            for r in a.items.iter_mut().filter_map(|item| match item.first_mut() {
                Some(EsfNode::Record(r)) => Some(&mut **r),
                _ => None,
            }) {
                let Some(n) = waypoints(r).filter(|&n| n > 0) else { continue };
                let last = RegionId(r.get_i32(1 + 5 * (n - 1)).unwrap_or(0) as u32);
                let Some(importer) = m.world.governing_faction(last) else { continue };
                let Some(paths) = m.world.trade_paths.get(&(exporter, importer)) else { continue };
                let taken = used.entry((exporter, importer)).or_insert_with(|| vec![false; paths.len()]);
                let Some(i) = (0..paths.len()).find(|&i| !taken[i] && same_waypoints(r, n, &paths[i])) else { continue };
                taken[i] = true;
                let want = accumulated_u32(paths[i].accumulated);
                if let Some(EsfNode::U32(v)) = r.children.get_mut(accumulated_index(n))
                    && *v != want
                {
                    *v = want;
                }
            }
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
            let capital_port = r.get(5 * n + 1).and_then(EsfNode::as_bool).unwrap_or(false);
            let accumulated = accumulated_i32(r.get_u32(accumulated_index(n)).unwrap_or(0));
            out.entry((exporter, importer)).or_default().push(TradePath { waypoints, volumes, capital_port, accumulated });
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
            entry.push(TradePath { waypoints, volumes: volumes.to_vec(), capital_port: false, accumulated: 0 });
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
    let (mut dropped, mut partial) = (0usize, 0usize);
    let legs: Vec<TradeLeg> = t
        .record_array("TRADE_ROUTES")
        .into_iter()
        .flat_map(|a| a.items.iter())
        .filter_map(|it| {
            let (Some(from), Some(to)) = (it.first().and_then(EsfNode::as_u32), it.get(1).and_then(EsfNode::as_u32)) else {
                dropped += 1;
                return None;
            };
            let length = it.get(3).and_then(EsfNode::as_f32);
            let splines = it.get(2).and_then(EsfNode::as_u32_array);
            if length.is_none() || splines.is_none() {
                partial += 1;
            }
            Some(TradeLeg { from, to, length: length.unwrap_or(0.0), splines: splines.map(<[u32]>::to_vec).unwrap_or_default() })
        })
        .collect();
    if dropped > 0 || partial > 0 {
        log::warn!(
            "load: TRADE_ROUTES: {dropped} leg(s) without their nodes skipped, {partial} without their splines or length (no segments, length 0)"
        );
    }
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
    warn_demand_mismatch(m);
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

/// Logs once per load the regions whose stored demand (`REGION` #32, `Region::commodity_demand`) does not
/// hold one entry per commodity: a #32 missing (read empty) or of another length, which no shipped file
/// has. How the exe reads a short #32 vector is not traced, so ours keeps it as loaded: the faction demand
/// and the price update count the entries it has (the missing ones as 0), until the next demand step
/// rebuilds it.
fn warn_demand_mismatch(m: &CampaignModel) {
    let n = m.world.commodity_keys.len();
    if n == 0 {
        return;
    }
    let missing = m.world.regions.values().filter(|r| r.commodity_demand.is_empty()).count();
    let other = m.world.regions.values().filter(|r| !r.commodity_demand.is_empty() && r.commodity_demand.len() != n).count();
    if missing > 0 {
        log::warn!("{missing} regions have no stored demand (REGION #32): their demand counts 0 until the next demand step");
    }
    if other > 0 {
        log::warn!("{other} regions store a demand (REGION #32) whose length is not the {n} commodities: kept as loaded until the next demand step");
    }
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
/// ([`attach_trade_nodes`]), the regions' land neighbours ([`attach_region_neighbours`]), their land
/// masses ([`attach_land_masses`]), the black shroud outlines (`World::black_shroud_outlines`) and
/// the settlements' area outlines ([`attach_settlement_areas`]).
pub fn attach_map(m: &mut CampaignModel, map: &ntw_formats::campaign_map::RegionMap) {
    attach_trade_nodes(m, &map.trade_nodes);
    attach_region_neighbours(m, map);
    attach_land_masses(m, map);
    m.world.black_shroud_outlines = map.black_shroud_outlines.clone();
    attach_settlement_areas(m, map);
}

/// Fills `World::settlement_area_outlines` for `0x008AF750`: the area index at each region's
/// settlement (the exe asks its map quadtree `0x00AA0FD0`, whose record's high half is the index of
/// the area within its map region, `0x00A0BBD0`; ours finds the area whose triangles hold the point,
/// the same area for any point inside one) and the outlines of the region's own area at that index
/// (area `+0x44` is its index in the region's list: the loader `0x00A4A7B0` passes each area its
/// running index and the area constructor `0x00A49C30` stores it, 16 bits, at `0x00A49CA8`;
/// CONFIRMED). The settlement position is the model's, Fixed20 read as map units as the exe reads
/// it.
///
/// A region is left out, and named in one warning per cause, when its settlement is on no area (or
/// the map has no region of its key, or no area at that index) and when one of the area's outlines
/// names a vertex past the map's vertex list (a damaged map: an outline with a point missing would
/// give a wrong entry point, so none is given). The model then measures from the force's own
/// position (`World::settlement_area_outlines`). A map region with an area face naming a vertex past
/// the list is named in a third warning (the face is skipped, so a settlement on it reads as on no
/// area).
pub fn attach_settlement_areas(m: &mut CampaignModel, map: &ntw_formats::campaign_map::RegionMap) {
    let (out, gaps) = settlement_areas(&m.world, map);
    if !gaps.no_area.is_empty() {
        log::warn!("map: {} regions have no map area at their settlement, so an expulsion measures the way to them from the army's own position: {:?}", gaps.no_area.len(), gaps.no_area);
    }
    if !gaps.bad_outline.is_empty() {
        log::warn!("map: {} regions' settlement areas have an outline vertex past the map's vertex list and are left out, so an expulsion measures the way to them from the army's own position: {:?}", gaps.bad_outline.len(), gaps.bad_outline);
    }
    if !gaps.bad_face.is_empty() {
        log::warn!("map: {} map regions have an area face naming a vertex past the map's vertex list; the face is skipped, so a settlement standing on it reads as on no area: {:?}", gaps.bad_face.len(), gaps.bad_face);
    }
    m.world.settlement_area_outlines = out;
}

/// The regions [`settlement_areas`] left out, by key, each list reported once by
/// [`attach_settlement_areas`].
#[derive(Debug, Default, PartialEq, Eq)]
struct SettlementAreaGaps {
    /// No map region of the key, the settlement on no area, or no area of the region at that index.
    no_area: Vec<String>,
    /// An outline of the area names a vertex past the map's vertex list.
    bad_outline: Vec<String>,
    /// Map regions (by their own key, game region or not) with an area face that names a vertex
    /// past the map's vertex list: the face is skipped when looking for the area under a
    /// settlement, so a settlement on it is among `no_area`.
    bad_face: Vec<String>,
}

/// The settlement area outlines by region, each outline a loop of map points
/// (`World::settlement_area_outlines`).
type AreaOutlines = BTreeMap<RegionId, Vec<Vec<(f32, f32)>>>;

/// The settlement area outlines of every game region that has them (see
/// [`attach_settlement_areas`]) and the regions left out.
fn settlement_areas(w: &ntw_sim::campaign::World, map: &ntw_formats::campaign_map::RegionMap) -> (AreaOutlines, SettlementAreaGaps) {
    let v = |i: u32| map.vertices.get(i as usize).copied();
    let holds = |area: &ntw_formats::campaign_map::RegionArea, p: (f32, f32)| {
        area.faces.as_chunks::<3>().0.iter().any(|t| match (v(t[0]), v(t[1]), v(t[2])) {
            (Some(a), Some(b), Some(c)) => in_triangle(p, a, b, c),
            _ => false,
        })
    };
    let mut out = BTreeMap::new();
    let mut gaps = SettlementAreaGaps {
        bad_face: map.regions.iter().filter(|mr| mr.areas.iter().any(|a| a.faces.iter().any(|&i| v(i).is_none()))).map(|mr| mr.key.clone()).collect(),
        ..SettlementAreaGaps::default()
    };
    for r in w.regions.values() {
        let p = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
        let area = map.regions.iter().find(|mr| mr.key == r.key).and_then(|mr| {
            let index = map.regions.iter().find_map(|x| x.areas.iter().position(|a| holds(a, p)))?;
            mr.areas.get(index)
        });
        let Some(area) = area else {
            gaps.no_area.push(r.key.clone());
            continue;
        };
        let loops: Option<Vec<Vec<(f32, f32)>>> = area.outlines.iter().map(|l| l.iter().map(|&i| v(i)).collect()).collect();
        match loops {
            Some(loops) => {
                out.insert(r.id, loops);
            }
            None => gaps.bad_outline.push(r.key.clone()),
        }
    }
    (out, gaps)
}

/// True when `p` is inside or on triangle (a, b, c), either winding.
fn in_triangle(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let side = |u: (f32, f32), w: (f32, f32)| (w.0 - u.0) * (p.1 - u.1) - (w.1 - u.1) * (p.0 - u.0);
    let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
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

/// Fills `World::region_land_masses`: each game region's CAI area's HLCIs (`AiArea::hlcis`) that pass
/// `0x00C5C490`, i.e. have a piece whose area of the region's map region (`regions.esf` `areas[]`,
/// the piece resolved by `0x00C47A00`) has flag #2 set (area byte +2, read by `0x00A49C30`).
///
/// Left empty when no area carries HLCIs: the region groups then take every region to lie on one
/// land mass (no warning for a campaign with no CAI areas at all; one when it has areas and none
/// has an HLCI). Otherwise every game region gets an entry, so the groups never fall back to "one
/// land mass" on partial data: a region with no CAI area or map region, or with no HLCI that
/// passes, gets an empty one, joins no neighbour and forms a region group of its own. Each such
/// gap, and a piece index past the map region's areas, is counted in one warning (the exe's
/// resolve `0x00C47A00` stops at such an index and returns false; UNKNOWN what its caller then
/// does, so leaving that piece out and reading the rest is PROVISIONAL).
pub fn attach_land_masses(m: &mut CampaignModel, map: &ntw_formats::campaign_map::RegionMap) {
    let w = &mut m.world;
    w.region_land_masses.clear();
    if w.ai_areas.iter().all(|a| a.hlcis.is_empty()) {
        if !w.ai_areas.is_empty() {
            log::warn!("map: none of the {} CAI areas has land-mass data (HLCIs); every region counts as lying on one land mass", w.ai_areas.len());
        }
        return;
    }
    let (out, gaps) = land_masses(w, map);
    if gaps.missing > 0 {
        log::warn!("map: {} regions have no CAI area or map region, so no land mass: each forms a region group of its own", gaps.missing);
    }
    if gaps.bad_pieces > 0 {
        log::warn!("map: {} HLCI pieces name no area of their map region; left out of the land-mass test", gaps.bad_pieces);
    }
    if gaps.no_land > 0 {
        log::warn!("map: {} regions have a CAI area but no HLCI with a land piece, so no land mass: each forms a region group of its own", gaps.no_land);
    }
    w.region_land_masses = out;
}

/// What [`land_masses`] could not fill, each reported once by [`attach_land_masses`].
#[derive(Debug, Default, PartialEq, Eq)]
struct LandMassGaps {
    /// Game regions with no CAI area or no map region of their key.
    missing: usize,
    /// HLCI pieces whose index is past their map region's areas.
    bad_pieces: usize,
    /// Game regions with both, but no HLCI that passes the land test.
    no_land: usize,
}

/// The land masses of every game region (see [`attach_land_masses`]; an empty list where there is
/// none) and what was missing.
fn land_masses(w: &ntw_sim::campaign::World, map: &ntw_formats::campaign_map::RegionMap) -> (BTreeMap<RegionId, Vec<u32>>, LandMassGaps) {
    let by_key: BTreeMap<&str, &ntw_formats::campaign_map::MapRegion> = map.regions.iter().map(|r| (r.key.as_str(), r)).collect();
    let areas: BTreeMap<&str, &ntw_sim::campaign::AiArea> = w.ai_areas.iter().map(|a| (a.key.as_str(), a)).collect();
    let mut gaps = LandMassGaps::default();
    let mut out = BTreeMap::new();
    for r in w.regions.values() {
        let mut land = Vec::new();
        match (areas.get(r.key.as_str()), by_key.get(r.key.as_str())) {
            (Some(a), Some(mr)) => {
                for h in &a.hlcis {
                    let mut passes = false;
                    for &p in &h.pieces {
                        match mr.areas.get(p as usize) {
                            Some(x) => passes |= x.flags[2],
                            None => gaps.bad_pieces += 1,
                        }
                    }
                    if passes {
                        land.push(h.index);
                    }
                }
                if land.is_empty() {
                    gaps.no_land += 1;
                }
            }
            _ => gaps.missing += 1,
        }
        out.insert(r.id, land);
    }
    (out, gaps)
}

#[cfg(test)]
mod settlement_area_tests {
    use ntw_formats::campaign_map::{MapRegion, RegionArea, RegionMap};

    use super::*;

    /// A map whose every region is a row of 10 x 10 squares, one area each: region `n`'s area `k`
    /// covers x `10k..10k+10`, z `20n..20n+10`. An area's outline starts at its far corner (so the
    /// outline order differs from the vertex order), and `bad` (region, area) gets an outline that
    /// names a vertex the map does not have.
    fn map(regions: &[(&str, usize)], bad: Option<(usize, usize)>) -> RegionMap {
        let mut vertices = Vec::new();
        let mut out = Vec::new();
        for (n, &(key, areas)) in regions.iter().enumerate() {
            let mut list = Vec::new();
            for k in 0..areas {
                let (x, z, i) = (10.0 * k as f32, 20.0 * n as f32, vertices.len() as u32);
                vertices.extend([(x, z), (x + 10.0, z), (x + 10.0, z + 10.0), (x, z + 10.0)]);
                let mut outline = vec![i + 2, i + 3, i, i + 1];
                if bad == Some((n, k)) {
                    outline[1] = u32::MAX;
                }
                list.push(RegionArea { faces: vec![i, i + 1, i + 2, i, i + 2, i + 3], outlines: vec![outline], flags: [false; 3] });
            }
            out.push(MapRegion { key: key.into(), is_sea: false, min: (0.0, 0.0), max: (1.0, 1.0), areas: list, settlement: None, neighbours: Vec::new() });
        }
        RegionMap {
            bounds_min: (0.0, 0.0),
            bounds_max: (1.0, 1.0),
            theatre: ((0.0, 0.0), (1.0, 1.0)),
            labels: Vec::new(),
            vertices,
            regions: out,
            trade_nodes: Vec::new(),
            ground_types: Vec::new(),
            black_shroud_outlines: Vec::new(),
        }
    }

    fn settle(m: &mut CampaignModel, region: u32, x: i32, z: i32) {
        m.world.regions.get_mut(&RegionId(region)).unwrap().settlement.position = (Fixed20::from_int(x), Fixed20::from_int(z));
    }

    /// The outline a region is entered at (`0x008AF750`) is the one of its own area with the index
    /// the map gives at its settlement, its points in the outline's order; a region whose
    /// settlement is on no area, or whose map region is missing, gets no entry and is named.
    #[test]
    fn a_region_gets_the_outline_of_its_own_area_at_its_settlements_index() {
        // The made-up model's regions: made_up_region_1, _2 and _5000000.
        let mut m = crate::own_save::tests::made_up_model();
        let regions = map(&[("made_up_region_1", 2), ("made_up_region_2", 2)], None);
        // Region 1's settlement in its second area, region 2's in its first, and the third region
        // (no map region of its key) on region 2's second area.
        settle(&mut m, 1, 15, 5);
        settle(&mut m, 2, 5, 25);
        settle(&mut m, 5_000_000, 15, 25);
        let (areas, gaps) = settlement_areas(&m.world, &regions);
        let want: AreaOutlines = [
            (RegionId(1), vec![vec![(20.0, 10.0), (10.0, 10.0), (10.0, 0.0), (20.0, 0.0)]]),
            (RegionId(2), vec![vec![(10.0, 30.0), (0.0, 30.0), (0.0, 20.0), (10.0, 20.0)]]),
        ]
        .into();
        assert_eq!(areas, want);
        assert_eq!(gaps, SettlementAreaGaps { no_area: vec!["made_up_region_5000000".into()], ..SettlementAreaGaps::default() });
        attach_settlement_areas(&mut m, &regions);
        assert_eq!(m.world.settlement_area_outlines, want);

        // The index is the one of the area under the settlement, whichever map region that area
        // belongs to, and the outline is the region's own area at that index: region 2's settlement
        // on region 1's second area gives region 2's second area.
        settle(&mut m, 2, 15, 5);
        let (areas, gaps) = settlement_areas(&m.world, &regions);
        assert_eq!(areas[&RegionId(2)], vec![vec![(20.0, 30.0), (10.0, 30.0), (10.0, 20.0), (20.0, 20.0)]]);
        assert_eq!(gaps.no_area, vec!["made_up_region_5000000".to_string()]);

        // A settlement on no area (here past every square), and an index its own map region does not
        // have (region 2 has one area, the settlement is on region 1's second): no entry, named.
        settle(&mut m, 1, 500, 500);
        let regions = map(&[("made_up_region_1", 2), ("made_up_region_2", 1)], None);
        let (areas, gaps) = settlement_areas(&m.world, &regions);
        assert!(areas.is_empty(), "{areas:?}");
        assert_eq!(gaps.no_area, ["made_up_region_1", "made_up_region_2", "made_up_region_5000000"].map(String::from));
        // An attach replaces what an earlier map gave.
        attach_settlement_areas(&mut m, &regions);
        assert!(m.world.settlement_area_outlines.is_empty());
    }

    /// An outline that names a vertex the map does not have leaves its region out and names it
    /// (review: the point was dropped without a log, and the shortened outline was used).
    #[test]
    fn an_outline_with_a_vertex_past_the_list_leaves_its_region_out() {
        let mut m = crate::own_save::tests::made_up_model();
        let regions = map(&[("made_up_region_1", 2), ("made_up_region_2", 2)], Some((1, 0)));
        settle(&mut m, 1, 15, 5);
        settle(&mut m, 2, 5, 25);
        let (areas, gaps) = settlement_areas(&m.world, &regions);
        assert_eq!(areas.keys().copied().collect::<Vec<_>>(), vec![RegionId(1)]);
        assert_eq!(
            gaps,
            SettlementAreaGaps { no_area: vec!["made_up_region_5000000".into()], bad_outline: vec!["made_up_region_2".into()], bad_face: Vec::new() }
        );
        attach_settlement_areas(&mut m, &regions);
        assert_eq!(m.world.settlement_area_outlines, areas);
    }

    /// A face that names a vertex the map does not have is skipped, so the settlement on it reads
    /// as on no area; its map region is named on its own (review: it was skipped without a report).
    #[test]
    fn a_face_with_a_vertex_past_the_list_is_named() {
        let mut m = crate::own_save::tests::made_up_model();
        let mut regions = map(&[("made_up_region_1", 2), ("made_up_region_2", 2)], None);
        settle(&mut m, 1, 15, 5);
        settle(&mut m, 2, 5, 25);
        // Region 1's second area (under its settlement) loses a vertex of both its triangles.
        for f in [0, 3] {
            regions.regions[0].areas[1].faces[f] = u32::MAX;
        }
        let (areas, gaps) = settlement_areas(&m.world, &regions);
        assert_eq!(areas.keys().copied().collect::<Vec<_>>(), vec![RegionId(2)]);
        assert_eq!(
            gaps,
            SettlementAreaGaps {
                no_area: vec!["made_up_region_1".into(), "made_up_region_5000000".into()],
                bad_outline: Vec::new(),
                bad_face: vec!["made_up_region_1".into()],
            }
        );
    }
}

#[cfg(test)]
mod land_mass_tests {
    use ntw_formats::campaign_map::{MapRegion, RegionArea, RegionMap};
    use ntw_sim::campaign::{AiArea, AiHlci};

    use super::*;

    fn map(regions: Vec<(&str, Vec<bool>)>) -> RegionMap {
        let area = |land: bool| RegionArea { faces: Vec::new(), outlines: Vec::new(), flags: [false, false, land] };
        let region = |(key, areas): (&str, Vec<bool>)| MapRegion {
            key: key.into(),
            is_sea: false,
            min: (0.0, 0.0),
            max: (1.0, 1.0),
            areas: areas.into_iter().map(area).collect(),
            settlement: None,
            neighbours: Vec::new(),
        };
        RegionMap {
            bounds_min: (0.0, 0.0),
            bounds_max: (1.0, 1.0),
            theatre: ((0.0, 0.0), (1.0, 1.0)),
            labels: Vec::new(),
            vertices: Vec::new(),
            regions: regions.into_iter().map(region).collect(),
            trade_nodes: Vec::new(),
            ground_types: Vec::new(),
            black_shroud_outlines: Vec::new(),
        }
    }

    fn area(key: &str, hlcis: Vec<(u32, Vec<u32>)>) -> AiArea {
        AiArea { key: key.into(), links: Vec::new(), hlcis: hlcis.into_iter().map(|(index, pieces)| AiHlci { index, pieces }).collect() }
    }

    /// Every game region gets an entry once a CAI area carries HLCIs: an HLCI counts when one of its
    /// pieces is a flag #2 area of the map region (`0x00C5C490`); a region with no CAI area, no map
    /// region or no passing HLCI gets an empty list (a group of its own) and is counted, and so is a
    /// piece index past the map region's areas (review: these read as "flag not set" with no log,
    /// and a map left empty flipped the groups to "every region on one land mass").
    #[test]
    fn every_region_gets_an_entry_and_the_gaps_are_counted() {
        // The made-up model's regions: made_up_region_1, _2 and _5000000.
        let mut m = crate::own_save::tests::made_up_model();
        m.world.ai_areas = vec![area("made_up_region_1", vec![(4, vec![0, 1]), (9, vec![0]), (7, vec![1, 5])]), area("made_up_region_2", vec![(4, vec![0])])];
        let regions = map(vec![("made_up_region_1", vec![false, true]), ("made_up_region_2", vec![false])]);
        let (masses, gaps) = land_masses(&m.world, &regions);
        let want: BTreeMap<RegionId, Vec<u32>> = [(RegionId(1), vec![4, 7]), (RegionId(2), vec![]), (RegionId(5_000_000), vec![])].into();
        assert_eq!(masses, want);
        assert_eq!(gaps, LandMassGaps { missing: 1, bad_pieces: 1, no_land: 1 });
        attach_land_masses(&mut m, &regions);
        assert_eq!(m.world.region_land_masses, masses);

        // Every region missing from the map: the entries stay, so no region joins another.
        attach_land_masses(&mut m, &map(Vec::new()));
        assert_eq!(m.world.region_land_masses.len(), 3);
        assert!(m.world.region_land_masses.values().all(Vec::is_empty));

        // No HLCI anywhere (a save or start without them), then no CAI area at all: left empty,
        // which the region groups read as one land mass.
        m.world.ai_areas = vec![area("made_up_region_1", Vec::new())];
        attach_land_masses(&mut m, &regions);
        assert!(m.world.region_land_masses.is_empty());
        m.world.ai_areas.clear();
        attach_land_masses(&mut m, &regions);
        assert!(m.world.region_land_masses.is_empty());
    }
}

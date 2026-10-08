//! Reads (and encodes) the sight data of a campaign file: `QUAD_TREE_BIT_ARRAY` (cell sets),
//! `FACTION` `CAMPAIGN_SHROUD` and the regions' `LINE_OF_SIGHT` shapes, into
//! `ntw_sim::campaign::visibility` (CHARACTERS_FIDELITY.md §10).
//!
//! `QUAD_TREE_BIT_ARRAY` v1 (CONFIRMED by decoding every settlement's saved sight disc, 129 in 4
//! vanilla saves): {u32 columns, u32 rows, u32 root size, `QUAD_TREE_BIT_ARRAY_NODE`}. A node is
//! either four child nodes (in the order (x, z + half), (x + half, z + half), (x, z), (x + half, z))
//! or a leaf {u32 low, u32 high}: at size 8 a 64-bit mask, bit 8 × row + column; above 8 a uniform
//! block (0 = empty, otherwise full).

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};
use ntw_sim::campaign::visibility::{CellSet, Shroud, SightGrid, TradeSegmentSight};
use ntw_sim::campaign::CampaignModel;

const ORDER: [(u32, u32); 4] = [(0, 1), (1, 1), (0, 0), (1, 0)];

fn node_kids(n: &EsfRecord) -> Vec<&EsfRecord> {
    n.children
        .iter()
        .filter_map(|c| match c {
            EsfNode::Record(r) if r.name == "QUAD_TREE_BIT_ARRAY_NODE" => Some(&**r),
            _ => None,
        })
        .collect()
}

fn decode_node(n: &EsfRecord, x: u32, z: u32, size: u32, out: &mut CellSet) {
    let kids = node_kids(n);
    if kids.len() == 4 {
        let h = size / 2;
        for (i, k) in kids.iter().enumerate() {
            decode_node(k, x + ORDER[i].0 * h, z + ORDER[i].1 * h, h, out);
        }
        return;
    }
    let m = u64::from(n.get_u32(0).unwrap_or(0)) | (u64::from(n.get_u32(1).unwrap_or(0)) << 32);
    if size > 8 {
        if m != 0 {
            for dz in 0..size {
                for dx in 0..size {
                    out.set(x + dx, z + dz);
                }
            }
        }
        return;
    }
    for b in 0..64u32 {
        if m >> b & 1 == 1 {
            out.set(x + b % 8, z + b / 8);
        }
    }
}

/// The grid a `QUAD_TREE_BIT_ARRAY` record describes.
pub fn grid_of(q: &EsfRecord) -> Option<SightGrid> {
    Some(SightGrid::centred(q.get_u32(0)?, q.get_u32(1)?, q.get_u32(2)?))
}

/// The cells of a `QUAD_TREE_BIT_ARRAY` record.
pub fn decode(q: &EsfRecord) -> Option<CellSet> {
    let (cols, rows, root) = (q.get_u32(0)?, q.get_u32(1)?, q.get_u32(2)?);
    let mut out = CellSet::new(cols, rows);
    decode_node(q.child("QUAD_TREE_BIT_ARRAY_NODE")?, 0, 0, root, &mut out);
    Some(out)
}

fn encode_node(set: &CellSet, x: u32, z: u32, size: u32) -> EsfRecord {
    let leaf = |lo: u32, hi: u32| EsfRecord { name: "QUAD_TREE_BIT_ARRAY_NODE".into(), version: 1, children: vec![EsfNode::U32(lo), EsfNode::U32(hi)] };
    if size == 8 {
        let mut m = 0u64;
        for b in 0..64u32 {
            if set.get(x + b % 8, z + b / 8) {
                m |= 1 << b;
            }
        }
        return leaf(m as u32, (m >> 32) as u32);
    }
    let (mut any, mut all) = (false, true);
    for dz in 0..size {
        for dx in 0..size {
            let v = set.get(x + dx, z + dz);
            any |= v;
            all &= v;
        }
    }
    if !any {
        return leaf(0, 0);
    }
    if all {
        return leaf(u32::MAX, u32::MAX);
    }
    let h = size / 2;
    EsfRecord {
        name: "QUAD_TREE_BIT_ARRAY_NODE".into(),
        version: 1,
        children: ORDER.iter().map(|o| EsfNode::Record(Box::new(encode_node(set, x + o.0 * h, z + o.1 * h, h)))).collect(),
    }
}

/// A `QUAD_TREE_BIT_ARRAY` record for a cell set (the inverse of [`decode`]; uniform blocks above
/// 8 cells are written as leaves; PROVISIONAL for full blocks, which the vanilla saves never show
/// above size 8).
pub fn encode(set: &CellSet, root: u32) -> EsfRecord {
    EsfRecord {
        name: "QUAD_TREE_BIT_ARRAY".into(),
        version: 1,
        children: vec![EsfNode::U32(set.cols), EsfNode::U32(set.rows), EsfNode::U32(root), EsfNode::Record(Box::new(encode_node(set, 0, 0, root)))],
    }
}

/// Reads the sight grid, every faction's shroud and the regions' sight shapes into the model.
pub(crate) fn read(esf: &EsfFile, model: &mut CampaignModel) {
    let Some(world) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return };
    let Some(factions) = world.record_array("FACTION_ARRAY") else { return };
    for fac in factions.records() {
        let Some(sh) = fac.child("CAMPAIGN_SHROUD") else { continue };
        let trees: Vec<&EsfRecord> = sh.children_named("QUAD_TREE_BIT_ARRAY").collect();
        if trees.len() != 3 {
            continue;
        }
        if model.world.sight_grid.is_none() {
            model.world.sight_grid = grid_of(trees[0]);
        }
        let Some(key) = fac.get_str(9) else { continue };
        let Some(id) = model.faction_by_key(key).map(|f| f.id) else { continue };
        let (Some(explored), Some(visible), Some(hidden)) = (decode(trees[0]), decode(trees[1]), decode(trees[2])) else { continue };
        let active = sh.values().find_map(EsfNode::as_bool).unwrap_or(true);
        model.world.shrouds.insert(id, Shroud { explored, visible, hidden, active });
    }
    let Some(regions) = world.child("REGION_MANAGER").and_then(|m| m.record_array("REGIONS_ARRAY")) else { return };
    for r in regions.records() {
        let Some(los) = r.child("LINE_OF_SIGHT") else { continue };
        if los.get(0).and_then(EsfNode::as_bool) != Some(true) {
            continue;
        }
        let Some(q) = los.child("QUAD_TREE_BIT_ARRAY") else { continue };
        if model.world.sight_grid.is_none() {
            model.world.sight_grid = grid_of(q);
        }
        let Some(key) = r.get_str(0) else { continue };
        let Some(id) = model.world.regions.values().find(|x| x.key == key).map(|x| x.id) else { continue };
        if let Some(cells) = decode(q) {
            model.world.region_sight.insert(id, cells.cells().collect());
        }
    }
    read_trade(esf, model);
}

/// The trade segments' sight shapes and the routes on them (`TRADE_SEGMENTS[]` = {[COMMERCE_RAIDS],
/// u32 n, 4n coord2d, f32[n], f32[n], f32, u32, u32[] domestic route ids, u32[] international route
/// ids, u32, f32, `LINE_OF_SIGHT`, i32, bool}; a route's id is the u32 after its record in
/// `FACTION_DOMESTIC_TRADE_ROUTES_ARRAY` / `FACTION_INTERNATIONAL_TRADE_ROUTES_ARRAY`, also its
/// record's last i32).
fn read_trade(esf: &EsfFile, model: &mut CampaignModel) {
    let Some(t) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER") else { return };
    let mut domestic = std::collections::BTreeMap::new();
    let mut international = std::collections::BTreeMap::new();
    for (name, intl) in [("DOMESTIC_TRADE_ROUTES", false), ("INTERNATIONAL_TRADE_ROUTES", true)] {
        for it in t.record_array(name).into_iter().flat_map(|a| a.items.iter()) {
            let Some(f) = it.first().and_then(EsfNode::as_str).and_then(|k| model.faction_by_key(k)).map(|f| f.id) else { continue };
            for e in it.iter().filter_map(EsfNode::as_record_array).flat_map(|a| a.items.iter()) {
                let Some(id) = e.get(1).and_then(EsfNode::as_u32) else { continue };
                if !intl {
                    domestic.insert(id, f);
                    continue;
                }
                // The last waypoint's region (`trade::routes` layout: u32 n, then n × {i32 region, ...}).
                let Some(r) = e.first().and_then(EsfNode::as_record) else { continue };
                let Some(n) = r.get_u32(0).filter(|&n| n > 0) else { continue };
                if let Some(last) = r.get_i32(1 + 5 * (n as usize - 1)) {
                    international.insert(id, (f, ntw_sim::campaign::RegionId(last as u32)));
                }
            }
        }
    }
    for it in t.record_array("TRADE_SEGMENTS").into_iter().flat_map(|a| a.items.iter()) {
        let Some(li) = it.iter().position(|c| matches!(c, EsfNode::Record(r) if r.name == "LINE_OF_SIGHT")) else { continue };
        let Some(los) = it[li].as_record() else { continue };
        let cells = if los.get(0).and_then(EsfNode::as_bool) == Some(true) {
            los.child("QUAD_TREE_BIT_ARRAY").and_then(decode).map(|c| c.cells().collect()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let ids = |k: usize| li.checked_sub(k).and_then(|i| it[i].as_u32_array()).unwrap_or(&[]);
        model.world.trade_sight.push(TradeSegmentSight {
            cells,
            domestic: ids(4).iter().filter_map(|i| domestic.get(i).copied()).collect(),
            international: ids(3).iter().filter_map(|i| international.get(i).copied()).collect(),
        });
    }
}

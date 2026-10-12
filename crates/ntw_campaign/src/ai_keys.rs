//! The campaign AI's stored keys and values in a start position or save, read into the model
//! ([`World::ai_keys`], [`World::region_base_values`]) so the AI never reads the file.
//!
//! **Manager and personality keys** (CONFIRMED round 4, `analysis/ai/AI_RESEARCH.md` §2.3): the DB
//! manager reads its key from the campaign faction's `+0x82C` (getter `0x00C164C0`) and the
//! personality object reads `+0x838` (getter `0x00C164D0`). Both are filled by the `FACTION` record
//! reader (`0x0087A190`, at `0x0087B559`): two UTF-16 strings read in a row, then, for record
//! version >= 15, two more strings into `+0x844` / `+0x850` (else both are set to `"default"`). In
//! the shipped eur startpos (`FACTION` v18) they are the children #48..#51 just before the two
//! `FACTION_FLAG_AND_COLOURS` records, e.g. France: `nap_eur_france`, `eur_france`, `default`,
//! `default`. The meaning of the third and fourth strings is UNKNOWN (INFERRED: director /
//! `cdir_*` configs, whose only key is `default`).
//!
//! **Region base values** (`analysis/ai/AI_RESEARCH.md` §4 "Region value", CONFIRMED layout):
//! every belief of `CAI_INTERFACE/CAI_BDI_POOL/CAI_BDI_POOL_BELIEFS[]` that holds a
//! `CAI_REGION_BASE_VALUE` record (belief type 77 = 0x4D, the value `0x00ABD5E0` computes)
//! carries the value in its `CAI_BASE_VALUE` record (`#0 i32`, the belief's `+0xFC`) and the
//! region's CAI component id in `CAI_REGION_BASE_VALUE #0`; `CAI_WORLD/CAI_WORLD_REGIONS[]` maps
//! that id (item value #2) to the region key (`CAI_REGION #10`). In the shipped eur startpos all
//! 72 values have the form `15000 + 25 k`, as the decoded formula gives.
//!
//! **Map areas** (CONFIRMED, `analysis/fidelity/CAMPAIGN_FIDELITY.md` §Campaign director): the CAI
//! world's areas (`CAI_WORLD_REGIONS[]`: the land regions, the seas, the theatres and `all`) with
//! their links, in the order the campaign director walks them ([`read_ai_areas`]).
//!
//! [`World::ai_keys`]: ntw_sim::campaign::World::ai_keys
//! [`World::region_base_values`]: ntw_sim::campaign::World::region_base_values

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::campaign::{AiArea, CampaignModel, FactionAiKeys, FactionId};

/// Reads the AI keys of one `FACTION` record: the four strings right before its first
/// `FACTION_FLAG_AND_COLOURS` child (two strings for record versions below 15, the others then
/// `"default"`). `None` when the layout does not match.
pub fn faction_ai_keys(rec: &EsfRecord) -> Option<FactionAiKeys> {
    let k = rec.children.iter().position(|c| matches!(c, EsfNode::Record(r) if r.name == "FACTION_FLAG_AND_COLOURS"))?;
    let n = if rec.version >= 15 { 4 } else { 2 };
    let s: Vec<&str> = rec.children.get(k.checked_sub(n)?..k)?.iter().map(EsfNode::as_str).collect::<Option<_>>()?;
    let extra = if n == 4 { [s[2].to_string(), s[3].to_string()] } else { ["default".to_string(), "default".to_string()] };
    Some(FactionAiKeys { manager: s[0].to_string(), personality: s[1].to_string(), extra })
}

/// The AI keys of every faction in a startpos or save, by faction key (the `FACTION` record's
/// second plain value). Reads `CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY` and the rebel
/// faction; factions whose record does not match the layout are left out.
pub fn read_ai_keys(root: &EsfRecord) -> BTreeMap<String, FactionAiKeys> {
    let mut out = BTreeMap::new();
    let Some(world) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return out };
    let mut factions: Vec<&EsfRecord> = Vec::new();
    if let Some(arr) = world.record_array("FACTION_ARRAY") {
        factions.extend(arr.records().filter(|r| r.name == "FACTION"));
    }
    if let Some(r) = world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")) {
        factions.push(r);
    }
    for rec in factions {
        let Some(key) = rec.values().nth(1).and_then(EsfNode::as_str) else { continue };
        if let Some(keys) = faction_ai_keys(rec) {
            out.insert(key.to_string(), keys);
        }
    }
    out
}

/// The original's own region base values stored in a startpos or save, by region key (see the
/// module notes). Empty when the file has no CAI state.
pub fn read_region_base_values(root: &EsfRecord) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    let Some(cai) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE") else { return out };
    let mut keys: BTreeMap<u32, String> = BTreeMap::new();
    if let Some(regions) = cai.child("CAI_WORLD").and_then(|w| w.record_array("CAI_WORLD_REGIONS")) {
        for item in &regions.items {
            let id = item.get(2).and_then(EsfNode::as_u32);
            let key = item
                .iter()
                .filter_map(EsfNode::as_record)
                .find(|r| r.name == "CAI_REGION")
                .and_then(|r| r.get(10))
                .and_then(EsfNode::as_str);
            if let (Some(id), Some(key)) = (id, key) {
                keys.insert(id, key.to_string());
            }
        }
    }
    let Some(beliefs) = cai.child("CAI_BDI_POOL").and_then(|p| p.record_array("CAI_BDI_POOL_BELIEFS")) else { return out };
    for item in &beliefs.items {
        let recs: Vec<&EsfRecord> = item.iter().filter_map(EsfNode::as_record).collect();
        let Some(region) = recs.iter().find(|r| r.name == "CAI_REGION_BASE_VALUE").and_then(|r| r.get(0)).and_then(EsfNode::as_u32) else {
            continue;
        };
        let Some(value) = recs.iter().find(|r| r.name == "CAI_BASE_VALUE").and_then(|r| r.get(0)).and_then(EsfNode::as_i32) else {
            continue;
        };
        if let Some(key) = keys.get(&region) {
            out.insert(key.clone(), value);
        }
    }
    out
}

/// The campaign AI's map areas in a startpos or save ([`World::ai_areas`](ntw_sim::campaign::World::ai_areas)):
/// `CAI_WORLD/CAI_WORLD_REGIONS[]` in file order (the order the CAI world loader `0x00BF0130`
/// appends them to its list +0x130/+0x134, and the saver `0x00C09400` writes them back), each
/// `CAI_REGION` key (#10) with its boundaries (#7, `CAI_REGION_BOUNDARY` components of
/// `CAI_WORLD_REGION_BOUNDARIES[]`, whose #0/#1 are the two areas' component ids) as the indexes of
/// the areas on their other side. A boundary naming an unknown area is left out. Empty when the
/// file has no CAI state.
pub fn read_ai_areas(root: &EsfRecord) -> Vec<AiArea> {
    let Some(world) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE/CAI_WORLD") else { return Vec::new() };
    fn class<'a>(item: &'a [EsfNode], name: &str) -> Option<&'a EsfRecord> {
        item.iter().filter_map(EsfNode::as_record).find(|r| r.name == name)
    }
    // A component's id: its item's first plain value (after the property records).
    let id = |item: &[EsfNode]| item.iter().find(|n| !matches!(n, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_u32);
    let mut boundaries: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    for item in world.record_array("CAI_WORLD_REGION_BOUNDARIES").map_or(&[][..], |a| a.items.as_slice()) {
        let (Some(c), Some(b)) = (id(item), class(item, "CAI_REGION_BOUNDARY")) else { continue };
        if let (Some(x), Some(y)) = (b.get(0).and_then(EsfNode::as_u32), b.get(1).and_then(EsfNode::as_u32)) {
            boundaries.insert(c, (x, y));
        }
    }
    let regions: Vec<(u32, &EsfRecord)> = world
        .record_array("CAI_WORLD_REGIONS")
        .map_or(&[][..], |a| a.items.as_slice())
        .iter()
        .filter_map(|item| Some((id(item)?, class(item, "CAI_REGION")?)))
        .collect();
    let index: BTreeMap<u32, usize> = regions.iter().enumerate().map(|(i, (id, _))| (*id, i)).collect();
    let mut dropped = 0usize;
    let areas = regions
        .iter()
        .map(|(me, r)| {
            let mut links = Vec::new();
            for b in r.get(7).and_then(EsfNode::as_u32_array).unwrap_or_default() {
                match boundaries.get(b).and_then(|&(x, y)| index.get(if x == *me { &y } else { &x })) {
                    Some(&i) => links.push(i),
                    None => dropped += 1,
                }
            }
            AiArea { key: r.get(10).and_then(EsfNode::as_str).unwrap_or_default().to_string(), links }
        })
        .collect();
    if dropped > 0 {
        log::warn!("load: {dropped} CAI region links name no known boundary or area; left out");
    }
    areas
}

/// The campaign AI's factions in a startpos or save ([`World::ai_factions`](ntw_sim::campaign::World::ai_factions)):
/// `CAI_WORLD/CAI_WORLD_FACTIONS[]` in file order (the CAI world's list +0x11C/+0x120), each by its
/// `CAI_FACTION` #6 (the `FACTION` #8 id), kept when the model has that faction. Empty when the
/// file has no CAI state.
pub fn read_ai_factions(root: &EsfRecord, model: &CampaignModel) -> Vec<FactionId> {
    let Some(cai) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE") else { return Vec::new() };
    crate::cai::faction_components(cai).into_iter().map(|(f, _)| FactionId(f as i32)).filter(|f| model.world.factions.contains_key(f)).collect()
}

/// Puts the file's AI keys, region base values, map areas and AI faction order into the model,
/// by the ids of the model's factions and regions with those keys (keys and values naming no
/// faction or region are left out; the areas are kept whole).
pub fn fill(root: &EsfRecord, model: &mut CampaignModel) {
    let factions = read_ai_factions(root, model);
    let keys = read_ai_keys(root);
    let values = read_region_base_values(root);
    let w = &mut model.world;
    w.ai_keys = w.factions.values().filter_map(|f| Some((f.id, keys.get(&f.key)?.clone()))).collect();
    w.region_base_values = w.regions.values().filter_map(|r| Some((r.id, *values.get(&r.key)?))).collect();
    w.ai_areas = read_ai_areas(root);
    w.ai_factions = factions;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> EsfNode {
        EsfNode::Utf16String(v.to_string())
    }

    #[test]
    fn reads_the_four_strings_before_the_flag_records() {
        let flag = EsfNode::Record(Box::new(EsfRecord::new("FACTION_FLAG_AND_COLOURS", 1)));
        let mut rec = EsfRecord::new("FACTION", 18);
        rec.children = vec![EsfNode::I32(1), s("france"), s("France"), s("nap_eur_france"), s("eur_france"), s("default"), s("default"), flag.clone()];
        let k = faction_ai_keys(&rec).expect("keys");
        assert_eq!(k.manager, "nap_eur_france");
        assert_eq!(k.personality, "eur_france");
        assert_eq!(k.extra, ["default".to_string(), "default".to_string()]);
        // Older records: two strings, the others default.
        rec.version = 14;
        rec.children = vec![EsfNode::I32(1), s("x"), s("m"), s("p"), flag];
        let k = faction_ai_keys(&rec).expect("keys");
        assert_eq!((k.manager.as_str(), k.personality.as_str()), ("m", "p"));
    }
}

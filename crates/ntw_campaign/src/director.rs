//! The campaign director's state in an ESF start position or save: read into
//! [`DirectorState`] and written back (CAMPAIGN_FIDELITY.md §Campaign director).
//!
//! Layout (CONFIRMED, loader `0x00B94170`, writer `0x00B9D6E0`, both run by the model loader
//! `0x00872550` for a start position and a save alike): `CAMPAIGN_MODEL/CAMPAIGN_DIRECTOR/CDIR_INTERFACE`,
//! where `CAMPAIGN_DIRECTOR` is a record array of one item (the eur_napoleon start position), version
//! 3 (the writer always writes 3, `0x004CDC50`):
//! - #0 u32: the director pool component's id (+0x1C; the loader fails the load when no such
//!   component exists). Not changed here.
//! - `CDIR_INTERFACE_AUTORESOLVER_MODIFIERS[]` {u32 winner, u32 loser} (+0x34 list): the at-war
//!   desire's pairs.
//! - `CDIR_INTERFACE_REGION_OWNERSHIP_CHANGES[]` {u32 receiver, u32 region, u32 queued owner}
//!   (+0x3C list, 12-byte entries `0x00518210`; read from version 1): the queued transfers.
//! - `CDIR_INTERFACE_UBER_FACTIONS[]` {u32} (+0x54 list; read from version 3): not modelled, kept.
//! - u32: the director's RNG (+0x60; read from version 2). Below version 2 the loader seeds it with
//!   the high 16 bits of one step of the campaign RNG (+0xFB8), so [`fill`] runs after the
//!   portrait allocator's steps (`0x008733CB` comes before `0x00873F3D` in the loader).
//!
//! The ids in these lists and in the XP counters are CAI component ids (CONFIRMED: the director
//! keys them by `0x004CE450`, the component's id), not the model's faction and region ids: a
//! faction's is its `CAI_WORLD/CAI_WORLD_FACTIONS[]` item's (the `CAI_FACTION` #6 id), a
//! region's its `CAI_WORLD_REGIONS[]` item's (the `CAI_REGION` #10 key). [`Components`] maps them
//! both ways; an id it cannot map is dropped (logged once per load or save).
//!
//! The desires' own records sit in the director's BDI pool
//! `CAI_INTERFACE/CAI_INTERFACE_DIRECTOR_BDI_POOL` (CONFIRMED): the region-transfer desire's
//! counter (+0xFC) is `CDIR_DESIRE_TRANSFER_REGION_OWNERSHIP` #0 u32 (loader `0x00B90210`, writer
//! `0x00B9B6C0`); the army XP desire's counters (+0x10C/+0x110 hash map) are
//! `CDIR_DESIRE_UNIT_XP_UPGRADER` (#0 true for armies) #1 `CDIR_DESIRE_UNIT_XP_UPGRADER_FACTION_COUNT[]`
//! {u32 faction component, u32 count}.

use std::collections::BTreeMap;

use ntw_formats::esf::{EsfNode, EsfRecord, EsfRecordArray};
use ntw_sim::campaign::director::{DirectorState, PendingTransfer};
use ntw_sim::campaign::names::lcg_step;
use ntw_sim::campaign::{CampaignModel, FactionId, RegionId};

/// The record array under `CAMPAIGN_MODEL` holding the interface record.
const DIRECTOR: &str = "CAMPAIGN_DIRECTOR";
/// The director interface's record.
const INTERFACE: &str = "CDIR_INTERFACE";
/// The version the original's writer writes.
const INTERFACE_VERSION: u8 = 3;
const AUTORESOLVER_MODIFIERS: &str = "CDIR_INTERFACE_AUTORESOLVER_MODIFIERS";
const REGION_OWNERSHIP_CHANGES: &str = "CDIR_INTERFACE_REGION_OWNERSHIP_CHANGES";
const UBER_FACTIONS: &str = "CDIR_INTERFACE_UBER_FACTIONS";
/// The director's BDI pool under `CAI_INTERFACE`.
const POOL: &str = "CAI_INTERFACE_DIRECTOR_BDI_POOL";
/// The region-transfer desire's record.
const TRANSFER_DESIRE: &str = "CDIR_DESIRE_TRANSFER_REGION_OWNERSHIP";
/// The XP desire's record.
const XP_DESIRE: &str = "CDIR_DESIRE_UNIT_XP_UPGRADER";
/// The XP desire's counters.
const XP_COUNTS: &str = "CDIR_DESIRE_UNIT_XP_UPGRADER_FACTION_COUNT";

/// The CAI component ids of the factions and regions, both ways (see the module docs).
#[derive(Default)]
struct Components {
    factions: BTreeMap<u32, FactionId>,
    regions: BTreeMap<u32, RegionId>,
    /// The factions' components in `CAI_WORLD_FACTIONS[]` order.
    faction_order: Vec<(FactionId, u32)>,
    region_ids: BTreeMap<RegionId, u32>,
    /// Ids that could not be mapped (logged once by the caller).
    unmapped: usize,
}

impl Components {
    /// From a `CAMPAIGN_MODEL` record; empty when it has no CAI world.
    fn read(model_rec: &EsfRecord, model: &CampaignModel) -> Self {
        let mut out = Components::default();
        let Some(cai) = model_rec.child("CAI_INTERFACE") else { return out };
        for (f, c) in crate::cai::faction_components(cai) {
            let f = FactionId(f as i32);
            out.factions.insert(c, f);
            out.faction_order.push((f, c));
        }
        let by_key: BTreeMap<&str, RegionId> = model.world.regions.values().map(|r| (r.key.as_str(), r.id)).collect();
        let items = cai.child("CAI_WORLD").and_then(|w| w.record_array("CAI_WORLD_REGIONS")).map_or(&[][..], |a| a.items.as_slice());
        for item in items {
            let id = item.iter().find(|n| !matches!(n, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_u32);
            let key = item.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_REGION").and_then(|r| r.get(10)).and_then(EsfNode::as_str);
            // The seas, theatres and `all` name no model region.
            if let (Some(id), Some(&r)) = (id, key.and_then(|k| by_key.get(k))) {
                out.regions.insert(id, r);
                out.region_ids.insert(r, id);
            }
        }
        out
    }

    fn faction(&mut self, component: u32) -> Option<FactionId> {
        let f = self.factions.get(&component).copied();
        self.unmapped += usize::from(f.is_none());
        f
    }

    fn region(&mut self, component: u32) -> Option<RegionId> {
        let r = self.regions.get(&component).copied();
        self.unmapped += usize::from(r.is_none());
        r
    }

    fn faction_id(&mut self, f: FactionId) -> Option<u32> {
        let c = self.faction_order.iter().find(|x| x.0 == f).map(|x| x.1);
        self.unmapped += usize::from(c.is_none());
        c
    }

    fn region_id(&mut self, r: RegionId) -> Option<u32> {
        let c = self.region_ids.get(&r).copied();
        self.unmapped += usize::from(c.is_none());
        c
    }
}

/// Reads the file's director state into the model. A file without the interface record (logged
/// once) gets a new director: no queued transfers and the default RNG seed, with the desire
/// counters still read from the file; the exe's new director also takes one campaign-RNG step for
/// its seed, which ours skips (PROVISIONAL, see the `None` arm). A record older than version 2
/// seeds the RNG from the campaign RNG, as the original's loader does ([`seed_from_campaign_rng`]).
pub fn fill(root: &EsfRecord, model: &mut CampaignModel) {
    let Some(model_rec) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL") else { return };
    let mut ids = Components::read(model_rec, model);
    let mut state = DirectorState::default();
    let interface = model_rec.record_array(DIRECTOR).and_then(|d| d.items.iter().flatten().filter_map(EsfNode::as_record).find(|r| r.name == INTERFACE));
    match interface {
        Some(cdir) => {
            if let Some(a) = cdir.record_array(AUTORESOLVER_MODIFIERS) {
                state.autoresolver_pairs = u32s(a).filter_map(|v| Some((ids.faction(*v.first()?)?, ids.faction(*v.get(1)?)?))).collect();
            }
            if cdir.version >= 1 {
                match cdir.record_array(REGION_OWNERSHIP_CHANGES) {
                    Some(a) => state.pending = read_queue(a, &mut ids),
                    None => log::warn!("Campaign director: {INTERFACE} v{} has no {REGION_OWNERSHIP_CHANGES}; no transfers are queued", cdir.version),
                }
            }
            if cdir.version >= 2 {
                match cdir.values().skip(1).find_map(EsfNode::as_u32) {
                    Some(rng) => state.rng.state = rng,
                    None => log::warn!("Campaign director: {INTERFACE} v{} has no RNG; a new director's is used", cdir.version),
                }
            } else {
                // The record's last read (`0x00B94170`, after the queue and the uber factions).
                seed_from_campaign_rng(model, &mut state);
            }
        }
        // PROVISIONAL: the model loader builds a new director instead (`0x0087418C` → constructor
        // `0x00B949B0`), which also seeds its RNG from one campaign-RNG step and builds a new BDI
        // pool (`0x00B95230`). Ours keeps the new director's seed and the file's desire counters:
        // [`write`] cannot add the missing record, so stepping here would step again at each reload
        // of our own save. Every vanilla file has the record.
        None => log::warn!("Campaign director: the file has no {DIRECTOR}/{INTERFACE}; a new director's state is used"),
    }
    let pool = model_rec.child("CAI_INTERFACE").and_then(|c| find(c, POOL, &|_| true));
    match pool.and_then(|p| find(p, TRANSFER_DESIRE, &|_| true)) {
        Some(desire) => state.transfer_counter = desire.get_u32(0).unwrap_or_else(|| {
            log::warn!("Campaign director: {TRANSFER_DESIRE} has no counter; 0 is used");
            0
        }),
        None => log::warn!("Campaign director: the file has no {TRANSFER_DESIRE}; its counter starts at 0"),
    }
    match pool.and_then(|p| find(p, XP_DESIRE, &is_army_xp)).and_then(|d| d.record_array(XP_COUNTS)) {
        Some(a) => state.xp_counters = u32s(a).filter_map(|v| Some((ids.faction(*v.first()?)?, *v.get(1)?))).collect(),
        None => log::warn!("Campaign director: the file has no army {XP_DESIRE} counters; they start empty"),
    }
    if ids.unmapped > 0 {
        log::warn!("Campaign director: {} CAI component ids name no faction or region of the model; their entries are dropped", ids.unmapped);
    }
    model.director = state;
}

/// A record older than version 2 has no RNG: one step of the campaign RNG (+0xFB8), the director's
/// RNG its high 16 bits (`0x00B94170`, its last read, after the queue and the uber factions;
/// CONFIRMED). In the model loader `0x00872550` the director is the last campaign-RNG user, after
/// the portrait allocator's steps (`0x008733BA`; a static scan of its calls three levels deep finds
/// no other), so [`fill`] runs after [`portraits::fill`](crate::portraits::fill).
fn seed_from_campaign_rng(model: &mut CampaignModel, state: &mut DirectorState) {
    model.rng.state = lcg_step(model.rng.state);
    state.rng.state = model.rng.state >> 16;
}

/// The army XP desire's record (#0 true; a navy's would be false).
fn is_army_xp(r: &EsfRecord) -> bool {
    r.get(0).and_then(EsfNode::as_bool) == Some(true)
}

/// Each item's u32s, in order.
fn u32s(a: &EsfRecordArray) -> impl Iterator<Item = Vec<u32>> + '_ {
    a.items.iter().map(|it| it.iter().filter_map(EsfNode::as_u32).collect())
}

/// The queued transfers; an entry without three u32s is skipped (logged once).
fn read_queue(a: &EsfRecordArray, ids: &mut Components) -> Vec<PendingTransfer> {
    let mut short = 0usize;
    let out: Vec<PendingTransfer> = u32s(a)
        .filter_map(|v| {
            let [receiver, region, queued_owner] = v[..] else {
                short += 1;
                return None;
            };
            Some(PendingTransfer { receiver: ids.faction(receiver)?, region: ids.region(region)?, queued_owner: ids.faction(queued_owner)? })
        })
        .collect();
    if short > 0 {
        log::warn!("Campaign director: {short} of {} queued transfers lack their three u32s and are dropped", a.items.len());
    }
    out
}

/// The first record named `name` at or below `r` that `keep` accepts (depth first, file order).
fn find<'a>(r: &'a EsfRecord, name: &str, keep: &dyn Fn(&EsfRecord) -> bool) -> Option<&'a EsfRecord> {
    let mut out = None;
    r.walk(&mut |x| {
        if out.is_none() && x.name == name && keep(x) {
            out = Some(x);
        }
    });
    out
}

/// [`find`], mutable.
fn find_mut<'a>(r: &'a mut EsfRecord, name: &str, keep: &dyn Fn(&EsfRecord) -> bool) -> Option<&'a mut EsfRecord> {
    if r.name == name && keep(r) {
        return Some(r);
    }
    find_in_mut(&mut r.children, name, keep)
}

fn find_in_mut<'a>(children: &'a mut [EsfNode], name: &str, keep: &dyn Fn(&EsfRecord) -> bool) -> Option<&'a mut EsfRecord> {
    for node in children {
        let found = match node {
            EsfNode::Record(r) => find_mut(r, name, keep),
            EsfNode::RecordArray(a) => a.items.iter_mut().find_map(|it| find_in_mut(it, name, keep)),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Writes the model's director state over a `CAMPAIGN_MODEL` record: the interface record as the
/// original's writer lays it out (version 3; the pool id and the uber factions kept from the
/// file), the region-transfer desire's counter and the army XP desire's counters (in the CAI
/// world's faction order). A record the file lacks is not added (logged once per save).
pub(crate) fn write(model_rec: &mut EsfRecord, model: &CampaignModel) {
    let state = &model.director;
    let mut ids = Components::read(model_rec, model);
    let interface = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::RecordArray(d) if d.name == DIRECTOR => d.items.iter_mut().flatten().find_map(|c| match c {
            EsfNode::Record(i) if i.name == INTERFACE => Some(&mut **i),
            _ => None,
        }),
        _ => None,
    });
    match interface {
        Some(cdir) => write_interface(cdir, state, &mut ids),
        None => log::warn!("save: the file has no {DIRECTOR}/{INTERFACE}; the director's RNG and queue are not written"),
    }
    let pool = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(cai) if cai.name == "CAI_INTERFACE" => find_mut(cai, POOL, &|_| true),
        _ => None,
    });
    match pool {
        Some(pool) => write_desires(pool, state, &mut ids),
        None => log::warn!("save: the file has no {POOL}; the director's desire counters are not written"),
    }
    if ids.unmapped > 0 {
        log::warn!("save: {} director entries name a faction or region without a CAI component; they are not written", ids.unmapped);
    }
}

fn write_interface(cdir: &mut EsfRecord, state: &DirectorState, ids: &mut Components) {
    let pool_id = cdir.children.first().cloned().filter(|n| matches!(n, EsfNode::U32(_))).unwrap_or_else(|| {
        log::warn!("save: {INTERFACE} #0 is not the pool id; 0 is written");
        EsfNode::U32(0)
    });
    let uber = cdir.record_array(UBER_FACTIONS).cloned().unwrap_or_else(|| EsfRecordArray::new(UBER_FACTIONS, 0));
    let mut modifiers = EsfRecordArray::new(AUTORESOLVER_MODIFIERS, 0);
    modifiers.items = state
        .autoresolver_pairs
        .iter()
        .filter_map(|&(w, l)| Some(vec![EsfNode::U32(ids.faction_id(w)?), EsfNode::U32(ids.faction_id(l)?)]))
        .collect();
    let mut queue = EsfRecordArray::new(REGION_OWNERSHIP_CHANGES, 0);
    queue.items = state
        .pending
        .iter()
        .filter_map(|p| Some(vec![EsfNode::U32(ids.faction_id(p.receiver)?), EsfNode::U32(ids.region_id(p.region)?), EsfNode::U32(ids.faction_id(p.queued_owner)?)]))
        .collect();
    cdir.version = INTERFACE_VERSION;
    let array = |a: EsfRecordArray| EsfNode::RecordArray(Box::new(a));
    cdir.children = vec![pool_id, array(modifiers), array(queue), array(uber), EsfNode::U32(state.rng.state)];
}

fn write_desires(pool: &mut EsfRecord, state: &DirectorState, ids: &mut Components) {
    match find_mut(pool, TRANSFER_DESIRE, &|_| true) {
        Some(d) if matches!(d.children.first(), Some(EsfNode::U32(_))) => d.children[0] = EsfNode::U32(state.transfer_counter),
        Some(_) => log::warn!("save: {TRANSFER_DESIRE} #0 is not a u32; the director's counter is not written"),
        None => log::warn!("save: the file has no {TRANSFER_DESIRE}; the director's counter is not written"),
    }
    let counts = find_mut(pool, XP_DESIRE, &is_army_xp).and_then(|d| {
        d.children.iter_mut().find_map(|c| match c {
            EsfNode::RecordArray(a) if a.name == XP_COUNTS => Some(&mut **a),
            _ => None,
        })
    });
    match counts {
        Some(a) => {
            a.items = ids.faction_order.iter().filter_map(|&(f, c)| Some(vec![EsfNode::U32(c), EsfNode::U32(*state.xp_counters.get(&f)?)])).collect();
            ids.unmapped += state.xp_counters.keys().filter(|f| !ids.faction_order.iter().any(|x| x.0 == **f)).count();
        }
        None if state.xp_counters.is_empty() => {}
        None => log::warn!("save: the file has no army {XP_DESIRE} counters; the XP desire's counters are not written"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::campaign::{Region, Settlement};
    use ntw_sim::fixed::Fixed20;

    fn rec(name: &str, version: u8, children: Vec<EsfNode>) -> EsfNode {
        EsfNode::Record(Box::new(EsfRecord { name: name.into(), version, children }))
    }

    fn array(name: &str, items: Vec<Vec<EsfNode>>) -> EsfNode {
        EsfNode::RecordArray(Box::new(EsfRecordArray { name: name.into(), version: 0, items }))
    }

    /// The CAI world's factions (component 213 is faction 4, 222 is 9, 225 is 12, 216 is 7) and
    /// regions (component 41 is `eur_a`; 43, a sea, names no model region).
    fn cai_world() -> EsfNode {
        let faction = |c: u32, f: u32| {
            let mut fields = vec![EsfNode::U32(0); 6];
            fields.push(EsfNode::U32(f));
            vec![EsfNode::U32(c), rec("CAI_FACTION", 0, fields)]
        };
        let region = |c: u32, key: &str| {
            let mut fields = vec![EsfNode::U32(0); 10];
            fields.push(EsfNode::AsciiString(key.into()));
            vec![rec("CAI_REGION", 0, fields), EsfNode::U32(c)]
        };
        rec(
            "CAI_WORLD",
            0,
            vec![
                array("CAI_WORLD_FACTIONS", vec![faction(213, 4), faction(222, 9), faction(225, 12), faction(216, 7)]),
                array("CAI_WORLD_REGIONS", vec![region(43, "sea_a"), region(41, "eur_a")]),
            ],
        )
    }

    /// A `CAMPAIGN_SAVE_GAME` root shaped like the original's (the shapes of `orig_fr_may1811.save`).
    fn root(version: u8, interface: Vec<EsfNode>, counter: u32) -> EsfRecord {
        let xp = |army: bool, counts: Vec<Vec<EsfNode>>| rec(XP_DESIRE, 0, vec![EsfNode::Bool(army), array(XP_COUNTS, counts)]);
        let desires = array(
            "CAI_BDI_POOL_DESIRES",
            vec![
                vec![EsfNode::U32(14), rec(TRANSFER_DESIRE, 0, vec![EsfNode::U32(counter)])],
                vec![EsfNode::U32(15), xp(false, vec![vec![EsfNode::U32(222), EsfNode::U32(8)]])],
                vec![EsfNode::U32(16), xp(true, vec![vec![EsfNode::U32(213), EsfNode::U32(3)], vec![EsfNode::U32(999), EsfNode::U32(5)]])],
            ],
        );
        let pool = rec(POOL, 0, vec![rec("CAI_BDI_POOL", 1, vec![EsfNode::Bool(false), desires])]);
        let model = rec(
            "CAMPAIGN_MODEL",
            10,
            vec![rec("CAI_INTERFACE", 13, vec![EsfNode::U32(1), cai_world(), pool]), array(DIRECTOR, vec![vec![rec(INTERFACE, version, interface)]])],
        );
        EsfRecord { name: "CAMPAIGN_SAVE_GAME".into(), version: 0, children: vec![rec("CAMPAIGN_ENV", 0, vec![model])] }
    }

    fn v3(modifiers: Vec<Vec<EsfNode>>, queue: Vec<Vec<EsfNode>>, rng: u32) -> Vec<EsfNode> {
        vec![
            EsfNode::U32(1329),
            array(AUTORESOLVER_MODIFIERS, modifiers),
            array(REGION_OWNERSHIP_CHANGES, queue),
            array(UBER_FACTIONS, vec![vec![EsfNode::U32(216)]]),
            EsfNode::U32(rng),
        ]
    }

    fn u32s(v: &[u32]) -> Vec<EsfNode> {
        v.iter().map(|&x| EsfNode::U32(x)).collect()
    }

    fn model_rec(file: &EsfRecord) -> &EsfRecord {
        file.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL").unwrap()
    }

    fn interface_of(model_rec: &EsfRecord) -> &EsfRecord {
        model_rec.record_array(DIRECTOR).unwrap().items[0][0].as_record().unwrap()
    }

    fn xp_counts(model_rec: &EsfRecord) -> &EsfRecordArray {
        find(model_rec, XP_DESIRE, &is_army_xp).unwrap().record_array(XP_COUNTS).unwrap()
    }

    /// A model with region 5 keyed `eur_a`.
    fn model() -> CampaignModel {
        let mut m = CampaignModel::new(
            ntw_sim::calendar::Calendar::new(ntw_sim::calendar::Date { year: 1805, season: 1, month: 0, half: ntw_sim::calendar::HALF_EARLY }, 0),
            ntw_sim::rng::CaRng::new(7),
            Default::default(),
        );
        let region = Region {
            id: RegionId(5),
            key: "eur_a".into(),
            owner: FactionId(12),
            settlement: Settlement { key: "settlement:eur_a:town".into(), position: (Fixed20::from_raw(0), Fixed20::from_raw(0)) },
            slots: Vec::new(),
            road: None,
            fortification: None,
            road_holder: None,
            fortification_holder: None,
            population: 0,
            base_gdp: 0,
            gdp: 0,
            wealth_growth_offset: 0,
            discontent_growth: 0,
            town_wealth: 0,
            town_wealth_growth: 0,
            town_wealth_refresh: Default::default(),
            commodity_demand: Vec::new(),
            tax_exempt: false,
            religions: Vec::new(),
            class_bases: Vec::new(),
            unit_resources: Vec::new(),
            population_state: Default::default(),
            recruitment_queue: Vec::new(),
            construction: Vec::new(),
            garrison: None,
            fleet: None,
            siege: Default::default(),
            traits: Vec::new(),
        };
        m.world.regions.insert(region.id, region);
        m
    }

    /// The director's records hold CAI component ids, mapped to the model's factions and regions
    /// both ways; an id naming nothing is dropped.
    #[test]
    fn a_save_round_trips_the_director_state() {
        let queue = vec![u32s(&[222, 41, 225]), u32s(&[222, 43, 225]), u32s(&[1, 41, 225])];
        let mut file = root(3, v3(vec![u32s(&[213, 225]), u32s(&[213, 1])], queue, 3_875_805_033), 3);
        let mut m = model();
        fill(&file, &mut m);
        let want = PendingTransfer { receiver: FactionId(9), region: RegionId(5), queued_owner: FactionId(12) };
        assert_eq!((m.director.rng.state, m.director.transfer_counter), (3_875_805_033, 3));
        assert_eq!(m.director.pending, vec![want]);
        assert_eq!(m.director.autoresolver_pairs, vec![(FactionId(4), FactionId(12))]);
        assert_eq!(m.director.xp_counters, [(FactionId(4), 3)].into(), "the army desire's counters; the navy's and an unknown id are not read");
        assert_eq!(m.rng.state, 7, "a version-3 record leaves the campaign RNG alone");

        // The model's state is written back by component id; the pool id and the uber list are
        // kept; the XP counters follow the CAI world's faction order.
        m.director.rng.state = 99;
        m.director.transfer_counter = 0;
        m.director.autoresolver_pairs = vec![(FactionId(12), FactionId(9)), (FactionId(12), FactionId(77))];
        m.director.xp_counters = [(FactionId(7), 2), (FactionId(4), 0), (FactionId(77), 1)].into();
        m.director.pending.push(PendingTransfer { region: RegionId(6), ..want });
        let EsfNode::Record(env) = &mut file.children[0] else { unreachable!() };
        let EsfNode::Record(mrec) = &mut env.children[0] else { unreachable!() };
        write(mrec, &m);
        assert_eq!(interface_of(model_rec(&file)).children, v3(vec![u32s(&[225, 222])], vec![u32s(&[222, 41, 225])], 99));
        assert_eq!(xp_counts(model_rec(&file)).items, vec![u32s(&[213, 0]), u32s(&[216, 2])]);
        let mut again = model();
        fill(&file, &mut again);
        m.director.autoresolver_pairs.pop();
        m.director.pending.pop();
        m.director.xp_counters.remove(&FactionId(77));
        assert_eq!(again.director, m.director);
    }

    /// Below version 2 there is no RNG: the loader seeds it from one step of the campaign RNG
    /// (`0x00B94170`); below version 1 there is no queue either. The writer writes version 3.
    #[test]
    fn an_old_record_seeds_the_rng_from_the_campaign_rng() {
        let file = root(0, vec![EsfNode::U32(1329), array(AUTORESOLVER_MODIFIERS, vec![])], 0);
        let mut m = model();
        fill(&file, &mut m);
        assert_eq!(m.rng.state, lcg_step(7));
        assert_eq!(m.director.rng.state, lcg_step(7) >> 16);
        assert!(m.director.pending.is_empty());

        let mut mrec = model_rec(&file).clone();
        write(&mut mrec, &m);
        let cdir = interface_of(&mrec);
        assert_eq!(cdir.version, 3);
        assert_eq!(cdir.children.len(), 5);
        assert_eq!(cdir.children[4], EsfNode::U32(lcg_step(7) >> 16));
    }
}

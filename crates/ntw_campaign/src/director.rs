//! The campaign director's state in an ESF start position or save: read into
//! [`DirectorState`] and written back (CAMPAIGN_FIDELITY.md §Campaign director).
//!
//! Layout (CONFIRMED, loader `0x00B94170`, writer `0x00B9D6E0`, both run by the model loader
//! `0x00872550` for a start position and a save alike): `CAMPAIGN_MODEL/CAMPAIGN_DIRECTOR/CDIR_INTERFACE`,
//! where `CAMPAIGN_DIRECTOR` is a record array of one item (the eur_napoleon start position), version
//! 3 (the writer always writes 3, `0x004CDC50`):
//! - #0 u32: the director pool component's id (+0x1C; the loader fails the load when no such
//!   component exists). Not changed here.
//! - `CDIR_INTERFACE_AUTORESOLVER_MODIFIERS[]` {u32, u32} (+0x34 list): not modelled, kept.
//! - `CDIR_INTERFACE_REGION_OWNERSHIP_CHANGES[]` {u32 receiver, u32 region, u32 queued owner}
//!   (+0x3C list, 12-byte entries `0x00518210`; read from version 1): the queued transfers.
//! - `CDIR_INTERFACE_UBER_FACTIONS[]` {u32} (+0x54 list; read from version 3): not modelled, kept.
//! - u32: the director's RNG (+0x60; read from version 2). Below version 2 the loader seeds it with
//!   the high 16 bits of one step of the campaign RNG (+0xFB8), so [`fill`] runs after the
//!   portrait allocator's steps (`0x008733CB` comes before `0x00873F3D` in the loader).
//!
//! The region-transfer desire's counter (+0xFC) is the one value of its own record,
//! `CDIR_DESIRE_TRANSFER_REGION_OWNERSHIP` #0 u32 (loader `0x00B90210`, writer `0x00B9B6C0`,
//! CONFIRMED), in the director's BDI pool `CAI_INTERFACE/CAI_INTERFACE_DIRECTOR_BDI_POOL`.

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

/// Reads the file's director state into the model. A file without the interface record keeps a
/// new director's state (logged); one older than version 2 seeds the RNG from the campaign RNG,
/// as the original's loader does.
pub fn fill(root: &EsfRecord, model: &mut CampaignModel) {
    let Some(model_rec) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL") else { return };
    let mut state = DirectorState::default();
    let interface = model_rec.record_array(DIRECTOR).and_then(|d| d.items.iter().flatten().filter_map(EsfNode::as_record).find(|r| r.name == INTERFACE));
    match interface {
        Some(cdir) => {
            if cdir.version >= 1 {
                match cdir.record_array(REGION_OWNERSHIP_CHANGES) {
                    Some(a) => state.pending = read_queue(a),
                    None => log::warn!("Campaign director: {INTERFACE} v{} has no {REGION_OWNERSHIP_CHANGES}; no transfers are queued", cdir.version),
                }
            }
            if cdir.version >= 2 {
                match cdir.values().skip(1).find_map(EsfNode::as_u32) {
                    Some(rng) => state.rng.state = rng,
                    None => log::warn!("Campaign director: {INTERFACE} v{} has no RNG; a new director's is used", cdir.version),
                }
            } else {
                model.rng.state = lcg_step(model.rng.state);
                state.rng.state = model.rng.state >> 16;
            }
        }
        None => log::warn!("Campaign director: the file has no {DIRECTOR}/{INTERFACE}; a new director's state is used"),
    }
    match model_rec.child("CAI_INTERFACE").and_then(|c| find(c, POOL)).and_then(|p| find(p, TRANSFER_DESIRE)) {
        Some(desire) => state.transfer_counter = desire.get_u32(0).unwrap_or_else(|| {
            log::warn!("Campaign director: {TRANSFER_DESIRE} has no counter; 0 is used");
            0
        }),
        None => log::warn!("Campaign director: the file has no {TRANSFER_DESIRE}; its counter starts at 0"),
    }
    model.director = state;
}

/// The queued transfers; an entry without three u32s is skipped (logged once).
fn read_queue(a: &EsfRecordArray) -> Vec<PendingTransfer> {
    let out: Vec<PendingTransfer> = a
        .items
        .iter()
        .filter_map(|it| {
            let mut v = it.iter().filter_map(EsfNode::as_u32);
            let entry = PendingTransfer {
                receiver: FactionId(v.next()? as i32),
                region: RegionId(v.next()?),
                queued_owner: FactionId(v.next()? as i32),
            };
            Some(entry)
        })
        .collect();
    if out.len() < a.items.len() {
        log::warn!("Campaign director: {} of {} queued transfers lack their three u32s and are dropped", a.items.len() - out.len(), a.items.len());
    }
    out
}

/// The first record named `name` at or below `r` (depth first, file order).
fn find<'a>(r: &'a EsfRecord, name: &str) -> Option<&'a EsfRecord> {
    let mut out = None;
    r.walk(&mut |x| {
        if out.is_none() && x.name == name {
            out = Some(x);
        }
    });
    out
}

/// [`find`], mutable.
fn find_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    if r.name == name {
        return Some(r);
    }
    find_in_mut(&mut r.children, name)
}

fn find_in_mut<'a>(children: &'a mut [EsfNode], name: &str) -> Option<&'a mut EsfRecord> {
    for node in children {
        let found = match node {
            EsfNode::Record(r) => find_mut(r, name),
            EsfNode::RecordArray(a) => a.items.iter_mut().find_map(|it| find_in_mut(it, name)),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Writes the model's director state over a `CAMPAIGN_MODEL` record: the interface record as the
/// original's writer lays it out (version 3; the pool id, the autoresolver modifiers and the uber
/// factions kept from the file) and the region-transfer desire's counter. A record the file lacks
/// is not added (logged once per save).
pub(crate) fn write(model_rec: &mut EsfRecord, model: &CampaignModel) {
    let state = &model.director;
    let interface = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::RecordArray(d) if d.name == DIRECTOR => d.items.iter_mut().flatten().find_map(|c| match c {
            EsfNode::Record(i) if i.name == INTERFACE => Some(&mut **i),
            _ => None,
        }),
        _ => None,
    });
    match interface {
        Some(cdir) => write_interface(cdir, state),
        None => log::warn!("save: the file has no {DIRECTOR}/{INTERFACE}; the director's RNG and queue are not written"),
    }
    let desire = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(cai) if cai.name == "CAI_INTERFACE" => find_mut(cai, POOL).and_then(|p| find_mut(p, TRANSFER_DESIRE)),
        _ => None,
    });
    match desire {
        Some(d) if matches!(d.children.first(), Some(EsfNode::U32(_))) => d.children[0] = EsfNode::U32(state.transfer_counter),
        Some(_) => log::warn!("save: {TRANSFER_DESIRE} #0 is not a u32; the director's counter is not written"),
        None => log::warn!("save: the file has no {TRANSFER_DESIRE}; the director's counter is not written"),
    }
}

fn write_interface(cdir: &mut EsfRecord, state: &DirectorState) {
    let pool_id = cdir.children.first().cloned().filter(|n| matches!(n, EsfNode::U32(_))).unwrap_or_else(|| {
        log::warn!("save: {INTERFACE} #0 is not the pool id; 0 is written");
        EsfNode::U32(0)
    });
    let array = |name: &str| {
        let kept = cdir.record_array(name).cloned();
        EsfNode::RecordArray(Box::new(kept.unwrap_or_else(|| EsfRecordArray::new(name, 0))))
    };
    let modifiers = array(AUTORESOLVER_MODIFIERS);
    let uber = array(UBER_FACTIONS);
    let mut queue = EsfRecordArray::new(REGION_OWNERSHIP_CHANGES, 0);
    queue.items = state
        .pending
        .iter()
        .map(|p| vec![EsfNode::U32(p.receiver.raw() as u32), EsfNode::U32(p.region.raw()), EsfNode::U32(p.queued_owner.raw() as u32)])
        .collect();
    cdir.version = INTERFACE_VERSION;
    cdir.children = vec![pool_id, modifiers, EsfNode::RecordArray(Box::new(queue)), uber, EsfNode::U32(state.rng.state)];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(name: &str, version: u8, children: Vec<EsfNode>) -> EsfNode {
        EsfNode::Record(Box::new(EsfRecord { name: name.into(), version, children }))
    }

    fn array(name: &str, items: Vec<Vec<EsfNode>>) -> EsfNode {
        EsfNode::RecordArray(Box::new(EsfRecordArray { name: name.into(), version: 0, items }))
    }

    /// A `CAMPAIGN_SAVE_GAME` root shaped like the original's (the shapes of `orig_fr_may1811.save`).
    fn root(version: u8, interface: Vec<EsfNode>, counter: u32) -> EsfRecord {
        let desires = array("CAI_BDI_POOL_DESIRES", vec![vec![EsfNode::U32(14), rec(TRANSFER_DESIRE, 0, vec![EsfNode::U32(counter)])]]);
        let pool = rec(POOL, 0, vec![rec("CAI_BDI_POOL", 1, vec![EsfNode::Bool(false), desires])]);
        let model = rec(
            "CAMPAIGN_MODEL",
            10,
            vec![rec("CAI_INTERFACE", 13, vec![EsfNode::U32(1), pool]), array(DIRECTOR, vec![vec![rec(INTERFACE, version, interface)]])],
        );
        EsfRecord { name: "CAMPAIGN_SAVE_GAME".into(), version: 0, children: vec![rec("CAMPAIGN_ENV", 0, vec![model])] }
    }

    fn v3(queue: Vec<Vec<EsfNode>>, rng: u32) -> Vec<EsfNode> {
        vec![
            EsfNode::U32(1329),
            array(AUTORESOLVER_MODIFIERS, vec![vec![EsfNode::U32(213), EsfNode::U32(225)]]),
            array(REGION_OWNERSHIP_CHANGES, queue),
            array(UBER_FACTIONS, vec![vec![EsfNode::U32(216)]]),
            EsfNode::U32(rng),
        ]
    }

    fn interface_of(model_rec: &EsfRecord) -> &EsfRecord {
        model_rec.record_array(DIRECTOR).unwrap().items[0][0].as_record().unwrap()
    }

    fn model() -> CampaignModel {
        CampaignModel::new(
            ntw_sim::calendar::Calendar::new(ntw_sim::calendar::Date { year: 1805, season: 1, month: 0, half: ntw_sim::calendar::HALF_EARLY }, 0),
            ntw_sim::rng::CaRng::new(7),
            Default::default(),
        )
    }

    #[test]
    fn a_save_round_trips_the_director_state() {
        let mut file = root(3, v3(vec![vec![EsfNode::U32(222), EsfNode::U32(41), EsfNode::U32(225)]], 3_875_805_033), 3);
        let mut m = model();
        fill(&file, &mut m);
        let want = PendingTransfer { receiver: FactionId(222), region: RegionId(41), queued_owner: FactionId(225) };
        assert_eq!((m.director.rng.state, m.director.transfer_counter), (3_875_805_033, 3));
        assert_eq!(m.director.pending, vec![want]);
        assert_eq!(m.rng.state, 7, "a version-3 record leaves the campaign RNG alone");

        // The model's state is written back; the pool id and the unmodelled lists are kept.
        m.director.rng.state = 99;
        m.director.transfer_counter = 0;
        m.director.pending.clear();
        let mut model_rec = file.children.remove(0);
        let EsfNode::Record(env) = &mut model_rec else { unreachable!() };
        let EsfNode::Record(mrec) = &mut env.children[0] else { unreachable!() };
        write(mrec, &m);
        let back = EsfRecord { name: "CAMPAIGN_SAVE_GAME".into(), version: 0, children: vec![model_rec] };
        let EsfNode::Record(env) = &back.children[0] else { unreachable!() };
        let EsfNode::Record(mrec) = &env.children[0] else { unreachable!() };
        assert_eq!(interface_of(mrec).children, v3(vec![], 99));
        let mut again = model();
        fill(&back, &mut again);
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

        let EsfNode::Record(env) = &file.children[0] else { unreachable!() };
        let EsfNode::Record(mrec) = &env.children[0] else { unreachable!() };
        let mut mrec = (**mrec).clone();
        write(&mut mrec, &m);
        let cdir = interface_of(&mrec);
        assert_eq!(cdir.version, 3);
        assert_eq!(cdir.children.len(), 5);
        assert_eq!(cdir.children[4], EsfNode::U32(lcg_step(7) >> 16));
    }
}

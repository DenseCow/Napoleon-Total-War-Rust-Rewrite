//! Names in a written ESF save (SAVE_COMPAT.md §14, §21). The model names new characters and unit
//! officers when it creates them ([`ntw_sim::campaign::names`]); this writer only writes what the
//! model holds: each new unit's officer name into its `COMMANDER_DETAILS` (a new character's name
//! goes with his `CHARACTER_DETAILS`, `save::write_new_details`) and each faction's name decks into
//! its `NAME_ALLOCATION_DETAILS`. A model built without the names table leaves new characters and
//! officers with the template's names and the decks as the source has them.

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::campaign::CampaignModel;

use crate::names::{write_allocator, Allocator};

fn set_loc(r: Option<&mut EsfNode>, key: &str) {
    if let Some(EsfNode::Record(c)) = r
        && let Some(n @ EsfNode::Utf16String(_)) = c.children.first_mut()
    {
        *n = EsfNode::Utf16String(key.to_string());
    }
}

/// Writes the model's officer names of the new units (`new_units`) and every faction's name decks
/// into the factions of a `WORLD` record.
pub(crate) fn write_model_names(world: &mut EsfRecord, model: &CampaignModel, new_units: &BTreeSet<i32>) {
    let officers: BTreeMap<i32, &(String, String)> = model
        .world
        .forces
        .values()
        .flat_map(|f| &f.units)
        .filter(|u| new_units.contains(&u.id.raw()) && !u.officer_name.0.is_empty())
        .map(|u| (u.id.raw(), &u.officer_name))
        .collect();
    let decks: BTreeMap<&str, &[Allocator]> =
        model.world.name_allocators.iter().filter_map(|(f, a)| Some((model.world.factions.get(f)?.key.as_str(), a.as_slice()))).collect();
    for c in world.children.iter_mut() {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "FACTION_ARRAY" {
            continue;
        }
        for f in a.items.iter_mut().flat_map(|it| it.iter_mut()) {
            let EsfNode::Record(f) = f else { continue };
            write_faction(f, &officers, &decks);
        }
    }
}

fn write_faction(f: &mut EsfRecord, officers: &BTreeMap<i32, &(String, String)>, decks: &BTreeMap<&str, &[Allocator]>) {
    let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
    // The decks, in save order.
    if let Some(deck) = decks.get(key.as_str()) {
        let records = f.children.iter_mut().filter_map(|c| match c {
            EsfNode::Record(r) if r.name == "NAME_ALLOCATION_DETAILS" => Some(&mut **r),
            _ => None,
        });
        for (r, a) in records.zip(deck.iter()) {
            write_allocator(a, r);
        }
    }
    if officers.is_empty() {
        return;
    }
    // Officers of new units.
    for c in f.children.iter_mut() {
        let EsfNode::RecordArray(arr) = c else { continue };
        if arr.name != "ARMY_ARRAY" {
            continue;
        }
        for force in arr.items.iter_mut().flat_map(|it| it.iter_mut()) {
            let EsfNode::Record(force) = force else { continue };
            for u in force.children.iter_mut() {
                let EsfNode::RecordArray(units) = u else { continue };
                if units.name != "UNITS_ARRAY" {
                    continue;
                }
                for w in units.items.iter_mut().flat_map(|it| it.iter_mut()) {
                    let EsfNode::Record(w) = w else { continue };
                    let Some(unit) = w.children.iter_mut().find_map(|n| match n {
                        EsfNode::Record(x) if x.name == "UNIT" => Some(&mut **x),
                        _ => None,
                    }) else {
                        continue;
                    };
                    let Some((fk, sk)) = unit.get_i32(4).and_then(|id| officers.get(&id)).map(|n| (n.0.clone(), n.1.clone())) else { continue };
                    if let Some(cd) = unit.children.iter_mut().find_map(|n| match n {
                        EsfNode::Record(x) if x.name == "COMMANDER_DETAILS" => Some(&mut **x),
                        _ => None,
                    }) {
                        set_loc(cd.children.get_mut(0), &fk);
                        set_loc(cd.children.get_mut(1), &sk);
                    }
                }
            }
        }
    }
}

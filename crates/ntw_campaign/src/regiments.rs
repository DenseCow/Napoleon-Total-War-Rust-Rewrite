//! Regiment and ship names of new units (SAVE_COMPAT.md §20).
//!
//! Each faction holds `LAND_UNIT_NAME_ALLOCATOR / LAND_UNIT_NAMES_MAP[]` items {u32 unit class,
//! `UNIT_CLASS_NAME_ALLOCATOR` {`UNIT_CLASS_NAMES_LIST[]` {`CAMPAIGN_LOCALISATION` name, bool in
//! use}, `CAMPAIGN_LOCALISATION` next}} and `NAVAL_UNIT_NAME_ALLOCATOR` {bool,
//! `UNIT_CLASS_NAME_ALLOCATOR`} (one list for every ship). What the original's vanilla saves show:
//! - the class number is the row index of the `unit_class` table (CONFIRMED: every list's names and
//!   trailing name are that class's `unit_regiment_names` rows, `regiment_names` example);
//! - a list's in-use flags are exactly the names the faction's units carry (CONFIRMED in all 6
//!   original vanilla saves: no flag without a unit, no unit name without its flag);
//! - a new unit takes the lowest free name of its class's list (INFERRED from consecutive saves,
//!   ~99% of the flag changes; the rest are names taken and freed inside one interval);
//! - a class without a free name gives the trailing name, or no name ("", "") when the class has
//!   no list and no trailing name (skirmishers, generals) (INFERRED).
//!
//! The writer applies this after the units are written: new units get a name, then every flag is
//! set from the names carried (so the names of units that are gone are freed).

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_sim::campaign::CampaignModel;

/// The `unit_class` table's rows in file order (CONFIRMED vanilla `data.pack`): the class numbers
/// of `LAND_UNIT_NAMES_MAP`.
pub const UNIT_CLASSES: [&str; 45] = [
    "artillery_fixed",
    "artillery_foot",
    "artillery_horse",
    "cavalry_camels",
    "cavalry_heavy",
    "cavalry_irregular",
    "cavalry_lancers",
    "cavalry_light",
    "cavalry_missile",
    "cavalry_standard",
    "dragoons",
    "elephants",
    "general",
    "infantry_berserker",
    "infantry_elite",
    "infantry_grenadiers",
    "infantry_irregulars",
    "infantry_light",
    "infantry_line",
    "infantry_melee",
    "infantry_militia",
    "infantry_mob",
    "infantry_skirmishers",
    "naval_admiral",
    "naval_bomb_ketch",
    "naval_brig",
    "naval_dhow",
    "naval_fifth_rate",
    "naval_first_rate",
    "naval_fourth_rate",
    "naval_galleon",
    "naval_heavy_galley",
    "naval_indiaman",
    "naval_light_galley",
    "naval_lugger",
    "naval_medium_galley",
    "naval_over_first_rate",
    "naval_razee",
    "naval_rocket_ship",
    "naval_second_rate",
    "naval_sixth_rate",
    "naval_sloop",
    "naval_steam_ship",
    "naval_third_rate",
    "naval_xebec",
];

/// The class number of a `unit_class` key.
pub fn class_index(class: &str) -> Option<u32> {
    UNIT_CLASSES.iter().position(|c| *c == class).map(|i| i as u32)
}

fn rec_mut<'a>(r: &'a mut EsfRecord, name: &str) -> Option<&'a mut EsfRecord> {
    r.children.iter_mut().find_map(|c| match c {
        EsfNode::Record(x) if x.name == name => Some(&mut **x),
        _ => None,
    })
}

fn loc_key(r: &EsfRecord) -> String {
    r.get_str(0).unwrap_or("").to_string()
}

/// One name list: its entries (name record, flag) and the trailing name.
struct List<'a> {
    entries: Vec<(&'a EsfRecord, &'a mut bool)>,
    next: Option<&'a EsfRecord>,
}

fn list_of(alloc: &mut EsfRecord) -> List<'_> {
    let mut entries = Vec::new();
    let mut next = None;
    for c in alloc.children.iter_mut() {
        match c {
            EsfNode::RecordArray(a) if a.name == "UNIT_CLASS_NAMES_LIST" => {
                for it in a.items.iter_mut() {
                    let (name, flag) = it.split_at_mut(1);
                    if let (Some(EsfNode::Record(n)), Some(EsfNode::Bool(b))) = (name.first(), flag.first_mut()) {
                        entries.push((&**n, b));
                    }
                }
            }
            EsfNode::Record(r) if r.name == "CAMPAIGN_LOCALISATION" => next = Some(&**r),
            _ => {}
        }
    }
    List { entries, next }
}

/// A unit of a faction record: its id, key, naval flag and its name record (`UNIT` #14).
struct UnitAt<'a> {
    id: i32,
    key: String,
    naval: bool,
    name: &'a mut EsfRecord,
}

fn units_of(faction: &mut EsfRecord) -> Vec<UnitAt<'_>> {
    let mut out = Vec::new();
    for c in faction.children.iter_mut() {
        let EsfNode::RecordArray(armies) = c else { continue };
        if armies.name != "ARMY_ARRAY" {
            continue;
        }
        for force in armies.items.iter_mut().flat_map(|it| it.iter_mut()) {
            let EsfNode::Record(force) = force else { continue };
            for u in force.children.iter_mut() {
                let EsfNode::RecordArray(units) = u else { continue };
                if units.name != "UNITS_ARRAY" {
                    continue;
                }
                for w in units.items.iter_mut().flat_map(|it| it.iter_mut()) {
                    let EsfNode::Record(w) = w else { continue };
                    let naval = w.name == "NAVAL_UNIT";
                    let Some(unit) = rec_mut(w, "UNIT") else { continue };
                    let id = unit.get_i32(4).unwrap_or(0);
                    let key = unit.child("UNIT_RECORD_KEY").map(loc_key).unwrap_or_default();
                    let name = unit.children.iter_mut().find_map(|c| match c {
                        EsfNode::Record(x) if x.name == "CAMPAIGN_LOCALISATION" => Some(&mut **x),
                        _ => None,
                    });
                    if let Some(name) = name {
                        out.push(UnitAt { id, key, naval, name });
                    }
                }
            }
        }
    }
    out
}

/// Names the new units (`new` = unit ids not in the source) of every faction of a `WORLD` record
/// and sets every name list's flags from the names the faction's units carry.
pub(crate) fn write_regiment_names(world: &mut EsfRecord, model: &CampaignModel, new: &BTreeSet<i32>) {
    for c in world.children.iter_mut() {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "FACTION_ARRAY" {
            continue;
        }
        for f in a.items.iter_mut().flat_map(|it| it.iter_mut()) {
            let EsfNode::Record(f) = f else { continue };
            write_faction(f, model, new);
        }
    }
}

fn write_faction(f: &mut EsfRecord, model: &CampaignModel, new: &BTreeSet<i32>) {
    // Take the name lists out of the record while the units are borrowed.
    let land_pos = f.children.iter().position(|c| matches!(c, EsfNode::Record(r) if r.name == "LAND_UNIT_NAME_ALLOCATOR"));
    let naval_pos = f.children.iter().position(|c| matches!(c, EsfNode::Record(r) if r.name == "NAVAL_UNIT_NAME_ALLOCATOR"));
    let mut land = land_pos.map(|i| std::mem::replace(&mut f.children[i], EsfNode::Bool(false)));
    let mut naval = naval_pos.map(|i| std::mem::replace(&mut f.children[i], EsfNode::Bool(false)));
    {
        // class -> list
        let mut lists: BTreeMap<Option<u32>, List<'_>> = BTreeMap::new();
        if let Some(EsfNode::Record(l)) = land.as_mut() {
            for c in l.children.iter_mut() {
                let EsfNode::RecordArray(m) = c else { continue };
                if m.name != "LAND_UNIT_NAMES_MAP" {
                    continue;
                }
                for it in m.items.iter_mut() {
                    let class = it.first().and_then(EsfNode::as_u32);
                    if let Some(alloc) = it.iter_mut().find_map(|n| match n {
                        EsfNode::Record(r) if r.name == "UNIT_CLASS_NAME_ALLOCATOR" => Some(&mut **r),
                        _ => None,
                    }) && let Some(class) = class
                    {
                        lists.insert(Some(class), list_of(alloc));
                    }
                }
            }
        }
        if let Some(EsfNode::Record(n)) = naval.as_mut()
            && let Some(alloc) = rec_mut(n, "UNIT_CLASS_NAME_ALLOCATOR")
        {
            lists.insert(None, list_of(alloc));
        }
        let mut units = units_of(f);
        let mut carried: BTreeSet<String> = units.iter().filter(|u| !new.contains(&u.id)).map(|u| loc_key(u.name)).collect();
        for u in units.iter_mut().filter(|u| new.contains(&u.id)) {
            let class = if u.naval {
                None
            } else {
                match model.rules.units.get(&u.key).and_then(|r| class_index(&r.unit_class)) {
                    Some(c) => Some(c),
                    None => continue,
                }
            };
            let Some(list) = lists.get_mut(&class) else {
                // No list for the class: no name.
                *u.name = EsfRecord { name: "CAMPAIGN_LOCALISATION".into(), version: u.name.version, children: vec![EsfNode::Utf16String(String::new()), EsfNode::Utf16String(String::new())] };
                continue;
            };
            let free = list.entries.iter().find(|(n, _)| !carried.contains(&loc_key(n)));
            let pick = match free {
                Some((n, _)) => Some((*n).clone()),
                None => list.next.filter(|n| !loc_key(n).is_empty()).cloned(),
            };
            match pick {
                Some(n) => {
                    carried.insert(loc_key(&n));
                    *u.name = n;
                }
                None => {
                    *u.name = EsfRecord { name: "CAMPAIGN_LOCALISATION".into(), version: u.name.version, children: vec![EsfNode::Utf16String(String::new()), EsfNode::Utf16String(String::new())] };
                }
            }
        }
        for list in lists.values_mut() {
            for (n, b) in list.entries.iter_mut() {
                **b = carried.contains(&loc_key(n));
            }
        }
    }
    if let (Some(i), Some(l)) = (land_pos, land) {
        f.children[i] = l;
    }
    if let (Some(i), Some(n)) = (naval_pos, naval) {
        f.children[i] = n;
    }
}

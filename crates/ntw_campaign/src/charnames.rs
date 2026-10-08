//! Names of new characters and unit officers in a written save (SAVE_COMPAT.md §14, §21).
//!
//! The original's algorithm (CONFIRMED in the exe; the random stream it runs on is PROVISIONAL:
//! exact names would need the original's own stream and event order):
//! - a new character or officer takes a forename from the faction's allocator 0 (male forenames)
//!   and a surname from allocator 4 (surnames): each draw takes the first deck entry, refilling and
//!   reshuffling an empty deck (`0x008A9FA0`, [`Allocator`]); the entry indexes the pool
//!   ([`pool_rows`], the builder `0x00F74CB0`);
//! - the naming routine `0x009940A0` keeps the pair unless "forename surname" equals the on-screen
//!   name of a historical character (`0x00A28CC0`); then it picks again, up to 1000 times, from the
//!   faction record's pools with the world's random state (world +0xFB8, saved as `RandSeed`):
//!   one step of the MS LCG per name, and `0x008AA290` turns the step's high 16 bits into two
//!   further steps, the second giving the stored key's index `(r >> 16) × n / 0xFFFF` (clamped)
//!   and the first the display text's index (a quirk of the original; the stored key is the one
//!   that counts);
//! - an allocator whose stored size is not its pool's size (stale start-position sizes) starts with
//!   that size and an empty deck, keeping its seed (the original's loader `0x0085EF80`, INFERRED).
//!
//! A new colonel / captain and the officer of the unit it is attached to share the name (as in the
//! original's saves, e.g. a colonel and his unit both "Andre Guadian"); other new units get their
//! own draw (INFERRED path for unit officers).

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfNode, EsfRecord};

use crate::names::{lcg_step, pool_rows, Allocator, NameData, NameRow, POOL_MALE_FORENAME, POOL_SURNAME};

/// The most picks the naming routine makes (CONFIRMED loop bound in `0x009940A0`).
const MAX_TRIES: u32 = 1000;

/// One faction's namer: its two pools and allocators.
pub struct FactionNamer<'a> {
    data: &'a NameData,
    fore: Vec<&'a NameRow>,
    sur: Vec<&'a NameRow>,
    /// Allocators 0 and 4 as loaded (stale sizes fixed).
    pub alloc_fore: Allocator,
    /// See `alloc_fore`.
    pub alloc_sur: Allocator,
}

/// `0x008AA290`: an index of a pool of `n` from the high bits `p` of a world-random step; returns
/// (display index, key index).
fn pool_pick(n: usize, p: u32) -> (usize, usize) {
    let u = lcg_step(p);
    let a = (((u >> 16) as u64 * n as u64) / 0xFFFF) as usize;
    let b = (((lcg_step(u) >> 16) as u64 * n as u64) / 0xFFFF) as usize;
    (a.min(n - 1), b.min(n - 1))
}

impl<'a> FactionNamer<'a> {
    /// The namer of the faction `key` from its stored allocators, or `None` without pools.
    pub fn new(data: &'a NameData, key: &str, allocs: &[Allocator]) -> Option<FactionNamer<'a>> {
        let group = data.groups.get(key)?;
        let fore = pool_rows(&data.rows, group, POOL_MALE_FORENAME)?;
        let sur = pool_rows(&data.rows, group, POOL_SURNAME)?;
        if fore.is_empty() || sur.is_empty() {
            return None;
        }
        let fix = |a: &Allocator, n: usize| {
            let mut a = a.clone();
            if a.size as usize != n {
                a.size = n as u32;
                a.deck.clear();
            }
            a
        };
        Some(FactionNamer {
            data,
            alloc_fore: fix(allocs.get(POOL_MALE_FORENAME)?, fore.len()),
            alloc_sur: fix(allocs.get(POOL_SURNAME)?, sur.len()),
            fore,
            sur,
        })
    }

    /// A (forename key, surname key) pair: deck draws, then world-random picks while the name is a
    /// historical character's (`world_seed` advances only then).
    pub fn draw(&mut self, world_seed: &mut u32) -> (String, String) {
        let f = self.alloc_fore.draw().map_or(0, usize::from).min(self.fore.len() - 1);
        let s = self.alloc_sur.draw().map_or(0, usize::from).min(self.sur.len() - 1);
        let (mut fk, mut ft) = (self.fore[f].loc_key(), self.data.text_of(self.fore[f]));
        let (mut sk, mut st) = (self.sur[s].loc_key(), self.data.text_of(self.sur[s]));
        let mut tries = 0;
        while self.data.historical.contains(&format!("{ft} {st}")) && tries < MAX_TRIES {
            tries += 1;
            *world_seed = lcg_step(*world_seed);
            let (a, b) = pool_pick(self.fore.len(), *world_seed >> 16);
            fk = self.fore[b].loc_key();
            ft = self.data.text_of(self.fore[a]);
            *world_seed = lcg_step(*world_seed);
            let (a, b) = pool_pick(self.sur.len(), *world_seed >> 16);
            sk = self.sur[b].loc_key();
            st = self.data.text_of(self.sur[a]);
        }
        (fk, sk)
    }
}

fn set_loc(r: Option<&mut EsfNode>, key: &str) {
    if let Some(EsfNode::Record(c)) = r
        && let Some(n @ EsfNode::Utf16String(_)) = c.children.first_mut()
    {
        *n = EsfNode::Utf16String(key.to_string());
    }
}

/// Names every new character (`new_chars`) and the officer of every new unit (`new_units`) of
/// each faction of a `WORLD` record, and writes the advanced allocators back. Returns the world
/// random state after any re-picks.
pub(crate) fn name_new_objects(world: &mut EsfRecord, data: &NameData, new_chars: &BTreeSet<i32>, new_units: &BTreeSet<i32>, mut world_seed: u32) -> u32 {
    for c in world.children.iter_mut() {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "FACTION_ARRAY" {
            continue;
        }
        for f in a.items.iter_mut().flat_map(|it| it.iter_mut()) {
            let EsfNode::Record(f) = f else { continue };
            name_faction(f, data, new_chars, new_units, &mut world_seed);
        }
    }
    world_seed
}

fn name_faction(f: &mut EsfRecord, data: &NameData, new_chars: &BTreeSet<i32>, new_units: &BTreeSet<i32>, world_seed: &mut u32) {
    let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
    // Anything to name?
    let mut any = false;
    f.walk(&mut |r: &EsfRecord| {
        if (r.name == "CHARACTER" && r.get_i32(2).is_some_and(|id| new_chars.contains(&id))) || (r.name == "UNIT" && r.get_i32(4).is_some_and(|id| new_units.contains(&id))) {
            any = true;
        }
    });
    if !any {
        return;
    }
    let allocs: Vec<Allocator> = f.children_named("NAME_ALLOCATION_DETAILS").filter_map(Allocator::read).collect();
    let Some(mut namer) = FactionNamer::new(data, &key, &allocs) else { return };
    // Officer names of the units that are not new: a new colonel / captain who takes over such a
    // unit (a dead commander's force, `CampaignModel::character_dies`) gets its officer's name, as
    // the original's colonels share their unit officer's name.
    let mut officers: BTreeMap<u32, (String, String)> = BTreeMap::new();
    f.walk(&mut |r: &EsfRecord| {
        if r.name == "UNIT"
            && let Some(id) = r.get_i32(4).filter(|id| !new_units.contains(id))
            && let Some(cd) = r.child("COMMANDER_DETAILS")
        {
            let name = |i: usize| cd.get(i).and_then(EsfNode::as_record).and_then(|l| l.get_str(0)).unwrap_or("").to_string();
            officers.insert(id as u32, (name(0), name(1)));
        }
    });
    // Characters first (in file order), remembering each name for the unit it is attached to.
    let mut by_char: BTreeMap<i32, (String, String)> = BTreeMap::new();
    for c in f.children.iter_mut() {
        let EsfNode::RecordArray(arr) = c else { continue };
        if arr.name != "CHARACTER_ARRAY" {
            continue;
        }
        for ch in arr.items.iter_mut().flat_map(|it| it.iter_mut()) {
            let EsfNode::Record(ch) = ch else { continue };
            if ch.name != "CHARACTER" {
                continue;
            }
            let Some(id) = ch.get_i32(2).filter(|id| new_chars.contains(id)) else { continue };
            let own = ch.get_u32(5).and_then(|u| officers.get(&u)).filter(|n| !n.0.is_empty() || !n.1.is_empty()).cloned();
            let (fk, sk) = match own {
                Some(n) => n,
                None => namer.draw(world_seed),
            };
            if let Some(EsfNode::Record(d)) = ch.children.get_mut(1) {
                set_loc(d.children.get_mut(1), &fk);
                set_loc(d.children.get_mut(2), &sk);
            }
            by_char.insert(id, (fk, sk));
        }
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
                    let Some(_) = unit.get_i32(4).filter(|id| new_units.contains(id)) else { continue };
                    let attached = unit.get_u32(10).unwrap_or(0) as i32;
                    let (fk, sk) = match by_char.get(&attached) {
                        Some(n) => n.clone(),
                        None => namer.draw(world_seed),
                    };
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
    // The advanced allocators back into their records (0 and 4 in save order).
    let mut k = 0;
    for c in f.children.iter_mut() {
        let EsfNode::Record(r) = c else { continue };
        if r.name != "NAME_ALLOCATION_DETAILS" {
            continue;
        }
        if k == POOL_MALE_FORENAME {
            namer.alloc_fore.write(r);
        } else if k == POOL_SURNAME {
            namer.alloc_sur.write(r);
        }
        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, forename: bool) -> NameRow {
        NameRow { group: "g".into(), name: name.into(), forename, gender: 'b', weight: 1, noble: false, id: String::new() }
    }

    #[test]
    fn a_historical_name_is_picked_again_with_the_world_random_state() {
        let mut data = NameData { rows: vec![row("Michel", true), row("Jean", true), row("Ney", false), row("Lannes", false)], ..NameData::default() };
        data.groups.insert("f".into(), "g".into());
        // Deck: forename 0 (Michel), surname 0 (Ney) first: "Michel Ney" is historical.
        data.historical.insert("Michel Ney".into());
        let alloc = |deck: Vec<u16>| Allocator { size: 2, seed: 7, deck };
        let allocs = vec![alloc(vec![0, 1]), alloc(vec![]), alloc(vec![]), alloc(vec![]), alloc(vec![0, 1])];
        let mut n = FactionNamer::new(&data, "f", &allocs).unwrap();
        let mut seed = 12345;
        let (f, s) = n.draw(&mut seed);
        assert_ne!(seed, 12345, "the world random state was used");
        // The stored keys come from the pools (the key index is the second step's, so with a pool
        // of two it may repeat the historical pair: the original's quirk, see the module docs).
        assert!(["names_name_gMichel", "names_name_gJean"].contains(&f.as_str()));
        assert!(["names_name_gNey", "names_name_gLannes"].contains(&s.as_str()));
        // The deck draws happened once each.
        assert_eq!((n.alloc_fore.deck.clone(), n.alloc_sur.deck.clone()), (vec![1], vec![1]));
        // Without a historical clash the world random state is untouched.
        let mut seed2 = 99;
        let (f2, s2) = n.draw(&mut seed2);
        assert_eq!((seed2, f2.as_str(), s2.as_str()), (99, "names_name_gJean", "names_name_gLannes"));
    }
}

//! Character names as the original allocates them (SAVE_COMPAT.md §14).
//!
//! **Table** `names` (CONFIRMED layout, 13 945 rows): {group key (FK `names_groups`), the name,
//! type `forename` / `surname`, gender `m` / `f` / `b` (both), i32 (0, 1 or 2; 0 = not used for
//! random names, e.g. Bonaparte, Bernadotte), bool, id string}. A character stores the
//! localisation key `names_name_<group><name>` (CONFIRMED in every save, e.g.
//! `names_name_names_frenchJoachim`; the builder `0x00F44290` formats `names_name_%S`).
//!
//! **Allocators** (CONFIRMED in the exe): each faction has ten name allocators (faction
//! +0x530 ..), saved as its ten `NAME_ALLOCATION_DETAILS` {u32 pool size, u32 seed, u16[] deck} in
//! the order of the pools. A draw (`0x008A9FA0`) takes the deck's first entry and removes it
//! (`0x008D3510`, `0x008BD9F0`); an empty deck is refilled (`0x008EFB70`) with 0 .. size − 1 and
//! shuffled: the stored seed is advanced once by the MS LCG `seed × 214013 + 2531011`, its high 16
//! bits start the shuffle's own LCG run (`0x0086EA70`: for i = 1 .. n − 1 draw `r = high 16 bits`
//! until it is not in the top partial range of `m = i + 1`, swap entry i with entry `r mod m`
//! unless that is i). The pool membership (which rows feed which allocator) is INFERRED from the
//! pool sizes stored in the saves ([`pool_rows`], `names_check`).

use ntw_data::GameDatabase;
use ntw_formats::db::DbValue;
use ntw_formats::db_folder::{TableError, tables};
use ntw_formats::esf::{EsfNode, EsfRecord};
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::names::{NameEntry, NamePools, NameRules};
use ntw_sim::campaign::CampaignModel;

/// One row of `names`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameRow {
    /// `names_groups` key, e.g. `names_french`.
    pub group: String,
    /// The name text.
    pub name: String,
    /// True for a forename, false for a surname.
    pub forename: bool,
    /// `m`, `f` or `b`.
    pub gender: char,
    /// 0 = not used for random names (INFERRED from the rows: historical names).
    pub weight: i32,
    /// The bool column (INFERRED: noble names, e.g. "de La Fayette").
    pub noble: bool,
    /// The last column (a numeric id string).
    pub id: String,
}

impl NameRow {
    /// The localisation key a character stores.
    pub fn loc_key(&self) -> String {
        // Spaces become underscores (CONFIRMED: "de Lamoignon" is stored as `..._frenchde_Lamoignon`).
        format!("names_name_{}{}", self.group, self.name.replace(' ', "_"))
    }
}

/// The `names` table, in file order (every file of it, mods included: the merged table reader).
pub fn read_names(vfs: &Vfs) -> Result<Vec<NameRow>, TableError> {
    let rows = tables::NAMES.read(vfs)?;
    let s = |v: &DbValue| match v {
        DbValue::Str(x) => x.clone(),
        _ => String::new(),
    };
    Ok(
        rows
            .iter()
            .map(|r| NameRow {
                group: s(&r[0]),
                name: s(&r[1]),
                forename: s(&r[2]) == "forename",
                gender: s(&r[3]).chars().next().unwrap_or('b'),
                weight: match r[4] {
                    DbValue::I32(v) => v,
                    _ => 0,
                },
                noble: matches!(r[5], DbValue::Bool(true)),
                id: s(&r[6]),
            })
            .collect(),
    )
}

/// The number of allocators per faction.
pub const POOLS: usize = 10;
/// Male forenames (save order 0).
pub const POOL_MALE_FORENAME: usize = ntw_sim::campaign::names::POOL_MALE_FORENAME;
/// Female forenames (save order 1).
pub const POOL_FEMALE_FORENAME: usize = 1;
/// Noble male forenames (save order 2).
pub const POOL_NOBLE_MALE_FORENAME: usize = 2;
/// Noble female forenames (save order 3).
pub const POOL_NOBLE_FEMALE_FORENAME: usize = 3;
/// Surnames (save order 4).
pub const POOL_SURNAME: usize = ntw_sim::campaign::names::POOL_SURNAME;
/// Noble surnames (save order 5).
pub const POOL_NOBLE_SURNAME: usize = 5;
/// Always empty in every save (save order 6).
pub const POOL_EMPTY: usize = 6;

/// The entries of pool `pool` (save order) of names group `group`: the matching rows in table
/// order, each repeated `weight` times (rows of weight 0 are left out). The rule reproduces every
/// stored pool size in the original saves except stale startpos sizes, and is the original's
/// builder `0x00F74CB0` (CONFIRMED: names table order, each name pushed `weight` times; SAVE_COMPAT.md §14,
/// `names_fit`, INFERRED from the sizes; the index → name order is checked against the names the
/// original drew, `names_draws`). `None` for pools 7 .. 9, whose source is UNKNOWN (not `names`).
pub fn pool_rows<'a>(names: &'a [NameRow], group: &str, pool: usize) -> Option<Vec<&'a NameRow>> {
    let rule: fn(&NameRow) -> bool = match pool {
        POOL_MALE_FORENAME => |r| r.forename && !r.noble && r.gender != 'f',
        POOL_FEMALE_FORENAME => |r| r.forename && !r.noble && r.gender != 'm',
        POOL_NOBLE_MALE_FORENAME => |r| r.forename && r.noble && r.gender != 'f',
        POOL_NOBLE_FEMALE_FORENAME => |r| r.forename && r.noble && r.gender != 'm',
        POOL_SURNAME => |r| !r.forename && !r.noble,
        // `f`/`b` and all genders fit equally (no male-only noble surname has a weight).
        POOL_NOBLE_SURNAME => |r| !r.forename && r.noble,
        POOL_EMPTY => |_| false,
        _ => return None,
    };
    Some(
        names
            .iter()
            .filter(|r| r.group == group && r.weight > 0 && rule(r))
            .flat_map(|r| std::iter::repeat_n(r, r.weight as usize))
            .collect(),
    )
}

/// One allocator: the stored state of a `NAME_ALLOCATION_DETAILS` (the model's type, whose draw
/// rule is the model's, [`ntw_sim::campaign::names`]).
pub use ntw_sim::campaign::names::NameAllocator as Allocator;

/// An allocator from a `NAME_ALLOCATION_DETAILS` record.
pub fn read_allocator(r: &EsfRecord) -> Option<Allocator> {
    Some(Allocator {
        size: r.get_u32(0)?,
        seed: r.get_u32(1)?,
        deck: match r.get(2)? {
            EsfNode::U16Array(v) => v.clone(),
            _ => return None,
        },
    })
}

/// Writes an allocator's state back into a `NAME_ALLOCATION_DETAILS` record.
pub fn write_allocator(a: &Allocator, r: &mut EsfRecord) {
    if let Some(n @ EsfNode::U32(_)) = r.children.get_mut(0) {
        *n = EsfNode::U32(a.size);
    }
    if let Some(n @ EsfNode::U32(_)) = r.children.get_mut(1) {
        *n = EsfNode::U32(a.seed);
    }
    if let Some(n @ EsfNode::U16Array(_)) = r.children.get_mut(2) {
        *n = EsfNode::U16Array(a.deck.clone());
    }
}

/// The faction's character names group (`factions` #6).
pub fn faction_group(db: &GameDatabase, faction_key: &str) -> Option<String> {
    db.faction(faction_key).map(|f| f.character_names_group.clone())
}

/// MS LCG step (the original's `rand` constants).
pub use ntw_sim::campaign::names::lcg_step;

/// What the save writer needs to name new characters and unit officers: the `names` rows, each
/// faction's names group, and the on-screen names of the historical characters (a drawn name may
/// not equal one of them, `0x00A28CC0`).
#[derive(Debug, Clone, Default)]
pub struct NameData {
    /// The `names` table.
    pub rows: Vec<NameRow>,
    /// Faction key → names group (`factions` #6).
    pub groups: std::collections::BTreeMap<String, String>,
    /// On-screen names of `historical_characters` ("Forename Surname").
    pub historical: std::collections::BTreeSet<String>,
    /// Localised text of a name key (`names_name_...`), when the localisation was loaded.
    pub text: std::collections::BTreeMap<String, String>,
}

impl NameData {
    /// Loads the names table, the faction groups and the historical names from an install.
    pub fn load(vfs: &Vfs, db: &GameDatabase) -> Option<NameData> {
        let rows = read_names(vfs).map_err(|e| eprintln!("WARN ntw_campaign: names: {e}; no character names")).ok()?;
        let groups = db.factions.iter().map(|f| (f.key.clone(), f.character_names_group.clone())).collect();
        let mut historical = std::collections::BTreeSet::new();
        let mut text = std::collections::BTreeMap::new();
        if let Ok(loc) = ntw_formats::loc::Localisation::from_vfs(vfs) {
            for (k, v) in loc.iter() {
                if k.starts_with("historical_characters_on_screen_name_") {
                    historical.insert(v.to_string());
                } else if k.starts_with("names_name_") {
                    text.insert(k.to_string(), v.to_string());
                }
            }
        }
        Some(NameData { rows, groups, historical, text })
    }

    /// The localised text of a name row (its localisation, else the table's name).
    pub fn text_of(&self, r: &NameRow) -> String {
        self.text.get(&r.loc_key()).cloned().unwrap_or_else(|| r.name.clone())
    }
}

/// Faction `key`'s naming pools (male forenames and surnames, [`pool_rows`]) with each entry's
/// localisation key and on-screen text; `None` when the faction has no names group or a pool is
/// empty.
pub fn faction_pools(data: &NameData, key: &str) -> Option<NamePools> {
    let group = data.groups.get(key)?;
    let entries = |pool: usize| -> Option<Vec<NameEntry>> {
        Some(pool_rows(&data.rows, group, pool)?.into_iter().map(|r| NameEntry { key: r.loc_key(), text: data.text_of(r) }).collect())
    };
    let pools = NamePools { forenames: entries(POOL_MALE_FORENAME)?, surnames: entries(POOL_SURNAME)? };
    (!pools.forenames.is_empty() && !pools.surnames.is_empty()).then_some(pools)
}

/// The model's naming data ([`NameRules`]) for the factions of `model`.
pub fn name_rules(data: &NameData, model: &CampaignModel) -> NameRules {
    NameRules {
        pools: model.world.factions.values().filter_map(|f| Some((f.key.clone(), faction_pools(data, &f.key)?))).collect(),
        historical: data.historical.clone(),
        texts: data.rows.iter().map(|r| (r.loc_key(), data.text_of(r))).collect(),
    }
}

/// Puts each faction's stored name allocators (`FACTION` `NAME_ALLOCATION_DETAILS`, save order)
/// into the model, by the ids of the model's factions.
pub fn fill_allocators(root: &EsfRecord, model: &mut CampaignModel) {
    let Some(world) = root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD") else { return };
    let mut by_key: std::collections::BTreeMap<String, Vec<Allocator>> = std::collections::BTreeMap::new();
    let mut factions: Vec<&EsfRecord> = world.record_array("FACTION_ARRAY").map(|a| a.records().filter(|r| r.name == "FACTION").collect()).unwrap_or_default();
    if let Some(r) = world.child("REBEL_FACTION").and_then(|r| r.child("FACTION")) {
        factions.push(r);
    }
    for rec in factions {
        let Some(key) = rec.values().nth(1).and_then(EsfNode::as_str) else { continue };
        by_key.insert(key.to_string(), rec.children_named("NAME_ALLOCATION_DETAILS").filter_map(read_allocator).collect());
    }
    let w = &mut model.world;
    w.name_allocators = w.factions.values().filter_map(|f| Some((f.id, by_key.remove(&f.key)?))).collect();
}

/// Gives the model its naming data from the install's `names` table and localisation, so new
/// characters and unit officers are named when they are created
/// (`CampaignModel::name_new_character`, `name_new_unit`). Logged once when the table cannot be read
/// (new characters then stay unnamed).
pub fn attach(model: &mut CampaignModel, vfs: &Vfs, db: &GameDatabase) {
    let Some(data) = NameData::load(vfs, db) else {
        log::warn!("Campaign names: the names table cannot be read; new characters stay unnamed");
        return;
    };
    attach_data(model, &data);
}

/// [`attach`] with naming data already loaded. A faction with pools but no name decks (a source
/// without its `NAME_ALLOCATION_DETAILS`) cannot name anyone: logged once, listing them.
pub fn attach_data(model: &mut CampaignModel, data: &NameData) {
    let rules = name_rules(data, model);
    let w = &model.world;
    let deckless: Vec<&str> = w
        .factions
        .values()
        .filter(|f| rules.pools.contains_key(&f.key) && w.name_allocators.get(&f.id).is_none_or(|a| a.len() <= POOL_SURNAME))
        .map(|f| f.key.as_str())
        .collect();
    if !deckless.is_empty() {
        log::warn!("Campaign names: no name decks for {deckless:?}; their new characters and officers stay unnamed");
    }
    std::sync::Arc::make_mut(&mut model.rules).names = rules;
}

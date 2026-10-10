//! The portrait allocator in an ESF start position or save (`CAMPAIGN_MODEL` `PORTRAIT_ALLOCATOR`,
//! CHARACTERS_FIDELITY.md §14): read into [`ntw_sim::campaign::portraits`] and written back.
//!
//! Layout (CONFIRMED, the loader `0x009942D0` and writer `0x009A0C50`): per culture {utf16 culture,
//! `CHARACTER_PORTRAIT_PATHS` {`CULTURE_PATHS`[] {utf16 agent type, utf16 folder culture},
//! `PORTRAIT_CATEGORIES`[] {`PORTRAITS`[] {utf16 agent type, `PORTRAIT_ALLOCATION` {u32 count, u32
//! next position, u32[] order}}}}}, four `PORTRAITS` per category. The decks' seeds are not
//! stored: the loader seeds each deck, in file order, with the high 16 bits of one step of the
//! campaign RNG (world +0xFB8, the pointer `0x00872550` passes through `0x00999870`), so reading
//! the allocator advances the model's RNG once per deck, as loading the file does in the original.

use ntw_formats::esf::{EsfNode, EsfRecord, EsfRecordArray};
use ntw_sim::campaign::names::lcg_step;
use ntw_sim::campaign::portraits::{CulturePortraits, PortraitCategory, PortraitDeck};
use ntw_sim::campaign::CampaignModel;

const MODEL_PATH: &str = "CAMPAIGN_ENV/CAMPAIGN_MODEL";

/// Reads the file's portrait allocator into the model, seeding each deck from the model's RNG
/// (one step per deck, file order). A file without one leaves the model without portraits (logged
/// once: new characters then get none).
pub fn fill(root: &EsfRecord, model: &mut CampaignModel) {
    let Some(arr) = root.find_path(MODEL_PATH).and_then(|m| m.record_array("PORTRAIT_ALLOCATOR")) else {
        log::warn!("Campaign portraits: the file has no PORTRAIT_ALLOCATOR; new characters get no portraits");
        return;
    };
    let seed = &mut model.rng.state;
    model.world.portraits = arr.items.iter().filter_map(|item| read_culture(item, seed)).collect();
}

fn read_culture(item: &[EsfNode], seed: &mut u32) -> Option<CulturePortraits> {
    let culture = item.first()?.as_str()?.to_string();
    let paths_rec = item.get(1)?.as_record()?;
    let paths = paths_rec
        .record_array("CULTURE_PATHS")
        .map(|a| a.items.iter().filter_map(|it| Some((it.first()?.as_str()?.to_string(), it.get(1)?.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let categories = paths_rec
        .record_array("PORTRAIT_CATEGORIES")
        .map(|a| a.items.iter().filter_map(|it| read_category(it.first()?.as_record_array()?, seed)).collect())
        .unwrap_or_default();
    Some(CulturePortraits { culture, paths, categories })
}

fn read_category(portraits: &EsfRecordArray, seed: &mut u32) -> Option<PortraitCategory> {
    let mut key = None;
    let mut decks = Vec::with_capacity(portraits.items.len());
    for it in &portraits.items {
        // One step per deck, read or not (`0x009942D0` steps before it builds the deck).
        *seed = lcg_step(*seed);
        key = key.or_else(|| it.first().and_then(EsfNode::as_str).map(str::to_string));
        let a = it.get(1).and_then(EsfNode::as_record);
        decks.push(PortraitDeck {
            count: a.and_then(|a| a.get_u32(0)).unwrap_or(0),
            cursor: a.and_then(|a| a.get_u32(1)).unwrap_or(0),
            order: a.and_then(|a| a.get(2)).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default(),
            seed: *seed >> 16,
        });
    }
    Some(PortraitCategory { key: key?, decks })
}

/// Writes the model's decks (count, next position, order) over the `PORTRAIT_ALLOCATOR` of a
/// `CAMPAIGN_MODEL` record, matched by culture and agent type; the paths are not changed (the
/// model does not change them). A deck the record does not have is not added.
pub(crate) fn write(model_rec: &mut EsfRecord, model: &CampaignModel) {
    let Some(arr) = model_rec.children.iter_mut().find_map(|c| match c {
        EsfNode::RecordArray(a) if a.name == "PORTRAIT_ALLOCATOR" => Some(a),
        _ => None,
    }) else {
        return;
    };
    for item in &mut arr.items {
        let Some(culture) = item.first().and_then(EsfNode::as_str) else { continue };
        let Some(set) = model.world.portraits.iter().find(|s| s.culture == culture) else { continue };
        let Some(EsfNode::Record(paths)) = item.get_mut(1) else { continue };
        let Some(cats) = paths.children.iter_mut().find_map(|c| match c {
            EsfNode::RecordArray(a) if a.name == "PORTRAIT_CATEGORIES" => Some(a),
            _ => None,
        }) else {
            continue;
        };
        for cat in &mut cats.items {
            let Some(EsfNode::RecordArray(portraits)) = cat.first_mut() else { continue };
            let Some(key) = portraits.items.first().and_then(|it| it.first()).and_then(EsfNode::as_str) else { continue };
            let Some(category) = set.categories.iter().find(|c| c.key == key) else { continue };
            for (it, deck) in portraits.items.iter_mut().zip(&category.decks) {
                if let Some(EsfNode::Record(a)) = it.get_mut(1) {
                    write_deck(a, deck);
                }
            }
        }
    }
}

fn write_deck(a: &mut EsfRecord, deck: &PortraitDeck) {
    for (i, v) in [(0, deck.count), (1, deck.cursor)] {
        if let Some(n @ EsfNode::U32(_)) = a.children.get_mut(i) {
            *n = EsfNode::U32(v);
        }
    }
    if let Some(n @ EsfNode::U32Array(_)) = a.children.get_mut(2) {
        *n = EsfNode::U32Array(deck.order.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocation(count: u32, cursor: u32, order: Vec<u32>) -> EsfNode {
        let mut r = EsfRecord::new("PORTRAIT_ALLOCATION", 1);
        r.children = vec![EsfNode::U32(count), EsfNode::U32(cursor), EsfNode::U32Array(order)];
        EsfNode::Record(Box::new(r))
    }

    /// A `CAMPAIGN_MODEL` with one culture, one category of two decks.
    fn model_record() -> EsfRecord {
        let mut paths = EsfRecordArray::new("CULTURE_PATHS", 0);
        paths.items.push(vec![EsfNode::Utf16String("General".into()), EsfNode::Utf16String("european".into())]);
        let mut portraits = EsfRecordArray::new("PORTRAITS", 0);
        portraits.items.push(vec![EsfNode::Utf16String("General".into()), allocation(3, 1, vec![2, 0, 1])]);
        portraits.items.push(vec![EsfNode::Utf16String("General".into()), allocation(2, 0, vec![1, 0])]);
        let mut cats = EsfRecordArray::new("PORTRAIT_CATEGORIES", 0);
        cats.items.push(vec![EsfNode::RecordArray(Box::new(portraits))]);
        let mut cpp = EsfRecord::new("CHARACTER_PORTRAIT_PATHS", 1);
        cpp.children = vec![EsfNode::RecordArray(Box::new(paths)), EsfNode::RecordArray(Box::new(cats))];
        let mut alloc = EsfRecordArray::new("PORTRAIT_ALLOCATOR", 1);
        alloc.items.push(vec![EsfNode::Utf16String("european".into()), EsfNode::Record(Box::new(cpp))]);
        let mut m = EsfRecord::new("CAMPAIGN_MODEL", 10);
        m.children.push(EsfNode::RecordArray(Box::new(alloc)));
        m
    }

    fn root(model: EsfRecord) -> EsfRecord {
        let mut env = EsfRecord::new("CAMPAIGN_ENV", 0);
        env.children.push(EsfNode::Record(Box::new(model)));
        let mut root = EsfRecord::new("CAMPAIGN_SAVE_GAME", 0);
        root.children.push(EsfNode::Record(Box::new(env)));
        root
    }

    #[test]
    fn reading_seeds_each_deck_from_one_rng_step_and_writing_round_trips() {
        let mut m = crate::own_save::tests::made_up_model();
        let start = m.rng.state;
        fill(&root(model_record()), &mut m);
        let set = &m.world.portraits[0];
        assert_eq!((set.culture.as_str(), set.paths.clone()), ("european", vec![("General".to_string(), "european".to_string())]));
        let decks = &set.categories[0].decks;
        assert_eq!((decks[0].count, decks[0].cursor, decks[0].order.clone()), (3, 1, vec![2, 0, 1]));
        assert_eq!(decks[0].seed, lcg_step(start) >> 16);
        assert_eq!(decks[1].seed, lcg_step(lcg_step(start)) >> 16);
        assert_eq!(m.rng.state, lcg_step(lcg_step(start)), "one campaign-RNG step per deck");
        // Writing the unchanged model leaves the record as it was; a draw is written back.
        let mut rec = model_record();
        write(&mut rec, &m);
        assert_eq!(rec, model_record());
        let deck = &mut m.world.portraits[0].categories[0].decks[0];
        assert_eq!(deck.draw(), Some(0));
        write(&mut rec, &m);
        let mut want = model_record();
        if let EsfNode::RecordArray(a) = &mut want.children[0]
            && let EsfNode::Record(cpp) = &mut a.items[0][1]
            && let EsfNode::RecordArray(cats) = &mut cpp.children[1]
            && let EsfNode::RecordArray(p) = &mut cats.items[0][0]
        {
            p.items[0][1] = allocation(3, 2, vec![2, 0, 1]);
        }
        assert_eq!(rec, want);
    }
}

//! The portrait allocator against the install (CHARACTERS_FIDELITY.md §14): the start positions'
//! decks against the portrait folders and the shuffle, a hire's and a promotion's portraits, and
//! the round trip through an ESF save. Skipped without an install.

use std::path::PathBuf;

use ntw_data::GameDatabase;
use ntw_formats::campaign_map::GameFiles;
use ntw_formats::esf::EsfFile;
use ntw_formats::pack::Vfs;
use ntw_sim::campaign::names::{lcg_step, shuffle_in_place};
use ntw_sim::campaign::portraits::{PortraitDeck, PORTRAIT_TYPES};
use ntw_sim::campaign::{CampaignModel, CharacterId};

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data")
    })
}

struct Install {
    db: GameDatabase,
    vfs: Vfs,
}

fn install() -> Option<Install> {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skipped: no install at {}", dir.display());
        return None;
    }
    Some(Install { db: GameDatabase::from_install(&dir).expect("DB"), vfs: Vfs::open_install(&dir).expect("vfs") })
}

fn startpos(i: &Install, campaign: &str) -> EsfFile {
    let bytes = GameFiles { vfs: &i.vfs }.read(&format!("campaigns/{campaign}/startpos.esf")).expect("startpos");
    EsfFile::from_bytes(&bytes).expect("esf")
}

/// The number of pictures the original's builder counts in a folder (`0x00A1F730`): probes 0, 1,
/// 2, 4, 8, ... until one is missing, then halves between the last found and the first missing;
/// the result is the last picture number found, 0 when 000 or 001 is missing.
fn exe_count(vfs: &Vfs, dir: &str, ext: &str) -> u32 {
    let exists = |n: u32| vfs.contains(&format!("{dir}/{n:03}.{ext}"));
    let (mut found, mut missing) = (0u32, 0u32);
    while exists(missing) {
        found = missing;
        missing = if missing == 0 { 1 } else { missing * 2 };
    }
    if missing < 2 {
        return 0;
    }
    loop {
        let mid = found + (missing - found) / 2;
        if mid == found || mid == missing {
            return mid;
        }
        if exists(mid) { found = mid } else { missing = mid }
    }
}

/// Every non-empty deck of the shipped start positions holds as many numbers as the original's
/// builder counts in the folder named by the culture path and the agent type's `agents` #6 (or
/// key): the folder rule and the count rule (the last picture number, so the last picture is
/// never dealt). Empty decks are not checked: the start positions were built from older tables
/// (eur's missionaries have none although `minister` has pictures; spa's do).
#[test]
fn start_position_decks_count_their_portrait_folders() {
    let Some(i) = install() else { return };
    let folders: std::collections::BTreeMap<&str, &str> =
        i.db.campaign.agents.iter().filter_map(|a| Some((a.key.as_str(), a.portrait_folder.as_deref().filter(|s| !s.is_empty())?))).collect();
    let mut checked = 0;
    for campaign in ["eur_napoleon", "egy_napoleon", "spa_napoleon", "ita_napoleon"] {
        let l = ntw_campaign::read_esf(&startpos(&i, campaign), &i.db).expect("load");
        assert!(!l.model.world.portraits.is_empty(), "{campaign}: an allocator");
        for set in &l.model.world.portraits {
            for cat in &set.categories {
                assert_eq!(cat.decks.len(), PORTRAIT_TYPES, "{campaign} {} {}", set.culture, cat.key);
                // king / queen are the family's categories (not agent types): their folder is the key.
                let folder = folders.get(cat.key.as_str()).copied().unwrap_or(cat.key.as_str()).to_ascii_lowercase();
                let path = set.paths.iter().find(|(a, _)| *a == cat.key).map(|(_, p)| p.as_str()).unwrap_or_else(|| panic!("{campaign} {}: no path for {}", set.culture, cat.key));
                for (t, deck) in cat.decks.iter().enumerate().filter(|(_, d)| d.count > 0) {
                    let (kind, ext) = if t < 2 { ("Info", "jpg") } else { ("Cards", "tga") };
                    let age = if t % 2 == 0 { "young" } else { "old" };
                    let dir = format!("ui/portraits/{path}/{kind}/{folder}/{age}");
                    assert_eq!(deck.count, exe_count(&i.vfs, &dir, ext), "{campaign} {} {} deck {t} ({dir})", set.culture, cat.key);
                    assert!(i.vfs.contains(&format!("{dir}/{:03}.{ext}", deck.count)), "the last picture is not dealt");
                    assert_eq!(deck.order.len() as u32, deck.count);
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 300, "{checked} decks");
}

/// The first deck's seeds: the high halves s of chain states whose deal (numbers 0..count
/// shuffled from one step of s, `0x00A1F730` → `0x00A1E910`) is `order`.
fn first_deal_seeds(count: u32, order: &[u32]) -> Vec<u32> {
    (0..=0xFFFFu32).filter(|&s| dealt(count, s) == order).collect()
}

fn dealt(count: u32, seed16: u32) -> Vec<u32> {
    let mut d: Vec<u32> = (0..count).collect();
    shuffle_in_place(&mut d, lcg_step(seed16));
    d
}

/// The start position's decks are the builder's (`0x00A1F890`): numbers 0..count shuffled
/// (`0x00A1E910`) from the high half of a chain state, the four decks of a category from four
/// consecutive chain steps; and a deck dealt past its end is the in-place reshuffle of its first
/// deal ([`PortraitDeck::draw`]). Found by search over the chain states; checks the fill, the
/// shuffle, the seeding and the reshuffle.
#[test]
fn start_position_decks_are_dealt_from_consecutive_seeds() {
    let Some(i) = install() else { return };
    let l = ntw_campaign::read_esf(&startpos(&i, "eur_napoleon"), &i.db).expect("load");
    let category = |culture: &str, key: &str| {
        let set = l.model.world.portraits.iter().find(|s| s.culture == culture).expect("culture");
        set.categories.iter().find(|c| c.key == key).expect("category").clone()
    };
    // Untouched categories (every cursor 0).
    for (culture, key) in [("PIR_european", "General"), ("middle_east", "assassin"), ("european", "gentleman")] {
        let cat = category(culture, key);
        assert!(cat.decks.iter().all(|d| d.cursor == 0));
        let highs = first_deal_seeds(cat.decks[0].count, &cat.decks[0].order);
        let states: Vec<u32> = highs
            .iter()
            .flat_map(|&h| (0..=0xFFFFu32).map(move |low| (h << 16) | low))
            .filter(|&s0| {
                let mut s = s0;
                cat.decks[1..].iter().all(|d| {
                    s = lcg_step(s);
                    dealt(d.count, s >> 16) == d.order
                })
            })
            .collect();
        assert_eq!(states.len(), 1, "{culture} {key}: one chain state deals the four decks");
    }
    // european General's young deck was dealt past its 95 numbers while the start position was
    // made (cursor 32): it is the reshuffle of a first deal.
    let d = category("european", "General").decks[0].clone();
    let reshuffled = (0..=0xFFFFu32).filter(|&s| {
        let mut deck = PortraitDeck { count: d.count, cursor: 0, order: (0..d.count).collect(), seed: s };
        deck.reshuffle();
        deck.cursor = deck.count;
        deck.draw();
        deck.order == d.order
    });
    assert_eq!(reshuffled.count(), 1);
}

fn eur_model(i: &Install) -> CampaignModel {
    let mut l = ntw_campaign::read_esf(&startpos(i, "eur_napoleon"), &i.db).expect("load");
    assert!(l.set_human("france"));
    l.model
}

/// A French candidate General gets a portrait from the european General decks by his age, with
/// both pictures in the shipped files; an admiral candidate draws from the same decks (agents #6).
#[test]
fn a_hire_candidate_gets_a_portrait_that_exists() {
    let Some(i) = install() else { return };
    let mut m = eur_model(&i);
    let france = m.faction_by_key("france").unwrap().id;
    let general = |m: &CampaignModel| m.world.portraits.iter().find(|s| s.culture == "european").unwrap().categories.iter().find(|c| c.key == "General").unwrap().decks.clone();
    for kind in [ntw_sim::campaign::pool::PoolKind::General, ntw_sim::campaign::pool::PoolKind::Admiral] {
        let before = general(&m);
        let c = m.create_candidate(france, kind).expect("a candidate");
        let d = &m.world.character_details[&c];
        let age = m.calendar.date.year as i32 - d.birth.unwrap().year as i32;
        let (p, old) = (&d.portrait, age >= 45);
        assert!(p.index >= 0, "a number");
        let deck = &before[usize::from(old)];
        assert_eq!(p.index as u32, deck.order[deck.cursor as usize], "the next number of the {} deck", if old { "old" } else { "young" });
        let age_dir = if old { "old" } else { "young" };
        assert_eq!(p.card, format!("ui/portraits/european/Cards/general/{age_dir}/{:03}.tga", p.index));
        assert_eq!(p.info, format!("ui/portraits/european/Info/general/{age_dir}/{:03}.jpg", p.index));
        assert!(i.vfs.contains(&p.card) && i.vfs.contains(&p.info), "{} exists", p.card);
    }
}

/// An ESF save carries a candidate's portrait and the drawn decks; reading it back gives the same
/// decks, newly seeded: one campaign-RNG step per deck from the saved `RandSeed`.
#[test]
fn a_candidate_keeps_his_portrait_through_an_esf_save() {
    let Some(i) = install() else { return };
    let esf = startpos(&i, "eur_napoleon");
    let mut l = ntw_campaign::read_esf(&esf, &i.db).expect("load");
    assert!(l.set_human("france"));
    let mut m = l.model;
    let france = m.faction_by_key("france").unwrap().id;
    let c: CharacterId = m.create_candidate(france, ntw_sim::campaign::pool::PoolKind::General).expect("a candidate");
    let portrait = m.world.character_details[&c].portrait.clone();
    assert!(portrait.index >= 0);
    let bytes = ntw_campaign::save::save_bytes(&esf, &m, "france", 0).expect("save");
    let back = ntw_campaign::read(&bytes, &i.db).expect("read back").model;
    assert_eq!(back.world.character_details[&c].portrait, portrait);
    let decks = |p: &[ntw_sim::campaign::portraits::CulturePortraits]| -> Vec<(u32, u32, Vec<u32>)> {
        p.iter().flat_map(|s| &s.categories).flat_map(|c| &c.decks).map(|d| (d.count, d.cursor, d.order.clone())).collect()
    };
    assert_eq!(decks(&back.world.portraits), decks(&m.world.portraits));
    let n = decks(&m.world.portraits).len();
    let stepped = (0..n).fold(m.rng.state, |s, _| lcg_step(s));
    assert_eq!(back.rng.state, stepped, "{n} decks, one step each");
}

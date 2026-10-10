//! Research helper (read-only): the faction name allocators of saves against the `names` table
//! and the shuffle port (SAVE_COMPAT.md §14).
//!
//! ```text
//! cargo run --release -p ntw_campaign --example names_check -- FILE...
//! ```

use std::path::PathBuf;

use ntw_campaign::names::{self, read_allocator, Allocator};
use ntw_data::GameDatabase;
use ntw_formats::esf::{EsfFile, EsfNode};
use ntw_formats::pack::Vfs;

fn main() {
    let dir = std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"));
    let db = GameDatabase::from_install(&dir).expect("db");
    let vfs = Vfs::open_install(&dir).expect("vfs");
    let rows = names::read_names(&vfs).expect("names");
    for p in std::env::args().skip(1) {
        let esf = EsfFile::open(&p).expect("open");
        let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").expect("world");
        let (mut size_ok, mut size_bad, mut suffix_ok, mut suffix_bad, mut empty) = ([0usize; 10], [0usize; 10], 0, 0, 0);
        let mut bad_examples = Vec::new();
        for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
            let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
            let Some(group) = names::faction_group(&db, &key) else { continue };
            let allocs: Vec<Allocator> = f.children_named("NAME_ALLOCATION_DETAILS").filter_map(read_allocator).collect();
            for (k, a) in allocs.iter().enumerate().take(10) {
                if let Some(pool) = names::pool_rows(&rows, &group, k) {
                    if pool.len() as u32 == a.size {
                        size_ok[k] += 1;
                    } else {
                        size_bad[k] += 1;
                        if bad_examples.len() < 6 {
                            bad_examples.push(format!("{key} pool {k}: stored {} ours {}", a.size, pool.len()));
                        }
                    }
                }
                if a.deck.is_empty() {
                    empty += 1;
                    continue;
                }
                let full = Allocator::shuffled(a.size, a.seed);
                if full.ends_with(&a.deck) {
                    suffix_ok += 1;
                } else {
                    suffix_bad += 1;
                    match subsequence_report(a) {
                        Some((front, middle)) => println!("    {key} pool {k}: subsequence of our shuffle, {front} drawn from the front, {middle} removed from the middle"),
                        None => println!("    {key} pool {k}: NOT a subsequence of our shuffle (size {}, deck {}); seed offset {:?}", a.size, a.deck.len(), seed_offset(a)),
                    }
                }
            }
        }
        println!("{p}:\n  pool sizes match (pool: ok/bad) {:?}\n  stored deck = tail of our shuffle of its seed: {suffix_ok} yes, {suffix_bad} no ({empty} empty)", (0..10).map(|k| format!("{k}:{}/{}", size_ok[k], size_bad[k])).collect::<Vec<_>>());
        for e in bad_examples {
            println!("    {e}");
        }
    }
}

/// For the decks that are not a tail of our shuffle: are they a subsequence of it (entries removed
/// from the middle), and what is removed?
pub fn subsequence_report(a: &Allocator) -> Option<(usize, usize)> {
    let full = Allocator::shuffled(a.size, a.seed);
    // Greedy subsequence match from the end.
    let mut j = full.len();
    for &v in a.deck.iter().rev() {
        loop {
            if j == 0 {
                return None;
            }
            j -= 1;
            if full[j] == v {
                break;
            }
        }
    }
    // j = index in full of the deck's first entry; removed before it = j, inside = gaps.
    let span = full.len() - j;
    Some((j, span - a.deck.len()))
}

/// Tries seeds a few LCG steps away from the stored one (forward), and the stored deck as a tail
/// or subsequence of that shuffle: how far the stored seed is from the deck's shuffle.
pub fn seed_offset(a: &Allocator) -> Option<i32> {
    let lcg = |s: u32| s.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
    // Inverse of the MS LCG: multiplier inverse mod 2^32.
    let inv = |s: u32| s.wrapping_sub(0x269EC3).wrapping_mul(0xB9B33155);
    let mut fwd = a.seed;
    let mut back = a.seed;
    for k in 1..=64 {
        fwd = lcg(fwd);
        back = inv(back);
        for (s, off) in [(fwd, k), (back, -k)] {
            let t = Allocator { size: a.size, seed: s, deck: a.deck.clone() };
            if subsequence_report(&t).is_some() {
                return Some(off);
            }
        }
    }
    None
}

//! Research helper: allocators whose seed changed between consecutive saves of one game (the
//! original refilled them in play): is the new deck a tail of our shuffle of the new seed?
use ntw_campaign::names::Allocator;
use ntw_formats::esf::{EsfFile, EsfNode};
fn allocs(p: &str) -> Vec<(String, usize, Allocator)> {
    let esf = EsfFile::open(p).unwrap();
    let w = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD").unwrap();
    let mut out = Vec::new();
    for f in w.record_array("FACTION_ARRAY").into_iter().flat_map(|a| a.records()) {
        let key = f.values().filter_map(EsfNode::as_str).next().unwrap_or("").to_string();
        for (k, r) in f.children_named("NAME_ALLOCATION_DETAILS").enumerate() {
            if let Some(a) = Allocator::read(r) {
                out.push((key.clone(), k, a));
            }
        }
    }
    out
}
fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    let (mut changed, mut tail) = (0, 0);
    for w in files.windows(2) {
        let (a, b) = (allocs(&w[0]), allocs(&w[1]));
        for (key, k, x) in &b {
            let Some((_, _, old)) = a.iter().find(|(kk, j, _)| kk == key && j == k) else { continue };
            if old.seed != x.seed {
                changed += 1;
                let full = Allocator::shuffled(x.size, x.seed);
                if full.ends_with(&x.deck) {
                    tail += 1;
                } else {
                    println!("{key} pool {k}: refilled in play but not a tail (size {}, deck {})", x.size, x.deck.len());
                }
            }
        }
    }
    println!("{changed} refilled in play, {tail} are tails of our shuffle");
}

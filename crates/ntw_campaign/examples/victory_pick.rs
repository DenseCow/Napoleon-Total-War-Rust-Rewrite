//! Research helper: which of a start position's victory options a save's human holds.
//!   victory_pick STARTPOS SAVE FACTION
use ntw_formats::esf::{EsfFile, EsfRecord};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let sp = EsfFile::open(&a[0]).unwrap();
    let sv = EsfFile::open(&a[1]).unwrap();
    let opts = ntw_campaign::victory::options(&sp.root, &a[2]).expect("options");
    let mut held = Vec::new();
    sv.root.walk(&mut |r: &EsfRecord| {
        if r.name == ntw_campaign::victory::RECORD && ntw_campaign::victory::kind(r).0 != ntw_campaign::victory::TYPE_PENINSULAR {
            held.push(r.clone());
        }
    });
    for (i, o) in opts.iter().enumerate() {
        let same = held.iter().filter(|h| *h == o).count();
        println!("option {i}: type/regions {:?}, held {same} times", ntw_campaign::victory::kind(o));
    }
    for h in &held {
        println!("held: type/regions {:?} met {:?}", ntw_campaign::victory::kind(h), h.get_bool(1));
    }
}

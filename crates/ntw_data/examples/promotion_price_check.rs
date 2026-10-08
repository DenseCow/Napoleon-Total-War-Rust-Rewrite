//! Research helper (0-G, read-only): the internal-consistency test for the field-promotion price
//! candidate. `0x008E2770` (the land unit class's slot +0x44) reads
//! `*(record->(+0x0C) + 0x38)` — one DB row field under the -4 record shift this file documents,
//! i.e. `units` column #7 `unknown_3c`. A promotion price must be a coherent function of the thing
//! being promoted, so this prints, for all `units` rows:
//!   - the value distribution (how many distinct prices, the shape),
//!   - whether `unknown_3c` is a pure function of `recruitment_cost` (ratio spread) or scatters,
//!   - the price a land promotion of a *colonel* would charge, by unit category,
//!   - the naval side (slot +0x44 is a return-0 stub there, so the naval value is never charged).
//!
//! Read-only against the install. Prints one block; nothing is written.
fn main() {
    let dir = std::env::var("NTW_DATA_DIR")
        .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data".into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("db");
    let rows: Vec<&ntw_data::UnitRecord> = db.units.rows().iter().collect();

    // 1. distinct values and histogram
    let mut vals: Vec<i32> = rows.iter().map(|u| u.unknown_3c).collect();
    vals.sort_unstable();
    println!("units rows            {}", rows.len());
    println!("distinct unknown_3c   {}", vals.iter().collect::<std::collections::BTreeSet<_>>().len());
    println!("min / max             {} / {}", vals.first().copied().unwrap_or(0), vals.last().copied().unwrap_or(0));
    let mut hist: std::collections::BTreeMap<i32, usize> = Default::default();
    for &v in &vals {
        *hist.entry(v).or_default() += 1;
    }
    println!("\n-- value histogram (value: how many unit rows) --");
    for (v, n) in &hist {
        println!("{v:>8} : {n:>4}");
    }

    // 2. is unknown_3c a pure function of recruitment_cost?
    let mut by_cost: std::collections::BTreeMap<i32, Vec<i32>> = Default::default();
    for u in &rows {
        by_cost.entry(u.recruitment_cost).or_default().push(u.unknown_3c);
    }
    let mut split = 0usize;
    let mut worst = (0i32, 0i32, 0i32);
    for (&c, v) in &by_cost {
        let lo = *v.iter().min().unwrap();
        let hi = *v.iter().max().unwrap();
        if lo != hi {
            split += 1;
            if hi - lo > worst.0 {
                worst = (hi - lo, c, lo);
            }
        }
    }
    println!("\n-- unknown_3c against recruitment_cost --");
    println!("distinct recruitment_cost values        {}", by_cost.len());
    println!("cost values where unknown_3c is NOT one  {split}");
    println!("largest spread at cost {}: {} .. {} (width {})", worst.1, worst.2, worst.2 + worst.0, worst.0);
    // ratio spread on the rows where it is well defined
    let ratios: Vec<(f64, String)> = rows
        .iter()
        .filter(|u| u.recruitment_cost > 0)
        .map(|u| (u.unknown_3c as f64 / u.recruitment_cost as f64, u.key.clone()))
        .collect();
    let lo = ratios.iter().map(|r| r.0).fold(f64::MAX, f64::min);
    let hi = ratios.iter().map(|r| r.0).fold(f64::MIN, f64::max);
    let lo_key = ratios.iter().find(|r| r.0 == lo).map(|r| r.1.clone()).unwrap_or_default();
    let hi_key = ratios.iter().find(|r| r.0 == hi).map(|r| r.1.clone()).unwrap_or_default();
    println!("ratio unknown_3c/cost   min {:.3} ({lo_key})  max {:.3} ({hi_key})  ({} rows)", lo, hi, ratios.len());

    // 3. the price a land promotion would charge, by category, and per culture general unit
    println!("\n-- candidate promotion price (unknown_3c) by category --");
    let mut cats: std::collections::BTreeMap<&str, Vec<i32>> = Default::default();
    for u in &rows {
        cats.entry(u.category.as_str()).or_default().push(u.unknown_3c);
    }
    for (c, v) in &cats {
        let mut s = v.clone();
        s.sort_unstable();
        println!("{c:<26} n={:<4} min={:<6} median={:<6} max={}", s.len(), s[0], s[s.len() / 2], s[s.len() - 1]);
    }

    // 4. the general / staff / bodyguard rows the manager quoted
    println!("\n-- rows whose key starts with Gen_ --");
    for u in rows.iter().filter(|u| u.key.starts_with("Gen_")) {
        println!(
            "{:44} {:<14} cost={:<6} u3c={:<6} upkeep={:<4} u44={:<4}",
            u.key, u.category, u.recruitment_cost, u.unknown_3c, u.upkeep, u.unknown_44
        );
    }

    // 5. naval: what the field would say if the same column were used (the exe charges 0)
    let naval: Vec<i32> = rows.iter().filter(|u| u.category.starts_with("naval")).map(|u| u.unknown_3c).collect();
    println!("\nnaval rows {} u3c min {} max {}", naval.len(), naval.iter().min().copied().unwrap_or(0), naval.iter().max().copied().unwrap_or(0));
}
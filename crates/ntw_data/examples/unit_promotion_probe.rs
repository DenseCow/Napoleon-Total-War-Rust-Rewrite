//! Research helper (0-G, read-only): the `units` cost columns the field-promotion price could be
//! (`units` #4 `recruitment_cost`, #5 `secondary_cost`, #6 `unknown_38` = recruitment time in
//! turns, #7 `unknown_3c`, #9 `unknown_44`, #8 `upkeep`), split by land / naval and by the
//! agent-record units, so the promotion-cost column can be recognised from the shipped data.
//! Prints every row's numbers (the table is ~700 rows).
fn main() {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data".into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("db");
    println!("{:52} {:<22} {:>6} {:>6} {:>5} {:>6} {:>5} {:>6} {:>5}", "key", "category", "cost", "second", "turns", "u3c", "upk", "u44", "u84");
    let mut naval = 0;
    let mut land = 0;
    for u in db.units.rows() {
        let is_naval = u.category.starts_with("naval");
        if is_naval {
            naval += 1;
        } else {
            land += 1;
        }
        println!(
            "{:52} {:<22} {:>6} {:>6} {:>5} {:>6} {:>5} {:>6} {:>5}{}",
            u.key,
            u.category,
            u.recruitment_cost,
            u.secondary_cost,
            u.unknown_38,
            u.unknown_3c,
            u.upkeep,
            u.unknown_44,
            u.unit_cap,
            if is_naval { "  naval" } else { "" }
        );
    }
    println!("land {land} naval {naval}");
}

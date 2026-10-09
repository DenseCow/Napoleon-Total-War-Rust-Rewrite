//! Research helper: `units` columns the custom battle reads (mp_category, the three era flags,
//! costs) for a few units. Read-only.
fn main() {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data".into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("db");
    let mut cats = std::collections::BTreeMap::new();
    for u in db.units.rows() {
        *cats.entry((u.mp_category.clone(), u.unknown_94, u.unknown_95, u.unknown_96)).or_insert(0) += 1;
    }
    for (k, n) in cats {
        println!("{k:?}: {n}");
    }
    for k in ["Inf_Line_Austrian_German_Fusiliers", "Gen_Generals_Staff", "Art_Foot_Austrian_12_lber"] {
        if let Some(u) = db.unit(k) {
            println!("{k}: cost {} second {} upkeep {} class {} cat {} mp {} scope {:?} cap {} u98 {} u9c {} ua0 {}", u.recruitment_cost, u.secondary_cost, u.upkeep, u.unit_class, u.category, u.mp_category, u.recruitment_scope, u.unit_cap, u.unknown_98, u.unknown_9c, u.unknown_a0);
        }
    }
}

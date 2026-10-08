//! Research helper for cavalry (read-only): prints the animation-related `unit_stats_land`
//! columns of units matching a filter. Usage:
//!   cargo run -p ntw_data --example cavalry_probe -- [filter]
use ntw_data::GameDatabase;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default().to_ascii_lowercase();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let db = GameDatabase::from_install(&dir).unwrap();
    for s in db.unit_stats_land.iter() {
        if !s.key.to_ascii_lowercase().contains(&filter) {
            continue;
        }
        println!(
            "{} | men {} mounts {} | off {} mus {:?} std {:?} | cult {} ent {} anim {} theme {} | mount {:?} {:?} {:?} {} {}",
            s.key,
            s.num_men,
            s.num_mounts,
            s.officer,
            s.musician,
            s.standard_bearer,
            s.animation_culture_set,
            s.man_entity,
            s.man_animation_type,
            s.weapon_anim_group,
            s.mount,
            s.mount_entity,
            s.mount_type,
            s.mount_text_a,
            s.mount_text_b,
        );
    }
}

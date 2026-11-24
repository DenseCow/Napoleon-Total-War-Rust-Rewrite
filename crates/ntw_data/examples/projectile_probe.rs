//! Research helper (read-only): prints the ballistic columns of every `projectiles` row whose key
//! contains the filter (trajectory class, ranges, maximum elevation, muzzle velocity, accuracy
//! modifier).
//!   cargo run -p ntw_data --example projectile_probe -- [filter]
use ntw_data::GameDatabase;


fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let dir = ntw_formats::install::data_dir();
    let db = GameDatabase::from_install(&dir).unwrap();
    for p in db.projectiles.iter().filter(|p| p.key.contains(&filter)) {
        println!(
            "{:40} {:12} range {:4} min {:3} elev {:3} v {:7.1} acc {:6.2} dmg {:5.2}",
            p.key, p.trajectory_class, p.effective_range, p.minimum_range, p.max_elevation, p.muzzle_velocity,
            p.accuracy_modifier, p.damage
        );
    }
}

//! The battle sea's two tables (`sea_surfaces`, `sea_climate_details`) against a real install
//! (read-only). `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_data --test sea_install -- --ignored --nocapture
//! ```
//! See `analysis/graphics/WATER.md` §4.
use std::path::PathBuf;

use ntw_data::GameDatabase;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

/// The five wind levels decode with the values the per-draw setter `0x0119E830` hands to
/// `ocean.fx`, and every climate row carries the same dark deep colour.
#[test]
#[ignore]
fn sea_tables_decode() {
    let db = GameDatabase::from_install(data_dir()).expect("database");
    let keys: Vec<&str> = db.sea_surfaces.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["wind_level_0", "wind_level_1", "wind_level_2", "wind_level_3", "wind_level_4"]);
    let shininess: Vec<f32> = db.sea_surfaces.iter().map(|s| s.shininess).collect();
    assert_eq!(shininess, [200.0, 1000.0, 378.0, 1000.0, 1000.0]);
    for s in db.sea_surfaces.iter() {
        assert_eq!((s.froth_value, s.froth_distortion_speed, s.froth_distortion_amount), (0.7, 0.35, 0.05));
        assert_eq!((s.decay, s.reflection_flattening), (0.0006, 0.5));
    }
    assert_eq!(db.sea_climate_details.len(), 33);
    for c in db.sea_climate_details.iter() {
        let deep = [c.deep_r, c.deep_g, c.deep_b];
        assert!(deep.iter().zip([0.02, 0.032, 0.035]).all(|(a, b)| (a - b).abs() < 1e-6), "{}: {deep:?}", c.climate);
    }
}

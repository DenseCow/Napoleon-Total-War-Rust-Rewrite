//! The battle sea's wave containers against a real install (read-only). `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_formats --test sea_waves_install -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use ntw_formats::pack::Vfs;
use ntw_formats::sea_waves::SeaWaves;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// `sea\sea` and `sea\swell` decode to the end with the header values `WATER.md` §3 lists.
#[test]
#[ignore]
fn shipped_sea_and_swell_decode() {
    let vfs = vfs();
    let sea = SeaWaves::parse(&vfs.read("sea\\sea").unwrap()).unwrap();
    assert_eq!((sea.size, sea.tile_metres, sea.period, sea.frames), (256, 70.0, 10.0, 100));
    assert_eq!((sea.lo, sea.hi), ([0x6754, 0x6614], [0x98aa, 0x99ea]));
    assert_eq!(sea.angles.len(), 100 * 256 * 256 * 2);
    let swell = SeaWaves::parse(&vfs.read("sea\\swell").unwrap()).unwrap();
    assert_eq!((swell.size, swell.tile_metres, swell.period, swell.frames), (128, 800.0, 30.0, 60));
    assert_eq!((swell.lo, swell.hi), ([0x7a5b, 0x7a3f], [0x85a3, 0x85bf]));
    // The slopes are small: every angle stays within a quarter turn of flat.
    for w in [&sea, &swell] {
        for ch in 0..2 {
            for byte in [0u8, 255] {
                let angle = (w.dequantise(ch, byte) as f32 / 65535.0 - 0.5) * std::f32::consts::TAU;
                assert!(angle.abs() < std::f32::consts::FRAC_PI_2, "{angle}");
            }
        }
    }
}

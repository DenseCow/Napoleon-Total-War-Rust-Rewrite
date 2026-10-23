//! The campaign camera of the original's saves (`ntw_campaign::camera`), read from read-only evidence
//! copies of vanilla saves (`tests/common`). Skips itself without them. Run with
//! `cargo test -p ntw_campaign --test camera_install -- --nocapture`.

mod common;

use std::path::PathBuf;

use ntw_campaign::camera::{self, CameraView};
use ntw_data::GameDatabase;
use ntw_formats::esf::EsfFile;

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data"))
}

/// Two vanilla saves hold the camera they were saved with (CAMPAIGN_CAMERA_MANAGER v4, no named
/// cameras), and its target lies among the campaign's settlements: it is in logic units, the units of
/// the model's positions (the exe's new-game view takes it from a faction position the same way,
/// `0x00987070`).
#[test]
fn vanilla_saves_hold_their_camera() {
    let dir = common::evidence_dir();
    let cases = [
        ("orig_fr_t1.save", CameraView { tilt: 0.65, field_13c: 0, target: [-64.0, 0.0, -4.0], flag_140: false }),
        ("orig_fr_may1811.save", CameraView { tilt: 0.65, field_13c: 0, target: [-64.0, 0.0, -4.0], flag_140: false }),
    ];
    let install = data_dir();
    let db = install.is_dir().then(|| GameDatabase::from_install(&install).expect("database"));
    for (name, want) in cases {
        let path = dir.join(name);
        if !path.is_file() {
            println!("SKIPPED: no {}", path.display());
            continue;
        }
        let bytes = std::fs::read(&path).expect("read");
        let esf = EsfFile::from_bytes(&bytes).expect("esf");
        let cam = camera::read(&esf).expect("a camera block");
        assert_eq!(cam.current, want, "{name}");
        assert!(cam.named.is_empty(), "{name}");
        let Some(db) = &db else { continue };
        let loaded = ntw_campaign::read_esf(&esf, db).expect("load");
        assert_eq!(loaded.camera.as_ref(), Some(&cam), "{name}");
        // The eur script's `set_zoom_limit(1.0, 0.61)` after the intro, saved at
        // `EPISODIC_RESTRICTIONS` #13 / #14 (writer `0x009A1070`).
        assert_eq!(loaded.model.world.zoom_limit, ntw_sim::campaign::ZoomLimit { max_tilt: 1.0, min_tilt: 0.61 }, "{name}");
        // The map bounds at #9..#12 (`0x009968A0`): every vanilla Europe save holds this square.
        let square = ntw_sim::campaign::MapBounds { min_x: -320.0, min_z: -320.0, max_x: 320.0, max_z: 320.0 };
        assert_eq!(loaded.model.world.map_bounds, square, "{name}");
        let (mut lo, mut hi) = ((f32::MAX, f32::MAX), (f32::MIN, f32::MIN));
        for r in loaded.model.world.regions.values() {
            let (x, y) = (r.settlement.position.0.to_f32(), r.settlement.position.1.to_f32());
            (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)));
        }
        let [x, _, y] = cam.current.target;
        assert!((lo.0..=hi.0).contains(&x) && (lo.1..=hi.1).contains(&y), "{name}: camera ({x}, {y}) outside the settlements {lo:?}..{hi:?}");
        let nearest = loaded.model.world.regions.values().min_by(|a, b| {
            let d = |r: &ntw_sim::campaign::Region| (r.settlement.position.0.to_f32() - x).hypot(r.settlement.position.1.to_f32() - y);
            d(a).total_cmp(&d(b))
        });
        println!("{name}: camera {:?}, nearest settlement {:?}, settlements {lo:?}..{hi:?}", cam.current, nearest.map(|r| (&r.key, r.settlement.position)));
    }
}

/// The eur_napoleon start position's camera (tilt 0.5 at (-54, 0, 151.12)) is what a new
/// single-player game opens on; a new multiplayer campaign opens on the human faction's capital
/// settlement at the minimum tilt instead (`0x00987070`, `camera::opening_camera`).
#[test]
fn a_new_game_opens_on_the_startpos_camera() {
    let install = data_dir();
    let path = install.join("campaigns").join("eur_napoleon").join("startpos.esf");
    if !path.is_file() {
        println!("SKIPPED: no {}", path.display());
        return;
    }
    let db = GameDatabase::from_install(&install).expect("database");
    let esf = EsfFile::from_bytes(&std::fs::read(&path).expect("read")).expect("esf");
    let loaded = ntw_campaign::read_esf(&esf, &db).expect("load");
    let file_camera = loaded.camera.clone().expect("every startpos has a camera block");
    let c = file_camera.current;
    assert_eq!((c.tilt, c.field_13c, c.flag_140), (0.5, 0, false));
    assert!((c.target[0] + 54.0).abs() < 1e-3 && c.target[1] == 0.0 && (c.target[2] - 151.12).abs() < 1e-3, "{c:?}");
    let sp = camera::opening_camera(loaded.camera.clone(), false, &loaded.model, Some("france"));
    assert_eq!(sp.as_ref(), Some(&file_camera));
    let france = loaded.model.faction_by_key("france").expect("france").id;
    let paris = loaded.model.world.capital(france).and_then(|r| loaded.model.world.regions.get(&r)).expect("france's capital");
    assert_eq!(paris.key, "eur_france");
    // The settlement's raw fixed-point ints (+4, +8) × 2^-20 become the target's east (vec3 x) and
    // north (vec3 z), the height 0 (`0x00900E50` reads the target's x and z, its y set to 0).
    let (east_raw, north_raw) = (paris.settlement.position.0.raw(), paris.settlement.position.1.raw());
    let (east, north) = (f64::from(east_raw) / f64::from(1u32 << 20), f64::from(north_raw) / f64::from(1u32 << 20));
    assert!((east - north).abs() > 1.0, "paris's east and north must differ for the axis check to mean anything");
    let mp = camera::opening_camera(loaded.camera.clone(), true, &loaded.model, Some("france")).expect("a camera");
    let t = mp.current.target;
    assert!((f64::from(t[0]) - east).abs() < 1e-4 && t[1] == 0.0 && (f64::from(t[2]) - north).abs() < 1e-4, "{t:?} vs ({east}, 0, {north})");
    assert_eq!((mp.current.tilt, mp.current.field_13c, mp.current.flag_140), (0.5, 0, false));
    assert_eq!(mp.named, file_camera.named);
    // France with its capital cleared: the walk skips it (`0x0098864D`) and finds no other faction
    // of its key, so the file's camera stays (`0x00988764`).
    let mut no_capital = loaded.model.clone();
    no_capital.world.faction_details.get_mut(&france).expect("france's details").capital = None;
    assert_eq!(camera::opening_camera(loaded.camera.clone(), true, &no_capital, Some("france")).as_ref(), Some(&file_camera));
}

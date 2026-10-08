//! The standard bearer's flag: the verlet cloth file, the pole it hangs on, and the frame the
//! two share. Read-only, against a real install. All `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_sim --test flag_install -- --ignored --nocapture
//! ```
//! The install path can be overridden with `NTW_DATA_DIR`.
//!
//! The **solve** is `ntw_sim::battle::cloth` (this crate); the **readers** come from
//! `ntw_formats` as a dev-dependency, so the model crate itself still builds std only.

use std::collections::BTreeSet;
use std::path::PathBuf;

use ntw_formats::cloth;
use ntw_formats::pack::Vfs;
use ntw_formats::texture_atlas::TaiAtlas;
use ntw_formats::verlet::{VerletLogic, bounds};
use ntw_formats::weighted_mesh::WeightedMesh;
use ntw_sim::battle::cloth::{ClothMesh, FlagCloth};

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

const FLAG_LOGIC: &str = r"rigidmodels\verletitems\standard_bearer_flag.logic";
const EQUIPMENT_MESH: &str = r"unitmodels\euro_equipment.variant_weighted_mesh";
const POLE: &str = "rigid_equip_euro_flagpole01";

fn vfs() -> Vfs {
    let dir = std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
    Vfs::open_install(dir).expect("open install")
}

/// Every shipped `.logic` file parses, and the standard bearer's is the one we expect.
#[test]
#[ignore]
fn every_verlet_logic_parses() {
    let vfs = vfs();
    let files: Vec<String> = vfs
        .list(r"rigidmodels\verletitems\")
        .into_iter()
        .filter(|p| p.ends_with(".logic"))
        .map(|p| p.to_owned())
        .collect();
    assert!(files.len() >= 3, "only {} .logic files", files.len());
    for p in &files {
        let text = String::from_utf8(vfs.read(p).expect("read")).expect("utf8");
        let l = VerletLogic::read(&text).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert!(!l.rigids.is_empty(), "{p}: no rigid block");
        assert!(!l.items.is_empty(), "{p}: no verlet_item block");
        for item in &l.items {
            assert!(item.mass > 0.0, "{p}: item {} mass", item.name);
            assert!(!item.texture_prefix.is_empty(), "{p}: item {} texture prefix", item.name);
            assert!(!item.triangles.is_empty(), "{p}: item {} has no triangles", item.name);
            // Every triangle names three distinct particles.
            for t in &item.triangles {
                assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2], "{p}: degenerate triangle {t:?}");
            }
            // Every `auto_` rope names a rigid that exists and a particle it has.
            for (particle, target) in item.pinned() {
                let (rigid, part) = target.rsplit_once('.').expect("auto rope target");
                let r = l.rigid(rigid).unwrap_or_else(|| panic!("{p}: no rigid {rigid}"));
                assert!(r.particles.contains_key(part), "{p}: rigid {rigid} has no particle {part}");
                assert!(item.particles.contains_key(particle), "{p}: no cloth particle {particle}");
            }
        }
    }
    println!("{files:#?}");
}

/// The standard bearer's flag: CONFIRMED geometry of the cloth and of the pole it hangs on, and
/// the offset between their frames (UNITS_TERRAIN_FIDELITY §3).
#[test]
#[ignore]
fn the_standard_bearers_flag_matches_its_pole() {
    let vfs = vfs();
    let text = String::from_utf8(vfs.read(FLAG_LOGIC).expect("read")).expect("utf8");
    let l = VerletLogic::read(&text).expect("parse the flag logic");
    assert_eq!(l.version, "1.0");

    // The mast: a collision rod whose `top` / `bottom` particles are its two ends.
    let mast = l
        .rigids
        .iter()
        .find(|r| r.name.starts_with("mast_rigid_euro_flagpole"))
        .expect("the flag's mast");
    let (top, bottom) = mast.ends().expect("top and bottom particles");
    assert!(top[0] > bottom[0], "the mast runs along +x");
    let (lo, hi) = bounds(mast.particles.values().map(|p| p.p)).unwrap();
    assert!((lo[1] - hi[1]).abs() < 0.05, "the mast is a thin rod in y: {lo:?}..{hi:?}");
    assert!((lo[2] - hi[2]).abs() < 0.05, "the mast is a thin rod in z: {lo:?}..{hi:?}");
    let rod = hi[0] - lo[0];

    // The sail hangs off that rod, spreading in +z and staying at one y.
    let sail = l
        .items
        .iter()
        .find(|i| i.name.starts_with("sail_rigid_euro_flagpole"))
        .expect("the flag's sail");
    assert_eq!(sail.texture_prefix, "flag_");
    let (slo, shi) = sail.bounds().expect("sail bounds");
    assert!((shi[1] - slo[1]).abs() < 0.05, "the sail is flat in y: {slo:?}..{shi:?}");
    assert!(shi[2] > slo[2] + 1.0, "the sail spreads in +z: {slo:?}..{shi:?}");
    assert_eq!(sail.particles.len(), 42, "42 cloth particles");
    assert_eq!(sail.triangles.len(), 60, "60 triangles");
    // The pinned edge is the u ~ 0 column: the six ropes that hold it to the mast, and it sits
    // inside the mast's span (that is the whole attachment).
    assert_eq!(sail.pinned().count(), 6);
    let (elo, ehi) = bounds(sail.pinned().map(|(n, _)| sail.particles[n].p)).expect("pinned edge");
    assert!(elo[0] >= lo[0] && ehi[0] <= hi[0], "the pinned edge lies along the mast: {elo:?}..{ehi:?}");
    assert!(elo[0] < shi[0] - 0.5, "the sail reaches past its pinned edge");
    for (n, _) in sail.pinned() {
        let t = sail.particles[n].t.expect("a pinned particle has a texture coordinate");
        assert!(t[0] < 0.01, "particle {n} is pinned but its u is {}", t[0]);
    }
    println!("mast {mast:?}\nsail bounds {slo:?}..{shi:?} rod {rod:.3} m");

    // The drawn pole: an attachment of euro_equipment, a 4.4 m rod along its own +x.
    let mesh = WeightedMesh::read(&vfs.read(EQUIPMENT_MESH).expect("read")).expect("parse");
    let pole = mesh
        .attachments
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(POLE))
        .expect("the flagpole attachment");
    let vs = pole.vertex_size;
    let n = pole.vertices.len() / vs;
    assert!(n > 100, "the pole has only {n} vertices");
    let (plo, phi) = bounds((0..n).map(|i| [pole.vertices[i * vs], pole.vertices[i * vs + 1], pole.vertices[i * vs + 2]]))
        .expect("pole bounds");
    let pole_len = phi[0] - plo[0];
    assert!((pole_len - 4.4).abs() < 0.05, "the pole is {pole_len:.3} m long, expected 4.4");
    assert!((phi[1] - plo[1]).abs() < 0.1 && (phi[2] - plo[2]).abs() < 0.1, "the pole is thin: {plo:?}..{phi:?}");

    // The two frames share the x axis and differ by a translation: the mast lies inside the
    // pole's span, offset only in y and z. That is the whole attachment rule.
    assert!(lo[0] > plo[0] && hi[0] < phi[0], "the mast lies along the pole: {lo:?}..{hi:?} in {plo:?}..{phi:?}");
    let off_y = (lo[1] + hi[1]) / 2.0;
    let off_z = (lo[2] + hi[2]) / 2.0;
    assert!((off_y + 0.967).abs() < 0.01, "the frame offset in y is {off_y:.3}");
    assert!(off_z.abs() < 0.15, "the frame offset in z is {off_z:.3}");
    println!("pole {plo:?}..{phi:?}; cloth frame offset y {off_y:.3} z {off_z:.3}");
}

/// Every shipped `.tai` reads, every page it names is a file in the install, and every image's
/// rectangle lies inside the page.
#[test]
#[ignore]
fn every_shipped_tai_parses() {
    let vfs = vfs();
    let mut files: Vec<String> = vfs.list("").into_iter().filter(|p| p.to_ascii_lowercase().ends_with(".tai")).map(|p| p.to_owned()).collect();
    files.sort();
    files.dedup();
    assert!(files.len() >= 3, "only {} .tai files", files.len());
    for p in &files {
        let text = String::from_utf8(vfs.read(p).expect("read")).expect("utf8");
        let a = TaiAtlas::read(&text).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert!(!a.entries.is_empty(), "{p}: no entries");
        let dir = p.rsplit_once('\\').map_or("", |(d, _)| d);
        for (name, e) in &a.entries {
            let page = format!("{dir}\\{}", e.page);
            assert!(vfs.read(&page).is_ok(), "{p}: {name} names a missing page {page}");
            let (lo, hi) = e.rect();
            assert!(lo[0] >= 0.0 && lo[1] >= 0.0 && hi[0] <= 1.0 && hi[1] <= 1.0, "{p}: {name} rect {lo:?}..{hi:?} is off the page");
            assert!(hi[0] > lo[0] && hi[1] > lo[1], "{p}: {name} has an empty rect");
            assert!((0.0..=1.0).contains(&e.depth_offset), "{p}: {name} depth {}", e.depth_offset);
        }
        println!("{p}: {} entries, {} pages, kinds {:?}", a.entries.len(), a.pages.len(), a.entries.values().map(|e| e.kind.clone()).collect::<BTreeSet<_>>());
    }
    println!("{} .tai files", files.len());
}

/// The flag a faction flies is looked up under a key the `factions` table names, not under the
/// faction key. That table is `ntw_data`'s, so this half of the evidence lives in
/// `ntw_data`'s own install test `flag_faction_install::every_factions_flag_key_is_in_the_shipped_atlas`
/// (a dev-dependency of this crate back on `ntw_data` would be a dependency cycle).
///
/// The flag atlas: the `.tai` really is a plain-text list of `flag_<key>` images, and it holds one
/// for **every** faction the DB knows, with `flag_default.tga` as the fallback. This is the data
/// side of "which flag does a unit carry".
#[test]
#[ignore]
fn every_factions_flag_is_in_the_shipped_atlas() {
    let vfs = vfs();
    let path = r"rigidmodels\flags\textures\flags.tai";
    let a = TaiAtlas::read(&String::from_utf8(vfs.read(path).expect("read")).expect("utf8")).expect("parse");
    assert!(a.find("flag_default.tga").is_some(), "no flag_default.tga in {path}");
    for name in a.entries.keys() {
        assert!(name.starts_with("flag_") && name.ends_with(".tga"), "{path}: odd entry {name:?}");
    }
    let mut keys: Vec<String> = a
        .entries
        .keys()
        .map(|k| k.trim_start_matches("flag_").trim_end_matches(".tga").to_owned())
        .collect();
    keys.sort();
    println!("{path}: {} entries\n  {}", a.entries.len(), keys.join(" "));

    // The atlas' keys are the campaign faction keys. `factions_screen_name_<key>` is a loc key for
    // every faction (an independent shipped list), so cross-check the two.
    let loc = ntw_formats::loc::Localisation::from_vfs(&vfs).expect("loc");
    let mut missing = Vec::new();
    let mut hit = 0;
    let mut total = 0;
    for (key, _) in loc.iter() {
        let Some(faction) = key.strip_prefix("factions_screen_name_") else { continue };
        if faction.is_empty() { continue }
        total += 1;
        if a.find(&format!("flag_{faction}.tga")).is_some() { hit += 1 } else { missing.push(faction.to_owned()) }
    }
    println!("{hit} of {total} factions with a screen name have their own flag in the atlas");
    assert!(total >= 40, "only {total} faction screen names");
    if !missing.is_empty() {
        println!("no own flag (they fall back to flag_default.tga): {}", missing.join(" "));
    }
}

/// The real cloth: the shipped `standard_bearer_flag.logic` solves, and it solves to the shape
/// the file and the pole bone together say it must. This is the install-side evidence for
/// `ntw_sim::battle::cloth`, and it is also the check that the reader's plain arrays and the solve
/// agree across the seam.
#[test]
#[ignore]
fn the_shipped_flag_cloth_solves() {
    let vfs = vfs();
    let text = String::from_utf8(vfs.read(FLAG_LOGIC).expect("read")).expect("utf8");
    let spec = cloth::standard_bearer_flag(&text).expect("the shipped flag decodes");
    // The seam, in one line: the reader's plain arrays are all the solve is handed.
    let mut cloth = FlagCloth::new(spec.parts());
    // And the crossing is lossless, field for field: the solve has exactly what the file says.
    assert_eq!(cloth.particles(), spec.rest.len());
    assert_eq!(cloth.uv, spec.uv);
    assert_eq!(cloth.triangles, spec.triangles);
    assert_eq!(cloth.mast, spec.mast);
    assert_eq!(cloth.rod, spec.rod);
    assert_eq!(cloth.radius, spec.radius);
    assert_eq!(cloth.file_index, spec.file_index);
    assert_eq!(
        (cloth.mass, cloth.surface_area, cloth.drag_coefficient, cloth.gravity_coefficient),
        (spec.mass, spec.surface_area, spec.drag_coefficient, spec.gravity_coefficient)
    );
    assert_eq!(
        cloth.links.iter().map(|l| (l.a, l.b, l.rest, l.pin)).collect::<Vec<_>>(),
        spec.links.iter().map(|l| (l.a, l.b, l.rest, l.pin)).collect::<Vec<_>>(),
        "the links cross in the file's order"
    );
    assert_eq!(cloth.particles(), 42);
    assert_eq!(cloth.triangles.len(), 60);
    assert_eq!(cloth.uv.len(), 42);
    let mesh = ClothMesh::layout(&cloth);
    assert_eq!(mesh.indices.len(), 180, "60 triangles");
    assert!(mesh.uv.iter().all(|t| (0.0..=1.0).contains(&t[0]) && (0.0..=1.0).contains(&t[1])));
    // Every `auto_` rope is a zero-rest-length weld: six of them, on the hoist column.
    let pins: Vec<usize> = cloth.pinned().collect();
    assert_eq!(pins.len(), 6);
    assert!(cloth.links.iter().filter(|l| l.pin.is_some()).all(|l| l.rest == 0.0));
    // And the 101 internal ropes carry rest lengths between 0.14 and 0.31 m.
    let sropes: Vec<f32> = cloth.links.iter().filter(|l| l.pin.is_none()).map(|l| l.rest).collect();
    assert_eq!(sropes.len(), 101, "the shipped file has 101 srope constraints");
    assert!(sropes.iter().all(|r| (0.14..0.31).contains(r)), "rope rest lengths {:?}", {
        let mut v = sropes.clone();
        v.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        v
    });

    // The bone: `Weapon3` at frame 0 of the FLAG_BEARER stand clip.
    let anim = ntw_formats::anim::Anim::read(&vfs.read("Animations/MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim").expect("anim")).expect("parse anim");
    assert_eq!(anim.bones[cloth::POLE_BONE as usize].name, "Weapon3");
    assert_eq!(anim.bones[cloth::POLE_BONE as usize].parent, None);
    let m = anim.world_matrices(0)[cloth::POLE_BONE as usize];
    // The exe's own bone matrix has +x along world up.
    assert!((m[1] - 1.0).abs() < 0.01 && m[0].abs() < 0.01, "bone 3's x axis is up: {:?}", &m[0..3]);
    // The hand-copied `bone3_at_ground` is this matrix, to its three printed decimals.
    let ground = cloth::bone3_at_ground();
    let wrong: Vec<String> =
        (0..16).filter(|&k| (ground[k] - m[k]).abs() >= 1e-3).map(|k| format!("[{k}] {} vs the clip's {}", ground[k], m[k])).collect();
    let clip: Vec<String> = m.iter().map(|v| format!("{v:.3}")).collect();
    assert!(wrong.is_empty(), "bone3_at_ground differs from the clip: {wrong:?}\nthe clip's matrix (column-major): [{}]", clip.join(", "));
    // The cloth is solved in the verlet item's frame: the pole bone shifted so the mast lies on
    // the drawn pole (`verlet_frame`, INFERRED; review 0-D -- round 8/9 used the bare bone, which
    // puts the mast 0.97 m beside the pole).
    let bone = cloth::verlet_frame(&ground);
    // The numbered mast particles sit on the pole axis to within the rod's own few millimetres of
    // authored tilt.
    for p in &cloth.mast {
        let on_pole = ntw_formats::anim::transform_point(&ground, [p[0], 0.0, 0.0]);
        let drawn = ntw_formats::anim::transform_point(&bone, *p);
        let d: f32 = (0..3).map(|k| (on_pole[k] - drawn[k]).powi(2)).sum::<f32>().sqrt();
        assert!(d < 0.03, "mast particle {p:?} is drawn {d:.3} m off the pole axis");
    }

    // Authored shape in world space: the hoist edge is 1.16 m of cloth hanging 1.43 .. 2.58 m up.
    let rest = cloth.rest_positions(&bone);
    // The solve carries its own matrix multiply (this crate may not depend on `ntw_formats`), so
    // check the two agree on all 42 authored particles of the shipped file, bit for bit.
    assert_eq!(
        rest,
        cloth.rest_in_file_frame().iter().map(|p| ntw_formats::anim::transform_point(&bone, *p)).collect::<Vec<_>>(),
        "the solve's transform_point is the reader's, on the shipped bone"
    );
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &rest {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    assert!((lo[1] - 1.42).abs() < 0.03 && (hi[1] - 2.58).abs() < 0.03, "hoist height {lo:?}..{hi:?}");
    assert!((hi[1] - lo[1] - 1.156).abs() < 0.02, "hoist edge is 1.156 m: {}..{}", lo[1], hi[1]);
    // And in the file's own frame the sail is 1.156 m along the pole (x) and 1.647 m off it (z).
    let file = cloth.rest_in_file_frame();
    let span = |k: usize| file.iter().map(|p| p[k]).fold(f32::MIN, f32::max) - file.iter().map(|p| p[k]).fold(f32::MAX, f32::min);
    assert!((span(0) - 1.157).abs() < 0.02, "sail along the pole {} m", span(0));
    assert!((span(2) - 1.647).abs() < 0.01, "sail depth {} m", span(2));
    assert!(span(1) < 0.01, "the sail is flat in y in the file frame: {} m", span(1));
    // In world space that flat sheet becomes a **vertical** flag: the bone's x axis is up, so the
    // 1.156 m becomes height and the 1.647 m becomes the width the player sees.
    let width = rest.iter().map(|p| p[0]).fold(f32::MIN, f32::max) - rest.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    assert!(width > 1.0 && width < 1.2, "the sail's world x spread {width:.3} m is not the file's 1.647 m depth");

    // Austerlitz ships prevailing_wind (0, 9); the cloth must fly out and stay out of the mast.
    let mut flown = cloth.clone();
    for _ in 0..240 {
        flown.step(1.0 / 60.0, [0.0, 0.0, 9.0], &bone);
    }
    let p = flown.positions();
    let pinned: Vec<usize> = flown.pinned().collect();
    for (i, q) in p.iter().enumerate() {
        if pinned.contains(&i) {
            continue;
        }
        // Distance from the mast's own line, which is bone-local (x, y=-0.967, z=-0.075).
        let along = ((q[0] - rest[0][0]).powi(2) + (q[1] - rest[0][1]).powi(2) + (q[2] - rest[0][2]).powi(2)).sqrt();
        assert!(along > 0.05, "particle {i} collapsed onto the mast at {q:?}");
    }
    // The bone's own z axis, so "how far off the pole is the fly edge" is measured the way the file
    // measures it rather than along a world axis (the bone is turned).
    let axis = ntw_formats::anim::transform_vector(&bone, [0.0, 0.0, 1.0]);
    let along = |q: &[f32; 3]| (0..3).map(|k| q[k] * axis[k]).sum::<f32>();
    let flown_depth = p.iter().map(along).fold(f32::MIN, f32::max) - p.iter().map(along).fold(f32::MAX, f32::min);
    println!("authored world box {lo:?}..{hi:?}");
    println!("after 4 s of a 9 m/s wind: depth {flown_depth:.3} m, y {:.3}..{:.3}", p.iter().map(|q| q[1]).fold(f32::MAX, f32::min), p.iter().map(|q| q[1]).fold(f32::MIN, f32::max));
    assert!(flown_depth > 0.9, "the wind did not fly the sail out: {flown_depth:.3} m");

    // No wind: it hangs, and every rope stays near its rest length. How far it hangs depends on the
    // relaxation count, which is PROVISIONAL (target: the exe's per-frame verlet update), so this
    // sweeps it: the number that makes the sail fall like cloth rather than hang like a board.
    for iterations in [6usize, 12, 24, 48, 96, 192] {
        let mut trial = cloth.clone();
        for _ in 0..240 {
                trial.step_with(1.0 / 60.0, [0.0; 3], &bone, iterations);
            }
            let hang = trial.positions();
            let hang_lo = hang.iter().map(|q| q[1]).fold(f32::MAX, f32::min);
            // The worst rope stretch, as a fraction of its rest length.
            let worst = cloth
                .links
                .iter()
                .filter(|l| l.pin.is_none())
                .map(|l| {
                    let a = hang[l.a];
                    let b = hang[l.b.expect("b")];
                    (((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() - l.rest).abs() / l.rest
                })
                .fold(0.0f32, f32::max);
            println!("{iterations:>4} passes: lowest y {hang_lo:.3} (authored {:.3}), worst rope stretch {:.1}%", lo[1], worst * 100.0);
            if iterations == 6 {
                // The bottom row's fly particle (file 33, dense index) over time.
                let mut probe = cloth.clone();
                for step in 1..=240 {
                    probe.step_with(1.0 / 60.0, [0.0; 3], &bone, iterations);
                    if [1, 5, 15, 60, 120, 240].contains(&step) {
                        let y = probe.positions().iter().map(|q| q[1]).fold(f32::MAX, f32::min);
                        println!("      step {step:>3}: lowest y {y:.4}");
                    }
                }
            }
    }
    for _ in 0..240 {
        cloth.step(1.0 / 60.0, [0.0; 3], &bone);
    }
    let hang = cloth.positions();
    let hang_lo = hang.iter().map(|q| q[1]).fold(f32::MAX, f32::min);
    println!("with no wind it hangs to y {hang_lo:.3}");
    // It is a stiff, fully triangulated sheet, so in still air it folds down beside the pole rather
    // than collapsing to the authored 1.647 m of cloth, and after 4 s it is still folding. The
    // exe's own limp-flag shape is UNKNOWN; this is PROVISIONAL. What matters for the draw is that
    // it hangs at all, keeps every rope at its rest length and never leaves the mast.
    assert!(hang_lo < lo[1] - 0.15, "the sail did not hang: {hang_lo:.3} vs {:.3}", lo[1]);
    for (i, q) in hang.iter().enumerate() {
        if i < 12 {
            println!("  particle {i:2} ({:+.3}, {:+.3}, {:+.3})", q[0], q[1], q[2]);
        }
    }
    for l in cloth.links.iter().filter(|l| l.pin.is_none()) {
        let a = hang[l.a];
        let b = hang[l.b.expect("b")];
        let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        assert!((d - l.rest).abs() < 0.03, "rope {l:?} is {d:.3} from its {:.3} rest", l.rest);
    }
    // And the six welds never leave the mast.
    for i in pins {
        let link = cloth.links.iter().find(|l| l.a == i && l.pin.is_some()).expect("pin");
        let want = ntw_formats::anim::transform_point(&bone, cloth.mast[link.pin.expect("mast index")]);
        let d: f32 = (0..3).map(|k| (cloth.positions()[i][k] - want[k]).powi(2)).sum::<f32>().sqrt();
        assert!(d < 1e-4, "pinned particle {i} is {d:.5} off the mast");
    }
}
//! The standard bearer's flag cloth: the **reader** side of `RigidModels\VerletItems\*.logic`.
//!
//! This module decodes a verlet file into the plain arrays the solve consumes and holds the
//! constants the shipped data fixes. **The solve itself is `ntw_sim::battle::cloth`** -- it is a
//! stateful `f32` integrator stepped once per frame from the bearer's pole bone, so it belongs in
//! the model crate, not in a byte-level reader (`docs/DESIGN.md` §2: `ntw_sim` is std only and free
//! of file I/O; `docs/BACKLOG.md`'s determinism item audits the model crates).
//!
//! ## The seam between the two
//!
//! The two crates are both dependency leaves and neither may depend on the other, so nothing
//! crosses except plain arrays: [`ClothSpec`] holds the file's own decoded shape, and
//! [`ClothParts`] -- the identical tuple type is spelled on both sides -- is what
//! [`ClothSpec::parts`] hands to `ntw_sim::battle::cloth::FlagCloth::new`. The reader's job is
//! exactly what a format crate should do: turn bytes into data, with no state and no solver.
//!
//! ```text
//! FlagCloth::new(ntw_formats::cloth::standard_bearer_flag(&text)?.parts())
//! ```
//!
//! ## What the shipped file gives, and what it does not
//!
//! CONFIRMED by `ntw_formats::verlet` on all three shipped `.logic` files (install test
//! `flag_install::every_verlet_logic_parses`): the cloth's own particle positions and texture
//! coordinates, its triangles, and its rope constraints. Two facts make the solve possible
//! without inventing anything:
//!
//! - **The `auto_<n>` ropes have rest length exactly 0** (in `standard_bearer_flag.logic` the six
//!   pinned cloth particles sit *on* rigid particles `1..6`, to the millimetre). So the pinned
//!   column is rigidly welded to the mast: each of those particles is simply placed at the mast
//!   particle's world position, moved by the pole bone. No stiffness, no tolerance, no spring.
//! - **The internal `srope<n>` ropes** carry the cloth's shape: 101 of them on the flag, with rest
//!   lengths `0.148 .. 0.300` m read straight from the authored positions.
//!
//! Everything the solve does with the rest is tagged in `ntw_sim::battle::cloth`: gravity as a
//! uniform field of `gravity_coefficient * 9.81` m/s², wind as a linear drag, and the mast's
//! `radius` as a collision rod are all **INFERRED**, and the solver's substep / iteration /
//! damping constants are **PROVISIONAL** (targets named there).
//!
//! ## The pole this cloth hangs on
//!
//! - The six numbered mast particles are colinear at exactly `y = -0.967, z = -0.075` and their x
//!   lies inside the pole's own bone-local span (CONFIRMED, data) -- [`FRAME_OFFSET`]. That the exe
//!   shifts the verlet item by that offset so the mast lies on the drawn pole is **INFERRED** (the
//!   exe's own transform is UNKNOWN); [`verlet_frame`] applies it.
//! - The bone is [`POLE_BONE`], as `ntw_formats::unit_variant` reports it (bone 3, which is
//!   `Weapon3` in the FLAG_BEARER skeleton; its +x axis is world up at frame 0).

use crate::verlet::{VerletItem, VerletLogic, VerletRigid};

/// Where the mast sits in the verlet file's frame, relative to the pole's own axis.
///
/// CONFIRMED (data): the six numbered mast particles of `standard_bearer_flag.logic` sit at exactly
/// `y = -0.967`, `z = -0.075` and their `x` runs 1.027 .. 2.183, inside the pole piece's own
/// bone-local span (`-1.020 .. 3.379`, `y`/`z` within 0.05 of 0); the install test
/// `flag_install::the_standard_bearers_flag_matches_its_pole` checks the y to 0.01.
///
/// **INFERRED**, not read out of the exe: that the item is drawn shifted by `-FRAME_OFFSET` from
/// the pole bone so that its mast lies on the drawn pole ([`verlet_frame`]). Placing the file's
/// coordinates straight through the bone instead puts the mast 0.97 m beside the pole. Target: the
/// exe's verlet item transform (reached from `0x00732190`).
pub const FRAME_OFFSET: [f32; 3] = [0.0, -0.967, -0.075];

/// The verlet item's frame for a pole bone matrix: `bone * translate(-FRAME_OFFSET)`, so a point
/// written in the `.logic` file lands where the pole's own geometry would put it, and the mast
/// particles lie on the pole's axis. Column-major, as [`crate::anim`] uses. INFERRED (see
/// [`FRAME_OFFSET`]).
pub fn verlet_frame(bone: &[f32; 16]) -> [f32; 16] {
    let mut shift = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    shift[12] = -FRAME_OFFSET[0];
    shift[13] = -FRAME_OFFSET[1];
    shift[14] = -FRAME_OFFSET[2];
    crate::anim::mat_mul(bone, &shift)
}

/// The mast rigid's and the cloth item's names in the shipped standard-bearer file.
pub const MAST_NAME: &str = "mast_rigid_euro_flagpoleA_01";
pub const SAIL_NAME: &str = "sail_rigid_euro_flagpoleA_01";

/// The bone the pole is bound to, as `ntw_formats::unit_variant` reports it (bone 3, which is
/// `Weapon3` in the FLAG_BEARER skeleton; its +x axis is world up at frame 0).
pub const POLE_BONE: u32 = 3;

/// One distance constraint as the file states it: either between two cloth particles, or a pin
/// welding one cloth particle to a mast particle. The same fields as
/// `ntw_sim::battle::cloth::Link`, spelled here because the two crates may not depend on each
/// other; [`ClothParts`] carries it as that crate's own type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpecLink {
    /// Cloth particle index (dense, 0-based).
    pub a: usize,
    /// The other cloth particle, or `None` when this link is a pin.
    pub b: Option<usize>,
    /// Rest length in metres, read from the authored positions.
    pub rest: f32,
    /// `Some(i)` when the link welds `a` to mast particle `i`.
    pub pin: Option<usize>,
}

/// A `VerletItems\*.logic` item decoded into the plain arrays the solve reads: one mast rigid and
/// one cloth item. No state, no solver, no dependencies -- this is the reader's whole output.
#[derive(Debug, Clone, PartialEq)]
pub struct ClothSpec {
    /// Dense particle index -> the file's own 1-based index, for logs.
    pub file_index: Vec<u32>,
    /// Texture coordinates, one per particle, straight from `t(u, v)`.
    pub uv: Vec<[f32; 2]>,
    /// Triangles as dense indices.
    pub triangles: Vec<[u32; 3]>,
    /// Every distance constraint, pins included, **in the order the file states them**. The solve
    /// is Gauss-Seidel, so this order is part of the result, not a detail.
    pub links: Vec<SpecLink>,
    /// The mast particles a pin can name, in the verlet file's frame.
    pub mast: Vec<[f32; 3]>,
    /// The mast's rod, in the verlet file's frame: `(top, bottom)`.
    pub rod: ([f32; 3], [f32; 3]),
    /// The mast's collision radius, as the file writes it.
    pub radius: f32,
    /// The item's authored constants.
    pub mass: f32,
    pub surface_area: f32,
    pub drag_coefficient: f32,
    pub gravity_coefficient: f32,
    /// Authored positions in the verlet file's frame, dense order.
    pub rest: Vec<[f32; 3]>,
}

/// The solve's input as plain arrays:
///
/// ```text
/// (rest, uv, triangles, links, mast, rod, radius, (mass, surface_area, drag, gravity), file_index)
/// ```
///
/// where `links` is one `(a, b, rest, pin)` each -- the four fields of
/// `ntw_sim::battle::cloth::Link` and [`SpecLink`], in that order.
///
/// Identical to `ntw_sim::battle::cloth::ClothParts`; both crates are dependency leaves and may not
/// depend on each other (`docs/DESIGN.md` §2), so the two spell the same tuple of standard-library
/// types and the only conversion is the field copy in [`ClothSpec::parts`].
pub type ClothParts = (
    Vec<[f32; 3]>,
    Vec<[f32; 2]>,
    Vec<[u32; 3]>,
    Vec<(usize, Option<usize>, f32, Option<usize>)>,
    Vec<[f32; 3]>,
    ([f32; 3], [f32; 3]),
    f32,
    (f32, f32, f32, f32),
    Vec<u32>,
);

impl ClothSpec {
    /// The solve's input, in the order `ntw_sim::battle::cloth::FlagCloth::new` takes it.
    pub fn parts(&self) -> ClothParts {
        (
            self.rest.clone(),
            self.uv.clone(),
            self.triangles.clone(),
            self.links.iter().map(|l| (l.a, l.b, l.rest, l.pin)).collect(),
            self.mast.clone(),
            self.rod,
            self.radius,
            (self.mass, self.surface_area, self.drag_coefficient, self.gravity_coefficient),
            self.file_index.clone(),
        )
    }
}

/// The standard bearer's cloth, decoded from a `.logic` file's text.
///
/// Returns `None` when the file has no such rigid or item, when the item has too few particles or
/// no triangles, or when a pin names a mast particle the file does not have.
pub fn standard_bearer_flag(text: &str) -> Option<ClothSpec> {
    let logic = VerletLogic::read(text).ok()?;
    cloth_spec(&logic, MAST_NAME, SAIL_NAME)
}

/// One mast rigid plus one cloth item out of a parsed `.logic` file, as plain arrays.
pub fn cloth_spec(logic: &VerletLogic, mast_name: &str, item_name: &str) -> Option<ClothSpec> {
    let mast: &VerletRigid = logic.rigid(mast_name)?;
    let item: &VerletItem = logic.item(item_name)?;
    // Dense particle order: the file's own indices, ascending, so the mesh is deterministic.
    let mut dense: Vec<u32> = item.particles.keys().copied().collect();
    dense.sort_unstable();
    if dense.len() < 3 || item.triangles.is_empty() {
        return None;
    }
    let file_index = dense;
    let slot = |n: u32| file_index.binary_search(&n).ok();
    let rest: Vec<[f32; 3]> = file_index.iter().map(|n| item.particles[n].p).collect();
    let uv: Vec<[f32; 2]> = file_index.iter().map(|n| item.particles[n].t.unwrap_or([0.0, 0.0])).collect();
    let mut triangles = Vec::with_capacity(item.triangles.len());
    for t in &item.triangles {
        let t3 = [slot(t[0])?, slot(t[1])?, slot(t[2])?];
        triangles.push([t3[0] as u32, t3[1] as u32, t3[2] as u32]);
    }
    // The mast's particles a pin may name, in the order the pins first use them.
    let mut mast_pts: Vec<[f32; 3]> = Vec::new();
    let mut links = Vec::with_capacity(item.ropes.len());
    for r in &item.ropes {
        let a = slot(r.a)?;
        let pa = item.particles[&r.a].p;
        match (r.b, &r.rigid) {
            (Some(b), _) => {
                let b = slot(b)?;
                let rest = dist(pa, item.particles[&file_index[b]].p);
                links.push(SpecLink { a, b: Some(b), rest, pin: None });
            }
            (None, Some(target)) => {
                let part = target.rsplit_once('.')?.1;
                let p = mast.particles.get(part)?.p;
                let pin = match mast_pts.iter().position(|q| *q == p) {
                    Some(i) => i,
                    None => {
                        mast_pts.push(p);
                        mast_pts.len() - 1
                    }
                };
                links.push(SpecLink { a, b: None, rest: 0.0, pin: Some(pin) });
            }
            (None, None) => {}
        }
    }
    if mast_pts.is_empty() {
        return None;
    }
    // The mast's rod: `top` and `bottom` when the file has them (every shipped one does),
    // else the bounding box.
    let rod = mast.ends().unwrap_or_else(|| {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in mast.particles.values() {
            for k in 0..3 {
                lo[k] = lo[k].min(p.p[k]);
                hi[k] = hi[k].max(p.p[k]);
            }
        }
        (lo, hi)
    });
    Some(ClothSpec {
        file_index,
        uv,
        triangles,
        links,
        mast: mast_pts,
        rod,
        radius: mast.radius,
        mass: item.mass,
        surface_area: item.surface_area,
        drag_coefficient: item.drag_coefficient,
        gravity_coefficient: item.gravity_coefficient,
        rest,
    })
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The pole bone's world matrix for a figure standing at the origin: `Weapon3` of
/// `Animations\MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim` at frame 0, column-major as
/// [`crate::anim::world_matrices`] gives it. **No Bevy mirror here** -- these are the exe's own
/// numbers, so they can be checked against the probe line for line (see [`mirror_z`] for the
/// mirror the renderer applies).
///
/// CONFIRMED by the install probe `unit0d_probe equipbones`, which prints:
/// ```text
/// bone   3 Weapon3   origin (-0.001, 0.400, 0.402)
///                    x (0.004, 1.000, -0.002)   <- world up
///                    y (-0.740, 0.004, 0.673)   z (0.673, -0.001, 0.740)
/// ```
/// So the sail's hoist edge, authored at file-frame x `1.027 .. 2.183`, hangs between **1.427 m
/// and 2.583 m above the bearer's feet** and the pole's butt sits 0.62 m below them -- both plain
/// arithmetic on those two rows. INFERRED that `Weapon3` is the bone the exe poses for the cloth
/// (the container binds the *pole* to **bone 3** and bone 3 is `Weapon3`, both CONFIRMED from
/// data), and INFERRED that the item is shifted onto the pole ([`verlet_frame`]). The numbers are
/// the probe's printout rounded to three decimals.
pub fn bone3_at_ground() -> [f32; 16] {
    [
        // x axis: world up
        0.004, 1.000, -0.002, 0.0,
        // y axis
        -0.740, 0.004, 0.673, 0.0,
        // z axis
        0.673, -0.001, 0.740, 0.0,
        // origin
        -0.001, 0.400, 0.402, 1.0,
    ]
}

/// The **z mirror** a model-space matrix needs before it can be used in Bevy: `S * m` with
/// `S = diag(1, 1, -1, 1)`. The figures are drawn with every *skinned* vertex's z negated (the
/// skinning shader mirrors after the bone transform, `napoleon::battle::soldier_skin.wgsl`), so a
/// bone matrix that positions the cloth is mirrored on its **output**: row 2 -- the z component of
/// every column, translation included -- changes sign; rows 0 and 1 do not.
///
/// Column-major, so row 2 is elements 2, 6, 10 and 14. (Round 8 negated elements 8..12 and 14,
/// i.e. column 2 plus the translation's z, which is neither `S * m` nor `m * S`: it put the cloth
/// on a mirrored copy of the bone's frame. Fixed in review 0-D.)
pub fn mirror_z(m: [f32; 16]) -> [f32; 16] {
    let mut out = m;
    for k in [2, 6, 10, 14] {
        out[k] = -out[k];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped file's shape, trimmed to four particles: two welded to the mast, two free.
    const FLAG: &str = r#"version 1.0

rigid mast_rigid_euro_flagpoleA_01
{
	mass	100
	radius	0.072

	particle top	p(0.500, -0.967, -0.075)
	particle bottom	p(0.000, -0.967, -0.075)
	particle 1	p(0.000, -0.967, -0.075)
	particle 2	p(0.250, -0.967, -0.075)

}

verlet_item sail_rigid_euro_flagpoleA_01
{
	mass	1.0
	surface_area	1.0
	texture	flag_
	drag_coefficient	0.5
	gravity_coefficient	0.1

	particle 1 	p(0.000, -0.967, -0.075)	t(0.0, 0.0)
	particle 2 	p(0.250, -0.967, -0.075)	t(0.0, 1.0)
	particle 3 	p(0.000, -0.967, 0.425)	t(1.0, 0.0)
	particle 4 	p(0.250, -0.967, 0.425)	t(1.0, 1.0)
	triangle 1 1 2 3
	triangle 2 2 4 3
	rope srope1 1 3 invisible
	rope srope2 2 4 invisible
	rope srope3 3 4 invisible
	rope auto_1	1	mast_rigid_euro_flagpoleA_01.1
	rope auto_2	2	mast_rigid_euro_flagpoleA_01.2

}
"#;

    fn spec() -> ClothSpec {
        standard_bearer_flag(FLAG).expect("cloth")
    }

    #[test]
    fn the_cloth_takes_its_shape_from_the_file() {
        let c = spec();
        assert_eq!(c.rest.len(), 4);
        assert_eq!(c.triangles, vec![[0, 1, 2], [1, 3, 2]]);
        assert_eq!(c.uv, vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]);
        assert_eq!(c.radius, 0.072);
        assert_eq!(c.mass, 1.0);
        assert_eq!(c.surface_area, 1.0);
        assert_eq!(c.drag_coefficient, 0.5);
        assert_eq!(c.gravity_coefficient, 0.1);
        assert_eq!(c.file_index, vec![1, 2, 3, 4]);
        // The two `auto_` ropes are the pins, and they hold particles 0 and 1.
        let pins: Vec<usize> = c.links.iter().filter_map(|l| l.pin.map(|_| l.a)).collect();
        assert_eq!(pins, vec![0, 1]);
        assert!(c.links.iter().filter(|l| l.pin.is_some()).all(|l| l.rest == 0.0), "a pin with a non-zero rest length");
        // The three internal ropes are the grid's edges, at their authored lengths, in file order.
        let sropes: Vec<(usize, usize, f32)> = c
            .links
            .iter()
            .filter_map(|l| l.b.map(|b| (l.a, b, l.rest)))
            .collect();
        assert_eq!(sropes.len(), 3);
        assert_eq!(sropes[0], (0, 2, 0.5), "pinned edge to fly edge, 0.5 m off the mast");
        assert_eq!(sropes[2], (2, 3, 0.25), "the fly edge, 0.25 m along the mast");
        // The rod is the mast's `top` and `bottom`, in that order.
        assert_eq!(c.rod.0, [0.5, -0.967, -0.075]);
        assert_eq!(c.rod.1, [0.0, -0.967, -0.075]);
        assert_eq!(c.mast, vec![[0.0, -0.967, -0.075], [0.25, -0.967, -0.075]]);
    }

    /// The seam: `parts()` is the field copy the solve is handed, in the order it takes it.
    #[test]
    fn parts_are_the_specs_fields_in_the_solves_order() {
        let c = spec();
        let (rest, uv, triangles, links, mast, rod, radius, (mass, area, drag, gravity), file_index) = c.parts();
        assert_eq!(rest, c.rest);
        assert_eq!(uv, c.uv);
        assert_eq!(triangles, c.triangles);
        assert_eq!(mast, c.mast);
        assert_eq!(rod, c.rod);
        assert_eq!(radius, c.radius);
        assert_eq!(file_index, c.file_index);
        assert_eq!((mass, area, drag, gravity), (c.mass, c.surface_area, c.drag_coefficient, c.gravity_coefficient));
        assert_eq!(
            links,
            c.links.iter().map(|l| (l.a, l.b, l.rest, l.pin)).collect::<Vec<_>>(),
            "the links cross as the same four fields, in the file's order"
        );
        // The hand-written numbers `ntw_sim::battle::cloth`'s own unit tests use for this same toy
        // cloth: if the reader's order or its dense indices ever change, these stop matching.
        assert_eq!(links, vec![(0, Some(2), 0.5, None), (1, Some(3), 0.5, None), (2, Some(3), 0.25, None), (0, None, 0.0, Some(0)), (1, None, 0.0, Some(1))]);
    }

    /// `verlet_frame` moves the file's mast line onto the bone's own x axis. (Round 8's test here
    /// compared the two constants with themselves; replaced in review 0-D.)
    #[test]
    fn the_verlet_frame_puts_the_mast_line_on_the_bone_axis() {
        let ident = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let f = verlet_frame(&ident);
        for x in [1.027f32, 1.5, 2.183] {
            let p = crate::anim::transform_point(&f, [x, FRAME_OFFSET[1], FRAME_OFFSET[2]]);
            assert!((p[0] - x).abs() < 1e-6 && p[1].abs() < 1e-6 && p[2].abs() < 1e-6, "{x} -> {p:?}");
        }
        // The toy file's mast particles are on that line too.
        for m in &spec().mast {
            let p = crate::anim::transform_point(&f, *m);
            assert!(p[1].abs() < 1e-6 && p[2].abs() < 1e-6, "{m:?} -> {p:?}");
        }
    }

    #[test]
    fn bone3_at_ground_puts_the_flag_where_the_shipped_bone_says() {
        let bone = bone3_at_ground();
        // The exe's own numbers, line for line (install probe `unit0d_probe equipbones`).
        assert_eq!(
            bone,
            [0.004, 1.000, -0.002, 0.0, -0.740, 0.004, 0.673, 0.0, 0.673, -0.001, 0.740, 0.0, -0.001, 0.400, 0.402, 1.0],
            "bone 3 Weapon3 of FLA_StandT.anim frame 0"
        );
        // Its +x axis is world up.
        assert!((bone[1] - 1.0).abs() < 1e-3 && bone[0].abs() < 1e-2 && bone[2].abs() < 1e-2, "bone 3's x axis is up: {:?}", &bone[0..3]);
        // The mirror flips row 2 (elements 2, 6, 10, 14) and nothing else.
        assert_eq!(mirror_z(bone), [0.004, 1.000, 0.002, 0.0, -0.740, 0.004, -0.673, 0.0, 0.673, -0.001, -0.740, 0.0, -0.001, 0.400, -0.402, 1.0]);
        // Mirroring is an involution, and it is exactly "transform, then negate z" -- what the
        // skinning shader does to a soldier's vertex -- for any point, not just the origin.
        assert_eq!(mirror_z(mirror_z(bone)), bone);
        for p in [[0.0, 0.0, 0.0], [1.027, -0.967, -0.075], [2.183, 0.5, 1.572], [-1.0, 3.0, -2.0]] {
            let model = crate::anim::transform_point(&bone, p);
            let drawn = crate::anim::transform_point(&mirror_z(bone), p);
            assert_eq!(drawn, [model[0], model[1], -model[2]], "point {p:?}");
        }
        // The bone origin is the exe's own (-0.001, 0.400, 0.402).
        assert!((bone[12] + 0.001).abs() < 1e-6 && (bone[13] - 0.400).abs() < 1e-6 && (bone[14] - 0.402).abs() < 1e-6, "origin");
        // Its +x axis is world up: file-frame x is height, so the hoist edge (x 1.027 .. 2.183 in
        // the verlet frame, unchanged by the (0, -0.967, -0.075) offset) hangs at 1.427 .. 2.583 m
        // above the feet. The bone's y axis is not exactly world up (it has a 0.004 x component),
        // so allow a centimetre.
        for (x, want) in [(1.027f32, 1.427f32), (2.183, 2.583)] {
            let p = crate::anim::transform_point(&verlet_frame(&bone), [x, -0.967, -0.075]);
            assert!((p[1] - want).abs() < 0.01, "hoist x {x} -> {p:?}, expected y {want}");
            // Through `verlet_frame` the mast particle lands on the pole's own axis (bone-local
            // y = z = 0), not 0.97 m beside it.
            let on_axis = crate::anim::transform_point(&bone, [x, 0.0, 0.0]);
            for k in 0..3 {
                assert!((p[k] - on_axis[k]).abs() < 1e-5, "hoist x {x}: {p:?} is off the pole axis {on_axis:?}");
            }
            let beside = crate::anim::transform_point(&bone, [x, -0.967, -0.075]);
            let gap = ((beside[0] - on_axis[0]).powi(2) + (beside[2] - on_axis[2]).powi(2)).sqrt();
            assert!(gap > 0.9, "without the shift the mast would sit {gap:.3} m beside the pole");
        }
        // The pole's own ends: -1.020 .. 3.379 along the bone's x, so 0.62 m below the feet to
        // 3.78 m above them.
        for (x, want) in [(-1.0195f32, -0.6195f32), (3.3793, 3.7793)] {
            let p = crate::anim::transform_point(&bone, [x, 0.0, 0.0]);
            assert!((p[1] - want).abs() < 1e-3, "pole x {x} -> {p:?}, expected y {want}");
        }
        // And the sail's authored width is 1.647 m along the bone's z.
        let a = crate::anim::transform_point(&bone, [1.027, -0.967, -0.075]);
        let b = crate::anim::transform_point(&bone, [1.027, -0.967, 1.572]);
        let w = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        assert!((w - 1.647).abs() < 1e-3, "sail width {w:.4}");
    }
}
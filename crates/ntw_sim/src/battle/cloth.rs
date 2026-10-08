//! The standard bearer's flag cloth: the verlet **solve** for `RigidModels\VerletItems\*.logic`.
//!
//! **This is a display-side model, not battle rules.** Nothing here feeds the battle model back;
//! the cloth is stepped once per frame from the bearer's pole bone, exactly as the original's
//! verlet item is stepped in its display set-up (`0x00732190`, CONFIRMED as the loader of
//! `standard_bearer_flag.logic`). It lives in `ntw_sim` because it is a stateful `f32` integrator
//! over a per-frame wind vector -- which is what this crate is for. The *reader* stays in
//! `ntw_formats::verlet`, and `ntw_formats::cloth` turns a parsed file into the plain arrays
//! [`ClothParts`] below, so nothing is decoded here and this crate keeps **zero dependencies**
//! (`docs/DESIGN.md` §2: `ntw_sim` is std only and free of file I/O).
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
//! Everything below the rest lengths is tagged:
//!
//! - `mass`, `surface_area`, `drag_coefficient` and `gravity_coefficient` are **CONFIRMED
//!   values** (1.0, 1.0, 0.5, 0.1 on the flag). **How the exe combines them is INFERRED**: this
//!   reads gravity as a uniform field of `gravity_coefficient * 9.81` m/s² and wind as a linear
//!   drag of `drag_coefficient * (surface_area / mass)` against the relative wind velocity. The
//!   quadratic drag form is not excluded by the file, but at the shipped wind speed of 9 it would
//!   pin the sail rigid (40 m/s² against 1 m/s² of gravity), which is not a flag.
//! - The wind velocity is the battle's own `weather/prevailing_wind` (CONFIRMED present and read
//!   by `ntw_formats::battle_spec`; Austerlitz ships `(0, 9)`, and five shipped battle files ship
//!   `(0, 0)`). Reading the vector's own magnitude as metres per second is **INFERRED** -- the exe
//!   normalises the vector separately for the wind audio levels, so its speed unit is not the
//!   metre. **PROVISIONAL** target: the exe-side wind speed the verlet item is fed.
//! - The solver's iteration count and velocity damping are **PROVISIONAL**. Target: the exe-side
//!   per-frame verlet update reached from `0x00732190`.
//! - The mast is a collision rod of `radius` about its `bottom`..`top` segment, which is how the
//!   file's `radius` is used here. **INFERRED** (the file states the radius; the collision
//!   response is the natural reading).
//!
//! ## Coordinates
//!
//! The cloth is solved in **world space**, because the wind is known in world space and the mast
//! is welded to a bone whose world matrix the caller already has. Rest lengths are frame
//! independent, so reading them from the authored file-frame positions is exact. [`FlagCloth::reset`]
//! takes the bone matrix, and so does [`FlagCloth::rest_positions`].
//!
//! The matrix this module is handed is the **verlet item's** frame, not the bare pole bone: the
//! six mast particles sit at `y = -0.967, z = -0.075` in the file (CONFIRMED, data), and the
//! caller shifts the item so they lie on the drawn pole (`ntw_formats::cloth::verlet_frame`,
//! INFERRED -- the exe's own transform is UNKNOWN).
//!
//! ## Robustness (not the exe's; review 0-D)
//!
//! A non-finite or non-positive `dt` does nothing, a non-finite wind is treated as still air, the
//! explicit drag term is bounded so a tiny `mass` cannot make it unstable, and a cloth whose state
//! has gone non-finite is put back on its authored shape. [`FlagCloth::new`] drops any link or
//! triangle that names a particle or mast point the arrays do not have, so a malformed
//! [`ClothParts`] cannot index out of bounds. None of this changes a step on the shipped file.
//!
//! ## The boundary with the reader
//!
//! [`ClothParts`] is a tuple of **standard-library types on purpose**. The reader
//! (`ntw_formats::cloth`) and this crate are both dependency leaves and neither may depend on
//! the other, so the data crosses as arrays rather than as a type one of them owns. Both sides
//! spell the same tuple type (`ntw_formats::cloth::ClothParts`) and the only conversion is the
//! field copy inside [`ClothSpec::parts`](ntw_formats::cloth::ClothSpec::parts).

/// Standard gravity, m/s². The exe's cloth gravity is `gravity_coefficient * this`
/// (INFERRED; the file carries the coefficient and nothing else).
const GRAVITY: f32 = 9.81;

/// Constraint relaxation passes per **substep**. PROVISIONAL (target: the exe's verlet iteration
/// count).
pub const ITERATIONS: usize = 6;

/// Integration substeps per [`FlagCloth::step`]. PROVISIONAL (target: the exe-side substep count).
/// This matters more than [`ITERATIONS`]: one integration step plus a handful of projections per
/// frame leaves the sail hanging like a stiff board, because the projection undoes the whole
/// frame's gravity displacement each time. Substepping converges (the install test sweeps both).
pub const SUBSTEPS: usize = 8;

/// Velocity retained per step. PROVISIONAL (target: the same).
const DAMPING: f32 = 0.985;

/// Longest step the solver accepts, so a stalled frame cannot blow the cloth up.
const MAX_STEP: f32 = 1.0 / 20.0;

/// One distance constraint: either between two cloth particles, or a pin welding one cloth
/// particle to a mast particle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    /// Cloth particle index (dense, 0-based).
    pub a: usize,
    /// The other cloth particle, or `None` when this link is a pin.
    pub b: Option<usize>,
    /// Rest length in metres, read from the authored positions.
    pub rest: f32,
    /// `Some(i)` when the link welds `a` to mast particle `i`.
    pub pin: Option<usize>,
}

/// Everything the solve reads out of one cloth file, as plain arrays:
///
/// ```text
/// (rest, uv, triangles, links, mast, rod, radius, (mass, surface_area, drag, gravity), file_index)
/// ```
///
/// - `rest`: the authored particle positions in the verlet file's own frame, dense order.
/// - `uv`: one `t(u, v)` per particle; `triangles`: dense particle indices.
/// - `links`: the distance constraints **in the order the file states them**, one
///   `(a, b, rest, pin)` each -- exactly the fields of [`Link`], which the projection turns them
///   into. The solve is Gauss-Seidel, so the order is part of the result, not a detail.
/// - `mast`: the mast particles a weld can name, in the verlet file's frame; `rod`: the mast's
///   `(top, bottom)`; `radius`: its collision radius.
/// - the four authored constants, and `file_index`: dense index -> the file's own 1-based index,
///   for logs.
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

/// Transforms a point by a column-major 4x4 matrix. The same arithmetic, in the same order, as
/// `ntw_formats::anim::transform_point`; the install test asserts the two agree on the shipped
/// flag, because this crate may not depend on that one.
fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn norm3(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Only the tests measure a rope by hand; the solve itself never needs the distance between two
/// points, because the reader hands it the rest lengths.
#[cfg(test)]
fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    norm3(sub3(a, b))
}

/// A cloth ready to be stepped: dense particle arrays, the links, and the mast it hangs on.
#[derive(Debug, Clone)]
pub struct FlagCloth {
    /// Cloth particle index (dense, 0-based) -> the file's own 1-based index, for logs.
    pub file_index: Vec<u32>,
    /// Texture coordinates, one per particle, straight from `t(u, v)`.
    pub uv: Vec<[f32; 2]>,
    /// Triangles as dense indices.
    pub triangles: Vec<[u32; 3]>,
    /// Every distance constraint, pins included.
    pub links: Vec<Link>,
    /// The mast particles a pin can name, in the verlet file's frame.
    pub mast: Vec<[f32; 3]>,
    /// The mast's rod, in the verlet file's frame: `(top, bottom)`, the two particles the shipped
    /// files name.
    pub rod: ([f32; 3], [f32; 3]),
    /// The mast's collision radius.
    pub radius: f32,
    /// The item's authored constants.
    pub mass: f32,
    /// The item's authored constants.
    pub surface_area: f32,
    /// The item's authored constants.
    pub drag_coefficient: f32,
    /// The item's authored constants.
    pub gravity_coefficient: f32,
    /// Authored positions in the verlet file's frame, kept so the shape can be reset and so a
    /// caller can measure the sail as the file states it.
    rest: Vec<[f32; 3]>,
    /// Live **world** positions, and the previous step's, for the verlet integrator. They are not
    /// the authored positions until the cloth has been placed (see [`Self::placed`]).
    pos: Vec<[f32; 3]>,
    prev: Vec<[f32; 3]>,
    /// False until [`Self::reset`] has run, so the first [`Self::step`] places the cloth through
    /// the bone matrix instead of starting it in the file's own frame -- which would be a 2.4 m
    /// jump on the very first frame.
    placed: bool,
}

impl FlagCloth {
    /// Build the cloth from the plain arrays a `VerletItems\*.logic` item decodes to
    /// ([`ClothParts`]; `ntw_formats::cloth::ClothSpec::parts` builds one from a parsed file).
    ///
    /// The cloth starts in the file's own frame with no motion and is *not* placed: the first
    /// [`Self::step`] runs [`Self::reset`] through the bone matrix, so the sail never starts 2.4 m
    /// from where the pole is.
    pub fn new(parts: ClothParts) -> Self {
        let (rest, uv, triangles, raw_links, mast, rod, radius, (mass, surface_area, drag_coefficient, gravity_coefficient), file_index) =
            parts;
        // The reader's four-field links become the solve's `Link`s, in the same order. A link that
        // names a particle or mast point these arrays do not have is dropped (the reader never
        // produces one; this keeps a hand-built `ClothParts` from indexing out of bounds).
        let n = rest.len();
        let links = raw_links
            .into_iter()
            .map(|(a, b, rest, pin)| Link { a, b, rest, pin })
            .filter(|l| {
                l.a < n
                    && l.b.is_none_or(|b| b < n)
                    && l.pin.is_none_or(|m| m < mast.len())
                    && l.rest.is_finite()
            })
            .collect();
        let triangles = triangles.into_iter().filter(|t| t.iter().all(|&i| (i as usize) < n)).collect();
        // One texture coordinate per particle, whatever the caller handed over.
        let mut uv = uv;
        uv.resize(n, [0.0, 0.0]);
        Self {
            file_index,
            uv,
            triangles,
            links,
            mast,
            rod,
            radius,
            mass,
            surface_area,
            drag_coefficient,
            gravity_coefficient,
            prev: rest.clone(),
            pos: rest.clone(),
            placed: false,
            rest,
        }
    }

    /// How many cloth particles.
    pub fn particles(&self) -> usize {
        self.pos.len()
    }

    /// The live world positions: this is what is drawn.
    pub fn positions(&self) -> &[[f32; 3]] {
        &self.pos
    }

    /// The authored shape in world space, through the pole bone's matrix.
    pub fn rest_positions(&self, bone: &[f32; 16]) -> Vec<[f32; 3]> {
        self.rest.iter().map(|p| transform_point(bone, *p)).collect()
    }

    /// The authored positions in the **verlet file's own frame**, exactly as the file writes them.
    /// The rest lengths and the mesh layout both come from these, and the pole piece's bone-local
    /// box is in the same axes (up to the `(0, -0.967, -0.075)` offset).
    pub fn rest_in_file_frame(&self) -> &[[f32; 3]] {
        &self.rest
    }

    /// Put every particle back on its authored position and forget the motion: the cloth's first
    /// frame, and the fix when its owner teleports.
    pub fn reset(&mut self, bone: &[f32; 16]) {
        self.pos = self.rest_positions(bone);
        self.prev.clone_from(&self.pos);
        self.placed = true;
    }

    /// The pinned particles: those welded to the mast. They never move on their own.
    pub fn pinned(&self) -> impl Iterator<Item = usize> + '_ {
        self.links.iter().filter_map(|l| l.pin.map(|_| l.a))
    }

    /// Step the cloth.
    ///
    /// - `dt` seconds, clamped to `MAX_STEP`.
    /// - `wind` the wind velocity in m/s, **world** space.
    /// - `bone` the pole bone's world matrix, column-major 4x4 (`ntw_formats::anim`), which places
    ///   the mast's authored particles.
    pub fn step(&mut self, dt: f32, wind: [f32; 3], bone: &[f32; 16]) {
        self.step_with(dt, wind, bone, ITERATIONS)
    }

    /// Step the cloth with an explicit relaxation count. [`ITERATIONS`] is the default; the
    /// install test sweeps it against the shipped file, because the answer is not "any number":
    /// too few passes and the sail hangs like a stiff board instead of falling.
    ///
    /// - `dt` seconds, clamped to `MAX_STEP`.
    /// - `wind` the wind velocity in m/s, **world** space.
    /// - `bone` the pole bone's world matrix, column-major 4x4 (`ntw_formats::anim`), which places
    ///   the mast's authored particles.
    pub fn step_with(&mut self, dt: f32, wind: [f32; 3], bone: &[f32; 16], iterations: usize) {
        if !self.placed {
            // The authored positions are in the file's frame; the solve is in world space.
            self.reset(bone);
        }
        // `NaN.clamp(..)` is NaN and `NaN <= 0.0` is false, so test for a usable step explicitly:
        // a NaN step would poison every particle for good.
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let dt = dt.min(MAX_STEP);
        let wind = if wind.iter().all(|w| w.is_finite()) { wind } else { [0.0; 3] };
        if !bone.iter().all(|v| v.is_finite()) {
            return;
        }
        // Where the mast's particles are this frame.
        let mast_world: Vec<[f32; 3]> = self.mast.iter().map(|p| transform_point(bone, *p)).collect();
        let mut targets: Vec<Option<[f32; 3]>> = vec![None; self.pos.len()];
        for l in &self.links {
            if let Some(m) = l.pin {
                targets[l.a] = Some(mast_world[m]);
            }
        }
        // The rod in world space, for the collision push-out.
        let rod_a = transform_point(bone, self.rod.0);
        let rod_b = transform_point(bone, self.rod.1);
        // Gravity is a uniform field (the cloth is uniform, so no mass term); the wind term is per
        // unit mass through `drag_coefficient * surface_area / mass`.
        let h = dt / SUBSTEPS as f32;
        // The drag term below is explicit: its velocity part removes `drag * h` of the velocity
        // per substep, which overshoots (and then diverges) once `drag * h` passes 1. The shipped
        // flag has `drag * h` = 0.5 * 0.05 / 8 = 0.003, so this bound never touches it; it only
        // keeps a tiny or zero `mass` from blowing the cloth up.
        let drag = (self.drag_coefficient * self.surface_area / self.mass.max(1e-6)).clamp(0.0, 1.0 / h);
        let g = self.gravity_coefficient * GRAVITY;
        for _ in 0..SUBSTEPS {
            for (i, t) in targets.iter().enumerate() {
                if let Some(t) = *t {
                    // A pinned particle is placed, not integrated (its link's rest length is 0).
                    self.pos[i] = t;
                    self.prev[i] = t;
                    continue;
                }
                let v = sub3(self.pos[i], self.prev[i]);
                // The wind drags the cloth towards its own velocity; gravity pulls it down.
                let acc = [
                    drag * (wind[0] - v[0] / h),
                    drag * (wind[1] - v[1] / h) - g,
                    drag * (wind[2] - v[2] / h),
                ];
                self.prev[i] = self.pos[i];
                for k in 0..3 {
                    self.pos[i][k] += v[k] * DAMPING + acc[k] * h * h;
                }
            }
            // Relax the constraints, re-pinning and re-colliding each pass so the mast always wins.
            for _ in 0..iterations.max(1) {
                for l in &self.links {
                    match (l.pin, l.b) {
                        (Some(_), _) => {
                            if let Some(t) = targets[l.a] {
                                self.pos[l.a] = t;
                            }
                        }
                        (None, Some(b)) => {
                            let d = sub3(self.pos[b], self.pos[l.a]);
                            let len = norm3(d);
                            if len < 1e-6 {
                                continue;
                            }
                            let corr = (len - l.rest) / len * 0.5;
                            for (k, dk) in d.iter().enumerate() {
                                let c = dk * corr;
                                // The correction moves `prev` with `pos`: it is a *positional*
                                // fix, not a force. Leaving `prev` alone would cancel the
                                // velocity the integrator just built, and a stiff sheet pinned
                                // along a vertical edge then never falls at all -- the whole frame's
                                // gravity displacement is undone every frame. This one line is
                                // what makes the sail hang instead of hanging like a board.
                                self.pos[l.a][k] += c;
                                self.prev[l.a][k] += c;
                                self.pos[b][k] -= c;
                                self.prev[b][k] -= c;
                            }
                        }
                        (None, None) => {}
                    }
                }
                self.push_out_of_the_rod(rod_a, rod_b, &targets);
            }
        }
        for (i, t) in targets.iter().enumerate() {
            if let Some(t) = t {
                self.pos[i] = *t;
                self.prev[i] = *t;
            }
        }
        // A cloth that has gone non-finite stays that way forever; put it back on the mast.
        if !self.pos.iter().flatten().all(|v| v.is_finite()) {
            self.reset(bone);
        }
    }

    /// Keep the free particles out of the mast's rod (the file carries a `radius`; INFERRED that
    /// it is the cloth's collision radius against it).
    fn push_out_of_the_rod(&mut self, a: [f32; 3], b: [f32; 3], targets: &[Option<[f32; 3]>]) {
        if self.radius <= 0.0 {
            return;
        }
        let axis = sub3(b, a);
        let axis_len2 = norm3(axis).powi(2);
        if axis_len2 < 1e-12 {
            return;
        }
        for (i, pinned) in targets.iter().enumerate() {
            if pinned.is_some() {
                continue;
            }
            let p = self.pos[i];
            let along = ((p[0] - a[0]) * axis[0] + (p[1] - a[1]) * axis[1] + (p[2] - a[2]) * axis[2]) / axis_len2;
            let t = along.clamp(0.0, 1.0);
            let foot = [a[0] + axis[0] * t, a[1] + axis[1] * t, a[2] + axis[2] * t];
            let d = sub3(p, foot);
            let len = norm3(d);
            if len >= self.radius {
                continue;
            }
            // Degenerate: exactly on the axis, push straight up.
            let n = if len < 1e-6 { [0.0, 1.0, 0.0] } else { [d[0] / len, d[1] / len, d[2] / len] };
            self.pos[i] = [foot[0] + n[0] * self.radius, foot[1] + n[1] * self.radius, foot[2] + n[2] * self.radius];
        }
    }

    /// The particle normals, for lighting: each particle's own average of the triangles around
    /// it. A flat sail gives every particle one face normal, which is what a two-sided cloth
    /// wants anyway.
    pub fn normals(&self) -> Vec<[f32; 3]> {
        let mut out = vec![[0.0; 3]; self.pos.len()];
        for t in &self.triangles {
            let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let e1 = sub3(self.pos[b], self.pos[a]);
            let e2 = sub3(self.pos[c], self.pos[a]);
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            for i in [a, b, c] {
                for (k, nk) in n.iter().enumerate() {
                    out[i][k] += nk;
                }
            }
        }
        for n in &mut out {
            let l = norm3(*n);
            *n = if l > 1e-9 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0, 1.0, 0.0] };
        }
        out
    }
}

/// The mesh of a cloth, in the form the renderer wants: one flat triangle list and the particle
/// normals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ClothMesh {
    /// The live world positions, one per particle.
    pub positions: Vec<[f32; 3]>,
    /// One face normal per particle.
    pub normals: Vec<[f32; 3]>,
    /// The cloth's own `t(u, v)` per particle.
    pub uv: Vec<[f32; 2]>,
    /// Three indices per triangle.
    pub indices: Vec<u32>,
}

impl ClothMesh {
    /// The cloth's static mesh data (UVs and indices) with the positions and normals left empty.
    pub fn layout(cloth: &FlagCloth) -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uv: cloth.uv.clone(),
            indices: cloth.triangles.iter().flat_map(|t| t.iter().copied()).collect(),
        }
    }

    /// Fill `positions` and `normals` from the cloth's live state.
    pub fn update(&mut self, cloth: &FlagCloth) {
        self.positions = cloth.positions().to_vec();
        self.normals = cloth.normals();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped file's shape trimmed to four particles: two welded to the mast, two free.
    /// These are **hand-written numbers**, which is the point of the solve being in `ntw_sim`
    /// (`docs/DESIGN.md` §2: no file I/O, so it is testable with numbers). They are exactly what
    /// the toy `.logic` in `ntw_formats::cloth`'s own tests decodes to.
    fn parts() -> ClothParts {
        (
            // rest: the two pinned particles on the mast, then the two free ones 0.5 m off it
            vec![
                [0.000, -0.967, -0.075],
                [0.250, -0.967, -0.075],
                [0.000, -0.967, 0.425],
                [0.250, -0.967, 0.425],
            ],
            vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]],
            vec![[0, 1, 2], [1, 3, 2]],
            // `srope1 1 3`, `srope2 2 4`, `srope3 3 4`, `auto_1 1 mast.1`, `auto_2 2 mast.2`,
            // in the order the file states them, as the reader hands them over: `(a, b, rest, pin)`.
            vec![
                (0, Some(2), 0.5, None),
                (1, Some(3), 0.5, None),
                (2, Some(3), 0.25, None),
                (0, None, 0.0, Some(0)),
                (1, None, 0.0, Some(1)),
            ],
            vec![[0.0, -0.967, -0.075], [0.25, -0.967, -0.075]],
            ([0.5, -0.967, -0.075], [0.0, -0.967, -0.075]),
            0.072,
            (1.0, 1.0, 0.5, 0.1),
            vec![1, 2, 3, 4],
        )
    }

    /// Identity, column-major.
    fn ident() -> [f32; 16] {
        [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]
    }

    fn cloth() -> FlagCloth {
        FlagCloth::new(parts())
    }

    #[test]
    fn the_cloth_takes_its_shape_from_the_arrays_it_is_given() {
        let c = cloth();
        assert_eq!(c.particles(), 4);
        assert_eq!(c.triangles, vec![[0, 1, 2], [1, 3, 2]]);
        assert_eq!(c.uv, vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]]);
        assert_eq!(c.radius, 0.072);
        assert_eq!(c.mass, 1.0);
        assert_eq!(c.drag_coefficient, 0.5);
        assert_eq!(c.gravity_coefficient, 0.1);
        assert_eq!(c.file_index, vec![1, 2, 3, 4]);
        // The two `auto_` ropes are the pins, and they hold particles 0 and 1.
        assert_eq!(c.pinned().collect::<Vec<_>>(), vec![0, 1]);
        assert!(c.links.iter().filter(|l| l.pin.is_some()).all(|l| l.rest == 0.0), "a pin with a non-zero rest length");
        // The three internal ropes are the grid's edges, at their authored lengths.
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

    #[test]
    fn the_pinned_column_never_moves_off_the_mast() {
        let mut c = cloth();
        // A bone matrix that lifts the mast 1.5 m.
        let mut bone = ident();
        bone[13] = 1.5;
        c.step(1.0 / 60.0, [0.0; 3], &bone);
        let p = c.positions().to_vec();
        // Mast particle 1 is at (0, -0.967, -0.075) in the file frame, so lifting the bone 1.5 m
        // puts the pinned cloth particle 0 at (0, 0.533, -0.075).
        assert!(p[0][0].abs() < 1e-5, "pinned 0 at {p:?}");
        assert!((p[0][1] - 0.533).abs() < 1e-5, "pinned 0 at {p:?}");
        assert!((p[0][2] + 0.075).abs() < 1e-5, "pinned 0 at {p:?}");
        assert!((p[1][1] - 0.533).abs() < 1e-5, "pinned 1 at {p:?}");
        assert!((p[1][0] - 0.25).abs() < 1e-5, "pinned 1 at {p:?}");
        // The free particles fell.
        assert!(p[2][1] < 0.533 - 1e-4, "free particle did not fall: {p:?}");
    }

    #[test]
    fn gravity_pulls_the_free_edge_down_and_the_ropes_stay_near_their_rest_length() {
        let mut c = cloth();
        let bone = ident();
        for _ in 0..120 {
            c.step(1.0 / 60.0, [0.0; 3], &bone);
        }
        // Six relaxation passes do not reach the rest length exactly; it must be close and stable.
        for l in c.links.iter().filter(|l| l.pin.is_none()) {
            let len = dist(c.positions()[l.a], c.positions()[l.b.expect("b")]);
            assert!((len - l.rest).abs() < 0.02, "rope {l:?} stretched to {len:.4} from {:.4}", l.rest);
        }
        // And the free edge is well below the pinned edge.
        assert!(c.positions()[2][1] < -0.967 - 0.2, "no droop: {:?}", c.positions()[2]);
        assert!(c.positions()[3][1] < -0.967 - 0.2, "no droop: {:?}", c.positions()[3]);
    }

    #[test]
    fn wind_pushes_the_free_edge_out_and_the_cloth_never_passes_through_the_mast() {
        let mut c = cloth();
        let bone = ident();
        for _ in 0..240 {
            c.step(1.0 / 60.0, [0.0, 0.0, 9.0], &bone);
        }
        // The mast runs along x at z = -0.075 with radius 0.072; the free particles stay outside it.
        let pinned: Vec<usize> = c.pinned().collect();
        for (i, p) in c.positions().iter().enumerate() {
            if pinned.contains(&i) {
                continue;
            }
            assert!((p[2] + 0.075).abs() >= 0.072 - 1e-4, "particle {i} at z {} is inside the mast", p[2]);
        }
        // With a 9 m/s wind off the pole (+z) the sail flies out: the free edge stays out at the mast
        // and drops only a little (drag 4.5 m/s² against gravity 0.98 gives a ~12 degree droop).
        assert!(c.positions()[2][2] > 0.3, "the wind did not blow the free edge out: {:?}", c.positions()[2]);
        assert!(c.positions()[2][1] > -0.967 - 0.3, "the free edge is hanging: {:?}", c.positions()[2]);
        // And with the same wind the other way round it blows to the other side of the pole.
        let mut c2 = cloth();
        for _ in 0..240 {
            c2.step(1.0 / 60.0, [0.0, 0.0, -9.0], &bone);
        }
        assert!(c2.positions()[2][2] < -0.1, "the wind did not blow the free edge round: {:?}", c2.positions()[2]);
    }

    #[test]
    fn no_wind_means_the_sail_hangs_and_reset_restores_the_authored_shape() {
        let mut c = cloth();
        let bone = ident();
        for _ in 0..120 {
            c.step(1.0 / 60.0, [0.0; 3], &bone);
        }
        assert!(c.positions()[2][1] < -0.967 - 0.2);
        c.reset(&bone);
        assert_eq!(c.positions()[2], [0.0, -0.967, 0.425]);
        assert_eq!(c.positions()[0], [0.0, -0.967, -0.075]);
    }

    #[test]
    fn the_step_is_deterministic() {
        let bone = ident();
        let run = || {
            let mut c = cloth();
            for _ in 0..60 {
                c.step(1.0 / 60.0, [1.0, 0.0, 4.0], &bone);
            }
            c.positions().to_vec()
        };
        assert_eq!(run(), run());
    }

    /// This crate's `transform_point` against the reader crate's own `ntw_formats::anim` one, bit
    /// for bit. `ntw_formats` is a **dev**-dependency, so the unit test can call it while the
    /// crate itself stays std only. (Round 9's version of this test compared the function with a
    /// copy of its own body; replaced in review 0-D.)
    #[test]
    fn the_transform_is_the_same_one_the_reader_crates_transform_by() {
        let bone = ntw_formats::cloth::bone3_at_ground();
        for m in [bone, ntw_formats::cloth::mirror_z(bone), ntw_formats::cloth::verlet_frame(&bone)] {
            for p in [[0.0, -0.967, -0.075], [1.027, -0.967, 1.572], [-3.5, 12.25, 0.125]] {
                assert_eq!(transform_point(&m, p), ntw_formats::anim::transform_point(&m, p), "{p:?}");
            }
        }
    }

    /// The seam, checked across the crates: the reader's `parts()` **is** this crate's
    /// [`ClothParts`] (this would not compile otherwise), and the toy file it decodes is exactly
    /// the hand-written [`parts`] the tests above use.
    #[test]
    fn the_readers_parts_are_the_solves_parts() {
        const TOY: &str = "version 1.0\n\
            rigid mast_rigid_euro_flagpoleA_01\n{\n\tmass\t100\n\tradius\t0.072\n\
            \tparticle top\tp(0.500, -0.967, -0.075)\n\tparticle bottom\tp(0.000, -0.967, -0.075)\n\
            \tparticle 1\tp(0.000, -0.967, -0.075)\n\tparticle 2\tp(0.250, -0.967, -0.075)\n}\n\
            verlet_item sail_rigid_euro_flagpoleA_01\n{\n\tmass\t1.0\n\tsurface_area\t1.0\n\ttexture\tflag_\n\
            \tdrag_coefficient\t0.5\n\tgravity_coefficient\t0.1\n\
            \tparticle 1 \tp(0.000, -0.967, -0.075)\tt(0.0, 0.0)\n\tparticle 2 \tp(0.250, -0.967, -0.075)\tt(0.0, 1.0)\n\
            \tparticle 3 \tp(0.000, -0.967, 0.425)\tt(1.0, 0.0)\n\tparticle 4 \tp(0.250, -0.967, 0.425)\tt(1.0, 1.0)\n\
            \ttriangle 1 1 2 3\n\ttriangle 2 2 4 3\n\
            \trope srope1 1 3 invisible\n\trope srope2 2 4 invisible\n\trope srope3 3 4 invisible\n\
            \trope auto_1\t1\tmast_rigid_euro_flagpoleA_01.1\n\trope auto_2\t2\tmast_rigid_euro_flagpoleA_01.2\n}\n";
        let read: ClothParts = ntw_formats::cloth::standard_bearer_flag(TOY).expect("toy cloth").parts();
        assert_eq!(read, parts());
        // And the two build the same cloth, step for step.
        let (mut a, mut b) = (FlagCloth::new(read), cloth());
        let bone = ident();
        for _ in 0..30 {
            a.step(1.0 / 60.0, [0.0, 0.0, 4.0], &bone);
            b.step(1.0 / 60.0, [0.0, 0.0, 4.0], &bone);
        }
        assert_eq!(a.positions(), b.positions());
    }

    /// Bad inputs never poison the cloth: a NaN or negative step does nothing, a NaN wind is still
    /// air, and a malformed set of arrays drops the links it cannot honour instead of panicking.
    #[test]
    fn bad_inputs_neither_panic_nor_poison_the_cloth() {
        let bone = ident();
        let mut c = cloth();
        c.step(1.0 / 60.0, [0.0; 3], &bone);
        let before = c.positions().to_vec();
        for dt in [f32::NAN, -1.0, 0.0, f32::NEG_INFINITY] {
            c.step(dt, [0.0; 3], &bone);
            assert_eq!(c.positions(), &before[..], "dt {dt} moved the cloth");
        }
        // An infinite step is clamped to `MAX_STEP`, like any long frame.
        c.step(f32::INFINITY, [0.0; 3], &bone);
        assert!(c.positions().iter().flatten().all(|v| v.is_finite()));
        let mut still = cloth();
        let mut nan_wind = cloth();
        for _ in 0..30 {
            still.step(1.0 / 60.0, [0.0; 3], &bone);
            nan_wind.step(1.0 / 60.0, [f32::NAN, 0.0, f32::INFINITY], &bone);
        }
        assert_eq!(nan_wind.positions(), still.positions(), "a non-finite wind is still air");
        // A zero mass would make the explicit drag diverge; it is bounded.
        let (rest, uv, tris, links, mast, rod, radius, (_, area, drag, grav), idx) = parts();
        let mut light = FlagCloth::new((rest, uv, tris, links, mast, rod, radius, (0.0, area, drag, grav), idx));
        for _ in 0..240 {
            light.step(1.0 / 20.0, [0.0, 0.0, 9.0], &bone);
        }
        assert!(light.positions().iter().flatten().all(|v| v.is_finite() && v.abs() < 10.0), "{:?}", light.positions());
        // Out-of-range indices are dropped, not followed.
        let (rest, uv, mut tris, mut links, mast, rod, radius, k, idx) = parts();
        links.push((0, Some(99), 0.5, None));
        links.push((7, None, 0.0, Some(0)));
        links.push((1, None, 0.0, Some(5)));
        tris.push([0, 1, 42]);
        let mut bad = FlagCloth::new((rest, uv[..2].to_vec(), tris, links, mast, rod, radius, k, idx));
        assert_eq!(bad.links.len(), 5, "only the file's own five links survive");
        assert_eq!(bad.triangles.len(), 2);
        assert_eq!(bad.uv.len(), 4, "one texture coordinate per particle");
        bad.step(1.0 / 60.0, [0.0; 3], &bone);
        let _ = ClothMesh::layout(&bad);
    }

    #[test]
    fn the_mesh_layout_matches_the_arrays() {
        let c = cloth();
        let m = ClothMesh::layout(&c);
        assert_eq!(m.uv.len(), 4);
        assert_eq!(m.indices, vec![0, 1, 2, 1, 3, 2]);
        assert!(m.positions.is_empty() && m.normals.is_empty());
        let mut m = m;
        m.update(&c);
        assert_eq!(m.positions.len(), 4);
        assert_eq!(m.normals.len(), 4);
        // The sail is flat in y (every particle at the same y), so every normal is +/-y.
        for n in &m.normals {
            assert!(n[0].abs() < 1e-5 && n[2].abs() < 1e-5, "normal {n:?} is not along y");
            assert!(n[1].abs() > 0.9, "normal {n:?} is not along y");
        }
    }
}
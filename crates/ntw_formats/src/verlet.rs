//! Verlet cloth: `RigidModels\VerletItems\*.logic`, the flag on a standard bearer's pole.
//!
//! Read-only. The original loads `standard_bearer_flag.logic` for a unit with a standard bearer
//! (`FUN_007101D0` -> the "Flag Display" of the battle display set-up -> `FUN_00732190`; INFERRED:
//! an exe reading whose decompile is not kept in the repo or in the sandbox evidence,
//! `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §3 and its review-0-D evidence audit).
//!
//! The file is plain text. Its shape (CONFIRMED by reading every shipped `.logic` file to the end):
//! ```text
//! version 1.0
//! rigid <name> { mass <f> radius <f> particle <id> p(x, y, z) ... }
//! verlet_item <name> {
//!     mass <f> surface_area <f> texture <prefix> drag_coefficient <f> gravity_coefficient <f>
//!     particle <i> p(x, y, z) t(u, v)
//!     triangle <i> <a> <b> <c>
//!     rope <name> <a> <b> [invisible]
//!     rope auto_<n> <particle> <rigid>.<particle>      // the cloth's pinned edge
//! }
//! ```
//! A `rigid` block is a **collision proxy**, not the drawn mesh: its particles are the points the
//! verlet solver pins the cloth to (CONFIRMED: in `standard_bearer_flag.logic` the six
//! `auto_<n>` ropes bind cloth particles 1, 2, 4, 5, 9, 10 -- the `t(u, 0.0)` leading edge -- to
//! the rigid's particles 1..6, and the rigid's `top`/`bottom` particles mark the mast's ends).
//!
//! ## Coordinates and the attachment frame
//!
//! Everything in the file is in the **verlet item's own frame**, not the bearer's bone frame.
//! CONFIRMED by comparing the two shipped files that describe the same flag:
//! - `rigid_equip_euro_flagpole01` in `unitmodels\euro_equipment.variant_weighted_mesh` (the drawn
//!   pole, 140 vertices) has its bounding box at x `-1.020 .. 3.379`, y `+-0.038`, z `-0.046 ..
//!   0.040`: a 4.40 m pole along **+x**, its origin 1.02 m up from the butt;
//! - `mast_rigid_euro_flagpoleA_01` in the `.logic` file has particles at x `1.021 .. 2.067`,
//!   y `~ -0.967`, z `~ -0.09`: the **same axis** (x) but offset by `-0.967` in y.
//!
//! So the cloth frame and the pole's frame share their axes and differ by a translation. The six
//! numbered mast particles (and the six cloth particles welded to them) sit at exactly
//! `y = -0.967, z = -0.075` (CONFIRMED, data); the `top`/`bottom` markers are the rod's slightly
//! tilted ends, which is why a bounding-box reading gives z `~ -0.087`. The mast lies *along* the
//! pole (x `1.027 .. 2.183`, inside the pole's `-1.02 .. 3.38` span) and the sail hangs off it in
//! **+z** (the `t(u, v)` grid runs `u` 0.003..0.998 along x and `v` 0.0..0.999 along z, 42
//! particles, 60 triangles).
//!
//! INFERRED, not read out of the exe: that the exe positions the verlet item so its mast lies on
//! the drawn pole, i.e. in the pole bone's frame shifted by `-(0, -0.967, -0.075)`
//! ([`crate::cloth::FRAME_OFFSET`], [`crate::cloth::verlet_frame`]). The data alone cannot rule
//! out "item origin = bone origin, sail 0.97 m beside the pole"; that reading is simply not a
//! flag. The standard bearer's variant lists the pole under `equipment_personal` (CONFIRMED from
//! the `.unit_variant` and `warscape_equipment_themes.euro_standard_bearer`).
//!
//! ## The flag's texture
//!
//! `texture flag_` is a **prefix**. The exe completes it with the flag key and looks the result up
//! in `RigidModels\Flags\Textures\flags.tai`, falling back to `flag_default.tga` (INFERRED from an
//! exe reading at `0x01227BD0` whose decompile is not kept; the `.tai` format is CONFIRMED from the
//! shipped file and the key rule is INFERRED -- see §3).

use std::collections::BTreeMap;
use std::fmt;

/// One point of a rigid body or a cloth particle, in the file's frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VerletPoint {
    /// Position in metres.
    pub p: [f32; 3],
    /// Texture coordinate, cloth particles only (`None` for rigid particles).
    pub t: Option<[f32; 2]>,
}

/// A `rigid` block: a collision proxy with named particles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VerletRigid {
    pub name: String,
    pub mass: f32,
    pub radius: f32,
    /// Particle name -> position, in file order. The two names `top` and `bottom` are the mast's
    /// ends (present in every shipped rigid block).
    pub particles: BTreeMap<String, VerletPoint>,
}

impl VerletRigid {
    /// The `top` and `bottom` particles, when the block has them: the mast's two ends.
    pub fn ends(&self) -> Option<([f32; 3], [f32; 3])> {
        Some((self.particles.get("top")?.p, self.particles.get("bottom")?.p))
    }
}

/// A `verlet_item` block: the cloth.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VerletItem {
    pub name: String,
    pub mass: f32,
    pub surface_area: f32,
    /// The `texture` **prefix** (e.g. `flag_`); the exe appends the flag key.
    pub texture_prefix: String,
    pub drag_coefficient: f32,
    pub gravity_coefficient: f32,
    /// Cloth particles, 1-based index as written in the file.
    pub particles: BTreeMap<u32, VerletPoint>,
    /// `triangle <i> <a> <b> <c>`, 1-based particle indices, in file order.
    pub triangles: Vec<[u32; 3]>,
    /// `rope <name> <a> <b> [invisible]`; `name` is `srope<n>` or `auto_<n>`.
    pub ropes: Vec<VerletRope>,
}

impl VerletItem {
    /// The cloth's **pinned edge**: the `auto_<n>` ropes name a rigid and one of its particles, so
    /// these cloth particles do not move. CONFIRMED: every shipped `.logic` file pins its cloth's
    /// `t(u, 0)` column this way.
    pub fn pinned(&self) -> impl Iterator<Item = (&u32, &str)> + '_ {
        self.ropes.iter().filter_map(|r| r.rigid.as_ref().map(|rig| (&r.a, rig.as_str())))
    }
}

/// One `rope` constraint. `rigid` is `Some("mast.3")` for an `auto_<n>` rope, which binds cloth
/// particle `a` to a particle of a rigid body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerletRope {
    pub name: String,
    /// Cloth particle (1-based).
    pub a: u32,
    /// Second cloth particle, or `None` when `rigid` is set.
    pub b: Option<u32>,
    /// `"<rigid name>.<particle>"` for an `auto_<n>` rope.
    pub rigid: Option<String>,
    /// The rope's own name ends with `invisible`, or the `invisible` keyword follows it.
    pub invisible: bool,
}

/// A whole `.logic` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VerletLogic {
    pub version: String,
    /// `rigid` blocks, in file order.
    pub rigids: Vec<VerletRigid>,
    /// `verlet_item` blocks, in file order.
    pub items: Vec<VerletItem>,
}

impl VerletLogic {
    /// The rigid block named `name`.
    pub fn rigid(&self, name: &str) -> Option<&VerletRigid> {
        self.rigids.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }
    /// The verlet item named `name`.
    pub fn item(&self, name: &str) -> Option<&VerletItem> {
        self.items.iter().find(|i| i.name.eq_ignore_ascii_case(name))
    }
}

/// Why a `.logic` file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerletError {
    /// A number did not parse.
    Number { line: usize, text: String },
    /// A block was not closed.
    Unclosed { line: usize, block: String },
    /// A `triangle` / `rope` named a particle the file never defined.
    BadParticle { line: usize, text: String },
    /// A block of a kind this reader does not know.
    UnknownBlock { line: usize, name: String },
}

impl fmt::Display for VerletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number { line, text } => write!(f, "line {line}: bad number in {text:?}"),
            Self::Unclosed { line, block } => write!(f, "line {line}: {block} block never closed"),
            Self::BadParticle { line, text } => write!(f, "line {line}: unknown particle in {text:?}"),
            Self::UnknownBlock { line, name } => write!(f, "line {line}: unknown block {name:?}"),
        }
    }
}

impl std::error::Error for VerletError {}

fn fields(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

fn number(t: &str, line: usize) -> Result<f32, VerletError> {
    t.parse().map_err(|_| VerletError::Number { line, text: t.to_owned() })
}

/// The text between `open` and its matching `)`, so a line can hold several groups.
fn group<'a>(t: &'a str, open: &str, line: usize) -> Result<&'a str, VerletError> {
    let inner = t
        .strip_prefix(open)
        .and_then(|s| s.split_once(')').map(|(a, _)| a))
        .ok_or_else(|| VerletError::Number { line, text: t.to_owned() })?;
    Ok(inner)
}

/// Exactly `N` comma-separated numbers inside `open ... )`. A short or long group is an error, not
/// a silent zero or an out-of-bounds write. Text after the `)` is ignored.
fn numbers<const N: usize>(t: &str, open: &str, line: usize) -> Result<[f32; N], VerletError> {
    let mut parts = group(t, open, line)?.split(',');
    let mut out = [0.0; N];
    for v in &mut out {
        let part = parts.next().ok_or_else(|| VerletError::Number { line, text: t.to_owned() })?;
        *v = number(part.trim(), line)?;
    }
    if parts.next().is_some() {
        return Err(VerletError::Number { line, text: t.to_owned() });
    }
    Ok(out)
}

/// `"p(1.5, -2.0, 0.25)"` -> `[1.5, -2.0, 0.25]`.
fn point(t: &str, line: usize) -> Result<[f32; 3], VerletError> {
    numbers::<3>(t, "p(", line)
}

/// `"t(0.003, 0.2)"` -> `[0.003, 0.2]`.
fn texcoord(t: &str, line: usize) -> Result<[f32; 2], VerletError> {
    numbers::<2>(t, "t(", line)
}

impl VerletLogic {
    /// Parse a `.logic` file.
    pub fn read(text: &str) -> Result<Self, VerletError> {
        let mut out = Self::default();
        #[derive(PartialEq)]
        enum Cur {
            None,
            Rigid(VerletRigid),
            Item(VerletItem),
        }
        let mut cur = Cur::None;
        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            let l = raw.trim();
            if l.is_empty() || l.starts_with("//") || l.starts_with('#') {
                continue;
            }
            let f = fields(l);
            // A keyword that reads a second field must have one: a bare `particle` or `rope` line
            // is an error, not an index panic.
            let second = || -> Result<&str, VerletError> {
                f.get(1).copied().ok_or_else(|| VerletError::BadParticle { line, text: l.to_owned() })
            };
            // A block opens as `rigid <name>` and closes on a line that is just `}`; the shipped
            // files put the `{` on the following line (CONFIRMED on all four).
            match f[0] {
                "version" => out.version = f.get(1).copied().unwrap_or_default().to_owned(),
                "rigid" if f.len() >= 2 && f[1] != "{" => {
                    cur = Cur::Rigid(VerletRigid { name: f[1].to_owned(), ..Default::default() });
                }
                "verlet_item" if f.len() >= 2 && f[1] != "{" => {
                    cur = Cur::Item(VerletItem { name: f[1].to_owned(), ..Default::default() });
                }
                "{" => {}
                "}" => match std::mem::replace(&mut cur, Cur::None) {
                    Cur::Rigid(r) => out.rigids.push(r),
                    Cur::Item(i) => out.items.push(i),
                    Cur::None => return Err(VerletError::Unclosed { line, block: "}".into() }),
                },
                _ => match &mut cur {
                    Cur::None => return Err(VerletError::UnknownBlock { line, name: f[0].into() }),
                    Cur::Rigid(r) => {
                        let rest = &l[f[0].len()..];
                        match f[0] {
                            "mass" => r.mass = number(first_value(rest, line)?, line)?,
                            "radius" => r.radius = number(first_value(rest, line)?, line)?,
                            "particle" => {
                                // `particle <name> p(x, y, z)` -- the name may itself contain
                                // `p(`-free text, so find the coordinate group by its prefix.
                                let at = rest
                                    .find("p(")
                                    .ok_or_else(|| VerletError::Number { line, text: l.to_owned() })?;
                                let p = point(&rest[at..], line)?;
                                r.particles.insert(second()?.to_owned(), VerletPoint { p, t: None });
                            }
                            _ => return Err(VerletError::UnknownBlock { line, name: f[0].into() }),
                        }
                    }
                    Cur::Item(i) => {
                        let rest = &l[f[0].len()..];
                        match f[0] {
                            "mass" => i.mass = number(first_value(rest, line)?, line)?,
                            "surface_area" => i.surface_area = number(first_value(rest, line)?, line)?,
                            "texture" => i.texture_prefix = first_value(rest, line)?.to_owned(),
                            "drag_coefficient" => {
                                i.drag_coefficient = number(first_value(rest, line)?, line)?
                            }
                            "gravity_coefficient" => {
                                i.gravity_coefficient = number(first_value(rest, line)?, line)?
                            }
                            "particle" => {
                                // `particle <i> p(x, y, z) t(u, v)`
                                let idx: u32 = second()?
                                    .parse()
                                    .map_err(|_| VerletError::Number { line, text: l.to_owned() })?;
                                let at = rest
                                    .find("p(")
                                    .ok_or_else(|| VerletError::Number { line, text: l.to_owned() })?;
                                let p = point(&rest[at..], line)?;
                                let t = rest.find("t(").map(|k| texcoord(&rest[k..], line)).transpose()?;
                                i.particles.insert(idx, VerletPoint { p, t });
                            }
                            "triangle" => {
                                // `triangle <own index> <a> <b> <c>`
                                let p = |k: usize| -> Result<u32, VerletError> {
                                    f.get(k)
                                        .and_then(|w| w.parse().ok())
                                        .ok_or_else(|| VerletError::BadParticle { line, text: l.into() })
                                };
                                i.triangles.push([p(2)?, p(3)?, p(4)?]);
                            }
                            "rope" => {
                                // `rope <name> <a> <b> [invisible]`, or
                                // `rope auto_<n> <particle> <rigid>.<particle>`
                                let name = second()?.to_owned();
                                let invisible = name.ends_with("invisible") || f.last() == Some(&"invisible");
                                let num = |k: usize| -> Result<u32, VerletError> {
                                    f.get(k)
                                        .and_then(|w| w.parse().ok())
                                        .ok_or_else(|| VerletError::BadParticle { line, text: l.into() })
                                };
                                match f.iter().rev().find(|w| w.contains('.')) {
                                    Some(r) => i.ropes.push(VerletRope {
                                        name,
                                        a: num(2)?,
                                        b: None,
                                        rigid: Some((*r).to_owned()),
                                        invisible,
                                    }),
                                    None => i.ropes.push(VerletRope {
                                        name,
                                        a: num(2)?,
                                        b: Some(num(3)?),
                                        rigid: None,
                                        invisible,
                                    }),
                                }
                            }
                            _ => return Err(VerletError::UnknownBlock { line, name: f[0].into() }),
                        }
                    }
                },
            }
        }
        if cur != Cur::None {
            return Err(VerletError::Unclosed { line: text.lines().count(), block: "body".into() });
        }
        // Every index a triangle or rope names must exist.
        for item in &out.items {
            let known = |n: u32| item.particles.contains_key(&n);
            for t in &item.triangles {
                if !t.iter().all(|n| known(*n)) {
                    return Err(VerletError::BadParticle { line: 0, text: format!("triangle {t:?}") });
                }
            }
            for r in &item.ropes {
                if !known(r.a) || r.b.is_some_and(|b| !known(b)) {
                    return Err(VerletError::BadParticle { line: 0, text: r.name.clone() });
                }
            }
        }
        Ok(out)
    }
}

/// The value after the keyword: everything up to whitespace, so `mass 100` and `mass\t100` both
/// give `100`, and `particle top p(...)` gives `p(...)`.
fn first_value(rest: &str, line: usize) -> Result<&str, VerletError> {
    rest.split_whitespace()
        .next()
        .ok_or_else(|| VerletError::Number { line, text: rest.trim().to_owned() })
}

/// The bounding box of a set of points, as `(min, max)`.
pub fn bounds(points: impl Iterator<Item = [f32; 3]>) -> Option<([f32; 3], [f32; 3])> {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    let mut any = false;
    for p in points {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
        any = true;
    }
    any.then_some((lo, hi))
}

impl VerletItem {
    /// Bounding box of the cloth particles.
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        bounds(self.particles.values().map(|v| v.p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAG: &str = r#"version 1.0

rigid mast_test
{
	mass	100
	radius	0.072

	particle top	p(2.067, -0.986, -0.093)
	particle bottom	p(1.021, -0.955, -0.1)

}

verlet_item sail_test
{
	mass	1.0
	surface_area	1.0
	texture	flag_
	drag_coefficient	0.5
	gravity_coefficient	0.1

	particle 1 	p(1.258, -0.967, -0.075)	t(0.003, 0.2)
	particle 2 	p(1.027, -0.967, -0.075)	t(0.003, 0.0)
	particle 3 	p(1.027, -0.967, 0.074)	t(0.093, 0.0)
	particle 4 	p(1.721, -0.967, -0.075)	t(0.003, 0.599)
	triangle 1 1 2 3
	triangle 2 4 1 2
	rope srope1 1 2 invisible
	rope srope2 1 3 invisible

	rope auto_1	1	mast_test.1
	rope auto_2	2	mast_test.2

}
"#;

    #[test]
    fn the_flag_file_reads_completely() {
        let l = VerletLogic::read(FLAG).expect("parse");
        assert_eq!(l.version, "1.0");
        let r = l.rigid("mast_test").expect("rigid");
        assert_eq!(r.mass, 100.0);
        assert_eq!(r.radius, 0.072);
        assert_eq!(r.particles.len(), 2);
        let (top, bottom) = r.ends().expect("ends");
        assert_eq!(top, [2.067, -0.986, -0.093]);
        assert_eq!(bottom, [1.021, -0.955, -0.1]);
        let i = l.item("sail_test").expect("item");
        assert_eq!(i.texture_prefix, "flag_");
        assert_eq!(i.drag_coefficient, 0.5);
        assert_eq!(i.gravity_coefficient, 0.1);
        assert_eq!(i.particles.len(), 4);
        assert_eq!(i.triangles, vec![[1, 2, 3], [4, 1, 2]]);
        assert_eq!(i.particles[&1].t, Some([0.003, 0.2]));
        // The pinned edge is the two `auto_` ropes, not the internal ones. Only the internal
        // (`srope`) ropes are marked invisible (CONFIRMED on the shipped file).
        assert_eq!(i.pinned().count(), 2);
        assert_eq!(i.ropes.iter().filter(|r| r.invisible).count(), 2);
        assert!(i.pinned().all(|(n, _)| i.ropes.iter().any(|r| &r.a == n && !r.invisible)));
    }

    #[test]
    fn a_triangle_naming_a_missing_particle_is_an_error() {
        let bad = FLAG.replace("triangle 2 4 1 2", "triangle 2 4 1 99");
        assert!(matches!(VerletLogic::read(&bad), Err(VerletError::BadParticle { .. })));
    }

    /// Malformed lines are errors, never panics: a bare keyword, a coordinate group with too many
    /// or too few numbers, a texture coordinate with three.
    #[test]
    fn malformed_lines_are_errors_not_panics() {
        for (from, to) in [
            ("particle 1 \tp(1.258, -0.967, -0.075)", "particle"),
            ("particle 1 \tp(1.258, -0.967, -0.075)", "particle 1 p(1.258, -0.967, -0.075, 4.0)"),
            ("particle 1 \tp(1.258, -0.967, -0.075)", "particle 1 p(1.258, -0.967)"),
            ("t(0.003, 0.2)", "t(0.003, 0.2, 0.5)"),
            ("rope srope1 1 2 invisible", "rope"),
            ("particle top\tp(2.067, -0.986, -0.093)", "particle"),
            ("particle top\tp(2.067, -0.986, -0.093)", "particle top p(2.067, -0.986, -0.093, 1.0)"),
        ] {
            assert!(FLAG.contains(from), "fixture lost {from:?}");
            let bad = FLAG.replacen(from, to, 1);
            assert!(VerletLogic::read(&bad).is_err(), "{to:?} parsed");
        }
    }

    #[test]
    fn bounds_cover_the_cloth() {
        let l = VerletLogic::read(FLAG).unwrap();
        let (lo, hi) = l.item("sail_test").unwrap().bounds().unwrap();
        assert_eq!(lo[0], 1.027);
        assert_eq!(hi[0], 1.721);
        assert_eq!(lo[1], -0.967);
        assert_eq!(hi[1], -0.967);
    }
}
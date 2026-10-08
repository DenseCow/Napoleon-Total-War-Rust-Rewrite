//! Our recreation of SpeedTreeRT 4's geometry generator (`CSpeedTreeRT::Compute` →
//! `CTreeEngine::Compute` `FUN_012c9a90` → recursive `CBranch::Compute` `FUN_012d45a0`).
//!
//! Everything here follows the exe's arithmetic step by step (single precision like its SSE2
//! code, double precision where it keeps x87 results), including the order in which random
//! numbers are drawn, so each tree grows the same shape as in the original. Spec and evidence:
//! `analysis/speedtree/SPEEDTREE.md` §5–7.
//!
//! Coordinates: SpeedTree works Z-up internally (gravity `(0,0,−1)`, branch forward axis
//! `(1,0,0)`); the exe writes vertices as `(−x, z, y)`, i.e. Y-up. [`TreeGeometry`] holds the
//! written (Y-up) values.

use super::params::{LevelParams, TreeParams};
use super::rng::{MsvcRand, Newran};
use std::f32::consts::{PI, TAU};

const DEG2RAD: f32 = 0.017_453_292;
/// `DAT_0146d058`: the branch forward axis (CONFIRMED value).
const FORWARD: [f32; 3] = [1.0, 0.0, 0.0];
/// `DAT_0146d064`: gravity (CONFIRMED value).
const GRAVITY: [f32; 3] = [0.0, 0.0, -1.0];

type M3 = [f32; 9];

/// `1 / max(len, ...)` the exe's way: lengths² ≤ 1.1920929e-5 count as 0.01.
fn inv_len(x: f32, y: f32, z: f32) -> f32 {
    let l2 = y * y + x * x + z * z;
    let l = if l2 <= 1.192_092_9e-5 { 0.01 } else { l2.sqrt() };
    1.0 / l
}

/// The bit-trick square root SpeedTreeRT uses for lengths (`(bits >> 1) + 0x1fc00000`).
pub fn approx_sqrt(v: f32) -> f32 {
    f32::from_bits(((v.to_bits() as i32 >> 1) + 0x1fc0_0000) as u32)
}

/// Writes a SpeedTree (Z-up) vector the way the exe stores vertices: `(−x, z, y)`.
fn out(v: [f32; 3]) -> [f32; 3] {
    [-v[0], v[2], v[1]]
}

/// Rotates `m` about `axis` by `deg` degrees (`FUN_012d9720`).
fn rotate_axis(m: &mut M3, deg: f32, ax: f32, ay: f32, az: f32) {
    let (s, c) = (deg * DEG2RAD).sin_cos();
    let omc = 1.0 - c;
    let f11 = ax * omc;
    let f6 = ax * f11 + c;
    let f16 = az * s + ay * f11;
    let f12 = az * f11 - ay * s;
    let f13 = ay * f11 - az * s;
    let f14 = ay * ay * omc + c;
    let f15a = az * ay * omc;
    let f7 = ax * s + f15a;
    let f15 = f15a - ax * s;
    let f8 = ay * s + az * f11;
    let f9 = az * omc * az + c;
    let o = *m;
    m[0] = o[3] * f16 + o[0] * f6 + o[6] * f12;
    m[1] = o[1] * f6 + o[4] * f16 + o[7] * f12;
    m[2] = o[2] * f6 + o[5] * f16 + o[8] * f12;
    m[3] = o[3] * f14 + o[0] * f13 + o[6] * f7;
    m[4] = o[4] * f14 + o[1] * f13 + o[7] * f7;
    m[5] = o[5] * f14 + o[2] * f13 + o[8] * f7;
    m[6] = o[3] * f15 + o[0] * f8 + o[6] * f9;
    m[7] = o[4] * f15 + o[1] * f8 + o[7] * f9;
    m[8] = o[5] * f15 + o[2] * f8 + o[8] * f9;
}

/// `FUN_012d9c00`: two rotations (degrees `a`, then `b`).
fn rotate_ab(m: &mut M3, a: f32, b: f32) {
    let (sa, ca) = (a * DEG2RAD).sin_cos();
    let (sb, cb) = (b * DEG2RAD).sin_cos();
    let f12 = sb * ca;
    let f13 = cb * ca;
    let f14 = sb * sa;
    let f7 = cb * sa;
    let o = *m;
    m[0] = o[0] * f13 + o[3] * f12 + o[6] * -sa;
    m[1] = o[1] * f13 + o[4] * f12 + o[7] * -sa;
    m[2] = o[2] * f13 + o[5] * f12 + o[8] * -sa;
    m[3] = o[3] * cb + o[0] * -sb;
    m[4] = o[4] * cb + o[1] * -sb;
    m[5] = o[5] * cb + o[2] * -sb;
    m[6] = o[0] * f7 + o[3] * f14 + o[6] * ca;
    m[7] = o[1] * f7 + o[4] * f14 + o[7] * ca;
    m[8] = o[2] * f7 + o[5] * f14 + o[8] * ca;
}

/// `FUN_012d99f0`: a fresh axis-angle matrix (degrees).
fn axis_angle(deg: f32, x: f32, y: f32, z: f32) -> M3 {
    let (s, c) = (deg * DEG2RAD).sin_cos();
    let omc = 1.0 - c;
    let f5 = x * omc;
    let f4 = y * omc * z;
    [
        f5 * x + c,
        z * s + f5 * y,
        f5 * z - y * s,
        f5 * y - z * s,
        y * omc * y + c,
        x * s + f4,
        y * s + f5 * z,
        f4 - x * s,
        z * omc * z + c,
    ]
}

/// `FUN_012d3b50`: `a · b`.
fn mul(a: &M3, b: &M3) -> M3 {
    [
        a[0] * b[0] + a[1] * b[3] + a[2] * b[6],
        b[1] * a[0] + b[4] * a[1] + b[7] * a[2],
        b[2] * a[0] + b[5] * a[1] + b[8] * a[2],
        a[3] * b[0] + a[4] * b[3] + a[5] * b[6],
        a[3] * b[1] + a[4] * b[4] + a[5] * b[7],
        a[3] * b[2] + a[4] * b[5] + a[5] * b[8],
        a[6] * b[0] + a[7] * b[3] + a[8] * b[6],
        a[6] * b[1] + a[7] * b[4] + a[8] * b[7],
        a[6] * b[2] + a[7] * b[5] + a[8] * b[8],
    ]
}

/// The direction a frame points along (`FORWARD` through the frame, the exe's expression).
fn frame_dir(m: &M3) -> [f32; 3] {
    let d = FORWARD;
    [
        m[3] * d[1] + m[0] * d[0] + m[6] * d[2],
        m[1] * d[0] + m[4] * d[1] + m[7] * d[2],
        m[2] * d[0] + m[5] * d[1] + m[8] * d[2],
    ]
}

/// One node along a branch (the exe's 0x54-byte record).
#[derive(Debug, Clone, Copy, Default)]
pub struct BranchNode {
    pub dir: [f32; 3],
    pub pos: [f32; 3],
    pub radius: f32,
    pub frame: M3,
    /// Length along the branch to this node.
    pub length: f32,
    /// Wind weights.
    pub wind: [f32; 2],
    /// Children snapped to this joint.
    pub snapped: i32,
    /// Sides of this node's ring.
    pub ring: i32,
}

/// A generated branch (the exe's `CBranch`, 0x388 bytes).
#[derive(Debug, Clone, Default)]
pub struct Branch {
    pub parent: Option<usize>,
    pub level: i32,
    /// Position on the parent (0..1).
    pub t_parent: f32,
    /// 1 = tube geometry, 0 = frond spine.
    pub mode: i32,
    pub nodes: Vec<BranchNode>,
    /// Sides of the tube.
    pub cross_sections: u16,
    /// First vertex in [`BranchMesh`] (−1: none).
    pub vertex_start: i32,
    /// Flares: angle, width, width exponent, length, length exponent, strength.
    pub flares: Vec<[f32; 6]>,
    /// LOD weight (`FUN_012d7e80`): Σ (r_i + r_{i+1}) · |segment|.
    pub weight: f32,
    /// Weight after the LOD blend (`+0x28`).
    pub lod_weight: f32,
}

/// Branch (bark) vertices as the exe writes them (Y-up `(−x, z, y)`).
#[derive(Debug, Clone, Default)]
pub struct BranchMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
    pub binormals: Vec<[f32; 3]>,
    /// Texture layer 0 (bark diffuse); V as computed (before the exe's optional global flip).
    pub uvs: Vec<[f32; 2]>,
    /// Wind weights (`1 − w`, clamped) and wind matrix groups.
    pub wind_weights: Vec<[f32; 2]>,
    pub wind_groups: Vec<[u8; 2]>,
    /// Triangle-strip indices per LOD (one strip per branch, as the exe stores them).
    pub lod_strips: Vec<Vec<Vec<u32>>>,
}

/// A placed leaf (before the leaf-card geometry).
#[derive(Debug, Clone, Default)]
pub struct PlacedLeaf {
    /// Position, Y-up.
    pub pos: [f32; 3],
    /// Leaf texture index.
    pub texture: usize,
    /// The texture slot as the exe stores it (`2 · texture` or `2 · texture + 1`).
    pub slot: i32,
    /// Normal (Y-up): the leaf direction `D` of the basis.
    pub normal: [f32; 3],
    /// The leaf basis `[R, U, D]` (Y-up), used to orient mesh leaves (`FUN_012d8090`).
    pub basis: [[f32; 3]; 3],
    /// Colour: 4001 + one `U(−4002, 4002)` draw.
    pub color: [f32; 3],
    /// Ambient/dimming value (0..1) and wind values.
    pub dimming: f32,
    pub rock_group: i32,
    pub wind: [f32; 2],
    pub wind_groups: [i32; 2],
    /// Position along the parent.
    pub t: f32,
}

/// A frond spine (`CFrondEngine` record 0x238 bytes).
#[derive(Debug, Clone, Default)]
pub struct FrondSpine {
    pub nodes: Vec<BranchNode>,
    pub wind_groups: [i32; 2],
    pub length: f32,
    pub half_width: f32,
    pub texture: usize,
    pub angle: f32,
    pub radius_scale: f32,
}

/// The output of one tree computation.
#[derive(Debug, Clone, Default)]
pub struct TreeGeometry {
    /// The tree size actually used (2006 ± 2007).
    pub size: f32,
    pub branches: Vec<Branch>,
    pub mesh: BranchMesh,
    pub leaves: Vec<PlacedLeaf>,
    pub fronds: Vec<FrondSpine>,
    /// Frond geometry built from `fronds`.
    pub frond_mesh: super::frond::FrondMesh,
}

struct Gen<'a> {
    t: &'a TreeParams,
    rng: Newran,
    crt: MsvcRand,
    table: Vec<f32>,
    /// `DAT_01843a48`: child probability draws.
    table_counter: usize,
    /// `DAT_01842aa4`: roughness draws.
    rough_counter: usize,
    /// `DAT_01843a4c`: leaf rotation draws.
    leaf_counter: usize,
    /// `DAT_01842a8c`: wind group counter.
    wind_counter: i32,
    nlevels: i32,
    out: TreeGeometry,
    /// Leaf texture slots: normal (`DAT_01843a50`) and blossom (`DAT_01843a78`).
    slots: Vec<i32>,
    blossom_slots: Vec<i32>,
    /// First leaf of the branch whose leaves are being placed (`DAT_01843a68` list).
    leaf_branch_start: usize,
}

impl<'a> Gen<'a> {
    fn level(&self, level: i32, is_root: bool) -> &'a LevelParams {
        if is_root { &self.t.root } else { &self.t.levels[level as usize] }
    }

    /// `FUN_012d94f0` with `FUN_012c7c20`: frond (0), branch (1) or nothing (2).
    fn frond_mode(&self, b: usize, t: f32) -> i32 {
        if !self.t.fronds_enabled {
            return 1;
        }
        let (x, depth, a, c) = self.t.frond_rule;
        let mut tt = t;
        if depth != 0 {
            // The record's own t is not set yet here (CONFIRMED order); walking up uses parents.
            let mut cur = b;
            let mut i = 0;
            while i < depth {
                match self.out.branches[cur].parent {
                    Some(p) => cur = p,
                    None => break,
                }
                i += 1;
            }
            tt = if cur == b { 0.0 } else { self.out.branches[cur].t_parent };
        }
        if x < 0.0 {
            if -x <= tt { c } else { a }
        } else if x <= tt {
            a
        } else {
            c
        }
    }

    /// `FUN_012d7550`: the flares of a new tube branch (C `rand()`).
    fn make_flares(&mut self, l: &LevelParams) -> Vec<[f32; 6]> {
        let f = &l.flares;
        if f.count == 0 {
            return Vec::new();
        }
        let base = self.crt.rand() as f32;
        let n = f.count;
        let mut v = Vec::new();
        for i in 0..n {
            let step = TAU / n as f32;
            let fixed = f.spacing * step;
            let r = self.crt.rand() as f32;
            let mut ang = (r * 3.051_851e-5 * (step - fixed) + fixed) * i as f32 + base * 0.000_191_753_45;
            if TAU < ang {
                ang -= TAU;
            }
            let (w0, w1) = (f.width[0], f.width[1]);
            let lo = w0 - w1;
            let r = self.crt.rand() as f32;
            let width = r * 5.326_484_7e-7 * ((w1 + w0) - lo) + lo * DEG2RAD;
            let (l0, l1) = (f.length[0], f.length[1]);
            let lo = l0 - l1;
            let r = self.crt.rand() as f32;
            let length = r * 3.051_851e-5 * ((l1 + l0) - lo) + lo;
            let (s0, s1) = (f.strength[0], f.strength[1]);
            let lo = s0 - s1;
            let r = self.crt.rand() as f32;
            let strength = r * 3.051_851e-5 * ((s1 + s0) - lo) + lo;
            v.push([ang, width, f.width_exponent, length, f.length_exponent, strength]);
        }
        v
    }

    /// `FUN_012d7dd0`: wind weights at position `t` along a branch of `level`.
    fn wind_weights(&self, w: &mut [f32; 2], level: i32, t: f32, flex: f32) {
        let wl = self.t.wind_level;
        if level < wl {
            *w = [1.0, 1.0];
        } else if level == wl {
            *w = [1.0 - t * flex, 1.0];
        } else if level == wl + 1 {
            w[1] = 1.0 - t * flex;
        } else if self.t.wind_falloff {
            w[1] *= 1.0 - flex;
        }
    }

    /// `FUN_012d3f30`: one ring of tube vertices.
    #[allow(clippy::too_many_arguments)]
    fn ring(
        &mut self,
        b: usize,
        node_i: usize,
        t: f32,
        n: i32,
        l: &LevelParams,
        wind: [f32; 2],
        groups: [u8; 2],
        tex_offset: f32,
        max_radius: f32,
        size: f32,
        base_radius: f32,
        length: f32,
        flip: bool,
    ) {
        let node = self.out.branches[b].nodes[node_i];
        self.out.branches[b].nodes[node_i].ring = n;
        let inv = 1.0 / n as f32;
        let mut first = node.pos;
        let rough_profile = l.roughness_profile.eval_fixed(t);
        let m = &node.frame;
        let mut u = 0.0f32;
        for j in 0..=n {
            let theta = u * TAU;
            // texture layer 0 (FUN_012d7c50)
            let uv = tex_coord(&l.layers[0], u, t, tex_offset, base_radius, length / size, flip);
            let (s, c) = theta.sin_cos();
            let radial = [m[3] * c + m[6] * s, m[4] * c + m[7] * s, m[5] * c + m[8] * s];
            let (s2, c2) = (theta + 1.570_796_4).sin_cos();
            let tangent = [m[3] * c2 + m[6] * s2, m[4] * c2 + m[7] * s2, m[5] * c2 + m[8] * s2];
            let binormal = [
                tangent[2] * radial[1] - tangent[1] * radial[2],
                tangent[0] * radial[2] - tangent[2] * radial[0],
                tangent[1] * radial[0] - tangent[0] * radial[1],
            ];
            let pos = if j == n {
                first
            } else {
                let mut flare = 0.0f32;
                for f in &self.out.branches[b].flares {
                    let mut fa = f[0];
                    let mut th = theta;
                    if PI < (theta - fa).abs() {
                        if fa <= theta {
                            fa += TAU;
                        } else {
                            th = theta + TAU;
                        }
                    }
                    let d = (th - fa).abs();
                    let mut add = 0.0;
                    let rem = f[3] - t;
                    if d < f[1] && 0.0 < rem {
                        add = (1.0 - d / f[1]).powf(f[2]) * f[5];
                        add *= (rem / f[3]).powf(f[4]);
                    }
                    flare += add;
                }
                let scale = flare + 1.0;
                let ratio = if node.radius != max_radius { node.radius / max_radius } else { 0.0 };
                let k = self.rough_counter % 1000;
                self.rough_counter += 1;
                let mut a = (l.roughness_frequency[0] * t).sin() * (l.roughness * ratio * size * rough_profile);
                a *= (l.roughness_frequency[1] * t).sin();
                a *= (l.roughness_frequency[1] * theta).cos();
                let mut r = (a + node.radius).min(max_radius);
                let rnd = self.table[k];
                r += (((rnd - 0.5) + rnd) - 0.5) * l.roughness_random * size * ratio * rough_profile;
                let mut off = [r * radial[0], r * radial[1], r * radial[2]];
                if scale != 1.0 {
                    off = [off[0] * scale, off[1] * scale, off[2] * scale];
                }
                let p = [node.pos[0] + off[0], node.pos[1] + off[1], node.pos[2] + off[2]];
                if j == 0 {
                    first = p;
                }
                p
            };
            let mesh = &mut self.out.mesh;
            mesh.positions.push(out(pos));
            mesh.tangents.push(out(tangent));
            mesh.binormals.push(out(binormal));
            mesh.normals.push([0.0, 1.0, 0.0]);
            mesh.uvs.push(uv);
            let ww = |w: f32| (1.0 - w).clamp(0.0, 1.0);
            mesh.wind_weights.push([ww(wind[0]), ww(wind[1])]);
            mesh.wind_groups.push(groups);
            u += inv;
        }
    }

    /// `FUN_012d6b10`: smooth normals of a finished tube branch.
    fn branch_normals(&mut self, b: usize, profile_t: f32) {
        let br = &self.out.branches[b];
        let start = br.vertex_start.max(0) as usize;
        let l = self.level(br.level, false);
        let _ = l;
        let segs = br.nodes.len();
        let blend = profile_t;
        let pos = |i: usize| self.out.mesh.positions[start + i];
        let mut normals = Vec::new();
        let mut base = 0usize;
        for c in 0..segs {
            let node = br.nodes[c];
            let dir_out = [-node.dir[0], node.dir[2], node.dir[1]];
            let n = node.ring as usize;
            let inv = 1.0 / n as f32;
            for j in 0..=n {
                let (a_i, mut self_i) = if j == 0 { (n + base, base) } else { (j + base, j + base) };
                let b_i = if j != n { self_i } else { base } + 1;
                let pa = pos(a_i - 1);
                let pb = pos(b_i);
                let (mut ax, mut ay, mut az) = (pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]);
                let il = inv_len(ax, ay, az);
                ax *= il;
                ay *= il;
                az *= il;
                let lower = if c != 0 {
                    let pn = br.nodes[c - 1].ring;
                    (base as i32 - 1 + ((pn as f32 * j as f32 * inv) as i32 - pn)) as usize
                } else {
                    self_i
                };
                if c != segs - 1 {
                    let nn = br.nodes[c + 1].ring;
                    self_i = (nn as f32 * j as f32 * inv) as usize + 1 + n + base;
                }
                let p0 = pos(lower);
                let p1 = pos(self_i);
                let (dx, dy, dz) = (p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]);
                let f = inv_len(dx, dy, dz);
                let mut nx = ay * f * dz - az * f * dy;
                let mut ny = az * f * dx - ax * f * dz;
                let mut nz = ax * f * dy - ay * f * dx;
                if nx == 0.0 && ny == 0.0 && nz == 0.0 {
                    [nx, ny, nz] = dir_out;
                }
                let frac = c as f32 / (segs as f32 - 1.0);
                let s = self.level(br.level, false).normal_profile.eval_fixed(frac);
                let k = (frac.sqrt() * (1.0 - blend) + blend) * s;
                let x = (nx - dir_out[0]) * k + dir_out[0];
                let y = (ny - dir_out[1]) * k + dir_out[1];
                let z = (nz - dir_out[2]) * k + dir_out[2];
                let il = inv_len(x, y, z);
                normals.push([il * x, y * il, z * il]);
            }
            base += 1 + n;
        }
        for (i, n) in normals.into_iter().enumerate() {
            if let Some(slot) = self.out.mesh.normals.get_mut(start + i) {
                *slot = n;
            }
        }
    }

    /// `FUN_012d7f20`: the segment of branch `b` holding length `x`, and the fraction in it.
    fn segment_at(&self, b: usize, x: f32) -> (usize, f32) {
        let nodes = &self.out.branches[b].nodes;
        let mut s = 0usize;
        for (i, node) in nodes.iter().enumerate().skip(1) {
            if x < node.length {
                s = i - 1;
                break;
            }
        }
        let f = (x - nodes[s].length) / (nodes[s + 1].length - nodes[s].length);
        (s, f)
    }

    /// `FUN_012d77f0`: snap a child to the nearest joint; −1 if not allowed.
    fn snap(&mut self, b: usize, t: f32, length: f32, min_angle: f32, max: i32) -> f32 {
        let x = t * length;
        let nodes = &mut self.out.branches[b].nodes;
        let n = nodes.len();
        if n < 2 {
            return -1.0;
        }
        let mut s = 0usize;
        for (i, node) in nodes.iter().enumerate().skip(1) {
            if x < node.length {
                s = i - 1;
                break;
            }
        }
        let mut k = s + 1;
        if x - nodes[s].length <= nodes[s + 1].length - x {
            k = s;
        }
        if k < n - 1 && nodes[k].snapped < max {
            nodes[k].snapped += 1;
            let mut ang = 180.0f32;
            if k > 0 {
                let a = nodes[k].dir;
                let p = nodes[k - 1].dir;
                let d = (a[2] * p[2] + a[1] * p[1] + p[0] * a[0]).clamp(-1.0, 1.0);
                ang = d.acos() * 57.295_78;
            }
            if min_angle <= ang {
                return nodes[k].length / length;
            }
        }
        -1.0
    }

    /// `FUN_012d95c0`: true = prune (no child here).
    fn pruned(&self, b: usize, t: f32, depth: i32, threshold: f32) -> bool {
        let mut x = t;
        if depth != 0 {
            let mut cur = b;
            let mut i = 1;
            while i < depth {
                match self.out.branches[cur].parent {
                    Some(p) => cur = p,
                    None => break,
                }
                i += 1;
            }
            x = self.out.branches[cur].t_parent;
        }
        if 0.0 <= threshold { x < threshold } else { -threshold < x }
    }

    /// `FUN_012d45a0`. Returns the branch mode (0 frond, 1 tube, 2 nothing).
    #[allow(clippy::too_many_arguments)]
    fn compute(
        &mut self,
        b: usize,
        mut seed: i32,
        size: f32,
        level: i32,
        origin: [f32; 3],
        t_parent: f32,
        parent_frame: &M3,
        parent_dir: [f32; 3],
        w_in: [f32; 2],
        group_in: [i32; 2],
        parent_radius: f32,
        is_root: bool,
    ) -> i32 {
        let t = self.t;
        let mut mode = if level < t.frond_start_level { 1 } else { self.frond_mode(b, t_parent) };
        if mode == 0 && is_root {
            mode = 1;
        }
        if mode == 2 {
            return 2;
        }
        let ga = if level == t.wind_level {
            let v = self.wind_counter;
            self.wind_counter += 1;
            v
        } else {
            group_in[0]
        };
        let gb = if level == t.wind_level + 1 {
            let v = self.wind_counter;
            self.wind_counter += 1;
            v
        } else {
            group_in[1]
        };
        if level == 0 {
            self.build_leaf_slots();
        }
        let l = self.level(level, is_root);
        if mode == 1 && t.cluster_level <= level {
            self.out.branches[b].vertex_start = self.out.mesh.positions.len() as i32;
        }
        self.out.branches[b].t_parent = t_parent;
        if mode == 1 {
            let f = self.make_flares(l);
            self.out.branches[b].flares = f;
        }
        let rng = &mut self.rng;
        let length = (l.length.eval(t_parent, false, rng) * f64::from(size)) as f32;
        let rot = rng.uniform(-180.0, 180.0) as f32;
        let start_angle = l.start_angle.eval(t_parent, false, rng) as f32;
        let gravity = l.gravity.eval(t_parent, false, rng) as f32;
        let radius = (l.radius.eval(t_parent, false, rng) * f64::from(size)) as f32;
        let mut r_abs = radius.abs();
        let flex = l.flexibility.eval(t_parent, false, rng) as f32;
        let mut cross = l.cross_sections as u16;
        let max_r = size * l.radius.max;
        let tex_offset = rot * 0.002_777_777_8 + 0.5 + t_parent;
        let mut limit = max_r;
        if t.cluster_level < level && 0.0 < parent_radius {
            limit = parent_radius * 0.85;
            if max_r <= limit {
                limit = max_r;
            }
        }
        if limit <= r_abs {
            r_abs = limit;
        }
        let r = r_abs;
        let rp0 = l.radius_profile.eval(0.0, false, &mut self.rng) as f32;
        let mut segs = l.segments + 1;
        let frond_radius = r * rp0;
        if mode == 0 && t.frond_segments.1 {
            segs = t.frond_segments.0 + 1;
        } else {
            if l.segment_reduction != 1.0 {
                let lo = l.length.min * size;
                let hi = l.length.max * size;
                if lo < hi {
                    let f = ((length - lo) / (hi - lo)).clamp(0.0, 1.0);
                    let s = segs as f32 * l.segment_reduction;
                    segs = ((segs as f32 - s) * f + s) as i32;
                    if segs < 2 {
                        segs = 2;
                    }
                }
            }
            if l.cross_section_reduction != 1.0 {
                let lo = l.radius.min * size;
                let hi = size * l.radius.max;
                if lo < hi {
                    let f = ((r_abs - lo) / (hi - lo)).clamp(0.0, 1.0);
                    let c = l.cross_section_reduction * f32::from(cross);
                    let n = ((f32::from(cross) - c) * f + c) as i32;
                    cross = n.max(3) as u16;
                }
            }
        }
        self.out.branches[b].cross_sections = cross;
        if segs < 2 {
            return mode;
        }
        let mut above_floor = t.floor_height <= origin[2];
        let d1 = l.disturbance.variance_only(0.0, &mut self.rng) as f32;
        let d2 = l.disturbance.variance_only(0.0, &mut self.rng) as f32;
        let mut n0 = BranchNode { pos: origin, frame: *parent_frame, ..Default::default() };
        let pf = parent_frame;
        let (px, py, pz) = (parent_dir[0], parent_dir[1], parent_dir[2]);
        let az = pf[6] * px + pf[7] * py + pf[8] * pz;
        rotate_axis(&mut n0.frame, rot, pf[1] * py + pf[0] * px + pf[2] * pz, pf[3] * px + pf[4] * py + pf[5] * pz, az);
        rotate_ab(&mut n0.frame, d2 + start_angle, d1);
        n0.dir = frame_dir(&n0.frame);
        n0.radius = (l.radius_profile.eval(0.0, true, &mut self.rng) * f64::from(r)) as f32;
        let flex0 = (l.flexibility_profile.eval(0.0, false, &mut self.rng) * f64::from(flex)) as f32;
        let (ang, ax) = gravity_axis(n0.dir);
        let g = (90.0 - ang).abs() * 0.011_111_111;
        let ap = (l.angle_profile.eval(0.0, false, &mut self.rng) - 0.5) as f32;
        let rm = axis_angle((ap + ap) * (g - 1.0) * gravity * ang, ax[0], ax[1], ax[2]);
        n0.frame = mul(&n0.frame, &rm);
        let tw = l.twist_profile.eval_fixed(0.0) * l.twist;
        let flip = !l.twist_no_alternate && (rot as i32 & 1) == 0;
        let tw = if flip { -tw } else { tw } * DEG2RAD;
        twist(&mut n0.frame, tw);
        n0.dir = frame_dir(&n0.frame);
        let mut w = w_in;
        self.wind_weights(&mut w, level, 0.0, flex0);
        n0.wind = w;
        let groups = [ga as u8, gb as u8];
        self.out.branches[b].nodes = vec![n0];
        let csp = l.cross_section_profile.eval_fixed(0.0).clamp(0.0, 1.0);
        let base_radius = n0.radius;
        if mode == 1 {
            if t.cluster_level <= level {
                let n = ((f32::from(cross) - 3.0) * csp + 3.0) as i32 & 0xffff;
                self.ring(b, 0, 0.0, n, l, w, groups, tex_offset, limit, size, base_radius, length, flip);
            }
        } else {
            self.out.fronds.push(FrondSpine {
                nodes: Vec::new(),
                wind_groups: [ga, gb],
                ..Default::default()
            });
            let fi = self.out.fronds.len() - 1;
            self.out.fronds[fi].nodes.push(n0);
        }
        let mut cum = 0.0f32;
        for i in 1..segs {
            let tr = i as f32 / (segs - 1) as f32;
            let te = tr.powf(l.segment_exponent);
            let step = te * length - cum;
            let csp = l.cross_section_profile.eval_fixed(te).clamp(0.0, 1.0);
            let nring = ((f32::from(cross) - 3.0) * csp + 3.0) as i32 & 0xffff;
            let prev = *self.out.branches[b].nodes.last().unwrap();
            let mut nd = BranchNode {
                radius: (l.radius_profile.eval(te, true, &mut self.rng) * f64::from(r)) as f32,
                ..Default::default()
            };
            let flexi = (l.flexibility_profile.eval(te, false, &mut self.rng) * f64::from(flex)) as f32;
            nd.frame = prev.frame;
            let dir = frame_dir(&nd.frame);
            let (ang, ax) = gravity_axis(dir);
            let g = (90.0 - ang).abs();
            let ap = (l.angle_profile.eval(te, false, &mut self.rng) - 0.5) as f32;
            let a = (ap * -0.034_906_585 - g * ap * -0.000_387_850_95) * gravity * ang;
            curve(&mut nd.frame, a, ax);
            let db = l.disturbance.variance_only(te, &mut self.rng) as f32;
            let da = (l.disturbance.variance_only(te, &mut self.rng) as f32) * DEG2RAD;
            disturb(&mut nd.frame, da, db * DEG2RAD);
            let tw = l.twist_profile.eval_fixed(te) * l.twist;
            let tw = if flip { -tw } else { tw } * DEG2RAD;
            twist(&mut nd.frame, tw);
            nd.dir = frame_dir(&nd.frame);
            nd.pos = [prev.dir[0] * step + prev.pos[0], prev.dir[1] * step + prev.pos[1], prev.dir[2] * step + prev.pos[2]];
            if t.floor_enabled && above_floor && t.floor_min_level <= level {
                let z = nd.pos[2];
                if z <= prev.pos[2] && z < t.floor_height {
                    nd.pos[2] = tr.powf(t.floor_exponent) * t.floor_strength * (z - t.floor_height) + t.floor_height;
                }
            }
            if t.floor_height <= nd.pos[2] {
                above_floor = true;
            }
            let mut w = w_in;
            self.wind_weights(&mut w, level, te, flexi);
            nd.wind = w;
            cum += step;
            nd.length = cum;
            self.out.branches[b].nodes.push(nd);
            let ni = self.out.branches[b].nodes.len() - 1;
            if mode == 1 {
                if t.cluster_level <= level {
                    self.ring(b, ni, te, nring, l, w, groups, tex_offset, limit, size, base_radius, length, flip);
                }
            } else {
                let fi = self.out.fronds.len() - 1;
                self.out.fronds[fi].nodes.push(nd);
            }
        }
        if mode == 1 {
            if t.cluster_level <= level {
                let p3 = l.normal_blend.eval_fixed(t_parent);
                self.branch_normals(b, p3);
            }
        } else {
            self.finish_frond(frond_radius);
        }
        self.children(b, seed_ref(&mut seed), size, level, length, is_root, groups_i(ga, gb));
        self.out.branches[b].weight = branch_weight(&self.out.branches[b].nodes);
        mode
    }

    /// `FUN_012c7a10`: frond length, size, texture and angle.
    fn finish_frond(&mut self, radius_scale: f32) {
        let fi = self.out.fronds.len() - 1;
        let mut len = 0.0f32;
        {
            let nodes = &self.out.fronds[fi].nodes;
            for k in 1..nodes.len() {
                let (a, b) = (nodes[k - 1].pos, nodes[k].pos);
                len += approx_sqrt((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1]) + (b[2] - a[2]) * (b[2] - a[2]));
            }
        }
        let odd = self.out.fronds.len() % 2 == 1;
        let fr = &mut self.out.fronds[fi];
        fr.length = len;
        let n = self.t.frond_textures.len();
        if n == 0 {
            fr.texture = 0;
            fr.angle = 0.0;
            fr.half_width = len * 0.5;
        } else {
            let k = (self.rng.uniform(0.0, 100_000.0) as u32 % n as u32) as usize;
            let tex = self.t.frond_textures[k];
            let hw = tex[0] * len * 0.5;
            fr.half_width = tex[1] * hw;
            let a = self.rng.uniform(tex[2], tex[3]) as f32;
            fr.texture = k;
            fr.angle = if odd { -a } else { a };
        }
        self.out.fronds[fi].radius_scale = len * radius_scale;
    }

    /// The children loop of `FUN_012d45a0` (after `LAB_012d61a7`).
    #[allow(clippy::too_many_arguments)]
    fn children(&mut self, b: usize, seed: &mut i32, size: f32, level: i32, length: f32, is_root: bool, groups: [i32; 2]) {
        let t = self.t;
        let l = self.level(level, is_root);
        let child_level = level + 1;
        let nchild = ((l.frequency / size) * length) as i32;
        let mut total = nchild;
        if child_level == t.root_level {
            total = ((t.root_frequency / size) * length) as i32 + nchild;
        }
        let count = if is_root { 0 } else { total };
        let last = self.nlevels - 1;
        if !(child_level <= last) || count <= 0 {
            return;
        }
        if child_level > last {
            return;
        }
        if child_level == last {
            self.leaf_branch_start = self.out.leaves.len();
        }
        for k in 0..count {
            let is_root_child = nchild <= k;
            let (lo, hi);
            if k == nchild {
                self.rng.seed(*seed);
            } else if child_level < last && k < nchild {
                *seed += 3;
                self.rng.seed(*seed);
            }
            if child_level < last && k == 0 && t.cluster_level < child_level && 0 < nchild {
                let (a, c) = (l.child_range[0], l.child_range[1]);
                hi = (c - a) * 0.95 + a;
                lo = (c - a) * 0.85 + a;
            } else if k < nchild {
                lo = l.child_range[0];
                hi = l.child_range[1];
            } else {
                lo = t.root_range[0];
                hi = t.root_range[1];
            }
            let pos_t = self.rng.uniform(lo, hi) as f32;
            if k < nchild {
                let (a, c) = (l.child_range[0], l.child_range[1]);
                let mut f = 0.0f32;
                if a < c {
                    f = (pos_t - a) / (c - a);
                    if f < 0.0 {
                        f = 0.0;
                    }
                }
                if 1.0 <= f {
                    f = 1.0;
                }
                let prob = l.child_probability.eval_fixed(f);
                if prob != 1.0 {
                    let r = self.table[self.table_counter % 1000];
                    self.table_counter += 1;
                    if r > prob {
                        continue;
                    }
                }
            }
            let mut tt = pos_t;
            if k < nchild && child_level < last && l.snap {
                let r = self.rng.uniform(0.0, 1.0) as f32;
                if r <= l.snap_probability {
                    let s = self.snap(b, pos_t, length, l.snap_min_angle, l.snap_max);
                    tt = s;
                    if s < l.child_range[0] || (l.child_range[1] <= s && s != l.child_range[1]) {
                        continue;
                    }
                }
            }
            if self.pruned(b, tt, l.prune_depth, l.prune) {
                continue;
            }
            let (s, f) = self.segment_at(b, tt * length);
            let (n0, n1) = (self.out.branches[b].nodes[s], self.out.branches[b].nodes[s + 1]);
            let w = [(n1.wind[0] - n0.wind[0]) * f + n0.wind[0], (n1.wind[1] - n0.wind[1]) * f + n0.wind[1]];
            if child_level < last && k < nchild {
                self.rng.seed(*seed);
                let _ = self.rng.uniform(0.0, 100.0);
            }
            let pos = [(n1.pos[0] - n0.pos[0]) * f + n0.pos[0], (n1.pos[1] - n0.pos[1]) * f + n0.pos[1], (n1.pos[2] - n0.pos[2]) * f + n0.pos[2]];
            let (rlo, rhi) = if k < nchild { (l.child_range[0], l.child_range[1]) } else { (t.root_range[0], t.root_range[1]) };
            let tc = if rhi != rlo { (tt - rlo) / (rhi - rlo) } else { 1.0 };
            if child_level < last {
                let child_r = (n1.radius - n0.radius) * f + n0.radius;
                let ci = self.out.branches.len();
                self.out.branches.push(Branch { parent: Some(b), level: child_level, vertex_start: -1, ..Default::default() });
                let (frame, dir, root) = if child_level == t.cluster_level {
                    ([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0], false)
                } else {
                    (n0.frame, n0.dir, is_root_child)
                };
                let r = self.compute(ci, *seed, size, child_level, pos, tc, &frame, dir, w, groups, child_r, root);
                if r == 2 {
                    self.out.branches.truncate(ci);
                }
            } else {
                self.place_leaf(b, size, child_level, pos, tc, &n0.frame, n0.dir, w, groups);
            }
        }
    }

    /// `FUN_012d3cd0`: the leaf texture slot lists (two slots per texture).
    fn build_leaf_slots(&mut self) {
        self.slots.clear();
        self.blossom_slots.clear();
        for (i, tex) in self.t.leaf_textures.iter().enumerate() {
            let list = if tex.blossom { &mut self.blossom_slots } else { &mut self.slots };
            list.push(2 * i as i32);
            list.push(2 * i as i32 + 1);
        }
    }

    /// `FUN_012d6fa0` + the placement part of `FUN_012d8090`.
    #[allow(clippy::too_many_arguments)]
    fn place_leaf(
        &mut self,
        b: usize,
        size: f32,
        level: i32,
        pos: [f32; 3],
        t: f32,
        frame: &M3,
        dir: [f32; 3],
        w: [f32; 2],
        groups: [i32; 2],
    ) {
        let l = self.level(level, false);
        let dist = (l.length.eval(t, false, &mut self.rng) as f32 * size) / l.segments as f32;
        let rot = self.rng.uniform(-180.0, 180.0) as f32;
        let mut m = *frame;
        let p = frame;
        let (dx, dy, dz) = (dir[0], dir[1], dir[2]);
        let az = p[6] * dx + p[7] * dy + p[8] * dz;
        rotate_axis(&mut m, rot, p[1] * dy + dx * p[0] + p[2] * dz, p[3] * dx + p[4] * dy + p[5] * dz, az);
        // fixed 60° tilt between rows 0 and 2
        let (a0, a1, a2) = (m[0] * 0.866_025_45, m[1] * 0.866_025_45, m[2] * 0.866_025_45);
        m[0] = m[0] * 0.499_999_97 - m[6] * 0.866_025_45;
        m[6] = m[6] * 0.499_999_97 + a0;
        m[1] = m[1] * 0.499_999_97 - m[7] * 0.866_025_45;
        m[7] = m[7] * 0.499_999_97 + a1;
        m[2] = m[2] * 0.499_999_97 - m[8] * 0.866_025_45;
        m[8] = m[8] * 0.499_999_97 + a2;
        let d = frame_dir(&m);
        let lp = [d[0] * dist + pos[0], dist * d[1] + pos[1], dist * d[2] + pos[2]];
        let _ = (w, groups, b);
        // The rest of FUN_012d8090 (texture choice, spacing test, normal, dimming).
        let slot = if !self.blossom_slots.is_empty() && self.blossom(b, t) {
            let n = self.blossom_slots.len();
            let mut k = 0;
            if n > 1 {
                k = self.rng.uniform(0.0, 100_000.0) as u32 as usize % n;
            }
            self.blossom_slots[k]
        } else if self.slots.len() < 2 {
            match self.slots.first() {
                Some(&s) => s,
                None => return,
            }
        } else {
            let n = self.slots.len();
            let k = self.rng.uniform(0.0, 1.0e6) as u32 as usize % n;
            self.slots[k]
        };
        let tex = (slot / 2) as usize;
        let (spacing, mode) = self.t.leaf_spacing;
        if mode == 1 || mode == 2 {
            let ts = &self.t.leaf_textures[tex];
            let half = ts.size[0].max(ts.size[1]) * spacing;
            let from = if mode == 1 { self.leaf_branch_start } else { 0 };
            let near = self.out.leaves[from..].iter().rev().any(|o| {
                lp[0] < o.pos_z_up()[0] + half
                    && o.pos_z_up()[0] - half < lp[0]
                    && lp[1] < o.pos_z_up()[1] + half
                    && o.pos_z_up()[1] - half < lp[1]
                    && lp[2] < o.pos_z_up()[2] + half
                    && o.pos_z_up()[2] - half < lp[2]
            });
            if near {
                return;
            }
        }
        let mut dim = 1.0f32;
        if self.t.leaf_dimming.0 {
            let mut tt = t;
            let depth = self.t.blossom_rule.1;
            let mut cur = Some(b);
            let mut i = 0;
            if depth != 0 {
                while let (Some(c), true) = (cur, i < depth) {
                    tt *= self.out.branches[c].t_parent;
                    cur = self.out.branches[c].parent;
                    i += 1;
                }
            }
            dim = (1.0 - self.t.leaf_dimming.1) * (1.0 - tt) + tt;
        }
        let rock = self.rng.uniform(0.0, 10_000.0) as i32;
        // The leaf basis (FUN_012d8090): D from the attach direction blended with the
        // orientation vector, R horizontal, U = R × D.
        let ts = &self.t.leaf_textures[tex];
        let p4 = {
            let v = [lp[0] - pos[0], lp[1] - pos[1], lp[2] - pos[2]];
            let i = inv_len(v[0], v[1], v[2]);
            [i * v[0], i * v[1], i * v[2]]
        };
        let p5 = {
            let br = &self.out.branches[b];
            let mut v = p4;
            if !br.nodes.is_empty() {
                if self.t.blossom_rule.1 == 0 {
                    let k = ((br.nodes.len() as f32 - 1.0) * t) as usize;
                    v = br.nodes[k.min(br.nodes.len() - 1)].dir;
                } else {
                    let mut cur = b;
                    let mut i = 0;
                    while i < self.t.blossom_rule.1 - 1 {
                        match self.out.branches[cur].parent {
                            Some(p) => cur = p,
                            None => break,
                        }
                        i += 1;
                    }
                    let base = self.out.branches[cur].nodes.first().map_or(pos, |n| n.pos);
                    v = [lp[0] - base[0], lp[1] - base[1], lp[2] - base[2]];
                }
            }
            let l2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
            let l = if 1.192_092_9e-5 < l2 { l2.sqrt() } else { 0.01 };
            [v[0] * (1.0 / l), v[1] * (1.0 / l), v[2] * (1.0 / l)]
        };
        let k = ts.color_variance;
        let mut d = if 0.0001 <= approx_sqrt(p4[0] * p4[0] + p4[1] * p4[1] + p4[2] * p4[2]) {
            [(p4[0] - p5[0]) * k + p5[0], (p4[1] - p5[1]) * k + p5[1], (p4[2] - p5[2]) * k + p5[2]]
        } else {
            p5
        };
        let i = inv_len(d[0], d[1], d[2]);
        d = [i * d[0], i * d[1], i * d[2]];
        if ts.mesh.is_some() {
            // PROVISIONAL: assumes the game's leaf lighting mode 0 (`DAT_01842a88 + 0x38`).
            let kb = ts.mesh_up_bend;
            d = [d[0] - d[0] * kb, d[1] - kb * d[1], (1.0 - d[2]) * kb + d[2]];
            let i = inv_len(d[0], d[1], d[2]);
            d = [d[0] * i, i * d[1], i * d[2]];
        }
        let mut r = [d[1], -d[0], 0.0];
        let il = inv_len(r[0], r[1], 0.0);
        r = [il * r[0], il * r[1], 0.0];
        let mut u = [r[1] * d[2], -(d[2] * r[0]), d[1] * r[0] - r[1] * d[0]];
        let iu = inv_len(u[0], u[1], u[2]);
        u = [iu * u[0], iu * u[1], iu * u[2]];
        if approx_sqrt(r[0] * r[0] + r[1] * r[1]) < 0.5 {
            let ll = d[0] * d[0] + d[1] * d[1];
            let ll = if ll <= 1.192_092_9e-5 { 0.01 } else { ll.sqrt() };
            u = [-(d[0] / ll), (-1.0 / ll) * d[1], 0.0];
            let (a, bb, c) = (d[2] * u[1], d[2] * u[0], d[0] * u[1] - d[1] * u[0]);
            let ir = inv_len(bb, a, c);
            r = [-(ir * a), bb * ir, ir * c];
        }
        if ts.mesh.is_some() && ts.mesh_roll != 0.0 {
            let rnd = self.table[self.leaf_counter % 1000];
            self.leaf_counter += 1;
            let ang = ((rnd + rnd) * ts.mesh_roll - ts.mesh_roll) * DEG2RAD;
            let (s, c) = ang.sin_cos();
            let (r0, u0) = (r, u);
            r = [r0[0] * c + u0[0] * s, r0[1] * c + u0[1] * s, r0[2] * c + u0[2] * s];
            u = [u0[0] * c + r0[0] * -s, u0[1] * c + r0[1] * -s, u0[2] * c + r0[2] * -s];
        }
        let cv = self.rng.uniform(-ts.color_variance, ts.color_variance) as f32;
        let color = [ts.color[0] + cv, ts.color[1] + cv, ts.color[2] + cv];
        self.out.leaves.push(PlacedLeaf {
            pos: out(lp),
            texture: tex,
            slot,
            normal: out(d),
            basis: [out(r), out(u), out(d)],
            color,
            dimming: dim,
            rock_group: rock,
            wind: w,
            wind_groups: groups,
            t,
        });
    }

    /// `FUN_012d7fa0`: blossom texture here?
    #[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(a < b)` keeps the exe's NaN handling
    fn blossom(&mut self, b: usize, t: f32) -> bool {
        let (x, depth, prob) = self.t.blossom_rule;
        let mut tt = t;
        if depth != 0 {
            let mut cur = b;
            let mut i = 1;
            while i < depth {
                match self.out.branches[cur].parent {
                    Some(p) => cur = p,
                    None => break,
                }
                i += 1;
            }
            tt = self.out.branches[cur].t_parent;
        }
        if 0.0 <= x {
            if x < tt {
                return self.rng.uniform(0.0, 1.0) as f32 <= prob;
            }
            false
        } else if tt < -x {
            !(prob < self.rng.uniform(0.0, 1.0) as f32)
        } else {
            false
        }
    }
}

impl PlacedLeaf {
    fn pos_z_up(&self) -> [f32; 3] {
        [-self.pos[0], self.pos[2], self.pos[1]]
    }
}

fn seed_ref(s: &mut i32) -> &mut i32 {
    s
}

fn groups_i(a: i32, b: i32) -> [i32; 2] {
    [a, b]
}

/// Angle (degrees) between `dir` and gravity, and the normalised rotation axis `dir × g`.
fn gravity_axis(dir: [f32; 3]) -> (f32, [f32; 3]) {
    let g = GRAVITY;
    let c = (g[0] * dir[0] + g[1] * dir[1] + g[2] * dir[2]).clamp(-1.0, 1.0);
    let ang = c.acos() * 57.295_78;
    let x = g[2] * dir[1] - g[1] * dir[2];
    let y = g[0] * dir[2] - g[2] * dir[0];
    let z = g[1] * dir[0] - g[0] * dir[1];
    let il = inv_len(y, x, z);
    (ang, [x * il, y * il, z * il])
}

/// The gravity bend inside the segment loop (inline axis-angle in radians, the exe's term order).
fn curve(m: &mut M3, a: f32, ax: [f32; 3]) {
    let (s, c) = a.sin_cos();
    let (x, y, z) = (ax[0], ax[1], ax[2]);
    let omc = 1.0 - c;
    let fx = omc * x;
    let r00 = x * fx + c;
    let r01 = s * z + y * fx;
    let r10 = y * fx - s * z;
    let r02 = z * fx - s * y;
    let r20 = s * y + z * fx;
    let r22 = omc * z * z + c;
    let r11 = y * omc * y + c;
    let yz = z * omc * y;
    let r12 = s * x + yz;
    let r21 = yz - s * x;
    let o = *m;
    m[0] = r10 * o[1] + r00 * o[0] + r20 * o[2];
    m[1] = r11 * o[1] + r01 * o[0] + r21 * o[2];
    m[2] = r12 * o[1] + r02 * o[0] + r22 * o[2];
    m[3] = r10 * o[4] + r00 * o[3] + r20 * o[5];
    m[4] = r11 * o[4] + r01 * o[3] + r21 * o[5];
    m[5] = r12 * o[4] + r02 * o[3] + r22 * o[5];
    m[6] = r10 * o[7] + r00 * o[6] + r20 * o[8];
    m[7] = r11 * o[7] + r01 * o[6] + r21 * o[8];
    m[8] = r12 * o[7] + r02 * o[6] + r22 * o[8];
}

/// The disturbance rotation inside the segment loop (angles in radians).
fn disturb(m: &mut M3, a: f32, b: f32) {
    let (sa, ca) = a.sin_cos();
    let (sb, cb) = b.sin_cos();
    let f18 = sb * ca;
    let f494 = cb * ca;
    let f4c4 = cb * sa;
    let f3e0 = sb * sa;
    let o = *m;
    m[0] = o[3] * f18 + f494 * o[0] + o[6] * -sa;
    m[1] = o[4] * f18 + f494 * o[1] + o[7] * -sa;
    m[2] = o[5] * f18 + f494 * o[2] + -sa * o[8];
    m[3] = o[3] * cb + -sb * o[0];
    m[4] = o[4] * cb + -sb * o[1];
    m[5] = o[5] * cb + -sb * o[2];
    m[6] = f4c4 * o[0] + f3e0 * o[3] + o[6] * ca;
    m[7] = f4c4 * o[1] + f3e0 * o[4] + o[7] * ca;
    m[8] = f4c4 * o[2] + f3e0 * o[5] + o[8] * ca;
}

/// The twist about the branch axis (radians).
fn twist(m: &mut M3, a: f32) {
    let (s, c) = a.sin_cos();
    let o = *m;
    m[0] = o[0] * c - o[1] * s;
    m[1] = s * o[0] + o[1] * c;
    m[3] = o[3] * c - o[4] * s;
    m[4] = o[3] * s + o[4] * c;
    m[6] = o[6] * c - s * o[7];
    m[7] = s * o[6] + c * o[7];
}

/// `FUN_012d7e80`.
fn branch_weight(nodes: &[BranchNode]) -> f32 {
    let mut w = 0.0f32;
    for k in 1..nodes.len() {
        let (a, b) = (nodes[k - 1], nodes[k]);
        let d = (b.pos[0] - a.pos[0]) * (b.pos[0] - a.pos[0])
            + (b.pos[1] - a.pos[1]) * (b.pos[1] - a.pos[1])
            + (b.pos[2] - a.pos[2]) * (b.pos[2] - a.pos[2]);
        w += (b.radius + a.radius) * approx_sqrt(d);
    }
    w
}

/// `FUN_012d7c50`: one texture layer's coordinate at ring position `u`, branch position `t`.
fn tex_coord(c: &super::params::TexLayer, u: f32, t: f32, offset: f32, radius: f32, len_ratio: f32, flip: bool) -> [f32; 2] {
    let mut tw = c.twist;
    if flip {
        tw = -tw;
    }
    let mut vs = c.v_scale;
    if !c.v_absolute {
        vs *= len_ratio;
    }
    let mut us = c.u_scale;
    if !c.u_absolute {
        us = us * radius * TAU;
    }
    let s = if !c.segmented {
        us * u + c.u_offset + tw * t
    } else {
        let mut a = c.u_offset * 90.0;
        if a <= 0.0 {
            a = 0.0;
        }
        while 360.0 < a {
            a -= 360.0;
        }
        while a < 0.0 {
            a += 360.0;
        }
        let w = if us == 0.0 { 0.0 } else { 360.0 / us };
        let mut end = w + a;
        let x = u * 360.0;
        if 360.0 < end {
            end = 360.0;
            a = 360.0 - w;
        }
        let mut q = a;
        if a <= x {
            q = x;
            if end <= x {
                q = end;
            }
        }
        ((q - a) / w) * (c.u_segment[1] - c.u_segment[0]) + c.u_segment[0]
    };
    let off = if c.random_offset { offset } else { 0.0 };
    let mut v = (off + t) * vs - c.v_offset;
    if c.clamp_v {
        v = v.clamp(0.0, 1.0);
        v = (c.v_range[1] - c.v_range[0]) * v + c.v_range[0];
    }
    [s, v]
}

/// Computes a tree (`CTreeEngine::Compute`) with the file's own seed.
pub fn compute_tree(t: &TreeParams) -> TreeGeometry {
    let mut g = Gen {
        t,
        rng: Newran::default(),
        crt: MsvcRand::default(),
        table: Vec::with_capacity(1000),
        table_counter: 0,
        rough_counter: 0,
        leaf_counter: 0,
        wind_counter: 0,
        nlevels: t.levels.len() as i32,
        out: TreeGeometry::default(),
        slots: Vec::new(),
        blossom_slots: Vec::new(),
        leaf_branch_start: 0,
    };
    g.rng.seed(t.seed);
    for _ in 0..1000 {
        let v = g.rng.uniform(0.0, 1.0) as f32;
        g.table.push(v);
    }
    g.rng.seed(t.seed);
    let size = g.rng.uniform(t.size - t.size_variance, t.size + t.size_variance) as f32;
    g.crt.srand(t.flare_seed as u32);
    g.rng.seed(t.seed);
    g.wind_counter = t.seed;
    g.out.size = size;
    let (s, c) = (t.rotation * DEG2RAD).sin_cos();
    let frame = [c, s, 0.0, -s, c, 0.0, 0.0, 0.0, 1.0];
    g.out.branches.push(Branch { parent: None, level: 0, vertex_start: -1, ..Default::default() });
    let _ = g.leaf_counter;
    g.compute(0, t.seed, size, 0, [0.0; 3], 0.0, &frame, [0.0, 0.0, 1.0], [1.0, 1.0], [t.seed, t.flare_seed], -1.0, false);
    build_branch_lods(&mut g);
    g.out.frond_mesh = super::frond::build_fronds(t, &g.out.fronds);
    g.out
}

/// `FUN_012c9250` + `FUN_012d7920`: branch triangle strips per LOD. PROVISIONAL: every LOD keeps
/// every branch for now (the exe drops the smallest branches by weight; see SPEEDTREE.md §5).
fn build_branch_lods(g: &mut Gen) {
    let nlods = g.t.branch_lods.0.max(1) as usize;
    let mut strips: Vec<Vec<u32>> = Vec::new();
    for br in &g.out.branches {
        if br.cross_sections < 2 || br.vertex_start < 0 || br.nodes.len() < 2 {
            continue;
        }
        let mut local: Vec<u32> = Vec::new();
        let mut off = 0i32;
        for s in 0..br.nodes.len() - 1 {
            let a = br.nodes[s].ring;
            let bn = br.nodes[s + 1].ring;
            let m = a.max(bn) + 1;
            for j in 0..m {
                let f = j as f32 * (1.0 / (m as f32 - 1.0));
                local.push(((bn as f32 * f + 0.5) as i32 + (a + 1) + off + br.vertex_start) as u16 as u32);
                local.push(((a as f32 * f + 0.5) as i32 + off + br.vertex_start) as u16 as u32);
            }
            let last = local[local.len() - 2];
            local.push(last);
            local.push(last);
            off += a + 1;
        }
        strips.push(local);
    }
    g.out.mesh.lod_strips = vec![strips; nlods];
}

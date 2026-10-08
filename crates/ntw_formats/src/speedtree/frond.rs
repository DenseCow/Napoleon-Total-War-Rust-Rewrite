//! Frond geometry (`CFrondEngine::Compute` `FUN_012c70e0`), from the frond spines the branch pass
//! made: blade fronds (13003 = 0, `FUN_012c48d0`) and extrusion fronds (13003 = 1, `FUN_012c56e0`
//! with the profile of `FUN_012c6b90`), both CONFIRMED arithmetic. Only LOD 0 strips are built
//! (the frond LOD selection `FUN_012c6480`/`FUN_012c7560` is not reproduced yet).

use super::generate::{BranchNode, FrondSpine, approx_sqrt};
use super::params::TreeParams;

/// Frond vertices as the exe writes them (Y-up `(−x, z, y)`).
#[derive(Debug, Clone, Default)]
pub struct FrondMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
    pub binormals: Vec<[f32; 3]>,
    /// 0..1 inside the frond texture (`u` across, `v` along).
    pub uvs: Vec<[f32; 2]>,
    /// Frond texture index per vertex (the exe's byte).
    pub textures: Vec<u8>,
    /// Wind weights (`1 − w`, clamped) and groups.
    pub wind_weights: Vec<[f32; 2]>,
    pub wind_groups: Vec<[u8; 2]>,
    /// Triangle strips (one per blade).
    pub strips: Vec<Vec<u32>>,
}

fn out(v: [f32; 3]) -> [f32; 3] {
    [-v[0], v[2], v[1]]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let l = if l2 <= 1.192_092_9e-5 { 0.01 } else { l2.sqrt() };
    let i = 1.0 / l;
    [v[0] * i, v[1] * i, v[2] * i]
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    approx_sqrt((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1]) + (b[2] - a[2]) * (b[2] - a[2]))
}

/// Builds all fronds of a tree.
pub fn build_fronds(t: &TreeParams, spines: &[FrondSpine]) -> FrondMesh {
    let mut m = FrondMesh::default();
    for f in spines {
        if t.frond_type == 1 {
            extrusion(&mut m, t, f);
        } else {
            blades(&mut m, t.frond_blades.max(1), f);
        }
    }
    m
}

/// `profile`'s result: the cross-section points, normals and tangents.
type Profile = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 3]>);

/// `FUN_012c6b90`: the extrusion cross-section — points `(0, y, z)`, normals and tangents.
/// The 13005 profile's variance is 0 in every shipped file, so its random draws do not change
/// the values (we evaluate without them).
fn profile(t: &TreeParams, hw: f32) -> Profile {
    let s = t.frond_profile_segments.max(2) as usize;
    let base = t.frond_profile.eval_fixed(0.0);
    let mut pts = vec![[0.0f32; 3]; 2 * s - 1];
    for (k, pt) in pts.iter_mut().enumerate().take(s) {
        let f = 1.0 - k as f32 / (s as f32 - 1.0);
        *pt = [0.0, -(hw * f), t.frond_profile.eval_fixed(f) * hw - base * hw];
    }
    for j in 1..s {
        let p = pts[s - 1 - j];
        pts[s - 1 + j] = [p[0], -p[1], p[2]];
    }
    let mut normals = Vec::with_capacity(pts.len());
    let mut tangents = Vec::with_capacity(pts.len());
    let atan = |a: [f32; 3], b: [f32; 3]| (b[2] - a[2]).atan2(b[1] - a[1]);
    for k in 0..pts.len() {
        let a = if k == s - 1 {
            1.570_796_4
        } else if k == 0 {
            atan(pts[0], pts[1]) + 1.570_796_4
        } else {
            let mut a = atan(pts[k - 1], pts[k]);
            if k != pts.len() - 1 {
                a = (atan(pts[k], pts[k + 1]) + a) * 0.5;
            }
            a + 1.570_796_4
        };
        let (sn, c) = a.sin_cos();
        let l = (c * c + sn * sn).sqrt();
        normals.push([0.0, c * (1.0 / l), sn * (1.0 / l)]);
        let (sn2, c2) = (a + 1.570_796_4).sin_cos();
        tangents.push([0.0, -c2, -sn2]);
    }
    (pts, normals, tangents)
}

/// `FUN_012c56e0`: the profile swept along the spine, each point averaged between this node's
/// and the previous node's frames (rows 1 and 2 turned by the frond angle).
fn extrusion(m: &mut FrondMesh, t: &TreeParams, f: &FrondSpine) {
    let n = f.nodes.len();
    if n < 2 {
        return;
    }
    let (pts, nrm, tan) = profile(t, f.half_width);
    let np = pts.len();
    let first = m.positions.len();
    let (s, c) = (f.angle * 0.017_453_292).sin_cos();
    let groups = [f.wind_groups[0] as u8, f.wind_groups[1] as u8];
    let turned = |fr: &[f32; 9]| {
        (
            [fr[0], fr[1], fr[2]],
            [fr[6] * s + fr[3] * c, fr[7] * s + fr[4] * c, fr[8] * s + fr[5] * c],
            [fr[3] * -s + fr[6] * c, fr[4] * -s + fr[7] * c, fr[5] * -s + fr[8] * c],
        )
    };
    let ww = |w: f32| (1.0 - w).clamp(0.0, 1.0);
    for i in 0..n {
        let node: &BranchNode = &f.nodes[i];
        let (c0, c1, c2) = turned(&node.frame);
        let (p0, p1, p2) = turned(if i == 0 { &node.frame } else { &f.nodes[i - 1].frame });
        let xf = |v: [f32; 3]| {
            [
                (p1[0] + c1[0]) * v[1] + c0[0] * v[0] + (p2[0] + c2[0]) * v[2] + v[0] * p0[0],
                (c1[1] + p1[1]) * v[1] + c0[1] * v[0] + (p2[1] + c2[1]) * v[2] + p0[1] * v[0],
                (c0[2] + p0[2]) * v[0] + (c1[2] + p1[2]) * v[1] + (p2[2] + c2[2]) * v[2],
            ]
        };
        let half_norm = |v: [f32; 3]| {
            let l2 = (v[1] * v[1] + v[0] * v[0] + v[2] * v[2]) * 0.25;
            let l = if l2 <= 1.192_092_9e-5 { 0.01 } else { l2.sqrt() };
            [v[0] * (1.0 / l) * 0.5, v[1] * (1.0 / l) * 0.5, v[2] * (1.0 / l) * 0.5]
        };
        let v = i as f32 / (n as f32 - 1.0);
        for k in 0..np {
            let q = xf(pts[k]);
            let pos = [node.pos[0] + q[0] * 0.5, node.pos[1] + q[1] * 0.5, node.pos[2] + q[2] * 0.5];
            let nn = half_norm(xf(nrm[k]));
            let tt = half_norm(xf(tan[k]));
            let cx = [tt[1] * nn[2] - tt[2] * nn[1], tt[2] * nn[0] - tt[0] * nn[2], tt[0] * nn[1] - tt[1] * nn[0]];
            let bin = if 0.1 <= approx_sqrt(cx[2] * cx[2] + cx[0] * cx[0] + cx[1] * cx[1]) { normalize(cx) } else { nn };
            m.positions.push(out(pos));
            m.normals.push(out(nn));
            m.tangents.push(out(tt));
            m.binormals.push(out(bin));
            m.uvs.push([k as f32 / (np as f32 - 1.0), v]);
            m.textures.push(f.texture as u8);
            m.wind_weights.push([ww(node.wind[0]), ww(node.wind[1])]);
            m.wind_groups.push(groups);
        }
    }
    // V by arc length down each profile column (nodes 1..n−1, × 0.999)
    for k in 0..np {
        let mut cum = vec![0.0f32; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + dist(m.positions[first + i * np + k], m.positions[first + (i - 1) * np + k]);
        }
        let total = cum[n - 1];
        for (i, &c) in cum.iter().enumerate().skip(1) {
            m.uvs[first + i * np + k][1] = c / total * 0.999;
        }
    }
    // LOD 0 strips: one per row of nodes (the exe joins them into one serpentine strip)
    for i in 0..n - 1 {
        let mut s = Vec::with_capacity(2 * np);
        for k in 0..np {
            s.push((first + i * np + k) as u32);
            s.push((first + (i + 1) * np + k) as u32);
        }
        m.strips.push(s);
    }
}

/// `FUN_012c48d0`: `blades` crossed cards along one spine.
fn blades(m: &mut FrondMesh, blades: i32, f: &FrondSpine) {
    let n = f.nodes.len();
    if n < 2 {
        return;
    }
    let step = 180.0 / blades as f32;
    let groups = [f.wind_groups[0] as u8, f.wind_groups[1] as u8];
    let hw = f.half_width;
    for b in 0..blades {
        let first = m.positions.len() as u32;
        let mut sides: Vec<[f32; 3]> = Vec::with_capacity(n);
        for i in 0..n {
            let fr = if i < n - 1 { &f.nodes[i].frame } else { &f.nodes[i - 1].frame };
            let (s, c) = ((f.angle + b as f32 * step) * 0.017_453_292).sin_cos();
            let side = [c * fr[3] + fr[6] * s, c * fr[4] + fr[7] * s, c * fr[5] + fr[8] * s];
            sides.push(side);
            let p = f.nodes[i].pos;
            let towards = if i == 0 { f.nodes[1].pos } else { p };
            let p0 = f.nodes[0].pos;
            let along = normalize([towards[0] - p0[0], towards[1] - p0[1], towards[2] - p0[2]]);
            let (a, bb) = if i == n - 1 {
                let s = sides[i - 1];
                ([s[0] * hw + p[0], s[1] * hw + p[1], s[2] * hw + p[2]], [p[0] - s[0] * hw, p[1] - s[1] * hw, p[2] - s[2] * hw])
            } else if i < 1 {
                ([hw * side[0] + p[0], p[1] + hw * side[1], hw * side[2] + p[2]], [p[0] - hw * side[0], p[1] - hw * side[1], p[2] - hw * side[2]])
            } else {
                let s = sides[i - 1];
                (
                    [
                        (s[0] * hw + p[0] + side[0] * hw + p[0]) * 0.5,
                        (s[1] * hw + p[1] + side[1] * hw + p[1]) * 0.5,
                        (s[2] * hw + p[2] + side[2] * hw + p[2]) * 0.5,
                    ],
                    [
                        ((p[0] - s[0] * hw) + (p[0] - side[0] * hw)) * 0.5,
                        ((p[1] - s[1] * hw) + (p[1] - side[1] * hw)) * 0.5,
                        ((p[2] - s[2] * hw) + (p[2] - side[2] * hw)) * 0.5,
                    ],
                )
            };
            // binormal: −normalize(along × side), or −along when that is (near) zero
            let cx = [
                along[1] * side[2] - along[2] * side[1],
                along[2] * side[0] - along[0] * side[2],
                along[0] * side[1] - along[1] * side[0],
            ];
            let l2 = cx[1] * cx[1] + cx[0] * cx[0] + cx[2] * cx[2];
            let bin = if 0.1 <= approx_sqrt(l2) { normalize(cx) } else { along };
            let bin = [-bin[0], -bin[1], -bin[2]];
            let v = i as f32 / (n as f32 - 1.0);
            let node: &BranchNode = &f.nodes[i];
            let ww = |w: f32| (1.0 - w).clamp(0.0, 1.0);
            for (pos, u) in [(a, 1.0f32), (bb, 0.0)] {
                m.positions.push(out(pos));
                m.normals.push(out(along));
                m.tangents.push(out(side));
                m.binormals.push(out(bin));
                m.uvs.push([u, v]);
                m.textures.push(f.texture as u8);
                m.wind_weights.push([ww(node.wind[0]), ww(node.wind[1])]);
                m.wind_groups.push(groups);
            }
        }
        // V by arc length along each edge (× 0.999), middle nodes only.
        let base = first as usize;
        let (mut la, mut lb) = (0.0f32, 0.0f32);
        let mut cum = Vec::with_capacity(n);
        for i in 0..n - 1 {
            la += dist(m.positions[base + 2 * i], m.positions[base + 2 * i + 2]);
            lb += dist(m.positions[base + 2 * i + 1], m.positions[base + 2 * i + 3]);
            cum.push((la, lb));
        }
        let (ia, ib) = (1.0 / la, 1.0 / lb);
        for i in 1..n - 1 {
            m.uvs[base + 2 * i][1] = cum[i - 1].0 * ia * 0.999;
            m.uvs[base + 2 * i + 1][1] = cum[i - 1].1 * ib * 0.999;
        }
        m.strips.push((first..first + 2 * n as u32).collect());
    }
}

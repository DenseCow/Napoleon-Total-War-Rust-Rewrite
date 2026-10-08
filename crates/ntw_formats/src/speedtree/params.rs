//! The `.spt` values as SpeedTreeRT's generator uses them: named, with the exe's defaults filled
//! in where a token is absent. Meanings come from how `CBranch::Compute` (`FUN_012d45a0`) and its
//! helpers use each value (`analysis/speedtree/SPEEDTREE.md` §5); names marked INFERRED are our
//! best reading of that use, the arithmetic itself is CONFIRMED.

use super::spline::BezierSpline;
use super::spt::{BranchLevel, SptFile, SptValue};

/// One of the 7 texture-coordinate layers of a branch level (record of 0x30 bytes at level
/// +0x14; defaults from `FUN_012da390`, CONFIRMED). Layer 0 is the bark diffuse.
#[derive(Debug, Clone, PartialEq)]
pub struct TexLayer {
    /// 6013 / 50004: U repeat around the branch.
    pub u_scale: f32,
    /// 6014 / 50005: V repeat along the branch.
    pub v_scale: f32,
    /// 6015 / 50006: U is absolute (not × circumference). Default true.
    pub u_absolute: bool,
    /// 6016 / 50007: V is absolute (not × length / size).
    pub v_absolute: bool,
    /// 15002 / 50009: add the branch's random V offset.
    pub random_offset: bool,
    /// 15003 / 50008: U twist along the branch.
    pub twist: f32,
    /// 50017: U offset.
    pub u_offset: f32,
    /// 50010: V offset.
    pub v_offset: f32,
    /// 50011: U is a segment of an atlas (`u_segment` range by angle).
    pub segmented: bool,
    /// 50012: V is clamped to 0..1 and mapped to `v_range`.
    pub clamp_v: bool,
    /// 50013, 50014: the atlas U range.
    pub u_segment: [f32; 2],
    /// 50015, 50016: the V range when clamped.
    pub v_range: [f32; 2],
    /// 50018 (UNKNOWN use), default true.
    pub flag_50018: bool,
}

impl Default for TexLayer {
    fn default() -> Self {
        Self {
            u_scale: 1.0,
            v_scale: 1.0,
            u_absolute: true,
            v_absolute: false,
            random_offset: false,
            twist: 0.0,
            u_offset: 0.0,
            v_offset: 0.0,
            segmented: false,
            clamp_v: false,
            u_segment: [0.0, 1.0],
            v_range: [0.0, 1.0],
            flag_50018: true,
        }
    }
}

/// Flare settings of a level (16002..16012, `FUN_012d7550`). Defaults CONFIRMED (`FUN_012da1b0`).
#[derive(Debug, Clone, PartialEq)]
pub struct Flares {
    /// 16003: number of flares (0 = none).
    pub count: i32,
    /// 16004: angular spread (fraction of the even spacing that is fixed).
    pub spacing: f32,
    /// 16005, 16006: angular width in degrees, ± variance.
    pub width: [f32; 2],
    /// 16007: exponent of the angular falloff.
    pub width_exponent: f32,
    /// 16008, 16009: strength (radius added), ± variance.
    pub strength: [f32; 2],
    /// 16010, 16011: length along the branch (0..1), ± variance.
    pub length: [f32; 2],
    /// 16012: exponent of the falloff along the branch.
    pub length_exponent: f32,
}

impl Default for Flares {
    fn default() -> Self {
        Self {
            count: 0,
            spacing: 1.0,
            width: [30.0, 10.0],
            width_exponent: 1.0,
            strength: [0.5, 0.25],
            length: [0.3, 0.1],
            length_exponent: 1.0,
        }
    }
}

/// A branch level with every value the generator reads.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelParams {
    /// 6000: disturbance (only its variance is used, in degrees).
    pub disturbance: BezierSpline,
    /// 6001: gravity strength.
    pub gravity: BezierSpline,
    /// 6002: flexibility (wind weight).
    pub flexibility: BezierSpline,
    /// 6003: flexibility profile along the branch.
    pub flexibility_profile: BezierSpline,
    /// 6004: length (× tree size); on the leaf level, the leaf distance.
    pub length: BezierSpline,
    /// 6005: radius (× tree size).
    pub radius: BezierSpline,
    /// 6006: radius profile along the branch.
    pub radius_profile: BezierSpline,
    /// 6007: start angle (degrees) from the parent.
    pub start_angle: BezierSpline,
    /// 6017: angle profile (gravity curvature along the branch, centred on 0.5).
    pub angle_profile: BezierSpline,
    /// 6008: sides of the tube (default 6).
    pub cross_sections: i32,
    /// 6009: segments along the branch (default 3).
    pub segments: i32,
    /// 6010, 6011: where children start/end along this level's parent (0..1).
    pub child_range: [f32; 2],
    /// 6012: children per unit length (× tree size).
    pub frequency: f32,
    /// Texture layers.
    pub layers: [TexLayer; 7],
    /// 16002: segment distribution exponent (`pow(i / (n − 1), e)`), default 1.
    pub segment_exponent: f32,
    /// 16003..16012.
    pub flares: Flares,
    /// 23002, 23003 (light seam reduction), default 1, 1.
    pub light_seam: [f32; 2],
    /// 26004: bark roughness.
    pub roughness: f32,
    /// 26005: segment reduction for short branches (1 = none).
    pub segment_reduction: f32,
    /// 26006: cross-section reduction for thin branches (1 = none).
    pub cross_section_reduction: f32,
    /// 26007: prune threshold on an ancestor's position (0 = none; negative = below).
    pub prune: f32,
    /// 26008: ancestor depth for the prune test.
    pub prune_depth: i32,
    /// 26009: snap children to the parent's segment joints.
    pub snap: bool,
    /// 26010: snap probability (default 1).
    pub snap_probability: f32,
    /// 26011: minimum joint angle (degrees) to snap to.
    pub snap_min_angle: f32,
    /// 26012: most children per joint (default 1).
    pub snap_max: i32,
    /// 26013: cross-section count profile along the branch (0..1 between 3 and the full count).
    pub cross_section_profile: BezierSpline,
    /// 26014: normal smoothing profile along the branch.
    pub normal_profile: BezierSpline,
    /// 26015, 26016: roughness frequencies (along, around), defaults 30, 5.
    pub roughness_frequency: [f32; 2],
    /// 26017: random roughness.
    pub roughness_random: f32,
    /// 26018: roughness profile along the branch.
    pub roughness_profile: BezierSpline,
    /// 26019: child probability by position.
    pub child_probability: BezierSpline,
    /// 26020: normal smoothing blend by position on the parent.
    pub normal_blend: BezierSpline,
    /// 26021: twist (degrees).
    pub twist: f32,
    /// 26022: twist profile along the branch.
    pub twist_profile: BezierSpline,
    /// 26023: do not alternate the twist direction.
    pub twist_no_alternate: bool,
}

fn f32_of(v: &SptValue) -> f32 {
    match v {
        SptValue::F32(f) => *f,
        SptValue::I32(i) => *i as f32,
        _ => 0.0,
    }
}

fn i32_of(v: &SptValue) -> i32 {
    match v {
        SptValue::I32(i) => *i,
        SptValue::F32(f) => *f as i32,
        _ => 0,
    }
}

fn bool_of(v: &SptValue) -> bool {
    matches!(v, SptValue::Bool(true))
}

impl LevelParams {
    /// Resolves a parsed level (with its supplemental sections) and its 50000 layers.
    pub fn from_level(l: &BranchLevel, layers50000: Option<&[Vec<(u32, SptValue)>; 7]>) -> Self {
        let s = &l.supplemental;
        let mut layers: [TexLayer; 7] = Default::default();
        layers[0].u_scale = l.t6013;
        layers[0].v_scale = l.t6014;
        layers[0].u_absolute = l.t6015;
        layers[0].v_absolute = l.t6016;
        if let Some(b) = s.t15002 {
            layers[0].random_offset = b;
        }
        if let Some(f) = s.t15003 {
            layers[0].twist = f;
        }
        if let Some(all) = layers50000 {
            for (layer, vals) in layers.iter_mut().zip(all.iter()) {
                for (t, v) in vals {
                    match t {
                        50004 => layer.u_scale = f32_of(v),
                        50005 => layer.v_scale = f32_of(v),
                        50006 => layer.u_absolute = bool_of(v),
                        50007 => layer.v_absolute = bool_of(v),
                        50008 => layer.twist = f32_of(v),
                        50009 => layer.random_offset = bool_of(v),
                        50010 => layer.v_offset = f32_of(v),
                        50011 => layer.segmented = bool_of(v),
                        50012 => layer.clamp_v = bool_of(v),
                        50013 => layer.u_segment[0] = f32_of(v),
                        50014 => layer.u_segment[1] = f32_of(v),
                        50015 => layer.v_range[0] = f32_of(v),
                        50016 => layer.v_range[1] = f32_of(v),
                        50017 => layer.u_offset = f32_of(v),
                        50018 => layer.flag_50018 = bool_of(v),
                        _ => {}
                    }
                }
            }
        }
        let mut p = Self {
            disturbance: l.splines[0].clone(),
            gravity: l.splines[1].clone(),
            flexibility: l.splines[2].clone(),
            flexibility_profile: l.splines[3].clone(),
            length: l.splines[4].clone(),
            radius: l.splines[5].clone(),
            radius_profile: l.splines[6].clone(),
            start_angle: l.splines[7].clone(),
            angle_profile: l.spline_6017.clone(),
            cross_sections: l.cross_sections,
            segments: l.segments,
            child_range: [l.t6010, l.t6011],
            frequency: l.t6012,
            layers,
            segment_exponent: 1.0,
            flares: Flares::default(),
            light_seam: [1.0, 1.0],
            roughness: 0.0,
            segment_reduction: 1.0,
            cross_section_reduction: 1.0,
            prune: 0.0,
            prune_depth: 0,
            snap: false,
            snap_probability: 1.0,
            snap_min_angle: 0.0,
            snap_max: 1,
            cross_section_profile: BezierSpline::default(),
            normal_profile: BezierSpline::default(),
            roughness_frequency: [30.0, 5.0],
            roughness_random: 0.0,
            roughness_profile: BezierSpline::default(),
            child_probability: BezierSpline::default(),
            normal_blend: BezierSpline::default(),
            twist: 0.0,
            twist_profile: BezierSpline::default(),
            twist_no_alternate: false,
        };
        if let Some(f) = &s.flare {
            p.segment_exponent = f.t16002;
            p.flares = Flares {
                count: f.t16003,
                spacing: f.rest[0],
                width: [f.rest[1], f.rest[2]],
                width_exponent: f.rest[3],
                strength: [f.rest[4], f.rest[5]],
                length: [f.rest[6], f.rest[7]],
                length_exponent: f.rest[8],
            };
        }
        if let Some(ls) = s.light_seam {
            p.light_seam = ls;
        }
        p.apply_supplemental(&s.values);
        p
    }

    /// Applies 26004..26023 (and, for the root level's 40007 block, 15002..23003) values.
    pub fn apply_supplemental(&mut self, values: &[(u32, SptValue)]) {
        for (t, v) in values {
            let spline = || match v {
                SptValue::Spline(s) => (**s).clone(),
                _ => BezierSpline::default(),
            };
            match t {
                15002 => self.layers[0].random_offset = bool_of(v),
                15003 => self.layers[0].twist = f32_of(v),
                16002 => self.segment_exponent = f32_of(v),
                16003 => self.flares.count = i32_of(v),
                16004 => self.flares.spacing = f32_of(v),
                16005 => self.flares.width[0] = f32_of(v),
                16006 => self.flares.width[1] = f32_of(v),
                16007 => self.flares.width_exponent = f32_of(v),
                16008 => self.flares.strength[0] = f32_of(v),
                16009 => self.flares.strength[1] = f32_of(v),
                16010 => self.flares.length[0] = f32_of(v),
                16011 => self.flares.length[1] = f32_of(v),
                16012 => self.flares.length_exponent = f32_of(v),
                23002 => self.light_seam[0] = f32_of(v),
                23003 => self.light_seam[1] = f32_of(v),
                26004 => self.roughness = f32_of(v),
                26005 => self.segment_reduction = f32_of(v),
                26006 => self.cross_section_reduction = f32_of(v),
                26007 => self.prune = f32_of(v),
                26008 => self.prune_depth = i32_of(v),
                26009 => self.snap = bool_of(v),
                26010 => self.snap_probability = f32_of(v),
                26011 => self.snap_min_angle = f32_of(v),
                26012 => self.snap_max = i32_of(v),
                26013 => self.cross_section_profile = spline(),
                26014 => self.normal_profile = spline(),
                26015 => self.roughness_frequency[0] = f32_of(v),
                26016 => self.roughness_frequency[1] = f32_of(v),
                26017 => self.roughness_random = f32_of(v),
                26018 => self.roughness_profile = spline(),
                26019 => self.child_probability = spline(),
                26020 => self.normal_blend = spline(),
                26021 => self.twist = f32_of(v),
                26022 => self.twist_profile = spline(),
                26023 => self.twist_no_alternate = bool_of(v),
                _ => {}
            }
        }
    }
}

/// One leaf texture as the generator uses it (record 0x60 bytes in the leaf object).
#[derive(Debug, Clone, PartialEq)]
pub struct LeafTextureParams {
    /// 4000: blossom.
    pub blossom: bool,
    /// 4001, 4002: colour and its variance.
    pub color: [f32; 3],
    pub color_variance: f32,
    /// 4003: file name.
    pub texture: String,
    /// 4004: card origin (pivot) in card space (default 0.5, 1).
    pub origin: [f32; 2],
    /// 4005: card size relative to the tree size (default 0.12).
    pub size_fraction: [f32; 2],
    /// Absolute card size: `4005 × tree size` when the size is positive (CONFIRMED, tree
    /// `Compute`), else 4006 (default 10).
    pub size: [f32; 2],
    /// 72001/72003: drawn as leaf mesh number `n` of section 71000 instead of a card.
    pub mesh: Option<i32>,
    /// 72005: mesh leaves bend toward up by this amount.
    pub mesh_up_bend: f32,
    /// 72006: random roll (degrees, ±) of mesh leaves.
    pub mesh_roll: f32,
}

/// Everything `CTreeEngine::Compute` reads, resolved.
#[derive(Debug, Clone)]
pub struct TreeParams {
    /// 2005: random seed.
    pub seed: i32,
    /// 2006, 2007: tree size ± variance.
    pub size: f32,
    pub size_variance: f32,
    /// 74002: rotation of the whole tree about the vertical (degrees).
    pub rotation: f32,
    /// 16013: `srand` seed for the flares.
    pub flare_seed: i32,
    /// Branch levels, trunk first; the last one is the leaf level.
    pub levels: Vec<LevelParams>,
    /// 40006 + 40002..40005: the root level.
    pub root: LevelParams,
    /// 40002: the child level at which roots are added.
    pub root_level: i32,
    /// 40003, 40004: root start/end on their parent.
    pub root_range: [f32; 2],
    /// 40005: roots per unit length.
    pub root_frequency: f32,
    /// 11002: the wind level.
    pub wind_level: i32,
    /// 22000: wind weights fall off past the wind level.
    pub wind_falloff: bool,
    /// 27002..27006: floor (roots bend onto the ground plane).
    pub floor_enabled: bool,
    pub floor_height: f32,
    pub floor_min_level: i32,
    pub floor_exponent: f32,
    pub floor_strength: f32,
    /// 29002: levels below this make no geometry (clusters).
    pub cluster_level: i32,
    /// 13002: first level that may be fronds; 13007: fronds enabled.
    pub frond_start_level: i32,
    pub fronds_enabled: bool,
    /// 25002..25005: frond-or-branch rule; 25006/25007 frond segment override.
    pub frond_rule: (f32, i32, i32, i32),
    pub frond_segments: (i32, bool),
    /// Frond textures (14003..14006 each).
    pub frond_textures: Vec<[f32; 4]>,
    /// 13003: frond type (0 = blades, 1 = extrusion); 13004: blades per frond; 13006: extrusion segments;
    /// 13005: extrusion profile.
    pub frond_type: i32,
    pub frond_blades: i32,
    pub frond_profile_segments: i32,
    pub frond_profile: super::spline::BezierSpline,
    /// Leaf textures.
    pub leaf_textures: Vec<LeafTextureParams>,
    /// 3000..3002: blossom rule (position threshold, ancestor depth, probability).
    pub blossom_rule: (f32, i32, f32),
    /// 3007: leaf spacing factor; 3008: spacing mode (0 none, 1 whole tree, 2 per branch).
    pub leaf_spacing: (f32, i32),
    /// 3009, 3010: leaf ambient-by-position switch and amount.
    pub leaf_dimming: (bool, f32),
    /// 9007, 9008, 9012, 9013, 9014: branch LOD count and parameters.
    pub branch_lods: (i32, f32, f32, f32, f32),
    /// 71000: the leaf meshes.
    pub leaf_meshes: Vec<super::spt::SptMesh>,
}

impl TreeParams {
    /// Resolves a parsed `.spt`.
    pub fn from_spt(f: &SptFile) -> Self {
        let x = &f.extra;
        let layers = |i: usize| x.texcoord_controls.as_ref().and_then(|v| v.get(i));
        let levels: Vec<LevelParams> =
            f.tree.branch_levels.iter().enumerate().map(|(i, l)| LevelParams::from_level(l, layers(i))).collect();
        let root_layers = layers(f.tree.branch_levels.len());
        let root_level = x.root.as_ref().and_then(|r| r.level.as_ref());
        let root = root_level.map(|l| LevelParams::from_level(l, root_layers)).unwrap_or_else(|| {
            LevelParams::from_level(&BranchLevel::default(), root_layers)
        });
        let rv = |t: u32| x.root.as_ref().and_then(|r| r.values.iter().find(|(k, _)| *k == t)).map(|(_, v)| v.clone());
        let fr = |t: u32| x.fronds.as_ref().and_then(|r| r.values.iter().find(|(k, _)| *k == t)).map(|(_, v)| v.clone());
        let fs = |t: u32| x.frond_supplement.as_ref().and_then(|r| r.iter().find(|(k, _)| *k == t)).map(|(_, v)| v.clone());
        let fl = |t: u32| x.floor.as_ref().and_then(|r| r.iter().find(|(k, _)| *k == t)).map(|(_, v)| v.clone());
        let lod = |t: u32| x.lod.as_ref().and_then(|r| r.iter().find(|(k, _)| *k == t)).map(|(_, v)| v.clone());
        let size = f.tree.t2006;
        let leaf_textures = f
            .leaves
            .textures
            .iter()
            .enumerate()
            .map(|(i, t)| {
                // 72000 block i (FUN_012cae70): 72001 mesh flag (+0x50), 72003 mesh index (+0x54),
                // 72005 bend toward up (+0x58), 72006 random roll amount (+0x5c); 72002 skipped.
                let lm = x.leaf_meshes.as_ref().and_then(|v| v.get(i));
                let lv = |tok: u32| lm.and_then(|v| v.iter().find(|(k, _)| *k == tok)).map(|(_, v)| v.clone());
                let frac = [t.t4005[0], t.t4005[1]];
                let size_abs = if size > 0.0 { [frac[0] * size, frac[1] * size] } else { [t.t4006[0], t.t4006[1]] };
                LeafTextureParams {
                    blossom: t.t4000,
                    color: t.t4001,
                    color_variance: t.t4002,
                    texture: t.texture.clone(),
                    origin: [t.t4004[0], t.t4004[1]],
                    size_fraction: frac,
                    size: size_abs,
                    mesh: lv(72001).is_some_and(|v| bool_of(&v)).then(|| lv(72003).map(|v| i32_of(&v)).unwrap_or(0)),
                    mesh_up_bend: lv(72005).map(|v| f32_of(&v)).unwrap_or(0.0),
                    mesh_roll: lv(72006).map(|v| f32_of(&v)).unwrap_or(0.0),
                }
            })
            .collect();
        Self {
            seed: f.tree.seed.unwrap_or(0),
            size,
            size_variance: f.tree.t2007,
            rotation: x.global_supplement.as_ref().and_then(|v| v.first().copied()).unwrap_or(0.0),
            flare_seed: x.t16013.unwrap_or(0),
            levels,
            root,
            root_level: rv(40002).map(|v| i32_of(&v)).unwrap_or(0),
            root_range: [rv(40003).map(|v| f32_of(&v)).unwrap_or(0.0), rv(40004).map(|v| f32_of(&v)).unwrap_or(0.0)],
            root_frequency: rv(40005).map(|v| f32_of(&v)).unwrap_or(0.0),
            wind_level: x.new_wind.as_ref().and_then(|v| v.first().copied()).unwrap_or(0),
            wind_falloff: x.t22000.unwrap_or(false),
            floor_enabled: fl(27002).map(|v| bool_of(&v)).unwrap_or(false),
            floor_height: fl(27003).map(|v| f32_of(&v)).unwrap_or(0.0),
            floor_min_level: fl(27004).map(|v| i32_of(&v)).unwrap_or(0),
            floor_exponent: fl(27005).map(|v| f32_of(&v)).unwrap_or(1.0),
            floor_strength: fl(27006).map(|v| f32_of(&v)).unwrap_or(1.0),
            cluster_level: x.cluster.as_ref().and_then(|v| v.first().copied()).unwrap_or(0),
            frond_start_level: fr(13002).map(|v| i32_of(&v)).unwrap_or(i32::MAX),
            fronds_enabled: fr(13007).map(|v| bool_of(&v)).unwrap_or(false),
            frond_rule: (
                fs(25002).map(|v| f32_of(&v)).unwrap_or(0.0),
                fs(25003).map(|v| i32_of(&v)).unwrap_or(0),
                fs(25004).map(|v| i32_of(&v)).unwrap_or(0),
                fs(25005).map(|v| i32_of(&v)).unwrap_or(0),
            ),
            frond_segments: (fs(25006).map(|v| i32_of(&v)).unwrap_or(1), fs(25007).map(|v| bool_of(&v)).unwrap_or(false)),
            frond_textures: x.fronds.as_ref().map(|r| r.textures.iter().map(|t| t.values).collect()).unwrap_or_default(),
            frond_type: fr(13003).map(|v| i32_of(&v)).unwrap_or(0),
            frond_blades: fr(13004).map(|v| i32_of(&v)).unwrap_or(1),
            frond_profile_segments: fr(13006).map(|v| i32_of(&v)).unwrap_or(1),
            frond_profile: x.fronds.as_ref().and_then(|r| r.spline.clone()).unwrap_or_default(),
            leaf_textures,
            blossom_rule: (f.leaves.t3000, f.leaves.t3001, f.leaves.t3002),
            leaf_spacing: (f.leaves.t3007.unwrap_or(-1.0), f.leaves.t3008),
            leaf_dimming: (f.leaves.t3009, f.leaves.t3010),
            leaf_meshes: x.meshes.as_ref().map(|m| m.meshes.clone()).unwrap_or_default(),
            branch_lods: (
                lod(9007).map(|v| i32_of(&v)).unwrap_or(1),
                lod(9008).map(|v| f32_of(&v)).unwrap_or(1.0),
                lod(9012).map(|v| f32_of(&v)).unwrap_or(1.0),
                lod(9013).map(|v| f32_of(&v)).unwrap_or(0.0),
                lod(9014).map(|v| f32_of(&v)).unwrap_or(0.0),
            ),
        }
    }
}

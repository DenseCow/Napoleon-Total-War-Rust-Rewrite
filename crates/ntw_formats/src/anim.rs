//! `.anim` skeletal animations (data.pack `animations\...`, 3,814 files).
//!
//! The file carries its own skeleton (bone names + parent indices), so it is also the
//! skeleton format: soldier part meshes (`unit_variant::VariantPartMesh`) store their
//! vertices relative to bones of this skeleton, by index.
//!
//! Layout (all little-endian), reverse-engineered from the shipped files
//! (`examples/variant_probe.rs`, `tests/real_install.rs::every_anim_parses`):
//! ```text
//! f32  frame_rate        20.0 in the files checked                       (INFERRED meaning)
//! f32  duration          seconds; (frame_count - 1) / frame_rate         (INFERRED, see tests)
//! u32  bone_count        41 for the standard soldier skeleton
//! bone_count x { u16 n; [u16; n] UTF-16 name; i32 parent (-1 = root) }   (CONFIRMED)
//! u32  frame_count
//! frame_count x bone_count x KEY (40 bytes):                              (CONFIRMED size)
//!      f32 x3 translation    relative to the parent bone (model space for roots)
//!      f32 x4 rotation       quaternion x, y, z, w, relative to the parent
//!      f32 x3 unknown        0.001 in every bone of frame 0; small values later
//!                            (UNKNOWN: maybe per-frame deltas / velocities)
//! u32  event_count                                                        (CONFIRMED)
//! event_count x { u32 n; n x (u16 len + UTF-16 string) }   e.g. IMPACT_TIME "0.25"
//! (end of file)
//! ```
//! Space: Y is up (the `Hips` root sits ~1.05 above the origin). Same left-handed D3D
//! space as the rigid models; a renderer negates Z to show it in a right-handed engine.

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// Size in bytes of one per-bone key.
pub const ANIM_KEY_SIZE: usize = 40;

/// A parsed `.anim` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Anim {
    /// INFERRED: frames per second (20.0 in the soldier files checked).
    pub frame_rate: f32,
    /// INFERRED: clip length in seconds.
    pub duration: f32,
    /// The skeleton this clip animates.
    pub bones: Vec<AnimBone>,
    /// `frames[f][b]` is the key of bone `b` in frame `f`.
    pub frames: Vec<Vec<AnimKey>>,
    /// Named events after the frames, each a list of strings: a name followed by
    /// decimal-text arguments, e.g. `["IMPACT_TIME", "0.25"]` or
    /// `["FIRE_POSITION_RIGHT", "0.24", "0.14", "-0.72", "0.70"]`. Meaning of the
    /// arguments per event is INFERRED from the names (times in seconds, positions).
    pub events: Vec<Vec<String>>,
}

/// One skeleton bone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimBone {
    pub name: String,
    /// Index of the parent bone, or `None` for a root.
    pub parent: Option<usize>,
}

/// One bone's local transform in one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimKey {
    /// Translation relative to the parent bone.
    pub translation: [f32; 3],
    /// Rotation relative to the parent, quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// UNKNOWN three floats (0.001 each in frame 0).
    pub unknown: [f32; 3],
}

/// Why an `.anim` could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimError {
    UnexpectedEof { offset: usize, needed: usize },
    InvalidUtf16 { offset: usize },
    /// A parent index that is not an earlier bone (or -1).
    BadParent { bone: usize, parent: i32 },
    /// Counts too large for the file.
    TooLarge,
    /// Bytes left after the event list.
    TrailingBytes { offset: usize, count: usize },
}

impl From<ReadError> for AnimError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for AnimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEof { offset, needed } => {
                write!(f, "unexpected end of data at {offset} (needed {needed})")
            }
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 at {offset}"),
            Self::BadParent { bone, parent } => write!(f, "bone {bone} has bad parent {parent}"),
            Self::TooLarge => write!(f, "frame/bone counts exceed the file size"),
            Self::TrailingBytes { offset, count } => write!(f, "{count} trailing bytes at {offset}"),
        }
    }
}

impl std::error::Error for AnimError {}

impl Anim {
    /// Parses a complete `.anim` file: the current layout (40-byte keys), else the older one of the
    /// 31 `testdata/animations` clips, whose keys are 28 bytes (translation and rotation only;
    /// CONFIRMED: all 31 read to the end that way). Older keys get `unknown = [0; 3]`.
    pub fn read(bytes: &[u8]) -> Result<Self, AnimError> {
        Self::read_with(bytes, 10, true)
            .or_else(|e| Self::read_with(bytes, 7, true).map_err(|_| e))
            .or_else(|e| Self::read_with(bytes, 7, false).map_err(|_| e))
    }

    /// The layout with `key_floats` floats per key (10 current, 7 old); `named = false` is the
    /// oldest layout (`testdata\test.anim`, `ranger_test_lod1`, `musketman_lod1_anim`): bones are
    /// only parent indices (no names) and there is no event list (CONFIRMED: those 4 files read to
    /// the end that way).
    fn read_with(bytes: &[u8], key_floats: usize, named: bool) -> Result<Self, AnimError> {
        let mut c = Cursor::new(bytes);
        let frame_rate = c.f32()?;
        let duration = c.f32()?;
        let bone_count = c.u32()? as usize;
        // Each bone record is at least 6 bytes.
        if bone_count > c.remaining() / 6 {
            return Err(AnimError::TooLarge);
        }
        let mut bones = Vec::with_capacity(bone_count);
        for bone in 0..bone_count {
            let name = if named { c.utf16()? } else { String::new() };
            let parent = c.u32()? as i32;
            let parent = match parent {
                -1 => None,
                p if p >= 0 && (p as usize) < bone => Some(p as usize),
                p => return Err(AnimError::BadParent { bone, parent: p }),
            };
            bones.push(AnimBone { name, parent });
        }
        let frame_count = c.u32()? as usize;
        let frame_bytes = bone_count.checked_mul(key_floats * 4).ok_or(AnimError::TooLarge)?;
        if frame_count.checked_mul(frame_bytes).is_none_or(|n| n > c.remaining()) {
            return Err(AnimError::TooLarge);
        }
        let mut frames = Vec::with_capacity(frame_count);
        for _ in 0..frame_count {
            let mut keys = Vec::with_capacity(bone_count);
            for _ in 0..bone_count {
                let mut v = [0f32; 10];
                for x in &mut v[..key_floats] {
                    *x = c.f32()?;
                }
                keys.push(AnimKey {
                    translation: [v[0], v[1], v[2]],
                    rotation: [v[3], v[4], v[5], v[6]],
                    unknown: [v[7], v[8], v[9]],
                });
            }
            frames.push(keys);
        }
        let event_count = if named { c.u32()? as usize } else { 0 };
        if event_count > c.remaining() / 4 {
            return Err(AnimError::TooLarge);
        }
        let mut events = Vec::with_capacity(event_count);
        for _ in 0..event_count {
            let n = c.u32()? as usize;
            if n > c.remaining() / 2 {
                return Err(AnimError::TooLarge);
            }
            let mut fields = Vec::with_capacity(n);
            for _ in 0..n {
                fields.push(c.utf16()?);
            }
            events.push(fields);
        }
        if c.remaining() != 0 {
            return Err(AnimError::TrailingBytes { offset: c.pos(), count: c.remaining() });
        }
        Ok(Self { frame_rate, duration, bones, frames, events })
    }

    /// Index of the bone with this name (case-insensitive).
    pub fn bone_index(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// Ground speed of the clip in m/s: the horizontal (x, z) distance the first root bone
    /// travels from the first to the last frame, over the clip's duration. Locomotion clips
    /// move their roots forward (`mus_t_walk_127` gives 1.269, matching its name;
    /// `horse_gallop` 10.19); stand clips give 0. CONFIRMED on the clips named by speed.
    pub fn root_speed(&self) -> f32 {
        let root = self.bones.iter().position(|b| b.parent.is_none()).unwrap_or(0);
        let (Some(first), Some(last)) = (self.frames.first(), self.frames.last()) else { return 0.0 };
        let (Some(a), Some(b)) = (first.get(root), last.get(root)) else { return 0.0 };
        let (dx, dz) = (b.translation[0] - a.translation[0], b.translation[2] - a.translation[2]);
        if self.duration > 0.0 { (dx * dx + dz * dz).sqrt() / self.duration } else { 0.0 }
    }

    /// Model-space bone matrices for one frame (column-major 4x4, `world = parent * local`).
    /// Frames outside the clip are clamped. Returns an empty list for a clip without frames.
    pub fn world_matrices(&self, frame: usize) -> Vec<[f32; 16]> {
        let Some(keys) = self.frames.get(frame.min(self.frames.len().saturating_sub(1))) else {
            return Vec::new();
        };
        let mut out: Vec<[f32; 16]> = Vec::with_capacity(keys.len());
        for (b, key) in keys.iter().enumerate() {
            let local = key.matrix();
            let world = match self.bones[b].parent {
                Some(p) => mat_mul(&out[p], &local),
                None => local,
            };
            out.push(world);
        }
        out
    }

    /// Like [`Self::world_matrices`] but blends two neighbouring frames (`time` in seconds,
    /// looping). Rotations are nlerped (PROVISIONAL: the game's interpolation is UNKNOWN).
    pub fn world_matrices_at(&self, time: f32) -> Vec<[f32; 16]> {
        let n = self.frames.len();
        if n == 0 {
            return Vec::new();
        }
        let rate = if self.frame_rate > 0.0 { self.frame_rate } else { 20.0 };
        let f = (time * rate).rem_euclid(n as f32);
        let (a, t) = (f.floor() as usize % n, f.fract());
        let b = (a + 1) % n;
        let mut out: Vec<[f32; 16]> = Vec::with_capacity(self.bones.len());
        for (i, (ka, kb)) in self.frames[a].iter().zip(&self.frames[b]).enumerate() {
            let local = ka.lerp(kb, t).matrix();
            let world = match self.bones[i].parent {
                Some(p) => mat_mul(&out[p], &local),
                None => local,
            };
            out.push(world);
        }
        out
    }
}

impl AnimKey {
    /// Local transform as a column-major 4x4 matrix (rotate, then translate).
    pub fn matrix(&self) -> [f32; 16] {
        let [x, y, z, w] = normalize4(self.rotation);
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz, wx, wy, wz) = (x * y, x * z, y * z, w * x, w * y, w * z);
        let t = self.translation;
        [
            1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy), 0.0,
            2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx), 0.0,
            2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy), 0.0,
            t[0], t[1], t[2], 1.0,
        ]
    }

    fn lerp(&self, other: &Self, t: f32) -> Self {
        let mut q = other.rotation;
        let dot: f32 = (0..4).map(|i| self.rotation[i] * q[i]).sum();
        if dot < 0.0 {
            q = q.map(|v| -v);
        }
        let l3 = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
        Self {
            translation: l3(self.translation, other.translation),
            rotation: normalize4([0, 1, 2, 3].map(|i| self.rotation[i] + (q[i] - self.rotation[i]) * t)),
            unknown: self.unknown,
        }
    }
}

fn normalize4(q: [f32; 4]) -> [f32; 4] {
    let len = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if len > 0.0 { q.map(|v| v / len) } else { [0.0, 0.0, 0.0, 1.0] }
}

/// Column-major 4x4 multiply `a * b`.
pub fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    out
}

/// Transforms a point by a column-major 4x4 matrix.
pub fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

/// Transforms a direction (no translation) by a column-major 4x4 matrix.
pub fn transform_vector(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::utf16_bytes;

    fn fixture() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&20f32.to_le_bytes());
        b.extend_from_slice(&0.05f32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend(utf16_bytes("Hips"));
        b.extend_from_slice(&(-1i32).to_le_bytes());
        b.extend(utf16_bytes("Spine"));
        b.extend_from_slice(&0i32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        for frame in 0..2 {
            for bone in 0..2 {
                // Hips at y=1, Spine 0.5 above along the parent's x axis.
                let t = if bone == 0 { [0.0, 1.0 + frame as f32, 0.0] } else { [0.5, 0.0, 0.0] };
                // Hips rotated +90 deg about z: x -> +y.
                let s = std::f32::consts::FRAC_1_SQRT_2;
                let q = if bone == 0 { [0.0, 0.0, s, s] } else { [0.0, 0.0, 0.0, 1.0] };
                for v in t.iter().chain(&q).chain(&[0.001f32; 3]) {
                    b.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        b.extend_from_slice(&[0; 4]);
        b
    }

    #[test]
    fn reads_skeleton_and_frames() {
        let a = Anim::read(&fixture()).unwrap();
        assert_eq!(a.bones.len(), 2);
        assert_eq!(a.bones[1].parent, Some(0));
        assert_eq!(a.bone_index("spine"), Some(1));
        assert_eq!(a.frames.len(), 2);
        assert!(a.events.is_empty());
        let w = a.world_matrices(0);
        let p = transform_point(&w[1], [0.0; 3]);
        assert!((p[0]).abs() < 1e-5 && (p[1] - 1.5).abs() < 1e-5, "{p:?}");
        let mid = a.world_matrices_at(0.025);
        assert!((transform_point(&mid[0], [0.0; 3])[1] - 1.5).abs() < 1e-4);
    }

    #[test]
    fn errors_instead_of_panicking() {
        let b = fixture();
        for cut in 0..b.len() - 4 {
            let _ = Anim::read(&b[..cut]);
        }
        let mut bad = b.clone();
        // Spine's parent -> 5 (not an earlier bone).
        let pos = 12 + 2 + 8 + 4 + 2 + 10;
        bad[pos..pos + 4].copy_from_slice(&5i32.to_le_bytes());
        assert!(matches!(Anim::read(&bad), Err(AnimError::BadParent { .. })));
    }
}

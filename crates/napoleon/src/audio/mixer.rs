//! The original's sound mixing rules as pure functions: the exe's sound manager plus Miles 7
//! (`mss32.dll`). Spec and evidence: `analysis/fidelity/MIDDLEWARE_VERIFY.md` §1.3–§1.9 (all
//! CONFIRMED in our Ghidra copies unless a function says otherwise). Our own code from that spec.
//!
//! Units: volumes are linear gains (0..=1), distances are world metres, times are seconds.
//! The x87 rounding the original uses (round half to even) is reproduced with
//! [`f64::round_ties_even`].

/// The Miles "digital master volume level" the exe sets (`SS_MILES_DIGITAL_MASTER_VOLUME_LEVEL`,
/// shipped 10).
pub const MILES_MASTER_LEVEL: f32 = 10.0;
/// Miles' pan law exponent (`(1 - p)^0.3`, `p^0.3`).
#[allow(dead_code)] // used by `miles_pan` (non-centre pans; the exe always pans 0.5)
const PAN_EXPONENT: f32 = 0.3;
/// `0.5^0.3`: both sides at the centre pan the exe always uses.
pub const CENTRE_PAN: f32 = 0.812_252_2;
/// Miles' volume curve exponent (`v^1.6666666`).
const VOLUME_EXPONENT: f32 = 1.666_666_6;
/// Q11 unity in the Miles mixer.
const Q11_ONE: f64 = 2048.0;

/// The six volume groups of the exe's sound manager (`0x010009E0`), in index order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolumeGroup {
    Music = 0,
    Sfx = 1,
    Speech = 2,
    Interface = 3,
    Movie = 4,
    Master = 5,
}

impl VolumeGroup {
    /// The group an event names in its `group` parameter (param 10); out-of-range values fall back
    /// to sfx (INFERRED: the shipped data only has 0, 1, 3).
    pub fn from_param(v: f32) -> Self {
        match v as i32 {
            0 => Self::Music,
            2 => Self::Speech,
            3 => Self::Interface,
            4 => Self::Movie,
            5 => Self::Master,
            _ => Self::Sfx,
        }
    }
}

/// One group's state: enabled flag and an integer volume 0..=100 (CONFIRMED representation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupVolume {
    pub enabled: bool,
    pub volume: i32,
}

impl Default for GroupVolume {
    fn default() -> Self {
        Self { enabled: true, volume: 100 }
    }
}

/// The exe's manager gain (§1.3 step 1): 0 when the master group or the event's group is
/// disabled, else `clamp01(vol_master × vol_group × 0.0001 × event_volume × speaker_mult ×
/// dim_mult)` where `dim_mult` is `SS_2D_VOLUME_MULTIPLIER` or `SS_3D_VOLUME_MULTIPLIER`.
pub fn manager_gain(master: GroupVolume, group: GroupVolume, event_volume: f32, speaker_mult: f32, dim_mult: f32) -> f32 {
    if !master.enabled || !group.enabled {
        return 0.0;
    }
    let g = master.volume as f32 * group.volume as f32 * 0.0001 * event_volume * speaker_mult * dim_mult;
    g.clamp(0.0, 1.0)
}

/// Miles' volume curve: `v^1.6666666` (§1.3 step 3).
pub fn miles_curve(v: f32) -> f32 {
    v.clamp(0.0, 1.0).powf(VOLUME_EXPONENT)
}

/// Miles' stereo pan of a (curved) volume: pan 0.5 gives `0.5^0.3` on both sides; other pans
/// `((1-p)^0.3 × v, p^0.3 × v)` (§1.3 step 3; stereo output).
#[allow(dead_code)] // the exe always uses pan 0.5; kept for the spec and tests
pub fn miles_pan(v_curved: f32, pan: f32) -> (f32, f32) {
    if pan == 0.5 {
        return (v_curved * CENTRE_PAN, v_curved * CENTRE_PAN);
    }
    let p = pan.clamp(0.0, 1.0);
    ((1.0 - p).powf(PAN_EXPONENT) * v_curved, p.powf(PAN_EXPONENT) * v_curved)
}

/// The Miles mixer's per-channel gain: `x` (gain × spatial × master level × dry level) converted
/// to Q11 as `round(x × 2048 + 0.5)` and clamped to 0..=2048 (= 1.0) (§1.3 step 4). Returns the
/// quantised gain as a float.
pub fn channel_gain(x: f32) -> f32 {
    let q = (f64::from(x) * Q11_ONE + 0.5).round_ties_even().clamp(0.0, Q11_ONE);
    (q / Q11_ONE) as f32
}

/// The per-side output gain of a 2D voice whose sound-manager volume (after fades) is `v`:
/// `channel_gain(master_level × 0.5^0.3 × v^(5/3))`.
pub fn voice_2d_gain(v: f32) -> f32 {
    channel_gain(MILES_MASTER_LEVEL * CENTRE_PAN * miles_curve(v))
}

/// Miles' default distance falloff (§1.4): gain 1 within `min' + 0.0001` (`min' = max(min,
/// 0.0001)`), else `min' / ((d − min') × rolloff + min')`.
pub fn default_falloff(d: f32, min: f32, rolloff: f32) -> f32 {
    let m = min.max(0.0001);
    if d <= m + 0.0001 {
        1.0
    } else {
        m / ((d - m) * rolloff + m)
    }
}

/// The exe's own falloff callback (`0x01002390`), used when an event's `falloff` differs from the
/// global rolloff: the same law with `rolloff = trunc(falloff × 100) / 100`, and gain 1 unless
/// `d > 1` and `min > 0.01`.
pub fn event_falloff(d: f32, min: f32, falloff: f32) -> f32 {
    if !(d > 1.0 && min > 0.01) {
        return 1.0;
    }
    let rolloff = (falloff * 100.0).trunc() / 100.0;
    default_falloff(d, min, rolloff)
}

/// The distance gain of a 3D voice (§1.4): the event callback when the event's falloff differs
/// from the global rolloff, else Miles' default; `d` is clamped to `max` as Miles does (the caller
/// stops voices past `max`, see [`past_max`]).
pub fn distance_gain(d: f32, min: f32, max: f32, event_falloff_value: f32, global_rolloff: f32) -> f32 {
    let d = if max > 0.0 { d.min(max) } else { d };
    if event_falloff_value != global_rolloff {
        event_falloff(d, min, event_falloff_value)
    } else {
        default_falloff(d, min, global_rolloff)
    }
}

/// The exe stops (or does not start) a 3D voice when `d² > max²` (§1.4).
pub fn past_max(d: f32, max: f32) -> bool {
    d * d > max * max
}

/// An event's min / max distances (§1.5): both × the recorded-distance multiplier `mult` (14 in
/// battle, 20 on the campaign map); a max ≤ 0 is estimated from the volume cutoff and rolloff as
/// `min × max(1, 1 − ln(cutoff × 0.01) / ln(rolloff + 1))` (`ln` INFERRED; the ratio does not
/// depend on the log base).
pub fn event_distances(min_dist: f32, max_dist: f32, mult: f32, cutoff: f32, rolloff: f32) -> (f32, f32) {
    let min = min_dist * mult;
    let mut max = max_dist * mult;
    if max <= 0.0 {
        let ratio = 1.0 - (cutoff * 0.01).ln() / (rolloff + 1.0).ln();
        max = min * ratio.max(1.0);
    }
    (min, max)
}

/// The speed-of-sound launch delay (§1.6): with `apply_relative` and `d > min_dist_to_apply`
/// (100): `(d − 100) / speed_of_sound`, and 0 when that is below `min_delay` (0.1 s).
pub fn launch_delay(d: f32, apply_relative: bool, min_dist_to_apply: f32, speed_of_sound: f32, min_delay: f32) -> f32 {
    if !apply_relative || d <= min_dist_to_apply || speed_of_sound <= 0.0 {
        return 0.0;
    }
    let t = (d - min_dist_to_apply) / speed_of_sound;
    if t < min_delay { 0.0 } else { t }
}

/// The random trigger delay (§1.6): `k × 0.01` s with `k` uniform in `[0, round(r × 100))`;
/// `u` is a uniform random number in `[0, 1)`.
pub fn random_trigger_delay(r: f32, u: f32) -> f32 {
    let n = (f64::from(r) * 100.0).round_ties_even();
    if n <= 0.0 {
        return 0.0;
    }
    let k = (f64::from(u.clamp(0.0, 0.999_999_9)) * n).floor();
    (k * 0.01) as f32
}

/// The low-pass cutoff of a 3D voice (§1.7), as a fraction of the sample's Nyquist: `x = mult ×
/// d²`, `f = slope × (x − 2.5) + 2.5`, cutoff `2.5 / f` (1 when `f ≤ 0`), clamped to
/// `[floor, 1]`. `slope` = `SS_LOW_PASS_FILTER × 0.001`, `floor` = `SS_LOW_PASS_FILTER_MIN`,
/// `mult` = the battle distance multiplier. Not used in campaign mode (the caller's job).
pub fn low_pass_cutoff(d: f32, mult: f32, slope: f32, floor: f32) -> f32 {
    let x = mult * d * d;
    let f = slope * (x - 2.5) + 2.5;
    let c = if f <= 0.0 { 1.0 } else { 2.5 / f };
    c.clamp(floor.min(1.0), 1.0)
}

/// Equal-power fade (`fade_type` 1, §1.8): the linear level through the exe's fast square root
/// (`bits → ((bits − 0x3F800000) >> 1) + 0x3F800000`).
pub fn equal_power_fade(linear: f32) -> f32 {
    let v = linear.clamp(0.0, 1.0);
    if v <= 0.0 {
        return 0.0;
    }
    let bits = v.to_bits() as i32;
    f32::from_bits((((bits - 0x3F80_0000) >> 1) + 0x3F80_0000) as u32)
}

/// A fade-out has ended when its level drops below this (§1.8).
pub const FADE_END: f32 = 0.001;

/// Miles' 3D panning on a stereo output (§1.9). `dir` is the source direction in listener space
/// (x right, y up, z forward), `d` its distance, `gain` the distance-attenuated voice gain.
/// Speakers FL (−√½, 0, √½), FR (√½, 0, √½) (INFERRED for stereo) plus Miles' virtual rear speaker
/// (0, 0, −1), whose share is split over the real speakers (÷ 2 per group, 3 groups). Each group
/// k gets `a_k = π − acos(dir·s_k)` and `sqrt(a_k³ / Σa)`; a channel is `clamp01(level / √3) ×
/// gain`. A source at the listener (`d < 0.0001`) gives `√½ × gain` on both channels.
pub fn stereo_3d_gains(dir: [f32; 3], d: f32, gain: f32) -> (f32, f32) {
    if d <= 0.0001 {
        let c = (0.5f32).sqrt() * gain;
        return (c, c);
    }
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let groups: [[f32; 3]; 3] = [[-h, 0.0, h], [h, 0.0, h], [0.0, 0.0, -1.0]];
    let mut angle = [0.0f32; 3];
    let mut sum = 0.0;
    for (k, s) in groups.iter().enumerate() {
        let dot = dir[0] * s[0] + dir[1] * s[1] + dir[2] * s[2];
        angle[k] = if dot > 0.9999 {
            std::f32::consts::PI
        } else if dot < -0.9999 {
            0.0
        } else {
            std::f32::consts::PI - dot.acos()
        };
        sum += angle[k];
    }
    let mut level = [0.0f32; 2];
    if sum > 0.0 {
        let share = |a: f32| (a.powi(3) / sum).sqrt();
        level[0] += share(angle[0]);
        level[1] += share(angle[1]);
        // The virtual rear speaker: its share goes to every real speaker (one per real group),
        // ÷ (2 × group count).
        let rear = share(angle[2]) / (2.0 * groups.len() as f32);
        for l in &mut level {
            *l += rear;
        }
    }
    let norm = 1.0 / (groups.len() as f32).sqrt();
    let ch = |l: f32| (l * norm).clamp(0.0, 1.0) * gain;
    (ch(level[0]), ch(level[1]))
}

/// Miles' doppler pitch factor (§1.9): `0.355 / (0.355 + v_radial × doppler × distance_factor)`
/// clamped to `[0.25, 4]` (4 when the sum is not positive).
#[allow(dead_code)] // PROVISIONAL: no velocities are set yet (the exe sets none by default)
pub fn doppler(v_radial: f32, doppler_factor: f32, distance_factor: f32) -> f32 {
    let x = v_radial * doppler_factor * distance_factor;
    if x > 0.355 {
        return 0.25;
    }
    if x < -0.355 {
        return 4.0;
    }
    (0.355 / (x + 0.355)).clamp(0.25, 4.0)
}

/// Miles' 2-pole Butterworth low-pass with Q13 coefficients (§1.7). Coefficients CONFIRMED
/// (`0x2112EB30`); the per-sample loop (direct form I) is INFERRED.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LowPass {
    /// `None` = filter off (pass-through).
    coef: Option<(i32, i32, i32)>,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Default for LowPass {
    fn default() -> Self {
        Self { coef: None, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
}

impl LowPass {
    /// The Q13 coefficients `(A, B1, B2)` for a cutoff (fraction of the sample's Nyquist) at a
    /// sample playback rate and output rate; `None` = off (cutoff ≥ 0.999 or < 0).
    pub fn coefficients(cutoff: f32, playback_rate: f32, output_rate: f32) -> Option<(i32, i32, i32)> {
        if !(0.0..0.999).contains(&cutoff) {
            return None;
        }
        let mut wc = f64::from(playback_rate) * f64::from(cutoff) / f64::from(output_rate.max(1.0));
        if wc > 0.98 {
            wc = 0.98;
        } else if wc < 0.0001 {
            wc = 0.0;
        }
        if wc < 0.001 {
            return Some((0, 0, 0));
        }
        let s2 = std::f64::consts::SQRT_2;
        let c = 1.0 / (wc * std::f64::consts::FRAC_PI_2).tan();
        let a = (8192.0 / (c * s2 + c * c + 1.0)).round_ties_even();
        if a == 0.0 {
            return Some((0, 0, 0));
        }
        let a0 = a / 8192.0;
        let c = ((4.0 / a0 - 2.0).sqrt() - s2) * 0.5;
        let b1 = ((1.0 - c * c) * a0 * 16384.0).round_ties_even();
        let b2 = (((1.0 - c * s2) + c * c) * a0 * 8192.0).round_ties_even();
        Some((a as i32, b1 as i32, b2 as i32))
    }

    /// Sets the cutoff; keeps the filter history.
    pub fn set(&mut self, cutoff: f32, playback_rate: f32, output_rate: f32) {
        self.coef = Self::coefficients(cutoff, playback_rate, output_rate);
    }

    /// Filters one sample.
    pub fn process(&mut self, x: f32) -> f32 {
        let Some((a, b1, b2)) = self.coef else { return x };
        let k = 1.0 / 8192.0;
        let y = (a as f32 * (x + 2.0 * self.x1 + self.x2) - b1 as f32 * self.y1 - b2 as f32 * self.y2) * k;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn manager_gain_law() {
        let full = GroupVolume::default();
        // UI click: volume 0.07, 2D multiplier 2, speakers 1 → 0.14.
        assert!(close(manager_gain(full, full, 0.07, 1.0, 2.0), 0.14, 1e-6));
        let half = GroupVolume { enabled: true, volume: 50 };
        assert!(close(manager_gain(half, full, 0.3, 1.0, 2.0), 0.3, 1e-6));
        assert_eq!(manager_gain(full, GroupVolume { enabled: false, volume: 100 }, 1.0, 1.0, 1.0), 0.0);
        assert_eq!(manager_gain(full, full, 1.0, 1.0, 2.0), 1.0, "clamped");
    }

    #[test]
    fn group_from_param() {
        assert_eq!(VolumeGroup::from_param(0.0), VolumeGroup::Music);
        assert_eq!(VolumeGroup::from_param(1.0), VolumeGroup::Sfx);
        assert_eq!(VolumeGroup::from_param(3.0), VolumeGroup::Interface);
    }

    #[test]
    fn curve_pan_and_q11() {
        assert!(close(miles_curve(0.5), 0.5f32.powf(5.0 / 3.0), 1e-6));
        assert!(close(CENTRE_PAN, 0.5f32.powf(0.3), 1e-6));
        let (l, r) = miles_pan(1.0, 0.5);
        assert!(close(l, CENTRE_PAN, 1e-7) && l == r);
        let (l, r) = miles_pan(1.0, 1.0);
        assert!(l == 0.0 && r == 1.0);
        assert_eq!(channel_gain(1.5), 1.0, "clamped at Q11 unity");
        assert_eq!(channel_gain(0.0), 0.0, "0.5 rounds to even");
        // 2D: min(1, 10 × 0.8122522 × v^(5/3)): 0.14 → ≈ 0.3054 before quantising.
        let g = voice_2d_gain(0.14);
        assert!(close(g, 10.0 * CENTRE_PAN * 0.14f32.powf(5.0 / 3.0), 1.0 / 2048.0));
        assert_eq!(voice_2d_gain(1.0), 1.0);
    }

    #[test]
    fn falloff_laws() {
        assert_eq!(default_falloff(1.0, 2.0, 1.0), 1.0);
        assert!(close(default_falloff(4.0, 2.0, 1.0), 0.5, 1e-6));
        assert!(close(default_falloff(6.0, 2.0, 0.5), 2.0 / (4.0 * 0.5 + 2.0), 1e-6));
        // The callback truncates the falloff to hundredths and is 1 near the listener.
        assert!(close(event_falloff(6.0, 2.0, 0.509), default_falloff(6.0, 2.0, 0.5), 1e-6));
        assert_eq!(event_falloff(0.9, 2.0, 0.5), 1.0);
        assert_eq!(event_falloff(10.0, 0.005, 0.5), 1.0);
        // distance_gain: same falloff as the global → Miles default; clamped at max.
        assert!(close(distance_gain(100.0, 2.0, 40.0, 1.0, 1.0), default_falloff(40.0, 2.0, 1.0), 1e-6));
        assert!(past_max(41.0, 40.0) && !past_max(40.0, 40.0));
    }

    #[test]
    fn distances_delays_and_lowpass() {
        assert_eq!(event_distances(1.0, 5.0, 14.0, 1.0, 1.0), (14.0, 70.0));
        // max ≤ 0: min × (1 − ln 0.01 / ln 2) = min × 7.64.
        let (_, m) = event_distances(1.0, 0.0, 14.0, 1.0, 1.0);
        assert!(close(m, 14.0 * (1.0 - 0.01f32.ln() / 2f32.ln()), 1e-3));
        assert_eq!(launch_delay(90.0, true, 100.0, 340.29, 0.1), 0.0);
        assert_eq!(launch_delay(120.0, true, 100.0, 340.29, 0.1), 0.0, "below 0.1 s");
        assert!(close(launch_delay(440.29, true, 100.0, 340.29, 0.1), 1.0, 1e-5));
        assert_eq!(launch_delay(1000.0, false, 100.0, 340.29, 0.1), 0.0);
        assert_eq!(random_trigger_delay(0.0, 0.5), 0.0);
        assert!(close(random_trigger_delay(0.5, 0.5), 0.25, 1e-6));
        assert!(random_trigger_delay(0.5, 0.9999) < 0.5);
        assert_eq!(low_pass_cutoff(0.1, 14.0, 0.00001, 0.05), 1.0);
        let c = low_pass_cutoff(300.0, 14.0, 0.00001, 0.05);
        assert!(close(c, 2.5 / (0.00001 * (14.0 * 90_000.0 - 2.5) + 2.5), 1e-6));
        assert_eq!(low_pass_cutoff(10_000.0, 14.0, 0.00001, 0.05), 0.05, "floor");
    }

    #[test]
    fn fades() {
        assert_eq!(equal_power_fade(1.0), 1.0);
        assert_eq!(equal_power_fade(0.0), 0.0);
        // The fast square root is close to sqrt and exact at powers of 4.
        assert_eq!(equal_power_fade(0.25), 0.5);
        assert_eq!(equal_power_fade(0.5), 0.75, "the bit trick, not sqrt (0.707)");
    }

    #[test]
    fn stereo_3d() {
        // Straight ahead: equal on both sides.
        let (l, r) = stereo_3d_gains([0.0, 0.0, 1.0], 10.0, 1.0);
        assert!(close(l, r, 1e-6) && l > 0.0);
        // To the right: louder on the right.
        let (l, r) = stereo_3d_gains([1.0, 0.0, 0.0], 10.0, 1.0);
        assert!(r > l);
        // At the listener.
        let (l, r) = stereo_3d_gains([0.0, 0.0, 1.0], 0.0, 0.8);
        assert!(close(l, 0.8 * 0.5f32.sqrt(), 1e-6) && l == r);
        // Behind: still both sides (the virtual rear speaker is shared).
        let (l, r) = stereo_3d_gains([0.0, 0.0, -1.0], 10.0, 1.0);
        assert!(close(l, r, 1e-6) && l > 0.0);
    }

    #[test]
    fn doppler_clamps() {
        assert_eq!(doppler(0.0, 1.0, 1.0), 1.0);
        assert_eq!(doppler(1.0, 1.0, 1.0), 0.25);
        assert_eq!(doppler(-1.0, 1.0, 1.0), 4.0);
    }

    #[test]
    fn low_pass_filter() {
        assert_eq!(LowPass::coefficients(1.0, 44_100.0, 44_100.0), None);
        let (a, b1, b2) = LowPass::coefficients(0.5, 44_100.0, 44_100.0).unwrap();
        // Half Nyquist: c = 1, a0 = 1/(2 + √2) → A = round(8192 × 0.29289) = 2399; B1 ≈ 0, B2 ≈ 1405.
        assert_eq!(a, 2399);
        assert!(b1.abs() <= 2, "{b1}");
        assert!((b2 - 1405).abs() <= 2, "{b2}");
        // Unity DC gain: (4A − B1 − B2) / 8192 ≈ 1 → a step settles near 1.
        let mut f = LowPass::default();
        f.set(0.5, 44_100.0, 44_100.0);
        let mut y = 0.0;
        for _ in 0..200 {
            y = f.process(1.0);
        }
        assert!(close(y, (4 * a) as f32 / (8192 + b1 + b2) as f32, 1e-3), "{y}");
        let mut off = LowPass::default();
        assert_eq!(off.process(0.3), 0.3);
    }
}

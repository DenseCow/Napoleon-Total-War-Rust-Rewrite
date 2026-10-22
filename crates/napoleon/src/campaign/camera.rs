//! The campaign camera: a target point on the map, a distance and a tilt that follows the zoom.
//!
//! The keyboard moves the camera only through the campaign's key bindings of
//! `text/default_keys.xml` (the camera's `step_fwd` / `step_bck` / `step_l` / `step_r`, W/S/A/D in
//! vanilla's default `fps` keyset; the arrows only in its `total_war` keyset), each a repeating
//! binding pressed every frame it is held, queued by `super::hud::keys` (CONFIRMED, `0x0095A510`
//! and the per-frame poll `0x00A27EB0`; see [`key_step`]). No campaign code reads the arrows
//! directly but the debug camera listener (`0x00970F70`), which vanilla never enables
//! (CONFIRMED, `analysis/fidelity/UI_FIDELITY.md` §5.1 "Shortcut keys").
//!
//! CONFIRMED (tweaker defaults in `Napoleon.exe`, `CampaignCamera.cpp` lines 91-100 and
//! `EmpireCampaignController.cpp` 70-71, read with `analysis/worker1/re_tools tweak`):
//! minimum distance 15, maximum distance 100, tilt from straight down (-y) between 0.5 and 1.0
//! radians, field of view 57.2957 (= 1 radian), pan speed 50, tilt speed 50.
//!
//! CONFIRMED (`0x009208B0`, the eye placement the save's read constructor `0x00900C00` runs): the
//! distance is linear in the tilt, from the maximum distance at the minimum tilt (tweakers
//! `0x015C4078` 0.5 and `0x015C2FA0` 100) to the minimum distance at the maximum tilt (`0x015C3B20`
//! 1.0 and `0x015C3A30` 15), and the eye sits at target + distance × (0, cos tilt, −sin tilt) in the
//! engine's axes (z north), so south of the target looking north ([`CampaignCamera::transform`]).
//!
//! A loaded game opens on the camera it was saved with (`ntw_campaign::camera`, CONFIRMED,
//! `0x0090E5A0` from the `CAMPAIGN_ENV` loader `0x00987070`): the save keeps the tilt, from which
//! [`distance_for_tilt`] gives the distance, and the target.
//!
//! INFERRED / PROVISIONAL (our own until the CampaignCamera code is read in Ghidra):
//! - the field of view is vertical;
//! - the mouse drag pans by `distance` × 0.002 per pixel (our own; the exe's per-frame mouse
//!   update `0x00970A90` is not ported yet);
//! - the target is clamped to the theatre bounds from `regions.esf`;
//! - no camera rotation (the original campaign camera always faces north, as far as we know).

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use ntw_sim::campaign::{zoom_limit, ZoomLimit};

/// Tweaker `CAMPAIGN_CAMERA_MINIMUM_DISTANCE` (CONFIRMED default, [`zoom_limit::MIN_DISTANCE`]).
pub const MIN_DISTANCE: f32 = zoom_limit::MIN_DISTANCE;
/// Tweaker `CAMPAIGN_CAMERA_MAXIMUM_DISTANCE` (CONFIRMED default, [`zoom_limit::MAX_DISTANCE`]).
pub const MAX_DISTANCE: f32 = zoom_limit::MAX_DISTANCE;
/// Tweaker `CAMPAIGN_CAMERA_MINIMUM_TILT_ANGLE`, radians from -y (CONFIRMED default,
/// [`zoom_limit::MIN_TILT`]).
pub const MIN_TILT: f32 = zoom_limit::MIN_TILT;
/// Tweaker field of view, 57.2957 degrees (CONFIRMED default).
pub const FOV_DEGREES: f32 = 57.2957;

/// The camera's state, in Bevy coordinates (x east, z south).
#[derive(Component, Debug, Clone, Copy)]
pub struct CampaignCamera {
    /// The point looked at (Bevy x, z).
    pub target: Vec2,
    /// Distance from the target.
    pub distance: f32,
    /// Allowed target area (Bevy x/z min and max).
    pub min: Vec2,
    /// Allowed target area max.
    pub max: Vec2,
    /// Ground height at the target (updated by the scene).
    pub ground: f32,
}

/// The distance the exe places the eye at for `tilt` (`0x009208B0`, CONFIRMED linear law; see the
/// module doc), not clamped: neither `0x009208B0` nor the save's read constructor `0x00900C00`
/// clamps, so a loaded tilt is shown as saved. The tilt is clamped only on a tilt change, by the tilt
/// step `0x00920D00`, to the world's zoom limit (CONFIRMED, [`ZoomLimit`]: the tweakers' 0.5..1.0,
/// 15..100 here, unless the scripts' `set_zoom_limit` narrowed it; [`CampaignCamera::zoom`]).
pub fn distance_for_tilt(tilt: f32) -> f32 {
    zoom_limit::distance_for_tilt(tilt)
}

impl CampaignCamera {
    /// Tilt from straight down for the current distance (the inverse of [`distance_for_tilt`]).
    pub fn tilt(&self) -> f32 {
        zoom_limit::tilt_for_distance(self.distance)
    }

    /// Zooms by `factor` (our own wheel step: the exe's wheel law `0x00956E10` is not ported yet),
    /// the new tilt clamped to `limit` as the exe's tilt step `0x00920D00` clamps every tilt change
    /// (`0x00956E10` → `0x00977100` → `0x00920D00`, CONFIRMED).
    pub fn zoom(&mut self, factor: f32, limit: &ZoomLimit) {
        self.distance = distance_for_tilt(limit.clamp(zoom_limit::tilt_for_distance(self.distance * factor)));
    }

    /// Looks where the saved camera `view` looks: its target (logic east, north) and the distance
    /// of its tilt, the target kept inside the allowed area.
    pub fn look_at_view(&mut self, view: &ntw_campaign::camera::CameraView) {
        let [east, _, north] = view.target;
        self.target = Vec2::new(east, -north).clamp(self.min, self.max);
        self.distance = distance_for_tilt(view.tilt);
    }

    /// `kept` with this camera's tilt and target (logic east, height 0, north, as the exe saves it),
    /// its other fields as they are.
    pub fn view(&self, kept: ntw_campaign::camera::CameraView) -> ntw_campaign::camera::CameraView {
        ntw_campaign::camera::CameraView { tilt: self.tilt(), target: [self.target.x, 0.0, -self.target.y], ..kept }
    }

    /// The camera transform for this state.
    pub fn transform(&self) -> Transform {
        let tilt = self.tilt();
        let focus = Vec3::new(self.target.x, self.ground, self.target.y);
        // Looking north (-Z): the camera sits south (+Z) of the target and above it.
        let eye = focus + self.distance * Vec3::new(0.0, tilt.cos(), tilt.sin());
        Transform::from_translation(eye).looking_at(focus, Vec3::Y)
    }
}

/// Tweaker `CAMPAIGN_CAMERA_PAN_SPEED` (CONFIRMED default 50, `0x015C4258` + 0x5C): the camera's
/// key steps scale with it ([`key_step`]).
pub const PAN_SPEED: f32 = 50.0;

/// A campaign camera key binding's func (`text/default_keys.xml`), as the key handler queues it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraFunc {
    /// `step_fwd`.
    Forward,
    /// `step_fwd_fast`.
    ForwardFast,
    /// `step_bck`.
    Back,
    /// `step_r` / `rot_r`.
    Right,
    /// `step_l` / `rot_l`.
    Left,
    /// A func the exe's camera acts on that ours does not port yet, with its name for the warning
    /// (`cam_up` / `cam_down` tilt, `0x00977100`; the bookmarks `0x00968900` / `0x00974B10`, by
    /// prefix): logged once.
    Unported(&'static str),
}

impl CameraFunc {
    /// The func a binding names, or None for one the camera ignores.
    pub fn from_binding(func: &str) -> Option<Self> {
        Some(match func {
            "step_fwd" => Self::Forward,
            "step_fwd_fast" => Self::ForwardFast,
            "step_bck" => Self::Back,
            "step_r" | "rot_r" => Self::Right,
            "step_l" | "rot_l" => Self::Left,
            "cam_up" => Self::Unported("cam_up"),
            "cam_down" => Self::Unported("cam_down"),
            f if f.starts_with("camera_bookmark_save") => Self::Unported("camera_bookmark_save"),
            f if f.starts_with("camera_bookmark_view") => Self::Unported("camera_bookmark_view"),
            _ => return None,
        })
    }
}

/// One camera binding pressed this frame: its func, the frame time it passes on and whether SHIFT
/// was held when the key handler ran it ([`key_step`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraKeyPress {
    pub func: CameraFunc,
    pub time: f32,
    pub shift: bool,
}

/// The campaign camera's key binding `func` pressed with frame time `time` (s): the target's move
/// in Bevy x/z, or None for a func ours does not port yet ([`CameraFunc::Unported`]).
/// CONFIRMED (`0x0095A510`, the camera handler's press): the step is `PAN_SPEED` × time × (eye
/// height × 0.01 + 0.25), the time doubled while SHIFT is held; `step_fwd` moves north
/// (`0x00956F10`, engine +z = Bevy −z), `step_fwd_fast` twice as far, `step_bck` south
/// (`0x00956E70`), `step_r` / `rot_r` east (`0x009570F0`) and `step_l` / `rot_l` west
/// (`0x00956FA0`). `time` is the frame time (the shortcut poll's, `0x00A27EB0`), the press coming
/// every frame the key is held. The press reads SHIFT from the handler's flag (+0x20), a flag the
/// camera's frame update `0x00970A90` (vtable slot `0x01362D6C`) sets from LSHIFT / RSHIFT once
/// per frame; ours passes the SHIFT held at the frame's poll (`super::hud::keys`). INFERRED: the
/// height is the camera's eye height above the map's zero (the camera's `+0x30` position, its
/// y); and that the flag is this frame's SHIFT (whether `0x00970A90` runs before or after the
/// poll in a frame, one frame's difference, is not traced).
/// PROVISIONAL: the handler's busy test (`0x00955820`, no step while it is set) is not ported.
pub fn key_step(func: CameraFunc, time: f32, shift: bool, eye_height: f32) -> Option<Vec2> {
    let time = if shift { time * 2.0 } else { time };
    let d = PAN_SPEED * time * (eye_height * 0.01 + 0.25);
    match func {
        CameraFunc::Forward => Some(Vec2::new(0.0, -d)),
        CameraFunc::ForwardFast => Some(Vec2::new(0.0, -2.0 * d)),
        CameraFunc::Back => Some(Vec2::new(0.0, d)),
        CameraFunc::Right => Some(Vec2::new(d, 0.0)),
        CameraFunc::Left => Some(Vec2::new(-d, 0.0)),
        CameraFunc::Unported(_) => None,
    }
}

/// The camera's key bindings move it ([`key_step`]), the mouse wheel zooms, dragging with the right
/// or middle mouse button pans.
#[allow(clippy::too_many_arguments)] // Bevy system: every parameter is a system param, not splittable
pub fn control(
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    ground: Option<Res<super::scene::CampaignGround>>,
    mut hud: Option<NonSendMut<super::hud::CampaignHud>>,
    sim: Option<NonSend<super::play::CampaignSim>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    mut cams: Query<(&mut CampaignCamera, &mut Transform)>,
    mut unported: Local<bool>,
) {
    // Harness runs (`--screenshot`) ignore the live keyboard and mouse so captures are repeatable.
    let live = harness.is_none();
    // The frame's camera key steps (only live runs queue any), emptied after the move.
    let steps = hud.as_deref().map_or(&[][..], |h| h.camera_steps.as_slice());
    for (mut cam, mut tf) in &mut cams {
        if !live {
            if let Some(g) = &ground {
                cam.ground = g.0.height_at(cam.target.x, -cam.target.y);
            }
            *tf = cam.transform();
            continue;
        }
        let eye_height = tf.translation.y;
        for step in steps {
            if let Some(d) = key_step(step.func, step.time, step.shift, eye_height) {
                cam.target += d;
            } else if let CameraFunc::Unported(func) = step.func
                && !*unported
            {
                *unported = true;
                warn!("Campaign camera: the key binding {func} is not ported yet (cam_up / cam_down tilt 0x00977100, the bookmarks 0x00968900); logged once");
            }
        }
        if mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) {
            let d = cam.distance;
            cam.target -= motion.delta * d * 0.002;
        }
        let zoom = -scroll.delta.y;
        if zoom != 0.0 {
            let limit = sim.as_ref().map_or_else(ZoomLimit::default, |s| s.host.model().world.zoom_limit);
            cam.zoom(1.12f32.powf(zoom), &limit);
        }
        let (min, max) = (cam.min, cam.max);
        cam.target = cam.target.clamp(min, max);
        if let Some(g) = &ground {
            cam.ground = g.0.height_at(cam.target.x, -cam.target.y);
        }
        *tf = cam.transform();
    }
    // The saver keeps the camera a save written now holds (the save is called from the HUD's scripts,
    // which do not see the camera).
    if let Some(sim) = &sim
        && let Some((cam, _)) = cams.iter().next()
    {
        sim.saver.camera.set(cam.view(sim.saver.camera.get()));
    }
    if let Some(h) = &mut hud {
        h.camera_steps.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilt_follows_zoom() {
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: MIN_DISTANCE, min: Vec2::splat(-1.0), max: Vec2::splat(1.0), ground: 0.0 };
        assert!((c.tilt() - zoom_limit::MAX_TILT).abs() < 1e-6);
        c.distance = MAX_DISTANCE;
        assert!((c.tilt() - MIN_TILT).abs() < 1e-6);
        let t = c.transform();
        // Zoomed out: above and south of the target, looking north and down.
        assert!(t.translation.y > 0.0 && t.translation.z > 0.0);
        assert!(t.forward().z < 0.0 && t.forward().y < 0.0);
    }

    /// The saved camera's tilt gives the exe's distance (`0x009208B0`: 0.5 → 100, 1.0 → 15, linear),
    /// and a camera opened on a saved view saves the same view back (logic north is Bevy −z), its
    /// unknown fields kept.
    #[test]
    fn a_saved_view_opens_and_saves_back() {
        use ntw_campaign::camera::CameraView;
        assert!((distance_for_tilt(MIN_TILT) - MAX_DISTANCE).abs() < 1e-4);
        assert!((distance_for_tilt(zoom_limit::MAX_TILT) - MIN_DISTANCE).abs() < 1e-4);
        assert!((distance_for_tilt(0.65) - 74.5).abs() < 1e-3);
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: 60.0, min: Vec2::splat(-200.0), max: Vec2::splat(200.0), ground: 0.0 };
        let saved = CameraView { tilt: 0.65, field_13c: 3, target: [-64.0, 0.0, -4.0], flag_140: true };
        c.look_at_view(&saved);
        assert_eq!(c.target, Vec2::new(-64.0, 4.0));
        let back = c.view(CameraView { tilt: 0.0, target: [0.0; 3], ..saved });
        assert!((back.tilt - 0.65).abs() < 1e-5);
        assert_eq!((back.target, back.field_13c, back.flag_140), ([-64.0, 0.0, -4.0], 3, true));
        // A target outside the theatre is brought inside it.
        c.look_at_view(&CameraView { target: [500.0, 0.0, 0.0], ..saved });
        assert_eq!(c.target, Vec2::new(200.0, 0.0));
    }

    /// A zoom clamps the new tilt to the world's zoom limit (`0x00920D00`); a loaded tilt outside it
    /// is shown as saved (`0x00900C00` does not clamp) until the next zoom.
    #[test]
    fn a_zoom_clamps_the_tilt_to_the_zoom_limit() {
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: 60.0, min: Vec2::splat(-1.0), max: Vec2::splat(1.0), ground: 0.0 };
        let full = ZoomLimit::default();
        c.zoom(10.0, &full);
        assert!((c.distance - MAX_DISTANCE).abs() < 1e-3);
        c.zoom(0.01, &full);
        assert!((c.distance - MIN_DISTANCE).abs() < 1e-3);
        // The eur script's limit after the intro (`set_zoom_limit(1.0, 0.61)`): out to about 81.3.
        let eur = ZoomLimit::from_script(1.0, 0.61);
        c.zoom(10.0, &eur);
        assert!((c.distance - 81.3).abs() < 1e-3, "{}", c.distance);
        // A tilt the scripts allow below 0.5 gives a distance past 100, unclamped.
        let low = ZoomLimit::from_script(1.0, 0.0);
        c.zoom(10.0, &low);
        assert!((c.distance - distance_for_tilt(0.02)).abs() < 1e-3 && c.distance > MAX_DISTANCE);
        c.look_at_view(&ntw_campaign::camera::CameraView { tilt: 0.3, field_13c: 0, target: [0.0; 3], flag_140: false });
        assert!((c.tilt() - 0.3).abs() < 1e-5);
    }

    /// The key step's length and direction (`0x0095A510`): 50 × time × (height × 0.01 + 0.25),
    /// doubled with SHIFT; forward is north (Bevy −z).
    #[test]
    fn key_steps_follow_the_exes_formula() {
        use CameraFunc::*;
        let d = 50.0 * 0.1 * (30.0 * 0.01 + 0.25);
        let near = |a: Option<Vec2>, b: Vec2| a.is_some_and(|a| (a - b).length() < 1e-4);
        assert!(near(key_step(Forward, 0.1, false, 30.0), Vec2::new(0.0, -d)));
        assert!(near(key_step(Forward, 0.1, true, 30.0), Vec2::new(0.0, -2.0 * d)));
        assert!(near(key_step(ForwardFast, 0.1, false, 30.0), Vec2::new(0.0, -2.0 * d)));
        assert!(near(key_step(Back, 0.1, false, 30.0), Vec2::new(0.0, d)));
        assert!(near(key_step(Right, 0.1, false, 30.0), Vec2::new(d, 0.0)));
        assert!(near(key_step(Left, 0.1, false, 30.0), Vec2::new(-d, 0.0)));
        assert_eq!(key_step(Unported("cam_up"), 0.1, false, 30.0), None);
    }

    /// The bindings' func names: `rot_r` / `rot_l` step as `step_r` / `step_l`; the tilt and the
    /// bookmarks are not ported; anything else the camera ignores.
    #[test]
    fn camera_funcs_come_from_the_binding_names() {
        assert_eq!(CameraFunc::from_binding("step_fwd"), Some(CameraFunc::Forward));
        assert_eq!(CameraFunc::from_binding("rot_r"), Some(CameraFunc::Right));
        assert_eq!(CameraFunc::from_binding("step_l"), Some(CameraFunc::Left));
        assert_eq!(CameraFunc::from_binding("cam_up"), Some(CameraFunc::Unported("cam_up")));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_view2"), Some(CameraFunc::Unported("camera_bookmark_view")));
        assert_eq!(CameraFunc::from_binding("increase_camera_rot"), None);
    }
}

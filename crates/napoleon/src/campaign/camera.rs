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
//! - the target is clamped to the theatre bounds from `regions.esf` after every move, standing in
//!   for the exe's clamp to the map's outline (`0x008B65B0` / `0x008B6400`, not ported); the key steps first stop
//!   at the world's map bounds as the exe's do ([`key_step`]);
//! - no camera rotation (the original campaign camera always faces north, as far as we know).
//!
//! The mouse wheel tilts the camera, so it zooms ([`wheel_tilt_percent`], CONFIRMED `0x00956E10`),
//! as the keys `cam_up` / `cam_down` do ([`key_tilt_percent`], CONFIRMED `0x0095A510`). The keys
//! `camera_bookmark_save<n>` / `camera_bookmark_view<n>` keep and restore the target
//! ([`CameraBookmarks`], CONFIRMED `0x00968900` / `0x00974B10`). Neither the keys nor the wheel act
//! while the camera holds its input (the busy test `0x00955820`, CONFIRMED; [`control`]).

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use ntw_sim::campaign::{zoom_limit, MapBounds, ZoomLimit};

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
#[require(CameraBookmarks)]
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
/// 15..100 here, unless the scripts' `set_zoom_limit` narrowed it; [`CampaignCamera::add_tilt_percent`]).
pub fn distance_for_tilt(tilt: f32) -> f32 {
    zoom_limit::distance_for_tilt(tilt)
}

impl CampaignCamera {
    /// Tilt from straight down for the current distance (the inverse of [`distance_for_tilt`]).
    pub fn tilt(&self) -> f32 {
        zoom_limit::tilt_for_distance(self.distance)
    }

    /// Adds `percent` hundredths of a radian to the tilt (`0x00977100`: tilt + percent × 0.01, the
    /// float `0x01318034`), the new tilt clamped to `limit` by the tilt step `0x00920D00`, as every
    /// tilt change is (CONFIRMED). The wheel ([`wheel_tilt_percent`]) and the keys `cam_up` /
    /// `cam_down` ([`key_tilt_percent`]) tilt through it.
    pub fn add_tilt_percent(&mut self, percent: f32, limit: &ZoomLimit) {
        self.distance = distance_for_tilt(limit.clamp(self.tilt() + percent * 0.01));
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

    /// Runs one key binding press (`0x0095A510`), the eye `eye_height` above the map's zero: a step
    /// moves the target ([`key_step`]), `cam_up` / `cam_down` tilt ([`key_tilt_percent`]),
    /// `camera_bookmark_save<n>` keeps the target in `marks` and `camera_bookmark_view<n>` jumps
    /// to a kept one ([`jump_target`]), the tilt unchanged.
    pub fn press(&mut self, press: &CameraKeyPress, eye_height: f32, marks: &mut CameraBookmarks, bounds: &MapBounds, limit: &ZoomLimit) {
        match press.func {
            CameraFunc::BookmarkSave(slot) => marks.save(slot, self.target),
            CameraFunc::BookmarkView(slot) => {
                if let Some(to) = marks.get(slot) {
                    self.target = jump_target(to, bounds);
                }
            }
            func => {
                if let Some(to) = key_step(func, press.time, press.shift, eye_height, self.target, bounds) {
                    self.target = to;
                } else if let Some(percent) = key_tilt_percent(func, press.time) {
                    self.add_tilt_percent(percent, limit);
                }
            }
        }
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

/// Tweaker `CAMPAIGN_CAMERA_TILT_SPEED` (CONFIRMED default 50, `0x015C36E0`): `cam_up` /
/// `cam_down` tilt by it ([`key_tilt_percent`]).
pub const TILT_SPEED: f32 = 50.0;

/// Wheel units per notch of a line-scrolling wheel (`WHEEL_DELTA`, what `WM_MOUSEWHEEL` reports
/// per notch and the exe's wheel sum `0x0164B408` adds).
pub const WHEEL_UNITS_PER_LINE: f32 = 120.0;

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
    /// `cam_up`: tilts towards the map, zooming out ([`key_tilt_percent`]).
    TiltUp,
    /// `cam_down`: tilts away from the map, zooming in ([`key_tilt_percent`]).
    TiltDown,
    /// `camera_bookmark_save<n>`: keeps the camera's target in bookmark `n` ([`CameraBookmarks`]).
    BookmarkSave(usize),
    /// `camera_bookmark_view<n>`: moves the target to bookmark `n` ([`CameraBookmarks`]).
    BookmarkView(usize),
}

/// The bookmark funcs' prefixes, 20 characters each; the slot follows them (`0x0095A510`).
const BOOKMARK_SAVE: &str = "camera_bookmark_save";
const BOOKMARK_VIEW: &str = "camera_bookmark_view";

/// The bookmark slot of a func whose prefix is followed by `rest` (CONFIRMED, `0x0095A510`): the
/// one character after the prefix (`0x004F47D0`: position 20, length 1) read as leading digits
/// (`0x004F37B0`), so a digit gives its value and anything else (nothing, a letter) slot 0;
/// `camera_bookmark_save12` is slot 1. Vanilla binds slots 0..3 (`text/default_keys.xml`:
/// SHIFT + F9..F12 save, F9..F12 view).
fn bookmark_slot(rest: &str) -> usize {
    rest.chars().next().and_then(|c| c.to_digit(10)).map_or(0, |d| d as usize)
}

impl CameraFunc {
    /// The func a binding names, or None for one the camera ignores.
    pub fn from_binding(func: &str) -> Option<Self> {
        if let Some(rest) = func.strip_prefix(BOOKMARK_SAVE) {
            return Some(Self::BookmarkSave(bookmark_slot(rest)));
        }
        if let Some(rest) = func.strip_prefix(BOOKMARK_VIEW) {
            return Some(Self::BookmarkView(bookmark_slot(rest)));
        }
        Some(match func {
            "step_fwd" => Self::Forward,
            "step_fwd_fast" => Self::ForwardFast,
            "step_bck" => Self::Back,
            "step_r" | "rot_r" => Self::Right,
            "step_l" | "rot_l" => Self::Left,
            "cam_up" => Self::TiltUp,
            "cam_down" => Self::TiltDown,
            _ => return None,
        })
    }
}

/// The campaign's camera bookmarks: per slot, the target kept by `camera_bookmark_save<n>`, or
/// none. CONFIRMED: the camera manager (`0x00901120`, built with each campaign by `0x00987070`)
/// holds them at +0x28, all empty; the save (`0x00968900`) keeps the current camera's eye and
/// target, and the view (`0x00974B10`) of a kept slot jumps the camera to its target with
/// `0x00955A00` ([`jump_target`]), the tilt unchanged, so the eye is never read back and ours
/// keeps the target only. The manager's save writer `0x0090FC90` does not write them: a loaded
/// game starts with none.
///
/// ORIGINAL BUG: the manager makes 4 slots (`0x00901120`) and neither the save nor the view checks
/// the slot, so a binding for slot 4..9 (none in vanilla) reads and writes past the array. Ours
/// keeps any slot.
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct CameraBookmarks(Vec<Option<Vec2>>);

impl CameraBookmarks {
    /// Keeps `target` (Bevy x/z) in `slot`, replacing what it held (`0x00968900`).
    pub fn save(&mut self, slot: usize, target: Vec2) {
        if self.0.len() <= slot {
            self.0.resize(slot + 1, None);
        }
        self.0[slot] = Some(target);
    }

    /// The target `slot` keeps, or None for one never saved (`0x00974B10` then does nothing).
    pub fn get(&self, slot: usize) -> Option<Vec2> {
        self.0.get(slot).copied().flatten()
    }
}

/// The target (Bevy x/z) of the camera's jump to `to` (Bevy x/z) (`0x00955A00`): `to` clamped to
/// the world's map bounds ([`MapBounds::clamp`], CONFIRMED). The jump then clamps it to the map's
/// outline (`0x008B6260`, not ported), which [`control`]'s theatre clamp stands in for.
pub fn jump_target(to: Vec2, bounds: &MapBounds) -> Vec2 {
    let (x, north) = bounds.clamp(to.x, -to.y);
    Vec2::new(x, -north)
}

/// One camera binding pressed this frame: its func, the frame time it passes on and whether SHIFT
/// was held when the key handler ran it ([`key_step`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraKeyPress {
    pub func: CameraFunc,
    pub time: f32,
    pub shift: bool,
}

/// The campaign camera's key binding `func` pressed with frame time `time` (s): the target (Bevy
/// x/z) after the step from `target`, or None for a func that does not move the target.
/// CONFIRMED (`0x0095A510`, the camera handler's press): the step is `PAN_SPEED` × time × (eye
/// height × 0.01 + 0.25), the time doubled while SHIFT is held; `step_fwd` moves north
/// (`0x00956F10`, engine +z = Bevy −z), `step_fwd_fast` twice as far, `step_bck` south
/// (`0x00956E70`), `step_r` / `rot_r` east (`0x009570F0`) and `step_l` / `rot_l` west
/// (`0x00956FA0`), each step stopped at the world's map bounds on its own axis and direction
/// (`MapBounds::step_north` and the rest). `time` is the frame time (the shortcut poll's,
/// `0x00A27EB0`), the press coming every frame the key is held. The press reads SHIFT from the
/// handler's flag (+0x20), a flag the camera's frame update `0x00970A90` (vtable slot
/// `0x01362D6C`) sets from LSHIFT / RSHIFT once per frame; ours passes the SHIFT held at the
/// frame's poll (`super::hud::keys`). INFERRED: the height is the camera's eye height above the
/// map's zero (the camera's `+0x30` position, its y); and that the flag is this frame's SHIFT
/// (whether `0x00970A90` runs before or after the poll in a frame, one frame's difference, is not
/// traced).
/// No func runs while the camera holds its input ([`control`], the busy test `0x00955820`).
/// PROVISIONAL: the step's clamp to the map's outline after the bounds (`0x008B65B0` / `0x008B6400`) is not
/// ported; [`control`] clamps to the theatre instead.
pub fn key_step(func: CameraFunc, time: f32, shift: bool, eye_height: f32, target: Vec2, bounds: &MapBounds) -> Option<Vec2> {
    let time = if shift { time * 2.0 } else { time };
    let d = PAN_SPEED * time * (eye_height * 0.01 + 0.25);
    // Bevy z is the engine's −z (north).
    let north = -target.y;
    match func {
        CameraFunc::Forward => Some(Vec2::new(target.x, -bounds.step_north(north, d))),
        CameraFunc::ForwardFast => Some(Vec2::new(target.x, -bounds.step_north(north, 2.0 * d))),
        CameraFunc::Back => Some(Vec2::new(target.x, -bounds.step_south(north, d))),
        CameraFunc::Right => Some(Vec2::new(bounds.step_east(target.x, d), target.y)),
        CameraFunc::Left => Some(Vec2::new(bounds.step_west(target.x, d), target.y)),
        CameraFunc::TiltUp | CameraFunc::TiltDown | CameraFunc::BookmarkSave(_) | CameraFunc::BookmarkView(_) => None,
    }
}

/// The tilt change, in hundredths of a radian ([`CampaignCamera::add_tilt_percent`]), of `cam_up` /
/// `cam_down` pressed with frame time `time` (s), or None for another func. CONFIRMED (`0x0095A510`):
/// `cam_up` tilts by −`TILT_SPEED` × time (towards the map, so out), `cam_down` by +`TILT_SPEED` ×
/// time; SHIFT does not speed the tilt up.
pub fn key_tilt_percent(func: CameraFunc, time: f32) -> Option<f32> {
    match func {
        CameraFunc::TiltUp => Some(-TILT_SPEED * time),
        CameraFunc::TiltDown => Some(TILT_SPEED * time),
        _ => None,
    }
}

/// The tilt change, in hundredths of a radian, of `units` wheel units turned over the map (positive
/// forward, 120 a notch): units × 0.03, so 0.036 radian a notch forward, zooming in (CONFIRMED,
/// `0x00956E10`, through [`CampaignCamera::add_tilt_percent`]); no frame time, so it does not
/// depend on the frame rate.
///
/// ORIGINAL BUG: the exe reads the wheel as the change of a 16-bit running sum (`0x0164B408`) since
/// the camera's last read (`0x015C4580`), wrapped at 65535 where the sum wraps at 65536
/// (`0x00E23470`, the float `0x0131A7CC`), so a turn across the wrap is one unit off; and the UI
/// reads the same sum against its own last value (`0x01459F90`), so wheel turned over a HUD panel
/// is applied to the camera at the next turn over the map. Ours takes the frame's wheel turned
/// over the map ([`control`]).
pub fn wheel_tilt_percent(units: f32) -> f32 {
    units * 0.03
}

/// The frame's wheel in wheel units (positive forward): a line-scrolling wheel's lines × 120, or
/// None for a pixel-scrolling device (a touchpad), which the exe never sees as notches.
pub fn wheel_units(scroll: &AccumulatedMouseScroll) -> Option<f32> {
    match scroll.unit {
        MouseScrollUnit::Line => Some(scroll.delta.y * WHEEL_UNITS_PER_LINE),
        MouseScrollUnit::Pixel => None,
    }
}

/// The camera's key bindings move it, tilt it and keep or view bookmarks
/// ([`CampaignCamera::press`]), the mouse wheel over the map tilts it ([`wheel_tilt_percent`]),
/// dragging with the right or middle mouse button pans. The bindings and the wheel do nothing while
/// the camera holds its input (CONFIRMED, the busy test `0x00955820`,
/// `ntw_campaign::camera::CameraView::holds_input`).
#[allow(clippy::too_many_arguments)] // Bevy system: every parameter is a system param, not splittable
pub fn control(
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    window: Query<&Window, With<PrimaryWindow>>,
    ground: Option<Res<super::scene::CampaignGround>>,
    mut hud: Option<NonSendMut<super::hud::CampaignHud>>,
    sim: Option<NonSend<super::play::CampaignSim>>,
    harness: Option<Res<crate::frontend::AutoScreenshot>>,
    mut cams: Query<(&mut CampaignCamera, &mut Transform, &mut CameraBookmarks)>,
    mut warned: Local<bool>,
) {
    // Harness runs (`--screenshot`) ignore the live keyboard and mouse so captures are repeatable.
    let live = harness.is_none();
    // The frame's camera key presses (only live runs queue any), emptied after the move.
    let steps = hud.as_deref().map_or(&[][..], |h| h.camera_steps.as_slice());
    let (limit, bounds) = sim.as_ref().map_or_else(Default::default, |s| {
        let world = &s.host.model().world;
        (world.zoom_limit, world.map_bounds)
    });
    // The wheel over a HUD panel belongs to the HUD (the UI's hit test `0x00DB2350` takes it), as
    // clicks do (`super::play::input`).
    let cursor = window.single().ok().and_then(|w| w.cursor_position());
    let over_hud = matches!((hud.as_deref(), cursor), (Some(h), Some(c)) if h.covers(c));
    let wheel = wheel_units(&scroll).unwrap_or_else(|| {
        if scroll.delta.y != 0.0 && !*warned {
            *warned = true;
            warn!("Campaign camera: a pixel-scrolling wheel or touchpad does not zoom (the exe reads wheel notches); logged once");
        }
        0.0
    });
    // The camera a save would hold, whose controller count and pre-battle hold the busy test reads
    // (`CameraView::holds_input`).
    let held = sim.as_ref().is_some_and(|s| s.saver.camera.get().holds_input());
    for (mut cam, mut tf, mut marks) in &mut cams {
        if !live {
            if let Some(g) = &ground {
                cam.ground = g.0.height_at(cam.target.x, -cam.target.y);
            }
            *tf = cam.transform();
            continue;
        }
        // The busy test `0x00955820` gates the key bindings and the wheel; the drag is not gated
        // here because the exe's drag (`0x00970A90`, which tests it too) is not ported.
        if !held {
            let eye_height = tf.translation.y;
            for step in steps {
                cam.press(step, eye_height, &mut marks, &bounds, &limit);
            }
        }
        if mouse.pressed(MouseButton::Right) || mouse.pressed(MouseButton::Middle) {
            let d = cam.distance;
            cam.target -= motion.delta * d * 0.002;
        }
        if !held && wheel != 0.0 && !over_hud {
            cam.add_tilt_percent(wheel_tilt_percent(wheel), &limit);
        }
        // PROVISIONAL: the theatre clamp stands in for the exe's clamp of a key step to the map's
        // outline (`0x008B65B0` / `0x008B6400`, not ported) and for the drag's (`0x00970A90`, not ported).
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
        && let Some((cam, _, _)) = cams.iter().next()
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

    /// A tilt change clamps the new tilt to the world's zoom limit (`0x00920D00`); a loaded tilt
    /// outside it is shown as saved (`0x00900C00` does not clamp) until the next change.
    #[test]
    fn a_tilt_change_clamps_the_tilt_to_the_zoom_limit() {
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: 60.0, min: Vec2::splat(-1.0), max: Vec2::splat(1.0), ground: 0.0 };
        let full = ZoomLimit::default();
        c.add_tilt_percent(-1000.0, &full);
        assert!((c.distance - MAX_DISTANCE).abs() < 1e-3);
        c.add_tilt_percent(1000.0, &full);
        assert!((c.distance - MIN_DISTANCE).abs() < 1e-3);
        // The eur script's limit after the intro (`set_zoom_limit(1.0, 0.61)`): out to about 81.3.
        let eur = ZoomLimit::from_script(1.0, 0.61);
        c.add_tilt_percent(-1000.0, &eur);
        assert!((c.distance - 81.3).abs() < 1e-3, "{}", c.distance);
        // A tilt the scripts allow below 0.5 (down to their floor 0.02) gives a distance past 100.
        let low = ZoomLimit::from_script(1.0, 0.0);
        c.add_tilt_percent(-1000.0, &low);
        assert!((c.distance - distance_for_tilt(0.02)).abs() < 1e-3 && c.distance > MAX_DISTANCE);
        c.look_at_view(&ntw_campaign::camera::CameraView { tilt: 0.3, field_13c: 0, target: [0.0; 3], flag_140: false });
        assert!((c.tilt() - 0.3).abs() < 1e-5);
    }

    /// The wheel (`0x00956E10`): a notch forward (120 units) adds 0.036 radian of tilt, zooming in,
    /// whatever the frame time; `cam_up` / `cam_down` (`0x0095A510`) tilt by ∓50 × time hundredths,
    /// SHIFT or not.
    #[test]
    fn the_wheel_and_the_tilt_keys_tilt_by_the_exes_law() {
        let full = ZoomLimit::default();
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: distance_for_tilt(0.65), min: Vec2::splat(-1.0), max: Vec2::splat(1.0), ground: 0.0 };
        let notch = AccumulatedMouseScroll { unit: MouseScrollUnit::Line, delta: Vec2::new(0.0, 1.0) };
        let units = wheel_units(&notch).expect("a line wheel");
        assert_eq!(units, 120.0);
        c.add_tilt_percent(wheel_tilt_percent(units), &full);
        assert!((c.tilt() - 0.686).abs() < 1e-5, "{}", c.tilt());
        assert!(c.distance < distance_for_tilt(0.65));
        assert_eq!(wheel_units(&AccumulatedMouseScroll { unit: MouseScrollUnit::Pixel, delta: Vec2::new(0.0, 30.0) }), None);
        assert_eq!(key_tilt_percent(CameraFunc::TiltUp, 0.1), Some(-5.0));
        assert_eq!(key_tilt_percent(CameraFunc::TiltDown, 0.1), Some(5.0));
        assert_eq!(key_tilt_percent(CameraFunc::Forward, 0.1), None);
        c.add_tilt_percent(key_tilt_percent(CameraFunc::TiltUp, 0.1).unwrap_or_default(), &full);
        assert!((c.tilt() - 0.636).abs() < 1e-5, "{}", c.tilt());
    }

    /// The key step's length and direction (`0x0095A510`): 50 × time × (height × 0.01 + 0.25),
    /// doubled with SHIFT; forward is north (Bevy −z); each step stops at the map bounds on its own
    /// axis and direction (`0x00956E70` / `0x00956F10` / `0x00956FA0` / `0x009570F0`).
    #[test]
    fn key_steps_follow_the_exes_formula() {
        use CameraFunc::*;
        let open = MapBounds::default();
        let d = 50.0 * 0.1 * (30.0 * 0.01 + 0.25);
        let near = |a: Option<Vec2>, b: Vec2| a.is_some_and(|a| (a - b).length() < 1e-4);
        let o = Vec2::ZERO;
        assert!(near(key_step(Forward, 0.1, false, 30.0, o, &open), Vec2::new(0.0, -d)));
        assert!(near(key_step(Forward, 0.1, true, 30.0, o, &open), Vec2::new(0.0, -2.0 * d)));
        assert!(near(key_step(ForwardFast, 0.1, false, 30.0, o, &open), Vec2::new(0.0, -2.0 * d)));
        assert!(near(key_step(Back, 0.1, false, 30.0, o, &open), Vec2::new(0.0, d)));
        assert!(near(key_step(Right, 0.1, false, 30.0, o, &open), Vec2::new(d, 0.0)));
        assert!(near(key_step(Left, 0.1, false, 30.0, o, &open), Vec2::new(-d, 0.0)));
        assert_eq!(key_step(BookmarkView(2), 0.1, false, 30.0, o, &open), None);
        assert_eq!(key_step(TiltUp, 0.1, false, 30.0, o, &open), None);
        // The tutorial's `set_map_bounds(-130, -75, -113, -98)`: x -130..-113, north -98..-75.
        let tut = MapBounds::from_script(-130.0, -75.0, -113.0, -98.0);
        let at = Vec2::new(-120.0, 80.0); // north -80
        assert!(near(key_step(Forward, 0.1, false, 30.0, at, &tut), Vec2::new(-120.0, 80.0 - d)));
        assert!(near(key_step(Forward, 1.0, false, 30.0, at, &tut), Vec2::new(-120.0, 75.0)));
        assert!(near(key_step(Back, 1.0, false, 30.0, at, &tut), Vec2::new(-120.0, 98.0)));
        assert!(near(key_step(Right, 1.0, false, 30.0, at, &tut), Vec2::new(-113.0, 80.0)));
        assert!(near(key_step(Left, 1.0, false, 30.0, at, &tut), Vec2::new(-130.0, 80.0)));
        // A target east of the bounds comes back to them on a step east, not on a step west.
        let east = Vec2::new(-100.0, 80.0);
        assert!(near(key_step(Right, 0.1, false, 30.0, east, &tut), Vec2::new(-113.0, 80.0)));
        assert!(near(key_step(Left, 0.1, false, 30.0, east, &tut), Vec2::new(-100.0 - d, 80.0)));
    }

    /// The bindings' func names: `rot_r` / `rot_l` step as `step_r` / `step_l`; `cam_up` /
    /// `cam_down` tilt; anything else the camera ignores. A bookmark's slot is the one character
    /// after its 20-character prefix read as a digit, else 0 (`0x004F47D0`, `0x004F37B0`).
    #[test]
    fn camera_funcs_come_from_the_binding_names() {
        use CameraFunc::*;
        assert_eq!(CameraFunc::from_binding("step_fwd"), Some(Forward));
        assert_eq!(CameraFunc::from_binding("rot_r"), Some(Right));
        assert_eq!(CameraFunc::from_binding("step_l"), Some(Left));
        assert_eq!(CameraFunc::from_binding("cam_up"), Some(TiltUp));
        assert_eq!(CameraFunc::from_binding("cam_down"), Some(TiltDown));
        assert_eq!(CameraFunc::from_binding("increase_camera_rot"), None);
        assert_eq!(CameraFunc::from_binding("camera_bookmark_view2"), Some(BookmarkView(2)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_save0"), Some(BookmarkSave(0)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_save3"), Some(BookmarkSave(3)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_save12"), Some(BookmarkSave(1)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_view"), Some(BookmarkView(0)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_viewx"), Some(BookmarkView(0)));
    }

    /// A bookmark keeps the target (`0x00968900`); its view jumps back to it, the tilt kept, the
    /// target clamped to the map bounds (`0x00974B10` → `0x00955A00`); an empty slot does nothing;
    /// a slot past the exe's 4 works (ORIGINAL BUG fix); a new save replaces the slot's target.
    #[test]
    fn bookmarks_keep_and_restore_the_target() {
        let open = MapBounds::default();
        let limit = ZoomLimit::default();
        let press = |func| CameraKeyPress { func, time: 0.1, shift: false };
        let mut marks = CameraBookmarks::default();
        let mut c = CampaignCamera { target: Vec2::new(10.0, -20.0), distance: 60.0, min: Vec2::splat(-500.0), max: Vec2::splat(500.0), ground: 0.0 };
        c.press(&press(CameraFunc::BookmarkSave(1)), 30.0, &mut marks, &open, &limit);
        c.press(&press(CameraFunc::BookmarkSave(7)), 30.0, &mut marks, &open, &limit);
        assert_eq!((marks.get(1), marks.get(7), marks.get(0)), (Some(Vec2::new(10.0, -20.0)), Some(Vec2::new(10.0, -20.0)), None));
        c.target = Vec2::new(-50.0, 40.0);
        c.distance = 30.0;
        c.press(&press(CameraFunc::BookmarkView(0)), 30.0, &mut marks, &open, &limit);
        assert_eq!(c.target, Vec2::new(-50.0, 40.0));
        c.press(&press(CameraFunc::BookmarkView(1)), 30.0, &mut marks, &open, &limit);
        assert_eq!((c.target, c.distance), (Vec2::new(10.0, -20.0), 30.0));
        c.target = Vec2::new(3.0, 4.0);
        c.press(&press(CameraFunc::BookmarkSave(1)), 30.0, &mut marks, &open, &limit);
        assert_eq!(marks.get(1), Some(Vec2::new(3.0, 4.0)));
        // The tutorial's bounds (x -130..-113, north -98..-75): the jump lands inside them.
        let tut = MapBounds::from_script(-130.0, -75.0, -113.0, -98.0);
        c.press(&press(CameraFunc::BookmarkView(7)), 30.0, &mut marks, &tut, &limit);
        assert_eq!(c.target, Vec2::new(-113.0, 75.0));
        assert_eq!(jump_target(Vec2::new(-120.0, 80.0), &tut), Vec2::new(-120.0, 80.0));
    }

    /// A press steps and tilts as [`key_step`] and [`key_tilt_percent`] give.
    #[test]
    fn a_press_steps_or_tilts() {
        let open = MapBounds::default();
        let limit = ZoomLimit::default();
        let mut marks = CameraBookmarks::default();
        let mut c = CampaignCamera { target: Vec2::ZERO, distance: distance_for_tilt(0.65), min: Vec2::splat(-500.0), max: Vec2::splat(500.0), ground: 0.0 };
        c.press(&CameraKeyPress { func: CameraFunc::Right, time: 0.1, shift: false }, 30.0, &mut marks, &open, &limit);
        assert!((c.target.x - 50.0 * 0.1 * 0.55).abs() < 1e-4);
        c.press(&CameraKeyPress { func: CameraFunc::TiltDown, time: 0.1, shift: false }, 30.0, &mut marks, &open, &limit);
        assert!((c.tilt() - 0.7).abs() < 1e-5, "{}", c.tilt());
        assert_eq!(marks, CameraBookmarks::default());
    }
}

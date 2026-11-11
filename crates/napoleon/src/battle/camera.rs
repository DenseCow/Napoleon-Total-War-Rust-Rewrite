//! The battle camera's rules: the original's free RTS camera, driven by the camera funcs of
//! `text/default_keys.xml` (the shortcut handler `0x005EAF00`) and kept in bounds every step and
//! every frame (`0x005B3DF0`). Pure logic; the systems that feed it are in `view.rs`.
//!
//! The camera is an eye and a look-at target one metre in front of it, plus the height above the
//! ground it keeps (BATTLE_FIDELITY.md §59.2). Positions are Bevy's (x, height, −map y); the
//! ground is passed in as a function of Bevy (x, z).

use bevy::prelude::*;
use ntw_formats::preferences::Preferences;

use crate::shortcut_keys::CameraBookmark;

/// The camera tweaks' defaults on land (CONFIRMED, each registered through `0x00454730` by its
/// initialiser): `CAMERA_HEIGHT_MIN_LAND` 2.5 (`0x00408860`), `CAMERA_HEIGHT_MAX_LAND` 60
/// (`0x004087C0`). The sea's (15 and 150, `0x004088B0` / `0x00408810`) wait for naval battles.
const CAMERA_HEIGHT_MIN_LAND: f32 = 2.5;
const CAMERA_HEIGHT_MAX_LAND: f32 = 60.0;
/// `CAMERA_MOVE_SPEED_MAX` 100 (`0x00408900`), `CAMERA_MOVE_SPEED_MIN` 10 (`0x00408950`),
/// `CAMERA_TURN_SPEED` 5 (`0x004089F0`), CONFIRMED.
const CAMERA_MOVE_SPEED_MAX: f32 = 100.0;
const CAMERA_MOVE_SPEED_MIN: f32 = 10.0;
const CAMERA_TURN_SPEED: f32 = 5.0;

/// The camera's vertical field of view, 70° (CONFIRMED, `0x01451468`), which scales its turns.
const FOV: f32 = 1.221_730_5;
/// The aspect ratio the yaw step multiplies by. INFERRED: the camera object's stays 4:3 (its
/// writer for a widescreen window is not traced).
const ASPECT: f32 = 4.0 / 3.0;
/// The camera's elevation limit, ± radians from the horizon (INFERRED: the generic camera's limit
/// fields, which no battle code was found writing; their start value).
const MAX_ELEVATION: f32 = 0.6;
/// A new camera's rate, before its first frame update sets it (CONFIRMED: the base camera
/// constructor `0x0058F3E0` stores it, `0x0058F46E`).
const START_RATE: f32 = 0.01;
/// The frame times the camera manager keeps (CONFIRMED: its ring's next slot wraps mod 100 at
/// `0x005FE4ED`, and `0x005F3790` reads at most 100).
const FRAME_TIMES: usize = 100;
/// The range the camera's rate is held in (CONFIRMED `0x005FE4F8`..`0x005FE520`: float
/// `0x01331B00` = 1/120, and 1.0).
const MIN_RATE: f32 = 1.0 / 120.0;
const MAX_RATE: f32 = 1.0;
/// The move and rotation scales' range and start (CONFIRMED, the controller `0x00592CC0` and the
/// funcs `accel` .. `decrease_camera_rot` of `0x005EAF00`).
const MIN_SCALE: f32 = 0.01;
const MAX_SCALE: f32 = 2.0;
/// The intelli-zoom's field of view, a quarter of the camera's (CONFIRMED `0x005F7D30`).
const ZOOM_FOV: f32 = FOV * 0.25;
/// The zoom camera's up / down (CONFIRMED `0x005E5CD0`): the field of view grows by the step's
/// vertical part × 0.005 (float `0x014514D4`), held in [5°, 85°] (`0x005E5D0E` / `0x005E5D21`).
/// The step's vector is the camera's up axis (`+0x54`) × the arg (`0x010EA4C0`), and that axis is
/// (0, 1, 0): `NewCameraOfType` `0x005F48F0` builds every camera with it (`0x005F497F`), the base
/// constructor `0x010CF360` stores it at `+0x54` (`0x010CF44B`), and the zoom camera takes the
/// manager's copy of the shown camera's (`0x005F7470` → `0x00592860`); the only later writers of
/// a camera's `+0x54` are the copy `0x010E9B80` (another camera's up) and the transition's blend
/// of its two cameras' ups (`0x005FFE65`), so the vertical part is the arg itself.
const ZOOM_FOV_STEP: f32 = 0.005;
const ZOOM_FOV_MIN: f32 = 0.087_266_46;
const ZOOM_FOV_MAX: f32 = 1.483_529_8;
/// The tracking camera's tilt is a rise by ten times the tilt's argument (CONFIRMED `0x005F9850`).
const TRACK_TILT_RISE: f32 = 10.0;
/// A transition's length (CONFIRMED, the transition camera's constructor `0x00592410`, called by
/// `0x005FB4D0`): the larger of 0.025 s per metre the eye moves and 3/π s per radian the view
/// turns, held in [0.5, 3] s, times the speed 1.0 every caller here passes.
const TRANSITION_PER_METRE: f32 = 0.025;
const TRANSITION_MIN: f32 = 0.5;
const TRANSITION_MAX: f32 = 3.0;
/// The shown eye's clearance over the ground during a transition (CONFIRMED `0x005FFB00`).
const TRANSITION_CLEARANCE: f32 = 2.0;

/// The ground under a point, Bevy (x, z) → height.
pub type Ground<'a> = &'a dyn Fn(Vec2) -> f32;

/// The speeds and limits a battle camera runs with: the tweaks, overridden by the player's
/// preferences (CONFIRMED, the camera manager's constructor `0x00590020` and `0x005EA6D0`):
/// `camera_move_speed` sets the top move speed and a tenth of it the bottom one,
/// `camera_turn_speed` the turn speed. `invert_cam_x_axis` inverts the tilt and
/// `invert_cam_y_axis` the turn: CONFIRMED that `0x005EAF00` negates the tilt when the preferences
/// object (`0x00611A50`) answers true to vfunc `+0x1EC` and the turn for vfunc `+0x1FC`; INFERRED
/// which preference each vfunc reads (only from the order of the options' help texts `0x0130F1B8`
/// "invert camera tilting up/down" and `0x0130F224` "invert camera panning left/right"; the
/// object's type, stored at battle UI `+0x2804C` by `0x00593BD0`, is not traced).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSettings {
    pub move_max: f32,
    pub move_min: f32,
    pub turn: f32,
    pub invert_tilt: bool,
    pub invert_turn: bool,
    pub min_height: f32,
    pub max_height: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            move_max: CAMERA_MOVE_SPEED_MAX,
            move_min: CAMERA_MOVE_SPEED_MIN,
            turn: CAMERA_TURN_SPEED,
            invert_tilt: false,
            invert_turn: false,
            min_height: CAMERA_HEIGHT_MIN_LAND,
            max_height: CAMERA_HEIGHT_MAX_LAND,
        }
    }
}

impl CameraSettings {
    /// The land camera's settings with the player's preferences (each falls back to its
    /// registered default, `Preferences::get_f64`, which gives the tweaks' values).
    pub fn from_prefs(prefs: &Preferences) -> Self {
        let mut s = Self::default();
        if let Some(v) = prefs.get_f64("camera_move_speed") {
            (s.move_max, s.move_min) = (v as f32, (v * 0.1) as f32);
        }
        if let Some(v) = prefs.get_f64("camera_turn_speed") {
            s.turn = v as f32;
        }
        s.invert_tilt = prefs.get_bool("invert_cam_x_axis").unwrap_or(false);
        s.invert_turn = prefs.get_bool("invert_cam_y_axis").unwrap_or(false);
        s
    }
}

/// A battle camera func of `text/default_keys.xml` (component `camera`): the funcs of the camera's
/// shortcut handler `0x005EAF00` (CONFIRMED names). The steps and turns are repeating bindings,
/// pressed every frame their key is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraFunc {
    StepFwd,
    /// The forward step doubled (CONFIRMED `0x005EAF00`).
    StepFwdFast,
    StepBck,
    StepL,
    StepR,
    RotL,
    RotR,
    CamUp,
    CamDown,
    RotU,
    RotD,
    /// `accel` / `decel`: the move scale up / down by e^time.
    Accel,
    Decel,
    /// `increase_camera_rot` / `decrease_camera_rot`: the rotation scale up / down by e^time.
    IncreaseRot,
    DecreaseRot,
    /// `toggle_move_fast`: doubles the steps, or stops doubling them.
    ToggleMoveFast,
    /// `camera_bookmark_save<n>` / `camera_bookmark_view<n>` ([`CameraBookmark`]).
    BookmarkSave(usize),
    BookmarkView(usize),
    /// `toggle_tracking` (`0x005F67B0`): follow the track target, or stop following it.
    ToggleTracking,
    /// `intelli_zoom` (`0x005F6840`): zoom in where the cursor points, or back out.
    IntelliZoom,
}

impl CameraFunc {
    /// The func of a binding's name; `None` for the one camera func not done yet,
    /// `context_camera` (`0x005F4CA0`, the context camera, BATTLE_FIDELITY.md §59.5), which
    /// `CameraKeys::press` logs.
    pub fn from_binding(func: &str) -> Option<Self> {
        match CameraBookmark::from_binding(func) {
            Some(CameraBookmark::Save(n)) => return Some(Self::BookmarkSave(n)),
            Some(CameraBookmark::View(n)) => return Some(Self::BookmarkView(n)),
            None => {}
        }
        Some(match func {
            "step_fwd" => Self::StepFwd,
            "step_fwd_fast" => Self::StepFwdFast,
            "step_bck" => Self::StepBck,
            "step_l" => Self::StepL,
            "step_r" => Self::StepR,
            "rot_l" => Self::RotL,
            "rot_r" => Self::RotR,
            "cam_up" => Self::CamUp,
            "cam_down" => Self::CamDown,
            "rot_u" => Self::RotU,
            "rot_d" => Self::RotD,
            "accel" => Self::Accel,
            "decel" => Self::Decel,
            "increase_camera_rot" => Self::IncreaseRot,
            "decrease_camera_rot" => Self::DecreaseRot,
            "toggle_move_fast" => Self::ToggleMoveFast,
            "toggle_tracking" => Self::ToggleTracking,
            "intelli_zoom" => Self::IntelliZoom,
            _ => return None,
        })
    }

    /// The input action a battle script's input handler hears for this func, if any
    /// (`ntw_script::battle_script::INPUT_EVENT_NAMES`; CONFIRMED: the camera handler broadcasts
    /// the code after each step, `0x004B0CF0`, so a held key posts it every frame, and the Battle
    /// Script Interface's listener `0x00657230` names codes 0..10 from table `0x014518E0` as here).
    /// Only the steps and turns post one (CONFIRMED `0x005EAF00`).
    pub fn input_name(self) -> Option<&'static str> {
        Some(match self {
            Self::StepFwd => "move forward",
            Self::StepFwdFast => "move forward fast",
            Self::StepBck => "move backward",
            Self::StepL => "move left",
            Self::StepR => "move right",
            Self::RotR => "rotate right",
            Self::RotL => "rotate left",
            Self::CamUp => "move up",
            Self::CamDown => "move down",
            Self::RotU => "rotate up",
            Self::RotD => "rotate down",
            _ => return None,
        })
    }
}

/// The camera manager's last frame times (CONFIRMED: the ring at manager `+0x850`, its count
/// `+0x9E0` and next slot `+0x9E4`, both zeroed by the manager's constructor `0x00590020` at
/// `0x00590427` / `0x00590431`). It belongs to the manager, so a new camera (a bookmark's view)
/// keeps it.
#[derive(Debug, Clone)]
struct FrameTimes {
    times: [f32; FRAME_TIMES],
    count: usize,
    next: usize,
}

impl Default for FrameTimes {
    fn default() -> Self {
        Self { times: [0.0; FRAME_TIMES], count: 0, next: 0 }
    }
}

impl FrameTimes {
    /// Keeps a frame's time and gives the camera's rate (CONFIRMED, the manager's frame vfunc
    /// `0x005FE410`): the time goes in the next slot (`0x005FE4C4`..`0x005FE4ED`: the count grows
    /// until it reaches 100, then the oldest slot is overwritten); the rate is the steady mean
    /// ([`Self::steady_mean`]) held in [1/120, 1]. A mean that is not a number (no time kept)
    /// gives 1: `minss` returns its second operand then (`0x005FE518`), as Rust's `min` does.
    fn next_rate(&mut self, time: f32) -> f32 {
        self.times[self.next] = time;
        self.count = self.count.max(self.next + 1);
        self.next = (self.next + 1) % FRAME_TIMES;
        held_rate(self.steady_mean())
    }

    /// The mean of the kept times within one standard deviation of their mean (CONFIRMED
    /// `0x005F3790`): the mean m of the first `count` slots in slot order, then the population
    /// standard deviation s (sum of (m − t)² over the count, square root), then the mean of the
    /// times with m − s ≤ t ≤ s + m, all in single precision as the exe's SSE code and summed in
    /// the same order, so it rounds as the exe's does. At least one time is always in range
    /// (were all farther than s, their variance would exceed s²); if rounding ever left none, the
    /// mean is 0/0, not a number, as in the exe.
    fn steady_mean(&self) -> f32 {
        let times = &self.times[..self.count];
        let n = times.len() as f32;
        let mean = times.iter().fold(0.0, |sum, &t| sum + t) / n;
        let sd = (times.iter().fold(0.0, |sum, &t| sum + (mean - t) * (mean - t)) / n).sqrt();
        let (lo, hi) = (mean - sd, sd + mean);
        let (sum, kept) = times.iter().filter(|&&t| t <= hi && lo <= t).fold((0.0, 0u32), |(sum, kept), &t| (sum + t, kept + 1));
        sum / kept as f32
    }
}

/// The steady mean held in [[`MIN_RATE`], [`MAX_RATE`]] (CONFIRMED `0x005FE4F8`..`0x005FE520`): the
/// floor when it is below it, else the smaller of it and 1, which is 1 for a mean that is not a
/// number.
fn held_rate(mean: f32) -> f32 {
    if MIN_RATE > mean { MIN_RATE } else { mean.min(MAX_RATE) }
}

/// A view direction's yaw (0 looking north, Bevy −z; positive to the east), pitch and length.
fn yaw_pitch_len(d: Vec3) -> (f32, f32, f32) {
    let len = d.length();
    let pitch = if len > 0.0 { (d.y / len).clamp(-1.0, 1.0).asin() } else { 0.0 };
    (d.x.atan2(-d.z), pitch, len)
}

/// A unit the intelli-zoom can zoom onto: its squad summary's centre on the ground (CONFIRMED
/// `0x005F7F25` / `0x005F7F31`, `GetSquadSummaryCentre` then its ground point) and radius (summary
/// `+0x670`, `0x005F7F76`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomUnit {
    pub centre: Vec3,
    pub radius: f32,
}

/// The unit the intelli-zoom zooms onto from `eye` along the unit vector `ray`, and the zoom's
/// target and field of view: the unit's centre, and half of atan2(radius, distance) held in
/// [5°, 85°] (CONFIRMED `0x005F80B3`..`0x005F80DB`). As the exe: first the unit with the largest
/// cosine between `ray` and the way from the eye to its centre over all units, a later one winning
/// a tie (CONFIRMED `0x005F7FC5`..`0x005F7FF6`), then one threshold test of that best cosine
/// (`0x005F8060`); when the best unit fails it, there is no unit, even if another unit's radius
/// holds the ray. The exe never zooms onto one (ORIGINAL BUG, see `BattleCamera::start_zoom`);
/// the threshold it meant is lost, so PROVISIONAL, ours: the best unit counts only when the ray
/// passes within its radius as seen from the eye (the cosine at least cos atan2(radius, distance)).
fn zoom_onto(eye: Vec3, ray: Vec3, units: &[ZoomUnit]) -> Option<(Vec3, f32)> {
    let mut best: Option<(f32, Vec3, f32)> = None;
    for u in units {
        let to = u.centre - eye;
        let Some(dir) = to.try_normalize() else { continue };
        let cos = dir.dot(ray);
        if best.is_none_or(|(b, ..)| b <= cos) {
            // The unit's angular half-size as seen from the eye.
            best = Some((cos, u.centre, u.radius.atan2(to.length())));
        }
    }
    let (cos, centre, ang) = best?;
    (cos >= ang.cos()).then(|| (centre, (0.5 * ang).clamp(ZOOM_FOV_MIN, ZOOM_FOV_MAX)))
}

/// Which camera the manager runs (BATTLE_FIDELITY.md §59.5): the free RTS camera, the tracking
/// camera or the intelli-zoom camera (the context camera is not done). One camera's view at a
/// time: [`BattleCamera::eye`], `target`, `height` and `fov` are that camera's.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Free,
    /// Following unit `id` (the tracking camera `0x00591BC0`): `frame` is the unit's position and
    /// facing at the last frame, `None` before the first; `free` is the free camera the manager
    /// keeps meanwhile (`+0x800`), which a zoom started while the tracking camera is still in its
    /// transition copies (`0x005F7D30` keeps `+0x800`).
    Tracking { id: u32, frame: Option<(Vec3, Quat)>, free: KeptFree },
    /// The intelli-zoom camera (`0x00592860`); `free` is the free camera the manager keeps
    /// meanwhile (`+0x800`) and shows again when the zoom ends.
    Zoom { free: KeptFree },
}

/// The free camera the manager keeps in `+0x800` while another camera is current: its eye, target,
/// kept height and rate (`+0x13C`). The kept camera gets no frame update (CONFIRMED `0x005FE410`
/// calls vfunc `+0x120` of the current camera `+0x814` and of the second camera `+0x81C`, never of
/// `+0x800`), so its rate stays the one its last update as the current camera stored.
#[derive(Debug, Clone, Copy, PartialEq)]
struct KeptFree {
    eye: Vec3,
    target: Vec3,
    height: f32,
    rate: f32,
}

/// A move from one view to the current camera's (the transition camera: built by its constructor
/// `0x00592410`, which sets the length, through `0x005FB4D0`; its frame update `0x005FFB00`,
/// CONFIRMED): the shown view eases from `from` to the current camera's own view.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Transition {
    from_eye: Vec3,
    from_dir: Vec3,
    from_fov: f32,
    /// Seconds of rate run so far, and the length.
    elapsed: f32,
    duration: f32,
    /// The eye, target and field of view it shows this frame.
    shown: (Vec3, Vec3, f32),
}

impl Transition {
    /// Whether it still runs: its frame update returns `duration > elapsed` (CONFIRMED
    /// `0x005FFB00`), and the manager makes its destination current once it returns false
    /// (`0x005FE410`).
    fn running(&self) -> bool {
        self.duration > self.elapsed
    }
}

/// The battle camera (BATTLE_FIDELITY.md §59.2, its modes §59.5).
#[derive(Component, Debug, Clone)]
pub struct BattleCamera {
    pub eye: Vec3,
    /// The look-at point, one metre in front of the eye once constrained.
    pub target: Vec3,
    /// The current camera's field of view (`+0x24`): [`FOV`], or the zoom's.
    fov: f32,
    mode: Mode,
    /// A transition to the current camera, if one is showing.
    transition: Option<Transition>,
    /// The track target (manager `+0x82C`, set by `SetTrackTarget` `0x005F9B70`): the unit the
    /// tracking camera follows. Never cleared, as in the exe.
    track: Option<u32>,
    /// The intelli-zoom's aim, set before its press ([`Self::set_zoom_aim`]): the ray under the
    /// cursor in the shown view (`None` while the cursor is off the window) and the units it can
    /// zoom onto, taken by the press.
    zoom_ray: Option<Vec3>,
    zoom_units: Vec<ZoomUnit>,
    /// The height above the ground the camera keeps, in [`CameraSettings::min_height`,
    /// `max_height`].
    height: f32,
    /// The time the steps are scaled by (`+0x13C`): the steady frame time the camera's last frame
    /// update (`0x00600480`, via `0x005FD7B0`) stored, which the manager's frame vfunc `0x005FE410`
    /// passes it ([`FrameTimes::next_rate`]). CONFIRMED that the steps read it; INFERRED that the
    /// keys run before that frame's update, so they use the previous frame's rate.
    rate: f32,
    /// The manager's frame-time ring the rate comes from.
    frame_times: FrameTimes,
    settings: CameraSettings,
    /// The controller's move and rotation scales (`0x00592CC0` `+0x4C` / `+0x50`, start 1).
    move_scale: f32,
    rot_scale: f32,
    /// The controller's move-fast byte (`+0x48`).
    move_fast: bool,
    /// The bookmarks, any number of slots (the exe's array at manager `+0xA00`).
    bookmarks: Vec<Option<(Vec3, Vec3)>>,
    /// A harness view (`view::camera_harness_run`): never constrained, so captures repeat.
    pub fixed: bool,
}

impl BattleCamera {
    /// A camera at `eye` looking at `target`, keeping its height above the ground there.
    pub fn new(settings: CameraSettings, eye: Vec3, target: Vec3, ground: Ground) -> Self {
        let mut cam = Self::unplaced(settings, eye, target);
        cam.place(eye, target, ground);
        cam
    }

    /// A harness view (`fixed`): exactly at `eye` looking at `target`, never constrained.
    pub fn fixed_view(eye: Vec3, target: Vec3) -> Self {
        Self { fixed: true, ..Self::unplaced(CameraSettings::default(), eye, target) }
    }

    fn unplaced(settings: CameraSettings, eye: Vec3, target: Vec3) -> Self {
        Self {
            eye,
            target,
            fov: FOV,
            mode: Mode::Free,
            transition: None,
            track: None,
            zoom_ray: None,
            zoom_units: Vec::new(),
            height: settings.min_height,
            rate: START_RATE,
            frame_times: FrameTimes::default(),
            settings,
            move_scale: 1.0,
            rot_scale: 1.0,
            move_fast: false,
            bookmarks: Vec::new(),
            fixed: false,
        }
    }

    /// Moves the camera to `eye` looking at `target` and constrains it; the height it keeps is the
    /// eye's above the ground (CONFIRMED, the type 0 camera's constructor `0x00592760`).
    pub fn place(&mut self, eye: Vec3, target: Vec3, ground: Ground) {
        (self.eye, self.target) = (eye, target);
        self.height = eye.y - ground(eye.xz());
        if !self.fixed {
            self.constrain(ground);
        }
    }

    /// The frame's end, `frame_time` the frame's real time (the battle UI's `+0x28224`). The camera
    /// manager's frame vfunc (CONFIRMED `0x005FE410`) keeps it in its ring and passes the steady
    /// mean, held in [1/120, 1] ([`FrameTimes::next_rate`]), to the camera's frame update
    /// (CONFIRMED `0x00600480`, the camera's vfunc `+0x120`): constrained first, and last that
    /// time kept as the rate of the next steps (`0x005FD7B0`). PROVISIONAL, not ported: between
    /// them it calls `0x010FBB40` on `+0x154` and on `+0x16C` with the rate (each sets three floats
    /// from `0x010FBB00` of the float, its goal three floats on, and the rate; what the fields are
    /// is not traced) and `0x006084D0` with the kept height, which sets the camera's floats `+0x3C`
    /// to 0, `+0x40` from 500 to 1500 and `+0x4C` from 2.5 to 0 (its setter vfuncs `+0x84` /
    /// `+0x88` / `+0x8C`), each lerped by where the height lies between the camera's minimum and
    /// maximum (`0x005B73E0` / `0x005B73C0`, held in [0, 1]); what reads them (getter vfuncs
    /// `+0x78` / `+0x7C` / `+0x80`) is not traced, it may belong to our own renderer (DESIGN.md
    /// §3.7.1). Not ported, debug only: the console
    /// command `battle_camera_use_game_time` (`0x005B4960`), which makes the manager pass the
    /// game time instead.
    ///
    /// The tracking camera's update (`0x005FF450`) constrains the same way after following its
    /// unit ([`Self::track_frame`]); the zoom camera's (`0x00600600`) only keeps the rate, so a
    /// zoom is never constrained. While a transition shows, its update (CONFIRMED `0x005FFB00`)
    /// runs the current camera's update first, then blends ([`Self::advance_transition`]); a
    /// transition that has ended is dropped at the next frame (the manager makes its destination
    /// current, `0x005FE410`).
    pub fn end_frame(&mut self, frame_time: f32, ground: Ground) {
        self.drop_ended_transition();
        if !self.fixed && !matches!(self.mode, Mode::Zoom { .. }) {
            self.constrain(ground);
        }
        self.rate = self.frame_times.next_rate(frame_time);
        self.advance_transition(ground);
    }

    /// The view on screen: eye, look-at target and field of view (radians, the exe's; our renderer
    /// scales its own by the ratio to [`FOV`], DESIGN.md §3.7.1). A transition's blend while one
    /// shows, else the current camera's.
    pub fn view(&self) -> (Vec3, Vec3, f32) {
        self.transition.map_or((self.eye, self.target, self.fov), |t| t.shown)
    }

    /// The field of view the renderer scales by: [`Self::view`]'s over the camera's normal one.
    pub fn zoom_ratio(&self) -> f32 {
        self.view().2 / FOV
    }

    /// The unit the tracking camera follows, while it is the current camera or the destination of
    /// a transition.
    pub fn tracked_unit(&self) -> Option<u32> {
        match self.mode {
            Mode::Tracking { id, .. } => Some(id),
            _ => None,
        }
    }

    /// The intelli-zoom's aim for its next press: the ray under the cursor in the view on screen
    /// (`None` while the cursor is off the window) and every unit of the battle (the exe's loop
    /// runs over all alliances' armies' units, `0x005F7EBE`..`0x005F802F`).
    pub fn set_zoom_aim(&mut self, ray: Option<Vec3>, units: Vec<ZoomUnit>) {
        (self.zoom_ray, self.zoom_units) = (ray, units);
    }

    /// The tracked unit this frame: its position and facing, `None` once it is gone. The tracking
    /// camera's update (CONFIRMED `0x005FF450`) refreshes the unit's matrix (`+0x194`) and its
    /// inverse (`+0x1D4`), so the eye and target ride along with the unit's moves and turns
    /// (INFERRED: the matrix is the unit's centre and facing, read through the entity's vfuncs);
    /// a gone unit (its vfunc `+0x14`) ends the update with false, and the manager then puts a
    /// free camera at the view with a transition (`0x005FE410` → `0x005CDD80`). The caller passes
    /// `None` for a gone unit and decides what gone is (`view::move_camera`, tagged there).
    pub fn track_frame(&mut self, frame: Option<(Vec3, Quat)>) {
        let Mode::Tracking { id, frame: last, free } = self.mode else { return };
        let Some((t, q)) = frame else {
            self.leave_tracking();
            return;
        };
        if let Some((t0, q0)) = last {
            let r = q * q0.inverse();
            self.eye = t + r * (self.eye - t0);
            self.target = t + r * (self.target - t0);
        }
        self.mode = Mode::Tracking { id, frame: Some((t, q)), free };
    }

    /// `SetTrackTarget` (CONFIRMED `0x005F9B70`; the battle scripts' `SetSelectionProxy`
    /// `0x005D3430`, which a unit card calls as it is selected and deselected): on, unit `id`
    /// becomes the track target, and a tracking camera that is current starts again on it; off, a
    /// current tracking camera on `id` gives way to a free one at its view, and the target is
    /// kept. The context camera's branch is not done (§59.5).
    ///
    /// `SetSelectionProxy` (CONFIRMED `0x005D3430`, BATTLE_FIDELITY.md §59.5) does only this for
    /// us: it ignores calls while the battle UI is torn down (its byte `+0x9CD`, set only by the
    /// UI manager's destructor `0x005A8FC0`; ours serves the request only while the battle UI runs,
    /// `hud::frame`) and returns no Lua values. PROVISIONAL, not ported: when the unit is the
    /// context camera's subject it also stores it as the manager's `+0x830` (the context camera
    /// is not done), and it sets the unit's battle entity's flag to `on` (`0x007C8D70`, byte
    /// `+0x88`; `+0x5D1` on the other branch), which `0x005F3E60` and the 12-value script getter
    /// `0x008590B0` read back, for what is not traced.
    pub fn set_track_target(&mut self, id: u32, on: bool) {
        if on {
            self.track = Some(id);
            if self.current_is_tracking() {
                self.start_tracking(id);
            }
        } else if self.current_is_tracking() && self.tracked_unit() == Some(id) {
            self.leave_tracking();
        }
    }

    /// `StartTracking` (CONFIRMED `0x005F7BE0`): the zoom's toggle off first (`0x005CDD90`, which
    /// acts only on a current zoom camera, `0x005F6861`: a zoom still in its transition is left in
    /// its slot unseen, and ours drops it, its free camera's height kept), then the manager copies
    /// the view on screen (`0x005F7470` of the current camera, `0x005F7C01`) and builds the
    /// tracking camera on `id` at that eye and target (`0x00591BC0`, its heading from them,
    /// `0x005922A8`), reached through a transition (`0x005FB4D0`, speed 1.0); the new camera's
    /// rate starts again (the base constructor `0x0058F3E0`, `0x0058F46E`). The free camera
    /// (`+0x800`) is kept as it was. Not ported: with the manager's camera type `+0x838` at 2 it
    /// builds another class (`0x0058F6E0`); ours has the type-0 cameras only (§59.2).
    fn start_tracking(&mut self, id: u32) {
        if self.current_is_zoom() {
            self.leave_zoom();
        }
        let from @ (eye, target, _) = self.view();
        let free = self.free_camera();
        if let Mode::Zoom { free } = self.mode {
            self.height = free.height;
        }
        (self.eye, self.target, self.rate) = (eye, target, START_RATE);
        (self.mode, self.fov) = (Mode::Tracking { id, frame: None, free }, FOV);
        self.start_transition(from);
    }

    /// The end of tracking (CONFIRMED `0x005CDD80` → `NewCameraOfType`, also the manager's way
    /// when the tracking camera's update returns false, `0x005FE410`): a new free camera at the
    /// tracking camera's view (its rate starting again, `0x0058F46E`), reached through a
    /// transition from the view on screen.
    fn leave_tracking(&mut self) {
        let from = self.view();
        (self.mode, self.fov, self.rate) = (Mode::Free, FOV, START_RATE);
        self.start_transition(from);
    }

    /// The free camera the manager keeps (`+0x800`): the current one, or the one kept while a
    /// tracking or zoom camera runs.
    fn free_camera(&self) -> KeptFree {
        match self.mode {
            Mode::Free => KeptFree { eye: self.eye, target: self.target, height: self.height, rate: self.rate },
            Mode::Tracking { free, .. } | Mode::Zoom { free } => free,
        }
    }

    /// `StartIntelliZoom` (CONFIRMED `0x005F7D30`): tracking ends first (as its toggle's off;
    /// the context camera's end is not done); the zoom camera's eye is the shown eye, its target
    /// one metre along the cursor's ray (`0x010EC7D0`, the cursor through the shown view), its
    /// field of view a quarter of the camera's ([`ZOOM_FOV`]); it takes over through a transition
    /// and its rate starts again (`0x0058F46E`). The free camera (`+0x800`) is kept as it was.
    /// Ours aims along the ray through what our renderer shows under the cursor
    /// ([`Self::set_zoom_aim`]), and along the view's own direction while the cursor is off the
    /// window (our choice: the exe always has a cursor).
    ///
    /// ORIGINAL BUG (`0x005F7E9B` / `0x005F8060`): the zoom onto the unit under the cursor never
    /// happens. The exe walks every unit keeping the largest cosine between the cursor's ray and
    /// the way to the unit's centre (a later unit wins a tie), and zooms onto the best unit when
    /// that cosine is above a threshold; but the running best starts at 1.3853649 (float
    /// `0x01331B64`) and the threshold is 1.3953649 (`0x01331B68`), both above any cosine. Ours
    /// does what the code was written for ([`zoom_onto`]: the same best-then-test structure, the
    /// running best starting below any cosine, our threshold).
    fn start_zoom(&mut self) {
        if self.current_is_tracking() {
            self.leave_tracking();
        }
        let from @ (eye, target, _) = self.view();
        let free = self.free_camera();
        let units = std::mem::take(&mut self.zoom_units);
        let ray = self.zoom_ray.and_then(Vec3::try_normalize).or((target - eye).try_normalize()).unwrap_or(Vec3::NEG_Z);
        let (target, fov) = zoom_onto(eye, ray, &units).unwrap_or((eye + ray, ZOOM_FOV));
        (self.eye, self.target, self.fov, self.rate) = (eye, target, fov, START_RATE);
        self.mode = Mode::Zoom { free };
        self.start_transition(from);
    }

    /// The end of the zoom (CONFIRMED `0x005CDD90`, and the leave flag through `0x005F9370`): the
    /// zoom camera is freed and the free camera, kept as it was, is current again at once, its
    /// rate the one it had (the manager's frame does not update the kept camera `+0x800`, `0x005FE410`).
    /// PROVISIONAL, not ported: `0x005F9370` also calls vfunc `+0x4C` of the battle UI's
    /// `+0x2804C` object (`0x00611A50`) with 0, which the zoom camera's constructor called with 1
    /// (`0x005928D0`); what it switches is not traced.
    fn leave_zoom(&mut self) {
        if let Mode::Zoom { free } = self.mode {
            (self.eye, self.target, self.height, self.rate) = (free.eye, free.target, free.height, free.rate);
        }
        (self.mode, self.fov, self.transition) = (Mode::Free, FOV, None);
    }

    fn current_is_tracking(&self) -> bool {
        matches!(self.mode, Mode::Tracking { .. }) && self.transition.is_none()
    }

    fn current_is_zoom(&self) -> bool {
        matches!(self.mode, Mode::Zoom { .. }) && self.transition.is_none()
    }

    /// Drops a transition that has ended: its destination is the current camera now.
    fn drop_ended_transition(&mut self) {
        if self.transition.is_some_and(|t| !t.running()) {
            self.transition = None;
        }
    }

    /// Starts a transition from `from` (eye, target, field of view) to the current camera's view
    /// (CONFIRMED `0x005FB4D0` → the transition camera's constructor `0x00592410`, which sets the
    /// length; `0x005FFB00` runs it): the length from how far the eye moves and how far
    /// the view turns (the angle between the two directions, its cosine held in [0, 1], so a turn
    /// of more than 90° counts as 90°).
    fn start_transition(&mut self, (from_eye, from_target, from_fov): (Vec3, Vec3, f32)) {
        let from_dir = from_target - from_eye;
        let cos = from_dir.normalize_or_zero().dot((self.target - self.eye).normalize_or_zero());
        let turn = cos.clamp(0.0, 1.0).acos() * 3.0 / std::f32::consts::PI;
        let duration = (from_eye.distance(self.eye) * TRANSITION_PER_METRE).max(turn).clamp(TRANSITION_MIN, TRANSITION_MAX);
        self.transition = Some(Transition {
            from_eye,
            from_dir,
            from_fov,
            elapsed: 0.0,
            duration,
            shown: (from_eye, from_target, from_fov),
        });
    }

    /// One frame of a transition (CONFIRMED `0x005FFB00`): r = sin(elapsed / length × π/2) held
    /// in [0, 1] (its ease byte `+0x15C` is 0 for every start here); the eye and the field of view
    /// blend linearly; the direction's yaw turns the short way round and its pitch and length
    /// blend; the target is the blended eye plus that direction, and only then is the eye lifted
    /// to at least 2 m over the ground; last, the elapsed time grows by the rate.
    fn advance_transition(&mut self, ground: Ground) {
        let Some(t) = self.transition.as_mut() else { return };
        let r = (t.elapsed / t.duration * std::f32::consts::FRAC_PI_2).sin().clamp(0.0, 1.0);
        let mut eye = t.from_eye.lerp(self.eye, r);
        let fov = t.from_fov + (self.fov - t.from_fov) * r;
        let (yaw0, pitch0, len0) = yaw_pitch_len(t.from_dir);
        let (yaw1, pitch1, len1) = yaw_pitch_len(self.target - self.eye);
        let tau = std::f32::consts::TAU;
        let dyaw = (yaw1 - yaw0 + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
        let (yaw, pitch, len) = (yaw0 + dyaw * r, pitch0 + (pitch1 - pitch0) * r, len0 + (len1 - len0) * r);
        let target = eye + len * Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), -pitch.cos() * yaw.cos());
        eye.y = eye.y.max(ground(eye.xz()) + TRANSITION_CLEARANCE);
        t.shown = (eye, target, fov);
        t.elapsed += self.rate;
    }

    /// The mouse wheel. PROVISIONAL, ours: changes the height by a tenth per notch. The original's
    /// is the mouse handler `0x005EE600` (BATTLE_FIDELITY.md §59.4: an up step, `0x005E5C30`,
    /// at the fixed rate 1/30 by the wheel delta × a preference × 0.00025), not ported yet; nor
    /// which camera it reaches while another is current, so ours leaves the zoom alone.
    pub fn wheel(&mut self, notches: f32, ground: Ground) {
        if matches!(self.mode, Mode::Zoom { .. }) {
            return;
        }
        self.height *= 1.0 - notches * 0.1;
        if !self.fixed {
            self.constrain(ground);
        }
    }

    /// One press of a camera func, `time` its press time (CONFIRMED `0x005EAF00`): a step moves by
    /// (2 with move-fast, else 1) × the move scale (`0x005B7AE0`; INFERRED: its other byte `+0x21`
    /// is clear in a battle, which would invert move-fast), a turn by the rotation scale × 0.5;
    /// `accel` / `decel` and `increase` / `decrease_camera_rot` scale by e^time, held in
    /// [0.01, 2]; the time the exe reads, battle UI `+0x28224`, is the frame's time in seconds
    /// (CONFIRMED: the battle UI's frame update `0x005F5530` stores the frame-time struct's delta
    /// seconds there and hands it to the camera manager's frame update).
    ///
    /// The steps, rises, turns and tilts go to the current camera (BATTLE_FIDELITY.md §59.5,
    /// CONFIRMED by its vtables): a transition's all set its leave flag (`+0x138`, vfunc
    /// `+0x100`: `0x005F9360`; through `0x005E5540` and `0x005E5C20`), so the press only ends it
    /// and its destination becomes current (`+0x810`, set by `0x005FB4D0` at `0x005FB4E1`, made
    /// current by `0x005FE410`); the
    /// zoom camera's up and down change its field of view (`0x005E5CD0`) and the rest end the zoom
    /// at once (`0x005F9370`; the free view comes back, no transition); the tracking camera's
    /// horizontal steps end the tracking (a free camera at the view, with a transition), its
    /// rises and turns are the free camera's (`0x005E5C30`; `0x005FB9B0`, the same turn) and its
    /// tilt is a rise ([`TRACK_TILT_RISE`]).
    pub fn key(&mut self, func: CameraFunc, time: f32, ground: Ground) {
        self.drop_ended_transition();
        let step = if self.move_fast { 2.0 } else { 1.0 } * self.move_scale;
        let turn = self.rot_scale * 0.5;
        let tilt = if self.settings.invert_tilt { -turn } else { turn };
        let yaw = if self.settings.invert_turn { -turn } else { turn };
        let (fwd, right) = self.axes();
        let horizontal = matches!(func, CameraFunc::StepFwd | CameraFunc::StepFwdFast | CameraFunc::StepBck | CameraFunc::StepL | CameraFunc::StepR);
        let vertical = matches!(func, CameraFunc::CamUp | CameraFunc::CamDown);
        let rotation = matches!(func, CameraFunc::RotL | CameraFunc::RotR | CameraFunc::RotU | CameraFunc::RotD);
        if horizontal || vertical || rotation {
            if self.transition.is_some() {
                self.transition = None;
                return;
            }
            match self.mode {
                Mode::Zoom { .. } if vertical => {
                    let arg = if func == CameraFunc::CamUp { step } else { -step };
                    self.fov = (self.fov + arg * ZOOM_FOV_STEP).clamp(ZOOM_FOV_MIN, ZOOM_FOV_MAX);
                    return;
                }
                Mode::Zoom { .. } => {
                    self.leave_zoom();
                    return;
                }
                Mode::Tracking { .. } if horizontal => {
                    self.leave_tracking();
                    return;
                }
                Mode::Tracking { .. } if matches!(func, CameraFunc::RotU | CameraFunc::RotD) => {
                    self.rise(if func == CameraFunc::RotU { tilt } else { -tilt } * TRACK_TILT_RISE);
                    if !self.fixed {
                        self.constrain(ground);
                    }
                    return;
                }
                _ => {}
            }
        }
        match func {
            CameraFunc::StepFwd => self.pan(fwd, step),
            CameraFunc::StepFwdFast => self.pan(fwd, step + step),
            CameraFunc::StepBck => self.pan(fwd, -step),
            CameraFunc::StepR => self.pan(right, step),
            CameraFunc::StepL => self.pan(right, -step),
            CameraFunc::CamUp => self.rise(step),
            CameraFunc::CamDown => self.rise(-step),
            CameraFunc::RotR => self.turn(yaw),
            CameraFunc::RotL => self.turn(-yaw),
            CameraFunc::RotU => self.tilt(tilt),
            CameraFunc::RotD => self.tilt(-tilt),
            CameraFunc::Accel => self.move_scale = (self.move_scale * time.exp()).clamp(MIN_SCALE, MAX_SCALE),
            CameraFunc::Decel => self.move_scale = (self.move_scale * (-time).exp()).clamp(MIN_SCALE, MAX_SCALE),
            CameraFunc::IncreaseRot => self.rot_scale = (self.rot_scale * time.exp()).clamp(MIN_SCALE, MAX_SCALE),
            CameraFunc::DecreaseRot => self.rot_scale = (self.rot_scale * (-time).exp()).clamp(MIN_SCALE, MAX_SCALE),
            CameraFunc::ToggleMoveFast => self.move_fast = !self.move_fast,
            CameraFunc::BookmarkSave(n) => {
                // CONFIRMED `0x005F35D0`: the slot's old view is freed, the current camera's
                // (`+0x814`) eye and target kept: the view on screen, during a transition its blend
                // (its getters `0x005EDAF0` / `0x005F9020` read `+0x08` / `+0x14`, which
                // `0x005FFB00` writes).
                if self.bookmarks.len() <= n {
                    self.bookmarks.resize(n + 1, None);
                }
                let (eye, target, _) = self.view();
                self.bookmarks[n] = Some((eye, target));
            }
            CameraFunc::BookmarkView(n) => {
                // CONFIRMED `0x00610770`: an empty slot does nothing; otherwise a new camera at
                // the kept view (`0x00592760`: rate back to its start by the base constructor
                // `0x0058F3E0`) in the free slot `+0x800`, made current through a transition that
                // starts ended (its flag 0: elapsed = length, `0x00592738`), so in place at once;
                // a tracking, zoom or transition camera is no longer current.
                if let Some(&Some((eye, target))) = self.bookmarks.get(n) {
                    (self.mode, self.transition, self.fov) = (Mode::Free, None, FOV);
                    self.place(eye, target, ground);
                    self.rate = START_RATE;
                }
                return;
            }
            CameraFunc::ToggleTracking => {
                // CONFIRMED `0x005F67B0`, which the handler `0x005EAF00` calls with on unless the
                // tracking camera is current: on starts it when a target is set, off ends it.
                if !self.current_is_tracking() {
                    if let Some(id) = self.track {
                        self.start_tracking(id);
                    }
                } else {
                    self.leave_tracking();
                }
                return;
            }
            CameraFunc::IntelliZoom => {
                // CONFIRMED `0x005F6840`: on unless the zoom camera is current; off
                // (`0x005CDD90`) shows the free camera again at once.
                if self.current_is_zoom() {
                    // The press's units are only for an on press.
                    self.zoom_units = Vec::new();
                    self.leave_zoom();
                } else {
                    self.start_zoom();
                }
                return;
            }
        }
        if !self.fixed {
            self.constrain(ground);
        }
    }

    /// The horizontal forward and right unit vectors of the view.
    fn axes(&self) -> (Vec3, Vec3) {
        let d = self.target - self.eye;
        let fwd = Vec3::new(d.x, 0.0, d.z).try_normalize().unwrap_or(Vec3::NEG_Z);
        (fwd, fwd.cross(Vec3::Y))
    }

    /// A step along `axis` (CONFIRMED `0x005E5550`, the one horizontal step: forward, back, left and
    /// right pass their axis, negated for back and left): eye and target move by `arg` × the pan
    /// speed `0x005E55C0` = (the minimum height + (top − bottom move speed) × t) × rate, t the kept
    /// height's place between its limits; then the constraint (vfunc `+0x124`, run by `key`). The
    /// base term is the minimum height, not the bottom move speed (CONFIRMED in the code
    /// `0x005E565C`..`0x005E5674`; kept: nothing shows it is a slip). The exe's rate is the
    /// alternative one (`+0x144`) while byte `+0x140` is set, which only the wheel and the
    /// `BCQ_FORCE_TICK` command do, never the keys (CONFIRMED, §59.4).
    fn pan(&mut self, axis: Vec3, arg: f32) {
        let s = &self.settings;
        let h = self.height.clamp(s.min_height, s.max_height);
        let t = (h - s.min_height) / (s.max_height - s.min_height);
        let speed = (s.min_height + (s.move_max - s.move_min) * t) * self.rate;
        let d = axis * arg * speed;
        self.eye += d;
        self.target += d;
    }

    /// Up or down (CONFIRMED `0x005E5C30`, the one vertical step, down passing a negated arg): by
    /// `arg` × the rise speed `0x005E5D30` = |h × ((max / min)^rate − 1)|, h the kept height within
    /// its limits, so the height grows by a fixed factor per second; the kept height moves with
    /// the eye. The rate is the alternative one while `+0x140` is set, as for [`Self::pan`].
    fn rise(&mut self, arg: f32) {
        let s = &self.settings;
        let h = self.height.clamp(s.min_height, s.max_height);
        let v = (h * ((s.max_height / s.min_height).powf(self.rate) - 1.0)).abs() * arg;
        self.height += v;
        self.eye.y += v;
        self.target.y += v;
    }

    /// A turn about the eye by turn speed × rate × `arg` × 0.5 × field of view × aspect radians
    /// (CONFIRMED `0x005FBA10`; the rate is the alternative one while `+0x140` is set, as for
    /// [`Self::pan`]). Positive turns to the right: INFERRED for the exe (`rot_r` passes the
    /// positive arg to vfunc `+0x114`; which way that turns on screen is not verified).
    fn turn(&mut self, arg: f32) {
        let angle = self.settings.turn * self.rate * arg * 0.5 * FOV * ASPECT;
        self.target = self.eye + Quat::from_rotation_y(-angle) * (self.target - self.eye);
    }

    /// A tilt, positive up (CONFIRMED `0x005F9890`): the elevation grows by turn speed × rate ×
    /// `arg` × 0.25 × field of view radians.
    fn tilt(&mut self, arg: f32) {
        let angle = self.settings.turn * self.rate * arg * 0.25 * FOV;
        let (fwd, _) = self.axes();
        let dir = (self.target - self.eye).try_normalize().unwrap_or(fwd);
        let e = dir.y.clamp(-1.0, 1.0).asin() + angle;
        self.target = self.eye + fwd * e.cos() + Vec3::Y * e.sin();
    }

    /// Keeps the camera in bounds (CONFIRMED `0x005B3DF0`, after every step and every frame): the
    /// kept height within its limits; the view one metre long with its elevation within
    /// ±[`MAX_ELEVATION`]; the eye at the kept height above the ground, and never lower than the
    /// minimum height above the ground one metre ahead to the left and right (`0x005D79A0`); the
    /// target moved up or down with it. PROVISIONAL: the exe also keeps the eye inside the
    /// battle's camera rectangle (manager `+0x2EC`..`+0x2F8`, copied by `0x00590B20` from the
    /// battle's data, whose source is not traced); ours has no such bound.
    pub fn constrain(&mut self, ground: Ground) {
        let s = self.settings;
        self.height = self.height.clamp(s.min_height, s.max_height);
        let (fwd, right) = self.axes();
        let dir = (self.target - self.eye).try_normalize().unwrap_or(Vec3::NEG_Z);
        let e = dir.y.clamp(-1.0, 1.0).asin().clamp(-MAX_ELEVATION, MAX_ELEVATION);
        self.target = self.eye + fwd * e.cos() + Vec3::Y * e.sin();
        let g = ground(self.eye.xz());
        let ahead = self.eye + fwd;
        let clearance = ground((ahead + right).xz()).max(ground((ahead - right).xz()));
        let y = g + self.height.max(clearance - g + s.min_height);
        let dy = y - self.eye.y;
        self.eye.y = y;
        self.target.y += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_: Vec2) -> f32 {
        0.0
    }

    /// A camera `h` metres over flat ground looking north (Bevy −z) along the horizon.
    fn cam(h: f32) -> BattleCamera {
        let mut c = BattleCamera::new(CameraSettings::default(), Vec3::new(0.0, h, 0.0), Vec3::new(0.0, h, -1.0), &flat);
        c.rate = 1.0;
        c
    }

    fn yaw_of(c: &BattleCamera) -> f32 {
        let d = c.target - c.eye;
        d.x.atan2(-d.z)
    }

    #[test]
    fn pan_speed_runs_from_the_minimum_height_to_its_sum_with_the_speed_range() {
        let mut c = cam(2.5);
        c.key(CameraFunc::StepFwd, 0.0, &flat);
        assert!((c.eye.z + 2.5).abs() < 1e-4, "{:?}", c.eye);
        let mut c = cam(60.0);
        c.key(CameraFunc::StepR, 0.0, &flat);
        assert!((c.eye.x - 92.5).abs() < 1e-3, "{:?}", c.eye);
        let mut c = cam(60.0);
        c.key(CameraFunc::StepFwdFast, 0.0, &flat);
        assert!((c.eye.z + 185.0).abs() < 1e-3, "{:?}", c.eye);
        c.key(CameraFunc::ToggleMoveFast, 0.0, &flat);
        c.key(CameraFunc::StepBck, 0.0, &flat);
        assert!((c.eye.z + 0.0).abs() < 1e-3, "move-fast doubles the back step: {:?}", c.eye);
    }

    #[test]
    fn height_grows_by_the_limits_ratio_per_second() {
        let mut c = cam(5.0);
        c.rate = 0.5;
        c.key(CameraFunc::CamUp, 0.0, &flat);
        // 5 × 24^0.5 ≈ 24.49.
        assert!((c.height - 5.0 * 24f32.sqrt()).abs() < 1e-3, "{}", c.height);
        assert!((c.eye.y - c.height).abs() < 1e-4);
        c.key(CameraFunc::CamUp, 0.0, &flat);
        assert_eq!(c.height, 60.0, "held at the top");
        let mut c = cam(3.0);
        c.key(CameraFunc::CamDown, 0.0, &flat);
        assert_eq!(c.height, 2.5, "held at the bottom");
    }

    /// A new camera's steps use the start rate 0.01 (`0x0058F3E0`); after a frame update, the frame
    /// time it kept (`0x00600480` → `0x005FD7B0`). Down is the same vertical step as up with the
    /// argument negated (`0x005E5C30`), so from one height both move by the same amount.
    #[test]
    fn steps_use_the_rate_the_last_frame_kept() {
        let mut c = BattleCamera::new(CameraSettings::default(), Vec3::new(0.0, 2.5, 0.0), Vec3::new(0.0, 2.5, -1.0), &flat);
        c.key(CameraFunc::StepFwd, 0.0, &flat);
        assert!((c.eye.z + 2.5 * START_RATE).abs() < 1e-6, "{:?}", c.eye);
        c.end_frame(0.2, &flat);
        c.key(CameraFunc::StepFwd, 0.0, &flat);
        assert!((c.eye.z + 2.5 * (START_RATE + 0.2)).abs() < 1e-5, "{:?}", c.eye);
        let (mut up, mut down) = (cam(10.0), cam(10.0));
        (up.rate, down.rate) = (0.01, 0.01);
        up.key(CameraFunc::CamUp, 0.0, &flat);
        down.key(CameraFunc::CamDown, 0.0, &flat);
        assert!(((up.height - 10.0) - (10.0 - down.height)).abs() < 1e-4, "{} {}", up.height, down.height);
    }

    /// The rate at a steady frame rate is its frame time; the ring keeps only the last 100 frames.
    #[test]
    fn steady_frames_give_their_time() {
        let mut c = cam(10.0);
        for _ in 0..100 {
            c.end_frame(0.05, &flat);
        }
        assert!((c.rate - 0.05).abs() < 1e-7, "{}", c.rate);
        for _ in 0..150 {
            c.end_frame(1.0 / 60.0, &flat);
        }
        assert!((c.rate - 1.0 / 60.0).abs() < 1e-7, "the older frames are overwritten: {}", c.rate);
        assert_eq!(c.frame_times.count, FRAME_TIMES);
    }

    /// A hitch lies outside one standard deviation and is left out (`0x005F3790`): the rate stays
    /// the steady frame time, where the plain mean would be 0.0195.
    #[test]
    fn a_hitch_is_left_out() {
        let mut c = cam(10.0);
        for _ in 0..99 {
            c.end_frame(1.0 / 60.0, &flat);
        }
        c.end_frame(0.3, &flat);
        assert!((c.rate - 1.0 / 60.0).abs() < 1e-7, "{}", c.rate);
        // Before the ring is full only the kept frames count: the first frame's time alone, then
        // the empty slots' zeros never pull the mean down.
        let mut c = cam(10.0);
        c.end_frame(0.04, &flat);
        assert_eq!(c.rate, 0.04);
        c.end_frame(0.05, &flat);
        c.end_frame(0.2, &flat);
        // Mean 0.0967, deviation 0.0732: 0.2 is left out, 0.04 and 0.05 kept.
        assert!((c.rate - 0.045).abs() < 1e-7, "{}", c.rate);
    }

    /// The rate is held in [1/120, 1] (`0x005FE4F8`..`0x005FE520`), so above 120 fps the camera moves
    /// as at 120; a mean that is not a number gives 1.
    #[test]
    fn the_rate_is_held_from_1_120_to_1() {
        assert_eq!(MIN_RATE.to_bits(), 0x3C08_8889, "the exe's float 0x01331B00");
        let mut c = cam(10.0);
        for _ in 0..10 {
            c.end_frame(1.0 / 144.0, &flat);
        }
        assert_eq!(c.rate, MIN_RATE);
        assert_eq!(held_rate(1.5), 1.0);
        assert_eq!(held_rate(f32::NAN), 1.0);
        assert_eq!(held_rate(0.02), 0.02);
    }

    #[test]
    fn turns_and_tilts_at_the_default_speeds() {
        let mut c = cam(20.0);
        c.rate = 0.1;
        c.key(CameraFunc::RotR, 0.0, &flat);
        let rad_s = yaw_of(&c) / 0.1;
        assert!((rad_s - 2.036).abs() < 1e-3, "yaw {rad_s} rad/s");
        c.key(CameraFunc::RotL, 0.0, &flat);
        assert!(yaw_of(&c).abs() < 1e-5);
        c.key(CameraFunc::RotU, 0.0, &flat);
        let e = (c.target - c.eye).normalize().y.asin() / 0.1;
        assert!((e - 0.7636).abs() < 1e-3, "pitch {e} rad/s");
        assert!(((c.target - c.eye).length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn invert_flags_flip_the_turn_and_the_tilt() {
        let prefs = Preferences::parse("invert_cam_x_axis true;\ninvert_cam_y_axis true;\n");
        let s = CameraSettings::from_prefs(&prefs);
        assert!(s.invert_tilt && s.invert_turn);
        let mut c = BattleCamera::new(s, Vec3::new(0.0, 20.0, 0.0), Vec3::new(0.0, 20.0, -1.0), &flat);
        c.rate = 0.1;
        c.key(CameraFunc::RotR, 0.0, &flat);
        assert!(yaw_of(&c) < 0.0);
        c.key(CameraFunc::RotU, 0.0, &flat);
        assert!(c.target.y < c.eye.y);
    }

    #[test]
    fn preferences_set_the_speeds() {
        let s = CameraSettings::from_prefs(&Preferences::parse("camera_move_speed 50;\ncamera_turn_speed 2;\n"));
        assert_eq!((s.move_max, s.move_min, s.turn), (50.0, 5.0, 2.0));
        assert_eq!(CameraSettings::from_prefs(&Preferences::parse("")), CameraSettings::default());
    }

    #[test]
    fn elevation_is_held_within_its_limit() {
        let mut c = cam(30.0);
        for _ in 0..10 {
            c.key(CameraFunc::RotD, 0.0, &flat);
        }
        let e = (c.target - c.eye).normalize().y.asin();
        assert!((e + MAX_ELEVATION).abs() < 1e-5, "{e}");
    }

    #[test]
    fn the_eye_clears_the_ground_ahead() {
        // A wall 1 m ahead (Bevy −z), 10 m high.
        let wall = |p: Vec2| if p.y < -0.5 { 10.0 } else { 0.0 };
        let c = BattleCamera::new(CameraSettings::default(), Vec3::new(0.0, 3.0, 0.0), Vec3::new(0.0, 3.0, -1.0), &wall);
        assert_eq!(c.height, 3.0);
        assert!((c.eye.y - 12.5).abs() < 1e-5, "{:?}", c.eye);
        assert!((c.target.y - 12.5).abs() < 1e-5, "{:?}", c.target);
    }

    #[test]
    fn scales_change_by_e_to_the_time_within_their_range() {
        let mut c = cam(10.0);
        c.key(CameraFunc::Accel, 0.5, &flat);
        assert!((c.move_scale - 0.5f32.exp()).abs() < 1e-5);
        c.key(CameraFunc::Accel, 5.0, &flat);
        assert_eq!(c.move_scale, MAX_SCALE);
        c.key(CameraFunc::Decel, 10.0, &flat);
        assert_eq!(c.move_scale, MIN_SCALE);
        c.key(CameraFunc::DecreaseRot, 0.25, &flat);
        assert!((c.rot_scale - (-0.25f32).exp()).abs() < 1e-5);
        c.key(CameraFunc::IncreaseRot, 9.0, &flat);
        assert_eq!(c.rot_scale, MAX_SCALE);
    }

    #[test]
    fn bookmarks_keep_and_restore_the_view() {
        let mut c = cam(10.0);
        c.key(CameraFunc::BookmarkView(2), 0.0, &flat);
        assert_eq!(c.eye, Vec3::new(0.0, 10.0, 0.0), "an empty slot does nothing");
        c.key(CameraFunc::BookmarkSave(7), 0.0, &flat);
        let saved = (c.eye, c.target);
        c.key(CameraFunc::StepR, 0.0, &flat);
        c.key(CameraFunc::CamUp, 0.0, &flat);
        assert_ne!(c.eye, saved.0);
        c.key(CameraFunc::BookmarkView(7), 0.0, &flat);
        assert_eq!((c.eye, c.target), saved);
        assert_eq!(c.height, 10.0);
        assert_eq!(c.rate, START_RATE);
    }

    #[test]
    fn bindings_name_the_funcs() {
        assert_eq!(CameraFunc::from_binding("accel"), Some(CameraFunc::Accel));
        assert_eq!(CameraFunc::from_binding("decrease_camera_rot"), Some(CameraFunc::DecreaseRot));
        assert_eq!(CameraFunc::from_binding("toggle_move_fast"), Some(CameraFunc::ToggleMoveFast));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_save3"), Some(CameraFunc::BookmarkSave(3)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_view12"), Some(CameraFunc::BookmarkView(1)));
        assert_eq!(CameraFunc::from_binding("camera_bookmark_viewx"), Some(CameraFunc::BookmarkView(0)));
        assert_eq!(CameraFunc::from_binding("toggle_tracking"), Some(CameraFunc::ToggleTracking));
        assert_eq!(CameraFunc::from_binding("intelli_zoom"), Some(CameraFunc::IntelliZoom));
        assert_eq!(CameraFunc::from_binding("context_camera"), None);
        assert_eq!(CameraFunc::Accel.input_name(), None);
        assert_eq!(CameraFunc::RotD.input_name(), Some("rotate down"));
        assert_eq!(CameraFunc::IntelliZoom.input_name(), None);
    }

    /// Runs frames at the rate 0.1 until no transition shows (at most 3 s).
    fn settle(c: &mut BattleCamera) {
        for _ in 0..40 {
            c.end_frame(0.1, &flat);
        }
        assert!(c.transition.is_none());
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        a.distance(b) < 1e-4
    }

    #[test]
    fn tracking_needs_a_target_and_toggles() {
        let mut c = cam(10.0);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        assert_eq!(c.mode, Mode::Free, "no track target: nothing");
        assert!(c.transition.is_none());
        c.set_track_target(4, true);
        assert_eq!(c.mode, Mode::Free, "setting the target alone does not track");
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        assert_eq!(c.tracked_unit(), Some(4));
        assert!(c.transition.is_some(), "tracking starts through a transition");
        // While its transition shows, the tracking camera is not current: the toggle starts it
        // again rather than ending it.
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        assert_eq!(c.tracked_unit(), Some(4));
        settle(&mut c);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        assert_eq!(c.mode, Mode::Free);
        assert!(c.transition.is_some(), "the free camera takes over through a transition");
        assert_eq!(c.track, Some(4), "the target is kept");
    }

    #[test]
    fn tracking_rides_with_the_unit() {
        let mut c = cam(10.0);
        c.set_track_target(1, true);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        settle(&mut c);
        let unit = Vec3::new(0.0, 0.0, -20.0);
        c.track_frame(Some((unit, Quat::IDENTITY)));
        assert_eq!(c.eye, Vec3::new(0.0, 10.0, 0.0), "the first frame only records the unit");
        // The unit moves 5 m east and turns 90° to the left (about +y): the eye keeps its place
        // relative to the unit.
        let q = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        c.track_frame(Some((unit + Vec3::X * 5.0, q)));
        c.end_frame(0.1, &flat);
        let expect = unit + Vec3::X * 5.0 + q * Vec3::new(0.0, 10.0, 20.0);
        assert!(close(c.eye, expect), "{:?} vs {expect:?}", c.eye);
        assert!(close(c.target - c.eye, q * Vec3::NEG_Z), "{:?}", c.target - c.eye);
        assert_eq!(c.eye.y, 10.0, "still at its height over the ground");
        // The unit is gone: a free camera at the view, through a transition.
        let eye = c.eye;
        c.track_frame(None);
        assert_eq!(c.mode, Mode::Free);
        assert!(c.transition.is_some());
        assert_eq!(c.eye, eye);
    }

    #[test]
    fn tracking_keys() {
        let mut c = cam(10.0);
        c.set_track_target(1, true);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        // A press during the transition only ends it: the tracking camera is current, unmoved.
        let eye = c.eye;
        c.key(CameraFunc::StepFwd, 0.0, &flat);
        assert!(c.transition.is_none());
        assert_eq!((c.tracked_unit(), c.eye), (Some(1), eye));
        // The tilt is a rise by ten times its argument (0.5 at the start scales).
        c.rate = 0.01;
        let mut free = c.clone();
        free.mode = Mode::Free;
        free.rise(0.5 * TRACK_TILT_RISE);
        free.constrain(&flat);
        c.key(CameraFunc::RotU, 0.0, &flat);
        assert!(c.height > 10.0);
        assert_eq!(c.height, free.height);
        assert!(close(c.target - c.eye, Vec3::NEG_Z), "no tilt");
        // Turns and rises are the free camera's.
        let mut free = c.clone();
        free.mode = Mode::Free;
        c.key(CameraFunc::RotR, 0.0, &flat);
        free.key(CameraFunc::RotR, 0.0, &flat);
        assert_eq!((c.eye, c.target), (free.eye, free.target));
        assert_eq!(c.tracked_unit(), Some(1));
        // A horizontal step ends the tracking without moving.
        let eye = c.eye;
        c.key(CameraFunc::StepL, 0.0, &flat);
        assert_eq!((c.mode, c.eye), (Mode::Free, eye));
    }

    #[test]
    fn selection_proxy_sets_and_drops_the_target() {
        let mut c = cam(10.0);
        c.set_track_target(1, true);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        settle(&mut c);
        c.set_track_target(2, true);
        assert_eq!(c.tracked_unit(), Some(2), "a current tracking camera moves to the new target");
        settle(&mut c);
        c.set_track_target(1, false);
        assert_eq!(c.tracked_unit(), Some(2), "another unit's deselection changes nothing");
        c.set_track_target(2, false);
        assert_eq!(c.mode, Mode::Free);
        assert_eq!(c.track, Some(2));
    }

    #[test]
    fn intelli_zoom_aims_along_the_cursor_and_cuts_back() {
        let mut c = cam(10.0);
        let dir = Vec3::new(1.0, -1.0, -1.0).normalize();
        c.rate = 0.1;
        // A unit off the ray is not zoomed onto.
        c.set_zoom_aim(Some(dir * 3.0), vec![ZoomUnit { centre: Vec3::new(-20.0, 0.0, -20.0), radius: 5.0 }]);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert!(close(c.target - c.eye, dir));
        assert_eq!(c.eye, Vec3::new(0.0, 10.0, 0.0));
        assert_eq!(c.fov, FOV * 0.25);
        assert_eq!(c.rate, START_RATE, "a new camera's rate");
        assert!(c.zoom_units.is_empty(), "the press takes the units");
        // The transition eases the field of view down: after one frame it is still the camera's.
        c.end_frame(0.1, &flat);
        assert_eq!(c.view().2, FOV);
        c.end_frame(0.1, &flat);
        assert!(c.view().2 < FOV && c.view().2 > ZOOM_FOV);
        settle(&mut c);
        assert_eq!(c.view(), (c.eye, c.target, ZOOM_FOV));
        assert!(close(c.target - c.eye, dir), "the zoom is never constrained");
        assert!((c.zoom_ratio() - 0.25).abs() < 1e-6);
        // Up and down change the field of view, held in [5°, 85°].
        c.key(CameraFunc::CamUp, 0.0, &flat);
        assert!((c.fov - (ZOOM_FOV + 0.005)).abs() < 1e-6);
        for _ in 0..400 {
            c.key(CameraFunc::CamDown, 0.0, &flat);
        }
        assert_eq!(c.fov, ZOOM_FOV_MIN);
        // A turn ends the zoom at once: the free view as it was.
        c.key(CameraFunc::RotL, 0.0, &flat);
        assert_eq!((c.mode, c.transition), (Mode::Free, None));
        assert_eq!(c.view(), (Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 10.0, -1.0), FOV));
        // The toggle ends it the same way.
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        settle(&mut c);
        c.set_zoom_aim(Some(dir), vec![ZoomUnit { centre: Vec3::new(-20.0, 0.0, -20.0), radius: 5.0 }]);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!(c.view(), (Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 10.0, -1.0), FOV));
        assert!(c.zoom_units.is_empty(), "an off press drops the units too");
    }

    /// ORIGINAL BUG `0x005F8060`: the exe never zooms onto a unit; ours zooms onto the unit the
    /// cursor's ray passes over, the largest cosine winning and a later unit a tie.
    #[test]
    fn intelli_zoom_zooms_onto_the_unit_under_the_cursor() {
        let eye = Vec3::new(0.0, 10.0, 0.0);
        let near = ZoomUnit { centre: Vec3::new(0.0, 0.0, -30.0), radius: 8.0 };
        let ray = (near.centre - eye + Vec3::X * 2.0).normalize();
        let d = near.centre.distance(eye);
        // Twice as far on the same line from the eye: the cosines tie exactly.
        let behind = ZoomUnit { centre: eye + (near.centre - eye) * 2.0, radius: 8.0 };
        let off = ZoomUnit { centre: Vec3::new(40.0, 0.0, 0.0), radius: 8.0 };
        let got = zoom_onto(eye, ray, &[off, near, behind]).unwrap();
        assert_eq!(got.0, behind.centre, "a tie goes to the later unit");
        let got = zoom_onto(eye, ray, &[behind, near, off]).unwrap();
        assert_eq!(got, (near.centre, 0.5 * 8.0f32.atan2(d)));
        assert_eq!(zoom_onto(eye, ray, &[off]), None, "outside its radius");
        // As the exe (`0x005F8060`): only the best cosine is tested. A small unit nearer the ray
        // than `wide` but missing it hides `wide`, whose radius holds the ray.
        let aim = Vec3::new(0.0, 0.0, -30.0);
        let ray2 = (aim - eye).normalize();
        let small = ZoomUnit { centre: aim + Vec3::X, radius: 0.1 };
        let wide = ZoomUnit { centre: aim - Vec3::X * 4.0, radius: 8.0 };
        assert!(zoom_onto(eye, ray2, &[wide]).is_some(), "alone, its radius holds the ray");
        assert_eq!(zoom_onto(eye, ray2, &[wide, small]), None, "the best fails, so no unit");
        assert_eq!(zoom_onto(eye, ray2, &[small, wide]), None, "in either order");
        // The field of view is held at 5° or more.
        let far = ZoomUnit { centre: eye + ray * 2000.0, radius: 1.0 };
        assert_eq!(zoom_onto(eye, ray, &[far]).unwrap().1, ZOOM_FOV_MIN);
        // Through the key: the zoom camera looks at the unit's centre with that field of view.
        let mut c = cam(10.0);
        c.set_zoom_aim(Some(ray), vec![off, near]);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!((c.eye, c.target, c.fov), (eye, near.centre, 0.5 * 8.0f32.atan2(d)));
        settle(&mut c);
        assert_eq!(c.view(), (eye, near.centre, 0.5 * 8.0f32.atan2(d)));
    }

    /// A zoom started while the tracking camera is still in its transition keeps the free camera
    /// the manager kept (`+0x800`), not the tracking camera's view.
    #[test]
    fn zoom_during_the_tracking_transition_keeps_the_free_camera() {
        let mut c = cam(10.0);
        c.rate = 0.1;
        let free = c.free_camera();
        c.set_track_target(1, true);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        c.track_frame(Some((Vec3::ZERO, Quat::IDENTITY)));
        c.track_frame(Some((Vec3::X * 7.0, Quat::IDENTITY)));
        assert!(c.transition.is_some());
        assert_ne!(c.eye, free.eye, "the tracking camera has moved");
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!(c.mode, Mode::Zoom { free });
        settle(&mut c);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!((c.mode, c.free_camera()), (Mode::Free, free));
    }

    /// The kept free camera gets no frame update while the zoom is current (`0x005FE410` never
    /// updates `+0x800`): when the zoom ends, its rate is the one it had before the zoom.
    #[test]
    fn the_free_camera_keeps_its_rate_through_a_zoom() {
        let mut c = cam(10.0);
        c.rate = 0.05;
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!(c.rate, START_RATE, "the zoom camera's own rate");
        settle(&mut c);
        assert_ne!(c.rate, 0.05, "the zoom's frame updates");
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!((c.mode, c.rate), (Mode::Free, 0.05));
    }

    #[test]
    fn new_cameras_start_at_the_start_rate() {
        let mut c = cam(10.0);
        c.set_track_target(1, true);
        c.rate = 0.1;
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        assert_eq!(c.rate, START_RATE, "the tracking camera");
        settle(&mut c);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        assert_eq!(c.rate, START_RATE, "the free camera after tracking");
    }

    #[test]
    fn zoom_from_tracking_comes_back_free_at_the_tracking_view() {
        let mut c = cam(10.0);
        c.set_track_target(1, true);
        c.key(CameraFunc::ToggleTracking, 0.0, &flat);
        settle(&mut c);
        c.track_frame(Some((Vec3::ZERO, Quat::IDENTITY)));
        c.track_frame(Some((Vec3::X * 7.0, Quat::IDENTITY)));
        c.end_frame(0.1, &flat);
        let view = (c.eye, c.target);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert!(matches!(c.mode, Mode::Zoom { .. }));
        settle(&mut c);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        assert_eq!((c.mode, (c.eye, c.target)), (Mode::Free, view));
    }

    /// The length is the larger of 0.025 s per metre and 3/π s per radian, in [0.5, 3]; the view
    /// eases by a sine; the shown eye stays 2 m over the ground.
    #[test]
    fn transitions_ease_over_their_length() {
        let mut c = cam(10.0);
        c.start_transition((Vec3::new(100.0, 10.0, 0.0), Vec3::new(100.0, 10.0, -1.0), FOV));
        assert!((c.transition.unwrap().duration - 2.5).abs() < 1e-6);
        c.start_transition((Vec3::new(0.0, 10.0, 0.0), Vec3::new(1.0, 10.0, 0.0), FOV));
        assert!((c.transition.unwrap().duration - 1.5).abs() < 1e-6, "a quarter turn");
        c.start_transition((Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 10.0, 1.0), FOV));
        assert!((c.transition.unwrap().duration - 1.5).abs() < 1e-6, "a half turn counts as a quarter");
        c.start_transition((Vec3::new(1.0, 10.0, 0.0), Vec3::new(1.0, 10.0, -1.0), FOV));
        assert_eq!(c.transition.unwrap().duration, TRANSITION_MIN);
        c.start_transition((Vec3::new(1000.0, 10.0, 0.0), Vec3::new(1000.0, 10.0, -1.0), FOV));
        assert_eq!(c.transition.unwrap().duration, TRANSITION_MAX);

        // 100 m east to the camera, looking the same way, over 2.5 s at the rate 0.5.
        let mut c = cam(10.0);
        c.start_transition((Vec3::new(100.0, 10.0, 0.0), Vec3::new(100.0, 10.0, -1.0), FOV));
        c.end_frame(0.5, &flat);
        assert_eq!(c.view().0, Vec3::new(100.0, 10.0, 0.0), "the first frame shows the start");
        c.end_frame(0.5, &flat);
        let r = (0.5f32 / 2.5 * std::f32::consts::FRAC_PI_2).sin();
        assert!(close(c.view().0, Vec3::new(100.0 * (1.0 - r), 10.0, 0.0)), "{:?}", c.view());
        for _ in 0..3 {
            c.end_frame(0.5, &flat);
        }
        assert!(c.transition.is_some_and(|t| !t.running()), "ended after 2.5 s, still shown this frame");
        c.end_frame(0.5, &flat);
        assert!(c.transition.is_none());
        assert_eq!(c.view(), (c.eye, c.target, FOV));

        // A start under the ground: the shown eye is lifted, the target is not.
        let mut c = cam(10.0);
        c.start_transition((Vec3::new(0.0, -5.0, 0.0), Vec3::new(0.0, -5.0, -1.0), FOV));
        c.end_frame(0.1, &flat);
        assert_eq!(c.view().0.y, TRANSITION_CLEARANCE);
        assert_eq!(c.view().1.y, -5.0);
    }

    /// The yaw turns the short way: from 170° to −170° through 180°, not through 0°.
    #[test]
    fn transitions_turn_the_short_way() {
        let mut c = cam(10.0);
        c.rate = 0.05;
        let deg = |a: f32| a.to_radians();
        c.target = c.eye + Vec3::new(deg(-170.0).sin(), 0.0, -deg(-170.0).cos());
        c.start_transition((c.eye, c.eye + Vec3::new(deg(170.0).sin(), 0.0, -deg(170.0).cos()), FOV));
        for _ in 0..6 {
            c.advance_transition(&flat);
            let (eye, target, _) = c.view();
            let yaw = yaw_of(&BattleCamera { eye, target, ..c.clone() });
            assert!(yaw.abs() > deg(169.9), "{}", yaw.to_degrees());
        }
    }

    #[test]
    fn a_bookmark_view_ends_the_modes() {
        let mut c = cam(10.0);
        c.key(CameraFunc::BookmarkSave(0), 0.0, &flat);
        c.key(CameraFunc::IntelliZoom, 0.0, &flat);
        c.key(CameraFunc::BookmarkView(0), 0.0, &flat);
        assert_eq!((c.mode, c.transition, c.fov), (Mode::Free, None, FOV));
    }
}

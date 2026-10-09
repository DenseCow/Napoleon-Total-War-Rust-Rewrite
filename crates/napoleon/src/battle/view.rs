//! Drawing, in 3D: every model unit is a block of real soldier figures (assembled from the
//! install by `crate::soldiers`), an outline on the ground in its side's colour, and a
//! screen-space text label. Volleys are drawn as lines.
//!
//! These Bevy entities are only a *picture* of the model. They are read from `BattleSim` every
//! frame and never feed back into the rules (the original's model/display split).
//!
//! Battlefield metres map to Bevy world units 1:1: model `(x, y)` -> world `(x, 0, -y)`.
//! If the install's soldier data cannot be read (e.g. running on the test fixture), units are
//! drawn as outlines only.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::transform::commands::BuildChildrenTransformExt;
use bevy::render::storage::ShaderBuffer;
use ntw_formats::anim::Anim;
use ntw_formats::unit_model::{VariantRole, unit_lod};
use ntw_sim::battle::model::LandUnit;
use ntw_sim::battle::morale::MoraleState;

use super::skin::{self, BoneAtlas, ClipSlot, SkinExt, SkinMaterial};
use super::{BattleSim, UnitInfo, VOLLEY_FX_SECONDS, VolleyFx};
use crate::data::GameData;
use crate::soldiers::{FigureKit, KitAssets, SoldierLibrary, animation_keys};
use ntw_formats::unit_animation::{self, DeathCause, Gait, SelectionRng, alternative, family_pick, pick_level};

use super::actions::{self, Mode};

/// Different men per unit (PROVISIONAL: the game varies parts per man; we build this many
/// kits per unit and share them round-robin; the men of a kit share its meshes and are drawn
/// instanced). Clips do not depend on the kit: each man plays his own alternative clips,
/// picked by his selection number, with his own phase (see `skin`).
const KITS_PER_UNIT: usize = 3;
/// Most figures drawn per unit.
const MAX_FIGURES: usize = 240;
/// The figure generators start from the battle's own seed, as the original's battle generator
/// does (CONFIRMED source: the battle set-up `0x00511210` gets `timeGetTime()`, or the
/// `constant_random_seed` option, from `0x004847F0`; our battles use their deterministic seed). Which
/// draws come before the men are made is UNKNOWN, so the state stays PROVISIONAL. The actions
/// generator is a second stream (ours) so deaths do not shift the men's selection numbers.
fn selection_seed(battle_seed: u32) -> u32 {
    battle_seed
}

/// Marks the entity (parent of the figures) that shows model unit `id`.
#[derive(Component)]
pub struct UnitView {
    pub id: u32,
    /// Where the unit was last seen in `sim.battle.units`: read only while that slot still holds
    /// unit `id` (checked on every read), re-found by id and kept by [`observe`](Self::observe)
    /// otherwise (a unit added before it, a reinforcement, moves it), so a lookup is O(1).
    slot: usize,
}

impl UnitView {
    fn new(id: u32, slot: usize) -> Self {
        Self { id, slot }
    }

    /// The unit at the kept slot, if that slot still holds it.
    fn at_slot<'a>(&self, units: &'a [LandUnit]) -> Option<&'a LandUnit> {
        units.get(self.slot).filter(|u| u.id == self.id)
    }

    /// The model unit this view shows (`units`: `sim.battle.units`): its slot when that still
    /// holds it, else found by id.
    fn unit<'a>(&self, units: &'a [LandUnit]) -> Option<&'a LandUnit> {
        self.at_slot(units).or_else(|| units.iter().find(|u| u.id == self.id))
    }

    /// [`unit`](Self::unit), keeping the slot it was found at (once per model tick).
    fn observe<'a>(&mut self, units: &'a [LandUnit]) -> Option<&'a LandUnit> {
        if self.at_slot(units).is_none() {
            self.slot = units.iter().position(|u| u.id == self.id)?;
        }
        units.get(self.slot)
    }

    /// The unit's display info: at the same slot in `sim.info` (kept in `battle.units`' order,
    /// [`BattleSim::add_unit`]) when that holds it, else found by id.
    fn info<'a>(&self, sim: &'a BattleSim) -> Option<&'a UnitInfo> {
        sim.info_at(self.slot, self.id)
    }
}

/// Where a unit is drawn between two model ticks.
///
/// The model moves units only on its 0.1 s ticks (a lock-step 100 ms tick in the original too,
/// `0x0057DF40`, CONFIRMED), so a unit drawn at its model position jumps 0.14 m (walking) to over
/// 1 m (cavalry running) ten times a second, while the frames in between see it still: the
/// "low-fps walking" of the user's 2026-10-08 report. The original's walking men look smooth, so
/// its drawing moves them between ticks; how it does that is not traced (UNKNOWN: no render-side
/// blend was found on the battle update path, BATTLE_FIDELITY.md §59). PROVISIONAL (ours): the
/// unit is drawn on the line from its pose before the last tick to its pose after it, at the
/// fraction of the next tick already elapsed (`Time<Fixed>::overstep_fraction`), so the picture
/// runs one tick (0.1 s) behind the model. A pose that changed without a moving tick (placed,
/// dragged in deployment, turned by a HUD button, snapped into a building) is drawn at once.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct DrawnPose {
    /// The model position and facing before and after the last model tick.
    from: (Vec2, f32),
    to: (Vec2, f32),
}

impl DrawnPose {
    /// A unit drawn where the model has it.
    pub fn at(pos: Vec2, facing: f32) -> Self {
        DrawnPose { from: (pos, facing), to: (pos, facing) }
    }

    /// After every fixed step, with the unit as the model left it: a tick in which the model moved
    /// the unit (`moved`) is blended from the last pose; anything else is drawn at once.
    fn observe(&mut self, pos: Vec2, facing: f32, moved: bool) {
        self.from = if moved && self.to.0.distance_squared(pos) < MAX_BLEND_STEP_M * MAX_BLEND_STEP_M { self.to } else { (pos, facing) };
        self.to = (pos, facing);
    }

    /// The pose to draw this frame, `alpha` (0..1) of the way through the next tick, for a unit
    /// the model now has at `pos` / `facing`. A pose the model changed outside a tick (it is not
    /// the one the last tick left) is drawn as it is.
    pub fn blend(&self, pos: Vec2, facing: f32, alpha: f32) -> (Vec2, f32) {
        if (pos, facing) != self.to {
            return (pos, facing);
        }
        let a = alpha.clamp(0.0, 1.0);
        let (p0, f0) = self.from;
        // The shorter way round.
        let turn = (facing - f0 + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        (p0.lerp(pos, a), f0 + turn * a)
    }
}

/// The longest model step that is blended: a faster unit moves about 1.2 m per tick (11 m/s
/// general's horse), so a longer jump is a placement, drawn at once (ours).
const MAX_BLEND_STEP_M: f32 = 5.0;

/// Marks the text label entity for model unit `id`.
#[derive(Component)]
pub struct UnitLabel {
    pub id: u32,
    /// The text last drawn (redrawn only when it changes).
    pub text: String,
}

/// The soldier figures of one unit view.
#[derive(Component)]
pub struct UnitFigures {
    kits: Vec<FigureKit>,
    men: Vec<Entity>,
    /// Figure slot of man 0 in [`SkinState::figures`]; man `i` uses slots `2 (first + i)` (man)
    /// and `2 (first + i) + 1` (mount).
    first_slot: usize,
    /// Drawn position and facing (see [`DrawnPose`]) at the last frame, and whether the men were placed on the
    /// terrain yet.
    last_pos: Vec2,
    last_facing: f32,
    seated: bool,
    /// The unit's ground speed, which picks the gait and the speed level (observed after every
    /// model tick by [`observe_ticks`]).
    ground: GroundSpeed,
    /// Each man's selection number: which alternative clip of every slot he plays
    /// (`unit_animation::alternative`).
    selections: Vec<u32>,
    /// Per-man loop clip, its own time and cross-fade, then the standard bearer's (last).
    blends: Vec<ClipBlend>,
    /// Per-man action state (fire, reload, melee, death; see `actions`).
    acts: Vec<actions::ManAct>,
    /// Model state at the last frame, to see events: men alive, volleys fired, in melee.
    last_men: u32,
    last_volleys: u32,
    last_melee: bool,
    /// The kit's `COMBAT_IDLE_n` / `ATTACK_n` slots (with `RIDER_` for riders).
    melee_idle: Vec<String>,
    melee_attack: Vec<String>,
    /// Each man's current part LOD (`unit_model::unit_lod`).
    lods: Vec<u8>,
    /// The unit's standard bearer, when `unit_stats_land` names one. He is not one of `unit.men`:
    /// he does not fire, and he does not die with the strength count.
    bearer: Option<Bearer>,
}

/// Model ticks without movement before a unit's men stop walking and stand (0.3 s at the 0.1 s
/// tick). PROVISIONAL (ours): the exe's stand rule is not traced.
const STILL_TICKS: u32 = 3;

/// A figure's loop clip (gait, speed level, ready or melee loop): which clip it plays, its own
/// time in it, and its cross-fade out of the poses shown at its last clip changes.
///
/// CONFIRMED rule (`0x007725D0`, UNITS_TERRAIN_FIDELITY.md §1.9): each soldier's display keeps its
/// own clip and clip time. When the clip changes, the new clip starts at [`carry_phase`]'s time,
/// the blend time D is the new fragment line's `blend_in_time` (vanilla gaits 0.5 or 0.25 s, 1.0 s
/// when the line has none; the gear-window override `0x00E6DB90` never applies: no vanilla soldier
/// clip has the markers) and the elapsed time e restarts at 0. Every frame the drawn pose is the
/// previous frame's drawn pose blended toward the new clip at weight e / D, then e grows by the
/// frame time, so the pose shown at the change keeps weight prod(1 - e_k / D) and is gone once
/// e >= D. With D = 0, or no pose drawn the frame before, the new clip is drawn alone.
///
/// PROVISIONAL (ours): the GPU keeps at most two poses frozen at clip changes, with their weights
/// (newest first, [`skin::Fade`]), rather than the recursive blended pose, so the new clip's
/// earlier frames do not linger ([`freeze`]: a change never waits; of three poses within one
/// blend time the lightest is dropped and the two heaviest kept). A one-shot drawn over the loop is
/// not blended and ends a running cross-fade (one-shots are not blended yet). The exe also lerps
/// the display root (position, heading) at e / Dr (Dr = 0.5 s, `0x01318038`); that root blend is
/// not implemented (our men stand at their formation places).
#[derive(Debug, Clone, Copy, Default)]
struct ClipBlend {
    /// The loop clip played (its `Arc` address; 0 before the first frame) and its man's and
    /// mount's atlas slots.
    clip: usize,
    man: ClipSlot,
    mount: Option<ClipSlot>,
    /// Its root speed (m/s, [`clip_speed`]) and the figure's time in it (s).
    speed: f32,
    time: f32,
    /// Up to two poses frozen at clip changes (the two heaviest when a third came within one blend
    /// time, [`freeze`]), newest first (weight 0 = unused); the clip played has the rest of the
    /// weight.
    frozen: [Frozen; 2],
    /// Blend time D (s) of the last change, and the time since it e (s).
    blend: f32,
    elapsed: f32,
    /// The man's and mount's frames drawn at the frame numbered `drawn` (`SkinState::frame`; 0 =
    /// never drawn).
    shown: [[u32; 4]; 2],
    drawn: u32,
}

/// A pose frozen at a clip change: the man's and mount's clip frames, and its weight.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Frozen {
    figs: [[u32; 4]; 2],
    keep: f32,
}

/// The loop clip a figure is to play this frame: its `Arc` address, its man's and mount's atlas
/// slots, and its line's `blend_in_time` (s).
#[derive(Debug, Clone, Copy)]
struct LoopClip {
    key: usize,
    man: ClipSlot,
    mount: Option<ClipSlot>,
    blend_in: f32,
}

impl ClipBlend {
    /// One frame of a figure that is to play `want`. `shown` is [`Self::shown`] (the frames drawn
    /// last frame, if any), `speed` gives `want`'s root speed (called only when the man's or the
    /// mount's clip changes), `offset`
    /// is the figure's own phase (a fraction of a clip, `skin::phase`), `advance` the clip time to
    /// add (frame time x playback rate), `dt` the frame time, and `shot` whether a one-shot is
    /// drawn instead of the loop.
    #[allow(clippy::too_many_arguments)]
    fn step(&mut self, want: &LoopClip, shown: Option<[[u32; 4]; 2]>, speed: impl FnOnce() -> f32, offset: f32, advance: f32, dt: f32, shot: bool) {
        // A one-shot ends a running cross-fade; so does a frame the figure was skipped (its slot
        // was drawn without the fade then, so the frozen poses would come back stale).
        if shot || shown.is_none() {
            self.frozen = Default::default();
        }
        // The mount clip is picked again every frame: in melee the man keeps his idle clip while the
        // mount's follows the gait. PROVISIONAL (ours): a mount clip changing on its own snaps (the
        // pair shares one blend; the exe blends each display, rider and mount, on its own).
        let mount_changed = want.mount != self.mount;
        self.mount = want.mount;
        if want.key == self.clip {
            // The pair's root speed is the faster clip's: a mount change alone changes it too.
            if mount_changed {
                self.speed = speed();
            }
        } else {
            let duration = want.man.duration();
            self.time = carry_phase(self.time, self.man.duration(), self.speed, duration).unwrap_or(offset * duration);
            self.clip = want.key;
            self.man = want.man;
            self.speed = speed();
            match shown.filter(|_| !shot && want.blend_in > 0.0) {
                Some(figs) => {
                    self.frozen = freeze(self.frozen, figs);
                    self.blend = want.blend_in;
                    self.elapsed = 0.0;
                }
                None => self.frozen = Default::default(),
            }
        }
        if self.fading() {
            let w = self.elapsed / self.blend;
            if w >= 1.0 {
                self.frozen = Default::default();
            } else {
                for f in &mut self.frozen {
                    f.keep *= 1.0 - w;
                }
            }
            self.elapsed += dt;
        }
        self.time += advance;
    }

    /// The frames the figure drew last frame (frame `frame - 1`), if it was drawn then: a figure
    /// skipped since (its whole unit, or itself) has no pose to fade from.
    fn shown(&self, frame: u32) -> Option<[[u32; 4]; 2]> {
        (self.drawn != 0 && self.drawn.wrapping_add(1) == frame).then_some(self.shown)
    }

    /// Whether a cross-fade runs.
    fn fading(&self) -> bool {
        self.frozen.iter().any(|f| f.keep > 0.0)
    }

    /// The man's and mount's frames to draw: the one-shot's when one is drawn, else the loop's at
    /// the figure's time (without a mount clip the mount slot repeats the man's frame).
    fn draw(&mut self, man_shot: Option<[u32; 4]>, mount_shot: Option<[u32; 4]>, frame: u32) -> [[u32; 4]; 2] {
        let man = man_shot.unwrap_or_else(|| self.man.figure(self.time));
        let mount = mount_shot.or_else(|| self.mount.map(|m| m.figure(self.time))).unwrap_or(man);
        self.shown = [man, mount];
        self.drawn = frame;
        [man, mount]
    }

    /// The cross-fade of the man's (`part` 0) or mount's (1) slot, if one runs.
    fn fade(&self, part: usize) -> Option<skin::Fade> {
        let [a, b] = self.frozen;
        self.fading().then(|| skin::fade([a.figs[part], b.figs[part]], [a.keep, b.keep]))
    }
}

/// The frozen poses after a clip change: the frame drawn last (`figs`) at the weight the old
/// clip had (1 - the frozen weights), then the poses frozen before, newest first, without the
/// ones of weight 0 (a change on the frame after a change: the old clip had none yet). Of three,
/// the lightest is dropped (the older of equals) and the other two scaled up to its weight
/// (PROVISIONAL, ours: a small jump of that weight, only on a third change within one blend
/// time). The weights then add up to 1, the new clip starts at 0, and every pose is gone one
/// blend time after the last change.
fn freeze(old: [Frozen; 2], figs: [[u32; 4]; 2]) -> [Frozen; 2] {
    let live = Frozen { figs, keep: (1.0 - old[0].keep - old[1].keep).max(0.0) };
    let mut poses = [live, old[0], old[1]];
    let lightest = (0..3).rev().min_by(|&a, &b| poses[a].keep.total_cmp(&poses[b].keep)).unwrap_or(2);
    let total: f32 = poses.iter().map(|p| p.keep).sum();
    let dropped = poses[lightest].keep;
    poses[lightest].keep = 0.0;
    let scale = if total - dropped > 0.0 { total / (total - dropped) } else { 1.0 };
    let mut out = [Frozen::default(); 2];
    for (slot, pose) in out.iter_mut().zip(poses.iter().filter(|p| p.keep > 0.0)) {
        *slot = Frozen { keep: pose.keep * scale, ..*pose };
    }
    out
}

/// Writes a figure slot: the frame drawn and, when it cross-fades, its fade entry's number.
fn put_figure(slot: &mut skin::Figure, frame: [u32; 4], fade: Option<skin::Fade>, fades: &mut Vec<skin::Fade>) {
    *slot = [frame[0], frame[1], frame[2], 0];
    if let Some(f) = fade {
        fades.push(f);
        slot[3] = fades.len() as u32;
    }
}

/// The time (s) a figure starts a new looping clip of `duration` s at, after a clip of
/// `old_duration` s whose root moved at `old_speed` m/s, when the step phase carries over.
///
/// CONFIRMED rule (`0x007725D0`, UNITS_TERRAIN_FIDELITY.md §1.9): after a clip whose root moves
/// faster than [`MOVING_CLIP_SPEED`] (the old clip's own root speed, anim `+0x60`), the new clip
/// starts at the old clip's normalised time (old time mod old length / old length x new length);
/// else at 0 (`None`; ours then starts the figure at its own phase offset, `skin::phase`). Each
/// figure keeps its own time, so a change carries that man's own phase and leaves the figures
/// whose clip did not change untouched.
fn carry_phase(time: f32, old_duration: f32, old_speed: f32, duration: f32) -> Option<f32> {
    (old_speed > MOVING_CLIP_SPEED && old_duration > 0.0).then(|| time.rem_euclid(old_duration) / old_duration * duration)
}

/// Clip root speed (m/s) above which the clip left counts as moving for [`carry_phase`]
/// (`0x0131A7B0`, CONFIRMED).
const MOVING_CLIP_SPEED: f32 = 0.1;

/// A loop clip's root speed for [`carry_phase`] (`Anim::root_speed`, the exe's anim `+0x60`): the
/// man's clip's, or a rider's mount clip's when faster. PROVISIONAL (ours): a rider and his
/// mount share one clip time here, so the pair carries its phase when either moves (the exe
/// tests each display's own clip).
fn clip_speed(man: &Anim, mount: Option<&Anim>) -> f32 {
    mount.map_or(0.0, Anim::root_speed).max(man.root_speed())
}

/// A unit's ground speed, which picks its gait and speed level (`unit_animation::pick_level`).
///
/// The model moves a unit only on its 0.1 s ticks, so the speed is the distance of a moving tick,
/// not of a frame: the frames between two ticks see no movement, and a per-frame estimate swings
/// every tick (the walking jitter of 2026-10-04..07: the men switched speed level and playback
/// rate several times a second). The exe picks a speed-matched clip from the entity's own speed
/// (`+0x14C`; `0x006611E0`, `0x005B7B20`, UNITS_TERRAIN_FIDELITY.md §1.4), which is steady while it
/// marches; the distance per moving tick is the model's equivalent. It is observed after every
/// model tick ([`observe_ticks`], `FixedUpdate`), not every frame, so a frame that spans several
/// ticks (low frame rate, fast battle speed) sees each one: a tick without movement does not dilute
/// the speed of the moving ones. Only a tick in which the model moved the unit
/// (`LandUnit::moved`) and the unit was on open ground before and after it ([`on_ground`]) counts.
/// A move with no tick (a deployment drag) is not walking, and neither is the tick a unit is
/// placed in: a reinforcement is put at the field edge and walks its first step in the same tick
/// (`Battle::step` runs `reinforcements_step`, then the unit's move), so that tick's distance is
/// the jump plus a step, and it is the tick the unit comes onto the field (it was `off_field`).
/// The snap into a building (`garrison_step`) is skipped the same way. Between moving ticks the
/// last speed is kept.
#[derive(Debug, Clone, Copy)]
struct GroundSpeed {
    /// The model position and tick last seen.
    pos: Vec2,
    tick: u32,
    /// Whether the unit was on open ground at the last observation ([`on_ground`]).
    on_ground: bool,
    /// m/s over the last model tick in which the unit moved.
    speed: f32,
    /// Fixed steps (model ticks, or 0.1 s while the model is not ticking) since the last model
    /// tick in which the unit moved.
    still_ticks: u32,
}

/// Whether a unit is on open ground: on the field (not a reinforcement still waiting or held) and
/// not inside a building. Only a move between two such observations is walking.
fn on_ground(unit: &LandUnit) -> bool {
    !unit.off_field() && unit.garrison.is_none()
}

impl GroundSpeed {
    fn new(pos: Vec2, tick: u32, on_ground: bool) -> Self {
        GroundSpeed { pos, tick, on_ground, speed: 0.0, still_ticks: STILL_TICKS }
    }

    /// After every fixed step, with the unit as the model left it and the model's tick counter.
    fn observe_unit(&mut self, unit: &LandUnit, tick: u32) {
        self.observe(Vec2::new(unit.position.0, unit.position.1), tick, unit.moved, on_ground(unit));
    }

    /// After every fixed step, with the unit's model position, the model's tick counter, whether
    /// the model moved the unit in that tick (`LandUnit::moved`) and whether it is on open ground
    /// ([`on_ground`]).
    fn observe(&mut self, pos: Vec2, tick: u32, moved: bool, on_ground: bool) {
        let Some(ticks) = tick.checked_sub(self.tick) else {
            // The tick went back. A restart (R) rebuilds the views (`spawn_missing_views`), so
            // this is not expected; start over, standing.
            *self = GroundSpeed::new(pos, tick, on_ground);
            return;
        };
        let distance = pos.distance(self.pos);
        let placed = !(self.on_ground && on_ground);
        self.on_ground = on_ground;
        if ticks > 0 && moved && !placed && distance > 1e-4 {
            self.speed = distance / (ticks as f32 * ntw_sim::battle::TICK_SECONDS);
            self.still_ticks = 0;
        } else {
            // A tick without a move of its own, or a step in which the model did not tick
            // (deployment, a decided battle): the unit is still.
            self.still_ticks = self.still_ticks.saturating_add(1);
        }
        self.pos = pos;
        self.tick = tick;
    }

    /// The gait: stand once still for [`STILL_TICKS`], run above the walk/run midpoint
    /// (PROVISIONAL, ours: the exe's walk/run choice is not traced).
    fn gait(&self, walk_speed: f32, run_speed: f32) -> Gait {
        if self.still_ticks >= STILL_TICKS {
            Gait::Stand
        } else if self.speed > (walk_speed + run_speed) * 0.5 {
            Gait::Run
        } else {
            Gait::Walk
        }
    }
}

/// A fallen man: detached from his unit so he stays where he fell (see `actions`).
#[derive(Component)]
pub struct Corpse;

/// The standard bearer of a unit that has one: his own kit, because his variant
/// (`*.standard_bearer.unit_variant`) and his skeleton (`Animations/MEN/FLAG_BEARER/...`) are not
/// his unit's, and the entity he is drawn on.
#[derive(Clone)]
pub struct Bearer {
    /// His figure, drawn like any other man.
    pub entity: Entity,
    /// His kit. Kept whole so the view picks his clip per frame exactly as it does for a man.
    pub kit: FigureKit,
}

/// The bearer's pole bone, in the bearer's own model space, for the frame the view posed him on.
/// The flag cloth is placed from it, so the cloth and the body always agree on the frame.
#[derive(Component)]
pub struct PoleBone {
    /// Column-major 4x4, exactly as `ntw_formats::anim::world_matrices` gives it. No Bevy mirror:
    /// `flag::sync_flags` applies that.
    pub model: [f32; 16],
}

/// The battle's generator for random clip families (deaths, knock-downs; `actions`).
#[derive(Resource)]
pub struct ActionRng(pub SelectionRng);

/// The soldier data, if the install has it.
#[derive(Resource)]
pub struct BattleSoldiers {
    lib: Option<SoldierLibrary>,
}

/// GPU skinning state of the current battle's figures (see `skin`).
#[derive(Resource)]
pub struct SkinState {
    atlas: BoneAtlas,
    figures: Handle<ShaderBuffer>,
    /// One entry per figure slot, rewritten every frame.
    data: Vec<skin::Figure>,
    /// The number of the current frame (counted by `sync_views` from 1), so a figure knows whether
    /// it was drawn the frame before (`ClipBlend::shown`).
    frame: u32,
    /// The [`BattleSim::build`] these views were built for.
    build: u32,
}

/// Orbit-style battle camera: looks at `focus` from `distance`, `yaw` around the vertical.
#[derive(Component)]
pub struct BattleCamera {
    focus: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

/// Model battlefield metres -> Bevy world position on the ground (the battle map's terrain
/// height, see `crate::terrain`; y = 0 when no map is loaded).
pub fn world_of(p: Vec2) -> Vec3 {
    crate::terrain::ground_point(p)
}

/// Startup system: 3D camera, sun, ground, and the soldier library.
pub fn spawn_camera(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sim: Option<Res<BattleSim>>,
) {
    let mut cam = BattleCamera { focus: Vec3::new(30.0, 0.0, 90.0), distance: 170.0, yaw: 0.0, pitch: 0.4 };
    // Start behind the player's army (side 0), looking the way it faces.
    if let Some(sim) = sim.as_ref() {
        let mine: Vec<&LandUnit> = sim.battle.units.iter().filter(|u| u.side == 0).collect();
        if let Some(first) = mine.first() {
            let n = mine.len() as f32;
            let centre = mine.iter().fold(Vec2::ZERO, |a, u| a + Vec2::new(u.position.0, u.position.1)) / n;
            let facing = Vec2::new(first.facing.cos(), first.facing.sin());
            let focus = centre + facing * 60.0;
            cam.focus = Vec3::new(focus.x, 0.0, -focus.y);
            cam.yaw = (-first.facing.cos()).atan2(first.facing.sin());
            // Test harness: `NAPOLEON_BATTLE_ZOOM=<metres>[,<k>]` looks at the player's k-th
            // unit (default the first) from close by.
            let zoom = std::env::var("NAPOLEON_BATTLE_ZOOM").unwrap_or_default();
            let mut it = zoom.split(',').map(|s| s.trim().parse::<f32>().ok());
            if let Some(Some(d)) = it.next() {
                let k = it.next().flatten().unwrap_or(0.0) as usize;
                let u = mine.get(k).copied().unwrap_or(*first);
                cam.focus = Vec3::new(u.position.0, 0.0, -u.position.1);
                (cam.distance, cam.pitch) = (d, 0.2);
            }
        }
    }
    // A battle file's own start view: camera_start/target_position are (x, height, map y)
    // (INFERRED). Heights are taken as given (the camera floor keeps it above the ground).
    if let Some((s, t)) = sim.as_ref().and_then(|s| s.camera_start).filter(|_| std::env::var_os("NAPOLEON_BATTLE_ZOOM").is_none()) {
        let (start, target) = (Vec3::new(s[0], s[1], -s[2]), Vec3::new(t[0], t[1], -t[2]));
        let off = start - target;
        let d = off.length().max(1.0);
        cam = BattleCamera { focus: target, distance: d, yaw: off.x.atan2(off.z), pitch: (off.y / d).clamp(-1.0, 1.0).asin().max(0.05) };
    }
    // Test harness: `NAPOLEON_BATTLE_CAMERA=x,z,distance,yaw,pitch` overrides the start view.
    if let Ok(s) = std::env::var("NAPOLEON_BATTLE_CAMERA") {
        let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if let [x, z, d, yaw, pitch] = v[..] {
            cam = BattleCamera { focus: Vec3::new(x, 0.0, z), distance: d, yaw, pitch };
        }
    }
    commands.spawn((Camera3d::default(), Transform::default(), cam));
    commands.spawn((
        DirectionalLight { illuminance: 10_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(-60.0, 120.0, 80.0).looking_at(Vec3::ZERO, Vec3::Y),
        // PROVISIONAL: 2 shadow cascades out to 150 m (Bevy's default is 4); every cascade
        // re-draws every skinned figure, so this halves the cost of soldier shadows.
        bevy::light::CascadeShadowConfigBuilder { num_cascades: 2, maximum_distance: 150.0, ..default() }.build(),
    ));
    commands.insert_resource(GlobalAmbientLight { brightness: 500.0, ..default() });
    // Flat ground only when no battle map loaded (`crate::terrain` draws the real terrain).
    if !crate::terrain::loaded() {
        commands.spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(2_000.0, 2_000.0))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.30, 0.40, 0.22),
                perceptual_roughness: 1.0,
                ..default()
            })),
        ));
    }
    let lib = match SoldierLibrary::open(&crate::config::game_data_dir()) {
        Ok(lib) => Some(lib),
        Err(e) => {
            warn!("Soldier models unavailable ({e}); drawing unit outlines only");
            None
        }
    };
    commands.insert_resource(super::labels::LabelFont::load(lib.as_ref().map(|l| &l.vfs)));
    commands.insert_resource(BattleSoldiers { lib });
}

/// Battle harness runs (a fixed camera, a screenshot, an AI shot, or `--screenshot`): the live keyboard
/// and mouse must not move the camera or give orders (`input.rs`), so captures can be repeated.
pub fn camera_harness_run() -> bool {
    static RUN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *RUN.get_or_init(|| {
        ["NAPOLEON_BATTLE_CAMERA", "NAPOLEON_BATTLE_ZOOM", "NAPOLEON_BATTLE_SCREENSHOT", "NAPOLEON_AI_SHOT"].iter().any(|v| std::env::var_os(v).is_some())
            || std::env::args().any(|a| a == "--screenshot")
    })
}

/// Arrow keys pan, mouse wheel zooms, Q / E turn the camera (not in harness runs, see
/// [`camera_harness_run`]).
/// A camera position requested by a battle script (`camera:move_to`): (camera position, look-at
/// target) in engine coordinates (x, height, map y). Applied at once (PROVISIONAL: the script's
/// move time is not used).
#[derive(Resource, Default)]
pub struct ScriptCamera(pub Option<([f32; 3], [f32; 3])>);

/// Applies a pending [`ScriptCamera`] to the battle camera (same conversion as a battle file's
/// start view).
pub fn apply_script_camera(mut pending: ResMut<ScriptCamera>, mut cams: Query<&mut BattleCamera>) {
    let Some((s, t)) = pending.0.take() else { return };
    let (start, target) = (Vec3::new(s[0], s[1], -s[2]), Vec3::new(t[0], t[1], -t[2]));
    let off = start - target;
    let d = off.length().max(1.0);
    for mut cam in &mut cams {
        *cam = BattleCamera { focus: target, distance: d, yaw: off.x.atan2(off.z), pitch: (off.y / d).clamp(-1.0, 1.0).asin().max(0.05) };
    }
}

pub fn move_camera(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time<Real>>,
    mut cams: Query<(&mut BattleCamera, &mut Transform)>,
    sim: Option<ResMut<BattleSim>>,
) {
    let fixed = camera_harness_run();
    // The camera actions a battle script's input handler hears (the original's action names; keys:
    // arrows or W/A/S/D move, Q/E rotate, X/Z up/down, INFERRED from the tutorial's texts).
    if let Some(mut sim) = sim
        && !fixed
    {
        for (k, alt, name) in [
            (KeyCode::ArrowUp, KeyCode::KeyW, "move forward"),
            (KeyCode::ArrowDown, KeyCode::KeyS, "move backward"),
            (KeyCode::ArrowLeft, KeyCode::KeyA, "move left"),
            (KeyCode::ArrowRight, KeyCode::KeyD, "move right"),
            (KeyCode::KeyQ, KeyCode::KeyQ, "rotate left"),
            (KeyCode::KeyE, KeyCode::KeyE, "rotate right"),
            (KeyCode::KeyX, KeyCode::KeyX, "move up"),
            (KeyCode::KeyZ, KeyCode::KeyZ, "move down"),
        ] {
            if keys.just_pressed(k) || keys.just_pressed(alt) {
                sim.script_events.push(super::ScriptEvent::Input(name));
            }
        }
    }
    let dt = if fixed { 0.0 } else { time.delta_secs() };
    for (mut cam, mut tf) in &mut cams {
        let fwd = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
        let right = Vec3::new(cam.yaw.cos(), 0.0, -cam.yaw.sin());
        let speed = cam.distance * 0.8 * dt;
        let mut pan = Vec3::ZERO;
        if keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
            pan += fwd;
        }
        if keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
            pan -= fwd;
        }
        if keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
            pan += right;
        }
        if keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
            pan -= right;
        }
        cam.focus += pan * speed;
        if keys.pressed(KeyCode::KeyQ) {
            cam.yaw += dt;
        }
        if keys.pressed(KeyCode::KeyE) {
            cam.yaw -= dt;
        }
        // X / Z: camera up / down (closer / farther along the view).
        if keys.pressed(KeyCode::KeyX) {
            cam.distance = (cam.distance * (1.0 + dt)).min(1_500.0);
        }
        if keys.pressed(KeyCode::KeyZ) {
            cam.distance = (cam.distance * (1.0 - dt)).max(4.0);
        }
        if scroll.delta.y != 0.0 && !fixed {
            cam.distance = (cam.distance * (1.0 - scroll.delta.y * 0.1)).clamp(4.0, 1_500.0);
            cam.pitch = (0.15 + cam.distance / 600.0).clamp(0.15, 1.2);
        }
        // Follow the terrain: the focus sits on the ground, or on the sea where the ground is the
        // sea bed; the camera never goes below either.
        cam.focus.y = crate::terrain::surface_height(Vec2::new(cam.focus.x, -cam.focus.z));
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        tf.translation = cam.focus + rot * Vec3::new(0.0, 0.0, cam.distance);
        let floor = crate::terrain::surface_height(Vec2::new(tf.translation.x, -tf.translation.z)) + 2.0;
        tf.translation.y = tf.translation.y.max(floor);
        tf.look_at(cam.focus, Vec3::Y);
    }
}

/// Whether the views on screen show this battle: built for this very [`BattleSim`] (`built` is
/// the `build` they were made for) and one per unit. A restart (R) builds a new `BattleSim`,
/// usually with the same unit ids, so the ids alone do not tell; nor does the tick going back
/// (a restart at tick ≤ 1 can reach the old tick by the next frame).
fn views_current(built: Option<u32>, build: u32, view_ids: impl Iterator<Item = u32> + Clone, mut unit_ids: impl Iterator<Item = u32> + Clone) -> bool {
    built == Some(build) && view_ids.clone().count() == unit_ids.clone().count() && unit_ids.all(|u| view_ids.clone().any(|v| v == u))
}

/// Creates the views and labels of a battle: on its first frame, on every restart (R), which
/// replaces the battle, and when its units change; the old pictures are thrown away first.
///
/// Figures are GPU-skinned (`skin`): all kits are built first, then their clips go into one
/// bone atlas, then every man's parts are spawned with static skinned meshes and his figure
/// slot as `MeshTag`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_missing_views(
    mut commands: Commands,
    sim: Res<BattleSim>,
    data: Res<GameData>,
    mut soldiers: ResMut<BattleSoldiers>,
    views: Query<(Entity, &UnitView)>,
    labels: Query<(Entity, &UnitLabel)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut skin_materials: ResMut<Assets<SkinMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut images: ResMut<Assets<Image>>,
    label_font: Option<Res<super::labels::LabelFont>>,
    corpses: Query<Entity, With<Corpse>>,
    skin: Option<Res<SkinState>>,
    mut fade_upload: ResMut<skin::FadeUpload>,
) {
    if !sim.is_changed() {
        return;
    }
    if views_current(skin.map(|s| s.build), sim.build, views.iter().map(|(_, v)| v.id), sim.info.iter().map(|i| i.id)) {
        return;
    }
    // On a restart, throw away the old pictures first.
    for (e, _) in &views {
        commands.entity(e).despawn();
    }
    for e in &corpses {
        commands.entity(e).despawn();
    }
    commands.insert_resource(ActionRng(SelectionRng(selection_seed(sim.seed) ^ 0x0BAD_F00D)));
    for (e, _) in &labels {
        commands.entity(e).despawn();
    }
    // 1. Kits per unit, plus each unit's standard bearer when it has one.
    let mut unit_kits: Vec<Vec<FigureKit>> = Vec::new();
    let mut bearer_kits: Vec<Option<FigureKit>> = Vec::new();
    for info in &sim.info {
        let mut kits = Vec::new();
        let mut bearer = None;
        if let (Some(lib), Some(unit)) = (soldiers.lib.as_mut(), sim.battle.units.iter().find(|u| u.id == info.id)) {
            let faction = if info.faction.is_empty() { sim.side_factions[usize::from(unit.side).min(1)].clone() } else { info.faction.clone() };
            let keys = data.db.unit_stats(&info.key).map(animation_keys);
            let mut assets = KitAssets { meshes: &mut meshes, materials: &mut materials, images: &mut images };
            for seed in 0..KITS_PER_UNIT as u64 {
                let Some(keys) = keys.as_ref() else { break };
                match lib.build_figure(&info.key, Some(&faction), keys, VariantRole::Soldier, seed, &mut assets) {
                    Ok(kit) => kits.push(kit),
                    Err(e) => {
                        warn!("{}: {e}", info.key);
                        break;
                    }
                }
            }
            // The standard bearer (CONFIRMED: `unit_stats_land` #4/5/6 officer / musician /
            // standard bearer feed `battle_personalities`, and `plan_figure` reads his own
            // animation table and equipment theme from it). He is a figure of his own because his
            // variant and skeleton are not his unit's, and the flag pole is his personal equipment.
            if let Some(keys) = keys.as_ref()
                && keys.standard_bearer.as_deref().is_some_and(|s| !s.is_empty())
            {
                match lib.build_figure(&info.key, Some(&faction), keys, VariantRole::StandardBearer, 0, &mut assets) {
                    Ok(kit) => bearer = Some(kit),
                    Err(e) => warn!("{} standard bearer: {e}", info.key),
                }
            }
        }
        unit_kits.push(kits);
        bearer_kits.push(bearer);
    }
    // 2. The bone atlas of every clip in use, and the figure slots.
    let mut atlas = BoneAtlas::default();
    for kit in unit_kits.iter().flatten().chain(bearer_kits.iter().flatten()) {
        for level in kit.levels.iter().flat_map(|(_, l)| l).chain(kit.actions.values()) {
            for clip in level.man.iter().chain(&level.mount) {
                atlas.add(clip);
            }
        }
    }
    let total_men: usize = sim
        .info
        .iter()
        .zip(&unit_kits)
        .filter(|(_, k)| !k.is_empty())
        .filter_map(|(i, _)| sim.battle.units.iter().find(|u| u.id == i.id))
        .map(|u| (u.men as usize).min(MAX_FIGURES))
        .sum::<usize>()
        // One figure slot each for the standard bearers.
        + bearer_kits.iter().filter(|k| k.is_some()).count();
    let figures_data = vec![[0u32; 4]; (2 * total_men).max(1)];
    info!("Bone atlas: {} matrices ({:.1} MB)", atlas.matrices.len(), atlas.matrices.len() as f64 * 64.0 / 1e6);
    let bones = buffers.add(ShaderBuffer::new(&atlas.bytes(), RenderAssetUsages::RENDER_WORLD));
    let mut figure_bytes = Vec::new();
    skin::write_figure_bytes(&figures_data, &mut figure_bytes);
    let figures = buffers.add(ShaderBuffer::new(&figure_bytes, RenderAssetUsages::default()));
    // Room for a cross-fade on every figure slot; only the entries in use are written (`skin::FadeUpload`).
    let fades = buffers.add(ShaderBuffer::with_size(figures_data.len() * std::mem::size_of::<skin::Fade>(), RenderAssetUsages::RENDER_WORLD));
    fade_upload.buffer = Some(fades.id());
    fade_upload.entries.clear();
    // 3. Static skinned meshes and skin materials, shared by every man of a kit.
    let mut part_meshes: HashMap<usize, Vec<Handle<Mesh>>> = HashMap::new();
    let mut skin_mats: HashMap<AssetId<StandardMaterial>, Handle<SkinMaterial>> = HashMap::new();
    let mut skin_mat = |h: &Handle<StandardMaterial>| -> Handle<SkinMaterial> {
        skin_mats
            .entry(h.id())
            .or_insert_with(|| {
                let base = materials.get(h).cloned().unwrap_or_default();
                skin_materials.add(SkinMaterial { base, extension: SkinExt { bones: bones.clone(), figures: figures.clone(), fades: fades.clone() } })
            })
            .clone()
    };
    // Per unit, per kit: its parts as (meshes per LOD, material, is_mount).
    let mut kit_parts: Vec<Vec<Vec<KitPart>>> = Vec::new();
    for kits in &unit_kits {
        let mut per_kit = Vec::new();
        for kit in kits {
            let mut parts = Vec::new();
            for kp in &kit.man.parts {
                let mesh = part_meshes
                    .entry(std::sync::Arc::as_ptr(&kp.part) as usize)
                    .or_insert_with(|| (0..skin::part_lod_count(&kp.part)).map(|l| meshes.add(skin::part_mesh_lod(&kp.part, l))).collect())
                    .clone();
                parts.push((mesh, skin_mat(&kp.material), false));
            }
            if let Some(m) = &kit.mount {
                for (pi, (piece, _)) in m.pieces.iter().enumerate() {
                    // LOD 0 and the horse's lower LODs (same piece in each; `mount::animated_lod`).
                    let mesh = part_meshes
                        .entry(std::sync::Arc::as_ptr(piece) as usize)
                        .or_insert_with(|| {
                            std::iter::once(piece).chain(m.lower_lods.iter().filter_map(|l| l.get(pi))).map(|p| meshes.add(skin::piece_mesh(p))).collect()
                        })
                        .clone();
                    parts.push((mesh, skin_mat(&m.material), true));
                }
            }
            per_kit.push(parts);
        }
        kit_parts.push(per_kit);
    }
    // The standard bearers' parts, with the same skin materials and the same shared meshes.
    let mut bearer_parts: Vec<Option<Vec<KitPart>>> = Vec::new();
    for kit in bearer_kits.iter() {
        bearer_parts.push(kit.as_ref().map(|kit| {
            let mut parts = Vec::new();
            for kp in &kit.man.parts {
                let mesh = part_meshes
                    .entry(std::sync::Arc::as_ptr(&kp.part) as usize)
                    .or_insert_with(|| (0..skin::part_lod_count(&kp.part)).map(|l| meshes.add(skin::part_mesh_lod(&kp.part, l))).collect())
                    .clone();
                parts.push((mesh, skin_mat(&kp.material), false));
            }
            if let Some(m) = &kit.mount {
                for (pi, (piece, _)) in m.pieces.iter().enumerate() {
                    let mesh = part_meshes
                        .entry(std::sync::Arc::as_ptr(piece) as usize)
                        .or_insert_with(|| {
                            std::iter::once(piece).chain(m.lower_lods.iter().filter_map(|l| l.get(pi))).map(|p| meshes.add(skin::piece_mesh(p))).collect()
                        })
                        .clone();
                    parts.push((mesh, skin_mat(&m.material), true));
                }
            }
            parts
        }));
    }
    // 4. Units, men and labels. Each man draws his selection number in spawn order.
    let mut next_slot = 0usize;
    let mut selection_rng = SelectionRng(selection_seed(sim.seed));
    let spawns: Vec<_> = sim
        .info
        .iter()
        .zip(unit_kits)
        .zip(kit_parts)
        .zip(bearer_kits)
        .zip(bearer_parts)
        .collect();
    for ((((info, kits), parts), bkit), bparts) in spawns {
        let Some((slot, unit)) = sim.battle.units.iter().enumerate().find(|(_, u)| u.id == info.id) else { continue };
        let pos = Vec2::new(unit.position.0, unit.position.1);
        let parent = commands
            .spawn((Transform::from_translation(world_of(pos)), Visibility::default(), UnitView::new(info.id, slot), DrawnPose::at(pos, unit.facing)))
            .id();
        if soldiers.lib.is_some() {
            let mut men = Vec::new();
            let mut selections = Vec::new();
            let first_slot = next_slot;
            if !kits.is_empty() {
                let count = (unit.men as usize).min(MAX_FIGURES);
                let ranks = info.ranks.max(1) as usize;
                let files = count.div_ceil(ranks).max(1);
                let (dx, dz) = (info.size_m.x / files as f32, info.size_m.y / ranks as f32);
                for i in 0..count {
                    let (file, rank) = (i % files, i / files);
                    let local = Vec3::new(
                        (file as f32 - (files - 1) as f32 * 0.5) * dx,
                        0.0,
                        (rank as f32 - (ranks - 1) as f32 * 0.5) * dz,
                    );
                    let slot = (first_slot + i) as u32;
                    let man = commands
                        .spawn((Transform::from_translation(local), Visibility::default()))
                        .with_children(|c| {
                            for (mesh, material, is_mount) in &parts[i % parts.len()] {
                                let mut part = c.spawn((
                                    Mesh3d(mesh[0].clone()),
                                    MeshMaterial3d(material.clone()),
                                    MeshTag(2 * slot + u32::from(*is_mount)),
                                    // The vertices are bone-local, so the mesh bounds mean nothing: a
                                    // fixed box around a man (or a horse) lets cameras and shadow
                                    // cascades cull him (PROVISIONAL sizes).
                                    figure_bounds(*is_mount),
                                    bevy::camera::visibility::NoAutoAabb,
                                ));
                                if mesh.len() > 1 {
                                    part.insert(PartLods(mesh.clone(), *is_mount));
                                }
                            }
                        })
                        .id();
                    commands.entity(parent).add_child(man);
                    men.push(man);
                    selections.push(selection_rng.next_selection());
                }
                next_slot += count;
            }
            // The standard bearer, when the unit has one: he stands in the middle of the front
            // rank (PROVISIONAL -- the exe places him in the formation and we do not know where;
            // target: the exe's standard bearer slot in the unit's formation). He gets his own
            // figure slot because he plays a clip of his own skeleton.
            let bearer = match (bkit, bparts) {
                (Some(kit), Some(bparts)) if !bparts.is_empty() => {
                    let ranks = info.ranks.max(1) as usize;
                    let dz = info.size_m.y / ranks as f32;
                    let slot = next_slot as u32;
                    next_slot += 1;
                    let local = Vec3::new(0.0, 0.0, -(ranks as f32 - 1.0) * 0.5 * dz);
                    let entity = commands
                        .spawn((
                            Transform::from_translation(local),
                            Visibility::default(),
                            // The pole bone starts at its frame-0 rest matrix; `sync_views` writes
                            // the real one every frame.
                            PoleBone { model: ntw_formats::cloth::bone3_at_ground() },
                        ))
                        .with_children(|c| {
                            for (mesh, material, is_mount) in &bparts {
                                c.spawn((
                                    Mesh3d(mesh[0].clone()),
                                    MeshMaterial3d(material.clone()),
                                    MeshTag(2 * slot + u32::from(*is_mount)),
                                    figure_bounds(*is_mount),
                                    bevy::camera::visibility::NoAutoAabb,
                                ));
                            }
                        })
                        .id();
                    commands.entity(parent).add_child(entity);
                    Some(Bearer { entity, kit })
                }
                _ => None,
            };
            commands.entity(parent).insert(UnitFigures {
                first_slot,
                last_pos: pos,
                last_facing: unit.facing,
                seated: false,
                ground: GroundSpeed::new(pos, sim.battle.tick, on_ground(unit)),
                acts: vec![actions::ManAct::default(); men.len()],
                blends: vec![ClipBlend::default(); men.len() + 1],
                lods: vec![0; men.len()],
                last_men: unit.men,
                last_volleys: unit.volleys_fired,
                last_melee: unit.in_melee,
                melee_idle: kits.first().map(|k| actions::present(k, unit_animation::COMBAT_IDLE, k.mount.is_some())).unwrap_or_default(),
                melee_attack: kits.first().map(|k| actions::present(k, unit_animation::ATTACK, k.mount.is_some())).unwrap_or_default(),
                kits,
                bearer,
                men,
                selections,
            });
        }
        let mut label = commands.spawn((Node { position_type: PositionType::Absolute, ..default() }, UnitLabel { id: info.id, text: String::new() }));
        if label_font.as_ref().is_some_and(|f| f.0.is_some()) {
            label.insert(ImageNode::default());
        } else {
            label.insert((Text::new(""), TextFont { font_size: bevy::text::FontSize::Px(13.0), ..default() }, TextColor(Color::WHITE)));
        }
    }
    commands.insert_resource(SkinState { atlas, figures, data: figures_data, frame: 0, build: sim.build });
}

/// After every fixed step (`FixedUpdate`, after the model tick and the battle scripts): each
/// unit's drawn pose and ground speed observe the tick (see [`DrawnPose`], [`GroundSpeed`]).
pub fn observe_ticks(sim: Res<BattleSim>, mut views: Query<(&mut UnitView, &mut DrawnPose, Option<&mut UnitFigures>)>) {
    for (mut view, mut pose, figures) in &mut views {
        // Through `Mut` only when the slot moved, so an unmoved view is not marked changed.
        let unit = match view.at_slot(&sim.battle.units) {
            Some(u) => u,
            None => match view.observe(&sim.battle.units) {
                Some(u) => u,
                None => continue,
            },
        };
        pose.observe(Vec2::new(unit.position.0, unit.position.1), unit.facing, unit.moved && on_ground(unit));
        if let Some(mut figures) = figures {
            figures.ground.observe_unit(unit, sim.battle.tick);
        }
    }
}

/// Every frame: copy each model unit's position, facing, men and morale into its picture,
/// and animate the figures (walk while moving, idle otherwise).
#[allow(clippy::too_many_arguments)]
pub fn sync_views(
    sim: Res<BattleSim>,
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    mut commands: Commands,
    mut action_rng: Option<ResMut<ActionRng>>,
    mut skin: Option<ResMut<SkinState>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    mut views: Query<(&UnitView, &DrawnPose, &mut Transform, Option<&mut UnitFigures>)>,
    mut men_tf: Query<&mut Transform, Without<UnitView>>,
    mut visibility: Query<&mut Visibility>,
    mut gizmos: Gizmos,
    cams: Query<&GlobalTransform, With<Camera3d>>,
    children: Query<&Children>,
    mut part_lods: Query<(&mut Mesh3d, &PartLods)>,
    mut pole_bones: Query<&mut PoleBone>,
    mut fade_upload: ResMut<skin::FadeUpload>,
) {
    // Test harness: `NAPOLEON_UNIT_LOD=0` keeps every man at LOD 0 (for frame-rate comparisons).
    let cam_pos = cams.iter().next().map(|g| g.translation()).filter(|_| std::env::var("NAPOLEON_UNIT_LOD").map_or(true, |v| v != "0"));
    let dt = time.delta_secs();
    let map = crate::terrain::current_map();
    let ground = |p: Vec2| map.as_ref().map_or(0.0, |m| m.height_at(p.x, p.y));
    // How far the next model tick has come: the drawn poses blend by it (see `DrawnPose`).
    let alpha = fixed.overstep_fraction();
    // This frame's cross-fades: no slot points into last frame's (a figure skipped this frame keeps
    // its frame, without the fade).
    if !fade_upload.entries.is_empty() {
        fade_upload.entries.clear();
        for f in skin.iter_mut().flat_map(|s| s.data.iter_mut()) {
            f[3] = 0;
        }
    }
    if let Some(s) = skin.as_mut() {
        s.frame = s.frame.wrapping_add(1).max(1);
    }
    let fades = &mut fade_upload.bypass_change_detection().entries;
    for (view, drawn, mut transform, figures) in &mut views {
        let Some(unit) = view.unit(&sim.battle.units) else { continue };
        let (pos, facing) = drawn.blend(Vec2::new(unit.position.0, unit.position.1), unit.facing, alpha);
        transform.translation = world_of(pos);
        // A figure faces local -Z; model facing 0 = +x. See `world_of` for the axes.
        transform.rotation = Quat::from_rotation_y(facing - std::f32::consts::FRAC_PI_2);
        let info = view.info(&sim);
        if let Some(info) = info.filter(|_| !unit.off_field()) {
            let half = info.size_m * 0.5;
            let corners = [(-half.x, -half.y), (half.x, -half.y), (half.x, half.y), (-half.x, half.y)]
                .map(|(x, z)| {
                    let p = transform.transform_point(Vec3::new(x, 0.0, z));
                    Vec3::new(p.x, ground(Vec2::new(p.x, -p.z)) + 0.15, p.z)
                });
            let color = unit_color(unit, sim.selected == Some(unit.id));
            for k in 0..4 {
                gizmos.line(corners[k], corners[(k + 1) % 4], color);
            }
        }
        let Some(mut figures) = figures else { continue };
        let moved = pos.distance(figures.last_pos);
        figures.last_pos = pos;
        // Re-seat the men on the terrain only when the drawn unit moved or turned (writing 2,000
        // transforms every frame would make Bevy re-propagate them all; a marching unit is drawn
        // somewhere new every frame, a standing one is not).
        let reseat = moved > 1e-4 || figures.last_facing != facing || !figures.seated;
        figures.last_facing = facing;
        figures.seated = true;
        // The ground speed picks the gait (walk below the walk/run midpoint).
        let gait = figures.ground.gait(unit.walk_speed, unit.run_speed);
        // What the unit is doing (display only, read from the model; see `actions`).
        let mode = if unit.in_melee {
            Mode::Melee
        } else if gait != Gait::Stand {
            Mode::Gait(gait)
        } else if unit.missile.is_some() && (unit.fire_target.is_some() || unit.reload_ticks_left > 0 || unit.aiming) {
            Mode::Ready { aiming: unit.aiming }
        } else {
            Mode::Gait(Gait::Stand)
        };
        let now = time.elapsed_secs();
        let Some(skin) = skin.as_mut() else { continue };
        let SkinState { atlas, data: skin_data, frame, .. } = &mut **skin;
        let frame = *frame;
        let mut fallback_rng = ActionRng(SelectionRng(1));
        let action_rng: &mut ActionRng = match action_rng.as_deref_mut() {
            Some(r) => r,
            None => &mut fallback_rng,
        };
        let length = |a: &std::sync::Arc<ntw_formats::anim::Anim>| atlas.slot(a).map_or(0.0, |s| s.play_length());
        let files = {
            let ranks = info.map_or(1, |i| i.ranks.max(1)) as usize;
            figures.men.len().div_ceil(ranks).max(1)
        };
        let figures = &mut *figures;
        // Events: men who fell, a volley, contact with charging cavalry.
        if unit.men < figures.last_men {
            let cause = match mode {
                Mode::Melee => DeathCause::Melee,
                Mode::Ready { .. } if unit.reload_ticks_left > 0 => DeathCause::Reloading,
                Mode::Ready { .. } => DeathCause::Poised,
                Mode::Gait(Gait::Walk) => DeathCause::Walking,
                Mode::Gait(Gait::Run) if unit.charging => DeathCause::Charging,
                Mode::Gait(Gait::Run) => DeathCause::Running,
                Mode::Gait(Gait::Stand) => DeathCause::Standing,
            };
            let speed = if matches!(mode, Mode::Gait(Gait::Walk | Gait::Run)) { figures.ground.speed } else { 0.0 };
            for i in (unit.men as usize)..(figures.last_men as usize).min(figures.men.len()) {
                if figures.acts[i].dead {
                    continue;
                }
                let kit = &figures.kits[i % figures.kits.len()];
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                if let Some(clips) = actions::death(kit, cause, speed, sel, &mut action_rng.0) {
                    figures.acts[i].play(now, vec![clips], true);
                    figures.acts[i].dead = true;
                    commands.entity(figures.men[i]).remove_parent_in_place().insert(Corpse);
                }
            }
        }
        figures.last_men = unit.men;
        let alive = (unit.men as usize).min(figures.men.len());
        if unit.volleys_fired > figures.last_volleys && mode != Mode::Melee {
            for i in 0..alive {
                let kit = &figures.kits[i % figures.kits.len()];
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                let reload = format!("RELOAD_{}", 1 + sel % 2);
                let chain: Vec<_> = [unit_animation::FIRE, reload.as_str()].iter().filter_map(|s| actions::pick(kit, s, sel)).collect();
                figures.acts[i].play(now + actions::volley_delay(sel), chain, false);
            }
        }
        figures.last_volleys = unit.volleys_fired;
        if unit.in_melee && !figures.last_melee {
            let charged = sim.battle.units.iter().any(|e| {
                e.side != unit.side && e.is_cavalry && e.charging && e.in_melee && {
                    let (dx, dy) = (e.position.0 - unit.position.0, e.position.1 - unit.position.1);
                    dx * dx + dy * dy < 80.0 * 80.0
                }
            });
            let knockdowns = figures.kits.first().map(|k| actions::present(k, unit_animation::KNOCKDOWN, false)).unwrap_or_default();
            for i in 0..alive {
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                figures.acts[i].next_attack = now + (sel % 10) as f32 * 0.2;
                // PROVISIONAL: front-rank men knocked down by a charge, about 3 in 10.
                if charged && i < files && !knockdowns.is_empty() && family_pick(&mut action_rng.0, 10) < 3 {
                    let kit = &figures.kits[i % figures.kits.len()];
                    let k = family_pick(&mut action_rng.0, knockdowns.len() as u32) as usize;
                    let chain: Vec<_> = [knockdowns[k].as_str(), unit_animation::FACE_DOWN_GET_UP].iter().filter_map(|s| actions::pick(kit, s, sel)).collect();
                    figures.acts[i].play(now, chain, false);
                }
            }
        }
        figures.last_melee = unit.in_melee;
        for (i, man) in figures.men.iter().enumerate() {
            let dead = figures.acts[i].dead;
            // Each man stands on the terrain under him (the unit's own origin is on the ground
            // at its centre; the parent only turns about Y, so a local y offset = a world one).
            // The fallen are detached and stay where they fell.
            if reseat && !dead && let Ok(mut man_tf) = men_tf.get_mut(*man) {
                let p = transform.transform_point(Vec3::new(man_tf.translation.x, 0.0, man_tf.translation.z));
                man_tf.translation.y = ground(Vec2::new(p.x, -p.z)) - transform.translation.y;
            }
            // Part LODs by camera distance (CONFIRMED switch distances; only on a change).
            if !dead && let (Some(cam), Ok(man_tf)) = (cam_pos, men_tf.get(*man)) {
                let d2 = transform.transform_point(man_tf.translation).distance_squared(cam);
                let soldier = unit_lod(d2, 4) as u8;
                // Horses: the `warscape_animated_lod` ranges (INFERRED metres).
                let horse = figures.kits[i % figures.kits.len()].mount.as_ref().map_or(0, |m| ntw_formats::mount::animated_lod(&m.lod_distances, d2.sqrt())) as u8;
                let want = soldier | (horse << 4);
                if figures.lods[i] != want {
                    figures.lods[i] = want;
                    for c in children.get(*man).into_iter().flatten() {
                        if let Ok((mut mesh, lods)) = part_lods.get_mut(*c) {
                            let level = if lods.1 { horse } else { soldier } as usize;
                            let h = &lods.0[level.min(lods.0.len() - 1)];
                            if mesh.0 != *h {
                                mesh.0 = h.clone();
                            }
                        }
                    }
                }
            }
            if let Ok(mut v) = visibility.get_mut(*man) {
                // Reinforcements still off the field (waiting or held) are not drawn, nor a garrison
                // inside its building (PROVISIONAL: the original shows men at the windows).
                let off_field = unit.off_field() || unit.garrison.is_some();
                let want = if (i < unit.men as usize || dead) && !off_field { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
        }
        // The speed level whose clip speed is closest to the unit's, played at
        // `speed / clip speed` (`unit_animation::pick_level`). All kits of a unit have the
        // same clips, so the first kit's levels pick for every man.
        let Some(levels) = figures.kits.first().and_then(|k| k.gait_levels(gait)) else { continue };
        let ground_speed = if gait == Gait::Stand { 0.0 } else { figures.ground.speed };
        let Some((li, rate)) = pick_level(levels.iter().map(|l| l.speed), ground_speed) else { continue };
        // PROVISIONAL clamp (ours): the exe's playback-rate limits are not traced. (A placement
        // does not show as a huge speed: `GroundSpeed` skips the tick a unit is placed in.)
        let advance = dt * rate.clamp(0.25, 4.0);
        // Every man plays his own alternative clips, with his own clip time (GPU skinning): a
        // one-shot if one runs, else the loop of the unit's mode.
        for i in 0..figures.men.len() {
            let shown = figures.blends[i].shown(frame);
            let kit = &figures.kits[i % figures.kits.len()];
            let sel = figures.selections.get(i).copied().unwrap_or(0);
            let act = &mut figures.acts[i];
            act.advance(now, length);
            if mode == Mode::Melee && !act.dead && i < alive && act.shot.is_none() && now >= act.next_attack && !figures.melee_attack.is_empty() {
                let k = family_pick(&mut action_rng.0, figures.melee_attack.len() as u32) as usize;
                if let Some(clips) = actions::pick(kit, &figures.melee_attack[k], sel) {
                    act.next_attack = now + length(&clips.0) + actions::swing_gap(sel);
                    act.play(now, vec![clips], false);
                }
            }
            // The loop: gait level, or the mode's action loop (stand level if the table has none).
            let Some(level) = kit.gait_levels(gait).and_then(|l| l.get(li)) else { continue };
            let loop_slot = match mode {
                Mode::Ready { aiming } if aiming && kit.actions.contains_key(unit_animation::AIM) => Some(unit_animation::AIM),
                Mode::Ready { .. } => Some(unit_animation::COMBAT_READY),
                Mode::Melee => figures.melee_idle.get(alternative(sel, figures.melee_idle.len().max(1))).map(String::as_str),
                Mode::Gait(_) => None,
            };
            // The action loop's man clip and line; its mount clip, else the gait level's.
            let action = loop_slot.and_then(|s| kit.actions.get(s));
            let man_level = action.unwrap_or(level);
            let mount_level = action.filter(|a| !a.mount.is_empty()).unwrap_or(level);
            let loop_man = &man_level.man[alternative(sel, man_level.man.len())];
            let loop_mount = (!mount_level.mount.is_empty()).then(|| &mount_level.mount[alternative(sel, mount_level.mount.len())]);
            let slot = figures.first_slot + i;
            let shot = act.shot.as_ref().filter(|s| now >= s.start);
            let (man_fig, mount_fig) = match shot {
                Some(s) => {
                    let t = now - s.start;
                    let m = atlas.slot(&s.man).map(|c| c.figure_once(t));
                    let h = s.mount.as_ref().and_then(|a| atlas.slot(a)).map(|c| c.figure_once(t));
                    (m, h)
                }
                None => (None, None),
            };
            let Some(man_clip) = atlas.slot(loop_man) else { continue };
            // A paired mount clip has the same frame count: same frame, same blend.
            let want = LoopClip {
                key: std::sync::Arc::as_ptr(loop_man) as usize,
                man: man_clip,
                mount: loop_mount.and_then(|m| atlas.slot(m)),
                blend_in: man_level.blend_in_of(sel),
            };
            let blend = &mut figures.blends[i];
            let speed = || clip_speed(loop_man, loop_mount.map(|m| &**m));
            blend.step(&want, shown, speed, skin::phase(view.id, i), advance, dt, man_fig.is_some());
            let [man_now, mount_now] = blend.draw(man_fig, mount_fig, frame);
            put_figure(&mut skin_data[2 * slot], man_now, blend.fade(0), fades);
            if mount_fig.is_none() && blend.mount.is_none() {
                // On foot the mount slot is unused: the man's slot again, without a second fade.
                skin_data[2 * slot + 1] = skin_data[2 * slot];
            } else {
                put_figure(&mut skin_data[2 * slot + 1], mount_now, blend.fade(1), fades);
            }
        }
        // The standard bearer: the same clip choice as a man, on his own figure slot, and the
        // pole bone's matrix for the frame he is on so `flag::sync_flags` can hang the cloth from
        // it. He is not one of `unit.men`, so he never fires and never dies with the count.
        if let Some(bearer) = figures.bearer.as_ref() {
            let blend = &mut figures.blends[figures.men.len()];
            let shown = blend.shown(frame);
            // A stable selection number of his own, from the unit's first man (deterministic, and
            // it only picks which alternative clip of the gait he plays).
            let sel = figures.selections.first().copied().unwrap_or(0);
            let level = bearer.kit.gait_levels(gait).and_then(|l| l.get(li));
            // Without levels he plays the kit's per-gait clip, whose line is not kept: drawn at
            // once on a change (no blend time known).
            let clip = level.map(|l| (&l.man[alternative(sel, l.man.len())], l.blend_in_of(sel))).or_else(|| bearer.kit.anims(gait).map(|a| (&a.man, 0.0)));
            let bslot = figures.first_slot + figures.men.len();
            if let Some((clip, blend_in)) = clip
                && let Some(slot_atlas) = atlas.slot(clip)
                && bslot * 2 + 1 < skin_data.len()
            {
                let want = LoopClip { key: std::sync::Arc::as_ptr(clip) as usize, man: slot_atlas, mount: None, blend_in };
                blend.step(&want, shown, || clip.root_speed(), skin::phase(view.id, usize::MAX), advance, dt, false);
                let [fig, _] = blend.draw(None, None, frame);
                let fade = blend.fade(0);
                put_figure(&mut skin_data[2 * bslot], fig, fade, fades);
                skin_data[2 * bslot + 1] = skin_data[2 * bslot];
                if let Some(bone) = bone_at(atlas, &fig, fade.as_ref(), ntw_formats::cloth::POLE_BONE as usize)
                    && let Ok(mut p) = pole_bones.get_mut(bearer.entity)
                {
                    p.model = bone;
                }
            }
            // He stands on the terrain under him, and is hidden with the rest of his unit.
            if reseat
                && let (Ok(mut b_tf), Ok(mut v)) = (men_tf.get_mut(bearer.entity), visibility.get_mut(bearer.entity))
            {
                let p = transform.transform_point(Vec3::new(b_tf.translation.x, 0.0, b_tf.translation.z));
                b_tf.translation.y = ground(Vec2::new(p.x, -p.z)) - transform.translation.y;
                let off_field = unit.off_field() || unit.garrison.is_some();
                let want = if off_field { Visibility::Hidden } else { Visibility::Inherited };
                if *v != want {
                    *v = want;
                }
            }
        }
    }
    // Fades pushed above bypass change detection: mark the frame's entries for the render world.
    if !fade_upload.entries.is_empty() {
        fade_upload.set_changed();
    }
    if let Some(skin) = skin.as_ref()
        && let Some(mut buffer) = buffers.get_mut(&skin.figures)
    {
        skin::write_figure_bytes(&skin.data, buffer.data.get_or_insert_default());
    }
}

/// One bone's matrix, in the figure's own model space, as the skinning shader poses it: the clip
/// frame the figure shows (`[frame A start, frame B start, blend A->B bits, _]`,
/// `skin::ClipSlot::figure`), cross-faded with its frozen poses when it has a [`skin::Fade`].
fn bone_at(atlas: &BoneAtlas, frame: &[u32; 4], fade: Option<&skin::Fade>, bone: usize) -> Option<[f32; 16]> {
    let now = frame_bone(atlas, frame, bone)?;
    let Some(fade) = fade else { return Some(now) };
    let mut out = now;
    for f in fade {
        let keep = f32::from_bits(f[3]);
        if keep > 0.0
            && let Some(m) = frame_bone(atlas, f, bone)
        {
            for k in 0..16 {
                out[k] += (m[k] - now[k]) * keep;
            }
        }
    }
    Some(out)
}

/// One bone's matrix from a clip frame `[frame A start, frame B start, blend A->B bits, _]`
/// (`skin::ClipSlot::figure`). The bone's own index offsets into the frame.
fn frame_bone(atlas: &BoneAtlas, frame: &[u32; 4], bone: usize) -> Option<[f32; 16]> {
    let a = (frame[0] as usize).checked_add(bone)?;
    let b = (frame[1] as usize).checked_add(bone)?;
    let t = f32::from_bits(frame[2]);
    let ma = atlas.matrices.get(a)?;
    let mb = atlas.matrices.get(b).unwrap_or(ma);
    Some(std::array::from_fn(|k| ma[k] + (mb[k] - ma[k]) * t))
}

fn find(sim: &BattleSim, id: u32) -> Option<&LandUnit> {
    sim.battle.units.iter().find(|u| u.id == id)
}

/// Blue for France (side 0), red for Austria (side 1). Paler as morale drops; grey when
/// shattered or destroyed; yellow when selected.
fn unit_color(unit: &LandUnit, selected: bool) -> Color {
    if selected {
        return Color::srgb(1.0, 0.9, 0.2);
    }
    let base = if unit.side == 0 {
        Vec3::new(0.15, 0.3, 0.85)
    } else {
        Vec3::new(0.85, 0.15, 0.15)
    };
    let fade = match unit.morale.state {
        MoraleState::Shattered => return Color::srgb(0.45, 0.45, 0.45),
        _ if unit.men == 0 => return Color::srgb(0.45, 0.45, 0.45),
        MoraleState::Broken => 0.65,
        MoraleState::Wavering => 0.45,
        MoraleState::Shaken => 0.25,
        _ => 0.0,
    };
    let c = base.lerp(Vec3::ONE, fade);
    Color::srgb(c.x, c.y, c.z)
}

/// Every frame: draws recent volleys (display only, from [`VolleyFx`]) with Bevy "gizmos"
/// (simple lines and circles drawn fresh every frame).
/// - A volley is a pale-yellow line from the shooter to the target plus a "flash" circle at the
///   shooter; both fade out over `VOLLEY_FX_SECONDS`. The more men it killed, the bigger the
///   flash at the target.
/// - The selected unit, if it can shoot, gets a faint ring showing its missile range, and a red
///   line to the enemy it was ordered to shoot at.
pub fn draw_volleys(sim: Res<BattleSim>, fx: Res<VolleyFx>, mut gizmos: Gizmos) {
    let now = sim.battle.time_seconds();
    let at = |id: u32| find(&sim, id).map(|u| world_of(Vec2::new(u.position.0, u.position.1)) + Vec3::Y * 1.4);
    let flat = |p: Vec3| Isometry3d::new(p, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    for (fired_at, v) in &fx.recent {
        let (Some(from), Some(to)) = (at(v.shooter), at(v.target)) else { continue };
        let fade = (1.0 - (now - fired_at) / VOLLEY_FX_SECONDS).clamp(0.0, 1.0);
        gizmos.line(from, to, Color::srgba(1.0, 0.95, 0.6, 0.8 * fade));
        gizmos.circle(flat(from), 3.0, Color::srgba(1.0, 1.0, 0.8, fade));
        if v.kills > 0 {
            let r = 2.0 + v.kills as f32 * 0.5;
            gizmos.circle(flat(to), r, Color::srgba(1.0, 0.3, 0.2, fade));
        }
    }
    if let Some(unit) = sim.selected.and_then(|id| find(&sim, id))
        && unit.missile.is_some()
    {
        let pos = world_of(Vec2::new(unit.position.0, unit.position.1)) + Vec3::Y * 0.2;
        let range = unit.missile_range(sim.battle.kv_rules.fire_on_walls_range_modifier);
        gizmos.circle(flat(pos), range, Color::srgba(1.0, 1.0, 1.0, 0.25)).resolution(64);
        if let Some(target) = unit.fire_target.and_then(at) {
            gizmos.line(pos, target, Color::srgba(1.0, 0.2, 0.2, 0.7));
        }
    }
}

/// Test harness: with `NAPOLEON_BATTLE_SCREENSHOT=<file.png>` set, saves one screenshot
/// after a few seconds and quits (for checking the 3D view without clicking).
pub fn screenshot_from_env(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut done: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = std::env::var_os("NAPOLEON_BATTLE_SCREENSHOT") else { return };
    let t = time.elapsed_secs();
    if !*done && t > 8.0 {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(std::path::PathBuf::from(path)));
        *done = true;
    }
    if *done && t > 9.5 {
        exit.write(AppExit::Success);
    }
}

/// A box around one standing figure in its own space: a man ~2 m tall, a horse ~3 m long.
fn figure_bounds(mount: bool) -> bevy::camera::primitives::Aabb {
    let (centre, half) = if mount { (Vec3::new(0.0, 1.4, 0.0), Vec3::new(1.2, 1.8, 2.2)) } else { (Vec3::new(0.0, 1.1, 0.0), Vec3::new(1.2, 1.4, 1.2)) };
    bevy::camera::primitives::Aabb::from_min_max(centre - half, centre + half)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_sim::battle::TICK_SECONDS;

    /// The user's report (2026-10-08): walking men looked low-fps, drawn in 0.1 s model steps.
    /// Between ticks a marching unit is drawn on the line between its last two tick poses.
    #[test]
    fn a_marching_unit_is_drawn_between_its_ticks() {
        let mut pose = DrawnPose::at(Vec2::ZERO, 0.0);
        let step = Vec2::new(1.4 * TICK_SECONDS, 0.0);
        pose.observe(step, 0.0, true);
        let frames: Vec<f32> = [0.0, 0.25, 0.5, 0.75, 1.0].iter().map(|&a| pose.blend(step, 0.0, a).0.x).collect();
        for w in frames.windows(2) {
            assert!(w[1] > w[0], "moves every frame: {frames:?}");
        }
        assert!((frames[2] - step.x * 0.5).abs() < 1e-6, "half way at half a tick");
        assert_eq!(pose.blend(step, 0.0, 1.0).0, step, "at the model pose when the next tick is due");
    }

    /// Polish (O(n²) per tick): a view reads its unit at its kept slot; a unit added before it
    /// (a reinforcement, `Battle::add_unit` keeps id order) moves it, and the view finds it again
    /// by id and keeps the new slot; a unit no longer there is none.
    #[test]
    fn a_view_finds_its_unit_at_its_slot_and_again_after_it_moved() {
        let mut units = vec![LandUnit::new(2, 0, 100, (0.0, 0.0)), LandUnit::new(5, 1, 100, (0.0, 0.0))];
        let mut view = UnitView::new(5, 1);
        assert_eq!(view.observe(&units).map(|u| u.id), Some(5));
        units.insert(0, LandUnit::new(1, 0, 100, (0.0, 0.0)));
        assert_eq!(view.unit(&units).map(|u| u.id), Some(5), "found by id while the slot is stale");
        assert_eq!(view.observe(&units).map(|u| u.id), Some(5));
        assert_eq!(view.slot, 2, "the new slot is kept");
        units.retain(|u| u.id != 5);
        assert!(view.unit(&units).is_none() && view.observe(&units).is_none());
    }

    /// Review of the polish-hotpaths branch: `sync_views` read each view's info by a linear search
    /// of `sim.info` (twice per view per frame), and `observe_ticks` marked every view changed
    /// each tick. The info is read at the view's slot (`info` keeps `battle.units`' order), also
    /// after a lower-id reinforcement moved it, and a view is changed only when its slot moved.
    #[test]
    fn a_view_reads_its_info_at_its_slot_and_changes_only_when_moved() {
        use bevy::ecs::system::RunSystemOnce;
        use ntw_sim::battle::fatigue::KvFatigue;
        use ntw_sim::battle::model::Battle;
        use ntw_sim::battle::morale::KvMorale;
        let info = |id| UnitInfo { id, ranks: id, ..UnitInfo::default() };
        let mut sim = BattleSim::new(Battle::new(1, KvMorale::default(), KvFatigue::default()), 1);
        sim.add_unit(LandUnit::new(2, 0, 100, (0.0, 0.0)), info(2));
        sim.add_unit(LandUnit::new(5, 1, 100, (0.0, 900.0)), info(5));
        let mut world = World::new();
        world.insert_resource(sim);
        let e = world.spawn((UnitView::new(5, 1), DrawnPose::at(Vec2::ZERO, 0.0))).id();
        let changed = |world: &World| world.entity(e).get_ref::<UnitView>().unwrap().last_changed();
        let spawned = changed(&world);
        world.run_system_once(observe_ticks).unwrap();
        assert_eq!(changed(&world), spawned, "slot unmoved: the view is not marked changed");
        world.resource_mut::<BattleSim>().add_unit(LandUnit::new(1, 0, 100, (0.0, -50.0)), info(1));
        world.run_system_once(observe_ticks).unwrap();
        assert_ne!(changed(&world), spawned, "slot moved: kept");
        let view = world.entity(e).get::<UnitView>().unwrap();
        assert_eq!(view.slot, 2);
        assert_eq!(view.info(world.resource::<BattleSim>()).map(|i| i.ranks), Some(5));
    }

    /// Anything that is not a moving tick is drawn at once: a still tick, a placement jump, and a
    /// pose the model changed outside a tick (a deployment drag, a HUD turn).
    #[test]
    fn a_pose_change_without_a_moving_tick_is_not_blended() {
        let mut pose = DrawnPose::at(Vec2::ZERO, 0.0);
        pose.observe(Vec2::new(0.1, 0.0), 0.0, false);
        assert_eq!(pose.blend(Vec2::new(0.1, 0.0), 0.0, 0.0).0, Vec2::new(0.1, 0.0), "not moved by the model");
        pose.observe(Vec2::new(50.0, 0.0), 0.0, true);
        assert_eq!(pose.blend(Vec2::new(50.0, 0.0), 0.0, 0.0).0, Vec2::new(50.0, 0.0), "a placement jump");
        let dragged = Vec2::new(60.0, 5.0);
        assert_eq!(pose.blend(dragged, 1.0, 0.3), (dragged, 1.0), "changed outside a tick");
    }

    /// The facing blends the shorter way round (from just under +pi to just over -pi).
    #[test]
    fn the_facing_turns_the_short_way() {
        let mut pose = DrawnPose::at(Vec2::ZERO, 3.1);
        pose.observe(Vec2::new(0.1, 0.0), -3.1, true);
        let (_, f) = pose.blend(Vec2::new(0.1, 0.0), -3.1, 0.5);
        assert!((f.rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI).abs() < 1e-3, "{f}");
    }

    /// Line infantry's walk and run clip speeds (`MUS_T_Walk_100/127/173`, `Jog_219/313`,
    /// analysis/units/CAVALRY.md §7) and a line battalion's `battle_entities` speeds.
    const WALK_LEVELS: [f32; 3] = [1.0, 1.27, 1.73];
    const RUN_LEVELS: [f32; 2] = [2.19, 3.13];
    const WALK: f32 = 1.4;
    const RUN: f32 = 3.6;

    /// Plays the app's frame loop: the model ticks at 10 Hz on the fixed clock (`FixedUpdate`
    /// before `Update`), moving the unit `speed × 0.1 s` per tick, and the view observes every
    /// tick (`observe_ticks`). Returns each frame's (gait, speed level, playback rate) from the
    /// second tick on.
    fn frames(speed: f32, frame_dts: &[f32], count: usize) -> Vec<(Gait, usize, f32)> {
        let (mut tick, mut x, mut fixed) = (0u32, 0.0f32, 0.0f32);
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        let mut out = Vec::new();
        for f in 0..count {
            let dt = frame_dts[f % frame_dts.len()];
            fixed += dt;
            while fixed >= TICK_SECONDS {
                fixed -= TICK_SECONDS;
                tick += 1;
                x += speed * TICK_SECONDS;
                g.observe(Vec2::new(x, 0.0), tick, true, true);
            }
            let gait = g.gait(WALK, RUN);
            let levels: &[f32] = if gait == Gait::Run { &RUN_LEVELS } else { &WALK_LEVELS };
            let (level, rate) = pick_level(levels.iter().copied(), g.speed).unwrap();
            if tick >= 2 {
                out.push((gait, level, rate));
            }
        }
        out
    }

    /// The walking jitter (2026-10-04 to 10-07): the view took the unit's speed from its
    /// per-frame displacement, but the model only moves it on 10 Hz ticks, so the smoothed speed
    /// swung by about ±20 % every tick and the men switched speed level (and clip) and playback
    /// rate several times a second. A unit marching at a steady speed must play one level at one
    /// rate, whatever the frame rate.
    #[test]
    fn steady_march_keeps_one_gait_level_and_rate() {
        for (speed, gait) in [(WALK, Gait::Walk), (RUN, Gait::Run)] {
            for dts in [&[1.0 / 60.0][..], &[1.0 / 144.0], &[1.0 / 30.0, 1.0 / 90.0, 1.0 / 45.0], &[1.0 / 20.0], &[0.35]] {
                let out = frames(speed, dts, 600);
                let (_, level0, rate0) = out[0];
                for (i, &(g, level, rate)) in out.iter().enumerate() {
                    assert_eq!(g, gait, "speed {speed}, frames {dts:?}: frame {i} gait");
                    assert_eq!(level, level0, "speed {speed}, frames {dts:?}: frame {i} speed level");
                    assert!((rate - rate0).abs() < 1e-3, "speed {speed}, frames {dts:?}: frame {i} rate {rate} vs {rate0}");
                }
            }
        }
    }

    /// A unit that stops stands after [`STILL_TICKS`] ticks without a move.
    #[test]
    fn stops_then_stands() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        let mut x = 0.0;
        for tick in 1..=10u32 {
            x += WALK * TICK_SECONDS;
            g.observe(Vec2::new(x, 0.0), tick, true, true);
            assert_eq!(g.gait(WALK, RUN), Gait::Walk, "tick {tick}");
        }
        for tick in 11..10 + STILL_TICKS {
            g.observe(Vec2::new(x, 0.0), tick, false, true);
            assert_eq!(g.gait(WALK, RUN), Gait::Walk, "tick {tick}");
        }
        g.observe(Vec2::new(x, 0.0), 10 + STILL_TICKS, false, true);
        assert_eq!(g.gait(WALK, RUN), Gait::Stand);
    }

    /// One frame that spans several model ticks (low frame rate, fast battle speed) in which the
    /// unit moved in only some: the speed was the frame's distance over all its ticks, a third of
    /// the walk here, until the unit's next moving tick (at a start or a stop). Each tick is
    /// observed on its own now, so the frame sees the walking speed.
    #[test]
    fn a_frame_of_several_ticks_keeps_the_moving_ticks_speed() {
        let step = WALK * TICK_SECONDS;
        // A start: still for two ticks of the frame, then a step.
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        for (tick, x, moved) in [(1, 0.0, false), (2, 0.0, false), (3, step, true)] {
            g.observe(Vec2::new(x, 0.0), tick, moved, true);
        }
        assert_eq!(g.gait(WALK, RUN), Gait::Walk);
        assert!((g.speed - WALK).abs() < 1e-3, "start: speed {}", g.speed);
        // A stop: a step, then two still ticks in the same frame.
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        for (tick, x, moved) in [(1, step, true), (2, step, false), (3, step, false)] {
            g.observe(Vec2::new(x, 0.0), tick, moved, true);
        }
        assert_eq!(g.gait(WALK, RUN), Gait::Walk);
        assert!((g.speed - WALK).abs() < 1e-3, "stop: speed {}", g.speed);
    }

    /// A reinforcement's arrival, as the model really steps it: `reinforcements_step` puts the
    /// unit at the field edge and its move walks it a first step in the same tick, so that tick's
    /// distance (hundreds of metres) read as a huge speed and the men ran. The arrival tick is
    /// skipped; from the next tick the unit walks at its walking speed.
    #[test]
    fn a_reinforcements_arrival_is_not_running() {
        use ntw_sim::battle::fatigue::KvFatigue;
        use ntw_sim::battle::model::{Battle, Reinforcement};
        use ntw_sim::battle::morale::KvMorale;
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 100, (0.0, 900.0)));
        let mut r = LandUnit::new(3, 0, 50, (0.0, 0.0));
        r.active = false;
        r.army_index = 1;
        r.reinforcement = Reinforcement::Waiting;
        r.unit_class = "infantry_line".into();
        b.add_unit(r);
        b.reinforcement_entries.push(((0, 1), (500.0, -400.0), 1.0));
        let unit = |b: &Battle| b.units.iter().find(|u| u.id == 3).unwrap().clone();
        let u = unit(&b);
        let mut g = GroundSpeed::new(Vec2::new(u.position.0, u.position.1), b.tick, on_ground(&u));
        b.step();
        let u = unit(&b);
        assert!(u.moved && matches!(u.reinforcement, Reinforcement::Arriving { .. }), "placed and moved in one tick");
        g.observe_unit(&u, b.tick);
        assert_eq!(g.gait(u.walk_speed, u.run_speed), Gait::Stand, "the arrival tick is not a step");
        b.step();
        let u = unit(&b);
        g.observe_unit(&u, b.tick);
        assert_eq!(g.gait(u.walk_speed, u.run_speed), Gait::Walk, "speed {}", g.speed);
        assert!(g.speed <= u.walk_speed + 1e-3, "speed {} walk {}", g.speed, u.walk_speed);
    }

    /// A deployment drag moves the unit with no model tick: not walking.
    #[test]
    fn a_deployment_drag_is_not_walking() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        g.observe(Vec2::new(40.0, 0.0), 0, false, true);
        assert_eq!(g.gait(WALK, RUN), Gait::Stand);
    }

    /// The tick going back (not expected: a restart rebuilds the views) starts over instead of
    /// spreading the next step over ~4 billion ticks.
    #[test]
    fn tick_going_back_starts_over() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        g.observe(Vec2::new(0.14, 0.0), 1, true, true);
        g.observe(Vec2::new(50.0, 0.0), 0, false, true);
        assert_eq!(g.gait(WALK, RUN), Gait::Stand);
        g.observe(Vec2::new(50.14, 0.0), 1, true, true);
        assert!((g.speed - WALK).abs() < 1e-3, "speed {}", g.speed);
    }

    /// A restart (R) builds a new battle with the same unit ids. It was told only by the tick
    /// going back, which a restart at tick ≤ 1 that reached the old tick by the next frame did not
    /// show, so the old views (their speeds, men and corpses) stayed. The views now carry the
    /// battle they were built for.
    #[test]
    fn a_restart_rebuilds_the_views() {
        let ids = [3u32, 7, 9];
        assert!(views_current(Some(4), 4, ids.iter().copied(), ids.iter().copied()));
        assert!(!views_current(Some(4), 5, ids.iter().copied(), ids.iter().copied()), "restart");
        assert!(!views_current(None, 4, ids.iter().copied(), ids.iter().copied()), "first frame");
        assert!(!views_current(Some(4), 4, ids[..2].iter().copied(), ids.iter().copied()), "a unit without a view");
        assert!(!views_current(Some(4), 4, ids.iter().copied(), ids[..2].iter().copied()), "a view without a unit");
    }

    /// Every battle built gets its own number, a restart's with the same seed and units too.
    #[test]
    fn every_battle_gets_its_own_build() {
        let (a, b) = (super::super::next_build(), super::super::next_build());
        assert_ne!(a, b);
    }

    /// Loop clips (their `Arc` addresses in `sync_views`), each 1 s at 20 frames/s, 1 bone, at
    /// their own place in the bone atlas.
    fn clip(key: usize, blend_in: f32) -> LoopClip {
        LoopClip { key, man: ClipSlot { base: key as u32, frames: 20, bones: 1, rate: 20.0 }, mount: None, blend_in }
    }
    const A: usize = 0x1000;
    const B: usize = 0x2000;
    const C: usize = 0x3000;
    const D: usize = 0x4000;
    /// A stand clip's root speed (m/s); clips moving at [`WALK`] count as moving.
    const STAND: f32 = 0.0;

    /// A figure and the number of the frame it is on.
    #[derive(Default)]
    struct Fig {
        b: ClipBlend,
        frame: u32,
    }
    impl std::ops::Deref for Fig {
        type Target = ClipBlend;
        fn deref(&self) -> &ClipBlend {
            &self.b
        }
    }
    impl std::ops::DerefMut for Fig {
        fn deref_mut(&mut self) -> &mut ClipBlend {
            &mut self.b
        }
    }
    impl Fig {
        /// A new frame starts: what the figure drew the frame before, if it was drawn then.
        fn next(&mut self) -> Option<[[u32; 4]; 2]> {
            self.frame += 1;
            self.b.shown(self.frame)
        }
        /// One frame as `sync_views` runs it: step, draw.
        fn run(&mut self, want: &LoopClip, speed: f32, dt: f32) -> [[u32; 4]; 2] {
            let shown = self.next();
            self.b.step(want, shown, || speed, 0.0, dt, dt, false);
            self.b.draw(None, None, self.frame)
        }
    }

    /// The weights of the pose shown at a change from A to B, every frame from the change until
    /// it is gone (dt and blend times are exact in binary, so the products are too).
    fn fade(blend_in: f32, dt: f32) -> Vec<f32> {
        let mut b = Fig::default();
        b.run(&clip(A, blend_in), WALK, dt);
        assert_eq!(b.frozen[0].keep, 0.0, "the first clip is drawn at once");
        let shown = b.run(&clip(A, blend_in), WALK, dt);
        assert_eq!(b.frozen[0].keep, 0.0, "no change");
        b.run(&clip(B, blend_in), WALK, dt);
        assert_eq!(b.frozen[0].figs, shown, "fades out of what was drawn the frame before the change");
        let mut keeps = vec![b.frozen[0].keep];
        while *keeps.last().unwrap() > 0.0 {
            b.run(&clip(B, blend_in), WALK, dt);
            keeps.push(b.frozen[0].keep);
            assert!(keeps.len() < 100, "the cross-fade ends");
        }
        keeps
    }

    /// `0x007725D0`: the pose shown at a clip change keeps weight prod(1 - e_k / D): whole on the
    /// change frame, gone once e >= D. D = 0.5 s (most vanilla WALK / RUN lines), 8 frames/s.
    #[test]
    fn a_clip_change_cross_fades_over_the_lines_blend_in_time() {
        // e = 0, 0.125, 0.25, 0.375, 0.5: weights 1, 0.75, 0.75 x 0.5, 0.375 x 0.25, gone.
        assert_eq!(fade(0.5, 0.125), [1.0, 0.75, 0.375, 0.09375, 0.0]);
        // A 0.25 s line (the other vanilla gait value) is gone twice as soon.
        assert_eq!(fade(0.25, 0.125), [1.0, 0.5, 0.0]);
    }

    /// A line without `blend_in_time` gets the parser's 1.0 s (`FragmentClip::blend_in`): still
    /// blending half way, gone at 1.0 s.
    #[test]
    fn a_line_without_blend_in_time_cross_fades_over_one_second() {
        let line = |blend_in_time| ntw_formats::battle_animation::FragmentClip { filename: "w.anim".into(), blend_in_time, attributes: vec![] };
        assert_eq!(line(Some(0.25)).blend_in(), 0.25);
        let d = line(None).blend_in();
        assert_eq!(d, 1.0);
        let keeps = fade(d, 0.125);
        assert_eq!(keeps.len(), 9, "change frame + e = 0.125 .. 1.0: {keeps:?}");
        assert!(keeps[4] > 0.0 && keeps[4] < keeps[3], "mid-way at e = 0.5: {keeps:?}");
        assert!(keeps.windows(2).all(|w| w[1] < w[0]), "fades steadily: {keeps:?}");
    }

    /// Each man's line: the alternative he plays gives the blend time.
    #[test]
    fn the_blend_time_is_the_played_alternatives_line() {
        let anim = || std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration: 1.0, bones: vec![], frames: vec![], events: vec![] });
        let level = crate::soldiers::KitLevel { speed: 1.4, man: vec![anim(), anim()], blend_in: vec![0.5, 0.25], mount: vec![] };
        assert_eq!([0, 1, 2, 3].map(|sel| level.blend_in_of(sel)), [0.5, 0.25, 0.5, 0.25]);
    }

    /// A zero blend time draws the new clip at once (the exe draws the new clip alone for D = 0).
    #[test]
    fn a_zero_blend_time_snaps() {
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(B, 0.0), WALK, 0.125);
        assert_eq!(b.frozen, [Frozen::default(); 2]);
        assert_eq!(b.fade(0), None);
    }

    /// Review bug 1: each figure keeps its own clip time, so a figure whose clip did not change
    /// (a standard bearer, a man whose kit has no AIM) goes on smoothly when another man's clip
    /// changes; and the man who changes carries his own phase.
    #[test]
    fn a_figure_whose_clip_did_not_change_keeps_its_time() {
        let (mut changing, mut steady) = (Fig::default(), Fig::default());
        for _ in 0..10 {
            changing.run(&clip(A, 0.5), WALK, 0.125);
            steady.run(&clip(A, 0.5), WALK, 0.125);
        }
        let before = steady.time;
        changing.run(&clip(B, 0.5), WALK, 0.125);
        steady.run(&clip(A, 0.5), WALK, 0.125);
        assert_eq!(steady.time, before + 0.125, "only the frame's advance");
        assert_eq!(steady.frozen[0].keep, 0.0, "and no cross-fade");
        // 1.25 s into the 1 s walk = a quarter step: B (also 1 s) goes on from there.
        assert_eq!(changing.time, 0.25 + 0.125);
    }

    /// Review bug 2: a change during a running cross-fade keeps the older pose at its weight and
    /// freezes the pose drawn last at the rest, so the drawn pose does not jump.
    #[test]
    fn a_change_mid_fade_does_not_drop_the_older_pose() {
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        let at_a = b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(B, 0.5), WALK, 0.125);
        let at_b = b.run(&clip(B, 0.5), WALK, 0.125);
        assert_eq!(b.frozen[0].keep, 0.75);
        // The level changes 0.125 s later: the frame drawn was B at 0.25 and A at 0.75.
        b.run(&clip(C, 0.5), WALK, 0.125);
        assert_eq!(b.frozen, [Frozen { figs: at_b, keep: 0.25 }, Frozen { figs: at_a, keep: 0.75 }]);
        assert_eq!(b.clip, C);
    }

    /// Runs `keys` as clip changes on consecutive frames (then holds the last), and returns the
    /// frames until the cross-fade is gone; checks every change is taken at once and the frozen
    /// weights never add up to more than 1.
    fn quick_changes(keys: &[usize], blend_in: f32, dt: f32) -> usize {
        let mut b = Fig::default();
        b.run(&clip(A, blend_in), WALK, dt);
        for &k in keys {
            b.run(&clip(k, blend_in), WALK, dt);
            assert_eq!(b.clip, k, "a change never waits");
            let sum: f32 = b.frozen.iter().map(|f| f.keep).sum();
            assert!(sum <= 1.0 + 1e-6, "weights {:?}", b.frozen);
        }
        let last = *keys.last().unwrap();
        let mut frames = 0;
        while b.fading() {
            b.run(&clip(last, blend_in), WALK, dt);
            frames += 1;
            assert!(frames < 1000, "the cross-fade ends");
        }
        assert_eq!(b.clip, last);
        assert_eq!(b.fade(0), None);
        frames
    }

    /// Round-2 review: a change on the frame right after a change (the old clip had no weight
    /// yet) used to leave a frozen pose that never faded and held every later change. Back-to-back
    /// changes, and three and four in quick succession, end on the newest clip with no fade within
    /// one blend time of the last change.
    #[test]
    fn back_to_back_changes_never_stick() {
        // D = 0.5 s at 8 frames/s: gone at e = 0.5, the 4th frame after the last change.
        assert_eq!(quick_changes(&[B, C], 0.5, 0.125), 4);
        assert_eq!(quick_changes(&[B, C, D], 0.5, 0.125), 4);
        assert_eq!(quick_changes(&[B, C, D, A], 0.5, 0.125), 4);
        assert_eq!(quick_changes(&[B, A, B, A, B], 0.5, 0.1), 5);
        // At ~10 frames/s the same, 3 frames at 0.25 s.
        assert_eq!(quick_changes(&[B, C, D, A], 0.25, 0.1), 3);
        // The change on the next frame: B had weight 0 yet, so only A stays, whole again as the
        // blend restarts (e = 0).
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        let at_a = b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(B, 0.5), WALK, 0.125);
        b.run(&clip(C, 0.5), WALK, 0.125);
        assert_eq!(b.frozen, [Frozen { figs: at_a, keep: 1.0 }, Frozen::default()]);
    }

    /// A third pose within one blend time: the lightest of the three is dropped and the other two
    /// scaled up to its weight, so the weights still add up to 1.
    #[test]
    fn a_third_pose_drops_the_lightest() {
        let pose = |n: u32| [[n, n, 0, 0], [n, n, 0, 0]];
        let old = [Frozen { figs: pose(1), keep: 0.25 }, Frozen { figs: pose(2), keep: 0.5 }];
        // The live clip had 0.25 too: the older 0.25 (the first of the lightest) goes.
        let out = freeze(old, pose(3));
        assert_eq!(out, [Frozen { figs: pose(3), keep: 1.0 / 3.0 }, Frozen { figs: pose(2), keep: 2.0 / 3.0 }]);
        let old = [Frozen { figs: pose(1), keep: 0.1 }, Frozen { figs: pose(2), keep: 0.3 }];
        let out = freeze(old, pose(3));
        assert_eq!(out.map(|f| f.figs), [pose(3), pose(2)]);
        assert!((out[0].keep + out[1].keep - 1.0).abs() < 1e-6);
    }

    /// Manager review: the mount clip is picked again every frame, also when the man's clip does
    /// not change (in melee the man keeps his idle clip while the horse follows the gait).
    #[test]
    fn the_mount_clip_follows_while_the_mans_stays() {
        let mut b = Fig::default();
        let stand = ClipSlot { base: 500, frames: 20, bones: 1, rate: 20.0 };
        let walk = ClipSlot { base: 900, ..stand };
        b.run(&LoopClip { mount: Some(stand), ..clip(A, 0.5) }, STAND, 0.125);
        let drawn = b.run(&LoopClip { mount: Some(walk), ..clip(A, 0.5) }, STAND, 0.125);
        assert_eq!(b.mount, Some(walk));
        assert_eq!(drawn[1], walk.figure(b.time));
    }

    /// Review round 3: a mount clip changing on its own also changes the pair's root speed, so the
    /// man's next change carries the phase when the horse moves (melee idle, horse walking, melee
    /// ends) and not when it stopped.
    #[test]
    fn a_mount_change_alone_updates_the_pairs_speed() {
        let stand = ClipSlot { base: 500, frames: 20, bones: 1, rate: 20.0 };
        let walk = ClipSlot { base: 900, ..stand };
        // The man on A throughout; the horse on `first` for 10 frames, then `then` (moving at
        // `speed`) for one; then the man changes to B.
        let time_after = |first: ClipSlot, first_speed: f32, then: ClipSlot, speed: f32| {
            let mut b = Fig::default();
            for _ in 0..10 {
                b.run(&LoopClip { mount: Some(first), ..clip(A, 0.5) }, first_speed, 0.125);
            }
            b.run(&LoopClip { mount: Some(then), ..clip(A, 0.5) }, speed, 0.125);
            assert_eq!(b.speed, speed);
            b.run(&LoopClip { mount: Some(then), ..clip(B, 0.5) }, speed, 0.125);
            b.time
        };
        // 1.375 s into the 1 s clip: B goes on from 0.375, plus the frame's advance.
        assert_eq!(time_after(stand, STAND, walk, WALK), 0.5);
        // The horse stopped: B starts at the figure's offset (0 here).
        assert_eq!(time_after(walk, WALK, stand, STAND), 0.125);
    }

    /// Review bug 4: the phase carries over from the old clip's own root speed: after a stand
    /// clip the new clip starts at the figure's own offset, after a walk at the walk's phase.
    #[test]
    fn the_phase_carries_by_the_old_clips_own_root_speed() {
        let step = |old_speed: f32| {
            let mut b = Fig::default();
            for _ in 0..10 {
                b.run(&clip(A, 0.5), old_speed, 0.125);
            }
            let shown = b.next();
            // The new clip is 0.5 s long; the figure's own offset is 0.5 of a clip.
            let half = LoopClip { man: ClipSlot { frames: 10, ..clip(B, 0.5).man }, ..clip(B, 0.5) };
            b.step(&half, shown, || WALK, 0.5, 0.0, 0.125, false);
            b.time
        };
        assert_eq!(step(WALK), 0.125, "1.25 s into a 1 s walk = a quarter of the 0.5 s clip");
        assert_eq!(step(STAND), 0.25, "from a still clip: the figure's own offset");
        assert_eq!(carry_phase(2.25, 1.0, 0.05, 0.5), None, "0.05 m/s is not moving (0x0131A7B0)");
        assert_eq!(carry_phase(2.25, 0.0, WALK, 0.5), None, "a zero-length clip");
    }

    /// Review bug 5: a figure that was not drawn last frame (`sync_views` skipped it) has no pose
    /// to fade from: its next clip is drawn at once.
    #[test]
    fn a_figure_not_drawn_last_frame_does_not_fade() {
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        // Skipped for two frames (its unit, or the man himself, hit a `continue`).
        b.frame += 2;
        b.run(&clip(B, 0.5), WALK, 0.125);
        assert_eq!(b.clip, B);
        assert_eq!(b.frozen[0].keep, 0.0);
    }

    /// Round-2 review: a figure skipped while it cross-fades was drawn without the fade then
    /// (`sync_views` clears every slot's fade each frame); drawn again, the fade does not come back
    /// with the poses frozen before the skip.
    #[test]
    fn a_skip_mid_fade_ends_the_fade() {
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(B, 0.5), WALK, 0.125);
        assert!(b.fading());
        // Skipped for one frame, then drawn again on the same clip.
        b.frame += 1;
        b.run(&clip(B, 0.5), WALK, 0.125);
        assert!(!b.fading());
        assert_eq!(b.fade(0), None);
    }

    /// A one-shot drawn over the loop: not blended, it ends a running cross-fade, and the loop
    /// clip is still followed, so the loop shown after it does not fade (PROVISIONAL).
    #[test]
    fn a_one_shot_follows_the_loop_clip_without_fading() {
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(B, 0.5), WALK, 0.125);
        assert_eq!(b.frozen[0].keep, 1.0);
        let shown = b.next();
        b.step(&clip(C, 0.5), shown, || WALK, 0.0, 0.125, 0.125, true);
        assert_eq!(b.b.draw(Some([7, 8, 0, 0]), None, b.frame)[0], [7, 8, 0, 0]);
        assert_eq!((b.clip, b.frozen[0].keep), (C, 0.0));
        b.run(&clip(C, 0.5), WALK, 0.125);
        assert_eq!(b.frozen[0].keep, 0.0);
    }

    /// The GPU side: a fading figure's slot points at its fade entry (1 + index), the others hold
    /// 0; and the pole bone is posed as the shader does: now + (frozen - now) x weight, per pose.
    #[test]
    fn fades_are_numbered_and_posed_like_the_shader() {
        let mut fades = Vec::new();
        let mut slots = [[9u32; 4]; 2];
        let fade = skin::fade([[1, 1, 0, 0], [2, 2, 0, 0]], [0.25, 0.5]);
        put_figure(&mut slots[0], [0, 0, 0, 0], None, &mut fades);
        put_figure(&mut slots[1], [0, 0, 0, 0], Some(fade), &mut fades);
        assert_eq!(slots, [[0, 0, 0, 0], [0, 0, 0, 1]]);
        assert_eq!(fades, [fade]);
        let mut atlas = BoneAtlas::default();
        atlas.matrices = vec![[0.0; 16], [4.0; 16], [8.0; 16]];
        // 0 x 0.25 + 4 x 0.25 + 8 x 0.5.
        assert_eq!(bone_at(&atlas, &[0, 0, 0, 1], Some(&fade), 0), Some([5.0; 16]));
        assert_eq!(bone_at(&atlas, &[1, 1, 0, 0], None, 0), Some([4.0; 16]));
    }
}

/// One drawable piece of a figure kit: GPU mesh, skin material, and whether it belongs to the mount.
type KitPart = (Vec<Handle<Mesh>>, Handle<SkinMaterial>, bool);

/// The LOD meshes of a figure part entity (most detailed first) and whether it belongs to the
/// mount (horses switch by `mount::animated_lod`, men by `unit_model::unit_lod`).
#[derive(Component)]
pub struct PartLods(pub Vec<Handle<Mesh>>, pub bool);

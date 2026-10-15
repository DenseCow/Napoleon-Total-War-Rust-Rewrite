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
use ntw_sim::battle::march::MoveKind;
use ntw_sim::battle::model::LandUnit;
use ntw_sim::battle::morale::MoraleState;

use super::skin::{self, BoneAtlas, ClipSlot, SkinExt, SkinMaterial};
use super::{BattleSim, UnitInfo, VOLLEY_FX_SECONDS, VolleyFx};
use crate::data::GameData;
use crate::soldiers::{FigureKit, KitAssets, SoldierLibrary, animation_keys};
use ntw_formats::unit_animation::{self, DeathCause, Gait, Rung, SelectionRng, alternative, family_pick};

use super::actions::{self, Mode};

/// Different men per unit (PROVISIONAL: the game varies parts per man; we build this many
/// kits per unit and share them round-robin; the men of a kit share its meshes and are drawn
/// instanced). Clips do not depend on the kit: each man plays his own alternative clips,
/// picked by his selection number, with his own phase (see `skin`).
const KITS_PER_UNIT: usize = 3;
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
        self.at_slot(units).or_else(|| units.binary_search_by_key(&self.id, |u| u.id).ok().map(|i| &units[i]))
    }

    /// [`unit`](Self::unit), keeping the slot it was found at (the same as `observe_ticks` does).
    #[cfg(test)]
    fn observe<'a>(&mut self, units: &'a [LandUnit]) -> Option<&'a LandUnit> {
        self.at_slot(units).or_else(|| self.relocate(units))
    }

    /// Finds the unit by id and keeps its slot: for when [`at_slot`](Self::at_slot) already said
    /// the kept slot no longer holds it.
    fn relocate<'a>(&mut self, units: &'a [LandUnit]) -> Option<&'a LandUnit> {
        self.slot = units.binary_search_by_key(&self.id, |u| u.id).ok()?;
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
/// its drawing moves them between ticks. A lead, not traced to the end: each soldier display reads
/// its entity through the two latest tick snapshots lerped by the elapsed fraction of the tick
/// (`0x00752A20`, `0x00752FF0`; UNITS_TERRAIN_FIDELITY.md §1.10). PROVISIONAL (ours): the
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
    /// Each figure's place in the unit's frame and his own speed ([`MenDrawn`]).
    march: MenDrawn,
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
/// The rider and his mount are two displays in the exe, each blending its own clip changes over
/// its own line's blend time (the mount's line is the mount fragment's), and so here: each part
/// has its own [`PartFade`], so a mount clip changing while the man's stays the same cross-fades
/// too. A rider plays the clip paired with his mount's (the loop choice in the figure update), so
/// the mount's clip changes alone only where two levels share the man's clip. The pair shares
/// one clip time (ours: the paired clips have the same frame count, `unit_animation`), so they
/// stay in step.
///
/// PROVISIONAL (ours): the GPU keeps at most [`skin::FROZEN`] poses frozen at clip changes per
/// part, with their weights (newest first, [`skin::Fade`]), rather than the recursive blended
/// pose, so the new clip's earlier frames do not linger ([`freeze`]: a change never waits; past
/// that many poses within one blend time the lightest one's weight goes to its neighbour in
/// time). A one-shot drawn over the loop is not blended and ends a running cross-fade (one-shots
/// are not blended yet).
///
/// Each display plays its locomotion loops at its own playback scale ([`playback_scale`],
/// CONFIRMED `0x0078B8A0`), so figures on the same loop drift out of step as the exe's do.
///
/// The exe also lerps each display's root (position, heading) at e / Dr (Dr = 0.5 s,
/// `0x01318038`). Ours has no separate display root to lerp: a figure stands at its formation
/// place, which moves with the unit's drawn pose ([`DrawnPose`]), and loop clips have their root
/// motion removed, so a clip change moves no root; the root bone's own offset is part of the pose
/// and fades with it over D (equal to Dr for the 0.5 s vanilla gait lines).
#[derive(Debug, Clone, Copy)]
struct ClipBlend {
    /// The display's locomotion playback scale ([`playback_scale`]; 1 in tests).
    scale: f32,
    /// The loop clip played (its `Arc` address; 0 before the first frame) and its man's atlas slot.
    clip: usize,
    man: ClipSlot,
    /// The mount's clip (its `Arc` address; 0 on foot) and atlas slot.
    mount_clip: usize,
    mount: Option<ClipSlot>,
    /// Its root speed (m/s: the shown level's [`carry_speed`], or a levelless bearer's
    /// `GaitAnims::speed`, [`bearer_loop`]) and the figure's time in it (s).
    speed: f32,
    time: f32,
    /// The man's (0) and the mount's (1) cross-fade.
    fades: [PartFade; 2],
    /// The figure's place in its locomotion graph: a mounted figure's ladder ([`on_ladder`]), a
    /// man's speed level ([`on_levels`]); and the graph (gait) it is in, on foot.
    rung: Rung,
    graph: Option<LocoGraph>,
    /// The man's and mount's frames drawn at the frame numbered `drawn` (`SkinState::frame`; 0 =
    /// never drawn).
    shown: [[u32; 4]; 2],
    drawn: u32,
}

impl Default for ClipBlend {
    fn default() -> Self {
        ClipBlend::with_scale(1.0)
    }
}

impl ClipBlend {
    /// A figure that has drawn nothing yet, playing its locomotion loops at `scale`.
    fn with_scale(scale: f32) -> Self {
        ClipBlend {
            scale,
            clip: 0,
            man: ClipSlot::default(),
            mount_clip: 0,
            mount: None,
            speed: 0.0,
            time: 0.0,
            fades: Default::default(),
            rung: Rung::default(),
            graph: None,
            shown: [[0; 4]; 2],
            drawn: 0,
        }
    }
}

/// A display's locomotion playback scale, drawn when the display is made: the MSVC `rand` step
/// `seed x 0x343FD + 0x269EC3` on the display manager's seed, then 0.95 + its high 16 bits x
/// 1.5259022e-6, so in [0.95, 1.05]. CONFIRMED: `0x0078B8A0`, called by the soldier display
/// constructor `0x007179B0` (its second draw on the seed at manager `+0x8`; the first, high 16 bits
/// / 65535, goes to the display input's `+0xD0`) and stored at display `+0xB0`, which scales the
/// clip advance of locomotion loops only (slot kind 2, `0x007725D0`; not transitions or stands).
/// The rider copies his mount display's clip time, so the mount's scale drives the pair. Where
/// the manager's seed comes from is UNKNOWN (ours: [`display_seed`]).
fn playback_scale(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
    (*seed >> 16) as f32 * 1.525_902_2e-6 + 0.95
}

/// The display constructor's draws for one figure (`0x007179B0`): the input timer's draw (ours
/// has no use for it, but it keeps the sequence), then the playback scale ([`playback_scale`]).
fn display_scale(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
    playback_scale(seed)
}

/// How the display update advances the loop clip a figure shows, by the clip's slot kind
/// (`ntw_formats::battle_animation::SLOT_KINDS`, the clip object's `+0x1C`; the switch in
/// `0x007725D0`, CONFIRMED, UNITS_TERRAIN_FIDELITY.md §1.9 "Clip advance by slot kind"). The
/// transitions (kind 3) have their own rate ([`unit_animation::transition_rate`], [`on_ladder`]).
///
/// PROVISIONAL: covers the clip advance only. The display update's clip-time rule
/// (`ntw_formats::battle_animation::display_time`, `0x00774228`..`0x00774398`: pinned ends, the
/// rider's mount time, the clock) is not called yet, because the figure carries no entity anim
/// state (`+0x1B8`) to pass it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LoopKind {
    /// Kind 2, a moving gait loop (`WALK_n`, `RUN_n`, `CHARGE`, the horse gaits): [`loop_rate`]
    /// times the display's [`playback_scale`] (`0x00772E19`..`0x00772E65`).
    Gait,
    /// Kinds 4, 0xC and 0xD: a speed over the clip's root speed (anim `+0x60`), unscaled, with
    /// different speeds (CONFIRMED, `0x007725D0`). 0xC / 0xD, the turns into a walk, reload the
    /// forward speed kind 2 divides (display input `+0x0`, entity `+0x14C`; `0x007730BC`, then
    /// `0x00772E6F`). Kind 4, the backward engine moves (`PUSH_ENGINE_*_BACKWARDS`,
    /// `ENGINE_LIMBERED_BACKWARDS`), divides display input `+0x8` (loaded at `0x00772C71`, divided
    /// at `0x00772E73`): the snapshot `0x0079FF50` stores there the horizontal length of the
    /// entity's vector `+0x8C`/`+0x94`, whose angle it keeps as a heading, so a speed whatever
    /// the facing, where the forward speed would be negative. That vector is the measured
    /// velocity (CONFIRMED): the motion integrator `0x007F08F0` stores the tick's displacement x 10
    /// there (`0x007F12AD`..`0x007F12F2`), the placing move `0x0080BAC0` (new - old position) x 10.
    /// So kind 4's input is the distance moved over the last 0.1 s tick, per second: the ground
    /// speed ([`GroundSpeed`], never negative) ours feeds both kinds, as [`loop_rate`] of it.
    Paced,
    /// Rate 1. Kinds 0 / 1 / 0xE / 0x20, the stands and action loops (`STAND`, `COMBAT_READY`,
    /// `AIM`), add the frame time (`0x00773086`). Kinds 0xF..0x13, the melee and one-shot actions
    /// (`COMBAT_n`, `COMBAT_IDLE_n`), are not advanced by the switch (`0x00773080`): their time is
    /// set every frame from the soldier's own clip clock (`0x0077430C`), its frame counter
    /// (entity `+0x1C0`, 10 frames/s, stepped by the 0.1 s tick, `0x0066CE00`; passed through the
    /// snapshot `0x0079FF50` and the display input `0x00752FF0`) x 0.1 plus the time into the
    /// current tick (`+0x28230` ms x 0.001, `0x005F5530`), 0.1 s more for kind 0xF: so it runs
    /// at rate 1. The rider's own kinds 0x16..0x1A (`RIDER_COMBAT_n` is 0x17) take his mount
    /// display's clip time and its `+0x115` flag (`0x00774282`, CONFIRMED): ours keeps one clip
    /// time for the pair and keys a mounted level on the mount's slot (`soldiers::level_kind`),
    /// so a rider's kind never reaches this. No loop a kit shows has another kind.
    Plain,
}

impl LoopKind {
    /// The advance of a clip of slot kind `kind`.
    fn of(kind: u8) -> Self {
        match kind {
            2 => LoopKind::Gait,
            4 | 0xC | 0xD => LoopKind::Paced,
            _ => LoopKind::Plain,
        }
    }
}

/// Whether a clip of slot kind `kind` plays the soldier's own clip clock ([`LoopKind::Plain`]):
/// its start is the sim's, not the display's.
fn sim_clocked(kind: u8) -> bool {
    (0xF..=0x13).contains(&kind)
}

/// The clip time a figure adds in `dt` s at `speed` m/s showing a clip of slot kind `kind` whose
/// root speed is `clip_speed` ([`LoopKind::of`]): the one place the kind rule is applied.
fn advance_by(blend: &ClipBlend, kind: u8, speed: f32, clip_speed: f32, dt: f32) -> f32 {
    match LoopKind::of(kind) {
        LoopKind::Gait => dt * loop_rate(speed, clip_speed) * blend.scale,
        LoopKind::Paced => dt * loop_rate(speed, clip_speed),
        LoopKind::Plain => dt,
    }
}

/// The clip time a figure with selection number `sel` adds in `dt` s at `speed` m/s showing the
/// loop of `level` ([`advance_by`], by the level's kind and its own alternative's root speed).
fn loop_advance(blend: &ClipBlend, level: &crate::soldiers::KitLevel, speed: f32, sel: u32, dt: f32) -> f32 {
    advance_by(blend, level.kind, speed, level.speed_of(sel), dt)
}

/// The display manager's seed: UNKNOWN in the exe (the manager's `+0x8` is set outside the
/// traced code); ours is a stream of the battle's own seed, apart from the selection numbers.
fn display_seed(battle_seed: u32) -> u32 {
    battle_seed ^ 0x5EED_D15B
}

/// One display's cross-fade out of the poses shown at its last clip changes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PartFade {
    /// Up to [`skin::FROZEN`] poses frozen at clip changes ([`freeze`]), newest first (weight 0 =
    /// unused, and so are the ones after it); the clip played has the rest of the weight.
    frozen: [Frozen; skin::FROZEN],
    /// Blend time D (s) of the last change, and the time since it e (s).
    blend: f32,
    elapsed: f32,
}

impl PartFade {
    /// A clip change: fade out of `fig` (the frame drawn last frame, if the figure was drawn then
    /// and no one-shot is drawn) over the new line's `blend_in` s, or draw the new clip at once.
    fn change(&mut self, fig: Option<[u32; 4]>, blend_in: f32) {
        match fig.filter(|_| blend_in > 0.0) {
            Some(fig) => *self = PartFade { frozen: freeze(self.frozen, fig), blend: blend_in, elapsed: 0.0 },
            None => *self = PartFade::default(),
        }
    }

    /// One frame: every frozen pose keeps (1 - e / D) of its weight, then e grows by `dt`.
    fn advance(&mut self, dt: f32) {
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
    }

    fn fading(&self) -> bool {
        self.frozen.iter().any(|f| f.keep > 0.0)
    }
}

/// A pose frozen at a clip change: the display's clip frame, and its weight.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Frozen {
    fig: [u32; 4],
    keep: f32,
}

/// The loop clip a figure is to play this frame: its man's and mount's clips (`Arc` addresses,
/// the mount's 0 on foot), their atlas slots, and their lines' `blend_in_time` (s).
#[derive(Debug, Clone, Copy)]
struct LoopClip {
    key: usize,
    man: ClipSlot,
    blend_in: f32,
    mount_key: usize,
    mount: Option<ClipSlot>,
    mount_blend_in: f32,
    /// When the clips change: start them at this time (s), cross-fading (`true`) or drawn at
    /// once (`false`), instead of the phase carry and the lines' blends (a gait ladder's step,
    /// [`on_ladder`]).
    start: Option<(f32, bool)>,
    /// The slot kind the clips play by (`KitLevel::kind`): whether a change into them carries
    /// the phase ([`start_time`]).
    kind: u8,
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
            self.fades = Default::default();
        }
        // A ladder step drawn at once starts its clips without a cross-fade.
        let from = shown.filter(|_| !shot && want.start.is_none_or(|(_, fade)| fade));
        let man_changed = want.key != self.clip;
        if man_changed {
            let duration = want.man.duration();
            self.time = match want.start {
                Some((t, _)) => t,
                None => start_time(self.clip != 0, want.kind, || carry_phase(self.time, self.man.duration(), self.speed, duration), offset * duration),
            };
            self.clip = want.key;
            self.man = want.man;
            self.fades[0].change(from.map(|f| f[0]), want.blend_in);
        }
        // The mount clip is checked on its own every frame: it may change while the man's stays
        // (two levels sharing the man's clip).
        let mount_changed = want.mount_key != self.mount_clip;
        if mount_changed {
            self.mount_clip = want.mount_key;
            self.mount = want.mount;
            self.fades[1].change(from.filter(|_| want.mount.is_some()).map(|f| f[1]), want.mount_blend_in);
        }
        // The root speed the next change carries by is the caller's: the shown level's
        // ([`carry_speed`]), or a levelless bearer's `GaitAnims::speed` ([`bearer_loop`]). A mount
        // change alone (another level sharing the man's clip) changes it too.
        if man_changed || mount_changed {
            self.speed = speed();
        }
        for f in &mut self.fades {
            f.advance(dt);
        }
        self.time += advance;
    }

    /// The frames the figure drew last frame (frame `frame - 1`), if it was drawn then: a figure
    /// skipped since (its whole unit, or itself) has no pose to fade from.
    fn shown(&self, frame: u32) -> Option<[[u32; 4]; 2]> {
        (self.drawn != 0 && self.drawn.wrapping_add(1) == frame).then_some(self.shown)
    }

    /// Whether a cross-fade runs (the man's or the mount's).
    #[cfg(test)]
    fn fading(&self) -> bool {
        self.fades.iter().any(PartFade::fading)
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
        let f = &self.fades[part];
        f.fading().then(|| skin::fade(f.frozen.map(|p| p.fig), f.frozen.map(|p| p.keep)))
    }
}

/// A display's frozen poses after a clip change: the frame drawn last (`fig`) at the weight the
/// old clip had (1 - the frozen weights), then the poses frozen before, newest first, without the
/// ones of weight 0 (a change on the frame after a change: the old clip had none yet). The weights
/// then add up to 1, the new clip starts at 0, and every pose is gone one blend time after the
/// last change. PROVISIONAL (ours, the exe blends recursively and keeps no list): past
/// [`skin::FROZEN`] poses the lightest (the older of equals) is dropped and its weight goes to its
/// neighbour in time (the next newer pose, or the next older when it is the newest), the pose most
/// like it: two levels of one gait at the same phase. Without that a run -> walk, which changes
/// clip about once a tick, dropped a quarter of the weight at once (the hiccup of the user's
/// 2026-10-10 check, with two poses).
fn freeze(old: [Frozen; skin::FROZEN], fig: [u32; 4]) -> [Frozen; skin::FROZEN] {
    let live = Frozen { fig, keep: (1.0 - old.iter().map(|p| p.keep).sum::<f32>()).max(0.0) };
    let mut poses = [Frozen::default(); skin::FROZEN + 1];
    let mut n = 0;
    for p in std::iter::once(live).chain(old).filter(|p| p.keep > 0.0) {
        poses[n] = p;
        n += 1;
    }
    if n > skin::FROZEN {
        let lightest = (0..n).rev().min_by(|&a, &b| poses[a].keep.total_cmp(&poses[b].keep)).unwrap_or(n - 1);
        let to = if lightest > 0 { lightest - 1 } else { 1 };
        poses[to].keep += poses[lightest].keep;
        poses.copy_within(lightest + 1..n, lightest);
    }
    std::array::from_fn(|k| poses[k])
}

/// A ladder level to play, the clip time to add, and how its clips start ([`LoopClip::start`]).
type LadderPick<'a> = (&'a crate::soldiers::KitLevel, f32, Option<(f32, bool)>);

/// The level a mounted figure plays on its gait ladder (`kit.ladder`) this frame, the clip time
/// to add, and how its clips start when this frame steps onto a new rung; `Some(None)` on the
/// stand rung, `None` without a ladder (on foot). `speed` is the unit's ground speed (0 when it
/// stands), `duration` a clip's length as drawn, `action` the Ready or Melee action loop
/// drawn this frame instead of the ladder's clip, if any.
///
/// CONFIRMED (`0x007725D0`, the locomotion graph and the debugger sitting of 2026-10-09,
/// UNITS_TERRAIN_FIDELITY.md §1.9): the rung follows `unit_animation::climb`. A step up from a loop
/// waits for the loop to finish its cycle: the transition clip starts on the frame the loop's
/// time wraps (its time mod its length after this frame's advance is at most the one before it,
/// `[ESP + 0x118]`), at the loop's phase past the wrap, drawn at once (the store at `0x00773D5C`,
/// blend time 0); out of the stand (the from-slot is the graph's node-0 slot) it starts at once at 0
/// and cross-fades over its line's blend time (`0x00773D18`). The transition plays at
/// `unit_animation::transition_rate`; on the frame its time after the advance reaches its end
/// (`0x0077404C`, the anim's length `+0x4C`) its loop follows at the time past the end, drawn at
/// once in that frame (`0x0077408E`..`0x007740A9`), so no frame past the end is drawn. A loop
/// plays by its slot kind ([`loop_advance`]: kind 2 for the horse gaits), an action loop shown over the ladder
/// (`action`) by its own; a transition at its own rate, without the playback scale. `sel` is the
/// figure's selection number (which alternative clips it plays, whose own root speeds the edges and
/// rates use, [`KitLevel::speed_of`](crate::soldiers::KitLevel::speed_of)). Steps down change loop with the
/// usual phase carry and cross-fade.
///
/// CONFIRMED (`0x00773C83`): the wait applies only while the clip shown is the from-loop's (the
/// transition's from-slot `+0x30` against the shown node's slot `+0x4`); with another clip shown
/// (the frame an action loop ends) the transition starts at once as a usual clip change (at 0:
/// a transition does not loop, [`start_time`]) and cross-fade (`0x00773D85`). The display keeps one clip and one clip time, so no graph
/// clock runs on by itself. PROVISIONAL (ours, not traced: in the exe the graph's clip is the one
/// shown, so a gait transition under an action loop does not arise there): while an action loop
/// is drawn the ladder skips its transitions, stepping straight between loops.
#[allow(clippy::too_many_arguments)]
fn on_ladder<'a>(ladder: &'a [crate::soldiers::LadderRung], blend: &mut ClipBlend, speed: f32, dt: f32, action: Option<&crate::soldiers::KitLevel>, sel: u32, duration: impl Fn(&std::sync::Arc<Anim>) -> f32) -> Option<Option<LadderPick<'a>>> {
    if ladder.is_empty() {
        return None;
    }
    // The length of the transition clip the figure plays: its own alternative (`sel`).
    let into_len = |i: usize| ladder[i].into.as_ref().and_then(|l| l.man.get(alternative(sel, l.man.len()))).map_or(0.0, &duration);
    // A transition's playback rate `played` s into it (`len` s long).
    let into_rate = |i: usize, len: f32, played: f32| {
        let from = if i == 0 { 0.0 } else { ladder[i - 1].gait.speed_of(sel) };
        unit_animation::transition_rate(speed, from, ladder[i].gait.speed_of(sel), if len > 0.0 { played / len } else { 1.0 })
    };
    let acting = action.is_some();
    let old = blend.rung;
    // The length of the transition the figure was in, looked up once a frame.
    let old_len = match old {
        Rung::Into(i) => into_len(i),
        _ => 0.0,
    };
    // A transition's time after this frame's advance: the exe advances the clip time first and
    // leaves the transition on the frame that time reaches its length (`0x0077404C`), so its
    // frames past the end are never drawn.
    let into_end = match old {
        Rung::Into(i) => blend.time + dt * into_rate(i, old_len, blend.time),
        _ => 0.0,
    };
    let done = matches!(old, Rung::Into(_) if acting || into_end >= old_len);
    let mut rung = unit_animation::climb(old, ladder.len(), |i| ladder[i].gait.speed_of(sel), |i| !acting && ladder[i].into.is_some(), speed, done);
    // A step up from a loop waits for the loop's cycle to wrap this frame, while the from-loop is
    // shown; until then the figure goes on with its loop (the graph node moves on, the clip does
    // not). A step up into a transition is never taken under an action loop, so the loop shown is the ladder's.
    let mut wrap_phase = None;
    let mut from_shown = true;
    if let (Rung::Loop(j), Rung::Into(_)) = (old, rung) {
        from_shown = ladder[j].gait.man.iter().any(|a| std::sync::Arc::as_ptr(a) as usize == blend.clip);
        let cycle = blend.man.duration();
        let next = blend.time + loop_advance(blend, &ladder[j].gait, speed, sel, dt);
        if from_shown && cycle > 0.0 && next.rem_euclid(cycle) > blend.time.rem_euclid(cycle) {
            rung = old;
        } else if from_shown {
            wrap_phase = Some(if cycle > 0.0 { next.rem_euclid(cycle) } else { 0.0 });
        }
    }
    blend.rung = rung;
    Some(match rung {
        Rung::Stand => None,
        Rung::Loop(i) => {
            let level = &ladder[i].gait;
            // Out of its transition: the loop at the transition's time past its end after this
            // frame's advance (`0x0077408E` takes the length off), which is the loop's time this
            // frame, so nothing more is added.
            if old == Rung::Into(i) && !acting {
                Some((level, 0.0, Some(((into_end - old_len).max(0.0), false))))
            } else {
                Some((level, loop_advance(blend, action.unwrap_or(level), speed, sel, dt), None))
            }
        }
        Rung::Into(i) => {
            let into = ladder[i].into.as_ref()?;
            let len = if old == Rung::Into(i) { old_len } else { into_len(i) };
            let rate = |played: f32| into_rate(i, len, played);
            match (old, wrap_phase) {
                (Rung::Into(j), _) if j == i => Some((into, dt * rate(blend.time), None)),
                // At the wrap: the transition at the loop's phase past it, which is its time after
                // this frame (the exe switches after the frame's advance), so nothing is added.
                (Rung::Loop(_), Some(phase)) => Some((into, 0.0, Some((phase, false)))),
                // Its from-loop not shown: a usual clip change.
                (Rung::Loop(_), None) if !from_shown => Some((into, dt * rate(0.0), None)),
                _ => Some((into, dt * rate(0.0), Some((0.0, true)))),
            }
        }
    })
}

/// The locomotion graph a man on foot is in. CONFIRMED: the soldier speed update (`0x006543D0`) sets
/// the entity state `+0x1B8` from its stance `+0x1D8` and its move kind `+0x220` (0 walk, 1 / 3 run,
/// 2 charge): 8 walk, 0xB run, 0xD charge (stances 0..3), 9 walk / 0xC run (the trained stance 4); the
/// display picks its graph by that state (`0x007A7190`): the `WALK(_TRAINED)_n` graph, the
/// `RUN(_TRAINED)_n` graph, or the charge graph whose one loop is `CHARGE` (slot table `0x0133D030`).
/// The move kind comes from the order's run option (the unit move `0x0051A9A0`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum LocoGraph {
    Gait(Gait),
    Charge,
}

impl From<Gait> for LocoGraph {
    fn from(g: Gait) -> Self {
        LocoGraph::Gait(g)
    }
}

/// The speed level a man on foot plays this frame in the locomotion graph of `gait` (`levels`,
/// slowest first), and the clip time to add; None without levels.
///
/// CONFIRMED (`0x007725D0`, `0x007A7190`; UNITS_TERRAIN_FIDELITY.md §1.9): a man's graph is a
/// ladder of speed levels with no transition clips (the men's slot tables `0x0133C608..`),
/// entered on its fastest level no faster than the man when the graph changes
/// (`unit_animation::enter`) and climbed by `unit_animation::climb` (up at midpoints, down at
/// quarter points). The stand rung plays `stand`, the kit's stand level (the first of `levels`
/// when None): node 0 maps to `STAND` (slot 1) in the walk, run and charge slot tables
/// (`0x0133C608`, `0x0133D030`), slot kind 0. The clip shown (the level's, or the action loop
/// `action` shown over the gait) advances by its own slot kind ([`loop_advance`]): the walk, run
/// and charge levels are kind 2, the stands, `COMBAT_READY` and `AIM` kind 0, the melee idles
/// kind 0x11.
/// The edges, the entry and the rate use the root speeds of the figure's own alternatives
/// (selection number `sel`, [`KitLevel::speed_of`](crate::soldiers::KitLevel::speed_of)).
#[allow(clippy::too_many_arguments)]
fn on_levels<'a>(levels: &'a [crate::soldiers::KitLevel], stand: Option<&'a crate::soldiers::KitLevel>, blend: &mut ClipBlend, graph: impl Into<LocoGraph>, speed: f32, dt: f32, action: Option<&crate::soldiers::KitLevel>, sel: u32) -> Option<(&'a crate::soldiers::KitLevel, f32)> {
    let n = levels.len();
    if n == 0 {
        return None;
    }
    let loops = |i: usize| levels[i].speed_of(sel);
    let graph = graph.into();
    if blend.graph != Some(graph) {
        blend.graph = Some(graph);
        blend.rung = unit_animation::enter(n, loops, speed);
    }
    blend.rung = unit_animation::climb(blend.rung, n, loops, |_| false, speed, false);
    let level = match blend.rung {
        Rung::Loop(i) | Rung::Into(i) => &levels[i.min(n - 1)],
        Rung::Stand => stand.unwrap_or(&levels[0]),
    };
    Some((level, loop_advance(blend, action.unwrap_or(level), speed, sel, dt)))
}

/// A moving gait loop's playback rate: the figure's speed over the clip's root speed, unclamped
/// (CONFIRMED `0x00772E22`..`0x00772E2B`: speed / anim `+0x60`, no limit, so a figure at 0 m/s holds
/// its pose); [`loop_advance`] multiplies in the display's playback scale. A clip without root
/// speed plays at 1: the exe would divide by 0, but no moving gait loop of the shipped data is
/// still (ours guards it for mod data).
fn loop_rate(speed: f32, clip_speed: f32) -> f32 {
    if clip_speed > 1e-3 { speed / clip_speed } else { 1.0 }
}

/// The loop clips a figure plays from `level`: its alternative `sel`'s man clip and, when mounted,
/// the paired mount clip (same frame count, so the same frame; each its own blend), starting at
/// `start` ([`LoopClip::start`]); None when the man's clip has no atlas slot (`slot`).
fn level_loop(level: &crate::soldiers::KitLevel, sel: u32, start: Option<(f32, bool)>, slot: impl Fn(&std::sync::Arc<Anim>) -> Option<ClipSlot>) -> Option<LoopClip> {
    let man = &level.man[alternative(sel, level.man.len())];
    let mount = (!level.mount.is_empty()).then(|| &level.mount[alternative(sel, level.mount.len())]);
    Some(LoopClip {
        key: std::sync::Arc::as_ptr(man) as usize,
        man: slot(man)?,
        blend_in: level.blend_in_of(sel),
        mount_key: mount.map_or(0, |m| std::sync::Arc::as_ptr(m) as usize),
        mount: mount.and_then(&slot),
        mount_blend_in: level.mount_blend_in_of(sel),
        start,
        kind: level.kind,
    })
}

/// The standard bearer's loop this frame ([`bearer_loop`]): the clip to step to, the clip time
/// to add, and the root speed his next clip change carries the phase by ([`carry_phase`]).
struct BearerLoop {
    want: LoopClip,
    advance: f32,
    carry: f32,
}

/// The standard bearer's loop for `gait` at `speed` m/s: the same choice as a man on foot
/// ([`on_levels`], [`level_loop`], carrying by [`carry_speed`]) when his kit has speed `levels`
/// (the stand rung playing `stand`, the kit's stand level); without them, the kit's per-gait clip
/// (`anims`), played by its slot kind at its read root speed ([`advance_by`]) and carrying by it
/// ([`GaitAnims::speed`](crate::soldiers::GaitAnims::speed): the clip drawn loops in place, so
/// its own is 0 and would restart his stride at every gait change). That clip's line is not kept,
/// so a change to it is drawn at once. None without a clip or atlas slot (`slot`).
#[allow(clippy::too_many_arguments)]
fn bearer_loop<'a>(levels: Option<&'a [crate::soldiers::KitLevel]>, stand: Option<&'a crate::soldiers::KitLevel>, anims: impl FnOnce() -> Option<&'a crate::soldiers::GaitAnims>, gait: Gait, blend: &mut ClipBlend, speed: f32, dt: f32, sel: u32, slot: impl Fn(&std::sync::Arc<Anim>) -> Option<ClipSlot>) -> Option<BearerLoop> {
    if let Some((level, advance)) = levels.and_then(|l| on_levels(l, stand, blend, gait, speed, dt, None, sel)) {
        return Some(BearerLoop { want: level_loop(level, sel, None, slot)?, advance, carry: carry_speed(level, sel) });
    }
    let a = anims()?;
    let want = LoopClip { key: std::sync::Arc::as_ptr(&a.man) as usize, man: slot(&a.man)?, blend_in: 0.0, mount_key: 0, mount: None, mount_blend_in: 0.0, start: None, kind: a.kind };
    Some(BearerLoop { want, advance: advance_by(blend, a.kind, speed, a.speed, dt), carry: a.speed })
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
/// else at 0 (`None`, [`start_time`]); only into a new clip that loops ([`start_time`]). Each
/// figure keeps its own time, so a change carries that man's own phase and leaves the figures
/// whose clip did not change untouched.
fn carry_phase(time: f32, old_duration: f32, old_speed: f32, duration: f32) -> Option<f32> {
    (old_speed > MOVING_CLIP_SPEED && old_duration > 0.0).then(|| time.rem_euclid(old_duration) / old_duration * duration)
}

/// The time (s) a figure starts a new clip of slot kind `kind` at on a usual clip change (not a
/// ladder step, [`LoopClip::start`]); `changed` whether it showed a clip before, `carry` the
/// carried phase ([`carry_phase`]), `own` its own phase offset in the new clip (`skin::phase`).
///
/// CONFIRMED (`0x00773E73`): the phase carries only into an anim with the loop flag (`+0x44`
/// mask 0x2, set by kind, `battle_animation::kind_loops`); any other new clip (a stand, a
/// transition) starts at 0 (`0x00773F17`). PROVISIONAL (ours, not traced): `own` where the
/// exe's start is set elsewhere or not traced: a looping clip the phase does not carry into (the
/// exe starts it at 0; ours desyncs the men's strides), a sim-clocked clip ([`sim_clocked`]:
/// the exe plays the soldier's clip clock, whose start ours does not keep) and a figure's first
/// clip (the display's start time is not traced).
fn start_time(changed: bool, kind: u8, carry: impl FnOnce() -> Option<f32>, own: f32) -> f32 {
    if ntw_formats::battle_animation::kind_loops(kind) {
        carry().unwrap_or(own)
    } else if changed && !sim_clocked(kind) {
        0.0
    } else {
        own
    }
}

/// Clip root speed (m/s) above which the clip left counts as moving for [`carry_phase`]
/// (`0x0131A7B0`, CONFIRMED).
const MOVING_CLIP_SPEED: f32 = 0.1;

/// The root speed (m/s) [`carry_phase`] tests for a level's loop clips (the exe's anim `+0x60`,
/// `Anim::root_speed`) when the figure with selection number `sel` leaves them: the root speed of
/// its own alternative ([`KitLevel::speed_of`](crate::soldiers::KitLevel::speed_of)), measured
/// when the clip was read, not the speed of the clip drawn. The clips drawn have their drift
/// removed (they loop in place, `soldiers::in_place`), so their own root speed is 0 and would
/// never carry the phase (the gait-jitter probe of 2026-10-11, UNITS_TERRAIN_FIDELITY.md §1.9).
/// CONFIRMED (`0x00773E2E`, `0x00773E85`..`0x00773E94`): the exe tests the anim `+0x60` of the
/// clip its display showed, the slot's alternative by the display's selection (`0x00E5F760`); a
/// rider copies his mount display's clip time (`0x0079BB10`), so a mounted pair carries by the
/// mount's alternative. The same holds for a transition clip left before its end (a step down
/// from the transition node, `0x007CE840`, or an action loop shown over it): its change is the
/// usual one (`0x00773D85`), carried by the transition clip's own root speed. The standard
/// bearer's per-gait clip carries by its own read speed
/// ([`GaitAnims::speed`](crate::soldiers::GaitAnims::speed), [`bearer_loop`]).
fn carry_speed(level: &crate::soldiers::KitLevel, sel: u32) -> f32 {
    level.speed_of(sel)
}

/// A unit's ground speed, which picks each figure's speed level in its locomotion graph ([`on_levels`], [`on_ladder`]).
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
///
/// The figures see the speed lerped from the one before the last tick to the last tick's by the
/// fraction of the next tick already elapsed ([`GroundSpeed::at`]), as the exe's display reads
/// its entity's speed through two tick snapshots lerped by the tick fraction (`0x00752FF0`,
/// UNITS_TERRAIN_FIDELITY.md §1.10), so a slowing unit's playback rate and speed level follow its
/// speed smoothly, not in a step every tick. Like [`DrawnPose`], it runs one tick behind.
#[derive(Debug, Clone, Copy)]
struct GroundSpeed {
    /// The model position and tick last seen.
    pos: Vec2,
    tick: u32,
    /// Whether the unit was on open ground at the last observation ([`on_ground`]).
    on_ground: bool,
    /// m/s over the last model tick in which the unit moved, and the speed before that tick (0
    /// when the unit stood).
    speed: f32,
    before: f32,
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
        GroundSpeed { pos, tick, on_ground, speed: 0.0, before: 0.0, still_ticks: STILL_TICKS }
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
        self.before = self.at(1.0);
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

    /// The speed the figures see, `alpha` (0..1) of the way through the next tick: lerped from
    /// the speed before the last tick to the last tick's; 0 once the unit stands.
    fn at(&self, alpha: f32) -> f32 {
        if self.still_ticks >= STILL_TICKS {
            0.0
        } else {
            self.before + (self.speed - self.before) * alpha.clamp(0.0, 1.0)
        }
    }

    /// The gait: stand once still for [`STILL_TICKS`]; else run when the unit's move runs (`runs`:
    /// its run option, or a rout), else walk. A man on foot plays the gait's locomotion graph
    /// ([`on_levels`]) chosen by the entity state, which follows the move's kind (CONFIRMED,
    /// [`LocoGraph`]: walk, run, or the charge graph for a charge). Mounted figures climb their
    /// ladder whatever the gait ([`on_ladder`]: the animal display has one graph, `0x007A72F0`).
    fn gait(&self, runs: bool) -> Gait {
        if self.still_ticks >= STILL_TICKS {
            Gait::Stand
        } else if runs {
            Gait::Run
        } else {
            Gait::Walk
        }
    }
}

/// Each figure's own march as drawn: where he stands in the unit's frame and his own speed, which
/// the model's men keep (`ntw_sim::battle::march`: each man walks his own timed move at his own
/// speed, and a re-lay gives him the nearest slot, UNITS_TERRAIN_FIDELITY.md §1.10 "Each man's
/// timed move"). Observed after every model tick with the unit's [`GroundSpeed`] and drawn lerped
/// by the tick fraction, as [`DrawnPose`]. Figure `j` is the model's man whose `figure` is `j`
/// (the model keeps its men in slot order, so a re-slot moves him to another entry); a figure the
/// model has no entry for (the unit not yet ticked) stands at slot `j`. A figure first seen starts
/// where his own man stands.
///
/// A man's speed is kept as his difference from the unit's drawn speed ([`GroundSpeed::at`]), so
/// the unit's own rules (the placement tick skipped, the still ticks bridged) stay the unit's: in a
/// tick the unit moved, the difference is his model speed less the unit's; in a tick it did not,
/// 0 while the unit's speed is bridged, and his own model speed once the unit stands (the men
/// still walking to their places after the unit stopped).
#[derive(Debug, Clone, Default)]
struct MenDrawn {
    /// Per figure: [right (m), back (m), speed difference (m/s)] before and after the last model
    /// tick, in the unit's frame.
    from: Vec<[f32; 3]>,
    to: Vec<[f32; 3]>,
    /// Per figure: the model's entry he is in (`LandUnit::march`), at the last observation.
    entry: Vec<u32>,
    /// The model tick last seen.
    tick: u32,
}

impl MenDrawn {
    /// After every fixed step, after [`GroundSpeed::observe_unit`] saw the same step. `figures`
    /// is the number of figures drawn.
    fn observe(&mut self, unit: &LandUnit, ground: &GroundSpeed, tick: u32, figures: usize) {
        let men = &unit.march.men;
        // The figures first seen now (a unit spawned late, or more figures drawn).
        let seen = self.to.len().min(figures);
        if self.to.len() != figures {
            self.to.resize(figures, [0.0; 3]);
            self.from.resize(figures, [0.0; 3]);
            self.entry.resize(figures, 0);
        }
        for (j, e) in self.entry.iter_mut().enumerate() {
            *e = j as u32;
        }
        for (k, m) in men.iter().enumerate() {
            if let Some(e) = self.entry.get_mut(m.figure as usize) {
                *e = k as u32;
            }
        }
        let ticked = tick != self.tick;
        self.tick = tick;
        let speed = |m: &ntw_sim::battle::march::ManMarch| {
            if ground.still_ticks == 0 {
                m.speed - ground.speed
            } else if ground.still_ticks < STILL_TICKS {
                0.0
            } else {
                m.speed
            }
        };
        let laid = unit.formation();
        for (j, (from, to)) in self.from.iter_mut().zip(self.to.iter_mut()).enumerate() {
            let k = self.entry[j] as usize;
            let (x, z) = laid.place(k);
            let (dx, dz, v) = men.get(k).map_or((0.0, 0.0, 0.0), |m| (m.side, m.lag, speed(m)));
            // A figure first seen stands where his own man stands, not lerped from elsewhere.
            if j >= seen {
                *to = [x + dx, z + dz, 0.0];
            }
            // The fallen keep where they fell (their entry is no longer laid out).
            let now = if k < unit.men as usize || men.is_empty() { [x + dx, z + dz, v] } else { [to[0], to[1], 0.0] };
            // The model did not tick (paused, deployment): nothing moves, a placement shows at once.
            *from = if ticked { *to } else { [now[0], now[1], 0.0] };
            *to = if ticked { now } else { [now[0], now[1], 0.0] };
        }
    }

    /// Figure `j`'s [right, back, speed difference] `alpha` (0..1) of the way through the next tick.
    fn at(&self, j: usize, alpha: f32) -> Option<[f32; 3]> {
        let (a, b) = (self.from.get(j)?, self.to.get(j)?);
        let t = alpha.clamp(0.0, 1.0);
        Some([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t])
    }

    /// Figure `j`'s speed difference `alpha` of the way through the next tick (0 before he is seen).
    fn speed(&self, j: usize, alpha: f32) -> f32 {
        self.at(j, alpha).map_or(0.0, |a| a[2])
    }

    /// The model's entry figure `j` is in (`j` before he is seen).
    fn entry(&self, j: usize) -> usize {
        self.entry.get(j).map_or(j, |&k| k as usize)
    }

    /// Whether figure `j` moves on his own while his unit stands (his speed before or after the
    /// last tick).
    fn settling(&self, j: usize) -> bool {
        self.from.get(j).is_some_and(|f| f[2] > 0.0) || self.to.get(j).is_some_and(|t| t[2] > 0.0)
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
    if views_current(skin.map(|s| s.build), sim.build, views.iter().map(|(_, v)| v.id), sim.infos().iter().map(|i| i.id)) {
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
    for info in sim.infos() {
        let mut kits = Vec::new();
        let mut bearer = None;
        if let (Some(lib), Some(unit)) = (soldiers.lib.as_mut(), find(&sim, info.id)) {
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
        let ladder = kit.ladder.iter().flat_map(|r| r.into.iter().chain([&r.gait]));
        for level in kit.levels.iter().flat_map(|(_, l)| l).chain(kit.actions.values()).chain(ladder) {
            for clip in level.man.iter().chain(&level.mount) {
                atlas.add(clip);
            }
        }
    }
    let total_men: usize = sim
        .infos()
        .iter()
        .zip(&unit_kits)
        .filter(|(_, k)| !k.is_empty())
        .filter_map(|(i, _)| find(&sim, i.id))
        // Every man is drawn (no per-unit cap: a modded 1,000-man unit shows 1,000 men).
        .map(|u| u.men as usize)
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
    // Each figure's display draws its playback scale in spawn order, the bearer after his men.
    let mut display_rng = display_seed(sim.seed);
    let spawns: Vec<_> = sim
        .infos()
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
                let count = unit.men as usize;
                for i in 0..count {
                    // His place in the block, the one the model walks him from (`march`).
                    let (x, z) = unit.man_place(i);
                    let local = Vec3::new(x, 0.0, z);
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
                    let slot = next_slot as u32;
                    next_slot += 1;
                    // The front rank's line in the men's layout (`LandUnit::man_place`).
                    let local = Vec3::new(0.0, 0.0, unit.man_place(0).1);
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
                march: MenDrawn { tick: sim.battle.tick, ..MenDrawn::default() },
                acts: vec![actions::ManAct::default(); men.len()],
                blends: (0..=men.len()).map(|_| ClipBlend::with_scale(display_scale(&mut display_rng))).collect(),
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
            None => match view.relocate(&sim.battle.units) {
                Some(u) => u,
                None => continue,
            },
        };
        pose.observe(Vec2::new(unit.position.0, unit.position.1), unit.facing, unit.moved && on_ground(unit));
        if let Some(mut figures) = figures {
            let figures = &mut *figures;
            figures.ground.observe_unit(unit, sim.battle.tick);
            figures.march.observe(unit, &figures.ground, sim.battle.tick, figures.men.len());
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
        let runs = unit.running || unit.morale.is_routing_or_shattered();
        let gait = figures.ground.gait(runs);
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
        let length = |a: &std::sync::Arc<ntw_formats::anim::Anim>| atlas.slot(a).map_or(0.0, |s| s.duration());
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
            // The model's men past the living ones fell (`LandUnit::march`): their figures.
            for k in (unit.men as usize)..(figures.last_men as usize) {
                let i = unit.march.men.get(k).map_or(k, |m| m.figure as usize);
                if i >= figures.men.len() || figures.acts[i].dead {
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
        // Figure `i` lives while the model's entry he is in does (`MenDrawn::entry`).
        let men_alive = unit.men as usize;
        if unit.volleys_fired > figures.last_volleys && mode != Mode::Melee {
            for i in (0..figures.men.len()).filter(|&i| figures.march.entry(i) < men_alive) {
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
            for i in 0..figures.men.len() {
                let entry = figures.march.entry(i);
                if entry >= men_alive {
                    continue;
                }
                let sel = figures.selections.get(i).copied().unwrap_or(0);
                figures.acts[i].next_attack = now + (sel % 10) as f32 * 0.2;
                // PROVISIONAL: front-rank men knocked down by a charge, about 3 in 10.
                if charged && unit.man_rank(entry) == 0 && !knockdowns.is_empty() && family_pick(&mut action_rng.0, 10) < 3 {
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
            // He stands where the model's man stands in the unit's frame (`MenDrawn`); his
            // transform is written only when that changed or the unit moved.
            let at = figures.march.at(i, alpha);
            let shifted = at.is_some_and(|a| men_tf.get(*man).is_ok_and(|t| (t.translation.x - a[0]).abs() > 1e-4 || (t.translation.z - a[1]).abs() > 1e-4));
            if (reseat || shifted) && !dead && let Ok(mut man_tf) = men_tf.get_mut(*man) {
                if let Some(a) = at {
                    (man_tf.translation.x, man_tf.translation.z) = (a[0], a[1]);
                }
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
                let want = if (figures.march.entry(i) < men_alive || dead) && !off_field { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
        }
        // Each figure picks its gait clip in its own locomotion graph from his own speed: the unit's
        // ground speed and his difference from it (`on_ladder` / `on_levels`, `MenDrawn`; a placement
        // does not show as a huge speed: `GroundSpeed` skips the tick a unit is placed in).
        let ground_speed = if gait == Gait::Stand { 0.0 } else { figures.ground.at(alpha) };
        // Every man plays his own alternative clips, with his own clip time (GPU skinning): a
        // one-shot if one runs, else the loop of the unit's mode.
        for i in 0..figures.men.len() {
            let shown = figures.blends[i].shown(frame);
            let kit = &figures.kits[i % figures.kits.len()];
            let sel = figures.selections.get(i).copied().unwrap_or(0);
            // His own gait and speed (`MenDrawn`): a man still walking to his place once his unit
            // stands moves in the kind of the move he was issued (`MoveKind`, CONFIRMED: while his
            // locomotive moves `0x006543D0` sets his state from it, whatever the unit's mode).
            let entry = figures.march.entry(i);
            let alive = entry < men_alive;
            // The kind of the move he was last issued (his own: a walk to his slot has its own).
            let kind = unit.march.men.get(entry).map_or(unit.march.kind, |m| m.kind);
            let settling = alive && gait == Gait::Stand && figures.march.settling(i);
            let (gait, ground_speed) = if !alive {
                (gait, ground_speed)
            } else if settling {
                let issued = if kind == MoveKind::Walk { Gait::Walk } else { Gait::Run };
                (issued, figures.march.speed(i, alpha).max(0.0))
            } else if gait == Gait::Stand {
                (gait, 0.0)
            } else {
                (gait, (ground_speed + figures.march.speed(i, alpha)).max(0.0))
            };
            let act = &mut figures.acts[i];
            act.advance(now, length);
            // PROVISIONAL: a man still moving to his place starts no melee attack. Whether a soldier
            // in his timed move starts one is not traced (the re-issue gate's action kinds, table
            // `0x00811890`, show some actions start while moving).
            if mode == Mode::Melee && !settling && !act.dead && alive && act.shot.is_none() && now >= act.next_attack && !figures.melee_attack.is_empty() {
                let k = family_pick(&mut action_rng.0, figures.melee_attack.len() as u32) as usize;
                if let Some(clips) = actions::pick(kit, &figures.melee_attack[k], sel) {
                    act.next_attack = now + length(&clips.0) + actions::swing_gap(sel);
                    act.play(now, vec![clips], false);
                }
            }
            let loop_slot = match mode {
                // A moving man plays his move's locomotion, not the mode's loop (CONFIRMED:
                // `0x006543D0` overwrites his state `+0x1B8` while his locomotive moves, and the
                // display picks his graph by that state, `0x007A7190`).
                _ if settling => None,
                Mode::Ready { aiming } if aiming && kit.actions.contains_key(unit_animation::AIM) => Some(unit_animation::AIM),
                Mode::Ready { .. } => Some(unit_animation::COMBAT_READY),
                Mode::Melee => figures.melee_idle.get(alternative(sel, figures.melee_idle.len().max(1))).map(String::as_str),
                Mode::Gait(_) => None,
            };
            // The action loop's man clip and line; its mount clip, else the gait level's. A rider
            // always plays the clip paired with his mount's (CONFIRMED, `0x007725D0`: a rider display
            // with a mount takes the slot the rider table `+0xE4`->`+0x60` maps the mount display's
            // slot to, RIDER_STAND when none, `0x0079BB10`, and copies the mount display's clip
            // time), so an action loop without a mount clip leaves both on the gait level.
            let action = loop_slot.and_then(|s| kit.actions.get(s)).filter(|a| kit.mount.is_none() || !a.mount.is_empty());
            // The loop: gait level (a mounted figure's rung of its gait ladder), or the mode's
            // action loop (stand level if the table has none).
            let stand = kit.gait_levels(Gait::Stand).and_then(|l| l.first());
            let (level, advance, start) = match on_ladder(&kit.ladder, &mut figures.blends[i], ground_speed, dt, action, sel, |a| atlas.slot(a).map_or(0.0, |s| s.duration())) {
                Some(Some(on)) => on,
                // On the ladder's stand rung: the stand level, or the action loop shown, by its kind.
                Some(None) => match stand {
                    Some(level) => (level, loop_advance(&figures.blends[i], action.unwrap_or(level), ground_speed, sel, dt), None),
                    None => continue,
                },
                None => {
                    // A charging man runs in the charge graph (`LocoGraph`), when his table has it.
                    let charge = kit.actions.get(unit_animation::CHARGE).filter(|_| if settling { kind == MoveKind::Charge } else { unit.charging && gait != Gait::Stand });
                    let picked = match charge {
                        Some(c) => on_levels(std::slice::from_ref(c), stand, &mut figures.blends[i], LocoGraph::Charge, ground_speed, dt, action, sel),
                        None => kit.gait_levels(gait).and_then(|l| on_levels(l, stand, &mut figures.blends[i], gait, ground_speed, dt, action, sel)),
                    };
                    match picked {
                        Some((level, advance)) => (level, advance, None),
                        None => continue,
                    }
                }
            };
            let shown_level = action.unwrap_or(level);
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
            let Some(want) = level_loop(shown_level, sel, start, |a| atlas.slot(a)) else { continue };
            let blend = &mut figures.blends[i];
            blend.step(&want, shown, || carry_speed(shown_level, sel), skin::phase(view.id, i), advance, dt, man_fig.is_some());
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
            let bslot = figures.first_slot + figures.men.len();
            if let Some(BearerLoop { want, advance, carry }) = bearer_loop(bearer.kit.gait_levels(gait), bearer.kit.gait_levels(Gait::Stand).and_then(|l| l.first()), || bearer.kit.anims(gait), gait, blend, ground_speed, dt, sel, |a| atlas.slot(a))
                && bslot * 2 + 1 < skin_data.len()
            {
                blend.step(&want, shown, || carry, skin::phase(view.id, usize::MAX), advance, dt, false);
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
    sim.unit(id)
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

    /// A man's place and lag (model `march`, unit frame) land where the view draws him: the unit's
    /// rotation `facing − π/2` about Y, world (X, Z) = model (x, −y).
    #[test]
    fn men_are_drawn_where_the_model_walks_them() {
        for facing in [0.0f32, 1.0, 2.5, -2.0] {
            let (right, back) = (1.5, -0.7);
            let p = Quat::from_rotation_y(facing - std::f32::consts::FRAC_PI_2) * Vec3::new(right, 0.0, back);
            let (x, y) = ntw_sim::battle::march::unit_frame_offset(facing, (right, back));
            assert!((p.x - x).abs() < 1e-5 && (-p.z - y).abs() < 1e-5, "{facing}: {p} vs ({x}, {y})");
        }
    }

    /// Each man's speed is drawn as his difference from the unit's; once the unit stands (after
    /// its still ticks), a man still behind his place walks at his own model speed; a paused model
    /// moves no one.
    #[test]
    fn a_man_walks_to_his_place_after_his_unit_stands() {
        use ntw_sim::battle::march::ManMarch;
        let mut u = LandUnit::new(0, 0, 2, (0.0, 0.0));
        u.march.men = (0..2).map(|j| ManMarch { lag: 1.0, figure: j, ..Default::default() }).collect();
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        let mut d = MenDrawn::default();
        for tick in 1..=3 {
            u.position.0 += 0.2;
            (u.speed, u.moved) = (2.0, true);
            (u.march.men[0].speed, u.march.men[1].speed) = (2.2, 2.0);
            g.observe_unit(&u, tick);
            d.observe(&u, &g, tick, 2);
        }
        assert!((d.speed(0, 1.0) - 0.2).abs() < 1e-4 && d.speed(1, 1.0).abs() < 1e-4, "{d:?}");
        assert_eq!(d.at(0, 0.5).map(|a| a[1]), Some(u.man_place(0).1 + 1.0), "his lag behind his slot");
        (u.speed, u.moved) = (0.0, false);
        (u.march.men[0].speed, u.march.men[1].speed) = (1.0, 0.0);
        for tick in 4..3 + STILL_TICKS {
            g.observe_unit(&u, tick);
            d.observe(&u, &g, tick, 2);
            assert_eq!(d.to[0][2], 0.0, "the unit's last speed is still bridged");
        }
        let tick = 3 + STILL_TICKS;
        g.observe_unit(&u, tick);
        d.observe(&u, &g, tick, 2);
        assert_eq!(g.gait(false), Gait::Stand);
        assert!(d.settling(0) && !d.settling(1) && (d.speed(0, 1.0) - 1.0).abs() < 1e-6);
        for _ in 0..2 {
            d.observe(&u, &g, tick, 2);
        }
        assert!(!d.settling(0), "paused: {d:?}");
    }

    /// A re-slot moves the model's men between entries: each figure is drawn where his own man
    /// stands (his entry's slot plus his offset from it), and moves on from there, not snapped.
    #[test]
    fn each_figure_follows_his_own_man_through_a_reslot() {
        use ntw_sim::battle::march::ManMarch;
        let mut u = LandUnit::new(0, 0, 4, (0.0, 0.0));
        u.march.men = (0..4).map(|j| ManMarch { figure: j, ..Default::default() }).collect();
        let g = GroundSpeed::new(Vec2::ZERO, 0, true);
        let mut d = MenDrawn::default();
        d.observe(&u, &g, 1, 4);
        let before: Vec<_> = (0..4).map(|j| d.at(j, 1.0).unwrap()).collect();
        // Figures 0 and 3 trade entries; each keeps standing where he stood.
        let (p0, p3) = (u.man_place(0), u.man_place(3));
        u.march.men.swap(0, 3);
        (u.march.men[0].side, u.march.men[0].lag) = (p3.0 - p0.0, p3.1 - p0.1);
        (u.march.men[3].side, u.march.men[3].lag) = (p0.0 - p3.0, p0.1 - p3.1);
        d.observe(&u, &g, 2, 4);
        for (j, b) in before.iter().enumerate() {
            let a = d.at(j, 1.0).unwrap();
            assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5, "figure {j}: {a:?} vs {b:?}");
        }
        assert_eq!((d.entry(0), d.entry(3), d.entry(1)), (3, 0, 1));
    }

    /// A unit first seen after it ticked (spawned late): each figure starts where his own man
    /// stands, not lerped from slot `j` to him.
    #[test]
    fn a_figure_first_seen_starts_at_his_man() {
        use ntw_sim::battle::march::ManMarch;
        let mut u = LandUnit::new(0, 0, 4, (0.0, 0.0));
        u.march.men = [3, 1, 2, 0].into_iter().map(|j| ManMarch { figure: j, lag: 0.5, ..Default::default() }).collect();
        let g = GroundSpeed::new(Vec2::ZERO, 0, true);
        let mut d = MenDrawn::default();
        d.observe(&u, &g, 7, 4);
        for j in 0..4 {
            let (a, b) = (d.at(j, 0.0).unwrap(), d.at(j, 1.0).unwrap());
            assert_eq!((a[0], a[1]), (b[0], b[1]), "figure {j} does not move");
        }
        let p = u.man_place(0);
        assert_eq!(d.at(3, 0.0).map(|a| (a[0], a[1])), Some((p.0, p.1 + 0.5)), "figure 3 is man 0");
    }
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
        let info = |id| UnitInfo { id, experience: id, ..UnitInfo::default() };
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
        assert_eq!(view.info(world.resource::<BattleSim>()).map(|i| i.experience), Some(5));
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
        let (walk, run) = (kit_levels(&WALK_LEVELS), kit_levels(&RUN_LEVELS));
        let mut b = ClipBlend::default();
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
            let gait = g.gait(speed == RUN);
            let levels = if gait == Gait::Run { &run } else { &walk };
            let (_, advance) = on_levels(levels, None, &mut b, gait, g.at(fixed / TICK_SECONDS), 1.0, None, 0).unwrap();
            let level = match b.rung {
                Rung::Loop(i) | Rung::Into(i) => i,
                Rung::Stand => 0,
            };
            if tick >= 2 {
                out.push((gait, level, advance));
            }
        }
        out
    }

    /// Speed levels with these root speeds (one 1 s clip each).
    fn kit_levels(speeds: &[f32]) -> Vec<crate::soldiers::KitLevel> {
        let anim = || std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration: 1.0, bones: vec![], frames: vec![], events: vec![] });
        speeds.iter().map(|&speed| crate::soldiers::KitLevel { man: vec![anim()], speeds: vec![speed], blend_in: vec![0.5], mount: vec![], mount_speeds: vec![], mount_blend_in: vec![], kind: 2 }).collect()
    }

    /// `0x007A7190` / the locomotion graph on foot: the run order plays the run graph's levels,
    /// the walk order the walk graph's; switching graph enters the new one on its fastest level
    /// no faster than the man, then levels step up at midpoints and down at quarter points.
    #[test]
    fn a_man_changes_graph_with_the_order_and_level_with_hysteresis() {
        let (walk, run) = (kit_levels(&WALK_LEVELS), kit_levels(&RUN_LEVELS));
        let mut b = ClipBlend::default();
        let level = |b: &ClipBlend| match b.rung {
            Rung::Loop(i) | Rung::Into(i) => i,
            Rung::Stand => usize::MAX,
        };
        on_levels(&walk, None, &mut b, Gait::Walk, WALK, 0.1, None, 0);
        assert_eq!(level(&b), 1, "1.4 m/s: WALK_TRAINED_2 (1.27)");
        // Run order: the run graph, entered below its slowest level, climbs to it.
        on_levels(&run, None, &mut b, Gait::Run, 1.6, 0.1, None, 0);
        assert_eq!(level(&b), 0);
        // 2.6 m/s is past the 2.19 / 3.13 midpoint (2.66)? No: stays; 2.7 is.
        on_levels(&run, None, &mut b, Gait::Run, 2.6, 0.1, None, 0);
        assert_eq!(level(&b), 0);
        on_levels(&run, None, &mut b, Gait::Run, 2.7, 0.1, None, 0);
        assert_eq!(level(&b), 1);
        // Down only at 2.19 + 0.25 x 0.94 = 2.425 m/s.
        on_levels(&run, None, &mut b, Gait::Run, 2.5, 0.1, None, 0);
        assert_eq!(level(&b), 1);
        on_levels(&run, None, &mut b, Gait::Run, 2.4, 0.1, None, 0);
        assert_eq!(level(&b), 0);
        // Walk order at 3.6 m/s (decelerating): the walk graph on its fastest level, 1.73.
        on_levels(&run, None, &mut b, Gait::Run, 3.6, 0.1, None, 0);
        on_levels(&walk, None, &mut b, Gait::Walk, 3.6, 0.1, None, 0);
        assert_eq!(level(&b), 2);
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
            assert_eq!(g.gait(false), Gait::Walk, "tick {tick}");
        }
        for tick in 11..10 + STILL_TICKS {
            g.observe(Vec2::new(x, 0.0), tick, false, true);
            assert_eq!(g.gait(false), Gait::Walk, "tick {tick}");
        }
        g.observe(Vec2::new(x, 0.0), 10 + STILL_TICKS, false, true);
        assert_eq!(g.gait(false), Gait::Stand);
        assert_eq!(g.at(0.5), 0.0, "a standing unit has no speed");
    }

    /// `0x00752FF0`: the figures see the speed lerped between the last two ticks by the tick
    /// fraction, so a slowing unit's speed falls smoothly between ticks (the run -> walk hiccup of
    /// the user's 2026-10-10 check), from 0 when it starts.
    #[test]
    fn the_figures_see_the_speed_lerped_between_ticks() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let mut x = 0.0;
        for (tick, speed) in [(1u32, 3.0f32), (2, 2.5)] {
            x += speed * TICK_SECONDS;
            g.observe(Vec2::new(x, 0.0), tick, true, true);
        }
        assert!(near(g.at(0.0), 3.0) && near(g.at(0.5), 2.75) && near(g.at(1.0), 2.5), "{g:?}");
        // A tick without a move holds the last speed.
        g.observe(Vec2::new(x, 0.0), 3, false, true);
        assert!(near(g.at(0.0), 2.5) && near(g.at(1.0), 2.5), "{g:?}");
        // A start out of the stand rises from 0.
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        g.observe(Vec2::new(0.3, 0.0), 1, true, true);
        assert!(near(g.at(0.0), 0.0) && near(g.at(0.5), 1.5) && near(g.at(1.0), 3.0), "{g:?}");
    }

    /// `0x0078B8A0`: the playback scale is 0.95 + the MSVC `rand` step's high 16 bits x
    /// 1.5259022e-6, in [0.95, 1.05]; the display constructor (`0x007179B0`) draws once before it.
    #[test]
    fn the_playback_scale_follows_the_exe_draw() {
        let mut s = 0u32;
        // 0 x 0x343FD + 0x269EC3 = 0x00269EC3: high bits 0x26.
        assert_eq!(playback_scale(&mut s), 0x26 as f32 * 1.525_902_2e-6 + 0.95);
        assert_eq!(s, 0x0026_9EC3);
        let (mut t, mut u) = (0u32, 0u32);
        playback_scale(&mut u);
        assert_eq!(display_scale(&mut t), playback_scale(&mut u), "the second draw");
        let mut s = 12345u32;
        let scales: Vec<f32> = (0..1000).map(|_| display_scale(&mut s)).collect();
        assert!(scales.iter().all(|x| (0.95..=1.05).contains(x)));
        assert!(scales.iter().any(|&x| x < 0.96) && scales.iter().any(|&x| x > 1.04));
    }

    /// The user's check of 2026-10-10 (cavalry in lockstep): figures on the same locomotion loop
    /// at the same speed advance at their own playback scales (`+0xB0`, `0x007725D0` kind 2), so
    /// they drift apart; a transition clip (kind 3) does not scale. And a transition ends by the
    /// length of the alternative the figure plays (`sel`), not the first one's.
    #[test]
    fn figures_drift_apart_on_one_loop_by_their_scales() {
        use crate::soldiers::{KitLevel, LadderRung};
        let walk = kit_levels(&WALK_LEVELS);
        let (mut slow, mut fast) = (ClipBlend::with_scale(0.95), ClipBlend::with_scale(1.05));
        let (_, a) = on_levels(&walk, None, &mut slow, Gait::Walk, WALK, 0.1, None, 0).unwrap();
        let (_, b) = on_levels(&walk, None, &mut fast, Gait::Walk, WALK, 0.1, None, 0).unwrap();
        assert!((b / a - 1.05 / 0.95).abs() < 1e-5, "{a} vs {b}");
        let anim = |duration: f32| std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration, bones: vec![], frames: vec![], events: vec![] });
        let level = |speed: f32, lens: &[f32]| KitLevel { man: lens.iter().map(|&l| anim(l)).collect(), speeds: vec![speed; lens.len()], blend_in: vec![0.5; lens.len()], mount: vec![], mount_speeds: vec![], mount_blend_in: vec![], kind: 2 };
        let ladder = vec![LadderRung { into: Some(level(0.6, &[3.0, 1.0])), gait: level(1.47, &[1.1]) }];
        let len = |a: &std::sync::Arc<Anim>| a.duration;
        let dt = 0.05;
        let mut rates = Vec::new();
        for scale in [0.95, 1.05] {
            let mut b = ClipBlend::with_scale(scale);
            let (_, into, _) = on_ladder(&ladder, &mut b, 0.5, dt, None, 0, len).unwrap().unwrap();
            b.rung = Rung::Loop(0);
            let (_, walk, _) = on_ladder(&ladder, &mut b, 0.5, dt, None, 0, len).unwrap().unwrap();
            rates.push((into, walk));
        }
        assert_eq!(rates[0].0, rates[1].0, "a transition plays without the scale");
        assert!((rates[1].1 / rates[0].1 - 1.05 / 0.95).abs() < 1e-5, "{rates:?}");
        // The figure on the 1 s alternative is done with it at 1.2 s; on the 3 s one it is not.
        for (sel, done) in [(0..64).find(|&s| alternative(s, 2) == 1).unwrap(), (0..64).find(|&s| alternative(s, 2) == 0).unwrap()].into_iter().zip([true, false]) {
            let mut b = ClipBlend { rung: Rung::Into(0), time: 1.2, ..ClipBlend::default() };
            on_ladder(&ladder, &mut b, 1.4, dt, None, sel, len);
            assert_eq!(b.rung == Rung::Loop(0), done, "sel {sel}: {:?}", b.rung);
        }
    }

    /// The exe reads the display's playback scale once (`0x00772E3D`), for slot kind 2 only: a
    /// stand (kind 0), an action loop (`COMBAT_READY`, `AIM`: kind 0) and a melee idle
    /// (`COMBAT_IDLE_n`, kind 0x11, the soldier's own clip clock, `0x0077430C`) play at rate 1,
    /// on foot and up the mounted ladder, whatever the figure's speed; walk and run loops take
    /// the scale. The kind is the shown clip's: an action loop over a moving gait plays by its
    /// own kind, not the gait level's.
    #[test]
    fn only_moving_gait_loops_take_the_playback_scale() {
        use crate::soldiers::{KitLevel, LadderRung};
        use ntw_formats::battle_animation::slot_kind;
        let dt = 0.1;
        let with_kind = |kind: u8| KitLevel { kind, ..kit_levels(&[0.0]).remove(0) };
        let (ready, melee) = (with_kind(slot_kind("COMBAT_READY").unwrap()), with_kind(slot_kind("COMBAT_IDLE_1").unwrap()));
        let advance = |levels: &[KitLevel], gait: Gait, speed: f32, scale: f32, action: Option<&KitLevel>| {
            on_levels(levels, None, &mut ClipBlend::with_scale(scale), gait, speed, dt, action, 0).unwrap().1
        };
        let stand = [with_kind(slot_kind("STAND").unwrap())];
        assert_eq!(advance(&stand, Gait::Stand, 0.0, 1.05, None), dt, "stand: kind 0, rate 1");
        assert_eq!(advance(&stand, Gait::Stand, 0.7, 1.05, None), dt, "stand at a shuffle: kind 0, rate 1");
        for (levels, gait, speed) in [(&WALK_LEVELS[..], Gait::Walk, WALK), (&RUN_LEVELS[..], Gait::Run, RUN)] {
            let levels = kit_levels(levels);
            let plain = advance(&levels, gait, speed, 1.0, None);
            assert!((advance(&levels, gait, speed, 1.05, None) / plain - 1.05).abs() < 1e-5, "{gait:?} scaled");
            assert_eq!(advance(&levels, gait, speed, 1.05, Some(&ready)), dt, "{gait:?} under an action loop: kind 0, rate 1");
            assert_eq!(advance(&levels, gait, speed, 1.05, Some(&melee)), dt, "{gait:?} under a melee idle: the clip clock, rate 1");
        }
        let anim = |duration: f32| std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration, bones: vec![], frames: vec![], events: vec![] });
        let level = |speed: f32, kind: u8| KitLevel { man: vec![anim(1.1)], speeds: vec![speed], blend_in: vec![0.5], mount: vec![], mount_speeds: vec![], mount_blend_in: vec![], kind };
        let ladder = vec![LadderRung { into: Some(level(0.6, 3)), gait: level(1.47, 2) }];
        let len = |a: &std::sync::Arc<Anim>| a.duration;
        let rung_loop = |scale: f32, action: Option<&KitLevel>| {
            let mut b = ClipBlend { rung: Rung::Loop(0), ..ClipBlend::with_scale(scale) };
            on_ladder(&ladder, &mut b, 1.47, dt, action, 0, len).unwrap().unwrap().1
        };
        assert!((rung_loop(1.05, None) / rung_loop(1.0, None) - 1.05).abs() < 1e-5, "ladder loop scaled");
        assert_eq!(rung_loop(1.05, Some(&ready)), dt, "ladder action loop: kind 0, rate 1");
        // The turns into a walk (0xC / 0xD) play at the forward speed over the root speed, the
        // backward engine moves (4) at the ground speed over it (`0x00772C71`); ours has one
        // speed, the ground speed, for both. Neither takes the scale.
        for kind in [slot_kind("TURN_LEFT_TO_WALK_45").unwrap(), slot_kind("TURN_RIGHT_TO_WALK_45").unwrap(), slot_kind("ENGINE_LIMBERED_BACKWARDS").unwrap()] {
            let paced = KitLevel { speeds: vec![2.0], ..with_kind(kind) };
            assert!((advance(std::slice::from_ref(&paced), Gait::Walk, 1.0, 1.05, None) - dt * 0.5).abs() < 1e-6, "kind {kind:#x} unscaled");
        }
    }

    /// gait-infantry (2026-10-12): the stand rung of a man's moving graph plays the kit's stand
    /// level (node 0 maps to `STAND` in the walk and charge slot tables `0x0133C608` / `0x0133D030`) at
    /// rate 1, unscaled (kind 0, `0x00773086`); it played the first walk level at the clamped
    /// rate 0.25, a man walking in place as his unit started off. On a level the rate is the
    /// speed over the clip's root speed with no clamp (`0x00772E22`..`0x00772E2B`): it was held
    /// at 0.25..4.
    #[test]
    fn a_moving_graphs_stand_rung_plays_the_stand_and_levels_are_unclamped() {
        let walk = kit_levels(&WALK_LEVELS);
        let stand = [crate::soldiers::KitLevel { kind: 0, ..kit_levels(&[0.0]).remove(0) }];
        let dt = 0.1;
        let mut b = ClipBlend::with_scale(1.05);
        let (level, advance) = on_levels(&walk, stand.first(), &mut b, Gait::Walk, 0.005, dt, None, 0).unwrap();
        assert_eq!(b.rung, Rung::Stand);
        assert!(std::ptr::eq(level, &stand[0]), "the stand rung plays the stand level");
        assert_eq!(advance, dt, "kind 0: rate 1, unscaled");
        // Off at 0.15 m/s: the slowest level (1 m/s) at 0.15, scaled; it was clamped to 0.25.
        let (level, advance) = on_levels(&walk, stand.first(), &mut b, Gait::Walk, 0.15, dt, None, 0).unwrap();
        assert_eq!(b.rung, Rung::Loop(0));
        assert_eq!(level.speed_of(0), 1.0);
        assert!((advance - dt * 0.15 * 1.05).abs() < 1e-6, "{advance}");
        // A sprint past four times a level's speed is not held at 4 either.
        assert!((loop_rate(5.0, 1.0) - 5.0).abs() < 1e-6 && loop_rate(0.0, 1.0) == 0.0);
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
        assert_eq!(g.gait(false), Gait::Walk);
        assert!((g.speed - WALK).abs() < 1e-3, "start: speed {}", g.speed);
        // A stop: a step, then two still ticks in the same frame.
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        for (tick, x, moved) in [(1, step, true), (2, step, false), (3, step, false)] {
            g.observe(Vec2::new(x, 0.0), tick, moved, true);
        }
        assert_eq!(g.gait(false), Gait::Walk);
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
        assert_eq!(g.gait(false), Gait::Stand, "the arrival tick is not a step");
        b.step();
        let u = unit(&b);
        g.observe_unit(&u, b.tick);
        assert_eq!(g.gait(false), Gait::Walk, "speed {}", g.speed);
        assert!(g.speed <= u.walk_speed + 1e-3, "speed {} walk {}", g.speed, u.walk_speed);
    }

    /// A deployment drag moves the unit with no model tick: not walking.
    #[test]
    fn a_deployment_drag_is_not_walking() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        g.observe(Vec2::new(40.0, 0.0), 0, false, true);
        assert_eq!(g.gait(false), Gait::Stand);
    }

    /// The tick going back (not expected: a restart rebuilds the views) starts over instead of
    /// spreading the next step over ~4 billion ticks.
    #[test]
    fn tick_going_back_starts_over() {
        let mut g = GroundSpeed::new(Vec2::ZERO, 0, true);
        g.observe(Vec2::new(0.14, 0.0), 1, true, true);
        g.observe(Vec2::new(50.0, 0.0), 0, false, true);
        assert_eq!(g.gait(false), Gait::Stand);
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

    /// Loop clips (their `Arc` addresses in `sync_views`), each 1 s (21 frames, 20 intervals) at
    /// 20 frames/s, 1 bone, at their own place in the bone atlas.
    fn clip(key: usize, blend_in: f32) -> LoopClip {
        LoopClip { key, man: ClipSlot { base: key as u32, frames: 21, bones: 1, rate: 20.0 }, blend_in, mount_key: 0, mount: None, mount_blend_in: 0.0, start: None, kind: 2 }
    }
    /// `man` ridden on the mount clip at atlas slot `mount` (its key: the slot's base), whose
    /// line blends over 0.25 s.
    fn ride(mount: ClipSlot, man: LoopClip) -> LoopClip {
        LoopClip { mount_key: mount.base as usize, mount: Some(mount), mount_blend_in: 0.25, ..man }
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
        assert_eq!(b.fades[0].frozen[0].keep, 0.0, "the first clip is drawn at once");
        let shown = b.run(&clip(A, blend_in), WALK, dt);
        assert_eq!(b.fades[0].frozen[0].keep, 0.0, "no change");
        b.run(&clip(B, blend_in), WALK, dt);
        assert_eq!(b.fades[0].frozen[0].fig, shown[0], "fades out of what was drawn the frame before the change");
        let mut keeps = vec![b.fades[0].frozen[0].keep];
        while *keeps.last().unwrap() > 0.0 {
            b.run(&clip(B, blend_in), WALK, dt);
            keeps.push(b.fades[0].frozen[0].keep);
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
        let level = crate::soldiers::KitLevel { man: vec![anim(), anim()], speeds: vec![1.4, 1.4], blend_in: vec![0.5, 0.25], mount: vec![], mount_speeds: vec![], mount_blend_in: vec![], kind: 2 };
        assert_eq!([0, 1, 2, 3].map(|sel| level.blend_in_of(sel)), [0.5, 0.25, 0.5, 0.25]);
    }

    /// A zero blend time draws the new clip at once (the exe draws the new clip alone for D = 0).
    #[test]
    fn a_zero_blend_time_snaps() {
        let mut b = Fig::default();
        b.run(&clip(A, 0.5), WALK, 0.125);
        b.run(&clip(B, 0.0), WALK, 0.125);
        assert_eq!(b.fades[0].frozen, [Frozen::default(); skin::FROZEN]);
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
        assert_eq!(steady.fades[0].frozen[0].keep, 0.0, "and no cross-fade");
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
        assert_eq!(b.fades[0].frozen[0].keep, 0.75);
        // The level changes 0.125 s later: the frame drawn was B at 0.25 and A at 0.75.
        b.run(&clip(C, 0.5), WALK, 0.125);
        assert_eq!(b.fades[0].frozen, [Frozen { fig: at_b[0], keep: 0.25 }, Frozen { fig: at_a[0], keep: 0.75 }, Frozen::default(), Frozen::default()]);
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
            let sum: f32 = b.fades[0].frozen.iter().map(|f| f.keep).sum();
            assert!(sum <= 1.0 + 1e-6, "weights {:?}", b.fades[0].frozen);
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
        assert_eq!(b.fades[0].frozen, [Frozen { fig: at_a[0], keep: 1.0 }, Frozen::default(), Frozen::default(), Frozen::default()]);
    }

    /// One pose more than the GPU keeps within one blend time: the lightest goes, its weight to
    /// its neighbour in time (the next newer pose; the next older for the newest), so the weights
    /// still add up to 1 and only the pose most like it gains.
    #[test]
    fn a_pose_past_the_limit_gives_its_weight_to_its_neighbour() {
        let pose = |n: u32| [n, n, 0, 0];
        let frozen = |keeps: [f32; skin::FROZEN]| -> [Frozen; skin::FROZEN] { std::array::from_fn(|k| Frozen { fig: pose(k as u32 + 1), keep: keeps[k] }) };
        // The live clip has 0.4; of the frozen 0.2, 0.15, 0.05, 0.2 the 0.05 (pose 3) goes to
        // pose 2, frozen just after it.
        let out = freeze(frozen([0.2, 0.15, 0.05, 0.2]), pose(9));
        let want = [(9, 0.4), (1, 0.2), (2, 0.2), (4, 0.2)];
        for (got, (fig, keep)) in out.iter().zip(want) {
            assert_eq!(got.fig, pose(fig));
            assert!((got.keep - keep).abs() < 1e-6, "{out:?}");
        }
        // The newest (the live clip, weight 0.05) the lightest: to the next older.
        let out = freeze(frozen([0.3, 0.25, 0.2, 0.2]), pose(9));
        assert_eq!(out.map(|f| f.fig), [pose(1), pose(2), pose(3), pose(4)]);
        assert!((out[0].keep - 0.35).abs() < 1e-6);
        // Under the limit nothing changes but the new pose in front.
        let out = freeze(frozen([0.5, 0.0, 0.0, 0.0]), pose(9));
        assert_eq!(out, [Frozen { fig: pose(9), keep: 0.5 }, Frozen { fig: pose(1), keep: 0.5 }, Frozen::default(), Frozen::default()]);
    }

    /// The user's check of 2026-10-10 (a hiccup on run -> walk): a slowing man changes clip about
    /// once a model tick (the walk graph entered on its fastest level, then down a level a tick).
    /// At 60 frames/s and 0.5 s blends, four changes 0.1 s apart keep every pose they froze, so
    /// none drops out at once (with two poses the third change dropped about a quarter of the
    /// weight).
    #[test]
    fn a_run_to_walk_keeps_every_frozen_pose() {
        let dt = 1.0 / 60.0;
        let mut b = Fig::default();
        for _ in 0..6 {
            b.run(&clip(A, 0.5), RUN, dt);
        }
        for (n, k) in [B, C, D, A].into_iter().enumerate() {
            for _ in 0..6 {
                b.run(&clip(k, 0.5), WALK, dt);
            }
            let kept = b.fades[0].frozen.iter().filter(|f| f.keep > 0.0).count();
            assert_eq!(kept, n + 1, "change {n}: {:?}", b.fades[0].frozen);
        }
    }

    /// Manager review: the mount clip is picked again every frame, also when the man's clip does
    /// not change (two levels sharing the man's clip).
    #[test]
    fn the_mount_clip_follows_while_the_mans_stays() {
        let mut b = Fig::default();
        let stand = ClipSlot { base: 500, frames: 21, bones: 1, rate: 20.0 };
        let walk = ClipSlot { base: 900, ..stand };
        b.run(&ride(stand, clip(A, 0.5)), STAND, 0.125);
        let drawn = b.run(&ride(walk, clip(A, 0.5)), STAND, 0.125);
        assert_eq!(b.mount, Some(walk));
        assert_eq!(drawn[1], walk.figure(b.time));
    }

    /// Each display blends on its own (`0x007725D0` runs per display): a mount clip changing
    /// alone cross-fades the mount over the mount line's blend time while the rider, whose clip
    /// stayed, draws his clip alone; a change of both fades each over its own line's time.
    #[test]
    fn the_mount_blends_its_own_clip_changes() {
        let stand = ClipSlot { base: 500, frames: 21, bones: 1, rate: 20.0 };
        let walk = ClipSlot { base: 900, ..stand };
        let mut b = Fig::default();
        b.run(&ride(stand, clip(A, 0.5)), STAND, 0.125);
        let before = b.run(&ride(stand, clip(A, 0.5)), STAND, 0.125);
        b.run(&ride(walk, clip(A, 0.5)), STAND, 0.125);
        assert_eq!(b.fade(0), None, "the rider's clip did not change");
        assert_eq!(b.fades[1].frozen[0], Frozen { fig: before[1], keep: 1.0 });
        // 0.25 s line at 8 frames/s: 1, 0.5, gone.
        b.run(&ride(walk, clip(A, 0.5)), STAND, 0.125);
        assert_eq!(b.fades[1].frozen[0].keep, 0.5);
        b.run(&ride(walk, clip(A, 0.5)), STAND, 0.125);
        assert!(!b.fading());
        // Both change: the rider over his 0.5 s, the mount over its 0.25 s.
        b.run(&ride(stand, clip(B, 0.5)), WALK, 0.125);
        b.run(&ride(stand, clip(B, 0.5)), WALK, 0.125);
        assert_eq!((b.fades[0].frozen[0].keep, b.fades[1].frozen[0].keep), (0.75, 0.5));
    }

    /// The horse's ladder in the view (`on_ladder`, debugger sitting 2026-10-09): out of the
    /// stand the transition starts at 0 and cross-fades; at its end the loop follows at the time
    /// past the end, drawn at once; a step up from a loop starts the next transition at the loop's
    /// phase, drawn at once, and plays it at the transition rate.
    #[test]
    fn a_horse_climbs_its_ladder_through_the_transition_clips() {
        use crate::soldiers::{KitLevel, LadderRung};
        let anim = |duration: f32| std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration, bones: vec![], frames: vec![], events: vec![] });
        let level = |speed: f32, len: f32| KitLevel { man: vec![anim(len)], speeds: vec![speed], blend_in: vec![0.5], mount: vec![anim(len)], mount_speeds: vec![speed], mount_blend_in: vec![0.5], kind: 2 };
        let ladder = vec![
            LadderRung { into: Some(level(0.6, 3.0)), gait: level(1.47, 1.1) },
            LadderRung { into: Some(level(2.69, 2.7)), gait: level(3.533, 0.8) },
        ];
        let len = |a: &std::sync::Arc<ntw_formats::anim::Anim>| a.duration;
        let mut b = ClipBlend::default();
        let dt = 0.1;
        let (lv, adv, start) = on_ladder(&ladder, &mut b, 0.5, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, ladder[0].into.as_ref().unwrap()));
        assert_eq!((b.rung, start), (Rung::Into(0), Some((0.0, true))));
        assert_eq!(adv, dt * 5.0, "0.5 m/s over the 0.1 m/s floor");
        // 2.95 s into the 3 s STAND_TO_WALK, this frame's advance takes it past its end: the walk
        // at the time past it, at once, with nothing more added (`0x0077404C`..`0x007740A9`).
        b.time = 2.95;
        let past = 2.95 + dt * unit_animation::transition_rate(1.4, 0.0, 1.47, 2.95 / 3.0) - 3.0;
        let (lv, adv, start) = on_ladder(&ladder, &mut b, 1.4, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, &ladder[0].gait));
        assert_eq!(b.rung, Rung::Loop(0));
        assert!(past > 0.0 && (start.unwrap().0 - past).abs() < 1e-5 && !start.unwrap().1 && adv == 0.0, "{start:?} {past} {adv}");
        // Walking (a 1.1 s clip) 0.3 s into a cycle, the unit at 3 m/s: the walk goes on until its
        // cycle wraps (`[ESP + 0x118]` test at `0x00773CCF`).
        b.man = ClipSlot { base: 0, frames: 23, bones: 1, rate: 20.0 };
        b.clip = std::sync::Arc::as_ptr(&ladder[0].gait.man[0]) as usize;
        b.time = 2.5;
        let (lv, _, start) = on_ladder(&ladder, &mut b, 3.0, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, &ladder[0].gait) && start.is_none() && b.rung == Rung::Loop(0));
        // 1.05 s into the cycle: this frame (0.204 s of walk at 3 / 1.47) wraps it, WALK_TO_TROT
        // starts at the 0.154 s past the wrap, at once.
        b.time = 3.25;
        let (lv, adv, start) = on_ladder(&ladder, &mut b, 3.0, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, ladder[1].into.as_ref().unwrap()));
        assert_eq!(b.rung, Rung::Into(1));
        let (t, fade) = start.unwrap();
        let phase = (3.25 + dt * 3.0 / 1.47f32).rem_euclid(1.1);
        assert!((t - phase).abs() < 1e-4 && !fade && adv == 0.0, "{t} {adv} {phase}");
        // The next frame plays it at the transition rate.
        b.time = t;
        let (_, adv, start) = on_ladder(&ladder, &mut b, 3.0, dt, None, 0, len).unwrap().unwrap();
        let want = unit_animation::transition_rate(3.0, 1.47, 3.533, t / 2.7);
        assert!((adv - dt * want).abs() < 1e-6 && start.is_none());
        // Standing still again: off the ladder's loops, the stand level.
        assert!(on_ladder(&ladder, &mut b, 0.0, dt, None, 0, len).unwrap().is_none());
        assert!(on_ladder(&[], &mut b, 3.0, dt, None, 0, len).is_none(), "on foot: no ladder");
    }

    /// A step down from a transition (`climb`'s `Into(i)` down edge, `0x007CE840`): slowing while
    /// WALK_TO_TROT plays, the figure goes back to the walk loop as a usual clip change (no start
    /// given, so `ClipBlend::step` carries the phase and cross-fades) at the walk's own rate.
    #[test]
    fn a_step_down_from_a_transition_is_a_usual_change_to_the_loop_below() {
        use crate::soldiers::{KitLevel, LadderRung};
        let anim = |duration: f32| std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration, bones: vec![], frames: vec![], events: vec![] });
        let level = |speed: f32, len: f32| KitLevel { man: vec![anim(len)], speeds: vec![speed], blend_in: vec![0.5], mount: vec![anim(len)], mount_speeds: vec![speed], mount_blend_in: vec![0.5], kind: 2 };
        let ladder = vec![
            LadderRung { into: Some(level(0.6, 3.0)), gait: level(1.47, 1.1) },
            LadderRung { into: Some(level(2.69, 2.7)), gait: level(3.533, 0.8) },
        ];
        let len = |a: &std::sync::Arc<ntw_formats::anim::Anim>| a.duration;
        let mut b = ClipBlend { rung: Rung::Into(1), time: 1.0, ..ClipBlend::default() };
        let dt = 0.1;
        // 1.9 m/s is at most the walk's 1.47 plus a quarter of the gap to the trot's 3.533.
        let (lv, adv, start) = on_ladder(&ladder, &mut b, 1.9, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, &ladder[0].gait));
        assert_eq!((b.rung, start), (Rung::Loop(0), None));
        assert!((adv - dt * 1.9 / 1.47).abs() < 1e-6, "{adv}");
        // Still above the down edge: the transition plays on.
        let mut b = ClipBlend { rung: Rung::Into(1), time: 1.0, ..ClipBlend::default() };
        let (lv, _, start) = on_ladder(&ladder, &mut b, 2.2, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, ladder[1].into.as_ref().unwrap()) && start.is_none() && b.rung == Rung::Into(1));
    }

    /// The start of a new clip on a usual change (`0x00773E73` / `0x00773F17`): the phase carries
    /// only into a looping kind (anim `+0x44` mask 0x2); a stand (kind 0) or a transition (kind 3)
    /// starts at 0; a sim-clocked kind (0xF..0x13) and a figure's first clip keep their own offset.
    #[test]
    fn only_a_looping_clip_takes_the_carried_phase() {
        let carried = || Some(0.4);
        assert_eq!(start_time(true, 2, carried, 0.7), 0.4, "a walk takes the phase");
        assert_eq!(start_time(true, 2, || None, 0.7), 0.7, "a walk the phase does not carry into");
        assert_eq!(start_time(true, 0, carried, 0.7), 0.0, "a stand starts at 0");
        assert_eq!(start_time(true, 3, carried, 0.7), 0.0, "a transition starts at 0");
        assert_eq!(start_time(true, 0xF, carried, 0.7), 0.7, "a melee clip is on the sim's clock");
        assert_eq!(start_time(false, 0, carried, 0.7), 0.7, "a figure's first clip");
    }

    /// gait-jitter (2026-10-11): a level change carries the stride's phase by the level's own root
    /// speed. The loop clips drawn are made in place, so their own root speed is 0; read from them
    /// (as before), no change ever carried and each step down restarted the stride at the figure's
    /// own offset (the probe: Loop1 -> Loop0 went from phase 0.89 to 0.27).
    #[test]
    fn a_level_change_carries_the_stride_by_the_levels_own_root_speed() {
        use ntw_formats::anim::{Anim, AnimBone, AnimKey};
        // A 23-frame walk at 20 frames/s whose root moves 1.617 m over its 1.1 s: 1.47 m/s.
        let key = |z: f32| AnimKey { translation: [0.0, 0.0, z], rotation: [0.0, 0.0, 0.0, 1.0], unknown: [0.0; 3] };
        let frames = (0..23).map(|f| vec![key(1.617 * f as f32 / 22.0)]).collect();
        let walk = Anim { frame_rate: 20.0, duration: 1.1, bones: vec![AnimBone { name: "Hips".into(), parent: None }], frames, events: vec![] };
        let speed = walk.root_speed();
        assert!((speed - 1.47).abs() < 1e-3, "{speed}");
        let drawn = std::sync::Arc::new(crate::soldiers::in_place(walk));
        assert!(drawn.root_speed() < MOVING_CLIP_SPEED, "the clip drawn loops in place");
        let level = crate::soldiers::KitLevel { man: vec![drawn.clone()], speeds: vec![speed], blend_in: vec![0.5], mount: vec![drawn], mount_speeds: vec![speed], mount_blend_in: vec![0.5], kind: 2 };
        // 0.98 s into the walk (phase 0.89), the trot (0.8 s) starts at the same phase.
        let t = carry_phase(0.98, 1.1, carry_speed(&level, 0), 0.8).expect("a moving level carries its phase");
        assert!((t - 0.98 / 1.1 * 0.8).abs() < 1e-5, "{t}");
    }

    /// The exe reads the root speed of the clip each display plays, its own alternative by
    /// selection (`0x00E5F760`): the playback rate (`0x00772E2B`), the phase carry
    /// (`0x00773E85`), the graph's edges (`0x007CEAA0`, `0x007CE840`) and the transition rate.
    /// Ours took the level's first clip's for every figure; a mounted figure goes by his mount's
    /// alternative (the rider copies its clip time, `0x0079BB10`).
    #[test]
    fn each_figure_goes_by_its_own_alternatives_root_speed() {
        use crate::soldiers::{KitLevel, LadderRung};
        let anim = || std::sync::Arc::new(Anim { frame_rate: 20.0, duration: 1.0, bones: vec![], frames: vec![], events: vec![] });
        let (first, second) = ((0..64).find(|&s| alternative(s, 2) == 0).unwrap(), (0..64).find(|&s| alternative(s, 2) == 1).unwrap());
        // On foot: walk alternatives at 1.0 and 1.6 m/s, run at 3.0.
        let foot = |speeds: Vec<f32>| KitLevel { man: speeds.iter().map(|_| anim()).collect(), blend_in: vec![0.5; speeds.len()], speeds, mount: vec![], mount_speeds: vec![], mount_blend_in: vec![], kind: 2 };
        let levels = vec![foot(vec![1.0, 1.6]), foot(vec![3.0])];
        assert_eq!((levels[0].speed_of(first), levels[0].speed_of(second)), (1.0, 1.6));
        let dt = 0.1;
        for (sel, clip) in [(first, 1.0), (second, 1.6)] {
            let mut b = ClipBlend::default();
            let (_, advance) = on_levels(&levels, None, &mut b, Gait::Walk, 1.2, dt, None, sel).unwrap();
            assert_eq!(b.rung, Rung::Loop(0));
            assert!((advance - dt * 1.2 / clip).abs() < 1e-6, "sel {sel}: {advance}");
            assert_eq!(carry_speed(&levels[0], sel), clip);
        }
        // Up to the run above the midpoint of the figure's own walk and the run: 2.0 for the 1.0
        // m/s alternative, 2.3 for the 1.6 one.
        for (sel, rung) in [(first, Rung::Loop(1)), (second, Rung::Loop(0))] {
            let mut b = ClipBlend::default();
            on_levels(&levels, None, &mut b, Gait::Walk, 1.2, dt, None, sel);
            on_levels(&levels, None, &mut b, Gait::Walk, 2.2, dt, None, sel);
            assert_eq!(b.rung, rung, "sel {sel}");
        }
        // Mounted: the mount's alternative, whatever the rider's.
        let mounted = KitLevel { man: vec![anim()], speeds: vec![0.0], blend_in: vec![0.5], mount: vec![anim(), anim()], mount_speeds: vec![2.0, 2.5], mount_blend_in: vec![0.5, 0.5], kind: 2 };
        assert_eq!((carry_speed(&mounted, first), carry_speed(&mounted, second)), (2.0, 2.5));
        let ladder = vec![LadderRung { into: None, gait: mounted }];
        let mut b = ClipBlend { rung: Rung::Loop(0), ..ClipBlend::default() };
        let (_, advance, _) = on_ladder(&ladder, &mut b, 2.0, dt, None, second, |a| a.duration).unwrap().unwrap();
        assert!((advance - dt * 2.0 / 2.5).abs() < 1e-6, "{advance}");
    }

    /// gait-jitter (2026-10-11): the standard bearer's per-gait clip (no speed levels) plays at
    /// the ground speed over its read root speed; read from the in-place clip drawn (0), the rate
    /// fell back to 1 whatever the speed.
    #[test]
    fn the_bearers_gait_clip_plays_by_its_read_root_speed() {
        use ntw_formats::anim::{Anim, AnimBone, AnimKey};
        let key = |z: f32| AnimKey { translation: [0.0, 0.0, z], rotation: [0.0, 0.0, 0.0, 1.0], unknown: [0.0; 3] };
        let frames = (0..23).map(|f| vec![key(1.617 * f as f32 / 22.0)]).collect();
        let walk = Anim { frame_rate: 20.0, duration: 1.1, bones: vec![AnimBone { name: "Hips".into(), parent: None }], frames, events: vec![] };
        let speed = walk.root_speed();
        let man = std::sync::Arc::new(crate::soldiers::in_place(walk));
        let anims = crate::soldiers::GaitAnims { man, speed, mount: None, slots: (String::new(), None), kind: 2 };
        let (_, slot) = in_place_clip(23);
        let rate = bearer_loop(None, None, || Some(&anims), Gait::Walk, &mut ClipBlend::default(), 2.0, 1.0, 0, |_| Some(slot)).unwrap().advance;
        assert!((rate - 2.0 / 1.47).abs() < 1e-3, "{rate}");
    }

    /// A clip as drawn (no keys, so its own root speed is 0, as an in-place loop's) and its atlas
    /// slot at 20 frames/s: `frames` frames, so (frames - 1) / 20 s long.
    fn in_place_clip(frames: u32) -> (std::sync::Arc<Anim>, ClipSlot) {
        let anim = std::sync::Arc::new(Anim { frame_rate: 20.0, duration: (frames - 1) as f32 / 20.0, bones: vec![], frames: vec![], events: vec![] });
        assert!(anim.root_speed() < MOVING_CLIP_SPEED);
        (anim, ClipSlot { base: frames * 100, frames, bones: 1, rate: 20.0 })
    }

    /// gait-jitter review (2026-10-11): through the per-figure step `sync_views` runs for a man on
    /// foot (`on_levels`, `level_loop`, `ClipBlend::step` carrying by `carry_speed`): a step down
    /// from the run (3.5 m/s, 0.8 s) to the walk (1.47 m/s, 1.1 s) goes on at the run's phase,
    /// though both clips drawn loop in place.
    #[test]
    fn a_mans_step_down_carries_his_stride_through_the_figure_step() {
        use crate::soldiers::KitLevel;
        let (walk, walk_slot) = in_place_clip(23);
        let (run, run_slot) = in_place_clip(17);
        let level = |speed: f32, a: &std::sync::Arc<Anim>| KitLevel { man: vec![a.clone()], speeds: vec![speed], blend_in: vec![0.0], mount: vec![], mount_speeds: vec![], mount_blend_in: vec![], kind: 2 };
        let levels = [level(1.47, &walk), level(3.5, &run)];
        let slot = |a: &std::sync::Arc<Anim>| Some(if std::sync::Arc::ptr_eq(a, &walk) { walk_slot } else { run_slot });
        let dt = 0.125;
        let mut b = ClipBlend::default();
        let mut frame = 0;
        let mut step = |b: &mut ClipBlend, speed: f32| {
            frame += 1;
            let shown = b.shown(frame);
            let (lv, advance) = on_levels(&levels, None, b, Gait::Run, speed, dt, None, 0).unwrap();
            let want = level_loop(lv, 0, None, slot).unwrap();
            b.step(&want, shown, || carry_speed(lv, 0), 0.5, advance, dt, false);
            b.draw(None, None, frame);
            advance
        };
        // Five frames on the run at its own speed: from the figure's offset (half of 0.8 s) on.
        for _ in 0..5 {
            step(&mut b, 3.5);
        }
        assert!((b.time - (0.4 + 5.0 * dt)).abs() < 1e-5, "{}", b.time);
        // Slowing to 1 m/s: the walk at the run's phase (0.225 of 0.8 s), plus the frame's advance.
        let advance = step(&mut b, 1.0);
        assert_eq!(b.clip, std::sync::Arc::as_ptr(&walk) as usize);
        let want = 0.225 / 0.8 * 1.1 + advance;
        assert!((b.time - want).abs() < 1e-5, "{} {want}", b.time);
        assert_eq!(b.speed, 1.47, "the walk's level speed, for the next change");
    }

    /// gait-jitter review (2026-10-11): through the standard bearer's per-figure step
    /// (`bearer_loop`, `ClipBlend::step`), a bearer without speed levels plays the kit's per-gait
    /// clip at the ground speed over its read root speed and carries his stride into the next
    /// gait's clip by it; read from the in-place clip drawn (0), each gait change restarted it.
    #[test]
    fn a_bearer_without_levels_carries_his_stride_through_the_figure_step() {
        let (walk, walk_slot) = in_place_clip(23);
        let (run, run_slot) = in_place_clip(17);
        let anims = |man: &std::sync::Arc<Anim>, speed: f32| crate::soldiers::GaitAnims { man: man.clone(), speed, mount: None, slots: (String::new(), None), kind: 2 };
        let (walk_anims, run_anims) = (anims(&walk, 1.47), anims(&run, 3.5));
        let slot = |a: &std::sync::Arc<Anim>| Some(if std::sync::Arc::ptr_eq(a, &walk) { walk_slot } else { run_slot });
        let dt = 0.125;
        let mut b = ClipBlend::default();
        let mut frame = 0;
        let mut step = |b: &mut ClipBlend, gait: Gait, speed: f32| {
            frame += 1;
            let shown = b.shown(frame);
            let a = if gait == Gait::Run { &run_anims } else { &walk_anims };
            let BearerLoop { want, advance, carry } = bearer_loop(None, None, || Some(a), gait, b, speed, dt, 0, slot).unwrap();
            b.step(&want, shown, || carry, 0.5, advance, dt, false);
            b.draw(None, None, frame);
            advance
        };
        // Walking at 2 m/s: the walk clip at 2 / 1.47 of its rate, from the offset (0.55 s).
        let advance = step(&mut b, Gait::Walk, 2.0);
        assert!((advance - dt * 2.0 / 1.47).abs() < 1e-5, "{advance}");
        for _ in 0..3 {
            step(&mut b, Gait::Walk, 2.0);
        }
        let t = b.time;
        // Running: the run clip at the walk's phase, plus the frame's advance (3.5 / 3.5 = 1).
        let advance = step(&mut b, Gait::Run, 3.5);
        assert_eq!(b.clip, std::sync::Arc::as_ptr(&run) as usize);
        let want = t.rem_euclid(1.1) / 1.1 * 0.8 + advance;
        assert!((b.time - want).abs() < 1e-5, "{} {want}", b.time);
    }

    /// Review round 2 (gait-blend2): a step up waits for a wrap only while its from-loop is the
    /// clip shown (`0x00773C83`); with an action loop shown it changes clip at once, and under an
    /// action loop the ladder steps between its loops without the transitions (PROVISIONAL).
    #[test]
    fn the_ladder_waits_for_a_wrap_only_when_its_loop_is_shown() {
        use crate::soldiers::{KitLevel, LadderRung};
        let anim = |duration: f32| std::sync::Arc::new(ntw_formats::anim::Anim { frame_rate: 20.0, duration, bones: vec![], frames: vec![], events: vec![] });
        let level = |speed: f32, len: f32| KitLevel { man: vec![anim(len)], speeds: vec![speed], blend_in: vec![0.5], mount: vec![anim(len)], mount_speeds: vec![speed], mount_blend_in: vec![0.5], kind: 2 };
        let ladder = vec![
            LadderRung { into: Some(level(0.6, 3.0)), gait: level(1.47, 1.1) },
            LadderRung { into: Some(level(2.69, 2.7)), gait: level(3.533, 0.8) },
        ];
        let len = |a: &std::sync::Arc<ntw_formats::anim::Anim>| a.duration;
        let dt = 0.1;
        let ready = KitLevel { kind: 0, ..level(0.0, 2.0) };
        // On the walk at 3 m/s with a 2 s action clip shown 0.3 s in (no wrap this frame).
        let action = ClipBlend { rung: Rung::Loop(0), clip: 7, time: 0.3, man: ClipSlot { base: 0, frames: 40, bones: 1, rate: 20.0 }, ..Default::default() };
        // Still under the action loop: straight to the trot loop, no transition, phase carried.
        let mut b = action;
        let (lv, _, start) = on_ladder(&ladder, &mut b, 3.0, dt, Some(&ready), 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, &ladder[1].gait) && start.is_none() && b.rung == Rung::Loop(1));
        // A transition playing when the action loop starts: on to its loop, phase carried.
        let mut b = ClipBlend { rung: Rung::Into(1), ..action };
        let (lv, _, start) = on_ladder(&ladder, &mut b, 3.0, dt, Some(&ready), 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, &ladder[1].gait) && start.is_none() && b.rung == Rung::Loop(1));
        // The action loop just ended: WALK_TO_TROT at once, as a usual change (from 0, as it
        // does not loop; cross-fade), without waiting for the action clip's or the walk's wrap.
        let mut b = action;
        let (lv, adv, start) = on_ladder(&ladder, &mut b, 3.0, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, ladder[1].into.as_ref().unwrap()) && start.is_none() && b.rung == Rung::Into(1));
        assert_eq!(adv, dt * unit_animation::transition_rate(3.0, 1.47, 3.533, 0.0));
        // The same frame with the walk shown waits for its wrap.
        let mut b = ClipBlend { clip: std::sync::Arc::as_ptr(&ladder[0].gait.man[0]) as usize, man: ClipSlot { base: 0, frames: 23, bones: 1, rate: 20.0 }, ..action };
        let (lv, _, start) = on_ladder(&ladder, &mut b, 3.0, dt, None, 0, len).unwrap().unwrap();
        assert!(std::ptr::eq(lv, &ladder[0].gait) && start.is_none() && b.rung == Rung::Loop(0));
    }

    /// Review round 3: a mount clip changing on its own also changes the pair's root speed, so the
    /// man's next change carries the phase when the horse moves (the horse walking on, then the
    /// man's clip changing) and not when it stopped.
    #[test]
    fn a_mount_change_alone_updates_the_pairs_speed() {
        let stand = ClipSlot { base: 500, frames: 21, bones: 1, rate: 20.0 };
        let walk = ClipSlot { base: 900, ..stand };
        // The man on A throughout; the horse on `first` for 10 frames, then `then` (moving at
        // `speed`) for one; then the man changes to B.
        let time_after = |first: ClipSlot, first_speed: f32, then: ClipSlot, speed: f32| {
            let mut b = Fig::default();
            for _ in 0..10 {
                b.run(&ride(first, clip(A, 0.5)), first_speed, 0.125);
            }
            b.run(&ride(then, clip(A, 0.5)), speed, 0.125);
            assert_eq!(b.speed, speed);
            b.run(&ride(then, clip(B, 0.5)), speed, 0.125);
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
            let half = LoopClip { man: ClipSlot { frames: 11, ..clip(B, 0.5).man }, ..clip(B, 0.5) };
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
        assert_eq!(b.fades[0].frozen[0].keep, 0.0);
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
        assert_eq!(b.fades[0].frozen[0].keep, 1.0);
        let shown = b.next();
        b.step(&clip(C, 0.5), shown, || WALK, 0.0, 0.125, 0.125, true);
        assert_eq!(b.b.draw(Some([7, 8, 0, 0]), None, b.frame)[0], [7, 8, 0, 0]);
        assert_eq!((b.clip, b.fades[0].frozen[0].keep), (C, 0.0));
        b.run(&clip(C, 0.5), WALK, 0.125);
        assert_eq!(b.fades[0].frozen[0].keep, 0.0);
    }

    /// The GPU side: a fading figure's slot points at its fade entry (1 + index), the others hold
    /// 0; and the pole bone is posed as the shader does: now + (frozen - now) x weight, per pose.
    #[test]
    fn fades_are_numbered_and_posed_like_the_shader() {
        let mut fades = Vec::new();
        let mut slots = [[9u32; 4]; 2];
        let fade = skin::fade([[1, 1, 0, 0], [2, 2, 0, 0], [0; 4], [0; 4]], [0.25, 0.5, 0.0, 0.0]);
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

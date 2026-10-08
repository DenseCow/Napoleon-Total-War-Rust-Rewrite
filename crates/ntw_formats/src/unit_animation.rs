//! Which figures a unit shows, what they ride and which clips they play, from the DB.
//!
//! Everything here follows keys the original data uses (`analysis/units/CAVALRY.md`), so a
//! mod pack that adds units, personalities, mounts or animation tables works unchanged:
//! ```text
//! unit_stats_land #9  man_animation_type  -> animation table (rider_sabre, man_musket ...)
//!                 #8  man_entity          -> battle_entities (walk / run speed of a man)
//!                 #4/5/6 officer / musician / standard bearer -> battle_personalities
//!                        -> (animation table, equipment theme) of that command figure
//!                 #10 equipment theme of the rank and file
//!                 #14 mount     -> mount_variants -> horse model (see `mount`)
//!                 #15 mount_entity -> battle_entities (horse_heavy: walk 2.6, run 10 m/s)
//!                 #16 mount_type   -> the mount's animation table (mount_horse)
//! animation table -> fragments -> slot -> clips (see `battle_animation`)
//! ```
//! Slots (engine vocabulary, the same in every fragment file):
//! - men on foot: `STAND_TRAINED` / `STAND`, `WALK_TRAINED_n` / `WALK_n`,
//!   `RUN_TRAINED_n` / `RUN_n` (n = speed level; the clips move at 1.00, 1.27, 1.73 ...
//!   m/s, matching their file names);
//! - mounts: `STAND`, `WALK_n`, `TROT`, `CANTER`, `GALLOP`;
//! - riders: the mount's slot with a `RIDER_` prefix (`RIDER_TROT`), without the speed
//!   number when the rider fragment has none (`WALK_1` -> `RIDER_WALK`). INFERRED from the
//!   names; the paired files (`Horse_Trot.anim` / `H_Rider_Trot.anim`) have the same frame
//!   count, duration and root motion (CONFIRMED for stand, walk, trot, canter and gallop),
//!   and the rider clip is authored in the horse's space (his hips sit at 1.9 m over the
//!   saddle), so the rider shares the horse's origin and needs no attachment bone.
//!
//! From the exe (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §1):
//! - the slot vocabulary is a fixed engine table of 864 slots (CONFIRMED), including which
//!   mount slot each `RIDER_*` slot plays with ([`rider_slots`]);
//! - a slot keeps up to 5 alternative clips; a figure plays `selection % count`
//!   ([`alternative`], CONFIRMED) with its own selection number ([`SelectionRng`],
//!   CONFIRMED arithmetic, PROVISIONAL seed);
//! - a speed level is the one whose clip root speed is closest to the figure's speed and
//!   plays at `speed / clip speed` ([`pick_level`]; CONFIRMED for the moving-death and
//!   mounted-attack families, INFERRED for walk/run).
//!
//! Choices still PROVISIONAL (the exe's logic is UNKNOWN):
//! - the `_TRAINED` slots are preferred when the table has them;
//! - a figure is mounted when the unit has mounts (`num_mounts > 0` and a `mount` key)
//!   and its animation table names a `mount_table` (standard bearers have none).

use crate::battle_animation::{AnimationTables, ResolvedClip};
use crate::unit_model::{BattleTables, VariantRole};

/// The animation-related `unit_stats_land` columns of one unit (plain data; the caller
/// fills it from its DB layer).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitAnimationKeys {
    /// #9 `man_animation_type`: the men's animation table.
    pub man_animation_type: String,
    /// #8 `man_entity` (battle_entities).
    pub man_entity: String,
    /// #10 equipment theme of the rank and file.
    pub weapon_theme: Option<String>,
    /// #4..#6 battle_personalities keys.
    pub officer: Option<String>,
    pub musician: Option<String>,
    pub standard_bearer: Option<String>,
    /// #2 number of mounts.
    pub num_mounts: i32,
    /// #14 `mounts` key.
    pub mount: Option<String>,
    /// #15 mount entity (battle_entities).
    pub mount_entity: Option<String>,
    /// #16 the mount's animation table.
    pub mount_type: Option<String>,
}

impl UnitAnimationKeys {
    /// INFERRED: units with mounts ride (cavalry, generals); artillery has a `mount` for
    /// its teams but no mounts of its own.
    pub fn is_mounted(&self) -> bool {
        self.num_mounts > 0 && self.mount.as_deref().is_some_and(|m| !m.is_empty())
    }

    /// The personality key of a command figure.
    pub fn personality(&self, role: VariantRole) -> Option<&str> {
        match role {
            VariantRole::Soldier => None,
            VariantRole::Officer => self.officer.as_deref(),
            VariantRole::Musician => self.musician.as_deref(),
            VariantRole::StandardBearer => self.standard_bearer.as_deref(),
        }
        .filter(|s| !s.is_empty())
    }
}

/// What one figure of a unit is: its tables, equipment and mount.
#[derive(Debug, Clone, PartialEq)]
pub struct FigurePlan {
    pub role: VariantRole,
    /// The battle_personalities key for command figures.
    pub personality: Option<String>,
    pub animation_table: String,
    pub equipment_theme: Option<String>,
    /// battle_entities key of the man (speeds when on foot).
    pub entity: String,
    pub mount: Option<MountPlan>,
}

/// The mount of a figure.
#[derive(Debug, Clone, PartialEq)]
pub struct MountPlan {
    /// `mounts` key (`horse_hussar_mixed`).
    pub mount: String,
    /// The mount's animation table (`mount_horse`).
    pub animation_table: String,
    /// battle_entities key (`horse_light`).
    pub entity: Option<String>,
}

/// Plans one figure of a unit, or `None` when the role has no personality / table.
pub fn plan_figure(
    keys: &UnitAnimationKeys,
    role: VariantRole,
    battle: &BattleTables,
    tables: &AnimationTables,
) -> Option<FigurePlan> {
    let (personality, animation_table, equipment_theme) = match role {
        VariantRole::Soldier => (None, keys.man_animation_type.clone(), keys.weapon_theme.clone()),
        _ => {
            let key = keys.personality(role)?;
            let p = battle.personality(key)?;
            (Some(p.key.clone()), p.animation_table.clone(), Some(p.equipment_theme.clone()).filter(|s| !s.is_empty()))
        }
    };
    let table = tables.table(&animation_table)?;
    let mount = if keys.is_mounted() {
        // The figure's own table must allow riding; its mount_table, else the unit's.
        table.mount_table.clone().map(|own| MountPlan {
            mount: keys.mount.clone().unwrap_or_default(),
            animation_table: keys.mount_type.clone().filter(|t| tables.table(t).is_some()).unwrap_or(own),
            entity: keys.mount_entity.clone(),
        })
    } else {
        None
    };
    Some(FigurePlan { role, personality, animation_table, equipment_theme, entity: keys.man_entity.clone(), mount })
}

/// A movement state with its own clip family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gait {
    Stand,
    Walk,
    Run,
}

impl Gait {
    pub const ALL: [Gait; 3] = [Gait::Stand, Gait::Walk, Gait::Run];
}

/// One chosen clip.
#[derive(Debug, Clone, PartialEq)]
pub struct ChosenClip {
    pub slot: String,
    pub fragment: String,
    /// Clip path as written in the fragment (the Vfs normalises it).
    pub path: String,
    /// The fragment's `default_equipment_display`.
    pub equipment_display: Vec<String>,
    /// Root speed (m/s) of the slot's first clip.
    pub speed: f32,
}

impl ChosenClip {
    fn from_resolved(r: &ResolvedClip, speed: f32) -> Self {
        Self {
            slot: r.slot.clone(),
            fragment: r.fragment.clone(),
            path: r.clip.filename.clone(),
            equipment_display: r.equipment_display.clone(),
            speed,
        }
    }
}

/// The clips a figure plays in one gait: the man's (or rider's) and, when mounted, the
/// mount's paired clip.
#[derive(Debug, Clone, PartialEq)]
pub struct GaitClips {
    pub man: ChosenClip,
    pub mount: Option<ChosenClip>,
}

/// Speed levels tried per family (`WALK_1` .. `WALK_9`).
const LEVELS: std::ops::RangeInclusive<u32> = 1..=9;

/// Slot families for a man on foot, most preferred first.
fn foot_families(gait: Gait) -> Vec<Vec<String>> {
    let levels = |stem: &str| LEVELS.map(|n| format!("{stem}_{n}")).collect::<Vec<_>>();
    match gait {
        Gait::Stand => vec![vec!["STAND_TRAINED".into()], vec!["STAND".into()]],
        Gait::Walk => vec![levels("WALK_TRAINED"), levels("WALK")],
        Gait::Run => vec![levels("RUN_TRAINED"), levels("RUN")],
    }
}

/// Slot families for a mount.
fn mount_families(gait: Gait) -> Vec<Vec<String>> {
    let levels = |stem: &str| LEVELS.map(|n| format!("{stem}_{n}")).collect::<Vec<_>>();
    match gait {
        Gait::Stand => vec![vec!["STAND".into()]],
        Gait::Walk => vec![levels("WALK")],
        Gait::Run => {
            let mut v: Vec<String> = ["TROT", "CANTER", "GALLOP"].map(String::from).to_vec();
            v.extend(levels("RUN"));
            vec![v]
        }
    }
}

/// The rider slots tried for a mount slot. CONFIRMED from the exe's slot table
/// (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §1.1): every `RIDER_<x>` slot names
/// the mount slot it plays with, and that is `<x>` itself except for the two numbered
/// locomotion families: `RIDER_WALK` pairs with `WALK_1` and `RIDER_RUN` with `RUN_1`
/// only (the mount's `WALK_2..5` / `RUN_2..5` have no rider clip).
pub fn rider_slots(mount_slot: &str) -> Vec<String> {
    let mut out = vec![format!("RIDER_{mount_slot}")];
    match mount_slot {
        "WALK_1" => out.push("RIDER_WALK".into()),
        "RUN_1" => out.push("RIDER_RUN".into()),
        _ => {}
    }
    out
}

/// Which alternative clip of a slot a figure plays. CONFIRMED (`0x00E5F760`): a slot holds
/// up to 5 clips (the fragment lines that repeat its name, in file order) and the engine
/// returns clip `selection % count`, where `selection` is the figure's own number (see
/// [`SelectionRng`]). The same number serves every slot, so a man who plays the 3rd
/// `STAND` clip of 5 also plays the 3rd `WALK_TRAINED_2` clip of 5.
pub fn alternative(selection: u32, count: usize) -> usize {
    if count == 0 { 0 } else { selection as usize % count }
}

/// The per-figure selection numbers. CONFIRMED arithmetic (`0x0061CB90`, soldier entity
/// set-up): each new soldier entity steps the battle's shared generator
/// `s = s * 0x343FD + 0x269EC3` (wrapping 32-bit) and keeps `s >> 16` (0..=65535).
/// The generator (the battle object's `+0x50`) is seeded at battle set-up from the clock
/// (`timeGetTime`, or the `constant_random_seed` option; CONFIRMED `0x004847F0` → `0x00511210`),
/// so the original's picks differ from battle to battle; we seed from the battle's own seed. Which
/// draws come before the men are made is UNKNOWN (PROVISIONAL state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRng(pub u32);

impl SelectionRng {
    /// The next figure's selection number.
    pub fn next_selection(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        self.0 >> 16
    }
}

/// One speed level of a gait: every alternative clip of its slot(s) and the root speed of
/// the first one.
#[derive(Debug, Clone, PartialEq)]
pub struct GaitLevel {
    /// The man's (or rider's) alternatives, in fragment order.
    pub man: Vec<ChosenClip>,
    /// The mount's alternatives when mounted.
    pub mount: Option<Vec<ChosenClip>>,
    /// Root speed (m/s) of the level's first clip (the mount's when mounted).
    pub speed: f32,
}

/// Every speed level of `gait` for a figure, slowest first: `STAND` has one level; walk
/// and run have the levels of the first family with any resolving slot (`WALK_TRAINED_1..5`
/// before `WALK_1..5`, as in [`choose_clips`]). A mount level is kept only when its rider
/// slot resolves too.
pub fn gait_levels(
    tables: &AnimationTables,
    plan: &FigurePlan,
    gait: Gait,
    speed_of: &mut dyn FnMut(&str) -> Option<f32>,
) -> Vec<GaitLevel> {
    let chosen = |r: &[ResolvedClip], speed: f32| r.iter().map(|c| ChosenClip::from_resolved(c, speed)).collect::<Vec<_>>();
    let mut levels = Vec::new();
    match &plan.mount {
        None => {
            for family in foot_families(gait) {
                for slot in &family {
                    let r = tables.resolve(&plan.animation_table, slot);
                    let Some(first) = r.first() else { continue };
                    let Some(speed) = speed_of(&first.clip.filename) else { continue };
                    levels.push(GaitLevel { man: chosen(&r, speed), mount: None, speed });
                }
                if !levels.is_empty() {
                    break;
                }
            }
        }
        Some(m) => {
            for family in mount_families(gait) {
                for slot in &family {
                    let r = tables.resolve(&m.animation_table, slot);
                    let Some(first) = r.first() else { continue };
                    let Some(speed) = speed_of(&first.clip.filename) else { continue };
                    let Some(rider) = rider_slots(slot)
                        .iter()
                        .map(|s| tables.resolve(&plan.animation_table, s))
                        .find(|v| !v.is_empty())
                    else {
                        continue;
                    };
                    levels.push(GaitLevel { man: chosen(&rider, speed), mount: Some(chosen(&r, speed)), speed });
                }
                if !levels.is_empty() {
                    break;
                }
            }
        }
    }
    levels.sort_by(|a, b| a.speed.total_cmp(&b.speed));
    levels
}

/// The level whose root speed is closest to `speed`, and the playback rate that makes the
/// clip cover the ground at `speed`: `speed / level speed`.
///
/// The rule is the engine's for every speed-matched family found so far (CONFIRMED for the
/// moving-death clips `DEATH_MOVING_1..12`, `0x006611E0`, and the mounted attacks while
/// moving, `0x005B7B20`: each clip's root displacement over 0.1 s × 10 is compared with
/// the entity's speed, the closest wins and plays at `speed / clip speed`). For walk/run
/// the same rule is INFERRED: the locomotion code itself was not found. A stand level (or
/// a level with no speed) plays at rate 1.
pub fn pick_level(level_speeds: impl IntoIterator<Item = f32>, speed: f32) -> Option<(usize, f32)> {
    let (i, level) = level_speeds
        .into_iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (a - speed).abs().total_cmp(&(b - speed).abs()))?;
    let rate = if level > 1e-3 && speed > 1e-3 { speed / level } else { 1.0 };
    Some((i, rate))
}

/// Picks, from the first family with any resolving slot, the slot whose first clip's
/// speed is closest to `target` (stand families have one slot). `alt` picks among the
/// slot's alternatives. Returns the resolved alternatives' chosen index too.
fn pick_slot(
    tables: &AnimationTables,
    table: &str,
    families: &[Vec<String>],
    target: f32,
    speed_of: &mut dyn FnMut(&str) -> Option<f32>,
) -> Option<(Vec<ResolvedClip>, f32)> {
    for family in families {
        let mut best: Option<(Vec<ResolvedClip>, f32)> = None;
        for slot in family {
            let r = tables.resolve(table, slot);
            let Some(first) = r.first() else { continue };
            let Some(speed) = speed_of(&first.clip.filename) else { continue };
            if best.as_ref().is_none_or(|(_, s)| (speed - target).abs() < (s - target).abs()) {
                best = Some((r, speed));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Chooses the clips of `plan` for `gait`. `speeds` gives the target ground speed (walk,
/// run) of the man or, when mounted, of the mount (see [`gait_speeds`]). `speed_of` returns
/// a clip's root speed (`Anim::root_speed`), or `None` if the clip cannot be read; `alt`
/// picks among a slot's alternative clips.
pub fn choose_clips(
    tables: &AnimationTables,
    plan: &FigurePlan,
    gait: Gait,
    speeds: (f32, f32),
    alt: usize,
    speed_of: &mut dyn FnMut(&str) -> Option<f32>,
) -> Option<GaitClips> {
    let target = match gait {
        Gait::Stand => 0.0,
        Gait::Walk => speeds.0,
        Gait::Run => speeds.1,
    };
    match &plan.mount {
        None => {
            let (r, speed) = pick_slot(tables, &plan.animation_table, &foot_families(gait), target, speed_of)?;
            Some(GaitClips { man: ChosenClip::from_resolved(&r[alt % r.len()], speed), mount: None })
        }
        Some(m) => {
            let (r, speed) = pick_slot(tables, &m.animation_table, &mount_families(gait), target, speed_of)?;
            let mount_clip = ChosenClip::from_resolved(&r[alt % r.len()], speed);
            let rider = rider_slots(&mount_clip.slot)
                .iter()
                .map(|s| tables.resolve(&plan.animation_table, s))
                .find(|v| !v.is_empty())?;
            let man = ChosenClip::from_resolved(&rider[alt % rider.len()], speed);
            Some(GaitClips { man, mount: Some(mount_clip) })
        }
    }
}

/// Target (walk, run) speeds of a figure from battle_entities: the mount's when mounted,
/// else the man's. Falls back to (0, 0) for unknown entities (any family member is then
/// as good as another; the slowest wins ties).
pub fn gait_speeds(plan: &FigurePlan, battle: &BattleTables) -> (f32, f32) {
    let entity = plan.mount.as_ref().and_then(|m| m.entity.as_deref()).unwrap_or(&plan.entity);
    battle.entity(entity).map_or((0.0, 0.0), |e| (e.walk_speed, e.run_speed))
}


// ---------------------------------------------------------------------------------------------
// Action clips (fire, reload, melee, death, knockdown): engine slot vocabulary from the exe's
// slot table (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §1.1, §1.5).

/// A numbered slot family: `first` .. `first + count - 1` as `<stem>_<n>` (n from 1). The death,
/// knockdown and combat-idle family sizes are the exe's random-family table (`0x01452100`,
/// CONFIRMED); attack and reload counts are the slot table's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotFamily {
    pub stem: &'static str,
    pub count: u32,
}

impl SlotFamily {
    /// The slot names of the family.
    pub fn slots(&self) -> impl Iterator<Item = String> + '_ {
        (1..=self.count).map(move |n| format!("{}_{n}", self.stem))
    }
}

/// Foot death families (`DEATH_*`, CONFIRMED counts).
pub const DEATH_STAND: SlotFamily = SlotFamily { stem: "DEATH_STAND", count: 13 };
pub const DEATH_STAND_TRAINED: SlotFamily = SlotFamily { stem: "DEATH_STAND_TRAINED", count: 10 };
pub const DEATH_WALK: SlotFamily = SlotFamily { stem: "DEATH_WALK", count: 5 };
pub const DEATH_MARCH: SlotFamily = SlotFamily { stem: "DEATH_MARCH", count: 5 };
pub const DEATH_RELOAD: SlotFamily = SlotFamily { stem: "DEATH_RELOAD", count: 5 };
pub const DEATH_POISED: SlotFamily = SlotFamily { stem: "DEATH_POISED", count: 10 };
pub const DEATH_RUN: SlotFamily = SlotFamily { stem: "DEATH_RUN", count: 7 };
pub const DEATH_RUN_TRAINED: SlotFamily = SlotFamily { stem: "DEATH_RUN_TRAINED", count: 5 };
pub const DEATH_CHARGE: SlotFamily = SlotFamily { stem: "DEATH_CHARGE", count: 7 };
pub const DEATH_COMBAT_READY: SlotFamily = SlotFamily { stem: "DEATH_COMBAT_READY", count: 10 };
/// Speed-matched moving deaths (`0x006611E0`: tried first, by root speed).
pub const DEATH_MOVING: SlotFamily = SlotFamily { stem: "DEATH_MOVING", count: 12 };
pub const KNOCKDOWN: SlotFamily = SlotFamily { stem: "KNOCKDOWN", count: 5 };
pub const COMBAT_IDLE: SlotFamily = SlotFamily { stem: "COMBAT_IDLE", count: 10 };
pub const ATTACK: SlotFamily = SlotFamily { stem: "ATTACK", count: 10 };
pub const RELOAD: SlotFamily = SlotFamily { stem: "RELOAD", count: 2 };

/// Single action slots used by the battle view.
pub const COMBAT_READY: &str = "COMBAT_READY";
pub const AIM: &str = "AIM";
pub const FIRE: &str = "FIRE";
pub const FACE_DOWN_GET_UP: &str = "FACE_DOWN_GET_UP";

/// Every foot action slot the battle view may play.
pub fn foot_action_slots() -> Vec<String> {
    let mut v: Vec<String> = [COMBAT_READY, AIM, FIRE, FACE_DOWN_GET_UP].map(String::from).to_vec();
    for f in [
        DEATH_STAND, DEATH_STAND_TRAINED, DEATH_WALK, DEATH_MARCH, DEATH_RELOAD, DEATH_POISED, DEATH_RUN,
        DEATH_RUN_TRAINED, DEATH_CHARGE, DEATH_COMBAT_READY, DEATH_MOVING, KNOCKDOWN, COMBAT_IDLE, ATTACK, RELOAD,
    ] {
        v.extend(f.slots());
    }
    v
}

/// Rider action slots: the rider families that exist in the slot table (`RIDER_DEATH_STAND` 5,
/// `RIDER_DEATH_MOVING` 12, `RIDER_COMBAT_IDLE` 10, `RIDER_ATTACK` 10; CONFIRMED).
pub fn rider_action_slots() -> Vec<String> {
    let mut v = Vec::new();
    for (stem, count) in [("RIDER_DEATH_STAND", 5), ("RIDER_DEATH_MOVING", 12), ("RIDER_COMBAT_IDLE", 10), ("RIDER_ATTACK", 10)] {
        v.extend((1..=count).map(|n| format!("{stem}_{n}")));
    }
    v
}

/// The mount slot a rider action slot plays with (CONFIRMED slot table): `RIDER_x` ↔ `x`, except
/// the rider-only kinds (attacks, blocked attacks, charge attacks, shooting, carried deaths,
/// instruments), which have none: the mount keeps what it plays.
pub fn rider_mount_slot(rider_slot: &str) -> Option<String> {
    let x = rider_slot.strip_prefix("RIDER_")?;
    const NONE: [&str; 9] =
        ["ATTACK_", "CHARGE_ATTACK_", "CARRY_WEAPON", "COMBAT_READY", "SHOOT_READY", "RELOAD", "FIRE_", "DEATH_ATTACHED", "PLAY_INSTRUMENT"];
    if NONE.iter().any(|p| x.starts_with(p)) || x == "CHARGE" {
        return None;
    }
    match x {
        "WALK" => Some("WALK_1".into()),
        "RUN" => Some("RUN_1".into()),
        _ => Some(x.into()),
    }
}

/// A random slot of a family as the exe draws knock-downs (`0x006611E0`, CONFIRMED): step the
/// battle generator and take `min(count · (s >> 16) / 0xFFFF, count − 1)`.
pub fn family_pick(rng: &mut SelectionRng, count: u32) -> u32 {
    let r = rng.next_selection();
    ((count as u64 * r as u64 / 0xFFFF) as u32).min(count.saturating_sub(1))
}

/// What a man was doing when he died, for the death family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathCause {
    Standing,
    Walking,
    Running,
    /// Ready to fire or firing.
    Poised,
    Reloading,
    Melee,
    Charging,
}

/// The death family for a man (foot). The exe picks the family from the soldier's state
/// (`0x0063F070`, not decoded); this mapping follows the family names (INFERRED).
pub fn death_family(cause: DeathCause, trained: bool) -> SlotFamily {
    match cause {
        DeathCause::Standing if trained => DEATH_STAND_TRAINED,
        DeathCause::Standing => DEATH_STAND,
        DeathCause::Walking if trained => DEATH_MARCH,
        DeathCause::Walking => DEATH_WALK,
        DeathCause::Running if trained => DEATH_RUN_TRAINED,
        DeathCause::Running => DEATH_RUN,
        DeathCause::Poised => DEATH_POISED,
        DeathCause::Reloading => DEATH_RELOAD,
        DeathCause::Melee => DEATH_COMBAT_READY,
        DeathCause::Charging => DEATH_CHARGE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLES: &str = "version 1\nanimation_table rider_sabre\n{\n skeleton_type man\n fragment foot default_equipment_display = primary_weapon\n fragment rider default_equipment_display = primary_weapon, secondary_weapon\n mount_table mount_horse\n}\nanimation_table mount_horse\n{\n skeleton_type horse\n fragment horse\n}\nanimation_table man_musket\n{\n skeleton_type man\n fragment foot default_equipment_display = primary_weapon, ambient\n}\n";
    const FOOT: &str = "STAND filename = \"s.anim\"\nSTAND_TRAINED filename = \"st.anim\"\nSTAND_TRAINED filename = \"st2.anim\"\nWALK_TRAINED_1 filename = \"w100.anim\"\nWALK_TRAINED_2 filename = \"w127.anim\"\nWALK_1 filename = \"iw.anim\"\nRUN_TRAINED_1 filename = \"r313.anim\"\nRUN_TRAINED_2 filename = \"r407.anim\"\n";
    const RIDER: &str = "RIDER_STAND filename = \"hr_stand.anim\"\nRIDER_WALK filename = \"hr_walk.anim\"\nRIDER_TROT filename = \"hr_trot.anim\"\nRIDER_GALLOP filename = \"hr_gallop.anim\"\n";
    const HORSE: &str = "STAND filename = \"h_stand.anim\"\nWALK_1 filename = \"h_walk.anim\"\nTROT filename = \"h_trot.anim\"\nGALLOP filename = \"h_gallop.anim\"\n";

    fn tables() -> AnimationTables {
        AnimationTables::from_text(TABLES, |n| match n {
            "foot" => Some(FOOT.into()),
            "rider" => Some(RIDER.into()),
            "horse" => Some(HORSE.into()),
            _ => None,
        })
    }

    fn speed(path: &str) -> Option<f32> {
        Some(match path {
            "w100.anim" => 1.0,
            "w127.anim" => 1.27,
            "r313.anim" => 3.13,
            "r407.anim" => 4.07,
            "h_walk.anim" => 1.47,
            "h_trot.anim" => 3.53,
            "h_gallop.anim" => 10.19,
            _ => 0.0,
        })
    }

    fn keys(mounted: bool) -> UnitAnimationKeys {
        UnitAnimationKeys {
            man_animation_type: if mounted { "rider_sabre" } else { "man_musket" }.into(),
            man_entity: "man".into(),
            num_mounts: if mounted { 60 } else { 0 },
            mount: mounted.then(|| "horse_x".into()),
            mount_entity: mounted.then(|| "horse_heavy".into()),
            mount_type: mounted.then(|| "mount_horse".into()),
            ..Default::default()
        }
    }

    #[test]
    fn foot_soldier_prefers_trained_and_matches_speed() {
        let t = tables();
        let plan = plan_figure(&keys(false), VariantRole::Soldier, &BattleTables::default(), &t).unwrap();
        assert!(plan.mount.is_none());
        let stand = choose_clips(&t, &plan, Gait::Stand, (1.4, 3.6), 1, &mut speed).unwrap();
        assert_eq!(stand.man.path, "st2.anim");
        assert_eq!(stand.man.equipment_display, vec!["primary_weapon", "ambient"]);
        let walk = choose_clips(&t, &plan, Gait::Walk, (1.4, 3.6), 0, &mut speed).unwrap();
        assert_eq!(walk.man.slot, "WALK_TRAINED_2");
        let run = choose_clips(&t, &plan, Gait::Run, (1.4, 3.9), 0, &mut speed).unwrap();
        assert_eq!(run.man.slot, "RUN_TRAINED_2");
    }

    #[test]
    fn rider_pairs_with_mount_slot() {
        let t = tables();
        let plan = plan_figure(&keys(true), VariantRole::Soldier, &BattleTables::default(), &t).unwrap();
        assert_eq!(plan.mount.as_ref().unwrap().animation_table, "mount_horse");
        let walk = choose_clips(&t, &plan, Gait::Walk, (2.6, 10.0), 0, &mut speed).unwrap();
        assert_eq!(walk.mount.as_ref().unwrap().slot, "WALK_1");
        assert_eq!(walk.man.slot, "RIDER_WALK");
        assert_eq!(walk.man.equipment_display, vec!["primary_weapon", "secondary_weapon"]);
        let run = choose_clips(&t, &plan, Gait::Run, (2.6, 10.0), 0, &mut speed).unwrap();
        assert_eq!(run.mount.as_ref().unwrap().path, "h_gallop.anim");
        assert_eq!(run.man.path, "hr_gallop.anim");
        assert_eq!(rider_slots("WALK_1"), vec!["RIDER_WALK_1", "RIDER_WALK"]);
        assert_eq!(rider_slots("RUN_1"), vec!["RIDER_RUN_1", "RIDER_RUN"]);
        // The exe pairs RIDER_WALK with WALK_1 only.
        assert_eq!(rider_slots("WALK_2"), vec!["RIDER_WALK_2"]);
        assert_eq!(rider_slots("STAND"), vec!["RIDER_STAND"]);
    }

    #[test]
    fn selection_numbers_follow_the_engine_generator() {
        // s = s * 0x343FD + 0x269EC3, keep s >> 16 (the MSVC rand() step without its mask).
        let mut rng = SelectionRng(0);
        assert_eq!(rng.next_selection(), 0x269EC3 >> 16);
        assert_eq!(rng.0, 0x269EC3);
        let s1 = 0x269EC3u32.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        assert_eq!(rng.next_selection(), s1 >> 16);
        assert_eq!(alternative(7, 5), 2);
        assert_eq!(alternative(7, 1), 0);
        assert_eq!(alternative(7, 0), 0);
    }

    #[test]
    fn levels_sorted_and_picked_by_closest_speed() {
        let t = tables();
        let plan = plan_figure(&keys(false), VariantRole::Soldier, &BattleTables::default(), &t).unwrap();
        let walk = gait_levels(&t, &plan, Gait::Walk, &mut speed);
        assert_eq!(walk.iter().map(|l| l.man[0].slot.as_str()).collect::<Vec<_>>(), ["WALK_TRAINED_1", "WALK_TRAINED_2"]);
        let (i, rate) = pick_level(walk.iter().map(|l| l.speed), 1.4).unwrap();
        assert_eq!(i, 1);
        assert!((rate - 1.4 / 1.27).abs() < 1e-5);
        let stand = gait_levels(&t, &plan, Gait::Stand, &mut speed);
        assert_eq!(stand.len(), 1);
        assert_eq!(stand[0].man.len(), 2, "both STAND_TRAINED alternatives kept");
        assert_eq!(pick_level(stand.iter().map(|l| l.speed), 0.0).unwrap(), (0, 1.0));
        // Mounted: each mount level carries its paired rider slot.
        let mplan = plan_figure(&keys(true), VariantRole::Soldier, &BattleTables::default(), &t).unwrap();
        let mrun = gait_levels(&t, &mplan, Gait::Run, &mut speed);
        let slots: Vec<_> = mrun.iter().map(|l| (l.mount.as_ref().unwrap()[0].slot.clone(), l.man[0].slot.clone())).collect();
        assert_eq!(slots, [("TROT".to_string(), "RIDER_TROT".to_string()), ("GALLOP".into(), "RIDER_GALLOP".into())]);
    }

    #[test]
    fn unmounted_without_mounts() {
        let mut k = keys(true);
        k.num_mounts = 0;
        let t = tables();
        assert!(plan_figure(&k, VariantRole::Soldier, &BattleTables::default(), &t).unwrap().mount.is_none());
        assert!(plan_figure(&k, VariantRole::Officer, &BattleTables::default(), &t).is_none());
    }

    #[test]
    fn action_vocabulary() {
        assert_eq!(rider_mount_slot("RIDER_DEATH_STAND_2").as_deref(), Some("DEATH_STAND_2"));
        assert_eq!(rider_mount_slot("RIDER_COMBAT_IDLE_3").as_deref(), Some("COMBAT_IDLE_3"));
        assert_eq!(rider_mount_slot("RIDER_ATTACK_4"), None);
        assert_eq!(rider_mount_slot("RIDER_WALK").as_deref(), Some("WALK_1"));
        assert_eq!(rider_mount_slot("STAND"), None);
        let slots = foot_action_slots();
        assert!(slots.contains(&"DEATH_STAND_13".to_string()) && !slots.contains(&"DEATH_STAND_14".to_string()));
        assert!(slots.contains(&"RELOAD_2".to_string()) && slots.contains(&"FIRE".to_string()));
        assert_eq!(death_family(DeathCause::Standing, true), DEATH_STAND_TRAINED);
        assert_eq!(death_family(DeathCause::Melee, false), DEATH_COMBAT_READY);
        // The knock-down pick stays inside the family.
        let mut rng = SelectionRng(1);
        for _ in 0..1000 {
            assert!(family_pick(&mut rng, 5) < 5);
        }
        let mut top = SelectionRng(0);
        top.0 = u32::MAX; // any state: the formula is min(count * r / 0xFFFF, count - 1)
        assert!(family_pick(&mut top, 5) <= 4);
    }
}

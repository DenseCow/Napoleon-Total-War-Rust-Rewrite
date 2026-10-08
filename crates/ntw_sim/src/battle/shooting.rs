//! Missile combat in the battle model: picking a target, reloading, firing volleys and resolving
//! every shot with the formulas of [`super::missile`] (W1 §12.6 and §12.10).
//!
//! # How a volley works here (plain words)
//! Each missile unit (muskets, rifles, cannon) has a [`MissileWeapon`] built from its real
//! `unit_stats_land` and `projectiles` rows. Every 0.1 s tick, in its own update slot, a unit:
//! 1. counts its reload timer down;
//! 2. picks a target: the enemy the player ordered it to shoot, or (when "fire at will" is on)
//!    the nearest fighting enemy within range;
//! 3. if it is loaded, has ammunition, is standing still and is not in melee, it fires one volley.
//!
//! A volley is a number of shots. For every shot:
//! - the **chance to hit** comes from [`missile::chance_to_hit`] (W1 §12.10, CONFIRMED formula);
//! - one RNG roll decides whether the shot strikes a man (APPROXIMATION, see below);
//! - a struck man is resolved by [`missile::impact`] (W1 §12.10, CONFIRMED formula and RNG use):
//!   KILL removes a man, KNOCKDOWN has no effect yet (no soldier animations), MISS nothing.
//!
//! # What is faithful and what is not
//! - CONFIRMED: [`missile::range`], [`missile::accuracy`], [`missile::chance_to_hit`],
//!   [`missile::impact`] (including its single 0..=100 roll from the battle RNG, and no roll at all
//!   for damage >= 1).
//! - APPROXIMATION: the original flies every bullet through the aim-dispersion ellipse
//!   ([`missile::dispersion`]) and checks what it really strikes. This model has no individual
//!   soldiers, so instead each shot strikes a man when one `unit_float()` roll is `<=` the chance
//!   to hit. That is one extra roll per shot that the original does not make in this form.
//! - CONFIRMED: the reload time ([`reload_time_seconds`], `0x00639E40`, every skill term in
//!   [`ReloadContext`]). The original computes it per soldier when he fires (`0x006FB740`).
//! - CONFIRMED (static, BATTLE_FIDELITY.md §52): which men fire is decided by the unit's fire order,
//!   one class per firing drill ([`volley_plan`]). With the default drill only the front rank fires;
//!   skirmishers and mounted units fire with every man. The model turns the per-soldier firing into
//!   volleys of the same men per reload cycle (APPROXIMATION of the timing, see [`volley_plan`]).
//! - PLACEHOLDER: how long a unit counts as "under fire" for morale, and target selection. Each
//!   constant below says so.
//! - INFERRED: the weapon range (`missile weapon +0x60` in W1 §12.6) is the projectile's
//!   `effective_range` (projectiles column 11). They are the same number for every musket we checked
//!   (80 m) but the link itself has not been confirmed in the exe.
//! - PROVISIONAL tick order: W1 §12.2 only confirms the 0.1 s tick and the morale stagger. Where the
//!   missile update sits inside the per-unit update (`0x0057F070`) is UNKNOWN; we run it right
//!   after melee (slot 4), so casualties from a volley count towards the target's next morale check.

use super::missile::{self, AccuracyMode, ChanceToHitInputs, ImpactInputs, ImpactOutcome};
use super::model::{Battle, LandUnit, dist2};

/// The fire-order groups of the platoon drills (`0x0051B120` / `0x0051B180` / `0x0051B090` pass 3 to the
/// drill base `0x0051A040`, field `+0x38`) and the cap on rank fire's groups (`0x0051B240`: ranks, at
/// most 3). CONFIRMED.
pub const DRILL_GROUPS: u32 = 3;

/// Who fires in one volley of a unit's men, and how many volleys share one reload cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VolleyPlan {
    /// Men who fire in one volley.
    pub shooters: u32,
    /// Volleys per reload cycle: the reload time is split between them (1 = one volley per reload).
    pub groups: u32,
}

/// The firing drill a unit's fire order uses (`0x00554920`, CONFIRMED): the card's drill (`+0x78`),
/// except 0 when the unit rides (`0x0055C2A0`) or is in a building (PROVISIONAL: the garrison's order
/// class `DEFEND_AND_FIRE_STATE` was not decoded). The same value is the order's vfunc `+0x30` that
/// the reload formula's drill switch reads (`0x005498B0`; the six order classes return 0..5, CONFIRMED).
pub fn effective_drill(u: &LandUnit) -> u8 {
    if super::strength::mounted(u) || u.garrison.is_some() { 0 } else { u.capabilities.firing_drill }
}

impl LandUnit {
    /// Men in one rank ([`LandUnit::formation_files`]); unknown → the start men over
    /// [`super::abilities::FORMATION_RANKS`] (PROVISIONAL).
    pub fn files(&self) -> u32 {
        if self.formation_files > 0 {
            self.formation_files
        } else {
            ((self.max_men.max(1) as f32 / super::abilities::FORMATION_RANKS).ceil() as u32).max(1)
        }
    }
}

/// Which men of a musket unit fire (CONFIRMED static, BATTLE_FIDELITY.md §52). The missile-attack
/// state (`0x00553C30`) builds one fire-order object per drill ([`effective_drill`]):
/// - **0 `fire_volley`** (vtable `0x013239F0`, fire step `0x0055EB30`): the candidates are the first
///   `n` soldiers of the unit (`0x00539910`), where `n` (`0x005694D0`) is every man when the unit may
///   skirmish (unit `+0x1A5`, column 53) or rides (`0x0055ABF0`), else the formation's files (shape
///   vfunc `+0x10`). Soldiers fill the formation slots in list order, slot `i` in rank `i / files`
///   (`0x00619E60`, `0x00581200`), so `n` = the front rank. Each loaded candidate (`0x0063E290`) is
///   paired with a target soldier and fires as soon as `max(1, round(0.025 × men))` of them are loaded,
///   after an aim hold drawn in [0, 0.5) s.
/// - **1 `mass_fire`** (vtable `0x013253E0`, states RELOAD → FIRE): every man reloads, then every man
///   fires (`0x0056C4C0`, aim hold in [0, 0.4) s).
/// - **2–4 platoon fires** (`0x0051B120`/`0x0051B180`/`0x0051B090`): the men are split into
///   [`DRILL_GROUPS`] groups (dispersed: soldier `i` in group `i mod 3`, `0x0054FF00`); the groups
///   fire in turn, each once it is loaded (states WAIT_FOR_GROUP_ALL_LOADED_AND_READY,
///   ORDER_GROUP_FIRE, ADVANCE_GROUP).
/// - **5 `rank_fire`** (`0x0051B240`): the groups are the ranks (`0x0054FF50`, shape vfunc `+0x2C`),
///   at most [`DRILL_GROUPS`]; men in later ranks never fire.
///
/// Every pair also needs its target within the arc of the order's facing (`0x005869A0`: half of
/// `battle_entities` column 17, 70° for infantry, `0x00E52F40`). The order faces the target, so at
/// unit level the arc never excludes anyone (not modelled).
///
/// APPROXIMATION (timing): the original fires per soldier as each one is loaded; the model fires the
/// same men once per reload cycle, and a grouped drill fires one group every `reload / groups`
/// (the steady state when reloading is slower than the other groups' turns). Mounted units use their
/// own issuer (`0x005849E0`), which also takes every man.
pub fn volley_plan(u: &LandUnit) -> VolleyPlan {
    let men = u.men;
    let all = VolleyPlan { shooters: men, groups: 1 };
    if super::strength::mounted(u) || u.attributes.skirmisher {
        return all;
    }
    let files = u.files().min(men.max(1));
    match effective_drill(u) {
        1 => all,
        2..=4 => VolleyPlan { shooters: men.div_ceil(DRILL_GROUPS), groups: DRILL_GROUPS },
        5 => {
            let ranks = men.div_ceil(files.max(1)).clamp(1, DRILL_GROUPS);
            VolleyPlan { shooters: men.min(ranks * files).div_ceil(ranks), groups: ranks }
        }
        _ => VolleyPlan { shooters: files.min(men), groups: 1 },
    }
}
/// A unit counts as "under projectile fire" (`+0xC84`, morale effect 0x24) for 60 ticks (6 s) after a hit
/// (`0x00566B90` sets it to 0x3C, CONFIRMED); slot 4 `0x005828A0` counts it down by 1 per tick.
pub const UNDER_FIRE_MEMORY_TICKS: u32 = 60;
/// "Under artillery fire" (`+0xC80`) lasts 300 ticks (30 s) (`0x00566910` sets 300, CONFIRMED).
pub const UNDER_ARTILLERY_FIRE_TICKS: u32 = 300;
/// PLACEHOLDER: units with ammunition that advance on their own stop at this fraction of their range.
pub const MISSILE_ADVANCE_STOP_FRACTION: f32 = 0.9;
/// A neutral chance-to-hit factor: the angle term of a weapon without ballistic data.
pub const NEUTRAL_SHOT_FACTOR: f32 = 1.0;

/// A unit's cartridge pool (round 12, CONFIRMED structure):
/// - created with the unit (`0x0051B7D0`): `+0xE10 = +0xE14 = (men +0x204 − +0x27C) × +0x1A0`,
///   the men times the rounds per man (`unit_stats_land` ammunition; `+0x27C` is a count of men who
///   do not carry rounds, UNKNOWN, taken as 0: PROVISIONAL);
/// - every soldier's GET_AMMO state (`0x00805ED0` → `0x00576480`) takes one round; at 0 the unit
///   reports "out of ammunition" (event 0x1E);
/// - a soldier who dies (`0x00574E90`, from the hit dispatch `0x0080A4E0`) takes his share with him:
///   `ceil(pool / men)` rounds;
/// - the ammunition the UI shows (`+0xE18`) is `pool / men × start men / start pool`, the rounds
///   per man left over the rounds per man at the start.
///
/// So a unit whose men take turns to fire shares one pool. The model's units fire in volleys of
/// several men, each volley taking one round per man who fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AmmoPool {
    /// `+0xE10`: rounds left.
    pub rounds: u32,
    /// `+0xE14`: rounds at the start.
    pub start: u32,
    /// The men the pool was last updated for.
    pub men: u32,
    /// Rounds per man at the start (`unit_stats_land` ammunition).
    pub per_man: u32,
}

impl AmmoPool {
    /// The pool of a unit of `men` men with `per_man` rounds each.
    pub fn new(men: u32, per_man: u32) -> Self {
        let rounds = men.saturating_mul(per_man);
        AmmoPool { rounds, start: rounds, men, per_man }
    }

    /// Takes up to `n` rounds; returns how many were taken.
    pub fn take(&mut self, n: u32) -> u32 {
        let t = n.min(self.rounds);
        self.rounds -= t;
        t
    }

    /// Removes the share of each man lost since the last call (one man at a time, `ceil` in `f32`
    /// as `0x00574E90` rounds). Men gained (none in the model) just raise the count.
    pub fn losses(&mut self, men_now: u32) {
        while self.men > men_now {
            let share = (self.rounds as f32 / self.men as f32).ceil().max(0.0) as u32;
            self.rounds = self.rounds.saturating_sub(share);
            self.men -= 1;
        }
        self.men = men_now;
    }

    /// Rounds per man left, rounded up (the model's `LandUnit::ammunition`, which the HUD and the
    /// scripts show against the starting rounds per man).
    pub fn rounds_per_man(&self) -> u32 {
        if self.men == 0 { 0 } else { self.rounds.div_ceil(self.men) }
    }
}

/// The ground types that put a target "in cover" (`0x00817130`, CONFIRMED indices 2, 14, 16:
/// field_forest, vegetation_dense_forest, vegetation_medium_woodland).
pub fn in_woods(ground_type: u8) -> bool {
    matches!(ground_type, 2 | 14 | 16)
}

/// A unit's missile weapon: the static numbers, read once from the game data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MissileWeapon {
    /// Weapon range in metres (int). INFERRED: `projectiles.effective_range` (col 11).
    pub range: i32,
    /// Base accuracy, `unit_stats_land` col 26 (W1 §12.9 table, CONFIRMED UI label "Accuracy").
    pub accuracy: f32,
    /// Reload skill, `unit_stats_land` col 27 (CONFIRMED UI label "Reloading").
    pub reload_skill: i32,
    /// Weapon reload time in seconds, `projectiles.reload_time` (col 24).
    pub reload_time_s: i32,
    /// Projectile damage, `projectiles.damage` (col 17). `>= 1` kills outright within range.
    pub damage: f32,
    /// Projectiles per shot, `projectiles.projectiles_per_shot` (col 8; canister fires many).
    pub projectiles_per_shot: u32,
    /// Artillery (uses the artillery half-chance distance, W1 §12.10).
    pub is_artillery: bool,
    /// Number of guns (`unit_stats_land` col 3). Artillery fires one shot per gun.
    pub guns: u32,
    /// The projectile's trajectory, muzzle velocity, maximum elevation and accuracy modifier
    /// (the chance-to-hit angle and bonus terms, `missile::launch_angle`). `Default` = no data:
    /// a level shot with no bonus.
    pub ballistics: super::missile::Ballistics,
}

/// The firing drill a soldier's formation object reports (`FUN_005498B0`, values 1..5).
/// CONFIRMED (0x00639E40 jump table at 0x0063A10C): 1 → `firing_drill_mass_fire_reload_modifier`,
/// 2 → `..._platoon_fire_...`, 3 → `..._improved_platoon_fire_...`, 4 → no modifier,
/// 5 → `..._rank_fire_...`. Value 0 (no drill object) adds nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FiringDrill {
    /// No drill object, or drill 4: no reload modifier.
    #[default]
    None,
    /// Drill 1.
    MassFire,
    /// Drill 2.
    PlatoonFire,
    /// Drill 3.
    ImprovedPlatoonFire,
    /// Drill 5.
    RankFire,
}

impl FiringDrill {
    /// The reload case of a drill enum value (the order's vfunc `+0x30`, see [`effective_drill`]).
    pub fn from_value(d: u8) -> Self {
        match d {
            1 => FiringDrill::MassFire,
            2 => FiringDrill::PlatoonFire,
            3 => FiringDrill::ImprovedPlatoonFire,
            5 => FiringDrill::RankFire,
            _ => FiringDrill::None,
        }
    }
}

/// The situation of a shooter that changes its reload skill in `0x00639E40` (CONFIRMED terms).
/// `Default` = a land soldier standing in the open, fresh, clear weather, no drill, no army bonus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ReloadContext {
    /// Soldier fatigue level (entity vfunc `+0xE0`, the 0..5 state): Tired −3, VeryTired −5,
    /// Exhausted −10 (CONFIRMED).
    pub fatigue: super::fatigue::FatigueState,
    /// Battle weather type 0 or 1 (`FUN_005DBB30` / `FUN_005DBD60`, rain or snow: the same two
    /// tests pick `idle_rain` / `idle_snow` in the fatigue code): −6 (CONFIRMED).
    pub rain_or_snow: bool,
    /// Unit state 7 (`FUN_0055C9A0(7)`): adds `fire_and_advance_reload_modifier` (CONFIRMED read).
    /// The same state cuts the missile range to 80 % (`0x005646A0`), so state 7 = fire and advance
    /// (INFERRED).
    pub fire_and_advance: bool,
    /// The soldier stands on walls (`FUN_0056AE70`): `fire_on_walls_reload_modifier` instead of the
    /// drill modifier (CONFIRMED).
    pub on_walls: bool,
    /// The firing drill (only when the soldier has a formation object and is not on walls).
    pub drill: FiringDrill,
    /// The soldier fires from a building that is not walls (entity `+0x10`, vfunc `+0x2C`, object
    /// `+0x231 == 0`): the unit's reload skill is added a second time (CONFIRMED code; meaning
    /// INFERRED).
    pub in_building: bool,
    /// The army "level" of `army+0x224` / `army+0x234` (also gives +4/+8 melee attack, W1 §12.9):
    /// flag clear: level 1 → +15, level 2 → +30; flag set: level 1 → +15 (CONFIRMED values).
    pub army_level_flag: bool,
    /// The army level 0..2 (see `army_level_flag`).
    pub army_level: u8,
    /// Unit field `+0xC28`: 4 → −3, 5 → −7 (CONFIRMED values). INFERRED: `+0xC28` is the morale
    /// state (the morale component sits at `unit+0xC00`; `+0xC28` = its state `[0xA]`), so Shaken
    /// −3 and Wavering −7. `None` = no morale state (e.g. a ship crew).
    pub morale_state: Option<super::morale::MoraleState>,
}

/// The adjusted reload skill of `0x00639E40` before it is turned into a time (CONFIRMED order).
pub fn reload_skill(base_skill: i32, ctx: &ReloadContext, rules: &super::rules::KvRules) -> i32 {
    use super::fatigue::FatigueState;
    let mut s = base_skill;
    match ctx.morale_state {
        Some(super::morale::MoraleState::Shaken) => s -= 3,
        Some(super::morale::MoraleState::Wavering) => s -= 7,
        _ => {}
    }
    if ctx.fire_and_advance {
        s += rules.fire_and_advance_reload_modifier;
    }
    if ctx.on_walls {
        s += rules.fire_on_walls_reload_modifier;
    } else {
        s += match ctx.drill {
            FiringDrill::None => 0,
            FiringDrill::MassFire => rules.firing_drill_mass_fire_reload_modifier,
            FiringDrill::PlatoonFire => rules.firing_drill_platoon_fire_reload_modifier,
            FiringDrill::ImprovedPlatoonFire => {
                rules.firing_drill_improved_platoon_fire_reload_modifier
            }
            FiringDrill::RankFire => rules.firing_drill_rank_fire_reload_modifier,
        };
    }
    if ctx.in_building {
        s += base_skill;
    }
    s += match (ctx.army_level_flag, ctx.army_level) {
        (false, 1) => 15,
        (false, 2) => 30,
        (true, 1) => 15,
        _ => 0,
    };
    s -= match ctx.fatigue {
        FatigueState::Tired => 3,
        FatigueState::VeryTired => 5,
        FatigueState::Exhausted => 10,
        _ => 0,
    };
    if ctx.rain_or_snow {
        s -= 6;
    }
    s
}

/// Reload time in seconds, `0x00639E40` (CONFIRMED from the disassembly, all `f32`):
/// ```text
/// s = adjusted reload skill (reload_skill), then s = max(s, 0)    // `test esi,esi; cmovle`
/// t = max((150 - s) * 0.01, 0.0) * weapon reload time (projectile +0x98, int seconds)
/// return max(t, 1.0)
/// ```
/// So skill 50 reloads in exactly the weapon's time, each skill point is 1 % faster, skill ≥ 150
/// hits the 1 s floor.
pub fn reload_time_seconds(adjusted_skill: i32, weapon_reload_time_s: i32) -> f32 {
    let s = adjusted_skill.max(0);
    let f = ((150i32.wrapping_sub(s)) as f32 * 0.01).max(0.0);
    (f * weapon_reload_time_s as f32).max(1.0)
}

impl MissileWeapon {
    /// Reload time in seconds for a shooter in situation `ctx` ([`reload_time_seconds`]).
    pub fn reload_seconds(&self, ctx: &ReloadContext, rules: &super::rules::KvRules) -> f32 {
        reload_time_seconds(reload_skill(self.reload_skill, ctx, rules), self.reload_time_s)
    }

    /// Reload time in whole 0.1 s ticks: `ceil(seconds * 10)`. INFERRED: the original stores the
    /// time as an `f32` on the soldier (`entity+0x730`) and counts it down per tick; how exactly the
    /// countdown ends is not decompiled, so a timer that is ready when it reaches 0 is assumed.
    pub fn reload_ticks(&self, ctx: &ReloadContext, rules: &super::rules::KvRules) -> u32 {
        let t = self.reload_seconds(ctx, rules) * 10.0;
        (t - 1e-4).ceil().max(1.0) as u32
    }
}

/// One volley, recorded for the display (and for tests). The model never reads these back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VolleyEvent {
    /// Tick the volley was fired on.
    pub tick: u32,
    /// Shooting unit id.
    pub shooter: u32,
    /// Target unit id.
    pub target: u32,
    /// Shots fired (projectiles, after `projectiles_per_shot`).
    pub shots: u32,
    /// Shots that struck a man (before the impact roll).
    pub hits: u32,
    /// Men killed.
    pub kills: u32,
}

impl LandUnit {
    /// The unit's effective missile range in metres ([`missile::range`], CONFIRMED formula).
    /// 0 when the unit has no missile weapon. Not on walls (no wall buildings in the model yet).
    /// The "state 7" of `0x005646A0` is ability 7, fire and advance, being active (`0x0055C9A0(7)`
    /// tests an active ability, BATTLE_FIDELITY.md §32): range × 0.8.
    pub fn missile_range(&self, fire_on_walls_range_modifier: i32) -> f32 {
        missile::range(
            self.missile.is_some(),
            self.missile.map(|w| w.range),
            false,
            self.active_abilities & (1 << super::attributes::ability::FIRE_AND_ADVANCE) != 0,
            fire_on_walls_range_modifier,
        )
    }

    /// True if this unit can still shoot at all (has a weapon and ammunition left).
    pub fn can_shoot(&self) -> bool {
        self.missile.is_some() && self.ammunition > 0
    }
}

impl Battle {
    /// Orders unit `shooter` to fire at unit `target`. Clears any move order (the unit walks into
    /// range by itself). Returns false (and changes nothing) if either id is unknown, the two are on
    /// the same side, or the shooter has no missile weapon.
    pub fn order_fire(&mut self, shooter: u32, target: u32) -> bool {
        let (Some(s), Some(t)) = (self.unit_index(shooter), self.unit_index(target)) else {
            return false;
        };
        if self.units[s].side == self.units[t].side || self.units[s].missile.is_none() {
            return false;
        }
        let u = &mut self.units[s];
        u.fire_target = Some(target);
        u.destination = None;
        true
    }

    /// Orders unit `id` to walk to `dest`, cancelling any fire order. Returns false if `id` is unknown.
    pub fn order_move(&mut self, id: u32, dest: (f32, f32)) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        let u = &mut self.units[i];
        u.destination = Some(dest);
        u.fire_target = None;
        true
    }

    /// The index in `units` of the unit with this id.
    pub fn unit_index(&self, id: u32) -> Option<usize> {
        self.units.binary_search_by_key(&id, |u| u.id).ok()
    }

    /// True if `u` can be shot at by side `side`: an enemy that still has men and is active.
    /// Routing units CAN be shot (they are still on the field).
    fn is_valid_missile_target(u: &LandUnit, side: u8) -> bool {
        u.side != side && u.men > 0 && u.active
    }

    /// PLACEHOLDER: true if enemy `t` is in melee contact with any unit of `side`. We hold fire on
    /// such targets because this model has no friendly fire (the original fires real projectiles
    /// that can hit anyone).
    fn engaged_with_side(&self, t: usize, side: u8) -> bool {
        let r2 = super::model::MELEE_CONTACT_RANGE * super::model::MELEE_CONTACT_RANGE;
        let tp = self.units[t].position;
        self.units
            .iter()
            .any(|u| u.side == side && u.men > 0 && u.active && dist2(u.position, tp) <= r2)
    }

    /// The target unit `idx` would shoot at now (index), if any. PLACEHOLDER selection:
    /// 1. the ordered target, if it is still valid and within range;
    /// 2. otherwise, with fire at will on, the nearest valid enemy within range that is not locked
    ///    in melee with our side (ties go to the lower id).
    pub(super) fn missile_target(&self, idx: usize) -> Option<usize> {
        let u = &self.units[idx];
        let range = u.missile_range(self.kv_rules.fire_on_walls_range_modifier);
        if range <= 0.0 {
            return None;
        }
        let r2 = range * range;
        if let Some(t) = u.fire_target.and_then(|id| self.unit_index(id)) {
            let e = &self.units[t];
            if Self::is_valid_missile_target(e, u.side)
                && dist2(u.position, e.position) <= r2
                && !self.engaged_with_side(t, u.side)
            {
                return Some(t);
            }
            // An ordered target outside range: the unit walks closer instead (see movement).
            return None;
        }
        if !u.fire_at_will {
            return None;
        }
        let mut best: Option<(usize, f32)> = None;
        for (j, e) in self.units.iter().enumerate() {
            if !Self::is_valid_missile_target(e, u.side) || e.is_out_of_fight() {
                continue;
            }
            let d = dist2(u.position, e.position);
            if d > r2 || self.engaged_with_side(j, u.side) {
                continue;
            }
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        best.map(|(j, _)| j)
    }

    /// PLACEHOLDER movement helper: the ordered fire target of unit `idx` (index) if the unit can
    /// still shoot and the target is farther than `MISSILE_ADVANCE_STOP_FRACTION` of its range,
    /// so the unit should walk closer.
    pub(super) fn ordered_fire_target_to_approach(&self, idx: usize) -> Option<usize> {
        let u = &self.units[idx];
        if !u.can_shoot() {
            return None;
        }
        let t = self.unit_index(u.fire_target?)?;
        let e = &self.units[t];
        if !Self::is_valid_missile_target(e, u.side) {
            return None;
        }
        let stop = u.missile_range(self.kv_rules.fire_on_walls_range_modifier)
            * MISSILE_ADVANCE_STOP_FRACTION;
        (dist2(u.position, e.position) > stop * stop).then_some(t)
    }

    /// The missile slot of unit `idx`'s update (PROVISIONAL position, see the module docs).
    pub(super) fn missile_step(&mut self, idx: usize, tick: u32) {
        let u = &self.units[idx];
        let Some(weapon) = u.missile else { return };
        // A dead or invalid ordered target is forgotten.
        if let Some(t) = u.fire_target {
            let gone = match self.unit_index(t) {
                Some(ti) => !Self::is_valid_missile_target(&self.units[ti], u.side),
                None => true,
            };
            if gone {
                self.units[idx].fire_target = None;
            }
        }
        let u = &self.units[idx];
        if !u.active || u.men == 0 || u.is_out_of_fight() || u.in_melee {
            return;
        }
        // PLACEHOLDER: reloading continues every tick the unit is not in melee or routing.
        if u.reload_ticks_left > 0 {
            self.units[idx].reload_ticks_left -= 1;
        }
        let u = &self.units[idx];
        // Line infantry cannot fire on the move (fire-and-advance is not modelled).
        if u.moved || u.ammunition == 0 || self.melee_contact(idx).is_some() {
            return;
        }
        let Some(t) = self.missile_target(idx) else { return };
        self.units[idx].aiming = true;
        if self.units[idx].reload_ticks_left > 0 {
            return;
        }
        let ev = self.fire_volley(idx, t, weapon, tick);
        self.volleys.push(ev);
    }

    /// Fires one volley from unit `s` at unit `t` and applies the casualties.
    fn fire_volley(&mut self, s: usize, t: usize, weapon: MissileWeapon, tick: u32) -> VolleyEvent {
        let rules = self.kv_rules;
        let shooter = &self.units[s];
        let target = &self.units[t];
        let distance = dist2(shooter.position, target.position).sqrt();
        // Volleys per reload cycle (grouped drills split the reload, see `volley_plan`).
        let mut groups = 1;
        let shooters = if weapon.is_artillery {
            weapon.guns.max(1)
        } else if shooter.garrison.is_some() {
            // From a building: one man per soldier slot on its fire lines (`garrison`).
            self.shooters(s)
        } else {
            let plan = volley_plan(shooter);
            groups = plan.groups.max(1);
            plan.shooters
        };
        // Each man who fires takes one round from the pool (GET_AMMO, `AmmoPool`).
        let shooters = shooter.ammo_pool.map_or(shooters, |p| shooters.min(p.rounds));
        let shots = shooters.saturating_mul(weapon.projectiles_per_shot.max(1));
        // CONFIRMED formula (W1 §12.6); not on walls; accuracy "mode" UNKNOWN → no bonus.
        let acc = missile::accuracy(
            weapon.accuracy,
            false,
            rules.fire_on_walls_accuracy_modifier,
            AccuracyMode::Other,
            false,
        );
        // In cover (shot `+0x34`, `0x00817130`, CONFIRMED): the target stands in woods (ground
        // field_forest, vegetation_dense_forest or vegetation_medium_woodland). PROVISIONAL stand-ins
        // for the projectile physics: behind its side's earthworks or gabions, or in a building
        // (`abilities::Battle::in_cover`).
        let in_cover = in_woods(self.ground.ground_type(target.position.0, target.position.1)) || self.in_cover(t, s);
        // The army level pair is (flag clear, 0) for every army in the model, so the level terms
        // change nothing (they are ported in `missile`).
        let (level_flag, level) = (false, 0);
        // Angle judgement from the launch elevation (unit-level: ground height to ground height).
        // APPROXIMATION: an unreachable target (no solution) shoots as on the level.
        let rise = self.ground.height(target.position.0, target.position.1)
            - self.ground.height(shooter.position.0, shooter.position.1);
        let angle = if weapon.ballistics.muzzle_velocity > 0.0 || weapon.ballistics.trajectory != missile::Trajectory::Low {
            missile::launch_angle(&weapon.ballistics, distance, rise).map_or(1.0, missile::angle_judgement)
        } else {
            NEUTRAL_SHOT_FACTOR
        };
        let control = missile::control(&missile::ControlInputs {
            fatigue_base: shooter.fatigue_effect().control,
            morale_state: shooter.morale.state,
            under_projectile_fire: shooter.under_fire_ticks > 0,
            under_artillery_fire: shooter.under_artillery_fire_ticks > 0,
            in_melee: shooter.in_melee,
        });
        let cth = missile::chance_to_hit(
            &ChanceToHitInputs {
                core_marksmanship: missile::level_core_marksmanship(acc, level_flag, level),
                marksmanship_bonus: missile::marksmanship_bonus(weapon.ballistics.accuracy_modifier, level_flag, level),
                control,
                visibility: missile::visibility(self.weather_intensity),
                angle_judgement: angle,
                is_land_artillery: weapon.is_artillery,
                is_naval: false,
                distance,
                target_in_cover: in_cover,
            },
            &rules,
        );
        let impact_in = ImpactInputs {
            damage: weapon.damage,
            horizontal_distance: distance,
            effective_range: weapon.range as f32,
            attacker_accuracy: Some(acc),
            defender_shield: target.shield,
            defender_armour: target.armour,
            // INFERRED: "D.defence" is the unit's melee defence (the only defence stat it has).
            defender_defence: target.melee_defence,
        };
        let (shooter_id, target_id) = (shooter.id, target.id);

        let mut hits = 0;
        let mut kills = 0;
        for _ in 0..shots {
            // An invincible unit's soldiers are never hit (0x00679B20 in the hit test 0x0080A4E0,
            // CONFIRMED gate on unit +0x2E8, set by the script call `set_invincible`).
            if self.units[t].men == 0 || self.units[t].invincible {
                break;
            }
            // APPROXIMATION: one roll stands in for the projectile flight (see the module docs).
            if self.rng.unit_float() > cth {
                continue;
            }
            hits += 1;
            if missile::impact(&mut self.rng, &impact_in, &rules) == ImpactOutcome::Kill {
                let d = &mut self.units[t];
                d.men -= 1;
                d.recent_losses += 1;
                kills += 1;
            }
        }
        // CONFIRMED reload formula (0x00639E40), with the fire order's drill (`effective_drill`).
        // The unit-level model has no walls, buildings or army level yet, so those terms are at their
        // defaults.
        let ctx = ReloadContext {
            fatigue: self.units[s].fatigue_state,
            morale_state: Some(self.units[s].morale.state),
            rain_or_snow: self.weather != super::fatigue::Weather::Clear,
            drill: FiringDrill::from_value(effective_drill(&self.units[s])),
            ..ReloadContext::default()
        };
        // A grouped drill fires one group per `reload / groups` (APPROXIMATION, see `volley_plan`).
        let reload = weapon.reload_ticks(&ctx, &rules).div_ceil(groups).max(1);
        let sh = &mut self.units[s];
        match sh.ammo_pool.as_mut() {
            Some(pool) => {
                pool.take(shooters);
                sh.ammunition = pool.rounds_per_man();
            }
            None => sh.ammunition -= 1,
        }
        sh.reload_ticks_left = reload;
        sh.volleys_fired = sh.volleys_fired.wrapping_add(1);
        sh.fired_this_tick = true;
        sh.kills += kills;
        let d = &mut self.units[t];
        if weapon.is_artillery {
            d.under_artillery_fire_ticks = UNDER_ARTILLERY_FIRE_TICKS;
        } else {
            d.under_fire_ticks = UNDER_FIRE_MEMORY_TICKS;
        }
        VolleyEvent {
            tick,
            shooter: shooter_id,
            target: target_id,
            shots,
            hits,
            kills,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::fatigue::KvFatigue;
    use super::super::morale::KvMorale;
    use super::super::rules::KvRules;
    use super::*;

    /// OBVIOUSLY MADE-UP placeholder rules (NOT game data).
    fn rules() -> KvRules {
        KvRules {
            missile_distance_for_half_chance_hit: 40,
            missile_distance_for_half_chance_hit_artillery: 100,
            projectile_damage_shield_divisor: 1,
            projectile_damage_armour_divisor: 1,
            projectile_damage_defense_divisor: 2,
            projectile_damage_distance_multiplier: 10.0,
            ..KvRules::default()
        }
    }

    /// A made-up musket (NOT game data).
    fn musket() -> MissileWeapon {
        MissileWeapon {
            range: 60,
            accuracy: 30.0,
            reload_skill: 50,
            reload_time_s: 10,
            damage: 0.5,
            projectiles_per_shot: 1,
            is_artillery: false,
            guns: 0,
            ballistics: Default::default(),
        }
    }

    fn shooter(id: u32, side: u8, pos: (f32, f32)) -> LandUnit {
        let mut u = LandUnit::new(id, side, 100, pos);
        u.missile = Some(musket());
        u.ammunition = 3;
        // A line of 50 files (two ranks): 50 men fire with the default drill.
        u.formation_files = 50;
        u
    }

    /// Two units 40 m apart that hold their positions.
    fn duel(seed: u32) -> Battle {
        let mut b = Battle::with_rules(seed, KvMorale::default(), KvFatigue::default(), rules());
        let mut a = shooter(1, 0, (0.0, 0.0));
        a.hold_position = true;
        let mut t = LandUnit::new(2, 1, 100, (40.0, 0.0));
        t.hold_position = true;
        t.armour = 2;
        t.melee_defence = 4;
        b.add_unit(a);
        b.add_unit(t);
        b
    }

    #[test]
    fn reload_time_matches_0x00639e40() {
        let r = rules();
        let ctx = ReloadContext::default();
        // 10 s at skill 50 = 10 s = 100 ticks; skill 40 → 11 s; skill 0 → 15 s.
        assert_eq!(musket().reload_seconds(&ctx, &r), 10.0);
        assert_eq!(musket().reload_ticks(&ctx, &r), 100);
        assert_eq!(MissileWeapon { reload_skill: 40, ..musket() }.reload_ticks(&ctx, &r), 110);
        assert_eq!(MissileWeapon { reload_skill: 0, ..musket() }.reload_ticks(&ctx, &r), 150);
        // Negative skill clamps to 0 (`cmovle`), skill >= 150 gives the 1 s floor.
        assert_eq!(reload_time_seconds(-20, 10), 15.0);
        assert_eq!(reload_time_seconds(200, 10), 1.0);
        assert_eq!(reload_time_seconds(50, 0), 1.0);
        // Fatigue and weather terms.
        let tired = ReloadContext {
            fatigue: super::super::fatigue::FatigueState::Exhausted,
            rain_or_snow: true,
            ..ctx
        };
        assert_eq!(reload_skill(50, &tired, &r), 50 - 10 - 6);
        // Drill vs walls: walls replace the drill modifier; buildings add the skill again.
        let r2 = KvRules {
            firing_drill_rank_fire_reload_modifier: 7,
            fire_on_walls_reload_modifier: -9,
            fire_and_advance_reload_modifier: -20,
            ..r
        };
        let drill = ReloadContext { drill: FiringDrill::RankFire, ..ctx };
        assert_eq!(reload_skill(50, &drill, &r2), 57);
        let walls = ReloadContext { on_walls: true, ..drill };
        assert_eq!(reload_skill(50, &walls, &r2), 41);
        let building = ReloadContext { in_building: true, fire_and_advance: true, ..ctx };
        assert_eq!(reload_skill(50, &building, &r2), 80);
        let army = ReloadContext { army_level: 2, morale_state: Some(super::super::morale::MoraleState::Wavering), ..ctx };
        assert_eq!(reload_skill(50, &army, &r2), 50 + 30 - 7);
        let army_flag = ReloadContext { army_level: 2, army_level_flag: true, ..ctx };
        assert_eq!(reload_skill(50, &army_flag, &r2), 50);
    }

    #[test]
    fn volley_plan_follows_the_drill_orders() {
        // 120 men in 40 files (three ranks).
        let mut u = LandUnit::new(1, 0, 120, (0.0, 0.0));
        u.formation_files = 40;
        let plan = |u: &LandUnit| (volley_plan(u).shooters, volley_plan(u).groups);
        // fire_volley: the front rank.
        assert_eq!(plan(&u), (40, 1));
        // Losses: the front rank stays full until fewer men than files remain.
        u.men = 41;
        assert_eq!(plan(&u), (40, 1));
        u.men = 25;
        assert_eq!(plan(&u), (25, 1));
        u.men = 120;
        // mass_fire: every man.
        u.capabilities.firing_drill = 1;
        assert_eq!(plan(&u), (120, 1));
        assert_eq!(FiringDrill::from_value(effective_drill(&u)), FiringDrill::MassFire);
        // Platoon fires: three groups of all the men.
        u.capabilities.firing_drill = 3;
        assert_eq!(plan(&u), (40, 3));
        // Rank fire: the ranks (at most three) in turn; a fourth rank never fires.
        u.capabilities.firing_drill = 5;
        assert_eq!(plan(&u), (40, 3));
        u.formation_files = 30;
        assert_eq!(plan(&u), (30, 3));
        u.formation_files = 60;
        assert_eq!(plan(&u), (60, 2));
        // Skirmishers fire with every man (unit `+0x1A5`), whatever the drill.
        u.capabilities.firing_drill = 0;
        u.attributes.skirmisher = true;
        assert_eq!(plan(&u), (120, 1));
        // Unknown files: the start men over three ranks.
        let v = LandUnit::new(2, 0, 100, (0.0, 0.0));
        assert_eq!(v.files(), 34);
    }

    #[test]
    fn volley_replays_the_rng_by_hand() {
        let mut b = duel(4242);
        // Replay the expected RNG use on a copy: 50 shooters (the front rank of 50 files), each one hit roll
        // and, when it hits, one impact roll.
        let mut rng = b.rng;
        let r = rules();
        let cth = missile::chance_to_hit(
            &ChanceToHitInputs {
                core_marksmanship: 30.0,
                control: 1.0,
                visibility: 1.0,
                angle_judgement: 1.0,
                distance: 40.0,
                ..Default::default()
            },
            &r,
        );
        // marks 30 / 1600 * (40*40*0.01 = 16) = 0.3
        assert!((cth - 0.3).abs() < 1e-6);
        let imp = ImpactInputs {
            damage: 0.5,
            horizontal_distance: 40.0,
            effective_range: 60.0,
            attacker_accuracy: Some(30.0),
            defender_shield: 0,
            defender_armour: 2,
            defender_defence: 4,
        };
        // kc = 30 + 20 - 0 - 2 - 2 - round(10*40/60 = 6.67 → 7) = 39
        assert_eq!(missile::impact_kill_chance(&imp, &r), Ok(39));
        let (mut hits, mut kills) = (0, 0);
        for _ in 0..50 {
            if rng.unit_float() <= cth {
                hits += 1;
                if missile::impact(&mut rng, &imp, &r) == ImpactOutcome::Kill {
                    kills += 1;
                }
            }
        }
        b.step();
        assert_eq!(b.volleys.len(), 1);
        let v = b.volleys[0];
        assert_eq!((v.tick, v.shooter, v.target, v.shots), (0, 1, 2, 50));
        assert_eq!((v.hits, v.kills), (hits, kills));
        assert!(kills > 0, "this seed should kill someone");
        assert_eq!(b.units[1].men, 100 - kills);
        assert_eq!(b.rng, rng, "same number and order of RNG calls");
        // Ammo used (50 of the pool of 100 men × 3 rounds; 2.5 per man shows as 3), reload started,
        // target counts as under fire for morale.
        assert_eq!(b.units[0].ammo_pool.map(|p| p.rounds), Some(250));
        assert_eq!(b.units[0].ammunition, 3);
        assert_eq!(b.units[0].reload_ticks_left, 100);
        assert!(b.units[1].under_fire_ticks > 0);
    }

    #[test]
    fn reload_gates_the_next_volley_and_ammo_runs_out() {
        let mut b = duel(7);
        let mut volley_ticks = Vec::new();
        for _ in 0..700 {
            b.step();
            volley_ticks.extend(b.volleys.iter().map(|v| v.tick));
        }
        // Loaded at tick 0, then every 100 ticks (10 s); 3 rounds per man for 100 men, 50 men
        // firing per volley: 6 volleys.
        assert_eq!(volley_ticks, vec![0, 100, 200, 300, 400, 500]);
        assert_eq!(b.units[0].ammunition, 0);
        assert!(!b.units[0].can_shoot());
    }

    #[test]
    fn ammo_pool_shares() {
        let mut p = AmmoPool::new(10, 3);
        assert_eq!((p.rounds, p.start, p.per_man, p.rounds_per_man()), (30, 30, 3, 3));
        assert_eq!(p.take(4), 4);
        // 26 rounds, 10 men: the first dead man takes ceil(2.6) = 3, the next ceil(23 / 9) = 3.
        p.losses(8);
        assert_eq!((p.rounds, p.men), (20, 8));
        assert_eq!(p.rounds_per_man(), 3);
        assert_eq!(p.take(50), 20);
        assert_eq!(p.rounds_per_man(), 0);
    }

    #[test]
    fn out_of_range_and_moving_units_do_not_fire() {
        let mut b = duel(1);
        b.units[1].position = (61.0, 0.0);
        b.units[0].fire_at_will = true;
        b.step();
        assert!(b.volleys.is_empty(), "61 m is outside the 60 m range");
        // Moving this tick: no fire even in range.
        let mut b = duel(1);
        b.units[0].destination = Some((1.0, 0.0));
        b.step();
        assert!(b.volleys.is_empty());
    }

    #[test]
    fn fire_order_and_fire_at_will() {
        let mut b = duel(1);
        b.units[0].fire_at_will = false;
        b.step();
        assert!(b.volleys.is_empty(), "no target without fire at will or an order");
        assert!(b.order_fire(1, 2));
        b.step();
        assert_eq!(b.volleys.len(), 1);
        // Invalid orders are refused.
        assert!(!b.order_fire(2, 1), "unit 2 has no missile weapon");
        assert!(!b.order_fire(1, 1), "same side");
        assert!(!b.order_fire(1, 99), "unknown id");
        // A move order cancels the fire order.
        assert!(b.order_move(1, (0.0, 0.0)));
        assert_eq!(b.units[0].fire_target, None);
    }

    #[test]
    fn ordered_target_out_of_range_is_approached() {
        let mut b = duel(1);
        b.units[1].position = (100.0, 0.0);
        b.units[0].walk_speed = 5.0;
        assert!(b.order_fire(1, 2));
        let mut first = None;
        for _ in 0..200 {
            b.step();
            if first.is_none() && !b.volleys.is_empty() {
                first = Some(b.units[0].position.0);
            }
        }
        let x = first.expect("the shooter walked into range and fired");
        // It stops at 90 % of the 60 m range = 54 m from the target.
        assert!((100.0 - x - 54.0).abs() < 0.6, "stopped at {x}");
    }

    #[test]
    fn under_fire_feeds_morale_and_fatigue() {
        let mut b = duel(3);
        b.step();
        let t = b.morale_inputs(1);
        assert!(t.under_projectile_fire);
        assert!(!t.under_artillery_fire);
        // The memory runs out.
        b.units[0].ammunition = 0;
        for _ in 0..UNDER_FIRE_MEMORY_TICKS {
            b.step();
        }
        assert!(!b.morale_inputs(1).under_projectile_fire);
    }

    #[test]
    fn artillery_kills_outright_with_damage_one() {
        // damage >= 1 inside range is a KILL with no impact roll (W1 §12.10).
        let mut b = duel(11);
        b.units[0].missile = Some(MissileWeapon {
            damage: 1.0,
            is_artillery: true,
            guns: 2,
            projectiles_per_shot: 3,
            ..musket()
        });
        let mut rng = b.rng;
        b.step();
        let v = b.volleys[0];
        assert_eq!(v.shots, 6);
        assert_eq!(v.hits, v.kills);
        // Only the 6 hit rolls were used.
        for _ in 0..6 {
            rng.unit_float();
        }
        assert_eq!(b.rng, rng);
        assert!(b.units[1].under_artillery_fire_ticks > 0);
    }

    #[test]
    fn shooting_battle_is_deterministic() {
        let make = |seed| {
            let mut b = Battle::with_rules(seed, KvMorale::default(), KvFatigue::default(), rules());
            for k in 0..3u32 {
                b.add_unit(shooter(k, 0, (k as f32 * 30.0, 0.0)));
                b.add_unit(shooter(10 + k, 1, (k as f32 * 30.0, 70.0)));
            }
            b
        };
        let (mut a, mut c) = (make(99), make(99));
        let mut volleys = 0;
        for _ in 0..600 {
            a.step();
            c.step();
            volleys += a.volleys.len();
            assert_eq!(a.state_hash(), c.state_hash());
        }
        assert!(volleys > 0);
        assert!(a.units.iter().any(|u| u.men < u.max_men));
        let mut d = make(100);
        for _ in 0..600 {
            d.step();
        }
        assert_ne!(a.state_hash(), d.state_hash());
    }
}

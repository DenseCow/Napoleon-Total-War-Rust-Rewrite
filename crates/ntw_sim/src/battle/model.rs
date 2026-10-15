//! The battle model skeleton for the vertical slice: `Battle`, `LandUnit` and the 0.1 s tick.
//!
//! What is faithful to the original:
//! - One `step()` = one 0.1 s tick (W1 §12.2, INFERRED high); the tick counter is `battle+0x58`
//!   and the battle RNG lives at `battle+0x50` (W1 §8, CONFIRMED).
//! - Units are updated one after another in a fixed order (the `Vec` order, which we keep sorted by
//!   id), and each unit follows the per-unit order of `0x0057F070` (W1 §5 item 6, CONFIRMED).
//! - The full morale evaluation runs only when `unit_id % 5 == tick % 5`; on the other ticks only
//!   the light update runs (CONFIRMED).
//!
//! - Melee uses the **real** blow pipeline of W1 §12.9 (pair selection rolls, hit number, kill
//!   chance, one 1..1000 roll per blow, strike-back after a charging miss). Because this model has
//!   no individual soldiers yet, *who* fights *how often* is APPROXIMATED; see
//!   the doc comment of `Battle::melee_step` for exactly what is approximated.
//!
//! What is **PLACEHOLDER** (our own simple stand-ins, to be replaced):
//! - Movement (straight lines), how attack direction is classified, the casualty-ratio
//!   bookkeeping, and treating each unit's fatigue as a single representative soldier.

use std::collections::BTreeSet;

use super::TICK_SECONDS;
use super::attributes::UnitAttributes;
use super::fatigue::{self, FatigueAction, FatigueEffects, FatigueInputs, FatigueState, KvFatigue, Weather};
use super::ground::{BattleGround, MovementClass};
use super::melee::{self, BlowOutcome, Combatant, EncounterMember, MeleeDir, MeleeInputs};
use super::morale::{
    self, AttackDirection, Attacker, KvMorale, MoraleBehaviour, MoraleComponent, MoraleInputs,
    MoraleState,
};
use super::rules::KvRules;
use super::shooting::{AmmoPool, MISSILE_ADVANCE_STOP_FRACTION, MissileWeapon, VolleyEvent};
use crate::fnv::Fnv64;
use crate::rng::CaRng;

/// PLACEHOLDER: distance (metres) at which two units are "in melee contact".
pub const MELEE_CONTACT_RANGE: f32 = 10.0;
/// PROVISIONAL: how long a destroyed general counts as "died recently" (army `+0x1B4` countdown,
/// one per tick; its start value was not found). 600 ticks = 60 s.
pub const GENERAL_DIED_RECENTLY_TICKS: u32 = 600;

/// PROVISIONAL fallback without a playable area: ticks a reinforcement unit takes to walk onto the
/// field. With one, the exe's rule applies (`0x005857A0`, see [`Battle::reinforcements_step`]).
/// 100 = 10 s.
pub const REINFORCEMENT_WALK_ON_TICKS: u32 = 100;

/// PROVISIONAL: sideways spacing of reinforcement units entering at the same point (metres).
pub const REINFORCEMENT_SPACING_M: f32 = 40.0;

/// Where a reinforcement army enters: `((side, army), position, facing)`.
pub type ReinforcementEntry = ((u8, u32), (f32, f32), f32);

/// A battle-file `reinforcement_army` unit's state (BATTLE_FIDELITY.md §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reinforcement {
    /// Not a reinforcement, or already arrived.
    #[default]
    None,
    /// Held off the field until the battle script releases it (`manually_deployed` true in the
    /// file, unit `+0x1E5`; `unit:deploy_reinforcement(true)` releases it). Counts as out of the
    /// fight.
    Held,
    /// Off the field, arrives in turn.
    Waiting,
    /// Walking in: not active yet (`+0xAA0` = 0) until its whole formation is inside the playable
    /// area; the next unit of its army waits for it (one unit at a time, `0x00608200`).
    Arriving {
        /// Ticks left.
        ticks_left: u32,
    },
}
/// PLACEHOLDER: advancing units stop at this fraction of the contact range.
const ADVANCE_STOP_FRACTION: f32 = 0.8;
/// APPROXIMATION: at most this many men of a unit are engaged in one melee (its "frontage").
pub const MELEE_FRONTAGE_MEN: u32 = 40;
/// APPROXIMATION: each engaged soldier pair starts one exchange every this many ticks (5 s).
/// The original resolves one exchange per fight object (`0x00664E80` → `0x006AFE20`), then picks the
/// combat animations for the participants and sets the fight's `+0xA8` to a time + 5.0 s (20.0 s for a
/// charge) (CONFIRMED constants). INFERRED: one exchange per pair per 5 s. It was 2 s, which made melee
/// about 2.5 times too deadly (BATTLE_FIDELITY.md §45).
pub const MELEE_EXCHANGE_INTERVAL_TICKS: u32 = 50;
/// APPROXIMATION: largest number of soldiers one side puts into a local encounter list.
pub const MELEE_MAX_LOCAL_PER_SIDE: u32 = 3;

/// One land unit in the battle model. Field offsets in the comments refer to W1 §8 "Land unit".
#[derive(Debug, Clone, PartialEq)]
pub struct LandUnit {
    /// Unit id (we use `u32`).
    pub id: u32,
    /// Byte `unit+0xD48`: experience level (chevrons) 0..9 (CONFIRMED meaning: two UI/script
    /// builders expose this byte as "Experience", `0x005ABF40` and `0x005CD340`; the fatigue
    /// function `0x00670F40` indexes the experience-bonus table by it). It drives the morale
    /// waver/rout timers (`morale::waver_timeout`, `rout_timeout`). Who writes it is UNKNOWN
    /// (no direct store in the exe, BATTLE_FIDELITY.md §56 (1), §58 (1)); the battle setup fills it.
    pub experience: u8,
    /// Alliance / side index.
    pub side: u8,
    /// Current men.
    pub men: u32,
    /// Starting men.
    pub max_men: u32,
    /// Position in metres (x, y) on the battlefield plane.
    pub position: (f32, f32),
    /// Facing in radians (0 = +x). PLACEHOLDER: set from the direction of movement.
    pub facing: f32,
    /// Ordered destination, if any. Written through [`LandUnit::set_destination`] (it restarts the
    /// timed move).
    pub(crate) destination: Option<(f32, f32)>,
    /// Walking speed in m/s (unit stat input; no game data hard-coded).
    pub walk_speed: f32,
    /// Running speed in m/s (unit stat input).
    pub run_speed: f32,
    /// Acceleration in m/s² (`battle_entities` column 5, the entity's locomotive `+0x158`; see
    /// [`step_speed`]). Unit stat input; a unit built without entity data (tests, the fixture's
    /// fallback speeds) changes speed at once (infinite).
    pub acceleration: f32,
    /// Deceleration in m/s² (column 6, locomotive `+0x100`), as [`LandUnit::acceleration`].
    pub deceleration: f32,
    /// Each man's radius in metres: the soldier's `+0x620`, which the soldier set-up `0x0061CB90`
    /// sets from `+0x58`, column 12 of the `battle_entities` record his unit builds its men from
    /// (unit `+0x60`, pushed at `0x0051C09B`; men 0.35). CONFIRMED. The measured box adds twice the
    /// largest to each extent ([`super::march::SquadBox`]). Unit stat input; 0 without entity data.
    pub man_radius: f32,
    /// The ground speed (m/s) the unit moved at in its last tick (the soldier's `+0x194`): the
    /// speed the next tick's change starts from ([`step_speed`]). 0 when it stopped.
    pub speed: f32,
    /// The timed-move order the unit's soldier follows ([`TimedMove`]); `None` when it is not
    /// moving.
    pub timed_move: Option<TimedMove>,
    /// Each man's own move in the unit's march (his lag behind his place and his speed), see
    /// [`super::march`].
    pub march: super::march::MenMarch,
    /// `+0x170`: melee attack (unit_stats_land col 34, W1 §12.9). Game-data input.
    pub melee_attack: i32,
    /// `+0x174`: charge bonus (col 35). Game-data input.
    pub charge_bonus: i32,
    /// `+0x158`: armour (col 11, INFERRED). Game-data input.
    pub armour: i32,
    /// `+0x178`: shield (col 36, INFERRED). Game-data input.
    pub shield: i32,
    /// `+0x17C`: melee defence (col 37, INFERRED). Game-data input.
    pub melee_defence: i32,
    /// `+0x184`: bonus vs cavalry (col 51, INFERRED). Game-data input.
    pub bonus_vs_cavalry: i32,
    /// Cavalry (combatant `+0x48`); otherwise the unit counts as infantry (`+0x4C`).
    pub is_cavalry: bool,
    /// Anti-charge / spear-like (combatant `+0x38`).
    pub anti_charge: bool,
    /// Braced (combatant `+0x44`).
    pub braced: bool,
    /// A charge order is on (set by `order_attack_unit`, cleared by `order_end_charge`). The
    /// combatant flag `+0x40` is [`LandUnit::charging_now`]: the charge counts only until its impact
    /// is over.
    pub charging: bool,
    /// Ticks this unit has been in melee contact; back to 0 once it has been out of contact for
    /// `MELEE_EXCHANGE_INTERVAL_TICKS` (see [`LandUnit::charging_now`]).
    pub contact_ticks: u32,
    /// The unit has left the battlefield (`+0xAA0` = 2, see [`Battle::leave_step`]): off the field for
    /// good, out of the fight, its men neither killed nor counted by anyone.
    pub left_field: bool,
    /// Ticks since this unit was last in melee contact.
    pub out_of_contact_ticks: u32,
    /// In square (combatant `+0x54`).
    pub in_square: bool,
    /// Blows this unit has resolved as attacker (statistics counter `unit+0xBF0`, W1 §12.9).
    pub blows_resolved: u32,
    /// `+0x180`: morale stat added to `morale_base`.
    pub morale_stat: i32,
    /// The unit attribute flags (`unit_stats_land` boolean columns, see [`super::attributes`]).
    pub attributes: UnitAttributes,
    /// The unit card's capability block (battle-file `unit_capabilities`), see
    /// [`super::attributes::UnitCapabilities`].
    pub capabilities: super::attributes::UnitCapabilities,
    /// `+0x194`: the training level, the exe enum of `0x00EED960` over the unit record's
    /// `training_level` (0 `mob`, 1 `rabble`, 2 `poorly_trained`, 3 `trained`, 4 `well_trained`,
    /// 5 `elite`, 6 any other; CONFIRMED static, [`training_level_of`]).
    pub training_level: u8,
    /// The in-place tolerance of the training level: its `entity_training_levels` float (row
    /// `+0xC`, read by `0x0057E1F0` through the table's per-level rows `0x00F88300`), in metres.
    pub training_tolerance: f32,
    /// `+0xD08`: morale-modifier category.
    pub category: i32,
    /// The unit's morale component.
    pub morale: MoraleComponent,
    /// `+0xC74` (INFERRED): unit fatigue = mean of soldiers. Here: one representative soldier.
    pub fatigue: i32,
    /// Fatigue state of the representative soldier.
    pub fatigue_state: FatigueState,
    /// The fatigue-effect multipliers per fatigue level 0..=5 (`fatigue_effects` rows of the unit's
    /// category; `FatigueEffects::NONE` for the levels without a row). See [`LandUnit::fatigue_effect`].
    pub fatigue_effects: [FatigueEffects; 6],
    /// `+0xAA0`: active flag.
    pub active: bool,
    /// PLACEHOLDER bookkeeping: enemies killed (for the "blood" kill ratio).
    pub kills: u32,
    /// PLACEHOLDER bookkeeping: men lost since the last full morale evaluation.
    pub recent_losses: u32,
    /// Tick of the last full morale evaluation (for debugging and tests).
    pub last_full_morale_tick: Option<u32>,
    /// True if the unit fought in melee this tick (drives the fatigue action).
    pub in_melee: bool,
    /// True if the unit moved this tick (drives the fatigue action).
    pub moved: bool,
    /// PLACEHOLDER order flag: with no destination, stay put instead of advancing on the enemy.
    pub hold_position: bool,
    /// The run option of the unit's current move: it moves at `run_speed` instead of `walk_speed`
    /// ([`LandUnit::move_speed`]). In the original the option belongs to the order, not the unit
    /// (CONFIRMED, `0x005600C0`, BATTLE_FIDELITY.md §59): each order carries it, a change of speed
    /// edits the current order, and it ends with the move. So it is cleared when the move ends
    /// (arrival, or no movement goal left) and set only through orders (`orders.rs`).
    pub running: bool,
    /// Missile weapon, if the unit has one (built from `unit_stats_land` + `projectiles`).
    pub missile: Option<MissileWeapon>,
    /// Rounds of ammunition per man (`unit_stats_land` col 31, CONFIRMED UI label "Ammunition"). For
    /// line units this is the starting value of the cartridge pool (`ammo_pool`) and afterwards the
    /// rounds per man left in it, rounded up. Artillery (PROVISIONAL): one round per volley.
    pub ammunition: u32,
    /// The cartridge pool of a unit with a non-artillery missile weapon ([`AmmoPool`]), created at
    /// the unit's first update from its men and `ammunition`.
    pub ammo_pool: Option<AmmoPool>,
    /// Ticks until the weapon is loaded again (0 = loaded).
    pub reload_ticks_left: u32,
    /// Unit id the player ordered this unit to shoot at.
    pub fire_target: Option<u32>,
    /// Fire at will: shoot the nearest enemy in range when there is no fire order.
    pub fire_at_will: bool,
    /// Volleys fired so far (statistics).
    pub volleys_fired: u32,
    /// True if the unit fired a volley this tick (drives the fatigue action).
    pub fired_this_tick: bool,
    /// True if the unit had a target in range this tick (so reloading counts as its action).
    pub aiming: bool,
    /// Ticks left during which the unit counts as under small-arms fire (morale flag `+0xC84`).
    pub under_fire_ticks: u32,
    /// Ticks left during which the unit counts as under artillery fire (morale flag `+0xC80`).
    pub under_artillery_fire_ticks: u32,
    /// Which `unit_movement_modifiers` column this unit uses (PROVISIONAL, see `ground`).
    pub movement_class: MovementClass,
    /// Gradient of this tick's move (height gained / distance; 0 when it did not move).
    pub gradient: f32,
    /// `units` table class key (column #3, e.g. `infantry_line`; empty when unknown). Added for the
    /// AI (`ntw_ai::battle::classes`), which maps it to the original's class enum.
    pub unit_class: String,
    /// `units` table category key (column #2, e.g. `infantry`, `cavalry`, `dragoons`; empty when
    /// unknown).
    pub unit_category: String,
    /// Index of the unit's army within its side (alliance): 0 = the first army; units of later
    /// armies are "allies" for `0x0054C840`.
    pub army_index: u32,
    /// `Some(rank)` if this unit is its army's general: the unit card `+0xC0` is 1, 2, 4 or 5 and
    /// `+0xBC` is the rank (battle files: a `general` element, rank = `star_rating level`; the
    /// `experience` child is not read, CONFIRMED parser `0x0050CAE0`). The last such unit of an
    /// army is its general (`0x00505B00`, CONFIRMED).
    pub general_rank: Option<i32>,
    /// Reinforcement state (battle-file `reinforcement_army` units), see [`Reinforcement`].
    pub reinforcement: Reinforcement,
    /// A battle script's unit controller has taken control of the unit (`take_control`): the
    /// battle AI leaves it alone until `release_control` (INFERRED from the scripts' use).
    pub script_controlled: bool,
    /// `+0xD9C`: skirmish mode (the `skirmish` order sets it, `0x005602A0`; read by the evade check
    /// `0x0054C9D0`). See [`super::abilities`].
    pub skirmish: bool,
    /// The skirmish check's memory (behaviour object `+0x24` / `+0x28`), see [`super::abilities`].
    pub skirmish_eval: super::abilities::SkirmishEval,
    /// `+0xDC0`: the selected deployable defence, an ability enum value (`None` = 0x16, `none`).
    pub deployable: Option<u8>,
    /// Special abilities the unit has performed (bit `n` = ability `n`), see
    /// [`Battle::order_special_ability`].
    pub active_abilities: u32,
    /// The shot type loaded (index into [`super::attributes::SHOT_TYPE_NAMES`]), if known.
    pub shot_type: Option<u8>,
    /// The other shots the unit's gun can load: `(shot type, weapon)` (artillery; from
    /// `gun_type_to_projectiles`). Empty for units with one projectile.
    pub shot_options: Vec<(u8, MissileWeapon)>,
    /// `+0x2E8`: set by the script call `set_invincible` (`BCQ_UNIT_SET_INVINCIBLE`, handler
    /// `0x005C2B80`, CONFIRMED): the hit test skips its soldiers (`0x00679B20`).
    pub invincible: bool,
    /// The formation's width in metres (formation `+0x24`; set from the unit's files and spacing).
    /// 0 = unknown: [`LandUnit::fresh_width`] falls back to the files at
    /// [`super::abilities::FILE_SPACING_M`].
    pub formation_width: f32,
    /// Metres between the formation's ranks (close order, `spacing_rank_close`). Its depth is this
    /// times the ranks of [`LandUnit::formation_grid`] ([`LandUnit::depth`]), so the depth and the
    /// men's places always come from the one grid. 0 = unknown:
    /// [`super::abilities::FILE_SPACING_M`] stands in.
    pub formation_rank_spacing: f32,
    /// Men in one rank of the formation (its files): the line shape's vfunc `+0x10`, `0x00642D00` →
    /// `0x00642C90`: `floor((width + 0.001 − 2r) / file spacing) + 1`, which its callers (`0x0061CAB0`,
    /// `0x00646B00`, `0x00646C70`, `0x00646D40`, `0x0065A410`, `0x0065A6D0`) cap at the men they are
    /// passed; the fire step (`0x005694D0`) passes the living men, unit `+0x18` (CONFIRMED;
    /// [`LandUnit::files`] applies the cap). Set by the battle setup from the drawn block (files ×
    /// spacing gives the same count). 0 = unknown: [`LandUnit::files`] falls back to the living men
    /// over [`super::abilities::FORMATION_RANKS`].
    pub formation_files: u32,
    /// The building (index into [`Battle::buildings`]) the unit is in (unit `+0x11C`), see
    /// [`super::garrison`].
    pub garrison: Option<usize>,
    /// The building the unit was ordered to garrison (`defend_building`), until it is inside.
    pub garrison_target: Option<usize>,
    /// The unit is against an enemy defence piece (its contact hits were dealt), see
    /// [`Battle::defence_contact`].
    pub defence_contact: bool,
    /// The casualty and kill log (`+0xC78` object, `0x005828A0`), see [`super::casualties`].
    pub casualty_log: super::casualties::CasualtyLog,
}

impl LandUnit {
    /// Installs a move order to `dest`, or clears it with `None`: the one path every order source
    /// writes `destination` by. Every install starts a new timed move, also one re-issued to the
    /// same point (`0x00659520` zeroes the order's elapsed time `+0x43C` on each install).
    pub fn set_destination(&mut self, dest: Option<(f32, f32)>) {
        self.destination = dest;
        self.timed_move = None;
    }

    /// The ordered destination, if any (written only through [`LandUnit::set_destination`]).
    pub fn destination(&self) -> Option<(f32, f32)> {
        self.destination
    }

    /// A new unit with neutral placeholder stats. Speeds and morale stat are inputs the caller
    /// should set from the unit's game data.
    pub fn new(id: u32, side: u8, men: u32, position: (f32, f32)) -> Self {
        LandUnit {
            id,
            experience: 0,
            side,
            men,
            max_men: men,
            position,
            facing: 0.0,
            destination: None,
            walk_speed: 1.0,
            run_speed: 2.0,
            acceleration: f32::INFINITY,
            deceleration: f32::INFINITY,
            man_radius: 0.0,
            speed: 0.0,
            timed_move: None,
            march: Default::default(),
            melee_attack: 0,
            charge_bonus: 0,
            armour: 0,
            shield: 0,
            melee_defence: 0,
            bonus_vs_cavalry: 0,
            is_cavalry: false,
            anti_charge: false,
            braced: false,
            charging: false,
            contact_ticks: 0,
            left_field: false,
            out_of_contact_ticks: 0,
            in_square: false,
            blows_resolved: 0,
            morale_stat: 0,
            attributes: UnitAttributes::default(),
            capabilities: Default::default(),
            training_level: 0,
            training_tolerance: 0.0,
            category: 0,
            morale: MoraleComponent::default(),
            fatigue: 0,
            fatigue_state: FatigueState::Fresh,
            fatigue_effects: [FatigueEffects::NONE; 6],
            active: true,
            kills: 0,
            recent_losses: 0,
            last_full_morale_tick: None,
            in_melee: false,
            moved: false,
            hold_position: false,
            running: false,
            missile: None,
            ammunition: 0,
            ammo_pool: None,
            reload_ticks_left: 0,
            fire_target: None,
            fire_at_will: true,
            volleys_fired: 0,
            fired_this_tick: false,
            aiming: false,
            under_fire_ticks: 0,
            under_artillery_fire_ticks: 0,
            movement_class: MovementClass::Infantry,
            gradient: 0.0,
            unit_class: String::new(),
            unit_category: String::new(),
            army_index: 0,
            general_rank: None,
            reinforcement: Reinforcement::None,
            script_controlled: false,
            skirmish: false,
            skirmish_eval: Default::default(),
            deployable: None,
            active_abilities: 0,
            shot_type: None,
            shot_options: Vec::new(),
            invincible: false,
            formation_width: 0.0,
            formation_rank_spacing: 0.0,
            formation_files: 0,
            garrison: None,
            garrison_target: None,
            defence_contact: false,
            casualty_log: Default::default(),
        }
    }

    /// The speed of the unit's current move: `run_speed` when its move runs ([`LandUnit::running`]),
    /// else `walk_speed`.
    pub fn move_speed(&self) -> f32 {
        if self.running { self.run_speed } else { self.walk_speed }
    }

    /// A reinforcement still off the field (held by the script or waiting its turn): not drawn,
    /// not on the map yet.
    pub fn off_field(&self) -> bool {
        !self.active && matches!(self.reinforcement, Reinforcement::Held | Reinforcement::Waiting)
    }

    /// True if the unit no longer fights: routing, shattered, or destroyed.
    pub fn is_out_of_fight(&self) -> bool {
        self.men == 0 || self.morale.is_routing_or_shattered() || self.reinforcement == Reinforcement::Held
    }

    /// The fatigue-effect multipliers for the unit's current fatigue level (`0x00649520`).
    pub fn fatigue_effect(&self) -> FatigueEffects {
        self.fatigue_effects[self.fatigue_state as usize]
    }

    /// The combatant "charging" flag (`+0x40`). In the exe it is the soldier's action `+0x1B8 ==
    /// 0xD` (the charge run, CONFIRMED in the combatant builder). Once the soldier fights, his
    /// action is a combat action (0x14–0x16, the fatigue table's "combat" group), so a charge
    /// counts for the impact only. Unit-level (INFERRED): a charging unit's charge counts for its
    /// first `MELEE_EXCHANGE_INTERVAL_TICKS` in contact, one exchange per engaged pair, and comes
    /// back only after the unit has been out of contact as long.
    pub fn charging_now(&self) -> bool {
        self.charging && self.contact_ticks < MELEE_EXCHANGE_INTERVAL_TICKS
    }

    /// This unit as a melee combatant (W1 §12.9 combatant info). APPROXIMATION: every soldier of
    /// the unit shares the unit's values. Attack and charge are scaled by the fatigue effects of
    /// the unit's level; the army-level bonus is 0 (every army is at level 0 in the model).
    pub fn combatant(&self) -> Combatant {
        Combatant {
            // The combatant's fatigue is the fatigue LEVEL 0..=5 (W1 §12.9 combatant +0x08 =
            // entity vfunc +0xE0, the same value that indexes the 6-row fatigue_effects table
            // via unit+0xC70), NOT the raw fatigue points (thousands). With raw points,
            // `relative_melee_fatigue_multiplier × (D - A)` swamped every other term and pinned
            // the kill chance at 1 or 990. Found by the manager's real-data battle check.
            fatigue: self.fatigue_state as i32,
            attack: melee::scaled_stat(self.melee_attack, self.fatigue_effect().attack)
                + melee::army_level_attack_bonus(false, 0),
            charge: melee::scaled_stat(self.charge_bonus, self.fatigue_effect().charge),
            armour: self.armour,
            shield: self.shield,
            defence: self.melee_defence,
            bonus_vs_cavalry: self.bonus_vs_cavalry,
            entrenchment: 0,
            environment: 0,
            anti_charge: self.anti_charge,
            charging: self.charging_now(),
            braced: self.braced,
            cavalry: self.is_cavalry,
            infantry: !self.is_cavalry,
            f50: false,
            in_square: self.in_square,
            on_walls: false,
            in_building: false,
        }
    }
}

/// Direction from which `attacker_pos` strikes a defender at `defender_pos` facing `facing`
/// (radians, 0 = +x). `0x006AD890`, CONFIRMED from the disassembly (all `f32`):
/// ```text
/// v = D - A ; if |v|^2 < 0.010000001: front
/// v = v / |v| ; c = clamp(dot(F, v), -1, 1) ; deg = acos(c) * 57.295776
/// c >= 0: deg <= 45 → rear  ; else atan2(v.x, v.z) >= 0 → flank 1 (left), < 0 → flank 2 (right)
/// c <  0: deg >= 135 → front; else atan2(v.x, v.z) >= 0 → flank 2 (right), < 0 → flank 1 (left)
/// ```
/// `F` is the defender's facing vector (table `0x0176CFF8` by u16 angle). The flank side uses the
/// sign of the *world* x of `v`, not the facing (as compiled). The exe's (x, z) plane is our
/// (x, y) plane; `acos` is the CRT call `0x0128448C` (INFERRED from its use).
pub fn melee_dir(defender_pos: (f32, f32), facing: f32, attacker_pos: (f32, f32)) -> MeleeDir {
    let vx = defender_pos.0 - attacker_pos.0;
    let vz = defender_pos.1 - attacker_pos.1;
    let l2 = vx * vx + vz * vz;
    if 0.010_000_001_f32 > l2 {
        return MeleeDir::Front;
    }
    let inv = 1.0 / l2.sqrt();
    let (nx, nz) = (vx * inv, vz * inv);
    let c = (facing.sin() * nz + facing.cos() * nx).clamp(-1.0, 1.0);
    let deg = c.acos() * 57.295_776;
    let side = nx.atan2(nz);
    if c >= 0.0 {
        if deg <= 45.0 {
            MeleeDir::Rear
        } else if side >= 0.0 {
            MeleeDir::FlankLeft
        } else {
            MeleeDir::FlankRight
        }
    } else if deg >= 135.0 {
        MeleeDir::Front
    } else if side >= 0.0 {
        MeleeDir::FlankRight
    } else {
        MeleeDir::FlankLeft
    }
}

/// Outcome of [`Battle::battle_result`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleResult {
    /// More than one side still has a fighting unit.
    Ongoing,
    /// Exactly one side still has a fighting unit.
    Won {
        /// The winning side.
        side: u8,
    },
    /// No side has a fighting unit left.
    Draw,
}

/// The land battle model.
#[derive(Debug, Clone, PartialEq)]
pub struct Battle {
    /// Tick counter (`battle+0x58`, W1 §8). Battle time = `tick * 0.1 s`. The exe's is the battle
    /// model's (ctor `0x00506C00` zeroes it at `0x00506C85`; `0x0057DF40` adds 1 after each model tick
    /// at `0x0057E0BC`). PROVISIONAL: the exe's model ticks in deployment too (its phase `0x00583820`
    /// runs from tick 8 on, analysis/ai/AI_RESEARCH.md), so its count at the conflict's start is the
    /// deployment's ticks; ours starts at 0 there. Every rule keyed on the count depends on the
    /// base, e.g. the timed move's rare 10-tick periods ([`timed_move_reissue_due`]).
    pub tick: u32,
    /// Battle-wide RNG (`battle+0x50`, W1 §8).
    pub rng: CaRng,
    /// Units, in update order (kept sorted by id by [`Battle::add_unit`]).
    pub units: Vec<LandUnit>,
    /// `kv_morale` game data.
    pub kv_morale: KvMorale,
    /// `kv_fatigue` game data.
    pub kv_fatigue: KvFatigue,
    /// `kv_rules` game data (melee and missile constants).
    pub kv_rules: KvRules,
    /// Weather (affects idle fatigue).
    pub weather: Weather,
    /// The intensity of the battle's weather, `battle_weather_types` col 1 (dry 0, light 1, heavy 2,
    /// torrential 3); it lowers every shot's chance to hit (`missile::visibility`, CONFIRMED). 0 until
    /// the battle setup picks a weather (PROVISIONAL, as for `weather`).
    pub weather_intensity: u32,
    /// The battle climate's fatigue terms `+0x2C` (heat) and `+0x30` (cold), added to every soldier's
    /// fatigue each tick unless the unit is exempt ([`fatigue::climate_term`]). INFERRED source:
    /// `battle_climate_weather_descriptions` columns 7 and 8 of the battle's climate/season/weather
    /// row (BATTLE_FIDELITY.md §4.2). 0 until the battle setup picks a weather (PROVISIONAL).
    pub climate_fatigue: (i32, i32),
    /// Column 6 (`+0x20`) of `unit_stats_land_experience_bonuses` in file order, indexed by the
    /// unit's experience level (`+0xD48`): the per-tick fatigue bonus of a veteran unit (CONFIRMED
    /// read in `0x00670F40`, see [`fatigue::experience_bonuses`]). Empty = no game data, which
    /// makes every lookup 0.
    pub experience_fatigue: Vec<i32>,
    /// The battle's unit-size scale, i.e. the `unit_scale` option's float (`0x004A6540` clamps it
    /// to `[0.1, 1.0]`; the four steps are [`super::unit_scale::STEPS`]). Every unit built from a
    /// unit card gets `trunc(card_men * scale)` men (CONFIRMED `CVTDQ2PS`/`MULSS`/`CVTTSS2SI` at
    /// `0x004A67DE..0x004A67EB`), so a battle at 0.25 has a quarter of the men per unit. The
    /// default 1.0 is the "no scaling" step, which is what the unit constructor got before this
    /// option was read; the exe's own default is `gfx_unit_scale 2` = 0.75
    /// ([`super::unit_scale::PREFERENCE_DEFAULT`]), a deliberate deviation when no preference is
    /// given (BATTLE_FIDELITY.md §18a). [`Battle::men_at_scale`] clamps it again before use.
    pub unit_scale: f32,
    /// The top of that clamp: the original's 1.0, or a modded unit-size step above it
    /// ([`crate::limits::GameLimits::max_scale`]); set with the scale.
    pub unit_scale_top: f32,
    /// The volleys fired during the most recent tick (cleared at the start of every `step`).
    /// Display-only output: the model never reads it.
    pub volleys: Vec<VolleyEvent>,
    /// The ground under the units (types and heights). Flat grassland by default; shared, as
    /// it never changes during a battle.
    pub ground: std::sync::Arc<BattleGround>,
    /// Each side's starting strength (alliance `+0x60`: the sum of `0x006AFB70 + 0x006B05B0` over
    /// its units, stored once; CONFIRMED in `0x00539E80`). Set at the first [`Battle::step`]
    /// (INFERRED moment), in ascending side order.
    pub side_start_strength: Vec<(u8, f32)>,
    /// When each army's general unit was destroyed: `((side, army), tick)` (for
    /// `GeneralStatus::DiedRecently`).
    pub general_died: Vec<((u8, u32), u32)>,
    /// Where each reinforcement army enters: `((side, army), position, facing)` (PROVISIONAL
    /// position, see `Battle::reinforcements_step`).
    pub reinforcement_entries: Vec<ReinforcementEntry>,
    /// Reinforcement units an army brings onto the field (the original's 20,
    /// [`crate::limits::GameLimits::max_reinforcement_units`]; the battle setup sets it from data).
    pub max_reinforcement_units: usize,
    /// Where arrived reinforcements are sent: the centre of the playable area (CONFIRMED rule of
    /// `0x00606CE0`; the game sets it from the battle file's `playable_area`).
    pub reinforcement_target: (f32, f32),
    /// The playable area `[min x, min y, max x, max y]` (battle `+0x84..+0x90`, read by the skirmish
    /// edge test `0x0057A6F0`); `None` = unbounded.
    pub playable_area: Option<[f32; 4]>,
    /// Deployable defences built when deployment ended (`0x00551BA0`), see [`super::abilities`].
    pub defences: Vec<super::abilities::Defence>,
    /// The battlefield's buildings (the map's near list, in file order), see [`super::garrison`].
    pub buildings: Vec<super::garrison::BattleBuilding>,
}

/// True if every living man of `u` where he stands ([`LandUnit::men_standing`]) has his circle
/// inside the area `[min x, min y, max x, max y]`, edges included: the soldier test of `0x005857A0`
/// (CONFIRMED: `min ≤ p − r` and `p + r ≤ max` on both axes, `0x0058582D` / `0x00585831`). True
/// without men, as the exe's empty loop. INFERRED: the soldier's circle radius (`+0x58`) is the man
/// radius the squad box reads ([`LandUnit::man_radius`], soldier `+0x620`).
fn men_inside(u: &LandUnit, a: [f32; 4]) -> bool {
    u.men_standing().all(|m| {
        let r = m.radius;
        a[0] <= m.pos.0 - r && m.pos.0 + r <= a[2] && a[1] <= m.pos.1 - r && m.pos.1 + r <= a[3]
    })
}

/// The training levels in the exe's enum order (`0x00EED960`, CONFIRMED static: the unit
/// record's `training_level` string mapped to 0..5, any other to 6): the index is
/// [`LandUnit::training_level`].
pub const TRAINING_LEVELS: [&str; 6] = ["mob", "rabble", "poorly_trained", "trained", "well_trained", "elite"];

/// The training level enum of a unit record's `training_level` key (`0x00EED960`): its index in
/// [`TRAINING_LEVELS`], `None` for any other key (the exe's 6).
pub fn training_level_of(key: &str) -> Option<u8> {
    TRAINING_LEVELS.iter().position(|k| *k == key).map(|i| i as u8)
}

/// The speed (m/s) a unit moves at this tick, from the speed it moved at last tick (`current`)
/// towards the speed it heads for (`target`).
///
/// CONFIRMED rule (the locomotion step `0x00819770`, once per 0.1 s tick): the speed rises by at
/// most `accel` × 0.1 per tick and falls by at most `decel` × 0.1, where `accel` is the entity's
/// acceleration (locomotive `+0x158`, `battle_entities` column 5) times the speed multiplier
/// `+0x1A4` (fatigue, ground and slope; the caller multiplies) and `decel` its deceleration
/// (`+0x100`, column 6, not multiplied). So a heavy horse ordered to run (2.5 m/s²) takes about
/// 3 s from its walk (2.6 m/s) to its run (10 m/s), passing through the trot and canter speeds,
/// and line infantry (2.4 m/s²) about 0.9 s from 1.4 to 3.6 m/s. An infinite rate (a unit built
/// without entity data) changes speed at once.
pub fn step_speed(current: f32, target: f32, accel: f32, decel: f32) -> f32 {
    let change = target - current;
    // `max` / `min` (not `clamp`): an infinite rate times a zero multiplier is NaN, which they skip.
    current + change.max(-decel * TICK_SECONDS).min(accel * TICK_SECONDS)
}

/// How far from its destination a unit moving at `speed` m/s starts braking: the wanted speed
/// drops to 0 once the distance left is at most `max(0.5, 0.5 / decel × speed²)` m, the distance it
/// needs to stop at its deceleration (at least half a metre). CONFIRMED: the soldier's move state
/// leaves for the stop state at that distance (`0x00807410`, the move state's check `0x007DE1F0`;
/// its distance left is the locomotive `+0x144`, its speed `+0x128`, its deceleration `+0x100`),
/// and the stop state sets the wanted speed `+0x148` to 0 (`0x00807080`), so `step_speed` slows the
/// unit at its deceleration. A unit without a deceleration (no entity data) does not brake.
pub fn arrival_braking(speed: f32, decel: f32) -> f32 {
    if decel.is_finite() && decel > 0.0 { (0.5 / decel * speed * speed).max(0.5) } else { 0.0 }
}

/// The slope factor of the speed multiplier `+0x1A4` for a gradient `g` (the height change over
/// the tick's step ahead, at least 0.01 m, divided by its length): uphill `1 / (1 + 3g)`, downhill
/// `min(1 − g, 1.5)`, level 1. CONFIRMED (`0x00819770`). The unit's move and each man's
/// (`super::march`) use this one rule.
pub fn slope_multiplier(gradient: f32) -> f32 {
    if gradient > 0.0 {
        1.0 / (gradient * 3.0 + 1.0)
    } else if gradient < 0.0 {
        (1.0 - gradient).min(1.5)
    } else {
        1.0
    }
}

/// Whether a soldier `left` metres from his destination at `speed` m/s brakes this tick
/// ([`arrival_braking`]; never without a finite positive deceleration `decel`). The exe stays in its
/// stop state once there; ours keeps braking by looking one tick ahead (the braking distance
/// shrinks as he slows, so a bare test would let go of the brake). The unit's move and each man's
/// (`super::march`) use this one rule.
pub fn brakes_for(left: f32, speed: f32, decel: f32) -> bool {
    decel.is_finite() && decel > 0.0 && left <= arrival_braking(speed, decel) + speed * TICK_SECONDS
}

/// The cap on a soldier's timed-move order speed: its battle entity's run speed (record `+0x20`)
/// times this. CONFIRMED (`0x0063F040`).
pub const TIMED_MOVE_SPEED_CAP: f32 = 1.45;

/// The order speed (m/s) of a timed move: a soldier that must reach a point `distance` m away in
/// `time_left` s at the move's speed `speed` speeds up by the distance it is behind, spread over the
/// time left, up to `cap` (run speed × [`TIMED_MOVE_SPEED_CAP`]). CONFIRMED (`0x0063AF40`, all f32):
/// - `time_left` ≤ 0 (or NaN): `speed`;
/// - else `speed + (distance − speed·time_left) / time_left`, where with `keep_speed` the
///   shortfall is floored at 0 (never slower than `speed`), and without it the result is plainly
///   `distance / time_left`, possibly slower;
/// - the result if it is under `cap`, else `cap` (also for NaN).
///
/// How the exe calls it (`0x00659600`, the soldier's move-order update, CONFIRMED): in the moving
/// states (4 / 5 of `+0x390`) of a move whose order block has the timed byte `+0x414` set, with
/// `speed` = `+0x40C`, `distance` = the soldier's ground distance to the order's destination
/// (`+0x3F4`), `time_left` = the planned time `+0x410` minus the time since the order `+0x43C`
/// (+0.1 per tick), and `keep_speed` = flag `+0x438` bit 0 clear. With `keep_speed` the speed is
/// computed on the first moving tick only (`+0x440`) and kept; without it, every tick; the time
/// since the order goes up after the speed is computed (`0x00659A91` reads it, `0x00659B49` adds
/// 0.1). The order block comes from the order object `0x007E2C80` fills from a descriptor (word 8
/// the speed, word 9 the planned time, word 10 the face flag that becomes `+0x408`), installed by
/// `0x006533E0`. Its use in our movement is [`TimedMove`].
pub fn timed_move_speed(speed: f32, cap: f32, distance: f32, time_left: f32, keep_speed: bool) -> f32 {
    if time_left.is_nan() || time_left <= 0.0 {
        return speed;
    }
    let mut behind = distance - speed * time_left;
    // MAXSS: a NaN shortfall becomes 0 as well.
    if keep_speed && (behind.is_nan() || behind <= 0.0) {
        behind = 0.0;
    }
    let v = behind / time_left + speed;
    if cap > v { v } else { cap }
}

/// Planned time (s) of the timed-move order a moving soldier is given (descriptor word 9).
/// INFERRED: the unit move order (`0x00584E10`, the source, see [`TimedMove`]) plans t = D / v with
/// D = n·k + 2v (`0x0054C650`). Statically (CONFIRMED, UNITS_TERRAIN_FIDELITY.md "n and k named")
/// n = the unit body's living-soldier count (`+0x510` → `+0x27C`, filled by `0x005307E0`, reduced by
/// `0x00574E90`) and k = the body's record copy of `unit_stats_land` column 45 `spacing_rank_close`
/// (`+0x13C`, record `+0x11C` from `0x00E8CFF0`), which for a full unit gives tens of seconds; but
/// the 2026-10-10 sitting's order speed rose ~0.035 a tick from 2.69 at multiplier 0.373, which
/// d / t fits only with t ≈ 5 s (t = 2 gives +0.089 a tick). Settled by reading n, k and t at
/// `0x0054C691` on a walk order and re-reading how `0x00659600` uses word 9. The same D is how far
/// ahead the men's places are laid out at each issue (`0x007F0790`, [`super::march`]), so D = v · t
/// either way.
pub const TIMED_MOVE_PLANNED_SECONDS: f32 = 5.0;

/// The issue time (s) of a unit move order issued at battle tick `tick`: the order's `+0x2C`.
/// CONFIRMED: the order's build (`BuildUnitMoveOrder` `0x0051A530`) and each re-issue (`0x00581B7C`)
/// store the battle model's tick count (`+0x58`, u32, read through order `+0x14` → `+8`) converted
/// to f32 (exact u32 → double, then rounded to float) times 0.1f (`0x0131A7B0`), in f32.
pub fn timed_move_issue_time(tick: u32) -> f32 {
    tick as f32 * TICK_SECONDS
}

/// Whether a unit move order last issued at `issued` ([`timed_move_issue_time`]) is re-issued at
/// battle tick `tick`. CONFIRMED: the order's update (vtable method `0x00581AE0`) re-issues the
/// soldiers' orders (`0x00581B81` → `0x00584E10`) when the tick's issue time is strictly above
/// `issued` + 1.0f (`0x01318048`, an f32 add; `COMISS`/`JBE` at `0x00581B73`/`0x00581B76`). In f32 the
/// 10-tick difference almost always rounds to exactly 1.0 (not above it), so an order is re-issued
/// on the 11th tick, and on the 10th for a few issue ticks (11, 32, 1274, 20 479 in a chain of
/// issues from tick 0). The counter is the battle model's (`StepBattleModelIfTickDue` `0x0057DF40`
/// adds 1 after each model tick's update, at `0x0057E0BC`; the model's ctor `0x00506C00` zeroes it
/// at `0x00506C85`), our [`Battle::tick`].
pub fn timed_move_reissue_due(issued: f32, tick: u32) -> bool {
    timed_move_issue_time(tick) > issued + 1.0
}

/// The timed-move order block of the unit's soldier (`+0x40C` speed, `+0x410` planned time,
/// `+0x43C` time since the order) and the distance left to the order's point.
///
/// The source (CONFIRMED): a moving unit's move order re-issues every soldier's timed move about
/// every 11 ticks ([`timed_move_reissue_due`]; `0x00581B81` → `0x00584E10` → `0x006533E0`), with the
/// order speed (descriptor word 8, `0x005855C7`) = the gait speed `0x00565030` before any ground or
/// slope factor, the planned time from `0x0054C650` (word 9, `0x005855F2`), and the face flag (word 10)
/// clear for a walk, so `+0x438` bit 0 is set and `0x00659600` recomputes d / t every tick
/// (`0x0063AF40`), neither held nor smoothed: the ground column and slope multiply only the wanted
/// speed in the step (`+0x1A4`, `0x00819D5F`), whose limits (acceleration 3.5 × multiplier,
/// deceleration 10 m/s² for the horse of the sitting) pass both the rise and the drop at once. So
/// on slowed ground the soldier falls behind its plan, d / t rises, and each re-issue restarts it
/// at the gait speed: a sawtooth. The 2026-10-10 sitting measured it in the original (light cavalry
/// horse in forest, ground 0xE, `+0x1A4` 0.373: order speed `+0x10C` 2.69 rising ~0.035 a tick to
/// 3.07, back to 2.69 at the re-issue; wanted 1.0..1.1 m/s). The per-soldier formation follow
/// `0x006DC700` (every 10th call, t = 5) is not this source: it is the artillery crew following
/// its gun (its update `0x006D7720` is in the six artillery-piece vtables, ctor `0x0068F610`).
/// Ours: the unit's own move is the reference soldier's (d is the unit's distance to the issue
/// point, [`TIMED_MOVE_PLANNED_SECONDS`] of travel ahead or the path's end, [`issue_point`]), and
/// each man follows the same order at his own speed from his own place ([`super::march`]); the
/// planned time stays INFERRED there.
///
/// At the path's end (CONFIRMED, `0x00584E10`): once the issue point reaches the end (`0x007FF1C0`
/// on the carried cursor, order `+0x3C`), descriptor word 10 (`+0x28` of the descriptor) is set
/// for an order whose `+0x28` byte is set, as every order of ours ([`issue_point`]); the soldier's
/// `+0x408` takes it, and `0x00659980..0x00659A00` then leaves `+0x438` bit 0 clear, so the order
/// keeps its speed: d / t computed once on the first moving tick (`+0x440`) and floored at v. Without
/// it a soldier would slow as d / t to his point at the end. PROVISIONAL: the bit stays clear only
/// when the soldier's byte `+0x3A4` is set too, whose writer is not traced (not in the order block
/// `0x00659520` copies); ours takes it as set. Ours computes the kept speed on the issue tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedMove {
    /// The order's speed `+0x40C`: the move's gait speed when it was planned.
    pub speed: f32,
    /// Metres left to the order's point (the soldier's distance to `+0x3F4`).
    pub distance: f32,
    /// Seconds since the order (`+0x43C`).
    pub elapsed: f32,
    /// Ticks since the move's last plan.
    pub ticks: u32,
    /// The issue time of the last plan (`+0x2C`, [`timed_move_issue_time`]).
    pub issued: f32,
    /// The issue point reached the path's end (the unit move order's `+0x3C`): the order keeps
    /// its speed (see the type's note) and a re-issue skips the men whose move finished.
    pub at_end: bool,
    /// The move's first issue (`0x0051A8FD`, `0x00584E10` with 1: every man is issued), not a
    /// re-issue (`0x00581B81`, with 0: gated, [`super::march`]).
    pub first: bool,
    /// The kept order speed once computed (`+0x440`), for an order at the path's end: the unit's
    /// own soldier's (each man keeps his own, [`super::march::ManMarch::kept`]).
    pub kept: Option<f32>,
    /// The enemy position the order was built to reach, for a move that follows an enemy unit (the
    /// advance on it, the approach to a fire target); `None` for an ordered destination or a rout. A
    /// target that moves is a new order: [`TimedMove::next`].
    pub target: Option<(f32, f32)>,
}

/// The issue point of a unit move order: the metres from the reference soldier's place on the path
/// to the point the cursor is carried to, and whether that point is the path's end. CONFIRMED
/// (`0x00584E10`): the cursor is carried `ahead` metres (D = v · t) along the path by `0x007F0790`,
/// which past the last node either extrapolates beyond the end or stops at it; its flag (computed
/// at `0x00584E54..0x00584E7B`) stops it only for an order with no destination layout (order
/// `+0x20` = 0) and its `+0x28` byte set. The order takes them from its descriptor's `+8` and
/// `+0x10`: the plain move's descriptor (`0x0051A970`, also called at `0x00565576` and
/// `0x005810FC`) has `+8` = 0 and `+0x10` = 1, and `0x0065D740` builds it when its command has no
/// heading (`+0x40` clear); with a heading it builds `0x0051A910` (`+8` = the layout from
/// `0x00649B30`, `+0x10` = 1), which extrapolates past the end. Ours has orders without a
/// destination layout only, so the point stops at the end: at most `path_left`. PROVISIONAL: which
/// of our order sources maps to which descriptor is not traced, and `0x0051A940` (`+0x10` from its
/// caller `0x0054D9F0`) would extrapolate with a 0 there. Then `0x007FF1C0` on the carried cursor
/// (the last segment, fraction ≥ 1) is the "at the end" flag (`+0x3C`).
pub fn issue_point(ahead: f32, path_left: f32) -> (f32, bool) {
    if ahead < path_left { (ahead, false) } else { (path_left.max(0.0), true) }
}

impl TimedMove {
    /// A new order at `speed` (following `target`, if any) on a path with `path_left` metres left:
    /// its point [`TIMED_MOVE_PLANNED_SECONDS`] of travel ahead, or the path's end ([`issue_point`]).
    /// A first issue at battle tick `tick`; [`TimedMove::next`] marks a re-issue.
    pub fn plan(speed: f32, path_left: f32, target: Option<(f32, f32)>, tick: u32) -> Self {
        let (distance, at_end) = issue_point(speed * TIMED_MOVE_PLANNED_SECONDS, path_left);
        let issued = timed_move_issue_time(tick);
        TimedMove { speed, distance, elapsed: 0.0, ticks: 0, issued, at_end, first: true, kept: None, target }
    }

    /// The order to follow at battle tick `tick` for a move at `speed` following `target` (if any) with
    /// `path_left` metres left: this one, or a new plan when there is none (a new move order clears
    /// it, [`LandUnit::set_destination`]: a first issue), its target moved (a first issue, below), the
    /// move's speed changed (a new gait) or it is due again ([`timed_move_reissue_due`]: a re-issue).
    ///
    /// A moved target (CONFIRMED mechanism): the exe's order never re-targets itself (`0x00581AE0`
    /// keeps its path); a state that follows a target (update `0x0054D9F0`) re-reads the target
    /// every tick (its object `+0x28`, vfunc `+4`) and, when that differs from the last read
    /// (`+0x38`), builds a new unit move order (`0x0051A940` descriptor, `BuildUnitMoveOrder`
    /// `0x0051A530`), which issues every man afresh (`0x00584E10` with 1) and restarts the issue time
    /// `+0x2C`. PROVISIONAL: the value compared is an int whose meaning (how far the target must move
    /// to count) is not traced; ours compares the enemy's position itself.
    ///
    /// PROVISIONAL: a gait change counts as a re-issue; whether the run toggle (`0x005600C0`)
    /// rebuilds the unit move order (a first issue) is not traced.
    pub fn next(current: Option<TimedMove>, speed: f32, path_left: f32, target: Option<(f32, f32)>, tick: u32) -> Self {
        match current {
            Some(t) if t.target != target => TimedMove::plan(speed, path_left, target, tick),
            Some(t) if t.speed == speed && !timed_move_reissue_due(t.issued, tick) => t,
            Some(_) => TimedMove { first: false, ..TimedMove::plan(speed, path_left, target, tick) },
            None => TimedMove::plan(speed, path_left, target, tick),
        }
    }

    /// This tick's order speed (m/s) for a soldier `distance` metres from his point who has kept
    /// `kept` (his own, for an order at the path's end), capped at `run_speed` ×
    /// [`TIMED_MOVE_SPEED_CAP`]: d / t, kept and floored at v at the path's end (see the type's
    /// note). The unit's own soldier and each man ([`super::march`]) use this one rule.
    pub fn speed_for(&self, run_speed: f32, distance: f32, kept: &mut Option<f32>) -> f32 {
        if let Some(k) = *kept {
            return k;
        }
        let time_left = TIMED_MOVE_PLANNED_SECONDS - self.elapsed;
        let v = timed_move_speed(self.speed, run_speed * TIMED_MOVE_SPEED_CAP, distance, time_left, self.at_end);
        if self.at_end {
            *kept = Some(v);
        }
        v
    }

    /// The unit's own soldier's order speed this tick ([`TimedMove::speed_for`]).
    pub fn order_speed(&mut self, run_speed: f32) -> f32 {
        let mut kept = self.kept;
        let v = self.speed_for(run_speed, self.distance, &mut kept);
        self.kept = kept;
        v
    }

    /// After the tick's step of `covered` metres: the point is that much nearer and 0.1 s passed.
    pub fn advance(&mut self, covered: f32) {
        self.distance -= covered;
        self.elapsed += TICK_SECONDS;
        self.ticks += 1;
    }
}

/// Squared distance between two points.
pub(super) fn dist2(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    dx * dx + dy * dy
}

impl Battle {
    /// A new battle at tick 0 with the given RNG seed and game data. `kv_rules` starts as all
    /// zeros; set the public field or use [`Battle::with_rules`].
    pub fn new(seed: u32, kv_morale: KvMorale, kv_fatigue: KvFatigue) -> Self {
        Self::with_rules(seed, kv_morale, kv_fatigue, KvRules::default())
    }

    /// A new battle at tick 0 with all three kv tables.
    pub fn with_rules(
        seed: u32,
        kv_morale: KvMorale,
        kv_fatigue: KvFatigue,
        kv_rules: KvRules,
    ) -> Self {
        Battle {
            tick: 0,
            rng: CaRng::new(seed),
            units: Vec::new(),
            kv_morale,
            kv_fatigue,
            kv_rules,
            weather: Weather::Clear,
            weather_intensity: 0,
            climate_fatigue: (0, 0),
            experience_fatigue: Vec::new(),
            unit_scale: super::unit_scale::MAX,
            unit_scale_top: super::unit_scale::MAX,
            volleys: Vec::new(),
            ground: std::sync::Arc::new(BattleGround::default()),
            side_start_strength: Vec::new(),
            general_died: Vec::new(),
            reinforcement_entries: Vec::new(),
            max_reinforcement_units: crate::limits::ORIGINAL_MAX_REINFORCEMENT_UNITS as usize,
            reinforcement_target: (0.0, 0.0),
            playable_area: None,
            defences: Vec::new(),
            buildings: Vec::new(),
        }
    }

    /// The men a unit card with `card_men` men has in this battle: the card's count scaled by
    /// [`Battle::unit_scale`] and truncated, the way `0x004A6600` builds the battle unit card's
    /// `+0xC8`/`+0xCC` (CONFIRMED). Battle-file units do not go through this in the exe (it hands
    /// `num_soldiers` to the card unchanged, §39); whether a caller applies it to them is the
    /// caller's choice.
    pub fn men_at_scale(&self, card_men: i32) -> u32 {
        // The exe clamps once per battle (0x004A6540) before any multiplication; clamping here too
        // is idempotent for an already-clamped scale and keeps a bad setting from reaching the men.
        super::unit_scale::scaled_men(card_men, super::unit_scale::clamp(self.unit_scale, self.unit_scale_top))
    }

    /// Sets [`Battle::unit_scale`] from a `unit_scale` option **index** (the steps of
    /// `limits.unit_scales`, the original's four [`super::unit_scale::STEPS`] by default) and
    /// returns the clamped float the battle will use, so the caller can log the step it picked.
    pub fn set_unit_scale_step(&mut self, index: i32, limits: &crate::limits::GameLimits) -> f32 {
        self.unit_scale_top = limits.max_scale();
        self.unit_scale = super::unit_scale::clamp(limits.unit_scale(index), self.unit_scale_top);
        self.unit_scale
    }

    /// Sets [`Battle::unit_scale`] from the `gfx_unit_scale` **preference index** the settings gave
    /// us, `None` meaning "no preferences file" (the model's 1.0), and returns the clamped float.
    /// The whole preference → scale decision is [`super::unit_scale::scale_for_setting`].
    pub fn set_unit_scale_setting(&mut self, setting: Option<i32>, limits: &crate::limits::GameLimits) -> f32 {
        self.unit_scale_top = limits.max_scale();
        self.unit_scale = super::unit_scale::scale_for_setting(setting, limits);
        self.unit_scale
    }

    /// Each side's current strength (alliance `+0x64`, `0x00539E80`), ascending side order.
    pub fn side_strengths(&self) -> Vec<(u8, f32)> {
        let sides: BTreeSet<u8> = self.units.iter().map(|u| u.side).collect();
        sides
            .into_iter()
            .map(|s| {
                let sum = self.units.iter().filter(|u| u.side == s).map(super::strength::strength).sum();
                (s, sum)
            })
            .collect()
    }

    /// Adds a unit, keeping `units` sorted by id (so the update order is deterministic), with its
    /// box measured where it stands ([`LandUnit::remeasure`]), so a unit that does not tick yet (a
    /// waiting reinforcement) is not measured again at every read.
    pub fn add_unit(&mut self, mut unit: LandUnit) {
        unit.remeasure();
        let pos = self.units.partition_point(|u| u.id < unit.id);
        self.units.insert(pos, unit);
    }

    /// Battle time in seconds.
    pub fn time_seconds(&self) -> f32 {
        self.tick as f32 * TICK_SECONDS
    }

    /// Index of the nearest enemy of unit `idx` that passes `filter`. Ties go to the earlier unit.
    fn nearest_enemy(&self, idx: usize, filter: impl Fn(&LandUnit) -> bool) -> Option<usize> {
        let me = &self.units[idx];
        let mut best: Option<(usize, f32)> = None;
        for (j, u) in self.units.iter().enumerate() {
            if u.side == me.side || !filter(u) {
                continue;
            }
            let d = dist2(me.position, u.position);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
        best.map(|(j, _)| j)
    }

    /// Advances the battle by one 0.1 s tick.
    pub fn step(&mut self) {
        let tick = self.tick;
        if self.side_start_strength.is_empty() {
            self.side_start_strength = self.side_strengths();
        }
        self.note_general_deaths();
        self.reinforcements_step();
        // Clear the per-tick flags for ALL units first. A melee encounter is resolved by the
        // first of the two units to update, and it marks BOTH units as in melee. If each unit
        // cleared its own flag at the start of its own update, the second unit would lose the
        // mark and count as idle, so the higher-id side never tired. (Bug found by the manager.)
        for u in &mut self.units {
            u.in_melee = false;
            u.moved = false;
            u.fired_this_tick = false;
            u.aiming = false;
            u.gradient = 0.0;
        }
        self.volleys.clear();
        for idx in 0..self.units.len() {
            self.update_unit(idx, tick);
        }
        self.tick = self.tick.wrapping_add(1);
    }

    /// The per-unit update, in the order of `0x0057F070` (W1 §5 item 6, CONFIRMED order).
    fn update_unit(&mut self, idx: usize, tick: u32) {
        if self.units[idx].men == 0 {
            // PLACEHOLDER: how the original retires a destroyed unit is UNKNOWN.
            let u = &mut self.units[idx];
            u.active = false;
            u.morale.state = MoraleState::Shattered;
            u.morale.behaviour = MoraleBehaviour::Shattered;
        }
        // The "under fire" timers (`shooting::UNDER_FIRE_MEMORY_TICKS`) run down one per tick (slot 4
        // `0x005828A0`, CONFIRMED; here at the start of the unit's own update).
        let u = &mut self.units[idx];
        u.under_fire_ticks = u.under_fire_ticks.saturating_sub(1);
        u.under_artillery_fire_ticks = u.under_artillery_fire_ticks.saturating_sub(1);
        // The cartridge pool (`shooting::AmmoPool`, CONFIRMED structure): made at the unit's first
        // update, then it loses the share of every man lost since the last update.
        if u.missile.is_some_and(|w| !w.is_artillery) {
            let (men, per_man) = (u.men, u.ammunition);
            let pool = u.ammo_pool.get_or_insert_with(|| AmmoPool::new(men, per_man));
            pool.losses(men);
            u.ammunition = pool.rounds_per_man();
        }

        // 1. FUN_0056CBF0(0): UNKNOWN (W1 §10), nothing here.
        // 2. FUN_005857A0: UNKNOWN. PLACEHOLDER: our straight-line movement runs in this slot.
        //    The skirmish behaviour (check and evading state, see `abilities`) runs first: the
        //    evading state sets the destination the movement then follows (INFERRED slot).
        //    Leaving the field (`0x005857A0`, CONFIRMED): see `Battle::leave_step`.
        self.leave_step(idx);
        self.skirmish_step(idx, tick);
        self.placeholder_movement(idx);
        //    Garrisons: enter a building on arrival, leave it on a new move (see `garrison`).
        self.garrison_step(idx);
        // 3. FUN_005821B0: UNKNOWN, nothing here.
        // 4. FUN_005828A0: UNKNOWN. APPROXIMATION: the melee exchanges of this unit's encounter run
        //    in this slot (in the original they come from 0x00664E80 → 0x006AFE20, whose place in
        //    the frame is not yet known).
        //    The contact clock that ends a charge's impact (`LandUnit::charging_now`).
        let in_contact = self.melee_contact(idx).is_some();
        let u = &mut self.units[idx];
        if in_contact {
            u.contact_ticks = u.contact_ticks.saturating_add(1);
            u.out_of_contact_ticks = 0;
        } else {
            u.out_of_contact_ticks = u.out_of_contact_ticks.saturating_add(1);
            if u.out_of_contact_ticks >= MELEE_EXCHANGE_INTERVAL_TICKS {
                u.contact_ticks = 0;
            }
        }
        self.melee_step(idx, tick);
        //    PROVISIONAL: missile fire (reload, target, volley) also runs in this slot, right after
        //    melee. Where the original updates missile fire inside the frame is UNKNOWN (W1 §12.2
        //    confirms only the 0.1 s tick and the morale stagger). See `shooting`.
        self.missile_step(idx, tick);
        //    The casualty log of slot 4 `0x005828A0` (CONFIRMED; see `casualties`): ratios and category.
        let u = &mut self.units[idx];
        u.casualty_log.update(u.men, u.kills, tick);
        u.category = u.casualty_log.category;
        // 5. Visibility / event check FUN_006011F0: not modelled yet.

        // 6. Morale update 0x00582540(0, unit_id % 5 == tick % 5). CONFIRMED stagger.
        if self.units[idx].id % 5 == tick % 5 {
            let inputs = self.morale_inputs(idx);
            let kv = self.kv_morale;
            let u = &mut self.units[idx];
            morale::evaluate(&mut u.morale, &kv, &inputs);
            u.last_full_morale_tick = Some(tick);
            u.recent_losses = 0; // PLACEHOLDER: "recent" = since the last full evaluation
        } else {
            let kv = self.kv_morale;
            let charging = self.units[idx].charging;
            morale::light_update(&mut self.units[idx].morale, &kv, charging);
        }

        // 7. Per-soldier averages (+0xC74 / +0xC70). PLACEHOLDER: one representative soldier,
        //    whose fatigue tick we run here; the "mean" of one soldier is itself.
        let weather = self.weather;
        let climate = self.climate_fatigue;
        let kvf = self.kv_fatigue;
        let u = &mut self.units[idx];
        // The soldier fatigue function 0x00670F40 maps the soldier's action (+0x1B8) to only nine
        // kv_fatigue keys: idle (+ rain/snow), ready, shooting, reloading, working, walking,
        // running, charging, combat (CONFIRMED byte table 0x006711D8). It never reads the
        // under-fire, artillery, cavalry, limbering or tight-formation keys, so a unit standing
        // under fire is idle and an artillery crew reloading uses `reloading` (INFERRED: the crew's
        // reload actions are the reloading group). Which of our unit-level situations is which
        // action stays an APPROXIMATION (the model has no per-soldier actions).
        let action = if u.in_melee {
            FatigueAction::Combat
        } else if u.moved && u.charging {
            FatigueAction::Charging
        } else if u.moved && (u.morale.is_routing_or_shattered() || u.running) {
            FatigueAction::Running
        } else if u.moved {
            FatigueAction::Walking
        } else if u.fired_this_tick {
            FatigueAction::Shooting
        } else if u.aiming && u.reload_ticks_left > 0 {
            FatigueAction::Reloading
        } else {
            FatigueAction::Idle
        };
        let fi = FatigueInputs {
            action,
            weather,
            // The exe's test is the soldier predicate 0x0064CAA0: `+0x118 == 1 && (+0x110 & 1 ||
            // +0x117 == 0)` (CONFIRMED; the three fields' meanings are UNKNOWN). PROVISIONAL: we
            // read it as "moved this tick", and the gradient (soldier +0x1A0, writer not found) as
            // the height change over the distance moved (only uphill reaches the thresholds).
            moving_on_slope: u.moved,
            gradient: u.gradient,
            unit_flag_18e: u.attributes.good_stamina,
            climate_term: fatigue::climate_term(climate.0, climate.1, u.attributes.climate_exempt_2c, u.attributes.climate_exempt_30),
            // Column 6 (+0x20) of the unit's `unit_stats_land_experience_bonuses` row, picked by
            // its experience byte (+0xD48): veterans tire more slowly (CONFIRMED, 0x00670F40).
            experience_bonus: fatigue::experience_bonuses(&self.experience_fatigue, u.experience),
        };
        fatigue::tick(&kvf, &mut u.fatigue, &mut u.fatigue_state, &fi);
        u.fatigue = fatigue::unit_fatigue(&[u.fatigue]);

        // 8. Inactive units: every soldier's fatigue and fatigue state reset to 0. CONFIRMED.
        if !u.active {
            u.fatigue = 0;
            u.fatigue_state = FatigueState::Fresh;
        }
        // 9. Commander/ability handling and FUN_00582830: not modelled yet.
    }

    /// Where unit `idx` is heading this tick, at what speed, and whether the goal is its ordered
    /// destination (cleared on arrival): the goal selection of the PLACEHOLDER movement. The
    /// skirmish code reads it for other units as the original reads an entity's movement target
    /// (`0x0057EEE0`).
    pub(super) fn movement_goal(&self, idx: usize) -> Option<((f32, f32), f32, bool)> {
        let routing = self.units[idx].morale.is_routing_or_shattered();
        let pos = self.units[idx].position;
        if routing {
            self.nearest_enemy(idx, |u| u.men > 0 && u.active).map(|j| {
                let e = self.units[j].position;
                // A point directly away from the enemy.
                (
                    (2.0 * pos.0 - e.0, 2.0 * pos.1 - e.1),
                    self.units[idx].run_speed,
                    false,
                )
            })
        } else if let Some(d) = self.units[idx].destination {
            let u = &self.units[idx];
            Some((d, u.move_speed(), true))
        } else if let Some(t) = self.ordered_fire_target_to_approach(idx) {
            // PLACEHOLDER: move towards an ordered fire target until it is within range, at the speed of
            // the current move (the run option applies to whatever move the unit makes, 0x005600C0).
            Some((self.units[t].position, self.units[idx].move_speed(), false))
        } else if self.units[idx].hold_position {
            None
        } else {
            let me = &self.units[idx];
            // PLACEHOLDER: units that can shoot (and fire at will) stop at firing range;
            // the others close to melee contact. The run option applies here too (0x005600C0 edits
            // whatever move the unit is making).
            let stop = if me.can_shoot() && me.fire_at_will {
                me.missile_range(self.kv_rules.fire_on_walls_range_modifier)
                    * MISSILE_ADVANCE_STOP_FRACTION
            } else {
                MELEE_CONTACT_RANGE * ADVANCE_STOP_FRACTION
            };
            self.nearest_enemy(idx, |u| !u.is_out_of_fight())
                .and_then(|j| {
                    let e = self.units[j].position;
                    if dist2(pos, e) > stop * stop {
                        Some((e, self.units[idx].move_speed(), false))
                    } else {
                        None
                    }
                })
        }
    }

    /// PLACEHOLDER movement: routing units run straight away from the nearest enemy; others walk to
    /// their destination, or (if they have none) advance on the nearest fighting enemy.
    fn placeholder_movement(&mut self, idx: usize) {
        // Arriving reinforcements walk in before they count as on the field.
        let arriving = matches!(self.units[idx].reinforcement, Reinforcement::Arriving { .. });
        // An inactive unit that is not arriving is destroyed or has left the field (`update_unit`)
        // and never moves again: its men's march stays as it is (none of them is drawn).
        if !self.units[idx].active && !arriving {
            return;
        }
        // The squad summary's refresh, then the men's re-slot after losses (`SQUAD::update`
        // `0x00580A30`, `march`).
        let tick = self.tick;
        self.units[idx].refresh_summary(tick);
        self.units[idx].reslot_tick(tick);
        let pos = self.units[idx].position;
        let Some((goal, speed, is_order)) = self.movement_goal(idx) else {
            // No move left: the run option went with it (it belongs to the move, `0x005600C0`).
            // An ordered move has braked to its destination before this (`arrival_braking`); a
            // move with no goal left otherwise (an advance that came into range) ends at once: that
            // goal and its stop belong to the stand-in goal choice of `movement_goal`.
            self.units[idx].running = false;
            self.units[idx].speed = 0.0;
            self.units[idx].timed_move = None;
            // The men still behind their places walk the rest (`march`).
            self.march_men(idx, None, None);
            return;
        };
        let dx = goal.0 - pos.0;
        let dy = goal.1 - pos.1;
        let len = (dx * dx + dy * dy).sqrt();
        // Ground type under the unit scales its speed (`unit_movement_modifiers`, see `ground`).
        let modifier = self.ground.speed_modifier(pos.0, pos.1, self.units[idx].movement_class);
        let ground = std::sync::Arc::clone(&self.ground);
        // The timed-move order (`TimedMove`, the unit move order re-issued about every 11 ticks): the order speed the soldier heads
        // for before the multiplier, from the move's gait speed, to a point at most the path's end.
        // A move that follows an enemy unit (the advance, the fire-target approach) is a new order
        // when the enemy moved (`TimedMove::next`); an ordered destination is fixed, and a rout's
        // point moves with the unit itself.
        let target = (!is_order && !self.units[idx].morale.is_routing_or_shattered()).then_some(goal);
        let mut timed = TimedMove::next(self.units[idx].timed_move, speed, len, target, self.tick);
        let order_speed = timed.order_speed(self.units[idx].run_speed);
        let planned_order = timed;
        // The fatigue speed multiplier (`0x006543D0`, `FatigueEffects::speed`).
        let speed = speed * self.units[idx].fatigue_effect().speed;
        let planned = (speed * modifier * TICK_SECONDS).max(0.01);
        // The slope (`0x00819770`, CONFIRMED, `slope_multiplier`): the gradient `+0x1A0` is the
        // height change over this tick's step ahead (at least 0.01 m) divided by its length.
        let gradient = if len > 0.0 {
            let ahead = (pos.0 + dx / len * planned, pos.1 + dy / len * planned);
            (ground.height(ahead.0, ahead.1) - ground.height(pos.0, pos.1)) / planned
        } else {
            0.0
        };
        // The speed multiplier (the soldier's `+0x1A4`: fatigue and ground from `0x006543D0`, then
        // the slope) scales both the speed the unit heads for and how fast it gets there.
        let multiplier = self.units[idx].fatigue_effect().speed * modifier * slope_multiplier(gradient);
        let u = &self.units[idx];
        // Braking for an ordered destination (`brakes_for`): the wanted speed drops to 0.
        let braking = is_order && brakes_for(len, u.speed, u.deceleration);
        let wanted = if braking { 0.0 } else { order_speed * multiplier };
        let moving = step_speed(u.speed, wanted, u.acceleration * multiplier, u.deceleration);
        let step = moving * TICK_SECONDS;
        // Braked to a stop short of the destination (by at most the 0.5 m minimum braking distance):
        // the move ends where the unit stands, as the exe's stop state keeps the soldier there.
        let stopped = braking && moving <= 0.0;
        // An enemy defence in the way stops the move; chevaux de frise and stakes also end a charge
        // (PROVISIONAL, see `abilities::Battle::defence_in_the_way`).
        let next = if len <= step { goal } else { (pos.0 + dx / len * step, pos.1 + dy / len * step) };
        if len > 0.0
            && let Some(d) = self.defence_in_the_way(idx, pos, next)
        {
            self.units[idx].facing = dy.atan2(dx);
            self.defence_contact(idx, &d);
            let u = &mut self.units[idx];
            u.speed = 0.0;
            u.timed_move = None;
            if d.stops_charges() {
                u.charging = false;
            }
            // Its men walk what is left of their lags (`march`).
            self.march_men(idx, None, None);
            return;
        }
        let u = &mut self.units[idx];
        u.defence_contact = false;
        // The speed is what the unit covered (the soldier's `+0x194` is measured from its move,
        // `0x007F0260`): short of `moving` when it arrives.
        u.speed = len.min(step) / TICK_SECONDS;
        timed.advance(len.min(step));
        u.timed_move = Some(timed);
        if len <= step {
            u.position = goal;
        } else if len > 0.0 {
            u.position = (pos.0 + dx / len * step, pos.1 + dy / len * step);
        }
        if is_order && (len <= step || stopped) {
            u.set_destination(None);
            u.running = false;
        }
        if len > 0.0 && step > 0.0 {
            u.facing = dy.atan2(dx);
            u.moved = true;
            u.gradient = gradient;
        }
        // Its men follow the same order, each at his own speed (`march`), laid out at the unit's
        // facing after the step (the drawn one).
        let dir = if len > 0.0 { (dx / len, dy / len) } else { (0.0, 0.0) };
        let (facing, fatigue) = (self.units[idx].facing, self.units[idx].fatigue_effect().speed);
        let unit_step = super::march::UnitStep { pos, facing, dir, len, covered: len.min(step), is_order, fatigue };
        self.march_men(idx, Some(unit_step), Some(planned_order));
    }

    /// The enemy unit that unit `idx` is in melee contact with (nearest active enemy with men,
    /// within [`MELEE_CONTACT_RANGE`]).
    pub(super) fn melee_contact(&self, idx: usize) -> Option<usize> {
        let u = &self.units[idx];
        if !u.active || u.men == 0 {
            return None;
        }
        let j = self.nearest_enemy(idx, |e| e.men > 0 && e.active)?;
        let r2 = MELEE_CONTACT_RANGE * MELEE_CONTACT_RANGE;
        (dist2(u.position, self.units[j].position) <= r2).then_some(j)
    }

    /// Melee for the encounter between unit `idx` and its contact enemy, using the real W1 §12.9
    /// blow pipeline.
    ///
    /// **APPROXIMATED** (the model is unit-level, with no individual soldiers):
    /// - An "encounter" is a pair of units in contact. It is resolved once per tick, by whichever of
    ///   the two is updated first.
    /// - Engaged men per unit = `min(men, MELEE_FRONTAGE_MEN)`, and engaged pairs = the smaller of
    ///   the two. Each pair starts one exchange every `MELEE_EXCHANGE_INTERVAL_TICKS`, spread
    ///   deterministically over the ticks (`pairs / I` per tick, plus 1 when `tick % I < pairs % I`).
    /// - Each exchange builds a small local encounter list: unit `idx`'s soldiers first, then the
    ///   enemy's. Each side contributes `clamp(engaged_own / engaged_enemy, 1, MELEE_MAX_LOCAL_PER_SIDE)`
    ///   members, so a side that outnumbers the other gets `attackers_on_target > 1`.
    /// - Every soldier carries its unit's stats. Routing/shattered soldiers get weight 0, so they
    ///   are never picked as attacker, but they can still be struck.
    /// - Knockdown, knockback and stepback have no effect yet (no soldier positions or animations).
    ///   Kill removes one man.
    ///
    /// **Faithful** within each exchange (W1 §12.9 RNG order): roll #1 picks the attacker by weight
    /// among the highest-priority members; rolls #2..k pick the defender until it is an enemy; one
    /// 1..1000 roll resolves the blow; after a charging attacker's MISS, the defender strikes back
    /// with one more roll (`attackers_on_target` = 1 for the strike back, INFERRED).
    fn melee_step(&mut self, idx: usize, tick: u32) {
        let Some(j) = self.melee_contact(idx) else {
            return;
        };
        // Already resolved this tick by `j` (updated earlier) if `j` fights us back.
        if j < idx && self.melee_contact(j) == Some(idx) {
            return;
        }
        if self.units[idx].is_out_of_fight() && self.units[j].is_out_of_fight() {
            return;
        }
        let engaged_i = self.units[idx].men.min(MELEE_FRONTAGE_MEN);
        let engaged_j = self.units[j].men.min(MELEE_FRONTAGE_MEN);
        let pairs = engaged_i.min(engaged_j);
        let interval = MELEE_EXCHANGE_INTERVAL_TICKS;
        let exchanges = pairs / interval + u32::from(tick % interval < pairs % interval);
        for _ in 0..exchanges {
            if self.units[idx].men == 0 || self.units[j].men == 0 {
                break;
            }
            self.melee_exchange(idx, j);
        }
        self.units[idx].in_melee = true;
        self.units[j].in_melee = true;
    }

    /// One exchange between units `a` and `b` (see [`Battle::melee_step`]). Returns the blow
    /// outcomes in order (one, or two with a strike back), as `(attacker unit index, outcome)`.
    fn melee_exchange(&mut self, a: usize, b: usize) -> Vec<(usize, BlowOutcome)> {
        let ea = self.units[a].men.clamp(1, MELEE_FRONTAGE_MEN);
        let eb = self.units[b].men.clamp(1, MELEE_FRONTAGE_MEN);
        let ka = (ea / eb).clamp(1, MELEE_MAX_LOCAL_PER_SIDE) as usize;
        let kb = (eb / ea).clamp(1, MELEE_MAX_LOCAL_PER_SIDE) as usize;
        let member = |u: &LandUnit| {
            let fighting = !u.is_out_of_fight();
            EncounterMember {
                side: u.side,
                priority: melee::priority(u.charging_now() && fighting, 0, false),
                weight: if fighting {
                    melee::selection_weight(u.melee_attack, 0)
                } else {
                    0.0
                },
            }
        };
        let mut list = vec![member(&self.units[a]); ka];
        list.extend(std::iter::repeat_n(member(&self.units[b]), kb));

        let Some(pair) = melee::select_pair(&mut self.rng, &list) else {
            return Vec::new();
        };
        let (att, def) = if pair.attacker < ka { (a, b) } else { (b, a) };
        let mut out = Vec::new();
        let first = self.resolve_unit_blow(att, def, pair.attackers_on_target);
        out.push((att, first));
        if pair.attacker_charging && first == BlowOutcome::Miss {
            let back = self.resolve_unit_blow(def, att, 1);
            out.push((def, back));
        }
        out
    }

    /// Resolves one blow from a soldier of unit `att` on a soldier of unit `def` and applies it.
    fn resolve_unit_blow(
        &mut self,
        att: usize,
        def: usize,
        attackers_on_target: u32,
    ) -> BlowOutcome {
        let (ua, ud) = (&self.units[att], &self.units[def]);
        let attacker = ua.combatant();
        let defender = ud.combatant();
        let inputs = MeleeInputs {
            attacker,
            defender,
            dir: melee_dir(ud.position, ud.facing, ua.position),
            // APPROXIMATION: flat battlefield, so the height delta is 0 (W1 §12.9 0x006CCAB0 is in
            // `melee::height_delta` for when terrain exists).
            height_delta: melee::height_delta(
                0.0,
                0.0,
                dist2(ua.position, ud.position),
                attacker.charging || defender.charging,
            ),
        };
        let rules = self.kv_rules;
        let outcome = melee::resolve_blow(&mut self.rng, &inputs, attackers_on_target, &rules);
        self.units[att].blows_resolved = self.units[att].blows_resolved.wrapping_add(1);
        // INFERRED: an invincible unit (`set_invincible`) loses no men to blows either.
        if outcome == BlowOutcome::Kill && self.units[def].men > 0 && !self.units[def].invincible {
            self.units[def].men -= 1;
            self.units[def].recent_losses += 1;
            self.units[att].kills += 1;
        }
        outcome
    }

    /// The rally test `0x0055C500` for unit `idx` ([`morale::rally_test`], CONFIRMED structure).
    /// Enemies count when within 80 m ([`morale::near`]) and still on the field (`left_field`);
    /// "engaged" (`FUN_0054EE90`) is read as being in melee contact (INFERRED). The steadfast
    /// attribute (unit `+0x189`) lowers the bar to ½ (CONFIRMED).
    pub(super) fn can_rally(&self, idx: usize) -> bool {
        let u = &self.units[idx];
        if u.morale.behaviour != MoraleBehaviour::Routing {
            return false;
        }
        let enemies: Vec<f32> = self
            .units
            .iter()
            .filter(|e| {
                e.side != u.side
                    && e.men > 0
                    && !e.left_field
                    && morale::near_units(u, e, 80.0)
            })
            .map(super::strength::strength)
            .collect();
        morale::rally_test(
            &u.morale,
            u.men,
            u.max_men as i32,
            self.melee_contact(idx).is_some(),
            super::strength::strength(u),
            u.attributes.steadfast,
            &enemies,
            u.active,
        )
    }

    /// Builds the morale inputs for unit `idx` (PLACEHOLDER bookkeeping where noted).
    pub(super) fn morale_inputs(&self, idx: usize) -> MoraleInputs {
        let u = &self.units[idx];
        let max = u.max_men.max(1) as f32;
        let total = 1.0 - u.men as f32 / max;
        // PLACEHOLDER: attackers = fighting enemies in contact; direction from the angle between our
        // facing and the attacker (front within 45°, rear beyond 135°, flank otherwise).
        let mut attackers = Vec::new();
        for e in &self.units {
            if e.side == u.side
                || e.is_out_of_fight()
                || !e.active
                || dist2(u.position, e.position) > MELEE_CONTACT_RANGE * MELEE_CONTACT_RANGE
            {
                continue;
            }
            let direction = match melee_dir(u.position, u.facing, e.position) {
                MeleeDir::Front => AttackDirection::Front,
                MeleeDir::FlankLeft | MeleeDir::FlankRight => AttackDirection::Flank,
                MeleeDir::Rear => AttackDirection::Rear,
            };
            attackers.push(Attacker {
                direction,
                excluded: false,
                doubles_front_value: false,
                plus_two_condition: false,
            });
        }
        // Sub-evaluator 0x0053B970 inputs (CONFIRMED): units within 100 m (`morale::near_100m`)
        // that pass FUN_0055CBD0 (active, has soldiers), the unit itself skipped.
        let mut fear = (false, false, false);
        for (j, e) in self.units.iter().enumerate() {
            if j == idx
                || !e.active
                || e.men == 0
                || !morale::near_units(u, e, 100.0)
            {
                continue;
            }
            if e.side != u.side {
                fear.0 |= e.attributes.frightens_enemy;
                fear.1 |= e.attributes.frightens_horses && super::strength::mounted(u);
            } else {
                fear.2 |= e.attributes.inspires;
            }
        }
        let (friends, enemies) = self.neighbours(idx);
        MoraleInputs {
            active_state: i32::from(u.active),
            morale_stat: u.morale_stat,
            impetuous_allowed: u.attributes.impetuous,
            training_level: u.training_level,
            category: u.category,
            under_projectile_fire: u.under_fire_ticks > 0,
            under_artillery_fire: u.under_artillery_fire_ticks > 0,
            total_casualty_ratio: total,
            recent_casualty_ratio: u.casualty_log.recent_ratio,
            extended_casualty_ratio: u.casualty_log.extended_ratio,
            kill_ratio: u.casualty_log.kill_ratio,
            attackers,
            fighting_cavalry: false,
            // 0x0053E980 (CONFIRMED formula): the model has no unit state 1 (UNKNOWN which).
            shock_persists: morale::shock_persists(false, u.attributes.steadfast),
            // 0x00532370: true outside the scripted battle-wide condition (INFERRED).
            may_break: true,
            // Byte `unit+0xD48` is the experience level (CONFIRMED: exposed as "Experience" by
            // `0x005ABF40` and `0x005CD340`).
            experience: u.experience,
            charging: u.charging,
            fatigue_level: u.fatigue_state as u8,
            column_formation: false, // no formations in the model yet
            can_rally: self.can_rally(idx),
            mode4_condition: false, // component byte +0x50 is never set in the model
            enemy_frightens_near: fear.0,
            horses_frightened: fear.1,
            friend_inspires_near: fear.2,
            // 0x0053BC70 inputs: the army general (CONFIRMED rules, PROVISIONAL death/flight timing).
            general: self.general_status(idx),
            steadfast: u.attributes.steadfast,
            allied_units: self.allied_units(u.side),
            army_destruction: self.army_destruction(u.side),
            friends,
            enemies,
            own_men: u.men as i32,
            own_strength: super::strength::strength(u),
        }
    }

    /// The general unit (index) of army `(side, army)`: the last unit of that army with a
    /// `general_rank` (CONFIRMED: `0x00505B00` assigns `+0x214` for every general card in order).
    pub fn general_of(&self, side: u8, army: u32) -> Option<usize> {
        self.units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.side == side && u.army_index == army && u.general_rank.is_some())
            .map(|(i, _)| i)
            .next_back()
    }

    /// The `0x0053BC70` view of unit `idx`'s army general.
    /// - No general unit: the army object is built with `+0x1B0 = 1` and `+0x214 = 0`, so the
    ///   evaluator takes the "present" branch with distance 0 and rank 0 (CONFIRMED).
    /// - General present: rank and distance (none when `idx` is the general's own unit).
    /// - General unit destroyed: "died recently" for [`GENERAL_DIED_RECENTLY_TICKS`], then "dead".
    /// - General unit routing or shattered: "fled recently".
    ///
    /// The last two are PROVISIONAL: the exe's army fields `+0x1B0` (general active), `+0x1B4`
    /// (died-recently countdown) and `+0x1B8` (fled countdown) are CONFIRMED readers and the
    /// countdowns tick down by 1 per army update (`0x00580390`), but their writers on death or
    /// flight were not found (BATTLE_FIDELITY.md §11).
    pub fn general_status(&self, idx: usize) -> morale::GeneralStatus {
        let u = &self.units[idx];
        let Some(g) = self.general_of(u.side, u.army_index) else {
            return morale::GeneralStatus::Present { rank: 0, distance: None };
        };
        let gu = &self.units[g];
        if gu.men == 0 {
            let died = self.general_died.iter().find(|x| x.0 == (u.side, u.army_index)).map_or(self.tick, |x| x.1);
            return if self.tick.saturating_sub(died) < GENERAL_DIED_RECENTLY_TICKS {
                morale::GeneralStatus::DiedRecently
            } else {
                morale::GeneralStatus::Dead
            };
        }
        if gu.morale.is_routing_or_shattered() {
            return morale::GeneralStatus::FledRecently;
        }
        let distance = (g != idx).then(|| dist2(u.position, gu.position).sqrt());
        morale::GeneralStatus::Present { rank: gu.general_rank.unwrap_or(0), distance }
    }

    /// `FUN_0054C840`: units in the side's armies after the first (CONFIRMED: the list sizes of
    /// armies 1.. of the alliance). Destroyed or inactive units are left out (INFERRED: they leave
    /// the army's list).
    pub fn allied_units(&self, side: u8) -> i32 {
        self.units.iter().filter(|u| u.side == side && u.army_index > 0 && u.active && u.men > 0).count() as i32
    }

    /// Leaving the field, the second half of slot 2 `0x005857A0` (CONFIRMED): a unit that is
    /// leaving (`0x0053EEE0`: its current order says so, or it is routing (`0x0055C480`) while on the
    /// field) and has a soldier outside the playable area gets `+0xAA0` = 2, once, and the battle
    /// raises its "unit left" events (battle `+0x94` lists `+0x608` / `+0xAB8`). From then on:
    /// - it fails the unit validity test `0x0055CBD0` (state 0 or 2), so the morale neighbour
    ///   terms, fear and inspiration skip it (the model's `active` filters);
    /// - the rally test `0x0055C500` rejects it (`+0xAA0 == 2`);
    /// - `0x0053EA00` and `0x0055CB50` no longer count it (state 2).
    ///
    /// A routing or shattered unit with a man whose circle is not inside the area ([`men_inside`])
    /// leaves: `left_field`, not active, no order. Its men are not killed. Without a playable area nobody leaves. The order-based
    /// leaving (withdraw) is not modelled.
    fn leave_step(&mut self, idx: usize) {
        let Some(area) = self.playable_area else { return };
        let u = &mut self.units[idx];
        if u.left_field || !u.active || u.men == 0 || u.reinforcement != Reinforcement::None {
            return;
        }
        if u.morale.is_routing_or_shattered() && !men_inside(u, area) {
            u.left_field = true;
            u.active = false;
            u.set_destination(None);
            u.fire_target = None;
            u.charging = false;
            u.running = false;
        }
    }

    /// Brings reinforcements onto the field, `0x00608200` (called each update from `0x00603A40`,
    /// CONFIRMED structure, BATTLE_FIDELITY.md §13): per reinforcement army, while no unit of it is
    /// still arriving, the next unit in order that is not held by the script (unit `+0x1E5`) and is
    /// not fixed artillery (class 0) starts to arrive. The exe also stops at 20 units per entry
    /// group (`+0xF0 < 0x14`; ours [`Battle::max_reinforcement_units`], moddable); arriving units
    /// become active when they enter the map.
    /// PROVISIONAL: the entry point ([`Battle::reinforcement_entries`]) and the walk-on time
    /// ([`REINFORCEMENT_WALK_ON_TICKS`]).
    fn reinforcements_step(&mut self) {
        // Units walking in. The exe's slot-2 update `0x005857A0` (CONFIRMED) sets unit `+0xAA0` to 1
        // (on the field: our `active`) once every soldier's circle (`+0x48/+0x50`, radius `+0x58`) lies
        // inside the battle's playable area (`+0x84..+0x8C`), and to 2 when a leaving unit has a soldier
        // outside ([`men_inside`]). Without an area, a PROVISIONAL walk-on time.
        let area = self.playable_area;
        for u in &mut self.units {
            if let Reinforcement::Arriving { ticks_left } = u.reinforcement {
                let arrived = match area {
                    Some(a) => men_inside(u, a),
                    None => ticks_left <= 1,
                };
                u.reinforcement = if arrived {
                    u.active = true;
                    Reinforcement::None
                } else {
                    Reinforcement::Arriving { ticks_left: ticks_left.saturating_sub(1) }
                };
            }
        }
        let armies: BTreeSet<(u8, u32)> = self
            .units
            .iter()
            .filter(|u| u.reinforcement == Reinforcement::Waiting)
            .map(|u| (u.side, u.army_index))
            .collect();
        for army in armies {
            let in_army = |u: &LandUnit| (u.side, u.army_index) == army;
            if self.units.iter().any(|u| in_army(u) && matches!(u.reinforcement, Reinforcement::Arriving { .. })) {
                continue;
            }
            let arrived = self.units.iter().filter(|u| in_army(u) && u.reinforcement == Reinforcement::None).count();
            if arrived >= self.max_reinforcement_units {
                continue;
            }
            let Some(i) = self.units.iter().position(|u| {
                in_army(u) && u.reinforcement == Reinforcement::Waiting && u.unit_class != "artillery_fixed"
            }) else {
                continue;
            };
            if let Some(&(_, pos, facing)) = self.reinforcement_entries.iter().find(|e| e.0 == army) {
                // PROVISIONAL spread along the edge: slots 0, +1, -1, +2, -2, ... across the
                // direction of travel, REINFORCEMENT_SPACING_M apart.
                let k = arrived as i32;
                let slot = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) } as f32;
                let (sx, sy) = (-facing.sin(), facing.cos());
                let p = (pos.0 + sx * slot * REINFORCEMENT_SPACING_M, pos.1 + sy * slot * REINFORCEMENT_SPACING_M);
                self.units[i].place_at(p, facing);
            }
            self.units[i].active = false;
            self.units[i].reinforcement = Reinforcement::Arriving { ticks_left: REINFORCEMENT_WALK_ON_TICKS };
            // CONFIRMED (0x00606CE0): an arrived unit is ordered to the centre of the battlefield
            // (the playable area's centre, radius 5 m); the AI or a script takes it from there.
            let id = self.units[i].id;
            let to = self.reinforcement_target;
            let from = self.units[i].position;
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let d = (dx * dx + dy * dy).sqrt();
            if d > 5.0 {
                self.order_move_at(id, to, super::orders::MoveSpeed::Walk);
            }
        }
    }

    /// The battle script's `unit:deploy_reinforcement(deploy)` (CONFIRMED binding `0x006172E0`:
    /// it queues `BCQ_UNIT_ORDER_SET_MANUAL_DEPLOYMENT`, whose handler `0x005C2AA0` sets unit
    /// `+0x1E5 = !deploy`): `true` releases a held reinforcement, `false` holds it again. Units
    /// already on the field are not affected.
    pub fn deploy_reinforcement(&mut self, unit_id: u32, deploy: bool) {
        if let Some(u) = self.units.iter_mut().find(|u| u.id == unit_id) {
            match (u.reinforcement, deploy) {
                (Reinforcement::Held, true) => u.reinforcement = Reinforcement::Waiting,
                (Reinforcement::Waiting, false) => u.reinforcement = Reinforcement::Held,
                _ => {}
            }
        }
    }

    /// Records the tick a general's unit was destroyed (for `GeneralStatus::DiedRecently`).
    fn note_general_deaths(&mut self) {
        let tick = self.tick;
        let dead: Vec<(u8, u32)> = self
            .units
            .iter()
            .filter(|u| u.general_rank.is_some() && u.men == 0)
            .map(|u| (u.side, u.army_index))
            .collect();
        for army in dead {
            if !self.general_died.iter().any(|x| x.0 == army) {
                self.general_died.push((army, tick));
            }
        }
    }

    /// `0x00532000` for `side` from [`Battle::side_start_strength`] and the current strengths.
    fn army_destruction(&self, side: u8) -> bool {
        if self.side_start_strength.is_empty() {
            return false; // not started yet (our guard; the exe stores the start strength first)
        }
        let start = |s: u8| self.side_start_strength.iter().find(|x| x.0 == s).map_or(0.0, |x| x.1);
        let now = self.side_strengths();
        let own = now.iter().find(|x| x.0 == side).map_or((start(side), 0.0), |x| (start(side), x.1));
        let others: Vec<(f32, f32)> = now.iter().filter(|x| x.0 != side).map(|x| (start(x.0), x.1)).collect();
        morale::army_destruction(own, &others)
    }

    /// The units within 160 m of unit `idx` for `0x0053CBD0` (friends, enemies), CONFIRMED
    /// fields, see [`morale::Neighbour`]. Units that are inactive or destroyed count as having
    /// left the field (INFERRED).
    fn neighbours(&self, idx: usize) -> (Vec<morale::Neighbour>, Vec<morale::Neighbour>) {
        let u = &self.units[idx];
        let own_h = self.ground.height(u.position.0, u.position.1);
        let (fx, fy) = (u.facing.cos(), u.facing.sin());
        let (mut friends, mut enemies) = (Vec::new(), Vec::new());
        for (j, e) in self.units.iter().enumerate() {
            let d2 = dist2(u.position, e.position);
            if j == idx || !e.active || e.men == 0 || !morale::near_units(u, e, 160.0) {
                continue;
            }
            let (dx, dy) = (e.position.0 - u.position.0, e.position.1 - u.position.1);
            let mut n = morale::Neighbour {
                distance: d2.sqrt(),
                men: e.men as i32,
                strength: super::strength::strength(e),
                routing: e.morale.is_routing_or_shattered(),
                ..Default::default()
            };
            if e.side == u.side {
                let along = dx * fx + dy * fy;
                let across = -dx * fy + dy * fx;
                n.in_box = along.abs() <= 100.0 && across.abs() <= 75.0;
                friends.push(n);
            } else {
                n.blocks_hill = own_h < self.ground.height(e.position.0, e.position.1) + 5.0;
                n.halved = super::strength::mounted(u) && u.run_speed > e.run_speed;
                enemies.push(n);
            }
        }
        (friends, enemies)
    }

    /// Who has won. A side has lost when every one of its units is routing, shattered or destroyed.
    /// Sides are visited in ascending order (BTreeSet), so the answer is deterministic.
    pub fn battle_result(&self) -> BattleResult {
        let sides: BTreeSet<u8> = self.units.iter().map(|u| u.side).collect();
        let alive: Vec<u8> = sides
            .into_iter()
            .filter(|&s| {
                self.units
                    .iter()
                    .any(|u| u.side == s && !u.is_out_of_fight())
            })
            .collect();
        match alive.as_slice() {
            [] => BattleResult::Draw,
            [s] => BattleResult::Won { side: *s },
            _ => BattleResult::Ongoing,
        }
    }

    /// A deterministic 64-bit hash (FNV-1a) of the whole model state, for desync checks and tests.
    /// Floats are hashed by their exact bit patterns.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv64::new();
        h.u32(self.tick);
        h.u32(self.rng.state);
        h.u32(self.weather as u32);
        h.u32(self.weather_intensity);
        h.u32(self.climate_fatigue.0 as u32);
        h.u32(self.climate_fatigue.1 as u32);
        for u in &self.units {
            h.u32(u.id);
            h.u32(u.side as u32);
            h.u32(u.men);
            h.u32(u.max_men);
            h.u32(u.position.0.to_bits());
            h.u32(u.position.1.to_bits());
            for m in &u.march.men {
                h.u32(m.lag.to_bits());
                h.u32(m.speed.to_bits());
                h.u32(m.distance.to_bits());
                h.u32(m.finished as u32);
                h.u32(m.kept.map_or(u32::MAX, f32::to_bits));
                h.u32(m.side.to_bits());
                h.u32(m.figure);
                h.u32(m.kind as u32);
                h.u32(m.walk.map_or(u32::MAX, f32::to_bits));
            }
            h.u32(u.march.kind as u32);
            h.u32(u.march.reforming as u32);
            // The laid formation and the re-slot after losses (`march`): the men's places.
            match u.march.laid {
                None => h.u32(0),
                Some(f) => {
                    h.u32(1);
                    h.u32(f.files as u32);
                    h.u32(f.ranks as u32);
                    h.u32(f.width.to_bits());
                    h.u32(f.depth.to_bits());
                }
            }
            h.u32(u.march.last_men);
            h.u32(u.march.reslot as u32);
            h.u32(u.march.reslot_in);
            // The idle reform gate's state (`march::LandUnit::idle_gate`).
            h.u32(u.march.in_place_at);
            h.u32(u.march.idle_reformed as u32);
            // The box measured from the men (`march::SquadBox`, squad summary `+0x62C`).
            match u.march.measured {
                None => h.u32(0),
                Some(b) => {
                    h.u32(1);
                    h.u32(b.centre.0.to_bits());
                    h.u32(b.centre.1.to_bits());
                    h.u32(b.heading as u32);
                    h.u32(b.width.to_bits());
                    h.u32(b.depth.to_bits());
                    h.u32(b.radius.to_bits());
                }
            }
            // The men's order and the unit's own (the reference soldier's). The men keep their own
            // kept speeds, and the order's target is the unit's: both only for the unit's own.
            for (plan, own) in [(u.march.plan, false), (u.timed_move, true)] {
                match plan {
                    None => h.u32(0),
                    Some(p) => {
                        h.u32(1);
                        h.u32(p.speed.to_bits());
                        h.u32(p.distance.to_bits());
                        h.u32(p.elapsed.to_bits());
                        h.u32(p.ticks);
                        h.u32(p.issued.to_bits());
                        h.u32(p.at_end as u32 | (p.first as u32) << 1);
                        if own {
                            h.u32(p.kept.map_or(u32::MAX, f32::to_bits));
                            let (x, y) = p.target.unwrap_or((f32::NAN, f32::NAN));
                            h.u32(x.to_bits());
                            h.u32(y.to_bits());
                        }
                    }
                }
            }
            h.u32(u.facing.to_bits());
            h.u32(u.active as u32);
            h.u32(u.script_controlled as u32);
            h.u32(match u.reinforcement {
                Reinforcement::None => 0,
                Reinforcement::Held => 1,
                Reinforcement::Waiting => 2,
                Reinforcement::Arriving { ticks_left } => 3 + ticks_left,
            });
            h.u32(u.morale.state as u32);
            h.u32(u.morale.behaviour as u32);
            h.u32(u.morale.morale as u32);
            h.u32(u.morale.rout_timer as u32);
            h.u32(u.morale.waver_timer as u32);
            h.u32(u.morale.times_routed as u32);
            h.u32(u.fatigue as u32);
            h.u32(u.fatigue_state as u32);
            h.u32(u.kills);
            h.u32(u.recent_losses);
            h.u32(u.blows_resolved);
            h.u32(u.ammunition);
            h.u32(u.ammo_pool.map_or(u32::MAX, |p| p.rounds));
            h.u32(u.reload_ticks_left);
            h.u32(u.fire_target.unwrap_or(u32::MAX));
            h.u32(u.volleys_fired);
            h.u32(u.under_fire_ticks);
            h.u32(u.under_artillery_fire_ticks);
            h.u32(u.contact_ticks);
            h.u32(u.left_field as u32);
            h.u32(u.running as u32);
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `0x00EED960`: the six training keys in the exe's order; any other key has no level.
    #[test]
    fn training_levels_follow_the_exes_order() {
        assert_eq!(training_level_of("mob"), Some(0));
        assert_eq!(training_level_of("elite"), Some(5));
        assert_eq!(training_level_of("trained"), Some(3));
        assert_eq!(training_level_of("veteran"), None);
    }

    /// OBVIOUSLY MADE-UP placeholder morale data (NOT game data).
    fn kv_morale() -> KvMorale {
        KvMorale {
            morale_base: 40,
            ums_impetuous_threshold_lower: 90,
            ums_eager_threshold_upper: 95,
            ums_eager_threshold_lower: 70,
            ums_confident_threshold_upper: 75,
            ums_confident_threshold_lower: 50,
            ums_steady_threshold_upper: 55,
            ums_steady_threshold_lower: 30,
            ums_shaken_threshold_upper: 35,
            ums_shaken_threshold_lower: 10,
            ums_wavering_threshold_upper: 15,
            ums_wavering_threshold_lower: -10,
            ums_broken_threshold_upper: -5,
            ums_broken_threshold_lower: -30,
            waver_base_timeout: 20,
            broken_finish_base_timeout: 50,
            was_attacked_in_front: -2,
            was_attacked_in_flank: -10,
            was_attacked_in_rear: -20,
            recent_casualties_shock_threshold: 100,
            total_casualties_penalty_20: -10,
            total_casualties_penalty_40: -25,
            total_casualties_penalty_60: -45,
            total_casualties_penalty_80: -60,
            total_casualties_penalty_90: -80,
            ..KvMorale::default()
        }
    }

    /// OBVIOUSLY MADE-UP placeholder fatigue data (NOT game data).
    fn kv_fatigue() -> KvFatigue {
        KvFatigue {
            idle: -1,
            walking: 1,
            running: 3,
            combat: 4,
            threshold_fresh: 0,
            threshold_active: 100,
            threshold_winded: 200,
            threshold_tired: 300,
            threshold_very_tired: 400,
            threshold_exhausted: 500,
            threshold_max: 600,
            ..KvFatigue::default()
        }
    }

    /// OBVIOUSLY MADE-UP placeholder rules data (NOT game data).
    fn kv_rules() -> KvRules {
        KvRules {
            relative_melee_fatigue_multiplier: 0,
            armour_melee_piercing_divisor: 2,
            defense_melee_piercing_divisor: 2,
            melee_height_delta_min: -1.0,
            melee_height_delta_max: 1.0,
            relative_melee_height_delta_divisor: 1.0,
            factor_attackdir_flankleft: 3,
            factor_attackdir_flankright: 3,
            factor_attackdir_rear: 6,
            melee_hn_to_xholds_0_max: -10,
            melee_hn_to_xholds_1_max: 0,
            melee_hn_to_xholds_2_max: 10,
            melee_hn_to_xholds_3_max: 20,
            melee_xholds_knockdown_2: 50,
            melee_xholds_knockback_2: 100,
            melee_xholds_stepback_2: 150,
            ..KvRules::default()
        }
    }

    /// A made-up unit: 120 men with the given (made-up) melee attack.
    fn unit(id: u32, side: u8, pos: (f32, f32), attack: i32) -> LandUnit {
        let mut u = LandUnit::new(id, side, 120, pos);
        u.melee_attack = attack;
        u.armour = 4;
        u.melee_defence = 4;
        u
    }

    fn sample_battle(seed: u32) -> Battle {
        let mut b = Battle::with_rules(seed, kv_morale(), kv_fatigue(), kv_rules());
        for k in 0..4u32 {
            let x = k as f32 * 30.0;
            b.add_unit(unit(k, 0, (x, 0.0), 8)); // side 0 is (made-up) stronger
            b.add_unit(unit(10 + k, 1, (x, 60.0), 4));
        }
        b
    }

    #[test]
    fn melee_dir_classification() {
        // Defender at the origin facing +x.
        assert_eq!(melee_dir((0.0, 0.0), 0.0, (5.0, 0.0)), MeleeDir::Front);
        // 0x006AD890: the flank side comes from the world x of v = D - A, so with v.x == 0 both
        // sides are "flank 1" (atan2(0, +-1) >= 0 and c >= 0).
        assert_eq!(melee_dir((0.0, 0.0), 0.0, (0.0, 5.0)), MeleeDir::FlankLeft);
        assert_eq!(melee_dir((0.0, 0.0), 0.0, (0.0, -5.0)), MeleeDir::FlankLeft);
        assert_eq!(melee_dir((0.0, 0.0), 0.0, (-5.0, 0.1)), MeleeDir::Rear);
        // Within 45 degrees of straight behind is rear; closer than 0.1 m is front.
        assert_eq!(melee_dir((0.0, 0.0), 0.0, (-5.0, 4.0)), MeleeDir::Rear);
        assert_eq!(melee_dir((0.0, 0.0), 0.0, (0.05, 0.05)), MeleeDir::Front);
        // Facing +y: front-half attackers (c < 0) take the side from the sign of v.x.
        let up = std::f32::consts::FRAC_PI_2;
        assert_eq!(melee_dir((0.0, 0.0), up, (-5.0, 1.0)), MeleeDir::FlankRight);
        assert_eq!(melee_dir((0.0, 0.0), up, (5.0, 1.0)), MeleeDir::FlankLeft);
        assert_eq!(melee_dir((0.0, 0.0), up, (0.5, 5.0)), MeleeDir::Front);
    }

    #[test]
    fn exchange_follows_the_w1_rng_order() {
        // Two equal 120-man units in contact: k = 1 member each, so the list is [a, b].
        let mut b = Battle::with_rules(777, kv_morale(), kv_fatigue(), kv_rules());
        b.add_unit(unit(0, 0, (0.0, 0.0), 8));
        b.add_unit(unit(1, 1, (5.0, 0.0), 4));
        b.units[1].facing = std::f32::consts::PI; // facing unit 0: frontal

        // Replay the expected RNG calls by hand on a copy of the RNG.
        let mut rng = b.rng;
        let attacker = melee::pick_weighted(&mut rng, &[8.0, 4.0]).unwrap(); // roll #1
        let defender = loop {
            let k = melee::pick_index(&mut rng, 2); // rolls #2..k
            if k != attacker {
                break k;
            }
        };
        let (att, def) = (&b.units[attacker], &b.units[defender]);
        let inputs = MeleeInputs {
            attacker: att.combatant(),
            defender: def.combatant(),
            dir: melee_dir(def.position, def.facing, att.position),
            height_delta: 0.0,
        };
        let expected = melee::resolve_blow(&mut rng, &inputs, 1, &kv_rules()); // the blow roll

        let got = b.melee_exchange(0, 1);
        assert_eq!(got, vec![(attacker, expected)]);
        assert_eq!(b.rng, rng, "same number and order of RNG calls");
        assert_eq!(b.units[attacker].blows_resolved, 1);
    }

    #[test]
    fn charging_miss_triggers_strike_back() {
        // Rules where every blow misses: hn is very negative -> kc = 1, roll >= 1, xholds 0.
        let rules = KvRules {
            factor_attackdir_front: -1000,
            factor_attackdir_rear: -1000,
            ..KvRules::default()
        };
        let mut b = Battle::with_rules(3, kv_morale(), kv_fatigue(), rules);
        b.add_unit(unit(0, 0, (0.0, 0.0), 8));
        b.add_unit(unit(1, 1, (5.0, 0.0), 4));
        b.units[0].charging = true;
        let before = b.rng;
        let got = b.melee_exchange(0, 1);
        // The charger is the only top-priority candidate, so it attacks; MISS -> strike back.
        assert_eq!(got, vec![(0, BlowOutcome::Miss), (1, BlowOutcome::Miss)]);
        assert_ne!(b.rng, before);
        // Without charging there is no strike back.
        b.units[0].charging = false;
        assert_eq!(b.melee_exchange(0, 1).len(), 1);
    }

    #[test]
    fn a_charge_counts_for_its_impact_only() {
        let mut b = Battle::with_rules(3, kv_morale(), kv_fatigue(), KvRules::default());
        b.add_unit(unit(0, 0, (0.0, 0.0), 8));
        b.add_unit(unit(1, 1, (5.0, 0.0), 4));
        for u in &mut b.units {
            u.hold_position = true;
        }
        b.units[0].charging = true;
        assert!(b.units[0].charging_now());
        for _ in 0..MELEE_EXCHANGE_INTERVAL_TICKS {
            b.step();
        }
        // Still ordered to charge, but the impact is over.
        assert!(b.units[0].charging && !b.units[0].charging_now());
        assert!(!b.units[0].combatant().charging);
        // Out of contact for the same time: the next charge counts again.
        b.units[1].position = (500.0, 0.0);
        for _ in 0..MELEE_EXCHANGE_INTERVAL_TICKS {
            b.step();
        }
        assert!(b.units[0].charging_now());
    }

    #[test]
    fn units_are_kept_in_id_order() {
        let b = sample_battle(1);
        let ids: Vec<u32> = b.units.iter().map(|u| u.id).collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 10, 11, 12, 13]);
    }

    #[test]
    fn morale_full_update_stagger() {
        let mut b = sample_battle(1);
        for _ in 0..7 {
            b.step(); // ticks 0..=6
        }
        // Unit id 3 is fully evaluated on ticks 3, 8, ...; id 10 on ticks 0, 5, ...; id 11 on 1, 6.
        let get = |id: u32| {
            b.units
                .iter()
                .find(|u| u.id == id)
                .unwrap()
                .last_full_morale_tick
        };
        assert_eq!(get(3), Some(3));
        assert_eq!(get(10), Some(5));
        assert_eq!(get(11), Some(6));
        assert_eq!(get(2), Some(2));
        assert_eq!(b.tick, 7);
        assert!((b.time_seconds() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn movement_reaches_destination() {
        let mut b = Battle::new(0, kv_morale(), kv_fatigue());
        let mut u = LandUnit::new(0, 0, 100, (0.0, 0.0));
        u.walk_speed = 2.0;
        u.destination = Some((1.0, 0.0));
        b.add_unit(u);
        for _ in 0..4 {
            b.step(); // 0.2 m per tick
        }
        assert!((b.units[0].position.0 - 0.8).abs() < 1e-5);
        b.step();
        assert_eq!(b.units[0].position, (1.0, 0.0));
        assert_eq!(b.units[0].destination, None);
    }

    /// The slope factor and the braking test the unit and each man share (`0x00819770`,
    /// `arrival_braking`).
    #[test]
    fn slope_and_braking_rules() {
        assert_eq!(slope_multiplier(0.0), 1.0);
        assert!((slope_multiplier(0.5) - 0.4).abs() < 1e-6);
        assert!((slope_multiplier(-0.2) - 1.2).abs() < 1e-6);
        assert_eq!(slope_multiplier(-2.0), 1.5);
        // 2 m/s with 10 m/s²: braking distance max(0.2, 0.5) = 0.5 m, plus a tick's 0.2 m.
        assert!(brakes_for(0.7, 2.0, 10.0) && !brakes_for(0.71, 2.0, 10.0));
        assert!(!brakes_for(0.0, 2.0, 0.0) && !brakes_for(0.0, 2.0, f32::INFINITY));
    }

    #[test]
    fn timed_move_speed_catches_up_with_the_schedule() {
        let cap = 2.0 * TIMED_MOVE_SPEED_CAP; // run 2.0 m/s
        // No time left: the move's own speed.
        assert_eq!(timed_move_speed(1.4, cap, 50.0, 0.0, true), 1.4);
        assert_eq!(timed_move_speed(1.4, cap, 50.0, -1.0, false), 1.4);
        // 10 m behind with 20 s left: +0.5 m/s.
        assert!((timed_move_speed(1.4, cap, 38.0, 20.0, true) - 1.9).abs() < 1e-6);
        // Ahead of schedule: kept at the move's speed with keep_speed, plainly d / t without.
        assert_eq!(timed_move_speed(1.4, cap, 10.0, 20.0, true), 1.4);
        assert!((timed_move_speed(1.4, cap, 10.0, 20.0, false) - 0.5).abs() < 1e-6);
        // Far behind: capped at run x 1.45.
        assert_eq!(timed_move_speed(1.4, cap, 500.0, 5.0, true), cap);
        // A NaN result takes the cap (COMISS / JBE in 0x0063AF40).
        assert_eq!(timed_move_speed(f32::NAN, cap, 10.0, 1.0, false), cap);
    }

    /// The re-issue schedule of `0x00581AE0` from tick 0: the f32 compare at `0x00581B73` gives
    /// 11-tick periods but for a few issue ticks, whose period is 10.
    #[test]
    fn the_timed_move_reissues_on_the_exes_f32_schedule() {
        let (mut issued, mut short) = (0u32, Vec::new());
        for tick in 1..200_000u32 {
            if timed_move_reissue_due(timed_move_issue_time(issued), tick) {
                let period = tick - issued;
                assert!(period == 10 || period == 11, "period {period} at {issued}");
                if period == 10 {
                    short.push(issued);
                }
                issued = tick;
            }
        }
        assert_eq!(short, [11, 32, 1274, 20_479]);
        assert!(!timed_move_reissue_due(timed_move_issue_time(0), 10) && timed_move_reissue_due(timed_move_issue_time(0), 11));
        assert!(timed_move_reissue_due(timed_move_issue_time(11), 21));
    }

    /// A walking unit's plan is re-issued on the battle tick's schedule, so a move planned at tick 11
    /// gets its next plan 10 ticks later and one planned at tick 21 11 ticks later.
    #[test]
    fn a_walk_replans_on_the_battle_ticks_schedule() {
        for (start, period) in [(11, 10), (21, 11)] {
            let mut b = walker(0.0, (500.0, 0.0), MovementClass::Infantry);
            b.tick = start;
            b.step();
            let plan = b.units[0].timed_move.expect("moving");
            assert!(plan.first && plan.issued == timed_move_issue_time(start), "{plan:?}");
            let mut replanned = None;
            for k in 1..=12 {
                b.step();
                if replanned.is_none() && b.units[0].timed_move.is_some_and(|t| !t.first && t.ticks == 1) {
                    replanned = Some(k);
                }
            }
            assert_eq!(replanned, Some(period), "planned at {start}");
        }
    }

    #[test]
    fn speed_changes_by_the_entity_acceleration_and_deceleration() {
        // `0x00819770`: up by at most accel × 0.1 per tick, down by at most decel × 0.1.
        assert!((step_speed(2.6, 10.0, 2.5, 6.0) - 2.85).abs() < 1e-6);
        assert!((step_speed(10.0, 2.6, 2.5, 6.0) - 9.4).abs() < 1e-6);
        assert!((step_speed(2.5, 2.6, 2.5, 6.0) - 2.6).abs() < 1e-6, "the last step lands on the target");
        assert_eq!(step_speed(0.0, 3.6, f32::INFINITY, f32::INFINITY), 3.6, "no entity data: at once");
        assert_eq!(step_speed(3.6, 0.0, f32::INFINITY * 0.0, f32::INFINITY), 0.0, "a NaN rate is no limit");
    }

    /// `0x00807410`: braking starts at max(0.5, 0.5 / decel × v²) m from the destination, and the
    /// unit comes to a stop there at its deceleration instead of halting at full speed.
    #[test]
    fn a_unit_brakes_to_its_destination() {
        assert_eq!(arrival_braking(10.0, 6.0), 0.5 / 6.0 * 100.0);
        assert_eq!(arrival_braking(1.4, 5.0), 0.5, "the half-metre minimum");
        assert_eq!(arrival_braking(10.0, f32::INFINITY), 0.0, "no entity data: no braking");
        let mut b = Battle::new(0, kv_morale(), kv_fatigue());
        let mut u = LandUnit::new(0, 0, 60, (0.0, 0.0));
        (u.walk_speed, u.run_speed, u.acceleration, u.deceleration) = (2.6, 10.0, 2.5, 6.0);
        u.speed = 10.0;
        u.running = true;
        u.hold_position = true;
        u.destination = Some((30.0, 0.0));
        b.add_unit(u);
        let mut speeds = Vec::new();
        while b.units[0].destination.is_some() && speeds.len() < 100 {
            b.step();
            speeds.push(b.units[0].speed);
        }
        let x = b.units[0].position.0;
        assert!(x > 29.4 && x <= 30.0, "stops within half a metre of it: {x}, {speeds:?}");
        assert!(speeds.windows(2).filter(|w| w[1] < w[0]).all(|w| w[0] - w[1] <= 0.6 + 1e-4), "slows at 6 m/s²: {speeds:?}");
        assert!(speeds.iter().rev().nth(1).is_some_and(|&s| s < 1.0), "nearly stopped before the end: {speeds:?}");
    }

    #[test]
    fn a_heavy_horse_ordered_to_run_gathers_speed_over_three_seconds() {
        let mut b = Battle::new(0, kv_morale(), kv_fatigue());
        let mut u = LandUnit::new(0, 0, 60, (0.0, 0.0));
        (u.walk_speed, u.run_speed, u.acceleration, u.deceleration) = (2.6, 10.0, 2.5, 6.0);
        u.speed = 2.6;
        u.running = true;
        u.hold_position = true;
        u.destination = Some((1000.0, 0.0));
        b.add_unit(u);
        let mut speeds = Vec::new();
        for _ in 0..40 {
            b.step();
            speeds.push(b.units[0].speed);
        }
        assert!((speeds[0] - 2.85).abs() < 1e-4, "{speeds:?}");
        assert!(speeds[10] > 5.0 && speeds[10] < 5.4, "about 1.1 s in it canters: {speeds:?}");
        // Run speed after ~3 s. Behind its timed-move schedule while it gathered speed, the order
        // speed is above the run speed, so it passes 10 (10.1, then 10.2) until the next plan (tick 32 on the
        // schedule from tick 0, 0, 11, 21, 32: `timed_move_reissue_due`) asks for 10 again.
        assert!((speeds[29] - 10.1).abs() < 1e-4 && speeds[32..].iter().all(|s| (s - 10.0).abs() < 1e-4), "run speed after ~3 s: {speeds:?}");
        assert!(speeds[..32].windows(2).all(|w| w[1] >= w[0]), "{speeds:?}");
        // The walk order brings it back at the deceleration, 0.6 m/s a tick.
        b.units[0].running = false;
        b.step();
        assert!((b.units[0].speed - 9.4).abs() < 1e-4, "{}", b.units[0].speed);
    }

    #[test]
    fn determinism_1000_ticks() {
        let mut a = sample_battle(12345);
        let mut b = sample_battle(12345);
        for _ in 0..1000 {
            a.step();
            b.step();
        }
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a, b);
        // Something actually happened (the placeholder melee consumed the RNG and killed men).
        assert!(a.units.iter().any(|u| u.men < u.max_men));
        assert_ne!(a.rng.state, 12345);
        // A different seed gives a different history.
        let mut c = sample_battle(54321);
        for _ in 0..1000 {
            c.step();
        }
        assert_ne!(a.state_hash(), c.state_hash());
    }

    #[test]
    fn battle_result_rules() {
        let mut b = sample_battle(1);
        assert_eq!(b.battle_result(), BattleResult::Ongoing);
        for u in b.units.iter_mut().filter(|u| u.side == 1) {
            u.morale.behaviour = MoraleBehaviour::Routing;
        }
        b.units[0].morale.behaviour = MoraleBehaviour::Shattered; // one side-0 unit gone is fine
        assert_eq!(b.battle_result(), BattleResult::Won { side: 0 });
        for u in b.units.iter_mut().filter(|u| u.side == 0) {
            u.men = 0;
        }
        assert_eq!(b.battle_result(), BattleResult::Draw);
    }

    #[test]
    fn simulated_battle_ends() {
        // With the made-up data, the (made-up) stronger side 0 should eventually win.
        let mut b = sample_battle(7);
        let mut ticks = 0;
        while b.battle_result() == BattleResult::Ongoing && ticks < 20_000 {
            b.step();
            ticks += 1;
        }
        assert_eq!(b.battle_result(), BattleResult::Won { side: 0 });
    }

    #[test]
    fn inactive_unit_fatigue_reset() {
        let mut b = Battle::new(0, kv_morale(), kv_fatigue());
        let mut u = LandUnit::new(0, 0, 100, (0.0, 0.0));
        u.fatigue = 250;
        u.fatigue_state = FatigueState::Winded;
        u.active = false;
        b.add_unit(u);
        b.step();
        assert_eq!(b.units[0].fatigue, 0);
        assert_eq!(b.units[0].fatigue_state, FatigueState::Fresh);
    }
    use crate::battle::ground::{GridSpec, GroundTypeGrid, HeightGrid, speed_table};

    /// A 200 m map: left half road (index 6), right half mud (index 4); heights rise 0.15 m per
    /// metre towards +y (a steep slope by the kv_fatigue ladder).
    fn test_ground() -> BattleGround {
        let spec = |cols, rows| GridSpec { cols, rows, width: 200.0, height: 200.0 };
        BattleGround {
            types: Some(GroundTypeGrid { spec: spec(2, 1), cells: vec![6, 4] }),
            heights: Some(HeightGrid {
                spec: spec(2, 2),
                // row 0 = +y edge (high), row 1 = −y edge (low)
                heights: vec![30.0, 30.0, 0.0, 0.0],
            }),
            speed_modifiers: speed_table([("road", [1.5; 4]), ("mud", [0.6, 0.65, 0.8, 0.8])]),
        }
    }

    fn walker(x: f32, dest: (f32, f32), class: MovementClass) -> Battle {
        let mut b = Battle::new(0, kv_morale(), kv_fatigue());
        b.ground = std::sync::Arc::new(test_ground());
        let mut u = LandUnit::new(0, 0, 100, (x, 0.0));
        u.walk_speed = 2.0;
        u.movement_class = class;
        u.destination = Some(dest);
        b.add_unit(u);
        b
    }

    /// Metres a unit covers in `ticks` ticks (one plan, ≤ 11) of a timed move at gait speed `v`
    /// under a constant speed multiplier `m`, speed changes at once: the order's point is 5·v
    /// ahead and the order speed is d / t every tick (`TimedMove`).
    fn timed_walk(v: f32, m: f32, ticks: u32) -> f32 {
        let (mut d, mut t, mut covered) = (v * 5.0, 5.0f32, 0.0f32);
        for _ in 0..ticks {
            let step = d / t * m * 0.1;
            (d, t, covered) = (d - step, t - 0.1, covered + step);
        }
        covered
    }

    #[test]
    fn ground_type_scales_speed() {
        // 10 ticks at 2 m/s: road ×1.5, mud ×0.8 (infantry column 3), foot artillery on mud ×0.6.
        // Off ×1 the timed move's order speed drifts: below 2 m/s on the road (ahead of its plan),
        // above on mud (behind it), so the road gives under 3 m and mud over 1.6 m.
        for (x, class, m) in [(-50.0, MovementClass::Infantry, 1.5), (50.0, MovementClass::Infantry, 0.8), (50.0, MovementClass::FootArtillery, 0.6)] {
            let mut b = walker(x, (x + 100.0, 0.0), class);
            for _ in 0..10 {
                b.step();
            }
            let moved = b.units[0].position.0 - x;
            let expected = timed_walk(2.0, m, 10);
            assert!((moved - expected).abs() < 1e-3, "{x} {class:?}: moved {moved}, expected {expected}");
        }
        assert!(timed_walk(2.0, 1.5, 10) < 3.0 && timed_walk(2.0, 0.8, 10) > 1.6);
        assert!((timed_walk(2.0, 1.0, 10) - 2.0).abs() < 1e-4, "on schedule: the gait speed");
    }

    #[test]
    fn slopes_change_speed() {
        // The test ground rises 30 m over 200 m towards +y (g = 0.15): uphill ×1/(1 + 0.45),
        // downhill ×1.15 (0x00819770).
        let walk = |dest: (f32, f32)| {
            let mut b = walker(-50.0, dest, MovementClass::Infantry);
            b.units[0].position = (-50.0, 0.0);
            for _ in 0..10 {
                b.step();
            }
            dist2(b.units[0].position, (-50.0, 0.0)).sqrt()
        };
        // On the road (×1.5) at 2 m/s for 1 s.
        let up = walk((-50.0, 90.0));
        let down = walk((-50.0, -90.0));
        assert!((up - timed_walk(2.0, 1.5 / 1.45, 10)).abs() < 0.05, "uphill {up}");
        assert!((down - timed_walk(2.0, 1.5 * 1.15, 10)).abs() < 0.05, "downhill {down}");
    }

    /// The 2026-10-10 sitting: a walk-ordered light cavalry horse in dense forest (ground 0xE,
    /// mounted column 0.40, slope 0.93) walks at 0.373 × an order speed that starts each plan at its
    /// walk 2.69 m/s and rises ~0.035 a tick; it never reaches the trot (the horse's walk / trot
    /// switch is at (1.47 + 3.53) / 2 = 2.50 m/s, the clips' root speeds). The surge over a re-plan
    /// is the original's (CONFIRMED, `TimedMove`): back to the walk speed at each re-issue
    /// (`0x00581B73`, mostly 11 ticks apart), never above the sitting's peak.
    #[test]
    fn walking_cavalry_in_dense_forest_walks_on_its_timed_order() {
        let mut b = Battle::new(0, kv_morale(), kv_fatigue());
        b.ground = std::sync::Arc::new(BattleGround {
            types: Some(GroundTypeGrid { spec: GridSpec { cols: 1, rows: 1, width: 400.0, height: 400.0 }, cells: vec![0xE] }),
            heights: None,
            speed_modifiers: speed_table([("vegetation_dense_forest", [0.4, 0.4, 0.4, 0.6])]),
        });
        let mut u = LandUnit::new(0, 0, 60, (0.0, 0.0));
        (u.walk_speed, u.run_speed) = (2.69, 7.0);
        u.movement_class = MovementClass::Mounted;
        u.destination = Some((150.0, 0.0));
        b.add_unit(u);
        let mut orders = Vec::new();
        let mut speeds = Vec::new();
        for _ in 0..33 {
            orders.push(b.units[0].timed_move.map_or(2.69, |mut t| t.order_speed(7.0)));
            b.step();
            speeds.push(b.units[0].speed);
        }
        // Each tick it walks at 0.40 × the order speed it was given.
        let given: Vec<f32> = speeds.iter().map(|s| s / 0.4).collect();
        assert!((given[0] - 2.69).abs() < 1e-4, "{given:?}");
        // The plans from tick 0 on the exe's f32 schedule (`timed_move_reissue_due`): ticks 0, 11, 21
        // (a 10-tick period), 32.
        for plan in [&given[..11], &given[11..21], &given[21..32]] {
            assert!((plan[0] - 2.69).abs() < 1e-4, "each plan starts at the walk speed: {given:?}");
            for w in plan.windows(2) {
                let rise = w[1] - w[0];
                assert!(rise > 0.03 && rise < 0.05, "rises ~0.035 a tick (the sitting): {given:?}");
            }
            assert!(plan.iter().all(|&s| s < 3.07 + 0.05), "peaks at the sitting's 3.07: {given:?}");
        }
        assert!(given[10] > 2.69 + 0.3, "no re-plan after 10 ticks: {given:?}");
        assert!((given[32] - 2.69).abs() < 1e-4, "the fourth plan: {given:?}");
        assert!(speeds.iter().all(|&s| s < 2.5), "walks, never trots: {speeds:?}");
        assert!((orders[1] - given[1]).abs() < 1e-4);
    }

    /// Every move order is a new plan at once, also one re-issued to the same point (`0x00659520`
    /// zeroes `+0x43C` on each install): the old plan's catch-up speed does not carry over.
    #[test]
    fn a_new_order_replaces_the_timed_plan() {
        for dest in [(0.0, 150.0), (150.0, 0.0)] {
            let mut b = Battle::new(0, kv_morale(), kv_fatigue());
            b.ground = std::sync::Arc::new(BattleGround {
                types: Some(GroundTypeGrid { spec: GridSpec { cols: 1, rows: 1, width: 400.0, height: 400.0 }, cells: vec![0xE] }),
                heights: None,
                speed_modifiers: speed_table([("vegetation_dense_forest", [0.4, 0.4, 0.4, 0.6])]),
            });
            let mut u = LandUnit::new(0, 0, 60, (0.0, 0.0));
            (u.walk_speed, u.run_speed) = (2.69, 7.0);
            u.movement_class = MovementClass::Mounted;
            u.destination = Some((150.0, 0.0));
            b.add_unit(u);
            for _ in 0..5 {
                b.step();
            }
            let mut old = b.units[0].timed_move.expect("moving");
            assert!(old.order_speed(7.0) > 2.69 + 0.1, "the old plan is catching up: {old:?}");
            assert!(b.order_move(0, dest));
            b.step();
            // The tick after the order walks at 0.40 × the walk speed, the start of a fresh plan.
            assert!((b.units[0].speed / 0.4 - 2.69).abs() < 1e-4, "{dest:?}: {}", b.units[0].speed);
            assert_eq!(b.units[0].timed_move.expect("moving").ticks, 1);
        }
    }

    #[test]
    fn uphill_walking_tires_more() {
        let mut kv = kv_fatigue();
        kv.walking = 10;
        kv.gradient_shallow_movement_multiplier = 133;
        kv.gradient_steep_movement_multiplier = 166;
        kv.gradient_very_steep_movement_multiplier = 200;
        let run = |dest: (f32, f32)| {
            let mut b = walker(-50.0, dest, MovementClass::Infantry);
            b.kv_fatigue = kv;
            for _ in 0..20 {
                b.step();
            }
            (b.units[0].fatigue, b.units[0].gradient)
        };
        let (flat, g_flat) = run((-50.0 + 1000.0, 0.0)); // along the contour
        let (up, g_up) = run((-50.0, 1000.0)); // straight uphill, gradient 0.15
        let (down, _) = run((-50.0, -1000.0));
        assert_eq!(g_flat, 0.0);
        assert!((g_up - 0.15).abs() < 1e-4);
        assert_eq!(flat, 20 * 10);
        assert_eq!(up, 20 * 16); // (166 * 10) / 100 per tick
        assert_eq!(down, flat); // downhill: no multiplier (PROVISIONAL)
    }

    /// The experience term of `0x00670F40`: a unit's fatigue row bonus comes from column 6 of
    /// `unit_stats_land_experience_bonuses` at its experience level, so two otherwise identical
    /// units differ by exactly that much per tick. The row values are the REAL ones (rank 9 = −3).
    #[test]
    fn veterans_tire_more_slowly() {
        let mut kv = kv_fatigue();
        kv.walking = 10;
        let run = |xp: u8| {
            let mut b = walker(-50.0, (-50.0 + 1000.0, 0.0), MovementClass::Infantry);
            b.kv_fatigue = kv;
            // The REAL table's column 6, ranks 0..9.
            b.experience_fatigue = vec![0, 0, 0, 0, 0, -1, -1, -2, -2, -3];
            b.units[0].experience = xp;
            for _ in 0..20 {
                b.step();
            }
            b.units[0].fatigue
        };
        assert_eq!(run(0), 200); // 20 ticks × walking 10
        assert_eq!(run(4), 200); // ranks 0..4 have no bonus
        assert_eq!(run(7), 160); // rank 7 = −2 per tick
        assert_eq!(run(9), 140); // rank 9 = −3 per tick
    }

    /// The unit-size option (`0x004A6600`): a unit card's men are multiplied by the battle's scale
    /// and truncated, and the default battle has none of it (scale 1.0, the "ultra" step). 160 is
    /// the real `num_men` of a line battalion.
    #[test]
    fn unit_size_option_thins_the_men() {
        let mut b = walker(-50.0, (-50.0 + 1000.0, 0.0), MovementClass::Infantry);
        assert_eq!(b.unit_scale, 1.0);
        assert_eq!(b.men_at_scale(160), 160);
        // The four steps of 0x01392770 through `set_unit_scale_step`.
        let men = |i: i32| {
            let mut b = b.clone();
            b.set_unit_scale_step(i, &Default::default());
            b.men_at_scale(160)
        };
        assert_eq!(men(0), 40);
        assert_eq!(men(1), 80);
        assert_eq!(men(2), 120);
        assert_eq!(men(3), 160);
        // A step index past the table is the last step, clamped at 1.0.
        assert_eq!(men(9), 160);
        // The exe's own default setting, `gfx_unit_scale 2` (0x00404230), is 0.75.
        let mut b2 = b.clone();
        assert_eq!(b2.set_unit_scale_step(crate::battle::unit_scale::PREFERENCE_DEFAULT, &Default::default()), 0.75);
        assert_eq!(b2.men_at_scale(160), 120);
        // CVTTSS2SI truncates: 7 × 0.75 = 5.25 -> 5, 158 × 0.75 = 118.5 -> 118.
        assert_eq!(b2.men_at_scale(7), 5);
        assert_eq!(b2.men_at_scale(158), 118);
        // The scale the exe clamps to [0.1, 1.0] (0x004A6540).
        b.unit_scale = 0.0;
        assert_eq!(b.men_at_scale(160), 16);
        b.unit_scale = 3.0;
        assert_eq!(b.men_at_scale(160), 160);
        // A modded step above the original's 1.0 gives units more men than their card.
        let mut limits = crate::limits::GameLimits::default();
        limits.unit_scales.push(1.5);
        let mut big = b.clone();
        assert_eq!(big.set_unit_scale_step(4, &limits), 1.5);
        assert_eq!(big.men_at_scale(160), 240);
        assert_eq!(big.set_unit_scale_setting(Some(9), &limits), 1.5);
    }

    #[test]
    fn determinism_with_ground() {
        let run = || {
            let mut b = sample_battle(777);
            b.ground = std::sync::Arc::new(test_ground());
            for _ in 0..600 {
                b.step();
            }
            b.state_hash()
        };
        assert_eq!(run(), run());
        let mut flat = sample_battle(777);
        for _ in 0..600 {
            flat.step();
        }
        assert_ne!(run(), flat.state_hash(), "the ground changes the battle");
    }
}

#[cfg(test)]
mod combatant_fatigue_tests {
    /// Regression test: BOTH units of a melee encounter must count as in melee after a tick,
    /// not only the one that updated first.
    #[test]
    fn both_sides_of_a_melee_are_marked_in_melee() {
        let mut b = Battle::new(7, KvMorale::default(), KvFatigue::default());
        b.add_unit(LandUnit::new(1, 0, 50, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 50, (2.0, 0.0)));
        b.step();
        assert!(b.units[0].in_melee && b.units[1].in_melee);
    }

    use super::*;

    /// Regression test: the melee combatant must carry the fatigue LEVEL (0..=5), not raw points.
    #[test]
    fn combatant_fatigue_is_the_level_not_raw_points() {
        let mut u = LandUnit::new(1, 0, 100, (0.0, 0.0));
        u.fatigue = 5000; // raw points (thousands in real data)
        u.fatigue_state = FatigueState::Tired;
        assert_eq!(u.combatant().fatigue, 3);
    }

    #[test]
    fn fear_and_inspiration_inputs() {
        // 0x0053B970: attribute holders within 100 m (strict) of the squads' box edges (centre
        // distance < r + 100 + r, the radius +0x670), the unit itself skipped.
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        let mut hussars = LandUnit::new(0, 0, 120, (0.0, 0.0));
        hussars.unit_category = "cavalry".into();
        hussars.attributes.inspires = true; // its own flag does not count
        b.add_unit(hussars);
        let mut guard = LandUnit::new(1, 1, 120, (99.0, 0.0));
        guard.attributes.frightens_enemy = true;
        b.add_unit(guard);
        let mut camels = LandUnit::new(2, 1, 120, (0.0, 99.5));
        camels.attributes.frightens_horses = true;
        b.add_unit(camels);
        // The same men facing the same way: their boxes sit alike on their positions.
        let r = b.units[0].radius();
        assert!(r > 1.0 && (b.units[1].radius() - r).abs() < 1e-3, "radius {r}");
        { let u = &mut b.units[1]; u.place_at((99.5 + 2.0 * r, 0.0), u.facing); }
        let i = b.morale_inputs(0);
        assert!(i.enemy_frightens_near && i.horses_frightened && !i.friend_inspires_near);
        // Past 100 m between the edges is not "within".
        { let u = &mut b.units[1]; u.place_at((100.01 + 2.0 * r, 0.0), u.facing); }
        { let u = &mut b.units[2]; u.place_at((0.0, 100.01 + 2.0 * r), u.facing); }
        let i = b.morale_inputs(0);
        assert!(!i.enemy_frightens_near && !i.horses_frightened);
        // Infantry is not frightened by camels; a friend's inspiration counts.
        { let u = &mut b.units[2]; u.place_at((60.0, 0.0), u.facing); }
        let mut c = b.morale_inputs(1);
        assert!(!c.horses_frightened);
        b.units[1].attributes.inspires = false;
        b.units[2].attributes.inspires = true;
        c = b.morale_inputs(1);
        assert!(c.friend_inspires_near);
    }

    #[test]
    fn steadfast_wires_shock_and_fatigue_flags() {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        let mut u = LandUnit::new(0, 0, 120, (0.0, 0.0));
        u.attributes.steadfast = true;
        b.add_unit(u);
        b.add_unit(LandUnit::new(1, 1, 120, (500.0, 0.0)));
        assert!(!b.morale_inputs(0).shock_persists);
        assert!(b.morale_inputs(1).shock_persists);
    }

    #[test]
    fn general_status_follows_the_general_unit() {
        use crate::battle::morale::GeneralStatus;
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 100, (0.0, 500.0)));
        // No general unit: "present", rank 0, distance 0 (the army object starts with +0x1B0 = 1).
        assert_eq!(b.general_status(0), GeneralStatus::Present { rank: 0, distance: None });
        let mut g = LandUnit::new(3, 0, 24, (30.0, 40.0));
        g.general_rank = Some(4);
        b.add_unit(g);
        assert_eq!(b.general_status(0), GeneralStatus::Present { rank: 4, distance: Some(50.0) });
        assert_eq!(b.general_status(2), GeneralStatus::Present { rank: 4, distance: None });
        // The other side has none.
        assert_eq!(b.general_status(1), GeneralStatus::Present { rank: 0, distance: None });
        // Routing general: fled; destroyed: died recently, then dead.
        b.units[2].morale.behaviour = MoraleBehaviour::Routing;
        assert_eq!(b.general_status(0), GeneralStatus::FledRecently);
        b.units[2].morale.behaviour = MoraleBehaviour::Normal;
        b.units[2].men = 0;
        b.step();
        assert_eq!(b.general_status(0), GeneralStatus::DiedRecently);
        b.tick += GENERAL_DIED_RECENTLY_TICKS;
        assert_eq!(b.general_status(0), GeneralStatus::Dead);
        // Allied armies: units of army 1 and later on the side.
        let mut ally = LandUnit::new(4, 0, 100, (-30.0, 0.0));
        ally.army_index = 1;
        b.add_unit(ally);
        assert_eq!(b.allied_units(0), 1);
        assert_eq!(b.allied_units(1), 0);
    }

    #[test]
    fn reinforcements_arrive_one_at_a_time() {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 100, (0.0, 900.0)));
        let reinf = |id: u32, state: Reinforcement, class: &str| {
            let mut u = LandUnit::new(id, 0, 50, (0.0, 0.0));
            u.active = false;
            u.army_index = 1;
            u.reinforcement = state;
            u.unit_class = class.into();
            u
        };
        b.add_unit(reinf(3, Reinforcement::Waiting, "infantry_line"));
        b.add_unit(reinf(4, Reinforcement::Waiting, "artillery_fixed"));
        b.add_unit(reinf(5, Reinforcement::Held, "infantry_line"));
        b.add_unit(reinf(6, Reinforcement::Waiting, "cavalry_light"));
        b.reinforcement_entries.push(((0, 1), (500.0, -400.0), 1.0));
        b.step();
        // The first eligible unit starts to arrive at the entry point; the others wait.
        assert!(matches!(b.units[2].reinforcement, Reinforcement::Arriving { .. }));
        // It enters at the entry point (and walks a step towards the centre in the same tick).
        let p = b.units[2].position;
        assert!(dist2(p, (500.0, -400.0)) < 4.0, "{p:?}");
        assert_eq!(b.units[2].destination, Some((0.0, 0.0)));
        assert!(!b.units[2].active, "not on the field while walking in");
        assert_eq!(b.units[5].reinforcement, Reinforcement::Waiting);
        for _ in 0..REINFORCEMENT_WALK_ON_TICKS {
            b.step();
        }
        assert!(b.units[2].active && b.units[2].reinforcement == Reinforcement::None);
        // Fixed artillery never arrives; the held unit waits for the script; unit 6 is next.
        assert!(matches!(b.units[5].reinforcement, Reinforcement::Arriving { .. }));
        // The second unit enters one slot to the side (spread, not stacked).
        let side_off = dist2(b.units[5].position, (500.0, -400.0)).sqrt();
        assert!((side_off - REINFORCEMENT_SPACING_M).abs() < 3.0, "{side_off}");
        assert_eq!(b.units[3].reinforcement, Reinforcement::Waiting);
        assert_eq!(b.units[4].reinforcement, Reinforcement::Held);
        assert!(b.units[4].is_out_of_fight());
        b.deploy_reinforcement(5, true);
        assert_eq!(b.units[4].reinforcement, Reinforcement::Waiting);
    }

    #[test]
    fn reinforcements_stop_at_the_cap_and_a_mod_lifts_it() {
        // 22 reinforcement units: the original brings 20 of an army (0x00608200); a mod's 30 all.
        let arrived = |cap: usize| {
            let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
            b.max_reinforcement_units = cap;
            b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
            b.add_unit(LandUnit::new(2, 1, 100, (0.0, 900.0)));
            for id in 3..25 {
                let mut u = LandUnit::new(id, 0, 50, (0.0, 0.0));
                (u.active, u.army_index, u.reinforcement) = (false, 1, Reinforcement::Waiting);
                b.add_unit(u);
            }
            b.reinforcement_entries.push(((0, 1), (500.0, -400.0), 1.0));
            for _ in 0..23 * (REINFORCEMENT_WALK_ON_TICKS + 1) {
                b.step();
            }
            b.units.iter().filter(|u| u.army_index == 1 && u.reinforcement == Reinforcement::None).count()
        };
        assert_eq!(arrived(crate::limits::GameLimits::default().max_reinforcement_units as usize), 20);
        assert_eq!(arrived(30), 22);
    }

    #[test]
    fn reinforcements_join_when_inside_the_playable_area() {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.playable_area = Some([-500.0, -500.0, 500.0, 500.0]);
        b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 100, (0.0, 400.0)));
        let mut r = LandUnit::new(3, 0, 60, (0.0, 0.0));
        r.active = false;
        r.army_index = 1;
        r.reinforcement = Reinforcement::Waiting;
        r.walk_speed = 1.5;
        r.formation_width = 20.0;
        r.formation_rank_spacing = 2.0;
        b.add_unit(r);
        // Entering at the south edge, facing north.
        b.reinforcement_entries.push(((0, 1), (0.0, -500.0), std::f32::consts::FRAC_PI_2));
        b.step();
        assert!(!b.units[2].active);
        // Half its depth (3 m) is outside: about 2 s at 1.5 m/s.
        for _ in 0..30 {
            b.step();
        }
        assert!(b.units[2].active && b.units[2].reinforcement == Reinforcement::None);
        assert!(b.units[2].position.1 > -497.0 && b.units[2].position.1 < -490.0, "{:?}", b.units[2].position);
    }

    #[test]
    fn routers_leave_the_field_at_the_edge() {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.playable_area = Some([-100.0, -100.0, 100.0, 100.0]);
        let mut r = LandUnit::new(1, 0, 60, (0.0, -95.0));
        r.formation_width = 20.0;
        r.formation_rank_spacing = 2.0;
        r.run_speed = 4.0;
        r.morale.behaviour = MoraleBehaviour::Routing;
        b.add_unit(r);
        b.add_unit(LandUnit::new(2, 1, 100, (0.0, 50.0)));
        assert!(!b.units[0].left_field);
        for _ in 0..30 {
            b.step();
        }
        // Fled south past the edge: off the field for good, not killed, out of everyone's sight.
        let u = &b.units[0];
        assert!(u.left_field && !u.active && u.men == 60);
        let pos = u.position;
        b.step();
        assert_eq!(b.units[0].position, pos);
        // Nobody leaves without a playable area.
        let mut c = Battle::new(1, KvMorale::default(), KvFatigue::default());
        let mut r = LandUnit::new(1, 0, 60, (0.0, -95.0));
        r.morale.behaviour = MoraleBehaviour::Routing;
        c.add_unit(r);
        c.step();
        assert!(!c.units[0].left_field);
    }

    #[test]
    fn armies_past_255_per_side_stay_apart() {
        // `army_index` was a u8 the battle setup saturated at 255, so armies 255 and later shared
        // one general.
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        for (id, army) in [(1, 300), (2, 301)] {
            let mut u = LandUnit::new(id, 0, 100, (0.0, id as f32 * 50.0));
            u.army_index = army;
            u.general_rank = Some(1);
            b.add_unit(u);
        }
        assert_eq!(b.general_of(0, 300), Some(0));
        assert_eq!(b.general_of(0, 301), Some(1));
        assert_eq!(b.allied_units(0), 2);
    }
}

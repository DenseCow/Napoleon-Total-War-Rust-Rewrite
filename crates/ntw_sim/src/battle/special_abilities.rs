//! Special abilities: the gate, the toggle, the slot timers every ability with a slot shares, and
//! the general's Rally and Inspire (BATTLE_FIDELITY.md §60).
//!
//! In the original a unit has one slot per special ability its card lists (container at unit
//! `+0xDAC`, built by `0x0051DBE0` from the card's 16-byte records). A record carries the ability's
//! active time, recharge time and number of uses ([`AbilityRecord`]); a slot counts them down
//! ([`AbilitySlot`]). CONFIRMED static, all of it: the record sources (`0x0050CAE0`, `0x005363C0`,
//! `0x00518D70`), the gate and its per-ability conditions (`0x0053EA60`), the perform and toggle
//! paths (`0x005612D0`, `0x00578860`), activation (`0x0052ED80` → `0x0052ECC0`), switch-off
//! (`0x00549F30` → `0x00549F10`), plugging bayonets (`0x0054E8E0` / `0x0054DB50`), the tick
//! (`0x005805E0`, `0x00582830`), Rally (`0x0051DDB0`) and Inspire (`0x0051DAF0`, removal
//! `0x00522970`).
//!
//! PROVISIONAL, the stand-ins of this module (each is marked "stand-in" where it is used):
//! 1. the gate's (`0x0053EA60`) and the perform path's (`0x005612D0`) inputs the model does not
//!    hold: the squad byte `+0x72C` (squad `+0xADC`), which with `0x0054EE90` refuses an inactive
//!    square, pike square, pike wall or dismount, and the unit byte `+0xE0C` that refuses stakes,
//!    are read as clear (their writers are not traced); unlimber's test of the current order
//!    (vfunc `+0xDC`) and of unit `+0x2A4` is read as "has guns"; for Rally and Inspire the unit's
//!    special-men list (unit `+0x280`, one entry whose vfunc `+0x1B0` returns 0) is read as "has
//!    men"; the refusal `0x0055C780` (battle byte `+0x2704` set and the unit's state 2 or 3) is
//!    read as "routing or shattered"; the building test (`0x0055B140`, the building of the unit's
//!    current order `+0xAE8`) reads the building the unit holds, the model having no order objects;
//!    the model also refuses a unit not on the field (`active` false: a reinforcement waiting or
//!    walking in) or without men, which neither `0x0053EA60` nor `0x005612D0` tests (a unit without
//!    men leaves its army's list at the next army update `0x00580390`; whether the original's
//!    callers can reach a waiting reinforcement is not traced); in deployment the perform path
//!    acts only when battle `+0x58` is 0 or the byte at unit `+0x1EC → +0xD4 → +0x38` is set
//!    (read as passing), and with a current order (unit `+0xAE8`, the order stack's count) it first
//!    hands the unit's current order the request (`0x0056AC40` → the order's vfunc `+0x54`) and
//!    stops there when it takes it (ours toggles at once, or refuses Rally and Inspire);
//! 2. the Inspire order's approach: the original's move (`0x00652950`) installs an approach task
//!    (`0x00621640`) holding the request {target, 75.0, run byte}, whose per-tick logic is not
//!    traced; here the general heads for the target until the two boxes are less than 75 m apart,
//!    and the box distance is the exact one where `0x005774A0` searches from the closest pair of
//!    corners;
//! 3. effects not ported: the artillery boosts (19 and 20, `0x0051D9C0` / `0x0051DA00`) and the
//!    column (21, `0x0051DA60`) run their slot but change nothing yet; stakes (11) are placed at once
//!    (`0x006525F0`), the original plants them over time; fougasse (9, 10) is not placed
//!    (`0x006526F0`); the formation shapes are not rebuilt (`0x005315A0`, `0x00579780`) nor laid out
//!    in deployment (`0x0054A7D0`, `0x0054A760`, `0x0054A850`); square switched on without a current
//!    order is queued in the original (battle `+0x94 → +0xBA8`), here it is active at once; dismount
//!    (15) acts in the original only when the unit has no current order (unit `+0xAE8` is 0), calls
//!    `0x00514900` under option 68 and, in deployment, re-lays the unit (`0x0054A710`), and its
//!    flag is not set by the toggle itself (the dismounting is not traced); the model has no order
//!    objects, so here the ability flips at once and nothing is re-laid; the event every toggle case
//!    but 5, 12..14, 17..21 posts (an id from `0x0057ADB0` by ability and on / off, with the unit and
//!    its centre `0x00563DA0`, to battle `+0x94 → +0x3D8`, vfunc `+8`) has no sink in the model;
//!    light infantry behaviour's extra call under option 52 (`0x00515210`) and unlimber's unit
//!    `+0xE30` (`0x0054C1F0`) are not ported.

use super::abilities::ability_ids;
use super::attributes::ability::{FIRE_AND_ADVANCE, PIKE_SQUARE_FORMATION, PLUG_BAYONETS, SQUARE_FORMATION};
use super::march::{SQUARE_MIN_SIDE, SquadBox};
use super::model::{Battle, LandUnit, dist2};
use super::morale::{TransientEffect, effect_id};
use super::rules::x87_round;
use super::TICK_SECONDS;

/// Rally reaches units whose squad box is within this many metres of the general's (`0x00555F20`
/// returns 75.0, CONFIRMED); the Inspire order's approach uses the same number.
pub const GENERAL_ABILITY_RANGE_M: f32 = 75.0;
/// The rank step of Rally's bonus: the `f32` nearest 1/9 (`0x01325C98` = `0x3DE38E39`, CONFIRMED).
pub const RALLY_RANK_STEP: f32 = 0.111_111_11;

/// One special-ability record of a unit card (16 bytes with the ability id; built by `0x00518D90`,
/// CONFIRMED). A negative recharge time means the ability is not timed, a negative number of uses
/// that they are not counted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbilityRecord {
    /// `+4`: seconds the ability's timed effect lasts (`unit_special_abilities.active_time`).
    pub active_time: f32,
    /// `+8`: seconds before it can be used again (`recharge_time`); it stays active for that long.
    pub recharge_time: f32,
    /// `+0xC`: how many times it can be used in a battle (`num_uses`).
    pub num_uses: i32,
}

impl AbilityRecord {
    /// The default record (`0x00518D70`: −1.0, −1.0, −1, CONFIRMED): not timed, uses not counted.
    /// An ability without a row in `unit_special_abilities` keeps it.
    pub const UNTIMED: AbilityRecord = AbilityRecord { active_time: -1.0, recharge_time: -1.0, num_uses: -1 };
}

/// One ability slot of a unit (44 bytes in the original, built by `0x005148C0`, CONFIRMED). Whether
/// the ability is active is the unit's [`LandUnit::active_abilities`] bit (slot byte `+0x28`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbilitySlot {
    /// The ability enum value ([`super::attributes::ABILITY_NAMES`]).
    pub ability: u8,
    /// The card's record for it.
    pub record: AbilityRecord,
    /// `[7]`: seconds of the active time left, −1.0 when none runs.
    pub duration_left: f32,
    /// `[8]`: seconds of the recharge left; −1.0 in a new slot, 0 after the first tick.
    pub recharge_left: f32,
    /// `[9]`: uses left (the record's number in a new slot).
    pub uses_left: i32,
    /// Inspire only: the unit that holds the bonuses (the effect object's target, slot `[6]`).
    pub target: Option<u32>,
}

impl AbilitySlot {
    /// A new slot for `ability` (`0x005148C0`).
    pub fn new(ability: u8, record: AbilityRecord) -> Self {
        AbilitySlot { ability, record, duration_left: -1.0, recharge_left: -1.0, uses_left: record.num_uses, target: None }
    }

    /// The slot's own tick, `0x005805E0` (CONFIRMED): a running time loses 0.1 s; then a duration
    /// at or below 0 becomes −1.0 and a recharge at or below 0 becomes 0 (also the new slot's −1.0).
    fn tick(&mut self) {
        if self.duration_left > 0.0 {
            self.duration_left -= TICK_SECONDS;
        }
        if self.duration_left <= 0.0 {
            self.duration_left = -1.0;
        }
        if self.recharge_left > 0.0 {
            self.recharge_left -= TICK_SECONDS;
        }
        if self.recharge_left <= 0.0 {
            self.recharge_left = 0.0;
        }
    }
}

/// A general's pending Inspire order (the special-ability order `0x0051D940` with its behaviour
/// `0x0051E360`): the unit to inspire and whether the general runs to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InspireOrder {
    /// Id of the unit to inspire.
    pub target: u32,
    /// The order's run byte.
    pub run: bool,
}

/// Rally's morale bonus for a general of `rank`, `0x0051DDB0` (CONFIRMED): in single precision
/// `(rank × 1/9) × (max − min) + min`, rounded as the x87 stores an integer.
///
/// ORIGINAL BUG: the original converts the rank as an unsigned number here (`0x0051DDB0`), while
/// the card's rank is signed everywhere else (range −1..9, `0x00A198D0`; signed in `0x0053BC70`).
/// A general card of rank −1 becomes 4294967295, the product overflows the integer store and
/// every rallied unit gets a morale effect of −2147483648. We read the rank signed.
pub fn rally_bonus(rank: i32, min: i32, max: i32) -> i32 {
    x87_round((rank as f32 * RALLY_RANK_STEP) * (max as f32 - min as f32) + min as f32)
}

/// The fewest men a unit needs to form square: the smallest square template, side
/// [`SQUARE_MIN_SIDE`], takes `8n − 8` of them (`0x00658B30` against the template table's minimum
/// `0x00655240`, CONFIRMED static: 48). The original also refuses more than the largest template
/// holds (side 51, `8n − 1` = 407 men, `0x006551F0`); ours lays out a square of any size
/// ([`SQUARE_MIN_SIDE`]), so it has no upper bound.
pub const SQUARE_MIN_MEN: u32 = 8 * SQUARE_MIN_SIDE as u32 - 8;

/// Whether `ability` is activated through its slot (`0x0052ED80`: a use spent, the timers
/// started) and switched off through it (`0x00549F30`), by the toggle or the slot tick: the toggle
/// `0x00578860` does so for the formations and behaviours 0..4, 6, 7, the defences 9..11, unlimber
/// and 17..21 (CONFIRMED). Plug bayonets (8) and dismount (15) have their own switch, and loose
/// formation, chevaux de frise, earthworks and gabionade (5, 12..14) do nothing when performed.
fn uses_slot(ability: u8) -> bool {
    matches!(ability, 0..=4 | 6 | 7 | 9..=11 | 16..=21)
}

/// The four corners of a squad box.
fn corners(b: &SquadBox) -> [(f32, f32); 4] {
    let f = b.front();
    let along = (f.0 * b.depth * 0.5, f.1 * b.depth * 0.5);
    let across = (-f.1 * b.width * 0.5, f.0 * b.width * 0.5);
    let c = b.centre;
    [
        (c.0 + along.0 - across.0, c.1 + along.1 - across.1),
        (c.0 + along.0 + across.0, c.1 + along.1 + across.1),
        (c.0 - along.0 + across.0, c.1 - along.1 + across.1),
        (c.0 - along.0 - across.0, c.1 - along.1 - across.1),
    ]
}

/// Squared distance from point `p` to the segment `a`–`b`.
fn point_segment2(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (ab, ap) = ((b.0 - a.0, b.1 - a.1), (p.0 - a.0, p.1 - a.1));
    let len2 = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if len2 > 0.0 { ((ap.0 * ab.0 + ap.1 * ab.1) / len2).clamp(0.0, 1.0) } else { 0.0 };
    dist2(p, (a.0 + ab.0 * t, a.1 + ab.1 * t))
}

/// The squared distance between two squad boxes, `0x005774A0` (CONFIRMED static: 0 when the sum of
/// each box's smaller side, squared, is greater than the squared distance of the centres, 0 when
/// the boxes overlap, else the smallest squared distance between their edges). The original
/// searches from the closest pair of corners; this is the exact distance (stand-in 2 of the module
/// list).
pub fn box_gap2(a: &SquadBox, b: &SquadBox) -> f32 {
    let reach = a.width.min(a.depth) + b.width.min(b.depth);
    if reach * reach > dist2(a.centre, b.centre) {
        return 0.0;
    }
    let (ca, cb) = (corners(a), corners(b));
    // Overlap: no edge direction of either box separates the corners.
    let (fa, fb) = (a.front(), b.front());
    let separated = [fa, (-fa.1, fa.0), fb, (-fb.1, fb.0)].into_iter().any(|axis| {
        let span = |c: &[(f32, f32); 4]| {
            c.iter().map(|p| p.0 * axis.0 + p.1 * axis.1).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        let ((lo_a, hi_a), (lo_b, hi_b)) = (span(&ca), span(&cb));
        hi_a < lo_b || hi_b < lo_a
    });
    if !separated {
        return 0.0;
    }
    let mut best = f32::MAX;
    for (p, q) in [(&ca, &cb), (&cb, &ca)] {
        for &corner in p {
            for k in 0..4 {
                best = best.min(point_segment2(corner, q[k], q[(k + 1) & 3]));
            }
        }
    }
    best
}

impl LandUnit {
    /// Gives the unit its ability slots: one per `(ability, record)` the unit lists (the unit
    /// constructor's `0x0051DBE0` over the card's records, CONFIRMED). An ability it lists that has
    /// no record here gets the default one when it is first used ([`AbilityRecord::UNTIMED`]).
    pub fn set_ability_records(&mut self, records: impl IntoIterator<Item = (u8, AbilityRecord)>) {
        self.ability_slots.clear();
        for (ability, record) in records {
            if self.capabilities.has_ability(ability) && self.ability_slot(ability).is_none() {
                self.ability_slots.push(AbilitySlot::new(ability, record));
            }
        }
    }

    /// The unit's slot for `ability`, if it has one yet.
    pub fn ability_slot(&self, ability: u8) -> Option<&AbilitySlot> {
        self.ability_slots.iter().find(|s| s.ability == ability)
    }

    /// True while `ability` is active (the slot's byte `+0x28`).
    pub fn ability_active(&self, ability: u8) -> bool {
        self.active_abilities & (1u32 << ability) != 0
    }

    /// Adds Inspire's two stat bonuses (negative numbers take them away again): accuracy (unit
    /// `+0x160`, here every shot the unit's weapon can load) and melee attack (unit `+0x170`).
    fn shift_inspire_stats(&mut self, accuracy: i32, melee_attack: i32) {
        self.melee_attack = self.melee_attack.wrapping_add(melee_attack);
        if let Some(w) = &mut self.missile {
            w.accuracy += accuracy as f32;
        }
        for (_, w) in &mut self.shot_options {
            w.accuracy += accuracy as f32;
        }
    }
}

impl Battle {
    /// Whether unit `i` can perform `ability` now, `0x0053EA60` (CONFIRMED static): the one gate the
    /// player's button, the AI and the scripts go through. The unit must be on the field with men
    /// (PROVISIONAL, stand-in 1: the original tests neither here) and list the ability; wedge and
    /// diamond (2, 3) are refused while dismount is active; an ability with counted uses is refused
    /// when none is left, and a timed one while its recharge runs. Then, per ability:
    /// - square, pike square, pike wall and dismount (1, 0, 6, 15): refused in a building
    ///   (`0x0055B140`, the building of the unit's current order; ours, the building it holds:
    ///   stand-in 1) and, unless already active, while the squad byte `+0x72C` is set (stand-in 1)
    ///   or enemies attack the unit (`0x0054EE90`: a squad's committed attacker list is not empty,
    ///   `0x00557430`); square also needs the men for its smallest template ([`SQUARE_MIN_MEN`]);
    /// - fire and advance (7): [`Battle::can_fire_and_advance`];
    /// - plug bayonets (8): in deployment always, otherwise only while not plugged;
    /// - stakes (11): refused in a building and in deployment;
    /// - unlimber (16): with the gabionade selected, only in deployment and with guns; otherwise
    ///   with guns;
    /// - the artillery boosts (19, 20): refused in deployment, for limbered artillery
    ///   ([`Battle::limbered_artillery`]) and for a unit on foot ([`super::strength::on_foot`]);
    /// - every other ability (Rally and Inspire after stand-in 1): allowed.
    ///
    /// The tests the model cannot feed are stand-in 1 of the module list.
    pub fn can_perform_special_ability(&self, i: usize, ability: u8) -> bool {
        let u = &self.units[i];
        // Stand-in 1: `active` and `men` are the model's own precondition, not the gate's.
        if !u.active || u.men == 0 || !u.capabilities.has_ability(ability) {
            return false;
        }
        if super::strength::dismount_active(u)
            && (ability == ability_ids::WEDGE_FORMATION || ability == ability_ids::DIAMOND_FORMATION)
        {
            return false;
        }
        let spent = |s: &AbilitySlot| {
            (s.record.num_uses >= 0 && s.uses_left == 0) || (s.record.recharge_time >= 0.0 && s.recharge_left > 0.0)
        };
        if u.ability_slot(ability).is_some_and(spent) {
            return false;
        }
        let guns = u.missile.map_or(0, |w| w.guns);
        match ability {
            PIKE_SQUARE_FORMATION | SQUARE_FORMATION | ability_ids::PIKE_WALL_FORMATION | ability_ids::DISMOUNT => {
                // An inactive ability is refused while the squad byte `+0x72C` is set (stand-in 1:
                // read as clear) or enemies attack the unit (`0x0054EE90`).
                let engaged = !u.attackers.entries().is_empty();
                if u.garrison.is_some() || (!u.ability_active(ability) && engaged) {
                    return false;
                }
                // The original counts the unit's entities (`+0x2CC`), the men for infantry.
                ability != SQUARE_FORMATION || u.men >= SQUARE_MIN_MEN
            }
            FIRE_AND_ADVANCE => self.can_fire_and_advance(i),
            PLUG_BAYONETS => self.deploying || !u.bayonets_plugged,
            // Stand-in 1: the unit byte `+0xE0C` is read as clear.
            ability_ids::WOODEN_STAKES => u.garrison.is_none() && !self.deploying,
            ability_ids::UNLIMBER if u.deployable == Some(ability_ids::GABIONADE) => self.deploying && guns != 0,
            // Stand-in 1: the current order's vfunc `+0xDC` and unit `+0x2A4` are not tested.
            ability_ids::UNLIMBER => guns != 0,
            ability_ids::ARTILLERY_ACCURACY_BOOST | ability_ids::ARTILLERY_ROF_BOOST => {
                !self.deploying && !self.limbered_artillery(i) && !super::strength::on_foot(u)
            }
            // Stand-in 1 for Rally and Inspire: the special-men list is read as "has men".
            _ => true,
        }
    }

    /// Whether unit `i` may fire and advance, `0x0053EFE0` (CONFIRMED static): it needs a
    /// projectile; grenades (shot types 19 and 20) always may; any other only with bayonets not
    /// plugged, not riding unless its stats allow it (column 65,
    /// [`super::attributes::UnitAttributes::fires_mounted`]), and rounds left (unit `+0xE10`).
    pub fn can_fire_and_advance(&self, i: usize) -> bool {
        let u = &self.units[i];
        if u.missile.is_none() {
            return false;
        }
        if matches!(u.shot_type, Some(19 | 20)) {
            return true;
        }
        !u.bayonets_plugged && (!super::strength::riding(u) || u.attributes.fires_mounted) && u.ammunition != 0
    }

    /// Limbered artillery, `0x0055C230` (CONFIRMED static): the gabionade is not selected, the unit
    /// is artillery (type record `+0x1C` == 1), it could unlimber now (the gate for 16) and has not.
    /// The artillery boosts' gate and the skirmish default ([`Battle::skirmish_default`]) read it.
    pub fn limbered_artillery(&self, i: usize) -> bool {
        let u = &self.units[i];
        u.deployable != Some(ability_ids::GABIONADE)
            && super::strength::category(u) == super::strength::Category::Artillery
            && !u.ability_active(ability_ids::UNLIMBER)
            && self.can_perform_special_ability(i, ability_ids::UNLIMBER)
    }

    /// Performs a special ability for unit `id`, `0x005612D0` (CONFIRMED): the one entry of the
    /// player's ability button, the AI and the scripts ([`Battle::order_special_ability`]). Nothing
    /// happens when the unit is refused (`0x0055C780`, stand-in 1) or fails the gate
    /// ([`Battle::can_perform_special_ability`]). In deployment (`0x0055AD60`) the original toggles
    /// only an ability outside Rally and Inspire (17, 18), so those two are refused there (its
    /// order path with a current order: stand-in 1). Out of deployment Inspire (18) becomes an
    /// order: the general goes to `target` (running when `run`) and inspires it on arrival
    /// ([`InspireOrder`]); any other order replaces it. Every other ability is done at once (Rally,
    /// wedge, diamond and fire and advance in the original too; the others' order acts when it
    /// starts and the model has no order queue) through the toggle (`0x00578860`,
    /// [`Battle::do_special_ability`]). Who may be inspired is the caller's choice (the AI offers
    /// its alliance's units); the ability code does not test it. Returns whether the ability was
    /// performed or its order taken.
    pub fn perform_special_ability(&mut self, id: u32, ability: u8, target: Option<u32>, run: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        if self.units[i].morale.is_routing_or_shattered() || !self.can_perform_special_ability(i, ability) {
            return false;
        }
        if self.deploying && matches!(ability, ability_ids::RALLY | ability_ids::INSPIRE_UNIT) {
            return false;
        }
        if ability != ability_ids::INSPIRE_UNIT {
            return self.do_special_ability(i, ability, target);
        }
        let Some(target) = target.filter(|&t| self.unit_index(t).is_some()) else { return false };
        let u = &mut self.units[i];
        u.set_destination(None);
        u.fire_target = None;
        u.inspire_order = Some(InspireOrder { target, run });
        u.running = run;
        true
    }

    /// The toggle `0x00578860` (CONFIRMED static): an inactive ability must pass the gate again.
    /// Then, per ability:
    /// - plug bayonets (8): plugged bayonets are unplugged in deployment (`0x0054DB50`), otherwise
    ///   the bayonets are plugged (`0x0054E8E0`) ([`Battle::plug_bayonets`]);
    /// - dismount (15): flips (stand-in 3: the original acts only with no current order, unit
    ///   `+0xAE8` 0, and sets no flag itself);
    /// - column (21): its slot is activated again even while active (`0x0052ED80`; refused while
    ///   it recharges), never switched off here;
    /// - unlimber (16): switching on first drops a selected gabionade (`0x00544620`, the unit's
    ///   deployable becomes `none`); both ways the skirmish default is set again (`0x005357B0`);
    /// - every other ability with a slot ([`uses_slot`]): an active one is switched off
    ///   (`0x00549F30`), an inactive one activated (`0x0052ED80`);
    /// - loose formation, chevaux de frise, earthworks, gabionade (5, 12..14): nothing.
    ///
    /// Through [`Battle::perform_special_ability`] a timed ability is never switched off: the gate
    /// refuses it while its recharge runs, and the tick ends it when the recharge is over.
    ///
    /// Every case but 5, 12..14 and 17..21 also posts the ability event `0x0057ADB0` at the unit's
    /// centre, which the model has no sink for (stand-in 3). The original returns the low byte of
    /// its last callee's result; ours returns whether the ability acted.
    fn do_special_ability(&mut self, i: usize, ability: u8, target: Option<u32>) -> bool {
        let active = self.units[i].ability_active(ability);
        if !active && !self.can_perform_special_ability(i, ability) {
            return false;
        }
        match ability {
            PLUG_BAYONETS => {
                let plug = !(self.units[i].bayonets_plugged && self.deploying);
                self.plug_bayonets(i, plug);
                true
            }
            ability_ids::DISMOUNT => {
                self.units[i].active_abilities ^= 1u32 << ability;
                true
            }
            ability_ids::COLUMN_FORMATION => self.activate_ability(i, ability, target),
            ability_ids::UNLIMBER => {
                let done = if active {
                    self.deactivate_ability(i, ability);
                    true
                } else {
                    let u = &mut self.units[i];
                    if u.deployable == Some(ability_ids::GABIONADE) {
                        u.deployable = None;
                    }
                    self.activate_ability(i, ability, target)
                };
                self.units[i].skirmish = self.skirmish_default(i);
                done
            }
            a if uses_slot(a) => {
                if active {
                    self.deactivate_ability(i, ability);
                    true
                } else {
                    self.activate_ability(i, ability, target)
                }
            }
            _ => true,
        }
    }

    /// Plugs (`plug` true, `0x0054E8E0`) or unplugs (`0x0054DB50`) unit `i`'s bayonets (CONFIRMED
    /// static): unit `+0xD9D` is set or cleared, melee attack (unit `+0x170`) gains or loses the
    /// `kv_rules` value `bayonet_melee_attack_bonus`, and the unit's state bytes are set again
    /// (`0x005357B0`: the skirmish default). The byte both also write, unit `+0xD98`, is overwritten
    /// at once by `0x005357B0` with "cannot fire and advance" (`!0x0053EFE0`), so it is not modelled.
    /// Like the original, neither tests the present state: the gate and the toggle only plug
    /// unplugged bayonets and unplug plugged ones.
    fn plug_bayonets(&mut self, i: usize, plug: bool) {
        let bonus = self.kv_rules.bayonet_melee_attack_bonus;
        let u = &mut self.units[i];
        u.bayonets_plugged = plug;
        u.melee_attack = u.melee_attack.wrapping_add(if plug { bonus } else { bonus.wrapping_neg() });
        self.units[i].skirmish = self.skirmish_default(i);
    }

    /// Activates a slot, `0x0052ED80` → `0x0052ECC0` (CONFIRMED): refused without a use left or
    /// while the recharge runs (`0x0053E940`); the slot's old effect ends first; the ability's
    /// effect is made; a counted use is spent; a timed ability starts its active time and its
    /// recharge; the slot is active. The effects in the model: Rally, Inspire, square (the melee
    /// square flag, combatant `+0x54`) and stakes (placed in front of the unit, `0x006525F0`; at
    /// once, stand-in 3).
    fn activate_ability(&mut self, i: usize, ability: u8, target: Option<u32>) -> bool {
        let inspired = if ability == ability_ids::INSPIRE_UNIT {
            let Some(t) = target.and_then(|t| self.unit_index(t)) else { return false };
            Some(t)
        } else {
            None
        };
        let u = &mut self.units[i];
        let k = u.ability_slots.iter().position(|s| s.ability == ability).unwrap_or_else(|| {
            u.ability_slots.push(AbilitySlot::new(ability, AbilityRecord::UNTIMED));
            u.ability_slots.len() - 1
        });
        let slot = u.ability_slots[k];
        if slot.uses_left == 0 || slot.recharge_left > 0.0 {
            return false;
        }
        self.deactivate_ability(i, ability);
        match (ability, inspired) {
            (ability_ids::RALLY, _) => self.rally(i, slot.record.active_time),
            (_, Some(t)) => self.inspire(t, slot.record.active_time),
            (SQUARE_FORMATION, _) => self.units[i].in_square = true,
            (ability_ids::WOODEN_STAKES, _) => self.build_defence(i, ability_ids::WOODEN_STAKES),
            // Stand-in 3: the other effects are not ported.
            _ => {}
        }
        let u = &mut self.units[i];
        let s = &mut u.ability_slots[k];
        s.target = inspired.and(target);
        if s.record.num_uses >= 0 {
            s.uses_left -= 1;
        }
        if s.record.recharge_time >= 0.0 {
            s.duration_left = s.record.active_time;
            s.recharge_left = s.record.recharge_time;
        }
        u.active_abilities |= 1u32 << ability;
        true
    }

    /// Ends a slot's effect and clears its active flag (`0x00549F10`, CONFIRMED). Inspire's effect
    /// takes its two stat bonuses off the target again, re-reading the `kv_rules` values as the
    /// original does (`0x00522970`); Rally's leaves nothing to undo (its morale effect runs out by
    /// its own time); square's clears the melee square flag. PROVISIONAL: the original skips the
    /// Inspire subtraction while the battle byte `+0x54` is set, a flag whose meaning is UNKNOWN
    /// (`0x00537E40` reads it too); here it always subtracts.
    fn deactivate_ability(&mut self, i: usize, ability: u8) {
        let u = &mut self.units[i];
        if !u.ability_active(ability) {
            return;
        }
        u.active_abilities &= !(1u32 << ability);
        if ability == SQUARE_FORMATION {
            u.in_square = false;
        }
        let target = u.ability_slots.iter_mut().find(|s| s.ability == ability).and_then(|s| s.target.take());
        if let Some(t) = target.and_then(|t| self.unit_index(t)) {
            let (accuracy, melee_attack) = self.inspire_stat_bonuses();
            self.units[t].shift_inspire_stats(accuracy.wrapping_neg(), melee_attack.wrapping_neg());
        }
    }

    /// The slots' tick, `0x00582830`, the last step of the unit update `0x0057F070` (CONFIRMED):
    /// each slot's own tick ([`AbilitySlot::tick`]), then an active timed ability whose recharge
    /// has reached 0 is switched off. So a timed ability stays active for its whole recharge time
    /// (180 s for Rally and Inspire in the shipped table), longer than its active time (30 s).
    /// Only the abilities activated through their slot ([`uses_slot`]) are switched off here
    /// (dismount flips its flag without its slot, stand-in 3).
    pub(super) fn ability_tick(&mut self, i: usize) {
        for k in 0..self.units[i].ability_slots.len() {
            let u = &mut self.units[i];
            let s = &mut u.ability_slots[k];
            s.tick();
            let (ability, over) = (s.ability, s.record.recharge_time >= 0.0 && s.recharge_left == 0.0);
            if over && uses_slot(ability) && u.ability_active(ability) {
                self.deactivate_ability(i, ability);
            }
        }
    }

    /// Rally's effect, `0x0051DDB0` (CONFIRMED): every unit of the general's alliance (all its
    /// armies; not the general's own unit) whose squad box is in range gets a timed morale effect
    /// of [`rally_bonus`] for the ability's active time, and its rout timer is set to −1, so a
    /// routing unit that can rally does so at its next morale update. In range means
    /// `(own radius + 75 + other radius)² >` the squared distance of the two boxes' centres.
    /// The rank is the general card's (`+0xBC`; 0 on a card without a general, `0x00513A50`).
    ///
    /// The collector `0x006C9E40` tests only the identity and the range over every unit of each
    /// army's list (`+0xF0/+0xF4`), so the filter is which units that list holds. CONFIRMED for
    /// units without men and units that left the field: the army update `0x00580390`
    /// (per army from `0x00580320`, per alliance from `0x00582FB0`) erases, through `0x00575AE0`,
    /// every unit failing `0x0055CB50`: one that has left the map (`+0xAA0 == 2`) or has no
    /// entities left (count `+0x2CC == 0`). So they are gone from the list from the next update
    /// on (an order in between would rally a unit no longer on the field, with no effect).
    ///
    /// INFERRED: the skip of reinforcements (ours: `active` false while waiting or walking in).
    /// They have `+0xAA0 == 0` and their men, so `0x0055CB50` keeps them in their army's list:
    /// while waiting, the entry's army (entry `+0x28`, picked by `0x005E63A0`); from arrival on,
    /// the field army (entry `+0x2C`: `0x005BD160` → `0x0057CF60` → `0x0057CFF0`, which appends
    /// the unit to the field army's list through `0x00530E30` and erases it from the entry's).
    /// So the original rallies a walking-in unit if the field army is in the alliance's army
    /// list (`+0x20/+0x24`), and a waiting one if the entry's army is; neither is traced, and a
    /// likely difference is the walking-in case. Settled by tracing who fills the alliance list
    /// when the battle's armies are built from the setup (`reinforcement_army`: `0x0050AFE0` →
    /// `0x0050CAE0` with 1), or by a debugger read of an entry's `+0x28` / `+0x2C` against it.
    ///
    /// ORIGINAL BUG: the collector `0x006C9E40` appends without a bounds check into the caller's
    /// 100-unit stack buffer (`0x0051DDB0`), so a 101st unit in range overwrites the count and
    /// then the stack. There is no cap here (test `rally_reaches_more_than_a_hundred_units`).
    fn rally(&mut self, i: usize, active_time: f32) {
        let kv = &self.kv_morale;
        let g = &self.units[i];
        let value = rally_bonus(g.general_rank.unwrap_or(0), kv.special_ability_rally_min_bonus, kv.special_ability_rally_max_bonus);
        let (side, own) = (g.side, g.squad_box());
        let reach = own.radius + GENERAL_ABILITY_RANGE_M;
        for (j, u) in self.units.iter_mut().enumerate() {
            // `u.men == 0` and a unit that left the field: CONFIRMED (`0x00580390`, see above).
            // INFERRED: `!u.active` for reinforcements waiting or walking in (see above).
            if j == i || u.side != side || !u.active || u.men == 0 {
                continue;
            }
            let other = u.squad_box();
            let r = reach + other.radius;
            if r * r > dist2(own.centre, other.centre) {
                u.morale.transient_effects.push(TransientEffect { id: effect_id::RALLIED, value, seconds_left: active_time });
                u.morale.rout_timer = -1;
            }
        }
    }

    /// Inspire's two stat bonuses as the signed numbers the original adds (32-bit adds of the
    /// unsigned `kv_rules` values, `0x0051DAF0`).
    fn inspire_stat_bonuses(&self) -> (i32, i32) {
        let kv = &self.kv_rules;
        (kv.special_ability_inspire_unit_marksmanship_bonus as i32, kv.special_ability_inspire_unit_melee_attack_bonus as i32)
    }

    /// Inspire's effect on unit `t`, `0x0051DAF0` (CONFIRMED): accuracy and melee attack go up by
    /// the two `kv_rules` bonuses until the ability is switched off, and a timed morale effect of
    /// `special_ability_inspire_unit_bonus` runs for the ability's active time. The general's rank
    /// plays no part.
    fn inspire(&mut self, t: usize, active_time: f32) {
        let (accuracy, melee_attack) = self.inspire_stat_bonuses();
        let value = self.kv_morale.special_ability_inspire_unit_bonus;
        let u = &mut self.units[t];
        u.shift_inspire_stats(accuracy, melee_attack);
        u.morale.transient_effects.push(TransientEffect { id: effect_id::INSPIRED, value, seconds_left: active_time });
    }

    /// The pending Inspire order of unit `i`, each tick (the order's behaviour `0x0051E360`, run by
    /// the shared state-machine update `0x00564870`; CONFIRMED states: test the distance, move to
    /// the target `0x00558870`, inspire `0x005580C0`). The order is dropped
    /// when the general cannot act or the target is gone. Close enough, the general inspires
    /// through the toggle (`0x005580C0` → `0x00578860`) and the order ends; until then
    /// [`Battle::inspire_goal`] moves him (stand-in 2 of the module list).
    pub(super) fn inspire_step(&mut self, i: usize) {
        let Some(order) = self.units[i].inspire_order else { return };
        let u = &self.units[i];
        let target = self.unit_index(order.target).filter(|&t| self.units[t].active && self.units[t].men > 0);
        let near = match target {
            Some(t) if u.active && u.men > 0 && !u.morale.is_routing_or_shattered() => {
                box_gap2(&u.squad_box(), &self.units[t].squad_box()) < GENERAL_ABILITY_RANGE_M * GENERAL_ABILITY_RANGE_M
            }
            _ => {
                let u = &mut self.units[i];
                u.inspire_order = None;
                u.running = false;
                return;
            }
        };
        if near {
            let u = &mut self.units[i];
            u.inspire_order = None;
            u.running = false;
            self.do_special_ability(i, ability_ids::INSPIRE_UNIT, Some(order.target));
        }
    }

    /// Where a pending Inspire order sends unit `i`: its target's position.
    pub(super) fn inspire_goal(&self, i: usize) -> Option<(f32, f32)> {
        let order = self.units[i].inspire_order?;
        self.unit_index(order.target).map(|t| self.units[t].position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::attributes::UnitCapabilities;
    use crate::battle::fatigue::KvFatigue;
    use crate::battle::morale::{KvMorale, MoraleBehaviour};
    use crate::battle::shooting::MissileWeapon;

    /// The shipped `unit_special_abilities` row of `rally` and `inspire_unit`.
    const SHIPPED: AbilityRecord = AbilityRecord { active_time: 30.0, recharge_time: 180.0, num_uses: 2 };

    fn battle() -> Battle {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.kv_morale.special_ability_rally_min_bonus = 2;
        b.kv_morale.special_ability_rally_max_bonus = 10;
        b.kv_morale.special_ability_inspire_unit_bonus = 7;
        b.kv_rules.special_ability_inspire_unit_marksmanship_bonus = 10;
        b.kv_rules.special_ability_inspire_unit_melee_attack_bonus = 3;
        b
    }

    fn unit_of(id: u32, side: u8, men: u32, pos: (f32, f32)) -> LandUnit {
        let mut u = LandUnit::new(id, side, men, pos);
        u.walk_speed = 1.5;
        u.run_speed = 4.0;
        u.hold_position = true;
        u
    }

    /// A one-man unit: its box has no size, so distances are those of the positions.
    fn unit(id: u32, side: u8, pos: (f32, f32)) -> LandUnit {
        unit_of(id, side, 1, pos)
    }

    fn general_of(id: u32, men: u32, pos: (f32, f32), rank: Option<i32>, record: Option<AbilityRecord>) -> LandUnit {
        let mut u = unit_of(id, 0, men, pos);
        u.general_rank = rank;
        u.capabilities = UnitCapabilities::from_names(["rally", "inspire_unit"], []);
        if let Some(r) = record {
            u.set_ability_records([(ability_ids::RALLY, r), (ability_ids::INSPIRE_UNIT, r)]);
        }
        u
    }

    fn general(id: u32, pos: (f32, f32), rank: Option<i32>, record: Option<AbilityRecord>) -> LandUnit {
        general_of(id, 1, pos, rank, record)
    }

    fn rallied(b: &Battle, id: u32) -> Vec<TransientEffect> {
        let u = &b.units[b.unit_index(id).unwrap()];
        u.morale.transient_effects.iter().copied().filter(|t| t.id == effect_id::RALLIED).collect()
    }

    #[test]
    fn rally_bonus_grows_with_the_rank() {
        assert_eq!(RALLY_RANK_STEP.to_bits(), 0x3DE3_8E39);
        // (rank / 9) × (10 − 2) + 2, rounded to nearest.
        assert_eq!(rally_bonus(0, 2, 10), 2);
        assert_eq!(rally_bonus(4, 2, 10), 6); // 5.56
        assert_eq!(rally_bonus(5, 2, 10), 6); // 6.44
        assert_eq!(rally_bonus(9, 2, 10), 10);
        // min == max: the rank does not matter.
        assert_eq!(rally_bonus(7, 4, 4), 4);
    }

    /// Regression for the original's unsigned read of the rank: rank −1 is one step below rank 0,
    /// not an overflowed bonus.
    #[test]
    fn rally_bonus_reads_rank_minus_one_signed() {
        assert_eq!(rally_bonus(-1, 2, 10), 1); // 2 − 8/9 = 1.11
        assert_eq!(rally_bonus(-1, 0, 9), -1);
    }

    #[test]
    fn rally_reaches_the_alliance_within_range_only() {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), Some(9), Some(SHIPPED)));
        b.add_unit(unit(2, 0, (74.9, 0.0)));
        // Exactly at the range: the test is strict.
        b.add_unit(unit(3, 0, (75.0, 0.0)));
        // Another army of the same alliance.
        let mut ally = unit(4, 0, (0.0, -50.0));
        ally.army_index = 1;
        b.add_unit(ally);
        b.add_unit(unit(5, 1, (10.0, 0.0)));
        assert_eq!(b.units[0].radius(), 0.0, "a one-man box has no radius");
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        let effect = TransientEffect { id: effect_id::RALLIED, value: 10, seconds_left: 30.0 };
        assert_eq!(rallied(&b, 2), vec![effect]);
        assert_eq!(rallied(&b, 4), vec![effect]);
        assert!(rallied(&b, 3).is_empty(), "75 m is out of range");
        assert!(rallied(&b, 5).is_empty(), "enemies are not rallied");
        assert!(rallied(&b, 1).is_empty(), "the general's own unit is skipped");
    }

    /// Regression for the original's unchecked list (`0x006C9E40` into a 100-unit stack buffer):
    /// every allied unit in range is rallied, however many there are.
    #[test]
    fn rally_reaches_more_than_a_hundred_units() {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), Some(9), Some(SHIPPED)));
        for id in 2..=121 {
            b.add_unit(unit(id, 0, (10.0, 0.0)));
        }
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert!((2..=121).all(|id| rallied(&b, id).len() == 1), "all 120 units are rallied");
    }

    #[test]
    fn rally_adds_the_radii_to_the_range() {
        let mut b = battle();
        b.add_unit(general_of(1, 60, (0.0, 0.0), Some(0), Some(SHIPPED)));
        b.add_unit(unit_of(2, 0, 60, (0.0, 0.0)));
        let reach = b.units[0].radius() + GENERAL_ABILITY_RANGE_M + b.units[1].radius();
        assert!(reach > GENERAL_ABILITY_RANGE_M + 1.0, "60 men have a radius");
        // Between 75 m and the reach with the radii: in range.
        b.units[1].place_at((0.0, reach - 0.5), 0.0);
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert_eq!(rallied(&b, 2).len(), 1);
    }

    #[test]
    fn rally_lets_a_routing_unit_rally_at_once() {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), Some(3), Some(SHIPPED)));
        let mut routing = unit(2, 0, (20.0, 0.0));
        routing.morale.behaviour = MoraleBehaviour::Routing;
        routing.morale.rout_timer = 150;
        b.add_unit(routing);
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert_eq!(b.units[1].morale.rout_timer, -1);
        assert_eq!(rallied(&b, 2).len(), 1);
    }

    #[test]
    fn rally_needs_the_ability_and_a_unit_that_can_act() {
        let mut b = battle();
        // No ability listed.
        b.add_unit(unit(1, 0, (0.0, 0.0)));
        b.add_unit(unit(2, 0, (10.0, 0.0)));
        assert!(b.order_special_ability(1, ability_ids::RALLY).is_err());
        assert!(!b.perform_special_ability(1, ability_ids::RALLY, None, false));
        // A routing general.
        let mut g = general(3, (0.0, 5.0), Some(5), Some(SHIPPED));
        g.morale.behaviour = MoraleBehaviour::Routing;
        b.add_unit(g);
        assert_eq!(b.order_special_ability(3, ability_ids::RALLY), Ok(false));
        assert!(rallied(&b, 2).is_empty());
        assert_eq!(b.units[2].ability_slot(ability_ids::RALLY).unwrap().uses_left, 2, "no use was spent");
    }

    /// A unit that lists the ability but has no general card: the card's default rank 0 gives the
    /// minimum bonus.
    #[test]
    fn rally_without_a_general_rank_gives_the_minimum() {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), None, Some(SHIPPED)));
        b.add_unit(unit(2, 0, (10.0, 0.0)));
        assert_eq!(b.order_special_ability(1, ability_ids::RALLY), Ok(true));
        assert_eq!(rallied(&b, 2)[0].value, 2);
    }

    #[test]
    fn a_timed_ability_counts_uses_active_time_and_recharge() {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), Some(9), Some(SHIPPED)));
        b.add_unit(unit(2, 0, (10.0, 0.0)));
        let slot = |b: &Battle| *b.units[0].ability_slot(ability_ids::RALLY).unwrap();
        assert_eq!((slot(&b).duration_left, slot(&b).recharge_left, slot(&b).uses_left), (-1.0, -1.0, 2));
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert!(b.units[0].ability_active(ability_ids::RALLY));
        assert_eq!((slot(&b).duration_left, slot(&b).recharge_left, slot(&b).uses_left), (30.0, 180.0, 1));
        // While it recharges it can neither be used again nor switched off.
        b.step();
        assert!(!b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert_eq!(b.order_special_ability(1, ability_ids::RALLY), Ok(false));
        assert!(b.units[0].ability_active(ability_ids::RALLY));
        assert_eq!(rallied(&b, 2).len(), 1);
        // The active time runs out after about 30 s; the ability stays active.
        for _ in 0..310 {
            b.step();
        }
        assert_eq!(slot(&b).duration_left, -1.0);
        assert!(rallied(&b, 2).is_empty(), "the morale effect lasted the active time");
        assert!(b.units[0].ability_active(ability_ids::RALLY));
        // It is switched off when the recharge reaches 0, about 180 s after the use.
        let mut ticks = 311;
        while b.units[0].ability_active(ability_ids::RALLY) && ticks < 2000 {
            b.step();
            ticks += 1;
        }
        assert!((1795..=1805).contains(&ticks), "{ticks} ticks");
        assert_eq!(slot(&b).recharge_left, 0.0);
        // The second and last use.
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert_eq!(slot(&b).uses_left, 0);
        for _ in 0..1810 {
            b.step();
        }
        assert!(!b.units[0].ability_active(ability_ids::RALLY));
        assert!(!b.perform_special_ability(1, ability_ids::RALLY, None, false), "no use left");
    }

    /// An ability without a record (no row in the table) is not timed: it stays on until it is
    /// performed again, and its uses are not counted.
    #[test]
    fn an_ability_without_a_record_is_a_plain_toggle() {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), Some(9), None));
        b.add_unit(unit(2, 0, (10.0, 0.0)));
        for _ in 0..3 {
            assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
            assert!(b.units[0].ability_active(ability_ids::RALLY));
            for _ in 0..20 {
                b.step();
            }
            assert!(b.units[0].ability_active(ability_ids::RALLY));
            // The scripts' call does nothing to an active ability; the button switches it off.
            assert_eq!(b.order_special_ability(1, ability_ids::RALLY), Ok(false));
            assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
            assert!(!b.units[0].ability_active(ability_ids::RALLY));
        }
        assert_eq!(b.units[0].ability_slot(ability_ids::RALLY).unwrap().uses_left, -1);
    }

    /// A unit listing `abilities`, with `men` men.
    fn able(id: u32, men: u32, abilities: &[&'static str]) -> LandUnit {
        let mut u = unit_of(id, 0, men, (id as f32 * 50.0, 0.0));
        u.capabilities = UnitCapabilities::from_names(abilities.iter().copied(), []);
        u
    }

    fn weapon(guns: u32, is_artillery: bool) -> MissileWeapon {
        MissileWeapon {
            range: 400,
            accuracy: 40.0,
            reload_skill: 50,
            reload_time_s: 20,
            damage: 10.0,
            projectiles_per_shot: 1,
            is_artillery,
            guns,
            ballistics: Default::default(),
        }
    }

    /// A formation ability uses its slot like any other (`0x00578860` → `0x0052ED80`): a timed
    /// record spends a use and starts the timers, the gate refuses it while it recharges, and the
    /// tick ends it when the recharge is over, taking the melee square flag off.
    #[test]
    fn a_formation_ability_runs_its_slot() {
        let mut b = battle();
        let mut u = able(1, 60, &["square_formation"]);
        u.set_ability_records([(SQUARE_FORMATION, SHIPPED)]);
        b.add_unit(u);
        assert!(b.perform_special_ability(1, SQUARE_FORMATION, None, false));
        assert!(b.units[0].in_square);
        let slot = |b: &Battle| *b.units[0].ability_slot(SQUARE_FORMATION).unwrap();
        assert_eq!((slot(&b).uses_left, slot(&b).recharge_left, slot(&b).duration_left), (1, 180.0, 30.0));
        b.step();
        assert!(!b.perform_special_ability(1, SQUARE_FORMATION, None, false), "recharging");
        assert!(b.units[0].in_square);
        let mut ticks = 1;
        while b.units[0].ability_active(SQUARE_FORMATION) && ticks < 2000 {
            b.step();
            ticks += 1;
        }
        assert!((1795..=1805).contains(&ticks), "{ticks} ticks");
        assert!(!b.units[0].in_square);
        assert!(b.perform_special_ability(1, SQUARE_FORMATION, None, false));
        assert_eq!(slot(&b).uses_left, 0);
    }

    /// Square (`0x0053EA60`, ability 1): the men of its smallest template, no building, and not
    /// while engaged unless already active; switching it off clears the melee square flag.
    #[test]
    fn square_needs_its_men_and_no_melee_to_start() {
        assert_eq!(SQUARE_MIN_MEN, 48);
        let mut b = battle();
        b.add_unit(able(1, SQUARE_MIN_MEN - 1, &["square_formation"]));
        assert!(!b.can_perform_special_ability(0, SQUARE_FORMATION), "47 men");
        b.units[0].men = SQUARE_MIN_MEN;
        assert!(b.can_perform_special_ability(0, SQUARE_FORMATION));
        // Far above the original's 407: the limit is lifted.
        b.units[0].men = 2000;
        assert!(b.can_perform_special_ability(0, SQUARE_FORMATION));
        b.units[0].garrison = Some(0);
        assert!(!b.can_perform_special_ability(0, SQUARE_FORMATION), "in a building");
        b.units[0].garrison = None;
        assert!(b.perform_special_ability(1, SQUARE_FORMATION, None, false));
        assert!(b.units[0].in_square);
        b.units[0].attackers.add_contact(9);
        b.units[0].attackers.commit();
        // Active: it can still be switched off while engaged.
        assert!(b.perform_special_ability(1, SQUARE_FORMATION, None, false));
        assert!(!b.units[0].ability_active(SQUARE_FORMATION));
        assert!(!b.units[0].in_square);
        assert!(!b.perform_special_ability(1, SQUARE_FORMATION, None, false), "engaged");
    }

    /// Fire and advance (`0x0053EFE0`): a projectile; grenades always; otherwise bayonets not
    /// plugged, not riding unless column 65 allows it, and ammunition.
    #[test]
    fn fire_and_advance_needs_a_loaded_weapon() {
        let mut b = battle();
        b.add_unit(able(1, 60, &["fire_and_advance"]));
        let can = |b: &Battle| b.can_perform_special_ability(0, FIRE_AND_ADVANCE);
        assert!(!can(&b), "no projectile");
        b.units[0].missile = Some(weapon(0, false));
        b.units[0].ammunition = 10;
        assert!(can(&b));
        b.units[0].ammunition = 0;
        assert!(!can(&b), "no ammunition");
        b.units[0].shot_type = Some(19);
        assert!(can(&b), "grenades");
        b.units[0].shot_type = None;
        b.units[0].ammunition = 10;
        b.units[0].bayonets_plugged = true;
        assert!(!can(&b), "plugged");
        b.units[0].bayonets_plugged = false;
        b.units[0].is_cavalry = true;
        assert!(!can(&b), "riding");
        b.units[0].attributes.fires_mounted = true;
        assert!(can(&b), "column 65");
        // Switched on and off through its slot.
        assert!(b.perform_special_ability(1, FIRE_AND_ADVANCE, None, false));
        assert!(b.units[0].ability_active(FIRE_AND_ADVANCE));
        assert!(b.perform_special_ability(1, FIRE_AND_ADVANCE, None, false));
        assert!(!b.units[0].ability_active(FIRE_AND_ADVANCE));
    }

    /// Plug bayonets (`0x0054E8E0` / `0x0054DB50`): plugging adds the `kv_rules` bonus to melee
    /// attack; out of deployment plugged bayonets stay plugged, in deployment they come out again.
    #[test]
    fn bayonets_unplug_only_in_deployment() {
        let mut b = battle();
        b.kv_rules.bayonet_melee_attack_bonus = 4;
        let mut u = able(1, 60, &["plug_bayonets"]);
        u.melee_attack = 8;
        b.add_unit(u);
        let state = |b: &Battle| (b.units[0].bayonets_plugged, b.units[0].melee_attack);
        assert!(b.perform_special_ability(1, PLUG_BAYONETS, None, false));
        assert_eq!(state(&b), (true, 12));
        assert!(!b.perform_special_ability(1, PLUG_BAYONETS, None, false), "plugged, not deploying");
        assert_eq!(state(&b), (true, 12));
        b.deploying = true;
        assert!(b.perform_special_ability(1, PLUG_BAYONETS, None, false));
        assert_eq!(state(&b), (false, 8));
        assert!(b.perform_special_ability(1, PLUG_BAYONETS, None, false));
        assert_eq!(state(&b), (true, 12));
    }

    /// Stakes (11): refused in deployment and in a building.
    #[test]
    fn stakes_are_refused_in_deployment_and_buildings() {
        let mut b = battle();
        b.add_unit(able(1, 60, &["wooden_stakes"]));
        b.deploying = true;
        assert!(!b.perform_special_ability(1, ability_ids::WOODEN_STAKES, None, false));
        b.deploying = false;
        b.units[0].garrison = Some(0);
        assert!(!b.perform_special_ability(1, ability_ids::WOODEN_STAKES, None, false));
        b.units[0].garrison = None;
        assert!(b.perform_special_ability(1, ability_ids::WOODEN_STAKES, None, false));
        assert!(!b.defences.is_empty());
    }

    /// Unlimber (16): needs guns; with the gabionade selected only in deployment, and switching it
    /// on drops the gabionade.
    #[test]
    fn unlimber_needs_guns_and_drops_the_gabionade() {
        let mut b = battle();
        let mut u = able(1, 20, &["unlimber", "gabionade"]);
        u.unit_category = "artillery".into();
        b.add_unit(u);
        assert!(!b.perform_special_ability(1, ability_ids::UNLIMBER, None, false), "no guns");
        b.units[0].missile = Some(weapon(2, true));
        assert!(b.limbered_artillery(0));
        assert!(b.perform_special_ability(1, ability_ids::UNLIMBER, None, false));
        assert!(b.units[0].ability_active(ability_ids::UNLIMBER));
        assert!(!b.limbered_artillery(0));
        assert!(b.perform_special_ability(1, ability_ids::UNLIMBER, None, false));
        assert!(!b.units[0].ability_active(ability_ids::UNLIMBER));
        b.units[0].deployable = Some(ability_ids::GABIONADE);
        assert!(!b.perform_special_ability(1, ability_ids::UNLIMBER, None, false), "gabionade, not deploying");
        b.deploying = true;
        assert!(b.perform_special_ability(1, ability_ids::UNLIMBER, None, false));
        assert_eq!(b.units[0].deployable, None);
        assert!(b.units[0].ability_active(ability_ids::UNLIMBER));
    }

    /// The artillery boosts (19, 20): only unlimbered artillery with guns, out of deployment.
    #[test]
    fn artillery_boosts_need_unlimbered_guns() {
        let mut b = battle();
        let mut u = able(1, 20, &["unlimber", "artillery_accuracy_boost"]);
        u.unit_category = "artillery".into();
        b.add_unit(u);
        let boost = ability_ids::ARTILLERY_ACCURACY_BOOST;
        assert!(!b.can_perform_special_ability(0, boost), "no guns: on foot");
        b.units[0].missile = Some(weapon(2, true));
        assert!(!b.can_perform_special_ability(0, boost), "limbered");
        assert!(b.perform_special_ability(1, ability_ids::UNLIMBER, None, false));
        assert!(b.can_perform_special_ability(0, boost));
        b.deploying = true;
        assert!(!b.can_perform_special_ability(0, boost), "deployment");
        b.deploying = false;
        // Infantry is on foot.
        let mut inf = able(2, 20, &["artillery_accuracy_boost"]);
        inf.unit_category = "infantry".into();
        inf.missile = Some(weapon(2, true));
        b.add_unit(inf);
        assert!(!b.can_perform_special_ability(1, boost));
        assert!(b.perform_special_ability(1, boost, None, false));
        assert!(b.units[0].ability_active(boost));
    }

    /// Wedge and diamond (2, 3) are refused while dismounted.
    #[test]
    fn wedge_is_refused_while_dismounted() {
        let mut b = battle();
        let mut u = able(1, 60, &["wedge_formation", "dismount"]);
        u.is_cavalry = true;
        b.add_unit(u);
        let wedge = ability_ids::WEDGE_FORMATION;
        assert!(b.perform_special_ability(1, wedge, None, false));
        assert!(b.perform_special_ability(1, wedge, None, false));
        assert!(!b.units[0].ability_active(wedge));
        assert!(b.perform_special_ability(1, ability_ids::DISMOUNT, None, false));
        assert!(super::super::strength::dismount_active(&b.units[0]));
        assert!(!b.perform_special_ability(1, wedge, None, false));
    }

    fn inspire_battle(rank: i32, target_at: (f32, f32)) -> Battle {
        let mut b = battle();
        b.add_unit(general(1, (0.0, 0.0), Some(rank), Some(SHIPPED)));
        let mut t = unit(2, 0, target_at);
        t.melee_attack = 8;
        t.missile = Some(MissileWeapon {
            range: 80,
            accuracy: 40.0,
            reload_skill: 50,
            reload_time_s: 20,
            damage: 10.0,
            projectiles_per_shot: 1,
            is_artillery: false,
            guns: 0,
            ballistics: Default::default(),
        });
        b.add_unit(t);
        b
    }

    fn inspired(b: &Battle) -> (i32, f32, Vec<TransientEffect>) {
        let u = &b.units[1];
        (
            u.melee_attack,
            u.missile.unwrap().accuracy,
            u.morale.transient_effects.iter().copied().filter(|t| t.id == effect_id::INSPIRED).collect(),
        )
    }

    #[test]
    fn inspire_raises_the_targets_stats_until_it_ends() {
        for rank in [0, 9] {
            let mut b = inspire_battle(rank, (20.0, 0.0));
            assert!(b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(2), true));
            assert!(!b.units[0].ability_active(ability_ids::INSPIRE_UNIT), "it is an order first");
            b.step();
            assert!(b.units[0].ability_active(ability_ids::INSPIRE_UNIT));
            assert_eq!(b.units[0].inspire_order, None);
            // The same bonuses for any rank.
            let (melee, accuracy, effects) = inspired(&b);
            assert_eq!((melee, accuracy), (11, 50.0));
            assert_eq!((effects.len(), effects[0].value), (1, 7));
            // The morale effect lasts the active time, the stats the whole recharge.
            for _ in 0..310 {
                b.step();
            }
            let (melee, accuracy, effects) = inspired(&b);
            assert_eq!((melee, accuracy), (11, 50.0));
            assert!(effects.is_empty());
            for _ in 0..1500 {
                b.step();
            }
            assert!(!b.units[0].ability_active(ability_ids::INSPIRE_UNIT));
            assert_eq!((b.units[1].melee_attack, b.units[1].missile.unwrap().accuracy), (8, 40.0));
        }
    }

    #[test]
    fn the_general_goes_to_a_distant_target_before_inspiring() {
        let mut b = inspire_battle(5, (200.0, 0.0));
        assert!(b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(2), true));
        assert!(b.units[0].running);
        let mut ticks = 0;
        while !b.units[0].ability_active(ability_ids::INSPIRE_UNIT) && ticks < 2000 {
            b.step();
            ticks += 1;
        }
        let x = b.units[0].position.0;
        assert!(ticks > 100 && x > 120.0 && x < 130.0, "inspired from {x} after {ticks} ticks");
        assert_eq!(b.units[1].melee_attack, 11);
        assert_eq!(b.units[0].inspire_order, None);
        assert!(!b.units[0].running);
    }

    #[test]
    fn another_order_replaces_the_inspire_order() {
        let mut b = inspire_battle(5, (200.0, 0.0));
        assert!(b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(2), false));
        b.step();
        assert!(b.order_move(1, (0.0, 50.0)));
        assert_eq!(b.units[0].inspire_order, None);
        for _ in 0..600 {
            b.step();
        }
        assert!(!b.units[0].ability_active(ability_ids::INSPIRE_UNIT));
        assert_eq!(b.units[1].melee_attack, 8);
        assert_eq!(b.units[0].ability_slot(ability_ids::INSPIRE_UNIT).unwrap().uses_left, 2);
    }

    #[test]
    fn inspire_needs_a_target_and_is_dropped_when_it_dies() {
        let mut b = inspire_battle(5, (200.0, 0.0));
        assert!(!b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, None, false));
        assert!(!b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(99), false));
        assert_eq!(b.order_special_ability(1, ability_ids::INSPIRE_UNIT), Ok(false));
        assert!(b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(2), false));
        b.units[1].men = 0;
        b.step();
        assert_eq!(b.units[0].inspire_order, None);
        assert!(!b.units[0].ability_active(ability_ids::INSPIRE_UNIT));
    }

    /// `0x005612D0` toggles only abilities outside Rally and Inspire (17, 18) in deployment: both
    /// are refused there without spending a use, and work once the battle starts.
    #[test]
    fn rally_and_inspire_are_not_performed_in_deployment() {
        let mut b = inspire_battle(5, (20.0, 0.0));
        b.deploying = true;
        for ability in [ability_ids::RALLY, ability_ids::INSPIRE_UNIT] {
            assert!(!b.perform_special_ability(1, ability, Some(2), false), "ability {ability} in deployment");
            assert!(!b.units[0].ability_active(ability));
            assert_eq!(b.units[0].ability_slot(ability).unwrap().uses_left, 2, "no use was spent");
        }
        assert_eq!(b.units[0].inspire_order, None);
        assert!(b.units[1].morale.transient_effects.is_empty(), "nobody was rallied or inspired");
        b.deploying = false;
        assert!(b.perform_special_ability(1, ability_ids::RALLY, None, false));
        assert!(b.units[0].ability_active(ability_ids::RALLY));
        assert!(b.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(2), false));
        assert!(b.units[0].inspire_order.is_some());
    }

    #[test]
    fn box_gap_is_zero_for_near_boxes_and_the_edge_distance_otherwise() {
        let boxed = |centre: (f32, f32), heading: u16| SquadBox { centre, heading, width: 20.0, depth: 6.0, radius: 10.5 };
        // Heading 0 faces +y: 20 m wide along x, 6 m deep along y.
        let a = boxed((0.0, 0.0), 0);
        // Centres closer than the two smaller sides together.
        assert_eq!(box_gap2(&a, &boxed((11.0, 0.0), 0)), 0.0);
        // Side by side, 30 m between the edges.
        assert!((box_gap2(&a, &boxed((50.0, 0.0), 0)) - 900.0).abs() < 1e-2);
        // One behind the other, 14 m between the edges.
        assert!((box_gap2(&a, &boxed((0.0, 20.0), 0)) - 196.0).abs() < 1e-2);
        // Crossing boxes overlap.
        assert_eq!(box_gap2(&a, &boxed((0.0, 12.5), 0x4000)), 0.0);
        // Corner to corner: 30 m along x and 14 m along y.
        assert!((box_gap2(&a, &boxed((50.0, 20.0), 0)) - (900.0 + 196.0)).abs() < 1e-2);
    }

    #[test]
    fn ability_state_is_part_of_the_state_hash() {
        let mut a = inspire_battle(5, (20.0, 0.0));
        let b = inspire_battle(5, (20.0, 0.0));
        assert_eq!(a.state_hash(), b.state_hash());
        a.perform_special_ability(1, ability_ids::INSPIRE_UNIT, Some(2), false);
        assert_ne!(a.state_hash(), b.state_hash(), "the pending order");
        let mut c = inspire_battle(5, (20.0, 0.0));
        c.perform_special_ability(1, ability_ids::RALLY, None, false);
        assert_ne!(c.state_hash(), b.state_hash(), "the slot");
    }
}

//! Timed special abilities: the slot timers every special ability shares, and the general's Rally
//! and Inspire (BATTLE_FIDELITY.md §60 "Rally and Inspire").
//!
//! In the original a unit has one slot per special ability its card lists (container at unit
//! `+0xDAC`, built by `0x0051DBE0` from the card's 16-byte records). A record carries the ability's
//! active time, recharge time and number of uses ([`AbilityRecord`]); a slot counts them down
//! ([`AbilitySlot`]). CONFIRMED static, all of it: the record sources (`0x0050CAE0`, `0x005363C0`,
//! `0x00518D70`), the gate's use and recharge tests (`0x0053EA60`), the perform and toggle paths
//! (`0x005612D0`, `0x00578860`), activation (`0x0052ED80` → `0x0052ECC0`), the tick (`0x005805E0`,
//! `0x00582830`), Rally (`0x0051DDB0`) and Inspire (`0x0051DAF0`, removal `0x00522970`).
//!
//! PROVISIONAL, the stand-ins of this module (each is marked "stand-in" where it is used):
//! 1. the rest of the gate `0x0053EA60`: its per-ability conditions (formations, fire and advance,
//!    plug bayonets, stakes, unlimber, the artillery boosts) are read as "the unit is active and has
//!    men", and for Rally and Inspire the test of the unit's special-men list (unit `+0x280`, one
//!    entry whose vfunc `+0x1B0` returns 0) as the same; the refusal `0x0055C780` (battle byte
//!    `+0x2704` set and the unit's state 2 or 3) is read as "routing or shattered"; the model has no
//!    deployment phase, where the original performs neither Rally nor Inspire;
//! 2. the Inspire order's approach: the original's move (`0x00652950`) installs an approach task
//!    (`0x00621640`) holding the request {target, 75.0, run byte}, whose per-tick logic is not
//!    traced; here the general heads for the target until the two boxes are less than 75 m apart,
//!    and the box distance is the exact one where `0x005774A0` searches from the closest pair of
//!    corners;
//! 3. the artillery boosts (abilities 19 and 20, effects `0x0051D9C0` / `0x0051DA00`) run their
//!    slot timers here but have no effect yet; ability 21 (effect `0x0051DA60`, also built by
//!    `0x0052ECC0`) is switched on like a formation ability, without effect or slot; switching an
//!    active formation or behaviour ability off again is not ported. Formation and behaviour abilities do
//!    not use their slot: the original activates it on switch-on (`0x00578860` → `0x0052ED80` on
//!    the switch-on path of cases 0..4, 6, 7, 9..11 and 16), spending a use and starting the timers;
//!    here a mod's record for them (a `unit_special_abilities` row, or a battle file's own
//!    `recharge_time`) is ignored, and the slot tick leaves them alone ([`uses_slot`]).

use super::abilities::ability_ids;
use super::march::SquadBox;
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

/// Whether `ability` is activated through its slot here (`0x0052ED80`: a use spent, the timers
/// started) and switched off by the toggle or the slot tick: Rally, Inspire and the two artillery
/// boosts. Every other ability is switched on without its slot (stand-in 3).
fn uses_slot(ability: u8) -> bool {
    matches!(
        ability,
        ability_ids::RALLY | ability_ids::INSPIRE_UNIT | ability_ids::ARTILLERY_ACCURACY_BOOST | ability_ids::ARTILLERY_ROF_BOOST
    )
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
    /// Whether unit `i` can perform `ability` now, `0x0053EA60`. CONFIRMED: the unit must list the
    /// ability; wedge and diamond (2, 3) are refused while dismount is active; an ability with
    /// counted uses is refused when none is left, and a timed one while its recharge runs. The rest
    /// is stand-in 1 of the module list.
    pub fn can_perform_special_ability(&self, i: usize, ability: u8) -> bool {
        let u = &self.units[i];
        if !u.capabilities.has_ability(ability) {
            return false;
        }
        if u.ability_active(ability_ids::DISMOUNT) && (ability == 2 || ability == 3) {
            return false;
        }
        let spent = |s: &AbilitySlot| {
            (s.record.num_uses >= 0 && s.uses_left == 0) || (s.record.recharge_time >= 0.0 && s.recharge_left > 0.0)
        };
        if u.ability_slot(ability).is_some_and(spent) {
            return false;
        }
        u.active && u.men > 0
    }

    /// Performs a special ability for unit `id`, `0x005612D0` (CONFIRMED): the one entry of the
    /// player's ability button, the AI and the scripts ([`Battle::order_special_ability`]). Nothing
    /// happens when the unit is refused (`0x0055C780`, stand-in 1) or fails the gate
    /// ([`Battle::can_perform_special_ability`]). Inspire (18) becomes an order: the general goes
    /// to `target` (running when `run`) and inspires it on arrival ([`InspireOrder`]); any other
    /// order replaces it. Every other ability is done at once (Rally in the original too; the
    /// others' order acts when it starts and the model has no order queue): an inactive one is
    /// switched on, an active Rally, Inspire or artillery boost off (`0x00578860`). Who may be
    /// inspired is the caller's choice (the AI offers its alliance's units); the ability code does
    /// not test it. Returns whether the ability was performed or its order taken.
    pub fn perform_special_ability(&mut self, id: u32, ability: u8, target: Option<u32>, run: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        if self.units[i].morale.is_routing_or_shattered() || !self.can_perform_special_ability(i, ability) {
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

    /// The toggle `0x00578860` (CONFIRMED for Rally, Inspire and the artillery boosts): an inactive
    /// ability must pass the gate again and is activated; an active one of those four is switched
    /// off. Through [`Battle::perform_special_ability`] a timed ability is never switched off: the
    /// gate refuses it while its recharge runs, and the tick ends it when the recharge is over.
    fn do_special_ability(&mut self, i: usize, ability: u8, target: Option<u32>) -> bool {
        let active = self.units[i].ability_active(ability);
        if !active && !self.can_perform_special_ability(i, ability) {
            return false;
        }
        match ability {
            a if uses_slot(a) => {
                if active {
                    self.deactivate_ability(i, ability);
                    true
                } else {
                    self.activate_ability(i, ability, target)
                }
            }
            // Stand-in 3: an active formation or behaviour ability stays on.
            _ if active => false,
            _ => {
                self.switch_on_untimed_ability(i, ability);
                true
            }
        }
    }

    /// Activates a slot, `0x0052ED80` → `0x0052ECC0` (CONFIRMED): refused without a use left or
    /// while the recharge runs (`0x0053E940`); the slot's old effect ends first; the ability's
    /// effect is made; a counted use is spent; a timed ability starts its active time and its
    /// recharge; the slot is active.
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
            // Stand-in 3: the artillery boosts have no effect yet.
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
    /// its own time). PROVISIONAL: the original skips the subtraction while the battle byte
    /// `+0x54` is set, a flag whose meaning is UNKNOWN; here it always subtracts.
    fn deactivate_ability(&mut self, i: usize, ability: u8) {
        let u = &mut self.units[i];
        if !u.ability_active(ability) {
            return;
        }
        u.active_abilities &= !(1u32 << ability);
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
    /// Only the abilities activated through their slot ([`uses_slot`]) are switched off here: a
    /// formation or behaviour ability never started its timers (stand-in 3), so a timed record
    /// must not end it.
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

    /// Regression: a timed record on a formation ability (a mod's `unit_special_abilities` row, or a
    /// battle file's own `recharge_time`; both become the unit's slot records) does not end the
    /// formation on the next tick, and its slot stays unused (stand-in 3).
    #[test]
    fn a_timed_record_does_not_end_a_formation_ability() {
        use crate::battle::attributes::ability::SQUARE_FORMATION;
        let mut b = battle();
        let mut u = unit_of(1, 0, 60, (0.0, 0.0));
        u.capabilities = UnitCapabilities::from_names(["square_formation"], []);
        u.set_ability_records([(SQUARE_FORMATION, SHIPPED)]);
        b.add_unit(u);
        assert!(b.perform_special_ability(1, SQUARE_FORMATION, None, false));
        for _ in 0..20 {
            b.step();
        }
        assert!(b.units[0].ability_active(SQUARE_FORMATION));
        assert!(b.units[0].in_square);
        let slot = *b.units[0].ability_slot(SQUARE_FORMATION).unwrap();
        assert_eq!((slot.uses_left, slot.recharge_left, slot.duration_left), (2, 0.0, -1.0));
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

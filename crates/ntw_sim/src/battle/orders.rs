//! Extra unit orders for the battle AI and battle scripts (added by the AI worker).
//!
//! Additive only: these functions set the same order fields the existing orders
//! ([`Battle::order_move`], [`Battle::order_fire`]) and the player input already use. They change
//! no existing behaviour. They are named after the original's battle-script unit-controller calls,
//! whose names are CONFIRMED in the shipped battle scripts (`analysis/worker3/lua_api.txt`):
//! `halt`, `fire_at_will`, `attack_unit`. What those calls do inside the original is UNKNOWN; the
//! effects below are our own (PROVISIONAL) mapping onto this model's order fields.

use super::model::Battle;

/// How fast a unit moves to its destination: the two speeds a `battle_entities` row gives
/// (`walk_speed`, `run_speed`, CONFIRMED columns; the app copies them into `LandUnit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveSpeed {
    /// `LandUnit::walk_speed` (what `order_move` uses, and what the player's right-click order
    /// carries: `0x005DEAB0` builds every order with run = 0, CONFIRMED).
    #[default]
    Walk,
    /// `LandUnit::run_speed`: a script `goto` with its run argument (`0x00645610`, CONFIRMED), or a
    /// change of speed on the current move ([`Battle::order_move_speed`]). How the original's
    /// double right-click makes a run is not traced (UNKNOWN).
    Run,
}

impl Battle {
    /// The walk/run change of `BattleUI.Current_Selection_Walks` / `_Runs`
    /// (`BCQ_MULTIPLE_SELECTION_ORDER_CHANGE_MOVE_SPEED`, executor `0x005C0530`, per unit
    /// `0x005600C0`; CONFIRMED structure, BATTLE_FIDELITY.md §59): it edits the move the unit is
    /// making now (its current order, else its current movement) and is kept nowhere else, so a
    /// unit that is not moving keeps nothing and its next order brings its own speed. Returns
    /// whether the unit had a move to change. Not modelled (UNKNOWN meaning): the exe's two gates,
    /// `0x0055C780` (battle `+0x2704` with the unit's state 2 or 3) and, for a run, `0x0053F4B0`
    /// (`0x0055C1C0` or unit `+0x1A9` clear).
    pub fn order_move_speed(&mut self, id: u32, speed: MoveSpeed) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        // Routing units flee at their run speed whatever the option says (`movement_goal`).
        if self.units[i].morale.is_routing_or_shattered() || self.movement_goal(i).is_none() {
            return false;
        }
        self.units[i].running = speed == MoveSpeed::Run;
        true
    }

    /// `halt`: stop where you are. Clears any move and fire order and keeps the unit in place
    /// (`hold_position`), so the placeholder auto-advance does not move it either.
    /// Returns false if `id` is unknown.
    pub fn order_halt(&mut self, id: u32) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        let u = &mut self.units[i];
        u.destination = None;
        u.fire_target = None;
        u.hold_position = true;
        u.running = false;
        true
    }

    /// Sets whether the unit stays put when it has no move order (`hold_position`). With `false`
    /// the model's placeholder behaviour (advance on the nearest enemy) applies again.
    /// Returns false if `id` is unknown.
    pub fn order_hold_position(&mut self, id: u32, hold: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        self.units[i].hold_position = hold;
        true
    }

    /// `fire_at_will`: switches fire at will on or off. Returns false if `id` is unknown.
    pub fn order_fire_at_will(&mut self, id: u32, on: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        self.units[i].fire_at_will = on;
        true
    }

    /// `attack_unit` (melee): walk to `target` and charge it. Sets the destination to a point
    /// `stand_off` metres short of the target (on the line from the attacker), clears any fire
    /// order, and sets `charging` when `charge` is true. Call it again to follow a moving target.
    /// Returns false if either id is unknown or both are on the same side.
    pub fn order_attack_unit(&mut self, id: u32, target: u32, stand_off: f32, charge: bool) -> bool {
        let (Some(i), Some(t)) = (self.unit_index(id), self.unit_index(target)) else {
            return false;
        };
        if self.units[i].side == self.units[t].side {
            return false;
        }
        let from = self.units[i].position;
        let to = self.units[t].position;
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt();
        let dest = if len > stand_off && len > 0.0 {
            let k = (len - stand_off) / len;
            (from.0 + dx * k, from.1 + dy * k)
        } else {
            from
        };
        let u = &mut self.units[i];
        u.destination = (dest != from).then_some(dest);
        u.fire_target = None;
        u.hold_position = true;
        u.charging = charge;
        // A charge closes at the run speed (PROVISIONAL: the original's charge speed and when the
        // run starts are UNKNOWN; `battle_entities` run speed is the DB value we have).
        u.running = charge;
        true
    }

    /// `order_move` with a speed choice: walk or run to `dest` (`running` until it arrives).
    /// Returns false if `id` is unknown.
    pub fn order_move_at(&mut self, id: u32, dest: (f32, f32), speed: MoveSpeed) -> bool {
        if !self.order_move(id, dest) {
            return false;
        }
        let i = self.unit_index(id).expect("checked by order_move");
        self.units[i].running = speed == MoveSpeed::Run;
        true
    }

    /// Ends a charge (clears `charging`). Returns false if `id` is unknown.
    pub fn order_end_charge(&mut self, id: u32) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        self.units[i].charging = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::fatigue::KvFatigue;
    use super::super::model::LandUnit;
    use super::super::morale::KvMorale;
    use super::*;

    fn battle() -> Battle {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        b.add_unit(LandUnit::new(1, 0, 100, (0.0, 0.0)));
        b.add_unit(LandUnit::new(2, 1, 100, (100.0, 0.0)));
        b
    }

    #[test]
    fn halt_and_hold() {
        let mut b = battle();
        b.order_move(1, (50.0, 0.0));
        assert!(b.order_halt(1));
        assert_eq!(b.units[0].destination, None);
        assert!(b.units[0].hold_position);
        assert!(b.order_hold_position(1, false));
        assert!(!b.units[0].hold_position);
        assert!(!b.order_halt(99));
    }

    #[test]
    fn attack_unit_stops_short_and_charges() {
        let mut b = battle();
        assert!(b.order_attack_unit(1, 2, 5.0, true));
        assert_eq!(b.units[0].destination, Some((95.0, 0.0)));
        assert!(b.units[0].charging);
        assert!(b.order_end_charge(1));
        assert!(!b.units[0].charging);
        assert!(!b.order_attack_unit(1, 1, 5.0, true), "same side refused");
    }

    #[test]
    fn running_uses_the_run_speed_until_arrival() {
        let tick = super::super::TICK_SECONDS;
        let mut b = battle();
        b.units[0].walk_speed = 1.0;
        b.units[0].run_speed = 4.0;
        b.units[1].hold_position = true;
        assert!(b.order_move_at(1, (30.0, 0.0), MoveSpeed::Run));
        let x0 = b.units[0].position.0;
        b.step();
        let ran = b.units[0].position.0 - x0;
        assert!((ran - 4.0 * tick).abs() < 1e-4, "moved {ran}");
        assert!(b.order_move_at(1, (30.0, 0.0), MoveSpeed::Walk));
        let x1 = b.units[0].position.0;
        b.step();
        let walked = b.units[0].position.0 - x1;
        assert!((walked - 1.0 * tick).abs() < 1e-4, "moved {walked}");
        // A charge runs; arriving clears it.
        assert!(b.order_attack_unit(1, 2, 69.0, true));
        assert!(b.units[0].running);
        for _ in 0..200 {
            b.step();
        }
        assert!(!b.units[0].running, "cleared on arrival");
    }

    /// The Run button edits the move the unit is making, whatever it is (here the advance on the
    /// enemy a unit makes without orders), and the option ends with that move.
    #[test]
    fn change_of_speed_edits_the_current_move_only() {
        let tick = super::super::TICK_SECONDS;
        let mut b = battle();
        b.units[0].walk_speed = 1.0;
        b.units[0].run_speed = 4.0;
        b.units[1].hold_position = true;
        // Advancing without an order: Run makes it run.
        assert!(b.order_move_speed(1, MoveSpeed::Run));
        let x0 = b.units[0].position.0;
        b.step();
        assert!((b.units[0].position.0 - x0 - 4.0 * tick).abs() < 1e-4);
        // Walk again.
        assert!(b.order_move_speed(1, MoveSpeed::Walk));
        let x1 = b.units[0].position.0;
        b.step();
        assert!((b.units[0].position.0 - x1 - 1.0 * tick).abs() < 1e-4);
        // A unit with no move keeps nothing: its next (walk) order walks.
        b.units[0].hold_position = true;
        assert!(!b.order_move_speed(1, MoveSpeed::Run));
        assert!(!b.units[0].running);
        assert!(b.order_move_at(1, (b.units[0].position.0 + 10.0, 0.0), MoveSpeed::Walk));
        let x2 = b.units[0].position.0;
        b.step();
        assert!((b.units[0].position.0 - x2 - 1.0 * tick).abs() < 1e-4);
        // The run ends with the move.
        assert!(b.order_move_speed(1, MoveSpeed::Run));
        for _ in 0..40 {
            b.step();
        }
        assert_eq!(b.units[0].destination, None);
        assert!(!b.units[0].running, "the option went with the move");
        assert!(!b.order_move_speed(99, MoveSpeed::Run));
    }

    #[test]
    fn fire_at_will_toggle() {
        let mut b = battle();
        assert!(b.order_fire_at_will(2, false));
        assert!(!b.units[1].fire_at_will);
    }
}

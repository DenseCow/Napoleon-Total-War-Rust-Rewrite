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
        u.set_destination(None);
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
        u.set_destination((dest != from).then_some(dest));
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

    /// A move to `dest` that ends facing `facing` (radians, the model's facing): the player's
    /// dragged-out formation ([`LandUnit::arrival_facing`], PROVISIONAL). With `width`, the order
    /// carries a new frontage (one unit dragged out alone: `0x0068E610` stores the drag's length
    /// at the order's `+0x1C`, CONFIRMED by debugger), which the unit takes with the order
    /// ([`LandUnit::set_frontage`]). Returns false if `id` is unknown.
    pub fn order_move_formed(&mut self, id: u32, dest: (f32, f32), facing: f32, speed: MoveSpeed, width: Option<f32>) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        // Before the move, whose lay-out re-forms the men (at the order's width, `0x00576B30`).
        if let Some(w) = width {
            self.units[i].set_frontage(w);
        }
        if !self.order_move_at(id, dest, speed) {
            return false;
        }
        self.units[i].arrival_facing = Some(facing);
        true
    }

    /// The player's attack on an enemy unit: a right click on it (`0x005DEAB0` builds
    /// `BCQ_MULTIPLE_SELECTION_ORDER_ATTACK_UNIT`, executor `0x005BFF70` → `0x006D1280`, which runs
    /// `0x0055FBE0` for each selected land unit; CONFIRMED structure, BATTLE_FIDELITY.md §59.6).
    /// The attack kind is the unit's: melee in melee mode ([`LandUnit::melee_mode`], unit
    /// `+0xD98`), else missile. A unit that cannot make that kind of attack does nothing: missile
    /// needs `0x0053EFE0` ([`Battle::can_fire_and_advance`]), melee needs `0x0053EFD0`, which is
    /// "not limbered artillery" ([`Battle::limbered_artillery`]). It does not fall back to a
    /// move. A missile attack is the fire order ([`Battle::order_fire`]); a melee attack chases
    /// the target ([`LandUnit::melee_target`]) and charges it (PROVISIONAL: `charging` set as the
    /// AI's and the scripts' attack sets it; when the original's charge starts is UNKNOWN). The
    /// order keeps the speed of the click (a right click walks, `0x005DEAB0`). Not modelled:
    /// the gates `0x0055CBD0` and `0x0055C780`, and the repeat check on the unit's current order.
    /// Returns whether the unit took the order.
    pub fn order_attack(&mut self, id: u32, target: u32) -> bool {
        let (Some(i), Some(t)) = (self.unit_index(id), self.unit_index(target)) else {
            return false;
        };
        if self.units[i].side == self.units[t].side {
            return false;
        }
        if !self.units[i].melee_mode {
            return self.can_fire_and_advance(i) && self.order_fire(id, target);
        }
        if self.limbered_artillery(i) {
            return false;
        }
        let u = &mut self.units[i];
        u.set_destination(None);
        u.fire_target = None;
        u.melee_target = Some(target);
        u.hold_position = true;
        u.charging = true;
        true
    }

    /// The melee button (`BCQ_MULTIPLE_SELECTION_ORDER_CHANGE_MELEE_MODE`, executor `0x005C0480` →
    /// `0x006D2EB0`, per land unit `0x0055FFF0`; CONFIRMED structure): sets melee mode
    /// ([`LandUnit::melee_mode`], unit `+0xD98`). A unit that cannot fire (`0x0053EFE0`) is refused
    /// leaving melee mode. The exe also raises event 0x2A / 0x2B (not modelled) and skips units
    /// that fail the gate `0x0055C780` (not modelled). INFERRED: which argument is the new value
    /// (read from the decompiler, `0x0055FFF0` writes one and tests the other). Returns whether
    /// the mode was set.
    pub fn order_melee_mode(&mut self, id: u32, on: bool) -> bool {
        let Some(i) = self.unit_index(id) else { return false };
        if !on && !self.can_fire_and_advance(i) {
            return false;
        }
        self.units[i].melee_mode = on;
        true
    }

    /// The enemy unit `idx`'s melee attack chases: while it is still on the field with men, and
    /// until the attacker is in melee or within the stop distance of the advance on an enemy
    /// (`MELEE_CONTACT_RANGE` × `ADVANCE_STOP_FRACTION`, the same PLACEHOLDER stop), where the
    /// melee code takes over.
    pub(super) fn melee_target_to_chase(&self, idx: usize) -> Option<usize> {
        let u = &self.units[idx];
        let t = self.unit_index(u.melee_target?)?;
        let e = &self.units[t];
        let stop = super::model::MELEE_CONTACT_RANGE * super::model::ADVANCE_STOP_FRACTION;
        let (dx, dy) = (e.position.0 - u.position.0, e.position.1 - u.position.1);
        (e.men > 0 && e.active && !e.left_field && !u.in_melee && dx * dx + dy * dy > stop * stop).then_some(t)
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

    fn musket() -> super::super::shooting::MissileWeapon {
        super::super::shooting::MissileWeapon {
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

    /// The state bytes (`0x005357B0`): a unit that cannot fire starts in melee mode, a musket unit
    /// with rounds does not.
    #[test]
    fn units_that_cannot_fire_start_in_melee_mode() {
        let mut b = battle();
        b.units[1].missile = Some(musket());
        b.units[1].ammunition = 5;
        b.reset_state_bytes(0);
        b.reset_state_bytes(1);
        assert!(b.units[0].melee_mode, "no missile weapon");
        assert!(!b.units[1].melee_mode, "a musket with rounds");
    }

    /// `0x0055FBE0`: in melee mode the attack chases the target into melee; the chase follows the
    /// target as it moves, and a move order ends it.
    #[test]
    fn a_melee_attack_chases_the_target_until_another_order() {
        let mut b = battle();
        b.reset_state_bytes(0);
        assert!(b.order_attack(1, 2));
        let u = &b.units[0];
        assert_eq!((u.melee_target, u.fire_target, u.destination), (Some(2), None, None));
        assert!(u.charging);
        assert_eq!(b.movement_goal(0).map(|g| g.0), Some((100.0, 0.0)));
        b.units[1].position = (80.0, 20.0);
        assert_eq!(b.movement_goal(0).map(|g| g.0), Some((80.0, 20.0)), "follows the target");
        b.units[1].position = (5.0, 0.0);
        assert_eq!(b.movement_goal(0), None, "stops at contact, where the melee code takes over");
        assert_eq!(b.units[0].melee_target, Some(2), "the attack stays on");
        assert!(b.order_move(1, (10.0, 0.0)));
        assert_eq!(b.units[0].melee_target, None, "a new order replaces the attack");
        assert!(!b.units[0].charging, "and its charge");
        assert!(!b.order_attack(1, 1), "same side refused");
    }

    /// `0x0055FBE0`: out of melee mode the attack is a fire order; a unit that cannot fire is
    /// refused and does not walk to the target instead.
    #[test]
    fn a_missile_attack_fires_and_a_unit_that_cannot_fire_does_nothing() {
        let mut b = battle();
        b.units[0].missile = Some(musket());
        b.units[0].ammunition = 5;
        b.reset_state_bytes(0);
        assert!(b.order_attack(1, 2));
        assert_eq!((b.units[0].fire_target, b.units[0].melee_target), (Some(2), None));
        let mut b = battle();
        b.units[0].melee_mode = false;
        b.units[0].hold_position = true;
        assert!(!b.order_attack(1, 2), "no weapon to fire");
        let u = &b.units[0];
        assert_eq!((u.fire_target, u.melee_target, u.destination), (None, None, None));
    }

    /// One unit dragged out alone takes the drag's width with its move order (`0x0068E610`): its
    /// files are the line shape's `floor((width + 0.001 − 2r) / spacing) + 1` (`0x00642C90`). Its
    /// drag limits are `round(men × 0.04)` files (× 0.1 mounted) and the line of 2r per man, at
    /// most 80 m (`0x00564530` / `0x005640C0`); artillery keeps its width.
    #[test]
    fn a_dragged_width_reforms_the_unit_within_its_limits() {
        let mut b = battle();
        let u = &mut b.units[0];
        (u.men, u.formation_files, u.formation_width, u.formation_file_spacing, u.man_radius) = (160, 40, 60.0, 1.5, 0.25);
        // Spans (files − 1) × 1.5 + 0.5: 6 files at the narrowest = 8 m; the widest: 2r × 160 =
        // 80 m holds floor((80.001 − 0.5) / 1.5) + 1 = 54 files = 80 m, the cap.
        assert_eq!(u.drag_width_limits(), Some((8.0, 80.0)));
        u.is_cavalry = true;
        assert_eq!(u.drag_width_limits().map(|l| l.0), Some(23.0), "16 files mounted");
        u.is_cavalry = false;
        assert!(b.order_move_formed(1, (0.5, 0.0), 0.0, MoveSpeed::Walk, Some(30.0)));
        // floor((30.001 − 0.5) / 1.5) + 1 = 20 files, a 29 m span; the layout keeps 1.5 m files.
        let u = &mut b.units[0];
        assert_eq!((u.formation_files, u.formation_width, u.formed_width()), (20, 30.0, 29.0));
        assert_eq!(u.set_frontage(80.0), 80.0, "the widest stays within the cap");
        assert_eq!(u.set_frontage(8.0), 8.0, "the narrowest gives its own files back");
        assert_eq!(u.files_for_width(1000.0), 160, "capped at the men");
        u.active_abilities |= 1; // in square
        assert_eq!(u.drag_width_limits(), None, "a square keeps its files");
        u.active_abilities = 0;
        u.unit_category = "artillery".into();
        assert_eq!(u.drag_width_limits(), None);
    }

    /// A unit that turns (to walk off sideways, and to its formation's facing on arrival, with a
    /// new width) keeps every man where he stands: no man moves farther in a tick than a run, and
    /// all end in their new slots (`LandUnit::turn_to`).
    #[test]
    fn no_man_jumps_when_his_unit_turns() {
        let mut b = battle();
        let u = &mut b.units[0];
        (u.men, u.max_men, u.formation_files, u.formation_width, u.formation_file_spacing, u.man_radius) = (60, 60, 20, 20.0, 1.0, 0.3);
        (u.walk_speed, u.run_speed, u.hold_position) = (1.5, 4.0, true);
        b.units[1].hold_position = true;
        for _ in 0..20 {
            b.step();
        }
        // Each man where he stands, by who he is (a re-slot reorders the entries).
        let men = |b: &Battle| {
            let u = &b.units[0];
            let mut by_figure: Vec<(u32, (f32, f32))> = u.men_standing().zip(&u.march.men).map(|(s, m)| (m.figure, s.pos)).collect();
            by_figure.sort_by_key(|f| f.0);
            by_figure.into_iter().map(|f| f.1).collect::<Vec<_>>()
        };
        assert!(b.order_move_formed(1, (2.0, 15.0), 0.0, MoveSpeed::Walk, Some(30.0)));
        let mut last = men(&b);
        let mut worst = 0.0f32;
        for _ in 0..400 {
            b.step();
            let now = men(&b);
            for (a, c) in last.iter().zip(&now) {
                let d = (a.0 - c.0).hypot(a.1 - c.1);
                worst = worst.max(d);
            }
            last = now;
        }
        assert!(worst <= 4.0 * super::super::TICK_SECONDS + 0.05, "a man moved {worst} m in one tick");
        let u = &b.units[0];
        assert_eq!((u.destination, u.facing, u.formation_files), (None, 0.0, 30));
        let off = u.march.men.iter().map(|m| m.side.hypot(m.lag)).fold(0.0f32, f32::max);
        assert!(off < 0.3, "the men stand in their new slots ({off} m off)");
    }

    /// A standing unit turned a little (the HUD's rotate buttons) walks its men to the turned
    /// slots; one turned about (180°) gives each man his nearest slot, so none crosses the block;
    /// a turn before the unit's first tick also leaves every man where he stands.
    #[test]
    fn a_standing_unit_turned_walks_its_men_to_their_nearest_slots() {
        let line = |b: &mut Battle| {
            let u = &mut b.units[0];
            (u.men, u.max_men, u.formation_files, u.formation_width, u.formation_file_spacing, u.man_radius) = (60, 60, 20, 20.0, 1.0, 0.3);
            (u.walk_speed, u.run_speed, u.hold_position) = (1.5, 4.0, true);
            b.units[1].hold_position = true;
        };
        let off = |b: &Battle| b.units[0].march.men.iter().map(|m| m.side.hypot(m.lag)).fold(0.0f32, f32::max);
        // A 15° turn: the men walk to their slots and stand in them.
        let mut b = battle();
        line(&mut b);
        for _ in 0..30 {
            b.step();
        }
        let f = b.units[0].facing;
        b.units[0].turn_to(f + 15f32.to_radians());
        assert!(off(&b) > 0.5 && b.units[0].march.reforming, "the turn left the men to walk");
        for _ in 0..100 {
            b.step();
        }
        assert!(off(&b) < 0.3, "in the turned slots ({} m off)", off(&b));
        // About turn: the farthest man has about a rank's depth to go, not the line's width.
        let f = b.units[0].facing;
        b.units[0].turn_to(f + std::f32::consts::PI);
        assert!(off(&b) < 4.0, "nearest slots ({} m off)", off(&b));
        // Before the first tick: every man is counted and stays where he stands.
        let mut b = battle();
        line(&mut b);
        let before: Vec<(f32, f32)> = { b.units[0].march.ensure_men(60); b.units[0].men_standing().map(|m| m.pos).collect() };
        b.units[0].march.men.clear();
        b.units[0].turn_to(1.0);
        let mut after: Vec<(u32, (f32, f32))> = b.units[0].men_standing().zip(&b.units[0].march.men).map(|(s, m)| (m.figure, s.pos)).collect();
        after.sort_by_key(|a| a.0);
        let mut moved = 0.0f32;
        for (a, (_, c)) in before.iter().zip(&after) {
            moved = moved.max((a.0 - c.0).hypot(a.1 - c.1));
        }
        assert!(moved < 1e-3, "a man moved {moved} m at the turn");
    }

    /// A formed move walks there facing its way, then turns to the formation's facing; a plain
    /// move keeps the walking facing.
    #[test]
    fn a_formed_move_turns_to_its_facing_on_arrival() {
        let mut b = battle();
        b.units[0].hold_position = true;
        b.units[1].hold_position = true;
        assert!(b.order_move_formed(1, (0.5, 0.0), 1.25, MoveSpeed::Walk, None));
        for _ in 0..50 {
            b.step();
        }
        let u = &b.units[0];
        assert_eq!((u.destination, u.arrival_facing), (None, None));
        assert!((u.facing - 1.25).abs() < 1e-6, "facing {}", u.facing);
        assert!(b.order_move(1, (0.5, 1.0)));
        for _ in 0..50 {
            b.step();
        }
        assert!((b.units[0].facing - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "walked north: {}", b.units[0].facing);
    }

    /// `0x0055FFF0`: melee mode can always be switched on; a unit that cannot fire stays in it.
    #[test]
    fn only_a_unit_that_can_fire_leaves_melee_mode() {
        let mut b = battle();
        b.units[0].missile = Some(musket());
        b.units[0].ammunition = 5;
        assert!(b.order_melee_mode(1, true));
        assert!(b.units[0].melee_mode);
        assert!(b.order_melee_mode(1, false));
        assert!(!b.units[0].melee_mode);
        b.units[1].melee_mode = true;
        assert!(!b.order_melee_mode(2, false), "no weapon: stays in melee mode");
        assert!(b.units[1].melee_mode);
        assert!(!b.order_melee_mode(99, true));
    }
}

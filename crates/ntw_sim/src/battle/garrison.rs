//! Building garrisons: units occupying defendable buildings (`defend_building`).
//!
//! CONFIRMED:
//! - the defendable rule of the script binding `0x00645350`: a building is refused ("specified
//!   building is not defendable") when its `+0x1E8` is 0 or its byte `+0x231` is set;
//! - `+0x231` marks wall buildings: a unit in such a building is "on walls" (`0x0056AE70`: unit
//!   `+0x11C` → building `+0x14` → `+0x231`), which is what the missile range and accuracy wall
//!   modifiers read;
//! - `+0x231` (with `+0x230`) is set by the building setup `0x00688DD0` for `battlefield_buildings` category 8,
//!   `fort` (category enum `0x00E4E9A0`): fort pieces are walls;
//! - the building's garrison capacity (`capacity()`, `0x008554F0`) is 0 unless `+0x1E8` is set, else the
//!   number of usable soldier slots (`0x006F22C0`: the garrison object's lines, 0x58 bytes each, count the
//!   slots, 0x30 bytes each, whose vfunc answers true; `0x006DB8E0`), capped by the building type's
//!   `+0x54 → +0x6C` (its source table was not found). Not enforced in the model (PROVISIONAL).
//!
//! CONFIRMED: `+0x1E8` is the garrison object `0x006FA380` builds (`0x0068CE40`) from the building
//! model's `models_building` entries (0x34 bytes each, the `EFLine` fire lines, see
//! `ntw_formats::models_building`), one garrison line per entry with soldier slots along it
//! (`0x006AA630`). Buildings without fire lines have none: Lodi's town hall and farmhouse have them,
//! plain houses do not.
//! PROVISIONAL (the model is unit-level):
//! - one unit per building; it enters when it comes within [`GARRISON_ENTER_M`] of the building;
//! - it fires from the building with at most one man per intact fire line;
//! - it is in cover against every shot ([`crate::battle::missile::chance_to_hit`]'s −0.2);
//! - any move order, or routing, takes it out.

use super::model::Battle;
use super::orders::MoveSpeed;

/// PROVISIONAL distance from a building's centre at which a unit ordered in enters it.
pub const GARRISON_ENTER_M: f32 = 15.0;

/// A building on the battlefield, as the garrison code sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleBuilding {
    /// Building key (`battlefield_buildings` / `models_building`).
    pub key: String,
    /// Centre on the map (metres).
    pub position: (f32, f32),
    /// Soldier slots on the intact fire lines (`models_building`, see the setup); 0 = the building
    /// cannot be garrisoned.
    pub fire_lines: u32,
    /// A wall building (`+0x231`, category `fort`): not defendable; its occupants are "on walls".
    pub wall: bool,
    /// The unit inside, if any.
    pub occupant: Option<u32>,
}

impl BattleBuilding {
    /// The defendable rule of `0x00645350` (CONFIRMED shape; fire lines stand for `+0x1E8`).
    pub fn defendable(&self) -> bool {
        self.fire_lines > 0 && !self.wall
    }
}

impl Battle {
    /// `defend_building(building, run)`: the unit goes to building `index` (into
    /// [`Battle::buildings`]) and garrisons it on arrival. Errors as the binding: "specified
    /// building is not defendable" (CONFIRMED message); a building another unit holds is refused
    /// too (PROVISIONAL: one unit per building).
    pub fn order_defend_building(&mut self, id: u32, index: usize, run: bool) -> Result<(), String> {
        let i = self.unit_index(id).ok_or("unknown unit")?;
        let b = self.buildings.get(index).ok_or("expecting building argument")?;
        if !b.defendable() {
            return Err("specified building is not defendable".into());
        }
        if b.occupant.is_some_and(|o| o != id) {
            return Err("building is occupied".into());
        }
        let pos = b.position;
        self.leave_building(i);
        self.order_move_at(id, pos, if run { MoveSpeed::Run } else { MoveSpeed::Walk });
        let u = &mut self.units[i];
        u.hold_position = true;
        u.garrison_target = Some(index);
        Ok(())
    }

    /// Takes unit `idx` out of its building, if it is in one.
    pub fn leave_building(&mut self, idx: usize) {
        if let Some(b) = self.units[idx].garrison.take()
            && let Some(b) = self.buildings.get_mut(b)
        {
            b.occupant = None;
        }
    }

    /// The garrison step of unit `idx`: a unit ordered into a building enters it on arrival; a
    /// routing unit, or one given another move, leaves it.
    pub(super) fn garrison_step(&mut self, idx: usize) {
        let u = &self.units[idx];
        if u.garrison.is_some() && (u.morale.is_routing_or_shattered() || u.destination.is_some() || u.men == 0) {
            self.leave_building(idx);
        }
        let u = &self.units[idx];
        let Some(target) = u.garrison_target else { return };
        if u.morale.is_routing_or_shattered() || u.men == 0 {
            self.units[idx].garrison_target = None;
            return;
        }
        let Some(b) = self.buildings.get(target) else {
            self.units[idx].garrison_target = None;
            return;
        };
        let d2 = (b.position.0 - u.position.0).powi(2) + (b.position.1 - u.position.1).powi(2);
        if d2 > GARRISON_ENTER_M * GARRISON_ENTER_M {
            return;
        }
        if b.occupant.is_some_and(|o| o != u.id) || !b.defendable() {
            self.units[idx].garrison_target = None;
            return;
        }
        let (id, pos) = (u.id, b.position);
        self.buildings[target].occupant = Some(id);
        let u = &mut self.units[idx];
        u.garrison = Some(target);
        u.garrison_target = None;
        u.position = pos;
        u.destination = None;
        u.running = false;
        u.hold_position = true;
    }

    /// The men of unit `idx` who can fire: everyone, or in a building at most one per fire line
    /// (PROVISIONAL).
    pub fn shooters(&self, idx: usize) -> u32 {
        let u = &self.units[idx];
        match u.garrison.and_then(|b| self.buildings.get(b)) {
            Some(b) => u.men.min(b.fire_lines),
            None => u.men,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::fatigue::KvFatigue;
    use crate::battle::model::LandUnit;
    use crate::battle::morale::KvMorale;

    fn building(key: &str, pos: (f32, f32), lines: u32) -> BattleBuilding {
        BattleBuilding { key: key.into(), position: pos, fire_lines: lines, wall: false, occupant: None }
    }

    #[test]
    fn a_unit_garrisons_a_defendable_building() {
        let mut b = Battle::new(1, KvMorale::default(), KvFatigue::default());
        let mut u = LandUnit::new(1, 0, 100, (0.0, 0.0));
        u.walk_speed = 2.0;
        u.run_speed = 4.0;
        b.add_unit(u);
        b.buildings = vec![building("south_euro_house02", (50.0, 0.0), 0), building("south_euro_farmhouse", (50.0, 0.0), 39)];
        assert_eq!(b.order_defend_building(1, 0, true), Err("specified building is not defendable".into()));
        b.buildings[1].wall = true;
        assert!(b.order_defend_building(1, 1, true).is_err(), "walls are not defendable");
        b.buildings[1].wall = false;
        b.order_defend_building(1, 1, true).unwrap();
        for _ in 0..200 {
            b.step();
        }
        assert_eq!(b.units[0].garrison, Some(1));
        assert_eq!(b.buildings[1].occupant, Some(1));
        assert_eq!(b.units[0].position, (50.0, 0.0));
        assert_eq!(b.shooters(0), 39);
        // A move order takes it out.
        b.order_move(1, (0.0, 0.0));
        b.step();
        assert_eq!(b.units[0].garrison, None);
        assert_eq!(b.buildings[1].occupant, None);
    }
}

//! Ship repair in port (CAMPAIGN_FIDELITY.md §Ship repair; CONFIRMED from the exe unless tagged).
//!
//! A ship of a navy in a port that can build ships may be ordered to repair: the order charges its repair cost
//! at once ([`super::treasury::pay`], category 2) and sets the ship's flag (`NAVAL_UNIT` #3, unit +0x168,
//! [`ShipState::repairing`](super::naval::ShipState::repairing)); at its faction's turn end the ship is made whole. Leaving the port, a unit
//! transfer that leaves a navy outside a port, or a battle cancels the order and refunds the cost
//! ([`super::treasury::refund`]).
//!
//! - Commands: `CCQ_REPAIR_NAVAL_UNIT` (registered by `0x00422380`, executor `0x00937040`: order on) and
//!   `CCQ_TOGGLE_REPAIR_NAVAL_UNIT` (`0x00423300`, executor `0x009387A0`: on ↔ off). The executors test only
//!   that the unit is a ship; their issuers (the panel `0x008DF820` and the AI `0x00AA90F0`) test
//!   [`CampaignModel::can_repair_ship`] first, and so do ours, so a repeated order is not charged twice.
//! - The order `0x008EEB70` ([`CampaignModel::set_ship_repair`]); its cost `0x008EA2F0`
//!   ([`CampaignModel::ship_repair_cost`]); the cancel-all `0x008B36D0` ([`CampaignModel::cancel_ship_repairs`]).
//! - The turn end: the navy's turn-end slot (`0x008BD430`, called per force by `0x008BD0F0`) repairs every
//!   flagged ship (`0x008EA2A0`) before its units' `UnitTurnEnd` ([`CampaignModel::repair_ships`]).
//!
//! - The undock `0x00A8C0B0` ([`CampaignModel::undock`]): every move of a navy goes through it
//!   ([`CampaignModel::move_character`]), and so does the end of a battle ([`CampaignModel::undock_after_battle`]).
//!
//! PROVISIONAL: the model has no docking link (the exe's navy +0xE0, set and cleared by the dock and undock
//! paths, `0x00A8C0B0`); a navy counts as docked in the port it stands at ([`CampaignModel::docked_port`]).
//! The undock the model has no counterpart for (a war declaration putting a navy in an enemy port out,
//! `0x008DA179`) is a BACKLOG line.

use super::commands::{fistp, CommandError};
use super::events::CampaignEvent;
use super::ids::{ForceId, RegionId};
use super::world::{CampaignModel, CampaignUnit};
use crate::fixed::Fixed20;

/// How near a port slot a navy or naval character stands to be in that port (squared map units). PROVISIONAL:
/// the exe links a docked navy to its port (navy +0xE0) instead of measuring.
const IN_PORT_DIST2: f32 = 1.0;

impl CampaignModel {
    /// The port slot at a map position, as (region, slot index): the first port slot (by region id, then slot
    /// order) within 1 map unit. PROVISIONAL (see [`IN_PORT_DIST2`]).
    pub fn port_at(&self, pos: (Fixed20, Fixed20)) -> Option<(RegionId, usize)> {
        let (x, z) = (pos.0.to_f32(), pos.1.to_f32());
        self.world.regions.values().find_map(|r| {
            r.slots
                .iter()
                .position(|s| {
                    s.port
                        && s.position.is_some_and(|p| {
                            let (dx, dz) = (p.0.to_f32() - x, p.1.to_f32() - z);
                            dx * dx + dz * dz < IN_PORT_DIST2
                        })
                })
                .map(|i| (r.id, i))
        })
    }

    /// The port a navy is docked in (the exe's navy +0xE0): PROVISIONAL, the port it stands at ([`Self::port_at`]).
    /// `None` for an army.
    pub fn docked_port(&self, force: ForceId) -> Option<(RegionId, usize)> {
        self.docked_port_at(force, self.force_position(force)?)
    }

    /// The port the navy `force` would be docked in standing at `pos` ([`Self::docked_port`] for a force whose
    /// commander has left the map already). `None` for an army.
    pub(crate) fn docked_port_at(&self, force: ForceId, pos: (Fixed20, Fixed20)) -> Option<(RegionId, usize)> {
        self.world.forces.get(&force).filter(|f| f.is_navy)?;
        self.port_at(pos)
    }

    /// A docked navy leaves the port slot `slot` of `region` (the undock `0x00A8C0B0` or the navy's destructor
    /// `0x0088EF90`): the slot goes back to the region's owner (`RestorePortSlotHolderToRegionOwner` `0x00A8D830`,
    /// CONFIRMED polish21: `SetSettlementOwner(slot, region owner +0xF4)`) unless the slot's vfunc +0x1C test is
    /// nonzero. That test is the slot's garrison handle (+0xC0), which the model does not hold, so the slot always
    /// passes back here, as in an owner change's slot pass ([`Self::pass_region_slots`]). (`None` = the owner's.)
    /// Then the trade price update with flag 0 runs (CONFIRMED, priceupdate: `SetSettlementOwner` `0x00B2B810`,
    /// called at `0x00A8D859`, runs `0x00BCB020(0)` at `0x00B2B84F` for a slot whose +0x4C is the slot itself,
    /// whose +0x20C is set and whose type is a port: the tests under which the slot got its port object, so every
    /// port slot. It has no same-holder test, so a slot that was already the owner's updates too.)
    pub(crate) fn restore_port_holder(&mut self, region: RegionId, slot: usize) {
        let Some(s) = self.world.regions.get_mut(&region).and_then(|r| r.slots.get_mut(slot)) else { return };
        s.holder = None;
        if s.port {
            self.update_commodity_prices_with(false);
        }
        // Then `SetSettlementOwner` rebuilds the region's demand (`0x00AB49F0(0)` at `0x00B2B85F`) and refreshes
        // its town wealth (`0x00AB4410(0)` at `0x00B2B86F`, CONFIRMED).
        self.refresh_region_demand(region);
        self.refresh_region_town_wealth(region);
    }

    /// Can the port slot repair ships (`0x00A894D0` → the port manager's slot +0x18, `0x00B44E50`): a building
    /// stands in it and its recruitable list (`0x00B43CA0`, [`Self::building_units`]) is not empty; flagged
    /// entries count (a damaged building passes here).
    pub fn port_can_repair(&self, region: RegionId, slot: usize) -> bool {
        let Some(r) = self.world.regions.get(&region) else { return false };
        r.slots.get(slot).and_then(|s| s.building.as_ref()).is_some_and(|b| self.building_units(r, b).next().is_some())
    }

    /// A ship's repair cost (`0x008EA2F0`): its `units` #7 price (record +0x38, [`super::rules::UnitRules::campaign_cost`])
    /// × its repair share ([`ShipState::repair_fraction`](super::naval::ShipState::repair_fraction)) clamped to 0..0.5 (`MINSS`, so a NaN share gives 0.5),
    /// multiplied in f32 and rounded with the x87 FISTP (half to even).
    pub fn ship_repair_cost(&self, unit: &CampaignUnit) -> i32 {
        let price = self.rules.units.get(&unit.unit_key).map_or(0, |u| u.campaign_cost);
        let f = self.ship_state(unit).repair_fraction();
        let share = if 0.0 > f {
            0.0
        } else if f < 0.5 {
            f
        } else {
            0.5
        };
        fistp(price as f32 * share)
    }

    /// The ship's faction's treasury (the unit's force's faction, the exe's unit +0x34 → navy → faction).
    fn force_treasury(&self, force: ForceId) -> i32 {
        self.world.forces.get(&force).and_then(|f| self.world.factions.get(&f.faction)).map_or(0, |f| f.treasury)
    }

    /// The order's own tests (`0x008EEB70`, order on): the ship needs a repair (`0x0055ACC0`), its cost is below
    /// its faction's treasury (signed, strict), its force is a navy docked in a port (navy +0xE0) that can repair
    /// ([`Self::port_can_repair`]). Returns the cost and the port.
    fn repair_order_ok(&self, force: ForceId, unit: usize) -> Option<(i32, (RegionId, usize))> {
        let u = self.world.forces.get(&force)?.units.get(unit)?;
        if !self.ship_state(u).needs_repair() {
            return None;
        }
        let cost = self.ship_repair_cost(u);
        if cost >= self.force_treasury(force) {
            return None;
        }
        let port = self.docked_port(force)?;
        self.port_can_repair(port.0, port.1).then_some((cost, port))
    }

    /// May the ship be ordered to repair (`0x008B3120`, the panel's and the AI's test): not already ordered,
    /// in a force (unit +0x11C, every unit of the model), the order's own tests ([`Self::set_ship_repair`]), and
    /// the port slot's building at full health (> 99).
    pub fn can_repair_ship(&self, force: ForceId, unit: usize) -> bool {
        let Some(u) = self.world.forces.get(&force).and_then(|f| f.units.get(unit)) else { return false };
        if self.ship_state(u).repairing {
            return false;
        }
        let Some((_, (region, slot))) = self.repair_order_ok(force, unit) else { return false };
        self.world.regions[&region].slots[slot].building.as_ref().is_some_and(|b| !b.is_damaged())
    }

    /// The repair order (`0x008EEB70`). On: when the order's tests pass ([`Self::can_repair_ship`] less the flag
    /// and building-health tests), the cost is charged to the ship's faction (`0x00BAF500`, category 2) and the
    /// flag set; the flag is not tested, so a second order charges again (the issuers test it first). Off: the
    /// cost is computed again from the ship's state now (`0x008EEC4E`; the exe keeps no paid cost, only the flag),
    /// refunded (`0x00BAF660`, category 2) when the flag was set and the cost is above 0, and the flag cleared.
    /// Returns whether the ship is now ordered to repair.
    pub(crate) fn set_ship_repair(&mut self, force: ForceId, unit: usize, on: bool) -> bool {
        let Some(u) = self.world.forces.get(&force).and_then(|f| f.units.get(unit)) else { return false };
        let (id, mut state) = (u.id, self.ship_state(u));
        let Some(faction) = self.world.forces.get(&force).map(|f| f.faction) else { return false };
        if on {
            let Some((cost, _)) = self.repair_order_ok(force, unit) else { return state.repairing };
            if let Some(f) = self.world.factions.get_mut(&faction) {
                super::treasury::pay(&mut f.treasury, cost);
            }
            state.repairing = true;
        } else {
            if !state.repairing {
                return false;
            }
            let cost = self.ship_repair_cost(&self.world.forces[&force].units[unit]);
            if cost > 0
                && let Some(f) = self.world.factions.get_mut(&faction)
            {
                super::treasury::refund(&mut f.treasury, cost);
            }
            state.repairing = false;
        }
        self.world.ship_states.insert(id, state);
        on
    }

    /// Cancels every repair order of a navy, refunding each (`0x008B36D0`: the order off for every flagged ship).
    /// The exe calls it when a navy leaves its port (the undock `0x00A8C0B0`, [`Self::undock`]) and, after a unit
    /// transfer (`0x008D5F10`, `0x008D47B0`, `0x008F04C0`), for each of the two forces that is a navy outside a port
    /// ([`Self::cancel_undocked_repairs`]).
    pub(crate) fn cancel_ship_repairs(&mut self, force: ForceId) {
        let Some(f) = self.world.forces.get(&force) else { return };
        let flagged: Vec<usize> = f.units.iter().enumerate().filter(|(_, u)| self.world.ship_states.get(&u.id).is_some_and(|s| s.repairing)).map(|(i, _)| i).collect();
        for i in flagged {
            self.set_ship_repair(force, i, false);
        }
    }

    /// The undock (`0x00A8C0B0`): a navy docked in a port leaves it and its repair orders are cancelled and refunded
    /// ([`Self::cancel_ship_repairs`]). The exe's undock is a method of the port and acts only when the character it
    /// is given is the navy docked there (`0x00A84990`, the port's docked-navy handle), so a navy at sea is left
    /// alone. Its callers: every move of a navy ([`Self::move_character`]) and the post-battle placement
    /// ([`Self::undock_after_battle`]). It then clears the port's handle (`0x008B02E0`; the model links a navy to
    /// its port by position) and hands the port slot back to the region's owner ([`Self::restore_port_holder`]).
    /// Its three notifications are not modelled (BACKLOG).
    pub(crate) fn undock(&mut self, force: ForceId) {
        if let Some((region, slot)) = self.docked_port(force) {
            self.cancel_ship_repairs(force);
            self.restore_port_holder(region, slot);
        }
    }

    /// After a unit transfer: a navy outside a port loses its repair orders ([`Self::cancel_ship_repairs`]).
    pub(crate) fn cancel_undocked_repairs(&mut self, force: ForceId) {
        if self.world.forces.get(&force).is_some_and(|f| f.is_navy) && self.docked_port(force).is_none() {
            self.cancel_ship_repairs(force);
        }
    }

    /// The navy's turn end, first part (`0x008BD430`): every ship ordered to repair is repaired (`0x008EA2A0`):
    /// its state made whole ([`ShipState::repair`](super::naval::ShipState::repair)), its men set to full (unit +0x4C = +0x50) and its flag cleared.
    /// The flag alone decides: the port is not tested again (leaving it cancels the order).
    pub(crate) fn repair_ships(&mut self, force: ForceId) {
        let Some(f) = self.world.forces.get_mut(&force) else { return };
        for u in &mut f.units {
            let Some(s) = self.world.ship_states.get_mut(&u.id).filter(|s| s.repairing) else { continue };
            s.repair();
            s.repairing = false;
            u.men = u.max_men;
        }
    }

    /// The AI's ship repair: the action of its `CAI_BDIM_REPAIR_NAVY` intention (slot 12 `0x00CD24A0` → `0x00AA90F0`,
    /// CONFIRMED). Nothing when the AI may not shop at the navy's position ([`Self::ai_shopping_hidden`],
    /// `0x00A8B2D0`; a navy without a commander has no position in the model and is not held back); else every ship that needs a repair (`0x008CE990` → `0x0055ACC0`) and may be ordered to
    /// ([`Self::can_repair_ship`], `0x008B3120`) is ordered ([`Self::set_ship_repair`], `0x008EEB70`: charged at once,
    /// so a later ship of the navy sees the treasury the earlier ones left). Straight calls: no command is queued.
    /// INFERRED: the exe walks the CAI force's unit list (+0x164), taken as the force's unit order.
    /// The goal that creates the intention (`0x00CDD540`) is not modelled: our AI has no naval goals (BACKLOG).
    pub fn ai_repair_navy(&mut self, force: ForceId) {
        if self.force_position(force).is_some_and(|p| self.ai_shopping_hidden(p)) {
            return;
        }
        let n = self.world.forces.get(&force).map_or(0, |f| f.units.len());
        for i in 0..n {
            let u = &self.world.forces[&force].units[i];
            if self.ship_state(u).needs_repair() && self.can_repair_ship(force, i) {
                self.set_ship_repair(force, i, true);
            }
        }
    }

    /// `CCQ_REPAIR_NAVAL_UNIT` (`0x00937040`): order the ship to repair. Refused unless
    /// [`Self::can_repair_ship`] (the issuers' test).
    pub fn order_ship_repair(&mut self, force: ForceId, unit: usize) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_ship(force, unit)?;
        if !self.can_repair_ship(force, unit) {
            return Err(CommandError::Unsupported("the ship cannot be repaired here"));
        }
        self.set_ship_repair(force, unit, true);
        Ok(Vec::new())
    }

    /// `CCQ_TOGGLE_REPAIR_NAVAL_UNIT` (`0x009387A0`): cancel the ship's repair order, or order it as
    /// [`Self::order_ship_repair`] does.
    pub fn toggle_ship_repair(&mut self, force: ForceId, unit: usize) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_ship(force, unit)?;
        let u = &self.world.forces[&force].units[unit];
        if self.ship_state(u).repairing {
            self.set_ship_repair(force, unit, false);
            return Ok(Vec::new());
        }
        self.order_ship_repair(force, unit)
    }

    /// The executors' test: a ship of a navy whose faction's turn it is.
    fn check_ship(&self, force: ForceId, unit: usize) -> Result<(), CommandError> {
        let f = self.world.forces.get(&force).ok_or(CommandError::UnknownForce(force))?;
        self.check_turn(f.faction)?;
        let u = f.units.get(unit).ok_or(CommandError::Unsupported("no such unit in the force"))?;
        if !f.is_navy || !self.rules.units.get(&u.unit_key).is_some_and(|r| r.is_naval) {
            return Err(CommandError::Unsupported("not a ship"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::naval::ShipState;

    fn ship(crews: [i32; 3], max: [i32; 3], damage: [f32; 5]) -> ShipState {
        ShipState { damage, crews, max_crews: max, guns: 10, max_guns: 74, ..Default::default() }
    }

    #[test]
    fn needs_repair_tests_crews_then_four_damages() {
        assert!(!ship([10, 10, 10], [10, 10, 10], [0.0; 5]).needs_repair());
        assert!(ship([10, 9, 10], [10, 10, 10], [0.0; 5]).needs_repair());
        assert!(ship([10, 10, 10], [10, 10, 10], [0.0, 0.0, 0.0, 0.1, 0.0]).needs_repair());
        // The fifth damage is not tested (`0x0055ACC0`).
        assert!(!ship([10, 10, 10], [10, 10, 10], [0.0, 0.0, 0.0, 0.0, 1.0]).needs_repair());
    }

    #[test]
    fn repair_fraction_divides_crews_in_integers() {
        // Any man left: the crew term is 0 (IDIV), only a quarter of the damages counts.
        let s = ship([1, 0, 0], [10, 10, 10], [0.2, 0.2, 0.2, 0.2, 0.0]);
        assert!((s.repair_fraction() - 0.2).abs() < 1e-6);
        // No man left: 1.
        assert_eq!(ship([0, 0, 0], [10, 10, 10], [0.0; 5]).repair_fraction(), 1.0);
        // More men than the full crew: −1 IDIV 30 truncates toward 0 (`0x008F65F0`), not down to −1.
        assert_eq!(ship([11, 10, 10], [10, 10, 10], [0.0; 5]).repair_fraction(), 0.0);
        // Twice the full crew: −30 IDIV 30 = −1 (the cost clamps the negative total to 0).
        assert_eq!(ship([20, 20, 20], [10, 10, 10], [0.0; 5]).repair_fraction(), -1.0);
        // ORIGINAL BUG guard: no full crew does not divide by zero.
        assert_eq!(ship([0, 0, 0], [0, 0, 0], [0.4, 0.0, 0.0, 0.0, 0.0]).repair_fraction(), 0.1);
    }

    #[test]
    fn repair_makes_the_ship_whole() {
        let mut s = ship([1, 2, 3], [10, 10, 10], [0.5; 5]);
        s.sunk = true;
        s.repair();
        assert_eq!(s, ShipState { crews: [10; 3], max_crews: [10; 3], guns: 74, max_guns: 74, ..Default::default() });
    }

    use super::super::commands::CampaignCommand;
    use super::super::ids::{CharacterId, FactionId, UnitId};
    use super::super::world::{Character, CharacterKind, MilitaryForce};
    use std::sync::Arc;

    const NAVY: ForceId = ForceId(1005);
    const PORT: (i32, i32) = (5, 5);

    fn at(p: (i32, i32)) -> (Fixed20, Fixed20) {
        (Fixed20::from_int(p.0), Fixed20::from_int(p.1))
    }

    /// The test world with faction A's navy (one `test_ship`, `units` #7 = 1000) docked at region 10's slot 0 (a port
    /// holding `test_building_level`, which recruits), the ship damaged to a repair share of 0.4: a cost of 400.
    fn docked() -> (CampaignModel, FactionId) {
        let mut m = super::super::tests::test_model();
        let a = m.world.regions[&RegionId(10)].owner;
        {
            let rules = Arc::make_mut(&mut m.rules);
            let mut ship = rules.units["test_unit"].clone();
            (ship.is_naval, ship.campaign_cost) = (true, 1000);
            rules.units.insert("test_ship".into(), ship);
            rules.allow_unit_for_all("test_ship");
        }
        let slot = &mut m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0];
        (slot.port, slot.position) = (true, Some(at(PORT)));
        let admiral = Character {
            id: CharacterId(109),
            faction: a,
            kind: CharacterKind::Admiral,
            position: at(PORT),
            movement_points: 20,
            max_movement_points: 20,
            base_movement_points: 20,
            garrisoned_in: None,
        };
        m.world.characters.insert(admiral.id, admiral);
        let unit = CampaignUnit { id: UnitId(50), unit_key: "test_ship".into(), men: 30, max_men: 300, character: None, officer_name: Default::default() };
        m.world.forces.insert(NAVY, MilitaryForce { id: NAVY, faction: a, commander: Some(CharacterId(109)), units: vec![unit], is_navy: true });
        m.world.ship_states.insert(UnitId(50), ship([100; 3], [100; 3], [0.4, 0.4, 0.4, 0.4, 0.0]));
        m.turn.humans = vec![a];
        m.start_campaign();
        (m, a)
    }

    fn treasury(m: &CampaignModel, f: FactionId) -> i32 {
        m.world.factions[&f].treasury
    }

    fn flagged(m: &CampaignModel) -> bool {
        m.world.ship_states[&UnitId(50)].repairing
    }

    #[test]
    fn the_cost_is_the_price_times_the_share_clamped_to_a_half() {
        let (mut m, _) = docked();
        let u = m.world.forces[&NAVY].units[0].clone();
        assert_eq!(m.ship_repair_cost(&u), 400);
        // No man left: a share of 1, clamped to 0.5.
        m.world.ship_states.get_mut(&UnitId(50)).unwrap().crews = [0; 3];
        assert_eq!(m.ship_repair_cost(&u), 500);
        // A share of 0.00125 × 1000 = 1.25 rounds to 1; 0.0025 × 1000 = 2.5 rounds half to even, to 2.
        m.world.ship_states.insert(UnitId(50), ship([100; 3], [100; 3], [0.005, 0.0, 0.0, 0.0, 0.0]));
        assert_eq!(m.ship_repair_cost(&u), 1);
        m.world.ship_states.insert(UnitId(50), ship([100; 3], [100; 3], [0.01, 0.0, 0.0, 0.0, 0.0]));
        assert_eq!(m.ship_repair_cost(&u), 2);
    }

    #[test]
    fn ordering_charges_once_and_cancelling_refunds_the_cost_now() {
        let (mut m, a) = docked();
        let before = treasury(&m, a);
        assert!(m.can_repair_ship(NAVY, 0));
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        assert_eq!(treasury(&m, a), before - 400);
        assert!(flagged(&m));
        // Already ordered: refused, not charged again.
        assert!(m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).is_err());
        assert_eq!(treasury(&m, a), before - 400);
        // The cost is computed again when cancelling (`0x008EEB70` off): more damage, a larger refund.
        m.world.ship_states.get_mut(&UnitId(50)).unwrap().damage = [0.48, 0.48, 0.48, 0.48, 0.0];
        m.apply(CampaignCommand::ToggleShipRepair { force: NAVY, unit: 0 }).unwrap();
        assert_eq!(treasury(&m, a), before - 400 + 480);
        assert!(!flagged(&m));
        // The toggle orders it again.
        m.apply(CampaignCommand::ToggleShipRepair { force: NAVY, unit: 0 }).unwrap();
        assert!(flagged(&m));
    }

    #[test]
    fn a_cancel_refunds_nothing_when_the_cost_now_is_zero() {
        // `0x008EEB70` off refunds the cost recomputed at cancel time (`0x008EEC4E` → `0x00BAF660` at
        // `0x008EEC7A`), only when it is above 0; no paid cost is kept.
        let (mut m, a) = docked();
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        let paid = treasury(&m, a);
        m.world.ship_states.get_mut(&UnitId(50)).unwrap().damage = [0.0; 5];
        m.apply(CampaignCommand::ToggleShipRepair { force: NAVY, unit: 0 }).unwrap();
        assert!(!flagged(&m));
        assert_eq!(treasury(&m, a), paid);
    }

    #[test]
    fn the_order_needs_a_damaged_ship_docked_at_a_whole_recruiting_port_and_more_money_than_the_cost() {
        let (mut m, a) = docked();
        let try_order = |m: &mut CampaignModel| m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).is_ok();
        // Too dear: the treasury must be above the cost (strict).
        let saved = treasury(&m, a);
        m.world.factions.get_mut(&a).unwrap().treasury = 400;
        assert!(!try_order(&mut m));
        m.world.factions.get_mut(&a).unwrap().treasury = saved;
        // A damaged building: the panel's test refuses, the order itself does not test it.
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building.as_mut().unwrap().health = 99;
        assert!(!m.can_repair_ship(NAVY, 0));
        assert!(m.set_ship_repair(NAVY, 0, true));
        m.set_ship_repair(NAVY, 0, false);
        // A building that recruits nothing cannot repair.
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building = Some(super::super::world::BuildingRef { level_key: "no_such_level".into(), health: 100 });
        assert!(!m.can_repair_ship(NAVY, 0));
        m.world.regions.get_mut(&RegionId(10)).unwrap().slots[0].building = Some(super::super::world::BuildingRef { level_key: "test_building_level".into(), health: 100 });
        // Not docked.
        m.world.characters.get_mut(&CharacterId(109)).unwrap().position = at((40, 40));
        assert!(!try_order(&mut m));
        m.world.characters.get_mut(&CharacterId(109)).unwrap().position = at(PORT);
        // A whole ship needs no repair.
        m.world.ship_states.insert(UnitId(50), ship([100; 3], [100; 3], [0.0; 5]));
        assert!(!try_order(&mut m));
        assert_eq!(treasury(&m, a), saved);
    }

    #[test]
    fn the_turn_end_repairs_the_ordered_ship() {
        let (mut m, _) = docked();
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        m.apply(CampaignCommand::EndTurn).unwrap();
        let s = m.world.ship_states[&UnitId(50)];
        assert!(!s.repairing && !s.needs_repair() && s.guns == s.max_guns);
        assert_eq!(m.world.forces[&NAVY].units[0].men, 300);
    }

    #[test]
    fn a_move_cancels_the_order_and_refunds_it() {
        let (mut m, a) = docked();
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        let paid = treasury(&m, a);
        m.apply(CampaignCommand::MoveForce { force: NAVY, to: at((8, 5)) }).unwrap();
        assert!(!flagged(&m));
        assert_eq!(treasury(&m, a), paid + 400);
    }

    #[test]
    fn a_transfer_out_of_port_cancels_the_order_and_refunds_it() {
        let (mut m, a) = docked();
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        let paid = treasury(&m, a);
        let mut sea = m.world.characters[&CharacterId(109)].clone();
        (sea.id, sea.position) = (CharacterId(110), at((40, 40)));
        m.world.characters.insert(sea.id, sea);
        let other = ForceId(1006);
        m.world.forces.insert(other, MilitaryForce { id: other, faction: a, commander: Some(CharacterId(110)), units: Vec::new(), is_navy: true });
        let mut events = Vec::new();
        m.merge_units(NAVY, other, &mut events);
        assert_eq!(m.world.forces[&other].units[0].id, UnitId(50));
        assert!(!flagged(&m));
        assert_eq!(treasury(&m, a), paid + 400);
    }

    /// A docked navy carrying an army, ordered to land it elsewhere, sails out through the same undock as any move
    /// (`0x00A8C0B0`): the order is cancelled and refunded, so the turn end repairs nothing at sea.
    #[test]
    fn sailing_out_to_land_an_army_cancels_the_order_and_refunds_it() {
        let (mut m, a) = docked();
        // Sea in x 0..9 (the port at (5, 5) lies in it), the coast at x 9..10, land from x 10.
        let mut grid = super::super::pathing::PathGrid::new((0.0, 0.0), 2.0, 10, 6);
        grid.poly = Some(super::super::embark::tests::coast_map());
        m.attach_terrain(super::super::world::Terrain(Arc::new(grid)));
        let army = ForceId(1000);
        let general = m.world.forces[&army].commander.unwrap();
        m.world.characters.get_mut(&general).unwrap().position = at(PORT);
        m.world.embarked.insert(army, NAVY);
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        let paid = treasury(&m, a);
        let ev = m.apply(CampaignCommand::Disembark { force: army, to: at((15, 5)) }).unwrap();
        assert!(ev.contains(&CampaignEvent::CharacterDisembarksNavy { character: general, navy: NAVY }), "{ev:?}");
        assert_ne!(m.world.characters[&CharacterId(109)].position, at(PORT), "the navy sailed");
        assert!(!flagged(&m));
        assert_eq!(treasury(&m, a), paid + 400);
        m.apply(CampaignCommand::EndTurn).unwrap();
        assert!(m.world.ship_states[&UnitId(50)].needs_repair(), "not repaired at sea");
    }

    /// A battle's result, real-time or autoresolved, goes through the post-battle undock once: a docked navy that
    /// fought loses its orders, refunded.
    #[test]
    fn a_real_time_battle_result_cancels_the_order_and_refunds_it() {
        let (mut m, a) = docked();
        m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
        let paid = treasury(&m, a);
        m.pending_battle = Some(super::super::battles::PendingBattle { attacker: ForceId(1001), defenders: vec![NAVY], settlement: None, resume: None });
        m.apply_battle_result(super::super::battles::BattleResult { attacker_won: false, attacker_casualties: 0.0, defender_casualties: 0.0, codes: None });
        assert!(!flagged(&m));
        assert_eq!(treasury(&m, a), paid + 400);
    }

    /// The naval autoresolve undocks once, after writing the ships back and before the captures: a docked ship
    /// still afloat has its order cancelled and refunded at its post-battle cost, a ship lost in the battle (sunk
    /// or without crew) takes its order with it, no refund. Run over seeds until one battle has both.
    #[test]
    fn a_naval_autoresolve_refunds_the_survivors_orders_but_not_a_lost_ships() {
        use super::super::naval::ShipRules;
        for seed in 0..64 {
            let (mut m, a) = docked();
            Arc::make_mut(&mut m.rules).ships.insert(
                "test_ship".into(),
                ShipRules { crews: [100, 100, 100], hull: 2000.0, sink_weight: 0.45, morale: 9.0, fire: [60, 40], guns: 74, range: 2, ..Default::default() },
            );
            // A second ship in the docked navy, near its sinking point; both ordered to repair.
            let mut second = m.world.forces[&NAVY].units[0].clone();
            second.id = UnitId(51);
            m.world.forces.get_mut(&NAVY).unwrap().units.push(second);
            for (id, damage) in [(UnitId(50), 0.05), (UnitId(51), 0.5)] {
                let state = ShipState { guns: 74, ..ship([100; 3], [100; 3], [damage, damage, damage, damage, 0.0]) };
                m.world.ship_states.insert(id, state);
            }
            m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 0 }).unwrap();
            m.apply(CampaignCommand::RepairShip { force: NAVY, unit: 1 }).unwrap();
            let paid = treasury(&m, a);
            // Another faction's navy of one whole ship attacks from the sea.
            let b = m.world.factions.keys().copied().find(|&f| f != a).unwrap();
            let mut admiral = m.world.characters[&CharacterId(109)].clone();
            (admiral.id, admiral.faction, admiral.position) = (CharacterId(111), b, at((7, 5)));
            m.world.characters.insert(admiral.id, admiral);
            let enemy = ForceId(1007);
            let whole = |id: i32| CampaignUnit { id: UnitId(id), unit_key: "test_ship".into(), men: 300, max_men: 300, character: None, officer_name: Default::default() };
            m.world.forces.insert(enemy, MilitaryForce { id: enemy, faction: b, commander: Some(CharacterId(111)), units: vec![whole(60)], is_navy: true });
            m.pending_battle = Some(super::super::battles::PendingBattle { attacker: enemy, defenders: vec![NAVY], settlement: None, resume: None });
            m.rng = crate::rng::CaRng::new(seed);
            m.autoresolve().unwrap();

            // A ship afloat keeps its state (wherever it ended, captured or not); a lost one's state is gone.
            let (mut refund, mut lost) = (0, 0);
            for id in [UnitId(50), UnitId(51)] {
                match m.world.ship_states.get(&id) {
                    Some(s) => {
                        assert!(!s.repairing, "seed {seed}: the undock cancelled the order of {id:?}");
                        let unit = m.world.forces.values().flat_map(|f| &f.units).find(|u| u.id == id).expect("a ship afloat is in a force");
                        refund += m.ship_repair_cost(unit);
                    }
                    None => lost += 1,
                }
            }
            assert_eq!(treasury(&m, a), paid + refund, "seed {seed}: only the ships afloat are refunded");
            if lost == 1 {
                return;
            }
        }
        panic!("no seed lost one docked ship and kept the other");
    }

    /// The AI orders every ship it can, charging each at once: with money for one repair the second ship is left.
    #[test]
    fn the_ai_orders_each_ship_it_can_pay_for_in_unit_order() {
        let (mut m, a) = docked();
        let mut second = m.world.forces[&NAVY].units[0].clone();
        second.id = UnitId(51);
        m.world.forces.get_mut(&NAVY).unwrap().units.push(second);
        let state = m.world.ship_states[&UnitId(50)];
        m.world.ship_states.insert(UnitId(51), state);
        m.world.factions.get_mut(&a).unwrap().treasury = 700;
        m.ai_repair_navy(NAVY);
        assert!(m.world.ship_states[&UnitId(50)].repairing);
        assert!(!m.world.ship_states[&UnitId(51)].repairing, "300 left is not above the cost of 400");
        assert_eq!(treasury(&m, a), 300);
        // An ordered ship is not ordered (and charged) again.
        m.world.factions.get_mut(&a).unwrap().treasury = 1000;
        m.ai_repair_navy(NAVY);
        assert_eq!(treasury(&m, a), 600, "only the second ship is charged");
        assert!(m.world.ship_states[&UnitId(51)].repairing);
    }

    /// `disable_shopping_for_ai_under_shroud`: the one human's shroud holds the AI back where it hides the navy.
    #[test]
    fn the_ai_does_not_repair_under_the_scripted_shroud() {
        use super::super::visibility::{CellSet, Shroud, SightGrid};
        let (mut m, a) = docked();
        let grid = SightGrid::centred(64, 64, 64);
        let mut hidden = CellSet::new(64, 64);
        let (x, z) = grid.cell_of((PORT.0 as f32, PORT.1 as f32)).unwrap();
        hidden.set(x, z);
        m.world.sight_grid = Some(grid);
        m.world.shrouds.insert(a, Shroud { explored: CellSet::new(64, 64), visible: CellSet::new(64, 64), hidden, active: true });
        // Two humans: no shroud is stored, the AI shops.
        let b = m.world.factions.keys().copied().find(|&f| f != a).unwrap();
        m.turn.humans = vec![a, b];
        m.set_ai_shopping_shroud(true);
        assert_eq!(m.world.ai_shopping_shroud, None);
        m.turn.humans = vec![a];
        m.set_ai_shopping_shroud(true);
        assert_eq!(m.world.ai_shopping_shroud, Some(a));
        let before = m.state_hash();
        m.ai_repair_navy(NAVY);
        assert_eq!(m.state_hash(), before, "hidden: nothing ordered, nothing paid");
        // The shroud off (`active` false) hides nothing.
        m.world.shrouds.get_mut(&a).unwrap().active = false;
        m.ai_repair_navy(NAVY);
        assert!(m.world.ship_states[&UnitId(50)].repairing);
        // The script's off clears the stored shroud.
        m.set_ai_shopping_shroud(false);
        assert_eq!(m.world.ai_shopping_shroud, None);
    }
}

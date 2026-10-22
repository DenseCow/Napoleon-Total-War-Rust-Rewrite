//! A settlement's siege on the campaign map and its round-start attrition (CAMPAIGN_FIDELITY.md
//! "Round start", siege attrition; CONFIRMED by disassembly unless a line says otherwise).
//!
//! The siege state is the settlement's `SIEGEABLE_GARRISON_RESIDENCE` (CONFIRMED, the ESF reader
//! `0x00AF4B00` / writer `0x00B7A5F0`): #2 the turns the garrison holds out before it must surrender
//! (+0x108), #3 the turns it has been besieged (+0x10C) and #13 the besieging forces (+0x12C, each
//! force's +0x7C links back). The first besieger (`0x00B112F0`) sets #2 from the residence's virtual
//! +0xB4 (a settlement: 2 below 25,000 people, 3 below 100,000, 4 below 1,000,000, 5 below
//! 10,000,000, else 6, `0x00B57B10`; a fort: twice its level + 2, `0x00B57B00`) and #3 to 0; the
//! turns left before surrender are #2 − #3, or 0 once #3 reaches #2 (`0x008AF520`, the
//! `siege_turns_until_surrender` script value).
//!
//! At each round start (`0x0096C050` → `0x00AAAF60` per region of world +0x34, in `REGIONS_ARRAY`
//! order, after the computed factors and before the trade update), a settlement under siege
//! (virtual +0x98: its besieger list is not empty, `0x00A8B9C0`) runs `0x00B796B0`, then each of the
//! region's forts under siege does the same:
//!
//! 1. #3 goes up by 1 (no cap), with or without a garrison.
//! 2. With a garrison army (virtual +0x1C): the percent `p` of [`attrition_percent`] from
//!    `n = min(#3, #2)`.
//! 3. Each garrison unit in order, but the commander's own (army +0x74 → character +0x2B0, no draw):
//!    one campaign RNG draw (model +0xFB8, the `0x005F0880` range draw inlined at `0x00B797B2`,
//!    [`crate::rng::CaRng::int_range`]`(0, p)`) gives the percent `r` it loses; `0x00B5C0E0` removes
//!    `men · r / 100` men (all of them at `r` ≥ 100) through `0x008AABB0`. A unit left at or below
//!    its minimum strength (`0x008F68F0`, [`super::economy::unit_minimum_men`]) is kept for step 4.
//! 4. After the loop, each kept unit's character (unit +0xF4) dies (`0x00A0C6F0(1)`) and the unit is
//!    deleted.
//!
//! The siege ends when its besieger list empties. A besieger leaves it through one path,
//! [`CampaignModel::end_force_siege`] (`EndForceSiegeOfSettlement` `0x008BD550`), reached from
//! (CONFIRMED, disassembly):
//! - its destruction: the military force's destructor `0x0088CE30` calls it at `0x0088CE93` when the
//!   force's siege object +0x94 is set and the campaign is not being torn down (+0xFC5), before the
//!   base teardown `0x0088ECC0` ([`CampaignModel::remove_force`]);
//! - any change of the settlement's owner (`TransferSettlementOwnership` `0x00B449F0` fires the
//!   settlement's owner-changed event +0x24 at `0x00B44B31`, after `0x00A64AC0`; the siege object's
//!   listener `0x008DB7D0` ends the siege (its call `0x008DB84A`) when the besieger's faction is the new owner or not at war
//!   with it, `IsFactionAtWarWith` `0x008CE9B0`): captures, deals, liberations and the director's
//!   transfer alike ([`CampaignModel::end_sieges_after_owner_change`]);
//! - a capture, for the captor's own siege, again after the owner change (`0x00B586E9`,
//!   `0x00B5898A`, `0x00B58D3B` in the three capture variants, each guarded by the force's siege
//!   object +0x94: a no-op after the listener);
//! - a peace (`0x00B262C0` → `0x00A25570` per commander of either side, the call `0x00A255C5`):
//!   his force's siege of a settlement the other side owns (target virtual +0x14 equal to it,
//!   `0x00898B20`) ends ([`CampaignModel::end_siege_on_peace`]); a besieger with no commander is
//!   not reached.
//!
//! Each end that finds a siege also runs the trade price update with flag 0 (`0x00BCB020(0)` at
//! `0x008BD65B`), as the exe's `0x008BD550` does.
//!
//! PROVISIONAL: its other callers are not traced (`0x008FF4B7`, `0x008BEB42`, `0x008AC130`,
//! `0x00B290B0`, `0x008DB7BD` the listener `0x008DB7A0`, `0x008F801B` the post-battle placement,
//! `0x00A787C0`, `0x00B584ED`, `0x00B5881D`, `0x00B58BD8`, `0x00932794`), so a besieger that moves
//! off, among others, does not end its siege here; nor how a siege starts in the model (BACKLOG
//! "Sieges on the campaign map"). The model has no forts under siege (`World::forts` is empty in
//! every shipped campaign), so only settlements run it.
//!
//! The model calls it through the [`siege.attrition`](super::seams::SIEGE_ATTRITION) seam
//! ([`CampaignModel::siege_attrition_round_start`]).

use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use super::mod_state::ModWrites;
use super::world::CampaignModel;

/// The siege state of a region's settlement (its `SIEGEABLE_GARRISON_RESIDENCE` #2, #3 and #13; see
/// the module docs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SiegeState {
    /// The forces besieging the settlement (#13, residence +0x12C), in the list's order.
    pub besiegers: Vec<ForceId>,
    /// The turns the garrison holds out before it must surrender (#2, +0x108), set when the siege
    /// starts.
    pub surrender_turns: u32,
    /// The turns the settlement has been besieged (#3, +0x10C).
    pub turns_besieged: u32,
}

impl SiegeState {
    /// `true` while a force besieges the settlement (the residence's virtual +0x98, `0x00A8B9C0`).
    pub fn under_siege(&self) -> bool {
        !self.besiegers.is_empty()
    }

    /// The turns left before the garrison must surrender: #2 − #3, or 0 once #3 reaches #2
    /// (`0x008AF520`).
    pub fn turns_until_surrender(&self) -> u32 {
        self.surrender_turns.saturating_sub(self.turns_besieged)
    }
}

/// The highest percent of its men a garrison unit can lose to one round of siege attrition
/// (`0x00B79732`).
pub const MAX_ATTRITION_PERCENT: u32 = 30;

/// The siege's attrition percent for a round (`0x00B796E3`–`0x00B79735`): with
/// `n = min(turns_besieged, surrender_turns)`, `trunc((n² − 1) · 1.5)` worked as the exe does (the
/// `u32` `n² − 1` wraps, goes to `f64` unsigned, to `f32`, times the `f32` 1.5 at `0x0131A7B8`, then
/// a truncating `f32` → `i32` conversion that gives `0x80000000` out of range), then 1 when the
/// result is below 1 as unsigned, else at most [`MAX_ATTRITION_PERCENT`]. So n = 1 gives 1, 2 gives
/// 4, 3 gives 12, 4 gives 22, 5 and more 30; n = 0 (a siege loaded with no surrender turns) wraps to
/// 30.
pub fn attrition_percent(turns_besieged: u32, surrender_turns: u32) -> u32 {
    let n = turns_besieged.min(surrender_turns);
    let square = n.wrapping_mul(n).wrapping_sub(1);
    let scaled = (square as f64) as f32 * 1.5f32;
    // CVTTSS2SI: truncation, the "integer indefinite" 0x80000000 when out of range.
    let truncated = if (-2_147_483_648.0..2_147_483_648.0).contains(&scaled) { scaled as i32 as u32 } else { 0x8000_0000 };
    if truncated < 1 {
        1
    } else {
        truncated.min(MAX_ATTRITION_PERCENT)
    }
}

/// The men a unit of `men` loses to `percent` (`0x00B5C0E0`): `men · percent / 100`, truncated, or
/// all of them from 100 up. The product is the exe's 32-bit `IMUL` (wrapping) and the division
/// unsigned (`MUL 0x51EB851F`, `SHR 5`); it wraps only above 143,165,576 men at 30 %, and the result
/// never exceeds `men` then either.
pub fn men_lost(men: u32, percent: u32) -> u32 {
    if percent < 100 {
        men.wrapping_mul(percent) / 100
    } else {
        men
    }
}

/// The original's round-start siege attrition for one region's settlement under siege
/// (`0x00B796B0`, see the module docs): the turns besieged go up, each garrison unit but the
/// commander's loses a drawn percent of its men, and the units left at or below minimum strength
/// go with their characters. Does nothing when the settlement is not under siege. The model calls
/// it through the [`siege.attrition`](super::seams::SIEGE_ATTRITION) seam.
pub fn siege_attrition(model: &mut CampaignModel, region: RegionId) -> Vec<CampaignEvent> {
    let mut events = Vec::new();
    let Some(r) = model.world.regions.get_mut(&region) else { return events };
    if !r.siege.under_siege() {
        return events;
    }
    r.siege.turns_besieged = r.siege.turns_besieged.wrapping_add(1);
    let percent = attrition_percent(r.siege.turns_besieged, r.siege.surrender_turns);
    let Some(garrison) = r.garrison else { return events };
    let Some(force) = model.world.forces.get(&garrison) else {
        log::warn!("region {region:?}: garrison force {garrison:?} is not in the world; no siege attrition");
        return events;
    };
    let commander = force.commander;
    let minimums: Vec<u32> = force.units.iter().map(|u| super::economy::unit_minimum_men(model, u.max_men)).collect();
    let mut gone = Vec::new();
    let force = model.world.forces.get_mut(&garrison).expect("checked above");
    for (i, u) in force.units.iter_mut().enumerate() {
        if commander.is_some() && u.character == commander {
            continue;
        }
        let drawn = model.rng.int_range(0, percent as i32) as u32;
        u.men -= men_lost(u.men, drawn);
        if u.men <= minimums[i] {
            gone.push(i);
        }
    }
    let mut fallen: Vec<CharacterId> = Vec::new();
    for &i in gone.iter().rev() {
        let u = force.units.remove(i);
        fallen.extend(u.character);
    }
    let emptied = force.units.is_empty();
    // The exe kills the characters in the units' order.
    for c in fallen.into_iter().rev() {
        model.character_falls(c);
    }
    if emptied {
        // Only a garrison with no commander can lose every unit (the commander's unit is never
        // drawn); the exe reads the commander unchecked (`0x00B79790`), so it never meets one. The
        // model keeps no empty force.
        log::warn!("region {region:?}: siege attrition removed every unit of garrison {garrison:?}; the force is destroyed");
        model.destroy_force(garrison, &mut events);
    }
    events
}

impl CampaignModel {
    /// `force` leaves the siege it holds, if any (`EndForceSiegeOfSettlement` `0x008BD550`, CONFIRMED,
    /// disassembly: the siege object at force +0x94 is destroyed, which unlinks the force from the
    /// settlement's besiegers; a force holds one siege at most). The settlement's turns stay, as in the
    /// residence. The one path every end of a siege takes (see the module docs). When a siege did end,
    /// the exe then runs, unless the campaign is being torn down (+0xFC5, which the model never is), the
    /// trade price update `0x00BCB020(0)` at `0x008BD65B` ([`Self::update_commodity_prices_with`] with
    /// flag 0: the prices, the commerce raid check and the trade income) and
    /// `RebuildRegionRecruitablesAndEffects` `0x00A6AE40` on the target (no port needed: the model derives
    /// a region's recruitables and building effects from its slots when read,
    /// [`Self::recruitable_units`], [`super::Region::effect_buildings`]). PROVISIONAL: its call
    /// `0x00B084F0` on the target's character and its event 0xDF (model +0x9A8) are not traced and not
    /// run.
    pub(crate) fn end_force_siege(&mut self, force: ForceId) {
        let Some(r) = self.world.regions.values_mut().find(|r| r.siege.besiegers.contains(&force)) else { return };
        r.siege.besiegers.retain(|&b| b != force);
        self.update_commodity_prices_with(false);
    }

    /// The settlement's owner-changed event (+0x24, fired by `0x00B449F0` at `0x00B44B31`) reaching
    /// each siege of `region`'s settlement (listener `0x008DB7D0`, CONFIRMED): a besieger of the new
    /// owner's faction, or not at war with it ([`Self::at_war`], `0x008CE9B0`), ends its siege
    /// ([`Self::end_force_siege`]); one at war with the new owner goes on besieging. A besieger id with
    /// no force in the world (the model never leaves one) is dropped with a warning.
    pub(crate) fn end_sieges_after_owner_change(&mut self, region: RegionId) {
        let Some(r) = self.world.regions.get(&region) else { return };
        let owner = r.owner;
        let ended: Vec<ForceId> = r
            .siege
            .besiegers
            .iter()
            .copied()
            .filter(|b| match self.world.forces.get(b) {
                Some(f) => f.faction == owner || !self.at_war(f.faction, owner),
                None => {
                    log::warn!("region {region:?}: besieger {b:?} is not in the world; its siege is dropped");
                    true
                }
            })
            .collect();
        for force in ended {
            self.end_force_siege(force);
        }
    }

    /// A peace's siege step for character `c` towards the other side `other` (`0x00B262C0` → `0x00A25570`,
    /// the call `0x00A255C5`, CONFIRMED): the army he commands (`0x009D3A40`, [`Self::force_of_kind`]: a navy
    /// is not looked at), if it besieges a settlement `other` owns (the target's virtual +0x14 equal to it,
    /// `0x00898B20`), ends its siege ([`Self::end_force_siege`]). The exe reaches a force only through its
    /// commander, so a besieger with none goes on besieging.
    pub(crate) fn end_siege_on_peace(&mut self, c: CharacterId, other: FactionId) {
        let Some(force) = self.force_of_kind(c, false) else { return };
        if self.world.regions.values().any(|r| r.owner == other && r.siege.besiegers.contains(&force)) {
            self.end_force_siege(force);
        }
    }

    /// The round start's siege attrition (`0x00AAAF60` per region in the region manager's order,
    /// [`super::World::regions_in_manager_order`]): each region whose settlement is under siege runs the
    /// [`siege.attrition`](super::seams::SIEGE_ATTRITION) seam ([`siege_attrition`] in the original).
    pub fn siege_attrition_round_start(&mut self, events: &mut Vec<CampaignEvent>) {
        if !self.world.regions.values().any(|r| r.siege.under_siege()) {
            return;
        }
        let rules = std::sync::Arc::clone(&self.rules);
        // Each region's siege is tested when its turn comes, as `0x00AAAF60` does.
        for region in self.world.regions_in_manager_order() {
            if !self.world.regions.get(&region).is_some_and(|r| r.siege.under_siege()) {
                continue;
            }
            let mut writes = ModWrites::default();
            events.extend(rules.seams.siege_attrition.rule()(self, region, &mut writes));
            self.mod_state.apply(&mut writes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::tests::test_model;
    use crate::campaign::{CampaignUnit, TurnStep, UnitId};

    const BESIEGED: RegionId = RegionId(10); // test_model's region of faction A
    const GARRISON: ForceId = ForceId(1000); // A's army under general 100
    const BESIEGER: ForceId = ForceId(1001); // B's army

    fn unit(id: i32, men: u32, character: Option<CharacterId>) -> CampaignUnit {
        CampaignUnit { id: UnitId(id), unit_key: "test_unit".into(), men, max_men: 100, character, officer_name: Default::default() }
    }

    /// test_model with A's army 1000 garrisoned in region 10 (the general's own unit first, then two
    /// more, one of them led by character 102) and B's army 1001 besieging it.
    fn besieged_model(surrender_turns: u32, turns_besieged: u32) -> CampaignModel {
        let mut m = test_model();
        let f = m.world.forces.get_mut(&GARRISON).unwrap();
        f.units = vec![unit(1, 60, Some(CharacterId(100))), unit(2, 80, None), unit(3, 90, Some(CharacterId(102)))];
        let r = m.world.regions.get_mut(&BESIEGED).unwrap();
        r.garrison = Some(GARRISON);
        r.siege = SiegeState { besiegers: vec![BESIEGER], surrender_turns, turns_besieged };
        m
    }

    fn men(m: &CampaignModel) -> Vec<(i32, u32)> {
        m.world.forces[&GARRISON].units.iter().map(|u| (u.id.0, u.men)).collect()
    }

    /// `0x00B796E3`–`0x00B79735`: trunc((n² − 1) · 1.5), at least 1, at most 30, n = min(turns, cap).
    #[test]
    fn the_attrition_percent_grows_with_the_turns_up_to_the_surrender_turns() {
        let table: Vec<u32> = (1..=7).map(|n| attrition_percent(n, 10)).collect();
        assert_eq!(table, [1, 4, 12, 22, 30, 30, 30]);
        assert_eq!(attrition_percent(9, 3), 12, "n is capped at the surrender turns");
        assert_eq!(attrition_percent(0, 4), 30, "n = 0: n² − 1 wraps to u32::MAX, out of i32 range");
        assert_eq!(attrition_percent(5, 0), 30, "a siege loaded with no surrender turns");
        assert_eq!(attrition_percent(70_000, 70_000), 30, "n² wraps");
    }

    /// `0x00B5C0E0`: a percent of the unit's men, truncated; all of them from 100.
    #[test]
    fn a_unit_loses_a_truncated_percent_of_its_men() {
        assert_eq!(men_lost(80, 12), 9);
        assert_eq!(men_lost(99, 1), 0);
        assert_eq!(men_lost(80, 0), 0);
        assert_eq!(men_lost(80, 100), 80);
        assert_eq!(men_lost(u32::MAX, 30), u32::MAX.wrapping_mul(30) / 100, "the 32-bit IMUL wraps");
        assert_eq!(men_lost(143_165_577, 30), 0, "the product wraps to 14");
        assert_eq!(men_lost(143_165_576, 30), 42_949_672, "the largest unit that does not wrap");
    }

    /// One draw per unit but the commander's (no draw), in unit order, of `int_range(0, p)` on the
    /// campaign RNG; the counter goes up first.
    #[test]
    fn each_garrison_unit_but_the_commanders_draws_its_loss_in_order() {
        let mut m = besieged_model(4, 2);
        let mut rng = m.rng;
        let percent = attrition_percent(3, 4);
        assert_eq!(percent, 12);
        let (r2, r3) = (rng.int_range(0, 12) as u32, rng.int_range(0, 12) as u32);
        let events = siege_attrition(&mut m, BESIEGED);
        assert!(events.is_empty());
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 3);
        assert_eq!(men(&m), [(1, 60), (2, 80 - men_lost(80, r2)), (3, 90 - men_lost(90, r3))]);
        assert_eq!(m.rng, rng, "two draws, none for the commander's unit");
        assert_eq!(m.world.regions[&BESIEGED].siege.besiegers, [BESIEGER], "the siege goes on");
    }

    /// `0x00B797F9`–`0x00B79925`: a unit left at or below its minimum strength (4 men here) goes after
    /// the loop, and its character dies with it.
    #[test]
    fn a_unit_at_minimum_strength_goes_with_its_character() {
        let mut m = besieged_model(6, 10);
        {
            let f = m.world.forces.get_mut(&GARRISON).unwrap();
            f.units[2].men = 4; // already at the minimum: any draw removes it
        }
        let min = crate::campaign::economy::unit_minimum_men(&m, 100);
        assert_eq!(min, 5, "test_rules: ceil(0.05 · 100)");
        let mut rng = m.rng;
        let r2 = rng.int_range(0, 30) as u32;
        let _ = rng.int_range(0, 30);
        siege_attrition(&mut m, BESIEGED);
        assert_eq!(men(&m), [(1, 60), (2, 80 - men_lost(80, r2))]);
        assert!(!m.world.characters.contains_key(&CharacterId(102)), "the unit's character dies");
        assert!(m.world.characters.contains_key(&CharacterId(100)), "the general stays");
        assert_eq!(m.rng, rng, "the removed unit drew too");
    }

    /// The counter goes up with no garrison (`0x00B796D2` before the garrison test at `0x00B796DD`), and no
    /// draw is made.
    #[test]
    fn the_turns_besieged_go_up_without_a_garrison() {
        let mut m = besieged_model(3, 1);
        m.world.regions.get_mut(&BESIEGED).unwrap().garrison = None;
        let rng = m.rng;
        siege_attrition(&mut m, BESIEGED);
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 2);
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_until_surrender(), 1);
        assert_eq!(m.rng, rng);
    }

    #[test]
    fn a_region_not_under_siege_is_left_alone() {
        let mut m = besieged_model(3, 1);
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.clear();
        let before = m.clone();
        let mut events = Vec::new();
        m.siege_attrition_round_start(&mut events);
        assert!(events.is_empty());
        assert_eq!(m.state_hash(), before.state_hash());
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 1);
    }

    /// A destroyed besieger leaves the siege (`0x0088ECC0` unlinks force +0x7C); the turns stay.
    #[test]
    fn a_destroyed_besieger_ends_the_siege() {
        let mut m = besieged_model(4, 2);
        let mut events = Vec::new();
        m.destroy_force(BESIEGER, &mut events);
        let siege = &m.world.regions[&BESIEGED].siege;
        assert!(!siege.under_siege());
        assert_eq!((siege.surrender_turns, siege.turns_besieged), (4, 2));
        let before = men(&m);
        m.siege_attrition_round_start(&mut events);
        assert_eq!(men(&m), before);
    }

    /// The round-start step runs the attrition (`0x0096C0F2`, after the computed factors, before the
    /// trade update) with the campaign RNG as it stands there.
    #[test]
    fn the_round_start_runs_the_siege_attrition() {
        let mut m = besieged_model(4, 2);
        let mut direct = m.clone();
        siege_attrition(&mut direct, BESIEGED);
        m.turn.humans = vec![FactionId(1)];
        m.turn.queue.push_back(TurnStep::RoundStart);
        m.step().expect("the round-start step");
        assert_eq!(m.world.regions[&BESIEGED].siege.turns_besieged, 3);
        assert_eq!(men(&m), men(&direct));
    }

    /// The regions run in the region manager's order (`World::region_order`), not id order.
    #[test]
    fn the_regions_run_in_the_region_managers_order() {
        let mut m = besieged_model(5, 4);
        // A second besieged settlement: region 12 (A's), garrisoned by a new army of A under general 100's
        // colleague 102 (his unit leaves army 1000).
        m.world.forces.get_mut(&GARRISON).unwrap().units.truncate(2);
        let other = ForceId(1005);
        m.world.forces.insert(
            other,
            crate::campaign::MilitaryForce { id: other, faction: FactionId(1), commander: Some(CharacterId(102)), units: vec![unit(5, 70, Some(CharacterId(102))), unit(6, 90, None), unit(7, 50, None)], is_navy: false },
        );
        let r = m.world.regions.get_mut(&RegionId(12)).unwrap();
        r.garrison = Some(other);
        r.siege = SiegeState { besiegers: vec![BESIEGER], surrender_turns: 5, turns_besieged: 1 };
        m.world.region_order = vec![RegionId(12), RegionId(11), RegionId(10)];
        let run = |m: &CampaignModel, order: [RegionId; 2]| {
            let mut m = m.clone();
            for r in order {
                siege_attrition(&mut m, r);
            }
            m
        };
        let file_order = run(&m, [RegionId(12), BESIEGED]);
        let id_order = run(&m, [BESIEGED, RegionId(12)]);
        let mut events = Vec::new();
        m.siege_attrition_round_start(&mut events);
        let all = |m: &CampaignModel| m.world.forces.values().flat_map(|f| f.units.iter().map(|u| u.men)).collect::<Vec<_>>();
        assert_eq!(all(&m), all(&file_order));
        assert_ne!(all(&m), all(&id_order), "the orders draw differently here");
    }

    /// A third army of faction 3 joins the siege of region 10.
    fn with_third_besieger(m: &mut CampaignModel) -> ForceId {
        let third = ForceId(1010);
        m.world.forces.insert(
            third,
            crate::campaign::MilitaryForce { id: third, faction: FactionId(3), commander: None, units: vec![unit(8, 50, None)], is_navy: false },
        );
        m.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.push(third);
        third
    }

    /// The captor's capture ends its siege (`0x00B586E9`, and the owner-changed listener `0x008DB7D0`
    /// for the new owner's own force); a besieger at war with the new owner goes on (`0x008CE9B0`).
    #[test]
    fn a_capture_ends_the_captors_siege_and_a_besieger_at_war_with_the_captor_stays() {
        let mut m = besieged_model(4, 2);
        let third = with_third_besieger(&mut m);
        m.world.set_stance(FactionId(1), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.world.set_stance(FactionId(3), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.capture_by_force(BESIEGED, FactionId(2), Some(BESIEGER));
        let siege = &m.world.regions[&BESIEGED].siege;
        assert_eq!(m.world.regions[&BESIEGED].owner, FactionId(2));
        assert_eq!(siege.besiegers, [third]);
        assert_eq!((siege.surrender_turns, siege.turns_besieged), (4, 2), "the turns stay");
    }

    /// The owner-changed listener `0x008DB7D0` ends the siege of a besieger not at war with the new owner.
    #[test]
    fn an_owner_change_ends_the_siege_of_a_besieger_at_peace_with_the_new_owner() {
        let mut m = besieged_model(4, 2);
        with_third_besieger(&mut m);
        m.world.set_stance(FactionId(1), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.capture_by_force(BESIEGED, FactionId(2), Some(BESIEGER));
        assert!(!m.world.regions[&BESIEGED].siege.under_siege());
    }

    /// Peace between the owner and a besieger's faction ends that siege (`0x00A25570`, the call
    /// `0x00A255C5`); a besieger of a third faction goes on, and so does one with no commander or a commander
    /// in no character list (the exe walks each side's character list).
    #[test]
    fn peace_ends_the_siege_of_the_other_sides_besieger() {
        let mut m = besieged_model(4, 2);
        let third = with_third_besieger(&mut m);
        m.world.set_stance(FactionId(1), FactionId(2), crate::campaign::Stance::War).unwrap();
        m.world.set_stance(FactionId(1), FactionId(3), crate::campaign::Stance::War).unwrap();
        m.make_peace_rules(FactionId(1), FactionId(2), true).expect("at war");
        assert_eq!(m.world.regions[&BESIEGED].siege.besiegers, [third]);
        let mut no_commander = m.clone();
        no_commander.make_peace_rules(FactionId(3), FactionId(1), true).expect("at war");
        assert_eq!(no_commander.world.regions[&BESIEGED].siege.besiegers, [third], "no commander: not reached");
        m.world.forces.get_mut(&third).unwrap().commander = Some(CharacterId(300));
        let mut unlisted = m.clone();
        unlisted.make_peace_rules(FactionId(3), FactionId(1), true).expect("at war");
        assert_eq!(unlisted.world.regions[&BESIEGED].siege.besiegers, [third], "a commander in no character list: not reached");
        let general = crate::campaign::Character {
            id: CharacterId(300),
            faction: FactionId(3),
            kind: crate::campaign::CharacterKind::General,
            position: Default::default(),
            movement_points: 0,
            max_movement_points: 0,
            base_movement_points: 0,
            garrisoned_in: None,
        };
        m.world.characters.insert(general.id, general);
        m.make_peace_rules(FactionId(3), FactionId(1), true).expect("at war");
        assert!(!m.world.regions[&BESIEGED].siege.under_siege(), "either side may propose");
    }

    /// The state hash covers the siege.
    #[test]
    fn the_state_hash_covers_the_siege() {
        let m = besieged_model(4, 2);
        let mut turns = m.clone();
        turns.world.regions.get_mut(&BESIEGED).unwrap().siege.turns_besieged = 3;
        let mut surrender = m.clone();
        surrender.world.regions.get_mut(&BESIEGED).unwrap().siege.surrender_turns = 5;
        let mut lifted = m.clone();
        lifted.world.regions.get_mut(&BESIEGED).unwrap().siege.besiegers.clear();
        for other in [&turns, &surrender, &lifted] {
            assert_ne!(m.state_hash(), other.state_hash());
        }
    }
}

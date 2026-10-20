//! Campaign battles: a pending battle, autoresolve, and applying a result.
//!
//! An attack (or an assault on a defended settlement) sets [`CampaignModel::pending_battle`].
//! The battle is then settled in one of two ways:
//! - **Autoresolve** ([`CampaignModel::autoresolve`], command `Autoresolve`): ports the original's
//!   stat-based resolver ([`super::autoresolve`]) with the campaign RNG; it sets every unit's men,
//!   then [`CampaignModel::apply_battle_result`] settles the rest with zero extra casualties (the naval
//!   autoresolve shares its settling step).
//! - **Real-time battle (hook, not wired yet):** the display reads the pending battle, launches
//!   `GameMode::Battle` with the two sides' units, and afterwards calls
//!   [`CampaignModel::apply_battle_result`] with the casualties. Autoresolve uses the same function,
//!   so both paths change the campaign in the same way.
//!
//! PROVISIONAL rules (the original's are UNKNOWN): a real-time battle's casualties spread over
//! every unit of a side in proportion to its men; a unit below `unit_minimum_strength` of its maximum is
//! removed; the loser of a field battle stays where it is (CONFIRMED: no aftermath step of 0x008F7880 moves it); a settlement
//! whose defenders lose is occupied (its garrison army destroyed and its queues cleared: CONFIRMED, see `occupy`).

use super::events::CampaignEvent;
use super::ids::{CharacterId, ForceId, RegionId};
use super::world::{CampaignModel, MilitaryForce};
use super::CommandError;
use super::autoresolve::{resolve, ArUnit, ArVars};

/// A battle waiting to be fought.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingBattle {
    /// The attacking force.
    pub attacker: ForceId,
    /// The defending forces (the target and, for a settlement, everything inside it).
    pub defenders: Vec<ForceId>,
    /// The region whose settlement is assaulted, if any.
    pub settlement: Option<RegionId>,
    /// The order the attacker resumes after winning (the post-battle state 7 of `0x008F7880`, CONFIRMED:
    /// the attacker continues its move or attack): the settlement it was marching on when an army
    /// outside it stood in the way.
    pub resume: Option<RegionId>,
}

/// How a battle ended (from autoresolve or from a real-time battle).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BattleResult {
    /// True if the attacker won.
    pub attacker_won: bool,
    /// Fraction of the attacker's men lost (0..=1).
    pub attacker_casualties: f32,
    /// Fraction of the defenders' men lost (0..=1).
    pub defender_casualties: f32,
}

impl CampaignModel {
    /// The autoresolve units of some forces, in force and unit order. A unit without autoresolve
    /// data (ships) takes part with zero potential. A unit carries a general when its character is
    /// the force's commander. Its star rating is its battle card's rank ([`Self::battle_general_card`],
    /// CONFIRMED: `0x007CA330` passes land card `+0xBC` to the resolver unit's `+0xAC` at `0x007CA5CE`; the
    /// cards are the `0x008AF200` ones the battle's armies hold, appended at `0x008B0F1C` → `0x00531370`).
    pub(super) fn autoresolve_units(&self, forces: &[ForceId]) -> Vec<(ForceId, usize, ArUnit)> {
        self.resolver_units(forces, |_, f, u, star_rating| {
            let data = self.rules.units.get(&u.unit_key).and_then(|r| r.autoresolve).unwrap_or_default();
            let has_general = u.character.is_some() && u.character == f.commander;
            ArUnit { data, men: u.men, has_general, star_rating, human: self.turn.humans.contains(&f.faction) }
        })
    }

    /// The resolver units of some forces, in force and unit order, shared by the land and naval
    /// autoresolves (`0x007CA330` walks the battle's armies the same way for both): `make` gets the
    /// force's index in `forces`, the force, the unit and the unit's star rating (its battle card's rank).
    fn resolver_units<T>(
        &self,
        forces: &[ForceId],
        mut make: impl FnMut(usize, &MilitaryForce, &super::world::CampaignUnit, i32) -> T,
    ) -> Vec<(ForceId, usize, T)> {
        let mut out = Vec::new();
        for (k, &fid) in forces.iter().enumerate() {
            let Some(f) = self.world.forces.get(&fid) else { continue };
            for (i, u) in f.units.iter().enumerate() {
                out.push((fid, i, make(k, f, u, self.battle_general_card(f, u).rank)));
            }
        }
        out
    }

    /// Autoresolves the pending battle with the original's stat-based resolver
    /// ([`super::autoresolve`]); the attacker is side A.
    pub fn autoresolve(&mut self) -> Result<Vec<CampaignEvent>, CommandError> {
        let battle = self.pending_battle.clone().ok_or(CommandError::NoPendingBattle)?;
        if self.world.forces.get(&battle.attacker).is_some_and(|f| f.is_navy) {
            return Ok(self.autoresolve_naval(&battle));
        }
        let a = self.autoresolve_units(&[battle.attacker]);
        let b = self.autoresolve_units(&battle.defenders);
        let men = |s: &[(ForceId, usize, ArUnit)]| s.iter().map(|x| x.2.men).sum::<u32>();
        let attacker_won = if men(&b) == 0 {
            true
        } else if men(&a) == 0 {
            false
        } else {
            let ua: Vec<ArUnit> = a.iter().map(|x| x.2).collect();
            let ub: Vec<ArUnit> = b.iter().map(|x| x.2).collect();
            // The battle's difficulty: the campaign difficulty of a human taking part (INFERRED).
            let human = a.iter().chain(&b).find(|x| x.2.human).and_then(|x| self.world.forces.get(&x.0)).map(|f| f.faction);
            let mut vars = ArVars::from_rules(&self.rules);
            vars.difficulty = human.and_then(|f| self.world.faction_details.get(&f)).map_or(0, |d| d.difficulty);
            let o = resolve(&ua, &ub, &vars, &mut self.rng);
            for ((fid, i, _), lost) in a.iter().zip(&o.losses_a).chain(b.iter().zip(&o.losses_b)) {
                if let Some(u) = self.world.forces.get_mut(fid).and_then(|f| f.units.get_mut(*i)) {
                    u.men = u.men.saturating_sub(*lost);
                }
            }
            self.last_autoresolve = Some(o.clone());
            o.a_won
        };
        Ok(self.apply_battle_result(BattleResult { attacker_won, attacker_casualties: 0.0, defender_casualties: 0.0 }))
    }

    /// Applies a battle's outcome to the pending battle and clears it. Also the hook for a
    /// real-time battle's result. Returns no events if no battle is pending.
    pub fn apply_battle_result(&mut self, result: BattleResult) -> Vec<CampaignEvent> {
        let Some(battle) = self.pending_battle.take() else { return Vec::new() };
        self.settle_battle(&battle, result, true)
    }

    /// The battle's outcome ([`Self::apply_battle_result`]) for `battle`, already taken off the model. `undock`: the
    /// post-battle undock is still to run, after the casualties ([`Self::undock_after_battle`]); the naval
    /// autoresolve runs it itself, before its captures, and passes false, so every battle undocks once.
    fn settle_battle(&mut self, battle: &PendingBattle, result: BattleResult, undock: bool) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        let attacker_faction = self.world.forces.get(&battle.attacker).map(|f| f.faction);
        let commanders: Vec<_> = std::iter::once(battle.attacker)
            .chain(battle.defenders.iter().copied())
            .filter_map(|f| self.world.forces.get(&f)?.commander)
            .collect();
        self.apply_casualties(battle.attacker, result.attacker_casualties, &mut events);
        for &d in &battle.defenders {
            self.apply_casualties(d, result.defender_casualties, &mut events);
        }
        if undock {
            self.undock_after_battle(battle);
        }
        // Defenders of a settlement that lose are destroyed (CONFIRMED for the garrison army: the capture
        // 0x00B58560 deletes the settlement's garrison army before the region changes hands).
        if result.attacker_won
            && let Some(region) = battle.settlement
        {
            for &d in &battle.defenders {
                self.destroy_force(d, &mut events);
            }
            if let Some(f) = attacker_faction {
                self.occupy(region, f, Some(battle.attacker), &mut events);
                // The capture after a battle (0x00B58560: the report with the surrender flag clear).
                self.settle_capture(region, f, Some(battle.attacker), false, &mut events);
            }
        }
        if let Some(f) = attacker_faction {
            events.insert(
                0,
                CampaignEvent::BattleCompleted { attacker: battle.attacker, attacker_faction: f, attacker_won: result.attacker_won },
            );
        }
        for c in commanders {
            if self.world.characters.contains_key(&c) {
                events.push(CampaignEvent::CharacterCompletedBattle { character: c });
            }
        }
        // The winner resumes its order (0x008F7880 state 7, CONFIRMED): on into the settlement.
        if result.attacker_won
            && let Some(region) = battle.resume
            && self.world.forces.contains_key(&battle.attacker)
            && let Ok(mut more) = self.enter_settlement(battle.attacker, region)
        {
            events.append(&mut more);
        }
        events
    }

    /// The post-battle placement (`0x008F7880`) undocks each force of the battle still standing ([`Self::undock`]):
    /// a docked navy's repair orders end, refunded at the ships' new cost (`0x008B36D0`). A ship lost in the battle
    /// takes its order with it (no refund). PROVISIONAL: when the placement runs against the naval captures is not
    /// traced; the naval autoresolve undocks before them, so the refund goes to the faction that paid.
    pub(crate) fn undock_after_battle(&mut self, battle: &PendingBattle) {
        for f in std::iter::once(battle.attacker).chain(battle.defenders.iter().copied()) {
            self.undock(f);
        }
    }

    fn apply_casualties(&mut self, force: ForceId, fraction: f32, events: &mut Vec<CampaignEvent>) {
        let min_strength = self.rules.var("unit_minimum_strength", 0.05);
        let Some(f) = self.world.forces.get_mut(&force) else { return };
        let fraction = fraction.clamp(0.0, 1.0) as f64;
        for u in &mut f.units {
            let lost = (u.men as f64 * fraction).round() as u32;
            u.men = u.men.saturating_sub(lost);
        }
        let commander = f.commander;
        let (kept, lost): (Vec<_>, Vec<_>) =
            std::mem::take(&mut f.units).into_iter().partition(|u| u.men > 0 && (u.men as f32) >= min_strength * u.max_men as f32);
        f.units = kept;
        let empty = f.units.is_empty();
        // A unit's attached character (colonel, captain or a general riding with it) falls with
        // it (the original kills him with reason 1 when his unit goes, CHARACTERS_FIDELITY.md §9);
        // when that was the commander, what is left gets a successor (`command_vacated`).
        if empty {
            self.destroy_force(force, events);
        }
        for c in lost.iter().filter_map(|u| u.character).filter(|&c| !(empty && Some(c) == commander)) {
            self.character_falls(c);
        }
    }

    /// A character dies (through [`Self::character_dies`]: a post he held gets a new holder, his
    /// forces a successor), unless he is one of the characters the original rebuilds when killed
    /// (`CHARACTER` #34, +0x52C, §5b; CONFIRMED from the destructor `0x0099D2D0`): a copy of him
    /// (`0x0098FE60`: the same faction, details with traits and ancillaries, sight radius, appeared
    /// turn, duel counters and +0x52C; the idle turns +0x4C8, the hidden flag +0x4E0 and the duel
    /// wound +0x512 cleared) is linked to his force when it still has units (an unlink `0x008B02E0`, then
    /// `0x008CF9D0`: a rider, not its commander) and enters the world where he stood
    /// (`0x009D3B60(0, old)`); the force then gets a successor as usual. The "wounded" message is
    /// `0x00A0B980` / `0x009D2240`. Model: he keeps his id, stands where he fell, leaves his unit
    /// and the command; PROVISIONAL: riding with the force is not kept (the model has no riders),
    /// so he stands on the map next to it.
    pub(crate) fn character_falls(&mut self, c: CharacterId) {
        let returns = self.world.character_details.get(&c).is_some_and(|d| d.returns_after_death);
        if returns {
            let Some(ch) = self.world.characters.get_mut(&c) else { return };
            let old = ch.clone();
            ch.garrisoned_in = None;
            for f in self.world.forces.values_mut() {
                for u in &mut f.units {
                    if u.character == Some(c) {
                        u.character = None;
                    }
                }
            }
            if let Some(d) = self.world.character_details.get_mut(&c) {
                d.idle_turns = 0;
                d.hidden = false;
                d.wounded = false;
            }
            self.command_vacated(&old);
        } else {
            self.character_dies(c);
        }
    }

    /// Removes a force; its commander and every character attached to its units fall with it
    /// (see [`Self::character_falls`]).
    pub(crate) fn destroy_force(&mut self, force: ForceId, events: &mut Vec<CampaignEvent>) {
        let port = self.docked_port(force);
        let Some(f) = self.remove_force(force, port) else { return };
        self.tidy_embarked();
        let mut fallen: Vec<CharacterId> = f.commander.into_iter().collect();
        fallen.extend(f.units.iter().filter_map(|u| u.character).filter(|c| Some(*c) != f.commander));
        for c in fallen {
            self.character_falls(c);
        }
        events.push(CampaignEvent::ForceDestroyed { force });
    }

    /// The one path a force leaves the world: the regions' garrison, fleet and siege links to it go, and a navy docked in
    /// a port (`port`, [`Self::docked_port`] looked up by the caller while the force still stands on the map) hands
    /// the slot back to the region's owner, as the navy's destructor does (`0x0088EF90` → `0x00A8D830` on the
    /// navy's port handle +0xE0, CONFIRMED polish21; skipped there only while the campaign is torn down, campaign
    /// +0xFC5); then a port a navy blockades is freed ([`Self::lift_blockade_of_navy`], the same destructor's next
    /// step). Its repair orders are not refunded: the ships go with it.
    pub(crate) fn remove_force(&mut self, force: ForceId, port: Option<(RegionId, usize)>) -> Option<MilitaryForce> {
        let f = self.world.forces.remove(&force)?;
        if let Some((region, slot)) = port {
            self.restore_port_holder(region, slot);
        }
        if f.is_navy {
            self.lift_blockade_of_navy(force);
        }
        for r in self.world.regions.values_mut() {
            if r.garrison == Some(force) {
                r.garrison = None;
            }
            if r.fleet == Some(force) {
                r.fleet = None;
            }
        }
        // A besieger ends its siege (the destructor `0x0088CE30` calls `0x008BD550` at `0x0088CE93`
        // when force +0x94 is set, CONFIRMED), the price update with it; the siege's turns stay.
        self.end_force_siege(force);
        // Its entry in the risen armies goes too, keeping the rest in order, with its rebel government
        // (`World::region_risen_armies`, `World::rebel_army_governments`): the army's destructor
        // `0x0088CE30` frees its REBEL_DATA (+0x98), whose destructor `0x0088F310` unlinks it from
        // region +0x220 (`0x008B0150`: erase and shift down). CONFIRMED by decompile.
        self.world.region_risen_armies.retain(|_, armies| {
            armies.retain(|&a| a != force);
            !armies.is_empty()
        });
        self.world.rebel_army_governments.remove(&force);
        Some(f)
    }

    /// `faction` takes `region`'s settlement ([`Self::take_settlement`]) and the capture is reported
    /// (`SettlementOccupied`; the event's constructor `0x00889EE0` is called only by the capture
    /// variants `0x00B58560`, `0x00B58890` and `0x00B58C00`, CONFIRMED). This is the one place the
    /// capture's events are ordered: each variant builds and posts `SettlementOccupied` after its
    /// owner change returns (`0x00B449F0` at `0x00B586D9` / `0x00B5897A` / `0x00B58D2B`, event at
    /// `0x00B5874F` / `0x00B589A6` / `0x00B58DA0`), and a capture that ends the old owner fires
    /// `FactionDestroyed` inside that owner change (`0x00A64AC0` → `0x008AB6C0` at `0x00A652A2`,
    /// model event +0x108, whose broadcast `0x008DBAC0` calls every listener at once). So
    /// `FactionDestroyed` comes first, then `SettlementOccupied`, then the capture report
    /// ([`Self::settle_capture`]; the variants queue it before posting the event, but the choice it
    /// carries runs later): CONFIRMED, disassembly.
    pub(crate) fn occupy(
        &mut self,
        region: RegionId,
        faction: super::FactionId,
        by: Option<ForceId>,
        events: &mut Vec<CampaignEvent>,
    ) {
        if self.take_settlement(region, faction, by, events) {
            events.push(CampaignEvent::SettlementOccupied { region, faction });
        }
    }

    /// `faction` takes `region`'s settlement ([`Self::change_region_owner`]; the capture variants
    /// destroy the garrison army first); `by` (if any) moves inside. Reports nothing: [`Self::occupy`]
    /// reports a capture, the campaign director's transfer (`0x00B58A30`, [`super::director`]) none.
    /// Returns false when the region does not exist.
    ///
    /// Every capture variant (the settlement's virtuals `0x00B58560`, `0x00B58890` and `0x00B58C00`,
    /// at vtable slots `0x0137CA44` / `0x0137CA48` / `0x0137CA5C`, and `0x00B58A30`, the campaign
    /// director's region transfer) empties the queues itself before the owner changes (CONFIRMED,
    /// disassembly): the region's land queue (region +0x124) with no refund (`0x00B1A760(0)`), each
    /// port's naval queue (slot +0x1E8) with the refund (`0x00B1A760(1)`), every construction with no
    /// refund (`0x00A6CBE0(0)`). So the old owner gets its queued ships' cost back but not its land
    /// units' ([`Self::clear_region_queues`]); the owner change that follows finds the queues empty.
    ///
    /// Every one of them also sets the owner change's capture flag, and `TransferRegionToFaction`
    /// (`0x00A64AC0`) with it set, neither side the rebels (`0x00C3FE80`) and the owner a different
    /// faction, fires the diplomacy-changed event through `0x00B62120`
    /// ([`Self::diplomacy_changed`]; CONFIRMED). A deal's or a liberation's handover
    /// ([`Self::transfer_region`], `0x00B58A10`) leaves the flag clear and fires nothing.
    ///
    /// The owner change's first flag is set too (a capture, the director's transfer): an old owner
    /// left without a capital leaves the game ([`Self::end_if_capital_lost`], `0x00A64AC0`), its
    /// events in `events`.
    pub(crate) fn take_settlement(
        &mut self,
        region: RegionId,
        faction: super::FactionId,
        by: Option<ForceId>,
        events: &mut Vec<CampaignEvent>,
    ) -> bool {
        let Some(old) = self.world.regions.get(&region).map(|r| r.owner) else { return false };
        self.clear_region_queues(region, QueueRefund { land: false, naval: true });
        if !self.change_region_owner(region, faction) {
            return false;
        }
        // The capture variants end the captor's own siege after the owner change (`0x00B586E9`,
        // `0x00B5898A`, `0x00B58D3B`, each when the force's siege object +0x94 is set; CONFIRMED). The
        // owner change's listener has ended it already ([`Self::end_sieges_after_owner_change`]).
        if let Some(f) = by {
            self.end_force_siege(f);
        }
        if old != faction && !self.is_rebel_faction(old) && !self.is_rebel_faction(faction) {
            // `0x00B62120` refreshes both sides' zones first: the new owner, then the old.
            self.refresh_faction_zones(faction);
            self.refresh_faction_zones(old);
            self.diplomacy_changed();
        }
        let Some(r) = self.world.regions.get_mut(&region) else { return false };
        // The capture variants destroy the garrison army first; the settlement's links go with it.
        // PROVISIONAL: the settlement's residents (characters in no force, `World::settlement_residents`)
        // stay inside under the new owner, so a foreign gentleman goes on counting in its public
        // order; what the capture variants (`0x00B58560` / `0x00B58C00`) do to the residence's list
        // +0xC8 is not traced.
        r.garrison = None;
        r.fleet = None;
        let pos = r.settlement.position;
        if let Some(c) = by.and_then(|f| self.world.forces.get(&f)).and_then(|f| f.commander)
            && self.world.characters.contains_key(&c)
        {
            self.move_character(c, pos);
            self.world.characters.get_mut(&c).expect("checked").garrisoned_in = Some(region);
            // He holds the settlement now (`0x00B4E760` → `0x00A285C0`): his force's zones again, once.
            self.rebuild_moved_zones(c);
        }
        if let Some(f) = by.filter(|f| self.world.forces.get(f).is_some_and(|f| !f.is_navy && f.commander.is_some()))
            && let Some(r) = self.world.regions.get_mut(&region)
        {
            r.garrison = Some(f);
        }
        if old != faction {
            self.end_if_capital_lost(old, events);
        }
        true
    }

    /// A region handed over without a capturing force: the deal's region item (`0x00C18BF0` →
    /// `0x00B58A10(faction, 1, 0)` → `TransferSettlementOwnership` `0x00B449F0` with a NULL force
    /// and no capture report). The owner change itself is [`Self::change_region_owner`], the one rule
    /// capture, liberation and deals share. A region `faction` already owns still goes through it
    /// (CONFIRMED: `0x00B449F0` tests the owner only against the faction (+0x34) of a passed force's
    /// `REBEL_DATA` (+0x98, a risen rebel army's), `0x00B44A3A`..`0x00B44A67`, and this path passes
    /// no force), which then does only its same-owner
    /// part. Of this path's callers only a deal can name such a region: nothing checks a deal's region
    /// owners ([`Self::propose_regions`]), a liberation's target is out of the game, and the
    /// destruction hand-over (`0x008AB6C0`, [`Self::destroy_faction_by`]) refuses a killer that is the
    /// destroyed faction (the exe's loop never ends then; CAMPAIGN_FIDELITY.md "Same-owner transfer").
    /// Neither the old owner's end (a deal's caller runs [`Self::end_if_capital_lost`] for its first
    /// flag) nor a liberated faction's return ([`Self::liberate_region`]) happens here. Returns true
    /// only when the region changed hands.
    pub(crate) fn transfer_region(&mut self, region: RegionId, faction: super::FactionId) -> bool {
        let Some(old) = self.world.regions.get(&region).map(|r| r.owner) else { return false };
        self.change_region_owner(region, faction) && old != faction
    }

    /// What every change of a region's owner does (`TransferRegionToFaction` `0x00A64AC0` and the
    /// slots' `0x00B1B300`, reached from the capture variants and the deal alike): research at the
    /// region's schools stops (`0x008B4AB0` → `0x008B3720`: the technology's researcher cleared,
    /// its progress kept; its second loop gives each character listed in a slot, slot +0xD8, to
    /// the old owner's `0x008B3580`, the same call the completion `0x008EED20` makes for a school's
    /// characters; the model keeps no per-character research link — gentlemen count by position,
    /// `gentlemen_in` — so there is nothing to clear), the queues are cleared, the slots and the governorship pass to the new
    /// owner. The capitals move with it ([`Self::capital_after_owner_change`]). Then, as
    /// `0x00A64AC0` with its fourth argument 0 does (every `0x00B449F0` handover passes 0: captures,
    /// deals, liberations, the director's transfer; CONFIRMED), the other factions' land armies in
    /// the region leave it, and their docked navies when not at war with the new owner
    /// ([`Self::expel_after_owner_change`], `0x008D9DC0`): after a deal or a liberation the old
    /// owner's garrison is put out, as the capturer's army is after a liberation. Nothing is
    /// destroyed here (the settlement's owner-changed event, +0x24 fired at `0x00B44B31`, has on the
    /// campaign side only the siege listener `0x008DB7D0`, which ends sieges and destroys no force:
    /// [`Self::end_sieges_after_owner_change`], in both branches). The trade
    /// price update (flag 0, [`Self::update_commodity_prices_with`]) runs after the route loss
    /// ([`Self::trade_routes_after_region_lost`]) and again when a port slot passes or the slot pass lifts a blockade.
    ///
    /// When `faction` already owns the region ([`Self::transfer_region`]) only the research stop and
    /// the slot pass run (CONFIRMED, disassembly): `0x00A64AC0` stops the research (`0x008B4AB0` at
    /// `0x00A64CE5`) and then returns (`CMP` of the region's owner with the new one at `0x00A64D11`,
    /// `JZ` to the epilogue at `0x00A64D13`) before the queues, the capitals, the routes, the
    /// expulsion, the policing tracker and the taxes; `0x00B449F0` still calls `0x00B1B300`
    /// (`0x00B44B69`), so slots a third faction held come back and a port slot's price update runs.
    /// Returns false when the region does not exist.
    fn change_region_owner(&mut self, region: RegionId, faction: super::FactionId) -> bool {
        let Some(r) = self.world.regions.get(&region) else { return false };
        let old = r.owner;
        let schools: Vec<u32> = r.slots.iter().map(|s| s.id).filter(|&id| id != 0).collect();
        if let Some(d) = self.world.faction_details.get_mut(&old) {
            for t in d.research.values_mut().filter(|t| schools.contains(&t.researcher)) {
                t.researcher = 0;
            }
        }
        if old == faction {
            // The owner-changed event fires here too (`0x00B44B31`, before the slot pass `0x00B44B69`).
            self.end_sieges_after_owner_change(region);
            if self.pass_region_slots(region, old, faction) {
                self.update_commodity_prices_with(false);
            }
            return true;
        }
        // The queues are cleared while the region is still the old owner's (CONFIRMED, `0x00A64AC0`: the
        // constructions `0x00A6CC10` → `0x00A6CBE0(0)`, no refund; then the land queue `0x00B1A760(flag)` and
        // each port's naval queue `0x00A6CC40(flag)` → `0x00A6CBF0`). The flag is set when the old owner is not
        // human (faction +0x6E0 clear) or the caller passed the capture-report flag (`0x00B449F0`'s third
        // argument, set only by the capture variants, which have emptied the queues already: see `occupy`). So
        // a deal, a liberation or a scripted handover (`0x00B58A10` → report 0) refunds an AI old owner's queued
        // units, land and naval, and a human old owner's none.
        let refund = !self.turn.humans.contains(&old);
        self.clear_region_queues(region, QueueRefund { land: refund, naval: refund });
        let slot_prices_due = self.pass_region_slots(region, old, faction);
        let Some(r) = self.world.regions.get_mut(&region) else { return false };
        r.owner = faction;
        // It leaves the old owner's region list and joins the end of the new owner's (`0x008BA580`,
        // `0x008AE270`; `World::region_gains`).
        self.world.note_region_gain(region);
        // The region leaves every governorship and joins the new owner's first one, if it has one
        // (INFERRED; the governorship supplies the region's faction part, see `World::governing_faction`).
        for d in self.world.faction_details.values_mut() {
            for g in d.posts.iter_mut().filter_map(|p| p.governorship.as_mut()) {
                g.regions.retain(|x| *x != region);
            }
        }
        if let Some(g) = self.world.faction_details.get_mut(&faction).and_then(|d| d.posts.iter_mut().find_map(|p| p.governorship.as_mut())) {
            g.regions.push(region);
        }
        self.capital_after_owner_change(region, old, faction);
        // Then the trade routes through the region (`0x00BBE1A0`, called at `0x00A64E87` when the old
        // owner is not the rebels).
        if !self.is_rebel_faction(old) {
            self.trade_routes_after_region_lost(region, old);
        }
        self.expel_after_owner_change(region);
        // The region's policing tracker follows the owner (`0x00AAF220` at `0x00A65103`), then both owners'
        // running economics records take their new taxes (`0x00BBD7C0(0)` at `0x00A6512D` for the new owner and
        // `0x00A6513E` for the old, CONFIRMED).
        super::economy::move_policing_tracker(self, region, old, faction);
        super::economy::refresh_running_income(self, faction, super::economy::RunningIncome::Taxes);
        super::economy::refresh_running_income(self, old, super::economy::RunningIncome::Taxes);
        // After `0x00A64AC0`, the settlement's owner-changed event (`0x00B44B31`): its sieges by the new
        // owner or by factions not at war with it end ([`Self::end_sieges_after_owner_change`]).
        self.end_sieges_after_owner_change(region);
        // Then the slot pass's price updates (`0x00B1B300` runs after `0x00A64AC0` in `0x00B449F0`).
        if slot_prices_due {
            self.update_commodity_prices_with(false);
        }
        true
    }

    /// The slot pass of an owner change (`0x00B1B300`, CONFIRMED): each town / port slot goes to `faction`
    /// (`0x00B2B810(new, 0)` sets the holder whoever held it before, a third faction too), except a slot with a
    /// garrison (slot virtual +0x1C, `0x00A64440`, the handle at slot +0xC0; not in the model) or a port with a
    /// navy docked (the port's +0x60, [`Self::docked_port`], whoever's navy: its holder stays, so a slot `old`
    /// held stays `old`'s). The road and the walls always pass. (`None` = the owner's.) The owner itself is not
    /// set here. Returns whether a port slot passed: `SetSettlementOwner` (`0x00B2B810`) then calls the trade
    /// price update with flag 0 (`PUSH 0x0` at `0x00B2B844`, the call `0x00BCB020` at `0x00B2B84F`) when the slot
    /// (region slot vtable +0x4C `0x004470B0`, the slot itself) creates port objects (+0x20C) and its slot type
    /// is a port (`IsPortSlotType 0x00A8B810`): the same two tests under which the slot's constructor `0x00A4D4F0`
    /// builds its recruitment manager, so exactly the [`RegionSlot::port`](super::RegionSlot::port) slots. The
    /// settlement's own `SetSettlementOwner` call before `0x00A64AC0` runs none (the settlement vtable
    /// `0x0137C9A4` +0x4C is `0x00461180`, null). The exe updates once per port slot passed; the caller's one
    /// update gives the same prices: nothing the update reads changes between those calls and flag 0 leaves the
    /// history alone. The same pass lifts the blockades of the ports the new owner now holds
    /// ([`Self::clear_blockades_after_slot_pass`]); each lift's own price update (`0x00A784E0` → `0x00BCB020(0)`)
    /// folds into the caller's one in the same way, so the return value is also true when a blockade was lifted.
    fn pass_region_slots(&mut self, region: RegionId, old: super::FactionId, faction: super::FactionId) -> bool {
        let docked: Vec<usize> = self.world.forces.keys().filter_map(|&x| self.docked_port(x)).filter(|&(r, _)| r == region).map(|(_, i)| i).collect();
        let Some(r) = self.world.regions.get_mut(&region) else { return false };
        let port_passed = r.slots.iter().enumerate().any(|(i, s)| s.port && !docked.contains(&i));
        for (i, s) in r.slots.iter_mut().enumerate() {
            let kept = s.holder.or(Some(old));
            s.holder = if docked.contains(&i) && kept != Some(faction) { kept } else { None };
        }
        r.road_holder = None;
        r.fortification_holder = None;
        let lifted = self.clear_blockades_after_slot_pass(region, faction);
        port_passed || lifted
    }

    /// Empties `region`'s queues as an owner change does (`0x00B1A760` on the land queue and on each
    /// port's naval queue, `0x00A6CBE0(0)` on each slot's construction). Each recruitment item leaves
    /// through the cancel path ([`Self::cancelled_recruitment_items`]) with the flag of its queue; the
    /// refund goes to the region's owner, still the old one. Constructions are never refunded here
    /// (`0x00B1A790` credits only with its flag set). Land and naval items share
    /// [`super::world::Region::recruitment_queue`]; a ship's item is the port's ([`super::rules::CampaignRules::is_naval_unit`]).
    fn clear_region_queues(&mut self, region: RegionId, refund: QueueRefund) {
        let Some(r) = self.world.regions.get_mut(&region) else { return };
        let items = std::mem::take(&mut r.recruitment_queue);
        r.construction.clear();
        self.cancelled_recruitment_items(region, &items, |rules, i| if rules.is_naval_unit(&i.unit_key) { refund.naval } else { refund.land });
    }
}

/// Which of a region's recruitment queues an owner change refunds ([`CampaignModel::clear_region_queues`]):
/// the `0x00B1A760` flag of the land queue and of the ports' naval queues.
#[derive(Debug, Clone, Copy)]
struct QueueRefund {
    land: bool,
    naval: bool,
}

impl CampaignModel {
    /// A ship's state: the saved one, or a new ship of its type (full crews; guns unknown: 0).
    pub fn ship_state(&self, u: &super::world::CampaignUnit) -> super::naval::ShipState {
        if let Some(s) = self.world.ship_states.get(&u.id) {
            return *s;
        }
        let crews = self.rules.ships.get(&u.unit_key).map_or([0, 0, u.men as i32], |r| r.crews);
        super::naval::ShipState { crews, max_crews: crews, ..Default::default() }
    }

    /// The naval autoresolve units of some navies, in navy and ship order (`force`: the navy's index in
    /// `forces`). A ship's star rating is its naval battle card's rank ([`Self::battle_general_card`], CONFIRMED:
    /// `0x007CA330` passes naval card `+0xE8` at `0x007CA465` to `0x0070D580`, which stores it at resolver unit `+0xAC`).
    pub(super) fn naval_autoresolve_units(&self, forces: &[ForceId]) -> Vec<(ForceId, usize, super::naval::NavalUnit)> {
        self.resolver_units(forces, |k, f, u, star_rating| {
            let rules = self.rules.ships.get(&u.unit_key).copied().unwrap_or_default();
            let human = self.turn.humans.contains(&f.faction);
            super::naval::NavalUnit { state: self.ship_state(u), rules, human, force: k, star_rating }
        })
    }

    /// The naval autoresolve ([`super::naval`]): the attacker's navy against the defending navies. The ships'
    /// states and men (the crew total) are written back; sunk ships and ships without crew are removed; a navy
    /// left without ships is destroyed. Then the rest is settled as [`Self::apply_battle_result`] does, its undock run
    /// once here, before the captures ([`Self::undock_after_battle`]).
    pub(crate) fn autoresolve_naval(&mut self, battle: &PendingBattle) -> Vec<CampaignEvent> {
        use super::naval::{resolve, NavalUnit, NavalVars};
        let a = self.naval_autoresolve_units(&[battle.attacker]);
        let b = self.naval_autoresolve_units(&battle.defenders);
        let mut ua: Vec<NavalUnit> = a.iter().map(|x| x.2).collect();
        let mut ub: Vec<NavalUnit> = b.iter().map(|x| x.2).collect();
        let mut vars = NavalVars::from_rules(&self.rules);
        let human = a.iter().chain(&b).find(|x| x.2.human).and_then(|x| self.world.forces.get(&x.0)).map(|f| f.faction);
        vars.land.difficulty = human.and_then(|f| self.world.faction_details.get(&f)).map_or(0, |d| d.difficulty);
        let o = resolve(&mut ua, &mut ub, &vars, &mut self.rng);
        let mut events = Vec::new();
        let mut emptied = Vec::new();
        for ((fid, i, _), u) in a.iter().zip(&ua).chain(b.iter().zip(&ub)) {
            let Some(unit) = self.world.forces.get_mut(fid).and_then(|f| f.units.get_mut(*i)) else { continue };
            unit.men = if u.state.sunk { 0 } else { u.state.crew() };
            let id = unit.id;
            let mut state = u.state;
            // A ship lost (sunk or without crew, removed below) takes its repair order with it: no refund.
            state.repairing &= unit.men > 0;
            self.world.ship_states.insert(id, state);
        }
        // The post-battle undock, before any ship changes hands (PROVISIONAL order, see the function).
        self.undock_after_battle(battle);
        // Captured ships join the captor's force (`0x007CD570` marks the card with its captor; CAMPAIGN_FIDELITY.md
        // §Naval autoresolve). The captain attached to the ship, if any, stays behind.
        let (losers, winners): (&[(ForceId, usize, NavalUnit)], Vec<ForceId>) =
            if o.a_won { (&b, vec![battle.attacker]) } else { (&a, battle.defenders.clone()) };
        let mut taken: Vec<(ForceId, usize, ForceId)> = o.captured.iter().filter_map(|&(i, f)| Some((losers.get(i)?.0, losers[i].1, *winners.get(f)?))).collect();
        // Remove from the back so the indexes stay valid.
        taken.sort_by_key(|x| std::cmp::Reverse((x.0, x.1)));
        for (from, i, to) in taken {
            let Some(mut unit) = self.world.forces.get_mut(&from).filter(|f| i < f.units.len()).map(|f| f.units.remove(i)) else { continue };
            unit.character = None;
            if let Some(f) = self.world.forces.get_mut(&to) {
                f.units.push(unit);
            }
        }
        for fid in a.iter().chain(&b).map(|x| x.0).collect::<std::collections::BTreeSet<_>>() {
            let Some(f) = self.world.forces.get_mut(&fid) else { continue };
            let gone: Vec<_> = f.units.iter().filter(|u| u.men == 0).map(|u| (u.id, u.character)).collect();
            f.units.retain(|u| u.men > 0);
            if f.units.is_empty() {
                emptied.push(fid);
            }
            for (id, c) in gone {
                self.world.ship_states.remove(&id);
                if let Some(c) = c.filter(|_| !emptied.contains(&fid)) {
                    self.character_falls(c);
                }
            }
        }
        for fid in emptied {
            self.destroy_force(fid, &mut events);
        }
        self.pending_battle = None;
        let mut rest = self.settle_battle(battle, BattleResult { attacker_won: o.a_won, attacker_casualties: 0.0, defender_casualties: 0.0 }, false);
        events.append(&mut rest);
        events
    }
}

/// A battle unit card's commander role: the land card's `+0xC0` value, which the campaign's card builders
/// (land `0x008AF200`, naval `0x008AEF40`, CONFIRMED) set from the unit's character through
/// `0x008B6AC0` (CONFIRMED listing; BATTLE_FIDELITY.md §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleCommandRole {
    /// 0: no character on the unit, or a character of another kind (colonel, captain, ...) in a force
    /// someone else commands.
    None,
    /// 1 (land) / 4 (naval): a General or admiral who commands the unit's force, or is in none.
    Commander,
    /// 2 (land) / 5 (naval): a character of another kind who commands the unit's force, or is in none
    /// (a colonel or captain whose unit was raised alone).
    OtherCommander,
    /// 3 (land) / 6 (naval): a General or admiral in a force someone else commands.
    Subordinate,
}

impl BattleCommandRole {
    /// The card `+0xC0` value: land 0/1/2/3, naval 0/4/5/6 (the two builders' maps of the
    /// `0x008B6AC0` result, CONFIRMED).
    pub fn card_value(self, navy: bool) -> u8 {
        match (self, navy) {
            (Self::None, _) => 0,
            (Self::Commander, false) => 1,
            (Self::OtherCommander, false) => 2,
            (Self::Subordinate, false) => 3,
            (Self::Commander, true) => 4,
            (Self::OtherCommander, true) => 5,
            (Self::Subordinate, true) => 6,
        }
    }

    /// The card makes its unit its army's general: the battle's army setup `0x00505B00` takes the
    /// cards whose `+0xC0` is 1, 2, 4 or 5 (CONFIRMED), so a subordinate General is not one.
    pub fn is_army_general(self) -> bool {
        matches!(self, Self::Commander | Self::OtherCommander)
    }
}

/// The general part of a battle unit card built from a campaign unit (CONFIRMED: `0x008AF200` /
/// `0x008AEF40` fill the record `0x00513A10` (+0x20 rank, +0x24 role, +0x28 category), which the land
/// card `0x00513320` copies to `+0xBC`/`+0xC0`/`+0xC4` and the naval card `0x00512E50` to
/// `+0xE8`/`+0xEC`/`+0xF0`): what the battle's morale reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleGeneralCard {
    /// Land card `+0xC0`.
    pub role: BattleCommandRole,
    /// Land card `+0xBC`: the unit character's rank ([`super::agents::rank`], `0x00A198D0`) when the card
    /// is built, at battle start; 0 without a character (the default record `0x00513A50`).
    pub rank: i32,
}

impl BattleGeneralCard {
    /// The battle unit's general rank (`LandUnit::general_rank`): the rank when the card makes its
    /// unit the army's general, else none.
    pub fn army_general_rank(self) -> Option<i32> {
        self.role.is_army_general().then_some(self.rank)
    }
}

impl CampaignModel {
    /// The battle card general record of `unit` of `force` (`0x008AF200` land / `0x008AEF40`
    /// naval, CONFIRMED). With the unit's character `c`:
    /// - rank: `GetCharacterRank` (`0x00A198D0`), here the model's one rank rule [`super::agents::rank`];
    /// - role (`0x008B6AC0`): when `force` has a commander other than `c`, a General or admiral is a
    ///   [`BattleCommandRole::Subordinate`] and any other kind gets no role; otherwise (he commands
    ///   it, or it has no commander) a General or admiral is the [`BattleCommandRole::Commander`] and
    ///   any other kind the [`BattleCommandRole::OtherCommander`].
    ///
    /// Without a character (or one the model does not hold), the default record: no role, rank 0.
    /// The card's third value (`+0xC4`: 1 for a character with `CHARACTER` #34, 2 when he is also
    /// Napoléon Bonaparte, `0x00A0B980`) is not read by any battle morale and is not modelled.
    pub fn battle_general_card(&self, force: &MilitaryForce, unit: &super::world::CampaignUnit) -> BattleGeneralCard {
        use super::world::CharacterKind;
        let Some((c, ch)) = unit.character.and_then(|c| self.world.characters.get(&c).map(|ch| (c, ch))) else {
            return BattleGeneralCard { role: BattleCommandRole::None, rank: 0 };
        };
        // The exe's agent kinds 0 and 1 (`0x00F9DB50`'s key table: "General", "admiral").
        let commander_kind = matches!(ch.kind, CharacterKind::General | CharacterKind::Admiral);
        // The force leader handle (force +0x74) counts only when it resolves to a character.
        let led_by_other = force.commander.is_some_and(|l| l != c && self.world.characters.contains_key(&l));
        let role = match (led_by_other, commander_kind) {
            (true, true) => BattleCommandRole::Subordinate,
            (true, false) => BattleCommandRole::None,
            (false, true) => BattleCommandRole::Commander,
            (false, false) => BattleCommandRole::OtherCommander,
        };
        BattleGeneralCard { role, rank: super::agents::rank(self, c) }
    }
}

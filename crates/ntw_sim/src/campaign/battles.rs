//! Campaign battles: a pending battle, autoresolve, and applying a result.
//!
//! An attack (or an assault on a defended settlement) sets [`CampaignModel::pending_battle`].
//! The battle is then settled in one of two ways:
//! - **Autoresolve** ([`CampaignModel::autoresolve`], command `Autoresolve`): ports the original's
//!   stat-based resolver ([`super::autoresolve`]) with the campaign RNG; it sets every unit's men,
//!   then [`CampaignModel::apply_battle_result`] settles the rest with zero extra casualties.
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
use super::world::CampaignModel;
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
    /// the force's commander; star ratings are not modelled (0).
    fn autoresolve_units(&self, forces: &[ForceId]) -> Vec<(ForceId, usize, ArUnit)> {
        let mut out = Vec::new();
        for &fid in forces {
            let Some(f) = self.world.forces.get(&fid) else { continue };
            for (i, u) in f.units.iter().enumerate() {
                let data = self.rules.units.get(&u.unit_key).and_then(|r| r.autoresolve).unwrap_or_default();
                let has_general = u.character.is_some() && u.character == f.commander;
                out.push((fid, i, ArUnit { data, men: u.men, has_general, general_rank: 0, human: self.turn.humans.contains(&f.faction) }));
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
    /// wound +0x512 cleared) is linked to his force when it still has units (`0x008B02E0`,
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
        let Some(f) = self.world.forces.remove(&force) else { return };
        self.tidy_embarked();
        let mut fallen: Vec<CharacterId> = f.commander.into_iter().collect();
        fallen.extend(f.units.iter().filter_map(|u| u.character).filter(|c| Some(*c) != f.commander));
        for c in fallen {
            self.character_falls(c);
        }
        for r in self.world.regions.values_mut() {
            if r.garrison == Some(force) {
                r.garrison = None;
            }
            if r.fleet == Some(force) {
                r.fleet = None;
            }
        }
        events.push(CampaignEvent::ForceDestroyed { force });
    }

    /// `faction` takes `region`'s settlement ([`Self::change_region_owner`]; the capture variants
    /// destroy the garrison army first); `by` (if any) moves inside.
    ///
    /// Every capture variant (the settlement's virtuals `0x00B58560` +0x44, `0x00B58890` +0x48 and
    /// `0x00B58C00` +0x5C, and `0x00B58A30`, the campaign director's region transfer) empties the
    /// queues itself before the owner changes (CONFIRMED, disassembly): the region's land queue
    /// (region +0x124) with no refund (`0x00B1A760(0)`), each port's naval queue (slot +0x1E8) with
    /// the refund (`0x00B1A760(1)`), every construction with no refund (`0x00A6CBE0(0)`). So the old
    /// owner gets its queued ships' cost back but not its land units' ([`Self::clear_region_queues`]);
    /// the owner change that follows finds the queues empty.
    pub(crate) fn occupy(
        &mut self,
        region: RegionId,
        faction: super::FactionId,
        by: Option<ForceId>,
        events: &mut Vec<CampaignEvent>,
    ) {
        self.clear_region_queues(region, QueueRefund { land: false, naval: true });
        if !self.change_region_owner(region, faction) {
            return;
        }
        let Some(r) = self.world.regions.get_mut(&region) else { return };
        // The capture variants destroy the garrison army first; the settlement's links go with it.
        r.garrison = None;
        r.fleet = None;
        let pos = r.settlement.position;
        if let Some(c) = by.and_then(|f| self.world.forces.get(&f)).and_then(|f| f.commander)
            && let Some(ch) = self.world.characters.get_mut(&c)
        {
            ch.position = pos;
            ch.garrisoned_in = Some(region);
        }
        if let Some(f) = by.filter(|f| self.world.forces.get(f).is_some_and(|f| !f.is_navy && f.commander.is_some()))
            && let Some(r) = self.world.regions.get_mut(&region)
        {
            r.garrison = Some(f);
        }
        events.push(CampaignEvent::SettlementOccupied { region, faction });
    }

    /// A region handed over without a character: the deal's region item (`0x00C18BF0` →
    /// `0x00B58A10(faction, 1, 0)` → `TransferSettlementOwnership` `0x00B449F0` with no character
    /// and no capture report). Nothing happens when `faction` already owns it (`0x00A64AC0` returns
    /// when the old owner is the new one). The owner change itself is [`Self::change_region_owner`],
    /// the one rule capture, liberation and deals share. Returns false when the region does not
    /// exist or did not change hands.
    pub(crate) fn transfer_region(&mut self, region: RegionId, faction: super::FactionId) -> bool {
        if self.world.regions.get(&region).is_none_or(|r| r.owner == faction) {
            return false;
        }
        self.change_region_owner(region, faction)
    }

    /// What every change of a region's owner does (`TransferRegionToFaction` `0x00A64AC0` and the
    /// slots' `0x00B1B300`, reached from the capture variants and the deal alike): research at the
    /// region's schools stops (`0x008B4AB0` → `0x008B3720`: the technology's researcher cleared,
    /// its progress kept; its second loop gives each character listed in a slot, slot +0xD8, to
    /// the old owner's `0x008B3580`, the same call the completion `0x008EED20` makes for a school's
    /// characters; the model keeps no per-character research link — gentlemen count by position,
    /// `gentlemen_in` — so there is nothing to clear), the queues are cleared, the slots and the governorship pass to the new
    /// owner. No army is moved or destroyed (CONFIRMED: `0x00B449F0` → `0x00B2B810` / `0x00A64AC0`
    /// / `0x00B1B300` move none, and the settlement's owner-changed event, +0x24 fired at
    /// `0x00B44B31`, has on the campaign side only the siege listener `0x008DB7D0`, sieges not
    /// being in the model): after a deal or a liberation the old owner's garrison stays inside,
    /// a foreign garrison (see `defenders_of`, `enter_settlement`, `recruit`). Returns false when
    /// the region does not exist.
    fn change_region_owner(&mut self, region: RegionId, faction: super::FactionId) -> bool {
        let Some(r) = self.world.regions.get_mut(&region) else { return false };
        let old = r.owner;
        let schools: Vec<u32> = r.slots.iter().map(|s| s.id).filter(|&id| id != 0).collect();
        if let Some(d) = self.world.faction_details.get_mut(&old) {
            for t in d.research.values_mut().filter(|t| schools.contains(&t.researcher)) {
                t.researcher = 0;
            }
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
        let Some(r) = self.world.regions.get_mut(&region) else { return false };
        // The slots the old owner held pass to the new one (INFERRED; slots held by a third faction stay).
        for s in r.slots.iter_mut().filter(|s| s.holder == Some(old)) {
            s.holder = None;
        }
        r.owner = faction;
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
        true
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
        let rules = std::sync::Arc::clone(&self.rules);
        self.cancelled_recruitment_items(region, &items, |i| if rules.is_naval_unit(&i.unit_key) { refund.naval } else { refund.land });
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

    /// The naval autoresolve ([`super::naval`]): the attacker's navy against the defending navies. The ships'
    /// states and men (the crew total) are written back; sunk ships and ships without crew are removed; a navy
    /// left without ships is destroyed. Then [`Self::apply_battle_result`] settles the rest.
    pub(crate) fn autoresolve_naval(&mut self, battle: &PendingBattle) -> Vec<CampaignEvent> {
        use super::naval::{resolve, NavalUnit, NavalVars};
        let collect = |m: &CampaignModel, forces: &[ForceId]| -> Vec<(ForceId, usize, NavalUnit)> {
            let mut out = Vec::new();
            for (k, &fid) in forces.iter().enumerate() {
                let Some(f) = m.world.forces.get(&fid) else { continue };
                for (i, u) in f.units.iter().enumerate() {
                    let rules = m.rules.ships.get(&u.unit_key).copied().unwrap_or_default();
                    out.push((fid, i, NavalUnit { state: m.ship_state(u), rules, human: m.turn.humans.contains(&f.faction), force: k }));
                }
            }
            out
        };
        let a = collect(self, &[battle.attacker]);
        let b = collect(self, &battle.defenders);
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
            self.world.ship_states.insert(id, u.state);
        }
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
        let mut rest = self.apply_battle_result(BattleResult { attacker_won: o.a_won, attacker_casualties: 0.0, defender_casualties: 0.0 });
        events.append(&mut rest);
        events
    }
}

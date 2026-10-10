//! The general and admiral recruitment pools (`FACTION` #75 `CHARACTER_RECRUITMENT_MANAGER`), read
//! from the exe (slot 0-G, CHARACTERS_FIDELITY.md §8; specs in our words).
//!
//! - A pool holds candidate characters of the faction (ordinary characters standing at the
//!   capital without a force) and a timer: the turn its next candidate appears (0 = idle).
//! - Per turn (`0x00A252B0`, pool slot 0x30): when the timer equals the turns elapsed, one candidate
//!   is created (`0x009DB1B0`, only while the pool is below its cap), and if the pool is still below
//!   the cap the timer restarts at turns elapsed + the refill time.
//! - Cap: variable `character_recruitment_pool_cap` (3, `0x00A17B50`). Refill time (`0x00A17AF0` /
//!   `0x00A17A90`): `character_recruitment_pool_refill_rate_general_2` (2) when the faction has
//!   `character_recruitment_general_refill_2` > 0, else `..._1` (3) with `..._refill_1`, else `..._0`
//!   (4); admirals the same with the admiral names (bonuses 122 / 123).
//! - Hiring (`0x00A1B8F0`): the candidate leaves the pool, the timer starts if idle (remaining turns
//!   0, `0x00A276E0`), his flags +0x520 / +0x521 are cleared (in the model: leaving the pool is what
//!   lets him gain traits); then he is placed (`0x00A164C0`): a new army led by him, with the unit
//!   `agent_culture_details` gives the `General` of the faction's culture (`0x008E27D0`); the cost
//!   is paid. Cost (`0x00A1BB20` + `0x00A1BBE0`, CONFIRMED): `character_recruitment_base_cost` (400)
//!   plus `character_recruitment_cost_per_command_star` (300) × his rank plus min(10, round(10 ×
//!   min(d, max) / max)) × 100, d = his distance to the capital, max =
//!   `character_recruitment_max_distance` (1000).

use super::ids::{CharacterId, FactionId, ForceId, UnitId};
use super::world::{CampaignModel, CampaignUnit, Character, CharacterKind, MilitaryForce};
use super::commands::CommandError;
use super::details::CharacterDetails;
use super::CampaignEvent;
use crate::fixed::Fixed20;

/// Which pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolKind {
    /// `GENERAL_RECRUITMENT`.
    General,
    /// `ADMIRAL_RECRUITMENT`.
    Admiral,
}

impl CampaignModel {
    /// The unit a general of `faction` leads: `agent_culture_details` #3 of the `General` row for
    /// the faction's culture (`rules.general_units`); `None` for a culture without one (`indian`,
    /// `tribal`). The exe reads it through the agent record's per-culture hash (`0x008E27D0`,
    /// `0x009CBC40`); the hire command and the army panel's recruitment-tab test both ask here.
    pub fn general_unit(&self, faction: FactionId) -> Option<&String> {
        let key = &self.world.factions.get(&faction)?.key;
        self.rules.general_units.get(self.rules.characters.culture(key))
    }

    fn pool(&self, f: FactionId, kind: PoolKind) -> Option<&(Vec<CharacterId>, u32)> {
        let d = self.world.faction_details.get(&f)?;
        Some(match kind {
            PoolKind::General => &d.general_pool,
            PoolKind::Admiral => &d.admiral_pool,
        })
    }

    fn pool_mut(&mut self, f: FactionId, kind: PoolKind) -> Option<&mut (Vec<CharacterId>, u32)> {
        let d = self.world.faction_details.get_mut(&f)?;
        Some(match kind {
            PoolKind::General => &mut d.general_pool,
            PoolKind::Admiral => &mut d.admiral_pool,
        })
    }

    /// The pool cap (`character_recruitment_pool_cap`).
    pub fn pool_cap(&self) -> usize {
        self.rules.var("character_recruitment_pool_cap", 3.0) as usize
    }

    /// The refill time of a faction's pool in turns (see the module docs).
    pub fn pool_refill_time(&self, f: FactionId, kind: PoolKind) -> u32 {
        let who = match kind {
            PoolKind::General => "general",
            PoolKind::Admiral => "admiral",
        };
        let fx = super::effects::Effects::faction_sum(self, f);
        let level = if fx.get(&format!("character_recruitment_{who}_refill_2")) > 0.0 {
            2
        } else if fx.get(&format!("character_recruitment_{who}_refill_1")) > 0.0 {
            1
        } else {
            0
        };
        let default = [4.0, 3.0, 2.0][level];
        self.rules.var(&format!("character_recruitment_pool_refill_rate_{who}_{level}"), default) as u32
    }

    /// The per-turn pool step of a faction (`0x00A252B0`, slot 12 of both pool classes), both pools.
    /// Returns the new candidates. CONFIRMED phase: the faction turn start `0x008F2620` calls the
    /// manager's step `0x00A25310` (the manager is faction +0x940, two pointers {general pool,
    /// admiral pool}) after the characters' turn starts, the hidden-flag pass and the spotting pass
    /// (`TurnStep::CharactersStart`).
    pub fn pool_tick(&mut self, f: FactionId) -> Vec<CharacterId> {
        let mut out = Vec::new();
        let elapsed = self.calendar.turns_elapsed;
        for kind in [PoolKind::General, PoolKind::Admiral] {
            let Some(&(ref ids, timer)) = self.pool(f, kind) else { continue };
            if timer != elapsed {
                continue;
            }
            if ids.len() < self.pool_cap()
                && let Some(c) = self.create_candidate(f, kind)
            {
                out.push(c);
            }
            let refill = self.pool_refill_time(f, kind);
            let cap = self.pool_cap();
            if let Some(p) = self.pool_mut(f, kind)
                && p.0.len() < cap
            {
                p.1 = elapsed + refill;
            }
        }
        out
    }

    /// The historical characters a pool may offer now (`0x008C4BB0` fires `HistoricalCharacters`;
    /// every handler of `export_historic_characters.lua` asks `CanGenerateHistoricalCharacter`
    /// (`0x0089B830`, CONFIRMED): the row's faction and agent type are the pool's, the current
    /// year lies in its years, and the key is not in the created list), in table order.
    pub fn due_historical(&self, f: FactionId, kind: PoolKind) -> Vec<usize> {
        let Some(fkey) = self.world.factions.get(&f).map(|x| x.key.as_str()) else { return Vec::new() };
        let want = match kind {
            PoolKind::General => "General",
            PoolKind::Admiral => "admiral",
        };
        let year = self.calendar.date.year as i32;
        self.rules
            .historical
            .iter()
            .enumerate()
            .filter(|(_, h)| h.faction == fkey && h.kind == want && h.years.0 <= year && year <= h.years.1)
            .filter(|(_, h)| self.world.historical_created.binary_search(&h.key).is_err())
            .map(|(i, _)| i)
            .collect()
    }

    /// Creates a candidate (`0x00A0DFC0`, CONFIRMED): one of the historical characters due for the
    /// faction and type when there is one (`0x008C4BB0`: `uniform_below(count)` on the campaign RNG
    /// picks among them, the key joins the created list, `0x0098F880` makes him), else a generic
    /// one (`0x0098F250`): a General or admiral of the faction standing at its capital without a
    /// force, aged 21 + min(19, ⌊next16 × 20 / 65535⌋) (campaign RNG, `0x00A05740`), born that many
    /// years ago, added to the pool. PROVISIONAL: a historical character's age is drawn the same
    /// way (his record has no birth year); a historical character is named like a
    /// generic one ([`Self::name_new_character`]), not with his own name, and has no portrait (the
    /// key is kept in `CharacterDetails::historical_key`); the appeared turn (+0x4EC, `CHARACTER`
    /// #24) is not kept.
    pub fn create_candidate(&mut self, f: FactionId, kind: PoolKind) -> Option<CharacterId> {
        let capital = self.world.faction_details.get(&f)?.capital?;
        let position = self.world.regions.get(&capital)?.settlement.position;
        let ckind = match kind {
            PoolKind::General => CharacterKind::General,
            PoolKind::Admiral => CharacterKind::Admiral,
        };
        let due = self.due_historical(f, kind);
        let historical = if due.is_empty() {
            None
        } else {
            let pick = self.rng.uniform_below(due.len() as u32) as usize;
            let key = self.rules.historical[due[pick]].key.clone();
            let at = self.world.historical_created.binary_search(&key).unwrap_or_else(|x| x);
            self.world.historical_created.insert(at, key.clone());
            Some(key)
        };
        let draw = self.rng.next16();
        let age = 21 + (draw * 20 / 65535).min(19) as i32;
        let mp = self.rules.agent_action_points.get(ckind.esf_name()).copied().unwrap_or(0);
        let id = CharacterId(self.world.alloc_id() as i32);
        self.world.characters.insert(
            id,
            Character { id, faction: f, kind: ckind, position, movement_points: mp, max_movement_points: mp, base_movement_points: mp, garrisoned_in: None },
        );
        let birth = crate::calendar::Date { year: (self.calendar.date.year as i32 - age).max(0) as u32, ..self.calendar.date };
        self.world.character_details.insert(id, CharacterDetails { birth: Some(birth), historical_key: historical, ..Default::default() });
        self.name_new_character(id);
        self.update_sight_radius(id);
        self.pool_mut(f, kind)?.0.push(id);
        Some(id)
    }

    /// The cost of hiring a candidate (see the module docs), or `None` if he is not a candidate.
    pub fn hire_cost(&self, c: CharacterId) -> Option<i32> {
        let at = self.world.characters.get(&c)?.position;
        self.hire_cost_at(c, at)
    }

    /// The cost of hiring a candidate into `force` (he is measured at the force's position).
    pub fn hire_cost_into(&self, c: CharacterId, force: ForceId) -> Option<i32> {
        let at = self.force_position(force)?;
        self.hire_cost_at(c, at)
    }

    /// The hire cost of a candidate standing at `at` (the original moves the candidate to the force
    /// he joins before `0x00A1BBE0` measures his distance to the capital).
    fn hire_cost_at(&self, c: CharacterId, at: (Fixed20, Fixed20)) -> Option<i32> {
        let ch = self.world.characters.get(&c)?;
        if !super::characters::is_pool_candidate(self, c) {
            return None;
        }
        Some(self.hire_cost_at_rank(ch.faction, at, super::agents::rank(self, c)))
    }

    /// Whether the player may hire a candidate into `force` (the interface's
    /// `CanRecruitCommander(force, is_navy)`, `army.lua:708`, CONFIRMED name and arity; the army /
    /// navy panel's Promote button is shown only when it answers true, `army.lua:1058`).
    ///
    /// PROVISIONAL as a reading of the exe's gate (its handler is not traced): the conditions are
    /// the model's own -- it is the faction's turn, the matching pool (general pool for an army,
    /// admiral pool for a navy) holds a candidate, and the treasury can pay for at least one of
    /// them ([`hire_cost_into`], the cost the hire charges).
    pub fn can_recruit_commander(&self, force: ForceId) -> bool {
        let Some(f) = self.world.forces.get(&force) else { return false };
        let kind = if f.is_navy { PoolKind::Admiral } else { PoolKind::General };
        let Some((candidates, _)) = self.pool(f.faction, kind) else { return false };
        if !self.may_act(f.faction) {
            return false;
        }
        let purse = self.world.factions.get(&f.faction).map_or(0, |x| x.treasury);
        candidates.iter().any(|&c| self.hire_cost_into(c, force).is_some_and(|cost| cost <= purse))
    }

    /// Whether the force may promote one of its units in the field right now (the interface's
    /// `CanPromoteUnit(unit card address)`, `army.lua:692`).
    ///
    /// The exe's own gate is the unit's slot 16 (`0x009E0AF0`) and the concrete value for a
    /// promotable unit is UNKNOWN (the base class tables hold a return-0 stub at slot +0x40,
    /// CHARACTERS_FIDELITY.md §12), so this answers the model's own conditions -- the same ones
    /// [`promote_unit`](Self::promote_unit) refuses on: the faction's turn, the faction's
    /// `promote_general_in_field` / `promote_admiral_at_sea` effect (INFERRED gate) and no
    /// General / admiral already in command (CONFIRMED).
    pub fn can_promote_unit(&self, force: ForceId, unit: usize) -> bool {
        let Some(f) = self.world.forces.get(&force) else { return false };
        if f.units.get(unit).is_none() || !self.may_act(f.faction) {
            return false;
        }
        let (new_kind, bonus) = if f.is_navy {
            (CharacterKind::Admiral, "promote_admiral_at_sea")
        } else {
            (CharacterKind::General, "promote_general_in_field")
        };
        if super::effects::Effects::faction_sum(self, f.faction).get(bonus) <= 0.0 {
            return false;
        }
        !f.commander.and_then(|c| self.world.characters.get(&c)).is_some_and(|c| c.kind == new_kind)
    }

    /// The treasury cost of promoting `unit` of `force` in the field: what
    /// [`promote_unit`](Self::promote_unit) charges, so the interface can show it before the
    /// player commits (the unit row's `PromotionCost`, CONFIRMED read by `army.lua:825`'s
    /// `SelectedUnitsPromotionCost`).
    ///
    /// A **naval** promotion is free -- **INFERRED** (static Ghidra trace, 0-G rounds 1-2; the
    /// decompile was kept only in the sandbox's ignored `target/tmp/gh/`, so it is not on record):
    /// the naval unit class's slot +0x44 reads as a return-0 stub, so `0x008E2260` would call the
    /// treasury spend `0x00BAF500` with 0. The probe's `EXEC-NAVAL` + `PAY amount=0` lines confirm
    /// or refute it in one sitting.
    ///
    /// **PROVISIONAL** value for a land force (the pool's hire formula), and **INFERRED to be the
    /// wrong shape**: the static trace (0-G rounds 1-3, decompiles not kept) reads the land class's
    /// slot +0x44 (`0x008E2770`) as one value looked up per (agent type, culture or subculture)
    /// through `0x008E27D0` / the string hash `0x00F9C2A0`, `-1` with no record, with **no rank
    /// and no distance input**. Which object owns that hash, and so which record the price is
    /// read off, is UNKNOWN (the round-2 and round-3 notes disagree -- a character object vs an
    /// `AGENT_RECORD` row -- and the round-3 row reader list has a byte write at `+0x40`, the
    /// offset the hash is said to keep its bucket array at).
    ///
    /// Why it needs the running game (an honest "not found statically", not a proof): the two
    /// shipped tables on the (agent type, culture) axis, `db\agents_tables\agents` and
    /// `db\agent_culture_details_tables\agent_culture_details`, were read on the install and hold
    /// strings only (CONFIRMED shipped data, 0-G round 3), and no static writer of the hash was
    /// found. The probe is the way in: `analysis/fidelity/debugger/0G_PROMOTION_PROBE_2026-10-05.md`,
    /// self-checked by `cargo test -p ntw_data --test probe_script` and
    /// `cargo test -p ntw_data --test probe_install -- --ignored`.
    ///
    /// The `units` column #7 `unknown_3c` as the price: **INFERRED unlikely**, not refuted. It
    /// tracks each unit's own recruitment cost (ratio 0.698..4.857 over 438 rows, CONFIRMED shipped
    /// data, `examples/promotion_price_check.rs`), which only rules it out if the price really is
    /// one number per (agent type, culture) -- itself the INFERRED reading above.
    pub fn promotion_cost(&self, force: ForceId, unit: usize) -> Option<i32> {
        let f = self.world.forces.get(&force)?;
        f.units.get(unit)?;
        if f.is_navy {
            return Some(0);
        }
        let at = self.force_position(force)?;
        // A unit without a character is given a fresh General (`0x00990EF0`), whose rank is the
        // model's -1 (no attributes yet) -- the same value the promotion itself charges.
        let rank = f.units[unit].character.map_or(-1, |c| super::agents::rank(self, c));
        Some(self.hire_cost_at_rank(f.faction, at, rank))
    }

    /// The pool's hire cost formula for a candidate of `faction` measured at `at` with `rank`
    /// (see the module docs). Shared by [`hire_cost_at`](Self::hire_cost_at) and
    /// [`promotion_cost`](Self::promotion_cost).
    fn hire_cost_at_rank(&self, faction: FactionId, at: (Fixed20, Fixed20), rank: i32) -> i32 {
        let base = self.rules.var("character_recruitment_base_cost", 400.0) as i32;
        let per_star = self.rules.var("character_recruitment_cost_per_command_star", 300.0) as i32;
        let max = self.rules.var("character_recruitment_max_distance", 1000.0);
        let capital = self.world.faction_details.get(&faction).and_then(|d| d.capital);
        let dist = capital.and_then(|r| self.world.regions.get(&r)).map_or(0.0, |r| {
            let (dx, dz) = (r.settlement.position.0.to_f32() - at.0.to_f32(), r.settlement.position.1.to_f32() - at.1.to_f32());
            (dx * dx + dz * dz).sqrt()
        });
        let part = ((dist.min(max) / max) * 10.0).round_ties_even().min(10.0) as i32 * 100;
        base + per_star * rank + part
    }

    /// Takes a candidate out of his pool (`0x00A1B8F0`, CONFIRMED: the timer starts if idle, his
    /// +0x520 / +0x521 flags are cleared) and pays `cost`.
    fn take_candidate(&mut self, c: CharacterId, kind: PoolKind, cost: i32) {
        let faction = self.world.characters[&c].faction;
        let elapsed = self.calendar.turns_elapsed;
        let refill = self.pool_refill_time(faction, kind);
        if let Some(p) = self.pool_mut(faction, kind) {
            p.0.retain(|&x| x != c);
            if p.1 <= elapsed {
                p.1 = elapsed + refill;
            }
        }
        if let Some(fx) = self.world.factions.get_mut(&faction) {
            fx.treasury -= cost;
        }
    }

    /// Hires a General from his faction's pool (`0x00A1B8F0` → `0x00A164C0`, CONFIRMED structure):
    /// pays the cost, takes him out of the pool and makes him a new army with his culture's general
    /// unit (`0x0087FB30`); with `into` (an army of his faction; the interface's "recruit general"
    /// is given on an army) that army is merged into it (`0x008D2FA0`) at the army's position, where
    /// the General takes command (the commander pick puts a General first), and the cost counts the
    /// distance from the capital to that army. Without a target (PROVISIONAL, no such path in the
    /// original) the army stands at the capital, inside the settlement when it has no garrison army.
    pub(crate) fn hire_general(&mut self, c: CharacterId, into: Option<ForceId>) -> Result<Vec<CampaignEvent>, CommandError> {
        let ch = self.world.characters.get(&c).cloned().ok_or(CommandError::UnknownCharacter(c))?;
        if !self.may_act(ch.faction) {
            return Err(CommandError::NotYourTurn(ch.faction));
        }
        if ch.kind != CharacterKind::General || !self.pool(ch.faction, PoolKind::General).is_some_and(|p| p.0.contains(&c)) {
            return Err(CommandError::Unsupported("not a General in his faction's recruitment pool"));
        }
        let target = match into {
            Some(f) => {
                let force = self.world.forces.get(&f).ok_or(CommandError::UnknownForce(f))?;
                if force.faction != ch.faction || force.is_navy {
                    return Err(CommandError::Unsupported("a General joins an army of his own faction"));
                }
                let cmd = force.commander.ok_or(CommandError::UnknownForce(f))?;
                let at = self.world.characters.get(&cmd).map(|x| (x.position, x.garrisoned_in)).ok_or(CommandError::UnknownCharacter(cmd))?;
                Some((f, at))
            }
            None => None,
        };
        let at = target.map_or(ch.position, |t| t.1 .0);
        let cost = self.hire_cost_at(c, at).expect("checked");
        let available = self.world.factions[&ch.faction].treasury;
        if available < cost {
            return Err(CommandError::InsufficientFunds { needed: cost, available });
        }
        let unit_key = self.general_unit(ch.faction).cloned().ok_or(CommandError::Unsupported("no general unit for this culture"))?;
        self.take_candidate(c, PoolKind::General, cost);
        let uid = UnitId(self.world.alloc_id() as i32);
        let men = self.rules.units.get(&unit_key).map_or(1, |u| u.men.max(1));
        let unit = CampaignUnit { id: uid, unit_key, men, max_men: men, character: Some(c), officer_name: Default::default() };
        let fid = match target {
            Some((f, (position, garrisoned_in))) => {
                if let Some(x) = self.world.characters.get_mut(&c) {
                    x.position = position;
                    x.garrisoned_in = garrisoned_in;
                }
                if let Some(force) = self.world.forces.get_mut(&f) {
                    force.units.push(unit);
                    force.commander = Some(c);
                }
                f
            }
            None => {
                let capital = self.world.faction_details.get(&ch.faction).and_then(|d| d.capital);
                let inside = capital.filter(|r| self.world.regions.get(r).is_some_and(|reg| reg.owner == ch.faction && reg.garrison.is_none_or(|g| !self.world.forces.contains_key(&g))));
                let fid = ForceId(self.world.alloc_id());
                self.world.forces.insert(fid, MilitaryForce { id: fid, faction: ch.faction, commander: Some(c), units: vec![unit], is_navy: false });
                if let Some(r) = inside {
                    if let Some(x) = self.world.characters.get_mut(&c) {
                        x.garrisoned_in = Some(r);
                    }
                    if let Some(reg) = self.world.regions.get_mut(&r) {
                        reg.garrison = Some(fid);
                    }
                }
                fid
            }
        };
        self.name_new_unit(fid, uid);
        self.update_hidden(c);
        Ok(vec![CampaignEvent::CharacterHired { character: c, force: fid, cost }])
    }

    /// Hires an admiral from his faction's pool onto `fleet`, a navy of his faction (`0x00A1B8F0` →
    /// `0x00A16110`, CONFIRMED structure): the cost is the General's formula measured at the fleet
    /// (`0x00A1BBE0` is the pool's shared cost slot), he moves to the fleet and takes command
    /// (`0x008EDE20`, `0x008D2FA0`); a captain commanding it is removed (`0x00A0C6F0(1)`).
    pub(crate) fn hire_admiral(&mut self, c: CharacterId, fleet: ForceId) -> Result<Vec<CampaignEvent>, CommandError> {
        let ch = self.world.characters.get(&c).cloned().ok_or(CommandError::UnknownCharacter(c))?;
        if !self.may_act(ch.faction) {
            return Err(CommandError::NotYourTurn(ch.faction));
        }
        if ch.kind != CharacterKind::Admiral || !self.pool(ch.faction, PoolKind::Admiral).is_some_and(|p| p.0.contains(&c)) {
            return Err(CommandError::Unsupported("not an admiral in his faction's recruitment pool"));
        }
        let force = self.world.forces.get(&fleet).ok_or(CommandError::UnknownForce(fleet))?;
        if force.faction != ch.faction || !force.is_navy {
            return Err(CommandError::Unsupported("an admiral joins a navy of his own faction"));
        }
        let old = force.commander.ok_or(CommandError::UnknownForce(fleet))?;
        let (position, garrisoned_in) = self.world.characters.get(&old).map(|x| (x.position, x.garrisoned_in)).ok_or(CommandError::UnknownCharacter(old))?;
        let cost = self.hire_cost_at(c, position).expect("checked");
        let available = self.world.factions[&ch.faction].treasury;
        if available < cost {
            return Err(CommandError::InsufficientFunds { needed: cost, available });
        }
        self.take_candidate(c, PoolKind::Admiral, cost);
        if let Some(x) = self.world.characters.get_mut(&c) {
            x.position = position;
            x.garrisoned_in = garrisoned_in;
        }
        if let Some(f) = self.world.forces.get_mut(&fleet) {
            f.commander = Some(c);
        }
        // A captain in command goes (he was made for the ship; a General or admiral riding along
        // would be another matter, UNKNOWN).
        if self.world.characters.get(&old).is_some_and(|x| x.kind == CharacterKind::Captain) {
            for u in self.world.forces.get_mut(&fleet).map(|f| f.units.iter_mut()).into_iter().flatten() {
                if u.character == Some(old) {
                    u.character = None;
                }
            }
            self.world.characters.remove(&old);
            self.world.character_details.remove(&old);
            self.world.sight_radius.remove(&old);
        }
        self.update_hidden(c);
        Ok(vec![CampaignEvent::CharacterHired { character: c, force: fleet, cost }])
    }

    /// Promotes a unit's commander in the field (the unit classes' slot 20: `0x008E1C20` land,
    /// `0x008E2260` naval; the player's `PromoteUnits` / `CCQ_PROMOTE_COMMANDER`, the console's
    /// `promote_unit_commander`, the AI's `CAI_BDI_PROMOTE_UNIT`; CONFIRMED structure): the unit's
    /// character gets the General (land) / admiral (naval) agent record through `0x00A1A2E0` →
    /// `0x00A1A300`, which copies the record's attributes and abilities, fires `CharacterPromoted`
    /// and re-sums his effects; a unit without a character gets a new one first (`0x00990EF0`).
    /// He then commands the force (`0x008CFBE0`). The faction needs the effect
    /// `promote_general_in_field` (army) / `promote_admiral_at_sea` (navy) > 0 (INFERRED from the
    /// bonus names; the interface's `CanPromoteUnit` asks the unit's slot 16, not decoded) and
    /// the force no General / admiral in command. PROVISIONAL: the cost (the interface shows a
    /// `PromotionCost`) is taken as the pool's hire formula for his rank and distance from the
    /// capital; the original's charge is not traced.
    pub(crate) fn promote_unit(&mut self, force: ForceId, unit: usize) -> Result<Vec<CampaignEvent>, CommandError> {
        let f = self.world.forces.get(&force).cloned().ok_or(CommandError::UnknownForce(force))?;
        if !self.may_act(f.faction) {
            return Err(CommandError::NotYourTurn(f.faction));
        }
        let u = f.units.get(unit).ok_or(CommandError::Unsupported("no such unit in the force"))?.clone();
        let (new_kind, bonus) = if f.is_navy { (CharacterKind::Admiral, "promote_admiral_at_sea") } else { (CharacterKind::General, "promote_general_in_field") };
        if super::effects::Effects::faction_sum(self, f.faction).get(bonus) <= 0.0 {
            return Err(CommandError::Unsupported("the faction cannot promote in the field"));
        }
        if f.commander.and_then(|c| self.world.characters.get(&c)).is_some_and(|c| c.kind == new_kind) {
            return Err(CommandError::Unsupported("the force already has a General or admiral"));
        }
        let at = self.force_position(force).ok_or(CommandError::UnknownForce(force))?;
        let garrisoned_in = f.commander.and_then(|c| self.world.characters.get(&c)).and_then(|c| c.garrisoned_in);
        let c = match u.character.filter(|c| self.world.characters.contains_key(c)) {
            Some(c) => c,
            None => {
                let mp = self.rules.agent_action_points.get(new_kind.esf_name()).copied().unwrap_or(0);
                let id = CharacterId(self.world.alloc_id() as i32);
                self.world.characters.insert(
                    id,
                    Character { id, faction: f.faction, kind: new_kind, position: at, movement_points: mp, max_movement_points: mp, base_movement_points: mp, garrisoned_in },
                );
                self.world.character_details.insert(id, CharacterDetails::default());
                if let Some(x) = self.world.forces.get_mut(&force).and_then(|x| x.units.get_mut(unit)) {
                    x.character = Some(id);
                }
                // He carries the unit's officer's name: land `0x008E1C20` through the unit's slot 1
                // `0x008B7EF0`, naval `0x008E2260` directly, both pass unit +0x7c to `0x00990EF0`.
                self.name_new_character(id);
                id
            }
        };
        // A naval promotion is free (INFERRED, static trace not kept: the naval class's slot +0x44 is a return-0 stub, so
        // `0x008E2260` pays `0x00BAF500(0, 2)`); a land one charges the pool hire formula
        // (PROVISIONAL, see `promotion_cost`).
        let cost = if f.is_navy { 0 } else { self.hire_cost_at_rank(f.faction, at, super::agents::rank(self, c)) };
        let available = self.world.factions[&f.faction].treasury;
        if available < cost {
            return Err(CommandError::InsufficientFunds { needed: cost, available });
        }
        if let Some(fx) = self.world.factions.get_mut(&f.faction) {
            fx.treasury -= cost;
        }
        if let Some(x) = self.world.characters.get_mut(&c) {
            x.kind = new_kind;
            let mp = self.rules.agent_action_points.get(new_kind.esf_name()).copied().unwrap_or(x.base_movement_points);
            x.base_movement_points = mp;
            x.max_movement_points = x.max_movement_points.max(mp);
        }
        if let Some(x) = self.world.forces.get_mut(&force) {
            x.commander = Some(c);
        }
        self.update_sight_radius(c);
        self.update_hidden(c);
        Ok(vec![CampaignEvent::CharacterPromoted { character: c }])
    }
}

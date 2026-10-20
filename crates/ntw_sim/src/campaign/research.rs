//! Technology research (CAMPAIGN_FIDELITY.md §Research).
//!
//! A school (a building whose own effects give `research_points` > 0, at full health, in a slot its
//! region's owner holds) researches one technology at a time (`0x008B33B0` / `0x008EEC90`). Each
//! research step adds the school's rate to the technology's progress (`0x008DD450`); at the cost the
//! technology is researched: state 0, progress = cost, researcher cleared (`0x008EED20`). Techs
//! become available (state 4 or 3 → 2) once their prerequisites are researched and the faction owns
//! a building of the tech's tree at its building level; they never go back (CONFIRMED writer
//! `0x008F91F0`, see [`CampaignModel::update_tech_availability`]).

use super::effects::{EffectSet, Effects};
use super::ids::{FactionId, RegionId};
use super::world::CampaignModel;
use super::CharacterKind;

/// Technology states of `FACTION_TECHNOLOGY_MANAGER` `techs[]` #1.
pub mod state {
    /// Researched (CONFIRMED: set on completion, 0x008EED20).
    pub const RESEARCHED: u32 = 0;
    /// Available to research (CONFIRMED: research may start in state 2 or 1, 0x008B33B0).
    pub const AVAILABLE: u32 = 2;
    /// Requirements met but a building level of its list +0x7C is missing (CONFIRMED: set by
    /// `0x008F91F0`; re-checked on every update like state 4).
    pub const BUILDING_MISSING: u32 = 3;
    /// Not yet available.
    pub const UNAVAILABLE: u32 = 4;
}

/// Why research cannot start (`0x008B33B0`'s codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchError {
    /// 1: no school there (no building giving `research_points`, or the slot is held by another faction).
    NotASchool,
    /// 2: another school is already researching it.
    AlreadyResearched,
    /// 3: the technology is not available (researched or not yet unlocked).
    NotAvailable,
    /// 4: the faction has no such technology.
    Unknown,
}

/// The inputs of a school's research rate (`0x008EA9F0`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResearchParts {
    /// Σ of the own gentlemen's effective research, capped by `character_research_points_cap`.
    pub gentlemen: f32,
    /// The int effect `research_points` (faction + building + gentlemen sets).
    pub points: i32,
    /// The thread effects: military, industry, enlightenment.
    pub threads: [i32; 3],
    /// The float effect `research_rate_mod`.
    pub rate_mod: f32,
}

impl ResearchParts {
    /// The rate for a technology of thread `thread`.
    pub fn rate(&self, thread: usize) -> f32 {
        let points = self.gentlemen + self.points as f32 + self.threads.get(thread).copied().unwrap_or(0) as f32;
        (self.rate_mod + 100.0) * points * 0.01
    }
}

impl CampaignModel {
    /// The school at `region`'s slot `slot`, if it is one: its owner, when the building's own effects
    /// give `research_points` > 0, it is at full health and the region's owner holds the slot.
    /// Public (read-only) for the campaign HUD's `CharacterInValidEnemyUniversity`.
    pub fn school(&self, region: RegionId, slot: usize) -> Option<FactionId> {
        let r = self.world.regions.get(&region)?;
        let s = r.slots.get(slot)?;
        let b = s.building.as_ref().filter(|b| !b.is_damaged())?;
        if r.slot_occupied(s) {
            return None;
        }
        let points = self.rules.effects.building_local().get(&b.level_key).map_or(0, |e| e.get_int("research_points"));
        (points > 0).then_some(r.owner)
    }

    /// The (region, slot) of the school with slot id `id`.
    pub fn school_slot(&self, id: u32) -> Option<(RegionId, usize)> {
        if id == 0 {
            return None;
        }
        self.world.regions.values().find_map(|r| r.slots.iter().position(|s| s.id == id).map(|i| (r.id, i)))
    }

    /// Research points a school adds to `tech` per research step (`0x008EA9F0`, CONFIRMED formula):
    /// `(100 + research_rate_mod) / 100 × (min(Σ gentlemen's research, character_research_points_cap) +
    /// research_points + thread effect)`. The effects are the owner's faction sum plus the school
    /// building's own set and the set of every own gentleman in the school; a gentleman's research is
    /// his effective attribute ([`Effects::character_attribute`]). 0 when it is no school.
    /// PROVISIONAL: the army / navy rate mods (`research_rate_mod_army_tech` / `_navy_tech`, added for
    /// military techs whose record names "army-admin" / "navy-admin") are not applied.
    /// The thread effect is the technology's [`super::rules::TechRules::thread`] (record +0x8C: 0
    /// `research_points_military`, 1 `research_points_industry`, 2 `research_points_enlightenment`,
    /// 3 none; CONFIRMED `0x008EA9F0`).
    pub fn research_rate(&self, region: RegionId, slot: usize, tech: &str) -> f32 {
        let thread = self.rules.technologies.get(tech).map_or(super::rules::NO_THREAD as usize, |t| t.thread);
        self.research_parts(region, slot).map_or(0.0, |p| p.rate(thread))
    }

    /// The inputs of [`Self::research_rate`] at a school (`None` when it is no school).
    pub fn research_parts(&self, region: RegionId, slot: usize) -> Option<ResearchParts> {
        let owner = self.school(region, slot)?;
        let s = &self.world.regions[&region].slots[slot];
        let mut set: EffectSet = Effects::faction_sum(self, owner);
        if let Some(local) = s.building.as_ref().and_then(|b| self.rules.effects.building_local().get(&b.level_key)) {
            set.merge(local);
        }
        let mut gentlemen = 0i32;
        for c in self.gentlemen_in(region, slot) {
            // The saved level + 1 (INFERRED from the vanilla saves: every AI school with a gentleman needs
            // one point more than his saved `research`; the saved level already holds his trait bonuses,
            // e.g. Academic Honours: base 2 + 1 = 3 saved). The +1 is the exe's attribute bonus array
            // (character +0x394, `0x009C7610`), whose source is not traced.
            let saved = self.world.character_details.get(&c).and_then(|d| d.attributes.iter().find(|(k, _)| k == "research")).map_or(-1, |(_, v)| *v);
            let skill = if saved >= 0 { saved + 1 } else { 0 };
            if skill > 0 {
                gentlemen += skill;
            }
            set.merge(&Effects::character_effects(self, c));
        }
        let cap = self.rules.var("character_research_points_cap", f32::MAX);
        Some(ResearchParts {
            gentlemen: (gentlemen as f32).min(cap),
            points: set.get_int("research_points"),
            threads: [set.get_int("research_points_military"), set.get_int("research_points_industry"), set.get_int("research_points_enlightenment")],
            rate_mod: set.get("research_rate_mod"),
        })
    }

    /// The owner's gentlemen in a school slot (standing at the slot's position, the residence's list).
    pub fn gentlemen_in(&self, region: RegionId, slot: usize) -> Vec<super::CharacterId> {
        let Some(r) = self.world.regions.get(&region) else { return Vec::new() };
        let Some(pos) = r.slots.get(slot).and_then(|s| s.position) else { return Vec::new() };
        self.world
            .characters
            .values()
            .filter(|c| c.kind == CharacterKind::Gentleman && c.faction == r.owner && c.position == pos)
            .map(|c| c.id)
            .collect()
    }

    /// A technology's state for a faction (`None` if the faction does not have it).
    pub fn tech_state(&self, faction: FactionId, tech: &str) -> Option<u32> {
        self.world.faction_details.get(&faction)?.technologies.iter().find(|(k, _)| k == tech).map(|(_, s)| *s)
    }

    /// Starts researching `tech` at a school (`0x008EEC90` after `0x008B33B0`'s checks). A school
    /// already researching another technology drops it (that progress is kept).
    pub fn start_research(&mut self, region: RegionId, slot: usize, tech: &str) -> Result<(), ResearchError> {
        let owner = self.school(region, slot).ok_or(ResearchError::NotASchool)?;
        let st = self.tech_state(owner, tech).ok_or(ResearchError::Unknown)?;
        if st != state::AVAILABLE && st != 1 {
            return Err(ResearchError::NotAvailable);
        }
        let id = self.world.regions[&region].slots[slot].id;
        let d = self.world.faction_details.get_mut(&owner).ok_or(ResearchError::Unknown)?;
        if d.research.get(tech).is_some_and(|t| t.researcher != 0 && t.researcher != id) {
            return Err(ResearchError::AlreadyResearched);
        }
        for t in d.research.values_mut().filter(|t| t.researcher == id) {
            t.researcher = 0;
        }
        d.research.entry(tech.to_string()).or_default().researcher = id;
        Ok(())
    }

    /// The technology a school is researching, if any.
    pub fn researching_at(&self, region: RegionId, slot: usize) -> Option<String> {
        let r = self.world.regions.get(&region)?;
        let id = r.slots.get(slot)?.id;
        let d = self.world.faction_details.get(&r.owner)?;
        d.research.iter().find(|(_, t)| id != 0 && t.researcher == id).map(|(k, _)| k.clone())
    }

    /// One research step for a faction (`0x008DD450`): every technology with a working school gains its
    /// rate; at the cost it is researched (`0x008EED20`). Then newly available techs open up.
    /// Returns the technologies researched this step.
    pub fn research_step(&mut self, faction: FactionId) -> Vec<String> {
        let Some(d) = self.world.faction_details.get(&faction) else { return Vec::new() };
        let gains: Vec<(String, f32, RegionId, usize)> = d
            .research
            .iter()
            .filter(|(_, t)| t.researcher != 0)
            .filter_map(|(k, t)| {
                let (region, slot) = self.school_slot(t.researcher)?;
                Some((k.clone(), self.research_rate(region, slot, k), region, slot))
            })
            .collect();
        let mut done = Vec::new();
        for (k, gain, region, slot) in gains {
            let cost = self.rules.technologies.get(&k).map_or(f32::MAX, |t| t.cost as f32);
            let Some(d) = self.world.faction_details.get_mut(&faction) else { break };
            let Some(t) = d.research.get_mut(&k) else { continue };
            t.progress += gain;
            if t.progress >= cost {
                self.complete_technology(faction, &k);
                done.push(k);
                continue;
            }
            // A school still researching: foreign gentlemen there may steal the technology
            // (`super::agents`, CHARACTERS_FIDELITY.md §7).
            self.steal_step(region, slot, &k);
        }
        self.update_tech_availability(faction);
        done
    }

    /// A technology is researched (`0x008EED20`): progress = its cost, no school researching it,
    /// state 0. The traded count stays.
    fn complete_technology(&mut self, faction: FactionId, tech: &str) {
        let cost = self.rules.technologies.get(tech).map_or(0.0, |t| t.cost as f32);
        let Some(d) = self.world.faction_details.get_mut(&faction) else { return };
        let t = d.research.entry(tech.to_string()).or_default();
        t.progress = cost;
        t.researcher = 0;
        if let Some(s) = d.technologies.iter_mut().find(|(x, _)| x == tech) {
            s.1 = state::RESEARCHED;
        }
    }

    /// A technology handed over in a deal (`GrantFactionTechnology` `0x008CDCB0(tech, 0, 1, 0)`,
    /// from the deal's `0x00C18CF0`, CONFIRMED): while the faction has the technology at state 1..4 it
    /// is researched (`0x008EED20`) and the walk moves on to its single requirement
    /// ([`super::rules::TechRules::parent`], record +0x60), stopping at none or one the faction has no
    /// entry for. Then availability is always updated (`0x008F91F0`); the technology effects follow
    /// from the states (the exe rebuilds its sum, `0x008E49C0`). A technology already researched, or
    /// one the faction has no entry for, researches nothing. Returns true when `tech` itself was
    /// researched by this. For the deal's technologies the walk stops at once: they are at state
    /// 1..3 for the receiver ([`CampaignModel::tradeable_technologies`]), which the availability
    /// rule only allows once the parent is researched (UI_FIDELITY.md §4.9).
    pub fn grant_technology(&mut self, faction: FactionId, tech: &str) -> bool {
        let mut next = Some(tech.to_string());
        let mut granted = false;
        while let Some(t) = next.take() {
            if !self.tech_state(faction, &t).is_some_and(|s| (1..=4).contains(&s)) {
                break;
            }
            self.complete_technology(faction, &t);
            granted |= t == tech;
            next = self.rules.technologies.get(&t).and_then(|r| r.parent.clone());
        }
        self.update_tech_availability(faction);
        granted
    }

    /// The giver's count of how often it handed `tech` over in a deal goes up by one (technology
    /// entry +0x28, `0x008F3DD0`; nothing when the faction has no entry for the technology). The
    /// count is saved ([`super::details::TechResearch::traded`]) and read by the AI's deal value (`0x00A36B20`, §6).
    pub(crate) fn count_technology_traded(&mut self, faction: FactionId, tech: &str) {
        if self.tech_state(faction, tech).is_none() {
            return;
        }
        if let Some(d) = self.world.faction_details.get_mut(&faction) {
            d.research.entry(tech.to_string()).or_default().traded += 1;
        }
    }

    /// State 4 or 3 → 2 (or 3) for every technology whose prerequisites are met (`0x008F91F0`,
    /// CONFIRMED). For each tech in state 4 or 3, in order:
    /// - every requirement ([`super::rules::TechRules::requires`]) must be researched;
    /// - some slot of an owned region (faction region list +0x77C → region +0x120 slots, `0x008B13E0`;
    ///   any health, any holder) must hold a building whose chain has the same
    ///   [`super::rules::BuildingRules::chain_pair`] as the tech's building level (chain record +0x10 /
    ///   +0x14), at that level or higher;
    /// - otherwise the state is left as it is. Then it is 2 when each level of
    ///   [`super::rules::TechRules::required_buildings`] (list +0x7C) is held by an owned building of
    ///   exactly that chain at that level or higher (`0x008B13C0` → `0x008B1330`), else 3.
    ///
    /// The writer never sets 4, so a tech never goes back. Callers in the exe: research step
    /// `0x008DD450`, completion `0x008CDCB0`, load `0x008CB8F0`, `0x008CDD60` (after the regions'
    /// update), and through the thunk `0x008AE3B0` the faction turn start (`0x008F2B40`, not for the
    /// rebels: see `TurnStep::FactionReady`) and `AddRegionToFaction` (`0x008AE2D1`, `0x008AE2E3`).
    /// Ours does not call it on a region gain yet (BACKLOG trace line).
    /// The rule reproduces every stored state of the shipped start positions and the vanilla saves.
    ///
    /// ORIGINAL BUG: the exe checks only the single requirement (record +0x60) and the list +0x70,
    /// and its linker `0x00F21A30` puts every requirement of the same tree into +0x60, each
    /// overwriting the last, so all but the last same-tree requirement are never checked. Ours
    /// checks every requirement (vanilla has at most one per tree, so it is unaffected).
    ///
    /// ORIGINAL BUG: a tech whose building level key is unknown keeps a null level (record +0x5C,
    /// `0x00F21A30`), which this walk (`0x008F91F0`) dereferences unconditionally: a crash. Ours
    /// never opens such a tech (the loader logs the key).
    ///
    /// PROVISIONAL: on 2 the exe posts an event (game +0x4E0 queue, payload the technology
    /// manager's +0x14, its faction, and the entry), not identified and not ported.
    pub fn update_tech_availability(&mut self, faction: FactionId) {
        let Some(d) = self.world.faction_details.get(&faction) else { return };
        let owned: Vec<&super::rules::BuildingRules> = self
            .world
            .regions
            .values()
            .filter(|r| r.owner == faction)
            .flat_map(|r| r.slots.iter().filter_map(|s| s.building.as_ref()))
            .filter_map(|b| self.rules.buildings.get(&b.level_key))
            .collect();
        let researched = |k: &str| d.technologies.iter().any(|(x, s)| x == k && *s == state::RESEARCHED);
        let holds = |need: &super::rules::BuildingRules| owned.iter().any(|b| b.chain == need.chain && b.level >= need.level);
        let updates: Vec<(usize, u32)> = d
            .technologies
            .iter()
            .enumerate()
            .filter(|(_, (_, s))| *s == state::UNAVAILABLE || *s == state::BUILDING_MISSING)
            .filter_map(|(i, (k, _))| {
                let t = self.rules.technologies.get(k)?;
                let need = self.rules.buildings.get(&t.building_level)?;
                let tree = need.chain_pair();
                let met = t.requires.iter().all(|q| researched(q)) && owned.iter().any(|b| b.chain_pair() == tree && b.level >= need.level);
                if !met {
                    return None;
                }
                let built = t.required_buildings.iter().all(|l| self.rules.buildings.get(l).is_some_and(holds));
                Some((i, if built { state::AVAILABLE } else { state::BUILDING_MISSING }))
            })
            .collect();
        if let Some(d) = self.world.faction_details.get_mut(&faction) {
            for (i, s) in updates {
                d.technologies[i].1 = s;
            }
        }
    }
}

impl CampaignModel {
    /// True if `faction` has researched every technology in `techs` (state 0).
    pub fn has_researched(&self, faction: FactionId, techs: &[String]) -> bool {
        techs.iter().all(|t| self.tech_state(faction, t) == Some(state::RESEARCHED))
    }

    /// True if `faction` may recruit `unit` as far as technology goes (CONFIRMED, `0x008AAB60`: every
    /// technology `unit_required_technology_junctions` links to the unit (`0x00EA9B10`, unit +0xCC) is in
    /// state 0; one the faction's tree lacks reads as state 5 (`0x008F3DB0`)). Failing it flags the unit's
    /// recruitable entry ([`super::commands::ENTRY_NO_TECHNOLOGY`]).
    pub fn unit_tech_ok(&self, faction: FactionId, unit: &str) -> bool {
        self.rules.unit_techs.get(unit).is_none_or(|t| self.has_researched(faction, t))
    }

    /// True if `faction` may build `level` as far as technology goes (CONFIRMED): every technology
    /// `building_level_required_technology_junctions` links to the level (`0x00DDF300`, level record
    /// +0x118 / +0x11C; [`super::rules::CampaignRules::building_techs`]) is in state 0; one the
    /// faction's tree lacks reads as state 5 (`0x008F3DB0`). Failing it flags the construction option
    /// (flag 8, `BuildSlotConstructionOptionsWithCost` `0x00B43300`), and the construct command,
    /// which stops on an unavailable option, drops it.
    pub fn building_tech_ok(&self, faction: FactionId, level: &str) -> bool {
        self.rules.building_techs.get(level).is_none_or(|t| self.has_researched(faction, t))
    }

    /// The technologies building level `level` unlocks for `faction`, whatever their state
    /// (CONFIRMED, `0x008C7620` → `0x008C7640`): every technology of the faction's tree whose
    /// building level (technology record +0x5C) is `level` itself (no chain or variant match) and
    /// whose faction list passes the faction ([`super::rules::TechRules::factions`], `0x00F1EFA0` with
    /// the faction's own record, faction +0x514, `0x008B9DE0`). In tree order: the walk is the one the
    /// `techs[]` saver `0x00894430` makes, so the order of [`super::world::FactionDetails::technologies`].
    /// The exe takes the faction's record from `0x008B9DE0` (+0x514) and falls back to `britain`'s when that is
    /// null; ours always has the faction's own key, so the fallback can't arise.
    /// Empty for a faction without a tree (faction +0x7C4 null). Callers show it only while
    /// [`super::world::World::technology_research_disabled`] is clear: see
    /// [`Self::building_info_technologies`] and [`Self::effects_table_technologies`].
    pub fn building_level_technologies(&self, faction: FactionId, level: &str) -> Vec<String> {
        let (Some(d), Some(f)) = (self.world.faction_details.get(&faction), self.world.factions.get(&faction)) else { return Vec::new() };
        d.technologies
            .iter()
            .map(|(k, _)| k)
            .filter(|k| {
                self.rules
                    .technologies
                    .get(*k)
                    .is_some_and(|t| t.building_level == level && (t.factions.is_empty() || t.factions.contains(&f.key)))
            })
            .cloned()
            .collect()
    }

    /// The technology list of the building info (`0x009B7300`, used twice by `CampaignUI.BuildingEffects`,
    /// handler `0x009DE150`): [`Self::building_level_technologies`], or nothing while
    /// [`super::world::World::technology_research_disabled`] is set (CONFIRMED test at `0x009B73AF`; the
    /// rest of that info, `0x009B73F0` / `0x009B74E0`, is built either way).
    pub fn building_info_technologies(&self, faction: FactionId, level: &str) -> Vec<String> {
        if self.world.technology_research_disabled {
            return Vec::new();
        }
        self.building_level_technologies(faction, level)
    }

    /// The "technology" table of a building effects table (`0x009B2BB0`, which also builds the
    /// "generic", "construction", "recruitment", "FactionColour" and "UniformColour" tables):
    /// [`Self::building_level_technologies`] of the owner, or nothing when `research_disabled`, when there
    /// is no owner, or when the campaign's [`super::features::CampaignFeatures::effects_technologies_hidden`]
    /// is set (CONFIRMED, `0x009B2FA1`..`0x009B2FE0`). `research_disabled` is
    /// [`super::world::World::technology_research_disabled`] as it stands for `CampaignUI.FortEffects`
    /// (`0x009E4AA0`), but for a building-completed message the value it had when the message was posted
    /// (`0x009CDA30` / `0x009FB920` store it at message +0xBC, its details builder `0x00A096C0` passes it
    /// on).
    pub fn effects_table_technologies(&self, owner: Option<FactionId>, level: &str, research_disabled: bool) -> Vec<String> {
        match owner {
            Some(f) if !research_disabled && !self.rules.features.effects_technologies_hidden => self.building_level_technologies(f, level),
            _ => Vec::new(),
        }
    }
}

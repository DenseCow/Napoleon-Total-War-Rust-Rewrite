//! Technology research (CAMPAIGN_FIDELITY.md §Research).
//!
//! A school (a building whose own effects give `research_points` > 0, at full health, in a slot its
//! region's owner holds) researches one technology at a time (`0x008B33B0` / `0x008EEC90`). Each
//! research step adds the school's rate to the technology's progress (`0x008DD450`); at the cost the
//! technology is researched: state 0, progress = cost, researcher cleared (`0x008EED20`). Techs
//! become available (state 4 → 2) once their prerequisites are researched and the faction owns the
//! tech's building level; they never go back (CONFIRMED writer `0x008F91F0`, see
//! [`CampaignModel::update_tech_availability`]).

use super::details::TechResearch;
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

/// The thread of a technology (`0x008EA9F0`, technology record +0x8C), which picks the thread effect:
/// 0 `research_points_military`, 1 `research_points_industry`, 2 `research_points_enlightenment`.
/// INFERRED from the key prefix (see CAMPAIGN_FIDELITY.md §Research).
pub fn thread_of(tech: &str) -> usize {
    if tech.starts_with("military") {
        0
    } else if tech.starts_with("economy") {
        1
    } else {
        2
    }
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
    pub fn research_rate(&self, region: RegionId, slot: usize, tech: &str) -> f32 {
        self.research_parts(region, slot).map_or(0.0, |p| p.rate(thread_of(tech)))
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
                *t = TechResearch { progress: cost, researcher: 0 };
                if let Some(s) = d.technologies.iter_mut().find(|(x, _)| *x == k) {
                    s.1 = state::RESEARCHED;
                }
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

    /// State 4 → 2 for every technology whose prerequisites are researched and whose building level
    /// the faction owns (`0x008F91F0`, CONFIRMED structure). For each tech in state 4 or 3: its single
    /// requirement (record +0x60) must be absent or researched; some slot of an owned region (region
    /// +0x120 list, `0x008B13E0`; any health, any holder) must hold a building whose chain matches the
    /// tech's building level (the chain record's +0x10 / +0x14 pair) at that level or higher; every tech
    /// of the record's list +0x70 (`technology_required_technology_junctions`) must be researched. Then
    /// the state is 2, or 3 when an item of the list +0x7C fails `0x008B13C0` (INFERRED to be
    /// `technology_required_building_levels_junctions`, empty in the shipped DB, so 3 never occurs).
    /// The writer never sets 4, so a tech never goes back. The chain match is modelled as "the chain,
    /// or a variant whose name starts with it" (`sAdminSpain` for `sAdmin`; INFERRED for the pair).
    /// Callers in the exe: research step `0x008DD450`, completion `0x008CDCB0`, load `0x008CB8F0`,
    /// and `0x008CDD60` (after the regions' update). The rule reproduces every stored state of the
    /// shipped start positions and the vanilla saves.
    pub fn update_tech_availability(&mut self, faction: FactionId) {
        let Some(d) = self.world.faction_details.get(&faction) else { return };
        let owned: Vec<(&str, i32)> = self
            .world
            .regions
            .values()
            .filter(|r| r.owner == faction)
            .flat_map(|r| r.slots.iter().filter_map(|s| s.building.as_ref()))
            .filter_map(|b| self.rules.buildings.get(&b.level_key))
            .map(|b| (b.chain.as_str(), b.level))
            .collect();
        let researched = |k: &str| d.technologies.iter().any(|(x, s)| x == k && *s == state::RESEARCHED);
        let opens: Vec<String> = d
            .technologies
            .iter()
            .filter(|(_, s)| *s == state::UNAVAILABLE)
            .filter(|(k, _)| {
                let Some(t) = self.rules.technologies.get(k) else { return false };
                let Some(need) = self.rules.buildings.get(&t.building_level) else { return false };
                t.requires.iter().all(|q| researched(q)) && owned.iter().any(|(c, l)| c.starts_with(need.chain.as_str()) && *l >= need.level)
            })
            .map(|(k, _)| k.clone())
            .collect();
        if let Some(d) = self.world.faction_details.get_mut(&faction) {
            for (k, s) in d.technologies.iter_mut() {
                if opens.contains(k) {
                    *s = state::AVAILABLE;
                }
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

    /// True if `faction` may build `level` as far as technology goes
    /// (`building_level_required_technology_junctions`; INFERRED gate, of the shape of [`Self::unit_tech_ok`], not traced).
    pub fn building_tech_ok(&self, faction: FactionId, level: &str) -> bool {
        self.rules.building_techs.get(level).is_none_or(|t| self.has_researched(faction, t))
    }
}

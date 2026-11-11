//! Campaign ambushes (CAMPAIGN_FIDELITY.md "Ambush"): a hidden army or navy springs on an enemy force
//! that walks into its zone of control.
//!
//! **No stance.** Napoleon has no ambush stance (and no forced march): no stance command among the CCQ
//! commands, no stance string, and the "can ambush" cursor and tooltip texts are only the UI's. An
//! ambush is a force standing hidden ([`super::details::CharacterDetails::hidden`], the character's
//! +0x4E0, set by the stealth test when it is set down, [`CampaignModel::set_down`]); the script query
//! `CharacterEndedInAmbushPosition` (handler `0x0089C580`) returns that flag (CONFIRMED by decompile).
//!
//! **The spring** ([`CampaignModel::ambush_on_walk`]). After each step of a move
//! (`CampaignCharacter::StepCampaignCharacterMove` `0x00972CA0`), the zone-entry step `0x00B6A270` looks
//! for characters within 50 map units of the mover on both axes (`0x00BA6440`) whose +0x4E0 is set and
//! whom the mover's faction does not know ([`super::agents::knows_character`], `0x008CE880`); if the
//! mover stands in one's stored zone (`0x00B13690` on +0x1C8, [`CampaignModel::in_stored_zone`]) it
//! hands him to `0x00959EA0`, which asks the encounter `0x00A15610` once per move for each such
//! character: anyone already in the move tracker's +124 list of {character, byte} pairs is skipped,
//! whatever happened when he was asked. The encounter (this: the
//! hidden character, the mover passed in) goes on only when both or neither lead an army
//! (`0x009D3A40` / `0x009D3A60`), they are of different factions at war, the hidden army's residence
//! is not besieged and the hidden navy is not docked in a blockaded port, and no battle is pending that
//! the mover is in (`0x008CE780`). The hidden side then decides ([`ambush_springs`], the
//! [`ambush.spring`](super::seams::AMBUSH_SPRING) seam); on yes, the battle setup `0x008E18D0` runs with
//! the hidden character as the attacker and the mover as the target, its fAmbush his +0x4E0: two
//! armies fight a `land_ambush` battle (`SelectCampaignBattleType` `0x008B55A0`,
//! [`CampaignModel::battle_type`]), and the setup clears the attacker's +0x4E0 at its end (CONFIRMED by
//! decompile, all of the above).
//!
//! Not modelled here: the camera calls of the encounter (`0x0095E580` / `0x0095E550`), the setup's
//! placement retry (`0x00B4DDF0`, a free spot within 10 map units by `0x00B37370`: the hidden side has
//! not moved, so its place is the one it was set down on), and the other zone entries the step hands to
//! the same encounter afterwards (`0x00959F80` on the lists of `0x00B7AD30`: the interception of a
//! visible enemy, BACKLOG §5).

use super::agents::knows_character;
use super::battles::PendingBattle;
use super::events::CampaignEvent;
use super::ids::{CharacterId, ForceId};
use super::world::{CampaignModel, Stance};

/// Half the side of the box the zone-entry step searches around the mover (`0x00B6A270` → `0x00BA6440`,
/// ±50.0 on both axes; CONFIRMED by decompile). Zones reach at most a dozen map units, so it never
/// leaves out a zone the mover can stand in.
const SEARCH_HALF_SIDE: f32 = 50.0;

/// The step (map units) at which [`CampaignModel::ambush_on_walk`] tests a walk's path. The model's walk
/// is one jump along its planned points; the exe tests the mover's position after each step of its move
/// (`0x00972CA0`), whose length is not traced. INFERRED: the walk stops within this distance of the
/// zone's edge, as the exe's stops at the first step inside it.
const STEP: f32 = 0.1;

/// An ambush sprung on a walk: the hidden character whose zone the walk entered, and where it stops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SprungAmbush {
    /// The hidden character who springs it (the battle's attacker).
    pub ambusher: CharacterId,
    /// The path segment the walk stops on: from point `segment` to point `segment + 1`.
    pub segment: usize,
    /// How far along that segment it stops (0..1].
    pub along: f32,
}

/// The original's [`ambush.spring`](super::seams::AMBUSH_SPRING): whether the hidden `ambusher` springs
/// on `mover`, who walked into his zone (`0x00A15610`). A human faction's hidden force always does
/// (faction +0x6E0 set; CONFIRMED by decompile). An AI faction's asks its campaign AI (`0x00A60F10` →
/// the CAI's virtual +200 with the moving forces, then the per-character override list `0x009CB500`),
/// which the model does not have yet ([`ai_springs`]). A query: it changes no state.
pub fn ambush_springs(model: &CampaignModel, ambusher: CharacterId, mover: CharacterId) -> bool {
    let Some(faction) = model.world.characters.get(&ambusher).map(|c| c.faction) else { return false };
    model.is_human(faction) || ai_springs(model, ambusher, mover)
}

/// The AI's answer for [`ambush_springs`]. PROVISIONAL: always yes (the CAI decision `0x00A60F10` is
/// not ported).
fn ai_springs(_model: &CampaignModel, _ambusher: CharacterId, _mover: CharacterId) -> bool {
    true
}

impl CampaignModel {
    /// The first ambush `mover`'s walk along `points` springs, if any (the zone-entry step `0x00B6A270`
    /// after each step of the move, module docs): the path is tested every [`STEP`] map units from its
    /// first point on, and at each position each hidden enemy whose zone holds it, in character id order
    /// (INFERRED: the order of the box search `0x00BA6440` is not traced), is asked once per walk
    /// ([`ambush_springs`] through its seam); the first yes springs. None when the mover leads no force
    /// or a battle is pending: the exe steps no mover while the battle sequence's stage is 1 or a
    /// pending action is queued (`UpdateMovingCampaignCharacters` `0x00973B90`, its loop at `0x00973C20`;
    /// CONFIRMED by decompile), so no walk reaches the zone-entry step then (INFERRED: that a set-up
    /// battle holds that gate is not traced), and the model keeps one pending battle.
    pub(crate) fn ambush_on_walk(&self, mover: CharacterId, points: &[(f32, f32)]) -> Option<SprungAmbush> {
        if self.pending_battle.is_some() || points.len() < 2 {
            return None;
        }
        let hidden = self.hidden_enemies(mover);
        if hidden.is_empty() {
            return None;
        }
        let mut asked: Vec<CharacterId> = Vec::new();
        for (segment, pair) in points.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            let steps = (len / STEP).ceil().max(1.0) as usize;
            for k in 1..=steps {
                let along = k as f32 / steps as f32;
                let p = (a.0 + (b.0 - a.0) * along, a.1 + (b.1 - a.1) * along);
                for &(c, pos) in &hidden {
                    if asked.contains(&c)
                        || (pos.0 - p.0).abs() > SEARCH_HALF_SIDE
                        || (pos.1 - p.1).abs() > SEARCH_HALF_SIDE
                        || !self.in_stored_zone(c, p)
                    {
                        continue;
                    }
                    asked.push(c);
                    if self.rules.seams.ambush_spring.rule()(self, c, mover) {
                        return Some(SprungAmbush { ambusher: c, segment, along });
                    }
                }
            }
        }
        None
    }

    /// The hidden characters `mover`'s walk may enter the zone of, with their positions, in id order: the
    /// encounter's tests (`0x00A15610`, module docs) that do not depend on where the mover is. Each is
    /// flagged hidden, unknown to the mover's faction, of another faction at war with it, and leads a
    /// force of the mover's kind (both armies or both navies), neither an army in a besieged
    /// settlement nor a navy docked in a blockaded port. Empty when the mover leads no force (INFERRED:
    /// the exe's test is that both or neither lead an army, `0x009D3A40` / `0x009D3A60`, so a mover
    /// leading neither an army nor a navy is not excluded there; what such a mover is is not traced).
    fn hidden_enemies(&self, mover: CharacterId) -> Vec<(CharacterId, (f32, f32))> {
        let Some(m) = self.world.characters.get(&mover) else { return Vec::new() };
        let Some(navy) = self.force_of(mover).and_then(|f| self.world.forces.get(&f)).map(|f| f.is_navy) else { return Vec::new() };
        self.world
            .characters
            .values()
            .filter(|c| c.faction != m.faction && self.world.character_details.get(&c.id).is_some_and(|d| d.hidden))
            .filter(|c| self.world.stance(m.faction, c.faction) == Stance::War && !knows_character(self, m.faction, c.id))
            .filter(|c| {
                let Some(force) = self.force_of(c.id).and_then(|f| self.world.forces.get(&f)) else { return false };
                force.is_navy == navy && !self.held_in_place(c.garrisoned_in, force.id, force.is_navy)
            })
            .map(|c| (c.id, (c.position.0.to_f32(), c.position.1.to_f32())))
            .collect()
    }

    /// The encounter's skip (`0x00A15610`): an army whose residence is besieged (+0x9C, its virtual +0x98)
    /// or a navy docked (+0xE0) in a port a navy blockades (+0x68).
    fn held_in_place(&self, residence: Option<super::ids::RegionId>, force: ForceId, navy: bool) -> bool {
        if navy {
            return self
                .docked_port(force)
                .is_some_and(|(r, i)| self.world.regions.get(&r).and_then(|r| r.slots.get(i)).is_some_and(|s| s.blockade.is_some()));
        }
        residence.and_then(|r| self.world.regions.get(&r)).is_some_and(|r| r.siege.under_siege())
    }

    /// The battle setup of a sprung ambush (`0x008E18D0` with the hidden `ambusher` as the attacker and
    /// fAmbush his +0x4E0): the pending battle of his force against `mover`'s, flagged an ambush, and his
    /// hidden flag cleared (the setup's end, CONFIRMED by decompile). The walk has already set the mover
    /// down where it stopped. Nothing when either leads no force.
    pub(crate) fn spring_ambush(&mut self, ambusher: CharacterId, mover: CharacterId) -> Vec<CampaignEvent> {
        let (Some(attacker), Some(target)) = (self.force_of(ambusher), self.force_of(mover)) else {
            log::warn!("the ambush of {ambusher:?} on {mover:?} has no force on one side; no battle");
            return Vec::new();
        };
        self.pending_battle = Some(PendingBattle { attacker, defenders: vec![target], settlement: None, resume: None, ambush: true });
        if let Some(d) = self.world.character_details.get_mut(&ambusher) {
            d.hidden = false;
        }
        vec![CampaignEvent::PreBattle { attacker }]
    }
}

//! Diplomatic stances between factions.
//!
//! W3 §3.4 (CONFIRMED structure): every faction stores one `DIPLOMACY_RELATIONSHIP` per other
//! faction, holding a stance string. So a pair of factions has **two** stored stances, one on each
//! side. The game must keep them consistent; here every change goes through
//! [`World::set_stance`], which always writes both sides (protectorate/patron are mirrored, see
//! [`Stance::mirror`]).
//!
//! The rules on the rest of the relationship record (attitudes, treaties, the per-turn update) are in
//! [`treaties`](super::treaties); `CampaignModel::declare_war_rules` / `make_peace_rules` call the functions here.

use super::commands::CommandError;
use super::events::CampaignEvent;
use super::ids::FactionId;
use super::world::{Stance, World};

impl World {
    /// The stance of `a` towards `b`. A missing relationship counts as [`Stance::Neutral`].
    pub fn stance(&self, a: FactionId, b: FactionId) -> Stance {
        self.factions
            .get(&a)
            .and_then(|f| f.diplomacy.get(&b).copied())
            .unwrap_or_default()
    }

    /// Sets the stance of `a` towards `b` to `stance`, and of `b` towards `a` to `stance.mirror()`.
    /// Both factions must exist and differ.
    pub fn set_stance(
        &mut self,
        a: FactionId,
        b: FactionId,
        stance: Stance,
    ) -> Result<(), CommandError> {
        self.check_pair(a, b)?;
        if let Some(fa) = self.factions.get_mut(&a) {
            fa.diplomacy.insert(b, stance);
        }
        if let Some(fb) = self.factions.get_mut(&b) {
            fb.diplomacy.insert(a, stance.mirror());
        }
        Ok(())
    }

    /// `a` declares war on `b`. Fails if they are already at war.
    ///
    /// The stance part only; the relationship rules (alliances broken first, attitude changes, third
    /// parties) are `CampaignModel::declare_war_rules` ([`treaties`](super::treaties)). Allies are not
    /// called in (an AI / player decision, `0x00B268B0`).
    pub fn declare_war(
        &mut self,
        a: FactionId,
        b: FactionId,
    ) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_pair(a, b)?;
        if self.stance(a, b) == Stance::War {
            return Err(CommandError::AlreadyAtWar(a, b));
        }
        self.set_stance(a, b, Stance::War)?;
        Ok(vec![CampaignEvent::StanceChanged {
            a,
            b,
            stance: Stance::War,
        }])
    }

    /// `a` and `b` make peace: from war back to neutral. Fails if they are not at war.
    /// PLACEHOLDER: peace terms (regions, payments, ...) are not modelled.
    pub fn make_peace(
        &mut self,
        a: FactionId,
        b: FactionId,
    ) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_pair(a, b)?;
        if self.stance(a, b) != Stance::War {
            return Err(CommandError::NotAtWar(a, b));
        }
        self.set_stance(a, b, Stance::Neutral)?;
        Ok(vec![CampaignEvent::StanceChanged {
            a,
            b,
            stance: Stance::Neutral,
        }])
    }

    /// `true` if every stored stance has a matching mirrored stance on the other side.
    pub fn diplomacy_is_symmetric(&self) -> bool {
        self.factions.iter().all(|(&a, f)| {
            f.diplomacy
                .iter()
                .all(|(&b, &s)| self.stance(b, a) == s.mirror())
        })
    }

    fn check_pair(&self, a: FactionId, b: FactionId) -> Result<(), CommandError> {
        for id in [a, b] {
            if !self.factions.contains_key(&id) {
                return Err(CommandError::UnknownFaction(id));
            }
        }
        if a == b {
            return Err(CommandError::SameFaction(a));
        }
        Ok(())
    }
}

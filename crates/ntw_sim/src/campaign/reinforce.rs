//! # The reinforcement march
//!
//! A new army raised for a commander (`CampaignModel::spawn_reinforcement`) gets an entry in its
//! faction's reinforcement manager ([`World::reinforcements`](super::world::World::reinforcements),
//! `ARMY_REINFORCEMENT_MANAGER`, faction +0x52C). The faction's turn start gives each entry's army
//! a multi-turn order to march to the commander and merge into his army; the order is kept on the
//! army's commander ([`World::standing_orders`](super::world::World::standing_orders), character
//! +0x9C) and resumed at every later turn start until the merge is made, the order is given
//! something else, or one of the two is gone. CAMPAIGN_FIDELITY.md "Recruitment spawn and
//! reinforcements".

use std::collections::BTreeSet;

use super::commands::CommandError;
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId};
use super::world::{CampaignModel, StandingOrder};

impl CampaignModel {
    /// Steps `faction`'s reinforcement entries, as the faction turn start does after its regions
    /// (`0x008F2620` calls `0x00B71FA0` on the manager at faction +0x52C at `0x008F2B35`, CONFIRMED;
    /// the rebels' branch `0x008F2B47` skips it, so `TurnStep::FactionReady` never calls this for
    /// them and their entries stay). Per entry, in order (`0x00B5C710`, CONFIRMED):
    /// - one that fails the validity check `0x00B5C830` is dropped: its new army's commander or the
    ///   target commander is gone (their death listener `0x00B57DA0` clears the entry), or
    ///   [`Self::may_join_own_force`] refuses;
    /// - else the order is given (`0x00B5C4A0` → `0x00968B00` → the move-to-character order
    ///   `0x009195A0` → `RunCharacterOrder` `0x009651A0`, which runs the order's execute
    ///   `0x0095BA40` at once): here the merge command's walk and exchange
    ///   ([`Self::march_and_merge`]). The order stays on the army's commander
    ///   ([`StandingOrder::JoinForce`]) and the entry is deleted. An order that cannot move this
    ///   turn for lack of action points is still given: the execute commits the path whatever the
    ///   points (CONFIRMED, `0x0095BA40`).
    /// - when the order cannot be given (`0x00968B00` returns 0: the gate `0x008BE360` refuses while
    ///   a battle is pending, or the execute finds no path) the entry is kept and tried again at the
    ///   next turn start (CONFIRMED, `0x00B5C710` deletes only on success).
    ///
    /// The gate `0x008BE360` also checks the campaign's turn faction and stage; this runs inside the
    /// faction's own turn start, so only its pending-battle part can refuse here.
    pub(crate) fn march_reinforcements(&mut self, faction: FactionId) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        // The faction's entries (and any whose new army's commander is gone, dropped below); the
        // others keep their place.
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.world.reinforcements)
            .into_iter()
            .partition(|r| self.world.characters.get(&r.source).is_none_or(|c| c.faction == faction));
        self.world.reinforcements = rest;
        for r in mine {
            if !self.may_join_own_force(r.source, r.target) {
                continue;
            }
            let (Some(src), Some(dst)) = (self.force_of(r.source), self.force_of(r.target)) else { continue };
            if self.pending_battle.is_some() {
                self.world.reinforcements.push(r);
                continue;
            }
            match self.march_and_merge(r.source, src, dst) {
                Ok(walk) => {
                    events.extend(walk.events);
                    self.keep_joining(r.source, dst);
                }
                Err(CommandError::NotEnoughMovementPoints { .. }) => self.keep_joining(r.source, dst),
                // PROVISIONAL (a target of another faction, `CommandError::WrongFaction`): ours keeps
                // the entry and tries again each turn. The exe checks no faction on this path: not the
                // validity check (`0x00B5C830` → `0x009D0D00`), the issue `0x00968B00`, the order
                // `0x009195A0` / execute `0x0095BA40`, the step builder `0x0091CED0` (exchange partner
                // the target army's +0x74) nor the arrival `0x0094D660` (the fit `0x008B2720`, then the
                // transfer `0x008D7310`), so it gives the order; whether the route `0x0091E5E0` reaches
                // a foreign army and what the transfer does across factions is not traced.
                Err(e) => {
                    log::debug!("the reinforcement raised in {:?} cannot march to {:?} this turn ({e}); it tries again next turn", r.settlement, r.target);
                    self.world.reinforcements.push(r);
                }
            }
        }
        events
    }

    /// Every standing order of `faction`'s characters, in the character list order (the turn
    /// start's loop at `0x008F2C5B` → `0x009653D0`, CONFIRMED). An order goes on when its class's
    /// still-valid check passes (vtable +0x10) and is cancelled otherwise (vtable +0, release
    /// `0x009310D0`, character +0x9C cleared). Every order is resumable here: the refill
    /// `0x0096C140` marks it so (+0x14 = 1) at the character's turn start, unconditionally, and a
    /// newly given one is marked by `RunCharacterOrder` `0x009651A0`.
    ///
    /// [`StandingOrder::JoinForce`] (the move-to-character order, vtable `0x0135F868`): valid while
    /// its army's commander still leads a force and the target force exists (the still-valid test
    /// `0x00974660` reads two bytes that only the order's listeners and its cancel `0x00915470` set,
    /// not a check of the live state: `0x00957F20` for the target force and the mover's force,
    /// `0x00957FA0` and `0x009580D0`; the fourth listener `0x009580B0` only asks for a new snapshot of
    /// the target); it runs the merge walk again ([`Self::march_and_merge`], the execute
    /// `0x0095BA40`) and ends when it is complete (`0x0094E840`: the mover's force is the target's, i.e. the merge is made; checked by
    /// the step mover `0x00972CA0` at `0x00973A04`). An exchange left pending (a full receiver) does
    /// not complete it, so the army tries again at each turn start. One whose execute finds no path
    /// is abandoned (`0x0095BA40`, CONFIRMED).
    pub(crate) fn resume_standing_orders(&mut self, faction: FactionId) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        if self.world.standing_orders.is_empty() {
            return events;
        }
        for c in self.world.faction_characters_in_list_order(faction) {
            // Taken out first; `keep_joining` puts it back while the march goes on.
            let Some(order) = self.world.standing_orders.remove(&c) else { continue };
            match order {
                StandingOrder::JoinForce { into } => {
                    let Some(force) = self.force_of(c).filter(|_| self.world.forces.contains_key(&into)) else { continue };
                    match self.march_and_merge(c, force, into) {
                        Ok(walk) => {
                            events.extend(walk.events);
                            self.keep_joining(c, into);
                        }
                        Err(CommandError::NotEnoughMovementPoints { .. }) => self.keep_joining(c, into),
                        Err(e) => log::debug!("{c:?}'s march to join {into:?} is abandoned: {e}"),
                    }
                }
            }
        }
        events
    }

    /// Keeps `c`'s order to join `into` unless it is complete (`0x0094E840`): his force merged
    /// into `into` and he leads none.
    fn keep_joining(&mut self, c: CharacterId, into: ForceId) {
        if self.force_of(c).is_some_and(|f| f != into) && self.world.forces.contains_key(&into) {
            self.world.standing_orders.insert(c, StandingOrder::JoinForce { into });
        }
    }

    /// Whether `agent` may join `target`'s force: `0x009D0D00` (CONFIRMED, disassembly), the
    /// reinforcement entry's validity check (`0x00B5C830`) with the agent the new army's commander.
    /// When the agent's type counts against a residence's room (`agents` #4 through `0x00A0B9C0`,
    /// see [`super::CampaignRules::residence_exempt_agents`]) and both lead a force (character
    /// +0x2A8):
    /// - his army to a navy: yes when the navy carries no army (navy +0xD8), else when his army fits
    ///   into the carried one;
    /// - his navy to an army: no when his navy carries nothing, else when the carried army fits into
    ///   the target's;
    /// - navy to navy: the navies must fit, and when both carry an army those must fit too;
    /// - army to army: yes.
    ///
    /// Otherwise yes only when his type counts and the target leads a force, army or navy (his +0x2A8
    /// resolves, `0x009D0F10`), whose accompanying characters (force +0x60, `0x006A3110`) number
    /// fewer than the room
    /// ([`crate::limits::GameLimits::max_residence_characters`]; the exe's literal 20). "Fits" is the
    /// exchange fit `0x008B2720(a, b, 1)` ([`Self::exchange_fits`]).
    /// PROVISIONAL: the model keeps no accompanying characters (`MILITARY_FORCE` #2), so that branch
    /// counts 0.
    pub(crate) fn may_join_own_force(&self, agent: CharacterId, target: CharacterId) -> bool {
        let Some(a) = self.world.characters.get(&agent) else { return false };
        if !self.world.characters.contains_key(&target) {
            return false;
        }
        if self.rules.residence_exempt_agents.contains(a.kind.esf_name()) {
            return false;
        }
        let navy = |f: ForceId| self.world.forces.get(&f).is_some_and(|f| f.is_navy);
        let carried = |f: ForceId| self.passengers_of(f).into_iter().next();
        let fits = |giver: ForceId, receiver: ForceId| self.exchange_fits(giver, receiver);
        match (self.force_of(agent), self.force_of(target)) {
            (Some(own), Some(theirs)) => match (navy(own), navy(theirs)) {
                (false, true) => carried(theirs).is_none_or(|t| fits(own, t)),
                (true, false) => carried(own).is_some_and(|o| fits(o, theirs)),
                (true, true) => match (carried(own), carried(theirs)) {
                    (Some(o), Some(t)) => fits(own, theirs) && fits(o, t),
                    (None, None) => true,
                    _ => fits(own, theirs),
                },
                (false, false) => true,
            },
            _ => {
                let accompanying = 0usize;
                self.force_of(target).is_some() && accompanying < self.rules.limits.max_residence_characters as usize
            }
        }
    }

    /// The commander a finished land recruit raised through `target` marches to (`0x00B1E500` phase 1,
    /// CONFIRMED by disassembly): `target` himself when he leads an army (`0x009D3A40`); else, when he
    /// is attached to a unit of a force (his holder +0x2B0 resolves and the unit's vfunc +0x38
    /// `0x008C92F0` says it is in one), that force's commander (unit vfunc +0x34 `0x008D9BB0`, force
    /// +0x74), who then leads the item (item vfunc +0x14). `None`: the item is a settlement recruit.
    /// `army_leaders` holds the commanders of every army.
    ///
    /// ORIGINAL BUG: the exe hands the item to that force's commander without checking that he leads
    /// an army, when the force is a navy (a general aboard a ship, or the target an admiral attached
    /// to his own ship). The retarget (item vfunc +0x14, `0x00B1B480`) relinks the item into the new
    /// target's army list (`0x009D3A40` → `0x008A9110` → `0x008CF810`), and with no army the list
    /// pointer is a small offset from null, read at `0x008CF82A`: a crash in phase 1 itself.
    /// Ours spawns that item as a settlement recruit, the branch the exe takes when the target is in
    /// no force.
    pub(crate) fn reinforcement_commander(&self, target: CharacterId, army_leaders: &BTreeSet<CharacterId>) -> Option<CharacterId> {
        if army_leaders.contains(&target) {
            return Some(target);
        }
        let holder = self.world.forces.values().find(|f| f.units.iter().any(|u| u.character == Some(target)))?;
        holder.commander.filter(|c| army_leaders.contains(c))
    }
}

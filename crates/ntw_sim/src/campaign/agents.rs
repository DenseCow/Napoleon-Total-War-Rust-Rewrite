//! Agent actions against characters: assassination and duels, read from the exe (slot 0-G). Spec:
//! `analysis/fidelity/CHARACTERS_FIDELITY.md` §7. Specs in our words; no decompiled code.
//!
//! - The chance of an action is looked up per ability and target kind in a table built at start
//!   (`0x00951F80`: `can_assassinate` → `0x009225D0` against a character, `can_duel` → `0x00922940`,
//!   `can_spy` → `0x00922850` / `0x00922A70`, `can_sabotage` → `0x00922C90` / `0x00922DF0`,
//!   `can_sabotage_army` → `0x00922BA0`).
//! - The roll (`0x00920B70`): one 0..=100 draw on the campaign RNG against the chance; the outcome
//!   ([`Outcome`], `0x00920C60`) also uses the agent's skill in the action's attribute.

use super::effects::Effects;
use super::commands::CommandError;
use super::events::CampaignEvent;
use super::ids::{CharacterId, FactionId, ForceId, RegionId};
use crate::fixed::Fixed20;
use super::world::{CampaignModel, CharacterKind};

/// The agent abilities in the exe's order (`0x0145D9B0`; the index is the ability number the
/// action code uses, CONFIRMED).
pub const ABILITIES: [&str; 12] = [
    "can_assassinate",
    "can_convert",
    "can_build_religious",
    "can_build_fort",
    "can_sabotage",
    "can_spy",
    "can_duel",
    "can_receive_duel",
    "can_research",
    "can_attack_land",
    "can_attack_naval",
    "can_sabotage_army",
];

/// The attributes in the order of the character's attribute array (character +0x394, 4 bytes each;
/// the saved `AgentAttributes` order, CONFIRMED in every save): the index the action code uses.
pub const ATTRIBUTES: [&str; 14] = [
    "command_land",
    "command_sea",
    "duelling_pistols",
    "duelling_swords",
    "management",
    "subterfuge",
    "research",
    "zeal",
    "land_defence_engineering",
    "land_siege_engineering",
    "morale_land",
    "morale_sea",
    "movement_points_land",
    "trade",
];

/// Attribute index of `subterfuge`.
pub const SUBTERFUGE: usize = 5;
/// Attribute index of `duelling_pistols`.
pub const PISTOLS: usize = 2;
/// Attribute index of `duelling_swords`.
pub const SWORDS: usize = 3;

/// A character's attribute by index (`0x009CB560` → character +0x394 + 4 × index): the effective
/// level ([`Effects::character_attribute`]: the saved level plus trait and ancillary attribute
/// effects; that the array holds the effective value is INFERRED), −1 when he has no such
/// attribute.
pub fn attribute(model: &CampaignModel, c: CharacterId, index: usize) -> i32 {
    ATTRIBUTES.get(index).and_then(|a| Effects::character_attribute(model, c, a)).unwrap_or(-1)
}

/// Whether faction `f` knows where character `c` is (`0x008CE880`, CONFIRMED): its own characters
/// always; a character with `subterfuge` ≥ 1 or flagged hidden (`CHARACTER` #22, +0x4E0) only once
/// `f` has exposed him ([`CampaignModel::expose`]); anyone else always. Used by the pathfinder
/// (an unknown character's zone of control is hidden, mode 5, obstacle slot 9 `0x00B4D600`) and by
/// the spotting and spying code. PROVISIONAL: the human-ally case (faction +0x814 set and +0x81C
/// clear: a character of the human's ally is known) is not modelled.
pub fn knows_character(model: &CampaignModel, f: FactionId, c: CharacterId) -> bool {
    let Some(ch) = model.world.characters.get(&c) else { return false };
    if ch.faction == f {
        return true;
    }
    let hidden = model.world.character_details.get(&c).is_some_and(|d| d.hidden);
    if attribute(model, c, SUBTERFUGE) >= 1 || hidden {
        return model.world.faction_details.get(&f).is_some_and(|d| d.exposed.contains(&c));
    }
    true
}

/// A character's ability level (`0x009C7610`, CONFIRMED): the saved `AgentAbilities` level; when it
/// is not negative and the ability names an attribute, plus that attribute floored at 0.
pub fn ability(model: &CampaignModel, c: CharacterId, ability: &str) -> i32 {
    let Some(d) = model.world.character_details.get(&c) else { return -1 };
    let Some((_, level, attr)) = d.abilities.iter().find(|a| a.0 == ability) else { return -1 };
    if *level < 0 || attr.is_empty() {
        return *level;
    }
    level + Effects::character_attribute(model, c, attr).unwrap_or(0).max(0)
}

/// The main attribute of a character type (`agents` column 8, CONFIRMED data), which his rank is
/// read from.
pub fn main_attribute(kind: CharacterKind) -> &'static str {
    match kind {
        CharacterKind::General | CharacterKind::Colonel => "command_land",
        CharacterKind::Admiral | CharacterKind::Captain => "command_sea",
        CharacterKind::Gentleman | CharacterKind::EasternScholar => "research",
        CharacterKind::Minister => "management",
        CharacterKind::Rake | CharacterKind::Assassin | CharacterKind::Guerilla => "subterfuge",
        CharacterKind::CatholicMissionary | CharacterKind::OrthodoxMissionary | CharacterKind::ProtestantMissionary => "zeal",
    }
}

/// A character's rank (`0x00A198D0`): his main attribute plus situation bonuses, at least −1 and at
/// most 9. CONFIRMED for agents (no bonus applies to spies and gentlemen). PROVISIONAL: the
/// situation bonuses of generals (theatre, army make-up), admirals (fleet make-up), missionaries
/// (theatre zeal) and ministers (their post) are not added here.
pub fn rank(model: &CampaignModel, c: CharacterId) -> i32 {
    let Some(ch) = model.world.characters.get(&c) else { return -1 };
    let v = Effects::character_attribute(model, c, main_attribute(ch.kind)).unwrap_or(-1);
    v.clamp(-1, 9)
}

/// A bonus of a character's own effects (traits and ancillaries, character +0x48C; `0x00E1F130`
/// rounds the value to an integer).
fn own_bonus(model: &CampaignModel, c: CharacterId, bonus: &str) -> i32 {
    Effects::character_effects(model, c).get_int(bonus)
}

fn is_spy(kind: CharacterKind) -> bool {
    matches!(kind, CharacterKind::Rake | CharacterKind::Assassin | CharacterKind::Guerilla)
}

/// The target's protector against assassination (`0x00A04910` with attribute 5): the character of
/// the target's faction with the most `subterfuge` (above 0) among the target himself and those
/// sharing his force or his settlement (the first found wins ties). PROVISIONAL: the original also
/// looks at the spies in the target's region (`0x009FB9D0` → `0x00B417E0`), not modelled.
pub fn protector(model: &CampaignModel, target: CharacterId) -> Option<CharacterId> {
    let t = model.world.characters.get(&target)?;
    let mut group = vec![target];
    if let Some(f) = model.world.forces.values().find(|f| f.commander == Some(target) || f.units.iter().any(|u| u.character == Some(target))) {
        group.extend(f.commander);
        group.extend(f.units.iter().filter_map(|u| u.character));
    }
    if let Some(r) = t.garrisoned_in {
        group.extend(model.world.characters.values().filter(|c| c.garrisoned_in == Some(r)).map(|c| c.id));
    }
    let mut best: Option<(CharacterId, i32)> = None;
    for c in group {
        let Some(ch) = model.world.characters.get(&c) else { continue };
        if ch.faction != t.faction {
            continue;
        }
        let s = attribute(model, c, SUBTERFUGE);
        if s > 0 && best.is_none_or(|(_, b)| s > b) {
            best = Some((c, s));
        }
    }
    best.map(|(c, _)| c)
}

/// The chance in percent that `agent` assassinates `target` (`0x009225D0`, CONFIRMED), or `None`
/// when he cannot (he is not a spy, or the target is a colonel or a captain):
/// - skill A = his `can_assassinate` + his `subterfuge_assassination` bonus, 0..=9;
/// - the target's rank T, 0..=9; the protector's C = his rank (0 when the protector is the target)
///   + his `subterfuge_counterspying` bonus, 0..=9 (0 without a protector);
/// - against a spy: 76.92308 / (A + T + C) × A; against a gentleman, scholar or missionary:
///   76.92308 / (A + T + C) × (A + 0.5); against a General: A / (A + T + C + 1 + 0.1 × the units of
///   his army) × 100; anything else 0;
/// - plus the target's `security_versus_assassination` bonus, then 5..=95.
pub fn assassination_chance(model: &CampaignModel, agent: CharacterId, target: CharacterId) -> Option<i32> {
    let a = model.world.characters.get(&agent)?;
    let t = model.world.characters.get(&target)?;
    if matches!(t.kind, CharacterKind::Colonel | CharacterKind::Captain) || !is_spy(a.kind) {
        return None;
    }
    let skill = (ability(model, agent, ABILITIES[0]) + own_bonus(model, agent, "subterfuge_assassination")).clamp(0, 9);
    let trank = rank(model, target).clamp(0, 9);
    let counter = protector(model, target).map_or(0, |p| {
        let r = if p == target { 0 } else { rank(model, p) };
        (r + own_bonus(model, p, "subterfuge_counterspying")).clamp(0, 9)
    });
    let sum = skill + trank + counter;
    let base = match t.kind {
        k if is_spy(k) => {
            if sum == 0 {
                return None;
            }
            ((76.923_08f32 / sum as f32) * skill as f32) as i32
        }
        CharacterKind::Gentleman
        | CharacterKind::EasternScholar
        | CharacterKind::CatholicMissionary
        | CharacterKind::OrthodoxMissionary
        | CharacterKind::ProtestantMissionary => {
            if sum == 0 {
                return None;
            }
            ((76.923_08f32 / sum as f32) * (skill as f32 + 0.5)) as i32
        }
        CharacterKind::General => {
            let units = model
                .world
                .forces
                .values()
                .find(|f| f.commander == Some(target) || f.units.iter().any(|u| u.character == Some(target)))
                .map_or(0, |f| f.units.len());
            let den = (trank + 1 + counter + skill) as f32 + (units as f64 as f32) * 0.1;
            (skill as f32 / den * 100.0) as i32
        }
        _ => 0,
    };
    Some((base + own_bonus(model, target, "security_versus_assassination")).clamp(5, 95))
}

/// The weapon of a duel: pistols or swords (the attribute the duel is fought with).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weapon {
    /// `duelling_pistols`.
    Pistols,
    /// `duelling_swords`.
    Swords,
}

impl Weapon {
    /// The attribute index.
    pub fn attribute(self) -> usize {
        match self {
            Weapon::Pistols => PISTOLS,
            Weapon::Swords => SWORDS,
        }
    }
}

/// The chance in percent that `challenger` wins a duel against `target` with `weapon`
/// (`0x00922940`, CONFIRMED), or `None` when they cannot duel: the challenger needs `can_duel`,
/// the target `can_duel` or `can_receive_duel`. With both skills in the weapon 0..=9 (P for the
/// challenger, Q for the target): 50 when both are 0, else P / (P + Q) × 100, then 5..=95.
pub fn duel_chance(model: &CampaignModel, challenger: CharacterId, target: CharacterId, weapon: Weapon) -> Option<i32> {
    if ability(model, challenger, "can_duel") < 1 {
        return None;
    }
    if ability(model, target, "can_duel") < 1 && ability(model, target, "can_receive_duel") < 1 {
        return None;
    }
    let p = attribute(model, challenger, weapon.attribute()).clamp(0, 9);
    let q = attribute(model, target, weapon.attribute()).clamp(0, 9);
    if p < 1 && q < 1 {
        return Some(50);
    }
    Some(((p as f32 / (p + q) as f32 * 100.0) as i32).clamp(5, 95))
}

/// The weapon an AI target chooses (`0x00AAAC30`, CONFIRMED rule): the one with the lower chance for
/// the challenger; on a tie a coin flip. PROVISIONAL: the original flips on an RNG of its own
/// (+0x71C), the model on the campaign RNG.
pub fn duel_weapon(model: &mut CampaignModel, challenger: CharacterId, target: CharacterId) -> Weapon {
    let p = duel_chance(model, challenger, target, Weapon::Pistols);
    let s = duel_chance(model, challenger, target, Weapon::Swords);
    if p != s {
        return if p < s { Weapon::Pistols } else { Weapon::Swords };
    }
    if model.rng.unit_float() > 0.5 { Weapon::Swords } else { Weapon::Pistols }
}

/// The result of an action roll (`0x00920C60`, CONFIRMED). The numbers are the exe's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// 0: the draw was at most the chance and below 25 + 4 × the agent's skill (message
    /// `duel_critical_success`; a sabotage does the most damage).
    CriticalSuccess,
    /// 1: the draw at most the chance, but at or above 25 + 4 × the skill.
    Success,
    /// 2: failure, the draw below trunc(100 − (100 − chance) × 0.15): the agent is detected and
    /// escapes (`spy_detected_escape`).
    Failure,
    /// 3: failure, the draw at or above that: the agent is detected and executed
    /// (`spy_detected_execute`).
    CriticalFailure,
}

impl Outcome {
    /// 0 or 1.
    pub fn succeeded(self) -> bool {
        matches!(self, Outcome::CriticalSuccess | Outcome::Success)
    }
}

/// The roll (`0x00920B70`): skill = the agent's attribute `attribute` (−1 for an index past the
/// table, as `army_sabotage`'s 14), one 0..=100 draw on the campaign RNG, then [`Outcome`].
pub fn roll(model: &mut CampaignModel, agent: CharacterId, chance: i32, attribute_index: usize) -> Outcome {
    let skill = attribute(model, agent, attribute_index);
    let r = model.rng.percent_0_100() as i32;
    classify(chance, r, skill)
}

/// [`Outcome`] for a draw `r` against `chance` with the agent's `skill`.
pub fn classify(chance: i32, r: i32, skill: i32) -> Outcome {
    if r <= chance {
        if (skill as f32) * 4.0 + 25.0 <= r as f32 { Outcome::Success } else { Outcome::CriticalSuccess }
    } else {
        let t = (100.0f32 - (100 - chance) as f32 * 0.15) as i32;
        if t <= r { Outcome::CriticalFailure } else { Outcome::Failure }
    }
}

/// What an agent action is (for [`super::CampaignEvent::AgentActionResolved`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAction {
    /// The `assassinate` order (`0x0094ACB0`).
    Assassinate,
    /// The `duel` order (`0x0094C920`, resolved by `0x008ED1D0`, ended by `0x008BFD90`).
    Duel(Weapon),
    /// The `army_sabotage` order (`0x0094DA30`).
    SabotageArmy,
    /// Building sabotage (`0x0094DEF0`).
    SabotageBuilding,
    /// Spying on a settlement (`0x0094CCB0`) or a force (`0x0094C500`).
    Spy(SpyTarget),
}

/// Which attitude factor and config entries an action's diplomatic reaction uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Misdeed {
    /// Factor 14 `assasination_attempt`; config `assassination` / `third_party_assassination`.
    Assassination,
    /// Factor 18 `sabotage_attempt`; config `sabotage` / `third_party_sabotage`.
    Sabotage,
    /// Factor 19 `spying_attempt`; config `spying` / `third_party_spying`.
    Spying,
}

/// An attitude change from the diplomacy config (CONFIRMED values: the defaults the exe builds at
/// start, `0x0042F110`, since the shipped data has no `diplomacy_attitudes` table): the factor's
/// value, drift per turn and limit are set and it becomes limited (`0x00B69640`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttitudeSet {
    /// New value.
    pub value: i32,
    /// New drift per turn.
    pub drift: i32,
    /// New limit.
    pub limit: i32,
}

impl Misdeed {
    /// The attitude factor index ([`super::details::ATTITUDE_FACTORS`]).
    pub fn factor(self) -> usize {
        match self {
            Misdeed::Assassination => 14,
            Misdeed::Sabotage => 18,
            Misdeed::Spying => 19,
        }
    }
    /// The victim's reaction (config entries 7 / 16 / 17).
    pub fn victim(self) -> AttitudeSet {
        match self {
            Misdeed::Assassination => AttitudeSet { value: -50, drift: 2, limit: 0 },
            Misdeed::Sabotage | Misdeed::Spying => AttitudeSet { value: -20, drift: 2, limit: 0 },
        }
    }
    /// Everybody else's reaction (config entries 19 / 20 / 21).
    pub fn third_party(self) -> AttitudeSet {
        match self {
            Misdeed::Assassination => AttitudeSet { value: -5, drift: 1, limit: 0 },
            Misdeed::Sabotage | Misdeed::Spying => AttitudeSet { value: -2, drift: 1, limit: 0 },
        }
    }
}

impl CampaignModel {
    /// Sets `owner`'s attitude factor towards `target` (`0x00B69640`).
    fn set_attitude(&mut self, owner: FactionId, target: FactionId, factor: usize, s: AttitudeSet) {
        if let Some(f) = self.world.relationships.get_mut(&(owner, target)).and_then(|r| r.attitudes.get_mut(factor)) {
            f.value = s.value;
            f.drift = s.drift;
            f.limit = s.limit;
            f.limited = true;
        }
    }

    /// A faction counts as alive (the exe's destroyed flag, `0x008CEEF0`): PROVISIONAL stand-in, it
    /// owns a region.
    fn faction_alive(&self, f: FactionId) -> bool {
        self.world.regions.values().any(|r| r.owner == f)
    }

    /// `victim` spots `agent` (`0x008A8B30`, CONFIRMED): a character with `subterfuge` ≥ 1 is added
    /// to its exposed list (`FACTION` `EXPOSED_CHARACTERS`) even when already there (the exe does not
    /// look); one flagged hidden (`CHARACTER` #22) only when not there yet; anyone else is not added
    /// (he is never hidden). Called on a detected assassination or army sabotage, on a failed spying
    /// attempt, and by spying's critical success and the spotting pass (CHARACTERS_FIDELITY.md §10).
    pub fn expose(&mut self, victim: FactionId, agent: CharacterId) {
        let hidden = self.world.character_details.get(&agent).is_some_and(|d| d.hidden);
        let sub = attribute(self, agent, SUBTERFUGE);
        if !self.world.factions.contains_key(&victim) {
            return;
        }
        let d = self.world.faction_details.entry(victim).or_default();
        if sub >= 1 || (hidden && !d.exposed.contains(&agent)) {
            d.exposed.push(agent);
        }
    }

    /// A detected attempt (`0x00B0D870` assassination, `0x00B67FD0` sabotage, CONFIRMED): the
    /// victim's attitude towards the agent's faction takes the misdeed's victim values, every other
    /// faction's (neither victim nor culprit) the third-party values. Nothing when the victim is
    /// gone.
    pub fn detected_misdeed(&mut self, culprit: FactionId, victim: FactionId, m: Misdeed) {
        if !self.faction_alive(victim) {
            return;
        }
        self.set_attitude(victim, culprit, m.factor(), m.victim());
        let others: Vec<FactionId> = self.world.factions.keys().copied().filter(|f| *f != victim && *f != culprit).collect();
        for f in others {
            self.set_attitude(f, culprit, m.factor(), m.third_party());
        }
    }

    /// An unseen success (`0x00B0D700` assassination, `0x00B67E60` sabotage; outcome 0): the victim
    /// blames the faction it likes least (its lowest attitude total, `0x00B0DB60`; ties go to the
    /// lower id; the culprit itself is passed over unless it is the first one looked at), which gets
    /// the victim values; then every faction other than the culprit and the blamed one gives the
    /// blamed one the third-party values (the victim too, overwriting its own: CONFIRMED order). The
    /// victim's relationships are taken in target id order (PROVISIONAL: the original keeps the file
    /// order, which matters only for the first one looked at). Which faction's list is searched is
    /// INFERRED (the decompiled call loses the object).
    pub fn blamed_misdeed(&mut self, culprit: FactionId, victim: FactionId, m: Misdeed) {
        if !self.faction_alive(victim) {
            return;
        }
        let mut blamed: Option<(FactionId, i32)> = None;
        for ((o, t), r) in &self.world.relationships {
            if *o != victim {
                continue;
            }
            let total = r.attitude_total();
            match blamed {
                None => blamed = Some((*t, total)),
                Some((b, bt)) => {
                    if *t != culprit && (total < bt || (total == bt && t.raw() < b.raw())) {
                        blamed = Some((*t, total));
                    }
                }
            }
        }
        let Some((scapegoat, _)) = blamed else { return };
        self.set_attitude(victim, scapegoat, m.factor(), m.victim());
        let others: Vec<FactionId> = self.world.factions.keys().copied().filter(|f| *f != culprit && *f != scapegoat).collect();
        for f in others {
            self.set_attitude(f, scapegoat, m.factor(), m.third_party());
        }
    }

    /// The agent walks to `to` and acts on arrival, once per turn (character +0x4CC, set by every
    /// action, CONFIRMED; [`super::World::agents_acted`]).
    fn approach_to(&mut self, agent: CharacterId, to: (Fixed20, Fixed20), target_faction: FactionId) -> Result<Option<Vec<CampaignEvent>>, CommandError> {
        let a = self.world.characters.get(&agent).ok_or(CommandError::UnknownCharacter(agent))?.clone();
        if !self.may_act(a.faction) {
            return Err(CommandError::NotYourTurn(a.faction));
        }
        if a.faction == target_faction {
            return Err(CommandError::WrongFaction);
        }
        if self.world.agents_acted.contains(&agent) {
            return Err(CommandError::Unsupported("the agent has already acted this turn"));
        }
        let walk = self.walk(agent, to, super::commands::CONTACT_DISTANCE)?;
        Ok(walk.arrived.then_some(walk.events))
    }

    fn approach(&mut self, agent: CharacterId, target: CharacterId) -> Result<Option<Vec<CampaignEvent>>, CommandError> {
        let t = self.world.characters.get(&target).ok_or(CommandError::UnknownCharacter(target))?.clone();
        self.approach_to(agent, t.position, t.faction)
    }

    /// `assassinate` (`0x0094ACB0`, CONFIRMED): on arrival the agent rolls against
    /// [`assassination_chance`] with his `subterfuge`; he has acted this turn in every case.
    /// - critical success: the target dies (reason 2; one the original rebuilds, `CHARACTER` #34,
    ///   escapes as in battle) and the victim blames the faction it likes least
    ///   ([`Self::blamed_misdeed`]);
    /// - success: the target dies, no reaction;
    /// - failure: detected ([`Self::detected_misdeed`]), the agent escapes;
    /// - critical failure: detected, the agent is executed.
    ///
    /// On detection the victim also spots the agent ([`Self::expose`]).
    /// PROVISIONAL (not modelled): the target's unit losing a man, the messages
    /// (0x13D / 0x13F, 246 / 248), and the forced result for a human target when a world flag is set.
    pub(crate) fn assassinate(&mut self, agent: CharacterId, target: CharacterId) -> Result<Vec<CampaignEvent>, CommandError> {
        if assassination_chance(self, agent, target).is_none() {
            return Err(CommandError::Unsupported("this agent cannot assassinate that character"));
        }
        let Some(mut events) = self.approach(agent, target)? else {
            return Ok(Vec::new());
        };
        // The chance where he stands now (the protector may differ after the walk).
        let chance = assassination_chance(self, agent, target).unwrap_or(5);
        let (culprit, victim) = (self.world.characters[&agent].faction, self.world.characters[&target].faction);
        // `SufferAssassinationAttempt` for the target before the roll (CONFIRMED order).
        events.push(CampaignEvent::SufferAssassinationAttempt { character: target });
        let outcome = self.forced_or_roll(agent, chance, SUBTERFUGE, self.world.force_success_for_human.0);
        self.world.agents_acted.insert(agent);
        match outcome {
            Outcome::CriticalSuccess => {
                self.blamed_misdeed(culprit, victim, Misdeed::Assassination);
                self.character_falls(target);
                events.push(CampaignEvent::AssassinationAttemptSuccess { character: agent });
            }
            Outcome::Success => {
                self.character_falls(target);
                events.push(CampaignEvent::AssassinationAttemptSuccess { character: agent });
            }
            Outcome::Failure => {
                self.detected_misdeed(culprit, victim, Misdeed::Assassination);
                self.expose(victim, agent);
            }
            Outcome::CriticalFailure => {
                self.detected_misdeed(culprit, victim, Misdeed::Assassination);
                self.expose(victim, agent);
                events.push(CampaignEvent::CharacterCriticallyFailsAssassination { character: agent });
                self.character_dies(agent);
            }
        }
        events.push(CampaignEvent::AgentActionResolved { agent, target: Some(target), action: AgentAction::Assassinate, outcome });
        Ok(events)
    }

    /// The roll, or outcome 1 without a draw when `forced` holds and the agent's faction is human
    /// (`0x0094ACB0` / `0x0094CCB0` with the episodic switches `World::force_success_for_human`;
    /// CONFIRMED: the switch is read only together with the faction's human flag +0x6E0).
    fn forced_or_roll(&mut self, agent: CharacterId, chance: i32, attribute_index: usize, forced: bool) -> Outcome {
        let human = self.world.characters.get(&agent).is_some_and(|a| self.turn.humans.contains(&a.faction));
        if forced && human {
            return Outcome::Success;
        }
        roll(self, agent, chance, attribute_index)
    }

    /// `duel` (`0x0094C920` → `0x008ED1D0` → the duel object's end `0x008BFD90`, CONFIRMED): the
    /// target picks the weapon ([`duel_weapon`]; PROVISIONAL for a human target, who chooses in a
    /// popup), the challenger rolls against [`duel_chance`] with his skill in it. The winner's
    /// `CHARACTER` #37 (duels won) and the loser's #36 (duels lost) go up by 1. On a critical
    /// success or failure the loser dies (kill reason 4 with pistols, 7 with swords; message
    /// `duel_critical_success` 0x45). On an ordinary success or failure he first tries to flee
    /// (`0x00A18460`, CONFIRMED: `CHARACTER` #27 +0x510 set, then a flight order towards his force
    /// if he has one, else the settlement of the region he stands in, `0x00A17CF0` +0x70): when the
    /// order can be issued he is only wounded (+0x512, message `duel_success` 0x47; the
    /// `_injured` texts), cleared when he arrives (`0x0094EA00`, message 0x46); when it cannot, he
    /// dies. `DuelFought` is then fired for both (`0x00934C60`). The "spared" loser of earlier
    /// notes was this flight. PROVISIONAL: the model moves the loser at once along his path as far
    /// as his action points reach and clears the wound; a loser with no region and no force dies.
    pub(crate) fn duel(&mut self, challenger: CharacterId, target: CharacterId) -> Result<Vec<CampaignEvent>, CommandError> {
        if duel_chance(self, challenger, target, Weapon::Pistols).is_none() {
            return Err(CommandError::Unsupported("these characters cannot duel"));
        }
        let Some(mut events) = self.approach(challenger, target)? else {
            return Ok(Vec::new());
        };
        let weapon = duel_weapon(self, challenger, target);
        let chance = duel_chance(self, challenger, target, weapon).unwrap_or(50);
        let outcome = roll(self, challenger, chance, weapon.attribute());
        self.world.agents_acted.insert(challenger);
        let (winner, loser) = if outcome.succeeded() { (challenger, target) } else { (target, challenger) };
        self.world.character_details.entry(winner).or_default().duels_won += 1;
        self.world.character_details.entry(loser).or_default().duels_lost += 1;
        let flees = matches!(outcome, Outcome::Success | Outcome::Failure) && self.flee_duel(loser, &mut events);
        if !flees {
            self.character_falls(loser);
        }
        events.push(CampaignEvent::DuelFought { character: challenger });
        events.push(CampaignEvent::DuelFought { character: target });
        events.push(CampaignEvent::AgentActionResolved { agent: challenger, target: Some(target), action: AgentAction::Duel(weapon), outcome });
        Ok(events)
    }

    /// The loser's flight (`0x00A18460`): marks him (`fled`, `wounded`) and walks him towards his
    /// force's position or the settlement of the region he stands in. True when a flight order
    /// could be issued (a destination exists and a path to it); the caller kills him otherwise.
    fn flee_duel(&mut self, loser: CharacterId, events: &mut Vec<CampaignEvent>) -> bool {
        let Some(ch) = self.world.characters.get(&loser).cloned() else { return false };
        let to = match self.force_of(loser).and_then(|f| self.force_position(f)) {
            Some(p) => Some(p),
            None => self.world.regions.values().find(|r| super::economy::in_region(self, r, &ch)).map(|r| r.settlement.position),
        };
        let Some(to) = to else { return false };
        if let Some(d) = self.world.character_details.get_mut(&loser) {
            d.fled = true;
            d.wounded = true;
        }
        if to != ch.position {
            match self.walk(loser, to, 0.0) {
                Ok(w) => events.extend(w.events),
                Err(CommandError::NotEnoughMovementPoints { .. }) => {}
                Err(_) => {
                    if let Some(d) = self.world.character_details.get_mut(&loser) {
                        d.fled = false;
                        d.wounded = false;
                    }
                    return false;
                }
            }
        }
        if let Some(d) = self.world.character_details.get_mut(&loser) {
            d.wounded = false;
        }
        true
    }
}

/// What a spy is sent to watch (`0x00907040` settlement order, `agent_join_force` `0x00905650`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpyTarget {
    /// A region's settlement (another faction's).
    Settlement(RegionId),
    /// A force (another faction's).
    Force(ForceId),
}

/// The chance in percent that `agent` spies on a settlement or a force (`0x00922A70`, CONFIRMED):
/// S = his rank + `subterfuge_spying` (bonus 97), 0..=9; C = the target's protector's rank +
/// `subterfuge_counterspying` (bonus 99), 0..=9, 0 without one; f = 0.5 for a settlement (or a
/// fort), half the unit count for a force; S / (S + f + C) × 100 truncated, then 5..=95 (5 when S
/// is 0). `None` without `can_spy` (INFERRED gate: the order needs the ability) or a target of his
/// own faction. PROVISIONAL: the protector of a settlement is [`building_protector`]; a force has
/// none (the original asks `0x00B417E0` for the target side's best spy, see there).
pub fn spy_chance(model: &CampaignModel, agent: CharacterId, target: SpyTarget) -> Option<i32> {
    if ability(model, agent, ABILITIES[5]) < 1 {
        return None;
    }
    let own = model.world.characters.get(&agent)?.faction;
    let (f, protector, owner) = match target {
        SpyTarget::Settlement(r) => (0.5f32, building_protector(model, r), model.world.regions.get(&r)?.owner),
        SpyTarget::Force(x) => {
            let force = model.world.forces.get(&x)?;
            force.commander?;
            (force.units.len() as f32 * 0.5, None, force.faction)
        }
    };
    if owner == own {
        return None;
    }
    let s = (rank(model, agent) + own_bonus(model, agent, "subterfuge_spying")).clamp(0, 9);
    let c = protector.map_or(0, |p| (rank(model, p) + own_bonus(model, p, "subterfuge_counterspying")).clamp(0, 9));
    if s <= 0 {
        return Some(5);
    }
    Some(((s as f32 / (s as f32 + f + c as f32) * 100.0) as i32).clamp(5, 95))
}

impl CampaignModel {
    /// Spying on a settlement (`0x0094CCB0`) or joining a force as a spy (`0x0094C500`), CONFIRMED
    /// outcomes: on arrival the agent rolls against [`spy_chance`] with his `subterfuge`; he has
    /// acted this turn in every case.
    /// - critical success: his faction spots every foreign character at the target it did not know
    ///   ([`Self::expose`]; the settlement's characters, list +0xD8, `0x00B54A70`; the force's,
    ///   +0x70, `0x008D2BE0`), then as a success;
    /// - success: a script event (0x144) and the human's spying counter (faction counter 37);
    /// - failure: detected (`spy_detected_escape`, 246): the victim's reaction
    ///   ([`Self::detected_misdeed`], `spying`) and the victim spots him;
    /// - critical failure: the same (`spy_detected_execute`, 248) and he is executed.
    ///
    /// The agent sees from where he stands like any character (§10). PROVISIONAL (not modelled):
    /// the spy staying inside the settlement or force, the script event, the counters, the messages
    /// and the forced success of the cheat flag (world +0xFA8 +0xEC); the characters at a settlement
    /// are those garrisoned there and at a force its commander and its units' characters (INFERRED
    /// for the lists).
    pub(crate) fn spy(&mut self, agent: CharacterId, target: SpyTarget) -> Result<Vec<CampaignEvent>, CommandError> {
        if spy_chance(self, agent, target).is_none() {
            return Err(CommandError::Unsupported("this agent cannot spy on that target"));
        }
        let (to, victim, watched) = match target {
            SpyTarget::Settlement(r) => {
                let reg = &self.world.regions[&r];
                (reg.settlement.position, reg.owner, None)
            }
            SpyTarget::Force(x) => {
                let force = &self.world.forces[&x];
                let c = force.commander.expect("checked");
                (self.world.characters.get(&c).ok_or(CommandError::UnknownCharacter(c))?.position, force.faction, Some(c))
            }
        };
        let Some(mut events) = self.approach_to(agent, to, victim)? else {
            return Ok(Vec::new());
        };
        let chance = spy_chance(self, agent, target).unwrap_or(5);
        let culprit = self.world.characters[&agent].faction;
        // Script events (CONFIRMED sites in `0x0094C500` force / `0x0094CCB0` settlement):
        // `SufferSpyingAttempt` for the watched force's commander before the roll; on outcomes 0
        // and 1 `SpyingAttemptSuccess` (the spy), `CharacterFactionSpyAttemptSuccessful` (the
        // spy's faction leader) and `CharacterFactionSuffersSuccessfulSpyAttempt` (the victim's);
        // on 2 and 3 `EspionageAgentApprehended` (the force's commander; none for a settlement).
        if let Some(c) = watched {
            events.push(CampaignEvent::SufferSpyingAttempt { character: c });
        }
        // The settlement order honours `force_garrison_infiltration_success_for_human`.
        let forced = matches!(target, SpyTarget::Settlement(_)) && self.world.force_success_for_human.1;
        let outcome = self.forced_or_roll(agent, chance, SUBTERFUGE, forced);
        self.world.agents_acted.insert(agent);
        match outcome {
            Outcome::CriticalSuccess | Outcome::Success => {
                if outcome == Outcome::CriticalSuccess {
                    let present: Vec<CharacterId> = match target {
                        SpyTarget::Settlement(r) => self.world.characters.values().filter(|c| c.garrisoned_in == Some(r)).map(|c| c.id).collect(),
                        SpyTarget::Force(x) => {
                            let f = &self.world.forces[&x];
                            f.commander.into_iter().chain(f.units.iter().filter_map(|u| u.character)).collect()
                        }
                    };
                    for c in present {
                        let foreign = self.world.characters.get(&c).is_some_and(|x| x.faction != culprit);
                        if foreign && !knows_character(self, culprit, c) {
                            self.expose(culprit, c);
                        }
                    }
                }
                events.push(CampaignEvent::SpyingAttemptSuccess { character: agent });
                if let Some(l) = self.world.faction_details.get(&culprit).and_then(|d| d.leader()) {
                    events.push(CampaignEvent::CharacterFactionSpyAttemptSuccessful { character: l });
                }
                if let Some(l) = self.world.faction_details.get(&victim).and_then(|d| d.leader()) {
                    events.push(CampaignEvent::CharacterFactionSuffersSuccessfulSpyAttempt { character: l });
                }
            }
            Outcome::Failure | Outcome::CriticalFailure => {
                self.detected_misdeed(culprit, victim, Misdeed::Spying);
                self.expose(victim, agent);
                if let Some(c) = watched {
                    events.push(CampaignEvent::EspionageAgentApprehended { character: c });
                }
                if outcome == Outcome::CriticalFailure {
                    self.character_dies(agent);
                }
            }
        }
        events.push(CampaignEvent::AgentActionResolved { agent, target: watched, action: AgentAction::Spy(target), outcome });
        Ok(events)
    }
}

/// The chance in percent that `agent` sabotages `force` (`0x00922BA0`, CONFIRMED), or `None` when
/// he cannot (no `can_sabotage_army`, or the force has no commander): R / ((units + C) × 0.33 + R)
/// × 100 with R the agent's rank and C the commander's, then 5..=95.
pub fn army_sabotage_chance(model: &CampaignModel, agent: CharacterId, force: ForceId) -> Option<i32> {
    let f = model.world.forces.get(&force)?;
    let commander = f.commander?;
    if ability(model, agent, ABILITIES[11]) < 1 {
        return None;
    }
    let r = rank(model, agent) as f32;
    let c = rank(model, commander) as f32;
    let units = f.units.len() as f32;
    Some(((r / ((units + c) * 0.33 + r)) * 100.0) as i32).map(|x| x.clamp(5, 95))
}

/// The protector of a building: the owner's character with the most `subterfuge` (above 0) standing
/// in the region's settlement. PROVISIONAL: the original asks for the owner's best spy of the region
/// (`0x00B417E0`), which needs region membership the model does not keep.
fn building_protector(model: &CampaignModel, region: RegionId) -> Option<CharacterId> {
    let r = model.world.regions.get(&region)?;
    model
        .world
        .characters
        .values()
        .filter(|c| c.faction == r.owner && c.garrisoned_in == Some(region))
        .map(|c| (c.id, attribute(model, c.id, SUBTERFUGE)))
        .filter(|(_, s)| *s > 0)
        .max_by_key(|(id, s)| (*s, std::cmp::Reverse(id.raw())))
        .map(|(id, _)| id)
}

/// The chance in percent that `agent` sabotages the building in `slot` of `region`
/// (`0x00922C90`, CONFIRMED): S = his rank + `subterfuge_sabotage`, 0..=9 (no chance at 0); C =
/// the protector's rank + `subterfuge_counterspying`, 0..=9; k = 3 for chain number 0 (most chains:
/// an empty `building_chains` #2 parses as 0), 1 for 1 or 2, 0.5 otherwise; S / (round((level + 1)
/// × k) + S + C) × 100, then 5..=95. `None` without `can_sabotage`, without a building, or when S is 0.
pub fn building_sabotage_chance(model: &CampaignModel, agent: CharacterId, region: RegionId, slot: usize) -> Option<i32> {
    if ability(model, agent, ABILITIES[4]) < 1 {
        return None;
    }
    let b = model.world.regions.get(&region)?.slots.get(slot)?.building.as_ref()?;
    let rules = model.rules.buildings.get(&b.level_key)?;
    let k = match rules.chain_class {
        0 => 3.0f32,
        1 | 2 => 1.0,
        _ => 0.5,
    };
    let s = (rank(model, agent) + own_bonus(model, agent, "subterfuge_sabotage")).clamp(0, 9);
    if s <= 0 {
        return None;
    }
    let c = building_protector(model, region).map_or(0, |p| (rank(model, p) + own_bonus(model, p, "subterfuge_counterspying")).clamp(0, 9));
    let den = (((rules.level + 1) as f32 * k).round_ties_even() as i32 + s + c) as f32;
    Some(((s as f32 / den * 100.0) as i32).clamp(5, 95))
}

impl CampaignModel {
    /// `army_sabotage` (`0x0094DA30`, CONFIRMED): on arrival the agent rolls against
    /// [`army_sabotage_chance`] with skill −1 (the order names attribute 14, past the table); he has
    /// acted this turn. Success (either kind): the force is sabotaged ([`super::World::sabotaged`]:
    /// its commander's action points are 0 at his next turn start, `0x008F2290` → `0x00A2A140`), and
    /// a guerilla also kills men ([`guerilla_casualties`]); a critical success also makes the victim
    /// blame the faction it likes least. Failure: detected; critical failure: detected and executed.
    /// On detection the victim also spots the agent ([`Self::expose`]). PROVISIONAL (not modelled):
    /// messages.
    pub(crate) fn sabotage_army(&mut self, agent: CharacterId, force: ForceId) -> Result<Vec<CampaignEvent>, CommandError> {
        if army_sabotage_chance(self, agent, force).is_none() {
            return Err(CommandError::Unsupported("this agent cannot sabotage that force"));
        }
        let commander = self.world.forces[&force].commander.expect("checked");
        let Some(mut events) = self.approach(agent, commander)? else {
            return Ok(Vec::new());
        };
        let chance = army_sabotage_chance(self, agent, force).unwrap_or(5);
        let (culprit, victim) = (self.world.characters[&agent].faction, self.world.forces[&force].faction);
        let guerilla = self.world.characters[&agent].kind == CharacterKind::Guerilla;
        let outcome = roll(self, agent, chance, 14);
        self.world.agents_acted.insert(agent);
        match outcome {
            Outcome::CriticalSuccess | Outcome::Success => {
                if outcome == Outcome::CriticalSuccess {
                    self.blamed_misdeed(culprit, victim, Misdeed::Sabotage);
                }
                self.world.sabotaged.insert(force);
                if guerilla {
                    let r = rank(self, agent);
                    guerilla_casualties(self, force, r);
                    events.push(CampaignEvent::HarassmentAttemptSuccess { character: agent });
                } else {
                    events.push(CampaignEvent::ArmySabotageAttemptSuccess { character: agent });
                }
            }
            Outcome::Failure => {
                self.detected_misdeed(culprit, victim, Misdeed::Sabotage);
                self.expose(victim, agent);
            }
            Outcome::CriticalFailure => {
                self.detected_misdeed(culprit, victim, Misdeed::Sabotage);
                self.expose(victim, agent);
                self.character_dies(agent);
            }
        }
        events.push(CampaignEvent::AgentActionResolved { agent, target: Some(commander), action: AgentAction::SabotageArmy, outcome });
        Ok(events)
    }

    /// Building sabotage (`0x0094DEF0`, CONFIRMED): on arrival at the slot the agent rolls against
    /// [`building_sabotage_chance`] with his `subterfuge` s; he has acted this turn. Critical
    /// success: the building's health × (1 − min(4 s + 63, 99.9) %), truncated (`0x00B452A0`), and the
    /// victim blames the faction it likes least; success: × (1 − min(4 s + 38, 99.9) %) (UNKNOWN
    /// extra call `0x0047FD40`); failure: detected; critical failure: detected and executed. A
    /// building below 100 health gives no effects (`Region::effect_buildings`). PROVISIONAL (not
    /// modelled): messages, repair. No exposure (CONFIRMED: `0x0094DEF0` is not a caller
    /// of `0x008A8B30`).
    pub(crate) fn sabotage_building(&mut self, agent: CharacterId, region: RegionId, slot: usize) -> Result<Vec<CampaignEvent>, CommandError> {
        if building_sabotage_chance(self, agent, region, slot).is_none() {
            return Err(CommandError::Unsupported("this agent cannot sabotage that building"));
        }
        let r = &self.world.regions[&region];
        let victim = r.slots[slot].holder.unwrap_or(r.owner);
        let to = r.slots[slot].position.unwrap_or(r.settlement.position);
        let Some(mut events) = self.approach_to(agent, to, victim)? else {
            return Ok(Vec::new());
        };
        let chance = building_sabotage_chance(self, agent, region, slot).unwrap_or(5);
        let culprit = self.world.characters[&agent].faction;
        let s = attribute(self, agent, SUBTERFUGE);
        let outcome = roll(self, agent, chance, SUBTERFUGE);
        self.world.agents_acted.insert(agent);
        let damage = |base: f32| ((s as f32) * 4.0 + base).clamp(0.0, 99.9) * 0.01;
        match outcome {
            Outcome::CriticalSuccess | Outcome::Success => {
                let d = damage(if outcome == Outcome::CriticalSuccess { 63.0 } else { 38.0 });
                if let Some(b) = self.world.regions.get_mut(&region).and_then(|r| r.slots[slot].building.as_mut()) {
                    b.health = ((1.0 - d) * b.health as f32) as u32;
                }
                if outcome == Outcome::CriticalSuccess {
                    self.blamed_misdeed(culprit, victim, Misdeed::Sabotage);
                }
                events.push(CampaignEvent::SabotageAttemptSuccess { character: agent });
            }
            Outcome::Failure => self.detected_misdeed(culprit, victim, Misdeed::Sabotage),
            Outcome::CriticalFailure => {
                self.detected_misdeed(culprit, victim, Misdeed::Sabotage);
                self.character_dies(agent);
            }
        }
        events.push(CampaignEvent::AgentActionResolved { agent, target: None, action: AgentAction::SabotageBuilding, outcome });
        Ok(events)
    }
}

/// A guerilla's sabotage kills men (`0x008EB660`, CONFIRMED): rank × 10 × the unit scale men
/// (PROVISIONAL scale 1.0: the world's unit-size setting is not modelled), spread over the force's
/// units: passes over the units in an order shuffled on the campaign RNG (for k = 2..=n: j =
/// next16 % k, swapped with k − 1 unless j = k − 1); a unit at 10 % strength or more loses
/// int_range(0, min(left, men / 2)); passes repeat while men were lost and some are left.
pub fn guerilla_casualties(model: &mut CampaignModel, force: ForceId, rank: i32) {
    if rank <= 0 {
        return;
    }
    let mut left = ((rank * 10) as f32 * 1.0).round_ties_even() as u32;
    while left > 0 {
        let n = model.world.forces.get(&force).map_or(0, |f| f.units.len());
        let mut order: Vec<usize> = (0..n).collect();
        for k in 2..=n {
            let j = (model.rng.next16() as usize) % k;
            if j != k - 1 {
                order.swap(k - 1, j);
            }
        }
        let mut lost_any = false;
        let mut lost_men = 0;
        for i in order {
            if left == 0 {
                break;
            }
            let Some(u) = model.world.forces.get(&force).and_then(|f| f.units.get(i)) else { continue };
            if (u.men as f32) < 0.1 * u.max_men as f32 {
                continue;
            }
            let cap = left.min(u.men / 2);
            let loss = model.rng.int_range(0, cap as i32) as u32;
            if let Some(u) = model.world.forces.get_mut(&force).and_then(|f| f.units.get_mut(i)) {
                u.men -= loss;
            }
            left -= loss;
            lost_men += loss;
            lost_any = true;
        }
        // A pass that kills nobody ends it (PROVISIONAL guard: the original would loop on units of one
        // man, whose half is 0).
        if !lost_any || lost_men == 0 {
            break;
        }
    }
}

/// The chance that gentlemen of a faction steal a technology at a foreign school in one research
/// step (`0x008F32C0`, CONFIRMED): (skill + 1) × 0.5 / √cost, 0.04..=0.9 (1, so 0.9, when the cost
/// is 0); skill = the summed `research` of that faction's gentlemen there, at most 12.
pub fn steal_chance(skill: i32, cost: i32) -> f32 {
    let root = (cost as f32).sqrt();
    if root == 0.0 {
        return 0.9;
    }
    let c = (skill + 1) as f32 * 0.5 / root;
    c.clamp(0.04, 0.9)
}

impl CampaignModel {
    /// Technology stealing at a school while it researches `tech` (in the research step
    /// `0x008DD450`, CONFIRMED structure): the foreign gentlemen standing at the school are grouped
    /// by faction (their `research` summed, at most 12); then each, in order, draws next16 / 65535 on
    /// the campaign RNG: at or below [`steal_chance`] for his faction's group his faction gets the
    /// technology the school is researching (researched, `0x008CDCB0`) and the stealing stops for this
    /// step; above it he is caught and thrown out of the school (`spy_detected_escape`). INFERRED:
    /// only foreign gentlemen take part. PROVISIONAL: the thrown-out gentleman goes to the region's
    /// settlement position (the original looks for a free spot within 100 of the school,
    /// `0x00B372A0`); what `0x00A25400` does to a successful one is UNKNOWN (he stays).
    pub fn steal_step(&mut self, region: RegionId, slot: usize, tech: &str) -> Vec<(CharacterId, bool)> {
        let mut out = Vec::new();
        let Some(r) = self.world.regions.get(&region) else { return out };
        let owner = r.slots.get(slot).and_then(|s| s.holder).unwrap_or(r.owner);
        let Some(pos) = r.slots.get(slot).and_then(|s| s.position) else { return out };
        let settlement = r.settlement.position;
        let thieves: Vec<(CharacterId, FactionId)> = self
            .world
            .characters
            .values()
            .filter(|c| c.kind == CharacterKind::Gentleman && c.faction != owner && c.position == pos)
            .map(|c| (c.id, c.faction))
            .collect();
        if thieves.is_empty() {
            return out;
        }
        let mut skill: std::collections::BTreeMap<FactionId, i32> = std::collections::BTreeMap::new();
        for (c, f) in &thieves {
            let s = attribute(self, *c, 6);
            let e = skill.entry(*f).or_insert(0);
            *e = (*e + s).min(12);
        }
        let cost = self.rules.technologies.get(tech).map_or(0, |t| t.cost);
        for (c, f) in thieves {
            let u = self.rng.next16() as f32 * crate::rng::INV_65535;
            if u <= steal_chance(skill[&f], cost) {
                if let Some(d) = self.world.faction_details.get_mut(&f)
                    && let Some(t) = d.technologies.iter_mut().find(|(k, _)| k == tech)
                {
                    t.1 = super::research::state::RESEARCHED;
                }
                out.push((c, true));
                break;
            }
            if let Some(ch) = self.world.characters.get_mut(&c) {
                ch.position = settlement;
            }
            out.push((c, false));
        }
        out
    }
}

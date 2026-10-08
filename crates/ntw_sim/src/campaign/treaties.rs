//! Diplomacy rules on the relationship records: what each treaty and action does to the two
//! `DIPLOMACY_RELATIONSHIP` records of a pair (and to third parties), and the per-turn update.
//!
//! CONFIRMED from the exe unless tagged (CAMPAIGN_FIDELITY.md §Diplomacy has the addresses):
//! - **Attitude events** ([`ATTITUDE_EVENTS`]): 30 {limit, drift, value} triples. The original builds
//!   them at campaign load (`0x00B52070`) from built-in defaults (`0x0042F110`) overridden by the
//!   `diplomacy_attitudes` table, which the game does not ship, so the defaults are the values.
//! - **Factor writes** on one of the 24 factors ([`AttitudeFactor`]): *set* (`0x00B69640`: value, drift,
//!   limit, limited), *add* (`0x00B02CB0`: value += add, then drift / limit set and the value clamped
//!   toward the limit), *reset* (`0x00B69620`: value, no drift, unlimited).
//! - **Per-turn update** (`0x00B29100` → `0x00B29170`, from the round-end economy `0x008BC650` of each
//!   faction, after its regions): [`CampaignModel::diplomacy_round_end`].
//! - **Actions**: war `0x00B26700`, peace `0x00B262C0`, alliance `0x00B29DA0`, breaking an alliance
//!   `0x00B13840`, trade agreement `0x00B55130`, breaking trade `0x00B29BB0`, embargo `0x00B28DB0`,
//!   military access `0x00B44550` / cancel `0x00B67BD0`, state gift `0x00B44590`, protectorate
//!   `0x00B105C0`.
//!
//! Not here: the AI's acceptance of a deal (§6), calling allies into a war (`0x00B268B0` asks them:
//! an AI decision), money changing hands for regular payments and protectorate tribute (the
//! economy's income lines, `0x00BBE9A0` categories 0 / 2 / 6; not modelled).

use super::details::{AttitudeFactor, RegularPayment, Relationship, ATTITUDE_FACTORS};
use super::events::CampaignEvent;
use super::ids::FactionId;
use super::world::{CampaignModel, GovernmentType, Stance};
use super::CommandError;

/// An attitude event: {limit, drift, value} (the original's triple order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttitudeEvent {
    /// Where the drift stops.
    pub limit: i32,
    /// Change per turn.
    pub drift: i32,
    /// The value set (or added).
    pub value: i32,
}

const fn ev(limit: i32, drift: i32, value: i32) -> AttitudeEvent {
    AttitudeEvent { limit, drift, value }
}

/// The 30 attitude events in the original's order (config offset = 12 × index), with the built-in
/// values (`0x0042F110`, CONFIRMED; the `diplomacy_attitudes` table that could override them is not
/// shipped).
pub const ATTITUDE_EVENTS: [(&str, AttitudeEvent); 30] = [
    ("abandoned_ally_in_war", ev(0, 2, -25)),
    ("abused_military_access", ev(0, 1, -30)),
    ("alliance", ev(80, 1, 30)),
    ("alliance_broken", ev(0, 2, -40)),
    ("alliance_refuse_war_of_agression", ev(0, 0, -50)),
    ("allied_with_enemies", ev(0, 1, -20)),
    ("annexed_territory", ev(0, 2, 0)),
    ("assassination", ev(0, 2, -50)),
    ("backstabbing", ev(0, 1, 0)),
    ("break_alliance", ev(0, -2, 0)),
    ("break_trade", ev(0, -2, 0)),
    ("cultural_alliance_broken", ev(0, 1, 0)),
    ("government_type", ev(0, 2, 0)),
    ("initial_modifier", ev(0, 1, 0)),
    ("peace", ev(0, 2, 120)),
    ("peace_dragged_by_ally", ev(0, 2, 60)),
    ("sabotage", ev(0, 2, -20)),
    ("spying", ev(0, 2, -20)),
    ("state_gift", ev(0, -1, 100)),
    ("third_party_assassination", ev(0, 1, -5)),
    ("third_party_sabotage", ev(0, 1, -2)),
    ("third_party_spying", ev(0, 1, -2)),
    ("threatened", ev(0, 3, -30)),
    ("trade", ev(60, 1, 15)),
    ("trade_broken", ev(0, 2, -20)),
    ("war", ev(-200, -2, -140)),
    ("war_against_enemies", ev(0, -1, 15)),
    ("war_against_friends", ev(0, 1, -15)),
    ("war_dragged_by_ally", ev(-130, -2, -70)),
    ("trade_embargoed", ev(0, -2, -40)),
];

/// The event with this key (panics on an unknown key: the keys are fixed).
pub fn event(key: &str) -> AttitudeEvent {
    ATTITUDE_EVENTS.iter().find(|(k, _)| *k == key).map(|(_, e)| *e).expect("attitude event key")
}

/// The index of a factor in [`ATTITUDE_FACTORS`] (panics on an unknown key).
pub fn slot(key: &str) -> usize {
    ATTITUDE_FACTORS.iter().position(|k| *k == key).expect("attitude factor key")
}

/// The alliance commitment a new alliance starts with (`0x01458B74` = 20, CONFIRMED).
pub const ALLIANCE_COMMITMENT: u32 = 20;
/// `0x01458B70` (−10): per counted unit of an abused access.
const ABUSED_ACCESS_STEP: i32 = -10;
/// `0x01458B78` (−5): per alliance the breaker has already broken.
const BROKEN_ALLIANCE_STEP: i32 = -5;

impl AttitudeFactor {
    /// `0x00B69640`: set value, drift and limit.
    pub fn set(&mut self, e: AttitudeEvent) {
        self.value = e.value;
        self.drift = e.drift;
        self.limit = e.limit;
        self.limited = true;
    }

    /// `0x00B02CB0`: add to the value, set drift and limit, and clamp toward the limit.
    pub fn add(&mut self, add: i32, drift: i32, limit: i32) {
        self.value += add;
        self.drift = drift;
        self.limit = limit;
        self.limited = true;
        self.value = if drift > 0 { self.value.min(limit) } else { self.value.max(limit) };
    }

    /// `0x00B69620`: a fixed value without drift or limit.
    pub fn reset(&mut self, value: i32) {
        self.value = value;
        self.drift = 0;
        self.limited = false;
    }

    /// `0x00B290D0`: one turn of drift.
    pub fn step(&mut self) {
        self.value += self.drift;
        if self.limited {
            self.value = if self.drift > 0 { self.value.min(self.limit) } else { self.value.max(self.limit) };
        }
    }
}

impl Relationship {
    fn factor_mut(&mut self, key: &str) -> &mut AttitudeFactor {
        if self.attitudes.len() < ATTITUDE_FACTORS.len() {
            self.attitudes.resize(ATTITUDE_FACTORS.len(), AttitudeFactor::default());
        }
        &mut self.attitudes[slot(key)]
    }

    /// Sets a factor from an event.
    pub fn set_factor(&mut self, key: &str, e: AttitudeEvent) {
        self.factor_mut(key).set(e);
    }

    /// Adds an event's value to a factor.
    pub fn add_factor(&mut self, key: &str, e: AttitudeEvent) {
        self.factor_mut(key).add(e.value, e.drift, e.limit);
    }
}

/// The attitude category of a total (`0x00B0DBA0`): 0 hostile .. 4 very friendly. The boundaries are
/// half-way between the `diplomatic_relations_attitudes` rows (integer halves), the upper two minus 1:
/// with the shipped rows ≤ −65 → 0, ≤ −22 → 1, ≤ 21 → 2, ≤ 64 → 3, else 4.
pub fn attitude_category(thresholds: &std::collections::BTreeMap<String, i32>, total: i32) -> u8 {
    let t = |k: &str, d: i32| thresholds.get(k).copied().unwrap_or(d);
    let (h, u, n, f, v) = (t("hostile", -85), t("unfriendly", -45), t("neutral", 0), t("friendly", 45), t("very_friendly", 85));
    if total <= (h + u) / 2 {
        0
    } else if total <= (u + n) / 2 {
        1
    } else if total < (n + f) / 2 {
        2
    } else if total < (f + v) / 2 {
        3
    } else {
        4
    }
}

impl CampaignModel {
    /// The relationship record of `a` towards `b`, created (24 zeroed factors) when missing.
    pub fn relationship_mut(&mut self, a: FactionId, b: FactionId) -> &mut Relationship {
        let r = self.world.relationships.entry((a, b)).or_insert_with(|| Relationship {
            attitudes: vec![AttitudeFactor::default(); ATTITUDE_FACTORS.len()],
            allows_region_return: true,
            ..Default::default()
        });
        if r.attitudes.len() < ATTITUDE_FACTORS.len() {
            r.attitudes.resize(ATTITUDE_FACTORS.len(), AttitudeFactor::default());
        }
        r
    }

    /// The attitude category of `a` towards `b` ([`attitude_category`] of the relationship's total).
    pub fn attitude_category(&self, a: FactionId, b: FactionId) -> u8 {
        let total = self.world.relationships.get(&(a, b)).map_or(0, Relationship::attitude_total);
        attitude_category(&self.rules.attitude_thresholds, total)
    }

    fn same_subculture(&self, a: FactionId, b: FactionId) -> bool {
        let sub = |f: FactionId| self.world.factions.get(&f).and_then(|x| self.rules.faction_subcultures.get(&x.key));
        sub(a).is_some_and(|s| Some(s) == sub(b))
    }

    fn other_factions(&self, a: FactionId, b: FactionId) -> Vec<FactionId> {
        self.world.factions.keys().copied().filter(|&x| x != a && x != b).collect()
    }

    /// `a` declares war on `b` (`0x00B26700` with no ally: the relationship side of a war declaration;
    /// the stances go through [`World::declare_war`](super::World::declare_war)).
    /// - `a`→`b`: the `war` factor set from the `war` event; a patron loses `allows_region_return`; an
    ///   alliance, patronage or protectorate is broken first ([`Self::break_alliance`]); the access `a`
    ///   gives `b` ends; a trade agreement is broken ([`Self::break_trade`]); during friendship turns the
    ///   declarer is a backstabber (`backstab`); payments are dropped.
    /// - `b`→`a`: the `war` factor likewise; payments dropped; if `b` gave `a` military access, `a` is
    ///   charged with abusing it (`0x00B0DA90`).
    /// - every other faction X: if X's attitude to `b` is hostile (category 0), X likes `a` more
    ///   (`war_against_enemies` on X's `declared_war_against_enemies`); if very friendly (4), less
    ///   (`war_against_friends`). (The exe also skips X when `0x008CEE00` holds for it: UNKNOWN test.)
    pub fn declare_war_rules(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        if self.world.stance(a, b) == Stance::War {
            return Err(CommandError::AlreadyAtWar(a, b));
        }
        // Third parties judge by their attitude *before* the war changes anything.
        let thirds: Vec<(FactionId, u8)> = self.other_factions(a, b).into_iter().map(|x| (x, self.attitude_category(x, b))).collect();
        let stance = self.world.stance(a, b);
        let mut events = Vec::new();
        if matches!(stance, Stance::Allied | Stance::Patron | Stance::Protectorate) {
            if stance == Stance::Patron {
                self.relationship_mut(a, b).allows_region_return = false;
            }
            events.extend(self.break_alliance(a, b)?);
        }
        let had_trade = self.world.relationships.get(&(a, b)).is_some_and(|r| r.trade_agreement);
        if had_trade {
            events.extend(self.break_trade(a, b)?);
        }
        let friendship = self.world.relationships.get(&(a, b)).map_or(0, |r| r.friendship_turns);
        if friendship != 0 {
            self.backstab(a, b, friendship);
        }
        let war = event("war");
        {
            let r = self.relationship_mut(a, b);
            r.set_factor("war", war);
            r.military_access_turns = 0;
            r.payments.clear();
        }
        let gave_access = {
            let r = self.relationship_mut(b, a);
            r.set_factor("war", war);
            r.payments.clear();
            r.military_access_turns != 0
        };
        if gave_access {
            self.abuse_access(b, a, 0);
        }
        events.extend(self.world.declare_war(a, b)?);
        // The allies of both sides are called (0x00B268B0).
        events.extend(self.call_allies(a, b));
        for (x, cat) in thirds {
            match cat {
                0 => self.relationship_mut(x, a).add_factor("declared_war_against_enemies", event("war_against_enemies")),
                4 => self.relationship_mut(x, a).add_factor("declared_war_against_friends", event("war_against_friends")),
                _ => {}
            }
        }
        Ok(events)
    }

    /// `0x00B0E420`: `a` broke `b`'s trust during `turns` friendship turns. With `n` = the treaties `a`
    /// has broken so far (manager +0x1C): `b`'s `peace_treaty` factor is set to value −3·n·turns, drift of
    /// the `backstabbing` event, limit −5·n (`0x00B0E500`), every other faction of `b`'s subculture
    /// (other than `a`) gets value −5n·turns, limit −25n (`0x00B0E540`); then n grows by 1 (by 3 when
    /// `turns` is 10).
    fn backstab(&mut self, a: FactionId, b: FactionId, turns: u32) {
        let n = self.world.treaty_breaks.get(&a).copied().unwrap_or(0) as i32;
        let t = turns as i32;
        let drift = event("backstabbing").drift;
        self.relationship_mut(b, a).factor_mut("peace_treaty").set(ev(n * -5, drift, n * t * -3));
        for x in self.other_factions(a, b) {
            if self.same_subculture(x, b) {
                let m = n * 5;
                self.relationship_mut(x, a).factor_mut("peace_treaty").set(ev(m * -5, drift, -(m * t)));
            }
        }
        *self.world.treaty_breaks.entry(a).or_insert(0) += if turns == 10 { 3 } else { 1 };
    }

    /// `0x00B0DA90` / `0x00B0DAF0`: `a` (who had military access from `owner`) abused it: `owner`'s access
    /// to `a` ends with the cancel grievance (`0x00B67BD0`, [`Self::cancel_military_access`]), and
    /// `owner`'s `abused_military_access` factor gets the event's value + `count` × (−10)
    /// (`count` = `0x004CE450`, UNKNOWN: 0 here).
    fn abuse_access(&mut self, owner: FactionId, a: FactionId, count: i32) {
        self.cancel_military_access(owner, a);
        let e = event("abused_military_access");
        self.relationship_mut(owner, a).factor_mut("abused_military_access").add(e.value + ABUSED_ACCESS_STEP * count, e.drift, e.limit);
    }

    /// `a` and `b` make peace (`0x00B262C0` → `0x00B26590` on both records):
    /// the `war` factor gets the `peace` event added (`peace_dragged_by_ally` if the side was in the war
    /// for an ally, `war_ally`, which is then cleared); friendship 10 turns; the war counters (#10–#13)
    /// zeroed. The access an alliance had suspended for this enemy is restored (`0x00B0D0C0`).
    pub fn make_peace_rules(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        let events = self.world.make_peace(a, b)?;
        for (x, y) in [(a, b), (b, a)] {
            let r = self.relationship_mut(x, y);
            let e = if r.war_ally.is_some() { event("peace_dragged_by_ally") } else { event("peace") };
            r.add_factor("war", e);
            r.war_ally = None;
            r.friendship_turns = 10;
            r.war_region_balance = 0;
            r.war_wealth_balance = 0;
            r.war_turns = 0;
            r.turns_since_battle = 0;
        }
        // 0x00B0D0C0: each ally of `a` that suspended its access to `a` for this war gets it back.
        for (owner, target) in [(a, b), (b, a)] {
            let allies: Vec<FactionId> = self.world.factions.keys().copied().filter(|&x| x != owner && matches!(self.world.stance(owner, x), Stance::Allied | Stance::Patron | Stance::Protectorate)).collect();
            for ally in allies {
                for (p, q) in [(owner, ally), (ally, owner)] {
                    let r = self.relationship_mut(p, q);
                    if let Some(i) = r.allied_in_war_against.iter().position(|w| w.enemy == target) {
                        r.military_access_turns = r.allied_in_war_against[i].saved_access_turns;
                        r.allied_in_war_against.remove(i);
                    }
                }
            }
        }
        Ok(events)
    }

    /// `a` and `b` become allies (`0x00B29DA0` on both records): the `alliance` factor set from the
    /// `alliance` event, the stance allied, the alliance commitment 20 turns, friendship 10.
    pub fn form_alliance(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.world.set_stance(a, b, Stance::Allied)?;
        for (x, y) in [(a, b), (b, a)] {
            let r = self.relationship_mut(x, y);
            r.set_factor("alliance", event("alliance"));
            r.alliance_commitment_turns = ALLIANCE_COMMITMENT;
            r.friendship_turns = 10;
        }
        Ok(vec![CampaignEvent::StanceChanged { a, b, stance: Stance::Allied }])
    }

    /// `a` breaks its alliance (or patronage / protectorate) with `b` (`0x00B13840`):
    /// - while `a`'s commitment to `b` (#6) runs, every other faction of `a`'s or `b`'s subculture sets its
    ///   `cultural_alliance_broken` factor towards `a` to value −commitment − 5·k, limit the event's limit
    ///   − 5·k (k = the alliances `a` broke before, manager +0x14, then k + 1) (`0x00B25FA0`);
    /// - `a`→`b` (`0x00B13A30`): the `alliance` factor set from the `break_alliance` event; stance neutral
    ///   (unless patron); commitment 0; the first `allied_in_war_against` item gives back its access;
    /// - `b`→`a` (`0x00B0CA20`): the `alliance` factor reset to 0, `alliance_broken` set from its event;
    ///   stance neutral; commitment 0; the protectorate income line cleared; access restored likewise.
    pub fn break_alliance(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        let commitment = self.world.relationships.get(&(a, b)).map_or(0, |r| r.alliance_commitment_turns) as i32;
        if commitment != 0 {
            let k = self.world.alliances_broken.get(&a).copied().unwrap_or(0) as i32;
            let e = event("cultural_alliance_broken");
            for x in self.other_factions(a, b) {
                if self.same_subculture(x, a) || self.same_subculture(x, b) {
                    let r = self.relationship_mut(x, a);
                    r.factor_mut("cultural_alliance_broken").set(ev(BROKEN_ALLIANCE_STEP * k + e.limit, e.drift, BROKEN_ALLIANCE_STEP * k - commitment));
                }
            }
            *self.world.alliances_broken.entry(a).or_insert(0) += 1;
        }
        let restore = |r: &mut Relationship| {
            r.alliance_commitment_turns = 0;
            if let Some(w) = r.allied_in_war_against.first() {
                r.military_access_turns = w.saved_access_turns;
            }
            r.allied_in_war_against.clear();
        };
        {
            let r = self.relationship_mut(a, b);
            r.set_factor("alliance", event("break_alliance"));
            restore(r);
        }
        {
            let r = self.relationship_mut(b, a);
            r.factor_mut("alliance").reset(0);
            r.set_factor("alliance_broken", event("alliance_broken"));
            r.protectorate_income = 0;
            r.protectorate_tribute = 0;
            restore(r);
        }
        self.world.set_stance(a, b, Stance::Neutral)?;
        Ok(vec![CampaignEvent::StanceChanged { a, b, stance: Stance::Neutral }])
    }

    /// A trade agreement between `a` and `b` (`0x00B55130` on both records): the `trade` factor set from the
    /// `trade` event, the agreement flag (#2) set. (The original also builds the trade route, `0x00BA0490`;
    /// the model's routes follow the flag, see [`trade`](super::trade).)
    pub fn trade_agreement(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        for (x, y) in [(a, b), (b, a)] {
            let r = self.relationship_mut(x, y);
            r.set_factor("trade", event("trade"));
            r.trade_agreement = true;
        }
        Ok(Vec::new())
    }

    /// `a` breaks its trade agreement with `b` (`0x00B29BB0`): `a`→`b` (`0x00B13C30`) `trade_broken` set from
    /// the `break_trade` event and `trade` cleared; `b`→`a` (`0x00B733B0`) `trade` cleared and `trade_broken`
    /// set from the `trade_broken` event; the flag cleared on both.
    pub fn break_trade(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        let zero = ev(0, 0, 0);
        {
            let r = self.relationship_mut(a, b);
            r.set_factor("trade_broken", event("break_trade"));
            r.set_factor("trade", zero);
            r.trade_agreement = false;
        }
        {
            let r = self.relationship_mut(b, a);
            r.set_factor("trade", zero);
            r.set_factor("trade_broken", event("trade_broken"));
            r.trade_agreement = false;
        }
        self.world.trade_paths.remove(&(a, b));
        self.world.trade_paths.remove(&(b, a));
        Ok(Vec::new())
    }

    /// `a` embargoes `b` (`0x00B28DB0`): a trade agreement is broken first; `a`'s embargo turns (#27) are
    /// 10 (`0x00B28E00`, S1_LEFTOVERS.md); `b`'s `trade_embargoed` factor is set from its event (value −40,
    /// drift −2, limit 0: with the original's clamp the value is 0 after one turn — as the exe does it).
    pub fn embargo(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        if self.world.relationships.get(&(a, b)).is_some_and(|r| r.trade_agreement) {
            self.break_trade(a, b)?;
        }
        self.relationship_mut(a, b).trade_embargo_turns = 10;
        self.relationship_mut(b, a).set_factor("trade_embargoed", event("trade_embargoed"));
        Ok(Vec::new())
    }

    /// `a` gives `b` military access for `turns` turns (−1: indefinitely) (`0x00B444B0` → `0x00B44550`):
    /// #3 becomes −1, or grows by `turns`; #24 the granted length; friendship 10; #25 elapsed 0.
    pub fn grant_military_access(&mut self, a: FactionId, b: FactionId, turns: i32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        let r = self.relationship_mut(a, b);
        r.military_access_turns = if turns == -1 { -1 } else { r.military_access_turns + turns };
        r.military_access_granted = r.military_access_turns;
        r.friendship_turns = 10;
        r.military_access_elapsed = 0;
        Ok(Vec::new())
    }

    /// `a` withdraws the military access it gives `b` (`0x00B67BD0`): the cancel grievance (#26) grows by
    /// base − used share by the granted length (5: 50 − 10·elapsed; 10: 60 − 6·elapsed; 20: 70 − 7·elapsed/2;
    /// other: 90 − 5·elapsed/2, unsigned halves) and the access ends. Returns the grievance added (the
    /// command path, `0x00B67B20`, also adds a third of it to `a`'s manager +0x20: [`World::access_cancel_marks`](super::World::access_cancel_marks)).
    pub fn cancel_military_access(&mut self, a: FactionId, b: FactionId) -> i32 {
        let r = self.relationship_mut(a, b);
        let e = r.military_access_elapsed;
        let (base, used) = match r.military_access_granted {
            5 => (50, e * 10),
            10 => (60, e * 6),
            20 => (70, (e * 7) >> 1),
            _ => (90, (e * 5) >> 1),
        };
        let g = (base as u32).wrapping_sub(used) as i32;
        r.access_cancel_grievance = r.access_cancel_grievance.wrapping_add(g as u32);
        r.military_access_turns = 0;
        g
    }

    /// `giver` gives `receiver` `amount` money as a state gift (`0x00B44590` → `0x00B446B0`): x = linear ×
    /// amount + quadratic × amount² / √(GDP of both), at most 100; the receiver's `state_gift` factor
    /// towards the giver gets trunc((100 − its current contribution) × x × 0.01) added, drift and limit of
    /// the `state_gift` event. The money moves from giver to receiver (the caller `0x00C4B440`; INFERRED).
    pub fn state_gift(&mut self, giver: FactionId, receiver: FactionId, amount: i32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(giver, receiver)?;
        let f = self.world.factions.get(&giver).ok_or(CommandError::UnknownFaction(giver))?;
        if amount <= 0 || f.treasury < amount {
            return Err(CommandError::InsufficientFunds { needed: amount, available: f.treasury });
        }
        let gdp = super::economy::faction_gdp(self, giver) + super::economy::faction_gdp(self, receiver);
        let lin = self.rules.var("state_gift_multiplier_linear", 0.002);
        let quad = self.rules.var("state_gift_multiplier_quadratic", 0.00002);
        let sq = (amount as f32) * (amount as f32);
        let x = (lin * amount as f32 + quad * sq / (gdp.max(1) as f32).sqrt()).min(100.0) as i32;
        let e = event("state_gift");
        let r = self.relationship_mut(receiver, giver);
        let current = r.factor_mut("state_gift").contribution();
        let add = ((100.0 - current as f32) * x as f32 * 0.01) as i32;
        r.factor_mut("state_gift").add(add, e.drift, e.limit);
        if let Some(g) = self.world.factions.get_mut(&giver) {
            g.treasury -= amount;
        }
        if let Some(r) = self.world.factions.get_mut(&receiver) {
            r.treasury += amount;
        }
        Ok(Vec::new())
    }

    /// `payer` agrees to pay `payee` `amount` a turn for `turns` turns: a `REGULAR_PAYMENTS` item on the payer's
    /// record (direction INFERRED), and the friendship raised to min(turns, 10) (S1_LEFTOVERS.md #15).
    pub fn regular_payment(&mut self, payer: FactionId, payee: FactionId, amount: i32, turns: u32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(payer, payee)?;
        let r = self.relationship_mut(payer, payee);
        r.payments.push(RegularPayment { amount, turns });
        r.friendship_turns = r.friendship_turns.max(turns.min(10));
        Ok(Vec::new())
    }

    /// `protectorate` becomes the protectorate of `patron` (`0x00B105C0`): the protectorate breaks its
    /// other protectorate and patron ties first (`0x00B104E0`); `protectorate`→`patron` stance 4 with the
    /// alliance factor set (`0x00B29DA0` unless already allied), `patron`→`protectorate` stance 3 likewise
    /// (`0x00B10480`); the patron's other alliances break (the alliances of `patron` with stance 2,
    /// `0x00B13840`, when not the protectorate). Tribute: `0x00BBCD00` = a faction value / 5 (the value,
    /// `0x00B803A0`, is not read: the tribute line is left at 0, PROVISIONAL).
    pub fn make_protectorate(&mut self, protectorate: FactionId, patron: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(protectorate, patron)?;
        let mut events = Vec::new();
        let ties: Vec<FactionId> = self
            .world
            .factions
            .keys()
            .copied()
            .filter(|&x| x != protectorate && x != patron && matches!(self.world.stance(protectorate, x), Stance::Protectorate | Stance::Patron))
            .collect();
        for x in ties {
            events.extend(self.break_alliance(protectorate, x)?);
        }
        for (x, y) in [(protectorate, patron), (patron, protectorate)] {
            let allied = self.world.stance(x, y) == Stance::Allied;
            let r = self.relationship_mut(x, y);
            if !allied {
                r.set_factor("alliance", event("alliance"));
                r.alliance_commitment_turns = ALLIANCE_COMMITMENT;
                r.friendship_turns = 10;
            }
        }
        self.world.set_stance(protectorate, patron, Stance::Protectorate)?;
        events.push(CampaignEvent::StanceChanged { a: protectorate, b: patron, stance: Stance::Protectorate });
        Ok(events)
    }

    fn check_diplomacy_pair(&self, a: FactionId, b: FactionId) -> Result<(), CommandError> {
        for id in [a, b] {
            if !self.world.factions.contains_key(&id) {
                return Err(CommandError::UnknownFaction(id));
            }
        }
        if a == b {
            return Err(CommandError::SameFaction(a));
        }
        Ok(())
    }

    /// The per-turn update of `faction`'s relationships (`0x00B29100` → `0x00B29170` per record), at the
    /// faction's round-end economy (`0x008BC650`, after its regions; CONFIRMED place). Per record (skipped
    /// when the target is out of the game):
    /// 1. every factor drifts ([`AttitudeFactor::step`]);
    /// 2. the alliance commitment (#6) −1;
    /// 3. military access (#3): timed access counts down (#25 elapsed +1, #19 streak +1); indefinite access
    ///    keeps the streak growing; none resets it;
    /// 4. `allied_in_war_against`: items of factions out of the game dropped, saved access turns −1;
    /// 5. friendship (#15) −1, then 10 again while allied (stance 2–4) or while the target gives the owner
    ///    military access;
    /// 6. at war: the war balances #10 and #11 (`war_balances`), #12 and #13 +1;
    /// 7. war momentum (#7): positive −2 (to 0), negative +1 (to 0);
    /// 8. regular payments: turns −1, finished ones removed (the money is an economy line: not modelled);
    /// 9. the cancel grievance (#26) −2 (to 0); embargo turns (#27) −1.
    ///
    /// Then the manager's own counter (+0x20, [`World::access_cancel_marks`](super::World::access_cancel_marks)) −1.
    pub fn diplomacy_round_end(&mut self, faction: FactionId) {
        let targets: Vec<FactionId> = self.world.relationships.keys().filter(|(o, _)| *o == faction).map(|(_, t)| *t).collect();
        let alive = |m: &CampaignModel, f: FactionId| m.world.regions.values().any(|r| r.owner == f) || m.world.forces.values().any(|x| x.faction == f);
        // A faction out of the game updates nothing (0x00B29100 tests the owner's +0x824 first; INFERRED meaning:
        // no regions and no forces). The vanilla saves agree: such factions' records do not drift.
        if !alive(self, faction) {
            return;
        }
        for t in targets {
            if !alive(self, t) {
                continue;
            }
            let at_war = self.world.stance(faction, t) == Stance::War;
            let allied = matches!(self.world.stance(faction, t), Stance::Allied | Stance::Patron | Stance::Protectorate);
            let given_access = self.world.relationships.get(&(t, faction)).is_some_and(|r| r.military_access_turns != 0);
            let balances = if at_war { Some(self.war_balances(faction, t)) } else { None };
            let dead: Vec<FactionId> = self
                .world
                .relationships
                .get(&(faction, t))
                .map(|r| r.allied_in_war_against.iter().map(|w| w.enemy).filter(|&e| !alive(self, e)).collect())
                .unwrap_or_default();
            let Some(r) = self.world.relationships.get_mut(&(faction, t)) else { continue };
            for f in &mut r.attitudes {
                f.step();
            }
            r.alliance_commitment_turns = r.alliance_commitment_turns.saturating_sub(1);
            match r.military_access_turns {
                n if n > 0 => {
                    r.military_access_elapsed += 1;
                    r.military_access_streak += 1;
                    r.military_access_turns = n - 1;
                }
                -1 => r.military_access_streak += 1,
                _ => r.military_access_streak = 0,
            }
            r.allied_in_war_against.retain(|w| !dead.contains(&w.enemy));
            for w in &mut r.allied_in_war_against {
                if w.saved_access_turns > 0 {
                    w.saved_access_turns -= 1;
                }
            }
            r.friendship_turns = r.friendship_turns.saturating_sub(1);
            if allied || given_access {
                r.friendship_turns = 10;
            }
            if let Some((regions, wealth)) = balances {
                r.war_region_balance = regions;
                r.war_wealth_balance = wealth;
                r.war_turns += 1;
                r.turns_since_battle += 1;
            }
            r.war_momentum = if r.war_momentum > 0 { (r.war_momentum - 2).max(0) } else { (r.war_momentum + 1).min(0) };
            for p in &mut r.payments {
                p.turns = p.turns.saturating_sub(1);
            }
            r.payments.retain(|p| p.turns != 0);
            r.access_cancel_grievance -= r.access_cancel_grievance.min(2);
            r.trade_embargo_turns = r.trade_embargo_turns.saturating_sub(1);
        }
        if let Some(n) = self.world.access_cancel_marks.get_mut(&faction) {
            *n = n.saturating_sub(1);
        }
    }

    /// The two war balances of `owner` against `target` (#10, #11 of the per-turn update, `0x00B29170`):
    /// - #10 = (value of `owner`'s armies standing in `target`'s regions − value of `target`'s armies in
    ///   `owner`'s regions) / 1000, clamped to ±10;
    /// - #11 = (GDP of `owner` + value of all its armies − value of all `target`'s armies − GDP of `target`)
    ///   / 2500 (`0x008C3110` GDP), clamped to ±10.
    ///
    /// CONFIRMED shape (two loops over the factions' army lists, `0x009D3A60` / `0x00A1BDF0` / `0x006649C0`);
    /// PROVISIONAL: an army's value (`0x008F9C50(1)`, not read) is the recruitment cost of its units, and
    /// "standing in a region" is the map region under its commander (the settlement it garrisons without a map).
    fn war_balances(&self, owner: FactionId, target: FactionId) -> (i32, i32) {
        let value = |f: &super::world::MilitaryForce| -> i64 {
            f.units.iter().map(|u| self.rules.units.get(&u.unit_key).map_or(0, |x| i64::from(x.cost))).sum()
        };
        let region_owner = |f: &super::world::MilitaryForce| -> Option<FactionId> {
            let c = self.world.characters.get(&f.commander?)?;
            if let Some(r) = c.garrisoned_in {
                return self.world.regions.get(&r).map(|r| r.owner);
            }
            let t = self.terrain.as_ref()?;
            let key = t.0.region_at(c.position.0.to_f32(), c.position.1.to_f32())?;
            self.world.regions.values().find(|r| r.key == key).map(|r| r.owner)
        };
        let (mut inside_owner, mut inside_target, mut all_owner, mut all_target) = (0i64, 0i64, 0i64, 0i64);
        for f in self.world.forces.values().filter(|f| !f.is_navy) {
            if f.faction == owner {
                all_owner += value(f);
                if region_owner(f) == Some(target) {
                    inside_owner += value(f);
                }
            } else if f.faction == target {
                all_target += value(f);
                if region_owner(f) == Some(owner) {
                    inside_target += value(f);
                }
            }
        }
        let regions = ((inside_owner - inside_target) / 1000).clamp(-10, 10) as i32;
        let gdp = |f: FactionId| super::economy::faction_gdp(self, f);
        let wealth = ((gdp(owner) + (all_owner - all_target) - gdp(target)) / 2500).clamp(-10, 10) as i32;
        (regions, wealth)
    }
}

/// A diplomatic action of one faction towards another (the items of the original's negotiation and
/// its diplomacy buttons), applied by [`CampaignModel::apply_diplomatic_action`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiplomaticAction {
    /// Form an alliance.
    Alliance,
    /// Break the alliance (or patronage / protectorate).
    BreakAlliance,
    /// Make a trade agreement.
    TradeAgreement,
    /// Break the trade agreement.
    BreakTrade,
    /// Embargo the other side's trade.
    Embargo,
    /// Give the other side military access for some turns (−1: indefinitely).
    GrantMilitaryAccess(i32),
    /// Withdraw the military access given to the other side.
    CancelMilitaryAccess,
    /// Give money as a state gift.
    StateGift(i32),
    /// Pay an amount each turn for some turns.
    RegularPayment(i32, u32),
    /// Become the other side's protectorate.
    BecomeProtectorate,
}

impl CampaignModel {
    /// Applies a diplomatic action of `a` towards `b` (see [`DiplomaticAction`]).
    pub fn apply_diplomatic_action(&mut self, a: FactionId, b: FactionId, action: DiplomaticAction) -> Result<Vec<CampaignEvent>, CommandError> {
        let treaty = matches!(action, DiplomaticAction::Alliance | DiplomaticAction::TradeAgreement | DiplomaticAction::GrantMilitaryAccess(_) | DiplomaticAction::BecomeProtectorate);
        if treaty && self.world.stance(a, b) == Stance::War {
            return Err(CommandError::Unsupported("the two factions are at war"));
        }
        match action {
            DiplomaticAction::Alliance => self.form_alliance(a, b),
            DiplomaticAction::BreakAlliance => {
                if !matches!(self.world.stance(a, b), Stance::Allied | Stance::Patron | Stance::Protectorate) {
                    return Err(CommandError::Unsupported("not allied"));
                }
                self.break_alliance(a, b)
            }
            DiplomaticAction::TradeAgreement => self.trade_agreement(a, b),
            DiplomaticAction::BreakTrade => self.break_trade(a, b),
            DiplomaticAction::Embargo => self.embargo(a, b),
            DiplomaticAction::GrantMilitaryAccess(turns) => self.grant_military_access(a, b, turns),
            DiplomaticAction::CancelMilitaryAccess => {
                self.check_diplomacy_pair(a, b)?;
                // 0x00B67B20: a third of the grievance stays on the canceller's manager (+0x20).
                let g = self.cancel_military_access(a, b);
                *self.world.access_cancel_marks.entry(a).or_insert(0) += (g / 3).max(0) as u32;
                Ok(Vec::new())
            }
            DiplomaticAction::StateGift(amount) => self.state_gift(a, b, amount),
            DiplomaticAction::RegularPayment(amount, turns) => self.regular_payment(a, b, amount, turns),
            DiplomaticAction::BecomeProtectorate => self.make_protectorate(a, b),
        }
    }
}

impl CampaignModel {
    /// The computed attitude factors at the round start (`0x0096C050` → `0x008BAF30` → `0x00B71E90` →
    /// `0x00B71EC0` per relationship, CONFIRMED): for every relationship of a faction still in the game, the
    /// `faction_leader` factor is reset to trunc(the target's faction effect `diplomacy_bonus_faction_leader`,
    /// 0x52) and `enlightenment` to trunc(its `diplomacy_bonus_enlightenment`, 0x7E). Checked on the vanilla
    /// saves: every enlightenment factor, 484 / 506 leader factors (France's leader: the model sums −9 where the
    /// saves hold +9; open).
    pub fn refresh_computed_factors(&mut self) {
        let fx = super::effects::Effects::compute(self);
        let alive: Vec<FactionId> = self.world.factions.keys().copied().filter(|&f| self.in_the_game(f)).collect();
        let keys: Vec<(FactionId, FactionId)> = self.world.relationships.keys().copied().filter(|(o, _)| alive.contains(o)).collect();
        for (o, t) in keys {
            let leader = fx.faction(t, "diplomacy_bonus_faction_leader") as i32;
            let enlightenment = fx.faction(t, "diplomacy_bonus_enlightenment") as i32;
            let r = self.relationship_mut(o, t);
            r.factor_mut("faction_leader").reset(leader);
            r.factor_mut("enlightenment").reset(enlightenment);
        }
    }

    /// The religion and government factors of a relationship (`0x00B1B540`: reset to the
    /// `diplomatic_relations_religion` #2 attitude for (owner religion, target religion); government: the
    /// `diplomatic_relations_government_type` #3 value for (owner government, target government)). Both CONFIRMED
    /// on the vanilla saves (506 / 506 each). A government change (`0x00B1B5A0`) instead sets the factor
    /// drifting toward #2 (2 a turn, INFERRED): [`Self::change_government`].
    pub fn setup_relationship_factors(&mut self, a: FactionId, b: FactionId) {
        let rel = |m: &CampaignModel, f: FactionId| m.world.faction_details.get(&f).map(|d| d.religion.clone()).unwrap_or_default();
        let gov = |m: &CampaignModel, f: FactionId| m.world.factions.get(&f).map(|x| x.government_key.clone()).unwrap_or_default();
        let religion = self.rules.religion_attitudes.get(&(rel(self, a), rel(self, b))).copied().unwrap_or(0);
        let government = self.rules.government_relations.get(&(gov(self, a), gov(self, b))).map_or(0, |x| x.1);
        let r = self.relationship_mut(a, b);
        r.factor_mut("religion").reset(religion);
        r.factor_mut("government_type").reset(government);
    }

    /// A faction's government changes (`0x00B1B5A0`, CONFIRMED from the bytes at `0x00B1B5A0`
    /// .. `0x00B1B6A4`; the round 12 port had the value and the limit the wrong way round, see the
    /// ledger in `analysis/fidelity/CAMPAIGN_FIDELITY.md`). The `government_type` attitude factor of
    /// every record that has the faction as its **target** — the counterpart's attitude towards the
    /// faction — is set to the new government's row of `diplomatic_relations_government_type` with
    /// drift and limit turned on, so that the per-turn update ([`Self::diplomacy_round_end`], run by
    /// the counterpart) walks it from the shocked value to the steady one.
    ///
    /// `0x00B1B5A0` in full (the relationship record in `ECX`, the new government key in the argument):
    ///
    /// ```text
    /// row = government_types_table.record_index(own_gov + SEP + new_gov);  // 0x00B1B5C6..0x00B1B61B
    /// mag = (row->limit < row->value) ? -events[0x94] : +events[0x94];    // 0x00B1B665..0x00B1B67E
    /// record[0x288].set(value = row->value, drift = mag, limit = row->limit, limited = 1);
    /// ```
    ///
    /// - **The row is keyed on the PAIR of governments, not on one** (0-B round 14, CONFIRMED twice
    ///   over; this corrects the round 13 transcription, which read `record_index(new_government_key)`
    ///   and left the question open). `record_index` (`0x0047AA40`, `RET 4`) does take a single
    ///   `UniString`, but it is the table's **composite** key: `0x00B1B5C6` pushes the separator
    ///   constant `0x013305F8`, `0x00B1B5CB`/`0x00B1B5D1` read the record **owner's** government key
    ///   (its faction +0x70C's string at +0xB0) and `0x00B1B5F1` the record **target's** new one, and
    ///   the two `0x004F1200` calls splice them into one string for `0x00B1B61B`. The own-side twin
    ///   `0x00B1B190` builds the key the same way (`0x00B1B1AC` the separator, `0x00B1B1D3`
    ///   `0x004B4A50` the other faction, `0x00B1B20A` the lookup), and the setup writer
    ///   `0x00B45A40` likewise. The data says the same: the table has 16 rows (4 government types x
    ///   4) and **both #2 and #3 take four different values within a single column** - e.g. with
    ///   `gov_republic` as the target, #2 is -100 / -50 / +70 / 0 and #3 is -30 / -15 / +30 / 0 as the
    ///   owner varies. A function of one government cannot produce that.
    /// - **The drift a turn is CONFIRMED, not assumed**: `0x00B1B66F` / `0x00B1B67E` read
    ///   `[campaign_model + 0xFAC + 0x94]`, the attitude events array (`0x00B27FF0` → `0x008BAF10`
    ///   returns `*(this + 0xFAC)`). The array is 30 triples of 12 bytes (`0x00B52070` copies 0x5A
    ///   dwords) in the order of [`ATTITUDE_EVENTS`], whose triple is {limit, drift, value}
    ///   (`0x00518210`: `*ecx = p1; ecx[1] = p2; ecx[2] = p3`; `0x00B45A40` then sets
    ///   `(events[war].value, events[war].drift, events[war].limit)`), so offset `0x94` = 12 × 12 + 4
    ///   is the **drift of triple 12, `government_type`**, whose built-in drift is 2 (`0x0042F110`).
    /// - **The value and the limit are the two columns, in that order**: `0x00B1B684` pushes
    ///   `[row + 0xC]` and `0x00B1B686` pushes `[row + 8]` as the last two arguments of
    ///   `0x00B69640`, whose listing (`*ptr(ECX+0x1C) = arg1`, `*ptr(ECX+0x18) = arg2`,
    ///   `*ptr(ECX+0x20) = arg3`, `*ptr(ECX+0x24) = 1`) is `(value, drift, limit)`. So
    ///   **value = `diplomatic_relations_government_type` #2 and limit = #3** — the opposite way round
    ///   from [`Self::setup_relationship_factors`], which resets the factor to the fixed #3
    ///   (`0x00B45A40` pushes `[row + 0xC]` into `0x00B69620`). A government change shocks the factor
    ///   to #2 and lets it recover to #3, which is what the shipped rows describe (absolute monarchy
    ///   towards republic −100 → −30).
    /// - **The sign** is `+` unless the limit is below the value (`0x00B1B665` `CMP EDI,[ESI + 8]`,
    ///   `JGE` to the positive branch), so it is **+2** on every shipped row. The raw event drift is
    ///   used, not its magnitude.
    /// - **One direction only.** The single caller `0x00B1B100` walks the changed faction's own
    ///   relationship records and, for each, looks the counterpart's own record up with `0x00B64C50`
    ///   (the first record of the counterpart's list whose **target** is the changed faction) before
    ///   calling `0x00B1B5A0` on it — so the factor written is the counterpart's attitude towards the
    ///   changed faction. The record the changed faction owns is *not* touched here; `0x00B1B190`
    ///   (its twin, called once per owned record) sets only that record's **drift and limit**
    ///   (`0x00B69A70`), from the counterpart's government row, and leaves its value alone. That
    ///   second pass is not modelled (it needs the counterpart's government row on its own; see the
    ///   ledger), so no factor on the changed faction's own side moves.
    /// - No in-the-game test: `0x00B1B5A0` writes whatever records exist, and a counterpart out of the
    ///   game only stops the per-turn drift (`0x00B29170` tests the target's `+0x824`).
    /// - The new government must be one the model has a [`GovernmentType`] for
    ///   ([`GovernmentType::from_db_key`]): `government_types`' 4th key, `gov_empire`, is rejected.
    pub fn change_government(&mut self, faction: FactionId, new_government_key: &str) -> Result<Vec<CampaignEvent>, CommandError> {
        let Some(old) = self.world.factions.get(&faction) else {
            return Err(CommandError::UnknownFaction(faction));
        };
        let old_government_key = old.government_key.clone();
        let Some(government) = GovernmentType::from_db_key(new_government_key) else {
            return Err(CommandError::Unsupported("government type not modelled"));
        };
        if let Some(f) = self.world.factions.get_mut(&faction) {
            f.government_key = new_government_key.to_string();
            f.government = government;
        }
        let gov = |m: &CampaignModel, f: FactionId| m.world.factions.get(&f).map(|x| x.government_key.clone()).unwrap_or_default();
        // The attitude event's own drift, as read at `0x00B1B66F`, not its magnitude.
        let step = event("government_type").drift;
        // The records that have the changed faction as their TARGET: `0x00B1B100` resolves each of them
        // with `0x00B64C50`, which matches on the record's target field.
        let pairs: Vec<(FactionId, FactionId)> = self.world.relationships.keys().copied().filter(|(_, t)| *t == faction).collect();
        for (o, t) in pairs {
            // `government_relations` is (own government, the record's target government) -> (#2, #3);
            // the row used here is the new government's.
            let (value, limit) = self.rules.government_relations.get(&(gov(self, o), new_government_key.to_string())).copied().unwrap_or_default();
            // `0x00B1B665`: negative only when the limit is below the value.
            let drift = if limit < value { -step } else { step };
            self.relationship_mut(o, t)
                .factor_mut("government_type")
                .set(AttitudeEvent { limit, drift, value });
        }
        Ok(vec![CampaignEvent::GovernmentChanged {
            faction,
            old_government: old_government_key,
            new_government: new_government_key.to_string(),
        }])
    }

    /// A faction still in the game (the exe's faction +0x824 clear; INFERRED: it holds a region or a force).
    pub fn in_the_game(&self, f: FactionId) -> bool {
        self.world.regions.values().any(|r| r.owner == f) || self.world.forces.values().any(|x| x.faction == f)
    }

    /// The money of treaties at `faction`'s round-end economy (PROVISIONAL place; INFERRED direction):
    /// - regular payments: the items on `faction`'s record towards a target move their amount from `faction` to
    ///   the target each turn (the economy line category 0 holds the sum of the active items, `0x00B29170`);
    /// - protectorate tribute: a protectorate pays its patron a fifth of its revenue (`0x00BBCD00` = the sum of
    ///   three economics lines / 5, read as taxes + trade + other), economy lines 6 (protectorate) and 2 (patron),
    ///   kept in #8 / #9.
    pub fn diplomacy_money(&mut self, faction: FactionId) {
        let targets: Vec<FactionId> = self.world.relationships.keys().filter(|(o, _)| *o == faction).map(|(_, t)| *t).collect();
        for t in targets {
            if !self.in_the_game(t) {
                continue;
            }
            let pay: i32 = self.world.relationships.get(&(faction, t)).map_or(0, |r| r.payments.iter().map(|p| p.amount).sum());
            let tribute = if self.world.stance(faction, t) == Stance::Protectorate {
                super::economy::faction_income(self, faction).revenue().max(0) / 5
            } else {
                0
            };
            if tribute != 0 || self.world.relationships.get(&(faction, t)).is_some_and(|r| r.protectorate_tribute != 0) {
                self.relationship_mut(faction, t).protectorate_tribute = tribute;
                self.relationship_mut(t, faction).protectorate_income = tribute;
            }
            let total = pay.saturating_add(tribute);
            if total != 0 {
                if let Some(f) = self.world.factions.get_mut(&faction) {
                    f.treasury = f.treasury.saturating_sub(total);
                }
                if let Some(f) = self.world.factions.get_mut(&t) {
                    f.treasury = f.treasury.saturating_add(total);
                }
            }
        }
    }

    /// The allies of both sides called into `a`'s war on `b` (`0x00B268B0`, CONFIRMED flow): every faction X
    /// allied to `b` (stance allied, patron or protectorate), in the game, not `a`, not already at war with `a`:
    /// a human X gets an offer (UI event `+0xB10`; not modelled: a human ally is not called); an AI X decides
    /// through the AI (`0x00AAA950` → `0x00AAA920`, §6). Join: X declares war on `a` on `b`'s behalf
    /// ([`Self::join_war`]). Refuse: X breaks its alliance with `b` (`0x00B27500`, `0x00B13840`). Then the same
    /// for `a`'s allies against `b`. PROVISIONAL AI decision: join, unless X is allied to the enemy as well
    /// (then it refuses).
    pub fn call_allies(&mut self, a: FactionId, b: FactionId) -> Vec<CampaignEvent> {
        let mut events = Vec::new();
        for (side, enemy) in [(b, a), (a, b)] {
            let allies: Vec<FactionId> = self
                .world
                .factions
                .keys()
                .copied()
                .filter(|&x| x != a && x != b && self.in_the_game(x))
                .filter(|&x| matches!(self.world.stance(x, side), Stance::Allied | Stance::Patron | Stance::Protectorate))
                .filter(|&x| self.world.stance(x, enemy) != Stance::War)
                .collect();
            for x in allies {
                if self.turn.humans.contains(&x) {
                    continue;
                }
                let torn = matches!(self.world.stance(x, enemy), Stance::Allied | Stance::Patron | Stance::Protectorate);
                let result = if torn { self.break_alliance(x, side) } else { self.join_war(x, enemy, side) };
                if let Ok(mut e) = result {
                    events.append(&mut e);
                }
            }
        }
        events
    }

    /// `x` goes to war with `enemy` on `ally`'s behalf (`0x00B26700` with an ally: `0x00B27070` sets the
    /// `war_dragged_by_ally` event and #5 = `ally`), and `0x00B0CCE0` on (x, ally) and (ally, x): the access
    /// they give each other is saved for this war (#17) and becomes indefinite.
    pub fn join_war(&mut self, x: FactionId, enemy: FactionId, ally: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(x, enemy)?;
        if self.world.stance(x, enemy) == Stance::War {
            return Err(CommandError::AlreadyAtWar(x, enemy));
        }
        {
            let r = self.relationship_mut(x, enemy);
            r.set_factor("war", event("war_dragged_by_ally"));
            r.war_ally = Some(ally);
            r.military_access_turns = 0;
            r.payments.clear();
        }
        {
            let r = self.relationship_mut(enemy, x);
            r.set_factor("war", event("war"));
            r.payments.clear();
        }
        for (p, q) in [(x, ally), (ally, x)] {
            let r = self.relationship_mut(p, q);
            if !r.allied_in_war_against.iter().any(|w| w.enemy == enemy) {
                let saved = r.military_access_turns;
                r.allied_in_war_against.push(super::details::AlliedWar { enemy, saved_access_turns: saved });
            }
            r.military_access_turns = -1;
        }
        self.world.declare_war(x, enemy)
    }
}

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
use super::events::{CampaignEvent, CampaignMessage};
use super::ids::FactionId;
use super::world::{CampaignModel, GovernmentType, Stance};
use super::CommandError;

/// Who answered a call to war ([`CampaignModel::call_allies`]), as the exe lists them for the zones'
/// refresh (`0x00B268B0`: lists +0, +0x20, +0x30).
#[derive(Debug, Default)]
struct CalledAllies {
    /// The declarer's allies that joined.
    joined_a: Vec<FactionId>,
    /// The target's allies that joined.
    joined_b: Vec<FactionId>,
    /// The target's allies that broke with it.
    torn_b: Vec<FactionId>,
}

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
    pub(crate) fn factor_mut(&mut self, key: &str) -> &mut AttitudeFactor {
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

/// The attitude categories' names by [`attitude_category`] (the `diplomatic_relations_attitudes` keys,
/// names `0x015F2910`, CONFIRMED): the one list the diplomat lines and the diplomacy UI read.
pub const ATTITUDE_NAMES: [&str; 5] = ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"];

/// The name of an attitude category ([`ATTITUDE_NAMES`]; above 4 the last).
pub fn attitude_name(category: u8) -> &'static str {
    ATTITUDE_NAMES[usize::from(category).min(ATTITUDE_NAMES.len() - 1)]
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
    ///   alliance, patronage or protectorate is broken first (`0x00B13840` with its war flag set,
    ///   [`Self::break_alliance_with`]`(.., true)`: the same steps as [`Self::break_alliance`], but no one is
    ///   sent out of the other's regions, neither for an access given back as none nor by a protectorate
    ///   tie's end); the access `a` gives `b` ends; a trade agreement is broken ([`Self::break_trade`]);
    ///   during friendship turns the declarer is a backstabber (`backstab`); payments are dropped.
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
            events.extend(self.break_alliance_with(a, b, true)?);
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
        // The stance turns to war (0x00B27250) before the abuse's cancel, so `a`'s forces in `b`'s
        // land may stay through its expulsion.
        events.extend(self.world.declare_war(a, b)?);
        if gave_access {
            self.abuse_access(b, a);
        }
        // The allies of both sides are called (0x00B268B0).
        let called = self.call_allies(a, b, &mut events);
        for (x, cat) in thirds {
            match cat {
                0 => self.relationship_mut(x, a).add_factor("declared_war_against_enemies", event("war_against_enemies")),
                4 => self.relationship_mut(x, a).add_factor("declared_war_against_friends", event("war_against_friends")),
                _ => {}
            }
        }
        // Then the zones of control again (`0x00B268B0` → `0x00B795A0`, CONFIRMED order): `a`'s allies
        // that joined, `b`'s that joined, `b`'s that broke with it, then `b`, then `a`. (The joins'
        // `0x00B26700` passes 0 for its refresh flag.)
        for x in called.joined_a.into_iter().chain(called.joined_b).chain(called.torn_b).chain([b, a]) {
            self.refresh_faction_zones(x);
        }
        self.diplomacy_changed();
        Ok(events)
    }

    /// `0x00B0E420`: `a` broke `b`'s trust during `turns` friendship turns. With `n` = the treaties `a`
    /// has broken so far (manager +0x1C, [`World::treaty_breaks_of`](super::World::treaty_breaks_of):
    /// it starts at 1, so the first backstab is already penalised): `b`'s `peace_treaty` factor is set
    /// to value −3·n·turns, drift of the `backstabbing` event, limit −5·n (`0x00B0E500`), every other
    /// faction of `a`'s own subculture (the declarer's, not the victim's: `0x00B0E446` loads `a` for the
    /// lookup, `0x008F3490` returns its record's +0x10 and `0x00B0E451` reads +0x10 of that; other than `a`
    /// and `b`) gets value −5n·turns, limit −25n (`0x00B0E540`); then n grows
    /// by 1 (by 3 when `turns` is 10). CONFIRMED by disassembly.
    fn backstab(&mut self, a: FactionId, b: FactionId, turns: u32) {
        let count = self.world.treaty_breaks_of(a);
        let n = count as i32;
        let t = turns as i32;
        let drift = event("backstabbing").drift;
        self.relationship_mut(b, a).factor_mut("peace_treaty").set(ev(n * -5, drift, n * t * -3));
        for x in self.other_factions(a, b) {
            if self.same_subculture(x, a) {
                let m = n * 5;
                self.relationship_mut(x, a).factor_mut("peace_treaty").set(ev(m * -5, drift, -(m * t)));
            }
        }
        self.world.treaty_breaks.insert(a, count.wrapping_add(if turns == 10 { 3 } else { 1 }));
    }

    /// `0x00B0DA90` / `0x00B0DAF0`: `a` (who had military access from `owner`) abused it: `owner`'s access
    /// to `a` ends with the cancel grievance (`0x00B67BD0`, [`Self::cancel_military_access`]), and
    /// `owner`'s `abused_military_access` factor gets the event's value + `count` × (−10), `count` =
    /// the times `a` abused an access before (`a`'s manager +0x18, read by `0x004CE450`; 0 when it has
    /// no entry); then that count +1 (`0x00B0DABA`). CONFIRMED by disassembly.
    fn abuse_access(&mut self, owner: FactionId, a: FactionId) {
        self.cancel_military_access(owner, a);
        let count = self.world.access_abuses.get(&a).copied().unwrap_or(0);
        let e = event("abused_military_access");
        self.relationship_mut(owner, a).factor_mut("abused_military_access").add(e.value + ABUSED_ACCESS_STEP * count as i32, e.drift, e.limit);
        self.world.access_abuses.insert(a, count.wrapping_add(1));
        self.diplomacy_changed();
    }

    /// `a` and `b` make peace (`0x00B262C0` → `0x00B26590` on both records):
    /// the `war` factor gets the `peace` event added (`peace_dragged_by_ally` if the side was in the war
    /// for an ally, `war_ally`, which is then cleared); friendship 10 turns; the war counters (#10–#13)
    /// zeroed. The access an alliance of `a` had suspended for this enemy is restored (`0x00B0D0C0`, on
    /// `a`'s side only), and a side given back no access leaves the other's regions.
    pub fn make_peace_rules(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        let events = self.world.make_peace(a, b)?;
        // 0x00A25570: a commander of either side raiding the other side loses all his raids.
        self.clear_raids_on_peace(a, b);
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
        // 0x00B0D0C0 (enemy `b`) on each of `a`'s alliance records (stance 2–4, record order), then on that
        // ally's record towards `a`. Only `a`'s side: the loop runs on the proposer's manager alone (CONFIRMED,
        // `0x00B262C0`, `force_make_peace` `0x00936710` passing the first faction's).
        let allies: Vec<FactionId> =
            self.world.factions.keys().copied().filter(|&x| x != a && matches!(self.world.stance(a, x), Stance::Allied | Stance::Patron | Stance::Protectorate)).collect();
        for ally in allies {
            for (owner, target) in [(a, ally), (ally, a)] {
                if self.restore_access_suspended_for(owner, target, b) {
                    self.expel_from_lands_of(target, owner);
                }
            }
        }
        // `0x00B262C0` floods the zones again: the proposer's, then the target's (CONFIRMED, disassembly).
        self.refresh_faction_zones(a);
        self.refresh_faction_zones(b);
        self.diplomacy_changed();
        Ok(events)
    }

    /// `0x00B0D0C0` on `owner`'s record towards `target`: the `allied_in_war_against` item for `enemy` gives
    /// back its saved access and is removed. True when there was one and the access given back is none:
    /// then the caller sends `target`'s forces out of `owner`'s regions (the exe's own loop,
    /// [`Self::expel_from_lands_of`]). No record or no item: nothing (CONFIRMED).
    fn restore_access_suspended_for(&mut self, owner: FactionId, target: FactionId, enemy: FactionId) -> bool {
        let Some(r) = self.world.relationships.get_mut(&(owner, target)) else { return false };
        let Some(i) = r.allied_in_war_against.iter().position(|w| w.enemy == enemy) else { return false };
        Self::give_back_access(r, i)
    }

    /// The item at `i` of `r`'s `allied_in_war_against` gives back its saved access and is removed
    /// (the one rule of [`Self::restore_access_suspended_for`]). True when the access given back is none.
    fn give_back_access(r: &mut Relationship, i: usize) -> bool {
        r.military_access_turns = r.allied_in_war_against.remove(i).saved_access_turns;
        r.military_access_turns == 0
    }

    /// The alliance break's step on `owner`'s record towards `target` (`0x00B13A30` / `0x00B0CA20`): a
    /// non-empty `allied_in_war_against` list gives back its first item's access
    /// ([`Self::give_back_access`]) and is then cleared. True as that function returns.
    fn restore_first_suspended_access(&mut self, owner: FactionId, target: FactionId) -> bool {
        let Some(r) = self.world.relationships.get_mut(&(owner, target)) else { return false };
        if r.allied_in_war_against.is_empty() {
            return false;
        }
        // The first item is the one the enemy search finds first, so it is item 0.
        let none_left = Self::give_back_access(r, 0);
        r.allied_in_war_against.clear();
        none_left
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
        self.diplomacy_changed();
        Ok(vec![CampaignEvent::StanceChanged { a, b, stance: Stance::Allied }])
    }

    /// `a` breaks its alliance (or patronage / protectorate) with `b` (`0x00B13840`):
    /// - when both are major powers, every human major power gets the `alliance_broken` message
    ///   ("<b> - <a>", `0x009CA7B0`), in the world's faction-list order: the loop at `0x00B1387F..0x00B138E5`
    ///   walks model +0xF5C → +0x2C count / +0x30 array, the list the loader `0x0090BEB0` appends to in
    ///   `FACTION_ARRAY` file order without the rebels (CONFIRMED, CAMPAIGN_FIDELITY trade split). Ours:
    ///   [`World::factions_in_turn_order`](super::world::World::factions_in_turn_order), which the importer
    ///   fills in that same file order, the rebels (appended last) left out;
    /// - while `a`'s commitment to `b` (#6) runs, every other faction of `a`'s or `b`'s subculture sets its
    ///   `cultural_alliance_broken` factor towards `a` to value −commitment − 5·k, limit the event's limit
    ///   − 5·k (k = the alliances `a` broke before, manager +0x14, then k + 1) (`0x00B25FA0`);
    /// - `a`→`b` (`0x00B13A30`): the `alliance` factor set from the `break_alliance` event; stance neutral
    ///   (unless patron); commitment 0; a non-empty `allied_in_war_against` list gives back its first
    ///   item's access and is cleared, and when that access is none `b`'s forces leave each of `a`'s
    ///   regions ([`Self::expel_from_lands_of`]; not from the war path, [`Self::break_alliance_with`]);
    /// - `b`→`a` (`0x00B0CA20`): first, when `b` is `a`'s protectorate, the tie ends
    ///   ([`Self::end_protectorate_tie`], the patron `a` leaving `b`'s regions unless from the war path);
    ///   the `alliance` factor reset to 0, `alliance_broken` set from its event; stance neutral; commitment 0;
    ///   the protectorate income line cleared; access restored likewise, `a` leaving `b`'s regions;
    /// - then the model's +0x810 event with {a, b}: its one campaign listener (the CAI interface,
    ///   vtable `0x01377BA4`, slot `0x00A8F5B0`) only times itself, so it has no state to model
    ///   (CONFIRMED by disassembly 2026-10-17); and the diplomacy-changed event (+0x3A8).
    pub fn break_alliance(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.break_alliance_with(a, b, false)
    }

    /// [`Self::break_alliance`] with `0x00B13840`'s war flag, passed down to `0x00B13A30` / `0x00B0CA20`:
    /// 1 from the war declaration (`0x00B27144`), 0 from every other caller (CONFIRMED). Clear, a side whose
    /// access the alliance had suspended and gives back none leaves the other's regions; set, it stays
    /// (the war that follows lets it).
    fn break_alliance_with(&mut self, a: FactionId, b: FactionId, from_war: bool) -> Result<Vec<CampaignEvent>, CommandError> {
        let mut events = Vec::new();
        if self.is_major(a) && self.is_major(b) {
            let message = CampaignMessage::AllianceBroken { first: b, second: a };
            events.extend(
                self.world.factions_in_turn_order().into_iter()
                    .filter(|&f| !self.is_rebel_faction(f) && self.is_major(f) && self.is_human(f))
                    .map(|recipient| CampaignEvent::Message { recipient, message: message.clone() }),
            );
        }
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
            // The manager's u32 counter (+0x14) wraps like the exe's.
            let n = self.world.alliances_broken.entry(a).or_insert(0);
            *n = n.wrapping_add(1);
        }
        // Read before either record changes: `0x00B0CA20` tests `b`→`a` stance 4 on entry.
        let b_protectorate = self.world.stance(b, a) == Stance::Protectorate;
        // 0x00B13A30 on `a`→`b`. The exe leaves a patron's stance to the tie's end below; the model's
        // stance is one value per pair, and neither patron nor neutral lets `b` stand in `a`'s land.
        {
            let r = self.relationship_mut(a, b);
            r.set_factor("alliance", event("break_alliance"));
            r.alliance_commitment_turns = 0;
        }
        let lost = self.restore_first_suspended_access(a, b);
        self.world.set_stance(a, b, Stance::Neutral)?;
        if lost && !from_war {
            self.expel_from_lands_of(b, a);
        }
        // 0x00B0CA20 on `b`→`a`: a protectorate's tie to its patron `a` ends first (the war flag passed
        // as its skip flag), then the record's own steps. Deliberately deduplicated: the tie's only event is
        // the pair's change to neutral, the same change pushed once below, so it is not passed on.
        if b_protectorate {
            let tie = self.end_protectorate_tie_with(b, a, from_war)?;
            debug_assert!(matches!(tie.as_slice(), [CampaignEvent::StanceChanged { stance: Stance::Neutral, .. }]));
        }
        {
            let r = self.relationship_mut(b, a);
            r.factor_mut("alliance").reset(0);
            r.set_factor("alliance_broken", event("alliance_broken"));
            r.protectorate_income = 0;
            r.protectorate_tribute = 0;
            r.alliance_commitment_turns = 0;
        }
        let lost = self.restore_first_suspended_access(b, a);
        if lost && !from_war {
            self.expel_from_lands_of(a, b);
        }
        self.diplomacy_changed();
        events.push(CampaignEvent::StanceChanged { a, b, stance: Stance::Neutral });
        Ok(events)
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
        self.diplomacy_changed();
        Ok(Vec::new())
    }

    /// `a` breaks its trade agreement with `b` (`BreakTradeAgreementWithFaction 0x00B29BB0`): `a`→`b` (`CancelTradeAgreementOnRelation 0x00B13C30`) `trade_broken` set from
    /// the `break_trade` event and `trade` cleared; `b`→`a` (`EndTradeAgreementForBrokenPartner 0x00B733B0`) `trade` cleared and `trade_broken`
    /// set from the `trade_broken` event; the flag cleared on both. Each side then removes only its first route
    /// to the other ([`Self::remove_first_trade_route`]: `RemoveFirstTradeRouteToImporter 0x00BC0120`, once from
    /// `0x00B13C30` and once from `0x00B733B0`); any further routes of the pair stay. CONFIRMED.
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
        self.remove_first_trade_route(a, b);
        self.remove_first_trade_route(b, a);
        self.diplomacy_changed();
        Ok(Vec::new())
    }

    /// The trade agreement of `a` and `b` ends because their routes were lost (`0x00B73430`, from the route
    /// loss [`Self::trade_routes_after_region_lost`]): on both records (`0x00B13C30` each way) the
    /// `trade_broken` factor is set from the `break_trade` event, `trade` cleared and the agreement flag
    /// cleared, and the first route each way is removed (`RemoveFirstTradeRouteToImporter 0x00BC0120`, called once per
    /// direction by `0x00B13C30`, [`Self::remove_first_trade_route`]; any further routes of the pair stay).
    /// Unlike [`Self::break_trade`], neither
    /// side gets the `trade_broken` event: both get the breaker's. Then the diplomacy refresh (the record's
    /// +0x3A8). Not modelled: the message to both sides (`0x00A26F30`, 0x10B) and the trade-changed flag
    /// (model +0x840). CONFIRMED.
    pub(crate) fn break_trade_both_ways(&mut self, a: FactionId, b: FactionId) {
        for (x, y) in [(a, b), (b, a)] {
            let r = self.relationship_mut(x, y);
            r.set_factor("trade_broken", event("break_trade"));
            r.set_factor("trade", ev(0, 0, 0));
            r.trade_agreement = false;
            self.remove_first_trade_route(x, y);
        }
        self.diplomacy_changed();
    }

    /// `a` embargoes `x`'s trade at the request of `requester`, the deal's other side (`0x00B28DB0`,
    /// called only by the deal loop `0x00C18800`: list `+0x2C` on the proposer with the recipient as
    /// requester, list `+0x1C` on the recipient with the proposer): `a`'s trade agreement with `x` is
    /// broken first (`0x00B29BB0`); `a`'s embargo turns (#27) towards `x` are 10 (`0x00B28E00`,
    /// S1_LEFTOVERS.md); `x`'s `trade_embargoed` factor towards `requester` is set from its event (value
    /// −40, drift −2, limit 0: with the original's clamp the value is 0 after one turn — as the exe does
    /// it): the embargoed faction resents the side that asked for it, not the embargoer. CONFIRMED.
    pub fn embargo(&mut self, a: FactionId, x: FactionId, requester: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, x)?;
        self.check_diplomacy_pair(x, requester)?;
        if self.world.relationships.get(&(a, x)).is_some_and(|r| r.trade_agreement) {
            self.break_trade(a, x)?;
        }
        self.relationship_mut(a, x).trade_embargo_turns = 10;
        self.relationship_mut(x, requester).set_factor("trade_embargoed", event("trade_embargoed"));
        Ok(Vec::new())
    }

    /// `a` gives `b` military access for `turns` turns (−1: indefinitely) (`GrantMilitaryAccessToFaction` `0x00B444B0` → `0x00B44550`):
    /// #3 becomes −1, or grows by `turns`; #24 the granted length; friendship 10; #25 elapsed 0.
    pub fn grant_military_access(&mut self, a: FactionId, b: FactionId, turns: i32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        let r = self.relationship_mut(a, b);
        r.military_access_turns = if turns == -1 { -1 } else { r.military_access_turns + turns };
        r.military_access_granted = r.military_access_turns;
        r.friendship_turns = 10;
        r.military_access_elapsed = 0;
        // `0x00B444B0` floods the zones again: `b`'s, then `a`'s (CONFIRMED, disassembly).
        self.refresh_faction_zones(b);
        self.refresh_faction_zones(a);
        self.diplomacy_changed();
        Ok(Vec::new())
    }

    /// `a` withdraws the military access it gives `b` (`0x00B67BD0`): the cancel grievance (#26) grows by
    /// base − used share by the granted length (5: 50 − 10·elapsed; 10: 60 − 6·elapsed; 20: 70 − 7·elapsed/2;
    /// other: 90 − 5·elapsed/2, unsigned halves) and the access ends; then `b`'s forces leave each of `a`'s
    /// regions ([`Self::expel_from_each_region_of`], unconditionally: the war abuse path, `0x00B0DAF0`, comes after
    /// the stance turned to war, so they may stay there). Returns the grievance added (the
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
        self.expel_from_each_region_of(b, a);
        g
    }

    /// `giver` gives `receiver` `amount` money as a state gift (`0x00B44590` → `0x00B446B0`): x = linear ×
    /// amount + quadratic × amount² / √(GDP of both), at most 100; the receiver's `state_gift` factor
    /// towards the giver gets trunc((100 − its current contribution) × x × 0.01) added, drift and limit of
    /// the `state_gift` event. The money (`0x00C4B440`, CONFIRMED static trace 2026-10-10): the giver is charged
    /// (`0x00BAF500(amount, 3)`) with no money test (the original Lua `InitialiseStateGift`, `diplomacy_panel.luac`,
    /// greys a value above `MaxPlayerPaymentAllowed`), and the receiver is credited nothing: the gift buys attitude,
    /// the money leaves the game. An amount of 0 or below is refused (the panel offers only the `state_gift_values`;
    /// the exe would charge the receiver for a negative one).
    pub fn state_gift(&mut self, giver: FactionId, receiver: FactionId, amount: i32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(giver, receiver)?;
        if amount <= 0 {
            return Err(CommandError::Unsupported("a state gift of 0 or less"));
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
            super::treasury::pay(&mut g.treasury, amount);
        }
        self.diplomacy_changed();
        Ok(Vec::new())
    }

    /// `payer` pays `payee` `amount` once: a deal's one-off payment (the payment item `0x00C18A70` with a
    /// length of 1 turn, CONFIRMED static trace 2026-10-10). The payer is charged (`0x00BAF500(amount, 3)`) and
    /// the payee credited (`0x00BB3810(amount, 1)`), with no money test and no attitude change; a negative amount
    /// runs the other way, which [`super::treasury::pay`] / [`super::treasury::credit`] give as they stand. When
    /// both sides are human the exe also adds ±amount to the faction's multiplayer results counter
    /// (`0x008F56C0`, FACTION +0x954), which only the results report of a multiplayer campaign reads
    /// (getter `0x008D9DB0`, CONFIRMED 2026-10-12): no gameplay effect, so it is not modelled here; the save
    /// keeps the field as loaded.
    pub fn one_off_payment(&mut self, payer: FactionId, payee: FactionId, amount: i32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(payer, payee)?;
        self.pay_once(payer, payee, amount);
        Ok(Vec::new())
    }

    /// The money of [`Self::one_off_payment`], for a caller that has checked the faction pair.
    pub(super) fn pay_once(&mut self, payer: FactionId, payee: FactionId, amount: i32) {
        if let Some(f) = self.world.factions.get_mut(&payer) {
            super::treasury::pay(&mut f.treasury, amount);
        }
        if let Some(f) = self.world.factions.get_mut(&payee) {
            super::treasury::credit(&mut f.treasury, amount);
        }
    }

    /// `payer` agrees to pay `payee` `amount` a turn for `turns` turns: a `REGULAR_PAYMENTS` item on the payer's
    /// record (direction INFERRED), and the friendship raised to min(turns, 10) (S1_LEFTOVERS.md #15).
    pub fn regular_payment(&mut self, payer: FactionId, payee: FactionId, amount: i32, turns: u32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(payer, payee)?;
        self.pay_regularly(payer, payee, amount, turns);
        Ok(Vec::new())
    }

    /// The record of [`Self::regular_payment`], for a caller that has checked the faction pair.
    pub(super) fn pay_regularly(&mut self, payer: FactionId, payee: FactionId, amount: i32, turns: u32) {
        let r = self.relationship_mut(payer, payee);
        r.payments.push(RegularPayment { amount, turns });
        r.friendship_turns = r.friendship_turns.max(turns.min(10));
        self.diplomacy_changed();
    }

    /// `protectorate` becomes the protectorate of `patron` (`0x00B105C0`, CONFIRMED):
    /// - its own ties first (`0x00B104E0`, over the protectorate's records): a faction it is already the
    ///   protectorate of gets the tie ended ([`Self::end_protectorate_tie`], `0x00B64B10`); a faction it is the
    ///   patron of becomes the protectorate of `patron` in turn (`0x00B105C0` on that faction). The patron
    ///   itself is skipped for the second case (the exe would make `patron` its own protectorate; a patron
    ///   cannot ask to become its own protectorate's protectorate, so the case does not arise in play);
    /// - `protectorate`→`patron` stance 4 with the alliance factor set (`0x00B29DA0` unless already allied),
    ///   `patron`→`protectorate` stance 3 likewise (`0x00B10480`);
    /// - every other alliance of the protectorate (stance 2, not with `patron`) breaks ([`Self::break_alliance`],
    ///   `0x00B13840`), then the relation-change broadcast.
    ///
    /// Not modelled: the deal's peace step (`param_3`, from the deal record's +0x16 flag: an AI faction at war
    /// with the protectorate makes peace with it when its own `0x00B0CDF0` check towards the patron holds,
    /// `0x00B262C0`; BACKLOG). Tribute: `0x00BBCD00` = a faction value / 5 (the value, `0x00B803A0`, is not
    /// read: the tribute line is left at 0, PROVISIONAL).
    pub fn make_protectorate(&mut self, protectorate: FactionId, patron: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(protectorate, patron)?;
        let mut events = Vec::new();
        let others: Vec<FactionId> = self.world.factions.keys().copied().filter(|&x| x != protectorate).collect();
        for x in others {
            // The stance is read when the record is reached: an earlier step may have changed it (0x00B104E0).
            if self.world.stance(protectorate, x) == Stance::Protectorate {
                events.extend(self.end_protectorate_tie(protectorate, x)?);
            }
            if x != patron && self.world.stance(protectorate, x) == Stance::Patron {
                events.extend(self.make_protectorate(x, patron)?);
            }
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
        let allies: Vec<FactionId> = self
            .world
            .factions
            .keys()
            .copied()
            .filter(|&x| x != protectorate && x != patron)
            .collect();
        for x in allies {
            if self.world.stance(protectorate, x) == Stance::Allied {
                events.extend(self.break_alliance(protectorate, x)?);
            }
        }
        // The patron's zones of control again, before the broadcast (`0x00B105C0` at `0x00B106CC` → `0x00B795A0`).
        self.refresh_faction_zones(patron);
        self.diplomacy_changed();
        Ok(events)
    }

    /// Ends `protectorate`'s tie to `patron` (`0x00B64B10` with flag 0, CONFIRMED): the stance neutral both
    /// ways (`0x00B64C10` / `0x00B54530`), the tribute (`protectorate`→`patron`) and income
    /// (`patron`→`protectorate`) lines cleared, then the relation-change broadcast. No attitude factor, no
    /// `alliance_broken`, no commitment change (unlike [`Self::break_alliance`]). Before the broadcast
    /// (unless its skip flag is set: only the war path's alliance break, `0x00B0CA20` passing its war
    /// flag, sets it; `0x00B104E0` passes 0) the patron's forces leave each of the protectorate's regions
    /// (`0x008D9DC0` per region of its list, [`Self::expel_from_lands_of`]). It also runs `0x00A0B570` per character of
    /// the patron and refreshes both factions' path-grid obstacles (`0x00B795A0`, `0x00B67490`), which
    /// `plan_path` builds at search time from the stances.
    pub fn end_protectorate_tie(&mut self, protectorate: FactionId, patron: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.end_protectorate_tie_with(protectorate, patron, false)
    }

    /// [`Self::end_protectorate_tie`] with the exe's skip flag (`0x00B64B10`'s third argument): set, the
    /// patron's forces stay. Only the war path's alliance break sets it ([`Self::break_alliance_with`]).
    fn end_protectorate_tie_with(&mut self, protectorate: FactionId, patron: FactionId, skip_expulsion: bool) -> Result<Vec<CampaignEvent>, CommandError> {
        self.world.set_stance(protectorate, patron, Stance::Neutral)?;
        self.relationship_mut(protectorate, patron).protectorate_tribute = 0;
        self.relationship_mut(patron, protectorate).protectorate_income = 0;
        if !skip_expulsion {
            self.expel_from_lands_of(patron, protectorate);
        }
        self.diplomacy_changed();
        Ok(vec![CampaignEvent::StanceChanged { a: protectorate, b: patron, stance: Stance::Neutral }])
    }

    /// Both factions exist and differ: the model's argument check for a two-faction diplomacy action.
    pub(super) fn check_diplomacy_pair(&self, a: FactionId, b: FactionId) -> Result<(), CommandError> {
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
    ///    keeps the streak growing; none resets it. Timed access running out (1 → 0) sends the target's
    ///    forces out of the faction's regions ([`Self::expel_from_lands_of`]). (When the count-down leaves
    ///    one turn, the exe posts the target the `military_access_prompt` message, 0x76 via `0x00A0E630`:
    ///    PROVISIONAL, not modelled.)
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
        // A faction out of the game updates nothing (0x00B29100 tests the owner's +0x824 first, [`Self::in_the_game`]).
        // The vanilla saves agree: such factions' records do not drift.
        if !self.in_the_game(faction) {
            return;
        }
        for t in targets {
            if !self.in_the_game(t) {
                continue;
            }
            let Some(r) = self.world.relationships.get_mut(&(faction, t)) else { continue };
            for f in &mut r.attitudes {
                f.step();
            }
            r.alliance_commitment_turns = r.alliance_commitment_turns.saturating_sub(1);
            let access_ran_out = match r.military_access_turns {
                n if n > 0 => {
                    r.military_access_elapsed += 1;
                    r.military_access_streak += 1;
                    r.military_access_turns = n - 1;
                    n == 1
                }
                -1 => {
                    r.military_access_streak += 1;
                    false
                }
                _ => {
                    r.military_access_streak = 0;
                    false
                }
            };
            // 0x00B29170: the access just ran out, so `t`'s forces leave `faction`'s regions, before the
            // war balances below read where the armies stand (CONFIRMED order).
            if access_ran_out {
                self.expel_from_lands_of(t, faction);
            }
            let at_war = self.world.stance(faction, t) == Stance::War;
            let allied = matches!(self.world.stance(faction, t), Stance::Allied | Stance::Patron | Stance::Protectorate);
            let given_access = self.world.relationships.get(&(t, faction)).is_some_and(|r| r.military_access_turns != 0);
            let balances = if at_war { Some(self.war_balances(faction, t)) } else { None };
            let dead: Vec<FactionId> = self
                .world
                .relationships
                .get(&(faction, t))
                .map(|r| r.allied_in_war_against.iter().map(|w| w.enemy).filter(|&e| !self.in_the_game(e)).collect())
                .unwrap_or_default();
            let Some(r) = self.world.relationships.get_mut(&(faction, t)) else { continue };
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
        self.diplomacy_changed();
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

/// The original's cap on a proposed payment (the seam [`super::seams::PROPOSED_PAYMENT`]): the payment
/// record's copy (vtable `0x01381EDC` slot +0x44, `0x00C4AB20`), run when the record is set
/// ([`CampaignModel::propose_payment`]; the exe's `0x00C11B80` → `0x00C49AA0`, from the propose
/// command), before the panel's row text is built from the result (`0x00C4AB85`). `amount` is
/// in the record's sign ([`super::negotiation::DealPayment`]: negative, offered by `proposer`), for
/// `turns` turns. An offer paid once (turns 1, amount negative) offers at most what the proposer
/// holds: when its negation is above the proposer's treasury (signed, `0x00BCAFE0`) it becomes minus
/// the treasury (`0x00C4AB4E`–`0x00C4AB82`). A demand, a regular payment, or turns other than 1 stays
/// as it is (the panel limits a demand itself: `diplomacy_panel.luac` `CheckMaxPayment`). An unknown
/// proposer keeps the amount. CONFIRMED (disassembly).
///
/// ORIGINAL BUG: a negative treasury turns the offer around: the stored amount becomes −treasury,
/// positive, a demand on the recipient (`0x00C4AB7E`–`0x00C4AB82`), which the cap's own intent ("never
/// more than the proposer has") contradicts. Ours caps at max(treasury, 0): an indebted proposer offers
/// 0, which pays nothing.
pub fn proposed_payment(model: &CampaignModel, proposer: FactionId, amount: i32, turns: u32) -> i32 {
    if turns != 1 || amount >= 0 {
        return amount;
    }
    match model.world.factions.get(&proposer) {
        Some(f) if amount.wrapping_neg() > f.treasury => -(f.treasury.max(0)),
        _ => amount,
    }
}

impl CampaignModel {
    /// What a deal's payment record holds when `proposer` proposes `amount` for `turns` turns (the
    /// record's sign, [`super::negotiation::DealPayment`]): the rule seam
    /// [`super::seams::PROPOSED_PAYMENT`] (the original's is [`proposed_payment`]). Called once, when
    /// the record is set ([`Self::propose_payment`]): the panel's row, the AI's evaluation and the
    /// accepted deal all read the record.
    pub fn proposed_payment(&self, proposer: FactionId, amount: i32, turns: u32) -> i32 {
        self.rules.seams.proposed_payment.rule()(self, proposer, amount, turns)
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
    /// Embargo a third faction's trade at the other side's request (a deal item, `0x00B28DB0`): the
    /// embargoed faction resents the other side.
    Embargo(FactionId),
    /// Give the other side military access for some turns (−1: indefinitely).
    GrantMilitaryAccess(i32),
    /// Withdraw the military access given to the other side.
    CancelMilitaryAccess,
    /// Give money as a state gift.
    StateGift(i32),
    /// Pay an amount once (a deal's one-off payment).
    OneOffPayment(i32),
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
            DiplomaticAction::Embargo(x) => self.embargo(a, x, b),
            DiplomaticAction::GrantMilitaryAccess(turns) => self.grant_military_access(a, b, turns),
            DiplomaticAction::CancelMilitaryAccess => {
                self.check_diplomacy_pair(a, b)?;
                // 0x00B67B20: a third of the grievance stays on the canceller's manager (+0x20).
                let g = self.cancel_military_access(a, b);
                let m = self.world.access_cancel_marks.entry(a).or_insert(0);
                *m = m.wrapping_add((g / 3).max(0) as u32);
                // Then `b`'s zones of control again (`0x00B67B20` → `0x00B795A0`; `0x00B67BD0` refreshes none).
                self.refresh_faction_zones(b);
                self.diplomacy_changed();
                // Then `b` gets the `alliance_broken` message "<b> - <a>" (`0x009CA7B0`, no human
                // or major test; the display shows only the viewed human's messages).
                Ok(vec![CampaignEvent::Message { recipient: b, message: CampaignMessage::AllianceBroken { first: b, second: a } }])
            }
            DiplomaticAction::StateGift(amount) => self.state_gift(a, b, amount),
            DiplomaticAction::OneOffPayment(amount) => self.one_off_payment(a, b, amount),
            DiplomaticAction::RegularPayment(amount, turns) => self.regular_payment(a, b, amount, turns),
            DiplomaticAction::BecomeProtectorate => self.make_protectorate(a, b),
        }
    }
}

impl CampaignModel {
    /// The computed attitude factors at the round start (`0x0096C050` → `0x008BAF30` → `0x00B71E90` →
    /// `0x00B71EC0` per relationship, CONFIRMED): for every relationship owned by a faction that is still in the
    /// game and is not the rebels (`0x008BAF30` returns early on `IsFactionWithoutRecord` `0x008CEEF0` or +0x824,
    /// and the rebels are not in the turn list it walks), the `faction_leader` factor is reset to trunc(the
    /// target's faction effect `diplomacy_bonus_faction_leader`, 0x52) and `enlightenment` to trunc(its
    /// `diplomacy_bonus_enlightenment`, 0x7E); the rebels' own relationships keep theirs. Checked on the vanilla
    /// saves: every enlightenment factor, 484 / 506 leader factors (France's leader: the model sums −9 where the
    /// saves hold +9; open).
    pub fn refresh_computed_factors(&mut self) {
        let fx = super::effects::Effects::compute(self);
        let alive: Vec<FactionId> =
            self.world.factions.keys().copied().filter(|&f| !self.is_rebel_faction(f) && self.in_the_game(f)).collect();
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
        self.diplomacy_changed();
        Ok(vec![CampaignEvent::GovernmentChanged {
            faction,
            old_government: old_government_key,
            new_government: new_government_key.to_string(),
        }])
    }

    /// A faction still in the game: the exe's faction +0x824 clear ([`super::details::FactionDetails::out_of_game`],
    /// CONFIRMED). An own save from before the flag was kept lacks it; then, INFERRED: the faction holds a
    /// region or a force.
    pub fn in_the_game(&self, f: FactionId) -> bool {
        match self.world.faction_details.get(&f).and_then(|d| d.out_of_game) {
            Some(out) => !out,
            None => self.world.regions.values().any(|r| r.owner == f) || self.world.forces.values().any(|x| x.faction == f),
        }
    }

    /// The money of treaties at `faction`'s round-end economy (PROVISIONAL place; INFERRED direction):
    /// - regular payments: the items on `faction`'s record towards a target move their amount from `faction` to
    ///   the target each turn (the economy line category 0 holds the sum of the active items, `0x00B29170`);
    /// - protectorate tribute: a protectorate pays its patron a fifth of its revenue (`0x00BBCD00` = the sum of
    ///   three economics lines / 5, read as taxes + trade + other), economy lines 6 (protectorate) and 2 (patron),
    ///   kept in #8 / #9.
    ///
    /// In the exe both are economy lines summed into the round settle (`0x00BABE30`), so the model moves them with
    /// [`super::treasury::pay`] / [`super::treasury::credit`], which saturate where the exe's 32-bit sum wraps
    /// (ORIGINAL BUG, `treasury` module docs).
    pub fn diplomacy_money(&mut self, faction: FactionId) {
        let targets: Vec<FactionId> = self.world.relationships.keys().filter(|(o, _)| *o == faction).map(|(_, t)| *t).collect();
        for t in targets {
            if !self.in_the_game(t) {
                continue;
            }
            let pay = self.world.relationships.get(&(faction, t)).map_or(0, |r| r.payments.iter().fold(0i32, |s, p| s.wrapping_add(p.amount)));
            let tribute = if self.world.stance(faction, t) == Stance::Protectorate {
                super::economy::faction_income(self, faction).revenue().max(0) / 5
            } else {
                0
            };
            if tribute != 0 || self.world.relationships.get(&(faction, t)).is_some_and(|r| r.protectorate_tribute != 0) {
                self.relationship_mut(faction, t).protectorate_tribute = tribute;
                self.relationship_mut(t, faction).protectorate_income = tribute;
            }
            let total = pay.wrapping_add(tribute);
            if total != 0 {
                if let Some(f) = self.world.factions.get_mut(&faction) {
                    super::treasury::pay(&mut f.treasury, total);
                }
                if let Some(f) = self.world.factions.get_mut(&t) {
                    super::treasury::credit(&mut f.treasury, total);
                }
            }
        }
    }

    /// The allies of both sides called into `a`'s war on `b` (`DeclareWarAndCallAllies` `0x00B268B0`,
    /// CONFIRMED flow): every faction X allied to `b` (stance allied, patron or protectorate), in the game,
    /// not `a`, not already at war with `a`: a human X gets an offer (UI event `+0xB10`; not modelled: a
    /// human ally is not called); an AI X decides through the AI (`0x00AAA950` → `0x00AAA920`, §6). Join: X
    /// declares war on `a` on `b`'s behalf ([`Self::join_war`]). Refuse: X breaks its alliance with `b`
    /// (`0x00B27500`, `0x00B13840`). Then the same for `a`'s allies against `b`, except that a refusing ally
    /// of `a` is only listed (`0x00B0DB40`) and keeps its alliance. PROVISIONAL AI decision: join, unless X
    /// is allied to the enemy as well (then it refuses). PROVISIONAL order: the exe calls the allies before
    /// declaring the war itself (`0x00B26700` inside `0x00B268B0`); [`Self::declare_war_rules`] declares it
    /// first. The events go to `events`; returns who joined and who broke with `b`, the lists the exe keeps
    /// for the zones' refresh.
    fn call_allies(&mut self, a: FactionId, b: FactionId, events: &mut Vec<CampaignEvent>) -> CalledAllies {
        let mut called = CalledAllies::default();
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
                let result = match (torn, side == b) {
                    (true, true) => self.break_alliance(x, side),
                    // A refusing ally of the attacker keeps its alliance (0x00B0DB40 only lists it).
                    (true, false) => Ok(Vec::new()),
                    (false, _) => self.join_war(x, enemy, side),
                };
                if let Ok(mut e) = result {
                    events.append(&mut e);
                    let list = match (torn, side == a) {
                        (false, true) => &mut called.joined_a,
                        (false, false) => &mut called.joined_b,
                        (true, false) => &mut called.torn_b,
                        (true, true) => continue,
                    };
                    list.push(x);
                }
            }
        }
        called
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
        let events = self.world.declare_war(x, enemy)?;
        self.diplomacy_changed();
        Ok(events)
    }
}

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
//! Not here: the AI's acceptance of a deal (§6), an AI ally's answer to a call to war (`0x00B268B0`
//! asks it in `call_allies`: an AI decision; a human ally's waits in an [`AllyWarCall`]). The treaty money (regular payments, protectorate
//! tribute and income) is booked into the running economics records by
//! [`CampaignModel::change_treaty_money`] and paid by the round settle ([`super::economy::settle_round`]).

use super::details::{AttitudeFactor, RegularPayment, Relationship, ATTITUDE_FACTORS};
use super::events::{CampaignEvent, CampaignMessage};
use super::ids::{CharacterId, FactionId};
use super::population::ClassWarEvent;
use super::world::{CampaignModel, GovernmentType, Stance};
use super::CommandError;

/// Who answered a call to war ([`CampaignModel::call_allies`]), as the exe lists them (`0x00B268B0`:
/// four lists at +0, +0x10, +0x20, +0x30; the zones' refresh walks +0, +0x20, +0x30, and the war
/// notice skips a faction in any of the four, `0x00B2BEC0`). A waiting [`AllyWarCall`] keeps a copy
/// (offer +0x1C..+0x5B, `0x00AE6260`), so the declaration resumes with them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CalledAllies {
    /// The declarer's allies that joined (`0x00741C70`, list +0).
    pub joined_a: Vec<FactionId>,
    /// The target's allies that joined (`0x00B274E0`, list +0x20).
    pub joined_b: Vec<FactionId>,
    /// The target's allies that broke with it (`0x00B27500`, list +0x30).
    pub torn_b: Vec<FactionId>,
    /// The declarer's allies that refused and kept their alliance (`0x00B0DB40`, list +0x10).
    pub refused_a: Vec<FactionId>,
}

impl CalledAllies {
    /// In any of the four lists (`0x00B2BEC0`).
    fn contains(&self, f: FactionId) -> bool {
        [&self.joined_a, &self.joined_b, &self.torn_b, &self.refused_a].iter().any(|l| l.contains(&f))
    }

    /// Already answered for `side` of the war: the target's lists +0x20 / +0x30 (`0x00B673D0`) when
    /// `side` is the target, else the declarer's +0 / +0x10 (`0x00B67380`). Each loop of
    /// `0x00B268B0` skips such an ally, so a resumed declaration asks no one twice.
    fn answered_for(&self, target_side: bool, f: FactionId) -> bool {
        let lists = if target_side { [&self.joined_b, &self.torn_b] } else { [&self.joined_a, &self.refused_a] };
        lists.iter().any(|l| l.contains(&f))
    }

    fn hash_into(&self, h: &mut crate::fnv::Fnv64) {
        for list in [&self.joined_a, &self.refused_a, &self.joined_b, &self.torn_b] {
            h.u32(list.len() as u32);
            for f in list {
                h.i32(f.raw());
            }
        }
    }
}

/// A call to war waiting for a human ally's answer: the exe's 100-byte `pending_diplomacy` offer
/// (`0x00885DA0`, vtable `0x01357314`), pushed on the campaign's pending stack (+0xF70, `0x008E2B80`) by
/// [`CampaignModel::call_allies`] and answered by `RespondToAllyWarCall`
/// ([`CampaignModel::respond_to_ally_war_call`], `CCQ_RESPOND_TO_ALLY_WAR_INVITATION` `0x009370F0` →
/// `0x008BFA80`). CONFIRMED by disassembly (CAMPAIGN_FIDELITY.md §Diplomacy, "Allies called into a war").
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AllyWarCall {
    /// The declarer (offer +0xC).
    pub declarer: FactionId,
    /// The declaration's target (offer +8).
    pub target: FactionId,
    /// The human ally asked (offer +0x10).
    pub ally: FactionId,
    /// The ally is asked as the target's ally (offer +0x60): always when the target's loop asks it;
    /// when the declarer's loop does, if it is allied to the target too (`0x008CE390`).
    pub allied_to_target: bool,
    /// The ally is asked as the declarer's ally (offer +0x61), likewise.
    pub allied_to_declarer: bool,
    /// Which loop asked it: the target's (`0x00B26B80`) or the declarer's (`0x00B26D79`). The UI
    /// event's last flag (+0xB10 vfunc +8); the answer does not read it.
    pub asked_by_target_side: bool,
    /// Who had answered when the offer was made (the declaration's four lists).
    pub called: CalledAllies,
}

impl AllyWarCall {
    /// The `ally_attacked` message's `InvitedByAggressor`: the signal `0x00B268B0` fires on the campaign's
    /// +0xB10 slot (+0xC, handler `0x00A14B80` → `0x009CA900`) carries the offer's +0x61 flag when the
    /// target's loop asks (`0x00B26B80`) and 1 when the declarer's does (`0x00B26D79`): in both cases
    /// [`Self::allied_to_declarer`]. CONFIRMED by disassembly.
    pub fn invited_by_declarer(&self) -> bool {
        self.allied_to_declarer
    }

    /// The message's `InvitedByDefender` (the signal's +0xD): 1 from the target's loop, 0 from the
    /// declarer's, even when the ally is allied to the target too (CONFIRMED, same addresses).
    pub fn invited_by_target(&self) -> bool {
        self.asked_by_target_side
    }

    pub(crate) fn hash_into(&self, h: &mut crate::fnv::Fnv64) {
        for f in [self.declarer, self.target, self.ally] {
            h.i32(f.raw());
        }
        h.u32(u32::from(self.allied_to_target) | u32::from(self.allied_to_declarer) << 1 | u32::from(self.asked_by_target_side) << 2);
        self.called.hash_into(h);
    }
}

/// What [`CampaignModel::call_allies`] ended with: every ally answered, or a human ally asked.
enum CallOutcome {
    Answered(CalledAllies),
    Asked(AllyWarCall),
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
    /// The relationship record of `a` towards `b`, created (24 zeroed factors) when missing and then
    /// listed last in `a`'s record order ([`super::World::list_new_record`]).
    pub fn relationship_mut(&mut self, a: FactionId, b: FactionId) -> &mut Relationship {
        if !self.world.relationships.contains_key(&(a, b)) {
            self.world.list_new_record(a, b);
        }
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

    /// The factions of the world's faction list (model +0xF5C: the turn list, which never holds the
    /// rebels, world +0x1C) other than `a` and `b`, in list order: the third parties the declaration's
    /// loops walk (`0x00B268B0`'s notice loop, `0x00B26700`'s attitude loop; CONFIRMED by disassembly).
    fn listed_third_parties(&self, a: FactionId, b: FactionId) -> Vec<FactionId> {
        let mut out = self.world.factions_in_turn_order();
        out.retain(|&x| x != a && x != b && !self.is_rebel_faction(x));
        out
    }

    /// `a` declares war on `b` (`DeclareWarAndCallAllies` `0x00B268B0`, CONFIRMED order):
    /// 1. both sides' allies are called while `a` and `b` are still at peace ([`Self::call_allies`]): `b`'s
    ///    first, then `a`'s; each joining ally's own war ([`Self::join_war`]) runs then;
    /// 2. if both sides are major powers (faction +0x524), the `war_declared` notice
    ///    ([`CampaignMessage::WarDeclared`], `0x009DAFE0`) to every listed faction
    ///    ([`Self::listed_third_parties`], so never the rebels) in none of the call lists
    ///    (`0x00B2BEC0`), in list order, with no human test;
    /// 3. then the war itself ([`Self::war_on`] with no ally, `0x00B26700`), so the third parties judge
    ///    it by their attitude after the allies' wars and broken alliances;
    /// 4. then the zones of control again (`0x00B795A0`).
    ///
    /// When step 1 reaches a human ally not yet at war, the declaration stops there: the ally's
    /// [`AllyWarCall`] is pushed on [`CampaignModel::pending_ally_war_calls`] with the lists so far,
    /// [`CampaignEvent::AllyWarCallPending`] is raised, and steps 2-4 wait for the answer
    /// ([`Self::respond_to_ally_war_call`], which resumes here). CONFIRMED: `0x00B268B0` returns after
    /// pushing the offer (`0x00B26C1F` / `0x00B26E17`), and `0x008BFA80` calls it again.
    ///
    /// Not modelled: the achievement hook (`0x009C7660`, a stat only, between the notices and the war),
    /// the path grid's view and the trade price update (`0x00B67490`, `0x00BCB020`, no model state).
    pub fn declare_war_rules(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        // ORIGINAL BUG: `0x00B268B0` / `0x00B26700` declare a war that already exists again (allies
        // called, notices and attitude factors added once more); ours refuses it (see
        // [`Self::declare_war_command`]).
        if self.world.stance(a, b) == Stance::War {
            return Err(CommandError::AlreadyAtWar(a, b));
        }
        let mut events = Vec::new();
        self.declare_war_with_calls(a, b, CalledAllies::default(), &mut events)?;
        Ok(events)
    }

    /// [`Self::declare_war_rules`] from step 1 on, with the allies in `called` already answered (a
    /// fresh declaration passes none; a resumed one the waiting call's lists).
    fn declare_war_with_calls(&mut self, a: FactionId, b: FactionId, called: CalledAllies, events: &mut Vec<CampaignEvent>) -> Result<(), CommandError> {
        let called = match self.call_allies(a, b, called, events) {
            CallOutcome::Answered(called) => called,
            CallOutcome::Asked(call) => {
                events.push(CampaignEvent::AllyWarCallPending {
                    ally: call.ally,
                    declarer: call.declarer,
                    target: call.target,
                    allied_to_declarer: call.allied_to_declarer,
                    asked_by_target_side: call.asked_by_target_side,
                });
                self.pending_ally_war_calls.push(call);
                return Ok(());
            }
        };
        if self.is_major(a) && self.is_major(b) {
            for recipient in self.listed_third_parties(a, b) {
                if !called.contains(recipient) {
                    events.push(CampaignEvent::Message { recipient, message: CampaignMessage::WarDeclared { declarer: a, target: b } });
                }
            }
        }
        events.extend(self.war_on(a, b, None)?);
        // Then the zones of control again (`0x00B268B0` → `0x00B795A0`, CONFIRMED order): `a`'s allies
        // that joined, `b`'s that joined, `b`'s that broke with it, then `b`, then `a`. (Every
        // `0x00B26700` of `0x00B268B0` passes 0 for its own refresh flag.)
        for x in called.joined_a.into_iter().chain(called.joined_b).chain(called.torn_b).chain([b, a]) {
            self.refresh_faction_zones(x);
        }
        Ok(())
    }

    /// Command `RespondToAllyWarCall`: the human ally's answer to the top waiting call
    /// (`CCQ_RESPOND_TO_ALLY_WAR_INVITATION`, handler `0x009370F0`: it stores the accept byte, `0x008A8190`
    /// → offer +0x62, and the side, `0x008EFCE0` → +0x14, then pops the offer and applies it,
    /// `0x008BF910` → `ApplyAllyWarCallAnswer` `0x008BFA80`; CONFIRMED by disassembly):
    /// - refused: as the target's ally, it is listed as broken with the target (`0x00B27500`) and breaks
    ///   the alliance ([`Self::break_alliance`], `0x00B13840` with no war flag); as the declarer's ally, it
    ///   is listed as refusing (`0x00B0DB40`) and keeps its alliance;
    /// - accepted for the target (`side` = the target, asked as its ally): listed as joined
    ///   (`0x00B274E0`) and joins ([`Self::join_war`]: its war on the declarer unless already at war, the
    ///   access saved for the war, `0x00B26700` / `0x00B0CCE0`); if it was the declarer's ally too, it is
    ///   also listed as refusing the declarer;
    /// - accepted for the declarer: likewise listed as joined (`0x00741C70`) and joins the war on the
    ///   target; if it was the target's ally too, it is also listed as broken with the target (its own
    ///   war has broken that alliance, `0x00B27070`).
    ///
    /// Then the declaration resumes (`0x008BFA80` calls `0x00B268B0` again with the offer's lists): the
    /// next human ally gets its call, or the war is declared. `side` is ignored on a refusal. An
    /// acceptance must name a side the ally was asked for; the exe's handler stores any side, and one
    /// that matches neither lists the ally nowhere, so the resumed declaration asks it again: ours
    /// refuses the command instead and the same call keeps waiting (the same state, without the
    /// round trip).
    pub fn respond_to_ally_war_call(&mut self, side: Option<FactionId>, accept: bool) -> Result<Vec<CampaignEvent>, CommandError> {
        let call = self.pending_ally_war_calls.last().ok_or(CommandError::Unsupported("no call to war is waiting for an answer"))?;
        if accept && !((call.allied_to_target && side == Some(call.target)) || (call.allied_to_declarer && side == Some(call.declarer))) {
            return Err(CommandError::Unsupported("the ally can only join the war on a side it is allied to"));
        }
        let Some(AllyWarCall { declarer, target, ally, allied_to_target, allied_to_declarer, mut called, .. }) = self.pending_ally_war_calls.pop() else {
            unreachable!("the top call was read above");
        };
        let mut events = Vec::new();
        let joined = if !accept {
            if allied_to_target {
                called.torn_b.push(ally);
                let result = self.break_alliance(ally, target);
                Self::answer_events(result, &mut events);
            }
            if allied_to_declarer {
                called.refused_a.push(ally);
            }
            None
        } else if side == Some(target) {
            called.joined_b.push(ally);
            if allied_to_declarer {
                called.refused_a.push(ally);
            }
            Some((declarer, target))
        } else {
            called.joined_a.push(ally);
            if allied_to_target {
                called.torn_b.push(ally);
            }
            Some((target, declarer))
        };
        if let Some((enemy, side)) = joined {
            let result = self.join_war(ally, enemy, side);
            Self::answer_events(result, &mut events);
        }
        if let Err(e) = self.declare_war_with_calls(declarer, target, called, &mut events) {
            // Unreachable while the pair stays a valid one: its only errors are the pair's.
            debug_assert!(false, "respond_to_ally_war_call: {e}");
            log::warn!("respond_to_ally_war_call: {declarer:?}'s war on {target:?} could not resume: {e}");
        }
        Ok(events)
    }

    /// The events of one of the answer's treaty steps; an error is logged (unreachable: the factions
    /// of a waiting call are known ones other than their enemy).
    fn answer_events(result: Result<Vec<CampaignEvent>, CommandError>, events: &mut Vec<CampaignEvent>) {
        match result {
            Ok(mut e) => events.append(&mut e),
            Err(e) => {
                debug_assert!(false, "respond_to_ally_war_call: {e}");
                log::warn!("respond_to_ally_war_call: {e}");
            }
        }
    }

    /// `a`'s war on `b` itself (`DeclareWarOnFactionRecords` `0x00B26700`), on `ally`'s behalf when `a`
    /// joins `ally`'s war (`None`: the declaration's own war). CONFIRMED by disassembly:
    /// - `a`→`b` (`ApplyWarToDeclarerRecord` `0x00B27070`): the `war` factor set from the `war` event
    ///   (`war_dragged_by_ally` for a joining war, whose #5 is then `ally`); a patron loses `allows_region_return`; an alliance,
    ///   patronage or protectorate is broken (`0x00B13840` with its war flag set,
    ///   [`Self::break_alliance_with`]`(.., true)`: the same steps as [`Self::break_alliance`], but no one
    ///   is sent out of the other's regions, neither for an access given back as none nor by a
    ///   protectorate tie's end); the access `a` gives `b` ends; a trade agreement is broken
    ///   ([`Self::break_trade`]); during friendship turns the declarer is a backstabber (`backstab`; not
    ///   for a joining war); payments are dropped; the stance war.
    /// - `b`→`a` (`ApplyWarToTargetRecord` `0x00B27250`, given the same `ally`): the `war` factor
    ///   likewise (`war_dragged_by_ally` for a joining war); payments dropped; the stance war; if `b` gave `a` military access, `a` is
    ///   charged with abusing it (`0x00B0DA90`), or for a joining war the access only ends.
    /// - not for a joining war, every other listed faction X ([`Self::listed_third_parties`]: the
    ///   world's faction list +0xF5C, so never the rebels): if X's attitude to `b` is hostile (category 0) and
    ///   `b` is neither the rebels nor the pirates (`IsFactionExemptFromWarApproval` `0x008CEE00`:
    ///   `0x008CEEF0` / `IsPiratesFaction` `0x008CEE20`, tested on `b`), X likes `a` more (`war_against_enemies` on X's
    ///   `declared_war_against_enemies`); else if very friendly (4), less (`war_against_friends`).
    ///
    /// First of all, both sides' `WarStartedCharacterFaction` flag is set (faction +0x925,
    /// [`FactionDetails::war_started`](super::details::FactionDetails::war_started): `b`'s at
    /// `0x00B2670D`, then `a`'s at `0x00B26716`), for a joining war too.
    ///
    /// For the declaration's own war (no `ally`), `0x00B27070` first moves the war factor of the classes of
    /// `a`'s capital by `a`'s attitude to `b` (`0x00A63490(6, b)` at `0x00B270DC`, [`Self::move_class_war`]
    /// with [`ClassWarEvent::WarDeclared`]), before it sets the `war` factor; a joining war does not
    /// (CONFIRMED: the call is skipped when `0x00B27070` has an ally or the skip-notice flag, which only the
    /// setup's `0x00B45C20` passes, and the model sets that war directly).
    ///
    /// ORIGINAL BUG: the exe reads the capital's vtable unchecked (`0x00B270D0`, after `0x008B3790` returns 0
    /// for a faction without one), so a declarer without a capital crashes; ours skips the step (the battle path
    /// `0x008ACCF0` tests `0x008C9200`, has a capital, before its same call). Reached by every own war with no
    /// skip flag, none testing the capital: the declare-war command (`0x00932B70` → `0x00B268B0` → `0x00B26700`),
    /// a deal's war record (`0x00C4B990` → `0x00B268B0`) and `0x008BFA80` / `0x008BFCA0`; by a declarer loaded
    /// with no capital (`0x0087A190`) or left without one by an owner change that found no other in its home
    /// theatre (`0x008BA580` → `0x008DABC0`) and did not end it (CAMPAIGN_FIDELITY.md §Population).
    fn war_on(&mut self, a: FactionId, b: FactionId, ally: Option<FactionId>) -> Result<Vec<CampaignEvent>, CommandError> {
        for f in [b, a] {
            // Every loaded faction has its details; a write here never makes a record for one without.
            if let Some(d) = self.world.faction_details.get_mut(&f) {
                d.war_started = true;
            }
        }
        let stance = self.world.stance(a, b);
        let war = event(if ally.is_some() { "war_dragged_by_ally" } else { "war" });
        let mut events = Vec::new();
        if ally.is_none()
            && let Some(capital) = self.world.capital(a)
        {
            self.move_class_war(capital, ClassWarEvent::WarDeclared, b);
        }
        {
            let r = self.relationship_mut(a, b);
            r.set_factor("war", war);
            if ally.is_some() {
                r.war_ally = ally;
            }
        }
        if matches!(stance, Stance::Allied | Stance::Patron | Stance::Protectorate) {
            if stance == Stance::Patron {
                self.relationship_mut(a, b).allows_region_return = false;
            }
            events.extend(self.break_alliance_with(a, b, true)?);
        }
        self.relationship_mut(a, b).military_access_turns = 0;
        if self.world.trade_held(a, b) {
            events.extend(self.break_trade(a, b)?);
        }
        let friendship = self.world.relationships.get(&(a, b)).map_or(0, |r| r.friendship_turns);
        if friendship != 0 && ally.is_none() {
            self.backstab(a, b, friendship);
        }
        // `a`'s regular payments to `b` end (`0x00B27070`), each item's lines taken back by its
        // destructor (`0x0086A120`).
        self.change_treaty_money(a, b, |r| r.payments.clear());
        let gave_access = {
            let r = self.relationship_mut(b, a);
            r.set_factor("war", war);
            r.military_access_turns != 0
        };
        // `b`'s regular payments to `a` end likewise (`0x00B27250` → `0x0086A120`).
        self.change_treaty_money(b, a, |r| r.payments.clear());
        // The stance turns to war (0x00B27250) before the abuse's cancel, so `a`'s forces in `b`'s
        // land may stay through its expulsion. `0x00B26700` has no at-war test: a deal's join-war item
        // against a faction already at war with the joiner applies the records again
        // ([`Self::join_war_by_deal`]), the stance staying war. The other callers test it themselves
        // and keep their answer: [`Self::declare_war_rules`] returns `AlreadyAtWar` before anything
        // changes, [`Self::join_war`] skips the war.
        if stance != Stance::War {
            events.extend(self.world.declare_war(a, b)?);
        }
        if gave_access {
            match ally {
                Some(_) => self.relationship_mut(b, a).military_access_turns = 0,
                None => self.abuse_access(b, a, &mut events),
            }
        }
        if ally.is_none() {
            let enemies_count = !self.is_rebel_faction(b) && !super::director::is_pirate(self, b);
            for x in self.listed_third_parties(a, b) {
                match self.attitude_category(x, b) {
                    0 if enemies_count => self.relationship_mut(x, a).add_factor("declared_war_against_enemies", event("war_against_enemies")),
                    4 => self.relationship_mut(x, a).add_factor("declared_war_against_friends", event("war_against_friends")),
                    _ => {}
                }
            }
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
    fn abuse_access(&mut self, owner: FactionId, a: FactionId, events: &mut Vec<CampaignEvent>) {
        self.cancel_military_access(owner, a, events);
        let count = self.world.access_abuses.get(&a).copied().unwrap_or(0);
        let e = event("abused_military_access");
        self.relationship_mut(owner, a).factor_mut("abused_military_access").add(e.value + ABUSED_ACCESS_STEP * count as i32, e.drift, e.limit);
        self.world.access_abuses.insert(a, count.wrapping_add(1));
        self.diplomacy_changed();
    }

    /// `a` and `b` make peace (`0x00B262C0` → `0x00B26590` on both records):
    /// the `war` factor gets the `peace` event added (`peace_dragged_by_ally` if the side was in the war
    /// for an ally, `war_ally`, which is then cleared); friendship 10 turns; the war counters (#10–#13)
    /// zeroed. The access each side's alliances had suspended for the war on the other is restored
    /// (`0x00B0D0C0`: `a`'s alliances, then `b`'s; the exe does `a`'s only, an ORIGINAL BUG fixed below),
    /// and a side given back no access leaves the other's regions. Unless
    /// `suppress_messages`, each side gets the `ceasefire_declared` message about the other, `b` first
    /// (`0x009D1F20`, posted with no human test; the flag is the `CCQ_MAKE_PEACE` command's byte, set only
    /// by the script's `force_make_protectorate` `0x0097AA20`, never saved), after the peace's own trade price
    /// update with flag 0 ([`Self::update_commodity_prices_with`], `0x00BCB020(0)` at `0x00B2643F`, on every
    /// peace; each siege end, blockade lift and raid clear before it runs its own). Last, a
    /// [`CampaignFeatures::peace_access_grants`](super::features::CampaignFeatures::peace_access_grants) row
    /// gives its grantee access (`0x00B444B0`). CONFIRMED by disassembly of `0x00B262C0`.
    pub fn make_peace_rules(&mut self, a: FactionId, b: FactionId, suppress_messages: bool) -> Result<Vec<CampaignEvent>, CommandError> {
        let mut events = self.world.make_peace(a, b)?;
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
        // ally's record towards `a` (CONFIRMED, `0x00B262C0`'s loop over its own manager's records).
        // ORIGINAL BUG (0x00B262C0): the loop runs on the proposer's manager alone (`this` = the first
        // faction's +0x528: `HandleMakePeaceCommand` `0x00936710`, `ApplyPeaceRecordOnAccept` `0x00C18B60`
        // with the deal's proposer), so `b`'s allies that joined `b`'s war on `a` keep their indefinite
        // access and their `allied_in_war_against` item for `a` after the peace: the saved access is never
        // given back (only an alliance break, or that ally's own peace with `a` as the proposer, clears it),
        // the cancel stays forbidden (`0x00C1A5D0` → `0x00B0CCD0`), and which side keeps them depends only on
        // who proposed, while every other step of `0x00B262C0` (both records, both zone floods, both
        // character walks, both messages) treats the two sides alike. Ours runs the same loop for `b`'s
        // alliances (enemy `a`) after `a`'s.
        for (side, enemy) in [(a, b), (b, a)] {
            let allies: Vec<FactionId> = self
                .world
                .record_targets_in_order(side)
                .into_iter()
                .filter(|&x| matches!(self.world.stance(side, x), Stance::Allied | Stance::Patron | Stance::Protectorate))
                .collect();
            for ally in allies {
                for (owner, target) in [(side, ally), (ally, side)] {
                    if self.restore_access_suspended_for(owner, target, enemy) {
                        self.expel_from_lands_of(target, owner, &mut events);
                    }
                }
            }
        }
        // Then, per side, the zone flood `0x00B795A0` and `ClearCommerceRaidsBetweenFactions` `0x00A25570` on
        // each character of its character list towards the other: the proposer's (`0x00B26392`, the walk's call
        // at `0x00B263C5`), then the target's (`0x00B263D7`, `0x00B26409`; CONFIRMED, disassembly). The loop
        // above comes first, so a raider its expulsion moves has lost his raids (`move_character`) without a
        // price update before the walk reaches him. Each character's steps run in turn, and each that ends
        // something runs its own price update, whose raid check can end a later character's raids first; so
        // the walk is per character.
        for (side, other) in [(a, b), (b, a)] {
            self.refresh_faction_zones(side);
            for c in self.world.faction_characters_in_list_order(side) {
                self.end_war_actions_on_peace(c, other);
            }
        }
        // `0x00B262C0`'s own price update `0x00BCB020(0)` (the call at `0x00B2643F`, after both sides'
        // `0x00A25570` loops and the path grid's `0x00B67490`, before the messages), on every peace: one more
        // than the siege ends, lifts and raid clears ran (CONFIRMED, disassembly).
        self.update_commodity_prices_with(false);
        if !suppress_messages {
            for (recipient, other) in [(b, a), (a, b)] {
                events.push(CampaignEvent::Message { recipient, message: CampaignMessage::CeasefireDeclared { other } });
            }
        }
        let (ka, kb) = (self.world.factions[&a].key.as_str(), self.world.factions[&b].key.as_str());
        let grants: Vec<(FactionId, FactionId, i32)> = self
            .rules
            .features
            .peace_access_grants
            .iter()
            .filter_map(|g| g.granter_of((ka, a), (kb, b)).map(|(granter, grantee)| (granter, grantee, g.turns)))
            .collect();
        for (granter, grantee, turns) in grants {
            events.extend(self.grant_military_access(granter, grantee, turns)?);
        }
        self.diplomacy_changed();
        Ok(events)
    }

    /// A peace's step for character `c` towards the other side `other` (`ClearCommerceRaidsBetweenFactions`
    /// `0x00A25570`, CONFIRMED, disassembly), in the exe's order: his force (character +0x2A8 as an army,
    /// `0x009D3A40`) besieging a settlement `other` owns ends that siege ([`Self::end_siege_on_peace`], the
    /// call at `0x00A255C5`); his force as a navy (`0x009D3A80`) blockading a port `other` holds lifts it
    /// ([`Self::lift_blockade_on_peace`], `0x00A25627`); then, while he still raids and one of his raids is
    /// on `other` (`0x008BC560`, `0x008CEEB0`), his raids end ([`Self::end_raids_on_peace`], `0x009DA140`).
    /// Each step that ends something runs the price update `0x00BCB020(0)`.
    fn end_war_actions_on_peace(&mut self, c: CharacterId, other: FactionId) {
        self.end_siege_on_peace(c, other);
        self.lift_blockade_on_peace(c, other);
        self.end_raids_on_peace(c, other);
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
            self.expel_from_lands_of(b, a, &mut events);
        }
        // 0x00B0CA20 on `b`→`a`: a protectorate's tie to its patron `a` ends first (the war flag passed
        // as its skip flag), then the record's own steps. The tie's move notices pass on; its last event,
        // the pair's change to neutral, is the same change pushed once below, so it is dropped here.
        if b_protectorate {
            let mut tie = self.end_protectorate_tie_with(b, a, from_war)?;
            debug_assert!(matches!(tie.last(), Some(CampaignEvent::StanceChanged { stance: Stance::Neutral, .. })));
            tie.pop();
            events.extend(tie);
        }
        {
            let r = self.relationship_mut(b, a);
            r.factor_mut("alliance").reset(0);
            r.set_factor("alliance_broken", event("alliance_broken"));
            r.alliance_commitment_turns = 0;
        }
        // The income and tribute lines cleared, each booking −old (`0x00B0CA9A` / `0x00B0CAB6`).
        self.change_treaty_money(b, a, |r| (r.protectorate_income, r.protectorate_tribute) = (0, 0));
        let lost = self.restore_first_suspended_access(b, a);
        if lost && !from_war {
            self.expel_from_lands_of(a, b, &mut events);
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
        if self.world.trade_held(a, x) {
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
    /// the stance turned to war, so they may stay there; the move notices go to `events`). Returns the grievance added (the
    /// command path, `0x00B67B20`, also adds a third of it to `a`'s manager +0x20: [`World::access_cancel_marks`](super::World::access_cancel_marks)).
    pub fn cancel_military_access(&mut self, a: FactionId, b: FactionId, events: &mut Vec<CampaignEvent>) -> i32 {
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
        self.expel_from_each_region_of(b, a, events);
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
    /// record towards the payee (`0x00B54FC0` → `0x00B55010`, CONFIRMED), and the friendship raised to
    /// min(turns, 10) (S1_LEFTOVERS.md #15). The item books `amount` into the payer's category 22 and the
    /// payee's category 8 while it lives ([`Self::change_treaty_money`]; `0x00B55010` books a temporary item,
    /// pushes its copy and destroys the temporary, a net single booking), so each round end pays it once.
    pub fn regular_payment(&mut self, payer: FactionId, payee: FactionId, amount: i32, turns: u32) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(payer, payee)?;
        self.pay_regularly(payer, payee, amount, turns);
        Ok(Vec::new())
    }

    /// The record of [`Self::regular_payment`], for a caller that has checked the faction pair.
    pub(super) fn pay_regularly(&mut self, payer: FactionId, payee: FactionId, amount: i32, turns: u32) {
        let r = self.relationship_mut(payer, payee);
        r.friendship_turns = r.friendship_turns.max(turns.min(10));
        self.change_treaty_money(payer, payee, |r| r.payments.push(RegularPayment { amount, turns }));
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
    /// `0x00B262C0`; BACKLOG).
    ///
    /// With the stances the money starts, before the other alliances break: the protectorate's tribute
    /// (`0x00B104E0`'s end, `0x00AF8130`) and the patron's income (`0x00B10480`), each a fifth of the
    /// protectorate's running categories 5..7 (`0x00BBCD00`, [`Self::refresh_protectorate_money`]).
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
        self.refresh_protectorate_money(protectorate, patron);
        self.refresh_protectorate_money(patron, protectorate);
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

    /// The `CCQ_DECLARE_WAR` command (`HandleDeclareWarCommand` `0x00932B70`, CONFIRMED by disassembly):
    /// both factions exist and differ, both are in the game ([`Self::in_the_game`]), then
    /// [`Self::declare_war_rules`] (the handler calls `DeclareWarAndCallAllies` `0x00B268B0` on `a`'s
    /// diplomacy with `b`, both sides' allies called, no negotiation context). Against the exe:
    /// - in the game: the command's payload writer (`0x00953CA0`, vtable `0x0136190C` slot 0) writes the
    ///   two factions only when neither has +0x824 set; with no payload the handler's reader
    ///   (`0x008FDEB0`) leaves both 0 and the handler does nothing. Ours refuses ([`CommandError::OutOfGame`]).
    /// - ORIGINAL BUG: with `a == b` the handler reaches `0x00B268B0` with the declarer as its own
    ///   target; `FindDiplomacyRelationshipByTarget` `0x00B64C50` misses and returns the first record, so
    ///   `DeclareWarOnFactionRecords` `0x00B26700` writes the war onto whichever faction comes first in
    ///   `a`'s list. The exe's intent is no self-war: its at-war test `0x008CE9B0` is false for `a == b`,
    ///   and the peace handler `0x00936710` and `force_make_protectorate` `0x0097AA20` refuse it. Ours
    ///   refuses ([`CommandError::SameFaction`]).
    /// - ORIGINAL BUG: already at war. The handler, `0x00B268B0` and `0x00B26700` have no at-war test
    ///   for the pair, and two senders test nothing either: the console's `declare_war`
    ///   (`Console_CmdDeclareWar` `0x00930CD0`, the turn faction on the named one) and the UI's
    ///   `DeclareWarInstant` (`HandleCampaignUIDeclareWarInstant` `0x009E2090`). Through them the exe
    ///   declares the war again: both sides' allies are called again (a human ally not yet at war gets
    ///   the offer again), every listed third party gets the `war_declared` notice again, and
    ///   `0x00B26700` adds the third parties' `declared_war_against_*` attitude factors and the war's own
    ///   factors once more and clears the payments, with the stance already war. The exe's intent is no
    ///   declaration between factions at war: every other path tests `IsFactionAtWarWith` `0x008CE9B0`
    ///   first (`force_declare_war` at `0x0097A9D1`; the deal's war action is offered only without a war,
    ///   `0x00C1A890`; `0x00B268B0` guards each ally's own war with it; the AI declares only on a neutral
    ///   stance). Ours refuses ([`CommandError::AlreadyAtWar`], in [`Self::declare_war_rules`]) before
    ///   anything changes. The console is a developer tool and not ported; `DeclareWarInstant` is
    ///   (ntw_script `ui/campaign/diplomacy.rs`).
    /// - Not modelled: when either side is human (+0x6E0) the handler then posts model event 0xBC (the
    ///   declarer, no position) on the model's event broadcaster (+0x9C0, `0x008DBAC0`). Its one listener
    ///   (the object `0x0090AE10` builds, handler `0x009593A0`) passes it on only when the event's faction
    ///   is its own (+0xB4), so only the local declarer hears it. It is a sound: 0xBC is sound slot 188,
    ///   `CAMPAIGN_EVENT_WAR_DECLARED` (ntw_formats `sound/slots.rs`); the event carries no sound name
    ///   (+0x0C holds the default key at `0x01457CAC`), so the listener plays slot +0x10 through its +0x74 object, the sound
    ///   manager (vtable +0x1C), and treats slot 0x191 as no sound. No model state reads it; the sound
    ///   belongs to the campaign audio, not this rule.
    pub fn declare_war_command(&mut self, a: FactionId, b: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(a, b)?;
        if let Some(f) = [a, b].into_iter().find(|&f| !self.in_the_game(f)) {
            return Err(CommandError::OutOfGame(f));
        }
        self.declare_war_rules(a, b)
    }

    /// The `CCQ_MAKE_PROTECTORATE` command (`HandleMakeProtectorateCommand` `0x009367A0`, CONFIRMED):
    /// both factions exist and differ, `patron` is not at war with `protectorate` ([`Self::at_war`],
    /// `0x008CE9B0`) and neither side already has a patron ([`Self::has_patron`], `0x008CEEA0`);
    /// then `protectorate` becomes `patron`'s protectorate ([`Self::make_protectorate`]: the handler calls
    /// `0x00B105C0` on the protectorate's diplomacy with the patron and the deal's peace step off).
    pub fn make_protectorate_command(&mut self, patron: FactionId, protectorate: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(patron, protectorate)?;
        if self.at_war(patron, protectorate) {
            return Err(CommandError::AlreadyAtWar(patron, protectorate));
        }
        if let Some(f) = [patron, protectorate].into_iter().find(|&f| self.has_patron(f)) {
            return Err(CommandError::HasPatron(f));
        }
        self.make_protectorate(protectorate, patron)
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
        // Each line cleared books −old (`0x00B64C10` → `0x00AF8130(0)`, `0x00B54530`).
        self.relationship_mut(protectorate, patron);
        self.relationship_mut(patron, protectorate);
        self.change_treaty_money(protectorate, patron, |r| r.protectorate_tribute = 0);
        self.change_treaty_money(patron, protectorate, |r| r.protectorate_income = 0);
        let mut events = Vec::new();
        if !skip_expulsion {
            self.expel_from_lands_of(patron, protectorate, &mut events);
        }
        self.diplomacy_changed();
        events.push(CampaignEvent::StanceChanged { a: protectorate, b: patron, stance: Stance::Neutral });
        Ok(events)
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
    ///    forces out of the faction's regions ([`Self::expel_from_lands_of`]). When the count-down leaves
    ///    one turn (2 → 1), the target gets the `military_access_prompt` message about the faction
    ///    ([`CampaignMessage::MilitaryAccessPrompt`], `0x00A0E630`), returned with the other events.
    /// 4. `allied_in_war_against`: items of factions out of the game dropped, saved access turns −1;
    /// 5. friendship (#15) −1, then 10 again while allied (stance 2–4) or while the target gives the owner
    ///    military access;
    /// 6. at war: the war balances #10 and #11 (`war_balances`), #12 and #13 +1;
    /// 7. war momentum (#7): positive −2 (to 0), negative +1 (to 0);
    /// 8. the protectorate tribute or patron income set again ([`Self::refresh_protectorate_money`]); regular
    ///    payments: turns −1, finished ones removed, their money taken back out of both running records
    ///    ([`Self::change_treaty_money`]). The faction's own settle has already paid them this round (the
    ///    payer's step runs after its settle). ORIGINAL BUG (`0x0086A120`): the exe removes a finished item
    ///    here at once, so a payee whose economy runs later in the round loses the last payment; ours
    ///    removes it after the payee's settle ([`Self::drop_finished_payments_to`]), so the payee receives
    ///    every payment the payer makes in either turn order;
    /// 9. the cancel grievance (#26) −2 (to 0); embargo turns (#27) −1.
    ///
    /// Then the manager's own counter (+0x20, [`World::access_cancel_marks`](super::World::access_cancel_marks)) −1.
    pub fn diplomacy_round_end(&mut self, faction: FactionId, events: &mut Vec<CampaignEvent>) {
        // A faction out of the game updates nothing (0x00B29100 tests the owner's +0x824 first, [`Self::in_the_game`]).
        // The vanilla saves agree: such factions' records do not drift.
        if !self.in_the_game(faction) {
            return;
        }
        // 0x00B29100 walks the faction's record vector (stride 0x848) in its own order (CONFIRMED), so the
        // messages and expulsions come in record order.
        let targets = self.world.record_targets_in_order(faction);
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
                    // 0x00B291E0: one turn left, so the holder `t` gets the prompt titled with the
                    // granter's name (`0x00A0E630`(t, faction), CONFIRMED by disassembly).
                    if n == 2 {
                        events.push(CampaignEvent::Message { recipient: t, message: CampaignMessage::MilitaryAccessPrompt { granter: faction } });
                    }
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
                self.expel_from_lands_of(t, faction, events);
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
            r.access_cancel_grievance -= r.access_cancel_grievance.min(2);
            r.trade_embargo_turns = r.trade_embargo_turns.saturating_sub(1);
            // Step 8's money (no field above reads it): the tribute or income set again, then the payments
            // tick and the finished ones go, each taking its lines back (`0x0086A120`). ORIGINAL BUG: the
            // exe removes a finished item here even when the payee's economy is still ahead this round, so
            // its income line is gone before the payee's settle and the payee gets one payment fewer than
            // the payer makes (`0x00B29170` → `0x0086A120`). Ours keeps such an item (at 0 turns) until the
            // payee has settled ([`Self::drop_finished_payments_to`]).
            self.refresh_protectorate_money(faction, t);
            let payee_settles_later = self.turn.queue.contains(&super::TurnStep::Economy(t));
            self.change_treaty_money(faction, t, |r| {
                for p in &mut r.payments {
                    p.turns = p.turns.saturating_sub(1);
                }
                if !payee_settles_later {
                    r.payments.retain(|p| p.turns != 0);
                }
            });
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
                let mut events = Vec::new();
                let g = self.cancel_military_access(a, b, &mut events);
                let m = self.world.access_cancel_marks.entry(a).or_insert(0);
                *m = m.wrapping_add((g / 3).max(0) as u32);
                // Then `b`'s zones of control again (`0x00B67B20` → `0x00B795A0`; `0x00B67BD0` refreshes none).
                self.refresh_faction_zones(b);
                // Then `b` gets the `alliance_broken` message "<b> - <a>" (`0x009CA7B0`, no human
                // or major test; the display shows only the viewed human's messages).
                events.push(CampaignEvent::Message { recipient: b, message: CampaignMessage::AllianceBroken { first: b, second: a } });
                // Then the model's +0x870 event ({a, b}): its only listener, the CAI's
                // `HandleCaiMilitaryAccessCancelledEvent` `0x00A8F950`, just times a profiling scope
                // (CONFIRMED by disassembly 2026-10-22), so nothing follows from it. Last the
                // diplomacy-changed event (+0x3A8).
                self.diplomacy_changed();
                Ok(events)
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
            self.world.factions.keys().copied().filter(|&f| self.in_the_game_with_record(f)).collect();
        let keys: Vec<(FactionId, FactionId)> = self.world.relationships.keys().copied().filter(|(o, _)| alive.contains(o)).collect();
        for (o, t) in keys {
            let leader = fx.faction(t, "diplomacy_bonus_faction_leader") as i32;
            let enlightenment = fx.faction(t, "diplomacy_bonus_enlightenment") as i32;
            // Steps 1 and 2: the tribute or income set again from the protectorate's running record.
            self.refresh_protectorate_money(o, t);
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
    /// CONFIRMED). An own save from before the flag was kept lacks it; then the flag is derived as the exe
    /// derives it ([`Self::out_of_game_by_startpos_rule`]).
    pub fn in_the_game(&self, f: FactionId) -> bool {
        match self.world.faction_details.get(&f).and_then(|d| d.out_of_game) {
            Some(out) => !out,
            None => !self.out_of_game_by_startpos_rule(f),
        }
    }

    /// The gate of most of a faction's round-end economy (`RunFactionRoundEndEconomy` `0x008BC650`,
    /// CONFIRMED by listing): a faction with a record (not the rebels, `IsFactionWithoutRecord`
    /// `0x008CEEF0`) still in the game (faction +0x824 clear, [`Self::in_the_game`]). The function tests
    /// both before the settle `0x00BABE30` (`0x008BC65D` / `0x008BC666`), the family year `0x008BC9C0`
    /// (`0x008BC6C7` / `0x008BC6D0`), the block of construction, diplomacy and research (`0x008BC862` /
    /// `0x008BC86B`) and its last two steps (`0x008BC8C6` / `0x008BC8CF`).
    pub fn in_the_game_with_record(&self, f: FactionId) -> bool {
        !self.is_rebel_faction(f) && self.in_the_game(f)
    }

    /// The exe's own rule for faction +0x824 (`UpdateFactionOutOfGameFlag` `0x008B4330`, CONFIRMED): a
    /// faction is out of the game when it holds no region and every character of its list holds a
    /// government post (character +0x2B8, the post handle `0x009CB1E0` sets; a faction with no character
    /// is out). The exe runs it only when it preprocesses a `startpos.esf` (`0x00A19350` → `0x00988B00` →
    /// the campaign model's creation `0x008742C0` → `0x0090C4A0`), so every campaign reads the result
    /// from the file; in play only the destruction (`0x008AB6C0`: no region left and, for an AI faction,
    /// no character; a human faction's ends the campaign) sets the flag and only a re-emergence
    /// (`0x008AE6E0`: a region again) clears it, both agreeing with this rule (the other byte writers of
    /// +0x824 are the three faction constructors' clears and the loader). It differs from the stored flag only for a faction that starts in
    /// the game without a region and later loses every character holding no post: the exe keeps it in
    /// the game, which an own save without the flag cannot show. The rebels are never tested (the loop at
    /// `0x0090C860` walks the faction list, which they are not in): their flag stays clear, as it is in all
    /// eight vanilla start positions (real-install test `loads_all_eight_startpos`, where the rule gives
    /// every other faction's stored flag).
    pub fn out_of_game_by_startpos_rule(&self, f: FactionId) -> bool {
        if self.is_rebel_faction(f) || self.world.regions.values().any(|r| r.owner == f) {
            return false;
        }
        let posts = self.world.faction_details.get(&f).map(|d| d.posts.as_slice()).unwrap_or_default();
        self.world.characters.values().filter(|c| c.faction == f).all(|c| posts.iter().any(|p| p.holder == Some(c.id)))
    }

    /// Changes the treaty money of `owner`'s record towards `target` (its regular payments, protectorate
    /// tribute or patron income) through `change`, booking it into the running records as the exe's
    /// trackers do: the old amounts taken back, the new ones booked
    /// ([`super::economy::book_treaty_money`]). Every writer of the three goes through here, so the
    /// records' categories 8 / 10 / 22 / 24 always hold the live amounts, which the round end pays
    /// ([`super::economy::settle_round`]). Nothing without a record.
    pub(super) fn change_treaty_money(&mut self, owner: FactionId, target: FactionId, change: impl FnOnce(&mut Relationship)) {
        if !self.world.relationships.contains_key(&(owner, target)) {
            return;
        }
        super::economy::book_treaty_money(self, owner, target, -1);
        if let Some(r) = self.world.relationships.get_mut(&(owner, target)) {
            change(r);
        }
        super::economy::book_treaty_money(self, owner, target, 1);
    }

    /// The regular payments to `payee` that finished in their payer's per-turn update earlier this round
    /// end, removed once `payee`'s own settle has paid their last round (run by the payee's round-end
    /// economy right after its settle, [`super::TurnStep::Economy`]), each taking its lines back
    /// (`0x0086A120`). ORIGINAL BUG fix: see [`Self::diplomacy_round_end`] step 8.
    pub(super) fn drop_finished_payments_to(&mut self, payee: FactionId) {
        let payers: Vec<FactionId> = self
            .world
            .relationships
            .iter()
            .filter(|((_, t), r)| *t == payee && r.payments.iter().any(|p| p.turns == 0))
            .map(|((o, _), _)| *o)
            .collect();
        for payer in payers {
            self.change_treaty_money(payer, payee, |r| r.payments.retain(|p| p.turns != 0));
        }
    }

    /// The protectorate tribute and patron income of `owner`'s record towards `target` set again from the
    /// protectorate's running record ([`super::economy::protectorate_tribute_value`]), CONFIRMED at the
    /// round end (`0x00B29170`, after the war momentum's drift) and the round start (`0x00B71EC0` steps 1
    /// and 2): owner the target's protectorate (stance 4, `0x00B5DCC0`): the tribute (`0x00AF8130` with
    /// `0x00BBCD00` of the owner's record); owner the target's patron (stance 3, `0x00B5DCB0`): the income
    /// (`0x00BBCD00` of the target's record). Any other stance keeps both as they are.
    fn refresh_protectorate_money(&mut self, owner: FactionId, target: FactionId) {
        match self.world.stance(owner, target) {
            Stance::Protectorate => {
                let tribute = super::economy::protectorate_tribute_value(self, owner);
                self.change_treaty_money(owner, target, |r| r.protectorate_tribute = tribute);
            }
            Stance::Patron => {
                let income = super::economy::protectorate_tribute_value(self, target);
                self.change_treaty_money(owner, target, |r| r.protectorate_income = income);
            }
            _ => {}
        }
    }

    /// The allies of both sides called into `a`'s war on `b` (`DeclareWarAndCallAllies` `0x00B268B0`,
    /// CONFIRMED flow and order), before the war itself: every faction X allied to `b` (stance allied,
    /// patron or protectorate), in the game, not `a` (`0x00898B80`), not yet answered for `b`'s side
    /// (in `called`'s lists +0x20 / +0x30, `0x00B673D0`): if X is already at war with `a` it is not asked
    /// and simply joins; else a human X is asked: its [`AllyWarCall`] is returned (`0x00885DA0` at
    /// `0x00B26C06`, pushed `0x00B26C1F`) and the calling stops there; an AI X decides through the AI
    /// (`0x00AAA950` → `0x00AAA920`, §6). Join ([`Self::join_war`]): X's own war on `a` on `b`'s behalf
    /// unless already at war, then the access of X and `b` is saved for the war (`0x00B0CCE0` on both
    /// records). Refuse: X breaks its alliance with `b` (`0x00B27500`, `0x00B13840`). Then the same for
    /// `a`'s allies against `b` (skipping those answered for `a`'s side, +0 / +0x10, `0x00B67380`; a human
    /// asked at `0x00B26DFE`), except that a refusing ally of `a` is only listed (`0x00B0DB40`) and keeps
    /// its alliance. The call records whether X is allied to the other side too (`0x008CE390`), so its
    /// answer lists it for both. PROVISIONAL AI decision: join, unless X is allied to the enemy as well
    /// (then it refuses). The events go to `events`; returns who joined and who broke with `b`, the lists
    /// the exe keeps for the zones' refresh, or the human ally's call.
    fn call_allies(&mut self, a: FactionId, b: FactionId, mut called: CalledAllies, events: &mut Vec<CampaignEvent>) -> CallOutcome {
        for (side, enemy) in [(b, a), (a, b)] {
            let target_side = side == b;
            // In the order of `side`'s diplomacy records (CONFIRMED: both loops walk the side's record
            // array, stride 0x848, front to back).
            let allies: Vec<FactionId> = self
                .world
                .record_targets_in_order(side)
                .into_iter()
                .filter(|&x| x != a && x != b && self.in_the_game(x))
                .filter(|&x| matches!(self.world.stance(x, side), Stance::Allied | Stance::Patron | Stance::Protectorate))
                .collect();
            for x in allies {
                if called.answered_for(target_side, x) {
                    continue;
                }
                let at_war = self.world.stance(x, enemy) == Stance::War;
                if !at_war && self.is_human(x) {
                    let allied_to_enemy = self.allied_with(x, enemy);
                    return CallOutcome::Asked(AllyWarCall {
                        declarer: a,
                        target: b,
                        ally: x,
                        allied_to_target: target_side || allied_to_enemy,
                        allied_to_declarer: !target_side || allied_to_enemy,
                        asked_by_target_side: target_side,
                        called,
                    });
                }
                let torn = !at_war && matches!(self.world.stance(x, enemy), Stance::Allied | Stance::Patron | Stance::Protectorate);
                let result = match (torn, side == b) {
                    (true, true) => self.break_alliance(x, side),
                    // A refusing ally of the attacker keeps its alliance (0x00B0DB40 only lists it).
                    (true, false) => Ok(Vec::new()),
                    (false, _) => self.join_war(x, enemy, side),
                };
                match result {
                    Ok(mut e) => {
                        events.append(&mut e);
                        let list = match (torn, side == a) {
                            (false, true) => &mut called.joined_a,
                            (false, false) => &mut called.joined_b,
                            (true, false) => &mut called.torn_b,
                            (true, true) => &mut called.refused_a,
                        };
                        list.push(x);
                    }
                    // Unreachable: every faction here is a known one other than its enemy.
                    Err(e) => {
                        debug_assert!(false, "call_allies: {e}");
                        log::warn!("call_allies: {x:?} could not join the war on {enemy:?}: {e}");
                    }
                }
            }
        }
        CallOutcome::Answered(called)
    }

    /// A deal's join-war item (`ApplyJoinWarRecordOnAccept` `0x00C188F0` → `0x00B26700` with its
    /// refresh flag set, CONFIRMED by disassembly): `x` declares war on `target` on `ally`'s behalf
    /// ([`Self::war_on`] with the ally: no allies called, no third parties' factors, no `war_declared`
    /// notices, no access saved for the war), with no at-war test (an existing war's records are
    /// applied again); then `0x00B26700`'s refresh step: the `campmap_faction_joins_your_enemies`
    /// notice to `target` ([`CampaignMessage::FactionJoinsYourEnemies`], `0x009DAE40`, no major-power
    /// test), then the zones of control of `target`, then of `x` (`0x00B795A0`). Not modelled: the
    /// path grid's view and the trade price update (`0x00B67490`, `0x00BCB020(0)`, no model state).
    ///
    /// The caller has checked the pair ([`Self::check_diplomacy_pair`], before any of the deal's
    /// records is applied); with that, [`Self::war_on`] cannot fail (its only errors are the pair's,
    /// and an existing war is no error here), so the item cannot leave a deal half-applied.
    pub(crate) fn join_war_by_deal(&mut self, x: FactionId, target: FactionId, ally: FactionId) -> Vec<CampaignEvent> {
        let mut events = match self.war_on(x, target, Some(ally)) {
            Ok(events) => events,
            // Unreachable: the caller checked the pair.
            Err(e) => {
                debug_assert!(false, "join_war_by_deal: {e}");
                log::warn!("join_war_by_deal: {x:?} could not declare war on {target:?} for {ally:?}: {e}");
                return Vec::new();
            }
        };
        events.push(CampaignEvent::Message { recipient: target, message: CampaignMessage::FactionJoinsYourEnemies { joiner: x } });
        for f in [target, x] {
            self.refresh_faction_zones(f);
        }
        events
    }

    /// `x` joins `ally`'s war on `enemy` (`0x00B268B0`'s joined step): `x`'s own war on `enemy` on
    /// `ally`'s behalf ([`Self::war_on`] with the ally) unless they are already at war, then
    /// `0x00B0CCE0` on (ally, x) and (x, ally): the access they give each other is saved for this war
    /// (#17) and becomes indefinite.
    pub fn join_war(&mut self, x: FactionId, enemy: FactionId, ally: FactionId) -> Result<Vec<CampaignEvent>, CommandError> {
        self.check_diplomacy_pair(x, enemy)?;
        let events = if self.world.stance(x, enemy) == Stance::War { Vec::new() } else { self.war_on(x, enemy, Some(ally))? };
        for (p, q) in [(ally, x), (x, ally)] {
            let r = self.relationship_mut(p, q);
            if !r.allied_in_war_against.iter().any(|w| w.enemy == enemy) {
                let saved = r.military_access_turns;
                r.allied_in_war_against.push(super::details::AlliedWar { enemy, saved_access_turns: saved });
            }
            r.military_access_turns = -1;
        }
        Ok(events)
    }
}

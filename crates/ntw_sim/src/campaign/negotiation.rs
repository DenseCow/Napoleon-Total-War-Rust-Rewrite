//! The negotiation screen's game rules: which diplomatic actions a negotiation offers
//! ([`CampaignModel::negotiation_actions`], [`CampaignModel::possible_actions`]), the negotiation
//! itself and the diplomat's greeting ([`Negotiations`], begun and ended by their commands) and the power / wealth
//! / prestige rankings ([`CampaignModel::faction_rankings`]).
//!
//! Traced in UI_FIDELITY.md §4.7 (CONFIRMED unless a line says otherwise). The UI only reads these.

use std::collections::{BTreeMap, BTreeSet};

use super::details::{DIPLOMACY_OPTIONS, Relationship, diplomacy_option_value, option_allows_acceptance, option_allows_proposal};
use super::ids::{FactionId, RegionId};
use super::world::{CampaignModel, Stance};

/// The diplomatic actions of a negotiation, by the exe's action id (`+0x0C` of an action record;
/// name table `0x014587F8`, read by `0x009C7700`). The name is the `diplomacy_button` template
/// state the panel puts the button in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NegotiationAction {
    /// 0 `trade`: ask for a trade agreement.
    Trade,
    /// 1 `trade_cancel`: cancel the trade agreement.
    TradeCancel,
    /// 2 `access`: military access.
    Access,
    /// 3 `access_cancel`: cancel the military access the proposer gives.
    AccessCancel,
    /// 4 `alliance`.
    Alliance,
    /// 5 `alliance_cancel`.
    AllianceCancel,
    /// 6 `regions`.
    Regions,
    /// 7 `technology`.
    Technology,
    /// 8 `state_gift`.
    StateGift,
    /// 9 `payments`.
    Payments,
    /// 10 `protector`: offer to become the recipient's protector.
    Protector,
    /// 11 `peace`.
    Peace,
    /// 12 `war`.
    War,
    /// 13 `request_join_war`.
    RequestJoinWar,
    /// 14 `break_trade`: trade embargoes.
    BreakTrade,
    /// 15 `break_alliance`.
    BreakAlliance,
}

impl NegotiationAction {
    /// Every action, by id.
    pub const ALL: [Self; 16] = [
        Self::Trade,
        Self::TradeCancel,
        Self::Access,
        Self::AccessCancel,
        Self::Alliance,
        Self::AllianceCancel,
        Self::Regions,
        Self::Technology,
        Self::StateGift,
        Self::Payments,
        Self::Protector,
        Self::Peace,
        Self::War,
        Self::RequestJoinWar,
        Self::BreakTrade,
        Self::BreakAlliance,
    ];

    /// The action with this name ([`Self::name`]), if any.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The action's name (`0x014587F8`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::TradeCancel => "trade_cancel",
            Self::Access => "access",
            Self::AccessCancel => "access_cancel",
            Self::Alliance => "alliance",
            Self::AllianceCancel => "alliance_cancel",
            Self::Regions => "regions",
            Self::Technology => "technology",
            Self::StateGift => "state_gift",
            Self::Payments => "payments",
            Self::Protector => "protector",
            Self::Peace => "peace",
            Self::War => "war",
            Self::RequestJoinWar => "request_join_war",
            Self::BreakTrade => "break_trade",
            Self::BreakAlliance => "break_alliance",
        }
    }

    /// The `force_diplomacy` option (`Relationship::diplomacy_options` index) that can forbid the
    /// action: the table `0x01459080` (0,0,1,2,3,3,4,5,6,7,8,9,10,11,12,13).
    pub fn option(self) -> usize {
        match self {
            Self::Trade | Self::TradeCancel => 0,
            Self::Access => 1,
            Self::AccessCancel => 2,
            Self::Alliance | Self::AllianceCancel => 3,
            Self::Regions => 4,
            Self::Technology => 5,
            Self::StateGift => 6,
            Self::Payments => 7,
            Self::Protector => 8,
            Self::Peace => 9,
            Self::War => 10,
            Self::RequestJoinWar => 11,
            Self::BreakTrade => 12,
            Self::BreakAlliance => 13,
        }
    }

    /// `Unilateral` of the panel's entry: true for ids 1, 3, 5, 8, 12 (`0x009B5220`).
    pub fn unilateral(self) -> bool {
        matches!(self, Self::TradeCancel | Self::AccessCancel | Self::AllianceCancel | Self::StateGift | Self::War)
    }
}

/// One action record of a negotiation (`InitNegotiationActionRecord` `0x00BF34F0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionRecord {
    /// The action (+0x0C).
    pub action: NegotiationAction,
    /// Type 1 (+0x08): listed under `OffersAndDemands`; type 0: under `Unilaterals`.
    pub offers_and_demands: bool,
    /// Possible (+0x10).
    pub possible: bool,
    /// Forbidden by the proposer's `force_diplomacy` option (+0x11).
    pub forbidden: bool,
}

impl ActionRecord {
    /// Available: possible and not forbidden (`IsNegotiationActionAvailable` `0x00C1A530`).
    pub fn available(&self) -> bool {
        self.possible && !self.forbidden
    }
}

/// One button the negotiation panel shows (`PushNegotiationPossibleActionsFromScript` `0x009B5220`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedAction {
    /// The action.
    pub action: NegotiationAction,
    /// `Active`: the action is available (an unavailable one is shown greyed).
    pub active: bool,
    /// `Tooltip`: a `random_localisation_strings` key, for the actions listed although unavailable.
    pub tooltip: Option<&'static str>,
}

/// The two button lists of the panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PossibleActions {
    /// `Unilaterals` (type 0), created first.
    pub unilaterals: Vec<ListedAction>,
    /// `OffersAndDemands` (type 1), under them.
    pub offers_and_demands: Vec<ListedAction>,
}

/// The rank categories of one faction (`BuildFactionRankingTable` `0x00949630`): 0 is the best
/// ("Terrifying", "Spectacular"), 5 the worst; the names are `random_localisation_strings`
/// `power_category_<c+1>`, `wealth_category_<c+1>`, `prestige_category_<c+1>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rankings {
    /// Power category.
    pub power: u8,
    /// Wealth category.
    pub wealth: u8,
    /// Prestige category.
    pub prestige: u8,
}

/// A diplomat's line picked when a negotiation begins ([`super::CampaignCommand::BeginNegotiation`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Greeting {
    /// A `diplomacy_strings` key (loc `diplomacy_strings_string_<key>`).
    Line(String),
    /// The data has no line: the exe says "Missing String" (`0x00C55CC0`).
    Missing {
        /// The speaking faction's key.
        faction: String,
        /// The row key looked up: (event, culture, government key).
        key: (String, String, String),
    },
}

/// The campaign negotiation (campaign +0xF9C, built by `0x008AF620` → `0x00BF5A60`): who
/// negotiates with whom and the lines its constructor picked. One at a time, as the campaign holds
/// one. Not saved and not hashed (a negotiation is open only inside the panel); the RNG draws its
/// constructor made are, through the campaign RNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiation {
    /// Which negotiation this is: [`Negotiations::begun`] when it began (a new one built over an
    /// old one is told apart by it).
    pub serial: u64,
    /// The proposer (+0x18).
    pub proposer: FactionId,
    /// The recipient (+0x1C).
    pub recipient: FactionId,
    /// The recipient's `receive_<attitude>` line, picked when the proposer is human (the "started"
    /// event's text, hub +0x318); `None` when the proposer is not human.
    pub greeting: Option<Greeting>,
    /// The proposer's `approach_<attitude>` line, picked when the recipient is human (hub +0x330);
    /// `None` when the recipient is not human.
    pub approach: Option<Greeting>,
    /// The regions action record's two lists (UI_FIDELITY.md §4.9): set by
    /// [`super::CampaignCommand::ProposeRegions`], applied by [`super::CampaignCommand::AcceptDeal`].
    pub regions: DealItems<RegionId>,
    /// The technology action record's two lists (technology keys): set by
    /// [`super::CampaignCommand::ProposeTechnologies`], applied by [`super::CampaignCommand::AcceptDeal`].
    pub technologies: DealItems<String>,
    /// The records have been applied since they last changed: a deal applies once (`AcceptDeal`
    /// again is a no-op until a Propose or Clear changes the deal).
    pub applied: bool,
}

/// The two lists of a deal's action record (`0x00C4B0A0` / `0x00C4B560` fill them): `demanded`
/// (record +0x14) is what the recipient gives the proposer, `offered` (record +0x24) what the
/// proposer gives the recipient. CONFIRMED (`0x00C18BF0` / `0x00C18CF0` apply them that way round).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DealItems<T> {
    /// The recipient gives these to the proposer.
    pub demanded: Vec<T>,
    /// The proposer gives these to the recipient.
    pub offered: Vec<T>,
}

impl<T> Default for DealItems<T> {
    fn default() -> Self {
        Self { demanded: Vec::new(), offered: Vec::new() }
    }
}

impl<T> DealItems<T> {
    /// No item on either side.
    pub fn is_empty(&self) -> bool {
        self.demanded.is_empty() && self.offered.is_empty()
    }

    fn clear(&mut self) {
        self.demanded.clear();
        self.offered.clear();
    }
}

/// The campaign's negotiation slot (campaign +0xF9C) and how many negotiations began and ended:
/// the one source of truth the UI reads. Changed only by the commands
/// [`super::CampaignCommand::BeginNegotiation`] / [`super::CampaignCommand::EndNegotiation`]
/// (`CCQ_DIPLOMACY_BEGIN_NEGOTIATION` / `CCQ_DIPLOMACY_END_NEGOTIATION`) and the deal's
/// `ProposeRegions` / `ProposeTechnologies` / `ClearNegotiation`, so a negotiation and the
/// RNG draw of its greeting go through the command queue like every other command. The counts
/// stand in for the exe's "started" (hub +0x318) and "ended" (hub +0x378) events: a listener that
/// has seen fewer begins or ends than these has events due. Not saved, not hashed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Negotiations {
    /// The open negotiation, if any.
    pub current: Option<Negotiation>,
    /// How many negotiations began.
    pub begun: u64,
    /// How many negotiations ended (an end with none open does not count).
    pub ended: u64,
}

/// A faction's two force strength totals (`0x008B2150`'s two outputs, split by the force's virtual
/// +0x38: army or navy); the ranking's power is their sum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FactionPower {
    /// Over the faction's armies.
    pub land: i32,
    /// Over the faction's navies.
    pub naval: i32,
}

impl FactionPower {
    /// Land + naval, as `BuildFactionRankingTable` `0x00949630` adds them (integer, wrapping as the
    /// exe's).
    pub fn total(self) -> i32 {
        self.land.wrapping_add(self.naval)
    }
}

/// The faction rankings ([`CampaignModel::faction_rankings`]) and the unit keys the power found no
/// unit record for (counted as 0; the caller logs them).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FactionRankings {
    /// Each ranked faction's categories.
    pub ranks: BTreeMap<FactionId, Rankings>,
    /// Unit keys in some force with no `units` record.
    pub unknown_units: BTreeSet<String>,
}

/// The stance record index of a stance (`DIPLOMATIC_STANCE_RECORD` +0x10, S1_LEFTOVERS.md §1 #4,
/// CONFIRMED): 0 war, 1 neutral, 2 allied, 3 patron, 4 protectorate.
fn stance_index(s: Stance) -> u8 {
    match s {
        Stance::War => 0,
        Stance::Neutral => 1,
        Stance::Allied => 2,
        Stance::Patron => 3,
        Stance::Protectorate => 4,
    }
}

/// Allied, patron or protectorate (`0x00B0CDF0`: stance index 2, 3 or 4).
fn allied_like(s: Stance) -> bool {
    (2..=4).contains(&stance_index(s))
}

impl CampaignModel {
    fn rel(&self, a: FactionId, b: FactionId) -> Option<&Relationship> {
        self.world.relationships.get(&(a, b))
    }

    /// `a`'s `force_diplomacy` permission value for `option` (a [`DIPLOMACY_OPTIONS`] index)
    /// towards `b` (`GetRelationshipDiplomacyOption` `0x00B27FE0`); 0 (allowed) without a
    /// relationship.
    pub fn diplomacy_option(&self, a: FactionId, b: FactionId, option: usize) -> u32 {
        self.rel(a, b).and_then(|r| r.diplomacy_options.get(option).copied()).unwrap_or(0)
    }

    /// May `a` propose `option` to `b` ([`option_allows_proposal`] of `a`'s value towards `b`)?
    pub fn may_propose(&self, a: FactionId, b: FactionId, option: usize) -> bool {
        option_allows_proposal(self.diplomacy_option(a, b, option))
    }

    /// May `a` accept `option` from `b` ([`option_allows_acceptance`] of `a`'s value towards `b`)?
    pub fn may_accept(&self, a: FactionId, b: FactionId, option: usize) -> bool {
        option_allows_acceptance(self.diplomacy_option(a, b, option))
    }

    /// `force_diplomacy(a, b, option, offer, accept)` (`SetDiplomacyOptionFromScript`
    /// `0x009792D0`): stores [`diplomacy_option_value`] on `a`'s relationship towards `b` only
    /// (CONFIRMED: `b`'s towards `a` is untouched). Returns false, storing nothing, when `option` is
    /// not a [`DIPLOMACY_OPTIONS`] index or `a` has no relationship towards `b` (the same faction, a
    /// faction not in the campaign, or a map without that pair); the caller logs the refusal. The
    /// exe never creates a relationship here: it only looks one up
    /// (`FindDiplomacyRelationshipByTarget` `0x00B64C50`).
    ///
    /// ORIGINAL BUG: on a miss (`a == b`, or any `b` without a relationship) `0x00B64C50` falls
    /// back to `a`'s first relationship, so the value lands on an unrelated faction pair; ours
    /// stores nothing.
    pub fn set_diplomacy_option(&mut self, a: FactionId, b: FactionId, option: usize, offer: bool, accept: bool) -> bool {
        if a == b || option >= DIPLOMACY_OPTIONS.len() {
            return false;
        }
        let Some(r) = self.world.relationships.get_mut(&(a, b)) else { return false };
        r.diplomacy_options[option] = diplomacy_option_value(offer, accept);
        true
    }

    /// The rebel faction (no faction record, `0x008CEEF0`): the model's faction with no key, as
    /// [`Self::at_war`] (`IsFactionAtWarWith` `0x008CE9B0`) tells it.
    pub fn is_rebel_faction(&self, f: FactionId) -> bool {
        self.world.factions.get(&f).is_some_and(|x| x.key.is_empty())
    }

    /// `0x008CE390`: neither is the rebel faction, and the same faction or an allied-like stance.
    fn allied_with(&self, a: FactionId, b: FactionId) -> bool {
        !self.is_rebel_faction(a) && !self.is_rebel_faction(b) && (a == b || allied_like(self.world.stance(a, b)))
    }

    pub(crate) fn is_human(&self, f: FactionId) -> bool {
        self.turn.humans.contains(&f)
    }

    /// The other factions of the campaign (the relationship lists the exe walks).
    fn third_factions(&self, a: FactionId) -> impl Iterator<Item = FactionId> + '_ {
        self.world.factions.keys().copied().filter(move |&x| x != a)
    }

    /// `0x00C1E590(a, b)`: some faction `a` is at war with (stance war on `a`'s record) is not at war
    /// with `b` — a war `b` could join.
    fn joinable_war(&self, a: FactionId, b: FactionId) -> bool {
        self.third_factions(a).any(|x| self.world.stance(a, x) == Stance::War && !self.at_war(b, x))
    }

    /// `0x00C3E410(a, b)`: `a` has a trade agreement with a faction other than `b`.
    fn trades_with_third(&self, a: FactionId, b: FactionId) -> bool {
        self.third_factions(a).any(|x| x != b && self.rel(a, x).is_some_and(|r| r.trade_agreement))
    }

    /// `0x00C3D5A0(a, b)`: `a` is allied (stance 2) with a faction other than `b`.
    fn allied_with_third(&self, a: FactionId, b: FactionId) -> bool {
        self.third_factions(a).any(|x| x != b && self.world.stance(a, x) == Stance::Allied)
    }

    /// `0x00C16990`: a faction at war with one of the two while allied-like with the other blocks an
    /// alliance. INFERRED direction of the two loops (the decompiler hides which list each walks;
    /// both directions are tested).
    fn alliance_blocked(&self, p: FactionId, r: FactionId) -> bool {
        let blocks = |a: FactionId, b: FactionId| {
            self.third_factions(a).any(|x| x != b && self.world.stance(a, x) == Stance::War && allied_like(self.world.stance(x, b)))
        };
        blocks(p, r) || blocks(r, p)
    }

    /// `0x00B4D9A0`: the faction has a relationship with the protectorate stance (index 4).
    fn has_protectorate(&self, f: FactionId) -> bool {
        self.third_factions(f).any(|x| self.world.stance(f, x) == Stance::Protectorate)
    }

    /// The 14 action records of a negotiation of `proposer` with `recipient`, in the exe's order
    /// (the constructors `0x00BF3E90` .. `0x00BF3520` called by `0x00BF5A60`). Every record is then
    /// forbidden when the proposer's relationship to the recipient has its option at 2 or 3
    /// (`0x00BF5CB3` loop).
    pub fn negotiation_actions(&self, proposer: FactionId, recipient: FactionId) -> Vec<ActionRecord> {
        use NegotiationAction as A;
        let (p, r) = (proposer, recipient);
        let war = self.at_war(p, r);
        let pr = self.rel(p, r);
        let rp = self.rel(r, p);
        let trade_held = pr.is_some_and(|x| x.trade_agreement);
        let embargo = |x: Option<&Relationship>| x.is_some_and(|x| x.trade_embargo_turns != 0);
        // Trade (`0x00C1A7D0`): the held agreement can be cancelled; a new one needs peace, a trade
        // route both ways (`0x00BA42E0`) and no embargo either way. PROVISIONAL: the route test is
        // not decoded; the model has no route builder (see `economy::trade_partners`), so a route
        // is assumed to exist.
        let trade_possible = trade_held || (!war && !embargo(pr) && !embargo(rp));
        // Access (`0x00C1A6E0`, side 1 / side 0): peace and the access not already indefinite.
        let access_free = |x: Option<&Relationship>| x.is_none_or(|x| x.military_access_turns != -1);
        let access_possible = !war && (access_free(pr) || access_free(rp));
        // Access cancel (`0x00C1A5D0`): the proposer gives access, not a protectorate, no joined war.
        let access_cancel = pr.is_some_and(|x| x.military_access_turns != 0 && x.allied_in_war_against.is_empty())
            && self.world.stance(p, r) != Stance::Protectorate;
        // Alliance (`0x00C1A630`, side 1 / side 0).
        let allied = self.allied_with(p, r);
        let alliance_possible = self.world.stance(p, r) == Stance::Allied
            || (!allied_like(self.world.stance(p, r)) && !war && !self.alliance_blocked(p, r));
        // Protector (`0x00C1A770`): neither has a protectorate, peace, the recipient holds exactly
        // one region (faction +0x76C list count == 1; CONFIRMED the region list: `0x008AE270` adds a
        // region to it and makes it the capital +0x72C when there is none) and is not human.
        let regions_of_r = self.world.regions.values().filter(|x| x.owner == r).count();
        let protector = !self.has_protectorate(p) && !self.has_protectorate(r) && !war && regions_of_r == 1 && !self.is_human(r);
        // Peace (`0x00C1A730`): at war and the peace option allowed (0 or 1).
        let peace = war && self.diplomacy_option(p, r, A::Peace.option()) <= 1;
        // War (`0x00C1A890`): not at war. PROVISIONAL: the proposer's forced war target (+0x754) is
        // not modelled (no faction has one).
        let declare = !war;
        let join = allied && (self.joinable_war(r, p) || self.joinable_war(p, r));
        let break_trade = !war && (self.trades_with_third(r, p) || self.trades_with_third(p, r));
        let break_alliance = !war && (self.allied_with_third(p, r) || self.allied_with_third(r, p));
        let rec = |action, offers_and_demands, possible| ActionRecord { action, offers_and_demands, possible, forbidden: false };
        let mut out = vec![
            rec(if trade_held { A::TradeCancel } else { A::Trade }, false, trade_possible),
            rec(A::Access, true, access_possible),
            rec(A::AccessCancel, false, access_cancel),
            rec(if allied { A::AllianceCancel } else { A::Alliance }, false, alliance_possible),
            rec(A::Regions, true, true),
            rec(A::Technology, true, true),
            rec(A::StateGift, false, !self.is_human(r)),
            rec(A::Payments, true, true),
            rec(A::Protector, false, protector),
            rec(A::Peace, true, peace),
            rec(A::War, false, declare),
            rec(A::RequestJoinWar, false, join),
            rec(A::BreakTrade, false, break_trade),
            rec(A::BreakAlliance, false, break_alliance),
        ];
        for a in &mut out {
            a.forbidden = !self.may_propose(p, r, a.action.option());
        }
        out
    }

    /// The regions `faction` can hand over in a deal (`0x00C5C040`, the list of `TradeableRegions`
    /// `0x009C5770`): every region of the faction's region list (faction `+0x778`) except its
    /// capital (`0x00A8B5A0`: faction `+0x72C`) and a settlement under siege (settlement virtual
    /// `+0x98`, the slot the script function `IsUnderSiege` `0x008A1FF0` reads). CONFIRMED rule.
    /// PROVISIONAL: sieges are not in the model, so no region is left out for one; the order is
    /// the model's region order, not the faction list's (acquisition) order.
    pub fn tradeable_regions(&self, faction: FactionId) -> impl Iterator<Item = super::ids::RegionId> + '_ {
        let capital = self.world.capital(faction);
        self.world.regions.values().filter(move |r| r.owner == faction && Some(r.id) != capital).map(|r| r.id)
    }

    /// The technologies `side` can hand `other` in a deal (`0x008F4F10`, from `0x00C5C170`, the
    /// lists of `TradeableTechnologies` `0x009C5AA0`): those `side` has researched (state 0) and
    /// `other` is researching, can research or can steal or trade for (state 1, 2 or 3,
    /// `0x008F3DB0` / `0x008F3AC0`; a technology `other` has no record of is state 5). CONFIRMED
    /// rule. PROVISIONAL: in `side`'s technology list order (the exe walks the technology table).
    pub fn tradeable_technologies(&self, side: FactionId, other: FactionId) -> Vec<String> {
        let techs = |f: FactionId| self.world.faction_details.get(&f).map(|d| d.technologies.as_slice()).unwrap_or_default();
        let theirs = techs(other);
        techs(side)
            .iter()
            .filter(|(key, state)| *state == 0 && theirs.iter().any(|(k, s)| k == key && (1..=3).contains(s)))
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// The panel's two button lists for `local` (the local player, one side of the negotiation)
    /// negotiating `proposer` → `recipient` (`0x009B5220`): a type-1 action is listed only when
    /// available; a type-0 action when available, and — for trade, alliance, request_join_war,
    /// break_trade and break_alliance — also when not available while the local player's stance to
    /// the other side is not war, then greyed with a tooltip.
    pub fn possible_actions(&self, local: FactionId, proposer: FactionId, recipient: FactionId) -> PossibleActions {
        use NegotiationAction as A;
        let counterpart = if local == proposer { recipient } else { proposer };
        let not_at_war = self.world.stance(local, counterpart) != Stance::War;
        let mut out = PossibleActions::default();
        for a in self.negotiation_actions(proposer, recipient) {
            let active = a.available();
            if a.offers_and_demands {
                if active {
                    out.offers_and_demands.push(ListedAction { action: a.action, active, tooltip: None });
                }
                continue;
            }
            let explained = matches!(a.action, A::Trade | A::Alliance | A::RequestJoinWar | A::BreakTrade | A::BreakAlliance);
            if explained && not_at_war {
                let tooltip = self.action_tooltip(a.action, local, counterpart);
                out.unilaterals.push(ListedAction { action: a.action, active, tooltip });
            } else if active {
                out.unilaterals.push(ListedAction { action: a.action, active, tooltip: None });
            }
        }
        out
    }

    /// The tooltip of an explained action. request_join_war (`0x00B4ECF0`, CONFIRMED keys and
    /// order): `join_war_tooltip_not_allied` when not allied-like, else `..._can_join_war` when a
    /// war is joinable either way (`0x00B28010`; INFERRED to be the same test as `0x00C1E590`),
    /// else `..._no_joinable_wars`. PROVISIONAL: the trade / alliance / break_trade /
    /// break_alliance builders (`0x00BC7910`, `0x00B0CBD0`, `0x00B13CB0`, `0x00B13B80`) are not
    /// decoded, so those buttons keep the template's own tooltip.
    fn action_tooltip(&self, action: NegotiationAction, local: FactionId, other: FactionId) -> Option<&'static str> {
        if action != NegotiationAction::RequestJoinWar {
            return None;
        }
        Some(if !allied_like(self.world.stance(local, other)) {
            "join_war_tooltip_not_allied"
        } else if self.joinable_war(local, other) || self.joinable_war(other, local) {
            "join_war_tooltip_can_join_war"
        } else {
            "join_war_tooltip_no_joinable_wars"
        })
    }

    /// A diplomat's line (`ResolveDiplomacyNegotiationString` `0x00C55CC0`, CONFIRMED): the
    /// `diplomacy_negotiation_strings` row for (`<event>_<attitude>`, the speaker's culture, its
    /// government key), the attitude the speaker's towards `listener` (`0x00B0DBA0`, names
    /// `0x015F2910`); when the speaker has a faction override row for that, one campaign-RNG draw
    /// below 0.5 picks the override. The draw happens only when an override row exists. No row:
    /// [`Greeting::Missing`] ("Missing String").
    fn diplomat_line(&mut self, event: &str, speaker: FactionId, listener: FactionId) -> Greeting {
        let attitude = super::treaties::attitude_name(self.attitude_category(speaker, listener));
        let Some(f) = self.world.factions.get(&speaker) else {
            return Greeting::Missing { faction: String::new(), key: (format!("{event}_{attitude}"), String::new(), String::new()) };
        };
        let culture = self.rules.faction_cultures.get(&f.key).cloned().unwrap_or_default();
        let key = (format!("{event}_{attitude}"), culture, f.government_key.clone());
        let over = self.rules.negotiation_overrides.get(&(key.0.clone(), key.1.clone(), key.2.clone(), f.key.clone())).cloned();
        if let Some(o) = over
            && self.rng.unit_float() < 0.5
        {
            return Greeting::Line(o);
        }
        match self.rules.negotiation_strings.get(&key) {
            Some(line) => Greeting::Line(line.clone()),
            None => Greeting::Missing { faction: f.key.clone(), key },
        }
    }

    /// `CCQ_DIPLOMACY_BEGIN_NEGOTIATION`: a new campaign negotiation of `proposer` with `recipient`,
    /// built over whatever was there (`0x008AF620` overwrites campaign +0xF9C). Its constructor
    /// (`InitializeCampaignNegotiationGreetings` `0x00BF5A60`, CONFIRMED order and speakers) picks
    /// the lines: when the proposer is human (+0x6E0), the recipient's `receive` line towards the
    /// proposer (carried by the "started" event, hub +0x318); then, when the recipient is human,
    /// the proposer's `approach` line towards the recipient (hub +0x330). Each pick may draw the
    /// campaign RNG ([`Self::diplomat_line`]) — here and only here: reading
    /// [`CampaignModel::negotiations`] draws nothing. Reached only through the command
    /// ([`super::CampaignCommand::BeginNegotiation`]), so the draw is ordered with every other
    /// command (multiplayer, replays). Both factions exist (the command checks).
    pub(crate) fn begin_negotiation(&mut self, proposer: FactionId, recipient: FactionId) {
        let greeting = self.is_human(proposer).then(|| self.diplomat_line("receive", recipient, proposer));
        let approach = self.is_human(recipient).then(|| self.diplomat_line("approach", proposer, recipient));
        let n = &mut self.negotiations;
        n.begun += 1;
        n.current = Some(Negotiation {
            serial: n.begun,
            proposer,
            recipient,
            greeting,
            approach,
            regions: DealItems::default(),
            technologies: DealItems::default(),
            applied: false,
        });
    }

    /// `CCQ_DIPLOMACY_PROPOSE_REGIONS` (executor `0x00933CC0`): `clear` empties the regions record
    /// (`0x00C5C730`); otherwise, when either list has a region, the record's two lists are
    /// replaced by these (`0x00C49AA0` → `0x00C4B0A0`); two empty lists leave it as it is. A region
    /// that does not exist aborts the command (the exe drops it when an id does not resolve).
    pub(crate) fn propose_regions(&mut self, clear: bool, demanded: Vec<RegionId>, offered: Vec<RegionId>) -> Result<(), super::CommandError> {
        if let Some(r) = demanded.iter().chain(&offered).find(|r| !self.world.regions.contains_key(r)) {
            return Err(super::CommandError::UnknownRegion(*r));
        }
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        if clear {
            n.regions.clear();
            n.applied = false;
        } else if !demanded.is_empty() || !offered.is_empty() {
            n.regions = DealItems { demanded, offered };
            n.applied = false;
        }
        Ok(())
    }

    /// `CCQ_DIPLOMACY_PROPOSE_TECHNOLOGIES` (executor `0x009340A0`): as [`Self::propose_regions`]
    /// with technology keys (a key the `technologies` table does not have aborts), except that two
    /// empty lists clear the record too.
    pub(crate) fn propose_technologies(&mut self, clear: bool, demanded: Vec<String>, offered: Vec<String>) -> Result<(), super::CommandError> {
        if let Some(t) = demanded.iter().chain(&offered).find(|t| !self.rules.technologies.contains_key(*t)) {
            return Err(super::CommandError::UnknownTechnology(t.clone()));
        }
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        if clear || (demanded.is_empty() && offered.is_empty()) {
            n.technologies.clear();
        } else {
            n.technologies = DealItems { demanded, offered };
        }
        n.applied = false;
        Ok(())
    }

    /// `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (executor `0x00932EC0`): the deal is emptied. The model
    /// holds the regions and technology records; the other items are still held by the UI.
    pub(crate) fn clear_negotiation(&mut self) -> Result<(), super::CommandError> {
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        n.regions.clear();
        n.technologies.clear();
        n.applied = false;
        Ok(())
    }

    /// The AI's answer to the open negotiation's deal (`CCQ_DIPLOMACY_PROPOSE_DEAL` → `0x00C49BE0`
    /// → `0x00AA5ED0`, run when the recipient is not human; AI_RESEARCH.md §4 "Deal evaluation"):
    /// true when it is refused, as the exe evaluates the regions and technology records
    /// ([`Self::ai_accepts_deal`]). A human recipient answers for itself: never refused here.
    pub fn ai_refuses_deal(&self) -> bool {
        self.ai_accepts_deal() == Some(false)
    }

    /// `CCQ_DIPLOMACY_ACCEPT_DEAL` (`AcceptCampaignNegotiationDeal` `0x00C114B0`) for the records
    /// the model holds, each through its virtual +0x3C, in the records' order (regions, then
    /// technologies). The deal stays (it goes with the negotiation's end), as in the exe; it is
    /// applied once: accepting it again before a Propose or Clear changes it does nothing
    /// ([`Negotiation::applied`]; the exe re-applies every record on each accept, which its UI
    /// never asks for, and which would raise the traded counts again).
    /// - regions (`0x00C18BF0`): each demanded region passes to the proposer, then each offered one
    ///   to the recipient ([`Self::transfer_region`]), each counted for its receiver
    ///   ([`Self::count_deal_region_received`]);
    /// - technologies (`0x00C18CF0`): each offered technology is granted to the recipient
    ///   ([`Self::grant_technology`]) and the proposer's traded count goes up by one
    ///   (`0x008F3DD0`); then each demanded one the other way round.
    ///
    /// Not ported: `0x00C1E240`, which can turn a deal with no demands into a payment before it is
    /// applied (the AI's side, AI_RESEARCH.md §6).
    /// A deal [`Self::ai_refuses_deal`] refuses is not applied (`CommandError::DealRefused`).
    pub(crate) fn accept_deal(&mut self) -> Result<Vec<super::CampaignEvent>, super::CommandError> {
        if self.negotiations.current.is_some() && self.ai_refuses_deal() {
            return Err(super::CommandError::DealRefused);
        }
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        if std::mem::replace(&mut n.applied, true) {
            return Ok(Vec::new());
        }
        let (proposer, recipient) = (n.proposer, n.recipient);
        let (regions, techs) = (n.regions.clone(), n.technologies.clone());
        for (list, to) in [(&regions.demanded, proposer), (&regions.offered, recipient)] {
            for &r in list {
                self.transfer_region(r, to);
                self.count_deal_region_received(to);
            }
        }
        for (techs, to, from) in [(&techs.offered, recipient, proposer), (&techs.demanded, proposer, recipient)] {
            for t in techs {
                self.grant_technology(to, t);
                self.count_technology_traded(from, t);
            }
        }
        Ok(Vec::new())
    }

    /// `0x008E2B70`, from the regions record's accept (`0x00C18C5F` / `0x00C18CD5`): a human
    /// faction that receives a region in a deal with no peace item counts it
    /// ([`super::World::deal_regions_received`], faction `+0x938`). CONFIRMED. PROVISIONAL: the
    /// model does not hold the peace record, so every deal counts as one without peace.
    fn count_deal_region_received(&mut self, faction: FactionId) {
        if self.is_human(faction) {
            let n = self.world.deal_regions_received.entry(faction).or_insert(0);
            *n = n.wrapping_add(1);
        }
    }

    /// `CCQ_DIPLOMACY_END_NEGOTIATION` (`0x008BC5D0`): the campaign negotiation goes, counted in
    /// [`Negotiations::ended`] ("ended" is posted once); with none open, nothing happens. Reached
    /// only through [`super::CampaignCommand::EndNegotiation`].
    pub(crate) fn end_negotiation(&mut self) {
        if self.negotiations.current.take().is_some() {
            self.negotiations.ended += 1;
        }
    }

    /// The land and naval force strength of every faction that has forces
    /// (`SumFactionForceUnitValuesLandNaval` `0x008B2150` with flag 0, called by
    /// `BuildFactionRankingTable` `0x00949630`; CONFIRMED): over the faction's forces (+0x7BC), into
    /// the land or naval total by the force kind, each force's units' per-unit value (`0x008F9C50`:
    /// virtual +0x3C of each unit) summed. With flag 0 that value is given an **empty** effect set
    /// (the round-end upkeep, flag 1, gives the force's commander / region and faction effects,
    /// `0x008AFC00`), so every upkeep modifier reads 0 and the land unit's `0x008F9B10` (vtable slot
    /// `0x013554D0`) gives round(upkeep × 100 × 0.01) = the unit's raw `upkeep`, whatever its
    /// strength: [`super::economy::unit_upkeep`] with no effects. Ships give their raw upkeep too
    /// and every force counts: CONFIRMED by the original's values read in the debugger (Coalition
    /// start `mp_eur_napoleon`, 1805), which this gives exactly for every listed faction, land and
    /// naval (install test `faction_power_is_the_originals_at_the_coalition_start`). Integer sums as
    /// the exe's. A unit key with no `units` record counts 0 and is added to `unknown_units` (the
    /// caller logs it).
    pub fn faction_powers(&self, unknown_units: &mut BTreeSet<String>) -> BTreeMap<FactionId, FactionPower> {
        let none = super::effects::Effects::default();
        let mut out: BTreeMap<FactionId, FactionPower> = BTreeMap::new();
        for f in self.world.forces.values() {
            let mut sum = 0i32;
            for u in &f.units {
                match self.rules.units.get(&u.unit_key) {
                    Some(r) => sum = sum.wrapping_add(super::economy::unit_upkeep(&none, f.faction, &self.rules.features, &u.unit_key, r)),
                    None => {
                        if !unknown_units.contains(&u.unit_key) {
                            unknown_units.insert(u.unit_key.clone());
                        }
                    }
                }
            }
            let p = out.entry(f.faction).or_default();
            let total = if f.is_navy { &mut p.naval } else { &mut p.land };
            *total = total.wrapping_add(sum);
        }
        out
    }

    /// The power, wealth and prestige categories of every faction (`BuildFactionRankingTable`
    /// `0x00949630`, rebuilt by the exe on each `FactionDetails` call). Per value: the factions out of
    /// the game (faction +0x824; [`Self::in_the_game`]) get 5 and sort last; the others sort by value,
    /// largest first, and get [`rank_categories`]. The rebel faction is not ranked (INFERRED).
    ///
    /// Values: power = [`Self::faction_powers`]; wealth = the last turn's income, categories 5..11
    /// of the last economics record (`0x00BBCC40`, [`super::World::last_income`]); prestige
    /// (`0x008F4D30` over the faction's `PRESTIGE` record, which the model does not hold yet;
    /// UI_FIDELITY.md §4.7 row "(1) prestige value"): PROVISIONAL 0 for every faction.
    pub fn faction_rankings(&self) -> FactionRankings {
        let ids: Vec<FactionId> = self.world.factions.values().filter(|f| !f.key.is_empty()).map(|f| f.id).collect();
        let out_of_game: Vec<bool> = ids.iter().map(|&f| !self.in_the_game(f)).collect();
        let mut unknown_units = BTreeSet::new();
        let powers = self.faction_powers(&mut unknown_units);
        let power: Vec<f32> = ids.iter().map(|f| powers.get(f).map_or(0, |p| p.total()) as f32).collect();
        let wealth: Vec<f32> = ids.iter().map(|&f| self.world.last_income(f) as f32).collect();
        let prestige = vec![0.0f32; ids.len()];
        let (p, w, s) = (rank_categories(&power, &out_of_game), rank_categories(&wealth, &out_of_game), rank_categories(&prestige, &out_of_game));
        let ranks = ids.iter().enumerate().map(|(i, &f)| (f, Rankings { power: p[i], wealth: w[i], prestige: s[i] })).collect();
        FactionRankings { ranks, unknown_units }
    }
}

/// The rank categories of one value per faction (`AssignFactionRankCategories` `0x0096B950` after
/// the sort `0x008FBCC0`), in the input order. Factions flagged `out` get 5. The others, sorted by
/// value (largest first; equal values keep their input order, the exe's `std::sort` gives them no
/// defined order but the same category): with n ≤ 3 all get 0; else the first three 0 and entry
/// 3 + k gets min(k / step + 1, 5), step = (n − 1) / 5 or n − 3 when that is 0; then each entry
/// whose value equals the previous entry's takes the previous entry's category.
pub fn rank_categories(values: &[f32], out: &[bool]) -> Vec<u8> {
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| !out[i]).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    let n = order.len();
    let mut cat = vec![5u8; values.len()];
    let mut sorted = vec![0u8; n];
    if n > 3 {
        let step = match (n - 1) / 5 {
            0 => n - 3,
            s => s,
        };
        for k in 0..n - 3 {
            sorted[3 + k] = (k / step + 1).min(5) as u8;
        }
        for i in 1..n {
            if values[order[i]] == values[order[i - 1]] {
                sorted[i] = sorted[i - 1];
            }
        }
    }
    for (i, &idx) in order.iter().enumerate() {
        cat[idx] = sorted[i];
    }
    cat
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `0x0096B950`: three at the top, then steps of (n − 1) / 5, ties share, the out-of-game last.
    #[test]
    fn rank_categories_follow_the_exe() {
        // 4 ranked: step = 3/5 = 0 → n − 3 = 1; entry 3 gets 1.
        assert_eq!(rank_categories(&[10.0, 40.0, 30.0, 20.0], &[false; 4]), vec![1, 0, 0, 0]);
        // 11 ranked: step 2; entries 3.. get 1,1,2,2,3,3,4,4.
        let v: Vec<f32> = (0..11).rev().map(|x| x as f32).collect();
        assert_eq!(rank_categories(&v, &[false; 11]), vec![0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
        // A tie takes the previous entry's category; an out-of-game faction gets 5.
        assert_eq!(rank_categories(&[50.0, 40.0, 30.0, 30.0, 9.0], &[false, false, false, false, true]), vec![0, 0, 0, 0, 5]);
        // Three or fewer: all 0.
        assert_eq!(rank_categories(&[1.0, 2.0, 3.0], &[false; 3]), vec![0, 0, 0]);
    }
}

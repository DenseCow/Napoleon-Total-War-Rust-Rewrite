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

/// The diplomacy screen's trade status of the player towards another faction
/// ([`CampaignModel::diplomacy_trade_status`]: the `Trading` / `LandTrade` / `TradingTooltip`
/// entries of `RetrieveFactionListForDiplomacy` `0x009F2EA0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiplomacyTradeStatus {
    /// The player holds a trade agreement with the faction (relationship `+0x788`): `Trading` is
    /// "trading" (or "cannot_trade" when `can_trade` is false) and the tooltip
    /// `trade_status_tooltip_already_trading`.
    pub held: bool,
    /// Held: no embargo either way. Not held: a new agreement is possible and the scripts allow it.
    /// `Trading` is "can_trade" / "cannot_trade" and the tooltip `trade_status_tooltip_can` /
    /// `_cannot` when not held.
    pub can_trade: bool,
    /// `LandTrade`.
    pub land: bool,
}

/// One button the negotiation panel shows (`PushNegotiationPossibleActionsFromScript` `0x009B5220`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedAction {
    /// The action.
    pub action: NegotiationAction,
    /// `Active`: the action is available (an unavailable one is shown greyed).
    pub active: bool,
    /// `Tooltip`: for trade and request_join_war while the local player is not at war with the other
    /// side (`action_tooltip`); `None` for every other action. The exe
    /// also explains alliance, break_trade and break_alliance (`0x00B0CBD0`, `0x00B13CB0`,
    /// `0x00B13B80`), still undecoded: see the PROVISIONAL note on `action_tooltip`.
    pub tooltip: Option<ActionTooltip>,
}

/// An explained action's tooltip ([`ListedAction::tooltip`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTooltip {
    /// A `random_localisation_strings` key.
    Key(&'static str),
    /// Trade (`0x00BC7910` with the counts, from `0x009B55FF`): `None` is the exe's empty text (a side
    /// without a capital).
    TradeAgreement(Option<TradeAgreementTooltip>),
}

/// The trade agreement explanation ([`CampaignModel::trade_agreement_tooltip`], `0x00BC7910`): the
/// text is the reason's string, then the route counts line when asked for, then
/// `trade_agreement_tooltip_existing_route_shifted` when [`Self::route_shifted`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeAgreementTooltip {
    /// The reason's `random_localisation_strings` key (one of the `TRADE_TIP_*` keys).
    pub reason: &'static str,
    /// `trade_agreement_tooltip_export_route_count` filled with (your routes left, your cap, their
    /// routes left, their cap), each `%d` in turn (`0x00446C80`); `None` when not asked for.
    pub sea_routes: Option<[i32; 4]>,
    /// Route check codes 4 and 6 (a sea route is freed by moving one over land).
    pub route_shifted: bool,
}

/// `trade_agreement_tooltip_embargoed` (random string 0x1CD, `0x00BC7910`).
pub const TRADE_TIP_EMBARGOED: &str = "trade_agreement_tooltip_embargoed";
/// `trade_agreement_tooltip_no_route_them` (0x1C9).
pub const TRADE_TIP_NO_ROUTE_THEM: &str = "trade_agreement_tooltip_no_route_them";
/// `trade_agreement_tooltip_no_route_you` (0x1C8).
pub const TRADE_TIP_NO_ROUTE_YOU: &str = "trade_agreement_tooltip_no_route_you";
/// `trade_agreement_tooltip_neighbour` (0x1CA).
pub const TRADE_TIP_NEIGHBOUR: &str = "trade_agreement_tooltip_neighbour";
/// `trade_agreement_tooltip_sea_routes` (0x1CB).
pub const TRADE_TIP_SEA_ROUTES: &str = "trade_agreement_tooltip_sea_routes";
/// `trade_agreement_tooltip_export_route_count` (0x1C7), the counts line.
pub const TRADE_TIP_ROUTE_COUNT: &str = "trade_agreement_tooltip_export_route_count";
/// `trade_agreement_tooltip_existing_route_shifted` (0x1CC).
pub const TRADE_TIP_ROUTE_SHIFTED: &str = "trade_agreement_tooltip_existing_route_shifted";

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
    /// The payment record (record 7): set by [`super::CampaignCommand::ProposePayment`], applied by
    /// [`super::CampaignCommand::AcceptDeal`].
    pub payment: DealPayment,
    /// The join-war record's two lists (record 11, vtable `0x0138202C`): `offered` the factions the
    /// proposer is to declare war on for the recipient, `demanded` those the recipient is to declare
    /// war on for the proposer. Set by [`super::CampaignCommand::ProposeJoinWar`], applied by
    /// [`super::CampaignCommand::AcceptDeal`].
    pub join_war: DealItems<FactionId>,
    /// The records have been applied since they last changed: a deal applies once (`AcceptDeal`
    /// again is a no-op until a Propose or Clear changes the deal).
    pub applied: bool,
    /// The deal was proposed as a threat (+0x2B8, stored by each `CCQ_DIPLOMACY_PROPOSE_DEAL`,
    /// `0x00C49BE0`; the panel's `ThreatOfForce` calls `ProposeDeal(true)`, `SendOffer`
    /// `ProposeDeal(false)`, diplomacy_panel.lua:1566 / :635): accepting or declining the deal then
    /// gives the recipient the `threatened` factor towards the proposer ([`CampaignModel::apply_threatened`]).
    pub threat: bool,
}

/// A deal's payment record (record 7, vtable `0x01381EDC`): `amount` (`+0x14`) for `turns` turns
/// (`+0x18`). A positive amount is a demand, paid by the recipient to the proposer; a negative one an
/// offer, the proposer paying its negation (the applier `0x00C18A70`; the panel's offer setters
/// `0x00C43D70` / `0x00C43D90` negate, the demand setters `0x00C1F6D0` / `0x00C1F6F0` do not). 0 is no
/// payment (the record's has-items `0x00C12640` is amount ≠ 0). Turns 1 is paid once, anything else
/// each turn (`0x00C18A70`). CONFIRMED (disassembly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DealPayment {
    /// Positive: demanded from the recipient; negative: offered by the proposer.
    pub amount: i32,
    /// 1: once; otherwise each turn for that many turns.
    pub turns: u32,
}

impl DealPayment {
    /// The record has an item (`0x00C12640`: amount ≠ 0).
    pub fn has_items(&self) -> bool {
        self.amount != 0
    }
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
/// `ProposeRegions` / `ProposeTechnologies` / `ProposePayment` / `ProposeJoinWar` /
/// `ClearNegotiation`, so a negotiation and the
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
        self.world.is_rebel_faction(f)
    }

    /// `0x008CE390`: neither is the rebel faction, and the same faction or an allied-like stance.
    pub(crate) fn allied_with(&self, a: FactionId, b: FactionId) -> bool {
        !self.is_rebel_faction(a) && !self.is_rebel_faction(b) && (a == b || allied_like(self.world.stance(a, b)))
    }

    pub(crate) fn is_human(&self, f: FactionId) -> bool {
        self.turn.humans.contains(&f)
    }

    /// A major power (faction +0x524, `0x00C3FB20`).
    pub(crate) fn is_major(&self, f: FactionId) -> bool {
        self.world.faction_details.get(&f).and_then(|d| d.major) == Some(true)
    }

    /// The other factions of the campaign (the relationship lists the exe walks).
    fn third_factions(&self, a: FactionId) -> impl Iterator<Item = FactionId> + '_ {
        self.world.factions.keys().copied().filter(move |&x| x != a)
    }

    /// `0x00C1E590(joiner = b, a)`: a war of `a`'s that `b` could join. Over `a`'s relationships,
    /// some `x` with `a`'s stance war, `b` not at war with `x` (`IsFactionAtWarWith` `0x008CE9B0`),
    /// and `b` free to fight `x`: `b` has no patron (`0x008CEEA0`), or its patron (`0x008E2A80`,
    /// [`Self::patron`]) is `x` itself (`0x00898B20`, a plain equality) or at war with `x`.
    /// CONFIRMED (disassembly).
    pub(crate) fn joinable_war(&self, a: FactionId, b: FactionId) -> bool {
        let patron = self.patron(b);
        self.third_factions(a).any(|x| {
            self.world.stance(a, x) == Stance::War && !self.at_war(b, x) && patron.is_none_or(|p| p == x || self.at_war(p, x))
        })
    }

    /// `0x00C3E410(a, b)`: `a` has a trade agreement with a faction other than `b`.
    fn trades_with_third(&self, a: FactionId, b: FactionId) -> bool {
        self.third_factions(a).any(|x| x != b && self.world.trade_held(a, x))
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

    /// `0x00B4D9A0`: the faction has a relationship with the protectorate stance (index 4), so it has a
    /// patron. The same answer as `FactionHasPatron` `0x008CEEA0` (faction +0x754 set): `SetFactionPatron`
    /// `0x008AF590` sets +0x754 to the patron right after `0x00B104E0` gives the record stance 4
    /// (`0x00B1058B`) and to 0 right after `0x00B64B10` takes it away (`0x00B64C2D`); the faction's
    /// destruction clears both (`0x008AB825`); the loader `0x0087A190` reads it back. CONFIRMED.
    pub fn has_patron(&self, f: FactionId) -> bool {
        self.patron(f).is_some()
    }

    /// The faction's patron (`0x008E2A80`, faction +0x754, kept by `SetFactionPatron` as
    /// [`Self::has_patron`] says): the faction its stance is protectorate towards.
    pub fn patron(&self, f: FactionId) -> Option<FactionId> {
        self.third_factions(f).find(|&x| self.world.stance(f, x) == Stance::Protectorate)
    }

    /// `0x00C1A7D0(p, r, cancel)`, CONFIRMED (decompile): with `cancel`, whether `p` holds a trade
    /// agreement with `r` (relationship `+0x788`, the held agreement can be cancelled); else whether a
    /// new one is possible: not at war (`IsFactionAtWarWith` `0x008CE9B0`), none held, a trade route
    /// both ways ([`Self::trade_route_check`] `p` → `r` and `r` → `p` non-zero, `0x00BA42E0`) and no
    /// embargo either way (`0x00B28E10`: relationship `+0x840` non-zero).
    pub fn trade_possible(&self, p: FactionId, r: FactionId, cancel: bool) -> bool {
        if cancel {
            return self.world.trade_held(p, r);
        }
        self.new_trade_route(p, r).is_some()
    }

    /// [`Self::trade_possible`] without `cancel`, answering `p`'s route code to `r`
    /// ([`Self::trade_route_check`], non-zero) when a new agreement is possible, else `None`. The exe
    /// searches the routes before it reads the embargoes (`0x00C1A7D0`); all the tests are reads, so
    /// testing the cheap embargo fields first gives the same answer without the two path searches.
    fn new_trade_route(&self, p: FactionId, r: FactionId) -> Option<u8> {
        if self.at_war(p, r)
            || self.world.trade_held(p, r)
            || self.trade_embargoed(p, r)
            || self.trade_embargoed(r, p)
        {
            return None;
        }
        let code = self.trade_route_check(p, r);
        (code != 0 && self.trade_route_check(r, p) != 0).then_some(code)
    }

    /// `0x00B28E10` on `a`'s relationship to `b`: an embargo is running (`+0x840` non-zero, CONFIRMED
    /// by disassembly; [`Relationship::trade_embargo_turns`]).
    fn trade_embargoed(&self, a: FactionId, b: FactionId) -> bool {
        self.rel(a, b).is_some_and(|x| x.trade_embargo_turns != 0)
    }

    /// The diplomacy screen's trade status of `player` towards `other` (`RetrieveFactionListForDiplomacy`
    /// `0x009F2EA0`, block `0x009F30AA`..`0x009F3507`), CONFIRMED (disassembly):
    /// - `None` at war (the player's relationship stance `+0x790` → `+0x10` is 0): no trade entries;
    /// - held agreement (`+0x788`, `0x009F30E6`): `can_trade` when neither side embargoes the other
    ///   (`0x00B28E10` on player → `other`, then `other` → player); `land` when the first of
    ///   `other`'s routes to the player has no sea hop (`0x00B28620`: no waypoint byte `+0x14`), false
    ///   without such a route. "First": the loop `0x009F3143`..`0x009F31B5` walks `other`'s route
    ///   vector in the trade manager's exporter map (`+0x11C`; count `+0x14`, data `+0x18`, 0x84 bytes)
    ///   from index 0 and stops at the first route whose importer (`0x00B27680`: `+0x54`) is the
    ///   player. That vector keeps file and build order with an order-keeping erase
    ///   (CAMPAIGN_FIDELITY.md "Agreement breaks remove one route each way"), the order of the pair's
    ///   `Vec` in [`super::World::trade_paths`], so its first entry is the same route;
    /// - else (`0x009F33E1`): `can_trade` when [`Self::trade_possible`] (`0x00C1A7D0(player, other, 0)`)
    ///   and the scripts allow it: the player may propose trade to `other` ([`Self::may_propose`]: its
    ///   option 0 towards `other` is not 2 or 3) and `other`'s option 0 towards the player is not 3;
    ///   `land` when also the player's route check to `other` (`0x00BA42E0`, called again by the exe,
    ///   the same code `0x00C1A7D0` got) is a land route ([`Self::trade_route_check`] code 1).
    ///
    /// `other`'s value 1 alone (it declines trade from the player) still shows "can_trade": the exe
    /// tests 3 only there (`CMP EAX,0x3` at `0x009F345C`..`0x009F3467`), as ours does.
    pub fn diplomacy_trade_status(&self, player: FactionId, other: FactionId) -> Option<DiplomacyTradeStatus> {
        if self.world.stance(player, other) == Stance::War {
            return None;
        }
        if self.world.trade_held(player, other) {
            let land = self.world.trade_paths.get(&(other, player)).and_then(|ps| ps.first()).is_some_and(|p| !p.uses_sea());
            return Some(DiplomacyTradeStatus {
                held: true,
                can_trade: !self.trade_embargoed(player, other) && !self.trade_embargoed(other, player),
                land,
            });
        }
        let route = if self.diplomacy_trade_allowed(player, other) { self.new_trade_route(player, other) } else { None };
        Some(DiplomacyTradeStatus { held: false, can_trade: route.is_some(), land: route == Some(1) })
    }

    /// The scripts allow the diplomacy screen's new trade agreement of `player` with `other`
    /// (`0x009F33E1`..`0x009F3469`, the flag at `[ESP+0x1FE]`): the player's option 0 towards `other`
    /// is not 2 or 3 and `other`'s towards the player is not 3 (it may neither propose nor accept
    /// trade with the player: `force_diplomacy` false, false).
    fn diplomacy_trade_allowed(&self, player: FactionId, other: FactionId) -> bool {
        let trade = NegotiationAction::Trade.option();
        self.may_propose(player, other, trade) && self.diplomacy_option(other, player, trade) != 3
    }

    /// The explanation the diplomacy screen appends to the `TradingTooltip` of a faction it shows
    /// ([`Self::diplomacy_trade_status`]) after the `trade_status_tooltip_can` / `_cannot` text
    /// (`0x004F1200` at `0x009F362A`), CONFIRMED (disassembly `0x009F35CC`..`0x009F361F`): with no
    /// agreement held and the scripts allowing one ([`Self::diplomacy_trade_allowed`]),
    /// [`Self::trade_agreement_tooltip`] (`0x00BC7910(player, other, 0)`, no counts); else nothing
    /// (the empty string `0x0131005F`). `None` also at war (no trade entries) or while held.
    pub fn diplomacy_trade_tooltip(&self, player: FactionId, other: FactionId) -> Option<TradeAgreementTooltip> {
        if self.world.stance(player, other) == Stance::War || self.world.trade_held(player, other) || !self.diplomacy_trade_allowed(player, other) {
            return None;
        }
        self.trade_agreement_tooltip(player, other, false)
    }

    /// Why `a` can or cannot make a trade agreement with `b` (`0x00BC7910(a, b, counts)` on the trade
    /// manager), CONFIRMED (disassembly):
    /// - either side without a capital (faction `+0x72C`, `0x008C9200`): `None`, the empty text
    ///   (`0x00BC7D3D`);
    /// - with `counts`, the line [`TRADE_TIP_ROUTE_COUNT`] filled from [`Self::sea_route_use`]: each side's sea route cap
    ///   ([`Self::sea_route_cap`] + [`Self::capital_port_route_cap`], `0x008DC150` + `0x008DC360`) less
    ///   its routes with a sea hop (its export routes in the trade manager's map `+0x11C`,
    ///   `0x00BB0980`, that are not land-only, `0x00B28620`: [`super::trade::TradePath::uses_sea`]),
    ///   then the cap: (a's left, a's cap, b's left, b's cap);
    /// - the reason (`0x00BC7BB1`): an embargo either way (`0x00B28E10`) → [`TRADE_TIP_EMBARGOED`];
    ///   else `b` cannot send a route to `a` ([`Self::trade_route_check`] `b` → `a` is 0) →
    ///   [`TRADE_TIP_NO_ROUTE_THEM`]; else by `a`'s code to `b`: 0 → [`TRADE_TIP_NO_ROUTE_YOU`],
    ///   1 → [`TRADE_TIP_NEIGHBOUR`], 2 / 3 / 5 / 7 → [`TRADE_TIP_SEA_ROUTES`], 4 / 6 → that with
    ///   [`TradeAgreementTooltip::route_shifted`], any other (a mod's check) → [`TRADE_TIP_NO_ROUTE_THEM`].
    pub fn trade_agreement_tooltip(&self, a: FactionId, b: FactionId, counts: bool) -> Option<TradeAgreementTooltip> {
        let w = &self.world;
        if w.capital(a).is_none() || w.capital(b).is_none() {
            return None;
        }
        let sea_routes = counts.then(|| {
            let side = |x: FactionId| {
                let (used, cap) = self.sea_route_use(x);
                (cap - used, cap)
            };
            let ((al, ac), (bl, bc)) = (side(a), side(b));
            [al, ac, bl, bc]
        });
        let (reason, route_shifted) = if self.trade_embargoed(a, b) || self.trade_embargoed(b, a) {
            (TRADE_TIP_EMBARGOED, false)
        } else if self.trade_route_check(b, a) == 0 {
            (TRADE_TIP_NO_ROUTE_THEM, false)
        } else {
            match self.trade_route_check(a, b) {
                0 => (TRADE_TIP_NO_ROUTE_YOU, false),
                1 => (TRADE_TIP_NEIGHBOUR, false),
                2 | 3 | 5 | 7 => (TRADE_TIP_SEA_ROUTES, false),
                4 | 6 => (TRADE_TIP_SEA_ROUTES, true),
                _ => (TRADE_TIP_NO_ROUTE_THEM, false),
            }
        };
        Some(TradeAgreementTooltip { reason, sea_routes, route_shifted })
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
        let trade_held = self.world.trade_held(p, r);
        let trade_possible = self.trade_possible(p, r, trade_held);
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
        let protector = !self.has_patron(p) && !self.has_patron(r) && !war && regions_of_r == 1 && !self.is_human(r);
        // Peace (`0x00C1A730`): at war and the peace option allowed (0 or 1).
        let peace = war && self.diplomacy_option(p, r, A::Peace.option()) <= 1;
        // War (`0x00C1A890`): not at war. PROVISIONAL: its patron test is not modelled (BACKLOG): a
        // proposer with a patron (+0x754, `Self::has_patron`) may declare only on the patron
        // itself or on a faction the patron is at war with.
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
    /// In the list's order ([`super::world::World::faction_regions_in_list_order`];
    /// CONFIRMED, CAMPAIGN_FIDELITY.md "Faction region list order"). PROVISIONAL: sieges are not in
    /// the model, so no region is left out for one. Allocates: cold paths only.
    pub fn tradeable_regions(&self, faction: FactionId) -> impl Iterator<Item = super::ids::RegionId> + '_ {
        let capital = self.world.capital(faction);
        self.world.faction_regions_in_list_order(faction).into_iter().filter(move |id| Some(*id) != capital)
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
    /// war is joinable either way ([`Self::joinable_war`]: `0x00B28010`'s body is `0x00C1E590`'s,
    /// CONFIRMED by disassembly),
    /// else `..._no_joinable_wars`. trade (`0x009B55FF`, CONFIRMED disassembly):
    /// [`Self::trade_agreement_tooltip`] of the local player to the other side with the counts
    /// (`0x00BC7910(local, other, 1)`). PROVISIONAL: the alliance / break_trade / break_alliance
    /// builders (`0x00B0CBD0`, `0x00B13CB0`, `0x00B13B80`) are not decoded, so those buttons keep the
    /// template's own tooltip.
    fn action_tooltip(&self, action: NegotiationAction, local: FactionId, other: FactionId) -> Option<ActionTooltip> {
        match action {
            NegotiationAction::Trade => Some(ActionTooltip::TradeAgreement(self.trade_agreement_tooltip(local, other, true))),
            NegotiationAction::RequestJoinWar => Some(ActionTooltip::Key(if !allied_like(self.world.stance(local, other)) {
                "join_war_tooltip_not_allied"
            } else if self.joinable_war(local, other) || self.joinable_war(other, local) {
                "join_war_tooltip_can_join_war"
            } else {
                "join_war_tooltip_no_joinable_wars"
            })),
            _ => None,
        }
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
            payment: DealPayment::default(),
            join_war: DealItems::default(),
            applied: false,
            threat: false,
        });
    }

    /// `CCQ_DIPLOMACY_PROPOSE_REGIONS` (executor `0x00933CC0`): `clear` empties the regions record
    /// (`0x00C5C730`); otherwise, when either list has a region, the record's two lists are
    /// replaced by these (`0x00C49AA0` → `0x00C4B0A0`); two empty lists leave it as it is. A region
    /// that does not exist aborts the command (the exe drops it when an id does not resolve).
    /// Who owns the regions is not checked, here or on accept, as in the exe (CONFIRMED: neither
    /// the panel's `Propose` `0x009BF3C0`, the executor, the record's setter `0x00C4B0A0` nor its
    /// accept `0x00C18BF0` reads an owner): the sides' lists come from the panel's
    /// [`Self::tradeable_regions`] rows and the AI's goal lists, which hold only each side's own
    /// regions (AI_RESEARCH.md §4 "Who checks the owners").
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

    /// `CCQ_DIPLOMACY_PROPOSE_PAYMENT` (executor `0x00933AD0`): `clear` empties the payment record
    /// (`0x00C5C730`); otherwise the record becomes `amount` for `turns` turns ([`DealPayment`]'s
    /// sign). The executor hands a positive amount to the demand setter (`0x00C1F6D0`) and any other
    /// to the offer setter with its negation (`0x00C43D70`, which negates it back), so the record is
    /// replaced (`0x00C11B80` → `0x00C49AA0`) by the amount as given, through the record's copy
    /// (`0x00C4AB20`), which caps an offered one-off payment at the proposer's treasury:
    /// [`CampaignModel::proposed_payment`], the rule seam. The capped amount is what the panel's
    /// row shows, what the AI evaluates and what the deal pays. CONFIRMED (disassembly).
    /// `i32::MIN` is refused: its negation does not exist (the exe would keep it and pay it as is).
    pub(crate) fn propose_payment(&mut self, clear: bool, amount: i32, turns: u32) -> Result<(), super::CommandError> {
        let proposer = self.negotiations.current.as_ref().ok_or(super::CommandError::NoNegotiation)?.proposer;
        let payment = if clear {
            DealPayment::default()
        } else if amount == i32::MIN {
            return Err(super::CommandError::Unsupported("payment amount out of range"));
        } else {
            DealPayment { amount: self.proposed_payment(proposer, amount, turns), turns }
        };
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        n.payment = payment;
        n.applied = false;
        Ok(())
    }

    /// `CCQ_DIPLOMACY_PROPOSE_JOIN_WAR` (posted by the join-war record's `0x00C4A960`, executor
    /// `ExecuteProposeJoinWarCommand` `0x009336C0`, CONFIRMED by disassembly): as
    /// [`Self::propose_regions`] with factions: `clear` empties the record (`0x00C11B80(11)` →
    /// `0x00C5C730`); otherwise, when either list has a faction, the record's two lists are replaced
    /// by these (the demand setter `0x00C1F690`, the offer setter `0x00C43D30`, then `0x00C49AA0` →
    /// the record's setter `0x00C4A670`); two empty lists leave it as it is. A faction that does not
    /// exist aborts the command (the executor drops it when an id does not resolve, `0x0090E2F0`).
    /// The executor checks nothing else (neither side's stance to the factions, nor that a faction
    /// is not the side that would declare war): an item naming its own joiner is refused on accept
    /// ([`Self::accept_deal`]).
    pub(crate) fn propose_join_war(&mut self, clear: bool, demanded: Vec<FactionId>, offered: Vec<FactionId>) -> Result<(), super::CommandError> {
        if let Some(f) = demanded.iter().chain(&offered).find(|f| !self.world.factions.contains_key(f)) {
            return Err(super::CommandError::UnknownFaction(*f));
        }
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        if clear {
            n.join_war.clear();
            n.applied = false;
        } else if !demanded.is_empty() || !offered.is_empty() {
            n.join_war = DealItems { demanded, offered };
            n.applied = false;
        }
        Ok(())
    }

    /// `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (executor `0x00932EC0`): the deal is emptied. The model
    /// holds the regions, technology, payment and join-war records; the other items are still held
    /// by the UI.
    pub(crate) fn clear_negotiation(&mut self) -> Result<(), super::CommandError> {
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        n.regions.clear();
        n.technologies.clear();
        n.payment = DealPayment::default();
        n.join_war.clear();
        n.applied = false;
        Ok(())
    }

    /// The AI's answer to the open negotiation's deal (`CCQ_DIPLOMACY_PROPOSE_DEAL` → `ProcessNegotiationDealProposal 0x00C49BE0`
    /// → `EvaluateProposedNegotiationDealAI 0x00AA5ED0`, run when the recipient is not human; AI_RESEARCH.md §4 "Deal evaluation"):
    /// true when it is refused, as the exe evaluates the regions, technology and payment records
    /// ([`Self::ai_accepts_deal`]). A human recipient answers for itself: never refused here.
    pub fn ai_refuses_deal(&self) -> bool {
        self.ai_accepts_deal() == Some(false)
    }

    /// `CCQ_DIPLOMACY_PROPOSE_DEAL` (executor `0x00933690` → `0x00C49BE0`): when the recipient is
    /// not human its evaluation (`0x00AA5ED0`) reads the recipient's relation belief
    /// (`0x00CCB810` / `0x00CCB210` → `GetCaiRelationBelief` `0x00A74A20`), which keeps it: a dirty
    /// one is rebuilt and stays clean until the next diplomacy change or turn change
    /// ([`Self::refresh_relation_belief`]). The proposal stores `threat` ([`Negotiation::threat`],
    /// +0x2B8). The answer is [`Self::ai_refuses_deal`]; the caller then accepts
    /// ([`Self::accept_deal`]) or declines ([`Self::decline_deal`]), as the evaluator calls
    /// `0x00C114B0` / `0x00C1F210`. Fails with no open negotiation.
    pub(crate) fn propose_deal(&mut self, threat: bool) -> Result<(), super::CommandError> {
        let n = self.negotiations.current.as_mut().ok_or(super::CommandError::NoNegotiation)?;
        n.threat = threat;
        let recipient = n.recipient;
        if !self.is_human(recipient) {
            self.refresh_relation_belief(recipient);
        }
        Ok(())
    }

    /// `CCQ_DIPLOMACY_ACCEPT_DEAL` (`AcceptCampaignNegotiationDeal` `0x00C114B0`) for the records
    /// the model holds, each through its virtual +0x3C, in the records' order (regions,
    /// technologies, the payment, then join war). The deal stays (it goes with the negotiation's end), as in the exe; it is
    /// applied once: accepting it again before a Propose or Clear changes it does nothing
    /// ([`Negotiation::applied`]; the exe re-applies every record on each accept, which its UI
    /// never asks for, and which would raise the traded counts again).
    /// - regions (`0x00C18BF0`): each demanded region passes to the proposer, then each offered one
    ///   to the recipient ([`Self::transfer_region`]), each counted for its receiver
    ///   ([`Self::count_deal_region_received`]);
    /// - technologies (`0x00C18CF0`): each offered technology is granted to the recipient
    ///   ([`Self::grant_technology`]) and the proposer's traded count goes up by one
    ///   (`0x008F3DD0`); then each demanded one the other way round;
    /// - payment (`0x00C18A70`): a positive amount is paid by the recipient to the proposer, a
    ///   negative one's negation by the proposer to the recipient; over 1 turn once
    ///   ([`Self::pay_once`]), else each turn ([`Self::pay_regularly`]); 0 pays nothing. The
    ///   amount was capped when it was proposed ([`Self::propose_payment`]): not capped again here.
    /// - join war (`ApplyJoinWarRecordOnAccept` `0x00C188F0`): the proposer declares war on each
    ///   offered faction on the recipient's behalf, then the recipient on each demanded one on the
    ///   proposer's, in list order ([`Self::join_war_by_deal`]: each war, its notice to the faction
    ///   war is declared on and its zones' refresh, before the next). No at-war or stance test, as in
    ///   the exe; an item naming its own joiner fails the deal (checked before any record is applied).
    ///   PROVISIONAL: the exe has no such test and would declare a faction's war on itself; what that
    ///   does, and whether anything can propose it, is not traced.
    ///
    /// Then the expulsions (`0x008D9DC0`, [`Self::expel_from_region`]): the proposer's forces leave
    /// each offered region, then the recipient's each demanded one. A deal with a peace record first
    /// expels each side from all of the other's regions (record `+0x184`); PROVISIONAL: the model's
    /// deal holds no peace record, so that pass is not run (BACKLOG).
    ///
    /// Then, for a threat ([`Negotiation::threat`]), the recipient gets the `threatened` factor
    /// towards the proposer ([`Self::apply_threatened`], `0x00C11671`).
    ///
    /// Not ported: `0x00C1E240`, which can turn a deal with no demands into a payment before it is
    /// applied (the AI's side, AI_RESEARCH.md §6).
    /// A deal [`Self::ai_refuses_deal`] refuses is not applied (`CommandError::DealRefused`); nor is
    /// one whose payment or join-war item names an invalid faction pair (checked before any record
    /// is applied).
    pub(crate) fn accept_deal(&mut self) -> Result<Vec<super::CampaignEvent>, super::CommandError> {
        if self.negotiations.current.is_some() && self.ai_refuses_deal() {
            return Err(super::CommandError::DealRefused);
        }
        let n = self.negotiations.current.as_ref().ok_or(super::CommandError::NoNegotiation)?;
        if n.applied {
            return Ok(Vec::new());
        }
        let (proposer, recipient, payment, threat) = (n.proposer, n.recipient, n.payment, n.threat);
        let (regions, techs, join_war) = (n.regions.clone(), n.technologies.clone(), n.join_war.clone());
        // The deal applies whole or not at all: the payment's and the join-war wars' only failure
        // (the faction pair) is checked before any record is applied, so an error leaves the deal
        // unapplied (the caller logs it). The payment has no money test (0x00BAF500): it cannot
        // fail on its amount.
        if payment.has_items() {
            self.check_diplomacy_pair(proposer, recipient)?;
        }
        // PROVISIONAL: an item naming its own joiner refuses the whole deal here (SameFaction). The
        // exe has no such test (`0x00C188F0` → `0x00B26700` takes any pair); what its war of a
        // faction on itself does to the records, and whether its panel or the AI can ever propose
        // one, is not traced.
        let join_wars = [(&join_war.offered, proposer, recipient), (&join_war.demanded, recipient, proposer)];
        for (list, joiner, _) in join_wars {
            for &f in list {
                self.check_diplomacy_pair(joiner, f)?;
            }
        }
        if let Some(n) = self.negotiations.current.as_mut() {
            n.applied = true;
        }
        let mut events = Vec::new();
        for (list, to) in [(&regions.demanded, proposer), (&regions.offered, recipient)] {
            for &r in list {
                let old = self.world.regions.get(&r).map(|x| x.owner);
                if self.transfer_region(r, to)
                    && let Some(old) = old
                {
                    // A deal sets the owner change's first flag (`0x00B58A10(faction, 1, 0)`): losing the
                    // capital with the last region ends the giver (`0x00A64AC0`, CONFIRMED).
                    self.end_if_capital_lost(old, &mut events);
                }
                self.count_deal_region_received(to);
            }
        }
        for (techs, to, from) in [(&techs.offered, recipient, proposer), (&techs.demanded, proposer, recipient)] {
            for t in techs {
                self.grant_technology(to, t);
                self.count_technology_traded(from, t);
            }
        }
        if payment.has_items() {
            // `propose_payment` refuses i32::MIN, so the negation exists. The pair was checked above.
            let (payer, payee, amount) = if payment.amount > 0 { (recipient, proposer, payment.amount) } else { (proposer, recipient, -payment.amount) };
            if payment.turns == 1 {
                self.pay_once(payer, payee, amount);
            } else {
                self.pay_regularly(payer, payee, amount, payment.turns);
            }
        }
        // Record 11, join war (`0x00C188F0`): the proposer declares war on each offered faction on
        // the recipient's behalf, then the recipient on each demanded one on the proposer's.
        for (list, joiner, ally) in join_wars {
            for &f in list {
                // The pairs were checked above: an item cannot fail here.
                events.extend(self.join_war_by_deal(joiner, f, ally));
            }
        }
        // After every record: the proposer's forces leave each offered region, then the recipient's
        // each demanded one (`0x00C114B0` → `0x008D9DC0`, CONFIRMED).
        for (list, f) in [(&regions.offered, proposer), (&regions.demanded, recipient)] {
            for &r in list {
                self.expel_from_region(f, r);
            }
        }
        // After the expulsions, `0x00C43520` and result 1, the threat (`0x00C11655`).
        if threat {
            self.apply_threatened(recipient, proposer);
        }
        Ok(events)
    }

    /// `CCQ_DIPLOMACY_DECLINE_DEAL` (`DeclineCampaignNegotiationDeal` `0x00C1F210`; also the AI
    /// evaluator's refusal, `0x00AA5ED0`): the result becomes "declined" (+0x28 = 2, held by the
    /// panel) and, for a threat ([`Negotiation::threat`]), the recipient gets the `threatened`
    /// factor towards the proposer ([`Self::apply_threatened`]). The reply strings are the panel's.
    /// Nothing else changes: the deal stays until the negotiation ends. A repeated decline adds the
    /// factor again (the command `0x00932F00` and `0x00C1F210` keep no record of an earlier answer;
    /// CONFIRMED). Fails with no open negotiation.
    pub(crate) fn decline_deal(&mut self) -> Result<(), super::CommandError> {
        let n = self.negotiations.current.as_ref().ok_or(super::CommandError::NoNegotiation)?;
        let (proposer, recipient, threat) = (n.proposer, n.recipient, n.threat);
        if threat {
            self.apply_threatened(recipient, proposer);
        }
        Ok(())
    }

    /// `ApplyThreatenedFactorTowardsFaction` `0x00B72F00` → `ApplyThreatenedAttitudeFactor`
    /// `0x00B72EC0`: `recipient`'s record towards `proposer` adds the `threatened` event (slot 20,
    /// +0x328; event 22: limit 0, drift 3, value −30) through `0x00B02CB0` (add, set drift and
    /// limit, clamp). No diplomacy broadcast: the stored relation beliefs stay as they are.
    pub(crate) fn apply_threatened(&mut self, recipient: FactionId, proposer: FactionId) {
        self.relationship_mut(recipient, proposer).add_factor("threatened", super::treaties::event("threatened"));
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
    /// (the round-end upkeep, flag 1, gives the force's set, its faction's sum plus its commander's
    /// own set, `0x008AFC00`), so every upkeep modifier reads 0 and the land unit's `0x008F9B10` (vtable slot
    /// `0x013554D0`) gives round(upkeep × 100 × 0.01) = the unit's raw `upkeep`, whatever its
    /// strength: [`super::economy::unit_upkeep`] with no effects. Ships give their raw upkeep too
    /// and every commanded force counts: CONFIRMED by the original's values read in the debugger (Coalition
    /// start `mp_eur_napoleon`, 1805), which this gives exactly for every listed faction, land and
    /// naval (install test `faction_power_is_the_originals_at_the_coalition_start`). Integer sums as
    /// the exe's. A force without a commander counts 0, before the flag is read (`0x008F9C62`,
    /// CONFIRMED disassembly; [`super::economy::costed_force`], as the upkeep). A unit
    /// key with no `units` record counts 0 and is added to `unknown_units` (the caller logs it).
    pub fn faction_powers(&self, unknown_units: &mut BTreeSet<String>) -> BTreeMap<FactionId, FactionPower> {
        let mut out: BTreeMap<FactionId, FactionPower> = BTreeMap::new();
        for f in self.world.forces.values() {
            let sum = self.force_power(f, unknown_units);
            let p = out.entry(f.faction).or_default();
            let total = if f.is_navy { &mut p.naval } else { &mut p.land };
            *total = total.wrapping_add(sum);
        }
        out
    }

    /// One force's units' summed value with flag 0 (`CalculateForceUnitValueTotal` `0x008F9C50`, an
    /// empty effect set): what [`Self::faction_powers`] sums per force, and the AI's war strength
    /// ([`Self::war_strength`]) per mobile. A unit key with no `units` record counts 0 and is added
    /// to `unknown_units`.
    pub(crate) fn force_power(&self, f: &super::MilitaryForce, unknown_units: &mut BTreeSet<String>) -> i32 {
        let none = super::effects::ForceEffects::default();
        let mut sum = 0i32;
        let units = super::economy::costed_force(f).map_or(&[][..], |(_, units)| units);
        for u in units {
            match self.rules.units.get(&u.unit_key) {
                Some(r) => sum = sum.wrapping_add(super::economy::unit_upkeep(none, &self.rules.features, &u.unit_key, r)),
                None => {
                    if !unknown_units.contains(&u.unit_key) {
                        unknown_units.insert(u.unit_key.clone());
                    }
                }
            }
        }
        sum
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

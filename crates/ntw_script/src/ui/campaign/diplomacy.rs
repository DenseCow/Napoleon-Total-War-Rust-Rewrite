//! The diplomacy screens: relations, treaties and the negotiation object
//! (`UIDiplomacyNegotiation`).

use super::*;

/// The diplomacy panel's negotiation: the host's stand-in for the exe's `UIDiplomacyNegotiation`
/// userdata (ctor 0x00A102B0, 0xB4 bytes) and the campaign negotiation behind it (campaign
/// +0xF9C, built by 0x008AF620 / 0x00BF5A60). One negotiation at a time, as the campaign holds one.
///
/// Lifecycle (CONFIRMED, UI_FIDELITY.md §4.6): the constructor's two-key form begins the campaign
/// negotiation (`CCQ_DIPLOMACY_BEGIN_NEGOTIATION`), which posts "started" to the panel; accept /
/// decline only set the campaign negotiation's result (+0x28, 1 / 2); cancel clears the deal and
/// re-initialises the panel; only `End()` (`CCQ_DIPLOMACY_END_NEGOTIATION`, executor 0x00932F20 →
/// 0x008BC5D0) ends it, posting "ended" once. Every way the panel closes reaches `End()`: the
/// PanelManager runs the panel's `ExitFunc` "OnExit", which ends a negotiation the panel still
/// holds.
#[derive(Debug, Default)]
pub(super) struct NegotiationState {
    /// The campaign negotiation's proposer (+0x18) and recipient (+0x1C).
    proposer: Option<String>,
    recipient: Option<String>,
    /// A campaign negotiation exists (campaign +0xF9C is set).
    open: bool,
    /// Its result (+0x28): 0 open, 1 accepted (0x00C114B0), 2 declined (0x00C1F210).
    status: NegotiationStatus,
    /// The object's counterpart (+0xAC): set when the "started" event reaches it (0x00A147A0);
    /// while unset every accessor answers nothing, as the original does.
    pub(super) target: Option<String>,
    /// The "Offers" rows.
    pub(super) offers: Vec<DealRow>,
    /// The "Demands" rows.
    demands: Vec<DealRow>,
    /// The script context that constructed the object (+0xB0, set from 0x01058750 by
    /// `InitializeUIDiplomacyNegotiationListeners` 0x0099AF70): the component whose globals the
    /// engine's events are LuaCalled in. `None` once `End()` unregistered the listeners.
    context: Option<NodeId>,
}

/// The campaign negotiation's result (+0x28), what `Finished()` reports (0x009BB8B0 pushes
/// `+0x28 != 0`, 0x006CCA70).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum NegotiationStatus {
    #[default]
    Open,
    Accepted,
    Declined,
}

/// The UI negotiation object's listeners (`InitializeUIDiplomacyNegotiationListeners` 0x0099AF70,
/// CONFIRMED): the calls they posted and that are not made yet go when the object is constructed
/// again or ended ([`Inner::cancel_posted`]).
const NEGOTIATION_LISTENER: &str = "UI negotiation object";
/// The HUD's own listener (HUD ctor 0x0098C2F0, CONFIRMED), never unregistered.
const HUD_LISTENER: &str = "campaign HUD";

/// The campaign events the negotiation object and the HUD listen to, posted for the start of the
/// next UI frame ([`Inner::post_call`]). "Started", hub +0x318, posted by the campaign negotiation's
/// constructor (0x00BF5A60) when the proposer is human: `NotifyPanelNegotiationStarted`
/// (0x00A147A0) sets the object's counterpart (+0xAC) and LuaCalls `InitialiseNegotiation(greeting,
/// proposer == local player, false)` in the context that constructed the object. PROVISIONAL: the
/// greeting is empty, so the panel hides its diplomat. The original's is the recipient's
/// `..._receive_<attitude>` diplomacy string, picked by `ResolveDiplomacyNegotiationString`
/// (0x00C55CC0) with a campaign-RNG draw when the faction has an override row (UI_FIDELITY.md
/// §4.6). "Proposer == local player" is the value when the event was posted. A context whose
/// component is gone by then gets nothing.
fn post_negotiation_started(lua: &Lua, inner: &Inner, ui: &Rc<CampaignUi>, context: NodeId, player_proposed: bool) -> mlua::Result<()> {
    let ui = ui.clone();
    inner.post_call(PostedCall {
        listener: NEGOTIATION_LISTENER,
        target: CallTarget::Component(context),
        name: "InitialiseNegotiation",
        args: ("", player_proposed, false).into_lua_multi(lua)?,
        before: Some(Box::new(move || {
            let mut state = ui.negotiation.borrow_mut();
            state.target = state.recipient.clone();
        })),
    });
    Ok(())
}

/// "Cleared", hub +0x348, posted by `CCQ_DIPLOMACY_CLEAR_NEGOTIATION` (0x00932EC0, event flag 0):
/// `NotifyPanelPendingMoveNegotiation` (0x00A14850) LuaCalls `InitialiseNegotiation(nil, proposer
/// == local player, flag)` in the object's context (INFERRED that 0x0097BC10(0x01085B30) pushes
/// nil). Posted as [`post_negotiation_started`] is.
fn post_negotiation_cleared(lua: &Lua, inner: &Inner, context: NodeId, player_proposed: bool) -> mlua::Result<()> {
    inner.post_call(PostedCall {
        listener: NEGOTIATION_LISTENER,
        target: CallTarget::Component(context),
        name: "InitialiseNegotiation",
        args: (Value::Nil, player_proposed, false).into_lua_multi(lua)?,
        before: None,
    });
    Ok(())
}

/// "Ended", hub +0x378, posted when the campaign's negotiation ends (0x008BC5D0): the HUD's
/// listener (vtable 0x0136B5B4, 0x00A14190) LuaCalls the root's `EnableDiplomacy`, which lets
/// `ToggleDiplomacyPopup` open the panel again.
fn post_negotiation_ended(inner: &Inner) {
    inner.post_call(PostedCall { listener: HUD_LISTENER, target: CallTarget::Root, name: "EnableDiplomacy", args: MultiValue::new(), before: None });
}

/// `End()` (0x009BA850): the end command ends the campaign negotiation if there is one (one
/// "ended" event, [`post_negotiation_ended`]), then the object's listeners go and its +0xAC /
/// +0xB0 are cleared (0x009B8560), so nothing more reaches the panel.
fn end_negotiation(inner: &Inner, ui: &CampaignUi) {
    let ended = ui.negotiation.borrow_mut().end();
    if ended {
        post_negotiation_ended(inner);
    }
    inner.cancel_posted(NEGOTIATION_LISTENER);
}

impl NegotiationState {
    /// The constructor's reset of the object (0x0099AF70): a new object with no counterpart, in
    /// `context` (the caller drops the last object's listeners' calls). The campaign negotiation is
    /// not the object's and stays as it is (only `begin` or `end` change it).
    fn construct(&mut self, context: Option<NodeId>) {
        self.target = None;
        self.context = context;
    }

    /// Whether the campaign negotiation's proposer is the local player (`human`): what the
    /// "started" and "cleared" events carry (0x00A147A0 / 0x00A14850 compare the proposer, +0x18,
    /// with the local player faction, 0x009BF110).
    fn player_proposed(&self, human: &str) -> bool {
        self.proposer.as_deref() == Some(human)
    }

    /// `CCQ_DIPLOMACY_BEGIN_NEGOTIATION` (the two-key constructor): a new campaign negotiation,
    /// built over whatever was there (0x008AF620 overwrites +0xF9C without ending it).
    pub(super) fn begin(&mut self, proposer: String, recipient: String) {
        self.proposer = Some(proposer);
        self.recipient = Some(recipient);
        self.open = true;
        self.status = NegotiationStatus::Open;
        self.clear_deal();
    }

    /// Empties the deal (BEGIN, CLEAR, END).
    fn clear_deal(&mut self) {
        self.offers.clear();
        self.demands.clear();
    }

    /// The state side of [`end_negotiation`]: ends the campaign negotiation if there is one and
    /// clears the object's +0xAC / +0xB0. Returns whether a negotiation ended (the "ended" event
    /// is then due).
    pub(super) fn end(&mut self) -> bool {
        // 0x008BC5D0: only while +0xF9C is set; it posts "ended" and deletes the negotiation.
        let ended = std::mem::take(&mut self.open);
        if ended {
            self.status = NegotiationStatus::Open;
            self.clear_deal();
        }
        self.target = None;
        self.context = None;
        ended
    }
}

/// One row of the deal: its item and whether it was applied, the row's +0x1E8 flag (CONFIRMED,
/// see [`accept_deal`]): the appliers skip an applied row, so a row applies at most once.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DealRow {
    pub(super) item: NegotiationItem,
    pub(super) applied: bool,
}

/// One item of the pending deal. The variants are the model's `DiplomaticAction`s plus the region,
/// payment and technology items (see `accept_deal` for which applier each has).
///
/// Nothing constructs an item yet, and that is the traced state of the original, not a gap in the
/// host: no CONFIRMED method takes the deal in. `BuildOfferAndDemandStrings` (0x009B48B0) reads no
/// Lua argument and only *reports* the rows, `ProposeDeal` (0x009BFCF0, 142 bytes) is the deal's
/// validation/error path, and `Propose` (0x009BF3C0, 2285 bytes, 40 callees) does the work on the
/// engine's own object. In the exe the rows are mutated by the engine when the panel's subpopup
/// OK buttons run, so the host waits for those callback bodies (UI_FIDELITY.md §4 open item 3)
/// before it fills the list.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub(super) enum NegotiationItem {
    /// Region keys to hand over (the exe's region-transfer action, id 6 in
    /// `BuildOfferAndDemandStrings`). The exe's applier is `0x00B449F0` (CONFIRMED address); the
    /// model has no command for it yet, so the row is dropped (DEFERRED, see `deal_item_commands`).
    Regions(Vec<String>),
    /// Technology keys to hand over. Still **UNKNOWN**: no granting address is reachable from the
    /// deal path, so the row is dropped (see `accept_deal`).
    Technologies(Vec<String>),
    /// A payment: an amount and how many turns it runs for. `turns == 0` is the lump sum the commit
    /// path runs as **`0x00BB3810(amount, 3)`** (CONFIRMED, the same money mover as the capture loot
    /// `0x00BB3810(money, 0)`); a positive `turns` is the per-turn schedule, one of the container's
    /// two optional single items (`+0x20` / `+0x24`, UNKNOWN which) -- INFERRED that it is the
    /// payment, since that is what a schedule on the deal would be for. The model has both: a state
    /// gift (a lump sum, `0x00B44590`) and a regular payment (per turn, `treaties::DiplomaticAction`).
    Payment {
        /// The amount moved.
        amount: i32,
        /// How many turns it runs for; 0 for the lump sum.
        turns: u32,
    },
    /// A diplomatic action with a model command (`DiplomaticAction`).
    Action(ntw_sim::campaign::treaties::DiplomaticAction),
}

/// Attitude level of an attitude total (0x00B0DBA0, CONFIRMED rule): the
/// `diplomatic_relations_attitudes` values (hostile, unfriendly, neutral, friendly, very_friendly)
/// give four thresholds, each the mean of two neighbours; total ≤ t1 → 0 hostile, ≤ t2 → 1
/// unfriendly, ≤ t3 → 2 neutral, ≤ t4 → 3 friendly, else 4 very friendly.
fn attitude_level(levels: &HashMap<String, i32>, total: i32) -> usize {
    let v = |k: &str| levels.get(k).copied().unwrap_or(0);
    let t = [(v("hostile") + v("unfriendly")) / 2, (v("unfriendly") + v("neutral")) / 2, (v("neutral") + v("friendly")) / 2, (v("friendly") + v("very_friendly")) / 2];
    t.iter().position(|&x| total <= x).unwrap_or(4)
}

/// The attitude factors of a relationship as the exe lists them (0x00B27760, CONFIRMED pieces):
/// one "[ALIGN:L]<factor>: [ALIGN:R]<±n>" line per factor that counts, the factor named by loc
/// `diplomacy_factor_strings_<positive|negative>_factor_string_<factor>`. PROVISIONAL: the exact
/// spacing and the peace-treaty special case of the original are not reproduced.
fn relationship_details(inner: &Inner, rel: &ntw_sim::campaign::Relationship) -> String {
    let mut lines = Vec::new();
    for (i, key) in ntw_sim::campaign::details::ATTITUDE_FACTORS.iter().enumerate() {
        let Some(f) = rel.attitudes.get(i) else { continue };
        let n = f.contribution();
        if n == 0 {
            continue;
        }
        let sign = if n > 0 { "positive" } else { "negative" };
        let name = loc(inner, &format!("diplomacy_factor_strings_{sign}_factor_string_{key}")).unwrap_or_else(|| (*key).to_owned());
        lines.push(format!("[ALIGN:L]{name}: [ALIGN:R]{n:+}"));
    }
    lines.join("\n")
}

/// `CampaignUI.RetrieveFactionListForDiplomacy()` (0x009F2EA0, CONFIRMED keys) → a table keyed by
/// faction key, one entry per faction except pirates: Name, Key, IsHuman, IsMajor, FlagPath,
/// ReligionIcon, ReligionName, Government, GovernmentName; for factions other than the player
/// also Relationship ("allied", "at war", "protectorate" or nil, the player's stance towards it),
/// Trading ("trading" with a trade agreement, else "can_trade" / "cannot_trade"), LandTrade,
/// TradingTooltip (random loc `trade_status_tooltip_already_trading` / `_can` / `_cannot`),
/// Attitude (diplomacy loc `relationship_<level>` of the faction's attitude towards the player,
/// 0x00B64CC0), AttitudeValue (that level, 0..4), PlayersRelationshipDetails and
/// FactionRelationshipDetailsTowardsPlayer (the attitude factor lists). PROVISIONAL: can_trade
/// when not at war (the exe's trade-route tests 0x00C1A7D0 / 0x00B27FE0 are not ported),
/// LandTrade false; destroyed factions are kept (the exe skips factions with +0x824 set, UNKNOWN).
fn faction_list_for_diplomacy(lua: &Lua, inner: &Inner, ui: &CampaignUi) -> mlua::Result<Value> {
    let m = ui.model();
    let out = lua.create_table()?;
    let me = m.faction_by_key(&ui.link.human).map(|f| f.id);
    let levels = ui.attitude_levels(inner);
    let word = |k: &str| loc(inner, &format!("random_localisation_strings_string_{k}")).unwrap_or_default();
    for f in m.world.factions.values() {
        if f.key == "pirates" {
            continue;
        }
        let e = lua.create_table()?;
        e.set("Name", faction_name(inner, &ui.link.db, &f.key))?;
        e.set("Key", f.key.as_str())?;
        e.set("IsHuman", m.turn.humans.contains(&f.id))?;
        let details = m.world.faction_details.get(&f.id);
        e.set("IsMajor", details.and_then(|d| d.major).unwrap_or(false))?;
        e.set("FlagPath", ui.link.db.faction(&f.key).map(|r| r.flag_path.clone()).unwrap_or_default())?;
        let religion = details.map(|d| d.religion.clone()).unwrap_or_default();
        e.set("ReligionIcon", ui.religion_icon(inner, &religion))?;
        e.set("ReligionName", loc(inner, &format!("religions_onscreen_{religion}")).unwrap_or(religion))?;
        e.set("Government", f.government_key.as_str())?;
        e.set("GovernmentName", loc(inner, &format!("government_types_onscreen_{}", f.government_key)).unwrap_or_default())?;
        if let Some(me) = me
            && me != f.id
        {
            let stance = m.world.factions.get(&me).and_then(|p| p.diplomacy.get(&f.id).copied()).unwrap_or_default();
            let relationship = match stance {
                ntw_sim::campaign::Stance::Allied => Some("allied"),
                ntw_sim::campaign::Stance::War => Some("at war"),
                ntw_sim::campaign::Stance::Protectorate | ntw_sim::campaign::Stance::Patron => Some("protectorate"),
                ntw_sim::campaign::Stance::Neutral => None,
            };
            e.set("Relationship", relationship)?;
            let towards_me = m.world.relationships.get(&(f.id, me));
            let mine = m.world.relationships.get(&(me, f.id));
            if stance != ntw_sim::campaign::Stance::War {
                let (trading, tip) = if mine.is_some_and(|r| r.trade_agreement) {
                    ("trading", "trade_status_tooltip_already_trading")
                } else {
                    ("can_trade", "trade_status_tooltip_can")
                };
                e.set("Trading", trading)?;
                e.set("LandTrade", false)?;
                e.set("TradingTooltip", word(tip))?;
            }
            let level = attitude_level(&levels, towards_me.map_or(0, |r| r.attitude_total()));
            const NAMES: [&str; 5] = ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"];
            e.set("Attitude", loc(inner, &format!("diplomacy_strings_string_relationship_{}", NAMES[level])).unwrap_or_default())?;
            e.set("AttitudeValue", level)?;
            e.set("PlayersRelationshipDetails", mine.map(|r| relationship_details(inner, r)).unwrap_or_default())?;
            e.set("FactionRelationshipDetailsTowardsPlayer", towards_me.map(|r| relationship_details(inner, r)).unwrap_or_default())?;
        }
        out.set(f.key.as_str(), e)?;
    }
    Ok(Value::Table(out))
}


/// `CampaignUI.RetrieveDiplomacyDetails(faction)` (0x009F2750 → 0x009B2690, CONFIRMED keys) →
/// `{AtWar, Allies, TradeRights, Protectorates, ProtectorOf}`: the factions the given faction is at
/// war with (stance 0), allied to (2), trades with (trade agreement), has as protectorates (3),
/// is the protectorate of (4). Each entry (0x009B1B50): Name, Label (the faction key), Flag
/// ("<flag path>/small.tga"). Nil for an unknown faction. Our stances map as Protectorate → 3,
/// Patron → 4 (INFERRED).
fn diplomacy_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, key: &str) -> mlua::Result<Value> {
    use ntw_sim::campaign::Stance;
    let m = ui.model();
    let Some(f) = m.faction_by_key(key) else { return Ok(Value::Nil) };
    let lists: Vec<Table> = (0..5).map(|_| lua.create_table()).collect::<mlua::Result<_>>()?;
    for (other, stance) in &f.diplomacy {
        let Some(o) = m.world.factions.get(other) else { continue };
        let e = lua.create_table()?;
        e.set("Name", faction_name(inner, &ui.link.db, &o.key))?;
        e.set("Label", o.key.as_str())?;
        let flag = ui.link.db.faction(&o.key).map(|r| r.flag_path.clone()).unwrap_or_default();
        e.set("Flag", format!("{flag}/small.tga"))?;
        let list = match stance {
            Stance::War => Some(0),
            Stance::Allied => Some(1),
            Stance::Protectorate => Some(3),
            Stance::Patron => Some(4),
            Stance::Neutral => None,
        };
        if let Some(i) = list {
            lists[i].set(o.key.as_str(), e.clone())?;
        }
        if m.world.relationships.get(&(f.id, *other)).is_some_and(|r| r.trade_agreement) {
            lists[2].set(o.key.as_str(), e)?;
        }
    }
    let out = lua.create_table()?;
    for (i, name) in ["AtWar", "Allies", "TradeRights", "Protectorates", "ProtectorOf"].iter().enumerate() {
        out.set(*name, lists[i].clone())?;
    }
    Ok(Value::Table(out))
}


/// `CampaignUI.RetrieveExistingTreaties(a, b)` → one string listing the treaties of `a` with `b`
/// (0x009F2B80 → 0x00B750D0, CONFIRMED pieces), each line a diplomacy loc string
/// `current_treaty_<x>`: protectorate_of_player, at_war, alliance, trade_agreement,
/// giving_military_access_indefinite / _turns, has_military_access_indefinite / _turns,
/// trade_embargoed, embargoing_trade ("%d" = turns). PROVISIONAL: the non-player protectorate
/// lines and the peace-treaty countdown are not listed; lines are joined with "\n" (the exe's
/// joiner 0x00B0B120 is not decoded).
fn existing_treaties(inner: &Inner, ui: &CampaignUi, a: &str, b: &str) -> Option<String> {
    use ntw_sim::campaign::Stance;
    let m = ui.model();
    let fa = m.faction_by_key(a)?;
    let fb = m.faction_by_key(b)?;
    let text = |k: &str| loc(inner, &format!("diplomacy_strings_string_current_treaty_{k}")).unwrap_or_default();
    let turns = |k: &str, n: i32| text(k).replace("%d", &n.to_string());
    let mut lines = Vec::new();
    let stance = fa.diplomacy.get(&fb.id).copied().unwrap_or_default();
    if stance == Stance::Patron {
        lines.push(text("protectorate_of_player"));
    }
    match stance {
        Stance::War => lines.push(text("at_war")),
        Stance::Allied => lines.push(text("alliance")),
        _ => {}
    }
    let ab = m.world.relationships.get(&(fa.id, fb.id));
    let ba = m.world.relationships.get(&(fb.id, fa.id));
    if ab.is_some_and(|r| r.trade_agreement) {
        lines.push(text("trade_agreement"));
    }
    match ab.map_or(0, |r| r.military_access_turns) {
        0 => {}
        n if n < 0 => lines.push(text("giving_military_access_indefinite")),
        n => lines.push(turns("giving_military_access_turns", n)),
    }
    match ba.map_or(0, |r| r.military_access_turns) {
        0 => {}
        n if n < 0 => lines.push(text("has_military_access_indefinite")),
        n => lines.push(turns("has_military_access_turns", n)),
    }
    if let Some(n) = ba.map(|r| r.trade_embargo_turns).filter(|n| *n > 0) {
        lines.push(turns("trade_embargoed", n as i32));
    }
    if let Some(n) = ab.map(|r| r.trade_embargo_turns).filter(|n| *n > 0) {
        lines.push(turns("embargoing_trade", n as i32));
    }
    Some(lines.join("\n"))
}

/// The diplomacy loc name of `from`'s attitude towards `to` (0x00B64CC0: `relationship_<level>`).
fn attitude_text(inner: &Inner, ui: &CampaignUi, from: &str, to: &str) -> Option<String> {
    let m = ui.model();
    let (f, t) = (m.faction_by_key(from)?.id, m.faction_by_key(to)?.id);
    let total = m.world.relationships.get(&(f, t)).map_or(0, |r| r.attitude_total());
    const NAMES: [&str; 5] = ["hostile", "unfriendly", "neutral", "friendly", "very_friendly"];
    let level = attitude_level(&ui.attitude_levels(inner), total);
    loc(inner, &format!("diplomacy_strings_string_relationship_{}", NAMES[level]))
}


// ===== Negotiation helpers =====

/// The proposer and recipient of the counterpart's negotiation, as ids: the one guard every
/// negotiation entry point and accessor shares. `None` unless the object has a counterpart (+0xAC,
/// set by "started", cleared by `End()`) and the campaign negotiation exists (+0xF9C). The exe's
/// wrappers test +0xAC (TradeableRegions also +0xF9C, 0x009C5770); ours never has a counterpart
/// without the negotiation, so the two tests agree everywhere.
fn negotiation_factions(ui: &CampaignUi) -> Option<(FactionId, FactionId)> {
    let state = ui.negotiation.borrow();
    if state.target.is_none() || !state.open {
        return None;
    }
    let m = ui.model();
    let id = |key: Option<&str>| key.and_then(|k| m.faction_by_key(k)).map(|f| f.id);
    Some((id(state.proposer.as_deref())?, id(state.recipient.as_deref())?))
}

/// What `MaxPlayerPaymentAllowed` / `MaxOppositionPaymentAllowed` (`method`) push: the faction's
/// treasury (CONFIRMED, `GetFactionEconomyTreasury` 0x00BCAFE0). Always a number: with no faction
/// to ask (the exe reads a null pointer there) the answer is 0, logged once per method.
fn payment_cap(inner: &Inner, ui: &CampaignUi, faction: Option<FactionId>, method: &'static str) -> i32 {
    let treasury = faction.and_then(|f| ui.model().world.factions.get(&f).map(|f| f.treasury));
    treasury.unwrap_or_else(|| {
        inner.log_once(method, || {
            format!("ERROR {method}: no faction to ask (no counterpart, or no local player faction): 0 (logged once)")
        });
        0
    })
}

/// Whether a region is already in the deal's offer (`offered`) or demand list.
fn region_in_deal(ui: &CampaignUi, key: &str, offered: bool) -> bool {
    let state = ui.negotiation.borrow();
    let items = if offered { &state.offers } else { &state.demands };
    items.iter().any(|r| matches!(&r.item, NegotiationItem::Regions(keys) if keys.iter().any(|k| k == key)))
}

/// The "Action" a deal row reports (CONFIRMED key). The exe's action *ids* are UNKNOWN (only the
/// region action's 6 is known, from `BuildOfferAndDemandStrings`), so these are the host's names
/// (INFERRED) except the region action, whose name is fixed by the id.
pub(super) fn negotiation_action_name(item: &NegotiationItem) -> &'static str {
    match item {
        NegotiationItem::Regions(_) => "transfer_region",
        NegotiationItem::Technologies(_) => "transfer_technology",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::Alliance) => "alliance",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::BreakAlliance) => "break_alliance",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::TradeAgreement) => "trade_agreement",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::BreakTrade) => "break_trade",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::Embargo) => "embargo",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::BecomeProtectorate) => "protectorate",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::GrantMilitaryAccess(_)) => "military_access",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::CancelMilitaryAccess) => "cancel_military_access",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::StateGift(_)) => "state_gift",
        NegotiationItem::Action(ntw_sim::campaign::treaties::DiplomaticAction::RegularPayment(_, _)) => "payments",
        NegotiationItem::Payment { .. } => "payments",
    }
}

/// Accept the pending deal: the campaign negotiation's result becomes "accepted" (+0x28 = 1,
/// 0x00C114B0) and, the first time only, every offer and demand the model can express becomes a
/// `CampaignCommand` (the appliers skip a row already marked applied, +0x1E8, so a deal applies
/// at most once; see [`DealRow`]). Nothing without a counterpart
/// ([`negotiation_factions`]).
///
/// The exe's appliers (own Ghidra copy, all CONFIRMED addresses):
/// - **offer applier** `0x00B58C00(item)` and **demand applier** `0x00B58A30(faction)` walk the
///   same deal container — the sub-object at `*(vt+0x38)+0x120`, whose `+0x14` is the row count and
///   `+0x18` the first row, plus two *optional single* items at `+0x20` and `+0x24`. Per row they
///   test the row's `+0x1E8` "applied" flag, call `0x00B1A760(1)` and then the per-item applier
///   `0x00A6CBE0(0)`; the two single items go through the same `0x00A6CBE0(0)`.
///   `0x00B58560` and `0x00B58890` are two more appliers with the same walk (the first also builds
///   a loc string, id `0xFD`).
/// - **per-item / commit** `0x00A6CBE0` is an 11-byte forward to **`0x00B1A790(flag)`**, the deal
///   commit: when `0x00B4E4B0()` and `flag` agree and the pending deal's `+0x14` amount is
///   non-zero it runs **`0x00BB3810(amount, 3)`** — that is the **payments / tribute applier** — then
///   frees the pending deal (`0x0126E016(deal, 0x18)`), clears it and notifies two listeners.
///   `0x00B1A760(flag)` drains the pending list at `+0x4C`/`+0x50` through `0x00B1A820(item, flag)`,
///   which sets the item's vtable `+0x1C` slot, applies it and erases it from the vector.
/// - **region transfer** `0x00B449F0(faction, region_item, 1, 1, 0)` is applied *once*, after the
///   rows, by all four of those appliers; the demand path passes a null `region_item`, the offer
///   path the region it was handed. `0x00B58A10(faction, a, b)` is the wrapper that calls
///   `0x00B449F0(faction, 0, 0, a, b)` (the liberate path, CAMPAIGN_FIDELITY.md §Capture).
///
/// WIRED (0-E round N+2), one `CampaignCommand` per row type that now has an applier:
/// - **stance / access / gift terms** → `CampaignCommand::Diplomacy`. INFERRED mapping from
///   `negotiation_action_name` onto `DiplomaticAction`; the exe's own per-row `DiplomaticAction`
///   appliers are `0x00B55090` (trade), `0x00B29BB0` (break trade), `0x00B28DB0` (embargo),
///   `0x00B44550` (military access), `0x00B67BD0` (cancel access), `0x00B44590` (state gift) and
///   `0x00B105C0` (protectorate) — CONFIRMED addresses, already ported by 0-G in `treaties.rs`.
/// - **payments** → `CampaignCommand::Diplomacy` again: the lump sum (`turns == 0`) as a
///   `StateGift(amount)`, which is the model's mover for "this faction gives that faction this much
///   now" (`treaties::state_gift`, `0x00B44590` → `0x00B446B0`) — INFERRED as the stand-in for
///   **`0x00BB3810(amount, 3)`**, the CONFIRMED lump sum the commit runs, which moves money and also
///   raises the receiver's `state_gift` factor (the factor's exact contribution on that path is
///   UNKNOWN). The per-turn schedule becomes `RegularPayment(amount, turns)`, one of the container's
///   two optional single items (`+0x20` / `+0x24`, UNKNOWN which) — INFERRED that it is the payment.
///
/// STILL DROPPED (PLACEHOLDER): **technologies** → UNKNOWN. The rows go through `0x00A6CBE0` →
/// `0x00B1A790`, which only commits money; the per-row-type work happens behind the row vtable
/// (`0x00B1A820` calls vtable `+0x1C`) and **no technology-granting address has been traced from the
/// deal path** — reachable only through that vtable. The model has no `GrantTechnology` command, and
/// 0-E proved none is reachable, so no address is guessed here. Left open: UI_FIDELITY.md §4.5 item 8.
///
/// DEFERRED: **regions** (`0x00B449F0`, CONFIRMED address). The sandbox mapped them onto a
/// `TransferRegion` command whose effect was INFERRED from the capture path (it left the old owner's
/// garrison army behind in the settlement); main has no such command, so the rows are dropped until
/// `0x00B449F0` is decoded.
///
/// INFERRED (this file): `Propose` / `ProposeDeal` (whose AI evaluation the model lacks) and
/// `AcceptOffer` (CCQ_DIPLOMACY_ACCEPT_DEAL → 0x00C114B0) all reach this one applier; the exe
/// applies a deal from its own engine-side object instead.
///
/// Nothing fills the deal from Lua yet (see [`NegotiationItem`]), so in play this applies an empty
/// deal. Applying a deal leaves it in place (it stays until END) and does not end the negotiation
/// (only `End()` does).
fn accept_deal(ui: &CampaignUi) {
    let Some((proposer, recipient)) = negotiation_factions(ui) else { return };
    let mut state = ui.negotiation.borrow_mut();
    let state = &mut *state;
    state.status = NegotiationStatus::Accepted;
    // An offer is the proposer acting on the recipient; a demand is the other way round.
    for (rows, actor, target) in [(&mut state.offers, proposer, recipient), (&mut state.demands, recipient, proposer)] {
        for row in rows.iter_mut().filter(|r| !r.applied) {
            row.applied = true;
            for cmd in deal_item_commands(&row.item, actor, target) {
                ui.push(CampaignRequest::Command(cmd));
            }
        }
    }
}

/// The `CampaignCommand`s one pending deal row becomes: `actor` acts on `target`. Split out of
/// [`accept_deal`] so the per-row mapping can be tested without a running campaign.
pub(super) fn deal_item_commands(item: &NegotiationItem, actor: FactionId, target: FactionId) -> Vec<CampaignCommand> {
    use ntw_sim::campaign::treaties::DiplomaticAction;
    match item {
        NegotiationItem::Action(action) => vec![CampaignCommand::Diplomacy { a: actor, b: target, action: *action }],
        // INFERRED: a lump sum is the CONFIRMED `0x00BB3810(amount, 3)` money move, which the model
        // applies as a state gift; a schedule is the container's per-turn single item.
        NegotiationItem::Payment { amount, turns } => {
            let action = match *turns {
                0 => DiplomaticAction::StateGift(*amount),
                t => DiplomaticAction::RegularPayment(*amount, t),
            };
            vec![CampaignCommand::Diplomacy { a: actor, b: target, action }]
        }
        // DEFERRED: `0x00B449F0` has no model command on main (see `accept_deal`).
        NegotiationItem::Regions(_) => Vec::new(),
        // PLACEHOLDER: UNKNOWN, no granting address is reachable. See `accept_deal`.
        NegotiationItem::Technologies(_) => Vec::new(),
    }
}

/// The negotiation object and its constructor. The panel script gets its negotiation from the
/// constructor, `negotiation = UIDiplomacyNegotiation(player_faction, opposing_faction)` in
/// `Initialise` (diplomacy_panel.lua:176) or `UIDiplomacyNegotiation(pending)` in
/// `RequestDiplomacy` (:199), and keeps it in a local (no shipped script reads a global
/// `negotiation`). The constructor (`CreateUIDiplomacyNegotiationFromScript` 0x00A102B0 →
/// `InitializeUIDiplomacyNegotiationListeners` 0x0099AF70, CONFIRMED):
/// - remembers the script context the engine is running (+0xB0), registers its listeners on the campaign's
///   event hub;
/// - every call is a new object: no counterpart or listener of the previous object is kept
///   ([`NegotiationState::construct`]);
/// - two faction keys (the top argument is a string): looks both factions up and queues the
///   open-negotiation command. The campaign's negotiation constructor (0x00BF5A60) then posts the
///   "started" event (hub +0x318) when the proposer is human, so the panel's
///   `InitialiseNegotiation` runs after `Initialise` has returned (and after its
///   `SetCloseable(false)`): the event's call is made at the start of the next UI frame
///   ([`post_negotiation_started`]; INFERRED that the queued command runs before
///   the next UI frame). Without a calling component there is no context to deliver to: logged
///   once;
/// - another argument (a pending move): stores it as the counterpart (+0xAC). PROVISIONAL: the
///   model has no pending diplomatic moves, so the object stays empty and no event follows
///   (logged once).
///
/// The methods are the `negotiation:*` set installed on `CampaignUI`; there is one object (one
/// negotiation at a time, the campaign's +0xF9C), so every call returns the same table.
pub(super) fn install_negotiation_object(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>) -> mlua::Result<()> {
    let methods: Table = lua.globals().get("CampaignUI")?;
    let obj = lua.create_table()?;
    for name in [
        "BuildPossibleActions",
        "BuildOfferAndDemandStrings",
        "TradeableRegions",
        "TradeableTechnologies",
        "FactionListsForStanceDeclarations",
        "MaxPlayerPaymentAllowed",
        "MaxOppositionPaymentAllowed",
        "ProposerId",
        "Propose",
        "ProposeDeal",
        "AcceptOffer",
        "End",
        "Cancel",
        "DeclineOffer",
        "RemoveAction",
        "CanPropose",
        "CanThreaten",
        "PrepareCounterOffer",
        "Finished",
        "IsNegotiation",
    ] {
        if let Ok(m) = methods.raw_get::<Value>(name) {
            obj.set(name, m)?;
        }
    }
    // `UIDiplomacyNegotiation(...)`: the constructor, in the context the engine is running
    // (`host::running_script_context`, 0x01058750's `lookup[L]`).
    let (inner, ui) = (inner.clone(), ui.clone());
    let new = lua.create_function(move |lua, args: Variadic<Value>| {
        let context = super::super::host::running_script_context(&inner);
        inner.cancel_posted(NEGOTIATION_LISTENER);
        ui.negotiation.borrow_mut().construct(context);
        if let [Value::String(proposer), Value::String(recipient), ..] = args.as_slice() {
            let player_proposed = {
                let mut state = ui.negotiation.borrow_mut();
                state.begin(proposer.to_string_lossy(), recipient.to_string_lossy());
                state.player_proposed(&ui.link.human)
            };
            match (player_proposed, context) {
                (true, Some(context)) => post_negotiation_started(lua, &inner, &ui, context, player_proposed)?,
                (true, None) => inner.log_once("negotiation without context", || {
                    "UNKNOWN UIDiplomacyNegotiation was called with no component script running: the \
                     negotiation's events have no panel to go to (logged once)"
                        .into()
                }),
                (false, _) => {}
            }
        } else {
            inner.log_once("pending move negotiation", || {
                "UNKNOWN UIDiplomacyNegotiation(pending move): the model has no pending diplomatic moves, \
                 so the negotiation stays empty (logged once)"
                    .into()
            });
        }
        Ok(obj.clone())
    })?;
    lua.globals().set("UIDiplomacyNegotiation", new)?;
    Ok(())
}

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // Theatres. TheatreList(no_sea_trade) → one entry per theatre of the campaign map (0x009F96C0
    // → 0x009B2080 → 0x009ACE50, CONFIRMED): Address, Id (the `campaign_map_playable_areas` row's
    // area column, e.g. "europe_main"), Name, Key (the theatre key = the row's key, e.g.
    // "1244818741", also the theatre's name in regions.esf), SeaTrade (the row's bool column; with
    // `true` those theatres are left out). Name: loc
    // `campaign_map_playable_areas_onscreen_name_<key>` (the exe reads a theatre string, INFERRED
    // to be that loc). HomeTheatre(faction) → the theatre key (CONFIRMED 0x009E5180).
    // GovernorshipList(faction) → {Name, Key, TheatreKey} (CONFIRMED names 0x009E4D50).
    // PROVISIONAL: the campaign's theatre is found from the campaign key (`theatre_of`); each
    // Napoleon map has one.
    // RetrieveExistingTreaties(a, b) → see `existing_treaties`.
    f!("RetrieveExistingTreaties", |_l, inner, ui, (a, b): (String, String)| Ok(existing_treaties(&inner, &ui, &a, &b)));
    // RetrieveDiplomaticOpinions(a, b) → two attitude texts (0x009F2830, CONFIRMED: two
    // `relationship_<level>` strings, one per direction; order INFERRED: b's opinion of a, then
    // a's opinion of b).
    f!("RetrieveDiplomaticOpinions", |_l, inner, ui, (a, b): (String, String)| {
        Ok((attitude_text(&inner, &ui, &b, &a), attitude_text(&inner, &ui, &a, &b)))
    });
    // RetrieveRemainingMilitaryAccessTurns(a, b) → two numbers, one per direction: the
    // relationship's military access turns (+0x78C), or -1 when the stance is 4 (protector)
    // (0x009F4380, CONFIRMED; order INFERRED as b → a, then a → b; our Patron stance = 4).
    f!("RetrieveRemainingMilitaryAccessTurns", |_l, inner, ui, (a, b): (String, String)| {
        let m = ui.model();
        let (Some(fa), Some(fb)) = (m.faction_by_key(&a), m.faction_by_key(&b)) else { return Ok((None, None)) };
        let turns = |x: &ntw_sim::campaign::Faction, y: &ntw_sim::campaign::Faction| {
            if x.diplomacy.get(&y.id) == Some(&ntw_sim::campaign::Stance::Patron) {
                -1
            } else {
                m.world.relationships.get(&(x.id, y.id)).map_or(0, |r| r.military_access_turns)
            }
        };
        Ok((Some(turns(fb, fa)), Some(turns(fa, fb))))
    });
    // RetrieveDiplomaticStanceString(a, b): the stance string for a relationship, CONFIRMED one call
    // site and UNKNOWN shape (no argument reads, no literal string in the wrapper). The value here
    // is INFERRED: the stance's debug name, and the loc key is left alone because the wrapper gives
    // no key to build one from.
    f!("RetrieveDiplomaticStanceString", |_l, inner, ui, (a, b): (String, String)| {
        let m = ui.model();
        let (Some(fa), Some(fb)) = (m.faction_by_key(&a), m.faction_by_key(&b)) else { return Ok(String::new()) };
        let stance = fa.diplomacy.get(&fb.id).copied().unwrap_or(ntw_sim::campaign::Stance::Neutral);
        Ok(format!("{stance:?}").to_lowercase())
    });
    // InviteAlliesIntoWar(a, b): CONFIRMED one call site, UNKNOWN shape. PLACEHOLDER: the model has
    // no "call the allies into the war" command (the open end of `call_allies`, UI_FIDELITY.md §4
    // open item 4), so this only reports whether there is anyone to call (INFERRED) and never
    // invites anybody.
    f!("InviteAlliesIntoWar", |_l, inner, ui, (a, b): (String, String)| {
        let m = ui.model();
        let (Some(fa), Some(fb)) = (m.faction_by_key(&a), m.faction_by_key(&b)) else { return Ok(false) };
        let has_allies = m.world.factions.values().any(|f| {
            f.id != fa.id
                && f.id != fb.id
                && m.in_the_game(f.id)
                && matches!(
                    m.world.stance(f.id, fa.id),
                    ntw_sim::campaign::Stance::Allied
                        | ntw_sim::campaign::Stance::Patron
                        | ntw_sim::campaign::Stance::Protectorate
                )
                && m.world.stance(f.id, fb.id) != ntw_sim::campaign::Stance::War
        });
        Ok(has_allies)
    });
    // IsMultiplayerOttomansFrenchDiplomacy(): only in multiplayer (CONFIRMED description); false.
    f!("IsMultiplayerOttomansFrenchDiplomacy", |_l, inner, ui, _a: Variadic<Value>| Ok(false));
    // StateGiftValues() → {v1, v2, v3}: the `state_gift_values` values in ascending order
    // (0x009F79B0, CONFIRMED: copies the table's values, sorts them, appends each).
    f!("StateGiftValues", |lua, inner, ui, _a: Variadic<Value>| {
        let mut v: Vec<i32> =
            small_table(&inner, "db/state_gift_values_tables/state_gift_values", "s,i").iter().filter_map(|r| r.get(1)?.as_i32()).collect();
        v.sort();
        lua.create_sequence_from(v)
    });
    // MinisterPortraitPath(faction) → a minister's portrait for the negotiation screen (0x009ED8A0
    // picks one through the minister agent record, "random" per the description). PROVISIONAL:
    // the card picture of the faction's first character whose portrait is a minister's, else of
    // its leader; "" if none.
    f!("MinisterPortraitPath", |_l, inner, ui, key: Option<String>| {
        let m = ui.model();
        let Some(f) = m.faction_by_key(&key.unwrap_or_else(|| ui.link.human.clone())).map(|f| f.id) else { return Ok(String::new()) };
        let cards: Vec<&str> = m
            .world
            .characters
            .values()
            .filter(|c| c.faction == f)
            .filter_map(|c| portrait_card(&m, c.id))
            .collect();
        let pick = cards.iter().find(|p| p.to_ascii_lowercase().contains("/minister/")).or(cards.first());
        Ok(pick.map(|p| format!("data/{p}")).unwrap_or_default())
    });
    // RetrieveDiplomacyDetails(key): a read only. The negotiation is opened by the panel's own
    // constructor call, `UIDiplomacyNegotiation(player, opposing)` (see `install_negotiation_object`).
    f!("RetrieveDiplomacyDetails", |lua, inner, ui, key: Option<String>| {
        let key = key.unwrap_or_else(|| ui.link.human.clone());
        diplomacy_details(lua, &inner, &ui, &key)
    });
    f!("RetrieveFactionListForDiplomacy", |lua, inner, ui, _a: Variadic<Value>| faction_list_for_diplomacy(lua, &inner, &ui));
    // ===== Negotiation (`UIDiplomacyNegotiation`, userdata ctor 0x00A102B0) =====
    //
    // The panel script drives a *negotiation object*, not the CampaignUI table: the 19
    // `negotiation:*` receivers in `worker3/lua_api.txt` are calls on one object, and the exe's
    // tolua ctor 0x00A102B0 allocates a 0xB4-byte userdata named "UIDiplomacyNegotiation"
    // (CONFIRMED, own Ghidra copy) whose fields start with the counterparty faction id at +0xAC
    // (every accessor tested returns nothing while +0xAC is 0). The host therefore keeps the
    // pending deal in `ui.negotiation` and hands the script the same methods, on the
    // object the panel's constructor call returns (`install_negotiation_object`).
    //
    // Confidence tags below are per shape:
    //   CONFIRMED - read out of the decompiled wrapper (arg reads, pushed return values, and the
    //               literal key strings the wrapper pushes);
    //   INFERRED  - the shape is confirmed but the value comes from the model;
    //   UNKNOWN   - the wrapper's shape gave nothing, so the host returns the neutral value.

    // BuildPossibleActions(): `PushNegotiationPossibleActionsFromScript` (0x009B5220, CONFIRMED
    // shape): nothing while the counterpart (+0xAC) is unset, else one table
    // `{OffersAndDemands = {...}, Unilaterals = {...}}` whose entries are
    // `{State, Active, Address, Unilateral, Tooltip}` (InitialiseNegotiation hands each list to
    // CreateButtons, which builds one `diplomacy_button` per entry).
    // PROVISIONAL: both lists are empty. The entries come from the campaign's diplomatic action
    // list (0x00C45E90), each action's availability (0x00C1A530) and its state name (0x009C7700),
    // none of which the model has yet (UI_FIDELITY.md §4.6).
    f!("BuildPossibleActions", |lua, inner, ui, _a: Variadic<Value>| {
        if ui.negotiation.borrow().target.is_none() {
            return Ok(Value::Nil);
        }
        let t = lua.create_table()?;
        t.set("OffersAndDemands", lua.create_table()?)?;
        t.set("Unilaterals", lua.create_table()?)?;
        Ok(Value::Table(t))
    });

    // BuildOfferAndDemandStrings() (`PushNegotiationOfferAndDemandStringsFromScript` 0x009B48B0,
    // CONFIRMED shape): nothing while the counterpart (+0xAC) is unset, else THREE values: the
    // offers list, the demands list (`{Action, Text, Regions}` rows; "Offers" / "Demands" are only
    // the names of the two Lua references, not keys) and the diplomacy string
    // `offer_or_demand_regions` (InitialiseNegotiation reads `offers, demands, regions_text =` and
    // hands each row to AddToOffersDemandsList). `Regions` is set for the region-transfer action
    // (action id 6). INFERRED: `Action` is the host's action name (the exe gives the action
    // record) and rows carry no `Text`; the host never builds a row yet (see [`NegotiationItem`]).
    f!("BuildOfferAndDemandStrings", |lua, inner, ui, _a: Variadic<Value>| {
        let state = ui.negotiation.borrow();
        if state.target.is_none() {
            return Ok(mlua::MultiValue::new());
        }
        let mut out = Vec::new();
        for items in [&state.offers, &state.demands] {
            let list = lua.create_table()?;
            for (i, DealRow { item, .. }) in items.iter().enumerate() {
                let row = lua.create_table()?;
                row.set("Action", negotiation_action_name(item))?;
                if let NegotiationItem::Regions(keys) = item {
                    let regions = lua.create_table()?;
                    for (j, r) in keys.iter().enumerate() {
                        regions.set(j + 1, r.as_str())?;
                    }
                    row.set("Regions", regions)?;
                }
                list.set(i + 1, row)?;
            }
            out.push(Value::Table(list));
        }
        // The exe reads `diplomacy_strings` row `offer_or_demand_regions` (CONFIRMED key); its
        // text is the loc entry `diplomacy_strings_string_<key>` (the table's localised column,
        // present in the install: "Give region:"). A missing entry is logged once (the exe logs a
        // missing row too) and gives an empty text.
        const REGIONS_KEY: &str = "diplomacy_strings_string_offer_or_demand_regions";
        let regions_text = loc(&inner, REGIONS_KEY).unwrap_or_else(|| {
            inner.log_once(REGIONS_KEY, || format!("UNKNOWN loc key {REGIONS_KEY} (logged once)"));
            String::new()
        });
        out.push(Value::String(lua.create_string(regions_text)?));
        Ok(mlua::MultiValue::from_vec(out))
    });

    // TradeableRegions(): CONFIRMED shape (0x009C5770)
    // `{ Proposer = {{ Region = <region>, "CurrentlyOffered" = bool }, ...},
    //    Recipient = {{ Region = <region>, "CurrentlyDemanded" = bool }, ...} }`, one entry per
    // region of that faction; the wrapper returns nothing unless the campaign flag at +0xF9C and
    // the counterparty (+0xAC) are both set (CONFIRMED guard). Region lists: the faction's own
    // regions (INFERRED).
    f!("TradeableRegions", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        let Some((proposer, recipient)) = negotiation_factions(&ui) else { return Ok(Value::Nil) };
        for (key, faction, offered) in [("Proposer", proposer, true), ("Recipient", recipient, false)] {
            let list = lua.create_table()?;
            for (i, region) in m.world.regions.values().filter(|r| r.owner == faction).enumerate() {
                let row = lua.create_table()?;
                row.set("Region", region.key.as_str())?;
                row.set(if offered { "CurrentlyOffered" } else { "CurrentlyDemanded" }, region_in_deal(&ui, region.key.as_str(), offered))?;
                list.set(i + 1, row)?;
            }
            t.set(key, list)?;
        }
        Ok(Value::Table(t))
    });

    // TradeableTechnologies(): CONFIRMED shape (0x009C5AA0)
    // `{ Proposer = {rows}, Recipient = {rows} }`; each row carries the literal key "FactionKey"
    // and the numeric "tech_status" (the wrapper pushes `FUN_0044de40("tech_status", 0)`); the
    // rows are built by the card helper 0x009ABB50, which also sets "BuildingLevel" and the icon
    // path `Data/UI/Campaign UI/Technologies/%S.tga`. Every technology of the faction is listed
    // with its real state (0 researched / 2 available / 4 not yet available, CONFIRMED in
    // `details.rs`), so the script can filter: the tradeable set (which side may give what) is
    // UNKNOWN. The two remaining row keys are string constants at 0x009C5B9B / 0x009C5BC0 and are
    // left out rather than invented.
    f!("TradeableTechnologies", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        let Some((proposer, recipient)) = negotiation_factions(&ui) else { return Ok(Value::Nil) };
        for (key, faction) in [("Proposer", proposer), ("Recipient", recipient)] {
            let list = lua.create_table()?;
            let mut i = 1;
            if let Some(details) = m.world.faction_details.get(&faction) {
                for (tech, status) in &details.technologies {
                    let row = lua.create_table()?;
                    row.set("FactionKey", tech.as_str())?;
                    row.set("tech_status", *status as i64)?;
                    list.set(i, row)?;
                    i += 1;
                }
            }
            t.set(key, list)?;
        }
        Ok(Value::Table(t))
    });

    // FactionListsForStanceDeclarations(): CONFIRMED shape (0x009BAB90) `{ offered = {...},
    // demanded = {...} }` (literal keys), filtered by the pending declaration: the wrapper
    // compares the negotiation's pending action against the CONFIRMED names "request_join_war",
    // "break_trade", "break_alliance" (INFERRED that each splits the factions by stance: at war
    // with the proposer, or allied to it).
    f!("FactionListsForStanceDeclarations", |lua, inner, ui, _a: Variadic<Value>| {
        let t = lua.create_table()?;
        let m = ui.model();
        let Some((proposer, recipient)) = negotiation_factions(&ui) else { return Ok(Value::Nil) };
        let offered = lua.create_table()?;
        let demanded = lua.create_table()?;
        let mut io = 1;
        let mut id = 1;
        for faction in m.world.factions.values() {
            if faction.id == proposer || faction.id == recipient || !m.in_the_game(faction.id) {
                continue;
            }
            let at_war = m.world.stance(proposer, faction.id) == ntw_sim::campaign::Stance::War;
            let row = lua.create_table()?;
            row.set("FactionKey", faction.key.as_str())?;
            row.set("Name", faction.key.as_str())?;
            if at_war {
                offered.set(io, row)?;
                io += 1;
            } else {
                demanded.set(id, row)?;
                id += 1;
            }
        }
        t.set("offered", offered)?;
        t.set("demanded", demanded)?;
        Ok(Value::Table(t))
    });

    // MaxPlayerPaymentAllowed() / MaxOppositionPaymentAllowed(): one number out, always: a
    // faction's treasury (CONFIRMED, UI_FIDELITY.md §4.6 "Payment caps"). Both push
    // `GetFactionEconomyTreasury` (0x00BCAFE0, the int at the faction's economy +0xAC +0x3F4: the
    // same value the HUD's "funds" shows, 0x009C1AD0, and construction affordability compares)
    // through `PushScriptIntegerAsNumber` (0x01056530). MaxPlayer (0x009BE720) asks it for the
    // local player's faction (0x009BF110) with no negotiation guard, so it answers after `End()`
    // too. MaxOpposition (0x009BE6B0) for the side of the counterpart's negotiation that is not the
    // local player (proposer +0x18 unless that is the player, then recipient +0x1C), read through
    // +0xAC with no null check: the panel only asks with a counterpart (its payments subpopup).
    // The exe has no answer without one (a null read); the host answers 0 and logs it once.
    f!("MaxPlayerPaymentAllowed", |_l, inner, ui, _a: Variadic<Value>| {
        let human = ui.model().faction_by_key(&ui.link.human).map(|f| f.id);
        Ok(payment_cap(&inner, &ui, human, "MaxPlayerPaymentAllowed"))
    });
    f!("MaxOppositionPaymentAllowed", |_l, inner, ui, _a: Variadic<Value>| {
        let other = negotiation_factions(&ui).map(|(proposer, recipient)| {
            if ui.negotiation.borrow().player_proposed(&ui.link.human) { recipient } else { proposer }
        });
        Ok(payment_cap(&inner, &ui, other, "MaxOppositionPaymentAllowed"))
    });

    // ProposerId(): CONFIRMED one value out, the proposer's faction (the native wraps the faction
    // userdata, FUN_008E5060; we have no userdata, so the faction key string is returned instead —
    // INFERRED). Nothing without a counterpart (CONFIRMED guard).
    f!("ProposerId", |_l, inner, ui, _a: Variadic<Value>| {
        Ok(negotiation_factions(&ui).and(ui.negotiation.borrow().proposer.clone()))
    });

    // The deal entry points. Each wrapper reads no Lua argument and, while the counterpart
    // (+0xAC) is set, queues one campaign command (UI_FIDELITY.md §4.6, CONFIRMED executors), each
    // acting only while the campaign negotiation exists (+0xF9C); all take the one guard
    // [`negotiation_factions`]: `AcceptOffer` → CCQ_DIPLOMACY_ACCEPT_DEAL (0x00932E40 → 0x00C114B0:
    // applies the deal, result +0x28 = 1; the deal stays until END), `DeclineOffer` →
    // CCQ_DIPLOMACY_DECLINE_DEAL (0x00932F00 → 0x00C1F210: result = 2, the reply goes to the
    // proposer's panel, which is not the decliner's), `Cancel` → CCQ_DIPLOMACY_CLEAR_NEGOTIATION
    // (0x00932EC0: the deal emptied, the panel re-initialised over hub +0x348), `End` →
    // CCQ_DIPLOMACY_END_NEGOTIATION (see [`NegotiationState::end`]). None but `End` ends the
    // negotiation. Accepting applies the deal at most once ([`accept_deal`]).
    // PLACEHOLDER: `Propose` / `ProposeDeal` (0x009BF3C0 / 0x009BFCF0, CCQ_DIPLOMACY_PROPOSE_DEAL →
    // 0x00933690 → 0x00C49BE0) hand the deal to the AI's evaluation, which the model does not
    // have; here the AI accepts at once (no reply).
    f!("Propose", |_l, inner, ui, _a: Variadic<Value>| { accept_deal(&ui); Ok(()) });
    f!("ProposeDeal", |_l, inner, ui, _a: Variadic<Value>| { accept_deal(&ui); Ok(()) });
    f!("AcceptOffer", |_l, inner, ui, _a: Variadic<Value>| { accept_deal(&ui); Ok(()) });
    f!("DeclineOffer", |_l, inner, ui, _a: Variadic<Value>| {
        if negotiation_factions(&ui).is_some() {
            ui.negotiation.borrow_mut().status = NegotiationStatus::Declined;
        }
        Ok(())
    });
    // Cancel: the CLEAR executor (0x00932EC0) writes no result, so `Finished()` keeps its answer
    // (CONFIRMED for the executor; INFERRED for the per-item callbacks it runs, item vtable +0x2C
    // and 0x00C5C730 → vtable +0x50, not traced).
    f!("Cancel", |lua, inner, ui, _a: Variadic<Value>| {
        if negotiation_factions(&ui).is_none() {
            return Ok(());
        }
        let cleared = {
            let mut state = ui.negotiation.borrow_mut();
            state.clear_deal();
            state.context.map(|context| (context, state.player_proposed(&ui.link.human)))
        };
        if let Some((context, player_proposed)) = cleared {
            post_negotiation_cleared(lua, &inner, context, player_proposed)?;
        }
        Ok(())
    });
    f!("End", |_l, inner, ui, _a: Variadic<Value>| {
        end_negotiation(&inner, &ui);
        Ok(())
    });
    // RemoveAction (0x009C0F70 → 0x00C5C780): takes an action out of the deal. PROVISIONAL: no
    // deal row is built yet (see [`NegotiationItem`]), so there is nothing to remove.
    f!("RemoveAction", |_l, inner, ui, _a: Variadic<Value>| Ok(()));
    // CanPropose(): the panel asks before enabling SendOffer. UNKNOWN shape; INFERRED rule: a
    // deal with at least one item on either side.
    f!("CanPropose", |_l, inner, ui, _a: Variadic<Value>| {
        let state = ui.negotiation.borrow();
        Ok(!state.offers.is_empty() || !state.demands.is_empty())
    });
    // CanThreaten(): UNKNOWN shape; INFERRED: any counterparty not already at war.
    f!("CanThreaten", |_l, inner, ui, _a: Variadic<Value>| {
        let m = ui.model();
        Ok(match negotiation_factions(&ui) {
            Some((a, b)) => m.world.stance(a, b) != ntw_sim::campaign::Stance::War,
            None => false,
        })
    });
    f!("PrepareCounterOffer", |_l, inner, ui, _a: Variadic<Value>| {
        // UNKNOWN shape; the AI's counter-offer is not modelled (no negotiation evaluation).
        Ok(false)
    });
    // Finished() (0x009BB8B0, CONFIRMED): nothing without a counterpart, else whether the campaign
    // negotiation has a result (+0x28 != 0: accepted or declined).
    f!("Finished", |_l, inner, ui, _a: Variadic<Value>| {
        Ok(negotiation_factions(&ui).map(|_| ui.negotiation.borrow().status != NegotiationStatus::Open))
    });
    // IsNegotiation() is not called from any shipped script (`worker3/lua_api.txt` has no
    // `negotiation:IsNegotiation`); the method exists in the exe (0x009BD7C0). Registered so a
    // script that asks gets the host's answer rather than a nil call error (UNKNOWN shape:
    // whether a campaign negotiation is open).
    f!("IsNegotiation", |_l, inner, ui, _a: Variadic<Value>| Ok(ui.negotiation.borrow().open));

    Ok(())
}

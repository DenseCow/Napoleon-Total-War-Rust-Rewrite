//! The human side of a call to war (CAMPAIGN_FIDELITY.md "Allies called into a war", UI_FIDELITY.md
//! "The ally_attacked message"): when a declaration of war asks the human to join as an ally, the
//! model keeps the call waiting (`CampaignModel::pending_ally_war_calls`) and the original posts the
//! `ally_attacked` event message (type 0xD, `0x009CA900`). The message is the root script's
//! `ShowMessage(details)` (the details converter `0x00A22B30`), which hands it to the shipped
//! `message_handler`: its icon opens by itself (`AutoShow`) into the `event_message` panel, whose
//! `ally_attacked` layout shows the two sides and the buttons. The buttons call
//! `CampaignUI.SideWithAlly(key)` / `CampaignUI.DeclineSidingWithAlly()`, which queue
//! `CampaignCommand::RespondToAllyWarCall`; the model decides what the answer does.

use super::*;
use ntw_sim::campaign::message_events::{ALLY_ATTACKED, MessageEventRow, message_text};
use ntw_sim::campaign::treaties::AllyWarCall;

/// The lists the message gives each side (`0x00A08B90` passes 0x1A to `0x009B2690`: allies,
/// protectorates and protector of, CONFIRMED by disassembly).
const SIDE_LISTS: u8 = 0x1A;

/// Which waiting call the message was shown for: the call's three factions and the depth of the
/// stack (a declaration resumed after an answer pushes its next call over the same depth).
pub(super) type ShownCall = (FactionId, FactionId, FactionId, usize);

pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // SideWithAlly(key): join the war on the side of the faction `key` (`0x009F6DB0`, CONFIRMED by
    // disassembly: it reads one faction key, looks the faction up and queues
    // CCQ_RESPOND_TO_ALLY_WAR_INVITATION with that side and accept = 1; the panel's assist buttons
    // pass `Data.Aggressor.Key` or `Data.Defender.Key`). A key the registry does not hold sends no
    // side, which the model refuses (logged once).
    f!("SideWithAlly", |_l, inner, ui, key: Option<String>| {
        let side = key.as_deref().and_then(|k| ui.model().registry_faction(k).map(|f| f.id));
        if side.is_none() {
            inner.log_once_for("SideWithAlly faction", key.as_deref().unwrap_or(""), || {
                format!("UNKNOWN CampaignUI.SideWithAlly({key:?}): no such faction; the answer names no side (logged once per key)")
            });
        }
        ui.push(CampaignRequest::Command(CampaignCommand::RespondToAllyWarCall { side, accept: true }));
        Ok(())
    });
    // DeclineSidingWithAlly([key]): refuse (`0x009E21D0`, CONFIRMED by disassembly: with exactly one
    // argument it reads a faction key as the side, otherwise none, and queues the command with
    // accept = 0; event_message.lua's decline button passes the inviting side's key when one side
    // invited and none when both did, and closing the panel unanswered passes none). The model
    // ignores the side of a refusal.
    f!("DeclineSidingWithAlly", |_l, inner, ui, args: Variadic<Value>| {
        let side = match args.as_slice() {
            [Value::String(k)] => ui.model().registry_faction(&k.to_string_lossy()).map(|f| f.id),
            _ => None,
        };
        ui.push(CampaignRequest::Command(CampaignCommand::RespondToAllyWarCall { side, accept: false }));
        Ok(())
    });
    Ok(())
}

impl UiScriptHost {
    /// The `ally_attacked` message: while the top waiting call to war asks the human
    /// (`CampaignModel::pending_ally_war_calls`, answered top first), the root's
    /// `ShowMessage(details)` is called once for it, as the exe's converter `0x00A22B30` does for the
    /// message `0x009CA900` posts (CONFIRMED by disassembly; the converter shows a message only to
    /// the local faction, `0x00898B20`). `details` (CONFIRMED keys, in the exe's order):
    /// `ScreenWidth` / `ScreenHeight` (the UI root's size, `0x01037880` / `0x0102ACF0`), `Event`,
    /// `Title`, `Text`, `Image`, `Layout`, `RequiresResponse`, `Icon`, `AutoShow` (the
    /// `message_event_strings` row of the human's culture, [`CampaignModel::message_event_row`];
    /// the title and text are its loc strings, the text through [`message_text`], the image
    /// [`CampaignModel::message_picture_path`]),
    /// and `Data` ([`ally_attacked_data`]). The title gets no "<x>: " prefix (the message's own
    /// title part, +0x2C, is empty for this message). No row: no message (the exe's queuer
    /// `0x009D3250` posts nothing), logged once. The call keeps waiting until the panel answers it:
    /// the turn does not move on meanwhile (`CampaignModel::step`). Returns true when the message
    /// was shown.
    pub fn campaign_ally_war_call(&self) -> bool {
        let Some(ui) = self.campaign_ui() else { return false };
        // Asked on every campaign change (the HUD update runs it when the model's generation moves):
        // nothing is cloned until a new call is to be shown.
        let call = {
            let m = ui.model();
            let top = m.pending_ally_war_calls.last().filter(|c| m.world.factions.get(&c.ally).is_some_and(|f| f.key == ui.link.human));
            let Some(c) = top else {
                ui.ally_call_shown.set(None);
                return false;
            };
            let shown = (c.declarer, c.target, c.ally, m.pending_ally_war_calls.len());
            if ui.ally_call_shown.get() == Some(shown) {
                return false;
            }
            (c.clone(), shown)
        };
        let (call, shown) = call;
        // Shown once per call, whatever happens below: a failure is logged once, not on every update.
        ui.ally_call_shown.set(Some(shown));
        let row = ui.model().message_event_row(ALLY_ATTACKED, call.ally).cloned();
        let Some(row) = row else {
            self.inner().log_once("ally_attacked row", || {
                format!("UNKNOWN message_event_strings has no {ALLY_ATTACKED:?} row for {}'s culture: no message (logged once)", ui.link.human)
            });
            return false;
        };
        let lua = self.lua();
        let inner = self.inner();
        let failure = match ally_attacked_details(lua, inner, &ui, &row, &call) {
            Ok(details) => match agents::try_call_root_global(lua, inner, "ShowMessage", details) {
                Ok(true) => return true,
                Ok(false) => "the root defines no ShowMessage".to_owned(),
                Err(e) => format!("ShowMessage failed: {e}"),
            },
            Err(e) => format!("building it failed: {e}"),
        };
        log(inner, format!("ERROR: the ally_attacked message was not shown: {failure}"));
        false
    }
}

/// The `details` table of [`UiScriptHost::campaign_ally_war_call`].
fn ally_attacked_details(lua: &Lua, inner: &Inner, ui: &CampaignUi, row: &MessageEventRow, call: &AllyWarCall) -> mlua::Result<Table> {
    let (w, h) = inner.screen();
    let t = lua.create_table()?;
    t.set("ScreenWidth", w)?;
    t.set("ScreenHeight", h)?;
    t.set("Event", row.event.as_str())?;
    t.set("Title", loc_required(inner, &row.title_key()))?;
    t.set("Text", message_text(&loc_required(inner, &row.text_key())))?;
    t.set("Image", ui.model().message_picture_path(row, |p| inner.source.exists(p)))?;
    t.set("Layout", row.layout.as_str())?;
    t.set("RequiresResponse", row.requires_response)?;
    t.set("Icon", row.icon_path())?;
    t.set("AutoShow", row.auto_show)?;
    t.set("Data", ally_attacked_data(lua, inner, ui, call)?)?;
    Ok(t)
}

/// The message's `Data` (`0x00A08B90`, CONFIRMED by disassembly): `Aggressor` (the declarer, message
/// +8) and `Defender` (the target, +0xC), each the side's [`diplomacy_lists`] for [`SIDE_LISTS`] with
/// its `Name` (on-screen name, `0x008DA780`) and `Key`; `LocalFaction` (the ally's key, +4);
/// `AlliedWithAggressor` / `AlliedWithDefender` (the ally's own record of each side is allied,
/// patron or protectorate, `0x00B0CDF0`: [`CampaignModel::allied_like_with`]); `InvitedByAggressor`
/// / `InvitedByDefender` (the message's +0x10 / +0x11: [`AllyWarCall::invited_by_declarer`] /
/// [`AllyWarCall::invited_by_target`]).
fn ally_attacked_data(lua: &Lua, inner: &Inner, ui: &CampaignUi, call: &AllyWarCall) -> mlua::Result<Table> {
    let key = |f: FactionId| ui.model().world.factions.get(&f).map(|f| f.key.clone()).unwrap_or_default();
    let side = |f: FactionId| -> mlua::Result<Table> {
        let k = key(f);
        let t = lua.create_table()?;
        diplomacy_lists(lua, inner, ui, &k, SIDE_LISTS, &t)?;
        t.set("Name", faction_name(inner, &ui.link.db, &k))?;
        t.set("Key", k)?;
        Ok(t)
    };
    let d = lua.create_table()?;
    d.set("Aggressor", side(call.declarer)?)?;
    d.set("Defender", side(call.target)?)?;
    d.set("LocalFaction", key(call.ally))?;
    let m = ui.model();
    d.set("AlliedWithAggressor", m.allied_like_with(call.ally, call.declarer))?;
    d.set("AlliedWithDefender", m.allied_like_with(call.ally, call.target))?;
    d.set("InvitedByAggressor", call.invited_by_declarer())?;
    d.set("InvitedByDefender", call.invited_by_target())?;
    Ok(d)
}

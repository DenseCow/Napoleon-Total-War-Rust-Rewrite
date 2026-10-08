//! The battle HUD's engine side: the `BattleUI.*` functions and the `UICardManager` class the
//! original battle UI scripts (`ui\battle ui\root.luac`, `layout_scripts\*`, `land_battle_orders`,
//! `deployment_end`, `in_battle_results_popup`, `template.battleunitcard`) call.
//!
//! Our own code: the game writes a [`BattleHudFacts`] snapshot every frame ([`set_facts`]) and
//! drains the scripts' [`BattleUiRequest`]s ([`take_requests`]). Most functions are Lua in
//! `battle_prelude.lua` reading the snapshot table `__battle`; requests go into
//! `__battle_requests`. Function names and argument shapes are CONFIRMED from the scripts and the
//! exe's binding table (`analysis/worker1/script_bindings.tsv`, registrar `0x59EB40`); return
//! values the scripts read are INFERRED from how they use them (see
//! `analysis/battle/BATTLE_FLOW.md` §3). Anything else stays an UNKNOWN logging stub.

use mlua::{Function, Lua, Table, Value};

use super::UiScriptHost;
use crate::ScriptSource;

const BATTLE_PRELUDE: &str = include_str!("battle_prelude.lua");

/// Battle phase as the HUD sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudPhase {
    /// Deployment (`BattleUI.HasEnteredDeployment()` true, `IsConflict()` false).
    #[default]
    Deployment,
    /// The battle runs (`IsConflict()` true).
    Conflict,
    /// Decided; the results are shown.
    Finished,
}

/// One of the player's units, as the unit cards show it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudUnit {
    /// Model unit id (the card's id is `"card_<id>"`).
    pub id: u32,
    /// `units` key.
    pub key: String,
    /// Display name (the results screen's unit statistics).
    pub name: String,
    /// Enemy men this unit killed.
    pub kills: u32,
    /// Card portrait without extension, e.g. `data/ui/units/icons/<key>` (the card script appends `.tga`).
    pub portrait: String,
    /// Men now and at the start.
    pub men: u32,
    pub max_men: u32,
    /// Guns (artillery) now and at the start.
    pub guns: u32,
    pub max_guns: u32,
    pub is_artillery: bool,
    /// Has a missile weapon with ammunition left.
    pub has_ammo: bool,
    /// Ammunition left, 0..100.
    pub ammo_percent: f32,
    /// Experience (chevrons).
    pub experience: u32,
    pub wavering: bool,
    pub routing: bool,
    pub walking: bool,
    pub running: bool,
    pub firing: bool,
    pub melee: bool,
    pub under_fire: bool,
    /// Selected by the player.
    pub selected: bool,
    /// Unit category (`infantry`, `cavalry`, `artillery`, ...), for the select-all buttons.
    pub category: String,
    /// Fire at will on.
    pub fire_at_will: bool,
}

/// One side's line in the results.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HudSideResult {
    /// Display name (faction screen name).
    pub name: String,
    /// Faction key.
    pub faction: String,
    /// The faction's flag folder (`data/ui/flags/...`), for the results.
    pub flag: String,
    /// Men at the start, alive now, killed.
    pub men_start: u32,
    pub men_alive: u32,
    pub kills: u32,
    /// Units at the start and units left fighting.
    pub units_start: u32,
    pub units_left: u32,
}

/// What the game tells the HUD, rewritten every frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BattleHudFacts {
    pub phase: HudPhase,
    /// Battle time in seconds.
    pub elapsed_s: f32,
    /// Time limit in seconds (0 = none).
    pub total_s: f32,
    /// Current speed multiplier (0 = paused).
    pub speed: f32,
    /// The player's units (cards).
    pub units: Vec<HudUnit>,
    /// Is this a naval battle.
    pub naval: bool,
    /// The player's side wins (Some(true)), loses (Some(false)) or nothing decided yet (None).
    pub player_won: Option<bool>,
    /// Results per side, player first.
    pub results: Vec<HudSideResult>,
    /// Name of the battle (loc), for the results.
    pub battle_name: String,
    /// The player's faction key and its flag folder (`data/ui/flags/<x>`).
    pub player_faction: String,
    pub player_flag: String,
    /// Kill-o-meter: 0..1 share of the player's side in the balance of power (INFERRED).
    pub balance: f32,
}

/// Something the battle UI scripts asked the game to do.
#[derive(Debug, Clone, PartialEq)]
pub enum BattleUiRequest {
    /// Speed buttons: `BattleUI.Pause/Slow/Play/Fwd/Ffwd()`, as a multiplier (0 = pause).
    SetSpeed(f32),
    /// `BattleUI.CycleBattleSpeed()`.
    CycleSpeed,
    /// `BattleUI.InformOfDeploymentFinished()` (the deployment panel's Start Battle).
    DeploymentFinished,
    /// Card clicks: select these unit ids (replacing the selection unless `add`).
    Select { ids: Vec<u32>, add: bool },
    /// An order button for the current selection, by its binding name
    /// (`Current_Selection_Halt`, `Fire_At_Will`, `Current_Selection_Runs`, ...) and argument.
    Order { name: String, arg: Option<bool> },
    /// `BattleUI.ExitBattle()`: leave the battle.
    ExitBattle,
    /// `PostBattleDismissEndBattle()` (the victory options' End Battle): show the summary.
    EndBattle,
    /// `PostBattleDismissContinueBattle()`: keep playing after the result.
    ContinueBattle,
    /// `InformOfBattleSummaryDismiss()`.
    SummaryDismissed,
    /// `ZoomToUnit(unit)` / `CameraZoomToSelection()`.
    ZoomTo(Option<u32>),
}

/// Installs the battle UI's engine side into a UI host. Call before loading
/// `data/ui/battle ui/layout`.
pub fn install(host: &UiScriptHost, source: ScriptSource) -> mlua::Result<()> {
    host.set_pages_fill_screen(false);
    let lua = host.lua();
    lua.globals().set("__battle", lua.create_table()?)?;
    lua.globals().set("__battle_requests", lua.create_table()?)?;
    // INFERRED: the engine's `loadfile` reads through the VFS (CoreUtils.NamespaceFile calls it on
    // `package.path` templates; the battle root sets `data/ui/battle ui/?.lua` there).
    lua.globals().set(
        "loadfile",
        lua.create_function(move |lua, path: String| -> mlua::Result<(Value, Value)> {
            let Some(file) = source.find(&path) else {
                return Ok((Value::Nil, Value::String(lua.create_string(format!("cannot open {path}"))?)));
            };
            let (bytes, mode) = if crate::luac::is_bytecode(&file.bytes) {
                (crate::luac::convert_chunk(&file.bytes).map_err(|e| mlua::Error::runtime(e.to_string()))?, mlua::chunk::ChunkMode::Binary)
            } else {
                (file.bytes, mlua::chunk::ChunkMode::Text)
            };
            let f = lua.load(&bytes[..]).set_name(file.chunk_name).set_mode(mode).into_function()?;
            Ok((Value::Function(f), Value::Nil))
        })?,
    )?;
    lua.load(BATTLE_PRELUDE).set_name("@ntw_battle_prelude.lua").exec()
}

/// Loads `data/ui/battle ui/layout` as the host's root and runs its InitStates. The root's runs
/// last because every new component is initialised children first (`0x01034210`, CONFIRMED):
/// root.lua's InitState (battle_HUD_visibility_manager) needs the "orders" panel that
/// battle_hud.lua's InitState creates. Call after [`install`] and a first [`set_facts`].
pub fn load_hud(host: &UiScriptHost) -> Result<super::NodeId, String> {
    host.load_root_layout("data/ui/battle ui/layout")
}

/// Writes the snapshot into `__battle` (read by the `BattleUI.*` functions and card updates).
pub fn set_facts(host: &UiScriptHost, f: &BattleHudFacts) -> mlua::Result<()> {
    let lua = host.lua();
    let t: Table = lua.globals().get("__battle")?;
    t.set("phase", match f.phase {
        HudPhase::Deployment => "deployment",
        HudPhase::Conflict => "conflict",
        HudPhase::Finished => "finished",
    })?;
    t.set("elapsed", f.elapsed_s)?;
    t.set("total", f.total_s)?;
    t.set("speed", f.speed)?;
    t.set("naval", f.naval)?;
    t.set("battle_name", f.battle_name.as_str())?;
    t.set("player_faction", f.player_faction.as_str())?;
    t.set("player_flag", f.player_flag.as_str())?;
    t.set("balance", f.balance)?;
    t.set("player_won", f.player_won)?;
    let units = lua.create_table()?;
    for (i, u) in f.units.iter().enumerate() {
        units.set(i + 1, unit_table(lua, u)?)?;
    }
    t.set("units", units)?;
    let results = lua.create_table()?;
    for (i, r) in f.results.iter().enumerate() {
        let e = lua.create_table()?;
        e.set("name", r.name.as_str())?;
        e.set("faction", r.faction.as_str())?;
        e.set("flag", r.flag.as_str())?;
        e.set("men_start", r.men_start)?;
        e.set("men_alive", r.men_alive)?;
        e.set("kills", r.kills)?;
        e.set("units_start", r.units_start)?;
        e.set("units_left", r.units_left)?;
        results.set(i + 1, e)?;
    }
    t.set("results", results)
}

fn unit_table(lua: &Lua, u: &HudUnit) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", u.id)?;
    t.set("key", u.key.as_str())?;
    t.set("name", u.name.as_str())?;
    t.set("kills", u.kills)?;
    t.set("portrait", u.portrait.as_str())?;
    t.set("men", u.men)?;
    t.set("max_men", u.max_men)?;
    t.set("guns", u.guns)?;
    t.set("max_guns", u.max_guns)?;
    t.set("is_artillery", u.is_artillery)?;
    t.set("has_ammo", u.has_ammo)?;
    t.set("ammo_percent", u.ammo_percent)?;
    t.set("experience", u.experience)?;
    t.set("wavering", u.wavering)?;
    t.set("routing", u.routing)?;
    t.set("walking", u.walking)?;
    t.set("running", u.running)?;
    t.set("firing", u.firing)?;
    t.set("melee", u.melee)?;
    t.set("under_fire", u.under_fire)?;
    t.set("selected", u.selected)?;
    t.set("category", u.category.as_str())?;
    t.set("fire_at_will", u.fire_at_will)?;
    Ok(t)
}

/// Calls a global function of the HUD (any component environment that defines it, e.g. the root's
/// `ShowDeploymentPopup`, the cards panel's `CreateCards`) with Lua arguments built by `args`.
/// Returns false when no component defines it.
pub fn call_global<'a>(host: &'a UiScriptHost, name: &str, args: impl FnOnce(&'a Lua) -> mlua::Result<mlua::MultiValue>) -> mlua::Result<bool> {
    let lua = host.lua();
    let f: Function = lua.globals().get("__ntw_battle_call_global")?;
    let mut a = args(lua)?;
    a.push_front(Value::String(lua.create_string(name)?));
    f.call::<bool>(a)
}

/// Drains the scripts' requests.
pub fn take_requests(host: &UiScriptHost) -> mlua::Result<Vec<BattleUiRequest>> {
    let lua = host.lua();
    let list: Table = lua.globals().get("__battle_requests")?;
    let mut out = Vec::new();
    for r in list.sequence_values::<Table>() {
        let r = r?;
        let kind: String = r.get("kind")?;
        out.push(match kind.as_str() {
            "speed" => BattleUiRequest::SetSpeed(r.get("value")?),
            "cycle_speed" => BattleUiRequest::CycleSpeed,
            "deployment_finished" => BattleUiRequest::DeploymentFinished,
            "select" => {
                let ids: Table = r.get("ids")?;
                BattleUiRequest::Select { ids: ids.sequence_values::<u32>().collect::<mlua::Result<_>>()?, add: r.get::<Option<bool>>("add")?.unwrap_or(false) }
            }
            "order" => BattleUiRequest::Order { name: r.get("name")?, arg: r.get("arg")? },
            "exit" => BattleUiRequest::ExitBattle,
            "end_battle" => BattleUiRequest::EndBattle,
            "continue" => BattleUiRequest::ContinueBattle,
            "summary_dismissed" => BattleUiRequest::SummaryDismissed,
            "zoom" => BattleUiRequest::ZoomTo(r.get("id")?),
            _ => continue,
        });
    }
    lua.globals().set("__battle_requests", lua.create_table()?)?;
    Ok(out)
}

/// Calls one of the prelude's engine steps (a global Lua function without arguments).
fn step(host: &UiScriptHost, name: &str) -> mlua::Result<()> {
    let f: Function = host.lua().globals().get(name)?;
    f.call::<()>(())
}

/// Creates the player's unit cards in the cards panel (after [`set_facts`]).
pub fn create_cards(host: &UiScriptHost) -> mlua::Result<()> {
    step(host, "__ntw_battle_create_cards")
}

/// Updates every unit card from the current facts (each frame).
pub fn update_cards(host: &UiScriptHost) -> mlua::Result<()> {
    step(host, "__ntw_battle_update_cards")
}

/// Calls a root-script function the engine calls at a battle event, without arguments
/// (`ShowDeploymentPopup`, `SetDeploymentPopupAsDeploymentStart`, `ClearDeploymentPanels`,
/// `ShowSinglePlayerEndPhasePopup`, `ShowBattleSummaryPopup`, `ShowPostBattleResultsPopup`, ...).
/// Returns false when no component defines it.
pub fn call_event(host: &UiScriptHost, name: &str) -> mlua::Result<bool> {
    call_global(host, name, |_| Ok(mlua::MultiValue::new()))
}

/// Sets the order buttons' states from the current selection (each frame; only changes are sent).
pub fn update_orders(host: &UiScriptHost) -> mlua::Result<()> {
    step(host, "__ntw_battle_update_orders")
}

/// A completed left click on HUD component `node`: selects the unit of a card that was clicked
/// (`add` = shift held). Returns true if it was a card.
pub fn click(host: &UiScriptHost, node: super::NodeId, add: bool) -> mlua::Result<bool> {
    let f: Function = host.lua().globals().get("__ntw_battle_click")?;
    f.call::<bool>((Value::LightUserData(mlua::LightUserData((node + 1) as *mut std::ffi::c_void)), add))
}

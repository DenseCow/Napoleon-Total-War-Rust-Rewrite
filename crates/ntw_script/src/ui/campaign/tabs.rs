//! The review panel's tabs: which tabs a selection shows, and switching between them.

use super::*;

/// Review-panel tab `i` (1-based, in range, not the current tab) becomes the current tab, inside
/// the `ReviewPanelTabSelectionSet_1_Indexed` call that asked for it, as the exe's `0x00A20620`
/// does (CONFIRMED): clear the review panel, the old tab's state change to deselected (its handler
/// calls the root's `ReviewPanelTabInit(.., index, 1)`, `0x009C7D80`), then `i` made current and
/// its state change to selected (`.., 2`), which generates its panel ([`generate_current_tab`]).
/// A script these calls run may ask for another tab in turn: that change nests inside this one,
/// as in the exe, which has no limit (the caller runs this under the state-function stack guard,
/// so a runaway ends with Lua's "C stack overflow" error, never a crash). The script that asked
/// reads the new panel's geometry current, as every read is (`host.rs` `lay_out_if_stale`).
fn select_tab(lua: &Lua, inner: &Rc<Inner>, ui: &CampaignUi, i: usize) {
    let old = ui.current_tab.get();
    call_root_global(lua, inner, "ClearReviewPanel", ());
    let old_tab = ui.tabs.borrow().get(old.wrapping_sub(1)).copied();
    if let Some(tab) = old_tab {
        call_root_global(lua, inner, "ReviewPanelTabInit", (tab_title(inner, tab), old, TAB_UNSELECTED));
    }
    ui.current_tab.set(i);
    generate_current_tab(lua, inner, ui);
}

/// A review-panel tab's title (`random_localisation_strings_string_<key>`, empty when missing).
pub(super) fn tab_title(inner: &Inner, tab: Tab) -> String {
    loc(inner, &format!("random_localisation_strings_string_{}", tab.key())).unwrap_or_default()
}

/// The current review-panel tab's state change to selected: the root's
/// `ReviewPanelTabInit(title, index, 2)`, then the tab's `Generate*Panel(info)`.
pub(super) fn generate_current_tab(lua: &Lua, inner: &Rc<Inner>, ui: &CampaignUi) {
    let i = ui.current_tab.get();
    let Some(tab) = ui.tabs.borrow().get(i.wrapping_sub(1)).copied() else { return };
    call_root_global(lua, inner, "ReviewPanelTabInit", (tab_title(inner, tab), i, TAB_SELECTED));
    let info = match tab {
        Tab::Army | Tab::Navy => match selected_force(ui) {
            Some(f) => force_info(lua, inner, ui, f).ok(),
            None => None,
        },
        Tab::Construction => match ui.selection.get() {
            CampaignSelection::Settlement(r) => construction_info(lua, inner, ui, r, ConstructionPanel::Settlement).ok(),
            _ => None,
        },
        Tab::Recruitment => match ui.selection.get() {
            CampaignSelection::Settlement(r) => recruitment_info(lua, inner, ui, r, false).ok(),
            // An army's recruitment tab ([`tabs_for`]). The exe's info builder `0x009FE7B0` reads
            // the tab's own recruitment manager (through the commander, `+0x34` → `+0x124`); what
            // that manager holds is not traced. PLACEHOLDER: the panel opens with no cards, so
            // nothing can be recruited through it (BACKLOG §0 "army recruitment tab contents").
            CampaignSelection::Character(c) => empty_recruitment_info(lua, ui, c).ok(),
            _ => None,
        },
        // The naval recruitment tab sits on a naval character (0-G's trace, §4.3); its manager is
        // the port the admiral stands in, falling back to the faction's capital (both INFERRED --
        // which port a navy recruits at is UNKNOWN, and the model keeps one naval queue per
        // region, so `recruitment_points(region, true)` is that region's whole naval capacity). PROVISIONAL:
        // the naval tab's manager (`0x009FE7B0`) is untraced, like the land army tab's.
        Tab::NavalRecruitment => match naval_region(ui) {
            Some(r) => recruitment_info(lua, inner, ui, r, true).ok(),
            None => None,
        },
        Tab::Agents => match ui.selection.get() {
            CampaignSelection::Settlement(r) => agents_info(lua, inner, ui, r).ok(),
            _ => None,
        },
        Tab::Infrastructure => match ui.selection.get() {
            CampaignSelection::Settlement(r) => construction_info(lua, inner, ui, r, ConstructionPanel::Infrastructure).ok(),
            _ => None,
        },
        Tab::Fort => match ui.selection.get() {
            CampaignSelection::Fort(r) => fort_info(lua, inner, ui, r).ok(),
            _ => None,
        },
    };
    match info {
        Some(Value::Table(t)) => {
            call_root_global(lua, inner, tab.generator(), t);
        }
        _ => log(inner, format!("UNKNOWN review panel info for {}", tab.key())),
    }
}

/// The review-panel tabs for a selection, in the engine's order (INFERRED from `FUN_00985f40`
/// (character) and `FUN_0099a200` (settlement): construction, recruitment, ..., army, agents).
/// PROVISIONAL: only the tabs whose panels we fill are listed.
///
/// 0-E round N+2, from 0-G's read-only trace of `FUN_0099A200` (its own Ghidra copy, §4.3): the
/// settlement's tab list is `construction_tab`, `recruitment_tab` (only when
/// `recruitment_points(region, false) > 0`), **`infrastructure_tab`**, `army_tab` and `agents_tab`,
/// and `naval_recruitment_tab` is **never** among them -- it is placed on the character panel, whose
/// generator is `GenerateNavyPanel`. So the naval tab below goes on a naval character. Re-traced
/// 2026-10-07 (own Ghidra copy, `ConstructSettlementPanelTabs`): the infrastructure tab is added when
/// the settlement has a road slot (`+0x1C0`) and shows that road slot (`0x00A021B0`), not the walls.
pub(super) fn tabs_for(ui: &CampaignUi, sel: CampaignSelection) -> Vec<Tab> {
    let m = ui.model();
    let mut tabs = Vec::new();
    match sel {
        // A commanded force's tab set (own Ghidra copy, 2026-10-07; CONFIRMED structure): the map
        // entity's selection `0x009C2AF0` builds `0x009855B0` for an army -- `army_tab`, then
        // `recruitment_tab` when [`commander_recruits`], then `agents_tab` when the force carries
        // an agent (`0x008B77C0` over force `+0x6C/+0x70`) -- and `0x009990B0` for a navy:
        // `navy_tab`, `army_tab` when it carries an army, `naval_recruitment_tab` when
        // [`commander_recruits`], `agents_tab`. Each tab is created at index = tabs so far + 1, and
        // the root's `CreateReviewPanelTabAtPosition` places tab i at (i - 1) x its width
        // (`layout.root.lua:600-623`), so Army is left of Recruitment, wherever the army stands
        // (no position test in `0x009855B0`). The model carries no agents or troops aboard a
        // force, so those two optional tabs never appear (PROVISIONAL).
        CampaignSelection::Character(c) => {
            if let Some(f) = m.force_of(c).and_then(|f| m.world.forces.get(&f)) {
                let recruits = f.commander.is_some_and(|k| commander_recruits(&m, k));
                if f.is_navy {
                    tabs.push(Tab::Navy);
                    if recruits {
                        tabs.push(Tab::NavalRecruitment);
                    }
                } else {
                    tabs.push(Tab::Army);
                    if recruits {
                        tabs.push(Tab::Recruitment);
                    }
                }
            }
        }
        CampaignSelection::Settlement(r) => {
            // Recruitment when the region can recruit (FUN_0099A200 asks FUN_00B44DA0; INFERRED:
            // its recruitment points).
            tabs.push(Tab::Construction);
            if m.recruitment_points(r, false) > 0 {
                tabs.push(Tab::Recruitment);
            }
            // The infrastructure (road) tab, after recruitment as in `ConstructSettlementPanelTabs`
            // `0x0099A200`, which adds it when the settlement's road slot (`+0x1C0`) exists
            // (CONFIRMED: its test is the settlement's road slot pointer, not
            // `IsConstructionSlotShown` `0x00B7A0E0`, which only `0x00A021B0` asks, for the panel's
            // entry). Every `REGION_SLOT_MANAGER` of every shipped startpos holds a road slot
            // (CONFIRMED, see [`construction_info`]) and the model keeps no empty slot, so ours adds
            // the tab for every settlement (PROVISIONAL for a mod region without one). With nothing
            // to show the panel gets an empty `slots` table, as the exe's does.
            tabs.push(Tab::Infrastructure);
            if let Some(g) = m.world.regions.get(&r).and_then(|r| r.garrison)
                && m.world.forces.get(&g).is_some_and(|f| !f.units.is_empty())
            {
                tabs.push(Tab::Army);
            }
            // `agents_tab`, last (CONFIRMED in `FUN_0099A200`, added there with no visible test).
            // INFERRED condition: the settlement lists at least one agent (`listed_agents`). Its
            // panel is `ui/agents.luac`'s `GenerateAgentsPanel(info)`, see `agents_info`.
            drop(m);
            if !listed_agents(ui, r).is_empty() {
                tabs.push(Tab::Agents);
            }
            return tabs;
        }
        // A fort's own panel: `ConstructFortPanelTabs` `0x009988C0` (CONFIRMED structure) adds the
        // fort construction tab (`construction_tab`, generator `GenerateFortConstructionPanel`) first,
        // then the army / recruitment tabs when the fort has a garrison and the agents tab. The
        // model's fort has no garrison or agents of its own (it does not load them), so ours shows
        // the construction tab only (PROVISIONAL).
        CampaignSelection::Fort(r) => {
            if m.world.regions.contains_key(&r) {
                tabs.push(Tab::Fort);
            }
        }
        CampaignSelection::None => {}
    }
    tabs
}

/// Does a force's commander get a recruitment tab on his force's panel? The exe's `0x009D1CB0`
/// on the commander (own Ghidra copy, 2026-10-07): true when his agent record's per-culture row
/// (`0x009CBC40`: the hash in the agent record `+0x1AC`, keyed by the faction's culture,
/// returning the row's `+0x2C`) names a unit, or his agent type (record `+0x2C`) is 1, the
/// admiral. That row is `agent_culture_details` #3 (CHARACTERS_FIDELITY.md §8: `0x008E27D0`
/// reads the same hash for the general's unit), which only `General` rows fill (`european` /
/// `egy_european` → `Gen_Generals_Staff`, `middle_east` / `egy_middle_east` →
/// `Gen_Generals_Bodyguard`; none for `indian` / `tribal`). So: a general of a culture with a
/// general's unit, or an admiral; a colonel or a captain gets none (CONFIRMED: `0x009D1CB0` + user
/// check 2026-10-07, Henry Fox, colonel: Army tab only; admirals: `0x009D1CB0` + user
/// statement). The culture is the commander's faction's (INFERRED: `0x00A257C0` reads it through the character, falling back to
/// the human faction's).
fn commander_recruits(m: &CampaignModel, commander: CharacterId) -> bool {
    let Some(ch) = m.world.characters.get(&commander) else { return false };
    match ch.kind {
        CharacterKind::Admiral => true,
        CharacterKind::General => m.general_unit(ch.faction).is_some(),
        _ => false,
    }
}


pub(super) fn install(lua: &Lua, inner: &Rc<Inner>, ui: &Rc<CampaignUi>, t: &Table) -> mlua::Result<()> {
    macro_rules! f { ($($tt:tt)*) => { campaign_fn!(t, lua, inner, ui; $($tt)*) }; }

    // ReviewPanelTabSelectionSet_1_Indexed(i): the player clicked review-panel tab i (1-based).
    // The exe (handler `0x009F48E0` → `0x009C14B0` → `0x00A20620`, CONFIRMED) checks i against the
    // CURRENT selection's tab list and does nothing (returns 0, no message) for an index out of
    // range; in range it clears the review panel, tells the old tab it is deselected, makes i
    // current and generates it, all inside the call. Ours: the same, see [`select_tab`]. The exe
    // also refuses a tab whose state (+0x24, the state `0x009C7D80` stores through the tab's +4
    // sub-object) is 2 = selected, i.e. the tab already current: no rebuild (CONFIRMED); ours
    // refuses the current tab the same way. State 0 is refused too (meaning UNKNOWN; our tabs
    // never have it).
    //
    // While a selection change builds its tab list the exe holds no tab set (manager +0xEEC is
    // 0) and the request handler (`0x009C14B0`, ECX = [manager+0xEEC], no null check) passes
    // that NULL to `0x00A20620`, which reads +0x14 of it: an access violation that Lua's
    // setjmp/longjmp protection does not catch (CONFIRMED static; whether an outer SEH handler
    // does is UNKNOWN; no shipped script asks then). That covers the generators the build runs
    // too: the kept tab's opening (`0x009C97B0`) and `OpenFirstEnabledPanelTab` (`0x009DA2A0`)
    // run inside the tab set's ctor (`ConstructSettlementPanelTabs 0x0099A200`), which returns
    // before `HandleSettlementSelected` stores the set at +0xEEC, and the Lua handler
    // (`0x009F48E0`) hands the manager global straight to `0x009C14B0` (CONFIRMED, round 16).
    // Ours refuses the request and logs it once per HUD (round 15).
    f!("ReviewPanelTabSelectionSet_1_Indexed", |lua, inner, ui, i: Option<usize>| {
        if ui.building_tabs.get() {
            inner.log_once("tab request during a tab-list build", || {
                format!(
                    "ERROR ReviewPanelTabSelectionSet_1_Indexed({i:?}) while a selection change builds the tab list: \
                     the original reads a null tab set here (0x009C14B0 -> 0x00A20620); refused (logged once)"
                )
            });
            return Ok(());
        }
        if let Some(i) = i
            && i >= 1
            && i <= ui.tabs.borrow().len()
            && i != ui.current_tab.get()
        {
            state_function_guard(|| select_tab(lua, &inner, &ui, i))?;
        }
        Ok(())
    });
    // ReviewPanelInfo(): the current army/navy panel's info, for a soft refresh (Army.lua).
    f!("ReviewPanelInfo", |lua, inner, ui, _a: Variadic<Value>| {
        match selected_force(&ui) {
            Some(f) => force_info(lua, &inner, &ui, f),
            None => Ok(Value::Nil),
        }
    });
    Ok(())
}

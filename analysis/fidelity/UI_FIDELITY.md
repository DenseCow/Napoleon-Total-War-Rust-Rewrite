# UI fidelity (slot 0-E): campaign map, campaign UI and front end

Branch `work/fidelity-ui`. Everything here is driven by the original layouts and `.luac` scripts;
our code only supplies the engine functions they call. Tags: CONFIRMED / INFERRED / UNKNOWN for
findings, PROVISIONAL / PLACEHOLDER for stand-ins.

## Where I am

**Sandbox round 2, 2026-10-06 (0-E).** §8 below is the round's finding and it **corrects an earlier
claim in `CHARACTER_UI_HOOKS.md` / `HANDOFF.md`: `panel_manager` is NOT missing.** It is the shipped
Lua module `ui\panelmanager.luac`, which `ui\campaign ui\layout.root.luac` requires itself, so it
already runs in our host and already opens the `enlist_commander` panel. What was blocking the hire
and the agent buttons was two of our own bugs: a trait row with no `Effects`/`AttributeEffects`, and
`Component.Call("IsDragged")` answering `nil`, which silently killed every card's left click. Both
fixed, with the tests `enlist_commander_panel_opens_and_lists_a_candidate` and
`a_card_left_click_shows_the_agents_action_buttons`. The four `CampaignUI.Agent*` button calls are
still no-ops and are **0-G's**, not mine -- §8's last paragraph says why they were not wired here.

**State on main (0-E campaign UI ported from sandbox/main 90bfc1c, branch `work/sandbox-port-ui`).**
The sandbox notes below are kept as written; where they and main differ, this paragraph wins.
- **On main:** the demolish button (`CanDemolishBuilding` / `DemolishBuilding` on the model's
  `can_demolish`); the infrastructure tab (on main: `fort_info`; on `work/blocker-walls` the road panel, and the walls are the construction panel's last card) and the
  fort actions (`BuildFort`, `UpgradeFort`, `RepairFort`, `CancelUpgradeFort`, `CancelFortRepair`,
  `DemolishFort`, `FortDetails`, `FortEffects`), on the selected settlement or fort; the naval
  recruitment tab on a naval character; the army panel's `can_build_fort_status` /
  `build_fort_cost`; recruitment card prices through `economy::recruit_cost` (the
  experience-adjusted cost, rank 0); the construction actions taking the slot key from any argument
  position; `RetrieveDiplomaticStanceString`, `InviteAlliesIntoWar` (PLACEHOLDER: reports only),
  `IsCharacterPlayerControlled`; and the `negotiation` object with its 20 methods.
- **Fort selection (branch `work/campaign-fort-agents`).** `CampaignSelection::Fort(RegionId)` is a
  selection of its own, as the original's `campaign_fort` entity (`0x00965550`, `player_fort` /
  `non_player_fort`, event `FortSelected`) is. The app keeps it in `CampaignSim::selected_fort`, and
  `hud.rs` maps it both ways. The scripts get a fort address (`TAG_FORT`) in `SetSelectedEntity`. The
  fort panel and all fort actions resolve the fort's region, and `FortDetails` also takes a fort
  address. Tests: `a_fort_selection_opens_the_fort_panel_and_its_actions_address_its_region` (lib, no
  install: a made-up world and root script) and `fort_selection_opens_the_fort_panel` (install).
  PROVISIONAL:
  - the payload is a **region**. The region is the handle the whole chain carries (`play::MapPick`
    → `CampaignSim::selected_fort` → `CampaignSelection::Fort`), so **two forts in one region cannot
    be told apart**. The model's other fort is the region's `FORTIFICATION_SLOT` building, which
    stands at the settlement and has no id or position;
  - a fort shows only `infrastructure_tab`. The fort panel's tab builder was never traced. No
    garrison tab: the model's fort has no garrison of its own;
  - the selection-bar name is the settlement's, and `RegionFromSelection` returns the fort's region.

  **`FORT_ARRAY` is now loaded (0-E, 2026-10-06).** `ntw_campaign::world::read_forts` reads each
  `REGION`'s `FORT_ARRAY` into `World::forts` (`ntw_sim::campaign::Fort`), and `play::map_pick`
  picks the nearest fort (within a PLACEHOLDER 1.5 map units) before the settlement, so
  `CampaignSelection::Fort` can now come from a map click.
  - **CONFIRMED (data):** `FORT_ARRAY` is a child of `REGION` (after
    `RELIGIOUS_MISSION_BUILDING_ARRAY`, before `RESOURCES_ARRAY`) and holds **0 items in all eight
    `campaigns/*/startpos.esf` and all ten vanilla saves**. Two independent checks: the region's
    reader `0x00A51E30` walks it there, and reading the install
    (`cargo run -p ntw_formats --example esf_find -- "<startpos>" FORT_ARRAY`) prints the path of
    every one. So no shipped file exercises the loader and `World::forts` is empty in every game we
    can start -- a fort selection still comes only from the HUD or the `selectfort:` harness step.
    Nothing in the model *creates* a fort either: `BuildFort` / `UpgradeFort` address the
    settlement's `FORTIFICATION_SLOT`, which is a different thing.
  - **CONFIRMED (exe; kept decompiles of the region reader `0x00A51E30`, the fort loader
    `0x00AEB190` and the fort entity `0x00965550`):** the array's place in `REGION`; that per item
    the region's reader reads **one `u32` itself** and then lets the fort object read the rest; that
    the fort object then reads a **map position** (two `f32`, put at fort +0x120) and a **string**
    (fort +0x188), plus a second string when the record's version byte says 2 and a third when it
    says 4; that the fort registers itself in the campaign model beside the region, i.e. it is a
    garrison residence of its own.
  - **UNKNOWN / PROVISIONAL:** what the leading `u32` identifies, what the strings are, the version
    byte, a fort's garrison and level (no `level` field is visible in the record at all -- see
    `Fort`), the original's clickable size for a fort, and **which model a fort draws — now closed
    from the data (0-D round 9): the region's fort is the building chain `fFort` (3 levels) and its model
    is `rigidmodels\campaignbuildings\buildings\generic\fort_lvl<n+1>_blend.rigid_model`, from
    `slots_art`'s `fort` row column #10 (`Fort_lvl1`); `slots_art` and `slots_templates_models` now load
    (`ntw_data::campaign::{SlotArtRecord, SlotTemplateModelRecord}`, tests
    `crates/ntw_data/tests/slots_install.rs`). `0x00B42B90` is the **settlement's**
    `_slot_fortifications_lvl` builder, NOT the region fort's — see CAMPAIGN_MAP.md §10.2.** The loader
    therefore finds its fields **by shape** (first integer, first coordinate
    pair, first string) and skips an item with no integer, rather than trusting indices.
  - **What a fort is** (CONFIRMED from the shipped localisation, read on the install):
    `army_fort_Tooltip_12006e` = "Build fort || Building a fort requires a general and you must be
    within your own region.", `campaign_map_tooltips_advice_line_armynon_player_fort` = "Forts allow
    armies to remain protected while outside of their cities.", and
    `campaign_map_tooltips_tooltip_line_armyplayer_fort` / `..._rakeplayer_fort` = "Right click to
    enter fort", `..._army_can_ambushnon_player_fort` = "Right click to take fort". So a fort is a
    **garrison residence a general builds in his own region**, which armies move into -- not the
    settlement's walls. The rule that builds one (cost, how many per region, whether the region must
    be under attack) is UNKNOWN and belongs to 0-B / the AI, not to the UI.
  - Tests: `fort_array_loads_as_map_objects` (`ntw_campaign`, a made-up world with a `FORT_ARRAY`; no
    install needed, because no file has one).
- **Agents tab (branch `work/campaign-agents-tab`): listed and filled.** Evidence: the user read
  `ui/agents.luac` and the character card template with `luac_dump` on the install (2026-10-05);
  details in CHARACTER_UI_HOOKS.md "Agents panel" (behaviour in words, no bytecode in the repo).
  - CONFIRMED: the info is `{agents, characters, controlable}` with `agents[i]` / `characters[i]`
    parallel; the panel reads `agents[i].card_id` (a string: the card's id and map key) and
    `agents[i].name` (tooltip); `characters[i]` is the card's character table plus `Abilities`
    (`can_assassinate`, `can_sabotage`, `can_sabotage_army`, `can_research`, `can_duel`) and
    `IsGuerilla`; the panel sets `PlayerControlled` itself; `controlable` is never used. The
    buttons ask `CharacterResidence`, `IsCharacterInPortResidence`, `CharacterInEnemyResidence`,
    `ValidAssassinationTargets`, `ValidSabotageTarget`, `ValidSabotageArmyTarget`,
    `CharacterInValidEnemyUniversity`, `ValidDuelTargetsInResidence`, `CanAgentEmbarkOrDisembark`
    and `CampaignKey`; every action call takes only the agent's Address.
  - In: `agents_info` (card id `agent_<id>` PROVISIONAL; the listed agents -- non-commander types
    in the settlement's region known to the human -- INFERRED; abilities from the saved
    `AgentAbilities` with the model's gate, else from the type, INFERRED); the eight questions bound
    on the model's CONFIRMED action gates (assassination, army sabotage, building sabotage, duel,
    school), with residences INFERRED from `garrisoned_in` and slot positions and the
    assassination / army-sabotage candidates the whole map (PROVISIONAL: reach not traced);
    `agents_tab` listed last when the settlement has an agent (INFERRED condition).
  - Still PROVISIONAL: the six action calls are no-ops (the original's target-picking step is not
    traced); `CanAgentEmbarkOrDisembark` answers false (no agent embark in the model). UNKNOWN: the
    card's `ShowAttributes`, `ShowFlag`, `TechImage` (not given). **CLOSED 2026-10-06 (0-E):** the
    agent tooltip template's fields are CONFIRMED -- `InitialiseAgent` in
    `ui\templates\template.unitcard_tooltip.luac` (proto at line 115) reads only `name` and
    `agent_type_name` of the table it is handed, and the loc key
    `agent_culture_details_onscreen_name_<agent><culture>` has one key per `agent_culture_details`
    row (53/53 on the install); `agents_info` now sets both (CHARACTER_UI_HOOKS.md). The sabotage /
    duel buttons' `and` vs `or` is the script's
    own combination of our answers. (Fixed 2026-10-07: `luac_dump` printed `TESTSET` reversed and
    labelled `TEST` by its register's parity, not its C operand; every `TEST` in
    `template.buildingframe.luac` has C = 0, "jump if false". Older dumps misprint `and` conditions
    on even registers as "jump if true".)
  - Tests: `agents_tab_lists_the_settlements_agents_with_the_info_the_panel_reads`,
    `agent_button_questions_answer_from_the_model`, `agent_panel_calls_are_bound_as_no_ops` (lib);
    `agents_panel_opens_without_script_errors` (install, no longer ignored: it skips without the
    install), `every_agent_culture_row_has_its_onscreen_name_key` (install, the 53-row loc
    contract). Run on the install:
    `cargo test -p ntw_script --test campaign_ui agents_panel -- --nocapture`.
  - `CampaignModel::school` was made public (read-only) for `CharacterInValidEnemyUniversity`.
- **Not on main:** deal **region** rows → `TransferRegion` (rejected in the campaign
  port: its effect was INFERRED from the capture path and left the old owner's garrison behind; the
  rows are dropped, with the technology rows); unit `Experience` stays 0 (no experience in the model).
- **Walls (campaign bug 2, branch `work/blocker-walls`, 2026-10-07): the last card of the settlement construction panel, CONFIRMED -- see "Where the walls are built" below.** First cause found:
  the frame's upgrade click calls `UpgradeFort(g_fort_ptr)` with no level and our binding needed a
  string, so it queued nothing.
  - CONFIRMED (own Ghidra copy): `UpgradeFort` is registered at `0x0042AC55` (handler `0x009FAE80` ->
    `0x009C6C20`), `BuildFort` at `0x00428295` (`0x009DDD00` -> `0x009B2A30`). Each handler reads one
    userdata argument and serialises a message on `g_pCampaignUiManager` (`0x015C4938`) `+0x9FC` `+0xC`
    (the command stream): a header with the command-type id (`UpgradeFort`'s global `0x015C4194`), then
    that one argument with an argument tag -- `0x89` (fort, writer `0x0090F930`) / `0x80` (writer
    `0x0090F8F0`, shared by 23 senders). No level and no faction travel, so the model picks the level.
    So `0x89` / `0x80` are argument tags, not queue ids (0-G's "DemolishFort queue id `0x89`" is the
    same fort-argument tag: `DemolishFort`'s `0x009BA250` uses `UpgradeFort`'s functor vtable
    `0x01361900` with its own type global `0x015C2ACC`). Documented in the shared Ghidra project
    (renames, prototypes, plates; `HandleCampaignUIUpgradeFort`, `CampaignUiManager::SendUpgradeFortCommand`,
    `HandleCampaignUIBuildFort`, `CampaignUiManager::SendBuildFortCommand`).
  - **The commands' executors (re-review, 2026-10-07, own Ghidra copy; all renamed / typed /
    plated, completeness 89-100).**
    - `UpgradeFort` = `CCQ_UPGRADE_FORT` (registered by `RegisterUpgradeFortCommandType`
      `0x00423480`), executed by `ProcessUpgradeFortCommand` `0x009389A0` -> the owner faction's
      `+0x728` manager (`ProcessFortUpgradeForFaction` `0x008F9B00`) -> `ProcessFortUpgradeConstruction`
      `0x00B4E620`. That body takes **one** level, `FindNextFortUpgradeLevel` `0x00B430C0`: the
      faction's fort level record (chains of slot type `fort`, i.e. `fFort`) whose level index is the
      fort's `+0x188` plus one, if it is not in the scripts' restriction set (`0x009CDA90`);
      affordability is not part of it. The panel builder `0x009FC010` makes its single upgrade row
      from the same call. CONFIRMED: one card, and it is the level the click would build.
    - **But both are switched off in the shipped exe.** `0x00B4E620` first asks `0x0047BA10`, and
      the panel builder fills the upgrade row's `affordable` from the same `0x0047BA10`; that function
      always returns false and discards its one stack argument (a compiler-folded constant `false`, 12 callers). So the original shows
      the fort's upgrade card greyed and the click queues nothing. CONFIRMED (bytes + both call sites).
    - `BuildFort` = `CCQ_CHARACTER_BUILD_FORT` (`0x0041FF00`), executed by
      `ProcessCharacterBuildFortCommand` `0x00932670` -> `HandleCharacterBuildFieldFort` `0x009200F0`
      on world `+0xF60`, with the army's general (`Army.lua:382` passes `g_general`). After the
      character's may-act check, its placement step is `0x00462CB0`, a function that always returns false (constant
      false, 14 callers), so the change context commits nothing and only the failure event `0xCE`
      is posted (success would be `0xD6`). CONFIRMED: the field fort cannot be built in the shipped
      game (the AI's caller `0x00A679E0` fails the same way).
    - `ui/army.luac` main chunk lines 8-10: `FBS_ABLE` = 0, `FBS_UNABLE` = 1 (`AbleToBuildFort` is
      `g_fort_building_status == FBS_ABLE`; `GenerateNavyPanel` sets `FBS_UNABLE`). `ShowArmyButtons`
      (`:1126-1138`) greys the button unless able, and compares `0 < g_fort_cost`.
    - **The writer, found (re-review 2, 2026-10-07):** the army panel info builder `0x009FCFB0`
      (`BuildArmyPanelInfoTable`, 2,360 bytes, undefined code until now -- which is why the two key
      strings had no references; a byte search for their addresses found its immediate pushes). It
      writes `controlable`, `commander`, `military_force`, `units_info` / `ArmedCitizenry`, and
      `can_build_fort_status` = `0x0047AF90()`, which always returns 1 (= `FBS_UNABLE`; INFERRED in game),
      `build_fort_cost` = `0x004613B0()`, which always returns -1. CONFIRMED: both constants.
    - **`FortDetails` body (re-review 2):** handler `0x009E49D0` reads the key only when called with
      exactly two arguments (a nil second one is popped), then `0x009B2AA0`: an empty key = the
      fort's standing level (`0x00B42E40`), any other key = that row of **all** `building_levels`
      (no fortification check; an invalid key logs "not a valid key"), and `0x009AA250` writes eight
      keys: `Key`, `Name`, `ShortDescription`, `LongDescription`, `IconFilename`, `InfoFilename`,
      `Level`, `MaxLevel` -- no `region_key` / `slot_key`. CONFIRMED.
    - **The other fort actions (re-review 2):** `RepairFort` `0x009F1370`, `CancelUpgradeFort`
      `0x009E1240`, `CancelFortRepair` `0x009E0F80`, `DemolishFort` `0x009E23D0` each read one fort
      userdata argument and send a command carrying only it. CONFIRMED.
    - What the fort entity is: the `fFort` chain (slot type `fort`), the map's forts -- not the
      settlement's walls (`sFortifications*`, slot `settlement_fortification`).
    - **Where the walls are built: the LAST card of the settlement's construction panel --
      CONFIRMED (static + runtime + in game, 2026-10-07).** The user's "small star fort" in London
      was the walls: "Small Star Fort" is
      `building_culture_variants_name_sFortifications1_settlement_fortifications` + `european` /
      `middle_east` / `egy_*` (install localisation); the `fFort` levels are "Wooden Fort"
      (`fFort1`), ... and "Star Fort" (`fFort3_star_fort`), never "Small". The static reading of the
      fort stubs stands: nothing in the shipped exe builds a map fort.
      - Static (own Ghidra copy): the region's `REGION_SLOT_MANAGER/FORTIFICATION_SLOT` is an
        ordinary building slot -- `LoadRegionSlotManager` `0x00A4DCB0` makes it with the same
        0x254-byte slot constructor (`0x00A4D4F0`) as the `REGION_SLOT_ARRAY` slots and the road, and
        `SaveRegionSlotManager` `0x00A533C0` writes all three with `0x00A530B0`.
        `AttachSlotToSettlement` `0x00B6D270` files a slot on its settlement: `settlement_road` ->
        `+0x1C0`, `settlement_fortification` -> `+0x1C4` (`IsSettlementRoadSlot` `0x00A8B970` /
        `IsSettlementFortificationSlot` `0x00A8B580`), any other -> the slot list `+0x1A8/+0x1AC`.
        `BuildSettlementConstructionInfoTable` `0x00A01F50` lists that slot list and then **the
        fortification slot `+0x1C4` as the last "Slots table entry"**, each through the ordinary
        slot entry builder `BuildConstructionSlotEntryTable` `0x009FBAB0`; its keys are `slots`,
        `faction_key`, `controlable` only (no `infrastructure`, no `fort_ptr`).
      - Runtime (the manager, original under Ghidra's debugger): a breakpoint on `0x00A01F50` hit
        when the user selected London. Call chain (return addresses): `0x00A0F14D`
        (`HandleCampaignMapMouseEvent` `0x00A0EDF0`, map click, event 0 -> settlement under the
        cursor) -> `0x009C38A1` (`HandleSettlementSelected` `0x009C37A0`) -> `0x0099A492`
        (`ConstructSettlementPanelTabs` `0x0099A200`) -> `0x009DA2EB` (`OpenFirstEnabledPanelTab`
        `0x009DA2A0`) -> `0x009C7D4C` (`ChangeConstructionTabStateGeneratePanel` `0x009C7CF0`, which
        calls Lua `GenerateConstructionPanel` with the tab's info builder) -> `0x00A01F50`.
      - In game (the user): "Small Star Fort" is the last card of London's construction panel.
      - The settlement's `infrastructure_tab` is the **road**, not the walls (CONFIRMED, static):
        `ConstructSettlementPanelTabs` adds tab 0x50 (`0x0098C260`, the construction tab's vtable
        `0x0136B2F4`, so `GenerateConstructionPanel` too) when the settlement's road slot `+0x1C0`
        exists, and its info builder `BuildSettlementInfrastructureInfoTable` `0x00A021B0` lists the
        road slot alone plus `infrastructure` = true (`Construction.lua:75` -> `g_infrastructure`,
        which hides the repair / demolish buttons, `:408`, `:429`).
      - The fort panel (`GenerateFortConstructionPanel`) is the map fort's: `BuildFortPanelRowsTable`
        `0x009FC010` takes its levels from `BuildFactionFortLevelList` `0x00B42ED0` (chains of slot
        type `"fort"`, `0x0134297C`; another caller is the `FORT_ARRAY` loader `0x00AEB190`), and
        `BuildFortPanelInfoTable` `0x009FDFE0` pushes `fort_ptr` = the panel's fort object (`+0x70`;
        the `1` beside it is the push's flag byte, not the value).
      - All of the above renamed and plated in the shared Ghidra project (saved).
  - **Ours, matched (2026-10-07, branch `work/blocker-walls`).**
    - `construction_info` (construction tab) lists the settlement's `settlement:` slots and then the
      walls slot as the last entry (`slot_key` `fortification:<region>`, PROVISIONAL format), built
      by the frame's ordinary calls: `BeginConstruction` / `BeginUpgrade`, `CancelConstruction`,
      `RepairBuilding`, `DemolishBuilding`, `CanDemolishBuilding` resolve that key to
      `FORTIFICATION_SLOT` (`slot_by_key`). Options use the same slot rule as every slot
      (`build_options` = the model's `CampaignModel::construction_options`, the one rule
      `can_build` checks too, less the scripts' restricted levels; `Region::construction_slot` is the
      one slot lookup). It no longer sets `infrastructure` (the exe does not), and the made-up
      `slot_index` key is gone.
    - Every slot entry, the walls and the road included, passes the exe's filter `0x00B7A0E0`
      (CONFIRMED, static, review 2026-10-07): listed when a building stands or is being built there,
      else only when its option list is not empty. A slot with nothing to show is dropped -- the
      shipped `Construction.lua:106` indexes an entry's first building, so an empty entry would be a
      script error (install test). The walls / road slot objects exist in every
      `REGION_SLOT_MANAGER` of all eight shipped startpos files (CONFIRMED, `esf_find`: 72/72 eur,
      30/30 egy, 25/25 ita, 31/31 spa, 8/8 tut, mp alike). `0x0099A200` adds the infrastructure tab when
      the road slot pointer (`+0x1C0`) is set -- not on `0x00B7A0E0`, which only the panel's info
      builder asks -- so ours adds it for every settlement, and an empty road panel opens without
      script errors (install test); a mod region without the slots is PROVISIONAL.
    - The settlement's `infrastructure_tab` is now the road's construction panel
      (`GenerateConstructionPanel`, road slot `road:<region>`, `infrastructure` = true). No road
      repair (review finding, rejected with evidence): that panel's `infrastructure` flag hides the
      repair / demolish buttons (`Construction.lua:408`, `:429`) and a damaged road gets no upgrade
      cards (`0x009FBAB0`), in the original too.
    - Round 6 (2026-10-07): `can_build` refuses an upgrade of a damaged building, as the panel
      offers upgrades only at full health (`0x009FBAB0`); one rule for panel and command. The map
      fort level list is every `fort` chain with no permission test, and the standing / next levels
      are picked by level index alone (`0x00B42ED0`, `0x00B42EA0`, `0x00B430C0`, CONFIRMED; list order
      PROVISIONAL). `FortDetails` addresses only a fort argument or a selected fort (a settlement has
      no map fort). One building-variant resolver: the culture is `faction_cultures` (the
      `building_culture_variants` culture keys; INFERRED the exe's `0x008B1480` source); the info
      picture is empty for the placeholder icon.
    - Round 7: the AI's build options are the model's `construction_options` (walls included) and
      the AI repairs its walls too (`ai_repairs`; the exe's AI repairs through `0x00AA4910`, see
      AI_RESEARCH.md), so a damaged AI building upgrades again after its repair. The building
      browser lists the walls between the region's slots and the road (`0x009B5AF0`, CONFIRMED order).
      Slot addresses are interned ids (no region / index packing). A loaded fort's `Fort::key` is its
      standing level (INFERRED). `fort_controlable` keeps the region owner (PROVISIONAL: the model's
      `Fort` has no owner). The model's `Option<usize>` + `FORTIFICATION_SLOT` slot convention predates
      this branch and is kept (one representation); an enum is a separate refactor.
    - `FortDetails` takes the texts and the icon from the fort owner's culture (the exe's details
      filler `0x008B1480` runs on the fort's faction `+0x210`, INFERRED owner).
    - The fort panel is only a map fort's (`CampaignSelection::Fort`, tab `Tab::Fort`, key
      `construction_tab`: CONFIRMED, `HandleFortSelected` `0x009C33E0` -> `ConstructFortPanelTabs`
      `0x009988C0` -> `ConstructFortConstructionTab` `0x00989BB0`, key id 0x52, tab vtable `0x0136B350`
      with `0x009C7B50` / `0x009FDFE0`; the army / recruitment / agents tabs it adds for a garrisoned
      fort are not shown, the model's fort has no garrison): the `fFort` chain's level 0 as the standing row
      (PROVISIONAL: the fort's level `+0x188` is not loaded) and the next level as one upgrade card
      with `affordable` = false (CONFIRMED `0x0047BA10`). `UpgradeFort` / `BuildFort` send nothing
      (CONFIRMED stubs); `RepairFort` / `CancelUpgradeFort` / `CancelFortRepair` / `DemolishFort` are
      PLACEHOLDER no-ops (the model's map fort has no building state; no shipped file has a fort).
      The walls path through the fort panel (`fort_offer`, `UpgradeFort` -> walls) is removed.
    - `force_info` reports `can_build_fort_status = FBS_UNABLE`, `build_fort_cost = -1` (INFERRED
      until the side-by-side army-panel check -- static only). PLACEHOLDER: the `0xCE` failure event
      is not posted.
    - `FortDetails` follows the exe: the second argument's key (any building level), an empty / absent
      key = the map fort's standing level, the eight keys always present; with no level (no fort
      chain, an unknown key, nothing addressed) the details are default (INFERRED empty strings and
      0), which the tooltip shows without the nil-description error (install test runs
      `TechTreeItem_Tooltip`'s `InitialiseBuilding` on the tables). INFERRED: `IconFilename` /
      `InfoFilename` are the culture variant's icon under `data/ui/buildings/icons|info/`.
    - `CampaignRules::buildings` is a `BuildingTable` whose chain index is built on first use and
      dropped on every mutable access, so `Level` / `MaxLevel` cannot go stale;
      the slot-type test lives in one place, `CampaignModel::construction_options` (review 2026-10-07; `is_fortification_level` and `fort_levels` / `fort_options` / `FortOption` are gone).
    - Tests: lib `walls_are_the_construction_panels_last_slot_card`,
      `the_infrastructure_tab_is_the_road_slot`, `the_map_fort_panel_never_builds`,
      `fort_details_describe_one_subject`; install (shipped layout and scripts)
      `walls_are_the_last_construction_card_and_build_from_it` (Paris: the walls card follows the 5
      settlement slots, nothing after it, its one constructable is
      `sFortifications1_settlement_fortifications`, the click queues and pays for it) and
      `fort_selection_opens_the_map_fort_panel`.
  - CONFIRMED (`template.buildingframe.luac`, proto at line 417): the click only fires when the
    card's `affordable` **and** `tech_present` are set. Proto at line 372: the card's tooltip calls
    `FortDetails(g_fort_ptr, building_key)` and feeds `InitialiseBuilding`, which needs `Name`,
    `ShortDescription`, `Level`, `MaxLevel`; ours lacked them with no fort standing (tooltip error at
    `TechTreeItem_Tooltip.lua:75`). Now it describes the named level.
  - The level built is the offered one (CONFIRMED rule, above); vanilla's chain is linear
    (`sFortifications1_settlement_fortifications` -> `...2_improved_settlement_fort`), so exactly one.
    PROVISIONAL for mods offering several: the first in `construction_options` order (the standing level's
    `upgrades_to` data order, or key order for an empty slot's level 0s), where the exe takes the next
    level index; an unaffordable first level greys the card and is never swapped for another.
  - Tests (unit, `ui/campaign.rs`): `upgrade_fort_sends_nothing_for_a_foreign_region`,
    `upgrade_fort_checks_a_named_level_like_the_offered_one`, `the_fort_card_and_the_click_are_the_same_level`,
    `fort_details_describe_one_subject`.
  - CONFIRMED (data): 0 fortifications at the start is right: none of the 72 `FORTIFICATION_SLOT`s
    in `eur_napoleon/startpos.esf` has a `BUILDING` (20 `ROAD_SLOT`s do). France starts with 6500;
    Paris' walls cost 7200 (8000 elsewhere), so on turn 1 the card is correctly greyed.
  - Test: `fort_tab_upgrade_card_queues_the_walls_and_pays_for_them` (campaign_ui, install).
- PROVISIONAL on main: the negotiation deal lists are never filled from Lua (open item 1), so
  `Propose` closes an empty deal.

**Round of 2026-10-05 (slot campaign-ui, branch `work/campaign-ui`): the campaign map's movement
arrows** (§7). Done: the ordered path is now drawn with the original's own textured arrows
(`rigidmodels\campaignpieces\textures\arrow.dds`, the map's `display\arrows\arrows.rigid_model`
proportions), sampled every `spacing` of walking distance and smoothed into two chains exactly as
`FUN_009CC5F0` does; the gizmo line is gone. The spacing (12 map units) and the two colours are
PROVISIONAL — they are arguments of `FUN_00A27C30`, whose caller Ghidra cannot resolve. Next in this
backlog line: **selection effects**, then the **zone-of-control display** (the model side is already
finished in `ntw_sim::campaign::zoc`; only the rendering is missing, and §7 notes that
`FUN_009CC5F0` is probably the same ribbon machinery), then the resource icons beside the names.

**Round of 2026-10-05 earlier (same branch): the campaign map's REGION labels** (§6). Every land
region's name painted on the map at the position `regions.esf` gives it, in the original's own bitmap
font.

**Round of 2026-10-04 afternoon (branch `work/fidelity-ui2`): order (1) settlement panel playability,
(2) building browser tree, (3) capture screen, (4) negotiation, (5) save naming, (6) 0-G hooks.**

- **(1) Settlement panel: done** (§2 below). Proof run, no person needed:
  `cargo run -p napoleon -- --campaign eur_napoleon --campaign-faction france --campaign-ui-proof --campaign-capture-proof --screenshot target/tmp/ui_proof/99_final.png`
  writes `target/tmp/ui_proof/01..09_*.png` (settlement, slot upgrades, upgrade queued, recruitment,
  two units queued, one cancelled, after End Turn x2) and `c1/c2` (capture screen, after Loot), and
  logs `Proof:` lines (treasury, construction and recruitment queues, garrison size, region owner).
  Checked 2026-10-04: the upgrade counts 8 → 7 → 6 turns, the fusiliers 2 → 1 → done (garrison
  2 → 3 units), treasury 6500 → 3090 (upgrade 2700 + 1 unit 710) and the cancelled unit refunded.
- **(3) Capture screen: opens and answers** (§3 below); the capture is staged in the proof run.
- **(2) Building browser tree: done.**
  - Data: `building_tree` / `CampaignUI.__BuildingTreeNodes(slot)` in `crates/ntw_script/src/ui/campaign.rs`.
    The doc comment there gives the exe structure (0x009B8830, 0x009B9120, 0x009B5A60).
  - Components: `CampaignUI.ConstructBuildingTree(slot, parent)` in `campaign_prelude.lua`. It makes
    `building_browser_node` templates named by level key, with the picture
    "{key:1}icon", state normal / available / unavailable, the tooltip, and `Initialise` with 4 arguments.
  - Layout: a tidy tree; the gaps are INFERRED from 0x0099A6A0.
  - Links: "vertical / horizontal node link" pieces from `general_purpose_pixel`, in black.
  - Returns the tree's height.
  - Test: `building_browser_entry_shows_the_slot_tree`.
  - Proof run: add `--campaign-browser-proof` (b1 browser, b2 tree, b3 node tooltip).
  - PROVISIONAL: nodes hang under the tree component, not under their parent node; the level
    record's +0x5C region flag is not applied; the gaps are INFERRED.
- **Proof command (all three proofs):**
  `cargo run -p napoleon -- --campaign eur_napoleon --campaign-faction france --campaign-ui-proof --campaign-browser-proof --campaign-capture-proof --screenshot target/tmp/ui_proof/99_final.png`
- **Round of 2026-10-04 evening (continued by the next worker):**
  - Task 0 (merge blockers): the `user_saves_*` tests skip a save that vanishes, is locked or is
    younger than 10 s (the user may be playing); three AI-block site rows the user's vanilla saves
    disprove (SAVE_COMPAT.md §31); `save_check::Report::informational` holds the rules that only
    guard the original's loader (pathfinder / boundary manager / loader replay / AI block), so
    `save_audit check` counts only our own rules as violations; the two "doc list item" clippy
    warnings in `ui/campaign.rs` fixed.
  - **Capture proof fixed:** `stagecapture:<region>` now goes through
    `CampaignModel::capture_by_force` (occupy, then the capture report), as a won assault does, so
    the proof's second `owner:` line shows the region as France's after Loot (it stayed with the
    old owner before: the staging only built the preview).
  - **(1) Restricted buildings in our saves: done.** `EPISODIC_RESTRICTIONS/BUILDING_RESTRICTIONS[]`
    and `UNIT_RESTRICTIONS[]` (one utf16 key per item; CONFIRMED in the user's vanilla auto_save: 55
    building levels, units empty; the startpos has both lists empty) are written from the script
    state at quick save (`script_values::write_restrictions`) and seeded into the script state at
    load (`LoadedCampaign::restrictions`, scene.rs) before `UICreated`. Checked in the game: the
    reloaded quick_save logs "55 restricted building levels and 0 units from the save". Unit test
    `restriction_lists_round_trip`.
- Not started yet this round: (6) 0-G hooks. (4) negotiation: layout
  mapped, appliers wired in the 0-E sandbox round N+2 (A4.6) but the deal object
  is still never filled from Lua (open item 1). (5) save naming: layout
  mapped, actions TODO (0-E sandbox note below, §5); **the naming rules are CONFIRMED and wired**
  (0-E, 2026-10-06 — `ValidateFilename` refuses `/ \ * ? " < > | :` and a 101st character, and
  `ntw_script::ui::frontend::is_valid_save_name` is that rule set, applied in `SaveFolders::resolve`;
  `ConfirmSave` / `SaveCampaign` are still UNKNOWN and unwired).
- **Round of 2026-10-04 evening (0-E sandbox, branch `work/sandbox/0e-ui`): negotiation screen
  mapped** (§4 below). Read-only probe of the original files (no Ghidra, no placeholders):
  the negotiation screen is `ui\campaign ui\diplomacy_panel` (Version039, 351 components) with
  four `ui\templates\diplomacy_*` templates and three
  `diplomacy_panel_scripts/*.luac` drivers. `unknown_da` (ClipChildren, CONFIRMED) is 1 on
  exactly 21 components (offer/demand `list_clip`s, the four radar-map `Blank` masks, the
  declare-war/war-declared `list_box`+`bg` pairs, the technology/join-war `list_clip`s, the
  diplomat portrait's `clip_box`); `unknown_e5` (UseGlobalClicks) and `unknown_140` (DrawMode)
  are 0 everywhere, so the whole screen draws in mode 0 by inheritance. No new script function
  was added: every remaining negotiation call shape is UNKNOWN (luac token counts only), so per
  the conservative rule the actions are notes-only with a TODO list (§4). The wired element is
  the layout contract itself, locked by test
  `ntw_formats/tests/real_install.rs::diplomacy_negotiation_layout_fields` (original files
  only).
- **Round of 2026-10-04 night (0-E sandbox, round N+1, same branch): the negotiation appliers
  traced** (A4.1, A4.2 below). Own Ghidra copy `NR-sb-ghidra-0e`, read-only, batches
  `e5_gen` / `e6_appliers` / `e8_items` / `e9_recruit` / `e10_keys` / `e11_naval` in
  `target/tmp`. Results: the 20 `negotiation:*` shapes have a per-shape CONFIRMED/UNKNOWN
  ledger; the native deal appliers are named (`0x00B58C00` offer, `0x00B58A30` demand,
  `0x00B58560` / `0x00B58890`, the commit `0x00A6CBE0 -> 0x00B1A790`, the money
  `0x00BB3810(amount, 3)`, the region transfer `0x00B449F0`), and technologies stay honestly
  UNKNOWN (no address behind the row vtable). STEP 3 answered both ways: the naval recruitment
  tab has no generator of its own (CONFIRMED - no naval recruitment generator is registered, and
  the one recruitment generator, implementation `0x009FE7B0`, carries a `naval` category and a
  per-card `is_naval`; who switches it is INFERRED), and
  `construction_manager.GenerateFortConstructionPanel` is CONFIRMED down to its info keys
  (`forts` / `fort_ptr` / `controlable`) but still unwired because it needs a
  `CampaignSelection::Fort`. Code: `accept_deal`'s PLACEHOLDER list now names a target address
  per dropped row type; `cargo test -p ntw_script` 60 pass, no warnings.
- **Round of 2026-10-04 night (0-E sandbox, round N+2, same branch): WIRING FIRST** -- the four
  contracts above, wired end to end (A4.6).
  - **(1) Demolish button: done.** `CampaignCommand::DemolishBuilding`, `can_demolish` and the test
    `buildings_and_forts_are_demolished` were ported read-only from 0-G's branch
    `work/sandbox/0g-characters` (`0ad4339`); the host now binds `CanDemolishBuilding` /
    `DemolishBuilding` / `DemolishFort`, and `CanDemolishBuilding` is no longer a `false` stub.
    Test `demolish_button_removes_a_standing_building`.
  - **(2) Fort / infrastructure tab: done.** `CampaignSelection::Fort(RegionId)` (model work:
    `CampaignModel::fort_levels` / `fort_options` / `can_build_fort` and the new
    `FortOption`), `Tab::Infrastructure` -> `GenerateFortConstructionPanel`, `fort_info` with
    `{forts, fort_ptr, controlable, faction_key, infrastructure}`, and the fort's six actions on
    `FORTIFICATION_SLOT`. Tests `fort_levels_are_listed_and_a_fort_can_be_affordable` (ntw_sim),
    `infrastructure_tab_builds_and_demolishes_the_fortification` (install).
  - **(3) Naval recruitment tab: done, PROVISIONAL on the selector.** `Tab::NavalRecruitment` ->
    `GenerateRecruitmentPanel` with the naval argument, on a **naval character's** panel (0-G's
    trace says the settlement builder never adds it); the region's naval capacity
    (`recruitment_points(region, true)`) and its ships only. Test
    `naval_recruitment_tab_lists_only_ships`.
  - **(4) Remaining negotiation appliers: three of four done.** Regions -> the new
    `CampaignCommand::TransferRegion` (`0x00B449F0`), payments -> `StateGift` / `RegularPayment`,
    stance / access / gift -> `Diplomacy` as before. **Technologies stay UNKNOWN and dropped** --
    0-E proved no granting address is reachable, so none is guessed. Tests
    `a_region_is_transferred_by_a_deal_command` (ntw_sim), `negotiation_rows_map_to_their_appliers`
    (host).
  - What the panel scripts then settled is in A4.6: every construction / fort call's argument list,
    `UIComponent:GlobalExists` as a getter, the fort panel's row set (no `CONSTRUCTABLE` branch, and
    `Construction.lua:624`'s assert), `ResetConstructionPanel`'s keys, the army panel's fort button,
    and the CONFIRMED fact that **no shipped script names `naval_recruitment_tab`,
    `infrastructure_tab` or `CampaignShipCard`**.
  - Counts: `cargo test -p ntw_sim` 286 pass, `cargo test -p ntw_script` 64 pass (13 in
    `campaign_ui`, up from 10), no new warnings. Evidence archived in
    `ghidra_evidence/0e/luac__*.txt` (PRIVATE FORK, read-only dumps of the shipped .luac).

| Item | State | Tag |
|---|---|---|
| Root layout globals visible to modules (`_G` falls back to the root's environment) | resolved | INFERRED (Recruitment.lua / BuildingFrame read root SETGLOBALs) |
| Slot hover shows upgrades (SelectPassive), click queues (BeginUpgrade / BeginConstruction) | resolved | CONFIRMED script flow |
| Construction info shape (`buildings` = item or building, then an empty slot's options; `upgrades` at full health) | resolved | CONFIRMED `0x009FBAB0` |
| Construction options filter (`0x00B43300`): permission test, unaffordable / no-tech kept greyed | resolved | CONFIRMED structure; tables INFERRED |
| Building permission `0x008BE7F0`: script restriction, then faction or culture variants | resolved (model `building_permitted`, restriction in `World::restricted_buildings`) | structure and restriction CONFIRMED, source tables INFERRED |
| Scripts' restricted buildings (UICreated → InitialiseCampaign: 55 levels, as the original save's BUILDING_RESTRICTIONS) | resolved | CONFIRMED count |
| CancelConstruction (`0x009E0F10`), model command `CancelConstruction` | resolved | call CONFIRMED, full refund INFERRED |
| RepairBuilding button | resolved | CONFIRMED call |
| Demolish (`CanDemolishBuilding`) | resolved (N+2) | `can_demolish` + `DemolishBuilding`, ported from 0-G `0ad4339`; queue ids `0x84` / `0x89` CONFIRMED |
| Recruitment card details (`unit_record` table for the tooltip), UnitScaleFactor | resolved | names CONFIRMED, stat columns INFERRED |
| Same selection re-sent keeps the open tab | resolved | INFERRED |
| Template texts set on every state (cost in the "red" state) | resolved | INFERRED |
| `obj:SequentialFind` returns an object | resolved | CONFIRMED usage |
| Capture screen (`SettlementLootingOptions`, `InformLootingSelection`) | resolved | names CONFIRMED; FactionLocation text INFERRED |
| Restricted buildings and units in our saves (BUILDING_RESTRICTIONS / UNIT_RESTRICTIONS) | resolved | layout CONFIRMED (buildings), INFERRED (units) |
| Brunswick Hussars card for France has no picture (no `france_` icon) | open | UNKNOWN whether France may recruit it |
| Agents tab of a settlement | listed and filled (install run pending) | CHARACTER_UI_HOOKS.md "Agents panel". Info and the eight button questions from the model (gates CONFIRMED, residences / candidates INFERRED or PROVISIONAL); the six action calls PROVISIONAL no-ops. Test `agents_panel_opens_without_script_errors` (install), see "State on main" |
| Naval recruitment / infrastructure tabs | resolved (N+2) | naval on a character panel, infrastructure on a settlement's |

**Earlier (resumed by the save-compat worker)** (2026-10-04, branch `work/fidelity-ui2`, worktree
`NR-ui2`). Order given: (1) loading saves from the front end's Load Game page, (2) front-end
tooltips, (3) credits, (4) custom battle setup. The 0-E work before the pause is merged (b205bbb).

- **(1) Load Game page: done** (FRONTEND_PAGES.md, rows `sp_load_game.lua`, `LoadCampaign`,
  `ContinueCampaign`, `DirectoryUtils.DeleteFiles`).
  - The page lists NapoleonRust's own save folder plus the original's (read only; ours win on
    equal names), newest first, with turn, local save time, year, season, portrait, flags and the
    save header's territory picture (`SAVE_GAME_HEADER/MAPS`, now read by `ntw_campaign`).
  - Load passes the save's name, as the original's page does; it is resolved to the listed file
    and the campaign loads it (checked in the game: the front end loads `quick_save` into the
    campaign).
  - Delete removes only files in our own folder.
  - Tests: `load_game_page_lists_both_folders_and_loads_by_name` (frontend_ui) and
    `only_our_saves_can_be_deleted_and_names_resolve`.
  - Open: our save writer still copies the startpos's `MAPS` picture (stale territories after
    conquests): a save-compat follow-up. The divorced `row_example`'s own script logs
    `UIComponent(nil)`: its main chunk asks for its parent after the page script has divorced it,
    because our host runs the page's script before its children's (INFERRED harmless; the order
    of script runs in a layout is UNKNOWN).
- **(2) Tooltips: done** (FRONTEND_PAGES.md "Tooltips"): front end and campaign share one hover path;
  the Tooltip template is fitted to its text (SetState always runs InitState, CONFIRMED `0x01035B30`)
  and its edges follow the box; the tooltip object is drawn last (CONFIRMED). Test
  `front_end_tooltips_show_and_fit_the_text`; harness `--ui-click hover:<id>` (front end) and
  `ui_run hover:<id>`. Open: the hover delay (UNKNOWN).
- **(3) Credits: done** (FRONTEND_PAGES.md "Credits"): `BuildCredits` builds the 21 pages from
  `text/credits.xml`; credits.lua cycles them. Open: the credits music, the exact column alignment.
- **(4) Custom battle setup: done except the leftovers below** (FRONTEND_PAGES.md "Custom battle").
  - The map and settings page; the players and armies page with the default armies
    (`RetrieveArmyPresets`), the recruit cards (`RecruitableUnits`), experience costs,
    `ValidateArmySetup`.
  - Setup files: `.army_setup` (Save / Load / Enumerate) and `.battle_preferences` (Save / Load;
    the settings page keeps its last settings in `.sp_default`; Load Battle opens a saved
    battle). Layouts CONFIRMED from the exe's writers; the original's `.mp_default` file reads
    and writes back byte-exact.
  - Starting the battle in our engine: OK sends `UiRequest::StartCustomBattle`, the game loads
    the map and deploys both armies (units, experience, general, the human's army controllable)
    in their deployment areas; checked in the game with
    `--ui-click single_player,sp_battle,button_classic_battle,button_host,button_ok`.
  - Tests (frontend_ui): `custom_battle_start_requests_both_armies`,
    `army_setups_save_load_validate_and_list`,
    `battle_settings_are_kept_in_the_default_preferences_file`,
    `saved_battle_setup_loads_from_the_requester`; (napoleon)
    `custom_battle_armies_deploy_in_their_areas`; (lib) the two file layouts.
  - Left: text entry in the UI host (typing a file name in Save army / Save battle), the unit
    size option in our battle, the category mask, naval custom battles, `GenerateShipName`.
- **Round after custom battle (paused 2026-10-04 ~13:30, manager's order 1-5):**
  - (1) **Text entry: working, notes owed.**
    - Focus: a click on a state with +0xD4 set, or StealInputFocus(true/false) (CONFIRMED
      0x01015DC0 / 0x010367F0), with OnInputFocusGain/Lose.
    - Typing: Latin-1 characters go to `CharacterInput` (CONFIRMED dispatch 0x0102DD50).
    - Editing keys go through the layout's OnKey binding.
    - Also added: IsCharPrintable, FindPositionIntoCurrentText, `vfs.exists`, click positions
      passed to mouse handlers, and game keyboard input with `--ui-click type:<text>`.
    - Test `typed_file_name_saves_the_army`. Screenshot checked: the Save Army requester shows
      "Austrian line" with the caret.
    - Open: save-game naming in the campaign, and a FRONTEND_PAGES.md section.
  - (4) `.sp_default` is written hidden (CONFIRMED 0x0048D2C0), and the requester lists skip
    hidden files.
  - (5) GenerateShipName: `ship_names` (s,s,s,o) of the faction's #44 group. INFERRED from
    France's naval list in the original's save; #43 says names_english.
  - Not started: (2) building browser tree (`ConstructBuildingTree`, called from
    building_browser.lua) and (3) the capture screen.
- Save work comes first when the manager sends it (the save-compat slot, `NR-save-compat`).

**Earlier exact next steps (0-E, before the pause), still valid:**
1. Front end: credits. `FrontEnd.BuildCredits(parent)` (0x0046B100, line builder 0x004635B0):
   parse `text/credits.xml` (<page delay> <line gap fontsize colour> with <left>/<right indent>),
   create one container per page under `credits_list` and one `string` library template per
   line (fontsize → font id table 0x0144F030: 12,14,16,18,22,24,38 → ids 12..18; family INFERRED
   Frontend), return {[i] = {Page, Height, WordCount, Delay}}; `EnableCreditScreenMusic(bool)`.
   Probe: `ui_run options,credits`.
2. Front end: tooltips, custom battle setup (sp_battle1/2) (loading saves: done, above).
3. Negotiation panel: the `UIDiplomacyNegotiation` class (constructor 0x00A102B0; methods
   BuildPossibleActions 0x009B5220, Propose 0x009BF3C0, ProposeDeal 0x009BFCF0, Cancel 0x009B7A30,
   End 0x009BA850, Finished 0x009BB8B0, BuildOfferAndDemandStrings 0x009B48B0, ProposerId
   0x009BFD80, PrepareCounterOffer 0x009BF120, CanPropose 0x009B7990, CanThreaten 0x009B79E0,
   AcceptOffer 0x009B1A70, DeclineOffer 0x009B9520, RemoveAction 0x009C0F70,
   TradeableTechnologies 0x009C5AA0, TradeableRegions 0x009C5770, MaxPlayerPaymentAllowed
   0x009BE720, MaxOppositionPaymentAllowed 0x009BE6B0, IsNegotiation 0x009BD7C0,
   FactionListsForStanceDeclarations 0x009BAB90). The AI's acceptance is not in the model.
   **Traced in the 0-E sandbox round N+1** (A4.1 / A4.2): all twenty shapes have a
   CONFIRMED/UNKNOWN verdict, the native deal appliers are named, and the two remaining
   appliers without an address are named as open (the technology row vtable, and the per-turn
   schedule as one of the container's two single items).
4. Building browser tree view: `ConstructBuildingTree` (0x009E1CB0 → 0x009B8830; nodes from the
   `building_browser_node` template by 0x009B9120, states normal / available / unavailable,
   layout 0x0099A6A0, lines 0x0099E120).
5. Map visuals: settlement composition (templates + building models), forts, trees
   (`campaign.rigid_trees`), coastline, textured border / river ribbons, finer supertexture.
6. Technology screen: the tree's dependency lines draw oddly (ParentX/Yoffset meaning INFERRED);
   research (universities) is not modelled.

## Resolved / open

| Item | State | Notes |
|---|---|---|
| Radar script error (map_image.lua:197) | resolved | `CampaignUI.RegionsInTheatre` implemented (§1) |
| `UIImage:Dimensions` | resolved | reads the file header (TGA / DDS) |
| `UIPaletisedImage` (Query, SetPaletteEntry, SetComponentTexture, Release) | resolved | `ntw_script::ui::image`, drawn as a run-time texture |
| Loose picture files (data/campaign_maps/...) in the UI renderer | resolved | `UiAssets::with_loose_files` |
| `Resize(w, h, false)` keeps children's sizes | resolved (flag CONFIRMED, meaning INFERRED) | needed for the radar's clip / map / overlay |
| RegionsInTheatre: region filter 0x008E0500 | open, PROVISIONAL | every model region is listed |
| RegionsInTheatre mode 1: OwnerStatus, *RelationshipDetails | open | attitudes given, texts not |
| RegionsInTheatre mode 3: OrderRGB colour, Lower / Upper | open, PLACEHOLDER | colour scale is ours (0x00A727D0 not decoded) |
| Radar camera follow and click-to-move (`CameraTarget` 4th value, `TheatreMapDimensions`, `SetCameraTarget`) | resolved | 0x009DFEC0, 0x009F9760 |
| Theatre keys (TheatreList Key = DB key "1244818741", Id = area, Name loc) | resolved | 0x009ACE50 |
| DockingPoint / SetDockingPoint | resolved | 0x01016050 / 0x01016100 |
| ClipChildren clipping (draw and hit test) | resolved | tiled pieces PROVISIONAL |
| BuildingBrowserDetails | resolved (PROVISIONAL parts listed in code) | 0x009B5AF0 |
| ConstructBuildingTree (browser tree view) | resolved, layout INFERRED | 0x009B8830 |
| RetrieveFactionListForDiplomacy, RetrieveDiplomacyDetails | resolved (trade tests PROVISIONAL) | 0x009F2EA0, 0x009B2690 |
| UI scale for windows under 1280x960 (0x0114EB20) | resolved | campaign HUD; front end and battle HUD keep their 1280x960 window |
| Government screen: InitialiseGovernmentDetails (ministers keyed by post number), RetrieveGovernorshipDetails, SetGovernorshipTaxRate, TradeInfo | resolved, parts PROVISIONAL | popularity inputs, class tax split, trade prices and route kinds, minister pool |
| Objectives: MissionsDetails, PrestigeDetails, VictoryConditions, CurrentSeasonString | resolved, parts PROVISIONAL | prestige and victory conditions are not in the model |
| Lists: tabgroup script (template.<id>.lua rule), IsMergingUnit | resolved (rule INFERRED) | |
| Diplomacy: treaties, opinions, military access, gift values, minister portrait, character details (0x009AD250) | resolved, parts PROVISIONAL | |
| Negotiation panel (`UIDiplomacyNegotiation` object) | layout mapped (§4), object traced, appliers wired (N+2) | layout fields CONFIRMED; per-shape ledger in A4.1; payments / stance-access-gift become commands (A4.6); **region rows dropped on main** (no `TransferRegion`, `0x00B449F0` not decoded); **the technology row is still UNKNOWN**; nothing fills the deal from Lua yet (open item 1) |
| Naval recruitment tab | resolved, wired (N+2) | no naval generator exists: `GenerateRecruitmentPanel` (`0x009FE7B0`) carries a `naval` category and a per-card `is_naval`; the selector is PROVISIONAL on the manager's `+0xA4`; the tab is a **character** panel's (A4.3, A4.6) |
| Fort construction panel (`construction_manager.GenerateFortConstructionPanel`) | resolved, wired (2026-10-07) | a **map fort's** panel only (`fFort`; the settlement walls are the construction panel's last card, CONFIRMED -- see "Where the walls are built"); `0x009C7B50` / `0x009FDFE0`, keys `forts` / `fort_ptr` (the fort object) / `controlable` CONFIRMED; its upgrade card is always greyed and its actions build nothing |
| Campaign save naming (`load-save_game` screen) | layout mapped (§5), save call open | layout fields CONFIRMED; naming rules CONFIRMED (0-E, 2026-10-06 — see below); `ConfirmSave` / `SaveCampaign` shapes still UNKNOWN |
| Technology screen (TechnologyPlayerDetails tree, Localisation.Get, GetStateText extent rule) | resolved, parts PROVISIONAL | tab = key prefix, dependency lines, no research |
| Radar view outline (AttachRadarView / UpdateRadarView, 0x00A27D50 corners) | resolved, look PROVISIONAL | |
| Town / port slot template models (regions.esf slot #6) | resolved, facing PROVISIONAL | |
| Building browser tree view | resolved | §1 Where I am |
| Front-end tooltips, credits | resolved, parts PROVISIONAL | FRONTEND_PAGES.md |
| Front end Load Game page (list, sort, details, territory map, load, delete) | resolved, parts PROVISIONAL | FRONTEND_PAGES.md |
| 960-high layouts in a 720 window | resolved | see the UI scale row |
| Campaign map region labels (the name of every land region painted on the map) | resolved, look PROVISIONAL | §6: positions CONFIRMED from `theatres_and_region_keys`, text CONFIRMED from loc `regions_onscreen_`, font / size / colour PROVISIONAL |
| Campaign map movement arrows (the ordered path drawn as the original's arrows) | resolved, spacing and colours PROVISIONAL | §7: the texture and the path rule CONFIRMED; the two numbers come from an unresolved caller |

## 1. The radar (template.map_image.lua)

The radar (`campaign ui/layout` → `radar` → `Blank` → `map` → `map_overlay`) runs
`theatre_map.lua`, whose `InitialiseMap` calls the `map` component's
`InitRegionMap(theatre, faction, mode[, sub])` (template.map_image.lua). That function does
`region_details = CampaignUI.RegionsInTheatre(theatre, faction, sub, modes[mode])` and indexes it
at once, so a nil return was the error.

**`CampaignUI.RegionsInTheatre` (handler 0x009EFC30), CONFIRMED:**
- Looks the theatre up in DB table `campaign_map_playable_areas`
  (`CAMPAIGN_MAP_PLAYABLE_AREA_RECORD`) and returns `Map`, `Overlay`, `Radar` = the record's fields
  +0x10, +0x1C, +0x28, each formatted `"%S/%S"` (string 0x01316124) with the campaign map's folder.
  Shipped rows: e.g. europe → `europe_map.tga`, `europe_lookup.tga`, `stratradar_europe.tga`,
  area `europe_main`, 605 x 300. The pictures are loose files in `data/campaign_maps/<map>/`.
  INFERRED: the folder string is `data/campaign_maps/<map>`; the table is matched on the area
  column (our TheatreList keys are those area keys).
- Then one entry per region of the theatre (list at campaign +0xF5C → +0x34), skipping regions a
  position test hides (0x008E0500, UNKNOWN meaning; not ported). Each entry has:
  `PaletteEntry` (0x00A97680 → 0x00A9D850: the lookup picture's colour → palette index map, keyed
  by the region's 24-bit colour; -1 if missing), `Key`, `Name`, `Address`, `Owner` (owner faction's
  name), `TaxExempt` (region +0xE4, 0x00AAF270), `OwnerKey`.
  - modes 0 and 4: `OwnerRGB {r, g, b}` = owner faction's colour floats (+0x84/+0x88/+0x8C) × 255.
    INFERRED: those floats are the faction's primary colour / 255.
  - mode 3: `OrderRGB {r, g, b, a}` (0x00A727D0, not decoded), `Lower` / `Upper` {…, DemandText},
    `PublicOrder`.
  - mode 1, when the owner is not the faction: `OwnerAttitude` (attitude total 0x00B0DB60),
    `OwnerStatus`, `OwnerRelationshipDetails`, `FactionAttitudeTowardsOwner`,
    `FactionStatusTowardsOwner`, `FactionRelationshipDetailsTowardsOwner` (key strings at
    0x0136C92C..0x0136C99C). Direction of each attitude INFERRED from the names.
- Checked against the install: all 72 eur_napoleon regions find their `regions` DB colour in
  `europe_lookup.tga`'s palette, each at a different index (test
  `radar_map_gets_its_pictures_and_region_colours`).

**Script image objects (CONFIRMED calls in template.map_image.lua):** `UIImage(path)`:
`Dimensions`, `SetComponentTexture(address, 0)`, `Release`. `UIPaletisedImage(path)`:
`Dimensions`, `Query(x, y)` → palette index (FindRegion), `SetPaletteEntry(i, r, g, b, a)` for
i = 0..255, `SetComponentTexture`, `Release`. INFERRED: Query's origin is the top-left pixel.

**`UIComponent:Resize(w, h, flag)` (handler 0x01014760), CONFIRMED:** the third argument is a
boolean, true when omitted, passed to the component's resize. INFERRED: false = do not resize the
children (map_image resizes the map and its overlay separately with false).

**Ported:**
- `crates/ntw_data/src/schemas.rs`: `CampaignMapPlayableArea`.
- `crates/ntw_script/src/ui/campaign.rs`: `RegionsInTheatre`, `CampaignUi::map_folder`,
  `playable_area`, `palette_index`.
- `crates/ntw_script/src/ui/image.rs` + `ui_prelude.lua`: the image objects;
  `crates/ntw_script/src/ui/world.rs`: `RuntimeImage`, `UiWorld::runtime_images`.
- `crates/napoleon/src/frontend/render.rs`: run-time textures, loose picture files.
- `crates/ntw_script/src/ui/host.rs`: the Resize flag.

## Capture screen: what the model gives (note from 0-B, round 9)
The occupy / loot / liberate choice after a capture is modelled (`ntw_sim::campaign::capture`, CAMPAIGN_FIDELITY.md
§Capture). What the screen needs:
- **When to open it:** the event `CampaignEvent::CaptureChoicePending { region, faction }` (a human faction took a
  settlement by force). The preview is in `CampaignModel::pending_capture` (`CapturePreview`).
- **Its content** (`CapturePreview`): `loot`, `occupy` (`CaptureOutcome`: `buildings` = (slot index, health after),
  `money`, `town_wealth` after, `public_order_reduction`, `public_order_after` = (lower, upper)) and `liberate`
  (`Option<FactionId>`, the faction a liberation would restore; no option when `None`).
- **What the original shows** (CONFIRMED, `0x00A13DB0` → `0x009AB1B0`): it fills a table `SettlementLootingOptions`
  for the UI scripts with `Looting` and `Occupation` = {`DamagedBuildings` [{`Building`, `Damage` (the list value:
  the health after)}], `Loot`, `LowerOrder`, `UpperOrder`, `TownWealth`}, `Liberation` = {`FactionLocation`} only when a
  target exists, and `SettlementName` ("settlement, region"). Map them from the fields above. The shipped UI scripts
  answer through `OccupySettlement` (one call site, `analysis/worker3/lua_api.txt`; its argument is presumably the
  option index 0 loot / 1 occupy / 2 liberate as in `0x008C0310`: not checked).
- **The answer:** `CampaignCommand::ChooseCapture { choice: CaptureChoice::{Loot, Occupy, Liberate} }`, then
  `CampaignEvent::CaptureResolved { region, faction, choice, money }`. If the turn ends with the choice still open, the
  model occupies (PROVISIONAL; the original's screen is modal).
- **Damaged buildings:** `CampaignModel::can_repair(region, slot)`, `repair_cost(region, slot)` and the command
  `CampaignCommand::RepairBuilding { region, slot }` back the building panel's `being_repaired` / `can_repair` /
  `can_afford_repair` / `repair_cost` fields (`0x009C8190`); a repair shows as a construction item of the building's
  own level.

## 2. Settlement panel (construction and recruitment), round of 2026-10-04

Why it "did not do much": only the first building slot ever showed its upgrade options (the panel
selects slot 1 when it is built), and Paris's first slot (sAdmin2) needs a technology, so its option
was greyed. Every other slot shows its options when the pointer rests on it, through
template.BuildingFrame.lua's `SelectPassive`, which returns at once unless
`Construction.g_repair_construction_button` is set. Construction.lua is a `module(..., package.seeall)`
table, so that read falls through to the globals, and only layout.root.lua sets it (SETGLOBAL of
`Find("build_repair")`). Recruitment.lua reads `g_review_panel` and `g_recruitment` the same way.
INFERRED: the original runs the root layout's script in the global table. Ours keeps its own
environment, so `campaign_prelude.lua` makes `_G` fall back to the root environment (rawget, one level).

Flow (CONFIRMED from the scripts):
- Pointer on a built / empty slot → `SelectPassive` → root `SelectPassiveConstructionSlotExclusive(id)`
  → the slot's option cards (`<slot>_Upgrade<n>` / `Constructable<n>`) appear under it.
- Click on an option → `SelectExplicit`: type 4 (upgrade) with `affordable` and `tech_present` →
  `CampaignUI.BeginUpgrade(building_key, slot_key)`; type 3 → `BeginConstruction`; type 2
  (constructing) or `being_repaired` → `CancelConstruction(building_key, slot_key)`.
- `affordable` / `tech_present` = availability bits 1 / 2 (Construction.lua's CreateBuildingFrame).
- Recruitment card click → `RecruitUnit(character, manager, record)`; a queued card →
  `CancelRecruitment(item_ptr)`. The card keeps `unit_record` as the unit's details table for the
  unit card tooltip (Name, Class, Description, Men, Range, Accuracy, Melee, Charge, Defence, Morale,
  IsNaval, IsArtillery; card: Key, UnitLimit).
- Only the selected recruitment card is drawn wide with its cost (Recruitment.lua's
  `SelectRecruitmentCard` / `RecrutmentCardPositions_OSX`); the others are narrow and overlap.

Engine side (CONFIRMED from the exe unless tagged):
- `0x009FBAB0` fills a slot: `buildings` = the construction item's entry (`0x009C85E0`) or else the
  standing building's (`0x009C8320`), then one type-3 entry per option of an empty slot
  (`0x009C8920`); `upgrades` (type 4) only when the standing building's health is above 99.
- `0x00B43300` lists the options: nothing for a damaged building in strict mode, flag 2 otherwise;
  candidates = the standing level's upgrades or every level 0 for an empty slot; each must pass
  `0x008BE7F0`; missing technology = flag 8 and unaffordable = flag 1 (kept unless strict); the cost
  is round(cost × (100 + modifier) / 100).
- `CanFactionBuildBuildingLevel` `0x008BE7F0` (this = faction, arg = level record), CONFIRMED order:
  (1) `0x008CEEF0`: a faction whose `+0x514` is 0 is refused (meaning not decoded, not modelled);
  (2) `IsBuildingLevelNotScriptRestricted` `0x009CDA90` on the restriction set reached as faction
  `+0xA8` → `+0x8` → `+0xFA8` (count `+0x10`, record pointers `+0x14`, a linear scan): one
  campaign-wide set, the scripts' `add_restricted_building_level_record` list -- a listed level is
  refused; (3) then the level record's two keyed sets: +0x124 keyed by the faction and +0x140 keyed by the faction's culture
  record. INFERRED sets: `building_faction_variants` and `building_culture_variants` (culture =
  `cultures_subcultures` of the faction's subculture). In the shipped DB each nation's prestige
  building has only its own faction row (Paris offers only the Arc de Triomphe). Model:
  `CampaignModel::building_permitted`, which reads the restricted set from
  `World::restricted_buildings` (the model owns it; the script calls, the panels, the commands and the
  AI all read that one list).
- The construct command applies the same test (CONFIRMED): `CCQ_BUILDING_CONSTRUCT` (registered by
  `0x0041F580`) is executed by `ProcessBuildingConstructCommand` `0x00931C80`, which reads the slot and
  level key and calls `StartSlotConstructionIfOffered` `0x00B13D50`: that rebuilds the slot's option
  list (`BuildSlotConstructionOptionList` `0x00B43880` → `0x00B43300`, flags 1, 1) and starts the item
  (`0x00B13DD0`) only when the requested level is in it; otherwise nothing happens. So a script's
  `BeginConstruction(restricted_key, …)` builds nothing. Model: `can_build` → `building_permitted`.
- Repair fields, `PushSlotRepairFieldsToLua` `0x009C8190` (slot = card `+0x1E0`), CONFIRMED:
  `being_repaired` = `0x00B4DA20`, `can_repair` = `0x00B1A6B0` and not being repaired,
  `can_afford_repair` = `0x00B16430`, and `repair_cost` = `CalculateSlotRepairCost` `0x00B66410`
  **unconditionally**: a building already under repair still shows its repair cost. `0x00B66410`
  gives 0 only for a slot with no building and never looks at the repair state. Ours: `slot_info`
  shows `CampaignModel::repair_cost` whatever `can_repair` says.
- Map fort upgrade (`FindNextFortUpgradeLevel` `0x00B430C0`, CONFIRMED): it reads the fort's own level
  index (`CampaignFort +0x188`, an int), never a level record of the standing level, so an unknown
  standing key cannot hide the upgrade. Ours: `map_fort_standing` returns (level index, key) and
  `map_fort_next_level` takes the index. The upgrade command itself never runs in the shipped exe
  (`ProcessFortUpgradeConstruction` `0x00B4E620` stops at the constant-false `0x0047BA10`).
- A construction item whose slot does not exist: the original cannot hold one (the item is a child of
  the slot's own `BUILDING_MANAGER`, CONFIRMED save structure), so it has no drop or refund path.
  Ours drops it at the region's turn without refund (PROVISIONAL) and reports
  `CampaignEvent::ConstructionItemDropped` once, a diagnostic the app logs and the scripts never see.
- `CancelConstruction` `0x009E0F10` → `0x009B7AA0` queues a command (serializer vtable `0x0136AB78`)
  with the slot and a flag byte 1. The full refund is INFERRED. Model: `CampaignCommand::CancelConstruction`.
- `TriggerBuildingCardSelectedEvent` `0x009FA0D0` posts a UI event (vtable `0x0136C30C`). Ours is a no-op.
- The scripts' restricted building list: EpisodicScripting.lua's `OnUICreated` runs `InitialiseCampaign`
  when `context.string == "Campaign UI"`. On a new eur_napoleon game that is 55 levels (tutorial and
  Peninsular chains), the same count as the original save's `BUILDING_RESTRICTIONS`. Our HUD fires
  `UICreated` once it exists.

Open: demolish (no dismantling in the model), naval recruitment / infrastructure / agents tabs
  (generator UNKNOWN for naval, show-condition UNKNOWN for agents — see
  `CHARACTER_UI_HOOKS.md §H5` for Ghidra trace findings), technology icons on options
  (`technologies` empty).

## 3. Capture screen

- The engine calls the root's `SettlementLootingOptions(t)`. layout.root.lua opens the
  `settlement_captured` panel through PanelManager with `Initialise(t)`.
- `t` = {SettlementName, Looting, Occupation, Liberation?}: see "Capture screen: what the model gives".
- The buttons call `CampaignUI.InformLootingSelection("loot" | "occupy" | "liberate")`, then `Close`
  (the root's `ClearSettlementLootingOptions`).
- Ours: `UiScriptHost::campaign_capture_screen` opens the panel once per pending human capture
  (`CampaignModel::pending_capture`). `InformLootingSelection` becomes `ChooseCapture`.
- INFERRED: `FactionLocation` is the liberated faction's name; LowerOrder / UpperOrder are the
  previewed public order, rounded.
- Fixed on the way: `obj:SequentialFind` returns a component object (the script calls SetState on it).

## 4. Negotiation screen (`ui\campaign ui\diplomacy_panel`), 0-E sandbox 2026-10-04

Read-only probe (temporary `ntw_formats` test, deleted after; Steam never written). The
negotiation screen is the `diplomacy_panel` layout: it hosts the faction overview
(`faction_left` / `faction_right`: titles, leader banners, flags, protectorate/allies/trade/
enemies rows, treaties, power/wealth), the offer lists (`offers_demands`: `list_offers` /
`list_demands`), the two negotiation button rows, the ten deal subpopups (`subpanel_group`),
the diplomat (`speech_bubble`, `clip_box` → `portrait`) and four radar maps (`map`: egypt,
europe, italy, spain). Files (all Version039, all parse to the last byte):
`ui\campaign ui\diplomacy_panel` (351 components), `ui\templates\diplomacy_button` (3),
`diplomacy_item_regions` (5), `diplomacy_item_text` (3), `diplomacy_region_tooltip` (11);
drivers `diplomacy_panel_scripts/{diplomacy_panel, diplomacy_tech_offer, offers}.luac` plus
`layout.button_diplomacy.luac` and `template.diplomacy_{item_text, region_tooltip}.luac`.

Layout fields (meanings per UI_LAYOUT_FORMAT.md; the DrawMode inherit-when-zero rule is
CONFIRMED `0x01027D20`, inherited through `unknown_140 == 0` parents):
- **`unknown_da` = ClipChildren: 1 on exactly 21 components**, all with clip-style ids:
  `list_clip` x8 (offer/demand lists, region offer/demand lists, technology offer/demand
  lists, join-war offer/demand lists), `Blank` x4 (the radar-map masks), `list_box` x4 plus
  its child `bg` x4 (the declare-war and war-declared ally lists clip twice: box and
  background), `clip_box` x1 (diplomat portrait). 8+4+4+4+1 = 21.
  Everywhere else 0, including all template components. (Elsewhere a `list_box` under a
  `list_clip` is 0; only the declare-war/war-declared ones clip themselves too: data fact.)
- **`unknown_e5` = UseGlobalClicks: 0 on all 351 + 3 + 5 + 3 + 11 components** (as in every
  shipped layout: the screen catches no out-of-bounds clicks).
- **`unknown_140` = DrawMode: 0 on every component**, so the effective mode is 0 (normal,
  UI-scaled) across the whole screen by inheritance; no unscaled (1) or full-screen (2).

Button wiring (CONFIRMED inline layout Lua, `parent:LuaCall("<fn>")` on `diplomacy_panel`,
a mechanism the host already implements): offer row ThreatOfForce / SendOffer / CancelOffer;
answer row AcceptOffer / CounterOffer / DeclineOffer; subpopups OkRegions, OkDeclareWar
(+`true` variant), OkWarDeclared, AcceptMilitaryAccess, OkStateGift, OkPayments (alliance,
trade and payments), OkTechnologies, OkStanceDeclaration; every cancel clears with
`ClearSubPopups`; region rows report `RegionSelectionChange`; the offer-item templates
`RemoveDiplomacyItem` / `ShowRegion`. Root events: OnUpdatePulse, OnDock→DockHudRelative,
OnDestroyed; close goes through `ParentPopup` LuaCall `ClosedByCloseButton`.

Why the screen is still mostly unwired: the reads it needs are already in
`crates/ntw_script/src/ui/campaign.rs` (RetrieveFactionListForDiplomacy,
RetrieveDiplomacyDetails, RetrieveExistingTreaties, RetrieveDiplomaticOpinions,
RetrieveRemainingMilitaryAccessTurns, StateGiftValues, MinisterPortraitPath) and the
mechanisms (LuaCall, `Parent("id")`, sliders, ClipChildren, tooltips, text entry for the
payment amounts) are proven by the settlement/front-end patterns. What is left is the
panel-script side: the LuaCall bodies (SendOffer, AcceptOffer, OkDeclareWar, ... in
`diplomacy_panel.luac`), how the script gets its `negotiation` object (2 construction
sites), and the shapes of RetrieveDiplomaticStanceString / InviteAlliesIntoWar (1 call site
each). The wired element is the CONFIRMED layout contract itself: test
`diplomacy_negotiation_layout_fields` (`ntw_formats/tests/real_install.rs`) locks version 39,
the 21-clip set, zero e5/140, the button->LuaCall map, the subpopups, the clipped portrait
and the radar maps, from the original files (no placeholders).

### 4.1 The 20 `negotiation:*` methods, per shape (Ghidra, own copy NR-sb-ghidra-0e)

Round N+1 ran the wrappers in one batch (targets `neg_targets.txt`, output
`ghidra_evidence/0e/negotiation_decomp.txt`, 4,643 lines; batches 2-4 in `gh__neg_decomp*.txt`).
There are **twenty** receivers, not nineteen: `IsNegotiation` (0x009BD7C0) exists but no shipped
script calls it, which is where the "19" came from. The object is the tolua userdata
`UIDiplomacyNegotiation` (ctor **0x00A102B0**, 0xB4 bytes, built by 0x0099AF70); every accessor
returns nothing while the counterparty at **+0xAC** is 0, so a negotiation with no counterparty is
silent, as the host reproduces.

| # | shape | wrapper (bytes) | what the wrapper itself shows | native behind it | verdict |
|---|-------|-----------------|------------------------------|------------------|---------|
| 1 | `BuildPossibleActions` | 0x009B5220 | no args read, no literal pushed | none (0 callees) | superseded by §4.6: CONFIRMED shape `{OffersAndDemands, Unilaterals}` (the address was not a function in Ghidra, hence "0 callees") |
| 2 | `Propose` | 0x009BF3C0 (2285) | log literals `"Offered"`, `"Demanded"`, `"demanded"` | 40 callees, all on the engine's own deal object; no path to 0x00B449F0 from here | shape UNKNOWN; it is the exe's real entry, host maps it to `accept_deal` (INFERRED) |
| 3 | `ProposeDeal` | 0x009BFCF0 (142) | no arg reads, no literal | 7 callees (`0x005DC270`, `0x01055840`, ...) - a validation/error path | UNKNOWN |
| 4 | `Cancel` | 0x009B7A30 (112) | no arg reads | `0x008FCFE0`, `0x008D1F30`, `0x00592C00`, `0x005A96C0`, `0x0049A1A0` | UNKNOWN |
| 5 | `End` | 0x009BA850 (114) | no arg reads | `0x008FD010`, ..., `0x009B8560` | UNKNOWN |
| 6 | `Finished` | 0x009BB8B0 (72) | no arg reads | `0x01056610`, `0x01058750`, `0x006CCA70` | UNKNOWN (one bool out, INFERRED) |
| 7 | `BuildOfferAndDemandStrings` | 0x009B48B0 | literals `"Offers"`, `"Demands"`, `"Action"`, `"Regions"` (x2); the row's action id from `0x006A3110` is tested **`== 6`** for the region list | 0 callees: a pure reader of the engine's rows | superseded by §4.6: three return values (offers, demands, regions text); "Offers" / "Demands" are not keys |
| 8 | `ProposerId` | 0x009BFD80 (105) | wraps the faction userdata (`0x008E5060`) | `0x01056500`, `0x004F1090`, ... | CONFIRMED one value out; the host returns the faction key (INFERRED) |
| 9 | `PrepareCounterOffer` | 0x009BF120 (112) | no arg reads | `0x008FCFF0`, `0x008D1F30`, ... | UNKNOWN (host: false, AI evaluation not modelled) |
| 10 | `CanPropose` | 0x009B7990 (72) | no arg reads | `0x00C1C080`, `0x01056610`, `0x01058750` | UNKNOWN (host INFERRED: a non-empty deal) |
| 11 | `CanThreaten` | 0x009B79E0 (72) | no arg reads | `0x01056610`, `0x01058750`, `0x00C1C0F0` | UNKNOWN (host INFERRED: not already at war) |
| 12 | `AcceptOffer` | 0x009B1A70 (112) | no arg reads | `0x008FCFC0`, `0x008D1F30`, `0x00592C00`, `0x005A96C0`, `0x0049A1A0` | UNKNOWN; host -> `accept_deal` (INFERRED) |
| 13 | `DeclineOffer` | 0x009B9520 (112) | no arg reads | `0x008FD000`, ... | UNKNOWN; host clears the deal (INFERRED) |
| 14 | `RemoveAction` | 0x009C0F70 (72) | no arg reads | `0x0044D320`, `0x01058750`, `0x00C5C780` | UNKNOWN; the host filters `possible_actions` by name (INFERRED) |
| 15 | `TradeableTechnologies` | 0x009C5AA0 (1002) | literals `"Proposer"`, `"Recipient"`, `"FactionKey"`, `FUN_0044DE40("tech_status", 0)`; rows built by the card helper **0x009ABB50** (also sets `BuildingLevel` and `Data/UI/Campaign UI/Technologies/%S.tga`) | 18 callees | **CONFIRMED** shape; which side may give what UNKNOWN; two row keys (string constants 0x009C5B9B / 0x009C5BC0) still unread |
| 16 | `TradeableRegions` | 0x009C5770 | literals `"Proposer"`, `"Recipient"`, `"CurrentlyOffered"`, `"CurrentlyDemanded"`; returns nothing unless campaign flag +0xF9C and counterparty +0xAC are set | 0 callees | **CONFIRMED** shape; the region list per side INFERRED |
| 17 | `MaxPlayerPaymentAllowed` | 0x009BE720 (56, `PushMaxPlayerPaymentAllowedFromScript`) | no arg reads | `GetFactionEconomyTreasury` 0x00BCAFE0 on the local player's faction (0x009BF110) | **CONFIRMED** one number out, the treasury (round 3, "Payment caps") |
| 18 | `MaxOppositionPaymentAllowed` | 0x009BE6B0 (103, `PushMaxOppositionPaymentAllowedFromScript`) | no arg reads | `0x00BCAFE0`, `0x009BF110`, `0x004CE450` / `0x004631B0` (+0x18 / +0x1C) | **CONFIRMED** one number out, the non-player side's treasury |
| 19 | `IsNegotiation` | 0x009BD7C0 (98) | no arg reads | `0x0044D320`, `0x01056610`, `0x01058750` | UNKNOWN; not called from any shipped script |
| 20 | `FactionListsForStanceDeclarations` | 0x009BAB90 | literals `"offered"`, `"demanded"`; compares the pending action against the CONFIRMED names **`request_join_war`**, **`break_trade`**, **`break_alliance`** (`FUN_004F0F30`); rows also set `"Selected"` | 0 callees | **CONFIRMED** shape + the 3 names; how each splits the factions INFERRED |

### 4.2 The native deal appliers (round N+1)

The panel never applies a deal itself: the rows live in an engine-side container and four
appliers walk it. All addresses CONFIRMED in this round's own copy (`e6_appliers_out.txt`,
`e8_items_out.txt`).

- **The container.** A vtable call at `+0x38` returns the deal manager; `*(manager + 0x120)` is the
  deal itself: `+0x14` = row count, `+0x18` = first row (a contiguous vector), and **two optional
  single items at `+0x20` and `+0x24`** (a GPT schedule and a technology trade are the obvious
  candidates, UNKNOWN which). Each row carries an "applied" int at **`+0x1E8`**.
- **The four appliers**, all with the same walk (rows, then `+0x24`, then `+0x20`, then the region
  transfer): **0x00B58C00** (offer; the one with a region argument), **0x00B58A30** (demand; only
  caller 0x00BCA430 <- 0x00BA90E0), **0x00B58560** (same plus an undo record `0x00AAA790` and a loc
  string, id `0xFD`), **0x00B58890**. Per row: if `row->+0x1E8` then `0x00B1A760(1)`, then
  `0x00A6CBE0(0)` for that row and for each of the two single items.
- **The per-item step** `0x00A6CBE0` is 11 bytes: a forward to **`0x00B1A790(flag)`**, the deal
  commit. `0x00B1A760(flag)` drains the pending list at `+0x4C`/`+0x50` through `0x00B1A820(item,
  flag)`, which calls the item's vtable slot `+0x1C`, applies it and erases it from the vector.
- **Payments / tribute -- CONFIRMED.** Inside the commit: if `0x00B4E4B0()` and the flag agree and
  the pending deal's **`+0x14`** amount is non-zero, it runs **`0x00BB3810(amount, 3)`** (the same
  money mover as the capture loot path, `0x00BB3810(money, 0)`), then frees the deal
  (`0x0126E016(deal, 0x18)`), clears the pointer and notifies two listeners.
- **Region transfer -- CONFIRMED.** **`0x00B449F0(faction, region_item, 1, 1, 0)`** (797 bytes) is
  applied once, after the rows, by all four appliers: the demand path passes a null
  `region_item`, the offer path the region it was handed. Its five callers are those four plus the
  24-byte wrapper **`0x00B58A10(faction, a, b)`** = `0x00B449F0(faction, 0, 0, a, b)`, which is the
  liberate path (`0x00B4F090`, CAMPAIGN_FIDELITY.md Capture).
- **Technologies -- UNKNOWN.** `0x00A6CBE0`/`0x00B1A790` only commits money; the per-row-type work
  lives behind the row vtable (`0x00B1A820` -> vtable `+0x1C`) and **no technology-granting address
  has been traced from the deal path**. No script-facing name exists either
  (`worker3/lua_api.txt` has no technology-transfer entry). The model has no `GrantTechnology`
  command, so the host drops the row.
- **Stance / access / gift rows -- CONFIRMED addresses, already ported by 0-G:** trade 0x00B55090,
  break trade 0x00B29BB0, embargo 0x00B28DB0, military access 0x00B44550, cancel access 0x00B67BD0,
  state gift 0x00B44590, protectorate 0x00B105C0 (`treaties.rs`).
- **What the host does** (`accept_deal`, `campaign.rs`): every offer and demand item that carries a
  model command becomes a `CampaignCommand::Diplomacy`; regions, technologies and payments are
  dropped, each with the address it would need named in the function's doc comment
  (PLACEHOLDER). Nothing constructs an item yet (`NegotiationItem` is `#[allow(dead_code)]`),
  because no CONFIRMED method takes the deal in from Lua -- the exe mutates the rows when the
  panel's subpopup OK buttons run, and those bodies are still open (item 2 below). Mapping
  `Propose`, `ProposeDeal` and `AcceptOffer` all onto `accept_deal` is INFERRED: none of the three
  wrappers reads an argument or pushes a literal, so nothing in the exe distinguishes them.

### 4.3 Naval recruitment tab generator -- no naval generator of its own; the one recruitment generator has a naval branch

Found with the string-ownership technique (own a handler by the name it pushes), not by scanning
call sites. `strs:` over the whole exe for the `Generate` names gives the complete set of
registered panel generators and their registration functions:

| generator name | string | registered at |
|----------------|--------|---------------|
| `GenerateRecruitmentPanel` | 0x0136D700 | **0x009C7C70** |
| `GenerateFortConstructionPanel` | 0x0136DA30 | **0x009C7B50** |
| `GenerateConstructionPanel` | 0x0136D9F0 | 0x009C7CF0 and 0x009C77A0 (two managers) |
| `GenerateAgentsPanel` | 0x0136D594 | 0x009C7AE7 |
| `GenerateArmyPanel` | 0x0136D63C | 0x009C7710, 0x00986650, 0x00986780, 0x0098BCF0 |
| `GenerateNavyPanel` | 0x0136D6E0 | 0x009C7BE0 |

**No `GenerateNaval` registration exists** (0-G's H5/T1 asked exactly this), and no other
`Generate...Recruitment...` either, so no naval recruitment generator is registered under that
prefix. CONFIRMED inside the recruitment implementation **0x009FE7B0** (14,199 bytes, the owner of
`recruitable_units` 0x0136D734, `recruitable_unit` 0x0136D748, the `recruitable` card-id marker
0x0136D7A0, `recruitment_capacity` 0x0136D87C and `Available` 0x0136D71C):

- a switch on `[obj+0x8]` (values 0..13, jump table 0x00A01F28) picks a category name, and one case
  pushes **`naval`** (0x0130CFFC, pushed at 0x00A01217);
- every card gets **`is_naval`** = (`[card+0xA0] != 0`) (0x013305C0, pushed at 0x009FF497,
  0x00A004E3, 0x00A00F21);
- the registration 0x009C7C70 stores the generator pointer at `+0xA0` and a bool at `+0xA4`
  (unguarded), while `GenerateNavyPanel` (0x009C7BE0) stores its pointer at `+0xA8` and writes the
  **same** `+0xA4` bool -- that bool is the likely land/naval selector (INFERRED);
- the panel's `naval_recruitment_tab` component is CONFIRMED (exe string 0x013CC71C, right beside
  `recruitment_tab` 0x013CC70C, and in the tab loc-string order army, navy, recruitment,
  naval_recruitment, agents, infrastructure, siege, construction).

**Answer to 0-G's H5/T1 question, with 0-G's own evidence folded in.** 0-G (read-only,
`NR-sb-0g/analysis/fidelity/CHARACTER_UI_HOOKS.md` H5/T1, its own Ghidra copy `NR-sb-ghidra-0g`)
traced the settlement tab builder `FUN_0099A200` and found it adds `construction_tab`,
`recruitment_tab` (if `recruitment_points(region, false) > 0`), `infrastructure_tab`, `army_tab`,
`agents_tab` -- **never** `naval_recruitment_tab`, which it places only on the character panel,
where the generator is `GenerateNavyPanel`. Taking both traces together:

- CONFIRMED: no naval recruitment generator is registered in the exe at all;
- CONFIRMED: the settlement's recruitment generator itself carries naval-ness (the `naval`
  category, the per-card `is_naval`), so the ship cards need no second generator;
- INFERRED: the land/naval selector is the manager bool at `+0xA4` (0-G saw no caller passing a
  "naval" argument, which fits a manager-side flag rather than an argument);
- UNKNOWN, and NOT refuted by this round: 0-G's `CampaignShipCard` template is referenced by no
  script, and a generator named without the `Generate` prefix would not show in the table above.
  "Who instantiates `CampaignShipCard`, and whether the character panel's `naval_recruitment_tab`
  reaches 0x009FE7B0 at all" stays open -- and it is a panel-script (luac) question, not another
  exe question.

NOT WIRED, and why: `recruitment_info` calls `recruitment_points(region, false)`, land only. The
naval capacity is modelled (`naval_recruitment_points` per port, `0x00B61EE0` CONFIRMED,
`recruitment_points(region, naval)`) and unused. Wiring needs the selector's writer (the manager's
`+0xA4`), i.e. open item 6.

### 4.4 Infrastructure tab generator -- CONFIRMED, still unwired

`construction_manager.GenerateFortConstructionPanel` (lua_api.txt:3943) is registered at
**0x009C7B50** (guarded on the manager's `+0xAC`, generator pointer `+0xB4`, bool `+0xB8`). Its info
builder is **0x009FDFE0** (282 bytes), which logs `"Fort tables"` and pushes

- **`forts`** = the table from **0x009FC010** (1,420 bytes, logs `"Fort table"`; one row per fort,
  each row built by **0x009C9170**),
- **`fort_ptr`** = the panel's fort object (`+0x70`; corrected 2026-10-07 -- the `1` is the push's flag byte),
- **`controlable`** = the caller's bool (string 0x0136D630).

**0x009C9170 is the same row builder the construction panel uses** (its only caller is 0x009FC010,
and `construction_info` already produces its keys): `percent_complete`, `building_key`,
`description`, `long_description`, `affordable`, `upkeep`, `turns_to_completion`, `image`,
`being_repaired`, `can_repair`, `can_afford_repair`, `health`, `repair_cost` -- CONFIRMED. One gap:
our entries keep `short_description` and have no `description`, so that key is missing (UNKNOWN
which text the exe puts there). `can_build_fort_status` (0x0136D6B8) and `build_fort_cost`
(0x0136D6D0) exist as exe strings with **no code reference at all**, i.e. the fort panel's script
looks them up and the engine never fills them (or fills them from a data table, UNKNOWN).

Why it is still unwired: the fort panel is a selection of its own -- the exe's `campaign_fort`
entity (0x00965550), `player_fort` / `non_player_fort` (0x00DE55F0) -- and `CampaignSelection` has
only `None` / `Character` / `Settlement`. Adding a fort kind plus the fort-selection path is open
item 7; it is not a one-line wiring job and guessing the row set would be a confident stub.

#### 4.6 Round N+2: the four contracts, wired (and what the panel scripts then settled)

Round N+2 was told to wire, not to re-derive: demolish (0-G's command), the fort tab, the naval
recruitment tab and the three remaining deal appliers. Doing it needed four argument shapes the
scripts own, and the .luac answered all of them. Dumps: `ghidra_evidence/0e/luac__*.txt`
(`README_luac_n2.md`), read-only out of `data.pack`; the disassembler was a temporary example,
deleted.

**Every construction / fort call, CONFIRMED from the shipped bytecode.** The frame's click handler
(`ui/templates/template.buildingframe.luac:417`, upvalue `g_fort_ptr`), `Construction.lua:366`
(`DemolishCurrentSelection`), `Construction.lua:383` (`RepairCurrentSelection`) and
`ui/campaign ui/building_information_scripts/building_information.luac:36`:

| call | arguments | our binding |
|---|---|---|
| `CanDemolishBuilding` | `slot_key` | `can_demolish(region, slot)` |
| `DemolishBuilding` | `building_key, slot_key` (Construction.lua) / `slot_key` (building_information.luac) | `DemolishBuilding { region, slot }` |
| `BeginConstruction` / `BeginUpgrade` | `building_key, slot_key` | `ConstructBuilding` |
| `CancelConstruction` | `building_key, slot_key` | `CancelConstruction` |
| `RepairBuilding` | `building_key, slot_key` | `RepairBuilding` |
| `UpgradeFort` / `CancelUpgradeFort` / `CancelFortRepair` / `DemolishFort` / `RepairFort` | `fort_ptr` (one argument) | nothing (2026-10-07): they act on the map fort -- `UpgradeFort` is switched off in the exe (CONFIRMED), the rest are PLACEHOLDER no-ops (the model's fort has no building state); the walls use the ordinary slot calls |
| `BuildFort` | `g_general` (`ui/army.luac:382`, `g_button_fort`) | nothing: the exe's `CCQ_CHARACTER_BUILD_FORT` never builds (see "Walls (campaign bug 2") |

Two things this settled that N+1 had as UNKNOWN:
- **`UIComponent:GlobalExists(name)` is a getter**, not a predicate (it returns the value): it is
  how every frame's `slot_key` / `building_key` / `type` / `affordable` / `tech_present` /
  `repairable` / `being_repaired` / `dismantling` reaches the click handler.
- **The fort actions carry no region**: their one argument is `fort_ptr`, the fort object (not the
  constant 1, corrected 2026-10-07). They address the map fort, not the settlement walls.

**The fort panel's info and rows (A4.4, now closed).** `GenerateFortConstructionPanel` is
`Construction.lua:879`, and `ResetConstructionPanel` (`:61`) reads **`fort_ptr`**, **`controlable`**,
**`faction_key`**, `infrastructure` and (if present) `slots`, with `g_num_slots` defaulting to **1** --
so the fort panel is a one-slot panel and our info now sets `faction_key` and `infrastructure` too.
The loop over `forts` handles **only** row types `BUILDING_ICON_TYPE_BUILT` (1), `CONSTRUCTING` (2)
and `UPGRADE` (4): there is **no `CONSTRUCTABLE` (3) branch**, and the function ends with
`SelectPassiveConstructionSlotExclusive(1)` / `SelectExplicitConstructionSlotExclusive(1)`, which
assert `g_building_slot_components[1]` at **`Construction.lua:624`**. An empty fortification is
therefore shown as a `BUILT` frame with no building key and the buildable levels as upgrade cards.
Per-row keys the script reads: `type`, `name`, `image`, `description`, `long_description`,
`building_key`, `slot_key`, `health`, `percent_complete`, `being_repaired`, **`repairable`**,
`can_repair`, `can_afford_repair`, `repair_cost`, `turns_to_completion`, and for upgrades `cost`,
**`affordable`**, **`tech_present`** as separate keys (not the construction panel's packed
`availability` bits). PROVISIONAL, and it is a real gap: the script compares `being_repaired` and
`dismantling` with the **string** `"true"`, so our booleans do not match and a repair click falls
through to the CONSTRUCTING branch; whether the engine's `SetGlobal` stringifies is UNKNOWN, and the
settlement panel's rows have always been booleans here.

**The tab keys are engine-side (A4.3, now closed).** No shipped script names
`naval_recruitment_tab`, `infrastructure_tab` or `recruitment_tab`; `layout.root.luac` names only the
*generators* (`GenerateRecruitmentPanel`, `GenerateFortConstructionPanel`,
`GenerateConstructionPanel`, `GenerateAgentsPanel`, `GenerateArmyPanel`, `GenerateNavyPanel`). So the
tabs are created by the engine's builder `FUN_0099A200` / `FUN_00985F40`, which is what 0-G traced --
CONFIRMED from the other side. `CampaignShipCard` is named by **no shipped script** either, so who
instantiates it stays UNKNOWN (open item 6).

**The army panel's fort button (A4.5 item 7, closed).** `ui/army.luac:213` (`GenerateArmyPanel`)
reads `info.can_build_fort_status` -> `g_fort_building_status` and `info.build_fort_cost` ->
`g_fort_cost`, `AbleToBuildFort()` (`:817`) is `g_fort_building_status == FBS_ABLE`, and
`ShowArmyButtons` (`:1058`) sets `g_button_fort`'s state `inactive` / `normal` and its tooltip from
`g_fort_cost`. Our `force_info` fills both keys with `FBS_UNABLE` (1) and -1, the constants the
exe's army info builder `0x009FCFB0` writes (CONFIRMED 2026-10-07, see the fort section).

**The deal appliers (A4.2, closed for three of four row types).** `deal_item_commands` maps each row
to its command, with a unit test (`negotiation_rows_map_to_their_appliers`):
- regions -> `CampaignCommand::TransferRegion` (**`0x00B449F0`**, the model's new command: the
  settlement changes hands exactly as a capture does, because the four deal appliers *are* the
  capture appliers);
- payments -> `StateGift(amount)` for the lump sum (`0x00BB3810(amount, 3)`) and
  `RegularPayment(amount, turns)` for the schedule;
- stance / access / gift -> `Diplomacy` as before;
- **technologies still dropped**: no granting address is reachable, so no address is invented.

### 4.5 Open items

1. RESOLVED (§4.6): the panel constructs its own negotiation, `UIDiplomacyNegotiation(player,
   opposing)`; nothing hands it one.
2. Panel-script bodies: SendOffer/AcceptOffer/CounterOffer/DeclineOffer/ThreatOfForce/CancelOffer,
   OkRegions/OkDeclareWar/OkWarDeclared/AcceptMilitaryAccess/OkStateGift/OkPayments/OkTechnologies/
   OkStanceDeclaration -> which engine calls, and which of them mutate the deal rows the host reads
   (the model has `CampaignCommand::{DeclareWar, MakePeace, Diplomacy}` + `call_allies` ready).
   Needs luac decompilation, not more Ghidra.
3. `RetrieveDiplomaticStanceString` and `InviteAlliesIntoWar` shapes (human-ally offer UI is the
   model's open end of `call_allies`).
4. AI acceptance (negotiation evaluation, `diplomacy_options` permissions #18, table 0x01459080):
   0-G hook.
5. The two unread row keys of `TradeableTechnologies` (string constants 0x009C5B9B / 0x009C5BC0) and
   the action ids other than 6 in `BuildOfferAndDemandStrings`.
6. Naval recruitment: WIRED (N+2, `naval_recruitment_tab` on a naval character, the generator's own
   `naval` category and per-card `is_naval` as the selector, PROVISIONAL on the manager's `+0xA4`
   bool). Still open: the `+0xA4` writer, and `CampaignShipCard`, which **no shipped script names**
   (CONFIRMED by the N+2 name scan, `ghidra_evidence/0e/luac__name_index.txt`) -- so whether the
   naval cards use it is UNKNOWN, and so is whether the character panel's tab reaches `0x009FE7B0`.
7. Fort construction panel: WIRED (N+2): `infrastructure_tab`, `CampaignSelection::Fort`,
   `fort_info` with the `0x009FDFE0` keys and the `0x009C9170` rows, and the fort's build / upgrade /
   repair / cancel / demolish actions on `FORTIFICATION_SLOT`. Left: the `description` row key (we
   repeat `short_description`), the `+0xA4`-style bool behind `g_controlable`, and whether the
   exe's `SetGlobal` stringifies booleans (see A4.6).
8. Technology deal item: the row vtable behind `0x00B1A820` (vtable `+0x1C`), the one applier still
   without an address. The per-turn schedule is likewise one of the container's `+0x20` / `+0x24`
   single items.

### 4.6 Opening and closing the negotiation screen; Power / Wealth; treaties (2026-10-08)

User bug 2026-10-07 (Britain vs France, late June 1805): the X did nothing, Power 0 / Wealth 0,
"Test" under Current Treaties. Static trace (ghidra-mcp, names and plates saved in Ghidra) plus
`luac_dump` of `diplomacy_panel.luac` and `layout.root.luac`.

**How the screen opens and closes (CONFIRMED unless tagged).**
- Diplomatic relations' "Open Negotiations" (`OpenNegotiations`, scroll.lua:230) calls the root's
  `ToggleDiplomacyPopup(faction)` (layout.root.lua:1123): only while the root's local
  `can_open_diplomacy` is true (starts true); it sets it false and opens `diplomacy_panel` with
  `Initialise(faction)`. The root's `EnableDiplomacy` (:1119) sets it true again.
- `Initialise` (diplomacy_panel.lua:143) fills both faction columns (`InitPanel`), then
  `negotiation = UIDiplomacyNegotiation(player, opposing)` (:176) and ends with
  `SetCloseable(false)` (:182). `SetCloseable(b)` (:1782) sets `g_closeable` and shows / hides
  `gilt_corner_TR`, the X's frame. So right after opening the X is hidden and inert.
- The X (`button_close`, `OnMouseLClickUp -> OnSelect`) calls `ClosedByCloseButton` on the address
  the main chunk put in its `ParentPopup` property (`Component.SetProperty("gilt_corner_TR.
  button_close.ParentPopup", Address)`, :36). `ClosedByCloseButton` (:1730) does nothing unless
  `IsCloseable()` (`g_closeable ~= false` and not multiplayer); then `negotiation:End()` when the
  player proposed or the negotiation is finished, else `negotiation:DeclineOffer()`, then
  `ClosePopup()` (root `ClosePopup`).
- The constructor: `CreateUIDiplomacyNegotiationFromScript` 0x00A102B0 →
  `InitializeUIDiplomacyNegotiationListeners` 0x0099AF70. The 0xB4 object stores the calling script
  context at +0xB0 (0x01058750) and registers listeners on the campaign event hub
  (`*(*(0x015C493C)+0x9FC)+8`): +0x38 on +0x360 (AI reply, `NotifyPanelNegotiationAIReply`
  0x00A14950: kinds 1/2/4 → `AIEndedDiplomacy(text)`, 3/6 → `AIResponse(text)`, 5 →
  `ConfirmWarDeclared(text, factions)`), +0x54 on +0xB28 (`InitialiseDeclareWar`, 0x00A14740), +0x70
  on +0x378 (`ClosePopupSoftly`, 0x00A14900). Two-key form (top argument a string): +0x00 on +0x318
  (`NotifyPanelNegotiationStarted` 0x00A147A0), looks the factions up (0x00955B70) and queues the
  open-negotiation command (functor 0x0136AB84). One-argument form (`RequestDiplomacy`, a pending
  move): the argument becomes the counterpart +0xAC.
- The campaign's negotiation (`InitializeCampaignNegotiationGreetings` 0x00BF5A60, built by
  0x008AF620 into campaign +0xF9C) posts hub +0x318 (negotiation, proposer, greeting text) when the
  proposer is human. 0x00A147A0 then LuaCalls, in the creating context,
  `InitialiseNegotiation(text, proposer == local player, false)`; that builds the option buttons
  and ends with `SetCloseable(player_init == true)` (:525), which shows the X. So the X works only
  after this event, which comes through the command queue, after `Initialise` returned.
- Greeting text: key `receive_` + attitude (table 0x015F2910 indexed by 0x00B0DBA0) with the
  recipient's culture / government parts (`diplomacy_strings_string_<culture>_<gov>_receive_<att>`,
  e.g. "Greetings. Courtesy demands ..."), resolved by `ResolveDiplomacyNegotiationString`
  0x00C55CC0: if the faction has a `diplomacy_negotiation_faction_override_strings` row, one
  campaign-RNG draw (+0xFB8, LCG ×0x343FD +0x269EC3, < 0.5 picks the override). The exact key
  assembly order is not read out (INFERRED from the loc keys).
- `End()` (0x009BA850, `EndNegotiationFromScript`): queues `CCQ_DIPLOMACY_END_NEGOTIATION`, then
  `UnregisterUIDiplomacyNegotiationListeners` 0x009B8560 removes every listener of the object and
  clears +0xAC and +0xB0. No +0xAC guard, so a second `End()` queues a second command.
- The lifecycle, from the command executors (round 2, CONFIRMED; registrations 0x00420600-0x00420B00,
  executors made functions and named): `CCQ_DIPLOMACY_END_NEGOTIATION` → 0x00932F20 →
  `EndCampaignNegotiation` 0x008BC5D0, which, only while campaign +0xF9C is set, posts hub +0x378,
  deletes the negotiation and clears +0xF9C (so a second END posts nothing); hub +0x378's HUD
  listener (vtable 0x0136B5B4 → 0x00A14190, HUD ctor 0x0098C2F0) LuaCalls the root's
  `EnableDiplomacy`. `ACCEPT_DEAL` → 0x00932E40 → 0x00C114B0 (applies the deal, result +0x28 = 1);
  `DECLINE_DEAL` → 0x00932F00 → 0x00C1F210 (+0x28 = 2; the reply, kind 2 over hub +0x360, carries
  the proposer, so the panel's listener ignores it unless the local player proposed);
  `CLEAR_NEGOTIATION` (`Cancel`) → 0x00932EC0 (deal items removed, hub +0x348 posted with flag 0 →
  `InitialiseNegotiation(nil?, proposer == player, false)` via 0x00A14850); `PROPOSE_DEAL` →
  0x00933690 → 0x00C49BE0 (AI evaluation). Only END ends the negotiation. `Finished()`
  (0x009BB8B0) is nothing without +0xAC, else `+0x28 != 0` (0x006CCA70): accepted or declined.
- Every panel close reaches `End()`: the PanelManager's table gives `diplomacy_panel` the
  `ExitFunc` "OnExit" (panelmanager.lua:34), and `OnExit` (:1755) ends a negotiation the panel still
  holds; `ClosedByCloseButton`, `CancelOffer` (`End()` + `ClosePopup()` when nothing can be
  proposed) and `OkWarDeclared` / `AcceptOffer` (`ClosePopup()`) all close through it.
- `BuildPossibleActions` (`PushNegotiationPossibleActionsFromScript` 0x009B5220; was not a function
  in Ghidra): nothing without +0xAC, else `{OffersAndDemands = {...}, Unilaterals = {...}}`, entries
  `{State, Active, Address, Unilateral (ids 1, 3, 5, 8, 12), Tooltip}` from the action list
  0x00C45E90 (type 1 → OffersAndDemands, type 0 → Unilaterals; availability 0x00C1A530, name
  0x009C7700). §4.1's "0 callees, UNKNOWN" came from the missing function.
- `BuildOfferAndDemandStrings` (`PushNegotiationOfferAndDemandStringsFromScript` 0x009B48B0; also
  not a function before): nothing without +0xAC, else THREE values: offers list, demands list,
  and the `offer_or_demand_regions` diplomacy string. "Offers" / "Demands" are names of the two
  Lua references, not keys (§4.1 row 7 corrected). Rows `{Action, Text, Regions}`.

**Power / Wealth (CONFIRMED trace, values not ported).** `FactionDetails`' `PowerRanking`,
`WealthRanking`, `PrestigeRanking` are strings (`GetFactionRankingStrings` 0x008C7170, which clears
them to "" first). `BuildFactionRankingTable` 0x00949630 makes one record per faction: power =
land + naval force strength (0x008B2150 over the faction's forces +0x7BC, 0x008F9C50 each),
prestige = 0x008F4D30 on faction +0x820, wealth = 0x00BBCC40 on faction +0xAC (last turn's slot of
a 10-turn ring). Three passes sort (0x008FBCC0) and categorise (`AssignFactionRankCategories`
0x0096B950: flagged factions (+0x824) get 5; of n others, when n > 3 the first three keep 0 and
entry i ≥ 3 gets min((i−3)/step + 1, 5), step = (n−1)/5 or n−3 when that is 0; a tie copies the
previous category). Names: `random_localisation_strings_string_power_category_<c+1>` (id 0x17A+,
"Terrifying" ... "Feeble"), `wealth_category_` (0x180+, "Spectacular" ... "Destitute"),
`prestige_category_` (0x186+); the ids are from the key table 0x0164DB58 filled at 0x00435CC6.
Ours: empty strings, PROVISIONAL, until the model has force strength, prestige and the economy
history. Open: 0x008F9C50, 0x008F4D30's parts, 0x00BBCC40 / 0x00B803A0, the sort order of 0x008FBCC0.

**"Test" under Current Treaties (CONFIRMED).** `SetPublicOpinionStrings` (:238) creates
`CreateComponentFromTemplate("string", "treaty_string", treaties, 0, 0, {}, {treaties_text})`.
`CreateUIComponentFromTemplateFromScript` 0x010171A0 reads the texts table with `lua_next`; a
NUMBER key becomes the empty name. `ApplyCreatedComponentStateTexts` 0x01032010: an empty name puts
that text and the ones after it on the component's states in order and is not used up, so every
descendant gets them too; a component id sets every state of that component; `id.state` (split at
the first '.', after the whole name failed as an id) sets one state. Ours ignored number keys, so
the library template `string` kept its own "Test".

**The running script context (round 3, CONFIRMED).** 0x01058750 returns the global
`lookup[L]`: the context of the Lua thread that is running. Each component's script has its own
thread; `LuaCall` (0x01014110 → 0x01025CD0) moves the arguments to the target component's thread
and `lua_pcall`s there. So the context is the target of the innermost engine entry (event, named
call, LuaCall, script load), whichever script defined the running function. Ours (round 3): the
Rust host keeps it (`ScriptContext`, host.rs). Every engine entry into a component's script
(`__ntw_fire`, `__ntw_call_if_defined`, `__ntw_call_get`, `__ntw_run_in`, `LuaCall`, the component
method dispatch, `Layout`, the selection and card managers' callbacks, the battle HUD's global calls, its per-frame card and order-button updates, the tooltip)
runs the function through the prelude's `__ntw_call_in(address, f, ...)` under an RAII guard: the host
enters it before it calls a prelude entry (`call_entry`, so the call itself is a plain Lua call
and a nesting costs Lua 5.1 one C-call level, as the exe's `lua_pcall` does), else
`__ntw_call_as` enters it for that call. The outer context comes back on return and on an error,
so nothing is reset per frame. The constructor reads it through `running_script_context`. Not modelled: LuaCall's
`lua_pcall` (an error in the called function does not reach the caller in the exe).
Accessor guards (bytes of the wrappers): `TradeableRegions` needs +0xF9C and +0xAC,
`TradeableTechnologies` / `FactionListsForStanceDeclarations` +0xAC (else nothing);
`MaxPlayerPaymentAllowed` (0x009BE720) has none; `MaxOppositionPaymentAllowed` reads the
counterpart through +0xAC unchecked.

**Payment caps (round 3, CONFIRMED, named in Ghidra).** 0x009BE720 was undefined code (no xrefs:
reached through the script method table) and is now `PushMaxPlayerPaymentAllowedFromScript`: it
takes the script context (`GetRunningScriptContext` 0x01058750), the local player's faction
(`GetLocalPlayerFaction` 0x009BF110 on `g_pCampaignManager` 0x015C493C: +0x9FC, then +0xB0) and
pushes `GetFactionEconomyTreasury` (0x00BCAFE0: the int at +0x3F4 of the faction's economy
sub-object, embedded at faction +0xAC) through `PushScriptIntegerAsNumber` (0x01056530: int to
the engine's 32-bit float lua_Number, 0x00FE95C0 `lua_pushnumber`); always one result.
`PushMaxOppositionPaymentAllowedFromScript` (0x009BE6B0) does the same for the counterpart's side
that is not the local player: proposer (+0x18, 0x004CE450) unless that is the local player, then
recipient (+0x1C, 0x004631B0); with no counterpart it reads a null pointer. 0x00BCAFE0 is the
treasury: it is the "funds" of `UpdateFactionFundsAndDate` / `UpdateFactionFundsAndTurn`
(0x009C1AD0, on the same local player faction), the "starting_treasury" log value (0x008BD0F0) and
the construction / repair affordability test (treasury < cost refuses). Ours: both return the
model faction's `treasury` (the value the HUD's funds show); with no faction to ask, 0, logged
once. Not modelled: the float rounding of treasuries above 2^24 (the host's Lua numbers are
doubles).

**A deal applies at most once.** The appliers skip a row whose +0x1E8 "applied" flag is set (see
`accept_deal`); ours keeps that flag per row (`DealRow`). Accept and the PLACEHOLDER propose both
set the result to accepted and apply the rows not yet applied; Cancel (CLEAR) empties the deal.

**Ours.** `UIDiplomacyNegotiation` (Rust) takes the running context; none → logged once.
`NegotiationState` keeps the object (+0xAC counterpart, +0xB0 context,
reset by every constructor call) apart from the campaign negotiation (proposer, recipient, open,
result +0x28, deal; changed only by `begin` = BEGIN and `end` = END, which posts "ended" once). The
two-key form queues "started" for a human proposer; `UiScriptHost::deliver_negotiation_events`
(start of `pulse`) delivers it as `InitialiseNegotiation("", proposer == player, false)` (PROVISIONAL empty
greeting: the diplomat stays hidden), "cleared" (after `Cancel`) as `InitialiseNegotiation(nil,
proposer == player, false)` and "ended" as the root's `EnableDiplomacy`. Accept (applying the rows not yet applied; the deal stays
until END) and decline set the result; Cancel keeps it (INFERRED, the executor writes none);
propose accepts at once (PLACEHOLDER: no AI evaluation in the model). The pending-move
form leaves the object empty and logs once (PROVISIONAL). `RetrieveDiplomacyDetails` no longer
opens a negotiation. `BuildPossibleActions` returns the CONFIRMED shape with both lists empty
(PROVISIONAL: no action list in the model, so no option buttons); `RemoveAction` is a PROVISIONAL
no-op (no deal rows yet). `finish_created` applies number keys, ids and `id.state` as 0x01032010
and logs once an entry it cannot use. Tests (ntw_script campaign_ui):
`negotiation_screen_closes_from_its_close_button_and_opens_again`,
`the_x_after_a_proposal_ends_the_negotiation_once`, `the_x_after_a_decline_ends_the_negotiation_once`,
`cancel_offer_ends_the_negotiation_once`,
`the_pending_move_constructor_keeps_nothing_of_the_last_negotiation`,
`a_negotiation_belongs_to_the_running_script_context` (lib: `the_script_context_comes_back_after_an_error`),
`negotiation_screen_lists_the_current_treaties`, `faction_rankings_are_strings` (pins the
PROVISIONAL empty rankings); lib: `a_deal_applies_at_most_once`, `payment_caps_are_the_treasuries`.
Still open: the greeting; the option buttons; the pending-move form; the AI reply events (+0x360)
and `ClosePopupSoftly` (+0x378 on a panel still listening); rankings; the log line
`UNKNOWN UIComponent:RegionSelectionChange` (the panel's `InitialiseMapHolders` calls a method the
host lacks).

## 5. Save naming (`ui\campaign ui\load-save_game`), 0-E sandbox 2026-10-04

Read-only probe of the original files (no Ghidra, no placeholders). The campaign
save/load screen is the `load-save_game` layout (Version039, 57 components): the root
`load-save_game` holds `bg_title_frame` (title `TX_save_game` with states `Load` =
"Load Game" / `Save` = "Save Game"), `map_panel` (the territory card `card_window`
with `Flags`, `txt_surround_box` "Territories", `flag_player_1/2`, `cartouche`
`dy_date`/`dy_season`, and the four `map_nap_*` theatre maps), `list_panel`
(`sortable_list`: `vslider`, clipped `list_clip` → `list_box` → `row_example`
with `game_name` / `time_played` / `date` cells; sortable `headers` `name` /
`turns` / `date` firing `SortList`; `button_delete`), `filename_panel`
(`_paper_pane2`: `input_name` with child `input_name_label` "Filename:"), and the
`button_ok` / `button_cancel` pair (five states each, firing
`call Parent.LuaCall, OnAccept` / `OnCancel` on `OnMouseLClickUp` into the panel
driver). Files (all parse to the last byte):
`ui\campaign ui\load-save_game` plus drivers
`load-save_game.load-save_game.luac` (15198 bytes),
`campaign_escape_menu_scripts\menu_save_game_button.luac` (969 bytes, entry
`SaveCurrentGameFromEscapeMenu`) and `ui\templates\template.campaign_save_game.luac`
(2419 bytes, row script reading its `requester`).

Layout fields (meanings per UI_LAYOUT_FORMAT.md):
- **`input_name` is the text-entry field per the text-entry rules**: its single
  `NewState` has state `unknown_d4` = 2 (the focus flag the host's `accepts_focus`
  tests) with sample text "Testtext", and the `OnKey` = `OnKeyEvent` binding the
  `CharacterInput` / RETURN path runs through. The front-end `file_requester`
  (`ui\frontend ui\file_requester`, Version039, 25 components) shares the contract
  exactly (same d4=2 "Testtext" field; its Ok/Cancel inline scripts LuaCall
  `Accept` / `Decline`); that path is already driven by
  `typed_file_name_saves_the_army`. Data fact: the requester's Cancel id carries
  a leading space (`" button_cancel"`).
- **`unknown_da` = ClipChildren: 1 on exactly 2 components**: `Flags` (the
  territory-map card) and `list_clip` (the save list).
- **`unknown_e5` = UseGlobalClicks and `unknown_140` = DrawMode: 0 everywhere**,
  so the screen draws in mode 0 by inheritance.

Name validation / limits (CONFIRMED locations, UNKNOWN bodies):
- The save writer takes no file name: `ntw_campaign::save::write_save*` rewrite
  the ESF tree from the model (`crates/ntw_campaign/src/save.rs:183-216`), so no
  validation or length limit lives there. The name becomes
  `<save_games>\<name>.save` through `FileExtenstionAndPathForWriteClass`
  (class `save_game` → `.save` / `save_games\`, CONFIRMED `0x0046EE50`,
  `crates/ntw_script/src/ui/frontend.rs:337-346,388-393`).
- Validation lives in the panel scripts (luac strings CONFIRMED, bodies UNKNOWN
  bytecode): `ValidateFilename` (in both `load-save_game` and `file_requester`
  drivers), `GenerateDefaultNameSP/MP` fed by `CampaignUI.DefaultSaveName(MP)`
  (1 call site each), `SaveNameFromFileSP/MP`, `CheckDuplicate`,
  `SortAndRemoveExcessiveFiles`, the `vfs.exists` overwrite check with the
  `panel_dropin_requester` confirm (`ConfirmSave` 10, `CancelSave` 3,
  `SaveRequesterEnded` 4, `OverwriteConfirmationAccept/Decline`), and the engine
  save call itself (`SaveCampaign` string in the driver, no `CampaignUI.*`
  entry in worker3/lua_api.txt — local or missed, UNKNOWN). No charset or
  length limit was found in any of these strings (UNKNOWN; only the OS-level
  Windows name rules apply, INFERRED). Our engine side already has
  `CampaignUI.CanSave` (stub true, `crates/ntw_script/src/ui/campaign.rs:1657`)
  and F5 quick-save to `quick_save.save` (`crates/napoleon/src/campaign/play.rs`).

Why only one element was wired: the mechanisms the screen needs (LuaCall,
`Parent("id")`, sliders, ClipChildren, text entry, `vfs.exists`, overwrite
confirm) are proven by the settlement/front-end patterns, and the reads it needs
(`EnumerateCampaignSaves`, `GetExtendedSaveGameInfo`, `FileExtenstionAndPathForWriteClass`)
already exist. Everything left is shape-UNKNOWN (the `ValidateFilename` /
`ConfirmSave` / `SaveCampaign` bodies, the `SaveRequesterEnded` / `PathName`
flow, `SortAndRemoveExcessiveFiles`' cap, the row template's `requester` link).
So the wired element is the CONFIRMED layout contract itself: test
`campaign_save_naming_layout_fields` (`ntw_formats/tests/real_install.rs`) locks
version 39, the 57-component tree, the d4=2 Testtext field with its label and
OnKey, the Ok→OnAccept / Cancel→OnCancel LuaCalls, the Load/Save title states,
the sortable headers and row cells, the Flags+list_clip clip set, zero e5/140,
the three driver luacs, and the shared `file_requester` text-entry contract,
from the original files (no placeholders).

TODO (needs luac decompilation or Ghidra; NR-ai-ghidra may be busy, not run):
1. `ValidateFilename` body (both drivers): allowed charset, length limit, what
   counts as invalid.
2. `OnAccept` → `ConfirmSave` → `SaveCampaign` arg/return shapes (overwrite flag?
   like `SaveArmySetup(setup, path, overwrite)` `0x00477F50`?); `OnCancel` /
   `CancelSave` / `OnCancelOverwrite` flow.
3. `SaveRequesterEnded` / `RequesterEnded` / `PathName` flow (cf. the front-end
   host note: `RequesterEnded` destroys the requester then asks it for its
   `PathName`).
4. `GenerateDefaultNameSP/MP` + `CampaignUI.DefaultSaveName(MP)` results (the
   prefilled name); `SaveNameFromFileSP/MP` (list display vs file name).
5. `SortAndRemoveExcessiveFiles`: the list cap and sort order.
6. `template.campaign_save_game.luac`: the row's `requester` link and
   `OnAccept` call shape; `menu_save_game_button.luac`'s
   `SaveCurrentGameFromEscapeMenu` (entry from the escape menu).
7. `panel_dropin_requester` overwrite confirm wiring (`overwrite_title` /
   `overwrite_msg` / `confirm_overwrite` strings in the driver).
8. Engine side: the actual campaign save call + `UiRequest`/model hook (ours
   only quick-saves today), and whether `CanSave` gates more than the button.

## 6. Region labels on the campaign map (slot campaign-ui, branch `work/campaign-ui`, 2026-10-05)

Settlement labels were already done (`city_info_bar` through the original `Labels.lua`); the
**region** names — the other half of BACKLOG §2 "Region and settlement labels" — were missing, even
though the map file already carries where every name goes (`RegionMap::labels` was parsed and
unused).

Code: `crates/napoleon/src/campaign/region_labels.rs` (`spawn` in `scene::enter`, `update` in the
campaign's system chain). One atlas image + one material for all the names; one quad entity per
name, draped on the ground and scaled with the camera distance.

| Item | State | Tag |
|---|---|---|
| Where a region name goes: `regions.esf` `theatres_and_region_keys` → per theatre, `region_keys` = (key, Coord2d) | CONFIRMED by parsing | 101 label positions on the European map, one per theatre region |
| The text: loc `regions_onscreen_<key>` (`regions_onscreen_eur_france` = "France") | CONFIRMED (the same keys the HUD's `region_name` reads) | 72 names on `nap_europe`; the other 29 label positions are sea regions, which have no such text and are skipped |
| The names are the engine's map text, not HUD components | INFERRED | the positions are logic map coordinates in the map file, one per region of the theatre |
| The font (`font\ingame_12.cuf`, rasterised with `battle::labels::rasterise`, the campaign HUD's in-game family) | PROVISIONAL | the original's map-label font is UNKNOWN |
| The look: white glyphs over a one-pixel dark halo, `AlphaMode::Blend`, unlit, draped on the terrain and depth-tested (so models in front cover a name) | PROVISIONAL | the river / coast map overlays are drawn without depth test (`scene.rs`); for text we chose the painted-on-the-map look |
| The size rule: one rasterised pixel = 0.15 logic units at camera distance 60, scaled by `distance / 60` afterwards, so a name is the same size on screen at any zoom | PROVISIONAL | the original's rule is UNKNOWN; this is what NTW's map reads like in play (legible at full zoom-out) |
| Only the names are drawn; resource icons (`RetrieveVisibleEnitityDetails().Resources`) | open | `Resources` is still an empty table in `campaign.rs` |

Tests (`cargo test -p napoleon`, 43 pass): `atlas_rows_do_not_overlap`,
`atlas_wraps_a_wide_name`, `quad_is_centred_and_reads_north_up`,
`the_outline_surrounds_the_glyphs`, `zoom_scaling_keeps_the_screen_size` and the real-install
`every_land_region_has_a_label_and_a_name` (101 positions, 72 names, every one a land region with
a loc name inside the map bounds, every quad finite).

Checked in the game (`--campaign eur_napoleon --campaign-faction france --screenshot ...`, log line
`Campaign: 72 region labels (1015x94 atlas, 0.150 units per name pixel at distance 60)`): the names
appear over northern Italy at the start view and keep the same size from distance 15 to 100
(`NAPOLEON_CAMPAIGN_CAMERA=x,z,distance`); `NAPOLEON_CAMPAIGN_REGION_LABELS=0` leaves them out
(A/B screenshots in `target/tmp/labels`).

Open:
1. The original's address for the label draw (font, size rule, colour, halo, any zoom fade, and
   whether a name is hidden for an unknown region) — needs a Ghidra copy.
2. Resource icons next to the names (`RetrieveVisibleEnitityDetails().Resources`, the mines / farms /
   logging slots in `regions.esf`).
3. Selection effects and the zone-of-control display (the rest of the BACKLOG line). The movement
   arrows are done — §7.

**Main-repo port notes (2026-10-05, from sandbox `90bfc1c`):** the names are rasterised with
`ntw_formats::font::CufFont::coverage` (plain glyph coverage) instead of
`battle::labels::rasterise`, so the battle module stays private and the rasteriser's drop shadow no
longer turns into a white smear under `composite`'s halo; the look is otherwise as above. The
in-game checks above were made on the sandbox, not re-run on main.

## 7. The campaign map's movement arrow (slot campaign-ui, branch `work/campaign-ui`, 2026-10-05)

BACKLOG §2's "movement arrows". `play::preview` used to draw the ordered path as a Bevy gizmo line
(green for the reachable part, red beyond); it now publishes the planned path and the new
`crates/napoleon/src/campaign/arrows.rs` lays the **original's** arrows along it. The selection ring
stays a gizmo — the original's own selection marker is UNKNOWN (open item 3 above).

### The assets (CONFIRMED from the install)

| Item | What it is | Tag |
|---|---|---|
| `rigidmodels\campaignpieces\textures\arrow.dds` | 512x256 DXT5, **one white arrow with an alpha channel**. Measured per column: a point at mid-height on the left edge, the head flaring to 162 of 256 rows by x = 80, a constant 64-row shaft to x = 288, then a tail tapering back to a point at x = 512 — so **the head is at the left of the picture** and the arrow points along −u | CONFIRMED |
| The exe's path prefix `RigidModels/CampaignPieces/Textures/arrow` (`0x00F6E14C`) | referenced **twice** from `FUN_00986430` (`0x00986562`, `0x0098656f`), the arrow-pool builder | CONFIRMED (Ghidra, `NR-f0a-ghidra`) |
| `campaign_maps\<map>\display\arrows\arrows.rigid_model` | one flat mesh lying in the XZ plane (vertex y 0.0032..0.0049 in its own units), **12.158 x 7.336**, 432 vertices in 576 triangles, subdivided along its length so it can be bent round a path, taking a 0.4549 x 1.0 slice of a 2:1 texture. Its own diffuse slot is `transitmarkers_diffuse`, a placeholder — hence the override above | CONFIRMED (parsed) |

The mesh is a **template, not a placed model**: its absolute size means nothing, because the original's
placement code supplies the scale. Only its width-to-length proportion (0.603) is used, which is
scale-invariant.

### The rule (INFERRED, `FUN_009CC5F0`)

**Evidence status (2026-10-05 review):** no decompile of `FUN_009CC5F0` was kept. The only trace on
record is a few stack-access lines (sandbox `ghidra_evidence/0b/0b_round12__step4.txt:2246`) and one
call site (`0c/ghidra_out_0c28.txt:7506`), so the rule below is INFERRED until the decompile is
redone in Ghidra.

- The path's points are walked and **one is taken every `spacing` units of walking distance**,
  interpolated linearly inside its segment at `(spacing − walked) / length`. The spacing carries across
  segments, so a bend does not restart it; the path's start and end are always kept.
- The sampled points go into **two buckets** — the loop's bound is the float value of `2` — the second
  starting where the first ended. They are the two colours the original draws. **INFERRED:** they are
  the part of the path the character can still reach this turn and the part beyond it.
- Each bucket is **smoothed** before it is drawn: the first and last legs give up their points at
  1/3 and 2/3 along, and every interior point `p[i]` becomes `lerp(p[i-1], p[i], 0.5)`, so the chain
  bends round a corner instead of kinking at it. The original then sweeps a strip through those points
  with `FUN_005AFED0`, the same spline builder the borders, roads and rivers use
  (`CAMPAIGN_MAP.md` §10.3).
- The pool holds **five chains per bucket**; a bucket with fewer than two points draws nothing, and a
  path with fewer than two points draws nothing at all.
- A spacing of zero or less keeps every path point: that is what the original's other entry point,
  `FUN_00A27CB0`, passes.

### What we draw, and what is still open

Our strip is **generated** from the install's texture and the rule above, with the mesh's own
proportions, rather than by bending the mesh itself: the mapping the original uses to bend it is
UNKNOWN. `SPACING` = 12 map units and the two colours are **PROVISIONAL** — they are arguments of
the virtual setter `FUN_00A27C30(this, colour, flag, path, spacing)`, and Ghidra resolves **no direct
call and no vtable entry** for it, so the values cannot be read without finding the caller another way
(a debugger session, or the vtable by hand from `FUN_00986430`'s class).

| Item | State | Tag |
|---|---|---|
| The texture and which way its head points | CONFIRMED | `arrow.dds`, the exe's own prefix, and a measured alpha profile |
| The mesh's proportions (0.603 of its length wide) | CONFIRMED | parsed from the install |
| Sampling every `spacing` of walking distance, carried across segments | INFERRED | `FUN_009CC5F0`, decompile not kept; redo in Ghidra |
| The two buckets, and the smoothing (thirds on the end legs, interior midpoints) | INFERRED | `FUN_009CC5F0`, decompile not kept; redo in Ghidra |
| Five chains per bucket; under two points draws nothing | INFERRED | `FUN_00986430` / `FUN_009CC5F0`, decompiles not kept; redo in Ghidra |
| That the buckets are the reachable part and the part beyond | INFERRED | the original is told the split by the same unresolved caller |
| The **spacing** (12 map units) | PROVISIONAL | argument of `FUN_00A27C30`; the caller is unresolved |
| The two **colours** | PROVISIONAL | as above |
| How the original bends `arrows.rigid_model` along the path | UNKNOWN | we generate the strip instead |

Tests (`cargo test -p napoleon`, **57 pass / 0 fail / 1 ignored**), 13 new: `samples_are_spacing_apart`,
`a_short_last_gap_is_kept`, `spacing_carries_over_a_bend`, `several_arrows_fit_in_one_segment`,
`a_zero_spacing_keeps_every_point`, `a_straight_chain_is_thirds`, `a_corner_becomes_a_midpoint`,
`a_single_point_draws_nothing`, `the_path_is_split_once`, `the_head_is_at_the_destination`,
`the_strip_is_the_arrow_width`, `the_width_follows_the_mesh_proportion`, `the_strip_lies_on_the_ground`,
and the real-install `the_install_has_the_arrow_assets` (the texture's measured profile and the
model's proportions).

Checked in the game (`--campaign eur_napoleon --campaign-faction france --campaign-demo
[--campaign-demo-zoc] --screenshot ...`, log line `Campaign arrows: 34 path points -> 8 sampled over
39.3 x 55.9 map units ..., 2 chains (6 of 8 reachable), one arrow 12 long and 7.24 wide`):

- `target/tmp/arrows/06_arrows.png` — a short path (7 points → 4 sampled), one chain.
- `target/tmp/arrows/08_off.png` — the same view with `NAPOLEON_CAMPAIGN_ARROWS=0`: the arrows are gone
  and only the selection ring is left, which is the A/B proof that this code draws them.
- `target/tmp/arrows/09_zoc.png` — `--campaign-demo-zoc`: the path bends round an enemy army's zone,
  the white arrows stop where the movement limit is and the red ones carry on. Two chains, 6 of 8
  sampled points reachable.

Open:
1. The **spacing and the two colours** — `FUN_00A27C30`'s caller. A debugger session, or the class's
   vtable traced by hand, would settle both.
2. How the original **bends `arrows.rigid_model`** along the path (its length parameter, and how the
   0.4549 texture slice is mapped), which would let us draw its mesh instead of generating a strip.
3. Selection effects and the zone-of-control display — the rest of the BACKLOG line. For the zone of
   control, note the model's side is already finished (`ntw_sim::campaign::zoc`, with its three
   tests); only the rendering is missing, and `FUN_009CC5F0` is very likely the same ribbon machinery
   the zone outline is drawn with.

**Main-repo port notes (2026-10-05, from sandbox `90bfc1c`):** (1) the colour split now converts the
model's `PathPlan::reachable` (an index into the *path's* points) into a count of *sampled* points by
walking distance (`arrows::reachable_samples`, INFERRED like the split itself) — the sandbox passed
the path index straight to `buckets`, which split at the wrong place; (2) `preview` withdraws the
`ArrowPath` resource when nothing is selected, hovered or plannable, and `draw` hides the pool, so
the arrows no longer stay on the map after a deselect (the gizmo line they replace was per-frame);
(3) `draw` redraws on every published path change rather than only on a generation change, so
hovering a new cell inside one model generation updates the arrow; (4) `ARROW_LIFT` is tagged
PLACEHOLDER. The in-game screenshots above were made on the sandbox, not re-run on main.

---

## 8. `panel_manager` -- it is a shipped Lua module, not an engine object (0-E, 2026-10-06)

**This corrects a claim that was in `CHARACTER_UI_HOOKS.md` and in `HANDOFF.md` ("the original opens
those popups through `panel_manager`, which we do not implement").** It is not something we have to
write: `ui\panelmanager.luac` is a **shipped Lua module** (a `module(..., package.seeall)` chunk),
and `ui\campaign ui\layout.root.luac` requires it itself in its first instructions --
`Utilities.Require("PanelManager")` at pc 29-31 and then `panel_manager.SetRootAndEnvironment(Address,
CampaignUI)` at pc 65-68. Our `require` resolves `data/ui/panelmanager.lua` -> `ui\panelmanager.luac`
through `ScriptSource::find`'s `.lua` -> `.luac` fallback, so **the module already runs in our host**,
with its own 26-panel table, its docking, its priority locks and its `ExitFunc`s.

Evidence that it runs and that its dispatch works: clicking the army panel's promote button reaches
`UI/PanelManager.lua:312: in function 'panelmanager.OpenPanel'` (`army.lua:588 in function
'army.PromoteUnits'`) and the `enlist_commander` panel appears under the root with its own
`list_box` and `commander_<Name>` rows. Test:
`enlist_commander_panel_opens_and_lists_a_candidate` (`ntw_script --test campaign_ui`, install).

### The contract, all CONFIRMED from `ui\panelmanager.luac`

The module's globals are **eight functions** (`SETGLOBAL` at pc 286-317, in this order), and
`analysis/worker3/lua_api.txt:4902-4911` lists exactly the same eight as the exe's Lua bindings:

| global | arity | where |
|---|---|---|
| `OpenPanel` | `numparams=3 is_vararg=3` | proto `panelmanager.lua:217` |
| `SetRootAndEnvironment` | 2 | proto `panelmanager.lua:333` |
| `ClosePanelWithShowData` | 1 | proto `panelmanager.lua:338` |
| `ClosePanel` | 1 | proto `panelmanager.lua:353` |
| `IsPanelOpen` | 1 | proto `panelmanager.lua:392` |
| `CloseAllPanels` | 0 | proto `panelmanager.lua:406` |
| `ClearCachedComponent` | 0 | proto `panelmanager.lua:429` |
| `OpenPanels` | 0 | proto `panelmanager.lua:437` |

(The eight `SETGLOBAL`s and the eight protos line up 8 for 8 once the upvalue `MOVE`s that follow
each `CLOSURE` are counted -- a useful reading rule for this chunk, see §8.4.)

**`OpenPanel(panel_name, show_data, init, ...)`** -- the argument list the brief asks about:

- `panel_name` is a key into the module's own table `m_layouts`, which the chunk builds at pc 12-256
  with **`NEWTABLE {} array=0 hash=26`**: **26 panels, CONFIRMED**, each `{ Layout = "<path>",
  Side = huds.g_left|g_right|g_centre|nil, PriorityLock = <n>, NoEscape = true, ExitFunc =
  "<name>", Undocked = true }`. The table is the *only* panel list -- nothing in our code hardcodes
  one, and nothing should.
- `show_data` is stored as `m_layouts[name].ShowData` and is the **toggle key** (pc 12-31: if the
  panel is already open, still parented, and `ShowData == show_data`, then `ClosePanel(name)` and
  return -- so calling it twice with the same data closes the panel).
- `init` is **either a string or a function** (pc 289-306): a string goes through
  `UIComponent(component):LuaCall(init, ...)`, a function is called directly with the varargs. Both
  get the varargs unchanged (`VARARG R[n] n=-1` + `CALL nargs=-1`).
- The panel is created with **`Component.CreateFromLayout(m_layouts[name].Layout, panel_name,
  m_root, 0, 0)`** (pc 138-145, nargs 5), then, unless the record says `Undocked`,
  `SetDockingPoint(Side, huds.g_centre)` and `huds.MoveRelativeToHUD(component, DockingPoint(),
  offset[Side])` where `offset = {8, -8, 0, 0}` indexed by `Side` (pc 161-196).
- It returns the component address (pc 367-368) and caches it as
  `cached_panel = {Panel, Component, Parent}`; a previously cached, still-open, *different* panel is
  destroyed first (pc 329-347).

**So the `enlist_commander` call is** `OpenPanel("enlist_commander", nil, "InitEnlistCommander",
g_panel_is_navy, g_military_force)` -- five arguments, of which the last two are the init function's
own parameters, **not** panel-manager options. `ui\army.luac:582` is `numparams=0` and its upvalues
are `["g_cardgroup", "panel_manager", "g_panel_is_navy", "g_military_force"]`, so the two flags are
the army panel's own globals. `ui\campaign ui\enlist_commander_scripts\enlist_commander.luac` then
confirms the order: `InitEnlistCommander` (`panelmanager`-side pc 0-4 of the proto at line 14) stores
its **second** parameter as `m_military_force` and passes its **first** to `SetAsRecruitmentType`,
whose proto at line 93 sets state `admirals`/`tx_enlist_new_admiral` when that argument is `true`.
**`InitEnlistCommander(is_navy, force)`, arity 2 -- CONFIRMED.** 0-G read these as "the two flags
`g_panel_is_navy` and `g_military_force`" without the order; the order is now settled from the data
and is the opposite of the argument order.

**`ClosePanel(x)`** takes a panel **name or a component address** (pc 0-21 branch on `type(x)`;
for an address it asks `IsPanelOpen(UIComponent(x):Id())`). It then refuses to close a component
whose environment has `g_closeable == false` (pc 28-36) and calls its own `ClearupPanel(side,
component, true, 0)`, which runs the panel's `ExitFunc`, destroys the component, unhooks it from
every side's list and fires `TriggerPanelClosedEvent`. `IsPanelOpen(name)` answers
**(component, side, is_topmost_on_that_side)** -- three values, pc 42 `RETURN R[3]..R[3+3]`.

The 65 shipped call sites (`cargo run -p ntw_script --example luac_scan -- OpenPanel`, the tool added
this round; `lua_api.txt` counts 66, the difference is one call in a non-`.luac` script) all use that
shape, e.g. `ui\campaign ui\layout.root.luac:1177`:
`OpenPanel("agent_action", nil, "Initialise", "duel", src, targets)`.

### What was actually blocking it -- two of our own bugs, not the panel manager

1. **`Utilities.GetEffectList` raised inside every trait row.** `enlist_commander_entry.lua:54` hands
   each trait of a candidate to `ui\templates\character_trait_entry.luac`'s `SetTraitTooltip` (proto
   at line 17), which calls `GetEffectList(row.Effects, row.AttributeEffects)`;
   `utilities.lua:182` pc 2 is `LEN R[4] = #R[1]` -- the **second** argument. Our commander rows
   carried only `Name`, so `InitEnlistCommander` aborted in the first trait's tooltip, the click's
   `LuaCall` chain unwound, and no candidate could be picked even though the rows were built.
   **CONFIRMED** the two field names (`Effects`, `AttributeEffects`) and that they must be lists;
   **PROVISIONAL** that both are empty and that `ColourText` / `ExplanationText` / `RemovalText`
   (the tooltip's other three reads) are empty strings -- the effect descriptions behind a trait come
   from a table we do not load. Fixed in `commanders_for_recruitment`.
2. **`Component.Call("IsDragged")` answered `nil`, and `false == nil` is false.**
   `ui\templates\cards.luac`'s `OnLeftClickUp` (proto at line 26) reads
   `Component.Call("IsDragged")` (no path, so this component) and does nothing unless the answer is
   exactly `false`; the branch it skips logs
   `"OnLeftClickUp: false == Component.Call( IsDragged ) and this:CurrentState() ~= UnSelectable. Failed"`.
   With no binding, **every unit and agent card in the campaign HUD and in the battle HUD silently
   ignored its own left click** -- which is also why `ShowAgentButtons` never ran and the agents
   panel's action buttons never appeared. Fixed with `UIComponent:IsDragged() -> false`
   (PROVISIONAL: we keep no drag state, so nothing can be dragged).

### Round 3 corrections (0-E, 2026-10-06)

Two of the claims above were wrong, and both were checked against the bytecode rather than assumed.

**1. `huds.RegisterHud` does not take a width and a height, and the panel was never misplaced.**
`layout.root.lua:790` pc 181-185 is `huds.RegisterHud(g_hud, true)` -- **two** arguments, the `g_hud`
component and the boolean `true`. `huds` is not an engine object but the shipped Lua module
`ui\huds.luac` (`Utilities.Require("Huds")`, `layout.root.lua:0` K[7]), and its `RegisterHud` (proto
at line 15, arity 2) is:

```
assert(type(hud) == "userdata", "Incorrect type passed to RegisterHud!")
hud = UIComponent(hud)
if flag == true then h_height, h_width = hud:Bounds() else h_height, h_width = hud:Dimensions() end
if 1280 < h_width then h_width = 1280 end
s_height, s_width = UIComponent(hud:Parent("root")):Dimensions()
```

So `h_width` / `h_height` are **not** 0 here: `Bounds` and `Parent` are both bound, and the layout's own
`veneer_DY` band is 1280 x 241 on the 1280 x 960 test screen. `UIComponent:Bounds` is therefore
**CONFIRMED** to be the size like `Dimensions` (the host had it as an INFERENCE) -- `RegisterHud` is
its only use.

**The panel position is the original's own arithmetic**, and it is worth writing out because it looks
like a bug and is not. `panelmanager.lua` pc 185-196 places every panel with
`huds.MoveRelativeToHUD(panel, horizontal, vertical, offset)`, and `enlist_commander`'s `Side` is
`huds.g_centre` (pc 188-190), so the centre branch of `Huds.MoveRelativeToHUD` (proto at line 47)
applies: `x = TruncToInt((s_width - w)/2)` and `y = TruncToInt((s_height - h_height - h)/2)`. With
`s_width = 1280`, `s_height = 960`, `h_width = 1280`, `h_height = 241` and a 624 x 720 panel that is
`x = 328` and `y = TruncToInt((960 - 241 - 720)/2) = TruncToInt(-0.5) = -1`.

`CoreUtils.TruncToInt(v)` is `v - (v % 1)` (`coreutils.lua:9`) and Lua 5.1's `%` is
`a - floor(a/b)*b`, so with `b = 1` it collapses to **`floor`** -- which is why a negative half lands on
`-1` and not `0`. **The one-pixel overhang is the original's, not ours**, and it happens because the
720-tall panel is 2 px taller than the 718 px above the HUD band. Install test
`the_panel_lands_where_the_huds_formula_puts_it` pins every one of those numbers.

**2. `OpenAgentActionPopup` / `OpenAgentOptionsPopup` are reachable, and three of the four buttons now
work.** They are defined but never called by any shipped `.luac` -- true, and irrelevant: they are
**root-layout globals**, so the engine reaches them with `root:LuaCall(...)`, which is exactly what
`agent_options.lua:109/118/128` and `agent_action.lua:49` do. The engine's side of that call is our
`CampaignUI.Agent*` binding. `AgentRakeAssassinate`, `AgentRakeSubterfuge` and `AgentGentlemanDuel` now
run the three lines `agent_options.Assassinate` / `.Sabotage` / `.Duel` run (ask `Request*Targets`, and
if the list is not empty hand it to `OpenAgentActionPopup`), so the picker opens, a row reaches
`Instigate*`, and the model command is queued. Install test
`an_agent_action_button_opens_the_target_picker`.

**The mask is still the engine's, and four of its nine bits are all that can be derived.** See
CHARACTER_UI_HOOKS.md "Agent action popups" for the bit values (corrected there) and for why the other
five are left out rather than invented.

### Still open, and deliberately not invented

- `agent_options.Initialise` also does `string.find(tostring(target), "CHARACTER")` (pc 2-9) to
  decide whether to build a `CampaignCharacter` interface. **Correction to the note above: our
  addresses are not tables** -- `entity()` hands scripts `mlua::Value::LightUserData`, so `tostring`
  gives `"userdata: 0x..."`, not `"table: 0x..."`. Either way **that branch can never be taken here**;
  whatever we pass for `target`, the popup's `character_interface` will be nil. The original's
  addresses must stringify with a `CHARACTER` prefix -- that is the only reason the test exists -- but
  the exact format is **UNKNOWN**: it appears in no shipped log string (`luac_scan -- strings CHARACTER`
  finds only this test, a template's debug label and an unrelated loc). **The contained fix is not
  small**, and this is why it was not done: light userdata cannot carry a `__tostring`, so the address
  would have to become a table with one, and then every `==` between two addresses would break unless
  the tables are interned per `(tag, id)`. Reported, not attempted; 0-G touches the same code.
- `CampaignUI.MoveIntoTarget` (the options popup's `visit` / `embed` / `research` /
  `steal_research` / `counterspy` buttons, `agent_options.lua:139/144/149/154/164`) is bound to a
  **logging stub**, not to a command: it appears in **no** shipped `.luac`, so its contract -- what the
  third `true` means, whether it moves the agent, resolves the action or sets a flag -- is UNKNOWN.
  The five mask bits that reach it are left **out**, so no button offers it and nothing can raise.
- `AgentRogueSabotageArmy` is still a no-op with a log line: its only target is an **army**, and there
  is no `Request*Targets` list for one (`SabotageArmy(agent, force)` is called straight from the popup
  button, `agent_options.lua:159`), so the original's exe picked the force itself.

## 9. The address representation, and the format `tostring` has to produce (0-E, 2026-10-06)

Round 3 left the address string **UNKNOWN** and refused to guess. It is now **SETTLED from the exe**,
which is where the format lives -- it is built by the engine when it makes the handle, so no amount of
searching `data.pack` could ever have found it. That is why `luac_scan -- strings CHARACTER` (round 3)
found only a test, a debug label and an unrelated loc.

### 9.1 The original's address is a userdata with a metatable, not light userdata

**CONFIRMED.** The original's addresses are `UTILITYDLL::LUA::Pointer<T>`:

- The exe carries the type strings. `strx:Pointer<CHARACTER>` finds the single occurrence at
  `0x0136C4C8`, and a plain string scan finds **57** `UTILITYDLL::LUA::Pointer<...>` occurrences, one
  per bound type, all of them spelled out in full
  (`... State::operator <<<const class EMPIRECAMPAIGN::CHARACTER>(const class UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::CHARACTER const > &)`).
- **53** registering functions call one shared registrar, `0x0105AE10` (114 bytes, 55 callers):
  `allpat:0x00401000:0x00F00000:CALL 0x0105ae10` lists all 53 entries. Each is a per-type template
  instantiation.

### 9.2 The shared metatable is exactly `type` + `__tostring` + `__eq`

**CONFIRMED** from the raw listing of `0x0105AE10` (the decompiled form mis-attributes the pushed
value to a nearby constant, so the listing is the authority):

```
PUSH dword ptr [ESP + 0xc]     ; the caller's own second argument = the type name
PUSH ESI                       ; L
CALL 0x00fe95e0                ; push that name
PUSH 0x131625c  ; "type"
PUSH -0x2
CALL 0x00fe9990                ; metatable.type = name
PUSH 0x0 / PUSH 0x1058f60 / CALL 0x00fe9460
PUSH 0x13163d8  ; "__tostring"
PUSH -0x2
CALL 0x00fe9990                ; metatable.__tostring = 0x01058F60
PUSH 0x0 / PUSH 0x1058f20 / CALL 0x00fe9460
PUSH 0x131640c  ; "__eq"
PUSH -0x2
CALL 0x00fe9990                ; metatable.__eq = 0x01058F20
```

There is **no `__index`**: an address is not a table of methods in the original. Its methods come from
`CampaignCharacter(address)`, which is Lua.

### 9.3 `__eq` is pointer identity -- so `==` between two addresses must stay identity

**CONFIRMED.** `0x01058F20` (51 bytes) reads both userdata payloads and pushes `(*a == *b)`. It is a
raw pointer comparison, not a value comparison. So the representation has to make `==` an identity
test, and it must be **stable**: two calls that mean the same entity must hand back the same thing.

### 9.4 `__tostring` is `"%s (0x0%x)"` of the metatable's `type` and the pointer

**CONFIRMED** from the raw listing of `0x01058F60` (121 bytes):

```
CALL 0x00fe9d70   ; ESI = the userdata payload (the wrapped pointer)
PUSH 0x131625c  ; "type"   ->  CALL 0x00fe8f90     ; push the key
PUSH 0x0 / PUSH -0x1 / CALL 0x00fe9c40              ; read metatable.type, EAX = its char*
PUSH dword ptr [ESI]                                ; the pointer
PUSH EAX                                           ; the type name
PUSH 0x13f3c3c  ; "%s (0x0%x)"   CALL 0x004f05a0  ; sprintf
...  CALL 0x00fe95e0                                ; push the resulting string
MOV EAX,0x1                                        ; one return value: __tostring
```

So the whole format is: **`tostring(address) == "<type name> (0x0<hex pointer>)"`**, the type name being
`metatable.type`.

**Independent cross-check on the same mechanism** (this is what makes the `type` reading solid rather
than inferred): the *named interface* family (`0x00DA99D0`, `0x00859A50`, `0x0045DCA0`, ... -- 608
bytes each) registers the same `type`/`__tostring`/`__eq` triple, and each of its `__tostring`s
hardcodes its own name and prints `"%s (%s)"`:
`FUN_0085a770` -> `"BattleShip"`, `FUN_0066a780` -> `"battle.alliance"`,
`FUN_00dafaa0` -> `"UIPrefsInterface"`, `FUN_0103f1d0` -> `"UIComponentImageMetrics"`,
`FUN_00db66c0` -> `"BlockedPlayers"`, `FUN_00db4420` -> `"UIMPDropInInterface"`.
So `metatable.type` is exactly the name that registration was handed -- no interpretation needed.

### 9.5 `type` is the interned C++ signature string, not a short name

**CONFIRMED.** Each of the 53 registering functions builds its full
`State::operator <<<T>(const Lua::Pointer<T> &)` **signature** at run time from a template argument (a
thread-local one-shot that interns the string into a CA constant pool) and passes that interned pointer
to `0x0105AE10`. Read verbatim out of nine of them:

| registrar | type it interns |
|---|---|
| `0x0097BC40` | `... EMPIREUTILITY::EVENT_RECORD ... Lua::Pointer<... EVENT_RECORD const > &` |
| `0x0097C320` | `... EMPIRECAMPAIGN::CAMPAIGN_MODEL ...` |
| `0x0097CF80` | `... EMPIRECAMPAIGN::POST_BATTLE_NAVAL_UNIT ...` |
| `0x0097DB70` | `... EMPIREUTILITY::UNIT_RECORD ...` |
| `0x0044DCB0` | `... EMPIREUTILITY::EMPIRE_DATABASES ...` |
| `0x005875E0` | `... EMPIRECOMMON::EMPIRE_ONLINE_PRESENCE ...` |
| `0x00858C40` | `... EMPIREBATTLE::PROXY ...` |
| `0x00DA4E30` | `... EMPIREUTILITY::KEYBOARD_SHORTCUT_DESCRIPTION ...` |
| `0x00DEBA40` | `... EMPIREUTILITY::EVENT ...` |

**`Pointer<CHARACTER>` is a second, different name and is NOT what `tostring` prints.** It is the only
string of that shape in the exe, it is referenced from exactly **one** place (`xref:0x0136c4c8` ->
`0x009C1230` only), and that function is the *argument converter*: it pushes the name and then calls
`0x00FE8DC0` to test an incoming argument. So `Pointer<CHARACTER>` is the Lua-visible type name used
by the `Lunar<>` registry (`Trying to retrieve type %s, found type %s`), while `metatable.type` -- the
one `__tostring` reads -- is the C++ signature.

### 9.6 The campaign character's own string, and why `CHARACTER` matches

**CONFIRMED.** Two signature strings for it exist in the exe's constant pool, and **both** contain
`CHARACTER`:

```
class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator <<<const class EMPIRECAMPAIGN::CHARACTER>(const class UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::CHARACTER const > &)
class UTILITYDLL::LUA::State &__thiscall UTILITYDLL::LUA::State::operator <<<class EMPIRECAMPAIGN::CHARACTER>(const class UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::CHARACTER> &)
```

(`const` at `0x00F71179`, non-`const` at `0x00F72343`.) Which one a given script sees is the
const/non-`const` binding its value came out of -- **INFERRED, and behaviourally immaterial**, because
the only thing any shipped script does with the string is `string.find(..., "CHARACTER")`.

**So `agent_options.lua:34`'s test does match in the original, and the format question is closed.**
The other campaign types' strings, read the same way, for the names ours prints:

- `REGION`, `REGION_SLOT`, `UNIT`, `MILITARY_FORCE`, `FORT` -- all with a `const` and most with a
  non-`const` variant.
- a theatre is `EMPIRECAMPAIGN::CAMPAIGN_THEATRE` (there is no bare `THEATRE`).

### 9.7 What `character_interface` is actually for -- and it is only a lifetime pin

`agent_options.lua:89` is the popup's teardown and the **only** reader of `character_interface`:

```
0  GETUPVAL  R[0] = character_interface
1  EQ        (R[0] == nil) ? skip to 6
3  GETUPVAL  R[0] = character_interface
4  SELF      R[0+1] = R[0]["Release"]
5  CALL      nargs=1
6  LOADNIL / SETUPVAL character_interface = nil
```

So the original's `CampaignCharacter(address)` returns a **handle that has to be released**, and the
`string.find` test exists so the popup releases only one it actually created. Two consequences:

- our `CampaignCharacter` (`campaign_prelude.lua:72`) already answers `Release()` as a no-op, so making
  the branch reachable **cannot raise** -- it is safe to implement;
- the interface is built at `:34` and only ever released at `:89`, so **it exists purely to keep the
  target alive for the popup's lifetime**. That is independent evidence that `m_target` really is a
  character at least some of the time -- which is what makes the `CHARACTER` branch, not the `else`,
  the interesting path.

### 9.8 The decision, written down before implementing (item 2 of the round-4 brief)

**Addresses become interned Lua tables.** Each address is a table with an `Address` field (the
existing light userdata payload, untouched) and a per-tag metatable whose only metamethod is
`__tostring`, producing `"<the exe's Lua::Pointer<T> signature> (0x0<hex>)"`. Interning is per
`(tag, id)`, so `==` between two addresses stays an identity test -- the original's `__eq`.

Why not keep light userdata: light userdata cannot carry `__tostring` in Lua 5.1 (nor in `mlua`), so
while addresses are light userdata the format is *unreachable*, which is exactly the bug this fixes.
The original does not have that problem because it does not use light userdata either.

Why not non-interned tables: tables compare by reference, so `==` between two addresses would silently
start answering false **everywhere** in the scripting layer. Interning is not an optimisation here, it
is the whole point.

**Blast radius, measured before touching anything:**

- `entity()` and `entity_of()` in `crates/ntw_script/src/ui/campaign.rs` are the **only** two functions
  that touch the representation; everything else goes through the six `*_value` helpers. **29**
  production call sites and 6 helper definitions. 23 of the 29 already have `&CampaignUi` or `&self`
  in scope; the one genuine exception is `Residence::value`.
- `entity_of()` needs **no** change: it already unwraps `Value::Table` -> `"Address"`, and the new
  table keeps that field pointing at the same light userdata payload. A raw light userdata address and
  an interned table address therefore both still decode -- nothing that hands us an address breaks.
- The interning store is a **strong** `HashMap` on `CampaignUi`, deliberately **not** the Lua registry
  and deliberately **not** weak: identity has to hold for the life of the campaign, and a weak store
  would let a collected table be replaced by a fresh one, reintroducing exactly the `==` breakage this
  change exists to prevent. It is bounded by the number of distinct entities of each kind, and a
  recycled id correctly reuses the same address, as the original's recycled object would.
- `crates/ntw_script/src/ui/host.rs`'s `addr()` (`LightUserData(node + 1)`) and
  `crates/ntw_script/src/ui/battle.rs`'s node handles are a **different** family in the exe -- the named
  interfaces of §9.4, which print `"%s (%s)"` with a short hardcoded name -- and are **not touched**.

### 9.9 `MoveIntoTarget` -- NOT 0-B's, and not a UI shell around nothing

Round 3 left it as "a logging stub, contract UNKNOWN". It is better described than that, and it is
**not** a model gap. What is now settled, all CONFIRMED from the install's bytecode:

- **The arity and the third argument**, from the five protos themselves: every one is
  `Close(); CampaignUI.MoveIntoTarget(m_src, m_target)`, and **exactly two** of the five pass a third
  `true` -- `research` (16) at `:149` and `steal_research` (32) at `:154`. `visit` (1, `:139`),
  `embed` (8, `:144`) and `counterspy` (256, `:164`) pass two arguments. So the flag distinguishes the
  two research actions from the other three; what it *selects* is still UNKNOWN.
- **The model command already exists.** `CampaignCommand::MoveCharacter { character, to }`, whose own
  doc comment reads "Move a character without a force (an agent, or a general alone)" and whose handler
  calls `self.walk(character, to, 0.0)`. The movement half is done and tested; **0-B is not the
  blocker.**
- **What is missing is one engine-side fact: what `m_target` *is*.** It arrives as `Initialise`'s
  second parameter from `OpenAgentOptionsPopup`, which has **no caller in any shipped `.luac`** -- the
  engine opened it. So the target was chosen by the exe, and no shipped script says whether it is a
  region, a character or a settlement. The `string.find` test in `:34` is the only evidence that it is
  sometimes a character (§9.7).
- Consequently the five mask bits stay **unset** and `MoveIntoTarget` stays a logging stub. Wiring it
  would mean *deciding* what `target` is, and a wrong guess turns a stub into a wrong command, which
  is worse than no command.
- **Named leads, in order:** (a) decompile the exe half of `OpenAgentOptionsPopup`
  (`layout.root.lua:1191`, arity 5) and read what it passes as argument 2; (b) the `percent_dy` the
  popup shows for `embed` is one of `Initialise`'s last two arguments, which are engine-computed
  chances with no data behind them -- the same gap as the mask; (c) `Lunar<>`'s `udata_lookup`
  (`0x01058750`) is the generic "userdata -> C pointer" entry, and the five action protos' `m_target`
  is the only untraced consumer left.

**All three leads are closed below (§9.10).** The last paragraph above is kept as written in round 4 so
the change of mind is visible: leads (a) and (b) did not need a decompile at all.

### 9.10 Round 5: `m_target` is a character **or a settlement** -- and it was in the exe all along

**Item 1 of the round-5 brief is SETTLED.** Round 4 recorded "one engine-side fact is missing: what
`m_target` *is*" and parked it, because no shipped `.luac` calls `OpenAgentOptionsPopup` and the
popup therefore has no shipped caller to read. The answer was not in the popup at all: it is in the
**exe's own embedded API documentation**, in the registration stub of the very command the popup
ends in.

#### 9.10.1 How the answer was found, and how it was checked

Ghidra is not installed on this machine, so the exe's `.text` was read directly instead: the image's
section table gives `.rdata` at file offset `0x00F05E00` = VA `0x01307000` (image base `0x00400000`,
`.text` at `0x00401000`), and `llvm-objdump` was used to read the registration stubs.

**The positive control first, because without it a negative here means nothing.** Every bound method
is registered by one five-instruction stub:

```text
push <doc string>        ; the method's own signature line
push <name string>       ; the name scripts call it by
push <the C function>
mov  ecx, <a build id>
call 0x00998C50          ; one shared "bind it" helper
ret
```

`RegionsPublicOrders` is one: doc `Returns the public order in the region` at `0x01364BC8`, name at
`0x01364BF0`, function `0x009F11D0`, at `0x00429E90`. And the round-4 anchors re-derive exactly:
`"%s (0x0%x)"` at `0x013F3C3C` is pushed once, inside `0x01058F60` (§9.4's `__tostring`); the
`Pointer<CHARACTER>` string at `0x0136C4C8` is pushed once, at `0x009C124A` inside the argument
converter `0x009C1230` (§9.5). Both agree with round 4 to the byte, so the method is sound.

#### 9.10.2 The registration, read verbatim

`MoveIntoTarget`'s stub is at `0x004295D0`:

```text
004295d0  68 50 4b 36 01   push 0x01364b50     ; the document
004295d5  68 b8 4b 36 01   push 0x01364bb8     ; "MoveIntoTarget"
004295da  68 50 e1 9e 00   push 0x009ee150     ; the C function
004295df  b9 69 4e 5c 01   mov  ecx, 0x15c4e69
004295e4  e8 67 f6 56 00   call 0x00998c50
004295e9  c3               ret
```

The string at `0x01364B50` is **CONFIRMED** and reads, in full:

```text
In: Character (agent), character or settlement (target), bool (research - steal if enemy settlement)
```

So `MoveIntoTarget` takes **an agent character, a target that is a character or a settlement, and a
boolean** -- and that boolean's documented meaning ("research -- steal if enemy settlement") is
exactly what separates the two actions that pass it (`research` 16 at `agent_options.lua:149` and
`steal_research` 32 at `:154`) from the three that do not (`visit` `:139`, `embed` `:144`,
`counterspy` `:164`). Round 4 had already measured that split and could not explain it; the document
explains it.

**`m_target` is therefore a character or a settlement, and never a bare military force.** That is
also why `Initialise`'s very first instruction is `string.find(tostring(target), "CHARACTER")`
(`agent_options.lua:2-9`): one popup serves both kinds and only the character branch builds the
`CampaignCharacter` handle its teardown releases at `:89` (§9.7). Round 4 read that test as weak
evidence that the target was "sometimes" a character; it is strong evidence that it is one of two
documented kinds.

#### 9.10.3 The engine's own call sites, which agree and say one thing more

The name `OpenAgentOptionsPopup` is pushed at exactly **two** places, `0x009C1E69` and
`0x009C1F29`. Each belongs to a small `__thiscall` that ends `ret 0x14` -- it pops five arguments --
and each pushes them in this order:

```text
0x009C1E63  6a 00              push 0
0x009C1E65  6a 00              push 0
0x009C1E67  6a 05              push 5
0x009C1E69  68 30 ef 36 01     push 0x0136ef30   ; "OpenAgentOptionsPopup"
0x009C1E6e  50                 push eax          ; a Lua value from [this + 0xc4] + 0x18
0x009C1E73  e8 48 65 a9 ff     call 0x004583c0   ; begin the call: (value, name, arity 5, 0, 0)
...                                 then five pushes of the five arguments, then
0x009C1ED0  e8 8b 91 a9 ff     call 0x0045b060   ; finish it
```

The five values pushed afterwards are, in order, the function's five incoming stack arguments -- so
the engine's caller is itself a five-argument method that forwards its five arguments unchanged.
The two functions are identical except for **one** call in the middle (`0x0097DEC0` in one,
`0x0097E060` in the other): two different `Lua::Pointer<T>` argument converters, one per branch.
That is the same kind of pairing as §9.5's `Pointer<CHARACTER>`-versus-signature distinction, and it
is the code-level form of "character or settlement".

#### 9.10.4 Which `Initialise` parameter is which -- CONFIRMED, and it settles item 2's identity

`Initialise`'s prototype (`agent_options.lua:34`, `numparams = 5`) reads its parameters like this:

| pc | register | what it is |
|---|---|---|
| 0 | `R[0]` | stored as upvalue `m_src` (`SETUPVAL`) |
| 1 | `R[1]` | stored as upvalue `m_target` (`SETUPVAL`) |
| 2-9 | `R[1]` | `string.find(tostring(target), "CHARACTER")` -- the kind test of §9.10.2 |
| 47 | `R[2]` | the mask: `bit.band(R[2], action.Mask)` |
| 82 | `R[3]` | `tostring(R[3]) .. "%"` into the `percent_dy` of the button whose `Mask` is `AGENT_ACTION_RAKE_EMBED` |
| 116 | `R[4]` | `tostring(R[4]) .. "%"` into the `percent_dy` of the button whose `Mask` is `AGENT_ACTION_RAKE_SABOTAGE_ARMY` |

So the engine call is exactly

```text
OpenAgentOptionsPopup(src, target, mask, infiltrate_pct, sabotage_army_pct)
```

-- the two percentages are the **fourth and fifth** arguments, and they are attached to **Infiltrate**
and **Sabotage Army** respectively. Round 3's "one of `Initialise`'s last two arguments" is now
specific, and the order is not a guess: `R[3]` feeds the `embed` branch at pc 71-87 and `R[4]` the
`sabotage_army` branch at pc 88-120.

#### 9.10.5 What the two percentages mean -- CONFIRMED from the shipped advisor, and how the exe asks

The game's own localisation, found this round by following the four mp3s that exist for nothing else
(`advisor\1207..1210_campaign_advice_ui_agent_options_panel_thread_1.mp3`), says:

```text
advice_levels_onscreen_text_1437352507 =
  "To infiltrate a city, select your spy and then right-click on the city in question. A menu will
   appear giving you the options of sabotage, assassination or infiltration. The percentages show the
   spy's chances of success in each activity."
advice_levels_onscreen_text_1547606257 =
  "Clicking on the infiltrate button means your agent will begin spying upon arrival at his
   destination. ... As with all subterfuge actions there is a percentage chance of success, and
   failure may mean capture and execution."
advice_levels_onscreen_text_2055775840 =
  "Select the infiltrate option. If the spy has enough movement points he will automatically try to
   enter the city. You will get a message to report his success or failure."
```

**Both numbers are a chance of success in percent.** That is settled by text, not inference.

The engine's two call sites each ask the *same* interface for the *same* agent character twice, with
two different integer ids -- **5** and **11** -- and hand the answers on as arguments 4 and 5
(`0x009D1FC0` / `0x009D2000` at `0x00A0C3E5` and `0x00A0C400`). So both numbers come off the agent
alone and **neither depends on the target**, which is what makes the mapping to formulas a mapping of
*ids* rather than a free choice. The two formulas are already ours and already CONFIRMED from the exe
(`ntw_sim::campaign::agents`):

- **Infiltrate** = `spy_chance(agent, SpyTarget::Settlement)` (`0x00922A70`). The advisor closes it:
  infiltrating a city *is* spying on it on arrival, and "he will automatically try to enter the city"
  is the model's `spy` order (`0x0094CCB0`).
- **Sabotage Army** = `army_sabotage_chance(agent, force)` (`0x00922BA0`). **Which force is INFERRED**:
  the documented target is a character or a settlement and `agent_options.lua:159` calls
  `CampaignUI.SabotageArmy(agent, force)` with a force the engine chose without asking. The named
  reading is "the target's force" -- the force a target character commands, else the foreign force
  whose commander stands in a target settlement. The alternative is *any* force the agent may sabotage
  anywhere (`valid_sabotage_army_target`); nothing found here refutes it.

#### 9.10.6 What is still open, and it is now a different question

`MoveIntoTarget` stays a **logging stub**, and the reason is no longer the contract -- the contract is
known. What is missing is the *behaviour*: what the agent does on arrival. The model already has the
movement half (`CampaignCommand::MoveCharacter` -> `walk`) and already has the resolve half
(`CampaignModel::spy` for `embed`); what it has no answer for is the **move-then-resolve** step, i.e.
the agent order queue. Queueing a bare `MoveCharacter` would put an agent in a foreign city with
nothing to do there, which is worse than no button, so the five mask bits stay unset.

**Next round's target, now sharp and small:** `CampaignModel`'s agent order queue. With it, `embed`
(bit 8) is `MoveIntoTarget(src, settlement)` = move, then on arrival `spy(src, Settlement(settlement))`
-- the advisor's sentence, the model's `spy`, and the documented target kind all agreeing.

#### 9.10.7 One correction this round found in our own code

`FactionDetails`' `Address` field had no address kind of its own: it was tagged
`TAG_FORCE | (1 << 39)`, a tag outside `TAG_MASK`, so it never collided with a real force address but
had no entry in the `__tostring` table and therefore printed the `void` stand-in. Sixteen shipped
call sites read that field back (`diplomacy_panel.luac`, `campaign_hud.luac`,
`objectives_screens.luac`, ...). It now has `TAG_FACTION` and the exe's own
`EMPIRECAMPAIGN::FACTION` signature string, read out of the constant pool the same way as the other
seven (VA `0x01371770`, the third `EMPIRECAMPAIGN::` signature string in the same run as
`CHARACTER` at `0x00F71179`).

The audit also confirmed the other seven kinds and found no second gap. The one address kind with no
signature string is the recruitment card's `item_ptr`: nothing in any shipped script can stringify it
(it goes straight back to `CampaignUI.CancelRecruitment`, `template.RecruitmentCard.lua:99` pc 22-30),
so it keeps the `void` fallback rather than gaining an invented name.

### 9.11 Review 0-E (2026-10-06): what of §9 stands, and on what evidence

The review environment has no install and no exe, so nothing in §9 was re-read; the verdicts below
are on the evidence *recorded* here. No decompile for any §9 address is kept in
`ghidra_evidence/0e`.

- **Address format (§9.1-9.6): accepted as CONFIRMED on the listings quoted inline** (the raw
  instructions of `0x0105AE10` and `0x01058F60`, with their string constants). Only the
  `CHARACTER` substring matters to any shipped script, so the exact signature text of the other kinds
  is behaviourally immaterial. Note the mixed address spaces: `0x00F71179` / `0x00F72343` (§9.6) are
  below `.rdata`'s VA `0x01307000` and so read as **file offsets**, while `0x01371770` (§9.10.7) is a
  VA; a later reader should not look them up in the same space.
- **Interning (§9.8): accepted, no breakage found.** Every address the campaign HUD hands out goes
  through `CampaignUi::entity`; `entity_payload` is called only from `entity` and one test; `entity_of`
  decodes both the table and a bare payload; there is no `Value` comparison or `Value`-keyed map on the
  Rust side; `host.rs` component addresses and the campaign script state (`game.rs`) are separate
  families and untouched. One PROVISIONAL difference beyond §9.10's `__index` note: `type(address)` is
  `"table"` here and `"userdata"` in the original (and it was `"userdata"` here before, with light
  userdata). Whether any shipped `.luac` asks `type()` of a *campaign* address was not checked
  (`luac_scan` needs the install) -- **check it before the next change to `entity`**.
- **`m_target` (§9.10.2): accepted as CONFIRMED** on the registration listing and the verbatim
  document string quoted there. §9.10.3's "two converters, one per branch" is an **interpretation**
  (INFERRED) that agrees with it, not separate proof.
- **The two percentages (§9.10.5): downgraded to INFERRED.** What they *mean* (a success chance) is
  CONFIRMED by shipped loc. *Which* model chance each is, is not: by §9.10.5's own reading the engine
  asks the **agent alone** with ids 5 and 11 -- the indices of `can_spy` and `can_sabotage_army` in
  `agents::ABILITIES` -- so the original's numbers would **not depend on the target**, whereas
  `spy_chance(.., Settlement)` and `army_sabotage_chance(.., force)` both do. The formulas used are
  the model's existing CONFIRMED ones (no new numbers), but the mapping is a reading, and the
  self-contradiction is the named lead: decompile `0x009D1FC0` / `0x009D2000` and keep it.
- **Target picker rows: one real mismatch fixed.** The duel row's `Chance` was the pistols chance
  only, but `CampaignModel::duel` rolls at the chance of `duel_weapon` (`0x00AAAC30`: the target picks
  the weapon worse for the challenger). The row now shows the smaller of the two `duel_chance`s, which
  is the number the duel is actually rolled at whichever way a tie is flipped. INFERRED that the
  original's single `Chance` is that number.
- **`CanHarrass`**: INFERRED that the engine asks the same two questions as the agents panel.

## 10. State enter / exit functions: why most campaign panels could not be closed (2026-10-07)

Campaign bug 1 (objectives, building browser, lists, research checkmark, agent popups' X). Root
cause, CONFIRMED: the host never ran a layout state's `enter_function` / `exit_function`.

- The shipped close buttons of `technology`, `building_browser`, `entity_lists`, `missions`,
  `agent_action` and `agent_options` are `template.button_close.lua` instances that bind **no event**.
  Their "depress" state has the exit function `OnLeaveDepress` (`ui_probe -- states <layout>
  button_close`); the script's `OnLeaveDepress` calls `Root.LuaCall("ClosePopup", ParentPopup)` with
  the address the panel script stored (`missions.lua:38-39`). Diplomacy worked because its layout binds
  `OnMouseLClickUp -> OnLeaveDepress` instead; government has its own close script.
- Exe (renamed in Ghidra): `ApplyUIComponentStateTransition` `0x01035620` looks up the transition for
  the event key and, when the state changes, runs the old state's exit function with the new state's
  name (`CallUIStateExitFunction` `0x0102DFD0`, skipped when the old state's +0x115 flag is set), then
  `CallUIStateInitAndEnterFunction` `0x0102DF60` on the new state (InitState via `0x0102B480`, then the
  enter function with the old state's name; skipped when the new state's +0x115 is set).
  `ApplyNamedUIComponentStateFromScript` `0x01035B30` (script `SetState`) runs only `0x0102DF60`:
  InitState and the enter function, never an exit function. CONFIRMED from the code; +0x115 = our
  `disabled` is INFERRED (the click handler `0x0102E340` refuses clicks on it).
- Ours: `host.rs` `state_transitioned` / `call_state_function`, and SetState's enter call. Tests:
  `campaign_ui::panels_close_from_their_close_buttons` (six HUD panels, plus reopening),
  `an_agent_action_button_opens_the_target_picker` (the picker's X), host unit test
  `state_changes_run_the_states_enter_and_exit_functions`.
- CONFIRMED as the original and kept on purpose (code-site notes in `host.rs`): a pointer transition
  into a disabled state skips InitState as well as the enter function (`0x01035620`), while SetState
  runs InitState on every call, also into a disabled state, and the enter function on every call to
  an enabled state, also the one it is already in (`0x01035B30` → `0x0102DF60`); "depress" → "depress
  mouse off" (press, then slide off the button) leaves "depress", so `OnLeaveDepress` runs and the
  panel closes too.

### 10.1 Event order and the review fixes (2026-10-07)

- **Event first, then the transition**, CONFIRMED for every pointer handler in the exe: click
  `0x0102E340` (now `DispatchUIComponentLeftClickUp`), press `0x0102E1F0`
  (`DispatchUIComponentLeftClickDown`), mouse on `0x0102EBC0` (`DispatchUIComponentMouseOn`), mouse
  off `0x0102EB40` (`DispatchUIComponentMouseOff`), and `0x0102E4B0` / `0x0102E730`. Correction
  (round 5): those last two are not a right-button pair. They fire event ids 4 and 9, and the ids
  index the exe's event-name table (`0x01464170`, now `g_szUIEventNameTable`, 30 names; the lookup
  `0x01038580`, now `FindUIEventIndexByName`, maps a name to the same index): 0 OnDrag, 1 OnMouseMove,
  2 OnMouseLClickDown, 3 OnMouseLClickUp, 4 OnMouseLDblClick, 5-7 right button, 8-9 middle button.
  So `0x0102E4B0` = left double click (event 4, transition key 8, no pressed flag) and `0x0102E730` =
  middle release (event 9, key 7; released elsewhere key 13, pressed flag +0x15E); INFERRED (the
  binding of names to component slots by this index is not traced). The press handler stores the
  pointer position (`g_flUIPressPosX/Y`), fires its event unless the state is disabled, sets the
  pressed flag +0x15C, transitions with key 2, then calls the component's notify (vtable +0x48) if
  the NEW state is not disabled. Mouse on / off fire their event whenever the state is interactive
  (+0x114), disabled or not, then transition (keys 0 / 1). Each offers the event
  to the children first (top-most first, stopping at the first that takes it), then on the component:
  only an interactive state (state +0x114) on an input layer (component +0xE0) at or above the global
  minimum layer reacts; the click is taken when the pointer is on it and it was pressed (+0x15C, set by
  the press handler) or when it captures the mouse (+0xE5). A disabled state (+0x115) gets no press /
  click event (no focus change either), but its transition still runs. Pressed but released elsewhere
  → key 11 transition, no event. The click moves the input focus (not forced) before the event.
- `0x01035620` reads the current state when it runs, i.e. after the event's script; after the exit
  function it checks the component still has its script environment and re-reads the current state,
  so the state an exit function moved to (SetState) is the one that gets InitState and the enter
  function, with the old state's name. CONFIRMED.
- **Which state the click's transition starts from** (round-5 finding 1), CONFIRMED from the
  disassembly: `0x0102E340` passes only the component to `0x01035620` (no state captured before the
  event; its own copy of the state pointer is used only for the interactive / disabled tests), and
  `0x01035620` loads the component's current state (+0xAC) on entry and looks the key up in THAT
  state's transition map (`0x0102D970`, now `FindUIStateTransitionTarget`: a hash map keyed by
  event key xor `0x4A545EED`; no entry = no change). So a click handler that sets a state (the card
  manager's `SetState("Selected")`) makes the click's transition start from that state. The shipped
  cards are safe: `CampaignUnitCard`, `CampaignCharacterCard` and `BattleUnitCard` have Default
  --mouse on--> roll and roll --mouse off--> Default only, and their "Selected" has no transition
  for any key (`ui_probe -- statekey Selected <0|1|2|3|11> ui/templates/`); only `RecruitmentCard`'s
  "Selected" leaves on mouse off (to "Unselected"). Tests: host
  `the_transition_starts_from_the_state_the_click_left` (also the case where the state the handler
  set has a click transition of its own: it is left at once), campaign
  `selecting_an_army_shows_its_unit_cards` (hover, press, release, leave on two real army cards: the
  clicked one stays "Selected", the other goes back to "Default").
- Fixed on the way (the card test's hover failed): the army card's `UnitRecord` was the unit key, but
  `template.CampaignUnitCard.lua` reads `UnitLimit` and `Key` from it on mouse-on and hands it to the
  unit tooltip's `Initialise` as the unit record (bytecode lines 122, 406-413, CONFIRMED); it is now
  the same details table a recruitment card's `unit_record` is (INFERRED that the two are the same).
  The tooltip compares an artillery unit's `Range` with 0, so `Range` is always a number now: the
  unit's projectile range, else the range of its gun type's first projectile
  (`gun_type_to_projectiles`), else 0. Round 15: the gun's LONGEST effective range, else the own
  projectile's, else 0 (CONFIRMED, `0x008DF190`; see "Round 15 fixes").
- Ours now: `pointer` fires the event, then transitions from whatever state the script left.
  Layout (superseded in round 15, see "Round 15 fixes" below): lazy, at the first geometry read
  after a change (`host.rs` `lay_out_if_stale`); the round 1-14 per-event layout rules and their
  layout-count tests are gone. Test: `the_enter_function_sees_the_new_states_layout`. InitState
  goes through `__ntw_call_if_defined` when `__ntw_has_handler` says there is one, so host hooks
  that answer it run (`init_state_is_set_up_only_where_there_is_one_to_call`).
- **Review-panel tab requests** (`ReviewPanelTabSelectionSet_1_Indexed(i)`), CONFIRMED from the exe
  (`0x009F48E0` → `0x009C14B0` → `0x00A20620`): i is checked against the CURRENT selection's tab
  list; out of range (or a tab whose state +0x24 is 0 or 2, meaning UNKNOWN, not modelled) the
  call does nothing, silently; in range it clears the review panel, tells the old tab it is
  deselected, makes i current and generates it, all inside the call. A selection change opens
  tabs by its own rule: the same selection keeps the current tab by identity, a different one
  opens its first tab. Test: `a_tab_request_applies_to_the_selection_it_was_made_for`.
- **Tab change inside the request** (round 13, design replaced). Rounds 1-12 deferred the tab
  change to an "after-script" pass at the outermost return, which kept finding bugs (a tab asked
  for during that pass or during `campaign_select` stayed pending; re-entry guards; the order
  differed from the exe). Ours now does what `0x00A20620` does (CONFIRMED, above): inside the
  request, in this order: `ClearReviewPanel`, `ReviewPanelTabInit(.., old, 1)`, i made current,
  `ReviewPanelTabInit(.., i, 2)`, `Generate*Panel(info)` (`campaign.rs` `select_tab`); the exe
  has no layout step (round 15: geometry is current at every read, ours lazily). Consequences,
  INFERRED from that synchronous call (no shipped script requests a tab from anywhere but
  `SelectTab`, the one call site in the UI scripts): a generator that asks for another tab has it
  generated inside its own call (during a selection change the request is refused instead, round
  15); requests nest without a
  limit of ours, like state functions, under the same native-stack guard, so an endless ping-pong
  ends with Lua's "C stack overflow" error (logged), not a crash; there and back again rebuilds
  both tabs (before: optimised away). Nothing is ever left pending, and the host's after-script
  machinery (`script_returned`, `campaign_after_script`, the
  pending tab) is gone; the root call helper is private. Tests:
  `a_generator_asking_for_a_tab_during_a_tab_change_gets_it_inside_that_change`,
  `an_endless_tab_ping_pong_ends_with_luas_error_not_a_crash`.
- **Round 14 review fixes** (the layout, tab-build and Range parts were replaced in round 15, see
  "Round 15 fixes"). InitState's image metrics go to the state the call was made for,
  also when InitState called SetState (`0x0102DF60` keeps its state pointer, CONFIRMED). A tab
  request made while a selection change builds its tab list (`ClearReviewPanel`, the
  `CreateReviewPanelTabAtPosition` loop) finds the list empty and is ignored, so each tab is
  generated once (INFERRED from the exe refusing a tab in state 0; the nested case not traced).
  `campaign_select`, the capture screen, the agent options popup and the funds update are engine
  events: each ends with `finish_event` (one layout). A key lays out between the focused
  component's OnKey and the shortcut holder's. New components are all offered to
  `__ntw_call_if_defined` (the default dispatcher is the exe's script-environment check; a hook
  sees scriptless ones too), and a batch of them is laid out once at the end (image metrics do
  not change rects). Range: one model rule, `GameDatabase::primary_projectile` (own projectile,
  else the gun type's first, the shot battle loads), used by battle, AI strength and the unit
  card (was: the longest; INFERRED that the card shows the loaded shot). Tests:
  `init_state_metrics_go_to_the_state_it_was_called_for`,
  `the_shortcut_holder_sees_the_focused_components_key_laid_out`,
  `new_components_reach_the_hook_and_lay_out_once_per_batch`,
  `a_tab_request_while_the_tab_list_is_built_is_ignored_and_the_change_lays_out`.
- **Round 15 trace (2026-10-07, static, ghidra-mcp). Implemented in the round-15 fixes (next
  item).** Three round-14 INFERRED points settled:
  1. *Layout scheduling* (CONFIRMED): the original has no layout pass, dirty flag or per-frame
     layout; geometry is current at every read. Width/Height read the current state's +0x24/..
     (`0x01037880`); a position is computed on each read from the parent chain (vtable +0x14
     `0x0102FEA0`: parent position + own offset +0x94/+0x98; MoveTo, vtable +0x10 `0x0102D780` →
     +0xC `0x0102D710`, stores the offset); Resize (vtable +4 `0x010324E0`) re-docks and resizes the
     docked children inside the call; SetState (`0x01035B30`) lays nothing out (the size is the
     state's). UIComponent vtable `0x013F225C` (ctor `0x0101E270`). So a lazy layout (mark stale on
     every change, lay out on the first geometry read) is observably the original's. Known gap,
     separate item: the exe re-docks children only on the parent's Resize; our full relayout
     re-docks every child every time.
  2. *Tab request during a selection change* (CONFIRMED): every selection handler
     (`HandleSettlementSelected 0x009C37A0`, `HandleFortSelected`, `0x009C2AF0`, `0x009C3FB0`,
     `0x009B8470`) frees the old tab set, sets manager+0xEEC to 0, builds the new set and only then
     stores it. The build runs every script call: base ctor `0x00998BC0` calls `ClearReviewPanelTabs`
     then `ClearHud` (zero-arg calls run at once in `0x004583C0`); each tab's ctor (`0x00986FD0` →
     `0x0099A060` → `0x0099A0B0`) calls `CreateReviewPanelTabAtPosition(title, key, i, 1)`; adding it
     (`0x009C97B0`) either opens it at once when it is the kept tab (state 2 → `0x00A20620` on the
     new set: `ReviewPanelTabInit(i, 2)` + its `Generate*Panel`, no `ClearReviewPanel` since +0x25 is
     still 0) or calls `ReviewPanelTabInit(i, 1)`; `OpenFirstEnabledPanelTab 0x009DA2A0` then opens
     the first enabled tab if none was. `SetSelectedEntity` runs after the store. A script's tab
     request (`0x009F48E0` → `0x009C14B0`: ECX = [manager+0xEEC], no null check → `0x00A20620`)
     made anywhere inside that build (also from the first tab's generator) dereferences a NULL tab
     set (reads +0x14). Lua's protection is setjmp/longjmp (`0x00FED0F0` / `0x00FED2A0`), so it does
     not catch the access violation; whether an outer SEH handler does is UNKNOWN (debugger: break
     at `0x00A20620` with ECX == 0; no shipped script triggers it). Ours should refuse such a
     request and log it once as the original's null dereference. Order differences found (ours:
     `ClearReviewPanel`, `ClearReviewPanelTabs`, all creates, then one `ReviewPanelTabInit(cur, 2)`):
     the original sends no `ClearReviewPanel`, sends `ClearHud` on every selection, and interleaves
     create + `ReviewPanelTabInit(i, 1|2)` per tab.
  3. *Unit card Range* (CONFIRMED): the card snapshot builder `0x008DF190` sets +0x5C (Range) to
     the gun type's +0x60 when the unit has a gun type, else its own projectile's range (+0x60), else
     0; +0x58 (Firepower) is the gun type's +0x64 or 0. Land unit record (ctor `0x00E8CFF0`, linker
     `0x00EDB840`): +0xC4 gun type, +0xE0 own projectile. GUN_TYPE_RECORD ctor `0x00F41750`: +0x58/
     +0x5C its projectile list, +0x60 = the LONGEST range of its projectiles (unsigned max from 0),
     +0x64 = the max of a second per-projectile value; the list is then sorted ascending by shot
     type (`0x00F25CF0`, key = projectile +0x24 → shot type record +0x10, the enum value from
     `0x00F59030`, unknown name → 0; record ctor `0x00F45F70`). So round 14's "first projectile" for
     the card was wrong (the pre-round-14 "longest" was right), and the battle's "first" gun
     projectile is the first in shot-type order (insertion sort for ≤ 32 entries keeps table order
     among equals), not table order.
     *Unsigned* (CONFIRMED, read 2026-10-07): the gun type ctor keeps its maximum with
     `CMP EAX,ECX; CMOVNC ECX,EAX` at `0x00F41893` (unsigned >=, from 0), and `0x008DF190` converts
     the chosen range (gun type +0x60, else own projectile +0x60, else 0; `0x008DF56D`-`0x008DF586`)
     to float with the unsigned idiom at `0x008DF588`-`0x008DF5FC` (CVTDQ2PD, then ADDSD of the
     double at `0x01318130` indexed by bit 31: {0.0, 4294967296.0}) before storing +0x5C. So a
     negative modded range is a huge value that wins the gun's maximum and shows as about 4.29e9,
     never negative. Ours (`GameDatabase::unit_card_range`, u32) matches; above 2^24 the exe's
     f32 rounds, ours shows the exact integer (modded values only; PROVISIONAL in ours).
- **Round 15 fixes (2026-10-07), the round-14 review items 1-10 from the trace above.**
  - *Lazy layout* (items 1, 2, 3, 7; `host.rs` `lay_out_if_stale`, the one place the rule is
    written): every change marks the layout stale (round 16: geometry changes only); every geometry read (UIComponent
    Position / Dimensions / Width / Height / Bounds, MoveTo, Resize, SetStateText, InitState's
    text measurement, a click's position, hit tests, the credits builder, `UiScriptHost::world`,
    which the renderer reads) lays the tree out first only if it changed. Nothing lays out
    eagerly and an event's end does not either (`finish_event` and the layout counts per event are
    gone), so N changes between reads cost one layout and an event that changed nothing none.
    Observably the exe's (geometry current at every read). Gap kept PROVISIONAL: ours re-docks
    every child, the exe only a resized parent's.
  - *InitState only where there is one* (item 1): `call_init_state` asks `__ntw_has_handler`
    first and builds no state table and measures no text otherwise (the exe calls it only on a
    component with a script environment); a host hook that supplies an InitState answers
    `__ntw_has_handler` too.
  - *Selection in the exe's order* (point 2 of the trace): `ClearReviewPanelTabs`, `ClearHud`
    on every selection, no `ClearReviewPanel`; per tab `CreateReviewPanelTabAtPosition(.., 1)`
    then either the kept tab's opening (`ReviewPanelTabInit(i, 2)` + its generator) or
    `ReviewPanelTabInit(i, 1)`; the first tab opened after the list when none was kept; then
    `SetSelectedEntity`. A tab request during that build is refused and logged once per HUD
    (item 8; the exe reads a NULL tab set there). `select_entity` folded back into
    `campaign_select` (item 10); the tab title lookup is one helper.
  - *Card Range* (items 4, 5, 6): ghidra-mcp settled the field: the projectile RECORD (built from
    the file row by `0x00F45970`, a different layout from the reader's `0x00F3EF70` row) has +0x60
    = the row's +0x7C, `projectiles` column 11 effective range, +0x78 = column 17 damage (the gun
    type's +0x64 "second value" is the max of damage, ftol'd by `0x0126E2B0`), +0x24 = the shot
    type record. `GameDatabase::unit_card_range`: gun type → its longest effective range, else
    the own projectile's, else 0 (CONFIRMED). `gun_projectiles` is in the exe's shot-type order
    (stable); `primary_projectile` (battle, FX, audio, AI) takes its first without allocating;
    `LandUnitView.projectile` removed (one rule). PROVISIONAL: a gun type key with no
    `gun_types` row counts as a gun (we do not load `gun_types`; the exe falls back to the
    projectile).
  - *Unit details cache* (item 9): `Rc` entries, no borrow held while the table is built.
  - Tests: host `layout_happens_only_at_a_geometry_read_after_a_change`,
    `the_shortcut_holder_reads_the_focused_components_change_laid_out`,
    `init_state_is_set_up_only_where_there_is_one_to_call`,
    `init_state_measures_the_layout_the_earlier_init_states_left`; campaign
    `a_selection_change_builds_its_tabs_in_the_exes_order`,
    `a_tab_request_while_the_tab_list_is_built_is_refused_and_logged_once`; ntw_data
    `gun_shots_are_in_shot_type_order_and_the_card_range_is_the_longest`. The round-14 tests
    that counted layouts per event are replaced by these.
- **Round 16 (delta review of round 15).**
  - Layout staleness is a geometry-only flag (`UiWorld::layout_dirty`, set by `get_mut` and the
    structural methods; `get_mut_appearance` for texts, colours, images, visibility, tooltips,
    input flags leaves it), plus `Inner::layout_stale` for the screen and page rule. Before, any
    change bumped the counter the layout keyed on, so SetStateText + Height in a loop laid out
    every pass. A `world()` borrow held while a screen change is pending no longer panics: the
    layout waits for the first read after the borrow (nothing in the tree can change meanwhile).
  - Gun shots: one shot-type lookup, `ntw_sim::battle::attributes::shot_type_value` (the battle
    setup, `change_shot_type`, the unit card and `ntw_data` call it); `GameDatabase` indexes each
    gun type's projectiles in shot order at load, so a volley's lookup scans no table.
  - Tab requests from the generators a selection change runs (the kept tab's, and the first tab's
    through `OpenFirstEnabledPanelTab 0x009DA2A0`) stay refused: ghidra-mcp shows both run inside
    `ConstructSettlementPanelTabs 0x0099A200`, which returns before `HandleSettlementSelected
    0x009C37A0` stores the set at +0xEEC, and the Lua handler `0x009F48E0` (now a function in
    Ghidra, `HandleLuaReviewPanelTabSelectionSet`) passes the manager global `0x015C4938` to
    `0x009C14B0` (`MOV ECX,[ECX+0xEEC]; JMP 0x00A20620`) with no NULL check (CONFIRMED).
  - Tests: host `text_changes_between_geometry_reads_cost_no_layout`,
    `a_second_world_borrow_with_a_screen_change_pending_does_not_panic`; ntw_sim
    `shot_type_value_is_the_exes_enum`; ntw_data `gun_shots_are_in_shot_type_order_and_the_card_range_is_the_longest`
    (reads the index after the junction table is cleared).
- **New components** (round 8), CONFIRMED from the disassembly of `0x01034210`, the component
  initialiser that every create path calls: it makes the initial state current, initialises the
  children first (each subtree in turn), then, if the component has a script environment, calls
  `0x0102DF60` on it (InitState, then the enter function) with an EMPTY old-state name, then fires
  its `OnCreate` event if bound. Ours (`init_new`) now does the same order and runs the enter
  function too (before: parent first, InitState only). Not modelled: `OnCreate` (only
  `unit_bling.luac` uses that name). `0x0102DF60` has exactly three callers (transition, SetState,
  initialiser); ours has one entry point for all three, `run_state_entry`, which owns the stack
  guard, the hook-routed InitState, the layout after it and the enter function. Test:
  `created_components_get_init_state_and_their_enter_function_children_first`.
- Layout before a hook-supplied InitState: the host asks `__ntw_has_handler` (ui_prelude.lua),
  which a host hooking `__ntw_call_if_defined` must answer too. Test:
  `a_hook_supplied_init_state_sees_the_new_states_layout`.
- Stack segments when the thread's stack cannot be measured: stacker sets the limit of every
  segment it makes (stacker 0.1.25 `_grow`), so only the first call moves to a segment and the
  calls inside it measure it; it is not a segment per call (round-8 finding 4 rejected).
- `Range` on every unit card, melee units too: the exe's builder `0x009AA5E0` sets it with its
  float setter from the card snapshot +0x5C, unconditionally (CONFIRMED); round-8 finding 3
  rejected.
- Tab change: the old tab is now set back to state 1 (`ReviewPanelTabInit(.., old, 1)`) before
  the new one gets 2, as `0x00A20620` does through the tab's state change `0x009C7D80`
  (CONFIRMED; that also confirms 1 = not selected, 2 = selected).
- Round 9: requesting the tab already current is refused by the exe (`0x00A20620` refuses a tab
  whose state +0x24 is 2 = selected; `0x009C7D80` stores that state through the tab's +4
  sub-object, CONFIRMED), so ours no longer rebuilds it. The battle HUD's hold-back of the root's
  InitState is gone: child-first initialisation (`0x01034210`) already runs root.lua's InitState
  after battle_hud.lua's. A pointer transition runs InitState / enter only on a component that
  still has its script environment (`0x010587B0`, CONFIRMED). Range on the recruitment card's
  record: INFERRED (only the army card builder `0x009AA5E0` is traced).
- A key: the focused component's `OnKey`, then the shortcut
  holder's `OnKey`, then one layout. The focus-first order stays INFERRED (the exe's key dispatch
  is not traced).
- A pointer transition into a disabled (+0x115) state runs no InitState and no enter function:
  `0x01035620` tests the new state's +0x115 at `0x010356A9` and skips `0x0102DF60` altogether
  (CONFIRMED, disassembly). Ours does the same; its new size is laid out at the end of the event.
  Test: `a_transition_into_a_disabled_state_runs_no_init_state`.
- The army cards' `UnitLimit`: always an integer in the exe (`0x009AA5E0` sets it with the integer
  setter `0x0044DE40` from the card snapshot +0x94, CONFIRMED), and every shipped reader tests
  `0 < UnitLimit` (CampaignUnitCard:406, recruitmentcard:259), so 0 = no limit. Source of the exe's
  value UNKNOWN; ours 0 (PROVISIONAL). The details' values are worked out once per unit key (the
  database and localisation are fixed for the HUD's lifetime); every card gets its own table. Enter state fixed before its
  InitState (as `0x0102DF60` keeps its state pointer), skipped if the component is gone. A click
  takes the input focus by the state it starts from (`0x010367F0` asks `0x01029FB0`, the current
  state's +0xD4, before the transition; CONFIRMED). Tests: host
  `the_click_event_fires_before_the_transition`,
  `the_transition_starts_from_the_state_the_click_left`,
  `a_release_elsewhere_and_a_disabled_click_still_transition`,
  `a_component_gone_after_its_exit_function_gets_no_enter_function`,
  `the_enter_function_is_the_state_the_exit_function_left`,
  `init_state_after_a_transition_sees_the_new_states_width`,
  `a_click_takes_the_focus_by_the_state_it_starts_from` (real text field: frontend
  `typed_file_name_saves_the_army`); campaign
  `pointer_events_without_a_lua_event_still_finish_the_script_work`.
- **Recursion: no limit of ours.** None of `0x01035620`, `0x0102DF60`, `0x0102DFD0` limits nesting
  (CONFIRMED), so a script that keeps changing state from its state functions runs until it stops
  by itself (a bounded A → B → A completes) or until Lua 5.1's nested C-call limit raises "C stack
  overflow" (INFERRED that the exe keeps Lua's default), which we log. The only path on which these
  calls nest is SetState from a script (its InitState and enter function); when that call finds
  less than 256 KiB of native stack left it continues on a fresh 2 MiB segment, so a small thread
  stack grows instead of overflowing. The memory is bounded by a byte budget, not a depth: the
  segments one thread holds stay within 32 MiB (given back when a call ends, also by a panic),
  past which the call fails with Lua's "C stack
  overflow" error before taking memory (a guard for the host process, not a nesting cap; the exe has
  none, and in our tests Lua's own limit ends a runaway script before any segment is needed). Test:
  `nested_state_function_calls_stay_within_the_stack_budget`. A pointer transition's own calls start
  at the top of an event and are not wrapped.
- **SetState runs the enter function every time**, also into the state the component is already in
  (CONFIRMED, `0x01035B30` → `0x0102DF60`), kept on purpose and noted at the code site. So engine-side
  code of ours must not call SetState more often than the engine does: the preludes' card managers
  (`ui_prelude.lua` `UICardManager`'s `look`) and the battle cards' per-frame update
  (`battle_prelude.lua` `__ntw_battle_update_cards`) call it only when the state differs (how the
  engine itself marks selected cards is UNKNOWN). That matters: `BattleUnitCard`'s "Selected" /
  "Default" states have the enter functions `Selected` / `Unselected` (`ui_probe -- statefns
  ui/templates/`), which a per-frame SetState would re-run every frame. Tests:
  `a_state_function_that_re_enters_its_own_state_runs_until_luas_call_limit`,
  `an_endless_init_state_ping_pong_ends_with_luas_error_not_a_crash`,
  `a_bounded_state_ping_pong_runs_to_completion` (A's InitState runs twice). No front-end, battle or
  campaign test on the install hits Lua's limit.
- **`CurrentState()` inside a click handler is the old state**, CONFIRMED: the script method
  (`0x01014300`, now `PushUIComponentCurrentStateName`) reads the component's live current state, and
  `0x0102E340` fires the event before `0x01035620` sets the new one. The shipped checkboxes rely on it:
  `template.checkbox.lua`'s `NotifySelected` (bound to `OnMouseLClickUp`) reports "selected" when the
  box is in "down" at click time. Test: front end `option_checkboxes_report_the_state_their_click_leads_to`.
- **Destroy is deferred and keeps the scripts**, CONFIRMED: `Component.Destroy` (`0x01017F10`, now
  `DestroyUIComponentFromScript`) calls `0x01037230` (`ScheduleUIComponentDestroy`), which detaches the
  component and appends it to an end-of-frame queue without touching its script environment; the
  liveness test after an exit function (`0x010587B0`, `HasLiveUIScriptEnvironment`) checks that
  environment. So a close button whose exit function closed its own panel still gets the new state's
  InitState and enter function, in the original and in ours (`panels_close_from_their_close_buttons`
  counts it on the real `template.button_close` → `OnLeaveDepress` → `ClosePopup` → `Destroy` path).
- **Every host runs state functions** (front end, battle, campaign), CONFIRMED: the exe has one UI
  component class and these functions test no mode. Shipped users (`ui_probe -- statefns`): the
  options sliders' end buttons (`UpdateSlider` exit functions), the grand campaign / load game cards
  (`Selected` / `Deselected` enter functions, via SetState), the battle order buttons
  (`Rotate_Left_Down` / `Rotate_Left_Up` etc.), the close / minimise buttons of `unit_information`,
  `advice_interface` and the multiplayer chat / player list (`OnLeaveDepress`). Tests: front end
  `option_slider_buttons_step_through_their_exit_function`, battle
  `turn_left_order_button_works_through_its_state_functions`.
- **No double close.** The exe runs both a bound click event and the state's exit function when both
  exist (event, then transition, CONFIRMED), but no shipped layout binds an event to a function that
  is also one of the component's state enter / exit functions (`statefns` lists none): diplomacy's
  `button_close` binds `OnMouseLClickUp -> OnLeaveDepress` and its "depress" state has no exit
  function. `panels_close_from_their_close_buttons` now counts: one `OnLeaveDepress` and one root
  `ClosePopup` per close.
- **InitState's argument and result** (`0x0102B480`, CONFIRMED from the code): the argument is
  `{State = name, Text = {Text, HAlign, VAlign, DisplayWidth, DisplayHeight, TextXOffset, TextYOffset},
  Images = {[i] = image metrics}}` (text measured first if it was not). The engine reads back only the
  function's **first return value**: its `Text.TextXOffset` / `TextYOffset` become the state's text
  offsets (text re-placed), and each `Images[i]` is applied to image i; if any image's X or Y changed,
  the state is refitted. OPEN (not changed here): ours sends only `State`, `Text.DisplayWidth/Height`
  and `Images`, reads the images back from the argument table even when nothing is returned, and
  ignores returned text offsets.
- Ghidra (round 5, saved): named, typed and plate-documented `DispatchUIComponentLeftClickDown`
  `0x0102E1F0`, `DispatchUIComponentLeftDoubleClick` `0x0102E4B0`, `DispatchUIComponentMiddleClickUp`
  `0x0102E730`, `DispatchUIComponentMouseOn` `0x0102EBC0`, `DispatchUIComponentMouseOff` `0x0102EB40`,
  `FindUIStateTransitionTarget` `0x0102D970`, `IsUIComponentHoveredOrNonBlocking` `0x0102C580` (the
  pointer is on it, or it does not block: +0x80 == 0), `FindUIEventIndexByName` `0x01038580`; globals
  `g_szUIEventNameTable` `0x01464170`, `g_flUIPressPosX/Y` `0x01765F48/4C`. The earlier renamed
  functions (`ApplyUIComponentStateTransition`, `CallUIStateExitFunction`,
  `CallUIStateInitAndEnterFunction`, `ApplyNamedUIComponentStateFromScript`, ...) already had plates
  (completeness 85-100).
- Still open: the user's `OnMouseLClickUp of component 1176: error converting Lua boolean to String`
  did not reproduce in the tests or the game harness. The technology panel logs missing `eu_*`
  building icons (`UI texture not found`), a separate issue.

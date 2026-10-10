# Campaign UI: the original HUD's engine side (selection, review panel, labels, panels)

Worker: campaign-ui (branch `work/campaign-ui`). Tags: CONFIRMED / INFERRED / UNKNOWN; our stand-ins
PLACEHOLDER / PROVISIONAL. Decompiled code is never stored; Ghidra findings are written here as specs.

Code:
- `crates/ntw_script/src/ui/campaign/`: the `CampaignUI.*` functions (one module per screen: agents, army,
  characters, diplomacy, government, map, settlement, tabs, technology) and the engine's calls into the HUD's
  Lua (selection, review-panel tabs, funds). `campaign_prelude.lua`: Lua glue (global `Component`, VFS
  `loadfile`, card-group start-up).
- `crates/ntw_formats/src/ui_templates.rs`: the UIEd template library `data/UI/Templates/uied.templates`.
- `crates/napoleon/src/campaign/hud.rs`: Bevy side (model link, requests, harness, sounds).
- Headless probe: `cargo run -p ntw_script --example campaign_hud_probe -- [--tree] char:0 region:eur_france`.
- Template library probe: `cargo run -p ntw_formats --example uied_probe [-- <name>]`.
- Loc search: `cargo run -p ntw_formats --example loc_probe -- key <substring>`.

## Where I am / what's next
- **Done (all from the original layouts and scripts, checked with `--screenshot` and `tests/campaign_ui.rs`):**
  - the start-up script errors (`message_handler.lua:64 stack_base`: `CampaignUI.ScreenSize` was a stub;
    `campaign_hud.lua:98 player_details`: `CampaignUI.FactionDetails` was a stub) and the theatre map errors;
  - the "ygT" text: the selection bar's layout sample text. The engine's `ClearHud` at start clears it and
    `SetSelectedEntity(entity, "a, b")` fills it ("Paris, France");
  - army selection: the original review panel with an "Army" tab (template from `uied.templates`) and unit cards
    (`CampaignUnitCard`, card pictures `ui/units/icons/<faction>_<unit>_icon.tga`, men counts, army buttons);
  - settlement selection: "Construction" tab (BuildingFrame slots with the standing buildings and their upgrade
    options, costs, turns; clicking one queues `ConstructBuilding`) and "Recruitment" tab (RecruitmentCard options and
    the queue; clicking queues `Recruit`, a queued card `CancelRecruitment`);
  - map labels: the original Labels.lua + `city_info_bar` ("Settlement, Region" in the owner's colours) for every
    settlement on screen;
  - tooltips: the HUD components' tooltip texts shown through the original `Tooltip` template (frame drawing PROVISIONAL);
    building tooltips through `BuildingDetails`;
  - panels: the Lists button opens the original entity lists (armies, navies, regions, agents) with model data;
  - HUD clicks send `audio::UiSound { component }` like the front end.
- **Next:** the government (finance) screen needs ministers / the faction leader (not in the model:
  `InitialiseGovernmentDetails` gives the summary fields only); technology (`TechnologyPlayerDetails`), building browser
  (`BuildingBrowserDetails`), missions (`MissionsDetails`), diplomacy panels; list clipping/scrolling in the renderer
  (lists overflow their frame); tooltip frame pieces (edges do not follow the tooltip's size); unit card
  experience; character names in the selection bar and lists; infrastructure/agents tabs; settlement-under-pointer
  bottom row.
- **Harness:** `--campaign-select-region <key>`, `--campaign-ui-click id,...` (`hover:<id>` rests the pointer for a
  tooltip; `selectfort:<region key>` selects that region's fort), plus the old `--campaign-demo`, `--campaign-demo-move`, `--campaign-end-turn N`.

## Engine → HUD calls (the engine calls these root-layout globals)
| Call | Evidence | Tag |
|---|---|---|
| `UpdateFactionFundsAndDate{funds, season, year, round_description}` | layout.root bytecode | CONFIRMED |
| `SetSelectedEntity(entity, "a, b")`: two names joined by `", "` (`DAT_01315FF0`); the script cuts at the comma if the text does not fit one line | `FUN_009C3FB0`/`FUN_009C37A0`/`FUN_009C33E0` call sites; root bytecode | CONFIRMED shape; the two names for a settlement INFERRED settlement + region; for a character PROVISIONAL "<agent type>, <faction>" (names not in the model yet) |
| `ClearHud()` = `ClearReviewPanel()` + `ClearSelectedEntity()` | exe string `ClearHud` 0x0136B3A8; root bytecode | CONFIRMED string, call on deselect / start INFERRED (`ClearSelectedEntity` is not in the exe) |
| `ClearReviewPanelTabs()`, `CreateReviewPanelTabAtPosition(title, id, index, 1)`, `ReviewPanelTabInit(title, index, state)` | `FUN_0099A0B0` (4 args: title, id, index, 1) | CONFIRMED |
| `Generate{Army,Navy,Recruitment,Construction,Agents}Panel(info)` for the selected tab | tab vtables `FUN_009C7710`, `FUN_009C7C70`, `FUN_009C7CF0` | CONFIRMED names |
| Card groups: `Initialise(manager_module, multiple)` on UnitCardGroup/AgentCardGroup/... at start | no script calls it; CardGroup.lua forwards `SelectionChanged` to `manager.SelectionChanged` | INFERRED; modules and the multiple flag PROVISIONAL |

### Review-panel tabs (CONFIRMED from the exe)
- Tab key = component id = `random_localisation_strings` key: `FUN_00F555C0(i)` / `FUN_00A0CB30(i)` with enum
  0x4B..0x52. The eight .rdata strings in order: `army_tab, navy_tab, recruitment_tab, naval_recruitment_tab,
  agents_tab, infrastructure_tab, siege_tab, construction_tab` (enum CONFIRMED 2026-10-07: the static
  initialiser `0x00434AF0` writes the table `0x0164DB58 + i*8` with 0x4B = `army_tab` (0x013CC6F4), 0x4C = `navy_tab`,
  0x4D = `recruitment_tab`, 0x4E = `naval_recruitment_tab`, 0x4F `agents_tab`, 0x50 `infrastructure_tab`, 0x51 `siege_tab`,
  0x52 `construction_tab`).
- A selected army / navy (the map entity, `0x009C2AF0`): army → `0x009855B0`: `army_tab`, `recruitment_tab` when
  `0x009D1CB0(commander)`, `agents_tab` when the force carries an agent (`0x008B77C0`); navy → `0x009990B0`: `navy_tab`,
  `army_tab` (troops aboard), `naval_recruitment_tab` when `0x009D1CB0`, `agents_tab`. `0x009D1CB0` = the commander's
  `agent_culture_details` row for his culture names a unit (`0x009CBC40`, agent record `+0x1AC` hash → row `+0x2C`; only
  `General` rows have one) or he is an admiral (agent record `+0x2C` == 1). Each tab's index is the count so far + 1, and
  `CreateReviewPanelTabAtPosition` (`layout.root.lua:600-623`) puts tab i at (i-1) × (tab width − overlap): Army left of
  Recruitment, with no test of where the army stands (user side-by-side 2026-10-07 agrees). CONFIRMED: `0x009D1CB0` + user check
  2026-10-07 (Henry Fox, colonel: Army tab only); admirals: `0x009D1CB0` + user statement.
  Ours: `tabs_for` / `commander_recruits`; the army's recruitment panel and the navy's naval recruitment panel are the
  commander panel (`0x009FE7B0`'s character path: the faction's regions as sources, training and march turns, items
  queued for him; UI_FIDELITY.md §4.10). No agents or troops aboard in the model, so those tabs never appear.
- A selected agent (`0x009C3FB0` → `FUN_00985F40`): construction (in a settlement), naval recruitment (port), navy,
  recruitment, army, agents.
  Settlement panel (`FUN_0099A200`): construction, recruitment (if the region can recruit), infrastructure (if a fort/port),
  army (garrison), agents. The tab named by the caller (previous tab) starts selected, else the first (INFERRED).
- Tab states (`Utilities.lua`, CONFIRMED): `RPTS_GREYED 0, RPTS_NORMAL 1, RPTS_SELECTED 2, RPTS_HIGHLIGHTED 3`.
- Clicking a tab: `SelectTab(i)` → `CampaignUI.ReviewPanelTabSelectionSet_1_Indexed(i)`; the engine regenerates the panel inside that
  call: `ClearReviewPanel`, `ReviewPanelTabInit(.., old, 1)`, `ReviewPanelTabInit(.., i, 2)`, generator (CONFIRMED `0x00A20620`, UI_FIDELITY 10.1).

## CampaignUI functions (implemented)
| Function | Returns | Tag |
|---|---|---|
| `ScreenSize()` | width, height, each at least 1280 x 960 | CONFIRMED `0x009F4B90` |
| `FactionDetails(key)` | {Key, Address, Name, FlagPath, UniformColour, PrimaryColour, WealthRanking, PowerRanking, PrestigeRanking, Leader, VictoryConditions} | names CONFIRMED `0x009E2CB0`/`0x009AF1A0`; rankings "", colour sub-keys r/g/b, empty VictoryConditions PROVISIONAL; Leader = the leader's character details table (`0x009AD250`, no extra keys) |
| `PlayerFactionId`, `PlayersFactionKey`, `CampaignKey`, `CurrentTurn`, `CurrentYear`, `IsPlayersTurn`, `IsMultiplayer` (false), `CanEndTurn`, `EndTurn` | | descriptions CONFIRMED (bindings table) |
| `LocalisationString(k)` | loc `random_localisation_strings_string_<k>` | CONFIRMED |
| `Time`, `WindowsTime` | `Time`: the pulse clock (real time since the HUD started, floored to whole ms by `pulse`) × 0.001 as a 32-bit float; `WindowsTime`: whole seconds of `timeGetTime()` (since Windows started) (see "UI clock" below) | CONFIRMED formulas; `Time`'s clock source PROVISIONAL |
| `ReviewPanelInfo()` | current army/navy info (Army.lua soft refresh) | INFERRED |
| `RegionFromSelection()` | selected settlement's region | INFERRED |

Army/navy info: {controlable, commander, military_force, can_build_fort_status, build_fort_cost,
units_info = {Units|Ships = {entry...}}} (CONFIRMED names, strings 0x0136D630..0x0136D6D0 and Army.lua).
Unit entry fields read by `template.CampaignUnitCard.lua` (CONFIRMED names): Id, Address, Key, UnitRecord, Name,
Description, Icon, Men, Max, MenAsPercent, EstimatedMenAsUnary, Experience, IsNaval, Guns, InTransit,
ReplenishmentLevel, Replenished, SufferingAttrition, CommanderType, CharacterPtr, CommandersName,
DisplayAsUnit, Attributes{PrimaryAttributePath, PrimaryLevel, PrimaryAttributeName}, knowledge_mask,
spying_data_level, Portrait, hull_health_l/r.
- `CommanderType` values from Utilities.lua (CONFIRMED): 0 primary general, 1 secondary general, 2 primary admiral,
  3 secondary admiral, 4 commodore, 5 brigadier, 6 naval unit, 7 land unit. `SPYING_DATA_LEVEL_OWNED = 3`.
- Card picture: `ui/units/icons/<faction>_<unit>_icon.tga` (CONFIRMED names; the script appends ".tga").
- Portraits (CONFIRMED, own Ghidra copy 2026-10-07): the army panel's cards have `DisplayAsUnit` false
  (`BuildArmyPanelInfoTable` `0x009FCFB0` clears the card info byte `+0x130` that `0x009ABE00` hands out), a
  character's `CommandedUnit` true (`0x009AD250` sets it at `0x009ADF80`). `Portrait` = "data/" + the unit's
  character's portrait path when his agent type is 0, the General (`0x008C9EF0`, `0x00F9C6C0`: agent record
  `+0x2C` == 0), else empty; the card shows it when DisplayAsUnit is false, CommanderType is a general/admiral and
  `string.len(Portrait) > 0`, else its `Icon` (bytecode, `luac_dump ui/templates/template.CampaignUnitCard.luac --proto 56`,
  pc 145-180, CONFIRMED 2026-10-08), so an admiral's card always shows its unit picture
  (`template.CampaignUnitCard.lua:66-99`). The path is INFERRED to be the PORTRAIT_DETAILS card picture (the exe
  reads character `+0x370`). Lists rows are the commanders' character details tables (`0x009AD250`):
  ShowAsCharacter = no commanded unit, or a General or admiral (`0x00F9C6C0` / `0x00F9C690`); else CommandedUnit
  is the unit card; Soldiers = the force's soldier count. So generals show their portrait in the army bar and
  the Lists, colonels their unit picture + star + count (user side by side 2026-10-07 agrees).
- PROVISIONAL: experience 0; attribute icon of his type's main attribute (`agent_attributes`, CHARACTERS_FIDELITY §13), level 0;
  the first unit of a commanded force is the commander's card.

## Settlement panels (findings)
| Rule | Evidence | Tag |
|---|---|---|
| Settlement tabs in order: construction, recruitment (region can recruit), infrastructure (fort/port), army (garrison), agents | `FUN_0099A200` | CONFIRMED order; conditions INFERRED; we list construction, recruitment, army |
| `GenerateConstructionPanel(info)`: `{slots = {{buildings = {b}, upgrades = {o...}}...}, controlable, faction_key, infrastructure, fort_ptr}` | Construction.lua | CONFIRMED names |
| Building entry `type`: Utilities.lua `BUILDING_ICON_TYPE_*` 0 empty, 1 built, 2 constructing, 3 constructable, 4 upgrade, 5 alt chain; `availability` bit 1 affordable, bit 2 technology present | Utilities.lua, Construction.lua locals | CONFIRMED values |
| Build commands: `BeginConstruction(building_key, slot_key)`, `BeginUpgrade(...)`, `CancelConstruction(...)` (slot_key = `REGION_SLOT` key) | template.BuildingFrame.lua | CONFIRMED calls; CancelConstruction not implemented (no model command) |
| Building pictures and names: `building_culture_variants` (schema s,s,o,o,o,o,o: level, culture, model, icon, ?, description key, icon) → `ui/buildings/icons/<icon>.tga`, loc `building_culture_variants_name_<level><culture>`, `building_description_texts_*_<key>`; culture = `factions.culture_variant` | table read to the end; file and loc names | CONFIRMED; empty-slot picture PROVISIONAL (placeholder icon) |
| `GenerateRecruitmentPanel(info)`: `{recruitable_units, enqueued_units, faction_colour, uniform_colour, recruitment_capacity, player_owned}`; entries carry item_ptr, manager, record, unit_record, status (Available/Unavailable/Enqueued), reasons_unavailable bits (card's list order), name, image_path, cost, turns, card_id (`<unit>!recruitable!<n>` / `!enqueued!`), category, class, slot | Recruitment.lua, template.RecruitmentCard.lua, exe strings | names CONFIRMED; reasons only "no slot"/"unaffordable" PROVISIONAL |
| `RecruitUnit(character, manager, record)`, `CancelRecruitment(item_ptr)` | template.RecruitmentCard.lua | CONFIRMED calls |
| `CreateComponentFromTemplate`'s 6th argument `{"{<id>:<n>}<path>"}` replaces image n (1-based) of component <id> | Recruitment.lua | shape CONFIRMED, 1-based INFERRED |
| Slot hover (BuildingFrame OnMouseOn → SelectPassive, no delay) shows the slot's cards with a transition: the leading card slides down from under the slot over `g_time_down` 0.15 s; the others are shown at start + 0.8 × 0.15 × `g_time_scalar` (1) = 0.12 s and slide sideways over `g_time_out` 0.25 s (fade-in 0.2 s, fade-out 0.15 s constants). Start times come from `CampaignUI.Time()` (s); OnUpdate gets `OnUpdatePulse`'s ms and multiplies by 0.001 | template.buildingframe.luac (`luac_dump`), protos at lines 77, 181, 329, 341, 352 | CONFIRMED |
| UI clock: the root's per-frame update `0x0102F4F0` (UI root vtable +0x30) takes a u32 time, stores it in `0x0176591C`, calls each registered listener's vtable +0x30 with it and fires the root's `OnUpdatePulse` (event 11, handler at component +0x1E4/+0x1EC) with it as a float. `CampaignUI.Time` = `0x009F9B80` → vtable +0x30 of the campaign UI manager's `+0x18` part = `0x00A0EB80`: `(float)(u32)[[this+0x9E4]+0x90] * 0.001` (a ms counter of the object at manager `+0x9FC`). The campaign UI frame (`0x009D5F08`..`0x009D5F31`) calls that same getter, × 1000, truncates to int and passes it via `0x00DB2AE0` (manager `+0xAC` = UI root, vtable +0x30) to `0x0102F4F0`: `OnUpdatePulse(ms)` and `Time()` are one clock. `WindowsTime` = `0x009FB0B0`: `(int)((float)timeGetTime() * 0.001f)`: the u32 to a 32-bit float, times `0.001f`, truncated to an int (whole seconds). Ours: `OnUpdatePulse` and `Time()` share one clock: the HUD's ms clock (`CampaignHud::clock_ms`, fractional), advanced by real time (`Time<Real>`, so a battle's pause or speed does not carry over; no per-frame cap; PROVISIONAL) and floored to whole ms by `UiScriptHost::pulse` for both; `WindowsTime()` calls `timeGetTime()` itself (winmm) and computes as the exe, float then truncate (`host::windows_time_secs`; the battle binding `0x005D4BF0` is the same float without the truncation, see BATTLE_FLOW.md). (Before: `Time()` was a separate wall clock and delayed the drop-down by 0.4 s at start, growing ~0.3-0.5 s per end turn, bug 2026-10-07.) | exe | formulas and the shared clock CONFIRMED; what advances the `+0x90` counter (pause, turn processing) UNKNOWN |
| Technology tree links: an entry's `ParentXoffset` / `ParentYoffset` (`0x009ABB50`: record `+0x88` / `+0x84`; ChainPosition `+0x18`, PointsRequired `+0x1C`) are computed at load by the technology link step `0x00F21A30`: per `technology_required_technology_junctions` row (T requires R, table order, a later row overwrites) with R in T's tree column (records `+0x90`/`+0x94` equal, from the building chain through the building level): X = R.pos % 4 − T.pos % 4, Y = (2·T.level + (T.pos > 3)) − (2·R.level + (R.pos > 3)); other requirements go to the record's list `+0x68..` and draw nothing. `template.tech_entry.lua:47-94` draws a horizontal link (icon width + x spacing) × \|X\| and a vertical one (icon height + y spacing) × \|Y\| from the slot's centre. Ours: `technology_parent_offsets` (was columns 6/7: a 640 px stray line over the title, bug 2026-10-07) | exe | formula CONFIRMED; "same column" = same chain and the level field = `building_levels.level` INFERRED |

## Labels, tooltips, lists (findings)
| Rule | Evidence | Tag |
|---|---|---|
| Labels.lua (root pulse) calls `RetrieveVisibleEnitityDetails()` → `{Settlements = {{Address, ScreenPos{X,Y}}}, Resources}` when `CameraPosition()` changes, and creates `CreateFromLayout("data/ui/Campaign UI/city_info_bar", "label"..n, Address, X, Y)` | Labels.lua | CONFIRMED |
| `CampaignSettlement(address)`: `Settlement()`, `LabelDetails()` → {Name, IsCapital, FactionRGB{R,G,B}, PopulationGrowthString, ShowBottomRow, ScreenPos, Region{Name, Region (address), Wealth, WealthChange, PopulationChange, ReligionKey}}, `Release()`; label text "Name, Region.Name". The changes are 0-based indices into the template's `change_rates`; the bottom row also calls `RegionsPublicOrders(Region.Region)` → upper, lower | template.city_info_bar.lua (lines 110-135, `luac_dump --line 125`) | CONFIRMED names and placement; IsCapital/changes PROVISIONAL |
| A positioned layout's top component runs `ui/templates/template.<id>.lua` (city_info_bar) | exe string "template." | INFERRED |
| Global `Address` and `Component` exist outside component environments (modules use them) | Labels.lua, Recruitment.lua | INFERRED (we bind them to the HUD root) |
| `GetStateText()` → text, width, height; `InitState(t)` gets `t.Images` {X, Y, Width, Height} and may return `t` with changes the engine applies | city_info_bar, template.tab.lua | CONFIRMED usage; front-end hosts keep the old one-value GetStateText (PROVISIONAL) |
| Tooltips: the engine calls the root's `SetTooltipText(component, text)`; the root creates the `Tooltip` template once and registers it (`Component.RegisterTooltipObject`); `Component.CursorPosition()` → x, y, screen w, h; `Cursor():DistanceToBL()` → cursor-image offset | root and Utilities.lua bytecode | calls CONFIRMED; who shows/hides INFERRED; DistanceToBL (0, 32) PROVISIONAL |
| Lists: `RetrieveFactionMilitaryForceLists(faction, armies)`, `RetrieveFactionRegionList(faction)`, `RetrieveFactionAgentsList(faction)` with the row templates' fields; `CampaignCharacter(address)` handle | entity_lists.lua, row templates | names CONFIRMED; names/locations PROVISIONAL |
| Card groups use `UICardManager`; selected card state "Selected", others "Default", inactive "Inactive" | CardGroup.lua, CampaignUnitCard states | INFERRED |
| Draw order of the labels: the UI draws a component's children first to last (`0x01027D20` copies the child list `+0x8C`/`+0x90` in order, `0x0101E0D0`). `CreateFromLayout` (`0x01016BD0`), `CreateComponentFromTemplate` (`0x010171A0`), `CreateFromComponent` (`0x01017850`) and `Adopt` (`0x01024F70`, which appends) call the parent's virtual `+0x38` (`0x0102DCD0`), which fires its event slot 17 (`+0x22C`, `OnAdoptChild`; event slots are 0xC bytes from `+0x160`, slot 18 `+0x238` is `OnDivorceChild`) with the new child. The campaign root's `OnAdoptChild` (`layout.root.lua:563`, the battle root's `root.lua:310` is the same) collects its children, `table.sort`s them by `Priority()` (then by x position) and calls `ReorderChildren(list)`. `Priority` is the component's `+0xE0`, an `:int32` (`0x0103A040`) compared signed: `city_info_bar` is -1, the HUD 0, `diplomatic_relations` 47, so the labels go under every panel. A component created under a parent that has a parent takes the parent's priority when higher (`0x0101E270` at `0x0101F1A6`, copy `0x0101FC20` at `0x0101FDBC`); the root's children keep theirs. `ReorderChildren(list)` (`0x01014850` → `0x01031ED0`): entries are addresses or (when `lua_isnumber`) 0-based child indices; the new order is the list then the unlisted children in order, applied only if the count equals the child count; answers that bool. ORIGINAL BUG: the index is not bounds-checked (reads past the child array); ours treats it as no child (nothing reordered, logged once). Labels.lua's `child_sorter` is never called (no script references it, no exe string). Ours: `host::adopted`, `UiWorld::inherited_priority` / `reorder_children`; before, Priority was read unsigned (4294967295), OnAdoptChild never fired and ReorderChildren took (child, index), so the labels drew over the panels (user screenshot 2026-10-09) | exe, `luac_dump` of layout.root.luac, labels.luac, real layouts | CONFIRMED |
| Region details panel (`region_info`, layout `region_details`): the root's `ShowRegionInfo(region)` opens it with `InitialiseFromDetails` and `CampaignUI.InitialiseRegionInfoDetails(region)` (handler `0x009E69B0`, registered at `0x00429075`); `region_details.lua:51` sets the title `region_name` to `details.Name` (the layout's own text is the placeholder " XXX Details"). The handler builds the region info table `0x009AF570` (also used by the negotiation offers `0x009B4B3D`): Address, Settlement (settlement virtual `+0x30`), Name (`0x00A8D5D0` = region `+0x29C`), Theatre, OwningFactionKey, PopulationNumber, Population (`"%d"`, `0x009FB8F0`), PopulationChange, UpperOrder, LowerOrder, Wealth (`+0xCC` + `+0xBC`), WealthChange (`+0xC4`), UpperTax, LowerTax (floats, fractions: the script shows `Wealth * (UpperTax + LowerTax)` and `%.1f%%` of the sum × 100), ReligionKey, ReligionIcon, Taxed, ActiveUpperClass, ActiveLowerClass, with a governor the tax, building, governor, technology and administration-cost percentages, NextTownName and a turns count; then adds Governor (`BuildCharacterDetailsInfoTable`), Effects, pip tables for UpperOrder / LowerOrder / PopulationGrowth / RegionWealth / TownWealth (Pip, Tooltip, PredictedTooltip, EqualPipPredictedTooltip, LostTooltip) and ReligiousBreakdown. With no argument it takes the HUD's current region. Both build the one-round projection `0x00A727D0` first (CAMPAIGN_FIDELITY.md §Population): PopulationChange is its trend (1 up, 2 unchanged, 3 down), the pip tables compare now with the projection (each factor's Predicted = |predicted| − |now|, `0x009E7064`; PopulationGrowth in percent with `%f` = six decimals with up to five trailing zeros dropped, `0x004F16F0`), ReligiousBreakdown's Change is the projected share's change × 100, TownWealth lists the `town_wealth_growth_factors` slots of `0x00A6AFC0`'s breakdown now and predicted (`0x00A995A0`), WealthChange is the predicted growth's trend (`0x00AB4410`), RegionWealth has no factors (no `region_wealth_factors` table ships). Name: region +0x29C is the text of REGION #41 `CAMPAIGN_LOCALISATION` (reader `0x00A45950` → `0x00872420` into +0x290 / +0x29C), `regions_onscreen_<region key>` in every vanilla save. Ours (`ui/campaign/region_info.rs`): all of it from `economy` and `population`; left: the region's Effects (region +0x1CC, writer not traced), NextTownName / TurnsUntilNextTown (towns emerging on the map are not modelled), Theatre, the projection's garrison adjustment for an army selected to move (`0x00A190A0`) | exe, `luac_dump` of region_details.luac and layout.root.luac, the vanilla saves | CONFIRMED; Effects, next town, Theatre and the selection garrison PROVISIONAL |

## Panel placement (2026-10-07, user side by side o2/o3/o5/o6 at 1920x1080)
- CONFIRMED (scripts): `PanelManager.lua` gives each panel a `Side` (`entity_lists`, `missions`,
  `government_screens`, info popups: `g_right`; `technology`, `building_browser`, `region_info`: `g_left`;
  `agent_action`, `diplomacy_panel`, `event_message`...: `g_centre`); `OpenPanel` (`PanelManager.lua:275-281`)
  calls `SetDockingPoint(Side, g_centre)` and `huds.MoveRelativeToHUD(panel, DockingPoint(), {8, -8, 0}[Side])`.
  `Huds.lua:47-82`: x = 0 (left), `h_width` − w (right) or trunc((`s_width` − w)/2), plus the offset; y =
  trunc((`s_height` − `h_height` − h)/2). `RegisterHud(g_hud, true)` (`layout.root.lua:840`): `h_width`,
  `h_height` = `veneer_DY:Bounds()` (`0x010133C0`: the union of the component and its direct children,
  CONFIRMED), `h_width` at least 1280; `s_width`, `s_height` = the root's Dimensions. Ours runs exactly this.
- CONFIRMED by a debugger sitting in the original (2026-10-07, Coalition campaign as Britain, 1920x1080): at
  `RegisterHud` the root is 1280x960 and `veneer_DY` 1280x241 at (−1, 718) (`+0xD4` = 1, `+0xD5` = 0, dock 8), so
  `h_width` = `s_width` = 1280, `s_height` = 960; with the Lists open in play the root is STILL 1280x960 and the Lists
  panel (dock 6 after `SetDockingPoint`) is at (648, −1) for the scripts, shown at x ≈ 1288, y ≈ 59 (o2). So the
  scripts work in the HUD layout's fixed 1280x960 frame, and a top-level component is moved on screen by its dock
  anchor's share of the screen's growth (x + 1.0·(1920−1280), y + 0.5·(1080−960)); the HUD band (dock 8) is
  centred the same way (o1). The Enlist panel (dock 5, o5: 648, 59) fits the same rule. Ours: `UiWorld::script_rect`
  / `script_to_screen` (Position, Dimensions, Width, Height, Bounds and MoveTo in that frame, campaign and battle HUDs via
  `set_frame(UiFrame::ScriptFrame)`, the battle one CONFIRMED by the sitting of 2026-10-09 (BATTLE_FLOW.md §3); the
  front end keeps screen geometry, PROVISIONAL); test `the_lists_panel_docks_to_the_right_edge_of_a_wide_screen`.
  `Component.CursorPosition` (`0x01018590`, 2026-10-09): its screen width and height are the virtual screen, window /
  UI scale (the corner mapped back through `0x0119B270`), CONFIRMED, as ours; the cursor x, y are the point the UI
  mouse handler `0x0102E9D0` was given, whose frame is not traced (ours: the virtual screen, PROVISIONAL).

## Original reference shots (2026-10-07, Coalition campaign as Britain, 1920x1080, turns 1-4)
In `target/orig_shots/2026-10-07_britain/` (o1..o6); ours: `--campaign mp_eur_napoleon --campaign-faction britain`.
A final check only: each fix takes its values from layouts, `.twui`, UI `.luac` and the exe, never from these
pixels. Still open (the tabs, portraits, Lists docking and the technology title line were fixed in `53361ec`):
- o1 army bar: a promote star button at the panel's right.
- o3 character details: Subterfuge skill mask + stars, age, traits, followers. Agent selected: one HUD tab
  **Agents**; card = portrait, 3 small stars on the left edge, mask icon bottom-left; three round action buttons
  at the right, greyed with no target. Settlement selected: tabs **Construction | Recruitment | Infrastructure | Army**.
- o4 agent actions: right-clicking a target opens a centred **Agent Options** popup (x 686-1232, y 208-630):
  Sabotage (no %), Infiltrate 80%, Sabotage Army 75% (spy vs Caen, late Feb 1805). Sabotage opens a centred
  **Sabotage** panel (x 650-1270, y 62-776): picture, "Select Target", rows of building icon + "Building Level"
  dots + chance (66%) + bomb button. How the HUD buttons enable is still to trace.
- o5 the star button = **Enlist New General** (centred, x 650-1270, y 62-776): "Generals enlisted 3/6",
  distance-from-capital bar, 3 candidates (portrait, name, stars, traits, cost), OK/X. Its bottom is ~55 px above
  the HUD band.
- o6 Research and Technology: Educational Buildings (Oxford); Civil / Military / Industrial columns.

## UIEd template library (`uied.templates`)
- CONFIRMED: the exe builds `data/UI/templates/` + `uied.templates` (`FUN_00DA6A60`). The campaign HUD's
  `ReviewPanelTab` exists only there (no `ui/templates/reviewpaneltab` layout).
- Format (CONFIRMED by reading 124 of 126 entries to the byte): u32 count; per entry a 256-byte name block (newer
  entries: u32 1 and u32 layout version in its last 8 bytes), u32 payload size, u32 UNKNOWN, payload = u32 image count +
  images {u16 path, TGA file (length from its header)} + component tree; every child component is preceded by its own
  embedded images in the same framing. `InputWindow` and `BattleEditor` (editor-only) do not read (UNKNOWN old version).
- `CreateComponentFromTemplate(name, ...)`: the `ui/templates/<name>` layout file (its single child under the editor
  `root` wrapper) first, else the library (INFERRED order). Embedded images are given to the renderer by path.

## Questions for Ghidra
1. The uied.templates loader (inside `FUN_01022A50`): lookup order between template files and the library; the UNKNOWN u32.
2. The character name shown by `SetSelectedEntity` (vtable +0x30/+0x34 of the selected character).
3. Which module/flag the engine passes to each card group's `Initialise`.
4. Unit card fields built by `FUN_0099BAD0`/`FUN_0099BB70` (experience, replenishment, portraits).
5. The engine's tooltip display: hover delay, where the tooltip object is placed and hidden, and how the template
   edges follow its size.
6. The settlement label's screen anchor (offset from the settlement's map position), and whether `RetrieveVisibleEnitityDetails` leaves out settlements behind the HUD (draw order: done, see "Labels, tooltips, lists").
7. Which children the engine clips (list clip components) and how lists scroll.
8. The faction leader / ministers in the ESF, for the government screen.

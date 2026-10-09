# Front-end pages: navigation, Single Player pages, Options

Code: `crates/ntw_script/src/ui/{host.rs,frontend.rs,ui_prelude.lua}` (engine functions),
`crates/napoleon/src/frontend/` (Bevy side), `crates/ntw_formats/src/preferences.rs`.
Headless runner: `cargo run -p ntw_script --release --example ui_run -- [--tree] single_player load_game`
(clicks components by id; `--key ESCAPE` presses a key). Real-install tests: `cargo test -p ntw_script --test frontend_ui`.
Screenshots: `cargo run -p napoleon -- --ui-click single_player,napoleon_battles,NHB_Arcole --screenshot x.png`
(`key:ESCAPE` presses a key; the shot is taken 2 s after the last click).

Handler addresses: `analysis/worker1/script_bindings.tsv` (FrontEnd registrar `0x004587A0`). Decompiled code is not stored.
Tags: CONFIRMED (exe/script/data), INFERRED (deduced), PROVISIONAL/PLACEHOLDER (our stand-in), UNKNOWN.

## Navigation (page stack)
| Rule | Evidence | Tag |
|---|---|---|
| All page changes are root.lua's own `TransitionTo(name, is_back, ...)` / `TransitionBack()`: pushes the current page on `Previous`, divorces + hides it, `CreateFromLayout("data/ui/frontend ui/"..name, name, root)`, calls the new page's `OnEnter(...)`; Back re-adopts the previous page and destroys the left one on the next forward move | root.lua bytecode | CONFIRMED |
| ESCAPE: root.lua `StealShortcutKey("ESCAPE")`, `OnKey(key, _, released)` → `TransitionBack` when the 3rd argument is true. A key goes to the most recent stealer still shown | root.lua, options.lua sub-panels steal ESCAPE/RETURN | names CONFIRMED, routing INFERRED |
| `CreateFromLayout` returns the file root's first child (every page file is `root` + one child named like the page); the engine's own front-end layout load keeps its root (root.lua runs on it) | `0x01027400` takes the holder's first child as the template (BATTLE_FLOW.md §3 "Where CreateFromLayout puts a layout"); options.lua `Find(2)` = button_ok of the child | CONFIRMED |
| Script files of a component: override first (`ui/templates/<o>` or `<layout>_scripts/<o>`; `"null"` = none), then `<layout>_scripts/<id>.lua`, `<layout>.<id>.lua`, and for a layout's top component `<layout>.lua` | battle_icon.lua override, options' gamma slider override `null`, message_box.luac/credits.luac | names CONFIRMED, order INFERRED |
| Each frame is `OnUpdatePulse(time_ms)`; callbacks may be function names (`SetEventCallback("OnUpdatePulse","OnUpdate")`) | template.heading.lua (slides in over 750 ms) | INFERRED unit |
| `Cursor("busy")` during TransitionTo, `SetMode("normal")` after | root.lua | CONFIRMED; system wait cursor PROVISIONAL |
| `SetImageColour(i,r,g,b,a)` / `SetImageMetrics(i,x,y,w,h)` act on the current state's i-th image metric; an untextured image is drawn only once a script painted it | root.lua black backdrop | INFERRED |

Other engine rules found while running the pages (all in `UI_SCRIPTING.md` terms): `Parent(n)` = n-th ancestor, `Parent("id")` = outermost
ancestor with that id; an unknown `UIComponent` method calls the component's own Lua global (`list:Layout()`, `SelectedItem`); native
`Layout()` builds `{Address,Width,Height,X,Y}` items, calls the Lua `Layout(items)` hook and moves children to `item.Y`;
`SetStateText` returns width, height, lines; `InitState{State, Text={DisplayWidth,DisplayHeight}}`; `GetStateTextDetails().XOffset`;
`Component.Call("Parent.Height")`, `Component.GetProperty("Parent.Value")`, path segments `Parent`/`Root`/`Children[n]`/ids;
`Component.CreateFromComponent(src, id, parent, x, y, {?}, {child=text})`; `Component.Messages.X`; `Component.SequentialFind` from the
top root returning a component; `DestroyChildren` detaches at once and destroys at the end of the frame; properties are numbers when numeric,
else text; `UISelectionManager` calls each item's Lua `Select(bool)`; `string.length = string.len`. All INFERRED from the scripts' use.

## Single Player pages
| Function | Returns / does | Tag |
|---|---|---|
| `FileExtenstionAndPathForWriteClass(c)` | `.save`/`save_games\`, `.save_multiplayer`, `.replay`, `.battle_preferences`, `.army_setup` + the user folder. Ours: NapoleonRust's own user folder (where we write), else the original's | tables CONFIRMED `0x01395E80..`; our folder PROVISIONAL (the original names its own) |
| `EnumerateCampaignSaves(dir, "*ext")` | `{FileName, Path, Date, DateString, TimePlayed}`; DateString `dd/mm/yyyy hh:mm` (layout sample) in local time (Windows `FileTimeToLocalFileTime`); TimePlayed = header turn. For our save folder the original's saves are listed too (read only; ours win on equal names) | fields CONFIRMED (script); the merged list is ours (PROVISIONAL) |
| `sp_load_game.lua` (CONFIRMED by disassembly) | sorts newest first by `Date` and keeps 900 (`SortAndRemoveExcessiveFiles`); selects the newest; a row's name = FileName without the extension (`NameFromDetails`), kept as the row global `m_name`; Load (`OnAccept`) calls `LoadCampaign(m_name)`; Delete asks (`confirm_delete_*`) then `DirectoryUtils.DeleteFiles(paths)`; selecting shows `dy_date` = Year, `dy_season` = Season, the leader portrait and `<FlagPath>/portrait_flags.tga` on `card_window`, and `Maps[theatre]` on `map_nap_italy/egypt/europe/spain` (theatres italy_main, egypt_main, europe_main, spain_main) | CONFIRMED |
| `GetExtendedSaveGameInfo(path)` | `{Faction, FlagPath, LeaderPortrait, TimePlayed, Year, Season, Maps}` from `SAVE_GAME_HEADER` (`ntw_campaign::read_info`, read only); `Maps[theatre]` = the header's `MAPS` item picture (theatre key, width, height, pitch, 0xAARRGGBB pixels), given to the page as a run-time image | fields CONFIRMED `0x008982C0`; MAPS layout CONFIRMED in every save; pixel order INFERRED |
| `EnumerateNapoleonsCampaigns(mp)` | tut/ita/egy/eur (mp: [spa], mp_ita/egy/eur) with `{Key, Name, Description, StartDate, BulletList, Unlocked}`; loc `campaigns_onscreen_name_*`/`campaigns_description_*`; StartDate = startpos year | list+fields CONFIRMED `0x0046E1A0`; sources INFERRED; BulletList UNKNOWN |
| Unlock rule | tut/ita always; egy `nap_unlock>1`, eur `>2`, NCAMP_Waterloo `>3`; `nap_unlock` = HKCU `...\Napoleon\nap_unlock`, default 1, clamp 1..4 (we read it with `reg query`) | CONFIRMED `0x0047C820`, `0x0047B750` |
| `GetWaterlooBattleDetails()` | name, description, spec file, unlocked (record NCAMP_Waterloo) | CONFIRMED |
| `StartCampaign(key[, faction])` | default factions ita→ita_french_republic, egy→egy_french_republic, tut→tut_france, eur/spa→france; → `CampaignStart` + `GameMode::Campaign` | CONFIRMED `0x00478840`; faction not passed yet PROVISIONAL |
| `CampaignDetails(key)` | `{Key, Name, Description, StartYear, Factions[key]={Key,Name,Description,LeaderPortrait,FlagPath,VictoryConditions,StartingRegions,GameTypes,ListOrder}}` from startpos playable factions + factions DB flag folder + loc | names CONFIRMED `0x0046BCC0` + main_panel.lua; descriptions/VCs/StartingRegions UNKNOWN (empty); GameTypes = short + "Historical" PROVISIONAL |
| `TheatreList`, `GenerateRegionOwnershipMaps` | theatre key from the map (nap_europe→europe_main); no images (the load page uses the save header's `MAPS` pictures instead) | PLACEHOLDER |
| `battles` DB table | 13 columns `s,s,b,s,o,i,i,b,b,b,b,o,i` (key, type, naval, spec, screenshot, w, h, 4 flags, movie, year) | schema INFERRED (all 87 rows parse) |
| `EnumerateNapoleonsBattleMaps()` | type `napoleon_historic`, entry fields `Key, File, Name, Description, Image, IsNaval, Map, IsHistoric, IsSiege, Movie, Teams` + `UnlockLevel` (7 = DLC locked, else stored progress + 2) | CONFIRMED `0x0046DFF0`, `0x0045B960`; we give 2 (all unlocked, the retail "all maps" flag at +0x5C INFERRED set) and 7 when the battle file is missing (PROVISIONAL) |
| `EnumerateBattleMaps(type, flag)` | same entries; filter record flag 1/2 | CONFIRMED `0x0046D7C0` |
| `UIHistoricBattleSetup(prefs, info)` / `BuildInfoSetup` / `StartBattle(setup:Address(), players, ...)` | RetrieveDetails() = `{Alliances={{ {Faction,Name} }}}` from the battle .xml; StartBattle → `UiRequest::StartBattle{battle, map}` → `terrain::load_battle_map` (the `--battle-map` preset from the xml's `<name>BattleTerrain/presets/X/</name>`) + `GameMode::Battle` | call shapes CONFIRMED; objects PROVISIONAL; the battle starts with the file's armies (`battle::BattleStart`, see `analysis/battle/BATTLE_FLOW.md`) |
| `LoadCampaign(name)` | the page passes the save name without folder or extension; the exe completes it with the save class folder and extension (`0x00471520` → `0x0047CC70`, the file system object's path builder, class 0) and starts loading. Ours: the path the last listing gave that name (ours or the original's), else ours/original's `<name>.save`; a full path is accepted → `UiRequest::LoadCampaign(path)` → `CampaignStart { save }` → the campaign loads the save | CONFIRMED (name form, folder completion) |
| `ContinueCampaign()` | the newest file of the class that reads as a save (`0x0046CF00` → `0x0085F7D0`, enumerates `*<ext>`, keeps the latest time; callback `0x0085F990`); ours: both folders | CONFIRMED; validity test `0x00A0BA70` INFERRED as "reads as a save" |
| `DirectoryUtils.DeleteFiles(paths)` | deletes only files directly in NapoleonRust's own save folder; anything else (the original's saves) is refused and logged | ours (the original deletes in its folder) |
| `PlayMovieInComponent`, `ShowDemoMovie` | no-op (no Bink) | PLACEHOLDER |
| `LocalisationString(k)` | loc `random_localisation_strings_string_<k>` | CONFIRMED description + data |

## Options
- `UIPrefsInterface(core, mp, dropin)`: CONFIRMED class/method table at `0x01459B10` (CurrentOptions, Set{Game,Audio,Graphics,UI,Gamma,Control,VoiceChat}Options,
  AvailableGfxOptions, AvailableGfxQualities, EnumerateScreenModes, ...). Section field names CONFIRMED from Set*Options (`CPU_moves, city_management,
  limitless_ammo, drop_in_battles, battle_time_limit, battle_difficulty, autoresolve_difficulty, campaign_advice, battle_advice, campaign_difficulty`;
  UI `minimised_ui, selection, paths, target_zones, orders, cards, radar, land_ids, naval_ids`; audio `master, music, speech, effects, mute_*, sound_provider,
  channels, variation, memory, caching, subtitles, voicechat`; gamma `gamma, brightness`). The pairing with preferences keys is INFERRED (`ui_prelude.lua` table);
  difficulty slider 0..3 = stored 1..-2 (INFERRED).
- File: `preferences.script.txt`, UTF-16LE + BOM, CRLF, `key value; # help #` (CONFIRMED from the player's file). `ntw_formats::preferences` keeps every line.
- Our copy: `%APPDATA%\NapoleonRust\scripts\preferences.script.txt` (or `$NAPOLEONRUST_USER_DIR`), seeded once from the original's file (read only).
  Written on every Set*Options; the original's file is never written.
- PROVISIONAL: graphics dropdown option lists and screen modes (only the preferences resolution), quality presets (all available, "custom"),
  key sets (empty), sound providers.

## Tooltips (2026-10-04, save-compat worker on 0-E)
- **Engine side, ours (INFERRED):** when the pointer comes to rest on a component, its tooltip text is
  passed to the root layout's `SetTooltipText(component, text)`; an empty text hides the registered
  tooltip. The text is a script's `SetTooltipText`, else the current state's, else the layout's (the
  order of `GetTooltipText`). Shared by the front end and the campaign HUD (`UiScriptHost::hover`,
  `__ntw_tooltip` in ui_prelude.lua). The delay before it shows is UNKNOWN (shown at once,
  PROVISIONAL).
- **Root script (CONFIRMED, front-end root.lua):** `SetTooltipText` creates the "Tooltip" template
  once (`SetTooltip("Tooltip")` → `CreateComponentFromTemplate`, `RegisterTooltipObject`,
  `Utilities.PositionTooltip`), then calls the template's `SetText(owner, text)` and adopts it.
- **template.tooltip.lua (CONFIRMED by disassembly):**
  - It splits the text at `||` and shows one part at a time, each for max(2000, length x 50) ms
    (its `Update` on OnUpdatePulse). It fires `TriggerTooltipAdvice` after a full cycle.
  - `SetText` does SetStateText, Resize(200, 200), SetState("NewState"). Its `InitState` then
    resizes the box to the text extent + 16.
- **SetState always calls InitState (CONFIRMED):** `0x01035B30` makes the named state current and
  calls InitState (`0x0102DF60`) without comparing with the old state. Our `SetState` now does the
  same, with a nesting guard.
- **The registered tooltip object is drawn after everything else (CONFIRMED):** the component draw
  `0x01027D20` skips it in the tree and draws it last. Ours is the root's last child, so it is drawn
  last too.
- **Resized docked children (INFERRED):** a child a script resized keeps its own dock point's
  distance to the parent's. This is how the template's edges `t` / `b` / `l` / `r` (docks 2 / 8 / 4
  / 6) follow the box.
- **Required modules:** modules such as Utilities.lua see a global `Component` bound to the root's
  environment in every host (it was campaign-only).

## Credits (2026-10-04, save-compat worker on 0-E; `ntw_script::ui::credits`)
- **`FrontEnd.BuildCredits(parent)`** (`0x0046B100`, line builder `0x004635B0`; CONFIRMED unless
  tagged):
  - It reads `text/credits.xml` (UTF-8, CR line ends): `<credits>` → 21 `<page>` → `<line gap
    fontsize colour>` with its own text and/or `<left|right indent fontsize>` parts.
  - Per page, a container goes under the parent (INFERRED: a plain component; the template library
    has no `container`). Per part, a `string` library template goes at the running y. The line
    height is the tallest part's, plus `gap`. The container is sized to the page.
  - It returns `{ {Page, Height, WordCount (characters), Delay (page attribute, 0 without)} }`.
  - Fonts: `fontsize` → font id through `0x0144F030` (12, 14, 16, 18, 22, 24, 38 → ids 12..18).
    We use Frontend at that size (INFERRED: 38 ships only as `frontend_38.cuf`).
  - Colours: `colour` RRGGBB with alpha 0xFF, else white (the builder starts from -1).
  - Placement: the line's own text is centred; `left` / `right` are aligned to their side with
    `indent` (our reading of the garbled alignment code, PROVISIONAL).
- **credits.lua (CONFIRMED by disassembly):** shows one page at a time, centred in `credits_list`,
  each for 4000 ms. It fades the text out over 3000 ms (`PropagateTextAlpha`), then goes to the next
  page and back to the menu after the last.
- `EnableCreditScreenMusic(on)`: accepted, no music switch yet (PLACEHOLDER).
- Test `credits_pages_are_built_from_the_xml`.

## Custom battle (Play Battle → Land / Sea / Siege; 2026-10-04, save-compat worker on 0-E)
Code: `ntw_script::ui::battle_setup`; the page scripts are sp_battle1 (type choice), sp_battle2 (map and
settings) and sp_battle3 (players and armies).
- **Done (sp_battle2 opens and fills without UNKNOWN calls):**
  - `WindLevelOptionsString` (`0x0047A260`): `wind_levels` sorted by their last column, on-screen
    names joined with `|`.
  - `BattleTypeString` (`0x0046A660`).
  - `BattleWeatherAndTimeOfDayOptionsString(battle)` (`0x0046A860` → `0x0045CEE0`): from the
    battle's sky types (`battles_to_battle_sky_types_junctions` → `battle_sky_types`, schema
    `ssssbsss`).
    - weathers: `{Data = key, Value = name}`;
    - times: `{Data = 0..3, Value = name}`, where the index is the position in morning, midday,
      afternoon, evening, night. Night is never offered.
    - `template.dropdown_menu.lua` SetOptions takes a `|` string or such `{Data, Value}` tables.
  - `DefaultPrefsForBattle(file)` (`0x0046CF60`): battle `.xml` only, giving wind, time_of_day and
    time_limit.
  - `LoadBattleSetup`: see "Setup files" below.
  - Map `Teams` (`0x0045B960`, two `{Players}` from record +0x4C / +0x50). PROVISIONAL source: the
    most deployment areas per alliance in the preset's `deployment_areas.xml`.
- **sp_battle3 (players and armies), done:**
  - `ArmyFundsForSize` (CONFIRMED tables: land 5000 / 10000 / 14000, sea 5000 / 14000 / 24000).
  - `UnitScaleFactor` (0.25 / 0.5 / 0.75 / 1.0; preference INFERRED `gfx_unit_scale`).
  - `MaxUnitsFromUnitScaleFactor` (20 and 6 / 8 / 10 / 20).
  - `BuildCpuName` ("CPU %d").
  - `FactionListForBattles` / front-end `FactionDetails` (`0x0046EB10` / `0x0046E950`,
    `0x0045C010`). Land / sea lists from `factions` columns #10 / #11 (INFERRED). The era's
    alternative flag is PROVISIONAL.
  - `RetrieveArmyPresets` (`0x004765F0`, filler `0x0045CB50`; code `ntw_script::ui::army_setup`):
    `battle_type_setup_limits` → `battle_type_faction_presets` → `battle_type_unit_to_faction_presets`,
    cut to the unit limit, then experience raised round-robin within the funds (up to 9).
  - `RecruitableUnits` (`0x00475B00`; argument order from the exe's pops), `MPExperienceTables`,
    XP-adjusted costs (`0x00ED49A0`), the unit details fields (`0x00E02260`).
  - **Starting the battle** (this round): the prelude's setup object keeps the teams that
    sp_battle3 hands to `SetDetails`; `FrontEnd.StartBattle(setup, players, prefs)` turns them and
    the players list into `UiRequest::StartCustomBattle { battle, map, armies }` (each army:
    alliance, army index, faction, human, units with experience). The game
    (`napoleon::frontend`) loads the battle's map and starts the battle with
    `BattleStart::custom`; `battle::setup::custom_armies` builds every army with its units'
    experience, the human army is the player's, the general's unit commands, and each army is
    deployed in its alliance's deployment area of the map's 1v1 setup (the original's default
    deployment templates, as for the test armies).
    - PROVISIONAL: units have their full men (the unit size option is not applied yet); a 2v2
      army uses its alliance's area of the same index (else the first); no technologies.
    - Tests: `custom_battle_start_requests_both_armies` (frontend_ui) and
      `custom_battle_armies_deploy_in_their_areas` (napoleon). Checked in the game:
      `--ui-click single_player,sp_battle,button_classic_battle,button_host,button_ok` opens the
      deployment stage on `nap_mp_amazon`, Austria vs Denmark, 9 units each.
  - **Setup files** (this round; code `ntw_script::ui::army_file` (formats) and `army_setup`):
    - `.army_setup` (write class `army_setup`, folder `army_setups`): layout CONFIRMED from the
      writer `0x0048CFC0` (version 3; Era, ArmySize, Faction, IsHuman, Name, the cards (ships
      first) {Key, Experience, IsGeneral / IsAdmiral, ShipName}, the limits {Max, Actual, Tag}).
      The reader is the writer's mirror (INFERRED; the exe's reader was not decompiled).
    - `.battle_preferences` (write class `battle_prefs`): layout CONFIRMED by the writer
      `0x0048D2C0`, the reader `0x00455F00` and the original's `.mp_default.battle_preferences`,
      which reads and writes back byte-exact (test `battle_prefs_layout_matches_the_original_sample`
      reads it from the user's folder when it is there, read only). Magic 0xBA, version 8; the
      settings, the map record and two teams {Players, army setup records}.
    - `SaveArmySetup(setup, path, overwrite)` `0x00477F50` / `SaveBattleSetup(prefs, path,
      overwrite)` `0x00478100`: CONFIRMED results (without overwrite: `true` if the file exists,
      else `false, success`; with overwrite: `success`). Written only in NapoleonRust's own user
      folder. PROVISIONAL: the exe also hides the `.sp_default` / `.mp_default` files.
    - `LoadArmySetup(path)` `0x004708A0` (`0x00461790`): the setup table with each card's unit
      details, Cost, Cap, TotalCost, TotalCards, IsHuman, Name, limits; nil if it does not open.
      PROVISIONAL: the battle's category mask (`+0x94`) is not applied.
    - `LoadBattleSetup(path)` `0x004709F0`: one flat table (the settings, `unit_scale` from the
      game's preference, `Armies` {[team] = setups}, `map` {File, Name, Type, TotalPlayers,
      IsNaval, Image, Description, Map, Key, IsHistoric, Teams}). INFERRED flat: sp_battle3.lua
      reads `allowed_funds`, `era` and `map` from it ("Preferences" is the exe's name for its
      reference, like "unit" / "preset" / "army").
    - `EnumerateArmySetups(dir, pattern, era, funds, unit_limit, factions)` `0x0046D410` (filter
      `0x0045C970`, CONFIRMED): same era and army size, at most `unit_limit` cards, a listed
      faction; the default files left out.
    - `ValidateArmySetup(setup, is_naval, era)` `0x00479690` → setup, cost[, error]: the units of
      the battle's kind are kept when the record exists, the era allows them, fewer than 20 were
      kept, their MP cost is above 0 and the faction fields them; errors `invalid_units` (the
      other kind of units) / `no_valid_units` (INFERRED message keys: the exe takes them by index
      `0x15D` / `0x15C`). PROVISIONAL: the category mask is not tested.
    - `DirectoryUtils.EnumerateDirectory(dir, pattern)`: `{FileName, Path, Date, DateString}`
      (the requester's reads, CONFIRMED); a missing folder lists nothing (INFERRED).
    - Host: a destroyed component's script stays callable until the end of the frame
      (INFERRED: file_requesters.lua destroys the requester, then asks it for `PathName`).
    - Tests: `army_setups_save_load_validate_and_list`,
      `battle_settings_are_kept_in_the_default_preferences_file`,
      `saved_battle_setup_loads_from_the_requester` (Load Battle: the requester lists the file,
      choosing it opens the armies page, and starting fights the loaded armies).
  - `GenerateShipName` `0x00470430`: PLACEHOLDER "" (naval custom battles are not done).
  - `MPLocalPlayerId` / `MPAvatar`: PLACEHOLDER offline stubs.
- **Next:**
  1. Text entry in our UI host (the requesters' file name field cannot be typed in yet, so
     Save army / Save battle from the GUI need it; Load works by choosing a listed file).
  2. The unit size option in our battle, the category mask, naval custom battles (ships are
     not in our battle yet), `GenerateShipName`.

## Not done
The tooltip delay; UI sounds, list clipping/scrolling, the credits music,
multiplayer pages, text entry, custom battle leftovers (see "Custom battle" Next), Bink movies, the original's text layout rules.

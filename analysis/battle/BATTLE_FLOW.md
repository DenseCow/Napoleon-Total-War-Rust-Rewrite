# Battle flow: historical armies, deployment, battle HUD, victory/defeat, music

Worker: battle-flow (`work/battle-flow`). Code: `ntw_formats::battle_spec`, `ntw_sim::battle::victory`,
`ntw_script::ui::battle` (+ `battle_prelude.lua`), `napoleon::battle::{setup, hud, input}`.
Tags: CONFIRMED (file bytes / exe / scripts), INFERRED, UNKNOWN; stand-ins PLACEHOLDER / PROVISIONAL.

Harnesses: `--battle-key <KEY>` (e.g. NHB_Arcole; implies `--battle`), `--skip-deployment`,
`--battle-ui-click id,...` (HUD clicks, one per second; `end:win` / `end:lose` decide the battle, `wait`),
`cargo run -p ntw_script --example battle_hud_probe -- [--phase conflict|finished] [--call Fn] [--tree] [--lua code] [--lua-end code] [--lua-final code] ids`
(the HUD headless), `cargo run -p ntw_formats --example loc_find -- <text>`.
Example: `napoleon --battle-key NHB_Arcole --battle-ui-click button_battle_start,end:win,end_battle_button,button_dismiss_results,wait --screenshot r.png`.

## Where I am / what's next (updated with each push)
- Done (all five tasks, first versions): historical armies from the battle files (menu + `--battle-key`); deployment
  phase with the original Start Battle panel; the original battle HUD (cards, orders bar, speed controls, timer,
  kill ratio bar) wired to the model; victory/defeat (rout/kill, time limit, timeout winner) with the original
  victory-options popup, battle summary popup and post-battle results screen, then back to the front end; music
  per phase with the player's subculture. Main merged at ff033bf (AI hook kept: it acts only in the conflict phase).
- Next: see §6 "Not done".

## 1. Historical battle files (CONFIRMED structure, all 29 files)
A `battles` record's specification column names an `.xml` (`Napoleon_Historical_Battles/Arcole/Arcole_Battle.xml`,
`MP_Historical_Battles/...`, `Wellington_Historical_Battles/...`); preset-only records name a terrain folder and
carry no armies. Next to each xml sits a `.battle_script` (the battle's Lua, not run yet).
Element tree and fields: see the module docs of `ntw_formats::battle_spec`. Highlights:
- `alliance id` → `army`* / `reinforcement_army approach_angle`*, `victory_condition { kill_or_rout_enemy |
  sink_or_surrender_enemy }`, `rout_position x y`.
- `army`: `faction`, `time_period`, `deployment_area` (same form as the preset's), camera start/target, AI hints,
  `unit unit_category num_soldiers [script_name]` with `unit_type type`, `position x y`, `orientation radians`,
  `width metres`, `unit_experience level`, optional `general`, `unit_capabilities`, `manually_deployed`.
- `battle_description`: `battle_script`, `time_of_day`, `type`, `duration` (1800–3000), `timeout_winning_alliance_index`.
- `battle_map_definition/name` (the preset), `playable_area dimension centre_x centre_y`, `Climate`, `Season`, `weather`.
- `skip_deployment` exists only commented out in the shipped files.
- Text encoding: declared UTF-8, but some names are Latin-1 (`Blücher`): read lossily.

| Rule | Evidence | Tag |
|---|---|---|
| Positions are map metres in the preset's frame | units stand inside the presets' deployment areas, Arcole's camera is behind the French line | INFERRED |
| Orientation 0 = facing +y, π/2 = +x (as `deployment_areas.xml`); sim facing = π/2 − θ | install test `armies_face_each_other`: mean cosine to the enemy 0.82 over 20 armies vs 0.04 for the other convention | INFERRED (strong) |
| Camera positions are (x, height, map y) | Arcole camera z −746.9 just behind the French at y ≈ −650 | INFERRED |
| `non_playable` before an `army` applies to that army; the player = alliance 0's first playable army | Austerlitz/Friedland layouts; every SP file puts the player first | PROVISIONAL |
| `duration` is seconds of battle time | values 1800–3000 | INFERRED |
| Frontage → formation: files = width / close file spacing, ranks = men / files | `width metres` per unit differs from the DB default | INFERRED |
| `num_soldiers` used as is | unit-size preference scaling UNKNOWN | PROVISIONAL |
| Reinforcement armies are not placed | arrival timing UNKNOWN (battle scripts?) | PROVISIONAL |
| Naval battles (Nile, Trafalgar) fall back to the test armies | no ships yet | PLACEHOLDER |

Unit types are matched case-insensitively to `units`; all 26 installed land battle files build with every unit
found (test `historical_battles_build_with_all_units`). The DB record `NHMPB_Austerlitz_3v3` names a file that is
not installed (the menu shows it locked).

## Open questions for Ghidra (battle files)
- How the exe picks the human army in a battle file (alliance/army index, `non_playable` scope).
- Unit-size scaling of `num_soldiers`; when reinforcement armies arrive (`approach_angle` use).
- Whether the `duration` timer runs in battle seconds or real seconds.

## 2. Deployment (task 2)
| Rule | Evidence | Tag |
|---|---|---|
| A battle starts in deployment unless the file has `skip_deployment` (only commented out in the shipped files) | `BattleUI.HasEnteredDeployment` / `IsConflict` / `IsDeploymentOrConflict` bindings; `max_deployment_time = 60` in root.lua (MP) | names CONFIRMED, SP flow INFERRED |
| Model time does not run in deployment; the AI gives no orders | none | INFERRED |
| The engine opens the deployment panel with root.lua `ShowDeploymentPopup()` then `SetDeploymentPopupAsDeploymentStart()` (PanelManager panel `finish_deployment` = layout `deployment_end`, SP part `deployment_end_sp` with `button_battle_start`) | root.lua, PanelManager.lua registry, deployment_end scripts | names CONFIRMED, call moment INFERRED |
| Start Battle → `BattleUI.InformOfDeploymentFinished()`; the engine then calls `ClearDeploymentPanels()` | button_battle_start.lua / root.lua | INFERRED |
| Placing units: select (card or 3D click), right-click inside the player's deployment area puts the unit there at once, keeping its facing | none | PROVISIONAL (the original drags a formation; rules UNKNOWN) |
| Deployment area = the player army's `deployment_area` (battle file), else the preset's 1v1 area; a rectangle `width` across, `height` deep, turned by `orientation` (`setup::area_contains`) | fields CONFIRMED | reading INFERRED |

## 3. Battle HUD (task 3)
`data/ui/battle ui/layout` is loaded in a `UiScriptHost` with the battle engine side installed
(`ntw_script::ui::battle::install`), drawn by `frontend::render::spawn_world` over the 3D view (window 1280x960,
the layouts' authored size). The game writes `BattleHudFacts` every frame and applies `BattleUiRequest`s.

Engine functions given to the scripts (names CONFIRMED in the scripts / binding table `0x59EB40`; behaviour ours):
`BattleDetails() → {IsMultiplayer, NavalBattle, TotalTime (s, -1 = none), FlagPath, Name}`; `ElapsedBattleTime`, `Time`,
`TickPeriod` (speed multiplier); `Pause/Slow/Play/Fwd/Ffwd/CycleBattleSpeed` → model speed;
`Current_Selection_*` / formation / fire names → orders; `SelectAll{Infantry,Cavalry,Artillery}`;
`UnitListForBattleIds → {Land={}, Naval={}}` (no floating unit ids yet); `GetHealthStatus` (player share of men alive,
INFERRED); `LocalisationString`; `PostBattleInfo` (see §4); `UICardManager` (Lua class: AddCard, RemoveCard, Selected,
ManageSelection, SelectCardList, DeselectAll, PositionCards, ...); `MPAvatar` (no Steam: no-op);
`Component.LockPriority` returns the previous lock (PanelManager logs "Was: ..").

Cards: the engine calls review_DY.lua `CreateCards(list, state)` with `{CardID="card_<unit id>", Portrait}` per player
unit (Portrait = `data/ui/units/icons/<faction unit_icon_path>_<key>_icon`, the script adds `.tga`; CONFIRMED file
names, prefix rule INFERRED), then each card gets its own state table through `SetInitialState(state, card)` and every
frame `Update(info)` with `{Men, NumMen, Guns, NumGuns, IsArtillery, HasAmmo, AmmoRemainingAsPercent, Experience,
RoutingState, WaveringState, WalkingState, RunningState, FiringState, MeleeState, UnderFireState, Inactive, ...}`
(field names CONFIRMED in template.battleunitcard.lua; the per-card state table is INFERRED). Selected cards get the
template's `Selected` state (PROVISIONAL). A click on a card selects its unit (shift adds; the model holds one
selected unit, PROVISIONAL).

Order buttons: the engine sets their states with root.lua `SetOrderButtonState(id, state)` (Inactive / Unselected /
Selected, CONFIRMED via land_hud_orders.lua); which buttons are enabled for which units is PROVISIONAL
(halt, run, melee, fire at will, withdraw, unit controls for any selection; groups, group formations and special
abilities inactive). Wired orders: halt (`order_halt`), run/walk (`LandUnit::running`), fire at will
(`order_fire_at_will`), move forwards/backwards (10 m) and rotate (15 degrees) (step sizes PROVISIONAL). Not done: melee
mode, withdraw, formations, groups, abilities, shot types. Clicks send `audio::UiSound` (component id).

Generic UI host changes needed by the battle scripts (all INFERRED rules; front-end tests still green):
relative script overrides (`../layout_scripts/battle_hud.lua`) are resolved; `<folder>/<id>.lua` runs for components
of a folder's main `layout` file (`root.luac`, `play.luac`, `pause.luac`, ...); a template's single child is the
created component (`BattleUnitCard`); scripts are found by the layout id before CreateFromLayout's rename
(`deployment_end` opened as `finish_deployment`); the first layout is the top root before its scripts run (sizes known
at load); `Find` falls back to siblings' subtrees (review_DY's `tab_group`); `StealShortcutKey(false)` releases keys;
`set_pages_fill_screen(false)` for the battle (its panels dock themselves). Battle-only (prelude): the root's
InitState runs after its children's; image lists `{id:n}path` are applied; a full-screen multi-panel wrapper
(land_battle_orders) is placed at the screen origin, only its first panel shown, and calls on it reach that panel.

## 4. Victory, defeat and results (task 4)
| Rule | Evidence | Tag |
|---|---|---|
| `kill_or_rout_enemy`: a side wins when no enemy unit is still fighting (routing, shattered or destroyed) | condition name CONFIRMED (every land file) | test INFERRED |
| Time limit `duration` seconds of battle time → `timeout_winning_alliance_index` wins | fields CONFIRMED | unit INFERRED |
| Decided → root.lua `ShowSinglePlayerEndPhasePopup()` (SP_victory_options: End battle / Continue) | root.lua, sp_victory_options.lua | call moment INFERRED |
| End battle (`PostBattleDismissEndBattle`) → `ShowBattleSummaryPopup(text)` (in_battle_results_popup) | root.lua | INFERRED |
| Summary text: `battle_script_strings_string_CreativeAssembly.HB_<name>_Battle_Won/Lost` (from `battle_script`), else `random_localisation_strings_string_battle_{victory,defeat}_minor_{decisive,close}` / `battle_draw` | loc keys CONFIRMED | use INFERRED, grading PROVISIONAL (decisive = winner lost < 25 %) |
| Close → root.lua `DismissBattleResult` → `PostBattleInfo()` → `ShowPostBattleResultsPopup(info)` (layout `mp_postbattle`: Battle Results + Unit Statistics tabs) | root.lua, mp_postbattle.lua | script flow CONFIRMED |
| `PostBattleInfo` fields: `is_from_campaign, is_drop_in_battle, is_multiplayer, is_draw, player_wins, winning_teams[], losing_teams[]` (team: `display_name, men_deployed, losses, enemy_killed, flags_path, portrait, skill, skill_change, human_player, local_player, mp_index, achievements`), `player_unit_statistics[]` (`name, CustomName, deployed, lost, kills, start_xp, end_xp, icon_name`) | field reads CONFIRMED | values INFERRED |
| Exit on the results → root.lua `ClosePopup` → `InformOfBattleSummaryDismiss()` → back to the front end | root.lua | INFERRED; campaign return not wired (PROVISIONAL) |
| Continue → keep fighting, no further checks | sp_victory_options.lua | INFERRED |

## 5. Music (task 5)
`audio::SetMusic` from `battle::hud` with the player's faction subculture (`factions` #2): `music_land_deployment`
on entering deployment, `music_land_battle` at Start Battle, `music_land_battle_results` when the battle is decided
(state names from `sound_bank_music_states`; checked: Arcole plays `Music_battle_deployment_western_3`,
`Music_battle_western_2`, `Music_battle_results_western`). PROVISIONAL timing: the original waits
`LAND_BATTLE_TIME_UNTIL_DEPLOYMENT_MUSIC_PLAYS` and switches to battle music when
`PERCENTAGE_OF_LAND_UNITS_FIGHTING_FOR_BATTLE_MUSIC_TO_PLAY` of units fight.

## 6. Not done / known issues
- Reinforcement armies (Waterloo's Prussians) are not placed; allied non-player armies on the player's side
  (Friedland) have no AI (the AI hook plays side 1 only).
- Naval historical battles fall back to the test armies (no ships).
- Results screen: the losing team's row is hidden behind the winner's list background (draw order / clipping);
  the translucent backdrop is not drawn.
- Radar (minimap) hidden; floating unit ids (`lb_unit_id`) not created; escape menu untested; tooltips; card
  drag/grouping; multi-selection; `UIComponent:DockingPoint`, shader effects on cards (UNKNOWN stubs).
- Flags: `FlagPath` = `data/` + the factions flag folder; the republic factions (ita_/egy_french_republic) show the
  checkered placeholder (their folder rule UNKNOWN).
- Return to the campaign via `CampaignModel::pending_battle` / `apply_battle_result`.

## Open questions for Ghidra (battle UI registrar 0x59EB40 and the battle mode)
- When the engine calls root.lua's ShowDeploymentPopup / SetDeploymentPopupAsDeploymentStart / ClearDeploymentPanels /
  ShowSinglePlayerEndPhasePopup / ShowBattleSummaryPopup / ShowPostBattleResultsPopup, and with which text.
- The order buttons' enable rules per unit type (SetOrderButtonState callers) and the unit-card info table
  (`SquadInfoByPointer`, card manager `PositionCards`, `Selected`).
- `CreateFromLayout` on a file with several top children (land_battle_orders): what is created and where it is placed;
  InitState order (children before parent?); `Find` search scope; `DockingPoint` return values.
- Victory: the exact "kill or rout" test (routing units leaving the map, rallying), the time unit of `duration`, the
  victory grading (close/decisive/heroic/pyrrhic) thresholds.
- Deployment placement rules (formation drag, spacing) and how the human army is chosen in a battle file.
